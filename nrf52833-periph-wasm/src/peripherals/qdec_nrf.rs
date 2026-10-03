use crate::system::System;
use super::Peripheral;

/// QDEC @ 0x40012000 (IRQ 18, quadrature decoder, edge connector P13-15).
/// Offsets from nrf52833.svd (note: STOPPED is 0x110 and SAMPLE is 0x50C):
/// TASKS_START 0x000, TASKS_STOP 0x004, TASKS_READCLRACC 0x008
/// (ACCREAD <- ACC, ACC <- 0), TASKS_RDCLRACC 0x00C (ACCREAD <- ACCDBL,
/// ACC <- 0), TASKS_RDCLRDBL 0x010 (ACCDBLREAD <- ACCDBL, ACCDBL <- 0,
/// raises DBLRDY), EVENTS_SAMPLERDY 0x100, EVENTS_REPORTRDY 0x104,
/// EVENTS_ACCOF 0x108, EVENTS_DBLRDY 0x10C, EVENTS_STOPPED 0x110,
/// SHORTS 0x200 (REPORTRDY_READCLRACC 0, SAMPLERDY_STOP 1,
/// REPORTRDY_RDCLRACC 2, REPORTRDY_STOP 3, DBLRDY_RDCLRDBL 4,
/// DBLRDY_STOP 5, SAMPLERDY_READCLRACC 6), INTENSET 0x304 (SAMPLERDY 0,
/// REPORTRDY 1, ACCOF 2, DBLRDY 3, STOPPED 4) / CLR 0x308, ENABLE 0x500,
/// LEDPOL 0x504, SAMPLEPER 0x508 (128us units), SAMPLE 0x50C,
/// REPORTPER 0x510, ACC 0x514, ACCREAD 0x518, PSELLED 0x51C, PSELA 0x520,
/// PSELB 0x524, DBFEN 0x528, LEDPRE 0x540, ACCDBL 0x544,
/// ACCDBLREAD 0x548.
///
/// Protocol: START begins sampling every (SAMPLEPER+1)*8192 instructions.
/// Each sample reads PSELA/PSELB GPIO levels (when connected, i.e. not
/// `0xFFFFFFFF`) and Gray-decodes transitions into the accumulator
/// (`00->01->11->10` = +1 step each edge); the double-read accumulator
/// ACCDBL mirrors the same edges for atomic reads. With DBFEN set, a new
/// level pair must read stable twice before it counts (documented
/// simplification of the hardware debounce filter). The host can also
/// drive steps directly via `qdec_step` (JS knob/encoder parts).
/// SAMPLERDY fires per sample (+IRQ); when `samples_since_report`
/// reaches REPORTPER (>0), REPORTRDY fires. ACCOF fires on true i32
/// accumulator wrap (checked_add, shared by ACC and ACCDBL). SHORTS
/// consume inline at the event quantum (single pass, no recursion):
/// REPORTRDY/SAMPLERDY auto-read ACC or ACCDBL into ACCREAD, the _STOP
/// shorts halt with STOPPED, DBLRDY_RDCLRDBL snapshots ACCDBL into
/// ACCDBLREAD.
pub struct QdecNrf {
    enabled: bool,
    running: bool,
    ev_samplerdy: bool,
    ev_reportrdy: bool,
    ev_accof: bool,
    ev_dblrdy: bool,
    ev_stopped: bool,
    intenset: u32,
    shorts: u32,
    sampleper: u32,
    reportper: u32,
    sample: i32,
    acc: i32,
    accread: i32,
    accdbl: i32,
    accdblread: i32,
    pselled: u32,
    psela: u32,
    pselb: u32,
    dbfen: bool,
    ledpol: u32,
    ledpre: u32,
    last_tick: u64,
    samples_since_report: u32,
    last_phase: u8,
    last_phase_valid: bool,
    debounce_hold: u8,
    debounce_count: u8,
}

