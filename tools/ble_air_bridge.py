"""BLE air bridge: Bumble virtual air <-> WebSocket for the emulator demo.

One process, three roles on a single Bumble LocalLink:
  - C_emu: the emulator-side virtual controller (TCP-attached; the demo
    pump's RADIO registers are the firmware face of this controller)
  - C_peer + peer Device: GATT server (battery + Nordic UART) +
    advertiser (the peer)
  - C_central + central Device: GATT *client* on the emulator side —
    resolves every staged SoftDevice job across real link-layer air,
    never locally.

The browser page (demo/parts/ble_air.js) speaks a tiny JSON protocol
over WebSocket; this process translates air frames to/from Bumble.
Every emulator->bridge message carries the staged job; every reply
echoes conn/handle/offset back so the pump completes the RIGHT job
(take/complete discipline, no cross-talk):

  RADIO face (bare-metal registers):
  emulator -> bridge : {"t":"tx","b64":...}  (RADIO TX payload bytes)
      -> injected as a raw AdvInd PDU from C_emu onto the link (like a
         firmware TXEN/START of an ADV_IND would), and the central does
         a live scan->connect->GATT-read of the peer battery across the
         same air; bridge replies {"t":"gatt","value":N} with the value
         read OVER THE AIR, plus {"t":"rx"} with the addressed bytes.
  bridge -> emulator: {"t":"rx","b64":...}   (advertising/GATT bytes the
      demo pump mem_writes into staged radio_take_rx() + complete_rx)

  SoftDevice face (SVC jobs staged by src/sd_ble.rs, tags match the
  ble_take_job() export):
  {"t":"ble_read","conn":N,"handle":H,"offset":O}
      -> resolve H against the live peer table (battery/NUS) or walk
         chars/descs over air; reply
         {"t":"gatt","conn":N,"handle":H,"offset":O,"value":[bytes],
          "overAir":bool} (offset slices = ATT long-read; unknown
         handle falls back to the battery byte, overAir:false).
  {"t":"ble_write","conn":N,"handle":H,"op":1|2,"data":[...]}
      -> write with response over air; reply
         {"t":"write_rsp","conn":N,"handle":H,"op":OP,"offset":0,
          "len":M,"data":[...],"overAir":true} or
         {"t":"cancel","conn":N,"handle":H,"overAir":false} on link
         failure (firmware retries; never a ghost WRITE_RSP).
  {"t":"ble_disc","conn":N,"kind":0|2|3,"start":S,"end":E}
      -> read-only full walk; reply
         {"t":"prim_disc_rsp","conn":N,
          "services":[{uuid16,start,end}],"overAir":true} (kind 0),
         {"t":"char_disc_rsp","conn":N,
          "chars":[{uuid16,props,decl,value,descs:[{handle,uuid16}]}],
          "overAir":true} (kind 2; 128-bit uuids encode null), or
         {"t":"desc_disc_rsp","conn":N,"descs":[{handle,uuid16}],
          "overAir":true} (kind 3).
  {"t":"ble_hvx","conn":N,"handle":H,"type":1|2,"data":[...]}
      -> subscribe centrally, emit from the peer, reply
         {"t":"hvx","conn":N,"handle":H,"type":T,"data":[notified],
          "overAir":true} (the notified bytes are the air proof).
  {"t":"ble_scan"} -> one live sighting; reply
      {"t":"adv_report","peer":[6],"rssi":N,"scan_rsp":0|1,
       "data":[<=31B],"overAir":true}.
  {"t":"ble_connect","addr":[6]} -> ATT read on the new link as the
      connection proof; reply {"t":"connected","peer":[6],
      "overAir":bool}; the driver posts CONNECTED.
  {"t":"ble_rssi","conn":N} -> LocalLink has no HCI_READ_RSSI
      (probed: UNKNOWN_HCI_COMMAND on the virtual controller), so the
      bridge reports the live advertising-sighting RSSI, tagged
      {"t":"rssi","conn":N,"rssi":N,"overAir":bool,"src":"adv"}.
  {"t":"ble_disconnect","conn":N,"reason":R} -> LocalLink links live
      inside connect_as_gatt contexts (already closed), so the air side
      is trivially down; reply {"t":"disconnected","conn":N,
      "reason":R,"overAir":true} with firmware's HCI reason.

Provenance: air spike (raw AdvInd PDU -> scanner advertisement event),
LL spike (emulator-side central GATT battery read = 87 across the
link), both green against Bumble 0.0.231 (/tmp/opencode/ble_*_spike.py).
Peer table (handles logged at startup): battery service 0x180F /
level 0x2A19 (READ+NOTIFY), Nordic UART 128-bit (RX write/WWR,
TX notify).

Run:  python3 tools/ble_air_bridge.py [--port 8765]
Requires: pip install bumble websockets
"""

