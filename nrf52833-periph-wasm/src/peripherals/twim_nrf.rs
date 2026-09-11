use crate::system::System;
use super::Peripheral;

/// TWIM0 @ 0x40003000 (IRQ 3) / TWIM1 @ 0x40004000 (IRQ 4, SVD ground
/// truth). Master subset + EASYDMA, all offsets/bit numbers from
/// nrf52833.svd: TASKS_STARTRX 0x000 / STARTTX 0x008 / STOP 0x014 /
/// SUSPEND 0x01C / RESUME 0x020, EVENTS_STOPPED 0x104 / ERROR 0x124 /
/// SUSPENDED 0x148 / RXSTARTED 0x14C / TXSTARTED 0x150 / LASTRX 0x15C /
/// LASTTX 0x160, SHORTS 0x200 (LASTTX_STARTRX 7, LASTTX_SUSPEND 8,
/// LASTTX_STOP 9, LASTRX_STARTTX 10, LASTRX_SUSPEND 11, LASTRX_STOP 12),
/// INTENSET 0x304 (STOPPED 1, ERROR 9, SUSPENDED 18, RXSTARTED 19,
/// TXSTARTED 20, LASTRX 23, LASTTX 24), ERRORSRC 0x4C4 (OVERRUN 0,
/// ANACK 1, DNACK 2), ENABLE 0x500, ADDRESS 0x588,
/// RXD.PTR 0x534 / MAXCNT 0x538 / AMOUNT 0x53C,
/// TXD.PTR 0x544 / MAXCNT 0x548 / AMOUNT 0x54C,
/// TXD byte 0x51C / RXD byte 0x518 (polling path).
/// DMA rule: START with MAXCNT>0 stages a driver transfer (take_* ->
/// mem move -> complete_*). The polling path (MAXCNT==0) completes
/// inline against the taps. NACK: a DMA transfer with no tap slave at
/// ADDRESS fails after the address phase (~6000 instr) with
/// ERROR + ANACK + STOPPED, exactly like silicon without an ACK.
/// (nrfx always uses DMA, so MicroPython's accel probe hangs without it.)
pub struct Twim {
    name: String,
    irq: i32,
    enable: u32,
    address: u8,
    errorsrc: u32,
    shorts: u32,
    ev_stopped: bool,
    ev_error: bool,
    ev_suspended: bool,
    ev_rxstarted: bool,
    ev_txstarted: bool,
    ev_lastrx: bool,
    ev_lasttx: bool,
    ev_endrx: bool,
    ev_endtx: bool,
    tx_byte: u8,
    intenset: u32,
    suspended: bool,
    started_tx: bool,
    started_rx: bool,
    tx_ptr: u32,
    tx_maxcnt: u32,
    tx_amount: u32,
    tx_pending: bool,
    rx_ptr: u32,
    rx_maxcnt: u32,
    rx_amount: u32,
    rx_pending: bool,
    nack_at: Option<u64>,
}

