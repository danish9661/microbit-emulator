// KL27 interface MCU + v2 board sound/touch parts (micro:bit v2).
//
// Ground truth: lancaster-university/codal-microbit-v2
//   MicroBitPowerManager.h/.cpp (UIPM protocol, 0x70), MicroBitUSBFlashManager.h/.cpp
//   (USB-FLASH protocol, 0x72<<1 wire = 0x39 7-bit), MicroBitIO.h/.cpp
//   (logo P1_04, speaker P0_00, runmic P0_20, microphone P0_05),
// plus the power-management + I2C protocol specs (microbit-foundation):
//   - UIPM: READ_REQ 0x10 / READ_RSP 0x11 / WRITE_REQ 0x12 / WRITE_RSP 0x13 /
//     ERR_RSP 0x20; props BOARD_REV/I2C_VER/DAPLINK_VER/POWER_SRC/POWER_CONS/
//     USB_STATE/KL27_MODE/LED_STATE/USER_EVENT; BUSY 0x39 / INCOMPLETE 0x31.
//   - FLASH transact: request [addr|cmd BE32, len BE32] + response; geometry
//     blockSize/blockCount; BUSY-flag vs zero/invalid-echo inference.
//   - KL27 wake errata e8777: first I2C message after sleep is lost — the
//     Target MCU leads every transaction with a NOP (we answer, harmless).
//   - COMBINED_SENSOR_INT (irq1/P0.25) threshold: 30 consecutive active
//     ticks before MicroBitPowerManager::idleCallback services KL27.
//
// Wiring contract (same as every part): the TWIM1 EASYDMA take/complete
// pair has ONE owner per bus. LSM303 owns TWIM1 takes and delegates
// 0x70/0x39 traffic here (see lsm303.js); these classes are the protocol
// engines (request/response/sample) plus GPIO/PDM/PWM board parts that
// touch no TWIM takes and are race-free in the parts[] poll loop.

const UIPM = {
  READ_REQ: 0x10, READ_RSP: 0x11, WRITE_REQ: 0x12, WRITE_RSP: 0x13, ERR_RSP: 0x20,
  BOARD_REV: 0x01, I2C_VER: 0x02, DAPLINK_VER: 0x03, POWER_SRC: 0x04,
  POWER_CONS: 0x05, USB_STATE: 0x06, KL27_MODE: 0x07, LED_STATE: 0x08,
  USER_EVENT: 0x09,
  SUCCESS: 0x30, INCOMPLETE: 0x31, UNKNOWN: 0x32, FORBIDDEN: 0x33,
  NOT_RECOG: 0x34, BAD_LEN: 0x35, RD_FORB: 0x36, WR_FORB: 0x37,
  WR_FAIL: 0x38, BUSY: 0x39,
  MAX_BUF: 12, MAX_RETRIES: 20, IRQ_THRESHOLD: 30,
  // Board revisions (MicroBitPowerManager.h): V2.00 KL27 = 0x9904.
  BOARD_V200_KL27: 0x9904,
};

const FLASH = {
  FILENAME: 0x01, FILESIZE: 0x02, VISIBILITY: 0x03, WRITE_CFG: 0x04,
  ERASE_CFG: 0x05, DISK_SIZE: 0x06, SECTOR_SIZE: 0x07, REMOUNT: 0x08,
  STATUS: 0x09, READ: 0x0A, WRITE: 0x0B, ERASE: 0x0C,
  MAX_TX_RETRIES: 20, MAX_RX_RETRIES: 20, MAX_STORAGE: 0x1F000,
};

function normAddr(a) {
  if (a === 0x32) return 0x19;
  if (a === 0x3C) return 0x1E;
  if (a === 0x72) return 0x39;
  if (a === 0xE0) return 0x70;
  return a > 0x7F ? (a >> 1) : a;
}

