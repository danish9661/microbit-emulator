# micro:bit v2.2 emulator — build plan (nRF52833 only)

Target: BBC micro:bit v2 / v2.2 target MCU = Nordic nRF52833
(Cortex-M4F, 64 MHz, 512 KB flash @ 0x00000000, 128 KB RAM @ 0x20000000).
v2.2 board rev = same SoC, minor board change.

OUT OF SCOPE (explicit):
- STM32 (all STM32 peripheral models deleted, not rewritten)
- Arduino UNO R4 / Renesas RA4M1 / Cortex-M33 (different core, not reusable)
- Interface MCU: KL27 M0+ (v2.00) / nRF52820 M4 (v2.2x) running DAPLink.
  Not emulated. Replaced by JS: hex-drop loader + UART console + reset.
- SoftDevice BLE stack (SVC interface). Target non-SoftDevice firmware first.

## 0. Baseline (done)
- git backup: `snapshot: STM32 baseline before nRF52833 v2.2 port`
- Reuse target: `src/cpu/` (thumb decoder, Cpu, FlatMemory trait, regs S0-S31).
  Never hand-edit `cpu/` for board problems.

## 1. KEEP (generic ARM)
`cpu/*`, `peripherals/nvic.rs,scb.rs,systick.rs,mpu.rs,fpu.rs,dwt.rs,itm.rs,stir.rs`,
`Peripheral` trait + `tick_n` + `INSTRUCTION_COUNT` clock, `*_tap` pattern,
`docs/*.s` probe method (same armv7e-m + fpv4-sp-d16).

## 2. DELETE (STM32-only, ~25 files)
`rcc,flash,pwr,gpio,tim,usart,spi,i2s,sai,i2c,adc,dac,can,sdio,dcmi,fsmc,ltdc,exti,syscfg,dbgmcu,cryp,hash,eth,crc,rtc,rng,wwdg,iwdg,qspi_stm32`
+ `monox/stm32f407.svd`, `blinky/blinky.bin` (linked at 0x08000000, useless),
+ STM32 globals in `system.rs` (ETH_*, CAN_STAGED, AUDIO_WAV, DCMI_FRAME, FLASH_PROGRAMMING),
+ STM32 exports in `lib.rs` (can_inject, tim_inject_capture, eth_*, ltdc_*, dcmi_*, fsmc_*, audio_*).
Reason: Nordic uses TASKS/EVENTS/SHORTS, zero register overlap. Rewrite = delete 95% anyway.

## 3. REWIRE
- `cpu/mem.rs`: `flash_base 0x08000000 -> 0x00000000`. Loader reads SP/PC from 0x0/0x4.
- `peripherals/mod.rs new_wasm/from_svd`: replace STM32 base table with nRF map.
- Crate + folder renamed `stm32-periph-wasm` -> `nrf52833-periph-wasm` (done P7c).
- SVD: fetch `nrf52833.svd` from Nordic MDK into `monox/nrf52833.svd`.

## 4. ADD (Nordic, in order)
1. `ficr_uicr.rs` (0x10000000/0x10001000, constants+storage, boot hangs without it)
2. `clock.rs` (0x40000000 HFCLK/LFCLK TASKS/EVENTS)
3. `power.rs` + `nvmc.rs` (stub READY + erase/program handshake)
4. `gpio_nrf.rs` (P0 0x50000000 32pins + P1 0x50000300 10pins, OUT/IN/DIR/CNF)
5. `timer_nrf.rs` (TIMER0-4) + `rtc_nrf.rs` (RTC0-2) + SysTick delays
6. `uarte_nrf.rs` (UARTE0 console, lifeline)
7. `twim_nrf.rs` + `gpiote_ppi.rs` (LSM303 sensor, buttons, matrix needs PPI dispatch)
8. `saadc.rs,temp.rs,rng_nrf.rs,pwm.rs,pdm.rs,qspi_nrf.rs,usbd.rs`
9. `radio.rs` last. Board: 5x5 matrix via GPIO rows/cols, BTN_A P0.14 BTN_B P0.23.

## 5. Firmware strategy
bare-metal blinky (flash@0x0, UART marker) -> UART echo -> CODAL blinky
(codal-microbit-v2, ARM GCC) -> MicroPython/Zephyr hello. No full DAL first.

## 6. Validation
Per-peripheral: boot marker + functional marker + 2nd consecutive run
(no state leak, `reset_state`). `cargo test` green before claim.
Minimum boot prove: `synth_vector_boot` + new `nrf_boot_flash_at_zero` test.

## 7. Phases
P1: strip + rewire + FICR/CLOCK/GPIO/NVMC (done)
P2: TIMER/RTC/UARTE + blinky marker (done)
P3: TWIM/GPIOTE/PPI + matrix/buttons/sensor (done)
P4: SAADC/TEMP/RNG/PWM/PDM/QSPI/USBD + CODAL (done, minus CODAL build)
P5: RADIO + browser page sweep (done: stubs + demo page)
P6: EASYDMA take/complete + PPI dispatch + USBD reset + RADIO loopback (done)
P7: SVD validation + IRQ audit + remaining stubs + cleanup (done)
P8: real-world firmware gate (done, see below)

