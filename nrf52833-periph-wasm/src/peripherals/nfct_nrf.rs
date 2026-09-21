use crate::system::System;
use super::Peripheral;

/// NFCT @ 0x40005000 (IRQ 5, NFC-A tag). Offsets from nrf52833.svd
/// (note: SELECTED is 0x14C and RXFRAMESTART is 0x114 -- earlier stubs
/// had 0x114/0x104): TASKS_ACTIVATE 0x000, TASKS_DISABLE 0x004,
/// TASKS_SENSE 0x008, TASKS_STARTTX 0x00C, TASKS_ENABLERXDATA 0x01C,
/// TASKS_GOIDLE 0x024, TASKS_GOSLEEP 0x028, EVENTS_READY 0x100,
/// EVENTS_FIELDDETECTED 0x104, EVENTS_FIELDLOST 0x108,
/// EVENTS_TXFRAMESTART 0x10C, EVENTS_TXFRAMEEND 0x110,
/// EVENTS_RXFRAMESTART 0x114, EVENTS_RXFRAMEEND 0x118, EVENTS_ERROR 0x11C,
/// EVENTS_RXERROR 0x128, EVENTS_ENDRX 0x12C, EVENTS_ENDTX 0x130,
/// EVENTS_AUTOCOLRESSTARTED 0x138, EVENTS_COLLISION 0x148,
/// EVENTS_SELECTED 0x14C, EVENTS_STARTED 0x150, INTENSET 0x304
/// (READY 0, FIELDDETECTED 1, FIELDLOST 2, TXFRAMESTART 3, TXFRAMEEND 4,
/// RXFRAMESTART 5, RXFRAMEEND 6, ERROR 7, RXERROR 10, ENDRX 11, ENDTX 12,
/// AUTOCOLRESSTARTED 14, COLLISION 16?/18, SELECTED 19, STARTED 20) /
/// CLR 0x308, ERRORSTATUS 0x404, SLEEPSTATE 0x420, FIELDPRESENT 0x43C,
/// ENABLE 0x500, FRAMEDELAYMODE 0x50C, PACKETPTR 0x510, MAXLEN 0x514.
///
/// State machine: Disabled --SENSE--> Sense (field detector on) --
/// field arrives--> FieldDetected --ACTIVATE--> Selected/Active --
/// frames via PACKETPTR/MAXLEN EASYDMA --FIELDLOST--> back to Sense.
/// GOIDLE/GOSLEEP park in sleep state (woken by SENSE). DISABLE always
/// returns to Disabled and clears events. The RF field comes from the
/// host (`nfct_field_present`, i.e. a phone tapped): FIELDPRESENT
/// mirrors it live. Frame bytes move driver-side (take/complete), like
/// every EASYDMA peripheral here; the driver also feeds received bytes
/// into RAM before completing RX.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    Disabled,
    Sense,
    Field,
    Active,
    Sleep,
}

pub struct NfctNrf {
    enabled: bool,
    state: State,
    field_present: bool,
    ev_ready: bool,
    ev_fielddet: bool,
    ev_fieldlost: bool,
    ev_txstart: bool,
    ev_txend: bool,
    ev_rxstart: bool,
    ev_rxend: bool,
    ev_error: bool,
    ev_rxerror: bool,
    ev_endrx: bool,
    ev_endtx: bool,
    ev_autocol: bool,
    ev_collision: bool,
    ev_selected: bool,
    ev_started: bool,
    intenset: u32,
    errorstatus: u32,
    packetptr: u32,
    maxlen: u32,
    framedelaymode: u32,
    tx_pending: bool,
    rx_pending: bool,
    rx_amount: u32,
}

impl Default for NfctNrf {
    fn default() -> Self {
        Self {
            enabled: false, state: State::Disabled, field_present: false,
            ev_ready: false, ev_fielddet: false, ev_fieldlost: false,
            ev_txstart: false, ev_txend: false, ev_rxstart: false,
            ev_rxend: false, ev_error: false, ev_rxerror: false,
            ev_endrx: false, ev_endtx: false, ev_autocol: false,
            ev_collision: false, ev_selected: false, ev_started: false,
            intenset: 0, errorstatus: 0, packetptr: 0, maxlen: 0,
            framedelaymode: 0, tx_pending: false, rx_pending: false,
            rx_amount: 0,
        }
    }
}

