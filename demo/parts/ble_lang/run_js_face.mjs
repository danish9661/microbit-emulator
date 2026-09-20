// JavaScript-language BLE face test (same contract as the C/C++/MPY/TS
// runners: ENABLE -> CONNECT -> CONNECTED(CENTRAL) -> READ -> READ_RSP=87).
// Exits nonzero on any FAIL — same contract as handshake.mjs.
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
const here = path.dirname(fileURLToPath(import.meta.url));
const pkgDir = path.join(here, "..", "pkg-test-handshake");
const mod = await import(path.join(pkgDir, "nrf52833_periph_wasm.js"));
await mod.default({ module_or_path: readFileSync(path.join(pkgDir, "nrf52833_periph_wasm_bg.wasm")) });
const wasm = mod;
wasm.reset_state();
wasm.init();
const cpu = new wasm.WasmCpu(0x20020000, 0x20000001, 512 * 1024, 128 * 1024);
const log = [];
function svc(num, r0 = 0, r1 = 0, r2 = 0) {
  cpu.reset_cpu(0x20020000, 0x20000001);
  cpu.set_deliver_irqs(true);
  cpu.mem_write(0x20000000, [0x08, 0x48, 0x09, 0x49, 0x09, 0x4A, num & 0xff, 0xdf, 0xfe, 0xe7]);
  const b = new Uint8Array(new Uint32Array([r0, r1, r2, 0]).buffer);
  cpu.mem_write(0x20000024, [...b]);
  cpu.step(8);
  if (cpu.fault_pc() !== 0xffffffff) throw new Error("svc fault");
  return cpu.get_regs()[0] >>> 0;
}
const le16 = (a, o) => a[o] | (a[o + 1] << 8);
const check = (c, m) => { log.push(c); console.log((c ? "ok: " : "FAIL: ") + m); };
check(svc(0x60, 0, 0) === 0, "js enable");
cpu.mem_write(0x20001200, [1, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
check(svc(0x8c, 0x20001200, 0) === 0, "js connect stages");
const job = wasm.ble_take_job();
check(job.length > 0 && job[0] === 1, "js take GapConnect");
wasm.ble_complete_gap_connect([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
cpu.mem_write(0x20003ff0, [128, 0]);
check(svc(0x61, 0x20003000, 0x20003ff0) === 0, "js evt_get");
const evt = [...cpu.mem_read(0x20003000, 32)];
check(le16(evt, 0) === 0x10, "js CONNECTED id");
check(evt[4 + 16] === 2, "js CENTRAL role");
check(svc(0x96, 1, 0x13, 0) === 0, "js read stages");
wasm.ble_complete_gattc_read(1, 0x13, 0, [87]);
cpu.mem_write(0x20003ff0, [128, 0]);
check(svc(0x61, 0x20003000, 0x20003ff0) === 0, "js evt_get read");
const rr = [...cpu.mem_read(0x20003000, 17)];
check(le16(rr, 0) === 0x36 && rr[rr.length - 1] === 87, "js READ_RSP=87");
if (log.some((c) => !c)) process.exit(1);
console.log("js BLE face OK (JavaScript-idiom contract)");
