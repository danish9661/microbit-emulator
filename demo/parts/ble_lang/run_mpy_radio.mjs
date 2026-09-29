// MicroPython RADIO TX proof (headless Node vs the built pkg).
// Boots demo/firmware/micropython-microbit-v2.1.2.hex with the exact
// bench recipe (reset_state, QSPI+LSM303 register, init, 512KB image,
// UICR words, MBR params, direct-app reset to 0x1C000 vectors) and the
// exact bench pump (20K slices: reset-honor appBoot-style, sleep
// tick_n+wake, lsm.poll, TX take/complete, NVMC erase apply/complete,
// RXDRDY-gated drip + DMA mirror, TAKE-accumulated UART log), then
// drives the DOCUMENTED stock-hex radio API (`from radio import *`,
// `on()`, `send('ping')`) and asserts the firmware staged a real
// bare-metal RADIO TX job (take/complete). Exits nonzero on mismatch.
// Proven 2026-09-29: star-import + on() + send() all return cleanly,
// send() stages exactly one TX job (ptr 0x20005070, len 32), zero
// faults. RX leg (`receive()`) is NOT asserted: the MPY IRQ handler
// runs (ipsr==17 observed) and consumes the completion, but the packet
// never reaches the MPY RX queue on this pump — parked, see STATUS.
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const pkgDir = path.join(here, "..", "pkg-test-handshake");
const mod = await import(path.join(pkgDir, "nrf52833_periph_wasm.js"));
await mod.default({ module_or_path: readFileSync(path.join(pkgDir, "nrf52833_periph_wasm_bg.wasm")) });
const wasm = mod;
wasm.reset_state();
const { LSM303 } = await import("../lsm303.js");
const lsm = new LSM303(wasm, "TWIM1");
wasm.qspi_register_flash("QSPI", new Array(65536).fill(0xff));
lsm.register();
wasm.init();

function parseHex(text) {
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
      const a = base + addr + i;
      const b = parseInt(line.slice(9 + i * 2, 11 + i * 2), 16);
      if (a >= 0x10001000 && a < 0x10002000) uicr.push([a, b]);
      else if (a < 512 * 1024) img[a] = b;
    }
  }
  return { img, uicr };
}

const { img, uicr } = parseHex(
  readFileSync(path.join(here, "..", "..", "firmware", "micropython-microbit-v2.1.2.hex"), "utf8"),
);
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
const w32 = (v) => [v & 0xff, (v >> 8) & 0xff, (v >> 16) & 0xff, (v >> 24) & 0xff];
cpu.mem_write(0x20000000, w32(0x1000));
cpu.mem_write(0x20000004, w32(0x1c000));
cpu.reset_cpu(0x20020000, 0x29c51);

let LOG = "";
const uartOut = [];
let txJobs = 0;
const pump = () => {
  cpu.step(20000);
  wasm.tick_peripherals();
  if (wasm.is_watchdog_reset_requested()) cpu.reset_cpu(0x20020000, 0x29c51);
  if (cpu.sleeping()) {
    wasm.tick_n(20000);
    if (wasm.has_pending_interrupt()) cpu.wake();
  }
  lsm.poll(cpu);
  let t = wasm.uarte_take_txdma();
  if (t.length) wasm.uarte_complete_txdma([...cpu.mem_read(t[0], t[1])]);
  t = wasm.nvmc_take_erase();
  if (t.length) {
    if (t[0] === 0xffffffff) cpu.mem_write(0, new Uint8Array(512 * 1024).fill(0xff));
    else cpu.mem_write(t[0], new Uint8Array(4096).fill(0xff));
    wasm.nvmc_complete_erase();
  }
  try {
    const tx = wasm.radio_take_tx();
    if (tx.length) { txJobs++; wasm.radio_complete_tx(); }
  } catch (e) { /* RADIO idle: no staged job */ }
  // Drip one byte per pump, paced on RXDRDY-consumed (P134: RXD holds
  // ONE byte — unpaced drip overruns it and bytes are lost).
  if (uartOut.length && wasm.periph_read(0x40002108, 4) === 0) {
    const b = uartOut.shift();
    const ptr = wasm.periph_read(0x40002534, 4), amt = wasm.periph_read(0x4000253c, 4),
      max = wasm.periph_read(0x40002538, 4);
    if (ptr >= 0x20000000 && amt < max && amt < 4096) cpu.mem_write(ptr + amt, new Uint8Array([b]));
    wasm.uart_rx_byte(0x40002000, b);
  }
  LOG += wasm.get_uart_output();
};

const t0 = Date.now();
for (let i = 0; i < 25000 && !LOG.includes(">>>"); i++) pump();
if (!LOG.includes("MicroPython v1.18") || !LOG.includes(">>>")) {
  console.log("FAIL: mpy radio banner");
  process.exit(1);
}
const send = (s) => { for (const b of new TextEncoder().encode(s)) uartOut.push(b); };
const run = (s, n = 6000) => { const m = LOG.length; send(s); for (let i = 0; i < n; i++) pump(); return LOG.slice(m); };
const ok = [];
const check = (c, m) => { ok.push(!!c); console.log((c ? "ok: " : "FAIL: ") + m); };
let r = run("from radio import *\r");
check(r.endsWith(">>> "), "mpy radio star import");
r = run("on()\r");
check(r.trimEnd().endsWith(">>>"), "mpy radio on()");
const jobsBefore = txJobs;
r = run("send('ping')\r", 8000);
check(r.trimEnd().endsWith(">>>"), "mpy radio send('ping') returned");
check(txJobs > jobsBefore, `mpy radio send staged bare-metal TX (jobs ${jobsBefore}->${txJobs})`);
if (ok.some((c) => !c) || cpu.fault_pc() !== 0xffffffff) {
  if (cpu.fault_pc() !== 0xffffffff) console.log(`fault pc=${cpu.fault_pc().toString(16)}`);
  process.exit(1);
}
console.log(`mpy radio TX OK (star-import + on + send staged ${txJobs - jobsBefore} job(s), zero faults, ${((Date.now() - t0) / 1000).toFixed(1)}s)`);
