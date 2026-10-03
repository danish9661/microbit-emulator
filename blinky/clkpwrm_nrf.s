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

    /* CAL -> EVENTS_DONE */
    ldr r0, =0x40000010
    movs r1, #1
    str r1, [r0]              /* TASKS_CAL */
    ldr r0, =0x4000010C
    bl spin_until_set         /* EVENTS_DONE */

    /* CTSTART -> CTSTARTED, then CTSTOP -> CTSTOPPED */
    ldr r0, =0x40000538
    movs r1, #0
    str r1, [r0]              /* CTIV */
    ldr r0, =0x40000014
    movs r1, #1
    str r1, [r0]              /* TASKS_CTSTART */
    ldr r0, =0x40000128
    bl spin_until_set         /* EVENTS_CTSTARTED */
    ldr r0, =0x40000018
    movs r1, #1
    str r1, [r0]              /* TASKS_CTSTOP */
    ldr r0, =0x4000012C
    bl spin_until_set         /* EVENTS_CTSTOPPED */

    /* INTEN: DONE + CTSTARTED, read back the mask */
    ldr r0, =0x40000304
    ldr r1, =0x408
    str r1, [r0]
    ldr r0, =0x40000304
    ldr r1, [r0]
    ldr r2, =0x408
    cmp r1, r2
    bne bad

    /* Debounce + trace config store */
    ldr r0, =0x40000528
    movs r1, #5
    str r1, [r0]
    ldr r1, [r0]
    cmp r1, #5
    bne bad
    ldr r0, =0x4000055C
    movs r1, #2
    str r1, [r0]
    ldr r1, [r0]
    cmp r1, #2
    bne bad

    /* Power mode tasks (no status on silicon either, must not fault) */
    ldr r0, =0x40000078
    movs r1, #1
    str r1, [r0]              /* CONSTLAT */
    ldr r0, =0x4000007C
    movs r1, #1
    str r1, [r0]              /* LOWPWR */

    /* RAM section power: CLR then SET around POWER */
    ldr r0, =0x40000908
    movs r1, #1
    str r1, [r0]              /* RAM0.POWERCLR */
    ldr r0, =0x40000900
    ldr r1, [r0]
    cmp r1, #0
    bne bad
    ldr r0, =0x40000904
    movs r1, #1
    str r1, [r0]              /* RAM0.POWERSET */
    ldr r0, =0x40000900
    ldr r1, [r0]
    cmp r1, #1
    bne bad

    /* SYSTEMOFF arms + reads back (driver takes it, like every pump) */
    ldr r0, =0x40000500
    movs r1, #1
    str r1, [r0]
    ldr r1, [r0]
    cmp r1, #1
    bne bad

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
msg_ok: .asciz "CLK:OK\n"
msg_bad: .asciz "CLK:BAD\n"
