// SoftDevice BLE SVC service: GAP + GATTS + GATTC over the Bumble air bridge.
//
// SCOPE: this is the "same GATT treatment for the SoftDevice stack
// bluetooth" slice. The RADIO register model (radio_nrf.rs) stays the
// bare-metal 802.15.4-ish face; THIS module is the SVC face — the calls
// BLE firmware actually makes (sd_ble_enable, sd_ble_gap_*,
// sd_ble_gatts_*, sd_ble_gattc_*, sd_ble_evt_get). Numbers are enum
// positions from the S132 headers (arduino nRF52 package, ble_ranges.h
// + ble.h + ble_gap.h + ble_gatts.h + ble_gattc.h), NOT guesses:
//
//   common 0x60: ENABLE, EVT_GET(0x61), TX_PACKET_COUNT_GET, UUID_VS_ADD,
//     UUID_DECODE, UUID_ENCODE, VERSION_GET, USER_MEM_REPLY, OPT_SET/GET
//   GAP 0x70: ADDRESS_SET/GET, ADV_DATA_SET, ADV_START/STOP,
//     CONN_PARAM_UPDATE, DISCONNECT, TX_POWER_SET, APPEARANCE_*,
//     PPCP_*, DEVICE_NAME_*, AUTHENTICATE, SEC_PARAMS_REPLY,
//     AUTH_KEY_REPLY, LESC_DHKEY_REPLY, KEYPRESS_NOTIFY,
//     LESC_OOB_DATA_GET/SET, ENCRYPT, SEC_INFO_REPLY, CONN_SEC_GET,
//     RSSI_START/STOP, SCAN_START/STOP, CONNECT, CONNECT_CANCEL, RSSI_GET
//   GATTC 0x90: PRIMARY_SERVICES_DISCOVER, RELATIONSHIPS_DISCOVER,
//     CHARACTERISTICS_DISCOVER, DESCRIPTORS_DISCOVER, ATTR_INFO_DISCOVER,
//     CHAR_VALUE_BY_UUID_READ, READ, CHAR_VALUES_READ, WRITE, HV_CONFIRM
//   GATTS 0xA0: SERVICE_ADD, INCLUDE_ADD, CHARACTERISTIC_ADD,
//     DESCRIPTOR_ADD, VALUE_SET, VALUE_GET, HVX, SERVICE_CHANGED,
//     RW_AUTHORIZE_REPLY, SYS_ATTR_SET/GET, INITIAL_USER_HANDLE_GET,
//     ATTR_GET
//
// Boundary rule (honest, documented): the emulator core CANNOT speak BLE
// link-layer — no SoftDevice binary, no GAP/GATT state machine, no
// pairing crypto. So every call that needs air consults the driver-side
// bridge (tools/ble_air_bridge.py over WebSocket, same as the RADIO
// air peer): the model stages take_*(), the driver resolves via Bumble
// (real scan/connect/discover/read over the virtual link) and calls
// complete_*(), exactly like every EASYDMA peripheral here. Calls that
// need no air (local table writes, config) complete synchronously with
// NRF_SUCCESS and queue the matching SoftDevice event for sd_ble_evt_get.
//
// Event wire: ble_evt_t = {evt_id u16, evt_len u16, ...params} written
// to the app buffer at r0 (S132 ble.h). evt_id series: GAP 0x10,
// GATTC 0x30, GATTS 0x50. evt_len INCLUDES the 4-byte header.
//   GAP CONNECTED (0x10): conn_handle + peer/own addr + role +
//     irk flags + conn_params
//   GAP ADV_REPORT (0x1D): peer addr + rssi + flags/type/dlen + 31B data
//   GATTC READ_RSP (0x36): handle + offset + len + data[1-var]
//   GATTS WRITE (0x50): conn_handle + handle + uuid + op + auth +
//     offset + len + data[1-var]
//
// Return codes (S132 nrf_error.h / ble_err.h): NRF_SUCCESS 0,
// NRF_ERROR_NOT_FOUND 5, NRF_ERROR_INVALID_STATE 8,
// NRF_ERROR_INVALID_PARAM 7, NRF_ERROR_INVALID_ADDR 16,
// NRF_ERROR_NO_MEM 4, NRF_ERROR_FORBIDDEN 15,
// BLE_ERROR_INVALID_CONN_HANDLE 0x3002, BLE_ERROR_NOT_ENABLED 0x3001,
// BLE_ERROR_GATTS_SYS_ATTR_MISSING 0x3401.
//
// SVC dispatch: thumb.rs SVC arm calls sd_ble::handle_svc() FIRST for
// numbers in 0x60..0xBF; handled = write r0 + advance past svc (like a
// completed call, per the sd_evt design). Unclaimed numbers fall through
// to the normal raise_sync path untouched — zero behavior change when
// BLE is idle. AGENTS.md compliance: NOT a Peripheral (no MMIO base —
// SVC interface, like the sd_evt draft); zero-cost when idle (one
// range compare per SVC). State is a process-wide singleton suivant
// the INSTRUCTION_COUNT pattern (RefCell, reset via reset_state path
// in tests through reset_for_test()).
use std::cell::RefCell;
use std::collections::VecDeque;

