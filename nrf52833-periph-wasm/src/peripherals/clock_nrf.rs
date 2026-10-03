use crate::system::System;
use super::Peripheral;

/// CLOCK + POWER combined at 0x40000000 (IRQ 0 POWER_CLOCK; they share the
/// 4KB region on nRF52, SVD-confirmed).
/// CLOCK: TASKS_HFCLKSTART/STOP 0x000/004, LFCLKSTART/STOP 0x008/00C,
///   TASKS_CAL 0x010, TASKS_CTSTART/STOP 0x014/018,
///   EVENTS_HFCLKSTARTED 0x100 / LFCLKSTARTED 0x104 / DONE 0x10C /
///   CTTO 0x110 / CTSTARTED 0x128 / CTSTOPPED 0x12C, INTENSET/CLR
///   0x304/308 (HFCLKSTARTED 0, LFCLKSTARTED 1, DONE 3, CTTO 4,
///   CTSTARTED 10, CTSTOPPED 11 + the POWER bits below — one shared
///   INTEN word on the shared slot), HFCLKRUN 0x408, HFCLKSTAT 0x40C,
///   LFCLKRUN 0x414, LFCLKSTAT 0x418, LFCLKSRCCOPY 0x41C, LFCLKSRC 0x518
///   (RW), HFXODEBOUNCE 0x528 / LFXODEBOUNCE 0x52C / CTIV 0x538 /
///   TRACECONFIG 0x55C (stored).
///   CAL flow: TASKS_CAL latches EVENTS_DONE (+IRQ 3) — LFRC trim
///   converges within the task quantum at this clock granularity.
///   CT flow: TASKS_CTSTART latches CTSTARTED (+IRQ 10) and arms the
///   calibration timer for (CTIV+1) x 0.25 s in virtual instructions
///   (64MHz/32768Hz x 8192 LF ticks per 250 ms); expiry latches CTTO
///   (+IRQ 4, one-shot). TASKS_CTSTOP latches CTSTOPPED (+IRQ 11).
/// POWER: TASKS_CONSTLAT 0x078 / LOWPWR 0x07C (stored mode, no status
///   register on silicon either), EVENTS_POFWARN 0x108 / SLEEPENTER 0x114 /
///   SLEEPEXIT 0x118 / USBDETECTED 0x11C / USBREMOVED 0x120 / USBPWRRDY
///   0x124, INTENSET/CLR bits POFWARN 2 / SLEEPENTER 5 / SLEEPEXIT 6 /
///   USBDETECTED 7 / USBREMOVED 8 / USBPWRRDY 9, RESETREAS 0x400
///   (write-1-clears, SREQ latched on AIRCR SYSRESETREQ via the
///   reset-cause latch), RAMSTATUS 0x428 (=0xF, all blocks on),
///   USBREGSTATUS 0x438 (VBUSDETECT|OUTPUTRDY: host attached),
///   SYSTEMOFF 0x500 (write 1 arms System OFF; the driver polls
///   take_systemoff() to power-cycle, like every take/complete pump),
///   GPREGRET/2 0x51C/520 (retention RW, survives soft reset with the
///   instance), DCDCEN 0x578 + POFCON 0x510 (RW), MAINREGSTATUS 0x640 (=1).
///   POFWARN is host-driven (`pof_warn(present)`: supply vs the POFCON
///   threshold is board analog the model has no handle on — the host
///   owns it, the event/IRQ path is modeled). SLEEPENTER/SLEEPEXIT are
///   host-driven (`notify_sleep_enter/exit`: the WFI/WFE execution that
///   raises them lives in src/cpu/, which board models never touch per
///   AGENTS.md — the event/IRQ latches are modeled, the host publishes
///   the edge).
/// USB policy: DETECTED/PWRRDY latch on read while USBD is enabled
/// (+IRQ when INTENabled, cleared by write-0, re-assert on next read
/// while enabled); USBREMOVED latches on read while USBD is disabled
/// after having been seen enabled.
/// Timing still comes only from INSTRUCTION_COUNT (no second clock).
pub struct ClockPower {
    events_hfclkstarted: bool,
    events_lfclkstarted: bool,
    events_done: bool,
    events_ctto: bool,
    events_ctstarted: bool,
    events_ctstopped: bool,
    events_pofwarn: bool,
    events_sleepenter: bool,
    events_sleepexit: bool,
    events_usbdetected: bool,
    events_usbremoved: bool,
    events_usbpwrrdy: bool,
    hfclk_running: bool,
    lfclk_running: bool,
    lfclksrc: u32,
    intenset: u32,
    gpregret: u32,
    gpregret2: u32,
    dcdcen: u32,
    pofcon: u32,
    hfxodebounce: u32,
    lfxodebounce: u32,
    ctiv: u32,
    traceconfig: u32,
    constlat: bool,
    systemoff_armed: bool,
    usb_seen_powered: bool,
    ct_deadline: Option<u64>,
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
               events_done: false, events_ctto: false,
               events_ctstarted: false, events_ctstopped: false,
               events_pofwarn: false, events_sleepenter: false,
               events_sleepexit: false, events_usbdetected: false,
               events_usbremoved: false, events_usbpwrrdy: false,
               hfclk_running: true, lfclk_running: true, lfclksrc: 0,
               intenset: 0, gpregret: 0xFF, gpregret2: 0xFF,
               dcdcen: 0, pofcon: 0, hfxodebounce: 0, lfxodebounce: 0,
               ctiv: 0, traceconfig: 0, constlat: false,
               systemoff_armed: false, usb_seen_powered: false,
               ct_deadline: None }
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
    /// Calibration-timer pump: expiry latches CTTO (+IRQ 4, one-shot).
    /// Called from read/write/tick (the only points firmware observes).
    fn poll_ct(&mut self, sys: &System) {
        if let Some(d) = self.ct_deadline {
            if crate::system::instruction_count() >= d {
                self.ct_deadline = None;
                self.events_ctto = true;
                self.fire(sys, 1 << 4);
            }
        }
    }
    /// Re-fire any already-set event the firmware just enabled (silicon
    /// pends the IRQ while event && INTEN hold, so enabling INTEN with
    /// a set event pends immediately).
    fn refire(&self, sys: &System, value: u32) {
        for (bit, set) in [
            (0u32, self.events_hfclkstarted),
            (1, self.events_lfclkstarted),
            (2, self.events_pofwarn),
            (3, self.events_done),
            (4, self.events_ctto),
            (5, self.events_sleepenter),
            (6, self.events_sleepexit),
            (7, self.events_usbdetected),
            (8, self.events_usbremoved),
            (9, self.events_usbpwrrdy),
            (10, self.events_ctstarted),
            (11, self.events_ctstopped),
        ] {
            if set && value & (1 << bit) != 0 {
                self.fire(sys, 1 << bit);
            }
        }
    }
    /// Host-attached USB power: true while the USBD peripheral is enabled.
    fn usb_powered(sys: &System) -> bool {
        sys.p.read(sys, 0x4002_7500, 4) & 1 == 1
    }
}

