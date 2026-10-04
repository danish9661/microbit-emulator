// TypeScript MakeCode-arrow example: boot the real vendored MakeCode
// firmware (demo/firmware/mbcodal-arrow.hex, `basic.showArrow(North)`)
// through WasmCpu + matrix_state(), assert the North 9-LED glyph.
// Same contract as arrow_js_example.mjs in strict TS, no any.
// Run: node --experimental-strip-types parts/ble_lang/arrow_ts_example.mts
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
    mem_write(addr: number, data: number[]): void;
    mem_read(addr: number, len: number): Uint8Array;
    reset_cpu(sp: number, pc: number): void;
    sleeping(): boolean;
    wake(): void;
  };
  reset_state(): void;
  init(): void;
  tick_peripherals(): void;
  tick_n(n: number): void;
  has_pending_interrupt(): boolean;
  is_watchdog_reset_requested(): boolean;
  periph_write(addr: number, len: number, v: number): void;
  qspi_register_flash(name: string, data: number[]): void;
  uarte_take_txdma(): number[];
  uarte_complete_txdma(bytes: number[]): void;
  saadc_take_result(): number[];
  saadc_complete_result(n: number): void;
  matrix_state(): Uint8Array;
};
type Lsm = {
  register(): void;
  poll(cpu: unknown): void;
  auto: boolean;
  accel: { x: number; y: number; z: number };
  mag: { x: number; y: number; z: number };
};
const { LSM303 } = (await import("../lsm303.js")) as unknown as {
  LSM303: new (w: unknown, s: string) => Lsm;
};

function parseHexFile(fn: string): { img: Uint8Array; uicr: Array<[number, number]> } {
  const text: string = readFileSync(fn, "utf8");
  const img = new Uint8Array(512 * 1024).fill(0xff);
  const uicr: Array<[number, number]> = [];
  let base = 0;
  for (const line of text.split(/\r?\n/)) {
    if (!line.startsWith(":")) continue;
    const n: number = parseInt(line.slice(1, 3), 16);
    const addr: number = parseInt(line.slice(3, 7), 16);
    const type: number = parseInt(line.slice(7, 9), 16);
    if (type === 4) { base = parseInt(line.slice(9, 13), 16) << 16; continue; }
    if (type === 2) { base = parseInt(line.slice(9, 13), 16) << 4; continue; }
    if (type !== 0) continue;
    for (let i = 0; i < n; i++) {
      const a: number = base + addr + i;
      const b: number = parseInt(line.slice(9 + i * 2, 11 + i * 2), 16);
      if (a >= 0x10001000 && a < 0x10002000) uicr.push([a, b]);
      else if (a < 512 * 1024) img[a] = b;
    }
  }
  return { img, uicr };
}

const { img, uicr } = parseHexFile(path.join(here, "..", "..", "firmware", "mbcodal-arrow.hex"));
const appSp: number = img[0x1c000] | (img[0x1c001] << 8) | (img[0x1c002] << 16) | (img[0x1c003] << 24);
const appPc: number = img[0x1c004] | (img[0x1c005] << 8) | (img[0x1c006] << 16) | (img[0x1c007] << 24);
const w32 = (v: number): number[] => [v & 0xff, (v >> 8) & 0xff, (v >> 16) & 0xff, (v >> 24) & 0xff];
wasm.reset_state();
wasm.qspi_register_flash("QSPI", new Array<number>(65536).fill(0xff));
const lsm: Lsm = new LSM303(wasm, "TWIM1");
lsm.register();
wasm.init();
lsm.auto = false;
lsm.accel = { x: 0, y: 0, z: 1000 };
lsm.mag = { x: 200, y: 0, z: 400 };
const cpu = new wasm.WasmCpu(0x20020000, 0x20000001, 512 * 1024, 128 * 1024);
cpu.load_firmware(img, 0);
cpu.set_deliver_irqs(true);
{
  const words = new Map<number, number>();
  for (const [a, b] of uicr) {
    const w: number = a & ~3;
    const i: number = a & 3;
    words.set(w, ((words.get(w) ?? 0xffffffff) & ~(0xff << (8 * i))) | (b << (8 * i)));
  }
  for (const [w, v] of words) wasm.periph_write(w, 4, v >>> 0);
}
cpu.mem_write(0x20000000, w32(0x1000));
cpu.mem_write(0x20000004, w32(0x1c000));
cpu.reset_cpu(appSp >>> 0, appPc >>> 0);
let t = 0;
const rn = Date.now;
Date.now = (): number => Math.floor(t / 64000);
const pump = (n: number): void => {
  cpu.step(n);
  wasm.tick_peripherals();
  if (wasm.is_watchdog_reset_requested()) {
    cpu.reset_cpu(appSp >>> 0, appPc >>> 0);
    return;
  }
  if (cpu.sleeping()) {
    wasm.tick_n(n);
    if (wasm.has_pending_interrupt()) cpu.wake();
  }
  t += n;
  lsm.poll(cpu);
  try {
    const x: number[] = wasm.uarte_take_txdma();
    if (x.length) wasm.uarte_complete_txdma([...cpu.mem_read(x[0], x[1])]);
  } catch { /* no DMA pending */ }
  try {
    const s: number[] = wasm.saadc_take_result();
    if (s.length) {
      cpu.mem_write(s[0], new Array<number>(s[1] * 2).fill(0x80));
      wasm.saadc_complete_result(s[1]);
    }
  } catch { /* no result pending */ }
};
const log: boolean[] = [];
const check = (c: boolean, m: string): void => {
  log.push(c);
  console.log((c ? "ok: " : "FAIL: ") + m);
};
for (let w = 0; w < 130; w++) for (let i = 0; i < 150; i++) pump(2000);
const sticky: number[] = new Array<number>(25).fill(0);
for (let k = 0; k < 25000; k++) {
  pump(400);
  const m: number[] = [...wasm.matrix_state()];
  for (let j = 0; j < 25; j++) sticky[j] |= m[j];
  if (cpu.fault_pc() !== 0xffffffff) break;
}
Date.now = rn;
const lit: string = sticky.map((v: number, j: number) => (v !== 0 ? j : "")).filter((x) => x !== "").join(",");
check(lit === "2,6,7,8,10,12,14,17,22", `ts arrow North glyph lit (${lit || "dark"})`);
check(sticky.filter((v: number) => v !== 0).length === 9, "ts arrow North is 9 LEDs");
if (log.some((c: boolean) => !c)) process.exit(1);
console.log("ts MakeCode-arrow example OK (North via WasmCpu + matrix_state)");
