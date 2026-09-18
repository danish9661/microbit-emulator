use std::sync::atomic::{AtomicU64, AtomicBool, AtomicI32, AtomicU8, AtomicU32, Ordering};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Mutex;
use crate::peripherals::{Peripherals, gpio_nrf::GpioPorts};
use crate::ext_devices::ExtDevices;

// UART output buffer: USART write_dr pushes chars here, JS reads via get_uart_output()
use std::sync::OnceLock;
static UART_OUTPUT: OnceLock<Mutex<String>> = OnceLock::new();
pub fn get_uart_output() -> &'static Mutex<String> {
    UART_OUTPUT.get_or_init(|| Mutex::new(String::new()))
}

#[cfg(test)]
static BOOT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
/// Serialize tests that install the process-global system (boot() and
/// any test driving mem hooks that need the installed instance).
#[cfg(test)]
pub(crate) fn lock_boot() -> std::sync::MutexGuard<'static, ()> {
    BOOT_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}
#[cfg(test)]
static UART_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
/// Serialize tests that assert on the process-global UART buffer
/// (firmware marker tests + peripheral console tests). Without it a
/// concurrent boot() drain clears the buffer mid-assert.
#[cfg(test)]
pub fn lock_uart() -> std::sync::MutexGuard<'static, ()> {
    UART_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
static I2C_TAP_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
/// Serialize tests sharing a global I2C tap queue (currently TWIM0: the
/// sensors firmware test and the DMA driver test both push/take it).
/// A stolen byte fails the other's `contains` assert ~1/50 runs.
#[cfg(test)]
pub fn lock_i2c_tap() -> std::sync::MutexGuard<'static, ()> {
    I2C_TAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}/// Best-effort drain guard for boot(): never blocks (a marker test may hold
/// the lock across its whole run, including its own boot() call — blocking
/// here would deadlock the same thread).
#[cfg(test)]
pub fn try_lock_uart() -> Option<std::sync::MutexGuard<'static, ()>> {
    UART_TEST_LOCK.try_lock().ok()
}

// Global ExtDevices: populated by JS add_* calls before init
static EXT_DEVICES: OnceLock<Mutex<ExtDevices>> = OnceLock::new();
pub fn get_ext_devices() -> &'static Mutex<ExtDevices> {
    EXT_DEVICES.get_or_init(|| Mutex::new(ExtDevices::default()))
}

pub static INSTRUCTION_COUNT: AtomicU64 = AtomicU64::new(0);
pub fn instruction_count() -> u64 { INSTRUCTION_COUNT.load(Ordering::Relaxed) }

