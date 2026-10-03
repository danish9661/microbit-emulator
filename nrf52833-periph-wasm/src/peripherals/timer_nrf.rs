use crate::system::{System, instruction_count};
use super::Peripheral;

/// TIMER0-4, stride 0x1000 from 0x40008000 (IRQ 8+i).
/// Offsets: TASKS_START 0x000, TASKS_STOP 0x004, TASKS_COUNT 0x008
/// (counter mode increment), TASKS_CLEAR 0x00C, TASKS_SHUTDOWN 0x010
/// (deprecated: behaves as STOP), TASKS_CAPTURE[n] 0x040+n*4,
/// EVENTS_COMPARE[n] 0x140+n*4, SHORTS 0x200 (COMPAREn_CLEAR bits 0-5,
/// COMPAREn_STOP bits 8-13), INTENSET 0x304 / CLR 0x308 (COMPAREn bits
/// 16-21), MODE 0x504 (0 Timer, 1 Counter, 2 LowPowerCounter),
/// BITMODE 0x508 (0=16b,1=8b,2=24b,3=32b), PRESCALER 0x510,
/// CC[n] 0x540+n*4. Counter driven by INSTRUCTION_COUNT in Timer mode
/// (16 MHz from the 64 MHz core: 4 core ticks per timer tick at
/// prescaler 0); in Counter mode only TASKS_COUNT advances it (silicon
/// counts an external pin — the host owns that edge, the task latch is
/// modeled). LowPowerCounter runs the same time base (documented: the
/// LFCLK-vs-HFCLK source difference is below this clock granularity).
pub struct TimerNrf {
    irq: i32,
    running: bool,
    counter: u32,
    cc: [u32; 6],
    ev_compare: [bool; 6],
    prescaler: u32,
    bitmode: u32,
    mode: u32,
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
            ev_compare: [false; 6], prescaler: 0, bitmode: 3, mode: 0,
            intenset: 0, shorts: 0, last_tick: instruction_count(),
        }))
    }
    fn mask(&self) -> u32 {
        match self.bitmode & 3 { 0 => 0xFFFF, 1 => 0xFF, 2 => 0xFF_FFFF, _ => 0xFFFF_FFFF }
    }
    /// Compare-match handling shared by the time pump and TASKS_COUNT:
    /// latch the event, pend the IRQ when INTENabled, then consume the
    /// SHORTS (CLEAR restarts the count, STOP halts the timer).
    fn check_compare(&mut self, sys: &System) {
        let mask = self.mask();
        for i in 0..6 {
            if self.counter == (self.cc[i] & mask) {
                self.ev_compare[i] = true;
                if self.intenset & (1 << (16 + i)) != 0 {
                    sys.p.nvic.borrow_mut().set_intr_pending(self.irq);
                }
                if self.shorts & (1 << i) != 0 { self.counter = 0; } // CLEAR shortcut
                if self.shorts & (1 << (8 + i)) != 0 { self.running = false; } // STOP shortcut
            }
        }
    }
    fn advance(&mut self, sys: &System) {
        let now = instruction_count();
        let elapsed = now.wrapping_sub(self.last_tick);
        self.last_tick = now;
        // Counter mode: time does not advance the count (TASKS_COUNT owns it).
        if !self.running || elapsed == 0 || self.mode == 1 { return; }
        // 16 MHz timer from 64 MHz core: 4 core ticks per timer tick at prescaler 0.
        let step = elapsed >> (self.prescaler.min(9) as u64 + 2);
        if step == 0 { return; }
        let mask = self.mask();
        for _ in 0..step.min(1_000_000) {
            self.counter = self.counter.wrapping_add(1) & mask;
            self.check_compare(sys);
            if !self.running { break; } // STOP shortcut halted mid-pump
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
            0x504 => self.mode,
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
            0x004 | 0x010 => self.running = false, // STOP (+deprecated SHUTDOWN)
            0x008 => {
                // TASKS_COUNT: counter-mode increment (one count per task).
                self.counter = self.counter.wrapping_add(1) & self.mask();
                self.check_compare(sys);
            }
            0x00C => { self.counter = 0; } // CLEAR
            0x040..=0x054 => {
                // TASKS_CAPTURE[n]: snapshot COUNTER into CC[n]. This is how
                // firmware READS a running timer (no COUNTER register exists);
                // MicroPython's tick hangs forever without it (found 2026-09-11).
                let n = ((offset - 0x040) >> 2) as usize;
                self.cc[n] = self.counter;
            }
            0x140..=0x154 => if value == 0 { self.ev_compare[((offset - 0x140) >> 2) as usize] = false; }
            0x200 => self.shorts = value & 0x3F3F, // CLEAR 0-5 + STOP 8-13
            0x304 => self.intenset |= value & 0x3F_0000, // COMPAREn 16-21
            0x308 => self.intenset &= !value,
            0x504 => self.mode = value & 7,
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
    use crate::system::{lock_boot, test_dummy_system};
    #[test]
    fn compare_fires_after_cc() {
        let _g = lock_boot();
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
    #[test]
    fn capture_snapshots_counter() {
        let _g = lock_boot();
        let sys = test_dummy_system();
        let mut t = TimerNrf::new("TIMER0").unwrap();
        t.write(&sys, 0x510, 0);
        t.write(&sys, 0x508, 3);
        t.write(&sys, 0x000, 1); // START
        crate::system::INSTRUCTION_COUNT.fetch_add(64, std::sync::atomic::Ordering::Relaxed);
        t.write(&sys, 0x04C, 1); // CAPTURE[3]
        let c3 = t.read(&sys, 0x54C);
        assert!(c3 > 0, "captured running counter, got {c3}");
        crate::system::INSTRUCTION_COUNT.fetch_add(64, std::sync::atomic::Ordering::Relaxed);
        t.write(&sys, 0x04C, 1);
        assert!(t.read(&sys, 0x54C) > c3, "counter advances between captures");
    }
    #[test]
    fn count_task_counter_mode_and_stop_short() {
        // Counter mode (MODE=1): time does not advance the count;
        // TASKS_COUNT increments one per task through COMPARE0, whose
        // STOP short halts the timer. SHUTDOWN behaves as STOP.
        let _g = lock_boot();
        let sys = test_dummy_system();
        let mut t = TimerNrf::new("TIMER0").unwrap();
        t.write(&sys, 0x504, 1); // MODE Counter
        assert_eq!(t.read(&sys, 0x504), 1, "MODE reads back");
        t.write(&sys, 0x540, 3); // CC0=3
        t.write(&sys, 0x200, 1 << 8); // SHORTS COMPARE0_STOP
        t.write(&sys, 0x000, 1); // START
        crate::system::INSTRUCTION_COUNT.fetch_add(1_000_000, std::sync::atomic::Ordering::Relaxed);
        t.tick(&sys);
        t.write(&sys, 0x04C, 1); // CAPTURE[3]: still 0 in counter mode
        assert_eq!(t.read(&sys, 0x54C), 0, "time does not advance counter mode");
        t.write(&sys, 0x008, 1); // COUNT x3
        t.write(&sys, 0x008, 1);
        t.write(&sys, 0x008, 1);
        assert_eq!(t.read(&sys, 0x140), 1, "COMPARE0 on 3rd count");
        t.write(&sys, 0x04C, 1);
        assert_eq!(t.read(&sys, 0x54C), 3, "counted to 3");
        // STOP short fired: further COUNT tasks still count (silicon
        // keeps counting only while running — halted here), timer halted.
        t.write(&sys, 0x000, 1); // START again
        t.write(&sys, 0x010, 1); // SHUTDOWN = STOP
        crate::system::INSTRUCTION_COUNT.fetch_add(1_000_000, std::sync::atomic::Ordering::Relaxed);
        t.tick(&sys);
        t.write(&sys, 0x04C, 1);
        assert_eq!(t.read(&sys, 0x54C), 3, "halted after SHUTDOWN");
        // Timer mode still advances with time (no regression).
        t.write(&sys, 0x504, 0); // MODE Timer
        t.write(&sys, 0x000, 1);
        crate::system::INSTRUCTION_COUNT.fetch_add(64, std::sync::atomic::Ordering::Relaxed);
        t.tick(&sys);
        t.write(&sys, 0x04C, 1);
        assert!(t.read(&sys, 0x54C) > 3, "timer mode advances");
    }
}
