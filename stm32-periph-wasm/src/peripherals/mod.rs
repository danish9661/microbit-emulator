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
    fn flash_erase_applied(&mut self) {}
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
    pub fn flash_erase_applied(&self) {}
    pub fn pwr_wakeup(&self) {}

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
                if let Some(mpu) = slot.peripheral.borrow_mut().as_any_mut().downcast_mut::<Mpu>() {
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
                if let Some(mpu) = slot.peripheral.borrow_mut().as_any_mut().downcast_mut::<Mpu>() {
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
            let (start, end) = if name.as_str() == "FPU" {
                (0xE000_EF34, 0xE000_EF34 + 0x18)
            } else if name.as_str() == "FPU_CPACR" {
                (0xE000_ED88, 0xE000_ED8C)
            } else {
                (p.base_address as u32, p.base_address as u32 + size)
            };
            // CLOCK+POWER share 0x40000000 region: merge into one slot to
            // avoid overlap assert (see new_wasm for the hardcoded twin).
            let name_eff = if name.as_str() == "POWER" { "CLOCK" } else { name.as_str() };
            if name.as_str() == "POWER" && peripherals.peripherals.iter().any(|s| s.start == 0x4000_0000) {
                continue;
            }
            if let Some(peri) = make_nrf_peripheral(name_eff, ext_devices) {
                // avoid double-register when SVD lists both CLOCK and POWER
                if peripherals.peripherals.iter().any(|s| s.start == start) {
                    continue;
                }
                peripherals.peripherals.push(PeripheralSlot {
                    start, end,
                    peripheral: RefCell::new(peri),
                });
            }
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
            (0x4000_6000, 0x4000_7000, "GPIOTE"),
            (0x4000_7000, 0x4000_8000, "SAADC"),
            (0x4000_8000, 0x4000_9000, "TIMER0"),
            (0x4000_9000, 0x4000_A000, "TIMER1"),
            (0x4000_A000, 0x4000_B000, "TIMER2"),
            (0x4000_B000, 0x4000_C000, "RTC0"),
            (0x4000_C000, 0x4000_D000, "TEMP"),
            (0x4000_D000, 0x4000_E000, "RNG"),
            (0x4001_1000, 0x4001_2000, "RTC1"),
            (0x4001_F000, 0x4002_0000, "PPI"),
            (0x4002_1000, 0x4002_2000, "PWM0"),
            (0x4001_E000, 0x4001_F000, "NVMC"),
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

    pub fn read(&self, sys: &System, addr: u32, size: u8) -> u32 {
        if let Some((addr, bit_number)) = Self::bitbanding(addr) {
            return (self.read(sys, addr, 1) >> bit_number) & 1;
        }
        let (addr, byte_offset) = Self::align_addr_4(addr);
        let value = if Self::NVIC_REGS_BASE <= addr && addr < Self::NVIC_REGS_END {
            self.nvic.borrow_mut().read(sys, addr - Self::NVIC_REGS_BASE)
        } else if let Some(p) = Self::get_peripheral(&self.peripherals, addr) {
            p.peripheral.borrow_mut().read(sys, addr - p.start)
        } else { 0 };
        value << (8 * byte_offset)
    }

    pub fn write(&self, sys: &System, addr: u32, size: u8, mut value: u32) {
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
            p.peripheral.borrow_mut().write(sys, addr - p.start, value);
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

use systick::SysTick;
use scb::Scb;
