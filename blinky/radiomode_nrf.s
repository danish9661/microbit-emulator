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

    /* Match unit: DAB0 + RXADDRESSES + DACNF ENA0 + MHR mask */
    ldr r0, =0x40001600
    movs r1, #0xEF
    str r1, [r0]              /* DAB[0] */
    ldr r0, =0x40001530
    movs r1, #1
    str r1, [r0]              /* RXADDRESSES: listen 0 */
    ldr r0, =0x40001640
    movs r1, #1
    str r1, [r0]              /* DACNF ENA0 */
    ldr r0, =0x40001644
    ldr r1, =0xBEEF
    str r1, [r0]              /* MHRMATCHCONF */
    ldr r0, =0x40001648
    ldr r1, =0xFFFF
    str r1, [r0]              /* MHRMATCHMAS */

    /* RX: ramp, then wait for the driver to queue a packet (mailbox 1),
       START once (queue non-empty -> stages), spin ADDRESS. */
    ldr r0, =0x40001004
    movs r1, #1
    str r1, [r0]              /* RXEN */
    ldr r4, =0x20003000
mbox1:
    ldr r1, [r4]
    cmp r1, #1
    bne mbox1
    ldr r0, =0x40001008
    movs r1, #1
    str r1, [r0]              /* START stages from the queued packet */
    ldr r0, =0x40001104
    bl spin_until_set         /* ADDRESS (driver completes) */
rxgot:
    /* DEVMATCH + RXMATCH idx 0 + SYNC + MHRMATCH + CRCOK */
    ldr r0, =0x40001114
    ldr r1, [r0]
    cmp r1, #1
    bne bad
    ldr r0, =0x40001408
    ldr r1, [r0]
    cmp r1, #0
    bne bad
    ldr r0, =0x40001168
    ldr r1, [r0]
    cmp r1, #1
    bne bad                   /* SYNC */
    ldr r0, =0x4000115C
    ldr r1, [r0]
    cmp r1, #1
    bne bad                   /* MHRMATCH */
    ldr r0, =0x40001130
    ldr r1, [r0]
    cmp r1, #1
    bne bad                   /* CRCOK (LEN=0 engine-off) */
    ldr r0, =msg_p1
    bl print_cstr

    /* TX: TXEN + START, poll END, expect PHYEND with it */
    ldr r0, =0x40001504
    ldr r1, =0x20001000
    str r1, [r0]              /* PACKETPTR */
    ldr r0, =0x40001000
    movs r1, #1
    str r1, [r0]              /* TXEN */
    ldr r0, =0x40001008
    movs r1, #1
    str r1, [r0]              /* START */
    ldr r0, =0x4000110C
    bl spin_until_set         /* END (driver completes) */
    ldr r0, =0x4000116C
    ldr r1, [r0]
    cmp r1, #1
    bne bad                   /* PHYEND */
    ldr r0, =msg_p2
    bl print_cstr

    /* LR125K mode: same RX loop, expect RATEBOOST on completion */
    ldr r0, =0x40001510
    movs r1, #5
    str r1, [r0]              /* MODE Ble_LR125Kbit */
    ldr r0, =0x40001004
    movs r1, #1
    str r1, [r0]              /* RXEN */
    ldr r4, =0x20003000
    movs r1, #0
    str r1, [r4]              /* re-arm mailbox */
    ldr r0, =0x4000116C
    movs r1, #0
    str r1, [r0]              /* clear phase-1 PHYEND */
    ldr r0, =0x4000110C
    movs r1, #0
    str r1, [r0]              /* clear phase-1 END */
mbox2:
    ldr r1, [r4]
    cmp r1, #2
    bne mbox2
    ldr r0, =0x4000116C
    movs r1, #0
    str r1, [r0]              /* clear stale TX END side */
    ldr r0, =0x4000110C
    movs r1, #0
    str r1, [r0]
    ldr r0, =0x40001008
    movs r1, #1
    str r1, [r0]              /* START stages from the queued packet */
    ldr r0, =0x4000110C
    bl spin_until_set         /* END (driver completes) */
rxgot2:
    ldr r0, =0x40001150
    ldr r1, [r0]
    cmp r1, #1
    bne bad                   /* RATEBOOST in LR125K */
    ldr r0, =msg_p3
    bl print_cstr

    /* DFE block stores + CLEARPATTERN */
    ldr r0, =0x40001900
    movs r1, #2
    str r1, [r0]              /* DFEMODE AoD */
    ldr r0, =0x40001928
    movs r1, #0xAB
    str r1, [r0]              /* SWITCHPATTERN */
    ldr r1, [r0]
    cmp r1, #0xAB
    bne bad
    ldr r0, =0x4000192C
    movs r1, #1
    str r1, [r0]              /* CLEARPATTERN */
    ldr r0, =0x40001928
    ldr r1, [r0]
    cmp r1, #0
    bne bad

    /* Bit counter: BCSTART -> BCMATCH, then BCSTOP */
    ldr r0, =0x4000101C
    movs r1, #1
    str r1, [r0]              /* BCSTART */
    ldr r0, =0x40001128
    bl spin_until_set         /* BCMATCH */
    ldr r0, =0x40001020
    movs r1, #1
    str r1, [r0]              /* BCSTOP */

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
msg_p1: .asciz "RDO:P1\n"
msg_p2: .asciz "RDO:P2\n"
msg_p3: .asciz "RDO:P3\n"
msg_ok: .asciz "RDO:OK\n"
msg_bad: .asciz "RDO:BAD\n"
