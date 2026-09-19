use crate::system::System;
use super::Peripheral;

/// MWU @ 0x40020000 (IRQ 32, memory watch unit). Offsets from
/// nrf52833.svd (note: REGIONEN is 0x510, not 0x500): EVENTS_REGION[n]
/// .WA 0x100+8n / .RA 0x104+8n (n=0..3), EVENTS_PREGION[n].WA 0x160+8n /
/// .RA 0x164+8n (n=0,1), INTEN 0x300, INTENSET 0x304 (REGIONnWA 2n /
/// REGIONnRA 2n+1, PREGIONnWA 24+2n / PREGIONnRA 25+2n) / CLR 0x308,
/// NMIEN 0x320 (+SET 0x324 / CLR 0x328), PERREGION[n].SUBSTATWA 0x400+8n
/// / SUBSTATRA 0x404+8n (write-1-clear), REGIONEN 0x510 (+SET 0x514 /
/// CLR 0x518), REGION[n].START 0x600+16n / END 0x604+16n (n=0..3),
/// PREGION[n].START 0x6C0+16n / END 0x6C4+16n / SUBS 0x6C8+16n (n=0,1).
///
/// Protocol: REGIONEN bit n arms REGION[n]; a PREGION is armed when its
/// SUBS mask is nonzero. Address match is START-inclusive,
/// END-exclusive, subdivided into 32 equal subregions for PREGIONs
/// (SUBS bit k = 1 watches subregion k; 0 excludes it). A matching CPU
/// data access (the memory layer calls `mwu_note` for every access
/// while any watch is armed -- a single atomic load when disarmed)
/// sets the WA/RA event, records the subregion bit in PERREGION
/// SUBSTAT, and fires the instance IRQ when the INTEN bit is set.
/// NMIEN-gated events also surface via IRQ (documented deviation:
/// silicon raises NMI instead, which has no delivery path here).
pub struct MwuNrf {
    intenset: u32,
    nmien: u32,
    regionen: u32,
    region_start: [u32; 4],
    region_end: [u32; 4],
    ev_region_wa: [bool; 4],
    ev_region_ra: [bool; 4],
    ev_pregion_wa: [bool; 2],
    ev_pregion_ra: [bool; 2],
    pregion_start: [u32; 2],
    pregion_end: [u32; 2],
    pregion_subs: [u32; 2],
    substat_wa: [u32; 2],
    substat_ra: [u32; 2],
}

impl Default for MwuNrf {
    fn default() -> Self {
        Self {
            intenset: 0, nmien: 0, regionen: 0,
            region_start: [0; 4], region_end: [0; 4],
            ev_region_wa: [false; 4], ev_region_ra: [false; 4],
            ev_pregion_wa: [false; 2], ev_pregion_ra: [false; 2],
            pregion_start: [0; 2], pregion_end: [0; 2],
            pregion_subs: [0; 2], substat_wa: [0; 2], substat_ra: [0; 2],
        }
    }
}

impl MwuNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "MWU" { Some(Box::new(Self::default())) } else { None }
    }
    fn fire(&self, sys: &System, bit: u32) {
        if self.intenset & bit != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(32);
        }
    }
    fn armed(&self) -> bool {
        self.regionen & 0x0F != 0
            || self.pregion_subs[0] != 0
            || self.pregion_subs[1] != 0
    }
    fn publish_armed(&self) {
        crate::system::mwu_set_armed(self.armed());
    }
    /// Check one data access against all armed watches. Called from the
    /// memory layer (no mem access inside -- borrow-safe).
    pub(crate) fn check(&mut self, sys: &System, addr: u32, is_write: bool) {
        for n in 0..4 {
            if self.regionen & (1 << n) == 0 {
                continue;
            }
            if addr >= self.region_start[n] && addr < self.region_end[n] {
                if is_write {
                    self.ev_region_wa[n] = true;
                    self.fire(sys, 1 << (2 * n));
                } else {
                    self.ev_region_ra[n] = true;
                    self.fire(sys, 1 << (2 * n + 1));
                }
            }
        }
        for n in 0..2 {
            let subs = self.pregion_subs[n];
            if subs == 0 {
                continue;
            }
            let (start, end) = (self.pregion_start[n], self.pregion_end[n]);
            if end <= start || addr < start || addr >= end {
                continue;
            }
            let span = end.wrapping_sub(start);
            // Subregion index, saturating the top edge into sub 31.
            let mut sub = ((addr.wrapping_sub(start) as u64 * 32) / span as u64) as u32;
            if sub > 31 {
                sub = 31;
            }
            if subs & (1 << sub) == 0 {
                continue; // excluded subregion
            }
            if is_write {
                self.ev_pregion_wa[n] = true;
                self.substat_wa[n] |= 1 << sub;
                self.fire(sys, 1 << (24 + 2 * n));
            } else {
                self.ev_pregion_ra[n] = true;
                self.substat_ra[n] |= 1 << sub;
                self.fire(sys, 1 << (25 + 2 * n));
            }
        }
    }
    fn clear_ev(&mut self, offset: u32) {
        match offset {
            0x100 => self.ev_region_wa[0] = false,
            0x104 => self.ev_region_ra[0] = false,
            0x108 => self.ev_region_wa[1] = false,
            0x10C => self.ev_region_ra[1] = false,
            0x110 => self.ev_region_wa[2] = false,
            0x114 => self.ev_region_ra[2] = false,
            0x118 => self.ev_region_wa[3] = false,
            0x11C => self.ev_region_ra[3] = false,
            0x160 => self.ev_pregion_wa[0] = false,
            0x164 => self.ev_pregion_ra[0] = false,
            0x168 => self.ev_pregion_wa[1] = false,
            0x16C => self.ev_pregion_ra[1] = false,
            _ => {}
        }
    }
}

