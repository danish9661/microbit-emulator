use crate::system::System;
use super::Peripheral;

/// MWU @ 0x40020000 (IRQ 32, memory watch unit). REGIONEN 0x500,
/// EVENTS_REGION[n] / PREGION RA/WA events. Stub: REGIONEN RW, events
/// never fire (no watch configured = no surprise faults for firmware
/// that leaves the MWU at reset).
pub struct MwuNrf {
    regionen: u32,
}

impl Default for MwuNrf {
    fn default() -> Self {
        Self { regionen: 0 }
    }
}

impl MwuNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "MWU" { Some(Box::new(Self::default())) } else { None }
    }
}

impl Peripheral for MwuNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x500 => self.regionen,
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        if offset == 0x500 {
            self.regionen = value;
        }
    }
}

/// ECB @ 0x4000E000 (IRQ 14, AES-128 engine). TASKS_STARTECB 0x000,
/// TASKS_STOPECB 0x004, EVENTS_ENDECB 0x100, EVENTS_ERRORECB 0x104,
/// INTENSET 0x304 (ENDECB 0), ECBDATAPTR 0x504 -> {KEY[16], CLEAR[16]}
/// in RAM (result goes back to ENCRYPTED[16] at PTR+32).
/// Crypto runs driver-side (the model has no RAM handle): STARTECB stages
/// take_ecb() (dataptr); the driver encrypts and calls complete_ecb().
/// The native test does this with real AES-128 (FIPS-197 vector).
pub struct EcbNrf {
    ev_end: bool,
    ev_error: bool,
    intenset: u32,
    dataptr: u32,
    staged: bool,
}

impl Default for EcbNrf {
    fn default() -> Self {
        Self { ev_end: false, ev_error: false, intenset: 0, dataptr: 0, staged: false }
    }
}

impl EcbNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "ECB" { Some(Box::new(Self::default())) } else { None }
    }
    fn fire(&self, sys: &System, bit: u32) {
        if self.intenset & bit != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(14);
        }
    }
}

impl Peripheral for EcbNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x100 => self.ev_end as u32,
            0x104 => self.ev_error as u32,
            0x304 => self.intenset,
            0x504 => self.dataptr,
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => {
                self.ev_end = false;
                self.staged = self.dataptr != 0;
                if !self.staged {
                    self.ev_error = true;
                    self.fire(sys, 1 << 1);
                }
            }
            0x004 => self.staged = false,
            0x100 => if value == 0 { self.ev_end = false; }
            0x104 => if value == 0 { self.ev_error = false; }
            0x304 => self.intenset |= value & 3,
            0x308 => self.intenset &= !value,
            0x504 => self.dataptr = value,
            _ => {}
        }
    }
}

fn with_ecb<R>(sys: &System, f: impl FnOnce(&mut EcbNrf) -> R) -> Option<R> {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4000_E000 {
            let mut b = slot.peripheral.borrow_mut();
            if let Some(e) = b.as_any_mut().downcast_mut::<EcbNrf>() {
                return Some(f(e));
            }
            return None;
        }
    }
    None
}

/// Take a staged ECB job (dataptr); None when idle.
pub fn take_ecb(sys: &System) -> Option<u32> {
    with_ecb(sys, |e| {
        if e.staged {
            e.staged = false;
            Some(e.dataptr)
        } else {
            None
        }
    })
    .flatten()
}

/// Complete ECB: driver encrypted in place; ENDECB set (+ IRQ bit 0).
pub fn complete_ecb(sys: &System) {
    let fire = with_ecb(sys, |e| {
        e.ev_end = true;
        e.intenset & 1 != 0
    });
    if fire == Some(true) {
        sys.p.nvic.borrow_mut().set_intr_pending(14);
    }
}

