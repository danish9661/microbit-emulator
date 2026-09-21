use crate::system::{System, instruction_count};
use super::Peripheral;

/// SAADC @ 0x40007000 (IRQ 7). Task/event handshake + RESULT EASYDMA:
/// TASKS_START 0x000, TASKS_SAMPLE 0x004, TASKS_STOP 0x008,
/// TASKS_CALIBRATEOFFSET 0x010, EVENTS_STARTED 0x100, EVENTS_END 0x104,
/// EVENTS_DONE 0x108, EVENTS_RESULTDONE 0x10C, EVENTS_CALIBRATEDONE 0x110,
/// EVENTS_STOPPED 0x114, EVENTS_CH[n].LIMITH 0x118+8n / LIMITL 0x11C+8n
/// (n=0..7), INTENSET 0x304 (STARTED 0, END 1, DONE 2, RESULTDONE 3,
/// CALIBRATEDONE 4, STOPPED 5, CHnLIMITH 6+2n, CHnLIMITL 7+2n) / CLR 0x308,
/// ENABLE 0x500, CH[n].PSELP 0x510+16n / PSELN 0x514+16n / CONFIG 0x518+16n
/// / LIMIT 0x51C+16n (LOW[15:0], HIGH[31:16], signed), RESOLUTION 0x5F0,
/// OVERSAMPLE 0x5F4, SAMPLERATE 0x5F8, RESULT.PTR 0x62C, RESULT.MAXCNT 0x630,
/// RESULT.AMOUNT 0x634.
/// DMA rule: SAMPLE with RESULT.MAXCNT>0 stages a driver transfer
/// (take_result -> RAM write -> complete_result); MAXCNT==0 completes at
/// once (polling timing preserved). Conversion itself runs driver-side
/// (the model has no analog handle); the driver reports each result via
/// saadc_check_limits(), which evaluates CH LIMIT windows (signed 16-bit,
/// like the 12-bit result domain) and raises LIMITH/LIMITL + IRQ.
pub struct Saadc {
    enabled: bool,
    ev_started: bool,
    ev_end: bool,
    ev_done: bool,
    ev_resultdone: bool,
    ev_cal: bool,
    ev_stopped: bool,
    ev_limith: [bool; 8],
    ev_limitl: [bool; 8],
    intenset: u32,
    ch_pselp: [u32; 8],
    ch_pseln: [u32; 8],
    ch_config: [u32; 8],
    ch_limlo: [i16; 8],
    ch_limhi: [i16; 8],
    resolution: u32,
    oversample: u32,
    samplerate: u32,
    res_ptr: u32,
    res_maxcnt: u32,
    res_amount: u32,
    res_pending: bool,
}

impl Default for Saadc {
    fn default() -> Self {
        Self { enabled: false, ev_started: false, ev_end: false,
               ev_done: false, ev_resultdone: false, ev_cal: false,
               ev_stopped: false, ev_limith: [false; 8], ev_limitl: [false; 8],
               intenset: 0, ch_pselp: [0xFFFFFFFF; 8], ch_pseln: [0xFFFFFFFF; 8],
               ch_config: [0; 8], ch_limlo: [0; 8], ch_limhi: [0; 8],
               resolution: 1, oversample: 0, samplerate: 0,
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
            0x10C => self.ev_resultdone as u32,
            0x110 => self.ev_cal as u32,
            0x114 => self.ev_stopped as u32,
            0x118..=0x154 => {
                let n = ((offset - 0x118) >> 3) as usize;
                if n < 8 {
                    if (offset - 0x118) & 4 == 0 {
                        self.ev_limith[n] as u32
                    } else {
                        self.ev_limitl[n] as u32
                    }
                } else {
                    0
                }
            }
            0x304 => self.intenset,
            0x500 => self.enabled as u32,
            0x510..=0x58F if ((offset - 0x510) & 0xF) < 0xD => {
                // CH[n] block: PSELP+0, PSELN+4, CONFIG+8, LIMIT+12.
                let n = ((offset - 0x510) >> 4) as usize;
                if n >= 8 {
                    0
                } else {
                    match (offset - 0x510) & 0xF {
                        0x0 => self.ch_pselp[n],
                        0x4 => self.ch_pseln[n],
                        0x8 => self.ch_config[n],
                        _ => ((self.ch_limhi[n] as u16 as u32) << 16)
                            | (self.ch_limlo[n] as u16 as u32),
                    }
                }
            }
            0x5F0 => self.resolution,
            0x5F4 => self.oversample,
            0x5F8 => self.samplerate,
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
            0x008 => {
                self.ev_started = false;
                self.ev_stopped = true;
                self.fire(sys, 1 << 5);
            }
            0x010 => { self.ev_cal = true; self.fire(sys, 1 << 4); }
            0x100 => if value == 0 { self.ev_started = false; }
            0x104 => if value == 0 { self.ev_end = false; }
            0x108 => if value == 0 { self.ev_done = false; }
            0x10C => if value == 0 { self.ev_resultdone = false; }
            0x110 => if value == 0 { self.ev_cal = false; }
            0x114 => if value == 0 { self.ev_stopped = false; }
            0x118..=0x154 => {
                if value == 0 {
                    let n = ((offset - 0x118) >> 3) as usize;
                    if n < 8 {
                        if (offset - 0x118) & 4 == 0 {
                            self.ev_limith[n] = false;
                        } else {
                            self.ev_limitl[n] = false;
                        }
                    }
                }
            }
            0x304 => self.intenset |= value,
            0x308 => self.intenset &= !value,
            0x500 => self.enabled = value & 1 == 1,
            0x510..=0x58F if ((offset - 0x510) & 0xF) < 0xD => {
                let n = ((offset - 0x510) >> 4) as usize;
                if n < 8 {
                    match (offset - 0x510) & 0xF {
                        0x0 => self.ch_pselp[n] = value,
                        0x4 => self.ch_pseln[n] = value,
                        0x8 => self.ch_config[n] = value,
                        _ => {
                            self.ch_limlo[n] = value as i16;
                            self.ch_limhi[n] = (value >> 16) as i16;
                        }
                    }
                }
            }
            0x5F0 => self.resolution = value & 3,
            0x5F4 => self.oversample = value & 0xF,
            0x5F8 => self.samplerate = value & 1,
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
            // try_borrow_mut (P108 family): take/complete paths re-enter
            // via read/write/tick while borrowed; drop instead of panic.
            let mut b = match slot.peripheral.try_borrow_mut() {
                Ok(b) => b,
                Err(_) => return None,
            };
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
            // try_borrow_mut (P108 family): take/complete paths re-enter
            // via read/write/tick while borrowed; drop instead of panic.
            let mut b = match slot.peripheral.try_borrow_mut() {
                Ok(b) => b,
                Err(_) => return,
            };
            if let Some(s) = b.as_any_mut().downcast_mut::<Saadc>() {
                s.res_amount = amount;
                s.ev_end = true;
                s.ev_done = true;
                s.ev_resultdone = true;
                if s.intenset & 0xE != 0 {
                    sys.p.nvic.borrow_mut().set_intr_pending(7);
                }
            }
            return;
        }
    }
}

