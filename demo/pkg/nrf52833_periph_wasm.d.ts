/* tslint:disable */
/* eslint-disable */

export class WasmCpu {
    free(): void;
    [Symbol.dispose](): void;
    fault_len(): number;
    fault_op1(): number;
    fault_op2(): number;
    fault_pc(): number;
    get_fpscr(): number;
    get_ipsr(): number;
    get_pc(): number;
    get_primask(): number;
    get_regs(): Uint32Array;
    get_sp(): number;
    get_sregs(): Uint32Array;
    get_xpsr(): number;
    load_firmware(data: Uint8Array, base: number): void;
    mem_fault(): number;
    mem_read(addr: number, len: number): Uint8Array;
    mem_write(addr: number, data: Uint8Array): void;
    constructor(sp: number, pc: number, flash_size: number, ram_size: number);
    read32(addr: number): number;
    read8(addr: number): number;
    reset_cpu(sp: number, pc: number): void;
    set_deliver_irqs(v: boolean): void;
    set_fpscr(v: number): void;
    set_sreg(i: number, v: number): void;
    sleeping(): boolean;
    /**
     * External-master SPI exchange with our SPIS slave (MISO bytes out).
     */
    spis_exchange(base: number, mosi: Uint8Array): Uint8Array;
    step(budget: number): number;
    take_trace(): Uint32Array;
    trace_start(): void;
    trace_stop(): void;
    /**
     * External-master I2C read from our TWIS slave (ORC-padded).
     */
    twis_master_read(base: number, addr: number, len: number): Uint8Array;
    /**
     * External-master I2C write to our TWIS slave at `base`/`addr7`.
     * Returns bytes accepted (0 on NACK/overflow; see error events).
     */
    twis_master_write(base: number, addr: number, data: Uint8Array): number;
    wake(): void;
    write32(addr: number, v: number): void;
    write8(addr: number, v: number): void;
}

export function aar_complete(resolved: boolean): void;

export function aar_take_job(): Uint32Array;

/**
 * Directed-advertising peer address (6 LE bytes; valid when directed).
 */
export function ble_adv_peer_addr(): Uint8Array;

/**
 * Advertising state: [active, directed, filter_policy, whitelist_addrs].
 * Armed by ADV_START validation, cleared by ADV_STOP / reset.
 */
export function ble_adv_state(): Uint8Array;

export function ble_batt_level(): number;

/**
 * Bond store: does the store hold keys for this 6B peer address with
 * this 10B master_id (silicon re-encrypt gate)? The bridge consults
 * this before answering SEC_INFO_REPLY: hit = reply with stored keys
 * + ENCRYPT; miss = all-NULL reply.
 */
export function ble_bond_has_keys(peer: Uint8Array, master_id: Uint8Array): boolean;

/**
 * Bond store: read back bonded keys (LTK[16] IRK[16] CSRK[16] MID[10]
 * = 52 bytes, empty when no bond). Bridge answers SEC_INFO_REPLY
 * from this instead of failing.
 */
export function ble_bond_read_keys(peer: Uint8Array): Uint8Array;

/**
 * Complete an attribute-info discovery: handles[i], uuids[i].
 * Posts ATTR_INFO_RSP (16-bit format).
 */
export function ble_complete_attr_info_disc(conn: number, handles: Uint16Array, uuids: Uint16Array): void;

/**
 * Complete a characteristic discovery: uuids[i] (0xFFFF = 128-bit),
 * props[i] (S132 u8 bitfield), decls[i], values[i]. Posts CHAR_DISC_RSP.
 */
export function ble_complete_char_disc(conn: number, uuids: Uint16Array, props: Uint8Array, decls: Uint16Array, values: Uint16Array): void;

/**
 * Conn-param update completion: posts CONN_PARAM_UPDATE on the link.
 */
export function ble_complete_conn_param_update(conn: number): void;

/**
 * Complete a descriptor discovery: handles[i], uuids[i].
 * Posts DESC_DISC_RSP.
 */
export function ble_complete_desc_disc(conn: number, handles: Uint16Array, uuids: Uint16Array): void;

export function ble_complete_gap_connect(peer: Uint8Array): void;

/**
 * Complete a GAP connect: driver connected over air; returns the
 * assigned connection handle (INVALID when the table is full).
 */
export function ble_complete_gap_connect_ret(peer: Uint8Array): number;

/**
 * Complete a GAP disconnect: posts DISCONNECTED with the HCI reason.
 */
export function ble_complete_gap_disconnect(conn: number, reason: number): void;

/**
 * Complete a peer notification/indication: posts HVX.
 */