/// Drive the power-fail warning (board analog owned by the host: supply
/// vs the POFCON threshold). true latches EVENTS_POFWARN (+IRQ 2 when
/// INTENabled); false clears the latch.
pub fn pof_warn(sys: &System, present: bool) {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4000_0000 {
            let mut b = match slot.peripheral.try_borrow_mut() {
                Ok(b) => b,
                Err(_) => return,
            };
            if let Some(c) = b.as_any_mut().downcast_mut::<ClockPower>() {
                if present {
                    c.events_pofwarn = true;
                    c.fire(sys, 1 << 2);
                } else {
                    c.events_pofwarn = false;
                }
            }
            return;
        }
    }
}

/// Publish a WFI/WFE sleep edge (the execution lives in src/cpu/, which
/// board models never touch — the host publishes the edge, the
/// event/IRQ latches are modeled). Latches SLEEPENTER (+IRQ 5 when
/// INTENabled).
pub fn notify_sleep_enter(sys: &System) {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4000_0000 {
            let mut b = match slot.peripheral.try_borrow_mut() {
                Ok(b) => b,
                Err(_) => return,
            };
            if let Some(c) = b.as_any_mut().downcast_mut::<ClockPower>() {
                c.events_sleepenter = true;
                c.fire(sys, 1 << 5);
            }
            return;
        }
    }
}

