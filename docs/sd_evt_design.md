# Scoped `sd_evt_get` transport — design

Status: DRAFT (2026-09-12). Unimplemented. This moves SoftDevice event
delivery from "deferred" (STATUS §7) to scoped: **flash-operation
events only** (phase 1). No BLE event synthesis (still out of scope).

## 1. Why (evidence)

Three live firmware paths wait on SD-event completion (plan P24–P26):

1. MicroPython app flash path: `SVC40` (`sd_flash_page_erase`) →
   waits `*0x20004A9E`, set by the app's event handler after pumping
   `sd_evt_get`. Reached in-demo (never natively — SD stays disabled
   there, so native takes the skip branch 10/10).
2. Bootloader validation flow (touches SD-gated flash ops; full chain
   still parks at `0x783FE`).
3. Anything BLE (radio attempts need the event pump; phase 2 at most).

## 2. Key insight: transport, not synthesis

We do NOT emulate the SoftDevice's event system. The real S140 binary
already implements the state machine; the model only needs to be the
**event wire**, exactly like DMA take/complete:

- The SD performs flash ops by writing NVMC registers directly. Our
  NVMC model already observes those writes (`take_erase`, write path).
- On completion (driver `complete_erase`, instant writes), the model
  posts an SoC event into a model-side queue — no SD internals involved.
- When firmware calls `sd_evt_get`, an SVC hook answers from the model
  queue INSTEAD of entering the SD (whose internal queue is empty, so
  it would return NOT_FOUND forever). Queue empty → fall through to
  the SD untouched (returns NOT_FOUND, harmless).

## 3. Numbers (from Arduino-nRF52 S132 headers; VERIFY against S140)

SVC numbers are enum positions (`SOC_SVC_BASE = 0x20`):

| Call | SVC # |
|---|---|
| `sd_softdevice_enable` | 16 (`SDM_SVC_BASE+0`) |
| `sd_softdevice_is_enabled` | 18 |
| `sd_flash_page_erase(page)` | 40 (`SOC_SVC_BASE+8`) |
| `sd_flash_write(dst, src, len)` | 41 |
| **`sd_evt_get(buf)`** | **82** (`SOC_SVC_BASE+50`) |

SoC event IDs (`NRF_SOC_EVTS`, from 0): 0 = HFCLKSTARTED,
1 = POWER_FAILURE_WARNING, **2 = FLASH_OPERATION_SUCCESS**,
3 = FLASH_OPERATION_ERROR.

`nrf_evt_t` = `{ evt_id: u16, evt_len: u16, params... }` (confirm the
`sd_evt_get` buffer protocol — app-provided word buffer address in r0
— against S140 headers and, if needed, the app's event-pump
disassembly before implementing).

S140 caveat: verify the three SVC numbers + event IDs against the
S140 binary in `full.bin` (find the app's `sd_evt_get` poll loop /
`svc 0x52` sites) — S132/S140 are stable here, but trust the binary.

## 4. Where it lives (AGENTS.md compliance note)

NOT a `Peripheral` (no MMIO base — it is an SVC interface). New module
`src/sd_evt.rs`: `struct SdEvt { queue: VecDeque<(u16 id, u16 len)> }`,
owned by `WasmSystem` (field, constructed in `new()`/`new_svd()`),
explicitly documented as a service, not a peripheral. Hook: one
compare in the SVC arm (`thumb.rs`, next to the existing delivery
gate) + a 3-line post in the NVMC completion path. Cost when idle:
one `u16` compare per SVC.

Phase-1 behavior:

- `complete_erase` / instant-write completion while the SD is enabled
  (track via SVC16/SVC17 observed calls, default off) → push
  `(FLASH_OPERATION_SUCCESS, 0)`.
- SVC 82 with a non-empty queue → write `{id, len}` (+ params, none
  for flash events) to the app buffer at r0, set r0 = NRF_SUCCESS(0),
  skip SD entry (advance past the `svc`, like a completed call).
- SVC 82 with an empty queue → fall through to the SD (NOT_FOUND).
- SD disabled → never queue (matches silicon: "no event will be
  generated").

No new wasm exports for phase 1 (fully automatic; demo unchanged).
Optional debug export later: `sd_evt_queue_len()`.

## 5. Double-delivery + ordering rules

- Post the event only on the driver completion (take→complete), never
  on staging — same discipline as every DMA pump.
- If the SD *also* posts internally (it shouldn't — its queue never
  fills without radio), model-first ordering keeps exactly-once
  semantics: app drains model events, then SD's (empty → NOT_FOUND).
- Never synthesize BLE/radio/timeslot events in phase 1. HFCLKSTARTED
  (id 0) explicitly NOT queued (CLOCK model owns that signal).

## 6. Firmware proof (required by repo rules)

`blinky/sd_evt_nrf.s`: NVMC page erase via model → pump
(take/complete) → `svc 82` loop → assert first event bytes are
`02 00 00 00` (id=2, len=0) and second poll falls through cleanly.
Plus native test: queue→hook delivery, empty→fallthrough,
disabled→no-queue. `cargo test` stays green; one peripheral-style
commit (module + hook + tests + proof), even though it is a service.

## 7. Done criteria

- MPY demo path (SVC40 → `*0x20004A9E`) completes instead of
  spinning: flag sets within bounded pumps after the erase completes.
- No behavior change when SD disabled (native MPY path: still skips
  10/10, still banners+fails identically — the pre-existing NULL
  fault is separate work).
- Zero-cost when idle (no SVC82 traffic in blinky/sensors proofs).
