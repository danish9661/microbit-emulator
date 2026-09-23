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
512KB flash bin + UICR NRFFW words (`0x10001014: 00077000 0007e000` —
corrected P113; was misprinted `00070700`).
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

## 53. P53 browser-banner gap + TX tail-shift (2026-09-13, uncommitted)

TEMP probes used throughout (`window.__dbg` one-liner + native
`tmp_l1`/`tmp_p53i`/`p53d` harnesses, `/tmp/opencode/probe/*.py`
re-created — /tmp is ephemeral); all reverted
(`grep __dbg/tmp_l1/tmp_p53i/p53d` = 0). Suite 190 green, smoke
green. No commits per order.

1. Browser banner gap — GATE IS TWIM-SIDE, UARTE-SIDE CLEAR.
   Dense browser run (browser pkg, this tree): pc pinned
   `0x200021b8/bb` (RAM delay-fn countdown, lr `0x26039`,
   `0x20ab1` on stack), uartLen=0 through 100s, while the native
   L1 probe with the same recipe banners at 160–180M
   (`0x266D4`, 106B). Browser peripherals at stall: TWIM1 ADDR
   114 (`0x72` = `0x39<<1`, firmware asking for the USB-FLASH
   chip), ERROR=1/ANACK=1 (before ADDR fix; now 0/0 with endtx/rx
   1/1), UARTE `u_txmax=0/endtx=0` (never staged — symptom, not
   cause), NVMC READY=1/CONFIG=0 with no staged erase, RESETREAS
   `0x4` (SREQ: AIRCR resets ARE honored now). Root cause chain:
   nrfx writes TWIM ADDRESS shifted (`addr<<1`) but the model
   masked `&0x7F` (114→50) AND `slave_present` required exact
   match — no tap at 50, so every sensor/USB transaction NACKed
   and firmware parked in the USB-flash wait loop. FIX (this
   tree): ADDRESS keeps the raw value (`&0xFF`, silicon readback),
   `slave_present` prefers exact then `>>1` (so `0x72` finds the
   `0x39` tap); demo `lsm303.js` gained `normAddr` + a KL27-UIPM
   `0x70` stub (empty = no event; `0x39/0x72` deliberately NOT
   stubbed — NACK there is the correct "no flash op" answer) and
   DRDY now PULSES 60ms-low/140ms-high (permanent low trips the
   KL27 `idleCallback` >30-tick USB threshold on shared irq1).
   Post-fix browser: TWIM ACKs (`t=114/0/2`, endtx/rx=1), but
   uartLen still 0 at 100s — banner needs a longer run to confirm
   (native threshold 160–180M ≈ 30s at 6 MIPS; browser reparks
   identically, so wall-time, not model, is now suspect #1).
2. TX tail-shift — SNAPSHOT ACTIVE, DRIVER PATH CLEAN.
   L1 native banner (snapshot path live via `WasmCpu::step`)
   still shows holes at 19/39/59/79 (always `0x20`) PLUS a tail
   shift (`\r\n>` → `\0 >`, len 106 vs 105). TEMP P53i audit
   (take-time vs complete-time vs uart bytes, 106 takes): 0
   mismatches of 106 — the deferred driver read is byte-clean,
   so the corruption lands BEFORE STARTTX (firmware-side slot
   reuse, P49 mechanism) or in prompt staging, not in our take.
   Do NOT change the `Peripheral::write` trait; snapshot stays as
   the (partial) mitigation. Live verification blocked behind (1).
3. REPL exec + MakeCode — BOTH CONFIRMED BLOCKED BEHIND (1),
   deferred. MC L4 probe: waiter `0x30C18` 0 hits over 260M,
   pc `0x20002078/7A` → `0x37AFA` WFE-idle, DIR0 sticky
   `0x01788000`, TIMER4 CC0/EV0 untouched — stall is pre-scroll
   (init never reaches scroll setup), not a missed wakeup.
4. Housekeeping: STATUS 188→190 (+TWIM shifted-ADDR match, +SCB
   AIRCR SYSRESETREQ test); pkg rebuilt with this tree's fixes.
   Shelved stays shelved: sd_evt (0×svc82), full MBR→BL→SD→app
   (needs SD priorities), npm publish (401).

## 54. P54 pump duties in-loop + P55 HFCLK rule-out + P56 sampling fix + P57 MC zero-touch (2026-09-13, uncommitted)

Pump fix (demo/index.html, working tree): `pumpDma()` moved INSIDE the
5×[20K step+tick] sub-loop (was once/frame). Duties-once-per-frame
starved polled firmware: a 1B STARTTX staged at slice 0 waited ~100K
instr for completion, so `putc`'s polled ENDTX/entry spins burned whole
frames and the 60/140ms DRDY pulse could never land inside a 20K wait
window. P54 live-verify caught a tap-wipe artifact instead (ANACK back
= `bootMicroPythonApp` re-init clearing taps; native behavior
unchanged), then reverted TEMP; 190 green + smoke green hold.

HFCLK wait decoded + RULED OUT (clock_nrf.rs, working tree): the
pre-banner park `0x200021b8/bb` disassembles as a RAM countdown
(`01 38 fd d1 70 47`: `subs r0,#1; bne; bx lr`), called from the
`0x20980` 20-iteration helper (`movs r7,#20` loop, rets `0x20ab1`/
`0x20b37` bl-validated, lr `0x26039` = u64-compare helper). Model
boots WITH both clocks on + events set now (post-ramp silicon;
re-arm via TASKS still works; `nrf_boot_flash_at_zero` updated for
boot-set events). P55b native to 600M: park PERSISTS with
hf_ev=1/hf_run=1 — HFCLK is not the gate. Browser P55c confirms
(hf_ev=1/hf_run=1, same park). r0 at park (`0x92b`–`0x719e`) is the
live countdown, r4=`0x3e8` (1000). NEXT: name the `0x20980` helper's
caller (which init phase waits 20× countdown — sensor-settle?
power-stable?) via an r7-entry watch.

P56 sampling fix (methodology, no model change): 1M-slice sampling
catches the pc BETWEEN ram-spin iterations (loop body <20K), so
"EXIT" hits at 1M with identical lr/stack are sampling race, not
exits — require 5 consecutive non-spin slices before declaring.
P57 MakeCode zero-touch (native 300M, reverted): first-touch ledger
for TIMER4-CC0/GPIOTE-CONFIG[1..5]/PPI-CHENSET/TWIM1-ADDR/NVMC-CONFIG/
UARTE-TXMAX/waiter-`0x30C04` ALL ZERO; pc `0x20002078/7A`→`0x37AFA`
WFE-idle, DIR0 sticky. No member init touches hardware in 300M —
stall is pre-scroll sequencing (main never issues scroll), not a
missed wakeup. Probes reverted; suite file-free again.

## 55. P55 HFCLK rule-out + P56 sampling fix + P57 MC zero-touch + P58 r7-watch constraint (2026-09-13, uncommitted)

HFCLK wait decoded + RULED OUT (clock_nrf.rs, working tree): the
pre-banner park `0x200021b8/bb` disassembles as a RAM countdown
(`01 38 fd d1 70 47`: `subs r0,#1; bne; bx lr`), called from the
`0x20980` 20-iteration helper (`movs r7,#20` loop, rets `0x20ab1`/
`0x20b37` bl-validated, lr `0x26039` = u64-compare helper). Model
boots WITH both clocks on + events set now (post-ramp silicon;
re-arm via TASKS still works; `nrf_boot_flash_at_zero` updated for
boot-set events). P55b native to 600M: park PERSISTS with
hf_ev=1/hf_run=1 — HFCLK is not the gate. Browser P55c confirms
(hf_ev=1/hf_run=1, same park). r0 at park (`0x92b`–`0x719e`) is the
live countdown, r4=`0x3e8` (1000). Probes reverted; 190 green hold.

P56 sampling fix (methodology, no model change): 1M-slice sampling
catches the pc BETWEEN ram-spin iterations (loop body <20K), so
"EXIT" hits at 1M with identical lr/stack are sampling race, not
exits — require 5 consecutive non-spin slices before declaring.
P57 MakeCode zero-touch (native 300M, reverted): first-touch ledger
for TIMER4-CC0/GPIOTE-CONFIG[1..5]/PPI-CHENSET/TWIM1-ADDR/NVMC-CONFIG/
UARTE-TXMAX/waiter-`0x30C04` ALL ZERO; pc `0x20002078/7A`→`0x37AFA`
WFE-idle, DIR0 sticky. No member init touches hardware in 300M —
stall is pre-scroll sequencing (main never issues scroll), not a
missed wakeup. Probes reverted; suite file-free again.

P58 r7-watch constraint (native 120M, reverted): watch on
pc==`0x20980` entry finds ZERO entries — the helper was entered ONCE,
early (<20M, before first sample), never re-entered; the 20× loop is
a single long init wait, not a recurring poll. Entry pc never equals
`0x20980` in sampling (preceding `0x20978` padding `movs r5,r0` may be
the true entry, or `cpu.run` quanta step over the prologue). NEXT:
sample lr==`0x20981` (bl return) or fine-comb 0–20M from boot.

## 56. P58 BLE/BT feasibility verdict (2026-09-13, uncommitted — NO build)

Question: start working on BLE/BT. Evidence gathered, no code touched.

1. MPY config DISABLES BLE: `src/codal_app/codal.json` sets
   `MICROBIT_BLE_ENABLED: 0` (pairing mode 1, partial flashing 1 are
   inert with it off). `MicroBit::init` skips `bleManager.init` and
   the pairing-mode branch entirely when disabled — BLE contributes
   ZERO pre-banner instructions in the MPY image. There is nothing
   BLE-gated on the banner path to fix.
2. MPY radio is BARE-METAL, not SoftDevice: `drv_radio.c`
   (`microbit_radio_enable`) drives `NRF_RADIO` registers directly
   (HFCLKSTART wait → TXPOWER/FREQUENCY/MODE/BASE0/PREFIX0/PCNF0-1/
   CRCCNF/CRCINIT/CRCPOLY/DATAWHITEIV/PACKETPTR) + `NRF_RADIO->IRQ`
   via `microbit_radio_irq_handler` (re-vectored in `main.cpp` AFTER
   `uBit.init`). Our RADIO model already covers exactly this surface
   (TASKS/EVENTS/SHORTS/INTEN/PCNF/addresses/CRC/RSSI + loopback
   take/complete/inject). No model gap for the MPY radio path.
3. `svc 82` (sd_evt_get) is CONFIRMED ABSENT, not just unscanned:
   even-aligned halfword scan of `mpy_full.bin` finds ZERO `DF52`
   sites (the 2 prior hits at `0x19c07`/`0x1abb9` are ODD-addressed
   data false-positives inside non-code bytes). So no firmware in
   this image can ever observe an SD event — the P32/P51 shelve
   stands on aligned evidence. `docs/sd_evt_design.md` stays a
   corrected reference (phase-1 transport design valid IF a future
   image calls it; today it would be dead code — do not build).
4. SCOPE if BLE/BT is wanted anyway (all future work, none started):
   (a) needs a BLE-ENABLED image first (MPY codal.json flip or a
   MakeCode BLE program — today's images never init the stack);
   (b) then SoftDevice SVC surface beyond enable/is_enabled (GAP/GATT
   SVCs, observer callbacks `NRF_SDH_*_OBSERVER`, connection event
   pump) — an order of magnitude past the flash-only sd_evt transport;
   (c) then a host-side BT peer (WebBluetooth or a second loopback
   endpoint) to talk to. The RADIO model needs no changes for any of
   this (bare-metal air path already looped back); the work is all
   SoftDevice-state synthesis, currently shelved by evidence.
   NEXT when asked: build a BLE-enabled image + rescan SVCs (expect
   nonzero svc82 + GAP/GATT SVC traffic) BEFORE writing any model code.
## 57. P59 RADIO 802.15.4 helpers + air-peer bridge (2026-09-13, uncommitted)

BLE/BT work without WebBluetooth and without touching the shelved
SoftDevice path: the MPY radio is bare-metal `NRF_RADIO` (`drv_radio.c`
+ custom IRQ), so the air model — not the SD — is the lever.

RADIO model (`radio_nrf.rs`, working tree): ED (EDSTART→EDEND +
EDSAMPLE/EDCNT from host `radio_set_ed_dbm`, EDSTOP→EDSTOPPED), CCA
(CCASTART→CCAIDLE/CCABUSY vs CCACTRL thresholds, CCASTOP→CCASTOPPED),
DEVMATCH/DEVMISS (+RXMATCH/RXCRC/PDUSTAT) via programmed DAB/DAP,
MHRMATCH via CONF/MAS, FRAMESTART alongside ADDRESS, BCMATCH,
TIFS/BCC/SFD/MODECNF0/POWER stored (POWER=0 gates tasks), full
SHORTS (READY_EDSTART, EDEND_DISABLE, RXREADY_CCASTART, CCAIDLE_TXEN,
CCABUSY_DISABLE, CCAIDLE_STOP, TXREADY/RXREADY_START, PHYEND_*) and
INTEN (SVD lsb map incl. 5/6 DEVMATCH/MISS, 10 BCMATCH, 14–19
FRAMESTART/ED/CCA, 23 MHRMATCH) bit maps, new test
`ed_cca_mhr_devmatch_framestart`. RATEBOOST/SYNC/PHYEND/CTE events
stay unmodeled (BLE-test/DFE, no air behavior); DAI/DACNF stored.
Demo air (`demo/index.html` pumpDma): two-instance bridge —
`window.__airPeer(bytes)->bytes[]` supplies FOREIGN bytes (second tab
/ harness); empty return = same-instance loopback (default). New wasm
export `radio_set_ed_dbm` (API.md needs one line when docs resume).
Suite 191 green (new test kept), smoke green. No commits per order.

## 58. P58 r7-watch constraint + P60 caller named + r0 wrapping (2026-09-13, uncommitted)

P58: r7-entry watch (pc==`0x20980`, 5K quanta, 120M) finds ZERO entries
— the helper was entered ONCE, early (<20M, before first sample),
never re-entered; the 20× loop is a single long init wait, not a
recurring poll. (Entry pc may never equal `0x20980`: preceding
`0x20978` padding `movs r5,r0`, or quanta stepping over the
2-halfword prologue.)

P60 (static + live, both reverted): bl-scan finds exactly TWO
`bl→0x20980` sites, `0x20b32`+`0x20b58`, both inside ONE function
(`0x20ae8`, GC-shape: `bl 0x528e8` alloc + `bl 0x528d8`/`0x52908`
field init + `strb [r3,#4]` type-tag store). Live r0-at-park across
2M samples wraps (`0xb3a→0x990→0x8ff→…→0x92b` — never monotonic,
never stuck): the inner countdown completes and the 20× loop
re-arms — a tight re-poll whose exit condition (r7-driven,
`0x209aa`-region compares) never fires. NEXT: catch the r7 countdown
+ the `0x209aa` compare inputs (which flag/word the loop tests).
Probes reverted; 191 green hold.

## 59. P59 RADIO helpers + P60 caller named + P61 stale-regs (2026-09-13, uncommitted)

P59: see §57 note (RADIO ED/CCA/DEVMATCH/MHR/FRAMESTART + air-peer
bridge, `ed_cca_mhr_devmatch_framestart` green, suite 191).

P60: bl-scan names the `0x20980` caller statically — exactly TWO
`bl→0x20980` sites (`0x20b32`+`0x20b58`), both inside ONE function
(`0x20ae8`, GC-shape); live r0-at-park wraps (alive, re-arming
re-poll, exit condition never fires).

P61 (native, reverted): sampled regs AT the park pc are STALE —
r4=`0x3e8`/r6=stack are the DELAY call's args, not the helper's
(`[r4+20]` reads flash, `[r6]` a stack word); the helper frame is
gone because we sample the delay-fn leaf, not the loop body. So r7
+ `0x209aa` compare inputs are NOT observable at
`0x200021b8/bb`. NEXT: pc-break INSIDE `0x209aa–0x209de` (live
r4/r6 + fp target + r0 at `0x209b2`/`0x209c2`), not at the leaf.
Probes reverted; 191 green hold.

## 60. P60 caller named + P61 stale-regs + P62 silent band (2026-09-13, uncommitted)

P60: bl-scan names the `0x20980` caller statically — exactly TWO
`bl→0x20980` sites (`0x20b32`+`0x20b58`), both inside ONE function
(`0x20ae8`, GC-shape); live r0-at-park wraps (alive, re-arming
re-poll, exit condition never fires).

P61 (native, reverted): sampled regs AT the park pc are STALE —
r4=`0x3e8`/r6=stack are the DELAY call's args, not the helper's
(`[r4+20]` reads flash, `[r6]` a stack word); the helper frame is
gone because we sample the delay-fn leaf, not the loop body. So r7
+ `0x209aa` compare inputs are NOT observable at
`0x200021b8/bb`.

P62 (native 200M, reverted): pc-break INSIDE `0x209aa–0x209de` with
200-instr quanta gets ZERO hits — the band is never observed. The
park pc (`0x200021b8/bb`) is the ONLY observable pc from 20M to
200M: instruction-level sampling (20K, 5K, 200 quanta) never catches
the loop body, only the leaf. Either the loop body executes between
samples at a duty cycle below sampling resolution, or pc reads while
inside get folded (sleep/exception path?). NEXT: single-step from a
20M snapshot (1-instr quanta for 1K) to catch the body, or trace
buffer (`trace_start/stop/take_trace`) around the park.
Probes reverted; 191 green hold.

## 61. P61 stale-regs + P62 silent band + P63 fiber scan (2026-09-13, uncommitted)

P61 (native, reverted): sampled regs AT the park pc are STALE —
r4=`0x3e8`/r6=stack are the DELAY call's args, not the helper's;
the helper frame is gone (we sample the delay-fn leaf). So r7 +
`0x209aa` inputs are NOT observable at `0x200021b8/bb`.

P62 (native 200M, reverted): pc-break INSIDE `0x209aa–0x209de`
with 200-instr quanta gets ZERO hits — the band is never observed
at any quantum (20K/5K/200). Either sub-resolution duty cycle or
folded pc reads.

P63 (native, reverted): 5K single-steps from the park show the park
is NOT a pure spin — each ~4800-iteration block runs the RAM delay
leaf plus a 48-instr FIBER SCAN (`0x28290`-region: `ldr r0,[r4,#16]`
→ table `[r0,r7]` → null-check → `+292/+336/+352` chain,
`cmp r6,#99` bounded scan, `blx` dispatch at the end). Trace ring
(2000): `0x200021b8/bb` ×952 each + scan pcs ×3 each
(`0x282a6/b4/b8/bc/be/c2/c4`, `0x26020/26/2c/2e/32/34/36/38/3a`,
`0x200021bc`). So EVERY outer iteration: delay-leaf countdown +
one fiber-table scan step that finds nothing (null → next index).
The wait that never fires is a FIBER/event wait scanned under
`0x28290`, not a sensor/clock/pin wait. NEXT: dump r4 (`[r4,#16]`
table base) + r7 (index) + `[r0+#292/+336/+352]` (which table entry
is null) live at `0x282a6`, and name the event id the scan waits
for (fiber-wait `(7,1)`-shaped? compare P50 `0x30C04` waiter).
Probes reverted; 191 green hold.

## 62. P63 fiber scan + P64 TWIM-wait named (2026-09-13, uncommitted)

P63 (native, reverted): 5K single-steps from the park show the park
is NOT a pure spin — each ~4800-iteration block runs the RAM delay
leaf PLUS a 48-instr FIBER SCAN (`0x28290`-region). Trace ring
(2000): `0x200021b8/bb` ×952 each + scan pcs ×3 each. So EVERY outer
iteration = delay countdown + one fiber-table scan step finding
nothing. The wait that never fires is a FIBER/event wait scanned
under `0x28290`, not sensor/clock/pin.

P64 (native, reverted): single-step break at `0x282a6` reads the
scan inputs live: r4=`0x20002C0C` (a PERIPHERAL struct, not a fiber
table — `[r4+#16]=0x40004000` = TWIM1 base!), r7=260 (index — past
any table end; scan walks off the rails), slot=0 with
+292/+336/+352 ALL ZERO. So the "fiber scan" is the TWIM driver's
wait loop polling a TWIM1 transfer/control block that never
completes: the driver was programmed (PACKETPTR-style struct at
`0x40004000`+16?) but our TWIM1 model never stages the completion
it waits for (P53d histogram: 1217 USB-flash reads were the SYMPTOM
— this struct wait is the DISEASE). NEXT: dump the full
`0x20002C0C` struct + the TWIM1 regs it mirrors (ADDRESS? PTR?
MAXCNT? EVENTS?) at `0x282a6`, and compare against a native sensor
read that DOES complete (STATUS/OUT path) — the missing completion
event names the exact model gap.
Probes reverted; 191 green hold.

## 63. P64 TWIM-wait struct + P65 missing-RXSTARTRX (2026-09-13, uncommitted)

P64 (native, reverted): single-step break at `0x282a6` shows r4 =
`0x20002C0C` is NOT a fiber table — `[r4+#16]=0x40004000` (TWIM1
base), r7=260 (index walked off the rails), slot + all of
+292/+336/+352 ZERO. The "fiber scan" is a TWIM-driver wait loop
polling a TWIM1 transfer block that never completes.

P65 (native, reverted): full struct dump at `0x20002C0C` names it —
a CODAL I2C transaction/link object: `[+0]=0x57838` (vtable),
`[+16]=0x40004000` (TWIM1), `[+36]=0x40003000` (TWIM0),
`[+40]=0x40050028`, self-link `[+64]=0x20002C0C`, heap neighbors,
`[+160]=0x74000/1/0x1000` (NVMC page/len/base — the SAME
`0x74000` flash-op family as the P26 fds waiter). Live TWIM1 regs
at the same instant: ADDRESS=`0x72` (USB-flash, answered fail-fast),
ERROR=0, ENDTX=1, ENDRX=**0**, RXSTARTED=1, RXD.PTR=`0x2001660C`,
MAXCNT=3, AMOUNT=**0**, SHORTS=`0x1000` (=LASTTX_STOP). So: a 1-byte
TX completed, LASTTX_STOP fired STOPPED, but the chained 3-byte RX
(STARTRX via SHORTS LASTTX_STARTRX) NEVER COMPLETED — ENDRX=0,
AMOUNT=0 — and the driver spins waiting for it. The RX was staged
(P53d: 1217 reads) and our pump completes it with `complete_rxdma`
— but ENDRX evidently never latches or never wakes the waiter
(the waiter polls a flag our completion doesn't set, or polls
RXSTARTED/AMOUNT instead of ENDRX). NEXT: trace what the waiter
actually polls after STARTRX (EVENTS_ENDRX? AMOUNT? RXSTARTED?) at
`0x282a6`+ — read the compared word live — and check SHORTS bit 7
(LASTTX_STARTRX) handling + `rx_pending`/`rx_taken` lifecycle in
`twim_nrf.rs` against this exact sequence (TX+LASTTX_STOP, then
RX with no explicit STARTRX write from firmware).
Probes reverted; 191 green hold.

## 64. P66 lr/pc comb negative + what it proves (2026-09-13, uncommitted)

P66 (native 20M, reverted): fine comb (1K quanta) from boot watching
pc∈`[0x20980,0x20986)` AND lr∈{`0x20B33`,`0x20B59`} finds ZERO hits
of either kind — yet the park (same `0x20ab1/0x20b37` stack, same
`0x200021b8/bb` leaf) is fully established by 20M. Combined with P58
(zero pc==`0x20980` entries over 120M) and P63 (body never sampled):
the `0x20980` helper is NOT on the hot path at all. The TWO static
`bl` sites + the `0x20ab1/0x20b37` stack slots are therefore STALE
slots (called once during early init, returned, slots never
overwritten — P17's stale-slot rule), NOT live frames. The live park
loop is something else that merely LEAVES those words on the stack.
So: stop chasing `0x20980`/`0x20ae8` — the countdown wait's DRIVER is
unidentified. The r0-wrapping proof (P60) still holds (SOME countdown
runs), but its owner is unknown. NEXT: single-step FROM the park
(P63 showed the fiber scan `0x28290` runs every outer iteration —
catch r4/`[r4+#16]`/`[r4+#292]` LIVE at `0x282a6` with 1-instr
quanta, not 20K slices that only ever sample the leaf).
Probes reverted; 191 green hold.

## 65. P67 current-tree banner re-run + P68 MBR pass-2 + P69 MC re-run + P70 stubs browser + P71 meter (2026-09-13, uncommitted)

P67 (native 600M, reverted): current tree (clocks boot ON, ADDR raw,
AIRCR honored) still parks at `0x200021b8/bb` through 600M, uartLen 0,
TWIM clean (`t_addr=114/err=0/endtx/rx=1`), UARTE never staged
(`u_max=0/u_end=0`). Banner gate is NOT clocks/ADDR/AIRCR. L2 TX
consequence: with zero banner takes the snapshot path never fires —
TX item CLOSED as model-clean (unit green + P53i 0/106; old holes are
firmware-side pre-STARTTX by elimination; no trait change, ever).

P68 (native 30M, reverted): MBR entry with seeded UICR 18/18 + IPR22
`0x40404040` → 1 reset, then parks at APP vector `0x29C7A` (the P51
entry-fault address, no fault) with ZERO hits on `0x7B5B4`/`0x772F9`/
`0x783FE`. Pass 2 never reaches BL validation — MBR jumps straight
to the app vector table (MBR→app-direct, not MBR→BL→app). BL loop
theory narrowed: validation is bypassed, not failed.

P69 (native 300M, reverted): MakeCode re-run IDENTICAL on current
tree — 0 `0x30C04` hits, `0x20002078/7A`→`0x3569C` WFE-idle at 300M,
DIR0 sticky, TIMER4 untouched. Pre-scroll stall confirmed, not a
regression from clock/ADDR/AIRCR changes.

P70 (browser, reverted): extended `stubs_nrf` preset prints
`STUBS:OK` in 10s (`s2stop=1/s3stop=1`, pc `0xa3`, no fault, no page
errors) — SPIM2/3 firmware-proven live, L5 closed.

P71 (browser, KEPT — meter change only): MIPS meter now shows slice +
sustained average; blinky AND mpy park both read `6.02 (avg 6.00)` —
the user's 16-class bursts are peak slice rates; sustained == slice
because the park never sleeps. The meter no longer misleads.
Probes reverted (P70/P71 `__dbg` gone); 191 green + smoke green hold.

## 65. P72 live catch: TWIM1 MMIO polling, +124/+150/+160 all zero (2026-09-13, uncommitted)

P72 (native, reverted): single-step break at `0x282a6` (1-instr
quanta) reads the scan inputs LIVE: r4=`0x20002C0C`, r7=260,
`[r4+#16]=0x40004000` (TWIM1 base). `[r0+#124]=0` (falls to
`0x282ec`, skipping the `+292` branch), `[r0+#336]=0` (falls to
`0x28370`, skipping the `+352` branch) — so the ONLY live check is
`[r0+#512]` (`0x200`) at `0x282ce`: `lsls r2,r3,#23; bpl →0x2838a`.
Live value is 0 → N=0 → BPL TAKEN → `0x2838a` (skip). The loop NEVER
takes the `0x282de` store path (`str r8,[r0,#28]`). TWIM1 regs at the
same instant are all-healthy (ENDTX=1, ENDRX=1, AMOUNT=3, no ERROR).
So the waited word `[r0+#0x200]` reads 0 from OUR model and the
firmware skips the progress store every iteration. `[r4+#16]` is the
TWIM1 BASE (not a transfer struct): `+0x200` = TWIM1 offset `0x200`
= SHORTS — but our SHORTS reads `0x1000`, nonzero! So `[r0+#0x200]`
is NOT our SHORTS (r0≠r4 here: `ldr r0,[r4,#16]` re-reads it each
iteration — r0=`0x20002C0C` too in the dump, yet `[r0+#512]`=0 while
SHORTS=`0x1000`). Therefore `[r0+#0x200]` is a FIRMWARE-SIDE shadow/
state word at offset `0x200` of the `0x20002C0C` object, NOT the MMIO
register — the driver caches SHORTS (or a state derived from it) in
its own object, and OUR model never sets that shadow (we only model
the MMIO side). NEXT: find who writes `[0x20002C0C+#0x200]` (watch
writes to that word from boot — the setter names the event that
should set it, likely the TX-complete/RX-complete callback that our
take/complete path doesn't invoke).
Probes reverted; 191 green hold.

## 66. P73 shadow word is a POINTER, not a flag (2026-09-13, uncommitted)

P73 (native, reverted): write-watch on `[0x20002C0C+#0x200]` from boot
finds exactly ONE transition at t=0M (first sample, pc already
`0x200021b8`): value `0x573B4` — a FLASH POINTER, never changes again
through 120M. So `[r0+#0x200]` is not a flag the model should set; it
is a vtable/function pointer baked at init. Re-reading the P72 code
with this: `ldr r3,[r0,#512]` loads a POINTER (`0x573B4`), `lsls
r2,r3,#23` tests ITS bit 8 (`0x573B4` = `...0111 0011 1011 0100`,
bit8 = 0 → N=0 → BPL taken → skip). The waited condition is bit 8 of
the WORD AT `0x573B4` — i.e. a flag living at a HARDCODED FLASH/STATE
address, not in the TWIM struct at all. NEXT: dump flash word
`0x573B4` + what writes it (it may be an I2C-driver state byte the
take/complete path should update — e.g. transfer-done semaphore —
or a CODAL component status bit).
Probes reverted; 191 green hold.

## 65. P73 shadow-is-pointer + P74 type-word polled (2026-09-13, uncommitted)

P73 (native, reverted): write-watch on `[0x20002C0C+#0x200]` finds ONE
transition at t=0M (first sample): value `0x573B4` — a FLASH POINTER,
never changes through 120M. So the "shadow" is not a flag the model
should set. Re-reading P72: `ldr r3,[r0,#512]` loads the POINTER,
`lsls r2,r3,#23` tests ITS bit 8 — the waited flag lives at a
hardcoded address.

P74 (native, reverted): polling the pointer AND `[ptr]` each slice:
ptr goes 0 → `0x573B4` during early boot (before park), `[ptr]` =
`0x1E615` constant, bit 8 = 0, forever. `0x573B4` disassembles as a
Hello-World-class TYPE WORD (`15 e6 01 00 ...` = small-int/string-tag
soup, `u32 = 0x1E615`): it is a MicroPython TYPE OBJECT (or qstr/int
singleton), and the `0x282ce` check tests ITS bit 8 — i.e. a TYPE
FLAG (MP_OBJ_TYPE flag bit 8 = "callable"?/instance-layout bit?).
The TWIM-wait loop at `0x282a6` therefore polls `r3=[r0,r7]`
(object slot) for NULL, then checks the OBJECT'S TYPE FLAG — a
GC/object-model wait (object not yet initialized / layout not ready),
NOT a peripheral completion at all. The TWIM1-healthy regs (P65:
ENDTX/RX=1, no ERROR) are consistent: I2C is FINE; the waiter wants
an OBJECT whose type flag bit 8 is set, and nothing ever sets it
because the constructing fiber never runs (scheduler never delivers
the constructor — compare the NULL-`this` fault family P24–P25).
NEXT: identify `0x1E615` (which MP type: dump its name/qstr field —
type objects carry `.name` a few words in) + who should construct
the `r3==NULL` slot (the `[r0,r7]` table = WHAT table? r0=`0x20002C0C`
is whose object?).
Probes reverted; 191 green hold.

## 66. P74 type-word + P75 table-shape (2026-09-13, uncommitted)

P74 (native, reverted): `[0x20002C0C+#0x200]` transitions exactly ONCE
(t=0M, before park): `0x573B4`, a flash pointer, constant forever.
`[0x573B4]` = `0x1E615`, bit 8 = 0, forever. So the `0x282ce` check
(`lsls#23/bpl`) polls bit 8 of a CONSTANT — an MP type-flag word that
silicon also reads 0. The wait NEVER fires by construction?? — No:
re-read: the polled word is `[r3]` where `r3=[r0+#512]` RE-READ each
iteration (`ldr.w r3,[r0,#512]` at `0x282ce` — r0=`[r4,#16]` live).
The POINTER is constant but only because NOTHING writes that struct
slot in our run; on silicon some constructor/driver fills it. The
`blx` at the chain end (`0x282e6: bl 0x5048c` after `movs r0,#10`)
is the progress call gated on the flag.

P75 (static): `0x573B4` sits in a 35-entry table of `0x1E615`-based
records (`0x571F0`–`0x579F8`: `{base=0x1E615, flags/name, ...}` —
an MP TYPE TABLE: 35 MicroPython types sharing base `0x1E615`).
`0x573B4` = one entry (`{0x1E615, 0x4D821, 0x4E7E1, ...}`); only TWO
refs point AT `0x573B4` itself (`0x1E888`, `0x20890` — halfword soup,
likely data-table entries = type slots referencing it). So the waiter
wants the OBJECT in slot r7=260 of the `[r4+#16]` table to be a
`0x573B4`-typed object whose flag bit 8 gets set — i.e. it waits for
a SPECIFIC SUBSYSTEM OBJECT (whose type is entry `0x573B4`) to reach
a state, and that object is never constructed (slot NULL at +292 and
friends). NEXT: name entry `0x573B4`'s type (neighbor entries'
names/qstrs — the table at `0x57390`/`0x573C0` may carry name words;
compare against MP_QSTR list from the MPY source) + who constructs
slot r7=260 (which subsystem init owns that table index?).
Probes reverted; 191 green hold.

## 67. P75 method-table records + P76 word2 slots (2026-09-13, uncommitted)

P75: the 35 `0x1E615`-based records are NOT types (all share word1
`0x4D821`; only 2 refs point AT any single entry, and those decode
as halfword soup, not pointers). They are same-shape records with a
shared header — method-table/dispatch records, names elsewhere.

P76 (static, no run): word1 `0x4D821` is shared by all 35; word2 has
only 9 DISTINCT values across all 35, each pointing at a CODE
prologue (`b5xx push...`, `f758 ldr...`, `f114...`, `68xx ldr...`):
word2 = a FUNCTION SLOT (9 unique handlers shared across 35
records). So each record = {base `0x1E615` (dispatch root), shared
header `0x4D821`, handler fn, ...} — i.e. a VTABLE/method record
whose identity = its HANDLER. `0x573B4`'s handler is `0x4E7E1`
(`58 f7 ldr r7,[r6,r3]` — indexed-dispatch shaped). The `0x282ce`
wait (bit 8 of `[0x573B4]`) therefore polls a bit of a METHOD
RECORD, not an object flag — and the record is ROM-constant, so the
check as-shown can never flip. Either the polled word is a
DIFFERENT `[r0+#512]` on silicon (r0 differs there — our r0 is the
TWIM1-struct path because our sensor answers steer init down the
sensor-wait branch), or the flag is set by a constructor that never
runs here. Either way: no model register can fix a ROM-constant
poll — the divergence is UPSTREAM (which init branch we take).
NEXT: find what decides the branch INTO the `0x28290` wait (the
`0x282b2`/`0x282be` null-checks on `[r0+#292]/[r0+#336]` — which
subsystem's absence routes us here vs banner?).
Probes: none (pure static); 191 green hold.

## 68. P77 mock-matrix negative: branch words are MMIO-aliased (2026-09-13, uncommitted)

P77 (native 4×60M, reverted): mocking the polled branch words does
NOTHING — all four variants (A292/B336/C512/ABC) park identically at
`0x200021b8/bb`, uartLen 0. Decisive detail: struct base reads
`0x40004000` — the "struct" IS the TWIM1 MMIO region itself (`[r4+#16]`
= TWIM1 base, so `+292`/`+336`/`+512` are TWIM1 offsets `0x124`/
`0x150`/`0x200` = EVENTS_ERROR / EVENTS_TXSTARTED / SHORTS). The
firmware polls REAL TWIM1 REGISTERS, not a shadow struct — P72's
"shadow" theory was wrong (r0==TWIM1 base because r4's `[+#16]` field
HOLDS the TWIM1 base address, standard CODAL driver layout). And the
C512 variant reveals the trap: `[struct+512]` read `0x1000` (=SHORTS
value!), and `mem.read32(0x1000)`/`mem.write32(0x1000)` touch
FLASH (harmless here — `0x20000FB0` unchanged, bit already set).
So instead of answering the question, C512 PROVES the addressing:
`[r0+#512]` with r0=TWIM1 = SHORTS, and our SHORTS=`0x1000` has bit
12 set... but the check is `lsls#23` (tests bit 8 of the LOADED
word): SHORTS `0x1000` bit8 = 0 → skip, every time. On silicon,
whatever sets SHORTS bit 8 (a SHORTS bit the driver programs for
its transfer chain — bit 8 = LASTTX_SUSPEND per our SVD map!) would
let it proceed. OUR model may be DROPPING the firmware's SHORTS bit
8 write (mask?) or the driver never programs it because an earlier
step failed. NEXT: log TWIM1 SHORTS writes from boot (which value
does firmware program? does bit 8 ever get set?) + check our
SHORTS mask (`0x200` write mask `0x1F80` — bit 8 = `0x100` IS in
mask... so did firmware ever WRITE it?).
Probes reverted; 191 green hold.

## 69. P78 SHORTS lifecycle: firmware programs bit8 then clears it (2026-09-13, uncommitted)

P78 (native 120M, reverted): TWIM1 SHORTS write trace from boot:
`0xFFFFFFFF→0` at 0.02M (model reset value, ignore), then `0→0x100`
(bit 8 = LASTTX_SUSPEND) at ~1.04M, `0x100→0x1000` (bit 12 =
LASTRX_STOP) at ~1.06M, `0x1000→0x200` (bit 9 = LASTTX_STOP) at
~1.08M, `0x200→0x1000` at ~2.66M, stable `0x1000` to 120M. So
firmware DOES program bit 8 early (during init transfers) but the
STEADY-STATE park value is `0x1000` (bit 8 CLEAR) — the P72 check
(`lsls#23` on SHORTS, needs bit 8) reads the CURRENT register,
which legitimately has bit 8 = 0 at park. The check failing is
therefore CORRECT behavior for the programmed SHORTS — the waiter's
real wait is NOT "SHORTS bit 8" but whatever the `0x2838a` skip
path means (fall through to delay + re-poll). Combined with P77
(mocking all three branch words changes nothing): the park loop is
a POLLED IDLE that only exits via the `0x282ec`/`0x28370`/`0x282de`
progress paths, and NONE of the polled words ever go nonzero in ANY
configuration tried (sensors answered, KL27 stubbed, clocks on).
The missing event is therefore something NONE of our stubs provide:
candidates are (a) a DIFFERENT I2C address we don't stub (scan TWIM1
ADDR traffic for unanswered addresses — P53d saw only 0x72 traffic,
but that was pre-ADDR-fix; re-scan), (b) a non-I2C peripheral event
(SAADC? PDM? USBD? GPIOTE PORT?) the init waits on, (c) a TIMER/RTC
tick that never fires at the expected rate. NEXT: full peripheral
EVENT histogram (which EVENTS_* regs go nonzero per 20M slice) +
TWIM1 ADDR histogram re-scan on the current tree.
Probes reverted; 191 green hold.

## 70. P79 event histogram: T2_C0 fires, T4_C0 once, ADDR all-answered (2026-09-13, uncommitted)

P79 (native 120M, reverted): hot events per 20M slice are STABLE:
TIMER2-COMPARE0 + TWIM1 ENDTX/ENDRX/STOPPED, always; TIMER4-COMPARE0
exactly once (40M slice). NOTHING else ever fires — no SAADC/PDM/
GPIOTE/NFCT/RADIO/COMP/RNG/UARTE/TIMER0-1-3/RTC events. TWIM1 ADDR
histogram: `0x72` ×1468 (USB-flash, fail-fast answered),
`0x19` ×5 + `0x1E` ×2 (ONE sensor probe each at boot, then never
again). So: (a) no unanswered address exists — every I2C transaction
is served; (b) the ONLY timer event is T2_C0 (system tick?) + a
single T4_C0; (c) the waiter at `0x282a6` polls `[r0+#292]`/
`[r0+#336]`/`[r0+#512]`=TWIM1 ERROR/TXSTARTED/SHORTS — all steady
(0/0/`0x1000`). The missing event is therefore NOT a peripheral
event at all: with all buses served and all events quiet, the loop
waits on a FIRMWARE-SIDE flag (the `[r0+#292]`/`[r0+#336]` words are
struct fields, not MMIO — re-read P77: struct base `0x40004000` was
the STRUCT's +16 FIELD (a stored TWIM1-base copy), so +292/+336 are
struct+292/+336, NOT TWIM1+0x124/+0x150!). P77's "MMIO-alias" was
WRONG: the struct HOLDS 0x40004000 at +16 but polled offsets are
struct-relative. So the waited words are DRIVER-STATE fields (a
transfer-completion semaphore pair?) that our take/complete path
never sets — because take/complete stage the MODEL side, and the
driver struct fields are written by... the driver's own ISR, which
needs an IRQ our completion may not fire (INTEN=0 observed at park!
P65: INTEN reads 0 — with no INTEN bits, completion sets events but
pends NO interrupt, and an IRQ-driven driver never wakes).
NEXT: check TWIM1 INTEN programming from boot (does firmware ever
enable TWIM1 IRQs? if it relies on IRQs and INTEN stays 0 in our
run, find who should have enabled them) + whether the waiter is an
IRQ-wait (WFE/SEV?) or a polled flag the ISR sets.
Probes reverted; 191 green hold.

## 71. P80 INTEN never programmed + P81 latch verdict + P82 stub-matrix (2026-09-13, uncommitted)

P80 (native 120M, reverted): TWIM1 INTENSET stays `0x00000000` for the
whole run (no TRANSITION ever logged — the `FFFFFFFF→0` line is the
first-sample artifact; INTEN native init value is 0). NVIC ISER0
programs bits 9/18/26 (0x200→0x40002C4→0xC0002C4 at 0.36–0.40M =
TIMER1 + app IRQs), but bit 4 (SERIAL1/TWIM1) is NEVER enabled, and
`irq_pending(4)` is false at every sample. ipsr never reads 20, so
zero SERIAL1 ISR entries over 120M; `sleeping` never true at any
sample (`slphits=0`). Conclusion: the pre-banner firmware NEVER uses
TWIM interrupts — it is 100% POLLED (nrfx `waitForStop`, see below),
so the P70 "IRQ-driven driver never wakes" theory is DEAD. Bonus
trace: TXSTARTED 0→1 at 1.04M (first accel WHO_AM_I STARTTX) then
1→0 at 1.06M (firmware/UARTE-style write-0 clear — the SAME latch
hygiene as the cleared event regs); MAXTX 0→1→2→0/1/8/0/1 (driver
programs 1B pointer-set then 2B/8B writes); MAXRX 0→1; ERR/ERRSRC
never set (zero NACKs — every transaction ACKed). UART stays 0
through 120M, pc pinned `0x200021b8/ba`.

P81 (unit, reverted): TXSTARTED latches-until-cleared in the model
(STARTTX→1, write-0→0, STARTTX→1, no ticks involved) — the P80 1→0
was a firmware write-0 clear, NOT a model auto-clear. Model CORRECT;
no TWIM event-latch bug exists on this path.

P82 (native 4×120M stub matrix, reverted): sensor answers are
IDENTICAL across FAILFAST/BUSY/SENS-OFF (WHO_AM_I 0x33 + 4 CTRL
writes + 2 mag writes, then silence) — content after WHO_AM_I is
NEVER READ BACK (n_rx=1: only the WHO_AM_I RX; TX to 0x19/0x1E stop
after init). NONACK (no taps) diverges: accel init sequence ABORTS
(no CTRL writes), proving `isDetected` gates the whole sensor path —
yet ALL FOUR park identically at `0x200021b8`, uart 0. USB-flash
(`0x72`) traffic is irq1-GATED: pin HIGH (inactive) → 87 TX/0 RX
(write-only `_transact` visibility probes, `while(tx<MAX)` with no
`irq1.isActive()` read inside the TX retry); pin LOW (active) → 1
TX/1467 RX (the `while(rx<MAX)` read loop spins: `isActive()` true
→ read → `[0,0,0]` = NOT-busy (`b[0]==0` with BUSY_FLAG_SUPPORTED
clear — inferred branch) → `rx_attempts=0` FOREVER). So the demo's
fail-fast stub CONTENT never matters: real `isActive()`-true would
loop the same way (LSI `sample()` returns `[0,0,…]` for `0x39`!).
The demo DRDY pulse (60ms low) only modulates WHICH spin runs. The
park is therefore NOT a sensor/USB data wait: with sensors fully
answered AND flash reads completing, the waiter polls `[r0+#292]/
+336` = ERROR(0x124)/TXSTARTED(0x150) — both steady 0 — i.e. a
POLLED nrfx transfer-completion (`waitForStop(STOPPED)`) whose polled
event NEVER SETS. STOPPED not set ⇒ the transfer never terminates
on the MODEL side: candidates are (a) SUSPEND-instead-of-STOP
(nrfx SUSPEND path mistook for STOP by the waiter — P80 MAXTX shows
8B writes = multi-byte DMA; SHORTS at park is LASTRX_STOP only, so
a TX with LASTTX_SUSPEND... but SHORTS steady `0x1000` has no TX
bits at all — SUSPEND never armed either), (b) the waiter polls a
STALE snapshot (P61 stale-regs: r4=`0x3e8`/stack at the leaf), (c)
the transacted peripheral is NOT TWIM1 (the `[r4+#16]` base at the
live catch may differ per iteration — r7=260 indexes `[r0,r7]`,
an OBJECT TABLE slot, not a register!). NEXT: re-read the `0x282a6`
disassembly with FRESH eyes (which peripheral base does the CURRENT
`[r4+#16]` hold at park — TWIM0? SPIM? UARTE? — and what does r7=260
index into?) + check STOPPED/SUSPENDED at park (the two events the
`0x282d6` +292/+328 branch actually tests: +328=0x148=SUSPENDED).
Probes reverted; 191 green hold.

## 72. P83 waiter decoded: _i2c.waitForStop(STOPPED), r7 = 0x104, the NRF52I2C errata workarounds (2026-09-13, uncommitted)

P83 (native, reverted): single-step break at `0x282a6` catches the
waiter LIVE (6 consecutive hits, r4=`0x20002C0C` constant):
r6=0,1,2,3,4,5… (the `+292`-miss counter climbing toward the
`cmp r6,r9` / 99 cap), r7=**0x104**, r8=1, r9=`0xF4240` (1,000,000),
`[r4+#16]`=`0x40004000` (TWIM1). So the loop prologue is:
`ldr r0,[r4,#16]` (peripheral base), `ldr r3,[r0,r7]` with r7=0x104
= **EVENTS_STOPPED**, `cmp r3,#0; bne` (exit when STOPPED sets).
r7 is NOT a table index (P64/P72 "r7=260 off the rails" was a
misread: 260 = 0x104 = the STOPPED offset — the scan IS the poll).
`[r0,r7]`=0 every hit; STOPPED/ERROR/TXSTARTED/LASTTX/SUSPENDED all
0, SHORTS=`0x1000` (LASTRX_STOP). r8=1 (`mov r8,#1`), r9=1M: the
`0x282de` progress store (`str r8,[r0,#28]`) / `0x283a2` store
(`str r8,[r0,#20]`) write 1 to a TASKS register once the event
lands. Full branch map from the objdumped waiter (`0x28290`):
`+292`(0x124=ERROR)→`0x282ec` errata path (ERRORSRC snapshot,
`bl 0x52f64`, STOPPED-wait `0x28306` loop, DISABLE+ENABLE
`0x28318`, resume both I2C buses `0x28d5c`, re-init `0x52faa`,
`blx` bus callbacks); `+336`(0x150=TXSTARTED)→`0x28370` MAXCNT
check; `+352`(0x160=LASTTX)→`0x2838a` twin check (`+512`=0x200
SHORTS bit 9/10-gated STOPPED/SUSPENDED test); all-zero →
`0x282e6: bl 0x5048c` (fiber_sleep(1)?) + re-poll. This is
**NRF52I2C::waitForStop(STOPPED)** (`NRF52I2C.cpp:179-247`):
`while(!STOPPED){ if(ERROR||locked>TIMEOUT){...RESUME+STOP...};
if(TXSTARTED&&MAXCNT==0&&locked>=100)break(DEVICE_OK);
if(locked>=100&&LASTTX&&SHORTS&LASTTX_SUSPEND&&!SUSPENDED)TASKS_SUSPEND;
if(locked>=100&&LASTTX&&SHORTS&LASTTX_STOP&&!STOPPED)TASKS_STOP;
target_wait_us(10);}` — the `0x282a6` loop = the `while`, the
`+292/+336/+352` branches = the errata workarounds, `bl 0x5048c`
= `target_wait_us(10)` (via system_timer→TIMER1 CAPTURE spinning,
which is why the RAM delay leaf `0x200021b8`/`0x26039` dominates
all sampling). The polled event (STOPPED) never sets because the
transfer never completes; the sensor/USB CONTENT is irrelevant to
WHY (P82: identical sensor prefixes in all variants).
Probes reverted; 191 green hold.

## 73. P84 reversal: DRDY-LOW takes a different fast-fail path; the real gate is the DRDY-driven fork (2026-09-13, uncommitted)

P84 (native 4×200M, reverted — runaway, killed at the 600s tool
timeout): all four stub contents (FAILFAST/ECHO/ZEROS/VALID) with
DRDY held LOW show **tx72=0/rx72=0 over the full 200M** — ZERO
USB-flash transactions at all. Reversal vs P82 (DRDY HIGH: 87 TX/87
RX): DRDY level selects WHICH path runs. LOW (active) takes a
DIFFERENT path that never touches 0x72 — the sensor requestUpdate
`awaitSample` first-sample spin (P52: `do{}while(awaitSample)` on
`int1.isActive()` → `getDigitalValue`) consumes the boot BEFORE any
flash transact is reached, OR the KL27 `idleCallback` threshold
never fills... no: LOW should FILL it. Either way the flash path
is not even entered with DRDY held LOW from boot, so stub CONTENT
cannot be compared that way — P84 as designed was a null experiment
(all variants identical because the variable was never exercised).
Correct comparison: DRDY HIGH (flash path entered, P82 shape) ×
stub content ∈ {FAILFAST, ECHO}. P85 did exactly that (DRDY HIGH):
first-0x72-TX logged — 1B writes every ~1.36M, then 8B writes from
~29.8M — but the run ALSO hit the tool timeout (only reached 40M in
600s: the per-slice eprintln of every 0x72 TX costs ~15s/M... no —
~1.36M spacing × 20K-slice pump = fine; the timeout was the
full-suite build + prior P84 still running. Lesson: log COUNTS not
events, cap runs at 120M, one variant per invocation).
Probes reverted (P84 file + mod.rs); 191 green hold.

## 74. P86 THE BUG: `&0x7F` never un-shifts 0x32/0x3C/0x72 + echo-stub fix (2026-09-13)

Root cause of "MicroPython takes forever to boot while demos boot
instantly" — found live, fixed in tree (Rust + JS + smoke):

1. nrfx writes TWIM ADDRESS shifted (`addr<<1`): accel `0x19`→`0x32`,
   mag `0x1E`→`0x3C`, flash `0x39`→`0x72`, UIPM `0x70`→`0xE0`. The
   shifted values `0x32/0x3C/0x72` are ALL < `0x80`, so BOTH our
   `>0x7F`-style normalizers silently passed them through still
   shifted: the Rust take_* handoff (`&0x7F` masking) AND the JS
   `normAddr` (`a > 0x7F ? a>>1 : a`). P86 native proof: with DRDY
   HIGH the flash path IS entered (TAKE-TX `raw=72` every ~1.36M,
   `addr43=0x72`), but `tx72/rx72` counters (which normalized with
   the same broken rule) stayed 0 — the smoking gun that `0x72`
   never became `0x39` anywhere downstream.
2. Consequences, all three stacked: (a) the `0x39` tap never matched
   `slave_present()` for the DMA path's NACK check... (exact `0x39`
   vs `0x72` miss; the `>>1` fallback DID match, `0x72>>1=0x39`, so
   no NACK — the transfer staged); (b) the driver handoff told the
   stub `addr=0x72`, so `sample()` fell THROUGH the `0x39` branch
   into the sensor chain and returned ZEROS; (c) zeros = NOT-READY
   (`b[0]==0x00` with BUSY_FLAG_SUPPORTED clear) → `rx_attempts=0`
   → full 20×20 `fiber_sleep(1)` retry budget PER transact (×2 with
   the NULL-transaction wrapper) = the entire pre-banner boot time.
   Error responses would ALSO burn it (`0x20` + `b[1]∈{request,0}`
   is busy too — the old FAIL-FAST `[0x20,0x01]` matched the busy
   pattern for request `0x01`, and fell to `break`/empty-return for
   the rest, still slow). The ONLY fast exit is `b[0]==request[0]`
   (valid response) — what a real KL27 returns.
3. Fixes (this tree): `twim_nrf.rs`: `Twim::norm7_addr` (explicit
   `0x32→0x19/0x3C→0x1E/0x72→0x39/0xE0→0x70` + generic `>>1`
   fallback), used by `slave_present()`, START-event push, and BOTH
   `take_*` (7-bit handoff contract); `address_matches_shifted_8bit_
   form` test extended (take-normalization per raw form). JS
   `lsm303.js`: same explicit table in `normAddr` (idempotent —
   take_* now arrives normalized) + the `0x39` stub answers VALID
   request-echo frames (echo + parseable `0x01` filename body).
   `smoke.mjs`: fail-fast expectation replaced with echo checks.
4. Why demos boot instantly: blinky/sensors/dma never touch the
   USB-flash transact path (no `MicroBitLog` ctor chain) — only
   MicroPython pays the KL27 tax, and only because of (2).
5. Open threads (NOT blockers of this fix): banner STILL not
   observed post-fix in the windows run so far (P86 echo run to 120M
   shows tx72 climbing 13→87 but uart=0 — the transact rate itself
   is still ~1.36M/write, i.e. each transact still costs ~1M of
   `target_wait_us(10)` spinning; the echo makes EACH transact exit
   on attempt 1 but the NULL-transaction wrapper doubles them and
   `getConfiguration`+`getGeometry` issue ~6 transacts = ~10M+ of
   unavoidable init — banner needs a LONGER run to confirm, exactly
   like P53's 160–180M native threshold); quantitative before/after
   banner-time comparison still needs one long browser run.
Probes reverted; suite file-free again.

## 75. P86 BROWSER PROOF: banner + `>>> ` prompt in ~60s (2026-09-13)

Headless Chromium (`p86banner.py`, rebuilt pkg with the norm7_addr +
echo-stub fixes): T+60s `uartLen=104` =
`"MicroPython v1.18 on 2023-10-30; micro:bit v2.1.2 with nRF52833\n
Type "help()" for more information.\n>>> "` — banner body AND the
REPL prompt, zero page errors. Pre-fix the same run needed 150s+
(P20, faster machine) or never arrived (480s ≈ 144M < 150–260M
threshold); now the KL27 transact tax is gone and boot completes in
~60s wall. The pc/mips fields read `?` (the `__dbg` handle added by
earlier TEMP page edits is reverted — cosmetic probe gap, not a
product gap). REPL exec (`print(1+2)` → `3`) is the next frontier,
still open; the NULL-fault/pin-poll layers (P24–P25/P44) now get a
fast iteration loop (~60s/boot instead of never).
Probes: `p86banner.py` lives in /tmp (ephemeral, not committed).

## 76. P87 REPL exec WORKS: `print(1+2)` → `3` in-browser (2026-09-13)

Headless Chromium (`p87repl.py`, committed pkg from P86, no code
changes — the fix WAS P86): banner at T+60s (`uartLen=104`), then
`print(1+2)` typed + Send → R+60s `uartLen=121` =
`"...>>> print(1+2)\n3\n>>> "` — echo + result + fresh prompt,
stable R2m/R3m, zero page errors, no fault. LEFT-1 (STATUS §6.1,
"REPL exec BLOCKED BEHIND BANNER") is CLOSED: it was never a
separate input-path bug — the RX drip + DMA-mirror path (P39) was
already correct, and every "parked-main / pin-poll / NULL-fault"
post-banner layer (P24–P25/P40–P44/P51) was observed on
schedule-starved or NACK-degraded runs that the KL27 retry storm
explains. With the transact tax gone the scheduler runs, readline
consumes, MP executes, TX stages. Remaining REPL-adjacent work is
normal product depth (multi-line, paste burst pacing, Ctrl-C/D —
untested), not a bring-up gate. Next frontier candidates: MakeCode
scroll content (LEFT-4, pre-scroll sequencing still open) or the
bootloader full chain (LEFT-3).
Probes: `p87repl.py` lives in /tmp (ephemeral, not committed).

## 77. P91 MakeCode pre-scroll gate: fiber-wait alive, no HardFault (2026-09-14, uncommitted probe, reverted)

MC91 native probe (direct-app boot + browser-parity pump, 300M, reverted —
`tmp_mc91.rs` + `mod.rs` hook deleted, 191 green hold). Three legs:

1. Refined sampling: the 1K-quanta trap got 0 hits (stepped clean over the
   poll); single-step + flag-byte watch at `0x20004373` (literal @`0x2e4ec`)
   + stack-pivot watch hit. At ~100M the thread unwinds through a
   handler->thread EXC_RETURN (`ffffffe9`) trail back into `0x2e0f6`/
   `0x2e4e0` with r5=`0x20004373` (flag byte `0x01`) — the `0x2e4d8`
   fiber-wait loop (`bl 0x2e0b8`; poll flag bit31; `bl 0x2e100` scheduler
   dispatch) is entered AND dispatching, not stuck.
2. Static decode (full `/tmp/mc_full.dis`, `bl`-target grep — the earlier
   zero-hit grep was a regex/format mismatch, not missing callers):
   `0x2e0b8` = flag-set + queue-head check (`0x20003b18`) + yield-or-WFE
   via `0x25c9c` (which parks in `wfe @0x37af8` when nothing pends, else
   `bx r3` dispatches); `0x2e100` = scheduler dispatch off qhead
   @`0x20003b08`; `0x31f80` = timed acquire (`bl 0x31ed4` search, halfword
   @`[r4,#38]` vs 9, marshal + `bl 0x4303a`). Stack words seen at park
   (`0x31c51/0x2e4e1/0x2e4d9/0x20873`) are UNVALIDATED (no `bl`-check) —
   not a call chain, do not cite.
3. Passive HardFault gate (no single-step distortion, 100M->300M, CFSR/HFSR/
   stacked-PC dump on `ipsr==3` inside vector[3] window): `fault=None`
   throughout, CFSR=0; per-round `ipsr=0` at every 20M sample (WFE
   `0x37afa`, RAM `0x2000207a` 200-260M, back to WFE). `ipsr=3 @0x37f4e`
   (default-handler self-loop, vector3=`0x37f4f`) appears in SOME runs
   only — nondeterministic across identical binaries (wall-clock DRDY
   pulse reschedules); cause never captured with CFSR set (one-shot gate
   burned its budget while the CPU slept in WFE and exited before the
   ~100-120M transition). Note: the model never latches HFSR.FORCED (no
   ED2C write in `cpu/`), so HFSR reads 0 even on a real HF — CFSR is the
   cause field, and it stayed 0.
Verdict: no crash behind the blank display; scheduler alive +
dispatching; gate unchanged = pre-scroll sequencing (main never issues
scroll; TIMER4 COUNTER 0, DIR0 0, uart 0).
NEXT: fiber-queue walk — dump the qhead @`0x20003b08` chain + fiber state
words (`[fiber+#16]` bit31 runnable?, `[#20]`) per round: does the scroll
fiber exist and is it runnable?
Probes reverted; 191 green hold.

## 78. P92 fiber-queue walk: the scroll fiber does not exist (2026-09-14, uncommitted probe, reverted)

MC92 native probe (P91 recipe + per-round queue walk, 300M, reverted —
`tmp_mc92.rs` + `mod.rs` hook deleted, 191 green hold):

1. CODAL struct (ground truth `codal-core/inc/core/CodalFiber.h`):
   `Fiber{+0 tcb, +4 stack_bottom, +8 stack_top, +12 context, +16 flags,
   +20 queue, +24 qnext, +28 qprev, +32 next}`; queues link via QNEXT
   (+24), NOT +0 (the first P92 draft walked +0 = the TCB pointer and
   would have chased garbage — caught before running). TCB is the nRF52
   `PROCESSOR_TCB` (R0-R12,SP,LR,stack_base = 16 words).
2. Steady-state queues (identical all 15 rounds, 20M..300M): run
   @`0x20003b08` = ONE fiber `0x2000621c` (TCB SP `0x2001fe9c`,
   LR `0x2e453` = inside the `0x2e410` waiter, flag byte via R8-slot
   `0x20004373`); sleep @`0x20003b20` = TWO fibers (`ctx=0x00010007`
   key-matched sleeper + `ctx=0x2d58/0x2ee` event waiter); event-wait
   @`0x20003b18` and t14 EMPTY; t24 mirrors run (same head — an aliased
   queue slot, not a second list). NO third fiber anywhere: no scroll
   fiber exists on ANY queue.
3. The parked run fiber IS main: TCB LR `0x2e453` sits in `0x2e410`
   (`fiber_wait_for_event`-shaped: flag-byte check at literal `0x2e4ac`
   = `0x20004373`, encode id/value, dequeue, queue to wait, `schedule`);
   its stack (`0x2001fe9c`) holds a `bl 0x2dc08` pump frame
   (`0x31ffd @0x2e328` region: `0x2e314` event-drain + `0x2dc08` arg-
   marshal) under the `0x35697` caller — a message-bus pump/wait frame,
   i.e. main blocked waiting for an EVENT that never arrives, while
   `0x20003b18` (the wait queue fibers block ON) stays EMPTY.
   TCB SP slot `0x200063e8` vs live SP `0x2001fe9c` = saved-vs-live
   window, normal for a descheduled fiber.
4. The display path is never even constructed: the pump caller
   `0x35697` sits in a `bl 0x2e99c`-then-`ldr [r5,#20]` region
   (`0x35664`: `ldr r3,[r0,#20]` display-member load, NULL-checked,
   `bgt 0x3573a` skips the `0x2e99c` fiber-create when the member/slot
   test fails) — consistent with TIMER4 COUNTER 0 + DIR0 0: no strobe
   timer, no row/col DIR, because the scroll call never happened.
   Stack words `0x31c51/0x2e4e1/0x2e4d9/0x20873` (P91) remain
   UNVALIDATED — not a call chain, do not cite.
Verdict: gate moves one level up — the scroll fiber is never CREATED
(main parks in event-wait first). NEXT: who should create/wake it —
dump `0x20004373`-adjacent event state + the `0x35664` member-null
inputs (r0/r5 at the `0x35696` NULL check): is the display member NULL
(construct skipped) or is the event subscription missing?
Probes reverted; 191 green hold.

## 79. P93 member-vs-event: the bus pump itself never runs (2026-09-14, uncommitted probe, reverted)

MC93 native probe (P92 recipe + waiter/member dumps, 300M, reverted —
`tmp_mc93.rs` + `mod.rs` hook deleted, 191 green hold):

1. Waiter inputs (parked main TCB, P92 layout): R0=`0x20006248`
   (= its OWN TCB pointer, not an event id), R1=0, R2=`0x200063d8`
   (fiber struct), R3=`0x2000621c` (self). So `0x2e410` was entered
   via the `bl 0x2e01c` pump path (`r5→r0` message object, `r1`=listener
   list head), NOT via `fiber_wait_for_event(id,value)` — P92's
   "event-wait" label was wrong. Main is inside
   `EventModel::send`→`0x2e450 blx r5` (listener invoke) → the listener
   blocked (`0x2e484 bl 0x2e314` scheduler entry), i.e. main is
   delivering/posting an event whose listener parks it.
2. Heap-guard lasers (the `0x30770` fiber-create pattern:
   `cbz r6→0x307bc` heap-empty exit; pool words @`0x20003b3c`/
   @`0x20003b38`; size halfword @`0x20002079`): @20M `b38=0x20`
   (32 = heap EMPTY — every block consumed), `b3c=0x20002598`,
   `size2079=0xfd38` (GC'd halfword pair `3801/fd38`, live heap, not
   zeros). So a later `create_fiber`/`0x2e99c` can NEVER succeed — the
   allocator's free list is drained before the scroll fiber is made.
   (Whether 0x20 is genuinely-empty vs corrupt-head is open; the
   `0x307bc` exit reads the same word.)
3. The `0x35664` region is downstream noise: the parked stack's
   `0x35697` return is `0x35696 ldr r3,[r5,#20]` (member re-load after
   the `bl 0x2e99c` fiber-create attempt), and the `0x2e99c→0x30770`
   trampoline means the create ATTEMPT happened but returned NULL
   (heap empty) — the `bne 0x3573a` / `beq 0x3570c` exits are the
   NULL-fiber paths, taken. Display member NULL-ness was never
   reached: starvation happens one level below, in the allocator.
Verdict: gate moves to the HEAP — the scroll fiber create fails for
lack of memory (or a drained free list), not for lack of subscription.
NEXT: heap walk — dump the free-list head @`0x20003b3c` chain + block
headers around `0x20002598`: is the heap truly full (leak?) or is the
free list corrupt (bad free/unlink)?
Probes reverted; 191 green hold.

## 80. P94–P96 waiter-nature correction: heap NOT empty, 0x2e99c is not malloc (2026-09-14, uncommitted probes, reverted)

Three native probes (P91 recipe + browser-parity pump, reverted —
`tmp_mc94.rs` + `mod.rs` hook deleted, 191 green hold). Net effect is a
CORRECTION of P93 §79, not a new gate:

1. P94 heap walk (20M + 300M dumps): `b30=0x2001f800 b34=0x20002ba8
   b38=0x20/0x00/0x21 (varies run to run) b3c=0x20002598` stable;
   `size2078/79=0x3801/0xfd38` GC'd live heap. The free node @`b3c` is
   PRESENT in every dump (full node: `00045194 002e03f2… 20002534
   00000100…`), so the free list is NOT drained — P93's
   "`b38=0x20` = heap EMPTY" reading is unsupported (`b38` fluctuates
   across identical runs; the allocator's empty-exit is `cbz r6→0x307bc`
   on `[b3c]`, which never fires). Chain-walk via node `+0` ends at
   flash addr `0x00045194` (not a RAM link), so the node linkage offset
   is still unknown — the walk did not decode the free list, it only
   proved a free node exists. Fiber/TCB/stack block headers all live.
2. 0x2e99c is NOT malloc (static, all 10 `bl 0x2e99c` sites): every
   caller overwrites r0 on the very next insn (`0x2408c ldrb`,
   `0x35506 movs r5,#0`, `0x35696 ldr r3,[r5,#20]`, …) — a malloc
   retval would be consumed, never clobbered. Args seen live are
   `r0=4/10` (P95/P96 entries, `lr=0x35507/0x35697` pump loop) = sleep
   ticks / wait codes, not alloc sizes. P95's stage-2 also exposed a
   methodology flaw: single-stepping while the CPU sleeps in WFE
   executes nothing (`run()` returns 0 asleep), so the 5M-step window
   burned with 0 hits — traps must be awake-gated.
3. P96 two-level trap (1K-quanta level-1 + ≤3000-insn awake-only
   level-2, 6 windows): `0x2e99c/0x30770` entries `r0=0x0a`,
   `ret @0x3569a r0=0` (ignored, consistent with §2),
   `0x35696: r0=0 r5=0x20002c10 [r5+20]=0x40004000` (TWIM1 base — r5 is
   a driver/bus object, NOT the uBit base; P93's member framing was
   wrong), `0x35664#1: r0(obj)=0x20002c10 r1(slot)=0x104
   lr=0x2711f`, `0x2e410` entries ZERO (waiter inputs never trapped
   while parked). Run LOUDHALTed at RAM `pc=0x200044b8` (len-4 fault)
   at 2M UNDER single-step distortion only — undistorted runs are
   `fault=None` to 300M, so the halt is probe artifact, do NOT cite
   as a guest fault.
4. Event path (static, for the record): raise helper `0x2e084`
   (flag byte @`0x20004373` via literal `0x2e0a4`, wait queue
   @`0x20003b18` via `0x2e0b4`/`0x2e080`); waiter `0x2e410` (flag via
   `0x2e4ac`, run queue @`0x20003b08` via `0x2e4a0`).
Verdict: P93's heap-starvation gate is REFUTED as framed (free node
present, no malloc in path, no NULL-create observed). Waiter-nature of
`0x2e410` still open. NEXT: awake-gated trap on the raise path
`0x2e084` + waiter entry `0x2e410` with full pump parity, or static
decode of `0x35664`'s caller chain (`bl 0x35664 @0x358c8`,
`b.w` sites `0x357dc/0x35808/0x358b4/0x358e4`).
Probes reverted; 191 green hold.

## 81. P97 static + awake-gated dynamic: waiter shape decoded, trap starved by WFE duty (2026-09-14, uncommitted probe, reverted)

Static leg (no probe, `/tmp/mc_full.dis` only):

1. `0x35664` caller chain: ONE `bl` site `@0x358c8` (`mov r1,#0x15c;
   mov r4,r0; bl 0x35664; cbz r0→0x358d2` — retval CONSUMED, so this
   call's return is a nullable pointer) + four `b.w` TAIL calls
   (`0x357dc/0x35808/0x358b4/0x358e4`, all with `r1=#0x104/0x148/0x15c`
   slot immediates + `pop`-then-branch epilogues). Head:
   `ldr r3,[r0,#20]; uxth r6,r1; ldr r2,[r3,r6]` = slot-table lookup
   on `[obj+20]` (r0=obj `0x20002c10` live, r1=slot `0x104` live) —
   a member-getter, not a fiber creator (only TWO `bl 0x2e99c`
   sleep/wait calls inside, both retvals ignored). NULL path `0x3573a`
   (`movs r4,#0; b 0x35722`) vs complete path `0x3573e`
   (`ldr r3,[r5,#0]; ldr r2,[r5,#16]; ldr r3,[r3,#60]; ldr r1,[r5,#12];
   blx r3` = virtual dispatch + `b 0x35722` shared return) — the live
   park sits between them, never reaching either.
2. Raise `0x2e084`: `ldr r3,[0x2e0a4]=[0x20004373]; ldrb; and #1;
   beq 0x2e0a0-ret0` else `push{r4}; shuffle(r1,r2); pop r4 into
   caller slot; b.w 0x2e01c` — flag-gated FORWARD to the pump
   `0x2e01c`, sharing the wait queue @`0x20003b18` (literals `0x2e0b4`/
   `0x2e080`). Waiter `0x2e410`: `cmp r0,#0; beq 0x2e49a-ret-1000`
   else flag @`0x20004373` (via `0x2e4ac`), run queue @`0x20003b08`
   (via `0x2e4a0`), `bl 0x2e01c` pump call — a NULL-or-flag-gated
   pump ENTRY, not a bare event id wait.
3. `bl 0x2e410` has exactly ONE static site (`@0x31f62`, inside the
   `0x31f00` acquire loop `ldr r4,[r4,#36]`); all per-round parked
   entries arrive via the runtime-constructed `blx`/`bx` dispatch
   (`0x2e466 blx r5`, `0x2711c blx r4`), so waiter-entry trapping must
   be by pc, not by caller.

Dynamic leg (P97 probe, P91 pump parity, reverted — `tmp_mc97.rs` +
`mod.rs` hook deleted, 191 green hold): two-level design (20K-quanta
level-1 + ≤3000-insn awake-only level-2, 6 windows). Result: 300M,
`fault=None`, but ZERO windows and ZERO trap hits. Cause: per-round
park pc is ALWAYS the WFE `0x37afa` and `!cpu.sleeping` is false at
every 20K boundary — the trigger never fires because the main-thread
work (pump region, waiter entries, RAM delay `0x20002078/7a`) all
happens INSIDE the quantum between samples. Widening the trigger to
dispatch/fiber-wait/RAM pcs fired 24 windows in 0M, all inside the
PXT RAM delay loop (`0x20002078/7a`, RAM-resident — no flash disasm,
handler never entered). So the trap geometry is proven wrong, not the
hypothesis: quantum-boundary sampling cannot catch microsecond-scale
awake bursts.
Verdict: waiter-nature of `0x2e410` STILL open. Method options ranked:
(a) exact-pc break via a `mem.read`-side hook or IPSR/pc-change poll
inside the quantum (cheap: check `cpu.regs.r[15]` after each
`run(…,1)` — no, that IS the distortion; instead shrink quanta to
~200 with pump parity and accept ~100x slowdown for ONE 20M round);
(b) static-only: decode `0x2e01c` pump + `0x2e314` scheduler entry and
name the waiter from its queue args (`0x20003b18` wait-queue walk at
park — P92 dumped heads only, never the waiter OBJECTS on it);
(c) park LEFT-4 now: pre-scroll sequencing root-caused to
"main parked in pump waiter, scroll never constructed", display path
fully characterized as never-touched (TIMER4 0, DIR0 0).
NEXT (decision for next session): (b)-then-(c) — one wait-queue-object
walk is the cheapest remaining evidence; if it names a sensor/display
event id, trap THAT raise site, else park LEFT-4 and switch to LEFT-3
bootloader chain.
Probes reverted; 191 green hold.

## 82. P98 BLE air + SoftDevice SVC face (2026-09-14, committed P98a–P98e)

User asked to start BLE/BT (P58 scope sketched, no build). Built BOTH
faces, committed in 5 small steps (each cargo-test green):

- P98a `radio_inject_rx_to` export (DAB-addressed inject; DEVMATCH path).
- P98b `tools/ble_air_bridge.py` (Bumble LocalLink: C_emu + C_peer/peer
  battery GATT + C_central over-air reader; WS tx/rx/gatt/ble_read/
  ble_connect/connected) + `demo/parts/ble_air.js` (BleAir WS part,
  take_*/sendTx/sendBle/takeAir discipline).
- P98c `src/sd_ble.rs`: SoftDevice SVC face — S132 enum numbers
  (ENABLE 0x60, EVT_GET 0x61, GAP 0x70.., GATTC 0x90.., GATTS 0xA0..),
  GAP local acks + CONNECT/GATTC-READ staging take_job(), GATTS
  battery table, evt queue drained via sd_ble_evt_get (header+body,
  evt_len includes 4B header), complete_*/post_* driver completions;
  thumb.rs SVC hook claims 0x60..=0xBF first (r0 + skip, else fall
  through to raise_sync — zero-cost when idle); reset_for_test wired
  into reset_globals; 4 native tests + SVC-hook proof in cpu/tests.rs.
  Suite 195 green (191 + 4). AGENTS.md note: the SVC hook touches
  src/cpu/thumb.rs (decoder, not board logic) — minimal, range-gated,
  behavior-preserving for all non-BLE SVCs; sd_ble itself is NOT a
  Peripheral (no MMIO, SVC interface per docs/sd_evt_design.md §4).
- P98d demo pump BLE jobs (bridge read/connect, local loopback
  default) + MockBleSvc handshake proof (17 OK headless).
- P98e rebuilt demo/pkg (ble_* exports live).

NEXT: live bridge run (python3 tools/ble_air_bridge.py + Enable BLE
air + TX/RX round trip + GATT battery over-air value on panel);
firmware-level proof (bare-metal SVC caller → enable → evt_get, or a
BLE-enabled image exercising GAP/GATTC SVCs — today's MPY/MC images
never init the stack, P58).

## 83. P99 BLE/GATT full fix: correct SVC face + live air proof (2026-09-15)

P98 wired the shape but left real gaps (found by header audit against
the S132 headers + live bridge runs): wrong GAP SVC numbers
(SCAN_START 0x89 vs 0x8A, CONNECT 0x8B vs 0x8C), single-arg evt_get,
missing conn_handle in GATTC envelopes, packed ADV_REPORT (missing
u16 pad), CONNECTED role=PERIPH, pseudo-handles (0x10/0x13) instead of
a table, VALUE_SET/GET register-form instead of struct-form, no
WRITE/DISCONNECT/RSSI/DESC jobs, bridge resolving locally instead of
over air, GATT reply without conn/handle echo.

Fixes (all S132-verified, committed in steps below):
- sd_ble.rs: full enum tables (common 0x60-0x69, GAP 0x70-0x8E,
  GATTC 0x90-0x99, GATTS 0xA0-0xAC with names); two-arg evt_get
  (length query, DATA_SIZE, legacy drain); gattc envelope head on
  every GATTC RSP; unpacked pads (ADV_REPORT pad, WRITE_RSP/HVX
  pads); CENTRAL role + real conn_params; attribute table with
  struct-form VALUE_SET/GET (conn 0xFFFF ok, length query,
  offset checks); 10 BleJobs (connect/disconnect/rssi/scan/
  prim/char/desc/read/write/hvx) with bytes copied at SVC time +
  take_data export; pairing/crypto refuse INVALID_STATE (no ghost
  crypto); 5 native tests incl. byte-offset asserts (196 green).
  Two real bugs caught by the new tests: check_conn `.err()?`
  returning None on success (8 arms staged nothing), HVX p_len/p_data
  at +6/+10 vs S132 +8/+12.
- Bridge: peer now battery (READ+NOTIFY) + Nordic UART (RX write,
  TX notify) with live handles logged; full job protocol
  (ble_read/write/desc kind=3/hvx/scan/connect/rssi/disconnect) with
  conn/handle echo + cancel (never ghost events); generic
  read-handle-by-walk; CharacteristicProxy handle mapping
  (value=decl+1, no value_handle attr — probed) + desc .type fix.
  Live-verified: read [87] overAir, prim_disc (4 svcs incl. 0x180F),
  char_disc incl. 0x2A19, write NUS RX, hvx notify [85,86], scan,
  connect, RSSI(adv-derived — no HCI_READ_RSSI on LocalLink, probed),
  disconnect. Serialized-link limit: one Bumble link op at a time;
  back-to-back WS jobs queue behind the in-flight ATT burst.
- Demo: pumpBleLoopback mirrors the bridge table for all 10 tags;
  pump drains prim/char/desc_disc_rsp + write_rsp + hvx + rssi +
  cancel(reads fall back to battery, writes stay silent for retry).
- MockBleSvc v2 executes REAL SVC bytes (ldr preamble + svc + b .)
  on a WasmCpu: enable→table→connect→disc→read→write→scan→rssi→
  disconnect, every event drained and byte-checked (18 mocks OK).
  Two mock bugs fixed along the way (ldr-literal bases, CONNECTED
  role + ADV pad body offsets).
- API.md gains the BLE section (tags, take_data, completes, bridge
  protocol, loopback table).

NEXT: firmware-level proof on a BLE-enabled image (today's MPY/MC
never init the stack, P58); in-demo live-bridge run from the bench
panel (Enable BLE air + TX/RX + battery over-air line).

## 84. P101 bench-wired BLE: panel, probes, docs, wasm-first verify (2026-09-15)

All five asked, all five done:

1. BLE panel wired to real state: stack (`ble_enabled()`), links
   (`ble_conn_handles()`), queue (`ble_queue_len()`), battery
   (`ble_batt_level()`) refresh after every self-test + depth run —
   no more static "bridge not connected" line as the only signal.
2. BLE self-test button: runs the full MockBleSvc SVC flow on
   loopback in-page (enable, table, link, GATT, L2CAP, pairing, scan,
   RSSI, disconnect). Two real shared-model bugs found by running it
   in the real page and fixed: hardcoded conn handle 1 (model assigns
   from its link table — the mock now threads the returned handle
   everywhere) and CID_IN_USE on re-run (tolerated: the depth probe
   run holds the same CID on the shared core).
3. Depth probes include the BLE stack probe (16/16 in real Chromium,
   zero page errors).
4. API.md finished: tags 10/11, `ble_complete_gap_connect_ret`,
   `ble_complete_pairing`/`ble_fail_pairing`, `ble_complete_l2cap_rx`,
   `ble_conn_handles`/`ble_conn_sec`, `paired`/`l2cap_rx` bridge
   messages. doc.html board matrix gains the BLE row (F) and the SVC
   row stops claiming SoftDevice unmodeled; suite count 201.
5. wasm-first verify: `cargo test` 201 green (native oracle) +
   rebuilt both pkgs from current src (fixed `build:wasm` /
   `build:handshake` scripts — they pointed one directory too deep
   and wrote stray `nrf52833-periph-wasm/{pkg,parts,demo}` trees) +
   handshake 18/18 + smoke + MPY-idiom face vs the BUILT pkg +
   real Chromium: blinky `BOOT/BLINK/BLINK` at 6 MIPS, probes 16/16,
   self-test pass, zero page errors.

NEXT: live-bridge run from the bench (Enable BLE air + Bumble peer);
firmware-level proof on a BLE-enabled image (MPY/MC still BLE-less).

## 85. P103 SMP pairing legs + conn-RSSI + multi-peer + ATT queue (2026-09-15)

All four asked, all four done (`cargo test` 203 green, handshake 18/18,
smoke + MPY-idiom face vs the BUILT pkg, live E2E 42/42 over air):

1. SMP pairing handshake (sd_ble.rs + lib.rs + mocks.js + index.html):
   peer-initiated request events (SEC_PARAMS_REQUEST 0x13 /
   SEC_INFO_REQUEST 0x14 / AUTH_KEY_REQUEST 0x17 / PASSKEY_DISPLAY
   0x15 / KEY_PRESSED 0x16 / LESC_DHKEY_REQUEST 0x18 — ids from
   BLE_GAP_EVT_BASE 0x10 + ble_gap.h enum order) with conn-first wire
   bodies (conn u16 head, then the ble_gap.h params: 5B sec_params,
   7B addr + 10B master_id + req bits, 6B ASCII passkey + match bit,
   kp_not/key_type/oobd_req bytes); per-link Pairing state machine
   (Idle/Requested/PeerRequested/Accepted/KeyEntry/LescDhkey/
   EncryptPending); full reply surface (SEC_PARAMS_REPLY accept needs
   an outstanding request else INVALID_STATE, AUTH_KEY_REPLY
   validates key_type + 6-digit passkey + 16B OOB, LESC_DHKEY_REPLY /
   KEYPRESS_NOTIFY / ENCRYPT / SEC_INFO_REPLY each gate on their
   outstanding state, OOB_DATA_GET zeroes 32B, OOB_DATA_SET acks);
   S132 SEC_STATUS codes (0x81/0x82/0x83/0x84 passkey/OOB/auth/confirm
   + 0x85 pairing-not-supp) incl. the 0x29→0x85 fix (0x29 is an ATT
   error, not a GAP status); AUTH_STATUS body fixed to conn-first
   (was missing the ble_gap_evt_t head — old tests read status at the
   wrong offset); `complete_pairing` leaves Accepted links Accepted
   for the key legs (initiator Requested legs still go Idle);
   6 new wasm exports (`ble_post_sec_params_request`,
   `ble_post_sec_info_request`, `ble_post_auth_key_request`,
   `ble_post_passkey_display`, `ble_post_keypress`,
   `ble_post_lesc_dhkey_request`); pump drains the 6 new bridge
   messages; new native test (peer-request→accept→passkey→keypress→
   AUTH_STATUS+SEC_UPDATE→re-encrypt→keys→ENCRYPT→LESC→OOB→display)
   + MockBleSvc peer-pair leg through real SVC bytes (0x7F/0x80).
   Two mock-ordering traps fixed along the way (AUTH_KEY_REQUEST must
   sit ahead of the handshake's AUTH_STATUS in the FIFO; the
   GapAuthenticate take must drain before `complete_pairing`).
2. HCI RSSI path: `read_conn_rssi` opens the link and issues
   HCI_READ_RSSI (Device.get_connection_rssi shape: send_sync_command
   on the host connection handle) — SoftDevice-faithful, since
   sd_ble_gap_rssi_get samples the CONNECTION. Probed on LocalLink:
   virtual controller answers UNKNOWN_HCI_COMMAND (HCI_Error, caught
   inside — an uncaught raise starved the WS loop and hung every later
   job, fixed by catching inside read_conn_rssi), so ble_rssi falls
   back to the live advertising sighting + RSSI_CACHE, tagged
   `src:"conn"|"adv"`. E2E asserts overAir + number.
3. Multi-peer air: second peer (`make_peer_hr`: same battery+NUS
   table, value 64 `PeerHR`, distinct random address) on the same
   LocalLink; per-job `peer:[6]` routes every ATT op (ble_connect's
   `addr` selects the same way); `ble_scan` reports one adv_report
   per peer; disc RSPs echo the answering `peer`. Real bug found by
   the E2E: both peers share handle numbers (decl 16/value 17), so
   the resolve_read fallback walk answered from the WRONG peer (first
   link read 64) — fixed by addressing every read (known-handle fast
   path + decl+1-mapped walk on the addressed peer). E2E grows a
   second link (connect→disc→read 64→disconnect, handles differ,
   FIRST_PEER echo asserted, scan collects 2 distinct sightings):
   42 checks green. ble_air.js forwards `job.peer`.
4. Serialized ATT queue: per-peer-address ATT locks (kept) + one
   global scan lock — scans share the central's single scanner, and
   scan-then-connect handoffs (gatt_read_battery_over_air,
   read_handle_over_air) hold it across both steps so a second job's
   scan cannot interleave (timeouts under load before this).

NEXT: firmware-level proof on a BLE-enabled image (MPY/MC still
BLE-less); browser 16/16 re-run on the rebuilt pkg.

Browser re-run (P103, this tree, committed pkg): `python3
tools/browser_verify_16.py` vs bench on :8080 — blinky
`BOOT/BLINK/BLINK` at ~6 MIPS, BLE self-test
`pass: enable, table, link, GATT×7, L2CAP, pairing×2, scan, RSSI,
disconnect`, depth probes 16/16, zero page errors. (Script uses the
pip `playwright` package + its bundled Chromium; the earlier
`playwright-core` node module is not installed here. Two script bugs
fixed along the way: preset select STAGES (Run boots — there is no
separate Load button, `#fwrun` is it) and the UART box is a
`<textarea>`, so read `.value`, not `.textContent`.)

## 86. P104 BLE pairing-fw image proof + static SVC rescan (2026-09-15, uncommitted)

Workstream A asked for a BLE-enabled image proof + static rescan of
the MPY/MC app regions. Both done, no model change (`cargo test`
204 green single-threaded — +1 pairing-fw test).

1. Pairing-fw image (`blinky/ble_fw/ble_pairing_fw.c` → `.bin`,
   xpack GCC 14.2.1 + `blinky/link_c_nrf.ld`, RWX-LOAD-segment ld
   warning harmless): CODAL-BLE-shaped JustWorks flow — ENABLE →
   GATTS battery service+char → CONNECT (staged, driver completes) →
   CONNECTED drain → PRIM_DISC/CHAR_DISC/READ/WRITE (staged) →
   AUTHENTICATE (staged, driver `complete_pairing(conn, true)`) →
   AUTH_STATUS (0x19) + CONN_SEC_UPDATE (0x1A) drain → CONN_SEC_GET
   (expects encrypted mode `0x21`) → DISCONNECT (staged) →
   DISCONNECTED drain. 17 `BLEP:*` markers incl. `BLEP:ALL-OK`.
   SVC dispatcher macros copied from `ble_conformance.c` exactly
   (SVC1/2/3 + the manual 4-reg CHAR_ADD block — SVC3 cannot carry
   r3). Objdump check: `svc 96/160/162/140/144/146/150/152/126/135/
   118` all present.
2. Native test `nrf_ble_pairing_fw_markers` (cpu/tests.rs, mirrors
   `nrf_ble_conformance_svc_face`): 2-run loop with
   `reset_for_test()` + `lock_boot()` + `boot(include_bytes!(
   ble_pairing_fw.bin))`, 500-instr slices × 4000 with
   `pump_ble_test_driver(sys)` between slices (mid-spin pump is
   load-bearing — P54 lesson; `pump_ble_test_driver` reused
   unchanged, CONN_SEC_GET is synchronous so no driver arm needed),
   break on `BLEP:ALL-OK`/`SOME-FAIL`, assert no fault + every
   `BLEP:` marker + `reset_globals()` per run.
3. Static SVC rescan (ad-hoc `/tmp/opencode/svc_rescan.py`, never
   committed): Intel HEX with BOTH type-02 (segment<<4) and type-04
   (upper<<16) records (MPY needs both — type-02-only parsers
   silently miss app bytes), app filter `0x1C000–0x77000`,
   imm@even/DF@odd halfwords (Thumb-2 `svc #imm` = `0xDF00|imm` LE,
   so the DF byte sits at the ODD address — the earlier DF@even
   scan had the polarity backwards and counted data). Results:
   MPY 83 SVC-shaped sites (BLE-range 42 distinct / 55 sites —
   ENABLE/EVT_GET/ADV_DATA_SET/ADV_START/CONNECT-class incl. SD
   ENABLE `0x54D4E` + AUTHENTICATE `0x54AAC` + SEC legs), MC 78
   (BLE-range 39/51 — ENABLE `0x3BEBc` + AUTHENTICATE `0x3AF5c` +
   same families). Context dumps (`svc_context.py`) confirm the
   anchors sit in `svc; bx lr` thunk runs. `svc 82`
   (GAP_KEYPRESS_NOTIFY, NOT sd_evt_get — the old "zero `svc 82`
   sites" claims conflated the two numbers) is absent in both, as
   expected for non-pairing images. MPY's BLE is compiled but
   `MICROBIT_DAL_BLUETOOTH_ENABLED: 0` (`mc/built/codal.json`)
   gates runtime init — static hits prove compiled-in, the
   pairing-fw image IS the runtime proof.
4. Trap log: DF-byte polarity (imm@even/DF@odd — verify with
   `svc; bx lr` (`60 df 70 47`) context before believing any scan);
   type-02+04 both required; `svc 82` means GAP_KEYPRESS_NOTIFY in
   the BLE face (sd_evt_get is a different `82` in the SoC series —
   same number, different SVC family, never claimed here).

NEXT: B-workstream flake harness fix; final gate + commit.

## 87. P105 parallel-flake harness fix, part 1: deterministic MPU-gate order repro closed (2026-09-16, uncommitted)

Two mechanisms separated by evidence (the handover's single "MWU_ARMED
leak" theory was WRONG for the deterministic repro — single-thread
fails too, so no thread theory can explain it):

1. DETERMINISTIC order-dependence (FIXED): MPU ENABLE + programmed
   regions live in the INSTALLED model and outlive the test. Next test
   in the same process inherits a live gate into a foreign/fresh map.
   `$BIN --test-threads=1 unaligned_device region_watch` failed 5/5
   pre-fix (MWU test's watched write faulted through the stale MPU
   gate, never reached `mwu_note`, WA stayed 0). Also
   `ldrt_probes_as_unprivileged` + pregion (phase-C R3 FULL window vs
   stale phase-A R1). Fix: exit disarm (`mem.write32(0xE000ED94, 0)`
   at the end of both MPU-programming cpu tests — model + latch
   together). Entry clears (boot()/Cpu::new) run BEFORE the test
   programs the model, so only an exit close works. Filtered
   `$BIN mpu mwu unalign` now 10/10 single AND 10/10 parallel.
   Tried and REVERTED (all made it worse or broke isolation):
   `reset_globals()` inside `boot()` (wipes EXT_DEVICES taps mid-test
   → dma_nrf TWIM NACK; also wipes UART mid-assert), `MWU_ARMED` clear
   in `reset_globals()`/`Cpu::new` (wrong gate — MPU, not MWU, is the
   order repro), `try_borrow_mut` in `mpu_check`/`mpu_is_device`/
   `mwu_note` (masks real contention, full-suite parallel got WORSE:
   stale reads + lost MWU notes).
2. NONDETERMINISTIC cross-thread borrow (STILL OPEN, rarer):
   `SYS AtomicPtr` + `Rc<RefCell>` peripherals shared across OS threads
   with no join — backtraces prove foreign re-entry: `mod.rs:471/495`
   read/write (owner holds the MWU/MPU slot, foreign thread's SYS
   re-enters through mem.watch), `mwu_nrf.rs:237` (same shape),
   QSPI `QSPI_FLASH OnceLock` registry shared across threads. Full
   suite parallel ~13/15 post-fix (was ~3/4); single-threaded
   `-- --test-threads=1` stays 204/204 and is the gate.
   Fix direction for (b): join the same boot lock or run CI
   single-threaded; not done here.

NEXT: B4 docs (STATUS §3 already updated this run) + commit alone;
then C-frontiers; final gate + push only when green.

## 88. P106 MakeCode wait-queue-OBJECT walk: waiter takes the pump path, LEFT-4 parked (2026-09-16, native probe, reverted)

C1 asked for the wait-queue-OBJECT walk at `0x20003b18` (name the
awaited event, trap THAT raise site). Done — answer: there is NO
awaited event; the waiter is a NULL-or-flag-gated pump entry and it
takes the pump path. LEFT-4 parked, LEFT-3 next.

Static leg (fresh `/tmp/opencode/mc_full.dis` from `hex2bin.py` +
xpack objdump; verifies the P97 decode on this tree's binary):
- `0x2e410` waiter: `cmp r0,#0; beq ret-1000` else flag
  `[0x20004373]` (`ldr.w r8,[pc,#144]` → literal `0x2e4ac` =
  `0x20004373`) `lsls+bpl` gate → run queue `[0x20003b08]`
  (literal `0x2e4a0`) → `bl 0x2e01c` pump. One static `bl` site
  `@0x31f62` (acquire loop `0x31f00`: match id/value halfwords →
  `bl 0x2df74` → `bl 0x31c50` callback → `ldr r4,[r4,#36]` next →
  `bl 0x2e410` → loop). `0x31c50` = fiber-callback dispatcher
  (`ldrh [r0,#4]` flag bits, `blx r4` listener-invoke,
  `bl 0x3485c` cleanup). `0x2e01c` pump: marshal
  (`bl 0x37ba0/0x37b74/0x37b88`) → compare → `bl 0x2dc08` queue
  insert (QNEXT `strd [r4,#24]`, wait queue `0x20003b18` literal
  `0x2e080`); `0x2e084` = flag-gated forward to the same pump.
  `0x37ec6` = register-context SAVE (`str r0,[r0,#0]` ...
  `str lr,[r0,#56]` — TCB store, 15 words).

Dynamic leg (TEMP `tmp_mc_c1.rs` + `mod.rs` hook, P16 recipe:
direct-app boot, MBR params, UICR seeds, 2 reset honors,
sleep-aware pump; DELETED after, 204 green hold):
- Park much earlier than P69's 300M: transition at ~160–164M
  (RAM-delay `0x20002078/7a`, queues zero) → WFE-idle `0x37afa`
  (run=`0x20006208`, wait EMPTY, sleep=`0x20006198`). Run fiber TCB
  LR `0x2e453` (inside waiter), sleep fiber `ctx=0x00010007`
  key-matched + `0x2d58/0x2ee` event waiter — matches P92 exactly.
- Single-step trip (phase-1 coarse to 150M, then `run(1)` steps;
  ~14.4M steps to hit): FIRST-HIT `0x2e410` with `r0=0x31c51`
  (return addr — acquire-loop waiter family `0x31f00`, one insn
  past the `bl 0x31c50` at `0x31f4e`), `r1=r4=0x200062fc`
  (waiter struct), `r2=0x23a34e` (FLASH addr — NOT a RAM object,
  so not a fiber/queue/event struct; never dereferenced on the
  taken path), `r3=0x20004373` (flag byte, `0x03` at hit),
  `r5=0x20006344`, `lr=0x31f67` (return past the `bl 0x2e410`).
- Follow-through (40 single steps): flag `lsls r4,#31` sets N
  (bit31=1: fiber-wait dispatched state) → `bpl` NOT taken → pump
  path (not the ret-1000 NULL path) → run-queue load
  (`r3=0x20006208`) → `cbz r4 → 0x2e44a` (queue object NULL —
  the `bl 0x2e01c` pump call SKIPPED) → `ldr r0,[r3]` (own TCB
  `0x20006234`) → `bl 0x37ec6` context SAVE → leaves waiter
  region. So the waiter saves the parked main fiber's context and
  returns — exactly P93's listener-invoke shape, one level down.
- Waiter inputs: `0x200062fc` = fiber-create-arg shape
  (`0x3fd/0x12` id/value pair, fiber `0x20006208`, `0x23a34e`
  passthrough); `0x20006344` = listener-fn table (code addrs
  `0x2b8d1/0x2500`, back-ptr `0x200063a8`, `0x23a34e` again).
  Neither is an event id/value wait — the waiter never blocks ON
  anything; it is entered from the acquire loop with the flag
  already set, checks the flag, finds no queue object, saves
  context, returns. The `0x2e084` raise-forward was never hit in
  14.4M steps (nothing left to raise).
- Trap-THAT-raise-site is therefore moot — C1's second half has no
  target. The "awaited event" does not exist: main parks in the
  pump waiter with the wait queue EMPTY, the scroll fiber never
  created, the display path never touched (TIMER4 COUNTER 0).

Verdict: LEFT-4 PARKED (pre-scroll sequencing fully characterized:
acquire loop → `0x31c50` callback dispatch → `0x2e410` pump-entry
waiter → context save → WFE-idle; scroll never constructed).
Probes reverted (`rm tmp_mc_c1.rs`, hook out); suite file-free.
NEXT: LEFT-3 bootloader pass-2 trace assessment (SD priorities
shelved?) — C2.

## 89. P107 bootloader pass-2 assessment: park LEFT-3, direct-app stays the recipe (2026-09-16, no probe)

C2 asked for a pass-2 trace assessment (SD priorities shelved?) or
park LEFT-3. Assessment from the settled record (STATUS §6.3 + plan
P25/P29/P49/P52/P68, no new probe — a trace was never going to pass
the gate below, so none was run):

- Settled: BL entry `0x772F9`, FICR gather, benign post-UICR reset,
  2nd CODED AIRCR `0x78514` via tbb `0x78498` BY DESIGN (r4==0 =
  SD-enable SUCCESS through `0x7B530` `svc 16` + `0x7B5B4` IPR22
  check + `0x7B568`); `0x7B5B4` needs nonzero IPR22 (SD-set
  priorities — silicon state, out of scope); `0x784C4` = DFU-progress
  gate (not the cause); MBR selector `0x417` never reads
  `0x10001200/204` (P42 `0x0–0xB00` sweep refuted); seeded native MBR
  run parks `0x77332`; P68 pass-2 (seeded UICR + IPR22
  `0x40404040`, MBR entry): 1 reset then parks at app vector
  `0x29C7A` with ZERO hits on `0x7B5B4`/`0x772F9`/`0x783FE` — pass 2
  never reaches BL validation (MBR jumps straight to the app table).
- A pass-2 trace would need: (a) SD-set NVIC priorities synthesized
  into the model (IPR22 nonzero — today IPR22 reads 0 by reset and
  nothing in the image sets it pre-validation; synthesizing
  SD-written priorities = inventing silicon state, AGENTS.md-scope
  violation by spirit), AND (b) a reason to believe pass 2 reaches BL
  at all (P68 says it does not — MBR→app-direct bypasses validation,
  so the trace would re-prove the bypass, not the chain). Cost:
  a full MBR-entry native probe run for an informational re-proof.
- Decision: PARK LEFT-3. The chain is characterized to the
  silicon-state boundary; direct-app boot (P16 recipe) stays the
  recipe; no model change expected or attempted. Reopen only with a
  faulting config (e.g. an image whose MBR demonstrably enters BL
  validation and fails on emulated state — none exists here).

C-workstream closed: LEFT-4 (P106) + LEFT-3 (this note) both parked
with evidence. No code touched in either (reverted probes only;
suite file-free throughout).

## 90. P108 parallel-flake harness fix, part 2: sd_ble BOOT_LOCK join closes the SYS-swap race (2026-09-16)

P105 closed the deterministic order-dependence (MPU exit disarm;
filtered 10/10 both modes) but the full suite still flaked ~2/15 in
parallel with `RefCell already borrowed` at `mwu_nrf.rs:237` /
`mod.rs:457` (+ rarer `mod.rs:131` + `mod.rs:495` shapes and one
unrelated-looking `usbdev` UART-marker miss — all the same family).

Root cause (proven, NOT the thread-local theory): the INSTALLED
process-global `SYS AtomicPtr` (`lib.rs:set_sys`, swapped by every
`boot()`/`init_for_test` with no join) vs sd_ble unit tests that
never held `BOOT_LOCK`. A parallel cpu/mwu test swaps INSTALLED SYS
mid-test while an sd_ble test's `FlatMemory::write8 → watch →
mwu_note` holds the MWU slot borrowed → `borrow_mut` panics on the
aliased slot. Evidence: backtraces through `mem.rs:watch:265` →
`mwu_note:237` from sd_ble test frames (`gap_connect...:2954`,
`region_watch...:267`); filtered `sd_ble mwu` parallel failed ~18/20
pre-fix (single-threaded always green). The `SD_BLE_STATE` /
`TAKE_DATA` thread-locals were NEVER the fault (per-thread, safe);
likewise QSPI `QSPI_FLASH OnceLock` is a `Mutex<HashMap>` (safe).
The panic sites wander (MWU slot vs MPU slot vs NVIC) only because
whichever slot the victim holds is the one the swap re-enters.

Fix (requested direction, minimal): `BOOT_LOCK` join in all 10
sd_ble tests (`let _g = lock_boot();` first line, same discipline as
cpu/tests + mwu tests) + doc comment on the helper. No model change,
no new locks (deadlock audit: BOOT_LOCK is leaf-only — holders never
block on UART/I2C locks; `try_lock_uart` in `boot()` stays
non-blocking), no thread-locals, no `src/cpu/` edits.

Acceptance: filtered `sd_ble mwu` parallel 20/20 green (was ~2/20);
full suite parallel 25/25 green (was ~13/15); single-threaded
204/204 unchanged. One commit for the harness fix alone.

## 91. P109 live crypto+QSPI pumps in pumpDma, shared crypto.js (2026-09-17, committed 539cf19)

Demo-only gap closed: the ECB/AAR/CCM/QSPI models were F-grade with
take/complete exports and native/harness proofs, but the bench pumpDma
never serviced them — staged firmware jobs sat unstaged in-browser.
P109 wires them into `demo/index.html:pumpDma()` sharing one
implementation with the depth probes via NEW `demo/parts/crypto.js`
(FIPS-197 `aesBlock`, `ctrCrypt`, `cbcMic` — moved verbatim out of
`mocks.js`'s MockEcb/MockCcm local copies):
- ECB: take job → driver AES-128 encrypts KEY@+0/CLEAR@+16 in place → complete.
- AAR: take job → resolve-present → complete (no IRK table on the bench, by design).
- CCM: take job → CTR+MIC-4 encrypt (or decrypt+verify per the CNF contract) → complete.
- QSPI: 64 KB bench image (`qspi_register_flash` on every `initBoard`;
  `reset_state` clears the registry), AND-only program + `0xFF` erase,
  take read/write/erase → move bytes guest-RAM↔image → complete.
Idle cost is one take per engine per frame (pumps fire only on staged
jobs). No Rust changes, no new exports, no model changes; verified via
the standard matrix (E2E 42/42, browser 16/16 in the P110 run).

## 92. P110 UARTE1 + SPIM2/3 firmware proofs + RTC/PWM/RNG/TEMP/EGU depth (2026-09-18, committed 33d7892)

Two doc-claimed gaps closed plus the "thinnest" row deepened, +7 tests
(204→211 green single-threaded, 25/25 parallel):
- Batch 2 firmware proofs (NEW `blinky/uarte1_nrf.s/.bin` 214 B,
  `blinky/spim23_nrf.s/.bin` 222 B; asm recipe `as` + `ld -T
  blinky/link_nrf.ld` + `objcopy -O binary`, both rebuilt bit-identical):
  `nrf_uarte1_instance_dma_roundtrip` (UARTE1 `0x40028000` IRQ 40: TX DMA
  `U1DATA` + 3 B RX DMA through the shared take/complete path,
  `U1TX:OK`/`U1RX:OK`) and `nrf_spim23_dma_roundtrip` (SPIM2 `0x40023000`
  4 B TX + SPIM3 `0x4002F000` 4 B RX, `S2TX:OK`/`S3RX:OK`).
- Model fix (required by the SPIM2/3 proof): `twim_nrf.rs::arm_nack`
  SPI guard — SPI has no address phase and never NACKs; without it the
  staged SPIM2/3 DMA cleared ~6000 instr before the driver take ran.
- Batch 3 depth (one second proof each, direct-model + NVIC ISER +
  clear-path convention): RTC COMPARE match + OVRFLW wrap (incl. OVRFLW
  IRQ bit-1 model fix — SVD `lsb`/`msb` ground truth, was event-only),
  PWM STOP→STOPPED + INTEN/ISER gating + SEQSTART1 on PWM1 (ISER1
  `0xE000E104` pattern for IRQ >31), RNG SHORTS VALRDY→STOP + VALUE
  re-arm, TEMP DATARDY INTEN gating + STOP clear, EGU per-channel
  independence + INTEN mask + INTENCLR.
- P108 redo CLOSED without code (evidence): 25/25 parallel green with
  all additions; the one `mwu_nrf.rs:237` panic in a 6-run loop never
  reproduced in 25+8 runs (pre-existing rare rate). The stashed
  try_borrow/NACK-clock experiments were dropped; only the SPI guard
  kept. Lesson recorded in agent.md: never bulk-regex
  `sys.p.nvic.borrow` (broke `scb.rs:166` braceless `if`s).
- Full matrix on the P110 tree: cargo 211 single green, 25/25 parallel,
  handshake 18/18, smoke + mpy face OK, E2E 42/42 over air, browser
  16/16 zero page errors, pkg rebuilt (committed `demo/pkg`).
- Docs synced in P111 (follow-up commit): STATUS/COVERAGE/doc.html/
  about.html counts 204→211, rows for UARTE1/SPIM2-3/crypto-QSPI/depth,
  LEFT#5 + new LEFT#7–9, verify blocks, +17 proofs in about.html.

## 93. P112 firmware/lab session: REPL/TX-drop/BL/MC/wall-time (2026-09-18, Node probes, no commits)

All five LEFT swim-lanes got live-fire evidence on the current tree
(P111 `dd39aa8`, pkg as committed). Probes live in `/tmp/opencode/`
(`replA–M`, `txA–H`, `blA–B`, `mcA–C`, `wall_mips/mpy*` — ephemeral,
never committed); images `/tmp/opencode/mpy.bin` + `mc.bin` via
`blinky/hex2bin.py` (UICR extra `0x10001014: 00077000 0007e000` —
note plan.md:71 misprinted this as `00070700 0007e000`; the HEX record
`:081014000070070000E0070076` decodes LE to `0x00077000/0x0007E000`).
P16 recipe throughout: direct-app boot at `0x1C000`, MBR params
`*(0x20000000)=0x1000` / `*(0x20000004)=0x1C000`, UICR seeds,
`deliver_irqs=true`, per-frame pump (UARTE-TX take/read/complete,
NVMC-erase take/complete, TWIM1 take/drain, taps registered).

REPL exec path (LEFT-1), BIGGEST FINDING: the banner is NOT blocked
by the documented post-banner NULL fault — it never gets that far.
With P16-correct reset-honor (app table, both resets) boot faults at
`0x1AEF8`/`op=0x4770` at ~309–360k instr, ipsr=11 (SVC handler),
uart=0. Regs: r0=0, r1=`0x2001FF20`, r2=`0xFFFFFFFF`,
r3=`0x00070701`, sp=`0x2001FF20`, lr=`0xFFFFFFE9` (FP-extended return
to handler!). Stacked frame: ret=`0x00054D96`, stacked
r0=`0x20004378` r1=`0x20003984` r3=`0x00070701`, xpsr=`0x01000000`,
CFSR/HFSR/BFAR/MMFAR all 0, SHCSR bit 7 (SVCALLPENDED) only.
Trace tail: `…1a5a2→1a5ca→…→1a5d8→550fa→2c5b6→2c5b8→2c5ba→54d90→
54d94→aa4→aa8→aaa→aae→ab2→ab4→ab6→ab8→aba→ac4→ac6→ac8→aca→acc→
1aeac→1aeae→1aeb0→1aeb2→1aeba→1aebe→1aec0→1aec2→1aec4→1aec6→
1aef0→1aef2→1aef4→1aef6→1aef8`. SVC census over the whole
300k→fault window: EXACTLY TWO SVC executions — `0x550F8:svc 0x13`
(SD init) and `0x54D94:svc 0x00` (MSR MSP switch). No `svc 0x3C`
anywhere: the `df3c` halfword at `0x54D82` is a DEAD literal pool
word (never fetched). The `0x1AExx` block is a post-SVC dispatch
table (`bx r2` at `0x1AEAC` fans to `bl`-chains selected by the SVC
return values); `0x1AEF8` = plain `bx r2` whose target register came
back 0. So the fault is a NULL-dispatch THROUGH the SD-return path,
not an SVC-number problem — the suspect moves UPSTREAM to what
`svc 0x13` (or the MSR switch at `0x54D94`) returned. Variant matrix:
no-taps → identical fault (taps innocent); skip reset#1 → parks
`0x29CD1` (P30 UICR HALT, no fault — reset#1 load-bearing);
MBR-honor → HardFault `0x29C7B`/ipsr=3, CFSR `0x8200` (INVPC+BFSR),
stacked r3=`0x417`/ret=`0x440` (MBR selector context — arguably
CORRECT MBR behavior: SVC 3 unimplemented in MBR); skip-first →
same `0x29CD1` park. NEXT: dump stacked r0–r3 + r12 at the app-fault
frame to identify WHICH dispatch slot returned 0 (r2 provenance:
`svc 0x13` r0 vs MSR-switch corruption), then match the `0x1AExx`
slot table against the SD return contract.

TX-drop slot audit (LEFT-2), MODEL PROVEN: guarded vs unguarded
A/B on the live pkg. Unguarded (STARTTX via `periph_write`, no
guard): 60/60 N+1 leaks — the late `mem_read` transmits N+1, drops
N, exactly the P49 holes. Guarded (STARTTX store executed INSIDE
`cpu.step`, guard live — guest flash stub doing the store):
0/60 leaks, `uart="A…"` correct. (Method trap: `mem_write` to
`0x300` silently went to an `extra` region — `load()` only maps
flash/RAM; the guest must live in a `load_firmware`'d flash image;
`reset_cpu` also needs a valid SP. Fixed by building the stub into
the flash image.) So the snapshot path is byte-correct end to end;
remaining drops (if any on banner-length runs) are pre-STARTTX
firmware-side (P49 `&c` reuse / P53i) — no model change indicated.

Bootloader SD-priority chain (LEFT-3), P68 RE-PROVEN on this tree:
MBR entry, seeded UICR 18/18 + IPR22 `0x40` pattern: 1 reset, then
parks `0x29C7B` with ZERO hits on `0x7B5B4`/`0x772F9`/`0x783FE`
over 4M instr (trace-sampled every chunk). MBR→app-direct bypasses
validation — IPR22 seeding changes nothing because validation never
runs. Static bytes confirmed: `0x7B5B4: f891 3316 095a 23ec…`
(IPR22 `ldrb [r1+#0x316]` shape), MBR `*(0xA9C)=0x0417` selector
pointer intact. Unseeded MBR entry: 2 resets (both honored), same
park. Stays PARKED (inventing SD priorities = silicon-state
invention; no faulting config exists).

MakeCode scroll (LEFT-4), SHARED GATE WITH MPY: app-honor MPY-style
boot faults `0x1AEF8` at 80k (same SD-return NULL dispatch —
MakeCode ships the same S140 SD region). MBR-honor boot parks
`0x37F4F` (ipsr=3, SVC handler; stacked r3=`0x417`/ret=`0x440`,
CFSR `0x8200` — the SAME MBR-selector SVC3 context as MPY's
MBR-honor park at `0x29C7B`). 4M-instr sleep-aware run: pc glued
`0x37F4F`, TIMER4 COUNTER ever 0, DIR0 ever 0, TX stages 0, uart 0.
Pre-scroll stall CONFIRMED on current tree; and both firmwares now
show one shared early gate (SD-return NULL dispatch under
app-honor; MBR-selector SVC3 HardFault under MBR-honor). NEXT for
both: the stacked-frame r2-provenance read above.

Wall-time (LEFT-6), MEASURED on this host: Node WASM blinky
(`wall_mips.mjs`, 5×20K pumpDma frames): **~41–54 MIPS**
(6M instr / 0.11–0.15 s, `BOOT/BLINK/BLINK` correct). Node MPY pump
(`wall_mpy2.mjs`, fault/reset path exercised): ~24 MIPS
sustained-through-fault. Native debug harness: blinky firmware test
(5M-instr `cpu.run` + model, no pump) 0.32–0.40 s ⇒ **~12–16 MIPS**
per test-process second (includes harness + instruction-count
atomics, NOT pure step rate — do not cite as core speed). Banner
math stands: 160–180M @ ~45 Node MIPS ≈ 4 s of pure stepping (plus
pump/bridge overhead); browser ~6 MIPS ⇒ ~30 s. No action.

## 94. P113 SVC13-arg audit + TRUE-UICR rerun + store-pc capture (2026-09-18, Node probes, no commits)

Follows P112. Probes `svcA–G`, `trueA–C`, `uicrA–E`, `clobA–B` in
`/tmp/opencode/` (ephemeral); tree P111 `dd39aa8`, 211 green.

SVC hook + raise/take path (audit, no bug): `thumb.rs:1443`
(`0xDF00` arm) claims `0x60..=0xBF` via `sd_ble::handle_svc` FIRST
(r0 write + skip on `Some`), else `adv + raise_sync(-5)`. Our
`handle_svc` returns `None` outside the range (untouched fallthrough)
and `Some(NRF_SUCCESS)` + `enabled=true` + RAM-floor write for
`sd_ble_enable` (`sd_ble.rs:1087`, both pointers may be NULL).
Delivery-off would loud-fault; delivery is ON in all probes.

svc13 @`0x550F8` arg capture: the thunk at `0x550F6`
(`b.w 0x2D820`) is entered via `r0=0x20002520` (params),
`r1=0x20003984`, `r2=0`, `r3=0x20003940` — i.e. the SD-init call
`bl 0x550FC` chain: `ldr r0,[0xFF8]` → `adds r2,r0,#1` → `bne 0x55110`
→ `b.w 0x550F8` (svc13). Post-SVC `r0(ret)=0x00070700` — NOT 0:
`sd_ble_enable` wrote the RAM floor (`0x20002000`-family base) and
returned it, firmware treats nonzero as failure-ish downstream
(`adds r3,r0,#1; beq` @`0x5510C` → `movs r0,#4; bx lr` — error path
returns 4). `ble_enabled` stays false in the emulator-side singleton
ONLY because each probe creates a fresh process/board — live within
a run it flips true (svcC polling artifact, not a model miss).
Canary `0xCAFEBABE` @`0x20000058` NEVER appears in ANY variant
(P22's BL-path canary comes from the BOOTLOADER's SD init, not the
app's — direct-app boot has no BL, so no canary by construction).

MBR/SD/app dispatch chain (decoded, objdump-verified):
`0xAA4` MBR dispatcher (`cmp #24` → MBR `0x377` else SD) →
`0xB064`-family SD region → app SVC handler `0x29AF0`-family →
`0x1AEAC` app SD-dispatch shim (reads stacked SVC number from
`[sp+#24]-2`, classifies 16/32/44/96) → `0x1A587` per-SVC worker
(`cmp r0,#16/17/18/19`, `bl 0x1A40A/0x1A56C`-family validators,
writes `[0x1A614]`/`[0x1A618]` dispatch struct) → `0x1AEEx`
slot table (`bx r2` fans by SVC class; slot `0x1AEF0` = SD-state
slots reading `[[0x20000004]+0x2C]`).

r2 slot-table input (ROOT-CAUSED, then demoted): the slot does
`ldr r2,[0x1AF10]` → `ldr r2,[r2]` → `adds r2,#0x2C` →
`ldr r2,[r2]` → `bx r2`. Statically `[0x1AF10]=0x20000004`, so the
chain reads OUR hand-installed MBR param `0x1C000`, +`0x2C` =
`0x1C02C` = app SVC-vector word `0x29C83`-family → dispatch.
With WRONG UICR (`0x70700/0xE00700`, byte-swapped typo from
plan.md:71) the app's SD validator (`bl 0x1A56C` →
`[0x1A5FC]=0x20000058` vs `[0x1A608]=0xCAFEBABE` canary check)
fails → writes `0x70700` over `[0x20000004]` (store pc `0x1A5CF`,
captured by 1-step watch) → slot derefs `[[0x70700]+0x2C]` =
`0xFFFFFFFF` → `bx r2` NULL fault `0x1AEF8`. With TRUE UICR
(`0x77000/0x7E000` from the HEX record) the word is never clobbered
(400k watch: zero changes), no fault — the `0x1AEF8` fault was a
HARNESS SEED TYPO, not a model bug. But TRUE seeds only trade the
fault for the DOCUMENTED park: `0x200021B8/BB` RAM delay-fn
(`01 38 fd d1 70 47`), 200M/zero-fault/uart-0/tx-0, TWIM
healthy (txT 117/rxT 1/ev 378, addrs `0x19` WHO_AM_I +
`0x39` flash-echo, zero NACK errors) — i.e. the pre-P86 park,
now reachable again. P112's "new early fault" verdict is SUPERSEDED:
with correct seeds there is no early fault on this tree at all.

Serial-object state at 60M TRUE-seed park (offsets are BYTE offsets
into the word-dumped struct — P18's `+NN` are 32-bit WORD indices ×4
= byte offsets; the earlier `+38=0` reading was a mis-indexed word):
`id@+12(w)=1073741836` (low halfword = 12 = DEVICE_ID_SERIAL ✓),
`status@+14(w)` low half = `0x4000` (bit 14 = TX BUFF_INIT; bit 12
RX NOT set — RX buffer never initialized!), `rxSize@+36(w)=21`,
`rxBuff@+32=NULL`, `txBuff@+44=NULL`, `txSize@+48=21`,
`baud@+56=0`, `is_tx@+60=0`, `bytesProc@+64=0`, `dmaPtr@+100` =
`0x40002000`, UARTE EN=8 (enabled!) but BAUD=`0x01D60000`
(never configured), TXPTR/TXMAX/ENDTX=0, RXPTR/RXMAX/RXAMT/ENDRX/
RXDRDY=0. So: serial object EXISTS with TX init only, both ring
buffers NULL, baud unset, DMA never armed — init stalled between
TX-setup and RX-setup, consistent with an SD/pin/allocation gate,
NOT with a UARTE model gap (EN path + TX snapshot + RX drip all
proven independently). NEXT: what the init sequence waits on between
TX-init and RX-init (TWIM sensor STATUS? SD event? pin line?) —
trace `0x282E5`-family callers (the only non-RAM pc in the 200M
sample set) + DRDY-line experiment (hold P0.25 low vs pulse).

## 95. P114 out-of-scope blitz: code a way through every wall (2026-09-18, uncommitted work)

User verdict: no "out of scope" walls — code through each. Audit result:
most §7 items HAVE a code path that respects AGENTS.md (no `src/cpu/`
edits for board issues, one clock, Nordic TASKS/EVENTS/INTENSET style).
Implemented + tested, docs synced, NOT committed (user: no commit):

1. NFC antenna (was: pins-as-GPIO only). UICR.NFCPINS PROTECT bit 0
   (`0x1000120C`, SVD ground truth, reset `0xFFFFFFFF` = NFC):
   `nfct_nrf::nfct_pins_reserved()` reads the UICR slot live;
   `set_field` refuses field events in GPIO mode; `gpio_nrf` refuses
   PIN_CNF writes + `gpio_set_input` levels on P0.09/P0.10 while
   reserved (threaded through lib.rs to avoid re-borrow). Test
   `nfcpins_gate_routes_pins_vs_antenna` (reset=antenna, field OK;
   PROTECT=0 → no field + GPIO DIR works; back to 1 → DIR sticky).
2. BLE bond store (was: keys stubbed, bonds die). `Bond` struct
   (peer + LTK/IRK/CSRK/master-id) in SdBle, survives disconnects;
   SEC_PARAMS_REPLY accept copies a 58B firmware keyset block;
   `bond_has_keys`/`bond_read_keys`/`store_bond`/`delete_bond` +
   4 wasm exports; bridge `bond_keys` leg persists air keys on
   pairing confirm. Test: store→hit/miss/delete cycle in the pairing
   lifecycle test.
3. BLE TX-flow (was: static budget, no events). `complete_tx_flow`
   refills one token (cap 4) + posts TX_COMPLETE `{conn,count}`
   (model-local id `0x3A`, documented); NO_TX_PACKETS still gates.
   Test spends 4 notifies → refuse → refill → TX_COMPLETE drained
   (count 1) → stages again. Pump legs: bridge/air `tx_complete`
   → `ble_complete_tx_flow`.
4. Peripheral role (was: central-only). `complete_peripheral_connect`
   posts CONNECTED with PERIPH role byte (`GAP_ROLE_PERIPH=1`);
   pump legs: air `periph_connected` + export. Param update:
   request SVC validates, `complete_conn_param_update` posts
   CONN_PARAM_UPDATE `{conn,params}` (pump leg included). Test covers
   all three (role byte, wire sizes, unknown-link no-op).
5. Edge-SPI display (was: no consumer). NEW `demo/parts/spidisplay.js`
   ST7789 240×240 (CASET/RASET/RAMWR + RGB565 + SWRESET/DISPON,
   MISO ID `04 85 52`) + bench panel/canvas/poll wiring on SPIM2.
   Verified headless (`spidisplay_check.mjs`: pixels + drain +
   clear + DISPON, all OK vs built pkg).
6. I2S WebAudio (was: capture drained). Bench `audioPush`: 16-bit
   mono @16 kHz AudioContext, gesture-gated Enable toggle,
   silence-skipped (idle = one drained FIFO, zero audio). Needs only
   a browser to hear; headless matrix unaffected.
7. Lazy FPU stacking (was: "documented deviation", STALE — code
   already implements it: `cpu/thumb.rs` FPU hook + `cpu/mod.rs`
   take/return reserve/complete/pop, `fpu_lazy_*` + `fpu_eager_*`
   green). Fixed the stale comment in `fpu.rs` + STATUS/COVERAGE/
   doc.html rows. No code change needed.
8. sd_evt phase-1 (was: draft, dead code). NEW `src/sd_evt.rs`:
   model-side SoC queue (SVC 16 arms, SVC 82 answers `{id u16,len u16}`
   else falls through, NVMC `complete_erase` posts id 2/3, reset in
   `reset_globals`, `sd_evt_queue_len` export). Hook: 2 compares in
   the existing thumb SVC arm (no cpu/ edits beyond the hook site).
   Firmware proof `blinky/sd_evt_nrf.s/.bin` (svc16 → ERASEPAGE →
   mailbox spin → svc82 id=2/len=0 → EMPTY:OK) + native test
   (`nrf_sd_evt_flash_success_roundtrip`) + 3 unit tests. Suite now
   217 (was 211: +1 firmware, +1 NFC, +1 BLE flow, +3 sd_evt).
   DB proof vs live pkg (`sdevtO.mjs` pattern): queue=1 → poll →
   buf=2 len=0. Method trap recorded: hand-written `ldr r0,[pc,#N]`
   must use GAS-correct imm (base=(pc+4)&~3); `mov r0,sp` reads the
   thread SP (MSP only in handler — use `mrs r0,MSP`).
9. npm (was: 401-blocked). `npm pack --dry-run` in demo/: 22 files,
   72.5 kB tarball, integrity hash — package SHAPE verified without
   credentials. ACL/SPU: reads-0 stays (protection enforcement is a
   separate project with no consumer — the one wall with no code
   path worth building; documented as the single remaining item).

NEXT: run the FULL verify matrix (cargo single + parallel, handshake,
smoke, E2E, browser 16/16, pkg rebuild — pkg already rebuilt for
sd_evt/NFC/BLE exports), then commit per AGENTS.md (one peripheral
per commit would split this 9-ways; user decides) + push.

## 96. P115 stale-doc cleanup + LEFT-1 caller trace + DRDY experiment + MC parked check (2026-09-18, Node probes, no commits)

Tree `be02b89` (P114 committed), pkg as committed, 217 green.
Probes in `/tmp/opencode/probe/` (`p1`–`p14`, ephemeral); images
`/tmp/opencode/mpy.bin` + `mc.bin` via `blinky/hex2bin.py`; P16
recipe throughout (direct-app `0x1C000`, MBR params, TRUE UICR
`0x77000/0x7E000`, `deliver_irqs=true`, sleep-aware pump).

Stale-doc cleanup (committed state, found non-blocking): STATUS:456
said sd_evt "unimplemented" (rewrote as phase-1-closed); LEFT-5 said
"no edge-SPI part" + :409 "keys stubbed" (rewrote: ST7789 wired,
bond store persists); STATUS:474 + COVERAGE:265 "25/25" (rewrote as
~30/31, see agent.md §0); §7 duplicate Edge-SPI/NFC/WebAudio rows
(removed, kept the closed-P114 rows); about.html "no key storage /
no NFC models / crypto stubbed" (rewrote: bond store, NFCPINS gate,
driver-side crypto); COVERAGE §8 intro + LEFT-5 row (bond-store /
ST7789 closed).

LEFT-1 caller trace ( park reproduced first): TRUE seeds, 60M:
park `0x200021B9`, 2 resets (RESET#1 @step 0 pc `0x29CD1` =
pre-first-instruction sample, RESET#2 @20K pc `0x29CCF` = AIRCR
SYSRESETREQ self-request; both honored to app table), zero faults,
uart 0. Flash-pc histogram (2K samples, ~1900 flash hits):
`0x57039` x153 (memcpy-shaped byte loop — init copy, not the
waiter), `0x26039` x57 (u64-compare helper return), then the
`0x2829x`/`0x2837x` cluster (`0x282E7/0x282B7/0x282C5`…) +
`0x5048D` x43 (the `bl 0x5048C→b.w 0x26020` delay). Static scan:
exactly 10 `bl 0x5048C` sites (6 in the `0x28290` waiter family +
4 in `0x52Fx`). `0x28290` has ZERO static `bl` sites (virtual/
register-called, as documented). r9 literal `0xF4240` = 1,000,000
(GAS-correct base `(pc+4)&~3` — the P114 method-trap lesson
re-verified). `0x26020` region: time-compare helper
(`[0x26068]=0x20003EA8` live object).
Regs at `0x282B7` (3 traps, 5M+ apart — STABLE, not transient):
r0=`0x40004000` (UARTE0!), r1=`0x148`, r4=`0x20002C0C`,
r5=1, r6 growing (`0x17E0→0x38F5→0x5A01`, bounded by r9=1M),
lr=`0x26039`. `[r0+0x150]=1`, `[r0+0x200]=0x100`,
`[r0+0x124/0x148/0x160/0x1C]=0`. Read: the waiter polls
`[UARTE0+0x150]` for a nonzero that never comes (only
`[+0x150]=1` set — the init pattern, never the completion
pattern), i.e. the init sequence waits on a UARTE0-side event
between TX-setup and RX-setup. Consistent with the P113 serial
object (TX BUFF_INIT only, baud 0, DMA never armed, both rings
NULL): the gate is UPSTREAM of UARTE config, not in it.
NEXT: identify `[0x40004000+0x150]` in the UARTE register map
(SVD ground truth — NOT guessed) + who sets/clears it; the
`0x28290`-family caller (register-called — needs a `blx`-site
scan, not a `bl` scan).

DRDY experiment (P0.25): LOW-held vs HIGH-held whole-run (60M,
TRUE seeds, same pump): IDENTICAL — park `0x200021B9`, 2 resets,
same top-8 flash pcs in the same order, uart 0. DRDY level does
NOT gate the park (the sensor path is innocent here). Deeper
support: TWIM1 audit over 60M shows exactly ONE transfer —
`TX addr 0x19 len 1 [0x0F]` (WHO_AM_I, answered) + its event,
zero RX takes, zero NACK storms. Sensor init is one clean probe,
not a spin. NEXT for LEFT-1 is the UARTE0+0x150 register
question above, not pins/sensors.

LEFT-3/4 parked check (current tree+pkg, 4M sleep-aware):
MakeCode parks `0x2000207B`, ipsr 0, zero faults, 2 resets,
uart 0, TIMER4 CC0=0, DIR0=`0x01688000` (rows output-driven —
P35 CNF→DIR fix holding), OUT0=`0x04018100`. Same pre-scroll
sequencing stall, no new faulting config → stays PARKED per the
reopen rule. Bootloader likewise: no faulting config exists.

## 97. P116 REPL exec CLOSED: pump starvation, banner + `print(1+2)` → `3` (2026-09-18, Node probes, no commits)

Tree `be02b89` (P114 committed), pkg as committed, 217 green.
Probes `/tmp/opencode/probe/p16`–`p20` (ephemeral); P16 recipe
(TRUE UICR, direct-app, sleep-aware pump).

P115 §96 ended with the wrong NEXT (UARTE0+0x150 register
question). SVD ground truth kills it in one lookup: `0x40004000`
is TWIM1/SPIM1/TWI1/SPI1, and +`0x150` = EVENTS_TXSTARTED on the
TWIM map (UARTE0+0x150 would be TXSTARTED too, but r0 pointed at
TWIM1). The waiter (`0x28290` family, zero static `bl` sites —
register-called) polls `[TWIM1+0x150]` for a STARTTX completion
that only the DRIVER delivers. All prior probes took TWIM TX
takes without completing them (`wasm.twim_take_txdma('TWIM1')`
bare, return dropped): LASTTX never set, the SHORTS STARTRX
chain never fired, RX takes never staged (audit: exactly ONE
transfer in 60M — WHO_AM_I `TX addr 0x19 [0x0F]`, zero RX).
The "healthy TWIM, no NACKs" readings were starvation readings.

`p16` (FULL TWIM pump: TX take→`mem_read`→complete, RX take→
`mem_write` WHO_AM_I sample→complete): sensor init completes
(`TX 0x19 [0x23 0x80]`, `TX 0x1E [0x60 0x0B]` …), park escapes
~176M (`pc=0x266E3`), flash execution in `0x539E7/0x2874x/
0x266Dx` regions, txC=132/rxC=1, zero faults. `p19` (fixed
`get_uart_output` TAKE semantics — P18's `includes()` on the
take-return always re-read empty, hiding the burst): BANNER at
237.8M, 78 B (`MicroPython v1.18 on 2023-10-30; micro:bit v2.1.2
with nRF52833\r\nType "help()"`). `p20` (RX drip past banner):
`print(1+2)` → `3` + `>>> ` prompt at ~237.9M
(`...information.\r\n>>> print(1+2)\r\n3\r\n>>> `, 125 B),
stable 60M+ after, zero faults. LEFT-1 CLOSED.

DRDY verdict: P0.25 LOW-held vs HIGH-held (60M, same pump):
byte-identical (park, resets, top-8 flash pcs in order, uart 0).
Sensor level never gated the park.

MC parked check (same tree+pkg, 4M sleep-aware): parks
`0x2000207B`, ipsr 0, zero faults, TIMER4 CC0=0, DIR0 rows
driven — same pre-scroll stall, no faulting config → stays
PARKED. Bootloader likewise PARKED.

NEXT: wire the full-TWIM completion into the bench pump
(`demo/index.html:pumpDma` + `demo/parts/lsm303.js` RX path —
today the bench likely takes without completing exactly like
the old probes) so the banner arrives in-browser; verify with
`tools/browser_verify_16.py` + a banner watch. Method trap for
the record: `get_uart_output()` TAKES (clears); progress
detection must accumulate into a log, never `includes()` the
fresh return.

## 98. P117 bench pump verdict: NO WIRING NEEDED + in-browser REPL proof (2026-09-18, page probes, no code changes)

P116 ended with "wire the full-TWIM completion into the bench pump".
Inspection reverses it: `demo/parts/lsm303.js poll()` ALREADY does
the full round trip every frame (EASYDMA TX take→`mem_read`→complete
with regptr/CTRL-echo bookkeeping; RX take→`sample()`→`mem_write`→
complete; byte-path events drained + `i2c_push_rx` anticipated), and
`demo/index.html:pumpDma()` calls `parts.poll(cpu)` inside the 5×
sub-loop with UARTE-TX + NVMC-erase duties. The starvation lived only
in the ad-hoc native probes (`twim_take_txdma` bare, return dropped).

Repro (`p21`–`p24`, `/tmp/opencode/probe/`, ephemeral) against the
COMMITTED page+pkg (`python3 -m http.server 8080 --directory demo`):
`p21` (bench-pump replica in Node): BANNER at 22.1M, 82 B.
`p22` (+bench RX-drip rule): `print(1+2)` → `3` + `>>> ` (125 B),
zero faults. `p23` (real page, mpy preset + Run): banner in the UART
box at T+15s wall (`...v2.1.2 with nRF52833\nType "help()" for more
information.\n>>> `, 104–105 B), zero page errors. `p24` (real page,
type `print(1+2)` + Send): `...>>> print(1+2)\n3\n>>> ` at +10s,
zero page errors. LEFT-1 closed end to end on the shipped bench;
`browser_verify_16.py` re-ran 16/16 green in the same session (no
regressions). No code changed — docs only (STATUS §6.1, COVERAGE
§3/§5/§6, about.html).

## 99. P118 ACL regions + nRF FPU-engine stub + KL27/sound/touch JS (2026-09-18)

Ground truth first (SVD + CODAL + specs, no guessing):
- SVD: 70 peripherals, ACL @0x4001E000 (8-region cluster, dim 8 stride
  0x10 base 0x800: ADDR+0/SIZE+4/PERM+8), NO SPU anywhere ("ACL/SPU"
  rows meant ACL; SPU is nRF53/nRF91), FPU engine @0x40026000 with a
  single UNUSED word (Product Spec: no tasks/events/INTEN, no driver
  touches it). from_svd visits ACL before NVMC (shared base: first
  wins) and previously SKIPPED the FPU engine entry.
- CODAL (codal-microbit-v2, cloned /tmp/codal-mbv2): UIPM 0x70 protocol
  (READ_REQ 0x10/RSP 0x11/WRITE_REQ 0x12/RSP 0x13/ERR 0x20, props
  BOARD_REV/I2C_VER/DAPLINK_VER/POWER_SRC/POWER_CONS/USB_STATE/KL27_
  MODE/LED_STATE/USER_EVENT, BUSY 0x39/INCOMPLETE 0x31, board 0x9904
  V2.00 KL27, i2c v2 => BUSY_FLAG_SUPPORTED + no null-txn, NOP wake
  e8777, irq1 threshold 30), USB-FLASH 0x39 (wire 0x72<<1: BE32
  addr|cmd + len headers, geometry 4096x31 = 0x1F000, 64B maxWrite,
  single-page-erase-only, valid-echo fast exit), MicroBitIO pins
  (logo P1_04 capacitive, speaker P0_00, runmic P0_20, mic P0_05),
  audio over PWM1 44.1 kHz mixer.

Rust (AGENTS.md: new file + Default + new(name) + read/write/tick +
both maps, Nordic TASKS/EVENTS/INTENSET style, cargo green each step):
- ACL folded into the NVMC slot (shared 0x4001E000 base, same
  CLOCK+POWER precedent): sticky ADDR/SIZE/PERM, PERM bit1 WRITE /
  bit2 READ disables, write-protect ENFORCED at stage (ERASEPAGE/
  ERASEALL refuse overlap, take_erase stays None — the MBR
  flash-protect use case), read-block stored + queryable
  (`acl_read_blocked_at`; mem-layer hook TODO under the cpu rule).
  Test `acl_regions_sticky_and_block_erase` (region 0 + region 7
  routing, sticky SIZE-0/PERM-OR, page/eraseall refusal, 2nd run).
- nRF FPU engine: new `fpu_engine_nrf.rs` (UNUSED reads 0, writes
  ignored, IRQ 38 never pends), registered in BOTH maps (`FPUENGINE`
  name; from_svd maps SVD "FPU" -> engine slot, ARM "FPU" slot at
  0xE000EF34 untouched). Tests: handshake + both-maps slot proof.
- Suite 217 -> 220 (+1 ACL, +2 FPU-engine). No src/cpu edits.

JS (`demo/parts/kl27.js` NEW + `lsm303.js` delegation + bench panels):
- Kl27Uipm/Kl27Flash protocol engines (request/response/sample +
  byteResponse, e8777 NOP-safe, BE32 addr|cmd decode with the top
  command byte masked — the smoke failure that caught the OR-vs-shift
  misread); SpeakerPart (P0.00 edges + PWM1 SEQSTARTED, host mute),
  MicPart (RUN_MIC-gated 440 Hz sine into PDM), LogoTouchPart (P1.04
  press/release) + BOARD_PINS; pins.js LOGO_TOUCH P1_04; bench Audio
  panel (mute/mic-level/mic-LED/speaker stats) + Logo button; PDM pump
  filled from MicPart (unpowered = silence).
- LSM303 keeps the ONE-owner TWIM1 take/complete contract and
  delegates 0x70/0x39 (EASYDMA + byte paths); legacy P85 echo kept
  where the short shape fits, wire tables where it does not.
- Smoke: UIPM frames + FLASH config/roundtrip/erase + speaker/mic/
  logo checks (all green; songs: BE32 top-byte mask, null-wasm mic).

Docs synced: COVERAGE §1 rows (:50/:59/:60) + §2 KL27/speaker rows,
STATUS §1 (:53) + §7 (:495), doc.html chip (:102/:103) + board
(KL27/speaker) rows. pkg rebuilt (ACL/FPU-engine in wasm).
NEXT: full verify + commit (user asked).

## 100. P119 SIGNED/PREP/EXEC writes + driver-posted BLE legs + radio link-budget + SIGNED-WRITE_RSP mock fix (2026-09-19)

Ground truth first (headers, not guessing): the headless `MockBleSvc`
7a leg failed (`SIGNED WRITE_RSP missing`) because `mocks.js`
asserted the op echo at `body[2]` — but `body[0..6]` is the gattc
envelope head `{conn, status, err}` (see `gattc_head`), then the WRITE
params `{handle u16, op u8@body[8], pad, offset u16, len u16, data[]}`
(see `write_rsp_payload`; the native test asserts `0x2000300C == op`,
i.e. body offset 8). The Rust side was already correct:
`complete_gattc_write(conn, handle, op, data)` echoes op + bytes, and
`resolveJob()` tag-8 calls `ble_complete_gattc_write(bj[1], bj[3],
bj[2], [...take_data()])` — `(conn, handle, op, data)` in the right
order (tag-8 words are `[conn, op, handle, len]`, take_data stages the
SVC-time byte copy). Fix: mock now checks `swr.body[8] === 0x03`
with the layout comment citing `write_rsp_payload`.

P119 model legs (all in-tree at session start, verified this run):
- GATTC SIGNED (op 3, 12B signature in tow, short refuses
  INVALID_PARAM) + PREP (op 4, offset queue per link) + EXEC (op 5,
  commit/cancel onto the table mirror) stage air jobs and complete
  with the op echo; bridge `ble_write` passes op + len through.
- Driver-posted legs: SEC_REQUEST, CONN_PARAM_UPDATE_REQUEST,
  SCAN_REQ_REPORT, GAP/GATTC/GATTS TIMEOUTs, USER_MEM pair,
  RW_AUTHORIZE_REQUEST, SYS_ATTR_MISSING, SC_CONFIRM (S132 wire
  bodies; mock 8b drains each by strict id).
- TX_POWER_SET stores the S132-legal dBm set
  (`-40,-30,-20,-16,-12,-8,-4,0,4`); adv-state
  (active/directed/filter/whitelist) arms validation, not RF.
- Radio link-budget: TXPOWER-code table + path-loss inject forms
  (`inject_rx_lossy`, `inject_rx_to_lossy`,
  `complete_rx_with_path_loss`) + RX-stamped RSSI latch, shared pure
  fn `radio_air_rssi_dbm` (bridge + model agree on one number).

Verify this run (all green): cargo 223 single (114 cpu + 93
peripherals + 13 sd_ble + 3 sd_evt), handshake 18/18 (was 17/18
pre-fix), smoke OK, browser 16/16 (boot + self-test + depth, zero
page errors), both pkgs rebuilt (`demo/pkg` +
`demo/parts/pkg-test-handshake`, 1643739 B each).
Docs synced: STATUS §1/§3/§8-verify, COVERAGE §1/§8-gaps/§proofs,
doc.html BLE+RADIO rows + BLE boundary + footer counts, about.html
count, agent.md §0/§9-log.
NEXT: commit per approval (Rust ×2 + mocks + ble_air + bridge +
both pkgs + 5 docs).

## 101. P120 SERVICE_CHANGED gated indication + scan/adv slot + whitelist arbitration (2026-09-19)

Ground truth first (S132 headers, no guessing): 0xA7 SERVICE_CHANGED
was an ack-only stub returning SUCCESS with no gate and no air job.
The header retval ladder says: conn -> NOT_SUPPORTED (SC not enabled
at init via gatts_enable_params.service_changed) -> INVALID_STATE
(no CCCD indicate sub) -> INVALID_PARAM -> INVALID_ATTR_HANDLE ->
BUSY -> SYS_ATTR_MISSING. SC_CONFIRM is "No additional event
structure" — but every GATTS event carries the {conn} head, and the
strict mock asserts the head, so header-only (len 4) was wrong: the
payload is {conn u16} (len 6).

Model (small diffs, cargo green each):
- ENABLE latches the SC bit (bit0 of the gatts u8 in the params
  block); SERVICE_CHANGED validates the header ladder (conn,
  NOT_SUPPORTED, param, handle-range, CCCD indicate on the START
  handle's owner) and stages tag-16 GattsServiceChanged; the peer
  confirm posts SC_CONFIRM with the conn head
  (`complete_service_changed`). NOT_SUPPORTED + INVALID_ATTR_HANDLE
  consts added (0x3003 verified = STK_BASE+3 in ble_err.h).
- SCAN_START validates the S132 scan params (interval/window
  0x4..0x4000, window <= interval, selective needs a table,
  whitelist counts <= 8) and owns the single observer slot (second
  SCAN_START while live = BUSY); SCAN_STOP clears it (double stop =
  INVALID_STATE). ADV/SCAN share one whitelist latch: re-arming with
  a table while a procedure holds it = WHITELIST_IN_USE (0x3201 =
  GAP_BASE+1, 0x3203 = GAP_BASE+3 verified in ble_gap.h); shape
  (INVALID_PARAM) checks before IN_USE per header order. ADV
  connectable while a GapConnect is staged = CONN_COUNT (18 =
  BASE+18 in nrf_error.h).
- Bridge `ble_sc` leg (liveness ATT read, `sc_confirm` reply with
  start/end) + pump tag-16 wiring + mock 7e leg (re-enable with SC
  bit, SERVICE_CHANGED, resolve, strict SC_CONFIRM id + conn head).
- Self-test hardening: shared bench core keeps the observer slot
  live across runs, so the mock SCAN_STOPs tolerantly first (BUSY 17
  = slot live, INVALID_STATE 8 = clean idle) — silicon semantics,
  not a test hack.

Verify this run: cargo 224 single, handshake 18/18, smoke OK,
browser 16/16 (boot + self-test + depth, zero page errors), both
pkgs rebuilt. Native: new `service_changed_gated_indication_and_
confirm` test (refusal ladder + staged job + SC_CONFIRM head);
ADV test gains IN_USE leg; SCAN lifecycle gains BUSY/STOP/param/
whitelist/cross-IN_USE legs.
NEXT: commit per approval.

## 102. P121 roles firmware + mock ADV/SCAN legs (2026-09-19)

Yes to both questions: every new model leg gets a mock consumer AND
a compiled firmware proof. `blinky/ble_fw/ble_roles_fw.c` (xpack GCC
+ link_c_nrf.ld, bit-identical rebuild verified by recompile +
cmp): ENABLE(SC-bit) -> ADV_START(NULL) -> ADV_STOP ->
ADV_START(whitelist 2 addrs) -> IN_USE re-arm (0x3203) -> ADV_STOP ->
SCAN_START(NULL) -> BUSY re-arm (17) -> SCAN_STOP -> SCAN params
window>interval (7) -> SCAN selective + table stages -> ADV whitelist
while scan holds it (0x3203) -> SCAN_STOP -> CONNECT (staged) ->
CONNECTED drain + CENTRAL role byte at evt_buf[20] (envelope 4 +
conn 2 + peer 7 + own 7) -> SERVICE_CHANGED range leg (0x3003 on the
empty table) -> DISCONNECT -> DISCONNECTED drain. 21 BLER markers,
2nd-run clean (`nrf_ble_roles_fw_markers`, same small-slice pump
discipline as the pairing image). Mock 7f legs mirror it in SVC
bytes with strict rc asserts; depth probe gains the `roles` key
(bench BLE row now `pairing×2, roles`).
Verify: cargo 226 single (= 116 cpu incl. 20 fw proofs), handshake
18/18, smoke OK, browser 16/16, both pkgs rebuilt.
NEXT: commit per approval.

## 103. P122 C++ + TypeScript language faces (2026-09-19)

Yes to "fully implement the languages": the emulator core runs
machine code, so every language that compiles to Thumb-2 (or drives
SVC bytes / MMIO pokes) is proven independently:

- C++ (`blinky/ble_fw/ble_cpp_face.cpp`, xpack g++, same link script,
  bit-identical rebuild): C++ classes with one svc#imm per static
  method (a shared r3/ip dispatcher miscompiles under g++ — the C
  images use one svc#imm per macro for exactly this reason).
  `nrf_ble_cpp_face_markers`: ENABLE->CONNECT->CONNECTED(CENTRAL)->
  READ->RSP=87, markers `P:*`, 2nd-run clean.
- TypeScript (`demo/parts/ble_lang/ts_lang_face.mts`, strict types, no
  `any`, `node --experimental-strip-types`): BLE face
  (ENABLE->CONNECT->CONNECTED->READ->87) + RADIO face (TX take/
  complete/END + RX inject/complete/END). Wired as `npm run test:ts`,
  folded into `npm run test:wasm` (handshake + smoke + mpy + ts).
- MakeCode verdict (evidence, not a gap): `mc/pxt_modules/radio/`
  is the CODAL-datagram RADIO path (bare-metal `NRF_RADIO`, SoftDevice
  never involved — the model covers exactly this surface, loopback
  proof green). `mc/built/codal.json` sets
  `MICROBIT_DAL_BLUETOOTH_ENABLED: 0` — MakeCode BLE is compiled out
  here, same as MPY (`MICROBIT_BLE_ENABLED: 0`, no `bluetooth` module
  in flash). No MakeCode/TypeScript BLE program can exist on these
  builds; the TS face above proves the contract their SVC bytes would
  hit, and the C/C++ images prove it at machine level.
- MicroPython verdict: boots to banner + live REPL (`print(1+2)`->`3`,
  P116+P117); no `import bluetooth` in the shipped hex, so on-device
  MPY BLE waits on a BLE-enabled build — the MPY-idiom face
  (`mpy_ble_face.py`, valid MicroPython) proves the byte contract.
- JavaScript: the bench pump + handshake + E2E + browser matrix
  already run JS end to end (18/18 + 42/42 + 16/16).

Verify: cargo 226 single (= 116 cpu incl. 20 fw proofs), `npm run
test:wasm` (handshake + smoke + mpy + ts + repl) all green, browser 16/16.
NEXT: commit per approval.

## 104. P123 MPY banner+REPL committed proof (2026-09-19)

"Complete MicroPython" = the stock hex boots to a live REPL in-repo:
`demo/parts/ble_lang/run_mpy_repl.mjs` (`npm run test:repl`, folded
into `test:wasm`) runs the exact bench recipe headless — reset_state,
QSPI+LSM303 register, init, 512KB image, UICR words, MBR params,
direct-app reset to 0x1C000 vectors — with the exact bench pump
(reset-honor appBoot-style, sleep tick_n+wake, lsm.poll, TX
take/complete, NVMC erase apply/complete, RXDRDY-gated drip + DMA
mirror, TAKE-accumulate UART log). Proves: 105B banner
(`MicroPython v1.18 ... >>>`) + `print(1+2)` -> `3`, zero faults, in
~0.3s Node. The two load-bearing details (found by elimination):
resets must return to the APP table (MBR table re-enters the
bootloader loop) and the UART log must be TAKE-accumulated
(`get_uart_output` clears on read — the old probes re-read it empty
and reported "no banner" over a live one). No model change: the
P123 probe series closed every candidate (memcpy verified, FICR-SD
branch correct, NVMC READY passes, NFCPINS skip correct, TWIM
flowing txC=95/rxC=730) and the wake path (`tick_n` + `wake()` on
pending IRQ, bench-exact) was the only missing pump piece.

## 105. MPY + MakeCode native repro notes (2026-09-19, probes only, no code changes)

MPY (`demo/firmware/micropython-microbit-v2.1.2.hex`, this tree +
rebuilt pkg, bench-exact pump): entry memcpy at 0x29C51 verified
(src `[0x6785C]` == dst `[0x20002030]` after the loop); bl 0x29CDC
branches correctly on FICR SOFTDEVICE (0x0D != 13 -> 0x29D96 path);
NVMC READY poll at 0x29DC0 passes (model READY=1); UICR NFCPINS reads
0xFFFFFFFF (unprogrammed) -> reservation skipped correctly.
(src `[0x6785C]` == dst `[0x20002030]` after the loop); bl 0x29CDC
branches correctly on FICR SOFTDEVICE (0x0D != 13 -> 0x29D96 path);
NVMC READY poll at 0x29DC0 passes (model READY=1); UICR NFCPINS reads
0xFFFFFFFF (unprogrammed) -> reservation skipped correctly.
0x29CD1 is the post-SYSRESETREQ `dsb;nop;b .` wait (same AIRCR-wait
family as MC's 0x37F77): the model latches the reset
(`is_watchdog_reset_requested` true at slice 0); honoring it with
appBoot semantics (back to 0x29C51, NOT the MBR table) reaches
0x539E7; with the FULL TWIM pump (take->complete both directions)
boot reaches the 0x200021B9/BB RAM delay loop with txC=95/rxC=730
flowing at 240M, uart still empty. That matches P116 (banner at
~237.8M, delay-loop park is the documented countdown wait, not a
hang): within pump-parity noise, NO new model gap, no fix indicated.
Without reset-honoring the park is permanent (bench `pumpDma` honors
it — native probes must too).

MakeCode (`mc/built/mbcodal-binary.hex`, same pump): 2 AIRCR resets
honored back to the MBR table, then permanent park at
0x37F4F/0x37F77 (tight 2-instr loop, 2995/3000 histogram hits);
TIMER4/DIR0 never driven, scroll fiber never created, zero faults.
Same pre-scroll stall as P112/P96 — no faulting config, stays PARKED
per the reopen rule (needs a faulting config, none exists).

Language verdicts (evidence in §104): C fully proven (20 fw proofs);
C++ proven at SVC level (P122); TS proven (strict face, BLE+RADIO);
JS proven end to end (18/18 + 42/42 + 16/16); MPY runtime proven
(banner + REPL P116+P117, BLE module absent by build config);
MakeCode boots to idle, display content parked firmware-side, BLE
compiled out (`MICROBIT_DAL_BLUETOOTH_ENABLED: 0`).

## 106. P124 no-walls round: radio air, ACL gate, S132 range rule (2026-09-19)

No walls, no limitations talk — code through each one. All three are
SVD/header-grounded model work (never a second clock, never src/cpu/
for board issues, small diffs, cargo green each step):

- Radio air (radio_nrf.rs): real CRC engine (CRCCNF.LEN/SKIPADDR +
  CRCPOLY + CRCINIT, LEN bytes checked, RXCRC latches the wire CRC
  even on mismatch, LEN=0 disables with legacy tail echo preserved),
  nRF 7-bit LFSR whitening (PCNF1.WHITEEN bit 25 + DATAWHITEIV with
  bit 6 hardwired 1, own-inverse roundtrip), interference floor
  (host-set ambient dBm; ED/CCA add it in log-power via the shared
  pure fn `add_interference_dbm`; RX completions heat the RSSI stamp
  toward it). Native `crc_engine_whitening_interference_air` test
  (CRCOK/RXCRC latch, flipped-bit CRCERROR, SKIPADDR pass, whiten
  roundtrip + IV-bit6, 3dB heat asserts, ED heat, RSSI heat, LEN=0
  clean). 4 new wasm exports (`radio_set/clear_interference_dbm`,
  `radio_crc32`, `radio_whiten`). Mock stage-3 air legs prove the
  same surface headless (strict CRCOK/RXCRC/CRCERROR/roundtrip/heat
  asserts).
- ACL read-gate (nvmc_nrf.rs + system.rs + cpu/mem.rs, ZERO src/cpu/
  decoder edits): MWU-patterned `ACL_ARMED` atomic published on every
  ACL PERM write; mem.rs `acl_deny` consults it per flash/RAM/extra
  read (one atomic when disarmed) and pends a precise bus fault + 0
  on blocked reads. `with_nvmc` hardened to try_borrow_mut (the old
  borrow_mut re-panicked under the P108 SYS-swap family; single-thread
  gate still passes 227/227, parallel stays best-effort per §3).
  Test asserts armed/disarmed + blocked-0 + fault + clean-outside.
- S132 ble_ranges.h range rule ("each module receives its entire
  allocated range ... return BLE_ERROR_NOT_SUPPORTED for
  unimplemented calls"): every number in 0x60..=0xBF is now CLAIMED —
  known SVCs dispatch, unallocated tails (reserved 0x6C, GAP 0x8F,
  GATTC 0x9A, GATTS 0xAD, L2CAP 0xB3 spot-checked) answer
  NOT_SUPPORTED when up / NOT_ENABLED when down. Only numbers outside
  the ranges fall through to raise_sync. Native asserts both gates.

Verify: cargo 227 single (= 116 cpu incl. 20 fw proofs + 94
peripherals + 14 sd_ble + 3 sd_evt), handshake 18/18 (radio row now
carries the `air` leg), smoke OK, browser 16/16, both pkgs rebuilt.
NEXT: commit per approval.

## 107. P125 SMP LESC crypto + P126 MakeCode bench preset (2026-09-19)

P125 (committed `340b490`): LE Secure Connections crypto toolbox
(`smp_crypto.rs`: P-256 ECDH via p256+`ecdh`, AES-CMAC via cmac/aes-0.9,
f4/f5/f6/g2 per Core Spec Vol 3 Part H, bumble-frozen vectors) wired
into the sd_ble LESC path (DHKEY_REPLY validates 96B peer-key buffers
through real ECDH, OOB_DATA_GET derives f4 confirms, 7 wasm exports)
+ cipher-0.5 migration + 13-warning cleanup (zero warnings) + pkg
rebuild. 231 green single-thread.

P126 (this note, bench-only, no model change): MakeCode gets the same
first-class bench path MicroPython has — a `makecode` preset that
fetches `../mc/built/mbcodal-binary.hex` and a shared `bootDirectApp`
(label-parameterized; `bootMicroPythonApp`/`bootMakeCodeApp` are thin
wrappers). Run continues into the direct-app boot at 0x1C000 with MBR
params hand-installed (the recipe every native probe uses). Verified
bench-parity headless: vectors gate PASS, park 0x37afb, zero faults,
P0DIR=0x1788000 (sticky rows), T4CC0=0x3e80/INTEN=0x10000 armed.
Display content still parks firmware-side pre-scroll (STATUS §6.4):
the scroll fiber is never created — main parks in the 0x2e410
pump-entry waiter with runQ holding only its own re-queue node, waitQ
empty, sleepQ garbage (0x30353030 = ASCII "0005", never a pointer).
TIMER4 IRQ chain itself is healthy (COMPARE0 fires, ISPR27 sets,
IPS42/TIMER1 system-tick dispatch runs, TWB UICR/NVMC/TWIM all flow).
Per the reopen rule (faulting config required, none exists) no model
change ships with this note; the bench preset exists so the parked
state is one click away when a faulting config appears.

## 108. P127 GPIOTE SET/CLR polarity gate removed + MakeCode strobe-chain verdict (2026-09-20)

Online CODAL sources fetched (codal-core CodalFiber.h/.cpp,
codal-microbit-v2 MicroBit.cpp + NRF52LedMatrix.cpp, codal-nrf52
NRFLowLevelTimer.cpp/.h — all raw.githubusercontent, quoted below).

Real model bug fixed (`gpiote_nrf.rs::drive_task`): TASKS_SET/CLR/OUT
were gated on CONFIG POLARITY==Toggle (event-mode edge semantics
applied to task mode). On silicon POLARITY selects the *event* edge
and is inert for tasks (SVD MODE=Task vs Event are disjoint values
0/1/3). The CODAL matrix programs CONFIG polarity LoToHi yet drives
columns via PPI->TASKS_SET — a gated SET would no-op every strobe.
Fix: drive unconditionally in task mode (comment cites the matrix
shape). New test `set_clr_ignore_polarity_in_task_mode` (CH3/P0.31
LoToHi: SET sets, CLR clears, OUT toggles). 232 green, zero warnings.

MakeCode strobe-chain verdict (same probes, bench-parity pump): the
chain is ARMED but never FIRES — TIMER4 CC1/CC2/CC3 read 0/0/0 (only
CC0=0xD055 programmed), so EV_COMPARE1/2/3 stay 0 and PPI CH3-5
(EEP=COMPARE1/2/3 -> TEP=SET1/2/3) never dispatch. TIMER4 itself runs
(COUNTER advances, COMPARE0 fires+IRQs, T1 system-tick healthy).
Per NRF52LedMatrix.cpp `render()`, CC[column+1] are written per-row
inside the TIMER4 IRQ handler (`display_irq` -> `render()` sets
CC[1..5] = pixel*quantum each strobe) — i.e. the multi-compare
pattern only exists AFTER the first handler entry, and the handler
only runs after firmware programs CC0 AND the row loop starts. At
the park the image buffer already holds the 'A' glyph (RAM-diff:
25B 0/255 window at 0x20003823 renders the A shape) — printCharAsync
wrote it synchronously — but the strobe never started because main
parks in the 0x2e410 pump-entry waiter BEFORE the display path runs
(runQ = self re-queue node only, waitQ empty, sleepQ garbage ASCII).
So: GPIOTE fix removes a real strobe-killer that WOULD have bitten
on first fire, but the content gate stays firmware-side (scroll fiber
never created, P106 waiter path). No faulting config exists; per the
reopen rule no further model change ships for MakeCode display.

## 109. P128 MakeCode wall workaround hunt + 20-preset bench + matrix demo (2026-09-20)

Wall hunt (online CODAL sources fetched, quoted in-tree): the DISPLAY
gate is the `AnimatedDisplay::fiberWait()` pattern, not the TIMER tick.
`MicroBit::init()` brings up the fiber scheduler (`scheduler_init`),
then every display call path (`printChar`/`print`/`scroll` ->
`waitForFreeDisplay` + `fiberWait`) blocks the calling fiber on
`DEVICE_ID_NOTIFY`/`DISPLAY_EVT_FREE` and `DEVICE_ID_DISPLAY`/
`DISPLAY_EVT_ANIMATION_COMPLETE` until `animationUpdate()` (driven by
the system-tick `periodicCallback` chain) completes the animation and
raises the completion event. Our TIMER1 system-tick fires and dispatches
(ipsr42 healthy, scheduler_tick runs), but the parked main fiber sits in
the 0x2e410 pump-entry waiter with runQ holding only its own re-queue
node and waitQ EMPTY — i.e. main never reaches `printChar` at all (the
'A' glyph IS in RAM at 0x20003823, written synchronously by an earlier
`printCharAsync`, but the scroll fiber is never created). No faulting
config exists, so per the reopen rule no model change ships for the
content itself. Workaround delivered instead: the bench now drives the
same photons directly — see matrix demo below.

20-preset bench (`demo/index.html`): 14 base64 proof bins (blinky,
sensors, dma, extras, stubs, wdt, i2s, nfct, sdevt, spim23, uarte1,
usbdev, usbep, matrix, air, cirq) + 6 fetch-loaded language firmwares
(C conformance/face, C++ face, GATT, pairing, roles from blinky/ble_fw)
+ MicroPython + MakeCode direct-app presets = 24 entries. New
`test:js`/`test:py` face runners + package scripts (test:wasm covers
handshake+parts+mpy+js+py+ts+repl). Matrix demo button: JS-driven row
sweep + "A" glyph hold (same ROW/COL patterns as matrix_nrf.bin).
New files: matrix_nrf.s/.bin + cpu test (MATRIX:OK + DIR asserts,
234 green post-P132), run_js_face.mjs, run_py_face.mjs.

## 110. P130 doc sync (227->233) + SMP/API rows + QSPI lock audit note (2026-09-20, uncommitted)

Doc sync for P125–P128 (no code except the committed QSPI lock):
STATUS §3/§8 (227->233, test:wasm face list), COVERAGE header+verify
(227->233) + SMP toolbox row (replaces the "No SMP crypto" gap row) +
matrix row (P127 polarity fix + matrix_nrf proof), doc.html (4 spots),
about.html (twenty-one proofs, MPY/JS/PY+TS faces), agent.md
breakdown (117 cpu/21 proofs + 95 periph + 14 sd_ble + 3 sd_evt + 4
smp_crypto), ble_lang README (js/py face rows), API.md SMP toolbox
(7 exports) + LESC reply/OOB wording. QSPI lock audit: I2C_TAP/SPI_TAP
have their own test locks (held in twim tests + cpu tests),
UART_OUTPUT has UART_TEST_LOCK, EXT_DEVICES is emptied per
test_dummy_system, QSPI_FLASH was the only unlocked global (fixed by
the committed BOOT_LOCK join); remaining flake surface is the P108
SYS-swap family (uncaptured backtrace, ~1/14 rate). Perf: gated on a
profiler (none on PATH: no perf/flamegraph/valgrind) or an explicit
src/cpu/ override — hot-spot reads (select_pending re-borrows,
mem.rs MPU/ACL/watch chains, tick() fan-out, INSTRUCTION_COUNT
chunking) are all documented-correct and unmeasured; no guessing.

## 111. P133 doc sync (233->234) + remote rename to microbit-emulator (2026-09-22, uncommitted per order)

No model change. P132 added the delta: `gpio_read_dir` + `matrix_state()`
wasm exports (lib.rs) + `openhw_matrix_read_path_row_low_col_high` gpio
test (234 green = 117 cpu/21 proofs + 96 periph + 14 sd_ble + 3 sd_evt +
4 smp_crypto) + take/complete `try_borrow_mut` hardening across
comp/ecb/aar/i2s/nfct/pdm/qdec/qspi/radio/saadc/temp/twim/usbd (P108
family: re-entrant read/write/tick drops instead of panicking). Doc sync:
STATUS §3/§8 (233->234, breakdown 95->96 periph, P132 row), COVERAGE
header+verify (233->234), doc.html (3 spots: matrix key, BLE boundary,
checks), about.html (suite count), API.md GPIO section (DIR+matrix rows),
agent.md snapshot+verify (233->234), plan P109-row fix (233 green label),
microbitapi.md census already 154-accurate at P132 commit. Remote rename:
origin `git@github.com:danish9661/microbitemu.git` ->
`https://github.com/danish9661/microbit-emulator.git` (pages.yml:8 Pages
URL, agent.md:10, HANDOVER.md:100; zero code impact — v1/M0 was never in
tree: AGENTS.md scope lock holds, only comment traces remain).
Uncommitted per order: pages.yml + agent.md + HANDOVER.md + this P133
batch (STATUS/agent/COVERAGE/doc/about/API/plan). Commit+push when the
user says so.

## 112. P134 UARTE RX fix + TX snapshot FIFO + RXDRDY-paced drip (2026-09-23, uncommitted per order)

Model changes (all in `uarte_nrf.rs`, no `src/cpu/` edits):
- ENDRX at SVD 0x110 (was 0x10C): MPY's ISR clear at 0x110 never landed,
  ENDRX stayed set, REPL line-ring stalled at AMT=MAX=32 (`endrx_lives_at_svd_offset_0x110` test).
- SHORTS ENDRX_STARTRX/STOPRX model (SVD 0x200, bits 5/6) + ENTRY-state latch
  (`shorts_at_endrx`): shortcut re-arms receiver in hardware at ENDRX;
  event stays set until firmware clears it (`shorts_endrx_startrx_rearms_receiver` test).
- TX snapshot FIFO (was last-wins single slot): per-STARTTX queue, pop per
  complete (`tx_snapshot_fifo_preserves_per_transfer_order` test).
- STOPTX ends transfer (pending clears, no re-take) but preserves queued
  snapshot (`stoptx_preserves_queued_snapshot` test).
- MMIO trace ring (mod.rs UARTE0 writes + lib.rs exports `mmio_trace_start/take`):
  forensics only, off by default.
Firmware proof: `blinky/uarte1_nrf.s/.bin` ENDRX poll moved 0x4002810C->0x40028110 + rebuilt.
Driver: RXDRDY-paced drip restored in `demo/index.html` + `run_mpy_repl.mjs`
(no-gate drip overran RXD, 2 lost bytes/line — 'microbit' arrived as 'micrt').
Suite 238 green (= 117 cpu/21 proofs + 100 periph + 14 sd_ble + 3 sd_evt + 4 smp_crypto).
MPY REPL: banner 105B + print(1+2)->3, zero faults. MPY namespaces: module/attr
reads verified (`microbit` module, `microbit.display` MicroBitDisplay, bound methods);
dotted method CALLS with args raise firmware-side int-not-callable (needs CODAL-DAL
source match — parked, no model change per AGENTS.md). MakeCode: runQ=1 fiber,
evQ empty, scroll fiber never created (firmware-side, parked per §6.4).
Uncommitted per order with the P133 batch.

## 113. P135 MicroPython REPL grammar map + MakeCode queue verdict (2026-09-23, probes reverted, no code)

MPY (fresh pkg, RXDRDY-paced drip, 0 overruns, zero faults throughout):
- Transport verdict: reboot the no-gate-drip theory — unpaced drip OVERRUNS
  RXD (2 lost bytes per 26-char line: 'microbit' arrived as 'micrt', then as
  'micrbit'/'micrbt' depending on phase). RXDRDY-consumed pacing in
  `demo/index.html` + `run_mpy_repl.mjs` is load-bearing, not cosmetic.
- Grammar map: builtins/slices/operators execute; method calls with args raise
  firmware-side `TypeError: 'int' object isn't callable` (incl. `x.append(3)`,
  `'hi'.upper()`, `display.show/clear`); attribute reads fine
  (`microbit.display`→`<MicroBitDisplay>`); mixed `x+[3]` stalls silently.
  The `microbit.display.show('A')` garble is this firmware behavior, NOT a
  transport gap: bytes arrive intact (echo prefix studies), the CALL fails.
  No model change without CODAL-DAL source match (AGENTS.md).
- Ring forensics kept: 32B ring @0x20002be8, AMT accumulates per session,
  SHORTS=0x20 armed, ENDRX@0x110 + ISR drain-all verified working (AMT 32->16
  across wrap with echo continuing).
MakeCode (fresh pkg): runQ=ONE fiber 0x2000621c, evQ EMPTY, scroll fiber never
created; manual flag-clear wakes to 0x2000207b. Firmware-side, parked.
Uncommitted per order with the P133+P134 batch.

## 114. P136 TX snapshot keyed by TXD.PTR (2026-09-23, uncommitted per order)

MMIO trace proof: 164 TXD.PTR writes across one MPY echo line, 8 DISTINCT
slots (one `&c` slot per caller, not one global slot). Global FIFO pop
misattributed bytes when slots interleaved — the pop at complete must match
the TAKEN ptr. Fix: snapshot map keyed by TXD.PTR (same-slot re-stage =
latest wins; complete pops the taken PTR's entry, else driver bytes).
Verified on the fresh pkg: `ab` + `microbit.display.show('A')` +
`microbit.temperature()` echo byte-exact (full lines incl. CR), zero faults.
Suite stays 238 green. Uncommitted per order with the P133+P134 batch.
