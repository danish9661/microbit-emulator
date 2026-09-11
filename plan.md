# micro:bit v2.2 emulator — build plan (nRF52833 only)

Target: BBC micro:bit v2 / v2.2 target MCU = Nordic nRF52833
(Cortex-M4F, 64 MHz, 512 KB flash @ 0x00000000, 128 KB RAM @ 0x20000000).
v2.2 board rev = same SoC, minor board change.

OUT OF SCOPE (explicit):
- STM32 (all STM32 peripheral models deleted, not rewritten)
- Arduino UNO R4 / Renesas RA4M1 / Cortex-M33 (different core, not reusable)
- Interface MCU: KL27 M0+ (v2.00) / nRF52820 M4 (v2.2x) running DAPLink.
  Not emulated. Replaced by JS: hex-drop loader + UART console + reset.
- SoftDevice BLE stack (SVC interface). Target non-SoftDevice firmware first.

## 0. Baseline (done)
- git backup: `snapshot: STM32 baseline before nRF52833 v2.2 port`
- Reuse target: `src/cpu/` (thumb decoder, Cpu, FlatMemory trait, regs S0-S31).
  Never hand-edit `cpu/` for board problems.

## 1. KEEP (generic ARM)
`cpu/*`, `peripherals/nvic.rs,scb.rs,systick.rs,mpu.rs,fpu.rs,dwt.rs,itm.rs,stir.rs`,
`Peripheral` trait + `tick_n` + `INSTRUCTION_COUNT` clock, `*_tap` pattern,
`docs/*.s` probe method (same armv7e-m + fpv4-sp-d16).

## 2. DELETE (STM32-only, ~25 files)
`rcc,flash,pwr,gpio,tim,usart,spi,i2s,sai,i2c,adc,dac,can,sdio,dcmi,fsmc,ltdc,exti,syscfg,dbgmcu,cryp,hash,eth,crc,rtc,rng,wwdg,iwdg,qspi_stm32`
+ `monox/stm32f407.svd`, `blinky/blinky.bin` (linked at 0x08000000, useless),
+ STM32 globals in `system.rs` (ETH_*, CAN_STAGED, AUDIO_WAV, DCMI_FRAME, FLASH_PROGRAMMING),
+ STM32 exports in `lib.rs` (can_inject, tim_inject_capture, eth_*, ltdc_*, dcmi_*, fsmc_*, audio_*).
Reason: Nordic uses TASKS/EVENTS/SHORTS, zero register overlap. Rewrite = delete 95% anyway.

## 3. REWIRE
- `cpu/mem.rs`: `flash_base 0x08000000 -> 0x00000000`. Loader reads SP/PC from 0x0/0x4.
- `peripherals/mod.rs new_wasm/from_svd`: replace STM32 base table with nRF map.
- Crate + folder renamed `stm32-periph-wasm` -> `nrf52833-periph-wasm` (done P7c).
- SVD: fetch `nrf52833.svd` from Nordic MDK into `monox/nrf52833.svd`.

## 4. ADD (Nordic, in order)
1. `ficr_uicr.rs` (0x10000000/0x10001000, constants+storage, boot hangs without it)
2. `clock.rs` (0x40000000 HFCLK/LFCLK TASKS/EVENTS)
3. `power.rs` + `nvmc.rs` (stub READY + erase/program handshake)
4. `gpio_nrf.rs` (P0 0x50000000 32pins + P1 0x50000300 10pins, OUT/IN/DIR/CNF)
5. `timer_nrf.rs` (TIMER0-4) + `rtc_nrf.rs` (RTC0-2) + SysTick delays
6. `uarte_nrf.rs` (UARTE0 console, lifeline)
7. `twim_nrf.rs` + `gpiote_ppi.rs` (LSM303 sensor, buttons, matrix needs PPI dispatch)
8. `saadc.rs,temp.rs,rng_nrf.rs,pwm.rs,pdm.rs,qspi_nrf.rs,usbd.rs`
9. `radio.rs` last. Board: 5x5 matrix via GPIO rows/cols, BTN_A P0.14 BTN_B P0.23.

## 5. Firmware strategy
bare-metal blinky (flash@0x0, UART marker) -> UART echo -> CODAL blinky
(codal-microbit-v2, ARM GCC) -> MicroPython/Zephyr hello. No full DAL first.

