use crate::system::System;
use super::Peripheral;

/// MWU @ 0x40020000 (IRQ 32, memory watch unit). REGIONEN 0x500,
/// EVENTS_REGION[n] / PREGION RA/WA events. Stub: REGIONEN RW, events
/// never fire (no watch configured = no surprise faults for firmware
/// that leaves the MWU at reset).
pub struct MwuNrf {
    regionen: u32,
}

impl Default for MwuNrf {
    fn default() -> Self {
        Self { regionen: 0 }
    }
}

impl MwuNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "MWU" { Some(Box::new(Self::default())) } else { None }
    }
}

impl Peripheral for MwuNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x500 => self.regionen,
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        if offset == 0x500 {
            self.regionen = value;
        }
    }
}

/// ECB @ 0x4000E000 (IRQ 14, AES engine). TASKS_CRYPT 0x000,
/// EVENTS_END 0x100, EVENTS_ERROR 0x104, ECBDATAPTR 0x504.
/// Stub: CRYPT->END immediately (no crypto performed; P8 encrypts via
/// a software AES pass over the pointed block, driver-side like EASYDMA).
pub struct EcbNrf {
    ev_end: bool,
    dataptr: u32,
}

impl Default for EcbNrf {
    fn default() -> Self {
        Self { ev_end: false, dataptr: 0 }
    }
}

impl EcbNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "ECB" { Some(Box::new(Self::default())) } else { None }
    }
}

impl Peripheral for EcbNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x100 => self.ev_end as u32,
            0x504 => self.dataptr,
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => self.ev_end = true,
            0x100 => if value == 0 { self.ev_end = false; }
            0x504 => self.dataptr = value,
            _ => {}
        }
    }
}

/// AAR/CCM @ 0x4000F000 (IRQ 15, shared base like CLOCK/POWER).
/// TASKS_START 0x000, TASKS_STOP 0x004, EVENTS_END 0x100,
/// EVENTS_RESOLVED 0x104, EVENTS_NOTRESOLVED 0x108, ENABLE 0x500.
/// Stub: START->END (address resolution always "not resolved" until P8).
pub struct AarCcmNrf {
    enabled: bool,
    ev_end: bool,
}

impl Default for AarCcmNrf {
    fn default() -> Self {
        Self { enabled: false, ev_end: false }
    }
}

impl AarCcmNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "AAR" || name == "CCM" {
            Some(Box::new(Self::default()))
        } else {
            None
        }
    }
}

impl Peripheral for AarCcmNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x100 => self.ev_end as u32,
            0x500 => self.enabled as u32,
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => self.ev_end = true,
            0x100 => if value == 0 { self.ev_end = false; }
            0x500 => self.enabled = value & 1 == 1,
            _ => {}
        }
    }
}

/// I2S @ 0x40025000 (IRQ 37, audio). TASKS_START 0x000, TASKS_STOP 0x004,
/// EVENTS_RXPTRUPD 0x100, EVENTS_TXPTRUPD 0x104, EVENTS_STOPPED 0x108,
/// RXD.PTR 0x538, TXD.PTR 0x540, ENABLE 0x500, CONFIG.*.
/// Stub: START->TXPTRUPD+RXPTRUPD, STOP->STOPPED. Sample streaming is P8.
pub struct I2sNrf {
    enabled: bool,
    ev_rxptr: bool,
    ev_txptr: bool,
    ev_stopped: bool,
}

impl Default for I2sNrf {
    fn default() -> Self {
        Self { enabled: false, ev_rxptr: false, ev_txptr: false, ev_stopped: false }
    }
}

impl I2sNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "I2S" { Some(Box::new(Self::default())) } else { None }
    }
}

impl Peripheral for I2sNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x100 => self.ev_rxptr as u32,
            0x104 => self.ev_txptr as u32,
            0x108 => self.ev_stopped as u32,
            0x500 => self.enabled as u32,
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => { self.ev_rxptr = true; self.ev_txptr = true; }
            0x004 => self.ev_stopped = true,
            0x100 => if value == 0 { self.ev_rxptr = false; }
            0x104 => if value == 0 { self.ev_txptr = false; }
            0x108 => if value == 0 { self.ev_stopped = false; }
            0x500 => self.enabled = value & 1 == 1,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn misc_stubs_handshake() {
        let sys = test_dummy_system();
        let mut m = MwuNrf::default();
        m.write(&sys, 0x500, 0x0F);
        assert_eq!(m.read(&sys, 0x500), 0x0F);
        let mut e = EcbNrf::default();
        e.write(&sys, 0x000, 1);
        assert_eq!(e.read(&sys, 0x100), 1);
        let mut a = AarCcmNrf::default();
        a.write(&sys, 0x500, 1);
        a.write(&sys, 0x000, 1);
        assert_eq!(a.read(&sys, 0x100), 1);
        let mut i = I2sNrf::default();
        i.write(&sys, 0x500, 1);
        i.write(&sys, 0x000, 1);
        assert_eq!(i.read(&sys, 0x104), 1);
        i.write(&sys, 0x004, 1);
        assert_eq!(i.read(&sys, 0x108), 1);
    }
}
