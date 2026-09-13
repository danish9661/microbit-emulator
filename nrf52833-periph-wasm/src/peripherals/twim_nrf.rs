use crate::system::System;
use crate::cpu::mem::Memory;
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
    // Slave state (TWIS ENABLE=9 / SPIS ENABLE=2 select the slave map
    // on the shared SERIAL base; all other ENABLE values run master).
    slv_addr: [u8; 2],
    slv_cfg: u32,
    slv_orc: u8,
    slv_match: u32,
    ev_twis_write: bool,
    ev_twis_read: bool,
    spis_acquired: bool,
    spis_def: u8,
    spis_orc: u8,
    spis_config: u32,
    ev_spis_end: bool,
    ev_spis_endrx: bool,
    ev_spis_acquired: bool,
    psel: [u32; 4],
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
            slv_addr: [0; 2], slv_cfg: 0, slv_orc: 0, slv_match: 0,
            ev_twis_write: false, ev_twis_read: false,
            spis_acquired: false, spis_def: 0, spis_orc: 0, spis_config: 0,
            ev_spis_end: false, ev_spis_endrx: false, ev_spis_acquired: false,
            psel: [0xFFFF_FFFF; 4],
        }))
    }
    /// Slave map selector: TWIS ENABLE=9, SPIS ENABLE=2 (SVD).
    /// Everything else (6/7/0) runs the master map (legacy default).
    fn is_twis(&self) -> bool {
        self.enable == 9
    }
    fn is_spis(&self) -> bool {
        self.enable == 2
    }
    fn fire(&self, sys: &System, bit: u32) {
        if self.intenset & bit != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(self.irq);
        }
    }
    /// True when a tap slave answers at the current ADDRESS on this bus.
    /// nrfx writes ADDRESS shifted (addr<<1: tap table is 7-bit), so
    /// match shifted>>1 too — 0x72 finds the 0x39 slave.
    /// The >>1 fallback can alias (0x19 == 0x33>>1: the SSD1306 0x3C
    /// tap is a real example), so prefer exact first; tests needing a
    /// guaranteed-empty bus must hold lock_i2c_tap().
    fn slave_present(&self) -> bool {
        let taps = crate::system::get_ext_devices().lock().unwrap();
        if taps.i2c_taps.iter().any(|tap| {
            let t = tap.borrow();
            t.config.peripheral == self.name && t.config.address == self.address
        }) {
            return true;
        }
        taps.i2c_taps.iter().any(|tap| {
            let t = tap.borrow();
            t.config.peripheral == self.name && t.config.address == (self.address >> 1)
        })
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
        if self.is_twis() {
            return self.twis_read(sys, offset);
        }
        if self.is_spis() {
            return self.spis_read(sys, offset);
        }
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
            0x518 => {
                // RXD polling path: master-mode reads return the slave's
                // response line (MISO for SPI, SDA for I2C); 0xFF idle.
                if self.name.starts_with("SPI") {
                    crate::system::spi_tap_miso_pop(&self.name) as u32
                } else {
                    crate::system::i2c_tap_rx_pop(&self.name) as u32
                }
            }
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
        if self.is_twis() {
            self.twis_write(sys, offset, value);
            return;
        }
        if self.is_spis() {
            self.spis_write(sys, offset, value);
            return;
        }
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
                // START boundary carries the 7-bit slave address in bits
                // 6..0 (boundary detection via bits 31/30 is unaffected).
                crate::system::i2c_tap_push_event(&self.name, (1 << 31) | (1 << 30) | (self.address as u32 & 0x7F));
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
                crate::system::i2c_tap_push_event(&self.name, (1 << 31) | (1 << 30) | (self.address as u32 & 0x7F));
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
                // TXD byte -> tapped slave. I2C slaves match ADDRESS;
                // SPIM-only instances (SPIM2/3) route register-mode bytes
                // and DMA frames to spi_taps (CS/DC handled JS-side, like
                // the display layer expects).
                self.tx_byte = (value & 0xFF) as u8;
                if self.name.starts_with("TWI") {
                    crate::system::i2c_tap_push_tx(&self.name, self.tx_byte);
                } else if self.name.starts_with("SPI") {
                    // SPIM register-mode bytes are observable to slave
                    // parts here (DMA bytes travel via take/complete).
                    crate::system::spi_tap_push_byte(&self.name, self.tx_byte as u32);
                }
            }
            0x588 => {
                // ADDRESS register: keep the raw firmware value (nrfx
                // writes the 8-bit shifted form, e.g. 0x72 for 7-bit
                // 0x39). slave_present() matches both forms against the
                // 7-bit tap table, so no normalization here (silicon
                // readback is the written value).
                self.address = (value & 0xFF) as u8;
            }
            0x534 => self.rx_ptr = value,
            0x538 => self.rx_maxcnt = value & 0xFF,
            0x544 => self.tx_ptr = value,
            0x548 => self.tx_maxcnt = value & 0xFF,
            _ => {}
        }
    }
    fn tick(&mut self, sys: &System) {
        if !self.is_twis() && !self.is_spis() {
            self.poll_nack(sys);
        }
    }
}

