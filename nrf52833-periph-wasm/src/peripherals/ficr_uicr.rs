use crate::system::System;
use super::Peripheral;

/// FICR (0x10000000, read-only factory info) + UICR (0x10001000, customer NVM).
/// Boot code (CODAL/Zephyr/MicroPython) reads DEVICEID + INFO.PART/RAM/FLASH
/// very early — without this, boot hangs. FICR = constants (full SVD face:
/// CODEPAGESIZE/CODESIZE, DEVICEID, ER/IR roots, DEVICEADDR/TYPE, INFO,
/// PRODTEST, TEMP trim, NFC TAGHEADERs). UICR = storage (erased
/// 0xFFFFFFFF, like silicon): NRFFW/NRFHW/CUSTOMER/PSELRESET/APPROTECT/
/// NFCPINS/DEBUGCTRL/REGOUT0 all live in the flat store at their SVD
/// offsets, so seeds and firmware writes land exactly where the NFCPINS
/// gate and the bootloader-address reads look.
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
            // ER/IR (encryption/identity roots, 0x080-0x09C): readable
            // device-unique secrets on silicon; the model reports fixed
            // example values (documented — never real key material).
            0x080 => 0xA5A5_A5A5,     // ER[0]
            0x084 => 0x5A5A_5A5A,     // ER[1]
            0x088 => 0x3C3C_3C3C,     // ER[2]
            0x08C => 0xC3C3_C3C3,     // ER[3]
            0x090 => 0x1111_2222,     // IR[0]
            0x094 => 0x3333_4444,     // IR[1]
            0x098 => 0x5555_6666,     // IR[2]
            0x09C => 0x7777_8888,     // IR[3]
            0x0A0 => 0x0000_0000,     // DEVICEADDRTYPE (public address)
            0x0A4 => 0xC9A8_4731,     // DEVICEADDR[0]
            0x0A8 => 0x5E4F_00C0,     // DEVICEADDR[1]
            0x100 => 0x0005_2833,     // INFO.PART = nRF52833
            0x104 => 0x4141_4141,     // INFO.VARIANT = AAAA
            0x108 => 0x0000_2004,     // INFO.PACKAGE = QIxx
            0x10C => 0x0000_0080,     // INFO.RAM = K128
            0x110 => 0x0000_0200,     // INFO.FLASH = K512
            0x350 | 0x354 | 0x358 => 0xFFFF_FFFF, // PRODTEST (SVD reset)
            // FICR TEMP trim cluster (0x404-0x474): same factory values
            // as the TEMP peripheral calibration block.
            0x404 => 0x00000326, // A0
            0x408 => 0x00000348, // A1
            0x40C => 0x000003AA, // A2
            0x410 => 0x0000040E, // A3
            0x414 => 0x000004BD, // A4
            0x418 => 0x000005A3, // A5
            0x41C => 0x00003FEF, // B0
            0x420 => 0x00003FBE, // B1
            0x424 => 0x00003FBE, // B2
            0x428 => 0x00000012, // B3
            0x42C => 0x00000124, // B4
            0x430 => 0x0000027C, // B5
            0x434 => 0x000000E2, // T0
            0x438 => 0x00000000, // T1
            0x43C => 0x00000019, // T2
            0x440 => 0x0000003C, // T3
            0x444 => 0x00000050, // T4
            // NFC tag headers (0x450-0x45C): chip-unique on silicon;
            // fixed example values here (documented, like ER/IR).
            0x450 => 0x00000059, // TAGHEADER0
            0x454 => 0x00000101, // TAGHEADER1
            0x458 => 0x0000AABB, // TAGHEADER2
            0x45C => 0x0000CCDD, // TAGHEADER3
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
    fn ficr_full_face_roots_addr_trim_tag() {
        // ER/IR roots, DEVICEADDR, PRODTEST reset, TEMP trim mirror,
        // TAGHEADERs (SVD ground truth); FICR ignores writes.
        let sys = test_dummy_system();
        let live = test_dummy_system();
        assert_eq!(live.p.read(&live, 0x10000080, 4), 0xA5A5_A5A5, "ER0");
        assert_eq!(live.p.read(&live, 0x1000009C, 4), 0x7777_8888, "IR3");
        assert_eq!(live.p.read(&live, 0x100000A0, 4), 0, "ADDRTYPE public");
        assert_eq!(live.p.read(&live, 0x100000A4, 4), 0xC9A8_4731, "DEVICEADDR0");
        assert_eq!(live.p.read(&live, 0x10000350, 4), 0xFFFF_FFFF, "PRODTEST reset");
        assert_eq!(live.p.read(&live, 0x10000404, 4), 0x326, "FICR A0 mirrors TEMP");
        assert_eq!(live.p.read(&live, 0x10000444, 4), 0x50, "FICR T4");
        assert_eq!(live.p.read(&live, 0x10000450, 4), 0x59, "TAGHEADER0");
        live.p.write(&live, 0x10000080, 4, 0xDEAD);
        assert_eq!(live.p.read(&live, 0x10000080, 4), 0xA5A5_A5A5, "FICR read-only");
        // UICR named regions land in the store (NFCPINS gate reads 0x20C).
        live.p.write(&live, 0x1000120C, 4, 0); // NFCPINS: GPIO mode
        assert_eq!(live.p.read(&live, 0x1000120C, 4), 0, "NFCPINS stored");
        live.p.write(&live, 0x10001208, 4, 0x5AFAAA); // APPROTECT
        assert_eq!(live.p.read(&live, 0x10001208, 4), 0x5AFAAA, "APPROTECT stored");
        let _ = sys;
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