export function ble_complete_gattc_hvx(conn: number, handle: number, hvx_type: number, data: Uint8Array): void;

export function ble_complete_gattc_read(conn: number, handle: number, offset: number, data: Uint8Array): void;

/**
 * Complete a GATTC write with the over-air WRITE_RSP proof.
 */
export function ble_complete_gattc_write(conn: number, handle: number, op: number, data: Uint8Array): void;

/**
 * Complete a GATTS HVX emission: posts HVC confirm.
 */
export function ble_complete_hvx(conn: number, handle: number): void;

/**
 * Complete an L2CAP TX: posts the RX echo on (conn, cid).
 */
export function ble_complete_l2cap_rx(conn: number, cid: number, data: Uint8Array): void;

/**
 * Complete a pairing handshake the driver ran over air: posts
 * AUTH_STATUS (success) + CONN_SEC_UPDATE, marks link bonded.
 */
export function ble_complete_pairing(conn: number, bonded: boolean): void;

/**
 * Peripheral-role accept: a peer answered our advertisement; brings
 * the link up with PERIPH role and posts CONNECTED. Returns handle.
 */
export function ble_complete_peripheral_connect(peer: Uint8Array): number;

/**
 * Complete a primary-service discovery with parallel arrays:
 * uuids[i] (0xFFFF = 128-bit, listed without number), starts[i],
 * ends[i]. Posts PRIM_DISC_RSP.
 */
export function ble_complete_prim_disc(conn: number, uuids: Uint16Array, starts: Uint16Array, ends: Uint16Array): void;

/**
 * Complete a relationship discovery: parallel arrays handles[i],
 * uuids[i] (0xFFFF = 128-bit), starts[i], ends[i]. Posts REL_DISC_RSP.
 */
export function ble_complete_rel_disc(conn: number, handles: Uint16Array, uuids: Uint16Array, starts: Uint16Array, ends: Uint16Array): void;

/**
 * Complete an RSSI sample: posts RSSI_CHANGED.
 */
export function ble_complete_rssi(conn: number, rssi: number): void;

/**
 * Complete a Service Changed indication: the peer confirmed the
 * 0x2A05 indication over air; posts SC_CONFIRM on the link.
 */
export function ble_complete_service_changed(conn: number): void;

/**
 * TX-flow refill: driver moved one packet over air; refills one TX
 * token on the link and posts TX_COMPLETE with the free count.
 */
export function ble_complete_tx_flow(conn: number): void;

/**
 * Complete a read-by-UUID: parallel handles[i] + flat values with
 * per-pair lengths lens[i] (ragged pads to the longest on the wire).
 * Posts UUID_READ_RSP.
 */
export function ble_complete_uuid_read(conn: number, handles: Uint16Array, flat: Uint8Array, lens: Uint16Array): void;

/**
 * Complete a multi-read: concatenated values. Posts VALS_READ_RSP.
 */
export function ble_complete_vals_read(conn: number, data: Uint8Array): void;

/**
 * Live connection handles (each u16 one link). Empty = no links.
 */
export function ble_conn_handles(): Uint16Array;

/**
 * Connection security: [sec_mode, key_size] for the link
 * (mode 0x11 open, 0x21 encrypted-after-pairing).
 */
export function ble_conn_sec(conn: number): Uint8Array;

/**
 * Explicit unbond: the next SEC_INFO_REQUEST for the peer MISSES.
 */
export function ble_delete_bond(peer: Uint8Array): boolean;

export function ble_enabled(): boolean;

/**
 * Fail a pairing handshake: posts AUTH_STATUS with the S132 status
 * (e.g. 0x85 PAIRING_NOT_SUPP); link stays up, unencrypted.
 */
export function ble_fail_pairing(conn: number, status: number): void;

/**
 * SMP toolbox (Core Spec Vol 3, Part H, 2.2.5–2.2.9): the pairing
 * crypto the SoftDevice leaves to firmware/host. All inputs/outputs
 * are SMP protocol order (little-endian).
 *
 * P-256 ECDH shared secret: our BE private scalar + peer LE point
 * (X ++ Y) -> DHKey LE, or empty when the point is off-curve
 * (silicon fails the procedure; the reply SVC refuses INVALID_PARAM).
 */
export function ble_lesc_dhkey(own_priv_be: Uint8Array, peer_x_le: Uint8Array, peer_y_le: Uint8Array): Uint8Array;

/**
 * Our P-256 public key (SMP LE order X ++ Y, 64 bytes) from our BE
 * private scalar. Empty on a bad scalar (never for RNG-fed scalars).
 */
export function ble_lesc_public_key(own_priv_be: Uint8Array): Uint8Array;

