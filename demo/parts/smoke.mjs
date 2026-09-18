// Headless smoke test for virtual parts (no browser needed):
//   node smoke.mjs
// Uses a mock wasm core + fake CPU RAM to drive the same poll() path
// the demo page uses every frame. Fails loudly (nonzero exit) on mismatch.
import { LSM303 } from './lsm303.js';
import { SSD1306 } from './ssd1306.js';
import { EDGE, ROWS, COLS, INTERNAL } from './pins.js';
import { Kl27Uipm, Kl27Flash, SpeakerPart, MicPart, LogoTouchPart, BOARD_PINS } from './kl27.js';

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
  // KL27 UIPM answers valid protocol frames (READ_RSP + prop + payload):
  // BOARD_REV 0x9904 (V2.00 KL27), I2C v2 (BUSY_FLAG_SUPPORTED),
  // unknown prop -> ERR_RSP/UNKNOWN (never a bare empty frame — zeros
  // would read as "no event" and burn the 20x20 retry budget).
  check(imu.sample(0x70, 0x01, 4).join() === '17,1,153,4', 'KL27 UIPM BOARD_REV 0x9904');
  check(imu.sample(0x70, 0x02, 4).join() === '17,2,0,2', 'KL27 UIPM I2C v2');
  check(imu.sample(0x70, 0xFF, 2).join() === '32,50', 'KL27 UIPM unknown -> ERR/UNKNOWN');
  // USB-FLASH geometry/config per the wire contract (BE16/BE32).
  check(imu.sample(0x39, 0x07, 3).join() === '7,16,0', 'USB-FLASH SECTOR_SIZE 4096');
  check(imu.sample(0x39, 0x06, 2).join() === '6,31', 'USB-FLASH DISK_SIZE 31');
  // USB-FLASH answers VALID frames: VISIBILITY echoes [cmd, visible]
  // (P85 legacy liveness: valid bytes exit _transact on the first RX
  // attempt instead of burning the 20x20 retry budget).
  check(imu.sample(0x39, 0x03, 2).join() === '3,1', 'USB-FLASH visibility frame');
  check(imu.sample(0x39, 0x01, 12).length === 12 && imu.sample(0x39, 0x01, 12)[0] === 1, 'USB-FLASH filename echo parses');
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

// --- KL27 UIPM protocol engine: valid frames per CODAL contract ---
{
  const u = new Kl27Uipm();
  check(u.version().board === 0x9904 && u.version().i2c === 2, 'UIPM version: board 0x9904, i2c v2');
  check(u.byteResponse([0x10, 0x01]).join() === '17,1,153,4', 'UIPM BOARD_REV READ_RSP');
  check(u.byteResponse([0x10, 0x06]).join() === '17,6,4', 'UIPM USB_STATE configured');
  check(u.byteResponse([0x10, 0xFF]).join() === '32,50', 'UIPM unknown -> ERR/UNKNOWN');
  u.request([0x12, 0x08, 1, 0]); // LED_STATE <- off
  check(u.response(2).join() === '19,0', 'UIPM WRITE_RSP');
  check(u.ledState === 0, 'UIPM LED_STATE applied');
  u.request([0, 0, 0]); // e8777 NOP wake: recorded, harmless
  check(u.response(0).length === 0, 'UIPM NOP wake ignored');
  u.queueUserEvent(0x03); // long-press
  check(u.byteResponse([0x10, 0x09]).join() === '17,9,1,3', 'UIPM USER_EVENT queued+read');
}

