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

## 22. P21 serial layout map + readline stall (2026-09-11)

uBit.serial object map (computed from codal-core/codal-nrf52 headers,
VERIFIED field-by-field against live RAM; base `0x20002BA4`): tx@+16,
rx@+20, delimeters@+24, rxBuffHeadMatch@+28 (`-1`), rxBuff@+32,
rxBuffSize@+36 (`129`), rxBuffHead@+38, rxBuffTail@+40, txBuff@+44,
txBuffSize@+48 (`21`), txBuffHead@+50, txBuffTail@+52, baudrate@+56,
is_tx@+60, bytesProcessed@+64, dmaBuffer[32]@+68, p_uarte_@+100
(`0x40002000`). CodalComponent id@+12 (`12` = DEVICE_ID_SERIAL),
status@+14 (`0x402C`: RX+TX BUFF_INIT both set).

Readline stall, nailed down: 11/11 input bytes reach dmaBuffer and
the codal ring (head=11) with correct content (`"print(1+2)\r"`),
tail frozen at 0, no echo, no exec, thread sleeps cleanly, no fault
(patched image). readline never reads despite `isReadable()` being
true by the book (`tail != head`). Post-feed thread sampling:
100% UARTE-IRQ cycling (closed handler loop, never returns to
thread) in native runs. `isReadable`/`read` source-audited clean;
`mp_hal_stdin_rx_chr` would consume instantly. Conclusion: the main
thread is NOT in readline (stuck before it, or main fiber dead after
the patched-over NULL call). Prime suspects left: (a) audio fetcher
(`modaudio.c:129` schedules `audio_data_fetcher_wrapper`; speaker
enabled with NO pin selected; fetcher with no source may call NULL
-- fits the MP-call-shaped faultchain entry at `0x518E6`); (b) TX
kick gap (queued prompt + is_tx false + no ENDTX pending + no
enableInterrupt coming = nothing starts the transfer; the first-byte
kick source for an idle ring is unidentified in the sources read so
far); (c) dangling messageBus/fiber listener on the 6ms tick (fault
vanishes with TIMER1 stopped).

## 23. P22 bootloader breakthrough: stacked-PC + subword fixes, SD runs (2026-09-11)

Two REAL emulator bugs found via the reset loop, both fixed with
regression tests (suite 167 green):

1. **Stacked return PC leaked the Thumb bit** (`cpu/mod.rs` stacked
   raw `r[15]`; fix: `& !1`; test
   `exception_svc_stacks_even_return_pc`, cpu_bug.md §2). The MBR SVC
   dispatcher reads `[stackedPC-2]` for the SVC number: for
   `svc 24` at `0x7A278` it needs stacked `0x7A27A`
   (`[0x7A278]` = `0x18`); we stacked `0x7A27B`, it read `[0x7A279]`
   (`0xDF`=223), took the unknown-SVC path, `sd_mbr_command`
   "failed" with 1, and the bootloader reset-looped forever
   (MBR -> BL -> failed `sd_mbr_command` -> reset). With the fix the
   SVC returns `NRF_SUCCESS` (0) and the SoftDevice INITIALIZES
   (canary `0xCAFEBABE` at `0x20000058`, SD base registered): SD code
   executes, multiple SVCs dispatch, BL<->SD interoperate.
2. **Subword peripheral reads shifted the wrong way**
   (`Peripherals::read` did `value << 8*off`; callers truncate, so
   every byte/halfword read past offset 0 returned 0; fix: `>>`;
   test `subword_reads_shift_down`). Caught via the BL's `ldrb` of
   NVIC IPR22 (`0xE000E416`); the write path was already correct.
   (Did not unblock the BL by itself -- IPR22 is genuinely 0.)

Full chain NOT yet closed: post-success the BL still resets
(`0x783FE` park) and MBR still prefers the valid bootloader
(empirically both directions; no-BL boots direct to APP). Ruled out:
WDT (never started), BL I2C/UIPM traffic (none), RESETREAS/GPREGRET/
UICR/FICR semantics (faithful), MBRPARAM content (only SD base).
The reset decision reads NVIC IPR22 (=0, takes the `r0=0x2002`
path into a memcpy-ish routine, then resets). Artifacts (scratch
image patch, all temp probes) reverted, never committed.

## 24. P23 no-BL path + reset-persistence finding (2026-09-11)

No-bootloader MBR boot (BOOTLOADERADDR erased) routes MBR -> SD-Reset
(`0x1AE20`) -> APP directly (traced region samples; SD validates
bases, sets a flag word, runs two init calls, `svc 255`). The APP
then issues its own handshake resets and, on the 3rd attempt, faults
in `memcmp` (`0x56FF4`, BFAR `0xFFFFCFFF`) on a garbage SD-struct
pointer -- same signature as the early direct-app fault, but the
direct-app recipe banners while no-BL faults. MBRPARAM words intact,
timers healthy, NVIC/VTOR clean, UICR seeded, clocks assumed (not the
differentiator checked last).

Key negative result: reinstalling FRESH peripherals (+NVIC/VTOR
clear) on every reboot makes it WORSE (instant reset storm), while
dirty-peripheral continuity lets attempts progress. Evidence that
nRF SYSRESETREQ preserves peripheral state and the SD handshake
depends on it (timers keep running so later boots short-circuit).
Consequence: `cpu.reset` staying CPU-only is CORRECT-ish, not a gap;
do NOT "fix" it by resetting peripherals (would break the working
handshake). Open sub-question: exact VTOR/NVIC reset semantics of
SYSRESETREQ on nRF52833 (ARM core says clear; whether Nordic's
reset controller clears NVIC enables is unverified -- self-heals in
practice since firmware re-inits).

npm: `microbit-v2-emulator@0.1.0` publish BLOCKED (registry 401, no
credentials in this environment; `npm publish` from demo/ when
authenticated).

## 25. P24 fault-path narrowing + browser-stall rooting (2026-09-12)

Listener walk (tasks 1+2), all verified against fresh
lancaster-university/codal-core + codal-microbit-v2 checkouts:
- `0x52C2E` = MemberFunctionCallback::fire (`ldr r5,[r4,#20]` is the
  `invoke` ptr: object@0, method[4]@4, invoke@20 — exact match), called
  from the queue-drain loop (`bl 0x52C2E` @`0x51976`, ret `0x5197B`).
  The bus/listener machinery is HEALTHY; the NULL is born downstream.
