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

/**
 * @param {Uint8Array} bytes
 */
export function radio_inject_rx(bytes) {
    const ptr0 = passArray8ToWasm0(bytes, wasm.__wbindgen_export2);
    const len0 = WASM_VECTOR_LEN;
    wasm.radio_inject_rx(ptr0, len0);
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