/// Report one converted sample for limit monitoring: the driver converted
/// channel `ch` to signed `value` (result-LSB domain) and wrote it to RAM
/// itself; this evaluates the CH LIMIT window (LOW[15:0], HIGH[31:16],
/// signed) and raises LIMITH/LIMITL + IRQ. Values strictly inside the
/// window raise nothing.
pub fn check_limits(sys: &System, ch: usize, value: i16) {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4000_7000 {
            // try_borrow_mut (P108 family): take/complete paths re-enter
            // via read/write/tick while borrowed; drop instead of panic.
            let mut b = match slot.peripheral.try_borrow_mut() {
                Ok(b) => b,
                Err(_) => return,
            };
            if let Some(s) = b.as_any_mut().downcast_mut::<Saadc>() {
                if ch >= 8 {
                    return;
                }
                if value > s.ch_limhi[ch] {
                    s.ev_limith[ch] = true;
                    s.fire(sys, 1 << (6 + 2 * ch as u32));
                } else if value < s.ch_limlo[ch] {
                    s.ev_limitl[ch] = true;
                    s.fire(sys, 1 << (7 + 2 * ch as u32));
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
    fn channel_config_limits_resultdone_stopped() {
        let sys = test_dummy_system();
        sys.p.write(&sys, 0xE000E100, 4, 1 << 7); // NVIC ISER: SAADC
        // CH2: PSELP=AIN2, LIMIT window [-100, +100].
        sys.p.write(&sys, 0x40007530, 4, 2); // CH2.PSELP
        assert_eq!(sys.p.read(&sys, 0x40007530, 4), 2);
        sys.p.write(&sys, 0x4000753C, 4, ((100u32) << 16) | (0xFF9Cu32)); // HIGH=100, LOW=-100
        assert_eq!(sys.p.read(&sys, 0x4000753C, 4), (100 << 16) | 0xFF9C);
        sys.p.write(&sys, 0x400075F0, 4, 2); // RESOLUTION 12-bit
        assert_eq!(sys.p.read(&sys, 0x400075F0, 4), 2);
        sys.p.write(&sys, 0x40007304, 4, (1 << 10) | (1 << 11)); // INTEN CH2 H/L
        check_limits(&sys, 2, 50);
        assert_eq!(sys.p.read(&sys, 0x40007128, 4), 0, "inside: no LIMITH");
        assert_eq!(sys.p.read(&sys, 0x4000712C, 4), 0, "inside: no LIMITL");
        check_limits(&sys, 2, 150);
        assert_eq!(sys.p.read(&sys, 0x40007128, 4), 1, "LIMITH");
        assert!(sys.p.nvic.borrow().has_pending(), "IRQ 7 pends");
        check_limits(&sys, 2, -150);
        assert_eq!(sys.p.read(&sys, 0x4000712C, 4), 1, "LIMITL");
        sys.p.write(&sys, 0x40007128, 4, 0);
        assert_eq!(sys.p.read(&sys, 0x40007128, 4), 0, "clear by write-0");
        // STOP + RESULTDONE events.
        sys.p.write(&sys, 0x40007008, 4, 1); // STOP
        assert_eq!(sys.p.read(&sys, 0x40007114, 4), 1, "STOPPED");
        sys.p.write(&sys, 0x4000762C, 4, 0x20002000);
        sys.p.write(&sys, 0x40007630, 4, 1);
        sys.p.write(&sys, 0x40007500, 4, 1);
        sys.p.write(&sys, 0x40007000, 4, 1);
        sys.p.write(&sys, 0x40007004, 4, 1);
        let _ = take_result(&sys);
        complete_result(&sys, 1);
        assert_eq!(sys.p.read(&sys, 0x4000710C, 4), 1, "RESULTDONE");
        // 2nd run: fresh defaults, limits zeroed (0 inside [0,0]).
        let s2 = Saadc::default();
        assert_eq!(s2.ch_limhi[2], 0);
        check_limits(&sys, 9, 1000); // bad channel: ignored, no panic
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
