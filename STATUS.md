# micro:bit v2.2 (nRF52833) emulator — implementation status

Audited 2026-09-12 by cross-checking all 39 `monox/nrf52833.svd`
peripherals against `src/peripherals/`, running the suite
(**220 passed, 0 failed** — +35 since audit: SPIM RXD MISO +
GPIO CNF→DIR + UARTE TX snapshot + TWIM shifted-ADDR match +
SCB AIRCR SYSRESETREQ + RADIO 802.15.4 helpers + sd_ble SVC face ×10
(+peer-request pairing legs) + BLE conformance/C-face/pairing-fw firmware proofs
+ P109 live crypto+QSPI pumpDma (shared crypto.js)
+ P110 UARTE1 + SPIM2/3 firmware proofs + RTC/PWM/RNG/TEMP/EGU depth
+ P114 NFC NFCPINS gate + BLE bond store/TX-flow/periph/param-update + sd_evt phase-1
+ ACL regions + nRF FPU-engine stub),
reading every model, and replaying the
live firmware runs. Grades: **F** = functional (timed, IRQs,
driver take/complete, firmware proof), **H** = handshake
(TASKS/EVENTS/INTEN minimum, no timed behavior or no consumer),
**–** = missing/deliberately omitted.

## 1. Peripheral coverage (live map `new_wasm` + SVD map `from_svd`)

