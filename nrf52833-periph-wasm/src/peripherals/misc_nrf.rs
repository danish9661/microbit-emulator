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

/// I2S @ 0x40025000 (IRQ 37, audio). TASKS_START 0x000, TASKS_STOP 0x004,
/// EVENTS_RXPTRUPD 0x100, EVENTS_TXPTRUPD 0x104, EVENTS_STOPPED 0x108,
/// RXD.PTR 0x538, TXD.PTR 0x540, ENABLE 0x500, CONFIG.*.
/// Stub: START->TXPTRUPD+RXPTRUPD, STOP->STOPPED. Sample streaming is P8.
pub struct I2sNrf {
    enabled: bool,
    ev_rxptr: bool,
    ev_txptr: bool,
    ev_stopped: bool,
}

impl Default for I2sNrf {
    fn default() -> Self {
        Self { enabled: false, ev_rxptr: false, ev_txptr: false, ev_stopped: false }
    }
}

impl I2sNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "I2S" { Some(Box::new(Self::default())) } else { None }
    }
}

impl Peripheral for I2sNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x100 => self.ev_rxptr as u32,
            0x104 => self.ev_txptr as u32,
            0x108 => self.ev_stopped as u32,
            0x500 => self.enabled as u32,
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => { self.ev_rxptr = true; self.ev_txptr = true; }
            0x004 => self.ev_stopped = true,
            0x100 => if value == 0 { self.ev_rxptr = false; }
            0x104 => if value == 0 { self.ev_txptr = false; }
            0x108 => if value == 0 { self.ev_stopped = false; }
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
    fn misc_stubs_handshake() {
        let sys = test_dummy_system();
        let mut m = MwuNrf::default();
        m.write(&sys, 0x500, 0x0F);
        assert_eq!(m.read(&sys, 0x500), 0x0F);
        let mut e = EcbNrf::default();
        e.write(&sys, 0x000, 1);
        assert_eq!(e.read(&sys, 0x100), 1);
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
        let mut i = I2sNrf::default();
        i.write(&sys, 0x500, 1);
        i.write(&sys, 0x000, 1);
        assert_eq!(i.read(&sys, 0x104), 1);
        i.write(&sys, 0x004, 1);
        assert_eq!(i.read(&sys, 0x108), 1);
    }
}
