# micro:bit v2.2 feature coverage — support matrix + audit

Target: BBC micro:bit v2/v2.2 (Nordic nRF52833, Cortex-M4F, 64 MHz,
512 KB flash @ `0x0`, 128 KB RAM @ `0x20000000`). Emulator = Rust/WASM
chip core (`nrf52833-periph-wasm/`) + JS virtual hardware (`demo/`).
DAPLink/interface MCU (KL27) is NOT emulated (JS loader + UART only).

Status key: **F** = functional (timed, IRQs, driver take/complete,
firmware proof) · **H** = handshake (TASKS/EVENTS/INTEN minimum, no
timed behavior or no consumer) · **–** = missing / deliberately
omitted. Counts: `cargo test` **238 green**,
`node demo/parts/smoke.mjs` green, `node demo/parts/handshake.mjs`
18/18, `node demo/parts/ble_live_e2e.mjs` 42/42 over air, browser
16/16 (`python3 tools/browser_verify_16.py`).

Generated 2026-09-13 from: `monox/nrf52833.svd` (70 peripherals, 46
unique base addresses), `src/peripherals/*.rs`, `src/lib.rs` exports,
`src/cpu/tests.rs`, `demo/parts/*`, `demo/API.md` (137 lines), plan
P16–P53, STATUS.md, codal-microbit-v2 v0.2.67 + MicroPython v2.1.2
sources (`/tmp` clones — ephemeral, re-clone on demand).

## 1. nRF52833 peripherals (SVD base → model → remark)