impl Peripheral for MwuNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x100 => self.ev_region_wa[0] as u32,
            0x104 => self.ev_region_ra[0] as u32,
            0x108 => self.ev_region_wa[1] as u32,
            0x10C => self.ev_region_ra[1] as u32,
            0x110 => self.ev_region_wa[2] as u32,
            0x114 => self.ev_region_ra[2] as u32,
            0x118 => self.ev_region_wa[3] as u32,
            0x11C => self.ev_region_ra[3] as u32,
            0x160 => self.ev_pregion_wa[0] as u32,
            0x164 => self.ev_pregion_ra[0] as u32,
            0x168 => self.ev_pregion_wa[1] as u32,
            0x16C => self.ev_pregion_ra[1] as u32,
            0x300 => self.intenset,
            0x320 => self.nmien,
            0x400 => self.substat_wa[0],
            0x404 => self.substat_ra[0],
            0x408 => self.substat_wa[1],
            0x40C => self.substat_ra[1],
            0x510 => self.regionen,
            0x600 | 0x610 | 0x620 | 0x630 => {
                self.region_start[((offset - 0x600) >> 4) as usize]
            }
            0x604 | 0x614 | 0x624 | 0x634 => {
                self.region_end[((offset - 0x600) >> 4) as usize]
            }
            0x6C0 | 0x6D0 => self.pregion_start[((offset - 0x6C0) >> 4) as usize],
            0x6C4 | 0x6D4 => self.pregion_end[((offset - 0x6C0) >> 4) as usize],
            0x6C8 | 0x6D8 => self.pregion_subs[((offset - 0x6C0) >> 4) as usize],
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        match offset {
            0x100 | 0x104 | 0x108 | 0x10C | 0x110 | 0x114 | 0x118 | 0x11C |
            0x160 | 0x164 | 0x168 | 0x16C => {
                if value == 0 {
                    self.clear_ev(offset);
                }
            }
            0x300 => self.intenset = value,
            0x304 => self.intenset |= value,
            0x308 => self.intenset &= !value,
            0x320 => self.nmien = value,
            0x324 => self.nmien |= value,
            0x328 => self.nmien &= !value,
            0x400 | 0x408 => {
                // SUBSTATWA write-1-clears.
                let n = ((offset - 0x400) >> 3) as usize;
                if n < 2 {
                    self.substat_wa[n] &= !value;
                }
            }
            0x404 | 0x40C => {
                let n = ((offset - 0x404) >> 3) as usize;
                if n < 2 {
                    self.substat_ra[n] &= !value;
                }
            }
            0x510 => {
                self.regionen = value & 0x0F;
                self.publish_armed();
            }
            0x514 => {
                self.regionen |= value & 0x0F;
                self.publish_armed();
            }
            0x518 => {
                self.regionen &= !value;
                self.publish_armed();
            }
            0x600 | 0x610 | 0x620 | 0x630 => {
                self.region_start[((offset - 0x600) >> 4) as usize] = value;
            }
            0x604 | 0x614 | 0x624 | 0x634 => {
                self.region_end[((offset - 0x600) >> 4) as usize] = value;
            }
            0x6C0 | 0x6D0 => {
                self.pregion_start[((offset - 0x6C0) >> 4) as usize] = value;
                self.publish_armed();
            }
            0x6C4 | 0x6D4 => {
                self.pregion_end[((offset - 0x6C0) >> 4) as usize] = value;
                self.publish_armed();
            }
            0x6C8 | 0x6D8 => {
                self.pregion_subs[((offset - 0x6C0) >> 4) as usize] = value;
                self.publish_armed();
            }
            _ => {}
        }
    }
}

