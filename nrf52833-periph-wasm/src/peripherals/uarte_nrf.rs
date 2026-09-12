use crate::system::{System, instruction_count, get_uart_output};
use super::Peripheral;

/// UARTE0 @ 0x40002000 (IRQ 2). Polling subset + EASYDMA:
///   ENABLE 0x500, BAUDRATE 0x524, TXD 0x51C (byte TX -> UART_OUTPUT),
///   RXD 0x518 (byte RX), EVENTS_RXDRDY 0x108 / EVENTS_ENDRX 0x10C /
///   EVENTS_TXDRDY 0x11C / EVENTS_ENDTX 0x120 / EVENTS_ERROR 0x124,
///   TASKS_STARTRX 0x000 / TASKS_STOPRX 0x004 / TASKS_STARTTX 0x008 /
///   TASKS_STOPTX 0x00C, RXD.PTR 0x534 / MAXCNT 0x538 / AMOUNT 0x53C,
///   TXD.PTR 0x544 / MAXCNT 0x548 / AMOUNT 0x54C, INTENSET 0x304/CLR 0x308.
/// DMA rule: STARTTX with TXD.MAXCNT>0 stages a driver transfer
/// (take_txdma -> mem_read -> complete_txdma); MAXCNT==0 completes at once
/// (keeps polling firmware timing). Same for RX.
pub struct Uarte {
    irq: i32,
    enable: u32,
    baudrate: u32,
    ev_txdrdy: bool,
    ev_endtx: bool,
    ev_txstopped: bool,
    ev_rxdrdy: bool,
    ev_endrx: bool,
    ev_error: bool,
    errorsrc: u32,
    rxd: u8,
    intenset: u32,
    rx_ptr: u32,
    rx_maxcnt: u32,
    rx_amount: u32,
    rx_pending: bool,
    rx_taken: bool,
    tx_ptr: u32,
    tx_maxcnt: u32,
    tx_amount: u32,
    tx_pending: bool,
    tx_taken: bool,
}

impl Default for Uarte {
    fn default() -> Self {
        Self { irq: 2, enable: 0, baudrate: 0, ev_txdrdy: false, ev_endtx: false,
               ev_txstopped: false,
               ev_rxdrdy: false, ev_endrx: false, ev_error: false, errorsrc: 0, rxd: 0,
               intenset: 0, rx_ptr: 0, rx_maxcnt: 0, rx_amount: 0, rx_pending: false, rx_taken: false,
               tx_ptr: 0, tx_maxcnt: 0, tx_amount: 0, tx_pending: false, tx_taken: false }
    }
}

impl Uarte {
    pub fn new(name: &str) -> Option<Box<dyn Peripheral>> {
        let irq = match name {
            "UARTE0" | "UART0" | "UARTE0_UART0" => 2,
            "UARTE1" => 40,
            _ => return None,
        };
        Some(Box::new(Self { irq, ..Self::default() }))
    }
    fn fire(&self, sys: &System, ev_bit: u32) {
        if self.intenset & ev_bit != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(self.irq);
        }
    }
}

