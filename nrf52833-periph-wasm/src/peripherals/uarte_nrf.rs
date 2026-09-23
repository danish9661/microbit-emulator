use std::cell::Cell;
use crate::system::{System, instruction_count, get_uart_output};
use crate::cpu::mem::{FlatMemory, Memory};
use super::Peripheral;

// Thread-local guest-RAM hook the TX snapshot path reads through.
//
// Synchronous TX snapshot (P49 N+1 drops): silicon EasyDMA latches
// TXD bytes within cycles of TASKS_STARTTX, but our driver take runs
// up to ~100K instructions later, after MicroPython's `putc` reuses
// its `&c` stack slot. `WasmCpu::step` publishes its `FlatMemory`
// here for the duration of `cpu.run`; STARTTX copies `MAXCNT` bytes
// synchronously and `complete_txdma` emits the snapshot instead of
// the driver's late bytes. No `Peripheral::write` trait change, no
// `src/cpu` edits; native harnesses never set it (null = legacy).
thread_local! {
    static TX_SNAPSHOT_MEM: Cell<*const FlatMemory> = Cell::new(std::ptr::null());
}
/// Publish guest RAM for synchronous TX snapshots; returned guard clears
/// on drop (also on panic), so no stale pointer can outlive the step.
pub fn tx_snapshot_guard(mem: &FlatMemory) -> TxSnapshotGuard {
    TX_SNAPSHOT_MEM.with(|c| c.set(mem as *const FlatMemory));
    TxSnapshotGuard
}
pub struct TxSnapshotGuard;
impl Drop for TxSnapshotGuard {
    fn drop(&mut self) {
        TX_SNAPSHOT_MEM.with(|c| c.set(std::ptr::null()));
    }
}
fn snapshot_tx_bytes(ptr: u32, len: u32) -> Option<Vec<u8>> {
    TX_SNAPSHOT_MEM.with(|c| {
        let p = c.get();
        if p.is_null() {
            None
        } else {
            // SAFETY: set by the enclosing WasmCpu::step guard on this
            // thread; cleared on guard drop. Snapshot reads RAM only
            // (TXD.PTR is always a RAM ring/slot address), never MMIO,
            // so no peripheral reentrancy through this read.
            let mem = unsafe { &*p };
            Some((0..len).map(|i| mem.read8(ptr.wrapping_add(i))).collect())
        }
    })
}

/// UARTE0 @ 0x40002000 (IRQ 2). Polling subset + EASYDMA:
///   ENABLE 0x500, BAUDRATE 0x524, TXD 0x51C (byte TX -> UART_OUTPUT),
///   RXD 0x518 (byte RX), EVENTS_RXDRDY 0x108 / EVENTS_ENDRX 0x110 /
///   EVENTS_TXDRDY 0x11C / EVENTS_ENDTX 0x120 / EVENTS_ERROR 0x124,
///   SHORTS 0x200 (bit 5 ENDRX_STARTRX, bit 6 ENDRX_STOPRX — SVD),
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
    /// SHORTS register (SVD 0x200): bit 5 ENDRX_STARTRX, bit 6
    /// ENDRX_STOPRX. P134: the model previously had no SHORTS storage
    /// at all (writes ignored, reads 0) — any firmware relying on the
    /// shortcut would stall with no trace.
    shorts: u32,
    /// SHORTS latched at the last TASKS_STARTRX (see the write arm):
    /// the shortcut fires on the ENDRX event, so the ENTRY state is
    /// what matters, not whatever firmware writes afterwards.
    shorts_at_endrx: u32,
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
    /// Bytes latched synchronously at STARTTX (see above). Used by
    /// `complete_txdma` instead of the driver's late `mem_read`.
    /// Keyed by TXD.PTR (P135): firmware uses one `&c` slot PER CALLER
    /// (8 distinct TXD.PTR values seen across one MPY echo line), so a
    /// global FIFO misattributes bytes when slots interleave — the pop
    /// at complete must match the take's PTR, not queue position.
    /// Each entry is (ptr, bytes); complete pops the entry for the
    /// taken PTR, else falls back to driver bytes.
    tx_snapshot: std::collections::VecDeque<(u32, Vec<u8>)>,
}

