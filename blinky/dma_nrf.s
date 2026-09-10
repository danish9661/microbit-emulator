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

    /* UARTE TX DMA: PTR=msg_dma, MAXCNT=7, STARTTX, poll ENDTX */
    ldr r0, =0x40002544
    ldr r1, =msg_dma
    str r1, [r0]
    ldr r0, =0x40002548
    movs r1, #7
    str r1, [r0]
    ldr r0, =0x40002008
    movs r1, #1
    str r1, [r0]
    ldr r0, =0x40002120
    bl spin_until_set

    /* TWIM0 RX DMA: PTR=RAM, MAXCNT=3, ADDR=0x19, STARTRX, poll ENDRX */
    ldr r0, =0x40003534
    ldr r1, =0x20001000
    str r1, [r0]
    ldr r0, =0x40003538
    movs r1, #3
    str r1, [r0]
    ldr r0, =0x40003588
    movs r1, #0x19
    str r1, [r0]
    ldr r0, =0x40003000
    movs r1, #1
    str r1, [r0]
    ldr r0, =0x4000310C
    bl spin_until_set
    ldr r0, =msg_i2c
    bl print_cstr

    /* SAADC RESULT DMA: PTR=RAM, MAXCNT=1, START, SAMPLE, poll END */
    ldr r0, =0x4000762C
    ldr r1, =0x20001010
    str r1, [r0]
    ldr r0, =0x40007630
    movs r1, #1
    str r1, [r0]
    ldr r0, =0x40007500
    str r1, [r0]              /* ENABLE */
    ldr r0, =0x40007000
    str r1, [r0]              /* START */
    ldr r0, =0x40007004
    str r1, [r0]              /* SAMPLE */
    ldr r0, =0x40007104
    bl spin_until_set
    ldr r0, =msg_adc
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
msg_dma: .asciz "DMA:OK\n"
msg_i2c: .asciz "I2C:OK\n"
msg_adc: .asciz "ADC:OK\n"
