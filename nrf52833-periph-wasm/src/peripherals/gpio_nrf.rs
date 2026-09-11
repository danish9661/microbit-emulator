use crate::system::System;
use super::Peripheral;


/// nRF GPIO: P0 @ 0x50000000 (32 pins) + P1 @ 0x50000300 (10 pins on 52833).
/// Nordic layout per port (offsets from port base):
///   0x504 OUT, 0x508 OUTSET, 0x50C OUTCLR, 0x510 IN, 0x514 DIR, 0x518 DIRSET,
///   0x51C DIRCLR, 0x51F? + 0x700+ PIN_CNF[32].
/// Plus shared GpioPorts state for JS/test input + output observation
/// (matrix renderer + buttons read these).
pub struct GpioPorts {
    pub out: [u32; 2],
    pub dir: [u32; 2],
    pub input_state: [u32; 2],
    pub cnf: [[u32; 32]; 2],
}

impl Default for GpioPorts {
    fn default() -> Self {
        Self { out: [0; 2], dir: [0; 2], input_state: [0; 2], cnf: [[0; 32]; 2] }
    }
}

impl GpioPorts {
    pub fn read_output_pin(&self, port: u8, pin: u8) -> bool {
        if (port as usize) < 2 && pin < 32 {
            (self.out[port as usize] >> pin) & 1 == 1
        } else { false }
    }
    pub fn set_input_pin(&mut self, port: u8, pin: u8, value: bool) {
        if (port as usize) < 2 && pin < 32 {
            if value { self.input_state[port as usize] |= 1 << pin; }
            else { self.input_state[port as usize] &= !(1 << pin); }
        }
    }
    pub fn read_input_pin(&self, port: u8, pin: u8) -> bool {
        if (port as usize) < 2 && pin < 32 {
            (self.input_state[port as usize] >> pin) & 1 == 1
        } else { false }
    }
    /// Full 32-bit port level for tap helpers (DC pin sampling): output
    /// latch (JS matrix/DC reads drive outputs).
    pub fn read_port(&self, _sys: &System, port: u8) -> u32 {
        if (port as usize) < 2 { self.out[port as usize] } else { 0 }
    }
    fn port_out(&self, port: usize) -> u32 { self.out[port] }
}

/// P0 model (port 0). P1 reuses same struct with port index 1.
pub struct GpioNrf {
    port: usize,
}

impl GpioNrf {
    pub fn new_p0(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "P0" || name == "GPIO" { Some(Box::new(Self { port: 0 })) } else { None }
    }
    pub fn new_p1(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "P1" { Some(Box::new(Self { port: 1 })) } else { None }
    }
    /// Combined GPIO block at 0x50000000: P0 regs at +0x500, P1 regs at +0x800
    /// (P1 base 0x50000300 + 0x500). Returns (port, reg_offset) or None.
    fn decode(offset: u32) -> Option<(usize, u32)> {
        if offset >= 0x500 && offset < 0x600 {
            Some((0, offset - 0x500))
        } else if offset >= 0x800 && offset < 0x900 {
            Some((1, offset - 0x800))
        } else if offset >= 0x700 && offset < 0x780 {
            return None; // legacy P0 CNF path below
        } else if offset >= 0xA00 && offset < 0xA80 {
            return None; // legacy P1 CNF path below
        } else {
            None
        }
    }
    fn cnf_idx(offset: u32) -> Option<(usize, usize)> {
        if offset >= 0x700 && offset < 0x780 && (offset & 3) == 0 {
            Some((0, ((offset - 0x700) >> 2) as usize))
        } else if offset >= 0xA00 && offset < 0xA80 && (offset & 3) == 0 {
            Some((1, ((offset - 0xA00) >> 2) as usize))
        } else {
            None
        }
    }
}

impl Peripheral for GpioNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, sys: &System, offset: u32) -> u32 {
        let gpio = sys.p.gpio.borrow();
        if let Some((port, i)) = Self::cnf_idx(offset) {
            return if i < 32 { gpio.cnf[port][i] } else { 0 };
        }
        // Combined block: decode port from absolute offset; single-port
        // slots (SVD path) fall back to self.port.
        let (port, reg) = Self::decode(offset).unwrap_or((self.port, offset));
        if let Some((_, i)) = Self::cnf_idx(offset) {
            let _ = i;
        }
        match reg {
            0x04 => gpio.port_out(port),
            0x10 => {
                let out = gpio.out[port];
                let dir = gpio.dir[port];
                let inp = gpio.input_state[port];
                (out & dir) | (inp & !dir)
            }
            0x14 => gpio.dir[port],
            _ => {
                // Legacy per-port offsets (0x504/0x510/0x514 from port base)
                match offset {
                    0x504 => gpio.port_out(port),
                    0x510 => {
                        let out = gpio.out[port];
                        let dir = gpio.dir[port];
                        let inp = gpio.input_state[port];
                        (out & dir) | (inp & !dir)
                    }
                    0x514 => gpio.dir[port],
                    _ => 0,
                }
            }
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        if let Some((port, i)) = Self::cnf_idx(offset) {
            if i < 32 { sys.p.gpio.borrow_mut().cnf[port][i] = value; }
            return;
        }
        let (port, reg) = Self::decode(offset).unwrap_or((self.port, offset));
        let mut gpio = sys.p.gpio.borrow_mut();
        match reg {
            0x04 => gpio.out[port] = value,
            0x08 => gpio.out[port] |= value,
            0x0C => gpio.out[port] &= !value,
            0x14 => gpio.dir[port] = value,
            0x18 => gpio.dir[port] |= value,
            0x1C => gpio.dir[port] &= !value,
            _ => {
                match offset {
                    0x504 => gpio.out[port] = value,
                    0x508 => gpio.out[port] |= value,
                    0x50C => gpio.out[port] &= !value,
                    0x514 => gpio.dir[port] = value,
                    0x518 => gpio.dir[port] |= value,
                    0x51C => gpio.dir[port] &= !value,
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn p0_outset_clr_and_in() {
        let sys = test_dummy_system();
        let mut p0 = GpioNrf { port: 0 };
        p0.write(&sys, 0x514, 0x1); // DIR = output pin0
        p0.write(&sys, 0x508, 0x1); // OUTSET
        assert_eq!(p0.read(&sys, 0x504), 0x1);
        assert_eq!(p0.read(&sys, 0x510), 0x1);
        p0.write(&sys, 0x50C, 0x1); // OUTCLR
        assert_eq!(p0.read(&sys, 0x504), 0x0);
        // input pin1 driven high from test harness
        sys.p.gpio.borrow_mut().set_input_pin(0, 1, true);
        assert_eq!(p0.read(&sys, 0x510) & 0x2, 0x2);
    }
}