impl Peripheral for Uarte {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn read(&mut self, _sys: &System, offset: u32) -> u32 {
        match offset {
            0x108 => self.ev_rxdrdy as u32,
            0x10C => self.ev_endrx as u32,
            0x11C => self.ev_txdrdy as u32,
            0x120 => self.ev_endtx as u32,
            0x158 => self.ev_txstopped as u32, // EVENTS_TXSTOPPED (SVD)
            0x124 => self.ev_error as u32,
            0x304 => self.intenset,
            0x480 => self.errorsrc,
            0x500 => self.enable,
            0x518 => self.rxd as u32,
            0x524 => self.baudrate,
            0x534 => self.rx_ptr,
            0x538 => self.rx_maxcnt,
            0x53C => self.rx_amount,
            0x544 => self.tx_ptr,
            0x548 => self.tx_maxcnt,
            0x54C => self.tx_amount,
            _ => 0,
        }
    }
    fn write(&mut self, sys: &System, offset: u32, value: u32) {
        match offset {
            0x000 => { // TASKS_STARTRX
                self.ev_endrx = false;
                self.rx_amount = 0;
                self.rx_pending = self.rx_maxcnt > 0;
            }
            0x004 => { self.rx_pending = false; self.ev_endrx = true; self.fire(sys, 1 << 4); }
            0x008 => { // TASKS_STARTTX
                self.ev_endtx = false;
                self.tx_amount = 0;
                if self.tx_maxcnt > 0 {
                    self.tx_pending = true; // driver completes (take/complete)
                } else {
                    self.ev_txdrdy = true;
                    self.ev_endtx = true;
                    self.fire(sys, 1 << 7);
                    self.fire(sys, 1 << 8);
                }
            }
            // TASKS_STOPTX aborts the transfer: TXSTOPPED only (silicon
            // never raises ENDTX here; doing so self-triggers an ENDTX
            // ISR loop -- MicroPython stalled exactly this way).
            0x00C => { self.tx_pending = false; self.ev_txstopped = true; self.fire(sys, 1 << 22); }
            0x108 => if value == 0 { self.ev_rxdrdy = false; }
            0x10C => if value == 0 { self.ev_endrx = false; }
            0x11C => if value == 0 { self.ev_txdrdy = false; }
            0x120 => if value == 0 { self.ev_endtx = false; }
            0x158 => if value == 0 { self.ev_txstopped = false; }
            0x124 => if value == 0 { self.ev_error = false; }
            0x304 => {
                self.intenset |= value;
                // re-fire any already-set event the firmware just enabled
                if self.ev_txdrdy && value & (1 << 7) != 0 { self.fire(sys, 1 << 7); }
                if self.ev_endtx && value & (1 << 8) != 0 { self.fire(sys, 1 << 8); }
                if self.ev_rxdrdy && value & (1 << 2) != 0 { self.fire(sys, 1 << 2); }
                if self.ev_txstopped && value & (1 << 22) != 0 { self.fire(sys, 1 << 22); }
            }
            0x308 => self.intenset &= !value,
            0x480 => self.errorsrc &= !value, // write-1-clears
            0x500 => self.enable = value & 0xF,
            0x518 => {} // RXD read-only
            0x51C => {
                // TXD byte: console lifeline
                let ch = (value & 0xFF) as u8;
                get_uart_output().lock().unwrap().push(ch as char);
                let _ = instruction_count();
                self.ev_txdrdy = true;
                self.ev_endtx = true;
                self.fire(sys, 1 << 7);
                self.fire(sys, 1 << 8);
            }
            0x524 => self.baudrate = value,
            0x534 => self.rx_ptr = value,
            0x538 => self.rx_maxcnt = value & 0xFF,
            0x544 => self.tx_ptr = value,
            0x548 => self.tx_maxcnt = value & 0xFF,
            _ => {}
        }
    }
    fn rx_byte(&mut self, sys: &System, byte: u8) {
        if self.rx_pending {
            // DMA RX accounting; the driver moves bytes into RAM and
            // completes (take/complete_rxdma). Count only, like silicon.
            self.rx_amount += 1;
            if self.rx_amount >= self.rx_maxcnt.max(1) {
                self.rx_pending = false;
                self.ev_endrx = true;
                self.fire(sys, 1 << 4);
            }
        }
        // RXD holds one byte: a second arrival before firmware reads is an
        // OVERRUN (real UARTE behavior, not a queue).
        if self.ev_rxdrdy {
            self.ev_error = true;
            self.errorsrc |= 1 << 0;
            self.fire(sys, 1 << 9);
        }
        self.rxd = byte;
        self.ev_rxdrdy = true;
        self.fire(sys, 1 << 2);
    }
}

/// Driver-side EASYDMA: find the UARTE0 slot and run `f` on it.
pub fn with_uarte<R>(sys: &System, f: impl FnOnce(&mut Uarte) -> R) -> Option<R> {
    with_uarte_at(sys, 0x4000_2000, f)
}

/// Same for an explicit instance base (UARTE1 lives at 0x40028000 and
/// stages transfers exactly like UARTE0).
pub fn with_uarte_at<R>(sys: &System, base: u32, f: impl FnOnce(&mut Uarte) -> R) -> Option<R> {
    for slot in &sys.p.peripherals {
        if slot.start == base {
            let mut b = slot.peripheral.borrow_mut();
            if let Some(u) = b.as_any_mut().downcast_mut::<Uarte>() {
                return Some(f(u));
            }
            return None;
        }
    }
    None
}

