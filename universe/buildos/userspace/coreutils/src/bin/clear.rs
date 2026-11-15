// ===============================================================================
// CLEAR - CLEAR THE TERMINAL SCREEN
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]

use coreutils_common::*;

entry!(main);

fn main(argc: usize, argv: *const *const u8) -> i32 {
    let args = unsafe { ArgIter::new(argc, argv) };
    let mut skip_first = true;

    for arg in args {
        if skip_first {
            skip_first = false;
            continue;
        }

        if str_eq(arg, b"--help") {
            println("Usage: clear");
            println("Clear the terminal screen.");
            return 0;
        }
    }

    // ANSI escape sequence to clear screen and move cursor to home
    print("\x1b[2J\x1b[H");

    0
}
