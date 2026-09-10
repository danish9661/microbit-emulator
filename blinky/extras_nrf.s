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

    /* SAADC: ENABLE, START, SAMPLE, poll END */
    ldr r0, =0x40007500
    movs r1, #1
    str r1, [r0]
    ldr r0, =0x40007000
    movs r1, #1
    str r1, [r0]              /* START */
    ldr r0, =0x40007004
    str r1, [r0]              /* SAMPLE */
    ldr r0, =0x40007104
    bl spin_until_set
    ldr r0, =msg_saadc
    bl print_cstr

    /* TEMP: START, poll DATARDY, read TEMP */
    ldr r0, =0x4000C000
    movs r1, #1
    str r1, [r0]
    ldr r0, =0x4000C100
    bl spin_until_set
    ldr r0, =0x4000C508
    ldr r0, [r0]
    cmp r0, #0
    beq temp_bad
    ldr r0, =msg_temp
    bl print_cstr
    b rng_go
temp_bad:
    ldr r0, =msg_tempbad
    bl print_cstr
rng_go:
    /* RNG: START, poll VALRDY, read VALUE!=0 */
    ldr r0, =0x4000D000
    movs r1, #1
    str r1, [r0]
    ldr r0, =0x4000D100
    bl spin_until_set
    ldr r0, =0x4000D508
    ldr r0, [r0]
    cmp r0, #0
    beq rng_bad
    ldr r0, =msg_rng
    bl print_cstr
    b pwm_go
rng_bad:
    ldr r0, =msg_rngbad
    bl print_cstr
pwm_go:
    /* PWM0: ENABLE, SEQSTART0, poll SEQEND0 */
    ldr r0, =0x40021500
    movs r1, #1
    str r1, [r0]
    ldr r0, =0x40021008
    str r1, [r0]
    ldr r0, =0x40021110
    bl spin_until_set
    ldr r0, =msg_pwm
    bl print_cstr
done:
    b done

/* r0 = event address; spins until *r0 != 0 (timeout 1M) */
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
msg_saadc:   .asciz "SAADC:OK\n"
msg_temp:    .asciz "TEMP:OK\n"
msg_tempbad: .asciz "TEMP:BAD\n"
msg_rng:     .asciz "RNG:OK\n"
msg_rngbad:  .asciz "RNG:BAD\n"
msg_pwm:     .asciz "PWM:OK\n"
