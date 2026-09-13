// Headless smoke test for virtual parts (no browser needed):
//   node smoke.mjs
// Uses a mock wasm core + fake CPU RAM to drive the same poll() path
// the demo page uses every frame. Fails loudly (nonzero exit) on mismatch.
import { LSM303 } from './lsm303.js';
import { SSD1306 } from './ssd1306.js';
import { EDGE, ROWS, COLS } from './pins.js';

let failures = 0;
function check(cond, msg) {
  if (!cond) { console.error('FAIL:', msg); failures++; }
  else console.log('ok:', msg);
}

function mockWasm() {
  const rx = [];
  return {
    slaves: [],
    inputs: [],
    gpio_set_input(port, pin, v) { this.inputs.push([port, pin, v]); },
    i2c_register_slave(p, a) { this.slaves.push([p, a]); },
    i2c_take_events(p) { return []; },
    i2c_push_rx(p, bytes) { rx.push(...bytes); },
    _rx: rx,
    _txStaged: null, _rxStaged: null,
    twim_take_txdma(p) { return this._txStaged ?? []; },
    twim_complete_txdma(p, b) { this._txDone = b; },
    twim_take_rxdma(p) { return this._rxStaged ?? []; },
    twim_complete_rxdma(p, n) { this._rxDone = n; },
  };
}
function mockCpu() {
  const ram = new Uint8Array(256);
  return {
    mem_read: (ptr, len) => ram.slice(ptr, ptr + len),
    mem_write: (ptr, bytes) => ram.set(bytes, ptr),
    _ram: ram,
  };
}

// --- pins.js ---
check(EDGE.P0.join() === '0,2', 'P0 = P0.02');
check(EDGE.P20.join() === '1,0', 'P20 = P1.00');
check(ROWS.length === 5 && COLS.length === 5, '5x5 matrix map');

// --- LSM303: WHO_AM_I + CTRL echo + live tilt via DMA path ---
{
  const wasm = mockWasm(), cpu = mockCpu(), imu = new LSM303(wasm);
  imu.register();
  check(wasm.slaves.join() === 'TWIM1,25,TWIM1,30,TWIM1,112,TWIM1,57', 'registers 0x19 + 0x1E + KL27 UIPM 0x70 + USB-FLASH 0x39');
  // TX: set pointer 0x0F, then read it back through sample()
  imu.feedWrite(0x19, [0x0F]);
  check(imu.sample(0x19, 0x0F, 1)[0] === 0x33, 'accel WHO_AM_I');
  check(imu.sample(0x1E, 0x0F, 1)[0] === 0x40, 'mag WHO_AM_I');
  check(imu.sample(0x70, 0x00, 4).join() === '0,0,0,0', 'KL27 UIPM empty frame');
  check(imu.sample(0x39, 0x00, 2).join() === '32,1', 'USB-FLASH fail-fast frame');
  imu.feedWrite(0x19, [0x20, 0x57]); // CTRL_REG1 write
  check(imu.sample(0x19, 0x20, 1)[0] === 0x57, 'CTRL echo');
  // tilt changes output
  imu.setAccel({ x: 1000, y: -1000, z: 0 });
  const d = imu.sample(0x19, 0x28, 6);
  check(d[0] === 0xE8 && d[1] === 0x03, `X=+1000 LE (${d.slice(0, 2)})`);
  check(d[2] === 0x18 && d[3] === 0xFC, `Y=-1000 LE (${d.slice(2, 4)})`);
  // DMA path through poll()
  wasm._txStaged = [0x19, 0, 1];
  cpu._ram.set([0x0F], 0);
  imu.poll(cpu);
  check((wasm._rxStaged ?? null) === null, 'no rx staged spontaneously');
  // DRDY pulse (60ms low / 140ms high): poll until both phases seen.
  for (let i = 0; i < 40 && !(wasm.inputs.some(([p, n, v]) => p === 0 && n === 25 && v === false) && wasm.inputs.some(([p, n, v]) => p === 0 && n === 25 && v === true)); i++) {
    imu.poll(cpu);
    await new Promise((r) => setTimeout(r, 10));
  }
  check(wasm.inputs.some(([p, n, v]) => p === 0 && n === 25 && v === false), 'DRDY pulses P0.25 low');
  check(wasm.inputs.some(([p, n, v]) => p === 0 && n === 25 && v === true), 'DRDY releases P0.25 high');
}

// --- LSM303 byte path: pointer-set anticipates the read ---
{
  const wasm = mockWasm(), cpu = mockCpu(), imu = new LSM303(wasm);
  imu.bytePathAddr = null;
  // simulate tap events: START(0x19) 0x0F STOP
  wasm.i2c_take_events = () => [0x80000000 | 0x40000000 | 0x19, 0x0F, 0x80000000];
  imu.poll(cpu);
  check(wasm._rx[0] === 0x33 && wasm._rx.length === 8, `WHO_AM_I pushed to rx queue (${wasm._rx})`);
}

// --- SSD1306: init + fill + framebuffer ---
{
  const wasm = mockWasm(), cpu = mockCpu(), oled = new SSD1306(wasm, 'TWIM0', null);
  oled.register();
  check(wasm.slaves.join() === 'TWIM0,60', 'registers 0x3C');
  // display ON + full-frame write via byte path
  oled.feed([0x00, 0xAF]); // command: on
  check(oled.on === true, 'display on');
  oled.feed([0x00, 0x21, 0, 127, 0x22, 0, 7]); // col/page windows
  const row = new Array(128).fill(0xFF);
  oled.feed([0x40, ...row]); // data: full row of pixels, page 0
  check(oled.fb[0] === 0xFF && oled.fb[127] === 0xFF, 'framebuffer row written');
  check(oled.fb[128] === 0, 'next page untouched');
  // DMA path with address framing
  wasm._txStaged = [0x3C, 0, 3];
  cpu._ram.set([0x00, 0xAE], 0);
  oled.poll(cpu);
  check(oled.on === false, 'DMA command path works');
}

if (failures) { console.error(`${failures} FAILURES`); process.exit(1); }
console.log('all parts smoke OK');
