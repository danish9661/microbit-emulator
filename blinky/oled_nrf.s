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
    /* UARTE ENABLE=8 (for the OLED:OK marker) */
    ldr r0, =0x40002500
    movs r1, #8
    str r1, [r0]

    /* TWIM0 ENABLE=6 (TWIM): *(0x40003500) = 6 */
    ldr r0, =0x40003500
    movs r1, #6
    str r1, [r0]
    /* ADDRESS 0x3C (SSD1306 OLED): *(0x40003588) = 0x3C */
    ldr r0, =0x40003588
    movs r1, #0x3C
    str r1, [r0]
    /* SHORTS LASTTX_STARTRX (bit 7): repeated START, no STOP between
       the init write and the status read */
    ldr r0, =0x40003200
    movs r1, #0x80
    str r1, [r0]
    /* TXD PTR/CNT: 9-byte init (control 0x00 + 8 commands) */
    ldr r0, =0x40003544
    ldr r1, =oled_init
    str r1, [r0]
    ldr r0, =0x40003548
    movs r1, #9
    str r1, [r0]
    /* RXD PTR/CNT: 1 status byte into RAM */
    ldr r0, =0x40003534
    ldr r1, =0x20001000
    str r1, [r0]
    ldr r0, =0x40003538
    movs r1, #1
    str r1, [r0]
    /* STARTTX; SHORTS chains STARTRX; poll ENDRX */
    ldr r0, =0x40003008
    movs r1, #1
    str r1, [r0]
    ldr r0, =0x4000310C
    bl spin_until_set
    /* status read complete: host served the byte */
    ldr r0, =msg_oled
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
    /* 0x00 = command stream; AE off, D5/80 clock, A8/3F mux64,
       8D/14 charge pump, AF on */
oled_init: .byte 0x00, 0xAE, 0xD5, 0x80, 0xA8, 0x3F, 0x8D, 0x14, 0xAF
msg_oled: .asciz "OLED:OK\n"