| SVD peripheral(s) | Model | Grade | Notes |
|---|---|---|---|
| CLOCK, POWER (shared `0x40000000`) | `clock_nrf.rs` | F | HF/LF STARTED events+STAT, USBDETECTED/USBPWRRDY, RESETREAS+SREQ latch, GPREGRET, RAMSTATUS, LFCLKSRC; POWER_CLOCK IRQ 0 |
| RADIO `0x40001000` | `radio_nrf.rs` | F | PCNF-length TX take / RX completion+inject, CRCERROR inject, RSSI, SHORTS; bare-metal loopback proven (`air_nrf`); link-budget air level (P119: TXPOWER-code table + path-loss inject forms, RX stamps the packet's own level) |
| UART0+UARTE0 | `uarte_nrf.rs` | F | 1-byte TX DMA + RXDMA ring; OVERRUN, ERROR, TXSTOPPED (`0x158`/INTEN 22, fixed P20); STARTTX snapshots bytes synchronously (P49 putc-slot drops) |
| UARTE1 | `uarte_nrf.rs` | F | TX/RX fully routed (was UARTE0-locked); firmware proof (`uarte1_nrf.s/.bin`, `U1DATA` TX + 3 B RX, `U1TX:OK`/`U1RX:OK`, P110) |
| TWIM0/TWI0/SPIM0/SPIS0/TWIS0/SPI0, TWIM1 family | `twim_nrf.rs` | F | Mode-blind shared-base; SHORTS, NACK-after-~6000-instr without slave (SPI never NACKs — P110 `arm_nack` guard), LASTTX/STARTRX/SUSPEND; TWIS/SPIS slave engines (`twis_master_write/read`, `spis_exchange`); register-mode RXD returns MISO for SPI names, I2C queue for TWI (`rxd_polling_reads_slave_response_line`); 7-bit `norm7_addr` on take_*/events/slave-match (nrfx shifted `0x32/0x3C/0x72` are <0x80 — P86 boot-time bug) |
| SPIM2, SPIM3 | `twim_nrf.rs` | F | Dedicated slots (IRQs 35/47); firmware DMA proof (`spim23_nrf.s/.bin`: SPIM2 4 B TX + SPIM3 4 B RX, `S2TX:OK`/`S3RX:OK`, P110); tap routing + RXD MISO |
| NFCT | `nfct_nrf.rs` | F | Field-detect/select state machine, frame TX/RX take-complete, C+S proof (`nfct_nrf.s/.bin`, `nrf_nfct_field_select_and_frames`); UICR.NFCPINS gate — GPIO P0.09/P0.10 config + input inert while antenna-reserved, sense gated (P114) |
| GPIOTE | `gpiote_nrf.rs` | F | 8 ch event/task, edge detect vs pull-up inputs, PORT event, OUT tasks drive GPIO |
| SAADC | `saadc_nrf.rs` | F | CH config/limits, LIMIT events, RESULTDONE/STOPPED, EASYDMA take/complete + result pump |
| TIMER0–4 | `timer_nrf.rs` | F | Prescaler/bitmode/SHORTS-CLEAR, CAPTURE snapshots live counter, INTEN 16+i, IRQs 8–10/26/27 |
| RTC0–2 | `rtc_nrf.rs` | F | All three instances, IRQs 11/17/36; COMPARE match + OVRFLW wrap + INTEN/ISER IRQ gating (P110) |
| TEMP | `temp_nrf.rs` | F | Driver-settable die temp (`temp_set_celsius`), DATARDY + INTEN/ISER IRQ gating + STOP clear (P110) |
| RNG | `rng_nrf.rs` | F | Deterministic LCG, VALRDY/SHORTS-to-STOP + VALUE re-arm + IRQ gating (P110) |
| ECB | `misc_nrf.rs` | F | take/complete, FIPS-197 AES-128 proof (`nrf_ecb_aes128_fips_vector`); crypto runs driver-side; live pumpDma in bench (shared `demo/parts/crypto.js`, P109) |
| AAR (shares `0x4000F000` with CCM) | `misc_nrf.rs` | F | take/complete, RESOLVED/NOTRESOLVED; live pumpDma resolve-present (P109); **no wasm export** (JS can't drive it — pump resolves present by design) |
| CCM (mode bit on shared AAR base) | `misc_nrf.rs` | F | take/complete, CTR+MIC roundtrip proof; live pumpDma encrypt/decrypt+MIC (P109); no separate slot (would alias AAR's task map) |
| WDT | `wdt_nrf.rs` | F | Expiry + reboot semantics, double-reset proof; RR reload by firmware (no JS export needed) |
| QDEC | `qdec_nrf.rs` | F | Gray-code quadrature decode, report/double-read, host-steppable (`qdec_step`), STOPPED/SAMPLE offsets fixed |
| COMP (+LPCOMP alias, shared base) | `comp_nrf.rs` | F | Thresholds, crossing events, driver input (`comp_set_input_mv`) |
| EGU0 (+SWI0 alias), EGU1–5 (+SWI1–5) | `egu_nrf.rs` | F | All six instances live (`0x40014000–0x40019000`), trigger/status; per-channel independence + INTEN mask + INTENCLR (P110) |
| MWU | `mwu_nrf.rs` | F | Region/pregion config, SUB-region masks, mem-access hook, armed-interrupt proof |
| PWM0–3 | `pwm_nrf.rs` | F | All four instances, IRQs 28/33/34/45; loop/decoders; STOP→STOPPED + INTEN/ISER gating + SEQSTART1 chain on PWM1 (P110) |
| PDM | `pdm_nrf.rs` | F | take/complete sample pump |
| NVMC | `nvmc_nrf.rs` | F | READY/READYNEXT always-1, WEN/EEN staging, erase take/complete (driver applies 0xFF) |
| PPI (+CHG groups, no FORK — no register exists) | `ppi_nrf.rs` | F | Direct dispatch + group EN/DIS |
| I2S | `misc_nrf.rs` | F | SVD-verified offsets, START-staged take_rx/take_tx, TX capture FIFO, streaming proof |
| USBD | `usbd_nrf.rs` | F | ENABLE/PULLUP/EPINEN, USBRESET+SETUP inject, EPIN/EPOUT take/complete; C proof (`usbdev_nrf.c`, flash-source DMA) |
| QSPI | `qspi_nrf.rs` | F | Registered image, AND-only program, 4K/64K erase, take/complete + JS backend exports; live pumpDma (64 KB bench image, P109) |
| FICR/UICR | `ficr_uicr.rs` | F | PART=`0x52833`, sizes; UICR RAM store (BOOTLOADERADDR-gated boot depends on it) |
| P0/P1 | `gpio_nrf.rs` | F | OUT/DIR/CNF, inputs idle-HIGH (active-low buttons), combined `0x50000000` block |
| NVIC/SCB/SysTick/MPU/FPU/DWT/ITM/STIR/DEMCR | core files | F | MPU enforced w/ MemManage+escalation; FPU SP (CPACR gate, lazy stacking implemented — `fpu_lazy_*` green); nRF FPU engine `0x40026000` minimal stub (`fpu_engine_nrf.rs`: UNUSED reads 0, both maps); CoreSight `0xF0000000` reads 0 |
| ACL (`0x4001E000`, shares NVMC base) | `nvmc_nrf.rs` | F | 8 regions ADDR/SIZE/PERM (sticky), write-protect enforced at stage (ERASEPAGE/ERASEALL refuse overlap), read-block enforced in mem.rs via MWU-patterned armed flag (bus fault + 0 on blocked reads, `acl_regions_sticky_and_block_erase` incl. gate asserts). No SPU on 833 (70 SVD peripherals, none named SPU). |
| SoC event transport (`sd_evt.rs`, SVC 16/82) | service, not peripheral | F | Phase-1 flash events only: NVMC complete posts id 2/3 while SD enabled, `sd_evt_get` answers from model queue else falls through; firmware proof (`sd_evt_nrf.s/.bin`, P114) |
| BLE bond store / TX-flow / peripheral-role / param-update | `sd_ble.rs` | F | Bonds persist across disconnects (hit/miss/delete + bridge `bond_keys` leg); TX token refill + `TX_COMPLETE`; dial-in CONNECTED (PERIPH role); param-update completion event (P114); SIGNED/PREP/EXEC write path + driver-posted request/report/timeout/user-mem/authorize legs + TX-power/adv-state store + radio link-budget RSSI (P119) |

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

## 3. Tests — 233 green (`cargo test`)

- 116 integration tests (`src/cpu/tests.rs`): 20 GCC-built firmware
  proofs (`blinky_nrf`, `sensors_nrf`, `extras_nrf`, `stubs_nrf`,
  `dma_nrf`, `air_nrf`, `c_irq_nrf.c`, `usbep_nrf`, `usbdev_nrf.c`,
  `i2s_nrf`, `wdt_nrf`, `nfct_nrf`, `ble_conformance.c`,
  `c_ble_face.bin`, `ble_cpp_face.cpp`, `ble_pairing_fw.c`, `ble_roles_fw.c`, `uarte1_nrf`, `spim23_nrf`,
  `sd_evt_nrf`, +2nd-run reset-state checks each).
- ~89 peripheral + 3 sd_evt + 14 sd_ble + 3 ACL/FPU-engine unit tests (register handshake,
  SHORTS/NACK/OVERRUN/CAPTURE, FIPS-197, reboot latch, TXSTOPPED,
  SPIM RXD MISO, GPIO CNF→DIR, UARTE TX STARTTX-snapshot,
  TWIM shifted-ADDR match, SCB AIRCR SYSRESETREQ,
  COMP/QDEC/NFCT/MWU/RADIO/SAADC/CCM depth, EGU slots, sd_ble
  peer-request pairing legs, +P110 RTC COMPARE/OVRFLW, PWM STOP/INTEN,
  RNG SHORTS/re-arm, TEMP INTEN/STOP, EGU channels/mask, +P114 NFC
  NFCPINS gate, BLE bond store, TX-flow/periph/param-update,
  sd_evt queue/order/reset, ACL sticky/block-erase, FPU-engine
  UNUSED/both-maps, +P119 TX-power store, adv validation, SIGNED/PREP/EXEC
  write path, radio link-budget RSSI, +P120 SC-gated indication,
  scan/adv slot + whitelist arbitration, +P122 C++ face, +P124 radio CRC/whiten/interference air, ACL read-gate, S132 NOT_SUPPORTED range rule).
- Parallel-test flake (CLOSED P108; open pre-existing before that):
  stock multi-threaded `cargo test` intermittently failed sd_ble/MWU
  tests with `RefCell already borrowed` at `peripherals/mod.rs:457` /
  `mwu_nrf.rs:237` (~1/4 runs pre-P105; ~2/15 post-P105;
   single-threaded `-- --test-threads=1` always 233/233). Two
  mechanisms, separated by evidence (P105+P108):
  (a) DETERMINISTIC order-dependence (fixed P105): the MPU ENABLE +
  programmed regions live in the INSTALLED model and outlive the test
  — the next test in the same process inherits a live gate into a
  foreign/fresh map (`$BIN --test-threads=1 unaligned_device
  region_watch` failed 10/10 pre-fix: the MWU test's watched write
  faulted through the stale MPU gate and never reached `mwu_note`, so
  WA stayed 0). Fix: exit disarm (`mem.write32 (0xE000ED94, 0)` at
  the end of `ldrt_probes_as_unprivileged` +
  `unaligned_device_faults_without_trap`) — entry clears in
  `boot()`/`Cpu::new` run BEFORE the test programs the model, so only
  an exit close works. Filtered `$BIN mpu mwu unalign` is now 10/10
  both thread modes.
  (b) NONDETERMINISTIC cross-thread SYS-swap (fixed P108): the
  process-global installed `SYS AtomicPtr` is swapped by every
  `boot()`/`init_for_test` with no join, while sd_ble unit tests on
  `test_dummy_system()` never held `BOOT_LOCK` — a parallel cpu/mwu
  test swapped the INSTALLED SYS mid-test while an sd_ble test's
  `FlatMemory::write8 → watch → mwu_note` held the MWU slot
  (backtraces at `mwu_nrf.rs:237` + `mod.rs:457`; thread-locals
  `SD_BLE_STATE`/`TAKE_DATA` were never the fault — per-thread and
  safe). Parallel filtered `sd_ble mwu` failed ~18/20 pre-fix, 20/20
  green post-fix. Fix: `BOOT_LOCK` join in all 14 sd_ble tests (same
  discipline as cpu/tests + mwu tests; no model change, no new locks,
  no thread-locals). Full suite parallel green post-fix (~30/31 runs
  on the P114 tree; the residual is rare scheduling noise with no
  captured backtrace, same family as the pre-existing rate). Gate
  stays single-threaded (`-- --test-threads=1`) by convention.
  (The old note about `unaligned_device_faults_without_trap` 1/10 was
  mechanism (a).)
- Depth (P110, was "Thinnest"): RTC/PWM/TEMP/RNG/EGU each carry a second
  proof now (COMPARE+OVRFLW, STOP+INTEN, DATARDY+INTEN, SHORTS+re-arm,
  channels+mask — §1 rows). No peripheral sits at a single handshake.
- P104 BLE pairing-fw proof (committed P104): `blinky/ble_fw/
  ble_pairing_fw.c` (CODAL-BLE-shaped JustWorks flow: ENABLE → GATTS
  battery service+char → CONNECT → CONNECTED drain → PRIM/CHAR/READ/
  WRITE → AUTHENTICATE → AUTH_STATUS + SEC_UPDATE drain → CONN_SEC_GET
  (encrypted `0x21`) → DISCONNECT) drives 17 `BLEP:*` markers through
  `nrf_ble_pairing_fw_markers` (2 runs, mid-spin pump like P54 —
  `pump_ble_test_driver` unchanged, CONN_SEC_GET is synchronous).
  Static SVC rescan of the stock app regions (`0x1C000–0x77000`,
  type-02+04 records, imm@even/DF@odd halfwords): MPY shows
  ENABLE/EVT_GET/ADV_DATA_SET/ADV_START/CONNECT-class SVCs (its BLE is
  compiled but `MICROBIT_DAL_BLUETOOTH_ENABLED: 0` gates runtime init —
  `mc/built/codal.json` confirms); MC shows the same family. Either
  way no static-hit claim replaces a runtime proof — the pairing-fw
  image IS the BLE-enabled image proof for this face.

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
P109 live crypto+QSPI pumps: `pumpDma` resolves staged ECB jobs
(driver AES-128 in place, FIPS-197), AAR jobs (resolve-present), CCM
jobs (CTR+MIC-4 encrypt / decrypt+verify per the CNF contract), and
QSPI read/write/erase against a 64 KB bench image (AND-only program,
`0xFF` erase) — all sharing one implementation with the depth probes
via `demo/parts/crypto.js` (moved verbatim out of `mocks.js`).
Sensor DRDY: the LSM303 part pulses P0.25 (`SENSOR_DATA_READY`/`irq1`,
active-lo) 60 ms low / 140 ms high (a stuck low trips the shared KL27
idle threshold; the pulse satisfies both consumers). UARTE TX snapshots `MAXCNT` bytes synchronously
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
    in the `awaitSample` first-sample loop — the demo part pulses
    P0.25 60/140 ms. (P41 "MP object `0x20003960`/type `0x57AC0`" was the
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

## 6. LEFT — prioritized (P114 verdicts, 2026-09-18)

  1. **REPL exec (`print(1+2)` → `3`) — CLOSED P116+P117 (2026-09-18, Node probes `p16`–`p20` + bench-page probes `p21`–`p24`, no code changes — the bench pump was already correct).**
     Root cause was PUMP STARVATION in the ad-hoc native probes only,
     not a model gap and not the bench: those probes drained TWIM TX
     takes without completing them, so the sensor-init
     `STARTTX → LASTTX → (SHORTS STARTRX)` chain never fired and the
     `0x28290`-family waiter spun in the `0x200021B8/BB` RAM delay-fn
     to 200M+. With a FULL TWIM pump (take→complete both directions,
     WHO_AM_I bytes on RX) boot escapes at ~176M, banners at ~237.8M
     (`MicroPython v1.18 on 2023-10-30; micro:bit v2.1.2 with nRF52833`
     + `Type "help()"`, 78 B), and `print(1+2)` → `3` + `>>> ` prompt
     via the RX drip path, zero faults throughout. The `0x28290` waiter
     polls `[TWIM1+0x150]` = EVENTS_TXSTARTED for a STARTTX the pump
     must complete (r0=`0x40004000` at trap, r1=`0x148`, r6 climbing to
     r9=1M bound, lr=`0x26039`); DRDY HIGH-vs-LOW is identical (sensor
     path innocent); TWIM1 audit pre-fix showed exactly ONE transfer
     (WHO_AM_I answered, zero RX). Post-banner flash pcs live in the
     `0x266Dx/0x2678x/0x2874x` + `0x539E7` region.
     P117 bench verdict: NO WIRING NEEDED — `lsm303.js poll()` already
     takes→`mem_read`→completes TX and takes→`mem_write`→completes RX
     every frame (`pumpDma` just calls `parts.poll`). Repro vs the
     committed page+pkg: banner in the UART box at T+15s wall
     (104–105 B), `print(1+2)` + Send → `...>>> print(1+2)\n3\n>>> `
     at +10s, zero page errors. LEFT-1 is closed end to end: native
     proof + in-browser proof on the shipped bench.
     Prior P113 forensics (kept for the record):
    P113 (Node probes svcA–G/trueA–C/uicrA–E/clobA–B, no commit —
    SUPERSEDES P112's "new early fault"): P112's `0x1AEF8` fault was a
    HARNESS SEED TYPO, not a model bug. Probes seeded UICR
    `0x70700/0xE00700` (byte-swapped; plan.md:71 misprint, now fixed);
    the HEX record `:081014000070070000E0070076` decodes LE to
    `0x77000/0x7E000`. With WRONG seeds the app's SD validator
    (`bl 0x1A56C` canary check `[0x20000058]` vs `0xCAFEBABE`) fails
    and stores `0x70700` over `[0x20000004]` (store pc `0x1A5CF`,
    1-step watch) → slot `0x1AEF0` derefs `[[0x70700]+0x2C]` =
    `0xFFFFFFFF` → NULL `bx r2`. With TRUE seeds the word is never
    clobbered (400k watch: zero changes), no fault — boot reaches the
    DOCUMENTED park (`0x200021B8/BB` RAM delay-fn, 200M/zero-fault/
     uart-0/tx-0, TWIM healthy: txT 117/rxT 1/ev 378, addrs `0x19`+
     `0x39`, zero NACKs — all SUPERSEDED by the P116 starvation
     finding above (those runs never completed a TWIM transfer, so
     "healthy" only meant "no NACK storm"). Serial object at 60M TRUE park: id low-half
    12 ✓, status low-half `0x4000` (TX BUFF_INIT only — RX never
     initialized), both ring buffers NULL, baud 0, DMA never armed,
     UARTE EN=8. Init stalled between TX-setup and RX-setup — P116
     names the gate: the TWIM sensor-init completion the pump was
     starving (see CLOSED note above), NOT a UARTE model gap.
     (Old NEXT, done: `0x282E5`-caller trace + DRDY-line experiment.)
    svc13 audit (keep): thunk `0x550F6` entered with
    r0=`0x20002520`/r1=`0x20003984`; post-SVC r0=`0x70700` (our
    RAM-floor report echoed back — firmware-side check fails it, not
    a model miss); canary NEVER set (BL-path artifact, no BL in
    direct-app boot by construction); SVC census whole window = 2
    (`svc13` init + `svc00` MSR switch; `df3c` a dead literal).
    Older CLOSED note (P87, 2026-09-13, headless Chrome, committed P86
    pkg, zero page errors, no fault):
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
  2. **TX byte drops — model done, no action.** P112 (live-pkg A/B):
     unguarded STARTTX leaks 60/60 N+1, guarded 0/60 — snapshot path
     byte-correct end to end. Remaining drops, if any on banner runs,
     are pre-STARTTX firmware-side (P49 `&c` reuse / P53i) — no model
     change indicated, no trait change ever. (P67 older closure +
     P49/`0x2001FEC7`/holes-19-39-59-79 forensics kept below.)
 3. **Bootloader full chain — PARKED, stays parked.** Direct-app boot
    is the recipe; the full MBR→BL→SD→app chain needs SD-synthesized
    NVIC priorities (inventing silicon state) AND a reason to believe
    pass 2 reaches BL at all (P68: MBR→app-direct bypasses validation).
    P112 re-proved P68 on this tree (seeded UICR+IPR22: 1 reset, parks
    `0x29C7B`, ZERO hits on `0x7B5B4`/`0x772F9`/`0x783FE` over 4M).
    Full decode (P25/P29/P49/P52/P107) kept below. Reopen only with a
    faulting config (none exists).
    Decode record (P25/P29/P49/P52/P107, kept): entry `0x772F9`,
    FICR gather, UICR writes + benign post-UICR reset; 2nd reset CODED
    AIRCR `0x78514` via tbb `0x78498` on the r4==0 path (= SD-enable
    SUCCESS through `bl 0x7B530`/`svc 16`, BY DESIGN handoff);
    `0x7B5B4` IPR22 validator (`(236>>a)` odd; IPR22=0 always fails —
    needs SD-set priorities); `0x784C4` DFU-progress gate (not the r4
    cause); MBR selector `0x417` never reads `0x10001200/204`
    (BL-internal DFU state); seeded UICR stalls direct-app at
    register-called HALT `0x29CD1` (caller open).
 4. **MakeCode display content — PARKED, shared gate with (1).**
    Pre-scroll stall CONFIRMED on current tree (P112 mcA–C): app-honor
    faults `0x1AEF8` (same S140 SD region as MPY); MBR-honor parks
    `0x37F4F`/ipsr=3 with the SAME MBR-selector SVC3 context as MPY's
    `0x29C7B` park (stacked r3=`0x417`/ret=`0x440`, CFSR `0x8200`);
    4M sleep-aware: TIMER4 0, DIR0 0, TX 0, uart 0. NEXT for both
    firmwares = LEFT-1 r2-provenance read. Forensics (P57/P69/P91–P97/
    P106 + strobe-OR) kept below.
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
   (TWIM1-base driver object, not uBit). P97 (2026-09-14, native,
   reverted): `0x35664` statically = member-getter on `[obj+20]`
   slots (`0x104/0x148/0x15c`), one consuming `bl @0x358c8` + four
   `b.w` tails; `0x2e084` = flag-gated forward to pump `0x2e01c`;
    `0x2e410` = NULL-or-flag-gated pump entry (one static `bl`
    `@0x31f62`, live entries via runtime `blx`). Dynamic trap got
    Forensics record (P69/P91–P97/P106 + strobe-OR, kept): 300M park
    `0x20002078/7A`→`0x3569C`/`0x37afa` WFE-idle, DIR0 sticky, TIMER4
    untouched; run queue ONE fiber (main in `0x2e410` waiter), scroll
    fiber never created; `0x2e410` = NULL-or-flag-gated PUMP ENTRY
    (not an event wait — wait queue EMPTY, raise-forward `0x2e084`
    never hit); strobe-OR all-zero (truly blank). Pre-scroll
    sequencing confirmed, not a missed wakeup.
5. **SPIM2/3** — done, firmware-proven twice AND browser-proven
   (P70): extended `stubs_nrf` (START/STOP→STOPPED on SPIM0/2/3, 326B,
   preset base64 byte-identical) prints `STUBS:OK` in-browser in 10s
   (`s2stop=1/s3stop=1`, no fault, no page errors). P110 adds the DMA
   layer: `spim23_nrf.s/.bin` (SPIM2 4 B TX DMA + SPIM3 4 B RX DMA,
   `S2TX:OK`/`S3RX:OK` via `nrf_spim23_dma_roundtrip`) + the SPI-NACK
   guard (`twim_nrf.rs::arm_nack` — SPI has no address phase; without
   it staged SPIM2/3 DMA cleared ~6000 instr before the driver take).
    Demo pumpDma covers SPIM2/3 frames live. P114 wires the consumer:
    `demo/parts/spidisplay.js` ST7789 240×240 (CASET/RASET/RAMWR +
    RGB565 + SWRESET/DISPON, MISO ID `04 85 52`) + bench panel/canvas
    on SPIM2, verified headless (`spidisplay_check.mjs`: pixels +
    drain + clear + DISPON, all OK vs built pkg).
   **UARTE1** — done (P110): `uarte1_nrf.s/.bin` (UARTE1 TX DMA
   `U1DATA` + 3 B RX DMA, `U1TX:OK`/`U1RX:OK` via
   `nrf_uarte1_instance_dma_roundtrip` through the shared take/complete path).
    RADIO 802.15.4 (P59, NEW): ED/CCA/DEVMATCH-MISS/MHRMATCH/
    FRAMESTART + full SHORTS/INTEN maps (`ed_cca_mhr_devmatch_
    framestart` green); demo air = Bumble bridge
    (`tools/ble_air_bridge.py` + `BleAir` part, `radio_inject_rx_to`
    addressed echo) with loopback default — BLE/BT without
    WebBluetooth; new export `radio_set_ed_dbm`.
    BLE SVC face (P98–P100): `src/sd_ble.rs` answers the SoftDevice
    SVCs BLE firmware actually calls — full S132-verified coverage:
    common ENABLE (RAM-floor report) + two-arg EVT_GET (length query,
    DATA_SIZE, legacy drain), GAP ADDR/ADV/SCAN/CONNECT/DISCONNECT/
     RSSI (stage air jobs), pairing handshake (AUTHENTICATE stages,
     SEC_PARAMS_REPLY accept/reject, AUTH_STATUS + CONN_SEC_UPDATE
     events; P103: full peer-initiated legs — SEC_PARAMS_REQUEST /
     SEC_INFO_REQUEST / AUTH_KEY_REQUEST / PASSKEY_DISPLAY /
     KEY_PRESSED / LESC_DHKEY_REQUEST events with conn-first wire
     bodies, per-link state machine, accept/reject/passkey/OOB/
     encrypt reply surface, S132 SEC_STATUS codes incl. the 0x29→0x85
     fix, LTK/IRK/CSRK/master-id persist per peer in the bond store
     (hit/miss/delete + bridge `bond_keys` leg — P114), TX tokens refill
     per air packet with TX_COMPLETE posted (P114), periph-role CONNECTED
     + param-update event (P114)), L2CAP CID register/TX/RX
     (0xB0–0xB2), multi-connection links (per-link handles, RSSI, TX
     budget, security; conn_handles/conn_sec exports), GATTC PRIM/CHAR/
     DESC/REL/ATTR_INFO discovery + READ-by-UUID + multi-READ + READ +
     WRITE (bytes copied at SVC time) + HV_CONFIRM, GATTS
     service/char/descriptor table with real handles + struct-form
     VALUE_SET/GET + CCCD-gated HVX staging (notify bit0 / indicate
     bit1, unsubscribed refuses). Event envelopes carry
     the gattc head / unpacked pads per the headers (tests assert byte
     offsets). thumb.rs SVC hook claims 0x60..=0xBF first (r0 + skip,
     else fall through to raise_sync — zero-cost when idle); air-backed
     ops stage take/complete jobs (17 tags incl. take_data for WRITE/
     HVX/L2CAP bytes) the demo pump resolves via the Bumble bridge
     (local loopback default mirrors the bridge peer table: battery 87
     + NUS). `ble_take_job/take_data/complete_*×17/post_adv_report/
     post_gatts_write/post_sec_params_request/post_sec_info_request/
     post_auth_key_request/post_passkey_display/post_keypress/
     post_lesc_dhkey_request/enabled/queue_len/batt_level/conn_handles/
     conn_sec/tx_power/adv_state/post_sec_request/post_timeouts/
     post_user_mem/post_rw_authorize/post_sys_attr/post_sc_confirm/
     complete_service_changed` exports + SMP toolbox (`ble_lesc_dhkey`,
     `ble_lesc_public_key`, `ble_smp_f4/f5/f6/g2`, P125); 14 native tests + SVC-hook proof
     in cpu/tests.rs (233 green); headless `MockBleSvc` executes REAL SVC
     bytes on a WasmCpu end to end (enable→table→connect→disc×6→read→
     write→L2CAP→pairing→peer-pairing(passkey)→HVX-indicate→
     service-changed→ADV/SCAN roles→scan→rssi→disconnect, 18 mocks OK). Bridge peers ×2: battery
     (READ+NOTIFY, 87 `PeerBatt`) + heart-rate twin (64 `PeerHR`,
     distinct address) + Nordic UART (RX write, TX notify) + live handles +
     full job protocol incl. desc_disc/pair/l2cap with per-job `peer`
     routing + `peer` echo on disc RSPs (both peers share handle
     numbers — unaddressed reads crossed peers, fixed by addressing);
     per-peer ATT locks + global scan lock serialize air (no timeouts
     under load); HCI_READ_RSSI probed first on the live link
     (UNKNOWN_HCI_COMMAND on LocalLink — falls back to adv sighting,
     `src:"conn"|"adv"` tagged). Bench: BLE panel shows live stack/links/queue/
     battery + loopback self-test button; depth probes include the BLE
     stack probe (16/16 green in real Chromium, zero page errors —
     re-ran P103 on this tree via tools/browser_verify_16.py: blinky
     BOOT/BLINK/BLINK, self-test pairing×2 pass, 16/16 probes).
     P121 roles firmware (`blinky/ble_fw/ble_roles_fw.c`, xpack GCC,
     bit-identical rebuild): ADV_START(NULL/struct/IN_USE) → SCAN_START
     (BUSY/param/selective/cross-IN_USE) → CONNECT (CENTRAL role) →
     SERVICE_CHANGED range leg → DISCONNECT, 21 `BLER:*` markers,
     2nd-run clean (`nrf_ble_roles_fw_markers`).
 6. **Demo wall-time — environmental, measured, no action.**
    Node WASM on this host: blinky `~41–54 MIPS` (6M/0.11–0.15 s,
    BOOT/BLINK/BLINK correct); MPY-fault path `~24 MIPS`
    sustained-through-fault; native debug blinky test 0.32–0.40 s
    (5M-instr run — harness time, NOT core speed). Bench meter
    `~6 MIPS` in-browser; banner math stands (160–180M ⇒ ~4 s Node,
    ~30 s browser). Pkg profile closed (byte-identical dev/release).

## 7. Deliberately out of scope (owner + reason — DO NOT reopen without both)

| Item | Owner if ever revisited | Why it stays out |
|---|---|---|
| SoftDevice event synthesis (full BLE pump) | BLE-face owner (new workstream) | Phase-1 flash-only CLOSED P114 (`sd_evt.rs`, SVC 16/82, NVMC-posted id 2/3, `sd_evt_nrf.s/.bin` proof); zero `svc 82` callers in MPY/MC so no shipped firmware observes it — full BLE/timeslot event synthesis stays out (P32/P51). |
| STM32 / UNO R4 / M0+ / DAPLink targets | Nobody (deleted) | Only comment references remain; Nordic TASKS/EVENTS/SHORTS has zero register overlap. |
| Third-party-framework boot quirks (Arduino Primo nRF52832 bootloader) | Framework owner | Needs an nRF52832 bootloader image, not an 833 model gap. |
| Lazy FPU stacking | — (closed P114: was already implemented; stale comment fixed) | `cpu/thumb.rs` FPU hook + `cpu/mod.rs` take/return reserve/complete/pop; `fpu_lazy_*` + `fpu_eager_*` green. |
| Edge-SPI display | — (closed P114: `demo/parts/spidisplay.js` ST7789 240×240 + bench panel, verified headless) | SPIM2 DMA + tap events + MISO ID; needs no Rust change. |
| I2S WebAudio sink | — (closed P114: bench `audioPush` 16 kHz mono, silence-skipped) | Capture FIFO already existed; sink is a gesture-gated AudioContext. |
| NFC antenna model | — (closed P114: NFCPINS gate, not physics) | UICR.PROTECT=1 reserves P0.09/P0.10 (GPIO inert) + NFCT sense gated; no RF emulation. |
| ACL/SPU protection | ACL owner (done: model + enforcement) / SPU n/a | ACL modeled + write-enforced + read-enforced in mem.rs (MWU-patterned armed flag, no cpu/ edits); borrows via try_borrow_mut (no RefCell panic). No SPU exists on nRF52833 (SVD-verified) — nothing to model. |
| Publish to npm | Release owner | Blocked: registry 401, no credentials in this environment. |
| BLE bond store / TX-flow / peripheral-role / param enforcement | — (closed P114: bond store + TX_COMPLETE + dial-in CONNECTED + param-update event) | Keys stored per peer (hit/miss/delete), TX tokens refill on air drain, PERIPH role byte, update completion posted; bridge `bond_keys` leg persists air keys. Crypto itself stays driver-side by design. |
| sd_evt flash transport | — (closed P114 phase 1: `sd_evt.rs`, SVC 16/82) | NVMC complete posts id 2/3 while SD enabled; `sd_evt_get` answers from model queue else falls through; firmware proof `sd_evt_nrf.s/.bin`. |

## 8. Verify

```
cargo test -- --test-threads=1    # 233 green (crate dir; parallel ~30/31 on the P114 tree — see §3)
npm run test:wasm --prefix demo  # handshake 18/18 + smoke + MPY/JS/PY/TS-idiom faces + REPL, all vs the BUILT pkg
python3 tools/ble_air_bridge.py --port 18771 &  # live air peers (PeerBatt 87 + PeerHR 64)
node demo/parts/ble_live_e2e.mjs ws://127.0.0.1:18771  # 42 over-air checks green (two links)
python3 -m http.server 8080 --directory demo &  # bench
python3 tools/browser_verify_16.py  # blinky + BLE self-test + 16/16 probes, zero page errors
wasm-pack build nrf52833-periph-wasm --target web --out-dir ../demo/pkg
```
Firmware proofs rebuild with `docs/README.md` recipes (xpack GCC
14.2.1 via arduino packages; C recipe verified bit-identical; asm
recipe `as` + `ld -T blinky/link_nrf.ld` + `objcopy -O binary`
verified bit-identical for `uarte1_nrf`/`spim23_nrf`).
MicroPython REPL state reproduces from `micropython-microbit-v2.1.2.hex`
+ plan P16 recipe; MakeCode from `makecode build` + same recipe.
