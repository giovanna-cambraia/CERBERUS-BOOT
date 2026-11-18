/* BL1 reset entry (RV32IMAC). */
        .section .text.init, "ax"
        .balign 4
        .globl  _start
_start:
        la      sp, _stack_top

        /* zero .bss */
        la      t0, _sbss
        la      t1, _ebss
1:      bgeu    t0, t1, 2f
        sw      zero, 0(t0)
        addi    t0, t0, 4
        j       1b

2:      /* copy .data from ROM to RAM */
        la      t0, _sdata
        la      t1, _edata
        la      t2, _sidata
3:      bgeu    t0, t1, 4f
        lw      t3, 0(t2)
        sw      t3, 0(t0)
        addi    t0, t0, 4
        addi    t2, t2, 4
        j       3b

4:      call    bl1_main
5:      wfi
        j       5b