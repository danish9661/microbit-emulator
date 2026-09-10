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
    /* HFCLK + UARTE enable */
    ldr r0, =0x40000000
    movs r1, #1
    str r1, [r0]
    ldr r0, =0x40002500
    movs r1, #8
    str r1, [r0]
    /* GPIOTE CH0 = event, P0.14 BTN_A, LoToHi: *(0x40006510) = 0x10E01 */
    ldr r0, =0x40006510
    ldr r1, =0x10E01
    str r1, [r0]
    /* TWIM0 ENABLE=6, ADDRESS=0x19 */
    ldr r0, =0x40003500
    movs r1, #6
    str r1, [r0]
    ldr r0, =0x40003588
    movs r1, #0x19
    str r1, [r0]
    /* STARTTX, TXD=0x28 (accel OUT_X_L), STOP */
    ldr r0, =0x40003008
    movs r1, #1
    str r1, [r0]
    ldr r0, =0x4000351C
    movs r1, #0x28
    str r1, [r0]
    ldr r0, =0x40003014
    movs r1, #1
    str r1, [r0]
    /* print SENS:OK */
    ldr r0, =msg_sens
    bl print_cstr
    /* read GPIOTE EVENTS_IN0 -> BTN:1 / BTN:0 */
    ldr r0, =0x40006100
    ldr r0, [r0]
    cmp r0, #0
    beq btn0
    ldr r0, =msg_btn1
    bl print_cstr
    b done
btn0:
    ldr r0, =msg_btn0
    bl print_cstr
done:
    b done

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
msg_sens: .asciz "SENS:OK\n"
msg_btn1: .asciz "BTN:1\n"
msg_btn0: .asciz "BTN:0\n"
