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
// nrfx writes ADDRESS shifted (addr<<1: 0x32/0x3C); the Rust model hands
// take_* the raw register value. Normalize both forms here.
function normAddr(a) { return a > 0x7F ? (a >> 1) : a; }

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
    // KL27 USB interface chip (MicroBitPowerManager, polled UIPM addr
    // 0x70): shares irq1/P0.25 with the sensors. With no tap the model
    // NACKs the version/board-revision probes and boot degrades — answer
    // empty (no event).
    // KL27 USB-FLASH chip (USBFlashManager, addr 0x39 / shifted 0x72):
    // MUST also be stubbed, with FAIL-FAST bytes (see sample()): NACK
    // there does NOT fail fast — i2cBus.read returns DEVICE_I2C_ERROR
    // which hits `break`, but a NACK-free zero frame means "NOT READY"
    // → busy → rx_attempts=0 forever (P53d: 1217 reads/100M, no banner).
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
  sample(addr, reg, len) {
    addr = normAddr(addr);
    // KL27 UIPM stub: empty frame (no event).
    if (addr === 0x70) return new Array(len).fill(0);
    // KL27 USB-FLASH stub: FAIL-FAST, not busy. _transact treats
    // b[0]==0x20 (ERROR_RESPONSE) with b[1]!=0x39 as NOT-busy → break
    // (one RX attempt) instead of resetting rx_attempts=0 forever.
    // (BUSY_FLAG_SUPPORTED is never set — version probe reads zeros —
    // so the inferred branch b[0]==0x20 && b[1] not in {request[0],0}
    // is the one that exits. Zeros would mean "NOT READY"→busy→retry.)
    if (addr === 0x39) { const f = [0x20, 0x01]; while (f.length < len) f.push(0); return f.slice(0, len); }
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
    let t = w.twim_take_txdma(P);
    if (t.length) t[0] = normAddr(t[0]);
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
    if (t.length) t[0] = normAddr(t[0]);
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
