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

    /* USBD attach + ISO enables + device address + EPSTALL roundtrip */
    ldr r0, =0x40027500
    movs r1, #1
    str r1, [r0]              /* ENABLE */
    ldr r0, =0x40027504
    movs r1, #1
    str r1, [r0]              /* PULLUP */
    ldr r0, =0x40027510
    ldr r1, =0x100
    str r1, [r0]              /* EPINEN ISO (bit 8) */
    ldr r0, =0x40027514
    ldr r1, =0x100
    str r1, [r0]              /* EPOUTEN ISO (bit 8) */
    ldr r0, =0x40027470
    movs r1, #0x2A
    str r1, [r0]              /* USBADDR */
    ldr r1, [r0]
    cmp r1, #0x2A
    bne bad
    /* EPSTALL EPIN1 then unstall: HALTED latches + clears */
    ldr r0, =0x40027518
    ldr r1, =0x101
    str r1, [r0]              /* EP=1 IN STALL */
    ldr r0, =0x40027424
    ldr r1, [r0]
    cmp r1, #1
    bne bad                   /* HALTED.EPIN1 */
    ldr r0, =0x40027518
    movs r1, #1
    str r1, [r0]              /* EP=1 IN unstall */
    ldr r0, =0x40027424
    ldr r1, [r0]
    cmp r1, #0
    bne bad

    /* ISOIN: PTR/MAXCNT + STARTISOIN, poll ENDISOIN (driver streams) */
    ldr r0, =0x400276A0
    ldr r1, =0x20001000
    str r1, [r0]              /* ISOIN.PTR */
    ldr r0, =0x400276A4
    movs r1, #4
    str r1, [r0]              /* ISOIN.MAXCNT */
    ldr r0, =0x40027024
    movs r1, #1
    str r1, [r0]              /* STARTISOIN */
    ldr r0, =0x4002712C
    bl spin_until_set         /* ENDISOIN */

    /* ISOOUT: PTR/MAXCNT + STARTISOOUT, poll ENDISOOUT + SIZE */
    ldr r0, =0x400277A0
    ldr r1, =0x20002000
    str r1, [r0]              /* ISOOUT.PTR */
    ldr r0, =0x400277A4
    movs r1, #4
    str r1, [r0]              /* ISOOUT.MAXCNT */
    ldr r0, =0x40027048
    movs r1, #1
    str r1, [r0]              /* STARTISOOUT */
    ldr r0, =0x40027150
    bl spin_until_set         /* ENDISOOUT */
    ldr r0, =0x400274C0
    ldr r1, [r0]
    cmp r1, #4
    bne bad                   /* SIZE.ISOOUT */

    /* EPOUT1: roundtrip raises ENDEPOUT1 + EPDATA (driver sinks) */
    ldr r0, =0x40027714
    ldr r1, =0x20003000
    str r1, [r0]              /* EPOUT1.PTR (stride 0x14) */
    ldr r0, =0x40027718
    movs r1, #8
    str r1, [r0]              /* EPOUT1.MAXCNT */
    ldr r0, =0x40027514
    movs r1, #2
    str r1, [r0]              /* EPOUTEN bit1 */
    ldr r0, =0x4002702C
    movs r1, #1
    str r1, [r0]              /* STARTEPOUT1 */
    ldr r0, =0x40027134
    bl spin_until_set         /* ENDEPOUT1 */
    ldr r0, =0x40027160
    bl spin_until_set         /* EPDATA */

    /* SOF + USBEVENT arrive from the host between slices */
    ldr r0, =0x40027154
    bl spin_until_set         /* SOF */
    ldr r0, =0x40027520
    ldr r1, [r0]
    cmp r1, #0
    beq bad                   /* FRAMECNTR advanced */
    ldr r0, =0x40027158
    bl spin_until_set         /* USBEVENT */
    ldr r0, =0x40027400
    ldr r1, [r0]
    movs r2, #1
    lsls r2, r2, #8           /* SUSPEND bit 8 */
    tst r1, r2
    beq bad                   /* EVENTCAUSE SUSPEND */

    /* EP0STATUS + DATADONE_STARTEPIN0 short: EPINEN0, EP0STATUS task,
       DATADONE poll, ENDEPIN0 poll (driver completes the chained IN). */
    ldr r0, =0x40027510
    movs r1, #1
    str r1, [r0]              /* EPINEN bit0 */
    ldr r0, =0x40027600
    ldr r1, =0x20001000
    str r1, [r0]              /* EPIN0.PTR */
    ldr r0, =0x40027604
    movs r1, #2
    str r1, [r0]              /* EPIN0.MAXCNT */
    ldr r0, =0x40027200
    movs r1, #1
    str r1, [r0]              /* SHORTS EP0DATADONE_STARTEPIN0 */
    ldr r0, =0x40027050
    movs r1, #1
    str r1, [r0]              /* EP0STATUS */
    ldr r0, =0x40027128
    bl spin_until_set         /* EP0DATADONE */
    ldr r0, =0x40027108
    bl spin_until_set         /* ENDEPIN0 via short */

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
msg_ok: .asciz "USBI:OK\n"
msg_bad: .asciz "USBI:BAD\n"
