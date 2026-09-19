use std::sync::atomic::{AtomicPtr, Ordering};
use wasm_bindgen::prelude::*;

mod system;
pub mod peripherals;
pub mod ext_devices;
pub mod cpu;
pub mod sd_ble;
pub mod sd_evt;

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
    // Physical pin levels survive reboot (SYSRESET doesn't touch GPIO
    // input latches): carry input_state across the fresh peripheral map
    // so a button held through load/boot still reads pressed.
    let saved = try_sys().map(|s| s.p.gpio.borrow().input_state);
    set_sys(WasmSystem::new());
    if let Some(st) = saved {
        sys().p.gpio.borrow_mut().input_state = st;
    }
}

/// Initialize the emulator from an SVD XML string (e.g., nrf52833.svd).
#[wasm_bindgen]
pub fn init_svd(svd_xml: &str) {
    console_error_panic_hook::set_once();
    let saved = try_sys().map(|s| s.p.gpio.borrow().input_state);
    set_sys(WasmSystem::new_svd(svd_xml));
    if let Some(st) = saved {
        sys().p.gpio.borrow_mut().input_state = st;
    }
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
/// NFC antenna pins (P0.09/P0.10 with UICR.NFCPINS PROTECT=1, the reset
/// state) ignore levels — silicon routes them to the NFCT front-end.
#[wasm_bindgen]
pub fn gpio_set_input(port: u32, pin: u32, value: bool) {
    let reserved = crate::peripherals::nfct_nrf::nfct_pins_reserved(sys());
    sys().p.gpio.borrow_mut().set_input_pin_gated(port as u8, pin as u8, value, reserved);
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

/// Inject a received packet addressed to a DAB/DAP entry (air peer).
/// Convenience over inject_rx for the two-instance bridge: the first
/// byte is the device-address byte the match unit checks (DEVMATCH
/// when it equals a programmed, listened DAB entry).
#[wasm_bindgen]
pub fn radio_inject_rx_to(dab_idx: usize, bytes: &[u8]) {
    crate::peripherals::radio_nrf::inject_rx_to(sys(), dab_idx, bytes.to_vec());
}

/// Addressed inject with a path-loss in dB (air range model): the RX
/// completion stamps TXPOWER-minus-loss into the RSSI latch, so a
/// firmware RSSISTART after RX reads this packet's level like silicon.
#[wasm_bindgen]
pub fn radio_inject_rx_to_lossy(dab_idx: usize, bytes: &[u8], path_loss_db: u32) {
    crate::peripherals::radio_nrf::inject_rx_to_lossy(sys(), dab_idx, bytes.to_vec(), path_loss_db);
}

/// Plain inject with a path-loss in dB (same RSSI stamp, no address byte).
#[wasm_bindgen]
pub fn radio_inject_rx_lossy(bytes: &[u8], path_loss_db: u32) {
    crate::peripherals::radio_nrf::inject_rx_lossy(sys(), bytes.to_vec(), path_loss_db);
}

/// Complete RX with an explicit path loss (dB) for this packet's RSSI
/// stamp. Driver-side air calls this when it knows the range; the
/// plain complete_rx() keeps the queued/default loss.
#[wasm_bindgen]
pub fn radio_complete_rx_with_path_loss(path_loss_db: u32) {
    crate::peripherals::radio_nrf::complete_rx_with_path_loss(sys(), path_loss_db);
}

/// nRF52 TXPOWER code (SVD 0x50C) as signed dBm (+8..0, -4..-40).
/// Pure function for the driver link-budget (shared with the model).
#[wasm_bindgen]
pub fn radio_txpower_dbm(code: u32) -> i32 {
    crate::peripherals::radio_nrf::txpower_dbm(code)
}

/// Link-budget air level: TX dBm minus path loss, clamped [-127, 0].
/// Pure function so JS air and the model agree on one honest number.
#[wasm_bindgen]
pub fn radio_air_rssi_dbm(tx_code: u32, path_loss_db: u32) -> i32 {
    crate::peripherals::radio_nrf::air_rssi_dbm(tx_code, path_loss_db)
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

/// Set the 802.15.4 energy-detect sample level in dBm (negative).
/// Reported via EDSAMPLE on the next EDSTART; defaults to RSSI level.
#[wasm_bindgen]
pub fn radio_set_ed_dbm(dbm: i32) {
    crate::peripherals::radio_nrf::set_ed_dbm(sys(), dbm);
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

/// SoC event queue length (sd_evt phase 1: flash completions while the
/// SD is enabled). Debug/pump path; firmware drains via SVC 82.
#[wasm_bindgen]
pub fn sd_evt_queue_len() -> u32 {
    crate::sd_evt::queue_len() as u32
}

// ── SoftDevice BLE SVC face (GAP/GATTS/GATTC over the Bumble air bridge) ──
// ble_take_job() returns one staged driver job as u32 words; the first
// word is the tag. The driver resolves it over air and calls the
// matching complete_*. Empty vec = idle. Tags:
//   0 GattcRead  [conn, handle, offset]
//   1 GapConnect [a0..a5] (peer addr LE)
//   2 GapDisconnect [conn, reason]
//   3 GapRssiGet [conn]
//   4 GapScanStart [] (one live sighting -> ADV_REPORT)
//   5 GattcPrimDisc [conn, start, uuid16|0xFFFF(none)]
//   6 GattcCharDisc [conn, start, end]
//   7 GattcDescDisc [conn, start, end]
//   8 GattcWrite [conn, op, handle, len] + ble_take_data() bytes
//   9 GattsHvx [conn, handle, type, len] + ble_take_data() bytes
//   10 L2capTx [conn, cid, len] + ble_take_data() bytes
//   11 GapAuthenticate [conn] (pairing handshake over air)
//   12 GattcRelDisc [conn, start, end] (include walk)
//   13 GattcAttrInfoDisc [conn, start, end] (table walk)
//   14 GattcUuidRead [conn, uuid16|0xFFFF, start, end]
//   15 GattcValsRead [conn, count] + ble_take_data() = handles u16[count]
//   16 GattsServiceChanged [conn, start, end] (peer confirm -> SC_CONFIRM)
#[wasm_bindgen]
pub fn ble_take_job() -> Vec<u32> {
    match crate::sd_ble::take_job() {
        Some(crate::sd_ble::BleJob::GattcRead { conn, handle, offset }) => {
            vec![0, conn as u32, handle as u32, offset as u32]
        }
        Some(crate::sd_ble::BleJob::GapConnect { addr }) => {
            let mut v = vec![1u32];
            v.extend(addr.iter().map(|&b| b as u32));
            v
        }
        Some(crate::sd_ble::BleJob::GapDisconnect { conn, reason }) => {
            vec![2, conn as u32, reason as u32]
        }
        Some(crate::sd_ble::BleJob::GapRssiGet { conn }) => vec![3, conn as u32],
        Some(crate::sd_ble::BleJob::GapScanStart) => vec![4],
        Some(crate::sd_ble::BleJob::GattcPrimDisc { conn, start, uuid16 }) => {
            vec![5, conn as u32, start as u32, uuid16.map(|u| u as u32).unwrap_or(0xFFFF)]
        }
        Some(crate::sd_ble::BleJob::GattcCharDisc { conn, start, end }) => {
            vec![6, conn as u32, start as u32, end as u32]
        }
        Some(crate::sd_ble::BleJob::GattcDescDisc { conn, start, end }) => {
            vec![7, conn as u32, start as u32, end as u32]
        }
        Some(crate::sd_ble::BleJob::GattcWrite { conn, op, handle, ref data }) => {
            crate::sd_ble::stage_take_data(data.clone());
            vec![8, conn as u32, op as u32, handle as u32, data.len() as u32]
        }
        Some(crate::sd_ble::BleJob::GattsHvx { conn, handle, hvx_type, ref data }) => {
            crate::sd_ble::stage_take_data(data.clone());
            vec![9, conn as u32, handle as u32, hvx_type as u32, data.len() as u32]
        }
        Some(crate::sd_ble::BleJob::L2capTx { conn, cid, ref data }) => {
            crate::sd_ble::stage_take_data(data.clone());
            vec![10, conn as u32, cid as u32, data.len() as u32]
        }
        Some(crate::sd_ble::BleJob::GapAuthenticate { conn }) => vec![11, conn as u32],
        Some(crate::sd_ble::BleJob::GattcRelDisc { conn, start, end }) => {
            vec![12, conn as u32, start as u32, end as u32]
        }
        Some(crate::sd_ble::BleJob::GattcAttrInfoDisc { conn, start, end }) => {
            vec![13, conn as u32, start as u32, end as u32]
        }
        Some(crate::sd_ble::BleJob::GattcUuidRead { conn, uuid16, start, end }) => {
            vec![14, conn as u32, uuid16.map(|u| u as u32).unwrap_or(0xFFFF), start as u32, end as u32]
        }
        Some(crate::sd_ble::BleJob::GattcValsRead { conn, ref handles }) => {
            let mut v = vec![15u32, conn as u32, handles.len() as u32];
            v.extend(handles.iter().map(|&h| h as u32));
            v
        }
        Some(crate::sd_ble::BleJob::GattsServiceChanged { conn, start, end }) => {
            vec![16, conn as u32, start as u32, end as u32]
        }
        None => Vec::new(),
    }
}

/// Bytes staged alongside the last take_job (WRITE/HVX payloads only;
/// the SVC copies firmware bytes at call time so the driver read is
/// stable). Drained once per job; empty when the job carries no bytes.
#[wasm_bindgen]
pub fn ble_take_data() -> Vec<u8> {
    crate::sd_ble::take_staged_data()
}

#[wasm_bindgen]
pub fn ble_complete_gattc_read(conn: u16, handle: u16, offset: u16, data: &[u8]) {
    crate::sd_ble::complete_gattc_read(conn, handle, offset, data);
}

/// Complete a primary-service discovery with parallel arrays:
/// uuids[i] (0xFFFF = 128-bit, listed without number), starts[i],
/// ends[i]. Posts PRIM_DISC_RSP.
#[wasm_bindgen]
pub fn ble_complete_prim_disc(conn: u16, uuids: &[u16], starts: &[u16], ends: &[u16]) {
    let n = uuids.len().min(starts.len()).min(ends.len());
    let svcs: Vec<crate::sd_ble::DiscService> = (0..n)
        .map(|i| crate::sd_ble::DiscService {
            uuid16: if uuids[i] == 0xFFFF { None } else { Some(uuids[i]) },
            start: starts[i],
            end: ends[i],
        })
        .collect();
    crate::sd_ble::complete_prim_disc(conn, &svcs);
}

/// Complete a characteristic discovery: uuids[i] (0xFFFF = 128-bit),
/// props[i] (S132 u8 bitfield), decls[i], values[i]. Posts CHAR_DISC_RSP.
#[wasm_bindgen]
pub fn ble_complete_char_disc(conn: u16, uuids: &[u16], props: &[u8], decls: &[u16], values: &[u16]) {
    let n = uuids.len().min(props.len()).min(decls.len()).min(values.len());
    let chars: Vec<crate::sd_ble::DiscChar> = (0..n)
        .map(|i| crate::sd_ble::DiscChar {
            uuid16: if uuids[i] == 0xFFFF { None } else { Some(uuids[i]) },
            props: props[i],
            decl: decls[i],
            value: values[i],
        })
        .collect();
    crate::sd_ble::complete_char_disc(conn, &chars);
}

/// Complete a descriptor discovery: handles[i], uuids[i].
/// Posts DESC_DISC_RSP.
#[wasm_bindgen]
pub fn ble_complete_desc_disc(conn: u16, handles: &[u16], uuids: &[u16]) {
    let n = handles.len().min(uuids.len());
    let descs: Vec<crate::sd_ble::DiscDesc> = (0..n)
        .map(|i| crate::sd_ble::DiscDesc {
            handle: handles[i],
            uuid16: if uuids[i] == 0xFFFF { None } else { Some(uuids[i]) },
        })
        .collect();
    crate::sd_ble::complete_desc_disc(conn, &descs);
}

/// Complete a GATTC write with the over-air WRITE_RSP proof.
#[wasm_bindgen]
pub fn ble_complete_gattc_write(conn: u16, handle: u16, op: u8, data: &[u8]) {
    crate::sd_ble::complete_gattc_write(conn, handle, op, data);
}

/// Complete a relationship discovery: parallel arrays handles[i],
/// uuids[i] (0xFFFF = 128-bit), starts[i], ends[i]. Posts REL_DISC_RSP.
#[wasm_bindgen]
pub fn ble_complete_rel_disc(conn: u16, handles: &[u16], uuids: &[u16], starts: &[u16], ends: &[u16]) {
    let n = handles.len().min(uuids.len()).min(starts.len()).min(ends.len());
    let incs: Vec<crate::sd_ble::DiscInclude> = (0..n)
        .map(|i| crate::sd_ble::DiscInclude {
            handle: handles[i],
            uuid16: if uuids[i] == 0xFFFF { None } else { Some(uuids[i]) },
            start: starts[i],
            end: ends[i],
        })
        .collect();
    crate::sd_ble::complete_rel_disc(conn, &incs);
}

/// Complete an attribute-info discovery: handles[i], uuids[i].
/// Posts ATTR_INFO_RSP (16-bit format).
#[wasm_bindgen]
pub fn ble_complete_attr_info_disc(conn: u16, handles: &[u16], uuids: &[u16]) {
    let n = handles.len().min(uuids.len());
    let infos: Vec<crate::sd_ble::DiscAttrInfo> = (0..n)
        .map(|i| crate::sd_ble::DiscAttrInfo {
            handle: handles[i],
            uuid16: if uuids[i] == 0xFFFF { None } else { Some(uuids[i]) },
        })
        .collect();
    crate::sd_ble::complete_attr_info_disc(conn, &infos);
}

/// Complete a read-by-UUID: parallel handles[i] + flat values with
/// per-pair lengths lens[i] (ragged pads to the longest on the wire).
/// Posts UUID_READ_RSP.
#[wasm_bindgen]
pub fn ble_complete_uuid_read(conn: u16, handles: &[u16], flat: &[u8], lens: &[u16]) {
    let mut pairs = Vec::new();
    let mut off = 0usize;
    for (i, &h) in handles.iter().enumerate() {
        let n = lens.get(i).copied().unwrap_or(0) as usize;
        let end = (off + n).min(flat.len());
        pairs.push(crate::sd_ble::HandleValue { handle: h, value: flat[off..end].to_vec() });
        off = end;
    }
    crate::sd_ble::complete_uuid_read(conn, &pairs);
}

/// Complete a multi-read: concatenated values. Posts VALS_READ_RSP.
#[wasm_bindgen]
pub fn ble_complete_vals_read(conn: u16, data: &[u8]) {
    crate::sd_ble::complete_vals_read(conn, data);
}

/// Complete a peer notification/indication: posts HVX.
#[wasm_bindgen]
pub fn ble_complete_gattc_hvx(conn: u16, handle: u16, hvx_type: u8, data: &[u8]) {
    crate::sd_ble::complete_gattc_hvx(conn, handle, hvx_type, data);
}

#[wasm_bindgen]
pub fn ble_complete_gap_connect(peer: &[u8]) {
    let mut addr = [0u8; 6];
    for (i, &b) in peer.iter().take(6).enumerate() {
        addr[i] = b;
    }
    crate::sd_ble::complete_gap_connect(addr);
}

/// Complete a GAP disconnect: posts DISCONNECTED with the HCI reason.
#[wasm_bindgen]
pub fn ble_complete_gap_disconnect(conn: u16, reason: u8) {
    crate::sd_ble::complete_gap_disconnect(conn, reason);
}

/// Complete an RSSI sample: posts RSSI_CHANGED.
#[wasm_bindgen]
pub fn ble_complete_rssi(conn: u16, rssi: i8) {
    crate::sd_ble::complete_rssi(conn, rssi);
}

/// Complete a GATTS HVX emission: posts HVC confirm.
#[wasm_bindgen]
pub fn ble_complete_hvx(conn: u16, handle: u16) {
    crate::sd_ble::complete_hvx(conn, handle);
}

/// Complete a GAP connect: driver connected over air; returns the
/// assigned connection handle (INVALID when the table is full).
#[wasm_bindgen]
pub fn ble_complete_gap_connect_ret(peer: &[u8]) -> u16 {
    let mut addr = [0u8; 6];
    for (i, &b) in peer.iter().take(6).enumerate() {
        addr[i] = b;
    }
    crate::sd_ble::complete_gap_connect(addr)
}

/// Complete a pairing handshake the driver ran over air: posts
/// AUTH_STATUS (success) + CONN_SEC_UPDATE, marks link bonded.
#[wasm_bindgen]
pub fn ble_complete_pairing(conn: u16, bonded: bool) {
    crate::sd_ble::complete_pairing(conn, bonded);
}

/// Fail a pairing handshake: posts AUTH_STATUS with the S132 status
/// (e.g. 0x85 PAIRING_NOT_SUPP); link stays up, unencrypted.
#[wasm_bindgen]
pub fn ble_fail_pairing(conn: u16, status: u8) {
    crate::sd_ble::fail_pairing(conn, status);
}

/// Bond store: does the store hold keys for this 6B peer address with
/// this 10B master_id (silicon re-encrypt gate)? The bridge consults
/// this before answering SEC_INFO_REPLY: hit = reply with stored keys
/// + ENCRYPT; miss = all-NULL reply.
#[wasm_bindgen]
pub fn ble_bond_has_keys(peer: &[u8], master_id: &[u8]) -> bool {
    let mut p = [0u8; 6];
    for (i, &b) in peer.iter().take(6).enumerate() {
        p[i] = b;
    }
    let mut m = [0u8; 10];
    for (i, &b) in master_id.iter().take(10).enumerate() {
        m[i] = b;
    }
    crate::sd_ble::bond_has_keys(&p, &m)
}

/// Bond store: read back bonded keys (LTK[16] IRK[16] CSRK[16] MID[10]
/// = 52 bytes, empty when no bond). Bridge answers SEC_INFO_REPLY
/// from this instead of failing.
#[wasm_bindgen]
pub fn ble_bond_read_keys(peer: &[u8]) -> Vec<u8> {
    let mut p = [0u8; 6];
    for (i, &b) in peer.iter().take(6).enumerate() {
        p[i] = b;
    }
    match crate::sd_ble::bond_read_keys(&p) {
        Some((ltk, irk, csrk, mid)) => [ltk.as_slice(), irk.as_slice(), csrk.as_slice(), mid.as_slice()].concat(),
        None => Vec::new(),
    }
}

/// Bond store: driver-side insert (bridge confirmed air keys when
/// firmware passed NULL keysets).
#[wasm_bindgen]
pub fn ble_store_bond(peer: &[u8], ltk: &[u8], irk: &[u8], csrk: &[u8], master_id: &[u8]) {
    let mut p = [0u8; 6];
    for (i, &b) in peer.iter().take(6).enumerate() {
        p[i] = b;
    }
    let mut l = [0u8; 16];
    for (i, &b) in ltk.iter().take(16).enumerate() {
        l[i] = b;
    }
    let mut ir = [0u8; 16];
    for (i, &b) in irk.iter().take(16).enumerate() {
        ir[i] = b;
    }
    let mut c = [0u8; 16];
    for (i, &b) in csrk.iter().take(16).enumerate() {
        c[i] = b;
    }
    let mut m = [0u8; 10];
    for (i, &b) in master_id.iter().take(10).enumerate() {
        m[i] = b;
    }
    crate::sd_ble::store_bond(p, l, ir, c, m);
}

/// Explicit unbond: the next SEC_INFO_REQUEST for the peer MISSES.
#[wasm_bindgen]
pub fn ble_delete_bond(peer: &[u8]) -> bool {
    let mut p = [0u8; 6];
    for (i, &b) in peer.iter().take(6).enumerate() {
        p[i] = b;
    }
    crate::sd_ble::delete_bond(p)
}

/// TX-flow refill: driver moved one packet over air; refills one TX
/// token on the link and posts TX_COMPLETE with the free count.
#[wasm_bindgen]
pub fn ble_complete_tx_flow(conn: u16) {
    crate::sd_ble::complete_tx_flow(conn);
}

/// Peripheral-role accept: a peer answered our advertisement; brings
/// the link up with PERIPH role and posts CONNECTED. Returns handle.
#[wasm_bindgen]
pub fn ble_complete_peripheral_connect(peer: &[u8]) -> u16 {
    let mut p = [0u8; 6];
    for (i, &b) in peer.iter().take(6).enumerate() {
        p[i] = b;
    }
    crate::sd_ble::complete_peripheral_connect(p)
}

/// Conn-param update completion: posts CONN_PARAM_UPDATE on the link.
#[wasm_bindgen]
pub fn ble_complete_conn_param_update(conn: u16) {
    crate::sd_ble::complete_conn_param_update(conn);
}

/// Complete a Service Changed indication: the peer confirmed the
/// 0x2A05 indication over air; posts SC_CONFIRM on the link.
#[wasm_bindgen]
pub fn ble_complete_service_changed(conn: u16) {
    crate::sd_ble::complete_service_changed(conn);
}

/// Post a peer-initiated SEC_PARAMS_REQUEST: the peer started SMP
/// with these ble_gap_sec_params_t wire bytes (flags, min/max key
/// size, kdist_own, kdist_peer); firmware answers SEC_PARAMS_REPLY.
/// Returns false when the link cannot take a request.
#[wasm_bindgen]
pub fn ble_post_sec_params_request(conn: u16, peer_params: &[u8]) -> bool {
    let mut p = [0u8; 5];
    for (i, &b) in peer_params.iter().take(5).enumerate() {
        p[i] = b;
    }
    crate::sd_ble::post_sec_params_request(conn, p)
}

/// Post a peer-initiated SEC_INFO_REQUEST: the peer asks to re-encrypt
/// (peer_addr 7B type+6, master_id 10B ediv+rand[8], req bits: bit0
/// enc_info, bit1 id_info, bit2 sign_info). Firmware answers
/// SEC_INFO_REPLY, then ENCRYPT. Returns false when the link cannot
/// take a request.
#[wasm_bindgen]
pub fn ble_post_sec_info_request(conn: u16, peer_addr: &[u8], master_id: &[u8], req: u8) -> bool {
    let mut a = [0u8; 7];
    for (i, &b) in peer_addr.iter().take(7).enumerate() {
        a[i] = b;
    }
    let mut m = [0u8; 10];
    for (i, &b) in master_id.iter().take(10).enumerate() {
        m[i] = b;
    }
    crate::sd_ble::post_sec_info_request(conn, a, m, req)
}

/// Post an AUTH_KEY_REQUEST: the driver needs a key of `key_type`
/// (0 none, 1 passkey, 2 OOB); firmware answers AUTH_KEY_REPLY.
/// Returns false outside an accepted handshake.
#[wasm_bindgen]
pub fn ble_post_auth_key_request(conn: u16, key_type: u8) -> bool {
    crate::sd_ble::post_auth_key_request(conn, key_type)
}

/// Post a PASSKEY_DISPLAY: the driver shows this 6-digit ASCII passkey
/// (firmware answers AUTH_KEY_REPLY when match_request).
#[wasm_bindgen]
pub fn ble_post_passkey_display(conn: u16, passkey: &[u8], match_request: bool) -> bool {
    let mut p = [0u8; 6];
    for (i, &b) in passkey.iter().take(6).enumerate() {
        p[i] = b;
    }
    crate::sd_ble::post_passkey_display(conn, p, match_request)
}

/// Post a peer KEYPRESS_NOTIFY (type 0..=4). Returns false with no link.
#[wasm_bindgen]
pub fn ble_post_keypress(conn: u16, kp_not: u8) -> bool {
    crate::sd_ble::post_keypress(conn, kp_not)
}

/// Post an LESC_DHKEY_REQUEST (firmware answers LESC_DHKEY_REPLY;
/// OOB via LESC_OOB_DATA_SET when oobd_req). Returns false outside an
/// accepted handshake.
#[wasm_bindgen]
pub fn ble_post_lesc_dhkey_request(conn: u16, oobd_req: bool) -> bool {
    crate::sd_ble::post_lesc_dhkey_request(conn, oobd_req)
}

/// Complete an L2CAP TX: posts the RX echo on (conn, cid).
#[wasm_bindgen]
pub fn ble_complete_l2cap_rx(conn: u16, cid: u16, data: &[u8]) {
    crate::sd_ble::complete_l2cap_rx(conn, cid, data);
}

/// Live connection handles (each u16 one link). Empty = no links.
#[wasm_bindgen]
pub fn ble_conn_handles() -> Vec<u16> {
    crate::sd_ble::conn_handles()
}

/// Connection security: [sec_mode, key_size] for the link
/// (mode 0x11 open, 0x21 encrypted-after-pairing).
#[wasm_bindgen]
pub fn ble_conn_sec(conn: u16) -> Vec<u8> {
    crate::sd_ble::conn_sec(conn)
}

/// GAP TX power level in dBm, as stored by TX_POWER_SET (debug/export).
/// Default 0 (silicon reset); only the S132-legal set is ever stored.
#[wasm_bindgen]
pub fn ble_tx_power_dbm() -> i8 {
    crate::sd_ble::tx_power_dbm()
}

/// Advertising state: [active, directed, filter_policy, whitelist_addrs].
/// Armed by ADV_START validation, cleared by ADV_STOP / reset.
#[wasm_bindgen]
pub fn ble_adv_state() -> Vec<u8> {
    let (active, directed, fp, wl) = crate::sd_ble::adv_state();
    vec![active as u8, directed as u8, fp, wl]
}

/// Directed-advertising peer address (6 LE bytes; valid when directed).
#[wasm_bindgen]
pub fn ble_adv_peer_addr() -> Vec<u8> {
    crate::sd_ble::adv_peer_addr().to_vec()
}

/// Post a peer SEC_REQUEST (firmware answers AUTHENTICATE).
#[wasm_bindgen]
pub fn ble_post_sec_request(conn: u16, bond: bool, mitm: bool, lesc: bool, keypress: bool) -> bool {
    crate::sd_ble::post_sec_request(conn, bond, mitm, lesc, keypress)
}

/// Post a peer CONN_PARAM_UPDATE_REQUEST (firmware answers with the
/// CONN_PARAM_UPDATE request SVC).
#[wasm_bindgen]
pub fn ble_post_conn_param_update_request(conn: u16) -> bool {
    crate::sd_ble::post_conn_param_update_request(conn)
}

/// Post a SCAN_REQ_REPORT (a scanner hit our advertisement).
#[wasm_bindgen]
pub fn ble_post_scan_req_report(peer: &[u8], rssi: i8) -> bool {
    let mut p = [0u8; 6];
    for (i, &b) in peer.iter().take(6).enumerate() {
        p[i] = b;
    }
    crate::sd_ble::post_scan_req_report(p, rssi)
}

/// Post a GAP TIMEOUT (src 0 adv, 1 sec-req, 2 scan, 3 conn).
#[wasm_bindgen]
pub fn ble_post_gap_timeout(conn: u16, src: u8) -> bool {
    crate::sd_ble::post_gap_timeout(conn, src)
}

/// Post a GATTC TIMEOUT (ATT protocol).
#[wasm_bindgen]
pub fn ble_post_gattc_timeout(conn: u16) -> bool {
    crate::sd_ble::post_gattc_timeout(conn)
}

/// Post a GATTS TIMEOUT (ATT protocol).
#[wasm_bindgen]
pub fn ble_post_gatts_timeout(conn: u16) -> bool {
    crate::sd_ble::post_gatts_timeout(conn)
}

/// Post a USER_MEM_REQUEST (firmware answers USER_MEM_REPLY).
#[wasm_bindgen]
pub fn ble_post_user_mem_request(conn: u16, mem_type: u8) -> bool {
    crate::sd_ble::post_user_mem_request(conn, mem_type)
}

/// Post a USER_MEM_RELEASE (informational, no reply path).
#[wasm_bindgen]
pub fn ble_post_user_mem_release(conn: u16, mem_type: u8) -> bool {
    crate::sd_ble::post_user_mem_release(conn, mem_type)
}

/// Post a GATTS RW_AUTHORIZE_REQUEST (firmware answers RW_AUTHORIZE_REPLY).
#[wasm_bindgen]
pub fn ble_post_rw_authorize_request(
    conn: u16,
    auth_type: u8,
    handle: u16,
    offset: u16,
    op: u8,
    data: &[u8],
) -> bool {
    crate::sd_ble::post_rw_authorize_request(conn, auth_type, handle, offset, op, data)
}

/// Post a GATTS SYS_ATTR_MISSING (firmware answers SYS_ATTR_SET).
#[wasm_bindgen]
pub fn ble_post_sys_attr_missing(conn: u16) -> bool {
    crate::sd_ble::post_sys_attr_missing(conn)
}

/// Post a GATTS SC_CONFIRM (header only, no reply path).
#[wasm_bindgen]
pub fn ble_post_sc_confirm(conn: u16) -> bool {
    crate::sd_ble::post_sc_confirm(conn)
}

#[wasm_bindgen]
pub fn ble_post_adv_report(peer: &[u8], rssi: i8, scan_rsp: bool, data: &[u8]) {
    let mut addr = [0u8; 6];
    for (i, &b) in peer.iter().take(6).enumerate() {
        addr[i] = b;
    }
    crate::sd_ble::post_adv_report(addr, rssi, scan_rsp, data);
}

/// Post a peer write to our table: conn handle, attr handle,
/// uuid16 (0xFFFF = 128-bit/vendor), op (1 = write request), bytes.
#[wasm_bindgen]
pub fn ble_post_gatts_write(conn: u16, handle: u16, uuid16: u16, op: u8, data: &[u8]) {
    crate::sd_ble::post_gatts_write(
        conn,
        handle,
        if uuid16 == 0xFFFF { None } else { Some(uuid16) },
        op,
        data,
    );
}

#[wasm_bindgen]
pub fn ble_enabled() -> bool {
    crate::sd_ble::is_enabled()
}

#[wasm_bindgen]
pub fn ble_queue_len() -> u32 {
    crate::sd_ble::queue_len() as u32
}

#[wasm_bindgen]
pub fn ble_batt_level() -> u8 {
    crate::sd_ble::batt_level()
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
    pub fn step(&mut self, budget: u32) -> u32 {
        // Publish RAM so UARTE STARTTX can snapshot TXD bytes synchronously
        // (P49 N+1 drops); guard clears on return even on host panic.
        let _snap = crate::peripherals::uarte_nrf::tx_snapshot_guard(&self.mem);
        self.cpu.run(sys(), &mut self.mem, budget)
    }
    pub fn trace_start(&mut self) { cpu::trace_start(); }
    pub fn trace_stop(&mut self) { cpu::trace_stop(); }
    pub fn take_trace(&mut self) -> Vec<u32> { cpu::take_trace() }
}
