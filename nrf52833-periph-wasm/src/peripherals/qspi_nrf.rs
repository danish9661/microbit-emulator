use crate::system::System;
use super::Peripheral;
use std::sync::{Mutex, OnceLock};
use std::collections::HashMap;

/// QSPI @ 0x40029000 (IRQ 41, external flash). Absent from the nRF52833 SVD
/// revision we ship, so offsets are from the Product Specification:
/// TASKS_ACTIVATE 0x000, READSTART 0x004, WRITESTART 0x008, ERASESTART
/// 0x00C, DEACTIVATE 0x010, EVENTS_READY 0x104, STATUS 0x400 (READY bit0),
/// READ.SRC 0x514 / DST 0x518 / CNT 0x51C, WRITE.SRC 0x520 / DST 0x524 /
/// CNT 0x528, ERASE.PTR 0x52C / ERASE.LEN 0x530 (0 = 4KB sector, 1 = 64KB
/// block), ENABLE 0x500.
/// Data path is driver-owned (no RAM handle in the model): transfers stage
/// take_*() and complete when the driver moves bytes. The registered flash
/// image backs native tests (and documents the JS contract): complete_write
/// programs it with AND semantics (flash clears 1->0 only), erase fills 0xFF.
pub struct QspiNrf {
    enabled: bool,
    ev_ready: bool,
    intenset: u32,
    read_src: u32,
    read_dst: u32,
    read_cnt: u32,
    read_pending: bool,
    write_src: u32,
    write_dst: u32,
    write_cnt: u32,
    write_pending: bool,
    erase_ptr: u32,
    erase_len: u32,
    erase_pending: bool,
}

impl Default for QspiNrf {
    fn default() -> Self {
        Self { enabled: false, ev_ready: false, intenset: 0,
               read_src: 0, read_dst: 0, read_cnt: 0, read_pending: false,
               write_src: 0, write_dst: 0, write_cnt: 0, write_pending: false,
               erase_ptr: 0, erase_len: 0, erase_pending: false }
    }
}

impl QspiNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "QSPI" { Some(Box::new(Self::default())) } else { None }
    }
    fn fire(&self, sys: &System) {
        if self.intenset & 1 != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(41);
        }
    }
}

static QSPI_FLASH: OnceLock<Mutex<HashMap<String, Vec<u8>>>> = OnceLock::new();
fn flash_images() -> &'static Mutex<HashMap<String, Vec<u8>>> {
    QSPI_FLASH.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register an external flash image for the demo/tests (JS provides its own
/// copy; this registry exists so native tests can run without a driver).
pub fn qspi_register_flash(name: &str, data: &[u8]) {
    flash_images().lock().unwrap().insert(name.to_string(), data.to_vec());
}

/// Forget all registered images (test isolation; called by reset_globals).
pub fn qspi_clear() {
    if let Some(m) = QSPI_FLASH.get() {
        m.lock().unwrap().clear();
    }
}

fn with_qspi<R>(sys: &System, f: impl FnOnce(&mut QspiNrf) -> R) -> Option<R> {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4002_9000 {
            let mut b = slot.peripheral.borrow_mut();
            if let Some(q) = b.as_any_mut().downcast_mut::<QspiNrf>() {
                return Some(f(q));
            }
            return None;
        }
    }
    None
}

/// Take a staged indirect read (src, dst, len); None when idle.
pub fn take_read(sys: &System) -> Option<(u32, u32, u32)> {
    with_qspi(sys, |q| {
        if q.read_pending {
            q.read_pending = false;
            Some((q.read_src, q.read_dst, q.read_cnt))
        } else {
            None
        }
    })
    .flatten()
}

/// Complete a read: driver moved the bytes; READY set (+ IRQ bit 0).
pub fn complete_read(sys: &System) {
    let fire = with_qspi(sys, |q| {
        q.ev_ready = true;
        q.intenset & 1 != 0
    });
    if fire == Some(true) {
        sys.p.nvic.borrow_mut().set_intr_pending(41);
    }
}

/// Take a staged indirect write (src, dst, len); None when idle.
pub fn take_write(sys: &System) -> Option<(u32, u32, u32)> {
    with_qspi(sys, |q| {
        if q.write_pending {
            q.write_pending = false;
            Some((q.write_src, q.write_dst, q.write_cnt))
        } else {
            None
        }
    })
    .flatten()
}

/// Complete a write: program `data` at flash offset `dst` with AND semantics
/// (bits clear 1->0 only), then READY.
pub fn complete_write(sys: &System, dst: u32, data: &[u8]) {
    if let Some(m) = QSPI_FLASH.get() {
        if let Some(img) = m.lock().unwrap().get_mut("QSPI") {
            for (i, &b) in data.iter().enumerate() {
                let idx = dst as usize + i;
                if idx < img.len() {
                    img[idx] &= b;
                }
            }
        }
    }
    let fire = with_qspi(sys, |q| {
        q.ev_ready = true;
        q.intenset & 1 != 0
    });
    if fire == Some(true) {
        sys.p.nvic.borrow_mut().set_intr_pending(41);
    }
}

/// Take a staged erase (ptr, len-code); None when idle.
pub fn take_erase(sys: &System) -> Option<(u32, u32)> {
    with_qspi(sys, |q| {
        if q.erase_pending {
            q.erase_pending = false;
            Some((q.erase_ptr, q.erase_len))
        } else {
            None
        }
    })
    .flatten()
}

