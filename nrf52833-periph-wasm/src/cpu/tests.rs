//! Native bring-up tests for the WASM-native Thumb-2 CPU.
//!
//! These run the real firmware binaries through `WasmCpu` + the real
//! peripheral model (SVD map) without any JS/Unicorn involvement, so the
//! edit-compile-debug loop stays inside `cargo test`. They deliberately do
//! NOT call `tick_n` (no INSTRUCTION_COUNT movement) and only drain their
//! own UART output, so they are independent of the other (parallel) tests.
//! The two tests serialize on `BOOT_LOCK` because they share the process
//! `SYS` instance.

use super::{Cpu, mem::FlatMemory};
use super::mem::Memory;
use crate::system::WasmSystem;

fn lock_boot() -> std::sync::MutexGuard<'static, ()> {
    crate::system::lock_boot()
}

fn boot(bin: &[u8]) -> (Cpu, FlatMemory) {
    assert!(bin.len() >= 8);
    let sp = u32::from_le_bytes([bin[0], bin[1], bin[2], bin[3]]);
    let pc = u32::from_le_bytes([bin[4], bin[5], bin[6], bin[7]]);
    assert!(sp != 0 && pc != 0, "bad vector table");
    // Install a fresh hardcoded-map system as the process instance.
    // File-free on purpose: board snapshots carry no SVD/firmware, so the
    // harness must not depend on test-data files (init_svd_for_test does).
    let sys = WasmSystem::new();
    crate::init_for_test(sys);
    let mut cpu = Cpu::new(sp, pc | 1);
    let mut mem = FlatMemory::new(512 * 1024, 128 * 1024);
    mem.load(bin, 0x00000000);
    assert_eq!(mem.read32(0x00000000), sp, "flash load failed");
    // Drain stale UART when possible (best-effort: marker tests hold the
    // UART lock across their whole body and drain explicitly themselves).
    if crate::system::try_lock_uart().is_some() {
        crate::system::get_uart_output().lock().unwrap().clear();
    }
    (cpu, mem)
}

fn no_fault(cpu: &Cpu, mem: &FlatMemory) {
    assert!(
        cpu.fault.is_none(),
        "cpu faulted: pc={:08x} op1={:04x} op2={:04x} len={}",
        cpu.fault.map(|f| f.pc).unwrap_or(0),
        cpu.fault.map(|f| f.op1).unwrap_or(0),
        cpu.fault.map(|f| f.op2).unwrap_or(0),
        cpu.fault.map(|f| f.len).unwrap_or(0),
    );
    assert_eq!(
        mem.bad.get(),
        None,
        "bad memory access at pc={:08x}",
        cpu.regs.r[15] & !1
    );
}

#[test]
fn synth_vector_boot() {
    // Reset-vector boot without any files: SP/PC come from the table.
    // (Replaces the old blinky-firmware boot test, which needed a .bin.)
    let _g = lock_boot();
    let mut img = vec![0u8; 8];
    img[0..4].copy_from_slice(&0x20002000u32.to_le_bytes());
    img[4..8].copy_from_slice(&0x00000101u32.to_le_bytes());
    let (cpu, mem) = boot(&img);
    assert_eq!(cpu.regs.r[13], 0x20002000, "SP from vector table");
    assert_eq!(cpu.regs.r[15] & !1, 0x00000100, "PC from vector table");
    assert_eq!(cpu.ipsr, 0, "thread mode");
    no_fault(&cpu, &mem);
}

#[test]
fn nrf_blinky_firmware_runs() {
    let _u = crate::system::lock_uart();
    crate::system::get_uart_output().lock().unwrap().clear();
    // Real bare-metal nRF52833 firmware (blinky_nrf.s, GCC): HFCLK start,
    // P0.21 DIR, UART "BOOT" + 2x "BLINK", LED toggles. Proves the full
    // path: flash@0x0 -> CLOCK -> GPIO -> UARTE.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    let sys = crate::sys();
    cpu.run(sys, &mut mem, 5_000_000);
    assert!(cpu.fault.is_none(), "blinky faulted: {:?}", cpu.fault);
    let out = crate::system::get_uart_output().lock().unwrap().clone();
    assert!(out.contains("BOOT"), "missing BOOT marker, got {out:?}");
    assert!(out.contains("BLINK"), "missing BLINK marker, got {out:?}");
    // LED pin P0.21 was driven (ends OFF after 2 blinks, DIR stays set)
    assert_eq!(sys.p.gpio.borrow().dir[0] & (1 << 21), 1 << 21, "P0.21 DIR");
}

#[test]
fn nrf_matrix_led_sweep_and_glyph() {
    let _u = crate::system::lock_uart();
    crate::system::get_uart_output().lock().unwrap().clear();
    // 5x5 matrix proof (matrix_nrf.s): row-sweep with column drive,
    // then the "A" glyph; MATRIX:OK when GPIO reads back driven state.
    // Rows sink (OUT=0), columns source (OUT=1) per parts/pins.js.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/matrix_nrf.bin"));
    let sys = crate::sys();
    cpu.run(sys, &mut mem, 5_000_000);
    assert!(cpu.fault.is_none(), "matrix faulted: {:?}", cpu.fault);
    let out = crate::system::get_uart_output().lock().unwrap().clone();
    assert!(out.contains("MATRIX:OK"), "missing MATRIX:OK marker, got {out:?}");
    // Row/col DIR latched (P0 rows+cols, P1.5 col4)
    assert_eq!(sys.p.gpio.borrow().dir[0] & 0xD8988000, 0xD8988000, "P0 matrix DIR");
    assert_eq!(sys.p.gpio.borrow().dir[1] & 0x20, 0x20, "P1.5 DIR");
}

#[test]
fn nrf_sensors_buttons_twim_gpiote() {
    let _u = crate::system::lock_uart();
    let _t = crate::system::lock_i2c_tap();
    crate::system::get_uart_output().lock().unwrap().clear();
    // P3 firmware (sensors_nrf.s): TWIM0 accel probe + GPIOTE BTN_A event.
    // BTN_A is active-low: idle pull-up HIGH = released, drive LOW = pressed.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/sensors_nrf.bin"));
    let sys = crate::sys();
    sys.p.gpio.borrow_mut().set_input_pin(0, 14, false);
    cpu.run(sys, &mut mem, 1_000_000);
    assert!(cpu.fault.is_none(), "sensors faulted: {:?}", cpu.fault);
    let out = crate::system::get_uart_output().lock().unwrap().clone();
    assert!(out.contains("SENS:OK"), "missing SENS marker, got {out:?}");
    assert!(out.contains("BTN:1"), "button press not seen, got {out:?}");
    let ev = crate::system::i2c_tap_take_tx("TWIM0");
    assert!(ev.contains(&0x28), "accel reg probe missing, got {ev:?}");
}

#[test]
fn nrf_extras_saadc_temp_rng_pwm() {
    let _u = crate::system::lock_uart();
    crate::system::get_uart_output().lock().unwrap().clear();
    // P4 firmware (extras_nrf.s, GCC): SAADC/TEMP/RNG/PWM0 handshake.
    // Toolchain note: micro:bit firmware is built with ARM GCC directly
    // (xpack 14.2.1 via arduino packages); arduino-cli itself is proven
    // working by compiling an (M33, out-of-scope) UNO R4 sketch.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/extras_nrf.bin"));
    let sys = crate::sys();
    cpu.run(sys, &mut mem, 8_000_000);
    assert!(cpu.fault.is_none(), "extras faulted: {:?}", cpu.fault);
    let out = crate::system::get_uart_output().lock().unwrap().clone();
    assert!(out.contains("SAADC:OK"), "missing SAADC marker, got {out:?}");
    assert!(out.contains("TEMP:OK"), "missing TEMP marker, got {out:?}");
    assert!(out.contains("RNG:OK"), "missing RNG marker, got {out:?}");
    assert!(out.contains("PWM:OK"), "missing PWM marker, got {out:?}");
}

#[test]
fn nrf_stubs_spim_pdm_qspi_usbd_radio() {
    let _u = crate::system::lock_uart();
    crate::system::get_uart_output().lock().unwrap().clear();
    // P5 firmware (stubs_nrf.s, GCC): SPIM alias + PDM + QSPI + USBD + RADIO.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/stubs_nrf.bin"));
    let sys = crate::sys();
    cpu.run(sys, &mut mem, 8_000_000);
    assert!(cpu.fault.is_none(), "stubs faulted: {:?}", cpu.fault);
    let out = crate::system::get_uart_output().lock().unwrap().clone();
    assert!(out.contains("STUBS:OK"), "missing STUBS marker, got {out:?}");
}

#[test]
fn nrf_uarte1_instance_dma_roundtrip() {
    // uarte1_nrf.s (GCC): UARTE1 (0x40028000, IRQ 40) TX DMA + RX DMA,
    // each completed driver-side in phases (take -> RAM move ->
    // complete), exactly like the JS driver will. Proves the second
    // instance stages through the shared take/complete path (the unit
    // test only drives registers, never firmware bytes).
    let _u = crate::system::lock_uart();
    crate::system::get_uart_output().lock().unwrap().clear();
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/uarte1_nrf.bin"));
    let sys = crate::sys();
    // Phase 1: firmware stages UARTE1 TX DMA, spins on ENDTX.
    cpu.run(sys, &mut mem, 20_000);
    let (ptr, len) = crate::peripherals::uarte_nrf::take_txdma(sys).expect("uarte1 tx staged");
    let bytes: Vec<u8> = (0..len).map(|i| mem.read8(ptr.wrapping_add(i))).collect();
    assert_eq!(bytes, b"U1DATA\n", "uarte1 TX source bytes");
    crate::peripherals::uarte_nrf::complete_txdma(sys, &bytes);
    // Phase 2: firmware stages UARTE1 RX DMA, spins on ENDRX.
    cpu.run(sys, &mut mem, 20_000);
    let (ptr, len) = crate::peripherals::uarte_nrf::take_rxdma(sys).expect("uarte1 rx staged");
    assert_eq!(len, 3, "uarte1 RX MAXCNT");
    for (i, &b) in [b'A', b'B', b'C'].iter().enumerate() {
        mem.write8(ptr.wrapping_add(i as u32), b);
    }
    crate::peripherals::uarte_nrf::complete_rxdma(sys, len);
    // Phase 3: drain to done loop.
    cpu.run(sys, &mut mem, 2_000_000);
    assert!(cpu.fault.is_none(), "uarte1 faulted: {:?}", cpu.fault);
    let out = crate::system::get_uart_output().lock().unwrap().clone();
    // TX bytes hit the console (shared UART_OUTPUT, like UARTE0).
    assert!(out.contains("U1DATA"), "missing UARTE1 TX bytes, got {out:?}");
    assert!(out.contains("U1TX:OK"), "missing U1TX marker, got {out:?}");
    assert!(out.contains("U1RX:OK"), "missing U1RX marker, got {out:?}");
    // 2nd run: fresh boot, no leak (ENDTX re-arms per boot).
    crate::system::reset_globals();
}

#[test]
fn nrf_spim23_dma_roundtrip() {
    // spim23_nrf.s (GCC): SPIM2 (0x40023000) TX DMA + SPIM3 (0x4002F000)
    // RX DMA, each completed driver-side in phases (take -> RAM move ->
    // complete), exactly like the JS driver will. Proves the dedicated
    // slots stage DMA frames, not just START/STOP (stubs_nrf.s covers
    // the handshake only; SPI has no address phase, so no slave needed).
    let _u = crate::system::lock_uart();
    crate::system::get_uart_output().lock().unwrap().clear();
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/spim23_nrf.bin"));
    let sys = crate::sys();
    // Phase 1: firmware stages SPIM2 TX DMA, spins on ENDTX.
    cpu.run(sys, &mut mem, 20_000);
    let (addr, ptr, len) = crate::peripherals::twim_nrf::take_txdma(sys, "SPIM2").expect("spim2 tx staged");
    let bytes: Vec<u8> = (0..len).map(|i| mem.read8(ptr.wrapping_add(i))).collect();
    assert_eq!(bytes, vec![0x01, 0x02, 0x03, 0x04], "spim2 TX source bytes");
    crate::peripherals::twim_nrf::complete_txdma(sys, "SPIM2", &bytes);
    let _ = addr;
    // Phase 2: firmware stages SPIM3 RX DMA, spins on ENDRX.
    cpu.run(sys, &mut mem, 20_000);
    let (addr, ptr, len) = crate::peripherals::twim_nrf::take_rxdma(sys, "SPIM3").expect("spim3 rx staged");
    assert_eq!(len, 4, "spim3 RX MAXCNT");
    for (i, &b) in [0xAAu8, 0xBB, 0xCC, 0xDD].iter().enumerate() {
        mem.write8(ptr.wrapping_add(i as u32), b);
    }
    crate::peripherals::twim_nrf::complete_rxdma(sys, "SPIM3", len);
    let _ = addr;
    // Phase 3: drain to done loop.
    cpu.run(sys, &mut mem, 2_000_000);
    assert!(cpu.fault.is_none(), "spim23 faulted: {:?}", cpu.fault);
    let out = crate::system::get_uart_output().lock().unwrap().clone();
    assert!(out.contains("S2TX:OK"), "missing S2TX marker, got {out:?}");
    assert!(out.contains("S3RX:OK"), "missing S3RX marker, got {out:?}");
    // 2nd run: fresh boot, no leak (ENDTX re-arms per boot).
    crate::system::reset_globals();
}

#[test]
fn nrf_sd_evt_flash_success_roundtrip() {
    // sd_evt_nrf.s (GCC): svc 16 arms the SoC transport, NVMC ERASEPAGE
    // stages, driver take -> 0xFF apply -> complete posts id 2, firmware
    // svc 82 polls a word buffer: id=2 len=0, then EMPTY:OK on the
    // drained queue. Proves the phase-1 wire end to end (design doc §6).
    // SVC delivery on (deliver_irqs): svc16/82 trap to the SVC handler
    // (vector 11, installed by the image) and return via EXC_RETURN.
    let _u = crate::system::lock_uart();
    crate::system::get_uart_output().lock().unwrap().clear();
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/sd_evt_nrf.bin"));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    // Phase 1: firmware runs svc16 + stages the erase, spins on the
    // mailbox until the driver completes the erase.
    cpu.run(sys, &mut mem, 20_000);
    let base = crate::peripherals::nvmc_nrf::take_erase(sys).expect("erase staged");
    assert_eq!(base, 0x0007_4000, "erase page base");
    for i in 0..4096u32 {
        mem.write8(base.wrapping_add(i), 0xFF);
    }
    crate::peripherals::nvmc_nrf::complete_erase(sys);
    assert_eq!(crate::sd_evt::queue_len(), 1, "flash-success queued");
    mem.write8(0x2000_1000, 1); // mailbox: release the firmware spin
    // Phase 2: firmware svc82-polls the stack buffer, prints markers.
    cpu.run(sys, &mut mem, 2_000_000);
    assert!(cpu.fault.is_none(), "sd_evt faulted: {:?}", cpu.fault);
    let out = crate::system::get_uart_output().lock().unwrap().clone();
    assert!(out.contains("EV:OK"), "missing EV marker, got {out:?}");
    assert!(out.contains("EMPTY:OK"), "missing EMPTY marker, got {out:?}");
    assert!(out.contains("SDEVT:OK"), "missing SDEVT marker, got {out:?}");
    assert_eq!(crate::sd_evt::queue_len(), 0, "queue drained by firmware");
    // 2nd run: fresh boot, no leak (SD disarmed until svc16 again).
    crate::system::reset_globals();
}

#[test]
fn nrf_dma_driver_roundtrip() {
    // P6a firmware (dma_nrf.s, GCC): UARTE TX DMA + TWIM RX DMA + SAADC
    // RESULT DMA, each completed driver-side in phases (take -> RAM move ->
    // complete), exactly like the JS driver will.
    let _u = crate::system::lock_uart();
    let _t = crate::system::lock_i2c_tap();
    crate::system::get_uart_output().lock().unwrap().clear();
    let _g = lock_boot();
    // Virtual accel so the TWIM RX DMA is ACKed (no slave -> NACK, by design).
    {
        use crate::ext_devices::i2c_tap::{I2cTap, I2cTapConfig};
        crate::system::get_ext_devices().lock().unwrap().i2c_taps.push(
            std::rc::Rc::new(std::cell::RefCell::new(I2cTap::new(I2cTapConfig {
                peripheral: "TWIM0".to_string(), address: 0x19,
            }))));
    }
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/dma_nrf.bin"));
    let sys = crate::sys();
    // Phase 1: firmware stages UARTE TX DMA, spins on ENDTX.
    cpu.run(sys, &mut mem, 20_000);
    let (ptr, len) = crate::peripherals::uarte_nrf::take_txdma(sys).expect("uarte staged");
    let bytes: Vec<u8> = (0..len).map(|i| mem.read8(ptr.wrapping_add(i))).collect();
    crate::peripherals::uarte_nrf::complete_txdma(sys, &bytes);
    // Phase 2: firmware stages TWIM RX DMA, spins on ENDRX.
    cpu.run(sys, &mut mem, 20_000);
    let (addr, ptr, len) = crate::peripherals::twim_nrf::take_rxdma(sys, "TWIM0").expect("twim staged");
    assert_eq!(addr, 0x19, "accel address");
    for (i, b) in [0x28u8, 0x00, 0x01].iter().enumerate().take(len as usize) {
        mem.write8(ptr.wrapping_add(i as u32), *b);
    }
    crate::peripherals::twim_nrf::complete_rxdma(sys, "TWIM0", len);
    // Phase 3: firmware stages SAADC RESULT, spins on END.
    cpu.run(sys, &mut mem, 20_000);
    let (ptr, len) = crate::peripherals::saadc_nrf::take_result(sys).expect("saadc staged");
    mem.write8(ptr, 0xAB);
    mem.write8(ptr.wrapping_add(1), 0x02);
    crate::peripherals::saadc_nrf::complete_result(sys, len);
    // Phase 4: drain to done loop.
    cpu.run(sys, &mut mem, 2_000_000);
    assert!(cpu.fault.is_none(), "dma faulted: {:?}", cpu.fault);
    let out = crate::system::get_uart_output().lock().unwrap().clone();
    assert!(out.contains("DMA:OK"), "missing DMA marker, got {out:?}");
    assert!(out.contains("I2C:OK"), "missing I2C marker, got {out:?}");
    assert!(out.contains("ADC:OK"), "missing ADC marker, got {out:?}");
}

/// Driver-style stepping: run small budgets with peripheral ticks between
/// (mirrors the JS driver's step -> tick_peripherals loop; cpu.run alone
/// only advances INSTRUCTION_COUNT, which is enough for polled counters
/// but PPI dispatch lives in tick()).
fn run_with_ticks(cpu: &mut Cpu, mem: &mut FlatMemory, sys: &crate::system::System, total: u32) {
    let mut done = 0;
    while done < total {
        let n = cpu.run(sys, mem, 500);
        sys.tick();
        if n == 0 {
            break;
        }
        done += n;
    }
}

#[test]
fn nrf_air_usb_radio_ppi() {
    // P6b/c firmware (air_nrf.s, GCC): USBRESET (pre-signaled) + RADIO TX
    // looped back through air + PPI TIMER->GPIOTE LED. Ticks interleaved.
    let _u = crate::system::lock_uart();
    crate::system::get_uart_output().lock().unwrap().clear();
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/air_nrf.bin"));
    let sys = crate::sys();
    crate::peripherals::usbd_nrf::signal_usbreset(sys);
    crate::peripherals::radio_nrf::inject_rx(sys, vec![0x01, 0x02, 0x03]);
    // Phase 1: USB + RADIO TX stage + RX END (no ticks needed yet).
    cpu.run(sys, &mut mem, 60_000);
    // Driver moves the TX packet through air (loopback).
    if let Some(_t) = crate::peripherals::radio_nrf::take_tx(sys) {
        crate::peripherals::radio_nrf::inject_rx(sys, vec![0xAA]);
        crate::peripherals::radio_nrf::complete_tx(sys);
    }
    // Driver delivers the queued RX packet to RAM and completes it
    // (END fires only on completion).
    if let Some(ptr) = crate::peripherals::radio_nrf::take_rx(sys) {
        for (i, &b) in [0x01u8, 0x02, 0x03].iter().enumerate() {
            mem.write8(ptr.wrapping_add(i as u32), b);
        }
        crate::peripherals::radio_nrf::complete_rx(sys);
    }
    // Phase 2: PPI/TIMER part with ticks.
    run_with_ticks(&mut cpu, &mut mem, sys, 400_000);
    assert!(cpu.fault.is_none(), "air faulted: {:?}", cpu.fault);
    let out = crate::system::get_uart_output().lock().unwrap().clone();
    assert!(out.contains("USB:OK"), "missing USB marker, got {out:?}");
    assert!(out.contains("RADIO:OK"), "missing RADIO marker, got {out:?}");
    assert!(out.contains("PPI:OK"), "missing PPI marker, got {out:?}");
}

#[test]
fn nrf_ble_conformance_svc_face() {
    // BLE conformance firmware (blinky/ble_fw/ble_conformance.c, GCC):
    // real SVC bytes drive the whole sd_ble face; the native driver
    // resolves every staged job (take_* -> complete_*), exactly like
    // the JS pump + Bumble bridge do live. Markers prove each stage;
    // a 2nd consecutive run proves reset_state leaves no leak.
    use crate::sd_ble::*;
    let _u = crate::system::lock_uart();
    crate::system::get_uart_output().lock().unwrap().clear();
    for run in 0..2 {
        reset_for_test();
        let _g = lock_boot();
        let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/ble_fw/ble_conformance.bin"));
        let sys = crate::sys();
        cpu.deliver_irqs = true;
        // Phase slices: SMALL slices + pump every slice. The firmware
        // waits for CONNECTED inside one spin loop (like silicon
        // firmware); the completion only lands if the driver pumps
        // mid-spin, so 20K slices would starve it (same lesson as the
        // demo's 5x20K pumpDma-in-loop, plan P54).
        for _ in 0..4000 {
            cpu.run(sys, &mut mem, 500);
            if cpu.fault.is_some() { break; }
            let _ = pump_ble_test_driver(sys);
            let out = crate::system::get_uart_output().lock().unwrap().clone();
            if out.contains("BLE:ALL-OK") || out.contains("BLE:SOME-FAIL") { break; }
        }
        assert!(cpu.fault.is_none(), "ble fw faulted (run {run}): {:?}", cpu.fault);
        let out = crate::system::get_uart_output().lock().unwrap().clone();
        for m in ["BLE:BOOT", "BLE:enable:OK", "BLE:service:OK", "BLE:char:OK",
                  "BLE:vset:OK", "BLE:vget:OK", "BLE:connect-stage:OK",
                  "BLE:read-stage:OK", "BLE:prim-stage:OK", "BLE:char-stage:OK",
                  "BLE:rel-stage:OK", "BLE:attrinfo-stage:OK",
                  "BLE:uuidread-stage:OK", "BLE:valsread-stage:OK",
                  "BLE:write-stage:OK", "BLE:scan-stage:OK", "BLE:rssi-stage:OK",
                  "BLE:l2cap-reg:OK", "BLE:l2cap-stage:OK", "BLE:auth-stage:OK",
                  "BLE:disc-stage:OK", "BLE:evt-get:OK", "BLE:ALL-OK"] {
            assert!(out.contains(m), "run {run} missing {m}, got {out:?}");
        }
        crate::system::reset_globals();
    }
}

/// Native test driver for the BLE conformance image: resolve one staged
/// job per call (loopback legibility: battery 87 + fixed table mirroring
/// pumpBleLoopback/tools/ble_air_bridge.py). Returns false when idle.
fn pump_ble_test_driver(sys: &crate::system::System) -> bool {
    use crate::sd_ble::*;
    let _ = sys;
    match take_job() {
        None => false,
        Some(BleJob::GapConnect { addr }) => { complete_gap_connect(addr); true }
        Some(BleJob::GapDisconnect { conn, reason }) => { complete_gap_disconnect(conn, reason); true }
        Some(BleJob::GapRssiGet { conn }) => { complete_rssi(conn, -50); true }
        Some(BleJob::GapScanStart) => {
            post_adv_report([0x11, 0x22, 0x33, 0x44, 0x55, 0x66], -50, false,
                            &[0x02, 0x01, 0x06, 0x03, 0x03, 0x0F, 0x18]);
            true
        }
        Some(BleJob::GattcPrimDisc { conn, .. }) => {
            complete_prim_disc(conn, &[DiscService { uuid16: Some(0x180F), start: 0x10, end: 0x16 }]);
            true
        }
        Some(BleJob::GattcCharDisc { conn, .. }) => {
            complete_char_disc(conn, &[DiscChar { uuid16: Some(0x2A19), props: 0x12, decl: 0x12, value: 0x13 }]);
            true
        }
        Some(BleJob::GattcDescDisc { conn, start, .. }) => {
            complete_desc_disc(conn, &[DiscDesc { handle: start, uuid16: Some(0x2902) }]);
            true
        }
        Some(BleJob::GattcRelDisc { conn, start, .. }) => {
            complete_rel_disc(conn, &[DiscInclude { handle: start, uuid16: Some(0x180F), start, end: start + 6 }]);
            true
        }
        Some(BleJob::GattcAttrInfoDisc { conn, start, .. }) => {
            complete_attr_info_disc(conn, &[DiscAttrInfo { handle: start, uuid16: Some(0x2A19) }]);
            true
        }
        Some(BleJob::GattcUuidRead { conn, .. }) => {
            complete_uuid_read(conn, &[HandleValue { handle: 0x13, value: vec![batt_level()] }]);
            true
        }
        Some(BleJob::GattcValsRead { conn, handles }) => {
            let data = handles.iter().map(|_| batt_level()).collect::<Vec<u8>>();
            complete_vals_read(conn, &data);
            true
        }
        Some(BleJob::GattcRead { conn, handle, offset }) => {
            complete_gattc_read(conn, handle, offset, &[batt_level()]);
            true
        }
        Some(BleJob::GattcWrite { conn, op, handle, data }) => {
            complete_gattc_write(conn, handle, op, &data);
            true
        }
        Some(BleJob::GattsHvx { conn, handle, .. }) => { complete_hvx(conn, handle); true }
        Some(BleJob::GattsServiceChanged { conn, .. }) => { complete_service_changed(conn); true }
        Some(BleJob::L2capTx { conn, cid, data }) => { complete_l2cap_rx(conn, cid, &data); true }
        Some(BleJob::GapAuthenticate { conn }) => { complete_pairing(conn, true); true }
    }
}

