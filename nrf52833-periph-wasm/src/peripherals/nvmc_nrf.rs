use crate::system::System;
use super::Peripheral;

/// NVMC @ 0x4001E000 (flash controller) + ACL @ 0x4001E000 (access
/// control lists, shared base — one combined slot, same precedent as
/// CLOCK+POWER and the P0/P1 GPIO block).
///
/// NVMC: READY 0x400 (always 1), READYNEXT 0x408 (always 1), CONFIG
/// 0x504 (0=REN read-only, 1=WEN write enable, 2=EEN erase enable),
/// ERASEPAGE 0x508, ERASEALL 0x50C, ERASEUICR 0x514.
/// Erase/write staging (driver owns the data path, like EASYDMA): with
/// CONFIG=EEN, an ERASEPAGE write stages take_erase() (page base); the
/// driver applies 0xFF to guest memory and calls complete_erase().
/// With CONFIG=WEN, flash-region writes via the NVMC window are accepted
/// by the memory layer (mem.rs flash protection stays: guest stores to
/// flash are still ignored natively; the JS driver applies program words
/// through mem_write, exactly like the original driver flow from the
/// early bring-up).
///
/// ACL (SVD ground truth: cluster dim 8, stride 0x10, base 0x800):
/// region n: ADDR 0x800+16n / SIZE 0x804+16n / PERM 0x808+16n.
/// PERM bit 1 = WRITE-disable, bit 2 = READ-disable (0 = allow, reset).
/// Sticky semantics per the SVD ("write '0' has no effect"): SIZE
/// ignores 0-writes, PERM can only set bits (OR), ADDR is plain RW;
/// only a reset clears a region. Write-protection is ENFORCED here:
/// ERASEPAGE/ERASEALL refuse to stage when the target overlaps a
/// write-protected region (take_erase stays None — the MBR `sd_mbr_
/// command` flash-protect use-case from the S140 headers).
/// Read-disable is STORED, queryable (`acl_read_blocked_at`), AND
/// enforced in the memory layer: every ACL PERM write republishes the
/// MWU-patterned `acl_armed` gate, and mem.rs consults it per access
/// (no src/cpu/ edits — the layer already calls out for MWU; ACL rides
/// the same hook). No SPU exists on nRF52833 (70 SVD peripherals, none
/// named SPU — SPU is an nRF53/nRF91 part); "ACL/SPU" rows mean ACL.
pub struct Nvmc {
    pub config: u32,
    erase_pending: Option<u32>,
    acl_addr: [u32; 8],
    acl_size: [u32; 8],
    acl_perm: [u32; 8],
}

impl Default for Nvmc {
    fn default() -> Self {
        Self { config: 0, erase_pending: None,
               acl_addr: [0; 8], acl_size: [0; 8], acl_perm: [0; 8] }
    }
}

impl Nvmc {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "NVMC" || name == "ACL" { Some(Box::new(Self::default())) } else { None }
    }
    /// Republish the MWU-patterned read gate after any ACL triple
    /// write (called with &mut self from the write arm — no model
    /// re-borrow, no RefCell risk).
    fn publish_acl_armed(&self) {
        let armed = (0..8).any(|n| self.acl_size[n] != 0 && self.acl_perm[n] & (1 << 2) != 0);
        crate::system::acl_set_armed(armed);
    }
    fn acl_write_blocked(&self, base: u32, erase_len: u32) -> bool {
        for n in 0..8 {
            if self.acl_size[n] == 0 || self.acl_perm[n] & (1 << 1) == 0 {
                continue;
            }
            let (rs, re) = (self.acl_addr[n], self.acl_addr[n].wrapping_add(self.acl_size[n]));
            let (es, ee) = (base, base.wrapping_add(erase_len));
            if es < re && rs < ee {
                return true;
            }
        }
        false
    }
    /// True when `addr` sits in a read-protected ACL region (PERM bit
    /// 2, SIZE nonzero). Same predicate as `acl_read_blocked_at`, but
    /// on &self (no model re-borrow) for the memory-layer hook.
    fn read_blocked(&self, addr: u32) -> bool {
        (0..8).any(|n| {
            self.acl_size[n] != 0
                && self.acl_perm[n] & (1 << 2) != 0
                && addr.wrapping_sub(self.acl_addr[n]) < self.acl_size[n]
        })
    }
}

