use crate::system::System;
use super::Peripheral;

/// RADIO @ 0x40001000 (IRQ 1, BLE/802.15.4). SoftDevice is OUT OF SCOPE;
/// bare-metal model: TASKS_TXEN 0x000, TASKS_RXEN 0x004, TASKS_START 0x008,
/// TASKS_STOP 0x00C, TASKS_DISABLE 0x010, EVENTS_READY 0x100,
/// EVENTS_ADDRESS 0x104, EVENTS_PAYLOAD 0x108, EVENTS_END 0x10C,
/// EVENTS_DISABLED 0x110, CRCSTATUS 0x400, PACKETPTR 0x504,
/// FREQUENCY 0x508, TXPOWER 0x50C, MODE 0x510, STATE 0x550
/// (0=disabled, 1=RxRu, 3=Rx, 9=TxRu, 11=Tx).
/// Air model = instance loopback via driver: START in Tx stages take_tx()
/// (ptr,len); the driver moves bytes (air) and inject_rx() queues them;
/// START in Rx with a queued packet raises ADDRESS+PAYLOAD+END+CRCSTATUS.
/// No RAM access inside the model (same take/complete rule as EASYDMA).
pub struct RadioNrf {
    state: u32,
    ev_ready: bool,
    ev_address: bool,
    ev_payload: bool,
    ev_end: bool,
    ev_disabled: bool,
    crcstatus: u32,
    packetptr: u32,
    frequency: u32,
    txpower: u32,
    mode: u32,
    tx_pending: Option<(u32, u32)>,
    rx_queue: Vec<Vec<u8>>,
}

impl Default for RadioNrf {
    fn default() -> Self {
        Self { state: 0, ev_ready: false, ev_address: false, ev_payload: false,
               ev_end: false, ev_disabled: false, crcstatus: 0, packetptr: 0,
               frequency: 0, txpower: 0, mode: 0, tx_pending: None, rx_queue: Vec::new() }
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
            0x104 => self.ev_address as u32,
            0x108 => self.ev_payload as u32,
            0x10C => self.ev_end as u32,
            0x110 => self.ev_disabled as u32,
            0x400 => self.crcstatus,
            0x504 => self.packetptr,
            0x508 => self.frequency,
            0x50C => self.txpower,
            0x510 => self.mode,
            0x550 => self.state,
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => { self.state = 9; self.ev_ready = true; } // TXEN -> TxRu
            0x004 => { self.state = 1; self.ev_ready = true; } // RXEN -> RxRu
            0x008 => { // START
                if self.state == 9 {
                    self.state = 11; // Tx
                    self.tx_pending = Some((self.packetptr, 32)); // len via PCNF in P7
                } else if self.state == 1 {
                    self.state = 3; // Rx
                    if let Some(pkt) = self.rx_queue.first().cloned() {
                        self.rx_queue.remove(0);
                        let _ = pkt;
                        self.ev_address = true;
                        self.ev_payload = true;
                        self.ev_end = true;
                        self.crcstatus = 1;
                    }
                }
            }
            0x00C | 0x010 => { self.state = 0; self.ev_disabled = true; } // STOP/DISABLE
            0x100 => if value == 0 { self.ev_ready = false; }
            0x104 => if value == 0 { self.ev_address = false; }
            0x108 => if value == 0 { self.ev_payload = false; }
            0x10C => if value == 0 { self.ev_end = false; self.crcstatus = 0; }
            0x110 => if value == 0 { self.ev_disabled = false; }
            0x504 => self.packetptr = value,
            0x508 => self.frequency = value & 0x7F,
            0x50C => self.txpower = value,
            0x510 => self.mode = value & 0xF,
            _ => {}
        }
    }
}

fn with_radio<R>(sys: &System, f: impl FnOnce(&mut RadioNrf) -> R) -> Option<R> {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4000_1000 {
            let mut b = slot.peripheral.borrow_mut();
            if let Some(r) = b.as_any_mut().downcast_mut::<RadioNrf>() {
                return Some(f(r));
            }
            return None;
        }
    }
    None
}

/// Take a staged TX packet (ptr, len); None when idle.
pub fn take_tx(sys: &System) -> Option<(u32, u32)> {
    with_radio(sys, |r| r.tx_pending.take()).flatten()
}

/// Inject a received packet (air -> RX queue).
pub fn inject_rx(sys: &System, pkt: Vec<u8>) {
    with_radio(sys, |r| r.rx_queue.push(pkt));
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
        r.write(&sys, 0x00C, 1);
        assert_eq!(r.read(&sys, 0x110), 1);
        assert_eq!(r.read(&sys, 0x550), 0);
    }
    #[test]
    fn loopback_tx_to_rx() {
        use crate::system::test_dummy_system;
        let sys = test_dummy_system();
        sys.p.write(&sys, 0x40001504, 4, 0x20001000); // PACKETPTR
        sys.p.write(&sys, 0x40001000, 4, 1);          // TXEN
        sys.p.write(&sys, 0x40001008, 4, 1);          // START (Tx)
        let t = take_tx(&sys).expect("tx staged");
        assert_eq!(t.0, 0x20001000);
        // air: loop the packet back (payload bytes travel driver-side)
        inject_rx(&sys, vec![0xAA, 0xBB, 0xCC]);
        sys.p.write(&sys, 0x4000100C, 4, 1);          // STOP/DISABLE
        sys.p.write(&sys, 0x40001004, 4, 1);          // RXEN
        sys.p.write(&sys, 0x40001008, 4, 1);          // START (Rx)
        assert_eq!(sys.p.read(&sys, 0x4000110C, 4), 1, "END after RX packet");
        assert_eq!(sys.p.read(&sys, 0x40001400, 4), 1, "CRCSTATUS ok");
    }
}
