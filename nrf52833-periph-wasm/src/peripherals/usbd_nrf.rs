use crate::system::System;
use super::Peripheral;

/// USBD @ 0x40027000 (IRQ 39). Full SVD face — enumeration + endpoint EASYDMA:
///   TASKS_STARTEPIN[n] 0x004+n*4, TASKS_STARTISOIN 0x024,
///   TASKS_STARTEPOUT[n] 0x028+n*4, TASKS_STARTISOOUT 0x048,
///   TASKS_EP0RCVOUT 0x04C, TASKS_EP0STATUS 0x050, TASKS_EP0STALL 0x054,
///   TASKS_DPDMDRIVE 0x058 / DPDMNODRIVE 0x05C (line-drive flag, stored),
///   EVENTS_USBRESET 0x100 (host-driven: signal_usbreset()),
///   EVENTS_STARTED 0x104, EVENTS_ENDEPIN[n] 0x108+n*4,
///   EVENTS_EP0DATADONE 0x128, EVENTS_ENDISOIN 0x12C,
///   EVENTS_ENDEPOUT[n] 0x130+n*4, EVENTS_ENDISOOUT 0x150,
///   EVENTS_SOF 0x154 (host-driven: signal_sof(), +FRAMECNTR),
///   EVENTS_USBEVENT 0x158 (host-driven: signal_usbevent(cause)),
///   EVENTS_EP0SETUP 0x15C, EVENTS_EPDATA 0x160,
///   SHORTS 0x200 (EP0DATADONE_STARTEPIN0 0 / _STARTEPOUT0 1 /
///   _EP0STATUS 2, ENDEPOUT0_EP0STATUS 3 / _EP0RCVOUT 4),
///   INTEN 0x300 / SET 0x304 / CLR 0x308 (all bits 0-24 defined:
///   USBRESET 0, STARTED 1, ENDEPINn 2+n, EP0DATADONE 10, ENDISOIN 11,
///   ENDEPOUTn 12+n, ENDISOOUT 20, SOF 21, USBEVENT 22, EP0SETUP 23,
///   EPDATA 24),
///   EVENTCAUSE 0x400 (ISOOUTCRC 0, SUSPEND 8, RESUME 9, USBWUALLOWED 10,
///   READY 11 — host-posted, write-1-clears),
///   HALTED.EPIN[n] 0x420+4n / EPOUT[n] 0x444+4n (stall flags: EPSTALL
///   command applies, RW),
///   SIZE.EPOUT[n] 0x4A0+4n / ISOOUT 0x4C0 (received byte counts),
///   EPSTATUS 0x468 (transfer-active: set by START tasks incl. ISO as
///   bit 8/24, cleared by driver completion),
///   EPDATASTATUS 0x46C (data-ready EPIN1-7 bits 1-7 / EPOUT1-7 bits
///   17-23, write-1-clears; any bit sets EPDATA),
///   USBADDR 0x470 (7-bit device address),
///   SETUP packet regs BMREQUESTTYPE 0x480 .. WLENGTHH 0x49C,
///   ENABLE 0x500, USBPULLUP 0x504, DPDMVALUE 0x508 (forced line state,
///   stored), DTOGGLE 0x50C (stored), EPINEN 0x510, EPOUTEN 0x514,
///   EPSTALL 0x518 (command: EP/IO/STALL applies to HALTED; RW store),
///   ISOSPLIT 0x51C, FRAMECNTR 0x520 (11-bit SOF counter),
///   LOWPOWER 0x52C, ISOINCONFIG 0x530 (RESPONSE bit0),
///   EPIN[n].PTR 0x600+n*0x14 / MAXCNT+4 / AMOUNT+8,
///   ISOIN.PTR 0x6A0 / MAXCNT 0x6A4 / AMOUNT 0x6A8,
///   EPOUT[n].PTR 0x700+n*0x14 / MAXCNT+4 / AMOUNT+8,
///   ISOOUT.PTR 0x7A0 / MAXCNT 0x7A4 / AMOUNT 0x7A8.
/// DMA rule (same as UARTE): STARTEPIN[n] with MAXCNT>0 and EPINEN stages
/// a driver transfer (take_epin -> mem_read -> complete_epin);
/// MAXCNT==0 completes at once. SETUP packets arrive via inject_setup
/// (host; clears an EP0STALL, like silicon). EP0STALL tasks a control
/// stall (internal flag, cleared by the next SETUP).
pub struct UsbdNrf {
    enabled: bool,
    pullup: bool,
    epinen: u32,
    epouten: u32,
    ev_usbreset: bool,
    ev_started: bool,
    ev_endepin: [bool; 8],
    ev_ep0datadone: bool,
    ev_endisoin: bool,
    ev_endepout: [bool; 8],
    ev_endisoout: bool,
    ev_sof: bool,
    ev_usbevent: bool,
    ev_ep0setup: bool,
    ev_epdata: bool,
    intenset: u32,
    shorts: u32,
    eventcause: u32,
    halted_in: u32,   // HALTED EPIN[8:0] (ISO = bit 8)
    halted_out: u32,  // HALTED EPOUT[8:0]
    epstatus_in: u32, // EPSTATUS transfer-active IN
    epstatus_out: u32,
    edata_in: u32,    // EPDATASTATUS EPIN1-7
    edata_out: u32,   // EPDATASTATUS EPOUT1-7
    size_out: [u32; 8],
    size_isoout: u32,
    usbaddr: u32,
    dpdmvalue: u32,
    dpdm_driving: bool,
    dtoggle: u32,
    epstall: u32,
    isosplit: u32,
    framecntr: u32,
    lowpower: bool,
    isoinconfig: u32,
    ep0stalled: bool,
    chaining: bool,
    setup: [u32; 8],
    epin_ptr: [u32; 8],
    epin_maxcnt: [u32; 8],
    epin_amount: [u32; 8],
    epin_pending: [bool; 8],
    isoin_ptr: u32,
    isoin_maxcnt: u32,
    isoin_amount: u32,
    isoin_pending: bool,
    epout_ptr: [u32; 8],
    epout_maxcnt: [u32; 8],
    epout_amount: [u32; 8],
    epout_pending: [bool; 8],
    isoout_ptr: u32,
    isoout_maxcnt: u32,
    isoout_amount: u32,
    isoout_pending: bool,
}

