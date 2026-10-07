use crate::system::System;
use super::Peripheral;

/// PWM0 0x4001C000 / PWM1 0x40021000 / PWM2 0x40022000 / PWM3 0x4002D000
/// (IRQs 28/33/34/45 per nrf52833.svd). Full SVD face:
///   TASKS_STOP 0x004, TASKS_SEQSTART[n] 0x008+n*4, TASKS_NEXTSTEP 0x010
///   (completes one more period on the active sequence),
///   EVENTS_STOPPED 0x104, EVENTS_SEQSTARTED[n] 0x108+n*4,
///   EVENTS_SEQEND[n] 0x110+n*4, EVENTS_PWMPERIODEND 0x118,
///   EVENTS_LOOPSDONE 0x11C, SHORTS 0x200 (SEQEND0_STOP 0, SEQEND1_STOP 1,
///   LOOPSDONE_SEQSTART0 2, LOOPSDONE_SEQSTART1 3, LOOPSDONE_STOP 4),
///   INTEN 0x300 / SET 0x304 / CLR 0x308 (STOPPED 1, SEQSTARTED0 2,
///   SEQSTARTED1 3, SEQEND0 4, SEQEND1 5, PWMPERIODEND 6, LOOPSDONE 7),
///   ENABLE 0x500, MODE 0x504 (UPDOWN bit0), COUNTERTOP 0x508 (15-bit),
///   PRESCALER 0x50C (3-bit), DECODER 0x510 (LOAD 1:0, MODE bit8),
///   LOOP 0x514 (CNT 15:0), SEQ[n].PTR 0x520+8n / CNT 0x524+8n /
///   REFRESH 0x528+8n / ENDDELAY 0x52C+8n, PSEL.OUT[4] 0x560+4n.
/// Waveform playback runs at the task quantum (no waveform RAM handle in
/// the model): SEQSTART[n] latches SEQSTARTED+SEQEND+PWMPERIODEND and,
/// with LOOP.CNT == 0 (play once), LOOPSDONE; nonzero LOOP.CNT counts
/// sequence pairs down across SEQSTART1 completions. SHORTS consume in
/// order: SEQENDn_STOP halts with STOPPED, LOOPSDONE_SEQSTARTn chains
/// the next sequence, LOOPSDONE_STOP halts with STOPPED. NEXTSTEP on the
/// active sequence latches another SEQEND+PWMPERIODEND (one more period
/// stepped); on idle it is a no-op. SEQ/PSEL registers store for
/// readback (the waveform bytes stay driver-side, like every DMA pump).
pub struct PwmNrf {
    irq: i32,
    enabled: bool,
    ev_stopped: bool,
    ev_seqstarted: [bool; 2],
    ev_seqend: [bool; 2],
    ev_periodend: bool,
    ev_loopsdone: bool,
    intenset: u32,
    shorts: u32,
    mode: u32,
    countertop: u32,
    prescaler: u32,
    decoder: u32,
    loopcnt: u32,
    loop_remaining: u32,
    chaining: bool,
    active_seq: Option<usize>,
    seq_ptr: [u32; 2],
    seq_cnt: [u32; 2],
    seq_refresh: [u32; 2],
    seq_enddelay: [u32; 2],
    pselout: [u32; 4],
    /// Host-observed SEQ compare word for OUT channel 0, as basis points
    /// (0..10000). The waveform bytes stay driver-side (see the model
    /// comment above — sys() has no guest-RAM handle), so the host pump
    /// feeds each SEQ word via observe_seq_word() after mem_read(SEQ.PTR).
    /// Sticky: TASKS_STOP / new SEQSTART do not clear it.
    duty_bp: u32,
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
            ev_periodend: false, ev_loopsdone: false, intenset: 0, shorts: 0,
            mode: 0, countertop: 0, prescaler: 0, decoder: 0, loopcnt: 0,
            loop_remaining: 0, chaining: false, active_seq: None,
            seq_ptr: [0; 2], seq_cnt: [0; 2], seq_refresh: [0; 2],
            seq_enddelay: [0; 2], pselout: [0xFFFF_FFFF; 4],
            duty_bp: 0,
        }))
    }
    fn fire(&self, sys: &System, bit: u32) {
        if self.intenset & bit != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(self.irq);
        }
    }
    /// Latch one sequence completion: SEQEND[n] + PWMPERIODEND (+IRQs),
    /// then consume the SHORTS in order (SEQENDn_STOP halts with STOPPED;
    /// LOOP accounting happens at the pair level in seqstart()).
    fn seq_end(&mut self, sys: &System, n: usize) {
        self.ev_seqend[n] = true;
        self.fire(sys, 1 << (4 + n));
        self.ev_periodend = true;
        self.fire(sys, 1 << 6);
        if self.shorts & (1 << n) != 0 {
            // SEQENDn_STOP: halt with STOPPED.
            self.active_seq = None;
            self.ev_stopped = true;
            self.fire(sys, 1 << 1);
        }
    }
    /// Consume the LOOPSDONE SHORTS after LOOPSDONE latches. Single-pass
    /// via the chaining guard (silicon re-pulses per completion and would
    /// loop forever on LOOPSDONE_SEQSTARTn with CNT==0 — the emulator
    /// terminates instead; documented divergence, same as RADIO cascade).
    fn loopsdone_shorts(&mut self, sys: &System) {
        if self.chaining {
            return;
        }
        self.chaining = true;
        if self.shorts & (1 << 4) != 0 {
            // LOOPSDONE_STOP: halt with STOPPED.
            self.active_seq = None;
            self.ev_stopped = true;
            self.fire(sys, 1 << 1);
        } else if self.shorts & (1 << 2) != 0 {
            self.seqstart(sys, 0);
        } else if self.shorts & (1 << 3) != 0 {
            self.seqstart(sys, 1);
        }
        self.chaining = false;
    }
    /// Start sequence n: SEQSTARTED + immediate completion (the waveform
    /// plays at the task quantum — no waveform RAM handle in the model).
    /// LOOP.CNT loads on SEQSTART0 and counts sequence pairs down across
    /// SEQSTART1 completions; LOOPSDONE latches when the pairs are
    /// exhausted (CNT == 0: SEQSTART0 alone is the whole playback).
    fn seqstart(&mut self, sys: &System, n: usize) {
        self.active_seq = Some(n);
        self.ev_seqstarted[n] = true;
        self.fire(sys, 1 << (2 + n));
        self.seq_end(sys, n);
        if n == 0 {
            self.loop_remaining = self.loopcnt;
            if self.loopcnt == 0 {
                self.ev_loopsdone = true;
                self.fire(sys, 1 << 7);
                self.loopsdone_shorts(sys);
            }
        } else {
            if self.loop_remaining > 0 {
                self.loop_remaining -= 1;
            }
            if self.loop_remaining == 0 {
                self.ev_loopsdone = true;
                self.fire(sys, 1 << 7);
                self.loopsdone_shorts(sys);
            }
        }
    }
}

