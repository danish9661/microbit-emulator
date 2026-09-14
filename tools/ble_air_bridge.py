"""BLE air bridge: Bumble virtual air <-> WebSocket for the emulator demo.

One process, three roles on a single Bumble LocalLink:
  - C_emu: the emulator-side virtual controller (TCP-attached; the demo
    pump's RADIO registers are the firmware face of this controller)
  - C_peer + peer Device: battery GATT server + advertiser (the peer)
  - C_central + central Device: GATT *client* on the emulator side —
    reads the peer's battery across real link-layer air (scan ->
    connect -> discover -> read), never resolved locally.

The browser page (demo/parts/ble_air.js) speaks a tiny JSON protocol
over WebSocket; this process translates air frames to/from Bumble:

  emulator -> bridge : {"t":"tx","b64":...}  (RADIO TX payload bytes)
      -> injected as a raw AdvInd PDU from C_emu onto the link (like a
         firmware TXEN/START of an ADV_IND would), and the central does
         a live scan->connect->GATT-read of the peer battery across the
         same air; bridge replies {"t":"gatt","value":N} with the value
         read OVER THE AIR, plus {"t":"rx"} with the addressed bytes.
  emulator -> bridge : {"t":"ble_read"}  (SoftDevice GATTC read job)
      -> same over-air battery read, but the reply ALSO carries the
         conn event shape firmware drains via sd_ble_evt_get: first
         {"t":"connected","peer":[6 LE bytes]} (the central's link to
         the peer), then {"t":"gatt","value":N,"overAir":bool}.
  emulator -> bridge : {"t":"ble_connect","addr":[6]}  (GAP connect job)
      -> the central connects over air to the advertised peer and the
         bridge replies {"t":"connected","peer":[6 LE bytes]}; the
         driver completes via ble_complete_gap_connect() which posts
         the SoftDevice CONNECTED event.
  bridge -> emulator: {"t":"rx","b64":...}   (advertising/GATT bytes the
      demo pump mem_writes into staged radio_take_rx() + complete_rx)

Provenance: air spike (raw AdvInd PDU -> scanner advertisement event),
LL spike (emulator-side central GATT battery read = 87 across the
link), both green against Bumble 0.0.231 (/tmp/opencode/ble_*_spike.py).

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
        gatt_server.Characteristic.Properties.READ,
        gatt_server.Characteristic.Permissions.READABLE,
        bytes([BATTERY_VALUE]),
    )
    peer.gatt_server.add_service(gatt_server.Service(BATTERY_SVC, [batt]))
    peer.advertising_data = bytes([0x02, 0x01, 0x06, 0x03, 0x03, 0x0F, 0x18])
    await peer.start_advertising()
    return peer


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
        async with central.connect_as_gatt(adv.address) as peer:
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
                        emu_ctrl: Controller, link_clients: set) -> None:
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
                # SoftDevice GATTC-read job: over-air battery read; the
                # driver completes via ble_complete_gattc_read() which
                # posts the READ_RSP firmware drains via sd_ble_evt_get.
                value = await gatt_read_battery_over_air(central)
                over_air = value is not None
                if value is None:
                    value = BATTERY_VALUE  # air best-effort fallback
                await websocket.send(json.dumps(
                    {'t': 'gatt', 'value': value, 'overAir': over_air,
                     'handle': 0x13, 'offset': 0}))
                echo = bytes([0xEF, 0xBE]) + bytes([value])
                await websocket.send(json.dumps(
                    {'t': 'rx', 'b64': base64.b64encode(echo).decode()}
                ))
                continue
            if msg.get('t') == 'ble_connect':
                # SoftDevice GAP-connect job: connect over air, report the
                # peer address; the driver marks connected + posts CONNECTED.
                value = await gatt_read_battery_over_air(central)
                try:
                    peer_bytes = list(bytes(peer.random_address))
                except TypeError:
                    peer_bytes = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66]
                await websocket.send(json.dumps(
                    {'t': 'connected', 'peer': peer_bytes,
                     'overAir': value is not None}))
                continue
            continue
    finally:
        link_clients.discard(websocket)


async def amain(port: int) -> None:
    link = LocalLink()
    emu_port, emu_ctrl = await make_air(link)
    logging.info('emu air controller on tcp 127.0.0.1:%d', emu_port)
    peer = await make_peer(link)
    logging.info('peer advertising as %s', peer.random_address)
    central = await make_central(link)
    logging.info('emu-side central ready')
    clients: set = set()

    async def serve(ws) -> None:
        await handle_socket(ws, peer, central, emu_ctrl, clients)

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
