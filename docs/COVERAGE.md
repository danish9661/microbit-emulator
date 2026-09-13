# micro:bit v2.2 feature coverage — support matrix + audit

Target: BBC micro:bit v2/v2.2 (Nordic nRF52833, Cortex-M4F, 64 MHz,
512 KB flash @ `0x0`, 128 KB RAM @ `0x20000000`). Emulator = Rust/WASM
chip core (`nrf52833-periph-wasm/`) + JS virtual hardware (`demo/`).
DAPLink/interface MCU (KL27) is NOT emulated (JS loader + UART only).

Status key: **F** = functional (timed, IRQs, driver take/complete,
firmware proof) · **H** = handshake (TASKS/EVENTS/INTEN minimum, no
timed behavior or no consumer) · **–** = missing / deliberately
omitted. Counts: `cargo test` **191 green**,
`node demo/parts/smoke.mjs` green. Working tree intentionally dirty
(see §7); do not commit unless asked.

Generated 2026-09-13 from: `monox/nrf52833.svd` (70 peripherals, 46
unique base addresses), `src/peripherals/*.rs`, `src/lib.rs` exports,
`src/cpu/tests.rs`, `demo/parts/*`, `demo/API.md` (137 lines), plan
P16–P53, STATUS.md, codal-microbit-v2 v0.2.67 + MicroPython v2.1.2
sources (`/tmp` clones — ephemeral, re-clone on demand).

## 1. nRF52833 peripherals (SVD base → model → remark)