- Fault fn `0x4F74C`: `ldr r0,[r0,#2340]; ldr r3,[r0]; ldr r3,[r3,#40];
  bx r3` (C++ virtual, slot 10). At fault r0=0: the OBJECT is NULL
  (reads alias flash `0x924`/`0x28`, dies on the null slot).
- Caller `0x4F690` = mp_call_function-shaped (`blx r4` @`0x4F6C8`).
- Below the fault, repeating dispatch records for a GPIO pin-TOGGLE
  virtual (`0x28744`: pin# @obj+16, toggles `GPIO->OUT`): obj
  `0x20002D94`, vtable `0x57A1C`, pin# 0 (= P0.00 = speaker), nested x2.
  Tick fan-out drives speaker toggles; MP NULL-call sits above. P21
  suspect (a) (audio fetcher, no source) still fits best.
- Listener-shape RAM scan was too loose (false positives); frame-math
  unwinding needs exact prologues — both documented dead ends. NEXT:
  capture SVC40 args / identify the firing (id,value) at the drain.

Prompt-first (task 3): tick-pause can't work (TX staging needs the
tick); eager same-call TX completion changes nothing (fault ~177M,
no prompt on wire either way). The NULL call always wins natively.

Browser stall (task 3 fallout), ROOTED though not yet fixed:
- Measured only ~300K instr/s in this Chromium (both old and fresh
  pkg stall identically; parts-off stalls too; image/UICR/params all
  byte-verified identical to native). Native banner threshold is
  ~150-260M, so 480s runs (~144M) simply never arrive. P20's 150s
  banner = faster environment, not a different build.
- Coarse quanta genuinely slow progress per instruction (5K: banner
  ~180M; 20K: "Mic"@200M; 100K: nothing@260M) — tick-starved waits
  burn instructions spinning. Demo pump changed to 20x5K+tick
  (matches validated native quantum; duties once per frame).
- Demo dwells at an NVMC-op spin (`0x215AC`, waits `*0x20004A9E`;
  NVMC idle/READY=1/CONFIG=0, flag never sets) while native dwells
  at `0x200021B9` then banners. The spin entry calls SVC40
  (`0x4EBEC` = `svc 40` + return): SD returns 0 in-demo (wait path)
  vs nonzero natively (delay+retry path, eventually banners).
  Pre-roll (2M MBR/BL), TX policy, drip, parts, UICR, image all
  EXCLUDED as the differentiator. NEXT: capture SVC40's r0 sequence
  natively (100K sampling too coarse — needs an SVC hook or finer
  quanta) and identify SD SVC#40's gating state; then read that
  state in-demo.
- OPEN perf item: verify pkg profile (dev vs --release; both builds
  sized ~1.55MB, inconclusive) — release WASM should be several x
  faster and would bring the banner into comfortable wall time.

BL entry anchored (4a, 2026-09-12): BL VT @`0x77000` (SP=`0x20020000`,
reset `0x772F9`). Entry copies .data `0x7D320`->`0x20002AE8..B4`,
calls `0x77354` (BL main) + `0x77290`. Main head: `bl 0x77334` check;
on nonzero clears `0x4000010C/110/538`; on second nonzero copies
FICR `0x10000404-444` (DEVICEID/ER/IR/DEVICEADDR) to RAM struct
(`*(0x774B0)`+`0x520`…). Continues at `0x77404` toward the IPR22
check / `0x783FE` park (P22). NEXT: trace `0x77404`→IPR22→park,
capture r0 at the init-runner cmp.

## 26. P26 SD-SVC decode + waiter-chain + pump-sensitivity + blinky (2026-09-12)

