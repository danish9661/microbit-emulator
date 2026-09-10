# AGENTS.md — micro:bit v2.2 (nRF52833) emulator rules

## Scope lock
- Target only: nRF52833 Cortex-M4F. No STM32, no UNO R4 (M33), no M0+/DAPLink.
- Never edit `src/cpu/` (thumb decoder, Cpu stepping, regs) for board issues.
- Never invent a second clock. Use `INSTRUCTION_COUNT` + `tick()` + `tick_n` batching.
- No limitations talk: if blocked, model minimal stub (READY=1, EVENTS=0) and log TODO.

## How to add a Nordic peripheral
1. New file `src/peripherals/<name>_nrf.rs`, struct + `Default`.
2. `impl <P> { pub fn new(name: &str) -> Option<Box<dyn Peripheral>> }` — match SVD name only.
3. `impl Peripheral for <P>`: `read(sys,offset)`, `write(sys,offset,value)`, `tick(sys)` if timed, `as_any_mut`.
4. Register in BOTH `Peripherals::from_svd` and `Peripherals::new_wasm` with correct base:
   CLOCK 0x40000000, POWER 0x40000000+?, NVMC 0x4001E000, FICR 0x10000000, UICR 0x10001000,
   P0 0x50000000, P1 0x50000300, TIMER0 0x40008000 (+0x1000 stride), RTC0 0x4000B000,
   UARTE0 0x40002000, TWIM0 0x40003000, SPIM0 0x40003000-alias, SAADC 0x40007000,
   TEMP 0x4000C000, RNG 0x4000D000, PWM0 0x40021000, PDM 0x4001D000, QSPI 0x40029000,
   USBD 0x40027000, RADIO 0x40001000, GPIOTE 0x40006000, PPI 0x4001F000.
   Verify against nRF52833 Product Specification, not STM32 headers.
5. Nordic style: TASKS_* write-1-to-start, EVENTS_* read-clear by write-0, INTENSET/CLR.
   Unlisted offsets read-as-0, writes ignored (HALs probe reserved).

## Memory / boot
- Flash 512KB @ 0x00000000, RAM 128KB @ 0x20000000. VTOR default 0.
- `FlatMemory::new(512*1024, 128*1024)`, `load(bin, 0x0)`, SP/PC from 0x0/0x4.
- FICR constants (DEVICEID/INFO) + UICR storage or boot hangs.

## Validation
- `cargo test` must stay green after each peripheral. New peripheral = new test:
  boot marker + functional marker + 2nd run (reset_state, no leak).
- Prefer native `WasmSystem::new()` + `Cpu` + `FlatMemory` harness (see `cpu/tests.rs:boot`).
- No JS/driver round-trip in Rust tests.

## Delete list (do not revive)
rcc,flash_stm32,pwr_stm32,gpio_stm32,tim_stm32,usart,spi,i2s,sai,i2c_stm32,adc_stm32,dac,can,sdio,dcmi,fsmc,ltdc,exti,syscfg,dbgmcu,cryp,hash,eth,crc,rtc_stm32,rng_stm32,wwdg,iwdg,qspi_stm32.
Keep: nvic,scb,systick,mpu,fpu,dwt,itm,stir.

## Workflow
- Small diffs, one peripheral per commit. `cargo test` + `git commit` each step.
- If unsure about encoding: assemble probe with ARM GCC (`docs/README.md` flags), read halfwords, never guess.
