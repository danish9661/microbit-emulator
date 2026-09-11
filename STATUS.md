# micro:bit v2.2 (nRF52833) emulator — implementation status

Audited 2026-09-11 by cross-checking all 70 `monox/nrf52833.svd`
peripherals against `src/peripherals/`, running the suite
(**164 passed, 0 failed**), reading every model header, and replaying
the live firmware runs. Grades: **F** = functional (timed, IRQs,
driver take/complete, firmware proof), **H** = handshake
(TASKS/EVENTS/INTEN minimum, no timed behavior or no consumer),
**–** = missing/deliberately omitted.

## 1. Peripheral coverage (live map `new_wasm` + SVD map `from_svd`)

| SVD peripheral(s) | Model | Grade | Notes |
|---|---|---|---|
| CLOCK, POWER (shared `0x40000000`) | `clock_nrf.rs` | F | HF/LF STARTED events+STAT, USBDETECTED/USBPWRRDY, RESETREAS+SREQ latch, GPREGRET, RAMSTATUS, LFCLKSRC; POWER_CLOCK IRQ 0 |
| RADIO `0x40001000` | `radio_nrf.rs` | F | TX take / RX inject, shortcut-capable; bare-metal loopback proven (`air_nrf`) |
| UARTE0+UART0, UARTE1 | `uarte_nrf.rs` | F | 1-byte TX DMA + RXDMA ring; OVERRUN, ERROR, TXSTOPPED (`0x158`/INTEN 22, fixed P20); UARTE1 modeled, no dedicated proof |
| TWIM0/TWI0/SPIM0/SPIS0/TWIS0/SPI0, TWIM1 family, SPIM2, SPIM3 | `twim_nrf.rs` | F | Mode-blind shared-base; SHORTS, NACK-after-~6000-instr without slave, LASTTX/STARTRX/SUSPEND; SPIM2/3 DMA works, I2C-tap routing for them open (stale TODO P8 comment) |
| NFCT | `nfct_nrf.rs` | H | Enable/events minimum; no tag emulation |
| GPIOTE | `gpiote_nrf.rs` | F | 8 ch event/task, edge detect vs pull-up inputs, PORT event, OUT tasks drive GPIO |
| SAADC | `saadc_nrf.rs` | F | EASYDMA take/complete, result pump |
| TIMER0–4 | `timer_nrf.rs` | F | Prescaler/bitmode/SHORTS-CLEAR, CAPTURE snapshots live counter, INTEN 16+i, IRQs 8–10/26/27 |
| RTC0–2 | `rtc_nrf.rs` | F | All three instances, IRQs 11/17/36 |
| TEMP | `temp_nrf.rs` | H | Plausible die-temp readout, DATARDY/INTEN |
| RNG | `rng_nrf.rs` | F | Deterministic LCG, VALRDY/SHORTS-to-STOP |
| ECB | `misc_nrf.rs` | F | take/complete, FIPS-197 AES-128 proof (`nrf_ecb_aes128_fips_vector`); crypto runs driver-side |
| AAR | `misc_nrf.rs` | F | take/complete, RESOLVED/NOTRESOLVED; **no wasm export** (JS can't drive it) |
| WDT | `wdt_nrf.rs` | F | Expiry + reboot semantics, double-reset proof; RR reload by firmware (no JS export needed) |
| QDEC | `qdec_nrf.rs` | H | START→SAMPLERDY, host-steppable accumulator, no wasm export |
| COMP (+LPCOMP alias, shared base) | `comp_nrf.rs` | H | Enable/events minimum |
| EGU0 (+SWI0 alias) | `egu_nrf.rs` | H | Trigger/status minimum |
| EGU1–5 (+SWI1–5 aliases) | – | – | **MISSING live slots** (`0x40015000–0x40019000` unmapped in `new_wasm`; SVD path covers them) |
| MWU | `misc_nrf.rs` | H | Region watch minimum |
| PWM0–3 | `pwm_nrf.rs` | F | All four instances, IRQs 28/33/34/45; loop/decoders |
| PDM | `pdm_nrf.rs` | F | take/complete sample pump |
| NVMC | `nvmc_nrf.rs` | F | READY/READYNEXT always-1, WEN/EEN staging, erase take/complete (driver applies 0xFF) |
| PPI (+CHG groups, no FORK — no register exists) | `ppi_nrf.rs` | F | Direct dispatch + group EN/DIS |
| I2S | `misc_nrf.rs` | F | SVD-verified offsets, START-staged take_rx/take_tx, TX capture FIFO, streaming proof |
| USBD | `usbd_nrf.rs` | F | ENABLE/PULLUP/EPINEN, USBRESET+SETUP inject, EPIN/EPOUT take/complete; C proof (`usbdev_nrf.c`, flash-source DMA) |
| QSPI | `qspi_nrf.rs` | F | Registered image, AND-only program, 4K/64K erase, take/complete + JS backend exports |
| FICR/UICR | `ficr_uicr.rs` | F | PART=`0x52833`, sizes; UICR RAM store (BOOTLOADERADDR-gated boot depends on it) |
| P0/P1 | `gpio_nrf.rs` | F | OUT/DIR/CNF, inputs idle-HIGH (active-low buttons), combined `0x50000000` block |
| NVIC/SCB/SysTick/MPU/FPU/DWT/ITM/STIR/DEMCR | core files | F | MPU enforced w/ MemManage+escalation; FPU SP (CPACR gate, no lazy stacking — documented); CoreSight `0xF0000000` reads 0 |
| CCM (`0x4000F000`, shares AAR base) | – | – | Deliberately unmodeled (would alias AAR's task map) |
| ACL (`0x4001E000`, shares NVMC base) | – | – | Registers unhandled (reads 0); SPU protection out of scope |

## 2. CPU core (`src/cpu/`, ~8.4k lines)

Thumb-1/Thumb-2 baseline + DSP/SIMD (`smul/smlal/qadd/ssat`…) + VFPv4-SP
(GAS-verified probes in `docs/*.s`), IT blocks, SVC/exception entry
with priority gating + escalation, SysTick, WFI/WFE sleep + event
register, MPU-checked fetch/load/store, loud `CpuFault` on anything
unimplemented (by design — never silent). One core bug found and fixed
via firmware (`TST`-as-`CMP`, see `docs/cpu_bug.md` + regression test).

## 3. Tests — 164 green (`cargo test`)

- 106 integration tests (`src/cpu/tests.rs`): 11 GCC-built firmware
  proofs (`blinky_nrf`, `sensors_nrf`, `extras_nrf`, `stubs_nrf`,
  `dma_nrf`, `air_nrf`, `c_irq_nrf.c`, `usbep_nrf`, `usbdev_nrf.c`,
  `i2s_nrf`, `wdt_nrf`, +2nd-run reset-state checks each).
- ~58 unit tests at the peripheral level (register handshake,
  SHORTS/NACK/OVERRUN/CAPTURE, FIPS-197, reboot latch, TXSTOPPED).
- Thinnest: QDEC/COMP/NFCT/EGU/RTC/PWM/TEMP/RNG (1 handshake each).

## 4. JS API + demo (`demo/`, API frozen v1)

~60 wasm exports; demo wires clocks, GPIO/buttons (active-low),
matrix (DIR+OUT), UARTE TX/RXDMA, TWIM taps, SAADC/PDM/USBD/NVMC/
radio pumps, I2S silence/capture, watchdog-reset reboot, sleep-aware
frame (`tick_n` + wake), Intel-HEX loader (type-02 + UICR), and a
MicroPython direct-app boot button. Parts: matrix pins, LSM303
(WHO_AM_I `0x33`/`0x40`), SSD1306, all green via `smoke.mjs`.
`microbit-v2-emulator@0.1.0` npm package defined (not published —
no evidence of a publish step). Driver-API gaps (no consumer yet):
ECB/AAR take-complete, QDEC stepping, COMP/NFCT/EGU beyond handshake.

## 5. Real-firmware results (all executed, zero CPU faults)

- MicroPython v2.1.2: boots (MBR-param seeds + sleep-aware pump),
  uBit.init() completes, **banner + `>>> ` prompt print** (native and
  in headless Chrome), input bytes land in the DMA ring.
- MakeCode (`basic.showString`, locally built): boots to scheduler,
  TIMER4 display refresh runs (rows strobe, blank — content never
  drawn, DIR stays 0 which is correct pre-first-show).
- Espruino 2v29: boots (needed the CoreSight PID map); console is
  P0.06 bit-bang serial, nothing transmitted in early windows.

## 6. LEFT — prioritized

1. **REPL exec (`print(1+2)` → `3`)**. Two known blockers, both
   characterized in plan P16–P21: (a) deterministic post-banner NULL
   virtual call (`bx r3`, r3=0 @`0x4F75A`, input-independent,
   tick-gated; absent in-browser) — prime suspect is a TX_EMPTY
   bus-listener reached on ring drain; next step is walking the
   messageBus listener list at fault. (b) Prompt composed in TX ring
   but never DMA-staged (queued + `is_tx` false + no kick source
   found); readline never consumes a non-empty RX ring. Field map
   (`P21`) makes both bounded.
2. **TX byte drops** (~3% single-byte N+1 substitutions, cosmetic).
   Exonerated: pump read/completion timing, STOPTX arm, stack/heap
   collision. Open: exact double-stage source.
3. **Bootloader full chain** (MBR→BL→SD→app; direct-app boot works
   around it). BL reset-loops on a validation error; next step is
   anchored disassembly from the BL vector + capturing r0 at the
   init-runner compare.
4. **MakeCode display content** (refresh runs blank) + BLE events
   (no radio attempts; needs SD event synthesis, see below).
5. **EGU1–5 live slots** (15-line table fix + handshake test).
6. **Stale artifacts**: `blinky/blinky.bin` (STM32 binary, superseded
   by `blinky_nrf.bin`), `monox/stm32f407.svd` (reference leftover),
   stale `TODO P8` comment in `twim_nrf.rs`.

## 7. Deliberately out of scope

SoftDevice event synthesis (`sd_evt_get` pump — the one explicitly
deferred workstream), STM32/UNO R4/M0+/DAPLink targets (deleted, only
comment references remain), third-party-framework boot quirks (Arduino
Primo binary needs an nRF52832 bootloader), lazy FPU stacking, CCM
(Task-map alias with AAR), publish to npm.

## 8. Verify

```
cargo test                       # 164 green (crate dir)
node demo/parts/smoke.mjs        # parts green
wasm-pack build nrf52833-periph-wasm --target web --out-dir ../demo/pkg
```
Firmware proofs rebuild with `docs/README.md` recipes (xpack GCC
14.2.1 via arduino packages; C recipe verified bit-identical).
MicroPython REPL state reproduces from `micropython-microbit-v2.1.2.hex`
+ plan P16 recipe; MakeCode from `makecode build` + same recipe.