impl Default for UsbdNrf {
    fn default() -> Self {
        Self { enabled: false, pullup: false, epinen: 0, epouten: 0,
               ev_usbreset: false, ev_started: false, ev_endepin: [false; 8],
               ev_ep0datadone: false, ev_endisoin: false, ev_endepout: [false; 8],
               ev_endisoout: false, ev_sof: false, ev_usbevent: false,
               ev_ep0setup: false, ev_epdata: false,
               intenset: 0, shorts: 0, eventcause: 0,
               halted_in: 0, halted_out: 0, epstatus_in: 0, epstatus_out: 0,
               edata_in: 0, edata_out: 0, size_out: [0; 8], size_isoout: 0,
               usbaddr: 0, dpdmvalue: 0, dpdm_driving: false, dtoggle: 0,
               epstall: 0, isosplit: 0, framecntr: 0, lowpower: false,
               isoinconfig: 0, ep0stalled: false, chaining: false, setup: [0; 8],
               epin_ptr: [0; 8], epin_maxcnt: [0; 8], epin_amount: [0; 8], epin_pending: [false; 8],
               isoin_ptr: 0, isoin_maxcnt: 0, isoin_amount: 0, isoin_pending: false,
               epout_ptr: [0; 8], epout_maxcnt: [0; 8], epout_amount: [0; 8], epout_pending: [false; 8],
               isoout_ptr: 0, isoout_maxcnt: 0, isoout_amount: 0, isoout_pending: false }
    }
}

impl UsbdNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "USBD" { Some(Box::new(Self::default())) } else { None }
    }
    fn fire(&self, sys: &System, bit: u32) {
        if self.intenset & bit != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(39);
        }
    }
    /// Chain-free STARTEPIN[n] effect (task arm + EP0DATADONE short):
    /// stage the DMA when EPINEN + MAXCNT>0, else complete at once.
    /// Marks EPSTATUS, clears any stale ENDEPIN.
    fn do_startepin(&mut self, sys: &System, n: usize) {
        if n >= 8 {
            return;
        }
        self.ev_endepin[n] = false;
        self.epstatus_in |= 1 << n;
        if self.epinen & (1 << n) != 0 && self.epin_maxcnt[n] > 0 {
            self.epin_pending[n] = true; // driver completes
        } else if self.epinen & (1 << n) != 0 {
            self.ev_endepin[n] = true;
            self.fire(sys, 1 << (2 + n));
        }
        self.ev_started = true;
    }
    /// Chain-free STARTEPOUT[n] effect (task arm + EP0DATADONE short).
    fn do_startepout(&mut self, sys: &System, n: usize) {
        if n >= 8 {
            return;
        }
        self.ev_endepout[n] = false;
        self.epstatus_out |= 1 << n;
        if self.epouten & (1 << n) != 0 && self.epout_maxcnt[n] > 0 {
            self.epout_pending[n] = true; // driver completes
        } else if self.epouten & (1 << n) != 0 {
            self.ev_endepout[n] = true;
            self.fire(sys, 1 << (12 + n));
        }
        self.ev_started = true;
    }
    /// EP0RCVOUT effect (task arm + ENDEPOUT0_EP0RCVOUT short):
    /// stage EPOUT[0] receive, or DATADONE at once when empty.
    fn do_ep0rcvout(&mut self, sys: &System) {
        self.epout_pending[0] = self.epout_maxcnt[0] > 0;
        if !self.epout_pending[0] {
            self.set_ep0datadone(sys);
        }
    }
    /// EP0STATUS effect (task arm + ENDEPOUT0_EP0STATUS short): the status
    /// stage ACKs at once with EP0DATADONE.
    fn do_ep0status(&mut self, sys: &System) {
        self.set_ep0datadone(sys);
    }
    /// Latch EP0DATADONE (+IRQ 10) and consume the EP0DATADONE_* SHORTS.
    /// SHORTS respond to the EVENT from any source (task, short, or
    /// completion); the chaining guard makes consumption single-pass so
    /// EP0DATADONE_EP0STATUS (which re-latches DATADONE) terminates.
    fn set_ep0datadone(&mut self, sys: &System) {
        self.ev_ep0datadone = true;
        self.fire(sys, 1 << 10);
        if self.chaining {
            return;
        }
        self.chaining = true;
        if self.shorts & 1 != 0 {
            self.do_startepin(sys, 0); // EP0DATADONE_STARTEPIN0
        }
        if self.shorts & (1 << 1) != 0 {
            self.do_startepout(sys, 0); // EP0DATADONE_STARTEPOUT0
        }
        if self.shorts & (1 << 2) != 0 {
            self.set_ep0datadone(sys); // EP0DATADONE_EP0STATUS (guarded)
        }
        self.chaining = false;
    }
}

