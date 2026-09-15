// SoftDevice BLE SVC service: GAP + GATTS + GATTC over the Bumble air bridge.
//
// SCOPE: this is the "same GATT treatment for the SoftDevice stack
// bluetooth" slice. The RADIO register model (radio_nrf.rs) stays the
// bare-metal 802.15.4-ish face; THIS module is the SVC face — the calls
// BLE firmware actually makes (sd_ble_enable, sd_ble_gap_*,
// sd_ble_gatts_*, sd_ble_gattc_*, sd_ble_evt_get). Every number below
// is verified against the S132 headers in the arduino nRF52 package
// (ble_ranges.h + ble.h + ble_gap.h + ble_gatts.h + ble_gattc.h +
// ble_gatt.h + ble_types.h + nrf_error.h + ble_err.h), NOT guessed:
//
// SVC bases (ble_ranges.h): common 0x60 (12: ENABLE..OPT_GET),
//   reserved 0x6C, GAP 0x70 (32), GATTC 0x90 (32), GATTS 0xA0 (16),
//   L2CAP 0xB0 (16 — unclaimed here, falls through to raise_sync).
//   Common enum (ble.h): ENABLE=0x60, EVT_GET=0x61,
//   TX_PACKET_COUNT_GET=0x62, UUID_VS_ADD=0x63, UUID_DECODE=0x64,
//   UUID_ENCODE=0x65, VERSION_GET=0x66, USER_MEM_REPLY=0x67,
//   OPT_SET=0x68, OPT_GET=0x69.
//   GAP enum (ble_gap.h): ADDRESS_SET=0x70, ADDRESS_GET=0x71,
//   ADV_DATA_SET=0x72, ADV_START=0x73, ADV_STOP=0x74,
//   CONN_PARAM_UPDATE=0x75, DISCONNECT=0x76, TX_POWER_SET=0x77,
//   APPEARANCE_SET=0x78, APPEARANCE_GET=0x79, PPCP_SET=0x7A,
//   PPCP_GET=0x7B, DEVICE_NAME_SET=0x7C, DEVICE_NAME_GET=0x7D,
//   AUTHENTICATE=0x7E, SEC_PARAMS_REPLY=0x7F, AUTH_KEY_REPLY=0x80,
//   LESC_DHKEY_REPLY=0x81, KEYPRESS_NOTIFY=0x82, LESC_OOB_DATA_GET=0x83,
//   LESC_OOB_DATA_SET=0x84, ENCRYPT=0x85, SEC_INFO_REPLY=0x86,
//   CONN_SEC_GET=0x87, RSSI_START=0x88, RSSI_STOP=0x89,
//   SCAN_START=0x8A, SCAN_STOP=0x8B, CONNECT=0x8C, CONNECT_CANCEL=0x8D,
//   RSSI_GET=0x8E.
//   GATTC enum (ble_gattc.h): PRIMARY_SERVICES_DISCOVER=0x90,
//   RELATIONSHIPS_DISCOVER=0x91, CHARACTERISTICS_DISCOVER=0x92,
//   DESCRIPTORS_DISCOVER=0x93, ATTR_INFO_DISCOVER=0x94,
//   CHAR_VALUE_BY_UUID_READ=0x95, READ=0x96, CHAR_VALUES_READ=0x97,
//   WRITE=0x98, HV_CONFIRM=0x99.
//   GATTS enum (ble_gatts.h): SERVICE_ADD=0xA0, INCLUDE_ADD=0xA1,
//   CHARACTERISTIC_ADD=0xA2, DESCRIPTOR_ADD=0xA3, VALUE_SET=0xA4,
//   VALUE_GET=0xA5, HVX=0xA6, SERVICE_CHANGED=0xA7,
//   RW_AUTHORIZE_REPLY=0xA8, SYS_ATTR_SET=0xA9, SYS_ATTR_GET=0xAA,
//   INITIAL_USER_HANDLE_GET=0xAB, ATTR_GET=0xAC.
//
// Signatures that matter (SVCALL lines, register contract r0..r3):
//   sd_ble_enable(ble_enable_params_t*, u32 *app_ram_base) — both
//     pointers may be NULL (dev use *app_ram_base=0 for sizing).
//   sd_ble_evt_get(u8 *dest, u16 *len) — TWO args (not one!): dest
//     NULL + len queries the pending length; dest set drains.
//   sd_ble_gap_address_get(ble_gap_addr_t*) — {u8 type, u8[6] addr LSB}.
//   sd_ble_gap_adv_start(ble_gap_adv_params_t*) — NULL = defaults.
//   sd_ble_gap_disconnect(conn, hci_code) — posts DISCONNECTED.
//   sd_ble_gap_rssi_get(conn, *rssi, *ch_index=NULL-ok).
//   sd_ble_gattc_read(conn, handle, offset) — three plain u16s.
//   sd_ble_gattc_write(conn, ble_gattc_write_params_t*) — struct
//     {u8 op, u8 flags, u16 handle, u16 offset, u16 len, u8 *value}.
//   sd_ble_gatts_value_set/get(conn, handle, ble_gatts_value_t*) —
//     struct {u16 len, u16 offset, u8 *value}; conn may be
//     BLE_CONN_HANDLE_INVALID (0xFFFF) for non-system attrs.
//   sd_ble_gatts_hvx(conn, ble_gatts_hvx_params_t*) — struct
//     {u16 handle, u8 type, u16 offset, u16 *len, u8 *data}.
//
// Boundary rule (honest, documented): the emulator core CANNOT speak BLE
// link-layer — no SoftDevice binary, no GAP/GATT state machine, no
// pairing crypto. So every call that needs air consults the driver-side
// bridge (tools/ble_air_bridge.py over WebSocket, same as the RADIO
// air peer): the model stages take_*(), the driver resolves via Bumble
// (real scan/connect/discover/read/write over the virtual link) and
// calls complete_*(), exactly like every EASYDMA peripheral here. Calls
// that need no air (local table writes, config) complete synchronously
// with NRF_SUCCESS and queue the matching SoftDevice event for
// sd_ble_evt_get.
//
// Event wire (ble.h: ble_evt_t = {u16 evt_id, u16 evt_len incl.
// header, union{common,gap,l2cap,gattc,gatts}}):
//   GAP CONNECTED (0x10): ble_gap_evt_t = {u16 conn_handle,
//     ble_gap_evt_connected_t = {peer_addr{type+6}, own_addr{type+6},
//     u8 role, u8 irk_match:1+idx:7, conn_params{6xu16}}}
//   GAP DISCONNECTED (0x11): {u16 conn_handle, u8 reason}
//   GAP ADV_REPORT (0x1D): {u16 0xFFFF (no conn),
//     {peer{type+6}, i8 rssi, u8 scan_rsp:1+type:2+dlen:5, u8[31] data}}
//   GAP RSSI_CHANGED (0x1C): {u16 conn_handle, i8 rssi}
//   GATTC PRIM_SRVC_DISC_RSP (0x30): {conn, gatt_status, error_handle,
//     u16 count, ble_gattc_service_t[count] = {uuid{16+8}, start, end}}
//   GATTC CHAR_DISC_RSP (0x32): {conn, status, err, count,
//     ble_gattc_char_t[count] = {uuid{16+8}, props u8, ext:8,
//     decl_handle, value_handle}} — props u8 then ext byte.
//   GATTC READ_RSP (0x36): {conn, status, err,
//     {u16 handle, u16 offset, u16 len, u8 data[]}}
//   GATTC WRITE_RSP (0x38): {conn, status, err,
//     {u16 handle, u8 op, u16 offset, u16 len, u8 data[]}}
//   GATTC HVX (0x39): {conn, status, err,
//     {u16 handle, u8 type, u16 len, u8 data[]}}
//   GATTS WRITE (0x50): {u16 conn, {u16 handle, uuid{16+8}, u8 op,
//     u8 auth, u16 offset, u16 len, u8 data[]}}
//   GATTS HVC (0x53): {u16 conn, {u16 handle}}
// Layout note: S132 structs are UNPACKED (no pragma pack in headers —
// verified by grep): u8 bitfields pack into their u8, but u16 fields
// align to even offsets, so e.g. adv_report's flag byte sits at +10
// after peer(7)+rssi(1)+1 pad, data at +12. adv_report dlen<=31.
// Return codes (nrf_error.h / ble_err.h): NRF_SUCCESS 0,
// NRF_ERROR_NOT_FOUND 5, NRF_ERROR_INVALID_STATE 8,
// NRF_ERROR_INVALID_PARAM 7, NRF_ERROR_INVALID_ADDR 16,
// NRF_ERROR_NO_MEM 4, NRF_ERROR_FORBIDDEN 15, NRF_ERROR_BUSY 17,
// NRF_ERROR_DATA_SIZE 12, BLE_ERROR_INVALID_CONN_HANDLE 0x3002,
// BLE_ERROR_NOT_ENABLED 0x3001, BLE_ERROR_NO_TX_PACKETS 0x3004,
// BLE_ERROR_GATTS_SYS_ATTR_MISSING 0x3401.
// UUID types (ble_types.h): UNKNOWN 0, BLE(sig) 1, VENDOR_BEGIN 2.
// Write ops (ble_gatt.h): INVALID 0, WRITE_REQ 1, WRITE_CMD 2,
//   SIGNED_WRITE 3, PREP_WRITE 4, EXEC_WRITE 5.
// HVX types: INVALID 0, NOTIFICATION 1, INDICATION 2.
// Roles (ble_gap.h): INVALID 0, PERIPH 1, CENTRAL 2 — we report
//   CENTRAL (we initiate the bridge connection).
// GATT (ble_gatt.h): HANDLE_INVALID 0, STATUS_SUCCESS 0.
// Conn (ble_types.h): CONN_HANDLE_INVALID 0xFFFF.
//
// SVC dispatch: thumb.rs SVC arm calls sd_ble::handle_svc() FIRST for
// numbers in 0x60..=0xBF; handled = write r0 + advance past svc (like a
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
pub const SVC_BLE_UUID_DECODE: u8 = 0x64;
pub const SVC_BLE_UUID_ENCODE: u8 = 0x65;
pub const SVC_BLE_VERSION_GET: u8 = 0x66;
pub const SVC_BLE_USER_MEM_REPLY: u8 = 0x67;
pub const SVC_BLE_OPT_SET: u8 = 0x68;
pub const SVC_BLE_OPT_GET: u8 = 0x69;
// GAP 0x70.. (ble_gap.h enum order: ADDRESS_SET=0x70 ... RSSI_GET=0x8E)
pub const SVC_GAP_ADDRESS_SET: u8 = 0x70;
pub const SVC_GAP_ADDRESS_GET: u8 = 0x71;
pub const SVC_GAP_ADV_DATA_SET: u8 = 0x72;
pub const SVC_GAP_ADV_START: u8 = 0x73;
pub const SVC_GAP_ADV_STOP: u8 = 0x74;
pub const SVC_GAP_CONN_PARAM_UPDATE: u8 = 0x75;
pub const SVC_GAP_DISCONNECT: u8 = 0x76;
pub const SVC_GAP_TX_POWER_SET: u8 = 0x77;
pub const SVC_GAP_APPEARANCE_SET: u8 = 0x78;
pub const SVC_GAP_APPEARANCE_GET: u8 = 0x79;
pub const SVC_GAP_PPCP_SET: u8 = 0x7A;
pub const SVC_GAP_PPCP_GET: u8 = 0x7B;
pub const SVC_GAP_DEVICE_NAME_SET: u8 = 0x7C;
pub const SVC_GAP_DEVICE_NAME_GET: u8 = 0x7D;
pub const SVC_GAP_AUTHENTICATE: u8 = 0x7E;
pub const SVC_GAP_SEC_PARAMS_REPLY: u8 = 0x7F;
pub const SVC_GAP_AUTH_KEY_REPLY: u8 = 0x80;
pub const SVC_GAP_LESC_DHKEY_REPLY: u8 = 0x81;
pub const SVC_GAP_KEYPRESS_NOTIFY: u8 = 0x82;
pub const SVC_GAP_LESC_OOB_DATA_GET: u8 = 0x83;
pub const SVC_GAP_LESC_OOB_DATA_SET: u8 = 0x84;
pub const SVC_GAP_ENCRYPT: u8 = 0x85;
pub const SVC_GAP_SEC_INFO_REPLY: u8 = 0x86;
pub const SVC_GAP_CONN_SEC_GET: u8 = 0x87;
pub const SVC_GAP_RSSI_START: u8 = 0x88;
pub const SVC_GAP_RSSI_STOP: u8 = 0x89;
pub const SVC_GAP_SCAN_START: u8 = 0x8A;
pub const SVC_GAP_SCAN_STOP: u8 = 0x8B;
pub const SVC_GAP_CONNECT: u8 = 0x8C;
pub const SVC_GAP_CONNECT_CANCEL: u8 = 0x8D;
pub const SVC_GAP_RSSI_GET: u8 = 0x8E;
// GATTC 0x90.. (ble_gattc.h enum order)
pub const SVC_GATTC_PRIMARY_DISC: u8 = 0x90;
pub const SVC_GATTC_REL_DISC: u8 = 0x91;
pub const SVC_GATTC_CHAR_DISC: u8 = 0x92;
pub const SVC_GATTC_DESC_DISC: u8 = 0x93;
pub const SVC_GATTC_ATTR_INFO_DISC: u8 = 0x94;
pub const SVC_GATTC_UUID_READ: u8 = 0x95;
pub const SVC_GATTC_READ: u8 = 0x96;
pub const SVC_GATTC_CHAR_VALS_READ: u8 = 0x97;
pub const SVC_GATTC_WRITE: u8 = 0x98;
pub const SVC_GATTC_HV_CONFIRM: u8 = 0x99;
// GATTS 0xA0..
pub const SVC_GATTS_SERVICE_ADD: u8 = 0xA0;
pub const SVC_GATTS_INCLUDE_ADD: u8 = 0xA1;
pub const SVC_GATTS_CHAR_ADD: u8 = 0xA2;
pub const SVC_GATTS_DESC_ADD: u8 = 0xA3;
pub const SVC_GATTS_VALUE_SET: u8 = 0xA4;
pub const SVC_GATTS_VALUE_GET: u8 = 0xA5;
pub const SVC_GATTS_HVX: u8 = 0xA6;
pub const SVC_GATTS_SERVICE_CHANGED: u8 = 0xA7;
pub const SVC_GATTS_RW_AUTHORIZE_REPLY: u8 = 0xA8;
pub const SVC_GATTS_SYS_ATTR_SET: u8 = 0xA9;
pub const SVC_GATTS_SYS_ATTR_GET: u8 = 0xAA;
pub const SVC_GATTS_INITIAL_USER_HANDLE_GET: u8 = 0xAB;
pub const SVC_GATTS_ATTR_GET: u8 = 0xAC;
// L2CAP 0xB0.. (ble_l2cap.h enum order)
pub const SVC_L2CAP_CID_REGISTER: u8 = 0xB0;
pub const SVC_L2CAP_CID_UNREGISTER: u8 = 0xB1;
pub const SVC_L2CAP_TX: u8 = 0xB2;

// ---- GAP peer-initiated security event ids (S132 ble_gap.h series;
// the numeric values follow from BLE_GAP_EVT_BASE 0x10 + enum order:
// CONNECTED 0x10, DISCONNECTED 0x11, CONN_PARAM_UPDATE 0x12,
// SEC_PARAMS_REQUEST 0x13, SEC_INFO_REQUEST 0x14, PASSKEY_DISPLAY 0x15,
// KEY_PRESSED 0x16, AUTH_KEY_REQUEST 0x17, LESC_DHKEY_REQUEST 0x18,
// AUTH_STATUS 0x19, CONN_SEC_UPDATE 0x1A, TIMEOUT 0x1B, ...) ----
pub const EVT_GAP_CONNECTED: u16 = 0x10;
pub const EVT_GAP_DISCONNECTED: u16 = 0x11;
pub const EVT_GAP_SEC_PARAMS_REQUEST: u16 = 0x13;
pub const EVT_GAP_SEC_INFO_REQUEST: u16 = 0x14;
pub const EVT_GAP_PASSKEY_DISPLAY: u16 = 0x15;
pub const EVT_GAP_KEY_PRESSED: u16 = 0x16;
pub const EVT_GAP_AUTH_KEY_REQUEST: u16 = 0x17;
pub const EVT_GAP_LESC_DHKEY_REQUEST: u16 = 0x18;
pub const EVT_GAP_AUTH_STATUS: u16 = 0x19;
pub const EVT_GAP_CONN_SEC_UPDATE: u16 = 0x1A;
pub const EVT_GAP_RSSI_CHANGED: u16 = 0x1C;
pub const EVT_GAP_ADV_REPORT: u16 = 0x1D;
pub const EVT_GATTC_PRIM_DISC_RSP: u16 = 0x30;
pub const EVT_GATTC_REL_DISC_RSP: u16 = 0x31;
pub const EVT_GATTC_CHAR_DISC_RSP: u16 = 0x32;
pub const EVT_GATTC_DESC_DISC_RSP: u16 = 0x33;
pub const EVT_GATTC_ATTR_INFO_RSP: u16 = 0x34;
pub const EVT_GATTC_UUID_READ_RSP: u16 = 0x35;
pub const EVT_GATTC_READ_RSP: u16 = 0x36;
pub const EVT_GATTC_VALS_READ_RSP: u16 = 0x37;
pub const EVT_GATTC_WRITE_RSP: u16 = 0x38;
pub const EVT_GATTC_HVX: u16 = 0x39;
pub const EVT_GATTS_WRITE: u16 = 0x50;
pub const EVT_GATTS_HVC: u16 = 0x53;
pub const EVT_L2CAP_RX: u16 = 0x70;

