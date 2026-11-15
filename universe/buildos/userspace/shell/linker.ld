/* ============================================================================
 * BUILDOS USER-SPACE LINKER SCRIPT
 * ============================================================================
 * Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
 * ============================================================================ */

ENTRY(_start)
PROGRAM_START = 0x400000;

SECTIONS
{
    . = PROGRAM_START;

    .text ALIGN(4K) : AT(ADDR(.text))
    {
        *(.text.entry)
        *(.text .text.*)
    }

    .rodata ALIGN(4K) : AT(ADDR(.rodata))
    {
        *(.rodata .rodata.*)
    }

    .data ALIGN(4K) : AT(ADDR(.data))
    {
        *(.data .data.*)
    }

    .bss ALIGN(4K) : AT(ADDR(.bss))
    {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }

    . = ALIGN(4K);
    __stack_bottom = .;
    . += 64K;
    __stack_top = .;

    . = ALIGN(4K);
    __heap_start = .;

    /DISCARD/ :
    {
        *(.comment)
        *(.note*)
        *(.eh_frame*)
        *(.gnu.hash)
        *(.dynsym)
        *(.dynstr)
        *(.dynamic)
        *(.plt*)
        *(.interp)
        *(.got*)
    }
}

__program_end = .;
