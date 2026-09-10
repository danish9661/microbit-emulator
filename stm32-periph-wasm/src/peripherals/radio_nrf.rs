use crate::system::System;
use super::Peripheral;

/// RADIO @ 0x40001000 (IRQ 1, BLE/802.15.4). P5 stub — SoftDevice is OUT
/// OF SCOPE, bare-metal RADIO last: TASKS_TXEN 0x000, TASKS_RXEN 0x004,
/// TASKS_START 0x008, TASKS_STOP 0x00C, TASKS_DISABLE 0x010,
/// EVENTS_READY 0x100, EVENTS_ADDRESS 0x104, EVENTS_END 0x10C,
/// EVENTS_DISABLED 0x110, STATE 0x50C (0=disabled,1=RxRu,3=Rx,9=TxRu,11=Tx),
/// MODE 0x50C? (MODE 0x510), FREQUENCY 0x508, TXPOWER 0x50C-adjacent.
/// Packet buffer DMA is P6; P5 proves task->event handshake.
pub struct RadioNrf {
    state: u32,
    ev_ready: bool,
    ev_end: bool,
    ev_disabled: bool,
}

impl Default for RadioNrf {
    fn default() -> Self {
        Self { state: 0, ev_ready: false, ev_end: false, ev_disabled: false }
    }
}

impl RadioNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "RADIO" { Some(Box::new(Self::default())) } else { None }
    }
}

impl Peripheral for RadioNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x100 => self.ev_ready as u32,
            0x10C => self.ev_end as u32,
            0x110 => self.ev_disabled as u32,
            0x50C => self.state,
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => { self.state = 9; self.ev_ready = true; } // TXEN -> TxRu
            0x004 => { self.state = 1; self.ev_ready = true; } // RXEN -> RxRu
            0x008 => { // START: Ru->Tx/Rx, immediately END (no air model yet)
                if self.state == 9 { self.state = 11; }
                else if self.state == 1 { self.state = 3; }
                self.ev_end = true;
            }
            0x00C | 0x010 => { self.state = 0; self.ev_disabled = true; } // STOP/DISABLE
            0x100 => if value == 0 { self.ev_ready = false; }
            0x10C => if value == 0 { self.ev_end = false; }
            0x110 => if value == 0 { self.ev_disabled = false; }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn txen_start_stop_chain() {
        let sys = test_dummy_system();
        let mut r = RadioNrf::default();
        r.write(&sys, 0x000, 1);
        assert_eq!(r.read(&sys, 0x100), 1);
        r.write(&sys, 0x008, 1);
        assert_eq!(r.read(&sys, 0x10C), 1);
        r.write(&sys, 0x00C, 1);
        assert_eq!(r.read(&sys, 0x110), 1);
        assert_eq!(r.read(&sys, 0x50C), 0);
    }
}