// ---- return codes (S132 nrf_error.h / ble_err.h) ----
pub const NRF_SUCCESS: u32 = 0;
pub const NRF_ERROR_NOT_FOUND: u32 = 5;
pub const NRF_ERROR_NOT_SUPPORTED: u32 = 6;
pub const NRF_ERROR_INVALID_STATE: u32 = 8;
pub const NRF_ERROR_INVALID_PARAM: u32 = 7;
pub const NRF_ERROR_INVALID_ADDR: u32 = 16;
pub const NRF_ERROR_NO_MEM: u32 = 4;
pub const NRF_ERROR_FORBIDDEN: u32 = 15;
pub const NRF_ERROR_BUSY: u32 = 17;
pub const NRF_ERROR_DATA_SIZE: u32 = 12;
pub const BLE_ERROR_INVALID_CONN_HANDLE: u32 = 0x3002;
pub const BLE_ERROR_NOT_ENABLED: u32 = 0x3001;
pub const BLE_ERROR_NO_TX_PACKETS: u32 = 0x3004;
pub const BLE_ERROR_L2CAP_CID_IN_USE: u32 = 0x3100;
pub const BLE_CONN_HANDLE_INVALID: u16 = 0xFFFF;
// L2CAP CIDs (ble_l2cap.h): dynamic range + MTU floor.
pub const L2CAP_CID_DYN_BASE: u16 = 0x0040;
pub const L2CAP_CID_DYN_MAX: u16 = 8;
pub const L2CAP_MTU_DEF: u16 = 23;
// Security status codes (ble_gap.h SEC_STATUS): the passkey/OOB/
// encrypt/auth handshake maps silicon's pair-fail codes onto the
// AUTH_STATUS firmware drains (see Pairing).
pub const SEC_STATUS_SUCCESS: u8 = 0x00;
pub const SEC_STATUS_PASSKEY_ENTRY_FAILED: u8 = 0x81;
pub const SEC_STATUS_OOB_NOT_AVAILABLE: u8 = 0x82;
pub const SEC_STATUS_AUTH_REQ: u8 = 0x83;
pub const SEC_STATUS_CONFIRM_VALUE: u8 = 0x84;
pub const SEC_STATUS_PAIRING_NOT_SUPP: u8 = 0x85;
// NOTE: the old code spelled this 0x29 (wrong series — a BLE_ATT
// error, not a GAP SEC_STATUS). SEC_PARAMS_REPLY reject used to emit
// it; fixed alongside the handshake work below.

// Roles (ble_gap.h): we initiate the bridge connection -> CENTRAL.
pub const GAP_ROLE_CENTRAL: u8 = 2;
// UUID types (ble_types.h).
pub const UUID_TYPE_BLE: u8 = 1;
pub const UUID_TYPE_VENDOR_BEGIN: u8 = 2;
// Write ops (ble_gatt.h) / HVX types.
pub const GATT_OP_WRITE_REQ: u8 = 1;
pub const GATT_OP_WRITE_CMD: u8 = 2;
pub const GATT_HVX_NOTIFICATION: u8 = 1;
pub const GATT_HVX_INDICATION: u8 = 2;
// GATT status / handle sentinels (ble_gatt.h).
pub const GATT_STATUS_SUCCESS: u16 = 0;
pub const GATT_HANDLE_INVALID: u16 = 0;

// Pretend connection handle handed to firmware for bridge connections.
pub const BRIDGE_CONN_HANDLE: u16 = 1;
// Local attribute table: battery service lives here for GATTS Anche.
pub const BATT_SVC_HANDLE: u16 = 0x10;
pub const BATT_CHAR_HANDLE: u16 = 0x12;
pub const BATT_VALUE_HANDLE: u16 = 0x13;

fn is_ram(addr: u32) -> bool {
    (0x2000_0000..0x2002_0000).contains(&addr)
}

/// One queued SoftDevice event: evt_id + evt_len + raw params.
/// evt_len (the u16 at buf+2) INCLUDES the 4-byte header (S132 ble.h),
/// so the pump must size firmware buffers from it, not from payload.
struct QueuedEvt {
    id: u16,
    payload: Vec<u8>,
}

/// GATT discovery data staged for a completion: the bridge walks the
/// live peer table and the model formats the S132 event structs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscService {
    pub uuid16: Option<u16>,
    pub start: u16,
    pub end: u16,
}

/// Characteristic row for CHAR_DISC_RSP: S132 ble_gattc_char_t =
/// {uuid{16+8}, props u8, ext byte, decl_handle, value_handle}.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscChar {
    pub uuid16: Option<u16>,
    pub props: u8,
    pub decl: u16,
    pub value: u16,
}

/// Descriptor row for DESC_DISC_RSP: ble_gattc_desc_t = {handle, uuid}.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscDesc {
    pub handle: u16,
    pub uuid16: Option<u16>,
}

/// Include row for REL_DISC_RSP: ble_gattc_include_t = {handle,
/// included service {uuid, start, end}} (ble_gattc.h, unpacked).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscInclude {
    pub handle: u16,
    pub uuid16: Option<u16>,
    pub start: u16,
    pub end: u16,
}

/// Attribute-info row: ble_gattc_attr_info_t = {handle, uuid16|uuid128}.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscAttrInfo {
    pub handle: u16,
    pub uuid16: Option<u16>,
}

/// Handle-value pair for UUID_READ_RSP: ble_gattc_handle_value_t.
/// The S132 wire is {handle u16, value bytes}; value_len is shared.
/// (The C struct carries a *pointer*; the event array inlines bytes.)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandleValue {
    pub handle: u16,
    pub value: Vec<u8>,
}

/// Bridge job staged for the driver (take_*/complete_* discipline).
///firmware SVCs stage exactly one job; the driver drains it via
/// ble_take_job(), resolves over air, and completes with the matching
/// complete_* — same discipline as every EASYDMA peripheral here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BleJob {
    /// GAP connect (SVC 0x8C): r0 = *ble_gap_addr_t {type, 6B LE}.
    /// Driver connects over air, completes with the conn handle.
    GapConnect { addr: [u8; 6] },
    /// GAP disconnect (SVC 0x76): (conn, hci reason). Driver tears the
    /// air link down, completes with DISCONNECTED.
    GapDisconnect { conn: u16, reason: u8 },
    /// GAP RSSI get (SVC 0x8E): conn. Driver samples air RSSI.
    GapRssiGet { conn: u16 },
    /// GAP scan start (SVC 0x8A): driver runs one live sighting,
    /// completes with an ADV_REPORT.
    GapScanStart,
    /// GATTC primary-service discovery (SVC 0x90): (conn, start, uuid
    /// or none). Driver walks the peer table over air.
    GattcPrimDisc { conn: u16, start: u16, uuid16: Option<u16> },
    /// GATTC characteristic discovery (SVC 0x92): (conn, start, end).
    GattcCharDisc { conn: u16, start: u16, end: u16 },
    /// GATTC descriptor discovery (SVC 0x93): (conn, start, end).
    GattcDescDisc { conn: u16, start: u16, end: u16 },
    /// GATTC relationship discovery (SVC 0x91): (conn, start, end).
    /// Driver walks includes over air; completes with REL_DISC_RSP.
    GattcRelDisc { conn: u16, start: u16, end: u16 },
    /// GATTC attribute-info discovery (SVC 0x94): (conn, start, end).
    /// Driver walks the table over air; completes ATTR_INFO_RSP.
    GattcAttrInfoDisc { conn: u16, start: u16, end: u16 },
    /// GATTC read-by-UUID (SVC 0x95): (conn, uuid|none, start, end).
    /// Driver resolves every matching handle over air; completes with
    /// UUID_READ_RSP (handle,value pairs sharing one value_len).
    GattcUuidRead { conn: u16, uuid16: Option<u16>, start: u16, end: u16 },
    /// GATTC multi-read (SVC 0x97): (conn, handles[]); driver reads
    /// each over air; completes with VALS_READ_RSP (concatenated).
    GattcValsRead { conn: u16, handles: Vec<u16> },
    /// GATTC read (SVC 0x96): (conn, handle, offset); driver reads
    /// over air and completes with the bytes.
    GattcRead { conn: u16, handle: u16, offset: u16 },
    /// GATTC write (SVC 0x98): struct {op, flags, handle, offset,
    /// len, *value}; driver writes over air, completes WRITE_RSP.
    GattcWrite { conn: u16, op: u8, handle: u16, data: Vec<u8> },
    /// GATTS HVX notify/indicate (SVC 0xA6): struct {handle, type,
    /// offset, *len, *data}; driver emits over air, completes HVC.
    GattsHvx { conn: u16, handle: u16, hvx_type: u8, data: Vec<u8> },
    /// L2CAP TX (SVC 0xB2): (conn, cid, bytes); driver moves the frame
    /// over air on the registered CID, completes by echoing RX (loopback
    /// legibility: same shape as the RADIO air echo).
    L2capTx { conn: u16, cid: u16, data: Vec<u8> },
    /// GAP authenticate (SVC 0x7E): (conn). Driver runs the pairing
    /// handshake over air (bridge confirms); reply SVCs complete it.
    /// (Peer-initiated security needs no job: the driver posts
    /// SEC_PARAMS_REQUEST / AUTH_KEY_REQUEST / ... events directly via
    /// post_sec_params_request / post_auth_key_request / ..., and
    /// firmware answers with the reply SVCs.)
    GapAuthenticate { conn: u16 },
}

/// Local GATTS attribute-table entry. Handles mirror the SoftDevice
/// allocator: service decl, then per characteristic decl+value (+CCCD
/// when the peer can subscribe). VALUE_SET/GET read and write `value`;
/// HVX sends it; peer writes via post_gatts_write update it + queue.
/// `subscribed` tracks per-link CCCD writes (bit0 notify, bit1
/// indicate per 0x2902 semantics); notify/indicate HVX on a link
/// without the matching bit refuses like silicon (0x0100-class GATT
/// error would surface as BLE_ERROR_GATTS_SYS_ATTR_MISSING on real
/// stacks — here INVALID_STATE, documented).
#[derive(Clone, Debug, Default)]
struct Attr {
    handle: u16,
    uuid16: Option<u16>,
    value: Vec<u8>,
    cccd: bool,
    cccd_handle: u16,
    subscribed: Vec<(u16, u8)>,
}

/// One link. S132 supports several concurrent connections; the old
/// code had a single global (connected/conn_handle/peer/rssi). Every
/// per-link field lives here now; the GATTS table stays global (server
/// side, shared across links like silicon).
#[derive(Clone, Debug, Default)]
struct Conn {
    handle: u16,
    up: bool,
    peer_addr: [u8; 6],
    role: u8,
    rssi_dbm: i8,
    tx_count: u8,
    encrypted: bool,
    bonded: bool,
    pairing: Pairing,
    cids: Vec<u16>,
}

/// Pairing state per link (S132-observable behavior; no crypto, no
/// key storage — the emulator core cannot do SMP, so the driver-side
/// bridge confirms the air handshake and the reply SVCs below complete
/// or fail it, posting AUTH_STATUS + CONN_SEC_UPDATE like silicon).
/// Idle = no procedure. Requested = AUTHENTICATE staged locally,
/// driver resolves. PeerRequested = the PEER started pairing: the
/// driver posted SEC_PARAMS_REQUEST / AUTH_KEY_REQUEST / ... and
/// firmware must answer with SEC_PARAMS_REPLY / AUTH_KEY_REPLY / ....
/// Accepted = firmware accepted (SEC_PARAMS_REPLY with params, or an
/// AUTH_KEY/DHKEY reply); the driver handshake completes it. KeyEntry
/// = AUTH_KEY_REQUEST outstanding (passkey/OOB expected). LescDhkey =
/// LESC_DHKEY_REQUEST outstanding. EncryptPending = SEC_INFO_REQUEST
/// answered with keys (SEC_INFO_REPLY non-NULL), ENCRYPT expected.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum Pairing {
    #[default]
    Idle,
    Requested,
    PeerRequested,
    Accepted,
    KeyEntry { key_type: u8 },
    LescDhkey { oobd_req: bool },
    EncryptPending,
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
    attrs: Vec<Attr>,
    next_handle: u16,
    vs_uuids: Vec<u8>,
    batt_level: u8,
    conns: Vec<Conn>,
    next_conn: u16,
    own_addr: [u8; 6],
    rssi_dbm: i8,
    tx_count: u8,
    app_ram_base: u32,
    l2cap_cids: Vec<u16>,
}

impl Conn {
    fn fresh(handle: u16, peer: [u8; 6]) -> Self {
        Conn {
            handle,
            up: true,
            peer_addr: peer,
            role: GAP_ROLE_CENTRAL,
            rssi_dbm: -50,
            tx_count: 4,
            encrypted: false,
            bonded: false,
            pairing: Pairing::Idle,
            cids: Vec::new(),
        }
    }
}

impl SdBle {
    fn fresh() -> Self {
        SdBle {
            enabled: false,
            evt_queue: VecDeque::new(),
            staged: None,
            attrs: Vec::new(),
            next_handle: 0x10,
            vs_uuids: Vec::new(),
            batt_level: 87,
            conns: Vec::new(),
            next_conn: BRIDGE_CONN_HANDLE,
            own_addr: [0u8; 6],
            rssi_dbm: -50,
            tx_count: 4,
            app_ram_base: 0x2000_2000,
            l2cap_cids: Vec::new(),
        }
    }

    fn conn(&self, h: u16) -> Option<&Conn> {
        self.conns.iter().find(|c| c.handle == h && c.up)
    }

    fn conn_mut(&mut self, h: u16) -> Option<&mut Conn> {
        self.conns.iter_mut().find(|c| c.handle == h && c.up)
    }

    /// First live link (legacy single-conn callers: RSSI sync answer,
    /// loopback pump). None when no link is up.
    fn first_conn(&self) -> Option<u16> {
        self.conns.iter().find(|c| c.up).map(|c| c.handle)
    }

    /// Legacy compat: the old `connected` bool (any link up).
    fn connected(&self) -> bool {
        self.conns.iter().any(|c| c.up)
    }

    /// Legacy compat: old single `conn_handle` (first live link).
    fn conn_handle(&self) -> u16 {
        self.first_conn().unwrap_or(BRIDGE_CONN_HANDLE)
    }

    /// Legacy compat: old single peer_addr (first live link).
    fn peer_addr(&self) -> [u8; 6] {
        self.conns.iter().find(|c| c.up).map(|c| c.peer_addr).unwrap_or([0u8; 6])
    }

    fn find_attr(&self, handle: u16) -> Option<&Attr> {
        self.attrs.iter().find(|a| a.handle == handle)
    }

    fn find_attr_mut(&mut self, handle: u16) -> Option<&mut Attr> {
        self.attrs.iter_mut().find(|a| a.handle == handle)
    }

    fn check_conn(&self, conn: u16) -> Result<(), u32> {
        if !self.enabled {
            return Err(BLE_ERROR_NOT_ENABLED);
        }
        if conn == BLE_CONN_HANDLE_INVALID {
            return Err(BLE_ERROR_INVALID_CONN_HANDLE);
        }
        match self.conn(conn) {
            Some(_) => Ok(()),
            None => {
                // Unknown handle, or a down link: silicon says
                // INVALID_CONN_HANDLE for a never-handle, INVALID_STATE
                // when no link is up at all.
                if self.conns.iter().any(|c| c.handle == conn) {
                    Err(NRF_ERROR_INVALID_STATE)
                } else if self.connected() {
                    Err(BLE_ERROR_INVALID_CONN_HANDLE)
                } else {
                    Err(NRF_ERROR_INVALID_STATE)
                }
            }
        }
    }

    /// Allocate the next connection handle (wraps, skips live + INVALID).
    fn alloc_conn(&mut self) -> u16 {
        for _ in 0..0xFFFE {
            let h = self.next_conn;
            self.next_conn = self.next_conn.wrapping_add(1);
            if self.next_conn == BLE_CONN_HANDLE_INVALID || self.next_conn == 0 {
                self.next_conn = 1;
            }
            if h != BLE_CONN_HANDLE_INVALID && h != 0 && self.conn(h).is_none() {
                return h;
            }
        }
        BLE_CONN_HANDLE_INVALID
    }

    /// Bring a link up (driver completed GAP connect over air).
    fn link_up(&mut self, peer: [u8; 6]) -> u16 {
        let h = self.alloc_conn();
        if h == BLE_CONN_HANDLE_INVALID {
            return h;
        }
        let mut c = Conn::fresh(h, peer);
        c.rssi_dbm = self.rssi_dbm;
        c.tx_count = self.tx_count;
        self.conns.push(c);
        h
    }

    /// Tear a link down. Returns the HCI reason echo (unchanged).
    fn link_down(&mut self, handle: u16) -> bool {
        match self.conn_mut(handle) {
            Some(c) => {
                c.up = false;
                c.pairing = Pairing::Idle;
                true
            }
            None => false,
        }
    }
}

thread_local! {
    static SD_BLE_STATE: RefCell<SdBle> = RefCell::new(SdBle::fresh());
}

fn with_sd_ble<R>(f: impl FnOnce(&mut SdBle) -> R) -> R {
    SD_BLE_STATE.with(|s| f(&mut s.borrow_mut()))
}

/// Test/process reset: clears enable, queue, staged job, table.
pub fn reset_for_test() {
    with_sd_ble(|s| {
        *s = SdBle::fresh();
    });
}

impl SdBle {
    fn push_evt(&mut self, id: u16, payload: Vec<u8>) {
        self.evt_queue.push_back(QueuedEvt { id, payload });
    }

    /// Drain one event into the firmware buffer. S132 contract
    /// (sd_ble_evt_get SVCALL): r0 = *dest, r1 = *len (in: buffer size
    /// incl. header; out: event size incl. header). NULL dest queries
    /// the pending length (needs a RAM *len); NULL *len with a real
    /// dest is a legacy single-arg shape we still drain. Small buffer
    /// (dest set, *len < need) returns DATA_SIZE without popping.
    /// Empty queue (any shape) returns NOT_FOUND. Non-RAM pointers
    /// for the parts actually used return INVALID_ADDR.
    fn evt_get(&mut self, mem: &mut dyn Memory, dest: u32, p_len: u32) -> u32 {
        let front_need = match self.evt_queue.front() {
            None => return NRF_ERROR_NOT_FOUND,
            Some(ev) => 4 + ev.payload.len() as u16,
        };
        if dest == 0 {
            // Length query needs a RAM *len; NULL *len = no contract.
            if !is_ram(p_len) {
                return NRF_ERROR_INVALID_ADDR;
            }
            mem.write16(p_len, front_need);
            return NRF_SUCCESS;
        }
        if !is_ram(dest) {
            return NRF_ERROR_INVALID_ADDR;
        }
        // Legacy single-arg drain (dest set, *len NULL): pop the full
        // event when the queue is non-empty (pre-two-arg firmware).
        if p_len == 0 {
            let ev = self.evt_queue.pop_front().expect("front checked");
            mem.write16(dest, ev.id);
            mem.write16(dest.wrapping_add(2), front_need);
            for (i, &b) in ev.payload.iter().enumerate() {
                mem.write8(dest.wrapping_add(4 + i as u32), b);
            }
            return NRF_SUCCESS;
        }
        if !is_ram(p_len) {
            return NRF_ERROR_INVALID_ADDR;
        }
        let room = mem.read16(p_len);
        if room < front_need {
            return NRF_ERROR_DATA_SIZE;
        }
        let ev = self.evt_queue.pop_front().expect("front checked");
        mem.write16(dest, ev.id);
        mem.write16(dest.wrapping_add(2), front_need);
        for (i, &b) in ev.payload.iter().enumerate() {
            mem.write8(dest.wrapping_add(4 + i as u32), b);
        }
        mem.write16(p_len, front_need);
        NRF_SUCCESS
    }