/// Publish the WFI/WFE wake edge (see notify_sleep_enter). Latches
/// SLEEPEXIT (+IRQ 6 when INTENabled).
pub fn notify_sleep_exit(sys: &System) {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4000_0000 {
            let mut b = match slot.peripheral.try_borrow_mut() {
                Ok(b) => b,
                Err(_) => return,
            };
            if let Some(c) = b.as_any_mut().downcast_mut::<ClockPower>() {
                c.events_sleepexit = true;
                c.fire(sys, 1 << 6);
            }
            return;
        }
    }
}

/// Take an armed System OFF request (firmware wrote SYSTEMOFF=1); None
/// when idle. The driver power-cycles the instance, like every
/// take/complete pump.
pub fn take_systemoff(sys: &System) -> bool {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4000_0000 {
            let mut b = match slot.peripheral.try_borrow_mut() {
                Ok(b) => b,
                Err(_) => return false,
            };
            if let Some(c) = b.as_any_mut().downcast_mut::<ClockPower>() {
                if c.systemoff_armed {
                    c.systemoff_armed = false;
                    return true;
                }
            }
            return false;
        }
    }
    false
}

impl Peripheral for ClockPower {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, sys: &System, offset: u32) -> u32 {
        self.poll_ct(sys);
        // USB level-to-edge conversion at observation: DETECTED/PWRRDY
        // latch while USBD is enabled (+IRQ when INTENabled); REMOVED
        // latches when USBD reads disabled after powered.
        if Self::usb_powered(sys) {
            self.usb_seen_powered = true;
            if !self.events_usbdetected {
                self.events_usbdetected = true;
                self.fire(sys, 1 << 7);
            }
            if !self.events_usbpwrrdy {
                self.events_usbpwrrdy = true;
                self.fire(sys, 1 << 9);
            }
        } else if self.usb_seen_powered && !self.events_usbremoved {
            self.events_usbremoved = true;
            self.fire(sys, 1 << 8);
        }
        match offset {
            0x000 | 0x004 | 0x008 | 0x00C
            | 0x010 | 0x014 | 0x018 | 0x078 | 0x07C => 0, // TASKS write-only
            0x100 => self.events_hfclkstarted as u32,
            0x104 => self.events_lfclkstarted as u32,
            0x108 => self.events_pofwarn as u32,
            0x10C => self.events_done as u32,
            0x110 => self.events_ctto as u32,
            0x114 => self.events_sleepenter as u32,
            0x118 => self.events_sleepexit as u32,
            0x11C => self.events_usbdetected as u32,
            0x120 => self.events_usbremoved as u32,
            0x124 => self.events_usbpwrrdy as u32,
            0x128 => self.events_ctstarted as u32,
            0x12C => self.events_ctstopped as u32,
            0x304 => self.intenset,
            0x400 => crate::system::resetreas(),
            0x408 => self.hfclk_running as u32,
            0x40C => if self.hfclk_running { 0x0001_0001 } else { 0 },
            0x414 => self.lfclk_running as u32,
            0x418 => if self.lfclk_running { 0x0001_0000 | (self.lfclksrc & 3) } else { 0 },
            0x41C => self.lfclksrc & 3,
            0x428 => 0xF, // RAMSTATUS: all blocks on
            0x438 => 0x3, // USBREGSTATUS: VBUSDETECT + OUTPUTRDY
            0x500 => self.systemoff_armed as u32,
            0x510 => self.pofcon,
            0x51C => self.gpregret & 0xFF,
            0x520 => self.gpregret2 & 0xFF,
            0x518 => self.lfclksrc,
            0x528 => self.hfxodebounce,
            0x52C => self.lfxodebounce,
            0x538 => self.ctiv,
            0x55C => self.traceconfig,
            0x578 => self.dcdcen & 1,
            0x640 => 1, // MAINREGSTATUS
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        self.poll_ct(sys);
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
            0x010 => {
                // TASKS_CAL: LFRC trim converges in the task quantum.
                if value == 1 {
                    self.events_done = true;
                    self.fire(sys, 1 << 3);
                }
            }
            0x014 => {
                // TASKS_CTSTART: CTSTARTED + arm the cal timer for
                // (CTIV+1) x 0.25 s: 8192 LF ticks per 250 ms at
                // 1953 core instructions per LF tick.
                if value == 1 {
                    self.events_ctstarted = true;
                    self.fire(sys, 1 << 10);
                    self.events_ctto = false;
                    let interval = 8192u64
                        .saturating_mul(1953)
                        .saturating_mul(self.ctiv as u64 + 1);
                    self.ct_deadline = Some(
                        crate::system::instruction_count().wrapping_add(interval),
                    );
                }
            }
            0x018 => {
                if value == 1 {
                    self.ct_deadline = None;
                    self.events_ctstopped = true;
                    self.fire(sys, 1 << 11);
                }
            }
            0x078 => if value == 1 { self.constlat = true; } // CONSTLAT
            0x07C => if value == 1 { self.constlat = false; } // LOWPWR
            0x100 => if value == 0 { self.events_hfclkstarted = false; }
            0x104 => if value == 0 { self.events_lfclkstarted = false; }
            0x108 => if value == 0 { self.events_pofwarn = false; }
            0x10C => if value == 0 { self.events_done = false; }
            0x110 => if value == 0 { self.events_ctto = false; }
            0x114 => if value == 0 { self.events_sleepenter = false; }
            0x118 => if value == 0 { self.events_sleepexit = false; }
            0x11C => if value == 0 { self.events_usbdetected = false; }
            0x120 => if value == 0 { self.events_usbremoved = false; }
            0x124 => if value == 0 { self.events_usbpwrrdy = false; }
            0x128 => if value == 0 { self.events_ctstarted = false; }
            0x12C => if value == 0 { self.events_ctstopped = false; }
            // Combined CLOCK+POWER INTEN (SVD bits 0,1,2,3,4,5,6,7,8,9,10,11).
            0x304 => {
                self.intenset |= value & 0xFFF;
                self.refire(sys, value & 0xFFF);
            }
            0x308 => self.intenset &= !value,
            0x400 => crate::system::resetreas_clear(value),
            0x500 => if value & 1 == 1 { self.systemoff_armed = true; } // SYSTEMOFF
            0x510 => self.pofcon = value,
            0x518 => self.lfclksrc = value & 3,
            0x51C => self.gpregret = value & 0xFF,
            0x520 => self.gpregret2 = value & 0xFF,
            0x528 => self.hfxodebounce = value & 7,
            0x52C => self.lfxodebounce = value & 7,
            0x538 => self.ctiv = value & 0x7F,
            0x55C => self.traceconfig = value & 3,
            0x578 => self.dcdcen = value & 1,
            _ => {}
        }
    }
    fn tick(&mut self, sys: &System) { self.poll_ct(sys); }
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
    #[test]
    fn cal_ct_timer_events_and_irq() {
        // TASKS_CAL latches DONE (+IRQ 3); CTSTART arms the cal timer
        // (+IRQ 10), expiry latches CTTO (+IRQ 4, one-shot); CTSTOP
        // latches CTSTOPPED (+IRQ 11) and disarms. NVIC ISER IRQ 0.
        let _g = crate::system::lock_boot();
        let sys = test_dummy_system();
        sys.p.write(&sys, 0xE000E100, 4, 1); // NVIC ISER: POWER_CLOCK
        sys.p.write(&sys, 0x40000304, 4, (1 << 3) | (1 << 4) | (1 << 10) | (1 << 11));
        sys.p.write(&sys, 0x40000010, 4, 1); // TASKS_CAL
        assert_eq!(sys.p.read(&sys, 0x4000010C, 4), 1, "DONE");
        assert!(sys.p.nvic.borrow().has_pending(), "DONE IRQ 0 pends");
        sys.p.write(&sys, 0x4000010C, 4, 0);
        sys.p.write(&sys, 0x40000538, 4, 0); // CTIV 0: 16M-instr interval
        sys.p.write(&sys, 0x40000014, 4, 1); // TASKS_CTSTART
        assert_eq!(sys.p.read(&sys, 0x40000128, 4), 1, "CTSTARTED");
        assert_eq!(sys.p.read(&sys, 0x40000110, 4), 0, "CTTO not yet");
        crate::system::INSTRUCTION_COUNT.fetch_add(16_000_000 + 8, std::sync::atomic::Ordering::Relaxed);
        sys.tick();
        assert_eq!(sys.p.read(&sys, 0x40000110, 4), 1, "CTTO after interval");
        assert_eq!(sys.p.read(&sys, 0x40000110, 4), 1, "CTTO sticky (one-shot, no repeat)");
        sys.p.write(&sys, 0x40000018, 4, 1); // TASKS_CTSTOP
        assert_eq!(sys.p.read(&sys, 0x4000012C, 4), 1, "CTSTOPPED");
        // Debounce + trace config store.
        sys.p.write(&sys, 0x40000528, 4, 5);
        sys.p.write(&sys, 0x4000052C, 4, 6);
        sys.p.write(&sys, 0x4000055C, 4, 2);
        assert_eq!(sys.p.read(&sys, 0x40000528, 4), 5, "HFXODEBOUNCE");
        assert_eq!(sys.p.read(&sys, 0x4000052C, 4), 6, "LFXODEBOUNCE");
        assert_eq!(sys.p.read(&sys, 0x4000055C, 4), 2, "TRACECONFIG");
        // 2nd run: fresh instance, no leak.
        let c2 = ClockPower::default();
        assert!(!c2.events_done && !c2.events_ctto);
    }
    #[test]
    fn power_mode_systemoff_pof_sleep_usbremove() {
        // CONSTLAT/LOWPWR tasks, SYSTEMOFF arm+take, host POFWARN,
        // sleep edges, USBREMOVED after disable. Live map throughout.
        let sys = test_dummy_system();
        sys.p.write(&sys, 0xE000E100, 4, 1); // NVIC ISER: POWER_CLOCK
        sys.p.write(&sys, 0x40000304, 4, (1 << 2) | (1 << 5) | (1 << 6) | (1 << 8));
        // SYSTEMOFF: write 1 arms, take consumes once.
        sys.p.write(&sys, 0x40000500, 4, 1);
        assert_eq!(sys.p.read(&sys, 0x40000500, 4), 1, "SYSTEMOFF armed reads back");
        assert!(take_systemoff(&sys), "driver takes System OFF");
        assert!(!take_systemoff(&sys), "taken once only");
        // POFWARN host edge + IRQ 2.
        pof_warn(&sys, true);
        assert_eq!(sys.p.read(&sys, 0x40000108, 4), 1, "POFWARN");
        assert!(sys.p.nvic.borrow().has_pending(), "POFWARN IRQ 0 pends");
        pof_warn(&sys, false);
        assert_eq!(sys.p.read(&sys, 0x40000108, 4), 0, "POFWARN cleared by host");
        // Sleep edges + IRQs 5/6.
        notify_sleep_enter(&sys);
        notify_sleep_exit(&sys);
        assert_eq!(sys.p.read(&sys, 0x40000114, 4), 1, "SLEEPENTER");
        assert_eq!(sys.p.read(&sys, 0x40000118, 4), 1, "SLEEPEXIT");
        // USB attach then detach: DETECTED latches, REMOVED after disable.
        sys.p.write(&sys, 0x40027500, 4, 1); // USBD ENABLE
        assert_eq!(sys.p.read(&sys, 0x4000011C, 4), 1, "USBDETECTED");
        sys.p.write(&sys, 0x4000011C, 4, 0);
        assert_eq!(sys.p.read(&sys, 0x4000011C, 4), 1, "re-asserts while powered");
        sys.p.write(&sys, 0x40027500, 4, 0); // USBD disable
        assert_eq!(sys.p.read(&sys, 0x40000120, 4), 1, "USBREMOVED");
        // 2nd run: fresh instance, no leak.
        let c2 = ClockPower::default();
        assert!(!c2.systemoff_armed && !c2.events_pofwarn);
    }
}