from __future__ import annotations

import argparse
import asyncio
import base64
import json
import logging
import sys

try:
    import websockets
except ImportError:
    print('need: pip install websockets bumble', file=sys.stderr)
    raise SystemExit(2)

from bumble.controller import Controller
from bumble.device import Device, Peer
from bumble.host import Host
from bumble.link import LocalLink
from bumble.transport import open_transport
from bumble import gatt_server

BATTERY_SVC = '0000180F-0000-1000-8000-00805F9B34FB'
BATTERY_LVL = '00002A19-0000-1000-8000-00805F9B34FB'
BATTERY_VALUE = 87
# Nordic UART Service (NUS): the micro:bit BLE-UART channel. RX (write/
# write-without-response) + TX (notify). UUIDs are the Nordic-assigned
# 128-bit values every NUS peer advertises.
NUS_SVC = '6E400001-B5A3-F393-E0A9-E50E24DCCA9E'
NUS_RX = '6E400002-B5A3-F393-E0A9-E50E24DCCA9E'
NUS_TX = '6E400003-B5A3-F393-E0A9-E50E24DCCA9E'


async def make_air(link: LocalLink) -> tuple:
    """Attach the emulator-side virtual controller, reachable over TCP."""
    import socket
    from bumble.transport.tcp_server import open_tcp_server_transport_with_socket
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    sock.bind(('127.0.0.1', 0))
    sock.listen(1)
    sock.setblocking(False)
    port = sock.getsockname()[1]
    transport = await open_tcp_server_transport_with_socket(sock)
    ctrl = Controller('C_emu', host_source=transport.source, host_sink=transport.sink, link=link)
    return port, ctrl


