# HANDOVER — micro:bit v2.2 (nRF52833) emulator → next agent (P108+; refreshed 2026-09-16)

> SUPERSEDED SNAPSHOT (original §1/§10/§11 kept below for the record —
> they describe HEAD `535cfcb`/203 tests/all-pending, which is 4+
> commits stale). Current state: HEAD `2aaf0ba`, 204 tests green in
> BOTH thread modes (25/25 parallel post-P108), all 13 items below
> DONE and pushed. The remaining sections (§2 architecture, §3 matrix
> minus counts, §12 traps, §13 file map, §14 cheat sheet, §15 glossary)
> are still accurate — only counts/HEAD changed (203→204).
>
> Opencode todo list — ALL 13 DONE (kept for the record; do NOT re-do):
> A1–A5 pairing-fw proof (P104, committed `cefa85c`), B1–B4 flake
> harness part 1 (P105 deterministic MPU-gate, committed `77a4ffb`),
> C1–C2 frontiers parked (P106 LEFT-4 pump-entry waiter + P107 LEFT-3,
> committed `61b63f3`), final gate + pkg rebuild + push (committed
> `2aaf0ba`, pushed through it — `git status -sb` shows clean vs
> origin except untracked `.openchamber/` + this file).
>
> Open item at refresh time (uncommitted work in tree): P108
> parallel-flake part 2 — `BOOT_LOCK` join in all 10 sd_ble tests
> (`nrf52833-periph-wasm/src/sd_ble.rs`, +22 lines, no model change):
> filtered `sd_ble mwu` parallel 20/20 green (was ~2/20), full suite
> parallel 25/25 green (was ~13/15), single 204/204 unchanged. Needs:
> full verify matrix (§3 — E2E + browser + pkg rebuild), doc touch-ups
> (STATUS §3 already rewritten for P108; plan P90 note appended;
> doc.html/about.html/COVERAGE §8 updated), then commit + push.
> No `src/cpu/` edits (AGENTS.md scope lock holds — only the two P105
> exit-disarm writes via the existing `mem.write32` harness path).

> Opencode todo list (13 items — load into `todowrite` verbatim, JSON in
> §10; start at the first `pending` item; mark `completed` only on
> evidence, never on intent; exactly one `in_progress` at a time):
>
> 1. `A1 compile ble_pairing_fw.c with xpack GCC + objcopy to .bin + objdump SVC check` (pending, high)
> 2. `A2 add nrf_ble_pairing_fw_markers test (2 runs, mid-spin pump, all BLEP markers)` (pending, high)
> 3. `A3 static SVC rescan of MPY+MC app regions (type-02+04 records, 0x1C000-0x77000 filter)` (pending, high)
> 4. `A4 pairing-fw docs (STATUS counts, plan P104, ble_lang README row, doc.html bullet)` (pending, medium)
> 5. `A5 verify pairing-fw gate (cargo 204 single-thread + handshake + E2E) then commit .c+.bin+test` (pending, high)
> 6. `B1 reproduce flake deterministically ($BIN mpu mwu unalign, single-thread included)` (pending, high)
> 7. `B2 implement lock_boot join (boot() reset_globals/MWU+MPU clears + Cpu::new disarm)` (pending, high)
> 8. `B3 verify flake fix (filtered 10/10 both modes, full suite 5/5, single 203/203)` (pending, high)
> 9. `B4 flake docs (STATUS S3, doc.html checks, COVERAGE verify) then commit alone` (pending, medium)
> 10. `C1 MakeCode wait-queue-OBJECT walk at 0x20003b18 (name awaited event, trap raise site)` (pending, medium)
> 11. `C2 bootloader pass-2 trace assessment (SD priorities shelved?) or park LEFT-3` (pending, low)
> 12. `Final gate (cargo + handshake + smoke + mpy + E2E 42/42 + browser 16/16 + pkg rebuild)` (pending, high)
> 13. `Commit + push only when final gate fully green` (pending, medium)
>
> Machine-readable copy of the same list lives in §10 (JSON).

> Read this file top-to-bottom, then load the todo list in §10 into
> `todowrite` and start at the first `in_progress` item. Do NOT re-do
> green work. Small diffs, one step per commit, `cargo test` green
> before every claim (AGENTS.md §Validation/Workflow).
> Tree state at handover: HEAD `535cfcb`, clean except
> `blinky/ble_fw/ble_pairing_fw.c` (untracked, uncompiled) and
> `.openchamber/` (ignored). Suite: 203 tests, `cargo test`
> deterministic green only with `-- --test-threads=1`.
> (STALE — see refresh banner at top for current HEAD/counts.)

## 0. How to use this file

- §1 pins the repo snapshot (HEAD, counts, key paths). Trust it over memory.
- §2–§3 give the BLE architecture + verify matrix so you never re-derive them.
- §4/§5/§6 are the three ordered workstreams the user asked to "do all":
  BLE-enabled image proof, parallel-flake harness fix, non-BLE frontiers.
- §7/§8 list validation commands + docs to update per step.
- §9 names suggested skills to load via the Skill tool.
- §10 is the opencode todo list (JSON). Load it verbatim into `todowrite`.
- §11 is the 15-minute quickstart. §12 records traps that already burned time.
- Specs/plans live in `plan.md`, `STATUS.md`, `docs/COVERAGE.md`,
  `demo/API.md`, `demo/parts/ble_lang/README.md`, `docs/README.md`,
  `docs/cpu_bug.md`, `docs/sd_evt_design.md` — referenced, not duplicated.

## 1. Repo snapshot (trust this, not memory)

- HEAD: `535cfcb` "Docs: BLE fully-answered verdict (added vs left) +
  parallel-flake characterization".
- Parent chain: `8c14b2a` (P103 SMP legs + conn-RSSI + multi-peer + ATT
  queue) ← `21ee9ce` (P102 live E2E) ← `08824e6` (P101 bench) ←
  `f62e855` (P100c pkg) ← `4d67f91` (P100a multi-conn+L2CAP+pairing).
