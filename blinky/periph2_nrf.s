    .syntax unified
    .cpu cortex-m4
    .thumb
    .section .vectors,"a",%progbits
    .word 0x20002000
    .word _start+1
    .text
    .global _start
    .thumb_func
_start:
    ldr r0, =0x40000000
    movs r1, #1
    str r1, [r0]              /* HFCLK */
    ldr r0, =0x40002500
    movs r1, #8
    str r1, [r0]              /* UARTE0 enable (console) */

    /* TIMER0 counter mode: MODE=1, CC0=3, three COUNTs -> COMPARE0 */
    ldr r0, =0x40008504
    movs r1, #1
    str r1, [r0]              /* MODE Counter */
    ldr r0, =0x40008540
    movs r1, #3
    str r1, [r0]              /* CC0 */
    ldr r0, =0x40008000
    movs r1, #1
    str r1, [r0]              /* START */
    ldr r0, =0x40008008
    movs r1, #1
    str r1, [r0]
    str r1, [r0]
    str r1, [r0]              /* COUNT x3 */
    ldr r0, =0x40008140
    bl spin_until_set         /* COMPARE0 */
    ldr r0, =0x4000804C
    movs r1, #1
    str r1, [r0]              /* CAPTURE[3] */
    ldr r0, =0x4000854C
    ldr r1, [r0]
    cmp r1, #3
    bne bad
    ldr r0, =0x40008504
    movs r1, #0
    str r1, [r0]              /* MODE Timer back */

    /* RTC0: EVTENSET, TRIGOVRFLW parks at 0xFFFFF0, CLEAR zeroes */
    ldr r0, =0x4000B344
    ldr r1, =0x10002
    str r1, [r0]              /* EVTENSET OVRFLW+COMPARE0 */
    ldr r0, =0x4000B340
    ldr r1, [r0]
    ldr r2, =0x10002
    cmp r1, r2
    bne bad
    ldr r0, =0x4000B000
    movs r1, #1
    str r1, [r0]              /* START */
    ldr r0, =0x4000B00C
    movs r1, #1
    str r1, [r0]              /* TRIGOVRFLW */
    ldr r0, =0x4000B504
    ldr r1, [r0]
    ldr r2, =0xFFFFF0
    cmp r1, r2
    bne bad
    ldr r0, =0x4000B008
    movs r1, #1
    str r1, [r0]              /* CLEAR */
    ldr r0, =0x4000B504
    ldr r1, [r0]
    cmp r1, #0
    bne bad

    /* PWM1: config block + LOOP.CNT=1 pair + NEXTSTEP */
    ldr r0, =0x40021500
    movs r1, #1
    str r1, [r0]              /* ENABLE */
    ldr r0, =0x40021508
    ldr r1, =1000
    str r1, [r0]              /* COUNTERTOP */
    ldr r0, =0x40021520
    ldr r1, =0x20001000
    str r1, [r0]              /* SEQ0.PTR */
    ldr r0, =0x40021524
    movs r1, #4
    str r1, [r0]              /* SEQ0.CNT */
    ldr r1, [r0]
    cmp r1, #4
    bne bad
    ldr r0, =0x40021514
    movs r1, #1
    str r1, [r0]              /* LOOP.CNT */
    ldr r0, =0x40021008
    movs r1, #1
    str r1, [r0]              /* SEQSTART0 */
    ldr r0, =0x40021110
    bl spin_until_set         /* SEQEND0 */
    ldr r0, =0x4002111C
    ldr r1, [r0]
    cmp r1, #0
    bne bad                   /* loops still pending */
    ldr r0, =0x4002100C
    movs r1, #1
    str r1, [r0]              /* SEQSTART1 */
    ldr r0, =0x4002111C
    bl spin_until_set         /* LOOPSDONE */
    ldr r0, =0x40021114
    movs r1, #0
    str r1, [r0]              /* clear SEQEND1 */
    ldr r0, =0x40021010
    movs r1, #1
    str r1, [r0]              /* NEXTSTEP */
    ldr r0, =0x40021114
    bl spin_until_set         /* SEQEND1 again */

    /* UARTE0 full face: CONFIG + PSEL + STARTED edges + FLUSHRX + RXTO */
    ldr r0, =0x4000256C
    ldr r1, =0x11F
    str r1, [r0]              /* CONFIG all */
    ldr r1, [r0]
    ldr r2, =0x11F
    cmp r1, r2
    bne bad
    ldr r0, =0x40002508
    movs r1, #5
    str r1, [r0]              /* PSEL.RTS */
    ldr r1, [r0]
    cmp r1, #5
    bne bad
    ldr r0, =0x40002000
    movs r1, #1
    str r1, [r0]              /* STARTRX */
    ldr r0, =0x4000214C
    bl spin_until_set         /* RXSTARTED */
    /* (driver drips one byte between slices) */
    ldr r0, =0x40002108
    bl spin_until_set         /* RXDRDY */
    ldr r0, =0x4000202C
    movs r1, #1
    str r1, [r0]              /* FLUSHRX */
    ldr r0, =0x40002108
    ldr r1, [r0]
    cmp r1, #0
    bne bad                   /* flushed */
    ldr r0, =0x40002008
    movs r1, #1
    str r1, [r0]              /* STARTTX (MAXCNT 0: instant) */
    ldr r0, =0x40002150
    bl spin_until_set         /* TXSTARTED */
    ldr r0, =0x40002004
    movs r1, #1
    str r1, [r0]              /* STOPRX (empty -> RXTO) */
    ldr r0, =0x40002144
    bl spin_until_set         /* RXTO */

    /* TWIM1 regs + SPIM2 config regs (no bus traffic, reads only) */
    ldr r0, =0x40004508
    movs r1, #3
    str r1, [r0]              /* PSEL.SCL */
    ldr r1, [r0]
    cmp r1, #3
    bne bad
    ldr r0, =0x40004524
    ldr r1, =0x06400000
    str r1, [r0]              /* FREQUENCY K400 */
    ldr r1, [r0]
    ldr r2, =0x06400000
    cmp r1, r2
    bne bad
    ldr r0, =0x40023554
    movs r1, #5
    str r1, [r0]              /* SPIM2 CONFIG */
    ldr r1, [r0]
    cmp r1, #5
    bne bad
    ldr r0, =0x400235C0
    movs r1, #0xAB
    str r1, [r0]              /* SPIM2 ORC */
    ldr r1, [r0]
    cmp r1, #0xAB
    bne bad

    /* SAADC: START -> STATUS busy; CAL -> CALIBRATEDONE; STOP idles */
    ldr r0, =0x40007500
    movs r1, #1
    str r1, [r0]              /* ENABLE */
    ldr r0, =0x40007000
    movs r1, #1
    str r1, [r0]              /* START */
    ldr r0, =0x40007400
    bl spin_until_set         /* STATUS busy */
    ldr r0, =0x4000700C
    movs r1, #1
    str r1, [r0]              /* CALIBRATEOFFSET (SVD) */
    ldr r0, =0x40007110
    bl spin_until_set         /* CALIBRATEDONE */
    ldr r0, =0x40007008
    movs r1, #1
    str r1, [r0]              /* STOP */
    ldr r0, =0x40007400
    ldr r1, [r0]
    cmp r1, #0
    bne bad                   /* idle after STOP */

    /* SPIS0 slave: ENABLE + buffers + ACQUIRE, poll END (driver
       exchanges 3 bytes into a 1-byte RX buffer -> OVERFLOW). */
    ldr r0, =0x40003500
    movs r1, #2
    str r1, [r0]              /* SPIS ENABLE */
    ldr r0, =0x40003534
    ldr r1, =0x20001000
    str r1, [r0]              /* RXD.PTR */
    ldr r0, =0x40003538
    movs r1, #1
    str r1, [r0]              /* RXD.MAXCNT=1 */
    ldr r0, =0x40003544
    ldr r1, =0x20002000
    str r1, [r0]              /* TXD.PTR */
    ldr r0, =0x40003548
    movs r1, #1
    str r1, [r0]              /* TXD.MAXCNT=1 */
    ldr r0, =0x40003024
    movs r1, #1
    str r1, [r0]              /* ACQUIRE */
    ldr r0, =0x40003128
    bl spin_until_set         /* ACQUIRED */
    ldr r0, =0x40003104
    bl spin_until_set         /* END (driver exchanges) */
    ldr r0, =0x40003440
    ldr r1, [r0]
    cmp r1, #3
    bne bad                   /* OVERREAD+OVERFLOW */

    /* GPIOTE CH0 event on P0.14 rising (driver presses between slices) */
    ldr r0, =0x40006510
    ldr r1, =0x10E01
    str r1, [r0]              /* event, P0.14, LoToHi */
    ldr r0, =0x40006100
    bl spin_until_set         /* IN0 */

    /* EGU0 INTEN + TRIGGER0; RNG CONFIG + START */
    ldr r0, =0x40014300
    ldr r1, =0xFFFF
    str r1, [r0]              /* INTEN all */
    ldr r1, [r0]
    ldr r2, =0xFFFF
    cmp r1, r2
    bne bad
    ldr r0, =0x40014000
    movs r1, #1
    str r1, [r0]              /* TRIGGER0 */
    ldr r0, =0x40014100
    bl spin_until_set         /* TRIGGERED0 */
    ldr r0, =0x4000D504
    movs r1, #1
    str r1, [r0]              /* RNG DERCEN */
    ldr r1, [r0]
    cmp r1, #1
    bne bad
    ldr r0, =0x4000D000
    movs r1, #1
    str r1, [r0]              /* START */
    ldr r0, =0x4000D100
    bl spin_until_set         /* VALRDY */
    ldr r0, =0x4000D508
    ldr r1, [r0]
    cmp r1, #0
    beq bad                   /* VALUE nonzero */

    ldr r0, =msg_ok
    bl print_cstr
done:
    b done
bad:
    ldr r0, =msg_bad
    bl print_cstr
    b done

spin_until_set:
    push {r1, r2, lr}
    ldr r1, =1000000
sloop:
    ldr r2, [r0]
    cmp r2, #0
    bne sdone
    subs r1, r1, #1
    bne sloop
sdone:
    pop {r1, r2, pc}

print_cstr:
    push {r4, lr}
    mov r4, r0
ploop:
    ldrb r1, [r4], #1
    cbz r1, pdone
    ldr r2, =0x4000251C
    str r1, [r2]
    b ploop
pdone:
    pop {r4, pc}

    .section .rodata
msg_ok: .asciz "PER:OK\n"
msg_bad: .asciz "PER:BAD\n"
