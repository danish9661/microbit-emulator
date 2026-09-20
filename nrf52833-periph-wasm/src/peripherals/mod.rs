pub mod systick;
pub mod nvic;
pub mod scb;
pub mod fpu;
pub mod mpu;
pub mod dwt;
pub mod itm;
pub mod stir;
// Nordic nRF52833 (micro:bit v2.2 target)
pub mod ficr_uicr;
pub mod clock_nrf;
pub mod nvmc_nrf;
pub mod gpio_nrf;
pub mod uarte_nrf;
pub mod timer_nrf;
pub mod rtc_nrf;
pub mod twim_nrf;
pub mod gpiote_nrf;
pub mod ppi_nrf;
pub mod saadc_nrf;
pub mod temp_nrf;
pub mod rng_nrf;
pub mod pwm_nrf;
pub mod pdm_nrf;
pub mod qspi_nrf;
pub mod usbd_nrf;
pub mod radio_nrf;
pub mod wdt_nrf;
pub mod qdec_nrf;
pub mod comp_nrf;
pub mod nfct_nrf;
pub mod egu_nrf;
pub mod mwu_nrf;
pub mod misc_nrf;
pub mod fpu_engine_nrf;

use std::cell::RefCell;
use std::collections::HashMap;
use crate::system::System;
use crate::ext_devices::ExtDevices;
use fpu::Fpu;
use mpu::Mpu;
use dwt::{Dwt, Demcr};
use itm::Itm;
use stir::Stir;
use gpio_nrf::GpioPorts;
use svd_parser::svd::{MaybeArray, PeripheralInfo};

pub trait Peripheral: std::any::Any {
    fn read(&mut self, sys: &System, offset: u32) -> u32;
    fn write(&mut self, sys: &System, offset: u32, value: u32);
    fn tick(&mut self, _sys: &System) {}
    fn rx_byte(&mut self, _sys: &System, _byte: u8) {}
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

pub struct PeripheralSlot<T> {
    pub start: u32,
    pub end: u32,
    pub peripheral: T,
}

pub struct Peripherals {
    pub(crate) peripherals: Vec<PeripheralSlot<RefCell<Box<dyn Peripheral>>>>,
    pub nvic: RefCell<nvic::Nvic>,
    pub gpio: RefCell<GpioPorts>,
}

impl Peripherals {
    pub fn dwt_count_exc(&self, sys: &System) {
        if sys.p.read(sys, 0xE000EDFC, 4) & (1 << 24) == 0 {
            return;
        }
        for slot in &self.peripherals {
            if slot.start == 0xE000_1000 {
                use crate::peripherals::dwt::Dwt;
                if let Some(dwt) = slot.peripheral.borrow_mut().as_any_mut().downcast_mut::<Dwt>() {
                    dwt.count_exc();
                }
                break;
            }
        }
    }

    pub fn dwt_count_fold(&self, sys: &System) {
        if sys.p.read(sys, 0xE000EDFC, 4) & (1 << 24) == 0 {
            return;
        }
        for slot in &self.peripherals {
            if slot.start == 0xE000_1000 {
                use crate::peripherals::dwt::Dwt;
                if let Some(dwt) = slot.peripheral.borrow_mut().as_any_mut().downcast_mut::<Dwt>() {
                    dwt.count_fold();
                }
                break;
            }
        }
    }

    pub fn fpu_fpexc_en(&self) -> bool {
        for slot in &self.peripherals {
            if slot.start == 0xE000_EF34 {
                if let Some(fpu) = slot.peripheral.borrow_mut().as_any_mut().downcast_mut::<Fpu>() {
                    return fpu.fpexc_en();
                }
                break;
            }
        }
        true
    }

    pub fn set_fpu_fpexc_en(&self, v: bool) {
        for slot in &self.peripherals {
            if slot.start == 0xE000_EF34 {
                if let Some(fpu) = slot.peripheral.borrow_mut().as_any_mut().downcast_mut::<Fpu>() {
                    fpu.set_fpexc_en(v);
                }
                break;
            }
        }
    }

