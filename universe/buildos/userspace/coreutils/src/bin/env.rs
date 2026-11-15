// ===============================================================================
// ENV - PRINT ENVIRONMENT OR RUN COMMAND IN MODIFIED ENVIRONMENT
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]

use coreutils_common::*;

entry!(main);

// Environment pointer (passed by kernel)
extern "C" {
    static environ: *const *const u8;
}

fn main(argc: usize, argv: *const *const u8) -> i32 {
    let args = unsafe { ArgIter::new(argc, argv) };
    let mut skip_first = true;

    for arg in args {
        if skip_first {
            skip_first = false;
            continue;
        }

        if str_eq(arg, b"--help") {
            println("Usage: env [OPTION]... [NAME=VALUE]... [COMMAND [ARG]...]");
            println("Print the current environment, or run COMMAND with modified environment.");
            println("");
            println("  -i, --ignore-environment  start with an empty environment");
            println("  -u, --unset=NAME         remove variable from the environment");
            return 0;
        }
    }

    // Print environment
    unsafe {
        if !environ.is_null() {
            let mut i = 0;
            loop {
                let entry = *environ.add(i);
                if entry.is_null() {
                    break;
                }
                let s = cstr_to_slice(entry);
                print_bytes(s);
                println("");
                i += 1;
            }
        }
    }

    0
}