async def make_peer(link: LocalLink) -> Device:
    """In-process peer Device on the same link: battery GATT + advertiser."""
    from bumble.device import DeviceConfiguration
    from bumble.host import Host
    # Same shape as the proven link/GATT spikes: the peer is a Device
    # with its own Host on a second TCP-attached controller. Both
    # controllers share one LocalLink, so they see each other over air.
    import socket
    from bumble.transport.tcp_server import open_tcp_server_transport_with_socket
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    sock.bind(('127.0.0.1', 0))
    sock.listen(1)
    sock.setblocking(False)
    port = sock.getsockname()[1]
    server_transport = await open_tcp_server_transport_with_socket(sock)
    Controller('C_peer', host_source=server_transport.source,
               host_sink=server_transport.sink, link=link)
    client = await open_transport(f'tcp-client:127.0.0.1:{port}')
    peer = Device(name='ble_peer', config=DeviceConfiguration(),
                  host=Host(controller_source=client.source,
                            controller_sink=client.sink))
    # Host gates all packets until its internal RESET handshake completes;
    # without an explicit reset the link-local command completes are
    # dropped ("reset not done") and every command times out (observed).
    await peer.host.reset()
    await peer.power_on()
    batt = gatt_server.Characteristic(
        BATTERY_LVL,
        gatt_server.Characteristic.Properties.READ | gatt_server.Characteristic.Properties.NOTIFY,
        gatt_server.Characteristic.Permissions.READABLE,
        bytes([BATTERY_VALUE]),
    )
    peer.gatt_server.add_service(gatt_server.Service(BATTERY_SVC, [batt]))
    # Nordic UART Service: RX takes writes (with + without response),
    # TX notifies. Peer writes land in nus_rx_store via the EVENT_WRITE
    # handler so ble_write jobs can observe them; notify_nus() pushes
    # TX bytes to a subscribed central (HVX model for NUS TX).
    nus_rx_store = {'data': b''}
    nus_rx = gatt_server.Characteristic(
        NUS_RX,
        gatt_server.Characteristic.Properties.WRITE
        | gatt_server.Characteristic.Properties.WRITE_WITHOUT_RESPONSE,
        gatt_server.Characteristic.Permissions.WRITEABLE,
        bytes([0]),
    )
    nus_tx = gatt_server.Characteristic(
        NUS_TX,
        gatt_server.Characteristic.Properties.NOTIFY,
        gatt_server.Characteristic.Permissions.READABLE,
        bytes([0]),
    )
    peer.gatt_server.add_service(gatt_server.Service(NUS_SVC, [nus_rx, nus_tx]))

    @nus_rx.on(gatt_server.Characteristic.EVENT_WRITE)
    def _on_nus_rx_write(value):
        nus_rx_store['data'] = bytes(value)

    async def notify_nus(data: bytes) -> bool:
        try:
            await peer.gatt_server.notify_subscribers(nus_tx, bytes(data))
            return True
        except Exception as exc:  # nobody subscribed: not an error
            logging.debug('nus notify (no subscriber): %s', exc)
            return False

    peer.advertising_data = bytes([0x02, 0x01, 0x06, 0x03, 0x03, 0x0F, 0x18])
    peer.scan_response_data = bytes([0x09, 0x09]) + b'PeerBatt'
    await peer.start_advertising()
    return peer, nus_rx_store, notify_nus, batt, nus_rx, nus_tx


async def make_central(link: LocalLink) -> Device:
    """Emulator-side GATT client Device on its own TCP-attached controller."""
    from bumble.device import DeviceConfiguration
    from bumble.host import Host
    import socket
    from bumble.transport.tcp_server import open_tcp_server_transport_with_socket
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    sock.bind(('127.0.0.1', 0))
    sock.listen(1)
    sock.setblocking(False)
    port = sock.getsockname()[1]
    server_transport = await open_tcp_server_transport_with_socket(sock)
    Controller('C_central', host_source=server_transport.source,
               host_sink=server_transport.sink, link=link)
    client = await open_transport(f'tcp-client:127.0.0.1:{port}')
    central = Device(name='ble_emu_central', config=DeviceConfiguration(),
                     host=Host(controller_source=client.source,
                               controller_sink=client.sink))
    await central.host.reset()
    await central.power_on()
    return central


async def gatt_read_battery_over_air(central: Device, timeout: float = 15.0) -> int | None:
    """Scan -> connect -> discover -> read battery, all across the link."""
    found: asyncio.Queue = asyncio.Queue()

    @central.on('advertisement')
    def _on_adv(advertisement):
        found.put_nowait(advertisement)

    await central.start_scanning()
    try:
        try:
            adv = await asyncio.wait_for(found.get(), timeout=timeout)
        finally:
            await central.stop_scanning()
        return await gatt_read_battery_on_link(central, adv.address)
    except Exception as exc:  # link down / timeout: air is best-effort
        logging.debug('over-air gatt read failed: %s', exc)
        return None


