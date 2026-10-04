// emulator.d.ts — types for the microbit-emu programmatic entry point.
// Boot real BBC micro:bit v2 firmware on the shipped WASM core.

/** The raw WASM core module (advanced use: registers, peripherals). */
export const wasm: typeof import("./pkg/nrf52833_periph_wasm.js");

/** Parse Intel HEX into a 512KB flash image + UICR byte list. */
export function parseHex(text: string): {
  img: Uint8Array;
  uicr: Array<[number, number]>;
};

export interface Vec3 {
  x: number;
  y: number;
  z: number;
}

export interface BootOptions {
  /** App base address (default 0x1c000, MBR-param direct boot). */
  appBase?: number;
  /** Accelerometer reading in mg (default { x: 0, y: 0, z: 1000 }). */
  accel?: Vec3;
  /** Magnetometer reading in arbitrary units (default { x: 200, y: 0, z: 400 }). */
  mag?: Vec3;
}

export interface Emulator {
  cpu: unknown;
  /** Advance emulation by n instructions + peripheral/time duties. */
  step(n?: number): void;
  /** Currently lit LED indices (0-24) from the model. */
  lit(): number[];
  /** Fault PC, or null when healthy (decoder faults only). */
  fault(): number | null;
}

/**
 * Boot a firmware HEX image (string) as a micro:bit v2 app.
 * Ship your own `.hex`, or use the bundled MicroPython image at
 * `microbit-emu/firmware/micropython-microbit-v2.1.2.hex`.
 */
export function bootEmulator(hexText: string, opts?: BootOptions): Emulator;
