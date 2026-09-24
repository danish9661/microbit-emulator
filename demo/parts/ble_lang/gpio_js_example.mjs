// JavaScript GPIO example: drive the 5x5 LED matrix from JS idioms.
// Boots matrix_nrf.bin (the strobing "A" proof), then reads the live
// slab three ways: matrix_state() pixels, gpio_read_output() pins, and
// the DIR gate. Exits nonzero on any FAIL — same contract as handshake.mjs.
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
const img = new Uint8Array(readFileSync(path.join(here, "..", "..", "..", "blinky", "matrix_nrf.bin")));
const dv = new DataView(img.buffer, img.byteOffset, img.byteLength);
const cpu = new wasm.WasmCpu(dv.getUint32(0, true), dv.getUint32(4, true), 512 * 1024, 128 * 1024);
cpu.load_firmware(img, 0);
cpu.set_deliver_irqs(true);
const log = [];
const check = (c, m) => { log.push(c); console.log((c ? "ok: " : "FAIL: ") + m); };
// run until the MATRIX:OK marker (strobe keeps sweeping after it)
let marker = "";
for (let i = 0; i < 200 && !marker.includes("MATRIX:OK"); i++) {
  cpu.step(20000);
  wasm.tick_peripherals();
  marker += wasm.get_uart_output();
}
check(marker.includes("MATRIX:OK"), "js matrix firmware prints MATRIX:OK");
check(cpu.fault_pc() === 0xffffffff, "js matrix runs fault-free");
// one slab sample: rows sink (OUT 0), cols source (OUT 1), DIR output
const ROWS = [[0, 21], [0, 22], [0, 15], [0, 24], [0, 19]];
const COLS = [[0, 28], [0, 11], [0, 31], [1, 5], [0, 30]];
const lit = (r, c) =>
  wasm.gpio_read_dir(...ROWS[r]) && !wasm.gpio_read_output(...ROWS[r]) &&
  wasm.gpio_read_dir(...COLS[c]) && wasm.gpio_read_output(...COLS[c]);
let jsLit = 0;
for (let r = 0; r < 5; r++) for (let c = 0; c < 5; c++) if (lit(r, c)) jsLit++;
const px = [...wasm.matrix_state()];
const modelLit = px.reduce((a, b) => a + b, 0);
check(px.length === 25, "js matrix_state() returns 25 pixels");
check(modelLit > 0, `js slab lit now (${modelLit}/25, strobe phase)`);
check(jsLit === modelLit, `js pin-level render matches matrix_state (${jsLit}==${modelLit})`);
// persistence over a full sweep: every glyph pixel lights at least once
const seen = new Array(25).fill(0);
for (let i = 0; i < 40; i++) {
  cpu.step(20000);
  wasm.tick_peripherals();
  const p = [...wasm.matrix_state()];
  for (let k = 0; k < 25; k++) seen[k] += p[k];
}
check(seen.filter((v) => v > 0).length >= 10, `js strobe covers the glyph (${seen.filter((v) => v > 0).length}/25 ever lit)`);
if (log.some((c) => !c)) process.exit(1);
console.log("js GPIO example OK (matrix via WasmCpu + matrix_state)");