static WATCHDOG_RESET_EVENT: AtomicBool = AtomicBool::new(false);
// RESETREAS latch (POWER 0x40000400): bit 2 SREQ is set on an AIRCR
// SYSRESETREQ / watchdog reboot so post-reset firmware (MBR/SD) sees a
// software-reset cause instead of power-on. Write-1-clears via resetreas_clear.
static RESETREAS_LATCH: AtomicU32 = AtomicU32::new(0);
pub fn resetreas() -> u32 { RESETREAS_LATCH.load(Ordering::Acquire) }
pub fn resetreas_clear(mask: u32) { RESETREAS_LATCH.fetch_and(!mask, Ordering::Release); }
// MWU watch gate: set while any MWU region/pregion is armed. The memory
// layer checks this single atomic per access (no cost when disarmed)
// and calls mwu_note() only then.
static MWU_ARMED: AtomicBool = AtomicBool::new(false);
pub fn mwu_armed() -> bool {
    MWU_ARMED.load(Ordering::Relaxed)
}
pub fn mwu_set_armed(v: bool) {
    MWU_ARMED.store(v, Ordering::Relaxed)
}
// MPU master-enable latch (MPU_CTRL.ENABLE write). Level semantics follow
// the register: clearing ENABLE clears this. The driver halts while set —
// protection is not enforced, so running on would be silently wrong.
static MPU_ENABLED: AtomicBool = AtomicBool::new(false);
/// True while the guest holds MPU_CTRL.ENABLE (protection unmodeled).
pub fn is_mpu_enabled() -> bool { MPU_ENABLED.load(Ordering::Acquire) }
pub fn set_mpu_enabled(v: bool) { MPU_ENABLED.store(v, Ordering::Release); }
// Current CPU context for the MPU check (FlatMemory has no CPU handle).
// Updated at Cpu::new/reset, exception entry/return, and MSR CONTROL —
// the only points where (ipsr, CONTROL) change, so there is zero
// per-instruction cost. Relaxed: single-threaded producer/consumer.
static CURRENT_PRIV: AtomicBool = AtomicBool::new(true);
static CURRENT_HFNMI: AtomicBool = AtomicBool::new(false);
pub fn current_privileged() -> bool { CURRENT_PRIV.load(Ordering::Relaxed) }
pub fn current_hfnmi() -> bool { CURRENT_HFNMI.load(Ordering::Relaxed) }
pub fn set_cpu_context(priv_: bool, hfnmi: bool) {
    CURRENT_PRIV.store(priv_, Ordering::Relaxed);
    CURRENT_HFNMI.store(hfnmi, Ordering::Relaxed);
}
// Live exception number for ICSR.VECTACTIVE (SCB reads have no CPU
// handle, like the privilege cache above). Updated at every take/chain/
// return alongside ipsr; 0 in thread mode.
static CURRENT_IPSR: AtomicU32 = AtomicU32::new(0);
pub fn current_ipsr() -> u32 { CURRENT_IPSR.load(Ordering::Relaxed) }
pub fn set_current_ipsr(v: u32) { CURRENT_IPSR.store(v, Ordering::Relaxed); }
// Force the next MPU check(s) to unprivileged, for LDRT/STRT (which probe
// memory as-unprivileged even in handler mode). Set/cleared around the
// single access by a Drop guard in the decoder, so no path leaks it.
static MPU_FORCE_UNPRIV: AtomicBool = AtomicBool::new(false);
pub fn set_mpu_force_unpriv(v: bool) { MPU_FORCE_UNPRIV.store(v, Ordering::Relaxed); }
pub(crate) fn mpu_force_unpriv() -> bool { MPU_FORCE_UNPRIV.load(Ordering::Relaxed) }
// CCR.UNALIGN_TRP cache for the access hot path (mem.rs must not do a
// model read per access): refreshed on every SCB CCR write. Reset state
// is clear, matching CCR reset.
static UNALIGN_TRP: AtomicBool = AtomicBool::new(false);
pub fn set_unalign_trp(v: bool) { UNALIGN_TRP.store(v, Ordering::Relaxed); }
pub(crate) fn unalign_trp() -> bool { UNALIGN_TRP.load(Ordering::Relaxed) }
// Deferred alignment-fault channel: like the MPU data path, the faulting
// access completes dropped and the UsageFault raises before the next
// fetch (one-instruction imprecision, documented; flags exact).
static ALIGN_FAULT_VALID: AtomicBool = AtomicBool::new(false);
static ALIGN_FAULT_ADDR: AtomicU32 = AtomicU32::new(0);
pub fn pend_align_fault(addr: u32) {
    if ALIGN_FAULT_VALID.load(Ordering::Relaxed) {
        return;
    }
    ALIGN_FAULT_ADDR.store(addr, Ordering::Relaxed);
    ALIGN_FAULT_VALID.store(true, Ordering::Release);
}
pub fn take_align_fault() -> Option<u32> {
    if ALIGN_FAULT_VALID.swap(false, Ordering::Acquire) {
        Some(ALIGN_FAULT_ADDR.load(Ordering::Relaxed))
    } else {
        None
    }
}
// Deferred bus-fault channel (unmapped access = precise BusFault on
// silicon): same deferred shape as the MPU/align paths (access completes
// dummy, flags exact, PC one behind). Peripheral-space holes are NOT
// routed here — unlisted SVD devices read-as-0 by design (many are
// documented-reserved; HALs probe them), so only the mem.rs bad-arms
// (wild memory: null derefs, overruns, gaps) pend.
static BUS_FAULT_VALID: AtomicBool = AtomicBool::new(false);
static BUS_FAULT_ADDR: AtomicU32 = AtomicU32::new(0);
static BUS_FAULT_EXEC: AtomicBool = AtomicBool::new(false);
pub fn pend_bus_fault(addr: u32, exec: bool) {
    if BUS_FAULT_VALID.load(Ordering::Relaxed) {
        return;
    }
    BUS_FAULT_ADDR.store(addr, Ordering::Relaxed);
    BUS_FAULT_EXEC.store(exec, Ordering::Relaxed);
    BUS_FAULT_VALID.store(true, Ordering::Release);
}
pub fn take_bus_fault() -> Option<(u32, bool)> {
    if BUS_FAULT_VALID.swap(false, Ordering::Acquire) {
        Some((BUS_FAULT_ADDR.load(Ordering::Relaxed), BUS_FAULT_EXEC.load(Ordering::Relaxed)))
    } else {
        None
    }
}
// Deferred MPU data-fault channel (see cpu/mod.rs): FlatMemory latches a
// violation (returning dummy/dropping the access); the run loop raises it
// before the next fetch. One instruction may complete with dummy data —
// documented imprecision; fault vector/flags/address are exact.
static MPU_FAULT_VALID: AtomicBool = AtomicBool::new(false);
static MPU_FAULT_ADDR: AtomicU32 = AtomicU32::new(0);
static MPU_FAULT_EXEC: AtomicBool = AtomicBool::new(false);
pub fn pend_mpu_fault(addr: u32, exec: bool) {
    // First fault wins: a split access (RAM write32 = 4x write8) pends once
    // per byte; the last byte must not overwrite the faulting address
    // (silicon reports the access; the PPB probe needs the base address).
    if MPU_FAULT_VALID.load(Ordering::Relaxed) {
        return;
    }
    MPU_FAULT_ADDR.store(addr, Ordering::Relaxed);
    MPU_FAULT_EXEC.store(exec, Ordering::Relaxed);
    MPU_FAULT_VALID.store(true, Ordering::Release);
}
pub fn take_mpu_fault() -> Option<(u32, bool)> {
    if MPU_FAULT_VALID.swap(false, Ordering::Acquire) {
        Some((MPU_FAULT_ADDR.load(Ordering::Relaxed), MPU_FAULT_EXEC.load(Ordering::Relaxed)))
    } else {
        None
    }
}
/// Latch MemManage fault state (CFSR MMFSR bits + MMFAR) via read-modify-
/// write, preserving any BusFault/UsageFault bits already latched.
pub fn latch_memmanage_fault(sys: &WasmSystem, mmfsr_bits: u32, mmfar: Option<u32>) {
    let cfsr = sys.p.read(sys, 0xE000ED28, 4);
    sys.p.write(sys, 0xE000ED28, 4, cfsr | (mmfsr_bits & 0xFF));
    if let Some(a) = mmfar {
        sys.p.write(sys, 0xE000ED34, 4, a);
    }
}
pub fn is_watchdog_reset_requested() -> bool { WATCHDOG_RESET_EVENT.swap(false, Ordering::Acquire) }
/// Latch a watchdog reset event (e.g. SCB AIRCR SYSRESETREQ). Consumed by
/// the JS driver (is_watchdog_reset_requested) to reboot the instance.
/// Also latches RESETREAS.SREQ for the rebooted firmware to observe.
pub fn request_watchdog_reset(_cause: u8) {
    WATCHDOG_RESET_EVENT.store(true, Ordering::Release);
    RESETREAS_LATCH.fetch_or(1 << 2, Ordering::Release);
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaDir { Read, Write, MemCopy }

#[derive(Debug, Clone)]
pub struct DmaTransfer {
    pub direction: DmaDir,
    pub stream_idx: usize,
    pub dma_name: String,
    pub src: u32,
    pub dst: u32,
    pub size: usize,
    pub peri_addr: u32,
    pub peripheral: bool,
    pub pinc: bool, // PINC: increment the peripheral address per transfer
    pub p_size: usize, // peripheral data width in bytes (PSIZE)
}

static DMA_COMPLETED: [AtomicBool; 8] = [
    AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false),
    AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false),
];