| SVD base(s) | SVD peripheral(s) | Model file | St | Remark |
|---|---|---|---|---|
| `0x40000000` | CLOCK+POWER | `clock_nrf.rs` | F | HF/LF STARTED+STAT, USBDETECTED/USBPWRRDY, RESETREAS+SREQ latch, GPREGRET, RAMSTATUS, LFCLKSRC; POWER_CLOCK IRQ0. AIRCR SYSRESETREQ fixed P53 (`aircr_sysresetreq_fires_and_self_clears`). |
| `0x40001000` | RADIO | `radio_nrf.rs` | F | PCNF-length TX take / RX completion+inject, CRCERROR inject, RSSI, SHORTS; `air_nrf` loopback proof. 802.15.4 helpers (P59): ED (EDSTART→EDEND+EDSAMPLE/EDCNT, EDSTOP→EDSTOPPED, `radio_set_ed_dbm`), CCA (CCASTART→CCAIDLE/CCABUSY vs CCACTRL, CCASTOP→CCASTOPPED), DEVMATCH/DEVMISS (+RXMATCH/RXCRC/PDUSTAT) via DAB/DAP, MHRMATCH via CONF/MAS, FRAMESTART+BCMATCH, TIFS/BCC/SFD/MODECNF0/POWER stored, full SHORTS/INTEN SVD bit maps (`ed_cca_mhr_devmatch_framestart`). Demo air = two-instance bridge (`window.__airPeer` foreign bytes, loopback default) — BLE/BT without WebBluetooth. |
| `0x40002000` | UART0+UARTE0 | `uarte_nrf.rs` | F | 1-byte TX DMA + RXDMA ring; OVERRUN/ERROR/TXSTOPPED (`0x158`/INTEN22, P20 fix); STOPTX never raises ENDTX. STARTTX snapshot (P52, `tx_snapshot_freezes_starttx_bytes`); holes 19/39/59/79 + tail-shift persist (P53i: 0/106 driver mismatches → pre-STARTTX, firmware-side). |
| `0x40028000` | UARTE1 | `uarte_nrf.rs` | F | TX/RX fully routed (was UARTE0-locked); `uarte1_txdma_roundtrip_targets_instance_1`; no dedicated UARTE1 firmware proof. |
| `0x40003000` | SPI0/SPIM0/SPIS0/TWI0/TWIM0/TWIS0 | `twim_nrf.rs` | F | Mode-blind shared base; SHORTS, LASTTX/STARTRX/SUSPEND, NACK-after-~6000-instr; TWIS/SPIS engines (`twis_master_write/read`, `spis_exchange`); RXD `0x518` MISO-for-SPI / I2C-queue-for-TWI (`rxd_polling_reads_slave_response_line`). P53: ADDRESS raw, `slave_present` exact-then-`>>1` (`0x72`→`0x39`); `address_matches_shifted_8bit_form`. |
| `0x40004000` | SPI1/SPIM1/SPIS1/TWI1/TWIM1/TWIS1 | `twim_nrf.rs` | F | Same engine, IRQ4. LSM303 + KL27-UIPM live here in demo (see §2). |
| `0x40023000` | SPI2/SPIM2/SPIS2 | `twim_nrf.rs` | F | Tap routing done (register RXD MISO + DMA frames); demo pump covers SPIM2/3 DMA; no dedicated SPIM2/3 firmware proof (L5: unit-green, re-verified). |
| `0x4002F000` | SPIM3 | `twim_nrf.rs` | F | Same as SPIM2 (IRQ47). See L5 remark above. |
| `0x40005000` | NFCT | `nfct_nrf.rs` | F | Field-detect/select state machine, frame TX/RX take-complete; C+S proof (`nfct_nrf.s/.bin`, `nrf_nfct_field_select_and_frames`). |
| `0x40006000` | GPIOTE | `gpiote_nrf.rs` | F | 8ch event/task, edge vs pull-up, PORT event, OUT drives GPIO. |
| `0x40007000` | SAADC | `saadc_nrf.rs` | F | CH config/limits, LIMIT events, RESULTDONE/STOPPED, EASYDMA take/complete + result pump. |
| `0x40008000`–`0x4000A000` | TIMER0–2 | `timer_nrf.rs` | F | Prescaler/bitmode/SHORTS-CLEAR, CAPTURE snapshots live counter, INTEN 16+i, IRQs 8–10. |
| `0x4001A000`–`0x4001B000` | TIMER3–4 | `timer_nrf.rs` | F | IRQs 26/27. TIMER4 never STARTs in MakeCode (display never constructed — L4 pre-scroll stall, not a timer gap). |
| `0x4000B000`/`0x40011000`/`0x40024000` | RTC0–2 | `rtc_nrf.rs` | F | All three, IRQs 11/17/36. Thinnest (1 handshake each). |
| `0x4000C000` | TEMP | `temp_nrf.rs` | F | Driver-settable (`temp_set_celsius`), DATARDY/INTEN. Thinnest. |
| `0x4000D000` | RNG | `rng_nrf.rs` | F | Deterministic LCG, VALRDY/SHORTS-to-STOP. Thinnest. |
| `0x4000E000` | ECB | `misc_nrf.rs` | F | take/complete, FIPS-197 AES-128 proof (`nrf_ecb_aes128_fips_vector`); crypto runs driver-side. |
| `0x4000F000` | AAR+CCM | `misc_nrf.rs` | F | take/complete, RESOLVED/NOTRESOLVED, CTR+MIC roundtrip; no separate CCM slot (would alias AAR task map); AAR has no wasm export (JS can't drive it). |
| `0x40010000` | WDT | `wdt_nrf.rs` | F | Expiry + reboot semantics, double-reset proof; RR reload by firmware (no JS export needed). |
| `0x40012000` | QDEC | `qdec_nrf.rs` | F | Gray-code decode, report/double-read, host-steppable (`qdec_step`), STOPPED/SAMPLE offsets fixed. |
| `0x40013000` | COMP+LPCOMP | `comp_nrf.rs` | F | Thresholds, crossing events, driver input (`comp_set_input_mv`). |
| `0x40014000`–`0x40019000` | EGU0–5 (+SWI0–5) | `egu_nrf.rs` | F | All six, trigger/status; handshake test. Thinnest. |
| `0x4001C000`/`0x40021000`/`0x40022000`/`0x4002D000` | PWM0–3 | `pwm_nrf.rs` | F | All four, IRQs 28/33/34/45; loop/decoders. Thinnest. |
| `0x4001D000` | PDM | `pdm_nrf.rs` | F | take/complete sample pump. |
| `0x4001E000` | ACL+NVMC | `nvmc_nrf.rs` (+–) | F/– | NVMC: READY/READYNEXT always-1, WEN/EEN staging, erase take/complete (driver applies 0xFF). ACL/SPU deliberately out of scope (reads 0). |
| `0x4001F000` | PPI (+CHG, no FORK) | `ppi_nrf.rs` | F | Direct dispatch + group EN/DIS; no FORK register exists. |
| `0x40020000` | MWU | `mwu_nrf.rs` | F | Region/pregion config, SUB-masks, mem-access hook, armed-interrupt proof. |
| `0x40025000` | I2S | `misc_nrf.rs` | F | SVD-verified offsets, START-staged take_rx/take_tx, TX capture FIFO, streaming proof. Demo feeds silence (no WebAudio route — TODO in page). |
| `0x40027000` | USBD | `usbd_nrf.rs` | F | ENABLE/PULLUP/EPINEN, USBRESET+SETUP inject, EPIN/EPOUT take/complete; C proof (`usbdev_nrf.c`, flash-source DMA). |
| `0x40029000` | QSPI | `qspi_nrf.rs` | F | Absent from this SVD revision (explicit `new_wasm` slot, `mod.rs:567`); registered image, AND-only program, 4K/64K erase, take/complete + JS backend exports. |
| `0x10000000`/`0x10001000` | FICR/UICR | `ficr_uicr.rs` | F | PART=`0x52833`, sizes; UICR RAM store (BOOTLOADERADDR-gated boot depends on it). |
| `0x50000000`/`0x50000300` | P0/P1 | `gpio_nrf.rs` | F | OUT/DIR/CNF, inputs idle-HIGH (active-low buttons), combined block; PIN_CNF.DIR drives `dir[]` (P35, `pin_cnf_dir_bit_drives_dir`). |
| ARM core | NVIC/SCB/SysTick/MPU/FPU/DWT/ITM/STIR/DEMCR | core files | F | MPU enforced w/ MemManage+escalation; FPU SP (CPACR gate, no lazy stacking — documented); CoreSight `0xF0000000` reads 0. AIRCR SYSRESETREQ honored (P53). |
| `0x40026000` | FPU (nRF engine) | – | – | Deliberately skipped in SVD path (`mod.rs:282`): nRF engine ≠ ARM core FPU; explicit ARM slot owns "FPU". |
| – | ACL/SPU | – | – | Out of scope (reads 0). |
| – | SoftDevice SVCs (16/17/18/40/41/82) | – | – | NOT emulated: zero `svc 82` sites in MPY+MC (`0xDF52` zero hits) — sd_evt hook would be dead code (P32/P51, shelved). SVC dispatch observed only (MBR `0xAA4` cmp #24). |

## 2. micro:bit v2 board hardware → virtual part / driver

| Board HW (CODAL v0.2.67 driver) | Emulated by | St | Remark |
|---|---|---|---|
| 5×5 LED matrix (NRF52LEDMatrix → TIMER4 + GPIOTE/PPI) | `demo/index.html` matrix (DIR+OUT gate) + `demo/parts/pins.js` ROWS/COLS | F | DIR-gated render (P35 CNF→DIR fix); sticky DIR0=`0x01788000` in MakeCode. Content never drawn — init stalls pre-scroll (L4), not a render gap. |
| BTN_A P0.14 / BTN_B P0.23 (active-low, pull-up) | Buttons + `gpio_set_input`; `input_state` survives `init()` (P43) | F | Level-poll `sensors_nrf` proof prints BTN:1/0 on change; Playwright press→release verified. |
| LSM303AGR accel `0x19` + mag `0x1E` (internal TWIM1, DRDY P0.25/`irq1` active-lo) | `demo/parts/lsm303.js` (WHO_AM_I `0x33`/`0x40`, STATUS data-ready, live tilt) | F | DRDY pulses 60ms/140ms (P53: permanent low trips KL27 `idleCallback` >30-tick USB threshold on shared irq1). `requestUpdate` awaitSample spin needs the low window. `normAddr` handles nrfx shifted form. Smoke: WHO_AM_I/CTRL-echo/tilt/DMA/byte paths green. |
| KL27 USB interface (UIPM `0x70`, irq1-shared) | `lsm303.js` UIPM stub (empty = no event) | H | Answers version/board-revision probes so boot proceeds. `0x39/0x72` deliberately NOT stubbed (USB-FLASH chip; NACK = correct "no flash op"). |
| KL27 USB-FLASH (`0x39`, shifted `0x72`) | `lsm303.js` fail-fast stub (`0x20 0x01` = ERROR_RESPONSE, non-busy) | H | Exits `transact` in one RX attempt (P54: zeros meant "NOT READY"→busy→`rx_attempts=0` forever, 1217 reads/100M; NACK `break`s but still costs 20 TX×20 RX retries per call). P54b native: USB-flash reads 0, sensor-only 250/20M — busy-loop gone, banner still gated elsewhere. |
| Edge I2C (P19/P20, TWIM0) + SSD1306 OLED add-on | `demo/parts/ssd1306.js` (addr `0x3C`) | F | Smoke: init/fill/framebuffer/DMA paths green. |
| Edge SPI (P13/SCK, P14/MISO, P15/MOSI, P16/CS) | SPIM2/3 DMA pump in page + `spi_tap`/`spi_push_miso` API | H | Model routes MISO/DMA (L5 unit-green); no edge-SPI part wired in demo (no consumer). |
| UART USB serial (UARTE0 → KL27) | UART box + drip (`RXD.PTR+AMOUNT` mirror, never early ENDRX) | F | P39: RX handoff proven (head=tail=1, readline consumes). P51: RXDRDY stays 0 post-banner — parked-main, not drip-deadlock. |
| Speaker P0.00 / mic P0.05+RUN_MIC P0.20 / logo touch | `INTERNAL` pins map only | – | No audio/mic/touch parts. modaudio fetcher exonerated (NULL-source instant return); speaker-tick NULL-`this` fault is MP-layer (P24–P25). TODO if REPL audio matters. |
| NFC pins (P0.09/P0.10) | GPIO only | H | NFCPINS-as-GPIO UICR path noted in `MicroBit.cpp`; no NFC antenna model. |
| Power (KL27 UIPM) / deep sleep (WFE/WFI) | Sleep-aware pump (`tick_n` + wake on pending IRQ) | F | Without it the core naps forever at first idle (P16). `is_watchdog_reset_requested` → vector-table reboot (app table after direct-app boot). |
| LSM303 data-ready IRQ line (P0.25 level) | DRDY pulse (see LSM303 row) | F | Permanent-low regresses to USB-threshold hammering; pulse satisfies both consumers. |
| QSPI external flash | `qspi_register_flash` + take/complete API | F | Model + API proven; no board QSPI chip wired in demo (no consumer). |
| USB device (USBD) | EPIN/EPOUT pump + `usbd_signal_reset` on air-Run | F | `air_nrf` polls USBRESET first (test host pre-signals; Run replicates). |
| Radio (2.4 GHz, CODAL datagram) | Demo loopback (`radio_inject_rx` + complete) | H | `air_nrf` loopback proof; no real air (foreign bytes = future work). RSSI/corrupt inject exported. |

| Edge map (`demo/parts/pins.js`) | Value |
|---|---|
| Rings P0–P2 | RING0/P0.02, RING1/P0.03, RING2/P0.04 |
| Buttons | P5=BTN_A/P0.14, P11=BTN_B/P0.23 |
| Matrix COLs | P3/P0.31, P4/P0.28, P6/P1.05, P7/P0.11, P10/P0.30 |
| SPI | P13/SCK, P14/MISO, P15/MOSI, P16/CS(P1.02) |
| I2C | P19/SCL external, P20/SDA external; internal TWIM1 = sensors+KL27 |
| INTERNAL | SPEAKER/P0.00, MIC_IN/P0.05, RUN_MIC/P0.20, UART_INT P0.06/P1.08 |

## 3. MicroPython v2.1.2 module surface → REPL reality

| Boot fact (P16 recipe) | Value |
|---|---|
| App vectors | VT SP `0x20020000`, PC `0x29C51` at `0x1C000` |
| MBR params | `*(0x20000000)=0x1000`, `*(0x20000004)=0x1C000` |
| UICR seeds | BOOTLOADERADDR `0x77000` + settings `0x7E000` |
| Pump | Pristine re-init + sleep-aware (`tick_n` + wake) |
| Native banner | 160–180M instr (`0x266D4`, 106B with P49 holes) |
| Browser park | `0x200021b8/bb` pre-banner, uartLen 0 (P55: countdown wait with lr `0x26039`, caller TBD — not wall-time, not HFCLK, not TWIM/NVMC/UARTE; model clear on all probed state). |

| `microbit.*` surface (`modmicrobit.c` + `microbit_*.c`) | St | Remark |
|---|---|---|
| `display` (show/scroll/brightness) | H | Object exists; content blocked behind banner (L4: pre-scroll stall in MC too). `setBrightness(255)` runs in MPY `main.cpp`. |
| `button_a` / `button_b` | H | GPIO path proven (`sensors_nrf` BTN:1/0); MPY objects never reached (pre-banner). |
| `accelerometer` (get_x/y/z, gestures, `set_range`) | H | HAL `getSample→requestUpdate` needs DRDY+STATUS (provided); MPY accessor layer never reached. |
| `compass` (heading, calibrate, field strength) | H | Same gate as accelerometer; `compassCalibrator` at uBit+2340 named (P47; P46 watch was `radio.rxQueue`, exonerated). |
| `speaker` / `music` / `audio` / `sound` | H | Speaker-enabled default; audio tick path is the NULL-`this` prime suspect (P24–P25). No WebAudio route (I2S capture drained, TODO). |
| `microphone` (`SoundEvent`, threshold) | H | PDM pump exists; no mic part; MPY layer never reached. |
| `pin0`–`pin16`, `pin19/20`, `pin_logo`, `pin_speaker` | H | `getDigitalValue@0x28744` IS the observed poll — but on CODAL LSM303 driver `0x20003960`, not an MP pin (P52 correction). P0.00 toggles nested under NULL fault. |
| `i2c` / `spi` / `uart` | H | TWIM/UARTE paths proven (`dma_nrf`, `air_nrf`); MPY objects never reached. |
| `Image` / `Sound` / `SoundEvent` / `SoundEffect` types | H | Types exist in flash (`MicroBitImage`, `AudioFrame`…); never instantiated (pre-banner). |
| `reset` / `sleep` / `running_time` / `panic` / `temperature` | H | `sleep` = RAM delay-fn `0x200021b8` (P53–P55: `subs r0,#1; bne; bx lr`, called from the `0x20980` 20× helper, lr `0x26039`; r0 live countdown, r4=1000 — a wait, not a hang; caller TBD via r7-entry watch). `temperature` → TEMP model exists. |
| `set_volume` / `ws2812_write` (neopixel) | H | Present in image; never reached. |
| `run_every` / `scale` / `log` (datalog FS) | H | `log` → NVMC/flash path; fds-waiter divergence is demo-only (P26: native skips, demo waits on SD-event completion nothing delivers). |
| `radio` (`drv_radio.c`) | H | CODAL radio repoints vector (`0x2E44D` = main:57 ran natively); no RX attempts pre-banner. |
| REPL proper (`pyexec_friendly_repl`, readline, `print(1+2)→3`) | – | BLOCKED: banner first (L1). Post-banner NULL-`this` (`bx r3@0x4F75A` via `mp_call_function`, TIMER1-only, RX/TX-IRQ-excluded) + prompt-in-ring-never-staged (`is_tx` false, no kick) still open. P20 browser prompt+no-fault is env-specific. |

## 4. JS/wasm API + firmware proofs + wall-time

| API group (`src/lib.rs`, API frozen v1, `demo/API.md`) | Exports | St | Remark |
|---|---|---|---|
| Lifecycle | `reset_state`, `init`/`init_svd`, `WasmCpu::new`, `load_firmware` | F | Order: reset → taps → init → cpu → step/tick → pump → drain. Taps snapshot once at construction. |
| Clock | `tick`, `tick_n`, `tick_peripherals` | F | Virtual time = `INSTRUCTION_COUNT`/64MHz. Frame: `cpu.step(20000)` + `tick_peripherals`. |
| CPU control | `step`, `reset_cpu`, `set_deliver_irqs`, `sleeping`/`wake`, `get_pc/sp/regs/xpsr/sregs/fpscr/primask/ipsr`, `fault_*`, `mem_read/write`, `read8/write8/read32/write32`, `trace_start/stop`, `take_trace` | F | `deliver_irqs` default off (pend, never preempt). TX snapshot guard published via `step`. |
| Interrupts | `has_pending_interrupt`, `get_next_pending_interrupt`, `set_intr_pending`, `is_watchdog_reset_requested` | F | Watchdog/AIRCR → vector-table reboot (app table after direct-app boot). |
| GPIO + MMIO | `periph_read/write`, `gpio_read_output`, `gpio_set_input`, `gpio_read_input` | F | Active-low buttons; `input_state` survives `init()` (P43). |
| UART | `uart_rx_byte`, `get_uart_output`, `uarte_take/complete_txdma/rxdma` | F | Drip via `RXD.PTR+AMOUNT` mirror, never early ENDRX (P39). |
| I2C master | `i2c_register_slave`, `i2c_take_events`, `i2c_push_rx`, `twim_take/complete_txdma/rxdma` | F | TWIM0 = OLED, TWIM1 = sensors+KL27. Shifted-form `normAddr` in part. |
| I2C/SPI slave | `twis_master_write/read`, `spis_exchange` | F | P58 mock audit (TEMP, reverted): TWIS write lands in RAM; SPIS acquire + MISO/MOSI exchange. Unit-proven; no demo consumer (only tests/host drive them). |
| SPI taps | `spi_tap`, `spi_take_events`, `spi_push_miso` | H | Exported; no edge-SPI part wired (no consumer). |
| SAADC/PDM/TEMP | `saadc_take/complete_result`, `saadc_check_limits`, `pdm_take/complete_sample`, `temp_set_celsius` | F | Pumped per frame (SAADC/PDM); TEMP/RNG driver-settable. |
| Crypto | `ccm_take_job`, `ccm_complete`, `aar_take_job`, `aar_complete`, `ecb_take_job`, `ecb_complete` | F | P58 mock audit (TEMP, reverted): ECB FIPS-197 block in place + END; AAR RESOLVED + NOTRESOLVED (`0x4000F108`) paths; CCM job fields + decrypt flag + ENDCRYPT. Crypto runs driver-side by design (mock = the driver). No demo consumer (no BLE pairing UI) — F-grade model, H-grade wiring. |
| COMP/QDEC | `comp_set_input_mv`, `qdec_step` | F | P58 mock audit (TEMP, reverted): COMP Below/Above + UP edge via driver mV; QDEC host steps accumulate (ACC=+3). No demo consumer (no board knob/comparator wired) — F-grade model, H-grade wiring. |
| USBD | `usbd_signal_reset`, `usbd_take/complete_epin/epout`, `usbd_inject_setup` | F | Pumped per frame; `air` Run pre-signals USBRESET. |
| QSPI/NVMC | `qspi_register_flash`, `qspi_take/complete_read/write/erase`, `nvmc_take/erase`, `nvmc_complete_erase` | F | Driver applies 0xFF via `mem_write`; NVMC erase pumped per frame. |
| RADIO | `radio_take/complete_tx/rx`, `radio_inject_rx/corrupt`, `radio_set_rssi_dbm` | F | P58 BLE verdict: MPY radio is BARE-METAL (`drv_radio.c` drives `NRF_RADIO` directly + custom IRQ handler; SoftDevice never involved) — model covers exactly this surface, loopback proof green. No BLE-enabled image exists (`MICROBIT_BLE_ENABLED: 0` in MPY codal.json; `svc 82` zero even-aligned hits) — BLE stack work needs such an image first (see §8). |
| I2S | `i2s_take/complete_rx/tx`, `i2s_take_capture` | F | P58 mock audit (TEMP, reverted): RX silence fill + TX capture FIFO. Streaming proof green; demo feeds silence, capture drained (no WebAudio) — F-grade model, H-grade wiring. |
| NFCT | `nfct_field_present`, `nfct_take/complete_tx/rx` | F | P58 mock audit (TEMP, reverted): field-present → ACTIVATE → STARTTX/ENDTX + ENABLERXDATA/ENDRX + FIELDLOST. C+S proof green; no demo consumer beyond proof (no NFC antenna UI) — F-grade model, H-grade wiring. |

| Firmware proof (`blinky/*.s/.c/.bin`, GCC per `docs/README.md`) | Marker / check | St | Remark |
|---|---|---|---|
| blinky | BOOT/BLINK + GPIO | F | Full path flash@0x0 → CLOCK → GPIO → UARTE. Preset base64, byte-verified. |
| sensors | SENS:OK + BTN:1/0 level-poll + TWIM + GPIOTE | F | BTN_A P0.14 active-low; `input_state` survives reboot (P43). |
| dma | TWIM DMA loopback | F | Driver-style take → RAM move → complete, like the JS pump. |
| extras | SAADC/TEMP/RNG/PWM OK | F | One handshake each (thinnest). |
| stubs | SPIM0/2/3 + PDM/QSPI/USBD/RADIO OK | F | START/STOP→STOPPED on all three SPIM instances (326B, rebuilt from `.s`); preset base64 byte-identical to `blinky/stubs_nrf.bin`. |
| air | USB+RADIO loopback+PPI OK | F | Polls USBRESET first (Run replicates pre-signal). |
| c_irq | C TIMER0 IRQ + UART | F | C toolchain proof (bit-identical rebuild). |
| usbep / usbdev | SETUP + EPIN flash-DMA | F | `usbdev_nrf.c` flash-source DMA. |
| i2s | Streaming roundtrip | F | EXCLUDED from presets: needs patterned-RX + mailbox release only the test driver provides. |
| wdt | Expiry + double-reset | F | Pet-vs-unpetted semantics. |
| nfct | Field-select + frames | F | Field-detect/select state machine. |
| 2nd-run | `reset_state`, no leak | F | Every proof re-runs clean (BOOT_LOCK + UART lock discipline). |

| Wall-time (L6, environmental) | Value | Remark |
|---|---|---|
| Pump batching | 5×[20K step+tick+pumpDma] per frame (P54: duties moved INSIDE the sub-loop) | Was 1.20 MIPS vsync-capped; meter reads ~6 in-browser (16-class bursts are peak slice rates, not sustained banner throughput). Duties-once-per-frame starved polled firmware (1B STARTTX waited ~100K; DRDY pulse couldn't land in a 20K window). Browser P54c post-fix: TWIM clean (`t_err=0/endrx=1`) but still pre-banner — throughput no longer suspect, gate is elsewhere (P55: countdown wait). |
| Banner cost | ~150–260M instr | ~30s at 6 MIPS; 600s+ at 300K. Native L1 banners 160–180M (~43s harness). |
| Profile | dev == release (byte-identical) | wasm-pack single profile; speed is environmental, never the lever. |
| Remaining gap | Browser reparks pre-banner at identical pc | Throughput, not model, is suspect #1. |
| Package | `demo/pkg` 1.5MB committed, `.gitignore` removed | Deliberate: Pages serves it directly, no toolchain needed. |

## 5. Real-firmware scoreboard (all executed, zero CPU faults*)

| Firmware | Result | Remark |
|---|---|---|
| MicroPython v2.1.2 (direct-app `0x1C000`) | Boots OK; banner 106B natively, pre-banner park in browser | 2 AIRCR resets honored, RESETREAS SREQ, GPIO live, TWIM ACKs, pc `0x282B2` @340M natively. Browser park `0x200021b8/bb`, uartLen 0. *Except post-banner NULL fault (P24–P25, MP-layer, open). |
| MakeCode `basic.showString("A")` (makecode 1.3.6, `mc/` gitignored) | Scheduler idle; display never enables | P57 zero-touch (300M): TIMER4-CC0/GPIOTE-CONFIG[1..5]/PPI-CHENSET/TWIM1-ADDR/NVMC-CONFIG/UARTE-TXMAX/waiter-`0x30C04` ALL untouched; pc `0x20002078/7A`→`0x37AFA` WFE-idle, DIR0 sticky `0x01788000`. Stall is pre-scroll sequencing (main never issues scroll). |
| Espruino 2v29 | Boots | CoreSight PID fix needed; console is P0.06 bit-bang, nothing TX in early windows. |
| Bootloader chain (`0x77000`) | Entry + FICR gather + benign post-UICR reset; 2nd reset CODED AIRCR | `0x78514` via tbb `0x78498` (r5=1); `0x783FE` park = post-AIRCR wait. r4==0 is SD-enable SUCCESS (`cbnz r4@0x7B636` skips validation on FAILURE; success → `0x7B5B4`+`0x7B568` → tbb reset #2 BY DESIGN). `0x7B5B4` = IPR22 validator (`236>>a` odd; IPR22=0 always fails — needs SD priorities). `0x784C4` = DFU-progress gate (`[0x20002DF1]`, `[0x2DFC]-[0x2DF4]` vs 59), not the r4 cause. MBR selector (`0x417`) never reads `0x10001200/204` (`0x0–0xB00` sweep) — P42 refuted, direct-app stays. MBR pass-2 needs SD priorities (shelved). |

## 6. LEFT — the 6 STATUS items, each with its next action

| # | Item | Status | Next action |
|---|---|---|---|
| 1 | REPL exec (`print(1+2)` → `3`) | Blocked behind banner (L1) | Long browser run past 160–180M to banner, then `print(1+2)` prompt-kick sampling (pc `0x2874x/0x266Dx` + RXDRDY + ring deltas, P51 pattern). |
| 2 | TX drops (holes + tail-shift) | Snapshot ACTIVE but incomplete | Firmware-side slot audit (no trait change); live verify needs (1). P53i holds: 0/106 driver mismatches. |
| 3 | Bootloader full chain | r4==0/DFU decoded (§5); MBR pass-2 needs SD priorities | None without SD event synthesis (explicitly out of scope). |
| 4 | MakeCode display content | Zero-touch proven (P57: no member init touches HW in 300M) | Constructor-order trace (which member init reaches `0x30CD0`) + (7,1) producer ID. |
| 5 | SPIM2/3 tap routing | Done + firmware-proven (START/STOP→STOPPED on both instances via extended `stubs_nrf`; preset base64 byte-identical) | No edge-SPI demo part (no consumer) — stays H by decision. |
| 6 | Demo wall-time | Environmental (~6 MIPS, banner ~30s at speed) | Re-measure after (1); wasm-opt/pump-quantum only if still slow. |

## 8. BLE/BT feasibility verdict (P58 — NO build, evidence only)

| Question | Finding |
|---|---|
| Does anything on the banner path need BLE? | No. MPY `codal.json` sets `MICROBIT_BLE_ENABLED: 0` — `MicroBit::init` skips `bleManager.init` + pairing branch entirely. BLE contributes zero pre-banner instructions. |
| Does the MPY radio path need SoftDevice? | No. `drv_radio.c` (`microbit_radio_enable`) is bare-metal `NRF_RADIO` + custom IRQ (`main.cpp` re-vectors `RADIO_IRQn` post-init). The RADIO model covers exactly this surface (loopback proof green). |
| Is `sd_evt_get` really absent? | Yes, on aligned evidence: even-address scan finds ZERO `DF52` sites (prior 2 hits were odd-addressed data bytes). No firmware here can observe an SD event — P32/P51 shelve stands. `docs/sd_evt_design.md` stays a corrected reference (valid IF a future image calls it; today dead code — do not build). |
| What would BLE/BT work need? | (a) A BLE-ENABLED image first (MPY codal.json flip or MakeCode BLE program); (b) rescan SVCs (expect nonzero svc82 + GAP/GATT traffic) BEFORE writing model code; (c) SoftDevice state synthesis (observer callbacks, conn pump — order past flash-only transport); (d) a host-side BT peer (WebBluetooth or 2nd loopback endpoint). RADIO model needs no changes. |

## 9. Working tree + verify (DO NOT COMMIT unless asked)

| File | Change | Remark |
|---|---|---|
| `STATUS.md` | 188→190 counts | +TWIM shifted-ADDR match, +SCB AIRCR SYSRESETREQ test. |
| `plan.md` | P53 notes | Browser-gap + TX tail-shift + L4 findings. |
| `demo/parts/lsm303.js` | `normAddr` + UIPM `0x70` stub + DRDY pulse | Shifted-form match; empty UIPM frame; 60/140ms pulse satisfies sensor spin + KL27 threshold. |
| `demo/parts/smoke.mjs` | KL27 + pulse checks | Slave list `0x19/0x1E/0x70`; both DRDY phases observed. |
| `demo/pkg/nrf52833_periph_wasm_bg.wasm` | Rebuilt (1.5MB) | Built from this tree; `.gitignore` removed (intentional). |
| `blinky/stubs_nrf.s` + `.bin` | SPIM2/3 START/STOP handshake (270B→326B) | GAS-verified halfwords, rebuilt via `docs/README.md` flags, `nrf_stubs` test green; preset base64 synced byte-identical. |
| `nrf52833-periph-wasm/src/peripherals/scb.rs` | AIRCR mask `0F04` + self-clear + test | `0x05FA0004` now latches reset (was swallowed). |
| `nrf52833-periph-wasm/src/peripherals/twim_nrf.rs` | Raw ADDRESS + exact-then-`>>1` + tests | `0x72` finds `0x39` tap; readback = written value (silicon). |
| `docs/COVERAGE.md` | This file | Table audit (uncommitted, per order). |

```
cargo test                       # 191 green (crate dir)
node demo/parts/smoke.mjs        # parts green
wasm-pack build nrf52833-periph-wasm --target web --out-dir ../demo/pkg
rm -f demo/pkg/.gitignore        # pkg intentionally committed
```

| Probe hygiene (AGENTS.md) | Rule |
|---|---|
| Location | `/tmp/opencode/**` only (ephemeral — re-create per session). |
| Page edits | One-line `window.__dbg` TEMP edits, always reverted (`grep __dbg` = 0). |
| Native harnesses | TEMP files, deleted after run (`git checkout -- src/cpu/mod.rs` pattern). |
| Core | Never edit `src/cpu/` for board issues; never invent a second clock (`INSTRUCTION_COUNT` + `tick()` + `tick_n` only). |
