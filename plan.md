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

- USBD endpoint DMA (ENDEPIN/EPDATASTATUS, descriptors at known RAM) so
  TinyUSB enumerates and the MicroPython REPL banner appears on USB.
- SoftDevice event pump: `sd_evt_get` must eventually return events
  (BLE/RTC), else the app SVC-spins forever; RADIO air already loops back.
- Then CODAL full build (`/tmp/codal` clone exists; needs the codal-core
  orchestrator + era-appropriate GCC, out of scope for P8).