impl Twim {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        // SERIAL0/1 share one base each (TWIM/SPIM/SPIS/TWIS aliases);
        // one slot per base, ENABLE selects the mode (emulator is mode-blind).
        // IRQ ground truth (nrf52833.svd): SERIAL0=3, SERIAL1=4.
        let irq = match name {
            "TWIM0" | "TWI0" | "SPIM0" | "SPIS0" | "TWIS0" | "SPI0"
            | "SPIM0_SPIS0_TWIM0_TWIS0" => 3,
            "TWIM1" | "TWI1" | "SPIM1" | "SPIS1" | "TWIS1" | "SPI1" => 4,
            "SPIM2" | "SPIS2" | "SPI2" => 35,
            "SPIM3" => 47,
            _ => return None,
        };
        Some(Box::new(Self {
            name: name.to_string(), irq, enable: 0, address: 0, errorsrc: 0, shorts: 0,
            ev_stopped: false, ev_error: false, ev_suspended: false,
            ev_rxstarted: false, ev_txstarted: false,
            ev_lastrx: false, ev_lasttx: false,
            ev_endrx: false, ev_endtx: false,
            tx_byte: 0, intenset: 0, suspended: false,
            started_tx: false, started_rx: false,
            tx_ptr: 0, tx_maxcnt: 0, tx_amount: 0, tx_pending: false,
            rx_ptr: 0, rx_maxcnt: 0, rx_amount: 0, rx_pending: false,
            nack_at: None,
        }))
    }
    fn fire(&self, sys: &System, bit: u32) {
        if self.intenset & bit != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(self.irq);
        }
    }
    /// True when a tap slave answers at the current ADDRESS on this bus.
    fn slave_present(&self) -> bool {
        for tap in crate::system::get_ext_devices().lock().unwrap().i2c_taps.iter() {
            let t = tap.borrow();
            if t.config.peripheral == self.name && t.config.address == self.address {
                return true;
            }
        }
        false
    }
    /// NACK deadline poll: address phase with no ACK fails the transfer.
    /// Runs on every access + tick (same lazy rule as the counters).
    fn poll_nack(&mut self, sys: &System) {
        if let Some(deadline) = self.nack_at {
            if crate::system::instruction_count() >= deadline {
                self.nack_at = None;
                self.tx_pending = false;
                self.rx_pending = false;
                self.started_tx = false;
                self.started_rx = false;
                self.ev_error = true;
                self.errorsrc |= 1 << 1; // ANACK
                self.ev_stopped = true;
                self.fire(sys, 1 << 9);
                self.fire(sys, 1 << 1);
            }
        }
    }
    fn arm_nack(&mut self) {
        // Address phase at 100 kHz ~= 90 us ~= ~6000 core instructions.
        if !self.slave_present() {
            self.nack_at = Some(crate::system::instruction_count().wrapping_add(6000));
        } else {
            self.nack_at = None;
        }
    }
}

impl Peripheral for Twim {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, sys: &System, offset: u32) -> u32 {
        self.poll_nack(sys);
        match offset {
            0x104 => self.ev_stopped as u32,
            0x10C => self.ev_endrx as u32,
            0x120 => self.ev_endtx as u32,
            0x124 => self.ev_error as u32,
            0x148 => self.ev_suspended as u32,
            0x14C => self.ev_rxstarted as u32,
            0x150 => self.ev_txstarted as u32,
            0x15C => self.ev_lastrx as u32,
            0x160 => self.ev_lasttx as u32,
            0x200 => self.shorts,
            0x304 => self.intenset,
            0x4C4 => self.errorsrc,
            0x500 => self.enable,
            0x518 => crate::system::i2c_tap_rx_pop(&self.name) as u32,
            0x534 => self.rx_ptr,
            0x538 => self.rx_maxcnt,
            0x53C => self.rx_amount,
            0x544 => self.tx_ptr,
            0x548 => self.tx_maxcnt,
            0x54C => self.tx_amount,
            0x588 => self.address as u32,
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        self.poll_nack(sys);
        match offset {
            0x000 => { // STARTRX
                self.started_rx = true;
                self.ev_rxstarted = true;
                self.fire(sys, 1 << 19);
                self.ev_endrx = false;
                self.rx_amount = 0;
                self.rx_pending = self.rx_maxcnt > 0;
                self.arm_nack();
            }
            0x008 => { // STARTTX
                self.started_tx = true;
                self.ev_txstarted = true;
                self.fire(sys, 1 << 20);
                self.ev_stopped = false;
                self.ev_endtx = false;
                self.tx_amount = 0;
                self.tx_pending = self.tx_maxcnt > 0;
                self.arm_nack();
            }
            0x010 => { self.started_tx = true; self.ev_stopped = false; } // SPIM TASKS_START
            0x014 => { // STOP -> STOPPED event
                self.started_tx = false;
                self.started_rx = false;
                self.suspended = false;
                self.tx_pending = false;
                self.rx_pending = false;
                self.nack_at = None;
                self.ev_stopped = true;
                crate::system::i2c_tap_push_event(&self.name, 1 << 31); // STOP boundary
                self.fire(sys, 1 << 1);
            }
            0x01C => { // SUSPEND
                self.suspended = true;
                self.ev_suspended = true;
                self.fire(sys, 1 << 18);
            }
            0x020 => { self.suspended = false; } // RESUME
            0x104 => if value == 0 { self.ev_stopped = false; }
            0x10C => if value == 0 { self.ev_endrx = false; }
            0x120 => if value == 0 { self.ev_endtx = false; }
            0x124 => if value == 0 { self.ev_error = false; }
            0x148 => if value == 0 { self.ev_suspended = false; }
            0x14C => if value == 0 { self.ev_rxstarted = false; }
            0x150 => if value == 0 { self.ev_txstarted = false; }
            0x15C => if value == 0 { self.ev_lastrx = false; }
            0x160 => if value == 0 { self.ev_lasttx = false; }
            0x200 => self.shorts = value & 0x1F80,
            0x304 => self.intenset |= value,
            0x308 => self.intenset &= !value,
            0x4C4 => self.errorsrc &= !value, // write-1-clears
            0x500 => self.enable = value & 0xF,
            0x51C => {
                // TXD byte -> tapped I2C slave matching ADDRESS. SPIM-only
                // instances (SPIM2/3) skip the I2C tap (TODO P8: route them
                // to spi_taps with CS/DC like the JS display layer expects).
                self.tx_byte = (value & 0xFF) as u8;
                if self.name.starts_with("TWI") {
                    crate::system::i2c_tap_push_tx(&self.name, self.tx_byte);
                }
            }
            0x588 => self.address = (value & 0x7F) as u8,
            0x534 => self.rx_ptr = value,
            0x538 => self.rx_maxcnt = value & 0xFF,
            0x544 => self.tx_ptr = value,
            0x548 => self.tx_maxcnt = value & 0xFF,
            _ => {}
        }
    }
    fn tick(&mut self, sys: &System) { self.poll_nack(sys); }
}

