use crate::system::System;
use super::Peripheral;

/// RADIO @ 0x40001000 (IRQ 1, BLE/802.15.4-ish bare metal; SoftDevice
/// air protocols are out of scope). Offsets from nrf52833.svd:
/// TASKS_TXEN 0x000, TASKS_RXEN 0x004, TASKS_START 0x008,
/// TASKS_STOP 0x00C, TASKS_DISABLE 0x010, TASKS_RSSISTART 0x014,
/// TASKS_RSSISTOP 0x018, EVENTS_READY 0x100, EVENTS_ADDRESS 0x104,
/// EVENTS_PAYLOAD 0x108, EVENTS_END 0x10C, EVENTS_DISABLED 0x110,
/// EVENTS_RSSIEND 0x11C, EVENTS_CRCOK 0x130, EVENTS_CRCERROR 0x134,
/// INTENSET 0x304 (READY 0, ADDRESS 1, PAYLOAD 2, END 3, DISABLED 4,
/// RSSIEND 7, CRCOK 12, CRCERROR 13, ...) / CLR 0x308, SHORTS 0x200
/// (READY_START 0, END_DISABLE 1, DISABLED_TXEN 2, DISABLED_RXEN 3,
/// ADDRESS_RSSISTART 4, END_START 5), CRCSTATUS 0x400, PACKETPTR 0x504,
/// FREQUENCY 0x508, TXPOWER 0x50C, MODE 0x510, PCNF0 0x514, PCNF1 0x518
/// (MAXLEN[7:0]), BASE0 0x51C, BASE1 0x520, PREFIX0 0x524, PREFIX1 0x528,
/// TXADDRESS 0x52C, RXADDRESSES 0x530, CRCCNF 0x534, CRCPOLY 0x538,
/// CRCINIT 0x53C, RSSISAMPLE 0x548, STATE 0x550 (0 disabled, 1 RxRu,
/// 3 Rx, 9 TxRu, 11 Tx), DATAWHITEIV 0x554, DAI 0x410, DACNF 0x640.
///
/// Air model = instance loopback via driver (no RAM access inside the
/// model): START in Tx stages take_tx() (ptr, len from PCNF1.MAXLEN,
/// default 32 micro:bit-style when unset); the driver moves bytes and
/// calls complete_tx() (->END). RX packets arrive via inject_rx()
/// (queued); START in Rx with a queued packet stages take_rx(); the
/// driver writes the bytes to RAM and calls complete_rx() (->ADDRESS+
/// PAYLOAD+END+CRCOK+CRCSTATUS). inject_corrupt() queues a CRC-failed
/// packet (->CRCERROR, CRCSTATUS 0, still END). RSSI measures on
/// TASKS_RSSISTART (->RSSIEND + sample, host-set dBm, default -40).
/// SHORTS chains READY_START / END_DISABLE / DISABLED_TXEN /
/// DISABLED_RXEN / ADDRESS_RSSISTART / END_START (step-capped).
/// Stored-but-unmodeled: TXREADY/RXREADY fire alongside READY
/// (documented generosity: silicon splits ramp vs on-air ready);
/// DEVMATCH/DEVMISS/MHRMATCH need the address-match unit; ED/CCA/BC/
/// CTE/PHYEND/RATEBOOST/SYNC/TIFS are 802.15.4/BLE-test features.
pub struct RadioNrf {
    state: u32,
    ev_ready: bool,
    ev_address: bool,
    ev_payload: bool,
    ev_end: bool,
    ev_disabled: bool,
    ev_rssiend: bool,
    ev_crcok: bool,
    ev_crcerror: bool,
    ev_txready: bool,
    ev_rxready: bool,
    crcstatus: u32,
    intenset: u32,
    shorts: u32,
    packetptr: u32,
    frequency: u32,
    txpower: u32,
    mode: u32,
    pcnf0: u32,
    pcnf1: u32,
    datawhiteiv: u32,
    dai: u32,
    dacnf: u32,
    prefix0: u32,
    prefix1: u32,
    base0: u32,
    base1: u32,
    txaddress: u32,
    rxaddresses: u32,
    crccnf: u32,
    crcpoly: u32,
    crcinit: u32,
    rssi_dbm: i32,
    tx_pending: Option<(u32, u32)>,
    rx_pending: bool,
    rx_queue: Vec<(Vec<u8>, bool)>,
}

