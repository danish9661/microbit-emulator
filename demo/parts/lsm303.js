// LSM303AGR accelerometer + magnetometer (micro:bit v2 motion sensor).
// Speaks the tap + EASYDMA driver API (see ../API.md): register() BEFORE
// wasm.init(), poll(cpu) every frame after stepping.
//
// Register model per slave (accel 0x19, mag 0x1E):
//   WHO_AM_I -> 0x33 / 0x40, STATUS -> data-ready, CTRL -> echo,
//   OUT_* -> live synthetic data (tilt simulation, overridable).
// Live model: gentle tilt sine waves in mg + static mag field; override
// anytime with setAccel({x,y,z}) / setMag({x,y,z}) / shake().

import { Kl27Uipm, Kl27Flash } from './kl27.js';

const ACCEL = 0x19;
const MAG = 0x1E;
// nrfx writes ADDRESS shifted (7-bit addr<<1: 0x19->0x32, 0x1E->0x3C,
// 0x39->0x72, 0x70->0xE0); the Rust model hands take_* the raw register
// value. Normalize BOTH forms here. NOTE: a plain `a > 0x7F` check is
// WRONG — shifted 0x32/0x3C/0x72 are all < 0x80 yet still shifted
// (P86: 0x72 traffic never reached the 0x39 stub, so flash reads got
// zeros = busy = the whole pre-banner boot time).
function normAddr(a) {
  if (a === 0x32) return 0x19;
  if (a === 0x3C) return 0x1E;
  if (a === 0x72) return 0x39;
  if (a === 0xE0) return 0x70;
  return a > 0x7F ? (a >> 1) : a;
}

function le16(v) {
  v = Math.max(-32768, Math.min(32767, Math.round(v)));
  if (v < 0) v += 65536;
  return [v & 0xFF, (v >> 8) & 0xFF];
}

export class LSM303 {
  constructor(wasm, peripheral = 'TWIM1', kl27 = null) {
    this.wasm = wasm;
    this.peripheral = peripheral;
    // KL27 protocol engines (kl27.js): owned here so the SAME instances
    // serve the EASYDMA path, the byte path, and the bench panels.
    // Shared instance may be injected (index.html wires one board-wide
    // pair); default constructs a private pair for headless tests.
    this.kl27 = kl27?.uipm ?? new Kl27Uipm();
    this.flash = kl27?.flash ?? new Kl27Flash();
    this.regptr = { [ACCEL]: 0, [MAG]: 0, [0x70]: 0, [0x39]: 0 };
    // regfile[(addr,reg)] = last written value (CTRL echo etc.)
    this.regs = {};
    // live sensor state (milli-g / microtesla-ish raw-ish units)
    this.accel = { x: 0, y: 0, z: 1000 };
    this.mag = { x: 200, y: 0, z: 400 };
    this.auto = true; // sine tilt simulation when true
    this.t0 = Date.now();
  }

  register() {
    this.wasm.i2c_register_slave(this.peripheral, ACCEL);
    this.wasm.i2c_register_slave(this.peripheral, MAG);
    // KL27 USB interface chip (MicroBitPowerManager, polled UIPM addr
    // 0x70): shares irq1/P0.25 with the sensors. With no tap the model
    // NACKs the version/board-revision probes and boot degrades — answer
    // empty (no event).
    // KL27 USB-FLASH chip (USBFlashManager, addr 0x39 / shifted 0x72):
    // MUST also be stubbed, with VALID request-echo frames (see
    // sample()): the old FAIL-FAST bytes ([0x20,0x01]) never matched
    // the request echo, so _transact read them as busy/not-ready and
    // burned the full 20x20 retry budget per transact (P85: the whole
    // pre-banner boot time). An echo exits on the first RX attempt.
    this.wasm.i2c_register_slave(this.peripheral, 0x70);
    this.wasm.i2c_register_slave(this.peripheral, 0x39);
  }

  setAccel(mg) { this.accel = { ...mg }; this.auto = false; }
  setMag(u) { this.mag = { ...u }; this.auto = false; }
  shake(mg = 2000) {
    this.accel = {
      x: (Math.random() - 0.5) * 2 * mg,
      y: (Math.random() - 0.5) * 2 * mg,
      z: (Math.random() - 0.5) * 2 * mg,
    };
    this.auto = false;
  }