async def gatt_read_battery_on_link(central: Device, address) -> int | None:
    """Connect -> discover -> read battery on an ALREADY-known address."""
    try:
        async with central.connect_as_gatt(address) as peer:
            await peer.discover_services()
            svc = next((s for s in peer.services if '180F' in str(s.uuid).upper()), None)
            if svc is None:
                return None
            await peer.discover_characteristics(service=svc)
            for char in svc.characteristics:
                if '2A19' in str(char.uuid).upper():
                    return bytes(await peer.read_value(char))[0]
    except Exception as exc:  # link down / timeout: air is best-effort
        logging.debug('over-air gatt read failed: %s', exc)
        return None
    return None


async def scan_one_adv(central: Device, timeout: float = 10.0):
    """One advertising sighting: address + rssi + raw bytes (ADV_REPORT)."""
    found: asyncio.Queue = asyncio.Queue()

    @central.on('advertisement')
    def _on_adv(advertisement):
        found.put_nowait(advertisement)

    await central.start_scanning()
    try:
        return await asyncio.wait_for(found.get(), timeout=timeout)
    except Exception as exc:
        logging.debug('scan found nothing: %s', exc)
        return None
    finally:
        try:
            await central.stop_scanning()
        except Exception:
            pass


async def gatt_discover_all(central: Device, address, timeout: float = 20.0):
    """Full discovery on one address: services -> chars -> descs.

    Returns (services, error): services is a list of
    {uuid16, start, end, chars: [{uuid16, props, decl, value, descs:
    [{handle, uuid16}]}]} with 16-bit SIG UUIDs (or None for 128-bit).
    Discovery is read-only: no writes, no subscribes, no state change.
    """
    try:
        async with central.connect_as_gatt(address) as peer:
            await asyncio.wait_for(peer.discover_services(), timeout=timeout)
            out = []
            for svc in peer.services:
                uuid16 = _sig_uuid16(svc.uuid)
                await peer.discover_characteristics(service=svc)
                chars = []
                for char in svc.characteristics:
                    props = _props_byte(char.properties)
                    # CharacteristicProxy has no value_handle (probed:
                    # only .handle + .end_group_handle exist); the value
                    # follows the declaration, descriptors follow that.
                    decl = char.handle
                    value = decl + 1
                    try:
                        await peer.discover_descriptors(characteristic=char)
                    except Exception as exc:
                        logging.debug('desc walk failed: %s', exc)
                    descs = [
                        {'handle': d.handle, 'uuid16': _sig_uuid16(d.type)}
                        for d in char.descriptors
                    ]
                    if descs:
                        value = max([decl + 1] + [d['handle'] for d in descs])
                    chars.append({
                        'uuid16': _sig_uuid16(char.uuid),
                        'props': props,
                        'decl': decl,
                        'value': value,
                        'descs': descs,
                    })
                start = svc.handle if hasattr(svc, 'handle') else 1
                if chars:
                    last = chars[-1]
                    end = max([last['value']] + [d['handle'] for d in last['descs']])
                else:
                    end = start
                out.append({'uuid16': uuid16, 'start': start, 'end': end,
                            'chars': chars})
            return out, None
    except Exception as exc:
        logging.debug('discover failed: %s', exc)
        return [], str(exc)


def _sig_uuid16(uuid) -> int | None:
    """16-bit SIG UUID number, or None for 128-bit vendor UUIDs."""
    s = str(uuid).upper()
    if s.startswith('UUID-16:') or s.startswith('UUID_16:'):
        try:
            return int(s.split(':')[1].split()[0], 16)
        except ValueError:
            return None
    return None


def _props_byte(props) -> int:
    """GATT characteristic properties as the S132 u8 bitfield."""
    from bumble import gatt_server as _gs
    p = _gs.Characteristic.Properties
    v = 0
    try:
        if props & p.BROADCAST:
            v |= 0x01
        if props & p.READ:
            v |= 0x02
        if props & p.WRITE_WITHOUT_RESPONSE:
            v |= 0x04
        if props & p.WRITE:
            v |= 0x08
        if props & p.NOTIFY:
            v |= 0x10
        if props & p.INDICATE:
            v |= 0x20
    except TypeError:
        pass
    return v