## 6. Validation
Per-peripheral: boot marker + functional marker + 2nd consecutive run
(no state leak, `reset_state`). `cargo test` green before claim.
Minimum boot prove: `synth_vector_boot` + new `nrf_boot_flash_at_zero` test.

## 7. Phases
P1: strip + rewire + FICR/CLOCK/GPIO/NVMC (done)
P2: TIMER/RTC/UARTE + blinky marker (done)
P3: TWIM/GPIOTE/PPI + matrix/buttons/sensor (done)
P4: SAADC/TEMP/RNG/PWM/PDM/QSPI/USBD + CODAL (done, minus CODAL build)
P5: RADIO + browser page sweep (done: stubs + demo page)
P6: EASYDMA take/complete + PPI dispatch + USBD reset + RADIO loopback (done)
P7: SVD validation + IRQ audit + remaining stubs + cleanup (done)
P8: real-world firmware gate (done, see below)

## 8. MicroPython v2.1.1 boot findings (2026-09-11, probe, not committed)

Image: official release hex (SoftDevice + app, 450KB) split with
`blinky/hex2bin.py` (handles type-02 segments + UICR extras) into a
512KB flash bin + UICR NRFFW words (`0x10001014: 00070700 0007e000`).
Release: MicroPython v2.1.1
(github.com/microbit-foundation/micropython-microbit-v2/releases —
re-download per session, /tmp is reaped; do NOT commit the 512KB blob).

Observed over 60M+ instructions, zero CPU faults:
- MBR/SD handoff issues two `SYSRESETREQ`s (AIRCR wait-loop); the driver
  must honor `is_watchdog_reset_requested()` by rebooting from the vector
  table (covered by `sysresetreq_latches_reboot_request`). UICR NRFFW must
  be seeded or the first stage misbehaves.
- App reaches main firmware: manages NVIC ISER (UARTE0/GPIOTE/SAADC/
  TIMER1/TIMER3), no faults, parks in an SVC-driven SoftDevice wait loop.
- No timer IRQ ever fires (SysTick/RTC0 never configured that far), no
  USBD pullup (REPL is USB-only), no matrix/NVMC traffic.

Verdict: boot chain works; REPL + scheduler progress are gated on P9.

## 9. Next (P9): SoftDevice-event + USB endpoint modeling

P9a USBD endpoint DMA: DONE (EPIN/EPOUT take-complete, SETUP inject,
`usbep_nrf` firmware proof). MicroPython never pulls USB up that far,
so no live traffic yet — expected until the app reaches USB init.

P9b MicroPython steady-state forensics (2026-09-11, read-histogram probe):
- The 0x26048 loop is a 64-bit deadline busy-wait fed by TIMER0
  CAPTURE-then-read-CC3 (no COUNTER register exists). Our CAPTURE stub
  was a no-op -> frozen time -> infinite wait. FIXED (TimerNrf captures
  `counter` now; `capture_snapshots_counter` regression test).
- Same class of bug fixed in SysTick: VAL never counted, COUNTFLAG
  missing. Now a real down-counter (INSTRUCTION_COUNT-partitioned).
- After the CAPTURE fix the firmware advances to RAM execution
  (0x200021BA), matrix rows live, NVIC IRQs managed (UARTE0/GPIOTE/
  SAADC/TIMER1/TIMER3), zero faults over 700M+ instructions.
- VECTACTIVE sampling proves live IRQ delivery: TIMER1 handler
  (vector 25) entered ×27 per window, TIMER3/4 firing. The main thread
  idles;BUTTON/USB/UART stimuli don't advance it further yet.
- Remaining gate for REPL: USB stack start (pullup never asserted that
  far) + SoftDevice event pump (`sd_evt_get` has nothing to return —
  separate project per scope lock, NOT faked).

P9c left: EASYDMA completion IRQs (INTEN-gated), NVMC erase staging.

## 10. P10: POWER/USB readiness + MPY steady state (2026-09-11)

- POWER model completed from SVD ground truth: USBDETECTED/USBPWRRDY
  (gated on USBD ENABLE), USBREGSTATUS VBUS+OUTPUTRDY, RESETREAS with
  SREQ latched on AIRCR reboot (write-1-clear), GPREGRET retention,
  RAMSTATUS/MAINREGSTATUS, LFCLKSTAT/SRC, INTEN. Fixes a real boot
  hazard class (TinyUSB gates attach on USBPWRRDY).
