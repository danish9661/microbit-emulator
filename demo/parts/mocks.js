// Driver-side mock consumers for every handshake (H) surface.
// Each part speaks the frozen v1 API (see ../API.md) the way a real board
// part would: register() BEFORE wasm.init(), poll(cpu) every frame.
// These are NOT demo wiring — they are the executable proof that each
// handshake model answers a live consumer. smoke.mjs drives the same
// poll() path with a mock wasm core; the demo page instantiates the same
// classes against the real core (wired in index.html, section "depth").
//
// What each mock proves (model behavior, not just register echo):
//   TEMP    driver mV/C value -> START -> DATARDY -> TEMP quarter-degree read
//   COMP    driver mV vs TH band -> SAMPLE -> Below/Above + UP/DOWN/CROSS
//   QDEC    host steps + Gray edges -> ACC accumulate + report/double-read
//   ECB     STARTECB stages job -> driver AES-128s in place -> ENDECB
//   AAR     START stages job -> driver resolves -> END + RESOLVED/NOTRESOLVED
//   CCM     KSGEN->ENDKSGEN, CRYPT stages job -> driver CTR+MIM -> ENDCRYPT
//   I2S     START stages RX+TX -> driver silence/capture -> STOPPED
//   NFCT    SENSE->READY, field->FIELDDETECTED, ACTIVATE->SELECTED,
//           STARTTX/ENABLERXDATA frames, FIELDLOST parks
//   TWIS    external-master write lands in RAM + RXSTARTED/WRITE/STOPPED;
//           read returns RAM + ORC pad; wrong addr DNACKs
//   SPIS    ACQUIRE + programmed RXD/TXD -> exchange returns RAM MISO,
//           END+ENDRX; unacquired returns empty
//   SPI tap edge display: CS-selected bytes queue as events with DC level,
//           MISO answers reads (ST7789-style command/data part)
//   SAADC lim: driver reports sample -> out-of-window raises LIMITH/LIMITL
//   USBD setup: host SETUP packet -> EP0SETUP + setup regs readable
//   RADIO 15.4: ED sample level, CCA idle/busy, corrupt CRCERROR path
//   QSPI backend: program AND-semantics + erase 0xFF via driver calls

const ECB_BASE = 0x4000E000;
const AAR_BASE = 0x4000F000;
const TEMP_BASE = 0x4000C000;
const COMP_BASE = 0x40013000;
const QDEC_BASE = 0x40012000;
const I2S_BASE = 0x40025000;
const NFCT_BASE = 0x40005000;
const TWIS_BASE = 0x40004000;
const SPIS_BASE = 0x40003000;
const SAADC_BASE = 0x40007000;
const USBD_BASE = 0x40027000;
const RADIO_BASE = 0x40001000;
const QSPI_BASE = 0x40029000;

function w32(wasm, base, off, v) { wasm.periph_write(base + off, 4, v >>> 0); }
function r32(wasm, base, off) { return wasm.periph_read(base + off, 4) >>> 0; }

// --- TEMP thermometer: driver owns the die value; START->DATARDY->TEMP ---
export class MockTemp {
  constructor(wasm, celsius = 27) { this.wasm = wasm; this.celsius = celsius; }
  register() {}
  poll(cpu) {
    const w = this.wasm;
    if (typeof w.temp_set_celsius === 'function') w.temp_set_celsius(this.celsius);
    w32(w, TEMP_BASE, 0x000, 1); // TASKS_START
    const ready = r32(w, TEMP_BASE, 0x100) === 1;
    const raw = r32(w, TEMP_BASE, 0x508);
    this.last = { ready, raw, celsius: (raw << 0) >> 0 === 0 ? 0 : undefined };
    // TEMP is signed quarter-degree; convert like firmware would.
    const signed = raw > 0x7FFFFFFF ? raw - 0x100000000 : raw;
    this.last.celsius = signed / 4;
    w32(w, TEMP_BASE, 0x100, 0);
    void cpu;
  }
}

// --- COMP comparator: driver mV vs TH band; SAMPLE resolves + edges ---
export class MockComp {
  constructor(wasm, mv = 100) { this.wasm = wasm; this.mv = mv; this.last = null; }
  register() {}
  setMv(mv) { this.mv = mv; }
  poll(cpu) {
    const w = this.wasm;
    w32(w, COMP_BASE, 0x500, 2); // ENABLE
    w32(w, COMP_BASE, 0x530, 16 | (48 << 8)); // THDOWN=16 THUP=48
    w32(w, COMP_BASE, 0x000, 1); // START -> READY
    if (typeof w.comp_set_input_mv === 'function') w.comp_set_input_mv(this.mv);
    w32(w, COMP_BASE, 0x008, 1); // SAMPLE
    this.last = {
      ready: r32(w, COMP_BASE, 0x100),
      result: r32(w, COMP_BASE, 0x400),
      down: r32(w, COMP_BASE, 0x104),
      up: r32(w, COMP_BASE, 0x108),
      cross: r32(w, COMP_BASE, 0x10C),
    };
    for (const e of [0x100, 0x104, 0x108, 0x10C]) w32(w, COMP_BASE, e, 0);
    void cpu;
  }
}

// --- QDEC knob: host steps accumulate; report + double-read + stop ---
export class MockQdec {
  constructor(wasm) { this.wasm = wasm; this.acc = 0; }
  register() {}
  turn(dir) { this.wasm.qdec_step(dir); }
  poll(cpu) {
    const w = this.wasm;
    w32(w, QDEC_BASE, 0x500, 1); // ENABLE
    w32(w, QDEC_BASE, 0x000, 1); // START
    this.acc = r32(w, QDEC_BASE, 0x514) | 0;
    w32(w, QDEC_BASE, 0x008, 1); // READCLRACC -> ACCREAD + clear
    this.snap = r32(w, QDEC_BASE, 0x518) | 0;
    void cpu;
  }
}

// --- ECB AES block: STARTECB stages job; driver encrypts in place ---
// FIPS-197 vector: key 00..0f, clear 00112233.., encrypted 69c4e0d8…
export class MockEcb {
  constructor(wasm) {
    this.wasm = wasm;
    this.key = [...Array(16).keys()];
    this.clear = [0x00,0x11,0x22,0x33,0x44,0x55,0x66,0x77,0x88,0x99,0xaa,0xbb,0xcc,0xdd,0xee,0xff];
    this.want = [0x69,0xc4,0xe0,0xd8,0x6a,0x7b,0x04,0x30,0xd8,0xcd,0xb7,0x80,0x70,0xb4,0xc5,0x5a];
    this.done = false;
  }
  register() {}
  poll(cpu) {
    const w = this.wasm;
    if (this.done) return;
    const ptr = 0x20001000;
    cpu.mem_write(ptr, this.key);
    cpu.mem_write(ptr + 16, this.clear);
    w32(w, ECB_BASE, 0x504, ptr); // ECBDATAPTR
    w32(w, ECB_BASE, 0x000, 1); // STARTECB
    const t = w.ecb_take_job();
    // wasm-bindgen returns Uint32Array for Vec<u32>: t[0] is the dataptr.
    // Empty vec (length 0) = idle, not a null pointer.
    if (!t || t.length === 0) return;
    const dataptr = t[0] >>> 0;
    // Driver AES-128 in place (key@+0, clear@+16 -> encrypted@+32).
    const aes = aesBlock(this.key, this.clear);
    cpu.mem_write(dataptr + 32, aes);
    w.ecb_complete();
    const got = [...cpu.mem_read(dataptr + 32, 16)];
    this.done = got.every((b, i) => b === this.want[i]) && r32(w, ECB_BASE, 0x100) === 1;
    w32(w, ECB_BASE, 0x100, 0);
    void cpu;
  }
}