impl Peripheral for UsbdNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x100 => self.ev_usbreset as u32,
            0x104 => self.ev_started as u32,
            0x108..=0x124 => self.ev_endepin[((offset - 0x108) >> 2) as usize] as u32,
            0x128 => self.ev_ep0datadone as u32,
            0x12C => self.ev_endisoin as u32,
            0x130..=0x14C => self.ev_endepout[((offset - 0x130) >> 2) as usize] as u32,
            0x150 => self.ev_endisoout as u32,
            0x154 => self.ev_sof as u32,
            0x158 => self.ev_usbevent as u32,
            0x15C => self.ev_ep0setup as u32,
            0x160 => self.ev_epdata as u32,
            0x200 => self.shorts,
            0x300 => self.intenset, // INTEN reads the enable word
            0x304 => self.intenset,
            0x400 => self.eventcause,
            0x420..=0x440 if ((offset - 0x420) & 3) == 0 => {
                // HALTED.EPIN[8:0] (ISO = bit 8).
                (self.halted_in >> ((offset - 0x420) >> 2)) & 1
            }
            0x444..=0x464 if ((offset - 0x444) & 3) == 0 => {
                // HALTED.EPOUT[8:0].
                (self.halted_out >> ((offset - 0x444) >> 2)) & 1
            }
            0x468 => self.epstatus_in | (self.epstatus_out << 16),
            0x46C => (self.edata_in & 0xFE) | ((self.edata_out & 0xFE) << 16),
            0x470 => self.usbaddr,
            0x480..=0x49C => self.setup[((offset - 0x480) >> 2) as usize],
            0x4A0..=0x4BC if ((offset - 0x4A0) & 3) == 0 => {
                // SIZE.EPOUT[7:0].
                self.size_out[((offset - 0x4A0) >> 2) as usize]
            }
            0x4C0 => self.size_isoout, // SIZE.ISOOUT
            0x500 => self.enabled as u32,
            0x504 => self.pullup as u32,
            0x508 => self.dpdmvalue & 0x1F,
            0x50C => self.dtoggle & 0x3FF,
            0x510 => self.epinen,
            0x514 => self.epouten,
            0x518 => self.epstall & 0x1FF,
            0x51C => self.isosplit & 0xFFFF,
            0x520 => self.framecntr & 0x7FF,
            0x52C => self.lowpower as u32,
            0x530 => self.isoinconfig & 1,
            0x600..=0x694 if ((offset - 0x600) % 0x14) < 0xC => {
                let n = ((offset - 0x600) / 0x14) as usize;
                match (offset - 0x600) % 0x14 {
                    0x0 => self.epin_ptr[n],
                    0x4 => self.epin_maxcnt[n],
                    _ => self.epin_amount[n],
                }
            }
            0x6A0 => self.isoin_ptr,
            0x6A4 => self.isoin_maxcnt,
            0x6A8 => self.isoin_amount,
            0x700..=0x794 if ((offset - 0x700) % 0x14) < 0xC => {
                let n = ((offset - 0x700) / 0x14) as usize;
                match (offset - 0x700) % 0x14 {
                    0x0 => self.epout_ptr[n],
                    0x4 => self.epout_maxcnt[n],
                    _ => self.epout_amount[n],
                }
            }
            0x7A0 => self.isoout_ptr,
            0x7A4 => self.isoout_maxcnt,
            0x7A8 => self.isoout_amount,
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x004..=0x020 if (offset & 3) == 0 => {
                self.do_startepin(sys, ((offset - 0x004) >> 2) as usize);
            }
            0x024 => {
                // STARTISOIN: stage the isochronous IN transfer.
                self.ev_endisoin = false;
                self.epstatus_in |= 1 << 8;
                if self.epinen & (1 << 8) != 0 && self.isoin_maxcnt > 0 {
                    self.isoin_pending = true; // driver completes
                } else if self.epinen & (1 << 8) != 0 {
                    self.ev_endisoin = true;
                    self.fire(sys, 1 << 11);
                }
                self.ev_started = true;
            }
            0x028..=0x044 if (offset & 3) == 0 => {
                self.do_startepout(sys, ((offset - 0x028) >> 2) as usize);
            }
            0x048 => {
                // STARTISOOUT: stage the isochronous OUT transfer.
                self.ev_endisoout = false;
                self.epstatus_out |= 1 << 8;
                if self.epouten & (1 << 8) != 0 && self.isoout_maxcnt > 0 {
                    self.isoout_pending = true; // driver completes
                } else if self.epouten & (1 << 8) != 0 {
                    self.ev_endisoout = true;
                    self.fire(sys, 1 << 20);
                }
                self.ev_started = true;
            }
            0x04C => self.do_ep0rcvout(sys), // EP0RCVOUT
            0x050 => self.do_ep0status(sys),  // EP0STATUS
            0x054 => self.ep0stalled = true, // EP0STALL (cleared by SETUP)
            0x058 => self.dpdm_driving = true,  // DPDMDRIVE
            0x05C => self.dpdm_driving = false, // DPDMNODRIVE
            0x100 => if value == 0 { self.ev_usbreset = false; }
            0x104 => if value == 0 { self.ev_started = false; }
            0x108..=0x124 => if value == 0 { self.ev_endepin[((offset - 0x108) >> 2) as usize] = false; }
            0x128 => if value == 0 { self.ev_ep0datadone = false; }
            0x12C => if value == 0 { self.ev_endisoin = false; }
            0x130..=0x14C => if value == 0 { self.ev_endepout[((offset - 0x130) >> 2) as usize] = false; }
            0x150 => if value == 0 { self.ev_endisoout = false; }
            0x154 => if value == 0 { self.ev_sof = false; }
            0x158 => if value == 0 { self.ev_usbevent = false; }
            0x15C => if value == 0 { self.ev_ep0setup = false; }
            0x160 => if value == 0 { self.ev_epdata = false; }
            0x200 => self.shorts = value & 0x1F,
            0x300 => self.intenset = value & 0x1FF_FFFF, // INTEN absolute
            0x304 => self.intenset |= value & 0x1FF_FFFF,
            0x308 => self.intenset &= !value,
            0x400 => self.eventcause &= !value, // write-1-clears
            0x420..=0x440 if ((offset - 0x420) & 3) == 0 => {
                // HALTED.EPIN[n]: RW latch.
                let n = (offset - 0x420) >> 2;
                if value & 1 != 0 {
                    self.halted_in |= 1 << n;
                } else {
                    self.halted_in &= !(1 << n);
                }
            }
            0x444..=0x464 if ((offset - 0x444) & 3) == 0 => {
                let n = (offset - 0x444) >> 2;
                if value & 1 != 0 {
                    self.halted_out |= 1 << n;
                } else {
                    self.halted_out &= !(1 << n);
                }
            }
            0x46C => {
                // EPDATASTATUS write-1-clears (EPIN1-7 / EPOUT1-7 lanes).
                self.edata_in &= !(value & 0xFE);
                self.edata_out &= !((value >> 16) & 0xFE);
            }
            0x470 => self.usbaddr = value & 0x7F,
            0x500 => self.enabled = value & 1 == 1,
            0x504 => self.pullup = value & 1 == 1,
            0x508 => self.dpdmvalue = value & 0x1F,
            0x50C => self.dtoggle = value & 0x3FF,
            0x510 => self.epinen = value & 0x1FF, // EPINEN incl. ISO bit 8
            0x514 => self.epouten = value & 0x1FF,
            0x518 => {
                // EPSTALL command: EP/IO/STALL applies to the HALTED bit.
                self.epstall = value & 0x1FF;
                let ep = (value & 7) as usize;
                let io_in = value & (1 << 7) == 0;
                if value & (1 << 8) != 0 {
                    if io_in {
                        self.halted_in |= 1 << ep;
                    } else {
                        self.halted_out |= 1 << ep;
                    }
                } else if io_in {
                    self.halted_in &= !(1 << ep);
                } else {
                    self.halted_out &= !(1 << ep);
                }
            }
            0x51C => self.isosplit = value & 0xFFFF,
            0x52C => self.lowpower = value & 1 == 1,
            0x530 => self.isoinconfig = value & 1,
            0x600..=0x694 if ((offset - 0x600) % 0x14) < 0xC => {
                let n = ((offset - 0x600) / 0x14) as usize;
                match (offset - 0x600) % 0x14 {
                    0x0 => self.epin_ptr[n] = value,
                    0x4 => self.epin_maxcnt[n] = value & 0x3FF,
                    _ => {}
                }
            }
            0x6A0 => self.isoin_ptr = value,
            0x6A4 => self.isoin_maxcnt = value & 0x3FF,
            0x700..=0x794 if ((offset - 0x700) % 0x14) < 0xC => {
                let n = ((offset - 0x700) / 0x14) as usize;
                match (offset - 0x700) % 0x14 {
                    0x0 => self.epout_ptr[n] = value,
                    0x4 => self.epout_maxcnt[n] = value & 0x3FF,
                    _ => {}
                }
            }
            0x7A0 => self.isoout_ptr = value,
            0x7A4 => self.isoout_maxcnt = value & 0x3FF,
            _ => {}
        }
    }
}