export function ble_post_adv_report(peer: Uint8Array, rssi: number, scan_rsp: boolean, data: Uint8Array): void;

/**
 * Post an AUTH_KEY_REQUEST: the driver needs a key of `key_type`
 * (0 none, 1 passkey, 2 OOB); firmware answers AUTH_KEY_REPLY.
 * Returns false outside an accepted handshake.
 */
export function ble_post_auth_key_request(conn: number, key_type: number): boolean;

/**
 * Post a peer CONN_PARAM_UPDATE_REQUEST (firmware answers with the
 * CONN_PARAM_UPDATE request SVC).
 */
export function ble_post_conn_param_update_request(conn: number): boolean;

/**
 * Post a GAP TIMEOUT (src 0 adv, 1 sec-req, 2 scan, 3 conn).
 */
export function ble_post_gap_timeout(conn: number, src: number): boolean;

/**
 * Post a GATTC TIMEOUT (ATT protocol).
 */
export function ble_post_gattc_timeout(conn: number): boolean;

/**
 * Post a GATTS TIMEOUT (ATT protocol).
 */
export function ble_post_gatts_timeout(conn: number): boolean;

/**
 * Post a peer write to our table: conn handle, attr handle,
 * uuid16 (0xFFFF = 128-bit/vendor), op (1 = write request), bytes.
 */
export function ble_post_gatts_write(conn: number, handle: number, uuid16: number, op: number, data: Uint8Array): void;

/**
 * Post a peer KEYPRESS_NOTIFY (type 0..=4). Returns false with no link.
 */
export function ble_post_keypress(conn: number, kp_not: number): boolean;

/**
 * Post an LESC_DHKEY_REQUEST (firmware answers LESC_DHKEY_REPLY;
 * OOB via LESC_OOB_DATA_SET when oobd_req). Returns false outside an
 * accepted handshake.
 */
export function ble_post_lesc_dhkey_request(conn: number, oobd_req: boolean): boolean;

/**
 * Post a PASSKEY_DISPLAY: the driver shows this 6-digit ASCII passkey
 * (firmware answers AUTH_KEY_REPLY when match_request).
 */
export function ble_post_passkey_display(conn: number, passkey: Uint8Array, match_request: boolean): boolean;

/**
 * Post a GATTS RW_AUTHORIZE_REQUEST (firmware answers RW_AUTHORIZE_REPLY).
 */
export function ble_post_rw_authorize_request(conn: number, auth_type: number, handle: number, offset: number, op: number, data: Uint8Array): boolean;

/**
 * Post a GATTS SC_CONFIRM (header only, no reply path).
 */
export function ble_post_sc_confirm(conn: number): boolean;

/**
 * Post a SCAN_REQ_REPORT (a scanner hit our advertisement).
 */
export function ble_post_scan_req_report(peer: Uint8Array, rssi: number): boolean;

/**
 * Post a peer-initiated SEC_INFO_REQUEST: the peer asks to re-encrypt
 * (peer_addr 7B type+6, master_id 10B ediv+rand[8], req bits: bit0
 * enc_info, bit1 id_info, bit2 sign_info). Firmware answers
 * SEC_INFO_REPLY, then ENCRYPT. Returns false when the link cannot
 * take a request.
 */
export function ble_post_sec_info_request(conn: number, peer_addr: Uint8Array, master_id: Uint8Array, req: number): boolean;

/**
 * Post a peer-initiated SEC_PARAMS_REQUEST: the peer started SMP
 * with these ble_gap_sec_params_t wire bytes (flags, min/max key
 * size, kdist_own, kdist_peer); firmware answers SEC_PARAMS_REPLY.
 * Returns false when the link cannot take a request.
 */
export function ble_post_sec_params_request(conn: number, peer_params: Uint8Array): boolean;

/**
 * Post a peer SEC_REQUEST (firmware answers AUTHENTICATE).
 */
export function ble_post_sec_request(conn: number, bond: boolean, mitm: boolean, lesc: boolean, keypress: boolean): boolean;

/**
 * Post a GATTS SYS_ATTR_MISSING (firmware answers SYS_ATTR_SET).
 */
export function ble_post_sys_attr_missing(conn: number): boolean;

/**
 * Post a USER_MEM_RELEASE (informational, no reply path).
 */
export function ble_post_user_mem_release(conn: number, mem_type: number): boolean;

/**
 * Post a USER_MEM_REQUEST (firmware answers USER_MEM_REPLY).
 */
export function ble_post_user_mem_request(conn: number, mem_type: number): boolean;

export function ble_queue_len(): number;

/**
 * f4 confirm value (LE 16B): peer/local public X coords (LE 32B
 * each), random (LE 16B), Z byte.
 */