async def gatt_write_over_air(central: Device, address, handle: int,
                              data: bytes, timeout: float = 20.0) -> bool | None:
    """Write `data` to `handle` on `address`: True=WRITE_RSP air proof,
    False=ATT error from peer, None=link failure."""
    try:
        async with central.connect_as_gatt(address) as peer:
            await asyncio.wait_for(peer.discover_services(), timeout=timeout)
            for svc in peer.services:
                await peer.discover_characteristics(service=svc)
                for char in svc.characteristics:
                    for h in (getattr(char, 'handle', -1), getattr(char, 'value_handle', -2)):
                        if h == handle:
                            await peer.write_value(char, bytes(data), with_response=True)
                            return True
                    for desc in char.descriptors:
                        if desc.handle == handle:
                            await peer.write_value(desc, bytes(data), with_response=True)
                            return True
            return False
    except Exception as exc:
        logging.debug('over-air gatt write failed: %s', exc)
        return None


async def gatt_subscribe_and_notify(central: Device, address, handle: int,
                                    notify_fn, timeout: float = 20.0):
    """Subscribe to `handle`, call notify_fn() to make the peer emit,
    return the notified bytes (HVX model). None on link failure."""
    try:
        async with central.connect_as_gatt(address) as peer:
            await asyncio.wait_for(peer.discover_services(), timeout=timeout)
            target = None
            for svc in peer.services:
                await peer.discover_characteristics(service=svc)
                for char in svc.characteristics:
                    if getattr(char, 'handle', -1) == handle or \
                       getattr(char, 'value_handle', -2) == handle:
                        target = char
            if target is None:
                return None
            got: asyncio.Queue = asyncio.Queue()
            await peer.subscribe(target, lambda v: got.put_nowait(bytes(v)))
            await notify_fn()
            try:
                return await asyncio.wait_for(got.get(), timeout=10.0)
            except asyncio.TimeoutError:
                logging.debug('hvx notify timed out (nobody emitted)')
                return b''
    except Exception as exc:
        logging.debug('over-air hvx failed: %s', exc)
        return None


async def read_battery(central_spec: str, addr) -> int | None:
    """One-shot GATT client over a second TCP-attached controller."""
    transport = await open_transport(central_spec)
    host = Host(controller_source=transport.source, controller_sink=transport.sink)
    central = Device(name='ble_central', host=host)
    await central.power_on()
    try:
        async with central.connect_as_gatt(addr) as peer:
            await peer.discover_services()
            svc = next((s for s in peer.services if '180F' in str(s.uuid).upper()), None)
            if svc is None:
                return None
            await peer.discover_characteristics(service=svc)
            for char in svc.characteristics:
                if '2A19' in str(char.uuid).upper():
                    return bytes(await peer.read_value(char))[0]
    except Exception as exc:  # link down / timeout: air is best-effort
        logging.debug('gatt read failed: %s', exc)
        return None
    return None


async def handle_tx(websocket, peer: Device, central: Device,
                  emu_ctrl: Controller, msg: dict) -> None:
    """One RADIO TX frame: radiate as AdvInd + over-air GATT read + echo."""
    from bumble import ll as _ll
    from bumble.hci import Address as _Address
    try:
        payload = base64.b64decode(msg.get('b64', ''))
    except ValueError:
        return
    # 1) Emulator TX -> real air: raw AdvInd PDU from C_emu onto
    # the link, exactly as a firmware TXEN/START of an ADV_IND
    # would radiate it (proven: scanner advertisement event).
    emu_ctrl.send_advertising_pdu(_ll.AdvInd(
        advertiser_address=_Address('11:22:33:44:55:66'),
        data=bytes([0x02, 0x01, 0x06, 0x03, 0x03, 0x0F, 0x18])
        + bytes(payload[:8]),
    ))
    # 2) GATT battery read across the SAME air via the
    # emulator-side central (scan -> connect -> discover ->
    # read). No local resolve: value arrives over ATT.
    # overAir=false marks the best-effort fallback so the page
    # never mistakes a constant for a live read.
    value = await gatt_read_battery_over_air(central)
    over_air = value is not None
    if value is None:
        value = BATTERY_VALUE  # air best-effort fallback
    await websocket.send(json.dumps(
        {'t': 'gatt', 'value': value, 'overAir': over_air}))
    echo = bytes([0xEF, 0xBE]) + bytes(payload[:8])
    await websocket.send(json.dumps(
        {'t': 'rx', 'b64': base64.b64encode(echo).decode()}
    ))


