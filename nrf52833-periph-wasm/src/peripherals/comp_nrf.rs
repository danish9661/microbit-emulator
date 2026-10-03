use crate::system::System;
use super::Peripheral;

/// COMP @ 0x40013000 (IRQ 19, +LPCOMP alias same base). Offsets from
/// nrf52833.svd: TASKS_START 0x000, TASKS_STOP 0x004, TASKS_SAMPLE 0x008,
/// EVENTS_READY 0x100, EVENTS_DOWN 0x104, EVENTS_UP 0x108,
/// EVENTS_CROSS 0x10C, SHORTS 0x200 (READY_SAMPLE 0, READY_STOP 1,
/// DOWN_STOP 2, UP_STOP 3, CROSS_STOP 4), INTEN 0x300 / SET 0x304
/// (READY 0, DOWN 1, UP 2, CROSS 3) / CLR 0x308, RESULT 0x400
/// (Below 0 / Above 1), ENABLE 0x500 (Disabled 0 / Enabled 2),
/// PSEL 0x504 (AnalogInput0-7), REFSEL 0x508 (Int1V2 0 / Int1V8 1 /
/// Int2V4 2 / VDD 4 / ARef 5), EXTREFSEL 0x50C (AREF0-7 select), TH 0x530
/// (THDOWN[5:0], THUP[13:8], 6-bit fractions of the reference), MODE 0x534
/// (SP speed, MAIN SE/Diff), HYST 0x538 (NoHyst 0 / Hyst50mV 1).
/// Protocol: START->READY (READY_SAMPLE short chains a SAMPLE at once;
/// READY_STOP clears READY at once); SAMPLE resolves RESULT against the
/// thresholds and raises DOWN/UP/CROSS on changes (crossing semantics:
/// UP fires when the result becomes Above, DOWN when it becomes Below,
/// CROSS on any change; inside the band the previous result is retained;
/// DOWN/UP/CROSS_STOP shorts clear READY at once).
/// Thresholds scale to the selected reference (Int1V2 1200 mV, Int1V8
/// 1800 mV, Int2V4 2400 mV, VDD 3300 mV, ARef = host-driven level, reset
/// 3300 mV). The analog input level comes from the driver
/// (`comp_set_input_mv`, default mid-scale reads Below until driven);
/// the AREF level comes from `comp_set_aref_mv` (reset 3300 mV).
pub struct CompNrf {
    enabled: bool,
    ev_ready: bool,
    ev_down: bool,
    ev_up: bool,
    ev_cross: bool,
    intenset: u32,
    shorts: u32,
    psel: u32,
    refsel: u32,
    extrefsel: u32,
    thdown: u32,
    thup: u32,
    hyst: u32,
    mode: u32,
    last: u32,
    vin_mv: u32,
    aref_mv: u32,
}

impl Default for CompNrf {
    fn default() -> Self {
        Self {
            enabled: false, ev_ready: false, ev_down: false, ev_up: false,
            ev_cross: false, intenset: 0, shorts: 0, psel: 0,
            refsel: 4, extrefsel: 0, thdown: 0, thup: 0,
            hyst: 0, mode: 0, last: 0, vin_mv: 1650, aref_mv: 3300,
        }
    }
}

impl CompNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "COMP" || name == "LPCOMP" {
            Some(Box::new(Self::default()))
        } else {
            None
        }
    }
    fn fire(&self, sys: &System, bit: u32) {
        if self.intenset & bit != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(19);
        }
    }
    /// Reference level in mV for the REFSEL selection (SVD enum:
    /// Int1V2/Int1V8/Int2V4/VDD/ARef; anything else reads as VDD).
    fn vref_mv(&self) -> u32 {
        match self.refsel & 7 {
            0 => 1200,
            1 => 1800,
            2 => 2400,
            5 => self.aref_mv,
            _ => 3300, // VDD (4) + reserved
        }
    }
    /// Resolve the comparator: Above (1) when VIN clears THUP, Below (0)
    /// when under THDOWN, else retain (threshold band / hysteresis).
    fn resolve(&self) -> u32 {
        // TH 6-bit values map linearly onto the reference.
        let vref = self.vref_mv();
        let down_mv = self.thdown.min(63) * vref / 64;
        let up_mv = self.thup.min(63) * vref / 64;
        if self.vin_mv > up_mv {
            1
        } else if self.vin_mv < down_mv {
            0
        } else {
            self.last
        }
    }
    fn sample(&mut self, sys: &System) {
        let new = self.resolve();
        if new != self.last {
            self.ev_cross = true;
            self.fire(sys, 1 << 3);
            if self.shorts & (1 << 4) != 0 {
                self.ev_ready = false; // CROSS_STOP
            }
            if new == 1 {
                self.ev_up = true;
                self.fire(sys, 1 << 2);
                if self.shorts & (1 << 3) != 0 {
                    self.ev_ready = false; // UP_STOP
                }
            } else {
                self.ev_down = true;
                self.fire(sys, 1 << 1);
                if self.shorts & (1 << 2) != 0 {
                    self.ev_ready = false; // DOWN_STOP
                }
            }
            self.last = new;
        }
    }
}

