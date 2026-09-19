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
  link-budget air: radio_txpower_dbm(code) -> dBm | radio_air_rssi_dbm(code, loss_db) -> dBm
  (pure fns, one honest number) | radio_inject_rx_lossy(bytes, loss_db) | radio_inject_rx_to_lossy(idx, bytes, loss_db)
  | radio_complete_rx_with_path_loss(loss_db) (RX stamps TX-minus-loss into the RSSI latch)
I2S RX:    i2s_take_rx() -> [ptr,len] | i2s_complete_rx() (silence/fill)
I2S TX:    i2s_take_tx() -> [ptr,len] | i2s_complete_tx(bytes) (+take_capture())
NFCT:      nfct_take_tx() -> [ptr,len] | nfct_complete_tx() (+take_rx/complete_rx, field_present; UICR.NFCPINS gates antenna vs GPIO)
SoC evt:   sd_evt_queue_len() (phase-1 flash events; firmware polls via SVC 82)
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
  12 GattcRelDisc [conn, start, end] (include walk)
  13 GattcAttrInfoDisc [conn, start, end] (table walk)
  14 GattcUuidRead [conn, uuid16|0xFFFF, start, end]
  15 GattcValsRead [conn, count] + handles u16[count] (in take words)
BLE bytes: ble_take_data() -> staged WRITE/HVX/L2CAP bytes (once per job)
Pairing legs (peer-initiated security; no job — the driver posts the
request event and firmware answers with the reply SVC; keys persist
in the bond store across disconnects — hit/miss/delete):
  SEC_PARAMS_REQUEST -> sd_ble_gap_sec_params_reply(conn, status, params, keyset)
    NULL params (or nonzero status) = reject (AUTH_STATUS pair-fail,
    link stays up); non-NULL params = accept needs an outstanding peer
    request (else INVALID_STATE), stages the air handshake; a 58B
    keyset block (LTK/IRK/CSRK/master-id) is COPIED to the bond store
  AUTH_KEY_REQUEST -> sd_ble_gap_auth_key_reply(conn, key_type, key)
    key_type 0 none / 1 passkey (6 ASCII digits, validated) / 2 OOB
    (16 bytes); needs an Accepted/key-entering handshake
  LESC_DHKEY_REQUEST -> sd_ble_gap_lesc_dhkey_reply(conn, dhkey32)
  KEYPRESS_NOTIFY posts KEY_PRESSED (types 0..4, validated)
  SEC_INFO_REQUEST -> sd_ble_gap_sec_info_reply(conn, enc, id, sign)
    all-NULL = no keys (AUTH_STATUS AUTH_REQ fail, link stays up);
    non-NULL enc = keys found (ENCRYPT expected next)
  sd_ble_gap_encrypt(conn, master_id, enc_info) re-encrypts (needs
  EncryptPending/Accepted); sd_ble_gap_lesc_oob_data_get zeroes 32B
  (no crypto — documented); LESC_OOB_DATA_SET is an ack