function be16(v) { return [(v >> 8) & 0xFF, v & 0xFF]; }
function be32(v) { return [(v >>> 24) & 0xFF, (v >>> 16) & 0xFF, (v >>> 8) & 0xFF, v & 0xFF]; }
function le32(v) { return [v & 0xFF, (v >>> 8) & 0xFF, (v >>> 16) & 0xFF, (v >>> 24) & 0xFF]; }

// --- KL27 UIPM control interface (addr 0x70) ---
// Answers version/board/power probes so boot proceeds; empty user-event
// (no wake/reset/long-press pending); applies LED-state + power-mode
// writes. Always answers valid immediately (a real KL27 serves these
// from RAM; INCOMPLETE/BUSY only appear mid-transaction, which our
// synchronous take/complete can never observe — returning them would
// just burn the firmware's 20x20 retry budget for zero information).
export class Kl27Uipm {
  constructor(opts = {}) {
    this.board = opts.board ?? UIPM.BOARD_V200_KL27; // 0x9904 V2.00 KL27
    this.i2cVer = 2;          // v2 => BUSY_FLAG_SUPPORTED, no null-txn
    this.daplink = opts.daplink ?? 0x0249;
    this.powerSource = 1;     // PWR_USB_ONLY (bench is USB-powered)
    this.usbState = 4;        // USB_CONFIGURED
    this.batteryUV = 4200000;
    this.vinUV = 5000000;
    this.kl27Mode = 0;
    this.ledState = 1;        // Power LED sleep-state default ON
    this.userEvents = [];     // queued USER_EVENT codes (none idle)
    this.lastReq = [];
    this.propLens = { 0x01: 2, 0x02: 2, 0x03: 2, 0x04: 1, 0x05: 8, 0x06: 1, 0x07: 1, 0x08: 1, 0x09: 1 };
  }
  register(wasm, peripheral = 'TWIM1') {
    wasm.i2c_register_slave(peripheral, 0x70);
  }
  propBytes(prop) {
    switch (prop) {
      case UIPM.BOARD_REV: return be16(this.board);
      case UIPM.I2C_VER: return be16(this.i2cVer);
      case UIPM.DAPLINK_VER: return be16(this.daplink);
      case UIPM.POWER_SRC: return [this.powerSource & 0xFF];
      case UIPM.POWER_CONS: return [...le32(this.batteryUV), ...le32(this.vinUV)];
      case UIPM.USB_STATE: return [this.usbState & 0xFF];
      case UIPM.KL27_MODE: return [this.kl27Mode & 0xFF];
      case UIPM.LED_STATE: return [this.ledState & 0xFF];
      case UIPM.USER_EVENT: {
        const n = Math.min(this.userEvents.length, 4);
        return [n, ...this.userEvents.slice(0, n)];
      }
      default: return null;
    }
  }
  // Full request write (EASYDMA TX bytes, or byte-path transaction).
  request(bytes) {
    const b = [...bytes];
    this.lastReq = b;
    if (!b.length || b.every((x) => x === 0)) return; // e8777 NOP wake
    if (b[0] === UIPM.WRITE_REQ && b.length >= 2) {
      const prop = b[1];
      if (prop === UIPM.LED_STATE && b.length >= 4) this.ledState = b[3] & 1;
      else if (prop === UIPM.KL27_MODE && b.length >= 4) this.kl27Mode = b[3] & 0xFF;
      // Consumed user events clear once read by the Target MCU.
      if (prop === UIPM.USER_EVENT) this.userEvents = [];
    }
  }
  // Response to the last request, exactly len bytes (pad 0 / truncate).
  response(len) {
    const q = this.lastReq;
    let out;
    if (!q.length || q.every((x) => x === 0)) out = [];
    else if (q[0] === UIPM.READ_REQ && q.length >= 2) {
      const payload = this.propBytes(q[1]);
      out = payload === null
        ? [UIPM.ERR_RSP, UIPM.UNKNOWN]
        : [UIPM.READ_RSP, q[1], ...payload];
    } else if (q[0] === UIPM.WRITE_REQ && q.length >= 2) {
      out = (q[1] in this.propLens) ? [UIPM.WRITE_RSP, 0x00] : [UIPM.ERR_RSP, UIPM.UNKNOWN];
    } else out = [UIPM.ERR_RSP, UIPM.UNKNOWN];
    while (out.length < len) out.push(0);
    return out.slice(0, len);
  }
  // Byte-path helper: natural-length response for a transaction buffer.
  byteResponse(buf) {
    const b = [...buf];
    if (!b.length) return [];
    if (b[0] === UIPM.READ_REQ && b.length >= 2) {
      const payload = this.propBytes(b[1]);
      return payload === null ? [UIPM.ERR_RSP, UIPM.UNKNOWN] : [UIPM.READ_RSP, b[1], ...payload];
    }
    if (b[0] === UIPM.WRITE_REQ) { this.request(b); return [UIPM.WRITE_RSP, 0x00]; }
    return [];
  }
  // Legacy sample() shape (pointer byte + length), for direct tests.
  sample(reg, len) {
    this.request([UIPM.READ_REQ, reg & 0xFF]);
    return this.response(len);
  }
  // Test/drive hooks.
  queueUserEvent(code) { this.userEvents.push(code & 0xFF); }
  version() { return { board: this.board, i2c: this.i2cVer, daplink: this.daplink }; }
  poll() { /* protocol is purely request/response; nothing periodic */ }
}