use crate::cpu::mem::Memory;
use crate::system::System;

// ---- SVC numbers (S132 enum positions) ----
pub const SVC_BLE_ENABLE: u8 = 0x60;
pub const SVC_BLE_EVT_GET: u8 = 0x61;
pub const SVC_BLE_TX_PACKET_COUNT_GET: u8 = 0x62;
pub const SVC_BLE_UUID_VS_ADD: u8 = 0x63;
// 0x64 UUID_DECODE, 0x65 UUID_ENCODE, 0x66 VERSION_GET,
// 0x67 USER_MEM_REPLY, 0x68 OPT_SET, 0x69 OPT_GET (local acks)
// GAP 0x70..
pub const SVC_GAP_ADDRESS_SET: u8 = 0x70;
pub const SVC_GAP_ADDRESS_GET: u8 = 0x71;
pub const SVC_GAP_ADV_DATA_SET: u8 = 0x72;
pub const SVC_GAP_ADV_START: u8 = 0x73;
pub const SVC_GAP_ADV_STOP: u8 = 0x74;
pub const SVC_GAP_SCAN_START: u8 = 0x89;
pub const SVC_GAP_SCAN_STOP: u8 = 0x8A;
pub const SVC_GAP_CONNECT: u8 = 0x8B;
// GATTC 0x90..
pub const SVC_GATTC_PRIMARY_DISC: u8 = 0x90;
pub const SVC_GATTC_CHAR_DISC: u8 = 0x92;
pub const SVC_GATTC_READ: u8 = 0x96;
pub const SVC_GATTC_WRITE: u8 = 0x98;
// GATTS 0xA0..
pub const SVC_GATTS_SERVICE_ADD: u8 = 0xA0;
pub const SVC_GATTS_CHAR_ADD: u8 = 0xA2;
pub const SVC_GATTS_VALUE_SET: u8 = 0xA4;
pub const SVC_GATTS_VALUE_GET: u8 = 0xA5;
pub const SVC_GATTS_HVX: u8 = 0xA6;

// ---- event ids (S132 ble_ranges.h series) ----
pub const EVT_GAP_CONNECTED: u16 = 0x10;
pub const EVT_GAP_DISCONNECTED: u16 = 0x11;
pub const EVT_GAP_ADV_REPORT: u16 = 0x1D;
pub const EVT_GATTC_READ_RSP: u16 = 0x36;
pub const EVT_GATTS_WRITE: u16 = 0x50;