#[test]
fn nrf_ble_c_face_markers() {
    // C-language BLE face (demo/parts/ble_lang/c_ble_face.c, same GCC as
    // ble_conformance.c): ENABLE -> CONNECT -> CONNECTED-drain (CENTRAL)
    // -> READ -> READ_RSP=87 drain. Driver pumps between small slices
    // (firmware spins on evt arrival like silicon firmware).
    let _u = crate::system::lock_uart();
    crate::system::get_uart_output().lock().unwrap().clear();
    crate::sd_ble::reset_for_test();
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/ble_fw/c_ble_face.bin"));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    for _ in 0..2000 {
        cpu.run(sys, &mut mem, 500);
        if cpu.fault.is_some() { break; }
        let _ = pump_ble_test_driver(sys);
        let out = crate::system::get_uart_output().lock().unwrap().clone();
        if out.contains("C:ALL-OK") || out.contains("C:SOME-FAIL") { break; }
    }
    assert!(cpu.fault.is_none(), "c face faulted: {:?}", cpu.fault);
    let out = crate::system::get_uart_output().lock().unwrap().clone();
    for m in ["C:BOOT", "C:enable:OK", "C:connect:OK", "C:connected:OK",
              "C:read:OK", "C:rsp:OK", "C:ALL-OK"] {
        assert!(out.contains(m), "missing {m}, got {out:?}");
    }
    crate::system::reset_globals();
}

#[test]
fn nrf_ble_cpp_face_markers() {
    // C++-language BLE face (blinky/ble_fw/ble_cpp_face.cpp, xpack g++,
    // same link_c_nrf.ld as the C images): ENABLE -> CONNECT ->
    // CONNECTED-drain (CENTRAL) -> READ -> READ_RSP=87 drain. Proves
    // the SVC face is language-agnostic at the machine level (C++
    // classes/mangling emit identical SVC bytes). Driver pumps between
    // small slices (firmware spins on evt arrival like silicon).
    let _u = crate::system::lock_uart();
    crate::system::get_uart_output().lock().unwrap().clear();
    crate::sd_ble::reset_for_test();
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/ble_fw/ble_cpp_face.bin"));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    for _ in 0..2000 {
        cpu.run(sys, &mut mem, 500);
        if cpu.fault.is_some() { break; }
        let _ = pump_ble_test_driver(sys);
        let out = crate::system::get_uart_output().lock().unwrap().clone();
        if out.contains("P:ALL-OK") || out.contains("P:SOME-FAIL") { break; }
    }
    assert!(cpu.fault.is_none(), "c++ face faulted: {:?}", cpu.fault);
    let out = crate::system::get_uart_output().lock().unwrap().clone();
    for m in ["P:BOOT", "P:enable:OK", "P:connect:OK", "P:connected:OK",
              "P:read:OK", "P:rsp:OK", "P:ALL-OK"] {
        assert!(out.contains(m), "missing {m}, got {out:?}");
    }
    crate::system::reset_globals();
}

#[test]
fn nrf_ble_pairing_fw_markers() {
    // BLE pairing firmware (blinky/ble_fw/ble_pairing_fw.c, GCC):
    // CODAL-BLE-shaped JustWorks flow — ENABLE -> GATTS battery
    // service+char -> CONNECT(staged, driver completes) ->
    // CONNECTED drain -> PRIM/CHAR/READ/WRITE(staged) ->
    // AUTHENTICATE(staged, driver completes via complete_pairing) ->
    // AUTH_STATUS + SEC_UPDATE drain -> CONN_SEC_GET (encrypted 0x21)
    // -> DISCONNECT(staged) -> DISCONNECTED drain. The native driver
    // resolves every staged job (take_* -> complete_*), exactly like
    // the JS pump + Bumble bridge do live. Markers prove each stage;
    // a 2nd consecutive run proves reset_state leaves no leak.
    use crate::sd_ble::*;
    let _u = crate::system::lock_uart();
    crate::system::get_uart_output().lock().unwrap().clear();
    for run in 0..2 {
        reset_for_test();
        let _g = lock_boot();
        let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/ble_fw/ble_pairing_fw.bin"));
        let sys = crate::sys();
        cpu.deliver_irqs = true;
        // SMALL slices + pump every slice. The firmware spins on evt
        // arrival inside drain_until (like silicon firmware); the
        // completion only lands if the driver pumps mid-spin, so 20K
        // slices would starve it (same lesson as the demo's 5x20K
        // pumpDma-in-loop, plan P54).
        for _ in 0..4000 {
            cpu.run(sys, &mut mem, 500);
            if cpu.fault.is_some() { break; }
            let _ = pump_ble_test_driver(sys);
            let out = crate::system::get_uart_output().lock().unwrap().clone();
            if out.contains("BLEP:ALL-OK") || out.contains("BLEP:SOME-FAIL") { break; }
        }
        assert!(cpu.fault.is_none(), "pairing fw faulted (run {run}): {:?}", cpu.fault);
        let out = crate::system::get_uart_output().lock().unwrap().clone();
        for m in ["BLEP:BOOT", "BLEP:enable:OK", "BLEP:service:OK", "BLEP:char:OK",
                  "BLEP:connect-stage:OK", "BLEP:connected-evt:OK",
                  "BLEP:prim-stage:OK", "BLEP:char-stage:OK",
                  "BLEP:read-stage:OK", "BLEP:write-stage:OK",
                  "BLEP:auth-stage:OK", "BLEP:auth-status:OK",
                  "BLEP:sec-update:OK", "BLEP:conn-sec:OK",
                  "BLEP:disc-stage:OK", "BLEP:disconnected-evt:OK",
                  "BLEP:ALL-OK"] {
            assert!(out.contains(m), "run {run} missing {m}, got {out:?}");
        }
        crate::system::reset_globals();
    }
}

#[test]
fn nrf_ble_roles_fw_markers() {
    // BLE roles firmware (blinky/ble_fw/ble_roles_fw.c, GCC):
    // the ADV/SCAN/whitelist/role-slot legs the pairing image never
    // drives — ENABLE(SC-bit) -> ADV_START(NULL) -> ADV_STOP ->
    // ADV_START(whitelist) -> IN_USE re-arm refuses -> ADV_STOP ->
    // SCAN_START(NULL) -> BUSY re-arm refuses -> SCAN_STOP -> SCAN
    // params (window>interval refuses) -> SCAN selective stages ->
    // ADV whitelist while scan holds it refuses IN_USE -> SCAN_STOP ->
    // CONNECT(staged) -> CONNECTED drain (CENTRAL role) ->
    // SERVICE_CHANGED unsubscribed refuses INVALID_STATE (the CCCD
    // gate, from real firmware bytes) -> DISCONNECT(staged) ->
    // DISCONNECTED drain. The native driver resolves every staged job
    // (take_* -> complete_*), exactly like the JS pump + Bumble bridge
    // do live. Markers prove each stage; a 2nd consecutive run proves
    // reset_state leaves no leak.
    use crate::sd_ble::*;
    let _u = crate::system::lock_uart();
    crate::system::get_uart_output().lock().unwrap().clear();
    for run in 0..2 {
        reset_for_test();
        let _g = lock_boot();
        let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/ble_fw/ble_roles_fw.bin"));
        let sys = crate::sys();
        cpu.deliver_irqs = true;
        // SMALL slices + pump every slice. The firmware spins on evt
        // arrival inside drain_until (like silicon firmware); the
        // completion only lands if the driver pumps mid-spin, so 20K
        // slices would starve it (same lesson as the demo's 5x20K
        // pumpDma-in-loop, plan P54).
        for _ in 0..4000 {
            cpu.run(sys, &mut mem, 500);
            if cpu.fault.is_some() { break; }
            let _ = pump_ble_test_driver(sys);
            let out = crate::system::get_uart_output().lock().unwrap().clone();
            if out.contains("BLER:ALL-OK") || out.contains("BLER:SOME-FAIL") { break; }
        }
        assert!(cpu.fault.is_none(), "roles fw faulted (run {run}): {:?}", cpu.fault);
        let out = crate::system::get_uart_output().lock().unwrap().clone();
        for m in ["BLER:BOOT", "BLER:enable-sc:OK", "BLER:adv-null:OK",
                  "BLER:adv-stop:OK", "BLER:adv-wl:OK", "BLER:adv-inuse:OK",
                  "BLER:adv-stop2:OK", "BLER:scan-null:OK", "BLER:scan-busy:OK",
                  "BLER:scan-stop:OK", "BLER:scan-param:OK", "BLER:scan-sel:OK",
                  "BLER:adv-scan-inuse:OK", "BLER:scan-stop2:OK",
                  "BLER:connect-stage:OK", "BLER:connected-evt:OK",
                  "BLER:role-central:OK", "BLER:sc-gate:OK",
                  "BLER:disc-stage:OK", "BLER:disconnected-evt:OK",
                  "BLER:ALL-OK"] {
            assert!(out.contains(m), "run {run} missing {m}, got {out:?}");
        }
        crate::system::reset_globals();
    }
}

#[test]
fn nrf_c_irq_timer_uart() {
    // P8a firmware (c_irq_nrf.c, GCC -O2): C vector table, TIMER0 IRQ via
    // NVIC delivery (stacking + EXC_RETURN), UARTE prints from thread and
    // handler mode. deliver_irqs=true, driver-style ticks.
    let _u = crate::system::lock_uart();
    crate::system::get_uart_output().lock().unwrap().clear();
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/c_irq_nrf.bin"));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    run_with_ticks(&mut cpu, &mut mem, sys, 800_000);
    assert!(cpu.fault.is_none(), "c-irq faulted: {:?}", cpu.fault);
    let out = crate::system::get_uart_output().lock().unwrap().clone();
    assert!(out.contains("BOOT"), "missing BOOT, got {out:?}");
    assert!(out.contains("TICK:1"), "missing TICK:1, got {out:?}");
    assert!(out.contains("TICK:3"), "missing TICK:3, got {out:?}");
    assert!(out.contains("DONE"), "missing DONE, got {out:?}");
}

#[test]
fn sysresetreq_latches_reboot_request() {
    // AIRCR write with VECTKEY + SYSRESETREQ latches a reboot the driver
    // must honor (MicroPython's MBR/SD handoff resets twice during boot).
    // movw r0,#0xE008 (AIRCR low: VECTKEYSTAT|SREQ); verified by effect.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&[0x00, 0x20, 0x00, 0x20, 0x01, 0x00, 0x00, 0x00]);
    // ldr r0,=0xE000ED0C; ldr r1,=0x05FA0004; str r1,[r0]; b .
    for (i, w) in [0x4802u16, 0x4903, 0x6001, 0xE7FE].iter().enumerate() {
        mem.write16(0x20002000 + i as u32 * 2, *w);
    }
    mem.write32(0x2000200C, 0xE000ED0C);
    mem.write32(0x20002010, 0x05FA0004);
    cpu.regs.r[15] = 0x20002001;
    let sys = crate::sys();
    assert!(!crate::system::is_watchdog_reset_requested(), "clean start");
    cpu.run(sys, &mut mem, 8);
    assert!(crate::system::is_watchdog_reset_requested(), "SYSRESETREQ latched");
    assert!(!crate::system::is_watchdog_reset_requested(), "consumed once");
}

#[test]
fn nrf_usbep_setup_and_epin_dma() {
    // P9a firmware (usbep_nrf.s, GCC): EP0SETUP (host pre-injects
    // GET_DESCRIPTOR) + EPIN0 DMA completed driver-side.
    let _u = crate::system::lock_uart();
    crate::system::get_uart_output().lock().unwrap().clear();
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/usbep_nrf.bin"));
    let sys = crate::sys();
    crate::peripherals::usbd_nrf::signal_usbreset(sys);
    crate::peripherals::usbd_nrf::inject_setup(sys, [0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 0x40, 0x00]);
    // Phase 1: firmware consumes SETUP, stages EPIN0, spins on ENDEPIN0.
    cpu.run(sys, &mut mem, 60_000);
    let (ep, ptr, len) = crate::peripherals::usbd_nrf::take_epin(sys).expect("epin staged");
    assert_eq!((ep, len), (0, 8));
    let bytes: Vec<u8> = (0..len).map(|i| mem.read8(ptr.wrapping_add(i))).collect();
    assert_eq!(bytes, vec![0x12, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x40], "descriptor bytes");
    crate::peripherals::usbd_nrf::complete_epin(sys, ep, &bytes);
    // Phase 2: drain to done loop.
    cpu.run(sys, &mut mem, 2_000_000);
    assert!(cpu.fault.is_none(), "usbep faulted: {:?}", cpu.fault);
    let out = crate::system::get_uart_output().lock().unwrap().clone();
    assert!(out.contains("SETUP:OK"), "missing SETUP marker, got {out:?}");
    assert!(out.contains("USBEP:OK"), "missing USBEP marker, got {out:?}");
}

#[test]
fn nrf_usbdev_c_setup_and_epin_flash_dma() {
    // usbdev_nrf.c (GCC -O2, same recipe as c_irq_nrf.c, verified
    // bit-identical rebuild): USBD ENABLE+PULLUP, EP0SETUP consume,
    // EPIN0 DMA sourced from a .rodata descriptor (flash-to-host path),
    // timeout-bounded polls. Same driver phasing as the asm usbep proof.
    let _u = crate::system::lock_uart();
    crate::system::get_uart_output().lock().unwrap().clear();
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/usbdev_nrf.bin"));
    let sys = crate::sys();
    crate::peripherals::usbd_nrf::signal_usbreset(sys);
    crate::peripherals::usbd_nrf::inject_setup(sys, [0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 0x40, 0x00]);
    // Phase 1: firmware consumes SETUP, stages EPIN0, spins on ENDEPIN0.
    cpu.run(sys, &mut mem, 60_000);
    let (ep, ptr, len) = crate::peripherals::usbd_nrf::take_epin(sys).expect("epin staged");
    assert_eq!((ep, len), (0, 8));
    assert!(ptr < 0x2000_0000, "descriptor must come from flash .rodata, got {ptr:#x}");
    let bytes: Vec<u8> = (0..len).map(|i| mem.read8(ptr.wrapping_add(i))).collect();
    assert_eq!(bytes, vec![0x12, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x40], "descriptor bytes");
    crate::peripherals::usbd_nrf::complete_epin(sys, ep, &bytes);
    // Phase 2: drain to done loop.
    cpu.run(sys, &mut mem, 2_000_000);
    assert!(cpu.fault.is_none(), "usbdev faulted: {:?}", cpu.fault);
    let out = crate::system::get_uart_output().lock().unwrap().clone();
    assert!(out.contains("UBOOT"), "missing UBOOT, got {out:?}");
    assert!(out.contains("SETUP:OK"), "missing SETUP marker, got {out:?}");
    assert!(out.contains("USBEP:OK"), "missing USBEP marker, got {out:?}");
}

#[test]
fn nrf_nfct_field_select_and_frames() {
    // nfct_nrf.s (GCC): ENABLE+SENSE, field pre-presented by the driver,
    // ACTIVATE->SELECTED, TX frame completed driver-side, RX buffer filled
    // driver-side and verified by firmware ('R').
    let _u = crate::system::lock_uart();
    crate::system::get_uart_output().lock().unwrap().clear();
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/nfct_nrf.bin"));
    let sys = crate::sys();
    crate::peripherals::nfct_nrf::nfct_field_present(sys, true);
    // Phase 1: firmware consumes FIELDDETECTED, activates, stages TX.
    cpu.run(sys, &mut mem, 60_000);
    let (ptr, len) = crate::peripherals::nfct_nrf::take_nfct_tx(sys).expect("tx staged");
    let bytes: Vec<u8> = (0..len).map(|i| mem.read8(ptr.wrapping_add(i))).collect();
    assert_eq!(bytes, vec![0xD0, 0x07, 0x86, 0x77], "payload bytes");
    crate::peripherals::nfct_nrf::complete_nfct_tx(sys);
    // Phase 2: firmware repoints at RAM, arms RX, spins on RXFRAMEEND.
    cpu.run(sys, &mut mem, 200_000);
    let (ptr, len) = crate::peripherals::nfct_nrf::take_nfct_rx(sys).expect("rx staged");
    assert_eq!((ptr, len), (0x20001000, 4));
    for (i, &b) in b"RESP".iter().enumerate() {
        mem.write8(ptr.wrapping_add(i as u32), b);
    }
    crate::peripherals::nfct_nrf::complete_nfct_rx(sys, 4);
    // Phase 3: drain to done loop.
    cpu.run(sys, &mut mem, 2_000_000);
    assert!(cpu.fault.is_none(), "nfct faulted: {:?}", cpu.fault);
    let out = crate::system::get_uart_output().lock().unwrap().clone();
    assert!(out.contains("NFC:OK"), "missing NFC marker, got {out:?}");
    assert!(out.contains("NFCT:OK"), "missing NFCT marker, got {out:?}");
}

#[test]
fn nrf_ecb_aes128_fips_vector() {
    // ECB encrypts driver-side (the model has no RAM handle): stage via
    // registers, take dataptr, AES-128 in the driver, write back, complete.
    // FIPS-197 B: key 00..0f, pt 001122..ff -> ct 69c4e0d8...
    use aes::Aes128;
    use cipher::{Block, BlockCipherEncrypt, Key, KeyInit};
    let _g = lock_boot();
    let sys = WasmSystem::new();
    crate::init_for_test(sys);
    let sys = crate::sys();
    let mut mem = FlatMemory::new(512 * 1024, 128 * 1024);
    let key: [u8; 16] = [0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15];
    let pt: [u8; 16] = [0x00,0x11,0x22,0x33,0x44,0x55,0x66,0x77,0x88,0x99,0xaa,0xbb,0xcc,0xdd,0xee,0xff];
    for (i, &b) in key.iter().chain(pt.iter()).enumerate() {
        mem.write8(0x20001000 + i as u32, b);
    }
    sys.p.write(sys, 0x4000E504, 4, 0x20001000); // ECBDATAPTR
    sys.p.write(sys, 0x4000E000, 4, 1);          // STARTECB
    let ptr = crate::peripherals::misc_nrf::take_ecb(sys).expect("ecb staged");
    assert_eq!(ptr, 0x20001000);
    let mut k = [0u8; 16];
    let mut blk = [0u8; 16];
    for i in 0..16 {
        k[i] = mem.read8(ptr.wrapping_add(i as u32));
        blk[i] = mem.read8(ptr.wrapping_add(16 + i as u32));
    }
    let mut block: Block<Aes128> = Block::<Aes128>::try_from(&blk[..]).expect("block len");
    Aes128::new(&Key::<Aes128>::try_from(&k[..]).expect("key len")).encrypt_block(&mut block);
    for (i, &b) in block.iter().enumerate() {
        mem.write8(ptr.wrapping_add(32 + i as u32), b);
    }
    crate::peripherals::misc_nrf::complete_ecb(sys);
    assert_eq!(sys.p.read(sys, 0x4000E100, 4), 1, "ENDECB set");
    let want: [u8; 16] = [0x69,0xc4,0xe0,0xd8,0x6a,0x7b,0x04,0x30,0xd8,0xcd,0xb7,0x80,0x70,0xb4,0xc5,0x5a];
    for (i, &b) in want.iter().enumerate() {
        assert_eq!(mem.read8(ptr.wrapping_add(32 + i as u32)), b, "ct byte {i}");
    }
}

#[test]
fn nrf_wdt_resets_unpetted_firmware() {
    // wdt_nrf.s counts boots in retained RAM, arms the WDT (CRV=1), and
    // spins. Each boot must end in a latched reboot request; RAM proves
    // two consecutive expiries with no petting.
    let _u = crate::system::lock_uart();
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/wdt_nrf.bin"));
    let sys = crate::sys();
    for expected in 1..=2u32 {
        // drain any stale flag, then run until the watchdog fires
        while crate::system::is_watchdog_reset_requested() {}
        let mut fired = false;
        for _ in 0..20 {
            cpu.run(sys, &mut mem, 20_000);
            sys.tick(); // WDT expiry lives in tick()
            if crate::system::is_watchdog_reset_requested() {
                fired = true;
                break;
            }
        }
        assert!(fired, "WDT never fired on boot {expected}");
        assert_eq!(mem.read32(0x20001000), expected, "boot count retained");
        cpu.reset(mem.read32(0x0), mem.read32(0x4));
    }
}

#[test]
fn nrf_i2s_streaming_roundtrip() {
    // i2s_nrf.s (GCC): TX buf filled 0xA0.., ENABLE + RXD/TXD PTR/MAXCNT +
    // START, poll PTRUPDs, park on RAM mailbox 0x20003000, verify RX bytes,
    // STOP, poll STOPPED, print. Driver moves samples both ways + captures TX.
    let _u = crate::system::lock_uart();
    crate::system::get_uart_output().lock().unwrap().clear();
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/i2s_nrf.bin"));
    let sys = crate::sys();
    crate::system::i2s_clear();
    // Phase 1: firmware stages both directions, parks on the mailbox.
    cpu.run(sys, &mut mem, 200_000);
    assert!(cpu.fault.is_none(), "i2s faulted: {:?}", cpu.fault);
    let (ptr, len) = crate::peripherals::misc_nrf::take_i2s_rx(sys).expect("rx staged");
    assert_eq!((ptr, len), (0x20001000, 8));
    for k in 0..len {
        mem.write8(ptr.wrapping_add(k), (0x10 + k) as u8);
    }
    crate::peripherals::misc_nrf::complete_i2s_rx(sys);
    let (ptr, len) = crate::peripherals::misc_nrf::take_i2s_tx(sys).expect("tx staged");
    assert_eq!((ptr, len), (0x20002000, 8));
    let bytes: Vec<u8> = (0..len).map(|k| mem.read8(ptr.wrapping_add(k))).collect();
    assert_eq!(bytes, vec![0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7]);
    crate::peripherals::misc_nrf::complete_i2s_tx(sys, &bytes);
    assert_eq!(crate::system::i2s_take_capture(), bytes, "TX captured");
    // Phase 2: release the mailbox; firmware verifies RX, STOPs, prints.
    mem.write32(0x2000_3000, 1);
    cpu.run(sys, &mut mem, 2_000_000);
    assert!(cpu.fault.is_none(), "i2s faulted: {:?}", cpu.fault);
    let out = crate::system::get_uart_output().lock().unwrap().clone();
    assert!(out.contains("I2S:OK"), "missing I2S marker, got {out:?}");
    crate::system::i2s_clear();
}

#[test]
fn nrf_boot_flash_at_zero() {
    // nRF52833 prove-out: flash at 0x0, FICR constants, CLOCK HFCLK, P0 GPIO.
    // Boot marker + functional marker + 2nd run (no state leak).
    let _g = lock_boot();
    let mut img = vec![0u8; 8];
    img[0..4].copy_from_slice(&0x20002000u32.to_le_bytes());
    img[4..8].copy_from_slice(&0x00000101u32.to_le_bytes());
    let (_cpu, mem) = boot(&img);
    let sys = crate::sys();
    // flash alias check: vector SP visible at both 0x0 and legacy alias
    assert_eq!(mem.read32(0x00000000), 0x20002000);
    // FICR PART = nRF52833
    assert_eq!(sys.p.read(sys, 0x10000100, 4), 0x0005_2833, "FICR PART");
    // CLOCK: start HFCLK -> EVENTS_HFCLKSTARTED + HFCLKRUN
    sys.p.write(sys, 0x40000000, 4, 1);
    assert_eq!(sys.p.read(sys, 0x40000100, 4), 1, "HFCLKSTARTED");
    // P0: DIR output + OUTSET -> readable back (matrix row/col pattern)
    sys.p.write(sys, 0x50000514, 4, 0x1);
    sys.p.write(sys, 0x50000508, 4, 0x1);
    assert_eq!(sys.p.read(sys, 0x50000504, 4) & 1, 1, "P0 OUT");
    // 2nd run: fresh system, no leak. (CLOCK events boot set per P55,
    // so clear-then-check instead of assuming a zero start.)
    let (_cpu2, mem2) = boot(&img);
    let sys2 = crate::sys();
    assert_eq!(mem2.read32(0x00000000), 0x20002000);
    assert_eq!(sys2.p.read(sys2, 0x40000100, 4), 1, "boot HFCLKSTARTED set");
    sys2.p.write(sys2, 0x40000100, 4, 0);
    assert_eq!(sys2.p.read(sys2, 0x40000100, 4), 0, "event clears by write-0");
    assert_eq!(sys2.p.read(sys2, 0x50000504, 4) & 1, 0, "no gpio leak");
}

