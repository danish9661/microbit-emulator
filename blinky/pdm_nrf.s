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

    /* PDM mic: ENABLE + MODE/RATIO/GAIN store + SAMPLE buffer + START.
       Offsets are SVD ground truth (SAMPLE 0x560/0x564, not the old
       0x52C/0x530). Driver fills the buffer + completes END. */
    ldr r0, =0x4001D500
    movs r1, #1
    str r1, [r0]              /* ENABLE */
    ldr r0, =0x4001D508
    movs r1, #0
    str r1, [r0]              /* MODE stereo */
    ldr r0, =0x4001D520
    movs r1, #0
    str r1, [r0]              /* RATIO 64 */
    ldr r0, =0x4001D518
    movs r1, #0x28
    str r1, [r0]              /* GAINL default */
    ldr r1, [r0]
    cmp r1, #0x28
    bne bad
    ldr r0, =0x4001D560
    ldr r1, =0x20001000
    str r1, [r0]              /* SAMPLE.PTR */
    ldr r0, =0x4001D564
    movs r1, #8
    str r1, [r0]              /* SAMPLE.MAXCNT */
    ldr r0, =0x4001D000
    movs r1, #1
    str r1, [r0]              /* START */
    ldr r0, =0x4001D100
    bl spin_until_set         /* STARTED */
    ldr r0, =0x4001D108
    bl spin_until_set         /* END (driver samples) */

    /* Rendezvous: driver filled 8x0x55 at PTR; verify, then STOP. */
    ldr r4, =0x20003000
mbox:
    ldr r1, [r4]
    cmp r1, #0
    beq mbox
    ldr r0, =0x20001000
    movs r1, #0x55
    movs r2, #8
rxcheck:
    ldrb r3, [r0], #1
    cmp r3, r1
    bne bad
    subs r2, r2, #1
    bne rxcheck
    ldr r0, =0x4001D004
    movs r1, #1
    str r1, [r0]              /* STOP */
    ldr r0, =0x4001D104
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
msg_ok: .asciz "PDM:OK\n"
msg_bad: .asciz "PDM:BAD\n"
