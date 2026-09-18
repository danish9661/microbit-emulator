# agent.md — live handover (micro:bit v2.2 nRF52833 emulator)

> Living file. Update every working turn: HEAD, `git status -sb`, test count,
> todo states. Trust this over memory. Details in `STATUS.md` / `plan.md` /
> `HANDOVER.md` (HANDOVER stale at 535cfcb/203 — this file supersedes for state).

## 0. Snapshot (2026-09-18, pushed through P115–P117, in sync with origin)

- HEAD: `82cf9b7` "P115-P117 docs: stale cleanup + REPL exec closed (banner T+15s wall, print(1+2)->3 in-browser, 217 green)".
- Branch: `master`, remote `git@github.com:danish9661/microbitemu.git`, in sync with `origin/master` (P114 `be02b89` + P115–P117 `82cf9b7` both pushed).
- Suite: **217 single green** (pre-commit gate re-run), = 114 cpu (incl. 18 firmware proofs) + 89 peripherals + 11 sd_ble + 3 sd_evt. Handshake 18/18, browser 16/16 re-verified.
- Working tree: CLEAN except `?? .openchamber/` (ignore — never commit).
- Big news: **LEFT-1 CLOSED P116+P117, proven in-browser** — banner at T+15s wall + `print(1+2)` → `3` at +10s via the shipped page (zero page errors). No code changed: the bench pump (`lsm303.js` full take→complete both directions) was already correct; starvation lived only in ad-hoc native probes.

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
cargo test --manifest-path nrf52833-periph-wasm/Cargo.toml -- --test-threads=1  # expect 217 green
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