/// Instance number (0-3) to slot base address. Unknown instances route to
/// None (callers return 0 — same bad-input convention as gpio_read_dir).
fn pwm_base(instance: u8) -> Option<u32> {
    match instance {
        0 => Some(0x4001_C000),
        1 => Some(0x4002_1000),
        2 => Some(0x4002_2000),
        3 => Some(0x4002_D000),
        _ => None,
    }
}

/// Driver-side observer for one PWM slot (base address): try_borrow_mut
/// (P108 family — take/complete paths re-enter via read/write while
/// borrowed; drop instead of panic) + downcast, mirroring with_twim.
fn with_pwm<R>(sys: &System, base: u32, f: impl FnOnce(&mut PwmNrf) -> R) -> Option<R> {
    for slot in &sys.p.peripherals {
        if slot.start == base {
            let mut b = match slot.peripheral.try_borrow_mut() {
                Ok(b) => b,
                Err(_) => return None,
            };
            if let Some(p) = b.as_any_mut().downcast_mut::<PwmNrf>() {
                return Some(f(p));
            }
            return None;
        }
    }
    None
}

/// Waveform frequency in Hz from the config registers: the 16 MHz HFCLK
/// divided by the prescaler (2^PRESCALER) and the period (COUNTERTOP, x2
/// in UpAndDown mode). 0 when disabled, COUNTERTOP == 0, or the instance
/// is unknown. Playback itself runs at the task quantum (no tick()), so
/// this is the programmed rate, same virtual class as matrix_state.
pub fn freq_hz(sys: &System, instance: u8) -> u32 {
    let Some(base) = pwm_base(instance) else { return 0 };
    with_pwm(sys, base, |p| {
        if !p.enabled {
            return 0;
        }
        let top = p.countertop & 0x7FFF;
        if top == 0 {
            return 0;
        }
        let hfclk = 16_000_000u32 >> (p.prescaler & 7);
        let period = if p.mode & 1 == 1 { top * 2 } else { top };
        hfclk / period
    })
    .unwrap_or(0)
}