| SVD base(s) | SVD peripheral(s) | Model file | St | Remark |
|---|---|---|---|---|
| `0x40000000` | CLOCK+POWER | `clock_nrf.rs` | F | HF/LF STARTED+STAT, USBDETECTED/USBPWRRDY, RESETREAS+SREQ latch, GPREGRET, RAMSTATUS, LFCLKSRC; POWER_CLOCK IRQ0. AIRCR SYSRESETREQ fixed P53 (`aircr_sysresetreq_fires_and_self_clears`). |
| `0x40001000` | RADIO | `radio_nrf.rs` | F | PCNF-length TX take / RX completion+inject, CRCERROR inject, RSSI, SHORTS; `air_nrf` loopback proof. 802.15.4 helpers (P59): ED (EDSTART→EDEND+EDSAMPLE/EDCNT, EDSTOP→EDSTOPPED, `radio_set_ed_dbm`), CCA (CCASTART→CCAIDLE/CCABUSY vs CCACTRL, CCASTOP→CCASTOPPED), DEVMATCH/DEVMISS (+RXMATCH/RXCRC/PDUSTAT) via DAB/DAP, MHRMATCH via CONF/MAS, FRAMESTART+BCMATCH, TIFS/BCC/SFD/MODECNF0/POWER stored, full SHORTS/INTEN SVD bit maps (`ed_cca_mhr_devmatch_framestart`). Link-budget air level (P119): TXPOWER-code table + path-loss inject forms + RX-stamped RSSI (`txpower_table_and_air_rssi_link_budget`). Demo air = two-instance bridge (`window.__airPeer` foreign bytes, loopback default) — BLE/BT without WebBluetooth. |
| `0x40002000` | UART0+UARTE0 | `uarte_nrf.rs` | F | 1-byte TX DMA + RXDMA ring; OVERRUN/ERROR/TXSTOPPED (`0x158`/INTEN22, P20 fix); STOPTX never raises ENDTX. STARTTX snapshot (P52, `tx_snapshot_freezes_starttx_bytes`); holes 19/39/59/79 + tail-shift persist (P53i: 0/106 driver mismatches → pre-STARTTX, firmware-side). |
| `0x40028000` | UARTE1 | `uarte_nrf.rs` | F | TX/RX fully routed (was UARTE0-locked); firmware proof (`uarte1_nrf.s/.bin`, `U1DATA` TX + 3 B RX, `nrf_uarte1_instance_dma_roundtrip`, P110). |
| `0x40003000` | SPI0/SPIM0/SPIS0/TWI0/TWIM0/TWIS0 | `twim_nrf.rs` | F | Mode-blind shared base; SHORTS, LASTTX/STARTRX/SUSPEND, NACK-after-~6000-instr; TWIS/SPIS engines (`twis_master_write/read`, `spis_exchange`); RXD `0x518` MISO-for-SPI / I2C-queue-for-TWI (`rxd_polling_reads_slave_response_line`). P53: ADDRESS raw, `slave_present` exact-then-`>>1` (`0x72`→`0x39`); `address_matches_shifted_8bit_form`. |
| `0x40004000` | SPI1/SPIM1/SPIS1/TWI1/TWIM1/TWIS1 | `twim_nrf.rs` | F | Same engine, IRQ4. LSM303 + KL27-UIPM live here in demo (see §2). |
| `0x40023000` | SPI2/SPIM2/SPIS2 | `twim_nrf.rs` | F | Tap routing done (register RXD MISO + DMA frames); firmware DMA proof (`spim23_nrf.s/.bin`, SPIM2 4 B TX, P110); SPI never NACKs (`arm_nack` guard); demo pumpDma covers SPIM2/3. |
| `0x4002F000` | SPIM3 | `twim_nrf.rs` | F | Same engine (IRQ47); firmware DMA proof (SPIM3 4 B RX, P110). |
| `0x40005000` | NFCT | `nfct_nrf.rs` | F | Field-detect/select state machine, frame TX/RX take-complete; C+S proof (`nfct_nrf.s/.bin`, `nrf_nfct_field_select_and_frames`). |
| `0x40006000` | GPIOTE | `gpiote_nrf.rs` | F | 8ch event/task, edge vs pull-up, PORT event, OUT drives GPIO. |
| `0x40007000` | SAADC | `saadc_nrf.rs` | F | CH config/limits, LIMIT events, RESULTDONE/STOPPED, EASYDMA take/complete + result pump. |
| `0x40008000`–`0x4000A000` | TIMER0–2 | `timer_nrf.rs` | F | Prescaler/bitmode/SHORTS-CLEAR, CAPTURE snapshots live counter, INTEN 16+i, IRQs 8–10. |
| `0x4001A000`–`0x4001B000` | TIMER3–4 | `timer_nrf.rs` | F | IRQs 26/27. TIMER4 never STARTs in MakeCode (display never constructed — L4 pre-scroll stall, not a timer gap). |
| `0x4000B000`/`0x40011000`/`0x40024000` | RTC0–2 | `rtc_nrf.rs` | F | All three, IRQs 11/17/36; COMPARE match + OVRFLW wrap + INTEN/ISER IRQ gating (P110 depth). |
| `0x4000C000` | TEMP | `temp_nrf.rs` | F | Driver-settable (`temp_set_celsius`), DATARDY + INTEN/ISER IRQ gating + STOP clear (P110 depth). |
| `0x4000D000` | RNG | `rng_nrf.rs` | F | Deterministic LCG, VALRDY/SHORTS-to-STOP + VALUE re-arm + IRQ gating (P110 depth). |
| `0x4000E000` | ECB | `misc_nrf.rs` | F | take/complete, FIPS-197 AES-128 proof (`nrf_ecb_aes128_fips_vector`); crypto runs driver-side. |
| `0x4000F000` | AAR+CCM | `misc_nrf.rs` | F | take/complete, RESOLVED/NOTRESOLVED, CTR+MIC roundtrip; no separate CCM slot (would alias AAR task map); AAR has no wasm export (JS can't drive it). |
| `0x40010000` | WDT | `wdt_nrf.rs` | F | Expiry + reboot semantics, double-reset proof; RR reload by firmware (no JS export needed). |
| `0x40012000` | QDEC | `qdec_nrf.rs` | F | Gray-code decode, report/double-read, host-steppable (`qdec_step`), STOPPED/SAMPLE offsets fixed. |
| `0x40013000` | COMP+LPCOMP | `comp_nrf.rs` | F | Thresholds, crossing events, driver input (`comp_set_input_mv`). |
| `0x40014000`–`0x40019000` | EGU0–5 (+SWI0–5) | `egu_nrf.rs` | F | All six, trigger/status; per-channel independence + INTEN mask + INTENCLR (P110 depth). |
| `0x4001C000`/`0x40021000`/`0x40022000`/`0x4002D000` | PWM0–3 | `pwm_nrf.rs` | F | All four, IRQs 28/33/34/45; loop/decoders; STOP→STOPPED + INTEN/ISER gating + SEQSTART1 (P110 depth). |
| `0x4001D000` | PDM | `pdm_nrf.rs` | F | take/complete sample pump. |
| `0x4001E000` | ACL+NVMC | `nvmc_nrf.rs` | F | NVMC: READY/READYNEXT always-1, WEN/EEN staging, erase take/complete (driver applies 0xFF). ACL: 8 regions (ADDR/SIZE/PERM, sticky SIZE-0/PERM-OR), write-protect enforced at stage (ERASEPAGE/ERASEALL refuse overlap), read-block queryable (`acl_read_blocked_at`; mem-layer hook TODO). No SPU on 833 (verified: no SPU in 70 SVD peripherals). |
| `0x4001F000` | PPI (+CHG, no FORK) | `ppi_nrf.rs` | F | Direct dispatch + group EN/DIS; no FORK register exists. |
| `0x40020000` | MWU | `mwu_nrf.rs` | F | Region/pregion config, SUB-masks, mem-access hook, armed-interrupt proof. |
| `0x40025000` | I2S | `misc_nrf.rs` | F | SVD-verified offsets, START-staged take_rx/take_tx, TX capture FIFO, streaming proof. Demo feeds silence (no WebAudio route — TODO in page). |
| `0x40027000` | USBD | `usbd_nrf.rs` | F | ENABLE/PULLUP/EPINEN, USBRESET+SETUP inject, EPIN/EPOUT take/complete; C proof (`usbdev_nrf.c`, flash-source DMA). |
| `0x40029000` | QSPI | `qspi_nrf.rs` | F | Absent from this SVD revision (explicit `new_wasm` slot, `mod.rs:567`); registered image, AND-only program, 4K/64K erase, take/complete + JS backend exports. |
| `0x10000000`/`0x10001000` | FICR/UICR | `ficr_uicr.rs` | F | PART=`0x52833`, sizes; UICR RAM store (BOOTLOADERADDR-gated boot depends on it). |
| `0x50000000`/`0x50000300` | P0/P1 | `gpio_nrf.rs` | F | OUT/DIR/CNF, inputs idle-HIGH (active-low buttons), combined block; PIN_CNF.DIR drives `dir[]` (P35, `pin_cnf_dir_bit_drives_dir`). |
| ARM core | NVIC/SCB/SysTick/MPU/FPU/DWT/ITM/STIR/DEMCR | core files | F | MPU enforced w/ MemManage+escalation; FPU SP (CPACR gate, lazy stacking implemented — `fpu_lazy_*` green); CoreSight `0xF0000000` reads 0. AIRCR SYSRESETREQ honored (P53). |
| `0x40026000` | FPU (nRF engine) | `fpu_engine_nrf.rs` | F | Minimal stub, SVD-grounded: single UNUSED word reads 0, writes ignored, both maps (`engine_slot_in_both_maps`). nRF engine ≠ ARM core FPU (FPCCR/FPCAR/MVFR at `0xE000EF34`, `fpu.rs`); IRQ 38 never pends. |
| – | ACL/SPU | – | – | No SPU peripheral exists on nRF52833 (SVD-verified: 70 peripherals, none named SPU); ACL is a modeled region above. |
| – | ACL CPU-read gate | – | – | Stored + queryable (`acl_read_blocked_at`); enforcement needs a mem-layer hook (AGENTS.md cpu rule — TODO, reads behave unprotected). |
| – | SoftDevice SVCs (16/17/18/40/41/82) | – | – | NOT emulated: zero `svc 82` sites in MPY+MC (`0xDF52` zero hits) — sd_evt hook would be dead code (P32/P51, shelved). SVC dispatch observed only (MBR `0xAA4` cmp #24). |

## 2. micro:bit v2 board hardware → virtual part / driver

| Board HW (CODAL v0.2.67 driver) | Emulated by | St | Remark |
|---|---|---|---|
| 5×5 LED matrix (NRF52LEDMatrix → TIMER4 + GPIOTE/PPI) | `demo/index.html` matrix (DIR+OUT gate) + `demo/parts/pins.js` ROWS/COLS + Matrix demo button (JS sweep + A glyph) | F | DIR-gated render (P35 CNF→DIR fix); P127 GPIOTE task polarity fix (SET/CLR unconditional in task mode); `blinky/matrix_nrf.s/.bin` firmware proof (`MATRIX:OK`, row/col DIR asserts). Content never drawn — init stalls pre-scroll (L4), not a render gap. |
| BTN_A P0.14 / BTN_B P0.23 (active-low, pull-up) | Buttons + `gpio_set_input`; `input_state` survives `init()` (P43) | F | Level-poll `sensors_nrf` proof prints BTN:1/0 on change; Playwright press→release verified. |
| LSM303AGR accel `0x19` + mag `0x1E` (internal TWIM1, DRDY P0.25/`irq1` active-lo) | `demo/parts/lsm303.js` (WHO_AM_I `0x33`/`0x40`, STATUS data-ready, live tilt) | F | DRDY pulses 60ms/140ms (P53: permanent low trips KL27 `idleCallback` >30-tick USB threshold on shared irq1). `requestUpdate` awaitSample spin needs the low window. `normAddr` handles nrfx shifted form. Smoke: WHO_AM_I/CTRL-echo/tilt/DMA/byte paths green. |
| KL27 USB interface (UIPM `0x70`, irq1-shared) | `demo/parts/kl27.js` Kl27Uipm via `lsm303.js` TWIM1 delegation | F | Valid protocol frames per CODAL wire contract: READ_RSP BOARD_REV 0x9904 (V2.00 KL27) / I2C v2 (BUSY_FLAG_SUPPORTED, no null-txn) / DAPLink / POWER_SRC / POWER_CONS / USB_STATE / KL27_MODE / LED_STATE + USER_EVENT queue; WRITE applies LED/mode; e8777 NOP-wake ignored; unknown -> ERR/UNKNOWN. Smoke: protocol + geometry + speaker/mic/logo checks. |
| KL27 USB-FLASH (`0x39`, wire `0x72<<1`) | `demo/parts/kl27.js` Kl27Flash via `lsm303.js` TWIM1 delegation | F | transact() contract: FILENAME echo+8.3 body / FILESIZE BE32 / VISIBILITY / DISK_SIZE / SECTOR_SIZE 4096 / STATUS never-busy; READ/WRITE/ERASE on a 0x1F000 image (4096x31, 64B maxWrite, single-page-erase-only). Always-valid answers (taps registered => no NACK, no 20x20 retry burn). |
| Edge I2C (P19/P20, TWIM0) + SSD1306 OLED add-on | `demo/parts/ssd1306.js` (addr `0x3C`) | F | Smoke: init/fill/framebuffer/DMA paths green. |
| Edge SPI (P13/SCK, P14/MISO, P15/MOSI, P16/CS) | SPIM2/3 DMA pump in page + `spi_tap`/`spi_push_miso` API | H | Model routes MISO/DMA (L5 unit-green); no edge-SPI part wired in demo (no consumer). |
| UART USB serial (UARTE0 → KL27) | UART box + drip (`RXD.PTR+AMOUNT` mirror, never early ENDRX) | F | P39: RX handoff proven (head=tail=1, readline consumes). P51: RXDRDY stays 0 post-banner — parked-main, not drip-deadlock. |
| Speaker P0.00 / mic P0.05+RUN_MIC P0.20 / logo touch P1.04 | `demo/parts/kl27.js` SpeakerPart/MicPart/LogoTouchPart + bench Audio panel | F | Pins per MicroBitIO.h/.cpp (logo P1_04 capacitive, speaker P0_00, runmic P0_20, mic P0_05). Speaker: P0.00 edge + PWM1 SEQSTARTED observer (read-only, host-side mute). Mic: RUN_MIC-gated 440 Hz sine+noise into PDM (unpowered = silence). Logo: active-low press/release on P1.04 + bench button. Smoke: edges/mute/silence/tone/press checks. |
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
| Pump | Pristine re-init + sleep-aware (`tick_n` + wake) + full TWIM completion (P116: takes drained-without-complete starved sensor-init to 200M+; the bench `lsm303.js` poll always completed both directions) |
| Native banner | ~237.8M instr (P116 full-TWIM pump; 78 B, zero faults) |
| Browser banner | T+15s wall via mpy preset + Run, 104–105 B + `>>> ` prompt (P117, zero page errors) |

| `microbit.*` surface (`modmicrobit.c` + `microbit_*.c`) | St | Remark |
|---|---|---|
| `display` (show/scroll/brightness) | H | Object exists; content blocked behind banner (L4: pre-scroll stall in MC too). `setBrightness(255)` runs in MPY `main.cpp`. |
| `button_a` / `button_b` | H | GPIO path proven (`sensors_nrf` BTN:1/0); MPY objects never reached (pre-banner). |
| `accelerometer` (get_x/y/z, gestures, `set_range`) | H | HAL `getSample→requestUpdate` needs DRDY+STATUS (provided); MPY accessor layer never reached. |
| `compass` (heading, calibrate, field strength) | H | Same gate as accelerometer; `compassCalibrator` at uBit+2340 named (P47; P46 watch was `radio.rxQueue`, exonerated). |
| `speaker` / `music` / `audio` / `sound` | H | Speaker-enabled default; audio tick path is the NULL-`this` prime suspect (P24–P25). No WebAudio route (I2S capture drained, TODO). |
| `microphone` (`SoundEvent`, threshold) | H | PDM pump exists; no mic part; MPY layer never reached. |
| `pin0`–`pin16`, `pin19/20`, `pin_logo`, `pin_speaker` | H | `getDigitalValue@0x28744` IS the observed poll — but on CODAL LSM303 driver `0x20003960`, not an MP pin (P52 correction). P0.00 toggles nested under NULL fault. |
| UART serial (`uBit.serial`, `NRF52Serial`) | F | Object at `0x20002BA4` (id 12 ✓); pre-P116 TRUE-seed park showed TX BUFF_INIT only (`0x4000`), rings NULL, baud 0 — SUPERSEDED by P116 (starved TWIM sensor-init, not a serial stall). Banner + REPL prompt proven native and in-browser (P116+P117). TX snapshot + RX drip carry the bytes. |
| `i2c` / `spi` | H | TWIM/UARTE paths proven (`dma_nrf`, `air_nrf`); MPY objects never reached. |
| `Image` / `Sound` / `SoundEvent` / `SoundEffect` types | H | Types exist in flash (`MicroBitImage`, `AudioFrame`…); never instantiated (pre-banner). |
| `reset` / `sleep` / `running_time` / `panic` / `temperature` | H | `sleep` = RAM delay-fn `0x200021b8` (P53–P55 shape; pre-P116 park site, exited once TWIM completions flow). `temperature` → TEMP model exists. |
| `set_volume` / `ws2812_write` (neopixel) | H | Present in image; never reached. |
| `run_every` / `scale` / `log` (datalog FS) | H | `log` → NVMC/flash path; fds-waiter divergence is demo-only (P26: native skips, demo waits on SD-event completion nothing delivers). |
| `radio` (`drv_radio.c`) | H | CODAL radio repoints vector (`0x2E44D` = main:57 ran natively); no RX attempts pre-banner. |
| REPL proper (`pyexec_friendly_repl`, readline, `print(1+2)→3`) | F | CLOSED P116+P117: native `print(1+2)` → `3` + prompt (125 B, zero faults) and in-browser `...>>> print(1+2)\n3\n>>> ` (+10s after banner, zero page errors). Pre-P116 park/`0x200021B8` forensics kept in STATUS §6. |

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
| RADIO | `radio_take/complete_tx/rx`, `radio_inject_rx/corrupt`, `radio_set_rssi_dbm` | F | P58 BLE verdict: MPY radio is BARE-METAL (`drv_radio.c` drives `NRF_RADIO` directly + custom IRQ handler; SoftDevice never involved) — model covers exactly this surface, loopback proof green. No BLE-enabled stock image (`MICROBIT_BLE_ENABLED: 0` in MPY codal.json; `svc 82` zero even-aligned hits) — see §8 gaps table. |
| I2S | `i2s_take/complete_rx/tx`, `i2s_take_capture` | F | P58 mock audit (TEMP, reverted): RX silence fill + TX capture FIFO. Streaming proof green; demo feeds silence, capture drained (no WebAudio) — F-grade model, H-grade wiring. |
| NFCT | `nfct_field_present`, `nfct_take/complete_tx/rx` | F | P58 mock audit (TEMP, reverted): field-present → ACTIVATE → STARTTX/ENDTX + ENABLERXDATA/ENDRX + FIELDLOST. C+S proof green; no demo consumer beyond proof (no NFC antenna UI) — F-grade model, H-grade wiring. |

| Firmware proof (`blinky/*.s/.c/.bin`, GCC per `docs/README.md`) | Marker / check | St | Remark |
|---|---|---|---|
| blinky | BOOT/BLINK + GPIO | F | Full path flash@0x0 → CLOCK → GPIO → UARTE. Preset base64, byte-verified. |
| sensors | SENS:OK + BTN:1/0 level-poll + TWIM + GPIOTE | F | BTN_A P0.14 active-low; `input_state` survives reboot (P43). |
| dma | TWIM DMA loopback | F | Driver-style take → RAM move → complete, like the JS pump. |
| extras | SAADC/TEMP/RNG/PWM OK | F | Depth proofs for TEMP/RNG/PWM (P110) alongside SAADC. |
| stubs | SPIM0/2/3 + PDM/QSPI/USBD/RADIO OK | F | START/STOP→STOPPED on all three SPIM instances (326B, rebuilt from `.s`); preset base64 byte-identical to `blinky/stubs_nrf.bin`. |
| uarte1 | UARTE1 TX DMA + RX DMA | F | `uarte1_nrf.s/.bin` (P110): `U1DATA` TX + 3 B RX, `U1TX:OK`/`U1RX:OK`; driver take → RAM move → complete (shared path). |
| spim23 | SPIM2 TX DMA + SPIM3 RX DMA | F | `spim23_nrf.s/.bin` (P110): 4 B TX + 4 B RX, `S2TX:OK`/`S3RX:OK`; SPI-NACK guard (`arm_nack`). |
| air | USB+RADIO loopback+PPI OK | F | Polls USBRESET first (Run replicates pre-signal). |
| c_irq | C TIMER0 IRQ + UART | F | C toolchain proof (bit-identical rebuild). |
| usbep / usbdev | SETUP + EPIN flash-DMA | F | `usbdev_nrf.c` flash-source DMA. |
| i2s | Streaming roundtrip | F | EXCLUDED from presets: needs patterned-RX + mailbox release only the test driver provides. |
| wdt | Expiry + double-reset | F | Pet-vs-unpetted semantics. |
| nfct | Field-select + frames | F | Field-detect/select state machine. |
| 2nd-run | `reset_state`, no leak | F | Every proof re-runs clean (BOOT_LOCK + UART lock discipline). |

| Wall-time (measured, raised 4x this round) | Value | Remark |
|---|---|---|
| Node WASM blinky | ~57–59 MIPS | 5M instr / ~85 ms, BOOT/BLINK/BLINK correct (raw core; was ~41–54 on the old meter). |
| Node WASM MPY banner | ~33 MIPS | Sustained through the bench-exact pump to the 105 B banner. |
| Node language faces (C/C++/BLE/matrix) | ~56–59 MIPS | c_irq ~36 (IRQ-heavy); all zero faults. |
| Browser raw core (same Chromium) | ~66 MIPS | Temp-page probe, reverted — vsync cap was the old 6 MIPS, never core speed. |
| Browser bench meter | ~6 → ~24 MIPS | 5x20K → 20x20K batch (same 20K quantum, duties in-loop); blinky + MPY banner ~5 s wall. |
| Native debug blinky test | 0.32–0.40 s | 5M-instr `cpu.run` + model — harness time, NOT core speed. Do not cite as MIPS. |
| Banner cost | ~150–260M instr | Was ~30 s browser @6 MIPS; now ~5 s @24 MIPS. |
| Pump batching | 20×[20K step+tick+pumpDma] per frame (was 5x; P54 duties-in-loop kept) | 20x20K probe-measured 6 ms wall in-browser; quantum unchanged so tick-starve behavior can't regress. |
| Profile | dev == release (byte-identical) | wasm-pack single profile; wasm-opt -O3 measured SLOWER (59→56), not shipped. |
| Package | `demo/pkg` 1.5MB committed, `.gitignore` removed | Deliberate: Pages serves it directly, no toolchain needed. |
| Vendored firmware | `demo/firmware/` ships MakeCode hex + 6 BLE `.bin` copies | Sources of truth stay `mc/built/` + `blinky/ble_fw/`; `../blinky`/`../mc` 404 under the `demo/` Pages root. |

## 5. Real-firmware scoreboard (all executed, zero CPU faults*)

| Firmware | Result | Remark |
|---|---|---|
| MicroPython v2.1.2 (direct-app `0x1C000`) | TRUE-seed park `0x200021B8/BB`, 200M/zero-fault/uart-0 (P113; wrong-seed `0x1AEF8` fault was a harness typo) | 2 AIRCR resets honored, RESETREAS SREQ, GPIO live, TWIM healthy (txT 117/rxT 1/ev 378, `0x19`+`0x39`, zero NACKs), serial TX-only stall (RX never init). Post-banner NULL fault notes kept in STATUS §6. |
| MakeCode `basic.showString("A")` (makecode 1.3.6, `mc/` gitignored) | Pre-scroll stall CONFIRMED (P112); shared gate with MPY (app-honor → same `0x1AEF8`; MBR-honor → `0x37F4F`/SVC3 park) | P57 zero-touch + P106 waiter forensics (kept in STATUS §6): run queue ONE fiber, scroll never created, TIMER4 0, DIR0 sticky. |
| Espruino 2v29 | Boots | CoreSight PID fix needed; console is P0.06 bit-bang, nothing TX in early windows. |
| Bootloader chain (`0x77000`) | Entry + FICR gather + benign post-UICR reset; 2nd reset CODED AIRCR | `0x78514` via tbb `0x78498` (r5=1); `0x783FE` park = post-AIRCR wait. r4==0 is SD-enable SUCCESS (`cbnz r4@0x7B636` skips validation on FAILURE; success → `0x7B5B4`+`0x7B568` → tbb reset #2 BY DESIGN). `0x7B5B4` = IPR22 validator (`236>>a` odd; IPR22=0 always fails — needs SD priorities). `0x784C4` = DFU-progress gate (`[0x20002DF1]`, `[0x2DFC]-[0x2DF4]` vs 59), not the r4 cause. MBR selector (`0x417`) never reads `0x10001200/204` (`0x0–0xB00` sweep) — P42 refuted, direct-app stays. MBR pass-2 needs SD priorities (shelved). |

## 6. LEFT — the 9 STATUS items, each with its next action

| # | Item | Status | Next action |
|---|---|---|---|
| 1 | REPL exec (`print(1+2)` → `3`) | CLOSED P116+P117 (native 125 B + in-browser `>>> print(1+2)\n3\n>>> `, zero faults/page-errors) | — |
| 2 | TX drops | Model done, no action (guarded 0/60, unguarded 60/60) | — |
| 3 | Bootloader full chain | PARKED, stays parked | Reopen only with a faulting config |
| 4 | MakeCode display content | PARKED, shared gate with (1) | Same NEXT as (1) |
| 5 | SPIM2/3 | Done + DMA-proven (P110 `spim23_nrf`: TX+RX DMA on both instances) + consumer wired (P114 `demo/parts/spidisplay.js` ST7789 240×240 on SPIM2, headless-verified) | — |
| 6 | Demo wall-time | Raised 4x (20x20K batch, ~24 MIPS meter, ~5 s MPY banner; no Rust change) | Vendored firmware ships in `demo/firmware/` |
| 7 | UARTE1 second-instance proof | Done (P110 `uarte1_nrf`: TX+RX DMA through shared take/complete) | — |
| 8 | RTC/PWM/RNG/TEMP/EGU depth | Done (P110 second proofs: COMPARE/OVRFLW, STOP/INTEN, DATARDY/INTEN, SHORTS/re-arm, channels/mask) | — |
| 9 | Bench crypto+QSPI pumps | Done (P109: ECB/AAR/CCM/QSPI live in pumpDma via shared `crypto.js`) | AAR has no wasm export (pump resolves present by design). |

## 8. BLE: is it fully done? (P58 verdict + P98–P103 build record)

Short answer: the firmware-visible BLE contract is fully answered and
proven over virtual air; the radio physics and the crypto math are not
(the per-peer bond store closed P114). P58 said "needs a BLE-enabled
work" — the SVC work happened anyway against the S132 headers with a
GCC conformance firmware + C face + headless mock + live Bumble air as
proof instead of a stock image (no shipped MPY/MC image enables the
stack to this day).

### Added (P98–P103): what the BLE face covers

All 67 S132 BLE SVC numbers are claimed by the `sd_ble` service
(`nrf52833-periph-wasm/src/sd_ble.rs`, numbers verified against the
Arduino nRF52 S132 headers, not guessed): common `0x60–0x69`, GAP
`0x70–0x8E`, GATTC `0x90–0x99`, GATTS `0xA0–0xAC`, L2CAP
`0xB0–0xB2`. The thumb SVC hook claims `0x60..=0xBF` first (r0 +
skip, else fall through to `raise_sync` — zero-cost when idle).

| Area | Done |
|---|---|
| Common | ENABLE with RAM-floor report; two-arg EVT_GET (length query, DATA_SIZE without popping, legacy drain); TX_PACKET_COUNT_GET per-link budget; UUID VS_ADD/DECODE/ENCODE; VERSION_GET; USER_MEM_REPLY + OPT_SET/GET as validated acks. |
| GAP central | ADDRESS_SET/GET, ADV_DATA_SET validation, ADV_START/STOP with state (active/directed/filter/whitelist) + directed-peer + validation + SCAN-whitelist IN_USE arbitration + ADV CONN_COUNT leg, SCAN_START (observer slot: BUSY/INVALID_STATE + S132 param/whitelist validation) / SCAN_STOP, CONNECT→staged air job→CONNECTED (CENTRAL role, real conn_params) / CONNECT_CANCEL, DISCONNECT with HCI reason echo, CONN_PARAM_UPDATE/PPCP/APPEARANCE/DEVICE_NAME as validated acks; TX_POWER_SET stores the S132-legal dBm set (readable via `ble_tx_power_dbm`). |
| GAP RSSI | RSSI_START/STOP validation; RSSI_GET answers the link level synchronously AND stages air sampling; completion posts RSSI_CHANGED. Bridge probes HCI_READ_RSSI on the live link first (SoftDevice-faithful) with adv-sighting fallback (`src:"conn"\|"adv"`), because LocalLink answers UNKNOWN_HCI_COMMAND (probed). |
| GATT client | All six discovery kinds (primary/relationship/characteristic/descriptor/attr-info/UUID-read) + multi-read + plain read with offset + write (REQ+CMD+SIGNED with 12B signature in tow + PREP queue/EXEC commit onto the table mirror; bytes copied at SVC time) + HV_CONFIRM; gattc envelope head + unpacked pads on every RSP (op echo at wire offset 8); 128-bit rows encode null+vendor type. |
| GATT server | Service/char/descriptor table with SoftDevice-shaped handles; struct-form VALUE_SET/GET (length query, offset checks, conn 0xFFFF allowed); ATTR_GET; per-link CCCD tracking with notify/indicate-bit gating (unsubscribed refuses); HVX staging with per-link TX budget (notify decrements, empty budget refuses; TX_COMPLETE refill on air drain); SERVICE_CHANGED (0xA7) gated indication (SC-enable bit at ENABLE + 0x2A05 CCCD indicate bit, header retval ladder) staging a tag-16 air job, peer confirm posts SC_CONFIRM with the conn head. |
| Pairing legs | AUTHENTICATE stages the handshake; all six peer-initiated request events (SEC_PARAMS_REQUEST 0x13 / SEC_INFO_REQUEST 0x14 / PASSKEY_DISPLAY 0x15 / KEY_PRESSED 0x16 / AUTH_KEY_REQUEST 0x17 / LESC_DHKEY_REQUEST 0x18, conn-first bodies); per-link state machine (Idle/Requested/PeerRequested/Accepted/KeyEntry/LescDhkey/EncryptPending); every reply SVC validated (accept needs a request, passkey shape-checked, OOB/DHKEY/keypress/encrypt/SEC_INFO each gated); S132 SEC_STATUS codes incl. the 0x29→0x85 fix; AUTH_STATUS conn-first; complete/fail post AUTH_STATUS (+CONN_SEC_UPDATE) with bonded/encrypted state feeding CONN_SEC_GET. Key bytes persist per peer in the bond store (hit/miss/delete); crypto math itself stays driver-side — documented. |
| L2CAP | Dynamic-CID register/unregister (range + capacity checks), TX staging with SVC-time byte copy, RX echo completion. |
| Multi-link + air | Per-link handles/RSSI/TX/security/pairing/CIDs; events carry their conn; pump + bridge + E2E prove two live links. Bridge (`tools/ble_air_bridge.py`): two Bumble peers on one LocalLink (battery 87 `PeerBatt` + twin 64 `PeerHR`, distinct addresses), per-job `peer` routing, `peer` echo on disc RSPs, per-peer ATT locks + global scan lock (no timeouts under load). |
| Proofs | 14 native sd_ble tests (byte-offset asserts, incl. unallocated-SVC NOT_SUPPORTED range rule) + SVC-hook proof in cpu/tests.rs; GCC `ble_conformance.c` + C face + `ble_pairing_fw.c` (CODAL-BLE-shaped JustWorks flow, 17 `BLEP:*` markers, 2nd-run clean) + `ble_roles_fw.c` (ADV/SCAN/whitelist/role-slot legs, 21 `BLER:*` markers, 2nd-run clean, bit-identical rebuild); headless MockBleSvc real-SVC flow 18/18 (incl. SIGNED/PREP/EXEC write legs + 8b driver-posted request/report/timeout legs + strict WRITE_RSP op echo at wire offset 8 + SERVICE_CHANGED/SC_CONFIRM leg + 7f ADV/SCAN roles legs); radio air mock stage 3 (CRC engine + whitening + interference, same surface as the native `crc_engine_whitening_interference_air` test); live E2E 42/42 over air (two links, 87-vs-64 reads); browser 16/16 (blinky + self-test pairing×2 + probes, zero page errors). |

### Left: the named, bounded gaps (none is a hidden fault)

| Gap | Why it stays |
|---|---|
| SMP crypto toolbox (P125) | `smp_crypto.rs`: P-256 ECDH + AES-CMAC + f4/f5/f6/g2 (bumble-frozen vectors, 4 tests); DHKEY_REPLY validates 96B peer-key buffers through real ECDH (INVALID_PARAM off-curve), OOB_DATA_GET derives f4 confirms, 7 wasm exports (`ble_lesc_*`, `ble_smp_*`). |
| Key / bond storage | CLOSED P114: per-peer LTK/IRK/CSRK/master-id store (hit/miss/delete + bridge `bond_keys` leg); SEC_INFO_REQUEST re-encrypts hit from store. |
| Central role only | CLOSED P114 (dial-in): `complete_peripheral_connect` posts CONNECTED with PERIPH role; bridge/pump `periph_connected` leg. ADV_START arms validation + whitelist/directed state (P119–P121: shape/IN_USE/CONN_COUNT legs, `ble_adv_state`); the bridge `periph_connected` leg completes dial-in over air. |
| No parameter enforcement | CLOSED P114 (events): request SVC validates; driver completion posts CONN_PARAM_UPDATE. No MTU/DLE/PHY SVCs exist in S132 form and none are synthesized. |
| No TX flow events | CLOSED P114: TX tokens refill per air packet + TX_COMPLETE posted with free count (pump `tx_complete` leg); NO_TX_PACKETS still gates staging. CLOSED P119: driver-posted completions for every previously never-posted leg — SEC_REQUEST, CONN_PARAM_UPDATE_REQUEST, SCAN_REQ_REPORT, GAP/GATTC/GATTS TIMEOUT, USER_MEM_REQUEST/RELEASE, RW_AUTHORIZE_REQUEST, SYS_ATTR_MISSING, SC_CONFIRM (proven by the mock's 8b strict-id drain). |
| GATTC write REQ/CMD only | CLOSED P119: SIGNED_WRITE (op 3, 12B signature in tow, short refuses INVALID_PARAM) + PREP_WRITE (op 4, offset queue per link) + EXEC_WRITE (op 5, commit/cancel onto the table mirror) all stage air jobs and complete with the op echo (mock's signed/prep/exec WRITE_RSP legs). Crypto MAC stays firmware-side — documented. |
| SoC/MBR SVCs unmodeled | Mutex/rand-pool/power/clock/PPI sd_ calls are out of scope for the BLE face; on-chip crypto keeps its own take/complete models. EXCEPTION: SoC flash events closed P114 (`sd_evt.rs` phase 1: SVC 16/82, NVMC-posted id 2/3, firmware proof). |
| No BLE-enabled stock image | MPY ships `MICROBIT_BLE_ENABLED: 0`; no shipped firmware exercises this face (proven by conformance fw + mock + E2E instead). |
| Virtual air, not RF | LocalLink peers, not spectrum; link-budget RSSI (P119: TX dBm minus path-loss, clamped [-127, 0], shared pure fn `radio_air_rssi_dbm`; RX completion stamps the packet's own level) with adv fallback; P124 interference floor (ambient dBm heats ED/CCA + RX stamp in log-power) + real CRC engine (CRCCNF/POLY/INIT, RXCRC latch) + nRF LFSR whitening (PCNF1.WHITEEN + DATAWHITEIV). No multipath/fading model. |

### Original P58 verdict (kept for the record)

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
| `demo/parts/crypto.js` (P109) | Shared AES-128/CCM (FIPS-197 `aesBlock`, `ctrCrypt`, `cbcMic`) | pumpDma + depth probes share one implementation (moved verbatim out of `mocks.js`). |
| `demo/index.html` pumpDma (P109) | Live ECB/AAR/CCM/QSPI pumps | ECB encrypts in place, AAR resolves present, CCM CTR+MIC-4 encrypt/decrypt+verify, QSPI 64 KB image (AND-only program, `0xFF` erase). Idle cost: one take each. |
| `blinky/stubs_nrf.s` + `.bin` | SPIM2/3 START/STOP handshake (270B→326B) | GAS-verified halfwords, rebuilt via `docs/README.md` flags, `nrf_stubs` test green; preset base64 synced byte-identical. |
| `blinky/uarte1_nrf.s` + `.bin` (P110) | UARTE1 TX+RX DMA firmware proof | GAS-verified, asm recipe (`as` + `ld -T blinky/link_nrf.ld` + `objcopy`), bit-identical; `nrf_uarte1_instance_dma_roundtrip` green. |
| `blinky/spim23_nrf.s` + `.bin` (P110) | SPIM2 TX + SPIM3 RX DMA firmware proof | Same asm recipe, bit-identical; `nrf_spim23_dma_roundtrip` green (needs the SPI-NACK guard). |
| `nrf52833-periph-wasm/src/peripherals/scb.rs` | AIRCR mask `0F04` + self-clear + test | `0x05FA0004` now latches reset (was swallowed). |
| `nrf52833-periph-wasm/src/peripherals/twim_nrf.rs` | Raw ADDRESS + exact-then-`>>1` + tests | `0x72` finds `0x39` tap; readback = written value (silicon). |
| `docs/COVERAGE.md` | This file | Table audit (uncommitted, per order). |

```
cargo test -- --test-threads=1   # 238 green (parallel ~30/31 on the P114 tree; single-threaded stays the gate by convention)
node demo/parts/smoke.mjs        # parts green
node demo/parts/handshake.mjs    # 18/18 vs the built pkg
node demo/parts/ble_live_e2e.mjs # 42/42 over air (bridge on :18771)
wasm-pack build nrf52833-periph-wasm --target web --out-dir ../demo/pkg
rm -f demo/pkg/.gitignore        # pkg intentionally committed
```

| Probe hygiene (AGENTS.md) | Rule |
|---|---|
| Location | `/tmp/opencode/**` only (ephemeral — re-create per session). |
| Page edits | One-line `window.__dbg` TEMP edits, always reverted (`grep __dbg` = 0). |
| Native harnesses | TEMP files, deleted after run (`git checkout -- src/cpu/mod.rs` pattern). |
| Core | Never edit `src/cpu/` for board issues; never invent a second clock (`INSTRUCTION_COUNT` + `tick()` + `tick_n` only). |