impl Default for QdecNrf {
    fn default() -> Self {
        Self {
            enabled: false, running: false, ev_samplerdy: false,
            ev_reportrdy: false, ev_accof: false, ev_dblrdy: false,
            ev_stopped: false, intenset: 0, shorts: 0,
            sampleper: 0, reportper: 0,
            sample: 0, acc: 0, accread: 0, accdbl: 0, accdblread: 0,
            pselled: 0xFFFF_FFFF, psela: 0xFFFF_FFFF, pselb: 0xFFFF_FFFF,
            dbfen: false, ledpol: 0, ledpre: 0,
            last_tick: crate::system::instruction_count(),
            samples_since_report: 0, last_phase: 0, last_phase_valid: false,
            debounce_hold: 0, debounce_count: 0,
        }
    }
}

/// Decode a PSEL register (port in bits 5..6 stb Nordic PSEL layout:
/// bit 5 selects P1, bits 0..4 the pin) into (port, pin).
fn psel_pin(psel: u32) -> Option<(u8, u8)> {
    if psel == 0xFFFF_FFFF {
        return None;
    }
    let port = ((psel >> 5) & 1) as u8;
    let pin = (psel & 0x1F) as u8;
    if pin < 32 && (port == 0 || pin < 10) {
        Some((port, pin))
    } else {
        None
    }
}

impl QdecNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "QDEC" { Some(Box::new(Self::default())) } else { None }
    }
    fn fire(&self, sys: &System, bit: u32) {
        if self.intenset & bit != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(18);
        }
    }
    fn add_acc(&mut self, sys: &System, delta: i32) {
        // Both accumulators mirror the edge; a true i32 wrap on either
        // raises the shared ACCOF event.
        let mut overflow = false;
        match self.acc.checked_add(delta) {
            Some(v) => self.acc = v,
            None => {
                self.acc = self.acc.wrapping_add(delta);
                overflow = true;
            }
        }
        match self.accdbl.checked_add(delta) {
            Some(v) => self.accdbl = v,
            None => {
                self.accdbl = self.accdbl.wrapping_add(delta);
                overflow = true;
            }
        }
        if overflow {
            self.ev_accof = true;
            self.fire(sys, 1 << 2);
        }
    }
    fn pin_level(sys: &System, psel: u32) -> Option<bool> {
        psel_pin(psel).map(|(port, pin)| sys.p.gpio.borrow().read_input_pin(port, pin))
    }
    /// One sampling period elapsed: Gray-decode the A/B pins.
    fn do_sample(&mut self, sys: &System) {
        let (a, b) = match (Self::pin_level(sys, self.psela), Self::pin_level(sys, self.pselb)) {
            (Some(a), Some(b)) => (a, b),
            _ => return,
        };
        let phase = ((a as u8) << 1) | (b as u8);
        if !self.last_phase_valid {
            self.last_phase = phase;
            self.last_phase_valid = true;
            return;
        }
        let mut edge = false;
        if self.dbfen {
            // Debounce: accept the new phase only after two stable reads.
            if phase == self.debounce_hold {
                self.debounce_count += 1;
            } else {
                self.debounce_hold = phase;
                self.debounce_count = 1;
            }
            if self.debounce_count >= 2 && phase != self.last_phase {
                edge = true;
            }
        } else if phase != self.last_phase {
            edge = true;
        }
        if edge {
            // Gray code 00->01->11->10->00 is forward (+1 per edge).
            let fwd = matches!(
                (self.last_phase, phase),
                (0b00, 0b01) | (0b01, 0b11) | (0b11, 0b10) | (0b10, 0b00)
            );
            let rev = matches!(
                (self.last_phase, phase),
                (0b00, 0b10) | (0b10, 0b11) | (0b11, 0b01) | (0b01, 0b00)
            );
            if fwd {
                self.add_acc(sys, 1);
            } else if rev {
                self.add_acc(sys, -1);
            }
            // Non-Gray jumps (both changed at once) are noise: ignored.
            self.last_phase = phase;
            if self.dbfen {
                self.debounce_count = 0;
            }
        }
    }
    fn advance(&mut self, sys: &System) {
        if !self.running || !self.enabled {
            self.last_tick = crate::system::instruction_count();
            return;
        }
        let elapsed = crate::system::instruction_count().wrapping_sub(self.last_tick);
        // SAMPLEPER in 128us units at 64MHz core = *8192 instructions.
        let period = (self.sampleper as u64 + 1) * 8192;
        let mut steps = elapsed / period;
        if steps == 0 {
            return; // keep last_tick: fractional periods accumulate
        }
        if steps > 1024 {
            steps = 1024;
        }
        self.last_tick = self.last_tick.wrapping_add(steps * period);
        for _ in 0..steps {
            self.do_sample(sys);
            self.sample = self.acc;
            self.ev_samplerdy = true;
            self.fire(sys, 1 << 0);
            // SHORTS consume inline, single pass: SAMPLERDY_READCLRACC
            // snapshots ACC, SAMPLERDY_STOP halts.
            if self.shorts & (1 << 6) != 0 {
                self.accread = self.acc;
                self.acc = 0;
            }
            if self.shorts & (1 << 1) != 0 {
                self.do_stop(sys);
            }
            if self.reportper > 0 {
                self.samples_since_report += 1;
                if self.samples_since_report >= self.reportper {
                    self.samples_since_report = 0;
                    self.ev_reportrdy = true;
                    self.fire(sys, 1 << 1);
                    // REPORTRDY_READCLRACC snapshots ACC, _RDCLRACC
                    // snapshots ACCDBL, _STOP halts.
                    if self.shorts & (1 << 0) != 0 {
                        self.accread = self.acc;
                        self.acc = 0;
                    }
                    if self.shorts & (1 << 2) != 0 {
                        self.accread = self.accdbl;
                        self.acc = 0;
                    }
                    if self.shorts & (1 << 3) != 0 {
                        self.do_stop(sys);
                    }
                }
            }
        }
    }
    /// TASKS_STOP effect shared by the task arm and the _STOP shorts:
    /// halt with STOPPED (+IRQ 4 when INTENabled).
    fn do_stop(&mut self, sys: &System) {
        self.running = false;
        self.ev_stopped = true;
        self.fire(sys, 1 << 4);
    }
}

