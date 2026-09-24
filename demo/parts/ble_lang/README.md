# BLE face, every language we can reach

The SVC face (`src/sd_ble.rs`) is language-agnostic — SVC bytes are SVC
bytes — but each language below proves it independently:

| Language | File | How it runs | What it proves |
|---|---|---|---|
| C (GCC, Arduino-nRF52 toolchain) | `c_ble_face.c` -> `blinky/ble_fw/c_ble_face.bin` | `cargo test nrf_ble_c_face_markers` | ENABLE->CONNECT->CONNECTED(CENTRAL)->READ->RSP=87, markers `C:*` |
| C++ (xpack g++, same link script) | `blinky/ble_fw/ble_cpp_face.cpp` -> `blinky/ble_fw/ble_cpp_face.bin` | `cargo test nrf_ble_cpp_face_markers` | Same contract via C++ classes (per-SVC static methods — g++ reorders r3/ip moves around a shared dispatcher, so one svc#imm per method), markers `P:*`, bit-identical rebuild |
| C (GCC, full workout) | `blinky/ble_fw/ble_conformance.c` | `cargo test nrf_ble_conformance_svc_face` | 19 markers incl. L2CAP + pairing + multi-stage, 2nd-run clean |
| C (GCC, pairing image) | `blinky/ble_fw/ble_pairing_fw.c` | `cargo test nrf_ble_pairing_fw_markers` | CODAL-BLE-shaped JustWorks flow (ENABLE→GATTS table→CONNECT→GATT→AUTHENTICATE→CONN_SEC_GET→DISCONNECT), markers `BLEP:*`, 2nd-run clean |
| C (GCC, roles image) | `blinky/ble_fw/ble_roles_fw.c` | `cargo test nrf_ble_roles_fw_markers` | ADV/SCAN/whitelist/role-slot legs (21 `BLER:*` markers), 2nd-run clean, bit-identical rebuild |
| Arduino sketch (arduino-cli) | `blinky/ble_fw/arduino/ble_sketch.ino` | `arduino-cli compile --fqbn arduino:nrf52:primo` | Real Arduino build emits `svc 0x60/0xa0` (objdump) + ships the S132 thunk table (19 thunks ground-truth our numbers); binary itself does NOT boot here (nRF52832 bootloader layout, out of scope — recipe in `docs/README.md`) |
| MicroPython-idiom | `mpy_ble_face.py` + `run_mpy_face.mjs` | `node run_mpy_face.mjs` (also valid MicroPython) | connect->CONNECTED->read->87 in MPY idioms; the shipped MPY hex has NO bluetooth module (verified absent in flash, `MICROBIT_BLE_ENABLED=0`), so on-device MPY BLE waits on a BLE-enabled build |
| MicroPython runtime | `run_mpy_repl.mjs` | `node run_mpy_repl.mjs` (`npm run test:repl`) | Full-stock-hex boot (P16 recipe + bench-exact pump): 105B banner + `print(1+2)`->`3`, zero faults. Load-bearing: resets return to the APP table, UART log TAKE-accumulated |
| TypeScript (Node, strict types) | `ts_lang_face.mts` | `node --experimental-strip-types ts_lang_face.mts` (`npm run test:ts`) | BLE face (ENABLE->CONNECT->CONNECTED->READ->87) + RADIO face (TX take/complete/END + RX inject/complete/END) in strict TS, no `any` |
| JS (Node) | `run_js_face.mjs` | `node run_js_face.mjs` (`npm run test:js`) | Same BLE contract in JS idioms (ENABLE->CONNECT->CONNECTED->READ->87) |
| JS GPIO example | `gpio_js_example.mjs` | `node gpio_js_example.mjs` (`npm run test:js-example`) | Matrix "A" from JS idioms: MATRIX:OK, fault-free, pin-level render == `matrix_state()`, strobe coverage ≥10/25 |
| TS GPIO example | `gpio_ts_example.mts` | `node --experimental-strip-types gpio_ts_example.mts` (`npm run test:ts-example`) | Same matrix contract in strict TS, no `any` |
| Python-idiom (Node runner) | `run_py_face.mjs` (mirrors `mpy_ble_face.py`) | `node run_py_face.mjs` (`npm run test:py`) | Same BLE contract in Python idioms (CPython runs the `.py` directly too) |

MicroPython note: `import bluetooth` does not exist in
`micropython-microbit-v2.1.2.hex` (flash scan: zero hits). When a
BLE-enabled MPY build exists, its C port will emit exactly the SVC
bytes proven here — no model change needed.
