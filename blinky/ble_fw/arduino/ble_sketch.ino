/* BLE GATT sketch: Arduino-CLI compiled proof that a real Arduino
 * toolchain build drives the emulator's SVC face. Uses raw svc #imm
 * (S132 numbers) so it needs no SoftDevice headers at compile time;
 * prints verdicts on Serial (UARTE) like every blinky/*_nrf proof.
 */
static unsigned char uuid_svc[4] = {0x0F, 0x18, 0x01, 0x00};
static unsigned short svc_handle = 0;

static unsigned long svc_enable(unsigned long r0, unsigned long r1) {
  register unsigned long _r0 __asm__("r0") = r0;
  register unsigned long _r1 __asm__("r1") = r1;
  __asm__ volatile("svc #0x60" : "+r"(_r0) : "r"(_r1) : "memory");
  return _r0;
}
static unsigned long svc_add(unsigned long r0, unsigned long r1) {
  register unsigned long _r0 __asm__("r0") = r0;
  register unsigned long _r1 __asm__("r1") = r1;
  __asm__ volatile("svc #0xA0" : "+r"(_r0) : "r"(_r1) : "memory");
  return _r0;
}
void setup() {
  Serial.begin(115200);
  Serial.println("ARDUINO-BLE:BOOT");
  unsigned long rc = svc_enable(0, 0);
  Serial.print("ARDUINO-BLE:enable:"); Serial.println(rc, HEX);
  rc = svc_add(1, (unsigned long)uuid_svc);
  (void)rc;
  Serial.println("ARDUINO-BLE:DONE");
}
void loop() {}