- MicroPython steady state after the CAPTURE fix: RAM execution,
  matrix live, TIMER1/3/4 IRQs delivering (VECTACTIVE-proven), zero
  faults over 700M+ instr. It never enables USBD in that window, so no
  host traffic exists to drive: REPL needs a USB *host* stack
  (enumeration against TinyUSB) — that is P11, and it is bounded work
  (SETUP/DATA/STATUS stages on the P9a endpoint primitives), not a
  peripheral gap.
- SoftDevice event pump: NOT needed for REPL-idle (300k-PC trace shows
  zero SVCs in steady state). Stays out of scope per §0; RADIO loopback
  covers bare-metal BLE.
- CODAL full build: blocked on orchestration, not sources. Needs the
  `codal` python tool (pip offline here), codal-core + nRF5-SDK deps,
  and era GCC (repo targets ~GCC 9/10; ours is 14.2.1/7-2017q4).
  Recipe: pip install codal, clone codal-microbit-v2 + build profile,
  `codal build`. Our GCC IRQ/C++ path is already proven by P8a's C demo.

## 11. Next (P11): USB host enumeration -> REPL banner

Drive SETUP/DATA/STATUS against the P9a primitives until TinyUSB
enumerates and the MicroPython banner appears on USB CDC. Pure driver
work (test-side first, then demo pump); no new peripheral registers.

## 12. P11a TWIM bus errors + UARTE RX truth (2026-09-11)

- TWIM mistakes fixed from SVD ground truth: RXSTARTED/TXSTARTED were
  SWAPPED (0x148=SUSPENDED, 0x14C=RXSTARTED, 0x150=TXSTARTED), LASTRX/
  LASTTX/SUSPEND/RESUME/SHORTS/ERRORSRC missing. Added + SHORTS chains
  (LASTTX_STOP/STARTRX/SUSPEND, LASTRX_*) and a NACK model: a DMA
  transfer with no tap slave at ADDRESS fails after the address phase
  (~6000 instr) with ERROR + ANACK + STOPPED, like silicon without ACK.
  MicroPython's accel probe NACKs live without a slave and proceeds.
- Virtual LSM303 (WHO_AM_I_A 0x33 / _M 0x40) answers MPY's probe:
  periodic I2C traffic serviced, zero faults. Sensor-data path proven.
- UARTE RXD is a single slot, not a queue: unread arrival = OVERRUN
  (ERRORSRC bit 0, write-1-clear). Test drivers must pace input to
  RXDRDY-cleared, like silicon.
- MicroPython steady state: alive main loop (sensor reads + timer IRQs
  live, zero faults over 850M+ instr), REPL input consumed on UARTE but
  no output yet (no echo/prompt). Display never configured, USB never
  started. Next: find what the REPL waits on (BLE-NUS vs USB-CDC binding).

## 13. P11b REPL forensics (2026-09-11, source-grounded)
- The v2.1.1 firmware IS CODAL-based (MicroPython on micro:bit via
  CODAL): REPL stdin/stdout = `uBit.serial` = UARTE0
  (`src/codal_app/mphalport.cpp`), NOT USB. "MicroBitUART" is the
  `microbit.uart` module type name, not BLE.
- UIPM is I2C to the interface chip (addr 0x70, `MicroBitPowerManager`),
  gated on the irq1 GPIO line — silent in emulation, correctly so;
  its retries terminate on working time, not a stall.
- Park anatomy: main fiber in a `delay_us` RAM leaf called from a
  retry/parser path (0x209AA retry x20 + digit parser at 0x20A9A+),
  with live sensor traffic and timer IRQs. I2C log shows clean sensor
  init (WHO_AM_I, CTRL writes) — virtual LSM303 answers correctly.
- Open: which init phase owns that retry loop, and what the REPL
  thread waits on. Candidates: display init ordering, BLE-disabled
  pairing check (`MICROBIT_BLE_PAIRING_MODE=1` in codal.json despite
  `MICROBIT_BLE_ENABLED=0`), button polarity (our inputs default LOW;
  real board pulls buttons HIGH — pressed reads LOW, so we may report
  both buttons stuck pressed!). Button default polarity is next to verify.