    /// GAP CONNECTED body: ble_gap_evt_t = {u16 conn,
    /// connected{peer{type+6}, own{type+6}, role, irk byte,
    /// conn_params{4xu16 min/max/lat/timeout}}}. Role CENTRAL: we
    /// initiate the bridge connection (ble_gap.h roles).
    fn connected_payload(conn: u16, peer: [u8; 6], own: [u8; 6]) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&conn.to_le_bytes());
        p.push(1); // peer type: random static
        p.extend_from_slice(&peer);
        p.push(1); // own type: random static
        p.extend_from_slice(&own);
        p.push(GAP_ROLE_CENTRAL);
        p.push(0); // irk_match 0 + idx 0
        // conn_params: min/max interval 6 (7.5ms), latency 0, timeout 400.
        for w in [6u16, 6, 0, 400] {
            p.extend_from_slice(&w.to_le_bytes());
        }
        p
    }

    /// GAP DISCONNECTED body: {u16 conn, u8 reason} (ble_gap.h).
    fn disconnected_payload(conn: u16, reason: u8) -> Vec<u8> {
        vec![(conn & 0xFF) as u8, (conn >> 8) as u8, reason]
    }

    /// GAP RSSI_CHANGED body: {u16 conn, i8 rssi} (ble_gap.h).
    fn rssi_changed_payload(conn: u16, rssi: i8) -> Vec<u8> {
        vec![(conn & 0xFF) as u8, (conn >> 8) as u8, rssi as u8]
    }

    /// GAP ADV_REPORT body (ble_gap.h, unpacked layout!): {u16 0xFFFF
    /// (no connection), peer{type+6}, i8 rssi, u8 bitfield, 1 PAD byte
    /// (u16 alignment — no pragma pack in S132 headers, verified), then
    /// u8[31] data}. Flag byte: scan_rsp bit0, adv type bits1-2,
    /// dlen bits3-7 (dlen<=31).
    fn adv_report_payload(peer: [u8; 6], rssi: i8, scan_rsp: bool, data: &[u8]) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&BLE_CONN_HANDLE_INVALID.to_le_bytes());
        p.push(1); // peer type: random static
        p.extend_from_slice(&peer);
        p.push(rssi as u8);
        let dlen = data.len().min(31) as u8;
        p.push(((scan_rsp as u8) & 1) | (dlen << 3));
        p.push(0); // pad: data[31] must u16-align (S132 unpacked)
        let mut d = [0u8; 31];
        d[..dlen as usize].copy_from_slice(&data[..dlen as usize]);
        p.extend_from_slice(&d);
        p
    }

    /// GATTC envelope: {u16 conn, u16 gatt_status, u16 error_handle}
    /// (ble_gattc.h ble_gattc_evt_t head). Success => status 0,
    /// error_handle HANDLE_INVALID.
    fn gattc_head(conn: u16) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&conn.to_le_bytes());
        p.extend_from_slice(&GATT_STATUS_SUCCESS.to_le_bytes());
        p.extend_from_slice(&GATT_HANDLE_INVALID.to_le_bytes());
        p
    }

    /// GATTC PRIM_SRVC_DISC_RSP params: {u16 count,
    /// ble_gattc_service_t[count] = {uuid u16, type u8, pad u8,
    /// start u16, end u16}} (ble_uuid_t + handle range, unpacked).
    fn prim_disc_payload(conn: u16, svcs: &[DiscService]) -> Vec<u8> {
        let mut p = Self::gattc_head(conn);
        p.extend_from_slice(&(svcs.len() as u16).to_le_bytes());
        for s in svcs {
            let uuid = s.uuid16.unwrap_or(0);
            p.extend_from_slice(&uuid.to_le_bytes());
            p.push(UUID_TYPE_BLE);
            p.push(0);
            p.extend_from_slice(&s.start.to_le_bytes());
            p.extend_from_slice(&s.end.to_le_bytes());
        }
        p
    }

    /// GATTC CHAR_DISC_RSP params: {count, ble_gattc_char_t[count] =
    /// {uuid u16, type u8, pad, props u8, ext byte, decl u16, value
    /// u16}} — props u8 then ext-present byte (ble_gatt.h,
    /// ble_gattc.h; 128-bit chars encode uuid 0 here +ATTR_INFO path).
    fn char_disc_payload(conn: u16, chars: &[DiscChar]) -> Vec<u8> {
        let mut p = Self::gattc_head(conn);
        p.extend_from_slice(&(chars.len() as u16).to_le_bytes());
        for c in chars {
            p.extend_from_slice(&c.uuid16.unwrap_or(0).to_le_bytes());
            p.push(UUID_TYPE_BLE);
            p.push(0);
            p.push(c.props);
            p.push(0); // char_ext_props absent
            p.extend_from_slice(&c.decl.to_le_bytes());
            p.extend_from_slice(&c.value.to_le_bytes());
        }
        p
    }

    /// GATTC DESC_DISC_RSP params: {count, {handle u16, uuid u16,
    /// type u8, pad}[]}.
    fn desc_disc_payload(conn: u16, descs: &[DiscDesc]) -> Vec<u8> {
        let mut p = Self::gattc_head(conn);
        p.extend_from_slice(&(descs.len() as u16).to_le_bytes());
        for d in descs {
            p.extend_from_slice(&d.handle.to_le_bytes());
            p.extend_from_slice(&d.uuid16.unwrap_or(0).to_le_bytes());
            p.push(UUID_TYPE_BLE);
            p.push(0);
        }
        p
    }

    /// GATTC REL_DISC_RSP params: {count, ble_gattc_include_t[count] =
    /// {handle u16, service {uuid u16, type u8, pad, start u16, end
    /// u16}}}.
    fn rel_disc_payload(conn: u16, incs: &[DiscInclude]) -> Vec<u8> {
        let mut p = Self::gattc_head(conn);
        p.extend_from_slice(&(incs.len() as u16).to_le_bytes());
        for r in incs {
            p.extend_from_slice(&r.handle.to_le_bytes());
            p.extend_from_slice(&r.uuid16.unwrap_or(0).to_le_bytes());
            p.push(UUID_TYPE_BLE);
            p.push(0);
            p.extend_from_slice(&r.start.to_le_bytes());
            p.extend_from_slice(&r.end.to_le_bytes());
        }
        p
    }

    /// GATTC ATTR_INFO_DISC_RSP params: {count u16, format u8 (1 =
    /// 16-bit), ble_gattc_attr_info_t[count] = {handle u16, uuid u16,
    /// type u8, pad}}. 128-bit rows would switch format to 2 with 16B
    /// UUIDs — the bridge reports SIG tables, so format is always 1.
    fn attr_info_payload(conn: u16, infos: &[DiscAttrInfo]) -> Vec<u8> {
        let mut p = Self::gattc_head(conn);
        p.extend_from_slice(&(infos.len() as u16).to_le_bytes());
        p.push(1); // BLE_GATTC_ATTR_INFO_FORMAT_16BIT
        for a in infos {
            p.extend_from_slice(&a.handle.to_le_bytes());
            p.extend_from_slice(&a.uuid16.unwrap_or(0).to_le_bytes());
            p.push(UUID_TYPE_BLE);
            p.push(0);
        }
        p
    }

    /// GATTC UUID_READ_RSP params: {count u16, value_len u16,
    /// {handle u16, value[value_len]}[count]}. All pairs share one
    /// value_len (S132); ragged values are padded with zeros to the
    /// longest (documented; silicon requires uniform lengths).
    fn uuid_read_payload(conn: u16, pairs: &[HandleValue]) -> Vec<u8> {
        let mut p = Self::gattc_head(conn);
        p.extend_from_slice(&(pairs.len() as u16).to_le_bytes());
        let vlen = pairs.iter().map(|x| x.value.len()).max().unwrap_or(0) as u16;
        p.extend_from_slice(&vlen.to_le_bytes());
        for hv in pairs {
            p.extend_from_slice(&hv.handle.to_le_bytes());
            let mut v = hv.value.clone();
            v.resize(vlen as usize, 0);
            p.extend_from_slice(&v);
        }
        p
    }

    /// GATTC VALS_READ_RSP params: {len u16, values[]}.
    fn vals_read_payload(conn: u16, data: &[u8]) -> Vec<u8> {
        let mut p = Self::gattc_head(conn);
        p.extend_from_slice(&(data.len() as u16).to_le_bytes());
        p.extend_from_slice(data);
        p
    }

    /// GATTC READ_RSP params: {handle u16, offset u16, len u16, data[]}.
    fn read_rsp_payload(conn: u16, handle: u16, offset: u16, data: &[u8]) -> Vec<u8> {
        let mut p = Self::gattc_head(conn);
        p.extend_from_slice(&handle.to_le_bytes());
        p.extend_from_slice(&offset.to_le_bytes());
        p.extend_from_slice(&(data.len() as u16).to_le_bytes());
        p.extend_from_slice(data);
        p
    }

    /// GATTC WRITE_RSP params (ble_gattc.h, unpacked!): {handle u16,
    /// op u8, PAD u8 (u16-aligns offset), offset u16, len u16, data[]}.
    fn write_rsp_payload(conn: u16, handle: u16, op: u8, offset: u16, data: &[u8]) -> Vec<u8> {
        let mut p = Self::gattc_head(conn);
        p.extend_from_slice(&handle.to_le_bytes());
        p.push(op);
        p.push(0);
        p.extend_from_slice(&offset.to_le_bytes());
        p.extend_from_slice(&(data.len() as u16).to_le_bytes());
        p.extend_from_slice(data);
        p
    }

    /// GATTC HVX params (ble_gattc.h, unpacked!): {handle u16, type
    /// u8, PAD u8 (u16-aligns len), len u16, data[]}.
    fn hvx_payload(conn: u16, handle: u16, hvx_type: u8, data: &[u8]) -> Vec<u8> {
        let mut p = Self::gattc_head(conn);
        p.extend_from_slice(&handle.to_le_bytes());
        p.push(hvx_type);
        p.push(0);
        p.extend_from_slice(&(data.len() as u16).to_le_bytes());
        p.extend_from_slice(data);
        p
    }

    /// GATTS WRITE body (ble_gatts.h): {u16 conn, {handle u16,
    /// uuid{16+8}, op u8, auth u8, offset u16, len u16, data[]}}.
    /// 128-bit attrs encode uuid 0 + type VENDOR_BEGIN.
    fn gatts_write_payload(conn: u16, handle: u16, uuid16: Option<u16>, op: u8, data: &[u8]) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&conn.to_le_bytes());
        p.extend_from_slice(&handle.to_le_bytes());
        match uuid16 {
            Some(u) => {
                p.extend_from_slice(&u.to_le_bytes());
                p.push(UUID_TYPE_BLE);
            }
            None => {
                p.extend_from_slice(&0u16.to_le_bytes());
                p.push(UUID_TYPE_VENDOR_BEGIN);
            }
        }
        p.push(op);
        p.push(0); // auth_required: no
        p.extend_from_slice(&0u16.to_le_bytes()); // offset
        p.extend_from_slice(&(data.len() as u16).to_le_bytes());
        p.extend_from_slice(data);
        p
    }

    /// GATTS HVC body: {u16 conn, u16 handle} (ble_gatts.h).
    fn hvc_payload(conn: u16, handle: u16) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&conn.to_le_bytes());
        p.extend_from_slice(&handle.to_le_bytes());
        p
    }

    /// GAP AUTH_STATUS body (ble_gap.h): {conn u16, auth_status u8,
    /// err_src:2 + bonded:1 packed u8, sm1 levels u8, sm2 levels u8,
    /// kdist_own u8, kdist_peer u8}. Conn FIRST (ble_gap_evt_t head),
    /// then the ble_gap_evt_auth_status_t params. Levels byte: sec_mode
    /// bit0 + encr key size hi.
    fn auth_status_payload(conn: u16, status: u8, bonded: bool) -> Vec<u8> {
        let mut p = Vec::with_capacity(8);
        p.extend_from_slice(&conn.to_le_bytes());
        p.push(status);
        p.push(if bonded { 0x04 } else { 0x00 });
        p.push(0x01); // sm1: mode1 level1 (open link, like our stub)
        p.push(0x01); // sm2: level1
        p.push(0x00); // kdist_own: nothing exchanged
        p.push(0x00); // kdist_peer: nothing exchanged
        p
    }

    /// GAP SEC_PARAMS_REQUEST body (ble_gap.h): {conn u16,
    /// ble_gap_sec_params_t peer_params (5B: flags, min/max key size,
    /// kdist_own, kdist_peer)}.
    fn sec_params_request_payload(conn: u16, peer_params: &[u8; 5]) -> Vec<u8> {
        let mut p = Vec::with_capacity(7);
        p.extend_from_slice(&conn.to_le_bytes());
        p.extend_from_slice(peer_params);
        p
    }

    /// GAP SEC_INFO_REQUEST body (ble_gap.h): {conn u16, peer_addr 7B,
    /// master_id 10B (ediv u16 + rand[8]), req-bits u8 (bit0 enc_info,
    /// bit1 id_info, bit2 sign_info)}.
    fn sec_info_request_payload(conn: u16, peer_addr: &[u8; 7], master_id: &[u8; 10], req: u8) -> Vec<u8> {
        let mut p = Vec::with_capacity(20);
        p.extend_from_slice(&conn.to_le_bytes());
        p.extend_from_slice(peer_addr);
        p.extend_from_slice(master_id);
        p.push(req & 0x07);
        p
    }

    /// GAP PASSKEY_DISPLAY body (ble_gap.h): {conn u16, passkey[6]
    /// ASCII, match_request bit0 u8}.
    fn passkey_display_payload(conn: u16, passkey: &[u8; 6], match_request: bool) -> Vec<u8> {
        let mut p = Vec::with_capacity(9);
        p.extend_from_slice(&conn.to_le_bytes());
        p.extend_from_slice(passkey);
        p.push(if match_request { 0x01 } else { 0x00 });
        p
    }

    /// GAP KEY_PRESSED body (ble_gap.h): {conn u16, kp_not u8}.
    fn key_pressed_payload(conn: u16, kp_not: u8) -> Vec<u8> {
        let mut p = Vec::with_capacity(3);
        p.extend_from_slice(&conn.to_le_bytes());
        p.push(kp_not);
        p
    }

    /// GAP AUTH_KEY_REQUEST body (ble_gap.h): {conn u16, key_type u8}.
    fn auth_key_request_payload(conn: u16, key_type: u8) -> Vec<u8> {
        let mut p = Vec::with_capacity(3);
        p.extend_from_slice(&conn.to_le_bytes());
        p.push(key_type);
        p
    }

    /// GAP LESC_DHKEY_REQUEST body (ble_gap.h): {conn u16, oobd_req u8}.
    /// (The S132 struct carries a *pointer* to the peer public key in
    /// app-supplied keyset memory; the event on the wire names only the
    /// link + OOB requirement — the key bytes move via the reply SVC.)
    fn lesc_dhkey_request_payload(conn: u16, oobd_req: bool) -> Vec<u8> {
        let mut p = Vec::with_capacity(3);
        p.extend_from_slice(&conn.to_le_bytes());
        p.push(if oobd_req { 0x01 } else { 0x00 });
        p
    }

    /// GAP CONN_SEC_UPDATE body (ble_gap.h): {sec_mode u8, key_size u8}.
    /// Mode byte packs sec_mode (open = 1: mode1 level1).
    fn conn_sec_payload() -> Vec<u8> {
        vec![0x11, 0x10] // mode1 level1, 16-octet key size
    }

    /// L2CAP RX body (ble_l2cap.h): {u16 conn, len u16, cid u16, data[]}.
    fn l2cap_rx_payload(conn: u16, cid: u16, data: &[u8]) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&conn.to_le_bytes());
        p.extend_from_slice(&(data.len() as u16).to_le_bytes());
        p.extend_from_slice(&cid.to_le_bytes());
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
    // GATT helpers live here (not methods) so dispatch stays a flat
    // match: each arm reads r0..r3, validates RAM, and either answers
    // synchronously or stages exactly one BleJob for the driver.
    fn read_u16_le(mem: &mut dyn Memory, p: u32) -> u16 {
        (mem.read8(p) as u16) | ((mem.read8(p.wrapping_add(1)) as u16) << 8)
    }
    fn write_u16_le(mem: &mut dyn Memory, p: u32, v: u16) {
        mem.write8(p, (v & 0xFF) as u8);
        mem.write8(p.wrapping_add(1), (v >> 8) as u8);
    }
    fn read_uuid(mem: &mut dyn Memory, p: u32) -> Option<u16> {
        // ble_uuid_t = {u16 uuid, u8 type}: SIG read, vendor->None.
        let uuid = read_u16_le(mem, p);
        match mem.read8(p.wrapping_add(2)) {
            UUID_TYPE_BLE => Some(uuid),
            _ => None,
        }
    }
    fn require_enabled(s: &SdBle) -> Result<(), u32> {
        if s.enabled {
            Ok(())
        } else {
            Err(BLE_ERROR_NOT_ENABLED)
        }
    }
    match svc {
        // ---- common ----
        x if x == SVC_BLE_ENABLE => {
            // sd_ble_enable(params*, *app_ram_base): both may be NULL.
            // Report our RAM floor through *app_ram_base when given.
            let base_ptr = r[1];
            if base_ptr != 0 {
                if !is_ram(base_ptr) {
                    return Some(NRF_ERROR_INVALID_ADDR);
                }
                let want = mem.read32(base_ptr);
                if want < s.app_ram_base {
                    mem.write32(base_ptr, s.app_ram_base);
                }
            }
            s.enabled = true;
            Some(NRF_SUCCESS)
        }
        x if x == SVC_BLE_EVT_GET => {
            // sd_ble_evt_get(dest, *len): either may be NULL, but a
            // non-RAM pointer where an address is expected is INVALID.
            let (dest, p_len) = (r[0], r[1]);
            if dest != 0 && !is_ram(dest) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            if p_len != 0 && !is_ram(p_len) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            Some(s.evt_get(mem, dest, p_len))
        }
        x if x == SVC_BLE_TX_PACKET_COUNT_GET => {
            // (conn, *count): per-link free packet budget.
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            let (conn, p_count) = (r[0] as u16, r[1]);
            if let Err(e) = s.check_conn(conn) {
                return Some(e);
            }
            if !is_ram(p_count) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            mem.write8(p_count, s.conn(conn).map(|c| c.tx_count).unwrap_or(0));
            Some(NRF_SUCCESS)
        }
        x if x == SVC_BLE_UUID_VS_ADD => {
            // (*uuid128, *type): hand out vendor type ids from 2 up.
            let (p_uuid, p_type) = (r[0], r[1]);
            if !is_ram(p_uuid) || !is_ram(p_type) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let t = UUID_TYPE_VENDOR_BEGIN + s.vs_uuids.len() as u8;
            s.vs_uuids.push(t);
            mem.write8(p_type, t);
            Some(NRF_SUCCESS)
        }
        x if x == SVC_BLE_UUID_DECODE => {
            // (len, *le_bytes, *ble_uuid): 2-byte LE -> SIG uuid.
            let (len, p_le, p_uuid) = (r[0], r[1], r[2]);
            if (len != 0 && !is_ram(p_le)) || !is_ram(p_uuid) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            if len == 2 {
                let v = read_u16_le(mem, p_le);
                write_u16_le(mem, p_uuid, v);
                mem.write8(p_uuid.wrapping_add(2), UUID_TYPE_BLE);
                Some(NRF_SUCCESS)
            } else {
                // 16-byte vendor decode needs a registered base: look
                // the 12/13 octets up against nothing here -> NOT_FOUND
                // unless a VS base exists (then VENDOR_BEGIN).
                if s.vs_uuids.is_empty() {
                    Some(NRF_ERROR_NOT_FOUND)
                } else {
                    write_u16_le(mem, p_uuid, 0);
                    mem.write8(p_uuid.wrapping_add(2), s.vs_uuids[0]);
                    Some(NRF_SUCCESS)
                }
            }
        }
        x if x == SVC_BLE_UUID_ENCODE => {
            // (*ble_uuid, *len, *le_bytes): SIG -> 2 LE bytes.
            let (p_uuid, p_len, p_le) = (r[0], r[1], r[2]);
            if !is_ram(p_uuid) || !is_ram(p_len) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let uuid = read_u16_le(mem, p_uuid);
            mem.write16(p_len, 2);
            if p_le != 0 {
                if !is_ram(p_le) {
                    return Some(NRF_ERROR_INVALID_ADDR);
                }
                write_u16_le(mem, p_le, uuid);
            }
            Some(NRF_SUCCESS)
        }
        x if x == SVC_BLE_VERSION_GET => {
            // (*ble_version_t = {ver u8, company u16, subver u16}).
            let p = r[0];
            if !is_ram(p) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            mem.write8(p, 7); // LL version 7 (BT 4.1, like S132)
            mem.write16(p.wrapping_add(1), 0x0059); // Nordic company id
            mem.write16(p.wrapping_add(3), 0x0100); // FWID-ish subversion
            Some(NRF_SUCCESS)
        }
        x if x == SVC_BLE_USER_MEM_REPLY => {
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            let conn = r[0] as u16;
            if s.check_conn(conn).is_err() {
                return Some(BLE_ERROR_INVALID_CONN_HANDLE);
            }
            Some(NRF_SUCCESS)
        }
        x if x == SVC_BLE_OPT_SET || x == SVC_BLE_OPT_GET => Some(NRF_SUCCESS),
        // ---- GAP ----
        x if x == SVC_GAP_ADDRESS_SET => {
            // (cycle_mode r0, *addr r1): store our address for CONNECTED.
            let (mode, p) = (r[0], r[1]);
            if mode > 2 {
                return Some(NRF_ERROR_INVALID_PARAM);
            }
            if !is_ram(p) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            for i in 0..6u32 {
                s.own_addr[i as usize] = mem.read8(p.wrapping_add(1 + i));
            }
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GAP_ADDRESS_GET => {
            let p = r[0];
            if !is_ram(p) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            mem.write8(p, 1); // type: random static
            for i in 0..6u32 {
                mem.write8(p.wrapping_add(1 + i), s.own_addr[i as usize]);
            }
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GAP_ADV_DATA_SET => {
            // (data, dlen, sr_data, srdlen): regs r0..r3, both <= 31.
            if r[1] > 31 || r[3] > 31 {
                return Some(NRF_ERROR_DATA_SIZE);
            }
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GAP_ADV_START => {
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            // r0 = *adv_params or NULL (defaults). No event on start;
            // connections arrive via CONNECTED like silicon.
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GAP_ADV_STOP => Some(NRF_SUCCESS),
        x if x == SVC_GAP_CONN_PARAM_UPDATE => {
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            let conn = r[0] as u16;
            if s.check_conn(conn).is_err() {
                return Some(BLE_ERROR_INVALID_CONN_HANDLE);
            }
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GAP_DISCONNECT => {
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            let (conn, reason) = (r[0] as u16, r[1] as u8);
            if s.check_conn(conn).is_err() {
                return Some(BLE_ERROR_INVALID_CONN_HANDLE);
            }
            s.staged = Some(BleJob::GapDisconnect { conn, reason });
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GAP_TX_POWER_SET => Some(NRF_SUCCESS),
        x if x == SVC_GAP_APPEARANCE_SET || x == SVC_GAP_APPEARANCE_GET => {
            let p = r[0];
            if x == SVC_GAP_APPEARANCE_GET {
                if !is_ram(p) {
                    return Some(NRF_ERROR_INVALID_ADDR);
                }
                mem.write16(p, 0); // UNKNOWN appearance
            }
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GAP_PPCP_SET || x == SVC_GAP_PPCP_GET => Some(NRF_SUCCESS),
        x if x == SVC_GAP_DEVICE_NAME_SET => {
            if r[2] > 248 {
                return Some(NRF_ERROR_DATA_SIZE);
            }
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GAP_DEVICE_NAME_GET => {
            // (*buf, *len): report empty name, len 0 out.
            let (p_buf, p_len) = (r[0], r[1]);
            if !is_ram(p_len) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            if p_buf != 0 && !is_ram(p_buf) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            mem.write16(p_len, 0);
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GAP_AUTHENTICATE => {
            // Initiate pairing: stage the handshake; the driver runs it
            // over air (bridge confirms) and completes via
            // complete_pairing / fail_pairing (AUTH_STATUS + SEC_UPDATE).
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            let conn = r[0] as u16;
            if let Err(e) = s.check_conn(conn) {
                return Some(e);
            }
            if let Some(c) = s.conn_mut(conn) {
                if c.pairing != Pairing::Idle {
                    return Some(NRF_ERROR_BUSY);
                }
                c.pairing = Pairing::Requested;
            }
            s.staged = Some(BleJob::GapAuthenticate { conn });
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GAP_SEC_PARAMS_REPLY => {
            // Reply to SEC_PARAMS_REQUEST (peer-initiated pairing): NULL
            // params (or nonzero sec_status) = reject, AUTH_STATUS
            // pair-fail, link stays up. Non-NULL params = accept: with
            // an outstanding peer request the link moves to Accepted and
            // the driver handshake completes it (AUTH_STATUS success +
            // CONN_SEC_UPDATE via complete_pairing). With NO request
            // outstanding there is nothing to reply to (silicon
            // INVALID_STATE). p_sec_keyset (r3) only names key memory;
            // never dereferenced (no key storage — documented).
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            let (conn, status, p_params) = (r[0] as u16, r[1] as u8, r[2]);
            if let Err(e) = s.check_conn(conn) {
                return Some(e);
            }
            if p_params != 0 && !is_ram(p_params) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let outstanding = matches!(
                s.conn(conn).map(|c| &c.pairing),
                Some(Pairing::PeerRequested) | Some(Pairing::Accepted)
            );
            if status != 0 || p_params == 0 {
                // Reject: AUTH_STATUS pair-fail, link stays up. A reject
                // answers a peer request when one is outstanding; with
                // none outstanding silicon still accepts the call as a
                // no-op reject (only the accept path needs a request).
                s.push_evt(
                    EVT_GAP_AUTH_STATUS,
                    SdBle::auth_status_payload(conn, SEC_STATUS_PAIRING_NOT_SUPP, false),
                );
                if let Some(c) = s.conn_mut(conn) {
                    c.pairing = Pairing::Idle;
                }
                return Some(NRF_SUCCESS);
            }
            if !outstanding {
                return Some(NRF_ERROR_INVALID_STATE);
            }
            // Accept: mark accepted; the driver handshake completes it.
            if let Some(c) = s.conn_mut(conn) {
                c.pairing = Pairing::Accepted;
            }
            s.staged = Some(BleJob::GapAuthenticate { conn });
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GAP_AUTH_KEY_REPLY => {
            // Reply to AUTH_KEY_REQUEST (passkey / OOB entry): (conn,
            // key_type, *key). NONE(0)+NULL accepts a no-key request;
            // PASSKEY(1) needs 6 ASCII digits; OOB(2) needs 16 bytes.
            // Completes an outstanding KeyEntry (or an Accepted legacy
            // handshake the driver is resolving); with nothing
            // outstanding there is nothing to reply to (silicon
            // INVALID_STATE).
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            let (conn, key_type, p_key) = (r[0] as u16, r[1] as u8, r[2]);
            if let Err(e) = s.check_conn(conn) {
                return Some(e);
            }
            if key_type > 2 {
                return Some(NRF_ERROR_INVALID_PARAM);
            }
            let need = match key_type {
                0 => 0,
                1 => 6,
                _ => 16,
            };
            if need != 0 {
                if !is_ram(p_key) {
                    return Some(NRF_ERROR_INVALID_ADDR);
                }
                if key_type == 1 {
                    for i in 0..6u32 {
                        let b = mem.read8(p_key.wrapping_add(i));
                        if !(b as char).is_ascii_digit() {
                            return Some(NRF_ERROR_INVALID_PARAM);
                        }
                    }
                }
            }
            match s.conn(conn) {
                Some(c)
                    if matches!(c.pairing, Pairing::KeyEntry { .. } | Pairing::Accepted) =>
                {
                    Some(NRF_SUCCESS)
                }
                _ => Some(NRF_ERROR_INVALID_STATE),
            }
        }
        x if x == SVC_GAP_LESC_DHKEY_REPLY => {
            // Reply to LESC_DHKEY_REQUEST: (conn, *dhkey32). Completes
            // an outstanding LescDhkey; otherwise INVALID_STATE.
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            let (conn, p_key) = (r[0] as u16, r[1]);
            if let Err(e) = s.check_conn(conn) {
                return Some(e);
            }
            if !is_ram(p_key) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            match s.conn(conn) {
                Some(c) if matches!(c.pairing, Pairing::LescDhkey { .. }) => Some(NRF_SUCCESS),
                _ => Some(NRF_ERROR_INVALID_STATE),
            }
        }
        x if x == SVC_GAP_KEYPRESS_NOTIFY => {
            // Keypress notification during passkey entry: (conn,
            // kp_not). Needs an outstanding KeyEntry (silicon
            // INVALID_STATE otherwise); posts KEY_PRESSED so the peer
            // side can drain it. Types 0..=4 (ble_gap.h KP_NOT_TYPES).
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            let (conn, kp_not) = (r[0] as u16, r[1] as u8);
            if let Err(e) = s.check_conn(conn) {
                return Some(e);
            }
            if kp_not > 4 {
                return Some(NRF_ERROR_INVALID_PARAM);
            }
            match s.conn(conn) {
                Some(c) if matches!(c.pairing, Pairing::KeyEntry { .. }) => {
                    s.push_evt(EVT_GAP_KEY_PRESSED, SdBle::key_pressed_payload(conn, kp_not));
                    Some(NRF_SUCCESS)
                }
                _ => Some(NRF_ERROR_INVALID_STATE),
            }
        }
        x if x == SVC_GAP_ENCRYPT => {
            // Master re-encrypt with stored keys: (conn, *master_id{ediv
            // u16, rand[8]}, *enc_info{ltk[16], lesc/auth/keylen u8}).
            // Completes an EncryptPending (SEC_INFO_REPLY answered with
            // keys) or an Accepted handshake; otherwise INVALID_STATE.
            // NULL master_id = use local keys (peripheral role).
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            let (conn, p_mid, p_enc) = (r[0] as u16, r[1], r[2]);
            if let Err(e) = s.check_conn(conn) {
                return Some(e);
            }
            if p_mid != 0 && !is_ram(p_mid) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            if !is_ram(p_enc) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            match s.conn(conn) {
                Some(c)
                    if matches!(
                        c.pairing,
                        Pairing::EncryptPending | Pairing::Accepted
                    ) =>
                {
                    Some(NRF_SUCCESS)
                }
                _ => Some(NRF_ERROR_INVALID_STATE),
            }
        }
        x if x == SVC_GAP_SEC_INFO_REPLY => {
            // Reply to SEC_INFO_REQUEST: all-NULL = no keys (bond not
            // found — AUTH_STATUS pair-fail AUTH_REQ, link stays up);
            // non-NULL enc (16B LTK block) = keys found, ENCRYPT
            // expected next (EncryptPending). Needs an outstanding
            // peer request; otherwise INVALID_STATE.
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            let (conn, p_enc, p_id, p_sign) = (r[0] as u16, r[1], r[2], r[3]);
            if let Err(e) = s.check_conn(conn) {
                return Some(e);
            }
            for p in [p_enc, p_id, p_sign] {
                if p != 0 && !is_ram(p) {
                    return Some(NRF_ERROR_INVALID_ADDR);
                }
            }
            if !matches!(
                s.conn(conn).map(|c| &c.pairing),
                Some(Pairing::PeerRequested)
            ) {
                return Some(NRF_ERROR_INVALID_STATE);
            }
            if p_enc == 0 {
                s.push_evt(
                    EVT_GAP_AUTH_STATUS,
                    SdBle::auth_status_payload(conn, SEC_STATUS_AUTH_REQ, false),
                );
                if let Some(c) = s.conn_mut(conn) {
                    c.pairing = Pairing::Idle;
                }
                return Some(NRF_SUCCESS);
            }
            if let Some(c) = s.conn_mut(conn) {
                c.pairing = Pairing::EncryptPending;
            }
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GAP_LESC_OOB_DATA_GET => {
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            let (conn, p_own) = (r[0] as u16, r[2]);
            if let Err(e) = s.check_conn(conn) {
                return Some(e);
            }
            // OOB data is 16B confirm + 16B random: zeroed stub (no real
            // crypto — documented; the handshake still completes).
            if p_own != 0 {
                if !is_ram(p_own) {
                    return Some(NRF_ERROR_INVALID_ADDR);
                }
                for i in 0..32u32 {
                    mem.write8(p_own.wrapping_add(i), 0);
                }
            }
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GAP_LESC_OOB_DATA_SET => {
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            let conn = r[0] as u16;
            if let Err(e) = s.check_conn(conn) {
                return Some(e);
            }
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GAP_CONN_SEC_GET => {
            // (conn, *conn_sec{sec_mode, key_size}): report open link
            // (mode1 level1) or encrypted-after-pairing per link.
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            let (conn, p_sec) = (r[0] as u16, r[1]);
            if !is_ram(p_sec) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let (mode, size) = match s.conn(conn) {
                None => return Some(BLE_ERROR_INVALID_CONN_HANDLE),
                Some(c) if c.encrypted => (0x21u8, 16u8),
                Some(_) => (0x11u8, 16u8),
            };
            mem.write8(p_sec, mode);
            mem.write8(p_sec.wrapping_add(1), size);
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GAP_RSSI_START || x == SVC_GAP_RSSI_STOP => {
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            let conn = r[0] as u16;
            if s.check_conn(conn).is_err() {
                return Some(BLE_ERROR_INVALID_CONN_HANDLE);
            }
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GAP_RSSI_GET => {
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            // (conn, *rssi, *ch_index=NULL-ok): driver samples air.
            let (conn, p_rssi) = (r[0] as u16, r[1]);
            if s.check_conn(conn).is_err() {
                return Some(BLE_ERROR_INVALID_CONN_HANDLE);
            }
            if !is_ram(p_rssi) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            s.staged = Some(BleJob::GapRssiGet { conn });
            // Synchronous legibility: report the link's last air RSSI
            // now; the completion posts RSSI_CHANGED for the drain.
            let level = s.conn(conn).map(|c| c.rssi_dbm).unwrap_or(s.rssi_dbm);
            mem.write8(p_rssi, level as u8);
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GAP_SCAN_START => {
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            // r0 = *scan_params or NULL (defaults). One live sighting
            // becomes the ADV_REPORT the driver completes.
            s.staged = Some(BleJob::GapScanStart);
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GAP_SCAN_STOP => Some(NRF_SUCCESS),
        x if x == SVC_GAP_CONNECT => {
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            // sd_ble_gap_connect(peer*, scan*, conn*): only the peer
            // address matters here; NULL scan/conn = defaults.
            let p = r[0];
            if !is_ram(p) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let mut addr = [0u8; 6];
            for i in 0..6u32 {
                addr[i as usize] = mem.read8(p.wrapping_add(1 + i));
            }
            // Silicon allows several concurrent links; BUSY only when
            // a connect procedure is already staged (one at a time).
            if matches!(s.staged, Some(BleJob::GapConnect { .. })) {
                return Some(NRF_ERROR_BUSY);
            }
            s.staged = Some(BleJob::GapConnect { addr });
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GAP_CONNECT_CANCEL => {
            s.staged = None;
            Some(NRF_SUCCESS)
        }
        // ---- GATTC: discovery + read + write stage driver jobs ----
        x if x == SVC_GATTC_PRIMARY_DISC => {
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            // (conn, start_handle, *uuid|NULL): NULL = all services.
            let (conn, start, p_uuid) = (r[0] as u16, r[1] as u16, r[2]);
            if let Err(e) = s.check_conn(conn) {
                return Some(e);
            }
            if p_uuid != 0 && !is_ram(p_uuid) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let uuid16 = if p_uuid == 0 { None } else { read_uuid(mem, p_uuid) };
            s.staged = Some(BleJob::GattcPrimDisc { conn, start, uuid16 });
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GATTC_REL_DISC => {
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            // (conn, *handle_range{start,end}): include walk over air.
            let (conn, p_range) = (r[0] as u16, r[1]);
            if let Err(e) = s.check_conn(conn) {
                return Some(e);
            }
            if !is_ram(p_range) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let (start, end) = (read_u16_le(mem, p_range), read_u16_le(mem, p_range.wrapping_add(2)));
            if start == 0 || start > end {
                return Some(NRF_ERROR_INVALID_PARAM);
            }
            s.staged = Some(BleJob::GattcRelDisc { conn, start, end });
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GATTC_DESC_DISC
            || x == SVC_GATTC_ATTR_INFO_DISC =>
        {
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            // (conn, *handle_range{start,end}): range walk over air.
            let (conn, p_range) = (r[0] as u16, r[1]);
            if let Err(e) = s.check_conn(conn) {
                return Some(e);
            }
            if !is_ram(p_range) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let (start, end) = (read_u16_le(mem, p_range), read_u16_le(mem, p_range.wrapping_add(2)));
            if start == 0 || start > end {
                return Some(NRF_ERROR_INVALID_PARAM);
            }
            if svc == SVC_GATTC_ATTR_INFO_DISC {
                s.staged = Some(BleJob::GattcAttrInfoDisc { conn, start, end });
            } else {
                s.staged = Some(BleJob::GattcDescDisc { conn, start, end });
            }
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GATTC_CHAR_DISC => {
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            let (conn, p_range) = (r[0] as u16, r[1]);
            if let Err(e) = s.check_conn(conn) {
                return Some(e);
            }
            if !is_ram(p_range) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let (start, end) = (read_u16_le(mem, p_range), read_u16_le(mem, p_range.wrapping_add(2)));
            if start == 0 || start > end {
                return Some(NRF_ERROR_INVALID_PARAM);
            }
            s.staged = Some(BleJob::GattcCharDisc { conn, start, end });
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GATTC_UUID_READ => {
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            // (conn, *uuid, *handle_range): match every attr with the
            // UUID in range; driver reads each value over air.
            let (conn, p_uuid, p_range) = (r[0] as u16, r[1], r[2]);
            if let Err(e) = s.check_conn(conn) {
                return Some(e);
            }
            if !is_ram(p_uuid) || !is_ram(p_range) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let uuid16 = read_uuid(mem, p_uuid);
            let (start, end) = (read_u16_le(mem, p_range), read_u16_le(mem, p_range.wrapping_add(2)));
            if start == 0 || start > end {
                return Some(NRF_ERROR_INVALID_PARAM);
            }
            s.staged = Some(BleJob::GattcUuidRead { conn, uuid16, start, end });
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GATTC_CHAR_VALS_READ => {
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            // (conn, *handles u16[count], count): copy the handle list
            // NOW (firmware may reuse it); driver reads each over air.
            let (conn, p_handles, count) = (r[0] as u16, r[1], r[2] as usize);
            if let Err(e) = s.check_conn(conn) {
                return Some(e);
            }
            if count == 0 || count > 32 {
                return Some(NRF_ERROR_INVALID_PARAM);
            }
            if !is_ram(p_handles) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let mut handles = Vec::with_capacity(count);
            for i in 0..count {
                let h = read_u16_le(mem, p_handles.wrapping_add(2 * i as u32));
                if h == GATT_HANDLE_INVALID {
                    return Some(NRF_ERROR_INVALID_PARAM);
                }
                handles.push(h);
            }
            s.staged = Some(BleJob::GattcValsRead { conn, handles });
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GATTC_READ => {
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            let (conn, handle, offset) = (r[0] as u16, r[1] as u16, r[2] as u16);
            if let Err(e) = s.check_conn(conn) {
                return Some(e);
            }
            if handle == GATT_HANDLE_INVALID {
                return Some(NRF_ERROR_INVALID_PARAM);
            }
            s.staged = Some(BleJob::GattcRead { conn, handle, offset });
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GATTC_WRITE => {
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            // (conn, *write_params{op u8, flags u8, handle u16,
            // offset u16, len u16, *value}): copy bytes NOW (firmware
            // may reuse the buffer before the driver drains the job).
            let (conn, p_wp) = (r[0] as u16, r[1]);
            if let Err(e) = s.check_conn(conn) {
                return Some(e);
            }
            if !is_ram(p_wp) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let op = mem.read8(p_wp);
            if op != GATT_OP_WRITE_REQ && op != GATT_OP_WRITE_CMD {
                return Some(NRF_ERROR_INVALID_PARAM);
            }
            let handle = read_u16_le(mem, p_wp.wrapping_add(2));
            let len = read_u16_le(mem, p_wp.wrapping_add(6)) as usize;
            let p_val = mem.read32(p_wp.wrapping_add(8));
            if handle == GATT_HANDLE_INVALID {
                return Some(NRF_ERROR_INVALID_PARAM);
            }
            if len != 0 && !is_ram(p_val) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let mut data = vec![0u8; len];
            for (i, b) in data.iter_mut().enumerate() {
                *b = mem.read8(p_val.wrapping_add(i as u32));
            }
            s.staged = Some(BleJob::GattcWrite { conn, op, handle, data });
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GATTC_HV_CONFIRM => {
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            let conn = r[0] as u16;
            if s.check_conn(conn).is_err() {
                return Some(BLE_ERROR_INVALID_CONN_HANDLE);
            }
            // Indication confirm: posts HVC server-side; nothing air.
            s.push_evt(EVT_GATTS_HVC, SdBle::hvc_payload(conn, r[1] as u16));
            Some(NRF_SUCCESS)
        }
        // ---- GATTS: local attribute table ----
        x if x == SVC_GATTS_SERVICE_ADD => {
            // (type r0, *uuid r1, *handle r2): alloc decl handle.
            let (uuid_ptr, h_ptr) = (r[1], r[2]);
            if !is_ram(uuid_ptr) || !is_ram(h_ptr) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let uuid16 = read_uuid(mem, uuid_ptr);
            let h = s.next_handle;
            s.next_handle = s.next_handle.wrapping_add(1);
            s.attrs.push(Attr { handle: h, uuid16, value: Vec::new(), cccd: false, cccd_handle: 0, subscribed: Vec::new() });
            mem.write16(h_ptr, h);
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GATTS_INCLUDE_ADD => {
            let h_ptr = r[2];
            if !is_ram(h_ptr) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let h = s.next_handle;
            s.next_handle = s.next_handle.wrapping_add(1);
            mem.write16(h_ptr, h);
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GATTS_CHAR_ADD => {
            // sd_ble_gatts_characteristic_add(service, *char_md,
            // *attr_value, *handles{value,...}): r1 = char_md (may be
            // NULL), r2 = attr, r3 = handles. Alloc decl + value
            // (+CCCD when peer-subscribable). Initial bytes copied NOW.
            let (p_attr, p_handles) = (r[2], r[3]);
            if p_attr != 0 && !is_ram(p_attr) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            if !is_ram(p_handles) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let (uuid16, init) = if p_attr == 0 {
                (None, Vec::new())
            } else {
                let p_uuid = mem.read32(p_attr);
                let uuid16 = if p_uuid != 0 && is_ram(p_uuid) {
                    read_uuid(mem, p_uuid)
                } else {
                    None
                };
                let init_len = read_u16_le(mem, p_attr.wrapping_add(8)) as usize;
                let p_val = mem.read32(p_attr.wrapping_add(16));
                let mut init = vec![0u8; init_len.min(64)];
                if init_len != 0 {
                    if !is_ram(p_val) {
                        return Some(NRF_ERROR_INVALID_ADDR);
                    }
                    for (i, b) in init.iter_mut().enumerate() {
                        *b = mem.read8(p_val.wrapping_add(i as u32));
                    }
                }
                (uuid16, init)
            };
            let decl = s.next_handle;
            s.next_handle = s.next_handle.wrapping_add(1);
            let value_h = s.next_handle;
            s.next_handle = s.next_handle.wrapping_add(1);
            // CCCD when the uuid is the battery level (notifiable in
            // our peer table) — else plain value.
            let cccd = uuid16 == Some(0x2A19);
            let cccd_h = if cccd {
                let h = s.next_handle;
                s.next_handle = s.next_handle.wrapping_add(1);
                h
            } else {
                0
            };
            s.attrs.push(Attr { handle: value_h, uuid16, value: init.clone(), cccd, cccd_handle: cccd_h, subscribed: Vec::new() });
            if uuid16 == Some(0x2A19) {
                if let Some(&b) = init.first() {
                    s.batt_level = b;
                }
            }
            mem.write16(p_handles, value_h);
            mem.write16(p_handles.wrapping_add(4), cccd_h);
            let _ = decl;
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GATTS_DESC_ADD => {
            let (p_attr, h_ptr) = (r[1], r[2]);
            if p_attr != 0 && !is_ram(p_attr) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            if !is_ram(h_ptr) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let h = s.next_handle;
            s.next_handle = s.next_handle.wrapping_add(1);
            s.attrs.push(Attr { handle: h, uuid16: Some(0x2902), value: vec![0, 0], cccd: true, cccd_handle: 0, subscribed: Vec::new() });
            mem.write16(h_ptr, h);
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GATTS_VALUE_SET => {
            // (conn, handle, *ble_gatts_value_t{len, offset, *value}).
            // conn 0xFFFF ok for non-system attrs (S132).
            let (conn, handle, p_val) = (r[0] as u16, r[1] as u16, r[2]);
            if conn != BLE_CONN_HANDLE_INVALID && s.check_conn(conn).is_err() {
                return Some(BLE_ERROR_INVALID_CONN_HANDLE);
            }
            if !is_ram(p_val) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let (len, off) = (
                read_u16_le(mem, p_val) as usize,
                read_u16_le(mem, p_val.wrapping_add(2)) as usize,
            );
            let p_bytes = mem.read32(p_val.wrapping_add(4));
            if len != 0 && !is_ram(p_bytes) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            match s.find_attr_mut(handle) {
                None => Some(NRF_ERROR_NOT_FOUND),
                Some(a) => {
                    if off > a.value.len() && !(off == 0 && a.value.is_empty()) {
                        return Some(NRF_ERROR_INVALID_PARAM);
                    }
                    if a.value.len() < off + len {
                        a.value.resize(off + len, 0);
                    }
                    for i in 0..len {
                        a.value[off + i] = mem.read8(p_bytes.wrapping_add(i as u32));
                    }
                    if a.uuid16 == Some(0x2A19) {
                        if let Some(&b) = a.value.first() {
                            s.batt_level = b;
                        }
                    }
                    Some(NRF_SUCCESS)
                }
            }
        }
        x if x == SVC_GATTS_VALUE_GET => {
            let (conn, handle, p_val) = (r[0] as u16, r[1] as u16, r[2]);
            if conn != BLE_CONN_HANDLE_INVALID && s.check_conn(conn).is_err() {
                return Some(BLE_ERROR_INVALID_CONN_HANDLE);
            }
            if !is_ram(p_val) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let (mut len, off) = (
                read_u16_le(mem, p_val) as usize,
                read_u16_le(mem, p_val.wrapping_add(2)) as usize,
            );
            let p_bytes = mem.read32(p_val.wrapping_add(4));
            match s.find_attr(handle) {
                None => Some(NRF_ERROR_NOT_FOUND),
                Some(a) => {
                    if off > a.value.len() {
                        return Some(NRF_ERROR_INVALID_PARAM);
                    }
                    let avail = a.value.len() - off;
                    if p_bytes == 0 {
                        // Length query: report full length, copy nothing.
                        write_u16_le(mem, p_val, avail as u16);
                        return Some(NRF_SUCCESS);
                    }
                    if !is_ram(p_bytes) {
                        return Some(NRF_ERROR_INVALID_ADDR);
                    }
                    if len > avail {
                        len = avail;
                    }
                    for i in 0..len {
                        mem.write8(p_bytes.wrapping_add(i as u32), a.value[off + i]);
                    }
                    write_u16_le(mem, p_val, len as u16);
                    Some(NRF_SUCCESS)
                }
            }
        }
        x if x == SVC_GATTS_HVX => {
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            // (conn, *hvx_params{handle@0, type@2, offset@4, *len@8,
            // *data@12}): copy bytes NOW; driver emits over air.
            let (conn, p_hp) = (r[0] as u16, r[1]);
            if let Err(e) = s.check_conn(conn) {
                return Some(e);
            }
            if !is_ram(p_hp) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let handle = read_u16_le(mem, p_hp);
            let hvx_type = mem.read8(p_hp.wrapping_add(2));
            if hvx_type != GATT_HVX_NOTIFICATION && hvx_type != GATT_HVX_INDICATION {
                return Some(NRF_ERROR_INVALID_PARAM);
            }
            let p_len = mem.read32(p_hp.wrapping_add(8));
            if !is_ram(p_len) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let len = mem.read16(p_len) as usize;
            let p_data = mem.read32(p_hp.wrapping_add(12));
            // NULL data = current attribute value (S132).
            let data = if p_data == 0 {
                match s.find_attr(handle) {
                    None => return Some(NRF_ERROR_NOT_FOUND),
                    Some(a) => a.value.clone(),
                }
            } else {
                if !is_ram(p_data) {
                    return Some(NRF_ERROR_INVALID_ADDR);
                }
                let mut d = vec![0u8; len];
                for (i, b) in d.iter_mut().enumerate() {
                    *b = mem.read8(p_data.wrapping_add(i as u32));
                }
                d
            };
            // Notifications need TX packets on THIS link (silicon
            // NO_TX_PACKETS); indications ride the ATT confirm path.
            let budget = s.conn(conn).map(|c| c.tx_count).unwrap_or(0);
            if hvx_type == GATT_HVX_NOTIFICATION && budget == 0 {
                return Some(BLE_ERROR_NO_TX_PACKETS);
            }
            // CCCD gate (silicon refuses notify/indicate on a link that
            // never subscribed): bit0 = notify, bit1 = indicate. The
            // battery char allocates a CCCD; other chars only if the
            // peer wrote one via DESC_ADD. Unsubscribed -> INVALID_STATE.
            let want = if hvx_type == GATT_HVX_NOTIFICATION { 0x01 } else { 0x02 };
            let subbed = s
                .find_attr(handle)
                .map(|a| a.subscribed.iter().any(|(c, b)| *c == conn && (*b & want) != 0))
                .unwrap_or(false);
            if !subbed {
                return Some(NRF_ERROR_INVALID_STATE);
            }
            if let Some(c) = s.conn_mut(conn) {
                if hvx_type == GATT_HVX_NOTIFICATION && c.tx_count > 0 {
                    c.tx_count -= 1;
                }
            }
            s.staged = Some(BleJob::GattsHvx { conn, handle, hvx_type, data });
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GATTS_SERVICE_CHANGED => {
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GATTS_RW_AUTHORIZE_REPLY
            || x == SVC_GATTS_SYS_ATTR_SET
            || x == SVC_GATTS_SYS_ATTR_GET =>
        {
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GATTS_INITIAL_USER_HANDLE_GET => {
            let p = r[0];
            if !is_ram(p) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            mem.write16(p, s.next_handle);
            Some(NRF_SUCCESS)
        }
        // ---- L2CAP (SVC 0xB0..): CoC CID register + TX stage ----
        x if x == SVC_L2CAP_CID_REGISTER => {
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            let cid = r[0] as u16;
            // Dynamic range only (silicon INVALID_PARAM below it).
            if cid < L2CAP_CID_DYN_BASE || cid >= L2CAP_CID_DYN_BASE + L2CAP_CID_DYN_MAX {
                return Some(NRF_ERROR_INVALID_PARAM);
            }
            if s.l2cap_cids.contains(&cid) {
                return Some(BLE_ERROR_L2CAP_CID_IN_USE);
            }
            if s.l2cap_cids.len() >= L2CAP_CID_DYN_MAX as usize {
                return Some(NRF_ERROR_NO_MEM);
            }
            s.l2cap_cids.push(cid);
            Some(NRF_SUCCESS)
        }
        x if x == SVC_L2CAP_CID_UNREGISTER => {
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            let cid = r[0] as u16;
            match s.l2cap_cids.iter().position(|c| *c == cid) {
                Some(i) => {
                    s.l2cap_cids.remove(i);
                    Some(NRF_SUCCESS)
                }
                None => Some(NRF_ERROR_NOT_FOUND),
            }
        }
        x if x == SVC_L2CAP_TX => {
            if require_enabled(s).is_err() {
                return Some(BLE_ERROR_NOT_ENABLED);
            }
            // (conn, *header{len u16, cid u16}, *data): copy NOW.
            let (conn, p_hdr, p_data) = (r[0] as u16, r[1], r[2]);
            if let Err(e) = s.check_conn(conn) {
                return Some(e);
            }
            if !is_ram(p_hdr) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let (len, cid) = (
                read_u16_le(mem, p_hdr) as usize,
                read_u16_le(mem, p_hdr.wrapping_add(2)),
            );
            if !s.l2cap_cids.contains(&cid) {
                return Some(NRF_ERROR_INVALID_PARAM);
            }
            if len != 0 && !is_ram(p_data) {
                return Some(NRF_ERROR_INVALID_ADDR);
            }
            let mut data = vec![0u8; len.min(512)];
            for (i, b) in data.iter_mut().enumerate() {
                *b = mem.read8(p_data.wrapping_add(i as u32));
            }
            s.staged = Some(BleJob::L2capTx { conn, cid, data });
            Some(NRF_SUCCESS)
        }
        x if x == SVC_GATTS_ATTR_GET => {
            // (handle, *uuid, *md): report table uuid, no metadata.
            let (handle, p_uuid) = (r[0] as u16, r[1]);
            match s.find_attr(handle) {
                None => Some(NRF_ERROR_NOT_FOUND),
                Some(a) => {
                    if p_uuid != 0 {
                        if !is_ram(p_uuid) {
                            return Some(NRF_ERROR_INVALID_ADDR);
                        }
                        match a.uuid16 {
                            Some(u) => {
                                write_u16_le(mem, p_uuid, u);
                                mem.write8(p_uuid.wrapping_add(2), UUID_TYPE_BLE);
                            }
                            None => {
                                write_u16_le(mem, p_uuid, 0);
                                mem.write8(p_uuid.wrapping_add(2), UUID_TYPE_VENDOR_BEGIN);
                            }
                        }
                    }
                    Some(NRF_SUCCESS)
                }
            }
        }
        _ => None,
    }
}

// ---- driver-side take/complete (take_* -> air -> complete_*) ----

/// Take a staged BLE job; None when idle.
pub fn take_job() -> Option<BleJob> {
    with_sd_ble(|s| s.staged.take())
}

thread_local! {
    /// Bytes staged alongside the last WRITE/HVX take_job: the SVC
    /// copies firmware bytes at call time (stable for the driver).
    static TAKE_DATA: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Stage take_data (called by the take_job export path).
pub fn stage_take_data(data: Vec<u8>) {
    TAKE_DATA.with(|t| *t.borrow_mut() = data);
}

/// Drain the staged take bytes (once per job; empty otherwise).
pub fn take_staged_data() -> Vec<u8> {
    TAKE_DATA.with(|t| std::mem::take(&mut *t.borrow_mut()))
}

/// Complete a GATTC primary-service discovery: driver walked the peer
/// table over air; posts PRIM_DISC_RSP firmware drains via evt_get.
pub fn complete_prim_disc(conn: u16, svcs: &[DiscService]) {
    with_sd_ble(|s| {
        s.push_evt(EVT_GATTC_PRIM_DISC_RSP, SdBle::prim_disc_payload(conn, svcs));
    });
}

/// Complete a GATTC characteristic discovery: posts CHAR_DISC_RSP.
pub fn complete_char_disc(conn: u16, chars: &[DiscChar]) {
    with_sd_ble(|s| {
        s.push_evt(EVT_GATTC_CHAR_DISC_RSP, SdBle::char_disc_payload(conn, chars));
    });
}

/// Complete a GATTC relationship discovery: posts REL_DISC_RSP.
pub fn complete_rel_disc(conn: u16, incs: &[DiscInclude]) {
    with_sd_ble(|s| {
        s.push_evt(EVT_GATTC_REL_DISC_RSP, SdBle::rel_disc_payload(conn, incs));
    });
}

/// Complete a GATTC attribute-info discovery: posts ATTR_INFO_RSP.
pub fn complete_attr_info_disc(conn: u16, infos: &[DiscAttrInfo]) {
    with_sd_ble(|s| {
        s.push_evt(EVT_GATTC_ATTR_INFO_RSP, SdBle::attr_info_payload(conn, infos));
    });
}

/// Complete a GATTC read-by-UUID: posts UUID_READ_RSP.
pub fn complete_uuid_read(conn: u16, pairs: &[HandleValue]) {
    with_sd_ble(|s| {
        s.push_evt(EVT_GATTC_UUID_READ_RSP, SdBle::uuid_read_payload(conn, pairs));
    });
}

/// Complete a GATTC multi-read: posts VALS_READ_RSP (concatenated).
pub fn complete_vals_read(conn: u16, data: &[u8]) {
    with_sd_ble(|s| {
        s.push_evt(EVT_GATTC_VALS_READ_RSP, SdBle::vals_read_payload(conn, data));
    });
}

/// Complete a GATTC descriptor discovery: posts DESC_DISC_RSP.
pub fn complete_desc_disc(conn: u16, descs: &[DiscDesc]) {
    with_sd_ble(|s| {
        s.push_evt(EVT_GATTC_DESC_DISC_RSP, SdBle::desc_disc_payload(conn, descs));
    });
}

/// Complete a GATTC read: driver resolved `data` over air; posts the
/// READ_RSP event firmware drains via sd_ble_evt_get.
pub fn complete_gattc_read(conn: u16, handle: u16, offset: u16, data: &[u8]) {
    with_sd_ble(|s| {
        s.push_evt(
            EVT_GATTC_READ_RSP,
            SdBle::read_rsp_payload(conn, handle, offset, data),
        );
    });
}

/// Complete a GATTC write: driver wrote over air (WRITE_RSP proof);
/// posts WRITE_RSP with the echoed op/bytes.
pub fn complete_gattc_write(conn: u16, handle: u16, op: u8, data: &[u8]) {
    with_sd_ble(|s| {
        s.push_evt(
            EVT_GATTC_WRITE_RSP,
            SdBle::write_rsp_payload(conn, handle, op, 0, data),
        );
    });
}

/// Complete a GATTC notification/indication from the peer: posts HVX.
pub fn complete_gattc_hvx(conn: u16, handle: u16, hvx_type: u8, data: &[u8]) {
    with_sd_ble(|s| {
        s.push_evt(EVT_GATTC_HVX, SdBle::hvx_payload(conn, handle, hvx_type, data));
    });
}

/// Complete a GAP connect: driver connected over air; brings a new
/// link up (fresh handle) and posts CONNECTED so sd_ble_evt_get
/// reports the new link. Returns the assigned handle (bridge echoes
/// it back; INVALID when the table is full, which never happens with
/// 64K handles).
pub fn complete_gap_connect(peer: [u8; 6]) -> u16 {
    with_sd_ble(|s| {
        let h = s.link_up(peer);
        if h == BLE_CONN_HANDLE_INVALID {
            return h;
        }
        let own = s.own_addr;
        s.push_evt(EVT_GAP_CONNECTED, SdBle::connected_payload(h, peer, own));
        h
    })
}

/// Complete a GAP disconnect: tears the link down, posts DISCONNECTED
/// with firmware's HCI reason.
pub fn complete_gap_disconnect(conn: u16, reason: u8) {
    with_sd_ble(|s| {
        s.link_down(conn);
        s.push_evt(EVT_GAP_DISCONNECTED, SdBle::disconnected_payload(conn, reason));
    });
}

/// Complete an RSSI sample: updates the link + cached level, posts
/// RSSI_CHANGED.
pub fn complete_rssi(conn: u16, rssi: i8) {
    with_sd_ble(|s| {
        if let Some(c) = s.conn_mut(conn) {
            c.rssi_dbm = rssi;
        }
        s.rssi_dbm = rssi;
        s.push_evt(EVT_GAP_RSSI_CHANGED, SdBle::rssi_changed_payload(conn, rssi));
    });
}

/// Complete a pairing handshake the driver ran over air: marks the
/// link bonded+encrypted and posts AUTH_STATUS (success) plus
/// CONN_SEC_UPDATE, like silicon after SMP completes. The handshake
/// stays open for the key exchange: an Accepted link keeps accepting
/// AUTH_KEY_REQUEST / LESC_DHKEY_REQUEST legs (silicon runs passkey /
/// OOB / numeric-comparison INSIDE the same SMP procedure), and only
/// returns to Idle after AUTH_STATUS is drained... in practice the
/// driver completes each leg explicitly, so complete_pairing leaves an
/// Accepted link Accepted (firmware answers the posted key requests),
/// while a locally-Requested handshake (AUTHENTICATE initiator) goes
/// Idle like silicon's completed procedure.
pub fn complete_pairing(conn: u16, bonded: bool) {
    with_sd_ble(|s| {
        if let Some(c) = s.conn_mut(conn) {
            if c.pairing == Pairing::Requested {
                c.pairing = Pairing::Idle;
            }
            c.bonded = bonded;
            c.encrypted = true;
        }
        s.push_evt(
            EVT_GAP_AUTH_STATUS,
            SdBle::auth_status_payload(conn, SEC_STATUS_SUCCESS, bonded),
        );
        // CONN_SEC_UPDATE envelope: {conn, sec_mode, key_size}.
        let mut p = conn.to_le_bytes().to_vec();
        p.extend_from_slice(&SdBle::conn_sec_payload());
        s.push_evt(EVT_GAP_CONN_SEC_UPDATE, p);
    });
}

/// Fail a pairing handshake: posts AUTH_STATUS with the S132 status
/// (e.g. PAIRING_NOT_SUPP) and leaves the link open, unencrypted —
/// silicon keeps the connection on pairing failure.
pub fn fail_pairing(conn: u16, status: u8) {
    with_sd_ble(|s| {
        if let Some(c) = s.conn_mut(conn) {
            c.pairing = Pairing::Idle;
        }
        s.push_evt(EVT_GAP_AUTH_STATUS, SdBle::auth_status_payload(conn, status, false));
    });
}

/// Post a peer-initiated SEC_PARAMS_REQUEST: the bridge observed the
/// peer start SMP with these params; firmware must answer with
/// SEC_PARAMS_REPLY (accept or reject). The link must be up and idle;
/// posts the event and marks PeerRequested (silicon INVALID_STATE on
/// the reply SVCs without this). Returns false when the link cannot
/// take a request (down, busy, or stack disabled).
pub fn post_sec_params_request(conn: u16, peer_params: [u8; 5]) -> bool {
    with_sd_ble(|s| {
        if !s.enabled {
            return false;
        }
        match s.conn_mut(conn) {
            Some(c) if c.pairing == Pairing::Idle => {
                c.pairing = Pairing::PeerRequested;
            }
            _ => return false,
        }
        s.push_evt(
            EVT_GAP_SEC_PARAMS_REQUEST,
            SdBle::sec_params_request_payload(conn, &peer_params),
        );
        true
    })
}

/// Post a peer-initiated SEC_INFO_REQUEST: the peer asks to re-encrypt
/// with stored keys (firmware answers SEC_INFO_REPLY, then ENCRYPT).
/// Same link rules as SEC_PARAMS_REQUEST.
pub fn post_sec_info_request(
    conn: u16,
    peer_addr: [u8; 7],
    master_id: [u8; 10],
    req: u8,
) -> bool {
    with_sd_ble(|s| {
        if !s.enabled {
            return false;
        }
        match s.conn_mut(conn) {
            Some(c) if c.pairing == Pairing::Idle => {
                c.pairing = Pairing::PeerRequested;
            }
            _ => return false,
        }
        s.push_evt(
            EVT_GAP_SEC_INFO_REQUEST,
            SdBle::sec_info_request_payload(conn, &peer_addr, &master_id, req),
        );
        true
    })
}

/// Post an AUTH_KEY_REQUEST: the driver needs a passkey/OOB key of
/// `key_type` (ble_gap.h AUTH_KEY_TYPES) from firmware, which answers
/// with AUTH_KEY_REPLY. Needs an Accepted (or already key-entering)
/// handshake; otherwise returns false and posts nothing.
pub fn post_auth_key_request(conn: u16, key_type: u8) -> bool {
    with_sd_ble(|s| {
        if !s.enabled || key_type > 2 {
            return false;
        }
        match s.conn_mut(conn) {
            Some(c)
                if matches!(
                    c.pairing,
                    Pairing::Accepted | Pairing::KeyEntry { .. }
                ) =>
            {
                c.pairing = Pairing::KeyEntry { key_type };
            }
            _ => return false,
        }
        s.push_evt(EVT_GAP_AUTH_KEY_REQUEST, SdBle::auth_key_request_payload(conn, key_type));
        true
    })
}

/// Post a PASSKEY_DISPLAY: the driver shows this 6-digit ASCII passkey
/// to the user (firmware answers AUTH_KEY_REPLY when match_request).
pub fn post_passkey_display(conn: u16, passkey: [u8; 6], match_request: bool) -> bool {
    with_sd_ble(|s| {
        if !s.enabled {
            return false;
        }
        if s.conn(conn).is_none() {
            return false;
        }
        s.push_evt(
            EVT_GAP_PASSKEY_DISPLAY,
            SdBle::passkey_display_payload(conn, &passkey, match_request),
        );
        true
    })
}

/// Post a KEYPRESS_NOTIFY from the peer (keypress notification type
/// 0..=4, ble_gap.h KP_NOT_TYPES). Needs a live link; returns false
/// without one.
pub fn post_keypress(conn: u16, kp_not: u8) -> bool {
    with_sd_ble(|s| {
        if !s.enabled || kp_not > 4 {
            return false;
        }
        if s.conn(conn).is_none() {
            return false;
        }
        s.push_evt(EVT_GAP_KEY_PRESSED, SdBle::key_pressed_payload(conn, kp_not));
        true
    })
}

/// Post an LESC_DHKEY_REQUEST: the driver needs the DHKey (firmware
/// answers LESC_DHKEY_REPLY; OOB data via LESC_OOB_DATA_SET when
/// oobd_req). Needs an Accepted handshake; otherwise false.
pub fn post_lesc_dhkey_request(conn: u16, oobd_req: bool) -> bool {
    with_sd_ble(|s| {
        if !s.enabled {
            return false;
        }
        match s.conn_mut(conn) {
            Some(c) if c.pairing == Pairing::Accepted => {
                c.pairing = Pairing::LescDhkey { oobd_req };
            }
            _ => return false,
        }
        s.push_evt(
            EVT_GAP_LESC_DHKEY_REQUEST,
            SdBle::lesc_dhkey_request_payload(conn, oobd_req),
        );
        true
    })
}

/// Complete an L2CAP TX: driver moved the frame over air on the
/// registered CID; posts RX echo (loopback legibility, same shape as
/// the RADIO air echo: bridge peers echo CoC frames in tests).
pub fn complete_l2cap_rx(conn: u16, cid: u16, data: &[u8]) {
    with_sd_ble(|s| {
        s.push_evt(EVT_L2CAP_RX, SdBle::l2cap_rx_payload(conn, cid, data));
    });
}

/// Complete a GATTS HVX: driver emitted over air; posts HVC confirm.
pub fn complete_hvx(conn: u16, handle: u16) {
    with_sd_ble(|s| {
        s.push_evt(EVT_GATTS_HVC, SdBle::hvc_payload(conn, handle));
    });
}

/// Post an advertising report (bridge scanner sighting) for evt_get.
pub fn post_adv_report(peer: [u8; 6], rssi: i8, scan_rsp: bool, data: &[u8]) {
    with_sd_ble(|s| {
        s.push_evt(EVT_GAP_ADV_REPORT, SdBle::adv_report_payload(peer, rssi, scan_rsp, data));
    });
}

/// Post a GATTS write (peer wrote our characteristic) for evt_get.
/// Updates the local table value when the handle is known. A write to
/// a CCCD handle (uuid 0x2902, or the auto CCCD of a notifiable char)
/// records the subscription bits for that link (bit0 notify, bit1
/// indicate) instead of attribute bytes — this is what gates HVX.
pub fn post_gatts_write(conn: u16, handle: u16, uuid16: Option<u16>, op: u8, data: &[u8]) {
    with_sd_ble(|s| {
        // CCCD write? Find the owning characteristic by cccd_handle.
        let mut cccd_owner: Option<u16> = None;
        for a in s.attrs.iter() {
            if a.cccd && a.cccd_handle == handle {
                cccd_owner = Some(a.handle);
                break;
            }
        }
        if cccd_owner.is_none() {
            // Direct write to a 0x2902 descriptor attr itself.
            if let Some(a) = s.find_attr_mut(handle) {
                if a.uuid16 == Some(0x2902) {
                    cccd_owner = Some(handle);
                }
            }
        }
        if let Some(owner) = cccd_owner {
            let bits = data.first().copied().unwrap_or(0) & 0x03;
            if let Some(a) = s.find_attr_mut(owner) {
                match a.subscribed.iter_mut().find(|(c, _)| *c == conn) {
                    Some(slot) => slot.1 = bits,
                    None => a.subscribed.push((conn, bits)),
                }
            }
            // CCCD value itself is readable: store the two bytes.
            if let Some(a) = s.find_attr_mut(handle) {
                a.value.clear();
                a.value.extend_from_slice(&[bits, 0]);
            }
        } else if let Some(a) = s.find_attr_mut(handle) {
            a.value.clear();
            a.value.extend_from_slice(data);
            if a.uuid16 == Some(0x2A19) {
                if let Some(&b) = data.first() {
                    s.batt_level = b;
                }
            }
        }
        s.push_evt(
            EVT_GATTS_WRITE,
            SdBle::gatts_write_payload(conn, handle, uuid16, op, data),
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

/// Live connection handles, ascending (debug/export/pump routing).
pub fn conn_handles() -> Vec<u16> {
    with_sd_ble(|s| {
        let mut v: Vec<u16> = s.conns.iter().filter(|c| c.up).map(|c| c.handle).collect();
        v.sort_unstable();
        v
    })
}

/// Connection security [sec_mode, key_size] (debug/export).
pub fn conn_sec(conn: u16) -> Vec<u8> {
    with_sd_ble(|s| match s.conn(conn) {
        None => Vec::new(),
        Some(c) if c.encrypted => vec![0x21, 16],
        Some(_) => vec![0x11, 16],
    })
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

    /// Drain one event with the REAL two-arg contract: r0 = dest RAM,
    /// r1 = *len RAM (in: room incl. header). Returns (r0, id, len).
    fn drain(sys: &System, mem: &mut FlatMemory, dest: u32, room: u16) -> (u32, u16, u16) {
        mem.write16(0x20003FF0, room);
        let r = regs(dest, 0x20003FF0, 0, 0);
        let rc = handle_svc(sys, mem, SVC_BLE_EVT_GET, &r).expect("evt_get claims");
        (rc, mem.read16(dest), mem.read16(dest.wrapping_add(2)))
    }

    /// Build a battery service in the table via real SVCs; returns the
    /// characteristic VALUE handle firmware would cache.
    fn build_battery(sys: &System, mem: &mut FlatMemory) -> u16 {
        // SERVICE_ADD(type=1 primary, *uuid{0x180F,BLE}, *handle).
        mem.write16(0x20001000, 0x180F);
        mem.write8(0x20001002, UUID_TYPE_BLE);
        mem.write16(0x20001010, 0);
        let r = regs(1, 0x20001000, 0x20001010, 0);
        assert_eq!(handle_svc(sys, mem, SVC_GATTS_SERVICE_ADD, &r), Some(NRF_SUCCESS));
        let svc = mem.read16(0x20001010);
        assert!(svc >= 0x10, "service handle {svc:#x}");
        // CHAR_ADD(service, *md, *attr, *handles): attr =
        // {*uuid@0, *md@4, init_len@8, init_offs@10, max_len@12,
        //  *value@16}. S132 ble_gatts_attr_t is 20B (4B-aligned).
        mem.write16(0x20001100, 0x2A19);
        mem.write8(0x20001102, UUID_TYPE_BLE);
        mem.write32(0x20001110, 0x20001100); // p_uuid
        mem.write32(0x20001114, 0); // p_attr_md
        mem.write16(0x20001118, 1); // init_len
        mem.write16(0x2000111A, 0); // init_offs
        mem.write16(0x2000111C, 1); // max_len
        mem.write8(0x20001128, 87);
        mem.write32(0x20001120, 0x20001128); // p_value -> 87
        mem.write16(0x20001130, 0);
        mem.write16(0x20001134, 0);
        let r = regs(svc as u32, 0x20001140, 0x20001110, 0x20001130);
        assert_eq!(handle_svc(sys, mem, SVC_GATTS_CHAR_ADD, &r), Some(NRF_SUCCESS));
        let value = mem.read16(0x20001130);
        assert!(value > svc, "value {value:#x} after service {svc:#x}");
        value
    }

    /// Connect over air (staged job + completion); drains CONNECTED.
    fn connect(sys: &System, mem: &mut FlatMemory) {
        mem.write8(0x20001200, 1);
        for (i, b) in [0x11u8, 0x22, 0x33, 0x44, 0x55, 0x66].iter().enumerate() {
            mem.write8(0x20001200 + 1 + i as u32, *b);
        }
        let r = regs(0x20001200, 0, 0, 0);
        assert_eq!(handle_svc(sys, mem, SVC_GAP_CONNECT, &r), Some(NRF_SUCCESS));
        assert!(matches!(take_job(), Some(BleJob::GapConnect { .. })));
        complete_gap_connect([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
        let (rc, id, _) = drain(sys, mem, 0x20003000, 128);
        assert_eq!(rc, NRF_SUCCESS);
        assert_eq!(id, EVT_GAP_CONNECTED);
    }

    #[test]
    fn enable_reports_ram_base_and_evt_get_two_arg_contract() {
        let sys = test_dummy_system();
        let mut mem = FlatMemory::new(512 * 1024, 128 * 1024);
        reset_for_test();
        assert_eq!(handle_svc(&sys, &mut mem, 0x59, &[0u32; 13]), None, "outside BLE range");
        // Empty queue, one-arg legacy shape: still NOT_FOUND (p_len NULL
        // is not an address error — there is no contract to violate).
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_BLE_EVT_GET, &regs(0x20001000, 0, 0, 0)),
            Some(NRF_ERROR_NOT_FOUND),
            "empty queue reads NOT_FOUND"
        );
        // *len not RAM -> INVALID_ADDR.
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_BLE_EVT_GET, &regs(0x20001000, 0x1000, 0, 0)),
            Some(NRF_ERROR_INVALID_ADDR)
        );
        // ENABLE with *app_ram_base: floor reported, stack up.
        mem.write32(0x20001000, 0);
        let r = regs(0, 0x20001000, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_BLE_ENABLE, &r), Some(NRF_SUCCESS));
        assert!(is_enabled());
        assert_eq!(mem.read32(0x20001000), 0x2000_2000, "app RAM floor");
        // Queue one event, query length with dest NULL (no pop)...
        post_adv_report([1, 2, 3, 4, 5, 6], -50, false, &[1, 2, 3]);
        mem.write16(0x20003FF0, 0);
        let r = regs(0, 0x20003FF0, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_BLE_EVT_GET, &r), Some(NRF_SUCCESS));
        let need = mem.read16(0x20003FF0);
        assert_eq!(need, 4 + 2 + 7 + 1 + 1 + 1 + 31, "adv_report wire size");
        // ...small room -> DATA_SIZE without popping...
        let (rc, _, _) = drain(&sys, &mut mem, 0x20003000, need - 1);
        assert_eq!(rc, NRF_ERROR_DATA_SIZE);
        assert_eq!(queue_len(), 1, "short read must not pop");
        // ...exact room drains ADV_REPORT with the unpacked layout.
        let (rc, id, len) = drain(&sys, &mut mem, 0x20003000, need);
        assert_eq!((rc, id, len), (NRF_SUCCESS, EVT_GAP_ADV_REPORT, need));
        assert_eq!(mem.read16(0x20003004), 0xFFFF, "no-conn handle");
        assert_eq!(mem.read8(0x20003006), 1, "peer type");
        assert_eq!(mem.read8(0x2000300D), 206, "rssi -50");
        assert_eq!(mem.read8(0x2000300E) >> 3, 3, "dlen 3 in bits3-7");
        assert_eq!(mem.read8(0x2000300F), 0, "pad byte (unpacked)");
        assert_eq!(mem.read8(0x20003010), 1, "data[0]");
        // Drained -> NOT_FOUND again.
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_BLE_EVT_GET, &regs(0x20003000, 0x20003FF0, 0, 0)),
            Some(NRF_ERROR_NOT_FOUND)
        );
    }

    #[test]
    fn gatts_table_service_char_value_roundtrip() {
        let sys = test_dummy_system();
        let mut mem = FlatMemory::new(512 * 1024, 128 * 1024);
        reset_for_test();
        mem.write32(0x20001000, 0);
        let r = regs(0, 0x20001000, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_BLE_ENABLE, &r), Some(NRF_SUCCESS));
        let value = build_battery(&sys, &mut mem);
        // VALUE_SET via the struct contract (conn=INVALID ok): 87 -> 63.
        mem.write8(0x20002000, 63);
        mem.write16(0x20002010, 1); // len
        mem.write16(0x20002012, 0); // offset
        mem.write32(0x20002014, 0x20002000); // *value
        let r = regs(0xFFFF, value as u32, 0x20002010, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GATTS_VALUE_SET, &r), Some(NRF_SUCCESS));
        assert_eq!(batt_level(), 63);
        // VALUE_GET copies back + reports len; NULL data = length query.
        mem.write16(0x20002020, 4); // room
        mem.write16(0x20002022, 0);
        mem.write32(0x20002024, 0x20002008);
        let r = regs(0xFFFF, value as u32, 0x20002020, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GATTS_VALUE_GET, &r), Some(NRF_SUCCESS));
        assert_eq!(mem.read16(0x20002020), 1, "len out");
        assert_eq!(mem.read8(0x20002008), 63);
        // Unknown handle -> NOT_FOUND (not INVALID_ADDR).
        let r = regs(0xFFFF, 0x9999, 0x20002020, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GATTS_VALUE_GET, &r), Some(NRF_ERROR_NOT_FOUND));
        // ATTR_GET reports the table uuid back.
        mem.write16(0x20002030, 0);
        mem.write8(0x20002032, 0);
        let r = regs(value as u32, 0x20002030, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GATTS_ATTR_GET, &r), Some(NRF_SUCCESS));
        assert_eq!(mem.read16(0x20002030), 0x2A19);
    }

    #[test]
    fn gattc_read_write_discovery_hvx_stage_take_complete() {
        let sys = test_dummy_system();
        let mut mem = FlatMemory::new(512 * 1024, 128 * 1024);
        reset_for_test();
        let _ = handle_svc(&sys, &mut mem, SVC_BLE_ENABLE, &[0u32; 13]);
        // Not connected -> INVALID_STATE (silicon rule).
        let r = regs(1, 0x10, 0, 0);
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_GATTC_READ, &r),
            Some(NRF_ERROR_INVALID_STATE)
        );
        connect(&sys, &mut mem);
        // PRIM_DISC stages (conn, start, NULL uuid = all).
        let r = regs(1, 1, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GATTC_PRIMARY_DISC, &r), Some(NRF_SUCCESS));
        assert_eq!(
            take_job(),
            Some(BleJob::GattcPrimDisc { conn: 1, start: 1, uuid16: None })
        );
        complete_prim_disc(1, &[DiscService { uuid16: Some(0x180F), start: 0x10, end: 0x16 }]);
        let (rc, id, len) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!(rc, NRF_SUCCESS);
        assert_eq!(id, EVT_GATTC_PRIM_DISC_RSP);
        assert_eq!(len, 4 + 6 + 2 + 8, "gattc head + count + 1 service");
        assert_eq!(mem.read16(0x20003004), 1, "conn");
        assert_eq!(mem.read16(0x20003006), 0, "status SUCCESS");
        assert_eq!(mem.read16(0x2000300A), 1, "count");
        assert_eq!(mem.read16(0x2000300C), 0x180F, "service uuid");
        // CHAR_DISC stages a range job; completion posts CHAR_DISC_RSP.
        mem.write16(0x20001300, 0x10);
        mem.write16(0x20001302, 0x16);
        let r = regs(1, 0x20001300, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GATTC_CHAR_DISC, &r), Some(NRF_SUCCESS));
        assert_eq!(take_job(), Some(BleJob::GattcCharDisc { conn: 1, start: 0x10, end: 0x16 }));
        complete_char_disc(1, &[DiscChar { uuid16: Some(0x2A19), props: 0x12, decl: 0x12, value: 0x13 }]);
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GATTC_CHAR_DISC_RSP));
        assert_eq!(mem.read8(0x20003010), 0x12, "props byte");
        assert_eq!(mem.read16(0x20003014), 0x13, "value handle");
        // READ stages; completion posts READ_RSP with the gattc head.
        let r = regs(1, 0x13, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GATTC_READ, &r), Some(NRF_SUCCESS));
        assert_eq!(take_job(), Some(BleJob::GattcRead { conn: 1, handle: 0x13, offset: 0 }));
        complete_gattc_read(1, 0x13, 0, &[87]);
        let (rc, id, len) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GATTC_READ_RSP));
        assert_eq!(len, 4 + 6 + 6 + 1, "head + hdl/off/len + 1B");
        assert_eq!(mem.read8(0x20003010), 87, "battery byte");
        // WRITE stages with bytes COPIED at SVC time (not at take).
        mem.write8(0x20001400, GATT_OP_WRITE_REQ);
        mem.write8(0x20001401, 0);
        mem.write16(0x20001402, 0x13);
        mem.write16(0x20001404, 0);
        mem.write16(0x20001406, 2);
        mem.write8(0x20001410, 0xAA);
        mem.write8(0x20001411, 0xBB);
        mem.write32(0x20001408, 0x20001410);
        let r = regs(1, 0x20001400, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GATTC_WRITE, &r), Some(NRF_SUCCESS));
        // Corrupt the source: the staged job must keep the SVC-time copy.
        mem.write8(0x20001410, 0);
        assert_eq!(
            take_job(),
            Some(BleJob::GattcWrite { conn: 1, op: 1, handle: 0x13, data: vec![0xAA, 0xBB] })
        );
        complete_gattc_write(1, 0x13, 1, &[0xAA, 0xBB]);
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GATTC_WRITE_RSP));
        assert_eq!(mem.read8(0x2000300C), 1, "op echo");
        assert_eq!(mem.read8(0x2000300D), 0, "pad after op");
        // HVX from the peer posts HVX with pad-correct layout.
        complete_gattc_hvx(1, 0x13, GATT_HVX_NOTIFICATION, &[0x55]);
        let (rc, id, len) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GATTC_HVX));
        assert_eq!(len, 4 + 6 + 2 + 1 + 1 + 2 + 1);
        assert_eq!(mem.read8(0x20003010), 0x55, "hvx data");
        // Queue drained -> NOT_FOUND again.
        mem.write16(0x20003FF0, 128);
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_BLE_EVT_GET, &regs(0x20003000, 0x20003FF0, 0, 0)),
            Some(NRF_ERROR_NOT_FOUND)
        );
    }

    /// SVC number under test for the read arm (kept beside the test so
    /// a renumber breaks loudly here, not deep in dispatch).
    fn svc_ble_read_wrap() -> u8 {
        SVC_GATTC_READ
    }

    #[test]
    fn gap_disconnect_rssi_scan_hvx_lifecycle() {
        let sys = test_dummy_system();
        let mut mem = FlatMemory::new(512 * 1024, 128 * 1024);
        reset_for_test();
        let _ = handle_svc(&sys, &mut mem, SVC_BLE_ENABLE, &[0u32; 13]);
        connect(&sys, &mut mem);
        // SCAN_START stages; completion posts a padded ADV_REPORT.
        let r = regs(0, 0, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GAP_SCAN_START, &r), Some(NRF_SUCCESS));
        assert_eq!(take_job(), Some(BleJob::GapScanStart));
        post_adv_report([9, 9, 9, 9, 9, 9], -60, true, &[0x02, 0x01, 0x06]);
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GAP_ADV_REPORT));
        assert_eq!(mem.read8(0x2000300E) & 1, 1, "scan_rsp bit");
        // RSSI_GET answers synchronously AND stages air sampling.
        mem.write8(0x20003100, 0);
        let r = regs(1, 0x20003100, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GAP_RSSI_GET, &r), Some(NRF_SUCCESS));
        assert_eq!(take_job(), Some(BleJob::GapRssiGet { conn: 1 }));
        assert_eq!(mem.read8(0x20003100), 206, "cached -50dBm now");
        complete_rssi(1, -60);
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GAP_RSSI_CHANGED));
        assert_eq!(mem.read8(0x20003006), 196, "rssi -60");
        // GATTS HVX needs a table handle: build battery, subscribe the
        // link via a CCCD write (silicon gates notify on subscription),
        // then stage. Unsubscribed HVX must refuse INVALID_STATE first.
        let value = build_battery(&sys, &mut mem);
        mem.write16(0x20003200, value);
        mem.write8(0x20003202, GATT_HVX_NOTIFICATION);
        mem.write16(0x20003204, 0);
        mem.write16(0x20003210, 1);
        mem.write8(0x20003220, 0x42);
        mem.write32(0x20003208, 0x20003210);
        mem.write32(0x2000320C, 0x20003220);
        let r = regs(1, 0x20003200, 0, 0);
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_GATTS_HVX, &r),
            Some(NRF_ERROR_INVALID_STATE),
            "notify without CCCD subscription refuses"
        );
        // Subscribe: the battery char auto-allocated a CCCD at value+1.
        post_gatts_write(1, value + 1, Some(0x2902), GATT_OP_WRITE_REQ, &[0x01, 0x00]);
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GATTS_WRITE));
        let r = regs(1, 0x20003200, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GATTS_HVX, &r), Some(NRF_SUCCESS));
        assert_eq!(
            take_job(),
            Some(BleJob::GattsHvx { conn: 1, handle: value, hvx_type: 1, data: vec![0x42] })
        );
        complete_hvx(1, value);
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GATTS_HVC));
        // Indication needs bit1 instead: re-subscribe indicate-only
        // (first drain the HVC the notify completion just posted),
        // stage an indication, peer confirms via HV_CONFIRM -> HVC.
        post_gatts_write(1, value + 1, Some(0x2902), GATT_OP_WRITE_REQ, &[0x02, 0x00]);
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GATTS_WRITE));
        // Notify now refuses (only indicate subscribed)...
        mem.write8(0x20003202, GATT_HVX_NOTIFICATION);
        let r = regs(1, 0x20003200, 0, 0);
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_GATTS_HVX, &r),
            Some(NRF_ERROR_INVALID_STATE),
            "notify without notify-bit refuses"
        );
        // ...but indication stages, and HV_CONFIRM posts HVC server-side.
        mem.write8(0x20003202, GATT_HVX_INDICATION);
        let r = regs(1, 0x20003200, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GATTS_HVX, &r), Some(NRF_SUCCESS));
        assert_eq!(
            take_job(),
            Some(BleJob::GattsHvx { conn: 1, handle: value, hvx_type: 2, data: vec![0x42] })
        );
        complete_hvx(1, value);
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GATTS_HVC));
        let r = regs(1, value as u32, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GATTC_HV_CONFIRM, &r), Some(NRF_SUCCESS));
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GATTS_HVC));
        // DISCONNECT stages; completion clears the link + posts reason.
        let r = regs(1, 19, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GAP_DISCONNECT, &r), Some(NRF_SUCCESS));
        assert_eq!(take_job(), Some(BleJob::GapDisconnect { conn: 1, reason: 19 }));
        complete_gap_disconnect(1, 19);
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GAP_DISCONNECTED));
        assert_eq!(mem.read8(0x20003006), 19, "HCI reason");
        // Link down: reads go INVALID_STATE again (silicon rule).
        let r = regs(1, value as u32, 0, 0);
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_GATTC_READ, &r),
            Some(NRF_ERROR_INVALID_STATE)
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
        // CONNECTED envelope: conn + addrs + CENTRAL role + params.
        let (rc, id, len) = drain(&sys, &mut mem, 0x20004000, 128);
        assert_eq!(rc, NRF_SUCCESS);
        assert_eq!(id, EVT_GAP_CONNECTED);
        assert_eq!(len, 4 + 2 + 7 + 7 + 1 + 1 + 8, "connected wire size");
        assert_eq!(mem.read16(0x20004004), BRIDGE_CONN_HANDLE);
        assert_eq!(mem.read8(0x20004014), GAP_ROLE_CENTRAL, "we dial out");
    }

    #[test]
    fn gattc_rel_attrinfo_uuid_vals_stage_take_complete() {
        let sys = test_dummy_system();
        let mut mem = FlatMemory::new(512 * 1024, 128 * 1024);
        reset_for_test();
        let _ = handle_svc(&sys, &mut mem, SVC_BLE_ENABLE, &[0u32; 13]);
        connect(&sys, &mut mem);
        // REL_DISC stages its own job (not the DESC funnel).
        mem.write16(0x20001300, 0x10);
        mem.write16(0x20001302, 0x16);
        let r = regs(1, 0x20001300, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GATTC_REL_DISC, &r), Some(NRF_SUCCESS));
        assert_eq!(take_job(), Some(BleJob::GattcRelDisc { conn: 1, start: 0x10, end: 0x16 }));
        complete_rel_disc(1, &[DiscInclude { handle: 0x10, uuid16: Some(0x180F), start: 0x10, end: 0x16 }]);
        let (rc, id, len) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GATTC_REL_DISC_RSP));
        assert_eq!(len, 4 + 6 + 2 + 10, "head + count + 1 include");
        assert_eq!(mem.read16(0x2000300C), 0x10, "include handle");
        assert_eq!(mem.read16(0x2000300E), 0x180F, "included uuid");
        // ATTR_INFO_DISC stages its own job with 16-bit format rows.
        let r = regs(1, 0x20001300, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GATTC_ATTR_INFO_DISC, &r), Some(NRF_SUCCESS));
        assert_eq!(take_job(), Some(BleJob::GattcAttrInfoDisc { conn: 1, start: 0x10, end: 0x16 }));
        complete_attr_info_disc(1, &[DiscAttrInfo { handle: 0x13, uuid16: Some(0x2A19) }]);
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GATTC_ATTR_INFO_RSP));
        assert_eq!(mem.read8(0x2000300C), 1, "16-bit format");
        assert_eq!(mem.read8(0x2000300D) as u16 | ((mem.read8(0x2000300E) as u16) << 8), 0x13, "attr handle");
        assert_eq!(mem.read8(0x2000300F) as u16 | ((mem.read8(0x20003010) as u16) << 8), 0x2A19, "attr uuid");
        // UUID_READ stages (uuid, range); completion posts pairs.
        mem.write16(0x20001310, 0x2A19);
        mem.write8(0x20001312, UUID_TYPE_BLE);
        let r = regs(1, 0x20001310, 0x20001300, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GATTC_UUID_READ, &r), Some(NRF_SUCCESS));
        assert_eq!(
            take_job(),
            Some(BleJob::GattcUuidRead { conn: 1, uuid16: Some(0x2A19), start: 0x10, end: 0x16 })
        );
        complete_uuid_read(1, &[HandleValue { handle: 0x13, value: vec![87] }]);
        let (rc, id, len) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GATTC_UUID_READ_RSP));
        assert_eq!(len, 4 + 6 + 2 + 2 + 2 + 1, "head + count/vlen + hdl + 1B");
        // UUID_READ wire: head(6: conn,status,err) + count@+10 +
        // vlen@+12 + pairs {handle@+14, value@+16}.
        assert_eq!(mem.read8(0x20003010), 87, "uuid-read value byte");
        // VALS_READ copies the handle list at SVC time; completion
        // concatenates.
        mem.write16(0x20001320, 0x13);
        mem.write16(0x20001322, 0x14);
        let r = regs(1, 0x20001320, 2, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GATTC_CHAR_VALS_READ, &r), Some(NRF_SUCCESS));
        mem.write16(0x20001320, 0); // corrupt source: staged keeps copy
        assert_eq!(take_job(), Some(BleJob::GattcValsRead { conn: 1, handles: vec![0x13, 0x14] }));
        complete_vals_read(1, &[87, 0x42]);
        let (rc, id, len) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GATTC_VALS_READ_RSP));
        assert_eq!(len, 4 + 6 + 2 + 2, "head + len + 2B");
        assert_eq!(mem.read8(0x2000300C), 87);
        assert_eq!(mem.read8(0x2000300D), 0x42);
    }

    #[test]
    fn l2cap_register_tx_unregister_roundtrip() {
        let sys = test_dummy_system();
        let mut mem = FlatMemory::new(512 * 1024, 128 * 1024);
        reset_for_test();
        let _ = handle_svc(&sys, &mut mem, SVC_BLE_ENABLE, &[0u32; 13]);
        // Static CID below the dynamic base: silicon INVALID_PARAM.
        let r = regs(0x0004, 0, 0, 0);
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_L2CAP_CID_REGISTER, &r),
            Some(NRF_ERROR_INVALID_PARAM)
        );
        // Register a dynamic CID; double-register = CID_IN_USE.
        let r = regs(0x0040, 0, 0, 0);
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_L2CAP_CID_REGISTER, &r),
            Some(NRF_SUCCESS)
        );
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_L2CAP_CID_REGISTER, &r),
            Some(BLE_ERROR_L2CAP_CID_IN_USE)
        );
        // TX stages with bytes copied at SVC time (header {len, cid}).
        mem.write16(0x20001000, 3); // len
        mem.write16(0x20001002, 0x0040); // cid
        mem.write8(0x20001010, 0xAA);
        mem.write8(0x20001011, 0xBB);
        mem.write8(0x20001012, 0xCC);
        let r = regs(1, 0x20001000, 0x20001010, 0);
        // Not connected yet: INVALID_STATE (silicon rule, no link).
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_L2CAP_TX, &r),
            Some(NRF_ERROR_INVALID_STATE)
        );
        connect(&sys, &mut mem);
        let r = regs(1, 0x20001000, 0x20001010, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_L2CAP_TX, &r), Some(NRF_SUCCESS));
        mem.write8(0x20001010, 0); // corrupt source: staged keeps copy
        assert_eq!(
            take_job(),
            Some(BleJob::L2capTx { conn: 1, cid: 0x0040, data: vec![0xAA, 0xBB, 0xCC] })
        );
        complete_l2cap_rx(1, 0x0040, &[0xAA, 0xBB, 0xCC]);
        let (rc, id, len) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_L2CAP_RX));
        assert_eq!(len, 4 + 2 + 2 + 2 + 3, "hdr + conn/len/cid + 3B");
        assert_eq!(mem.read16(0x20003004), 1, "conn");
        assert_eq!(mem.read16(0x20003008), 0x0040, "cid echo");
        assert_eq!(mem.read8(0x2000300A), 0xAA, "frame byte 0");
        // Unregistered CID: TX refuses INVALID_PARAM, nothing staged.
        let r = regs(0x0041, 0, 0, 0);
        let _ = handle_svc(&sys, &mut mem, SVC_L2CAP_CID_UNREGISTER, &r);
        mem.write16(0x20001002, 0x0041);
        let r = regs(1, 0x20001000, 0x20001010, 0);
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_L2CAP_TX, &r),
            Some(NRF_ERROR_INVALID_PARAM)
        );
        assert_eq!(take_job(), None, "refused TX stages nothing");
        // Unregister twice: second is NOT_FOUND.
        let r = regs(0x0040, 0, 0, 0);
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_L2CAP_CID_UNREGISTER, &r),
            Some(NRF_SUCCESS)
        );
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_L2CAP_CID_UNREGISTER, &r),
            Some(NRF_ERROR_NOT_FOUND)
        );
    }

    #[test]
    fn pairing_request_reply_complete_lifecycle() {
        let sys = test_dummy_system();
        let mut mem = FlatMemory::new(512 * 1024, 128 * 1024);
        reset_for_test();
        let _ = handle_svc(&sys, &mut mem, SVC_BLE_ENABLE, &[0u32; 13]);
        connect(&sys, &mut mem);
        // AUTHENTICATE stages the handshake (BUSY while outstanding).
        let r = regs(1, 0, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GAP_AUTHENTICATE, &r), Some(NRF_SUCCESS));
        assert_eq!(take_job(), Some(BleJob::GapAuthenticate { conn: 1 }));
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_GAP_AUTHENTICATE, &r),
            Some(NRF_ERROR_BUSY)
        );
        // Key replies with a locally-initiated request outstanding are
        // NOT for the driver to consume — the driver owns the handshake
        // (silicon INVALID_STATE: no AUTH_KEY_REQUEST was ever posted).
        let r = regs(1, 0, 0, 0);
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_GAP_AUTH_KEY_REPLY, &r),
            Some(NRF_ERROR_INVALID_STATE)
        );
        // Driver completes over air: AUTH_STATUS success + SEC_UPDATE.
        complete_pairing(1, true);
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GAP_AUTH_STATUS));
        assert_eq!(mem.read8(0x20003006), SEC_STATUS_SUCCESS);
        assert_eq!(mem.read8(0x20003007) & 0x04, 0x04, "bonded bit");
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GAP_CONN_SEC_UPDATE));
        // Link now encrypted: CONN_SEC_GET reports mode1 + 16-octet key.
        mem.write8(0x20003100, 0);
        mem.write8(0x20003101, 0);
        let r = regs(1, 0x20003100, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GAP_CONN_SEC_GET, &r), Some(NRF_SUCCESS));
        assert_eq!(mem.read8(0x20003100), 0x21, "encrypted mode");
        assert_eq!(mem.read8(0x20003101), 16);
        // SEC_PARAMS_REPLY reject path: NULL params -> pair-fail event,
        // link stays up unencrypted (fresh link to show it).
        connect(&sys, &mut mem);
        let h2 = conn_handles().into_iter().max().unwrap();
        let r = regs(h2 as u32, 1, 0, 0);
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_GAP_SEC_PARAMS_REPLY, &r),
            Some(NRF_SUCCESS)
        );
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GAP_AUTH_STATUS));
        assert_eq!(mem.read8(0x20003006), SEC_STATUS_PAIRING_NOT_SUPP);
        // Rejected link still connected: reads stage fine.
        let r = regs(h2 as u32, 0x13, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GATTC_READ, &r), Some(NRF_SUCCESS));
        let _ = take_job();
    }

    #[test]
    fn pairing_peer_request_accept_passkey_oob_encrypt_flow() {
        let sys = test_dummy_system();
        let mut mem = FlatMemory::new(512 * 1024, 128 * 1024);
        reset_for_test();
        let _ = handle_svc(&sys, &mut mem, SVC_BLE_ENABLE, &[0u32; 13]);
        connect(&sys, &mut mem);
        // Peer starts pairing: driver posts SEC_PARAMS_REQUEST (2B
        // conn + 5B peer params). Firmware drains it before replying.
        assert!(post_sec_params_request(1, [0x0D, 7, 16, 0x01, 0x00]));
        let (rc, id, len) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GAP_SEC_PARAMS_REQUEST));
        assert_eq!(len, 4 + 2 + 5, "hdr + conn + peer_params");
        assert_eq!(mem.read16(0x20003004), 1, "conn head");
        assert_eq!(mem.read8(0x20003006), 0x0D, "peer flags echo");
        // Accept needs key memory named; NULL keyset is fine (never
        // dereferenced). Accept stages the air handshake.
        mem.write8(0x20004000, 0x0D);
        mem.write8(0x20004001, 7);
        mem.write8(0x20004002, 16);
        mem.write8(0x20004003, 0x01);
        mem.write8(0x20004004, 0x00);
        let r = regs(1, 0, 0x20004000, 0);
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_GAP_SEC_PARAMS_REPLY, &r),
            Some(NRF_SUCCESS)
        );
        assert_eq!(take_job(), Some(BleJob::GapAuthenticate { conn: 1 }));
        // Passkey entry: driver posts AUTH_KEY_REQUEST(PASSKEY);
        // bad digits refuse, good digits complete the reply.
        assert!(post_auth_key_request(1, 1));
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GAP_AUTH_KEY_REQUEST));
        assert_eq!(mem.read8(0x20003006), 1, "key_type PASSKEY");
        mem.write8(0x20004100, b'1');
        mem.write8(0x20004101, b'2');
        mem.write8(0x20004102, b'X'); // not a digit
        mem.write8(0x20004103, b'4');
        mem.write8(0x20004104, b'5');
        mem.write8(0x20004105, b'6');
        let r = regs(1, 1, 0x20004100, 0);
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_GAP_AUTH_KEY_REPLY, &r),
            Some(NRF_ERROR_INVALID_PARAM)
        );
        mem.write8(0x20004102, b'3');
        let r = regs(1, 1, 0x20004100, 0);
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_GAP_AUTH_KEY_REPLY, &r),
            Some(NRF_SUCCESS)
        );
        // Keypress notify needs the key-entry state: posts KEY_PRESSED.
        let r = regs(1, 0, 0, 0);
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_GAP_KEYPRESS_NOTIFY, &r),
            Some(NRF_SUCCESS)
        );
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GAP_KEY_PRESSED));
        // Driver handshake completes: AUTH_STATUS success + SEC_UPDATE.
        complete_pairing(1, true);
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GAP_AUTH_STATUS));
        assert_eq!(mem.read8(0x20003006), SEC_STATUS_SUCCESS);
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GAP_CONN_SEC_UPDATE));
        // Re-encrypt path on a fresh link: SEC_INFO_REQUEST, no keys ->
        // AUTH_REQ pair-fail, link stays up.
        connect(&sys, &mut mem);
        let h2 = conn_handles().into_iter().max().unwrap();
        let peer_addr = [1u8, 2, 3, 4, 5, 6, 7];
        let master_id = [0x34u8, 0x12, 1, 2, 3, 4, 5, 6, 7, 8];
        assert!(post_sec_info_request(h2, peer_addr, master_id, 0x01));
        let (rc, id, len) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GAP_SEC_INFO_REQUEST));
        assert_eq!(len, 4 + 2 + 7 + 10 + 1, "hdr + conn + addr + mid + req");
        let r = regs(h2 as u32, 0, 0, 0);
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_GAP_SEC_INFO_REPLY, &r),
            Some(NRF_SUCCESS)
        );
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GAP_AUTH_STATUS));
        assert_eq!(mem.read8(0x20003006), SEC_STATUS_AUTH_REQ);
        // Keys found -> EncryptPending; ENCRYPT then answers SUCCESS.
        connect(&sys, &mut mem);
        let h3 = conn_handles().into_iter().max().unwrap();
        assert!(post_sec_info_request(h3, peer_addr, master_id, 0x01));
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GAP_SEC_INFO_REQUEST));
        // 16B LTK block + 8B master id + 18B enc info at RAM.
        for i in 0..16u32 {
            mem.write8(0x20004200 + i, i as u8);
        }
        mem.write8(0x20004210, 1);
        let r = regs(h3 as u32, 0x20004200, 0, 0);
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_GAP_SEC_INFO_REPLY, &r),
            Some(NRF_SUCCESS)
        );
        for i in 0..16u32 {
            mem.write8(0x20004300 + i, 0xA0 + i as u8);
        }
        mem.write8(0x20004310, 0);
        let r = regs(h3 as u32, 0x20004300, 0x20004310, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GAP_ENCRYPT, &r), Some(NRF_SUCCESS));
        // LESC paths: DHKEY request needs Accepted; OOB data zeroes.
        connect(&sys, &mut mem);
        let h4 = conn_handles().into_iter().max().unwrap();
        assert!(!post_lesc_dhkey_request(h4, false), "no handshake yet");
        assert!(post_sec_params_request(h4, [0x09, 7, 16, 0x00, 0x00]));
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GAP_SEC_PARAMS_REQUEST));
        let r = regs(h4 as u32, 0, 0x20004000, 0);
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_GAP_SEC_PARAMS_REPLY, &r),
            Some(NRF_SUCCESS)
        );
        let _ = take_job();
        assert!(post_lesc_dhkey_request(h4, true));
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GAP_LESC_DHKEY_REQUEST));
        assert_eq!(mem.read8(0x20003006), 1, "oobd_req set");
        for i in 0..32u32 {
            mem.write8(0x20004400 + i, 0x55);
        }
        let r = regs(h4 as u32, 0x20004400, 0, 0);
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_GAP_LESC_DHKEY_REPLY, &r),
            Some(NRF_SUCCESS)
        );
        let r = regs(h4 as u32, 0, 0x20004500, 0);
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_GAP_LESC_OOB_DATA_GET, &r),
            Some(NRF_SUCCESS)
        );
        // Passkey display + peer keypress post without a handshake.
        assert!(post_passkey_display(h4, *b"123456", true));
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GAP_PASSKEY_DISPLAY));
        assert_eq!(mem.read8(0x20003006), b'1', "passkey ASCII");
        assert!(post_keypress(h4, 1));
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GAP_KEY_PRESSED));
    }

    #[test]
    fn multi_conn_handles_isolated_state() {
        let sys = test_dummy_system();
        let mut mem = FlatMemory::new(512 * 1024, 128 * 1024);
        reset_for_test();
        let _ = handle_svc(&sys, &mut mem, SVC_BLE_ENABLE, &[0u32; 13]);
        // Two links up: handles differ, both live.
        connect(&sys, &mut mem);
        connect(&sys, &mut mem);
        let hs = conn_handles();
        assert_eq!(hs.len(), 2, "two live links, got {hs:?}");
        assert_ne!(hs[0], hs[1]);
        // Per-link RSSI: sample each, each sync answer is its own.
        for &h in &hs {
            mem.write8(0x20003100, 0);
            let r = regs(h as u32, 0x20003100, 0, 0);
            assert_eq!(handle_svc(&sys, &mut mem, SVC_GAP_RSSI_GET, &r), Some(NRF_SUCCESS));
            assert_eq!(take_job(), Some(BleJob::GapRssiGet { conn: h }));
            complete_rssi(h, -60 - h as i8);
            // Drain the RSSI_CHANGED now: completions queue in order,
            // so the later DISCONNECTED assert sees its own event.
            let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
            assert_eq!((rc, id), (NRF_SUCCESS, EVT_GAP_RSSI_CHANGED));
            assert_eq!(mem.read16(0x20003004), h, "rssi carries its conn");
        }
        // Reads on each handle stage with THAT conn attached.
        for &h in &hs {
            let r = regs(h as u32, 0x13, 0, 0);
            assert_eq!(handle_svc(&sys, &mut mem, SVC_GATTC_READ, &r), Some(NRF_SUCCESS));
            assert_eq!(take_job(), Some(BleJob::GattcRead { conn: h, handle: 0x13, offset: 0 }));
        }
        // Unknown handle with links up: INVALID_CONN_HANDLE.
        let r = regs(0x77, 0x13, 0, 0);
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_GATTC_READ, &r),
            Some(BLE_ERROR_INVALID_CONN_HANDLE)
        );
        // Disconnect the first: second stays live, events carry handles.
        let r = regs(hs[0] as u32, 19, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GAP_DISCONNECT, &r), Some(NRF_SUCCESS));
        complete_gap_disconnect(hs[0], 19);
        let (rc, id, _) = drain(&sys, &mut mem, 0x20003000, 128);
        assert_eq!((rc, id), (NRF_SUCCESS, EVT_GAP_DISCONNECTED));
        assert_eq!(mem.read16(0x20003004), hs[0], "event carries its conn");
        assert_eq!(conn_handles(), vec![hs[1]], "one link left");
        // Down handle now: INVALID_STATE (was live, now torn down).
        let r = regs(hs[0] as u32, 0x13, 0, 0);
        assert_eq!(
            handle_svc(&sys, &mut mem, SVC_GATTC_READ, &r),
            Some(NRF_ERROR_INVALID_STATE)
        );
        // Survivor still stages.
        let r = regs(hs[1] as u32, 0x13, 0, 0);
        assert_eq!(handle_svc(&sys, &mut mem, SVC_GATTC_READ, &r), Some(NRF_SUCCESS));
        let _ = take_job();
    }
}