impl NfctNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "NFCT" { Some(Box::new(Self::default())) } else { None }
    }
    fn fire(&self, sys: &System, bit: u32) {
        if self.intenset & bit != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(5);
        }
    }
    fn set_field(&mut self, sys: &System, present: bool) {
        self.field_present = present;
        if !self.enabled {
            return;
        }
        // Antenna pins routed to GPIO (UICR.NFCPINS PROTECT=0): no RF
        // front-end, so no field events — silicon-identical gating.
        if present && !nfcpins_nfc(sys) {
            return;
        }
        if present {
            if self.state == State::Sense {
                self.state = State::Field;
                self.ev_fielddet = true;
                self.fire(sys, 1 << 1);
            }
        } else {
            if self.state == State::Field || self.state == State::Active {
                self.state = State::Sense;
                self.tx_pending = false;
                self.rx_pending = false;
                self.ev_fieldlost = true;
                self.fire(sys, 1 << 2);
            }
        }
    }
}

impl Peripheral for NfctNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x100 => self.ev_ready as u32,
            0x104 => self.ev_fielddet as u32,
            0x108 => self.ev_fieldlost as u32,
            0x10C => self.ev_txstart as u32,
            0x110 => self.ev_txend as u32,
            0x114 => self.ev_rxstart as u32,
            0x118 => self.ev_rxend as u32,
            0x11C => self.ev_error as u32,
            0x128 => self.ev_rxerror as u32,
            0x12C => self.ev_endrx as u32,
            0x130 => self.ev_endtx as u32,
            0x138 => self.ev_autocol as u32,
            0x148 => self.ev_collision as u32,
            0x14C => self.ev_selected as u32,
            0x150 => self.ev_started as u32,
            0x304 => self.intenset,
            0x404 => self.errorstatus,
            0x420 => (self.state == State::Sleep) as u32,
            0x43C => self.field_present as u32,
            0x500 => self.enabled as u32,
            0x50C => self.framedelaymode,
            0x510 => self.packetptr,
            0x514 => self.maxlen,
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => {
                // ACTIVATE: only from Field state with the field up.
                if self.enabled && self.state == State::Field && self.field_present {
                    self.state = State::Active;
                    self.ev_selected = true;
                    self.fire(sys, 1 << 19);
                    self.ev_started = true;
                    self.fire(sys, 1 << 20);
                }
            }
            0x004 => {
                // DISABLE: full stop, events cleared.
                self.state = State::Disabled;
                self.tx_pending = false;
                self.rx_pending = false;
                self.ev_ready = false;
                self.ev_fielddet = false;
                self.ev_fieldlost = false;
                self.ev_txstart = false;
                self.ev_txend = false;
                self.ev_rxstart = false;
                self.ev_rxend = false;
                self.ev_error = false;
                self.ev_rxerror = false;
                self.ev_endrx = false;
                self.ev_endtx = false;
                self.ev_autocol = false;
                self.ev_collision = false;
                self.ev_selected = false;
                self.ev_started = false;
            }
            0x008 => {
                // SENSE: field detector on; READY announces it.
                if self.enabled {
                    self.state = State::Sense;
                    self.ev_ready = true;
                    self.fire(sys, 1 << 0);
                    // Field already up (phone waiting): detect at once.
                    if self.field_present {
                        self.state = State::Field;
                        self.ev_fielddet = true;
                        self.fire(sys, 1 << 1);
                    }
                }
            }
            0x00C => {
                // STARTTX: send PACKETPTR[..MAXLEN] when active.
                if self.enabled && self.state == State::Active && self.maxlen > 0 {
                    self.tx_pending = true;
                    self.ev_txstart = true;
                    self.fire(sys, 1 << 3);
                }
            }
            0x01C => {
                // ENABLERXDATA: arm the RX buffer when active.
                if self.enabled && self.state == State::Active && self.maxlen > 0 {
                    self.rx_pending = true;
                    self.rx_amount = 0;
                    self.ev_rxstart = true;
                    self.fire(sys, 1 << 5);
                }
            }
            0x024 | 0x028 => {
                // GOIDLE/GOSLEEP: park (SENSE wakes).
                if self.enabled {
                    self.state = State::Sleep;
                    self.tx_pending = false;
                    self.rx_pending = false;
                }
            }
            0x100 => if value == 0 { self.ev_ready = false; }
            0x104 => if value == 0 { self.ev_fielddet = false; }
            0x108 => if value == 0 { self.ev_fieldlost = false; }
            0x10C => if value == 0 { self.ev_txstart = false; }
            0x110 => if value == 0 { self.ev_txend = false; }
            0x114 => if value == 0 { self.ev_rxstart = false; }
            0x118 => if value == 0 { self.ev_rxend = false; }
            0x11C => if value == 0 { self.ev_error = false; }
            0x128 => if value == 0 { self.ev_rxerror = false; }
            0x12C => if value == 0 { self.ev_endrx = false; }
            0x130 => if value == 0 { self.ev_endtx = false; }
            0x138 => if value == 0 { self.ev_autocol = false; }
            0x148 => if value == 0 { self.ev_collision = false; }
            0x14C => if value == 0 { self.ev_selected = false; }
            0x150 => if value == 0 { self.ev_started = false; }
            0x304 => self.intenset |= value & 0x1C_5CFF,
            0x308 => self.intenset &= !value,
            0x404 => self.errorstatus &= !value, // write-1-clears
            0x500 => {
                self.enabled = value & 1 == 1;
                if !self.enabled {
                    self.state = State::Disabled;
                }
            }
            0x50C => self.framedelaymode = value & 3,
            0x510 => self.packetptr = value,
            0x514 => self.maxlen = value & 0xFF,
            _ => {}
        }
    }
}