/// Driver-side EASYDMA for one SERIAL slot (base address).
fn with_twim<R>(sys: &System, base: u32, f: impl FnOnce(&mut Twim) -> R) -> Option<R> {
    for slot in &sys.p.peripherals {
        if slot.start == base {
            let mut b = slot.peripheral.borrow_mut();
            if let Some(t) = b.as_any_mut().downcast_mut::<Twim>() {
                return Some(f(t));
            }
            return None;
        }
    }
    None
}

fn base_of(name: &str) -> Option<u32> {
    match name {
        "TWIM0" | "SPIM0" | "TWI0" | "SPI0" => Some(0x4000_3000),
        "TWIM1" | "SPIM1" | "TWI1" | "SPI1" => Some(0x4000_4000),
        "SPIM2" | "SPI2" => Some(0x4002_3000),
        "SPIM3" => Some(0x4002_F000),
        _ => None,
    }
}

/// Take a staged TX DMA transfer (addr, ptr, maxcnt); None when idle.
pub fn take_txdma(sys: &System, name: &str) -> Option<(u8, u32, u32)> {
    let base = base_of(name)?;
    with_twim(sys, base, |t| {
        if t.tx_pending {
            t.tx_pending = false;
            Some((t.address, t.tx_ptr, t.tx_maxcnt))
        } else {
            None
        }
    })
    .flatten()
}

