# agent.md — live handover (micro:bit v2.2 nRF52833 emulator)

> Living file. Update every working turn: HEAD, `git status -sb`, test count,
> todo states. Trust this over memory. Details in `STATUS.md` / `plan.md` /
> `HANDOVER.md` (HANDOVER stale at 535cfcb/203 — this file supersedes for state).

## 0. Snapshot (2026-10-04, P160 Playwright browser proof + TS/JS demos — gates green)

- HEAD: `592aa0a` "P158 predicated-T1 IT-flags CPU fix…".
- Branch: `master`, in sync with `origin/master` (P159 staged-uncommitted per user call; P160 uncommitted until user approval).
- Suite: **273 single green** + test:wasm ok incl. new test:arrow-js/ts + test:mc-matrix (9 presets) + `run_mpy_full.mjs` full-face green.
- P160 (this turn): served `demo/` over HTTP + Playwright headless proofs, zero page errors — mc_arrow/mc_east 9/9 in 3.1s, rotation N→E→W in sequence (unwind fault is phase-dependent), scroll H→North, smiley stable 9/9, mpy banner 5.1s + `print(1+2)`→`3` + HAPPY screenshot. Bench fixes: matrix persistence 3→5 bits, per-preset accurate status lines (old blanket "never runs" withdrawn for arrow/blink/scroll/rotation). New TS demos (mc_smiley/heart/plot `pxt build`, vendored + presets) + new JS demos (arrow_js_example.mjs, arrow_ts_example.mts, run_mc_matrix.mjs — all wired into `test:wasm`).
- Working tree: P160 files (demo/index.html persistence+status+3 presets, demo/package.json chain, demo/firmware 3 new hexes, 3 new ble_lang demos, STATUS.md, agent.md, plan.md) + P159 staged set + gitignored `.probe-tmp/` (never commit).
- CORRECTION to the P145 "Big news" below: the Sept-29 vendored hex's user
  program is NOT the `showLeds` smiley — the compiler's own
  `mc/built/mbcodal-binary.asm` (`_main___P3096`: `movs r0,#100; movs r1,#1;
  bl pins::digitalWritePin`, bytes match at `0x47052`) drives P0
  (ID_PIN_P0=100); `0x4788e` holds "my-project", no smiley literal exists;
  `mc/main.ts` does not match the built hex (stale build — needs `pxt build`
  rerun). Correct user-run observable is P0.02 output HIGH (currently
  input/low); matrix-dark is EXPECTED for this build. Wedge re-localized on
  current tree, scheduler core verified healthy — see P152 entry.

## 1. What we did so far (this recovery session)

1. **Audited gaps**: thinnest models RTC/PWM/TEMP/RNG/EGU (1 handshake each); UARTE1 + SPIM2/3 had no firmware DMA proof; SPI instances wrongly armed I2C NACK timeout.
2. **Batch 1 (committed P109)**: ECB/AAR/CCM + QSPI live pumps in `pumpDma`, shared `demo/parts/crypto.js`. At HEAD.
3. **Batch 2 (in working tree, NOT committed)**:
   - `blinky/uarte1_nrf.s/.bin`: UARTE1 (0x40028000, IRQ 40) TX DMA (`U1DATA\n`, PTR/MAXCNT/STARTTX → poll ENDTX) + RX DMA (3 B, STARTRX → poll ENDRX) → prints `U1TX:OK` / `U1RX:OK` via UARTE0 console.
   - `blinky/spim23_nrf.s/.bin`: SPIM2 (0x40023000) TX DMA (4 B `01 02 03 04`) + SPIM3 (0x4002F000) RX DMA (4 B) → prints `S2TX:OK` / `S3RX:OK`.
   - Tests in `src/cpu/tests.rs`: driver-style take → RAM move → complete phases, assert source bytes + console markers + `reset_globals()` 2nd-run hygiene. Follow `nrf_dma_driver_roundtrip` pattern (lock_uart + clear + lock_boot + boot + phased run).
   - Fix in `twim_nrf.rs::arm_nack`: `if name.starts_with("SPI") { nack_at=None; return; }` — SPI has no address phase; without this the staged SPIM2/3 DMA is cleared ~6000 instr before driver take.
   - Verified: both `.bin` rebuild identical (`as` + `ld -T blinky/link_nrf.ld` + `objcopy -O binary` + `cmp`).
