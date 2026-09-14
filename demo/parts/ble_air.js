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
    } else if (msg.t === 'gatt') {
      this.available = true;
      // Battery level read over real ATT air (scan -> connect ->
      // discover -> read on the bridge's emulator-side central).
      // `overAir:false` = bridge fallback (peer unreachable); treat the
      // value as advisory, keep the RX frame as the completion proof.
      const via = msg.overAir === false ? 'fallback' : 'over air';
      this.say(`air: battery ${msg.value} (${via}) (tx ${this.txFrames}, rx ${this.rxFrames})`);
      if (typeof msg.value === 'number') this.lastBattery = msg.value;
      if (msg.conn) this.lastConn = msg.conn;
    } else if (msg.t === 'connected' || msg.t === 'adv_report' || msg.t === 'written') {
      // SoftDevice-facing air events: queued for the BLE pump below.
      this.available = true;
      this.pendingAir.push(msg);
      this.say(`air: ${msg.t} (tx ${this.txFrames}, rx ${this.rxFrames})`);
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

  // SoftDevice face: ask the bridge to resolve a staged BLE job over
  // air. kind 'read' (GATTC read of the peer battery) or 'connect'
  // (GAP connect to a 6-byte LE address). Replies arrive as 'gatt' /
  // 'connected' messages handled in onMessage above.
  sendBle(kind, arg = {}) {
    if (!this.ws || this.ws.readyState !== 1) return false;
    if (kind === 'read') this.ws.send(JSON.stringify({ t: 'ble_read' }));
    else if (kind === 'connect') this.ws.send(JSON.stringify({ t: 'ble_connect', addr: arg.addr ?? [] }));
    else return false;
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