SVC numbers decoded from Arduino-nRF52 S132 headers (stable across
S132/S140): SVC18 = sd_softdevice_is_enabled (SDM 0x10+2), SVC40 =
sd_flash_page_erase (SoC 0x20+8), SVC41 = sd_flash_write. The
0x21578-fn: NVMC CONFIG/READY dance + SVC18(is_enabled?) into
[sp,#7]; 0 → skip (`0x215B8`); nonzero → SVC40(erase) → wait
`*0x20004A9E` (SD-event completion — never comes: sd_evt_get is out
of scope). Native SVC18 log (temp hook, reverted): 10 calls, all
skip, zero SVC40 over full boot — native never enables the SD.
Flash page 0x74000 verified erased in-image and never written
natively; demo stuck-state reads it erased too.

Demo-vs-native, settled findings:
- Blinky (`BOOT/BLINK` via UARTE+GPIO) works in the demo (<10s):
  WASM execution is equivalent to native. Divergence is
  MPY-state-specific, not a WASM gap.
- Demo stack capture at the spin: `bl 0x215A2` (waiter mid-entry,
  r0=0) with r4=`0x74000` (flash page!) and returns
  `0x215A3/0x25633/0x263FD/0x23775`. 0x502F8-subtree = heap free-list
  (0x80000000 busy-bit) i.e. malloc inside an fds/flash-write path;
  no direct `bl` to the waiter anywhere in flash (register call),
  no flash vtable holds `0x215A3` (runtime-constructed pointer).
  NEXT: widen the captured backtrace below 0x25633 (256B window
  overflowed) to name the exact fds caller.
- Pre-roll (2M/8M/20M MBR/BL), TX policy, duty latency, drip, parts,
  UICR, image, MBR params, watchdog handling, USBD duties, RAM-clear
  at reset: ALL excluded (native still banners+faults identically).
  SVC40-hook pkg + svc_hook_drain export verified live in-browser
  (zero SVC16/18/40 over 120s of spinning) — then fully reverted
  (hook, export, instrumentation, probes; suite 185 green; pkg
  rebuilt clean, hash-matches pre-hook bytes).
- Pump-sensitivity (real, open): the 20x5K demo pump deterministically
  faults at app entry (`0x29C7A`, `op=0xDEAD`) while 1x20K spins at
  the waiter with no fault. Tick density changes app-entry behavior;
  mechanism unknown (stale-IRQ-during-entry is the prime suspect but
  bootImage re-inits, so unproven). REVERTED to 1x20K (this diff);
  do not re-land fine-grained pumping without explaining the entry
  fault.
- Pkg profile question closed: dev and --release builds are byte-
  identical in this wasm-pack setup (single profile); browser speed
  (~300K/s-1.5M/s here) is environmental. Banner needs ~150-260M:
  600s+ wall in this Chromium; P20 = faster machine.

## 27. P27 entry-fault + waiter-caller + sd_evt design (2026-09-12)

Entry fault (`0x29C7A`/`op=0xDEAD`, browser-only): narrowed to a
HardFault-sled double-fault lockup (raise_sync overwrites the first
fault; the sled at app entry IS the Default_Handler). Never
reproduced natively across ~8 configurations (pristine, pre-rolls,
dirty NVIC incl. matured TIMER1/2/4 + pending bits, sensor-answered
pre-roll, exact demo pump order). deliver_irqs has no runtime clearer
— writer audit done. Correlation found but unproven: fault runs all
carried heavy status instrumentation; needs a clean multi-run
classification + first-fault-preserving hook before any pump change.
The 20x5K revert stands.

Waiter caller: demo stack gives waiter ← … ← `0x25633` (heap
free-list/`0x502F8` subtree, i.e. malloc inside the fds path) ←
`0x263FD` (virtual slot-6 call) ← `0x23775`, r4=`0x74000`,
no-SVC16/18/40 anywhere (SVC hook pkg verified live, then reverted).
The fds path calls the completion-waiter directly (not via the
0x21578/SVC18 gate). NEXT: 2048B backtrace on a spin-lottery run.

sd_evt transport designed (`docs/sd_evt_design.md`, phase 1 =
flash events only, no new wasm exports, proof spec included).
SVC/event numbers resolved from S132 headers (SD_EVT_GET=82,
FLASH_SUCCESS=2; verify against S140 binary before implementing).
STATUS §7 updated accordingly.

## 28. P28 SPIM RXD + TX-drop ring diagnosis (2026-09-12, uncommitted)

SPIM fix (working tree): `0x518` RXD now returns the MISO queue for
`SPI*` names, I2C queue for `TWI*` (was I2C for all; TWIS/SPIS keep
their own paths) + `rxd_polling_reads_slave_response_line` test.
Stale "still open" comment corrected. Suite 186 green.

TX drops, diagnosed to the firmware ring (working tree only has the
fix above; probes reverted): per-take log vs flash ground truth gives
holes at takes #19/#39/#59/#79 — every 20 takes, always `0x20`,
aligned, PTR constant, len 1 — under eager AND delayed pumps,
pristine and pre-rolled. Slot-watch shows putc wrote the space itself
(pc `0x50F37`-family); heap txBuff holds mismatching bytes too
(`micro:bn .` window, head 20→19 wrap). NOT an N+1/timing race.
NEXT: audit putc→ring→flush index math against OUR AMOUNT/ENDTX
timing, else accept as firmware-cosmetic (likely silicon-visible).

## 29. P29 BL resets decoded + MC constructor finding (2026-09-12, uncommitted)

BL (all static addresses in `full.bin`, temp src markers reverted):
- reset#0 (`0x774A7`, r1=SCB, r3=AIRCR-magic) = deliberate post-UICR-
  programming reset (design, benign). reset#1 (`0x78405`,
  r0=`0x2002` STALE, lr=`0x78519`) = CODED AIRCR via the 2nd of three
  `bl 0x783EC` sites (`0x7850E/14/24`); the `0x783FE` "park" is the
  post-reset `dsb;nop;b .` wait (silicon-instant, model-deferred up
  to a pump quantum). NOT a WDT expiry (proven by caller-marked
  reset sources). No static IPR22 access exists in the BL (priority-
  table walk at runtime); r0=`0x2002` is stale, not causative.
- Validation dispatch: tbb at `0x78498` on (r0-1), entered when r4==0
  from the `0x7B530`-check (`cbnz r4` at `0x7B636` skips validation);
  r5=1 selects the 2nd reset site. NEXT: why r4==0 (`0x7B5B4`/
  `0x7B568` validators) + the `0x784C4`-flag / r4-vs-59 compare.
- UICR writes observed live (CONFIG=1, `0x10001200/204`=18, CONFIG=0).

TX: putc path runs through C++ virtuals (`0x50F34/0x50F54` via
`0x25Bxx` glue); the lone TXD.PTR-site hit (`0x29D3A`) is a false
positive (FICR-copy code, same offsets different struct). Ring-filler
live-catch blocked by scheduler pacing (micro-steps stall without
sleep+TX servicing; windows miss). NEXT per P28.

MC display: `NRF52LEDMatrix` calls `enable()` IN ITS CONSTRUCTOR, so
TIMER4-zero means construction never reached display (stall is in
power/flash/storage/i2c-probe members per `MicroBit.h` order — not
post-init). Live waiter is `0x30C04` (busy byte `[r4+20]`, sole
`wfe`-caller `0x30C1E`); no direct `bl` to it (register/virtual
call). NEXT: capture r4 at `0x30C18` via pc-triggered reg dump.

## 30. P30 BL reset sources + UICR-gated app halt (2026-09-12, uncommitted)

Reset sources proven distinct (temp caller markers, reverted): reset#0
(`0x774A7`) and reset#1 (`0x78405`) are BOTH AIRCR-coded, never WDT.
reset#0 = post-UICR-programming design reset. reset#1 comes via the
2nd `bl 0x783EC` (`0x78514`, r5=1); the `0x783FE` park is the
post-AIRCR `dsb;nop;b .` wait (model defers up to a quantum).
No static IPR22 access in BL; r0=`0x2002` stale.

UICR replay (native): BL programs exactly `0x10001200/204`=18 (SVD
has no field there — undocumented word). Seeding those two words then
direct-app boot STALLS at `0x29CD1` (400M, no fault, no output) vs
pristine banner+fault. `0x29CD1` is a `b .` park AFTER an
AIRCR-SYSRESETREQ write — but honors never land (8000 entry samples
miss), and no `bl` targets the park or its `0x29CB4` reset-fn, so it
is entered via register as a HALT routine, gated on programmed UICR.
NEXT: find the halt caller (register-called; scan the UICR-gated
branch above the app entry init) + identify UICR+0x200 (PS check).

## 31. P31 MC waiter object + display-channel event (2026-09-12, uncommitted)