impl Peripheral for CompNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x100 => self.ev_ready as u32,
            0x104 => self.ev_down as u32,
            0x108 => self.ev_up as u32,
            0x10C => self.ev_cross as u32,
            0x200 => self.shorts,
            0x300 => self.intenset, // INTEN reads the enable word
            0x304 => self.intenset,
            0x400 => self.last,
            0x500 => self.enabled as u32,
            0x504 => self.psel,
            0x508 => self.refsel,
            0x50C => self.extrefsel,
            0x530 => self.thdown | (self.thup << 8),
            0x534 => self.mode,
            0x538 => self.hyst,
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => {
                self.ev_ready = true;
                self.fire(sys, 1 << 0);
                if self.shorts & 1 != 0 {
                    self.sample(sys); // READY_SAMPLE
                }
                if self.shorts & (1 << 1) != 0 {
                    self.ev_ready = false; // READY_STOP
                }
            }
            0x004 => {
                self.ev_ready = false;
            }
            0x008 => self.sample(sys),
            0x100 => if value == 0 { self.ev_ready = false; }
            0x104 => if value == 0 { self.ev_down = false; }
            0x108 => if value == 0 { self.ev_up = false; }
            0x10C => if value == 0 { self.ev_cross = false; }
            0x200 => self.shorts = value & 0x1F,
            0x300 => self.intenset = value & 0x0F, // INTEN absolute
            0x304 => self.intenset |= value & 0x0F,
            0x308 => self.intenset &= !value,
            0x500 => self.enabled = value & 3 == 2,
            0x504 => self.psel = value & 7,
            0x508 => self.refsel = value & 7,
            0x50C => self.extrefsel = value & 7,
            0x530 => {
                self.thdown = value & 0x3F;
                self.thup = (value >> 8) & 0x3F;
            }
            0x534 => self.mode = value & 0x103,
            0x538 => self.hyst = value & 1,
            _ => {}
        }
    }
}

fn with_comp<R>(sys: &System, f: impl FnOnce(&mut CompNrf) -> R) -> Option<R> {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4001_3000 {
            // try_borrow_mut (P108 family): take/complete paths re-enter
            // via read/write/tick while borrowed; drop instead of panic.
            let mut b = match slot.peripheral.try_borrow_mut() {
                Ok(b) => b,
                Err(_) => return None,
            };
            if let Some(c) = b.as_any_mut().downcast_mut::<CompNrf>() {
                return Some(f(c));
            }
            return None;
        }
    }
    None
}

/// Drive the analog input level in millivolts (compared against the
/// REFSEL reference; TH fractions scale to it). Test/JS side of the
/// comparator: firmware programs PSEL + TH, then TASKS_SAMPLE resolves
/// against this level.
pub fn comp_set_input_mv(sys: &System, mv: u32) {
    with_comp(sys, |c| c.vin_mv = mv.min(3300));
}

