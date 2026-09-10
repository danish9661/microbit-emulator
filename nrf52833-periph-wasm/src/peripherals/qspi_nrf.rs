use crate::system::System;
use super::Peripheral;

/// QSPI @ 0x40029000 (IRQ 41, external flash). P5 stub: READY=1 always,
/// TASKS_ACTIVATE 0x000, TASKS_READSTART 0x004, TASKS_WRITESTART 0x008,
/// TASKS_ERASESTART 0x00C, TASKS_DEACTIVATE 0x010, EVENTS_READY 0x104,
/// ENABLE 0x500. Indirect-transfer data path is P6.
pub struct QspiNrf {
    enabled: bool,
    ev_ready: bool,
}

impl Default for QspiNrf {
    fn default() -> Self {
        Self { enabled: false, ev_ready: false }
    }
}

impl QspiNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "QSPI" { Some(Box::new(Self::default())) } else { None }
    }
}

impl Peripheral for QspiNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x104 => self.ev_ready as u32,
            0x400 => 1, // READY (IFSTATUS-like): always ready
            0x500 => self.enabled as u32,
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 | 0x004 | 0x008 | 0x00C => self.ev_ready = true,
            0x010 => self.ev_ready = false,
            0x104 => if value == 0 { self.ev_ready = false; }
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
    fn activate_sets_ready_event() {
        let sys = test_dummy_system();
        let mut q = QspiNrf::default();
        assert_eq!(q.read(&sys, 0x400), 1);
        q.write(&sys, 0x500, 1);
        q.write(&sys, 0x000, 1);
        assert_eq!(q.read(&sys, 0x104), 1);
    }
}
