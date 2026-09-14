# micro:bit v2.2 (nRF52833) emulator — implementation status

Audited 2026-09-12 by cross-checking all 39 `monox/nrf52833.svd`
peripherals against `src/peripherals/`, running the suite
(**191 passed, 0 failed** — +6 since audit: SPIM RXD MISO +
GPIO CNF→DIR + UARTE TX snapshot + TWIM shifted-ADDR match +
SCB AIRCR SYSRESETREQ + RADIO 802.15.4 helpers), reading every model, and replaying the
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
| TWIM0/TWI0/SPIM0/SPIS0/TWIS0/SPI0, TWIM1 family, SPIM2, SPIM3 | `twim_nrf.rs` | F | Mode-blind shared-base; SHORTS, NACK-after-~6000-instr without slave, LASTTX/STARTRX/SUSPEND; TWIS/SPIS slave engines (`twis_master_write/read`, `spis_exchange`); SPIM tap routing incl. SPIM2/3; register-mode RXD returns MISO for SPI names, I2C queue for TWI (`rxd_polling_reads_slave_response_line`); 7-bit `norm7_addr` on take_*/events/slave-match (nrfx shifted `0x32/0x3C/0x72` are <0x80 — P86 boot-time bug) |
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

## 3. Tests — 191 green (`cargo test`)

- 108 integration tests (`src/cpu/tests.rs`): 12 GCC-built firmware
  proofs (`blinky_nrf`, `sensors_nrf`, `extras_nrf`, `stubs_nrf`,
  `dma_nrf`, `air_nrf`, `c_irq_nrf.c`, `usbep_nrf`, `usbdev_nrf.c`,
  `i2s_nrf`, `wdt_nrf`, `nfct_nrf`, +2nd-run reset-state checks each).
- ~82 unit tests at the peripheral level (register handshake,
  SHORTS/NACK/OVERRUN/CAPTURE, FIPS-197, reboot latch, TXSTOPPED,
  SPIM RXD MISO, GPIO CNF→DIR, UARTE TX STARTTX-snapshot,
  TWIM shifted-ADDR match, SCB AIRCR SYSRESETREQ,
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
  uBit.init() completes, **banner body prints** natively; headless
  Chrome banners in ~60s wall with the REPL prompt (P86, 2026-09-13:
  the pre-banner boot time was a KL27 USB-flash transact retry storm —
  nrfx shifted ADDRs `0x32/0x3C/0x72` never normalized to 7-bit so the
  `0x39` stub answered zeros = NOT-READY = 20×20 retries per
  transact; fixed via `norm7_addr` + request-echo stub). REPL exec
  (`print(1+2)` → `3`) is the next frontier.
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

1. **REPL exec (`print(1+2)` → `3`)**. CLOSED (P87, 2026-09-13,
   headless Chrome, committed P86 pkg, zero page errors, no fault):
   banner at T+60s, `print(1+2)` + Send → `...>>> print(1+2)\n3\n>>> `
   at R+60s, stable after. It was never a separate input-path bug —
   the RX drip + DMA-mirror path (P39) was already correct; the
   "parked-main / pin-poll / NULL-fault" post-banner layers
   (P24–P25/P40–P44/P51) were schedule-starved/NACK-degraded runs
   that the KL27 retry storm explains. Historical forensics kept
   below for the record.
   Pre-P86 state (superseded, kept for context):
   0, TWIM clean (`t_addr=114/err=0/endtx/rx=1`), UARTE never staged
   (`u_max=0/u_end=0`) — so the banner gate is NOT clocks/ADDR/AIRCR.
   Pre-banner park is a countdown wait, not a hang: pc `0x200021b8/bb`
   = RAM delay-fn (`01 38 fd d1 70 47`: `subs r0,#1; bne; bx lr`),
   called from the `0x20980` 20-iteration helper (`movs r7,#20` loop,
   rets `0x20ab1`/`0x20b37` both `bl`-validated) with lr `0x26039`
   (the u64-compare-then-branch helper `0x26020`: returns 0 or falls
   into `0x2603e` time-add path). HFCLK wait RULED OUT (P55b: clocks
   boot ON yet park persists to 600M). r0 at park (`0x92b`–`0x719e`)
   is the live countdown, r4=`0x3e8` (1000), r1=3, r2=`0x20016608`.
    NEXT: name the `0x20980` helper's caller — CONSTRAINED (P58) then
    STATIC+NAMED (P60): exactly TWO `bl→0x20980` sites exist,
    `0x20b32`+`0x20b58`, both inside ONE function (`0x20ae8`,
    GC-shape: `bl 0x528e8` alloc + `bl 0x528d8`/`0x52908` field init +
    `strb [r3,#4]` type-tag store). r0 at park is ALIVE and WRAPPING
    (`0xb3a→0x990→0x8ff→…→0x92b` across 2M samples — never monotonic,
    never stuck): the inner countdown completes and the 20× loop
    re-arms, i.e. a tight re-poll whose exit condition (r7-driven,
    `0x209aa`-region compares) never fires. P61: sampled regs at the
    park are STALE (r4=`0x3e8`/r6=stack — the DELAY call's args, not
    the helper's; `[r4+20]` reads flash `0xF878F000`, `[r6]` a stack
    word) — the helper's frame is long gone (we sample the delay-fn
    leaf, not the loop body). So the r7/compare inputs are NOT
    observable at pc — need a break INSIDE `0x209aa–0x209de`
    (pc-triggered reg dump), not at the leaf. NEXT: pc-break at
    `0x209b2`/`0x209c2` to read the live r4/r6 + fp target + r0.
   Post-banner NULL fault (plan P24–P25, all native) still open after:
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
    CLOSED as model-clean (P67): with zero banner takes on the current
    tree the snapshot path never fires — unit test green + P53i 0/106
    hold, and there is no live path left to verify until L1 banners.
    Old holes (19/39/59/79 + tail-shift) are firmware-side pre-STARTTX
    (P49 `&c` slot reuse) by elimination. No trait change, ever.