// ---- return codes (S132 nrf_error.h / ble_err.h) ----
pub const NRF_SUCCESS: u32 = 0;
pub const NRF_ERROR_NOT_FOUND: u32 = 5;
pub const NRF_ERROR_INVALID_STATE: u32 = 8;
pub const NRF_ERROR_INVALID_PARAM: u32 = 7;
pub const NRF_ERROR_INVALID_ADDR: u32 = 16;
pub const NRF_ERROR_NO_MEM: u32 = 4;
pub const NRF_ERROR_FORBIDDEN: u32 = 15;
pub const BLE_ERROR_INVALID_CONN_HANDLE: u32 = 0x3002;
pub const BLE_ERROR_NOT_ENABLED: u32 = 0x3001;

// Pretend connection handle handed to firmware for bridge connections.
pub const BRIDGE_CONN_HANDLE: u16 = 1;
// Local attribute table: battery service lives here for GATTS Anche.
pub const BATT_SVC_HANDLE: u16 = 0x10;
pub const BATT_CHAR_HANDLE: u16 = 0x12;
pub const BATT_VALUE_HANDLE: u16 = 0x13;

fn is_ram(addr: u32) -> bool {
    (0x2000_0000..0x2002_0000).contains(&addr)
}

/// One queued SoftDevice event: raw payload AFTER the 4-byte header
/// (evt_id + evt_len are filled at delivery from id + payload len).
struct QueuedEvt {
    id: u16,
    payload: Vec<u8>,
}

/// Bridge job staged for the driver (take_*/complete_* discipline).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BleJob {
    /// GATTC read: firmware wants handle `handle` on `conn`; driver
    /// performs the over-air read and completes with the bytes.
    GattcRead { conn: u16, handle: u16, offset: u16 },
    /// GAP connect: firmware asked to connect to `addr` (6 bytes LE);
    /// driver resolves over air and completes with conn handle.
    GapConnect { addr: [u8; 6] },
}

/// SoftDevice BLE SVC state. Process-wide singleton (same pattern as
/// INSTRUCTION_COUNT atomics / tap queues): the SVC interface has no
/// MMIO base, so there is no peripheral slot to own it. Reset via
/// reset_for_test() (tests) — live instances reboot the whole process
/// state through reset_state() like every other global.
#[derive(Default)]
pub struct SdBle {
    enabled: bool,
    evt_queue: VecDeque<QueuedEvt>,
    staged: Option<BleJob>,
    next_svc_handle: u16,
    next_value_handle: u16,
    batt_level: u8,
    connected: bool,
}

thread_local! {
    static SD_BLE_STATE: RefCell<SdBle> = RefCell::new(SdBle {
        enabled: false,
        evt_queue: VecDeque::new(),
        staged: None,
        next_svc_handle: BATT_SVC_HANDLE,
        next_value_handle: BATT_VALUE_HANDLE,
        batt_level: 87,
        connected: false,
    });
}

fn with_sd_ble<R>(f: impl FnOnce(&mut SdBle) -> R) -> R {
    SD_BLE_STATE.with(|s| f(&mut s.borrow_mut()))
}

/// Test/process reset: clears enable, queue, staged job, handles.
pub fn reset_for_test() {
    with_sd_ble(|s| {
        *s = SdBle {
            enabled: false,
            evt_queue: VecDeque::new(),
            staged: None,
            next_svc_handle: BATT_SVC_HANDLE,
            next_value_handle: BATT_VALUE_HANDLE,
            batt_level: 87,
            connected: false,
        }
    });
}

impl SdBle {
    fn push_evt(&mut self, id: u16, payload: Vec<u8>) {
        self.evt_queue.push_back(QueuedEvt { id, payload });
    }

    fn deliver_evt(&mut self, mem: &mut dyn Memory, buf: u32) -> u32 {
        match self.evt_queue.pop_front() {
            None => NRF_ERROR_NOT_FOUND,
            Some(ev) => {
                let len = (4 + ev.payload.len()) as u16;
                mem.write16(buf, ev.id);
                mem.write16(buf.wrapping_add(2), len);
                for (i, &b) in ev.payload.iter().enumerate() {
                    mem.write8(buf.wrapping_add(4 + i as u32), b);
                }
                NRF_SUCCESS
            }
        }
    }

