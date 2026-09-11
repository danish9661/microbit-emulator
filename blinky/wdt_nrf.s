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
    /* boot counter in retained RAM */
    ldr r0, =0x20001000
    ldr r1, [r0]
    adds r1, r1, #1
    str r1, [r0]
    /* WDT: CRV=1 (~4k instr), RREN=RR0, START. Never petted. */
    ldr r0, =0x40010504
    movs r1, #1
    str r1, [r0]              /* CRV */
    ldr r0, =0x40010508
    str r1, [r0]              /* RREN */
    ldr r0, =0x40010000
    str r1, [r0]              /* START */
spin:
    b spin                    /* reset lands here on expiry */