export function ble_smp_f4(u_le: Uint8Array, v_le: Uint8Array, x_le: Uint8Array, z: number): Uint8Array;

/**
 * f5 key generation: DHKey (LE 32B), nonces (LE 16B), addrs (LE 7B)
 * -> MacKey ++ LTK (LE 16B each, 32 bytes).
 */
export function ble_smp_f5(w_le: Uint8Array, n1_le: Uint8Array, n2_le: Uint8Array, a1_le: Uint8Array, a2_le: Uint8Array): Uint8Array;

/**
 * f6 DHKey-check (LE 16B): MacKey (LE 16B), nonces (LE 16B),
 * r (LE 16B), IOcap (3B), addrs (LE 7B).
 */
export function ble_smp_f6(w_le: Uint8Array, n1_le: Uint8Array, n2_le: Uint8Array, r_le: Uint8Array, iocap: Uint8Array, a1_le: Uint8Array, a2_le: Uint8Array): Uint8Array;

/**
 * g2 numeric comparison: public X coords (LE 32B), nonces (LE 16B)
 * -> u32 (firmware shows % 1000000, 6 digits).
 */
export function ble_smp_g2(u_le: Uint8Array, v_le: Uint8Array, x_le: Uint8Array, y_le: Uint8Array): number;

/**
 * Bond store: driver-side insert (bridge confirmed air keys when
 * firmware passed NULL keysets).
 */
export function ble_store_bond(peer: Uint8Array, ltk: Uint8Array, irk: Uint8Array, csrk: Uint8Array, master_id: Uint8Array): void;

/**
 * Bytes staged alongside the last take_job (WRITE/HVX payloads only;
 * the SVC copies firmware bytes at call time so the driver read is
 * stable). Drained once per job; empty when the job carries no bytes.
 */
export function ble_take_data(): Uint8Array;

export function ble_take_job(): Uint32Array;

/**
 * GAP TX power level in dBm, as stored by TX_POWER_SET (debug/export).
 * Default 0 (silicon reset); only the S132-legal set is ever stored.
 */
export function ble_tx_power_dbm(): number;

export function ccm_complete(mic_ok: boolean): void;

export function ccm_take_job(): Uint32Array;

export function comp_set_input_mv(mv: number): void;

export function ecb_complete(): void;

export function ecb_take_job(): Uint32Array;

export function get_next_pending_interrupt(): number;

/**
 * Collect UART output since last call.
 */
export function get_uart_output(): string;

/**
 * Direction bit: true = firmware configured the pin as output
 * (PIN_CNF.DIR source of truth, kept in sync by the model).
 * OpenHW matrix/buttons render needs this: an OUT latch toggling on
 * an input pin must stay dark (see the bench frame loop).
 */
export function gpio_read_dir(port: number, pin: number): boolean;

export function gpio_read_input(port: number, pin: number): boolean;

export function gpio_read_output(port: number, pin: number): boolean;

/**
 * Drive a raw input level. Buttons are active-low: released = true
 * (idle pull-up default), pressed = false. JS button layer maps to this.
 * NFC antenna pins (P0.09/P0.10 with UICR.NFCPINS PROTECT=1, the reset
 * state) ignore levels — silicon routes them to the NFCT front-end.
 */
export function gpio_set_input(port: number, pin: number, value: boolean): void;

export function has_pending_interrupt(): boolean;

export function i2c_push_rx(peripheral: string, bytes: Uint8Array): void;

export function i2c_register_slave(peripheral: string, address: number): void;

export function i2c_take_events(peripheral: string): Uint32Array;

export function i2s_complete_rx(): void;

export function i2s_complete_tx(bytes: Uint8Array): void;

export function i2s_take_capture(): Uint8Array;

export function i2s_take_rx(): Uint32Array;

export function i2s_take_tx(): Uint32Array;

/**
 * Initialize the emulator with the nRF52833 hardcoded peripheral map.
 */
export function init(): void;

/**
 * Initialize the emulator from an SVD XML string (e.g., nrf52833.svd).
 */
export function init_svd(svd_xml: string): void;

/**
 * True when firmware requested a reboot (AIRCR SYSRESETREQ / WDT).
 * The JS driver must then reset the CPU from the vector table
 * (MicroPython does this twice during boot).
 */
export function is_watchdog_reset_requested(): boolean;

/**
 * 5x5 LED matrix state for an OpenHW matrix component: 25 bytes,
 * row-major, 1 = lit. Lit <=> row OUT==0 && col OUT==1 with both pins
 * configured output (same rule the bench frame loop uses).
 * Rows: P0.21/P0.22/P0.15/P0.24/P0.19. Cols: P0.28/P0.11/P0.31/P1.05/P0.30.
 */
