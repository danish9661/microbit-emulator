// JavaScript MakeCode-arrow example: boot the real vendored MakeCode
// firmware (demo/firmware/mbcodal-arrow.hex, `basic.showArrow(North)`)
// through WasmCpu + matrix_state(), assert the North 9-LED glyph.
// Same fail-nonzero contract as gpio_js_example.mjs. The arrow program
// takes a post-render firmware-unwind fault (RAM-PC class, documented);
// the assertion only needs the pre-fault persistence-OR lit set.
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
const here = path.dirname(fileURLToPath(import.meta.url));
const pkgDir = path.join(here, "..", "..", "pkg");
const mod = await import(path.join(pkgDir, "nrf52833_periph_wasm.js"));
await mod.default({ module_or_path: readFileSync(path.join(pkgDir, "nrf52833_periph_wasm_bg.wasm")) });
const wasm = mod;
wasm.reset_state();
const { LSM303 } = await import("../lsm303.js");

function parseHexFile(fn) {
  const text = readFileSync(fn, "utf8");
  const img = new Uint8Array(512 * 1024).fill(0xff);
  const uicr = [];
  let base = 0;
  for (const line of text.split(/\r?\n/)) {
    if (!line.startsWith(":")) continue;
    const n = parseInt(line.slice(1, 3), 16), addr = parseInt(line.slice(3, 7), 16), type = parseInt(line.slice(7, 9), 16);
    if (type === 4) { base = parseInt(line.slice(9, 13), 16) << 16; continue; }
    if (type === 2) { base = parseInt(line.slice(9, 13), 16) << 4; continue; }
    if (type !== 0) continue;
    for (let i = 0; i < n; i++) {
      const a = base + addr + i, b = parseInt(line.slice(9 + i * 2, 11 + i * 2), 16);
      if (a >= 0x10001000 && a < 0x10002000) uicr.push([a, b]); else if (a < 512 * 1024) img[a] = b;
    }
  }
  return { img, uicr };
}

const { img, uicr } = parseHexFile(path.join(here, "..", "..", "firmware", "mbcodal-arrow.hex"));
const appSp = img[0x1c000] | (img[0x1c001] << 8) | (img[0x1c002] << 16) | (img[0x1c003] << 24);
const appPc = img[0x1c004] | (img[0x1c005] << 8) | (img[0x1c006] << 16) | (img[0x1c007] << 24);
const w32 = (v) => [v & 0xff, (v >> 8) & 0xff, (v >> 16) & 0xff, (v >> 24) & 0xff];
wasm.qspi_register_flash("QSPI", new Array(65536).fill(0xff));
const lsm = new LSM303(wasm, "TWIM1");
lsm.register(); wasm.init();
lsm.auto = false; lsm.accel = { x: 0, y: 0, z: 1000 }; lsm.mag = { x: 200, y: 0, z: 400 };
const cpu = new wasm.WasmCpu(0x20020000, 0x20000001, 512 * 1024, 128 * 1024);
cpu.load_firmware(img, 0);
cpu.set_deliver_irqs(true);
{
  const words = new Map();
  for (const [a, b] of uicr) { const w = a & ~3, i = a & 3; words.set(w, ((words.get(w) ?? 0xffffffff) & ~(0xff << (8 * i))) | (b << (8 * i))); }
  for (const [w, v] of words) wasm.periph_write(w, 4, v >>> 0);
}
cpu.mem_write(0x20000000, w32(0x1000));
cpu.mem_write(0x20000004, w32(0x1c000));
cpu.reset_cpu(appSp >>> 0, appPc >>> 0);
let t = 0;
const rn = Date.now; Date.now = () => Math.floor(t / 64000);
const pump = (n) => {
  cpu.step(n); wasm.tick_peripherals();
  if (wasm.is_watchdog_reset_requested()) { cpu.reset_cpu(appSp >>> 0, appPc >>> 0); return; }
  if (cpu.sleeping()) { wasm.tick_n(n); if (wasm.has_pending_interrupt()) cpu.wake(); }
  t += n; lsm.poll(cpu);
  try { const x = wasm.uarte_take_txdma(); if (x.length) wasm.uarte_complete_txdma([...cpu.mem_read(x[0], x[1])]); } catch (e) {}
  try { const s = wasm.saadc_take_result(); if (s.length) { cpu.mem_write(s[0], new Array(s[1] * 2).fill(0x80)); wasm.saadc_complete_result(s[1]); } } catch (e) {}
};
const log = [];
const check = (c, m) => { log.push(c); console.log((c ? "ok: " : "FAIL: ") + m); };
for (let w = 0; w < 130; w++) for (let i = 0; i < 150; i++) pump(2000);
const sticky = new Array(25).fill(0);
for (let k = 0; k < 25000; k++) {
  pump(400);
  const m = Array.from(wasm.matrix_state());
  for (let j = 0; j < 25; j++) sticky[j] |= m[j];
  if (cpu.fault_pc() !== 0xffffffff) break;
}
Date.now = rn;
const lit = sticky.map((v, j) => (v !== 0 ? j : "")).filter((x) => x !== "").join(",");
check(lit === "2,6,7,8,10,12,14,17,22", `js arrow North glyph lit (${lit || "dark"})`);
check(sticky.filter((v) => v !== 0).length === 9, "js arrow North is 9 LEDs");
if (log.some((c) => !c)) process.exit(1);
console.log("js MakeCode-arrow example OK (North via WasmCpu + matrix_state)");
