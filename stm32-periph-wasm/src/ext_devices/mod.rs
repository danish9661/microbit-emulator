pub mod spi_tap;
pub mod i2c_tap;

pub use spi_tap::SpiTap;
pub use i2c_tap::I2cTap;

use std::{rc::Rc, cell::RefCell};

pub struct SpiDeviceEntry {
    pub cs: Option<(u8, u8)>,
    pub device: Rc<RefCell<dyn ExtDevice<(), u8>>>,
    pub name: String,
}

#[derive(Clone)]
pub struct I2cDeviceEntry {
    pub address: u8,
    pub device: Rc<RefCell<dyn ExtDevice<(), u8>>>,
    pub name: String,
}

#[derive(Default)]
pub struct ExtDevices {
    /// Protocol-agnostic SPI bus taps (SPIM0-3 <-> JS sensor/display).
    pub spi_taps: Vec<Rc<RefCell<SpiTap>>>,
    /// Protocol-agnostic I2C slaves (TWIM0-1 <-> JS LSM303 etc).
    pub i2c_taps: Vec<Rc<RefCell<I2cTap>>>,
}

impl ExtDevices {
    pub fn find_serial_devices(&self, peri_name: &str) -> Vec<SpiDeviceEntry> {
        let mut result: Vec<SpiDeviceEntry> = Vec::new();
        for d in &self.spi_taps {
            if d.borrow().config.peripheral == peri_name {
                result.push(SpiDeviceEntry {
                    cs: d.borrow().config.cs.as_ref().map(|s| parse_pin(s)),
                    device: d.clone() as Rc<RefCell<dyn ExtDevice<(), u8>>>,
                    name: format!("{} spi-tap", peri_name),
                });
            }
        }
        result
    }

    pub fn find_serial_device(&self, peri_name: &str) -> Option<Rc<RefCell<dyn ExtDevice<(), u8>>>> {
        self.spi_taps.iter()
            .filter(|d| d.borrow().config.peripheral == peri_name)
            .next()
            .map(|d| d.clone() as Rc<RefCell<dyn ExtDevice<(), u8>>>)
    }

    pub fn find_i2c_devices(&self, peri_name: &str) -> Vec<I2cDeviceEntry> {
        let mut out: Vec<I2cDeviceEntry> = Vec::new();
        for d in &self.i2c_taps {
            if d.borrow().config.peripheral == peri_name {
                out.push(I2cDeviceEntry {
                    address: d.borrow().config.address,
                    device: d.clone() as Rc<RefCell<dyn ExtDevice<(), u8>>>,
                    name: format!("{} i2c-tap", peri_name),
                });
            }
        }
        out
    }
}

pub trait ExtDevice<A, T> {
    fn connect_peripheral(&mut self, peri_name: &str) -> String;
    fn read(&mut self, sys: &crate::system::System, addr: A) -> T;
    fn write(&mut self, sys: &crate::system::System, addr: A, v: T);
    fn reset(&mut self) {}
    fn cs_changed(&mut self, _sys: &crate::system::System, _asserted: bool) {}
}

pub fn parse_pin(s: &str) -> (u8, u8) {
    // nRF style "P0.12" / "P1.00" plus legacy "PA4".
    let s = s.to_ascii_uppercase();
    if let Some(dot) = s.find('.') {
        let port = s.as_bytes().get(1).copied().unwrap_or(b'0') - b'0';
        let pin: u8 = s[dot + 1..].parse().unwrap_or(0);
        (port.min(1), pin)
    } else {
        let b = s.as_bytes();
        if b.len() >= 3 {
            let port = (b[1] as char).to_ascii_uppercase() as u8 - b'A';
            let pin: u8 = s[2..].trim_start_matches('0').parse().unwrap_or(0);
            (port.min(1), pin)
        } else {
            (0, 0)
        }
    }
}

// SAFETY: WASM is single-threaded; Rc/RefCell are safe
unsafe impl Send for ExtDevices {}
unsafe impl Sync for ExtDevices {}
