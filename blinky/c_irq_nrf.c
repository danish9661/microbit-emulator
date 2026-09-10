/* Bare-metal C for nRF52833 (micro:bit v2.2): GCC-compiled, real NVIC IRQs.
 * Exercises what asm probes don't: compiler-generated prologues, vector
 * table with C handler addresses, TIMER0 IRQ delivery + stacking, UARTE
 * polling print from both thread and handler mode.
 * Prints: BOOT, then TICK:n every TIMER0 compare (PPI-free path).
 */
typedef unsigned int u32;
typedef unsigned short u16;
typedef unsigned char u8;

#define REG32(a) (*(volatile u32 *)(a))
#define CLOCK_TASKS_HFCLKSTART 0x40000000u
#define UARTE_ENABLE  0x40002500u
#define UARTE_TXD     0x4000251Cu
#define UARTE_ENDTX   0x40002120u
#define TIMER_START   0x40008000u
#define TIMER_CC0     0x40008540u
#define TIMER_SHORTS  0x40008200u
#define TIMER_EVCMP0  0x40008140u
#define TIMER_INTEN   0x40008304u
#define P0_DIRSET     0x50000518u
#define P0_OUTSET     0x50000508u
#define P0_OUTCLR     0x5000050Cu
#define NVIC_ISER0    0xE000E100u

static volatile u32 ticks = 0;

static void uart_putc(char c) {
    REG32(UARTE_TXD) = (u32)(u8)c;
}
static void uart_print(const char *s) {
    while (*s) uart_putc(*s++);
}
static void uart_print_u32(u32 v) {
    char buf[11];
    int i = 0;
    if (v == 0) { uart_putc('0'); return; }
    while (v > 0 && i < 10) { buf[i++] = '0' + (v % 10); v /= 10; }
    while (i > 0) uart_putc(buf[--i]);
}

void TIMER0_IRQHandler(void) {
    if (REG32(TIMER_EVCMP0)) {
        REG32(TIMER_EVCMP0) = 0;
        ticks++;
        REG32(P0_OUTSET) = (1u << 21);
        uart_print("TICK:");
        uart_print_u32(ticks);
        uart_print("\n");
        REG32(P0_OUTCLR) = (1u << 21);
    }
}

static void delayish(u32 n) {
    for (volatile u32 i = 0; i < n; i++) __asm__ volatile("" ::: "memory");
}

int main(void) {
    REG32(CLOCK_TASKS_HFCLKSTART) = 1;
    REG32(UARTE_ENABLE) = 8;
    REG32(P0_DIRSET) = (1u << 21);
    uart_print("BOOT\n");
    /* TIMER0: 32-bit, CC0 periodic via SHORTS CLEAR, IRQ on COMPARE0. */
    REG32(TIMER_CC0) = 4000;
    REG32(TIMER_SHORTS) = 1;
    REG32(TIMER_INTEN) = (1u << 16);
    REG32(NVIC_ISER0) = (1u << 8);
    REG32(TIMER_START) = 1;
    __asm__ volatile("cpsie i" ::: "memory");
    /* Spin until 3 ticks (IRQs do the work), then park. */
    while (ticks < 3) { delayish(1000); }
    uart_print("DONE\n");
    for (;;) { }
    return 0;
}

/* Vector table + reset trampoline. */
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
    (u32)Default_Handler + 1, (u32)Default_Handler + 1,          /* NMI/HardFault */
    0, 0, 0, 0, 0, 0, 0,                                        /* 4-10 */
    (u32)Default_Handler + 1,                                    /* SVC 11 */
    0, 0,                                                       /* 12-13 */
    (u32)Default_Handler + 1,                                    /* PendSV 14 */
    (u32)Default_Handler + 1,                                    /* SysTick 15 */
    [16] = (u32)Default_Handler + 1,                             /* IRQ0 POWER_CLOCK */
    [17] = (u32)Default_Handler + 1,                             /* IRQ1 RADIO */
    [18] = (u32)Default_Handler + 1,                             /* IRQ2 UARTE0 */
    [19] = (u32)Default_Handler + 1,                             /* IRQ3 SERIAL0 */
    [20] = (u32)Default_Handler + 1,                             /* IRQ4 SERIAL1 */
    [21] = (u32)Default_Handler + 1,                             /* IRQ5 NFCT */
    [22] = (u32)Default_Handler + 1,                             /* IRQ6 GPIOTE */
    [23] = (u32)Default_Handler + 1,                             /* IRQ7 SAADC */
    [24] = (u32)TIMER0_IRQHandler + 1,                           /* IRQ8 TIMER0 */
};