impl Peripheral for QdecNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, sys: &System, offset: u32) -> u32 {
        self.advance(sys);
        match offset {
            0x100 => self.ev_samplerdy as u32,
            0x104 => self.ev_reportrdy as u32,
            0x108 => self.ev_accof as u32,
            0x10C => self.ev_dblrdy as u32,
            0x110 => self.ev_stopped as u32,
            0x200 => self.shorts,
            0x304 => self.intenset,
            0x500 => self.enabled as u32,
            0x504 => self.ledpol,
            0x508 => self.sampleper,
            0x50C => self.sample as u32,
            0x510 => self.reportper,
            0x514 => self.acc as u32,
            0x518 => self.accread as u32,
            0x51C => self.pselled,
            0x520 => self.psela,
            0x524 => self.pselb,
            0x528 => self.dbfen as u32,
            0x540 => self.ledpre,
            0x544 => self.accdbl as u32,
            0x548 => self.accdblread as u32,
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        self.advance(sys);
        match offset {
            0x000 => {
                self.running = true;
                self.last_tick = crate::system::instruction_count();
            }
            0x004 => self.do_stop(sys), // TASKS_STOP
            0x008 => {
                // READCLRACC: snapshot ACC, clear ACC.
                self.accread = self.acc;
                self.acc = 0;
            }
            0x00C => {
                // RDCLRACC: snapshot ACCDBL, clear ACC.
                self.accread = self.accdbl;
                self.acc = 0;
            }
            0x010 => {
                // RDCLRDBL: snapshot ACCDBL, clear ACCDBL, raise DBLRDY.
                self.accdblread = self.accdbl;
                self.accdbl = 0;
                self.ev_dblrdy = true;
                self.fire(sys, 1 << 3);
                // DBLRDY_RDCLRDBL re-snapshots (single pass: the event is
                // already set, no re-fire); DBLRDY_STOP halts.
                if self.shorts & (1 << 4) != 0 {
                    self.accdblread = self.accdbl;
                    self.accdbl = 0;
                }
                if self.shorts & (1 << 5) != 0 {
                    self.do_stop(sys);
                }
            }
            0x100 => if value == 0 { self.ev_samplerdy = false; }
            0x104 => if value == 0 { self.ev_reportrdy = false; }
            0x108 => if value == 0 { self.ev_accof = false; }
            0x10C => if value == 0 { self.ev_dblrdy = false; }
            0x110 => if value == 0 { self.ev_stopped = false; }
            0x200 => self.shorts = value & 0x7F,
            0x304 => self.intenset |= value & 0x1F,
            0x308 => self.intenset &= !value,
            0x500 => self.enabled = value & 1 == 1,
            0x504 => self.ledpol = value & 1,
            0x508 => self.sampleper = value & 0xFF,
            0x510 => self.reportper = value & 0xFF,
            0x51C => self.pselled = value,
            0x520 => self.psela = value,
            0x524 => self.pselb = value,
            0x528 => self.dbfen = value & 1 == 1,
            0x540 => self.ledpre = value & 0x1FF,
            _ => {}
        }
    }
    fn tick(&mut self, sys: &System) { self.advance(sys); }
}

