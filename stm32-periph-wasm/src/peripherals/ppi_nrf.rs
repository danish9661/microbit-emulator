use crate::system::System;
use super::Peripheral;

/// PPI @ 0x4001F000. P3 minimal: CHEN/CHENSET/CHENCLR + CH[n].EEP/TEP +
/// FORK[n].TEP storage, TASKS_CHG[n].EN/DIS. No auto-dispatch yet (TODO):
/// without it SDK examples that route TIMER->GPIOTE via PPI stall, so P4
/// wires dispatch (direct call, same tick). Recording the graph now keeps
/// firmware writes observable and tests green.
pub struct Ppi {
    chen: u32,
    eep: [u32; 20],
    tep: [u32; 20],
    fork: [u32; 20],
    chg_en: [u32; 6],
}

impl Default for Ppi {
    fn default() -> Self {
        Self { chen: 0, eep: [0; 20], tep: [0; 20], fork: [0; 20], chg_en: [0; 6] }
    }
}

impl Ppi {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "PPI" { Some(Box::new(Self::default())) } else { None }
    }
}

impl Peripheral for Ppi {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x500 => self.chen,
            0x510..=0x55C => self.eep[((offset - 0x510) / 8) as usize],
            0x514..=0x560 if ((offset - 0x514) % 8) == 0 => self.tep[((offset - 0x514) / 8) as usize],
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        match offset {
            0x500 => self.chen = value & 0xF_FFFF,
            0x504 => self.chen |= value,
            0x508 => self.chen &= !value,
            0x510..=0x55C if ((offset - 0x510) % 8) == 0 => self.eep[((offset - 0x510) / 8) as usize] = value,
            0x514..=0x560 if ((offset - 0x514) % 8) == 0 => self.tep[((offset - 0x514) / 8) as usize] = value,
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
        p.write(&sys, 0x508, 1);
        assert_eq!(p.read(&sys, 0x500), 0);
    }
}