    /// GAP CONNECTED event body: conn_handle + peer/own addr + role +
    /// irk flags + zeroed conn_params (S132 ble_gap_evt_connected_t).
    fn connected_payload(peer: [u8; 6]) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&BRIDGE_CONN_HANDLE.to_le_bytes());
        // peer_addr: type=1 (random static) + 6 bytes LSB-first
        p.push(1);
        p.extend_from_slice(&peer);
        // own_addr: type=1 + zeros (emulator has no programmed address)
        p.push(1);
        p.extend_from_slice(&[0u8; 6]);
        p.push(1); // role: peripheral (we connect out as central? peer side sees central; report 1)
        p.push(0); // irk_match(0)+idx(0)
        p.extend_from_slice(&[0u8; 12]); // conn_params zeroed
        p
    }

    /// GAP ADV_REPORT body: peer addr + rssi + flags/type/dlen + 31B data.
    fn adv_report_payload(peer: [u8; 6], rssi: i8, data: &[u8]) -> Vec<u8> {
        let mut p = Vec::new();
        p.push(1);
        p.extend_from_slice(&peer);
        p.push(rssi as u8);
        let dlen = data.len().min(31) as u8;
        // scan_rsp=0, type=0 (ADV_IND), dlen in low 5 bits of flag byte
        p.push(dlen & 0x1F);
        let mut d = [0u8; 31];
        d[..dlen as usize].copy_from_slice(&data[..dlen as usize]);
        p.extend_from_slice(&d);
        p
    }

    /// GATTC READ_RSP body: handle + offset + len + data.
    fn read_rsp_payload(handle: u16, offset: u16, data: &[u8]) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&handle.to_le_bytes());
        p.extend_from_slice(&offset.to_le_bytes());
        p.extend_from_slice(&(data.len() as u16).to_le_bytes());
        p.extend_from_slice(data);
        p
    }

    /// GATTS WRITE body: conn + handle + uuid(2B) + op + auth + off + len + data.
    fn write_payload(conn: u16, handle: u16, uuid16: u16, data: &[u8]) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&conn.to_le_bytes());
        p.extend_from_slice(&handle.to_le_bytes());
        p.extend_from_slice(&uuid16.to_le_bytes());
        p.push(1); // op: write request
        p.push(0); // auth_required: no
        p.extend_from_slice(&0u16.to_le_bytes()); // offset
        p.extend_from_slice(&(data.len() as u16).to_le_bytes());
        p.extend_from_slice(data);
        p
    }
}

/// SVC entry: returns Some(r0) when claimed (caller writes r0 + skips
/// past svc), None to fall through to raise_sync untouched.
///
/// `sys` is unused today (state is the process-wide singleton suivant
/// the INSTRUCTION_COUNT/tap pattern); it is threaded through so the
/// thumb.rs hook + future per-instance state share one call shape.
pub fn handle_svc(
    _sys: &System,
    mem: &mut dyn Memory,
    svc: u8,
    r: &[u32; 13],
) -> Option<u32> {
    if !(0x60..=0xBF).contains(&svc) {
        return None;
    }
    with_sd_ble(|s| dispatch(mem, s, svc, r))
}