/// AAR @ 0x4000F000 (IRQ 15, shared base with CCM below).
/// TASKS_START 0x000, TASKS_STOP 0x008, EVENTS_END 0x100,
/// EVENTS_RESOLVED 0x104, EVENTS_NOTRESOLVED 0x108, INTENSET 0x304
/// (END 0, RESOLVED 1, NOTRESOLVED 2), STATUS 0x400, ENABLE 0x500,
/// NIRK 0x504, IRKPTR 0x508, ADDRPTR 0x510, SCRATCHPTR 0x514.
/// Resolution runs driver-side: START stages take_aar() (irkptr, addrptr);
/// complete_aar(resolved) sets END + RESOLVED/NOTRESOLVED.
pub struct AarCcmNrf {
    enabled: bool,
    ev_end: bool,
    ev_resolved: bool,
    ev_notresolved: bool,
    intenset: u32,
    status: u32,
    irkptr: u32,
    addrptr: u32,
    staged_aar: bool,
}

impl Default for AarCcmNrf {
    fn default() -> Self {
        Self { enabled: false, ev_end: false, ev_resolved: false, ev_notresolved: false,
               intenset: 0, status: 0, irkptr: 0, addrptr: 0, staged_aar: false }
    }
}

impl AarCcmNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        // AAR only. CCM shares this base with a different task map
        // (KSGEN/CRYPT); it stays unmodeled (read-as-0) until firmware
        // needs it — aliasing the two would lie about BOTH task sets.
        if name == "AAR" {
            Some(Box::new(Self::default()))
        } else {
            None
        }
    }
    fn fire(&self, sys: &System, bit: u32) {
        if self.intenset & bit != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(15);
        }
    }
}

impl Peripheral for AarCcmNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x100 => self.ev_end as u32,
            0x104 => self.ev_resolved as u32,
            0x108 => self.ev_notresolved as u32,
            0x304 => self.intenset,
            0x400 => self.status,
            0x500 => self.enabled as u32,
            0x508 => self.irkptr,
            0x510 => self.addrptr,
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => {
                // AAR TASKS_START: stage unless IRKPTR is null.
                self.ev_end = false;
                self.staged_aar = self.irkptr != 0;
            }
            0x008 => self.staged_aar = false, // STOP
            0x100 => if value == 0 { self.ev_end = false; }
            0x104 => if value == 0 { self.ev_resolved = false; }
            0x108 => if value == 0 { self.ev_notresolved = false; }
            0x304 => self.intenset |= value & 7,
            0x308 => self.intenset &= !value,
            0x500 => self.enabled = value & 1 == 1,
            0x508 => self.irkptr = value,
            0x510 => self.addrptr = value,
            _ => {}
        }
    }
}

fn with_aar<R>(sys: &System, f: impl FnOnce(&mut AarCcmNrf) -> R) -> Option<R> {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4000_F000 {
            let mut b = slot.peripheral.borrow_mut();
            if let Some(a) = b.as_any_mut().downcast_mut::<AarCcmNrf>() {
                return Some(f(a));
            }
            return None;
        }
    }
    None
}

/// Take a staged AAR resolution (irkptr, addrptr); None when idle.
pub fn take_aar(sys: &System) -> Option<(u32, u32)> {
    with_aar(sys, |a| {
        if a.staged_aar {
            a.staged_aar = false;
            Some((a.irkptr, a.addrptr))
        } else {
            None
        }
    })
    .flatten()
}

/// Complete AAR: sets END + RESOLVED (or NOTRESOLVED) with INTEN IRQs.
pub fn complete_aar(sys: &System, resolved: bool) {
    let fire = with_aar(sys, |a| {
        a.ev_end = true;
        if resolved {
            a.ev_resolved = true;
            a.status = 1;
        } else {
            a.ev_notresolved = true;
            a.status = 0;
        }
        a.intenset
    });
    if let Some(en) = fire {
        if en & 0x7 != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(15);
        }
    }
}

