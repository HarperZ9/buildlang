// ===============================================================================
// DIRNAME - STRIP LAST COMPONENT FROM FILE NAME
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]

use coreutils_common::*;

entry!(main);

fn main(argc: usize, argv: *const *const u8) -> i32 {
    let mut zero_terminated = false;
    let mut names: [&[u8]; 64] = [&[]; 64];
    let mut name_count = 0;

    let args = unsafe { ArgIter::new(argc, argv) };
    let mut skip_first = true;

    for arg in args {
        if skip_first {
            skip_first = false;
            continue;
        }

        if arg.len() > 1 && arg[0] == b'-' {
            if arg[1] == b'-' {
                if str_eq(arg, b"--zero") {
                    zero_terminated = true;
                } else if str_eq(arg, b"--help") {
                    println("Usage: dirname [OPTION] NAME...");
                    println("Output each NAME with its last non-slash component removed.");
                    println("");
                    println("  -z, --zero  end each output line with NUL");
                    return 0;
                }
            } else {
                for i in 1..arg.len() {
                    match arg[i] {
                        b'z' => zero_terminated = true,
                        _ => {
                            eprint("dirname: invalid option -- '");
                            write(STDERR, &[arg[i]]);
                            eprintln("'");
                            return 1;
                        }
                    }
                }
            }
        } else if name_count < 64 {
            names[name_count] = arg;
            name_count += 1;
        }
    }

    if name_count == 0 {
        eprintln("dirname: missing operand");
        return 1;
    }

    for i in 0..name_count {
        let dir = dirname(names[i]);
        print_bytes(dir);

        if zero_terminated {
            write(STDOUT, &[0]);
        } else {
            println("");
        }
    }

    0
}
