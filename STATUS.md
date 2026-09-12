# micro:bit v2.2 (nRF52833) emulator — implementation status

Audited 2026-09-12 by cross-checking all 39 `monox/nrf52833.svd`
peripherals against `src/peripherals/`, running the suite
(**185 passed, 0 failed**), reading every model, and replaying the
live firmware runs. Grades: **F** = functional (timed, IRQs,
driver take/complete, firmware proof), **H** = handshake
(TASKS/EVENTS/INTEN minimum, no timed behavior or no consumer),
**–** = missing/deliberately omitted.

## 1. Peripheral coverage (live map `new_wasm` + SVD map `from_svd`)

| SVD peripheral(s) | Model | Grade | Notes |
|---|---|---|---|
| CLOCK, POWER (shared `0x40000000`) | `clock_nrf.rs` | F | HF/LF STARTED events+STAT, USBDETECTED/USBPWRRDY, RESETREAS+SREQ latch, GPREGRET, RAMSTATUS, LFCLKSRC; POWER_CLOCK IRQ 0 |
| RADIO `0x40001000` | `radio_nrf.rs` | F | PCNF-length TX take / RX completion+inject, CRCERROR inject, RSSI, SHORTS; bare-metal loopback proven (`air_nrf`) |
| UARTE0+UART0, UARTE1 | `uarte_nrf.rs` | F | 1-byte TX DMA + RXDMA ring; OVERRUN, ERROR, TXSTOPPED (`0x158`/INTEN 22, fixed P20); UARTE1 TX/RX fully routed (was UARTE0-locked), no dedicated UARTE1 proof |
| TWIM0/TWI0/SPIM0/SPIS0/TWIS0/SPI0, TWIM1 family, SPIM2, SPIM3 | `twim_nrf.rs` | F | Mode-blind shared-base; SHORTS, NACK-after-~6000-instr without slave, LASTTX/STARTRX/SUSPEND; TWIS/SPIS slave engines (`twis_master_write/read`, `spis_exchange`); SPIM tap routing incl. SPIM2/3; SPIM2/3 I2C-tap routing still open |
| NFCT | `nfct_nrf.rs` | F | Field-detect/select state machine, frame TX/RX take-complete, C+S proof (`nfct_nrf.s/.bin`, `nrf_nfct_field_select_and_frames`) |
| GPIOTE | `gpiote_nrf.rs` | F | 8 ch event/task, edge detect vs pull-up inputs, PORT event, OUT tasks drive GPIO |
| SAADC | `saadc_nrf.rs` | F | CH config/limits, LIMIT events, RESULTDONE/STOPPED, EASYDMA take/complete + result pump |
| TIMER0–4 | `timer_nrf.rs` | F | Prescaler/bitmode/SHORTS-CLEAR, CAPTURE snapshots live counter, INTEN 16+i, IRQs 8–10/26/27 |
| RTC0–2 | `rtc_nrf.rs` | F | All three instances, IRQs 11/17/36 |
| TEMP | `temp_nrf.rs` | F | Driver-settable die temp (`temp_set_celsius`), DATARDY/INTEN |
| RNG | `rng_nrf.rs` | F | Deterministic LCG, VALRDY/SHORTS-to-STOP |
| ECB | `misc_nrf.rs` | F | take/complete, FIPS-197 AES-128 proof (`nrf_ecb_aes128_fips_vector`); crypto runs driver-side |
| AAR (shares `0x4000F000` with CCM) | `misc_nrf.rs` | F | take/complete, RESOLVED/NOTRESOLVED; **no wasm export** (JS can't drive it) |
| CCM (mode bit on shared AAR base) | `misc_nrf.rs` | F | take/complete, CTR+MIC roundtrip proof; no separate slot (would alias AAR's task map) |
| WDT | `wdt_nrf.rs` | F | Expiry + reboot semantics, double-reset proof; RR reload by firmware (no JS export needed) |
| QDEC | `qdec_nrf.rs` | F | Gray-code quadrature decode, report/double-read, host-steppable (`qdec_step`), STOPPED/SAMPLE offsets fixed |
| COMP (+LPCOMP alias, shared base) | `comp_nrf.rs` | F | Thresholds, crossing events, driver input (`comp_set_input_mv`) |
| EGU0 (+SWI0 alias), EGU1–5 (+SWI1–5) | `egu_nrf.rs` | F | All six instances live (`0x40014000–0x40019000`), trigger/status; handshake test |
| MWU | `mwu_nrf.rs` | F | Region/pregion config, SUB-region masks, mem-access hook, armed-interrupt proof |
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
| ACL (`0x4001E000`, shares NVMC base) | – | – | Registers unhandled (reads 0); SPU protection out of scope |

## 2. CPU core (`src/cpu/`, ~8.4k lines)

Thumb-1/Thumb-2 baseline + DSP/SIMD (`smul/smlal/qadd/ssat`…) + VFPv4-SP
(GAS-verified probes in `docs/*.s`), IT blocks, SVC/exception entry
with priority gating + escalation, SysTick, WFI/WFE sleep + event
register, MPU-checked fetch/load/store, loud `CpuFault` on anything
unimplemented (by design — never silent). Three core bugs found and
fixed via firmware: `TST`-as-`CMP` (§1), stacked return PC leaking the
Thumb bit (§2, broke MBR→SD returns), subword peripheral reads
shifting the wrong way (§3) — see `docs/cpu_bug.md` + regression
tests (`exception_svc_stacks_even_return_pc`, `subword_reads_shift_down`).

## 3. Tests — 185 green (`cargo test`)

- 108 integration tests (`src/cpu/tests.rs`): 12 GCC-built firmware
  proofs (`blinky_nrf`, `sensors_nrf`, `extras_nrf`, `stubs_nrf`,
  `dma_nrf`, `air_nrf`, `c_irq_nrf.c`, `usbep_nrf`, `usbdev_nrf.c`,
  `i2s_nrf`, `wdt_nrf`, `nfct_nrf`, +2nd-run reset-state checks each).
- ~77 unit tests at the peripheral level (register handshake,
  SHORTS/NACK/OVERRUN/CAPTURE, FIPS-197, reboot latch, TXSTOPPED,
  COMP/QDEC/NFCT/MWU/RADIO/SAADC/CCM depth, EGU slots).
- One rare parallel flake seen once
  (`unaligned_device_faults_without_trap`, 1/10 runs, never
  reproduced in 10 follow-ups) — kept on watch, not yet root-caused.
- Thinnest: RTC/PWM/TEMP/RNG/EGU (1 handshake each).

## 4. JS API + demo (`demo/`, API frozen v1)

~75 wasm exports (adds since P17: `temp_set_celsius`, `ccm_take_job/
ccm_complete`, `comp_set_input_mv`, `qdec_step`,
`radio_set_rssi_dbm`, `radio_inject_corrupt`, USBD EPOUT); demo wires
clocks, GPIO/buttons (active-low), matrix (DIR+OUT), UARTE TX/RXDMA,
TWIM taps, SAADC/PDM/USBD/NVMC/radio pumps, I2S silence/capture,
watchdog-reset reboot, sleep-aware frame (`tick_n` + wake), Intel-HEX
loader (type-02 + UICR), and a MicroPython direct-app boot button.
Parts: matrix pins, LSM303 (WHO_AM_I `0x33`/`0x40`), SSD1306, all
green via `smoke.mjs`. `microbit-v2-emulator@0.1.0` npm package
defined (publish blocked: registry 401). Driver-API gaps (no consumer
yet): ECB/AAR take-complete, NFCT beyond proof, MWU beyond proof.

## 5. Real-firmware results (all executed, zero CPU faults except MPY §6.1)

- MicroPython v2.1.2: boots (MBR-param seeds + sleep-aware pump),
  uBit.init() completes, **banner body prints** (native and headless
  Chrome); headless-Chrome run (P20) also showed the `>>>` prompt with
  no fault over 240s+. Natively the prompt is composed in the TX ring
  but never DMA-staged (queued + `is_tx` false, no kick source found);
  input bytes land in the DMA buffer but the ring stays empty.
- MakeCode (`basic.showString`, locally built): boots to scheduler,
  TIMER4 display refresh runs (rows strobe, blank — content never
  drawn, DIR stays 0 which is correct pre-first-show).
- Espruino 2v29: boots (needed the CoreSight PID map); console is
  P0.06 bit-bang serial, nothing transmitted in early windows.
- Bootloader chain (P22–P23): SD initializes after the two core fixes
  (canary `0xCAFEBABE` written); MBR→SD→app path still open, no-BL
  MBR-param→SD→app path used instead; reset-persistence finding noted
  in plan.

## 6. LEFT — prioritized

1. **REPL exec (`print(1+2)` → `3`)**. Deterministic post-banner NULL
   fault, now narrowed hard (Sept-12 probe series, all native):
   - `bx r3` with r3=0 @`0x4F75A` (`ldr r0,[r0,#2340]; ldr r3,[r0];
     ldr r3,[r3,#40]; bx r3` — C++ virtual call, vtable slot 10).
   - **The object pointer itself is NULL** (r0=0 on entry; reads alias
     flash `0x924`/`0x28`, dies on the null slot). Caller is
     `mp_call_function`-shaped (`blx r4` @`0x4F690`); stack return
     pcs `0x4F6CB/0x4F691/0x52C4F/0x5250B/0x5197B`.
   - RX-event group unsubscribed → still fires. TX group
     unsubscribed (DMA kept alive) → still fires. ALL IRQs cut →
     no fault (thread parks in idle @`0x4D939`). TIMER1-only →
     still fires. So: **tick-scheduled MicroPython work, no
     peripheral handler delivers it, RX/TX IRQ paths excluded**.
   - Fires natively just after the banner body, before the prompt is
     staged; absent in the headless-Chrome run (timing-dependent).
   - Next steps: (a) walk the messageBus listener list at fault (field
     map in plan P21); (b) identify the NULL `this` (which
     stream/device object is expected at +2340/+2336); (c) test whether
     completing the prompt write first (browser timing) always avoids it.
   - Companion stall: prompt sits in TX ring, `is_tx` false, no kick
     source; readline never consumes a non-empty RX ring.
2. **TX byte drops** (~3% single-byte N+1 substitutions, cosmetic).
   Verdict update: **bytes are staged wrong at the source**
   (`staged==taken==completed==uartlen`, substitution not loss);
   pump read/completion timing, STOPTX arm, stack/heap collision all
   exonerated. Open: exact double-stage source.
3. **Bootloader full chain** (MBR→BL→SD→app; direct-app boot works
   around it). BL reset-loops on a validation error; next step is
   anchored disassembly from the BL vector + capturing r0 at the
   init-runner compare.
4. **MakeCode display content** (refresh runs blank) + BLE events
   (no radio attempts; needs SD event synthesis, see below).
5. **SPIM2/3 I2C-tap routing** (DMA works; tap routing open).

## 7. Deliberately out of scope

SoftDevice event synthesis (`sd_evt_get` pump — the one explicitly
deferred workstream), STM32/UNO R4/M0+/DAPLink targets (deleted, only
comment references remain), third-party-framework boot quirks (Arduino
Primo binary needs an nRF52832 bootloader), lazy FPU stacking, ACL/SPU
protection, publish to npm.

## 8. Verify

```
cargo test                       # 185 green (crate dir)
node demo/parts/smoke.mjs        # parts green
wasm-pack build nrf52833-periph-wasm --target web --out-dir ../demo/pkg
```
Firmware proofs rebuild with `docs/README.md` recipes (xpack GCC
14.2.1 via arduino packages; C recipe verified bit-identical).
MicroPython REPL state reproduces from `micropython-microbit-v2.1.2.hex`
+ plan P16 recipe; MakeCode from `makecode build` + same recipe.
