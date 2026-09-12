use std::sync::atomic::{AtomicPtr, Ordering};
use wasm_bindgen::prelude::*;

mod system;
pub mod peripherals;
pub mod ext_devices;
pub mod cpu;

use system::WasmSystem;

static SYS: AtomicPtr<WasmSystem> = AtomicPtr::new(std::ptr::null_mut());

pub(crate) fn sys() -> &'static WasmSystem {
    let p = SYS.load(Ordering::Acquire);
    assert!(!p.is_null(), "WasmSystem not initialized");
    // SAFETY: p came from Box::into_raw in set_sys and is never freed.
    unsafe { &*p }
}

/// Fallible system handle for hot paths (e.g. the MWU mem hook): unit
/// tests with dummy (non-installed) systems must not trap here.
pub(crate) fn try_sys() -> Option<&'static WasmSystem> {
    let p = SYS.load(Ordering::Acquire);
    if p.is_null() {
        None
    } else {
        // SAFETY: same contract as sys().
        Some(unsafe { &*p })
    }
}

fn set_sys(s: WasmSystem) {
    SYS.store(Box::into_raw(Box::new(s)), Ordering::Release);
}

#[cfg(test)]
pub(crate) fn init_svd_for_test(s: WasmSystem) {
    set_sys(s);
}

#[cfg(test)]
pub(crate) fn init_for_test(s: WasmSystem) {
    set_sys(s);
}

/// Initialize the emulator with the nRF52833 hardcoded peripheral map.
#[wasm_bindgen]
pub fn init() {
    console_error_panic_hook::set_once();
    set_sys(WasmSystem::new());
}

/// Initialize the emulator from an SVD XML string (e.g., nrf52833.svd).
#[wasm_bindgen]
pub fn init_svd(svd_xml: &str) {
    console_error_panic_hook::set_once();
    set_sys(WasmSystem::new_svd(svd_xml));
}

#[wasm_bindgen]
pub fn periph_read(addr: u32, width: u32) -> u32 {
    sys().p.read(&*sys(), addr, width as u8)
}

#[wasm_bindgen]
pub fn periph_write(addr: u32, width: u32, value: u32) {
    sys().p.write(&*sys(), addr, width as u8, value);
}

#[wasm_bindgen]
pub fn tick() {
    use std::sync::atomic::Ordering;
    system::INSTRUCTION_COUNT.fetch_add(1, Ordering::Relaxed);
    sys().tick();
}

#[wasm_bindgen]
pub fn tick_n(delta: u32) {
    use std::sync::atomic::Ordering;
    system::INSTRUCTION_COUNT.fetch_add(delta as u64, Ordering::Relaxed);
    sys().tick();
}

#[wasm_bindgen]
pub fn tick_peripherals() {
    sys().tick();
}

#[wasm_bindgen]
pub fn has_pending_interrupt() -> bool {
    sys().p.nvic.borrow().has_pending()
}

#[wasm_bindgen]
pub fn get_next_pending_interrupt() -> i32 {
    sys().p.nvic.borrow_mut().get_and_clear_next_intr_pending()
        .unwrap_or(-255)
}

#[wasm_bindgen]
pub fn set_intr_pending(irq: i32) {
    sys().p.nvic.borrow_mut().set_intr_pending(irq);
}

/// True when firmware requested a reboot (AIRCR SYSRESETREQ / WDT).
/// The JS driver must then reset the CPU from the vector table
/// (MicroPython does this twice during boot).
#[wasm_bindgen]
pub fn is_watchdog_reset_requested() -> bool {
    system::is_watchdog_reset_requested()
}

/// Clear all process-lifetime globals so a NEW emulator instance starts clean.
#[wasm_bindgen]
pub fn reset_state() {
    system::reset_globals();
}

// P0 = port 0 (32 pins), P1 = port 1 (10 pins on nRF52833).
#[wasm_bindgen]
pub fn gpio_read_output(port: u32, pin: u32) -> bool {
    sys().p.gpio.borrow().read_output_pin(port as u8, pin as u8)
}

