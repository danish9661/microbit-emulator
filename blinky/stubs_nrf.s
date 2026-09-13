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
    str r1, [r0]
    ldr r0, =0x40002500
    movs r1, #8
    str r1, [r0]

    /* SPIM0 (SERIAL0 alias): START, poll STOPPED after STOP */
    ldr r0, =0x40003010
    movs r1, #1
    str r1, [r0]              /* TASKS_START */
    ldr r0, =0x40003014
    str r1, [r0]              /* TASKS_STOP */
    ldr r0, =0x40003104
    bl spin_until_set

    /* SPIM2 (dedicated SPI2 @0x40023000): same START/STOP handshake.
       Proves the SPIM2 slot is live (L5: model routed MISO/DMA but no
       firmware ever touched the instance). */
    ldr r0, =0x40023010
    movs r1, #1
    str r1, [r0]              /* TASKS_START */
    ldr r0, =0x40023014
    str r1, [r0]              /* TASKS_STOP */
    ldr r0, =0x40023104
    bl spin_until_set

    /* SPIM3 (@0x4002F000): same handshake, proves the SPIM3 slot. */
    ldr r0, =0x4002F010
    movs r1, #1
    str r1, [r0]              /* TASKS_START */
    ldr r0, =0x4002F014
    str r1, [r0]              /* TASKS_STOP */
    ldr r0, =0x4002F104
    bl spin_until_set

    /* PDM: ENABLE, START, poll STARTED */
    ldr r0, =0x4001D500
    movs r1, #1
    str r1, [r0]
    ldr r0, =0x4001D000
    str r1, [r0]
    ldr r0, =0x4001D100
    bl spin_until_set

    /* QSPI: ENABLE, ACTIVATE, zero-length READSTART, poll READY event */
    ldr r0, =0x40029500
    movs r1, #1
    str r1, [r0]
    ldr r0, =0x40029000
    str r1, [r0]
    ldr r0, =0x40029004
    str r1, [r0]
    ldr r0, =0x40029104
    bl spin_until_set

    /* USBD: ENABLE, PULLUP, STARTEPIN0, poll STARTED */
    ldr r0, =0x40027500
    movs r1, #1
    str r1, [r0]
    ldr r0, =0x40027504
    str r1, [r0]
    ldr r0, =0x40027000
    str r1, [r0]
    ldr r0, =0x40027104
    bl spin_until_set

    /* RADIO: TXEN, poll READY, START, poll END, STOP */
    ldr r0, =0x40001000
    movs r1, #1
    str r1, [r0]
    ldr r0, =0x40001100
    bl spin_until_set
    ldr r0, =0x40001008
    movs r1, #1
    str r1, [r0]
    ldr r0, =0x4000110C
    bl spin_until_set
    ldr r0, =0x4000100C
    movs r1, #1
    str r1, [r0]

    ldr r0, =msg_ok
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
msg_ok: .asciz "STUBS:OK\n"
