use crate::system::{System, instruction_count, get_uart_output};
use super::Peripheral;

/// UARTE0 @ 0x40002000 (IRQ 2). P2 polling subset:
///   ENABLE 0x500, BAUDRATE 0x524, TXD 0x51C (byte TX -> UART_OUTPUT),
///   RXD 0x518 (byte RX), EVENTS_TXDRDY 0x11C / EVENTS_ENDTX 0x120 /
///   EVENTS_RXDRDY 0x108 / EVENTS_ERROR 0x124, TASKS_STARTTX 0x008 /
///   TASKS_STARTRX 0x000, INTENSET 0x304 / INTENCLR 0x308.
/// EASYDMA (TXD.PTR/MAXCNT at 0x544+) comes in P3 with TWIM/SPIM DMA.
pub struct Uarte {
    irq: i32,
    enable: u32,
    baudrate: u32,
    ev_txdrdy: bool,
    ev_endtx: bool,
    ev_rxdrdy: bool,
    ev_error: bool,
    rx_buf: Vec<u8>,
    intenset: u32,
}

impl Default for Uarte {
    fn default() -> Self {
        Self { irq: 2, enable: 0, baudrate: 0, ev_txdrdy: false, ev_endtx: false,
               ev_rxdrdy: false, ev_error: false, rx_buf: Vec::new(), intenset: 0 }
    }
}

impl Uarte {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        let irq = match name {
            "UARTE0" | "UART0" | "UARTE0_UART0" => 2,
            "UARTE1" => 40,
            _ => return None,
        };
        Some(Box::new(Self { irq, ..Self::default() }))
    }
    fn fire(&self, sys: &System, ev_bit: u32) {
        if self.intenset & ev_bit != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(self.irq);
        }
    }
}

impl Peripheral for Uarte {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x108 => self.ev_rxdrdy as u32,
            0x11C => self.ev_txdrdy as u32,
            0x120 => self.ev_endtx as u32,
            0x124 => self.ev_error as u32,
            0x304 => self.intenset,
            0x500 => self.enable,
            0x518 => self.rx_buf.first().copied().unwrap_or(0) as u32,
            0x524 => self.baudrate,
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => {} // TASKS_STARTRX
            0x008 => { self.ev_endtx = false; } // TASKS_STARTTX
            0x108 => if value == 0 { self.ev_rxdrdy = false; }
            0x11C => if value == 0 { self.ev_txdrdy = false; }
            0x120 => if value == 0 { self.ev_endtx = false; }
            0x124 => if value == 0 { self.ev_error = false; }
            0x304 => {
                self.intenset |= value;
                // re-fire any already-set event the firmware just enabled
                if self.ev_txdrdy && value & (1 << 7) != 0 { self.fire(sys, 1 << 7); }
                if self.ev_endtx && value & (1 << 8) != 0 { self.fire(sys, 1 << 8); }
                if self.ev_rxdrdy && value & (1 << 2) != 0 { self.fire(sys, 1 << 2); }
            }
            0x308 => self.intenset &= !value,
            0x500 => self.enable = value & 0xF,
            0x518 => {} // RXD read-only
            0x51C => {
                // TXD byte: console lifeline
                let ch = (value & 0xFF) as u8;
                get_uart_output().lock().unwrap().push(ch as char);
                let _ = instruction_count();
                self.ev_txdrdy = true;
                self.ev_endtx = true;
                self.fire(sys, 1 << 7);
                self.fire(sys, 1 << 8);
            }
            0x524 => self.baudrate = value,
            _ => {}
        }
    }
    fn rx_byte(&mut self, sys: &System, byte: u8) {
        self.rx_buf.push(byte);
        if self.rx_buf.len() == 1 {
            self.ev_rxdrdy = true;
            self.fire(sys, 1 << 2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn tx_byte_reaches_console_and_events() {
        let sys = test_dummy_system();
        crate::system::get_uart_output().lock().unwrap().clear();
        let mut u = Uarte::default();
        u.write(&sys, 0x500, 8); // ENABLE=UARTE
        u.write(&sys, 0x51C, b'H' as u32);
        assert_eq!(u.read(&sys, 0x11C), 1);
        assert_eq!(u.read(&sys, 0x120), 1);
        assert!(crate::system::get_uart_output().lock().unwrap().contains('H'));
        u.write(&sys, 0x11C, 0);
        assert_eq!(u.read(&sys, 0x11C), 0);
    }
    #[test]
    fn rx_byte_sets_event_and_reads() {
        let sys = test_dummy_system();
        let mut u = Uarte::default();
        u.rx_byte(&sys, 0x41);
        assert_eq!(u.read(&sys, 0x108), 1);
        assert_eq!(u.read(&sys, 0x518), 0x41);
    }
}