## 14. P12 stub completions (2026-09-11)
- ECB: real offsets (STARTECB/STOPECB/ENDECB/ERRORECB/INTEN, DATAPTR),
  take/complete driver flow, FIPS-197 AES-128 round-trip proof
  (`nrf_ecb_aes128_fips_vector`). Crypto itself runs driver-side
  (model has no RAM handle); AAR likewise (take/complete +
  RESOLVED/NOTRESOLVED). CCM stays unmodeled (shares AAR's base with a
  different task map — aliasing would lie about both).
- PPI groups: CHG[0..5] masks + TASKS_CHG[n].EN/DIS; FORK field removed
  (no register mapping exists to ever set it).
- QSPI backend: registered image, AND-only program, 4K/64K erase,
  take/complete for read/write/erase + JS exports.
- Deferred with reason: WDT expiry (a wrong deadline breaks running
  firmware; needs a WDT-pet firmware proof first), I2S streaming
  (needs sample-source infra stripped in P7c; zero consumers).

## 15. P13 REPL end-to-end (2026-09-11): input path proven, app gated on SD

- NRF52Serial source (codal-nrf52, cloned): TX is 1-byte DMA per char,
  RX drains via DMA ring (`dataReceivedDMA` from RXD.PTR RAM) + RXDRDY
  IRQ. Input bytes only reach the REPL through the DMA ring — the
  `uarte_take_rxdma/complete_rxdma` driver API exists for exactly this
  (demo pump wires it).
- Live proof the app never prints: 200M+ instr with full RX+TX DMA
  pumping, `take_txdma` never stages, TXD byte never written. Not a
  model gap on the output path.
- IPR words prove the SOFTDEVICE IS ENABLED (app priorities 2/1 in
  TIMER/UARTE slots — SD-reserved pattern, not CODAL MicroBit::init
  values). Main()'s `NVIC_SetVector(RADIO)` never lands (live vector
  == flash vector), RXD.PTR stays 0, display DIR stays 0: uBit.init()
  never completes; the app parks waiting on SoftDevice events.
- 300k-PC trace: ZERO SVCs in steady state (the wait is not an SD SVC
  spin), zero WFI-sleep, live TIMER1/3/4 IRQs (VECTACTIVE-proven).
- Verdict: every peripheral + driver path on the REPL route is modeled
  and proven. Progress now requires SoftDevice event synthesis
  (sd_evt_get responses, BLE/RTC event pump) — the explicitly
  out-of-scope workstream from §0. Nothing further is actionable
  without it.

## 16. P14 firmware breadth (2026-09-11): Espruino + MakeCode boot

- CoreSight PID space mapped read-as-0 (`0xF0000000-0xF0001000`):
  Espruino HardFaulted at boot on `BFAR=0xF0000FE0` (Nordic's own
  `system_nrf52.c` reads `PID_REGS` there for errata checks — real
  silicon answers, so faulting was an emulator hole, not correct
  behavior). After the fix it boots with GPIO activity, no faults.
- Espruino 2v29 (prebuilt hex) console is BIT-BANG serial on P0.06
  (9600 8N1 per MICROBIT2.py), not UARTE — needs edge-decode to
  observe; nothing transmitted in the first windows.
- MakeCode TypeScript compiles LOCALLY: `npm i -g pxt` works (npm
  registry open, unlike pip), `pxt target microbit` + `pxt build`
  emits `mbcodal-binary.hex` (CODAL, SD-style table) with xpack GCC.
  Both a full program (showString+forever) and a minimal one boot to
  the same live-idle shape as MicroPython (RAM delay leaf, timer
  IRQs, sensor-capable I2C): all three real runtimes share one init
  gate, still unidentified, with zero faults everywhere.
- UARTE RXDMA take/complete API added (NRF52Serial drains RX
  exclusively through the DMA ring in RAM — bytes in RXD alone never
  reach any consumer); demo pump + UART input box wired for it.

## 17. P16 MicroPython REPL: banner live (2026-09-11)

MicroPython v2.1.2 (`micropython-microbit-v2.1.2.hex`, GitHub release
asset) boots and PRINTS (`uart="MicroPython v1.18 o[n] 2023-10-30;
micro:[b]it v2.1.2 with nRF5[2]833\r\nType "help()" [f]or more
information"` — a few single-byte TX drops, pump-side, cosmetic).
This SUPERSEDES the §15 verdict (uBit.init() DOES complete now).