#[test]
fn exception_svc_roundtrip() {
    let _g = lock_boot();
    // Minimal image in RAM (executable here): main does SVC #0 then loops;
    // SVC handler (vector 11) bumps a counter and returns via EXC_RETURN.
    // Layout: vector table at 0x20000000 is NOT used (CPU vectors come from
    // flash VTOR); instead point VTOR at RAM by writing the model SCB? The
    // model SCB defaults VTOR=0x00000000, so install vectors in flash image.
    let mut img = vec![0u8; 0x200];
    // SP=0x20002000, reset PC=0x00000100
    img[0..4].copy_from_slice(&0x20002000u32.to_le_bytes());
    img[4..8].copy_from_slice(&0x00000100u32.to_le_bytes());
    // SVC vector (11) -> handler at 0x00000110
    img[11 * 4..11 * 4 + 4].copy_from_slice(&0x00000111u32.to_le_bytes());
    // main at 0x100: svc #0 (0xDF00), then b.n loop (0xE7FE)
    img[0x100] = 0x00;
    img[0x101] = 0xDF;
    img[0x102] = 0xFE;
    img[0x103] = 0xE7;
    // handler at 0x110: ldr r0, [pc, #8] (counter addr); ldr r1,[r0]; adds r1,#1;
    // str r1,[r0]; bx lr. Counter at 0x130.
    // 0x110: 4802 (ldr r0,[pc,#8] -> 0x11C); 0x112: 6801 (ldr r1,[r0]); 0x114: 3101 (adds r1,#1)
    // 0x116: 6001 (str r1,[r0]); 0x118: 4770 (bx lr); 0x11A: bf00; 0x11C: 00 01 00 20
    let h: [u8; 16] = [0x02, 0x48, 0x01, 0x68, 0x01, 0x31, 0x01, 0x60, 0x70, 0x47, 0x00, 0xBF, 0x00, 0x01, 0x00, 0x20];
    img[0x110..0x120].copy_from_slice(&h);
    // counter at 0x20001000? use RAM 0x20001000 (in 128K SRAM).
    // patch handler literal to point there:
    img[0x11C..0x120].copy_from_slice(&0x20001000u32.to_le_bytes());
    let (mut cpu, mut mem) = boot(&img);
    // VTOR is 0x00000000 by default: vectors above are in flash image ✓.
    // SP/PC already at reset vector from boot():
    assert_eq!(cpu.regs.r[13], 0x20002000);
    assert_eq!(cpu.regs.r[15] & !1, 0x00000100);
    cpu.deliver_irqs = true;
    let sys = crate::sys();
    cpu.run(sys, &mut mem, 10);
    assert!(cpu.fault.is_none(), "fault: {:?}", cpu.fault);
    // SVC handler should have run exactly once (counter==1) and main resumed
    // into its branch-to-self loop at 0x102.
    assert_eq!(mem.read32(0x20001000), 1, "SVC handler did not run");
    assert_eq!(cpu.regs.r[15] & !1, 0x00000102, "did not resume after SVC");
    assert_eq!(cpu.ipsr, 0, "still in handler mode");
}

#[test]
fn exception_svc_stacks_even_return_pc() {
    // Stacked PC bit 0 must be clear (Thumb travels in xPSR.T): silicon
    // consumers (the MBR SVC dispatcher reads [PC-2] for the SVC number)
    // break when it leaks the internal Thumb bit (see docs/cpu_bug.md).
    let _g = lock_boot();
    let mut img = vec![0u8; 0x200];
    img[0..4].copy_from_slice(&0x20002000u32.to_le_bytes());
    img[4..8].copy_from_slice(&0x00000100u32.to_le_bytes());
    img[11 * 4..11 * 4 + 4].copy_from_slice(&0x00000111u32.to_le_bytes());
    // main at 0x100: svc #0x18 (0xDF18), then b.n loop (0xE7FE)
    img[0x100] = 0x18;
    img[0x101] = 0xDF;
    img[0x102] = 0xFE;
    img[0x103] = 0xE7;
    // handler at 0x110 (GAS-built): mrs r0,MSP; ldr r0,[r0,#24];
    // ldr r1,=0x20001000; str r0,[r1]; bx lr. NOTE: GAS/objdump show
    // halfword VALUES (F3EF = bytes EF,F3 little-endian).
    let h: [u8; 16] = [0xEF, 0xF3, 0x08, 0x80, 0x80, 0x69, 0x01, 0x49,
                       0x08, 0x60, 0x70, 0x47, 0x00, 0x10, 0x00, 0x20];
    img[0x110..0x120].copy_from_slice(&h);
    let (mut cpu, mut mem) = boot(&img);
    cpu.deliver_irqs = true;
    let sys = crate::sys();
    cpu.run(sys, &mut mem, 12);
    assert!(cpu.fault.is_none(), "fault: {:?}", cpu.fault);
    // Return address of the 16-bit SVC at 0x100 is 0x102, bit 0 clear.
    assert_eq!(mem.read32(0x20001000), 0x00000102, "stacked PC has Thumb bit set");
}

fn run_snippet(code: &[u16], regs: &[(usize, u32)]) -> (Cpu, FlatMemory) {
    let _g = lock_boot();
    let sys = WasmSystem::new();
    crate::init_for_test(sys);
    let mut cpu = Cpu::new(0x20002000, 0x20002001);
    let mut mem = FlatMemory::new(0x100000, 0x20000);
    mem.write16(0x20001000, 0xE7FE); // spin landing pad (never executed)
    crate::system::get_uart_output().lock().unwrap().clear();
    for (i, w) in code.iter().enumerate() {
        mem.write16(0x20002000 + (i as u32) * 2, *w);
    }
    for &(r, v) in regs {
        cpu.regs.r[r] = v;
    }
    cpu.regs.r[15] = 0x20002001;
    let sys = crate::sys();
    cpu.run(sys, &mut mem, code.len() as u32 / 2 + 2);
    (cpu, mem)
}

#[test]
fn tbb_index_by_value() {
    // tbb [pc,r3] indexes by r3's VALUE with an unmasked pc+4 base.
    // Table at (pc+4): [0x04 -> case0][0x10 -> case1]; r3=1 -> case1.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    mem.write16(0x20002000, 0xE8DF);
    mem.write16(0x20002002, 0xF003);
    mem.write8(0x20002004, 0x04);
    mem.write8(0x20002005, 0x10);
    cpu.regs.r[3] = 1;
    cpu.regs.r[15] = 0x20002001;
    let sys = crate::sys();
    cpu.run(sys, &mut mem, 1);
    assert_eq!(cpu.regs.r[15] & !1, 0x20002024);
}

#[test]
fn sdiv_plain_and_it() {
    // sdiv r1,r1,r3 (FB91 F1F3): plain, IT-taken, IT-skipped (sentinel kept).
    let (mut cpu, _) = run_snippet(&[0xFB91, 0xF1F3], &[(1, 1680), (3, 10)]);
    assert_eq!(cpu.regs.r[1], 168);
    // cmp r1,#11 (NE, r1=1680) ; ite gt (BFCC) ; sdivne (taken: 168)
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    let sys = crate::sys();
    for (i, w) in [0x290Bu16, 0xBFCC, 0xFB91, 0xF1F3].iter().enumerate() {
        mem.write16(0x20002000 + i as u32 * 2, *w);
    }
    cpu.regs.r[1] = 1680;
    cpu.regs.r[3] = 10;
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 4); // cmp,it,sdiv(taken) + 1 nop to consume ite slot2
    assert_eq!(cpu.regs.r[1], 168);
    // EQ: cmp r1,#11 (r1=11) ; ite gt ; sdivne must NOT run
    cpu.regs.r[1] = 11;
    cpu.regs.r[3] = 10;
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 3);
    // sdivne skipped -> r1 stays 11
    assert_eq!(cpu.regs.r[1], 11);
}

#[test]
fn tst_sets_flags_without_writeback() {
    // GAS: tst r0,r1 = 0x4208 (sop 8), cmp r0,r1 = 0x4288 (sop 10).
    // Regression 2026-09-11: sop 8 ran CMP (sub_flags). TST must AND.
    // Flag read-out is via beq (D000 skips the movs when Z=1): the snippet
    // harness always executes a trailing zero halfword (flag-setting lsls),
    // so XPSR cannot be asserted directly.
    // Z=0 case: 0x200000 & 0x200000 != 0 (CMP would set Z here).
    let (cpu, _) = run_snippet(&[0x4208, 0xD000, 0x2201], &[(0, 0x200000), (1, 0x200000), (2, 0)]);
    assert_eq!(cpu.regs.r[0], 0x200000, "TST must not write Rd");
    assert_eq!(cpu.regs.r[2], 1, "beq not taken: Z clear");
    // Z=1 case: disjoint bits AND to zero.
    let (cpu, _) = run_snippet(&[0x4208, 0xD000, 0x2201], &[(0, 0xF0), (1, 0x0F), (2, 0)]);
    assert_eq!(cpu.regs.r[2], 0, "beq taken: Z set");
    // CMP control: equal operands set Z (beq taken).
    let (cpu, _) = run_snippet(&[0x4288, 0xD000, 0x2201], &[(0, 5), (1, 5), (2, 0)]);
    assert_eq!(cpu.regs.r[2], 0, "CMP equal: Z set");
}

#[test]
fn usat_ssat_q() {
    let (mut cpu, _) = run_snippet(&[0xF380, 0x0005], &[(0, 100)]);
    assert_eq!(cpu.regs.r[0], 31);
    assert_ne!(cpu.regs.xpsr & 0x08000000, 0);
    let (mut cpu, _) = run_snippet(&[0xF380, 0x0005], &[(0, 20)]);
    assert_eq!(cpu.regs.r[0], 20);
    assert_eq!(cpu.regs.xpsr & 0x08000000, 0);
    // SSAT sat field encodes N-1 (ssat#8 = o2 0x0007)
    let (mut cpu, _) = run_snippet(&[0xF300, 0x0007], &[(0, 1000)]);
    assert_eq!(cpu.regs.r[0], 127);
    assert_ne!(cpu.regs.xpsr & 0x08000000, 0);
    let (mut cpu, _) = run_snippet(&[0xF300, 0x0007], &[(0, 0xFFFFFC18)]);
    assert_eq!(cpu.regs.r[0], 0xFFFFFF80);
    assert_ne!(cpu.regs.xpsr & 0x08000000, 0);
}

#[test]
fn smlald_dual_add_long() {
    // smlald r0,r1,r2,r3 = fbc2 01c3: acc += lo*lo + hi*hi (signed).
    let (mut cpu, _) = run_snippet(
        &[0xFBC2, 0x01C3],
        &[(0, 0), (1, 0), (2, 0x00020001), (3, 0x00040003)],
    );
    assert_eq!(cpu.regs.r[0], 11); // 1*3 + 2*4
    assert_eq!(cpu.regs.r[1], 0);
    // Mixed signs: 1*1 + (-1)*1 = 0.
    let (mut cpu, _) = run_snippet(
        &[0xFBC2, 0x01C3],
        &[(0, 0), (1, 0), (2, 0xFFFF0001), (3, 0x00010001)],
    );
    assert_eq!(cpu.regs.r[0], 0);
    assert_eq!(cpu.regs.r[1], 0);
}

#[test]
fn smlsld_dual_sub_long() {
    // smlsld r0,r1,r2,r3 = fbd2 01c3: acc += lo*lo - hi*hi.
    // 1*3 - 2*4 = -5.
    let (mut cpu, _) = run_snippet(
        &[0xFBD2, 0x01C3],
        &[(0, 0), (1, 0), (2, 0x00020001), (3, 0x00040003)],
    );
    assert_eq!(cpu.regs.r[0], 0xFFFFFFFB);
    assert_eq!(cpu.regs.r[1], 0xFFFFFFFF);
}

#[test]
fn umaal_dual_accumulate() {
    // umaal r4,r5,r6,r7 = fbe6 4567: acc += Rn*Rm + RdLo + RdHi.
    // (2^32-1)^2 + 1 + 2 = 0xFFFFFFFE00000004.
    let (mut cpu, _) = run_snippet(
        &[0xFBE6, 0x4567],
        &[(4, 1), (5, 2), (6, 0xFFFFFFFF), (7, 0xFFFFFFFF)],
    );
    assert_eq!(cpu.regs.r[4], 4);
    assert_eq!(cpu.regs.r[5], 0xFFFFFFFE);
    // Small: 2*3 + 10 + 0 = 16 (exercises both Rd adds).
    let (mut cpu, _) = run_snippet(
        &[0xFBE6, 0x4567],
        &[(4, 10), (5, 0), (6, 2), (7, 3)],
    );
    assert_eq!(cpu.regs.r[4], 16);
    assert_eq!(cpu.regs.r[5], 0);
}

#[test]
fn smmul_rounding() {
    // smmul r0,r1,r2 = fb51 f002: top32(prod). 0x40000000^2 top = 0x10000000.
    let (mut cpu, _) = run_snippet(&[0xFB51, 0xF002], &[(1, 0x40000000), (2, 0x40000000)]);
    assert_eq!(cpu.regs.r[0], 0x10000000);
    // smmulr (fb51 f012) rounds: (-2^31)*(-1) = 2^31 -> top 0...
    // prod = 0x80000000, +0x80000000 = 0x100000000 -> top 1.
    let (mut cpu, _) = run_snippet(&[0xFB51, 0xF012], &[(1, 0x80000000), (2, 0xFFFFFFFF)]);
    assert_eq!(cpu.regs.r[0], 1);
    let (mut cpu, _) = run_snippet(&[0xFB51, 0xF002], &[(1, 0x80000000), (2, 0xFFFFFFFF)]);
    assert_eq!(cpu.regs.r[0], 0);
    // smmla r0,r1,r2,r3 = fb51 3002: Ra + top.
    let (mut cpu, _) = run_snippet(
        &[0xFB51, 0x3002],
        &[(1, 0x40000000), (2, 0x40000000), (3, 5)],
    );
    assert_eq!(cpu.regs.r[0], 0x10000005);
    // smmlar (fb51 3012) vs smmla on the rounding edge (prod = 0x80000000).
    let (mut cpu, _) = run_snippet(
        &[0xFB51, 0x3012],
        &[(1, 0x80000000), (2, 0xFFFFFFFF), (3, 7)],
    );
    assert_eq!(cpu.regs.r[0], 8);
    let (mut cpu, _) = run_snippet(
        &[0xFB51, 0x3002],
        &[(1, 0x80000000), (2, 0xFFFFFFFF), (3, 7)],
    );
    assert_eq!(cpu.regs.r[0], 7);
    // smmls r0,r1,r2,r3 = fb61 3002: Ra - top.
    let (mut cpu, _) = run_snippet(
        &[0xFB61, 0x3002],
        &[(1, 0x40000000), (2, 0x40000000), (3, 5)],
    );
    assert_eq!(cpu.regs.r[0], 0xF0000005);
    // smmlsr (fb61 3012): 7 - 1 = 6 with rounding, 7 - 0 = 7 without.
    let (mut cpu, _) = run_snippet(
        &[0xFB61, 0x3012],
        &[(1, 0x80000000), (2, 0xFFFFFFFF), (3, 7)],
    );
    assert_eq!(cpu.regs.r[0], 6);
    let (mut cpu, _) = run_snippet(
        &[0xFB61, 0x3002],
        &[(1, 0x80000000), (2, 0xFFFFFFFF), (3, 7)],
    );
    assert_eq!(cpu.regs.r[0], 7);
}

#[test]
fn usad8_accumulate() {
    // usad8 r0,r1,r2 = fb71 f002: |1-4|+|2-3|+|3-2|+|4-1| = 8.
    let (mut cpu, _) = run_snippet(&[0xFB71, 0xF002], &[(1, 0x01020304), (2, 0x04030201)]);
    assert_eq!(cpu.regs.r[0], 8);
    // usada8 r0,r1,r2,r3 = fb71 3002: + Ra.
    let (mut cpu, _) = run_snippet(
        &[0xFB71, 0x3002],
        &[(1, 0x01020304), (2, 0x04030201), (3, 100)],
    );
    assert_eq!(cpu.regs.r[0], 108);
}

#[test]
fn parallel_qadd_qsub() {
    // qadd8 r0,r1,r2 = fa81 f012: 0x7F+1 saturates per lane + Q.
    let (mut cpu, _) = run_snippet(&[0xFA81, 0xF012], &[(1, 0x7F7F7F7F), (2, 0x01010101)]);
    assert_eq!(cpu.regs.r[0], 0x7F7F7F7F);
    assert_ne!(cpu.regs.xpsr & 0x08000000, 0);
    // No saturation: exact + Q clear.
    let (mut cpu, _) = run_snippet(&[0xFA81, 0xF012], &[(1, 0x01010101), (2, 0x02020202)]);
    assert_eq!(cpu.regs.r[0], 0x03030303);
    assert_eq!(cpu.regs.xpsr & 0x08000000, 0);
    // qsub8 r0,r1,r2 = fac1 f012: -128-1 saturates to -128 + Q.
    let (mut cpu, _) = run_snippet(&[0xFAC1, 0xF012], &[(1, 0x80808080), (2, 0x01010101)]);
    assert_eq!(cpu.regs.r[0], 0x80808080);
    assert_ne!(cpu.regs.xpsr & 0x08000000, 0);
    // qadd16 r0,r1,r2 = fa91 f012: 0x7FFF+1 saturates + Q.
    let (mut cpu, _) = run_snippet(&[0xFA91, 0xF012], &[(1, 0x7FFF7FFF), (2, 0x00010001)]);
    assert_eq!(cpu.regs.r[0], 0x7FFF7FFF);
    assert_ne!(cpu.regs.xpsr & 0x08000000, 0);
    // qsub16 r0,r1,r2 = fad1 f012: 0x8000-1 saturates to 0x8000 + Q.
    let (mut cpu, _) = run_snippet(&[0xFAD1, 0xF012], &[(1, 0x80008000), (2, 0x00010001)]);
    assert_eq!(cpu.regs.r[0], 0x80008000);
    assert_ne!(cpu.regs.xpsr & 0x08000000, 0);
    // uqadd16 r0,r1,r2 = fa91 f052: 0xFFFF+1 saturates + Q (lo exact).
    let (mut cpu, _) = run_snippet(&[0xFA91, 0xF052], &[(1, 0xFFFF0001), (2, 0x00010000)]);
    assert_eq!(cpu.regs.r[0], 0xFFFF0001);
    assert_ne!(cpu.regs.xpsr & 0x08000000, 0);
    // uqsub16 r0,r1,r2 = fad1 f052: underflow saturates to 0 + Q.
    let (mut cpu, _) = run_snippet(&[0xFAD1, 0xF052], &[(1, 0x00010000), (2, 0x00020001)]);
    assert_eq!(cpu.regs.r[0], 0x00000000);
    assert_ne!(cpu.regs.xpsr & 0x08000000, 0);
    // uqadd8/uqsub8 (fa81/fac1 f052).
    let (mut cpu, _) = run_snippet(&[0xFA81, 0xF052], &[(1, 0xFF00FF00), (2, 0x01000100)]);
    assert_eq!(cpu.regs.r[0], 0xFF00FF00);
    assert_ne!(cpu.regs.xpsr & 0x08000000, 0);
    let (mut cpu, _) = run_snippet(&[0xFAC1, 0xF052], &[(1, 0x01000100), (2, 0x02000200)]);
    assert_eq!(cpu.regs.r[0], 0x00000000);
    assert_ne!(cpu.regs.xpsr & 0x08000000, 0);
}

#[test]
fn parallel_halving() {
    // shadd8 r0,r1,r2 = fa81 f022: (2+4)>>1 = 3, no Q.
    let (mut cpu, _) = run_snippet(&[0xFA81, 0xF022], &[(1, 0x02020202), (2, 0x04040404)]);
    assert_eq!(cpu.regs.r[0], 0x03030303);
    assert_eq!(cpu.regs.xpsr & 0x08000000, 0);
    // Arithmetic shift keeps sign: (-2 + -4)>>1 = -3.
    let (mut cpu, _) = run_snippet(&[0xFA81, 0xF022], &[(1, 0xFEFEFEFE), (2, 0xFCFCFCFC)]);
    assert_eq!(cpu.regs.r[0], 0xFDFDFDFD);
    assert_eq!(cpu.regs.xpsr & 0x08000000, 0);
    // shsub8 r0,r1,r2 = fac1 f022: (4-2)>>1 = 1.
    let (mut cpu, _) = run_snippet(&[0xFAC1, 0xF022], &[(1, 0x04040404), (2, 0x02020202)]);
    assert_eq!(cpu.regs.r[0], 0x01010101);
    // uhadd8 r0,r1,r2 = fa81 f062: (255+255)>>1 = 255 (logical).
    let (mut cpu, _) = run_snippet(&[0xFA81, 0xF062], &[(1, 0xFFFFFFFF), (2, 0xFFFFFFFF)]);
    assert_eq!(cpu.regs.r[0], 0xFFFFFFFF);
    // uhsub8 r0,r1,r2 = fac1 f062: (4-6)>>1 logical = 0x7FFFFFFF[lane] = 0xFF.
    let (mut cpu, _) = run_snippet(&[0xFAC1, 0xF062], &[(1, 0x04040404), (2, 0x06060606)]);
    assert_eq!(cpu.regs.r[0], 0xFFFFFFFF);
    // shadd16 r0,r1,r2 = fa91 f022: (2+6)>>1=4, (4+8)>>1=6.
    let (mut cpu, _) = run_snippet(&[0xFA91, 0xF022], &[(1, 0x00020004), (2, 0x00060008)]);
    assert_eq!(cpu.regs.r[0], 0x00040006);
    // shsub16 r0,r1,r2 = fad1 f022.
    let (mut cpu, _) = run_snippet(&[0xFAD1, 0xF022], &[(1, 0x00040006), (2, 0x00020004)]);
    assert_eq!(cpu.regs.r[0], 0x00010001);
    // uhadd16 r0,r1,r2 = fa91 f062: (0xFFFE+4)>>1 = 0x8001, (2+6)>>1 = 4.
    let (mut cpu, _) = run_snippet(&[0xFA91, 0xF062], &[(1, 0xFFFE0002), (2, 0x00040006)]);
    assert_eq!(cpu.regs.r[0], 0x80010004);
    // uhsub16 r0,r1,r2 = fad1 f062: (4-2)>>1 = 1, (6-8)>>1 logical = 0xFFFF.
    let (mut cpu, _) = run_snippet(&[0xFAD1, 0xF062], &[(1, 0x00040006), (2, 0x00020008)]);
    assert_eq!(cpu.regs.r[0], 0x0001FFFF);
}

#[test]
fn parallel_asx_sax() {
    // qasx r0,r1,r2 = faa1 f012: top = hi+lo, bottom = lo-hi.
    // Rn=0x00020001, Rm=0x00040003 -> top 2+3=5, bot 1-4=-3.
    let (mut cpu, _) = run_snippet(&[0xFAA1, 0xF012], &[(1, 0x00020001), (2, 0x00040003)]);
    assert_eq!(cpu.regs.r[0], 0x0005FFFD);
    assert_eq!(cpu.regs.xpsr & 0x08000000, 0);
    // Saturating: top 0x7FFF+1 -> 0x7FFF + Q; bot 1-0x7FFF fits.
    let (mut cpu, _) = run_snippet(&[0xFAA1, 0xF012], &[(1, 0x7FFF0001), (2, 0x7FFF0001)]);
    assert_eq!(cpu.regs.r[0], 0x7FFF8002);
    assert_ne!(cpu.regs.xpsr & 0x08000000, 0);
    // qsax r0,r1,r2 = fae1 f012: top = hi-lo, bot = lo+hi.
    let (mut cpu, _) = run_snippet(&[0xFAE1, 0xF012], &[(1, 0x00020001), (2, 0x00040003)]);
    assert_eq!(cpu.regs.r[0], 0xFFFF0005);
    // uqasx r0,r1,r2 = faa1 f052: top 0xFFFF+0xFFFF saturates + Q.
    let (mut cpu, _) = run_snippet(&[0xFAA1, 0xF052], &[(1, 0xFFFF0001), (2, 0x0001FFFF)]);
    assert_eq!(cpu.regs.r[0], 0xFFFF0000);
    assert_ne!(cpu.regs.xpsr & 0x08000000, 0);
    // uqsax r0,r1,r2 = fae1 f052: bot 2+0xFFFF saturates + Q.
    let (mut cpu, _) = run_snippet(&[0xFAE1, 0xF052], &[(1, 0x00010002), (2, 0xFFFF0001)]);
    assert_eq!(cpu.regs.r[0], 0x0000FFFF);
    assert_ne!(cpu.regs.xpsr & 0x08000000, 0);
    // shasx r0,r1,r2 = faa1 f022: top (2+8)>>1=5, bot (4-6)>>1=-1.
    let (mut cpu, _) = run_snippet(&[0xFAA1, 0xF022], &[(1, 0x00020004), (2, 0x00060008)]);
    assert_eq!(cpu.regs.r[0], 0x0005FFFF);
    // shsax r0,r1,r2 = fae1 f022: top (2-8)>>1=-3, bot (4+6)>>1=5.
    // (Rm halves exchange: top uses Rm.lo=8, bottom uses Rm.hi=6.)
    let (mut cpu, _) = run_snippet(&[0xFAE1, 0xF022], &[(1, 0x00020004), (2, 0x00060008)]);
    assert_eq!(cpu.regs.r[0], 0xFFFD0005);
    // uhasx r0,r1,r2 = faa1 f062: top (2+8)>>1=5, bot (4-6)>>1 logical=0xFFFF.
    let (mut cpu, _) = run_snippet(&[0xFAA1, 0xF062], &[(1, 0x00020004), (2, 0x00060008)]);
    assert_eq!(cpu.regs.r[0], 0x0005FFFF);
    // uhsax r0,r1,r2 = fae1 f062: top (2-6)>>1 logical=0xFFFE,
    // bot (4+8)>>1=6.
    let (mut cpu, _) = run_snippet(&[0xFAE1, 0xF062], &[(1, 0x00020004), (2, 0x00080006)]);
    assert_eq!(cpu.regs.r[0], 0xFFFE0006);
}