export function matrix_state(): Uint8Array;

/**
 * MMIO trace controls (P134 forensics; also exported so JS harnesses
 * can capture the firmware's UARTE register sequence around a stall).
 */
export function mmio_trace_start(): void;

export function mmio_trace_take(): Uint32Array;

export function nfct_complete_rx(amount: number): void;

export function nfct_complete_tx(): void;

export function nfct_field_present(present: boolean): void;

export function nfct_take_rx(): Uint32Array;

export function nfct_take_tx(): Uint32Array;

export function nvmc_complete_erase(): void;

export function nvmc_take_erase(): Uint32Array;

export function pdm_complete_sample(): void;

export function pdm_take_sample(): Uint32Array;

export function periph_read(addr: number, width: number): number;

export function periph_write(addr: number, width: number, value: number): void;

export function qdec_step(dir: number): void;

export function qspi_complete_erase(ptr: number, len_code: number): void;

export function qspi_complete_read(): void;

export function qspi_complete_write(dst: number, data: Uint8Array): void;

export function qspi_register_flash(name: string, data: Uint8Array): void;

export function qspi_take_erase(): Uint32Array;

export function qspi_take_read(): Uint32Array;

export function qspi_take_write(): Uint32Array;

/**
 * Link-budget air level: TX dBm minus path loss, clamped [-127, 0].
 * Pure function so JS air and the model agree on one honest number.
 */
export function radio_air_rssi_dbm(tx_code: number, path_loss_db: number): number;

/**
 * Clear the ambient floor (quiet air again).
 */
export function radio_clear_interference(): void;

export function radio_complete_rx(): void;

/**
 * Complete RX with an explicit path loss (dB) for this packet's RSSI
 * stamp. Driver-side air calls this when it knows the range; the
 * plain complete_rx() keeps the queued/default loss.
 */
export function radio_complete_rx_with_path_loss(path_loss_db: number): void;

export function radio_complete_tx(): void;

/**
 * CRC over body with the RADIO engine shape (len 1..3, poly, init).
 * Pure function: the same wire algorithm the RX completion runs.
 */
export function radio_crc32(body: Uint8Array, poly: number, init: number, len: number): number;

export function radio_inject_corrupt(bytes: Uint8Array): void;

export function radio_inject_rx(bytes: Uint8Array): void;

/**
 * Plain inject with a path-loss in dB (same RSSI stamp, no address byte).
 */
export function radio_inject_rx_lossy(bytes: Uint8Array, path_loss_db: number): void;

/**
 * Inject a received packet addressed to a DAB/DAP entry (air peer).
 * Convenience over inject_rx for the two-instance bridge: the first
 * byte is the device-address byte the match unit checks (DEVMATCH
 * when it equals a programmed, listened DAB entry).
 */
export function radio_inject_rx_to(dab_idx: number, bytes: Uint8Array): void;

/**
 * Addressed inject with a path-loss in dB (air range model): the RX
 * completion stamps TXPOWER-minus-loss into the RSSI latch, so a
 * firmware RSSISTART after RX reads this packet's level like silicon.
 */
export function radio_inject_rx_to_lossy(dab_idx: number, bytes: Uint8Array, path_loss_db: number): void;

/**
 * Set the 802.15.4 energy-detect sample level in dBm (negative).
 * Reported via EDSAMPLE on the next EDSTART; defaults to RSSI level.
 */
export function radio_set_ed_dbm(dbm: number): void;

/**
 * Ambient RF floor in dBm (negative) for the interference model:
 * ED/CCA add it in log-power and RX completions heat toward it.
 */
export function radio_set_interference_dbm(dbm: number): void;

export function radio_set_rssi_dbm(dbm: number): void;

export function radio_take_rx(): Uint32Array;

export function radio_take_tx(): Uint32Array;

/**
 * nRF52 TXPOWER code (SVD 0x50C) as signed dBm (+8..0, -4..-40).
 * Pure function for the driver link-budget (shared with the model).
 */
export function radio_txpower_dbm(code: number): number;

/**
 * Whiten (de-whiten — same operation) bytes in place with the nRF
 * 7-bit LFSR + DATAWHITEIV seed. Pure function for driver-side air.
 */
export function radio_whiten(bytes: Uint8Array, iv: number): Uint8Array;

/**
 * Clear all process-lifetime globals so a NEW emulator instance starts clean.
 */
export function reset_state(): void;

export function saadc_check_limits(ch: number, value: number): void;

export function saadc_complete_result(amount: number): void;