/// Complete TX DMA: bytes go to the tapped slave, AMOUNT + LASTTX, then the
/// SHORTS chain (LASTTX_STOP / LASTTX_STARTRX / LASTTX_SUSPEND), each with
/// its INTEN IRQ (LASTTX 24, STOPPED 1 — SVD ground truth).
pub fn complete_txdma(sys: &System, name: &str, data: &[u8]) {
    let Some(base) = base_of(name) else { return };
    let fire = with_twim(sys, base, |t| {
        for &b in data {
            if t.name.starts_with("TWI") {
                crate::system::i2c_tap_push_tx(&t.name, b);
            }
        }
        t.tx_amount = data.len() as u32;
        t.nack_at = None;
        t.ev_endtx = true;
        t.ev_lasttx = true;
        let mut fire: u32 = 1 << 24;
        if t.shorts & (1 << 9) != 0 {
            // LASTTX_STOP
            t.started_tx = false;
            t.ev_stopped = true;
            fire |= 1 << 1;
        }
        if t.shorts & (1 << 7) != 0 {
            // LASTTX_STARTRX
            t.started_rx = true;
            t.ev_rxstarted = true;
            t.rx_amount = 0;
            t.rx_pending = t.rx_maxcnt > 0;
            fire |= 1 << 19;
        }
        if t.shorts & (1 << 8) != 0 {
            // LASTTX_SUSPEND
            t.suspended = true;
            t.ev_suspended = true;
            fire |= 1 << 18;
        }
        (t.irq, t.intenset, fire)
    });
    if let Some((irq, en, fire)) = fire {
        if en & fire != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(irq);
        }
    }
}

/// Take a staged RX DMA transfer (addr, ptr, maxcnt); None when idle.
pub fn take_rxdma(sys: &System, name: &str) -> Option<(u8, u32, u32)> {
    let base = base_of(name)?;
    with_twim(sys, base, |t| {
        if t.rx_pending {
            t.rx_pending = false;
            Some((t.address, t.rx_ptr, t.rx_maxcnt))
        } else {
            None
        }
    })
    .flatten()
}