4. **Lost work (needs redo, user chose "Tests first")**:
   - Batch 3 depth tests 207–211 (RTC/PWM/RNG/TEMP/EGU second proofs) — were in working tree, lost.
   - P108 race fixes (try_borrow guards, NACK per-instance clock, tap locks) — stashed then dropped after a bulk regex broke `scb.rs:166` (`unexpected closing delimiter`). Lesson: never bulk-regex `sys.p.nvic` borrows; scb.rs ICSR read path has `if` without braces that regex mangled. Redo by hand, one file at a time, `cargo test` after each.

## 2. Recovery plan (user decision: Tests first)

- [x] Batch 2 rebuild (done, in tree: 2 firmware proofs + SPI-NACK guard)
- [x] Batch 3 depth tests 207–211 (done, in tree: RTC COMPARE+OVRFLW, PWM STOP+INTEN+SEQ1, RNG SHORTS+re-arm, TEMP INTEN+STOP, EGU channels+mask — 211 green single + 25/25 parallel)
- [x] P108 race-fix redo — CLOSED by evidence, no redo needed (see §4)
- [x] Full verify matrix (done 2026-09-18: 211 single, 25/25 parallel, handshake 18/18, smoke OK, mpy OK, E2E 42/42, browser 16/16, pkg rebuilt)
- [x] Commit P110 (33d7892)
- [x] Doc sync P111: STATUS (counts/rows/LEFT#5/P109-pumps/verify), COVERAGE (counts/rows/proofs/LEFT#5+#7–9/verify+worktree), doc.html (pill/key/UARTE1+SPIM2-3/depth/crypto rows/checks/footer), about.html (17 proofs, 211), plan P91+P92
- [x] Verify P111 (cargo 211 green + stale-grep clean) + commit 3f95560
- [x] P112–P113 lab + P114 blitz (UNCOMMITTED in tree): TRUE-UICR rerun, LEFT rewrite, NFCPINS gate, BLE bond/TX-flow/periph/param-update, ST7789 part, WebAudio sink, FPU comment fix, sd_evt phase-1 + proof (217 green, docs synced)
- [x] Full matrix re-verify on P114 tree (217 single, ~30/31 parallel, 18/18, smoke+mpy OK, E2E 42/42, browser 16/16, bins identical, pkg in sync)
- [x] P115 stale-doc cleanup (STATUS:456/LEFT-5/verify/§7-dups, COVERAGE:265/§8-intro/LEFT-5, about.html bond/NFC/crypto rows) + P116 LEFT-1 closure (STATUS §6.1, plan §97): TRUE-seed park repro, 0x28290-waiter regs (r0=TWIM1, TXSTARTED poll), DRDY identical, TWIM1 1-transfer audit, SVD re-read (0x40004000=TWIM1, +0x150=TXSTARTED), FULL-pump escape ~176M → banner ~237.8M → `print(1+2)`→`3` (zero faults); `get_uart_output` TAKE trap; MC `0x2000207B` parked-check.
- [x] P117 bench verdict — NO WIRING NEEDED (plan §98): `lsm303.js` already completes both directions; page probes `p21` (banner 22.1M) + `p22` (REPL 125 B) + `p23` (banner T+15s wall, 104–105 B) + `p24` (`print(1+2)`→`3` +10s, zero page errors); browser 16/16 re-green; cargo 217 + handshake 18/18 re-green. Docs (STATUS §6.1/§4, COVERAGE §3/§5/§6, about.html) updated.
- [x] Commit P115–P117 docs (5 files, no code; user approved commit+push) — committed `82cf9b7`
- [x] Push P114 + P115–P117 (`dd39aa8..82cf9b7 master -> master`, exit 0) — in sync with origin

## 3. Batch 3 spec (to rebuild)

Current per-file state (all thin):
- `rtc_nrf.rs`: 1 test (`tick_sets_event`). Add: COMPARE match + IRQ fire + OVRFLW wrap (24-bit counter overflow sets EVENTS_OVRFLW).
- `pwm_nrf.rs`: 1 test (`seqstart_chains_to_end`). Add: STOP→STOPPED + INTEN gating (SEQSTARTx fires IRQ 28/33/34/45 only when enabled; STOPPED event + clear-by-write-0).
- `rng_nrf.rs`: 1 test (`start_yields_nonzero_value`). Add: SHORTS VALRDY→STOP (running clears) + VALUE-read re-arms (next VALRDY) + IRQ 13 gating.
- `temp_nrf.rs`: 2 tests (`datardy_and_temp_value`, `host_driven_temperature`). Add: INTEN IRQ 12 gating (START fires only when enabled; DATARDY clear-by-write-0).
- `egu_nrf.rs`: 2 tests (`trigger_fires_irq_when_enabled`, `all_instances_live_in_map`). Add: multi-channel independence (TRIGGER[0..15] → TRIGGERED[n] only that bit; INTEN mask per-bit).
- Convention: unit tests use `test_dummy_system()` + direct model read/write (no firmware needed for these); assert event set + IRQ pending via NVIC ISER + clear path. Keep small diffs, one peripheral per edit, `cargo test` green after each.

## 4. P108 race-fix status — CLOSED, no redo (2026-09-18, evidence)

The planned "P108 race-fix redo" (try_borrow guards + NACK clock + tap locks)
is NOT needed — it described stashed exploratory edits from the recovery
session, not a real gap:

- The committed P108 fix (BOOT_LOCK join in all sd_ble tests, HEAD~1
  `c5b61ee`) holds: full suite parallel green on this tree with all
  Batch 2+3 additions (last loop just run). No `RefCell already borrowed`
  in 25 consecutive parallel runs.
- The single `pregion_subs_include_exclude` panic seen earlier (`mwu_nrf.rs:237`)
  appeared in a 6-run loop (2 fails) then never reproduced in 25+8 runs —
  same rare pre-existing rate as documented (~2/15 pre-P108, now far lower).
  Mechanism per plan P108: a parallel cpu/mwu test swaps INSTALLED SYS
  mid-`watch → mwu_note` while the MWU slot is borrowed. All MWU/sd_ble/cpu
  tests already hold BOOT_LOCK; residual is scheduling noise, not a model bug.
- The stash containing the try_borrow/NACK-clock/tap-lock experiments was
  dropped AFTER verifying the tree builds + 204 green (pre-Batch-3 count) without it; the only
  keeper (SPI `arm_nack` guard) was re-applied by hand to `twim_nrf.rs` and
  is covered by `nrf_spim23_dma_roundtrip`.
- Do NOT bulk-regex `sys.p.nvic.borrow` → helpers: it broke `scb.rs:166`
  (braceless `if`s in the ICSR read path). Any future borrow hardening must
  be hand-edited one file at a time with `cargo test` after each.
- Reopen only if parallel failures exceed ~1/25 with a NEW backtrace pointing
  at a specific unguarded site; then fix that site alone (hand edit).

## 5. Scope lock (AGENTS.md)

- Target only nRF52833 Cortex-M4F. No STM32/UNO-R4/M0+/DAPLink.
- Never edit `src/cpu/` (decoder/stepping/regs) for board issues.
- One clock: `INSTRUCTION_COUNT` + `tick()` + `tick_n`. Never invent second clock.
- New peripheral: `src/peripherals/<name>_nrf.rs`, struct+Default, `new(name)`, `read/write/tick/as_any_mut`, register in BOTH `from_svd` + `new_wasm` with correct base.
- Nordic style: TASKS write-1-start, EVENTS read-clear write-0, INTENSET/CLR. Unlisted offsets read-0/ignore.
- Flash 512 KB @0x0, RAM 128 KB @0x20000000, VTOR 0. FICR/UICR or boot hangs.
- Validation: `cargo test` green after each peripheral; new peripheral = boot marker + functional marker + 2nd run.

## 6. Verify matrix (run in order, stop on red)

```
cargo test --manifest-path nrf52833-periph-wasm/Cargo.toml -- --test-threads=1  # expect 263 green
cargo test --manifest-path nrf52833-periph-wasm/Cargo.toml --lib -- --list 2>/dev/null | grep -c ": test"
node demo/parts/handshake.mjs          # 18/18 (rebuild via npm run build:handshake --prefix demo after Rust changes)
npm run test:parts --prefix demo ; npm run test:mpy --prefix demo
python3 tools/ble_air_bridge.py --port 18771 & node demo/parts/ble_live_e2e.mjs ws://127.0.0.1:18771  # 42/42
python3 -m http.server 8080 --directory demo & python3 tools/browser_verify_16.py  # 16/16
npm run build:wasm --prefix demo  # after Rust changes (committed demo/pkg)
```

Firmware rebuild: `TC=$HOME/.arduino15/packages/STMicroelectronics/tools/xpack-arm-none-eabi-gcc/14.2.1-1.1/bin/arm-none-eabi-; ${TC}as -march=armv7e-m -mfloat-abi=hard -mfpu=fpv4-sp-d16 -o /tmp/x.o blinky/<n>.s && ${TC}ld -T blinky/link_nrf.ld -o /tmp/x.elf /tmp/x.o && ${TC}objcopy -O binary /tmp/x.elf /tmp/x.bin && cmp /tmp/x.bin blinky/<n>.bin`

## 7. File map

- `nrf52833-periph-wasm/src/cpu/tests.rs:19` `boot()` (installs WasmSystem, no flag hygiene — P108 worksite), `:147+` Batch 2 tests, `:193+` `nrf_dma_driver_roundtrip` (pattern ref).
- `nrf52833-periph-wasm/src/peripherals/twim_nrf.rs:172` `arm_nack` (SPI guard), `:502+` take/complete DMA, `:489` `base_of` (SPIM2 0x40023000, SPIM3 0x4002F000).
- `nrf52833-periph-wasm/src/peripherals/{rtc,pwm,rng,temp,egu}_nrf.rs` Batch 3 worksites.
- `nrf52833-periph-wasm/src/system.rs:15` BOOT_LOCK/UART/I2C locks, `:425` `reset_globals` (note: does NOT reset INSTRUCTION_COUNT by design).
- `nrf52833-periph-wasm/src/peripherals/mod.rs:344` `new_wasm` map (SPIM2/UARTE1/SPIM3 rows).
- `blinky/{uarte1,spim23}_nrf.{s,bin}` Batch 2 firmware; `blinky/link_nrf.ld` linker script.
- Docs: `STATUS.md` (counts §3, LEFT §6), `plan.md` (append P110+, never rewrite), `HANDOVER.md` (stale at 535cfcb/203 — this file supersedes for state).

## 8. Traps

- Bulk regex on `sys.p.nvic.borrow` breaks `scb.rs` ICSR braceless `if`s. Hand-edit only.
- `boot()` + `init_for_test` swap global SYS with no join — every SYS-touching test must hold `lock_boot()`; UART-asserting tests also `lock_uart()`; TWIM0 tap sharers `lock_i2c_tap()`. `try_lock_uart` in `boot()` must never block (deadlock: marker tests hold UART across boot).
- `reset_globals()` deliberately keeps INSTRUCTION_COUNT (peripherals capture last_tick at construction; zeroing breaks elapsed math).
- SPI never NACKs; I2C NACK ~6000 instr without slave. `slave_present()` matches exact then norm7 (`0x32→0x19, 0x3C→0x1E, 0x72→0x39, else >>1`).
- UARTE TX snapshot: STARTTX latches bytes via thread-local RAM guard (putc `&c` reuse); tests must not assume late `mem_read` equals staged bytes.
- Never commit `/tmp/opencode/*`, `tools/__pycache__/`, `.openchamber/`, stray `pkg/parts/demo` trees. Bins ARE committed by policy.
- Small diffs, one peripheral per commit, `cargo test` green before every claim.

## 9. Log

- 2026-09-18: created this file; tree = HEAD 539cf19 + Batch 2 (2 tests + SPI guard + 4 blinky files), 206 listed. Next: Batch 3 depth tests.
- 2026-09-18 (Batch 3 done): +5 tests (RTC COMPARE/OVRFLW incl. OVRFLW-IRQ model fix; PWM STOP/INTEN/SEQ1 on PWM1; RNG SHORTS/re-arm; TEMP INTEN/STOP; EGU per-channel+INTENCLR) → 211 single green, 25/25 parallel green. P108 redo closed as not-needed (evidence). Next: verify matrix + docs + commit/push.
- 2026-09-18 (P110 committed 33d7892): full matrix green (211/25-25/18-18/E2E-42/browser-16/16), pkg rebuilt+committed.
- 2026-09-18 (P111 doc-sync committed (see `git log --oneline -1`; amended: +HANDOVER banner, LEFT numbering)): STATUS+COVERAGE+doc.html+about.html 204→211 + P109/P110 rows; plan P91+P92; fixups (eighteen checks, LEFT-9 head, verify line). Pending: push (ahead 3).
- 2026-09-18 (P114 out-of-scope blitz, UNCOMMITTED): user verdict "no walls — code through each". NFC NFCPINS gate (UICR 0x20C → GPIO 09/10 + NFCT sense, test); BLE bond store (keys + bridge bond_keys leg, test); BLE TX-flow + periph CONNECTED + param-update (tests + pump/bridge legs); edge-SPI ST7789 part + bench UI (headless-verified); I2S WebAudio sink (gesture-gated); lazy-FPU stale-comment fix (already implemented); sd_evt phase-1 (`sd_evt.rs` + hook + NVMC post + `sd_evt_nrf.s/.bin` proof, id=2 verified live); npm pack dry-run OK (22 files/72.5 kB). Suite 217 green. Docs synced (STATUS/COVERAGE/doc/about/API/plan). NEXT: full matrix + commit decision + push.
- 2026-09-18 (P117 bench verdict, docs UNCOMMITTED): NO WIRING NEEDED — `lsm303.js poll()` already takes→completes both TWIM directions; starvation was probe-only. Page proof: banner T+15s (104–105 B) + `print(1+2)`→`3` +10s, zero page errors; browser 16/16 re-green. NEXT: commit P115–P117 docs + push (user approval).
- 2026-09-18 (P118 COMMITTED `e7c28ba`, 15 files): ACL regions + nRF FPU-engine stub + kl27.js + bench panels + docs. 220 green + smoke + handshake 18/18 + browser 16/16, pkg rebuilt. Push pending user approval.
- 2026-09-19 (P119 in tree, UNCOMMITTED — awaiting user approval to commit): SIGNED/PREP/EXEC write path + driver-posted request/report/timeout/user-mem/authorize legs + TX-power/adv-state store + radio link-budget RSSI + SIGNED-WRITE_RSP mock fix. Suite 223 single green; handshake 18/18 (was 17/18 pre-fix: `SIGNED WRITE_RSP missing`); smoke OK; browser 16/16 re-verified (boot/self-test/depth, zero page errors); both pkgs rebuilt (`demo/pkg` + `demo/parts/pkg-test-handshake`). Docs synced (STATUS §1/§3/§5-LEFT/§8-verify, COVERAGE §1-GAP/GATT/gaps/RADIO/§proofs/§verify, doc.html BLE+RADIO rows + BLE boundary + footer counts, about.html count). NEXT: commit per approval (see §9 for the file list).
- 2026-09-19 (P120 in tree, UNCOMMITTED): SERVICE_CHANGED gated indication (SC-enable latch at ENABLE + 0x2A05 CCCD indicate gate, tag-16 air job, SC_CONFIRM with conn head — bridge `ble_sc` leg + pump + mock 7e leg) + scan/adv role-slot + whitelist arbitration (SCAN BUSY/INVALID_STATE + S132 param/whitelist validation, ADV CONN_COUNT/IN_USE legs, cross IN_USE test) + SC_CONFIRM conn-head fix (was header-only, broke strict mock asserts). Suite 224 single green; handshake 18/18; browser 16/16 re-verified (self-test needed a SCAN_STOP-tolerant mock: shared core keeps the observer slot live); both pkgs rebuilt. NEXT: commit per approval.
- 2026-09-19 (P121 in tree, UNCOMMITTED): roles firmware `blinky/ble_fw/ble_roles_fw.c` (xpack GCC + link_c_nrf.ld, bit-identical rebuild, 21 BLER markers: ADV NULL/struct/IN_USE, SCAN NULL/BUSY/param/selective/cross-IN_USE, CONNECT CENTRAL role, SC range leg, DISCONNECT — 2nd-run clean via `nrf_ble_roles_fw_markers`) + MockBleSvc 7f ADV/SCAN legs (real SVC bytes, strict rc asserts: 0x3203/17/7) + depth-probe roles key. Suite 226 single green (= 116 cpu incl. 20 fw proofs); handshake 18/18; smoke OK; browser 16/16 (BLE probe now `pairing×2, roles`). NEXT: commit per approval.
- 2026-09-19 (P122 COMMITTED `ac8ee38`, 12 files): C++ face + TypeScript face + `test:ts` in `test:wasm`. Suite 226 (= 116 cpu incl. 20 fw proofs); handshake + smoke + mpy + ts all green; browser 16/16.
- 2026-09-19 (P123 COMMITTED `a525dfb`, 5 files): MPY banner+REPL committed proof (`run_mpy_repl.mjs`, `test:repl` in `test:wasm`): stock-hex boot with bench-exact recipe+pump — 105B banner + `print(1+2)`->`3`, zero faults, ~0.3s. Load-bearing: resets to APP table + TAKE-accumulate UART log. No model change. MakeCode re-verified PARKED.
- 2026-09-19 (P124 in tree, UNCOMMITTED): no-walls round — radio air (real CRC engine CRCCNF/POLY/INIT + RXCRC latch, nRF LFSR whitening, interference floor heating ED/CCA + RX stamp in log-power; 4 new wasm exports; native `crc_engine_whitening_interference_air` test) + ACL read-gate enforced in mem.rs (MWU-patterned armed flag, try_borrow_mut, bus fault + 0 on blocked reads) + S132 ble_ranges.h range rule (unallocated SVCs in 0x60..=0xBF answer NOT_SUPPORTED/NOT_ENABLED, never fault) + MockRadio154 stage-3 air legs (same surface, strict asserts). Suite 227 single green; handshake 18/18; smoke OK; browser 16/16; both pkgs rebuilt. NEXT: commit per approval.
- 2026-09-19 (P123 probes only, no code changes): MPY + MakeCode native repro on this tree+pkg with bench-exact pump. MPY: entry memcpy verified, FICR-SD branch correct, NVMC READY passes, NFCPINS skip correct, 0x29CD1 = AIRCR-wait honored with appBoot semantics -> 0x539E7 -> delay-loop park with TWIM flowing (txC=95/rxC=730 at 240M), P116-consistent, no fix indicated. MakeCode: 2 resets honored -> permanent 0x37F4F/0x37F77 park (2995/3000 hits), TIMER4/DIR0 never driven, no faulting config -> stays PARKED. NEXT: commit per approval.
- 2026-09-25 (P137 COMMITTED `e412bb5`): speed round — 20x20K batch (~24 MIPS meter, ~5s MPY banner), firmware 404 fix (7 vendored bins), microbitapi §1/§3 sync, npm files[] += firmware/.
- 2026-09-25 (P138 COMMITTED `c72c979`): bench redesign (silkscreen theme, loader grid + staged line, bus tags) + matrix mask fix (0xD8988000 → 0xD1688800, strobing "A", persistence render) + JS/TS GPIO examples in test:wasm.
- 2026-09-25 (P139 COMMITTED `37f6400`): JS+TS SVC faces live on the bench (in-page panel, scratch core, loopback pass).
- 2026-09-25 (P140 full final gate, this turn): cargo 238 + test:wasm 124 ok + E2E 42/42 over air + browser 16/16, zero page errors. Doc sync only; no model change.
- 2026-09-29 (P144 in tree, UNCOMMITTED): SVC 18 `sd_softdevice_is_enabled` + `test:mpy-radio` + MakeCode gate correction + doc sync. Suite 239 single green; test:wasm 128 ok; bench hex re-verified (MPY banner/REPL/radio-on, MC same park). NEXT: commit per approval.
- 2026-09-29 (P144 COMMITTED `f1ea2bb`, 13 files): SVC18 model + rebuilt `demo/pkg` wasm + `test:mpy-radio` + MakeCode gate correction (post1-never-returns) + full doc sync (STATUS/plan/agent/doc/about/index/package/README/ble_lang). Suite 239 + test:wasm 128 ok.
- 2026-09-30 (P145 in tree, UNCOMMITTED): vendored fresh `mc/built` smiley hex into `demo/firmware` (1247 differing bytes, user section `0x47000`+) + preset label `showString`→`showLeds`; shipped-artifact park probe 7/7 (idle, 2 resets, zero faults, DIR0/T4/matrix dark); stock limits re-verified (bluetooth 0 hits, SVC82 0 in both hexes; radio TX proof green). NEXT: commit per approval.
- 2026-09-30 (P145 COMMITTED `81a33a2`, 4 files): vendored hex + label + agent/plan sync. Suite 239 + test:wasm 128 ok re-verified.
- 2026-09-30 (P146 in tree, UNCOMMITTED): README `showString`→`showLeds` + agent snapshot to P145-committed state. NEXT: commit per approval.
- 2026-10-03 (P153 SAADC conversion-latency FIX + fresh smiley build — UNCOMMITTED,
  do not commit per standing order): ROOT CAUSE PROVEN for the MakeCode wedge:
  zero-time SAADC END inverted init order (ISR fired before driver registered
  its vector/queue → batch orphaned → main fiber parked forever on the stream
  wait). Fix (`saadc_nrf.rs`): TASKS_SAMPLE with MAXCNT>0 stages the RESULT
  offer only after MAXCNT×768 instr (TACQ+TCCONV silicon estimate; 256-sample
  batches ≈196k steps ≈3ms, dwarfing registration windows); STOP aborts pending;
  +`conversion_latency_orders_init_race` test. Proof: Sept-29 hex boots to USER
  CODE with fix (P0.02 HIGH@~36M steps deterministic virtual-time pump, was LOW
  without) — first user execution in emulation. Fresh `pxt build` smiley
  (`mc/main.ts` showLeds, target v9.1.1) vendored to `demo/firmware` (now truly
  matches source); same wedge shape (identical pc/LR/regs/cell/table) but NOT
  unblocked at 72M steps: its SAADC order is already correct (reg@70k <<
  stage@762k) yet ISR bails (STARTED==0, no copies/post ever); TIMER2 verified
  free-running, PPI edge model intact, all other peripherals untouched/disabled.
  mpy-radio flake characterized (2/3 vs 3/3 pristine; change is dead code there —
  no takes — full test:wasm green incl. radio on retry). NEXT: see P154 (fix
  verified on Sept-29 hex; fresh build needs its post-sensor stall named).

- 2026-10-03 (P154 fix VERIFIED (3x deterministic) + fresh-build triage — UNCOMMITTED,
  do not commit per standing order): Sept-29 hex boots to USER CODE with the
  P153 fix (P0.02 HIGH@~36M steps, reproduced 3x incl. fully virtual-time pump)
  — first user execution in emulation. Needs BOTH fix + toggling DRDY edges
  (static-HIGH DRDY never boots: sensors silent without data-ready edges —
  2-gate model confirmed). Fresh smiley hex does NOT boot (150M steps, dark):
  same wedge SHAPE but different STALL POINT — sensors COMPLETE init (~250 TWIM
  then plateau), SAADC single batch, TIMER2 one-shot stopped, then silence (no
  user), yet system stays ALIVE (display strobes, polls cycle, ISRs deliver,
  zero faults). Cell 0x3d is STALE (persists while system cycles — not the live
  blocker). The 0x10e/0x37f4f faults are Heisen probe artifacts (wall-clock
  DRDY + fine-chunk timing), absent in deterministic runs — no emulator fault
  bug. NEXT: fresh build's post-sensor stall (streaming trigger never fires).
- 2026-10-03 (P155 wedge-2 triage: same shape, different stall — UNCOMMITTED, do
  not commit per standing order): fresh smiley does NOT boot (150M steps, dark)
  despite SAADC order verified correct (reg@70k << stage@762k) + forced 31
  batches + TIMER2-drive + QSPI servicing + realistic sensor data: sensors
  COMPLETE init (~250 TWIM then plateau), SAADC single batch consumed, TIMER2
  one-shot stopped, then silence with system ALIVE (display strobes, polls
  cycle, ISRs deliver, zero faults). Binaries differ ONLY at 0x47000 (user);
  state/regs/RAM/pc-sets IDENTICAL at 6M steps — yet outcomes differ, so user
  bytes are read pre-user (header/globals init) shifting a timing race, OR a
  second producer is missing. 0x10e/0x37f4f faults proven Heisen artifacts
  (wall-clock DRDY + fine-chunk timing), absent deterministic. No stubs exist
  (zero TODO/unimplemented; reserved-offset defaults only). NEXT: wedge-2
  producer (candidates: post-sensor streaming trigger; SECOND SAADC batch path).
- 2026-10-03 (P156 wedge-2 ROOT CAUSE (deterministic, instruction-level proof) —
  UNCOMMITTED, do not commit per standing order): the "Heisen" verdict in P155
  is WITHDRAWN for the fault class: NEW dives 100% deterministically (slow
  faithful wall clock t/64000). Primary: delivered HardFault@~32M (ipsr 3,
  CFSR 0x8200 PRECISERR+BFARVALID, BFAR 0x118000), parked in default `b .`
  0x37f4e (NOT zero-faults: fault_pc only reports UNDELIVERED halts). Chain
  (single-stepped): TIMER1 handler tail-calls E 0x302d8 via `b.w` (0x303f2/
  0x303f8) with LR=EXC_RETURN → E `pop {r4-r11,pc}` loads EXC_RETURN →
  exception_return unstacks live-caller-below regs as frame → resumes stale
  retpc mid-S 0x33e34 (or 0x0/MBR, or RAM data) → S-tail `pop {r4,pc}` loads
  even → branch fault 0x33e3a (or MBR 0x10e `pop {r0-r4,pc}`, or erased-flash
  0x703fc). Every step ARM-correct (LDM writeback-then-branch verified
  silicon-faithful; xpsr ITSTATE=0 at faults; SPSEL=0 so banks moot; masks
  audited) — NO cpu/ or peripheral misbehavior in the chain; Q (0x31cc0, 131
  visits) and S slots healthy when uninvolved. Q/E have ZERO `bl` callers:
  scheduler tail-chain co-routines, so stale-LR/unwind fragility is
  firmware-constructed; same binary+events on silicon dives identically.
  Phase proof (10 DRDY offsets, slow clock): 2/10 dive (O0 S@w114, O120 MBR@w82),
  8/10 clean-park (WFE 0x37afa, one at 0x33ddc) — dive is event-alignment
  detonated, firmware-race class. No stimulus reaches user yet (matrix dark):
  stall (pre-user, display init never completes first frame) is the remaining
  blocker, dive its noisy neighbor. NEXT: OLD-vs-NEW fiber/event forensics
  6M→12M (which fiber/event fires in OLD but not NEW) for the stall; no src/
  change indicated (270 green re-verified).
- 2026-10-03 (P157 SMILEY RENDERS + render-rule fix + SENSE fix — UNCOMMITTED,
  do not commit per standing order): full-coverage trace proves NEW user main
  RUNS at w30 (both hexes, same pump!) — no pre-user stall; display setter
  runs (image copy, flag 7), blocks in fiber-wait; renderer/state-machine
  invocation mapped (0 hits); single fiber proven. Root visibility gap (not a
  model stub): CODAL drives matrix COLUMNS via GPIOTE tasks (DIR stays input
  by design; per-frame brightness in task OUTINIT, cleared=bright) with rows
  GPIO-selected HIGH — matrix_state()'s both-DIR-output + row-LOW rule only
  fits bit-bang firmware. Fix (lib.rs matrix_state + bench frame loop, same
  switch): task-bound cols use CODAL rule, else GPIO rule (bit-bang proofs
  untouched). Proof: real matrix_state() + persistence shows the EXACT smiley
  (9/9: 1,3,6,8,15,19,21,22,23) on unmodified NEW; OLD stays dark (no false
  positives). SENSE gap also closed (OUT/DIR writes re-evaluate SENSE;
  GPIOTE poll uses IN-mixed level) + test — silicon-faithful, untriggered
  here (matrix pins SENSE=0). Gates: cargo 273 + test:wasm green, both
  pkgs rebuilt. Standing dive (P156 firmware race, post-user, phase-
  sensitive) unchanged by design — faithful emulation. NEXT (P158 done:
  E/S/W singles lit, rotation sequencing narrowed to fiber/event
  bookkeeping): waiter-cell semantics for the parked forever fiber +
  scroll glyph phase-alignment; commit+push approved 2026-10-04.
   P159 2026-10-04 DONE: no stall exists — blinky proves sleeps/wakeups;
  rotation dies ~72M in E-epilogue stale-pop HardFault (same class as
  smiley), scroll dies ~28M in queue-walk fault (same family); both
  renders proven pre-fault. NEXT: commit+push on approval; npm login
  for publish (user); waiter-cell work CLOSED (superseded by fault
  proof).