#[test]
fn sxtab16_uxtab16() {    // sxtab16 r0,r1,r2 = fa21 f082: lo = 2+SXTH(4), hi = 1+SXTH(3).
    let (mut cpu, _) = run_snippet(&[0xFA21, 0xF082], &[(1, 0x00010002), (2, 0x00030004)]);
    assert_eq!(cpu.regs.r[0], 0x00040006);
    // ror #8 first: Rm=0x04000300 ror 8 = 0x00040003.
    let (mut cpu, _) = run_snippet(&[0xFA21, 0xF092], &[(1, 0x00010002), (2, 0x04000300)]);
    assert_eq!(cpu.regs.r[0], 0x00050005);
    // uxtab16 r0,r1,r2 = fa31 f082: zero-extend.
    let (mut cpu, _) = run_snippet(&[0xFA31, 0xF082], &[(1, 0x00010002), (2, 0x00FF00FE)]);
    assert_eq!(cpu.regs.r[0], 0x01000100);
}

#[test]
fn shift_reg_flag_setting() {
    // lsls.w r0,r1,r2 = fa11 f002.
    let (mut cpu, _) = run_snippet(&[0xFA11, 0xF002], &[(1, 1), (2, 3)]);
    assert_eq!(cpu.regs.r[0], 8);
    assert_eq!(cpu.regs.xpsr & 0xE0000000, 0);
    // Carry out of bit 31.
    let (mut cpu, _) = run_snippet(&[0xFA11, 0xF002], &[(1, 0x80000000), (2, 1)]);
    assert_eq!(cpu.regs.r[0], 0);
    assert_ne!(cpu.regs.xpsr & 0x60000000, 0); // Z=1, C=1
    // lsrs.w r0,r1,r2 = fa31 f002: 1>>1 = 0, C=1 (bit 0 out), Z=1.
    let (mut cpu, _) = run_snippet(&[0xFA31, 0xF002], &[(1, 1), (2, 1)]);
    assert_eq!(cpu.regs.r[0], 0);
    assert_ne!(cpu.regs.xpsr & 0x60000000, 0);
    // asrs.w r0,r1,r2 = fa51 f002: 0x80000000>>4 arithmetic.
    let (mut cpu, _) = run_snippet(&[0xFA51, 0xF002], &[(1, 0x80000000), (2, 4)]);
    assert_eq!(cpu.regs.r[0], 0xF8000000);
    assert_ne!(cpu.regs.xpsr & 0x80000000, 0); // N=1
    // rors.w r0,r1,r2 = fa71 f002: ror(1, 1) = 0x80000000, C=1.
    let (mut cpu, _) = run_snippet(&[0xFA71, 0xF002], &[(1, 1), (2, 1)]);
    assert_eq!(cpu.regs.r[0], 0x80000000);
    assert_ne!(cpu.regs.xpsr & 0xA0000000, 0); // N=1, C=1
}

#[test]
fn ldrex_strex_sizes() {
    // Byte/halfword/word exclusives; single-threaded: STREX always 0.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    mem.write8(0x20003000, 0xAB);
    mem.write16(0x20003010, 0xCDEF);
    mem.write32(0x20003020, 0x12345678);
    // ldrexb r0,[r1] = e8d1 0f4f ; ldrexh r2,[r3] = e8d3 2f5f
    // (Rt = o2[15:12]).
    mem.write16(0x20002000, 0xE8D1);
    mem.write16(0x20002002, 0x0F4F);
    mem.write16(0x20002004, 0xE8D3);
    mem.write16(0x20002006, 0x2F5F);
    cpu.regs.r[1] = 0x20003000;
    cpu.regs.r[3] = 0x20003010;
    cpu.regs.r[15] = 0x20002001;
    let sys = crate::sys();
    cpu.run(sys, &mut mem, 2);
    assert_eq!(cpu.regs.r[0], 0xAB);
    assert_eq!(cpu.regs.r[2], 0xCDEF);
    // strexb r4,r5,[r6] = e8c6 5f44 ; strexh r7,r8,[r9] = e8c9 8f57
    // (o2 = Rt:F:size:Rd).
    mem.write16(0x20002000, 0xE8C6);
    mem.write16(0x20002002, 0x5F44);
    mem.write16(0x20002004, 0xE8C9);
    mem.write16(0x20002006, 0x8F57);
    cpu.regs.r[5] = 0x12;
    cpu.regs.r[6] = 0x20003000;
    cpu.regs.r[8] = 0x3456;
    cpu.regs.r[9] = 0x20003010;
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 2);
    assert_eq!(mem.read8(0x20003000), 0x12);
    assert_eq!(cpu.regs.r[4], 0);
    assert_eq!(mem.read16(0x20003010), 0x3456);
    assert_eq!(cpu.regs.r[7], 0);
    // Word forms: ldrex r0,[r1] = e851 0f00 ;
    // strex r2,r3,[r4,#8] = e844 3208 (word o1 nibble is 0x084x, NOT 0x08Cx;
    // o2 = Rt:Rd:imm8, imm scaled x4).
    // (Reload the word source: the strexb above wrote 0x12 to 0x20003000.)
    mem.write32(0x20003000, 0x12345678);
    mem.write16(0x20002000, 0xE851);
    mem.write16(0x20002002, 0x0F00);
    mem.write16(0x20002004, 0xE844);
    mem.write16(0x20002006, 0x3208);
    cpu.regs.r[1] = 0x20003000;
    cpu.regs.r[3] = 0xDEADBEEF;
    cpu.regs.r[4] = 0x20003000;
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 2);
    assert_eq!(cpu.regs.r[0], 0x12345678);
    assert_eq!(mem.read32(0x20003020), 0xDEADBEEF);
    assert_eq!(cpu.regs.r[2], 0);
    // Word LDREX with an offset whose imm8 hits the B/H size nibbles
    // (imm8 0x40 -> [7:4]==4): still a word load, addr scaled x4.
    // ldrex r5,[r6,#0x100] = e856 5f40 (nibble stays 0x0850).
    mem.write32(0x20003100, 0xA5A5A5A5);
    mem.write16(0x20002000, 0xE856);
    mem.write16(0x20002002, 0x5F40);
    cpu.regs.r[6] = 0x20003000;
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 1);
    assert_eq!(cpu.regs.r[5], 0xA5A5A5A5);
}

#[test]
fn addw_subw_plain_imm() {
    let (mut cpu, _) = run_snippet(&[0xF20A, 0x46BC], &[(10, 100)]);
    assert_eq!(cpu.regs.r[6], 100 + 1212);
    let (mut cpu, _) = run_snippet(&[0xF2AA, 0x46BC], &[(10, 100)]);
    assert_eq!(cpu.regs.r[6], (100i32 - 1212) as u32);
    let (mut cpu, _) = run_snippet(&[0xF6A1, 0x71FF], &[(1, 5000)]);
    assert_eq!(cpu.regs.r[1], 5000 - 4095);
}

#[test]
fn t3_reg_no_writeback() {
    // strh.w r2,[r9,r3,lsl#1] (F829 2013) must not write back Rn/Rm.
    let (mut cpu, mem) = run_snippet(&[0xF829, 0x2013], &[(9, 0x20003000), (3, 5), (2, 0xABCD)]);
    assert_eq!(mem.read16(0x2000300A), 0xABCD);
    assert_eq!(cpu.regs.r[9], 0x20003000);
    assert_eq!(cpu.regs.r[3], 5);
}

#[test]
fn it_pred_mov_preserves() {
    // D_PageTicker: cmp sets N=1; itt lt; movlt (taken) must preserve N
    // so strlt (LT) also takes. Unpredicated movs still sets N/Z.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    let sys = crate::sys();
    // cmp r3,#0 (r3=-20) ; itt lt (BFBC) ; movlt r3,#1 ; strlt r3,[r2]
    for (i, w) in [0x2B03u16, 0xBFBC, 0x2301, 0x6013].iter().enumerate() {
        mem.write16(0x20002000 + i as u32 * 2, *w);
    }
    cpu.regs.r[3] = 0xFFFFFFEC;
    cpu.regs.r[2] = 0x20003000;
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 4);
    assert_eq!(cpu.regs.r[3], 1);
    assert_eq!(mem.read32(0x20003000), 1);
    assert_eq!((cpu.regs.xpsr >> 31) & 1, 1, "N preserved through predicated movs");
}

#[test]
fn bare_movs_sets_nz_preserves_c() {
    // cmp r2,#1 (r2=-6: C=1) ; movs r0,#0 -> N=0,Z=1,C stays 1
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    let sys = crate::sys();
    for (i, w) in [0x2A01u16, 0x2000].iter().enumerate() {
        mem.write16(0x20002000 + i as u32 * 2, *w);
    }
    cpu.regs.r[2] = 0xFFFFFFFA;
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 2);
    let x = cpu.regs.xpsr;
    assert_eq!(cpu.regs.r[0], 0);
    assert_eq!((x >> 31) & 1, 0, "N");
    assert_eq!((x >> 30) & 1, 1, "Z");
    assert_eq!((x >> 29) & 1, 1, "C preserved");
}

#[test]
fn it_pred_add_preserves() {
    // cmp r3,#0 (r3=-5, N=1) ; itt mi (BF? mask C cond MI=4: 0xBFC4) ;
    // addmi r6,r6,r3 (taken, r6=0+5, N stays 1)
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    let sys = crate::sys();
    for (i, w) in [0x2B00u16, 0xBF44, 0x18F6].iter().enumerate() {
        mem.write16(0x20002000 + i as u32 * 2, *w);
    }
    cpu.regs.r[3] = 0xFFFFFFFB;
    cpu.regs.r[6] = 0;
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 3);
    assert_eq!(cpu.regs.r[6], 0xFFFFFFFB);
    assert_eq!((cpu.regs.xpsr >> 31) & 1, 1, "N preserved through predicated add");
}

#[test]
fn bare_subreg_sets_flags() {
    // subs r3,r3,r0 (1A1B) unpredicated with equal inputs -> Z=1.
    // Run EXACTLY 1 step: run_snippet's trailing NOPs (movs r0,r0) would
    // clobber Z and mask the assertion.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    let sys = crate::sys();
    mem.write16(0x20002000, 0x1A1B);
    cpu.regs.r[3] = 0x64;
    cpu.regs.r[0] = 0x64;
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 1);
    assert_eq!(cpu.regs.r[3], 0);
    assert_eq!((cpu.regs.xpsr >> 30) & 1, 1, "Z");
}

#[test]
fn ldrsh_reg_sx() {
    // ldrsh.w r2,[r0,r3,lsl#2] (F930 2023): signed halfword, no writeback.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    let sys = crate::sys();
    mem.write16(0x20002000, 0xF930);
    mem.write16(0x20002002, 0x2023);
    mem.write16(0x20003000, 0xFF80); // -128
    cpu.regs.r[0] = 0x20003000;
    cpu.regs.r[3] = 0;
    cpu.regs.r[9] = 0x20003000;
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 1);
    assert_eq!(cpu.regs.r[2], 0xFFFFFF80);
    assert_eq!(cpu.regs.r[0], 0x20003000, "no writeback to Rn");
}

#[test]
fn cmp13_n_flag() {
    // cmp r3,#3 with r3=1 -> N=1,Z=0,C=0,V=0 (S_Start's LE depends on N).
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    let sys = crate::sys();
    mem.write16(0x20002000, 0x2B03);
    cpu.regs.r[3] = 1;
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 1);
    let x = cpu.regs.xpsr;
    assert_eq!((x >> 31) & 1, 1, "N");
    assert_eq!((x >> 30) & 1, 0, "Z");
    assert_eq!((x >> 29) & 1, 0, "C");
    assert_eq!((x >> 28) & 1, 0, "V");
}

#[test]
fn lsr_reg_zero_noop() {
    // LSRS-reg with Rs==0 is a no-op (result + carry preserved); the
    // immediate-#0-means-32 rule must NOT apply. DOOM's `(v >> (i*8))`
    // nibble/byte extracts with i==0 returned 0 (patch id 0x120).
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    let sys = crate::sys();
    // lsrs r2, r3, r1 (FA T2 LSR-reg: check exact encoding via GAS? use
    // 16-bit T1 LSRS-reg: 000100_xxxx? T1 LSR-reg = 010000_0010_Rm_Rd
    mem.write16(0x20002000, 0x408B); // lsrs r3, r1? (0100000010100011: Rm=1,Rd=3)
    cpu.regs.r[3] = 0x12345678;
    cpu.regs.r[1] = 0;
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 1);
    assert_eq!(cpu.regs.r[3], 0x12345678);
}

#[test]
fn fpu_mvfr_and_cpacr_reset() {
    // M4F ID values (M4F TRM) + CPACR/FPSCR/S-file reset state. Grounds the
    // FPU bring-up: guests probe MVFR0-2 at 0xE000EF40-48 and enable CP10/11
    // via CPACR before the first VFP insn.
    let _g = lock_boot();
    let (cpu, mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    assert_eq!(mem.read32(0xE000EF40), 0x1011_0021, "MVFR0");
    assert_eq!(mem.read32(0xE000EF44), 0x1100_0011, "MVFR1");
    assert_eq!(mem.read32(0xE000EF48), 0x0000_0040, "MVFR2");
    assert_eq!(mem.read32(0xE000ED88), 0, "CPACR reset disables FPU");
    assert_eq!(mem.read32(0xE000EF34) & 0xC000_0000, 0xC000_0000, "FPCCR ASPEN|LSPEN");
    assert_eq!(cpu.regs.fpscr, 0, "FPSCR reset");
    assert!(cpu.regs.s.iter().all(|&w| w == 0), "S-file reset");
}

#[test]
fn fpu_nocp_faults_and_latches_ufsr() {
    // vmov.f32 s0, #1.0 (EEB7 0A00) with CPACR==0: loud fault (no delivery
    // in tests) + UFSR NOCP latched (CFSR bit 19). Non-FPU coproc (0xC)
    // faults regardless of CPACR. One locked boot: the latch lives in the
    // process-global model, so no fresh boot may intervene before the read.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    mem.write32(0xE000ED88, 0); // do not rely on reset: parallel tests share SYS
    mem.write16(0x20002000, 0xEEB7);
    mem.write16(0x20002002, 0x0A00);
    cpu.regs.r[15] = 0x20002001;
    let sys = crate::sys();
    cpu.run(sys, &mut mem, 1);
    assert!(cpu.fault.is_some(), "FPU insn without CPACR must fault");
    assert_ne!(mem.read32(0xE000ED28) & 0x0008_0000, 0, "UFSR NOCP latched");
    // coproc 0xC (not FPU) faults even with CPACR fully enabled.
    mem.write32(0xE000ED88, 0x00F0_0000);
    cpu.fault = None;
    mem.write16(0x20002000, 0xEEC7);
    mem.write16(0x20002002, 0x0C00);
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 1);
    assert!(cpu.fault.is_some(), "non-FPU coproc must fault");
}

/// FPU snippet runner: fresh boot, CPACR full access (explicit: SYS is
/// process-global), S-file/FPSCR seeding, run, return owned state. Model
/// state must not be read after return (another test may re-boot); cpu and
/// RAM results are stable.
fn run_fpu_snippet(code: &[u16], regs: &[(usize, u32)], sregs: &[(usize, u32)], fpscr: u32, n: u32) -> (Cpu, FlatMemory) {
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    mem.write32(0xE000ED88, 0x00F0_0000); // CP10+CP11 full access
    for (i, w) in code.iter().enumerate() {
        mem.write16(0x20002000 + (i as u32) * 2, *w);
    }
    for &(r, v) in regs {
        cpu.regs.r[r] = v;
    }
    for &(r, v) in sregs {
        cpu.regs.s[r] = v;
    }
    cpu.regs.fpscr = fpscr;
    cpu.regs.r[15] = 0x20002001;
    let sys = crate::sys();
    cpu.run(sys, &mut mem, n);
    assert!(cpu.fault.is_none(), "fpu fault: pc={:08x} op1={:04x} op2={:04x}",
        cpu.fault.map(|f| f.pc).unwrap_or(0), cpu.fault.map(|f| f.op1).unwrap_or(0),
        cpu.fault.map(|f| f.op2).unwrap_or(0));
    (cpu, mem)
}

#[test]
fn fpu_vmov_imm() {
    // GAS: vmov.f32 s0,#1.0=EEB7 0A00; vmov.f32 s5,#-0.5=EEFE 2A00;
    // vmov.f32 s0,#6.75=EEB1 0A0B (VFPExpandImm pins).
    let (cpu, _) = run_fpu_snippet(&[0xEEB7, 0x0A00], &[], &[], 0, 1);
    assert_eq!(cpu.regs.s[0], 0x3F80_0000);
    let (cpu, _) = run_fpu_snippet(&[0xEEFE, 0x2A00], &[], &[], 0, 1);
    assert_eq!(cpu.regs.s[5], 0xBF00_0000);
    let (cpu, _) = run_fpu_snippet(&[0xEEB1, 0x0A0B], &[], &[], 0, 1);
    assert_eq!(cpu.regs.s[0], 0x40D8_0000);
    assert_ne!(cpu.regs.control & 4, 0, "FPCA set by FPU use");
}

#[test]
fn fpu_vmov_reg_and_core() {
    // vmov.f32 s0,s1=EEB0 0A60; vmov s4,r5=EE02 5A10; vmov r4,s5=EE12 4A90.
    let (cpu, _) = run_fpu_snippet(&[0xEEB0, 0x0A60], &[], &[(1, 0x4049_0FDB)], 0, 1);
    assert_eq!(cpu.regs.s[0], 0x4049_0FDB);
    let (cpu, _) = run_fpu_snippet(&[0xEE02, 0x5A10], &[(5, 0xDEAD_BEEF)], &[], 0, 1);
    assert_eq!(cpu.regs.s[4], 0xDEAD_BEEF);
    let (cpu, _) = run_fpu_snippet(&[0xEE12, 0x4A90], &[], &[(5, 0x1234_5678)], 0, 1);
    assert_eq!(cpu.regs.r[4], 0x1234_5678);
    // High regs: vmov s20,r4=EE0A 4A10; vmov r4,s20=EE1A 4A10.
    let (cpu, _) = run_fpu_snippet(&[0xEE0A, 0x4A10], &[(4, 0xA5A5_A5A5)], &[], 0, 1);
    assert_eq!(cpu.regs.s[20], 0xA5A5_A5A5);
    let (cpu, _) = run_fpu_snippet(&[0xEE1A, 0x4A10], &[], &[(20, 0x5A5A_5A5A)], 0, 1);
    assert_eq!(cpu.regs.r[4], 0x5A5A_5A5A);
}

#[test]
fn fpu_vmov_double_pair() {
    // vmov r4,r5,d6=EC55 4B16 (r4=S12, r5=S13); reverse EC45 4B16.
    let (cpu, _) = run_fpu_snippet(&[0xEC55, 0x4B16], &[], &[(12, 0x1111_1111), (13, 0x2222_2222)], 0, 1);
    assert_eq!(cpu.regs.r[4], 0x1111_1111);
    assert_eq!(cpu.regs.r[5], 0x2222_2222);
    let (cpu, _) = run_fpu_snippet(&[0xEC45, 0x4B16], &[(4, 0x3333_3333), (5, 0x4444_4444)], &[], 0, 1);
    assert_eq!(cpu.regs.s[12], 0x3333_3333);
    assert_eq!(cpu.regs.s[13], 0x4444_4444);
}

#[test]
fn fpu_vmrs_vmsr() {
    // vmrs APSR_nzcv,fpscr=EEF1 FA10 imports NZCV only; vmrs r0,fpscr=EEF1
    // 0A10 moves the whole word; vmsr fpscr,r0=EEE1 0A10 is masked.
    let (cpu, _) = run_fpu_snippet(&[0xEEF1, 0xFA10], &[], &[], 0xE000_0000, 1);
    assert_eq!(cpu.regs.xpsr & 0xF000_0000, 0xE000_0000);
    let (cpu, _) = run_fpu_snippet(&[0xEEF1, 0x0A10], &[], &[], 0x1234_5678, 1);
    assert_eq!(cpu.regs.r[0], 0x1234_5678);
    let (cpu, _) = run_fpu_snippet(&[0xEEE1, 0x0A10], &[(0, 0xFFFF_FFFF)], &[], 0, 1);
    assert_eq!(cpu.regs.fpscr, 0xFFC0_01FF, "VMSR writes NZCVQC+AHP/DN/FZ/RMode+enables/flags only");
}

#[test]
fn fpu_vldr_vstr() {
    // GAS: vstr s4,[r5,#8]=ED85 2A02; vldr s4,[r5,#8]=ED95 2A02;
    // vldr s5,[r0]=EDD0 2A00. Store+reload in ONE boot (RAM is zeroed fresh).
    let (cpu, mem) = run_fpu_snippet(
        &[0xED85, 0x2A02, 0xED95, 0x2A02, 0xEDD0, 0x2A00],
        &[(5, 0x2000_3000), (0, 0x2000_3008)], &[(4, 0x4049_0FDB)], 0, 3);
    assert_eq!(mem.read32(0x2000_3008), 0x4049_0FDB, "vstr wrote RAM");
    assert_eq!(cpu.regs.s[4], 0x4049_0FDB, "reload via vldr s4");
    assert_eq!(cpu.regs.s[5], 0x4049_0FDB, "high-reg vldr s5");
    // Double: vldr d1,[r0]=ED90 1B00 / vstr d1,[r0]=ED80 1B00 round-trip.
    let (cpu, mem) = run_fpu_snippet(&[0xED80, 0x1B00, 0xED90, 0x1B00], &[(0, 0x2000_3100)], &[(2, 0xAAAAAAAA), (3, 0xBBBB_BBBB)], 0, 2);
    assert_eq!(mem.read32(0x2000_3100), 0xAAAAAAAA);
    assert_eq!(mem.read32(0x2000_3104), 0xBBBB_BBBB);
    assert_eq!((cpu.regs.s[2], cpu.regs.s[3]), (0xAAAAAAAA, 0xBBBB_BBBB));
}

#[test]
fn fpu_vldm_vstm_push_pop() {
    // vstmia r4,{s4-s7}=EC84 2A04 / vldmia r4,{s4-s7}=EC94 2A04;
    // vpush {s0-s3}=ED2D 0A04 / vpop {s0-s3}=ECBD 0A04; D-list EC84
    // 2B04/EC94 2B04; writeback vstmia r4!,{s16-s19}=ECA4 8A04.
    let seeds = [(4, 0x1111_1111), (5, 0x2222_2222), (6, 0x3333_3333), (7, 0x4444_4444)];
    let (cpu, mem) = run_fpu_snippet(&[0xEC84, 0x2A04], &[(4, 0x2000_3200)], &seeds, 0, 1);
    assert_eq!((mem.read32(0x2000_3200), mem.read32(0x2000_320C)), (0x1111_1111, 0x4444_4444));
    assert_eq!(cpu.regs.r[4], 0x2000_3200, "no writeback without !");
    let (cpu, _) = run_fpu_snippet(
        &[0xEC84, 0x2A04, 0xEC94, 0x2A04], &[(4, 0x2000_3300)], &seeds, 0, 2);
    assert_eq!((cpu.regs.s[4], cpu.regs.s[5], cpu.regs.s[6], cpu.regs.s[7]),
        (0x1111_1111, 0x2222_2222, 0x3333_3333, 0x4444_4444), "store+reload round-trip");
    // push/pop round-trip on the real stack.
    let (cpu, _) = run_fpu_snippet(
        &[0xED2D, 0x0A04, 0xECBD, 0x0A04], &[(13, 0x2000_4000)],
        &[(0, 0xAAAAAAAA), (1, 0xBBBB_BBBB), (2, 0xCCCC_CCCC), (3, 0xDDDD_DDDD)], 0, 2);
    assert_eq!((cpu.regs.s[0], cpu.regs.s[1], cpu.regs.s[2], cpu.regs.s[3]),
        (0xAAAAAAAA, 0xBBBB_BBBB, 0xCCCC_CCCC, 0xDDDD_DDDD));
    assert_eq!(cpu.regs.r[13], 0x2000_4000, "push+pop restores SP");
    // D-list + writeback.
    let (cpu, _) = run_fpu_snippet(
        &[0xEC84, 0x2B04, 0xEC94, 0x2B04], &[(4, 0x2000_3400)],
        &[(4, 0xAAAAAAAA), (5, 0xBBBB_BBBB)], 0, 2);
    assert_eq!((cpu.regs.s[4], cpu.regs.s[5]), (0xAAAAAAAA, 0xBBBB_BBBB), "d2-d3 round-trip");
    let (cpu, _) = run_fpu_snippet(&[0xECA4, 0x8A04], &[(4, 0x2000_3500)], &[(16, 1)], 0, 1);
    assert_eq!(cpu.regs.r[4], 0x2000_3510, "vstmia! writes back +16");
}

