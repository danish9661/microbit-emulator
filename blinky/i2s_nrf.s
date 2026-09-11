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
    str r1, [r0]              /* UARTE enable */

    /* Fill TX buffer 0x20002000 with 0xA0..0xA7 (driver captures these). */
    ldr r0, =0x20002000
    movs r1, #0xA0
    movs r2, #8
txfill:
    strb r1, [r0], #1
    adds r1, r1, #1
    subs r2, r2, #1
    bne txfill

    /* I2S: ENABLE, RXD.PTR/MAXCNT, TXD.PTR/MAXCNT, START */
    ldr r0, =0x40025500
    movs r1, #1
    str r1, [r0]              /* ENABLE */
    ldr r0, =0x40025538
    ldr r1, =0x20001000
    str r1, [r0]              /* RXD.PTR */
    ldr r0, =0x4002553C
    movs r1, #8
    str r1, [r0]              /* RXD.MAXCNT */
    ldr r0, =0x40025540
    ldr r1, =0x20002000
    str r1, [r0]              /* TXD.PTR */
    ldr r0, =0x40025544
    movs r1, #8
    str r1, [r0]              /* TXD.MAXCNT */
    ldr r0, =0x40025000
    movs r1, #1
    str r1, [r0]              /* START */
    ldr r0, =0x40025104
    bl spin_until_set         /* RXPTRUPD */
    ldr r0, =0x40025114
    bl spin_until_set         /* TXPTRUPD */

    /* Rendezvous: wait until the driver moved both buffers (mailbox). */
    ldr r4, =0x20003000
mbox:
    ldr r1, [r4]
    cmp r1, #0
    beq mbox

    /* Verify RX buffer holds the driver's 0x10..0x17 pattern. */
    ldr r0, =0x20001000
    movs r1, #0x10
    movs r2, #8
rxcheck:
    ldrb r3, [r0], #1
    cmp r3, r1
    bne bad
    adds r1, r1, #1
    subs r2, r2, #1
    bne rxcheck

    ldr r0, =0x40025004
    movs r1, #1
    str r1, [r0]              /* STOP */
    ldr r0, =0x40025108
    bl spin_until_set         /* STOPPED */
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
msg_ok: .asciz "I2S:OK\n"
msg_bad: .asciz "I2S:BAD\n"
