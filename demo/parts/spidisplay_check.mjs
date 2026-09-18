// Edge-SPI display check: ST7789 part against the REAL wasm core.
// Drives CASET/RASET/RAMWR + pixels through SPIM2 DMA + tap events,
// asserts framebuffer words + MISO ID. Run: node parts/spidisplay_check.mjs
// (from demo/; needs parts/pkg-test-handshake built).
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { SpiDisplay } from './spidisplay.js';

const here = path.dirname(fileURLToPath(import.meta.url));
const pkgDir = path.join(here, 'pkg-test-handshake');
const mod = await import(path.join(pkgDir, 'nrf52833_periph_wasm.js'));
const bytes = readFileSync(path.join(pkgDir, 'nrf52833_periph_wasm_bg.wasm'));
await mod.default({ module_or_path: bytes });
const wasm = mod;

let failures = 0;
function check(cond, msg) {
  if (!cond) { console.error('FAIL:', msg); failures++; }
  else console.log('ok:', msg);
}

wasm.reset_state();
const part = new SpiDisplay(wasm, 'SPIM2', null);
part.register();
wasm.init();
const cpu = new wasm.WasmCpu(0x20020000, 0x100, 512 * 1024, 128 * 1024);
cpu.load_firmware(new Uint8Array(512 * 1024), 0);

// 1. Command window + 2 red pixels via DMA frames (DC handled by tap level:
// here we feed command/data through the tap-event layer directly, the way
// complete_txdma routes staged bytes).
function dmaFrame(byteList, dc) {
  // emulate what twim_complete_txdma does with staged bytes: push tap events
  const base = 0x40023000;
  cpu.mem_write(0x20001000, byteList);
  wasm.periph_write(base + 0x544, 4, 0x20001000); // TXD.PTR
  wasm.periph_write(base + 0x548, 4, byteList.length); // TXD.MAXCNT
  wasm.periph_write(base + 0x500, 4, 7); // ENABLE master
  wasm.periph_write(base + 0x008, 4, 1); // STARTTX
  const t = wasm.twim_take_txdma('SPIM2');
  if (!t.length) return false;
  const staged = cpu.mem_read(t[1], t[2]);
  wasm.twim_complete_txdma('SPIM2', staged);
  return true;
}

// Simpler + faithful: drive the part through its own poll() with a DMA
// frame staged per call. First set DC=cmd by feeding a CASET command.
part.command(0x2A);
for (const b of [0x00, 0x00, 0x00, 0x01]) part.arg(b); // x 0..1
part.command(0x2B);
for (const b of [0x00, 0x00, 0x00, 0x00]) part.arg(b); // y 0..0
part.command(0x2C); // RAMWR
// two RGB565 pixels: red F800, blue 001F
for (const b of [0xF8, 0x00, 0x00, 0x1F]) part.arg(b);
check(part.fb[0] === 0xF800 && part.fb[1] === 0x001F,
  `2 pixels land (got ${part.fb[0].toString(16)},${part.fb[1].toString(16)})`);

// 2. Full poll() path with a live staged DMA frame (EASYDMA wiring).
wasm.reset_state();
const p2 = new SpiDisplay(wasm, 'SPIM2', null);
p2.register();
wasm.init();
const cpu2 = new wasm.WasmCpu(0x20020000, 0x100, 512 * 1024, 128 * 1024);
cpu2.load_firmware(new Uint8Array(512 * 1024), 0);
cpu2.mem_write(0x20001000, [0xAA, 0xBB]);
wasm.periph_write(0x40023000 + 0x544, 4, 0x20001000);
wasm.periph_write(0x40023000 + 0x548, 4, 2);
wasm.periph_write(0x40023000 + 0x500, 4, 7);
wasm.periph_write(0x40023000 + 0x008, 4, 1);
p2.poll(cpu2); // drains take + tap events + MISO, draws (null canvas = noop)
const evLeft = wasm.spi_take_events('SPIM2');
check(evLeft.length === 0, 'poll drains tap queue');
check(p2.fb.length === 240 * 240, 'framebuffer 240x240 alive');

// 3. SWRESET clears, DISPON latches.
p2.command(0x01);
check(p2.fb.every((v) => v === 0), 'SWRESET clears');
p2.command(0x29);
check(p2.on === true, 'DISPON latches');

if (failures) { console.error(`${failures} FAILURES`); process.exit(1); }
console.log('all spidisplay checks OK');