- Branch `master`, remote `git@github.com:danish9661/microbitemu.git`
  (pushed through `535cfcb`; verify with `git status -sb`).
- Suite: 203 tests (`cargo test -- --list | grep -c ": test"` = 203).
  Deterministic green ONLY single-threaded; parallel default flakes
  ~1/4 runs (§5). Never claim green without stating thread mode.
- JS suites: `handshake.mjs` 18/18, `smoke.mjs` green,
  `run_mpy_face.mjs` green, `ble_live_e2e.mjs` 42/42 over air (two
  links), `browser_verify_16.py` 16/16 + zero page errors (P103 run).
- Untracked work: `blinky/ble_fw/ble_pairing_fw.c` (130 lines, NEW this
  session, uncompiled, no `.bin`, no test wires it yet). Everything
  else in `blinky/ble_fw/` is committed (`.bin` + `.c` pairs).
- Ignored: `.openchamber/` (screenshots only), `tools/__pycache__/`
  (regenerates), `demo/parts/pkg-test-handshake/` (has own `.gitignore`
  `*`, rebuilt by `npm run build:handshake`), `mc/` (gitignored
  MakeCode project; `mc/built/` hexes present on disk).
- Crate layout: `nrf52833-periph-wasm/src/{lib.rs,system.rs,sd_ble.rs,
  cpu/{mod.rs,mem.rs,regs.rs,tests.rs,thumb.rs},peripherals/*.rs,
  ext_devices/}`. AGENTS.md scope lock applies: nRF52833 only, never
  edit `src/cpu/` for board issues, one clock, small diffs.
- Key numbers: 40 `ble_*` wasm exports (`grep -c "pub fn ble_"
  src/lib.rs` = 33 + take/data variants); 10 sd_ble native tests;
  16 BleJob tags; 67 S132 BLE SVC numbers claimed (`0x60..=0xBF`
  hook in `cpu/thumb.rs:1437-1451`); bridge peers ×2 (87/64).
- Toolchains present: xpack GCC 14.2.1
  (`~/.arduino15/.../xpack-arm-none-eabi-gcc/14.2.1-1.1/bin/`,
  `docs/README.md` flags), `arduino-cli` 1.5.1 + `arduino:nrf52`
  core, `makecode` 1.3.6 + `pxt` 0.5.1 (npm-global), pip
  `playwright` 1.62.0 + bundled Chromium (+ ms-playwright
  `chromium-1234`), Bumble 0.0.231, node v22.22.2. No `claude`
  binary on PATH (handoff launch was skipped for that reason).

## 2. BLE architecture (do not re-derive — reference)

- `src/sd_ble.rs` (~3362 lines) is the SVC face, NOT a Peripheral (no
  MMIO base, AGENTS.md-compliant). Header comment pins every number
  to S132 headers in the arduino nRF52 package (`ble_ranges.h`,
  `ble.h`, `ble_gap.h`, `ble_gatts.h`, `ble_gattc.h`, `ble_gatt.h`,
  `ble_types.h`, `nrf_error.h`, `ble_err.h`). SVC bases: common
  `0x60` (12), GAP `0x70` (32), GATTC `0x90` (32), GATTS `0xA0`
  (16), L2CAP `0xB0` (3 used). Comment block also pins register
  contracts (r0..r3), event wire layouts (unpacked, pad bytes), return
  codes, UUID/write-op/HVX/role/handle sentinels.
- State: process-wide singleton `SD_BLE_STATE: RefCell<SdBle>`
  + `TAKE_DATA: RefCell<Vec<u8>>` (WRITE/HVX/L2CAP bytes staged at
  SVC time). Reset via `reset_for_test()` (fresh + take-data clear;
  wired into `system.rs:reset_globals`). Per-link `Conn` table
  (handle/up/peer/role/rssi/tx_count/encrypted/bonded/pairing/cids);
  global GATTS attr table; `staged: Option<BleJob>` (exactly one job).
- Pairing machine per link: Idle/Requested/PeerRequested/Accepted/
  KeyEntry{key_type}/LescDhkey{oobd_req}/EncryptPending. Initiator
  path (AUTHENTICATE stages GapAuthenticate, driver completes via
  complete/fail_pairing → AUTH_STATUS + CONN_SEC_UPDATE). Peer path
  (driver posts request events via `post_*`, firmware answers reply
  SVCs; accept needs outstanding request else INVALID_STATE).
- Take/complete discipline (same as every EASYDMA peripheral):
  firmware SVC stages one BleJob → driver drains `ble_take_job()`
  (+ `ble_take_data()` once for byte jobs) → resolves over air →
  calls matching `complete_*/post_*` → firmware drains `sd_ble_evt_get`
  (two-arg contract: dest NULL queries length; small room → DATA_SIZE
  without popping; legacy single-arg drain kept). Never ghost events;
  `cancel` means retry, never a fake RSP.
- `src/cpu/thumb.rs` SVC arm calls `sd_ble::handle_svc()` FIRST for
  `0x60..=0xBF`; `Some(r0)` writes r0 + skips past svc, `None` falls
  through to `raise_sync` untouched (zero-cost idle).
- Bridge `tools/ble_air_bridge.py` (~1169 lines): one LocalLink with
  C_emu (RADIO face) + C_peer/peer (battery 87 `PeerBatt` + NUS) +
  C_peer_hr/peer_hr (battery 64 `PeerHR` + NUS, distinct address) +
  C_central (client). JSON protocol over WebSocket documents every
  tag in the module docstring. Per-job `peer:[6]` routes ATT ops;
  disc RSPs echo answering `peer`; per-peer ATT locks + global scan
  lock serialize (no timeouts under load); `read_conn_rssi` probes
  HCI_READ_RSSI first (LocalLink → UNKNOWN_HCI_COMMAND, caught
  inside) then adv-sighting fallback tagged `src:"conn"|"adv"`.
