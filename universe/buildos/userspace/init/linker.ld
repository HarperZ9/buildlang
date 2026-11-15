/* ============================================================================
 * BUILDOS USER-SPACE LINKER SCRIPT
 * ============================================================================
 * Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
 * ============================================================================
 *
 * Linker script for user-space programs
 * Programs are loaded at virtual address 0x400000 (standard Linux-like layout)
 *
 * ============================================================================ */

ENTRY(_start)

/* User-space program starts at 4MB virtual address */
PROGRAM_START = 0x400000;

SECTIONS
{
    . = PROGRAM_START;

    /* Program headers */
    .text ALIGN(4K) : AT(ADDR(.text))
    {
        *(.text.entry)      /* Entry point first */
        *(.text .text.*)    /* All other code */
    }

    /* Read-only data */
    .rodata ALIGN(4K) : AT(ADDR(.rodata))
    {
        *(.rodata .rodata.*)
    }

    /* Initialized data */
    .data ALIGN(4K) : AT(ADDR(.data))
    {
        *(.data .data.*)
    }

    /* Uninitialized data (BSS) */
    .bss ALIGN(4K) : AT(ADDR(.bss))
    {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }

    /* Stack grows downward from high address */
    . = ALIGN(4K);
    __stack_bottom = .;
    . += 64K;               /* 64KB stack */
    __stack_top = .;

    /* Heap starts after stack */
    . = ALIGN(4K);
    __heap_start = .;

    /* Discard unnecessary sections */
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

/* Program end marker */
__program_end = .;
