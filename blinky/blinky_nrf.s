    .syntax unified
    .cpu cortex-m4
    .thumb
    .section .vectors,"a",%progbits
    .word 0x20002000          /* SP */
    .word _start+1            /* Reset */
    .text
    .global _start
    .thumb_func
_start:
    /* HFCLK start: *(0x40000000) = 1 */
    ldr r0, =0x40000000
    movs r1, #1
    str r1, [r0]
    /* UARTE ENABLE=8: *(0x40002500) = 8 */
    ldr r0, =0x40002500
    movs r1, #8
    str r1, [r0]
    /* P0 DIRSET bit21 (ROW1): *(0x50000518) = 1<<21 */
    ldr r0, =0x50000518
    ldr r1, =0x200000
    str r1, [r0]
    /* print "BOOT\n" */
    ldr r0, =msg_boot
    bl print_cstr
    /* blink 2x: OUTSET/OUTCLR + delay + print "BLINK\n" */
    movs r4, #2
blink:
    ldr r0, =0x50000508
    ldr r1, =0x200000
    str r1, [r0]              /* LED on */
    bl delay
    ldr r0, =0x5000050C
    ldr r1, =0x200000
    str r1, [r0]              /* LED off */
    bl delay
    ldr r0, =msg_blink
    bl print_cstr
    subs r4, r4, #1
    bne blink
done:
    b done

/* r0 = cstring pointer; clobbers r1-r3 */
print_cstr:
    push {r4, lr}
    mov r4, r0
ploop:
    ldrb r1, [r4], #1
    cbz r1, pdone
    ldr r2, =0x4000251C       /* UARTE TXD */
    str r1, [r2]
    b ploop
pdone:
    pop {r4, pc}

delay:
    ldr r2, =200000
dloop:
    subs r2, r2, #1
    bne dloop
    bx lr

    .section .rodata
msg_boot:  .asciz "BOOT\n"
msg_blink: .asciz "BLINK\n"
