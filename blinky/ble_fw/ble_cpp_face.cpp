// C++-language BLE face test (xpack arm-none-eabi-g++, same flags as
// the C images, run natively in cargo test).
// Same contract as the C/MPY/JS runners: ENABLE -> CONNECT -> evt_get
// CONNECTED (CENTRAL role) -> READ -> evt_get READ_RSP=87.
// Proves the SVC face is language-agnostic at the machine level: C++
// name mangling, new/delete-free freestanding, and class-method SVC
// dispatch all produce identical SVC bytes.
// Markers: P:BOOT, P:enable:OK, P:connect:OK, P:connected:OK,
// P:read:OK, P:rsp:OK, P:ALL-OK.
typedef unsigned int u32;
typedef unsigned short u16;
typedef unsigned char u8;
#define REG32(a) (*(volatile u32 *)(a))
static constexpr u32 UARTE_ENABLE = 0x40002500u;
static constexpr u32 UARTE_TXD = 0x4000251Cu;

class Uart {
public:
    static void putc(char c) { REG32(UARTE_TXD) = (u32)(u8)c; }
    static void print(const char *s) { while (*s) putc(*s++); }
};

class Svc {
public:
    // NOTE: no call2/call3 dispatcher — g++ reorders the r3/ip moves
    // around the inline asm in ways that clobber the SVC number (the C
    // images use one svc#imm per macro for exactly this reason). Each
    // SVC number gets its own naked-ish static method with the number
    // as an immediate, like the proven C macros.
    static u32 enable(u32 r0, u32 r1) {
        register u32 _r0 __asm__("r0") = r0;
        register u32 _r1 __asm__("r1") = r1;
        __asm__ volatile("svc #0x60" : "+r"(_r0) : "r"(_r1) : "memory");
        return _r0;
    }
    static u32 evt_get(u32 r0, u32 r1) {
        register u32 _r0 __asm__("r0") = r0;
        register u32 _r1 __asm__("r1") = r1;
        __asm__ volatile("svc #0x61" : "+r"(_r0) : "r"(_r1) : "memory");
        return _r0;
    }
    static u32 connect(u32 r0) {
        register u32 _r0 __asm__("r0") = r0;
        __asm__ volatile("svc #0x8C" : "+r"(_r0) :: "memory");
        return _r0;
    }
    static u32 read(u32 r0, u32 r1, u32 r2) {
        register u32 _r0 __asm__("r0") = r0;
        register u32 _r1 __asm__("r1") = r1;
        register u32 _r2 __asm__("r2") = r2;
        __asm__ volatile("svc #0x96" : "+r"(_r0) : "r"(_r1), "r"(_r2) : "memory");
        return _r0;
    }
};

struct Facebufs {
    u8 peer[7];
    u8 evt[32];
    u16 evtlen;
    int fails;
    Facebufs() : fails(0) {
        peer[0] = 1; peer[1] = 0x11; peer[2] = 0x22; peer[3] = 0x33;
        peer[4] = 0x44; peer[5] = 0x55; peer[6] = 0x66;
        evtlen = 0;
    }
};
static Facebufs bufs __attribute__((section(".data")));

#define CHECK(c, n) do { if (c) Uart::print("P:" n ":OK\n"); \
    else { Uart::print("P:" n ":FAIL\n"); bufs.fails++; } } while (0)

int main(void) {
    u32 rc;
    REG32(UARTE_ENABLE) = 8;
    Uart::print("P:BOOT\n");
    rc = Svc::enable(0, 0); CHECK(rc == 0, "enable");
    rc = Svc::connect((u32)bufs.peer); CHECK(rc == 0, "connect");
    { int i; for (i = 0; i < 500; i++) {
        bufs.evtlen = 32;
        rc = Svc::evt_get((u32)bufs.evt, (u32)&bufs.evtlen);
        if (rc == 0 && bufs.evt[0] == 0x10) break;
    } CHECK(bufs.evt[0] == 0x10 && bufs.evt[4 + 16] == 2, "connected"); }
    rc = Svc::read(1, 0x13, 0); CHECK(rc == 0, "read");
    { int i; for (i = 0; i < 500; i++) {
        bufs.evtlen = 32;
        rc = Svc::evt_get((u32)bufs.evt, (u32)&bufs.evtlen);
        if (rc == 0 && bufs.evt[0] == 0x36) break;
    } CHECK(bufs.evt[0] == 0x36 && bufs.evt[16] == 87, "rsp"); }
    if (!bufs.fails) Uart::print("P:ALL-OK\n"); else Uart::print("P:SOME-FAIL\n");
    for (;;) {}
    return 0;
}
extern u32 _estack;
void Reset_Handler(void) __attribute__((naked));
void Reset_Handler(void) {
    __asm__ volatile("ldr r0, =_estack\n" "mov sp, r0\n" "bl main\n" "b .\n");
}
void *_vectors[] __attribute__((section(".vectors"))) = {
    (void *)0x20002000, (void *)((u32)Reset_Handler + 1),
};