- Pump `demo/index.html:pumpDma()` + `demo/parts/ble_air.js` (BleAir
  class) is the reference driver: `sendBle(job)` mirrors take tags
  (incl. `job.peer` forward), `takeAir()` drains replies into
  completions, `pumpBleLoopback` resolves locally when bridge down
  (battery 87 + fixed table). Handle learning via
  `ble_complete_gap_connect_ret` (never hardcode handle 1).
- Proofs, strongest-first: `ble_live_e2e.mjs` 42/42 over REAL bridge
  (two links, 87-vs-64 reads, FIRST_PEER echo, 2-sighting scan,
  RSSI overAir, order-sensitive); `MockBleSvc` (`demo/parts/mocks.js`)
  real-SVC-bytes flow 18/18 headless; `run_mpy_face.mjs` MPY-idiom
  contract; GCC `ble_conformance.c` (19 markers) + `c_ble_face.c`
  (C: markers) natively in `cargo test`; browser self-test +
  16 depth probes in real Chromium via `tools/browser_verify_16.py`.
- Deliberate non-goals (STATUS §7 + doc.html "fully done?" section):
  no SMP crypto/key/bond store, central-role only, no conn-param /
  MTU / DLE / PHY enforcement, no TX-flow / timeout / sec-request /
  rw-authorize / sys-attr events (reply SVCs ack SUCCESS), GATTC
  write REQ/CMD only (no signed/queued), no SoC/MBR SVCs, no
  BLE-enabled stock image, virtual air not RF (RSSI −50 const).

## 3. Verify matrix (run in this order, stop on red)

- `cargo test --manifest-path nrf52833-periph-wasm/Cargo.toml -- --test-threads=1`
  → expect `ok. 203 passed` (add +1 when pairing-fw test lands).
  Parallel default is NOT a gate (§5).
- `node demo/parts/handshake.mjs` → 18 `ok:` + `all handshake mocks OK`
  (needs `demo/parts/pkg-test-handshake/` built; rebuild via
  `npm run build:handshake --prefix demo` after Rust changes).
- `npm run test:parts --prefix demo` (smoke) + `npm run test:mpy
  --prefix demo` (MPY-idiom face) → both `OK`.
- Bridge E2E: `python3 tools/ble_air_bridge.py --port 18771 &` then
  `node demo/parts/ble_live_e2e.mjs ws://127.0.0.1:18771` → 42 `ok:`,
  0 FAIL, `all live-bridge E2E checks OVER AIR OK`.
- Browser: `python3 -m http.server 8080 --directory demo &` then
  `python3 tools/browser_verify_16.py` → boot BOOT/BLINK/BLINK
  ~6 MIPS + self-test `pairing×2` pass + `16/16 probes pass` + zero
  page errors.
- Rebuild committed pkg after Rust changes: `npm run build:wasm
  --prefix demo` (writes `demo/pkg/`, committed intentionally for
  Pages; `demo/pkg/.gitignore` + `parts/pkg-test-handshake/.gitignore`
  exist — do NOT delete). Firmware `.bin` rebuilds: xpack GCC per
  `docs/README.md` (`-mcpu=cortex-m4 -mthumb -mfloat-abi=hard
  -mfpu=fpv4-sp-d16 -O2 -nostdlib -ffreestanding -T
  blinky/link_c_nrf.ld`, `objcopy -O binary`).
- Docs to touch per change: `STATUS.md` (counts + §5/§8), `plan.md`
  (append P104+ notes, never rewrite history), `demo/API.md` (frozen
  v1 — additive only), `demo/doc.html` (matrix + "fully done?"
  section + counts), `demo/about.html` (scope), `docs/COVERAGE.md`
  (§8 tables), `demo/parts/ble_lang/README.md` (language table).

## 4. WORKSTREAM A — BLE-enabled image proof (highest priority)

- Goal: replace "no BLE-enabled stock image" with a real compiled
  image that drives ENABLE→GATTS table→CONNECT→GATT→AUTHENTICATE→
  CONN_SEC_GET→DISCONNECT end-to-end, plus a static SVC rescan of
  MPY/MC app regions. Starter file already written (untracked):
  `blinky/ble_fw/ble_pairing_fw.c` (CODAL-BLE-shaped JustWorks flow,
  `BLEP:` markers, `drain_until()` spin like silicon firmware).
- A1. Compile it with the pinned toolchain (do NOT invent flags):
  `TC=~/.arduino15/packages/STMicroelectronics/tools/xpack-arm-none-eabi-gcc/14.2.1-1.1/bin/arm-none-eabi-;
  ${TC}gcc -mcpu=cortex-m4 -mthumb -mfloat-abi=hard -mfpu=fpv4-sp-d16
  -O2 -nostdlib -ffreestanding -T blinky/link_c_nrf.ld -o /tmp/blep.elf
  blinky/ble_fw/ble_pairing_fw.c && ${TC}objcopy -O binary /tmp/blep.elf
  blinky/ble_fw/ble_pairing_fw.bin`. Expect RWX-LOAD-segment ld
  warning (harmless, documented). Verify halfwords with
  `${TC}objdump -d /tmp/blep.elf | grep -A2 'svc'`.
- A2. Add native test `nrf_ble_pairing_fw_markers` in
  `src/cpu/tests.rs` mirroring `nrf_ble_conformance_svc_face`
  (lines ~244-286): `lock_uart()` + clear, 2-run loop with
  `reset_for_test()` + `lock_boot()` + `boot(include_bytes!(
  "../../../blinky/ble_fw/ble_pairing_fw.bin"))`, `deliver_irqs=true`,
  500-instr slices × ~4000 with `pump_ble_test_driver(sys)` between
  slices (mid-spin pump is load-bearing — 20K slices starve the
  CONNECTED spin, plan P54), break on `BLEP:ALL-OK`/`SOME-FAIL`,
  assert no fault + every `BLEP:` marker + `reset_globals()` per run.
  Reuse `pump_ble_test_driver` unchanged (it already resolves
  GapAuthenticate via `complete_pairing(conn, true)`; CONN_SEC_GET is
  synchronous so no driver arm needed).
