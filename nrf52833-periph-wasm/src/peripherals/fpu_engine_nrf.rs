use crate::system::System;
use super::Peripheral;

/// nRF FPU *engine* @ 0x40026000 (IRQ 38, SVD ground truth) — NOT the
/// ARM core FPU (0xE000EF34, `fpu.rs`, CPACR-gated S0-S31 + lazy
/// stacking). The nRF engine exposes a single read-only UNUSED word at
/// 0x000 (reset 0); the Product Specification documents no tasks,
/// events, or INTEN here, and no driver touches it. Model: UNUSED
/// reads 0, all writes ignored, unlisted offsets read-as-0 (Nordic
/// style — HALs probe reserved). IRQ 38 never pends (no event source).
pub struct FpuEngineNrf;

impl Default for FpuEngineNrf {
    fn default() -> Self {
        Self
    }
}

impl FpuEngineNrf {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        if name == "FPUENGINE" { Some(Box::new(Self::default())) } else { None }
    }
}

impl Peripheral for FpuEngineNrf {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x000 => 0, // UNUSED (read-only, reset 0)
            _ => 0,
        }
    }
    fn write(&mut self, _sys: &System, _offset: u32, _value: u32) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn unused_reads_zero_writes_ignored() {
        let sys = test_dummy_system();
        // Live map: UNUSED + unlisted offsets read 0, writes ignored.
        assert_eq!(sys.p.read(&sys, 0x4002_6000, 4), 0, "UNUSED");
        sys.p.write(&sys, 0x4002_6000, 4, 0xFFFF_FFFF);
        assert_eq!(sys.p.read(&sys, 0x4002_6000, 4), 0, "write ignored");
        assert_eq!(sys.p.read(&sys, 0x4002_6100, 4), 0, "unlisted reads 0");
        // 2nd run: fresh instance, no leak.
        let sys2 = test_dummy_system();
        assert_eq!(sys2.p.read(&sys2, 0x4002_6000, 4), 0);
    }
    #[test]
    fn engine_slot_in_both_maps() {
        use crate::ext_devices::ExtDevices;
        use crate::peripherals::{GpioPorts, Peripherals};
        // new_wasm (live) map.
        let live = Peripherals::new_wasm(GpioPorts::default(), &ExtDevices::default());
        assert!(live.peripherals.iter().any(|s| s.start == 0x4002_6000), "live map slot");
        // from_svd map (SVD ground truth: FPU @ 0x40026000).
        static SVD: &str = include_str!("../../../monox/nrf52833.svd");
        let svd = Peripherals::from_svd(SVD, GpioPorts::default(), &ExtDevices::default());
        assert!(svd.peripherals.iter().any(|s| s.start == 0x4002_6000), "svd map slot");
    }
}
