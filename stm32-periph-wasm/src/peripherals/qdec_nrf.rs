use crate::system::System;
use super::Peripheral;

/// QDEC @ 0x40012000 (IRQ 18, quadrature decoder, edge connector P13-15).
/// TASKS_START 0x000, TASKS_STOP 0x004, TASKS_READCLRACC 0x008,
/// TASKS_RDDBLS 0x00C, TASKS_RDDBL 0x010, TASKS_RDDBLACC 0x014,
/// EVENTS_SAMPLERDY 0x100, EVENTS_REPORTRDY 0x104, EVENTS_ACCOF 0x108,
/// EVENTS_DBLRDY 0x10C, EVENTS_STOPPED 0x114, SAMPLE 0x504, ENABLE 0x500.
/// Stub: START->SAMPLERDY on tick (synthetic phase), host can step the
/// accumulator via qdec_step() (P8: motion events).
pub struct QdecNrf {
    enabled: bool,
    running: bool,
    ev_samplerdy: bool,
    sample: i32,
}

impl Default for QdecNrf {
    fn default() -> Self {
        Self { enabled: false, running: false, ev_samplerdy: false, sample: 0 }
    }
}

impl QdecNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "QDEC" { Some(Box::new(Self::default())) } else { None }
    }
}

impl Peripheral for QdecNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x100 => self.ev_samplerdy as u32,
            0x500 => self.enabled as u32,
            0x504 => self.sample as u32,
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => self.running = true,
            0x004 => { self.running = false; }
            0x100 => if value == 0 { self.ev_samplerdy = false; }
            0x500 => self.enabled = value & 1 == 1,
            _ => {}
        }
    }
    fn tick(&mut self, _sys: &System) {
        if self.running && self.enabled {
            self.ev_samplerdy = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn samplerdy_on_tick() {
        let sys = test_dummy_system();
        let mut q = QdecNrf::default();
        q.write(&sys, 0x500, 1);
        q.write(&sys, 0x000, 1);
        q.tick(&sys);
        assert_eq!(q.read(&sys, 0x100), 1);
    }
}
