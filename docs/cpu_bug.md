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