// --- KL27 USB-FLASH protocol engine: config/geometry/storage ---
{
  const f = new Kl27Flash();
  check(f.geometry().blockSize === 4096 && f.geometry().blockCount === 31, 'FLASH geometry 4096x31');
  check(f.byteResponse([0x01]).join().split(',').slice(0, 2).join() === '1,68', 'FLASH FILENAME echo+body');
  check(f.byteResponse([0x02]).join() === '2,0,1,240,0', 'FLASH FILESIZE 0x1F000 BE32');
  check(f.byteResponse([0x07]).join() === '7,16,0', 'FLASH SECTOR_SIZE 4096 BE16');
  check(f.byteResponse([0x09]).join() === '9,0', 'FLASH STATUS never-busy');
  // WRITE then READ round trip through request/response.
  // Wire shape (htonl on silicon): word0 = addr24 | (cmd<<24), word1 =
  // byte count; payload follows. So WRITE @0x1000 len 4 =
  // [0x0B,0,0x10,0, 0,0,0,4, DE,AD,BE,EF].
  f.request([0x0B, 0, 0x10, 0x00, 0, 0, 0, 4, 0xDE, 0xAD, 0xBE, 0xEF]);
  check(f.response(9).join() === '11,0,0,0,0,0,0,0,0', 'FLASH WRITE ack');
  f.request([0x0A, 0, 0x10, 0x00, 0, 0, 0, 4]);
  const rd = f.response(12);
  check(rd.slice(0, 8).join() === '10,0,16,0,0,0,0,4' && rd.slice(8).join() === '222,173,190,239', `FLASH READ echo+data (${rd})`);
  // ERASE restores 0xFF.
  f.request([0x0C, 0, 0x10, 0x00, 0, 0, 0, 1]);
  check(f.response(1).join() === '12', 'FLASH ERASE ack');
  check(f.readStorage(0x1000, 4).join() === '255,255,255,255', 'FLASH ERASE -> 0xFF');
  // Unknown command => empty (transact failure path, not a hang).
  f.request([0x7F]);
  check(f.response(4).join() === '0,0,0,0', 'FLASH unknown pads zero');
}

// --- Speaker/mic/logo board parts: pins + behavior, no TWIM takes ---
{
  const rec = { outs: [], ins: [] };
  const wasm = {
    ...mockWasm(),
    gpio_read_output: (p, n) => rec.outs.find((x) => x[0] === p && x[1] === n)?.[2] ?? false,
    gpio_set_input: (p, n, v) => rec.ins.push([p, n, v]),
    periph_read: () => 0,
  };
  check(BOARD_PINS.SPEAKER.join() === '0,0', 'SPEAKER = P0.00');
  check(BOARD_PINS.LOGO.join() === '1,4', 'LOGO = P1.04');
  check(INTERNAL.LOGO_TOUCH.join() === '1,4', 'pins.js LOGO_TOUCH = P1.04');
  const spk = new SpeakerPart(wasm);
  rec.outs = [[0, 0, false]];
  spk.poll(); spk.poll();
  rec.outs = [[0, 0, true]];
  spk.poll();
  check(spk.activity().toggles === 1, `speaker counts P0.00 edges (${spk.activity().toggles})`);
  spk.setSpeakerEnabled(false);
  check(spk.activity().muted === true, 'speaker mute is host-side flag');
  // headless mock pretends RUN_MIC powered (no gpio model): sine+noise.
  // NOTE: mockWasm() has no gpio_read_output, so MicPart is built on a
  // bare stub exposing only level (no wasm handle at all).
  const mic = new MicPart(null, { level: 0 });
  check([...mic.nextSamples(4)].join() === '0,0,0,0,0,0,0,0', 'mic level 0 / unpowered = silence');
  mic.setLevel(255);
  const s = mic.nextSamples(16);
  check(s.length === 32 && s.some((b) => b !== 0), 'mic level 255 = audible samples');
  const logo = new LogoTouchPart(wasm);
  logo.press();
  check(logo.isPressed() && rec.ins.some(([p, n, v]) => p === 1 && n === 4 && v === false), 'logo press drives P1.04 low');
  logo.release();
  check(!logo.isPressed() && rec.ins.some(([p, n, v]) => p === 1 && n === 4 && v === true), 'logo release drives P1.04 high');
}
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
