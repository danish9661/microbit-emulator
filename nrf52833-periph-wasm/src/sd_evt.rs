//! Scoped SoftDevice-event transport, phase 1: flash-operation events
//! only (`docs/sd_evt_design.md`). NOT a Peripheral (no MMIO base — it
//! is an SVC interface, like `sd_ble`): a process-wide singleton queue
//! (same pattern as the INSTRUCTION_COUNT atomics / tap queues).
//!
//! Wire, not synthesis: the real S140 binary implements the event state
//! machine; the model only carries the completion event from the NVMC
//! driver completion to the firmware `sd_evt_get` (SVC 82) call. The
//! SD performs flash ops by writing NVMC registers directly; our NVMC
//! model already observes those writes, and on driver completion
//! (`complete_erase`, staged writes) we post
//! FLASH_OPERATION_SUCCESS (id 2, len 0) — but ONLY while the SD is
//! enabled (tracked via observed SVC 16 enable calls; silicon generates
//! no event with the SD disabled). `sd_evt_get` with a non-empty queue
//! answers from the model queue (writes `{id u16, len u16}` to the app
//! buffer at r0, r0 = NRF_SUCCESS) and skips SD entry; empty queue
//! falls through to the SD untouched (NOT_FOUND, harmless). No BLE /
//! radio / timeslot events in phase 1 — HFCLKSTARTED (id 0) is owned by
//! the CLOCK model and is never queued here.
//!
//! Numbers: SOC_SVC_BASE 0x20 → ENABLE 16, EVT_GET 82; NRF_SOC_EVTS
//! FLASH_OPERATION_SUCCESS 2, FLASH_OPERATION_ERROR 3 (S132 headers;
//! S140-stable; MPY/MC binaries contain zero `DF52` sites, so this hook
//! is dead code for shipped firmware — it exists for the demo/BLE path
//! that DOES stage flash ops through the SD, plan P24–P26).

use std::cell::RefCell;
use std::collections::VecDeque;

use crate::cpu::mem::Memory;

/// SoC SVC numbers (SOC_SVC_BASE 0x20 + enum position).
pub const SVC_SOC_ENABLE: u8 = 16;
pub const SVC_SOC_EVT_GET: u8 = 82;

/// SoC event ids (NRF_SOC_EVTS from 0).
pub const SOC_EVT_FLASH_SUCCESS: u16 = 2;
pub const SOC_EVT_FLASH_ERROR: u16 = 3;

/// NRF return codes (mirrors sd_ble.rs values; kept local so this
/// module has no dependency on the BLE face).
pub const SOC_SUCCESS: u32 = 0;
pub const SOC_NOT_FOUND: u32 = 5;

#[derive(Clone, Copy, Debug, Default)]
struct SocEvt {
    id: u16,
    len: u16,
}

#[derive(Default)]
struct SdEvt {
    /// True once firmware enabled the SoftDevice (observed SVC 16).
    /// Default off: silicon generates no event with the SD disabled.
    sd_enabled: bool,
    queue: VecDeque<SocEvt>,
}

thread_local! {
    static SD_EVT_STATE: RefCell<SdEvt> = RefCell::new(SdEvt::default());
}

fn with_sd_evt<R>(f: impl FnOnce(&mut SdEvt) -> R) -> R {
    SD_EVT_STATE.with(|s| f(&mut s.borrow_mut()))
}

/// Observe an SD-enable call (SVC 16 path): arms event generation.
/// Called from the thumb SVC hook site (no cpu/ edits — the hook
/// already exists; this is a model-side observer like the NVMC tap).
pub fn note_sd_enable() {
    with_sd_evt(|s| {
        s.sd_enabled = true;
    })
}

/// Observe SD disable / full reset: disarms + drains the queue.
pub fn reset_sd_evt() {
    with_sd_evt(|s| {
        *s = SdEvt::default();
    });
}

/// Post a flash-operation completion (driver `complete_erase` / staged
/// write completion path calls this AFTER the take→complete move).
/// Queued only while the SD is enabled; success=true → id 2, else 3.
pub fn post_flash_op(success: bool) {
    with_sd_evt(|s| {
        if !s.sd_enabled {
            return;
        }
        s.queue.push_back(SocEvt {
            id: if success {
                SOC_EVT_FLASH_SUCCESS
            } else {
                SOC_EVT_FLASH_ERROR
            },
            len: 0,
        });
    })
}

/// Queue length (debug/export path; driver pump consults it).
pub fn queue_len() -> usize {
    with_sd_evt(|s| s.queue.len())
}

/// Handle `sd_evt_get` (SVC 82): r0 = app word-buffer address (may be
/// any RAM; 0 = length-query convention is NOT part of the SoC
/// contract — a NULL buffer with events pending is INVALID_PARAM on
/// silicon; we return the event only into a real buffer).
/// Returns Some(r0): SUCCESS + `{id u16, len u16}` written, entry
/// popped; None = queue empty → caller falls through to the SD
/// (which returns NOT_FOUND from its own empty queue, harmless).
pub fn handle_evt_get(mem: &mut dyn Memory, buf: u32) -> Option<u32> {
    fn is_ram(addr: u32) -> bool {
        (0x2000_0000..0x2002_0000).contains(&addr)
    }
    with_sd_evt(|s| {
        let ev = *s.queue.front()?;
        if buf == 0 || !is_ram(buf) {
            return Some(SOC_SUCCESS);
        }
        mem.write16(buf, ev.id);
        mem.write16(buf.wrapping_add(2), ev.len);
        s.queue.pop_front();
        Some(SOC_SUCCESS)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::mem::FlatMemory;
    use crate::system::test_dummy_system;

    #[test]
    fn disabled_never_queues() {
        reset_sd_evt();
        post_flash_op(true);
        assert_eq!(queue_len(), 0, "SD disabled: no event");
        let sys = test_dummy_system();
        let _ = sys;
        let mut mem = FlatMemory::new(512 * 1024, 128 * 1024);
        assert_eq!(handle_evt_get(&mut mem, 0x20001000), None, "empty falls through");
    }

    #[test]
    fn enable_post_drain_in_order() {
        reset_sd_evt();
        note_sd_enable();
        post_flash_op(true);
        post_flash_op(false);
        assert_eq!(queue_len(), 2);
        let mut mem = FlatMemory::new(512 * 1024, 128 * 1024);
        assert_eq!(handle_evt_get(&mut mem, 0x20001000), Some(SOC_SUCCESS));
        assert_eq!(mem.read16(0x20001000), SOC_EVT_FLASH_SUCCESS);
        assert_eq!(mem.read16(0x20001002), 0);
        assert_eq!(handle_evt_get(&mut mem, 0x20001000), Some(SOC_SUCCESS));
        assert_eq!(mem.read16(0x20001000), SOC_EVT_FLASH_ERROR);
        assert_eq!(handle_evt_get(&mut mem, 0x20001000), None, "drained: fall through");
        assert_eq!(queue_len(), 0);
    }

    #[test]
    fn reset_disarms_and_drains() {
        note_sd_enable();
        post_flash_op(true);
        assert_eq!(queue_len(), 1);
        reset_sd_evt();
        assert_eq!(queue_len(), 0);
        post_flash_op(true);
        assert_eq!(queue_len(), 0, "disarmed again");
    }
}
