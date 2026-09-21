# microbitapi.md — micro:bit v2.2 (`microbit-v2-emulator@0.1.0`) API + OpenHW gap spec

Probed from `"board/microbit-v2"` (`demo/pkg/*.d.ts` 154 named exports =
`WasmCpu` class + `initSync` + 152 free fns: 61 `ble_*` + 91 others,
`demo/API.md` frozen v1, `demo/index.html`, `demo/parts/*`, `blinky/`,
`tools/ble_air_bridge.py`, `demo/package.json`). All names dumped live from
the built glue; lifecycle order is `API.md` §Lifecycle, verified in
`demo/index.html` (`initBoard:432`, `bootImage:589`, `bootDirectApp:620`,
`pumpDma:758`, `frame:1104`).

## 1. Package layout

- `demo/package.json`: `microbit-v2-emulator` 0.1.0, MIT, ESM; scripts
  `test:parts/handshake/mpy/js/py/repl/ts/live`, `build:wasm` (wasm-pack web,
  `../nrf52833-periph-wasm → ../demo/pkg`), `bridge` (ble_air_bridge.py).
- Chip: nRF52833 Cortex-M4F, 512 KB flash @0x0, 128 KB RAM @0x20000000
  (`demo/API.md`, `AGENTS.md`). Build: `wasm-pack build … --target web
  --out-dir ../demo/pkg` (wasm ~1.7 MB). `demo/pkg/` = `_bg.wasm + .js +
  .d.ts` (+`_bg.wasm.d.ts`); `demo/parts/pkg-test-handshake/` = second build.
- `blinky/` = 16 `*_nrf.bin` + `.s/.c` proofs (air, blinky, c_irq, dma,
  extras, i2s, matrix, nfct, sd_evt, sensors, spim23, stubs, uarte1,
  usbdev(.c), usbep, wdt) + `ble_fw/` (ble_conformance/gatt/pairing/roles/
  c_ble_face/ble_cpp_face + arduino sketch) + `link_nrf.ld/link_c_nrf.ld/
  hex2bin.py`.
  `demo/firmware/` = `micropython-microbit-v2.1.2.hex`. `mc/` = PXT/MakeCode.
- `demo/parts/`: `pins.js` (EDGE/ROWS/COLS/INTERNAL/LSM303 addrs),
  `lsm303.js` (accel 0x19/mag 0x1E on TWIM1), `spidisplay.js`/`ssd1306.js`
  (SPI/I2C displays), `kl27.js` (interface MCU), `mocks.js`, `crypto.js`,
  `ble_air.js`, `ble_live_e2e.mjs`, `handshake.mjs`, `smoke.mjs`,
  `spidisplay_check.mjs`, `ble_lang/` (c/mpy/js/py/ts/repl faces).

## 2. Lifecycle (call in this order — `API.md`, `demo/index.html` `bootImage`/`bootDirectApp`/`initBoard`/`pumpDma` + frame loop)

```
// demo/index.html bootImage (~589-610): staged bytes -> live core
wasm.reset_state();
…spi_tap / i2c_register_slave…   // parts' register(), BEFORE init
wasm.init();
cpu = new wasm.WasmCpu(sp, pc, 512*1024, 128*1024)
cpu.load_firmware(bytes, 0x0)
cpu.set_deliver_irqs(true)       // real fw (SoftDevice/CODAL/MPy) needs SVC+IRQ delivery
// demo/index.html bootDirectApp (~620-656): stock hexes (MPY/MakeCode)
// re-boot the app table at 0x1C000 with MBR params hand-installed
// (reset_state+init re-run, app vectors sanity-gated, reset_cpu(sp,pc))
loop: cpu.step(N) + wasm.tick_peripherals() + pumps + drainUart
```

Clock = `INSTRUCTION_COUNT/64MHz` virtual only (`tick`=1 insn, `tick_n`=batch,
`tick_peripherals`=model tick w/o clock). IRQs pend by default;
`cpu.set_deliver_irqs(true)` to preempt. Reboot: poll
`is_watchdog_reset_requested()` → `cpu.reset_cpu(read32(0), read32(4))`.