impl Peripheral for Nvmc {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x400 => 1, // READY
            0x408 => 1, // READYNEXT
            0x504 => self.config,
            0x508 => 0, // ERASEPAGE write-only
            0x800..=0x87F => {
                // ACL cluster (dim 8, stride 0x10): ADDR +0, SIZE +4, PERM +8.
                let n = ((offset - 0x800) >> 4) as usize;
                match (offset - 0x800) & 0xF {
                    0x0 if n < 8 => self.acl_addr[n],
                    0x4 if n < 8 => self.acl_size[n],
                    0x8 if n < 8 => self.acl_perm[n],
                    _ => 0,
                }
            }
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        match offset {
            0x504 => self.config = value & 0x3,
            0x508 => {
                // ERASEPAGE is only meaningful with CONFIG=EEN, and never
                // stages into a write-protected ACL region.
                if self.config == 2 {
                    let base = value & !0xFFF;
                    if !self.acl_write_blocked(base, 4096) {
                        self.erase_pending = Some(base);
                    }
                }
            }
            0x50C => {
                // ERASEALL with EEN: stage the whole flash (driver loops).
                // A protected region anywhere blocks the whole erase
                // (silicon refuses rather than partially erasing).
                if self.config == 2 && !self.acl_write_blocked(0, 0x80000) {
                    self.erase_pending = Some(0xFFFF_FFFF);
                }
            }
            0x800..=0x87F => {
                let n = ((offset - 0x800) >> 4) as usize;
                if n >= 8 {
                    return;
                }
                match (offset - 0x800) & 0xF {
                    0x0 => self.acl_addr[n] = value,
                    // SIZE: write '0' has no effect (sticky once set).
                    0x4 => if value != 0 { self.acl_size[n] = value; },
                    // PERM: bits are sticky-set (write '0' has no effect):
                    // bit 1 WRITE-disable, bit 2 READ-disable.
                    0x8 => self.acl_perm[n] |= value & 0x6,
                    _ => {}
                }
                // Republish the MWU-patterned read gate (any region with
                // READ-disable + nonzero SIZE arms it; clearing needs a
                // reset since PERM bits are sticky-set).
                self.publish_acl_armed();
            }
            _ => {}
        }
    }
}

/// Borrow-safe NVMC accessor: tries a non-blocking borrow first, so a
/// re-entrant call from inside the model's own write arm (e.g. a query
/// issued while the slot is already borrowed) degrades to None instead
/// of panicking. The write arm calls `publish_acl_armed` directly on
/// &mut self and never needs this path; only cross-cutting callers
/// (take/complete/queries) do.
fn with_nvmc<R>(sys: &System, f: impl FnOnce(&mut Nvmc) -> R) -> Option<R> {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4001_E000 {
            let b = match slot.peripheral.try_borrow_mut() {
                Ok(b) => b,
                Err(_) => return None,
            };
            let mut b = b;
            if let Some(n) = b.as_any_mut().downcast_mut::<Nvmc>() {
                return Some(f(n));
            }
            return None;
        }
    }
    None
}

/// Take a staged erase (page base, or 0xFFFF_FFFF for ERASEALL).
pub fn take_erase(sys: &System) -> Option<u32> {
    with_nvmc(sys, |n| n.erase_pending.take()).flatten()
}

/// Complete an erase: clears the staged request (driver applied 0xFF).
/// Posts the SoC flash-success event (sd_evt phase 1) when the SD is
/// enabled — same take→complete discipline as every DMA pump: post on
/// completion, never on staging.
pub fn complete_erase(sys: &System) {
    with_nvmc(sys, |n| n.erase_pending = None);
    crate::sd_evt::post_flash_op(true);
}

/// Stored ACL state for one region (ADDR/SIZE/PERM triple).
/// Read path for the driver/JS layer + the read-block query below.
pub fn acl_region(sys: &System, n: usize) -> Option<(u32, u32, u32)> {
    if n >= 8 {
        return None;
    }
    with_nvmc(sys, |nv| (nv.acl_addr[n], nv.acl_size[n], nv.acl_perm[n]))
}

/// True when `addr` sits in a read-protected ACL region (PERM bit 2,
/// SIZE nonzero). Enforcement hook for the memory layer (mem.rs gates
/// on the MWU-patterned `acl_armed` flag and calls `acl_read_deny`
/// below — no src/cpu/ edits, same hook as MWU).
pub fn acl_read_blocked_at(sys: &System, addr: u32) -> bool {
    with_nvmc(sys, |nv| {
        (0..8).any(|n| {
            nv.acl_size[n] != 0
                && nv.acl_perm[n] & (1 << 2) != 0
                && addr.wrapping_sub(nv.acl_addr[n]) < nv.acl_size[n]
        })
    })
    .unwrap_or(false)
}