/// Drive a raw input level. Buttons are active-low: released = true
/// (idle pull-up default), pressed = false. JS button layer maps to this.
#[wasm_bindgen]
pub fn gpio_set_input(port: u32, pin: u32, value: bool) {
    sys().p.gpio.borrow_mut().set_input_pin(port as u8, pin as u8, value);
}

#[wasm_bindgen]
pub fn gpio_read_input(port: u32, pin: u32) -> bool {
    sys().p.gpio.borrow().read_input_pin(port as u8, pin as u8)
}

/// Inject a received byte into the UARTE peripheral at the given base address.
#[wasm_bindgen]
pub fn uart_rx_byte(addr: u32, byte: u8) -> bool {
    sys().p.rx_byte(&*sys(), addr, byte)
}

/// Collect UART output since last call.
#[wasm_bindgen]
pub fn get_uart_output() -> String {
    use std::mem::take;
    take(&mut *system::get_uart_output().lock().unwrap())
}

// ── SPI bus taps (JS hardware layer: sensors/displays) ──
#[wasm_bindgen]
pub fn spi_tap(peripheral: &str, cs: Option<String>, dc: Option<String>) {
    use crate::ext_devices::spi_tap::{SpiTap, SpiTapConfig};
    let config = SpiTapConfig { peripheral: peripheral.to_string(), cs, dc };
    system::get_ext_devices().lock().unwrap().spi_taps
        .push(std::rc::Rc::new(std::cell::RefCell::new(SpiTap::new(config))));
}

#[wasm_bindgen]
pub fn spi_take_events(peripheral: &str) -> Vec<u32> {
    system::spi_tap_take_events(peripheral)
}

#[wasm_bindgen]
pub fn spi_push_miso(peripheral: &str, bytes: &[u8]) {
    system::spi_tap_miso_push(peripheral, bytes);
}

// ── EASYDMA driver API (JS owns the data path: take -> mem move -> complete)
#[wasm_bindgen]
pub fn uarte_take_txdma() -> Vec<u32> {
    crate::peripherals::uarte_nrf::take_txdma(sys()).map(|(p, n)| vec![p, n]).unwrap_or_default()
}

#[wasm_bindgen]
pub fn uarte_complete_txdma(bytes: &[u8]) {
    crate::peripherals::uarte_nrf::complete_txdma(sys(), bytes);
}

/// Take a staged UARTE RX transfer [ptr, maxcnt]; driver writes bytes to
/// guest RAM at ptr, then calls uarte_complete_rxdma(amount).
#[wasm_bindgen]
pub fn uarte_take_rxdma() -> Vec<u32> {
    crate::peripherals::uarte_nrf::take_rxdma(sys()).map(|(p, n)| vec![p, n]).unwrap_or_default()
}

#[wasm_bindgen]
pub fn uarte_complete_rxdma(amount: u32) {
    crate::peripherals::uarte_nrf::complete_rxdma(sys(), amount);
}

#[wasm_bindgen]
pub fn twim_take_txdma(peripheral: &str) -> Vec<u32> {
    crate::peripherals::twim_nrf::take_txdma(sys(), peripheral)
        .map(|(a, p, n)| vec![a as u32, p, n]).unwrap_or_default()
}

#[wasm_bindgen]
pub fn twim_complete_txdma(peripheral: &str, data: &[u8]) {
    crate::peripherals::twim_nrf::complete_txdma(sys(), peripheral, data);
}

#[wasm_bindgen]
pub fn twim_take_rxdma(peripheral: &str) -> Vec<u32> {
    crate::peripherals::twim_nrf::take_rxdma(sys(), peripheral)
        .map(|(a, p, n)| vec![a as u32, p, n]).unwrap_or_default()
}