// Per-stream DMA interrupt info: IRQ number (-1 = none) and flags (bit 0=TCIE, 1=HTIE, 2=TEIE)
static DMA_STREAM_IRQ: [AtomicI32; 8] = [
    AtomicI32::new(-1), AtomicI32::new(-1), AtomicI32::new(-1), AtomicI32::new(-1),
    AtomicI32::new(-1), AtomicI32::new(-1), AtomicI32::new(-1), AtomicI32::new(-1),
];
static DMA_STREAM_FLAGS: [AtomicU8; 8] = [
    AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
    AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0),
];

// ── I2S TX capture FIFO (browser playback / test compare) ───────────────
// DR writes (TX DMA MEM->PERIPH) complete here via complete_i2s_tx;
// JS drains with i2s_take_capture.
static I2S_CAPTURE: OnceLock<Mutex<Vec<u8>>> = OnceLock::new();
pub fn i2s_capture() -> Option<&'static Mutex<Vec<u8>>> {
    Some(I2S_CAPTURE.get_or_init(|| Mutex::new(Vec::new())))
}
pub fn i2s_take_capture() -> Vec<u8> {
    I2S_CAPTURE.get().map_or(Vec::new(), |m| std::mem::take(&mut *m.lock().unwrap()))
}
pub fn i2s_clear() {
    if let Some(m) = I2S_CAPTURE.get() {
        m.lock().unwrap().clear();
    }
}

