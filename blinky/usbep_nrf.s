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

    /* USBD: ENABLE, PULLUP, poll EP0SETUP (host pre-injects GET_DESCRIPTOR) */
    ldr r0, =0x40027500
    movs r1, #1
    str r1, [r0]
    ldr r0, =0x40027504
    str r1, [r0]
    ldr r0, =0x4002715C
    bl spin_until_set
    movs r1, #0
    str r1, [r0]              /* clear EP0SETUP */
    ldr r0, =0x40027480
    ldr r0, [r0]              /* BMREQUESTTYPE */
    cmp r0, #0x80
    bne setup_bad
    ldr r0, =msg_setup
    bl print_cstr
    b epin_go
setup_bad:
    ldr r0, =msg_setupbad
    bl print_cstr
epin_go:
    /* EPIN0 DMA: PTR=desc, MAXCNT=8, STARTEPIN0, poll ENDEPIN0 */
    ldr r0, =0x40027600
    ldr r1, =msg_desc
    str r1, [r0]
    ldr r0, =0x40027604
    movs r1, #8
    str r1, [r0]
    ldr r0, =0x40027510
    movs r1, #1
    str r1, [r0]              /* EPINEN */
    ldr r0, =0x40027000
    str r1, [r0]              /* STARTEPIN0 */
    ldr r0, =0x40027108
    bl spin_until_set
    ldr r0, =msg_usbep
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
msg_setup:    .asciz "SETUP:OK\n"
msg_setupbad: .asciz "SETUP:BAD\n"
msg_desc:     .byte 0x12,0x01,0x00,0x02,0x00,0x00,0x00,0x40
msg_usbep:    .asciz "USBEP:OK\n"
