# Changelog (consumable bundle: `demo/pkg/`)

`demo/pkg/` (glue JS + `.d.ts` + `.wasm`) is the frozen artifact
downstream runners vendor. Its bytes are committed in-tree and must match
the tagged release: bundle `nrf52833-periph-wasm` version below =
`demo/pkg/package.json` = crate `nrf52833-periph-wasm` version.
Rebuild recipe (pinned): `wasm-pack 0.14.0` + `wasm-bindgen` crate
`=0.2.126` (exact pin in `Cargo.toml`, `0.2.126` in Cargo.lock) +
`binaryen version_132` wasm-opt (CI) + `node >= 22.6.0`, via
`npm run build:wasm` in `demo/` (which deletes the regenerated
`pkg/.gitignore` so the wasm ships). Rebuilds from the same tree are
bit-identical (verified: no-op, post-pin, and forced full-LTO recompile);
canonical bytes = committed `demo/pkg/` at the tag.

## 0.2.0

New tap surface (additive, no behavior change to existing paths):

- Lifecycle: `reset_state`, `init`, `WasmCpu` (`load_firmware`,
  `reset_cpu`, `step`, `mem_read/mem_write`, `set_deliver_irqs`,
  `sleeping/wake`, `fault_pc`), `tick_peripherals`, `tick_n`,
  `has_pending_interrupt`, `is_watchdog_reset_requested`.
- GPIO: `gpio_read_output`, `gpio_read_dir`, `gpio_read_input`,
  `gpio_set_input` (+ `matrix_state` 25-byte 5x5 readback).
- UART: `uart_rx_byte`, `get_uart_output`, `uarte_take/complete_txdma`,
  `uarte_take/complete_rxdma`, `uarte_cts_asserted`, `uarte_rx_timeout`,
  `uarte_rx_error`.
- SPI: `spi_tap`, `spi_take_events`, `spi_push_miso`
  (+ `WasmCpu.spis_exchange`, `cpu.twis_master_write/read`).
- I2C/TWIM: `i2c_register_slave`, `i2c_take_events`, `i2c_push_rx`,
  `twim_take/complete_txdma`, `twim_take/complete_rxdma`.
- SAADC: `saadc_take_result`, `saadc_complete_result`,
  `saadc_check_limits` (+ `temp_set_celsius`, `comp_set_input_mv/aref_mv`,
  `qdec_step` for the other analog-ish inputs).
- PWM readback: `pwm_get_duty` (OUT0, basis points 0..10000),
  `pwm_get_freq_hz` (HFCLK-derived Hz), `pwm_observe_seq_word`
  (driver-fed SEQ latch). Slots 0-3 = PWM0-3.
- Rest of the EASYDMA pump table (`demo/API.md`): PDM, USBD
  (EPIN/EPOUT/ISO/setup/SOF), QSPI, NVMC, RADIO (+ air link-budget and
  DFE/IQ), I2S, NFCT, POWER, WDT, CCM/AAR/ECB crypto jobs,
  `periph_read/write` raw access, `qspi_register_flash`.

Fixed test firmwares (`demo/firmware/`, sources `blinky/*_nrf.s`):
`hello_nrf.bin` (UART `HELLO`), `oled_nrf.bin` (TWIM0 0x3C init +
repeated-START status read, `OLED:OK`), `blinky_nrf.bin` (P0.21 blink),
`spim23_nrf.bin` (SPIM2 TX + SPIM3 RX). Recipes in `demo/API.md`
("Fixed test firmwares", "Master-read recipe", "SAADC injection").

## 0.1.0

Initial vendored bundle (pre-changelog surface: lifecycle, GPIO, UART,
SPI/I2C taps, EASYDMA pump pairs, matrix, BLE SVC face).
