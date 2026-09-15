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

export function ble_batt_level(): number;

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

export function ble_enabled(): boolean;

/**
 * Fail a pairing handshake: posts AUTH_STATUS with the S132 status
 * (e.g. 0x29 PAIRING_NOT_SUPP); link stays up, unencrypted.
 */
export function ble_fail_pairing(conn: number, status: number): void;

export function ble_post_adv_report(peer: Uint8Array, rssi: number, scan_rsp: boolean, data: Uint8Array): void;

/**
 * Post a peer write to our table: conn handle, attr handle,
 * uuid16 (0xFFFF = 128-bit/vendor), op (1 = write request), bytes.
 */
export function ble_post_gatts_write(conn: number, handle: number, uuid16: number, op: number, data: Uint8Array): void;

export function ble_queue_len(): number;

/**
 * Bytes staged alongside the last take_job (WRITE/HVX payloads only;
 * the SVC copies firmware bytes at call time so the driver read is
 * stable). Drained once per job; empty when the job carries no bytes.
 */
export function ble_take_data(): Uint8Array;

export function ble_take_job(): Uint32Array;

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

export function gpio_read_input(port: number, pin: number): boolean;

export function gpio_read_output(port: number, pin: number): boolean;

/**
 * Drive a raw input level. Buttons are active-low: released = true
 * (idle pull-up default), pressed = false. JS button layer maps to this.
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

export function radio_complete_rx(): void;

export function radio_complete_tx(): void;

export function radio_inject_corrupt(bytes: Uint8Array): void;

export function radio_inject_rx(bytes: Uint8Array): void;

/**
 * Inject a received packet addressed to a DAB/DAP entry (air peer).
 * Convenience over inject_rx for the two-instance bridge: the first
 * byte is the device-address byte the match unit checks (DEVMATCH
 * when it equals a programmed, listened DAB entry).
 */
export function radio_inject_rx_to(dab_idx: number, bytes: Uint8Array): void;

/**
 * Set the 802.15.4 energy-detect sample level in dBm (negative).
 * Reported via EDSAMPLE on the next EDSTART; defaults to RSSI level.
 */
export function radio_set_ed_dbm(dbm: number): void;

export function radio_set_rssi_dbm(dbm: number): void;

export function radio_take_rx(): Uint32Array;

export function radio_take_tx(): Uint32Array;

/**
 * Clear all process-lifetime globals so a NEW emulator instance starts clean.
 */
export function reset_state(): void;

export function saadc_check_limits(ch: number, value: number): void;

export function saadc_complete_result(amount: number): void;

export function saadc_take_result(): Uint32Array;

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
    readonly ble_batt_level: () => number;
    readonly ble_complete_attr_info_disc: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly ble_complete_char_disc: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => void;
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
    readonly ble_complete_prim_disc: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly ble_complete_rel_disc: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => void;
    readonly ble_complete_rssi: (a: number, b: number) => void;
    readonly ble_complete_uuid_read: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly ble_complete_vals_read: (a: number, b: number, c: number) => void;
    readonly ble_conn_handles: (a: number) => void;
    readonly ble_conn_sec: (a: number, b: number) => void;
    readonly ble_enabled: () => number;
    readonly ble_fail_pairing: (a: number, b: number) => void;
    readonly ble_post_adv_report: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly ble_post_gatts_write: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly ble_queue_len: () => number;
    readonly ble_take_data: (a: number) => void;
    readonly ble_take_job: (a: number) => void;
    readonly ccm_complete: (a: number) => void;
    readonly ccm_take_job: (a: number) => void;
    readonly comp_set_input_mv: (a: number) => void;
    readonly ecb_complete: () => void;
    readonly ecb_take_job: (a: number) => void;
    readonly get_next_pending_interrupt: () => number;
    readonly get_uart_output: (a: number) => void;
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
    readonly radio_complete_rx: () => void;
    readonly radio_complete_tx: () => void;
    readonly radio_inject_corrupt: (a: number, b: number) => void;
    readonly radio_inject_rx: (a: number, b: number) => void;
    readonly radio_inject_rx_to: (a: number, b: number, c: number) => void;
    readonly radio_set_ed_dbm: (a: number) => void;
    readonly radio_set_rssi_dbm: (a: number) => void;
    readonly radio_take_rx: (a: number) => void;
    readonly radio_take_tx: (a: number) => void;
    readonly reset_state: () => void;
    readonly saadc_check_limits: (a: number, b: number) => void;
    readonly saadc_complete_result: (a: number) => void;
    readonly saadc_take_result: (a: number) => void;
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
