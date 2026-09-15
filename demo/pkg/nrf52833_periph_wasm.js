/* @ts-self-types="./nrf52833_periph_wasm.d.ts" */

export class WasmCpu {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmCpuFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmcpu_free(ptr, 0);
    }
    /**
     * @returns {number}
     */
    fault_len() {
        const ret = wasm.wasmcpu_fault_len(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    fault_op1() {
        const ret = wasm.wasmcpu_fault_op1(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    fault_op2() {
        const ret = wasm.wasmcpu_fault_op2(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    fault_pc() {
        const ret = wasm.wasmcpu_fault_pc(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    get_fpscr() {
        const ret = wasm.wasmcpu_get_fpscr(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    get_ipsr() {
        const ret = wasm.wasmcpu_get_ipsr(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    get_pc() {
        const ret = wasm.wasmcpu_get_pc(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    get_primask() {
        const ret = wasm.wasmcpu_get_primask(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {Uint32Array}
     */
    get_regs() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.wasmcpu_get_regs(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var v1 = getArrayU32FromWasm0(r0, r1).slice();
            wasm.__wbindgen_export(r0, r1 * 4, 4);
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @returns {number}
     */
    get_sp() {
        const ret = wasm.wasmcpu_get_sp(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {Uint32Array}
     */
    get_sregs() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.wasmcpu_get_sregs(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var v1 = getArrayU32FromWasm0(r0, r1).slice();
            wasm.__wbindgen_export(r0, r1 * 4, 4);
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @returns {number}
     */
    get_xpsr() {
        const ret = wasm.wasmcpu_get_xpsr(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @param {Uint8Array} data
     * @param {number} base
     */
    load_firmware(data, base) {
        const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.wasmcpu_load_firmware(this.__wbg_ptr, ptr0, len0, base);
    }
    /**
     * @returns {number}
     */
    mem_fault() {
        const ret = wasm.wasmcpu_mem_fault(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @param {number} addr
     * @param {number} len
     * @returns {Uint8Array}
     */
    mem_read(addr, len) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.wasmcpu_mem_read(retptr, this.__wbg_ptr, addr, len);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var v1 = getArrayU8FromWasm0(r0, r1).slice();
            wasm.__wbindgen_export(r0, r1 * 1, 1);
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @param {number} addr
     * @param {Uint8Array} data
     */
    mem_write(addr, data) {
        const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.wasmcpu_mem_write(this.__wbg_ptr, addr, ptr0, len0);
    }
    /**
     * @param {number} sp
     * @param {number} pc
     * @param {number} flash_size
     * @param {number} ram_size
     */
    constructor(sp, pc, flash_size, ram_size) {
        const ret = wasm.wasmcpu_new(sp, pc, flash_size, ram_size);
        this.__wbg_ptr = ret;
        WasmCpuFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * @param {number} addr
     * @returns {number}
     */
    read32(addr) {
        const ret = wasm.wasmcpu_read32(this.__wbg_ptr, addr);
        return ret >>> 0;
    }
    /**
     * @param {number} addr
     * @returns {number}
     */
    read8(addr) {
        const ret = wasm.wasmcpu_read8(this.__wbg_ptr, addr);
        return ret;
    }
    /**
     * @param {number} sp
     * @param {number} pc
     */
    reset_cpu(sp, pc) {
        wasm.wasmcpu_reset_cpu(this.__wbg_ptr, sp, pc);
    }
    /**
     * @param {boolean} v
     */
    set_deliver_irqs(v) {
        wasm.wasmcpu_set_deliver_irqs(this.__wbg_ptr, v);
    }
    /**
     * @param {number} v
     */
    set_fpscr(v) {
        wasm.wasmcpu_set_fpscr(this.__wbg_ptr, v);
    }
    /**
     * @param {number} i
     * @param {number} v
     */
    set_sreg(i, v) {
        wasm.wasmcpu_set_sreg(this.__wbg_ptr, i, v);
    }
    /**
     * @returns {boolean}
     */
    sleeping() {
        const ret = wasm.wasmcpu_sleeping(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * External-master SPI exchange with our SPIS slave (MISO bytes out).
     * @param {number} base
     * @param {Uint8Array} mosi
     * @returns {Uint8Array}
     */
    spis_exchange(base, mosi) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passArray8ToWasm0(mosi, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            wasm.wasmcpu_spis_exchange(retptr, this.__wbg_ptr, base, ptr0, len0);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var v2 = getArrayU8FromWasm0(r0, r1).slice();
            wasm.__wbindgen_export(r0, r1 * 1, 1);
            return v2;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @param {number} budget
     * @returns {number}
     */
    step(budget) {
        const ret = wasm.wasmcpu_step(this.__wbg_ptr, budget);
        return ret >>> 0;
    }
    /**
     * @returns {Uint32Array}
     */
    take_trace() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.wasmcpu_take_trace(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var v1 = getArrayU32FromWasm0(r0, r1).slice();
            wasm.__wbindgen_export(r0, r1 * 4, 4);
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    trace_start() {
        wasm.wasmcpu_trace_start(this.__wbg_ptr);
    }
    trace_stop() {
        wasm.wasmcpu_trace_stop(this.__wbg_ptr);
    }
    /**
     * External-master I2C read from our TWIS slave (ORC-padded).
     * @param {number} base
     * @param {number} addr
     * @param {number} len
     * @returns {Uint8Array}
     */
    twis_master_read(base, addr, len) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.wasmcpu_twis_master_read(retptr, this.__wbg_ptr, base, addr, len);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var v1 = getArrayU8FromWasm0(r0, r1).slice();
            wasm.__wbindgen_export(r0, r1 * 1, 1);
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * External-master I2C write to our TWIS slave at `base`/`addr7`.
     * Returns bytes accepted (0 on NACK/overflow; see error events).
     * @param {number} base
     * @param {number} addr
     * @param {Uint8Array} data
     * @returns {number}
     */
    twis_master_write(base, addr, data) {
        const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmcpu_twis_master_write(this.__wbg_ptr, base, addr, ptr0, len0);
        return ret >>> 0;
    }
    wake() {
        wasm.wasmcpu_wake(this.__wbg_ptr);
    }
    /**
     * @param {number} addr
     * @param {number} v
     */
    write32(addr, v) {
        wasm.wasmcpu_write32(this.__wbg_ptr, addr, v);
    }
    /**
     * @param {number} addr
     * @param {number} v
     */
    write8(addr, v) {
        wasm.wasmcpu_write8(this.__wbg_ptr, addr, v);
    }
}
if (Symbol.dispose) WasmCpu.prototype[Symbol.dispose] = WasmCpu.prototype.free;

/**
 * @param {boolean} resolved
 */
export function aar_complete(resolved) {
    wasm.aar_complete(resolved);
}

/**
 * @returns {Uint32Array}
 */
export function aar_take_job() {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.aar_take_job(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v1 = getArrayU32FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 4, 4);
        return v1;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @returns {number}
 */
export function ble_batt_level() {
    const ret = wasm.ble_batt_level();
    return ret;
}

/**
 * Complete an attribute-info discovery: handles[i], uuids[i].
 * Posts ATTR_INFO_RSP (16-bit format).
 * @param {number} conn
 * @param {Uint16Array} handles
 * @param {Uint16Array} uuids
 */
export function ble_complete_attr_info_disc(conn, handles, uuids) {
    const ptr0 = passArray16ToWasm0(handles, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray16ToWasm0(uuids, wasm.__wbindgen_export2);
    const len1 = WASM_VECTOR_LEN;
    wasm.ble_complete_attr_info_disc(conn, ptr0, len0, ptr1, len1);
}

/**
 * Complete a characteristic discovery: uuids[i] (0xFFFF = 128-bit),
 * props[i] (S132 u8 bitfield), decls[i], values[i]. Posts CHAR_DISC_RSP.
 * @param {number} conn
 * @param {Uint16Array} uuids
 * @param {Uint8Array} props
 * @param {Uint16Array} decls
 * @param {Uint16Array} values
 */
export function ble_complete_char_disc(conn, uuids, props, decls, values) {
    const ptr0 = passArray16ToWasm0(uuids, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray8ToWasm0(props, wasm.__wbindgen_export2);
    const len1 = WASM_VECTOR_LEN;
    const ptr2 = passArray16ToWasm0(decls, wasm.__wbindgen_export2);
    const len2 = WASM_VECTOR_LEN;
    const ptr3 = passArray16ToWasm0(values, wasm.__wbindgen_export2);
    const len3 = WASM_VECTOR_LEN;
    wasm.ble_complete_char_disc(conn, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3);
}

/**
 * Complete a descriptor discovery: handles[i], uuids[i].
 * Posts DESC_DISC_RSP.
 * @param {number} conn
 * @param {Uint16Array} handles
 * @param {Uint16Array} uuids
 */
export function ble_complete_desc_disc(conn, handles, uuids) {
    const ptr0 = passArray16ToWasm0(handles, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray16ToWasm0(uuids, wasm.__wbindgen_export2);
    const len1 = WASM_VECTOR_LEN;
    wasm.ble_complete_desc_disc(conn, ptr0, len0, ptr1, len1);
}

/**
 * @param {Uint8Array} peer
 */
export function ble_complete_gap_connect(peer) {
    const ptr0 = passArray8ToWasm0(peer, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    wasm.ble_complete_gap_connect(ptr0, len0);
}

/**
 * Complete a GAP connect: driver connected over air; returns the
 * assigned connection handle (INVALID when the table is full).
 * @param {Uint8Array} peer
 * @returns {number}
 */
export function ble_complete_gap_connect_ret(peer) {
    const ptr0 = passArray8ToWasm0(peer, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.ble_complete_gap_connect_ret(ptr0, len0);
    return ret;
}

/**
 * Complete a GAP disconnect: posts DISCONNECTED with the HCI reason.
 * @param {number} conn
 * @param {number} reason
 */
export function ble_complete_gap_disconnect(conn, reason) {
    wasm.ble_complete_gap_disconnect(conn, reason);
}

/**
 * Complete a peer notification/indication: posts HVX.
 * @param {number} conn
 * @param {number} handle
 * @param {number} hvx_type
 * @param {Uint8Array} data
 */
export function ble_complete_gattc_hvx(conn, handle, hvx_type, data) {
    const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    wasm.ble_complete_gattc_hvx(conn, handle, hvx_type, ptr0, len0);
}

/**
 * @param {number} conn
 * @param {number} handle
 * @param {number} offset
 * @param {Uint8Array} data
 */
export function ble_complete_gattc_read(conn, handle, offset, data) {
    const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    wasm.ble_complete_gattc_read(conn, handle, offset, ptr0, len0);
}

/**
 * Complete a GATTC write with the over-air WRITE_RSP proof.
 * @param {number} conn
 * @param {number} handle
 * @param {number} op
 * @param {Uint8Array} data
 */
export function ble_complete_gattc_write(conn, handle, op, data) {
    const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    wasm.ble_complete_gattc_write(conn, handle, op, ptr0, len0);
}

/**
 * Complete a GATTS HVX emission: posts HVC confirm.
 * @param {number} conn
 * @param {number} handle
 */
export function ble_complete_hvx(conn, handle) {
    wasm.ble_complete_hvx(conn, handle);
}

/**
 * Complete an L2CAP TX: posts the RX echo on (conn, cid).
 * @param {number} conn
 * @param {number} cid
 * @param {Uint8Array} data
 */
export function ble_complete_l2cap_rx(conn, cid, data) {
    const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    wasm.ble_complete_l2cap_rx(conn, cid, ptr0, len0);
}

/**
 * Complete a pairing handshake the driver ran over air: posts
 * AUTH_STATUS (success) + CONN_SEC_UPDATE, marks link bonded.
 * @param {number} conn
 * @param {boolean} bonded
 */
export function ble_complete_pairing(conn, bonded) {
    wasm.ble_complete_pairing(conn, bonded);
}

/**
 * Complete a primary-service discovery with parallel arrays:
 * uuids[i] (0xFFFF = 128-bit, listed without number), starts[i],
 * ends[i]. Posts PRIM_DISC_RSP.
 * @param {number} conn
 * @param {Uint16Array} uuids
 * @param {Uint16Array} starts
 * @param {Uint16Array} ends
 */
export function ble_complete_prim_disc(conn, uuids, starts, ends) {
    const ptr0 = passArray16ToWasm0(uuids, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray16ToWasm0(starts, wasm.__wbindgen_export2);
    const len1 = WASM_VECTOR_LEN;
    const ptr2 = passArray16ToWasm0(ends, wasm.__wbindgen_export2);
    const len2 = WASM_VECTOR_LEN;
    wasm.ble_complete_prim_disc(conn, ptr0, len0, ptr1, len1, ptr2, len2);
}

/**
 * Complete a relationship discovery: parallel arrays handles[i],
 * uuids[i] (0xFFFF = 128-bit), starts[i], ends[i]. Posts REL_DISC_RSP.
 * @param {number} conn
 * @param {Uint16Array} handles
 * @param {Uint16Array} uuids
 * @param {Uint16Array} starts
 * @param {Uint16Array} ends
 */
export function ble_complete_rel_disc(conn, handles, uuids, starts, ends) {
    const ptr0 = passArray16ToWasm0(handles, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray16ToWasm0(uuids, wasm.__wbindgen_export2);
    const len1 = WASM_VECTOR_LEN;
    const ptr2 = passArray16ToWasm0(starts, wasm.__wbindgen_export2);
    const len2 = WASM_VECTOR_LEN;
    const ptr3 = passArray16ToWasm0(ends, wasm.__wbindgen_export2);
    const len3 = WASM_VECTOR_LEN;
    wasm.ble_complete_rel_disc(conn, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3);
}

/**
 * Complete an RSSI sample: posts RSSI_CHANGED.
 * @param {number} conn
 * @param {number} rssi
 */
export function ble_complete_rssi(conn, rssi) {
    wasm.ble_complete_rssi(conn, rssi);
}

/**
 * Complete a read-by-UUID: parallel handles[i] + flat values with
 * per-pair lengths lens[i] (ragged pads to the longest on the wire).
 * Posts UUID_READ_RSP.
 * @param {number} conn
 * @param {Uint16Array} handles
 * @param {Uint8Array} flat
 * @param {Uint16Array} lens
 */
export function ble_complete_uuid_read(conn, handles, flat, lens) {
    const ptr0 = passArray16ToWasm0(handles, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray8ToWasm0(flat, wasm.__wbindgen_export2);
    const len1 = WASM_VECTOR_LEN;
    const ptr2 = passArray16ToWasm0(lens, wasm.__wbindgen_export2);
    const len2 = WASM_VECTOR_LEN;
    wasm.ble_complete_uuid_read(conn, ptr0, len0, ptr1, len1, ptr2, len2);
}

/**
 * Complete a multi-read: concatenated values. Posts VALS_READ_RSP.
 * @param {number} conn
 * @param {Uint8Array} data
 */
export function ble_complete_vals_read(conn, data) {
    const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    wasm.ble_complete_vals_read(conn, ptr0, len0);
}

/**
 * Live connection handles (each u16 one link). Empty = no links.
 * @returns {Uint16Array}
 */
export function ble_conn_handles() {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.ble_conn_handles(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v1 = getArrayU16FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 2, 2);
        return v1;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Connection security: [sec_mode, key_size] for the link
 * (mode 0x11 open, 0x21 encrypted-after-pairing).
 * @param {number} conn
 * @returns {Uint8Array}
 */
export function ble_conn_sec(conn) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.ble_conn_sec(retptr, conn);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v1 = getArrayU8FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 1, 1);
        return v1;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @returns {boolean}
 */
export function ble_enabled() {
    const ret = wasm.ble_enabled();
    return ret !== 0;
}

/**
 * Fail a pairing handshake: posts AUTH_STATUS with the S132 status
 * (e.g. 0x29 PAIRING_NOT_SUPP); link stays up, unencrypted.
 * @param {number} conn
 * @param {number} status
 */
export function ble_fail_pairing(conn, status) {
    wasm.ble_fail_pairing(conn, status);
}

/**
 * @param {Uint8Array} peer
 * @param {number} rssi
 * @param {boolean} scan_rsp
 * @param {Uint8Array} data
 */
export function ble_post_adv_report(peer, rssi, scan_rsp, data) {
    const ptr0 = passArray8ToWasm0(peer, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray8ToWasm0(data, wasm.__wbindgen_export2);
    const len1 = WASM_VECTOR_LEN;
    wasm.ble_post_adv_report(ptr0, len0, rssi, scan_rsp, ptr1, len1);
}

/**
 * Post a peer write to our table: conn handle, attr handle,
 * uuid16 (0xFFFF = 128-bit/vendor), op (1 = write request), bytes.
 * @param {number} conn
 * @param {number} handle
 * @param {number} uuid16
 * @param {number} op
 * @param {Uint8Array} data
 */
export function ble_post_gatts_write(conn, handle, uuid16, op, data) {
    const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    wasm.ble_post_gatts_write(conn, handle, uuid16, op, ptr0, len0);
}

/**
 * @returns {number}
 */
export function ble_queue_len() {
    const ret = wasm.ble_queue_len();
    return ret >>> 0;
}

/**
 * Bytes staged alongside the last take_job (WRITE/HVX payloads only;
 * the SVC copies firmware bytes at call time so the driver read is
 * stable). Drained once per job; empty when the job carries no bytes.
 * @returns {Uint8Array}
 */
export function ble_take_data() {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.ble_take_data(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v1 = getArrayU8FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 1, 1);
        return v1;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @returns {Uint32Array}
 */
export function ble_take_job() {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.ble_take_job(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v1 = getArrayU32FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 4, 4);
        return v1;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @param {boolean} mic_ok
 */
export function ccm_complete(mic_ok) {
    wasm.ccm_complete(mic_ok);
}

/**
 * @returns {Uint32Array}
 */
export function ccm_take_job() {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.ccm_take_job(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v1 = getArrayU32FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 4, 4);
        return v1;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @param {number} mv
 */
export function comp_set_input_mv(mv) {
    wasm.comp_set_input_mv(mv);
}

export function ecb_complete() {
    wasm.ecb_complete();
}

/**
 * @returns {Uint32Array}
 */
export function ecb_take_job() {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.ecb_take_job(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v1 = getArrayU32FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 4, 4);
        return v1;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @returns {number}
 */
export function get_next_pending_interrupt() {
    const ret = wasm.get_next_pending_interrupt();
    return ret;
}

/**
 * Collect UART output since last call.
 * @returns {string}
 */
export function get_uart_output() {
    let deferred1_0;
    let deferred1_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.get_uart_output(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred1_0 = r0;
        deferred1_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export(deferred1_0, deferred1_1, 1);
    }
}

/**
 * @param {number} port
 * @param {number} pin
 * @returns {boolean}
 */
export function gpio_read_input(port, pin) {
    const ret = wasm.gpio_read_input(port, pin);
    return ret !== 0;
}

/**
 * @param {number} port
 * @param {number} pin
 * @returns {boolean}
 */
export function gpio_read_output(port, pin) {
    const ret = wasm.gpio_read_output(port, pin);
    return ret !== 0;
}

/**
 * Drive a raw input level. Buttons are active-low: released = true
 * (idle pull-up default), pressed = false. JS button layer maps to this.
 * @param {number} port
 * @param {number} pin
 * @param {boolean} value
 */
export function gpio_set_input(port, pin, value) {
    wasm.gpio_set_input(port, pin, value);
}

/**
 * @returns {boolean}
 */
export function has_pending_interrupt() {
    const ret = wasm.has_pending_interrupt();
    return ret !== 0;
}

/**
 * @param {string} peripheral
 * @param {Uint8Array} bytes
 */
export function i2c_push_rx(peripheral, bytes) {
    const ptr0 = passStringToWasm0(peripheral, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray8ToWasm0(bytes, wasm.__wbindgen_export2);
    const len1 = WASM_VECTOR_LEN;
    wasm.i2c_push_rx(ptr0, len0, ptr1, len1);
}

/**
 * @param {string} peripheral
 * @param {number} address
 */
export function i2c_register_slave(peripheral, address) {
    const ptr0 = passStringToWasm0(peripheral, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
    const len0 = WASM_VECTOR_LEN;
    wasm.i2c_register_slave(ptr0, len0, address);
}

/**
 * @param {string} peripheral
 * @returns {Uint32Array}
 */
export function i2c_take_events(peripheral) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(peripheral, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
        const len0 = WASM_VECTOR_LEN;
        wasm.i2c_take_events(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v2 = getArrayU32FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 4, 4);
        return v2;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

export function i2s_complete_rx() {
    wasm.i2s_complete_rx();
}

/**
 * @param {Uint8Array} bytes
 */
export function i2s_complete_tx(bytes) {
    const ptr0 = passArray8ToWasm0(bytes, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    wasm.i2s_complete_tx(ptr0, len0);
}

/**
 * @returns {Uint8Array}
 */
export function i2s_take_capture() {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.i2s_take_capture(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v1 = getArrayU8FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 1, 1);
        return v1;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @returns {Uint32Array}
 */
export function i2s_take_rx() {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.i2s_take_rx(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v1 = getArrayU32FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 4, 4);
        return v1;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @returns {Uint32Array}
 */
export function i2s_take_tx() {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.i2s_take_tx(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v1 = getArrayU32FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 4, 4);
        return v1;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Initialize the emulator with the nRF52833 hardcoded peripheral map.
 */
export function init() {
    wasm.init();
}

/**
 * Initialize the emulator from an SVD XML string (e.g., nrf52833.svd).
 * @param {string} svd_xml
 */
export function init_svd(svd_xml) {
    const ptr0 = passStringToWasm0(svd_xml, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
    const len0 = WASM_VECTOR_LEN;
    wasm.init_svd(ptr0, len0);
}

/**
 * True when firmware requested a reboot (AIRCR SYSRESETREQ / WDT).
 * The JS driver must then reset the CPU from the vector table
 * (MicroPython does this twice during boot).
 * @returns {boolean}
 */
export function is_watchdog_reset_requested() {
    const ret = wasm.is_watchdog_reset_requested();
    return ret !== 0;
}

/**
 * @param {number} amount
 */
export function nfct_complete_rx(amount) {
    wasm.nfct_complete_rx(amount);
}

export function nfct_complete_tx() {
    wasm.nfct_complete_tx();
}

/**
 * @param {boolean} present
 */
export function nfct_field_present(present) {
    wasm.nfct_field_present(present);
}

/**
 * @returns {Uint32Array}
 */
export function nfct_take_rx() {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.nfct_take_rx(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v1 = getArrayU32FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 4, 4);
        return v1;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @returns {Uint32Array}
 */
export function nfct_take_tx() {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.nfct_take_tx(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v1 = getArrayU32FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 4, 4);
        return v1;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

export function nvmc_complete_erase() {
    wasm.nvmc_complete_erase();
}

/**
 * @returns {Uint32Array}
 */
export function nvmc_take_erase() {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.nvmc_take_erase(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v1 = getArrayU32FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 4, 4);
        return v1;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

export function pdm_complete_sample() {
    wasm.pdm_complete_sample();
}

/**
 * @returns {Uint32Array}
 */
export function pdm_take_sample() {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.pdm_take_sample(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v1 = getArrayU32FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 4, 4);
        return v1;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @param {number} addr
 * @param {number} width
 * @returns {number}
 */
export function periph_read(addr, width) {
    const ret = wasm.periph_read(addr, width);
    return ret >>> 0;
}

/**
 * @param {number} addr
 * @param {number} width
 * @param {number} value
 */
export function periph_write(addr, width, value) {
    wasm.periph_write(addr, width, value);
}

/**
 * @param {number} dir
 */
export function qdec_step(dir) {
    wasm.qdec_step(dir);
}

/**
 * @param {number} ptr
 * @param {number} len_code
 */
export function qspi_complete_erase(ptr, len_code) {
    wasm.qspi_complete_erase(ptr, len_code);
}

export function qspi_complete_read() {
    wasm.qspi_complete_read();
}

/**
 * @param {number} dst
 * @param {Uint8Array} data
 */
export function qspi_complete_write(dst, data) {
    const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    wasm.qspi_complete_write(dst, ptr0, len0);
}

/**
 * @param {string} name
 * @param {Uint8Array} data
 */
export function qspi_register_flash(name, data) {
    const ptr0 = passStringToWasm0(name, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray8ToWasm0(data, wasm.__wbindgen_export2);
    const len1 = WASM_VECTOR_LEN;
    wasm.qspi_register_flash(ptr0, len0, ptr1, len1);
}

/**
 * @returns {Uint32Array}
 */
export function qspi_take_erase() {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.qspi_take_erase(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v1 = getArrayU32FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 4, 4);
        return v1;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @returns {Uint32Array}
 */
export function qspi_take_read() {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.qspi_take_read(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v1 = getArrayU32FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 4, 4);
        return v1;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @returns {Uint32Array}
 */
export function qspi_take_write() {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.qspi_take_write(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v1 = getArrayU32FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 4, 4);
        return v1;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

export function radio_complete_rx() {
    wasm.radio_complete_rx();
}

export function radio_complete_tx() {
    wasm.radio_complete_tx();
}

/**
 * @param {Uint8Array} bytes
 */
export function radio_inject_corrupt(bytes) {
    const ptr0 = passArray8ToWasm0(bytes, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    wasm.radio_inject_corrupt(ptr0, len0);
}

/**
 * @param {Uint8Array} bytes
 */
export function radio_inject_rx(bytes) {
    const ptr0 = passArray8ToWasm0(bytes, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    wasm.radio_inject_rx(ptr0, len0);
}

/**
 * Inject a received packet addressed to a DAB/DAP entry (air peer).
 * Convenience over inject_rx for the two-instance bridge: the first
 * byte is the device-address byte the match unit checks (DEVMATCH
 * when it equals a programmed, listened DAB entry).
 * @param {number} dab_idx
 * @param {Uint8Array} bytes
 */
export function radio_inject_rx_to(dab_idx, bytes) {
    const ptr0 = passArray8ToWasm0(bytes, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    wasm.radio_inject_rx_to(dab_idx, ptr0, len0);
}

/**
 * Set the 802.15.4 energy-detect sample level in dBm (negative).
 * Reported via EDSAMPLE on the next EDSTART; defaults to RSSI level.
 * @param {number} dbm
 */
export function radio_set_ed_dbm(dbm) {
    wasm.radio_set_ed_dbm(dbm);
}

/**
 * @param {number} dbm
 */
export function radio_set_rssi_dbm(dbm) {
    wasm.radio_set_rssi_dbm(dbm);
}

/**
 * @returns {Uint32Array}
 */
export function radio_take_rx() {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.radio_take_rx(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v1 = getArrayU32FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 4, 4);
        return v1;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @returns {Uint32Array}
 */
export function radio_take_tx() {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.radio_take_tx(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v1 = getArrayU32FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 4, 4);
        return v1;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Clear all process-lifetime globals so a NEW emulator instance starts clean.
 */
export function reset_state() {
    wasm.reset_state();
}

/**
 * @param {number} ch
 * @param {number} value
 */
export function saadc_check_limits(ch, value) {
    wasm.saadc_check_limits(ch, value);
}

/**
 * @param {number} amount
 */
export function saadc_complete_result(amount) {
    wasm.saadc_complete_result(amount);
}

/**
 * @returns {Uint32Array}
 */
export function saadc_take_result() {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.saadc_take_result(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v1 = getArrayU32FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 4, 4);
        return v1;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @param {number} irq
 */
export function set_intr_pending(irq) {
    wasm.set_intr_pending(irq);
}

/**
 * @param {string} peripheral
 * @param {Uint8Array} bytes
 */
export function spi_push_miso(peripheral, bytes) {
    const ptr0 = passStringToWasm0(peripheral, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray8ToWasm0(bytes, wasm.__wbindgen_export2);
    const len1 = WASM_VECTOR_LEN;
    wasm.spi_push_miso(ptr0, len0, ptr1, len1);
}

/**
 * @param {string} peripheral
 * @returns {Uint32Array}
 */
export function spi_take_events(peripheral) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(peripheral, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
        const len0 = WASM_VECTOR_LEN;
        wasm.spi_take_events(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v2 = getArrayU32FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 4, 4);
        return v2;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @param {string} peripheral
 * @param {string | null} [cs]
 * @param {string | null} [dc]
 */
export function spi_tap(peripheral, cs, dc) {
    const ptr0 = passStringToWasm0(peripheral, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
    const len0 = WASM_VECTOR_LEN;
    var ptr1 = isLikeNone(cs) ? 0 : passStringToWasm0(cs, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
    var len1 = WASM_VECTOR_LEN;
    var ptr2 = isLikeNone(dc) ? 0 : passStringToWasm0(dc, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
    var len2 = WASM_VECTOR_LEN;
    wasm.spi_tap(ptr0, len0, ptr1, len1, ptr2, len2);
}

/**
 * @param {number} c
 */
export function temp_set_celsius(c) {
    wasm.temp_set_celsius(c);
}

export function tick() {
    wasm.tick();
}

/**
 * @param {number} delta
 */
export function tick_n(delta) {
    wasm.tick_n(delta);
}

export function tick_peripherals() {
    wasm.tick_peripherals();
}

/**
 * @param {string} peripheral
 * @param {number} amount
 */
export function twim_complete_rxdma(peripheral, amount) {
    const ptr0 = passStringToWasm0(peripheral, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
    const len0 = WASM_VECTOR_LEN;
    wasm.twim_complete_rxdma(ptr0, len0, amount);
}

/**
 * @param {string} peripheral
 * @param {Uint8Array} data
 */
export function twim_complete_txdma(peripheral, data) {
    const ptr0 = passStringToWasm0(peripheral, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray8ToWasm0(data, wasm.__wbindgen_export2);
    const len1 = WASM_VECTOR_LEN;
    wasm.twim_complete_txdma(ptr0, len0, ptr1, len1);
}

/**
 * @param {string} peripheral
 * @returns {Uint32Array}
 */
export function twim_take_rxdma(peripheral) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(peripheral, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
        const len0 = WASM_VECTOR_LEN;
        wasm.twim_take_rxdma(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v2 = getArrayU32FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 4, 4);
        return v2;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @param {string} peripheral
 * @returns {Uint32Array}
 */
export function twim_take_txdma(peripheral) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(peripheral, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
        const len0 = WASM_VECTOR_LEN;
        wasm.twim_take_txdma(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v2 = getArrayU32FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 4, 4);
        return v2;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Inject a received byte into the UARTE peripheral at the given base address.
 * @param {number} addr
 * @param {number} byte
 * @returns {boolean}
 */
export function uart_rx_byte(addr, byte) {
    const ret = wasm.uart_rx_byte(addr, byte);
    return ret !== 0;
}

/**
 * @param {number} amount
 */
export function uarte_complete_rxdma(amount) {
    wasm.uarte_complete_rxdma(amount);
}

/**
 * @param {Uint8Array} bytes
 */
export function uarte_complete_txdma(bytes) {
    const ptr0 = passArray8ToWasm0(bytes, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    wasm.uarte_complete_txdma(ptr0, len0);
}

/**
 * Take a staged UARTE RX transfer [ptr, maxcnt]; driver writes bytes to
 * guest RAM at ptr, then calls uarte_complete_rxdma(amount).
 * @returns {Uint32Array}
 */
export function uarte_take_rxdma() {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.uarte_take_rxdma(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v1 = getArrayU32FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 4, 4);
        return v1;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @returns {Uint32Array}
 */
export function uarte_take_txdma() {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.uarte_take_txdma(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v1 = getArrayU32FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 4, 4);
        return v1;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @param {number} ep
 * @param {Uint8Array} bytes
 */
export function usbd_complete_epin(ep, bytes) {
    const ptr0 = passArray8ToWasm0(bytes, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    wasm.usbd_complete_epin(ep, ptr0, len0);
}

/**
 * @param {number} ep
 * @param {number} amount
 */
export function usbd_complete_epout(ep, amount) {
    wasm.usbd_complete_epout(ep, amount);
}

/**
 * @param {Uint8Array} bytes
 */
export function usbd_inject_setup(bytes) {
    const ptr0 = passArray8ToWasm0(bytes, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    wasm.usbd_inject_setup(ptr0, len0);
}

export function usbd_signal_reset() {
    wasm.usbd_signal_reset();
}

/**
 * @returns {Uint32Array}
 */
export function usbd_take_epin() {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.usbd_take_epin(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v1 = getArrayU32FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 4, 4);
        return v1;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @returns {Uint32Array}
 */
export function usbd_take_epout() {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.usbd_take_epout(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var v1 = getArrayU32FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 4, 4);
        return v1;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg___wbindgen_throw_344f42d3211c4765: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_error_a6fa202b58aa1cd3: function(arg0, arg1) {
            let deferred0_0;
            let deferred0_1;
            try {
                deferred0_0 = arg0;
                deferred0_1 = arg1;
                console.error(getStringFromWasm0(arg0, arg1));
            } finally {
                wasm.__wbindgen_export(deferred0_0, deferred0_1, 1);
            }
        },
        __wbg_new_227d7c05414eb861: function() {
            const ret = new Error();
            return addHeapObject(ret);
        },
        __wbg_stack_3b0d974bbf31e44f: function(arg0, arg1) {
            const ret = getObject(arg1).stack;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbindgen_object_drop_ref: function(arg0) {
            takeObject(arg0);
        },
    };
    return {
        __proto__: null,
        "./nrf52833_periph_wasm_bg.js": import0,
    };
}

const WasmCpuFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmcpu_free(ptr, 1));

function addHeapObject(obj) {
    if (heap_next === heap.length) heap.push(heap.length + 1);
    const idx = heap_next;
    heap_next = heap[idx];

    heap[idx] = obj;
    return idx;
}

function dropObject(idx) {
    if (idx < 1028) return;
    heap[idx] = heap_next;
    heap_next = idx;
}

function getArrayU16FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint16ArrayMemory0().subarray(ptr / 2, ptr / 2 + len);
}

function getArrayU32FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint32ArrayMemory0().subarray(ptr / 4, ptr / 4 + len);
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint16ArrayMemory0 = null;
function getUint16ArrayMemory0() {
    if (cachedUint16ArrayMemory0 === null || cachedUint16ArrayMemory0.byteLength === 0) {
        cachedUint16ArrayMemory0 = new Uint16Array(wasm.memory.buffer);
    }
    return cachedUint16ArrayMemory0;
}

let cachedUint32ArrayMemory0 = null;
function getUint32ArrayMemory0() {
    if (cachedUint32ArrayMemory0 === null || cachedUint32ArrayMemory0.byteLength === 0) {
        cachedUint32ArrayMemory0 = new Uint32Array(wasm.memory.buffer);
    }
    return cachedUint32ArrayMemory0;
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function getObject(idx) { return heap[idx]; }

let heap = new Array(1024).fill(undefined);
heap.push(undefined, null, true, false);

let heap_next = heap.length;

function isLikeNone(x) {
    return x === undefined || x === null;
}

function passArray16ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 2, 2) >>> 0;
    getUint16ArrayMemory0().set(arg, ptr / 2);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeObject(idx) {
    const ret = getObject(idx);
    dropObject(idx);
    return ret;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedUint16ArrayMemory0 = null;
    cachedUint32ArrayMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('nrf52833_periph_wasm_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
