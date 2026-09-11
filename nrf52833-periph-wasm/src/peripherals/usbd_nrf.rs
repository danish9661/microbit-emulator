use crate::system::System;
use super::Peripheral;

/// USBD @ 0x40027000 (IRQ 39). Enumeration subset + endpoint EASYDMA:
///   TASKS_STARTEPIN[n] 0x000+n*4, TASKS_EP0RCVOUT 0x04C,
///   EVENTS_USBRESET 0x100 (host-driven: signal_usbreset()),
///   EVENTS_STARTED 0x104, EVENTS_ENDEPIN[n] 0x108+n*4,
///   EVENTS_EP0DATADONE 0x128, EVENTS_ENDEPOUT[n] 0x130+n*4,
///   EVENTS_EP0SETUP 0x15C, INTENSET 0x304/CLR 0x308,
///   SETUP packet regs BMREQUESTTYPE 0x480 .. WLENGTHH 0x49C,
///   ENABLE 0x500, USBPULLUP 0x504, EPINEN 0x510, EPOUTEN 0x514,
///   EPIN[n].PTR 0x600+n*0x14 / MAXCNT+4 / AMOUNT+8,
///   EPOUT[n].PTR 0x700+n*0x14 / MAXCNT+4 / AMOUNT+8.
/// DMA rule (same as UARTE): STARTEPIN[n] with MAXCNT>0 stages a driver
/// transfer (take_epin -> mem_read -> complete_epin); MAXCNT==0 completes
/// at once. SETUP packets arrive via inject_setup (host).
pub struct UsbdNrf {
    enabled: bool,
    pullup: bool,
    epinen: u32,
    epouten: u32,
    ev_usbreset: bool,
    ev_started: bool,
    ev_endepin: [bool; 8],
    ev_ep0datadone: bool,
    ev_endepout: [bool; 8],
    ev_ep0setup: bool,
    intenset: u32,
    setup: [u32; 8],
    epin_ptr: [u32; 8],
    epin_maxcnt: [u32; 8],
    epin_amount: [u32; 8],
    epin_pending: [bool; 8],
    epout_ptr: [u32; 8],
    epout_maxcnt: [u32; 8],
    epout_amount: [u32; 8],
    epout_pending: [bool; 8],
}

