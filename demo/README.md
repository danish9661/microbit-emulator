# demo/ — browser front-end + virtual parts (micro:bit v2.2)

[![npm version](https://img.shields.io/npm/v/microbit-emu.svg)](https://www.npmjs.com/package/microbit-emu)
[![npm downloads](https://img.shields.io/npm/dm/microbit-emu.svg)](https://www.npmjs.com/package/microbit-emu)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![CI](https://github.com/danish9661/microbit-emulator/actions/workflows/publish.yml/badge.svg)](https://github.com/danish9661/microbit-emulator/actions/workflows/publish.yml)
[![Pages](https://github.com/danish9661/microbit-emulator/actions/workflows/pages.yml/badge.svg)](https://danish9661.github.io/microbit-emulator/)
[![Demo](https://img.shields.io/website?url=https%3A%2F%2Fdanish9661.github.io%2Fmicrobit-emulator%2F)](https://danish9661.github.io/microbit-emulator/)

**GitHub repo (all files):** https://github.com/danish9661/microbit-emulator —
every firmware preset (MakeCode arrows/smiley/scroll/rotation, BLE faces),
the interactive demo site, JS/TS examples, and the Rust source live there.
The npm tarball ships only the library + a MicroPython image (wokwi-style:
you supply firmware as input); grab anything else from the repo.
**Live demo:** https://danish9661.github.io/microbit-emulator/

## npm use (`microbit-emu`)

```bash
npm i microbit-emu
```

```js
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { bootEmulator } from "microbit-emu";

const require = createRequire(import.meta.url);
// Bundled MicroPython image (the build our test-suite boots); or pass
// any micro:bit v2 `.hex` you built yourself.
const hex = readFileSync(
  require.resolve("microbit-emu/firmware/micropython-microbit-v2.1.2.hex"),
  "utf8"
);
const emu = await bootEmulator(hex); // MicroPython REPL boots on the emulated nRF52833
for (let i = 0; i < 200; i++) emu.step(20000);
console.log(emu.lit()); // lit LED indices, e.g. []
```

`bootEmulator(hexText)` → `{ step(n), lit(), fault(), cpu }`; `wasm` (raw core),
`parseHex`, and sensor options (`accel`/`mag`) are exported for advanced use.

Pages: `index.html` (bench: loader + matrix + serial), `doc.html`
(docs + board/chip support matrices), `about.html` (scope + method).
One shared sheet: `bench.css` (load it on every page).

`index.html` is the loader + USB-serial replacement for the interface MCU:
drop a `.hex`/`.bin` (flash @ `0x0`, no SoftDevice), 5×5 matrix renders
from `P0/P1 OUT`, buttons drive `P0.14/P0.23`, UART box drains `UARTE0`,
and `pumpDma()` moves every staged EASYDMA transfer (UARTE/TWIM/SAADC/PDM)
plus RADIO loopback. I2C buses belong to virtual parts (below), not the
generic pump.

Build + serve (rebuild `pkg/` after Rust changes, then commit it — the
npm tarball ships the built wasm). Pinned reproducible recipe
(`npm run build:wasm`, versions in `demo/CHANGELOG.md`): `wasm-pack
0.14.0` + `wasm-bindgen` crate `0.2.126` (Cargo.lock) + `binaryen
version_132` wasm-opt (CI) + `node >= 22.6.0`:

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
npm run test:handshake  # node parts/handshake.mjs — 16 mock-consumer checks
  # against a local pkg build (npm run build:handshake first)
```

## npm package (`microbit-emu`)

Install the published core + virtual parts in any JS/TS project:

```bash
npm install microbit-emu
```

```js
import init, * as emu from 'microbit-emu';
await init();          // loads pkg/nrf52833_periph_wasm_bg.wasm
emu.reset_state();
emu.init();            // model live: peripherals, GPIO, NVIC
const cpu = new emu.WasmCpu(0x20020000, 0x20000001, 512 * 1024, 128 * 1024);
cpu.load_firmware(bytes, 0x0);
cpu.set_deliver_irqs(true);
// pump: cpu.step(N) + emu.tick_peripherals() (+ tick_n + wake on sleep)
```

The bench page shows the same snippet in its npm panel (copy button
included). The header pill + npm panel heading track the published
version (single source of truth: `demo/package.json`); the release
table lists published GitHub tags live.

Validate with `npm pack --dry-run` (note:
`pkg/.gitignore` is neutered on purpose — wasm-pack's default would
hide the built wasm from the tarball). Tarball identity must read
`microbit-emu-0.1.0.tgz` (37 files).

### Releasing a new version

Releases ship from the `publish.yml` workflow (manual trigger:
Actions → Publish to Registries → Run workflow, inputs: branch,
`x.y.z` version, release notes). It rebuilds the wasm from source,
bumps the version, runs the full gate (Rust suite + all JS faces +
REPL + 42/42 air E2E + 16/16 browser), publishes to npmjs.org
(`microbit-emu`) AND GitHub Packages (`@danish9661/microbit-emu`),
then cuts tag `v<version>` + a GitHub Release. Prereqs: repo secret
`NPM_TOKEN` (npmjs automation token); Pages keeps serving `demo/`
unchanged.
