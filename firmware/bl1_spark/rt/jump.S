/* BL1 hand-off helpers, called from the Ada code (bl1-platform.ads). */
        .section .text.bl1_jump_to, "ax"
        .balign 4
        .globl  bl1_jump_to
        .type   bl1_jump_to, @function
bl1_jump_to:
        .word   0x0000100f      /* fence.i: the instruction stream is coherent */
        jr      a0

        .section .text.bl1_halt, "ax"
        .balign 4
        .globl  bl1_halt
        .type   bl1_halt, @function
bl1_halt:
1:      wfi
        j       1b