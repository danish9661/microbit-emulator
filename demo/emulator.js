// emulator.js — programmatic Node entry point for the microbit-emu package.
//
// Boots real BBC micro:bit v2 firmware (Intel HEX) on the shipped WASM
// core (nRF52833, Cortex-M4F + every peripheral) with virtual sensors
// attached, and exposes stepping plus the 5x5 LED matrix. ESM only
// ("type": "module").
//
//   import { bootEmulator } from 'microbit-emu';
//   const emu = await bootEmulator(hexText);
//   for (let i = 0; i < 200; i++) emu.step(20000);
//   console.log(emu.lit().join(',')); // lit LED indices, e.g. arrow North
//
// Firmware images ship under ./firmware/ (MakeCode arrows, MicroPython).
// No build step needed: ./pkg holds the prebuilt engine.
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const mod = await import("./pkg/nrf52833_periph_wasm.js");
await mod.default({
  module_or_path: readFileSync(path.join(here, "pkg", "nrf52833_periph_wasm_bg.wasm")),
});

/** The raw WASM core module (advanced use: registers, peripherals). */
export const wasm = mod;

const { LSM303 } = await import("./parts/lsm303.js");

/** Parse Intel HEX into a 512KB flash image + UICR byte list. */
export function parseHex(text) {
  const img = new Uint8Array(512 * 1024).fill(0xff);
  const uicr = [];
  let base = 0;
  for (const line of text.split(/\r?\n/)) {
    if (!line.startsWith(":")) continue;
    const n = parseInt(line.slice(1, 3), 16);
    const addr = parseInt(line.slice(3, 7), 16);
    const type = parseInt(line.slice(7, 9), 16);
    if (type === 4) { base = parseInt(line.slice(9, 13), 16) << 16; continue; }
    if (type === 2) { base = parseInt(line.slice(9, 13), 16) << 4; continue; }
    if (type !== 0) continue;
    for (let i = 0; i < n; i++) {
      const a = base + addr + i, b = parseInt(line.slice(9 + i * 2, 11 + i * 2), 16);
      if (a >= 0x10001000 && a < 0x10002000) uicr.push([a, b]);
      else if (a < 512 * 1024) img[a] = b;
    }
  }
  return { img, uicr };
}

/**
 * Boot a firmware HEX image (string) as a micro:bit v2 app at `appBase`
 * (default 0x1C000, MBR-param direct boot). Returns a live emulator.
 */
export function bootEmulator(hexText, { appBase = 0x1c000, accel = { x: 0, y: 0, z: 1000 }, mag = { x: 200, y: 0, z: 400 } } = {}) {
  const { img, uicr } = parseHex(hexText);
  const w32 = (v) => [v & 0xff, (v >> 8) & 0xff, (v >> 16) & 0xff, (v >> 24) & 0xff];
  wasm.reset_state();
  wasm.qspi_register_flash("QSPI", new Array(65536).fill(0xff));
  const lsm = new LSM303(wasm, "TWIM1");
  lsm.register();
  wasm.init();
  lsm.auto = false;
  lsm.accel = accel;
  lsm.mag = mag;
  const appSp = img[appBase] | (img[appBase + 1] << 8) | (img[appBase + 2] << 16) | (img[appBase + 3] << 24);
  const appPc = img[appBase + 4] | (img[appBase + 5] << 8) | (img[appBase + 6] << 16) | (img[appBase + 7] << 24);
  const cpu = new wasm.WasmCpu(0x20020000, 0x20000001, 512 * 1024, 128 * 1024);
  cpu.load_firmware(img, 0);
  cpu.set_deliver_irqs(true);
  {
    const words = new Map();
    for (const [a, b] of uicr) {
      const w = a & ~3, i = a & 3;
      words.set(w, ((words.get(w) ?? 0xffffffff) & ~(0xff << (8 * i))) | (b << (8 * i)));
    }
    for (const [w, v] of words) wasm.periph_write(w, 4, v >>> 0);
  }
  cpu.mem_write(0x20000000, w32(0x1000));
  cpu.mem_write(0x20000004, w32(0x1c000));
  cpu.reset_cpu(appSp >>> 0, appPc >>> 0);
  const pump = (n = 20000) => {
    cpu.step(n);
    wasm.tick_peripherals();
    if (wasm.is_watchdog_reset_requested()) cpu.reset_cpu(appSp >>> 0, appPc >>> 0);
    if (cpu.sleeping()) {
      wasm.tick_n(n);
      if (wasm.has_pending_interrupt()) cpu.wake();
    }
    lsm.poll(cpu);
    try {
      const x = wasm.uarte_take_txdma();
      if (x.length) wasm.uarte_complete_txdma([...cpu.mem_read(x[0], x[1])]);
    } catch { /* idle */ }
    try {
      const s = wasm.saadc_take_result();
      if (s.length) {
        cpu.mem_write(s[0], new Array(s[1] * 2).fill(0x80));
        wasm.saadc_complete_result(s[1]);
      }
    } catch { /* idle */ }
  };
  return {
    cpu,
    /** Advance emulation by n instructions + peripheral/time duties. */
    step: pump,
    /** Currently lit LED indices (0-24) from the model. */
    lit: () => {
      const m = Array.from(wasm.matrix_state());
      return m.map((v, j) => (v !== 0 ? j : "")).filter((x) => x !== "");
    },
    /** Fault PC, or null when healthy (decoder faults only). */
    fault: () => (cpu.fault_pc() === 0xffffffff ? null : cpu.fault_pc() >>> 0),
  };
}
