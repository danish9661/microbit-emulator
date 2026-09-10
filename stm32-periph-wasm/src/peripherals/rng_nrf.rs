use crate::system::{System, instruction_count};
use super::Peripheral;

/// RNG @ 0x4000D000 (IRQ 22). TASKS_START 0x000, TASKS_STOP 0x004,
/// EVENTS_VALRDY 0x100, SHORTS 0x200, INTENSET 0x304/CLR 0x308,
/// VALUE 0x508. Deterministic LCG seeded from INSTRUCTION_COUNT
/// (same recipe as the old STM32 RNG, new addresses). SHORTS bit0 =
/// shortcut VALRDY->STOP.
pub struct RngNrf {
    running: bool,
    ev_valrdy: bool,
    value: u32,
    shorts: u32,
    intenset: u32,
}

impl Default for RngNrf {
    fn default() -> Self {
        Self { running: false, ev_valrdy: false, value: 0, shorts: 0, intenset: 0 }
    }
}

impl RngNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "RNG" { Some(Box::new(Self::default())) } else { None }
    }
    fn generate(&mut self, sys: &System) {
        let n = instruction_count() as u32;
        self.value = n.wrapping_mul(1103515245).wrapping_add(12345);
        self.value ^= self.value >> 16;
        self.value ^= self.value << 5;
        if self.value == 0 { self.value = 0x1F2E_3D4C; }
        self.ev_valrdy = true;
        if self.intenset & 1 != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(22);
        }
        if self.shorts & 1 != 0 { self.running = false; }
    }
}

impl Peripheral for RngNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, sys: &System, offset: u32) -> u32 {
        match offset {
            0x100 => {
                if self.running && !self.ev_valrdy {
                    self.generate(sys);
                }
                self.ev_valrdy as u32
            }
            0x200 => self.shorts,
            0x304 => self.intenset,
            0x508 => {
                let v = self.value;
                self.ev_valrdy = false;
                if self.running {
                    // next read regenerates (Nordic re-arms on VALUE read)
                    self.generate(sys);
                    return v;
                }
                v
            }
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => { self.running = true; self.generate(sys); }
            0x004 => { self.running = false; }
            0x100 => if value == 0 { self.ev_valrdy = false; },
            0x200 => self.shorts = value & 1,
            0x304 => self.intenset |= value,
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
    fn start_yields_nonzero_value() {
        let sys = test_dummy_system();
        let mut r = RngNrf::default();
        r.write(&sys, 0x000, 1);
        assert_eq!(r.read(&sys, 0x100), 1);
        assert_ne!(r.read(&sys, 0x508), 0);
    }
}
