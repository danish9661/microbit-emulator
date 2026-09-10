# demo/ — browser front-end (micro:bit v2.2)

`index.html` is the loader + USB-serial replacement for the interface MCU:
drop a `.hex`/`.bin` (flash @ `0x0`, no SoftDevice), 5×5 matrix renders
from `P0/P1 OUT`, buttons drive `P0.14/P0.23`, UART box drains `UARTE0`,
and `pumpDma()` moves every staged EASYDMA transfer (UARTE/TWIM/SAADC/PDM)
plus RADIO loopback.

Build + serve (pkg/ is generated, not committed):

```bash
~/.cargo/bin/wasm-pack build nrf52833-periph-wasm --target web --out-dir ../demo/pkg
cd demo && python3 -m http.server 8080
# open http://localhost:8080 (file:// won't load the wasm module)
```

Try first: `blinky/blinky_nrf.bin` (BOOT/BLINK + ROW1 LED).
