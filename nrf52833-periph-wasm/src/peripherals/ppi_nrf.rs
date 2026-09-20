use crate::system::System;
use super::Peripheral;

/// PPI @ 0x4001F000. Channel graph (CHEN/CHENSET/CHENCLR, CH[n].EEP/TEP,
/// FORK[n].TEP, CHG groups) + direct dispatch: every tick, each enabled
/// channel whose event register reads nonzero fires its task register
/// (edge-triggered: once per set, firmware clears the event as usual).
/// Without this, SDK examples routing TIMER->GPIOTE stall.
pub struct Ppi {
    chen: u32,
    eep: [u32; 20],
    tep: [u32; 20],
    chg: [u32; 6],
    last: [bool; 20],
}

impl Default for Ppi {
    fn default() -> Self {
        Self { chen: 0, eep: [0; 20], tep: [0; 20], chg: [0; 6], last: [false; 20] }
    }
}

impl Ppi {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "PPI" { Some(Box::new(Self::default())) } else { None }
    }
}

/// Inside the PPI window? EEP/TEP pointing here are skipped (tick already
/// holds PPI's borrow; re-entering it would panic).
fn in_ppi(addr: u32) -> bool {
    (0x4001_F000..0x4002_0000).contains(&addr)
}

impl Peripheral for Ppi {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn tick(&mut self, sys: &System) {
        for ch in 0..20 {
            if self.chen & (1 << ch) == 0 {
                self.last[ch] = false;
                continue;
            }
            let (eep, tep) = (self.eep[ch], self.tep[ch]);
            if eep == 0 || tep == 0 || in_ppi(eep) || in_ppi(tep) {
                continue;
            }
            if sys.p.read(sys, eep, 4) != 0 {
                if !self.last[ch] {
                    self.last[ch] = true;
                    sys.p.write(sys, tep, 4, 1);
                }
            } else {
                self.last[ch] = false;
            }
        }
    }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x500 => self.chen,
            0x510..=0x55C if ((offset - 0x510) % 8) == 0 => self.eep[((offset - 0x510) / 8) as usize],
            0x514..=0x560 if ((offset - 0x514) % 8) == 0 => self.tep[((offset - 0x514) / 8) as usize],
            0x800..=0x814 => self.chg[((offset - 0x800) >> 2) as usize],
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        match offset {
            // TASKS_CHG[n].EN/DIS: enable/disable every channel in group n.
            0x000..=0x02C if offset % 8 == 0 => {
                self.chen |= self.chg[(offset >> 3) as usize];
            }
            0x004..=0x02C if offset % 8 == 4 => {
                self.chen &= !self.chg[(offset >> 3) as usize];
            }
            0x500 => self.chen = value & 0xF_FFFF,
            0x504 => self.chen |= value,
            0x508 => self.chen &= !value,
            0x510..=0x55C if ((offset - 0x510) % 8) == 0 => self.eep[((offset - 0x510) / 8) as usize] = value,
            0x514..=0x560 if ((offset - 0x514) % 8) == 0 => self.tep[((offset - 0x514) / 8) as usize] = value,
            0x800..=0x814 => self.chg[((offset - 0x800) >> 2) as usize] = value & 0xF_FFFF,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn chen_and_graph_recorded() {
        let sys = test_dummy_system();
        let mut p = Ppi::default();
        p.write(&sys, 0x510, 0x4000_8140); // CH0.EEP = TIMER0 COMPARE0
        p.write(&sys, 0x514, 0x4000_6000); // CH0.TEP = GPIOTE OUT0
        p.write(&sys, 0x504, 1); // CHENSET0
        assert_eq!(p.read(&sys, 0x500), 1);
        assert_eq!(p.read(&sys, 0x510), 0x4000_8140);
        assert_eq!(p.read(&sys, 0x514), 0x4000_6000, "TEP reads back");
        p.write(&sys, 0x508, 1);
        assert_eq!(p.read(&sys, 0x500), 0);
    }
    #[test]
    fn timer_compare_drives_gpiote_task() {
        use crate::system::{lock_boot, test_dummy_system};
        let _g = lock_boot();
        let sys = test_dummy_system();
        // GPIOTE CH0 = task mode on P0.21
        sys.p.write(&sys, 0x40006510, 4, (3) | (21 << 8));
        // TIMER0: CC0=2, START
        sys.p.write(&sys, 0x40008540, 4, 2);
        sys.p.write(&sys, 0x40008000, 4, 1);
        // PPI CH0: EEP=TIMER0 COMPARE0, TEP=GPIOTE OUT0, enable
        sys.p.write(&sys, 0x4001F510, 4, 0x40008140);
        sys.p.write(&sys, 0x4001F514, 4, 0x40006000);
        sys.p.write(&sys, 0x4001F504, 4, 1);
        assert!(!sys.p.gpio.borrow().read_output_pin(0, 21));
        crate::system::INSTRUCTION_COUNT.fetch_add(64, std::sync::atomic::Ordering::Relaxed);
        sys.tick();
        crate::system::INSTRUCTION_COUNT.fetch_add(64, std::sync::atomic::Ordering::Relaxed);
        sys.tick();
        assert!(sys.p.gpio.borrow().read_output_pin(0, 21), "PPI dispatched COMPARE->OUT");
    }
    #[test]
    fn group_enable_disable() {
        let sys = test_dummy_system();
        let mut p = Ppi::default();
        p.write(&sys, 0x800, 0b101); // CHG0 = CH0 + CH2
        assert_eq!(p.read(&sys, 0x800), 0b101);
        p.write(&sys, 0x000, 1); // TASKS_CHG0.EN
        assert_eq!(p.read(&sys, 0x500), 0b101, "group enabled");
        p.write(&sys, 0x004, 1); // TASKS_CHG0.DIS
        assert_eq!(p.read(&sys, 0x500), 0, "group disabled");
    }
}