PC-entry hook (temp, reverted after) caught the waiter once:
obj=`0x20002D58` (vtable `0x43DC0`, +4=`0x30000007`, +16=`0x7E`,
busy@+20, target@+22, progress@+24), entered with lr=`0x30CFB`.
Busy trajectory: 0 → 7 (~180M) → 0 (~210M, no resume).
Call chain: `0x30CD0`-region → `0x30C30` (starter: sets
target=len/progress=0/busy=7 after a buffered-copy `0x34248`) →
`0x30C04` waiter (fiber-wait-shaped `(7,1)` via `0x2E2E8` + busy poll).
No direct `bl` to waiter/starter (virtual-called); vtable literal
`0x43DC0` absent from flash (runtime-constructed pointer).
Event id 7 = DEVICE_ID_DISPLAY (source headers): the wait is for a
display-channel event that never arrives; TIMER4 vector stuck at the
MBR forwarder `0x869` (display_irq never installed); 30 COMPARE0
events vanish into the SD default. `enable()` runs in the
`NRF52LEDMatrix` constructor, so the stall is pre-display-construct
(power/flash/storage/i2c member phase). NEXT: constructor-order
trace (which member init reaches the `0x30CD0` region) + the (7,1)
producer (TIMER-driven completion of the same transfer?).

## 32. P32 SVC16 sites + sd_evt premise refuted (2026-09-12, uncommitted)

`svc 16` (sd_softdevice_enable) sites: `0x54D4E` (app — BLE init,
post-banner) and `0x7B530` (BL). Nothing enables the SD pre-banner
in ANY configuration (hook: zero SVC16/18/40 in-demo over 120s of
spinning; zero natively). So SD-disabled is UNIVERSAL pre-banner —
yet demo takes the fds-wait path and native skips it. The fds entry
condition reads non-SD state that still differs (fds queue leftovers
excluded 4x via pre-roll; UICR excluded except the two BL words;
NVIC excluded 3x). NEXT: capture the fds-entry compare (which
address does the 0x21578-fn's caller test before `bl 0x215A2` —
needs a spin-lottery run with the entry regs).

sd_evt_design.md premise REFUTED: zero `svc 82` sites in MPY+MC
binaries — no firmware calls sd_evt_get, so an SVC82 hook would be
dead code. The doc stays as a corrected reference (do not build as
specified). The NVMC-IRQ idea is likewise dead (no NVMC IRQ/INTEN
in the SVD — polled READY only, our model already faithful).
Real remaining question is unchanged: what differs demo-vs-native
at the fds gate.

## 33. P33 reset timing + sd_evt shelved (2026-09-12, uncommitted)

Watchdog honors in native boot land at instructions 5000 and 10000
(back-to-back). A per-frame honorer coalesces them (single bool
latch); native per-5K honors both. Disproven as the differentiator:
per-100K honoring (`tmp_delayed_duties`) still banners natively, so
coalescing is benign (both resets target app vectors; redundant).
Demo divergence is NOT reset phasing.

sd_evt phase-1 as designed is SHELVED before building: zero `svc 82`
sites in MPY+MC binaries (no firmware calls sd_evt_get — hook would
be dead code), and NVMC has no IRQ/INTEN in the SVD (polled READY
only; our model already faithful, nothing to fix). The doc stays as
a corrected reference. The live question remains what differs at
the fds gate (SD-disabled universally pre-banner; no SVC16 ever).

## 34. P34 GitHub Pages workflow (2026-09-12, uncommitted)

No workflow existed. Added `.github/workflows/pages.yml` (official
actions only: checkout/configure-pages/upload-pages-artifact/deploy-
pages; triggers on master for `demo/**` + manual dispatch; serves
`demo/` as site root so relative `./pkg`/`./parts` imports resolve;
uses the intentionally-committed wasm pkg, no toolchain needed) plus
`demo/.nojekyll`. YAML validated. Still needed to go live: commit +
push, then repo Settings -> Pages -> Source: "GitHub Actions".

## 35. P35 GPIO CNF->DIR sync fixes display drive (2026-09-12, uncommitted)

Real model gap, found via MakeCode (TIMER4 ISR runs render(), OUT
toggles, but DIR ever 0): PIN_CNF writes stored the register without
syncing `dir[]`, so firmware configuring pins the normal Nordic way
never showed DIR=output (IN reads and the demo's DIR-gate stayed
dark). Fix: sync `dir[port]` bit from PIN_CNF.DIR on every CNF write
(both directions) + `pin_cnf_dir_bit_drives_dir` test. Proof:
MakeCode boot now shows sticky DIR0=`0x01788000`, all five matrix
rows output-driven. MC waiter chain also captured en route (obj
`0x20002D58`, busy 7->0, display-channel event (7,1), starter
`0x30C30`, caller lr `0x30CFB`, TIMER4 vector stuck at MBR forwarder
`0x869`). Remaining MC: scroll completion/content (needs wall time).

## 36. P36 demo presets + MIPS meter (2026-09-12, uncommitted)

`demo/index.html`: preset `<select>` (blinky/sensors/dma embedded as
base64, same `bootImage` path as dropped files — verified live:
blinky prints BOOT/BLINK, sensors prints SENS:OK/BTN without any
file) + live speedometer (`cpu.step()` return accumulated per frame,
`#mips` span updated 2x/sec; measured 1.20 MIPS in this Chromium).
Playwright-verified end to end, zero page errors.

## 37. P37 MP preset + boot gate + speed verdict (2026-09-12, uncommitted)

User hit `CPU fault pc=60012100` from Boot MicroPython app with no
real MicroPython loaded (preset/empty image leaves 0x1C000 as
0x00000000; reset_cpu(0,0) jumps into the weeds). Fixed properly:
`bootMicroPythonApp` now sanity-gates SP/PC (RAM/flash ranges) and
fails loudly (`no valid MicroPython app at 0x1C000 (SP=0 PC=0)` —
verified live). MicroPython v2.1.2 pre-added as
`demo/firmware/micropython-microbit-v2.1.2.hex` (1.24MB, same-origin
fetch, no CORS) with a one-click preset that loads + auto-boots
(verified: correct vectors, clean 60s run, no fault, no page errors).

Speed, measured not guessed: 1.20 MIPS in this Chromium, IDENTICAL
for the wasm-pack build and a guaranteed-release cargo+wasm-bindgen
build (profile is not the lever; pkg restored byte-identical).
Native debug ~5M/s, native release ~17M/s (suite 3.0s->0.87s), so
browser WASM costs ~4x vs native-debug structurally (bounds+MPU
checks, RefCell traffic, atomics, JS boundary per frame). Full
20K/frame budget is consumed awake (no sleep savings available).
Faster later: wasm-opt (not installed), hotter-loop work.

## 38. P38 demo 5x throughput (2026-09-12, uncommitted)