#[test]
fn fpu_rejects() {
    // sz=1 data-processing (no f64 on FPv4-SP), DB without writeback, bad
    // P/U combo, VLDM Rn=PC, D-list overflow, VMOV Rt=PC.
    for code in [
        [0xEE30u16, 0x0B81u16], // vadd sz=1
        [0xEE80, 0x0AD1u16],    // opc1 8 + op 1: no such op
        [0xEE30, 0x0A91u16],    // vadd shape with op2[4]=1 (reserved)
        [0xEEB6, 0x0A60u16],    // B-group opc2=6: no such op
        [0xED00, 0x2A04],       // DB store without ! (no such encoding)
        [0xEC50, 0x0A04],       // P=1,U=1: no such mode (also VMOV-2reg shape? op2 0A04: (0x04&0xD0)=0x00 != 0x10, falls to VLDM -> bad P/U)
        [0xEC9F, 0x0A04],       // vldmia pc,{s0-s3}
        [0xEC94, 0xEB08],       // vldmia r4,{d14-d17}: d17 > 15
        [0xEE10, 0xFA10],       // vmov pc,s0
    ] {
        let _g = lock_boot();
        let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
        mem.write32(0xE000ED88, 0x00F0_0000);
        for (i, w) in code.iter().enumerate() {
            mem.write16(0x20002000 + (i as u32) * 2, *w);
        }
        cpu.regs.r[15] = 0x20002001;
        let sys = crate::sys();
        cpu.run(sys, &mut mem, 1);
        assert!(cpu.fault.is_some(), "must fault: {:04x} {:04x}", code[0], code[1]);
    }
}

#[test]
fn fpu_arith_basic() {
    // GAS: vadd s4,s5,s6=EE32 2A83; vsub=EE32 2AC3; vmul s4,s5,s6=EE22
    // 2A83; vdiv=EE82 2A83; vmla s0,s1,s2=EE00 0A81; vmls=EE00 0AC1;
    // vnmla=EE10 0AC1.
    let f = |x: f32| x.to_bits();
    let (cpu, _) = run_fpu_snippet(&[0xEE32, 0x2A83], &[], &[(5, f(2.5)), (6, f(1.5))], 0, 1);
    assert_eq!(cpu.regs.s[4], f(4.0));
    assert_eq!(cpu.regs.fpscr & 0x1F, 0, "exact add sets no flags");
    let (cpu, _) = run_fpu_snippet(&[0xEE32, 0x2AC3], &[], &[(5, f(2.5)), (6, f(5.0))], 0, 1);
    assert_eq!(cpu.regs.s[4], f(-2.5));
    let (cpu, _) = run_fpu_snippet(&[0xEE22, 0x2A83], &[], &[(5, f(2.0)), (6, f(1.5))], 0, 1);
    assert_eq!(cpu.regs.s[4], f(3.0));
    let (cpu, _) = run_fpu_snippet(&[0xEE82, 0x2A83], &[], &[(5, f(7.0)), (6, f(2.0))], 0, 1);
    assert_eq!(cpu.regs.s[4], f(3.5));
    let (cpu, _) = run_fpu_snippet(&[0xEE00, 0x0A81], &[], &[(0, f(1.0)), (1, f(2.0)), (2, f(3.0))], 0, 1);
    assert_eq!(cpu.regs.s[0], f(7.0), "vmla unfused");
    let (cpu, _) = run_fpu_snippet(&[0xEE00, 0x0AC1], &[], &[(0, f(10.0)), (1, f(2.0)), (2, f(3.0))], 0, 1);
    assert_eq!(cpu.regs.s[0], f(4.0), "vmls");
    let (cpu, _) = run_fpu_snippet(&[0xEE10, 0x0AC1], &[], &[(0, f(1.0)), (1, f(2.0)), (2, f(3.0))], 0, 1);
    assert_eq!(cpu.regs.s[0], f(-7.0), "vnmla");
}

#[test]
fn fpu_arith_specials() {
    let f = |x: f32| x.to_bits();
    // Overflow: max+max -> +inf + OFC|IXC.
    let (cpu, _) = run_fpu_snippet(&[0xEE30, 0x0A81], &[], &[(1, f(f32::MAX)), (2, f(f32::MAX))], 0, 1);
    assert_eq!(cpu.regs.s[0], 0x7F80_0000);
    assert_eq!(cpu.regs.fpscr & 0x1F, 0x04 | 0x10, "OFC|IXC");
    // Divide by zero: 1/0 -> +inf + DZC; 0/0 -> NaN + IOC; inf-inf -> NaN + IOC.
    let (cpu, _) = run_fpu_snippet(&[0xEE80, 0x0A81], &[], &[(1, f(1.0)), (2, 0)], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0x7F80_0000, 0x02));
    let (cpu, _) = run_fpu_snippet(&[0xEE80, 0x0A81], &[], &[(1, 0), (2, 0)], 0, 1);
    assert_eq!(cpu.regs.fpscr & 0x1F, 0x01, "0/0 IOC");
    assert_eq!(cpu.regs.s[0], 0x7FC0_0000);
    let (cpu, _) = run_fpu_snippet(&[0xEE30, 0x0A81], &[], &[(1, 0x7F80_0000), (2, 0xFF80_0000)], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0x7FC0_0000, 0x01), "inf-inf IOC");
    // 0*inf -> NaN + IOC.
    let (cpu, _) = run_fpu_snippet(&[0xEE20, 0x0A81], &[], &[(1, 0), (2, 0x7F80_0000)], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0x7FC0_0000, 0x01));
    // QNaN propagates quietly (same bits, no flag); SNaN -> IOC + quieted.
    let (cpu, _) = run_fpu_snippet(&[0xEE30, 0x0A81], &[], &[(1, 0x7FC0_1234), (2, f(1.0))], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0x7FC0_1234, 0));
    let (cpu, _) = run_fpu_snippet(&[0xEE30, 0x0A81], &[], &[(1, 0x7F80_0001), (2, f(1.0))], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0x7FC0_0001, 0x01), "SNaN quieted + IOC");
    // DN=1: any NaN result is the default NaN.
    let (cpu, _) = run_fpu_snippet(&[0xEE30, 0x0A81], &[], &[(1, 0x7FC0_1234), (2, f(1.0))], 1 << 25, 1);
    assert_eq!(cpu.regs.s[0], 0x7FC0_0000);
    // FZ=1 flushes subnormal inputs: 2^-127 * 2 -> +0, no flags.
    let (cpu, _) = run_fpu_snippet(&[0xEE20, 0x0A81], &[], &[(1, 0x0040_0000), (2, f(2.0))], 1 << 24, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0, 0));
    // Same without FZ: 2^-126 * 0.5 -> 2^-127 subnormal -> UFC (+IXC).
    let (cpu, _) = run_fpu_snippet(&[0xEE20, 0x0A81], &[], &[(1, 0x0080_0000), (2, f(0.5))], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0x0040_0000, 0x08 | 0x10));
    // Exact min-normal needs no flags: 2^-127 * 2 -> 2^-126, exact + normal.
    let (cpu, _) = run_fpu_snippet(&[0xEE20, 0x0A81], &[], &[(1, 0x0040_0000), (2, f(2.0))], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0x0080_0000, 0));
}

#[test]
fn fpu_sqrt_abs_neg() {
    let f = |x: f32| x.to_bits();
    // GAS: vsqrt s0,s1=EEB1 0AE0; vabs=EEB0 0AE0; vneg s0,s1=EEB1 0A60.
    let (cpu, _) = run_fpu_snippet(&[0xEEB1, 0x0AE0], &[], &[(1, f(4.0))], 0, 1);
    assert_eq!(cpu.regs.s[0], f(2.0));
    let (cpu, _) = run_fpu_snippet(&[0xEEB1, 0x0AE0], &[], &[(1, f(2.0))], 0, 1);
    assert_eq!(cpu.regs.s[0], 0x3FB5_04F3, "sqrt(2)");
    let (cpu, _) = run_fpu_snippet(&[0xEEB1, 0x0AE0], &[], &[(1, f(-1.0))], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0x7FC0_0000, 0x01), "sqrt(-1) IOC");
    let (cpu, _) = run_fpu_snippet(&[0xEEB0, 0x0AE0], &[], &[(1, f(-3.0))], 0, 1);
    assert_eq!(cpu.regs.s[0], f(3.0));
    let (cpu, _) = run_fpu_snippet(&[0xEEB1, 0x0A60], &[], &[(1, f(1.0))], 0, 1);
    assert_eq!(cpu.regs.s[0], f(-1.0));
    // ABS/NEG never raise, even on SNaN (bit ops).
    let (cpu, _) = run_fpu_snippet(&[0xEEB0, 0x0AE0], &[], &[(1, 0xFF80_0001)], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0x7F80_0001, 0));
}

#[test]
fn fpu_vcmp() {
    let f = |x: f32| x.to_bits();
    // GAS: vcmp s0,s1=EEB4 0A60; vcmp s0,#0=EEB5 0A40; vmrs=EEF1 FA10.
    // LT -> 0x8, EQ -> 0x6, GT -> 0x2, unordered -> 0x3 in FPSCR+APSR.
    for (a, b, want) in [(1.0f32, 2.0, 0x8u32), (1.0, 1.0, 0x6), (2.0, 1.0, 0x2)] {
        let (cpu, _) = run_fpu_snippet(
            &[0xEEB4, 0x0A60, 0xEEF1, 0xFA10], &[], &[(0, f(a)), (1, f(b))], 0, 2);
        assert_eq!(cpu.regs.fpscr & 0xF000_0000, want << 28, "fpscr {a} vs {b}");
        assert_eq!(cpu.regs.xpsr & 0xF000_0000, want << 28, "apsr {a} vs {b}");
    }
    // -0 == +0.
    let (cpu, _) = run_fpu_snippet(&[0xEEB5, 0x0A40], &[], &[(0, f(-0.0))], 0, 1);
    assert_eq!(cpu.regs.fpscr & 0xF000_0000, 0x6000_0000);
    // QNaN: unordered, no flag. SNaN: unordered + IOC.
    let (cpu, _) = run_fpu_snippet(&[0xEEB4, 0x0A60], &[], &[(0, 0x7FC0_0000), (1, f(1.0))], 0, 1);
    assert_eq!((cpu.regs.fpscr & 0xF000_0000, cpu.regs.fpscr & 0x1F), (0x3000_0000, 0));
    let (cpu, _) = run_fpu_snippet(&[0xEEB4, 0x0A60], &[], &[(0, 0x7F80_0001), (1, f(1.0))], 0, 1);
    assert_eq!((cpu.regs.fpscr & 0xF000_0000, cpu.regs.fpscr & 0x1F), (0x3000_0000, 0x01));
}

#[test]
fn fpu_vcvt_int() {
    let f = |x: f32| x.to_bits();
    // GAS: vcvt.s32.f32=EEBD 0AC0; vcvt.u32.f32=EEBC 0AC0;
    // vcvt.f32.s32=EEB8 0AC0; vcvt.f32.u32=EEB8 0A40.
    // RNE ties-to-even: 1.5->2, 2.5->2 (not 3!), -1.5->-2, 0.5->0.
    for (x, want) in [(1.5f32, 2u32), (2.5, 2), (-1.5, 0xFFFF_FFFEu32), (0.5, 0), (2.6, 3)] {
        let (cpu, _) = run_fpu_snippet(&[0xEEBD, 0x0AC0], &[], &[(0, f(x))], 0, 1);
        assert_eq!(cpu.regs.s[0], want, "s32({x})");
    }
    let (cpu, _) = run_fpu_snippet(&[0xEEBC, 0x0AC0], &[], &[(0, f(1.5))], 0, 1);
    assert_eq!(cpu.regs.s[0], 2);
    // Invalid: NaN->0, +overflow saturates, -overflow saturates, all + IOC.
    let (cpu, _) = run_fpu_snippet(&[0xEEBD, 0x0AC0], &[], &[(0, 0x7FC0_0000)], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0, 0x01));
    let (cpu, _) = run_fpu_snippet(&[0xEEBD, 0x0AC0], &[], &[(0, f(1e20))], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0x7FFF_FFFF, 0x01));
    let (cpu, _) = run_fpu_snippet(&[0xEEBD, 0x0AC0], &[], &[(0, f(-1e20))], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0x8000_0000, 0x01));
    let (cpu, _) = run_fpu_snippet(&[0xEEBC, 0x0AC0], &[], &[(0, f(4294967295.0))], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0xFFFF_FFFF, 0x01), "u32 range");
    // Exact: i32::MIN is valid and exact (no IXC); 1.5 is inexact (IXC).
    let (cpu, _) = run_fpu_snippet(&[0xEEBD, 0x0AC0], &[], &[(0, f(-2147483648.0))], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0x8000_0000, 0));
    let (cpu, _) = run_fpu_snippet(&[0xEEBD, 0x0AC0], &[], &[(0, f(1.5))], 0, 1);
    assert_eq!(cpu.regs.fpscr & 0x1F, 0x10, "inexact IXC");
    // int->float: exact (no IXC) vs inexact (IXC).
    let (cpu, _) = run_fpu_snippet(&[0xEEB8, 0x0AC0], &[], &[(0, 42)], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (f(42.0), 0));
    let (cpu, _) = run_fpu_snippet(&[0xEEB8, 0x0AC0], &[], &[(0, 0x1234_5678)], 0, 1);
    assert_eq!(cpu.regs.s[0], (0x1234_5678i32 as f32).to_bits());
    assert_eq!(cpu.regs.fpscr & 0x1F, 0x10);
    // RMode via VMSR: toward-zero turns 1.9 into 1 (RNE would give 2).
    let (cpu, _) = run_fpu_snippet(
        &[0xEEE1, 0x0A10, 0xEEBD, 0x0AC0], &[(0, 3 << 22)], &[(0, f(1.9))], 0, 2);
    assert_eq!(cpu.regs.s[0], 1);
}

#[test]
fn fpu_vcvt_fixed() {
    let f = |x: f32| x.to_bits();
    // GAS: vcvt.f32.s32 #16=EEBA 0AC8; vcvt.s32.f32 #16=EEBE 0AC8;
    // vcvt.f32.u32 #16=EEBB 0AC8; vcvt.f32.s32 #1=EEBA 0AEF.
    let (cpu, _) = run_fpu_snippet(&[0xEEBA, 0x0AC8], &[], &[(0, 0x0001_0000)], 0, 1);
    assert_eq!(cpu.regs.s[0], f(1.0), "Q16.16 1.0");
    let (cpu, _) = run_fpu_snippet(&[0xEEBE, 0x0AC8], &[], &[(0, f(1.0))], 0, 1);
    assert_eq!(cpu.regs.s[0], 0x0001_0000);
    let (cpu, _) = run_fpu_snippet(&[0xEEBA, 0x0AEF], &[], &[(0, 3)], 0, 1);
    assert_eq!(cpu.regs.s[0], f(1.5), "#1 frac");
    let (cpu, _) = run_fpu_snippet(&[0xEEBB, 0x0AC8], &[], &[(0, 0x0001_0000)], 0, 1);
    assert_eq!(cpu.regs.s[0], f(1.0), "unsigned Q16.16");
    // Negative fixed stays negative; overflow saturates + IOC.
    let (cpu, _) = run_fpu_snippet(&[0xEEBA, 0x0AC8], &[], &[(0, 0xFFFF_0000u32)], 0, 1);
    assert_eq!(cpu.regs.s[0], f(-1.0));
    let (cpu, _) = run_fpu_snippet(&[0xEEBE, 0x0AC8], &[], &[(0, f(100000.0))], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0x7FFF_FFFF, 0x01), "s32 #16 saturate");
}

#[test]
fn fpu_vcvt_f16() {
    let f = |x: f32| x.to_bits();
    // GAS: vcvtb.f32.f16=EEB2 0A60; vcvtt.f32.f16=EEB2 0AE0;
    // vcvtb.f16.f32=EEB3 0A60; vcvtt.f16.f32=EEB3 0AE0.
    let (cpu, _) = run_fpu_snippet(&[0xEEB2, 0x0A60], &[], &[(1, 0x3C00)], 0, 1);
    assert_eq!(cpu.regs.s[0], f(1.0), "bottom half");
    let (cpu, _) = run_fpu_snippet(&[0xEEB2, 0x0AE0], &[], &[(1, 0x3C00_0000)], 0, 1);
    assert_eq!(cpu.regs.s[0], f(1.0), "top half");
    let (cpu, _) = run_fpu_snippet(&[0xEEB2, 0x0A60], &[], &[(1, 0x0001)], 0, 1);
    assert_eq!(cpu.regs.s[0], 0x3380_0000, "f16 subnormal 2^-24");
    let (cpu, _) = run_fpu_snippet(&[0xEEB2, 0x0A60], &[], &[(1, 0x7C00)], 0, 1);
    assert_eq!(cpu.regs.s[0], 0x7F80_0000, "f16 inf");
    // f32->f16: 1.0->0x3C00 (low half, high preserved), 100000->inf+OFC|IXC.
    let (cpu, _) = run_fpu_snippet(&[0xEEB3, 0x0A60], &[], &[(0, 0xABCD_0000), (1, f(1.0))], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0xABCD_3C00, 0));
    let (cpu, _) = run_fpu_snippet(&[0xEEB3, 0x0AE0], &[], &[(1, f(6.75))], 0, 1);
    assert_eq!(cpu.regs.s[0] >> 16, 0x46C0, "top-half 6.75");
    let (cpu, _) = run_fpu_snippet(&[0xEEB3, 0x0A60], &[], &[(1, f(100000.0))], 0, 1);
    assert_eq!((cpu.regs.s[0] & 0xFFFF, cpu.regs.fpscr & 0x1F), (0x7C00, 0x04 | 0x10));
    // Tiny: 1e-5 -> f16 subnormal 0xA8 + UFC|IXC.
    let (cpu, _) = run_fpu_snippet(&[0xEEB3, 0x0A60], &[], &[(1, f(1e-5))], 0, 1);
    assert_eq!((cpu.regs.s[0] & 0xFFFF, cpu.regs.fpscr & 0x1F), (0x00A8, 0x08 | 0x10));
    // f16 SNaN -> IOC (both directions); f32 SNaN narrows quieted.
    let (cpu, _) = run_fpu_snippet(&[0xEEB2, 0x0A60], &[], &[(1, 0x7C01)], 0, 1);
    assert_eq!(cpu.regs.fpscr & 0x1F, 0x01);
    assert_eq!(cpu.regs.s[0] & 0x7FFF_FFFF, 0x7FC0_2000, "SNaN widened+quieted");
    let (cpu, _) = run_fpu_snippet(&[0xEEB3, 0x0A60], &[], &[(1, 0x7F80_0001)], 0, 1);
    assert_eq!(cpu.regs.fpscr & 0x1F, 0x01);
    assert_eq!((cpu.regs.s[0] >> 10) & 0x1F, 0x1F, "narrowed NaN exp all-ones");
    assert_ne!(cpu.regs.s[0] & 0x3FF, 0, "narrowed NaN keeps payload");
}

#[test]
fn fpu_fma_fused() {
    let f = |x: f32| x.to_bits();
    // GAS (fpu11.s): vfma s0,s1,s2=EEA0 0A81; vfms=EEA0 0AC1;
    // vfnma s0,s1,s2=EE90 0AC1; vfnms=EE90 0A81.
    let (cpu, _) = run_fpu_snippet(&[0xEEA0, 0x0A81], &[], &[(0, f(1.0)), (1, f(2.0)), (2, f(3.0))], 0, 1);
    assert_eq!(cpu.regs.s[0], f(7.0));
    let (cpu, _) = run_fpu_snippet(&[0xEEA0, 0x0AC1], &[], &[(0, f(10.0)), (1, f(2.0)), (2, f(3.0))], 0, 1);
    assert_eq!(cpu.regs.s[0], f(4.0), "vfms");
    let (cpu, _) = run_fpu_snippet(&[0xEE90, 0x0AC1], &[], &[(0, f(1.0)), (1, f(2.0)), (2, f(3.0))], 0, 1);
    assert_eq!(cpu.regs.s[0], f(-7.0), "vfnma");
    let (cpu, _) = run_fpu_snippet(&[0xEE90, 0x0A81], &[], &[(0, f(10.0)), (1, f(2.0)), (2, f(3.0))], 0, 1);
    assert_eq!(cpu.regs.s[0], f(-4.0), "vfnms");
    // THE fused-vs-unfused discriminator: a=b=1+2^-23, acc=-(1+2^-22).
    // Unfused rounds a*b to 1+2^-22 first, then acc+p = 0. Fused keeps the
    // exact 2^-46 square term: result 2^-46 (exp 81-127, 0x28800000).
    let e = 2f32.powi(-23);
    let (cpu, _) = run_fpu_snippet(&[0xEEA0, 0x0A81], &[], &[(0, f(-(1.0 + 2.0 * e))), (1, f(1.0 + e)), (2, f(1.0 + e))], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0x2880_0000, 0), "fused keeps eps^2");
    // Same inputs through UNfused vmla give exactly 0 (control case).
    let (cpu, _) = run_fpu_snippet(&[0xEE00, 0x0A81], &[], &[(0, f(-(1.0 + 2.0 * e))), (1, f(1.0 + e)), (2, f(1.0 + e))], 0, 1);
    assert_eq!(cpu.regs.s[0], 0x0000_0000, "unfused rounds first");
    // Specials mirror unfused: 0*inf -> IOC; inf-inf -> IOC; QNaN quiet.
    let (cpu, _) = run_fpu_snippet(&[0xEEA0, 0x0A81], &[], &[(0, f(1.0)), (1, 0), (2, 0x7F80_0000)], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0x7FC0_0000, 0x01));
    let (cpu, _) = run_fpu_snippet(&[0xEEA0, 0x0A81], &[], &[(0, 0xFF80_0000), (1, 0x7F80_0000), (2, f(1.0))], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0x7FC0_0000, 0x01), "inf-inf IOC");
    let (cpu, _) = run_fpu_snippet(&[0xEEA0, 0x0A81], &[], &[(0, 0x7FC0_1234), (1, f(1.0)), (2, f(2.0))], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0x7FC0_1234, 0));
    // Overflow: max*2 + max -> +inf + OFC|IXC.
    let (cpu, _) = run_fpu_snippet(&[0xEEA0, 0x0A81], &[], &[(0, f(f32::MAX)), (1, f(f32::MAX)), (2, f(2.0))], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0x7F80_0000, 0x04 | 0x10));
    // Exact cancellation: acc == a*b -> +0, no flags.
    let (cpu, _) = run_fpu_snippet(&[0xEEA0, 0x0AC1], &[], &[(0, f(6.0)), (1, f(2.0)), (2, f(3.0))], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0, 0), "vfms exact zero");
    // Zero addend: single-rounding product (vfms negates it).
    let (cpu, _) = run_fpu_snippet(&[0xEEA0, 0x0A81], &[], &[(0, 0), (1, f(2.0)), (2, f(3.0))], 0, 1);
    assert_eq!(cpu.regs.s[0], f(6.0));
    let (cpu, _) = run_fpu_snippet(&[0xEEA0, 0x0AC1], &[], &[(0, 0), (1, f(2.0)), (2, f(3.0))], 0, 1);
    assert_eq!(cpu.regs.s[0], f(-6.0), "vfms zero-acc negates");
}

#[test]
fn fpu_vcmpe() {
    let f = |x: f32| x.to_bits();
    // GAS (fpu12.s): vcmpe s0,s1=EEB4 0AE0; vcmpe s4,s5=EEB4 2AE2;
    // vcmpe s0,#0=EEB5 0AC0. E-form raises IOC on ANY NaN (quiet too).
    let (cpu, _) = run_fpu_snippet(&[0xEEB4, 0x0AE0], &[], &[(0, f(1.0)), (1, f(2.0))], 0, 1);
    assert_eq!((cpu.regs.fpscr & 0xF000_0000, cpu.regs.fpscr & 0x1F), (0x8000_0000, 0), "ordered LT, no flag");
    let (cpu, _) = run_fpu_snippet(&[0xEEB4, 0x0AE0], &[], &[(0, 0x7FC0_0000), (1, f(1.0))], 0, 1);
    assert_eq!((cpu.regs.fpscr & 0xF000_0000, cpu.regs.fpscr & 0x1F), (0x3000_0000, 0x01), "QNaN unordered + IOC");
    let (cpu, _) = run_fpu_snippet(&[0xEEB4, 0x2AE2], &[], &[(4, f(2.0)), (5, f(1.0))], 0, 1);
    assert_eq!(cpu.regs.fpscr & 0xF000_0000, 0x2000_0000, "high-reg GT");
    let (cpu, _) = run_fpu_snippet(&[0xEEB5, 0x0AC0], &[], &[(0, f(0.0))], 0, 1);
    assert_eq!((cpu.regs.fpscr & 0xF000_0000, cpu.regs.fpscr & 0x1F), (0x6000_0000, 0), "E-#0 equal");
}