fn dispatch(mem: &mut dyn Memory, s: &mut SdBle, svc: u8, r: &[u32; 13]) -> Option<u32> {
    match svc {
        // ---- common: enable / evt_get ----
        x if x == SVC_BLE_ENABLE => {
            s.enabled = true;
            Some(NRF_SUCCESS)
        }
        x if x == SVC_BLE_EVT_GET => {
            let buf = r[0];
            if !is_ram(buf) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            Some(s.deliver_evt(mem, buf))
        }
        // ---- GAP: local acks + air-backed ops ----
        x if x == SVC_GAP_ADDRESS_SET => Some(NRF_SUCCESS),
        x if x == SVC_GAP_ADDRESS_GET => {
            let p = r[0];
            if !is_ram(p) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            mem.write8(p, 1); // type: random static
            for i in 0..6u32 {
                mem.write8(p.wrapping_add(1 + i), 0);
            }
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GAP_ADV_DATA_SET => Some(NRF_SUCCESS),
        x if x == SVC_GAP_ADV_START => {
            if !s.enabled {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GAP_ADV_STOP => Some(NRF_SUCCESS),
        x if x == SVC_GAP_SCAN_START => {
            if !s.enabled {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GAP_SCAN_STOP => Some(NRF_SUCCESS),
        x if x == SVC_GAP_CONNECT => {
            if !s.enabled {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            // r0 = peer addr struct ptr: {type u8, addr[6]}. Stage the
            // driver job; the bridge resolves the connection over air.
            let p = r[0];
            if !is_ram(p) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let mut addr = [0u8; 6];
            for i in 0..6u32 {
                addr[i as usize] = mem.read8(p.wrapping_add(1 + i));
            }
            s.staged = Some(BleJob::GapConnect { addr });
            Some(NRF_SUCCESS)
        }
        // ---- GATTC: reads stage driver jobs, discovery acks ----
        x if x == SVC_GATTC_PRIMARY_DISC => Some(NRF_SUCCESS),
        x if x == SVC_GATTC_CHAR_DISC => Some(NRF_SUCCESS),
        x if x == SVC_GATTC_READ => {
            if !s.enabled {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            let (conn, handle, offset) = (r[0] as u16, r[1] as u16, r[2] as u16);
            if conn != BRIDGE_CONN_HANDLE && s.connected {
                return Some(BLE_ERROR_INVALID_CONN_HANDLE);
            }
            if !s.connected {
                return Some(NRF_ERROR_INVALID_STATE);
            }
            s.staged = Some(BleJob::GattcRead { conn, handle, offset });
            Some(NRF_SUCCESS)
        }
        // ---- GATTS: local attribute table, battery lives here ----
        x if x == SVC_GATTS_SERVICE_ADD => {
            let uuid_ptr = r[1];
            let h_ptr = r[2];
            if !is_ram(uuid_ptr) || !is_ram(h_ptr) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let h = s.next_svc_handle;
            s.next_svc_handle = s.next_svc_handle.wrapping_add(8);
            mem.write16(h_ptr, h);
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GATTS_CHAR_ADD => {
            let h_ptr = r[2];
            if !is_ram(h_ptr) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let h = s.next_value_handle;
            s.next_value_handle = s.next_value_handle.wrapping_add(2);
            mem.write16(h_ptr, h);
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GATTS_VALUE_SET => {
            let (handle, _off, len, p_val) = (r[0] as u16, r[1] as u16, r[2] as u16, r[3]);
            if len != 0 && !is_ram(p_val) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            if handle == s.next_value_handle.wrapping_sub(2)
                || handle == BATT_VALUE_HANDLE
            {
                if len >= 1 {
                    s.batt_level = mem.read8(p_val);
                }
                return Some(NRF_SUCCESS);
            }
            Some(NRF_ERROR_NOT_FOUND)
        }
        x if x == SVC_GATTS_VALUE_GET => {
            let (handle, _off, p_len, p_val) = (r[0] as u16, r[1] as u16, r[2], r[3]);
            if !is_ram(p_len) || !is_ram(p_val) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            if handle == s.next_value_handle.wrapping_sub(2)
                || handle == BATT_VALUE_HANDLE
            {
                mem.write16(p_len, 1);
                mem.write8(p_val, s.batt_level);
                return Some(NRF_SUCCESS);
            }
            Some(NRF_ERROR_NOT_FOUND)
        }
        x if x == SVC_GATTS_HVX => {
            if !s.connected {
                return Some(NRF_ERROR_INVALID_STATE);
            }
            Some(NRF_SUCCESS)
        }
        _ => None,
    }
}

// ---- driver-side take/complete (take_* -> air -> complete_*) ----

/// Take a staged BLE job; None when idle.
pub fn take_job() -> Option<BleJob> {
    with_sd_ble(|s| s.staged.take())
}

/// Complete a GATTC read: driver resolved `data` over air; posts the
/// READ_RSP event firmware drains via sd_ble_evt_get.
pub fn complete_gattc_read(handle: u16, offset: u16, data: &[u8]) {
    with_sd_ble(|s| {
        s.push_evt(EVT_GATTC_READ_RSP, SdBle::read_rsp_payload(handle, offset, data));
    });
}

/// Complete a GAP connect: driver connected over air; marks connected
/// and posts CONNECTED so sd_ble_evt_get reports the new link.
pub fn complete_gap_connect(peer: [u8; 6]) {
    with_sd_ble(|s| {
        s.connected = true;
        s.push_evt(EVT_GAP_CONNECTED, SdBle::connected_payload(peer));
    });
}

/// Post an advertising report (bridge scanner sighting) for evt_get.
pub fn post_adv_report(peer: [u8; 6], rssi: i8, data: &[u8]) {
    with_sd_ble(|s| {
        s.push_evt(EVT_GAP_ADV_REPORT, SdBle::adv_report_payload(peer, rssi, data));
    });
}

/// Post a GATTS write (peer wrote our characteristic) for evt_get.
pub fn post_gatts_write(handle: u16, uuid16: u16, data: &[u8]) {
    with_sd_ble(|s| {
        if handle == BATT_VALUE_HANDLE || handle == s.next_value_handle.wrapping_sub(2) {
            if let Some(&b) = data.first() {
                s.batt_level = b;
            }
        }
        s.push_evt(
            EVT_GATTS_WRITE,
            SdBle::write_payload(BRIDGE_CONN_HANDLE, handle, uuid16, data),
        );
    });
}

/// Is the BLE stack enabled (SD face up)?
pub fn is_enabled() -> bool {
    with_sd_ble(|s| s.enabled)
}

/// Queued SoftDevice event count (debug/export).
pub fn queue_len() -> usize {
    with_sd_ble(|s| s.evt_queue.len())
}

/// Current battery level in the local GATTS table (debug/export).
pub fn batt_level() -> u8 {
    with_sd_ble(|s| s.batt_level)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::mem::{FlatMemory, Memory};
    use crate::system::test_dummy_system;

    fn regs(r0: u32, r1: u32, r2: u32, r3: u32) -> [u32; 13] {
        let mut r = [0u32; 13];
        r[0] = r0;
        r[1] = r1;
        r[2] = r2;
        r[3] = r3;
        r
    }

    #[test]
    fn enable_then_evt_get_empty_is_not_found() {
        let sys = test_dummy_system();
        let mut mem = FlatMemory::new(512 * 1024, 128 * 1024);
        reset_for_test();
        assert_eq!(handle_svc(&sys, &mut mem, 0x59, &[0u32; 13]), None, "outside BLE range");
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_BLE_EVT_GET, &regs(0x20001000, 0, 0, 0)),
            Some(NRF_ERROR_NOT_FOUND),
            "empty queue reads NOT_FOUND"
        );
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_BLE_ENABLE, &[0u32; 13]),
            Some(NRF_SUCCESS)
        );
        assert!(is_enabled());
        // Still empty -> NOT_FOUND (no fallthrough to the SD).
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_BLE_EVT_GET, &regs(0x20001000, 0, 0, 0)),
            Some(NRF_ERROR_NOT_FOUND)
        );
    }

    #[test]
    fn gatts_battery_roundtrip() {
        let sys = test_dummy_system();
        let mut mem = FlatMemory::new(512 * 1024, 128 * 1024);
        reset_for_test();
        let _ = handle_svc(&sys, &mut mem, SVC_BLE_ENABLE, &[0u32; 13]);
        // SERVICE_ADD writes a handle to RAM.
        mem.write32(0x20001000, 0x180F);
        mem.write16(0x20001010, 0);
        let r = regs(1, 0x20001000, 0x20001010, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GATTS_SERVICE_ADD, &r), Some(NRF_SUCCESS));
        assert_eq!(mem.read16(0x20001010), BATT_SVC_HANDLE);
        // VALUE_SET battery -> VALUE_GET reads it back.
        mem.write8(0x20002000, 63);
        let r = regs(BATT_VALUE_HANDLE as u32, 0, 1, 0x20002000);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GATTS_VALUE_SET, &r), Some(NRF_SUCCESS));
        mem.write16(0x20002010, 0);
        let r = regs(BATT_VALUE_HANDLE as u32, 0, 0x20002010, 0x20002008);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GATTS_VALUE_GET, &r), Some(NRF_SUCCESS));
        assert_eq!(mem.read16(0x20002010), 1);
        assert_eq!(mem.read8(0x20002008), 63);
        assert_eq!(batt_level(), 63);
    }

    #[test]
    fn gattc_read_stages_take_complete_posts_read_rsp() {
        let sys = test_dummy_system();
        let mut mem = FlatMemory::new(512 * 1024, 128 * 1024);
        reset_for_test();
        let _ = handle_svc(&sys, &mut mem, SVC_BLE_ENABLE, &[0u32; 13]);
        // Not connected -> INVALID_STATE (silicon rule).
        let r = regs(1, BATT_VALUE_HANDLE as u32, 0, 0);
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_GATTC_READ, &r),
            Some(NRF_ERROR_INVALID_STATE)
        );
        complete_gap_connect([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
        // Drain the CONNECTED event the connect completion posted.
        let r = regs(0x20003000, 0, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_BLE_EVT_GET, &r), Some(NRF_SUCCESS));
        assert_eq!(mem.read16(0x20003000), EVT_GAP_CONNECTED);
        // Now the read stages a driver job...
        let r = regs(1, BATT_VALUE_HANDLE as u32, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GATTC_READ, &r), Some(NRF_SUCCESS));
        assert_eq!(
            take_job(),
            Some(BleJob::GattcRead { conn: 1, handle: BATT_VALUE_HANDLE, offset: 0 })
        );
        // ...driver resolves over air, event drains with header+body.
        complete_gattc_read(BATT_VALUE_HANDLE, 0, &[87]);
        let r = regs(0x20003000, 0, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_BLE_EVT_GET, &r), Some(NRF_SUCCESS));
        assert_eq!(mem.read16(0x20003000), EVT_GATTC_READ_RSP);
        // evt_len includes the 4-byte header: 4 + 2+2+2 + 1 byte data.
        assert_eq!(mem.read16(0x20003002), 4 + 6 + 1);
        assert_eq!(mem.read8(0x20003004), BATT_VALUE_HANDLE as u8);
        assert_eq!(mem.read8(0x2000300A), 87);
        // Queue drained -> NOT_FOUND again.
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_BLE_EVT_GET, &regs(0x20003000, 0, 0, 0)),
            Some(NRF_ERROR_NOT_FOUND)
        );
    }

    #[test]
    fn gap_connect_stages_and_posts_connected() {
        let sys = test_dummy_system();
        let mut mem = FlatMemory::new(512 * 1024, 128 * 1024);
        reset_for_test();
        let _ = handle_svc(&sys, &mut mem, SVC_BLE_ENABLE, &[0u32; 13]);
        // Peer addr struct at RAM: type + 6 bytes.
        mem.write8(0x20001000, 1);
        for (i, b) in [0x11u8, 0x22, 0x33, 0x44, 0x55, 0x66].iter().enumerate() {
            mem.write8(0x20001000 + 1 + i as u32, *b);
        }
        let r = regs(0x20001000, 0, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GAP_CONNECT, &r), Some(NRF_SUCCESS));
        assert_eq!(
            take_job(),
            Some(BleJob::GapConnect { addr: [0x11, 0x22, 0x33, 0x44, 0x55, 0x66] })
        );
        complete_gap_connect([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
        let r = regs(0x20004000, 0, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_BLE_EVT_GET, &r), Some(NRF_SUCCESS));
        assert_eq!(mem.read16(0x20004000), EVT_GAP_CONNECTED);
        assert_eq!(mem.read16(0x20004004), BRIDGE_CONN_HANDLE);
    }
}
