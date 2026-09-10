use crate::system::{System, instruction_count};
use super::Peripheral;

/// SAADC @ 0x40007000 (IRQ 7). P4 subset: TASKS_START 0x000,
/// TASKS_SAMPLE 0x004, TASKS_STOP 0x008, TASKS_CALIBRATEOFFSET 0x010,
/// EVENTS_STARTED 0x100, EVENTS_END 0x104, EVENTS_DONE 0x108,
/// EVENTS_CALIBRATEDONE 0x110, EVENTS_CH_LIMITH/LIMITL 0x118+,
/// INTENSET 0x304/CLR 0x308, ENABLE 0x500, RESOLUTION 0x5F0.
/// EASYDMA RESULT hydration comes with the DMA pass (P5); P4 proves the
/// task->event handshake firmware polls before touching RESULT.
pub struct Saadc {
    enabled: bool,
    ev_started: bool,
    ev_end: bool,
    ev_done: bool,
    ev_cal: bool,
    intenset: u32,
}

impl Default for Saadc {
    fn default() -> Self {
        Self { enabled: false, ev_started: false, ev_end: false,
               ev_done: false, ev_cal: false, intenset: 0 }
    }
}

impl Saadc {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "SAADC" { Some(Box::new(Self::default())) } else { None }
    }
    fn fire(&self, sys: &System, bit: u32) {
        if self.intenset & bit != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(7);
        }
    }
}

impl Peripheral for Saadc {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x100 => self.ev_started as u32,
            0x104 => self.ev_end as u32,
            0x108 => self.ev_done as u32,
            0x110 => self.ev_cal as u32,
            0x304 => self.intenset,
            0x500 => self.enabled as u32,
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => { self.ev_started = true; self.fire(sys, 1); }
            0x004 => {
                self.ev_end = true; self.ev_done = true;
                let _ = instruction_count();
                self.fire(sys, 1 << 1); self.fire(sys, 1 << 2);
            }
            0x008 => { self.ev_started = false; }
            0x010 => { self.ev_cal = true; self.fire(sys, 1 << 4); }
            0x100 => if value == 0 { self.ev_started = false; }
            0x104 => if value == 0 { self.ev_end = false; }
            0x108 => if value == 0 { self.ev_done = false; }
            0x110 => if value == 0 { self.ev_cal = false; }
            0x304 => self.intenset |= value,
            0x308 => self.intenset &= !value,
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
    fn start_sample_stop_handshake() {
        let sys = test_dummy_system();
        let mut s = Saadc::default();
        s.write(&sys, 0x500, 1);
        s.write(&sys, 0x000, 1);
        assert_eq!(s.read(&sys, 0x100), 1);
        s.write(&sys, 0x004, 1);
        assert_eq!(s.read(&sys, 0x104), 1);
        assert_eq!(s.read(&sys, 0x108), 1);
    }
}
