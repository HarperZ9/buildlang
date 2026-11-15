// ===============================================================================
// PWD - PRINT WORKING DIRECTORY
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
            println("Usage: pwd [OPTION]...");
            println("Print the full filename of the current working directory.");
            println("");
            println("  -L, --logical   use PWD from environment");
            println("  -P, --physical  avoid all symlinks");
            return 0;
        }
    }

    let mut buf = [0u8; 4096];
    let len = getcwd(&mut buf);

    if len > 0 {
        print_bytes(&buf[..len as usize]);
        println("");
        0
    } else {
        eprintln("pwd: error getting current directory");
        1
    }
}