BLE complete (driver -> model, posts the SoftDevice event):
  ble_complete_gattc_read(conn, handle, offset, data)  (READ_RSP)
  ble_complete_gattc_write(conn, handle, op, data)      (WRITE_RSP, op echo — SIGNED op 3 carries its 12B signature in data; PREP op 4 / EXEC op 5 queue/commit)
  ble_complete_prim_disc(conn, uuids[], starts[], ends[]) (0xFFFF = 128-bit)
  ble_complete_char_disc(conn, uuids[], props[], decls[], values[])
  ble_complete_desc_disc(conn, handles[], uuids[])
  ble_complete_rel_disc(conn, handles[], uuids[], starts[], ends[]) (REL_DISC_RSP)
  ble_complete_attr_info_disc(conn, handles[], uuids[]) (ATTR_INFO_RSP, 16-bit)
  ble_complete_uuid_read(conn, handles[], flat[], lens[]) (UUID_READ_RSP)
  ble_complete_vals_read(conn, data[]) (VALS_READ_RSP, concatenated)
  ble_complete_gattc_hvx(conn, handle, type, data)     (HVX)
  ble_complete_gap_connect(peer6) -> void (legacy; handle = first link)
  ble_complete_gap_connect_ret(peer6) -> u16 (assigned handle)
  ble_complete_gap_disconnect(conn, reason)            (DISCONNECTED)
  ble_complete_rssi(conn, rssi)                        (RSSI_CHANGED)
  ble_complete_hvx(conn, handle)                       (HVC confirm)
  ble_complete_pairing(conn, bonded)                   (AUTH_STATUS + CONN_SEC_UPDATE)
  ble_bond_has_keys(peer6, master_id10) -> bool         (re-encrypt gate)
  ble_bond_read_keys(peer6) -> [ltk16, irk16, csrk16, mid10] (52B or empty)
  ble_store_bond(peer6, ltk16, irk16, csrk16, mid10)    (driver-side insert)
  ble_delete_bond(peer6) -> bool                        (explicit unbond)
  ble_complete_tx_flow(conn)                            (TX token refill + TX_COMPLETE)
  ble_complete_peripheral_connect(peer6) -> u16         (dial-in CONNECTED, PERIPH role)
  ble_complete_conn_param_update(conn)                  (CONN_PARAM_UPDATE)
  ble_fail_pairing(conn, status)                       (AUTH_STATUS failure, link stays up;
                                                        S132 SEC_STATUS codes: 0x00 success,
                                                        0x81 passkey-fail, 0x82 OOB-missing,
                                                        0x83 auth-req, 0x84 confirm, 0x85
                                                        pairing-not-supp — the old code
                                                        spelled this 0x29, an ATT error)
  ble_post_sec_params_request(conn, peer_params5) -> bool  (peer started SMP;
                                                        firmware answers SEC_PARAMS_REPLY)
  ble_post_sec_info_request(conn, peer_addr7, master_id10, req) -> bool
                                                      (peer re-encrypt ask;
                                                       firmware answers SEC_INFO_REPLY)
  ble_post_auth_key_request(conn, key_type) -> bool   (driver needs key;
                                                       firmware answers AUTH_KEY_REPLY)
  ble_post_passkey_display(conn, passkey6, match_request) -> bool
  ble_post_keypress(conn, kp_not) -> bool             (peer keypress -> KEY_PRESSED)
  ble_post_lesc_dhkey_request(conn, oobd_req) -> bool (firmware answers LESC_DHKEY_REPLY)
  ble_post_sec_request(conn, bond, mitm, lesc, keypress) -> bool     (peer SEC_REQUEST)
  ble_post_conn_param_update_request(conn) -> bool                   (peer param ask)
  ble_post_scan_req_report(peer6, rssi) -> bool                      (scanner hit our ADV)
  ble_post_gap_timeout(conn, src) -> bool       (0 adv, 1 sec-req, 2 scan, 3 conn)
  ble_post_gattc_timeout(conn) -> bool          (ATT client timeout)
  ble_post_gatts_timeout(conn) -> bool          (ATT server timeout)
  ble_post_user_mem_request(conn, mem_type) -> bool  (firmware answers USER_MEM_REPLY)
  ble_post_user_mem_release(conn, mem_type) -> bool  (informational)
  ble_post_rw_authorize_request(conn, auth_type, handle, offset, op, data) -> bool
  ble_post_sys_attr_missing(conn) -> bool       (firmware answers SYS_ATTR_SET)
  ble_post_sc_confirm(conn) -> bool             (header only, no reply path)
  ble_tx_power_dbm() -> i8                      (TX_POWER_SET store, S132-legal set)
  ble_adv_state() -> [active, directed, fp, wl] (ADV_START arms, ADV_STOP clears)
  ble_adv_peer_addr() -> peer6                  (directed-ADV target)
  ble_complete_l2cap_rx(conn, cid, data)               (L2CAP RX echo)
  ble_post_adv_report(peer6, rssi, scan_rsp, data31)   (ADV_REPORT)
  ble_post_gatts_write(conn, handle, uuid16, op, data) (WRITE + table update)
BLE state: ble_enabled() | ble_queue_len() | ble_batt_level()
  | ble_conn_handles() -> u16[] (live links) | ble_conn_sec(conn) -> [mode, keysize]
```

Bridge protocol (`tools/ble_air_bridge.py`, JSON over WebSocket) mirrors
the tags: `ble_read`/`ble_write`/`ble_disc`/`ble_uuid_read`/
`ble_vals_read`/`ble_hvx`/`ble_scan`/`ble_connect`/`ble_rssi`/
`ble_disconnect`/`ble_pair`/`ble_l2cap` in, `gatt`/`write_rsp`/
`prim_disc_rsp`/`char_disc_rsp`/`desc_disc_rsp`/`rel_disc_rsp`/
`attr_info_rsp`/`uuid_read_rsp`/`vals_read_rsp`/`hvx`/`connected`/
`adv_report`/`disconnected`/`rssi`/`paired`/`l2cap_rx`/`cancel` out — every reply echoes conn/handle
so the pump completes the right job. Two air peers share one
LocalLink (default battery 87 `PeerBatt` + second battery 64
`PeerHR`, distinct addresses): every job carries an optional
`peer:[6 LE bytes]` selecting the peer (default when absent;
`ble_connect`'s `addr` selects the same way), and `ble_scan`
reports one `adv_report` per peer. `prim_disc_rsp`/`char_disc_rsp`
echo the answering `peer` so the driver can pin reads to their link
(both peers share handle numbers — decl 16/value 17 — so an
unaddressed fallback walk could answer from the wrong peer; fixed
here by addressing every read). `ble_rssi` probes HCI_READ_RSSI on
the live link first (SoftDevice-faithful: the stack samples the
CONNECTION) and falls back to the advertising sighting — LocalLink's
virtual controller answers UNKNOWN_HCI_COMMAND (probed) — with
`src:"conn"|"adv"` tagging the path. Air is serialized: one ATT
burst per peer address (per-peer lock) + one global scan lock, so
back-to-back jobs queue instead of colliding and scan-then-connect
handoffs never interleave a second scan (timeouts under load before
this). With no bridge the demo pump
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