#[wasm_bindgen]
pub fn twim_complete_rxdma(peripheral: &str, amount: u32) {
    crate::peripherals::twim_nrf::complete_rxdma(sys(), peripheral, amount);
}

#[wasm_bindgen]
pub fn saadc_take_result() -> Vec<u32> {
    crate::peripherals::saadc_nrf::take_result(sys()).map(|(p, n)| vec![p, n]).unwrap_or_default()
}

#[wasm_bindgen]
pub fn saadc_complete_result(amount: u32) {
    crate::peripherals::saadc_nrf::complete_result(sys(), amount);
}

#[wasm_bindgen]
pub fn pdm_take_sample() -> Vec<u32> {
    crate::peripherals::pdm_nrf::take_sample(sys()).map(|(p, n)| vec![p, n]).unwrap_or_default()
}

#[wasm_bindgen]
pub fn pdm_complete_sample() {
    crate::peripherals::pdm_nrf::complete_sample(sys());
}

// ── USBD host events + RADIO air ──
#[wasm_bindgen]
pub fn usbd_signal_reset() {
    crate::peripherals::usbd_nrf::signal_usbreset(sys());
}

#[wasm_bindgen]
pub fn radio_take_tx() -> Vec<u32> {
    crate::peripherals::radio_nrf::take_tx(sys()).map(|(p, n)| vec![p, n]).unwrap_or_default()
}

#[wasm_bindgen]
pub fn radio_complete_tx() {
    crate::peripherals::radio_nrf::complete_tx(sys());
}

#[wasm_bindgen]
pub fn radio_inject_rx(bytes: &[u8]) {
    crate::peripherals::radio_nrf::inject_rx(sys(), bytes.to_vec());
}

#[wasm_bindgen]
pub fn radio_inject_corrupt(bytes: &[u8]) {
    crate::peripherals::radio_nrf::inject_corrupt(sys(), bytes.to_vec());
}

#[wasm_bindgen]
pub fn radio_take_rx() -> Vec<u32> {
    crate::peripherals::radio_nrf::take_rx(sys()).map(|p| vec![p]).unwrap_or_default()
}

#[wasm_bindgen]
pub fn radio_complete_rx() {
    crate::peripherals::radio_nrf::complete_rx(sys());
}

#[wasm_bindgen]
pub fn radio_set_rssi_dbm(dbm: i32) {
    crate::peripherals::radio_nrf::set_rssi_dbm(sys(), dbm);
}

// ── I2S streaming driver API ──
#[wasm_bindgen]
pub fn i2s_take_rx() -> Vec<u32> {
    crate::peripherals::misc_nrf::take_i2s_rx(sys()).map(|(p, n)| vec![p, n]).unwrap_or_default()
}

#[wasm_bindgen]
pub fn i2s_complete_rx() {
    crate::peripherals::misc_nrf::complete_i2s_rx(sys());
}

#[wasm_bindgen]
pub fn i2s_take_tx() -> Vec<u32> {
    crate::peripherals::misc_nrf::take_i2s_tx(sys()).map(|(p, n)| vec![p, n]).unwrap_or_default()
}

#[wasm_bindgen]
pub fn i2s_complete_tx(bytes: &[u8]) {
    crate::peripherals::misc_nrf::complete_i2s_tx(sys(), bytes);
}

#[wasm_bindgen]
pub fn i2s_take_capture() -> Vec<u8> {
    system::i2s_take_capture()
}

// ── NFCT tag driver API ──
#[wasm_bindgen]
pub fn nfct_field_present(present: bool) {
    crate::peripherals::nfct_nrf::nfct_field_present(sys(), present);
}

#[wasm_bindgen]
pub fn nfct_take_tx() -> Vec<u32> {
    crate::peripherals::nfct_nrf::take_nfct_tx(sys()).map(|(p, n)| vec![p, n]).unwrap_or_default()
}

#[wasm_bindgen]
pub fn nfct_complete_tx() {
    crate::peripherals::nfct_nrf::complete_nfct_tx(sys());
}

