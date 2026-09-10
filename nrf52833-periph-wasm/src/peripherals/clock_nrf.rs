use crate::system::System;
use super::Peripheral;

/// CLOCK + POWER combined at 0x40000000 (they share the 4KB region on nRF52).
/// Minimal boot stub: TASKS_HFCLKSTART sets EVENTS_HFCLKSTARTED + HFCLKRUN.
/// Everything else reads-as-0, writes ignored. No second clock invented —
/// timing still comes from INSTRUCTION_COUNT.
pub struct ClockPower {
    events_hfclkstarted: bool,
    events_lfclkstarted: bool,
    hfclk_running: bool,
    lfclk_running: bool,
}

impl Default for ClockPower {
    fn default() -> Self {
        Self { events_hfclkstarted: false, events_lfclkstarted: false, hfclk_running: false, lfclk_running: false }
    }
}

impl ClockPower {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "CLOCK" || name == "POWER" || name == "CLOCK_POWER" {
            Some(Box::new(Self::default()))
        } else {
            None
        }
    }
}

impl Peripheral for ClockPower {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x000 => 0, // TASKS_HFCLKSTART (task, reads 0)
            0x100 => self.events_hfclkstarted as u32,
            0x104 => self.events_lfclkstarted as u32,
            0x408 => self.hfclk_running as u32, // HFCLKRUN
            0x40C => self.lfclk_running as u32, // LFCLKRUN
            0x400 => 1, // HFCLKSTAT: always running after start (stub)
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => if value == 1 { self.hfclk_running = true; self.events_hfclkstarted = true; }
            0x004 => if value == 1 { self.hfclk_running = false; }
            0x008 => if value == 1 { self.lfclk_running = true; self.events_lfclkstarted = true; }
            0x100 => if value == 0 { self.events_hfclkstarted = false; }
            0x104 => if value == 0 { self.events_lfclkstarted = false; }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn hfclk_start_sets_event() {
        let sys = test_dummy_system();
        let mut c = ClockPower::default();
        assert_eq!(c.read(&sys, 0x100), 0);
        c.write(&sys, 0x000, 1);
        assert_eq!(c.read(&sys, 0x100), 1);
        assert_eq!(c.read(&sys, 0x408), 1);
        c.write(&sys, 0x100, 0); // EVENTS clear by write-0
        assert_eq!(c.read(&sys, 0x100), 0);
    }
}