// --- KL27 USB-FLASH storage interface (addr 0x39, wire 0x72<<1) ---
// Backs MicroBitUSBFlashManager transact(): config/geometry queries,
// READ/WRITE/ERASE on a 0x1F000-byte image (blockSize 4096 x 31).
// Always answers valid (taps registered => no NACK; BUSY-flag path
// would only burn the 20x20 retry budget — same bytes, slower).
export class Kl27Flash {
  constructor(opts = {}) {
    this.blockSize = 4096;
    this.blockCount = 31; // 31*4096 = 0x1F000 = MAX_STORAGE
    this.maxWrite = 64;
    this.fileName = [...'DATA    TXT'].map((c) => c.charCodeAt(0)); // 8.3 body
    this.fileSize = 0x1F000;
    this.visible = 1;
    const n = opts.storageBytes ?? 0x1F000;
    this.storage = new Uint8Array(n).fill(0xFF);
    this.lastReq = [];
  }
  register(wasm, peripheral = 'TWIM1') {
    wasm.i2c_register_slave(peripheral, 0x39);
  }
  geometry() { return { blockSize: this.blockSize, blockCount: this.blockCount }; }
  request(bytes) { this.lastReq = [...bytes]; }
  // Build the transact response for the stored request, exactly len bytes.
  response(len) {
    const q = this.lastReq;
    let out = [];
    if (q.length) {
      const cmd = q[0];
      const u8 = (i) => (i < q.length ? q[i] : 0);
      switch (cmd) {
        case FLASH.FILENAME: out = [cmd, ...this.fileName.slice(0, 11)]; break;
        case FLASH.FILESIZE: out = [cmd, ...be32(this.fileSize >>> 0)]; break;
        case FLASH.VISIBILITY: out = [cmd, this.visible & 1]; break;
        case FLASH.WRITE_CFG: case FLASH.ERASE_CFG: case FLASH.REMOUNT:
          out = [cmd]; break;
        case FLASH.DISK_SIZE: out = [cmd, this.blockCount & 0xFF]; break;
        case FLASH.SECTOR_SIZE: out = [cmd, ...be16(this.blockSize)]; break;
        case FLASH.STATUS: out = [cmd, 0x00]; break; // never busy
        case FLASH.READ: {
          if (q.length >= 8) {
            // Header words are BE32 (htonl on silicon): the low 24 bits
            // of word0 are the address (the top byte ORs the command —
            // silicon ORs, it does not shift: `p | (CMD << 24)`).
            const addr = ((u8(1) << 16) | (u8(2) << 8) | u8(3)) >>> 0;
            const n = ((u8(4) << 24) | (u8(5) << 16) | (u8(6) << 8) | u8(7)) >>> 0;
            out = [...q.slice(0, 8)];
            for (let i = 0; i < n; i++) out.push(this.storage[(addr + i) % this.storage.length] ?? 0xFF);
          }
          break;
        }
        case FLASH.WRITE: {
          if (q.length >= 8) {
            const addr = ((u8(1) << 16) | (u8(2) << 8) | u8(3)) >>> 0;
            const n = ((u8(4) << 24) | (u8(5) << 16) | (u8(6) << 8) | u8(7)) >>> 0;
            const seg = Math.min(n, this.maxWrite, q.length - 8);
            for (let i = 0; i < seg; i++) this.storage[(addr + i) % this.storage.length] = q[8 + i];
            out = new Array(9).fill(0); out[0] = cmd;
          }
          break;
        }
        case FLASH.ERASE: {
          if (q.length >= 8) {
            // Single-page-erase-only (status flag): erase [page, page+blockSize).
            const page = ((u8(1) << 16) | (u8(2) << 8) | u8(3)) >>> 0;
            const base = Math.floor(page / this.blockSize) * this.blockSize;
            for (let i = 0; i < this.blockSize; i++) this.storage[(base + i) % this.storage.length] = 0xFF;
            out = [cmd];
          }
          break;
        }
        default: out = []; // unknown => empty (transact failure path)
      }
    }
    while (out.length < len) out.push(0);
    return out.slice(0, len);
  }
  byteResponse(buf) {
    this.request([...buf]);
    // Natural length: config/geometry/status short; READ/WRITE/ERASE per table.
    const cmd = buf[0];
    const lens = {
      [FLASH.FILENAME]: 12, [FLASH.FILESIZE]: 5, [FLASH.VISIBILITY]: 2,
      [FLASH.WRITE_CFG]: 1, [FLASH.ERASE_CFG]: 1, [FLASH.DISK_SIZE]: 2,
      [FLASH.SECTOR_SIZE]: 3, [FLASH.REMOUNT]: 1, [FLASH.STATUS]: 2,
    };
    // Single-pointer legacy shape (no BE32 addr+len): commands 0x04+
    // carry no payload here, so VISIBILITY echoes its own leg plus the
    // stored bit (P85 legacy: [cmd, visible]).
    if (buf.length === 1 && (cmd === FLASH.VISIBILITY || (cmd >= 0x04 && cmd <= 0x09))) {
      if (cmd === FLASH.VISIBILITY) return [cmd, this.visible & 1];
      return this.response(lens[cmd] ?? 8);
    }
    return this.response(lens[cmd] ?? 8);
  }
  sample(reg, len) {
    // Legacy single-pointer shape: only meaningful for short queries.
    this.request([reg & 0xFF]);
    return this.response(len);
  }
  readStorage(addr, len) {
    const out = [];
    for (let i = 0; i < len; i++) out.push(this.storage[(addr + i) % this.storage.length]);
    return out;
  }
  poll() { /* request/response only; nothing periodic */ }
}