#[wasm_bindgen]
pub fn nfct_take_rx() -> Vec<u32> {
    crate::peripherals::nfct_nrf::take_nfct_rx(sys()).map(|(p, n)| vec![p, n]).unwrap_or_default()
}

#[wasm_bindgen]
pub fn nfct_complete_rx(amount: u32) {
    crate::peripherals::nfct_nrf::complete_nfct_rx(sys(), amount);
}

// ── SAADC limit-monitor driver API ──
#[wasm_bindgen]
pub fn saadc_check_limits(ch: usize, value: i16) {
    crate::peripherals::saadc_nrf::check_limits(sys(), ch, value);
}

// ── TEMP thermometer driver API ──
#[wasm_bindgen]
pub fn temp_set_celsius(c: i32) {
    crate::peripherals::temp_nrf::temp_set_celsius(sys(), c);
}

// ── CCM crypt driver API (job words: cnf,in,out,scratch,len,decrypt) ──
#[wasm_bindgen]
pub fn ccm_take_job() -> Vec<u32> {
    crate::peripherals::misc_nrf::take_ccm(sys())
        .map(|j| vec![j.cnfptr, j.inptr, j.outptr, j.scratchptr, j.len, j.decrypt as u32])
        .unwrap_or_default()
}

#[wasm_bindgen]
pub fn ccm_complete(mic_ok: bool) {
    crate::peripherals::misc_nrf::complete_ccm(sys(), mic_ok);
}

// ── AAR host driver API (mirrors CCM; resolution runs driver-side) ──
#[wasm_bindgen]
pub fn aar_take_job() -> Vec<u32> {
    crate::peripherals::misc_nrf::take_aar(sys())
        .map(|(irkptr, addrptr)| vec![irkptr, addrptr])
        .unwrap_or_default()
}

#[wasm_bindgen]
pub fn aar_complete(resolved: bool) {
    crate::peripherals::misc_nrf::complete_aar(sys(), resolved);
}

// ── ECB host driver API (AES-128 block runs driver-side, FIPS-197 shape) ──
#[wasm_bindgen]
pub fn ecb_take_job() -> Vec<u32> {
    crate::peripherals::misc_nrf::take_ecb(sys())
        .map(|dataptr| vec![dataptr])
        .unwrap_or_default()
}

#[wasm_bindgen]
pub fn ecb_complete() {
    crate::peripherals::misc_nrf::complete_ecb(sys());
}

// ── COMP/QDEC host driver API ──
#[wasm_bindgen]
pub fn comp_set_input_mv(mv: u32) {
    crate::peripherals::comp_nrf::comp_set_input_mv(sys(), mv);
}

#[wasm_bindgen]
pub fn qdec_step(dir: i32) {
    crate::peripherals::qdec_nrf::qdec_step(sys(), dir);
}

// ── USBD endpoint driver API ──
#[wasm_bindgen]
pub fn usbd_take_epin() -> Vec<u32> {
    crate::peripherals::usbd_nrf::take_epin(sys())
        .map(|(e, p, n)| vec![e as u32, p, n]).unwrap_or_default()
}

#[wasm_bindgen]
pub fn usbd_complete_epin(ep: usize, bytes: &[u8]) {
    crate::peripherals::usbd_nrf::complete_epin(sys(), ep, bytes);
}

#[wasm_bindgen]
pub fn usbd_take_epout() -> Vec<u32> {
    crate::peripherals::usbd_nrf::take_epout(sys())
        .map(|(e, p, n)| vec![e as u32, p, n]).unwrap_or_default()
}

#[wasm_bindgen]
pub fn usbd_complete_epout(ep: usize, amount: u32) {
    crate::peripherals::usbd_nrf::complete_epout(sys(), ep, amount);
}

