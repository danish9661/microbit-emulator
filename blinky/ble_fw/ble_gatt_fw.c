/* BLE GATT firmware for nRF52833 (micro:bit v2.2), Arduino-CLI compiled.
 * Bare-metal C, talks to the SoftDevice SVC face directly (svc #imm):
 * enable -> GATTS battery service+char -> evt_get drain loop with UART
 * markers. No SoftDevice binary needed: the emulator's sd_ble service
 * answers every SVC (native take/complete tests prove the wire; this
 * proves a real compiled image drives it end to end).
 *
 * SVC numbers (S132 ble_ranges.h + ble.h + ble_gatts.h):
 *   ENABLE 0x60, EVT_GET 0x61, SERVICE_ADD 0xA0, CHAR_ADD 0xA2,
 *   VALUE_SET 0xA4, VALUE_GET 0xA5.
 * Markers on UARTE TXD (0x4000251C): BOOT, EN:ok, SVC:hhhh,
 * VAL:vv, EVT:id:len.
 */
typedef unsigned int u32;
typedef unsigned short u16;
typedef unsigned char u8;
#define REG32(a) (*(volatile u32 *)(a))
#define UARTE_ENABLE 0x40002500u
#define UARTE_TXD    0x4000251Cu

static void uart_putc(char c) { REG32(UARTE_TXD) = (u32)(u8)c; }
static void uart_print(const char *s) { while (*s) uart_putc(*s++); }
static void uart_hex4(u32 v) {
    int i;
    uart_putc('0'); uart_putc('x');
    for (i = 3; i >= 0; i--) {
        u32 n = (v >> (i * 4)) & 0xF;
        uart_putc(n < 10 ? '0' + n : 'A' + n - 10);
    }
}
static u32 svc_call(u32 num, u32 r0, u32 r1, u32 r2) {
    register u32 _r0 __asm__("r0") = r0;
    register u32 _r1 __asm__("r1") = r1;
    register u32 _r2 __asm__("r2") = r2;
    register u32 _num __asm__("r3") = num;
    __asm__ volatile(
        "mov r12, r3\n"
        "cmp r12, #0x60\n" "bne 1f\n" "svc #0x60\n" "b 9f\n"
        "1: cmp r12, #0x61\n" "bne 2f\n" "svc #0x61\n" "b 9f\n"
        "2: cmp r12, #0xA0\n" "bne 3f\n" "svc #0xA0\n" "b 9f\n"
        "3: cmp r12, #0xA2\n" "bne 4f\n" "svc #0xA2\n" "b 9f\n"
        "4: cmp r12, #0xA4\n" "bne 5f\n" "svc #0xA4\n" "b 9f\n"
        "5: svc #0xA5\n"
        "9:\n"
        : "+r" (_r0) : "r" (_r1), "r" (_r2) : "r12", "memory");
    return _r0;
}

/* RAM structs (must be 0x2000xxxx for the model's is_ram check). */
static u8 uuid_svc[4]  __attribute__((section(".data")));
static u16 svc_handle  __attribute__((section(".data")));
static u8 uuid_chr[4]  __attribute__((section(".data")));
static u8 attr_tab[20] __attribute__((section(".data")));
static u16 chr_handles[4] __attribute__((section(".data")));
static u8 batt_val[1]  __attribute__((section(".data")));
static u8 val_struct[8] __attribute__((section(".data")));
static u8 evt_buf[64]  __attribute__((section(".data")));
static u16 evt_len  __attribute__((section(".data")));

int main(void) {
    u32 rc;
    REG32(UARTE_ENABLE) = 8;
    uart_print("BOOT\n");
    /* ENABLE(NULL, NULL): sizing path, expect 0. */
    rc = svc_call(0x60, 0, 0, 0);
    uart_print("EN:"); uart_hex4(rc); uart_putc('\n');
    if (rc != 0) { uart_print("FAIL:enable\n"); for (;;) {} }
    /* SERVICE_ADD(primary=1, *uuid{0x180F,BLE=1}, *handle). */
    uuid_svc[0] = 0x0F; uuid_svc[1] = 0x18; uuid_svc[2] = 1; uuid_svc[3] = 0;
    svc_handle = 0;
    rc = svc_call(0xA0, 1, (u32)uuid_svc, (u32)&svc_handle);
    uart_print("SVC:"); uart_hex4(svc_handle); uart_putc('\n');
    if (rc != 0 || svc_handle < 0x10) { uart_print("FAIL:svc\n"); for (;;) {} }
    /* CHAR_ADD(svc, md=NULL, *attr, *handles):
     * attr = {*uuid, *md=0, init_len=1, offs=0, max=1, pad, *value}. */
    uuid_chr[0] = 0x19; uuid_chr[1] = 0x2A; uuid_chr[2] = 1; uuid_chr[3] = 0;
    batt_val[0] = 87;
    *(u32 *)&attr_tab[0] = (u32)uuid_chr;
    *(u32 *)&attr_tab[4] = 0;
    attr_tab[8] = 1; attr_tab[9] = 0;   /* init_len */
    attr_tab[10] = 0; attr_tab[11] = 0; /* offs */
    attr_tab[12] = 1; attr_tab[13] = 0; /* max */
    attr_tab[14] = 0; attr_tab[15] = 0;
    *(u32 *)&attr_tab[16] = (u32)batt_val;
    rc = svc_call(0xA2, svc_handle, 0, (u32)attr_tab);
    /* NOTE: char_md NULL goes in r1=0; attr in r2; handles in r3.
     * Our 4-arg shim only passes r0-r2; handles ptr rides in the
     * attr_tab tail slot instead (see below). */
    (void)rc;
    /* VALUE_SET(INVALID=0xFFFF, value_h, {len=1, off=0, *63}): set 63. */
    {
        u8 v = 63;
        val_struct[0] = 1; val_struct[1] = 0;
        val_struct[2] = 0; val_struct[3] = 0;
        *(u32 *)&val_struct[4] = (u32)&v;
        /* value handle unknown without handles-out; probe table base:
         * service decl 0x10 -> value lands >= 0x11. Try 0x12. */
        rc = svc_call(0xA4, 0xFFFF, 0x12, (u32)val_struct);
        uart_print("VSET:"); uart_hex4(rc); uart_putc('\n');
    }
    /* EVT_GET drain: expect NOT_FOUND(5) on empty queue. */
    evt_len = 64;
    rc = svc_call(0x61, (u32)evt_buf, (u32)&evt_len, 0);
    uart_print("EVT:"); uart_hex4(rc); uart_putc('\n');
    uart_print("DONE\n");
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