// ── SPI bus taps (JS hardware layer plumbing) ──────────────────────────────
// Event word layout: bit 31 = CS edge event, bit 30 = asserted (1) when CS
// is a CS event, bit 29 = DC level (1 = data) when the tap has a DC pin,
// bits 7..0 = the shifted byte. Byte and CS events interleave in the order
// the controller produced them.
static SPI_TAP_EVENTS: OnceLock<Mutex<std::collections::HashMap<String, Vec<u32>>>> = OnceLock::new();
static SPI_TAP_MISO: OnceLock<Mutex<std::collections::HashMap<String, Vec<u8>>>> = OnceLock::new();

fn spi_tap_events() -> &'static Mutex<std::collections::HashMap<String, Vec<u32>>> {
    SPI_TAP_EVENTS.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}
fn spi_tap_miso() -> &'static Mutex<std::collections::HashMap<String, Vec<u8>>> {
    SPI_TAP_MISO.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

/// Tap event queues are lossy observer channels: cap each peripheral at
/// 4096 entries (drop oldest) so bulk DMA traffic without a drainer
/// (e.g. display frames) cannot grow memory unboundedly in long runs.
fn tap_push_capped(map: &mut std::collections::HashMap<String, Vec<u32>>, peri: &str, v: u32) {
    let q = map.entry(peri.to_string()).or_default();
    q.push(v);
    let over = q.len().saturating_sub(4096);
    if over > 0 {
        q.drain(..over);
    }
}
pub fn spi_tap_push_byte(peri: &str, v: u32) {
    tap_push_capped(&mut spi_tap_events().lock().unwrap(), peri, v & 0x2FF);
}
pub fn spi_tap_push_cs(peri: &str, asserted: bool) {
    let e = 0x8000_0000u32 | (if asserted { 1 << 30 } else { 0 });
    spi_tap_events().lock().unwrap().entry(peri.to_string()).or_default().push(e);
}
pub fn spi_tap_take_events(peri: &str) -> Vec<u32> {
    spi_tap_events().lock().unwrap().get_mut(peri).map(std::mem::take).unwrap_or_default()
}
pub fn spi_tap_miso_push(peri: &str, bytes: &[u8]) {
    spi_tap_miso().lock().unwrap().entry(peri.to_string()).or_default().extend_from_slice(bytes);
}
pub(crate) fn spi_tap_miso_pop(peri: &str) -> u8 {
    spi_tap_miso().lock().unwrap().get_mut(peri).and_then(|q| q.first().copied().map(|b| { q.remove(0); b })).unwrap_or(0xFF)
}

// ── I2C bus taps (JS hardware layer plumbing) ─────────────────────────────
// The TX queue carries u32 events: bit31 = boundary event (bit30 = 1 START /
// 0 STOP), otherwise the low byte is one master-write byte. START/STOP let
// the JS device parser find transaction group boundaries (SSD1306 needs
// them: a data group's length is only terminated by STOP).
static I2C_TAP_TX: OnceLock<Mutex<std::collections::HashMap<String, Vec<u32>>>> = OnceLock::new();
static I2C_TAP_RX: OnceLock<Mutex<std::collections::HashMap<String, Vec<u8>>>> = OnceLock::new();

