use crate::system::System;
use super::Peripheral;

/// CLOCK + POWER combined at 0x40000000 (IRQ 0 POWER_CLOCK; they share the
/// 4KB region on nRF52, SVD-confirmed).
/// CLOCK: TASKS_HFCLKSTART/STOP 0x000/004, LFCLKSTART/STOP 0x008/00C,
///   EVENTS_HFCLKSTARTED 0x100 / LFCLKSTARTED 0x104, INTENSET/CLR
///   0x304/308, HFCLKRUN 0x408, HFCLKSTAT 0x40C, LFCLKRUN 0x414,
///   LFCLKSTAT 0x418, LFCLKSRC 0x518 (RW).
/// POWER: TASKS_CONSTLAT/LOWPWR 0x078/07C, EVENTS_USBDETECTED 0x11C /
///   USBREMOVED 0x120 / USBPWRRDY 0x124, RESETREAS 0x400 (write-1-clears,
///   SREQ latched on AIRCR SYSRESETREQ via the reset-cause latch),
///   RAMSTATUS 0x428 (=0xF, all blocks on), USBREGSTATUS 0x438
///   (VBUSDETECT|OUTPUTRDY: host attached), GPREGRET/2 0x51C/520 (retention
///   RW, survives soft reset with the instance), DCDCEN 0x578 + POFCON
///   0x510 (RW stubs), MAINREGSTATUS 0x640 (=1).
/// USB policy: DETECTED/PWRRDY read 1 while USBD is enabled (firmware
/// gates TinyUSB attach on them); EVENTS clear by write-0 and re-assert
/// on next read while enabled (level, like the READY stub).
/// Timing still comes only from INSTRUCTION_COUNT (no second clock).
pub struct ClockPower {
    events_hfclkstarted: bool,
    events_lfclkstarted: bool,
    hfclk_running: bool,
    lfclk_running: bool,
    lfclksrc: u32,
    intenset: u32,
    gpregret: u32,
    gpregret2: u32,
    dcdcen: u32,
    pofcon: u32,
}

impl Default for ClockPower {
    fn default() -> Self {
        // Silicon boot state: HFINT (not HFCLK) runs the core out of
        // reset, but our model has a single clock domain — firmware
        // that polls EVENTS_HFCLKSTARTED after TASKS_HFCLKSTART would
        // spin ~ms waiting for crystal ramp that we model as instant.
        // Boot WITH both clocks already running + events set (matches
        // post-ramp silicon; firmware clears events by write-0 and
        // re-arms via TASKS when it actually needs an edge).
        // (P55: native MPY banner run parked at 0x200021b8/bb with
        // hf_started=0 — the 0x20980 delay loop spins on the STARTED
        // event that only a TASKS write would set. HFCLKRUN/STAT alone
        // don't satisfy an event poll.)
        Self { events_hfclkstarted: true, events_lfclkstarted: true,
               hfclk_running: true, lfclk_running: true, lfclksrc: 0,
               intenset: 0, gpregret: 0xFF, gpregret2: 0xFF,
               dcdcen: 0, pofcon: 0 }
    }
}

impl ClockPower {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "CLOCK" || name == "POWER" || name == "CLOCK_POWER" {
            Some(Box::new(Self::default()))
        } else {
            None
        }
    }
    fn fire(&self, sys: &System, bit: u32) {
        if self.intenset & bit != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(0);
        }
    }
    /// Host-attached USB power: true while the USBD peripheral is enabled.
    fn usb_powered(sys: &System) -> bool {
        sys.p.read(sys, 0x4002_7500, 4) & 1 == 1
    }
}

