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

    /* NFCT: ENABLE, SENSE, poll FIELDDETECTED (host pre-presents field) */
    ldr r0, =0x40005500
    movs r1, #1
    str r1, [r0]
    ldr r0, =0x40005008
    str r1, [r0]              /* SENSE */
    ldr r0, =0x40005104
    bl spin_until_set
    movs r1, #0
    str r1, [r0]              /* clear FIELDDETECTED */
    ldr r0, =0x40005000
    movs r1, #1
    str r1, [r0]              /* ACTIVATE */
    ldr r0, =0x4000514C
    bl spin_until_set         /* SELECTED */
    ldr r0, =msg_nfc
    bl print_cstr
    /* TX frame: PACKETPTR=payload, MAXCNT=4, STARTTX, poll TXFRAMEEND */
    ldr r0, =0x40005510
    ldr r1, =msg_payload
    str r1, [r0]
    ldr r0, =0x40005514
    movs r1, #4
    str r1, [r0]
    ldr r0, =0x4000500C
    movs r1, #1
    str r1, [r0]              /* STARTTX */
    ldr r0, =0x40005110
    bl spin_until_set         /* TXFRAMEEND */
    /* RX frame: repoint PACKETPTR at RAM, ENABLERXDATA, poll RXFRAMEEND,
       check first byte 'R' */
    ldr r0, =0x40005510
    ldr r1, =0x20001000
    str r1, [r0]
    ldr r0, =0x4000501C
    movs r1, #1
    str r1, [r0]
    ldr r0, =0x40005118
    bl spin_until_set         /* RXFRAMEEND */
    ldr r0, =0x20001000
    ldrb r0, [r0]
    cmp r0, #0x52            /* 'R' */
    bne rx_bad
    ldr r0, =msg_usbep
    bl print_cstr
done:
    b done
rx_bad:
    ldr r0, =msg_rxb
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
    str r1, [r2, #0]
    b ploop
pdone:
    pop {r4, pc}

    .section .rodata
msg_nfc:     .asciz "NFC:OK\n"
msg_payload: .byte 0xD0,0x07,0x86,0x77
msg_usbep:   .asciz "NFCT:OK\n"
msg_rxb:     .asciz "NFCT:BAD\n"
