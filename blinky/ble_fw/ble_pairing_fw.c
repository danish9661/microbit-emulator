/* BLE pairing+GATT firmware for nRF52833 (micro:bit v2.2).
 * Bare-metal C, talks to the SoftDevice SVC face directly (svc #imm):
 * CODAL-BLE-shaped flow — ENABLE -> GATTS battery service+char ->
 * ADV_START -> CONNECT(staged, driver completes) -> CONNECTED drain ->
 * PRIM_DISC/CHAR_DISC/READ/WRITE(staged) -> AUTHENTICATE(staged, driver
 * completes) -> AUTH_STATUS+SEC_UPDATE drain -> CONN_SEC_GET ->
 * DISCONNECT(staged) -> evt drain. No SoftDevice binary needed: the
 * emulator's sd_ble service answers every SVC (native take/complete
 * tests prove the wire; this proves a real compiled image drives the
 * pairing + GATT legs end to end, like a CODAL BLE app would).
 *
 * SVC numbers (S132 ble_ranges.h + ble.h + ble_gap.h + ble_gattc.h):
 *   ENABLE 0x60, EVT_GET 0x61, AUTHENTICATE 0x7E, CONNECT 0x8C,
 *   DISCONNECT 0x76, CONN_SEC_GET 0x87, PRIM_DISC 0x90, READ 0x96,
 *   WRITE 0x98, SERVICE_ADD 0xA0, CHAR_ADD 0xA2.
 * Markers on UARTE TXD (0x4000251C): BLEP:<name>:OK, BLEP:ALL-OK.
 */