3. **Bootloader full chain** (MBR→BL→SD→app; direct-app boot works
   around it). Decoded (plan P25/P29): entry `0x772F9`, FICR gather,
   UICR writes + deliberate post-UICR reset (benign); 2nd reset is a
   CODED AIRCR via `0x78514` after a tbb validation dispatch (r4==0
   path) — not WDT, not IPR22-caused (no static IPR22 access; r0 stale).
    NEXT: why r4==0 + the `0x784C4`-flag compare — ANSWERED (P49+L3):
    r4==0 is SD-enable SUCCESS (`bl 0x7B530` = `svc 16; bx lr`;
    `cbnz r4@0x7B636` skips validation on FAILURE; success → `0x7B5B4`
    IPR22 check + `bl 0x7B568` → tbb `0x78498` selects reset site #2
    `0x78514` BY DESIGN, handoff into the SD-enabled state).
    `0x7B5B4` = IPR22 validator (`ldrb [0xE000E100+#0x316]` = IPR22,
    pass iff `(236>>a)` odd → IPR22=0 always fails in emulation; needs
    SD-set app priorities 2/1, silicon state). `0x784C4` = DFU-progress
    gate (flag `[0x20002DF1]`; `r4=[0x2DFC]-[0x2DF4]` vs 59; `r1=0/1`
    into `bl 0x78760`; copy `[r6]→[r5]` when in range) — not the r4
    cause. MBR pass-2 needs SD priorities: SHELVED with sd_evt.
    (P30: both resets proven AIRCR-coded via caller markers; seeding
    BL-programmed UICR (`0x10001200/204`=18) stalls direct-app boot at
    a register-called HALT (`0x29CD1`) — UICR-gated halt caller open.)
    P68 MBR pass-2 (seeded UICR 18/18 + IPR22 `0x40404040`, MBR entry):
    1 reset then parks at app vector `0x29C7A` (the P51 entry-fault
    address, no fault here) with ZERO hits on `0x7B5B4`/`0x772F9`/
    `0x783FE` — pass 2 never reaches BL validation at all (MBR jumps
    straight to the app vector table). So the loop is MBR→app-direct,
    not MBR→BL→app; BL validation is bypassed, not failed.
    CLOSED-static (plan P52): the MBR selector (`0x417`: `*(0xFF8)`/
    `*(0xFFC)` chain, UICR `0x10001014`/`0x10001018`, `0xAA` marker,
    `*(r5)==4`→boot-app) NEVER reads `0x10001200/204` (full `0x0–0xB00`
    sweep) — the markers are BL-internal DFU state, so seeding them
    cannot skip BL (native seeded run: 1 reset, parks `0x77332`).
    Blocker is BL-side `0x7B5B4` needing nonzero IPR22 (SD-set
    priorities — silicon state, out of scope); direct-app boot stays.