1.20 MIPS was the vsync cap (60fps x fixed 20K/frame), not WASM speed:
batching 5x[20K step+tick] per frame with duties once holds 60fps and
runs 6.0 MIPS (release-guaranteed build measures identical — profile
was never the lever). Same 20K tick granularity; sleep/fault handling
moved into the sub-loop (old trailing sleep block removed as double).
Verified: blinky/sensors presets, MIPS meter, zero page errors.
MicroPython preset now banners in ~30s (was: never in 700s) with the
known drop artifacts, partial prompt (`\n\n >`), then stalls with NO
fault at 150s+ — the native prompt-stall frontier, now reachable
in-browser. REPL exec still open (prompt kick + NULL fault).

## 39. P39 RX path proven live + audio exonerated (2026-09-12, uncommitted)

Native RX handoff end-to-end (P21 correction): banner run-up, drip
one byte the demo way (DMA mirror + `rx_byte`), 200K later head=1
AND tail=1 — the ISR moved it to the ring and **readline consumed
it**. P21's "readline never reads" was phase-specific, not
structural. modaudio fetcher also exonerated (returns instantly on
NULL source). Browser input stall is therefore downstream of delivery:
either the drip gate deadlocks (RXDRDY stays 1 because firmware never
consumes) or main is parked pre-readline — needs in-browser ev/pc
sampling (both proven-safe reads) to separate. RX-kick test was
negative (90s, no movement): no TX side effect from input.

## 40. P40 pin-poll + shifted vtable (2026-09-12, uncommitted)

In-browser main-loop forensics (temp instrumentation, reverted):
stalled REPL shows pc cycling `getDigitalValue` (`0x28744`) + MP
virtuals (`0x266Dx`) with a FROZEN deep-MP stack (0x2A416Bx3, ...).
The polled pin is P0.25 (obj `0x20003960`), but its vtable
(`0x57EF4`) sits +12 past NRF52Pin's (`0x57EE8`) — shifted dispatch
(corrupt pointer or sibling class sharing the reader at slot 1).
Enclosing-guess refuted (`*(obj-12)` is RAM data, not a vtable).
modaudio fetcher exonerated (returns on NULL source); idle() is
WFI+SCHEDULER_IDLE, not a pin poll. RX side meanwhile fills the CODAL
ring (head=4, tail=0) with main never reading. NEXT: name the
polling driver (r4-object at the `0x266D8`-blx / MP bytecode ID of
the looping frames) + P0.25's role (touch/logo/light/speaker?).

## 41. P41 MP-type object in pin-reader + creator chain (2026-09-12, uncommitted)

Pin-poll target is MP object `0x20003960` (type `0x57AC0`), NOT an
NRF52Pin — yet `getDigitalValue` (`0x28744`) runs with r0=it, reading
MP field +16 (=25) as a pin number and toggling P0.25. Type confusion
(pin method on MP object) or a type legitimately delegating to pin
hardware. Type ctors at `0x26848`/`0x2685C` (store type, status,
byte@+76, words@+68 — matches waiter fields); sole creator
`bl @0x24230` (80B alloc + params incl. 50/5, gated by `0x51874`
check). Vtable `0x57AC0` slots shown contain no `0x28745`, so the
route into the reader is NOT slot-1 of this type — open how r0 gets
there (stale MP-object pointer reused as pin? wrong-arg call?).
modaudio/idle exonerated again. NEXT: resolve the type NAME via its
qstr field + capture r0-provenance at the `0x28744` blx (needs the
inner call site: NOT `0x266DB` (stale lr) — sample lr strictly inside
`0x28744-0x28748` before any call).

## 42. P42 BL handoff theory: UICR markers (2026-09-12, uncommitted)

r0=`0x2002` is constructed by `movw` at `0x7B5D2` when the IPR22-bit
test fails — an error/length code into the reset path, not a live
value. MBR reset jumps via `*(0xA9C)`=`0x417` into a chained-pointer
boot selector (UICR base `0x10001000` + list walk). Piece together:
BL validates → programs UICR markers (`0x10001200/204`=18) → coded
AIRCR reset (Mbps pump honors to MBR vectors) → MBR selector sees
markers → should skip BL to APP. Our UICR store keeps the markers,
so pass 2 SHOULD diverge — but native pre-rolls show BL looping at
entry. Untested discriminator (next): seed markers + run MBR (not
app!) and trace whether pass 2 skips validation at `0x7B5B4`.
Also re-examine: the IPR22-bit math (`236>>a(byte>>5)` odd = pass)
with IPR22=0 always fails — so either the pass condition is met
elsewhere or real silicon reads nonzero reserved bits (unlikely);
more likely the markers, not IPR22, gate progress.

## 43. P43 interactive buttons (2026-09-12, uncommitted)

Button presses did nothing observable: sensors proof read EVENTS_IN0
once at boot (any later press invisible), and boot wiped latched
inputs (`initBoard` installs a fresh model). Fixed both properly:
`sensors_nrf.s` now level-polls P0.14 (print on change + delay;
verified encoding via objdump) and `init()`/`init_svd()` preserve
GPIO `input_state` across the fresh map (physical pins survive
SYSRESET). Native test green unchanged; Playwright: press->BTN:1,
release->BTN:0 (last line), hold-during-boot samples BTN:1.
testall.cjs now 15/15 (all presets, interactions, MIPS, layout).

## 44. P44 REPL main stuck in MP pin-poll loop (2026-09-12, uncommitted)

Browser main-loop forensics converged: stalled REPL has pc cycling
`getDigitalValue` (`0x28744`) + MP virtuals (`0x266Dx`) over a FROZEN
deep-MP stack — main is inside MP bytecode execution polling pin
P0.25 on heap MP object `0x20003960` (type `0x57AC0`, ctors
`0x26848`/`0x2685C`, sole creator `bl @0x24230` with 80B alloc +
params 50/5). NOT readline-blocked, NOT dead, NOT audio (fetcher
exonerated, idle is WFI). RX fills CODAL ring (head=4) but tail never
moves because main never reaches `serial.read()`. Display shows
nothing at 90s (no boot heart in this build, or scroll never starts).
Excluded as the poll driver: modaudio fetcher, idle/WFI, display
scroll ticker (needs TIMER4), touch-scan assumption. P0.25 has no
reference in MP pin/speaker/mic sources (internal CODAL use).
NEXT: MP type NAME via qstr pool (names the subsystem without runs)
+ whether the poll expects LOW (drive P0.25 low and watch for
unblock — 2-min browser test).

