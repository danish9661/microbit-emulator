/* BLE roles firmware for nRF52833 (micro:bit v2.2), xpack-GCC compiled.
 * Bare-metal C, talks to the SoftDevice SVC face directly (svc #imm):
 * the ADV/SCAN/whitelist/role-slot legs the pairing image never drives:
 * ENABLE(SC-bit) -> ADV_START(NULL defaults) -> ADV_STOP ->
 * ADV_START(struct: ADV_IND + whitelist 2 addrs) -> ADV IN_USE re-arm
 * refuses -> ADV_STOP -> SCAN_START(NULL) -> SCAN BUSY re-arm refuses ->
 * SCAN_STOP -> SCAN params (window>interval refuses) -> SCAN selective
 * stages -> ADV whitelist while scan holds it refuses IN_USE ->
 * SCAN_STOP -> CONNECT(staged, driver completes) -> CONNECTED drain
 * (CENTRAL role byte) -> SERVICE_CHANGED(staged, driver completes) ->
 * SC_CONFIRM drain (conn head) -> DISCONNECT(staged) -> evt drain.
 *
 * SVC numbers (S132 ble_ranges.h + ble_gap.h + ble_gatts.h):
 *   ENABLE 0x60, EVT_GET 0x61, ADV_START 0x73, ADV_STOP 0x74,
 *   DISCONNECT 0x76, CONNECT 0x8C, SCAN_START 0x8A, SCAN_STOP 0x8B,
 *   SERVICE_CHANGED 0xA7.
 * Return codes (nrf_error.h / ble_err.h):
 *   SUCCESS 0, INVALID_PARAM 7, INVALID_STATE 8, BUSY 17,
 *   WHITELIST_IN_USE 0x3203.
 * Event ids: CONNECTED 0x10, DISCONNECTED 0x11, SC_CONFIRM 0x54.
 * Markers on UARTE TXD (0x4000251C): BLER:<name>:OK, BLER:ALL-OK.
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
/* RAM structs (must be 0x2000xxxx for the model's is_ram check). */
static u8 en_params[8] __attribute__((section(".data")));
static u8 adv_params[16] __attribute__((section(".data")));
static u8 wl_tab[16] __attribute__((section(".data")));
static u8 scan_params[12] __attribute__((section(".data")));
static u8 peer_addr[7] __attribute__((section(".data")));
static u8 evt_buf[64] __attribute__((section(".data")));
static u16 evt_len __attribute__((section(".data")));
static int fails = 0;
#define CHECK(cond, name) do { if (cond) { uart_print("BLER:" name ":OK\n"); } \
    else { uart_print("BLER:" name ":FAIL\n"); fails++; } } while (0)
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
    uart_print("BLER:BOOT\n");
    /* ENABLE with the SC bit set (gatts byte bit0 at params+4). */
    en_params[4] = 1;
    rc = SVC2(0x60, (u32)en_params, 0);
    CHECK(rc == 0, "enable-sc");
    /* ADV_START(NULL) = defaults; then ADV_STOP clears. */
    rc = SVC1(0x73, 0);
    CHECK(rc == 0, "adv-null");
    rc = SVC1(0x74, 0);
    CHECK(rc == 0, "adv-stop");
    /* ADV_START struct: ADV_IND(0), fp ANY(0), whitelist 2 addrs,
     * interval 0x20, timeout 0. Whitelist table: counts at +4/+12. */
    wl_tab[4] = 2; wl_tab[12] = 0;
    adv_params[0] = 0; /* type ADV_IND */
    *(u32 *)&adv_params[1] = 0; /* p_peer NULL */
    adv_params[5] = 0; /* fp ANY */
    *(u32 *)&adv_params[6] = (u32)wl_tab;
    *(u16 *)&adv_params[10] = 0x20; /* interval */
    *(u16 *)&adv_params[12] = 0; /* timeout, no adv data needed */
    adv_params[14] = 0; /* channel mask: all on */
    rc = SVC1(0x73, (u32)adv_params);
    CHECK(rc == 0, "adv-wl");
    /* Re-arm with a whitelist while live: WHITELIST_IN_USE (0x3203). */
    rc = SVC1(0x73, (u32)adv_params);
    CHECK(rc == 0x3203, "adv-inuse");
    rc = SVC1(0x74, 0);
    CHECK(rc == 0, "adv-stop2");
    /* SCAN_START(NULL) stages; re-arm while live: BUSY (17). */
    rc = SVC1(0x8A, 0);
    CHECK(rc == 0, "scan-null");
    rc = SVC1(0x8A, 0);
    CHECK(rc == 17, "scan-busy");
    rc = SVC1(0x8B, 0);
    CHECK(rc == 0, "scan-stop");
    /* SCAN params: window > interval refuses INVALID_PARAM (7). */
    scan_params[0] = 0; /* passive, non-selective */
    *(u32 *)&scan_params[1] = 0; /* p_whitelist NULL */
    *(u16 *)&scan_params[6] = 0x10; /* interval */
    *(u16 *)&scan_params[8] = 0x20; /* window > interval */
    *(u16 *)&scan_params[10] = 0;
    rc = SVC1(0x8A, (u32)scan_params);
    CHECK(rc == 7, "scan-param");
    /* SCAN selective with the 2-addr table stages. */
    scan_params[0] = 0x02; /* selective */
    *(u32 *)&scan_params[1] = (u32)wl_tab;
    *(u16 *)&scan_params[6] = 0x10;
    *(u16 *)&scan_params[8] = 0x10;
    rc = SVC1(0x8A, (u32)scan_params);
    CHECK(rc == 0, "scan-sel");
    /* ADV whitelist while the scan holds the table: IN_USE. */
    rc = SVC1(0x73, (u32)adv_params);
    CHECK(rc == 0x3203, "adv-scan-inuse");
    rc = SVC1(0x8B, 0);
    CHECK(rc == 0, "scan-stop2");
    /* CONNECT stages (driver completes); wait for CONNECTED. */
    peer_addr[0] = 1; peer_addr[1] = 0x11; peer_addr[2] = 0x22; peer_addr[3] = 0x33;
    peer_addr[4] = 0x44; peer_addr[5] = 0x55; peer_addr[6] = 0x66;
    rc = SVC1(0x8C, (u32)peer_addr);
    CHECK(rc == 0, "connect-stage");
    CHECK(drain_until(0x10), "connected-evt");
    /* CENTRAL role byte: evt envelope is {id u16, len u16} then the
     * payload {conn u16, peer type+6, own type+6, role, ...}, so the
     * role lands at evt_buf[4+2+7+7] = evt_buf[20]. */
    CHECK(evt_buf[20] == 2, "role-central");
    /* SERVICE_CHANGED on the empty table: start 0x10 is below the
     * table top (next_handle 0x10 with no services), so the
     * handle-range check fires INVALID_ATTR_HANDLE (0x3003) — the
     * range leg of the header ladder, from real firmware bytes.
     * (The CCCD INVALID_STATE leg needs a table + link; the native
     * service_changed test + the mock 7e leg prove it.) */
    rc = SVC3(0xA7, 1, 0x10, 0x16);
    CHECK(rc == 0x3003, "sc-gate");
    /* DISCONNECT stages; wait for DISCONNECTED. */
    rc = SVC2(0x76, 1, 19);
    CHECK(rc == 0, "disc-stage");
    CHECK(drain_until(0x11), "disconnected-evt");
    if (fails == 0) uart_print("BLER:ALL-OK\n");
    else uart_print("BLER:SOME-FAIL\n");
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