fn with_usbd<R>(sys: &System, f: impl FnOnce(&mut UsbdNrf) -> R) -> Option<R> {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4002_7000 {
            // try_borrow_mut (P108 family): take/complete paths re-enter
            // via read/write/tick while borrowed; drop instead of panic.
            let mut b = match slot.peripheral.try_borrow_mut() {
                Ok(b) => b,
                Err(_) => return None,
            };
            if let Some(u) = b.as_any_mut().downcast_mut::<UsbdNrf>() {
                return Some(f(u));
            }
            return None;
        }
    }
    None
}

/// Host-side USB reset injection (enumeration start): sets EVENTS_USBRESET
/// (+ IRQ when INTEN bit 0 is set, SVD ground truth).
pub fn signal_usbreset(sys: &System) {
    let fire = with_usbd(sys, |u| {
        u.ev_usbreset = true;
        u.intenset & 1 != 0
    });
    if fire == Some(true) {
        sys.p.nvic.borrow_mut().set_intr_pending(39);
    }
}

/// Host-side SETUP packet injection (8 bytes): fills the SETUP regs and
/// raises EVENTS_EP0SETUP (+ IRQ when INTEN bit 23 is set). Clears an
/// EP0STALL, like silicon.
pub fn inject_setup(sys: &System, pkt: [u8; 8]) {
    let fire = with_usbd(sys, |u| {
        for (i, &b) in pkt.iter().enumerate() {
            u.setup[i] = b as u32;
        }
        u.ev_ep0setup = true;
        u.ep0stalled = false;
        u.intenset & (1 << 23) != 0
    });
    if fire == Some(true) {
        sys.p.nvic.borrow_mut().set_intr_pending(39);
    }
}

