# micro:bit v2.2 (nRF52833) emulator — implementation status

Audited 2026-09-12 by cross-checking all 39 `monox/nrf52833.svd`
peripherals against `src/peripherals/`, running the suite
(**188 passed, 0 failed** — +3 since audit: SPIM RXD MISO +
GPIO CNF→DIR + UARTE TX snapshot), reading every model, and replaying the
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
| TWIM0/TWI0/SPIM0/SPIS0/TWIS0/SPI0, TWIM1 family, SPIM2, SPIM3 | `twim_nrf.rs` | F | Mode-blind shared-base; SHORTS, NACK-after-~6000-instr without slave, LASTTX/STARTRX/SUSPEND; TWIS/SPIS slave engines (`twis_master_write/read`, `spis_exchange`); SPIM tap routing incl. SPIM2/3; register-mode RXD returns MISO for SPI names, I2C queue for TWI (`rxd_polling_reads_slave_response_line`) |
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

## 3. Tests — 188 green (`cargo test`)

- 108 integration tests (`src/cpu/tests.rs`): 12 GCC-built firmware
  proofs (`blinky_nrf`, `sensors_nrf`, `extras_nrf`, `stubs_nrf`,
  `dma_nrf`, `air_nrf`, `c_irq_nrf.c`, `usbep_nrf`, `usbdev_nrf.c`,
  `i2s_nrf`, `wdt_nrf`, `nfct_nrf`, +2nd-run reset-state checks each).