Image layout (Intel HEX, seg+linear records): MBR `0x0-0xAFF`,
S140 SD `0x1000-0x1B3FF`, app `0x1C000-0x67A4B` (VT SP=`0x20020000`,
Reset=`0x29C51`), layout table `0x67FC0`, bootloader `0x77000-0x7D3EB`,
settings `0x7E000`, UICR `BOOTLOADERADDR=0x77000` + settings=`0x7E000`.
Bootloader entered from MBR but reset-loops (`NVIC_SystemReset` at
`0x77492` after NVMC waits + `PSELRESET[1]=18` write at `0x10001204`,
and a second site at `0x783F0`); MBR re-enters BL instead of SD —
bootloader-logic puzzle, bypassed by booting the app at `0x1C000`
directly (as in prior forensics).

Harness facts (all required, all minimal stubs per AGENTS.md):
- UICR seeds (`0x10001014/0x18`), `deliver_irqs=true` (SVCs
  everywhere: bootloader `SVC 24` at `0x7A278`, app `SVC 0x13`
  = sd init at `0x550F8`), honor 2 SYSRESETREQs (SD-enable handshake).
- MBR param page hand-install (a real MBR->SD boot installs these):
  `*(0x20000000)=0x1000` (SD base; without it non-24 SVCs forward to
  `*(0)+44` = self-loop storm, thread 0%) and `*(0x20000004)=0x1C000`
  (app base for SD chained dispatch at `0xB07C`; without it TIMER1
  IRQ ping-pongs MBR<->SD forever). MBR SVC dispatcher at `0xAA4`
  (`cmp #24`: `==24` -> MBR `sd_mbr_command` at `0x377`, else SD).
- Sleep-aware driver pump (REAL EMULATOR/HARNESS GAP FOUND): plain
  `run()` breaks on `sleeping` and nothing wakes it — firmware that
  WFE-idles naps forever in harness (all "stable park" readings
  before this were the coma, not firmware). Silicon advances time in
  sleep and wakes on pending IRQ: `run slice + tick + (asleep ?
  bump INSTRUCTION_COUNT + tick + wake-if-pending)`. Demo `frame()`
  fixed the same way (`tick_n` + `has_pending_interrupt` + `wake()`).
- Driver duties per slice: UARTE TXDMA take/read/complete (banner
  bytes!), RXDMA fill+complete (input), NVMC erase apply+complete
  (`eraseUserStorage` stages `0x74000`).

Healthy-state evidence: thread 100% (`4000/4000`), TIMER1 time flows
(`CAPTURE[3]` delta `15580`/1M), radio vector repointed to app
(`0x2E44D` = main() line 57 ran), serial RXDMA armed
(`RXD.PTR=0x20002BE8`, 32B, baud set), SD dispatch chain live
(MBR fwd `0x7B4` -> SD `0xB064` -> app SVC handler `0x29AF0`).

Residual (bounded follow-ups, NOT blockers of this commit):
- Deterministic post-banner NULL-dispatch fault ~179M instr (input-
  independent): `bx r3` with `r3=0` at `0x4F75A` (`r3=*(*(r0+0x924)
  +0x28)`, `r0=0`), chain `...51A61->51A3D->51921->4F6E7->fault`
  (nested `blx r4` virtual/interface calls, serial region). Between
  help-text and `>>>` prompt. Needs CODAL-DAL source matching.
- TX drops (~3% single bytes): firmware SYNC_SPINWAITs on ENDTX so
  overrun is impossible; cause not yet isolated (suspect DMA buffer
  reuse vs take/complete interleave). Fine-grained demo pump should
  confirm.
- Bootloader reset-loop (full MBR->BL->SD->app chain) still open;
  direct-app boot + param seeds is the working recipe meanwhile.

## 18. P17 REPL fault + browser demo + MakeCode status (2026-09-11)

Post-banner NULL-dispatch fault (from P16) root-caused as far as
possible without symbols, then parked; browser demo finished;
MakeCode blocked on network, bootloader parked with a concrete next
step. Temp probes reverted (suite file-free again, 163 green).