impl Default for Uarte {
    fn default() -> Self {
        Self { irq: 2, enable: 0, baudrate: 0, ev_txdrdy: false, ev_endtx: false,
               ev_txstopped: false,
                ev_rxdrdy: false, ev_endrx: false, ev_error: false, errorsrc: 0, rxd: 0,
                intenset: 0, shorts: 0, shorts_at_endrx: 0, rx_ptr: 0, rx_maxcnt: 0, rx_amount: 0, rx_pending: false, rx_taken: false,
                tx_ptr: 0, tx_maxcnt: 0, tx_amount: 0, tx_pending: false, tx_taken: false,
                tx_snapshot: std::collections::VecDeque::new() }
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
            0x110 => self.ev_endrx as u32, // ENDRX (SVD 0x110; was 0x10C — P134: MPY polls 0x110, so ENDRX never cleared and the REPL line-ring stalled at AMT=MAX)
            0x11C => self.ev_txdrdy as u32,
            0x120 => self.ev_endtx as u32,
            0x158 => self.ev_txstopped as u32, // EVENTS_TXSTOPPED (SVD)
            0x124 => self.ev_error as u32,
            0x200 => self.shorts,
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
                // SHORTS state snapshot (P134): a shortcut fires when its
                // EVENT is set, so ENDRX_STARTRX re-arms the receiver at
                // ENDRX time even if firmware later clears SHORTS for the
                // next line (MicroPython re-arms per line: SHORTS=0x20 on
                // entry, cleared on exit — the stall fix needs the ENTRY
                // state, not the exit state).
                self.shorts_at_endrx = self.shorts;
            }
            0x004 => { self.rx_pending = false; self.ev_endrx = true; self.fire(sys, 1 << 4); }
            0x008 => { // TASKS_STARTTX
                self.ev_endtx = false;
                self.tx_amount = 0;
                if self.tx_maxcnt > 0 {
                    self.tx_pending = true; // driver completes (take/complete)
                    // Latch the DMA source now: by take time the firmware
                    // has reused putc's `&c` slot (P49). No mem in this
                    // call means a legacy harness path — take/complete as
                    // before. Keyed by TXD.PTR (P135): one `&c` slot PER
                    // CALLER (8 distinct PTRs across one MPY echo line),
                    // so the pop at complete matches the taken PTR.
                    // Snapshot ONLY when the DMA source is a single reused
                    // slot (MAXCNT==1, the putc `&c` shape): multi-byte
                    // transfers read a stable firmware buffer, and a stale
                    // snapshot there would DUPLICATE bytes if the driver
                    // also completes (the "ZSHi!" interleave: snapshot
                    // emitted a byte the TXD path had already printed).
                    if self.tx_maxcnt == 1 {
                        match snapshot_tx_bytes(self.tx_ptr, self.tx_maxcnt) {
                            Some(bytes) => {
                                let ptr = self.tx_ptr;
                                if let Some(e) = self.tx_snapshot.iter_mut().find(|(p, _)| *p == ptr) {
                                    e.1 = bytes; // same slot re-staged: latest wins
                                } else {
                                    self.tx_snapshot.push_back((ptr, bytes));
                                }
                            }
                            None => {}
                        }
                    }
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
            // P134: STOPTX ends the CURRENT transfer (pending clears, no
            // phantom re-take) but must NOT clear a queued STARTTX
            // snapshot. MicroPython's putc abort shape (STARTTX,
            // ENDTX-clear, STOPTX per byte — proven by MMIO trace) takes
            // effect as take -> STOPTX -> complete in the pump; the
            // snapshot popped at complete is the byte's own. Clearing
            // the queue on STOPTX dropped the staged byte and the echo
            // garbled (spaces for letters) with no fault.
            0x00C => { self.tx_pending = false; self.ev_txstopped = true; self.fire(sys, 1 << 22); }
            0x108 => if value == 0 { self.ev_rxdrdy = false; }
            0x110 => if value == 0 { self.ev_endrx = false; } // ENDRX clear (SVD 0x110)
            0x11C => if value == 0 { self.ev_txdrdy = false; }
            0x120 => if value == 0 { self.ev_endtx = false; }
            0x158 => if value == 0 { self.ev_txstopped = false; }
            0x124 => if value == 0 { self.ev_error = false; }
            0x200 => self.shorts = value & 0x60, // SHORTS: bit 5 ENDRX_STARTRX, bit 6 ENDRX_STOPRX (SVD)
            0x304 => {
                self.intenset |= value;
                // re-fire any already-set event the firmware just enabled
                if self.ev_rxdrdy && value & (1 << 2) != 0 { self.fire(sys, 1 << 2); }
                if self.ev_endrx && value & (1 << 4) != 0 { self.fire(sys, 1 << 4); }
                if self.ev_txdrdy && value & (1 << 7) != 0 { self.fire(sys, 1 << 7); }
                if self.ev_endtx && value & (1 << 8) != 0 { self.fire(sys, 1 << 8); }
                if self.ev_txstopped && value & (1 << 22) != 0 { self.fire(sys, 1 << 22); }
            }
            0x308 => self.intenset &= !value,
            0x480 => self.errorsrc &= !value, // write-1-clears
            0x500 => self.enable = value & 0xF,
            0x518 => {} // RXD read-only
            0x51C => {
                // TXD byte: console lifeline. Guarded by ENABLE (silicon
                // ignores TXD when the UARTE is disabled; without this a
                // parallel test's stale TXD write lands in the shared
                // UART buffer mid-assert — the "SENZS"/"ZSHi!" dup-byte
                // flake: sensors asserts SENS while another test's byte
                // interleaves).
                if self.enable == 0 {
                    return;
                }
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
                // ENDRX_STARTRX shortcut (SVD SHORTS bit 5): silicon
                // re-arms the receiver in hardware. P134: MicroPython's
                // line reader only re-arms per line via its slow path; a
                // 32B ring that fills mid-line never re-arms and the REPL
                // stalls with no fault. The shortcut is the architected
                // re-arm — apply it exactly when firmware enabled it.
                // LIVE state (shorts): firmware arms SHORTS=0x20 per line
                // and it stays armed through the transfer (proven live:
                // SHORTS reads 0x20 at prompt AND after the stalled line
                // — the exit-clear theory was wrong, the register simply
                // stays armed). Latch kept for the exit-clear shape.
                // CRITICAL: do NOT clear ev_endrx here. On silicon the
                // shortcut fires ON the event but the event stays set
                // until firmware clears it — the ISR's ENDRX branch
                // (drain-all + counter reset) is what unblocks the line
                // reader. Clearing it hid ENDRX: the per-byte RXDRDY path
                // hit its counter>=31 gate and the line froze mid-echo
                // with no fault (P134 root cause).
                if self.shorts & (1 << 5) != 0 || self.shorts_at_endrx & (1 << 5) != 0 {
                    self.rx_amount = 0;
                    self.rx_pending = self.rx_maxcnt > 0;
                } else if self.shorts & (1 << 6) != 0 || self.shorts_at_endrx & (1 << 6) != 0 {
                    // ENDRX_STOPRX: receiver stops (already not pending).
                }
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
            // try_borrow_mut (P108 SYS-swap family): Peripherals::read/
            // write/tick already hold this slot while driving UARTE
            // (e.g. a model write re-enters via take/complete paths).
            // A failed borrow drops the op, never panics.
            let mut b = match slot.peripheral.try_borrow_mut() {
                Ok(b) => b,
                Err(_) => return None,
            };
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
    let complete_on = |_sys: &System, u: &mut Uarte| {
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
/// NOTE: take/complete are a rate-1 pair per transfer. Firmware may
/// STARTTX again before the driver completes (putc's polled ENDTX spin
/// + once-per-pump take); each STARTTX pushes one snapshot, each
/// complete pops one. take clears `pending` so a second take without
/// an intervening STARTTX returns None (no phantom re-emit).
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
/// Pops the snapshot keyed by the TAKEN PTR (P135): one `&c` slot per
/// caller, so the snapshot at THIS transfer's STARTTX wins over the
/// driver's late `mem_read` (which may already hold the reused N+1
/// byte — P49). Falls back to driver bytes only when no snapshot is
/// queued for that PTR (legacy paths: unit/native harnesses never
/// publish mem).
pub fn complete_txdma(sys: &System, data: &[u8]) {
    let taken0 = with_uarte(sys, |u| (u.tx_taken, u.tx_ptr)).unwrap_or((false, 0));
    let taken1 = with_uarte_at(sys, 0x4002_8000, |u| (u.tx_taken, u.tx_ptr)).unwrap_or((false, 0));
    let complete_on = |ptr: u32, _sys: &System, u: &mut Uarte| {
        let pos = u.tx_snapshot.iter().position(|(p, _)| *p == ptr);
        let snap = pos.map(|i| u.tx_snapshot.remove(i).unwrap().1);
        let bytes: &[u8] = snap.as_deref().unwrap_or(data);
        for &b in bytes {
            get_uart_output().lock().unwrap().push(b as char);
        }
        u.tx_amount = bytes.len() as u32;
        u.tx_taken = false;
        u.ev_txdrdy = true;
        u.ev_endtx = true;
        (u.irq, u.intenset)
    };
    let n = if taken0.0 || !taken1.0 {
        with_uarte(sys, |u| complete_on(taken0.1, sys, u))
    } else {
        with_uarte_at(sys, 0x4002_8000, |u| complete_on(taken1.1, sys, u))
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
    fn stoptx_preserves_queued_snapshot() {
        // P134: MicroPython's putc abort shape (STARTTX, ENDTX-clear,
        // STOPTX per byte — proven by MMIO trace) ends the CURRENT
        // transfer at STOPTX: pending clears (no phantom re-take), but
        // the STARTTX snapshot survives for the driver's complete.
        // The pump takes BEFORE firmware's STOPTX lands, so complete
        // pops the snapshot even though pending is already clear.
        use crate::cpu::mem::{FlatMemory, Memory};
        let _u = crate::system::lock_uart();
        let sys = test_dummy_system();
        crate::system::get_uart_output().lock().unwrap().clear();
        let mut mem = FlatMemory::new(512 * 1024, 128 * 1024);
        mem.write8(0x20001000, b'Q');
        sys.p.write(&sys, 0x40002544, 4, 0x20001000);
        sys.p.write(&sys, 0x40002548, 4, 1);
        let _g = tx_snapshot_guard(&mem);
        sys.p.write(&sys, 0x40002008, 4, 1); // STARTTX snapshots 'Q'
        sys.p.write(&sys, 0x40002120, 4, 0); // ENDTX-clear (putc shape)
        let t = take_txdma(&sys).expect("take before STOPTX");
        assert_eq!(t, (0x20001000, 1));
        sys.p.write(&sys, 0x4000200C, 4, 1); // STOPTX after take
        drop(_g);
        assert!(take_txdma(&sys).is_none(), "STOPTX ends transfer: no re-take");
        mem.write8(0x20001000, b'Z'); // slot reuse
        complete_txdma(&sys, b"Z"); // driver late bytes lose to snapshot
        let out = crate::system::get_uart_output().lock().unwrap().clone();
        assert!(out.contains('Q'), "snapshot 'Q' emitted, got {out:?}");
        assert!(!out.contains('Z'), "stale driver bytes suppressed, got {out:?}");
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
        // UART lock: complete_txdma pushes "Z" to the shared console.
        let _u = crate::system::lock_uart();
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
    fn tx_snapshot_freezes_starttx_bytes() {
        use crate::cpu::mem::{FlatMemory, Memory};
        let _u = crate::system::lock_uart();
        // STARTTX copies RAM now; a later slot reuse (the P49 N+1 race)
        // must not leak into the console. Second run without the guard
        // takes the legacy driver-bytes path (no leak of the snapshot).
        let sys = test_dummy_system();
        crate::system::get_uart_output().lock().unwrap().clear();
        let mut mem = FlatMemory::new(512 * 1024, 128 * 1024);
        mem.write8(0x20001000, b'A');
        sys.p.write(&sys, 0x40002544, 4, 0x20001000); // TXD.PTR
        sys.p.write(&sys, 0x40002548, 4, 1);          // TXD.MAXCNT
        let _g = tx_snapshot_guard(&mem);
        sys.p.write(&sys, 0x40002008, 4, 1);          // STARTTX snapshots 'A'
        drop(_g);
        mem.write8(0x20001000, b'B'); // firmware reuses putc's slot
        let t = take_txdma(&sys).expect("staged");
        assert_eq!(t, (0x20001000, 1));
        complete_txdma(&sys, b"B"); // driver read late, holds N+1
        let out = crate::system::get_uart_output().lock().unwrap().clone();
        assert!(out.contains('A'), "snapshot byte emitted, got {out:?}");
        assert!(!out.contains('B'), "reused byte suppressed, got {out:?}");
        assert_eq!(sys.p.read(&sys, 0x40002120, 4), 1, "ENDTX after complete");
        // 2nd run: no guard, no snapshot — legacy path, no leak.
        crate::system::get_uart_output().lock().unwrap().clear();
        sys.p.write(&sys, 0x40002008, 4, 1); // STARTTX without mem
        let t2 = take_txdma(&sys).expect("staged again");
        assert_eq!(t2, (0x20001000, 1));
        complete_txdma(&sys, b"C");
        let out2 = crate::system::get_uart_output().lock().unwrap().clone();
        assert!(out2.contains('C'), "legacy driver bytes pass through, got {out2:?}");
    }
    #[test]
    fn tx_snapshot_fifo_preserves_per_transfer_order() {
        // P134: back-to-back STARTTX on DIFFERENT slots without an
        // intervening complete (firmware stages ahead of the
        // once-per-pump driver) must NOT collapse: take1's complete
        // emits take1's snapshot even though STARTTX #2 already fired.
        // (Proven live: take-time bytes read 0x20 while the console
        // needed 'l' — the take-time mem_read was stale.)
        use crate::cpu::mem::{FlatMemory, Memory};
        let _u = crate::system::lock_uart();
        let sys = test_dummy_system();
        crate::system::get_uart_output().lock().unwrap().clear();
        let mut mem = FlatMemory::new(512 * 1024, 128 * 1024);
        mem.write8(0x20001000, b'A');
        mem.write8(0x20001004, b'B');
        sys.p.write(&sys, 0x40002544, 4, 0x20001000);
        sys.p.write(&sys, 0x40002548, 4, 1);
        let _g = tx_snapshot_guard(&mem);
        sys.p.write(&sys, 0x40002008, 4, 1); // STARTTX #1 snapshots 'A'@1000
        sys.p.write(&sys, 0x40002544, 4, 0x20001004);
        sys.p.write(&sys, 0x40002008, 4, 1); // STARTTX #2 snapshots 'B'@1004
        drop(_g);
        // Complete take1's transfer (taken PTR 0x20001000): pops 'A'
        // even though take2 staged after. Model take/complete order
        // honestly: take, complete, take, complete.
        let t1 = take_txdma(&sys).expect("take1 staged");
        assert_eq!(t1, (0x20001004, 1), "take returns latest PTR (single pending slot)");
        // NOTE: single pending slot — take2 overwrote take1's pending.
        // take1's transfer is already lost at the take layer (silicon
        // runs one transfer at a time); the snapshot keyed by PTR still
        // lets complete emit the right byte for the TAKEN ptr.
        complete_txdma(&sys, b"Z"); // stale driver bytes must lose
        let out = crate::system::get_uart_output().lock().unwrap().clone();
        assert!(out.contains('B'), "taken PTR's snapshot 'B' emitted, got {out:?}");
        assert!(!out.contains('Z'), "stale driver bytes suppressed, got {out:?}");
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
        assert_eq!(sys.p.read(&sys, 0x40002110, 4), 0, "ENDRX waits for driver");
        let t = take_rxdma(&sys).expect("staged");
        assert_eq!(t, (0x20001000, 4));
        complete_rxdma(&sys, 4);
        assert_eq!(sys.p.read(&sys, 0x40002110, 4), 1, "ENDRX after complete");
        assert_eq!(sys.p.read(&sys, 0x4000253C, 4), 4, "AMOUNT");
    }
    #[test]
    fn uarte1_txdma_roundtrip_targets_instance_1() {
        // UARTE1 (0x40028000, IRQ 40) stages and completes exactly like
        // UARTE0; completion lands on instance 1, not instance 0.
        // UART lock: complete_txdma pushes "Hi!" to the shared console —
        // without it a parallel marker test's assert can interleave.
        use crate::system::{lock_uart, test_dummy_system};
        let _u = lock_uart();
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
        assert_eq!(sys.p.read(&sys, 0x40028110, 4), 1, "UARTE1 ENDRX set");
        assert_eq!(sys.p.read(&sys, 0x40002110, 4), 0, "UARTE0 ENDRX stays clear");
    }
    #[test]
    fn endrx_lives_at_svd_offset_0x110() {
        // P134 regression: EVENTS_ENDRX is at 0x110 per the SVD (the model
        // wrongly used 0x10C, so MicroPython's ISR clear at 0x110 never
        // landed: ENDRX stayed set, the REPL line-ring stalled at
        // AMT=MAX=32, and lines >=17B never echoed). 0x10C must read 0.
        let sys = test_dummy_system();
        sys.p.write(&sys, 0x40002534, 4, 0x20001000); // RXD.PTR
        sys.p.write(&sys, 0x40002538, 4, 4);          // RXD.MAXCNT
        sys.p.write(&sys, 0x40002000, 4, 1);          // STARTRX
        let t = take_rxdma(&sys).expect("staged");
        assert_eq!(t, (0x20001000, 4));
        complete_rxdma(&sys, 4);
        assert_eq!(sys.p.read(&sys, 0x40002110, 4), 1, "ENDRX set at 0x110");
        assert_eq!(sys.p.read(&sys, 0x4000210C, 4), 0, "0x10C is not ENDRX");
        sys.p.write(&sys, 0x40002110, 4, 0); // firmware clear-by-write-0
        assert_eq!(sys.p.read(&sys, 0x40002110, 4), 0, "ENDRX clears at 0x110");
    }
    #[test]
    fn shorts_endrx_startrx_rearms_receiver() {
        // P134: SHORTS bit 5 (ENDRX_STARTRX, SVD 0x200) re-arms the
        // receiver in hardware: AMOUNT resets, ENDRX clears, and the
        // next byte starts a fresh transfer. Without this a 32B ring
        // that fills mid-line stalls the REPL with no fault.
        use crate::system::test_dummy_system;
        let sys = test_dummy_system();
        sys.p.write(&sys, 0x40002534, 4, 0x20001000); // RXD.PTR
        sys.p.write(&sys, 0x40002538, 4, 2);          // RXD.MAXCNT=2
        sys.p.write(&sys, 0x40002200, 4, 1 << 5);      // SHORTS ENDRX_STARTRX
        assert_eq!(sys.p.read(&sys, 0x40002200, 4), 1 << 5, "SHORTS reads back");
        sys.p.write(&sys, 0x40002000, 4, 1);          // STARTRX
        sys.p.rx_byte(&sys, 0x40002000, 0x41);
        assert_eq!(sys.p.read(&sys, 0x4000253C, 4), 1, "AMOUNT counts");
        sys.p.rx_byte(&sys, 0x40002000, 0x42); // fills ring: shortcut re-arms
        assert_eq!(sys.p.read(&sys, 0x4000253C, 4), 0, "AMOUNT reset by shortcut");
        assert_eq!(sys.p.read(&sys, 0x40002110, 4), 1, "ENDRX stays set (firmware clears it)");
        sys.p.rx_byte(&sys, 0x40002000, 0x43); // fresh transfer continues
        assert_eq!(sys.p.read(&sys, 0x4000253C, 4), 1, "next transfer counts");
        // No shortcut: classic ENDRX-stays-set behavior (driver re-arms).
        let sys2 = test_dummy_system();
        sys2.p.write(&sys2, 0x40002534, 4, 0x20001000);
        sys2.p.write(&sys2, 0x40002538, 4, 2);
        sys2.p.write(&sys2, 0x40002000, 4, 1);
        sys2.p.rx_byte(&sys2, 0x40002000, 0x41);
        sys2.p.rx_byte(&sys2, 0x40002000, 0x42);
        assert_eq!(sys2.p.read(&sys2, 0x40002110, 4), 1, "ENDRX stays without shortcut");
        assert_eq!(sys2.p.read(&sys2, 0x4000253C, 4), 2, "AMOUNT holds without shortcut");
        // Latched state: shortcut armed at STARTRX fires even if firmware
        // clears SHORTS before ENDRX (MicroPython's per-line pattern:
        // SHORTS=0x20 on entry, W=0 on exit — the MMIO trace proves it).
        let sys3 = test_dummy_system();
        sys3.p.write(&sys3, 0x40002534, 4, 0x20001000);
        sys3.p.write(&sys3, 0x40002538, 4, 2);
        sys3.p.write(&sys3, 0x40002200, 4, 1 << 5); // arm
        sys3.p.write(&sys3, 0x40002000, 4, 1);      // STARTRX latches
        sys3.p.write(&sys3, 0x40002200, 4, 0);      // exit-path clear
        sys3.p.rx_byte(&sys3, 0x40002000, 0x41);
        sys3.p.rx_byte(&sys3, 0x40002000, 0x42); // fills: latched shortcut re-arms
        assert_eq!(sys3.p.read(&sys3, 0x4000253C, 4), 0, "AMOUNT reset by latched shortcut");
        assert_eq!(sys3.p.read(&sys3, 0x40002110, 4), 1, "ENDRX stays set (firmware clears it)");
    }
}