#[wasm_bindgen]
pub fn usbd_inject_setup(bytes: &[u8]) {
    let mut pkt = [0u8; 8];
    for (i, &b) in bytes.iter().take(8).enumerate() {
        pkt[i] = b;
    }
    crate::peripherals::usbd_nrf::inject_setup(sys(), pkt);
}

// ── NVMC erase driver API (driver applies 0xFF to guest flash, then completes)
// ── QSPI external flash driver API ──
#[wasm_bindgen]
pub fn qspi_register_flash(name: &str, data: &[u8]) {
    crate::peripherals::qspi_nrf::qspi_register_flash(name, data);
}

#[wasm_bindgen]
pub fn qspi_take_read() -> Vec<u32> {
    crate::peripherals::qspi_nrf::take_read(sys()).map(|(s, d, n)| vec![s, d, n]).unwrap_or_default()
}

#[wasm_bindgen]
pub fn qspi_complete_read() {
    crate::peripherals::qspi_nrf::complete_read(sys());
}

#[wasm_bindgen]
pub fn qspi_take_write() -> Vec<u32> {
    crate::peripherals::qspi_nrf::take_write(sys()).map(|(s, d, n)| vec![s, d, n]).unwrap_or_default()
}

#[wasm_bindgen]
pub fn qspi_complete_write(dst: u32, data: &[u8]) {
    crate::peripherals::qspi_nrf::complete_write(sys(), dst, data);
}

#[wasm_bindgen]
pub fn qspi_take_erase() -> Vec<u32> {
    crate::peripherals::qspi_nrf::take_erase(sys()).map(|(p, l)| vec![p, l]).unwrap_or_default()
}

#[wasm_bindgen]
pub fn qspi_complete_erase(ptr: u32, len_code: u32) {
    crate::peripherals::qspi_nrf::complete_erase(sys(), ptr, len_code);
}
#[wasm_bindgen]
pub fn nvmc_take_erase() -> Vec<u32> {
    crate::peripherals::nvmc_nrf::take_erase(sys()).map(|a| vec![a]).unwrap_or_default()
}

#[wasm_bindgen]
pub fn nvmc_complete_erase() {
    crate::peripherals::nvmc_nrf::complete_erase(sys());
}

// ── I2C bus taps (JS hardware layer: LSM303 accel/mag) ──
#[wasm_bindgen]
pub fn i2c_register_slave(peripheral: &str, address: u8) {
    use crate::ext_devices::i2c_tap::{I2cTap, I2cTapConfig};
    let config = I2cTapConfig { peripheral: peripheral.to_string(), address };
    system::get_ext_devices().lock().unwrap().i2c_taps
        .push(std::rc::Rc::new(std::cell::RefCell::new(I2cTap::new(config))));
}

#[wasm_bindgen]
pub fn i2c_take_events(peripheral: &str) -> Vec<u32> {
    system::i2c_tap_take_tx(peripheral)
}

#[wasm_bindgen]
pub fn i2c_push_rx(peripheral: &str, bytes: &[u8]) {
    system::i2c_tap_rx_push(peripheral, bytes);
}