Fault (`bx r3`, r3=0 at `0x4F75A`, `r3=*(*(r0+0x924)+0x28)`, r0=0):
- Input-INDEPENDENT (fires with no input), tick-DELIVERED (vanishes
  with TIMER1 IRQs stopped), deterministic ~179M instr post-boot.
- Patching it to `bx lr` in a scratch image: no fault, system runs a
  healthy UARTE-IRQ cycle (RX drain -> app SVC -> SD -> repeat), but
  no prompt/echo/exec either (the skipped call is load-bearing for
  progress, or a second stall follows it).
- Entry path (10k sampling): wfe-idle -> `0x24BE0` -> `0x50312` ->
  `0x57038` (memmove/memcmp) -> `0x322CA` (list walk) -> `0x2EEDA`
  (0xFE-sentinel table) -> `0x3Axxx` (bfi event packing?) -> ring
  reader `0x50F34` (serial RX ring drain) -> `0x50FAE` -> sleep-ret
  `0x4D938` -> NULL call. All thread mode.
- The prompt IS composed (`"...information.\r\n>>> "` in heap TX
  buffer `0x200169A0`) but never DMA-staged; input bytes DO land in
  the DMA buffer (`"print(1+2)\r"` observed at `RXD.PTR`); RX ring
  (`rxBuffSize=129` @`0x20002BC8`) stays empty.
- RX feed lesson (model contract!): NRF52Serial re-arms STARTRX only
  via enableInterrupt; completing a staged 32B RXDMA early (ENDRX
  after 1 byte) kills the transfer with no re-arm -> input stalls
  after 1 byte. Correct trickle protocol: mirror each byte into
  `RXD.PTR+AMOUNT` + `uart_rx_byte` (RXDRDY path), never
  take/complete until full. Demo pump fixed accordingly.
- TX drops (~3% single bytes) persist with slicetimed completion:
  not a pump race; bytes missing from firmware buffers at DMA time
  (heap/buffer reuse suspected, unproven).
- Serial object hunt: CodalComponent id==12 scan hits the component
  TABLE (24B entries, dynamic ids 110+), not uBit.serial; dynamic
  stacks (`[502D7,1E5EF]`, fiber clusters) are stale slots, not
  frames -- stack-scan heuristics need bl-preceded validation.
- Prime suspects left: (a) RX_BUFF_INIT clear (setRxBufferSize malloc
  failed at main:61 -- never verified: need Serial object base +
  status bit read); (b) TX ring stall (prompt queued, is_tx state
  unknown -- same layout block); (c) dangling messageBus/fiber
  listener on the 6ms tick. Next step: compute NRF52Serial field
  offsets from codal-core/codal-nrf52 headers (cloned to /tmp/src)
  and read status/head/tail/is_tx directly. Image patch
  (`0x4F75A: 0x4718->0x4770`) is DIAGNOSTIC ONLY, in /tmp, never
  committed.

Browser demo (demo/index.html, JS-checked + smoke green, no pkg
rebuild needed -- all exports pre-existed):
- parseHex: type-02 segment records + 0xFF fill (erased flash reads
  as FF, old code zero-filled!) + UICR capture (applied via
  periph_write words on boot; old code aliased UICR into flash!).
- Buttons fixed to active-low (were inverted vs the pull-up model).
- Sleep-aware frame (`tick_n` + wake on pending IRQ -- without this
  the core naps forever at first MicroPython idle, proven in P16).
- RX drip fixed (DMA-buffer mirror, no early ENDRX completion).
- I2S RX-silence/TX-capture pump wired.
- "Boot MicroPython app" button: app VT at 0x1C000 + MBR param
  seeds (P16 recipe). Load any microbit-micropython-v2.x hex, boot
  app, banner follows in the UART box.

Bootloader: BL entered from MBR (UICR seed) but reset-loops; two
NVIC_SystemReset sites (`0x77492` after NVMC waits + PSELRESET[1]=18
write, `0x783F0`); MBR re-enters BL (BOOTLOADERADDR set). Only MBR
param write observed: `*(0x20000000)=0x77000`. Hand disassembly past
`0x782E8` is alignment-unreliable (literal pools decode as code).
Next step: anchor disassembly at the BL reset vector (`0x77000`
VT) and trace forward, or sample BL-window PCs densely and decode
only true boundaries; capture r0 at the init-runner `cmp` to read
the NRF_ERROR code (0x8502/0x8514 family seen statically).

