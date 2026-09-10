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

    /* USBD: poll USBRESET (host pre-signals), clear, print */
    ldr r0, =0x40027100
    bl spin_until_set
    movs r1, #0
    str r1, [r0]
    ldr r0, =msg_usb
    bl print_cstr

    /* RADIO TX then RX (test loops one packet back through air) */
    ldr r0, =0x40001504
    ldr r1, =0x20001200
    str r1, [r0]              /* PACKETPTR */
    ldr r0, =0x40001000
    movs r1, #1
    str r1, [r0]              /* TXEN */
    ldr r0, =0x40001008
    str r1, [r0]              /* START (Tx, stages take_tx) */
    ldr r0, =0x4000100C
    str r1, [r0]              /* STOP */
    ldr r0, =0x40001004
    str r1, [r0]              /* RXEN */
    ldr r0, =0x40001008
    str r1, [r0]              /* START (Rx) */
    ldr r0, =0x4000110C
    bl spin_until_set
    ldr r0, =msg_radio
    bl print_cstr

    /* PPI: TIMER0 COMPARE0 -> GPIOTE OUT0 (P0.21), poll GPIO */
    ldr r0, =0x40006510
    ldr r1, =0x1503           /* task mode, P0.21 */
    str r1, [r0]
    ldr r0, =0x40008540
    ldr r1, =2000
    str r1, [r0]              /* CC0 */
    ldr r0, =0x4001F510
    ldr r1, =0x40008140
    str r1, [r0]              /* CH0.EEP = COMPARE0 */
    ldr r0, =0x4001F514
    ldr r1, =0x40006000
    str r1, [r0]              /* CH0.TEP = OUT0 */
    ldr r0, =0x4001F504
    movs r1, #1
    str r1, [r0]              /* CHENSET */
    ldr r0, =0x40008000
    str r1, [r0]              /* TIMER START */
    bl delay_50k
    ldr r0, =0x50000504
    ldr r0, [r0]              /* P0.OUT */
    ldr r1, =0x200000
    tst r0, r1
    beq ppi_bad
    ldr r0, =msg_ppi
    bl print_cstr
    b done
ppi_bad:
    ldr r0, =msg_ppibad
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

delay_50k:
    push {r2, lr}
    ldr r2, =50000
dloop:
    subs r2, r2, #1
    bne dloop
    pop {r2, pc}

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
msg_usb:   .asciz "USB:OK\n"
msg_radio: .asciz "RADIO:OK\n"
msg_ppi:   .asciz "PPI:OK\n"
msg_ppibad:.asciz "PPI:BAD\n"