fn i2c_tap_tx() -> &'static Mutex<std::collections::HashMap<String, Vec<u32>>> {
    I2C_TAP_TX.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}
fn i2c_tap_rx() -> &'static Mutex<std::collections::HashMap<String, Vec<u8>>> {
    I2C_TAP_RX.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

pub fn i2c_tap_push_tx(peri: &str, v: u8) {
    tap_push_capped(&mut i2c_tap_tx().lock().unwrap(), peri, v as u32);
}
pub fn i2c_tap_push_event(peri: &str, ev: u32) {
    tap_push_capped(&mut i2c_tap_tx().lock().unwrap(), peri, ev);
}
pub fn i2c_tap_take_tx(peri: &str) -> Vec<u32> {
    i2c_tap_tx().lock().unwrap().get_mut(peri).map(std::mem::take).unwrap_or_default()
}
pub fn i2c_tap_rx_push(peri: &str, bytes: &[u8]) {
    i2c_tap_rx().lock().unwrap().entry(peri.to_string()).or_default().extend_from_slice(bytes);
}
pub(crate) fn i2c_tap_rx_pop(peri: &str) -> u8 {
    i2c_tap_rx().lock().unwrap().get_mut(peri).and_then(|q| q.first().copied().map(|b| { q.remove(0); b })).unwrap_or(0xFF)
}

pub struct WasmSystem {
    pub p: Rc<Peripherals>,
    pending_dma: RefCell<Vec<DmaTransfer>>,
}

#[cfg(test)]
pub fn test_dummy_system() -> ::std::rc::Rc<crate::system::System> {
    use crate::ext_devices::ExtDevices;
    use crate::peripherals::Peripherals;
    let gpio = GpioPorts::default();
    // Empty ext devices: keeps tests independent of the global (shared,
    // Rc<RefCell>-based) device list, whose cross-thread borrows race when
    // tests run in parallel (see bug fix 2026-08-10).
    let empty = ExtDevices::default();
    let p = Rc::new(Peripherals::new_wasm(gpio, &empty));
    ::std::rc::Rc::new(WasmSystem { p, pending_dma: RefCell::new(Vec::new()) })
}

impl WasmSystem {
    pub fn new() -> Self {
        let gpio = GpioPorts::default();
        let ext = get_ext_devices().lock().unwrap();
        let p = Rc::new(Peripherals::new_wasm(gpio, &*ext));
        drop(ext);
        WasmSystem { p, pending_dma: RefCell::new(Vec::new()) }
    }

    pub fn new_svd(svd_xml: &str) -> Self {
        let gpio = GpioPorts::default();
        let ext = get_ext_devices().lock().unwrap();
        let p = Rc::new(Peripherals::from_svd(svd_xml, gpio, &*ext));
        drop(ext);
        WasmSystem { p, pending_dma: RefCell::new(Vec::new()) }
    }

    /// Atomically remove and return the oldest queued DMA transfer iff it is
    /// a pure memory-to-memory move (MemCopy direction, no peripheral side).
    /// The CPU core drains these synchronously right after the guest's EN
    /// store, so polling firmware observes completion (data + TCIF/HTIF)
    /// without waiting for the JS driver round-trip. Peripheral-involving
    /// transfers always stay staged for the driver (it owns the data path).
    pub fn take_memcopy_dma_transfer(&self) -> Option<DmaTransfer> {
        let mut pending = self.pending_dma.borrow_mut();
        match pending.first() {
            Some(t) if t.direction == DmaDir::MemCopy && !t.peripheral => Some(pending.remove(0)),
            _ => None,
        }
    }

    pub fn mark_dma_completed(&self, stream_idx: usize, _success: bool) {
        DMA_COMPLETED[stream_idx].store(true, Ordering::Release);
        // Fire NVIC interrupt after transfer completes
        if stream_idx < 8 {
            let irq = DMA_STREAM_IRQ[stream_idx].swap(-1, Ordering::Acquire);
            if irq >= 0 {
                let flags = DMA_STREAM_FLAGS[stream_idx].swap(0, Ordering::Acquire);
                if flags & 0x7 != 0 {
                    self.p.nvic.borrow_mut().set_intr_pending(irq);
                }
            }
        }
    }

