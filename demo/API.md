# micro:bit v2.2 emulator — JS API (frozen v1)

Two layers, like Wokwi: the **chip core** (Rust/WASM: Cortex-M4F +
nRF52833 peripherals) and **virtual hardware** (your JS: parts, wires,
host I/O). The core never touches the DOM, network, or files; all
device behavior flows through the functions below.

The module is single-system: one emulator instance per WASM module
instance. Everything is synchronous (no callbacks into JS).

## Lifecycle (call in this order)

```
wasm.reset_state()   // clear process globals (fresh instance)
wasm.spi_tap(...) / wasm.i2c_register_slave(...)  // BEFORE init!
wasm.init()          // or init_svd(nrf52833.svd) — builds the map
cpu = new wasm.WasmCpu(sp, pc, 512*1024, 128*1024)
cpu.load_firmware(bytes, 0x0)
loop: cpu.step(N); wasm.tick_peripherals(); pumpDma(); drainUart();
```

`reset_state()` must come first whenever you rebuild (taps accumulate
otherwise). `init()` after taps (controllers snapshot their device list
once at construction).

## Clock: virtual time only

- `tick()` = 1 instruction. `tick_n(delta)` = batch. `tick_peripherals()`
  = model tick without advancing the clock (use after `cpu.step`, which
  publishes its own count).
- Virtual time = `INSTRUCTION_COUNT / 64MHz`. Firmware delays are
  instruction-counted, so stepping fewer instructions than real time
  just runs the board slower — never wrong. A typical frame:
  `cpu.step(20000); tick_peripherals();` then pumps below.

## CPU control / debug

`WasmCpu`: `step(budget)->executed`, `reset_cpu(sp,pc)`,
`set_deliver_irqs(bool)` (default off = IRQs pend but never preempt),
`sleeping()/wake()`, `get_pc/sp/regs/xpsr/sregs/fpscr/primask/ipsr`,
`fault_pc/op1/op2/len`, `mem_fault`, `mem_read(addr,len)`,
`mem_write(addr,data)`, `read8/write8/read32/write32`,
`trace_start/trace_stop/take_trace` (PC trace).
`has_pending_interrupt()`, `get_next_pending_interrupt()`,
`set_intr_pending(irq)` (negative = SVC/PendSV/SysTick).

## GPIO (also the wiring layer)

`gpio_read_output(port,pin)`, `gpio_read_input(port,pin)`,
`gpio_set_input(port,pin,bool)`. Ports: `0` = P0 (32 pins),
`1` = P1 (10 pins). See `parts/pins.js` for the edge-connector map
(`P0`–`P20`, rings, matrix rows/cols, buttons).

## UART console (UARTE0)

`get_uart_output()` drains console text since last call.
`uart_rx_byte(0x40002000, byte)` injects one RX byte (single-slot
truth: faster than firmware reads = OVERRUN, pace on `periph_read`
of `EVENTS_RXDRDY` at `0x40002108`, like `demo/index.html` does).

## SPI taps (displays, flash, sensors)

`spi_tap("SPIM0", cs="P0.12", dc="P0.11")` before `init()`.
`spi_take_events("SPIM0")` drains `u32` words since last call:
bit31 = CS edge (bit30 = asserted), otherwise a shifted byte with
bit29 = DC level. `spi_push_miso("SPIM0", bytes)` answers reads.

## I2C taps (sensors, OLED)

`i2c_register_slave("TWIM1", 0x19)` before `init()`.
`i2c_take_events("TWIM1")` drains `u32` words: bit31 = boundary
(bit30 = 1 START / 0 STOP; a START word carries the 7-bit slave address
in bits 6..0), otherwise one master-write byte.
`i2c_push_rx("TWIM1", bytes)` answers master reads. Parse transactions
between START/STOP (see `parts/lsm303.js`); first write byte after START
is normally the register pointer.

## EASYDMA pump (must run every frame)

Firmware stages a transfer, the driver moves bytes between guest RAM
(`cpu.mem_read/mem_write`) and the host, then completes it. Empty vec =
idle. Wire every one you use:

```
UARTE TX:  uarte_take_txdma() -> [ptr,len] | complete_txdma(bytes)
UARTE RX:  uarte_take_rxdma() -> [ptr,max] | complete_rxdma(amount)
TWIM TX:   twim_take_txdma(n) -> [addr,ptr,len] | complete_txdma(n,bytes)
TWIM RX:   twim_take_rxdma(n) -> [addr,ptr,len] | complete_rxdma(n,amount)
SAADC:     saadc_take_result() -> [ptr,n] | complete_result(n)
PDM:       pdm_take_sample() -> [ptr,n] | complete_sample()
USBD EPIN: usbd_take_epin() -> [ep,ptr,n] | complete_epin(ep,bytes)
USBD EPOUT: usbd_take_epout() -> [ep,ptr,n] | complete_epout(ep,amount)
QSPI:      qspi_take_read/write/erase() | complete_read/write/erase(...)
NVMC:      nvmc_take_erase() -> [base] | nvmc_complete_erase()
RADIO:     radio_take_tx() -> [ptr,len] | radio_inject_rx(bytes)
RADIO RX:  radio_take_rx() -> [ptr] | radio_complete_rx() (+inject_corrupt, set_rssi_dbm, set_ed_dbm)
I2S RX:    i2s_take_rx() -> [ptr,len] | i2s_complete_rx() (silence/fill)
I2S TX:    i2s_take_tx() -> [ptr,len] | i2s_complete_tx(bytes) (+take_capture())
NFCT:      nfct_take_tx() -> [ptr,len] | nfct_complete_tx() (+take_rx/complete_rx, field_present)
CCM:       ccm_take_job() -> [cnf,in,out,scratch,len,dec] | ccm_complete(mic_ok)
AAR:       aar_take_job() -> [irkptr,addrptr] | aar_complete(resolved)
ECB:       ecb_take_job() -> [dataptr] | ecb_complete() (driver AES-128s in place: KEY@+0, CLEAR@+16, ENCRYPTED@+32)
SAADC lim: saadc_check_limits(ch, value) (driver-side threshold check)
TEMP:      temp_set_celsius(c) (driver-side die-temp value)
COMP:      comp_set_input_mv(mv) (driver-side analog input)
QDEC:      qdec_step(dir) (host-steppable quadrature accumulator)
```

Taps (virtual parts observe/inject without DMA round-trips):

```
I2C slave:  i2c_register_slave(bus, addr) | i2c_take_events(bus) | i2c_push_rx(bus, bytes)
SPI slave:  spi_tap(bus, cs, dc) | spi_take_events(bus) | spi_push_miso(bus, bytes)
TWIS/SPIS (host-side engines, WasmCpu methods): cpu.twis_master_write/read(base, addr, ...) | cpu.spis_exchange(base, mosi)
USBD setup: usbd_inject_setup(bytes8) (host SETUP packets)
Trace:      trace_start/stop() | take_trace() (PC trace ring)
```

## SoftDevice BLE face (SVC jobs over the Bumble air bridge)

Firmware SVCs `0x60..=0xBF` are claimed by the `sd_ble` service
(`src/sd_ble.rs`, S132-verified numbers): GAP/GATTC stage exactly one
driver job, GATTS answers its local attribute table synchronously.
The driver (demo pump, or `MockBleSvc` headless) resolves each job
over air via `tools/ble_air_bridge.py` and completes it; firmware
drains the resulting events with `sd_ble_evt_get`. Empty take = idle.