/// I2S @ 0x40025000 (IRQ 37, audio). Offsets from nrf52833.svd:
/// TASKS_START 0x000, TASKS_STOP 0x004, EVENTS_RXPTRUPD 0x104,
/// EVENTS_STOPPED 0x108, EVENTS_TXPTRUPD 0x114, INTENSET 0x304
/// (RXPTRUPD 1, STOPPED 2, TXPTRUPD 5) / CLR 0x308, ENABLE 0x500,
/// CONFIG 0x504 (stored), RXD.PTR 0x538 / MAXCNT 0x53C,
/// TXD.PTR 0x540 / MAXCNT 0x544. All sample movement is EASYDMA:
/// START stages take_rx/take_tx (PTRUPD events fire); the driver moves
/// bytes and completes. TX bytes also land in a capture FIFO (browser
/// playback / test compare via i2s_take_capture).
pub struct I2sNrf {
    enabled: bool,
    config: u32,
    ev_rxptr: bool,
    ev_txptr: bool,
    ev_stopped: bool,
    intenset: u32,
    rx_ptr: u32,
    rx_maxcnt: u32,
    rx_pending: bool,
    tx_ptr: u32,
    tx_maxcnt: u32,
    tx_pending: bool,
}

impl Default for I2sNrf {
    fn default() -> Self {
        Self { enabled: false, config: 0, ev_rxptr: false, ev_txptr: false,
               ev_stopped: false, intenset: 0, rx_ptr: 0, rx_maxcnt: 0,
               rx_pending: false, tx_ptr: 0, tx_maxcnt: 0, tx_pending: false }
    }
}

impl I2sNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "I2S" { Some(Box::new(Self::default())) } else { None }
    }
    fn fire(&self, sys: &System, bit: u32) {
        if self.intenset & bit != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(37);
        }
    }
}

impl Peripheral for I2sNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x104 => self.ev_rxptr as u32,
            0x108 => self.ev_stopped as u32,
            0x114 => self.ev_txptr as u32,
            0x304 => self.intenset,
            0x500 => self.enabled as u32,
            0x504 => self.config,
            0x538 => self.rx_ptr,
            0x53C => self.rx_maxcnt,
            0x540 => self.tx_ptr,
            0x544 => self.tx_maxcnt,
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => {
                // START: consume both pointers (PTRUPD events), stage DMA.
                if self.rx_maxcnt > 0 {
                    self.rx_pending = true;
                    self.ev_rxptr = true;
                    self.fire(sys, 1 << 1);
                }
                if self.tx_maxcnt > 0 {
                    self.tx_pending = true;
                    self.ev_txptr = true;
                    self.fire(sys, 1 << 5);
                }
            }
            0x004 => {
                self.rx_pending = false;
                self.tx_pending = false;
                self.ev_stopped = true;
                self.fire(sys, 1 << 2);
            }
            0x104 => if value == 0 { self.ev_rxptr = false; }
            0x108 => if value == 0 { self.ev_stopped = false; }
            0x114 => if value == 0 { self.ev_txptr = false; }
            0x304 => self.intenset |= value & 0x27,
            0x308 => self.intenset &= !value,
            0x500 => self.enabled = value & 1 == 1,
            0x504 => self.config = value,
            0x538 => self.rx_ptr = value,
            0x53C => self.rx_maxcnt = value & 0xFFFF,
            0x540 => self.tx_ptr = value,
            0x544 => self.tx_maxcnt = value & 0xFFFF,
            _ => {}
        }
    }
}

fn with_i2s<R>(sys: &System, f: impl FnOnce(&mut I2sNrf) -> R) -> Option<R> {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4002_5000 {
            let mut b = slot.peripheral.borrow_mut();
            if let Some(i) = b.as_any_mut().downcast_mut::<I2sNrf>() {
                return Some(f(i));
            }
            return None;
        }
    }
    None
}

/// Take a staged RX transfer (ptr, maxcnt); None when idle.
pub fn take_i2s_rx(sys: &System) -> Option<(u32, u32)> {
    with_i2s(sys, |i| {
        if i.rx_pending {
            i.rx_pending = false;
            Some((i.rx_ptr, i.rx_maxcnt))
        } else {
            None
        }
    })
    .flatten()
}

