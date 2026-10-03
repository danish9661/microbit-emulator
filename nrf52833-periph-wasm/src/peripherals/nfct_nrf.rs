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
    shorts: u32,
    errorstatus: u32,
    packetptr: u32,
    maxlen: u32,
    framedelaymin: u32,
    framedelaymax: u32,
    framedelaymode: u32,
    modulationctrl: u32,
    modulationpsel: u32,
    nfcid1: [u32; 3], // LAST, 2ND_LAST, 3RD_LAST (0x590/594/598)
    autocolres: u32,  // AUTOCOLRESCONFIG.MODE bit0
    sensres: u32,
    selres: u32,
    framestatus_rx: u32, // FRAMESTATUS.RX: CRCERROR 0 + PARITYSTATUS 2 + OVERRUN 3
    tx_frameconfig: u32, // TXD.FRAMECONFIG: PARITY 0 + DISCARDMODE 1 + SOF 2 + CRCMODETX 4
    tx_bytes: u32,       // last staged TX length (TXD.AMOUNT 11:3)
    rx_frameconfig: u32, // RXD.FRAMECONFIG: PARITY 0 + SOF 2 + CRCMODERX 4
    rx_bytes: u32,       // last received length (RXD.AMOUNT 11:3)
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
            intenset: 0, shorts: 0, errorstatus: 0, packetptr: 0, maxlen: 0,
            framedelaymin: 0, framedelaymax: 0, framedelaymode: 0,
            modulationctrl: 0, modulationpsel: 0xFFFF_FFFF,
            nfcid1: [0; 3], autocolres: 0, sensres: 0, selres: 0,
            framestatus_rx: 0, tx_frameconfig: 0, tx_bytes: 0,
            rx_frameconfig: 0, rx_bytes: 0, tx_pending: false, rx_pending: false,
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
    /// Shared ACTIVATE effect (TASKS_ACTIVATE + FIELDDETECTED_ACTIVATE
    /// short): Field + present field -> Selected/Active with SELECTED +
    /// STARTED (+IRQs 19/20).
    fn do_activate(&mut self, sys: &System) {
        if self.enabled && self.state == State::Field && self.field_present {
            self.state = State::Active;
            self.ev_selected = true;
            self.fire(sys, 1 << 19);
            self.ev_started = true;
            self.fire(sys, 1 << 20);
        }
    }
    /// Shared SENSE effect (TASKS_SENSE + FIELDLOST_SENSE short): field
    /// detector on with READY (+IRQ 0); an already-up field detects at
    /// once; with AUTOCOLRES enabled the auto-resolution announces
    /// AUTOCOLRESSTARTED (+IRQ 14).
    fn do_sense(&mut self, sys: &System) {
        if !self.enabled {
            return;
        }
        self.state = State::Sense;
        self.ev_ready = true;
        self.fire(sys, 1 << 0);
        if self.autocolres & 1 != 0 {
            self.ev_autocol = true;
            self.fire(sys, 1 << 14);
        }
        if self.field_present {
            self.set_field(sys, true);
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
                if self.shorts & 1 != 0 {
                    self.do_activate(sys); // FIELDDETECTED_ACTIVATE
                }
            }
        } else {
            if self.state == State::Field || self.state == State::Active {
                self.state = State::Sense;
                self.tx_pending = false;
                self.rx_pending = false;
                self.ev_fieldlost = true;
                self.fire(sys, 1 << 2);
                if self.shorts & (1 << 1) != 0 {
                    self.do_sense(sys); // FIELDLOST_SENSE
                }
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
            0x200 => self.shorts,
            0x300 => self.intenset, // INTEN reads the enable word
            0x304 => self.intenset,
            0x404 => self.errorstatus,
            0x40C => self.framestatus_rx, // FRAMESTATUS.RX
            0x410 => {
                // NFCTAGSTATE: coarse tag-state mirror (SVD 0/1/2/3/4 =
                // Disabled/RampUp/Idle/Receive/Transmit). Sense/Field
                // park in Idle; Active reports the transfer direction.
                match self.state {
                    State::Disabled => 0,
                    State::Sleep => 0,
                    State::Sense | State::Field => 2,
                    State::Active => {
                        if self.tx_pending {
                            4
                        } else if self.rx_pending {
                            3
                        } else {
                            2
                        }
                    }
                }
            }
            0x420 => (self.state == State::Sleep) as u32,
            0x43C => self.field_present as u32,
            0x500 => self.enabled as u32,
            0x504 => self.framedelaymin,
            0x508 => self.framedelaymax,
            0x50C => self.framedelaymode,
            0x518 => self.tx_frameconfig, // TXD.FRAMECONFIG
            0x51C => (self.tx_bytes & 0xFF) << 3, // TXD.AMOUNT (bytes 11:3, bits 0)
            0x520 => self.rx_frameconfig, // RXD.FRAMECONFIG
            0x524 => (self.rx_bytes & 0xFF) << 3, // RXD.AMOUNT
            0x510 => self.packetptr,
            0x514 => self.maxlen,
            0x52C => self.modulationctrl,
            0x538 => self.modulationpsel,
            0x590 => self.nfcid1[0], // NFCID1_LAST
            0x594 => self.nfcid1[1], // NFCID1_2ND_LAST
            0x598 => self.nfcid1[2], // NFCID1_3RD_LAST
            0x59C => self.autocolres,
            0x5A0 => self.sensres,
            0x5A4 => self.selres,
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => self.do_activate(sys), // ACTIVATE
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
            0x008 => self.do_sense(sys), // SENSE
            0x00C => {
                // STARTTX: send PACKETPTR[..MAXLEN] when active.
                if self.enabled && self.state == State::Active && self.maxlen > 0 {
                    self.tx_pending = true;
                    self.tx_bytes = self.maxlen;
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
            0x200 => self.shorts = value & 0x23, // FIELDDET_ACT + FIELDLOST_SENSE + TXEND_ENABLERX
            0x300 => self.intenset = value & 0x1C_5CFF, // INTEN absolute
            0x304 => self.intenset |= value & 0x1C_5CFF,
            0x308 => self.intenset &= !value,
            0x404 => self.errorstatus &= !value, // write-1-clears
            0x40C => self.framestatus_rx &= !value, // write-1-clears
            0x500 => {
                self.enabled = value & 1 == 1;
                if !self.enabled {
                    self.state = State::Disabled;
                }
            }
            0x50C => self.framedelaymode = value & 3,
            0x510 => self.packetptr = value,
            0x514 => self.maxlen = value & 0xFF,
            0x518 => self.tx_frameconfig = value & 0x17,
            0x520 => self.rx_frameconfig = value & 0x15,
            0x504 => self.framedelaymin = value & 0xFFFF,
            0x508 => self.framedelaymax = value & 0xF_FFFF,
            0x52C => self.modulationctrl = value & 3,
            0x538 => self.modulationpsel = value,
            0x590 => self.nfcid1[0] = value,
            0x594 => self.nfcid1[1] = value,
            0x598 => self.nfcid1[2] = value,
            0x59C => self.autocolres = value & 1,
            0x5A0 => self.sensres = value & 0xFFFF,
            0x5A4 => self.selres = value & 0xFF,
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

/// Host-inject a modulation collision (two tags answering at once):
/// latches COLLISION (+IRQ 18 when INTENabled) with the ERRORSTATUS
/// bit the short path reports.
pub fn inject_collision(sys: &System) {
    with_nfct(sys, |n| {
        n.ev_collision = true;
        n.fire(sys, 1 << 18);
    });
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

/// Complete TX: frame went on air; sets TXFRAMEEND + ENDTX (+ the
/// TXFRAMEEND_ENABLERXDATA short re-arms the RX buffer when active).
pub fn complete_nfct_tx(sys: &System) {
    with_nfct(sys, |n| {
        n.ev_txend = true;
        n.fire(sys, 1 << 4);
        n.ev_endtx = true;
        n.fire(sys, 1 << 12);
        if n.shorts & (1 << 5) != 0 && n.enabled && n.state == State::Active && n.maxlen > 0 {
            n.rx_pending = true;
            n.rx_amount = 0;
            n.ev_rxstart = true;
            n.fire(sys, 1 << 5);
        }
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
        n.rx_bytes = amount;
        n.framestatus_rx = 0; // clean frame: no CRC/parity/overrun flags
        n.ev_rxend = true;
        n.fire(sys, 1 << 6);
        n.ev_endrx = true;
        n.fire(sys, 1 << 11);
    });
}

/// Host-inject an RX frame error (corrupt air): sets FRAMESTATUS.RX
/// flags (bit0 CRCERROR, bit2 PARITYSTATUS, bit3 OVERRUN per `flags`)
/// with EVENTS_RXERROR (+IRQ 10 when INTENabled).
pub fn inject_nfct_rxerror(sys: &System, flags: u32) {
    with_nfct(sys, |n| {
        n.framestatus_rx |= flags & 0xD;
        n.ev_rxerror = true;
        n.fire(sys, 1 << 10);
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
    fn shorts_tagstate_id_config_collision() {
        // FIELDDETECTED_ACTIVATE auto-selects; FIELDLOST_SENSE re-arms;
        // TXFRAMEEND_ENABLERXDATA re-arms RX; NFCTAGSTATE mirrors;
        // NFCID/AUTOCOLRES/SENSRES/SELRES/FRAMEDELAY/MODULATION store;
        // injected collision latches with IRQ.
        let sys = test_dummy_system();
        sys.p.write(&sys, 0xE000E100, 4, 1 << 5); // NVIC ISER: NFCT
        sys.p.write(&sys, 0x40005304, 4, (1 << 1) | (1 << 19) | (1 << 18) | (1 << 14) | (1 << 5));
        sys.p.write(&sys, 0x40005500, 4, 1); // ENABLE
        sys.p.write(&sys, 0x40005200, 4, (1 << 0) | (1 << 1) | (1 << 5)); // SHORTS
        assert_eq!(sys.p.read(&sys, 0x40005200, 4), 0x23, "SHORTS mask");
        // ID + config block.
        sys.p.write(&sys, 0x40005590, 4, 0x11223344); // NFCID1_LAST
        sys.p.write(&sys, 0x40005594, 4, 0x55667788); // 2ND_LAST
        sys.p.write(&sys, 0x40005598, 4, 0x99AABBCC); // 3RD_LAST
        sys.p.write(&sys, 0x4000559C, 4, 1); // AUTOCOLRES on
        sys.p.write(&sys, 0x400055A0, 4, 0x2177); // SENSRES
        sys.p.write(&sys, 0x400055A4, 4, 0x60); // SELRES
        sys.p.write(&sys, 0x40005504, 4, 0x1234); // FRAMEDELAYMIN
        sys.p.write(&sys, 0x40005508, 4, 0x23456); // FRAMEDELAYMAX
        sys.p.write(&sys, 0x4000552C, 4, 2); // MODULATIONCTRL
        assert_eq!(sys.p.read(&sys, 0x40005590, 4), 0x11223344, "NFCID1_LAST");
        assert_eq!(sys.p.read(&sys, 0x400055A0, 4), 0x2177, "SENSRES");
        assert_eq!(sys.p.read(&sys, 0x400055A4, 4), 0x60, "SELRES");
        assert_eq!(sys.p.read(&sys, 0x40005504, 4), 0x1234, "FRAMEDELAYMIN");
        // SENSE with AUTOCOLRES: READY + AUTOCOLRESSTARTED.
        sys.p.write(&sys, 0x40005008, 4, 1);
        assert_eq!(sys.p.read(&sys, 0x40005138, 4), 1, "AUTOCOLRESSTARTED");
        assert_eq!(sys.p.read(&sys, 0x40005410, 4), 2, "TAGSTATE Idle in Sense");
        // Field arrives: detect + auto-activate via short.
        nfct_field_present(&sys, true);
        assert_eq!(sys.p.read(&sys, 0x4000514C, 4), 1, "auto SELECTED");
        assert_eq!(sys.p.read(&sys, 0x40005150, 4), 1, "auto STARTED");
        assert!(sys.p.nvic.borrow().has_pending(), "IRQ 5 pends");
        // TX with the TXEND_ENABLERXDATA short: RX re-arms at once.
        sys.p.write(&sys, 0x40005510, 4, 0x20001000);
        sys.p.write(&sys, 0x40005514, 4, 4);
        sys.p.write(&sys, 0x4000500C, 4, 1);
        assert_eq!(sys.p.read(&sys, 0x40005410, 4), 4, "TAGSTATE Transmit");
        assert!(take_nfct_tx(&sys).is_some(), "TX staged");
        complete_nfct_tx(&sys);
        assert!(take_nfct_rx(&sys).is_some(), "RX re-armed by short");
        assert_eq!(sys.p.read(&sys, 0x40005114, 4), 1, "RXFRAMESTART via short");
        // Collision inject.
        inject_collision(&sys);
        assert_eq!(sys.p.read(&sys, 0x40005148, 4), 1, "COLLISION");
        // Field lost: FIELDLOST + auto-SENSE via short (READY again).
        sys.p.write(&sys, 0x40005100, 4, 0);
        nfct_field_present(&sys, false);
        assert_eq!(sys.p.read(&sys, 0x40005108, 4), 1, "FIELDLOST");
        assert_eq!(sys.p.read(&sys, 0x40005100, 4), 1, "READY via short");
        assert_eq!(sys.p.read(&sys, 0x40005410, 4), 2, "TAGSTATE Idle in Sense");
    }
    #[test]
    fn frameconfig_amount_status_rxerror() {
        // TXD/RXD.FRAMECONFIG store; TXD/RXD.AMOUNT report staged and
        // received lengths (bytes 11:3); clean RX clears FRAMESTATUS.RX;
        // injected RX errors latch flags + RXERROR (+IRQ 10).
        let sys = test_dummy_system();
        sys.p.write(&sys, 0xE000E100, 4, 1 << 5); // NVIC ISER: NFCT
        sys.p.write(&sys, 0x40005304, 4, 1 << 10); // INTEN RXERROR
        sys.p.write(&sys, 0x40005500, 4, 1); // ENABLE
        sys.p.write(&sys, 0x40005518, 4, 0x17); // TXD.FRAMECONFIG all
        sys.p.write(&sys, 0x40005520, 4, 0x15); // RXD.FRAMECONFIG all
        assert_eq!(sys.p.read(&sys, 0x40005518, 4), 0x17, "TX FRAMECONFIG");
        assert_eq!(sys.p.read(&sys, 0x40005520, 4), 0x15, "RX FRAMECONFIG");
        sys.p.write(&sys, 0x40005008, 4, 1); // SENSE
        nfct_field_present(&sys, true);
        sys.p.write(&sys, 0x40005000, 4, 1); // ACTIVATE
        sys.p.write(&sys, 0x40005510, 4, 0x20001000);
        sys.p.write(&sys, 0x40005514, 4, 4);
        sys.p.write(&sys, 0x4000500C, 4, 1); // STARTTX stages 4
        assert_eq!(sys.p.read(&sys, 0x4000551C, 4), 4 << 3, "TXD.AMOUNT 4 bytes");
        sys.p.write(&sys, 0x4000501C, 4, 1); // ENABLERXDATA
        assert!(take_nfct_rx(&sys).is_some(), "RX staged");
        complete_nfct_rx(&sys, 4);
        assert_eq!(sys.p.read(&sys, 0x40005524, 4), 4 << 3, "RXD.AMOUNT 4 bytes");
        assert_eq!(sys.p.read(&sys, 0x4000540C, 4), 0, "FRAMESTATUS clean");
        inject_nfct_rxerror(&sys, (1 << 0) | (1 << 3)); // CRCERROR + OVERRUN
        assert_eq!(sys.p.read(&sys, 0x4000540C, 4) & 0xD, 0x9, "status flags");
        assert_eq!(sys.p.read(&sys, 0x40005128, 4), 1, "RXERROR event");
        assert!(sys.p.nvic.borrow().has_pending(), "RXERROR IRQ 5 pends");
        sys.p.write(&sys, 0x4000540C, 4, 0xD); // write-1-clear
        assert_eq!(sys.p.read(&sys, 0x4000540C, 4), 0, "status cleared");
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