## 45. P45 content-sensitivity + WFE park (2026-09-12, uncommitted)

App-phase with ANSWERED TWIM1 slaves (LSM303-faithful WHO_AM_I/
STATUS/data) takes a FOURTH path: slow drift (0x29C51 -> RAM-dwell
-> 0x539E7 (WFE-wait helper, r0=2/3/4 modes) + UARTE region),
uartlen=0 at 260M, no fault — vs NACK/pristine banner+NULL-fault and
demo pin-poll spin. Sensor answer CONTENT steers the boot (NACK vs
answered vs real-part give different paths); pre-roll TWIM traffic
is zero either way (BL never touches I2C — verified by take logging).
`0x539E4` = `wfe; bx lr` waiter. So native replications now span
banner+fault (pristine), spin (dirty), drift+WFE-park (answered) —
the firmware is schedule+content chaotic; exact demo replication is
the wrong goal. The lever with payoff is sensor-part fidelity
(DRDY/data-ready behavior the waiter needs) or the NULL-this fix on
the banner path. Probes reverted; suite green.

## 46. P46 NULL member never initialized + SD-up inversion (2026-09-12, uncommitted)

`[0x20002E24]` (+2340 of the uBit-area object) is 0 from boot and
NEVER WRITTEN through banner+fault (transition watch) — the member
is never constructed, not cleared. Direct SVC16 pre-app is the wrong
enabler (MBR eats it with VTOR=0; lands in a trap sled). MBR-path
SD init (P22 canary) + app boot on top INVERTS the outcome: SVC-wait
spin (`0x215B1/AD`), uartlen=0, no fault, m2340 still 0. So SD-up
avoids the NULL fault by taking a different stuck path (waits an
SD-event completion nothing delivers — sd_evt_get has zero callers;
S140 delivers via registered observer callbacks (NRF_SDH_*_OBSERVER),
not polling). The 5 pre-fault calls (r0=obj constant, r2 mask
growing, r3 `0x5250B`→`0xC0`, last from a different sp) look like
event/bus listener invocations, the last possibly IRQ-driven.
NEXT: (1) name the +2340 member via MicroBit.h member-size math
(sizes in headers — offline, no runs); (2) find which init should
construct it and what gate it missed (compare uBit.init progress
markers vs silicon order); (3) sd_evt observer delivery remains the
deep option (SD internals, out of scope).

## 47. P47 +2340 named: compassCalibrator, watch was radio.rxQueue (2026-09-12, uncommitted)

Offline header math (no runs, ARM GCC 14.2.1 `Show<sizeof>` error
trick with stub `platform_includes`/`nrf.h`/`ble.h`): MicroPython
v2.1.2 pins codal-microbit-v2 v0.2.67 (not master) + codal-core
`509086c` + codal-nrf52 `8802eb4`; config from v0.2.67
target-locked (`DEVICE_BLE=1`, `COMPONENT_COUNT=60`, `QUEUE=10`,
`TIMESTAMP=uint64_t`, etc.). v0.2.67 has NO `displayTimer` (3 timers,
master adds a 4th) — master math is off by a timer plus drift.

v0.2.67 offsets: serial 1648, radio 2272 (40B: `rxQueue` at +16,
`rxBuf` +20, datagram +24, event +32), thermometer 2312 (20B),
accelerometer 2332 (4B ref), compass 2336 (4B ref),
**compassCalibrator 2340 (16B: `compass&`, `accelerometer&`,
`display&`, `storage*`)**, audio 2360 (2376B), log 4736, sizeof
4896. So +2340 = `MicroBit::compassCalibrator` base.

Init gate: NONE missed — it is value-constructed in the `MicroBit()`
initializer list (always, before main) as
`compassCalibrator(compass, accelerometer, display, storage)`; the
4-arg ctor does `storage->get("compassCal")` (NULL when flash erased,
no wait) + `defaultEventBus`-guarded listen. Its 4 words should be
non-zero after construction (heap accel/compass + uBit display/
storage).

P46's watch was the wrong address: base `0x20002500` came from serial
`0x2BA4-1700` (master math); with v0.2.67 serial 1648 the base is
`0x2534`, true +2340 is `0x2E58`. Watched `0x2E24` = +2288 =
`radio.rxQueue` (NULL until radio RX — never pre-banner, no radio
attempts per STATUS). "Never written" is expected, not a bug; the
real calibrator was never watched. The NULL fault (`r0=0` entry,
reads flash `0x924`/`0x28`) is therefore unrelated to uBit+2340 —
link spurious, calibrator exonerated. Fault stays MP-layer (pin-poll
obj, event/bus listeners). Optional confirm (single native run):
uBit base `0x2534`, calibrator non-zero at `0x2E58`.

## 48. P48 handover prompt fix + plan hygiene (2026-09-12, uncommitted)

Ops: fixed plan.md renumber glitch (duplicate P40 → deduped, P40-P47
re-sequenced; all branch defaults restored) and prepared the 1.3
handover prompt (this file's canonical copy). No model code touched
in this step; suite 187 green, parts smoke green.

## 49. P49 BL validators decoded + TX audit (2026-09-12, uncommitted)

Offline only (objdump halfwords + v0.2.67 sources, no runs, no commits
per order).

BL (full.bin, Thumb): `0x7B530` = `svc 16; bx lr`
(sd_softdevice_enable wrapper), `0x7B534` = svc 17. The `0x7B5EC`
runner: flag `[0x20004E2D]` nonzero → `r4=8`, return; else set
`[0x20004E2C]=1`, `bl 0x7B538` (list-walk; 17 = nothing-to-do →
return `r4=0`); else `bl 0x7B568` (second list-walk), `bl 0x7763C`,
`bl 0x7B530` (SVC16) → `r4` = SD-enable result. `cbnz r4` at
`0x7B636` skips validation on SD-enable FAILURE; `r4==0` = SD-enable
SUCCESS → proceeds to `0x7B5B4` + `bl 0x7B568`, then the tbb
dispatch at `0x78498` selects reset site #2 (`0x78514` via
`bl 0x783EC`; park `0x783FE` is the post-AIRCR wait). So "why r4==0"
is answered: SD enable succeeded, and the coded reset is BY DESIGN
(handoff into the SD-enabled state). The loop is the MBR selector
re-entering BL despite UICR markers `0x10001200/204=18` (P42 theory
stands; needs an MBR-pass-2 trace, not done here).