/// Complete RX: driver wrote samples to RAM at PTR.
pub fn complete_i2s_rx(sys: &System) {
    with_i2s(sys, |_| {});
}

/// Take a staged TX transfer (ptr, maxcnt); None when idle.
pub fn take_i2s_tx(sys: &System) -> Option<(u32, u32)> {
    with_i2s(sys, |i| {
        if i.tx_pending {
            i.tx_pending = false;
            Some((i.tx_ptr, i.tx_maxcnt))
        } else {
            None
        }
    })
    .flatten()
}

/// Complete TX: `data` went on the wire; captured for playback/compare.
pub fn complete_i2s_tx(sys: &System, data: &[u8]) {
    let _ = sys;
    if let Some(m) = crate::system::i2s_capture() {
        m.lock().unwrap().extend_from_slice(data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn misc_stubs_handshake() {
        let sys = test_dummy_system();
        let mut m = MwuNrf::default();
        m.write(&sys, 0x500, 0x0F);
        assert_eq!(m.read(&sys, 0x500), 0x0F);
        let mut e = EcbNrf::default();
        e.write(&sys, 0x504, 0x20001000); // ECBDATAPTR
        e.write(&sys, 0x000, 1); // STARTECB stages
        assert_eq!(e.read(&sys, 0x100), 0, "END waits for driver");
        drop(e);
        let sys_e = test_dummy_system();
        sys_e.p.write(&sys_e, 0x4000E504, 4, 0x20001000);
        sys_e.p.write(&sys_e, 0x4000E000, 4, 1);
        assert!(take_ecb(&sys_e).is_some(), "staged");
        let mut a = AarCcmNrf::default();
        a.write(&sys, 0x500, 1);
        assert_eq!(a.read(&sys, 0x500), 1, "ENABLE reads back");
        // DMA path runs through the live map (take/complete need it):
        let sys2 = test_dummy_system();
        sys2.p.write(&sys2, 0x4000F508, 4, 0x20001000);
        sys2.p.write(&sys2, 0x4000F500, 4, 1);
        sys2.p.write(&sys2, 0x4000F000, 4, 1);
        assert_eq!(take_aar(&sys2), Some((0x20001000, 0)));
        complete_aar(&sys2, true);
        assert_eq!(sys2.p.read(&sys2, 0x4000F100, 4), 1, "END set");
        assert_eq!(sys2.p.read(&sys2, 0x4000F104, 4), 1, "RESOLVED set");
        let sys3 = test_dummy_system();
        sys3.p.write(&sys3, 0x40025500, 4, 1); // ENABLE
        sys3.p.write(&sys3, 0x40025538, 4, 0x20001000); // RXD.PTR
        sys3.p.write(&sys3, 0x4002553C, 4, 8); // RXD.MAXCNT
        sys3.p.write(&sys3, 0x40025540, 4, 0x20002000); // TXD.PTR
        sys3.p.write(&sys3, 0x40025544, 4, 8); // TXD.MAXCNT
        sys3.p.write(&sys3, 0x40025000, 4, 1); // START
        assert_eq!(take_i2s_rx(&sys3), Some((0x20001000, 8)));
        assert_eq!(take_i2s_tx(&sys3), Some((0x20002000, 8)));
        assert_eq!(sys3.p.read(&sys3, 0x40025104, 4), 1, "RXPTRUPD");
        assert_eq!(sys3.p.read(&sys3, 0x40025114, 4), 1, "TXPTRUPD");
        complete_i2s_rx(&sys3);
        complete_i2s_tx(&sys3, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(crate::system::i2s_take_capture(), vec![1, 2, 3, 4, 5, 6, 7, 8]);
        sys3.p.write(&sys3, 0x40025004, 4, 1); // STOP
        assert_eq!(sys3.p.read(&sys3, 0x40025108, 4), 1, "STOPPED");
        crate::system::i2s_clear();
    }
}
