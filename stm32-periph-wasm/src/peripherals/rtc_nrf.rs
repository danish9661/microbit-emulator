use crate::system::{System, instruction_count};
use super::Peripheral;

/// RTC0-2 (RTC0 0x4000B000, RTC1 0x40011000, RTC2 0x4000D000? — 52833:
/// RTC0 0x4000B000, RTC1 0x40011000, RTC2 0x40024000).
/// Offsets: TASKS_START 0x000, TASKS_STOP 0x004, TASKS_CLEAR 0x00C,
/// EVENTS_TICK 0x100, EVENTS_OVRFLW 0x104, EVENTS_COMPARE[n] 0x140+n*4,
/// INTENSET 0x304, INTENCLR 0x308, COUNTER 0x504, PRESCALER 0x508,
/// CC[n] 0x540+n*4. 32768 Hz LFCLK: counter += elapsed/1953/(presc+1).
pub struct RtcNrf {
    irq: i32,
    running: bool,
    counter: u32, // 24-bit
    prescaler: u32,
    cc: [u32; 4],
    ev_tick: bool,
    ev_ovrflw: bool,
    ev_compare: [bool; 4],
    intenset: u32,
    last_tick: u64,
    frac: u64,
}

impl RtcNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        let irq = match name {
            "RTC0" => 11,
            "RTC1" => 17,
            "RTC2" => 41,
            _ => return None,
        };
        Some(Box::new(Self {
            irq, running: false, counter: 0, prescaler: 0, cc: [0; 4],
            ev_tick: false, ev_ovrflw: false, ev_compare: [false; 4],
            intenset: 0, last_tick: instruction_count(), frac: 0,
        }))
    }
    fn advance(&mut self, sys: &System) {
        let now = instruction_count();
        let elapsed = now.wrapping_sub(self.last_tick);
        self.last_tick = now;
        if !self.running || elapsed == 0 { return; }
        // 64MHz/32768 = 1953 core ticks per LF tick at prescaler 0.
        let div = 1953u64 * (self.prescaler as u64 + 1);
        let total = self.frac + elapsed as u64;
        let steps = total / div;
        self.frac = total % div;
        for _ in 0..steps.min(100_000) {
            self.counter = (self.counter + 1) & 0xFF_FFFF;
            if self.counter == 0 { self.ev_ovrflw = true; }
            self.ev_tick = true;
            for i in 0..4 {
                if self.counter == (self.cc[i] & 0xFF_FFFF) {
                    self.ev_compare[i] = true;
                    if self.intenset & (1 << (16 + i)) != 0 {
                        sys.p.nvic.borrow_mut().set_intr_pending(self.irq);
                    }
                }
            }
            if self.ev_tick && self.intenset & 1 != 0 {
                sys.p.nvic.borrow_mut().set_intr_pending(self.irq);
            }
        }
    }
}

impl Peripheral for RtcNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, sys: &System, offset: u32) -> u32 {
        self.advance(sys);
        match offset {
            0x100 => self.ev_tick as u32,
            0x104 => self.ev_ovrflw as u32,
            0x140..=0x14C => self.ev_compare[((offset - 0x140) >> 2) as usize] as u32,
            0x304 => self.intenset,
            0x504 => self.counter,
            0x508 => self.prescaler,
            0x540..=0x54C => self.cc[((offset - 0x540) >> 2) as usize],
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        self.advance(sys);
        match offset {
            0x000 => { self.running = true; self.last_tick = instruction_count(); }
            0x004 => self.running = false,
            0x00C => { self.counter = 0; self.frac = 0; }
            0x100 => if value == 0 { self.ev_tick = false; }
            0x104 => if value == 0 { self.ev_ovrflw = false; }
            0x140..=0x14C => if value == 0 { self.ev_compare[((offset - 0x140) >> 2) as usize] = false; }
            0x304 => self.intenset |= value,
            0x308 => self.intenset &= !value,
            0x508 => self.prescaler = value & 0xFFF,
            0x540..=0x54C => self.cc[((offset - 0x540) >> 2) as usize] = value & 0xFF_FFFF,
            _ => {}
        }
    }
    fn tick(&mut self, sys: &System) { self.advance(sys); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn tick_sets_event() {
        let sys = test_dummy_system();
        let mut r = RtcNrf::new("RTC0").unwrap();
        r.write(&sys, 0x508, 0);
        r.write(&sys, 0x000, 1);
        crate::system::INSTRUCTION_COUNT.fetch_add(4000, std::sync::atomic::Ordering::Relaxed);
        r.tick(&sys);
        assert_eq!(r.read(&sys, 0x100), 1, "TICK set");
        assert!(r.read(&sys, 0x504) >= 1, "counter advanced");
    }
}
