use crate::system::{System, instruction_count};
use super::Peripheral;

/// WDT @ 0x40010000 (IRQ 16). TASKS_START 0x000, EVENTS_TIMEOUT 0x100,
/// INTENSET 0x304 (TIMEOUT bit 0) / CLR 0x308, RUNSTATUS 0x400,
/// REQSTATUS 0x404 (all RRs enabled = 0xFF), CRV 0x504 (reload value in
/// 32768 Hz ticks; reset default 0xFFFFFFFF), RREN 0x508 (reload-enable
/// mask), CONFIG 0x50C, RR[0..7] 0x600+n*4 (reload magic 0x6E524635).
/// Once started it cannot be stopped (only reset stops it): on expiry it
/// latches TIMEOUT and requests a system reset through the watchdog
/// channel (the driver reboots; RESETREAS gets DOG+SREQ — DOG set here
/// via the reset-cause path would need plumbing, so SREQ only for now).
/// Timeout in virtual instructions: (CRV+1) * 1953 (64MHz/32768Hz).
pub struct WdtNrf {
    running: bool,
    ev_timeout: bool,
    intenset: u32,
    crv: u32,
    rren: u32,
    deadline: Option<u64>,
    fired: bool,
}

impl Default for WdtNrf {
    fn default() -> Self {
        Self { running: false, ev_timeout: false, intenset: 0,
               crv: 0xFFFF_FFFF, rren: 0, deadline: None, fired: false }
    }
}

impl WdtNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "WDT" { Some(Box::new(Self::default())) } else { None }
    }
    fn period(&self) -> u64 {
        (self.crv as u64 + 1) * 1953
    }
    fn arm(&mut self) {
        if self.running {
            self.deadline = Some(instruction_count().wrapping_add(self.period()));
        }
    }
    fn poll(&mut self, sys: &System) {
        if let Some(d) = self.deadline {
            if !self.fired && instruction_count() >= d {
                self.fired = true;
                self.deadline = None;
                self.ev_timeout = true;
                if self.intenset & 1 != 0 {
                    sys.p.nvic.borrow_mut().set_intr_pending(16);
                }
                // Expiry resets the SoC: same channel as AIRCR SYSRESETREQ
                // (the driver reboots from the vector table).
                crate::system::request_watchdog_reset(0);
            }
        }
    }
}

impl Peripheral for WdtNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, sys: &System, offset: u32) -> u32 {
        self.poll(sys);
        match offset {
            0x100 => self.ev_timeout as u32,
            0x304 => self.intenset,
            0x400 => self.running as u32,
            0x404 => 0xFF,
            0x504 => self.crv,
            0x508 => self.rren,
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        self.poll(sys);
        match offset {
            0x000 => {
                // START is ignored while running — except after an expiry:
                // a watchdog reboot restarts the firmware, which starts the
                // WDT from scratch. Since the test driver reboots via
                // cpu.reset() (peripherals persist), re-arm here reproduces
                // exactly that fresh-boot behavior.
                if !self.running || self.fired {
                    self.running = true;
                    self.fired = false;
                    self.ev_timeout = false;
                    self.arm();
                }
            }
            0x100 => if value == 0 { self.ev_timeout = false; }
            0x304 => self.intenset |= value & 1,
            0x308 => self.intenset &= !value,
            0x504 => {
                if !self.running {
                    self.crv = value;
                }
            }
            0x508 => {
                if !self.running {
                    self.rren = value & 0xFF;
                }
            }
            0x600..=0x61C => {
                // Reload: magic value in an enabled RR restarts the countdown.
                let n = ((offset - 0x600) >> 2) as u32;
                if value == 0x6E52_4635 && self.rren & (1 << n) != 0 {
                    self.fired = false;
                    self.arm();
                }
            }
            _ => {}
        }
    }
    fn tick(&mut self, sys: &System) { self.poll(sys); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn start_and_pet() {
        let sys = test_dummy_system();
        let mut w = WdtNrf::default();
        assert_eq!(w.read(&sys, 0x400), 0);
        w.write(&sys, 0x504, 0xFFFF_FFFF); // CRV: ~long (ignored pre-start anyway)
        w.write(&sys, 0x508, 0x01); // RREN: RR0
        w.write(&sys, 0x000, 1); // START
        assert_eq!(w.read(&sys, 0x400), 1, "RUNSTATUS");
        w.write(&sys, 0x600, 0x6E52_4635); // pet RR0
        assert_eq!(w.read(&sys, 0x100), 0, "no timeout when petted");
    }
    #[test]
    fn expiry_resets() {
        // Shares the process-global reset event with the sysresetreq test.
        let _u = crate::system::lock_uart();
        let sys = test_dummy_system();
        // CRV small: timeout after ~2000 virtual instructions.
        let mut w = WdtNrf::default();
        w.write(&sys, 0x504, 1);
        w.write(&sys, 0x508, 0x01);
        w.write(&sys, 0x000, 1);
        assert!(!crate::system::is_watchdog_reset_requested(), "clean start");
        crate::system::INSTRUCTION_COUNT.fetch_add(100, std::sync::atomic::Ordering::Relaxed);
        w.tick(&sys);
        assert!(!crate::system::is_watchdog_reset_requested(), "not yet");
        crate::system::INSTRUCTION_COUNT.fetch_add(1_000_000, std::sync::atomic::Ordering::Relaxed);
        w.tick(&sys);
        assert_eq!(w.read(&sys, 0x100), 1, "TIMEOUT latched");
        assert!(crate::system::is_watchdog_reset_requested(), "reset requested");
        // Second run: fresh instance, no leak.
        let mut w2 = WdtNrf::default();
        assert_eq!(w2.read(&sys, 0x400), 0);
    }
    #[test]
    fn petting_prevents_expiry() {
        let _u = crate::system::lock_uart();
        let sys = test_dummy_system();
        // Drain any stale reset flag from parallel tests first.
        let _ = crate::system::is_watchdog_reset_requested();
        let mut w = WdtNrf::default();
        w.write(&sys, 0x504, 10);
        w.write(&sys, 0x508, 0x01);
        w.write(&sys, 0x000, 1);
        for _ in 0..10 {
            crate::system::INSTRUCTION_COUNT.fetch_add(5_000, std::sync::atomic::Ordering::Relaxed);
            w.write(&sys, 0x600, 0x6E52_4635); // pet before deadline
            w.tick(&sys);
        }
        assert_eq!(w.read(&sys, 0x100), 0, "never times out while petted");
        assert!(!crate::system::is_watchdog_reset_requested(), "no reset");
    }
}