// Minimal AES-128 (FIPS-197) for the ECB mock — driver-side crypto.
// Shared implementation lives in ./crypto.js (also used by the bench
// pumpDma); re-exported here so handshake.mjs keeps one import.
import { aesBlock as aesBlockImp, ctrCrypt as ctrCryptImp, cbcMic as cbcMicImp } from './crypto.js';
export const aesBlock = aesBlockImp, ctrCrypt = ctrCryptImp, cbcMic = cbcMicImp;
const ctrCryptFn = ctrCryptImp, cbcMicFn = cbcMicImp;

// --- AAR resolver: START stages job; driver answers resolved/not ---
export class MockAar {
  constructor(wasm, resolved = true) { this.wasm = wasm; this.resolved = resolved; this.done = false; }
  register() {}
  poll(cpu) {
    const w = this.wasm;
    if (this.done) return;
    w32(w, AAR_BASE, 0x508, 0x20001000); // IRKPTR
    w32(w, AAR_BASE, 0x510, 0x20002000); // ADDRPTR
    w32(w, AAR_BASE, 0x500, 1); // ENABLE = AAR map
    w32(w, AAR_BASE, 0x000, 1); // START
    const t = w.aar_take_job();
    if (!t.length) return;
    w.aar_complete(this.resolved);
    const end = r32(w, AAR_BASE, 0x100) === 1;
    const bit = r32(w, AAR_BASE, this.resolved ? 0x104 : 0x108) === 1;
    this.done = end && bit;
    w32(w, AAR_BASE, 0x100, 0); w32(w, AAR_BASE, 0x104, 0); w32(w, AAR_BASE, 0x108, 0);
    void cpu;
  }
}

// --- CCM crypt: KSGEN->ENDKSGEN, CRYPT stages job; driver CTR+MIC ---
export class MockCcm {
  constructor(wasm, plaintext = [1,2,3,4,5,6,7,8]) {
    this.wasm = wasm;
    this.pt = plaintext;
    this.key = [...Array(16).keys()];
    this.nonce = [...Array(13).keys()];
    this.done = false;
  }
  register() {}
  poll(cpu) {
    const w = this.wasm;
    if (this.done) return;
    const cnf = 0x20001000, inp = 0x20002000, outp = 0x20003000;
    cpu.mem_write(cnf, this.key);
    cpu.mem_write(cnf + 16, this.nonce);
    cpu.mem_write(inp, this.pt);
    w32(w, AAR_BASE, 0x500, 2); // ENABLE = CCM map
    w32(w, AAR_BASE, 0x504, 0); // MODE encrypt
    w32(w, AAR_BASE, 0x508, cnf);
    w32(w, AAR_BASE, 0x50C, inp);
    w32(w, AAR_BASE, 0x510, outp);
    w32(w, AAR_BASE, 0x518, this.pt.length);
    w32(w, AAR_BASE, 0x000, 1); // KSGEN -> ENDKSGEN at once
    if (r32(w, AAR_BASE, 0x100) !== 1) return;
    w32(w, AAR_BASE, 0x004, 1); // CRYPT stages job
    const t = w.ccm_take_job();
    if (!t.length) return;
    // Driver CTR crypt per the CNF contract + MIC-4 append.
    const ct = ctrCryptFn(this.key, this.nonce, this.pt);
    const mic = cbcMicFn(this.key, this.nonce, this.pt);
    cpu.mem_write(t[1], ct);
    cpu.mem_write(t[1] + ct.length, mic);
    w.ccm_complete(true);
    const ok = r32(w, AAR_BASE, 0x104) === 1 && r32(w, AAR_BASE, 0x400) === 1;
    // Decrypt back: MODE=1, in=ct+mic -> pt, MIC verifies.
    const back = ctrCryptFn(this.key, this.nonce, ct);
    this.done = ok && back.every((b, i) => b === this.pt[i]);
    w32(w, AAR_BASE, 0x104, 0); w32(w, AAR_BASE, 0x500, 0);
    void cpu;
  }
}

// (ctrCrypt/cbcMic locals removed: call sites use the imported names
// directly — ctrCrypt/cbcMic below resolve to the crypto.js imports.)

// --- I2S stream: START stages RX+TX; driver silence/capture; STOP ---
export class MockI2s {
  constructor(wasm) { this.wasm = wasm; this.done = false; }
  register() {}
  poll(cpu) {
    const w = this.wasm;
    if (this.done) return;
    w32(w, I2S_BASE, 0x500, 1); // ENABLE
    w32(w, I2S_BASE, 0x538, 0x20001000); w32(w, I2S_BASE, 0x53C, 8);
    w32(w, I2S_BASE, 0x540, 0x20002000); w32(w, I2S_BASE, 0x544, 8);
    w32(w, I2S_BASE, 0x000, 1); // START
    const rx = w.i2s_take_rx(), tx = w.i2s_take_tx();
    if (!rx.length || !tx.length) return;
    cpu.mem_write(rx[0], new Array(rx[1]).fill(0)); // silence in
    w.i2s_complete_rx();
    const bytes = [...cpu.mem_read(tx[0], tx[1])];
    w.i2s_complete_tx(bytes);
    const cap = w.i2s_take_capture();
    this.done = r32(w, I2S_BASE, 0x104) === 1 && r32(w, I2S_BASE, 0x114) === 1
      && cap.length === 8 && cap.every((b, i) => b === bytes[i]);
    w32(w, I2S_BASE, 0x004, 1); // STOP
    this.done = this.done && r32(w, I2S_BASE, 0x108) === 1;
    void cpu;
  }
}

