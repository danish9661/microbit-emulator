use crate::system::{System, instruction_count};
use super::Peripheral;

/// SAADC @ 0x40007000 (IRQ 7). Task/event handshake + RESULT EASYDMA:
/// TASKS_START 0x000, TASKS_SAMPLE 0x004, TASKS_STOP 0x008,
/// TASKS_CALIBRATEOFFSET 0x010, EVENTS_STARTED 0x100, EVENTS_END 0x104,
/// EVENTS_DONE 0x108, EVENTS_CALIBRATEDONE 0x110, RESULT.PTR 0x62C,
/// RESULT.MAXCNT 0x630, RESULT.AMOUNT 0x634, ENABLE 0x500.
/// DMA rule: SAMPLE with RESULT.MAXCNT>0 stages a driver transfer
/// (take_result -> RAM write -> complete_result); MAXCNT==0 completes at
/// once (polling timing preserved).
pub struct Saadc {
    enabled: bool,
    ev_started: bool,
    ev_end: bool,
    ev_done: bool,
    ev_cal: bool,
    intenset: u32,
    res_ptr: u32,
    res_maxcnt: u32,
    res_amount: u32,
    res_pending: bool,
}

impl Default for Saadc {
    fn default() -> Self {
        Self { enabled: false, ev_started: false, ev_end: false,
               ev_done: false, ev_cal: false, intenset: 0,
               res_ptr: 0, res_maxcnt: 0, res_amount: 0, res_pending: false }
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
            0x62C => self.res_ptr,
            0x630 => self.res_maxcnt,
            0x634 => self.res_amount,
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => { self.ev_started = true; self.fire(sys, 1); }
            0x004 => {
                if self.res_maxcnt > 0 {
                    self.res_pending = true; // driver completes
                    self.res_amount = 0;
                } else {
                    self.ev_end = true; self.ev_done = true;
                    let _ = instruction_count();
                    self.fire(sys, 1 << 1); self.fire(sys, 1 << 2);
                }
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
            0x62C => self.res_ptr = value,
            0x630 => self.res_maxcnt = value & 0x7FFF,
            _ => {}
        }
    }
}

/// Take a staged RESULT transfer (ptr, maxcnt); None when idle.
pub fn take_result(sys: &System) -> Option<(u32, u32)> {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4000_7000 {
            let mut b = slot.peripheral.borrow_mut();
            if let Some(s) = b.as_any_mut().downcast_mut::<Saadc>() {
                if s.res_pending {
                    s.res_pending = false;
                    return Some((s.res_ptr, s.res_maxcnt));
                }
                return None;
            }
            return None;
        }
    }
    None
}

/// Complete RESULT: driver wrote `amount` samples to RAM at PTR.
/// END (bit 1) + DONE (bit 2) IRQs pended per INTEN (SVD ground truth).
pub fn complete_result(sys: &System, amount: u32) {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4000_7000 {
            let mut b = slot.peripheral.borrow_mut();
            if let Some(s) = b.as_any_mut().downcast_mut::<Saadc>() {
                s.res_amount = amount;
                s.ev_end = true;
                s.ev_done = true;
                if s.intenset & 0x6 != 0 {
                    sys.p.nvic.borrow_mut().set_intr_pending(7);
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
    #[test]
    fn result_dma_roundtrip() {
        use crate::system::test_dummy_system;
        let sys = test_dummy_system();
        sys.p.write(&sys, 0x4000762C, 4, 0x20002000); // RESULT.PTR
        sys.p.write(&sys, 0x40007630, 4, 2);          // RESULT.MAXCNT
        sys.p.write(&sys, 0x40007500, 4, 1);          // ENABLE
        sys.p.write(&sys, 0x40007000, 4, 1);          // START
        sys.p.write(&sys, 0x40007004, 4, 1);          // SAMPLE
        assert_eq!(sys.p.read(&sys, 0x40007104, 4), 0, "END waits for driver");
        let t = take_result(&sys).expect("staged");
        assert_eq!(t, (0x20002000, 2));
        complete_result(&sys, 2);
        assert_eq!(sys.p.read(&sys, 0x40007104, 4), 1, "END after complete");
        assert_eq!(sys.p.read(&sys, 0x40007634, 4), 2, "AMOUNT");
    }
    #[test]
    fn result_completion_irq_when_enabled() {
        use crate::system::test_dummy_system;
        let sys = test_dummy_system();
        sys.p.write(&sys, 0xE000E100, 4, 1 << 7); // NVIC ISER: SAADC
        sys.p.write(&sys, 0x40007304, 4, 1 << 1); // INTEN: END
        sys.p.write(&sys, 0x4000762C, 4, 0x20002000);
        sys.p.write(&sys, 0x40007630, 4, 1);
        sys.p.write(&sys, 0x40007500, 4, 1);
        sys.p.write(&sys, 0x40007000, 4, 1);
        sys.p.write(&sys, 0x40007004, 4, 1);
        complete_result(&sys, 1);
        assert!(sys.p.nvic.borrow().has_pending(), "END IRQ pends");
    }
}