export function saadc_take_result(): Uint32Array;

/**
 * SoC event queue length (sd_evt phase 1: flash completions while the
 * SD is enabled). Debug/pump path; firmware drains via SVC 82.
 */
export function sd_evt_queue_len(): number;

export function set_intr_pending(irq: number): void;

export function spi_push_miso(peripheral: string, bytes: Uint8Array): void;

export function spi_take_events(peripheral: string): Uint32Array;

export function spi_tap(peripheral: string, cs?: string | null, dc?: string | null): void;

export function temp_set_celsius(c: number): void;

export function tick(): void;

export function tick_n(delta: number): void;

export function tick_peripherals(): void;

export function twim_complete_rxdma(peripheral: string, amount: number): void;

export function twim_complete_txdma(peripheral: string, data: Uint8Array): void;

export function twim_take_rxdma(peripheral: string): Uint32Array;

export function twim_take_txdma(peripheral: string): Uint32Array;

/**
 * Inject a received byte into the UARTE peripheral at the given base address.
 */
export function uart_rx_byte(addr: number, byte: number): boolean;

export function uarte_complete_rxdma(amount: number): void;

export function uarte_complete_txdma(bytes: Uint8Array): void;

/**
 * Take a staged UARTE RX transfer [ptr, maxcnt]; driver writes bytes to
 * guest RAM at ptr, then calls uarte_complete_rxdma(amount).
 */
export function uarte_take_rxdma(): Uint32Array;

export function uarte_take_txdma(): Uint32Array;

export function usbd_complete_epin(ep: number, bytes: Uint8Array): void;

export function usbd_complete_epout(ep: number, amount: number): void;

export function usbd_inject_setup(bytes: Uint8Array): void;

export function usbd_signal_reset(): void;

export function usbd_take_epin(): Uint32Array;