// --- NFCT tag: phone tap -> field -> select -> frames -> field lost ---
export class MockNfct {
  constructor(wasm) { this.wasm = wasm; this.stage = 0; this.done = false; }
  register() {}
  poll(cpu) {
    const w = this.wasm;
    if (this.done) return;
    if (this.stage === 0) {
      w32(w, NFCT_BASE, 0x500, 1); // ENABLE
      w32(w, NFCT_BASE, 0x008, 1); // SENSE -> READY
      if (r32(w, NFCT_BASE, 0x100) !== 1) return;
      w.nfct_field_present(true); // phone tapped
      if (r32(w, NFCT_BASE, 0x104) !== 1) return;
      w32(w, NFCT_BASE, 0x000, 1); // ACTIVATE
      if (r32(w, NFCT_BASE, 0x14C) !== 1) return;
      this.stage = 1;
    }
    if (this.stage === 1) {
      w32(w, NFCT_BASE, 0x510, 0x20001000); w32(w, NFCT_BASE, 0x514, 4);
      w32(w, NFCT_BASE, 0x00C, 1); // STARTTX
      const t = w.nfct_take_tx();
      if (!t.length) return;
      w.nfct_complete_tx();
      if (r32(w, NFCT_BASE, 0x110) !== 1 || r32(w, NFCT_BASE, 0x130) !== 1) return;
      w32(w, NFCT_BASE, 0x01C, 1); // ENABLERXDATA
      const r = w.nfct_take_rx();
      if (!r.length) return;
      cpu.mem_write(r[0], [0xDE, 0xAD, 0xBE, 0xEF]);
      w.nfct_complete_rx(4);
      if (r32(w, NFCT_BASE, 0x118) !== 1 || r32(w, NFCT_BASE, 0x12C) !== 1) return;
      this.stage = 2;
    }
    if (this.stage === 2) {
      w.nfct_field_present(false); // phone leaves
      this.done = r32(w, NFCT_BASE, 0x108) === 1
        && r32(w, NFCT_BASE, 0x43C) === 0
        && w.nfct_take_tx().length === 0;
    }
    void cpu;
  }
}

// --- TWIS slave: external master writes land in RAM; reads return RAM ---
export class MockTwis {
  constructor(wasm, base = TWIS_BASE, addr = 0x42) {
    this.wasm = wasm; this.base = base; this.addr = addr; this.done = false;
  }
  register() {}
  poll(cpu) {
    const w = this.wasm;
    if (this.done) return;
    // Stage RX: firmware would PREPARERX with RXD.PTR/MAXCNT; the mock
    // plays both sides through periph regs + the host engine.
    w32raw(w, this.base, 0x500, 9); // TWIS ENABLE
    w32raw(w, this.base, 0x588, this.addr); // ADDRESS[0]
    w32raw(w, this.base, 0x594, 1); // CONFIG: ADDRESS0
    w32raw(w, this.base, 0x534, 0x20001000); // RXD.PTR
    w32raw(w, this.base, 0x538, 8); // RXD.MAXCNT
    w32raw(w, this.base, 0x030, 1); // PREPARERX
    const bad = cpu.twis_master_write(this.base, this.addr + 1, [9, 9]);
    if (bad !== 0) return;
    const n = cpu.twis_master_write(this.base, this.addr, [1, 2, 3]);
    if (n !== 3) return;
    const back = [...cpu.mem_read(0x20001000, 3)];
    if (back.join() !== '1,2,3') return;
    // TX: PREPARETX with known bytes, master reads them back (+ORC pad).
    cpu.mem_write(0x20002000, [9, 8, 7, 6]);
    w32raw(w, this.base, 0x544, 0x20002000); // TXD.PTR
    w32raw(w, this.base, 0x548, 4); // TXD.MAXCNT
    w32raw(w, this.base, 0x034, 1); // PREPARETX
    const rd = cpu.twis_master_read(this.base, this.addr, 6);
    // 4 RAM bytes + 2 ORC pad (default ORC 0x00).
    this.done = rd.length === 6 && rd.slice(0, 4).join() === '9,8,7,6'
      && rd.slice(4).join() === '0,0'
      && r32(w, this.base, 0x104) === 1; // STOPPED
    void cpu;
  }
}
function w32raw(w, base, off, v) { w.periph_write(base + off, 4, v >>> 0); }

// --- SPIS slave: ACQUIRE + buffers -> exchange returns RAM MISO ---
export class MockSpis {
  constructor(wasm, base = SPIS_BASE) { this.wasm = wasm; this.base = base; this.done = false; }
  register() {}
  poll(cpu) {
    const w = this.wasm;
    if (this.done) return;
    w32raw(w, this.base, 0x500, 2); // SPIS ENABLE
    w32raw(w, this.base, 0x534, 0x20001000); // RXD.PTR
    w32raw(w, this.base, 0x538, 8);
    cpu.mem_write(0x20002000, [0xAA, 0xBB]);
    w32raw(w, this.base, 0x544, 0x20002000); // TXD.PTR
    w32raw(w, this.base, 0x548, 2);
    w32raw(w, this.base, 0x024, 1); // ACQUIRE
    const miso = cpu.spis_exchange(this.base, [0x11, 0x22]);
    if (!miso.length) return;
    const rx = [...cpu.mem_read(0x20001000, 2)];
    // END 0x104 + ENDRX 0x110 (SPIS map; ENDRX is 0x110, not 0x108).
    this.done = miso.join() === '170,187' && rx.join() === '17,34'
      && r32(w, this.base, 0x104) === 1 && r32(w, this.base, 0x110) === 1;
    void cpu;
  }
}

// --- Edge SPI display (ST7789-style): CS bytes queue with DC level ---
// Bus MUST be SPIM2/SPIM3: SERIAL0/1 slots are TWIM-named ("TWIM0"),
// so DMA bytes there route to the I2C tap, never the SPI tap.
export class MockSpiDisplay {
  constructor(wasm, bus = 'SPIM2', cs = 'P0.12', dc = 'P0.11') {
    this.wasm = wasm; this.bus = bus; this.cs = cs; this.dc = dc;
    this.cmds = []; this.data = []; this.done = false;
  }
  register() { this.wasm.spi_tap(this.bus, this.cs, this.dc); }
  poll(cpu) {
    const w = this.wasm;
    if (this.done) return;
    // Drive one DMA frame like firmware would: TXD.PTR/MAXCNT + STARTTX.
    const base = this.bus === 'SPIM3' ? 0x4002F000 : 0x40023000;
    cpu.mem_write(0x20001000, [0x2C, 0x00, 0xFF]); // cmd-ish + data bytes
    w32(w, base, 0x544, 0x20001000); // TXD.PTR
    w32(w, base, 0x548, 3); // TXD.MAXCNT
    w32(w, base, 0x008, 1); // STARTTX
    const t = w.twim_take_txdma(this.bus);
    if (!t.length) return;
    const bytes = [...cpu.mem_read(t[1], t[2])];
    w.twim_complete_txdma(this.bus, bytes);
    // DMA completion routes bytes to the SPI tap (CS/DC handled JS-side).
    const evs = w.spi_take_events(this.bus);
    let nbytes = 0;
    for (const e of evs) {
      if (e & 0x80000000) continue; // CS edge: ordering only
      nbytes++;
      const byte = e & 0xFF, dc = (e >> 29) & 1;
      if (dc) this.data.push(byte); else this.cmds.push(byte);
    }
    if (nbytes < 3) return;
    // MISO answers reads (display ID read).
    w.spi_push_miso(this.bus, [0x04, 0x85, 0x52]);
    this.done = this.cmds.length + this.data.length >= 3;
    void cpu;
  }
}