## 8. MicroPython v2.1.1 boot findings (2026-09-11, probe, not committed)

Image: official release hex (SoftDevice + app, 450KB) split with
`blinky/hex2bin.py` (handles type-02 segments + UICR extras) into a
512KB flash bin + UICR NRFFW words (`0x10001014: 00070700 0007e000`).
Release: MicroPython v2.1.1
(github.com/microbit-foundation/micropython-microbit-v2/releases —
re-download per session, /tmp is reaped; do NOT commit the 512KB blob).

Observed over 60M+ instructions, zero CPU faults:
- MBR/SD handoff issues two `SYSRESETREQ`s (AIRCR wait-loop); the driver
  must honor `is_watchdog_reset_requested()` by rebooting from the vector
  table (covered by `sysresetreq_latches_reboot_request`). UICR NRFFW must
  be seeded or the first stage misbehaves.
- App reaches main firmware: manages NVIC ISER (UARTE0/GPIOTE/SAADC/
  TIMER1/TIMER3), no faults, parks in an SVC-driven SoftDevice wait loop.
- No timer IRQ ever fires (SysTick/RTC0 never configured that far), no
  USBD pullup (REPL is USB-only), no matrix/NVMC traffic.

Verdict: boot chain works; REPL + scheduler progress are gated on P9.

## 9. Next (P9): SoftDevice-event + USB endpoint modeling

P9a USBD endpoint DMA: DONE (EPIN/EPOUT take-complete, SETUP inject,
`usbep_nrf` firmware proof). MicroPython never pulls USB up that far,
so no live traffic yet — expected until the app reaches USB init.

P9b MicroPython steady-state forensics (2026-09-11, read-histogram probe):
- The 0x26048 loop is a 64-bit deadline busy-wait fed by TIMER0
  CAPTURE-then-read-CC3 (no COUNTER register exists). Our CAPTURE stub
  was a no-op -> frozen time -> infinite wait. FIXED (TimerNrf captures
  `counter` now; `capture_snapshots_counter` regression test).
- Same class of bug fixed in SysTick: VAL never counted, COUNTFLAG
  missing. Now a real down-counter (INSTRUCTION_COUNT-partitioned).
- After the CAPTURE fix the firmware advances to RAM execution
  (0x200021BA), matrix rows live, NVIC IRQs managed (UARTE0/GPIOTE/
  SAADC/TIMER1/TIMER3), zero faults over 700M+ instructions.
- VECTACTIVE sampling proves live IRQ delivery: TIMER1 handler
  (vector 25) entered ×27 per window, TIMER3/4 firing. The main thread
  idles;BUTTON/USB/UART stimuli don't advance it further yet.
- Remaining gate for REPL: USB stack start (pullup never asserted that
  far) + SoftDevice event pump (`sd_evt_get` has nothing to return —
  separate project per scope lock, NOT faked).

P9c left: EASYDMA completion IRQs (INTEN-gated), NVMC erase staging.

## 10. P10: POWER/USB readiness + MPY steady state (2026-09-11)

- POWER model completed from SVD ground truth: USBDETECTED/USBPWRRDY
  (gated on USBD ENABLE), USBREGSTATUS VBUS+OUTPUTRDY, RESETREAS with
  SREQ latched on AIRCR reboot (write-1-clear), GPREGRET retention,
  RAMSTATUS/MAINREGSTATUS, LFCLKSTAT/SRC, INTEN. Fixes a real boot
  hazard class (TinyUSB gates attach on USBPWRRDY).
- MicroPython steady state after the CAPTURE fix: RAM execution,
  matrix live, TIMER1/3/4 IRQs delivering (VECTACTIVE-proven), zero
  faults over 700M+ instr. It never enables USBD in that window, so no
  host traffic exists to drive: REPL needs a USB *host* stack
  (enumeration against TinyUSB) — that is P11, and it is bounded work
  (SETUP/DATA/STATUS stages on the P9a endpoint primitives), not a
  peripheral gap.
- SoftDevice event pump: NOT needed for REPL-idle (300k-PC trace shows
  zero SVCs in steady state). Stays out of scope per §0; RADIO loopback
  covers bare-metal BLE.
- CODAL full build: blocked on orchestration, not sources. Needs the
  `codal` python tool (pip offline here), codal-core + nRF5-SDK deps,
  and era GCC (repo targets ~GCC 9/10; ours is 14.2.1/7-2017q4).
  Recipe: pip install codal, clone codal-microbit-v2 + build profile,
  `codal build`. Our GCC IRQ/C++ path is already proven by P8a's C demo.

## 11. Next (P11): USB host enumeration -> REPL banner

Drive SETUP/DATA/STATUS against the P9a primitives until TinyUSB
enumerates and the MicroPython banner appears on USB CDC. Pure driver
work (test-side first, then demo pump); no new peripheral registers.

## 12. P11a TWIM bus errors + UARTE RX truth (2026-09-11)