use cpu::{Cpu, mem::{FlatMemory, Memory}};
#[wasm_bindgen]
pub struct WasmCpu { cpu: Cpu, mem: FlatMemory }
#[wasm_bindgen]
impl WasmCpu {
    #[wasm_bindgen(constructor)]
    pub fn new(sp: u32, pc: u32, flash_size: u32, ram_size: u32) -> Self { Self { cpu: Cpu::new(sp, pc), mem: FlatMemory::new(flash_size as usize, ram_size as usize) } }
    pub fn load_firmware(&mut self, data: &[u8], base: u32) { self.mem.load(data, base); }
    pub fn read8(&self, addr: u32) -> u8 { self.mem.read8(addr) }
    pub fn write8(&mut self, addr: u32, v: u8) { self.mem.write8(addr, v) }
    pub fn read32(&self, addr: u32) -> u32 { self.mem.read32(addr) }
    pub fn write32(&mut self, addr: u32, v: u32) { self.mem.write32(addr, v) }
    pub fn mem_read(&self, addr: u32, len: u32) -> Vec<u8> {
        (0..len).map(|i| self.mem.read8(addr.wrapping_add(i))).collect()
    }
    pub fn mem_write(&mut self, addr: u32, data: &[u8]) {
        for (i, &b) in data.iter().enumerate() { self.mem.write8(addr.wrapping_add(i as u32), b); }
    }
    /// External-master I2C write to our TWIS slave at `base`/`addr7`.
    /// Returns bytes accepted (0 on NACK/overflow; see error events).
    pub fn twis_master_write(&mut self, base: u32, addr: u8, data: &[u8]) -> u32 {
        crate::peripherals::twim_nrf::twis_master_write(sys(), &mut self.mem, base, addr, data)
    }
    /// External-master I2C read from our TWIS slave (ORC-padded).
    pub fn twis_master_read(&mut self, base: u32, addr: u8, len: u32) -> Vec<u8> {
        crate::peripherals::twim_nrf::twis_master_read(sys(), &mut self.mem, base, addr, len)
    }
    /// External-master SPI exchange with our SPIS slave (MISO bytes out).
    pub fn spis_exchange(&mut self, base: u32, mosi: &[u8]) -> Vec<u8> {
        crate::peripherals::twim_nrf::spis_exchange(sys(), &mut self.mem, base, mosi)
    }
    pub fn reset_cpu(&mut self, sp: u32, pc: u32) { self.cpu.reset(sp, pc); }
    pub fn set_deliver_irqs(&mut self, v: bool) { self.cpu.deliver_irqs = v; }
    pub fn sleeping(&self) -> bool { self.cpu.sleeping }
    pub fn wake(&mut self) { self.cpu.sleeping = false; }
    pub fn get_ipsr(&self) -> u32 { self.cpu.ipsr }
    pub fn get_pc(&self) -> u32 { self.cpu.regs.r[15] }
    pub fn get_sp(&self) -> u32 { self.cpu.regs.r[13] }
    pub fn get_regs(&self) -> Vec<u32> { self.cpu.regs.r.to_vec() }
    pub fn get_xpsr(&self) -> u32 { self.cpu.regs.xpsr }
    pub fn get_sregs(&self) -> Vec<u32> { self.cpu.regs.s.to_vec() }
    pub fn get_fpscr(&self) -> u32 { self.cpu.regs.fpscr }
    pub fn set_sreg(&mut self, i: u32, v: u32) {
        if (i as usize) < 32 { self.cpu.regs.s[i as usize] = v; }
    }
    pub fn set_fpscr(&mut self, v: u32) {
        self.cpu.regs.fpscr = (self.cpu.regs.fpscr & !0xFFC0_01FF) | (v & 0xFFC0_01FF);
    }
    pub fn get_primask(&self) -> u32 { self.cpu.regs.primask }
    pub fn fault_pc(&self) -> u32 { self.cpu.fault.map(|f| f.pc).unwrap_or(0xFFFF_FFFF) }
    pub fn fault_op1(&self) -> u32 { self.cpu.fault.map(|f| f.op1 as u32).unwrap_or(0) }
    pub fn fault_op2(&self) -> u32 { self.cpu.fault.map(|f| f.op2 as u32).unwrap_or(0) }
    pub fn fault_len(&self) -> u32 { self.cpu.fault.map(|f| f.len as u32).unwrap_or(0) }
    pub fn mem_fault(&self) -> u32 { self.mem.bad.get().unwrap_or(0xFFFF_FFFF) }
    pub fn step(&mut self, budget: u32) -> u32 { self.cpu.run(sys(), &mut self.mem, budget) }
    pub fn trace_start(&mut self) { cpu::trace_start(); }
    pub fn trace_stop(&mut self) { cpu::trace_stop(); }
    pub fn take_trace(&mut self) -> Vec<u32> { cpu::take_trace() }
}