typedef unsigned int u32;
typedef unsigned short u16;
typedef unsigned char u8;
#define REG32(a) (*(volatile u32 *)(a))
#define UARTE_ENABLE 0x40002500u
#define UARTE_TXD    0x4000251Cu
static void uart_putc(char c) { REG32(UARTE_TXD) = (u32)(u8)c; }
static void uart_print(const char *s) { while (*s) uart_putc(*s++); }
/* svc #imm dispatcher: real SVC bytes the thumb hook claims. */
#define SVC1(num, r0) ({ register u32 _r0 __asm__("r0") = (r0); \
    __asm__ volatile("svc #" #num : "+r"(_r0) :: "memory"); _r0; })
#define SVC2(num, r0, r1) ({ register u32 _r0 __asm__("r0") = (r0); \
    register u32 _r1 __asm__("r1") = (r1); \
    __asm__ volatile("svc #" #num : "+r"(_r0) : "r"(_r1) : "memory"); _r0; })
#define SVC3(num, r0, r1, r2) ({ register u32 _r0 __asm__("r0") = (r0); \
    register u32 _r1 __asm__("r1") = (r1); register u32 _r2 __asm__("r2") = (r2); \
    __asm__ volatile("svc #" #num : "+r"(_r0) : "r"(_r1), "r"(_r2) : "memory"); _r0; })
static u8 uuid_svc[4] __attribute__((section(".data")));
static u16 svc_handle __attribute__((section(".data")));
static u8 uuid_chr[4] __attribute__((section(".data")));
static u8 attr_tab[20] __attribute__((section(".data")));
static u16 chr_h[4] __attribute__((section(".data")));
static u8 batt[1] __attribute__((section(".data")));
static u8 peer_addr[7] __attribute__((section(".data")));
static u8 range_buf[4] __attribute__((section(".data")));
static u8 wparams[12] __attribute__((section(".data")));
static u8 wbytes[2] __attribute__((section(".data")));
static u8 evt_buf[64] __attribute__((section(".data")));
static u16 evt_len __attribute__((section(".data")));
static u8 sec_buf[2] __attribute__((section(".data")));
static int fails = 0;
#define CHECK(cond, name) do { if (cond) { uart_print("BLEP:" name ":OK\n"); } \
    else { uart_print("BLEP:" name ":FAIL\n"); fails++; } } while (0)
/* Drain evt_get until event id `want` arrives (driver pumps between
 * slices, like silicon firmware spinning on evt arrival). */
static int drain_until(u8 want) {
    int spins;
    u32 rc;
    for (spins = 0; spins < 400; spins++) {
        evt_len = 64;
        rc = SVC2(0x61, (u32)evt_buf, (u32)&evt_len);
        if (rc == 0 && evt_buf[0] == want) return 1;
    }
    return 0;
}
int main(void) {
    u32 rc;
    REG32(UARTE_ENABLE) = 8;
    uart_print("BLEP:BOOT\n");
    rc = SVC2(0x60, 0, 0);
    CHECK(rc == 0, "enable");
    /* GATTS battery service + char (peripheral-role table, like a
     * CODAL BLE app exposing battery before advertising). */
    uuid_svc[0] = 0x0F; uuid_svc[1] = 0x18; uuid_svc[2] = 1; uuid_svc[3] = 0;
    rc = SVC3(0xA0, 1, (u32)uuid_svc, (u32)&svc_handle);
    CHECK(rc == 0 && svc_handle >= 0x10, "service");
    uuid_chr[0] = 0x19; uuid_chr[1] = 0x2A; uuid_chr[2] = 1; uuid_chr[3] = 0;
    batt[0] = 87;
    *(u32 *)&attr_tab[0] = (u32)uuid_chr; *(u32 *)&attr_tab[4] = 0;
    attr_tab[8] = 1; attr_tab[9] = 0; attr_tab[10] = 0; attr_tab[11] = 0;
    attr_tab[12] = 1; attr_tab[13] = 0; attr_tab[14] = 0; attr_tab[15] = 0;
    *(u32 *)&attr_tab[16] = (u32)batt;
    rc = ({ register u32 _r0 __asm__("r0") = svc_handle;
        register u32 _r1 __asm__("r1") = 0;
        register u32 _r2 __asm__("r2") = (u32)attr_tab;
        register u32 _r3 __asm__("r3") = (u32)chr_h;
        __asm__ volatile("svc #0xA2" : "+r"(_r0) : "r"(_r1), "r"(_r2), "r"(_r3) : "memory"); _r0; });
    CHECK(rc == 0 && chr_h[0] > svc_handle, "char");
    /* CONNECT stages (driver completes); wait for CONNECTED. */
    peer_addr[0] = 1; peer_addr[1] = 0x11; peer_addr[2] = 0x22; peer_addr[3] = 0x33;
    peer_addr[4] = 0x44; peer_addr[5] = 0x55; peer_addr[6] = 0x66;
    rc = SVC1(0x8C, (u32)peer_addr);
    CHECK(rc == 0, "connect-stage");
    CHECK(drain_until(0x10), "connected-evt");
    /* GATT legs on the live link: discovery + read + write. */
    rc = SVC3(0x90, 1, 1, 0); CHECK(rc == 0, "prim-stage");
    range_buf[0] = 0x10; range_buf[1] = 0; range_buf[2] = 0x16; range_buf[3] = 0;
    rc = SVC2(0x92, 1, (u32)range_buf); CHECK(rc == 0, "char-stage");
    rc = SVC3(0x96, 1, chr_h[0], 0); CHECK(rc == 0, "read-stage");
    wbytes[0] = 0xAA; wbytes[1] = 0xBB;
    wparams[0] = 1; wparams[1] = 0;
    wparams[2] = chr_h[0] & 0xFF; wparams[3] = (chr_h[0] >> 8) & 0xFF;
    wparams[4] = 0; wparams[5] = 0; wparams[6] = 2; wparams[7] = 0;
    *(u32 *)&wparams[8] = (u32)wbytes;
    rc = SVC2(0x98, 1, (u32)wparams); CHECK(rc == 0, "write-stage");
    /* Pairing leg: AUTHENTICATE stages, driver completes; AUTH_STATUS
     * (0x19) then CONN_SEC_UPDATE (0x1A) arrive; CONN_SEC_GET reports
     * the encrypted mode. This is the CODAL-ble-manager-shaped flow
     * (JustWorks accept) a BLE-enabled image would run. */
    rc = SVC1(0x7E, 1); CHECK(rc == 0, "auth-stage");
    CHECK(drain_until(0x19), "auth-status");
    CHECK(drain_until(0x1A), "sec-update");
    sec_buf[0] = 0; sec_buf[1] = 0;
    rc = SVC2(0x87, 1, (u32)sec_buf);
    CHECK(rc == 0 && sec_buf[0] == 0x21, "conn-sec");
    /* Disconnect cleanly. */
    rc = SVC2(0x76, 1, 19); CHECK(rc == 0, "disc-stage");
    CHECK(drain_until(0x11), "disconnected-evt");
    if (fails == 0) uart_print("BLEP:ALL-OK\n");
    else uart_print("BLEP:SOME-FAIL\n");
    for (;;) {}
    return 0;
}
extern u32 _estack;
void Reset_Handler(void) __attribute__((naked));
void Reset_Handler(void) {
    __asm__ volatile("ldr r0, =_estack\n" "mov sp, r0\n" "bl main\n" "b .\n");
}
/* Vector table (matches blinky_nrf.s layout: SP=0x20002000 at 0x0,
 * Reset+1 at 0x4 — the boot() harness reads SP/PC from there). */
void *_vectors[] __attribute__((section(".vectors"))) = {
    (void *)0x20002000, (void *)Reset_Handler + 1,
};
