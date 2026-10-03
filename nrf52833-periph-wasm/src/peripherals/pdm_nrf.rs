use crate::system::System;
use super::Peripheral;

/// PDM @ 0x4001D000 (IRQ 29, microphone). Task/event handshake + SAMPLE
/// EASYDMA: TASKS_START 0x000, TASKS_STOP 0x004, EVENTS_STARTED 0x100,
/// EVENTS_STOPPED 0x104, EVENTS_END 0x108, INTEN 0x300 / SET 0x304
/// (STARTED 0, STOPPED 1, END 2) / CLR 0x308, ENABLE 0x500,
/// PDMCLKCTRL 0x504 (32-bit clock-frequency code, stored),
/// MODE 0x508 (OPERATION bit0, EDGE bit1), GAINL 0x518 / GAINR 0x51C
/// (7-bit), RATIO 0x520, PSEL.CLK 0x540 / DIN 0x544,
/// SAMPLE.PTR 0x560 / MAXCNT 0x564. START with MAXCNT>0 stages a driver
/// transfer (take_sample -> RAM write -> complete_sample); MAXCNT==0 keeps
/// the immediate-END timing.
pub struct PdmNrf {
    enabled: bool,
    ev_started: bool,
    ev_stopped: bool,
    ev_end: bool,
    intenset: u32,
    clkctrl: u32,
    mode: u32,
    gainl: u32,
    gainr: u32,
    ratio: u32,
    pselclk: u32,
    pseldin: u32,
    sample_ptr: u32,
    sample_maxcnt: u32,
    sample_pending: bool,
}

impl Default for PdmNrf {
    fn default() -> Self {
        Self { enabled: false, ev_started: false, ev_stopped: false, ev_end: false,
               intenset: 0, clkctrl: 0x08000000, mode: 0, gainl: 0x28, gainr: 0x28,
               ratio: 0, pselclk: 0xFFFF_FFFF, pseldin: 0xFFFF_FFFF,
               sample_ptr: 0, sample_maxcnt: 0, sample_pending: false }
    }
}

impl PdmNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "PDM" { Some(Box::new(Self::default())) } else { None }
    }
    fn fire(&self, sys: &System, bit: u32) {
        if self.intenset & bit != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(29);
        }
    }
}