/// Take a staged RX DMA transfer (PTR, MAXCNT); None when idle.
/// Checks UARTE0 then UARTE1.
pub fn take_rxdma(sys: &System) -> Option<(u32, u32)> {
    with_uarte(sys, |u| {
        if u.rx_pending {
            u.rx_pending = false;
            u.rx_taken = true;
            Some((u.rx_ptr, u.rx_maxcnt))
        } else {
            None
        }
    })
    .flatten()
    .or_else(|| {
        with_uarte_at(sys, 0x4002_8000, |u| {
            if u.rx_pending {
                u.rx_pending = false;
                u.rx_taken = true;
                Some((u.rx_ptr, u.rx_maxcnt))
            } else {
                None
            }
        })
        .flatten()
    })
}

/// Complete an RX DMA transfer: driver already wrote `amount` bytes to RAM
/// at PTR. Sets ENDRX (+ IRQ when INTEN bit 4 is set) on the taken
/// instance (UARTE0 on ties / legacy direct completes).
pub fn complete_rxdma(sys: &System, amount: u32) {
    let taken0 = with_uarte(sys, |u| u.rx_taken).unwrap_or(false);
    let taken1 = with_uarte_at(sys, 0x4002_8000, |u| u.rx_taken).unwrap_or(false);
    let complete_on = |sys: &System, u: &mut Uarte| {
        u.rx_amount = amount;
        u.rx_taken = false;
        u.ev_endrx = true;
        (u.irq, u.intenset)
    };
    let fire = if taken0 || !taken1 {
        with_uarte(sys, |u| complete_on(sys, u))
    } else {
        with_uarte_at(sys, 0x4002_8000, |u| complete_on(sys, u))
    };
    if let Some((irq, en)) = fire {
        if en & (1 << 4) != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(irq);
        }
    }
}
/// Take a staged TX DMA transfer (PTR, MAXCNT); None when idle.
/// Checks UARTE0 then UARTE1 (both stage identically).
pub fn take_txdma(sys: &System) -> Option<(u32, u32)> {
    with_uarte(sys, |u| {
        if u.tx_pending {
            u.tx_pending = false;
            u.tx_taken = true;
            Some((u.tx_ptr, u.tx_maxcnt))
        } else {
            None
        }
    })
    .flatten()
    .or_else(|| {
        with_uarte_at(sys, 0x4002_8000, |u| {
            if u.tx_pending {
                u.tx_pending = false;
                u.tx_taken = true;
                Some((u.tx_ptr, u.tx_maxcnt))
            } else {
                None
            }
        })
        .flatten()
    })
}