MakeCode: BLOCKED on network, not on depends. `npm i -g pxt` installs
a 2018 squatter (0.5.1, useless); the real CLI is the `makecode`
package (`makecode`/`mkc` bins, works). `makecode init microbit`
fails fetching `cdn.makecode.com/.../target.json` (ECONNRESET;
same host class as api.github.com). pxt-microbit cloned from
github (reachable) for source comparison; test hexes present are
v1 (nRF51, wrong arch). NOTE: shell reports `NotFound:
FileSystem.access (<dir>)` when the bash workdir does not exist --
several tool failures this session were that, not tool failures.
Display finding (source-grounded): matrix GPIO configures ONLY on
refresh/strobe (NRF52LedMatrix), constructor just wires TIMER4 --
DIR=0 pre-first-show is CORRECT, not evidence of stuck init.
Recipe when CDN returns: `makecode init microbit`, `makecode
build`, boot `mbcodal-binary.hex` with the P16 recipe (it is also
a CODAL+S140 image).

## 19. P18 REPL serial deep-dive: layout solved, TX_EMPTY prime suspect (2026-09-11)

NRF52Serial/Serial/CodalComponent/PinPeripheral layouts computed from
cloned sources (codal-core, codal-nrf52) and VERIFIED against live RAM
(id==12 at the predicted offset, rxBuffSize==129, rxBuff/txBuff heap
ptrs, bytesProcessed==11, dmaBuffer content). uBit.serial base =
`0x20002BA4`: id@+12, status@+14 (`0x402C`: RX+TX BUFF_INIT BOTH SET),
rxBuff@+32, rxBuffSize@+36, rxBuffHead@+38, rxBuffTail@+40, txBuff@+44,
txBuffSize@+48, txBuffHead@+50, txBuffTail@+52, baudrate@+56,
is_tx@+60, bytesProcessed@+64, dmaBuffer[32]@+68. Temp probes fully
reverted (163 green, suite file-free again).

Settled facts:
- Banner+help print via 1-byte TXDMAs from ONE stack slot
  (`0x2001FEC7`, putc `&c`), all len=1. TX drops are STAGED-wrong
  (isolated N+1 substitutions, self-correcting, no shift), NOT pump
  read-timing (eager per-slice reads) or completion-timing (delayed
  ENDTX) artifacts. STOPTX model arm completes (ENDTX) so no loss
  there; no stack/heap collision (min SP `0x2001FD98`, heap top
  `0x20016xxx`). Mechanism of N+1 staging still open (is_tx gating
  audited sound in every path examined).
- Input bytes land in dmaBuffer (`"print(1+2)\r"` observed) and
  bytesProcessed counts them (11); RX ring stays empty, readline
  never consumes (tail frozen). RX feed MUST use the RXDRDY path
  (mirror byte to `RXD.PTR+AMOUNT` + `uart_rx_byte`); bulk
  take/complete_rxdma ends the 32B transfer with no firmware re-arm
  (input stalls after 1 byte -- demo pump fixed for this).
- Prompt composed in TX ring (`"...information.\r\n>>> "`) but never
  DMA-staged; TX ring empty at end (head==tail); is_tx false.
- Unpatched fault correlates with TX RING DRAIN-COMPLETION:
  txHead==txTail exactly when it fires (~179M, after help text).
  Prime suspect: `Serial::dataTransmitted` fires
  `Event(DEVICE_ID_NOTIFY, CODAL_SERIAL_EVT_TX_EMPTY)` when the ring
  empties (source line confirmed) into a NULL/dangling bus listener
  (fault is a C++ virtual through NULL, serial/MP region). Patching
  the dispatch to `bx lr` neutralizes survival but progress still
  stalls (call load-bearing or second stall follows).
- Display DIR=0 is CORRECT pre-first-show (matrix GPIO configures
  only on refresh/strobe per NRF52LedMatrix source), not stuck init.
- Sleep coma resolved for harnesses (plain run() breaks on sleeping;
  silicon-faithful pump = run slice + tick + advance time in sleep +
  wake-on-pending; demo frame() fixed the same way).