impl Peripheral for ClockPower {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, sys: &System, offset: u32) -> u32 {
        match offset {
            0x000 | 0x004 | 0x008 | 0x00C => 0, // TASKS are write-only
            0x100 => self.events_hfclkstarted as u32,
            0x104 => self.events_lfclkstarted as u32,
            0x11C => Self::usb_powered(sys) as u32, // USBDETECTED
            0x120 => 0,                             // USBREMOVED
            0x124 => Self::usb_powered(sys) as u32, // USBPWRRDY
            0x304 => self.intenset,
            0x400 => crate::system::resetreas(),
            0x408 => self.hfclk_running as u32,
            0x40C => if self.hfclk_running { 0x0001_0001 } else { 0 },
            0x414 => self.lfclk_running as u32,
            0x418 => if self.lfclk_running { 0x0001_0000 | (self.lfclksrc & 3) } else { 0 },
            0x41C => self.lfclksrc & 3,
            0x428 => 0xF, // RAMSTATUS: all blocks on
            0x438 => 0x3, // USBREGSTATUS: VBUSDETECT + OUTPUTRDY
            0x510 => self.pofcon,
            0x51C => self.gpregret & 0xFF,
            0x520 => self.gpregret2 & 0xFF,
            0x518 => self.lfclksrc,
            0x578 => self.dcdcen & 1,
            0x640 => 1, // MAINREGSTATUS
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => {
                if value == 1 {
                    self.hfclk_running = true;
                    self.events_hfclkstarted = true;
                    self.fire(sys, 1 << 0);
                }
            }
            0x004 => if value == 1 { self.hfclk_running = false; }
            0x008 => {
                if value == 1 {
                    self.lfclk_running = true;
                    self.events_lfclkstarted = true;
                    self.fire(sys, 1 << 1);
                }
            }
            0x00C => if value == 1 { self.lfclk_running = false; }
            0x100 => if value == 0 { self.events_hfclkstarted = false; }
            0x104 => if value == 0 { self.events_lfclkstarted = false; }
            0x11C | 0x120 | 0x124 => {} // level events: clear is a no-op while powered
            0x304 => self.intenset |= value & 0x3,
            0x308 => self.intenset &= !value,
            0x400 => crate::system::resetreas_clear(value),
            0x510 => self.pofcon = value,
            0x518 => self.lfclksrc = value & 3,
            0x51C => self.gpregret = value & 0xFF,
            0x520 => self.gpregret2 = value & 0xFF,
            0x578 => self.dcdcen = value & 1,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn hfclk_start_sets_event() {
        let sys = test_dummy_system();
        // Boot state: clocks already running + events set (P55).
        let mut c = ClockPower::default();
        assert_eq!(c.read(&sys, 0x100), 1, "HFCLKSTARTED set at boot");
        assert_eq!(c.read(&sys, 0x408), 1, "HFCLKRUN at boot");
        assert_eq!(c.read(&sys, 0x104), 1, "LFCLKSTARTED set at boot");
        c.write(&sys, 0x100, 0); // EVENTS clear by write-0
        assert_eq!(c.read(&sys, 0x100), 0);
        // Re-arm via TASKS still works (firmware that needs an edge).
        c.write(&sys, 0x000, 1);
        assert_eq!(c.read(&sys, 0x100), 1);
        assert_eq!(c.read(&sys, 0x408), 1);
    }
    #[test]
    fn power_usb_and_resetreas() {
        let sys = test_dummy_system();
        // No USBD enabled yet: no power events.
        assert_eq!(sys.p.read(&sys, 0x40000124, 4), 0, "USBPWRRDY gated on USBD");
        assert_eq!(sys.p.read(&sys, 0x40000438, 4) & 3, 3, "VBUS present");
        // Enable USBD -> power events assert (level).
        sys.p.write(&sys, 0x40027500, 4, 1);
        assert_eq!(sys.p.read(&sys, 0x40000124, 4), 1, "USBPWRRDY");
        assert_eq!(sys.p.read(&sys, 0x4000011C, 4), 1, "USBDETECTED");
        // RESETREAS latches SREQ on watchdog/AIRCR reset, clears by write-1.
        // (Clear-first: the latch is process-global by design — retained
        // across reboots for MBR/SD — so never assume its initial state.)
        sys.p.write(&sys, 0x40000400, 4, 0xFFFF_FFFF);
        assert_eq!(sys.p.read(&sys, 0x40000400, 4), 0);
        crate::system::request_watchdog_reset(0);
        assert_eq!(sys.p.read(&sys, 0x40000400, 4) & (1 << 2), 1 << 2, "SREQ");
        crate::system::is_watchdog_reset_requested();
        sys.p.write(&sys, 0x40000400, 4, 1 << 2);
        assert_eq!(sys.p.read(&sys, 0x40000400, 4) & (1 << 2), 0, "cleared");
        // GPREGRET retention storage.
        sys.p.write(&sys, 0x4000051C, 4, 0xAB);
        assert_eq!(sys.p.read(&sys, 0x4000051C, 4), 0xAB);
    }
}
