/* Bare-metal C for nRF52833 (micro:bit v2.2): USBD device minimum in C.
 * What the asm proof (usbep_nrf.s) doesn't cover: compiler-generated
 * access to the EPIN/SETUP register block, a .rodata descriptor as the
 * DMA source (flash-to-host path), timeout-bounded event polls with
 * BAD-path markers. Driver pre-injects USBRESET + GET_DESCRIPTOR SETUP.
 * Prints: UBOOT, SETUP:OK/BAD, USBEP:OK/BAD.
 */
typedef unsigned int u32;
typedef unsigned char u8;

#define REG32(a) (*(volatile u32 *)(a))
#define CLOCK_TASKS_HFCLKSTART 0x40000000u
#define UARTE_ENABLE  0x40002500u
#define UARTE_TXD     0x4000251Cu
#define USBD_ENABLE   0x40027500u
#define USBD_PULLUP   0x40027504u
#define USBD_EPINEN   0x40027510u
#define USBD_START_EPIN0 0x40027000u
#define USBD_END_EPIN0   0x40027108u
#define USBD_EP0SETUP    0x4002715Cu
#define USBD_BMREQ       0x40027480u
#define USBD_EPIN0_PTR   0x40027600u
#define USBD_EPIN0_MAX   0x40027604u

static const u8 desc[8] = { 0x12, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x40 };

static void uart_putc(char c) {
    REG32(UARTE_TXD) = (u32)(u8)c;
}
static void uart_print(const char *s) {
    while (*s) uart_putc(*s++);
}

/* Poll *reg until set or budget expires; returns nonzero on set. */
static u32 spin_until_set(u32 reg) {
    volatile u32 budget = 1000000u;
    while (REG32(reg) == 0 && budget > 0) budget--;
    return REG32(reg);
}

int main(void) {
    REG32(CLOCK_TASKS_HFCLKSTART) = 1;
    REG32(UARTE_ENABLE) = 8;
    uart_print("UBOOT\n");
    REG32(USBD_ENABLE) = 1;
    REG32(USBD_PULLUP) = 1;
    if (!spin_until_set(USBD_EP0SETUP)) {
        uart_print("SETUP:TIMEOUT\n");
        for (;;) { }
    }
    REG32(USBD_EP0SETUP) = 0;
    if (REG32(USBD_BMREQ) == 0x80u) {
        uart_print("SETUP:OK\n");
    } else {
        uart_print("SETUP:BAD\n");
        for (;;) { }
    }
    REG32(USBD_EPIN0_PTR) = (u32)desc;
    REG32(USBD_EPIN0_MAX) = 8;
    REG32(USBD_EPINEN) = 1;
    REG32(USBD_START_EPIN0) = 1;
    if (!spin_until_set(USBD_END_EPIN0)) {
        uart_print("USBEP:TIMEOUT\n");
        for (;;) { }
    }
    uart_print("USBEP:OK\n");
    for (;;) { }
    return 0;
}

/* Vector table + reset trampoline (polling only: all vectors default). */
extern u32 _estack;
void Reset_Handler(void) __attribute__((naked));
void Reset_Handler(void) {
    __asm__ volatile(
        "ldr r0, =_estack\n"
        "mov sp, r0\n"
        "bl main\n"
        "b .\n"
    );
}
void Default_Handler(void) { for (;;) { } }

__attribute__((section(".vectors")))
u32 vectors[] = {
    (u32)&_estack, (u32)Reset_Handler + 1,
    (u32)Default_Handler + 1, (u32)Default_Handler + 1,
    0, 0, 0, 0, 0, 0, 0,
    (u32)Default_Handler + 1,
    0, 0,
    (u32)Default_Handler + 1,
    (u32)Default_Handler + 1,
    [16] = (u32)Default_Handler + 1,
};
