/* BL1 boot ROM layout (ASM-003): 32 KB ROM at 0x0001_0000. */
OUTPUT_ARCH(riscv)
ENTRY(_start)

MEMORY
{
    ROM (rx)  : ORIGIN = 0x00010000, LENGTH = 0x8000
    RAM (rwx) : ORIGIN = 0x20000000, LENGTH = 0x40000
}

SECTIONS
{
    .text : ALIGN(4)
    {
        KEEP(*(.text.init))
        *(.text .text.*)
    } > ROM

    .rodata : ALIGN(4)
    {
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
        . = ALIGN(4);
    } > ROM

    .data : ALIGN(4)
    {
        _sdata = .;
        *(.data .data.*)
        *(.sdata .sdata.*)
        . = ALIGN(4);
        _edata = .;
    } > RAM AT > ROM
    _sidata = LOADADDR(.data);

    .bss (NOLOAD) : ALIGN(4)
    {
        _sbss = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        *(COMMON)
        . = ALIGN(4);
        _ebss = .;
    } > RAM

    /DISCARD/ :
    {
        *(.eh_frame)
        *(.eh_frame_hdr)
        *(.comment)
    }
}

/* The BL1 stack grows down from 0x2003_A000. 0x2003_B000 holds the boot report. */
PROVIDE(_stack_top = 0x2003A000);
ASSERT(_stack_top - _ebss >= 0x1000, "RAM: less than 4 KB left for the BL1 stack")