- A3. Static SVC rescan (evidence for docs): run the repo's ad-hoc
  scanners (recreate under `/tmp/opencode/`, never commit) over
  `mc/built/mbcodal-binary.hex` + `demo/firmware/
  micropython-microbit-v2.1.2.hex` handling BOTH type-02 (segment<<4)
  and type-04 (upper<<16) records, filter app region
  `0x1C000–0x77000`, decode `0xDF imm` halfwords at even addresses,
  map via S132 names. Expected (already measured): MC app shows
  ENABLE/EVT_GET/ADV_DATA_SET/ADV_START/CONNECT-equivalents etc.;
  MPY app shows ENABLE/EVT_GET/CONNECT/READ-class SVCs (its BLE is
  compiled but `MICROBIT_BLE_ENABLED=0` gates runtime init —
  codal.json in `mc/built/codal.json` confirms `"...BLUETOOTH_ENABLED":
  0`). Record counts in plan note; do NOT claim runtime BLE from
  static hits alone.
- A4. CODAL-BLE runtime question (explicitly bounded): a full CODAL
  `bleManager.init` boot is NOT required for this workstream. The
  pairing-fw image IS the "BLE-enabled image proof" (it runs the
  CODAL-shaped flow). Optionally boot `mc/built/mbcodal-binary.hex`
  with the P16 recipe (app VT `0x1C000`, MBR params, UICR) and sample
  pc to confirm scheduler-idle (known) — informational only.
- A5. Wire the new proof into docs + suites: `ble_lang/README.md`
  language table (+1 row: pairing fw, markers `BLEP:*`, test name),
  STATUS §3 counts (203→204), `demo/doc.html` depth list (BLE bullet
  already exists — extend, don't duplicate), `plan.md` P104 note with
  marker list + rescan table. `cargo test` must stay 204 green
  single-threaded before commit. Commit the `.c` + `.bin` + test in
  ONE commit (bins are committed in this repo by policy).
- Traps: SVC dispatcher macros (`SVC1/2/3` + manual 4-reg block for
  CHAR_ADD) must match `ble_conformance.c` exactly; `drain_until`
  bound 400 spins is enough at 500-instr slices; `sec_buf` expects
  mode `0x21` (encrypted) after driver `complete_pairing(..., true)`;
  `evt_buf[0]` is the u16 id low byte; 2nd-run-clean is mandatory
  (reset_state leak check); never add JS round-trip to Rust tests.

## 5. WORKSTREAM B — parallel-flake harness fix (join lock_boot)

- Symptom (characterized, open, pre-existing): stock parallel `cargo
  test` flakes ~1/4 runs; failures wander across sd_ble/MWU/MPU tests
  (`RefCell already borrowed` at `peripherals/mod.rs:131`, or MWU
  `REGION0.WA reads 0` / `left: 0 right: 1` asserts). Single-threaded
  `-- --test-threads=1` is ALWAYS 203/203. Old STATUS note about
  `unaligned_device_faults_without_trap` 1/10 is the same family.
- Minimal repro (deterministic!): filtered subset `mpu mwu unalign`
  fails ~9/10 even single-threaded when the unaligned cpu test runs
  before an MWU test in the SAME process; each test alone is always
  green; `mwu` alone (both tests) is green; `unalign` alone is green.
  Commands: `BIN=$(ls -t
  nrf52833-periph-wasm/target/debug/deps/nrf52833_periph_wasm-* |
  grep -v '\.' | head -n1)` then `$BIN --test-threads=1
  unaligned_device region_watch` (fails) vs `$BIN
  --test-threads=1 region_watch` (passes). Full suite flake rate is
  lower only because scheduling varies.
- Root cause (proven by the repro, NOT the earlier thread theory):
  process-global `MWU_ARMED: AtomicBool` (system.rs:68) leaks from the
  unaligned-device cpu test's `boot()` (which installs a system whose
  map has MWU disarmed but leaves the flag whatever the previous
  occupant set... precisely: the cpu test enables MPU via
  `0xE000ED94=0x5`, runs, and its `boot()`-installed system + flags
  persist; the following MWU test's `mem.write8` sees armed-stale or
  disarmed-stale mismatch and notes against the wrong map state, so
  WA never sets). The earlier "cross-thread foreign borrow" theory is
  WRONG for the deterministic repro (single thread fails too); the
  thread-safety angle only explains run-to-run variance. Evidence:
  stash-everything (clean tree) still reproduces; `TAKE_DATA` clear /
  `MWU_ARMED` reset in `reset_globals` / `boot()` hygiene /
  `unaligned_deny` reorder were all tried and REVERTED (none fixed
  the deterministic order-dependence; tree is clean of them now).
