// Driver-side mock consumers for every handshake (H) surface.
// Same poll() contract as the demo parts: register() BEFORE wasm.init(),
// poll(cpu) every frame. Run headless: node parts/handshake.mjs
// drives each mock against the REAL wasm core (built pkg) and fails
// loudly (nonzero exit) on mismatch — the executable proof each
// handshake model answers a live consumer, not just unit registers.
//
// What each mock proves:
//   TEMP    driver C value -> START -> DATARDY -> TEMP quarter-degree read
//   COMP    driver mV vs TH band -> SAMPLE -> Below/Above + UP/DOWN/CROSS
//   QDEC    host steps + Gray edges -> ACC + report/double-read + STOPPED
//   ECB     STARTECB stages job -> driver AES-128s in place -> ENDECB
//   AAR     START stages job -> driver resolves -> END + RESOLVED
//   CCM     KSGEN->ENDKSGEN, CRYPT stages job -> driver CTR+MIC -> ENDCRYPT
//   I2S     START stages RX+TX -> driver silence/capture -> STOPPED
//   NFCT    SENSE->READY, field->FIELDDETECTED, ACTIVATE->SELECTED,
//           STARTTX/ENABLERXDATA frames, FIELDLOST parks
//   TWIS    master write lands in RAM + RXSTARTED/WRITE/STOPPED; read back
//           + ORC pad; wrong address DNACKs
//   SPIS    ACQUIRE + buffers -> exchange returns RAM MISO + END/ENDRX
//   SPI tap edge display (ST7789-style): CS bytes queue with DC level,
//           MISO answers reads
//   SAADC lim: driver sample vs window raises LIMITH/LIMITL
//   USBD setup: host SETUP packet -> EP0SETUP + setup regs readable
//   RADIO 15.4: corrupt packet -> CRCERROR + END (ED/CCA via driver dBm)
//   QSPI backend: staged write/read/erase round trip through the driver
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import {
  MockTemp, MockComp, MockQdec, MockEcb, MockAar, MockCcm, MockI2s,
  MockNfct, MockTwis, MockSpis, MockSpiDisplay, MockSaadcLim,
  MockUsbdSetup, MockRadio154, MockRadioAir, MockBleSvc, MockQspi,
} from './mocks.js';

const here = path.dirname(fileURLToPath(import.meta.url));
const pkgDir = path.join(here, 'pkg-test-handshake');
const wasmUrl = path.join(pkgDir, 'nrf52833_periph_wasm.js');

let failures = 0;
function check(cond, msg) {
  if (!cond) { console.error('FAIL:', msg); failures++; }
  else console.log('ok:', msg);
}

let wasm;
try {
  const mod = await import(wasmUrl);
  const { readFileSync: rf } = await import('node:fs');
  const bytes = rf(path.join(pkgDir, 'nrf52833_periph_wasm_bg.wasm'));
  await mod.default({ module_or_path: bytes });
  wasm = mod;
} catch (e) {
  console.error('FAIL: load built pkg from', pkgDir, '-', e.message);
  console.error('hint: wasm-pack build nrf52833-periph-wasm --target web --out-dir demo/parts/pkg-test-handshake');
  process.exit(1);
}

// Mock CPU RAM (64KB is plenty for the staged buffers used here).
// NOTE: TWIS/SPIS/RADIO/QSPI/NFCT mocks use cpu.mem_* for the SAME
// addresses the model reads (0x2000xxxx), so the mock RAM must BE the
// model's RAM. mem_* route through a WasmCpu instance — but it must be
// created AFTER wasm.init() (periph_write before init hits no map).
let boardCpu = null;
function mockCpu() {
  const ram = new Uint8Array(65536);
  return {
    mem_read: (ptr, len) => boardCpu.mem_read(ptr, len),
    mem_write: (ptr, bytes) => boardCpu.mem_write(ptr, bytes),
    twis_master_write: (base, addr, data) => boardCpu.twis_master_write(base, addr, data),
    twis_master_read: (base, addr, len) => [...boardCpu.twis_master_read(base, addr, len)],
    spis_exchange: (base, mosi) => [...boardCpu.spis_exchange(base, mosi)],
    _ram: ram,
  };
}