  liveAccel() {
    if (!this.auto) return this.accel;
    const t = (Date.now() - this.t0) / 1000;
    return {
      x: Math.round(500 * Math.sin(t * 0.9)),
      y: Math.round(500 * Math.sin(t * 0.7 + 1)),
      z: Math.round(1000 + 80 * Math.sin(t * 1.3)),
    };
  }

  // Sample bytes for (addr, startReg, len), MSB-masked auto-increment.
  // KL27 traffic is owned by the kl27.js protocol engines (Kl27Uipm /
  // Kl27Flash): UIPM answers valid protocol frames per the CODAL wire
  // contract (READ_RSP/WRITE_RSP/ERR_UNKNOWN, never a bare empty frame —
  // the model would treat zeros as "no event" and burn the 20x20 retry
  // budget); USB-FLASH answers config/geometry/storage per the same.
  // NOTE: this pointer+length shape only fits SHORT queries (UIPM ≤ 12 B,
  // config/geometry). Full transact frames (READ/WRITE/ERASE with BE32
  // addr+len headers) flow through poll()'s take/complete path below.
  sample(addr, reg, len) {
    addr = normAddr(addr);
    if (addr === 0x70) {
      const r = this.kl27.byteResponse([0x10, reg & 0xFF]);
      while (r.length < len) r.push(0);
      return r.slice(0, len);
    }
    // USB-FLASH legacy echo (P85): answers VALID request-echo frames so
    // _transact EXITS on the first RX attempt (b[0]==request[0]); zeros
    // or 0x20-mismatches read as busy and burn the 20x20 retry budget.
    // Full READ/WRITE/ERASE transact frames go through poll() below.
    // Config/geometry queries additionally match the wire contract:
    // FILENAME echoes + 8.3 body (getConfiguration needs len>5 to parse),
    // FILESIZE/DISK_SIZE/SECTOR_SIZE/VISIBILITY answer their tables.
    if (addr === 0x39) {
      const f = this.flash.byteResponse([reg & 0xFF]);
      if (f.length) {
        while (f.length < len) f.push(0);
        return f.slice(0, len);
      }
      const g = [reg & 0xFF];
      if ((reg & 0xFF) === 0x01) { g.push(...[...'DATA    TXT'].map((c) => c.charCodeAt(0)), 0); }
      while (g.length < len) g.push(0);
      return g.slice(0, len);
    }
    const out = [];
    const a = this.liveAccel();
    for (let k = 0; k < len; k++) {
      const r = (reg + k) & 0x7F;
      const key = addr * 256 + r;
      if (addr === ACCEL && r === 0x0F) out.push(0x33);
      else if (addr === MAG && (r === 0x0F || r === 0x4F)) out.push(0x40);
      else if (addr === ACCEL && r === 0x27) out.push(0x0F); // ZYXDA+...
      else if (addr === MAG && (r === 0x27 || r === 0x67)) out.push(0x08);
      else if (addr === ACCEL && r >= 0x28 && r <= 0x2D) {
        const vals = [a.x, a.y, a.z].flatMap(le16);
        out.push(vals[(r - 0x28) % 6] ?? 0);
      } else if (addr === MAG && r >= 0x68 && r <= 0x6D) {
        const vals = [this.mag.x, this.mag.y, this.mag.z].flatMap(le16);
        out.push(vals[(r - 0x68) % 6] ?? 0);
      } else if (key in this.regs) out.push(this.regs[key]);
      else out.push(0);
    }
    return out;
  }