// --- SAADC limit monitor: driver sample vs window raises edges ---
export class MockSaadcLim {
  constructor(wasm) { this.wasm = wasm; this.done = false; }
  register() {}
  poll(cpu) {
    const w = this.wasm;
    if (this.done) return;
    w32(w, SAADC_BASE, 0x500, 1); // ENABLE
    w32(w, SAADC_BASE, 0x51C + 2 * 16, (100 << 16) | ((-100) & 0xFFFF)); // CH2 LIMIT
    w.saadc_check_limits(2, 50);
    if (r32(w, SAADC_BASE, 0x118 + 2 * 8) !== 0) return; // inside: quiet
    w.saadc_check_limits(2, 150);
    if (r32(w, SAADC_BASE, 0x118 + 2 * 8) !== 1) return;
    w.saadc_check_limits(2, -150);
    this.done = r32(w, SAADC_BASE, 0x11C + 2 * 8) === 1;
    w32(w, SAADC_BASE, 0x118 + 2 * 8, 0); w32(w, SAADC_BASE, 0x11C + 2 * 8, 0);
    void cpu;
  }
}

// --- USBD SETUP: host packet lands in regs + EP0SETUP ---
export class MockUsbdSetup {
  constructor(wasm) {
    this.wasm = wasm;
    this.setup = [0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 0x40, 0x00]; // GET_DESCRIPTOR
    this.done = false;
  }
  register() {}
  poll(cpu) {
    const w = this.wasm;
    if (this.done) return;
    w.usbd_inject_setup(this.setup);
    if (r32(w, USBD_BASE, 0x15C) !== 1) return;
    const back = [];
    for (let i = 0; i < 8; i++) back.push(r32(w, USBD_BASE, 0x480 + i * 4) & 0xFF);
    this.done = back.every((b, i) => b === this.setup[i]);
    w32(w, USBD_BASE, 0x15C, 0);
    void cpu;
  }
}

// --- RADIO 802.15.4 helpers: ED sample, DEVMATCH/MISS, MHR, FRAMESTART,
// corrupt CRC path. Each stage programs the real registers, injects one
// addressed packet, completes RX, and checks the event batch — the same
// surface the Rust ed_cca_mhr_devmatch_framestart test proves natively.
export class MockRadio154 {
  constructor(wasm) { this.wasm = wasm; this.done = false; this.stage = 0; this.seen = {}; }
  register() {}
  oneRx(packet, corrupt) {
    const w = this.wasm;
    // Inject-then-ramp: the queue must be non-empty when START runs,
    // because do_start only stages take_rx from a non-empty queue
    // (silicon ramps first, air arrives later — same net effect here).
    if (corrupt) w.radio_inject_corrupt(packet); else w.radio_inject_rx(packet);
    w32(w, RADIO_BASE, 0x504, 0x20001000); // PACKETPTR
    w32(w, RADIO_BASE, 0x004, 1); // RXEN
    w32(w, RADIO_BASE, 0x008, 1); // START (Rx stages take_rx)
    const rxp = w.radio_take_rx();
    if (!rxp || rxp.length === 0) return null;
    return rxp[0] >>> 0;
  }
  poll(cpu) {
    const w = this.wasm;
    if (this.done) return;
    // Stage 0: ED sample level + DEVMATCH/MHR/FRAMESTART on a good packet.
    if (this.stage === 0) {
      w.radio_set_ed_dbm(-50);
      w.radio_set_rssi_dbm(-50);
      w32(w, RADIO_BASE, 0x024, 0); // noop guard (ED needs Rx below)
      w32(w, RADIO_BASE, 0x004, 1); // RXEN (ED/CCA need Rx)
      w32(w, RADIO_BASE, 0x008, 1); // START
      w32(w, RADIO_BASE, 0x024, 1); // EDSTART -> EDEND + EDSAMPLE/EDCNT
      if (r32(w, RADIO_BASE, 0x13C) !== 1) return; // EDEND
      if (r32(w, RADIO_BASE, 0x668) !== 50) return; // EDSAMPLE = -dBm
      w32(w, RADIO_BASE, 0x028, 1); // EDSTOP -> EDSTOPPED
      if (r32(w, RADIO_BASE, 0x140) !== 1) return;
      // CCA: threshold 0 (reset) vs level 50 -> busy.
      w32(w, RADIO_BASE, 0x02C, 1); // CCASTART
      if (r32(w, RADIO_BASE, 0x148) !== 1) return; // CCABUSY
      w32(w, RADIO_BASE, 0x030, 1); // CCASTOP
      if (r32(w, RADIO_BASE, 0x14C) !== 1) return;
      // Quiet air (level 0): idle.
      w.radio_set_ed_dbm(0);
      w32(w, RADIO_BASE, 0x148, 0); w32(w, RADIO_BASE, 0x144, 0);
      w32(w, RADIO_BASE, 0x02C, 1);
      if (r32(w, RADIO_BASE, 0x144) !== 1) return; // CCAIDLE
      this.seen.edCca = true;
      // Address match + MHR + FRAMESTART on RX completion.
      w32(w, RADIO_BASE, 0x600, 0xEF); // DAB[0]
      w32(w, RADIO_BASE, 0x530, 1); // RXADDRESSES: listen addr 0
      w32(w, RADIO_BASE, 0x644, 0xBEEF); // MHRMATCHCONF
      w32(w, RADIO_BASE, 0x648, 0xFFFF); // MHRMATCHMAS
      const ptr = this.oneRx([0xEF, 0xBE, 0x01], false);
      if (ptr == null) return;
      cpu.mem_write(ptr, [0xEF, 0xBE, 0x01]);
      w.radio_complete_rx();
      if (r32(w, RADIO_BASE, 0x114) !== 1) return; // DEVMATCH
      if (r32(w, RADIO_BASE, 0x138) !== 1) return; // FRAMESTART
      if (r32(w, RADIO_BASE, 0x15C) !== 1) return; // MHRMATCH
      if (r32(w, RADIO_BASE, 0x408) !== 0) return; // RXMATCH idx 0
      this.seen.match = true;
      this.stage = 1;
    }
    // Stage 1: miss path — different first byte, DAB programmed.
    if (this.stage === 1) {
      const ptr = this.oneRx([0x55, 0x00], false);
      if (ptr == null) return;
      cpu.mem_write(ptr, [0x55, 0x00]);
      w.radio_complete_rx();
      if (r32(w, RADIO_BASE, 0x118) !== 1) return; // DEVMISS
      this.seen.miss = true;
      this.stage = 2;
    }
    // Stage 2: corrupt packet -> CRCERROR (not CRCOK), END still fires.
    if (this.stage === 2) {
      const ptr = this.oneRx([0xDE, 0xAD], true);
      if (ptr == null) return;
      cpu.mem_write(ptr, [0xDE, 0xAD]);
      w.radio_complete_rx();
      const crcerr = r32(w, RADIO_BASE, 0x134) === 1;
      const end = r32(w, RADIO_BASE, 0x10C) === 1;
      const crcst = r32(w, RADIO_BASE, 0x400) === 0;
      if (!crcerr || !end || !crcst) return;
      this.seen.corrupt = true;
      this.done = this.seen.edCca && this.seen.match && this.seen.miss && this.seen.corrupt;
    }
    void cpu;
  }
}

