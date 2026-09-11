use crate::system::{System, instruction_count};
use super::Peripheral;

/// SysTick (ARM core): CSR 0x00, RVR 0x04, CVR 0x08, CALIB 0x0C.
/// The down-counter runs on the core clock while ENABLE=1 and RELOAD!=0:
/// CVR advances on every access (INSTRUCTION_COUNT-partitioned, same rule
/// as TIMER/RTC), COUNTFLAG (CSR bit 16) latches on wrap and clears on CSR
/// read. Firmware doing VAL-delta timing (MicroPython's busy-wait hangs
/// forever without this — found 2026-09-11) needs a real counter, not the
/// old fixed-value stub.
pub struct SysTick {
    csr: u32,
    rvr: u32,
    cvr: u32,
    calib: u32,
    countflag: bool,
    last_tick: u64,
}

impl SysTick {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "SysTick" || name == "STK" {
            Some(Box::new(Self { csr: 0, rvr: 0, cvr: 0, calib: 0, countflag: false, last_tick: instruction_count() }))
        } else {
            None
        }
    }
    fn advance(&mut self) {
        let now = instruction_count();
        let elapsed = now.wrapping_sub(self.last_tick);
        self.last_tick = now;
        if self.csr & 1 == 0 || self.rvr == 0 || elapsed == 0 {
            return;
        }
        // Down-count; each pass through 0 latches COUNTFLAG and reloads.
        let mut down = self.cvr as u64;
        let mut left = elapsed;
        while left > 0 {
            if down == 0 {
                down = self.rvr as u64;
                self.countflag = true;
            }
            let step = down.min(left);
            down -= step;
            left -= step;
            if down == 0 && left > 0 {
                // exact hit on 0 with time left: wrap now so COUNTFLAG is
                // visible even when elapsed lands exactly on the boundary.
                self.countflag = true;
                down = self.rvr as u64;
            }
        }
        self.cvr = down as u32;
    }
}

impl Peripheral for SysTick {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, sys: &System, offset: u32) -> u32 {
        self.advance();
        match offset {
            0x00 => {
                let v = self.csr | ((self.countflag as u32) << 16);
                self.countflag = false; // COUNTFLAG clears on CSR read
                let _ = sys;
                v
            }
            0x04 => self.rvr,
            0x08 => self.cvr,
            0x0C => self.calib,
            _ => 0,
        }
    }

    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        self.advance();
        match offset {
            0x00 => {
                self.csr = value & 0x10007;
                if value & 1 != 0 && self.rvr != 0 {
                    sys.p.nvic.borrow_mut().systick_period = Some(self.rvr);
                } else {
                    sys.p.nvic.borrow_mut().systick_period = None;
                }
            }
            0x04 => self.rvr = value & 0x00FF_FFFF,
            0x08 => {
                self.cvr = 0;
                self.countflag = false;
                sys.p.nvic.borrow_mut().last_systick_trigger = crate::system::INSTRUCTION_COUNT.load(std::sync::atomic::Ordering::Relaxed);
            }
            0x0C => {}
            _ => {}
        }
    }
    fn tick(&mut self, _sys: &System) { self.advance(); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn val_counts_down_and_flags() {
        let sys = test_dummy_system();
        let mut s = SysTick::new("SysTick").unwrap();
        s.write(&sys, 0x04, 1000); // RVR
        s.write(&sys, 0x08, 0);    // CVR clear
        s.write(&sys, 0x00, 1);    // ENABLE
        // Enabled VAL moves 1/instr (direction tolerant: parallel tests also
        // advance the shared clock; exact deltas can't be asserted).
        let va = s.read(&sys, 0x08);
        crate::system::INSTRUCTION_COUNT.fetch_add(100, std::sync::atomic::Ordering::Relaxed);
        let vb = s.read(&sys, 0x08);
        assert!(vb != va || vb > 900, "VAL moves: {va} -> {vb}");
        // Wrap latches COUNTFLAG, cleared by CSR read.
        crate::system::INSTRUCTION_COUNT.fetch_add(100_000, std::sync::atomic::Ordering::Relaxed);
        s.read(&sys, 0x08);
        assert_ne!(s.read(&sys, 0x00) & (1 << 16), 0, "COUNTFLAG latched");
        assert_eq!(s.read(&sys, 0x00) & (1 << 16), 0, "COUNTFLAG clears on read");
    }
    #[test]
    fn disabled_counter_holds() {
        let sys = test_dummy_system();
        let mut s = SysTick::new("SysTick").unwrap();
        s.write(&sys, 0x04, 1000);
        s.write(&sys, 0x08, 0);
        crate::system::INSTRUCTION_COUNT.fetch_add(500, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(s.read(&sys, 0x08), 0, "disabled: VAL holds");
    }
}
