# demo/ — browser front-end + virtual parts (micro:bit v2.2)

`index.html` is the loader + USB-serial replacement for the interface MCU:
drop a `.hex`/`.bin` (flash @ `0x0`, no SoftDevice), 5×5 matrix renders
from `P0/P1 OUT`, buttons drive `P0.14/P0.23`, UART box drains `UARTE0`,
and `pumpDma()` moves every staged EASYDMA transfer (UARTE/TWIM/SAADC/PDM)
plus RADIO loopback. I2C buses belong to virtual parts (below), not the
generic pump.

Build + serve (rebuild `pkg/` after Rust changes, then commit it — the
npm tarball ships the built wasm):

```bash
~/.cargo/bin/wasm-pack build nrf52833-periph-wasm --target web --out-dir ../demo/pkg
cd demo && python3 -m http.server 8080
# open http://localhost:8080 (file:// won't load the wasm module)
```

Try first: `blinky/blinky_nrf.bin` (BOOT/BLINK + ROW1 LED).

## parts/ — virtual hardware (`API.md` is the contract)

- `pins.js` — edge-connector map (`P0`–`P20`), matrix rows/cols,
  internal pins, LSM303 addresses.
- `lsm303.js` — LSM303AGR accel+mag on TWIM1: WHO_AM_I, CTRL echo,
  STATUS DRDY, live tilt simulation (`setAccel/setMag/shake`).
  `register()` before `wasm.init()`, `poll(cpu)` every frame.
- `ssd1306.js` — 128×64 OLED on TWIM0 (addr `0x3C`): command set,
  GDDRAM framebuffer, canvas render. Same register/poll protocol.

Verify without a browser:

```bash
npm run test:parts   # node parts/smoke.mjs — 17 checks, nonzero exit on fail
```

## npm package

`package.json` (`microbit-v2-emulator`) ships `pkg/ + parts/ +
index.html + API.md`. Validate with `npm pack --dry-run` (note:
`pkg/.gitignore` is neutered on purpose — wasm-pack's default would
hide the built wasm from the tarball).
