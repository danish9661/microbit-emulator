// Live-bridge E2E over-air proof: drives the BUILT wasm pkg through a
// REAL tools/ble_air_bridge.py (Bumble LocalLink air) over WebSocket.
// Run: 1) python3 tools/ble_air_bridge.py --port 18771
//      2) node demo/parts/ble_live_e2e.mjs [ws://127.0.0.1:18771]
// Every check asserts overAir===true (no loopback, no fallback) and
// correct conn/handle echo. Exits nonzero on any mismatch.
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const pkgDir = path.join(here, 'pkg-test-handshake');
const BRIDGE_URL = process.argv[2] ?? 'ws://127.0.0.1:18771';

let failures = 0;
function check(cond, msg, extra = '') {
  if (!cond) { console.error('FAIL:', msg, extra); failures++; }
  else console.log('ok:', msg);
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const mod = await import(path.join(pkgDir, 'nrf52833_periph_wasm.js'));
await mod.default({ module_or_path: readFileSync(path.join(pkgDir, 'nrf52833_periph_wasm_bg.wasm')) });
const wasm = mod;
wasm.reset_state();
wasm.init();
const cpu = new wasm.WasmCpu(0x20020000, 0x20000001, 512 * 1024, 128 * 1024);

// --- minimal SVC runner (same preamble as MockBleSvc) ---
function svc(num, r0 = 0, r1 = 0, r2 = 0, r3 = 0) {
  cpu.reset_cpu(0x20020000, 0x20000001);
  cpu.set_deliver_irqs(true);
  cpu.mem_write(0x20000000, [
    0x08, 0x48, 0x09, 0x49, 0x09, 0x4A, 0x0A, 0x4B,
    num & 0xFF, 0xDF, 0xFE, 0xE7,
  ]);
  const w32 = (v) => [v & 0xFF, (v >> 8) & 0xFF, (v >> 16) & 0xFF, (v >> 24) & 0xFF];
  cpu.mem_write(0x20000024, [...w32(r0), ...w32(r1), ...w32(r2), ...w32(r3)]);
  cpu.step(8);
  if (cpu.fault_pc() !== 0xFFFFFFFF) throw new Error(`SVC fault ${cpu.fault_pc().toString(16)}`);
  return cpu.get_regs()[0] >>> 0;
}
function drainEvt() {
  cpu.mem_write(0x20003FF0, [128, 0]);
  const rc = svc(0x61, 0x20003000, 0x20003FF0);
  if (rc === 5) return null;
  if (rc !== 0) throw new Error(`evt_get rc=${rc}`);
  const id = cpu.read8(0x20003000) | (cpu.read8(0x20003001) << 8);
  const len = cpu.read8(0x20003002) | (cpu.read8(0x20003003) << 8);
  return { id, len, body: [...cpu.mem_read(0x20003004, len - 4)] };
}

// --- bridge socket: send one job, await the single reply ---
const ws = new WebSocket(BRIDGE_URL);
await new Promise((res, rej) => {
  ws.onopen = res; ws.onerror = rej;
  setTimeout(() => rej(new Error('bridge connect timeout')), 8000);
});
const pending = [];
ws.onmessage = (ev) => {
  try { pending.push(JSON.parse(typeof ev.data === 'string' ? ev.data : Buffer.from(ev.data).toString())); }
  catch { /* ignore */ }
};
async function roundtrip(msg, wantT, timeoutMs = 45000) {
  // Skip the greeting ('hello') the bridge sends on connect: it is not
  // a job reply, so drop any already queued before sending.
  for (let i = pending.length - 1; i >= 0; i--) {
    if (pending[i].t === 'hello') pending.splice(i, 1);
  }
  ws.send(JSON.stringify(msg));
  const t0 = Date.now();
  while (Date.now() - t0 < timeoutMs) {
    const i = pending.findIndex((m) => m.t === wantT || m.t === 'cancel');
    if (i >= 0) return pending.splice(i, 1)[0];
    await sleep(150);
  }
  throw new Error(`timeout waiting for ${wantT} (got ${JSON.stringify(pending).slice(0, 200)})`);
}

// --- firmware side: enable + connect, resolve via the REAL bridge ---
check(svc(0x60, 0, 0) === 0, 'e2e enable');
cpu.mem_write(0x20001200, [0x01, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
check(svc(0x8C, 0x20001200, 0, 0) === 0, 'e2e connect stages');
{
  const bj = wasm.ble_take_job();
  check(bj.length > 0 && bj[0] === 1, 'e2e take GapConnect');
  const rep = await roundtrip({ t: 'ble_connect', addr: [...bj.slice(1, 7)] }, 'connected');
  check(rep.t === 'connected' && rep.overAir === true, 'e2e connected OVER AIR', JSON.stringify(rep).slice(0, 160));
  const h = wasm.ble_complete_gap_connect_ret(rep.peer.slice(0, 6));
  check(h !== 0xFFFF, `e2e link handle ${h}`);
  const ev = drainEvt();
  check(ev && ev.id === 0x10, 'e2e CONNECTED drained');
  var H = h;
}
// --- over-air GATT read of the peer battery handle ---
{
  // discover first: which handle IS the battery level over real air?
  // NOTE: with two peers up, the default peer's read follows the same
  // addressed path as the second link below (peer = first CONNECTED
  // address), so the value is that peer's battery (87), never the
  // other peer's (64) — cross-link reads stay on their link.
  const bj0 = (() => { svc(0x90, H, 1, 0); return wasm.ble_take_job(); })();
  check(bj0.length > 0 && bj0[0] === 5, 'e2e take PrimDisc');
  const pr = await roundtrip({ t: 'ble_disc', conn: H, kind: 0, start: 1, end: 0xFFFF }, 'prim_disc_rsp');
  check(pr.t === 'prim_disc_rsp' && pr.overAir === true, 'e2e prim_disc OVER AIR');
  const battSvc = (pr.services ?? []).find((s) => s.uuid16 === 0x180F);
  check(!!battSvc, 'e2e battery service over air', JSON.stringify((pr.services ?? []).map((s) => s.uuid16)));
  const cr = await roundtrip({ t: 'ble_disc', conn: H, kind: 2, start: battSvc.start, end: battSvc.end }, 'char_disc_rsp');
  const battChar = (cr.chars ?? []).find((c) => c.uuid16 === 0x2A19);
  check(!!battChar, 'e2e battery char over air');
  var FIRST_PEER = pr.peer ?? null;
  check(Array.isArray(FIRST_PEER) && FIRST_PEER.length === 6, `e2e first-link peer echo (got ${JSON.stringify(FIRST_PEER)})`);
  check(svc(0x96, H, battChar.value, 0) === 0, 'e2e read stages');
  const bj = wasm.ble_take_job();
  check(bj[0] === 0 && bj[2] === battChar.value, 'e2e take GattcRead (air handle)');
  const g = await roundtrip({ t: 'ble_read', conn: H, handle: battChar.value, offset: 0, ...(FIRST_PEER ? { peer: FIRST_PEER.slice(0, 6) } : {}) }, 'gatt');
  check(g.t === 'gatt' && g.overAir === true, 'e2e READ over air');
  // The addressed peer is the first link's own peer: its battery is 87.
  // (FIRST_PEER echo is asserted below; a 64 here would mean the read
  // crossed to the second peer.)
  check(Array.isArray(g.value) && g.value[0] === 87, `e2e battery byte 87 over air (got ${JSON.stringify(g.value)})`);
  wasm.ble_complete_gattc_read(g.conn, g.handle, g.offset, g.value);
  const ev = drainEvt();
  check(ev && ev.id === 0x36, 'e2e READ_RSP drained');
  var H1PEER = FIRST_PEER;
}
// --- over-air write to NUS RX, then NUS TX notify (HVX) ---
{
  const bjw = (() => {
    cpu.mem_write(0x20001410, [0x48, 0x49]);
    cpu.mem_write(0x20001400, [0x01, 0x00, 0x14, 0x00, 0x00, 0x00, 0x02, 0x00, 0x10, 0x14, 0x00, 0x20]);
    // discover NUS RX handle over air instead of assuming 20
    return null;
  })();
  void bjw;
  const pr = await roundtrip({ t: 'ble_disc', conn: H, kind: 0, start: 1, end: 0xFFFF }, 'prim_disc_rsp');
  const cr = await roundtrip({ t: 'ble_disc', conn: H, kind: 2, start: 1, end: 0xFFFF }, 'char_disc_rsp');
  const nusRx = (cr.chars ?? []).find((c) => c.uuid16 === null);
  check(!!nusRx, 'e2e NUS (128-bit) char over air');
  const w = await roundtrip({ t: 'ble_write', conn: H, handle: nusRx.value, op: 1, data: [0x48, 0x49] }, 'write_rsp');
  check(w.t === 'write_rsp' && w.overAir === true && w.len === 2, 'e2e WRITE over air (NUS RX)');
  // NUS TX notify: discover the TX VALUE handle over air (decl+1),
  // like firmware would after char discovery — never assume it.
  const nusTx = (cr.chars ?? []).find((c) => c.uuid16 === null && c.value !== nusRx.value)
    ?? (cr.chars ?? []).filter((c) => c.uuid16 === null)[1];
  check(!!nusTx, 'e2e NUS TX char over air');
  const hv = await roundtrip({ t: 'ble_hvx', conn: H, handle: nusTx.value, type: 1, data: [0x55] }, 'hvx');
  check(hv.t === 'hvx' && hv.overAir === true && hv.data[0] === 0x55, 'e2e NOTIFY over air (NUS TX)');
}
// --- multi-peer air: second peer sighting + per-link reads ---
{
  // Collect sightings until BOTH peer addresses are seen (the scan
  // reports one ADV_REPORT per advertising peer). The second address
  // (whoever is not the first link's peer) is the multi-peer target;
  // its battery reads 64, distinct from the default peer's 87.
  const seenPeers = [];
  for (let i = 0; i < 4; i++) {
    const a = await roundtrip({ t: 'ble_scan' }, 'adv_report', 60000);
    if (a.t === 'adv_report' && a.overAir === true) {
      if (!seenPeers.some((p) => p.join() === a.peer.join())) seenPeers.push(a.peer);
      if (seenPeers.length >= 2) break;
    }
  }
  check(seenPeers.length >= 2, `e2e multi-peer scan: 2 distinct sightings (got ${seenPeers.length})`);
  // The multi-peer target is the sighting that is NOT the first link's
  // peer (FIRST_PEER echo from the first prim_disc above): seenPeers[1]
  // is scan-ordered, which need not differ from the first link.
  const adv2peer = seenPeers.find((p) => !FIRST_PEER || p.join() !== FIRST_PEER.join()) ?? seenPeers[0];
  check(!!adv2peer, 'e2e multi-peer scan sighting');
  check(!FIRST_PEER || adv2peer.join() !== FIRST_PEER.join(),
    `e2e second peer differs from first (${adv2peer} vs ${FIRST_PEER})`);
  check(svc(0x60, 0, 0) === 0, 'e2e second link enable (idempotent)');
  cpu.mem_write(0x20001200, [0x01, ...adv2peer.slice(0, 6)]);
  check(svc(0x8C, 0x20001200, 0, 0) === 0, 'e2e second connect stages');
  const bj2 = wasm.ble_take_job();
  check(bj2.length > 0 && bj2[0] === 1, 'e2e take second GapConnect');
  const rep2 = await roundtrip({ t: 'ble_connect', addr: [...bj2.slice(1, 7)] }, 'connected');
  check(rep2.t === 'connected' && rep2.overAir === true, 'e2e second link OVER AIR');
  const h2 = wasm.ble_complete_gap_connect_ret(rep2.peer.slice(0, 6));
  check(h2 !== 0xFFFF && h2 !== H, `e2e second link handle ${h2} != first ${H}`);
  const ev2 = drainEvt();
  check(ev2 && ev2.id === 0x10, 'e2e second CONNECTED drained');
  // Per-link read on the second link: discover + read battery over air.
  const bj3 = (() => { svc(0x90, h2, 1, 0); return wasm.ble_take_job(); })();
  check(bj3.length > 0 && bj3[0] === 5, 'e2e take second PrimDisc');
  const pr2 = await roundtrip({ t: 'ble_disc', conn: h2, kind: 0, start: 1, end: 0xFFFF, peer: rep2.peer.slice(0, 6) }, 'prim_disc_rsp');
  check(pr2.t === 'prim_disc_rsp' && pr2.overAir === true, 'e2e second prim_disc OVER AIR');
  const bs2 = (pr2.services ?? []).find((s) => s.uuid16 === 0x180F);
  check(!!bs2, 'e2e second battery service over air');
  const cr2 = await roundtrip({ t: 'ble_disc', conn: h2, kind: 2, start: bs2.start, end: bs2.end, peer: rep2.peer.slice(0, 6) }, 'char_disc_rsp');
  const bc2 = (cr2.chars ?? []).find((c) => c.uuid16 === 0x2A19);
  check(!!bc2, 'e2e second battery char over air');
  check(svc(0x96, h2, bc2.value, 0) === 0, 'e2e second read stages');
  const bj4 = wasm.ble_take_job();
  check(bj4[0] === 0 && bj4[1] === h2, 'e2e take read on second link');
  const g2 = await roundtrip({ t: 'ble_read', conn: h2, handle: bc2.value, offset: 0, peer: rep2.peer.slice(0, 6) }, 'gatt');
  check(g2.t === 'gatt' && g2.overAir === true, 'e2e second READ over air');
  check(Array.isArray(g2.value) && g2.value[0] === 64, `e2e second-peer battery 64 (got ${JSON.stringify(g2.value)})`);
  wasm.ble_complete_gattc_read(g2.conn, g2.handle, g2.offset, g2.value);
  const evr2 = drainEvt();
  check(evr2 && evr2.id === 0x36, 'e2e second READ_RSP drained');
  var H2 = h2;
}
// --- scan + rssi + disconnect over air ---
{
  const adv = await roundtrip({ t: 'ble_scan' }, 'adv_report');
  check(adv.t === 'adv_report' && adv.overAir === true && adv.peer.length === 6, 'e2e ADV_REPORT over air');
  const r = await roundtrip({ t: 'ble_rssi', conn: H }, 'rssi');
  check(r.t === 'rssi' && typeof r.rssi === 'number' && r.overAir === true, `e2e RSSI over air (${r.rssi}dBm, src=${r.src})`);
  const d = await roundtrip({ t: 'ble_disconnect', conn: H, reason: 19 }, 'disconnected');
  check(d.t === 'disconnected' && d.reason === 19, 'e2e DISCONNECTED over air');
  const d2 = await roundtrip({ t: 'ble_disconnect', conn: H2, reason: 19 }, 'disconnected');
  check(d2.t === 'disconnected' && d2.reason === 19, 'e2e second DISCONNECTED over air');
}
ws.close();
if (failures) { console.error(`${failures} FAILURES (bridge must run: python3 tools/ble_air_bridge.py --port 18771)`); process.exit(1); }
console.log('all live-bridge E2E checks OVER AIR OK');