  poll(cpu) {
    const w = this.wasm, P = this.peripheral;
    // DRDY (P0.25 = MICROBIT_PIN_SENSOR_DATA_READY, irq1, active-lo):
    // PULSE, never permanent low. irq1 is shared with the KL27 USB
    // interface chip: MicroBitPowerManager::idleCallback treats a
    // sustained low (>30 consecutive idle ticks) as a USB event and
    // hammers I2C 0x70/0x72 (which NACKs here — no KL27), while the
    // LSM303 requestUpdate awaitSample spin only needs a brief active
    // window to latch its first sample. 60ms low / 140ms high: the
    // tight sensor spin exits within ~2 frames of a low window, and
    // the USB threshold (30 ticks) can never fill.
    if (typeof w.gpio_set_input === 'function') w.gpio_set_input(0, 25, (Date.now() % 200) < 60 ? false : true);
    // --- EASYDMA path (nrfx drivers): staged transfers with addresses ---
    // KL27 frames route to the protocol engines: the UIPM request write
    // is recorded (e8777 NOP-safe) and its RX answered from the SAME
    // request bytes; USB-FLASH transact frames (BE32 addr+len headers)
    // go request->response through Kl27Flash. Sensor addrs keep the
    // regptr/file behavior below.
    let t = w.twim_take_txdma(P);
    if (t.length) t[0] = normAddr(t[0]);
    if (t.length && (t[0] === 0x70 || t[0] === 0x39)) {
      const bytes = [...cpu.mem_read(t[1], t[2])];
      if (t[0] === 0x70) this.kl27.request(bytes);
      else this.flash.request(bytes);
      w.twim_complete_txdma(P, bytes);
      t = [];
    }
    if (t.length) {
      const bytes = cpu.mem_read(t[1], t[2]);
      if (bytes.length === 1) {
        this.regptr[t[0]] = bytes[0] & 0x7F;
      } else if (bytes.length > 1) {
        let r = bytes[0] & 0x7F;
        for (const b of bytes.slice(1)) {
          if (!this.isIdReg(t[0], r)) this.regs[t[0] * 256 + r] = b;
          r = (r + 1) & 0x7F;
        }
      }
      w.twim_complete_txdma(P, bytes);
    }
    t = w.twim_take_rxdma(P);
    // take_* already hands the normalized 7-bit address (P86: the Rust
    // model normalizes at the source, so 0x72 arrives as 0x39); the
    // local normAddr is a harmless idempotent belt-and-braces.
    if (t.length) t[0] = normAddr(t[0]);
    if (t.length && (t[0] === 0x70 || t[0] === 0x39)) {
      const bytes = t[0] === 0x70
        ? this.kl27.response(t[2])
        : this.flash.response(t[2]);
      cpu.mem_write(t[1], bytes);
      w.twim_complete_rxdma(P, t[2]);
      t = [];
    }
    if (t.length) {
      const reg = this.regptr[t[0]] ?? 0;
      const bytes = this.sample(t[0], reg, t[2]);
      cpu.mem_write(t[1], bytes);
      w.twim_complete_rxdma(P, t[2]);
    }
    // --- byte/polling path: parse START(addr)+bytes+STOP from tap events ---
    const evs = w.i2c_take_events(P);
    let addr = null, buf = [];
    const flush = () => {
      if (addr === null || !buf.length) { buf = []; return; }
      if (addr === 0x70) {
        // UIPM byte transaction: request bytes in, natural-length reply
        // anticipated into the RX queue (recvUIPMPacket only reads while
        // irq1 is active — the DRDY pulse gates that on silicon).
        const resp = this.kl27.byteResponse(buf);
        if (resp.length) w.i2c_push_rx(P, resp);
      } else if (addr === 0x39) {
        // USB-FLASH byte transaction: same request/response shape.
        const resp = this.flash.byteResponse(buf);
        if (resp.length) w.i2c_push_rx(P, resp);
      } else {
        this.feedWrite(addr, buf);
        // A lone pointer-set always precedes a read: anticipate it now,
        // whether the transaction closed (STOP) or stays open for a
        // repeated START. (Only staleness source: a pointer-set that is
        // never read; firmware always reads what it points at.)
        if (buf.length === 1) {
          w.i2c_push_rx(P, this.sample(addr, buf[0] & 0x7F, 8));
        }
      }
      buf = [];
    };
    for (const e of evs) {
      if (e & 0x80000000) {
        if (e & 0x40000000) { flush(); addr = normAddr(e & 0x7F); }
        else { flush(); addr = null; }
      } else if (addr !== null) {
        buf.push(e & 0xFF);
      }
    }
    flush(); // trailing open transaction, if any
  }

  isIdReg(addr, r) {
    return (addr === ACCEL && r === 0x0F) ||
      (addr === MAG && (r === 0x0F || r === 0x4F));
  }

  feedWrite(addr, bytes) {
    if (bytes.length === 1) {
      this.regptr[addr] = bytes[0] & 0x7F;
    } else if (bytes.length > 1) {
      let r = bytes[0] & 0x7F;
      for (const b of bytes.slice(1)) {
        if (!this.isIdReg(addr, r)) this.regs[addr * 256 + r] = b;
        r = (r + 1) & 0x7F;
      }
    }
  }
}
