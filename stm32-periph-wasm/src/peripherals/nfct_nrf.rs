use crate::system::System;
use super::Peripheral;

/// NFCT @ 0x40005000 (IRQ 5, NFC tag). TASKS_ACTIVATE 0x000,
/// TASKS_DISABLE 0x004, TASKS_SENSE 0x008, TASKS_STARTTX 0x00C,
/// TASKS_ENABLERXDATA 0x01C, EVENTS_FIELDDETECTED 0x100,
/// EVENTS_FIELDLOST 0x104, EVENTS_SELECTED 0x114, EVENTS_RXFRAMEEND 0x11C,
/// EVENTS_TXFRAMEEND 0x120, EVENTS_READY 0x12C, ENABLE 0x500.
/// Stub: SENSE->FIELDDETECTED (host can clear), ACTIVATE->SELECTED.
/// Frame DMA is P8 (needs tag memory + RF analog, host-owned).
pub struct NfctNrf {
    enabled: bool,
    ev_field: bool,
    ev_selected: bool,
}

impl Default for NfctNrf {
    fn default() -> Self {
        Self { enabled: false, ev_field: false, ev_selected: false }
    }
}

impl NfctNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "NFCT" { Some(Box::new(Self::default())) } else { None }
    }
}

impl Peripheral for NfctNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x100 => self.ev_field as u32,
            0x114 => self.ev_selected as u32,
            0x500 => self.enabled as u32,
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => self.ev_selected = true,
            0x004 => { self.ev_field = false; self.ev_selected = false; }
            0x008 => self.ev_field = true,
            0x100 => if value == 0 { self.ev_field = false; }
            0x114 => if value == 0 { self.ev_selected = false; }
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
    fn sense_activate() {
        let sys = test_dummy_system();
        let mut n = NfctNrf::default();
        n.write(&sys, 0x008, 1);
        assert_eq!(n.read(&sys, 0x100), 1);
        n.write(&sys, 0x000, 1);
        assert_eq!(n.read(&sys, 0x114), 1);
    }
}