- Fix direction (requested: join lock_boot()): the MWU tests already
  hold `lock_boot()` across config+access+asserts and install via
  `init_for_test`, but the cpu `boot()` helper (`cpu/tests.rs:19`)
  installs WITHOUT the MWU-flag hygiene and cpu tests don't clear
  `MWU_ARMED`/`MPU_ENABLED`/`UNALIGN_TRP` on entry. Implement ALL of:
  (1) `boot()` calls `crate::system::reset_globals()` (or at minimum
  clears `MWU_ARMED`, `MPU_ENABLED`, fault-valid latches,
  `UNALIGN_TRP`, force-unpriv) AFTER `init_for_test` and BEFORE
  `Cpu::new` (Cpu::new already clears MPU-side latches — extend the
  same pattern); (2) MWU tests keep holding `lock_boot()` for the
  whole body (already true — verify, don't duplicate); (3) optionally
  add `mwu_set_armed(false)` to `Cpu::new` alongside the existing
  `set_mpu_enabled(false)` so every fresh CPU starts disarmed.
  Do NOT: make MWU_ARMED thread-local (hides the real leak), add new
  locks beyond BOOT_LOCK (deadlock risk: `try_lock_uart` exists
  because marker tests hold UART lock across boot()), or edit
  `src/cpu/` stepping/decoder (AGENTS.md forbids board-issue edits
  there; harness latches are the exception already blessed by
  `Cpu::new` clears).
- Acceptance: `$BIN mpu mwu unalign` green 10/10 single-threaded AND
  10/10 at default parallelism; then full `cargo test` green 5/5
  consecutive (any thread mode); then
  `cargo test -- --test-threads=1` still 203/203. Update STATUS §3
  flake note (replace "~1/4 + use single-threaded" with fixed
  wording + mechanism), `demo/doc.html` checks section (drop the
  flake bullet or mark closed), `docs/COVERAGE.md` verify block.
  One commit for the harness fix alone (no model changes).
- Files: `src/cpu/tests.rs::boot`, `src/cpu/mod.rs::Cpu::new`,
  `src/system.rs::reset_globals` (already clears MPU but NOT
  MWU_ARMED — add it there too so production resets match tests),
  `src/peripherals/mwu_nrf.rs` tests (lock discipline only).
  Reference: `src/peripherals/mod.rs:122-153` (`mpu_check` /
  `mpu_is_device` borrow_mut sites = the panic location),
  `src/cpu/mem.rs::watch/mpu_deny/unaligned_deny`,
  `src/system.rs:15-47` (locks), `:68-111` (flag latches).

## 6. WORKSTREAM C — non-BLE frontiers (bounded, evidence-first)

- C1. MakeCode scroll content (pre-scroll sequencing stall, open
  since P57): `basic.showString("A")` (`mc/main.ts`, built
  `mc/built/mbcodal-binary.hex` via makecode 1.3.6, recipe plan P19)
  boots to scheduler idle, TIMER4 COUNTER 0, DIR0 sticky, 0 `0x30C04`
  hits over 300M. Settled findings (STATUS §6.4 + plan P91–P97):
  fiber-wait `0x2e4d8` entered+dispatching, no HardFault; run queue
  ONE fiber (main in `0x2e410` waiter, TCB LR `0x2e453`); sleep queue
  2; event-wait EMPTY (scroll fiber never CREATED); main inside
  `EventModel::send` listener-invoke (R0=own TCB); heap NOT empty
  (free node `@0x20003b3c` present); `0x2e99c` is not malloc; `0x35664`
  is a member-getter (`[obj+20]` slots `0x104/0x148/0x15c`, one
  `bl @0x358c8` + four `b.w` tails); `0x2e410` is a NULL-or-flag-gated
  pump entry (`bl @0x31f62` only static site; live entries via
  `blx`). Quantum-boundary sampling can't catch awake bursts (park
  pc always WFE `0x37afa`). NEXT (ranked): (b) wait-queue-OBJECT walk
  at `0x20003b18` (P92 dumped heads only — dump the waiter OBJECTS to
  name the awaited event id), then trap THAT raise site; else park
  LEFT-4 and switch to LEFT-3. Reverted native probes only; suite
  stays file-free; do NOT change init order by guessing.
- C2. Bootloader full chain (MBR→BL→SD→app; direct-app boot is the
  working recipe). Settled (STATUS §6.3): BL entry `0x772F9`, FICR
  gather, benign post-UICR reset, 2nd CODED AIRCR `0x78514` via tbb
  `0x78498` BY DESIGN (r4==0 = SD-enable SUCCESS through `0x7B530`
  `svc 16` + `0x7B5B4` IPR22 check + `0x7B568`); `0x7B5B4` needs
  nonzero IPR22 (SD-set priorities — silicon state, out of scope);
  `0x784C4` = DFU-progress gate (not the cause); MBR selector `0x417`
  never reads `0x10001200/204` (P42 refuted by `0x0–0xB00` sweep);
  seeded native MBR run parks `0x77332`. NEXT if pursued: MBR pass-2
  trace with SD priorities (shelved with sd_evt) — informational;
  direct-app boot stays the recipe. No model changes expected.
- C3. Already closed (do NOT reopen without a faulting config):
  REPL exec (P87 `print(1+2)`→`3` in-browser), TX drops (P67
  model-clean), SPIM2/3 routing (P70 browser-proven), entry-fault
  (P51 pump-evolution elapsed), FDS-lottery (elapsed). Historical
  forensics in STATUS §6 stay untouched.
- Out of scope (STATUS §7, do not build): full BLE pump beyond the
  SVC face, STM32/UNO-R4/M0+/DAPLink, Arduino-Primo boot quirks,
  lazy FPU stacking, ACL/SPU, npm publish (401).

## 7. Validation checklist (per workstream, in order)

- A (pairing fw): xpack compile clean → `cargo test
  nrf_ble_pairing_fw_markers -- --test-threads=1` green (both runs,
  all BLEP markers) → full `cargo test -- --test-threads=1` green
  (204) → `npm run build:handshake --prefix demo` + handshake 18/18
  → docs (STATUS counts, plan P104, ble_lang README row, doc.html
  bullet) → commit `.c`+`.bin`+test together.
- B (flake): filtered `$BIN mpu mwu unalign` 10/10 (both thread
  modes) → full suite 5/5 consecutive → single-thread 203/203 →
  docs (STATUS §3, doc.html checks, COVERAGE verify) → commit alone.