/// Complete RX DMA: driver already wrote `amount` bytes to RAM at PTR.
/// Sets LASTRX + ENDRX, then the SHORTS chain (LASTRX_STOP / LASTRX_STARTTX
/// / LASTRX_SUSPEND), with INTEN IRQs (LASTRX 23, STOPPED 1).
pub fn complete_rxdma(sys: &System, name: &str, amount: u32) {
    let Some(base) = base_of(name) else { return };
    let fire = with_twim(sys, base, |t| {
        t.rx_amount = amount;
        t.nack_at = None;
        t.ev_endrx = true;
        t.ev_lastrx = true;
        let mut fire: u32 = 1 << 23;
        if t.shorts & (1 << 12) != 0 {
            // LASTRX_STOP
            t.started_rx = false;
            t.ev_stopped = true;
            fire |= 1 << 1;
        }
        if t.shorts & (1 << 10) != 0 {
            // LASTRX_STARTTX
            t.started_tx = true;
            t.ev_txstarted = true;
            t.tx_amount = 0;
            t.tx_pending = t.tx_maxcnt > 0;
            fire |= 1 << 20;
        }
        if t.shorts & (1 << 11) != 0 {
            // LASTRX_SUSPEND
            t.suspended = true;
            t.ev_suspended = true;
            fire |= 1 << 18;
        }
        (t.irq, t.intenset, fire)
    });
    if let Some((irq, en, fire)) = fire {
        if en & fire != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(irq);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn tx_byte_reaches_tap_and_stop_events() {
        let sys = test_dummy_system();
        // Own tap queue (TWI0 alias): TWIM0 is used by the sensors firmware
        // test and TWIM1 by the DMA test (global per-name queues).
        let mut t = Twim::new("TWI0").unwrap();
        t.write(&sys, 0x500, 6); // ENABLE
        t.write(&sys, 0x588, 0x19); // LSM303 accel addr
        t.write(&sys, 0x008, 1); // STARTTX
        t.write(&sys, 0x51C, 0x28); // OUT_X_L register
        t.write(&sys, 0x014, 1); // STOP
        assert_eq!(t.read(&sys, 0x104), 1, "STOPPED");
        let ev = crate::system::i2c_tap_take_tx("TWI0");
        assert!(ev.contains(&0x28), "tap got byte, got {ev:?}");
        // 2nd run: fresh instance, no leak
        let mut t2 = Twim::new("TWI0").unwrap();
        assert_eq!(t2.read(&sys, 0x104), 0);
    }
    #[test]
    fn tx_dma_roundtrip_to_tap() {
        use crate::system::test_dummy_system;
        let sys = test_dummy_system();
        sys.p.write(&sys, 0x40004544, 4, 0x20001000); // TXD.PTR
        sys.p.write(&sys, 0x40004548, 4, 2);          // TXD.MAXCNT
        sys.p.write(&sys, 0x40004588, 4, 0x19);       // ADDRESS
        sys.p.write(&sys, 0x40004008, 4, 1);          // STARTTX
        assert_eq!(sys.p.read(&sys, 0x40004120, 4), 0, "ENDTX waits for driver");
        let t = take_txdma(&sys, "TWIM1").expect("staged");
        assert_eq!(t, (0x19, 0x20001000, 2));
        complete_txdma(&sys, "TWIM1", &[0x2A, 0x00]);
        assert_eq!(sys.p.read(&sys, 0x40004120, 4), 1, "ENDTX after complete");
        let ev = crate::system::i2c_tap_take_tx("TWIM1");
        assert!(ev.contains(&0x2A), "tap got DMA bytes, got {ev:?}");
    }
    #[test]
    fn rx_dma_roundtrip_from_driver() {
        use crate::system::test_dummy_system;
        let sys = test_dummy_system();
        sys.p.write(&sys, 0x40004534, 4, 0x20002000); // RXD.PTR
        sys.p.write(&sys, 0x40004538, 4, 6);          // RXD.MAXCNT
        sys.p.write(&sys, 0x40004588, 4, 0x19);       // ADDRESS
        sys.p.write(&sys, 0x40004000, 4, 1);          // STARTRX
        let t = take_rxdma(&sys, "TWIM1").expect("staged");
        assert_eq!(t, (0x19, 0x20002000, 6));
        complete_rxdma(&sys, "TWIM1", 6);
        assert_eq!(sys.p.read(&sys, 0x4000410C, 4), 1, "ENDRX after complete");
        assert_eq!(sys.p.read(&sys, 0x4000453C, 4), 6, "AMOUNT");
    }
    #[test]
    fn tx_completion_irq_when_enabled() {
        use crate::system::test_dummy_system;
        let sys = test_dummy_system();
        sys.p.write(&sys, 0xE000E100, 4, 1 << 4); // NVIC ISER: SERIAL1
        sys.p.write(&sys, 0x40004304, 4, 1 << 24); // INTEN: LASTTX
        sys.p.write(&sys, 0x40004544, 4, 0x20001000);
        sys.p.write(&sys, 0x40004548, 4, 1);
        sys.p.write(&sys, 0x40004008, 4, 1); // STARTTX
        complete_txdma(&sys, "TWIM1", &[0x55]);
        assert!(sys.p.nvic.borrow().has_pending(), "LASTTX IRQ pends");
        assert_eq!(sys.p.read(&sys, 0x40004160, 4), 1, "LASTTX event set");
    }
    #[test]
    fn nack_without_slave_sets_error_and_stopped() {
        use crate::system::test_dummy_system;
        let sys = test_dummy_system();
        // No tap slave registered for TWIM1/0x19: address phase NACKs.
        sys.p.write(&sys, 0x40004588, 4, 0x19); // ADDRESS
        sys.p.write(&sys, 0x40004548, 4, 2);    // TXD.MAXCNT (DMA path)
        sys.p.write(&sys, 0x40004008, 4, 1);    // STARTTX
        assert_eq!(sys.p.read(&sys, 0x40004124, 4), 0, "no error yet");
        crate::system::INSTRUCTION_COUNT.fetch_add(6000, std::sync::atomic::Ordering::Relaxed);
        sys.tick(); // NACK deadline passes
        assert_eq!(sys.p.read(&sys, 0x40004124, 4), 1, "ERROR set");
        assert_eq!(sys.p.read(&sys, 0x400044C4, 4) & (1 << 1), 1 << 1, "ANACK cause");
        assert_eq!(sys.p.read(&sys, 0x40004104, 4), 1, "STOPPED set");
        // take finds nothing: the transfer died on the bus.
        assert!(take_txdma(&sys, "TWIM1").is_none());
    }
}