fn with_nfct<R>(sys: &System, f: impl FnOnce(&mut NfctNrf) -> R) -> Option<R> {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4000_5000 {
            // try_borrow_mut (P108 family): take/complete paths re-enter
            // via read/write/tick while borrowed; drop instead of panic.
            let mut b = match slot.peripheral.try_borrow_mut() {
                Ok(b) => b,
                Err(_) => return None,
            };
            if let Some(n) = b.as_any_mut().downcast_mut::<NfctNrf>() {
                return Some(f(n));
            }
            return None;
        }
    }
    None
}

/// UICR.NFCPINS (0x1000120C, bit 0 PROTECT): 1 = pins P0.09/P0.10 are
/// the NFC antenna (reset value 0xFFFFFFFF, i.e. NFC); 0 = GPIO.
/// Read live from the UICR slot so `periph_write` seeds apply.
fn nfcpins_nfc(sys: &System) -> bool {
    for slot in &sys.p.peripherals {
        if slot.start == 0x1000_1000 {
            let v = slot.peripheral.borrow_mut().read(sys, 0x20C);
            return v & 1 != 0;
        }
    }
    true
}

/// True when P0.09/P0.10 are currently NFC antenna pins (not GPIO).
/// Exported so the GPIO bank can refuse NFC-pin traffic the same way.
pub fn nfct_pins_reserved(sys: &System) -> bool {
    nfcpins_nfc(sys)
}

/// Host side of the RF field (a phone tapped / removed). Drives
/// FIELDDETECTED/FIELDLOST + FIELDPRESENT like the analog front-end.
pub fn nfct_field_present(sys: &System, present: bool) {
    with_nfct(sys, |n| n.set_field(sys, present));
}

/// Take a staged TX frame (ptr, maxcnt); None when idle.
pub fn take_nfct_tx(sys: &System) -> Option<(u32, u32)> {
    with_nfct(sys, |n| {
        if n.tx_pending {
            n.tx_pending = false;
            Some((n.packetptr, n.maxlen))
        } else {
            None
        }
    })
    .flatten()
}

/// Complete TX: frame went on air; sets TXFRAMEEND + ENDTX.
pub fn complete_nfct_tx(sys: &System) {
    with_nfct(sys, |n| {
        n.ev_txend = true;
        n.fire(sys, 1 << 4);
        n.ev_endtx = true;
        n.fire(sys, 1 << 12);
    });
}

/// Take a staged RX buffer (ptr, maxcnt); None when idle.
pub fn take_nfct_rx(sys: &System) -> Option<(u32, u32)> {
    with_nfct(sys, |n| {
        if n.rx_pending {
            n.rx_pending = false;
            Some((n.packetptr, n.maxlen))
        } else {
            None
        }
    })
    .flatten()
}

