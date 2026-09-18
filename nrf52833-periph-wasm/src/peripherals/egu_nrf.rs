use crate::system::System;
use super::Peripheral;

/// EGU0-5 @ 0x40014000 + n*0x1000 (IRQs 20-25, SWI/EGU software interrupts).
/// TASKS_TRIGGER[n] 0x000+n*4 (n=0..15), EVENTS_TRIGGERED[n] 0x100+n*4,
/// INTENSET 0x304/CLR 0x308. TRIGGER sets TRIGGERED + fires the instance
/// IRQ when enabled. Used by SDKs/SoftDevice-less stacks for deferred work.
pub struct EguNrf {
    irq: i32,
    ev: [bool; 16],
    intenset: u32,
}

impl EguNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        let irq = match name {
            "EGU0" | "SWI0" => 20,
            "EGU1" | "SWI1" => 21,
            "EGU2" | "SWI2" => 22,
            "EGU3" | "SWI3" => 23,
            "EGU4" | "SWI4" => 24,
            "EGU5" | "SWI5" => 25,
            _ => return None,
        };
        Some(Box::new(Self { irq, ev: [false; 16], intenset: 0 }))
    }
}

impl Peripheral for EguNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x100..=0x13C => self.ev[((offset - 0x100) >> 2) as usize] as u32,
            0x304 => self.intenset,
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x000..=0x03C => {
                let n = (offset >> 2) as usize;
                self.ev[n] = true;
                if self.intenset & (1 << n) != 0 {
                    sys.p.nvic.borrow_mut().set_intr_pending(self.irq);
                }
            }
            0x100..=0x13C => if value == 0 { self.ev[((offset - 0x100) >> 2) as usize] = false; }
            0x304 => self.intenset |= value & 0xFFFF,
            0x308 => self.intenset &= !value,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn trigger_fires_irq_when_enabled() {
        let sys = test_dummy_system();
        // NVIC deliverability needs the ISER bit (real NVIC behavior).
        sys.p.write(&sys, 0xE000E100, 4, 1 << 20);
        let mut e = EguNrf::new("EGU0").unwrap();
        e.write(&sys, 0x304, 1);
        e.write(&sys, 0x000, 1);
        assert_eq!(e.read(&sys, 0x100), 1);
        assert!(sys.p.nvic.borrow().has_pending());
    }
    #[test]
    fn all_instances_live_in_map() {
        // EGU1-5 slots exist in new_wasm (were missing); SWI aliases share
        // the same bases. Highest instance exercises the full path.
        let sys = test_dummy_system();
        sys.p.write(&sys, 0xE000E100, 4, 1 << 25);
        sys.p.write(&sys, 0x40019304, 4, 1 << 3);
        sys.p.write(&sys, 0x4001900C, 4, 1); // EGU5 TASKS_TRIGGER[3]
        assert_eq!(sys.p.read(&sys, 0x4001910C, 4), 1, "EGU5 TRIGGERED[3]");
        assert!(sys.p.nvic.borrow().has_pending(), "EGU5 IRQ 25 pends");
        sys.p.write(&sys, 0x4001910C, 4, 0);
        assert_eq!(sys.p.read(&sys, 0x4001910C, 4), 0, "clear by write-0");
    }
    #[test]
    fn channels_are_independent_and_masked() {
        // TRIGGER[5] sets only TRIGGERED[5]; INTEN gates per-bit (bit 5
        // set, bit 7 clear => TRIGGER[7] sets its event but no IRQ).
        let sys = test_dummy_system();
        sys.p.write(&sys, 0xE000E100, 4, 1 << 22); // NVIC ISER: EGU2
        let mut e = EguNrf::new("EGU2").unwrap();
        e.write(&sys, 0x304, 1 << 5);
        e.write(&sys, 0x014, 1); // TASKS_TRIGGER[5]
        assert_eq!(e.read(&sys, 0x114), 1, "TRIGGERED[5]");
        assert_eq!(e.read(&sys, 0x100), 0, "TRIGGERED[0] untouched");
        assert_eq!(e.read(&sys, 0x118), 0, "TRIGGERED[6] untouched");
        assert!(sys.p.nvic.borrow().has_pending(), "EGU2 IRQ 22 pends");
        e.write(&sys, 0x01C, 1); // TASKS_TRIGGER[7], INTEN bit clear
        assert_eq!(e.read(&sys, 0x11C), 1, "TRIGGERED[7] still latches");
        e.write(&sys, 0x114, 0);
        assert_eq!(e.read(&sys, 0x114), 0, "clear by write-0");
        e.write(&sys, 0x308, 1 << 5); // INTENCLR
        assert_eq!(e.read(&sys, 0x304), 0, "INTEN cleared");
    }
}