impl Twim {
    // ---- TWIS slave map (ENABLE=9) ----
    fn twis_read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x104 => self.ev_stopped as u32,
            0x124 => self.ev_error as u32,
            0x14C => self.ev_rxstarted as u32,
            0x150 => self.ev_txstarted as u32,
            0x164 => self.ev_twis_write as u32,
            0x168 => self.ev_twis_read as u32,
            0x200 => self.shorts,
            0x304 => self.intenset,
            0x4D0 => self.errorsrc,
            0x4D4 => self.slv_match,
            0x500 => self.enable,
            0x508 | 0x50C => self.psel[((offset - 0x508) >> 2) as usize % 4],
            0x534 => self.rx_ptr,
            0x538 => self.rx_maxcnt,
            0x53C => self.rx_amount,
            0x544 => self.tx_ptr,
            0x548 => self.tx_maxcnt,
            0x54C => self.tx_amount,
            0x588 => self.slv_addr[0] as u32,
            0x58C => self.slv_addr[1] as u32,
            0x594 => self.slv_cfg,
            0x5C0 => self.slv_orc as u32,
            _ => 0,
        }
    }
    fn twis_write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x014 => {
                // STOP: end transaction.
                self.started_tx = false;
                self.started_rx = false;
                self.rx_pending = false;
                self.tx_pending = false;
                self.ev_stopped = true;
                self.fire(sys, 1 << 1);
            }
            0x01C => {
                self.suspended = true;
                self.ev_suspended = true;
                self.fire(sys, 1 << 18);
            }
            0x020 => self.suspended = false,
            0x030 => {
                // PREPARERX: arm the RX buffer.
                self.rx_amount = 0;
                self.rx_pending = true;
            }
            0x034 => {
                // PREPARETX: arm the TX buffer.
                self.tx_amount = 0;
                self.tx_pending = true;
            }
            0x104 => if value == 0 { self.ev_stopped = false; }
            0x124 => if value == 0 { self.ev_error = false; }
            0x14C => if value == 0 { self.ev_rxstarted = false; }
            0x150 => if value == 0 { self.ev_txstarted = false; }
            0x164 => if value == 0 { self.ev_twis_write = false; }
            0x168 => if value == 0 { self.ev_twis_read = false; }
            0x200 => self.shorts = value & 0x6000,
            0x304 => self.intenset |= value & 0x0618_0202,
            0x308 => self.intenset &= !value,
            0x4D0 => self.errorsrc &= !value, // write-1-clears
            0x500 => self.enable = value & 0xF,
            0x508 | 0x50C => self.psel[((offset - 0x508) >> 2) as usize % 4] = value,
            0x534 => self.rx_ptr = value,
            0x538 => self.rx_maxcnt = value & 0xFF,
            0x544 => self.tx_ptr = value,
            0x548 => self.tx_maxcnt = value & 0xFF,
            0x588 => self.slv_addr[0] = (value & 0x7F) as u8,
            0x58C => self.slv_addr[1] = (value & 0x7F) as u8,
            0x594 => self.slv_cfg = value & 3,
            0x5C0 => self.slv_orc = (value & 0xFF) as u8,
            _ => {}
        }
    }
    /// Address match against ADDRESS[0/1] gated by CONFIG bits.
    fn twis_match(&self, addr7: u8) -> Option<u32> {
        if self.slv_cfg & 1 != 0 && addr7 == self.slv_addr[0] {
            Some(0)
        } else if self.slv_cfg & 2 != 0 && addr7 == self.slv_addr[1] {
            Some(1)
        } else {
            None
        }
    }
    // ---- SPIS slave map (ENABLE=2) ----
    fn spis_read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x104 => self.ev_spis_end as u32,
            0x110 => self.ev_spis_endrx as u32,
            0x128 => self.ev_spis_acquired as u32,
            0x200 => self.shorts,
            0x304 => self.intenset,
            0x500 => self.enable,
            0x508 | 0x50C | 0x510 | 0x514 => {
                self.psel[((offset - 0x508) >> 2) as usize % 4]
            }
            0x534 => self.rx_ptr,
            0x538 => self.rx_maxcnt,
            0x53C => self.rx_amount,
            0x544 => self.tx_ptr,
            0x548 => self.tx_maxcnt,
            0x54C => self.tx_amount,
            0x554 => self.spis_config,
            0x55C => self.spis_def as u32,
            0x5C0 => self.spis_orc as u32,
            _ => 0,
        }
    }
    fn spis_write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x024 => {
                // ACQUIRE: take the semaphore if free.
                if !self.spis_acquired {
                    self.spis_acquired = true;
                    self.ev_spis_acquired = true;
                    self.fire(sys, 1 << 10);
                }
            }
            0x028 => self.spis_acquired = false, // RELEASE
            0x104 => if value == 0 { self.ev_spis_end = false; }
            0x110 => if value == 0 { self.ev_spis_endrx = false; }
            0x128 => if value == 0 { self.ev_spis_acquired = false; }
            0x200 => self.shorts = value & 4,
            0x304 => self.intenset |= value & 0x412,
            0x308 => self.intenset &= !value,
            0x500 => self.enable = value & 0xF,
            0x508 | 0x50C | 0x510 | 0x514 => {
                self.psel[((offset - 0x508) >> 2) as usize % 4] = value;
            }
            0x534 => self.rx_ptr = value,
            0x538 => self.rx_maxcnt = value & 0xFF,
            0x544 => self.tx_ptr = value,
            0x548 => self.tx_maxcnt = value & 0xFF,
            0x554 => self.spis_config = value & 7,
            0x55C => self.spis_def = (value & 0xFF) as u8,
            0x5C0 => self.spis_orc = (value & 0xFF) as u8,
            _ => {}
        }
    }
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
            t.nack_at = None; // driver owns it now: no bus-error timeout
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
            } else if t.name.starts_with("SPI") {
                crate::system::spi_tap_push_byte(&t.name, b as u32);
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
            t.nack_at = None;
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
    use crate::cpu::mem::{FlatMemory, Memory};
    #[test]
    fn twis_address_match_write_read() {
        let sys = test_dummy_system();
        sys.p.write(&sys, 0xE000E100, 4, 1 << 4); // NVIC ISER: SERIAL1
        sys.p.write(&sys, 0x40004304, 4, (1 << 1) | (1 << 19) | (1 << 25)); // INTEN STOPPED/RXSTARTED/WRITE
        sys.p.write(&sys, 0x40004500, 4, 9); // TWIS ENABLE
        sys.p.write(&sys, 0x40004588, 4, 0x42); // ADDRESS[0]
        sys.p.write(&sys, 0x40004594, 4, 1); // CONFIG: ADDRESS0
        let mut mem = FlatMemory::new(512 * 1024, 128 * 1024);
        // Wrong address: DNACK, nothing accepted.
        sys.p.write(&sys, 0x40004534, 4, 0x20001000);
        sys.p.write(&sys, 0x40004538, 4, 8);
        sys.p.write(&sys, 0x40004030, 4, 1); // PREPARERX
        assert_eq!(twis_master_write(&sys, &mut mem, 0x40004000, 0x43, &[1, 2]), 0, "NACK");
        assert_eq!(sys.p.read(&sys, 0x400044D0, 4) & (1 << 2), 1 << 2, "DNACK cause");
        assert_eq!(sys.p.read(&sys, 0x40004104, 4), 1, "STOPPED");
        // Right address: bytes land in RAM, events fire.
        assert_eq!(twis_master_write(&sys, &mut mem, 0x40004000, 0x42, &[1, 2, 3]), 3);
        assert_eq!(sys.p.read(&sys, 0x400044D4, 4), 0, "MATCH index 0");
        assert_eq!(mem.read8(0x20001000), 1);
        assert_eq!(mem.read8(0x20001002), 3);
        assert_eq!(sys.p.read(&sys, 0x4000453C, 4), 3, "RX AMOUNT");
        assert_eq!(sys.p.read(&sys, 0x4000414C, 4), 1, "RXSTARTED");
        assert_eq!(sys.p.read(&sys, 0x40004164, 4), 1, "WRITE");
        assert!(sys.p.nvic.borrow().has_pending(), "IRQ 4 pends");
    }
    #[test]
    fn twis_read_orc_pad_and_overflows() {
        let sys = test_dummy_system();
        sys.p.write(&sys, 0x40004500, 4, 9);
        sys.p.write(&sys, 0x40004588, 4, 0x42);
        sys.p.write(&sys, 0x40004594, 4, 1);
        let mut mem = FlatMemory::new(512 * 1024, 128 * 1024);
        for (i, &b) in [9u8, 8, 7, 6].iter().enumerate() {
            mem.write8(0x20002000 + i as u32, b);
        }
        sys.p.write(&sys, 0x40004544, 4, 0x20002000); // TXD.PTR
        sys.p.write(&sys, 0x40004548, 4, 4); // TXD.MAXCNT
        sys.p.write(&sys, 0x400045C0, 4, 0xFF); // ORC
        sys.p.write(&sys, 0x40004034, 4, 1); // PREPARETX
        // 6 clocked bytes: 4 from RAM + 2 ORC pad.
        assert_eq!(twis_master_read(&sys, &mut mem, 0x40004000, 0x42, 6), vec![9, 8, 7, 6, 0xFF, 0xFF]);
        assert_eq!(sys.p.read(&sys, 0x40004150, 4), 1, "TXSTARTED");
        assert_eq!(sys.p.read(&sys, 0x40004168, 4), 1, "READ");
        // Unprepared RX is OVERFLOW; unprepared TX is OVERREAD.
        assert_eq!(twis_master_write(&sys, &mut mem, 0x40004000, 0x42, &[1]), 0, "no PREPARERX");
        assert_eq!(sys.p.read(&sys, 0x400044D0, 4) & 1, 1, "OVERFLOW cause");
        assert_eq!(twis_master_read(&sys, &mut mem, 0x40004000, 0x42, 2), vec![0xFF, 0xFF], "ORC clocked");
        assert_eq!(sys.p.read(&sys, 0x400044D0, 4) & (1 << 3), 1 << 3, "OVERREAD cause");
        // 2nd run: fresh default is master-mode (enable 0), slave idle.
        let t2 = Twim::new("TWIM1").unwrap();
        let mut b = Box::new(t2);
        let _ = b.as_any_mut();
    }
    #[test]
    fn spis_acquire_exchange_release() {
        let sys = test_dummy_system();
        sys.p.write(&sys, 0xE000E100, 4, 1 << 3); // NVIC ISER: SERIAL0
        sys.p.write(&sys, 0x40003304, 4, (1 << 1) | (1 << 4)); // INTEN END/ENDRX
        sys.p.write(&sys, 0x40003500, 4, 2); // SPIS ENABLE
        let mut mem = FlatMemory::new(512 * 1024, 128 * 1024);
        // No ACQUIRE: exchange refused.
        assert_eq!(spis_exchange(&sys, &mut mem, 0x40003000, &[1, 2]), Vec::<u8>::new());
        sys.p.write(&sys, 0x40003024, 4, 1); // ACQUIRE
        assert_eq!(sys.p.read(&sys, 0x40003128, 4), 1, "ACQUIRED");
        sys.p.write(&sys, 0x40003534, 4, 0x20001000); // RXD.PTR
        sys.p.write(&sys, 0x40003538, 4, 8); // RXD.MAXCNT
        for (i, &b) in [9u8, 8, 7, 6].iter().enumerate() {
            mem.write8(0x20002000 + i as u32, b);
        }
        sys.p.write(&sys, 0x40003544, 4, 0x20002000); // TXD.PTR
        sys.p.write(&sys, 0x40003548, 4, 4); // TXD.MAXCNT
        sys.p.write(&sys, 0x400035C0, 4, 0xEE); // ORC
        // 6 SCK bytes: 4 MISO from RAM + 2 ORC; MOSI lands in RX RAM.
        assert_eq!(
            spis_exchange(&sys, &mut mem, 0x40003000, &[1, 2, 3, 4, 5, 6]),
            vec![9, 8, 7, 6, 0xEE, 0xEE]
        );
        assert_eq!(mem.read8(0x20001000), 1);
        assert_eq!(mem.read8(0x20001003), 4);
        assert_eq!(sys.p.read(&sys, 0x40003104, 4), 1, "END");
        assert_eq!(sys.p.read(&sys, 0x40003110, 4), 1, "ENDRX");
        assert!(sys.p.nvic.borrow().has_pending(), "IRQ 3 pends");
        // Semaphore released at END: next frame needs a new ACQUIRE.
        assert_eq!(spis_exchange(&sys, &mut mem, 0x40003000, &[1]), Vec::<u8>::new(), "released");
        // END_ACQUIRE shorts keeps it armed across frames.
        sys.p.write(&sys, 0x40003200, 4, 1 << 2); // SHORTS END_ACQUIRE
        sys.p.write(&sys, 0x40003024, 4, 1);
        assert_eq!(spis_exchange(&sys, &mut mem, 0x40003000, &[7, 7]).len(), 2);
        assert_eq!(spis_exchange(&sys, &mut mem, 0x40003000, &[7, 7]).len(), 2, "still acquired");
        // DEF character when no TX buffer programmed.
        sys.p.write(&sys, 0x40003548, 4, 0); // TXD.MAXCNT=0
        sys.p.write(&sys, 0x4000355C, 4, 0xDD); // DEF
        assert_eq!(spis_exchange(&sys, &mut mem, 0x40003000, &[0]), vec![0xDD]);
    }
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
        // Take BEFORE any event read: the NACK deadline runs on the
        // process-global clock, so a model read here could let a
        // parallel test's clock advance trip the timeout first (flake).
        // take() itself never polls, making this order deterministic.
        let t = take_txdma(&sys, "TWIM1").expect("staged");
        assert_eq!(sys.p.read(&sys, 0x40004120, 4), 0, "ENDTX waits for driver");
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
    fn rxd_polling_reads_slave_response_line() {
        use crate::system::test_dummy_system;
        let sys = test_dummy_system();
        // SPIM register-mode RXD returns MISO bytes pushed by the slave.
        crate::system::spi_tap_miso_push("SPIM2", &[0xA5, 0x5A]);
        let mut s = Twim::new("SPIM2").unwrap();
        assert_eq!(s.read(&sys, 0x518), 0xA5, "first MISO byte");
        assert_eq!(s.read(&sys, 0x518), 0x5A, "second MISO byte");
        assert_eq!(s.read(&sys, 0x518), 0xFF, "idle when queue drains");
        // TWIM register-mode RXD still returns the I2C (SDA) queue.
        crate::system::i2c_tap_rx_push("TWIM0", &[0x33]);
        let mut t = Twim::new("TWIM0").unwrap();
        assert_eq!(t.read(&sys, 0x518), 0x33, "I2C queue preserved");
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
    fn address_matches_shifted_8bit_form() {
        // nrfx writes ADDRESS as the 8-bit shifted form (addr<<1:
        // 0x32/0x3C/0xE0/0xE4); the tap table registers 7-bit
        // (0x19/0x1E/0x70/0x39). slave_present() must accept both, and
        // the take/complete path must hand the driver the 7-bit form.
        // Browser-measured: TWIM1 ADDR 114 (0x72 = 0x39<<1) with
        // ERROR+ANACK and no slave match.
        use crate::system::test_dummy_system;
        let _t = crate::system::lock_i2c_tap();
        let sys = test_dummy_system();
        sys.p.write(&sys, 0x40004588, 4, 0x72); // shifted form of 0x39
        assert_eq!(sys.p.read(&sys, 0x40004588, 4), 0x72, "raw readback");
        sys.p.write(&sys, 0x40004588, 4, 0x19); // 7-bit stays as-is
        assert_eq!(sys.p.read(&sys, 0x40004588, 4), 0x19, "7-bit passthrough");
    }
    #[test]
    fn nack_without_slave_sets_error_and_stopped() {
        use crate::system::test_dummy_system;
        // 0x19 has no tap on TWIM1 in this test's view; the DMA/sensors
        // tests register TWIM0/0x19 (different bus, no alias). Lock the
        // tap table so no parallel test can add one mid-flight.
        let _t = crate::system::lock_i2c_tap();
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

fn with_twim_base<R>(sys: &System, base: u32, f: impl FnOnce(&mut Twim) -> R) -> Option<R> {
    with_twim(sys, base, f)
}

/// TWIS address match helper: returns the matched ADDRESS index, or
/// performs the DNACK path (ERROR + DNACK cause + STOPPED + IRQs) and
/// returns None.
fn twis_match_or_nack(sys: &System, t: &mut Twim, addr7: u8) -> Option<u32> {
    // Read fields first (borrow ends before fire() calls below reuse sys).
    let (m, irq, en) = match t.twis_match(addr7) {
        Some(m) => (Some(m), t.irq, t.intenset),
        None => (None, t.irq, t.intenset),
    };
    match m {
        Some(m) => {
            t.slv_match = m;
            Some(m)
        }
        None => {
            t.ev_error = true;
            t.errorsrc |= 1 << 2; // DNACK
            t.ev_stopped = true;
            if en & (1 << 9) != 0 {
                sys.p.nvic.borrow_mut().set_intr_pending(irq);
            }
            if en & (1 << 1) != 0 {
                sys.p.nvic.borrow_mut().set_intr_pending(irq);
            }
            None
        }
    }
}

/// External-master write to our TWIS slave: address match (else DNACK),
/// then bytes land in RAM at RXD.PTR (needs PREPARERX; unprepared RX is
/// OVERFLOW). Returns bytes accepted. Sets RXSTARTED + WRITE + STOPPED
/// (+IRQs); SHORTS WRITE_SUSPEND suspends instead of stopping.
pub fn twis_master_write(
    sys: &System,
    mem: &mut dyn Memory,
    base: u32,
    addr7: u8,
    data: &[u8],
) -> u32 {
    let Some(r) = with_twim(sys, base, |t| {
        if !t.is_twis() {
            return None;
        }
        twis_match_or_nack(sys, t, addr7)?;
        if !t.rx_pending {
            t.ev_error = true;
            t.errorsrc |= 1 << 0; // OVERFLOW
            t.ev_stopped = true;
            let (irq, en) = (t.irq, t.intenset);
            if en & (1 << 9) != 0 {
                sys.p.nvic.borrow_mut().set_intr_pending(irq);
            }
            if en & (1 << 1) != 0 {
                sys.p.nvic.borrow_mut().set_intr_pending(irq);
            }
            return None;
        }
        let n = data.len().min(t.rx_maxcnt as usize);
        for (i, &b) in data[..n].iter().enumerate() {
            mem.write8(t.rx_ptr.wrapping_add(i as u32), b);
        }
        t.rx_amount = n as u32;
        t.rx_pending = false;
        t.ev_rxstarted = true;
        t.ev_twis_write = true;
        t.ev_stopped = true;
        let fire: u32 = (1 << 19) | (1 << 25) | (1 << 1);
        let (irq, en) = (t.irq, t.intenset);
        if t.shorts & (1 << 13) != 0 {
            // WRITE_SUSPEND instead of STOPPED.
            t.ev_stopped = false;
            t.suspended = true;
            t.ev_suspended = true;
            if en & (1 << 18) != 0 {
                sys.p.nvic.borrow_mut().set_intr_pending(irq);
            }
        }
        if en & fire != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(irq);
        }
        Some(n as u32)
    }) else {
        return 0;
    };
    r.unwrap_or(0)
}

/// External-master read from our TWIS slave: address match (else DNACK),
/// then bytes come from RAM at TXD.PTR (needs PREPARETX; unprepared TX
/// is OVERREAD and clocks out ORC). Short reads pad with ORC.
pub fn twis_master_read(
    sys: &System,
    mem: &mut dyn Memory,
    base: u32,
    addr7: u8,
    len: u32,
) -> Vec<u8> {
    let Some(r) = with_twim(sys, base, |t| {
        if !t.is_twis() {
            return None;
        }
        twis_match_or_nack(sys, t, addr7)?;
        if !t.tx_pending {
            t.ev_error = true;
            t.errorsrc |= 1 << 3; // OVERREAD
            t.ev_stopped = true;
            let (irq, en) = (t.irq, t.intenset);
            if en & (1 << 9) != 0 {
                sys.p.nvic.borrow_mut().set_intr_pending(irq);
            }
            if en & (1 << 1) != 0 {
                sys.p.nvic.borrow_mut().set_intr_pending(irq);
            }
            return Some(vec![t.slv_orc; len.min(256) as usize]);
        }
        let n = (len as usize).min(t.tx_maxcnt as usize);
        let mut out = Vec::with_capacity(len as usize);
        for i in 0..n {
            out.push(mem.read8(t.tx_ptr.wrapping_add(i as u32)));
        }
        while out.len() < len.min(256) as usize {
            out.push(t.slv_orc);
        }
        t.tx_amount = n as u32;
        t.tx_pending = false;
        t.ev_txstarted = true;
        t.ev_twis_read = true;
        t.ev_stopped = true;
        let fire: u32 = (1 << 20) | (1 << 26) | (1 << 1);
        let (irq, en) = (t.irq, t.intenset);
        if t.shorts & (1 << 14) != 0 {
            // READ_SUSPEND instead of STOPPED.
            t.ev_stopped = false;
            t.suspended = true;
            t.ev_suspended = true;
            if en & (1 << 18) != 0 {
                sys.p.nvic.borrow_mut().set_intr_pending(irq);
            }
        }
        if en & fire != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(irq);
        }
        Some(out)
    }) else {
        return Vec::new();
    };
    r.unwrap_or_default()
}

/// External-master SPI exchange with our SPIS slave. Requires the
/// ACQUIRE semaphore and programmed RXD/TXD; otherwise returns empty
/// (documented: the master must ACQUIRE first, the CSN protocol).
/// RX bytes land in RAM (up to RXD.MAXCNT); TX bytes come from RAM
/// (up to TXD.MAXCNT), padded with ORC past TX data, or DEF when no
/// TX buffer is programmed. Sets END + ENDRX (+IRQs); SHORTS
/// END_ACQUIRE re-acquires automatically.
pub fn spis_exchange(
    sys: &System,
    mem: &mut dyn Memory,
    base: u32,
    mosi: &[u8],
) -> Vec<u8> {
    let Some(r) = with_twim(sys, base, |t| {
        if !t.is_spis() || !t.spis_acquired {
            return None;
        }
        let n = mosi.len().min(256);
        let rx_n = n.min(t.rx_maxcnt as usize);
        for (i, &b) in mosi[..rx_n].iter().enumerate() {
            mem.write8(t.rx_ptr.wrapping_add(i as u32), b);
        }
        t.rx_amount = rx_n as u32;
        let tx_n = n.min(t.tx_maxcnt as usize);
        let mut miso = Vec::with_capacity(n);
        for i in 0..tx_n {
            miso.push(mem.read8(t.tx_ptr.wrapping_add(i as u32)));
        }
        let pad = if t.tx_maxcnt > 0 { t.spis_orc } else { t.spis_def };
        while miso.len() < n {
            miso.push(pad);
        }
        t.tx_amount = tx_n as u32;
        t.ev_spis_end = true;
        t.ev_spis_endrx = true;
        let (irq, en) = (t.irq, t.intenset);
        if en & ((1 << 1) | (1 << 4)) != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(irq);
        }
        if t.shorts & (1 << 2) != 0 {
            // END_ACQUIRE: stay acquired for the next frame.
        } else {
            // Semaphore releases at END unless re-acquired (silicon
            // holds it until RELEASE; we release here so a forgotten
            // RELEASE can't wedge the bus -- documented).
            t.spis_acquired = false;
        }
        Some(miso)
    }) else {
        return Vec::new();
    };
    r.unwrap_or_default()
}