/// Complete RX: driver wrote `amount` bytes at PTR; sets RXFRAMEEND + ENDRX.
pub fn complete_nfct_rx(sys: &System, amount: u32) {
    with_nfct(sys, |n| {
        n.rx_amount = amount;
        n.ev_rxend = true;
        n.fire(sys, 1 << 6);
        n.ev_endrx = true;
        n.fire(sys, 1 << 11);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn sense_activate() {
        // Legacy handshake keeps working (offsets moved to SVD truth).
        let sys = test_dummy_system();
        sys.p.write(&sys, 0x40005500, 4, 1); // ENABLE
        sys.p.write(&sys, 0x40005008, 4, 1); // SENSE
        assert_eq!(sys.p.read(&sys, 0x40005100, 4), 1, "READY");
        assert_eq!(sys.p.read(&sys, 0x40005104, 4), 0, "no field yet");
        nfct_field_present(&sys, true);
        assert_eq!(sys.p.read(&sys, 0x40005104, 4), 1, "FIELDDETECTED");
        assert_eq!(sys.p.read(&sys, 0x4000543C, 4), 1, "FIELDPRESENT");
        sys.p.write(&sys, 0x40005000, 4, 1); // ACTIVATE
        assert_eq!(sys.p.read(&sys, 0x4000514C, 4), 1, "SELECTED at 0x14C");
        assert_eq!(sys.p.read(&sys, 0x40005150, 4), 1, "STARTED");
    }
    #[test]
    fn frame_exchange_and_fieldlost() {
        let sys = test_dummy_system();
        sys.p.write(&sys, 0xE000E100, 4, 1 << 5); // NVIC ISER: NFCT
        sys.p.write(&sys, 0x40005304, 4, (1 << 4) | (1 << 6) | (1 << 2)); // INTEN TXEND/RXEND/FIELDLOST
        sys.p.write(&sys, 0x40005500, 4, 1);
        sys.p.write(&sys, 0x40005008, 4, 1);
        nfct_field_present(&sys, true);
        sys.p.write(&sys, 0x40005000, 4, 1);
        // TX a frame.
        sys.p.write(&sys, 0x40005510, 4, 0x20001000); // PACKETPTR
        sys.p.write(&sys, 0x40005514, 4, 4); // MAXLEN
        sys.p.write(&sys, 0x4000500C, 4, 1); // STARTTX
        assert_eq!(sys.p.read(&sys, 0x4000510C, 4), 1, "TXFRAMESTART");
        assert_eq!(take_nfct_tx(&sys), Some((0x20001000, 4)));
        complete_nfct_tx(&sys);
        assert_eq!(sys.p.read(&sys, 0x40005110, 4), 1, "TXFRAMEEND");
        assert_eq!(sys.p.read(&sys, 0x40005130, 4), 1, "ENDTX");
        assert!(sys.p.nvic.borrow().has_pending(), "IRQ 5 pends");
        // RX a frame.
        sys.p.write(&sys, 0x4000501C, 4, 1); // ENABLERXDATA
        assert_eq!(take_nfct_rx(&sys), Some((0x20001000, 4)));
        complete_nfct_rx(&sys, 4);
        assert_eq!(sys.p.read(&sys, 0x40005118, 4), 1, "RXFRAMEEND");
        assert_eq!(sys.p.read(&sys, 0x4000512C, 4), 1, "ENDRX");
        // Phone leaves: back to Sense, transfers cancelled.
        nfct_field_present(&sys, false);
        assert_eq!(sys.p.read(&sys, 0x40005108, 4), 1, "FIELDLOST");
        assert_eq!(sys.p.read(&sys, 0x4000543C, 4), 0, "field gone");
        assert_eq!(take_nfct_tx(&sys), None, "TX cancelled");
        // 2nd run: fresh instance, no leak.
        let n2 = NfctNrf::default();
        assert_eq!(n2.state, State::Disabled);
    }
    #[test]
    fn nfcpins_gate_routes_pins_vs_antenna() {
        // UICR.NFCPINS PROTECT (0x1000120C bit 0): 1 = P0.09/P0.10 are
        // the NFC antenna (reset 0xFFFFFFFF); 0 = GPIO. The gate is
        // live: default (erased UICR) reserves the pins + senses field;
        // clearing PROTECT releases pins to GPIO and kills field events.
        let sys = test_dummy_system();
        assert!(nfct_pins_reserved(&sys), "reset = antenna");
        sys.p.write(&sys, 0x40005500, 4, 1); // ENABLE
        sys.p.write(&sys, 0x40005008, 4, 1); // SENSE
        nfct_field_present(&sys, true);
        assert_eq!(sys.p.read(&sys, 0x40005104, 4), 1, "FIELDDETECTED via antenna");
        // Release to GPIO: field no longer detected...
        sys.p.write(&sys, 0x1000120C, 4, 0);
        assert!(!nfct_pins_reserved(&sys), "PROTECT=0 = GPIO");
        sys.p.write(&sys, 0x40005104, 4, 0);
        nfct_field_present(&sys, false);
        nfct_field_present(&sys, true);
        assert_eq!(sys.p.read(&sys, 0x40005104, 4), 0, "no field on GPIO pins");
        // ...and GPIO config on P0.09 works again.
        sys.p.write(&sys, 0x50000724, 4, 0x1); // PIN_CNF[9] DIR=output
        assert_eq!(sys.p.read(&sys, 0x50000514, 4) & (1 << 9), 1 << 9, "P0.09 DIR");
        // Antenna back: config refused.
        sys.p.write(&sys, 0x1000120C, 4, 1);
        sys.p.write(&sys, 0x50000724, 4, 0x0);
        assert_eq!(sys.p.read(&sys, 0x50000514, 4) & (1 << 9), 1 << 9, "DIR sticky under antenna");
    }
}
