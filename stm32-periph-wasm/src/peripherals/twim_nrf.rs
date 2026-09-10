use crate::system::System;
use super::Peripheral;

/// TWIM0 @ 0x40003000 (IRQ 3) / TWIM1 @ 0x40004000 (IRQ 33). P3 master subset:
///   TASKS_STARTTX 0x008, TASKS_STARTRX 0x000, TASKS_STOP 0x014,
///   EVENTS_STOPPED 0x104, EVENTS_ERROR 0x124, EVENTS_TXSTARTED 0x14C,
///   EVENTS_RXSTARTED 0x148, SHORTS 0x200, INTENSET 0x304, INTENCLR 0x308,
///   ERRORSRC 0x4C4, ENABLE 0x500, PSEL.SCL/SDA 0x508/0x50C, FREQUENCY 0x524,
///   ADDRESS 0x588, TXD.PTR/MAXCNT 0x544/0x548, RXD.PTR/MAXCNT/AMOUNT 0x534/0x538/0x53C,
///   TXD byte 0x51C / RXD byte 0x518 (polling path for bring-up).
/// Bytes flow to/from JS i2c_taps (LSM303 accel/mag) by ADDRESS match.
pub struct Twim {
    name: String,
    irq: i32,
    enable: u32,
    address: u8,
    ev_stopped: bool,
    ev_error: bool,
    tx_byte: u8,
    intenset: u32,
    started_tx: bool,
    started_rx: bool,
}

impl Twim {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        let irq = match name {
            "TWIM0" | "TWI0" | "SPIM0_SPIS0_TWIM0_TWIS0" => 3,
            "TWIM1" | "TWI1" => 33,
            _ => return None,
        };
        Some(Box::new(Self {
            name: name.to_string(), irq, enable: 0, address: 0,
            ev_stopped: false, ev_error: false, tx_byte: 0,
            intenset: 0, started_tx: false, started_rx: false,
        }))
    }
    fn fire(&self, sys: &System, bit: u32) {
        if self.intenset & bit != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(self.irq);
        }
    }
    fn tap_addr(&self) -> u8 { self.address }
}

impl Peripheral for Twim {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x104 => self.ev_stopped as u32,
            0x124 => self.ev_error as u32,
            0x148 => self.started_rx as u32,
            0x14C => self.started_tx as u32,
            0x304 => self.intenset,
            0x500 => self.enable,
            0x518 => crate::system::i2c_tap_rx_pop(&self.name) as u32,
            0x588 => self.address as u32,
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => { self.started_rx = true; } // STARTRX
            0x008 => { self.started_tx = true; self.ev_stopped = false; } // STARTTX
            0x014 => { // STOP -> STOPPED event
                self.started_tx = false;
                self.started_rx = false;
                self.ev_stopped = true;
                crate::system::i2c_tap_push_event(&self.name, 1 << 31); // STOP boundary
                self.fire(sys, 1 << 1);
            }
            0x104 => if value == 0 { self.ev_stopped = false; }
            0x124 => if value == 0 { self.ev_error = false; }
            0x304 => self.intenset |= value,
            0x308 => self.intenset &= !value,
            0x500 => self.enable = value & 0xF,
            0x51C => {
                // TXD byte -> tapped slave matching ADDRESS
                self.tx_byte = (value & 0xFF) as u8;
                let _ = self.tap_addr();
                crate::system::i2c_tap_push_tx(&self.name, self.tx_byte);
            }
            0x588 => self.address = (value & 0x7F) as u8,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn tx_byte_reaches_tap_and_stop_events() {
        let sys = test_dummy_system();
        let mut t = Twim::new("TWIM0").unwrap();
        t.write(&sys, 0x500, 6); // ENABLE
        t.write(&sys, 0x588, 0x19); // LSM303 accel addr
        t.write(&sys, 0x008, 1); // STARTTX
        t.write(&sys, 0x51C, 0x28); // OUT_X_L register
        t.write(&sys, 0x014, 1); // STOP
        assert_eq!(t.read(&sys, 0x104), 1, "STOPPED");
        let ev = crate::system::i2c_tap_take_tx("TWIM0");
        assert!(ev.contains(&0x28), "tap got byte, got {ev:?}");
        // 2nd run: fresh instance, no leak
        let mut t2 = Twim::new("TWIM0").unwrap();
        assert_eq!(t2.read(&sys, 0x104), 0);
    }
}