/// Complete a TX DMA transfer: bytes hit the console, AMOUNT + ENDTX set.
/// Completes whichever instance was taken (UARTE0 on ties or when
/// completing without a prior take, preserving legacy behavior).
pub fn complete_txdma(sys: &System, data: &[u8]) {
    let taken0 = with_uarte(sys, |u| u.tx_taken).unwrap_or(false);
    let taken1 = with_uarte_at(sys, 0x4002_8000, |u| u.tx_taken).unwrap_or(false);
    let complete_on = |sys: &System, u: &mut Uarte| {
        for &b in data {
            get_uart_output().lock().unwrap().push(b as char);
        }
        u.tx_amount = data.len() as u32;
        u.tx_taken = false;
        u.ev_txdrdy = true;
        u.ev_endtx = true;
        (u.irq, u.intenset)
    };
    let n = if taken0 || !taken1 {
        with_uarte(sys, |u| complete_on(sys, u))
    } else {
        with_uarte_at(sys, 0x4002_8000, |u| complete_on(sys, u))
    };
    if let Some(n) = n {
        if n.1 & (1 << 7) != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(n.0);
        }
        if n.1 & (1 << 8) != 0 {
            sys.p.nvic.borrow_mut().set_intr_pending(n.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::test_dummy_system;
    #[test]
    fn tx_byte_reaches_console_and_events() {
        let _u = crate::system::lock_uart();
        let sys = test_dummy_system();
        crate::system::get_uart_output().lock().unwrap().clear();
        let mut u = Uarte::default();
        u.write(&sys, 0x500, 8); // ENABLE=UARTE
        u.write(&sys, 0x51C, b'H' as u32);
        assert_eq!(u.read(&sys, 0x11C), 1);
        assert_eq!(u.read(&sys, 0x120), 1);
        assert!(crate::system::get_uart_output().lock().unwrap().contains('H'));
        u.write(&sys, 0x11C, 0);
        assert_eq!(u.read(&sys, 0x11C), 0);
    }
    #[test]
    fn rx_byte_sets_event_and_reads() {
        let sys = test_dummy_system();
        let mut u = Uarte::default();
        u.rx_byte(&sys, 0x41);
        assert_eq!(u.read(&sys, 0x108), 1);
        assert_eq!(u.read(&sys, 0x518), 0x41);
    }
    #[test]
    fn map_routed_rx_byte_sets_event() {
        use crate::system::test_dummy_system;
        let sys = test_dummy_system();
        assert!(sys.p.rx_byte(&sys, 0x40002000, 0x41), "route exists");
        assert_eq!(sys.p.read(&sys, 0x40002108, 4), 1, "RXDRDY via map");
        assert_eq!(sys.p.read(&sys, 0x40002518, 4), 0x41, "RXD via map");
    }
    #[test]
    fn stoptx_raises_txstopped_not_endtx() {
        // TASKS_STOPTX must raise EVENTS_TXSTOPPED (0x158, INTEN 22) and
        // must NOT raise ENDTX: ENDTX-on-STOPTX self-triggers an ENDTX
        // ISR loop (firmware re-enters on its own event forever).
        use crate::system::test_dummy_system;
        let sys = test_dummy_system();
        let mut u = Uarte::default();
        u.write(&sys, 0x500, 8); // enable
        sys.p.write(&sys, 0xE000E100, 4, 1 << 2); // NVIC ISER: UARTE0
        u.write(&sys, 0x304, 1 << 22); // INTEN TXSTOPPED
        u.write(&sys, 0x544, 0x20001000); // TXD.PTR
        u.write(&sys, 0x548, 4); // TXD.MAXCNT
        u.write(&sys, 0x008, 1); // STARTTX stages
        u.write(&sys, 0x00C, 1); // STOPTX aborts
        assert_eq!(u.read(&sys, 0x158), 1, "TXSTOPPED set");
        assert_eq!(u.read(&sys, 0x120), 0, "ENDTX must stay clear");
        assert!(sys.p.nvic.borrow().has_pending(), "TXSTOPPED IRQ pends");
        u.write(&sys, 0x158, 0);
        assert_eq!(u.read(&sys, 0x158), 0, "clear by write-0");
    }
    #[test]
    fn rx_overrun_sets_error() {        let sys = test_dummy_system();
        let mut u = Uarte::default();
        u.rx_byte(&sys, 0x41);
        u.rx_byte(&sys, 0x42); // unread: overrun, latest wins
        assert_eq!(u.read(&sys, 0x518), 0x42);
        assert_eq!(u.read(&sys, 0x124), 1, "ERROR event");
        assert_eq!(u.read(&sys, 0x480) & 1, 1, "OVERRUN cause");
        u.write(&sys, 0x480, 1);
        assert_eq!(u.read(&sys, 0x480) & 1, 0, "cleared");
    }
    #[test]
    fn tx_dma_completion_irq_when_enabled() {
        let sys = test_dummy_system();
        sys.p.write(&sys, 0xE000E100, 4, 1 << 2); // NVIC ISER: UARTE0
        sys.p.write(&sys, 0x40002304, 4, 1 << 8); // INTEN: ENDTX
        sys.p.write(&sys, 0x40002544, 4, 0x20001000);
        sys.p.write(&sys, 0x40002548, 4, 1);
        sys.p.write(&sys, 0x40002008, 4, 1);
        complete_txdma(&sys, b"Z");
        assert!(sys.p.nvic.borrow().has_pending(), "ENDTX IRQ pends");
    }
    #[test]
    fn tx_dma_stages_and_completes() {
        let _u = crate::system::lock_uart();
        // Full driver round-trip against the live map (take/complete take
        // sys explicitly, like the JS driver — no SYS install needed).
        let sys = test_dummy_system();
        crate::system::get_uart_output().lock().unwrap().clear();
        sys.p.write(&sys, 0x40002544, 4, 0x20001000); // TXD.PTR
        sys.p.write(&sys, 0x40002548, 4, 3);          // TXD.MAXCNT
        sys.p.write(&sys, 0x40002008, 4, 1);          // STARTTX
        assert_eq!(sys.p.read(&sys, 0x40002120, 4), 0, "ENDTX waits for driver");
        let t = take_txdma(&sys).expect("staged");
        assert_eq!(t, (0x20001000, 3));
        assert!(take_txdma(&sys).is_none(), "staged once only");
        complete_txdma(&sys, b"DMA");
        assert_eq!(sys.p.read(&sys, 0x40002120, 4), 1, "ENDTX after complete");
        assert_eq!(sys.p.read(&sys, 0x4000254C, 4), 3, "AMOUNT");
        assert!(crate::system::get_uart_output().lock().unwrap().contains("DMA"));
    }
    #[test]
    fn rx_dma_stages_and_completes() {
        let sys = test_dummy_system();
        sys.p.write(&sys, 0x40002534, 4, 0x20001000); // RXD.PTR
        sys.p.write(&sys, 0x40002538, 4, 4);          // RXD.MAXCNT
        sys.p.write(&sys, 0x40002000, 4, 1);          // STARTRX
        assert_eq!(sys.p.read(&sys, 0x4000210C, 4), 0, "ENDRX waits for driver");
        let t = take_rxdma(&sys).expect("staged");
        assert_eq!(t, (0x20001000, 4));
        complete_rxdma(&sys, 4);
        assert_eq!(sys.p.read(&sys, 0x4000210C, 4), 1, "ENDRX after complete");
        assert_eq!(sys.p.read(&sys, 0x4000253C, 4), 4, "AMOUNT");
    }
    #[test]
    fn uarte1_txdma_roundtrip_targets_instance_1() {
        // UARTE1 (0x40028000, IRQ 40) stages and completes exactly like
        // UARTE0; completion lands on instance 1, not instance 0.
        use crate::system::test_dummy_system;
        let sys = test_dummy_system();
        sys.p.write(&sys, 0xE000E104, 4, 1 << 8); // NVIC ISER word1: IRQ 40
        sys.p.write(&sys, 0x40028304, 4, 1 << 8); // UARTE1 INTEN: ENDTX
        sys.p.write(&sys, 0x40028500, 4, 8); // UARTE1 ENABLE
        sys.p.write(&sys, 0x40028544, 4, 0x20003000); // TXD.PTR
        sys.p.write(&sys, 0x40028548, 4, 3); // TXD.MAXCNT
        sys.p.write(&sys, 0x40028008, 4, 1); // STARTTX
        let t = take_txdma(&sys).expect("uarte1 staged");
        assert_eq!(t, (0x20003000, 3));
        // UARTE0 must NOT show completion.
        assert_eq!(sys.p.read(&sys, 0x40002120, 4), 0, "UARTE0 ENDTX stays clear");
        complete_txdma(&sys, b"Hi!");
        assert_eq!(sys.p.read(&sys, 0x40028120, 4), 1, "UARTE1 ENDTX set");
        assert!(sys.p.nvic.borrow().has_pending(), "IRQ 40 pends");
        // 2nd run: fresh default has nothing staged.
        assert_eq!(Uarte::default().tx_taken, false);
        // RX direction too.
        sys.p.write(&sys, 0x40028534, 4, 0x20003000); // RXD.PTR
        sys.p.write(&sys, 0x40028538, 4, 2); // RXD.MAXCNT
        sys.p.write(&sys, 0x40028000, 4, 1); // STARTRX
        let r = take_rxdma(&sys).expect("uarte1 rx staged");
        assert_eq!(r, (0x20003000, 2));
        complete_rxdma(&sys, 2);
        assert_eq!(sys.p.read(&sys, 0x4002810C, 4), 1, "UARTE1 ENDRX set");
        assert_eq!(sys.p.read(&sys, 0x4000210C, 4), 0, "UARTE0 ENDRX stays clear");
    }
}
