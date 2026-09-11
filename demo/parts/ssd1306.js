// SSD1306 128x64 OLED (typical micro:bit edge add-on, I2C addr 0x3C).
// Speaks the tap + EASYDMA driver API: register() BEFORE wasm.init(),
// poll(cpu, canvas) every frame. Renders the GDDRAM framebuffer.
//
// Protocol: control byte Co(bit7 continuation) + D/C#(bit6: 0 command,
// 1 data). Commands honored: 0x21 column range, 0x22 page range,
// 0x81 contrast (ignored), 0xA4/A5 normal/all-on, 0xAE/AF off/on,
// 0x20 addressing mode (horizontal assumed). Everything else ignored.
const OLED_ADDR = 0x3C;
const W = 128, H = 64;

export class SSD1306 {
  constructor(wasm, peripheral = 'TWIM0', canvas = null) {
    this.wasm = wasm;
    this.peripheral = peripheral;
    this.canvas = canvas;
    this.fb = new Uint8Array((W * H) / 8);
    this.col0 = 0; this.col1 = W - 1;
    this.page0 = 0; this.page1 = 7;
    this.col = 0; this.page = 0;
    this.on = false;
    this.allOn = false;
    if (canvas) {
      canvas.width = W; canvas.height = H;
      this.ctx = canvas.getContext('2d');
    } else {
      this.ctx = null;
    }
    this.cmdBuf = [];
  }

  register() {
    this.wasm.i2c_register_slave(this.peripheral, OLED_ADDR);
  }

  command(b) {
    switch (b) {
      case 0xAE: this.on = false; break;
      case 0xAF: this.on = true; break;
      case 0xA4: this.allOn = false; break;
      case 0xA5: this.allOn = true; break;
      default: this.cmdBuf.push(b); this.runCmd(); break;
    }
  }

  runCmd() {
    const c = this.cmdBuf;
    if (c[0] === 0x21 && c.length >= 3) {
      this.col0 = c[1] & 0x7F; this.col1 = c[2] & 0x7F;
      this.col = this.col0; this.cmdBuf = [];
    } else if (c[0] === 0x22 && c.length >= 3) {
      this.page0 = c[1] & 7; this.page1 = c[2] & 7;
      this.page = this.page0; this.cmdBuf = [];
    } else if (c[0] === 0x81 && c.length >= 2) {
      this.cmdBuf = []; // contrast, ignored
    } else if (c[0] === 0x20 && c.length >= 2) {
      this.cmdBuf = []; // addressing mode, assume horizontal
    } else if (c.length === 1 && ![0x21, 0x22, 0x81, 0x20].includes(c[0])) {
      this.cmdBuf = []; // single-byte command, done
    }
  }

  data(b) {
    if (this.page <= this.page1 && this.col <= this.col1) {
      this.fb[this.page * W + this.col] = b;
    }
    this.col++;
    if (this.col > this.col1) {
      this.col = this.col0;
      this.page++;
      if (this.page > this.page1) this.page = this.page0;
    }
  }

  // Feed one I2C payload (post-address bytes): [control, args...] groups.
  feed(payload) {
    let i = 0;
    while (i < payload.length) {
      const ctrl = payload[i++];
      const isData = (ctrl & 0x40) !== 0;
      let j = i;
      if (ctrl & 0x80) {
        // continuation set: exactly one following byte belongs here
        j = i + 1;
      } else {
        j = payload.length;
      }
      for (; i < j && i < payload.length; i++) {
        if (isData) this.data(payload[i]);
        else this.command(payload[i]);
      }
    }
  }

  poll(cpu) {
    const w = this.wasm, P = this.peripheral;
    // EASYDMA path.
    let t = w.twim_take_txdma(P);
    if (t.length && t[0] === OLED_ADDR) {
      this.feed(cpu.mem_read(t[1], t[2]));
      w.twim_complete_txdma(P, []);
    } else if (t.length) {
      w.twim_complete_txdma(P, []);
    }
    t = w.twim_take_rxdma(P);
    if (t.length) {
      cpu.mem_write(t[1], new Uint8Array(t[2])); // status reads -> 0
      w.twim_complete_rxdma(P, t[2]);
    }
    // Byte path: group tap events by transaction, filter our address.
    const evs = w.i2c_take_events(P);
    let addr = null, buf = [];
    const flush = () => {
      if (addr === OLED_ADDR && buf.length) this.feed(buf);
      buf = [];
    };
    for (const e of evs) {
      if (e & 0x80000000) {
        if (e & 0x40000000) { flush(); addr = e & 0x7F; }
        else { flush(); addr = null; }
      } else if (addr !== null) buf.push(e & 0xFF);
    }
    if (addr === OLED_ADDR && buf.length) this.feed(buf);
    this.draw();
  }

  draw() {
    if (!this.ctx) return;
    const img = this.ctx.createImageData(W, H);
    for (let p = 0; p < 8; p++) {
      for (let c = 0; c < W; c++) {
        const byte = this.fb[p * W + c];
        for (let bit = 0; bit < 8; bit++) {
          const on = this.on && (this.allOn || (byte & (1 << bit)));
          const idx = ((p * 8 + bit) * W + c) * 4;
          const v = on ? 255 : 8;
          img.data[idx] = v * 0.6; img.data[idx + 1] = v; img.data[idx + 2] = v * 0.8;
          img.data[idx + 3] = 255;
        }
      }
    }
    this.ctx.putImageData(img, 0, 0);
  }

  clear() { this.fb.fill(0); this.draw(); }
}
