// ===============================================================================
// YES - OUTPUT A STRING REPEATEDLY UNTIL KILLED
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]

use coreutils_common::*;

entry!(main);

fn main(argc: usize, argv: *const *const u8) -> i32 {
    let args = unsafe { ArgIter::new(argc, argv) };
    let mut output = b"y\n" as &[u8];
    let mut custom_buf = [0u8; 4096];
    let mut custom_len = 0;

    let mut skip_first = true;
    let mut first_arg = true;

    for arg in args {
        if skip_first {
            skip_first = false;
            continue;
        }

        if str_eq(arg, b"--help") {
            println("Usage: yes [STRING]...");
            println("Repeatedly output a line with STRING, or 'y'.");
            return 0;
        }

        if !first_arg {
            if custom_len < custom_buf.len() - 1 {
                custom_buf[custom_len] = b' ';
                custom_len += 1;
            }
        }
        first_arg = false;

        let copy_len = arg.len().min(custom_buf.len() - custom_len - 1);
        custom_buf[custom_len..custom_len + copy_len].copy_from_slice(&arg[..copy_len]);
        custom_len += copy_len;
    }

    if custom_len > 0 {
        custom_buf[custom_len] = b'\n';
        custom_len += 1;
        output = &custom_buf[..custom_len];
    }

    // Output repeatedly
    loop {
        let result = write(STDOUT, output);
        if result < 0 {
            break;
        }
    }

    0
}
