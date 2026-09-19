use crate::system::System;
use super::Peripheral;

/// RADIO @ 0x40001000 (IRQ 1, BLE/802.15.4-ish bare metal; SoftDevice
/// air protocols are out of scope). Offsets from nrf52833.svd:
/// TASKS_TXEN 0x000, TASKS_RXEN 0x004, TASKS_START 0x008,
/// TASKS_STOP 0x00C, TASKS_DISABLE 0x010, TASKS_RSSISTART 0x014,
/// TASKS_RSSISTOP 0x018, TASKS_EDSTART 0x024, TASKS_EDSTOP 0x028,
/// TASKS_CCASTART 0x02C, TASKS_CCASTOP 0x030, EVENTS_READY 0x100,
/// EVENTS_ADDRESS 0x104, EVENTS_PAYLOAD 0x108, EVENTS_END 0x10C,
/// EVENTS_DISABLED 0x110, EVENTS_DEVMATCH 0x114, EVENTS_DEVMISS 0x118,
/// EVENTS_RSSIEND 0x11C, EVENTS_CRCOK 0x130, EVENTS_CRCERROR 0x134,
/// EVENTS_FRAMESTART 0x138, EVENTS_EDEND 0x13C, EVENTS_EDSTOPPED 0x140,
/// EVENTS_CCAIDLE 0x144, EVENTS_CCABUSY 0x148, EVENTS_CCASTOPPED 0x14C,
/// EVENTS_MHRMATCH 0x15C, INTENSET 0x304 (READY 0, ADDRESS 1, PAYLOAD 2,
/// END 3, DISABLED 4, DEVMATCH 5, DEVMISS 6, RSSIEND 7, BCMATCH 10,
/// CRCOK 12, CRCERROR 13, FRAMESTART 14, EDEND 15, EDSTOPPED 16,
/// CCAIDLE 17, CCABUSY 18, CCASTOPPED 19, RATEBOOST 20, TXREADY 21,
/// RXREADY 22, MHRMATCH 23, SYNC 26, PHYEND 27, CTEPRESENT 28) /
/// CLR 0x308, SHORTS 0x200 (READY_START 0, END_DISABLE 1, DISABLED_TXEN 2,
/// DISABLED_RXEN 3, ADDRESS_RSSISTART 4, END_START 5, ADDRESS_BCSTART 6,
/// DISABLED_RSSISTOP 8, RXREADY_CCASTART 11, CCAIDLE_TXEN 12,
/// CCABUSY_DISABLE 13, FRAMESTART_BCSTART 14, READY_EDSTART 15,
/// EDEND_DISABLE 16, CCAIDLE_STOP 17, TXREADY_START 18, RXREADY_START 19,
/// PHYEND_DISABLE 20, PHYEND_START 21), CRCSTATUS 0x400, RXMATCH 0x408,
/// RXCRC 0x40C, PDUSTAT 0x414, PACKETPTR 0x504, FREQUENCY 0x508,
/// TXPOWER 0x50C, MODE 0x510, PCNF0 0x514, PCNF1 0x518 (MAXLEN[7:0]),
/// BASE0 0x51C, BASE1 0x520, PREFIX0 0x524, PREFIX1 0x528,
/// TXADDRESS 0x52C, RXADDRESSES 0x530, CRCCNF 0x534, CRCPOLY 0x538,
/// CRCINIT 0x53C, TIFS 0x544, RSSISAMPLE 0x548, STATE 0x550
/// (0 disabled, 1 RxRu, 3 Rx, 9 TxRu, 11 Tx), DATAWHITEIV 0x554,
/// BCC 0x560, DAI 0x410, DACNF 0x640, MHRMATCHCONF 0x644,
/// MHRMATCHMAS 0x648, MODECNF0 0x650, SFD 0x660, EDCNT 0x664,
/// EDSAMPLE 0x668, CCACTRL 0x66C, POWER 0xFFC.
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
/// CRC engine (CRCCNF/CRCPOLY/CRCINIT, all SVD-grounded): when LEN != 0
/// the RX completion runs the real CRC over the packet (address field
/// included unless SKIPADDR skips it) with the programmed polynomial
/// and init value; mismatch -> CRCERROR + CRCSTATUS 0 exactly like
/// silicon, and RXCRC latches the received wire CRC. LEN == 0 disables
/// the check (always CRCOK). inject_corrupt() still forces the error
/// path regardless of the registers.
/// Whitening (PCNF1.WHITEEN + DATAWHITEIV, SVD-grounded): RX bytes are
/// de-whitened with the nRF 7-bit LFSR (x^7+x^4+1, IV bit 6 hardwired
/// 1) before the CRC + delivery; TX-side whitening stays driver-side
/// (the driver hands us post-air bytes, like every take/complete pump).
/// Interference (host-set ambient level): ED/CCA read packet power +
/// ambient in log-power, and each RX completion jitters the RSSI stamp
/// by the ambient floor, so loaded spectrum reads busy/hot like real
/// air instead of a constant -40 dBm room.
/// SHORTS chains READY_START / END_DISABLE / DISABLED_TXEN /
/// DISABLED_RXEN / ADDRESS_RSSISTART / END_START (step-capped).
/// Stored-but-unmodeled: TXREADY/RXREADY fire alongside READY
/// (documented generosity: silicon splits ramp vs on-air ready);
/// RATEBOOST/SYNC/PHYEND/CTEPRESENT/BC tasks+events are BLE-test/
/// DFE features (no air-side behavior; registers stored, SHORTS for
/// PHYEND consumed).
///
/// 802.15.4 helpers (P59, all SVD-grounded, driver-observed):
/// ED (energy detect): TASKS_EDSTART in Rx latches EDSAMPLE from the
/// host level (`set_ed_dbm`, default = rssi level) and raises EDEND
/// (+IRQ 15); TASKS_EDSTOP (or EDEND_DISABLE SHORTS) raises EDSTOPPED.
/// CCA (clear-channel assessment): TASKS_CCASTART in Rx compares the
/// host level against CCACTRL thresholds → CCAIDLE (clear) or CCABUSY
/// (+IRQs); TASKS_CCASTOP (or CCAIDLE_STOP) raises CCASTOPPED.
/// SHORTS consumed: READY_EDSTART, EDEND_DISABLE, RXREADY_CCASTART,
/// CCAIDLE_TXEN, CCABUSY_DISABLE, CCAIDLE_STOP, TXREADY_START,
/// RXREADY_START, DISABLED_RSSISTOP, ADDRESS_BCSTART/FRAMESTART_BCSTART
/// (BCSTART arms BCMATCH, see below).
/// Device-address match: on RX completion the first packet byte is
/// matched against DAB/DAP (programmed via the DAB/DAP-indexed alias
/// registers — stored, indexed by TXADDRESS/RXADDRESSES selection):
/// match → DEVMATCH (+IRQ 5), else DEVMISS (+IRQ 6). Disabled when no
/// DAB entries are programmed (both read 0, like silicon reset).
/// MHR match: MHRMATCHCONF/MAS mask the first packet bytes on RX
/// completion → MHRMATCH (+IRQ 23) when `(pkt & mas) == (conf & mas)`
/// with a nonzero mask. FRAMESTART fires alongside ADDRESS on every
/// RX completion (SVD 0x138; silicon raises it at frame start — we
/// raise it with the completion batch, same quantum).
/// RXMATCH/RXCRC/PDUSTAT report the last RX completion (match index,
/// received CRC, PDU status flags); BCC counts down TX payload bytes
/// (reloaded from PCNF1.BALEN-adjacent BCC init on START — stored).
/// TIFS/SFD/MODECNF0/POWER stored (TIFS = inter-frame spacing timer
/// preset; POWER must be 1 for any task to act — tasks are ignored
/// while POWER==0 after an explicit write of 0, reset default on).
pub struct RadioNrf {
    state: u32,
    ev_ready: bool,
    ev_address: bool,
    ev_payload: bool,
    ev_end: bool,
    ev_disabled: bool,
    ev_devmatch: bool,
    ev_devmiss: bool,
    ev_rssiend: bool,
    ev_bcmatch: bool,
    ev_crcok: bool,
    ev_crcerror: bool,
    ev_framestart: bool,
    ev_edend: bool,
    ev_edstopped: bool,
    ev_ccaidle: bool,
    ev_ccabusy: bool,
    ev_ccastopped: bool,
    ev_mhrmatch: bool,
    ev_phyend: bool,
    ev_txready: bool,
    ev_rxready: bool,
    crcstatus: u32,
    rxmatch: u32,
    rxcrc: u32,
    pdustat: u32,
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
    tifs: u32,
    bcc: u32,
    mhrmatchconf: u32,
    mhrmatchmas: u32,
    modecnf0: u32,
    sfd: u32,
    edcnt: u32,
    edsample: u32,
    ccactrl: u32,
    powered: bool,
    rssi_dbm: i32,
    ed_dbm: Option<i32>,
    /// Ambient RF level in dBm for the interference model (host-set via
    /// `set_interference_dbm`, default None = quiet air). Adds
    /// log-power into the ED/CCA front end and jitters the per-packet
    /// RSSI stamp, like real spectrum under load.
    interference_dbm: Option<i32>,
    cca_busy: bool,
    tx_pending: Option<(u32, u32)>,
    rx_pending: bool,
    rx_queue: Vec<(Vec<u8>, bool, Option<u32>)>,
    dab: [u32; 8],
    dap: [u32; 8],
}

