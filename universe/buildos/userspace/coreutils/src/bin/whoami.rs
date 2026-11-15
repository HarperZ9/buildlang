// ===============================================================================
// WHOAMI - PRINT EFFECTIVE USERID
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
            println("Usage: whoami [OPTION]...");
            println("Print the user name associated with the current effective user ID.");
            return 0;
        }
    }

    let uid = geteuid();

    // In a full implementation, we'd look up /etc/passwd
    // For now, just print the uid or "root" for uid 0
    if uid == 0 {
        println("root");
    } else {
        print("user");
        print_num(uid as u64);
        println("");
    }

    0
}