- ~80 unit tests at the peripheral level (register handshake,
  SHORTS/NACK/OVERRUN/CAPTURE, FIPS-197, reboot latch, TXSTOPPED,
  SPIM RXD MISO, GPIO CNF→DIR, UARTE TX STARTTX-snapshot,
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
Preset dropdown (same `bootImage` path as dropped files, staged — Run
boots): blinky/sensors/dma/extras/stubs/air/c-irq + built-in
MicroPython hex (i2s excluded: needs patterned-RX + mailbox release
only the test driver provides); separate Load (stage, no boot) and
Run buttons; live MIPS meter (~6.0 with the 5x pump).
Sensor DRDY: the LSM303 part holds P0.25 (`SENSOR_DATA_READY`/`irq1`,
active-lo) low while polling (else the LSM303 `requestUpdate`
`awaitSample` loop spins on `getDigitalValue` forever — the MPY
post-banner stall). UARTE TX snapshots `MAXCNT` bytes synchronously
at STARTTX (thread-local RAM published by `WasmCpu::step`; no trait
or `src/cpu` change), so the deferred driver take cannot transmit
the reused N+1 byte (P49 putc-slot drops).
Parts: matrix pins, LSM303 (WHO_AM_I `0x33`/`0x40`), SSD1306, all
green via `smoke.mjs`. `microbit-v2-emulator@0.1.0` npm package
defined (publish blocked: registry 401). No open driver-API gaps: every
take/complete pair (incl. ECB/AAR/CCM, I2S, NFCT, QDEC/COMP/TEMP
driver values) is exported and documented in `demo/API.md`; MWU/NFCT
beyond proof-level driving remain future work.

## 5. Real-firmware results (all executed, zero CPU faults except MPY §6.1)

- MicroPython v2.1.2: boots (MBR-param seeds + sleep-aware pump),
  uBit.init() completes, **banner body prints** natively; the
  headless-Chrome P20 run (banner+prompt, no fault) does NOT
  reproduce in the current environment — old and fresh pkgs stall
  identically pre-banner (measured ~300K instr/s vs a ~150–260M
  threshold; 480s runs ≈144M never arrive). P20 = faster machine,
  not a different build (a fine-grained 20x5K demo pump was tried
  and reverted — it faults at app entry, see item 1). Natively the prompt is composed in the TX ring
   but never DMA-staged (queued + `is_tx` false, no kick source found);
   input bytes land in the DMA buffer but the ring stays empty.
   Post-banner pin-poll stall NAMED+F fixed (plan P52): main loops
   `NRF52Pin::getDigitalValue@0x28744` inside
   `LSM303Accelerometer/Magnetometer::requestUpdate()` (`0x266B8`/
   `0x26890`) polling `irq1`/P0.25 (`SENSOR_DATA_READY`, active-lo)
   in the `awaitSample` first-sample loop — the demo part now holds
   P0.25 low. (P41 "MP object `0x20003960`/type `0x57AC0`" was the
   CODAL LSM303 driver/vtables, not MicroPython.)
  Post-banner NULL fault narrowed (Sept-12, plan P24–P25): C++ virtual
  through NULL `this` (`bx r3 @0x4F75A`, r0=0) via mp_call_function;
  tick-scheduled (TIMER1-only suffices; all-IRQ-cut parks clean);
  RX/TX IRQ paths, TX pacing, drip, parts, UICR, image, MBR pre-roll
  all excluded. Prime suspect: audio/speaker tick path (pin-toggle
  virtuals @`0x28744`, P0.00) into an MP call with no source.
- MakeCode (`basic.showString("A")`, rebuilt 2026-09-12 with makecode
  1.3.6, recipe in plan P19; project in `mc/`, gitignored): boots to
  scheduler idle (decoded as the normal CODAL idle fiber, not a
  hang), but the display never enables — TIMER4 COUNTER stays 0 over
  620M (CC0 armed, IRQ27 enabled), matrix DIR ever 0. Stuck-or-slow
  settled as stuck: uBit.init parks in scheduler idle with main's
  print never driving refresh. NEXT: fiber walk (did main's scroll
  fiber run?) + who calls NRF52LEDMatrix::enable.
- Espruino 2v29: boots (needed the CoreSight PID map); console is
  P0.06 bit-bang serial, nothing transmitted in early windows.
- Bootloader chain (P22–P23 + Sept-12 anchor): entry decoded at BL
  VT `0x77000` (reset `0x772F9`: .data copy + BL-main call); main
  head gathers FICR DEVICEID/ER/IR/DEVICEADDR to RAM. SD initializes
  after the two core fixes (canary `0xCAFEBABE`); MBR→SD→app path
  still open, no-BL MBR-param→SD→app path used instead. NEXT: trace
  `0x77404`→IPR22 check→`0x783FE` park, capture r0 at the cmp.
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
   fault, narrowed hard (plan P24–P25; all native):
   - `bx r3` with r3=0 @`0x4F75A` (`ldr r0,[r0,#2340]; ldr r3,[r0];
     ldr r3,[r3,#40]; bx r3` — C++ virtual call, vtable slot 10).
   - **The object pointer itself is NULL** (r0=0 on entry; reads alias
     flash `0x924`/`0x28`, dies on the null slot). Caller is
     `mp_call_function`-shaped (`blx r4` @`0x4F690`); queue-drain
     (`bl 0x52C2E` @`0x51976`) → MemberFunctionCallback::fire
     (`0x52C2E`, layout-verified) → … → NULL call. Bus machinery
     healthy; NULL born downstream (audio/speaker tick path fits:
     P0.00 pin-toggle virtuals nested under the fault).
   - RX-event group unsubscribed → still fires. TX group
     unsubscribed (DMA kept alive) → still fires. ALL IRQs cut →
     no fault (thread parks in idle @`0x4D939`). TIMER1-only →
     still fires. So: **tick-scheduled MicroPython work, no
     peripheral handler delivers it, RX/TX IRQ paths excluded**.
   - Fires natively just after the banner body, before the prompt is
     staged. Prompt-first is not achievable by pump policy
     (tick-pause impossible — TX staging needs the tick; eager TX
     completion changes nothing). P20's browser prompt+no-fault is
     env-specific (see §5) and unavailable as schedule evidence.
   - Companion stall: prompt sits in TX ring, `is_tx` false, no kick
     source; readline never consumes a non-empty RX ring.
   - Demo-only divergence decoded (plan P26): the demo reaches the
     flash-op waiter via `bl 0x215A2` (r0=0, r4=`0x74000`) with
     returns `0x215A3/0x25633/0x263FD/0x23775`; the 0x502F8 subtree
     is heap free-list code inside an fds/flash-write path (no direct
     `bl`, no flash vtable — runtime-constructed pointer). SD SVC
     numbers decoded from S132 headers (SVC18=is_enabled,
     SVC40=page_erase, SVC41=write): native takes the skip branch
     10/10, zero SVC40 over full boot; the demo waits on the SD-event
     completion only sd_evt_get could deliver. WASM execution proven
     equivalent (blinky prints in-demo <10s). Pre-roll (2/8/20M),
     duty latency, drip, parts, USBD, image, UICR, params all
     excluded. Pump sensitivity is real and open: the 20x5K demo
     pump deterministically faults at app entry (`0x29C7A`,
     `op=0xDEAD`) while 1x20K spins fault-free — reverted to 1x20K;
     do not re-land without explaining the entry fault.
2. **TX byte drops** (single-byte →space substitutions, cosmetic).
    FIXED via synchronous snapshot (no trait/`src/cpu` change):
    STARTTX latches `MAXCNT` RAM bytes through a thread-local
    published by `WasmCpu::step`; `complete_txdma` emits the snapshot
    over the driver's late bytes (`tx_snapshot_freezes_starttx_bytes`
    proof). Demo pump untouched.
    Mechanism (plan P49): IRQ-mode `putc` returns right after STARTTX;
    the caller reuses the `&c` stack slot before the deferred take —
    aligned N+1 substitution (holes #19/#39/#59/#79, always `0x20`).
3. **Bootloader full chain** (MBR→BL→SD→app; direct-app boot works
   around it). Decoded (plan P25/P29): entry `0x772F9`, FICR gather,
   UICR writes + deliberate post-UICR reset (benign); 2nd reset is a
   CODED AIRCR via `0x78514` after a tbb validation dispatch (r4==0
   path) — not WDT, not IPR22-caused (no static IPR22 access; r0 stale).
   NEXT: why r4==0 + the `0x784C4`-flag compare.
    (P30: both resets proven AIRCR-coded via caller markers; seeding
    BL-programmed UICR (`0x10001200/204`=18) stalls direct-app boot at
    a register-called HALT (`0x29CD1`) — UICR-gated halt caller open.)
    CLOSED-static (plan P52): the MBR selector (`0x417`: `*(0xFF8)`/
    `*(0xFFC)` chain, UICR `0x10001014`/`0x10001018`, `0xAA` marker,
    `*(r5)==4`→boot-app) NEVER reads `0x10001200/204` (full `0x0–0xB00`
    sweep) — the markers are BL-internal DFU state, so seeding them
    cannot skip BL (native seeded run: 1 reset, parks `0x77332`).
    Blocker is BL-side `0x7B5B4` needing nonzero IPR22 (SD-set
    priorities — silicon state, out of scope); direct-app boot stays.
4. **MakeCode display content** (TIMER4 never STARTs because the
   display object is never constructed — `enable()` runs in the
   `NRF52LEDMatrix` constructor, so the stall is in an earlier member
   init; live waiter is `0x30C04` busy-`[r4+20]`)
    + BLE events (no radio attempts; needs SD event synthesis).
    NEXT: capture r4 at `0x30C18` + fiber walk.
    Strobe-OR proof (plan P52): 200-sample OR over +1M post-172M is
    all-zero — truly blank, not a multiplex alias. OUT never produces
    an on-phase; init stalls before display construction.
5. **SPIM2/3 tap routing** — done (RXD register returns the MISO
   queue for SPI names; DMA frames already routed; stale "still open"
   comment corrected).
6. **Demo wall-time**: banner needs ~150–260M at ~300K–1.5M instr/s
   in this Chromium — 600s+ per boot. Pkg profile question closed:
   dev and --release builds are byte-identical in this wasm-pack
   setup (single profile); speed is environmental. Demo pump stays
   1x20K (see item 1).

## 7. Deliberately out of scope

SoftDevice event synthesis (full BLE pump — still out of scope; but a
scoped flash-events-only `sd_evt_get` transport is drafted, unimplemented,
in `docs/sd_evt_design.md`), STM32/UNO R4/M0+/DAPLink targets (deleted, only
comment references remain), third-party-framework boot quirks (Arduino
Primo binary needs an nRF52832 bootloader), lazy FPU stacking, ACL/SPU
protection, publish to npm.

## 8. Verify

```
cargo test                       # 188 green (crate dir)
node demo/parts/smoke.mjs        # parts green
wasm-pack build nrf52833-periph-wasm --target web --out-dir ../demo/pkg
```
Firmware proofs rebuild with `docs/README.md` recipes (xpack GCC
14.2.1 via arduino packages; C recipe verified bit-identical).
MicroPython REPL state reproduces from `micropython-microbit-v2.1.2.hex`
+ plan P16 recipe; MakeCode from `makecode build` + same recipe.
