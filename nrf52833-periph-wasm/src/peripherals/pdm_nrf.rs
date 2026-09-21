use crate::system::System;
use super::Peripheral;

/// PDM @ 0x4001D000 (IRQ 19, microphone). Task/event handshake + SAMPLE
/// EASYDMA: TASKS_START 0x000, TASKS_STOP 0x004, EVENTS_STARTED 0x100,
/// EVENTS_STOPPED 0x104, EVENTS_END 0x108, SAMPLE.PTR 0x52C,
/// SAMPLE.MAXCNT 0x530, ENABLE 0x500. START with MAXCNT>0 stages a driver
/// transfer (take_sample -> RAM write -> complete_sample); MAXCNT==0 keeps
/// the P5 immediate-END timing.
pub struct PdmNrf {
    enabled: bool,
    ev_started: bool,
    ev_stopped: bool,
    ev_end: bool,
    intenset: u32,
    sample_ptr: u32,
    sample_maxcnt: u32,
    sample_pending: bool,
}

impl Default for PdmNrf {
    fn default() -> Self {
        Self { enabled: false, ev_started: false, ev_stopped: false, ev_end: false,
               intenset: 0, sample_ptr: 0, sample_maxcnt: 0, sample_pending: false }
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
            0x304 => self.intenset,
            0x500 => self.enabled as u32,
            0x52C => self.sample_ptr,
            0x530 => self.sample_maxcnt,
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => {
                self.ev_started = true;
                self.sample_pending = self.sample_maxcnt > 0;
                if !self.sample_pending {
                    self.ev_end = true; // P5 immediate timing
                }
            }
            0x004 => { self.ev_stopped = true; self.ev_end = true; self.sample_pending = false; }
            0x100 => if value == 0 { self.ev_started = false; }
            0x104 => if value == 0 { self.ev_stopped = false; }
            0x108 => if value == 0 { self.ev_end = false; }
            0x304 => self.intenset |= value,
            0x308 => self.intenset &= !value,
            0x500 => self.enabled = value & 1 == 1,
            0x52C => self.sample_ptr = value,
            0x530 => self.sample_maxcnt = value & 0x7FFF,
            _ => {}
        }
    }
}

/// Take a staged SAMPLE transfer (ptr, maxcnt); None when idle.
pub fn take_sample(sys: &System) -> Option<(u32, u32)> {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4001_D000 {
            // try_borrow_mut (P108 family): take/complete paths re-enter
            // via read/write/tick while borrowed; drop instead of panic.
            let mut b = match slot.peripheral.try_borrow_mut() {
                Ok(b) => b,
                Err(_) => return None,
            };
            if let Some(p) = b.as_any_mut().downcast_mut::<PdmNrf>() {
                if p.sample_pending {
                    p.sample_pending = false;
                    return Some((p.sample_ptr, p.sample_maxcnt));
                }
                return None;
            }
            return None;
        }
    }
    None
}

/// Complete SAMPLE: driver wrote samples to RAM at PTR.
/// END (bit 2) IRQ pended per INTEN (SVD ground truth).
pub fn complete_sample(sys: &System) {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4001_D000 {
            // try_borrow_mut (P108 family): take/complete paths re-enter
            // via read/write/tick while borrowed; drop instead of panic.
            let mut b = match slot.peripheral.try_borrow_mut() {
                Ok(b) => b,
                Err(_) => return,
            };
            if let Some(p) = b.as_any_mut().downcast_mut::<PdmNrf>() {
                p.ev_end = true;
                if p.intenset & (1 << 2) != 0 {
                    sys.p.nvic.borrow_mut().set_intr_pending(29);
                }
            }
            return;
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
    #[test]
    fn sample_dma_roundtrip() {
        use crate::system::test_dummy_system;
        let sys = test_dummy_system();
        sys.p.write(&sys, 0x4001D52C, 4, 0x20002000); // SAMPLE.PTR
        sys.p.write(&sys, 0x4001D530, 4, 8);          // SAMPLE.MAXCNT
        sys.p.write(&sys, 0x4001D500, 4, 1);          // ENABLE
        sys.p.write(&sys, 0x4001D000, 4, 1);          // START
        assert_eq!(sys.p.read(&sys, 0x4001D108, 4), 0, "END waits for driver");
        let t = take_sample(&sys).expect("staged");
        assert_eq!(t, (0x20002000, 8));
        complete_sample(&sys);
        assert_eq!(sys.p.read(&sys, 0x4001D108, 4), 1, "END after complete");
    }
}
