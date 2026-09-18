    .syntax unified
    .cpu cortex-m4
    .thumb
    .section .vectors,"a",%progbits
    .word 0x20002000
    .word _start+1
    .space 36                    /* vectors 2..10 */
    .word svc_handler+1          /* vector 11: SVC handler (bx lr) */
    .text
    .global _start
    .thumb_func
svc_handler:
    bx lr                        /* EXC_RETURN: exception return */
    .thumb_func
_start:
    ldr r0, =0x40000000
    movs r1, #1
    str r1, [r0]              /* HFCLK */
    ldr r0, =0x40002500
    movs r1, #8
    str r1, [r0]              /* UARTE0 enable (console) */

    /* SD enable observer: svc 16 arms the SoC event transport
       (sd_evt phase 1). Real firmware enables the SoftDevice first;
       without it no flash event may queue (silicon rule). The SVC
       handler above returns straight through. */
    svc #16

    /* NVMC page erase via model: CONFIG=EEN, ERASEPAGE=page.
       The test driver takes + completes (driver applies 0xFF),
       which posts FLASH_OPERATION_SUCCESS (id 2) into the
       model-side SoC queue — same take/complete discipline as
       every DMA pump. Firmware spins on a RAM mailbox the driver
       sets after completing (NVMC has no event register). */
    ldr r0, =0x4001E504
    movs r1, #2
    str r1, [r0]              /* CONFIG=EEN */
    ldr r0, =0x4001E508
    ldr r1, =0x00074000
    str r1, [r0]              /* ERASEPAGE (driver completes) */
    ldr r0, =mailbox_ptr
    ldr r0, [r0]              /* r0 = mailbox address (NOT the pointer!) */
    bl spin_until_set         /* driver sets *mailbox after complete_erase */

    /* Poll sd_evt_get (svc 82) with a fixed word buffer (NOT sp:
       the SVC return value clobbers r0, so an sp-relative reload
       would read [0]. Buffer at 0x20001010, clear of the 0x20001000
       mailbox). The queue holds exactly one event, id=2 len=0. */
    ldr r0, =evt_buf
    ldr r0, [r0]              /* r0 = event buffer address */
    svc #82
    ldr r0, =evt_buf
    ldr r0, [r0]
    ldrh r2, [r0]             /* id (r2/r3 survive print_cstr) */
    ldrh r3, [r0, #2]         /* len */
    cmp r2, #2
    bne fail
    cmp r3, #0
    bne fail
    ldr r0, =msg_ev
    bl print_cstr

    /* Empty-queue poll: falls through to the SD (SVC handler
       returns cleanly) — must NOT fault. */
    ldr r0, =evt_buf
    ldr r0, [r0]
    svc #82
    ldr r0, =msg_empty
    bl print_cstr
    ldr r0, =msg_ok
    bl print_cstr
done:
    b done
fail:
    ldr r0, =msg_fail
    bl print_cstr
    b done

    /* Spin until [r0] != 0 (bounded, like the other proofs). */
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
mailbox_ptr: .word 0x20001000
evt_buf: .word 0x20001010
msg_ev:    .asciz "EV:OK\n"
msg_empty: .asciz "EMPTY:OK\n"
msg_ok:    .asciz "SDEVT:OK\n"
msg_fail:  .asciz "SDEVT:FAIL\n"