/// Host-side SOF injection (1 ms USB frame): latches EVENTS_SOF
/// (+ IRQ 21 when INTENabled) and counts FRAMECNTR (11-bit wrap).
pub fn signal_sof(sys: &System) {
    let fire = with_usbd(sys, |u| {
        u.ev_sof = true;
        u.framecntr = (u.framecntr + 1) & 0x7FF;
        u.intenset & (1 << 21) != 0
    });
    if fire == Some(true) {
        sys.p.nvic.borrow_mut().set_intr_pending(39);
    }
}

/// Host-side USB event injection (suspend/resume/wakeup-ready): ORs
/// `cause` into EVENTCAUSE (ISOOUTCRC 0, SUSPEND 8, RESUME 9,
/// USBWUALLOWED 10, READY 11) and latches EVENTS_USBEVENT (+ IRQ 22
/// when INTENabled).
pub fn signal_usbevent(sys: &System, cause: u32) {
    let fire = with_usbd(sys, |u| {
        u.eventcause |= cause & 0xF01;
        u.ev_usbevent = true;
        u.intenset & (1 << 22) != 0
    });
    if fire == Some(true) {
        sys.p.nvic.borrow_mut().set_intr_pending(39);
    }
}

/// Take a staged ISOIN transfer (ptr, maxcnt); None when idle.
pub fn take_isoin(sys: &System) -> Option<(u32, u32)> {
    with_usbd(sys, |u| {
        if u.isoin_pending {
            u.isoin_pending = false;
            Some((u.isoin_ptr, u.isoin_maxcnt))
        } else {
            None
        }
    })
    .flatten()
}

