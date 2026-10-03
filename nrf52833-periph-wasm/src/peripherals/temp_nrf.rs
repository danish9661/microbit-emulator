use crate::system::System;
use super::Peripheral;

/// TEMP @ 0x4000C000 (IRQ 12 per nrf52833.svd). TASKS_START 0x000, TASKS_STOP 0x004,
/// EVENTS_DATARDY 0x100, INTENSET 0x304/CLR 0x308, TEMP 0x508 (signed,
/// 0.25 degC LSB), factory calibration block A0-A5 0x520-0x534 /
/// B0-B5 0x540-0x554 / T0-T4 0x560-0x570 (read-only, SVD reset defaults
/// below — per-chip trim the model reports but never alters). Synthetic
/// 21 degC = 84. DATARDY set on START. The die temperature itself is
/// host-driven (`temp_set_celsius`): the calibration math converts raw
/// sensor readings, but the model has no analog sensor — the host owns
/// the temperature, the cal block is reported for firmware that dumps it.
pub struct TempNrf {
    ev_datardy: bool,
    intenset: u32,
    temp: i32,
}

impl Default for TempNrf {
    fn default() -> Self {
        Self { ev_datardy: false, intenset: 0, temp: 84 }
    }
}

impl TempNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "TEMP" { Some(Box::new(Self::default())) } else { None }
    }
    /// Factory calibration slope/intercept trim (SVD reset defaults —
    /// per-chip values the model reports read-only, like silicon).
    fn cal_read(offset: u32) -> u32 {
        match offset {
            0x520 => 0x00000326, // A0
            0x524 => 0x00000348, // A1
            0x528 => 0x000003AA, // A2
            0x52C => 0x0000040E, // A3
            0x530 => 0x000004BD, // A4
            0x534 => 0x000005A3, // A5
            0x540 => 0x00003FEF, // B0
            0x544 => 0x00003FBE, // B1
            0x548 => 0x00003FBE, // B2
            0x54C => 0x00000012, // B3
            0x550 => 0x00000124, // B4
            0x554 => 0x0000027C, // B5
            0x560 => 0x000000E2, // T0
            0x564 => 0x00000000, // T1
            0x568 => 0x00000019, // T2
            0x56C => 0x0000003C, // T3
            0x570 => 0x00000050, // T4
            _ => 0,
        }
    }
}

fn with_temp<R>(sys: &System, f: impl FnOnce(&mut TempNrf) -> R) -> Option<R> {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4000_C000 {
            // try_borrow_mut (P108 family): take/complete paths re-enter
            // via read/write/tick while borrowed; drop instead of panic.
            let mut b = match slot.peripheral.try_borrow_mut() {
                Ok(b) => b,
                Err(_) => return None,
            };
            if let Some(t) = b.as_any_mut().downcast_mut::<TempNrf>() {
                return Some(f(t));
            }
            return None;
        }
    }
    None
}

/// Drive the die temperature in whole degrees C (quarter-degree TEMP
/// register reads back `c * 4`, reset default 21 C). Test/JS side of
/// the thermometer: firmware STARTs, polls DATARDY, reads TEMP.
pub fn temp_set_celsius(sys: &System, c: i32) {
    with_temp(sys, |t| t.temp = c.clamp(-40, 85) * 4);
}

impl Peripheral for TempNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x100 => self.ev_datardy as u32,
            0x304 => self.intenset,
            0x508 => self.temp as u32,
            0x520..=0x570 => Self::cal_read(offset),
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => {
                self.ev_datardy = true;
                if self.intenset & 1 != 0 {
                    sys.p.nvic.borrow_mut().set_intr_pending(12);
                }
            }
            0x004 => self.ev_datardy = false,
            0x100 => if value == 0 { self.ev_datardy = false; }
            0x304 => self.intenset |= value,
            0x308 => self.intenset &= !value,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn datardy_and_temp_value() {
        let sys = test_dummy_system();
        let mut t = TempNrf::default();
        t.write(&sys, 0x000, 1);
        assert_eq!(t.read(&sys, 0x100), 1);
        assert_eq!(t.read(&sys, 0x508) as i32, 84);
    }
    #[test]
    fn datardy_irq_gated_by_inten() {
        // START sets DATARDY always; IRQ 12 pends only with INTEN bit 0
        // (NVIC ISER IRQ 12). Clear by write-0; STOP also clears.
        let sys = test_dummy_system();
        sys.p.write(&sys, 0xE000E100, 4, 1 << 12);
        let mut t = TempNrf::default();
        t.write(&sys, 0x000, 1);
        assert_eq!(t.read(&sys, 0x100), 1, "DATARDY");
        assert!(!sys.p.nvic.borrow().has_pending(), "no IRQ without INTEN");
        t.write(&sys, 0x304, 1); // INTENSET DATARDY
        t.write(&sys, 0x000, 1);
        assert!(sys.p.nvic.borrow().has_pending(), "DATARDY IRQ 12 pends");
        t.write(&sys, 0x100, 0);
        assert_eq!(t.read(&sys, 0x100), 0, "clear by write-0");
        t.write(&sys, 0x000, 1);
        t.write(&sys, 0x004, 1); // STOP
        assert_eq!(t.read(&sys, 0x100), 0, "STOP clears DATARDY");
    }
    #[test]
    fn host_driven_temperature() {
        let sys = test_dummy_system();
        sys.p.write(&sys, 0x4000C500, 4, 1); // ENABLE
        temp_set_celsius(&sys, 27);
        sys.p.write(&sys, 0x4000C000, 4, 1); // START
        assert_eq!(sys.p.read(&sys, 0x4000C100, 4), 1, "DATARDY");
        assert_eq!(sys.p.read(&sys, 0x4000C508, 4) as i32, 108, "27C in quarters");
        temp_set_celsius(&sys, -40);
        sys.p.write(&sys, 0x4000C000, 4, 1);
        assert_eq!(sys.p.read(&sys, 0x4000C508, 4) as i32, -160);
        assert_eq!(TempNrf::default().temp, 84, "fresh default 21C");
    }
    #[test]
    fn factory_cal_block_reads_svd_defaults() {
        // A/B/T trim reads the SVD reset values (read-only factory data);
        // writes are ignored.
        let sys = test_dummy_system();
        assert_eq!(sys.p.read(&sys, 0x4000C520, 4), 0x326, "A0");
        assert_eq!(sys.p.read(&sys, 0x4000C534, 4), 0x5A3, "A5");
        assert_eq!(sys.p.read(&sys, 0x4000C540, 4), 0x3FEF, "B0");
        assert_eq!(sys.p.read(&sys, 0x4000C554, 4), 0x27C, "B5");
        assert_eq!(sys.p.read(&sys, 0x4000C560, 4), 0xE2, "T0");
        assert_eq!(sys.p.read(&sys, 0x4000C570, 4), 0x50, "T4");
        sys.p.write(&sys, 0x4000C520, 4, 0xDEAD);
        assert_eq!(sys.p.read(&sys, 0x4000C520, 4), 0x326, "writes ignored");
    }
}