export function usbd_take_epout(): Uint32Array;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_wasmcpu_free: (a: number, b: number) => void;
    readonly aar_complete: (a: number) => void;
    readonly aar_take_job: (a: number) => void;
    readonly ble_adv_peer_addr: (a: number) => void;
    readonly ble_adv_state: (a: number) => void;
    readonly ble_batt_level: () => number;
    readonly ble_bond_has_keys: (a: number, b: number, c: number, d: number) => number;
    readonly ble_bond_read_keys: (a: number, b: number, c: number) => void;
    readonly ble_complete_attr_info_disc: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly ble_complete_char_disc: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => void;
    readonly ble_complete_conn_param_update: (a: number) => void;
    readonly ble_complete_desc_disc: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly ble_complete_gap_connect: (a: number, b: number) => void;
    readonly ble_complete_gap_connect_ret: (a: number, b: number) => number;
    readonly ble_complete_gap_disconnect: (a: number, b: number) => void;
    readonly ble_complete_gattc_hvx: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly ble_complete_gattc_read: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly ble_complete_gattc_write: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly ble_complete_hvx: (a: number, b: number) => void;
    readonly ble_complete_l2cap_rx: (a: number, b: number, c: number, d: number) => void;
    readonly ble_complete_pairing: (a: number, b: number) => void;
    readonly ble_complete_peripheral_connect: (a: number, b: number) => number;
    readonly ble_complete_prim_disc: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly ble_complete_rel_disc: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => void;
    readonly ble_complete_rssi: (a: number, b: number) => void;
    readonly ble_complete_service_changed: (a: number) => void;
    readonly ble_complete_tx_flow: (a: number) => void;
    readonly ble_complete_uuid_read: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly ble_complete_vals_read: (a: number, b: number, c: number) => void;
    readonly ble_conn_handles: (a: number) => void;
    readonly ble_conn_sec: (a: number, b: number) => void;
    readonly ble_delete_bond: (a: number, b: number) => number;
    readonly ble_enabled: () => number;
    readonly ble_fail_pairing: (a: number, b: number) => void;
    readonly ble_lesc_dhkey: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly ble_lesc_public_key: (a: number, b: number, c: number) => void;
    readonly ble_post_adv_report: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly ble_post_auth_key_request: (a: number, b: number) => number;
    readonly ble_post_conn_param_update_request: (a: number) => number;
    readonly ble_post_gap_timeout: (a: number, b: number) => number;
    readonly ble_post_gattc_timeout: (a: number) => number;
    readonly ble_post_gatts_timeout: (a: number) => number;
    readonly ble_post_gatts_write: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly ble_post_keypress: (a: number, b: number) => number;
    readonly ble_post_lesc_dhkey_request: (a: number, b: number) => number;
    readonly ble_post_passkey_display: (a: number, b: number, c: number, d: number) => number;
    readonly ble_post_rw_authorize_request: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => number;
    readonly ble_post_sc_confirm: (a: number) => number;
    readonly ble_post_scan_req_report: (a: number, b: number, c: number) => number;
    readonly ble_post_sec_info_request: (a: number, b: number, c: number, d: number, e: number, f: number) => number;
    readonly ble_post_sec_params_request: (a: number, b: number, c: number) => number;
    readonly ble_post_sec_request: (a: number, b: number, c: number, d: number, e: number) => number;
    readonly ble_post_sys_attr_missing: (a: number) => number;
    readonly ble_post_user_mem_release: (a: number, b: number) => number;
    readonly ble_post_user_mem_request: (a: number, b: number) => number;
    readonly ble_queue_len: () => number;
    readonly ble_smp_f4: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => void;
    readonly ble_smp_f5: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number) => void;
    readonly ble_smp_f6: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number) => void;
    readonly ble_smp_g2: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => number;
    readonly ble_store_bond: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number) => void;
    readonly ble_take_data: (a: number) => void;
    readonly ble_take_job: (a: number) => void;
    readonly ble_tx_power_dbm: () => number;
    readonly ccm_complete: (a: number) => void;
    readonly ccm_take_job: (a: number) => void;
    readonly comp_set_input_mv: (a: number) => void;
    readonly ecb_complete: () => void;
    readonly ecb_take_job: (a: number) => void;
    readonly get_next_pending_interrupt: () => number;
    readonly get_uart_output: (a: number) => void;
    readonly gpio_read_dir: (a: number, b: number) => number;
    readonly gpio_read_input: (a: number, b: number) => number;
    readonly gpio_read_output: (a: number, b: number) => number;
    readonly gpio_set_input: (a: number, b: number, c: number) => void;
    readonly has_pending_interrupt: () => number;
    readonly i2c_push_rx: (a: number, b: number, c: number, d: number) => void;
    readonly i2c_register_slave: (a: number, b: number, c: number) => void;
    readonly i2c_take_events: (a: number, b: number, c: number) => void;
    readonly i2s_complete_rx: () => void;
    readonly i2s_complete_tx: (a: number, b: number) => void;
    readonly i2s_take_capture: (a: number) => void;
    readonly i2s_take_rx: (a: number) => void;
    readonly i2s_take_tx: (a: number) => void;
    readonly init: () => void;
    readonly init_svd: (a: number, b: number) => void;
    readonly is_watchdog_reset_requested: () => number;
    readonly matrix_state: (a: number) => void;
    readonly mmio_trace_start: () => void;
    readonly mmio_trace_take: (a: number) => void;
    readonly nfct_complete_rx: (a: number) => void;
    readonly nfct_complete_tx: () => void;
    readonly nfct_field_present: (a: number) => void;
    readonly nfct_take_rx: (a: number) => void;
    readonly nfct_take_tx: (a: number) => void;
    readonly nvmc_complete_erase: () => void;
    readonly nvmc_take_erase: (a: number) => void;
    readonly pdm_complete_sample: () => void;
    readonly pdm_take_sample: (a: number) => void;
    readonly periph_read: (a: number, b: number) => number;
    readonly periph_write: (a: number, b: number, c: number) => void;
    readonly qdec_step: (a: number) => void;
    readonly qspi_complete_erase: (a: number, b: number) => void;
    readonly qspi_complete_read: () => void;
    readonly qspi_complete_write: (a: number, b: number, c: number) => void;
    readonly qspi_register_flash: (a: number, b: number, c: number, d: number) => void;
    readonly qspi_take_erase: (a: number) => void;
    readonly qspi_take_read: (a: number) => void;
    readonly qspi_take_write: (a: number) => void;
    readonly radio_air_rssi_dbm: (a: number, b: number) => number;
    readonly radio_clear_interference: () => void;
    readonly radio_complete_rx: () => void;
    readonly radio_complete_rx_with_path_loss: (a: number) => void;
    readonly radio_complete_tx: () => void;
    readonly radio_crc32: (a: number, b: number, c: number, d: number, e: number) => number;
    readonly radio_inject_corrupt: (a: number, b: number) => void;
    readonly radio_inject_rx: (a: number, b: number) => void;
    readonly radio_inject_rx_lossy: (a: number, b: number, c: number) => void;
    readonly radio_inject_rx_to: (a: number, b: number, c: number) => void;
    readonly radio_inject_rx_to_lossy: (a: number, b: number, c: number, d: number) => void;
    readonly radio_set_ed_dbm: (a: number) => void;
    readonly radio_set_interference_dbm: (a: number) => void;
    readonly radio_set_rssi_dbm: (a: number) => void;
    readonly radio_take_rx: (a: number) => void;
    readonly radio_take_tx: (a: number) => void;
    readonly radio_txpower_dbm: (a: number) => number;
    readonly radio_whiten: (a: number, b: number, c: number, d: number) => void;
    readonly reset_state: () => void;
    readonly saadc_check_limits: (a: number, b: number) => void;
    readonly saadc_complete_result: (a: number) => void;
    readonly saadc_take_result: (a: number) => void;
    readonly sd_evt_queue_len: () => number;
    readonly set_intr_pending: (a: number) => void;
    readonly spi_push_miso: (a: number, b: number, c: number, d: number) => void;
    readonly spi_take_events: (a: number, b: number, c: number) => void;
    readonly spi_tap: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly temp_set_celsius: (a: number) => void;
    readonly tick: () => void;
    readonly tick_n: (a: number) => void;
    readonly tick_peripherals: () => void;
    readonly twim_complete_rxdma: (a: number, b: number, c: number) => void;
    readonly twim_complete_txdma: (a: number, b: number, c: number, d: number) => void;
    readonly twim_take_rxdma: (a: number, b: number, c: number) => void;
    readonly twim_take_txdma: (a: number, b: number, c: number) => void;
    readonly uart_rx_byte: (a: number, b: number) => number;
    readonly uarte_complete_rxdma: (a: number) => void;
    readonly uarte_complete_txdma: (a: number, b: number) => void;
    readonly uarte_take_rxdma: (a: number) => void;
    readonly uarte_take_txdma: (a: number) => void;
    readonly usbd_complete_epin: (a: number, b: number, c: number) => void;
    readonly usbd_complete_epout: (a: number, b: number) => void;
    readonly usbd_inject_setup: (a: number, b: number) => void;
    readonly usbd_signal_reset: () => void;
    readonly usbd_take_epin: (a: number) => void;
    readonly usbd_take_epout: (a: number) => void;
    readonly wasmcpu_fault_len: (a: number) => number;
    readonly wasmcpu_fault_op1: (a: number) => number;
    readonly wasmcpu_fault_op2: (a: number) => number;
    readonly wasmcpu_fault_pc: (a: number) => number;
    readonly wasmcpu_get_fpscr: (a: number) => number;
    readonly wasmcpu_get_ipsr: (a: number) => number;
    readonly wasmcpu_get_pc: (a: number) => number;
    readonly wasmcpu_get_primask: (a: number) => number;
    readonly wasmcpu_get_regs: (a: number, b: number) => void;
    readonly wasmcpu_get_sp: (a: number) => number;
    readonly wasmcpu_get_sregs: (a: number, b: number) => void;
    readonly wasmcpu_get_xpsr: (a: number) => number;
    readonly wasmcpu_load_firmware: (a: number, b: number, c: number, d: number) => void;
    readonly wasmcpu_mem_fault: (a: number) => number;
    readonly wasmcpu_mem_read: (a: number, b: number, c: number, d: number) => void;
    readonly wasmcpu_mem_write: (a: number, b: number, c: number, d: number) => void;
    readonly wasmcpu_new: (a: number, b: number, c: number, d: number) => number;
    readonly wasmcpu_read32: (a: number, b: number) => number;
    readonly wasmcpu_read8: (a: number, b: number) => number;
    readonly wasmcpu_reset_cpu: (a: number, b: number, c: number) => void;
    readonly wasmcpu_set_deliver_irqs: (a: number, b: number) => void;
    readonly wasmcpu_set_fpscr: (a: number, b: number) => void;
    readonly wasmcpu_set_sreg: (a: number, b: number, c: number) => void;
    readonly wasmcpu_sleeping: (a: number) => number;
    readonly wasmcpu_spis_exchange: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly wasmcpu_step: (a: number, b: number) => number;
    readonly wasmcpu_take_trace: (a: number, b: number) => void;
    readonly wasmcpu_trace_start: (a: number) => void;
    readonly wasmcpu_trace_stop: (a: number) => void;
    readonly wasmcpu_twis_master_read: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly wasmcpu_twis_master_write: (a: number, b: number, c: number, d: number, e: number) => number;
    readonly wasmcpu_wake: (a: number) => void;
    readonly wasmcpu_write32: (a: number, b: number, c: number) => void;
    readonly wasmcpu_write8: (a: number, b: number, c: number) => void;
    readonly __wbindgen_export: (a: number, b: number, c: number) => void;
    readonly __wbindgen_export2: (a: number, b: number) => number;
    readonly __wbindgen_export3: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