/// Complete ISOIN: bytes went on the wire; AMOUNT + ENDISOIN set
/// (+ IRQ 11 when INTENabled). Clears the EPSTATUS ISO bit.
pub fn complete_isoin(sys: &System, data: &[u8]) {
    let fire = with_usbd(sys, |u| {
        u.isoin_amount = data.len() as u32;
        u.ev_endisoin = true;
        u.epstatus_in &= !(1 << 8);
        u.intenset & (1 << 11) != 0
    });
    if fire == Some(true) {
        sys.p.nvic.borrow_mut().set_intr_pending(39);
    }
}

/// Take a staged ISOOUT transfer (ptr, maxcnt); None when idle.
pub fn take_isoout(sys: &System) -> Option<(u32, u32)> {
    with_usbd(sys, |u| {
        if u.isoout_pending {
            u.isoout_pending = false;
            Some((u.isoout_ptr, u.isoout_maxcnt))
        } else {
            None
        }
    })
    .flatten()
}

/// Complete ISOOUT: driver wrote `amount` bytes to RAM at PTR; AMOUNT +
/// SIZE.ISOOUT + ENDISOOUT set (+ IRQ 20 when INTENabled). Clears the
/// EPSTATUS ISO bit.
pub fn complete_isoout(sys: &System, amount: u32) {
    let fire = with_usbd(sys, |u| {
        u.isoout_amount = amount;
        u.size_isoout = amount;
        u.ev_endisoout = true;
        u.epstatus_out &= !(1 << 8);
        u.intenset & (1 << 20) != 0
    });
    if fire == Some(true) {
        sys.p.nvic.borrow_mut().set_intr_pending(39);
    }
}

/// Take a staged EPIN transfer (ep, ptr, maxcnt); None when idle.
pub fn take_epin(sys: &System) -> Option<(usize, u32, u32)> {
    with_usbd(sys, |u| {
        u.epin_pending.iter().position(|&p| p).map(|n| {
            u.epin_pending[n] = false;
            (n, u.epin_ptr[n], u.epin_maxcnt[n])
        })
    })
    .flatten()
}

/// Complete EPIN: bytes went on the wire; AMOUNT + ENDEPIN set
/// (+ ENDEPINn IRQ per INTEN bit 2+n, SVD ground truth). Clears the
/// EPSTATUS transfer bit; EPIN1-7 raise EPDATASTATUS + EPDATA (+IRQ 24).
pub fn complete_epin(sys: &System, ep: usize, data: &[u8]) {
    let fire = with_usbd(sys, |u| {
        u.epin_amount[ep] = data.len() as u32;
        u.ev_endepin[ep] = true;
        u.epstatus_in &= !(1 << ep);
        let mut irq = u.intenset & (1 << (2 + ep)) != 0;
        if ep >= 1 {
            u.edata_in |= 1 << ep;
            u.ev_epdata = true;
            irq |= u.intenset & (1 << 24) != 0;
        }
        irq
    });
    if fire == Some(true) {
        sys.p.nvic.borrow_mut().set_intr_pending(39);
    }
}

/// Take a staged EPOUT transfer (ep, ptr, maxcnt); None when idle.
pub fn take_epout(sys: &System) -> Option<(usize, u32, u32)> {
    with_usbd(sys, |u| {
        u.epout_pending.iter().position(|&p| p).map(|n| {
            u.epout_pending[n] = false;
            (n, u.epout_ptr[n], u.epout_maxcnt[n])
        })
    })
    .flatten()
}