## 3. `WasmCpu` + free fns (`demo/pkg/nrf52833_periph_wasm.d.ts`: 154 named
exports = `WasmCpu` class + `initSync` + 152 free fns: 61 `ble_*` + 91 others)

`WasmCpu(sp,pc,flash,ram)`: `step(budget)→executed, reset_cpu,
set_deliver_irqs, sleeping/wake, get_pc/sp/regs/xpsr/sregs/fpscr/primask/ipsr,
fault_pc/op1/op2/len, mem_fault, mem_read/write, read8/write8/read32/write32,
trace_start/stop/take_trace, set_sreg/set_fpscr, twis_master_write/read,
spis_exchange`.
Free fns (grouped, verbatim): `reset_state, init, init_svd, tick, tick_n,
tick_peripherals, periph_read/write, has_pending_interrupt,
get_next_pending_interrupt, set_intr_pending, is_watchdog_reset_requested`;
`gpio_read_output/input, gpio_set_input` (P0=32 pins, P1=10) +
`gpio_read_dir` (PIN_CNF.DIR bit) + `matrix_state()` (25B row-major
pixels; lit <=> row OUT==0 && col OUT==1, both DIR=output — same rule
the bench frame loop uses);
`get_uart_output, uart_rx_byte` (UARTE0 0x40002000, pace on EVENTS_RXDRDY
0x40002108); `spi_tap, spi_take_events, spi_push_miso`;
`i2c_register_slave, i2c_take_events, i2c_push_rx`;
EASYDMA pumps `uarte_take_txdma/complete_txdma, uarte_take_rxdma/complete_rxdma,
twim_take_txdma/complete_txdma, twim_take_rxdma/complete_rxdma,
saadc_take_result/complete_result + saadc_check_limits,
pdm_take_sample/complete_sample,
usbd_take_epin/complete_epin, usbd_take_epout/complete_epout +
usbd_inject_setup/signal_reset,
qspi_take_read/write/erase + complete_*, qspi_register_flash,
nvmc_take_erase/complete_erase,
radio_take_tx/radio_inject_rx(+_lossy/_to_lossy/_to, inject_corrupt,
set_rssi/ed_dbm, complete_rx/tx + complete_rx_with_path_loss,
txpower/air_rssi/whiten/crc32/set+clear_interference),
i2s_take_rx/tx/complete_rx/tx/take_capture, nfct_take_tx/rx/complete_tx/rx +
field_present, sd_evt_queue_len, ccm_take_job/complete, aar_take_job/complete,
ecb_take_job/complete, temp_set_celsius, comp_set_input_mv,
qdec_step`; BLE jobs `ble_take_job (tags 0-16)/ble_take_data` + 61
`ble_complete_*/ble_post_*/ble_bond_*/ble_smp_*/ble_lesc_*/ble_tx_power_dbm/
ble_adv_state/peer_addr/queue_len/enabled/conn_handles/sec/batt_level` (full
list `API.md` §SoftDevice; tags 0-16 incl. 16 GattsServiceChanged).

## 4. Virtual parts + BLE air (`demo/parts/*`, `tools/ble_air_bridge.py`)

- `pins.js`: EDGE P0–P20 → (port,pin); ROWS/COLS 5×5 matrix (row OUT==0 &&
  col OUT==1); INTERNAL (BTN_A/B, SPEAKER P0.00, MIC, LOGO_TOUCH P1.04,
  I2C_INT, UART_INT); `LSM303_ACCEL=0x19, MAG=0x1E`.
- Parts own buses: TWIM1=LSM303 (`lsm303.js`), TWIM0=SSD1306 (`ssd1306.js`),
  SPIM0/2/3=display/flash (`spidisplay.js`), KL27 interface (`kl27.js`).
  Pattern: `register_slave/tap` before init → `poll(cpu)` per frame →
  `take_events` parse START/addr/STOP or CS-edge words → `push_rx/miso`.
