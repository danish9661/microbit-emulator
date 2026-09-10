use crate::system::System;
use super::Peripheral;

/// USBD @ 0x40027000 (IRQ 39). P5 stub + P6 enumeration events:
/// ENABLE 0x500, USBPULLUP 0x504, DPDMVALUE 0x508,
/// EVENTS_USBRESET 0x100 (host-driven: signal_usbreset()),
/// EVENTS_STARTED 0x104, EVENTS_ENDEPIN[n] 0x1C0+n*4,
/// EPDATASTATUS 0x1C8, TASKS_STARTEPIN[n] 0x000+n*4.
/// Endpoint DMA + descriptors are P7; enumeration handshake (reset ->
/// started -> endpoint events) is fully model-side.
pub struct UsbdNrf {
    enabled: bool,
    pullup: bool,
    ev_usbreset: bool,
    ev_started: bool,
}

impl Default for UsbdNrf {
    fn default() -> Self {
        Self { enabled: false, pullup: false, ev_usbreset: false, ev_started: false }
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
            0x100 => self.ev_usbreset as u32,
            0x104 => self.ev_started as u32,
            0x500 => self.enabled as u32,
            0x504 => self.pullup as u32,
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        match offset {
            0x000..=0x01C => self.ev_started = true, // STARTEPIN[n]
            0x100 => if value == 0 { self.ev_usbreset = false; }
            0x104 => if value == 0 { self.ev_started = false; }
            0x500 => self.enabled = value & 1 == 1,
            0x504 => self.pullup = value & 1 == 1,
            _ => {}
        }
    }
}

/// Host-side USB reset injection (enumeration start): sets EVENTS_USBRESET.
pub fn signal_usbreset(sys: &System) {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4002_7000 {
            let mut b = slot.peripheral.borrow_mut();
            if let Some(u) = b.as_any_mut().downcast_mut::<UsbdNrf>() {
                u.ev_usbreset = true;
            }
            return;
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
    #[test]
    fn usbreset_injection_and_clear() {
        use crate::system::test_dummy_system;
        let sys = test_dummy_system();
        assert_eq!(sys.p.read(&sys, 0x40027100, 4), 0);
        signal_usbreset(&sys);
        assert_eq!(sys.p.read(&sys, 0x40027100, 4), 1, "USBRESET set");
        sys.p.write(&sys, 0x40027100, 4, 0);
        assert_eq!(sys.p.read(&sys, 0x40027100, 4), 0, "clear by write-0");
    }
}