#[test]
fn fpu_high_regs_and_even_sm() {
    // The D=1 (odd-high dest) and M=0 (even source) space the first FPU
    // cut missed: opc used to include D, and the B-group op-nibble baked
    // in M=1. GAS vectors: fpu13.s + fpu14.s.
    let f = |x: f32| x.to_bits();
    // vadd s17,s18,s19=EE79 8A29 (the firmware's EE76-class bug).
    let (cpu, _) = run_fpu_snippet(&[0xEE79, 0x8A29], &[], &[(18, f(1.5)), (19, f(2.5))], 0, 1);
    assert_eq!(cpu.regs.s[17], f(4.0));
    // vsub s31,s30,s29=EE7F FA6E (D,N,M all set).
    let (cpu, _) = run_fpu_snippet(&[0xEE7F, 0xFA6E], &[], &[(30, f(5.0)), (29, f(1.5))], 0, 1);
    assert_eq!(cpu.regs.s[31], f(3.5));
    // vfma s21,s22,s23=EEEB AA2B.
    let (cpu, _) = run_fpu_snippet(&[0xEEEB, 0xAA2B], &[], &[(21, f(1.0)), (22, f(2.0)), (23, f(3.0))], 0, 1);
    assert_eq!(cpu.regs.s[21], f(7.0));
    // vsqrt s17,s18=EEF1 8AC9 (op1 collides with VMRS shape; op2lo decides).
    let (cpu, _) = run_fpu_snippet(&[0xEEF1, 0x8AC9], &[], &[(18, f(9.0))], 0, 1);
    assert_eq!(cpu.regs.s[17], f(3.0));
    // vcmp s17,s18=EEF4 8A49; vmov s17,s18=EEF0 8A49.
    let (cpu, _) = run_fpu_snippet(&[0xEEF4, 0x8A49], &[], &[(17, f(1.0)), (18, f(1.0))], 0, 1);
    assert_eq!(cpu.regs.fpscr & 0xF000_0000, 0x6000_0000);
    let (cpu, _) = run_fpu_snippet(&[0xEEF0, 0x8A49], &[], &[(18, 0xDEAD_BEEF)], 0, 1);
    assert_eq!(cpu.regs.s[17], 0xDEAD_BEEF);
    // vcvt.s32.f32 s17,s18=EEFD 8AC9 (old gate faulted M=1 here).
    let (cpu, _) = run_fpu_snippet(&[0xEEFD, 0x8AC9], &[], &[(18, f(2.5))], 0, 1);
    assert_eq!(cpu.regs.s[17], 2);
    // M=0 forms the old nibble match missed.
    let (cpu, _) = run_fpu_snippet(&[0xEEB0, 0x0A40], &[], &[(0, 0x1234_5678)], 0, 1);
    assert_eq!(cpu.regs.s[0], 0x1234_5678, "vmov M=0");
    let (cpu, _) = run_fpu_snippet(&[0xEEB1, 0x0A40], &[], &[(0, f(1.0))], 0, 1);
    assert_eq!(cpu.regs.s[0], f(-1.0), "vneg M=0");
    let (cpu, _) = run_fpu_snippet(&[0xEEB0, 0x0AC0], &[], &[(0, f(-1.0))], 0, 1);
    assert_eq!(cpu.regs.s[0], f(1.0), "vabs M=0");
    let (cpu, _) = run_fpu_snippet(&[0xEEB4, 0x0A40], &[], &[(0, f(1.0))], 0, 1);
    assert_eq!(cpu.regs.fpscr & 0xF000_0000, 0x6000_0000, "vcmp M=0");
    let (cpu, _) = run_fpu_snippet(&[0xEEB2, 0x0A40], &[], &[(0, 0x3C00)], 0, 1);
    assert_eq!(cpu.regs.s[0], f(1.0), "vcvtb M=0");
    // Unsigned int->float (sign is op2[7], not op2[6]) + M=1 source.
    let (cpu, _) = run_fpu_snippet(&[0xEEB8, 0x2A62], &[], &[(5, 0xFFFF_FFFF)], 0, 1);
    assert_eq!((cpu.regs.s[4], cpu.regs.fpscr & 0x1F), (0x4F80_0000, 0x10), "u32 max -> 2^32");
    let (cpu, _) = run_fpu_snippet(&[0xEEB8, 0x2AE2], &[], &[(5, 42)], 0, 1);
    assert_eq!((cpu.regs.s[4], cpu.regs.fpscr & 0x1F), (f(42.0), 0), "s32 M=1 exact");
}

#[test]
fn fpu_lazy_reserve_and_return() {
    // CONTROL.FPCA=1 + FPCCR.ASPEN (reset) => PendSV reserves the 26-word
    // extended frame: FPSCR stacked at +96, S0-S15 untouched (lazy),
    // LSPACT=1, FPCAR=sp+32, LR=0xFFFFFFE9. Return restores FPSCR + SP.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    let sys = crate::sys();
    mem.write32(0xE000ED88, 0x00F0_0000);
    cpu.regs.s[0] = 0xAAAAAAAA;
    cpu.regs.s[15] = 0xBBBB_BBBB;
    cpu.regs.fpscr = 0x1234_5678;
    cpu.regs.control |= 4; // FPCA: thread uses the FPU
    let sp0 = cpu.regs.r[13];
    cpu.take_exception(sys, &mut mem, -2); // PendSV
    assert_eq!(cpu.regs.r[14], 0xFFFF_FFE9, "LR carries FType=0");
    let sp = cpu.regs.r[13];
    assert_eq!(sp, sp0.wrapping_sub(104), "26-word frame");
    assert_eq!(mem.read32(sp.wrapping_add(96)), 0x1234_5678, "FPSCR stacked");
    assert_eq!(mem.read32(sp.wrapping_add(32)), 0, "S0 lazy (untouched)");
    assert_ne!(sys.p.read(sys, 0xE000EF34, 4) & 1, 0, "LSPACT set");
    assert_eq!(sys.p.read(sys, 0xE000EF38, 4), sp.wrapping_add(32), "FPCAR at S0 slot");
    // Handler clobbers FPSCR; return must restore it (lazy: no S traffic).
    cpu.regs.fpscr = 0;
    cpu.regs.s[0] = 0;
    assert!(cpu.exception_return(sys, &mut mem, 0xFFFF_FFE9, 0x2000_2000));
    no_fault(&cpu, &mem);
    assert_eq!(cpu.regs.fpscr, 0x1234_5678, "FPSCR restored");
    assert_eq!(cpu.regs.s[0], 0, "S0 not restored (was never stacked)");
    assert_eq!(cpu.regs.r[13], sp0, "SP restored");
    assert_eq!(sys.p.read(sys, 0xE000EF34, 4) & 1, 0, "LSPACT clear after pop");
}

#[test]
fn fpu_lazy_first_use_stacks() {
    // First handler FPU use stacks the LIVE S regs into FPCAR, clears
    // LSPACT; return then restores the thread's values (over handler mods).
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    let sys = crate::sys();
    mem.write32(0xE000ED88, 0x00F0_0000);
    cpu.regs.s[0] = 0xAAAAAAAA;
    cpu.regs.s[15] = 0xBBBB_BBBB;
    cpu.regs.control |= 4;
    cpu.take_exception(sys, &mut mem, -2);
    let fpcar = sys.p.read(sys, 0xE000EF38, 4);
    // Run a real FPU insn in handler mode: vmov.f32 s0,#1.0.
    mem.write16(0x20002000, 0xEEB7);
    mem.write16(0x20002002, 0x0A00);
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 1);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.regs.s[0], 0x3F80_0000, "insn executed");
    assert_eq!(mem.read32(fpcar), 0xAAAAAAAA, "live S0 stacked first");
    assert_eq!(mem.read32(fpcar.wrapping_add(60)), 0xBBBB_BBBB, "live S15 stacked");
    assert_eq!(sys.p.read(sys, 0xE000EF34, 4) & 1, 0, "LSPACT cleared by use");
    // Return with LSPACT=0: full unstack overwrites the handler's s0=1.0
    // with the thread's seeded value.
    assert!(cpu.exception_return(sys, &mut mem, 0xFFFF_FFE9, 0x2000_2000));
    no_fault(&cpu, &mem);
    assert_eq!(cpu.regs.s[0], 0xAAAAAAAA, "thread S0 restored");
    assert_eq!(cpu.regs.s[15], 0xBBBB_BBBB, "thread S15 restored");
}

#[test]
fn fpu_eager_stacks_at_entry() {
    // LSPEN=0: S0-S15 + FPSCR land in the frame at entry; LSPACT stays 0.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    let sys = crate::sys();
    mem.write32(0xE000ED88, 0x00F0_0000);
    mem.write32(0xE000EF34, 0x8000_0000); // ASPEN only (no LSPEN)
    cpu.regs.s[3] = 0xCCCC_CCCC;
    cpu.regs.fpscr = 0x1122_3344;
    cpu.regs.control |= 4;
    cpu.take_exception(sys, &mut mem, -2);
    assert_eq!(cpu.regs.r[14], 0xFFFF_FFE9, "FType=0 even eager");
    let sp = cpu.regs.r[13];
    assert_eq!(mem.read32(sp.wrapping_add(32 + 12)), 0xCCCC_CCCC, "S3 stacked at entry");
    assert_eq!(mem.read32(sp.wrapping_add(96)), 0x1122_3344, "FPSCR stacked at entry");
    assert_eq!(sys.p.read(sys, 0xE000EF34, 4) & 1, 0, "no LSPACT when eager");
    cpu.regs.s[3] = 0;
    assert!(cpu.exception_return(sys, &mut mem, 0xFFFF_FFE9, 0x2000_2000));
    assert_eq!(cpu.regs.s[3], 0xCCCC_CCCC, "eager frame restores");
}

#[test]
fn fpu_no_fpca_unchanged() {
    // No FPU use: classic 8-word frame, classic LR, model FP regs untouched.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    let sys = crate::sys();
    mem.write32(0xE000ED88, 0x00F0_0000);
    let sp0 = cpu.regs.r[13];
    cpu.take_exception(sys, &mut mem, -2);
    assert_eq!(cpu.regs.r[14], 0xFFFF_FFF9, "classic EXC_RETURN");
    assert_eq!(cpu.regs.r[13], sp0.wrapping_sub(32), "8-word frame");
    assert_eq!(sys.p.read(sys, 0xE000EF34, 4) & 1, 0, "no LSPACT");
    assert_eq!(sys.p.read(sys, 0xE000EF38, 4), 0, "no FPCAR");
    assert!(cpu.exception_return(sys, &mut mem, 0xFFFF_FFF9, 0x2000_2000));
    no_fault(&cpu, &mem);
    assert_eq!(cpu.regs.r[13], sp0);
}

#[test]
fn fpu_nested_frames() {
    // Outer PendSV reserve, inner SysTick reserve, inner use+return, outer
    // use+return — every S modification goes through a real FPU snippet
    // (the only way guest code CAN touch S-regs, so the hook always fires).
    // Thread 0x11111111 -> outer use stacks it, sets 1.0 -> inner use
    // stacks 1.0, sets -0.5 -> returns unwind exactly.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    let sys = crate::sys();
    mem.write32(0xE000ED88, 0x00F0_0000);
    // Thread uses the FPU: vmov s0,r0 (EE00 0A10) with r0=0x11111111.
    cpu.regs.r[0] = 0x1111_1111;
    mem.write16(0x20002000, 0xEE00);
    mem.write16(0x20002002, 0x0A10);
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 1);
    assert_eq!(cpu.regs.s[0], 0x1111_1111);
    assert_ne!(cpu.regs.control & 4, 0, "FPCA set by use");
    cpu.take_exception(sys, &mut mem, -2); // outer PendSV
    let outer_fpcar = sys.p.read(sys, 0xE000EF38, 4);
    let outer_lr = cpu.regs.r[14];
    assert_eq!(outer_lr, 0xFFFF_FFE9);
    // Outer handler: vmov.f32 s0,#1.0 stacks 0x11111111, sets 1.0.
    mem.write16(0x20002000, 0xEEB7);
    mem.write16(0x20002002, 0x0A00);
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 1);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.regs.s[0], 0x3F80_0000);
    assert_eq!(mem.read32(outer_fpcar), 0x1111_1111, "thread value stacked");
    cpu.take_exception(sys, &mut mem, -1); // inner SysTick
    let inner_fpcar = sys.p.read(sys, 0xE000EF38, 4);
    assert_ne!(inner_fpcar, outer_fpcar, "inner frame is distinct");
    let inner_lr = cpu.regs.r[14];
    // Inner handler: vmov.f32 s0,#-0.5 (EEBE 0A00, GAS fpu15.s)
    // stacks live 1.0, sets -0.5.
    mem.write16(0x20002000, 0xEEBE);
    mem.write16(0x20002002, 0x0A00);
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 1);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.regs.s[0], 0xBF00_0000);
    assert_eq!(mem.read32(inner_fpcar), 0x3F80_0000, "outer-live stacked");
    assert!(cpu.exception_return(sys, &mut mem, inner_lr, 0x2000_2000));
    assert_eq!(cpu.regs.s[0], 0x3F80_0000, "outer-live restored");
    assert_eq!(sys.p.read(sys, 0xE000EF38, 4), outer_fpcar, "outer FPCAR restored");
    assert_eq!(sys.p.read(sys, 0xE000EF34, 4) & 1, 0, "outer LSPACT consumed");
    assert!(cpu.exception_return(sys, &mut mem, outer_lr, 0x2000_2000));
    assert_eq!(cpu.regs.s[0], 0x1111_1111, "thread value restored");
    assert_eq!(sys.p.read(sys, 0xE000EF34, 4) & 1, 0, "LSPACT clear at end");
}

#[test]
fn fpu_rmode_arith() {
    // Directed rounding via VMSR RMode (vmsr=EEE1 0A10): 1.0 + 2^-24 is
    // exact-tie-ish — RNE gives 1.0 (even), +inf gives nextUp, -inf/zero
    // give 1.0, all with IXC (except the exact RNE case... 1.0 is inexact
    // too: the true sum is not representable, so IXC fires everywhere).
    let f = |x: f32| x.to_bits();
    let tiny = 0x3380_0000u32; // 2^-24
    // RNE baseline.
    let (cpu, _) = run_fpu_snippet(&[0xEE30, 0x0A81], &[], &[(1, f(1.0)), (2, tiny)], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (f(1.0), 0x10));
    // Toward +inf: nextUp(1.0) = 1+2^-23.
    let (cpu, _) = run_fpu_snippet(
        &[0xEEE1, 0x0A10, 0xEE30, 0x0A81], &[(0, 1 << 22)], &[(1, f(1.0)), (2, tiny)], 0, 2);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0x3F80_0001, 0x10));
    // Toward -inf and toward zero: 1.0.
    for rmode in [2u32, 3u32] {
        let (cpu, _) = run_fpu_snippet(
            &[0xEEE1, 0x0A10, 0xEE30, 0x0A81], &[(0, rmode << 22)], &[(1, f(1.0)), (2, tiny)], 0, 2);
        assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (f(1.0), 0x10), "rmode {rmode}");
    }
    // Mul directed: (1+2^-22)^2... use (1+2^-23)*(1+2^-23): RNE rounds the
    // 2^-46 square term away -> 1+2^-22; -inf keeps it too (above); the
    // discriminating mode is toward-zero/+inf only when below... instead
    // pin exactness: 1.5*1.5 = 2.25 exact in all modes, no IXC.
    let (cpu, _) = run_fpu_snippet(
        &[0xEEE1, 0x0A10, 0xEE20, 0x0A81], &[(0, 1 << 22)], &[(1, f(1.5)), (2, f(1.5))], 0, 2);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (f(2.25), 0));
    // Div/sqrt directed, expectations computed from host f64 (correctly
    // rounded) + next_up/down — execution-grounded, not hand-derived.
    let t = 1f64 / 3f64;
    let rne = t as f32;
    let up = if (rne as f64) < t { rne.next_up() } else { rne };
    let (cpu, _) = run_fpu_snippet(
        &[0xEEE1, 0x0A10, 0xEE80, 0x0A81], &[(0, 1 << 22)], &[(1, f(1.0)), (2, f(3.0))], 0, 2);
    assert_eq!(cpu.regs.s[0], up.to_bits(), "div toward +inf");
    assert_ne!(cpu.regs.fpscr & 0x10, 0, "div inexact IXC");
    let t = 2f64.sqrt();
    let rne = t as f32;
    let up = if (rne as f64) < t { rne.next_up() } else { rne };
    let (cpu, _) = run_fpu_snippet(
        &[0xEEE1, 0x0A10, 0xEEB1, 0x0AE0], &[(0, 1 << 22)], &[(1, f(2.0))], 0, 2);
    assert_eq!(cpu.regs.s[0], up.to_bits(), "sqrt toward +inf");
}

#[test]
fn fpu_directed_div_sqrt_exact() {
    // Directed div/sqrt go through exact integer cores (long division +
    // digit-recurrence sqrt) — no f64 anywhere. Hand-derived vectors where
    // the directed answer provably differs from RNE.
    let f = |x: f32| x.to_bits();
    // 1/3: RNE = 0x3EAAAAAB (above truth); toward -inf steps down one ulp.
    let (cpu, _) = run_fpu_snippet(
        &[0xEEE1, 0x0A10, 0xEE80, 0x0A81], &[(0, 2 << 22)], &[(1, f(1.0)), (2, f(3.0))], 0, 2);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0x3EAA_AAAA, 0x10));
    // 2/3: RNE = 0x3F2AAAAB (above truth); -inf and toward-zero agree here.
    for (rm, want) in [(1u32, 0x3F2A_AAAB), (2, 0x3F2A_AAAA), (3, 0x3F2A_AAAA)] {
        let (cpu, _) = run_fpu_snippet(
            &[0xEEE1, 0x0A10, 0xEE80, 0x0A81], &[(0, rm << 22)], &[(1, f(2.0)), (2, f(3.0))], 0, 2);
        assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (want, 0x10), "2/3 rmode {rm}");
    }
    // Overflow quotient: max/0.5 overflows; -inf clamps to max finite.
    let (cpu, _) = run_fpu_snippet(
        &[0xEEE1, 0x0A10, 0xEE80, 0x0A81], &[(0, 1 << 22)], &[(1, f(f32::MAX)), (2, f(0.5))], 0, 2);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0x7F80_0000, 0x04 | 0x10));
    let (cpu, _) = run_fpu_snippet(
        &[0xEEE1, 0x0A10, 0xEE80, 0x0A81], &[(0, 2 << 22)], &[(1, f(f32::MAX)), (2, f(0.5))], 0, 2);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0x7F7F_FFFF, 0x04 | 0x10));
    // Subnormal quotient ties: (2^-126 + 2^-149)/2 = 2^-127 + half-ulp:
    // RNE-even -> 0x00400000, +inf -> 0x00400001, -inf/zero -> 0x00400000.
    for (rm, want) in [(0u32, 0x0040_0000), (1, 0x0040_0001), (2, 0x0040_0000), (3, 0x0040_0000)] {
        let (cpu, _) = run_fpu_snippet(
            &[0xEEE1, 0x0A10, 0xEE80, 0x0A81], &[(0, rm << 22)], &[(1, 0x0080_0001), (2, f(2.0))], 0, 2);
        assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (want, 0x08 | 0x10), "tiny div rmode {rm}");
    }
    // sqrt(2): RNE = 0x3FB504F3 (below truth); +inf steps up one ulp.
    let (cpu, _) = run_fpu_snippet(
        &[0xEEE1, 0x0A10, 0xEEB1, 0x0AE0], &[(0, 1 << 22)], &[(1, f(2.0))], 0, 2);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0x3FB5_04F4, 0x10));
    // Perfect squares are exact in every mode (no IXC).
    for rm in [0u32, 1, 2, 3] {
        let (cpu, _) = run_fpu_snippet(
            &[0xEEE1, 0x0A10, 0xEEB1, 0x0AE0], &[(0, rm << 22)], &[(1, f(6.25))], 0, 2);
        assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (f(2.5), 0), "sqrt exact rmode {rm}");
    }
    // sqrt(+-0) = +-0, exact, no flags (must not reach the integer core).
    let (cpu, _) = run_fpu_snippet(&[0xEEB1, 0x0AE0], &[], &[(1, 0x8000_0000)], 0, 1);
    assert_eq!((cpu.regs.s[0], cpu.regs.fpscr & 0x1F), (0x8000_0000, 0));
}

#[test]
fn fpu_vmrs_id_regs() {
    // GAS (fpu16.s): vmrs r0,mvfr0=EEF7 0A10; vmrs r4,mvfr1=EEF6 4A10.
    // Same M4F constants as the MMIO block (peripherals/fpu.rs).
    let (cpu, _) = run_fpu_snippet(&[0xEEF7, 0x0A10], &[], &[], 0, 1);
    assert_eq!(cpu.regs.r[0], 0x1011_0021, "MVFR0");
    let (cpu, _) = run_fpu_snippet(&[0xEEF6, 0x4A10], &[], &[], 0, 1);
    assert_eq!(cpu.regs.r[4], 0x1100_0011, "MVFR1");
    // MVFR with Rt=13/15 faults (no APSR form for ID regs).
    for code in [[0xEEF7, 0xFA10u16], [0xEEF6, 0xDA10u16]] {
        let _g = lock_boot();
        let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
        mem.write32(0xE000ED88, 0x00F0_0000);
        for (i, w) in code.iter().enumerate() {
            mem.write16(0x20002000 + (i as u32) * 2, *w);
        }
        cpu.regs.r[15] = 0x20002001;
        let sys = crate::sys();
        cpu.run(sys, &mut mem, 1);
        assert!(cpu.fault.is_some(), "must fault: {:04x} {:04x}", code[0], code[1]);
    }
}

#[test]
fn fpu_mvfr2_and_fpexc() {
    // MVFR2 (EEF5, probed via neon-fp-armv8 — GAS rejects the mnemonic on
    // fpv4-sp, but the encoding is architectural).
    let (cpu, _) = run_fpu_snippet(&[0xEEF5, 0x5A10], &[], &[], 0, 1);
    assert_eq!(cpu.regs.r[5], 0x0000_0040, "MVFR2");
    // FPEXC read: EX=0 (no lazy frame), EN=1 (CPACR full, reset-set shadow).
    let (cpu, _) = run_fpu_snippet(&[0xEEF8, 0x0A10], &[], &[], 0, 1);
    assert_eq!(cpu.regs.r[0], 0x4000_0000, "EX=0 EN=1");
    // With CPACR cleared the gate still faults first (FPEXC read needs the
    // FPU enabled, like every cp10/11 insn). Single lock for the whole
    // manual section (shadowed `let _g` does NOT drop the guard early —
    // re-locking deadlocks).
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    mem.write16(0x20002000, 0xEEF8);
    mem.write16(0x20002002, 0x0A10);
    cpu.regs.r[15] = 0x20002001;
    let sys = crate::sys();
    cpu.run(sys, &mut mem, 1);
    assert!(cpu.fault.is_some(), "FPEXC without CPACR must fault");
    // Clearing EN via VMSR bricks FPU access (silicon behavior): the next
    // VFP insn faults, including a re-enabling VMSR. (Same lock scope —
    // the boot() below installs a fresh system, no re-lock needed.)
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    let sys = crate::sys();
    mem.write32(0xE000ED88, 0x00F0_0000);
    // vmsr fpexc, r0 (EEE8 0A10) with r0 bit30 clear.
    mem.write16(0x20002000, 0xEEE8);
    mem.write16(0x20002002, 0x0A10);
    cpu.regs.r[0] = 0;
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 1);
    assert!(cpu.fault.is_none(), "VMSR FPEXC itself runs (EN was set)");
    // Now every VFP insn faults, and the EX/EN read shows EN=0.
    mem.write16(0x20002000, 0xEEB7);
    mem.write16(0x20002002, 0x0A00);
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 1);
    assert!(cpu.fault.is_some(), "FPU use with EN=0 must fault");
}

#[test]
fn fpu_fpexc_ex_tracks_lazy() {
    // EX (bit 31) is live LSPACT: set while a lazy frame is outstanding,
    // clear otherwise. NOTE the subtlety this pins: reading FPEXC via
    // VMRS is itself an FPU use, so it completes the pending stacking
    // FIRST (like silicon) and then observes EX=0. To see EX=1, read the
    // model FPCCR directly (no FPU insn involved).
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    let sys = crate::sys();
    mem.write32(0xE000ED88, 0x00F0_0000);
    cpu.regs.s[0] = 0xAAAAAAAA;
    cpu.regs.control |= 4;
    cpu.take_exception(sys, &mut mem, -2);
    assert_ne!(mem.read32(0xE000EF34) & 1, 0, "EX outstanding (MMIO view)");
    // VMRS FPEXC in handler mode: stacks first, so EX reads clear — but
    // the seeded S0 must have landed in the frame, and EN reads set.
    mem.write16(0x20002000, 0xEEF8);
    mem.write16(0x20002002, 0x0A10);
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 1);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.regs.r[0], 0x4000_0000, "EX consumed by the read, EN set");
    let fpcar = mem.read32(0xE000EF38);
    assert_eq!(mem.read32(fpcar), 0xAAAAAAAA, "read stacked thread S0 first");
    assert_eq!(mem.read32(0xE000EF34) & 1, 0, "LSPACT consumed");
}

