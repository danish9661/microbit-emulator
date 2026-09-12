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
RADIO RX:  radio_take_rx() -> [ptr] | radio_complete_rx() (+inject_corrupt, set_rssi_dbm)
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
