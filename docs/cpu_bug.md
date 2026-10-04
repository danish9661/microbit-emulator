# cpu_bug.md — core decoder bugs found via the nRF52833 port

The `src/cpu/` decoder is shared/audited. Board work must never silently
work around a suspected core bug here: record it (claim, repro, exact
pc/opcode), fix in the core, add a native regression test.

## 1. TST dispatched as CMP (FIXED 2026-09-11)

- Claim: ALU `sop 8` (`0100001000`, GAS `tst r0,r1 = 0x4208`) executed
  `sub_flags` (CMP) instead of `AND`-flags (TST).
- Repro: `tst r0, r1` with `r0 == r1 == 0x200000` set Z (0x200000-0x200000
  == 0); correct TST clears Z (0x200000 & 0x200000 != 0). The next `beq`
  took the wrong branch.
- Found by: `blinky/air_nrf.s` PPI check (`tst r0, r1` / `beq ppi_bad`)
  printed `PPI:BAD` with GPIO OUT verifiably set. GAS ground truth
  (`docs/README.md` flags): `tst r0,r1 = 0x4208`, `cmp r0,r1 = 0x4288`.
- Fix: `src/cpu/thumb.rs` ALU arm 8 → `nz(cpu, a & b)` (no writeback).
- Regression: `cpu::tests::tst_sets_flags_without_writeback`.
- Why old tests stayed green: no prior test used TST with equal-nonzero
  operands (the only case where TST and CMP flag results differ... more
  precisely where `(a&b)==0 != (a==b)`).

## 2. Stacked return PC leaked the Thumb bit (FIXED 2026-09-11)

- Claim: `take_exception` stacked raw `r[15]` (which always carries
  `|1` internally), so every stacked return address was odd.
- Repro: MicroPython bootloader's MBR SVC dispatcher reads
  `[stackedPC-2]` as a byte to recover the SVC number. For
  `svc 24` (`0xDF18`) at `0x7A278` it needs stacked `0x7A27A`
  (`[0x7A278]=0x18`); we stacked `0x7A27B`, it read `[0x7A279]`
  (`0xDF`=223), took the unknown-SVC path, the command failed with
  `NRF_ERROR_SVC_HANDLER_MISSING`, and the bootloader reset-looped
  forever (MBR -> BL -> failed `sd_mbr_command` -> reset).
- Found by: tracing the bootloader reset loop to `svc24-ret r0=1`
  with command struct `[2,0,0,0,0,0x7A125]`.
- Fix: `src/cpu/mod.rs` stacks `r[15] & !1` (Thumb travels in
  stacked xPSR.T; silicon stacks the aligned return address).
- Regression: `cpu::tests::exception_svc_stacks_even_return_pc`
  (verified to fail without the fix: `0x103` vs `0x102`).

## 4. Predicated T1 data-processing clobbered IT-block flags (FIXED 2026-10-04)

- Claim: 16-bit Thumb data-processing executed inside an IT block updated
  N/Z/C/V, so the NEXT slot's condition tested the clobbered flags instead
  of the IT-setting instruction's. Silicon (and Unicorn, and GCC's own
  emission) preserves flags for predicated T1: only the pure flag-setters
  CMP/CMN/TST set when executed predicated.
- Repro (live MicroPython v2.1.2 REPL): `print(machine.mem32[0])` raised
  `TypeError: 'int' object isn't callable` while `print(m[0])` returned
  `536871936`. The compiler's `ittee ne; addne; movne; lsreq r7,r7,#8;
  addeq r6,#8` (`2b8e bf19 f104 060c 2701 0a3f 3608`, GAS-verified) walks
  the trailers array: `cmp r3,#142` equal sets Z=1, both NE slots skip,
  `lsreq` runs (r7 `0x28e`->`2`) and — old code — cleared Z, so `addeq`
  skipped and r6 stayed 8 low (struct base, not `nodes[0]`). The loop then
  compiled the kind word `0x28e` (TOKEN) as a node, called the wrong
  emitter (`kind+67`), and wrote bytecode `0x6B` (reserved
  `BASE_BYTE_E+0x0B`) where `0x55` (`LOAD_SUBSCR`) belongs; the VM
  faithfully pushed small-int `-21`, corrupting the call stack.
- Found by: R1-provenance audit (`emit_write_bytecode_byte` writer PC
  `0x36e82` with R2=`0x6B`: correct `movs r2,#85` emitter at `0x3726e`
  bypassed), single-step register trace to the `ittee` slots, then
  XPSR capture per slot (`lsreq` executed with Z=1 in, Z=0 out).
- Fix: `src/cpu/thumb.rs` `exec16` guards every T1 implicit-flag-setting
  path with `if !cpu.it_pred` (LSL/LSR/ASR-imm, ADD/SUB-imm3,
  ADDS/SUBS-imm8, ALU arms 0-7/9/12-15 incl. ADC/SBC/RSB; MOVS/ADD-reg/
  SUB-reg already had it). Pure tests CMP-imm8/reg, high-reg CMP, TST,
  CMN still set unconditionally. 32-bit (T2, explicit S) untouched.
- Regression: `cpu::tests::it_pred_shift_preserves` (exact firmware
  halfword sequence; asserts r7=`2`, r6 adjusted `+8`, Z/C preserved).
- Why old tests stayed green: no prior test put a flag-setting T1 inside
  an IT block ahead of a flag-sensitive later slot (the MOVS/ADD-reg
  guards covered only their own idioms).