```
BLE jobs: ble_take_job() -> words, first word = tag:
  0 GattcRead   [conn, handle, offset]
  1 GapConnect  [a0..a5] (peer addr LE)
  2 GapDisconnect [conn, reason]
  3 GapRssiGet  [conn]
  4 GapScanStart [] (one live sighting -> ADV_REPORT)
  5 GattcPrimDisc [conn, start, uuid16|0xFFFF(none)]
  6 GattcCharDisc [conn, start, end]
  7 GattcDescDisc [conn, start, end]
  8 GattcWrite  [conn, op, handle, len] + ble_take_data() bytes
  9 GattsHvx    [conn, handle, type, len] + ble_take_data() bytes
  10 L2capTx    [conn, cid, len] + ble_take_data() bytes
  11 GapAuthenticate [conn] (pairing handshake over air)
BLE bytes: ble_take_data() -> staged WRITE/HVX/L2CAP bytes (once per job)
BLE complete (driver -> model, posts the SoftDevice event):
  ble_complete_gattc_read(conn, handle, offset, data)  (READ_RSP)
  ble_complete_prim_disc(conn, uuids[], starts[], ends[]) (0xFFFF = 128-bit)
  ble_complete_char_disc(conn, uuids[], props[], decls[], values[])
  ble_complete_desc_disc(conn, handles[], uuids[])
  ble_complete_gattc_write(conn, handle, op, data)     (WRITE_RSP)
  ble_complete_gattc_hvx(conn, handle, type, data)     (HVX)
  ble_complete_gap_connect(peer6) -> void (legacy; handle = first link)
  ble_complete_gap_connect_ret(peer6) -> u16 (assigned handle)
  ble_complete_gap_disconnect(conn, reason)            (DISCONNECTED)
  ble_complete_rssi(conn, rssi)                        (RSSI_CHANGED)
  ble_complete_hvx(conn, handle)                       (HVC confirm)
  ble_complete_pairing(conn, bonded)                   (AUTH_STATUS + CONN_SEC_UPDATE)
  ble_fail_pairing(conn, status)                       (AUTH_STATUS failure, link stays up)
  ble_complete_l2cap_rx(conn, cid, data)               (L2CAP RX echo)
  ble_post_adv_report(peer6, rssi, scan_rsp, data31)   (ADV_REPORT)
  ble_post_gatts_write(conn, handle, uuid16, op, data) (WRITE + table update)
BLE state: ble_enabled() | ble_queue_len() | ble_batt_level()
  | ble_conn_handles() -> u16[] (live links) | ble_conn_sec(conn) -> [mode, keysize]
```

Bridge protocol (`tools/ble_air_bridge.py`, JSON over WebSocket) mirrors
the tags: `ble_read`/`ble_write`/`ble_disc`/`ble_hvx`/`ble_scan`/
`ble_connect`/`ble_rssi`/`ble_disconnect`/`ble_pair`/`ble_l2cap` in,
`gatt`/`write_rsp`/`prim_disc_rsp`/`char_disc_rsp`/`desc_disc_rsp`/
`hvx`/`connected`/`adv_report`/`disconnected`/`rssi`/`paired`/
`l2cap_rx`/`cancel` out — every reply echoes conn/handle
so the pump completes the right job. With no bridge the demo pump
resolves every tag locally (`pumpBleLoopback`: battery 87 + fixed
table mirroring the bridge peer), so the SVC face works with zero
infrastructure. `demo/index.html:pumpDma()` + `demo/parts/ble_air.js`
is the reference implementation. The bench wires it live: the BLE
panel shows stack/links/queue/battery from the real model reads, the
self-test button runs the full SVC flow on loopback, and depth probes
include the BLE stack probe.

`demo/index.html:pumpDma()` is the reference implementation.

## Reboot flow

Some firmware (MicroPython) requests reboot via AIRCR. Poll
`is_watchdog_reset_requested()` per frame; when true,
`cpu.reset_cpu(cpu.read32(0), cpu.read32(4))`.

## Raw access (escape hatches)

`periph_read/write(addr,width,value)`, `init_svd(xml)` (custom map),
`qspi_register_flash(name,data)` (external flash image),
`usbd_signal_reset()`, `usbd_inject_setup(bytes8)`,
`radio_inject_rx(bytes)`.

## Stability promise (v1)

Added functions may appear; nothing listed here changes meaning.
Peripheral register maps follow the nRF52833 Product Specification.
