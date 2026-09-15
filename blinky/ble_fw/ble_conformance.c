/* BLE conformance firmware: full SVC-face workout with UART verdicts.
 * Compiled with arduino-cli's nRF52 GCC (see plan P100), runs natively
 * in the emulator like every blinky/*_nrf.bin proof (markers + 2nd run).
 *
 * Flow: ENABLE -> SERVICE_ADD -> CHAR_ADD -> VALUE_SET/GET ->
 * CONNECT(staged) -> PRIM_DISC/CHAR_DISC/READ/WRITE(staged) ->
 * SCAN(staged) -> RSSI(staged) -> L2CAP register/TX(staged) ->
 * AUTHENTICATE(staged) -> DISCONNECT(staged) -> evt drain.
 * Each staged job is resolved by the native test driver (take_* ->
 * complete_*), exactly like the JS pump. Markers: BLE:<name>:OK.
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
static u8 val_set[8] __attribute__((section(".data")));
static u8 val_get[8] __attribute__((section(".data")));
static u8 peer_addr[7] __attribute__((section(".data")));
static u8 range_buf[4] __attribute__((section(".data")));
static u8 wparams[12] __attribute__((section(".data")));
static u8 wbytes[2] __attribute__((section(".data")));
static u8 evt_buf[64] __attribute__((section(".data")));
static u16 evt_len __attribute__((section(".data")));
static u8 l2hdr[4] __attribute__((section(".data")));
static u8 l2data[3] __attribute__((section(".data")));
static int fails = 0;
#define CHECK(cond, name) do { if (cond) { uart_print("BLE:" name ":OK\n"); } \
    else { uart_print("BLE:" name ":FAIL\n"); fails++; } } while (0)
int main(void) {
    u32 rc;
    REG32(UARTE_ENABLE) = 8;
    uart_print("BLE:BOOT\n");
    rc = SVC2(0x60, 0, 0);
    CHECK(rc == 0, "enable");
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
    /* VALUE_SET then GET round trip on the value handle. */
    { u8 v = 63; val_set[0] = 1; val_set[1] = 0; val_set[2] = 0; val_set[3] = 0;
      *(u32 *)&val_set[4] = (u32)&v;
      rc = SVC3(0xA4, 0xFFFF, chr_h[0], (u32)val_set); CHECK(rc == 0, "vset"); }
    val_get[0] = 4; val_get[1] = 0; val_get[2] = 0; val_get[3] = 0;
    { static u8 dst[4]; *(u32 *)&val_get[4] = (u32)dst;
      rc = SVC3(0xA5, 0xFFFF, chr_h[0], (u32)val_get);
      CHECK(rc == 0 && dst[0] == 63, "vget"); }
    /* CONNECT stages (driver completes); then GATTC READ stages. */
    peer_addr[0] = 1; peer_addr[1] = 0x11; peer_addr[2] = 0x22; peer_addr[3] = 0x33;
    peer_addr[4] = 0x44; peer_addr[5] = 0x55; peer_addr[6] = 0x66;
    rc = SVC1(0x8C, (u32)peer_addr);
    CHECK(rc == 0, "connect-stage");
    /* Real firmware waits for CONNECTED before any GATTC op (the link
     * does not exist until the driver/bridge completes the connect).
     * Drain evt_get until the CONNECTED id (0x10) arrives. */
    { int spins;
      for (spins = 0; spins < 200; spins++) {
        evt_len = 64;
        rc = SVC2(0x61, (u32)evt_buf, (u32)&evt_len);
        if (rc == 0 && evt_buf[0] == 0x10) break;
      }
      CHECK(evt_buf[0] == 0x10, "connected-evt"); }
    rc = SVC3(0x96, 1, chr_h[0], 0);
    CHECK(rc == 0, "read-stage");
    /* PRIM_DISC + CHAR_DISC + WRITE + SCAN + RSSI + L2CAP + AUTH + DISC. */
    rc = SVC3(0x90, 1, 1, 0); CHECK(rc == 0, "prim-stage");
    range_buf[0] = 0x10; range_buf[1] = 0; range_buf[2] = 0x16; range_buf[3] = 0;
    rc = SVC2(0x92, 1, (u32)range_buf); CHECK(rc == 0, "char-stage");
    /* New RSPs: REL_DISC + ATTR_INFO + UUID_READ + VALS_READ. */
    rc = SVC2(0x91, 1, (u32)range_buf); CHECK(rc == 0, "rel-stage");
    rc = SVC2(0x94, 1, (u32)range_buf); CHECK(rc == 0, "attrinfo-stage");
    { static u8 uuid_le[3]; uuid_le[0] = 0x19; uuid_le[1] = 0x2A; uuid_le[2] = 1;
      rc = SVC3(0x95, 1, (u32)uuid_le, (u32)range_buf); CHECK(rc == 0, "uuidread-stage"); }
    { static u16 hlist[2]; hlist[0] = chr_h[0]; hlist[1] = chr_h[0] + 1;
      rc = SVC3(0x97, 1, (u32)hlist, 2); CHECK(rc == 0, "valsread-stage"); }
    wbytes[0] = 0xAA; wbytes[1] = 0xBB;
    wparams[0] = 1; wparams[1] = 0;
    wparams[2] = chr_h[0] & 0xFF; wparams[3] = (chr_h[0] >> 8) & 0xFF;
    wparams[4] = 0; wparams[5] = 0; wparams[6] = 2; wparams[7] = 0;
    *(u32 *)&wparams[8] = (u32)wbytes;
    rc = SVC2(0x98, 1, (u32)wparams); CHECK(rc == 0, "write-stage");
    rc = SVC1(0x8A, 0); CHECK(rc == 0, "scan-stage");
    { static u8 rss; rc = SVC2(0x8E, 1, (u32)&rss); CHECK(rc == 0, "rssi-stage"); }
    rc = SVC1(0xB0, 0x40); CHECK(rc == 0, "l2cap-reg");
    l2hdr[0] = 3; l2hdr[1] = 0; l2hdr[2] = 0x40; l2hdr[3] = 0;
    l2data[0] = 0xDE; l2data[1] = 0xAD; l2data[2] = 0xBE;
    rc = SVC3(0xB2, 1, (u32)l2hdr, (u32)l2data); CHECK(rc == 0, "l2cap-stage");
    rc = SVC1(0x7E, 1); CHECK(rc == 0, "auth-stage");
    rc = SVC2(0x76, 1, 19); CHECK(rc == 0, "disc-stage");
    /* Drain one event slot: queue is non-empty (many completions
     * pending driver-side in live runs; here just prove evt_get
     * answers NOT_FOUND(5) vs SUCCESS shape correctly). */
    evt_len = 64;
    rc = SVC2(0x61, (u32)evt_buf, (u32)&evt_len);
    CHECK(rc == 0 || rc == 5, "evt-get");
    if (fails == 0) uart_print("BLE:ALL-OK\n");
    else uart_print("BLE:SOME-FAIL\n");
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