Next moves (ordered): (1) walk messageBus listeners at fault for the
NULL TX_EMPTY recipient (listener list head is the prize); (2) verify
TX ring drain->Event->fault causation by stuffing the ring (never
empty -> no Event -> no fault?); (3) identify the N+1 stager via
is_tx/ENDTX event timeline around a hole (log ev_endtx transitions
per slice near banner mid-point).

## 20. P19 MakeCode boots: display refreshes blank (2026-09-11)

Real MakeCode build locally (`makecode` CLI from the `makecode` npm
package -- NOT the squatter `pxt@0.5.1`; `makecode init microbit`
needs `cdn.makecode.com`, fails ECONNRESET when that host class is
down). `basic.showString("A")` -> `built/mbcodal-binary.hex` =
MBR + S140 + CODAL app @`0x1C000` (VT SP=`0x20020000`,
Reset=`0x37F25`) + bootloader + settings + UICR, same shape as MPY.
Boots with the P16 recipe (app VT + UICR seeds + MBR params
`0x1000`/`0x1C000`, reset-honoring, sleep-aware pump): zero faults
over 400M instr, 2 SD-handshake resets, scheduler idles in RAM
(`0x20002078`), then app-flash sleep leaf (`0x37AF8: wfe; bx lr`).

Display finding: TIMER4 armed (INTEN COMPARE0, CC0=`0xD055` ~107ms
period) and P0 OUT latch strobes rows {22,24,15,...} on that period
(TIMER4-tick-driven NRF52LedMatrix refresh, CONFIRMED running), but
cols never set and P0/P1 DIR stay 0 -> renders BLANK (also true on
silicon: no DIR, no light). showString content never reaches the
matrix; no radio attempts (RADIO state 0), SD canary 0, same as MPY
pre-banner. A `codal.json` BLE-disabled rebuild behaves IDENTICALLY
(BLE is not the gate). Constant OUT bits {8,16,20} are idle I2C/mic
latches, not display. Next: why the animation/frame never advances
(fiber/event-gated scroll? needs the TX_EMPTY-class answer from P18
applied to the display path?).

Demo correctness fix (same commit): matrix LEDs now AND DIR with OUT
(a toggling latch on an input pin stays dark, as on silicon;
previously OUT-only, which would ghost on pre-show refresh).

## 21. P20 REPL in real Chrome: banner+prompt, no fault (2026-09-11)

Headless Chrome (cached Playwright chromium-1234 + playwright-core
over system Chrome/Firefox present) drives demo/index.html end to
end: MicroPython hex loads, "Boot MicroPython app" boots, and at
~150s the UART box shows the banner, help text AND the `>>> ` prompt
-- with NO CPU fault over 240s+. The native NULL fault does not fire
in the browser (pump-timing dependent, see below). Input typed in
`#uartIn` does not yet echo/execute (same pre-readline stall as
native; ring holds bytes unconsumed).

Demo bugs fixed same commit (all required for the above):
- `set_deliver_irqs(true)` was never called: first SVC faulted
  silently (no fault surfacing existed). Added the call + fault line
  in status via `fault_pc/fault_op1`.
- Watchdog reboots reloaded MBR vectors, losing a direct-app boot
  back to the bootloader loop; pumpDma now resets to the app table
  when an app boot is active (`appBoot`, cleared on fresh load).

Model fix (SVD-grounded, regression-tested): TASKS_STOPTX raised
ENDTX; silicon raises only TXSTOPPED (`0x158`, INTEN 22). The old
behavior self-triggers an ENDTX ISR loop. New unit test
`stoptx_raises_txstopped_not_endtx`. (Did not by itself kill the
native NULL fault -- that fault is timing-sensitive, see P18.)

TX-drop forensics (cosmetic, open): full per-transfer log (ptr, len,
bytes) shows isolated N+1 substitutions, self-correcting, no shift.
DMA source is putc's 1B stack slot (`0x2001FEC7` constant across 60+
transfers): putc returns immediately after STARTTX (IRQs on: no post
wait), so the slot is reusable before slow pumps take; is_tx gating
was audited sound in every firmware path examined, yet substitution
persists -- stager unidentified. Prompt bytes sit composed-but-unstaged
in the TX ring with head==tail and is_tx false (no kick source found).
RX trickle protocol (mirror byte to `RXD.PTR+AMOUNT` + `uart_rx_byte`,
never early-complete) is validated: 11/11 bytes land and count.