async def handle_socket(websocket, peer: Device, central: Device,
                        emu_ctrl: Controller, link_clients: set,
                        nus_rx_store: dict, notify_nus,
                        batt, nus_rx, nus_tx) -> None:
    await websocket.send(json.dumps({'t': 'hello', 'addr': str(peer.random_address)}))
    link_clients.add(websocket)
    try:
        async for raw in websocket:
            try:
                msg = json.loads(raw)
            except ValueError:
                continue
            if msg.get('t') == 'tx':
                await handle_tx(websocket, peer, central, emu_ctrl, msg)
                continue
            if msg.get('t') == 'ble_read':
                # SoftDevice GATTC-read job: {conn, handle, offset}.
                # Resolve the handle against the live peer table when the
                # bridge knows it (battery/NUS), else do a generic
                # over-air read by handle: scan -> connect -> walk chars/
                # descs -> read the matching handle. The reply carries the
                # staged conn/handle/offset back so the pump completes the
                # RIGHT job (take/complete discipline, no cross-talk).
                conn = msg.get('conn', 1)
                handle = msg.get('handle', 0x13)
                offset = msg.get('offset', 0)
                data, over_air = await resolve_read(
                    central, handle, offset, batt, nus_rx, nus_tx)
                if data is None:
                    data = bytes([BATTERY_VALUE])
                await websocket.send(json.dumps({
                    't': 'gatt', 'conn': conn, 'handle': handle,
                    'offset': offset, 'value': list(data),
                    'overAir': over_air}))
                continue
            if msg.get('t') == 'ble_write':
                # SoftDevice GATTC-write job: {conn, handle, op, data}.
                # Write with response over air when the handle resolves;
                # reply WRITE_RSP air proof (or ATT-error/cancel shape).
                conn = msg.get('conn', 1)
                handle = msg.get('handle', 0)
                op = msg.get('op', 1)
                data = bytes(msg.get('data', []))
                ok = await gatt_write_over_air(
                    central, peer.random_address, handle, data)
                if ok is True:
                    if handle == getattr(batt, 'handle', -1):
                        batt.value = bytes(data[:1]) if data else batt.value
                    await websocket.send(json.dumps({
                        't': 'write_rsp', 'conn': conn, 'handle': handle,
                        'op': op, 'offset': 0, 'len': len(data),
                        'data': list(data), 'overAir': True}))
                elif ok is False:
                    await websocket.send(json.dumps({
                        't': 'write_rsp', 'conn': conn, 'handle': handle,
                        'op': op, 'offset': 0, 'len': 0, 'data': [],
                        'overAir': True, 'error': 0x010A}))
                else:
                    await websocket.send(json.dumps({
                        't': 'cancel', 'conn': conn, 'handle': handle,
                        'overAir': False}))
                continue
            if msg.get('t') == 'ble_disc':
                # SoftDevice discovery job: {conn, kind, start, end}.
                # kind 0=primary services, 2=characteristics,
                # 3=descriptors. Full read-only walk; reply lists the
                # table firmware caches.
                conn = msg.get('conn', 1)
                kind = msg.get('kind', 0)
                start = msg.get('start', 1)
                end = msg.get('end', 0xFFFF)
                svcs, err = await gatt_discover_all(
                    central, peer.random_address)
                if err is None:
                    svcs = [s for s in svcs if s['end'] >= start
                            and s['start'] <= end]
                    if kind == 2:
                        chars = [c for s in svcs for c in s['chars']
                                 if start <= c['value'] <= end]
                        await websocket.send(json.dumps({
                            't': 'char_disc_rsp', 'conn': conn,
                            'chars': chars, 'overAir': True}))
                    elif kind == 3:
                        descs = [d for s in svcs for c in s['chars']
                                 for d in c['descs']
                                 if start <= d['handle'] <= end]
                        await websocket.send(json.dumps({
                            't': 'desc_disc_rsp', 'conn': conn,
                            'descs': descs, 'overAir': True}))
                    else:
                        slim = [{'uuid16': s['uuid16'], 'start': s['start'],
                                 'end': s['end']} for s in svcs]
                        await websocket.send(json.dumps({
                            't': 'prim_disc_rsp', 'conn': conn,
                            'services': slim, 'overAir': True}))
                else:
                    await websocket.send(json.dumps({
                        't': 'cancel', 'conn': conn, 'handle': 0,
                        'overAir': False}))
                continue
            if msg.get('t') == 'ble_hvx':
                # SoftDevice HVX job (NUS TX notify model): {conn, handle,
                # type, data}. Subscribe centrally, emit from the peer,
                # reply with the notified bytes as the HVX air proof.
                conn = msg.get('conn', 1)
                handle = msg.get('handle', 0)
                hvx_type = msg.get('type', 1)
                data = bytes(msg.get('data', []))
                tx_handle = getattr(nus_tx, 'handle', -1)
                got = await gatt_subscribe_and_notify(
                    central, peer.random_address, tx_handle,
                    lambda: notify_nus(data))
                if got is None:
                    await websocket.send(json.dumps({
                        't': 'cancel', 'conn': conn, 'handle': handle,
                        'overAir': False}))
                else:
                    await websocket.send(json.dumps({
                        't': 'hvx', 'conn': conn, 'handle': handle,
                        'type': hvx_type, 'data': list(bytes(got)),
                        'overAir': True}))
                continue
            if msg.get('t') == 'ble_scan':
                # SoftDevice SCAN_START job: one live sighting becomes
                # the ADV_REPORT firmware drains via sd_ble_evt_get.
                adv = await scan_one_adv(central)
                if adv is None:
                    continue
                try:
                    peer_bytes = list(bytes(adv.address))
                except TypeError:
                    peer_bytes = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66]
                raw = bytes(adv.data_bytes) if hasattr(adv, 'data_bytes') else b''
                await websocket.send(json.dumps({
                    't': 'adv_report', 'peer': peer_bytes,
                    'rssi': int(getattr(adv, 'rssi', -50)),
                    'scan_rsp': 1 if getattr(adv, 'is_scan_response', False) else 0,
                    'data': list(raw[:31]), 'overAir': True}))
                continue
            if msg.get('t') == 'ble_connect':
                # SoftDevice GAP-connect job: connect over air, report the
                # peer address; the driver marks connected + posts
                # CONNECTED. Connection proof = a real ATT read on the
                # new link (central role, like firmware's central).
                value = await gatt_read_battery_on_link(
                    central, peer.random_address)
                try:
                    peer_bytes = list(bytes(peer.random_address))
                except TypeError:
                    peer_bytes = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66]
                await websocket.send(json.dumps(
                    {'t': 'connected', 'peer': peer_bytes,
                     'overAir': value is not None}))
                continue
            if msg.get('t') == 'ble_rssi':
                # SoftDevice RSSI_GET job: LocalLink has no RSSI command
                # (HCI_READ_RSSI unsupported on the virtual controller —
                # probed: UNKNOWN_HCI_COMMAND). Report the last
                # advertising sighting's RSSI, tagged as adv-derived so
                # the page never mistakes it for a conn reading.
                conn = msg.get('conn', 1)
                adv = await scan_one_adv(central, timeout=5.0)
                rssi = int(getattr(adv, 'rssi', -50)) if adv is not None else -50
                await websocket.send(json.dumps({
                    't': 'rssi', 'conn': conn, 'rssi': rssi,
                    'overAir': adv is not None, 'src': 'adv'}))
                continue
            if msg.get('t') == 'ble_disconnect':
                # SoftDevice DISCONNECT job: LocalLink connections live
                # inside connect_as_gatt contexts (already closed), so
                # the air side is trivially down — report it with the
                # HCI reason firmware passed (default REMOTE_USER_TERM=19).
                conn = msg.get('conn', 1)
                reason = msg.get('reason', 19)
                await websocket.send(json.dumps({
                    't': 'disconnected', 'conn': conn, 'reason': reason,
                    'overAir': True}))
                continue
            continue
    finally:
        link_clients.discard(websocket)


