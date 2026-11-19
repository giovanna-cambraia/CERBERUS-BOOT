/* BL2 memory layout (ASM-003). The 256-byte image header sits at 0x4000_0000. */
OUTPUT_ARCH(riscv)
ENTRY(_start)

MEMORY
{
    FLASH (rx)  : ORIGIN = 0x40000100, LENGTH = 0x1FF00
    RAM   (rwx) : ORIGIN = 0x20000000, LENGTH = 0x40000
}

SECTIONS
{
    /* The vendor public key comes first in the payload so BL1 can find it
       (ADR-004). The entry point, _start, follows it. */
    .vendor_key : ALIGN(4)
    {
        KEEP(*(.vendor_key))
    } > FLASH

    .text : ALIGN(4)
    {
        KEEP(*(.text.init))
        *(.text .text.*)
    } > FLASH

    .rodata : ALIGN(4)
    {
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
        . = ALIGN(4);
    } > FLASH

    .data : ALIGN(4)
    {
        _sdata = .;
        *(.data .data.*)
        *(.sdata .sdata.*)
        . = ALIGN(4);
        _edata = .;
    } > RAM AT > FLASH
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

/* The stack grows down from the end of RAM. */
PROVIDE(_stack_top = ORIGIN(RAM) + LENGTH(RAM));
ASSERT(_ebss + 0x2000 <= _stack_top, "RAM: less than 8 KB left for the stack")