/// Memory-layer deny check: true when a CPU read at `addr` hits a
/// read-protected ACL region. Called from mem.rs only while
/// `acl_armed()` (one atomic when disarmed, zero cost). Borrow-safe:
/// single with_nvmc pass, no mem access inside (same discipline as
/// mwu_note — never re-enters the memory layer).
pub fn acl_read_deny(sys: &System, addr: u32) -> bool {
    with_nvmc(sys, |n| n.read_blocked(addr)).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::mem::Memory;
    use crate::system::test_dummy_system;
    #[test]
    fn ready_and_config() {
        let sys = test_dummy_system();
        let mut n = Nvmc::default();
        assert_eq!(n.read(&sys, 0x400), 1);
        n.write(&sys, 0x504, 1);
        assert_eq!(n.read(&sys, 0x504), 1);
    }
    #[test]
    fn erase_stages_only_with_een() {
        use crate::system::test_dummy_system;
        let sys = test_dummy_system();
        // REN (0): erase ignored.
        sys.p.write(&sys, 0x4001E508, 4, 0x0007E000);
        assert!(take_erase(&sys).is_none());
        // EEN (2): erase stages the 4KB-aligned page base.
        sys.p.write(&sys, 0x4001E504, 4, 2);
        sys.p.write(&sys, 0x4001E508, 4, 0x0007E123);
        assert_eq!(take_erase(&sys), Some(0x0007E000));
        assert!(take_erase(&sys).is_none(), "staged once only");
        // Second run: fresh instance, no leak.
        let sys2 = test_dummy_system();
        assert!(take_erase(&sys2).is_none());
    }
    #[test]
    fn acl_regions_sticky_and_block_erase() {
        use crate::system::test_dummy_system;
        // mem hooks notify the INSTALLED system: install one under the
        // boot lock (same discipline as the MWU watch test — the hook
        // reads try_sys, and a dummy system is invisible to it).
        let _g = crate::system::lock_boot();
        let sys0 = crate::system::WasmSystem::new();
        crate::init_for_test(sys0);
        let sys = crate::sys();
        // Reset state: no protection, region triples read 0.
        assert_eq!(acl_region(&sys, 0), Some((0, 0, 0)));
        assert!(!acl_read_blocked_at(&sys, 0x0007_4000));
        assert!(!crate::system::acl_armed(), "gate disarmed at reset");
        sys.p.write(&sys, 0x4001E504, 4, 2); // CONFIG=EEN
        // Program region 0: [0x74000, 0x75000), write-protected.
        sys.p.write(&sys, 0x4001E800, 4, 0x0007_4000); // ACL[0].ADDR
        sys.p.write(&sys, 0x4001E804, 4, 0x1000);      // ACL[0].SIZE
        sys.p.write(&sys, 0x4001E808, 4, 1 << 1);      // ACL[0].PERM WRITE-disable
        assert_eq!(acl_region(&sys, 0), Some((0x0007_4000, 0x1000, 0x2)));
        // Sticky: SIZE 0-write ignored, PERM ORs, ADDR plain RW.
        sys.p.write(&sys, 0x4001E804, 4, 0);
        sys.p.write(&sys, 0x4001E808, 4, 0);
        assert_eq!(acl_region(&sys, 0), Some((0x0007_4000, 0x1000, 0x2)));
        sys.p.write(&sys, 0x4001E808, 4, 1 << 2); // add READ-disable
        assert_eq!(acl_region(&sys, 0), Some((0x0007_4000, 0x1000, 0x6)));
        assert!(acl_read_blocked_at(&sys, 0x0007_4123), "inside: read-blocked");
        assert!(!acl_read_blocked_at(&sys, 0x0007_5000), "outside: open");
        // Enforcement: erase into the protected page refuses to stage.
        sys.p.write(&sys, 0x4001E508, 4, 0x0007_4000);
        assert!(take_erase(&sys).is_none(), "protected page: no stage");
        // Neighbor page still stages.
        sys.p.write(&sys, 0x4001E508, 4, 0x0007_5000);
        assert_eq!(take_erase(&sys), Some(0x0007_5000));
        // ERASEALL with any protected region: refused wholesale.
        sys.p.write(&sys, 0x4001E50C, 4, 1);
        assert!(take_erase(&sys).is_none(), "eraseall blocked by region 0");
        // Region 7 (last cluster slot) routes independently.
        sys.p.write(&sys, 0x4001E870, 4, 0x0001_0000);
        sys.p.write(&sys, 0x4001E874, 4, 0x1000);
        assert_eq!(acl_region(&sys, 7), Some((0x0001_0000, 0x1000, 0)));
        // Read-gate enforcement (mem-layer hook, MWU pattern): a CPU
        // read inside the READ-disabled region faults + returns 0,
        // outside reads clean. Gate publishes on the PERM write above.
        assert!(crate::system::acl_armed(), "gate armed by READ-disable");
        let mut mem = crate::cpu::mem::FlatMemory::new(512 * 1024, 128 * 1024);
        // NOTE: mem.write8 goes through the Flash arm (guest stores to
        // flash are ignored — real flash needs erase/program), so seed
        // via load() (bypasses protection, like firmware flashing).
        mem.load(&[0xAA], 0x0007_4123);
        mem.load(&[0xBB], 0x0007_5000);
        crate::system::take_bus_fault();
        assert_eq!(mem.read8(0x0007_4123), 0, "blocked read returns 0");
        assert_eq!(crate::system::take_bus_fault(), Some((0x0007_4123, false)), "blocked read pends bus fault");
        crate::system::take_bus_fault();
        assert_eq!(mem.read8(0x0007_5000), 0xBB, "outside reads clean");
        assert_eq!(crate::system::take_bus_fault(), None, "no fault outside");
        // 2nd run: fresh instance, no leak.
        crate::system::reset_globals();
        let sys2 = test_dummy_system();
        assert_eq!(acl_region(&sys2, 0), Some((0, 0, 0)));
        assert!(take_erase(&sys2).is_none());
    }
}