/// Drive the external reference level in millivolts (used when REFSEL
/// selects ARef; reset 3300 mV).
pub fn comp_set_aref_mv(sys: &System, mv: u32) {
    with_comp(sys, |c| c.aref_mv = mv.min(3300));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn ready_handshake() {
        let sys = test_dummy_system();
        let mut c = CompNrf::default();
        c.write(&sys, 0x500, 2);
        c.write(&sys, 0x000, 1);
        assert_eq!(c.read(&sys, 0x100), 1);
    }
    #[test]
    fn thresholds_cross_up_down() {
        let sys = test_dummy_system();
        // THDOWN=16 (~825mV), THUP=48 (~2475mV); live map so the
        // driver API path is covered too.
        sys.p.write(&sys, 0x40013500, 4, 2); // ENABLE
        sys.p.write(&sys, 0x40013530, 4, 16 | (48 << 8)); // TH
        sys.p.write(&sys, 0x40013304, 4, 0x0F); // INTEN all
        sys.p.write(&sys, 0x40013000, 4, 1); // START
        assert_eq!(sys.p.read(&sys, 0x40013100, 4), 1, "READY");
        comp_set_input_mv(&sys, 100); // below THDOWN
        sys.p.write(&sys, 0x40013008, 4, 1); // SAMPLE
        assert_eq!(sys.p.read(&sys, 0x40013400, 4), 0, "Below");
        assert_eq!(sys.p.read(&sys, 0x40013104, 4), 0, "no DOWN edge yet");
        comp_set_input_mv(&sys, 3200); // above THUP
        sys.p.write(&sys, 0x40013008, 4, 1);
        assert_eq!(sys.p.read(&sys, 0x40013400, 4), 1, "Above");
        assert_eq!(sys.p.read(&sys, 0x40013108, 4), 1, "UP edge");
        assert_eq!(sys.p.read(&sys, 0x4001310C, 4), 1, "CROSS edge");
        comp_set_input_mv(&sys, 1500); // inside band: retains Above
        sys.p.write(&sys, 0x40013008, 4, 1);
        assert_eq!(sys.p.read(&sys, 0x40013400, 4), 1, "band retains");
        comp_set_input_mv(&sys, 100); // back below
        sys.p.write(&sys, 0x40013008, 4, 1);
        assert_eq!(sys.p.read(&sys, 0x40013104, 4), 1, "DOWN edge");
        // 2nd run: fresh instance reads Below default, no leak.
        let c2 = CompNrf::default();
        assert_eq!(c2.last, 0);
    }
    #[test]
    fn shorts_refsel_inten_absolute() {
        // READY_SAMPLE chains a sample into START; UP_STOP clears READY
        // on the UP edge; REFSEL Int1V2 rescales the thresholds; ARef
        // uses the host level; INTEN absolute obeys the mask.
        let sys = test_dummy_system();
        sys.p.write(&sys, 0x40013300, 4, 0xFF);
        assert_eq!(sys.p.read(&sys, 0x40013300, 4), 0x0F, "INTEN mask");
        sys.p.write(&sys, 0x40013500, 4, 2); // ENABLE
        sys.p.write(&sys, 0x40013530, 4, 16 | (48 << 8)); // TH
        sys.p.write(&sys, 0x40013200, 4, (1 << 0) | (1 << 3)); // READY_SAMPLE + UP_STOP
        comp_set_input_mv(&sys, 3200); // above VDD-scaled THUP
        sys.p.write(&sys, 0x40013000, 4, 1); // START samples at once
        assert_eq!(sys.p.read(&sys, 0x40013400, 4), 1, "Above");
        assert_eq!(sys.p.read(&sys, 0x40013100, 4), 0, "READY cleared by UP_STOP");
        // REFSEL Int1V2 (1200 mV): THUP=48 -> 900 mV; 1000 mV reads Above.
        sys.p.write(&sys, 0x40013508, 4, 0); // REFSEL Int1V2
        assert_eq!(sys.p.read(&sys, 0x40013508, 4), 0, "REFSEL reads back");
        comp_set_input_mv(&sys, 1000);
        sys.p.write(&sys, 0x40013008, 4, 1); // SAMPLE
        assert_eq!(sys.p.read(&sys, 0x40013400, 4), 1, "Above Int1V2 THUP");
        comp_set_input_mv(&sys, 100);
        sys.p.write(&sys, 0x40013008, 4, 1);
        assert_eq!(sys.p.read(&sys, 0x40013400, 4), 0, "Below Int1V2 THDOWN");
        // ARef path: host level 2000 mV, THUP=48 -> 1500 mV.
        sys.p.write(&sys, 0x40013508, 4, 5); // REFSEL ARef
        comp_set_aref_mv(&sys, 2000);
        comp_set_input_mv(&sys, 1600);
        sys.p.write(&sys, 0x40013008, 4, 1);
        assert_eq!(sys.p.read(&sys, 0x40013400, 4), 1, "Above ARef THUP");
        // 2nd run: fresh defaults (VDD ref, no shorts).
        let c2 = CompNrf::default();
        assert_eq!((c2.refsel, c2.shorts, c2.aref_mv), (4, 0, 3300));
    }
}