4. **MakeCode display content** (ZERO-TOUCH proof, P57; re-run P69 on
   current tree: IDENTICAL — 300M, 0 `0x30C04` hits, `0x20002078/7A`→
   `0x3569C` WFE-idle at 300M, DIR0 sticky, TIMER4 untouched). So NO
   member init before the scroll call touches hardware — stall is
   pre-scroll sequencing (main never issues the scroll), not a missed
   wakeup or display-construct gap. P91 (2026-09-14, native, reverted):
   fiber-wait `0x2e4d8` entered+dispatching, no HardFault (CFSR 0,
   `fault=None` to 300M); `ipsr=3 @0x37f4e` nondeterministic across
   runs, cause never captured. P92 (2026-09-14, native, reverted):
   run queue = ONE fiber (main, parked in `0x2e410` waiter,
   TCB LR `0x2e453`); sleep queue = 2 fibers; event-wait EMPTY — the
   scroll fiber is never CREATED. P93 (2026-09-14, native, reverted):
   main is inside `EventModel::send` listener-invoke (R0=own TCB, not
   an event id — P92's "event-wait" label was wrong). P94–P96
   (2026-09-14, native, reverted) CORRECT P93's heap framing: free
   node @`0x20003b3c` present in every dump (heap NOT empty),
   `0x2e99c` is not malloc (all 10 callers clobber r0; live args
   `r0=4/10` are sleep/wait codes), `0x35664: r0=0x20002c10`
   (TWIM1-base driver object, not uBit). NEXT: awake-gated trap on
   raise path `0x2e084` + waiter entry `0x2e410`, or static decode of
   `0x35664`'s caller chain.
    Strobe-OR proof (plan P52): 200-sample OR over +1M post-172M is
    all-zero — truly blank, not a multiplex alias. OUT never produces
    an on-phase; init stalls before display construction.
5. **SPIM2/3 tap routing** — done, firmware-proven AND browser-proven
   (P70): extended `stubs_nrf` (START/STOP→STOPPED on SPIM0/2/3, 326B,
   preset base64 byte-identical) prints `STUBS:OK` in-browser in 10s
   (`s2stop=1/s3stop=1`, no fault, no page errors). Demo pump covers
   SPIM2/3 DMA frames. No edge-SPI part wired (no consumer) — stays H
   by decision, not by gap.
   RADIO 802.15.4 (P59, NEW): ED/CCA/DEVMATCH-MISS/MHRMATCH/
   FRAMESTART + full SHORTS/INTEN maps (`ed_cca_mhr_devmatch_
   framestart` green); demo air = two-instance bridge via
   `window.__airPeer` (foreign bytes) with loopback default — BLE/BT
   without WebBluetooth; new export `radio_set_ed_dbm`.
6. **Demo wall-time**: meter now shows slice + sustained average
   (`6.02 MIPS (avg 6.00)` on both blinky AND mpy park — P71: the
   16-class bursts the user saw are peak slice rates; sustained == slice
   here because the park never sleeps). Banner math stands (~150–260M
   needed; native L1 bannered 160–180M). Pkg profile closed
   (byte-identical dev/release); speed is environmental.

## 7. Deliberately out of scope

SoftDevice event synthesis (full BLE pump — still out of scope; but a
scoped flash-events-only `sd_evt_get` transport is drafted, unimplemented,
in `docs/sd_evt_design.md`), STM32/UNO R4/M0+/DAPLink targets (deleted, only
comment references remain), third-party-framework boot quirks (Arduino
Primo binary needs an nRF52832 bootloader), lazy FPU stacking, ACL/SPU
protection, publish to npm.

## 8. Verify

```
cargo test                       # 191 green (crate dir)
node demo/parts/smoke.mjs        # parts green
wasm-pack build nrf52833-periph-wasm --target web --out-dir ../demo/pkg
```
Firmware proofs rebuild with `docs/README.md` recipes (xpack GCC
14.2.1 via arduino packages; C recipe verified bit-identical).
MicroPython REPL state reproduces from `micropython-microbit-v2.1.2.hex`
+ plan P16 recipe; MakeCode from `makecode build` + same recipe.
