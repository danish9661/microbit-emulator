// BLE air peer over Bumble (Google's Python BLE stack) via WebSocket.
//
// The emulator core models the nRF52833 RADIO registers (bare-metal
// 802.15.4-ish: TXEN/RXEN/START, PACKETPTR, SHORTS, RSSI/ED/CCA,
// DEVMATCH, CRCERROR...). It cannot speak BLE link-layer itself —
// no SoftDevice, no GAP/GATT, no pairing. This part bridges that gap
// the same way every other part does: firmware stages a transfer,
// the driver moves bytes, then completes it.
//
// Architecture (driver-side, zero model changes):
//   TX: radio_take_tx() -> [ptr,len] -> mem_read bytes ->
//       POST bytes to the Bumble air bridge -> radio_complete_tx()
//   RX: GET pending air frame from the bridge -> mem_write to staged
//       radio_take_rx() ptr -> radio_complete_rx()
//
// The bridge is a small local Python process (see tools/ble_air_bridge.py):
// a Bumble virtual controller pair on one LocalLink — one end is the
// "air" the emulator talks to, the other end runs a real Bumble Device
// (GATT server, scanner, initiator). Transport is plain WebSocket with
// tiny JSON frames ({t:'tx'|'rx'|'...'}), so the browser page needs no
// HCI knowledge at all.
//
// Why Bumble and not the alternatives:
//   - BabbleSim: physical-layer simulator for Zephyr bsim images. Needs
//     Zephyr-built Linux executables + its own 2G4 PHY process; it
//     simulates RF between Zephyr boards, not HCI for an external host.
//     Wrong layer for a WASM demo that needs a GATT peer over a socket.
//   - TrouBLE (embassy-rs): a no_std Rust BLE *host* for firmware. It
//     would replace firmware-side code, not give the demo page a peer
//     to talk to. Nothing to connect it to from JavaScript.
//   - BlueZ btvirt: kernel VHCI virtual controllers. Needs Linux +
//     root + BlueZ; invisible to a browser page. Great for host-stack
//     CI, useless as a demo-page air peer.
//   - Bumble: pure Python, pip-installable, virtual Controller + Host +
//     Device in-process, TCP *and* WebSocket HCI transports, GATT
//     client+server, advertising/scanning proven in our spikes
//     (link smoke + GATT battery read, /tmp/opencode/*.py).
//
// Status display contract: this part owns one status line element id;
// the bench section wires it. `available` flips true on first good
// bridge round trip; frames counters prove air is moving, not looped.

const RECONNECT_MS = 3000;

export class BleAir {
  constructor(statusEl = null, url = 'ws://127.0.0.1:8765') {
    this.statusEl = statusEl;
    this.url = url;
    this.ws = null;
    this.available = false;
    this.txFrames = 0;
    this.rxFrames = 0;
    this.wantConnect = false;
    this.retryTimer = null;
    this.pendingRx = []; // decoded bridge -> emulator frames
    this.pendingAir = []; // SoftDevice air events (connected/adv_report/written)
    this.lastBattery = null; // last GATT battery value (number|null)
    this.lastConn = null; // last bridge conn handle ({peer:[6]}|null)
    this.say('air: bridge not connected');
  }

  say(msg) {
    if (this.statusEl) this.statusEl.textContent = msg;
  }

  // Bench calls this when the user enables the BLE air peer.
  enable() {
    this.wantConnect = true;
    this.connect();
  }

  disable() {
    this.wantConnect = false;
    if (this.retryTimer) { clearTimeout(this.retryTimer); this.retryTimer = null; }
    if (this.ws) { try { this.ws.close(); } catch { /* closed */ } this.ws = null; }
    this.available = false;
    this.say('air: bridge not connected');
  }

  connect() {
    if (!this.wantConnect || (this.ws && this.ws.readyState <= 1)) return;
    let sock;
    try {
      sock = new WebSocket(this.url);
    } catch {
      this.retry();
      return;
    }
    sock.binaryType = 'arraybuffer';
    sock.onopen = () => {
      this.say('air: bridge connected, waiting for peer…');
    };
    sock.onmessage = (ev) => this.onMessage(ev.data);
    sock.onclose = () => {
      this.available = false;
      this.say('air: bridge not connected');
      this.retry();
    };
    sock.onerror = () => {
      try { sock.close(); } catch { /* closed */ }
    };
    this.ws = sock;
  }

  retry() {
    if (!this.wantConnect || this.retryTimer) return;
    this.retryTimer = setTimeout(() => {
      this.retryTimer = null;
      this.connect();
    }, RECONNECT_MS);
  }