// --- v2 board sound/touch (P0.00 speaker, P0.05/P0.20 mic, P1.04 logo) ---
// Pin ground truth: MicroBitIO.h/.cpp — logo P1_04, speaker P0_00,
// runmic P0_20, microphone P0_05. Audio path: MicroBitAudio over
// NRF52PWM PWM1 (44100 Hz mixer); logo is capacitive-touch by default.

export const BOARD_PINS = {
  SPEAKER: [0, 0],
  MIC_IN: [0, 5],
  RUN_MIC: [0, 20],
  LOGO: [1, 4], // P1_04 capacitive touch
};

// Speaker observer (read-only: never clears firmware events).
// CODAL drives sound through PWM1 SEQSTART + P0.00 toggles; the part
// counts P0.00 OUT edges + PWM1 SEQSTARTED rising edges per frame for
// the bench Audio panel. Mute is host-side (mirrors
// setSpeakerEnabled(false): pins still play, the board speaker stops).
export class SpeakerPart {
  constructor(wasm) {
    this.wasm = wasm;
    this.toggles = 0;
    this.seqStarts = 0;
    this.lastLevel = null;
    this.lastSeq = 0;
    this.muted = false;
  }
  register() {}
  setSpeakerEnabled(on) { this.muted = !on; }
  poll() {
    const w = this.wasm;
    let lvl = null;
    try { lvl = w.gpio_read_output(...BOARD_PINS.SPEAKER); } catch { /* headless mock without gpio */ }
    if (lvl !== null && this.lastLevel !== null && !!lvl !== !!this.lastLevel) this.toggles++;
    if (lvl !== null) this.lastLevel = !!lvl;
    let seq = null;
    try { seq = w.periph_read(0x40021108, 4) >>> 0; } catch { /* no periph in mock */ }
    if (seq !== null) {
      if (this.lastSeq === 0 && seq !== 0) this.seqStarts++;
      this.lastSeq = seq;
    }
  }
  activity() { return { toggles: this.toggles, seqStarts: this.seqStarts, muted: this.muted }; }
}

