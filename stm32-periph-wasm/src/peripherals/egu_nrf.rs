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
}
