// ST7789 240x240 edge-SPI display part (micro:bit P13-P16 SPI add-on).
// Speaks the SPI tap + SPIM DMA driver API (see ../API.md): register()
// BEFORE wasm.init(), poll(cpu, canvas) every frame. Renders a 240x240
// RGB565 framebuffer like the SSD1306 part renders GDDRAM.
//
// Protocol (ST7789, SPI mode 0, DC level = cmd/data):
//   CASET 0x2A [x0hi x0lo x1hi x1lo], RASET 0x2B [y0hi y0lo y1hi y1lo],
//   RAMWR 0x2C + RGB565 BE pixels, SWRESET 0x01 (clear), DISPON 0x29,
//   DISPOFF 0x28, MADCTL 0x36 (ignored), COLMOD 0x3A (assume 16-bit).
// MISO: display-ID read answers [0x04, 0x85, 0x52] (same as the
// handshake mock, so firmware ID probes pass).
const W = 240, H = 240;

export class SpiDisplay {
  constructor(wasm, peripheral = 'SPIM2', canvas = null, cs = 'P0.12', dc = 'P0.11') {
    this.wasm = wasm;
    this.peripheral = peripheral;
    this.canvas = canvas;
    this.cs = cs;
    this.dc = dc;
    this.fb = new Uint16Array(W * H);
    this.x0 = 0; this.x1 = W - 1;
    this.y0 = 0; this.y1 = H - 1;
    this.x = 0; this.y = 0;
    this.on = false;
    if (canvas) {
      canvas.width = W; canvas.height = H;
      this.ctx = canvas.getContext('2d');
    } else {
      this.ctx = null;
    }
    this.cmd = null;
    this.cmdBuf = [];
  }

  register() {
    this.wasm.spi_tap(this.peripheral, this.cs, this.dc);
  }

  command(b) {
    // A new command byte ends any pending arg run.
    if (b === 0x01) { this.fb.fill(0); this.cmd = null; this.cmdBuf = []; return; } // SWRESET
    if (b === 0x28) { this.on = false; this.cmd = null; this.cmdBuf = []; return; } // DISPOFF
    if (b === 0x29) { this.on = true; this.cmd = null; this.cmdBuf = []; return; } // DISPON
    if (b === 0x2C) { this.cmd = 0x2C; this.x = this.x0; this.y = this.y0; this.cmdBuf = []; return; } // RAMWR
    this.cmd = b; this.cmdBuf = [];
  }

  arg(b) {
    if (this.cmd === 0x2C) { this.pixel(b); return; } // RAMWR data = pixels
    this.cmdBuf.push(b);
    if (this.cmd === 0x2A && this.cmdBuf.length >= 4) { // CASET
      this.x0 = (this.cmdBuf[0] << 8) | this.cmdBuf[1];
      this.x1 = (this.cmdBuf[2] << 8) | this.cmdBuf[3];
      this.cmdBuf = [];
    } else if (this.cmd === 0x2B && this.cmdBuf.length >= 4) { // RASET
      this.y0 = (this.cmdBuf[0] << 8) | this.cmdBuf[1];
      this.y1 = (this.cmdBuf[2] << 8) | this.cmdBuf[3];
      this.cmdBuf = [];
    } else if (this.cmdBuf.length >= 4) {
      this.cmdBuf = []; // unknown multi-arg command: drop
    }
  }

  // RAMWR data arrives byte-wise; two bytes = one BE RGB565 pixel.
  pixel(b) {
    if (this._hi === undefined) { this._hi = b; return; }
    const v = (this._hi << 8) | b;
    this._hi = undefined;
    if (this.x <= this.x1 && this.y <= this.y1 && this.x < W && this.y < H) {
      this.fb[this.y * W + this.x] = v;
    }
    this.x++;
    if (this.x > this.x1) {
      this.x = this.x0;
      this.y++;
      if (this.y > this.y1) this.y = this.y0;
    }
  }

  // Feed tap events: bit29 = DC level (1 data, 0 cmd), low byte = value.
  // CS edges (bit31) are ordering only.
  feed(evs) {
    for (const e of evs) {
      if (e & 0x80000000) continue;
      const dc = (e >> 29) & 1;
      if (dc) this.arg(e & 0xFF);
      else this.command(e & 0xFF);
    }
  }

  poll(cpu) {
    const w = this.wasm, P = this.peripheral;
    // EASYDMA path: staged SPIM TX frames land here; completion routes
    // bytes to the SPI tap (CS/DC handled model-side).
    let t = w.twim_take_txdma(P);
    if (t.length) {
      const bytes = cpu.mem_read(t[1], t[2]);
      w.twim_complete_txdma(P, bytes);
    }
    t = w.twim_take_rxdma(P);
    if (t.length) {
      // Display-ID read: answer MISO like the handshake mock.
      cpu.mem_write(t[1], [0x04, 0x85, 0x52].slice(0, t[2]));
      w.twim_complete_rxdma(P, t[2]);
    }
    this.feed(w.spi_take_events(P));
    // MISO answers polled reads too.
    w.spi_push_miso(P, [0x04, 0x85, 0x52]);
    this.draw();
  }

  draw() {
    if (!this.ctx) return;
    const img = this.ctx.createImageData(W, H);
    for (let i = 0; i < W * H; i++) {
      const v = this.on ? this.fb[i] : 0;
      const o = i * 4;
      img.data[o] = ((v >> 11) & 0x1F) * 255 / 31;
      img.data[o + 1] = ((v >> 5) & 0x3F) * 255 / 63;
      img.data[o + 2] = (v & 0x1F) * 255 / 31;
      img.data[o + 3] = 255;
    }
    this.ctx.putImageData(img, 0, 0);
  }

  clear() { this.fb.fill(0); this.draw(); }
}