function freshBoard(extraParts = []) {
  wasm.reset_state();
  const parts = [
    new MockTemp(wasm, 27),
    new MockComp(wasm, 100),
    new MockQdec(wasm),
    new MockEcb(wasm),
    new MockAar(wasm, true),
    new MockCcm(wasm),
    new MockI2s(wasm),
    new MockNfct(wasm),
    new MockSpiDisplay(wasm, 'SPIM2', 'P0.12', 'P0.11'),
    new MockSaadcLim(wasm),
    new MockUsbdSetup(wasm),
    new MockRadio154(wasm),
    new MockQspi(wasm),
    ...extraParts,
  ];
  for (const p of parts) p.register();
  wasm.init();
  boardCpu = new wasm.WasmCpu(0x20020000, 0x100, 512 * 1024, 128 * 1024);
  return parts;
}

function cpuWithBridge() {
  return mockCpu();
}

// --- TEMP: 27C -> DATARDY + 108 quarters ---
{
  const parts = freshBoard();
  const cpu = cpuWithBridge();
  const t = parts[0];
  for (let i = 0; i < 3 && t.last?.celsius !== 27; i++) t.poll(cpu);
  check(t.last?.ready === true && t.last?.celsius === 27, `TEMP 27C DATARDY (got ${JSON.stringify(t.last)})`);
}

// --- COMP: below -> Below, above -> Above + UP + CROSS ---
{
  const parts = freshBoard();
  const cpu = cpuWithBridge();
  const c = parts[1];
  c.setMv(100); c.poll(cpu);
  check(c.last?.result === 0, `COMP Below at 100mV (got ${JSON.stringify(c.last)})`);
  c.setMv(3200); c.poll(cpu);
  check(c.last?.result === 1 && c.last?.up === 1 && c.last?.cross === 1,
    `COMP Above+UP+CROSS at 3200mV (got ${JSON.stringify(c.last)})`);
}

// --- QDEC: host steps accumulate; snapshot holds while ACC moves ---
{
  const parts = freshBoard();
  const cpu = cpuWithBridge();
  const q = parts[2];
  wasm.qdec_step(5); wasm.qdec_step(-2);
  q.poll(cpu);
  check(q.acc === 3 && q.snap === 3, `QDEC ACC=+3 snapshot (acc=${q.acc} snap=${q.snap})`);
}

// --- ECB: FIPS-197 vector encrypts in place + ENDECB ---
{
  const parts = freshBoard();
  const cpu = cpuWithBridge();
  const e = parts[3];
  for (let i = 0; i < 3 && !e.done; i++) e.poll(cpu);
  check(e.done === true, 'ECB FIPS-197 block + ENDECB');
}

// --- AAR: START stages job, driver resolves ---
{
  const parts = freshBoard();
  const cpu = cpuWithBridge();
  const a = parts[4];
  for (let i = 0; i < 3 && !a.done; i++) a.poll(cpu);
  check(a.done === true, 'AAR END + RESOLVED');
}

// --- CCM: KSGEN + CRYPT round trip, MIC verifies ---
{
  const parts = freshBoard();
  const cpu = cpuWithBridge();
  const c = parts[5];
  for (let i = 0; i < 4 && !c.done; i++) c.poll(cpu);
  check(c.done === true, 'CCM CTR+MIC roundtrip + ENDCRYPT');
}

// --- I2S: RX silence + TX capture + STOPPED ---
{
  const parts = freshBoard();
  const cpu = cpuWithBridge();
  const s = parts[6];
  for (let i = 0; i < 3 && !s.done; i++) s.poll(cpu);
  check(s.done === true, 'I2S RX/TX staged + capture + STOPPED');
}

// --- NFCT: field -> select -> TX/RX frames -> field lost ---
{
  const parts = freshBoard();
  const cpu = cpuWithBridge();
  const n = parts[7];
  for (let i = 0; i < 6 && !n.done; i++) n.poll(cpu);
  check(n.done === true, 'NFCT select + TX/RX frames + FIELDLOST');
}

// --- SPI tap display: DMA bytes route to the SPI tap + MISO answers ---
{
  const parts = freshBoard();
  const cpu = cpuWithBridge();
  const d = parts[8];
  for (let i = 0; i < 4 && !d.done; i++) d.poll(cpu);
  check(d.done === true && d.cmds.length + d.data.length >= 3,
    `SPI tap DMA->events+MISO (cmds=${d.cmds.length} data=${d.data.length})`);
}

// --- SAADC limits: inside quiet, high->LIMITH, low->LIMITL ---
{
  const parts = freshBoard();
  const cpu = cpuWithBridge();
  const s = parts[9];
  for (let i = 0; i < 3 && !s.done; i++) s.poll(cpu);
  check(s.done === true, 'SAADC LIMITH + LIMITL edges');
}

