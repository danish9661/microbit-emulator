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
    /* print "HELLO" (UART TXD polling, no DMA) */
    ldr r0, =msg_hello
    bl print_cstr
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

    .section .rodata
msg_hello: .asciz "HELLO\n"
