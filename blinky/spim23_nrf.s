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

    /* SPIM2 TX DMA: PTR=msg_spi, MAXCNT=4, STARTTX, poll ENDTX.
       SPIM2 lives at 0x40023000 (IRQ 35); proves the dedicated slot
       stages DMA frames, not just START/STOP (stubs_nrf.s covers the
       handshake only). No slave needed: SPI has no address phase. */
    ldr r0, =0x40023544
    ldr r1, =msg_spi
    str r1, [r0]              /* TXD.PTR */
    ldr r0, =0x40023548
    movs r1, #4
    str r1, [r0]              /* TXD.MAXCNT */
    ldr r0, =0x40023500
    movs r1, #7
    str r1, [r0]              /* ENABLE (master) */
    ldr r0, =0x40023008
    movs r1, #1
    str r1, [r0]              /* TASKS_STARTTX */
    ldr r0, =0x40023120
    bl spin_until_set         /* EVENTS_ENDTX (driver completes) */
    ldr r0, =msg_s2
    bl print_cstr

    /* SPIM3 RX DMA: PTR=RAM, MAXCNT=4, STARTRX, poll ENDRX.
       SPIM3 lives at 0x4002F000 (IRQ 47); proves the RX path on the
       second dedicated slot. The test driver fills MISO + completes. */
    ldr r0, =0x4002F534
    ldr r1, =0x20001000
    str r1, [r0]              /* RXD.PTR */
    ldr r0, =0x4002F538
    movs r1, #4
    str r1, [r0]              /* RXD.MAXCNT */
    ldr r0, =0x4002F500
    movs r1, #7
    str r1, [r0]              /* ENABLE */
    ldr r0, =0x4002F000
    movs r1, #1
    str r1, [r0]              /* TASKS_STARTRX */
    ldr r0, =0x4002F10C
    bl spin_until_set         /* EVENTS_ENDRX */
    ldr r0, =msg_s3
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
msg_spi: .word 0x04030201
msg_s2:  .asciz "S2TX:OK\n"
msg_s3:  .asciz "S3RX:OK\n"
