use crate::system::System;
use super::Peripheral;

/// FICR (0x10000000, read-only factory info) + UICR (0x10001000, customer NVM).
/// Boot code (CODAL/Zephyr/MicroPython) reads DEVICEID + INFO.PART/RAM/FLASH
/// very early — without this, boot hangs. FICR = constants, UICR = storage.
pub struct FicrUicr {
    is_ficr: bool,
    /// UICR backing store (erased = 0xFFFFFFFF). 0x1000 bytes.
    uicr: Vec<u32>,
}

impl FicrUicr {
    pub fn new_ficr(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "FICR" {
            Some(Box::new(Self { is_ficr: true, uicr: Vec::new() }))
        } else {
            None
        }
    }
    pub fn new_uicr(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "UICR" {
            Some(Box::new(Self { is_ficr: false, uicr: vec![0xFFFF_FFFF; 0x400] }))
        } else {
            None
        }
    }

    fn ficr_read(offset: u32) -> u32 {
        match offset {
            0x010 => 0x1000,          // CODEPAGESIZE = 4KB
            0x014 => 128,             // CODESIZE = 128 pages x 4KB = 512KB
            0x060 => 0x1234_5678,     // DEVICEID[0]
            0x064 => 0x9ABC_DEF0,     // DEVICEID[1]
            0x100 => 0x0005_2833,     // INFO.PART = nRF52833
            0x104 => 0x4141_4141,     // INFO.VARIANT = AAAA
            0x108 => 0x0000_2004,     // INFO.PACKAGE = QIxx
            0x10C => 0x0000_0080,     // INFO.RAM = K128
            0x110 => 0x0000_0200,     // INFO.FLASH = K512
            _ => 0,
        }
    }
}

impl Peripheral for FicrUicr {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        if self.is_ficr {
            Self::ficr_read(offset)
        } else {
            let idx = (offset >> 2) as usize;
            self.uicr.get(idx).copied().unwrap_or(0)
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        // FICR is read-only: ignore. UICR is flash-backed: model as RAM store.
        if !self.is_ficr {
            let idx = (offset >> 2) as usize;
            if idx < self.uicr.len() {
                self.uicr[idx] = value;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn ficr_part_and_sizes() {
        let sys = test_dummy_system();
        let mut f = FicrUicr { is_ficr: true, uicr: Vec::new() };
        assert_eq!(f.read(&sys, 0x100), 0x0005_2833);
        assert_eq!(f.read(&sys, 0x010), 0x1000);
        assert_eq!(f.read(&sys, 0x014), 128);
        assert_eq!(f.read(&sys, 0x10C), 0x80);
        assert_eq!(f.read(&sys, 0x110), 0x200);
    }
    #[test]
    fn uicr_rw_and_second_run_clean() {
        let sys = test_dummy_system();
        let mut u = FicrUicr { is_ficr: false, uicr: vec![0xFFFF_FFFF; 0x400] };
        assert_eq!(u.read(&sys, 0x080), 0xFFFF_FFFF);
        u.write(&sys, 0x080, 0x1234);
        assert_eq!(u.read(&sys, 0x080), 0x1234);
        // 2nd run: fresh instance, no leak
        let mut u2 = FicrUicr { is_ficr: false, uicr: vec![0xFFFF_FFFF; 0x400] };
        assert_eq!(u2.read(&sys, 0x080), 0xFFFF_FFFF);
    }
}