`0x7B5B4` = IPR22 validator: `r1=[0x7B5E4]=0xE000E100`,
`ldrb [r1+#0x316]` = `0xE000E416` (IPR22) → `r2=byte>>5`,
pass iff `(236>>r2)` odd (`0xEC=0b11101100`: r2∈{2,3,5,6,7}).
IPR22=0 → r2=0 → even → fail → `r0=0x2002` → reset path. Pass path
sets bit `0x400000` at `[0x20004EC0]`/`[r3]` (or enables IRQ22 via
ISER0 on the other branch). Either way IPR22=0 always fails in
emulation (NVIC reset); on silicon SD sets app priorities (2/1
pattern) first.

`0x784C4` = DFU-progress gate, not the r4 cause: flag byte
`[0x20002DF1]`; if set, `r4=[0x2DFC]-[0x2DF4]`, `cmp #59`; `r1=0/1`
into `bl 0x78760`, then copy `[r6]→[r5]` when in range. Counter
semantics (validated-pages/image progress) left unread; no model
impact (no new registers).

TX audit (v0.2.67 `NRF52Serial::putc` + `Serial::dataTransmitted`):
banner uses 1B TXDMAs from putc's `&c` stack slot (`0x2001FEC7`);
IRQ-mode putc sets `is_tx_in_progress_=true`, STARTTX, returns
IMMEDIATELY (no ENDTX wait); the caller reuses the slot for the next
char before our deferred `take_txdma` (pumpDma, up to ~100K later)
reads it → N+1 substitution, aligned, self-correcting — exactly the
holes (#19/39/59/79, always `0x20`, PTR constant). The entry spin
(`while(is_tx_in_progress_)`) cannot help: the overwrite lands during
argument setup BEFORE the next putc's spin. Silicon EasyDMA reads the
byte within cycles of STARTTX; we read K instructions later. Proper
fix = snapshot `MAXCNT` bytes synchronously at STARTTX, but
`Peripheral::write(sys,…)` has no mem handle — needs a trait change
(core-touching, anti-small-diff). Deferred as invasive; accepted as
cosmetic TODO (likely silicon-visible only in the sense that silicon
wins the race by speed, not by protocol).

Prompt-kick source (same files): the idle-ring kick is
`enableInterrupt(TxInterrupt)` (tail-advance + direct `putc`, end of
`setTxInterrupt` fill path; `dataTransmitted` chains off ENDTX).
Prompt queued-but-unstaged with `is_tx` false ⇒ the fill path never
reached its kick (needs in-browser ev/pc sampling to separate
drip-deadlock vs parked-main — still open, no model change here).

No model code touched in this step; 187 green + smoke green hold
from the P43/demo commits.

## 50. P50 MC fiber walk: one-shot render, waiter never parks (2026-09-12, uncommitted)

Native temp probe (reverted; suite file-free, 187 green): MakeCode
`showString("A")` direct-app boot (UICR+params P16 recipe,
sleep-aware 20K pump, pristine NACK): 2 handshake resets, zero
faults over 240M. DIR0 sticky `0x01788000`, TIMER4 COUNTER ever 0,
main dwells RAM `0x2000207A` (scheduler idle) / sleep leaf
`0x37AF8` (`wfe; bx lr`, verified halfwords).

Waiter `0x30C04` fully decoded (objdump, no guessing): fiber-wait
`(7,1)` via `bl 0x2E2E8`, then busy-poll (`+20`: 7→sleep via
`0x37AF8`+recheck, 1/0→return). Starter `0x30C30`: buffered-copy
`bl 0x34248`, then `target=len/progress=0/busy=7` (len 0 returns
without touching busy; busy==7 on entry returns -1001). A twin
channel exists at `0x30C70`.

Trace (20K slices + 1K fine-comb 160–172M, busy watch on
`0x20002D6C`): periodic ticker fiber at `0x31F0A` (lr `0x30F97`,
every ~267K); render cascade 164.53–164.59M (`0x31AE0` with
r0=`0x200025D0` = uBit area, `0x34880/72`, `0x31F14`…); BUSY
0→7 at 165.0M under EVENT dispatch (pc `0x2E68C`, lr `0x2E667`) →
7→0 at 168.1M in display code (pc `0x31BDA`, lr `0x2DD71`).
region_hits=0 / exact30C18=0 over the whole 240M: the waiter fiber
NEVER parks — `showString("A")` fits 5x5, renders once (~165M) with
no blocking scroll call outstanding, so no fiber ever waits on
(7,1); starter+completion both run inside event handling.
TIMER4 refresh never STARTs (display renders on demand in this
CODAL cut, or refresh start is the still-missing init).

NEXT: matrix content check needs browser wall time (long run, "A"
should already be latched post-165M); (7,1) producer identity is now
secondary (event fires, waiter absent). Temp probe + /tmp bins
reverted; nothing committed per order.

## 51. P51 browser batch: entry-fault closed, fds-lottery elapsed, prompt-kick = parked-main, sd_evt re-shelved (2026-09-13, uncommitted)

All browser runs headless Chromium, demo pkg as committed + working
presets/Load-Run (probes via Playwright `evaluate`, one-line
`__dbg` + 20x5K pump as TEMP page edits — fully reverted,
`grep __dbg/__firstFault` = 0, `node --check` clean; no commits).

1. Entry-fault classification (P27 ask: clean multi-run + first-fault
   hook): 20x5K pump ×3 with first-fault JS stash → 3/3 BANNER path
   (~30s, uartLen 78, firstFault null, no page errors). The
   deterministic `0x29C7A`/`0xDEAD` app-entry fault NO LONGER
   REPRODUCES — almost certainly killed by the P45 pristine re-init in
   `bootMicroPythonApp` (removes the dirty MBR/BL model state P27
   itself suspected for stale-IRQ-at-entry). Banner shows live TX-drop
   artifacts (`o  2023`, `micro: it`, `nRF5 833`) — P49's putc-slot
   mechanism confirmed in the wild. Verdict: CLOSED, do not re-open
   without a config that faults.
2. FDS lottery (2048B backtrace at the `0x215AC` spin): unreachable —
   current 5x20K pump takes the banner path deterministically (see
   above + P38); the waiter spin was a 1x20K-era path. Backtrace probe
   (regs + 2048B stack + flash-validated walk) stood ready but never
   triggered. Verdict: ELAPSED by pump evolution; re-arm only if a
   future pump/config re-enters the spin.
3. Prompt-kick sampling (RXDRDY-ev + pc during input — the separation
   experiment): banner → stall (pc `0x28747`, rxdrdy 0, ring 129/0/0)
   → Send `print(1+2)` → 60s of pc cycling `0x2874x`
   (getDigitalValue) + `0x266Dx` (MP virtuals), IDENTICAL loop
   pre/post input, rxdrdy 0 throughout, uart frozen at 106B, no
   fault. Main is inside MP bytecode execution polling P0.25 — NOT
   readline-blocked, NOT dead. Verdict: PARKED-MAIN (pin-poll),
   drip-deadlock moot as primary (consumer never reaches
   `serial.read()`; ring deltas downstream of the stall). Matches P44
   natively, now proven in-browser post-input. Remaining MP-layer
   question unchanged: which bytecode loop (frame/qstr ID) drives the
   P0.25 poll — MP internals, needs frame inspection.
4. sd_evt: re-scanned full MPY + MC images for the `svc 82`
   halfword (`0xDF52`): ZERO sites in both (control counts sane —
   svc16:2 incl. BL `0x7B530`, svc18:2, svc40:4, svc41:5). Hook would
   be dead code. Verdict: SHELVED, stays shelved; doc untouched.

## 52. P52 batch: pin-poll named+fixed, TX snapshot, MBR static refutation, matrix strobe-OR (2026-09-13, single commit)

All four NEXT items closed out in one commit (user order). Suite
188 green (+1 TX snapshot test), smoke green incl. new DRDY check.
Temp native probes reverted (`git checkout -- .../cpu/tests.rs`).

1. NEXT-1 MP pin-poll driver ID — DONE offline (no browser needed).
   `P0.25 = MICROBIT_PIN_SENSOR_DATA_READY` (`irq1`, active-lo,
   pull-up): `codal-microbit-v2@v0.2.67` `model/MicroBitIO.h:268` +
   `source/MicroBitAccelerometer.cpp:55-68` (`new LSM303Accelerometer/
   Magnetometer(i2c, irq1, …)`), `model/MicroBit.cpp:128-130`.
   `0x20003960` is the CODAL `LSM303Accelerometer` heap driver (80B,
   `autoDetect@0x24200-0x24230`), NOT an MP object — corrects P41:
   `0x57AC0`/`0x57B28` are C++ vtables (slots `0x266B9`/`0x26791`,
   `0x26891`/`0x26941`/`0x269B9`), P40 "shifted vtable" was NRF52Pin
   vtable `0x57EE8` (`0x28745` = `getDigitalValue+1`) read as a type.
   Loop `0x266B8` = `LSM303Accelerometer::requestUpdate()`
   (`codal-core/source/drivers/LSM303Accelerometer.cpp:148`:
   STATUS `0x27` @`0x266F4 movs r2,#39`, OUT `0xA8=0x28|0x80`
   @`0x26720`, `/32`, ENU `sampleENU.x=-y*range`); twin `0x26890` =
   `LSM303Magnetometer::requestUpdate()` (STATUS `0x67` @`0x268CA
   movs #103`, OUT `0xE8=0x68|0x80` @`0x268F8`, `0xFF6A`/150
   normalize). Both spin `do{}while(awaitSample)` on
   `int1.isActive()` → `NRF52Pin::getDigitalValue@0x28744`
   (`PORT->IN`); our GPIO idle-HIGH reads inactive forever, zero I2C
   (matches P45 zero-traffic). NACK WHO_AM_I → stub driver (no poll,
   banner+fault); answered → LSM303 + DRDY spin. FIX (demo-only, no
   model change): `demo/parts/lsm303.js:poll()` holds P0.25 low
   (`gpio_set_input(0,25,false)` — synthetic sample always ready).
2. NEXT-2 TX sync-snapshot — DONE without the invasive trait change.
   Thread-local `FlatMemory` published by `WasmCpu::step` via a
   drop-guard (`uarte_nrf.rs:tx_snapshot_guard`, no `src/cpu` edits,
   no signature changes); STARTTX copies `MAXCNT` bytes synchronously;
   `complete_txdma` emits the snapshot over the driver's late bytes.
   Native/unit harnesses never set it (null = legacy path, all old
   tests unchanged). Proof: new `tx_snapshot_freezes_starttx_bytes`
   (STARTTX 'A', corrupt slot to 'B', complete with 'B' → console
   'A', no 'B'; 2nd run without guard passes driver bytes through).
   Demo pump untouched (same take/complete calls).
3. NEXT-3 BL MBR selector — CLOSED by static decode (P42 refuted).
   MBR reset `0xA81` → `*(0xA9C)=0x417` selector `0x416+`: reads
   `*(0xFF8)`/`*(0xFFC)` chain, UICR `[base,#20]=0x10001014`
   (BOOTLOADERADDR) / `[#24]=0x10001018` (settings), `0xAA` marker
   byte, 16B memcmp, `*(r5)==4`→boot-app / `==0`→`bl 0x38E` / else
   HALT `0x4BC`; missing SD (`*(0x1000)==FFFFFFFF`) halts `0x4EE`.
   Full `0x0-0xB00` sweep: NO read of `0x10001200/204` anywhere —
   the markers are BL-internal DFU state, never MBR inputs. Seeded
   native MBR run (30M, markers pre-set): 1 reset, BL head parks at
   `0x77332`, markers persist `[18,18]`, validation never passable
   because BL-side `0x7B5B4` needs IPR22 nonzero (SD-set app
   priorities — silicon state, out of scope like sd_evt). Direct-app
   boot stays the recipe; do not re-trace without SD priorities.
4. NEXT-4 MC matrix — blank PROVEN decisive (not strobe aliasing).
   Native `showString("A")` direct-app 172M (~27s, no fault):
   DIR0 sticky `0x01788000`, pc `0x2000207A`/`0x37AFA`, instant grids
   blank at 100/164/172M; strobe-OR phase (200 samples over +1M)
   all-zero counts — OUT never produces an on-phase. So the one-shot
   render bookkeeping (BUSY flip P50) never starts TIMER4 refresh:
   `NRF52LEDMatrix::enable` (ctor) never runs, init stalls in an
   earlier member. Still open, but the fix is NOT timing/pump (any
   init-order change now would be guessing).
