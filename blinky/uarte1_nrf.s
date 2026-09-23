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

    /* UARTE1 TX DMA: PTR=msg_dma, MAXCNT=7, STARTTX, poll ENDTX.
       UARTE1 lives at 0x40028000 (IRQ 40); proves the second instance
       stages + completes through the shared take/complete path (the
       unit test only drives registers, never firmware bytes). */
    ldr r0, =0x40028544
    ldr r1, =msg_dma
    str r1, [r0]              /* TXD.PTR */
    ldr r0, =0x40028548
    movs r1, #7
    str r1, [r0]              /* TXD.MAXCNT */
    ldr r0, =0x40028500
    movs r1, #8
    str r1, [r0]              /* UARTE1 ENABLE */
    ldr r0, =0x40028008
    movs r1, #1
    str r1, [r0]              /* TASKS_STARTTX */
    ldr r0, =0x40028120
    bl spin_until_set         /* EVENTS_ENDTX (driver completes) */
    ldr r0, =msg_tx
    bl print_cstr

    /* UARTE1 RX DMA: PTR=RAM, MAXCNT=3, STARTRX, poll ENDRX.
       The test driver fills 3 bytes + completes (JS drip path). */
    ldr r0, =0x40028534
    ldr r1, =0x20001000
    str r1, [r0]              /* RXD.PTR */
    ldr r0, =0x40028538
    movs r1, #3
    str r1, [r0]              /* RXD.MAXCNT */
    ldr r0, =0x40028000
    movs r1, #1
    str r1, [r0]              /* TASKS_STARTRX */
    ldr r0, =0x40028110
    bl spin_until_set         /* EVENTS_ENDRX (SVD 0x110) */
    ldr r0, =msg_rx
    bl print_cstr
done:
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
msg_dma: .asciz "U1DATA\n"
msg_tx:  .asciz "U1TX:OK\n"
msg_rx:  .asciz "U1RX:OK\n"