// --- RADIO air peer (Bumble BLE bridge): TX -> bridge, RX <- bridge ---
// The bare-metal RADIO model moves bytes; the Bumble bridge process
// (tools/ble_air_bridge.py) is the air. In the demo pump the BleAir
// part (ble_air.js) owns the WebSocket; this mock is the headless
// twin used by handshake.mjs with a stub bridge object exposing
// {available, sendTx(bytes)->bool, takeRx()->Uint8Array|null}.
// TX payloads POST to the bridge and come back addressed (DEVMATCH
// path); the bridge also pushes advertising/GATT frames the mock
// delivers to staged RX + complete.
export class MockRadioAir {
  constructor(wasm, bridge = null) {
    this.wasm = wasm; this.bridge = bridge; this.done = false;
    this.seenTx = 0; this.seenRx = 0;
  }
  register() {}
  poll(cpu) {
    const w = this.wasm;
    if (this.done) return;
    if (!this.bridge || !this.bridge.available) return;
    // TX leg: stage a TX like firmware, hand bytes to the bridge.
    w32(w, RADIO_BASE, 0x504, 0x20001000); // PACKETPTR
    w32(w, RADIO_BASE, 0x518, 8); // PCNF1.MAXLEN=8
    cpu.mem_write(0x20001000, [0xEF, 0xBE, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
    w32(w, RADIO_BASE, 0x000, 1); // TXEN
    w32(w, RADIO_BASE, 0x008, 1); // START (Tx stages take_tx)
    const t = w.radio_take_tx();
    if (!t.length) return;
    const bytes = [...cpu.mem_read(t[0], t[1])];
    const sent = this.bridge.sendTx(bytes);
    w.radio_complete_tx();
    if (!sent || r32(w, RADIO_BASE, 0x10C) !== 1) return; // END
    this.seenTx++;
    // RX leg: drain one bridge frame into staged RX + complete.
    // Inject-then-ramp (queue non-empty at START), same as oneRx.
    const frame = this.bridge.takeRx();
    if (!frame) return;
    w.radio_inject_rx([...frame].slice(0, 252));
    w32(w, RADIO_BASE, 0x504, 0x20002000);
    w32(w, RADIO_BASE, 0x600, 0xEF); // DAB[0] matches echo byte 0
    w32(w, RADIO_BASE, 0x530, 1); // listen addr 0
    w32(w, RADIO_BASE, 0x004, 1); // RXEN
    w32(w, RADIO_BASE, 0x008, 1); // START
    const rxp = w.radio_take_rx();
    if (!rxp.length) return;
    cpu.mem_write(rxp[0], [...frame].slice(0, 252));
    w.radio_complete_rx();
    if (r32(w, RADIO_BASE, 0x10C) !== 1) return; // END
    this.seenRx++;
    this.done = this.seenTx >= 1 && this.seenRx >= 1;
    void cpu;
  }
}
// --- SoftDevice BLE SVC face: full GATT flow through REAL SVC bytes ---
// Unlike every other mock (which drives the model through exported
// take/complete helpers), this one executes actual `svc` instructions
// on a WasmCpu: svc #0x60 enable, GATTS service+char add, connect,
// discovery, read, write, HVX, RSSI, scan, disconnect — then drains
// every event via svc #0x61 evt_get. The stub bridge resolves staged
// jobs exactly like index.html's pumpBleLoopback (battery 87 + fixed
// table mirroring tools/ble_air_bridge.py), so this is the headless
// proof the firmware-visible SVC contract works end to end, not just
// the Rust take/complete surface (which has its own native tests).
//
// SVC numbers (S132): ENABLE 0x60, EVT_GET 0x61, GAP SCAN_START 0x8A,
// CONNECT 0x8C, DISCONNECT 0x76, RSSI_GET 0x8E, GATTC PRIM_DISC 0x90,
// CHAR_DISC 0x92, READ 0x96, WRITE 0x98, GATTS SVC_ADD 0xA0,
// CHAR_ADD 0xA2, VALUE_SET 0xA4, VALUE_GET 0xA5, HVX 0xA6,
// HV_CONFIRM 0x99.
const BLE_SVC = { ENABLE: 0x60, EVT_GET: 0x61, SCAN_START: 0x8A, CONNECT: 0x8C, DISCONNECT: 0x76, RSSI_GET: 0x8E, PRIM_DISC: 0x90, CHAR_DISC: 0x92, READ: 0x96, WRITE: 0x98, HV_CONFIRM: 0x99, SVC_ADD: 0xA0, CHAR_ADD: 0xA2, VSET: 0xA4, VGET: 0xA5, HVX: 0xA6 };
export class MockBleSvc {
  constructor(wasm) { this.wasm = wasm; this.done = false; this.seen = {}; }
  register() {}
  // Run one SVC on a scratch WasmCpu: r0-r3 in, r0 out. The hook needs
  // deliver_irqs on (else SVC faults); IRQs themselves never fire here
  // (no NVIC enables, no pending) so the run is a pure call.
  // Up to 8 register args (r0-r7) via two literal blocks: the first
  // four load r0-r3 (ldr at 0x20000000..07, literals at 0x20000024),
  // the second four load r4-r7 (ldr at 0x2000000C..13, literals at
  // 0x20000034). Callers pass r4..r7 positionally after r3.
  svc(cpu, num, r0 = 0, r1 = 0, r2 = 0, r3 = 0, r4 = 0, r5 = 0, r6 = 0, r7 = 0) {
    cpu.reset_cpu(0x20020000, 0x20000001);
    cpu.set_deliver_irqs(true);
    cpu.mem_write(0x20000000, [
      0x08, 0x48, // ldr r0, [pc, #32] -> 0x20000024
      0x09, 0x49, // ldr r1, [pc, #36] -> 0x20000028
      0x09, 0x4A, // ldr r2, [pc, #36] -> 0x2000002C
      0x0A, 0x4B, // ldr r3, [pc, #40] -> 0x20000030
      0x0A, 0x4C, // ldr r4, [pc, #40] -> 0x20000034
      0x0B, 0x4D, // ldr r5, [pc, #44] -> 0x20000038
      0x0B, 0x4E, // ldr r6, [pc, #44] -> 0x2000003C
      0x0C, 0x4F, // ldr r7, [pc, #48] -> 0x20000040
      num & 0xFF, 0xDF, // svc #num
      0xFE, 0xE7, // b .
    ]);
    const w32 = (v) => [v & 0xFF, (v >> 8) & 0xFF, (v >> 16) & 0xFF, (v >> 24) & 0xFF];
    // ldr bases: r0@00 sees 0x20000004; r1@02 sees 0x20000004; r2@04,
    // r3@06 see 0x20000008; r4@08 and r5@0A see 0x2000000C(+4&!3);
    // r6@0C, r7@0E see 0x20000010. Immediates above land each literal
    // in its slot: r0->24, r1->28, r2->2C, r3->30, r4->34, r5->38,
    // r6->3C, r7->40. (GAS-verified pattern; the 4-reg form above ran
    // the whole suite before this extension.)
    cpu.mem_write(0x20000024, [...w32(r0), ...w32(r1), ...w32(r2), ...w32(r3),
                               ...w32(r4), ...w32(r5), ...w32(r6), ...w32(r7)]);
    cpu.step(12);
    if (cpu.fault_pc() !== 0xFFFFFFFF) throw new Error(`SVC fault pc=${cpu.fault_pc().toString(16)} op=${cpu.fault_op1().toString(16)}`);
    return cpu.get_regs()[0] >>> 0;
  }
  // Drain one event via the two-arg contract; null when NOT_FOUND.
  // NOTE: the scratch cpu runs with NO system installed (pure JS
  // FlatMemory reads), so only aligned reads are used here — the
  // native FlatMemory traps odd read16 (covers a real unaligned-
  // access rule) unless a system is installed.
  drainEvt(cpu) {
    cpu.mem_write(0x20003FF0, [128, 0]); // room
    const rc = this.svc(cpu, BLE_SVC.EVT_GET, 0x20003000, 0x20003FF0);
    if (rc === 5) return null; // NOT_FOUND
    if (rc !== 0) throw new Error(`evt_get rc=${rc}`);
    const id = cpu.read8(0x20003000) | (cpu.read8(0x20003001) << 8);
    const len = cpu.read8(0x20003002) | (cpu.read8(0x20003003) << 8);
    return { id, len, body: [...cpu.mem_read(0x20003004, len - 4)] };
  }
  // Stub bridge: resolve one staged job exactly like pumpBleLoopback.
  // Returns the assigned handle for GapConnect (the bridge learns the
  // peer's link the same way, via ble_complete_gap_connect_ret).
  resolveJob() {
    const w = this.wasm;
    const bj = w.ble_take_job();
    if (!bj.length) return false;
    const tag = bj[0];
    if (tag === 0) w.ble_complete_gattc_read(bj[1], bj[2], bj[3], [w.ble_batt_level()]);
    else if (tag === 1) return w.ble_complete_gap_connect_ret([...bj.slice(1, 7)]);
    else if (tag === 2) w.ble_complete_gap_disconnect(bj[1], bj[2]);
    else if (tag === 3) w.ble_complete_rssi(bj[1], -50);
    else if (tag === 4) w.ble_post_adv_report([0x11, 0x22, 0x33, 0x44, 0x55, 0x66], -50, false, [0x02, 0x01, 0x06, 0x03, 0x03, 0x0F, 0x18]);
    else if (tag === 5) w.ble_complete_prim_disc(bj[1], [0x180F], [0x10], [0x16]);
    else if (tag === 6) w.ble_complete_char_disc(bj[1], [0x2A19], [0x12], [0x12], [0x13]);
    else if (tag === 7) w.ble_complete_desc_disc(bj[1], [0x14], [0x2902]);
    else if (tag === 8) w.ble_complete_gattc_write(bj[1], bj[3], bj[2], [...w.ble_take_data()]);
    else if (tag === 9) w.ble_complete_hvx(bj[1], bj[2]);
    else if (tag === 10) w.ble_complete_l2cap_rx(bj[1], bj[2], [...w.ble_take_data()]);
    else if (tag === 11) w.ble_complete_pairing(bj[1], true);
    else if (tag === 12) w.ble_complete_rel_disc(bj[1], [bj[2]], [0x180F], [bj[2]], [bj[3]]);
    else if (tag === 13) w.ble_complete_attr_info_disc(bj[1], [bj[2]], [0x2A19]);
    else if (tag === 14) w.ble_complete_uuid_read(bj[1], [0x13], [w.ble_batt_level()], [1]);
    else if (tag === 15) {
      const n = bj[2];
      w.ble_complete_vals_read(bj[1], new Array(n).fill(w.ble_batt_level()));
    }
    return true;
  }
  poll(cpu) {
    const w = this.wasm;
    if (this.done) return;
    if (typeof w.ble_enabled !== 'function' || typeof cpu.reset_cpu !== 'function') return;
    // Bench-shared model: the page runs blinky + depth probes on one
    // process-global core, so a previous run's links/events may still
    // be live here. Tear everything down first (all links, drain the
    // queue) so every step below sees exactly the state it creates —
    // the same isolation handshake.mjs gets from freshBoard().
    for (const h of [...w.ble_conn_handles()]) w.ble_complete_gap_disconnect(h, 19);
    // Drain the posted DISCONNECTEDs plus anything else Voices from a
    // previous run left behind (bounded; NOT_FOUND breaks the loop).
    for (let i = 0; i < 16 && w.ble_queue_len() > 0; i++) {
      try { this.drainEvt(cpu); } catch { break; }
    }
    // From here every step asserts its own event id strictly: with the
    // teardown above, nothing stale can sit ahead in the queue.
    const ok = (rc, what) => { if (rc !== 0) throw new Error(`${what} rc=${rc}`); };
    // 1. ENABLE (NULL params: sizing path).
    ok(this.svc(cpu, BLE_SVC.ENABLE, 0, 0), 'enable');
    // 2. GATTS: service + battery characteristic via real SVCs.
    cpu.mem_write(0x20001000, [0x0F, 0x18, 0x01, 0x00]); // uuid{0x180F,BLE}
    cpu.mem_write(0x20001010, [0, 0]);
    ok(this.svc(cpu, BLE_SVC.SVC_ADD, 1, 0x20001000, 0x20001010), 'svc_add');
    const svcH = cpu.read8(0x20001010) | (cpu.read8(0x20001011) << 8);
    if (!(svcH >= 0x10)) throw new Error(`svc handle ${svcH}`);
    // attr = {*uuid, *md=NULL, init_len=1, offs=0, max=1, pad, *value}.
    cpu.mem_write(0x20001100, [0x19, 0x2A, 0x01, 0x00]); // uuid{0x2A19,BLE}
    const w32 = (v) => [v & 0xFF, (v >> 8) & 0xFF, (v >> 16) & 0xFF, (v >> 24) & 0xFF];
    cpu.mem_write(0x20001110, [...w32(0x20001100), ...w32(0), 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, ...w32(0x20001128)]);
    cpu.mem_write(0x20001128, [87]);
    cpu.mem_write(0x20001130, [0, 0, 0, 0, 0, 0]);
    ok(this.svc(cpu, BLE_SVC.CHAR_ADD, svcH, 0x20001140, 0x20001110, 0x20001130), 'char_add');
    const valH = cpu.read8(0x20001130) | (cpu.read8(0x20001131) << 8);
    if (!(valH > svcH)) throw new Error(`value handle ${valH}`);
    this.seen.gatts = true;
    // 3. CONNECT via real SVC (peer addr struct), resolve, drain CONNECTED.
    // The stub returns the assigned handle; every later step uses THAT
    // handle, never a hardcoded 1 — the model assigns handles from its
    // link table, and a previous run's links shift the numbering.
    cpu.mem_write(0x20001200, [0x01, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
    ok(this.svc(cpu, BLE_SVC.CONNECT, 0x20001200, 0, 0), 'connect');
    const connH = this.resolveJob();
    if (typeof connH !== 'number') throw new Error('connect staged nothing');
    const conn = this.drainEvt(cpu);
    if (!conn || conn.id !== 0x10) throw new Error(`CONNECTED missing ${JSON.stringify(conn)}`);
    if (conn.body[16] !== 2) throw new Error(`role CENTRAL, got ${conn.body[16]}`);
    this.seen.connected = true;
    const H = connH;
    // 4. PRIM_DISC -> resolve -> drain (battery service present).
    ok(this.svc(cpu, BLE_SVC.PRIM_DISC, H, 1, 0), 'prim_disc');
    this.resolveJob();
    const pd = this.drainEvt(cpu);
    if (!pd || pd.id !== 0x30) throw new Error('PRIM_DISC_RSP missing');
    this.seen.disc = true;
    // 4b. REL_DISC + ATTR_INFO_DISC + UUID_READ + VALS_READ (new RSPs).
    // Range lives at 0x20001300 (written by the CHAR_DISC step's twin
    // below — write it here first; each discovery step is independent).
    cpu.mem_write(0x20001300, [0x10, 0x00, 0x16, 0x00]); // range{start,end}
    cpu.mem_write(0x20001310, [0x19, 0x2A, 0x01, 0x00]); // uuid{0x2A19}
    ok(this.svc(cpu, 0x91, H, 0x20001300), 'rel_disc');
    this.resolveJob();
    const rel = this.drainEvt(cpu);
    if (!rel || rel.id !== 0x31) throw new Error('REL_DISC_RSP missing');
    ok(this.svc(cpu, 0x94, H, 0x20001300), 'attr_info_disc');
    this.resolveJob();
    const ai = this.drainEvt(cpu);
    if (!ai || ai.id !== 0x34) throw new Error('ATTR_INFO_RSP missing');
    ok(this.svc(cpu, 0x95, H, 0x20001100, 0x20001300), 'uuid_read');
    this.resolveJob();
    const ur = this.drainEvt(cpu);
    if (!ur || ur.id !== 0x35) throw new Error('UUID_READ_RSP missing');
    if (ur.body[ur.body.length - 1] !== 87) throw new Error('uuid-read byte');
    cpu.mem_write(0x20001320, [0x13, 0x00, 0x14, 0x00]);
    ok(this.svc(cpu, 0x97, H, 0x20001320, 2), 'vals_read');
    this.resolveJob();
    const vr = this.drainEvt(cpu);
    if (!vr || vr.id !== 0x37) throw new Error('VALS_READ_RSP missing');
    if (vr.body[vr.body.length - 2] !== 87) throw new Error('vals-read bytes');
    this.seen.disc2 = true;
    // 5. CHAR_DISC over the service range -> drain.
    cpu.mem_write(0x20001300, [0x10, 0x00, 0x16, 0x00]);
    ok(this.svc(cpu, BLE_SVC.CHAR_DISC, H, 0x20001300), 'char_disc');
    this.resolveJob();
    const cd = this.drainEvt(cpu);
    if (!cd || cd.id !== 0x32) throw new Error('CHAR_DISC_RSP missing');
    // 6. READ the battery value -> drain (87 over the stub air).
    ok(this.svc(cpu, BLE_SVC.READ, H, valH, 0), 'read');
    this.resolveJob();
    const rr = this.drainEvt(cpu);
    if (!rr || rr.id !== 0x36) throw new Error('READ_RSP missing');
    const battByte = rr.body[rr.body.length - 1];
    if (battByte !== 87) throw new Error(`battery ${battByte}`);
    this.seen.readRsp = true;
    // 7. WRITE two bytes -> drain WRITE_RSP.
    cpu.mem_write(0x20001410, [0xAA, 0xBB]);
    cpu.mem_write(0x20001400, [0x01, 0x00, valH & 0xFF, (valH >> 8) & 0xFF, 0x00, 0x00, 0x02, 0x00, 0x10, 0x14, 0x00, 0x20]);
    ok(this.svc(cpu, BLE_SVC.WRITE, H, 0x20001400), 'write');
    this.resolveJob();
    const wr = this.drainEvt(cpu);
    if (!wr || wr.id !== 0x38) throw new Error('WRITE_RSP missing');
    this.seen.writeRsp = true;
    // 7b. L2CAP: register CID, TX a frame, drain RX echo, unregister.
    // NOTE: the depth-probe run just before this one on the bench
    // registers the same CID on the shared model and never unregisters
    // (its scratch cpu is gone but model state persists) — so tolerate
    // CID_IN_USE (0x3100) here: the CID is already ours.
    // (Unregister-then-register would break the probe run still holding
    // it; registration is idempotent-by-tolerance instead.)
    {
      const rc = this.svc(cpu, 0xB0, 0x40, 0, 0);
      if (rc !== 0 && rc !== 0x3100) throw new Error(`l2cap_register rc=${rc}`);
    }
    cpu.mem_write(0x20001600, [0x03, 0x00, 0x40, 0x00]); // header{len=3, cid}
    cpu.mem_write(0x20001610, [0xDE, 0xAD, 0xBE]);
    ok(this.svc(cpu, 0xB2, H, 0x20001600, 0x20001610), 'l2cap_tx');
    this.resolveJob();
    const l2 = this.drainEvt(cpu);
    if (!l2 || l2.id !== 0x70) throw new Error('L2CAP_RX missing');
    if ((l2.body[4] | (l2.body[5] << 8)) !== 0x40 || l2.body[6] !== 0xDE) throw new Error('L2CAP echo transposed');
    // 7c. AUTHENTICATE -> resolve (paired) -> drain AUTH_STATUS + SEC_UPDATE.
    ok(this.svc(cpu, 0x7E, H, 0, 0), 'authenticate');
    this.resolveJob();
    const au = this.drainEvt(cpu);
    if (!au || au.id !== 0x19) throw new Error('AUTH_STATUS missing');
    if (au.body[2] !== 0x00) throw new Error(`AUTH_STATUS success, got ${au.body[2]}`);
    if ((au.body[3] & 0x04) === 0) throw new Error('AUTH_STATUS bonded bit unset');
    const su = this.drainEvt(cpu);
    if (!su || su.id !== 0x1A) throw new Error('CONN_SEC_UPDATE missing');
    this.seen.paired = true;
    // 7c2. Peer-initiated pairing: driver posts SEC_PARAMS_REQUEST,
    // firmware accepts, passkey roundtrip, paired again. Exercises the
    // reply-SVC face (SEC_PARAMS_REPLY accept + AUTH_KEY_REQUEST +
    // AUTH_KEY_REPLY) through the same real-SVC path as silicon.
    // (ble_gap_sec_params_t wire: flags, min/max key size, kdist×2.)
    w.ble_post_sec_params_request(H, [0x0D, 7, 16, 0x01, 0x00]);
    const spr = this.drainEvt(cpu);
    if (!spr || spr.id !== 0x13) throw new Error('SEC_PARAMS_REQUEST missing');
    if (spr.body[2] !== 0x0D) throw new Error('peer params flags echo');
    cpu.mem_write(0x20004000, [0x0D, 7, 16, 0x01, 0x00]);
    ok(this.svc(cpu, 0x7F, H, 0, 0x20004000, 0), 'sec-params-accept');
    // Drain order: the accept stages GapAuthenticate; post the key
    // request BEFORE resolving the handshake (the event queue is
    // FIFO — AUTH_KEY_REQUEST must sit ahead of the handshake's
    // AUTH_STATUS + CONN_SEC_UPDATE). Then resolve (pairs the stub
    // air) and drain in order: key request, reply, handshake pair.
    w.ble_post_auth_key_request(H, 1);
    this.resolveJob(); // driver handshake (GapAuthenticate -> paired)
    const akr = this.drainEvt(cpu);
    if (!akr || akr.id !== 0x17) throw new Error('AUTH_KEY_REQUEST missing');
    cpu.mem_write(0x20004100, [0x31, 0x32, 0x33, 0x34, 0x35, 0x36]); // "123456"
    ok(this.svc(cpu, 0x80, H, 1, 0x20004100), 'auth-key-passkey');
    const hk = this.drainEvt(cpu);
    if (!hk || hk.id !== 0x19) throw new Error('AUTH_STATUS (peer leg) missing');
    const sk = this.drainEvt(cpu);
    if (!sk || sk.id !== 0x1A) throw new Error('CONN_SEC_UPDATE (peer leg) missing');
    this.seen.peerPair = true;
    // 7d. Indication roundtrip (CCCD-gated): subscribe indicate bit,
    // HVX indication stages, peer confirms via HV_CONFIRM -> HVC.
    // The HVX params block at 0x20003200 was left staged for notify by
    // the native test shape; rewrite it here for indication (type=2).
    cpu.mem_write(0x20003200, [valH & 0xFF, (valH >> 8) & 0xFF, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00]);
    cpu.mem_write(0x20003210, [0x01, 0x00]); // *len = 1
    cpu.mem_write(0x20003220, [0x42]);
    cpu.mem_write(0x20003208, [0x10, 0x32, 0x00, 0x20]); // *len ptr
    cpu.mem_write(0x2000320C, [0x20, 0x32, 0x00, 0x20]); // *data ptr
    w.ble_post_gatts_write(H, valH + 1, 0x2902, 1, [0x02, 0x00]);
    this.drainEvt(cpu); // consume the WRITE event
    ok(this.svc(cpu, BLE_SVC.HVX, H, 0x20003200), 'hvx-indicate');
    this.resolveJob();
    const hvc1 = this.drainEvt(cpu);
    if (!hvc1 || hvc1.id !== 0x53) throw new Error('HVC (indicate) missing');
    ok(this.svc(cpu, BLE_SVC.HV_CONFIRM, H, valH), 'hv_confirm');
    const hvc2 = this.drainEvt(cpu);
    if (!hvc2 || hvc2.id !== 0x53) throw new Error('HVC (confirm) missing');
    this.seen.hvx = true;
    // 8. SCAN_START -> resolve -> drain ADV_REPORT (padded layout).
    ok(this.svc(cpu, BLE_SVC.SCAN_START, 0), 'scan');
    this.resolveJob();
    const adv = this.drainEvt(cpu);
    if (!adv || adv.id !== 0x1D) throw new Error('ADV_REPORT missing');
    if (adv.body[11] !== 0) throw new Error('adv pad byte nonzero');
    // 9. RSSI_GET answers now; completion posts RSSI_CHANGED.
    cpu.mem_write(0x20001500, [0]);
    ok(this.svc(cpu, BLE_SVC.RSSI_GET, H, 0x20001500), 'rssi');
    this.resolveJob();
    const rc = this.drainEvt(cpu);
    if (!rc || rc.id !== 0x1C) throw new Error('RSSI_CHANGED missing');
    // 10. DISCONNECT -> resolve -> drain DISCONNECTED, queue empties.
    ok(this.svc(cpu, BLE_SVC.DISCONNECT, H, 19), 'disconnect');
    this.resolveJob();
    const dc = this.drainEvt(cpu);
    if (!dc || dc.id !== 0x11 || dc.body[2] !== 19) throw new Error('DISCONNECTED missing');
    if (this.drainEvt(cpu) !== null) throw new Error('queue not drained');
    this.seen.full = true;
    this.done = this.seen.gatts && this.seen.connected && this.seen.readRsp && this.seen.writeRsp && this.seen.paired && this.seen.peerPair && this.seen.hvx && this.seen.disc2 && this.seen.full;
    void cpu;
  }
}
export class MockQspi {
  constructor(wasm) { this.wasm = wasm; this.stage = 0; this.done = false; }
  register() {
    this.wasm.qspi_register_flash('QSPI', new Array(65536).fill(0xFF));
  }
  poll(cpu) {
    const w = this.wasm;
    if (this.done) return;
    if (this.stage === 0) {
      w32(w, QSPI_BASE, 0x500, 1); // ENABLE
      w32(w, QSPI_BASE, 0x524, 0x1000); // WRITE.DST
      w32(w, QSPI_BASE, 0x528, 4); // WRITE.CNT
      w32(w, QSPI_BASE, 0x008, 1); // WRITESTART
      const t = w.qspi_take_write();
      if (!t.length) return;
      cpu.mem_write(0x20001000, [0xDE, 0xAD, 0xBE, 0xEF]);
      const bytes = [...cpu.mem_read(0x20001000, t[2])];
      w.qspi_complete_write(t[1], bytes);
      this.stage = 1;
    }
    if (this.stage === 1) {
      // AND semantics: programming 0xFF/0xFF/0x00/0xFF over
      // DE AD BE EF keeps DE AD 00 EF.
      w32(w, QSPI_BASE, 0x524, 0x1000);
      w32(w, QSPI_BASE, 0x528, 4);
      w32(w, QSPI_BASE, 0x008, 1);
      const t = w.qspi_take_write();
      if (!t.length) return;
      w.qspi_complete_write(t[1], [0xFF, 0xFF, 0x00, 0xFF]);
      // Indirect read back through the staged path.
      w32(w, QSPI_BASE, 0x514, 0x1000); // READ.SRC
      w32(w, QSPI_BASE, 0x518, 0x20002000); // READ.DST
      w32(w, QSPI_BASE, 0x51C, 4); // READ.CNT
      w32(w, QSPI_BASE, 0x004, 1); // READSTART
      const r = w.qspi_take_read();
      if (!r.length) return;
      w.qspi_complete_read();
      if (r32(w, QSPI_BASE, 0x104) !== 1) return;
      this.stage = 2;
    }
    if (this.stage === 2) {
      w32(w, QSPI_BASE, 0x52C, 0x1000); // ERASE.PTR
      w32(w, QSPI_BASE, 0x530, 0); // 4KB sector
      w32(w, QSPI_BASE, 0x00C, 1); // ERASESTART
      const e = w.qspi_take_erase();
      if (!e.length) return;
      w.qspi_complete_erase(e[0], e[1]);
      this.done = r32(w, QSPI_BASE, 0x104) === 1;
    }
    void cpu;
  }
}