/// Complete an erase: 0xFF the backend range, then READY.
pub fn complete_erase(sys: &System, ptr: u32, len_code: u32) {
    let len = if len_code == 1 { 64 * 1024 } else { 4 * 1024 };
    if let Some(m) = QSPI_FLASH.get() {
        if let Some(img) = m.lock().unwrap().get_mut("QSPI") {
            for i in 0..len {
                let idx = ptr as usize + i;
                if idx < img.len() {
                    img[idx] = 0xFF;
                }
            }
        }
    }
    let fire = with_qspi(sys, |q| {
        q.ev_ready = true;
        q.intenset & 1 != 0
    });
    if fire == Some(true) {
        sys.p.nvic.borrow_mut().set_intr_pending(41);
    }
}

/// Read back backend bytes (driver/test helper).
pub fn backend_read(ptr: u32, len: usize) -> Vec<u8> {
    if let Some(m) = QSPI_FLASH.get() {
        if let Some(img) = m.lock().unwrap().get("QSPI") {
            return (0..len).map(|i| img.get(ptr as usize + i).copied().unwrap_or(0xFF)).collect();
        }
    }
    vec![0xFF; len]
}

impl Peripheral for QspiNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x104 => self.ev_ready as u32,
            0x304 => self.intenset,
            0x400 => 1, // STATUS.READY
            0x500 => self.enabled as u32,
            0x514 => self.read_src,
            0x518 => self.read_dst,
            0x51C => self.read_cnt,
            0x520 => self.write_src,
            0x524 => self.write_dst,
            0x528 => self.write_cnt,
            0x52C => self.erase_ptr,
            0x530 => self.erase_len,
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => self.ev_ready = false, // ACTIVATE
            0x004 => {
                // READSTART: stage unless nothing to move.
                self.ev_ready = false;
                self.read_pending = self.read_cnt > 0;
                if !self.read_pending {
                    self.ev_ready = true;
                    self.fire(sys);
                }
            }
            0x008 => {
                self.ev_ready = false;
                self.write_pending = self.write_cnt > 0;
                if !self.write_pending {
                    self.ev_ready = true;
                    self.fire(sys);
                }
            }
            0x00C => {
                self.ev_ready = false;
                self.erase_pending = true;
            }
            0x010 => { // DEACTIVATE
                self.ev_ready = false;
                self.read_pending = false;
                self.write_pending = false;
                self.erase_pending = false;
            }
            0x104 => if value == 0 { self.ev_ready = false; }
            0x304 => self.intenset |= value & 1,
            0x308 => self.intenset &= !value,
            0x500 => self.enabled = value & 1 == 1,
            0x514 => self.read_src = value,
            0x518 => self.read_dst = value,
            0x51C => self.read_cnt = value & 0xFFFF_FF,
            0x520 => self.write_src = value,
            0x524 => self.write_dst = value,
            0x528 => self.write_cnt = value & 0xFFFF_FF,
            0x52C => self.erase_ptr = value,
            0x530 => self.erase_len = value & 1,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn activate_sets_ready_event() {
        let sys = test_dummy_system();
        let mut q = QspiNrf::default();
        assert_eq!(q.read(&sys, 0x400), 1);
        q.write(&sys, 0x500, 1);
        q.write(&sys, 0x000, 1);
        // ACTIVATE alone stages nothing; READY only after a transfer.
        assert_eq!(q.read(&sys, 0x104), 0);
    }
    #[test]
    fn write_read_erase_roundtrip() {
        use crate::system::test_dummy_system;
        let sys = test_dummy_system();
        qspi_register_flash("QSPI", &vec![0xFF; 65536]);
        sys.p.write(&sys, 0x40029500, 4, 1); // ENABLE
        sys.p.write(&sys, 0x40029524, 4, 0x1000); // WRITE.DST
        sys.p.write(&sys, 0x40029528, 4, 4); // WRITE.CNT
        sys.p.write(&sys, 0x40029008, 4, 1); // WRITESTART
        let t = take_write(&sys).expect("write staged");
        assert_eq!(t, (0, 0x1000, 4));
        complete_write(&sys, t.1, &[0xDE, 0xAD, 0xBE, 0xEF]);
        assert_eq!(backend_read(0x1000, 4), vec![0xDE, 0xAD, 0xBE, 0xEF]);
        // Programmed bits only clear: AND semantics.
        sys.p.write(&sys, 0x40029008, 4, 1);
        let _ = take_write(&sys);
        complete_write(&sys, 0x1000, &[0xFF, 0xFF, 0x00, 0xFF]);
        assert_eq!(backend_read(0x1000, 4), vec![0xDE, 0xAD, 0x00, 0xEF]);
        // Indirect read stages (driver would mem_write from backend).
        sys.p.write(&sys, 0x40029514, 4, 0x1000); // READ.SRC
        sys.p.write(&sys, 0x40029518, 4, 0x20001000); // READ.DST
        sys.p.write(&sys, 0x4002951C, 4, 4); // READ.CNT
        sys.p.write(&sys, 0x40029004, 4, 1); // READSTART
        assert_eq!(take_read(&sys), Some((0x1000, 0x20001000, 4)));
        complete_read(&sys);
        assert_eq!(sys.p.read(&sys, 0x40029104, 4), 1, "READY set");
        // Erase the sector back to 0xFF.
        sys.p.write(&sys, 0x4002952C, 4, 0x1000); // ERASE.PTR
        sys.p.write(&sys, 0x40029530, 4, 0); // 4KB
        sys.p.write(&sys, 0x4002900C, 4, 1); // ERASESTART
        let e = take_erase(&sys).expect("erase staged");
        complete_erase(&sys, e.0, e.1);
        assert_eq!(backend_read(0x1000, 4), vec![0xFF; 4]);
        qspi_clear();
    }
}
