// TypeScript GPIO example: drive the 5x5 LED matrix from TS idioms.
// Same contract as gpio_js_example.mjs (MATRIX:OK, fault-free, pin-level
// render == matrix_state, strobe coverage) in strict TS, no any.
// Run: node --experimental-strip-types parts/ble_lang/gpio_ts_example.mts
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here: string = path.dirname(fileURLToPath(import.meta.url));
const pkgDir: string = path.join(here, "..", "pkg-test-handshake");

type WasmMod = typeof import("../pkg-test-handshake/nrf52833_periph_wasm.js");
const mod = (await import(path.join(pkgDir, "nrf52833_periph_wasm.js"))) as WasmMod;
await mod.default({
  module_or_path: readFileSync(path.join(pkgDir, "nrf52833_periph_wasm_bg.wasm")),
});
const wasm = mod as unknown as {
  WasmCpu: new (sp: number, pc: number, flash: number, ram: number) => {
    load_firmware(b: Uint8Array, base: number): void;
    set_deliver_irqs(v: boolean): void;
    step(n: number): number;
    fault_pc(): number;
  };
  reset_state(): void;
  init(): void;
  tick_peripherals(): void;
  get_uart_output(): string;
  gpio_read_dir(port: number, pin: number): boolean;
  gpio_read_output(port: number, pin: number): boolean;
  matrix_state(): Uint8Array;
};

wasm.reset_state();
wasm.init();
const img: Uint8Array = new Uint8Array(
  readFileSync(path.join(here, "..", "..", "..", "blinky", "matrix_nrf.bin")),
);
const dv: DataView = new DataView(img.buffer, img.byteOffset, img.byteLength);
const cpu = new wasm.WasmCpu(dv.getUint32(0, true), dv.getUint32(4, true), 512 * 1024, 128 * 1024);
cpu.load_firmware(img, 0);
cpu.set_deliver_irqs(true);
const log: boolean[] = [];
const check = (c: boolean, m: string): void => {
  log.push(c);
  console.log((c ? "ok: " : "FAIL: ") + m);
};
let marker = "";
for (let i = 0; i < 200 && !marker.includes("MATRIX:OK"); i++) {
  cpu.step(20000);
  wasm.tick_peripherals();
  marker += wasm.get_uart_output();
}
check(marker.includes("MATRIX:OK"), "ts matrix firmware prints MATRIX:OK");
check(cpu.fault_pc() === 0xffffffff, "ts matrix runs fault-free");
type Pin = [number, number];
const ROWS: Pin[] = [[0, 21], [0, 22], [0, 15], [0, 24], [0, 19]];
const COLS: Pin[] = [[0, 28], [0, 11], [0, 31], [1, 5], [0, 30]];
const lit = (r: number, c: number): boolean =>
  wasm.gpio_read_dir(...ROWS[r]) && !wasm.gpio_read_output(...ROWS[r]) &&
  wasm.gpio_read_dir(...COLS[c]) && wasm.gpio_read_output(...COLS[c]);
let tsLit = 0;
for (let r = 0; r < 5; r++) for (let c = 0; c < 5; c++) if (lit(r, c)) tsLit++;
const px: number[] = [...wasm.matrix_state()];
const modelLit: number = px.reduce((a: number, b: number) => a + b, 0);
check(px.length === 25, "ts matrix_state() returns 25 pixels");
check(modelLit > 0, `ts slab lit now (${modelLit}/25, strobe phase)`);
check(tsLit === modelLit, `ts pin-level render matches matrix_state (${tsLit}==${modelLit})`);
const seen: number[] = new Array<number>(25).fill(0);
for (let i = 0; i < 40; i++) {
  cpu.step(20000);
  wasm.tick_peripherals();
  const p: number[] = [...wasm.matrix_state()];
  for (let k = 0; k < 25; k++) seen[k] += p[k];
}
check(seen.filter((v: number) => v > 0).length >= 10, `ts strobe covers the glyph (${seen.filter((v: number) => v > 0).length}/25 ever lit)`);
if (log.some((c: boolean) => !c)) process.exit(1);
console.log("ts GPIO example OK (matrix via WasmCpu + matrix_state)");
