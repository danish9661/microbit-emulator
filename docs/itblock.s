.syntax unified
.thumb
@ MicroPython REPL `machine.mem32[0]` trailers walk (ittee block):
@ cmp equal sets Z=1; addne/movne skip; lsreq runs and must PRESERVE
@ flags (T1-in-IT rule) so addeq runs. Assembled halfwords must read:
@ 2b8e bf19 f104 060c 2701 0a3f 3608 (byte-identical to the firmware).
cmp r3, #142
ittee ne
addne r6, r4, #12
movne r7, #1
lsreq r7, r7, #8
addeq r6, #8