impl Peripheral for PdmNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x100 => self.ev_started as u32,
            0x104 => self.ev_stopped as u32,
            0x108 => self.ev_end as u32,
            0x300 => self.intenset, // INTEN reads the enable word
            0x304 => self.intenset,
            0x500 => self.enabled as u32,
            0x504 => self.clkctrl,
            0x508 => self.mode & 3,
            0x518 => self.gainl & 0x7F,
            0x51C => self.gainr & 0x7F,
            0x520 => self.ratio & 1,
            0x540 => self.pselclk,
            0x544 => self.pseldin,
            0x560 => self.sample_ptr,
            0x564 => self.sample_maxcnt,
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => {
                self.ev_started = true;
                self.fire(sys, 1 << 0);
                self.sample_pending = self.sample_maxcnt > 0;
                if !self.sample_pending {
                    self.ev_end = true; // immediate-END timing
                    self.fire(sys, 1 << 2);
                }
            }
            0x004 => {
                self.ev_stopped = true;
                self.fire(sys, 1 << 1);
                self.ev_end = true;
                self.fire(sys, 1 << 2);
                self.sample_pending = false;
            }
            0x100 => if value == 0 { self.ev_started = false; }
            0x104 => if value == 0 { self.ev_stopped = false; }
            0x108 => if value == 0 { self.ev_end = false; }
            0x300 => self.intenset = value & 7, // INTEN absolute
            0x304 => self.intenset |= value & 7,
            0x308 => self.intenset &= !value,
            0x500 => self.enabled = value & 1 == 1,
            0x504 => self.clkctrl = value,
            0x508 => self.mode = value & 3,
            0x518 => self.gainl = value & 0x7F,
            0x51C => self.gainr = value & 0x7F,
            0x520 => self.ratio = value & 1,
            0x540 => self.pselclk = value,
            0x544 => self.pseldin = value,
            0x560 => self.sample_ptr = value,
            0x564 => self.sample_maxcnt = value & 0x7FFF,
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
        sys.p.write(&sys, 0x4001D560, 4, 0x20002000); // SAMPLE.PTR (SVD)
        sys.p.write(&sys, 0x4001D564, 4, 8);          // SAMPLE.MAXCNT (SVD)
        sys.p.write(&sys, 0x4001D500, 4, 1);          // ENABLE
        sys.p.write(&sys, 0x4001D000, 4, 1);          // START
        assert_eq!(sys.p.read(&sys, 0x4001D108, 4), 0, "END waits for driver");
        let t = take_sample(&sys).expect("staged");
        assert_eq!(t, (0x20002000, 8));
        complete_sample(&sys);
        assert_eq!(sys.p.read(&sys, 0x4001D108, 4), 1, "END after complete");
    }
    #[test]
    fn config_psel_inten_irqs() {
        // PDMCLKCTRL/MODE/GAIN/RATIO/PSEL store; INTEN absolute obeys
        // the mask; STARTED/STOPPED/END pend IRQ 29 when enabled.
        let sys = test_dummy_system();
        sys.p.write(&sys, 0xE000E100, 4, 1 << 29); // NVIC ISER: PDM
        sys.p.write(&sys, 0x4001D304, 4, 0xFF);
        assert_eq!(sys.p.read(&sys, 0x4001D300, 4), 7, "INTEN mask");
        sys.p.write(&sys, 0x4001D504, 4, 0x08000000); // PDMCLKCTRL
        sys.p.write(&sys, 0x4001D508, 4, 0x3); // MODE both bits
        sys.p.write(&sys, 0x4001D518, 4, 0x30); // GAINL
        sys.p.write(&sys, 0x4001D51C, 4, 0x2A); // GAINR
        sys.p.write(&sys, 0x4001D520, 4, 1); // RATIO
        sys.p.write(&sys, 0x4001D540, 4, 0x1A); // PSEL.CLK
        sys.p.write(&sys, 0x4001D544, 4, 0x1B); // PSEL.DIN
        assert_eq!(sys.p.read(&sys, 0x4001D504, 4), 0x08000000, "PDMCLKCTRL");
        assert_eq!(sys.p.read(&sys, 0x4001D508, 4), 0x3, "MODE");
        assert_eq!(sys.p.read(&sys, 0x4001D518, 4), 0x30, "GAINL");
        assert_eq!(sys.p.read(&sys, 0x4001D51C, 4), 0x2A, "GAINR");
        assert_eq!(sys.p.read(&sys, 0x4001D520, 4), 1, "RATIO");
        assert_eq!(sys.p.read(&sys, 0x4001D540, 4), 0x1A, "PSEL.CLK");
        assert_eq!(sys.p.read(&sys, 0x4001D544, 4), 0x1B, "PSEL.DIN");
        sys.p.write(&sys, 0x4001D500, 4, 1); // ENABLE
        sys.p.write(&sys, 0x4001D000, 4, 1); // START (MAXCNT 0: instant END)
        assert_eq!(sys.p.read(&sys, 0x4001D100, 4), 1, "STARTED");
        assert_eq!(sys.p.read(&sys, 0x4001D108, 4), 1, "END instant");
        assert!(sys.p.nvic.borrow().has_pending(), "IRQ 29 pends");
        sys.p.write(&sys, 0x4001D004, 4, 1); // STOP
        assert_eq!(sys.p.read(&sys, 0x4001D104, 4), 1, "STOPPED");
        // 2nd run: fresh defaults (mic gains 0x28, pins disconnected).
        let p2 = PdmNrf::default();
        assert_eq!((p2.gainl, p2.gainr, p2.pselclk), (0x28, 0x28, 0xFFFF_FFFF));
    }
}