/// Duty cycle of OUT channel 0 in basis points (0..10000: 0 = always low,
/// 10000 = always high, 5000 = 50.0%). 0 when disabled or when no SEQ
/// word has been observed yet. Single channel per instance: an LED cell
/// reads one pin, and OUT0 is the CODAL sound/LED channel.
pub fn duty_bp(sys: &System, instance: u8) -> u32 {
    let Some(base) = pwm_base(instance) else { return 0 };
    with_pwm(sys, base, |p| if p.enabled { p.duty_bp } else { 0 }).unwrap_or(0)
}

/// Feed one SEQ compare word (OUT channel 0) observed by the host pump
/// from guest RAM at SEQ.PTR. 15-bit compare against COUNTERTOP, clamped
/// to 0..10000; bit 15 inverts polarity (1 - duty). No-op on unknown
/// instances. Same driver-fed style as the EASYDMA take/complete pairs:
/// the model never touches guest RAM itself.
pub fn observe_seq_word(sys: &System, instance: u8, compare: u32) {
    let Some(base) = pwm_base(instance) else { return };
    with_pwm(sys, base, |p| {
        let top = p.countertop & 0x7FFF;
        if top == 0 {
            p.duty_bp = 0;
            return;
        }
        let cmp = compare & 0x7FFF;
        let bp = (cmp.min(top) * 10_000 / top).min(10_000);
        p.duty_bp = if compare & 0x8000 != 0 { 10_000 - bp } else { bp };
    });
}