- TWIM mistakes fixed from SVD ground truth: RXSTARTED/TXSTARTED were
  SWAPPED (0x148=SUSPENDED, 0x14C=RXSTARTED, 0x150=TXSTARTED), LASTRX/
  LASTTX/SUSPEND/RESUME/SHORTS/ERRORSRC missing. Added + SHORTS chains
  (LASTTX_STOP/STARTRX/SUSPEND, LASTRX_*) and a NACK model: a DMA
  transfer with no tap slave at ADDRESS fails after the address phase
  (~6000 instr) with ERROR + ANACK + STOPPED, like silicon without ACK.
  MicroPython's accel probe NACKs live without a slave and proceeds.
- Virtual LSM303 (WHO_AM_I_A 0x33 / _M 0x40) answers MPY's probe:
  periodic I2C traffic serviced, zero faults. Sensor-data path proven.
- UARTE RXD is a single slot, not a queue: unread arrival = OVERRUN
  (ERRORSRC bit 0, write-1-clear). Test drivers must pace input to
  RXDRDY-cleared, like silicon.
- MicroPython steady state: alive main loop (sensor reads + timer IRQs
  live, zero faults over 850M+ instr), REPL input consumed on UARTE but
  no output yet (no echo/prompt). Display never configured, USB never
  started. Next: find what the REPL waits on (BLE-NUS vs USB-CDC binding).

## 13. P11b REPL forensics (2026-09-11, source-grounded)
- The v2.1.1 firmware IS CODAL-based (MicroPython on micro:bit via
  CODAL): REPL stdin/stdout = `uBit.serial` = UARTE0
  (`src/codal_app/mphalport.cpp`), NOT USB. "MicroBitUART" is the
  `microbit.uart` module type name, not BLE.
- UIPM is I2C to the interface chip (addr 0x70, `MicroBitPowerManager`),
  gated on the irq1 GPIO line — silent in emulation, correctly so;
  its retries terminate on working time, not a stall.
- Park anatomy: main fiber in a `delay_us` RAM leaf called from a
  retry/parser path (0x209AA retry x20 + digit parser at 0x20A9A+),
  with live sensor traffic and timer IRQs. I2C log shows clean sensor
  init (WHO_AM_I, CTRL writes) — virtual LSM303 answers correctly.
- Open: which init phase owns that retry loop, and what the REPL
  thread waits on. Candidates: display init ordering, BLE-disabled
  pairing check (`MICROBIT_BLE_PAIRING_MODE=1` in codal.json despite
  `MICROBIT_BLE_ENABLED=0`), button polarity (our inputs default LOW;
  real board pulls buttons HIGH — pressed reads LOW, so we may report
  both buttons stuck pressed!). Button default polarity is next to verify.

## 14. P12 stub completions (2026-09-11)

- ECB: real offsets (STARTECB/STOPECB/ENDECB/ERRORECB/INTEN, DATAPTR),
  take/complete driver flow, FIPS-197 AES-128 round-trip proof
  (`nrf_ecb_aes128_fips_vector`). Crypto itself runs driver-side
  (model has no RAM handle); AAR likewise (take/complete +
  RESOLVED/NOTRESOLVED). CCM stays unmodeled (shares AAR's base with a
  different task map — aliasing would lie about both).
- PPI groups: CHG[0..5] masks + TASKS_CHG[n].EN/DIS; FORK field removed
  (no register mapping exists to ever set it).
- QSPI backend: registered image, AND-only program, 4K/64K erase,
  take/complete for read/write/erase + JS exports.
- Deferred with reason: WDT expiry (a wrong deadline breaks running
  firmware; needs a WDT-pet firmware proof first), I2S streaming
  (needs sample-source infra stripped in P7c; zero consumers).

## 15. P13 REPL end-to-end (2026-09-11): input path proven, app gated on SD

- NRF52Serial source (codal-nrf52, cloned): TX is 1-byte DMA per char,
  RX drains via DMA ring (`dataReceivedDMA` from RXD.PTR RAM) + RXDRDY
  IRQ. Input bytes only reach the REPL through the DMA ring — the
  `uarte_take_rxdma/complete_rxdma` driver API exists for exactly this
  (demo pump wires it).
- Live proof the app never prints: 200M+ instr with full RX+TX DMA
  pumping, `take_txdma` never stages, TXD byte never written. Not a
  model gap on the output path.
- IPR words prove the SOFTDEVICE IS ENABLED (app priorities 2/1 in
  TIMER/UARTE slots — SD-reserved pattern, not CODAL MicroBit::init
  values). Main()'s `NVIC_SetVector(RADIO)` never lands (live vector
  == flash vector), RXD.PTR stays 0, display DIR stays 0: uBit.init()
  never completes; the app parks waiting on SoftDevice events.
- 300k-PC trace: ZERO SVCs in steady state (the wait is not an SD SVC
  spin), zero WFI-sleep, live TIMER1/3/4 IRQs (VECTACTIVE-proven).
- Verdict: every peripheral + driver path on the REPL route is modeled
  and proven. Progress now requires SoftDevice event synthesis
  (sd_evt_get responses, BLE/RTC event pump) — the explicitly
  out-of-scope workstream from §0. Nothing further is actionable
  without it.
