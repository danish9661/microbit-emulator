# MicroPython-language BLE face test (runs on CPython AND MicroPython).
# The shipped micro:bit MPY hex has no bluetooth module (verified: no
# 'bluetooth' strings in flash, MICROBIT_BLE_ENABLED=0), so no on-device
# MPY BLE test is possible today. This file proves the *contract* MPY
# would call — same SVC numbers/structs the MPY C port would emit —
# using only MicroPython-compatible idioms (no imports beyond struct).
# A BLE-enabled MPY build would replace the FakeWasm with machine.mem
# writes + inline SVC; the byte layouts asserted here are identical.
try:
    import struct
except ImportError:  # pragma: no cover
    struct = None

SVC_ENABLE = 0x60
SVC_EVT_GET = 0x61
SVC_CONNECT = 0x8C
SVC_READ = 0x96
EVT_CONNECTED = 0x10
EVT_READ_RSP = 0x36
ROLE_CENTRAL = 2


def check(cond, name, log):
    log.append((bool(cond), name))
    print(("ok: " if cond else "FAIL: ") + name)


def le16(b, off):
    return b[off] | (b[off + 1] << 8)


def run_face(wasm, cpu, log):
    # ENABLE via real SVC byte (works on CPython-driven wasm AND would
    # work from MPY inline asm against the same emulator).
    def svc(num, r0=0, r1=0, r2=0):
        cpu.reset_cpu(0x20020000, 0x20000001)
        cpu.set_deliver_irqs(True)
        cpu.mem_write(0x20000000, bytes([0x08, 0x48, 0x09, 0x49,
                                         0x09, 0x4A, num & 0xFF, 0xDF,
                                         0xFE, 0xE7]))
        import struct as st
        cpu.mem_write(0x20000024, st.pack("<IIII", r0, r1, r2, 0))
        cpu.step(8)
        assert cpu.fault_pc() == 0xFFFFFFFF, "svc fault"
        return cpu.get_regs()[0]
    check(svc(SVC_ENABLE, 0, 0) == 0, "mpy enable", log)
    # CONNECT -> loopback resolve -> CONNECTED envelope shape.
    cpu.mem_write(0x20001200, bytes([1, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66]))
    check(svc(SVC_CONNECT, 0x20001200, 0) == 0, "mpy connect stages", log)
    job = bytes(wasm.ble_take_job()).decode("latin1") if False else wasm.ble_take_job()
    check(len(job) > 0 and job[0] == 1, "mpy take GapConnect", log)
    wasm.ble_complete_gap_connect(bytes(bytearray([0x11, 0x22, 0x33, 0x44, 0x55, 0x66])))
    cpu.mem_write(0x20003FF0, bytes([128, 0]))
    check(svc(SVC_EVT_GET, 0x20003000, 0x20003FF0) == 0, "mpy evt_get", log)
    evt = bytes(cpu.mem_read(0x20003000, 32))
    check(le16(evt, 0) == EVT_CONNECTED, "mpy CONNECTED id", log)
    check(evt[4 + 16] == ROLE_CENTRAL, "mpy CENTRAL role", log)
    # READ -> loopback battery -> READ_RSP tail byte.
    check(svc(SVC_READ, 1, 0x13, 0) == 0, "mpy read stages", log)
    wasm.ble_complete_gattc_read(1, 0x13, 0, bytes(bytearray([87])))
    cpu.mem_write(0x20003FF0, bytes([128, 0]))
    check(svc(SVC_EVT_GET, 0x20003000, 0x20003FF0) == 0, "mpy evt_get read", log)
    evt = bytes(cpu.mem_read(0x20003000, 17))
    check(le16(evt, 0) == EVT_READ_RSP and evt[-1] == 87, "mpy READ_RSP=87", log)
    return log
