use crate::system::{System, instruction_count};
use super::Peripheral;

/// TIMER0-4, stride 0x1000 from 0x40008000 (IRQ 8+i).
/// Offsets: TASKS_START 0x000, TASKS_STOP 0x004, TASKS_CLEAR 0x00C,
/// TASKS_CAPTURE[n] 0x040+n*4, EVENTS_COMPARE[n] 0x140+n*4,
/// SHORTS 0x200, INTENSET 0x304, INTENCLR 0x308, MODE 0x504,
/// BITMODE 0x508 (0=16b,1=8b,2=24b,3=32b), PRESCALER 0x510,
/// CC[n] 0x540+n*4. Counter driven by INSTRUCTION_COUNT.
pub struct TimerNrf {
    irq: i32,
    running: bool,
    counter: u32,
    cc: [u32; 6],
    ev_compare: [bool; 6],
    prescaler: u32,
    bitmode: u32,
    intenset: u32,
    shorts: u32,
    last_tick: u64,
}

impl TimerNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        let (irq, _) = match name {
            "TIMER0" => (8, 0),
            "TIMER1" => (9, 1),
            "TIMER2" => (10, 2),
            "TIMER3" => (26, 3),
            "TIMER4" => (27, 4),
            _ => return None,
        };
        Some(Box::new(Self {
            irq, running: false, counter: 0, cc: [0; 6],
            ev_compare: [false; 6], prescaler: 0, bitmode: 3,
            intenset: 0, shorts: 0, last_tick: instruction_count(),
        }))
    }
    fn mask(&self) -> u32 {
        match self.bitmode & 3 { 0 => 0xFFFF, 1 => 0xFF, 2 => 0xFF_FFFF, _ => 0xFFFF_FFFF }
    }
    fn advance(&mut self, sys: &System) {
        let now = instruction_count();
        let elapsed = now.wrapping_sub(self.last_tick);
        self.last_tick = now;
        if !self.running || elapsed == 0 { return; }
        // 16 MHz timer from 64 MHz core: 4 core ticks per timer tick at prescaler 0.
        let step = elapsed >> (self.prescaler.min(9) as u64 + 2);
        if step == 0 { return; }
        let mask = self.mask();
        for _ in 0..step.min(1_000_000) {
            self.counter = self.counter.wrapping_add(1) & mask;
            for i in 0..6 {
                if self.counter == (self.cc[i] & mask) {
                    self.ev_compare[i] = true;
                    if self.intenset & (1 << (16 + i)) != 0 {
                        sys.p.nvic.borrow_mut().set_intr_pending(self.irq);
                    }
                    if self.shorts & (1 << i) != 0 { self.counter = 0; } // CLEAR shortcut
                }
            }
        }
    }
}

impl Peripheral for TimerNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, sys: &System, offset: u32) -> u32 {
        self.advance(sys);
        match offset {
            0x140..=0x154 => self.ev_compare[((offset - 0x140) >> 2) as usize] as u32,
            0x200 => self.shorts,
            0x304 => self.intenset,
            0x504 => 0, // MODE timer
            0x508 => self.bitmode,
            0x510 => self.prescaler,
            0x540..=0x554 => self.cc[((offset - 0x540) >> 2) as usize],
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        self.advance(sys);
        match offset {
            0x000 => { self.running = true; self.last_tick = instruction_count(); } // START
            0x004 => self.running = false, // STOP
            0x00C => { self.counter = 0; } // CLEAR
            0x040..=0x054 => {} // CAPTURE: P2 stub
            0x140..=0x154 => if value == 0 { self.ev_compare[((offset - 0x140) >> 2) as usize] = false; }
            0x200 => self.shorts = value & 0x3F,
            0x304 => self.intenset |= value,
            0x308 => self.intenset &= !value,
            0x508 => self.bitmode = value & 3,
            0x510 => self.prescaler = value & 0xF,
            0x540..=0x554 => self.cc[((offset - 0x540) >> 2) as usize] = value,
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
    fn compare_fires_after_cc() {
        let sys = test_dummy_system();
        let mut t = TimerNrf::new("TIMER0").unwrap();
        t.write(&sys, 0x510, 0); // prescaler 0
        t.write(&sys, 0x508, 3); // 32-bit
        t.write(&sys, 0x540, 4); // CC0=4
        t.write(&sys, 0x000, 1); // START
        // pump the virtual clock directly (tick_n equivalent for unit test)
        crate::system::INSTRUCTION_COUNT.fetch_add(64, std::sync::atomic::Ordering::Relaxed);
        t.tick(&sys);
        assert_eq!(t.read(&sys, 0x140), 1, "COMPARE0 set");
        t.write(&sys, 0x140, 0);
        assert_eq!(t.read(&sys, 0x140), 0, "clear by write-0");
    }
}
