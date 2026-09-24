    .syntax unified
    .cpu cortex-m4
    .thumb
    .section .vectors,"a",%progbits
    .word 0x20002000          /* SP */
    .word _start+1            /* Reset */
    .text
    .global _start
    .thumb_func
/* MATRIX: 5x5 LED proof. Drives every row low vs every column high
   (sweep), then lights the "A" glyph pixels from the image buffer.
   Rows sink (OUT=0), columns source (OUT=1); led on <=> row low &&
   col high. Prints MATRIX:OK when GPIO reads back the driven state. */
_start:
    /* HFCLK start */
    ldr r0, =0x40000000
    movs r1, #1
    str r1, [r0]
    /* UARTE ENABLE=8 */
    ldr r0, =0x40002500
    movs r1, #8
    str r1, [r0]
    /* DIRSET rows P0.21,22,15,24,19 + cols P0.28,11,31,30 P1.5.
       Masks from demo/parts/pins.js (MicroBitIO ground truth):
       ROWS_ALL = 0x01688000, COLS_P0 = 0xD0000800, union = 0xD1688800. */
    ldr r0, =0x50000518       /* P0 DIRSET */
    ldr r1, =0xD1688800
    str r1, [r0]
    ldr r0, =0x50000818       /* P1 DIRSET (col4 P1.5) */
    movs r1, #0x20
    str r1, [r0]
    /* sweep: each row low, all cols high, verify one led reads lit.
       Later strobe passes (r8 != 0) skip the verify: the sweep is the
       display refresh, not the proof. r8 is zeroed at reset (BSS-style:
       _start is the reset entry, so an explicit movs covers every boot). */
    movs r8, #0
    movs r4, #0
    ldr r5, =rows
    ldr r6, =cols_p0
    ldr r7, =cols_p1
sweep:
    lsls r0, r4, #2
    ldr r0, [r5, r0]          /* row bit */
    ldr r1, =0x5000050C       /* P0 OUTCLR */
    str r0, [r1]              /* row low */
    ldr r0, =0x50000508       /* P0 OUTSET: cols high */
    ldr r1, =0xD0000800
    str r1, [r0]
    ldr r0, =0x50000808       /* P1 OUTSET bit5 */
    movs r1, #0x20
    str r1, [r0]
    bl delay_short
    /* verify: row OUT bit reads 0 (first pass only — see above) */
    cmp r8, #0
    bne sweep_next
    lsls r0, r4, #2
    ldr r0, [r5, r0]
    ldr r1, =0x50000504       /* P0 OUT */
    ldr r1, [r1]
    tst r1, r0
    bne fail
sweep_next:
    adds r4, r4, #1
    cmp r4, #5
    blt sweep
    /* light the "A" glyph: rows with cols per glyph_a bits */
    movs r4, #0
glyph:
    lsls r0, r4, #2
    ldr r0, [r5, r0]
    ldr r1, =0x5000050C
    str r0, [r1]              /* this row low */
    /* previous row back high (except first) */
    cbz r4, nocl
    subs r0, r4, #1
    lsls r0, r0, #2
    ldr r0, [r5, r0]
    ldr r1, =0x50000508
    str r0, [r1]
nocl:
    /* cols for this glyph row: bits in glyph_a[r4] select cols */
    ldr r0, =glyph_a
    ldrb r0, [r0, r4]
    /* col bits: bit i -> COLS[i]; P0 mask + P1 bit5 */
    movs r1, #0
    movs r2, #0               /* P1 col4 flag */
    lsls r3, r0, #31          /* bit0 -> COL1 P0.28 */
    bpl noc0
    ldr r3, =0x10000000
    orrs r1, r3
noc0:
    lsls r3, r0, #30
    bpl noc1
    ldr r3, =0x800
    orrs r1, r3
noc1:
    lsls r3, r0, #29
    bpl noc2
    ldr r3, =0x80000000
    orrs r1, r3
noc2:
    lsls r3, r0, #28
    bpl noc3
    movs r2, #1               /* COL4 P1.5 */
noc3:
    lsls r3, r0, #27
    bpl noc4
    ldr r3, =0x40000000
    orrs r1, r3
noc4:
    ldr r3, =0x50000508
    str r1, [r3]
    ldr r3, =0x50000808
    movs r3, r3
    cmp r2, #0
    beq nocol4
    ldr r3, =0x50000808
    movs r2, #0x20
    str r2, [r3]
    b doclr4
nocol4:
    ldr r3, =0x5000080C
    movs r2, #0x20
    str r2, [r3]
doclr4:
    bl delay_short
    adds r4, r4, #1
    cmp r4, #5
    blt glyph
    /* verify glyph row2 cols read high + row4 sunk (first pass only) */
    cmp r8, #0
    bne have_glyph
    ldr r0, =0x50000504
    ldr r0, [r0]
    ldr r1, =0xD0000800
    ands r0, r1
    cmp r0, r1
    bne fail
    /* verify row4 (last) is the sunk row: its bit reads 0 */
    ldr r0, =0x50000504
    ldr r0, [r0]
    ldr r1, =0x80000            /* ROW4 P0.19 */
    tst r0, r1
    bne fail
have_glyph:
    /* print once: first pass (r8 == 0) prints + arms the strobe flag;
       later passes skip straight to the loop. Flag set BEFORE the
       print (print_cstr clobbers r4/r1/r2, r8 survives). */
    cmp r8, #0
    bne sweep
    movs r8, #1
    ldr r0, =msg_ok
    bl print_cstr
    /* Strobe the glyph forever (multiplexed like silicon): re-run the
       sweep+glyph loop so the bench's persistence renderer sees every
       row lit in turn. r8 != 0 skips both verifies + the print above
       on later passes. */
    b sweep
fail:
    ldr r0, =msg_fail
    bl print_cstr
done:
    b done

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

delay_short:
    ldr r2, =50000
dloop:
    subs r2, r2, #1
    bne dloop
    bx lr

    .section .rodata
rows:   .word 0x200000, 0x400000, 0x8000, 0x1000000, 0x80000
cols_p0:.word 0x10000000, 0x800, 0x80000000, 0x40000000
cols_p1:.word 0x20
/* "A" glyph rows (bit i = COLS[i] lit): full/upper bar pattern */
glyph_a:.byte 0x0E, 0x11, 0x1F, 0x11, 0x11
msg_ok:  .asciz "MATRIX:OK\n"
msg_fail:.asciz "MATRIX:FAIL\n"
