use crate::system::System;
use super::Peripheral;

/// PDM @ 0x4001D000 (IRQ 19, microphone). P5 stub: TASKS_START 0x000,
/// TASKS_STOP 0x004, EVENTS_STARTED 0x100, EVENTS_STOPPED 0x104,
/// EVENTS_END 0x108, ENABLE 0x500. Sample DMA hydration is P6.
pub struct PdmNrf {
    enabled: bool,
    ev_started: bool,
    ev_stopped: bool,
    ev_end: bool,
}

impl Default for PdmNrf {
    fn default() -> Self {
        Self { enabled: false, ev_started: false, ev_stopped: false, ev_end: false }
    }
}

impl PdmNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "PDM" { Some(Box::new(Self::default())) } else { None }
    }
}

impl Peripheral for PdmNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x100 => self.ev_started as u32,
            0x104 => self.ev_stopped as u32,
            0x108 => self.ev_end as u32,
            0x500 => self.enabled as u32,
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => self.ev_started = true,
            0x004 => { self.ev_stopped = true; self.ev_end = true; }
            0x100 => if value == 0 { self.ev_started = false; }
            0x104 => if value == 0 { self.ev_stopped = false; }
            0x108 => if value == 0 { self.ev_end = false; }
            0x500 => self.enabled = value & 1 == 1,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn start_stop_handshake() {
        let sys = test_dummy_system();
        let mut p = PdmNrf::default();
        p.write(&sys, 0x500, 1);
        p.write(&sys, 0x000, 1);
        assert_eq!(p.read(&sys, 0x100), 1);
        p.write(&sys, 0x004, 1);
        assert_eq!(p.read(&sys, 0x104), 1);
        assert_eq!(p.read(&sys, 0x108), 1);
    }
}
