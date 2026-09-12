use crate::system::System;
use super::Peripheral;

/// TEMP @ 0x4000C000 (IRQ 12 per nrf52833.svd). TASKS_START 0x000, TASKS_STOP 0x004,
/// EVENTS_DATARDY 0x100, INTENSET 0x304/CLR 0x308, TEMP 0x508 (signed,
/// 0.25 degC LSB). Synthetic 21 degC = 84. DATARDY set on START.
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
}

fn with_temp<R>(sys: &System, f: impl FnOnce(&mut TempNrf) -> R) -> Option<R> {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4000_C000 {
            let mut b = slot.peripheral.borrow_mut();
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
}
