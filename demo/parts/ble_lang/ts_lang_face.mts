// TypeScript-language BLE + RADIO face test (runs on Node against the
// built wasm pkg; also valid TypeScript — strict types, no any).
// Proves the SVC face + RADIO registers from TS idioms: ENABLE ->
// CONNECT -> CONNECTED(CENTRAL) -> READ -> READ_RSP=87, plus RADIO
// TXEN/START take -> complete TX -> RXEN/START take -> inject ->
// complete RX -> END. Same contract as the C/C++/MPY/JS runners.
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here: string = path.dirname(fileURLToPath(import.meta.url));
const pkgDir: string = path.join(here, "..", "pkg-test-handshake");

type WasmMod = typeof import("../pkg-test-handshake/nrf52833_periph_wasm.js");
const mod = (await import(
  path.join(pkgDir, "nrf52833_periph_wasm.js")
)) as WasmMod;
await mod.default({
  module_or_path: readFileSync(path.join(pkgDir, "nrf52833_periph_wasm_bg.wasm")),
});
const wasm = mod as unknown as Record<string, (...a: never[]) => number> & {
  WasmCpu: new (sp: number, pc: number, flash: number, ram: number) => {
    reset_cpu(sp: number, pc: number): void;
    set_deliver_irqs(v: boolean): void;
    mem_write(addr: number, bytes: number[]): void;
    mem_read(addr: number, len: number): Uint8Array;
    step(n: number): void;
    fault_pc(): number;
    get_regs(): number[];
  };
  reset_state(): void;
  init(): void;
};

wasm.reset_state();
wasm.init();
const cpu = new wasm.WasmCpu(0x20020000, 0x20000001, 512 * 1024, 128 * 1024);
const log: boolean[] = [];
const check = (c: boolean, m: string): void => {
  log.push(c);
  console.log((c ? "ok: " : "FAIL: ") + m);
};
function svc(num: number, r0 = 0, r1 = 0, r2 = 0): number {
  cpu.reset_cpu(0x20020000, 0x20000001);
  cpu.set_deliver_irqs(true);
  cpu.mem_write(0x20000000, [0x08, 0x48, 0x09, 0x49, 0x09, 0x4a, num & 0xff, 0xdf, 0xfe, 0xe7]);
  const b: Uint8Array = new Uint8Array(new Uint32Array([r0, r1, r2, 0]).buffer);
  cpu.mem_write(0x20000024, [...b]);
  cpu.step(8);
  if (cpu.fault_pc() !== 0xffffffff) throw new Error("svc fault");
  return cpu.get_regs()[0] >>> 0;
}
const le16 = (a: number[], o: number): number => a[o] | (a[o + 1] << 8);

// BLE face (typed SVC bytes).
check(svc(0x60, 0, 0) === 0, "ts enable");
cpu.mem_write(0x20001200, [1, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
check(svc(0x8c, 0x20001200, 0) === 0, "ts connect stages");
const wasmAny = wasm as unknown as {
  ble_take_job(): number[];
  ble_complete_gap_connect(p: number[]): void;
  ble_complete_gattc_read(c: number, h: number, o: number, d: number[]): void;
};
const job: number[] = wasmAny.ble_take_job();
check(job.length > 0 && job[0] === 1, "ts take GapConnect");
wasmAny.ble_complete_gap_connect([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
cpu.mem_write(0x20003ff0, [128, 0]);
check(svc(0x61, 0x20003000, 0x20003ff0) === 0, "ts evt_get");
const evt: number[] = [...cpu.mem_read(0x20003000, 32)];
check(le16(evt, 0) === 0x10, "ts CONNECTED id");
check(evt[4 + 16] === 2, "ts CENTRAL role");
check(svc(0x96, 1, 0x13, 0) === 0, "ts read stages");
wasmAny.ble_complete_gattc_read(1, 0x13, 0, [87]);
cpu.mem_write(0x20003ff0, [128, 0]);
check(svc(0x61, 0x20003000, 0x20003ff0) === 0, "ts evt_get read");
const rr: number[] = [...cpu.mem_read(0x20003000, 17)];
check(le16(rr, 0) === 0x36 && rr[rr.length - 1] === 87, "ts READ_RSP=87");

// RADIO face (typed register pokes via periph exports).
const radio = wasm as unknown as {
  periph_write(addr: number, w: number, v: number): void;
  periph_read(addr: number, w: number): number;
  radio_take_tx(): number[];
  radio_complete_tx(): void;
  radio_take_rx(): number[];
  radio_inject_rx(b: number[]): void;
  radio_complete_rx(): void;
};
radio.periph_write(0x40001504, 4, 0x20001200); // PACKETPTR
radio.periph_write(0x40001000, 4, 1); // TXEN
radio.periph_write(0x40001008, 4, 1); // START (Tx)
const tx: number[] = radio.radio_take_tx();
check(tx.length === 2, "ts radio TX staged");
radio.radio_complete_tx();
check(radio.periph_read(0x4000110c, 4) === 1, "ts radio END");
radio.periph_write(0x40001004, 4, 1); // RXEN
radio.periph_write(0x40001008, 4, 1); // START (Rx)
radio.radio_inject_rx([0xde, 0xad]);
const rx: number[] = radio.radio_take_rx();
check(rx.length === 1, "ts radio RX staged");
radio.radio_complete_rx();
check(radio.periph_read(0x4000110c, 4) === 1, "ts radio RX END");

if (log.some((c: boolean) => !c)) process.exit(1);
console.log("ts BLE+RADIO face OK (TypeScript-typed contract)");