impl Peripheral for PwmNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x104 => self.ev_stopped as u32,
            0x108 | 0x10C => self.ev_seqstarted[((offset - 0x108) >> 2) as usize] as u32,
            0x110 | 0x114 => self.ev_seqend[((offset - 0x110) >> 2) as usize] as u32,
            0x118 => self.ev_periodend as u32,
            0x11C => self.ev_loopsdone as u32,
            0x200 => self.shorts,
            0x300 => self.intenset, // INTEN reads the enable word
            0x304 => self.intenset,
            0x500 => self.enabled as u32,
            0x504 => self.mode & 1,          // UPDOWN
            0x508 => self.countertop & 0x7FFF,
            0x50C => self.prescaler & 7,
            0x510 => self.decoder & 0x101,    // LOAD + MODE
            0x514 => self.loopcnt & 0xFFFF,  // LOOP.CNT
            0x520 | 0x530 => self.seq_ptr[((offset - 0x520) >> 4) as usize],
            0x524 | 0x534 => self.seq_cnt[((offset - 0x524) >> 4) as usize],
            0x528 | 0x538 => self.seq_refresh[((offset - 0x528) >> 4) as usize],
            0x52C | 0x53C => self.seq_enddelay[((offset - 0x52C) >> 4) as usize],
            0x560 | 0x564 | 0x568 | 0x56C => {
                self.pselout[((offset - 0x560) >> 2) as usize]
            }
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x004 => {
                self.active_seq = None;
                self.ev_stopped = true;
                self.fire(sys, 1 << 1);
            }
            0x008 | 0x00C => {
                let n = ((offset - 0x008) >> 2) as usize;
                self.seqstart(sys, n);
            }
            0x010 => {
                // TASKS_NEXTSTEP: one more period on the active sequence;
                // no-op on idle (silicon steps the sequencer, which only
                // exists while a sequence runs).
                if let Some(n) = self.active_seq {
                    self.seq_end(sys, n);
                }
            }
            0x104 => if value == 0 { self.ev_stopped = false; }
            0x108 | 0x10C => if value == 0 { self.ev_seqstarted[((offset - 0x108) >> 2) as usize] = false; }
            0x110 | 0x114 => if value == 0 { self.ev_seqend[((offset - 0x110) >> 2) as usize] = false; }
            0x118 => if value == 0 { self.ev_periodend = false; }
            0x11C => if value == 0 { self.ev_loopsdone = false; }
            0x200 => self.shorts = value & 0x1F, // SEQENDn_STOP + LOOPSDONE_*
            0x300 => self.intenset = value & 0xFE, // INTEN absolute
            0x304 => self.intenset |= value & 0xFE, // STOPPED..LOOPSDONE
            0x308 => self.intenset &= !value,
            0x500 => self.enabled = value & 1 == 1,
            0x504 => self.mode = value & 1,
            0x508 => self.countertop = value & 0x7FFF,
            0x50C => self.prescaler = value & 7,
            0x510 => self.decoder = value & 0x101,
            0x514 => self.loopcnt = value & 0xFFFF,
            0x520 | 0x530 => self.seq_ptr[((offset - 0x520) >> 4) as usize] = value,
            0x524 | 0x534 => self.seq_cnt[((offset - 0x524) >> 4) as usize] = value & 0x7FFF,
            0x528 | 0x538 => self.seq_refresh[((offset - 0x528) >> 4) as usize] = value & 0xFF_FFFF,
            0x52C | 0x53C => self.seq_enddelay[((offset - 0x52C) >> 4) as usize] = value & 0xFF_FFFF,
            0x560 | 0x564 | 0x568 | 0x56C => {
                self.pselout[((offset - 0x560) >> 2) as usize] = value;
            }
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
    #[test]
    fn nextstep_loop_shorts_config_seq_psel() {
        // NEXTSTEP steps the active sequence; LOOP.CNT counts SEQSTART1
        // pairs; SEQEND0_STOP / LOOPSDONE_STOP halt; config + SEQ + PSEL
        // store; INTEN absolute obeys the SVD mask.
        let sys = test_dummy_system();
        sys.p.write(&sys, 0xE000E104, 4, 1 << (33 - 32)); // NVIC ISER1: PWM1
        sys.p.write(&sys, 0x40021304, 4, 0xFE); // INTEN all
        sys.p.write(&sys, 0x40021500, 4, 1); // ENABLE
        // Config block.
        sys.p.write(&sys, 0x40021504, 4, 1); // MODE UPDOWN
        sys.p.write(&sys, 0x40021508, 4, 1000); // COUNTERTOP
        sys.p.write(&sys, 0x4002150C, 4, 3); // PRESCALER
        sys.p.write(&sys, 0x40021510, 4, 0x101); // DECODER
        sys.p.write(&sys, 0x40021520, 4, 0x20001000); // SEQ0.PTR
        sys.p.write(&sys, 0x40021524, 4, 4); // SEQ0.CNT
        sys.p.write(&sys, 0x40021530, 4, 0x20002000); // SEQ1.PTR
        sys.p.write(&sys, 0x40021560, 4, 0x05); // PSEL.OUT0
        assert_eq!(sys.p.read(&sys, 0x40021504, 4), 1, "MODE");
        assert_eq!(sys.p.read(&sys, 0x40021508, 4), 1000, "COUNTERTOP");
        assert_eq!(sys.p.read(&sys, 0x40021510, 4), 0x101, "DECODER");
        assert_eq!(sys.p.read(&sys, 0x40021520, 4), 0x20001000, "SEQ0.PTR");
        assert_eq!(sys.p.read(&sys, 0x40021524, 4), 4, "SEQ0.CNT");
        assert_eq!(sys.p.read(&sys, 0x40021530, 4), 0x20002000, "SEQ1.PTR");
        assert_eq!(sys.p.read(&sys, 0x40021560, 4), 0x05, "PSEL.OUT0");
        // NEXTSTEP on idle: no-op.
        sys.p.write(&sys, 0x40021010, 4, 1);
        assert_eq!(sys.p.read(&sys, 0x40021110, 4), 0, "idle NEXTSTEP silent");
        // SEQSTART0 (play-once): STARTED+END+PERIODEND+LOOPSDONE.
        sys.p.write(&sys, 0x40021008, 4, 1);
        assert_eq!(sys.p.read(&sys, 0x40021118, 4), 1, "PWMPERIODEND");
        assert_eq!(sys.p.read(&sys, 0x4002111C, 4), 1, "LOOPSDONE play-once");
        // NEXTSTEP on the active sequence: another period completes.
        sys.p.write(&sys, 0x40021110, 4, 0);
        sys.p.write(&sys, 0x40021010, 4, 1);
        assert_eq!(sys.p.read(&sys, 0x40021110, 4), 1, "NEXTSTEP ends a period");
        // LOOP.CNT=2: SEQSTART0 loads, two SEQSTART1 completions to drain.
        sys.p.write(&sys, 0x40021514, 4, 2); // LOOP.CNT
        sys.p.write(&sys, 0x4002111C, 4, 0);
        sys.p.write(&sys, 0x40021008, 4, 1); // SEQSTART0
        assert_eq!(sys.p.read(&sys, 0x4002111C, 4), 0, "loops pending");
        sys.p.write(&sys, 0x4002100C, 4, 1); // SEQSTART1 (pair 1)
        assert_eq!(sys.p.read(&sys, 0x4002111C, 4), 0, "one pair left");
        sys.p.write(&sys, 0x4002100C, 4, 1); // SEQSTART1 (pair 2)
        assert_eq!(sys.p.read(&sys, 0x4002111C, 4), 1, "LOOPSDONE after pairs");
        // SEQEND0_STOP short halts with STOPPED.
        sys.p.write(&sys, 0x40021200, 4, 1 << 0); // SHORTS SEQEND0_STOP
        sys.p.write(&sys, 0x40021104, 4, 0);
        sys.p.write(&sys, 0x40021008, 4, 1);
        assert_eq!(sys.p.read(&sys, 0x40021104, 4), 1, "STOPPED via short");
        assert!(sys.p.nvic.borrow().has_pending(), "PWM IRQs pend");
        // 2nd run: fresh defaults.
        let p2 = PwmNrf::new("PWM0").unwrap();
        drop(p2);
    }
    #[test]
    fn freq_hz_from_config_registers() {
        // 16 MHz HFCLK >> PRESCALER over COUNTERTOP (x2 in UpAndDown).
        // P110 periph2 firmware programs TOP=1000, PSC=0, Up -> 16000 Hz.
        let sys = test_dummy_system();
        sys.p.write(&sys, 0x40021500, 4, 1); // PWM1 ENABLE
        sys.p.write(&sys, 0x40021508, 4, 1000); // COUNTERTOP
        assert_eq!(super::freq_hz(&sys, 1), 16_000, "16MHz/1000 Up");
        sys.p.write(&sys, 0x4002150C, 4, 3); // PRESCALER 3
        assert_eq!(super::freq_hz(&sys, 1), 2_000, "2MHz/1000");
        sys.p.write(&sys, 0x40021504, 4, 1); // MODE UPDOWN
        assert_eq!(super::freq_hz(&sys, 1), 1_000, "up-down doubles period");
        sys.p.write(&sys, 0x40021500, 4, 0); // ENABLE 0
        assert_eq!(super::freq_hz(&sys, 1), 0, "disabled reads 0");
        sys.p.write(&sys, 0x40021500, 4, 1);
        sys.p.write(&sys, 0x40021508, 4, 0); // COUNTERTOP 0
        assert_eq!(super::freq_hz(&sys, 1), 0, "TOP 0 reads 0");
        assert_eq!(super::freq_hz(&sys, 0), 0, "PWM0 disabled");
        assert_eq!(super::freq_hz(&sys, 4), 0, "unknown instance reads 0");
        assert_eq!(super::freq_hz(&sys, 255), 0, "unknown instance reads 0");
    }
    #[test]
    fn freq_hz_routes_all_four_instances() {
        // Same config on every slot base reads the same rate back.
        let sys = test_dummy_system();
        for (inst, base) in [(0u8, 0x4001_C000u32), (1, 0x4002_1000), (2, 0x4002_2000), (3, 0x4002_D000)] {
            sys.p.write(&sys, base + 0x500, 4, 1); // ENABLE
            sys.p.write(&sys, base + 0x508, 4, 500); // COUNTERTOP
            sys.p.write(&sys, base + 0x50C, 4, 1); // PRESCALER 1 -> 8 MHz
            assert_eq!(super::freq_hz(&sys, inst), 16_000, "instance routes");
        }
    }
    #[test]
    fn duty_bp_observe_clamp_polarity() {
        // Basis points on OUT0: compare/TOP, clamped; bit15 inverts.
        let sys = test_dummy_system();
        sys.p.write(&sys, 0x4001C500, 4, 1); // PWM0 ENABLE
        sys.p.write(&sys, 0x4001C508, 4, 1000); // COUNTERTOP
        assert_eq!(super::duty_bp(&sys, 0), 0, "unobserved reads 0");
        super::observe_seq_word(&sys, 0, 500);
        assert_eq!(super::duty_bp(&sys, 0), 5000, "50.0%");
        super::observe_seq_word(&sys, 0, 0);
        assert_eq!(super::duty_bp(&sys, 0), 0, "always low");
        super::observe_seq_word(&sys, 0, 1000);
        assert_eq!(super::duty_bp(&sys, 0), 10_000, "always high");
        super::observe_seq_word(&sys, 0, 1500);
        assert_eq!(super::duty_bp(&sys, 0), 10_000, "clamped, never wraps");
        super::observe_seq_word(&sys, 0, 0x8000 | 250);
        assert_eq!(super::duty_bp(&sys, 0), 7500, "polarity inverts");
        super::observe_seq_word(&sys, 0, 0x8000);
        assert_eq!(super::duty_bp(&sys, 0), 10_000, "inverted zero is full");
        // Sticky across STOP; cleared view while disabled.
        sys.p.write(&sys, 0x4001C004, 4, 1); // TASKS_STOP
        assert_eq!(super::duty_bp(&sys, 0), 10_000, "STOP keeps latch");
        sys.p.write(&sys, 0x4001C500, 4, 0); // ENABLE 0
        assert_eq!(super::duty_bp(&sys, 0), 0, "disabled reads 0");
        // TOP 0 + unknown instances: no panic, read 0.
        sys.p.write(&sys, 0x4001C500, 4, 1);
        sys.p.write(&sys, 0x4001C508, 4, 0);
        super::observe_seq_word(&sys, 0, 500);
        assert_eq!(super::duty_bp(&sys, 0), 0, "TOP 0 reads 0");
        super::observe_seq_word(&sys, 9, 500);
        assert_eq!(super::duty_bp(&sys, 9), 0, "unknown instance reads 0");
    }
}