- C (frontiers): native reverted probes only (`git checkout --`
  after), suite file-free; browser runs via committed pkg only;
  record negative results in plan (zero-hit greps need regex check —
  P91's miss was a format mismatch, not absence).
- Final gate before push: `cargo test -- --test-threads=1` (203 or
  204), handshake 18/18, smoke green, mpy-face green, bridge E2E
  42/42, browser 16/16 zero page errors, `wasm-pack build ... demo/pkg`
  if Rust changed, `git status` shows only intended files.

## 8. Docs to update (additive, never rewrite history)

- `plan.md`: append `## 86. P104 ...` (pairing-fw proof), `## 87.
  P105 ...` (flake fix), frontier notes as P106+; keep P103 text
  intact (it already records the E2E cross-peer bug, RSSI catch,
  mock-ordering traps, script bugs).
- `STATUS.md`: §3 counts 203→204 after A lands; §5 LEFT-4/LEFT-3
  pointers after C; §8 verify block (thread flag + new test names).
- `demo/API.md`: only if exports change (none expected; pairing fw
  uses existing SVCs — if you add an export, document tag +
  take/complete + bridge message + loopback line).
- `demo/doc.html`: matrix counts + "fully done?" section (extend the
  8-gap list only if a gap genuinely closes — pairing-fw does NOT
  close crypto/store gaps); checks section thread note after B.
- `demo/about.html`, `docs/COVERAGE.md` (§8 tables + verify),
  `demo/parts/ble_lang/README.md` (+1 pairing-fw row), `docs/README.md`
  only if build flags change (they don't).
- Commit style (repo convention): `P104 BLE pairing-fw image proof
  (204 green, BLEP markers, ...)` / `P105 parallel-flake harness fix
  (lock_boot join, 5/5 green)` / frontier notes as `P106 ...`. Push
  only when the §7 gate is fully green.

## 9. Suggested skills (call the Skill tool for each)

- `frontend-design`: NOT needed (no UI changes; bench pixels frozen).
- `playwright-interactive`: only if the browser run misbehaves beyond
  the committed `tools/browser_verify_16.py` flow (preset stage →
  `#fwrun` → UART `.value` → self-test → depth 16/16). Prefer the
  script over ad-hoc driving.
- `webapp-testing`: only for live-bridge E2E triage (`ble_live_e2e.mjs`
  timeouts mean bridge down or scan-lock contention — check
  `tools/ble_air_bridge.py` logs first, not the page).
- `doc-coauthoring`: use when writing the P104–P106 plan notes so the
  numbered-phase style matches (numbered headings, NEXT lines,
  reverted-probe discipline).
- `customize-opencode`: do NOT use (no opencode config changes).

## 10. Opencode todo list (load verbatim into todowrite)

```json
[
  {"content": "A1 compile ble_pairing_fw.c with xpack GCC + objcopy to .bin + objdump SVC check", "status": "pending", "priority": "high"},
  {"content": "A2 add nrf_ble_pairing_fw_markers test (2 runs, mid-spin pump, all BLEP markers)", "status": "pending", "priority": "high"},
  {"content": "A3 static SVC rescan of MPY+MC app regions (type-02+04 records, 0x1C000-0x77000 filter)", "status": "pending", "priority": "high"},
  {"content": "A4 pairing-fw docs (STATUS counts, plan P104, ble_lang README row, doc.html bullet)", "status": "pending", "priority": "medium"},
  {"content": "A5 verify pairing-fw gate (cargo 204 single-thread + handshake + E2E) then commit .c+.bin+test", "status": "pending", "priority": "high"},
  {"content": "B1 reproduce flake deterministically ($BIN mpu mwu unalign, single-thread included)", "status": "pending", "priority": "high"},
  {"content": "B2 implement lock_boot join (boot() reset_globals/MWU+MPU clears + Cpu::new disarm)", "status": "pending", "priority": "high"},
  {"content": "B3 verify flake fix (filtered 10/10 both modes, full suite 5/5, single 203/203)", "status": "pending", "priority": "high"},
  {"content": "B4 flake docs (STATUS S3, doc.html checks, COVERAGE verify) then commit alone", "status": "pending", "priority": "medium"},
  {"content": "C1 MakeCode wait-queue-OBJECT walk at 0x20003b18 (name awaited event, trap raise site)", "status": "pending", "priority": "medium"},
  {"content": "C2 bootloader pass-2 trace assessment (SD priorities shelved?) or park LEFT-3", "status": "pending", "priority": "low"},
  {"content": "Final gate (cargo + handshake + smoke + mpy + E2E 42/42 + browser 16/16 + pkg rebuild)", "status": "pending", "priority": "high"},
  {"content": "Commit + push only when final gate fully green", "status": "pending", "priority": "medium"}
]
```

## 11. Quickstart (first 15 minutes)

- `git status -sb; git log --oneline -3` (expect `535cfcb` clean +
  untracked `blinky/ble_fw/ble_pairing_fw.c`).
- `cargo test --manifest-path nrf52833-periph-wasm/Cargo.toml -- --test-threads=1 2>&1 | tail -n 2`
  (expect 203 green; parallel default is NOT the gate).
- `BIN=$(ls -t nrf52833-periph-wasm/target/debug/deps/nrf52833_periph_wasm-* | grep -v '\.' | head -n1); $BIN --test-threads=1 unaligned_device region_watch`
  (expect the deterministic B-flake repro: 1 pass + 1 fail).
- `TC=~/.arduino15/packages/STMicroelectronics/tools/xpack-arm-none-eabi-gcc/14.2.1-1.1/bin/arm-none-eabi-; ${TC}gcc --version`
  (expect xPack 14.2.1; all firmware builds use exactly this).
- `npm run build:handshake --prefix demo >/dev/null 2>&1; node demo/parts/handshake.mjs 2>&1 | tail -n 2`
  (expect 18/18 `all handshake mocks OK`).
- Bridge E2E only after A/B land: `python3 tools/ble_air_bridge.py
  --port 18771 &` + `node demo/parts/ble_live_e2e.mjs
  ws://127.0.0.1:18771` (expect 42/42). Browser only at the end:
  serve `:8080` + `python3 tools/browser_verify_16.py` (16/16).

## 12. Traps that already burned time (read before touching)

- SVC dispatcher macros: the 4-reg CHAR_ADD block (`svc_handle, 0,
  attr, chr_h` across r0–r3) CANNOT use `SVC3` (3-reg max) — copy
  the manual register block from `ble_conformance.c:63-67` exactly.
- Mid-spin pump is load-bearing: firmware spins on evt arrival, so
  the native driver must pump every ~500-instr slice
  (`cpu/tests.rs:265-271` pattern); 20K slices starve CONNECTED
  (plan P54 lesson, re-learned every phase).
- Mock FIFO order matters: AUTH_KEY_REQUEST must sit ahead of the
  handshake's AUTH_STATUS in the queue; GapAuthenticate take must
  drain before `complete_pairing` (P103 traps, plan §85).
- `evt_buf[0]` is the u16 id LOW byte; `evt_len` must be reset to 64
  before every `evt_get`; `sec_buf` expects `0x21` post-pairing.
- Both bridge peers share handle numbers (decl 16/value 17):
  unaddressed reads cross peers (first link read 64 — P103 E2E
  caught it); always address reads (`peer:` + `peer` echo asserts).
- Uncaught HCI raise starves the WS loop and hangs every later job:
  `read_conn_rssi` catches UNKNOWN_HCI_COMMAND INSIDE (P103).
- Zero-hit greps need regex checks (P91's caller miss was a
  format mismatch); `svc 82` absence must be even-address
  `0xDF52` scan with type-02+04 handling (odd hits are data).
- Page probes: preset select STAGES, `#fwrun` boots (no Load
  button); UART box is `<textarea>` (read `.value`); pkg must be
  rebuilt from current src before any browser claim.
- Never commit `/tmp/opencode/*` probes, `tools/__pycache__/`,
  `.openchamber/` screenshots, or `nrf52833-periph-wasm/{pkg,parts,
  demo}` stray trees (P101 build-script bug — scripts fixed, stay
  vigilant). Revert native probes (`git checkout --`) before commit;
  suite must stay file-free.
- No `claude` binary exists on this host — the claude-handoff skill's
  `claude --bg --name ...` launch step cannot run here; this file IS
  the handoff. The next agent starts by reading it, not by polling
  any background job.

## 13. Appendix A — file map with line anchors (open these first)

- `nrf52833-periph-wasm/src/sd_ble.rs` (~3362 lines): header comment
  (1–120, all numbers/contracts), `BleJob` enum 16 tags (~1200s),
  `SdBle` state + `Conn` table (~500s), `handle_svc` dispatch
  (`pub fn handle_svc`, ~1047), take/complete/post fns (2175–2560:
  `take_job`, `stage_take_data`, `complete_*`, `post_*`,
  `reset_for_test` at 645), native tests module (~2598+).
- `nrf52833-periph-wasm/src/cpu/thumb.rs:1437-1451`: SVC hook
  (`0x60..=0xBF` → `handle_svc` first, fall through to `raise_sync`).
- `nrf52833-periph-wasm/src/cpu/tests.rs:19-39` (`boot()` helper —
  installs system WITHOUT flag hygiene; this is B-worksite #1),
  `:244-286` (`nrf_ble_conformance_svc_face`, 2-run loop, 500-slice
  pump), `:291-344` (`pump_ble_test_driver`, reuse for pairing fw),
  `:347+` (`nrf_ble_c_face_markers`), `:2759+` + `:3027+` (the two
  unaligned tests in the flake repro).
- `nrf52833-periph-wasm/src/cpu/mod.rs:127-138` (`Cpu::new` clears
  MPU latches — extend pattern for MWU in B2), `:440/:663/:1003`
  (`is_mpu_enabled` gates).
- `nrf52833-periph-wasm/src/cpu/mem.rs`: `watch()` (~262),
  `mpu_deny` (~252), `unaligned_deny` (~221),
  `Memory for FlatMemory` read8/write8 arms (293+).
- `nrf52833-periph-wasm/src/system.rs:15-47` (BOOT/UART/I2C locks +
  `try_lock_uart`), `:68-74` (MWU_ARMED flag), `:78-111` (MPU /
  priv / UNALIGN_TRP latches), `:425-455` (`reset_globals` — clears
  MPU but NOT MWU_ARMED; B2 adds it).
- `nrf52833-periph-wasm/src/lib.rs`: `SYS AtomicPtr` + `sys()` /
  `try_sys()` / `set_sys` / `init_for_test` (top ~45 lines), 40
  `ble_*` exports (`grep -n "pub fn ble_"` → 33 fns), `reset_state`.
- `nrf52833-periph-wasm/src/peripherals/mod.rs:122-153`
  (`mpu_check` / `mpu_is_device`, `borrow_mut` at :131/:146 = panic
  site), `:344+` (`new_wasm` map incl. MPU slot
  `0xE000_ED90–0xE000_EDFC`), `:449+` (`read`/`write` dispatch).
- `nrf52833-periph-wasm/src/peripherals/mwu_nrf.rs`: model + both
  tests (`region_watch_read_write_and_irq` ~:251,
  `pregion_subs_include_exclude` ~:288; both hold `lock_boot()`).
- Firmware: `blinky/ble_fw/ble_conformance.c` (19 markers),
  `blinky/ble_fw/ble_pairing_fw.c` (NEW, untracked, `BLEP:` markers),
  `blinky/ble_fw/ble_gatt_fw.c` (GATTS-only), `blinky/link_c_nrf.ld`,
  `demo/parts/ble_lang/c_ble_face.c` + `mpy_ble_face.py` +
  `run_mpy_face.mjs` + `README.md` (language table).
- Bridge/pump/E2E: `tools/ble_air_bridge.py` (docstring protocol
  :1–110, peers, locks, `read_conn_rssi`),
  `demo/parts/ble_air.js` (BleAir, `sendBle` peer forward),
  `demo/index.html:pumpDma()`, `demo/parts/ble_live_e2e.mjs` (42
  checks), `demo/parts/mocks.js` (MockBleSvc 18/18),
  `demo/parts/handshake.mjs`, `demo/parts/smoke.mjs`,
  `tools/browser_verify_16.py` (16/16 script).
- Docs: `plan.md` (P103 at tail, append P104+), `STATUS.md` (§3
  counts/flake, §5 LEFT, §8 verify), `docs/COVERAGE.md` (§8 BLE
  tables), `demo/API.md` (frozen v1, BLE section),
  `demo/doc.html` ("fully done?" + matrices + checks),
  `demo/about.html` (scope), `demo/package.json` (scripts),
  `docs/README.md` (GCC flags), `docs/cpu_bug.md`, `docs/sd_evt_design.md`.

## 14. Appendix B — command cheat sheet (copy-paste)

- Suite (deterministic gate): `cargo test --manifest-path
  nrf52833-periph-wasm/Cargo.toml -- --test-threads=1 2>&1 | tail -n 2`
- Filtered flake repro: `BIN=$(ls -t
  nrf52833-periph-wasm/target/debug/deps/nrf52833_periph_wasm-* |
  grep -v '\.' | head -n1); $BIN --test-threads=1 unaligned_device
  region_watch` (deterministic fail pre-fix) and `$BIN mpu mwu
  unalign` (parallel flake probe).
- Test count: `cargo test ... -- --list 2>/dev/null | grep -c ": test"`.
- BLE exports: `grep -c "pub fn ble_" nrf52833-periph-wasm/src/lib.rs`.
- Firmware build: `TC=~/.arduino15/packages/STMicroelectronics/tools/
  xpack-arm-none-eabi-gcc/14.2.1-1.1/bin/arm-none-eabi-;
  ${TC}gcc -mcpu=cortex-m4 -mthumb -mfloat-abi=hard -mfpu=fpv4-sp-d16
  -O2 -nostdlib -ffreestanding -T blinky/link_c_nrf.ld -o /tmp/x.elf
  blinky/ble_fw/<name>.c && ${TC}objcopy -O binary /tmp/x.elf
  blinky/ble_fw/<name>.bin && ${TC}objdump -d /tmp/x.elf | grep svc`.
- Single BLE test: `cargo test --manifest-path
  nrf52833-periph-wasm/Cargo.toml nrf_ble_conformance_svc_face -- --test-threads=1`.
- JS suites: `npm run build:handshake --prefix demo;
  node demo/parts/handshake.mjs | tail -n 2` (18/18);
  `npm run test:parts --prefix demo | tail -n 2`;
  `npm run test:mpy --prefix demo | tail -n 2`.
- Bridge E2E: `python3 tools/ble_air_bridge.py --port 18771 & sleep 12;
  node demo/parts/ble_live_e2e.mjs ws://127.0.0.1:18771 | tail -n 3`
  (42 ok, 0 FAIL); `kill %1` after.
- Browser: `python3 -m http.server 8080 --directory demo & sleep 3;
  python3 tools/browser_verify_16.py` (boot + self-test + 16/16).
- Pkg rebuild (after Rust changes): `npm run build:wasm --prefix demo`
  (committed `demo/pkg/`) and `npm run build:handshake --prefix demo`.
- Git hygiene: `git status --short; git log --oneline -3`;
  `git status -sb` must show `## master...origin/master` after push.
  Never commit `/tmp/opencode/*`, `tools/__pycache__/`,
  `.openchamber/`, stray `nrf52833-periph-wasm/{pkg,parts,demo}`.

## 15. Appendix C — IDs and markers glossary (no re-derivation)

- SVC bases: common `0x60`–`0x69` (ENABLE/EVT_GET/TX_PKT_COUNT/
  UUID_VS_ADD/DECODE/ENCODE/VERSION/USER_MEM_REPLY/OPT_SET/OPT_GET),
  GAP `0x70`–`0x8E`, GATTC `0x90`–`0x99`, GATTS `0xA0`–`0xAC`,
  L2CAP `0xB0`–`0xB2`. Hook range `0x60..=0xBF`.
- Key GAP SVCs: AUTHENTICATE `0x7E`, SEC_PARAMS_REPLY `0x7F`,
  AUTH_KEY_REPLY `0x80`, LESC_DHKEY `0x81`, KEYPRESS `0x82`,
  OOB_GET `0x83`, OOB_SET `0x84`, ENCRYPT `0x85`, SEC_INFO_REPLY
  `0x86`, CONN_SEC_GET `0x87`, SCAN_START `0x8A`, CONNECT `0x8C`,
  RSSI_GET `0x8E`. Key GATTC: PRIM `0x90`, READ `0x96`,
  VALS_READ `0x97`, WRITE `0x98`. Key GATTS: SVC_ADD `0xA0`,
  CHAR_ADD `0xA2`, HVX `0xA6`.
- Request events: SEC_PARAMS_REQUEST `0x13`, SEC_INFO_REQUEST `0x14`,
  PASSKEY_DISPLAY `0x15`, KEY_PRESSED `0x16`, AUTH_KEY_REQUEST
  `0x17`, LESC_DHKEY_REQUEST `0x18`, AUTH_STATUS `0x19`,
  CONN_SEC_UPDATE `0x1A`, RSSI_CHANGED `0x1C`, ADV_REPORT `0x1D`,
  CONNECTED `0x10`, DISCONNECTED `0x11`.
- SEC_STATUS: `0x00` success, `0x81` passkey-fail, `0x82`
  OOB-missing, `0x83` auth-req, `0x84` confirm, `0x85`
  pairing-not-supp (NOT `0x29` — ATT error, fixed P103).
- Markers: conformance `BLE:*` (19 incl. `BLE:ALL-OK`), C face `C:*`
  (`C:ALL-OK`), pairing fw `BLEP:*` (`BLEP:ALL-OK`), blinky
  `BOOT/BLINK`, self-test `pass: ... pairing×2 ...`, depth
  `16/16 probes pass`, E2E `all live-bridge E2E checks OVER AIR OK`,
  handshake `all handshake mocks OK`, MPY face `mpy BLE face OK`.
- Bridge peers: default battery 87 `PeerBatt`, HR twin 64 `PeerHR`;
  shared handles decl 16/value 17; `peer:[6]` + `peer` echo prevent
  cross-reads; RSSI `-50` const with `src` tag; HCI reason 19
  (REMOTE_USER_TERM) on disconnects.

(End of handover — 600 lines. Next action: load §10 into todowrite, start A1.)
