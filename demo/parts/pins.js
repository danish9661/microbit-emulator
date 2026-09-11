// micro:bit v2 edge-connector + internal pin map (nRF52833).
// Ports: 0 = P0 (32 pins), 1 = P1. A "wire" in a Wokwi-style setup
// connects a part to one of these EDGE pins by name ("P0".."P20").

export const EDGE = {
  P0:  [0, 2],   // RING0, ADC
  P1:  [0, 3],   // RING1, ADC
  P2:  [0, 4],   // RING2, ADC
  P3:  [0, 31],  // COL3
  P4:  [0, 28],  // COL1
  P5:  [0, 14],  // BTN_A
  P6:  [1, 5],   // COL4
  P7:  [0, 11],  // COL2
  P8:  [0, 10],  // GPIO1
  P9:  [0, 9],   // GPIO2
  P10: [0, 30],  // COL5
  P11: [0, 23],  // BTN_B
  P12: [0, 12],  // GPIO4
  P13: [0, 17],  // SCK (SPI)
  P14: [0, 1],   // MISO (SPI)
  P15: [0, 13],  // MOSI (SPI)
  P16: [1, 2],   // GPIO3
  P19: [0, 26],  // SCL (external I2C)
  P20: [1, 0],   // SDA (external I2C)
};

// 5x5 matrix: rows sink (active low), columns source (active high).
// led on <=> row OUT==0 && col OUT==1.
export const ROWS = [[0,21],[0,22],[0,15],[0,24],[0,19]];
export const COLS = [[0,28],[0,11],[0,31],[1,5],[0,30]];

// Internal (not on edge): buttons, speaker/mic, sensor bus, interface UART.
export const INTERNAL = {
  BTN_A: [0, 14],
  BTN_B: [0, 23],
  SPEAKER: [0, 0],
  MIC_IN: [0, 5],
  RUN_MIC: [0, 20],
  I2C_INT_SCL: [0, 8],   // motion sensors (TWIM1)
  I2C_INT_SDA: [0, 16],
  UART_INT_RX: [0, 6],   // interface MCU (UARTE0)
  UART_INT_TX: [1, 8],
};

// Internal I2C slave addresses (LSM303AGR on TWIM1).
export const LSM303_ACCEL = 0x19;
export const LSM303_MAG = 0x1E;
