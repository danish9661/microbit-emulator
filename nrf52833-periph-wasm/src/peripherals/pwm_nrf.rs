use crate::system::System;
use super::Peripheral;

/// PWM0 0x4001C000 / PWM1 0x40021000 / PWM2 0x40022000 / PWM3 0x4002D000
/// (IRQs 28/33/34/45 per nrf52833.svd). P4 subset:
///   TASKS_STOP 0x004, TASKS_SEQSTART[n] 0x008+n*4,
///   EVENTS_STOPPED 0x104, EVENTS_SEQSTARTED[n] 0x108+n*4,
///   EVENTS_SEQEND[n] 0x110+n*4, EVENTS_PWMPERIODEND 0x120,
///   EVENTS_LOOPSDONE 0x124, SHORTS 0x200, INTENSET 0x304/CLR 0x308,
///   ENABLE 0x500, MODE 0x504, COUNTERTOP 0x508, PRESCALER 0x50C,
///   DECODER 0x510, LOOP 0x514. SEQSTART chains SEQSTARTED->SEQEND->
///   (LOOPSDONE) ->STOPPED immediately (speaker waveform DMA is P5).
pub struct PwmNrf {
    irq: i32,
    enabled: bool,
    ev_stopped: bool,
    ev_seqstarted: [bool; 2],
    ev_seqend: [bool; 2],
    ev_periodend: bool,
    ev_loopsdone: bool,
    intenset: u32,
}

impl PwmNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        let irq = match name {
            "PWM0" => 28,
            "PWM1" => 33,
            "PWM2" => 34,
            "PWM3" => 45,
            _ => return None,
        };
        Some(Box::new(Self {
            irq, enabled: false, ev_stopped: false,
            ev_seqstarted: [false; 2], ev_seqend: [false; 2],
            ev_periodend: false, ev_loopsdone: false, intenset: 0,
        }))
    }
    fn fire(&self, sys: &System, bit: u32) {
        if self.intenset & bit != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(self.irq);
        }
    }
}

impl Peripheral for PwmNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x104 => self.ev_stopped as u32,
            0x108 | 0x10C => self.ev_seqstarted[((offset - 0x108) >> 2) as usize] as u32,
            0x110 | 0x114 => self.ev_seqend[((offset - 0x110) >> 2) as usize] as u32,
            0x120 => self.ev_periodend as u32,
            0x124 => self.ev_loopsdone as u32,
            0x304 => self.intenset,
            0x500 => self.enabled as u32,
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x004 => { self.ev_stopped = true; self.fire(sys, 1 << 1); }
            0x008 | 0x00C => {
                let n = ((offset - 0x008) >> 2) as usize;
                self.ev_seqstarted[n] = true;
                self.ev_seqend[n] = true;
                self.ev_loopsdone = true;
                self.fire(sys, 1 << (2 + n));
            }
            0x104 => if value == 0 { self.ev_stopped = false; }
            0x108 | 0x10C => if value == 0 { self.ev_seqstarted[((offset - 0x108) >> 2) as usize] = false; }
            0x110 | 0x114 => if value == 0 { self.ev_seqend[((offset - 0x110) >> 2) as usize] = false; }
            0x124 => if value == 0 { self.ev_loopsdone = false; }
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
    fn seqstart_chains_to_end() {
        let sys = test_dummy_system();
        let mut p = PwmNrf::new("PWM0").unwrap();
        p.write(&sys, 0x500, 1);
        p.write(&sys, 0x008, 1);
        assert_eq!(p.read(&sys, 0x108), 1);
        assert_eq!(p.read(&sys, 0x110), 1);
    }
    #[test]
    fn stop_fires_irq_gated_and_second_instance() {
        // STOP path: EVENTS_STOPPED + IRQ only when INTEN bit 1 set;
        // clear by write-0. Exercises PWM1 (IRQ 33) — the first test
        // only covers PWM0 events without the NVIC path.
        let sys = test_dummy_system();
        sys.p.write(&sys, 0xE000E104, 4, 1 << (33 - 32)); // NVIC ISER1: PWM1
        let mut p = PwmNrf::new("PWM1").unwrap();
        p.write(&sys, 0x500, 1);
        // No INTEN yet: STOP sets the event but must not pend the IRQ.
        p.write(&sys, 0x004, 1);
        assert_eq!(p.read(&sys, 0x104), 1, "STOPPED event");
        assert!(!sys.p.nvic.borrow().has_pending(), "no IRQ without INTEN");
        p.write(&sys, 0x104, 0);
        assert_eq!(p.read(&sys, 0x104), 0, "clear by write-0");
        // With INTEN STOPPED (bit 1): STOP pends IRQ 33.
        p.write(&sys, 0x304, 1 << 1);
        p.write(&sys, 0x004, 1);
        assert!(sys.p.nvic.borrow().has_pending(), "STOPPED IRQ 33 pends");
        // SEQSTART1 on the same instance chains its own pair.
        p.write(&sys, 0x00C, 1);
        assert_eq!(p.read(&sys, 0x10C), 1, "SEQSTARTED1");
        assert_eq!(p.read(&sys, 0x114), 1, "SEQEND1");
    }
}
