// LSM303AGR accelerometer + magnetometer (micro:bit v2 motion sensor).
// Speaks the tap + EASYDMA driver API (see ../API.md): register() BEFORE
// wasm.init(), poll(cpu) every frame after stepping.
//
// Register model per slave (accel 0x19, mag 0x1E):
//   WHO_AM_I -> 0x33 / 0x40, STATUS -> data-ready, CTRL -> echo,
//   OUT_* -> live synthetic data (tilt simulation, overridable).
// Live model: gentle tilt sine waves in mg + static mag field; override
// anytime with setAccel({x,y,z}) / setMag({x,y,z}) / shake().

const ACCEL = 0x19;
const MAG = 0x1E;

function le16(v) {
  v = Math.max(-32768, Math.min(32767, Math.round(v)));
  if (v < 0) v += 65536;
  return [v & 0xFF, (v >> 8) & 0xFF];
}

export class LSM303 {
  constructor(wasm, peripheral = 'TWIM1') {
    this.wasm = wasm;
    this.peripheral = peripheral;
    this.regptr = { [ACCEL]: 0, [MAG]: 0 };
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
  sample(addr, reg, len) {
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
    // --- EASYDMA path (nrfx drivers): staged transfers with addresses ---
    let t = w.twim_take_txdma(P);
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
      this.feedWrite(addr, buf);
      // A lone pointer-set always precedes a read: anticipate it now,
      // whether the transaction closed (STOP) or stays open for a
      // repeated START. (Only staleness source: a pointer-set that is
      // never read; firmware always reads what it points at.)
      if (buf.length === 1) {
        w.i2c_push_rx(P, this.sample(addr, buf[0] & 0x7F, 8));
      }
      buf = [];
    };
    for (const e of evs) {
      if (e & 0x80000000) {
        if (e & 0x40000000) { flush(); addr = e & 0x7F; }
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