    pub fn mpu_check(&self, addr: u32, size: u32, write: bool, exec: bool) -> Option<bool> {
        if !crate::system::is_mpu_enabled() {
            return None;
        }
        let priv_ = crate::system::current_privileged();
        let hfnmi = crate::system::current_hfnmi();
        let priv_ = priv_ && !crate::system::mpu_force_unpriv();
        for slot in &self.peripherals {
            if slot.start == 0xE000_ED90 {
                // try_borrow_mut: the mem.rs unaligned_deny path calls
                // mpu_is_device() while a cpu write holds the MPU slot
                // (P108 SYS-swap family: RefCell already borrowed at
                // mod.rs:132). A failed borrow = treat as no-match
                // (the holder's check decides), never panic.
                let mut b = match slot.peripheral.try_borrow_mut() {
                    Ok(b) => b,
                    Err(_) => return None,
                };
                if let Some(mpu) = b.as_any_mut().downcast_mut::<Mpu>() {
                    return mpu.check_range(addr, size, write, exec, priv_, hfnmi);
                }
                break;
            }
        }
        None
    }

    pub fn mpu_is_device(&self, addr: u32) -> bool {
        if !crate::system::is_mpu_enabled() {
            return false;
        }
        for slot in &self.peripherals {
            if slot.start == 0xE000_ED90 {
                let mut b = match slot.peripheral.try_borrow_mut() {
                    Ok(b) => b,
                    Err(_) => return false,
                };
                if let Some(mpu) = b.as_any_mut().downcast_mut::<Mpu>() {
                    return mpu.is_device(addr);
                }
                break;
            }
        }
        false
    }
}

fn extract_svd_max_offset(p: &PeripheralInfo) -> u32 {
    let mut max_off = 0u32;
    use svd_parser::svd::register::{address_offsets as reg_offsets};
    use svd_parser::svd::array::{names as arr_names};
    use svd_parser::svd::cluster::{address_offsets as clus_offsets};
    for reg in p.registers() {
        match reg {
            MaybeArray::Single(r) => max_off = max_off.max(r.address_offset + 4),
            MaybeArray::Array(r, dim) => {
                for (off, _) in reg_offsets(r, dim).zip(arr_names(r, dim)) {
                    max_off = max_off.max(off + 4);
                }
            }
        }
    }
    for cluster in p.clusters() {
        match cluster {
            MaybeArray::Single(c) => {
                let base = c.address_offset;
                for reg in c.registers() {
                    match reg {
                        MaybeArray::Single(r) => max_off = max_off.max(base + r.address_offset + 4),
                        MaybeArray::Array(r, dim) => {
                            for (off, _) in reg_offsets(r, dim).zip(arr_names(r, dim)) {
                                max_off = max_off.max(base + off + 4);
                            }
                        }
                    }
                }
            }
            MaybeArray::Array(c, dim) => {
                for (clus_off, _) in clus_offsets(c, dim).zip(dim.indexes()) {
                    let base = c.address_offset + clus_off as u32;
                    for reg in c.registers() {
                        match reg {
                            MaybeArray::Single(r) => max_off = max_off.max(base + r.address_offset + 4),
                            MaybeArray::Array(r, d) => {
                                for (off, _) in reg_offsets(r, d).zip(arr_names(r, d)) {
                                    max_off = max_off.max(base + off + 4);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    max_off
}

fn make_nrf_peripheral(name: &str, _ext: &ExtDevices) -> Option<Box<dyn Peripheral>> {
    None
        .or_else(|| nvic::NvicWrapper::new(name))
        .or_else(|| SysTick::new(name))
        .or_else(|| Scb::new(name))
        .or_else(|| ficr_uicr::FicrUicr::new_ficr(name))
        .or_else(|| ficr_uicr::FicrUicr::new_uicr(name))
        .or_else(|| clock_nrf::ClockPower::new(name))
        .or_else(|| nvmc_nrf::Nvmc::new(name))
        .or_else(|| gpio_nrf::GpioNrf::new_p0(name))
        .or_else(|| gpio_nrf::GpioNrf::new_p1(name))
        .or_else(|| uarte_nrf::Uarte::new(name))
        .or_else(|| timer_nrf::TimerNrf::new(name))
        .or_else(|| rtc_nrf::RtcNrf::new(name))
        .or_else(|| twim_nrf::Twim::new(name))
        .or_else(|| gpiote_nrf::Gpiote::new(name))
        .or_else(|| ppi_nrf::Ppi::new(name))
        .or_else(|| saadc_nrf::Saadc::new(name))
        .or_else(|| temp_nrf::TempNrf::new(name))
        .or_else(|| rng_nrf::RngNrf::new(name))
        .or_else(|| pwm_nrf::PwmNrf::new(name))
        .or_else(|| pdm_nrf::PdmNrf::new(name))
        .or_else(|| qspi_nrf::QspiNrf::new(name))
        .or_else(|| usbd_nrf::UsbdNrf::new(name))
        .or_else(|| radio_nrf::RadioNrf::new(name))
        .or_else(|| wdt_nrf::WdtNrf::new(name))
        .or_else(|| qdec_nrf::QdecNrf::new(name))
        .or_else(|| comp_nrf::CompNrf::new(name))
        .or_else(|| nfct_nrf::NfctNrf::new(name))
        .or_else(|| egu_nrf::EguNrf::new(name))
        .or_else(|| mwu_nrf::MwuNrf::new(name))
        .or_else(|| misc_nrf::EcbNrf::new(name))
        .or_else(|| misc_nrf::AarCcmNrf::new(name))
        .or_else(|| misc_nrf::I2sNrf::new(name))
        .or_else(|| fpu_engine_nrf::FpuEngineNrf::new(name))
        .or_else(|| Mpu::new(name))
        .or_else(|| Fpu::new(name))
        .or_else(|| Dwt::new(name))
        .or_else(|| Demcr::new(name))
        .or_else(|| Stir::new(name))
}

impl Peripherals {
    pub const NVIC_REGS_BASE: u32 = 0xE000_E100;
    pub const NVIC_REGS_END: u32 = 0xE000_E500;

    pub const MEMORY_MAPS: [(u32, u32); 3] = [
        (0x0000_0000, 0x1000_0000),
        (0x4000_0000, 0x5100_0000),
        (0xE000_0000, 0xE100_0000),
    ];

    pub fn from_svd(svd_xml: &str, gpio: GpioPorts, ext_devices: &ExtDevices) -> Self {
        let mut device: svd_parser::svd::Device = svd_parser::parse(svd_xml)
            .expect("Failed to parse SVD XML");
        device.peripherals.sort_by_key(|p| p.base_address);
        let mut peripherals = Peripherals {
            peripherals: Vec::new(),
            nvic: RefCell::new(nvic::Nvic::default()),
            gpio: RefCell::new(gpio),
        };
        let svd_map: HashMap<&str, &PeripheralInfo> = device.peripherals.iter()
            .filter_map(|p| match p {
                MaybeArray::Single(p) => Some((p.name.as_str(), p)),
                MaybeArray::Array(_, _) => None,
            })
            .collect();
        for p in &device.peripherals {
            let p = match p {
                MaybeArray::Single(p) => p,
                MaybeArray::Array(_, _) => continue,
            };
            let resolved = p.derived_from.as_ref()
                .and_then(|d| svd_map.get(d.as_str()).copied())
                .unwrap_or(p);
            let name = &p.name;
            let size = extract_svd_max_offset(resolved).max(0x10).min(0x1000);
            // nRF FPU engine (@0x40026000) is not the ARM core FPU: it is
            // its own minimal model (UNUSED reads 0, writes ignored) at
            // the SVD base. The explicit ARM slot below owns the "FPU"
            // system registers (FPCCR/FPCAR/MVFR at 0xE000EF34).
            let (start, end, name_eff) = if name.as_str() == "FPU" {
                (0x4002_6000, 0x4002_7000, "FPUENGINE")
            } else if name.as_str() == "P0" {
                // P0/P1 blocks overlap in the SVD (shared GPIO register
                // file): one combined slot, same as new_wasm (see gpio_nrf).
                (0x5000_0000, 0x5000_0C00, "GPIO")
            } else if name.as_str() == "P1" {
                continue;
            } else if name.as_str() == "FPU_CPACR" {
                (0xE000_ED88, 0xE000_ED8C, "FPU_CPACR")
            } else if name.as_str() == "POWER" {
                // CLOCK+POWER share 0x40000000: merge into one slot.
                (0x4000_0000, 0x4000_1000, "CLOCK")
            } else {
                (p.base_address as u32, p.base_address as u32 + size, name.as_str())
            };
            if let Some(peri) = make_nrf_peripheral(name_eff, ext_devices) {
                // avoid double-register on shared-base aliases
                if peripherals.peripherals.iter().any(|s| s.start == start) {
                    continue;
                }
                peripherals.peripherals.push(PeripheralSlot {
                    start, end,
                    peripheral: RefCell::new(peri),
                });
            }
        }
        // Explicit ARM core slots (absent from the SVD address space).
        if let Some(p) = Fpu::new("FPU") {
            peripherals.peripherals.push(PeripheralSlot {
                start: 0xE000_EF34, end: 0xE000_EF34 + 0x18, peripheral: RefCell::new(p),
            });
        }
        // Explicit slots absent from SVD (same precedent as old QSPI/DWT).
        if let Some(p) = Dwt::new("DWT") {
            peripherals.peripherals.push(PeripheralSlot {
                start: 0xE000_1000, end: 0xE000_1020, peripheral: RefCell::new(p),
            });
        }
        if let Some(p) = Demcr::new("DEMCR") {
            peripherals.peripherals.push(PeripheralSlot {
                start: 0xE000_EDFC, end: 0xE000_EE00, peripheral: RefCell::new(p),
            });
        }
        if let Some(p) = Itm::new("ITM") {
            peripherals.peripherals.push(PeripheralSlot {
                start: 0xE000_0000, end: 0xE000_0F00, peripheral: RefCell::new(p),
            });
        }
        if let Some(p) = Stir::new("STIR") {
            peripherals.peripherals.push(PeripheralSlot {
                start: 0xE000_EF00, end: 0xE000_EF04, peripheral: RefCell::new(p),
            });
        }
        peripherals.finish_registration();
        peripherals
    }

    pub fn new_wasm(gpio: GpioPorts, ext_devices: &ExtDevices) -> Self {
        let mut peripherals = Peripherals {
            peripherals: Vec::new(),
            nvic: RefCell::new(nvic::Nvic::default()),
            gpio: RefCell::new(gpio),
        };
        // nRF52833 map (Product Specification). CLOCK+POWER share 0x40000000.
        let regs: Vec<(u32, u32, &str)> = vec![
            (0x4000_0000, 0x4000_1000, "CLOCK"),
            (0x4000_1000, 0x4000_2000, "RADIO"),
            (0x4000_2000, 0x4000_3000, "UARTE0"),
            (0x4000_3000, 0x4000_4000, "TWIM0"),
            (0x4000_4000, 0x4000_5000, "TWIM1"),
            (0x4000_5000, 0x4000_6000, "NFCT"),
            (0x4000_6000, 0x4000_7000, "GPIOTE"),
            (0x4000_7000, 0x4000_8000, "SAADC"),
            (0x4000_8000, 0x4000_9000, "TIMER0"),
            (0x4000_9000, 0x4000_A000, "TIMER1"),
            (0x4000_A000, 0x4000_B000, "TIMER2"),
            (0x4000_B000, 0x4000_C000, "RTC0"),
            (0x4000_C000, 0x4000_D000, "TEMP"),
            (0x4000_D000, 0x4000_E000, "RNG"),
            (0x4000_E000, 0x4000_F000, "ECB"),
            (0x4000_F000, 0x4001_0000, "AAR"),
            (0x4001_0000, 0x4001_1000, "WDT"),
            (0x4001_1000, 0x4001_2000, "RTC1"),
            (0x4001_2000, 0x4001_3000, "QDEC"),
            (0x4001_3000, 0x4001_4000, "COMP"),
            (0x4001_4000, 0x4001_5000, "EGU0"),
            (0x4001_5000, 0x4001_6000, "EGU1"),
            (0x4001_6000, 0x4001_7000, "EGU2"),
            (0x4001_7000, 0x4001_8000, "EGU3"),
            (0x4001_8000, 0x4001_9000, "EGU4"),
            (0x4001_9000, 0x4001_A000, "EGU5"),
            (0x4001_A000, 0x4001_B000, "TIMER3"),
            (0x4001_B000, 0x4001_C000, "TIMER4"),
            (0x4001_C000, 0x4001_D000, "PWM0"),
            (0x4001_D000, 0x4001_E000, "PDM"),
            (0x4001_E000, 0x4001_F000, "NVMC"),
            (0x4001_F000, 0x4002_0000, "PPI"),
            (0x4002_0000, 0x4002_1000, "MWU"),
            (0x4002_1000, 0x4002_2000, "PWM1"),
            (0x4002_2000, 0x4002_3000, "PWM2"),
            (0x4002_3000, 0x4002_4000, "SPIM2"),
            (0x4002_4000, 0x4002_5000, "RTC2"),
            (0x4002_5000, 0x4002_6000, "I2S"),
            (0x4002_6000, 0x4002_7000, "FPUENGINE"),
            (0x4002_7000, 0x4002_8000, "USBD"),
            (0x4002_8000, 0x4002_9000, "UARTE1"),
            (0x4002_9000, 0x4002_A000, "QSPI"),
            (0x4002_D000, 0x4002_E000, "PWM3"),
            (0x4002_F000, 0x4003_0000, "SPIM3"),
            (0x1000_0000, 0x1000_1000, "FICR"),
            (0x1000_1000, 0x1000_2000, "UICR"),
            (0x5000_0000, 0x5000_0C00, "GPIO"),
            (0xE000_1000, 0xE000_1020, "DWT"),
            (0xE000_E000, 0xE000_E010, "NVIC"),
            (0xE000_E010, 0xE000_E100, "SysTick"),
            (0xE000_ED00, 0xE000_ED90, "SCB"),
            (0xE000_ED90, 0xE000_EDFC, "MPU"),
            (0xE000_EDFC, 0xE000_EE00, "DEMCR"),
            (0xE000_EF00, 0xE000_EF04, "STIR"),
            (0xE000_EF34, 0xE000_EF4C, "FPU"),
        ];
        for (start, end, name) in regs {
            if let Some(p) = make_nrf_peripheral(name, ext_devices) {
                peripherals.peripherals.push(PeripheralSlot { start, end, peripheral: RefCell::new(p) });
            }
        }
        if let Some(p) = Itm::new("ITM") {
            peripherals.peripherals.push(PeripheralSlot { start: 0xE000_0000, end: 0xE000_0F00, peripheral: RefCell::new(p) });
        }
        peripherals.finish_registration();
        peripherals
    }

    fn finish_registration(&mut self) {
        self.peripherals.sort_by_key(|p| p.start);
        let a = self.peripherals.iter();
        let mut b = self.peripherals.iter();
        b.next();
        for (p1, p2) in a.zip(b) {
            assert!(p1.end <= p2.start, "Overlap: 0x{:08x}-0x{:08x} vs 0x{:08x}-0x{:08x}",
                p1.start, p1.end, p2.start, p2.end);
        }
    }

    fn get_peripheral<T>(slots: &[PeripheralSlot<T>], addr: u32) -> Option<&PeripheralSlot<T>> {
        let index = slots.binary_search_by_key(&addr, |p| p.start)
            .map_or_else(|e| e.checked_sub(1), |v| Some(v));
        index.map(|i| slots.get(i).filter(|p| addr <= p.end)).flatten()
    }

    fn bitbanding(addr: u32) -> Option<(u32, u8)> {
        if (0x4200_0000..0x4400_0000).contains(&addr) {
            let bit_number = (addr % 32) / 4;
            let mapped = 0x4000_0000 + (addr - 0x4200_0000) / 32;
            Some((mapped, bit_number as u8))
        } else { None }
    }

    fn align_addr_4(addr: u32) -> (u32, u8) {
        let byte_offset = (addr % 4) as u8;
        (addr - byte_offset as u32, byte_offset)
    }

    pub fn read(&self, sys: &System, addr: u32, _size: u8) -> u32 {
        if let Some((addr, bit_number)) = Self::bitbanding(addr) {
            return (self.read(sys, addr, 1) >> bit_number) & 1;
        }
        let (addr, byte_offset) = Self::align_addr_4(addr);
        // try_borrow_mut throughout this arm (P108 SYS-swap family):
        // mem.rs read paths re-enter the same slot (e.g. MPU-region
        // or MWU-watch reads while a model read holds it). A failed
        // borrow reads 0 instead of panicking — same discipline as
        // write() above and mpu_check/mpu_is_device.
        let value = if Self::NVIC_REGS_BASE <= addr && addr < Self::NVIC_REGS_END {
            match self.nvic.try_borrow_mut() {
                Ok(mut n) => n.read(sys, addr - Self::NVIC_REGS_BASE),
                Err(_) => 0,
            }
        } else if let Some(p) = Self::get_peripheral(&self.peripherals, addr) {
            match p.peripheral.try_borrow_mut() {
                Ok(mut b) => b.read(sys, addr - p.start),
                Err(_) => 0,
            }
        } else { 0 };
        // Shift DOWN: callers truncate to their width (mem.read8 takes the
        // low byte). Shifting up here zeroed every sub-word read past
        // offset 0 (e.g. the bootloader's ldrb of NVIC IPR22), which
        // reset-looped MicroPython (see docs/cpu_bug.md §3).
        value >> (8 * byte_offset)
    }

    pub fn write(&self, sys: &System, addr: u32, _size: u8, mut value: u32) {
        if let Some((addr, bit_number)) = Self::bitbanding(addr) {
            let mut v = self.read(sys, addr, 1);
            v &= !(1 << bit_number);
            v |= (value & 1) << bit_number;
            return self.write(sys, addr, 1, v);
        }
        let (addr, byte_offset) = Self::align_addr_4(addr);
        if byte_offset != 0 {
            let v = self.read(sys, addr, 4);
            value = (value << 8 * byte_offset) | (v & (0xFFFF_FFFF >> (32 - 8 * byte_offset)));
        }
        if Self::NVIC_REGS_BASE <= addr && addr < Self::NVIC_REGS_END {
            self.nvic.borrow_mut().write(sys, addr - Self::NVIC_REGS_BASE, value);
        } else if let Some(p) = Self::get_peripheral(&self.peripherals, addr) {
            // try_borrow_mut: a peripheral write can re-enter this same
            // slot (e.g. SCB AIRCR write -> reset path -> model write
            // to the same peripheral; P108 SYS-swap family). A failed
            // borrow drops the re-entrant write instead of panicking
            // (same discipline as mpu_check/mpu_is_device above).
            if let Ok(mut b) = p.peripheral.try_borrow_mut() {
                b.write(sys, addr - p.start, value);
            }
        }
    }

    pub fn rx_byte(&self, sys: &System, addr: u32, byte: u8) -> bool {
        if let Some(p) = Self::get_peripheral(&self.peripherals, addr) {
            p.peripheral.borrow_mut().rx_byte(sys, byte);
            true
        } else { false }
    }

    pub fn addr_desc(&self, addr: u32) -> String {
        format!("addr=0x{:08x}", addr)
    }
}

#[cfg(test)]
mod svd_tests {
    use super::*;

    static NRF_SVD: &str = include_str!("../../../monox/nrf52833.svd");

    fn slot_at(p: &Peripherals, addr: u32) -> bool {
        p.peripherals.iter().any(|s| s.start == addr)
    }

    #[test]
    fn svd_builds_without_overlap_and_matches_bases() {
        let p = Peripherals::from_svd(NRF_SVD, GpioPorts::default(), &ExtDevices::default());
        // Spot-check SVD ground-truth bases landed as slots.
        for (base, why) in [
            (0x1000_0000, "FICR"), (0x1000_1000, "UICR"),
            (0x4000_0000, "CLOCK/POWER"), (0x4000_1000, "RADIO"),
            (0x4000_2000, "UARTE0"), (0x4000_3000, "SERIAL0"),
            (0x4000_4000, "SERIAL1"), (0x4000_6000, "GPIOTE"),
            (0x4000_7000, "SAADC"), (0x4000_8000, "TIMER0"),
            (0x4000_B000, "RTC0"), (0x4000_C000, "TEMP"),
            (0x4000_D000, "RNG"), (0x4001_E000, "NVMC"),
            (0x4002_7000, "USBD"), (0x5000_0000, "P0"),
        ] {
            assert!(slot_at(&p, base), "missing slot {base:#010x} ({why})");
        }
        // No duplicate starts (shared-base aliases merged).
        let mut starts: Vec<u32> = p.peripherals.iter().map(|s| s.start).collect();
        starts.sort();
        let ndedup = starts.len();
        starts.dedup();
        assert_eq!(ndedup, starts.len(), "duplicate slot starts");
        // FICR PART readable through the SVD-built map too.
        assert_eq!(p.peripherals.iter().filter(|s| s.start == 0x1000_0000).count(), 1);
    }

    #[test]
    fn subword_reads_shift_down() {
        // Peripheral byte/halfword reads at nonzero offsets must return
        // the addressed lanes (callers truncate to width). Shifting up
        // zeroed them all: the bootloader's ldrb of NVIC IPR22 read 0
        // and reset-looped MicroPython (docs/cpu_bug.md §3).
        let sys = crate::system::test_dummy_system();
        sys.p.write(&sys, 0xE000E400, 4, 0x04030201);
        // (Peripherals::read returns the lane-shifted word; callers like
        // mem.read8 truncate to width -- mask here like they do.)
        assert_eq!(sys.p.read(&sys, 0xE000E400, 1) & 0xFF, 0x01);
        assert_eq!(sys.p.read(&sys, 0xE000E401, 1) & 0xFF, 0x02);
        assert_eq!(sys.p.read(&sys, 0xE000E402, 1) & 0xFF, 0x03);
        assert_eq!(sys.p.read(&sys, 0xE000E403, 1) & 0xFF, 0x04);
        assert_eq!(sys.p.read(&sys, 0xE000E402, 2) & 0xFFFF, 0x0403);
        assert_eq!(sys.p.read(&sys, 0xE000E400, 4), 0x04030201);
    }
    #[test]
    fn hardcoded_map_matches_svd_bases() {
        // Every new_wasm slot start must equal some SVD peripheral base
        // (guards against hand-typed address typos like the PWM0 one).
        let xml_bases: std::collections::HashSet<u32> = {
            let dev: svd_parser::svd::Device = svd_parser::parse(NRF_SVD).unwrap();
            dev.peripherals.into_iter().filter_map(|p| match p {
                MaybeArray::Single(p) => Some(p.base_address as u32),
                MaybeArray::Array(_, _) => None,
            }).collect()
        };
        let p = Peripherals::new_wasm(GpioPorts::default(), &ExtDevices::default());
        for s in &p.peripherals {
            if s.start >= 0xE000_0000 {
                continue; // ARM core slots (DWT/SCB/...) are not in the SVD
            }
            if s.start == 0x4002_9000 {
                continue; // QSPI absent from this SVD revision (see qspi_nrf)
            }
            assert!(xml_bases.contains(&s.start),
                "hardcoded slot {:#010x} not a SVD base", s.start);
        }
    }
}

use systick::SysTick;
use scb::Scb;