impl Default for UsbdNrf {
    fn default() -> Self {
        Self { enabled: false, pullup: false, epinen: 0, epouten: 0,
               ev_usbreset: false, ev_started: false, ev_endepin: [false; 8],
               ev_ep0datadone: false, ev_endepout: [false; 8], ev_ep0setup: false,
               intenset: 0, setup: [0; 8],
               epin_ptr: [0; 8], epin_maxcnt: [0; 8], epin_amount: [0; 8], epin_pending: [false; 8],
               epout_ptr: [0; 8], epout_maxcnt: [0; 8], epout_amount: [0; 8], epout_pending: [false; 8] }
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
}

impl Peripheral for UsbdNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x100 => self.ev_usbreset as u32,
            0x104 => self.ev_started as u32,
            0x108..=0x124 => self.ev_endepin[((offset - 0x108) >> 2) as usize] as u32,
            0x128 => self.ev_ep0datadone as u32,
            0x130..=0x14C => self.ev_endepout[((offset - 0x130) >> 2) as usize] as u32,
            0x15C => self.ev_ep0setup as u32,
            0x304 => self.intenset,
            0x480..=0x49C => self.setup[((offset - 0x480) >> 2) as usize],
            0x500 => self.enabled as u32,
            0x504 => self.pullup as u32,
            0x510 => self.epinen,
            0x514 => self.epouten,
            0x600..=0x694 if ((offset - 0x600) % 0x14) < 0xC => {
                let n = ((offset - 0x600) / 0x14) as usize;
                match (offset - 0x600) % 0x14 {
                    0x0 => self.epin_ptr[n],
                    0x4 => self.epin_maxcnt[n],
                    _ => self.epin_amount[n],
                }
            }
            0x700..=0x794 if ((offset - 0x700) % 0x14) < 0xC => {
                let n = ((offset - 0x700) / 0x14) as usize;
                match (offset - 0x700) % 0x14 {
                    0x0 => self.epout_ptr[n],
                    0x4 => self.epout_maxcnt[n],
                    _ => self.epout_amount[n],
                }
            }
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x000..=0x01C => {
                let n = (offset >> 2) as usize;
                self.ev_endepin[n] = false;
                if self.epin_maxcnt[n] > 0 {
                    self.epin_pending[n] = true; // driver completes
                } else {
                    self.ev_endepin[n] = true;
                    self.fire(sys, 1 << 8);
                }
                self.ev_started = true;
            }
            0x04C => { // EP0RCVOUT: stage EPOUT[0] receive
                self.epout_pending[0] = self.epout_maxcnt[0] > 0;
                if !self.epout_pending[0] {
                    self.ev_ep0datadone = true;
                }
            }
            0x100 => if value == 0 { self.ev_usbreset = false; }
            0x104 => if value == 0 { self.ev_started = false; }
            0x108..=0x124 => if value == 0 { self.ev_endepin[((offset - 0x108) >> 2) as usize] = false; }
            0x128 => if value == 0 { self.ev_ep0datadone = false; }
            0x130..=0x14C => if value == 0 { self.ev_endepout[((offset - 0x130) >> 2) as usize] = false; }
            0x15C => if value == 0 { self.ev_ep0setup = false; }
            0x304 => self.intenset |= value,
            0x308 => self.intenset &= !value,
            0x500 => self.enabled = value & 1 == 1,
            0x504 => self.pullup = value & 1 == 1,
            0x510 => self.epinen = value & 0xFF,
            0x514 => self.epouten = value & 0xFF,
            0x600..=0x694 if ((offset - 0x600) % 0x14) < 0xC => {
                let n = ((offset - 0x600) / 0x14) as usize;
                match (offset - 0x600) % 0x14 {
                    0x0 => self.epin_ptr[n] = value,
                    0x4 => self.epin_maxcnt[n] = value & 0x3FF,
                    _ => {}
                }
            }
            0x700..=0x794 if ((offset - 0x700) % 0x14) < 0xC => {
                let n = ((offset - 0x700) / 0x14) as usize;
                match (offset - 0x700) % 0x14 {
                    0x0 => self.epout_ptr[n] = value,
                    0x4 => self.epout_maxcnt[n] = value & 0x3FF,
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

fn with_usbd<R>(sys: &System, f: impl FnOnce(&mut UsbdNrf) -> R) -> Option<R> {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4002_7000 {
            let mut b = slot.peripheral.borrow_mut();
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
/// raises EVENTS_EP0SETUP (+ IRQ when INTEN bit 23 is set).
pub fn inject_setup(sys: &System, pkt: [u8; 8]) {
    let fire = with_usbd(sys, |u| {
        for (i, &b) in pkt.iter().enumerate() {
            u.setup[i] = b as u32;
        }
        u.ev_ep0setup = true;
        u.intenset & (1 << 23) != 0
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
/// (+ ENDEPINn IRQ per INTEN bit 2+n, SVD ground truth).
pub fn complete_epin(sys: &System, ep: usize, data: &[u8]) {
    let fire = with_usbd(sys, |u| {
        u.epin_amount[ep] = data.len() as u32;
        u.ev_endepin[ep] = true;
        u.intenset & (1 << (2 + ep)) != 0
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
/// (+ ENDEPOUTn IRQ per INTEN bit 12+n).
pub fn complete_epout(sys: &System, ep: usize, amount: u32) {
    let fire = with_usbd(sys, |u| {
        u.epout_amount[ep] = amount;
        u.ev_endepout[ep] = true;
        u.ev_ep0datadone = true;
        u.intenset & (1 << (12 + ep)) != 0
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
    #[test]
    fn epin_dma_roundtrip() {
        use crate::system::test_dummy_system;
        let sys = test_dummy_system();
        sys.p.write(&sys, 0x40027600, 4, 0x20001000); // EPIN0.PTR
        sys.p.write(&sys, 0x40027604, 4, 8);          // EPIN0.MAXCNT
        sys.p.write(&sys, 0x40027510, 4, 1);          // EPINEN
        sys.p.write(&sys, 0x40027000, 4, 1);          // STARTEPIN0
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
        sys.p.write(&sys, 0x40027000, 4, 1);
        complete_epin(&sys, 0, &[0x12]);
        assert!(sys.p.nvic.borrow().has_pending(), "ENDEPIN0 IRQ pends");
    }
}