  onMessage(data) {
    let msg;
    try {
      msg = JSON.parse(typeof data === 'string' ? data : new TextDecoder().decode(data));
    } catch {
      return;
    }
    if (msg.t === 'hello' || msg.t === 'adv') {
      this.available = true;
      this.say(`air: peer ${msg.addr ?? 'seen'} (tx ${this.txFrames}, rx ${this.rxFrames})`);
    } else if (msg.t === 'rx') {
      // Bridge -> emulator: one air frame (base64 bytes).
      this.available = true;
      this.pendingRx.push(Uint8Array.from(atob(msg.b64), (c) => c.charCodeAt(0)));
      this.rxFrames++;
      this.say(`air: live (tx ${this.txFrames}, rx ${this.rxFrames})`);
    } else if (msg.t === 'gatt' || msg.t === 'write_rsp' || msg.t === 'prim_disc_rsp'
        || msg.t === 'char_disc_rsp' || msg.t === 'desc_disc_rsp' || msg.t === 'hvx'
        || msg.t === 'rel_disc_rsp' || msg.t === 'attr_info_rsp' || msg.t === 'uuid_read_rsp'
        || msg.t === 'vals_read_rsp'
        || msg.t === 'connected' || msg.t === 'adv_report' || msg.t === 'disconnected'
        || msg.t === 'rssi' || msg.t === 'cancel' || msg.t === 'written'
        || msg.t === 'paired' || msg.t === 'sec_params_request' || msg.t === 'sec_info_request'
        || msg.t === 'auth_key_request' || msg.t === 'passkey_display' || msg.t === 'key_pressed'
        || msg.t === 'lesc_dhkey_request' || msg.t === 'l2cap_rx') {
      // SoftDevice-facing air replies: queued for the BLE pump below,
      // which completes the matching staged job (take/complete, no
      // cross-talk: every reply echoes conn/handle back). 'gatt' also
      // feeds the battery status line + lastBattery/lastConn mirrors.
      this.available = true;
      this.pendingAir.push(msg);
      if (msg.t === 'gatt') {
        const via = msg.overAir === false ? 'fallback' : 'over air';
        const v = Array.isArray(msg.value) ? msg.value[0] : msg.value;
        this.say(`air: battery ${v} (${via}) (tx ${this.txFrames}, rx ${this.rxFrames})`);
        if (typeof v === 'number') this.lastBattery = v;
        if (msg.conn) this.lastConn = msg.conn;
      } else {
        this.say(`air: ${msg.t} (tx ${this.txFrames}, rx ${this.rxFrames})`);
      }
    }
  }

  // Emulator -> bridge: one TX frame (Uint8Array payload bytes).
  sendTx(bytes) {
    if (!this.ws || this.ws.readyState !== 1) return false;
    const b64 = btoa(String.fromCharCode(...bytes.slice(0, 252)));
    this.ws.send(JSON.stringify({ t: 'tx', b64 }));
    this.txFrames++;
    return true;
  }

  // SoftDevice face: ask the bridge to resolve one staged BLE job
  // over air. The job object mirrors the ble_take_job() tags (see
  // lib.rs): {tag, conn, handle, offset, op, data, kind, start, end,
  // type, addr, cid, reason, uuid16, handles, peer}. `peer` (6 LE
  // bytes) addresses a NON-default air peer (multi-peer air); absent
  // means the default peer. Replies arrive as queued air messages
  // handled above. Every message carries its conn so the pump
  // completes the right link (multi-connection firmware).
  sendBle(job) {
    if (!this.ws || this.ws.readyState !== 1) return false;
    const m = { t: 'ble_read', conn: job.conn ?? 1, handle: job.handle ?? 0x13, offset: job.offset ?? 0 };
    if (job.peer) m.peer = [...job.peer];
    switch (job.tag) {
      case 0: m.t = 'ble_read'; break;
      case 1: return this.sendBleConnect(job.addr ?? []);
      case 2: m.t = 'ble_disconnect'; m.reason = job.reason ?? 19; break;
      case 3: m.t = 'ble_rssi'; break;
      case 4: m.t = 'ble_scan'; break;
      case 5: m.t = 'ble_disc'; m.kind = 0; m.start = job.start ?? 1; m.end = job.end ?? 0xFFFF; break;
      case 6: m.t = 'ble_disc'; m.kind = 2; m.start = job.start ?? 1; m.end = job.end ?? 0xFFFF; break;
      case 7: m.t = 'ble_disc'; m.kind = 3; m.start = job.start ?? 1; m.end = job.end ?? 0xFFFF; break;
      case 8: m.t = 'ble_write'; m.op = job.op ?? 1; m.handle = job.handle ?? 0; m.data = [...(job.data ?? [])]; m.len = job.data?.length ?? 0; break;
      case 9: m.t = 'ble_hvx'; m.handle = job.handle ?? 0; m.type = job.type ?? 1; m.data = [...(job.data ?? [])]; break;
      case 10: m.t = 'ble_l2cap'; m.cid = job.cid ?? 0x40; m.data = [...(job.data ?? [])]; break;
      case 11: m.t = 'ble_pair'; break;
      case 12: m.t = 'ble_disc'; m.kind = 1; m.start = job.start ?? 1; m.end = job.end ?? 0xFFFF; break;
      case 13: m.t = 'ble_disc'; m.kind = 4; m.start = job.start ?? 1; m.end = job.end ?? 0xFFFF; break;
      case 14: m.t = 'ble_uuid_read'; m.uuid16 = job.uuid16 ?? 0xFFFF; m.start = job.start ?? 1; m.end = job.end ?? 0xFFFF; break;
      case 15: m.t = 'ble_vals_read'; m.handles = [...(job.handles ?? [])]; break;
      case 16: m.t = 'ble_sc'; m.start = job.start ?? 1; m.end = job.end ?? 1; break;
      default: return false;
    }
    this.ws.send(JSON.stringify(m));
    this.txFrames++;
    return true;
  }

  sendBleConnect(addr) {
    if (!this.ws || this.ws.readyState !== 1) return false;
    this.ws.send(JSON.stringify({ t: 'ble_connect', addr: [...addr] }));
    this.txFrames++;
    return true;
  }

  // Drain one queued SoftDevice air event for the BLE pump, or null.
  takeAir() {
    return this.pendingAir.shift() ?? null;
  }

  // Drain one queued RX frame for the pump, or null.
  takeRx() {
    return this.pendingRx.shift() ?? null;
  }
}