// --- USBD SETUP: host packet readable + EP0SETUP ---
{
  const parts = freshBoard();
  const cpu = cpuWithBridge();
  const u = parts[10];
  for (let i = 0; i < 3 && !u.done; i++) u.poll(cpu);
  check(u.done === true, 'USBD SETUP regs + EP0SETUP');
}

// --- RADIO 15.4: ED/CCA + DEVMATCH/MISS + MHR/FRAMESTART + CRCERROR ---
{
  const parts = freshBoard();
  const cpu = cpuWithBridge();
  const r = parts[11];
  for (let i = 0; i < 10 && !r.done; i++) r.poll(cpu);
  check(r.done === true && r.seen?.edCca && r.seen?.match && r.seen?.miss && r.seen?.corrupt,
    `RADIO ED/CCA+match/miss+MHR/FRAMESTART+CRCERROR (seen=${JSON.stringify(r.seen)})`);
}

// --- RADIO air peer (stub bridge): TX posts, addressed echo returns ---
// The live bridge is tools/ble_air_bridge.py over WebSocket (BleAir in
// ble_air.js, wired on the bench). Headless, a 20-line in-memory stub
// proves the same pump contract: TX bytes leave via sendTx, an
// addressed frame comes back via takeRx, DEVMATCH fires on completion.
{
  const parts = freshBoard();
  const cpu = cpuWithBridge();
  const { MockRadioAir } = await import('./mocks.js');
  const inbox = [];
  const stub = {
    available: true,
    sendTx(bytes) { inbox.push(bytes); return true; },
    takeRx() {
      const b = inbox.shift();
      // Bridge echo shape: [0xEF, 0xBE, ...payload]. DAB[0]=0xEF matches.
      return b ? Uint8Array.from([0xEF, 0xBE, ...b.slice(0, 8)]) : null;
    },
  };
  const air = new MockRadioAir(wasm, stub);
  for (let i = 0; i < 6 && !air.done; i++) air.poll(cpu);
  check(air.done === true && air.seenTx >= 1 && air.seenRx >= 1,
    `RADIO air peer TX->bridge->RX (tx=${air.seenTx} rx=${air.seenRx})`);
}

// --- SoftDevice BLE SVC face: full GATT flow through REAL SVC bytes ---
// The mock executes actual `svc` instructions on its own WasmCpu
// (enable, GATTS table build, connect, discovery, read, write, scan,
// RSSI, disconnect) and drains every event via evt_get — the stub
// bridge resolves staged jobs locally (battery 87 + fixed table,
// mirroring pumpBleLoopback). Needs no live bridge headless.
{
  freshBoard();
  const svcCpu = new wasm.WasmCpu(0x20020000, 0x20000001, 512 * 1024, 128 * 1024);
  const b = new MockBleSvc(wasm);
  try {
    for (let i = 0; i < 4 && !b.done; i++) b.poll(svcCpu);
  } catch (e) {
    console.error('FAIL: BLE SVC exception:', e.message);
  }
  check(b.done === true && b.seen?.gatts && b.seen?.connected && b.seen?.readRsp && b.seen?.writeRsp && b.seen?.full,
    `BLE SVC full flow enable->disc->read->write->rssi->disc (seen=${JSON.stringify(b.seen)})`);
}

// --- QSPI: staged write/read/erase round trip ---
{
  const parts = freshBoard();
  const cpu = cpuWithBridge();
  const q = parts[12];
  for (let i = 0; i < 8 && !q.done; i++) q.poll(cpu);
  check(q.done === true, 'QSPI write/read/erase roundtrip');
}

// --- TWIS + SPIS host engines (need the bridge CPU) ---
{
  freshBoard();
  const cpu = cpuWithBridge();
  const tw = new (await import('./mocks.js').then(m => m.MockTwis))(wasm);
  for (let i = 0; i < 4 && !tw.done; i++) tw.poll(cpu);
  check(tw.done === true, 'TWIS write->RAM + read->MISO + STOPPED');
  const sp = new (await import('./mocks.js').then(m => m.MockSpis))(wasm);
  for (let i = 0; i < 4 && !sp.done; i++) sp.poll(cpu);
  check(sp.done === true, 'SPIS ACQUIRE + exchange END/ENDRX');
}

if (failures) { console.error(`${failures} FAILURES`); process.exit(1); }
console.log('all handshake mocks OK');
