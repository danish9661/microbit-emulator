/* C-language BLE face test (compiled with the Arduino-nRF52 GCC like
 * blinky/ble_fw/ble_conformance.c, run natively in cargo test).
 * Same contract as the JS/MPY runners: ENABLE -> CONNECT -> evt_get
 * CONNECTED (CENTRAL role) -> READ -> evt_get READ_RSP=87.
 * Markers: C:BOOT, C:enable:OK, C:connect:OK, C:connected:OK,
 * C:read:OK, C:rsp:OK, C:ALL-OK.
 */
typedef unsigned int u32;
typedef unsigned short u16;
typedef unsigned char u8;
#define REG32(a) (*(volatile u32 *)(a))
#define UARTE_ENABLE 0x40002500u
#define UARTE_TXD    0x4000251Cu
static void uart_putc(char c) { REG32(UARTE_TXD) = (u32)(u8)c; }
static void uart_print(const char *s) { while (*s) uart_putc(*s++); }
#define SVC2(num, r0, r1) ({ register u32 _r0 __asm__("r0") = (r0); \
    register u32 _r1 __asm__("r1") = (r1); \
    __asm__ volatile("svc #" #num : "+r"(_r0) : "r"(_r1) : "memory"); _r0; })
#define SVC3(num, r0, r1, r2) ({ register u32 _r0 __asm__("r0") = (r0); \
    register u32 _r1 __asm__("r1") = (r1); register u32 _r2 __asm__("r2") = (r2); \
    __asm__ volatile("svc #" #num : "+r"(_r0) : "r"(_r1), "r"(_r2) : "memory"); _r0; })
static u8 peer[7] __attribute__((section(".data")));
static u8 evt[32] __attribute__((section(".data")));
static u16 evtlen __attribute__((section(".data")));
static int fails = 0;
#define CHECK(c, n) do { if (c) uart_print("C:" n ":OK\n"); \
    else { uart_print("C:" n ":FAIL\n"); fails++; } } while (0)
int main(void) {
    u32 rc;
    REG32(UARTE_ENABLE) = 8;
    uart_print("C:BOOT\n");
    rc = SVC2(0x60, 0, 0); CHECK(rc == 0, "enable");
    peer[0] = 1; peer[1] = 0x11; peer[2] = 0x22; peer[3] = 0x33;
    peer[4] = 0x44; peer[5] = 0x55; peer[6] = 0x66;
    rc = SVC2(0x8C, (u32)peer, 0); CHECK(rc == 0, "connect");
    /* evt_get is driver-pumped between slices (see test): spin bound. */
    { int i; for (i = 0; i < 500; i++) {
        evtlen = 32;
        rc = SVC2(0x61, (u32)evt, (u32)&evtlen);
        if (rc == 0 && evt[0] == 0x10) break;
    } CHECK(evt[0] == 0x10 && evt[4 + 16] == 2, "connected"); }
    rc = SVC3(0x96, 1, 0x13, 0); CHECK(rc == 0, "read");
    { int i; for (i = 0; i < 500; i++) {
        evtlen = 32;
        rc = SVC2(0x61, (u32)evt, (u32)&evtlen);
        if (rc == 0 && evt[0] == 0x36) break;
    } CHECK(evt[0] == 0x36 && evt[16] == 87, "rsp"); }
    if (!fails) uart_print("C:ALL-OK\n"); else uart_print("C:SOME-FAIL\n");
    for (;;) {}
    return 0;
}
extern u32 _estack;
void Reset_Handler(void) __attribute__((naked));
void Reset_Handler(void) {
    __asm__ volatile("ldr r0, =_estack\n" "mov sp, r0\n" "bl main\n" "b .\n");
}
void *_vectors[] __attribute__((section(".vectors"))) = {
    (void *)0x20002000, (void *)Reset_Handler + 1,
};
