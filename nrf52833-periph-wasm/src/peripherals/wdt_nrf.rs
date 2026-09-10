use crate::system::System;
use super::Peripheral;

/// WDT @ 0x40010000 (IRQ 16). TASKS_START 0x000, RR[0..7] 0x600+n*4,
/// EVENTS_TIMEOUT 0x100, INTENSET 0x304/CLR 0x308, RUNSTATUS 0x400,
/// REQSTATUS 0x404, CRV 0x504, RREN 0x508, CONFIG 0x50C.
/// Model: START latches running (RUNSTATUS=1); RR writes accepted and
/// noted; TIMEOUT never fires (TODO: expiry -> system reset once the
/// reset path exists). SDK start code that enables + pets the WDT boots.
pub struct WdtNrf {
    running: bool,
    rr: [u32; 8],
}

impl Default for WdtNrf {
    fn default() -> Self {
        Self { running: false, rr: [0; 8] }
    }
}

impl WdtNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "WDT" { Some(Box::new(Self::default())) } else { None }
    }
}

impl Peripheral for WdtNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x100 => 0, // TIMEOUT: never (see above)
            0x304 => 0,
            0x400 => self.running as u32,
            0x404 => 0xFF, // all reload registers enabled
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => self.running = true,
            0x600..=0x61C => {
                let i = ((offset - 0x600) >> 2) as usize;
                if value == 0x6E52_4752 {
                    self.rr[i] = value; // reload magic
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn start_and_pet() {
        let sys = test_dummy_system();
        let mut w = WdtNrf::default();
        assert_eq!(w.read(&sys, 0x400), 0);
        w.write(&sys, 0x000, 1);
        assert_eq!(w.read(&sys, 0x400), 1);
        w.write(&sys, 0x600, 0x6E52_4752);
        assert_eq!(w.read(&sys, 0x100), 0, "never times out");
    }
}
