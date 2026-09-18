use crate::system::System;
use super::Peripheral;

/// NVMC @ 0x4001E000 (flash controller). READY 0x400 (always 1),
/// READYNEXT 0x408 (always 1), CONFIG 0x504 (0=REN read-only, 1=WEN write
/// enable, 2=EEN erase enable), ERASEPAGE 0x508, ERASEALL 0x50C,
/// ERASEUICR 0x514.
/// Erase/write staging (driver owns the data path, like EASYDMA): with
/// CONFIG=EEN, an ERASEPAGE write stages take_erase() (page base); the
/// driver applies 0xFF to guest memory and calls complete_erase().
/// With CONFIG=WEN, flash-region writes via the NVMC window are accepted
/// by the memory layer (mem.rs flash protection stays: guest stores to
/// flash are still ignored natively; the JS driver applies program words
/// through mem_write, exactly like the original driver flow from the
/// early bring-up).
pub struct Nvmc {
    pub config: u32,
    erase_pending: Option<u32>,
}

impl Default for Nvmc {
    fn default() -> Self {
        Self { config: 0, erase_pending: None }
    }
}

impl Nvmc {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "NVMC" { Some(Box::new(Self::default())) } else { None }
    }
}

impl Peripheral for Nvmc {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x400 => 1, // READY
            0x408 => 1, // READYNEXT
            0x504 => self.config,
            0x508 => 0, // ERASEPAGE write-only
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, offset: u32, value: u32) {
        match offset {
            0x504 => self.config = value & 0x3,
            0x508 => {
                // ERASEPAGE is only meaningful with CONFIG=EEN.
                if self.config == 2 {
                    self.erase_pending = Some(value & !0xFFF);
                }
            }
            0x50C => {
                // ERASEALL with EEN: stage the whole flash (driver loops).
                if self.config == 2 {
                    self.erase_pending = Some(0xFFFF_FFFF);
                }
            }
            _ => {}
        }
    }
}

fn with_nvmc<R>(sys: &System, f: impl FnOnce(&mut Nvmc) -> R) -> Option<R> {
    for slot in &sys.p.peripherals {
        if slot.start == 0x4001_E000 {
            let mut b = slot.peripheral.borrow_mut();
            if let Some(n) = b.as_any_mut().downcast_mut::<Nvmc>() {
                return Some(f(n));
            }
            return None;
        }
    }
    None
}

/// Take a staged erase (page base, or 0xFFFF_FFFF for ERASEALL).
pub fn take_erase(sys: &System) -> Option<u32> {
    with_nvmc(sys, |n| n.erase_pending.take()).flatten()
}

/// Complete an erase: clears the staged request (driver applied 0xFF).
/// Posts the SoC flash-success event (sd_evt phase 1) when the SD is
/// enabled — same take→complete discipline as every DMA pump: post on
/// completion, never on staging.
pub fn complete_erase(sys: &System) {
    with_nvmc(sys, |n| n.erase_pending = None);
    crate::sd_evt::post_flash_op(true);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn ready_and_config() {
        let sys = test_dummy_system();
        let mut n = Nvmc::default();
        assert_eq!(n.read(&sys, 0x400), 1);
        n.write(&sys, 0x504, 1);
        assert_eq!(n.read(&sys, 0x504), 1);
    }
    #[test]
    fn erase_stages_only_with_een() {
        use crate::system::test_dummy_system;
        let sys = test_dummy_system();
        // REN (0): erase ignored.
        sys.p.write(&sys, 0x4001E508, 4, 0x0007E000);
        assert!(take_erase(&sys).is_none());
        // EEN (2): erase stages the 4KB-aligned page base.
        sys.p.write(&sys, 0x4001E504, 4, 2);
        sys.p.write(&sys, 0x4001E508, 4, 0x0007E123);
        assert_eq!(take_erase(&sys), Some(0x0007E000));
        assert!(take_erase(&sys).is_none(), "staged once only");
        // Second run: fresh instance, no leak.
        let sys2 = test_dummy_system();
        assert!(take_erase(&sys2).is_none());
    }
}