#[test]
fn dsp_nonzero_regs() {
    // Every FB/FA family with nonzero Rd/Ra (GAS regmatrix.s): the old
    // o2-mask gates faulted all of these (only Rd=0 probes existed).
    // smmla r4,r1,r2,r3=FB51 3402: (2^30)^2>>32 + 1.
    let (cpu, _) = run_snippet(&[0xFB51, 0x3402], &[(1, 0x4000_0000), (2, 0x4000_0000), (3, 1)]);
    assert_eq!(cpu.regs.r[4], 0x1000_0001);
    // smmls r5,r1,r2,r3=FB61 3502: 1 - 2^28 (wrapping).
    let (cpu, _) = run_snippet(&[0xFB61, 0x3502], &[(1, 0x4000_0000), (2, 0x4000_0000), (3, 1)]);
    assert_eq!(cpu.regs.r[5], 0xF000_0001);
    // smmul r6,r1,r2=FB51 F602 (no accumulate).
    let (cpu, _) = run_snippet(&[0xFB51, 0xF602], &[(1, 0x4000_0000), (2, 0x4000_0000)]);
    assert_eq!(cpu.regs.r[6], 0x1000_0000);
    // usada8 r7,r1,r2,r3=FB71 3702: 4x|1-2| + 16 = 20.
    let (cpu, _) = run_snippet(&[0xFB71, 0x3702], &[(1, 0x0101_0101), (2, 0x0202_0202), (3, 16)]);
    assert_eq!(cpu.regs.r[7], 20);
    // usad8 r8,r1,r2=FB71 F802 (no accumulate).
    let (cpu, _) = run_snippet(&[0xFB71, 0xF802], &[(1, 0x0101_0101), (2, 0x0202_0202)]);
    assert_eq!(cpu.regs.r[8], 4);
    // smlad r9,r1,r2,r3=FB21 3902: 1*3 + 2*4 + 16 = 27.
    let (cpu, _) = run_snippet(&[0xFB21, 0x3902], &[(1, 0x0001_0002), (2, 0x0003_0004), (3, 16)]);
    assert_eq!(cpu.regs.r[9], 27);
    // smulwb/smulwt r10,r1,r2 (B/T half select via o2[4]).
    let (cpu, _) = run_snippet(&[0xFB31, 0xFA02], &[(1, 0x0004_0000), (2, 0x0003_FFFF)]);
    assert_eq!(cpu.regs.r[10], 0xFFFF_FFFC, "262144 x -1 >> 16");
    let (cpu, _) = run_snippet(&[0xFB31, 0xFA12], &[(1, 0x0004_0000), (2, 0x0003_0000)]);
    assert_eq!(cpu.regs.r[10], 12, "262144 x 3 >> 16");
    // smlawb/smlawt r11,r1,r2,r3 (accumulate).
    let (cpu, _) = run_snippet(&[0xFB31, 0x3B02], &[(1, 0x0004_0000), (2, 0x0003_FFFF), (3, 0x100)]);
    assert_eq!(cpu.regs.r[11], 0xFC);
    let (cpu, _) = run_snippet(&[0xFB31, 0x3B12], &[(1, 0x0004_0000), (2, 0x0003_0000), (3, 0x100)]);
    assert_eq!(cpu.regs.r[11], 0x10C);
    // smlal r8,r9,r1,r2=FBC1 8902 (lo/hi accumulators, high regs).
    let (cpu, _) = run_snippet(&[0xFBC1, 0x8902], &[(8, 1), (9, 2), (1, 3), (2, 5)]);
    assert_eq!((cpu.regs.r[8], cpu.regs.r[9]), (0x10, 2));
    // FA parallel with nonzero Rd: qadd8 + shadd16.
    let (cpu, _) = run_snippet(&[0xFA85, 0xF416], &[(5, 0x7F7F_7F7F), (6, 0x0101_0101)]);
    assert_eq!(cpu.regs.r[4], 0x7F7F_7F7F, "saturate, not wrap");
    assert_ne!(cpu.regs.xpsr & 0x08000000, 0, "Q set");
    let (cpu, _) = run_snippet(&[0xFA98, 0xF729], &[(8, 0x0004_0004), (9, 0x0002_0002)]);
    assert_eq!(cpu.regs.r[7], 0x0003_0003, "halving add");
}

/// Synthetic interrupt-test image: vector table + main spin + two handlers.
/// IRQ0 -> A @0x110 (cntA++), IRQ1 -> B @0x120 (order=cntA snapshot, cntB++).
/// NMI/SVC/SysTick -> A, HardFault -> B; everything else -> A. Counters at
/// 0x20001000 (A) / 0x20001004 (B), order slot at 0x20001008. With
/// `spinning`, both handlers are branch-to-self loops (for nesting tests,
/// where a returning handler would close the preemption window).
fn irq_test_image(spinning: bool) -> Vec<u8> {
    let mut img = vec![0u8; 0x200];
    fn w32(img: &mut Vec<u8>, off: usize, v: u32) {
        img[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn w16(img: &mut Vec<u8>, off: usize, v: u16) {
        img[off..off + 2].copy_from_slice(&v.to_le_bytes());
    }
    w32(&mut img, 0x00, 0x20002000); // SP
    w32(&mut img, 0x04, 0x00000101); // reset -> main
    for v in 2..16u32 {
        w32(&mut img, (v * 4) as usize, if v == 3 { 0x00000121 } else { 0x00000111 });
    }
    w32(&mut img, 0x40, 0x00000111); // IRQ0 -> A
    w32(&mut img, 0x44, 0x00000121); // IRQ1 -> B
    w16(&mut img, 0x100, 0xE7FE); // main: b .
    if spinning {
        w16(&mut img, 0x110, 0xE7FE);
        w16(&mut img, 0x120, 0xE7FE);
    } else {
        // A: ldr r0,[pc,#8](0x11C); ldr r1,[r0]; adds r1,#1; str r1,[r0]; bx lr
        for (o, v) in [(0x110, 0x4802u16), (0x112, 0x6801), (0x114, 0x3101), (0x116, 0x6001), (0x118, 0x4770), (0x11A, 0xBF00)] {
            w16(&mut img, o, v);
        }
        w32(&mut img, 0x11C, 0x20001000);
        // B: order=cntA; cntB++
        for (o, v) in [(0x120, 0x4804u16), (0x122, 0x6801), (0x124, 0x4A04), (0x126, 0x6011), (0x128, 0x4804), (0x12A, 0x6801), (0x12C, 0x3101), (0x12E, 0x6001), (0x130, 0x4770), (0x132, 0xBF00)] {
            w16(&mut img, o, v);
        }
        w32(&mut img, 0x134, 0x20001000);
        w32(&mut img, 0x138, 0x20001008);
        w32(&mut img, 0x13C, 0x20001004);
    }
    img
}

#[test]
fn irq_priority_order() {
    // B (prio 0x40) must run before A (prio 0xC0) when both pend: the old
    // vector-order selection ran A first (order slot would read 1).
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write32(0xE000E100, 0x3); // ISER0: IRQ0+IRQ1
    mem.write32(0xE000E400, 0x000040C0); // IPR0: A=0xC0 (low), B=0x40 (high)
    sys.p.nvic.borrow_mut().set_intr_pending(0);
    sys.p.nvic.borrow_mut().set_intr_pending(1);
    cpu.run(sys, &mut mem, 300);
    no_fault(&cpu, &mem);
    assert_eq!(mem.read32(0x20001008), 0, "B must run before A (order slot)");
    assert_eq!(mem.read32(0x20001000), 1, "A ran");
    assert_eq!(mem.read32(0x20001004), 1, "B ran");
    assert_eq!(cpu.ipsr, 0, "back in thread mode");
}

#[test]
fn basepri_masks_and_unmasks() {
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write32(0xE000E100, 0x2); // ISER0: IRQ1 only
    mem.write32(0xE000E400, 0x00004000); // IPR0: B=0x40
    sys.p.nvic.borrow_mut().set_intr_pending(1);
    cpu.regs.basepri = 0x40; // masks prio >= 0x40, i.e. B
    cpu.run(sys, &mut mem, 100);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.ipsr, 0, "masked IRQ must not activate");
    assert_eq!(mem.read32(0x20001004), 0, "B handler must not run");
    assert!(sys.p.nvic.borrow().irq_pending(1), "masked IRQ stays pending");
    cpu.regs.basepri = 0; // unmask
    cpu.run(sys, &mut mem, 100);
    no_fault(&cpu, &mem);
    assert_eq!(mem.read32(0x20001004), 1, "B runs after unmask");
    assert!(!sys.p.nvic.borrow().irq_pending(1), "pending cleared by take");
}

#[test]
fn faultmask_blocks_all_but_nmi() {
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write32(0xE000E100, 0x2);
    mem.write32(0xE000E400, 0x00004000);
    sys.p.nvic.borrow_mut().set_intr_pending(1);
    sys.p.nvic.borrow_mut().set_intr_pending(-14); // NMI
    cpu.regs.faultmask = true;
    cpu.run(sys, &mut mem, 200);
    no_fault(&cpu, &mem);
    assert_eq!(mem.read32(0x20001000), 1, "NMI (vector->A) runs through FAULTMASK");
    assert_eq!(mem.read32(0x20001004), 0, "B stays masked");
    assert!(sys.p.nvic.borrow().irq_pending(1), "B still pending");
    cpu.regs.faultmask = false;
    cpu.run(sys, &mut mem, 100);
    no_fault(&cpu, &mem);
    assert_eq!(mem.read32(0x20001004), 1, "B runs after clear");
}

#[test]
fn nesting_preempts_upward() {
    // Spinning A (prio 0xC0) is preempted by B (prio 0x00): ipsr moves 16->17.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(true));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write32(0xE000E100, 0x3);
    mem.write32(0xE000E400, 0x000000C0); // A=0xC0, B=0x00
    sys.p.nvic.borrow_mut().set_intr_pending(0);
    cpu.run(sys, &mut mem, 50);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.ipsr, 16, "in A handler");
    sys.p.nvic.borrow_mut().set_intr_pending(1);
    cpu.run(sys, &mut mem, 50);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.ipsr, 17, "B preempted A");
}

#[test]
fn nesting_blocks_downward() {
    // Spinning B (prio 0x00) is NOT preempted by A (prio 0xC0).
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(true));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write32(0xE000E100, 0x3);
    mem.write32(0xE000E400, 0x000000C0);
    sys.p.nvic.borrow_mut().set_intr_pending(1);
    cpu.run(sys, &mut mem, 50);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.ipsr, 17, "in B handler");
    sys.p.nvic.borrow_mut().set_intr_pending(0);
    cpu.run(sys, &mut mem, 50);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.ipsr, 17, "A must not preempt B");
    assert!(sys.p.nvic.borrow().irq_pending(0), "A stays pending");
}

#[test]
fn tail_chain_reuses_frame() {
    // Direct-drive: take A, pend B, return -> must chain into B (ipsr 17,
    // handler PC, one live entry) instead of unstacking to thread.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write32(0xE000E100, 0x3);
    mem.write32(0xE000E400, 0x000040C0);
    cpu.take_exception(sys, &mut mem, 0);
    assert_eq!(cpu.ipsr, 16);
    sys.p.nvic.borrow_mut().set_intr_pending(1);
    let lr_a = cpu.regs.r[14];
    assert!(cpu.exception_return(sys, &mut mem, lr_a, cpu.regs.r[15] & !1));
    no_fault(&cpu, &mem);
    assert_eq!(cpu.ipsr, 17, "chained into B, not returned to thread");
    assert_eq!(cpu.regs.r[15] & !1, 0x00000120, "at B handler");
    assert_eq!(cpu.exc_stack.len(), 1, "one live entry (pop+push)");
    // B's LR returns to the thread the reused frame came from (F9: the
    // stack held only A, so the frame owner is thread-on-MSP).
    assert_eq!(cpu.regs.r[14], super::EXC_RETURN_MSP, "chained LR targets thread MSP frame");
    let lr_b = cpu.regs.r[14];
    assert!(cpu.exception_return(sys, &mut mem, lr_b, cpu.regs.r[15] & !1));
    no_fault(&cpu, &mem);
    assert_eq!(cpu.ipsr, 0, "back in thread mode");
    assert!(cpu.exc_stack.is_empty(), "stack drained");
}

#[test]
fn sleeponexit_naps_on_thread_return() {
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write32(0xE000E100, 0x1);
    mem.write32(0xE000ED10, 0x2); // SCR.SLEEPONEXIT
    cpu.take_exception(sys, &mut mem, 0);
    let lr = cpu.regs.r[14];
    assert!(!cpu.sleeping, "not asleep mid-handler");
    assert!(cpu.exception_return(sys, &mut mem, lr, cpu.regs.r[15] & !1));
    no_fault(&cpu, &mem);
    assert_eq!(cpu.ipsr, 0);
    assert!(cpu.sleeping, "SLEEPONEXIT naps the core on return to thread");
}

#[test]
fn dwt_cyccnt_counts_instructions() {
    // 100 spinning instructions must read back as exactly 100 counts; with
    // TRCENA clear the counter stays frozen.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    let sys = crate::sys();
    mem.write16(0x20002000, 0xE7FE); // b . spin in SRAM
    cpu.regs.r[15] = 0x20002001;
    mem.write32(0xE000EDFC, 1 << 24); // DEMCR.TRCENA
    mem.write32(0xE0001000, 1); // DWT_CTRL.CYCCNTENA
    cpu.run(sys, &mut mem, 100);
    no_fault(&cpu, &mem);
    assert_eq!(mem.read32(0xE0001004), 100, "CYCCNT tracks instructions 1:1");
    mem.write32(0xE000EDFC, 0); // TRCENA clear freezes
    cpu.run(sys, &mut mem, 50);
    assert_eq!(mem.read32(0xE0001004), 100, "frozen without TRCENA");
}

#[test]
fn msr_basepri_max_semantics() {
    // MSR BASEPRI,R0 (F380 8812) sets the mask; BASEPRI_MAX (F380 8813)
    // only ever raises it; MRS (F3E0 8112) reads it back.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    let sys = crate::sys();
    mem.write16(0x20002000, 0xF380);
    mem.write16(0x20002002, 0x8812); // msr basepri, r0
    mem.write16(0x20002004, 0xF380);
    mem.write16(0x20002006, 0x8813); // msr basepri_max, r0
    mem.write16(0x20002008, 0xF3E0);
    mem.write16(0x2000200A, 0x8112); // mrs r1, basepri
    mem.write16(0x2000200C, 0xE7FE);
    cpu.regs.r[0] = 0x40;
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 1);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.regs.basepri, 0x40, "MSR BASEPRI writes through");
    cpu.regs.r[0] = 0x20;
    cpu.run(sys, &mut mem, 1);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.regs.basepri, 0x40, "BASEPRI_MAX never lowers");
    cpu.regs.r[0] = 0x80;
    cpu.regs.r[15] = 0x20002004; // re-run the MAX insn
    cpu.run(sys, &mut mem, 1);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.regs.basepri, 0x80, "BASEPRI_MAX raises");
    cpu.regs.r[15] = 0x20002008; // MRS insn
    cpu.run(sys, &mut mem, 1);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.regs.r[1], 0x80, "MRS BASEPRI reads back");
}

#[test]
fn svc_taken_and_escalated() {
    // Unmasked SVC takes the SVC vector; FAULTMASK-locked SVC halts
    // (lockup: even HardFault is blocked), it does not ghost-take.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write16(0x20002000, 0xDF00); // svc #0
    mem.write16(0x20002002, 0xE7FE); // b .
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 20);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.ipsr, 0, "SVC handler returned");
    assert_eq!(mem.read32(0x20001000), 1, "SVC vector (A) ran");

    // SD_BLE SVC face (0x60..=0xBF, claimed SVCs complete synchronously,
    // per docs/sd_evt_design.md §4 transport-not-synthesis discipline):
    // firmware calling sd_ble_enable with no handler installed must NOT
    // fault — r0 carries NRF_SUCCESS and execution resumes past the svc.
    // Unclaimed BLE-range SVCs still fall through to the normal SVC path.
    let (mut cpu3, mut mem3) = boot(&irq_test_image(false));
    let sys3 = crate::sys();
    crate::sd_ble::reset_for_test();
    cpu3.deliver_irqs = true;
    mem3.write16(0x20002000, 0xDF60); // svc #0x60 = sd_ble_enable
    mem3.write16(0x20002002, 0xE7FE); // b .
    cpu3.regs.r[15] = 0x20002001;
    cpu3.run(sys3, &mut mem3, 6);
    no_fault(&cpu3, &mem3);
    assert_eq!(cpu3.regs.r[0], 0, "sd_ble_enable returns NRF_SUCCESS");
    assert_eq!(cpu3.regs.r[15] & !1, 0x20002002, "resumes past the svc");
    assert!(crate::sd_ble::is_enabled(), "SD_BLE enabled latch set");

    let (mut cpu2, mut mem2) = boot(&irq_test_image(false));
    let sys2 = crate::sys();
    cpu2.deliver_irqs = true;
    cpu2.regs.faultmask = true;
    mem2.write16(0x20002000, 0xDF00);
    mem2.write16(0x20002002, 0xE7FE);
    cpu2.regs.r[15] = 0x20002001;
    cpu2.run(sys2, &mut mem2, 20);
    assert!(cpu2.fault.is_some(), "FAULTMASK-locked SVC must lock up loudly");
}

#[test]
fn usagefault_nocp_escalates_past_basepri() {
    // NOCP with USGFAULTENA + low UsageFault priority under BASEPRI must
    // escalate to a TAKEN HardFault (vector B), with the NOCP sticky set.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write32(0xE000ED24, 1 << 18); // SHCSR.USGFAULTENA
    mem.write8(0xE000ED1A, 0x80); // SHPR1 UsageFault byte = 0x80
    cpu.regs.basepri = 0x40; // masks 0x80, not HardFault (-1)
    mem.write16(0x20002000, 0xEEF1); // vsqrt s17,s18 (needs CPACR=FPU)
    mem.write16(0x20002002, 0x8AC9);
    mem.write16(0x20002004, 0xE7FE);
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 30);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.ipsr, 0, "HardFault handler returned");
    assert_eq!(mem.read32(0x20001004), 1, "HardFault vector (B) ran");
    assert_ne!(mem.read32(0xE000ED28) & 0x00080000, 0, "NOCP sticky latched");
}

#[test]
fn aircr_vectkey_gate() {
    // AIRCR writes need the VECTKEY in the HIGH halfword; the old gate
    // checked the low half, so PRIGROUP/SYSRESETREQ never applied.
    let _g = lock_boot();
    let (_, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    let sys = crate::sys();
    assert_eq!((mem.read32(0xE000ED0C) >> 8) & 7, 0, "reset PRIGROUP=0");
    mem.write32(0xE000ED0C, 0x05FA0300); // valid key, PRIGROUP=3
    assert_eq!((mem.read32(0xE000ED0C) >> 8) & 7, 3, "PRIGROUP write applies");
    mem.write32(0xE000ED0C, 0x00000300); // bad key: ignored
    assert_eq!((mem.read32(0xE000ED0C) >> 8) & 7, 3, "bad key ignored");
    mem.write32(0xE000ED0C, 0x05FA0000); // back to 0
    assert_eq!((mem.read32(0xE000ED0C) >> 8) & 7, 0);
    let _ = sys;
}

#[test]
fn prigroup_subpriority_ignores_preemption() {
    // PRIGROUP=0: A=0x41 and B=0x40 share group 0x20 — subpriority never
    // preempts, so B must NOT preempt running A (full-value compare would).
    // B=0x3F (group 0x1F) does preempt.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(true));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write32(0xE000ED0C, 0x05FA0000); // PRIGROUP=0 explicitly
    mem.write32(0xE000E100, 0x3);
    mem.write32(0xE000E400, 0x00004041); // A=0x41, B=0x40
    sys.p.nvic.borrow_mut().set_intr_pending(0);
    cpu.run(sys, &mut mem, 50);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.ipsr, 16, "in A handler");
    sys.p.nvic.borrow_mut().set_intr_pending(1);
    cpu.run(sys, &mut mem, 50);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.ipsr, 16, "same-group subpriority must not preempt");
    assert!(sys.p.nvic.borrow().irq_pending(1), "B stays pending");
    mem.write8(0xE000E401, 0x3F); // B=0x3F: group 0x1F < 0x20
    cpu.run(sys, &mut mem, 50);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.ipsr, 17, "lower group preempts");
}

#[test]
fn basepri_masks_by_group() {
    // BASEPRI compares in group space: with PRIGROUP=0, BASEPRI=0x41
    // (group 0x20) masks B=0x40 (group 0x20) — full-value compare would
    // let it through (0x40 < 0x41). BASEPRI=0x60 (group 0x30) releases it.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write32(0xE000ED0C, 0x05FA0000);
    mem.write32(0xE000E100, 0x2);
    mem.write32(0xE000E400, 0x00004000);
    sys.p.nvic.borrow_mut().set_intr_pending(1);
    cpu.regs.basepri = 0x41;
    cpu.run(sys, &mut mem, 100);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.ipsr, 0, "group-masked IRQ must not activate");
    assert_eq!(mem.read32(0x20001004), 0);
    assert!(sys.p.nvic.borrow().irq_pending(1), "stays pending");
    cpu.regs.basepri = 0x60;
    cpu.run(sys, &mut mem, 100);
    no_fault(&cpu, &mem);
    assert_eq!(mem.read32(0x20001004), 1, "released by higher group");
}

#[test]
fn sev_wfe_event_register() {
    // SEV then WFE: event registered -> clear-and-continue, no sleep.
    // Bare WFE: sleeps. Encodings: SEV=BF40, WFE=BF20, b .=E7FE.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write16(0x20002000, 0xBF40);
    mem.write16(0x20002002, 0xBF20);
    mem.write16(0x20002004, 0xE7FE);
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 3);
    no_fault(&cpu, &mem);
    assert!(!cpu.sleeping, "SEV-armed WFE must not sleep");
    assert_eq!(cpu.regs.r[15] & !1, 0x20002004, "past the WFE");

    let (mut cpu2, mut mem2) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    let sys2 = crate::sys();
    cpu2.deliver_irqs = true;
    mem2.write16(0x20002000, 0xBF20);
    mem2.write16(0x20002002, 0xE7FE);
    cpu2.regs.r[15] = 0x20002001;
    cpu2.run(sys2, &mut mem2, 2);
    no_fault(&cpu2, &mem2);
    assert!(cpu2.sleeping, "bare WFE sleeps with no event");
}

#[test]
fn exception_entry_return_sets_event() {
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    assert!(!cpu.event_register, "reset clears the event register");
    cpu.take_exception(sys, &mut mem, 0);
    assert!(cpu.event_register, "entry sets the event");
    cpu.event_register = false;
    let lr = cpu.regs.r[14];
    assert!(cpu.exception_return(sys, &mut mem, lr, cpu.regs.r[15] & !1));
    assert!(cpu.event_register, "return sets the event");
}

#[test]
fn cps_faultmask_target() {
    // CPSID F (B673) sets FAULTMASK, leaving PRIMASK alone; CPSIE I (B662)
    // still drives PRIMASK. Bit 4 is the value, bit 0 the target.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    let sys = crate::sys();
    mem.write16(0x20002000, 0xB673); // cpsid f
    mem.write16(0x20002002, 0xB663); // cpsie f
    mem.write16(0x20002004, 0xB672); // cpsid i
    mem.write16(0x20002006, 0xE7FE);
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 1);
    no_fault(&cpu, &mem);
    assert!(cpu.regs.faultmask, "CPSID F sets FAULTMASK");
    assert_eq!(cpu.regs.primask, 0, "PRIMASK untouched by F-target");
    cpu.run(sys, &mut mem, 1);
    assert!(!cpu.regs.faultmask, "CPSIE F clears FAULTMASK");
    cpu.run(sys, &mut mem, 1);
    assert_eq!(cpu.regs.primask, 1, "CPSID I still drives PRIMASK");
}

#[test]
fn stkalign_pads_and_roundtrips() {
    // CCR reset carries STKALIGN=1: an entry from a 4-but-not-8-aligned PSP
    // pads one word (xPSR bit 9 set) and the return skips it exactly.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    assert_ne!(mem.read32(0xE000ED14) & (1 << 9), 0, "STKALIGN reset set");
    cpu.deliver_irqs = true;
    cpu.regs.control |= 2;
    cpu.regs.psp = 0x200013E4;
    cpu.regs.r[13] = 0x200013E4;
    cpu.regs.r[0] = 0xDEADBEEF;
    cpu.take_exception(sys, &mut mem, 0);
    assert_eq!(cpu.regs.r[13] & 7, 0, "post-push SP 8-aligned");
    // Frame R0 sits at the aligned SP; stacked xPSR flags the pad.
    assert_eq!(mem.read32(0x200013C0), 0xDEADBEEF, "R0 at aligned base");
    assert_ne!(mem.read32(0x200013DC) & 0x200, 0, "xPSR ALIGN bit");
    let lr = cpu.regs.r[14];
    assert_eq!(lr, super::EXC_RETURN_PSP, "PSP thread entry");
    assert!(cpu.exception_return(sys, &mut mem, lr, cpu.regs.r[15] & !1));
    no_fault(&cpu, &mem);
    assert_eq!(cpu.regs.r[13], 0x200013E4, "SP fully restored past pad");
    assert_eq!(cpu.regs.r[0], 0xDEADBEEF, "regs round-tripped");
    assert_eq!(cpu.ipsr, 0);
}