- BLE: firmware SVC `0x60-0xBF` → `ble_take_job` → driver resolves over air
  (`ble_air_bridge.py` WS JSON: `ble_read/write/disc/uuid_read/vals_read/hvx/
  scan/connect/rssi/disconnect/pair/l2cap` in, `gatt/write_rsp/…/adv_report/
  connected/paired/…` out) → `ble_complete_*` posts SoftDevice event; no
  bridge = `pumpBleLoopback` (battery 87 + fixed table). `ble_live_e2e.mjs`
  + `ble_lang/` faces prove JS/MicroPython/C/TS drivers.

## 5. OpenHW gap (what's missing today)

- No micro:bit board in `component-registry.ts`/`board-profiles.ts`/
  `backend/src/compiler/boardRegistry.js` (UNO+PICO only), no runner branch in
  `execute.ts`, no `openhw-microbit*` component. Closest template: STM32-style
  tap runner (§6) + Pico `GP-normalize` precedent for EDGE P-names.
- New surfaces vs existing boards: 5×5 LED matrix (charlieplex GPIO render,
  like uno-r4 `matrix_trace_take`), 2 buttons + logo touch, LSM303 + mic/speaker,
  NFC, qspi flash, BLE/radio air env (map to `RadioEnvironment`/
  `wifi-status-parser`-shaped status, gateway room per espc3 pattern).

## 6. Runner mapping checklist (BoardRunner `component-registry.ts:472-501`)

1. Load: `reset_state()` → register taps/slaves per circuit → `init()` →
   `new WasmCpu(sp,pc,512K,128K)` → `load_firmware(bytes,0x0)`; `reset_cpu` = reload.
2. Tick: `cpu.step(budget)` + `tick_peripherals()` per frame (batched, never
   per-insn); `get_pc/sp/regs/fault_pc/mem_fault` → debug snapshot; `mem_read/
   write + periph_read/write` → READ/WRITE_MEM; `trace_start/stop/take_trace`
   → telemetry/digest, `tick_n` ← `setSpeed`.
3. GPIO/matrix: `gpio_read_output` → propagateBoardPin (EDGE names via
   `pins.js` map; rows/cols → matrix component `updatePixels`-shaped);
   `gpio_set_input` ← buttons/sliders/touch; `set_intr_pending + has/get_next_
   pending` → IRQ debug.
4. UART: `get_uart_output` drain → serial TX; `uart_rx_byte(0x40002000,b)` ←
   `serialRx/serialRxByte`, paced on `periph_read(0x40002108)` RXDRDY.
5. I2C/SPI/sensors: `i2c_take_events/push_rx`, `spi_take_events/push_miso` →
   ComponentSignalAPI `onI2CWrite/onI2CRead/onSPIByte` (LSM303/OLED/display
   virtual devices per circuit); `twis_master_*/spis_exchange` = host-engine
   side; SAADC/PDM/I2S pumps → `onAnalogVoltage/onI2SData/onMicrophoneRequest`;
   TEMP/COMP/QDEC jigs ← sliders; `qspi_register_flash + nvmc/qspi` pumps →
   block storage; `usbd_*` pumps + `usbd_inject_setup/signal_reset` → USB bytes.
6. Radio/BLE: `radio_take_tx/inject_rx(_lossy)/complete_rx + txpower/air_rssi`
   → `RadioEnvironment` air; `ble_take_job/ble_take_data → ble_complete_*` →
   BLE status + future `onBleAdvertise`-shaped hooks (bridge WS when present,
   loopback offline); NFC `nfct_*/field_present` → connect/disconnect events.
7. Debug/SAB: chunk-boundary breaks + `watch_add`-equivalent via `periph` +
   map/ELF symbols; `forceEmitState` publishes EDGE/matrix/slot/telemetry
   (pumps stay in-tick method calls, never postMessage-per-byte); watchdog bit
   → reset event on evt ring.

(End of file)