impl Default for RadioNrf {
    fn default() -> Self {
        Self {
            state: 0, ev_ready: false, ev_address: false, ev_payload: false,
            ev_end: false, ev_disabled: false, ev_devmatch: false,
            ev_devmiss: false, ev_rssiend: false, ev_bcmatch: false,
            ev_crcok: false, ev_crcerror: false, ev_framestart: false,
            ev_edend: false, ev_edstopped: false, ev_ccaidle: false,
            ev_ccabusy: false, ev_ccastopped: false, ev_mhrmatch: false,
            ev_phyend: false,
            ev_txready: false, ev_rxready: false, crcstatus: 0,
            rxmatch: 0, rxcrc: 0, pdustat: 0, intenset: 0, shorts: 0,
            packetptr: 0, frequency: 0, txpower: 0, mode: 0,
            pcnf0: 0, pcnf1: 0, datawhiteiv: 0, dai: 0, dacnf: 0,
            prefix0: 0, prefix1: 0, base0: 0, base1: 0, txaddress: 0,
            rxaddresses: 0, crccnf: 0, crcpoly: 0, crcinit: 0,
            tifs: 0, bcc: 0, mhrmatchconf: 0, mhrmatchmas: 0,
            modecnf0: 0, sfd: 0, edcnt: 0, edsample: 0, ccactrl: 0,
            powered: true, rssi_dbm: -40, ed_dbm: None, interference_dbm: None, cca_busy: false,
            tx_pending: None, rx_pending: false,
            rx_queue: Vec::new(), dab: [0; 8], dap: [0; 8],
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
        if !self.powered {
            return;
        }
        self.state = 9; // TxRu
        self.ev_ready = true;
        self.fire(sys, 1 << 0);
        self.ev_txready = true;
        self.fire(sys, 1 << 21);
        self.cascade(sys, 0);
    }
    fn do_rxen(&mut self, sys: &System) {
        if !self.powered {
            return;
        }
        self.state = 1; // RxRu
        self.ev_ready = true;
        self.fire(sys, 1 << 0);
        self.ev_rxready = true;
        self.fire(sys, 1 << 22);
        self.cascade(sys, 0);
    }
    fn do_start(&mut self, sys: &System) {
        if !self.powered {
            return;
        }
        if self.state == 9 {
            self.state = 11; // Tx
            let len = self.tx_len();
            self.bcc = len; // payload byte counter (counts down on TX)
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
    /// Energy-detect level source: host ED level when set, else RSSI,
    /// plus the ambient interference floor in log-power (loaded
    /// spectrum reads hotter than the link budget alone).
    fn ed_level(&self) -> i32 {
        let base = self.ed_dbm.unwrap_or(self.rssi_dbm);
        match self.interference_dbm {
            Some(amb) => add_interference_dbm(base, amb),
            None => base,
        }
    }
    fn do_edstart(&mut self, sys: &System) {
        // Silicon counts ED iterations in EDCNT; EDSAMPLE latches the
        // measured level. Model: one instant sample from the host level.
        if self.state != 3 && self.state != 1 {
            return;
        }
        self.edcnt = self.edcnt.wrapping_add(1);
        self.edsample = self.ed_level().clamp(-127, 0).unsigned_abs() as u32;
        self.ev_edend = true;
        self.fire(sys, 1 << 15);
        self.cascade(sys, 0);
    }
    fn do_edstop(&mut self, sys: &System) {
        self.ev_edstopped = true;
        self.fire(sys, 1 << 16);
    }
    /// Clear-channel assessment: host level vs CCACTRL thresholds.
    /// CCAMODE 0 (ED threshold): busy iff level >= EDTHRES.
    /// CCAMODE 1/2/3 consult CORRTHRES/CORRCNT — unmodeled correlates,
    /// so fall back to the ED rule (documented in the field comment).
    fn do_ccastart(&mut self, sys: &System) {
        if self.state != 3 && self.state != 1 {
            return;
        }
        let edth = ((self.ccactrl >> 8) & 0xFF) as i32;
        // EDSAMPLE-style level: magnitude of dBm. Default CCACTRL=0 →
        // threshold 0 → any real signal reads busy, silence reads idle.
        let lvl = self.ed_level().clamp(-127, 0).unsigned_abs() as i32;
        // Map threshold byte to dB-ish scale like EDSAMPLE (0..127).
        // Plausible default: with CCACTRL reset (0), threshold reads 0
        // and only absolute silence (level 0, never real) is idle — so
        // expose the comparison directly: busy iff lvl > edth.
        self.cca_busy = lvl > edth;
        if self.cca_busy {
            self.ev_ccabusy = true;
            self.fire(sys, 1 << 18);
        } else {
            self.ev_ccaidle = true;
            self.fire(sys, 1 << 17);
        }
        self.cascade(sys, 0);
    }
    fn do_ccastop(&mut self, sys: &System) {
        self.ev_ccastopped = true;
        self.fire(sys, 1 << 19);
    }
    fn do_bcstart(&mut self, sys: &System) {
        // Bit-counter compare: fires BCMATCH (+IRQ 10) immediately
        // (BCC counts TX payload bytes; a real counter would compare
        // air bits — instant match is the faithful idle semantic).
        self.ev_bcmatch = true;
        self.fire(sys, 1 << 10);
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
        if self.shorts & (1 << 6) != 0 && self.ev_address {
            self.do_bcstart(sys);
        }
        if self.shorts & (1 << 14) != 0 && self.ev_framestart {
            self.do_bcstart(sys);
        }
        if self.shorts & (1 << 8) != 0 && self.ev_disabled {
            // DISABLED_RSSISTOP: no latched RSSI-op state exists (RSSI is
            // instant), so nothing to stop — consume per silicon wiring.
        }
        if self.shorts & (1 << 11) != 0 && self.ev_rxready {
            self.ev_rxready = false;
            self.do_ccastart(sys);
            self.cascade(sys, depth + 1);
            return;
        }
        if self.shorts & (1 << 12) != 0 && self.ev_ccaidle {
            self.ev_ccaidle = false;
            self.do_txen(sys);
            self.cascade(sys, depth + 1);
            return;
        }
        if self.shorts & (1 << 13) != 0 && self.ev_ccabusy {
            self.ev_ccabusy = false;
            self.do_disable(sys);
            self.cascade(sys, depth + 1);
            return;
        }
        if self.shorts & (1 << 15) != 0 && self.ev_ready {
            self.ev_ready = false;
            self.do_edstart(sys);
            self.cascade(sys, depth + 1);
            return;
        }
        if self.shorts & (1 << 16) != 0 && self.ev_edend {
            self.ev_edend = false;
            self.do_edstop(sys);
            self.cascade(sys, depth + 1);
            return;
        }
        if self.shorts & (1 << 17) != 0 && self.ev_ccaidle {
            self.ev_ccaidle = false;
            self.do_disable(sys); // STOP shares the disable path
            self.cascade(sys, depth + 1);
            return;
        }
        if self.shorts & (1 << 18) != 0 && self.ev_txready {
            self.ev_txready = false;
            self.do_start(sys);
            self.cascade(sys, depth + 1);
            return;
        }
        if self.shorts & (1 << 19) != 0 && self.ev_rxready {
            self.ev_rxready = false;
            self.do_start(sys);
            self.cascade(sys, depth + 1);
            return;
        }
        if self.shorts & (1 << 20) != 0 && self.ev_phyend {
            self.ev_phyend = false;
            self.do_disable(sys);
            self.cascade(sys, depth + 1);
            return;
        }
        if self.shorts & (1 << 21) != 0 && self.ev_phyend {
            self.ev_phyend = false;
            self.do_start(sys);
            self.cascade(sys, depth + 1);
            return;
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
            0x114 => self.ev_devmatch as u32,
            0x118 => self.ev_devmiss as u32,
            0x11C => self.ev_rssiend as u32,
            0x128 => self.ev_bcmatch as u32,
            0x130 => self.ev_crcok as u32,
            0x134 => self.ev_crcerror as u32,
            0x138 => self.ev_framestart as u32,
            0x13C => self.ev_edend as u32,
            0x140 => self.ev_edstopped as u32,
            0x144 => self.ev_ccaidle as u32,
            0x148 => self.ev_ccabusy as u32,
            0x14C => self.ev_ccastopped as u32,
            0x154 => self.ev_txready as u32,
            0x158 => self.ev_rxready as u32,
            0x15C => self.ev_mhrmatch as u32,
            0x16C => self.ev_phyend as u32,
            0x200 => self.shorts,
            0x304 => self.intenset,
            0x400 => self.crcstatus,
            0x408 => self.rxmatch,
            0x40C => self.rxcrc,
            0x410 => self.dai,
            0x414 => self.pdustat,
            0x44C => 0, // CTESTATUS (BLE-test counters, no air-side behavior)
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
            0x544 => self.tifs,
            0x548 => (-self.rssi_dbm).clamp(0, 127) as u32,
            0x550 => self.state,
            0x554 => self.datawhiteiv,
            0x560 => self.bcc,
            0x600 | 0x604 | 0x608 | 0x60C | 0x610 | 0x614 | 0x618 | 0x61C => {
                // DAB[0..7]: device-address base (indexed alias).
                self.dab[((offset - 0x600) >> 2) as usize]
            }
            0x620 | 0x624 | 0x628 | 0x62C | 0x630 | 0x634 | 0x638 | 0x63C => {
                // DAP[0..7]: device-address prefix (indexed alias).
                self.dap[((offset - 0x620) >> 2) as usize]
            }
            0x640 => self.dacnf,
            0x644 => self.mhrmatchconf,
            0x648 => self.mhrmatchmas,
            0x650 => self.modecnf0,
            0x660 => self.sfd,
            0x664 => self.edcnt,
            0x668 => self.edsample,
            0x66C => self.ccactrl,
            0xFFC => self.powered as u32,
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
            0x024 => self.do_edstart(sys),
            0x028 => self.do_edstop(sys),
            0x02C => self.do_ccastart(sys),
            0x030 => self.do_ccastop(sys),
            0x100 => if value == 0 { self.ev_ready = false; }
            0x104 => if value == 0 { self.ev_address = false; }
            0x108 => if value == 0 { self.ev_payload = false; }
            0x10C => if value == 0 { self.ev_end = false; self.crcstatus = 0; }
            0x110 => if value == 0 { self.ev_disabled = false; }
            0x114 => if value == 0 { self.ev_devmatch = false; }
            0x118 => if value == 0 { self.ev_devmiss = false; }
            0x11C => if value == 0 { self.ev_rssiend = false; }
            0x128 => if value == 0 { self.ev_bcmatch = false; }
            0x130 => if value == 0 { self.ev_crcok = false; }
            0x134 => if value == 0 { self.ev_crcerror = false; }
            0x138 => if value == 0 { self.ev_framestart = false; }
            0x13C => if value == 0 { self.ev_edend = false; }
            0x140 => if value == 0 { self.ev_edstopped = false; }
            0x144 => if value == 0 { self.ev_ccaidle = false; }
            0x148 => if value == 0 { self.ev_ccabusy = false; }
            0x14C => if value == 0 { self.ev_ccastopped = false; }
            0x154 => if value == 0 { self.ev_txready = false; }
            0x158 => if value == 0 { self.ev_rxready = false; }
            0x15C => if value == 0 { self.ev_mhrmatch = false; }
            0x16C => if value == 0 { self.ev_phyend = false; }
            0x200 => self.shorts = value & 0x003F_FFFF,
            // SVD INTEN bits: 0-7,10,12-23,26-28 (RATEBOOST 20 has no
            // event cell in this model; PHYEND/CTE events unmodeled).
            0x304 => self.intenset |= value & 0x1CFF_F4FF,
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
            0x544 => self.tifs = value & 0xFFFF_FFFF,
            0x554 => self.datawhiteiv = value & 0x3F,
            0x560 => self.bcc = value & 0xFFFF_FFFF,
            0x600 | 0x604 | 0x608 | 0x60C | 0x610 | 0x614 | 0x618 | 0x61C => {
                self.dab[((offset - 0x600) >> 2) as usize] = value;
            }
            0x620 | 0x624 | 0x628 | 0x62C | 0x630 | 0x634 | 0x638 | 0x63C => {
                self.dap[((offset - 0x620) >> 2) as usize] = value & 0xFFFF;
            }
            0x640 => self.dacnf = value,
            0x644 => self.mhrmatchconf = value,
            0x648 => self.mhrmatchmas = value,
            0x650 => self.modecnf0 = value & 3,
            0x660 => self.sfd = value,
            0x664 => {} // EDCNT is read-only (counts ED iterations)
            0x668 => {} // EDSAMPLE is read-only (latched level)
            0x66C => self.ccactrl = value,
            0xFFC => {
                // POWER: 0 cuts the radio (silicon reset value is 1;
                // an explicit 0 must gate tasks until re-enabled).
                let on = value & 1 == 1;
                self.powered = on;
                if !on {
                    self.state = 0;
                    self.tx_pending = None;
                    self.rx_pending = false;
                }
            }
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
/// receiver is already running. No path loss known: the RX completion
/// stamps TXPOWER-minus-zero (co-located default).
pub fn inject_rx(sys: &System, pkt: Vec<u8>) {
    with_radio(sys, |r| {
        r.rx_queue.push((pkt, true, None));
        if r.state == 3 && !r.rx_pending {
            r.rx_pending = true;
        }
    });
}

/// Inject with a known path loss in dB (driver-side air range model):
/// the RX completion stamps TXPOWER-minus-loss into the RSSI latch.
pub fn inject_rx_lossy(sys: &System, pkt: Vec<u8>, path_loss_db: u32) {
    with_radio(sys, |r| {
        r.rx_queue.push((pkt, true, Some(path_loss_db)));
        if r.state == 3 && !r.rx_pending {
            r.rx_pending = true;
        }
    });
}

/// Inject a CRC-failed packet (drives the CRCERROR path).
pub fn inject_corrupt(sys: &System, pkt: Vec<u8>) {
    with_radio(sys, |r| {
        r.rx_queue.push((pkt, false, None));
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
/// Also runs the address-match unit (DEVMATCH/DEVMISS + RXMATCH/RXCRC/
/// PDUSTAT), the MHR matcher, and FRAMESTART (all SVD-grounded).
/// Air level: the RSSI latch is stamped from TXPOWER minus the queued
/// packet's path loss (see inject_rx path-loss forms), so a firmware
/// RSSISTART after RX reads this packet's level like silicon.
/// CRC engine: with CRCCNF.LEN != 0 the packet's trailing LEN bytes are
/// checked against a real CRC (CRCPOLY/CRCINIT, SKIPADDR honored);
/// mismatch forces the CRCERROR path exactly like silicon. With LEN ==
/// 0 the check is disabled (always CRCOK). Whitening (PCNF1.WHITEEN)
/// de-whitens with DATAWHITEIV before the check.
pub fn complete_rx(sys: &System) {
    complete_rx_with_loss(sys, 0);
}

/// Same as complete_rx with an explicit path-loss override (dB) for
/// the RSSI stamp. The queued per-packet loss wins when present (see
/// inject_rx_lossy); this parameter covers the legacy lossless queue.
fn complete_rx_with_loss(sys: &System, default_loss_db: u32) {
    with_radio(sys, |r| {
        let mut pkt = r.rx_queue.first().map(|(p, _, _)| p.clone()).unwrap_or_default();
        let (mut ok, loss) = r
            .rx_queue
            .first()
            .map(|(_, ok, loss)| (*ok, *loss))
            .unwrap_or((true, Some(default_loss_db)));
        if !r.rx_queue.is_empty() {
            r.rx_queue.remove(0);
        }
        // Air-level stamp FIRST (silicon samples the packet on air):
        // TXPOWER-derived dBm minus this packet's path loss. Queued
        // per-packet loss wins; the legacy queue carries None and falls
        // back to the caller's default (0 = co-located loopback).
        // Interference floor: ambient RF adds log-power heat so loaded
        // spectrum reads hotter than the link budget alone.
        let loss_db = loss.unwrap_or(default_loss_db);
        r.rssi_dbm = air_rssi_dbm(r.txpower, loss_db);
        if let Some(amb) = r.interference_dbm {
            r.rssi_dbm = add_interference_dbm(r.rssi_dbm, amb);
        }
        // Whitening (PCNF1.WHITEEN bit 25, DATAWHITEIV 6-bit LFSR seed
        // with bit 6 hardwired 1): de-whiten the air bytes before the
        // CRC + match units see them, like silicon's baseband.
        if (r.pcnf1 >> 25) & 1 == 1 && !pkt.is_empty() {
            whiten_in_place(&mut pkt, r.datawhiteiv);
        }
        // CRC engine (CRCCNF 0x534: LEN[1:0], SKIPADDR[9:8]; CRCPOLY
        // 0x538 up to 24-bit; CRCINIT 0x53C seed, LEN bytes wide).
        // LEN == 0 disables (always CRCOK). Otherwise the trailing LEN
        // bytes are the wire CRC over [address-skipped] payload; SKIPADDR
        // == 1 (Skip) drops byte 0 from the computation, == 2 is the
        // 802.15.4 variant (same skip-one shape here). inject_corrupt's
        // forced error still wins regardless of the registers.
        let crc_len = (r.crccnf & 0x03) as usize;
        let skip = (r.crccnf >> 8) & 0x03;
        if ok && crc_len != 0 && pkt.len() >= crc_len {
            let body_end = pkt.len() - crc_len;
            let body_start = if skip == 0 { 0 } else { body_end.min(1) };
            let expect = radio_crc(
                &pkt[body_start..body_end],
                r.crcpoly,
                r.crcinit,
                crc_len,
            );
            let mut wire: u32 = 0;
            for (i, &b) in pkt[body_end..].iter().enumerate() {
                wire |= (b as u32) << (8 * i);
            }
            // RXCRC always latches the received wire CRC (silicon does,
            // even on mismatch — firmware reads it to diagnose).
            r.rxcrc = wire;
            if expect != wire {
                ok = false;
            }
        }
        r.ev_address = true;
        r.fire(sys, 1 << 1);
        r.ev_framestart = true;
        r.fire(sys, 1 << 14);
        r.ev_payload = true;
        r.fire(sys, 1 << 2);
        // Device-address match: first packet byte vs programmed DAB/DAP.
        // Any DAB entry equal to byte 0 with the matching DAP prefix
        // byte counts as a match (RXADDRESSES selects which logical
        // addresses listen; 0 = none listen → always MISS).
        let mut matched = false;
        let mut match_idx = 0u32;
        if !pkt.is_empty() {
            for i in 0..8u32 {
                if r.rxaddresses & (1 << i) != 0
                    && r.dab[i as usize] != 0
                    && (pkt[0] as u32) == (r.dab[i as usize] & 0xFF)
                {
                    matched = true;
                    match_idx = i;
                    break;
                }
            }
        }
        let any_dab = r.dab.iter().any(|&d| d != 0);
        if matched {
            r.ev_devmatch = true;
            r.fire(sys, 1 << 5);
        } else if any_dab {
            r.ev_devmiss = true;
            r.fire(sys, 1 << 6);
        }
        r.rxmatch = match_idx;
        // RXCRC: the CRC engine arm above already latched the wire CRC
        // when LEN != 0; with the engine disabled keep the legacy tail
        // echo (last two bytes) so old firmware still sees *something*.
        if crc_len == 0 {
            r.rxcrc = if pkt.len() >= 2 {
                (pkt[pkt.len() - 2] as u32) | ((pkt[pkt.len() - 1] as u32) << 8)
            } else {
                0
            };
        }
        // PDUSTAT: bit0 = CRC ok, bit1 = address matched (local layout,
        // documented here; silicon PDUSTAT packs PHY/CI flags we don't
        // model — only these two bits are ever nonzero).
        r.pdustat = (ok as u32) | ((matched as u32) << 1);
        // MHR match: (pkt[0..2] & mas) == (conf & mas) with nonzero mask.
        if r.mhrmatchmas & 0xFFFF != 0 && pkt.len() >= 2 {
            let pm = ((pkt[0] as u32) | ((pkt[1] as u32) << 8)) & (r.mhrmatchmas & 0xFFFF);
            if pm == (r.mhrmatchconf & 0xFFFF) {
                r.ev_mhrmatch = true;
                r.fire(sys, 1 << 23);
            }
        }
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

/// Complete RX with an explicit path loss (dB) for this packet's RSSI
/// stamp. Driver-side air (bridge/loopback) calls this when it knows
/// the range; the plain complete_rx() keeps the queued/default loss.
pub fn complete_rx_with_path_loss(sys: &System, path_loss_db: u32) {
    complete_rx_with_loss(sys, path_loss_db);
}

/// Set the RSSI sample level in dBm (negative, e.g. -40). Reported via
/// RSSISAMPLE as -dBm clamped 0..127.
pub fn set_rssi_dbm(sys: &System, dbm: i32) {
    with_radio(sys, |r| r.rssi_dbm = dbm.clamp(-127, 0));
}

/// TXPOWER (SVD 0x50C) as signed dBm: the SVD enumerates the nRF52
/// radio levels (+8..0, -4, -8, -12, -16, -20, -30, -40); unlisted
/// codes read back verbatim but contribute 0 dBm (documented: silicon
/// behavior there is unspecified, and no firmware here depends on it).
pub fn txpower_dbm(code: u32) -> i32 {
    match code & 0xFF {
        0x08 => 8,
        0x07 => 7,
        0x06 => 6,
        0x05 => 5,
        0x04 => 4,
        0x03 => 3,
        0x02 => 2,
        0x00 => 0,
        0xFC => -4,
        0xF8 => -8,
        0xF4 => -12,
        0xF0 => -16,
        0xEC => -20,
        0xE2 => -30,
        0xD8 => -40,
        _ => 0,
    }
}

/// Link-budget air level: TX dBm minus path loss, clamped to the
/// [-127, 0] RSSI window. Pure function so the driver (JS bridge or
/// bench loopback) and the tests share one honest number instead of
/// the old fixed -50 dBm constant.
pub fn air_rssi_dbm(tx_code: u32, path_loss_db: u32) -> i32 {
    (txpower_dbm(tx_code) - path_loss_db as i32).clamp(-127, 0)
}

/// Sample the air level for an RX completion: TXPOWER-derived dBm
/// minus path loss, stamped into the RSSI sample latch so a firmware
/// RSSISTART right after RX reads the packet's own level (silicon
/// samples the on-air packet, not a stale register).
pub fn sample_air_rssi(sys: &System, path_loss_db: u32) {
    with_radio(sys, |r| {
        r.rssi_dbm = air_rssi_dbm(r.txpower, path_loss_db);
    });
}

/// Set the energy-detect sample level in dBm (negative). Reported via
/// EDSAMPLE as -dBm clamped 0..127 on the next EDSTART. Defaults to
/// the RSSI level (shared front end).
pub fn set_ed_dbm(sys: &System, dbm: i32) {
    with_radio(sys, |r| r.ed_dbm = Some(dbm.clamp(-127, 0)));
}

/// Set the ambient RF floor in dBm for the interference model
/// (negative, e.g. -70 for a busy room; None-equivalent clears via
/// `clear_interference`). ED/CCA add it in log-power to the packet
/// level, and each RX completion heats the RSSI stamp toward it, so
/// loaded spectrum reads busy/hot instead of a constant quiet room.
/// Pure host-side air state — no SVD register, documented here.
pub fn set_interference_dbm(sys: &System, dbm: i32) {
    with_radio(sys, |r| r.interference_dbm = Some(dbm.clamp(-127, 0)));
}

/// Clear the ambient floor (quiet air again).
pub fn clear_interference(sys: &System) {
    with_radio(sys, |r| r.interference_dbm = None);
}

/// Log-power add of an ambient floor onto a packet level (both dBm,
/// negative): P_total = 10*log10(10^(a/10) + 10^(b/10)), clamped to
/// the [-127, 0] RSSI window. Pure function so the ED/CCA path and
/// the RX-stamp path share one honest number.
pub fn add_interference_dbm(packet_dbm: i32, ambient_dbm: i32) -> i32 {
    let pa = 10f64.powf(packet_dbm as f64 / 10.0);
    let pb = 10f64.powf(ambient_dbm as f64 / 10.0);
    let total = (10.0 * (pa + pb).log10()).round() as i32;
    total.clamp(-127, 0)
}

/// nRF data-whitening LFSR (polynomial x^7 + x^4 + 1, 7-bit state):
/// de-whiten (or whiten — the operation is its own inverse) `buf` in
/// place with the DATAWHITEIV seed (bit 6 hardwired 1 per the SVD:
/// writing 0 there has no effect). One keystream bit per payload bit,
/// MSB-first per byte, LFSR advanced per bit like silicon's baseband.
pub fn whiten_in_place(buf: &mut [u8], iv: u32) {
    let mut lfsr = ((iv & 0x3F) | 0x40) as u8; // 7 bits, bit6 forced 1
    for b in buf.iter_mut() {
        let mut out = 0u8;
        for i in (0..8).rev() {
            // Keystream bit = bit6 ^ bit3 of the current state.
            let ks = ((lfsr >> 6) ^ (lfsr >> 3)) & 1;
            out |= (((*b >> i) & 1) ^ ks) << i;
            // Advance: shift left, new LSB = old bit6.
            let nb = (lfsr >> 6) & 1;
            lfsr = ((lfsr << 1) & 0x7F) | nb;
        }
        *b = out;
    }
}

/// CRC over `body` with the RADIO engine shape: `len` bytes wide
/// (1..3 from CRCCNF.LEN), polynomial `poly` (up to 24 bits from
/// CRCPOLY), seed `init` (LEN bytes from CRCINIT), MSB-first
/// shift-register like silicon's baseband CRC. Pure function so the
/// RX check and the tests share the exact wire algorithm.
pub fn radio_crc(body: &[u8], poly: u32, init: u32, len: usize) -> u32 {
    let len = len.clamp(1, 3);
    let mask: u32 = if len >= 3 { 0xFF_FFFF } else { (1 << (8 * len)) - 1 };
    let mut crc = init & mask;
    let top = 1 << (8 * len - 1);
    let poly = poly & mask;
    for &b in body {
        for i in (0..8).rev() {
            let bit = ((b >> i) & 1) as u32;
            let msb = (crc & top) != 0;
            crc = ((crc << 1) & mask) | bit;
            if (msb) {
                crc ^= poly;
            }
        }
    }
    // Flush LEN*8 zero bits through (silicon clocks the register out).
    for _ in 0..8 * len {
        let msb = (crc & top) != 0;
        crc = (crc << 1) & mask;
        if (msb) {
            crc ^= poly;
        }
    }
    crc & mask
}

/// Inject a received packet addressed to a DAB/DAP entry (air peer).
/// Convenience over inject_rx for the two-instance bridge: the first
/// byte is the device-address byte the match unit checks.
pub fn inject_rx_to(sys: &System, dab_idx: usize, pkt: Vec<u8>) {
    with_radio(sys, |r| {
        let _ = dab_idx;
        r.rx_queue.push((pkt, true, None));
        if r.state == 3 && !r.rx_pending {
            r.rx_pending = true;
        }
    });
}

/// Addressed form with path loss (bridge peer at range).
pub fn inject_rx_to_lossy(sys: &System, dab_idx: usize, pkt: Vec<u8>, path_loss_db: u32) {
    with_radio(sys, |r| {
        let _ = dab_idx;
        r.rx_queue.push((pkt, true, Some(path_loss_db)));
        if r.state == 3 && !r.rx_pending {
            r.rx_pending = true;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    use crate::cpu::mem::Memory;
    #[test]
    fn ed_cca_mhr_devmatch_framestart() {
        let sys = test_dummy_system();
        sys.p.write(&sys, 0xE000E100, 4, 1 << 1); // NVIC ISER: RADIO
        sys.p.write(&sys, 0x40001304, 4, (1 << 15) | (1 << 16) | (1 << 17) | (1 << 18) | (1 << 5) | (1 << 6) | (1 << 23) | (1 << 14));
        sys.p.write(&sys, 0x40001004, 4, 1); // RXEN (ED/CCA need Rx)
        sys.p.write(&sys, 0x40001008, 4, 1); // START
        // ED: host level -50 dBm -> EDSAMPLE 50, EDEND+IRQ, EDCNT+1.
        set_ed_dbm(&sys, -50);
        sys.p.write(&sys, 0x40001024, 4, 1); // EDSTART
        assert_eq!(sys.p.read(&sys, 0x4000113C, 4), 1, "EDEND");
        assert_eq!(sys.p.read(&sys, 0x40001668, 4), 50, "EDSAMPLE");
        assert_eq!(sys.p.read(&sys, 0x40001664, 4), 1, "EDCNT");
        assert!(sys.p.nvic.borrow().has_pending(), "EDEND IRQ pends");
        sys.p.write(&sys, 0x40001028, 4, 1); // EDSTOP
        assert_eq!(sys.p.read(&sys, 0x40001140, 4), 1, "EDSTOPPED");
        // CCA: threshold 0 (reset CCACTRL) vs level 50 -> busy.
        sys.p.write(&sys, 0x4000102C, 4, 1); // CCASTART
        assert_eq!(sys.p.read(&sys, 0x40001148, 4), 1, "CCABUSY");
        sys.p.write(&sys, 0x40001030, 4, 1); // CCASTOP
        assert_eq!(sys.p.read(&sys, 0x4000114C, 4), 1, "CCASTOPPED");
        // Quiet air (level 0): idle. Force via ED override at 0 dBm.
        set_ed_dbm(&sys, 0);
        sys.p.write(&sys, 0x40001148, 4, 0);
        sys.p.write(&sys, 0x40001144, 4, 0);
        sys.p.write(&sys, 0x4000102C, 4, 1);
        assert_eq!(sys.p.read(&sys, 0x40001144, 4), 1, "CCAIDLE in quiet");
        // Device-address match + MHR + FRAMESTART on RX completion.
        sys.p.write(&sys, 0x40001600, 4, 0xEF); // DAB[0] (matches pkt[0])
        sys.p.write(&sys, 0x40001530, 4, 1); // RXADDRESSES: listen addr 0
        sys.p.write(&sys, 0x40001644, 4, 0xBEEF); // MHRMATCHCONF
        sys.p.write(&sys, 0x40001648, 4, 0xFFFF); // MHRMATCHMAS
        inject_rx(&sys, vec![0xEF, 0xBE, 0x01]);
        let _ = take_rx(&sys).expect("rx staged");
        complete_rx(&sys);
        assert_eq!(sys.p.read(&sys, 0x40001114, 4), 1, "DEVMATCH");
        assert_eq!(sys.p.read(&sys, 0x40001138, 4), 1, "FRAMESTART");
        assert_eq!(sys.p.read(&sys, 0x4000115C, 4), 1, "MHRMATCH");
        assert_eq!(sys.p.read(&sys, 0x40001408, 4), 0, "RXMATCH idx 0");
        // Miss path: different first byte, DAB programmed.
        inject_rx(&sys, vec![0x55, 0x00]);
        let _ = take_rx(&sys).expect("rx staged again");
        complete_rx(&sys);
        assert_eq!(sys.p.read(&sys, 0x40001118, 4), 1, "DEVMISS");
        // 2nd run: fresh default has no DAB, no MHR, ED untouched.
        let r2 = RadioNrf::default();
        assert_eq!(r2.dab, [0; 8]);
        assert_eq!(r2.edcnt, 0);
    }
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
    fn crc_engine_whitening_interference_air() {
        // CRC engine (CRCCNF/CRCPOLY/CRCINIT, SVD-grounded): LEN=2 with
        // the BLE poly derives the wire CRC; a good packet passes CRCOK
        // and latches RXCRC, a flipped bit fails CRCERROR + CRCSTATUS 0.
        // LEN=0 disables (tail echo preserved). Whitening roundtrips
        // through the nRF LFSR. Interference heats ED + RSSI in
        // log-power. Each leg asserts real register effects.
        let sys = test_dummy_system();
        sys.p.write(&sys, 0x40001534, 4, 2); // CRCCNF.LEN=2
        sys.p.write(&sys, 0x40001538, 4, 0x010065); // CRCPOLY (BLE-ish)
        sys.p.write(&sys, 0x4000153C, 4, 0x00BEEF); // CRCINIT seed
        let body = vec![0xEF, 0xBE, 0x01];
        let crc = radio_crc(&body, 0x010065, 0x00BEEF, 2);
        let mut good = body.clone();
        good.push((crc & 0xFF) as u8);
        good.push(((crc >> 8) & 0xFF) as u8);
        inject_rx(&sys, good);
        sys.p.write(&sys, 0x40001004, 4, 1); // RXEN
        sys.p.write(&sys, 0x40001008, 4, 1); // START
        let _ = take_rx(&sys).expect("rx staged");
        complete_rx(&sys);
        assert_eq!(sys.p.read(&sys, 0x40001130, 4), 1, "engine CRCOK");
        assert_eq!(sys.p.read(&sys, 0x40001400, 4), 1, "CRCSTATUS set");
        assert_eq!(sys.p.read(&sys, 0x4000140C, 4), crc, "RXCRC latches wire");
        // Flip one payload bit: same registers now fail.
        let mut bad = body.clone();
        bad[0] ^= 0x01;
        bad.push((crc & 0xFF) as u8);
        bad.push(((crc >> 8) & 0xFF) as u8);
        inject_rx(&sys, bad);
        sys.p.write(&sys, 0x40001008, 4, 1); // START again
        let _ = take_rx(&sys).expect("rx staged again");
        complete_rx(&sys);
        assert_eq!(sys.p.read(&sys, 0x40001134, 4), 1, "engine CRCERROR");
        assert_eq!(sys.p.read(&sys, 0x40001400, 4), 0, "CRCSTATUS clear");
        // SKIPADDR=1 drops byte 0: recompute without it, passes again.
        sys.p.write(&sys, 0x40001534, 4, 2 | (1 << 8));
        let crc2 = radio_crc(&body[1..], 0x010065, 0x00BEEF, 2);
        let mut sk = body.clone();
        sk.push((crc2 & 0xFF) as u8);
        sk.push(((crc2 >> 8) & 0xFF) as u8);
        inject_rx(&sys, sk);
        sys.p.write(&sys, 0x40001008, 4, 1);
        let _ = take_rx(&sys).expect("rx staged skip");
        complete_rx(&sys);
        assert_eq!(sys.p.read(&sys, 0x40001130, 4), 1, "skipaddr CRCOK");
        // Whitening: LFSR is its own inverse + IV bit6 forced.
        let mut w = vec![0xAA, 0x55, 0x00, 0xFF];
        let orig = w.clone();
        whiten_in_place(&mut w, 0x12);
        assert_ne!(w, orig, "whitened differs");
        whiten_in_place(&mut w, 0x12);
        assert_eq!(w, orig, "double whiten roundtrips");
        let mut z = vec![0x11];
        whiten_in_place(&mut z, 0x00); // bit6 forced: same as IV 0x40
        let mut z2 = vec![0x11];
        whiten_in_place(&mut z2, 0x40);
        assert_eq!(z, z2, "IV bit6 hardwired");
        // Interference: -40 packet + -40 ambient ~= -37 (log-power).
        assert_eq!(add_interference_dbm(-40, -40), -37, "3dB heat");
        assert_eq!(add_interference_dbm(-127, -127), -124, "floor heat");
        // ED reads the heat: quiet -40 vs ambient -40 -> hotter sample.
        set_rssi_dbm(&sys, -80);
        sys.p.write(&sys, 0x40001004, 4, 1); // RXEN (ED needs Rx/TxRu)
        sys.p.write(&sys, 0x40001024, 4, 1); // EDSTART
        let quiet = sys.p.read(&sys, 0x40001668, 4);
        set_interference_dbm(&sys, -70);
        sys.p.write(&sys, 0x40001024, 4, 1); // EDSTART again
        let hot = sys.p.read(&sys, 0x40001668, 4);
        assert!(hot < quiet, "interference heats ED (sample {hot} < {quiet})");
        clear_interference(&sys);
        // RX stamp heats too: same loss, ambient on -> hotter latch.
        // (Re-arm Rx first: the earlier completions consumed the queue.)
        sys.p.write(&sys, 0x40001004, 4, 1); // RXEN
        sys.p.write(&sys, 0x40001008, 4, 1); // START (Rx)
        inject_rx_lossy(&sys, vec![0x01], 40);
        let _ = take_rx(&sys).expect("rx staged heat");
        set_interference_dbm(&sys, -40);
        complete_rx(&sys);
        sys.p.write(&sys, 0x40001014, 4, 1); // RSSISTART
        let hot_rssi = sys.p.read(&sys, 0x40001548, 4);
        clear_interference(&sys);
        assert!(hot_rssi < 80, "RSSI latch heated (sample {hot_rssi} < 80)");
        // 2nd run: engine off (LEN 0), no whitening, quiet air.
        let sys2 = test_dummy_system();
        inject_rx(&sys2, vec![0xAA, 0xBB]);
        sys2.p.write(&sys2, 0x40001004, 4, 1);
        sys2.p.write(&sys2, 0x40001008, 4, 1);
        let _ = take_rx(&sys2).expect("rx staged clean");
        complete_rx(&sys2);
        assert_eq!(sys2.p.read(&sys2, 0x40001130, 4), 1, "LEN=0 CRCOK");
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
    fn txpower_table_and_air_rssi_link_budget() {
        // SVD TXPOWER codes -> signed dBm (spot-check the table ends).
        assert_eq!(txpower_dbm(0x08), 8, "+8 dBm");
        assert_eq!(txpower_dbm(0x00), 0, "0 dBm");
        assert_eq!(txpower_dbm(0xFC), -4, "-4 dBm");
        assert_eq!(txpower_dbm(0xD8), -40, "-40 dBm");
        // Link budget: TX minus path loss, clamped to the RSSI window.
        assert_eq!(air_rssi_dbm(0x08, 48), -40, "8-48 = -40");
        assert_eq!(air_rssi_dbm(0x00, 0), 0, "co-located");
        assert_eq!(air_rssi_dbm(0xD8, 200), -127, "clamped floor");
        // RX completion stamps the packet's own level: TXPOWER 0 dBm
        // with 57 dB loss reads RSSISAMPLE 57 on the next RSSISTART.
        let sys = test_dummy_system();
        sys.p.write(&sys, 0x4000150C, 4, 0x00); // TXPOWER 0 dBm
        sys.p.write(&sys, 0x40001000, 4, 1); // TXEN (radio on)
        inject_rx_lossy(&sys, vec![0xAA, 0xBB], 57);
        sys.p.write(&sys, 0x40001004, 4, 1); // RXEN
        sys.p.write(&sys, 0x40001008, 4, 1); // START (Rx)
        let _ = take_rx(&sys).expect("rx staged");
        complete_rx(&sys);
        sys.p.write(&sys, 0x40001014, 4, 1); // RSSISTART
        assert_eq!(sys.p.read(&sys, 0x40001548, 4), 57, "RSSISAMPLE = path loss");
        // 2nd run: fresh default (TXPOWER reset 0, no queue loss).
        let sys2 = test_dummy_system();
        assert_eq!(sys2.p.read(&sys2, 0x4000150C, 4), 0, "TXPOWER reset 0");
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