/// Note one CPU data access for MWU watches. No-op unless armed.
pub fn mwu_note(sys: &System, addr: u32, is_write: bool) {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4002_0000 {
            let mut b = slot.peripheral.borrow_mut();
            if let Some(m) = b.as_any_mut().downcast_mut::<MwuNrf>() {
                m.check(sys, addr, is_write);
            }
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    use crate::cpu::mem::Memory;
    #[test]
    fn region_watch_read_write_and_irq() {
        // mem hooks notify the INSTALLED system: install one under the
        // boot lock (serializes vs cpu tests that swap the global).
        let _g = crate::system::lock_boot();
        let sys0 = crate::system::WasmSystem::new();
        crate::init_for_test(sys0);
        let sys = crate::sys();
        // NVIC ISER word 1 (IRQs 32..63): MWU is IRQ 32.
        sys.p.write(&sys, 0xE000E104, 4, 1);
        sys.p.write(&sys, 0x40020304, 4, 0x03); // INTEN REGION0 WA+RA
        sys.p.write(&sys, 0x40020600, 4, 0x20001000); // REGION0.START
        sys.p.write(&sys, 0x40020604, 4, 0x20001010); // REGION0.END
        sys.p.write(&sys, 0x40020514, 4, 1); // REGIONENSET region 0
        assert!(crate::system::mwu_armed(), "armed once configured");
        let mut mem = crate::cpu::mem::FlatMemory::new(512 * 1024, 128 * 1024);
        mem.write8(0x20001004, 0xAA);
        assert_eq!(sys.p.read(&sys, 0x40020100, 4), 1, "REGION0.WA on write inside");
        mem.write8(0x20002000, 0xBB);
        assert_eq!(sys.p.read(&sys, 0x40020100, 4), 1, "sticky until cleared");
        assert_eq!(mem.read8(0x20001004), 0xAA, "guarded byte readable");
        assert_eq!(sys.p.read(&sys, 0x40020104, 4), 1, "REGION0.RA on read inside");
        assert!(sys.p.nvic.borrow().has_pending(), "IRQ 32 pends");
        // Outside the region: silent.
        sys.p.write(&sys, 0x40020100, 4, 0);
        sys.p.write(&sys, 0x40020104, 4, 0);
        mem.write8(0x20002000, 0xCC);
        let _ = mem.read8(0x20002000);
        assert_eq!(sys.p.read(&sys, 0x40020100, 4), 0, "no event outside");
        assert_eq!(sys.p.read(&sys, 0x40020104, 4), 0, "no event outside");
        // Disarm: silent everywhere.
        sys.p.write(&sys, 0x40020518, 4, 1); // REGIONENCLR
        assert!(!crate::system::mwu_armed(), "disarmed");
        mem.write8(0x20001004, 0xDD);
        assert_eq!(sys.p.read(&sys, 0x40020100, 4), 0, "no event when disarmed");
    }
    #[test]
    fn pregion_subs_include_exclude() {
        let _g = crate::system::lock_boot();
        let sys0 = crate::system::WasmSystem::new();
        crate::init_for_test(sys0);
        let sys = crate::sys();
        // PREGION0 [0x20003000, 0x20003020): 32 subregions of 1 byte.
        // Watch only subregion 5 (byte 0x20003005).
        sys.p.write(&sys, 0x400206C0, 4, 0x20003000); // START
        sys.p.write(&sys, 0x400206C4, 4, 0x20003020); // END
        sys.p.write(&sys, 0x400206C8, 4, 1 << 5); // SUBS: sub 5 only
        let mut mem = crate::cpu::mem::FlatMemory::new(512 * 1024, 128 * 1024);
        mem.write8(0x20003005, 0x11);
        assert_eq!(sys.p.read(&sys, 0x40020160, 4), 1, "PREGION0.WA on watched sub");
        assert_eq!(sys.p.read(&sys, 0x40020400, 4), 1 << 5, "SUBSTATWA bit 5");
        sys.p.write(&sys, 0x40020160, 4, 0);
        mem.write8(0x20003006, 0x22);
        assert_eq!(sys.p.read(&sys, 0x40020160, 4), 0, "excluded sub silent");
        assert_eq!(sys.p.read(&sys, 0x40020400, 4), 1 << 5, "SUBSTAT keeps bit 5");
        sys.p.write(&sys, 0x40020400, 4, 1 << 5); // write-1-clear
        assert_eq!(sys.p.read(&sys, 0x40020400, 4), 0, "SUBSTAT cleared");
        sys.p.write(&sys, 0x400206C8, 4, 0); // SUBS=0 disarms
        assert!(!crate::system::mwu_armed(), "global disarmed for other tests");
        crate::system::mwu_set_armed(false); // hygiene for other tests
    }
}