impl Default for RadioNrf {
    fn default() -> Self {
        Self {
            state: 0, ev_ready: false, ev_address: false, ev_payload: false,
            ev_end: false, ev_disabled: false, ev_rssiend: false,
            ev_crcok: false, ev_crcerror: false, ev_txready: false,
            ev_rxready: false, crcstatus: 0, intenset: 0, shorts: 0,
            packetptr: 0, frequency: 0, txpower: 0, mode: 0,
            pcnf0: 0, pcnf1: 0, datawhiteiv: 0, dai: 0, dacnf: 0,
            prefix0: 0, prefix1: 0, base0: 0, base1: 0, txaddress: 0,
            rxaddresses: 0, crccnf: 0, crcpoly: 0, crcinit: 0,
            rssi_dbm: -40, tx_pending: None, rx_pending: false,
            rx_queue: Vec::new(),
        }
    }
}

impl RadioNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "RADIO" { Some(Box::new(Self::default())) } else { None }
    }
    fn fire(&self, sys: &System, bit: u32) {
        if self.intenset & bit != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(1);
        }
    }
    /// On-air TX length: PCNF1.MAXLEN, micro:bit-style default 32.
    fn tx_len(&self) -> u32 {
        let m = self.pcnf1 & 0xFF;
        if m == 0 {
            32
        } else {
            m.min(252).max(1)
        }
    }
    fn do_txen(&mut self, sys: &System) {
        self.state = 9; // TxRu
        self.ev_ready = true;
        self.fire(sys, 1 << 0);
        self.ev_txready = true;
        self.fire(sys, 1 << 21);
        self.cascade(sys, 0);
    }
    fn do_rxen(&mut self, sys: &System) {
        self.state = 1; // RxRu
        self.ev_ready = true;
        self.fire(sys, 1 << 0);
        self.ev_rxready = true;
        self.fire(sys, 1 << 22);
        self.cascade(sys, 0);
    }
    fn do_start(&mut self, sys: &System) {
        if self.state == 9 {
            self.state = 11; // Tx
            let len = self.tx_len();
            self.tx_pending = Some((self.packetptr, len));
        } else if self.state == 1 {
            self.state = 3; // Rx
            if !self.rx_queue.is_empty() && !self.rx_pending {
                self.rx_pending = true;
            }
        }
    }
    fn do_disable(&mut self, sys: &System) {
        self.state = 0;
        self.tx_pending = None;
        self.rx_pending = false;
        self.ev_disabled = true;
        self.fire(sys, 1 << 4);
        self.cascade(sys, 0);
    }
    fn do_rssistart(&mut self, sys: &System) {
        if self.state == 0 {
            return;
        }
        self.ev_rssiend = true;
        self.fire(sys, 1 << 7);
    }
    /// SHORTS cascade with a step cap (pathological combos like
    /// END_DISABLE+DISABLED_TXEN must terminate).
    fn cascade(&mut self, sys: &System, depth: u32) {
        if depth >= 4 {
            return;
        }
        // NOTE: each arm re-checks live state; effects set events that
        // may chain further (handled by recursion, capped above).
        if self.shorts & (1 << 1) != 0 && self.ev_end {
            self.ev_end = false;
            self.do_disable(sys);
            return;
        }
        if self.shorts & (1 << 5) != 0 && self.ev_end {
            self.ev_end = false;
            self.do_start(sys);
            self.cascade(sys, depth + 1);
            return;
        }
        if self.shorts & (1 << 2) != 0 && self.ev_disabled {
            self.ev_disabled = false;
            self.do_txen(sys);
            self.cascade(sys, depth + 1);
            return;
        }
        if self.shorts & (1 << 3) != 0 && self.ev_disabled {
            self.ev_disabled = false;
            self.do_rxen(sys);
            self.cascade(sys, depth + 1);
            return;
        }
        if self.shorts & (1 << 4) != 0 && self.ev_address {
            self.do_rssistart(sys);
        }
        if self.shorts & (1 << 0) != 0 && self.ev_ready {
            // READY_START fires START once per ramp (consume the event
            // so a second ramp is needed for a second START).
            self.ev_ready = false;
            self.do_start(sys);
            self.cascade(sys, depth + 1);
        }
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
            0x11C => self.ev_rssiend as u32,
            0x130 => self.ev_crcok as u32,
            0x134 => self.ev_crcerror as u32,
            0x154 => self.ev_txready as u32,
            0x158 => self.ev_rxready as u32,
            0x200 => self.shorts,
            0x304 => self.intenset,
            0x400 => self.crcstatus,
            0x410 => self.dai,
            0x504 => self.packetptr,
            0x508 => self.frequency,
            0x50C => self.txpower,
            0x510 => self.mode,
            0x514 => self.pcnf0,
            0x518 => self.pcnf1,
            0x51C => self.base0,
            0x520 => self.base1,
            0x524 => self.prefix0,
            0x528 => self.prefix1,
            0x52C => self.txaddress,
            0x530 => self.rxaddresses,
            0x534 => self.crccnf,
            0x538 => self.crcpoly,
            0x53C => self.crcinit,
            0x548 => (-self.rssi_dbm).clamp(0, 127) as u32,
            0x550 => self.state,
            0x554 => self.datawhiteiv,
            0x640 => self.dacnf,
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => self.do_txen(sys),
            0x004 => self.do_rxen(sys),
            0x008 => {
                self.do_start(sys);
                self.cascade(sys, 0);
            }
            0x00C | 0x010 => self.do_disable(sys), // STOP/DISABLE
            0x014 => self.do_rssistart(sys),
            0x018 => {} // RSSISTOP: measurement already latched
            0x100 => if value == 0 { self.ev_ready = false; }
            0x104 => if value == 0 { self.ev_address = false; }
            0x108 => if value == 0 { self.ev_payload = false; }
            0x10C => if value == 0 { self.ev_end = false; self.crcstatus = 0; }
            0x110 => if value == 0 { self.ev_disabled = false; }
            0x11C => if value == 0 { self.ev_rssiend = false; }
            0x130 => if value == 0 { self.ev_crcok = false; }
            0x134 => if value == 0 { self.ev_crcerror = false; }
            0x154 => if value == 0 { self.ev_txready = false; }
            0x158 => if value == 0 { self.ev_rxready = false; }
            0x200 => self.shorts = value & 0x003F_FFFF,
            0x304 => self.intenset |= value & 0x02BF_C4FF,
            0x308 => self.intenset &= !value,
            0x410 => self.dai = value,
            0x504 => self.packetptr = value,
            0x508 => self.frequency = value & 0x7F,
            0x50C => self.txpower = value,
            0x510 => self.mode = value & 0xF,
            0x514 => self.pcnf0 = value,
            0x518 => self.pcnf1 = value,
            0x51C => self.base0 = value,
            0x520 => self.base1 = value,
            0x524 => self.prefix0 = value & 0xFF,
            0x528 => self.prefix1 = value & 0xFFFF_FFFF,
            0x52C => self.txaddress = value & 7,
            0x530 => self.rxaddresses = value & 0xFF,
            0x534 => self.crccnf = value & 0x303,
            0x538 => self.crcpoly = value & 0xFFFF_FFFF,
            0x53C => self.crcinit = value & 0xFFFF_FFFF,
            0x554 => self.datawhiteiv = value & 0x3F,
            0x640 => self.dacnf = value,
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

/// Take a staged TX packet (ptr, len from PCNF1.MAXLEN, default 32);
/// None when idle.
pub fn take_tx(sys: &System) -> Option<(u32, u32)> {
    with_radio(sys, |r| r.tx_pending.take()).flatten()
}

/// Complete TX: packet went on air; sets END (+SHORTS chains).
pub fn complete_tx(sys: &System) {
    with_radio(sys, |r| {
        r.ev_end = true;
        r.fire(sys, 1 << 3);
        r.cascade(sys, 0);
    });
}

/// Inject a received packet (air -> RX queue). Stages take_rx when the
/// receiver is already running.
pub fn inject_rx(sys: &System, pkt: Vec<u8>) {
    with_radio(sys, |r| {
        r.rx_queue.push((pkt, true));
        if r.state == 3 && !r.rx_pending {
            r.rx_pending = true;
        }
    });
}

/// Inject a CRC-failed packet (drives the CRCERROR path).
pub fn inject_corrupt(sys: &System, pkt: Vec<u8>) {
    with_radio(sys, |r| {
        r.rx_queue.push((pkt, false));
        if r.state == 3 && !r.rx_pending {
            r.rx_pending = true;
        }
    });
}

/// Take a staged RX buffer address; None when idle. The driver writes
/// the packet bytes to RAM at PTR, then calls complete_rx().
pub fn take_rx(sys: &System) -> Option<u32> {
    with_radio(sys, |r| {
        if r.rx_pending {
            r.rx_pending = false;
            Some(r.packetptr)
        } else {
            None
        }
    })
    .flatten()
}

/// Complete RX: pops the queued packet; sets ADDRESS+PAYLOAD+END and
/// CRCOK/CRCSTATUS (good) or CRCERROR with CRCSTATUS 0 (corrupt).
pub fn complete_rx(sys: &System) {
    with_radio(sys, |r| {
        let ok = r.rx_queue.first().map(|(_, ok)| *ok).unwrap_or(true);
        if !r.rx_queue.is_empty() {
            r.rx_queue.remove(0);
        }
        r.ev_address = true;
        r.fire(sys, 1 << 1);
        r.ev_payload = true;
        r.fire(sys, 1 << 2);
        r.ev_end = true;
        r.fire(sys, 1 << 3);
        if ok {
            r.ev_crcok = true;
            r.fire(sys, 1 << 12);
            r.crcstatus = 1;
        } else {
            r.ev_crcerror = true;
            r.fire(sys, 1 << 13);
            r.crcstatus = 0;
        }
        r.cascade(sys, 0);
    });
}

/// Set the RSSI sample level in dBm (negative, e.g. -40). Reported via
/// RSSISAMPLE as -dBm clamped 0..127.
pub fn set_rssi_dbm(sys: &System, dbm: i32) {
    with_radio(sys, |r| r.rssi_dbm = dbm.clamp(-127, 0));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    use crate::cpu::mem::Memory;
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
    fn tx_len_from_pcnf_maxlen() {
        let sys = test_dummy_system();
        sys.p.write(&sys, 0x40001504, 4, 0x20001000); // PACKETPTR
        sys.p.write(&sys, 0x40001518, 4, 12); // PCNF1.MAXLEN=12
        sys.p.write(&sys, 0x40001000, 4, 1); // TXEN
        sys.p.write(&sys, 0x40001008, 4, 1); // START
        assert_eq!(take_tx(&sys), Some((0x20001000, 12)));
        // Default (MAXLEN unset): micro:bit-style 32.
        let sys2 = test_dummy_system();
        sys2.p.write(&sys2, 0x40001504, 4, 0x20001000);
        sys2.p.write(&sys2, 0x40001000, 4, 1);
        sys2.p.write(&sys2, 0x40001008, 4, 1);
        assert_eq!(take_tx(&sys2), Some((0x20001000, 32)));
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
        crate::peripherals::radio_nrf::complete_tx(&sys);
        assert_eq!(sys.p.read(&sys, 0x4000110C, 4), 1, "END after TX complete");
        // air: loop the packet back (payload bytes travel driver-side)
        inject_rx(&sys, vec![0xAA, 0xBB, 0xCC]);
        sys.p.write(&sys, 0x4000100C, 4, 1);          // STOP/DISABLE
        sys.p.write(&sys, 0x40001004, 4, 1);          // RXEN
        sys.p.write(&sys, 0x40001008, 4, 1);          // START (Rx)
        let rxp = take_rx(&sys).expect("rx staged");
        assert_eq!(rxp, 0x20001000);
        let mut mem = crate::cpu::mem::FlatMemory::new(512 * 1024, 128 * 1024);
        for (i, &b) in [0xAAu8, 0xBB, 0xCC].iter().enumerate() {
            mem.write8(rxp.wrapping_add(i as u32), b);
        }
        complete_rx(&sys);
        assert_eq!(sys.p.read(&sys, 0x4000110C, 4), 1, "END after RX packet");
        assert_eq!(sys.p.read(&sys, 0x40001400, 4), 1, "CRCSTATUS ok");
        assert_eq!(sys.p.read(&sys, 0x40001130, 4), 1, "CRCOK");
        assert_eq!(mem.read8(rxp), 0xAA, "bytes delivered to RAM");
    }
    #[test]
    fn corrupt_packet_crcerror_path() {
        let sys = test_dummy_system();
        inject_corrupt(&sys, vec![0xDE, 0xAD]);
        sys.p.write(&sys, 0x40001004, 4, 1); // RXEN
        sys.p.write(&sys, 0x40001008, 4, 1); // START
        complete_rx(&sys);
        assert_eq!(sys.p.read(&sys, 0x40001134, 4), 1, "CRCERROR");
        assert_eq!(sys.p.read(&sys, 0x40001400, 4), 0, "CRCSTATUS clear");
        assert_eq!(sys.p.read(&sys, 0x4000110C, 4), 1, "END still fires");
    }
    #[test]
    fn shorts_end_disable_chain() {
        let sys = test_dummy_system();
        sys.p.write(&sys, 0x40001200, 4, 1 << 1); // SHORTS END_DISABLE
        sys.p.write(&sys, 0x40001504, 4, 0x20001000);
        sys.p.write(&sys, 0x40001000, 4, 1); // TXEN
        sys.p.write(&sys, 0x40001008, 4, 1); // START
        let _ = take_tx(&sys);
        complete_tx(&sys); // END -> DISABLE chain
        assert_eq!(sys.p.read(&sys, 0x40001550, 4), 0, "disabled by SHORTS");
        assert_eq!(sys.p.read(&sys, 0x40001110, 4), 1, "DISABLED event");
    }
    #[test]
    fn rssi_sample_reports_host_level() {
        let sys = test_dummy_system();
        set_rssi_dbm(&sys, -57);
        sys.p.write(&sys, 0x40001000, 4, 1); // TXEN (radio on)
        sys.p.write(&sys, 0x40001014, 4, 1); // RSSISTART
        assert_eq!(sys.p.read(&sys, 0x4000111C, 4), 1, "RSSIEND");
        assert_eq!(sys.p.read(&sys, 0x40001548, 4), 57, "RSSISAMPLE = -dBm");
    }
    #[test]
    fn rx_end_irq_when_enabled() {
        use crate::system::test_dummy_system;
        let sys = test_dummy_system();
        sys.p.write(&sys, 0xE000E100, 4, 1 << 1); // NVIC ISER: RADIO
        sys.p.write(&sys, 0x40001304, 4, 1 << 3); // INTEN: END
        inject_rx(&sys, vec![0x01]);
        sys.p.write(&sys, 0x40001004, 4, 1); // RXEN
        sys.p.write(&sys, 0x40001008, 4, 1); // START
        let _ = take_rx(&sys);
        complete_rx(&sys);
        assert!(sys.p.nvic.borrow().has_pending(), "END IRQ pends");
    }
}
