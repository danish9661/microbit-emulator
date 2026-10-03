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

    /* GPIO SENSE: P0.15 SENSE-High latches live (idle-HIGH input) */
    ldr r0, =0x5000073C
    ldr r1, =0x10000
    str r1, [r0]              /* CNF[15] SENSE=High */
    ldr r0, =0x50000520
    ldr r1, =0x8000
    bl spin_until_bits        /* LATCH bit15 */
    ldr r0, =0x50000520
    ldr r1, [r0]
    movs r2, #0x80
    lsls r2, r2, #8           /* 0x8000 */
    tst r1, r2
    beq bad

    /* COMP: Int1V2 ref + TH + READY_SAMPLE short; default 1650 mV
       clears the 900 mV THUP -> Above at once */
    ldr r0, =0x40013500
    movs r1, #2
    str r1, [r0]              /* ENABLE */
    ldr r0, =0x40013530
    ldr r1, =0x3010
    str r1, [r0]              /* THDOWN=16 THUP=48 */
    ldr r0, =0x40013508
    movs r1, #0
    str r1, [r0]              /* REFSEL Int1V2 (1200 mV) */
    ldr r0, =0x40013200
    movs r1, #1
    str r1, [r0]              /* SHORTS READY_SAMPLE */
    ldr r0, =0x40013000
    movs r1, #1
    str r1, [r0]              /* START samples at once */
    ldr r0, =0x40013400
    bl spin_until_set         /* RESULT Above */

    /* QDEC: REPORTPER=1 + REPORTRDY_READCLRACC; driver steps 5 */
    ldr r0, =0x40012500
    movs r1, #1
    str r1, [r0]              /* ENABLE */
    ldr r0, =0x40012510
    movs r1, #1
    str r1, [r0]              /* REPORTPER */
    ldr r0, =0x40012200
    movs r1, #1
    str r1, [r0]              /* SHORTS REPORTRDY_READCLRACC */
    ldr r0, =0x40012000
    movs r1, #1
    str r1, [r0]              /* START */
    ldr r0, =0x40012104
    bl spin_until_set         /* REPORTRDY (driver stepped) */
    ldr r0, =0x40012518
    ldr r1, [r0]
    cmp r1, #5
    bne bad                   /* auto-read ACC==5 */

    /* WDT: CONFIG reset + store; huge CRV; pet; TIMEOUT stays 0 */
    ldr r0, =0x4001050C
    ldr r1, [r0]
    cmp r1, #1
    bne bad                   /* CONFIG reset SLEEP */
    ldr r0, =0x4001050C
    movs r1, #9
    str r1, [r0]
    ldr r1, [r0]
    cmp r1, #9
    bne bad
    ldr r0, =0x40010504
    ldr r1, =0xFFFFFF00
    str r1, [r0]              /* CRV: ~never */
    ldr r0, =0x40010508
    movs r1, #1
    str r1, [r0]              /* RREN RR0 */
    ldr r0, =0x40010000
    movs r1, #1
    str r1, [r0]              /* START */
    ldr r0, =0x40010600
    ldr r1, =0x6E524635
    str r1, [r0]              /* pet RR0 */
    ldr r0, =0x40010100
    ldr r1, [r0]
    cmp r1, #0
    bne bad                   /* no TIMEOUT */

    /* NFCT: ID regs + SHORTS FIELDDET_ACT; SENSE; field -> auto
       SELECTED; STARTTX; driver completes TXFRAMEEND */
    ldr r0, =0x40005590
    ldr r1, =0x11223344
    str r1, [r0]              /* NFCID1_LAST */
    ldr r1, [r0]
    ldr r2, =0x11223344
    cmp r1, r2
    bne bad
    ldr r0, =0x40005500
    movs r1, #1
    str r1, [r0]              /* ENABLE */
    ldr r0, =0x40005200
    movs r1, #1
    str r1, [r0]              /* SHORTS FIELDDETECTED_ACTIVATE */
    ldr r0, =0x40005008
    movs r1, #1
    str r1, [r0]              /* SENSE */
    ldr r0, =0x40005510
    ldr r1, =0x20001000
    str r1, [r0]              /* PACKETPTR */
    ldr r0, =0x40005514
    movs r1, #4
    str r1, [r0]              /* MAXLEN */
    ldr r0, =0x4000514C
    bl spin_until_set         /* SELECTED (field + short) */
    ldr r0, =0x4000500C
    movs r1, #1
    str r1, [r0]              /* STARTTX */
    ldr r0, =0x40005110
    bl spin_until_set         /* TXFRAMEEND (driver completes) */
    ldr r0, =0x40005410
    ldr r1, [r0]
    cmp r1, #2
    bne bad                   /* TAGSTATE back to Idle after completion */

    /* FICR face: PART + ER0 + PRODTEST reset + TAGHEADER0 */
    ldr r0, =0x10000100
    ldr r1, [r0]
    ldr r2, =0x52833
    cmp r1, r2
    bne bad
    ldr r0, =0x10000080
    ldr r1, [r0]
    ldr r2, =0xA5A5A5A5
    cmp r1, r2
    bne bad
    ldr r0, =0x10000350
    ldr r1, [r0]
    ldr r2, =0xFFFFFFFF
    cmp r1, r2
    bne bad
    ldr r0, =0x10000450
    ldr r1, [r0]
    cmp r1, #0x59
    bne bad

    /* NVMC: EEN + ERASEPCR0 stages the page (driver takes, completes
       clean — flash untouched); ICACHE/PARTIALCFG store */
    ldr r0, =0x4001E504
    movs r1, #2
    str r1, [r0]              /* CONFIG EEN */
    ldr r0, =0x4001E510
    ldr r1, =0x70000
    str r1, [r0]              /* ERASEPCR0 page 0x70000 */
    ldr r0, =0x4001E51C
    movs r1, #2
    str r1, [r0]              /* PARTIALCFG */
    ldr r1, [r0]
    cmp r1, #2
    bne bad
    ldr r0, =0x4001E540
    movs r1, #1
    str r1, [r0]              /* ICACHECNF */
    ldr r1, [r0]
    cmp r1, #1
    bne bad

    ldr r0, =msg_ok
    bl print_cstr
done:
    b done
bad:
    ldr r0, =msg_bad
    bl print_cstr
    b done

/* Spin until [r0] has any bit in r1 set (mask passed in r1). */
spin_until_bits:
    push {r1, r2, lr}
    mov r2, r1
    ldr r1, =1000000
sbloop:
    ldr r3, [r0]
    tst r3, r2
    bne sbdone
    subs r1, r1, #1
    bne sbloop
sbdone:
    pop {r1, r2, pc}

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
msg_ok: .asciz "SEN:OK\n"
msg_bad: .asciz "SEN:BAD\n"