fn with_qdec<R>(sys: &System, f: impl FnOnce(&mut QdecNrf) -> R) -> Option<R> {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4001_2000 {
            // try_borrow_mut (P108 family): take/complete paths re-enter
            // via read/write/tick while borrowed; drop instead of panic.
            let mut b = match slot.peripheral.try_borrow_mut() {
                Ok(b) => b,
                Err(_) => return None,
            };
            if let Some(q) = b.as_any_mut().downcast_mut::<QdecNrf>() {
                return Some(f(q));
            }
            return None;
        }
    }
    None
}

/// Drive quadrature steps from the host (knob/encoder parts, tests):
/// positive = forward detents, negative = reverse. Always honored
/// (like real pin edges), independent of the sampling clock. Both
/// accumulators mirror the steps, like sampled edges.
pub fn qdec_step(sys: &System, dir: i32) {
    let accof = with_qdec(sys, |q| {
        let mut overflow = false;
        match q.acc.checked_add(dir) {
            Some(v) => q.acc = v,
            None => {
                q.acc = q.acc.wrapping_add(dir);
                overflow = true;
            }
        }
        match q.accdbl.checked_add(dir) {
            Some(v) => q.accdbl = v,
            None => {
                q.accdbl = q.accdbl.wrapping_add(dir);
                overflow = true;
            }
        }
        if overflow {
            q.ev_accof = true;
        }
        overflow
    })
    .unwrap_or(false);
    if accof {
        if let Some(en) = with_qdec(sys, |q| q.intenset) {
            if en & (1 << 2) != 0 {
                sys.p.nvic.borrow_mut().set_intr_pending(18);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    use std::sync::atomic::Ordering;
    fn step_clock(n: u64) {
        crate::system::INSTRUCTION_COUNT.fetch_add(n, Ordering::Relaxed);
    }
    #[test]
    fn samplerdy_after_period() {
        use crate::system::{lock_boot, test_dummy_system};
        // INSTRUCTION_COUNT is process-global (shared virtual clock):
        // hold BOOT_LOCK so a parallel test's clock steps cannot add or
        // steal fractional periods mid-test (P108-family flake: exact-8192
        // elapsed reads as 0 steps when a neighbor consumed the window).
        let _g = lock_boot();
        let sys = test_dummy_system();
        let mut q = QdecNrf::default();
        q.write(&sys, 0x500, 1); // ENABLE
        q.write(&sys, 0x508, 0); // SAMPLEPER 0 -> 8192 instr/period
        q.write(&sys, 0x000, 1); // START
        q.tick(&sys);
        assert_eq!(q.read(&sys, 0x100), 0, "no time elapsed yet");
        step_clock(8192);
        q.tick(&sys);
        assert_eq!(q.read(&sys, 0x100), 1, "SAMPLERDY after one period");
    }
    #[test]
    fn gray_decode_from_gpio() {
        let sys = test_dummy_system();
        // PSELA=P0.13, PSELB=P0.14 (edge P13/P14 area).
        sys.p.write(&sys, 0x40012520, 4, 13);
        sys.p.write(&sys, 0x40012524, 4, 14);
        sys.p.write(&sys, 0x40012500, 4, 1);
        sys.p.write(&sys, 0x40012000, 4, 1); // START
        // One full forward Gray cycle 00->01->11->10->00: 4 edges.
        // Each iteration advances one period; the EVENTS read pumps it.
        // (Extra global clock counts only re-read a settled phase.)
        for (a, b) in [(false, false), (false, true), (true, true), (true, false), (false, false)] {
            sys.p.gpio.borrow_mut().set_input_pin(0, 13, a);
            sys.p.gpio.borrow_mut().set_input_pin(0, 14, b);
            step_clock(8192);
            assert_eq!(sys.p.read(&sys, 0x40012100, 4), 1, "SAMPLERDY each period");
            sys.p.write(&sys, 0x40012100, 4, 0);
        }
        // 4 forward edges: 00->01->11->10->00.
        assert_eq!(sys.p.read(&sys, 0x40012514, 4), 4, "ACC=+4, got {:#x}",
            sys.p.read(&sys, 0x40012514, 4));
        // One reverse edge 00->10 (a=true(P0.13), b=false(P0.14)).
        sys.p.gpio.borrow_mut().set_input_pin(0, 13, true);
        sys.p.gpio.borrow_mut().set_input_pin(0, 14, false);
        step_clock(8192);
        let _ = sys.p.read(&sys, 0x40012100, 4);
        assert_eq!(sys.p.read(&sys, 0x40012514, 4) as i32, 3, "reverse edge -1");
        // READCLRACC snapshots and clears.
        sys.p.write(&sys, 0x40012008, 4, 1);
        assert_eq!(sys.p.read(&sys, 0x40012518, 4), 3, "ACCREAD snapshot");
        assert_eq!(sys.p.read(&sys, 0x40012514, 4), 0, "ACC cleared");
    }
    #[test]
    fn host_step_report_double_stop() {
        let sys = test_dummy_system();
        sys.p.write(&sys, 0xE000E100, 4, 1 << 18); // NVIC ISER: QDEC
        sys.p.write(&sys, 0x40012304, 4, 0x1F); // INTEN all
        sys.p.write(&sys, 0x40012500, 4, 1);
        sys.p.write(&sys, 0x40012000, 4, 1);
        qdec_step(&sys, 5);
        qdec_step(&sys, -2);
        assert_eq!(sys.p.read(&sys, 0x40012514, 4) as i32, 3, "host steps accumulate");
        // REPORTPER=1 fires on any sample (robust under parallel tests:
        // extra global clock only adds more samples, never fewer).
        sys.p.write(&sys, 0x40012510, 4, 1);
        step_clock(8192);
        let _ = sys.p.read(&sys, 0x40012100, 4);
        assert_eq!(sys.p.read(&sys, 0x40012104, 4), 1, "REPORTRDY");
        // Double-read path.
        sys.p.write(&sys, 0x4001200C, 4, 1); // RDDBLS snapshots
        qdec_step(&sys, 10); // move ACC after the snapshot
        sys.p.write(&sys, 0x40012010, 4, 1); // RDDBL
        assert_eq!(sys.p.read(&sys, 0x4001210C, 4), 1, "DBLRDY");
        assert_eq!(sys.p.read(&sys, 0x40012518, 4) as i32, 3, "snapshot value, not live");
        // STOP path + IRQ.
        sys.p.write(&sys, 0x40012004, 4, 1);
        assert_eq!(sys.p.read(&sys, 0x40012110, 4), 1, "STOPPED at 0x110");
        assert!(sys.p.nvic.borrow().has_pending(), "IRQ 18 pends");
    }
    #[test]
    fn accdbl_mirror_shorts_rdclracc_rdclrdbl() {
        // ACCDBL mirrors sampled + host steps; RDCLRACC snapshots ACCDBL
        // while clearing ACC; RDCLRDBL snapshots/clears ACCDBL with
        // DBLRDY; REPORTRDY_READCLRACC auto-reads; SAMPLERDY_STOP halts.
        let sys = test_dummy_system();
        sys.p.write(&sys, 0x40012500, 4, 1); // ENABLE
        sys.p.write(&sys, 0x40012000, 4, 1); // START
        qdec_step(&sys, 7);
        assert_eq!(sys.p.read(&sys, 0x40012544, 4) as i32, 7, "ACCDBL mirrors steps");
        sys.p.write(&sys, 0x4001200C, 4, 1); // RDCLRACC
        assert_eq!(sys.p.read(&sys, 0x40012518, 4) as i32, 7, "ACCREAD from ACCDBL");
        assert_eq!(sys.p.read(&sys, 0x40012514, 4), 0, "ACC cleared");
        assert_eq!(sys.p.read(&sys, 0x40012544, 4) as i32, 7, "ACCDBL kept");
        qdec_step(&sys, 3); // ACC=3, ACCDBL=10
        sys.p.write(&sys, 0x40012010, 4, 1); // RDCLRDBL
        assert_eq!(sys.p.read(&sys, 0x4001210C, 4), 1, "DBLRDY");
        assert_eq!(sys.p.read(&sys, 0x40012548, 4) as i32, 10, "ACCDBLREAD");
        assert_eq!(sys.p.read(&sys, 0x40012544, 4), 0, "ACCDBL cleared");
        assert_eq!(sys.p.read(&sys, 0x40012514, 4) as i32, 3, "ACC kept");
        // SHORTS: REPORTRDY_READCLRACC auto-reads into ACCREAD.
        sys.p.write(&sys, 0x40012510, 4, 1); // REPORTPER=1
        sys.p.write(&sys, 0x40012200, 4, 1 << 0); // REPORTRDY_READCLRACC
        qdec_step(&sys, 5); // ACC=8 now
        step_clock(8192);
        let _ = sys.p.read(&sys, 0x40012100, 4); // pump the sample
        assert_eq!(sys.p.read(&sys, 0x40012104, 4), 1, "REPORTRDY");
        assert_eq!(sys.p.read(&sys, 0x40012518, 4) as i32, 8, "auto-read ACC");
        assert_eq!(sys.p.read(&sys, 0x40012514, 4), 0, "auto-cleared");
        // SHORTS: SAMPLERDY_STOP halts with STOPPED.
        sys.p.write(&sys, 0x40012200, 4, 1 << 1); // SAMPLERDY_STOP
        sys.p.write(&sys, 0x40012110, 4, 0);
        step_clock(8192);
        let _ = sys.p.read(&sys, 0x40012100, 4);
        assert_eq!(sys.p.read(&sys, 0x40012110, 4), 1, "STOPPED via short");
        // 2nd run: fresh defaults.
        let q2 = QdecNrf::default();
        assert_eq!((q2.accdbl, q2.shorts), (0, 0));
    }
}
