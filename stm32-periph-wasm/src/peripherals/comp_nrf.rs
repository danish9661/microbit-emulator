use crate::system::System;
use super::Peripheral;

/// COMP @ 0x40013000 (IRQ 19, +LPCOMP alias same base). TASKS_START 0x000,
/// TASKS_STOP 0x004, TASKS_SAMPLE 0x008, EVENTS_READY 0x100,
/// EVENTS_DOWN 0x104, EVENTS_UP 0x108, EVENTS_CROSS 0x10C,
/// INTENSET 0x304/CLR 0x308, RESULT 0x400, ENABLE 0x500, PSEL 0x504,
/// MODE 0x50C. Stub: START->READY, RESULT=0 (below threshold).
pub struct CompNrf {
    enabled: bool,
    ev_ready: bool,
}

impl Default for CompNrf {
    fn default() -> Self {
        Self { enabled: false, ev_ready: false }
    }
}

impl CompNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "COMP" || name == "LPCOMP" {
            Some(Box::new(Self::default()))
        } else {
            None
        }
    }
}

impl Peripheral for CompNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x100 => self.ev_ready as u32,
            0x400 => 0,
            0x500 => self.enabled as u32,
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => self.ev_ready = true,
            0x004 => self.ev_ready = false,
            0x100 => if value == 0 { self.ev_ready = false; }
            0x500 => self.enabled = value & 3 != 0,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn ready_handshake() {
        let sys = test_dummy_system();
        let mut c = CompNrf::default();
        c.write(&sys, 0x500, 2);
        c.write(&sys, 0x000, 1);
        assert_eq!(c.read(&sys, 0x100), 1);
    }
}