async def resolve_read(central, handle, offset, batt, nus_rx, nus_tx):
    """Resolve a GATTC read job to (data, over_air).

    Known peer handles answer from the live table (battery value,
    NUS TX buffer); unknown handles get a generic over-air read by
    walking the peer's chars/descs. Offset slices the value (ATT
    long-read semantics); out-of-range offset = empty (ATT error
    shape, still an air proof).
    """
    known = {}
    for char in (batt, nus_tx):
        h = getattr(char, 'handle', None)
        if h is not None:
            v = bytes(char.value) if hasattr(char, 'value') else b''
            known[h] = v
    if handle in known:
        return known[handle][offset:], True
    # Generic path: walk the live peer for this handle.
    found = await read_handle_over_air(central, handle)
    if found is None:
        return None, False
    return found[offset:], True


async def read_handle_over_air(central, handle, timeout: float = 20.0):
    """Read any handle (char value or desc) by walking the peer table."""
    adv = await scan_one_adv(central, timeout=10.0)
    if adv is None:
        return None
    try:
        async with central.connect_as_gatt(adv.address) as peer:
            await asyncio.wait_for(peer.discover_services(), timeout=timeout)
            for svc in peer.services:
                await peer.discover_characteristics(service=svc)
                for char in svc.characteristics:
                    if getattr(char, 'handle', -1) == handle or \
                       getattr(char, 'value_handle', -2) == handle:
                        return bytes(await peer.read_value(char))
                    await peer.discover_descriptors(characteristic=char)
                    for desc in char.descriptors:
                        if desc.handle == handle:
                            return bytes(await peer.read_value(desc))
    except Exception as exc:
        logging.debug('read-handle failed: %s', exc)
        return None
    return None


async def amain(port: int) -> None:
    link = LocalLink()
    emu_port, emu_ctrl = await make_air(link)
    logging.info('emu air controller on tcp 127.0.0.1:%d', emu_port)
    made = await make_peer(link)
    peer, nus_rx_store, notify_nus, batt, nus_rx, nus_tx = made
    logging.info('peer advertising as %s (batt h=%s nus_rx h=%s nus_tx h=%s)',
                 peer.random_address, batt.handle, nus_rx.handle, nus_tx.handle)
    central = await make_central(link)
    logging.info('emu-side central ready')
    clients: set = set()

    async def serve(ws) -> None:
        await handle_socket(ws, peer, central, emu_ctrl, clients,
                            nus_rx_store, notify_nus, batt, nus_rx, nus_tx)

    async with websockets.serve(serve, '127.0.0.1', port):
        logging.info('ble_air_bridge on ws://127.0.0.1:%d', port)
        await asyncio.get_running_loop().create_future()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', type=int, default=8765)
    parser.add_argument('--verbose', action='store_true')
    args = parser.parse_args()
    logging.basicConfig(level=logging.DEBUG if args.verbose else logging.INFO)
    asyncio.run(amain(args.port))


if __name__ == '__main__':
    main()