/// Complete EPOUT: driver wrote `amount` bytes to RAM at PTR
/// (+ ENDEPOUTn IRQ per INTEN bit 12+n). Clears the EPSTATUS transfer
/// bit and reports SIZE; EPOUT1-7 raise EPDATASTATUS + EPDATA (+IRQ 24).
/// EPOUT0 (control) latches EP0DATADONE (+IRQ 10) and consumes the
/// ENDEPOUT0_EP0STATUS / _EP0RCVOUT shorts once.
pub fn complete_epout(sys: &System, ep: usize, amount: u32) {
    let fire = with_usbd(sys, |u| {
        u.epout_amount[ep] = amount;
        u.size_out[ep] = amount;
        u.ev_endepout[ep] = true;
        u.epstatus_out &= !(1 << ep);
        let mut irq = u.intenset & (1 << (12 + ep)) != 0;
        if ep >= 1 {
            u.edata_out |= 1 << ep;
            u.ev_epdata = true;
            irq |= u.intenset & (1 << 24) != 0;
        } else {
            u.set_ep0datadone(sys); // auto-chains EP0DATADONE_* (guarded)
            irq |= u.intenset & (1 << 10) != 0;
            if u.shorts & (1 << 3) != 0 {
                u.do_ep0status(sys); // ENDEPOUT0_EP0STATUS
            }
            if u.shorts & (1 << 4) != 0 {
                u.do_ep0rcvout(sys); // ENDEPOUT0_EP0RCVOUT
            }
        }
        irq
    });
    if fire == Some(true) {
        sys.p.nvic.borrow_mut().set_intr_pending(39);
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
        u.write(&sys, 0x004, 1); // STARTEPIN0 (SVD)
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
    #[test]
    fn epin_dma_roundtrip() {
        use crate::system::test_dummy_system;
        let sys = test_dummy_system();
        sys.p.write(&sys, 0x40027600, 4, 0x20001000); // EPIN0.PTR
        sys.p.write(&sys, 0x40027604, 4, 8);          // EPIN0.MAXCNT
        sys.p.write(&sys, 0x40027510, 4, 1);          // EPINEN
        sys.p.write(&sys, 0x40027004, 4, 1);          // STARTEPIN0 (SVD)
        assert_eq!(sys.p.read(&sys, 0x40027108, 4), 0, "ENDEPIN waits");
        let t = take_epin(&sys).expect("staged");
        assert_eq!(t, (0, 0x20001000, 8));
        complete_epin(&sys, 0, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(sys.p.read(&sys, 0x40027108, 4), 1, "ENDEPIN set");
    }
    #[test]
    fn setup_injection_readable() {
        use crate::system::test_dummy_system;
        let sys = test_dummy_system();
        inject_setup(&sys, [0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 0x40, 0x00]);
        assert_eq!(sys.p.read(&sys, 0x4002715C, 4), 1, "EP0SETUP set");
        assert_eq!(sys.p.read(&sys, 0x40027480, 4), 0x80, "BMREQUESTTYPE");
        assert_eq!(sys.p.read(&sys, 0x40027484, 4), 0x06, "BREQUEST=GET_DESCRIPTOR");
    }
    #[test]
    fn epin_completion_irq_when_enabled() {
        use crate::system::test_dummy_system;
        let sys = test_dummy_system();
        sys.p.write(&sys, 0xE000E104, 4, 1 << (39 - 32)); // NVIC ISER1: USBD
        sys.p.write(&sys, 0x40027304, 4, 1 << 2); // INTEN: ENDEPIN0
        sys.p.write(&sys, 0x40027600, 4, 0x20001000);
        sys.p.write(&sys, 0x40027604, 4, 1);
        sys.p.write(&sys, 0x40027510, 4, 1); // EPINEN
        sys.p.write(&sys, 0x40027004, 4, 1); // STARTEPIN0 (SVD)
        complete_epin(&sys, 0, &[0x12]);
        assert!(sys.p.nvic.borrow().has_pending(), "ENDEPIN0 IRQ pends");
    }
    #[test]
    fn full_face_epout_iso_sof_usbevent_epdata_status() {
        // STARTEPOUT + ISO pair + SOF/USBEVENT + EPDATA/EPSTATUS/SIZE +
        // EP0STATUS/EP0STALL/DPDM/ADDR/config + SHORTS chain.
        let sys = test_dummy_system();
        sys.p.write(&sys, 0xE000E104, 4, 1 << (39 - 32)); // NVIC ISER1: USBD
        sys.p.write(&sys, 0x40027304, 4, (1 << 12) | (1 << 20) | (1 << 21) | (1 << 22) | (1 << 24) | (1 << 10));
        sys.p.write(&sys, 0x40027500, 4, 1); // ENABLE
        // Config block.
        sys.p.write(&sys, 0x40027508, 4, 0x0D); // DPDMVALUE
        sys.p.write(&sys, 0x4002750C, 4, 0x123); // DTOGGLE
        sys.p.write(&sys, 0x4002751C, 4, 0x40); // ISOSPLIT
        sys.p.write(&sys, 0x4002752C, 4, 1); // LOWPOWER
        sys.p.write(&sys, 0x40027530, 4, 1); // ISOINCONFIG
        sys.p.write(&sys, 0x40027470, 4, 0x2A); // USBADDR
        assert_eq!(sys.p.read(&sys, 0x40027508, 4), 0x0D, "DPDMVALUE");
        assert_eq!(sys.p.read(&sys, 0x40027470, 4), 0x2A, "USBADDR");
        // EPSTALL command stalls EPIN1; HALTED latches.
        sys.p.write(&sys, 0x40027518, 4, (1 << 8) | 1); // EP=1 IN STALL
        assert_eq!(sys.p.read(&sys, 0x40027420 + 4, 4), 1, "HALTED.EPIN1");
        sys.p.write(&sys, 0x40027518, 4, 1); // EP=1 IN unstall
        assert_eq!(sys.p.read(&sys, 0x40027420 + 4, 4), 0, "unhalted");
        // EPOUT1 roundtrip: START -> take -> complete -> ENDEPOUT + SIZE + EPDATA.
        sys.p.write(&sys, 0x40027714, 4, 0x20001000); // EPOUT1.PTR (stride 0x14)
        sys.p.write(&sys, 0x40027718, 4, 8); // EPOUT1.MAXCNT
        sys.p.write(&sys, 0x40027514, 4, 1 << 1); // EPOUTEN bit1
        sys.p.write(&sys, 0x4002702C, 4, 1); // STARTEPOUT1 (SVD 0x028+4)
        assert_eq!(sys.p.read(&sys, 0x40027468, 4) & (1 << 17), 1 << 17, "EPSTATUS EPOUT1");
        let t = take_epout(&sys).expect("epout staged");
        assert_eq!(t, (1, 0x20001000, 8));
        complete_epout(&sys, 1, 5);
        assert_eq!(sys.p.read(&sys, 0x40027134, 4), 1, "ENDEPOUT1");
        assert_eq!(sys.p.read(&sys, 0x400274A4, 4), 5, "SIZE.EPOUT1");
        assert_eq!(sys.p.read(&sys, 0x40027468, 4) & (1 << 17), 0, "EPSTATUS cleared");
        assert_eq!(sys.p.read(&sys, 0x4002746C, 4) & (1 << 17), 1 << 17, "EPDATASTATUS EPOUT1");
        assert_eq!(sys.p.read(&sys, 0x40027160, 4), 1, "EPDATA");
        sys.p.write(&sys, 0x4002746C, 4, 1 << 17); // write-1-clear
        assert_eq!(sys.p.read(&sys, 0x4002746C, 4) & (1 << 17), 0, "EDATA cleared");
        // ISO pair.
        sys.p.write(&sys, 0x400276A0, 4, 0x20002000); // ISOIN.PTR
        sys.p.write(&sys, 0x400276A4, 4, 4);
        sys.p.write(&sys, 0x400277A0, 4, 0x20003000); // ISOOUT.PTR
        sys.p.write(&sys, 0x400277A4, 4, 4);
        sys.p.write(&sys, 0x40027510, 4, 1 << 8); // EPINEN ISO
        sys.p.write(&sys, 0x40027514, 4, 1 << 8); // EPOUTEN ISO
        sys.p.write(&sys, 0x40027024, 4, 1); // STARTISOIN
        assert_eq!(take_isoin(&sys), Some((0x20002000, 4)));
        complete_isoin(&sys, &[1, 2, 3]);
        assert_eq!(sys.p.read(&sys, 0x4002712C, 4), 1, "ENDISOIN");
        sys.p.write(&sys, 0x40027048, 4, 1); // STARTISOOUT
        assert_eq!(take_isoout(&sys), Some((0x20003000, 4)));
        complete_isoout(&sys, 2);
        assert_eq!(sys.p.read(&sys, 0x40027150, 4), 1, "ENDISOOUT");
        assert_eq!(sys.p.read(&sys, 0x400274C0, 4), 2, "SIZE.ISOOUT");
        assert!(sys.p.nvic.borrow().has_pending(), "IRQ 39 pends");
        // SOF + USBEVENT host legs.
        signal_sof(&sys);
        assert_eq!(sys.p.read(&sys, 0x40027154, 4), 1, "SOF");
        assert_eq!(sys.p.read(&sys, 0x40027520, 4), 1, "FRAMECNTR counts");
        signal_usbevent(&sys, (1 << 8) | (1 << 11)); // SUSPEND + READY
        assert_eq!(sys.p.read(&sys, 0x40027158, 4), 1, "USBEVENT");
        assert_eq!(sys.p.read(&sys, 0x40027400, 4) & 0xF01, (1 << 8) | (1 << 11), "EVENTCAUSE");
        sys.p.write(&sys, 0x40027400, 4, 1 << 8); // write-1-clear SUSPEND
        assert_eq!(sys.p.read(&sys, 0x40027400, 4) & 0xF01, 1 << 11, "SUSPEND cleared");
        // EP0STATUS task ACKs with DATADONE; EP0DATADONE_STARTEPIN0 short chains.
        sys.p.write(&sys, 0x40027200, 4, 1 << 0); // SHORTS EP0DATADONE_STARTEPIN0
        sys.p.write(&sys, 0x40027510, 4, 1); // EPINEN bit0
        sys.p.write(&sys, 0x40027050, 4, 1); // EP0STATUS
        assert_eq!(sys.p.read(&sys, 0x40027128, 4), 1, "EP0DATADONE");
        // 2nd run: fresh defaults.
        let u2 = UsbdNrf::default();
        assert_eq!((u2.framecntr, u2.eventcause), (0, 0));
    }
}