// Microphone source (PDM sample filler).
// RUN_MIC P0.20 powers the mic (MicroBitAudio ctor); when the firmware
// leaves it low the mic is unpowered and reads silence — same as silicon.
// Enabled: 440 Hz sine at 16 kHz + noise, amplitude by level (0..255).
export class MicPart {
  constructor(wasm, opts = {}) {
    this.wasm = wasm;
    this.level = opts.level ?? 128;
    this.phase = 0;
  }
  register() {}
  setLevel(v) { this.level = Math.max(0, Math.min(255, v | 0)); }
  isEnabled() {
    // No wasm handle (headless test) => pretend powered so the tone
    // path is exercisable; with a handle, RUN_MIC decides (silicon).
    if (!this.wasm || typeof this.wasm.gpio_read_output !== 'function') return true;
    try { return !!this.wasm.gpio_read_output(...BOARD_PINS.RUN_MIC); }
    catch { return true; }
  }
  nextSamples(n) {
    const out = new Uint8Array(n * 2);
    if (!this.isEnabled() || this.level === 0) return out;
    const amp = (this.level / 255) * 12000;
    for (let i = 0; i < n; i++) {
      const t = this.phase++ / 16000;
      const s = Math.round(amp * Math.sin(2 * Math.PI * 440 * t) + (Math.random() - 0.5) * amp * 0.1);
      const v = Math.max(-32768, Math.min(32767, s));
      out[2 * i] = v & 0xFF;
      out[2 * i + 1] = (v >> 8) & 0xFF;
    }
    return out;
  }
  // Mic LED: lit while RUN_MIC powers the mic (bench panel dot).
  ledOn() { return this.isEnabled(); }
  poll() {}
}

// Logo touch (P1.04, capacitive by default: single-finger touch, no GND
// circuit needed — electrically still an active-low level like the
// buttons for the GPIO model). Idle HIGH (pull-up), press drives low.
export class LogoTouchPart {
  constructor(wasm) {
    this.wasm = wasm;
    this.pressed = false;
  }
  register() {}
  press() {
    this.pressed = true;
    try { this.wasm.gpio_set_input(...BOARD_PINS.LOGO, false); } catch { /* mock records */ }
  }
  release() {
    this.pressed = false;
    try { this.wasm.gpio_set_input(...BOARD_PINS.LOGO, true); } catch { /* mock records */ }
  }
  isPressed() { return this.pressed; }
  poll() {}
}

export { UIPM, FLASH, normAddr };