#[test]
fn stkalign_disabled_no_pad() {
    // With STKALIGN cleared, odd-aligned entry stacks tight, no pad flag.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write32(0xE000ED14, 0); // clear STKALIGN
    cpu.regs.control |= 2;
    cpu.regs.psp = 0x200013E4;
    cpu.regs.r[13] = 0x200013E4;
    cpu.regs.r[0] = 0x12345678;
    cpu.take_exception(sys, &mut mem, 0);
    assert_eq!(mem.read32(0x200013C4), 0x12345678, "R0 at unpadded base");
    assert_eq!(mem.read32(0x200013E0) & 0x200, 0, "no ALIGN flag");
    let lr = cpu.regs.r[14];
    assert!(cpu.exception_return(sys, &mut mem, lr, cpu.regs.r[15] & !1));
    no_fault(&cpu, &mem);
    assert_eq!(cpu.regs.r[13], 0x200013E4, "SP restored");
}

#[test]
fn shcsr_active_bits_track_handlers() {
    // SHCSR active bits set on entry, clear on return (PendSV bit 10 here).
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    assert_eq!(mem.read32(0xE000ED24) & (1 << 10), 0, "PENDSVACT reset clear");
    cpu.take_exception(sys, &mut mem, -2);
    assert_ne!(mem.read32(0xE000ED24) & (1 << 10), 0, "PENDSVACT set on entry");
    // Guest-written SHCSR bits elsewhere survive the RMW.
    mem.write32(0xE000ED24, mem.read32(0xE000ED24) | (1 << 16));
    let lr = cpu.regs.r[14];
    assert!(cpu.exception_return(sys, &mut mem, lr, cpu.regs.r[15] & !1));
    no_fault(&cpu, &mem);
    let shcsr = mem.read32(0xE000ED24);
    assert_eq!(shcsr & (1 << 10), 0, "PENDSVACT clear on return");
    assert_ne!(shcsr & (1 << 16), 0, "MEMFAULTENA preserved");
}

#[test]
fn icsr_vectactive_and_set_clear() {
    // ICSR low bits show the live exception; taking PendSV clears a stored
    // PENDSVSET bit instead of going stale.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    assert_eq!(mem.read32(0xE000ED04) & 0x1FF, 0, "thread VECTACTIVE=0");
    cpu.take_exception(sys, &mut mem, -2);
    assert_eq!(mem.read32(0xE000ED04) & 0x1FF, 14, "VECTACTIVE=PendSV");
    let lr = cpu.regs.r[14];
    assert!(cpu.exception_return(sys, &mut mem, lr, cpu.regs.r[15] & !1));
    assert_eq!(mem.read32(0xE000ED04) & 0x1FF, 0, "back to thread");
    // Stored SET bit clears on activation (was stale-forever before).
    mem.write32(0xE000ED04, 1 << 28); // PENDSVSET via ICSR
    assert_ne!(mem.read32(0xE000ED04) & (1 << 28), 0, "SET bit stored");
    cpu.take_exception(sys, &mut mem, -2);
    assert_eq!(mem.read32(0xE000ED04) & (1 << 28), 0, "SET cleared by take");
}

/// Program one MPU SRAM region + enable (PRIVDEFENA for background).
fn mpu_sram_test_setup(mem: &mut FlatMemory) {
    mem.write32(0xE000ED98, 1);
    mem.write32(0xE000ED9C, 0x20000011); // R1: SRAM 128KB base
    mem.write32(0xE000EDA0, 0x01000021); // RW-priv, XN clear (snippets run from SRAM)
    mem.write32(0xE000ED24, mem.read32(0xE000ED24) | (1 << 16)); // MEMFAULTENA
    mem.write32(0xE000ED94, 0x5); // ENABLE|PRIVDEFENA
}

#[test]
fn ldrt_probes_as_unprivileged() {
    // GAS tform.s: ldrt r2,[r5,#4] = F855 2E04 (op2[11:8]==0xE). From a
    // privileged handler, LDRT into a priv-only region must fault
    // (DACCVIOL + MMFAR, escalated past equal-priority MemManage to a
    // taken HardFault); plain LDR to the same address passes; LDRT into
    // a FULL region passes with the right value.
    let _g = lock_boot();
    // Phase A: denied (escalates: MemManage prio 0 can't preempt prio 0).
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write32(0x20000100, 0xA5A5A5A5);
    mpu_sram_test_setup(&mut mem);
    cpu.take_exception(sys, &mut mem, 0);
    mem.write16(0x20002000, 0xF855);
    mem.write16(0x20002002, 0x2E04); // ldrt r2,[r5,#4]
    mem.write16(0x20002004, 0xE7FE);
    cpu.regs.r[5] = 0x200000FC; // +#4 -> 0x20000100
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 6);
    no_fault(&cpu, &mem);
    assert_eq!(mem.read32(0xE000ED28) & 0x82, 0x82, "DACCVIOL+MMARVALID");
    assert_eq!(mem.read32(0xE000ED34), 0x20000100, "MMFAR is the probe");
    assert_eq!(cpu.ipsr, 3, "escalated to taken HardFault");

    // Phase B: privileged LDR to the same address passes in-handler.
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write32(0x20000100, 0xA5A5A5A5);
    mpu_sram_test_setup(&mut mem);
    cpu.take_exception(sys, &mut mem, 0);
    mem.write16(0x20002000, 0xF8D5);
    mem.write16(0x20002002, 0x2004); // ldr.w r2,[r5,#4] (priv, GAS tform)
    mem.write16(0x20002004, 0xE7FE);
    cpu.regs.r[5] = 0x200000FC; // +#4 -> 0x20000100
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 4);
    no_fault(&cpu, &mem);
assert_eq!(cpu.regs.r[2], 0xA5A5A5A5, "privileged load passes");
    assert_eq!(cpu.ipsr, 16, "no fault taken");

    // Phase C: LDRT into a FULL region passes with the value.
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write32(0x20001200, 0x11223344);
    mpu_sram_test_setup(&mut mem);
    mem.write32(0xE000ED98, 3);
    mem.write32(0xE000ED9C, 0x20001003); // R3: scratch 1KB base
    mem.write32(0xE000EDA0, 0x13000013); // FULL access
    cpu.take_exception(sys, &mut mem, 0);
    mem.write16(0x20002000, 0xF855);
    mem.write16(0x20002002, 0x2E04); // ldrt r2,[r5,#4]
    mem.write16(0x20002004, 0xE7FE);
    cpu.regs.r[5] = 0x200011FC; // +#4 -> 0x20001200
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 4);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.regs.r[2], 0x11223344, "unpriv load from FULL region");
    assert_eq!(cpu.ipsr, 16, "no fault taken");
    // P105 exit hygiene: MPU enable + programmed regions live in the
    // INSTALLED model and outlive this test. The next test in this
    // process inherits a live gate into a foreign/fresh map (the
    // deterministic mpu+MWU order repro: the MWU test's watched write
    // faults through the stale gate and never reaches mwu_note).
    // Entry clears (boot()/Cpu::new) run BEFORE this test programs the
    // model, so only an exit disarm closes it.
    mem.write32(0xE000ED94, 0); // MPU CTRL disable (model + latch)
}

#[test]
fn unalign_trp_traps_odd_access() {
    // CCR.UNALIGN_TRP=1: odd halfword access takes UsageFault (UNALIGNED
    // sticky) via vector A; trap clear reads through fine. ldrh.w r2,[r5,#1]
    // is F8B5 2001 (GAS tform probe).
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write32(0xE000ED24, 1 << 18); // USGFAULTENA
    mem.write16(0x20002000, 0xF8B5);
    mem.write16(0x20002002, 0x2001); // ldrh.w r2,[r5,#1]
    mem.write16(0x20002004, 0xE7FE);
    mem.write16(0x20000101, 0xBEEF); // halfword straddling .101/.102
    cpu.regs.r[5] = 0x20000100;
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 4); // trap clear: odd access reads through
    no_fault(&cpu, &mem);
    assert_eq!(cpu.regs.r[2], 0xBEEF, "trap clear: odd access reads through");
    mem.write32(0xE000ED14, 0x208); // keep STKALIGN, add UNALIGN_TRP
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 12);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.ipsr, 0, "UsageFault handler returned");
    assert_eq!(mem.read32(0x20001000), 1, "UsageFault vector (A) ran");
    assert_ne!(mem.read32(0xE000ED28) & (1 << 24), 0, "UNALIGNED sticky");
}

#[test]
fn div0_trp_traps_zero_divisor() {
    // CCR.DIV_0_TRP=1: sdiv by zero takes UsageFault (DIVBYZERO sticky);
    // trap clear returns 0. sdiv r0,r1,r2 is FB91 F0F2 (GAS).
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write32(0xE000ED24, 1 << 18); // USGFAULTENA
    mem.write16(0x20002000, 0xFB91);
    mem.write16(0x20002002, 0xF0F2); // sdiv r0,r1,r2
    mem.write16(0x20002004, 0xE7FE);
    cpu.regs.r[1] = 100;
    cpu.regs.r[2] = 0;
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 4); // trap clear
    no_fault(&cpu, &mem);
    assert_eq!(cpu.regs.r[0], 0, "divide-by-zero returns 0 by default");
    mem.write32(0xE000ED14, 0x210); // STKALIGN + DIV_0_TRP
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 12);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.ipsr, 0, "UsageFault handler returned");
    assert_eq!(mem.read32(0x20001000), 1, "UsageFault vector (A) ran");
    assert_ne!(mem.read32(0xE000ED28) & (1 << 25), 0, "DIVBYZERO sticky");
}

#[test]
fn fpu_fpccr_user_gates_unprivileged() {
    // Unprivileged FPU without FPCCR.USER faults NOCP; with USER=1 the
    // same access works. vmov s0,r0 is EE00 0A10 (existing FPU tests).
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write32(0xE000ED88, 0x00F00000); // CPACR full (guest boot did this)
    mem.write32(0xE000ED24, 1 << 18); // USGFAULTENA
    mem.write16(0x20002000, 0xF380);
    mem.write16(0x20002002, 0x8814); // msr control,r0 (drop privilege next)
    mem.write16(0x20002004, 0xEE00);
    mem.write16(0x20002006, 0x0A10); // vmov s0,r0
    mem.write16(0x20002008, 0xE7FE);
    cpu.regs.r[0] = 0x11111111;
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 1); // MSR only: now unprivileged
    no_fault(&cpu, &mem);
    assert_eq!(cpu.regs.control & 1, 1, "dropped to unprivileged");
    cpu.run(sys, &mut mem, 8); // vmov faults NOCP (USER=0)
    no_fault(&cpu, &mem);
    assert_eq!(mem.read32(0x20001000), 1, "UsageFault vector (A) ran");
    assert_ne!(mem.read32(0xE000ED28) & 0x00080000, 0, "NOCP sticky");
    mem.write32(0xE000EF34, mem.read32(0xE000EF34) | 2); // FPCCR.USER
    cpu.regs.r[0] = 0x11111111; // A clobbered r0; restore the sample
    cpu.regs.r[15] = 0x20002004; // the vmov again, still unprivileged
    cpu.run(sys, &mut mem, 4);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.regs.s[0], 0x11111111, "unpriv FPU works with USER=1");
    assert_eq!(mem.read32(0x20001000), 1, "no second fault");
}

#[test]
fn msr_control_ignores_unprivileged_escalation() {
    // Unprivileged MSR CONTROL cannot clear nPRIV (or flip SPSEL).
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    mem.write16(0x20002000, 0xF380);
    mem.write16(0x20002002, 0x8814); // msr control,r0
    mem.write16(0x20002004, 0xE7FE);
    cpu.regs.r[0] = 1;
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 1);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.regs.control & 1, 1, "privileged drop works");
    assert!(!crate::system::current_privileged(), "context unprivileged");
    cpu.regs.r[0] = 0;
    cpu.run(sys, &mut mem, 1); // unprivileged MSR CONTROL=0: ignored
    no_fault(&cpu, &mem);
    assert_eq!(cpu.regs.control & 1, 1, "nPRIV sticky from unprivileged");
    assert!(!crate::system::current_privileged(), "still unprivileged");
}

#[test]
fn usersetmpend_gates_unprivileged_pends() {
    // CCR.USERSETMPEND=0: unprivileged ICSR PENDSVSET writes are ignored;
    // with the bit set (or privileged) they pend.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    mem.write16(0x20002000, 0xF380);
    mem.write16(0x20002002, 0x8814); // msr control,r0
    mem.write16(0x20002004, 0xE7FE);
    cpu.regs.r[0] = 1;
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 1); // drop to unprivileged (CCR bit1 clear)
    // Unprivileged ICSR write still reaches the model (MPU off here), but
    // the pend itself must be suppressed.
    mem.write32(0xE000ED04, 1 << 28);
    assert!(!sys.p.nvic.borrow().irq_pending(-2), "unpriv pend ignored");
    // Same write privileged pends (control case for the gate).
    cpu.regs.control &= !1;
    crate::system::set_cpu_context(true, false);
    mem.write32(0xE000ED04, 1 << 28);
    assert!(sys.p.nvic.borrow().irq_pending(-2), "privileged pend works");
    sys.p.nvic.borrow_mut().clear_pending(-2);
    // With USERSETMPEND set, unprivileged pends work again.
    mem.write32(0xE000ED14, 0x202); // STKALIGN + USERSETMPEND
    cpu.regs.control |= 1;
    crate::system::set_cpu_context(false, false);
    mem.write32(0xE000ED04, 1 << 28);
    assert!(sys.p.nvic.borrow().irq_pending(-2), "USERSETMPEND allows");
}

#[test]
fn busfault_unmapped_data_access() {
    // Wild data read takes BusFault with PRECISERR + BFARVALID + BFAR when
    // BUSFAULTENA is set (vector A), else escalates to HardFault (vector B).
    // ldr r0,[r1,#0] is 0x6808 (GAS). nRF flash lives at 0x0, so 0x0 is NOT
    // wild here — use 0x30000000 (unmapped on the nRF map).
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write32(0xE000ED24, 1 << 17); // SHCSR.BUSFAULTENA
    mem.write16(0x20002000, 0x6808); // ldr r0,[r1,#0]
    mem.write16(0x20002002, 0xE7FE);
    cpu.regs.r[1] = 0x30000000;
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 12);
    assert!(cpu.fault.is_none(), "cpu faulted: {:?}", cpu.fault);
    assert_eq!(cpu.ipsr, 0, "BusFault handler returned");
    assert_eq!(mem.read32(0x20001000), 1, "BusFault vector (A) ran");
    assert_eq!(mem.read32(0xE000ED28) & 0x8200, 0x8200, "PRECISERR+BFARVALID");
    assert_eq!(mem.read32(0xE000ED38), 0x30000000, "BFAR is the wild address");

    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write16(0x20002000, 0x6808);
    mem.write16(0x20002002, 0xE7FE);
    cpu.regs.r[1] = 0x30000000;
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 12);
    assert!(cpu.fault.is_none(), "cpu faulted: {:?}", cpu.fault);
    assert_eq!(mem.read32(0x20001004), 1, "escalated HardFault (B) ran");
    assert_eq!(mem.read32(0xE000ED28) & 0x8200, 0x8200, "flags latched anyway");
}

#[test]
fn busfault_unmapped_fetch() {
    // Branching into the void faults the fetch (IACCVIOL flavor + BFAR).
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write32(0xE000ED24, 1 << 17); // SHCSR.BUSFAULTENA
    mem.write16(0x20002000, 0x4700); // bx r0
    cpu.regs.r[0] = 0x30000001;
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 12);
    assert!(cpu.fault.is_none(), "cpu faulted: {:?}", cpu.fault);
    assert_eq!(mem.read32(0xE000ED28) & 0x8100, 0x8100, "IACCVIOL+BFARVALID");
    assert_eq!(mem.read32(0xE000ED38), 0x30000000, "BFAR is the wild PC");
    // Handler returns into the wild PC and faults again (each round
    // re-latches the same values), so count-at-least-once, not exactly.
    assert!(mem.read32(0x20001000) >= 1, "BusFault vector (A) ran");
}

#[test]
fn nonbasethrdena_faults_boosted_thread_return() {
    // CCR.NONBASETHRDENA=1: returning to Thread while BASEPRI-boosted
    // faults INVPC into UsageFault (vector A); without the bit the same
    // return succeeds.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write32(0xE000ED24, 1 << 18); // USGFAULTENA
    mem.write32(0xE000ED14, 0x201); // CCR: STKALIGN + NONBASETHRDENA
    cpu.regs.basepri = 0x50; // boosted: thread return must fault
    cpu.take_exception(sys, &mut mem, 0);
    cpu.regs.r[15] = 0x00000100; // park PC at the main spin: the aborted
    // return never ran anything, so without this the replacement fault
    // would stack (and resume into) handler code.
    let lr = cpu.regs.r[14];
    assert!(cpu.exception_return(sys, &mut mem, lr, cpu.regs.r[15] & !1));
    no_fault(&cpu, &mem);
    // Boosted return is cancelled (no unstack) and replaced by the fault:
    // nothing is active anymore, so UsageFault takes fresh and returns.
    // (The handler itself needs a run to execute; budget covers exactly
    // one round — the boost persists, so every later return re-faults
    // the same way, silicon-identical.)
    cpu.run(sys, &mut mem, 7);
    no_fault(&cpu, &mem);
    assert_eq!(mem.read32(0x20001000), 1, "UsageFault vector (A) ran");
    assert_ne!(mem.read32(0xE000ED28) & (1 << 10), 0, "INVPC sticky");
    assert_eq!(cpu.ipsr, 6, "re-faulted into UsageFault (boost persists)");
}

#[test]
fn itm_port0_console_gated() {
    // ITM STIM0 writes reach UART output only with TCR.ITMENA + TER[0].
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    let _ = (cpu, sys);
    crate::system::get_uart_output().lock().unwrap().clear();
    mem.write8(0xE0000000, b'Q'); // gated off: dropped
    assert!(!crate::system::get_uart_output().lock().unwrap().contains('Q'));
    mem.write32(0xE0000E80, 1); // TCR.ITMENA
    mem.write32(0xE0000E00, 1); // TER[0]
    assert_ne!(mem.read32(0xE0000000) & 1, 0, "STIM0 reads ready when on");
    mem.write8(0xE0000000, b'A');
    mem.write8(0xE0000000, b'B');
    let out = crate::system::get_uart_output().lock().unwrap().clone();
    assert!(out.contains('A') && out.contains('B'), "console got {}", out);
}

#[test]
fn dwt_exccnt_counts_takes() {
    // EXCCNT ticks once per taken exception while DEMCR.TRCENA is set.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    cpu.take_exception(sys, &mut mem, 0);
    assert_eq!(mem.read32(0xE000100C), 0, "frozen without TRCENA");
    mem.write32(0xE000EDFC, 1 << 24); // DEMCR.TRCENA
    let lr = cpu.regs.r[14];
    assert!(cpu.exception_return(sys, &mut mem, lr, cpu.regs.r[15] & !1));
    cpu.take_exception(sys, &mut mem, 1);
    let lr = cpu.regs.r[14];
    assert!(cpu.exception_return(sys, &mut mem, lr, cpu.regs.r[15] & !1));
    cpu.take_exception(sys, &mut mem, 1);
    let lr = cpu.regs.r[14];
    assert!(cpu.exception_return(sys, &mut mem, lr, cpu.regs.r[15] & !1));
    no_fault(&cpu, &mem);
    assert_eq!(mem.read32(0xE000100C), 2, "two TRCENA-gated takes counted");
}

#[test]
fn unaligned_device_faults_without_trap() {
    // MPU Device region (TEX=0,C=0,B=1), TRP clear: an odd halfword access
    // still faults (the one observable memory-type rule). strh.w r2,[r5,#1]
    // is F8A5 2001 (GAS: T2 imm12 class, like ldrh.w F8B5 2001).
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write32(0xE000ED24, 1 << 18); // USGFAULTENA
    mem.write32(0xE000ED98, 4);
    mem.write32(0xE000ED9C, 0x20002004); // R4: 32B @0x20002000
    mem.write32(0xE000EDA0, 0x03010009); // FULL, TEX=0,C=0,B=1 Device
    mem.write32(0xE000ED94, 0x5); // ENABLE|PRIVDEFENA
    mem.write16(0x20002100, 0xF8A5);
    mem.write16(0x20002102, 0x2001); // strh.w r2,[r5,#1]
    mem.write16(0x20002104, 0xE7FE);
    cpu.regs.r[5] = 0x20002000;
    cpu.regs.r[2] = 0xBE;
    cpu.regs.r[15] = 0x20002101;
    cpu.run(sys, &mut mem, 12);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.ipsr, 0, "UsageFault handler returned");
    assert_eq!(mem.read32(0x20001000), 1, "UsageFault vector (A) ran");
    assert_ne!(mem.read32(0xE000ED28) & (1 << 24), 0, "UNALIGNED sticky");
    // P105 exit hygiene (same as ldrt_probes_as_unprivileged): the R4
    // Device region + MPU ENABLE outlive this test in the installed
    // model; disarm so the next test inherits no live gate.
    mem.write32(0xE000ED94, 0); // MPU CTRL disable (model + latch)
}

#[test]
fn sevonpend_pending_wakes_wfe() {
    // SEVONPEND=1: a masked-but-enabled pending IRQ wakes WFE (no sleep,
    // no take); with the bit clear the same setup sleeps. WFE=BF20.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    mem.write32(0xE000E100, 0x2); // ISER0: IRQ1
    mem.write32(0xE000E400, 0x00004000); // IPR: B=0x40
    sys.p.nvic.borrow_mut().set_intr_pending(1);
    cpu.regs.basepri = 0x40; // masks B (group-equal)
    mem.write32(0xE000ED10, 1 << 4); // SCR.SEVONPEND
    mem.write16(0x20002000, 0xBF20); // wfe
    mem.write16(0x20002002, 0xE7FE);
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 2);
    no_fault(&cpu, &mem);
    assert!(!cpu.sleeping, "SEVONPEND pending wakes WFE");
    assert_eq!(cpu.regs.r[15] & !1, 0x20002002, "continued past WFE");
    assert_eq!(cpu.ipsr, 0, "masked IRQ not taken");
    mem.write32(0xE000ED10, 0); // SEVONPEND clear
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 2);
    no_fault(&cpu, &mem);
    assert!(cpu.sleeping, "same setup sleeps without SEVONPEND");
}

#[test]
fn stir_pends_and_gates() {
    // STIR write pends the low-9-bit IRQ (taken with ISER); unprivileged
    // writes are ignored without USERSETMPEND; out-of-range ignored; reads 0.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(&irq_test_image(false));
    let sys = crate::sys();
    cpu.deliver_irqs = true;
    assert_eq!(mem.read32(0xE000EF00), 0, "STIR reads 0 (WO)");
    mem.write32(0xE000E100, 0x3); // ISER0: IRQ0+IRQ1
    mem.write32(0xE000EF00, 0); // STIR -> IRQ0
    assert!(sys.p.nvic.borrow().irq_pending(0), "STIR pends");
    cpu.run(sys, &mut mem, 12);
    no_fault(&cpu, &mem);
    assert_eq!(mem.read32(0x20001000), 1, "pended IRQ0 ran (A)");
    // Unprivileged without USERSETMPEND: ignored.
    cpu.regs.control |= 1;
    crate::system::set_cpu_context(false, false);
    mem.write32(0xE000EF00, 1);
    assert!(!sys.p.nvic.borrow().irq_pending(1), "unpriv STIR ignored");
    // Same write privileged works (control case).
    cpu.regs.control &= !1;
    crate::system::set_cpu_context(true, false);
    mem.write32(0xE000EF00, 1);
    assert!(sys.p.nvic.borrow().irq_pending(1), "priv STIR pends");
    sys.p.nvic.borrow_mut().clear_pending(1);
    // With USERSETMPEND, unprivileged works again.
    mem.write32(0xE000ED14, 0x202); // STKALIGN + USERSETMPEND
    cpu.regs.control |= 1;
    crate::system::set_cpu_context(false, false);
    mem.write32(0xE000EF00, 1);
    assert!(sys.p.nvic.borrow().irq_pending(1), "USERSETMPEND allows");
    // Out-of-range (>=97) ignored, no panic, nothing pended.
    let before = sys.p.nvic.borrow().pending_bits();
    mem.write32(0xE000EF00, 500);
    assert_eq!(sys.p.nvic.borrow().pending_bits(), before, "range-checked");
}

#[test]
fn dwt_foldcnt_counts_skipped_slots() {
    // ite eq with Z set: moveq executes, movne folds (FOLDCNT+1, zero
    // guest cycles). GAS: cmp=4289 ite=BF0C moveq=2001 movne=2002.
    let _g = lock_boot();
    let (mut cpu, mut mem) = boot(include_bytes!("../../../blinky/blinky_nrf.bin"));
    let sys = crate::sys();
    mem.write32(0xE000EDFC, 1 << 24); // DEMCR.TRCENA
    mem.write16(0x20002000, 0x4289); // cmp r1,r1 (Z=1)
    mem.write16(0x20002002, 0xBF0C); // ite eq
    mem.write16(0x20002004, 0x2001); // moveq r0,#1
    mem.write16(0x20002006, 0x2002); // movne r0,#2 (skipped)
    mem.write16(0x20002008, 0xE7FE);
    cpu.regs.r[15] = 0x20002001;
    cpu.run(sys, &mut mem, 6);
    no_fault(&cpu, &mem);
    assert_eq!(cpu.regs.r[0], 1, "taken slot executed");
    assert_eq!(mem.read32(0xE0001018), 1, "one folded slot counted");
}









