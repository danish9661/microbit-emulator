use crate::system::System;
use super::Peripheral;

/// USBD @ 0x40027000 (IRQ 39). P5 stub: ENABLE 0x500, USBPULLUP 0x504,
/// DPDMVALUE 0x508, EVENTS_USBRESET 0x100, EVENTS_STARTED 0x104,
/// EVENTS_ENDEPIN[n] 0x1C0+n*4, TASKS_STARTEPIN[n] 0x000+n*4.
/// Endpoint DMA + descriptors are P6; P4/P5 firmware only enables the
/// pull-up and polls STARTED (enumeration is JS-host owned).
pub struct UsbdNrf {
    enabled: bool,
    pullup: bool,
    ev_started: bool,
}

impl Default for UsbdNrf {
    fn default() -> Self {
        Self { enabled: false, pullup: false, ev_started: false }
    }
}

impl UsbdNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "USBD" { Some(Box::new(Self::default())) } else { None }
    }
}

impl Peripheral for UsbdNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x100 => 0, // USBRESET: host-driven, P6
            0x104 => self.ev_started as u32,
            0x500 => self.enabled as u32,
            0x504 => self.pullup as u32,
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        match offset {
            0x000..=0x01C => self.ev_started = true, // STARTEPIN[n]
            0x104 => if value == 0 { self.ev_started = false; }
            0x500 => self.enabled = value & 1 == 1,
            0x504 => self.pullup = value & 1 == 1,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn enable_pullup_startep() {
        let sys = test_dummy_system();
        let mut u = UsbdNrf::default();
        u.write(&sys, 0x500, 1);
        u.write(&sys, 0x504, 1);
        u.write(&sys, 0x000, 1);
        assert_eq!(u.read(&sys, 0x104), 1);
        assert_eq!(u.read(&sys, 0x504), 1);
    }
}