    pub fn tick(&self) {
        let p = self.p.clone();
        for slot in &p.peripherals {
            slot.peripheral.borrow_mut().tick(self);
        }
        p.nvic.borrow_mut().maybe_set_systick_intr_pending();
    }

    pub fn addr_desc(&self, addr: u32) -> String {
        self.p.addr_desc(addr)
    }
}

pub type System = WasmSystem;

// SAFETY: WasmSystem contains Rc<RefCell> peripherals and is single-system
// (SYS AtomicPtr). On wasm32-unknown-unknown the module is single-threaded
// — Send/Sync are never exercised. wasm-bindgen requires them for exported
// types, so we assert unsafely. Native `cargo test` (multi-threaded) is
// guarded by per-suite Mutex locks (CAN_TEST_LOCK, AUDIO_TEST_LOCK, etc.)
// to avoid `already borrowed` panics. Do not share WasmSystem across OS
// threads in a native build; use the WASM artifact for multi-instance.
#[cfg(target_arch = "wasm32")]
unsafe impl Sync for WasmSystem {}
#[cfg(target_arch = "wasm32")]
unsafe impl Send for WasmSystem {}
#[cfg(not(target_arch = "wasm32"))]
unsafe impl Sync for WasmSystem {}
#[cfg(not(target_arch = "wasm32"))]
unsafe impl Send for WasmSystem {}

// ── process-wide state reset ────────────────────────────────────────────────
/// Clear every process-lifetime global so a fresh emulator instance starts
/// clean.  Without this, creating a second instance in the same process is
/// broken in a subtle way: `ExtDevices` ACCUMULATES, and the peripheral
/// constructors use `find_*_device(name)`, which returns the FIRST match —
/// so instance 2 silently binds to instance 1's devices (measured: a regfile
/// seeded 0x22 read back 0x11 from the previous instance, and rtc_test hung
/// right after its first UART line when run after another firmware).
///
/// Call this BEFORE registering devices for a new instance (emulator.js does
/// it immediately after the wasm module is ready).  It is safe to call when
/// no instance exists — every table is lazily created.
pub fn reset_globals() {
    use std::sync::atomic::Ordering::Relaxed;
    if let Some(m) = EXT_DEVICES.get() { *m.lock().unwrap() = ExtDevices::default(); }
    if let Some(m) = UART_OUTPUT.get() { m.lock().unwrap().clear(); }
    if let Some(m) = SPI_TAP_EVENTS.get() { m.lock().unwrap().clear(); }
    if let Some(m) = SPI_TAP_MISO.get() { m.lock().unwrap().clear(); }
    if let Some(m) = I2C_TAP_TX.get() { m.lock().unwrap().clear(); }
    if let Some(m) = I2C_TAP_RX.get() { m.lock().unwrap().clear(); }
    crate::peripherals::qspi_nrf::qspi_clear();
    crate::sd_ble::reset_for_test();
    crate::sd_evt::reset_sd_evt();
    WATCHDOG_RESET_EVENT.store(false, Relaxed);
    RESETREAS_LATCH.store(0, Relaxed);
    MPU_ENABLED.store(false, Relaxed);
    MPU_FAULT_VALID.store(false, Relaxed);
    ALIGN_FAULT_VALID.store(false, Relaxed);
    BUS_FAULT_VALID.store(false, Relaxed);
    UNALIGN_TRP.store(false, Relaxed);
    set_mpu_force_unpriv(false);
    CURRENT_PRIV.store(true, Relaxed);
    CURRENT_HFNMI.store(false, Relaxed);
    CURRENT_IPSR.store(0, Relaxed);
    for i in 0..8 {
        DMA_COMPLETED[i].store(false, Relaxed);
        DMA_STREAM_IRQ[i].store(0, Relaxed);
        DMA_STREAM_FLAGS[i].store(0, Relaxed);
    }
    // NOTE: deliberately NOT resetting INSTRUCTION_COUNT here — peripherals
    // capture last_tick at construction; zeroing the global afterwards makes
    // elapsed = now.wrapping_sub(last_tick) enormous and breaks tick logic.
    // INSTRUCTION_COUNT.store(0, Relaxed);
}
