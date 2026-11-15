// ===============================================================================
// SLEEP - DELAY FOR A SPECIFIED AMOUNT OF TIME
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
    let mut total_seconds: u64 = 0;
    let mut found_number = false;

    for arg in args {
        if skip_first {
            skip_first = false;
            continue;
        }

        if str_eq(arg, b"--help") {
            println("Usage: sleep NUMBER[SUFFIX]...");
            println("Pause for NUMBER seconds.");
            println("");
            println("SUFFIX may be 's' for seconds, 'm' for minutes,");
            println("'h' for hours, or 'd' for days.");
            return 0;
        }

        // Parse number with optional suffix
        let mut num_end = arg.len();
        let mut multiplier: u64 = 1;

        // Check for suffix
        if arg.len() > 0 {
            match arg[arg.len() - 1] {
                b's' => {
                    multiplier = 1;
                    num_end -= 1;
                }
                b'm' => {
                    multiplier = 60;
                    num_end -= 1;
                }
                b'h' => {
                    multiplier = 3600;
                    num_end -= 1;
                }
                b'd' => {
                    multiplier = 86400;
                    num_end -= 1;
                }
                _ => {}
            }
        }

        if let Some(n) = parse_num(&arg[..num_end]) {
            total_seconds += n * multiplier;
            found_number = true;
        } else {
            eprint("sleep: invalid time interval '");
            print_bytes(arg);
            eprintln("'");
            return 1;
        }
    }

    if !found_number {
        eprintln("sleep: missing operand");
        return 1;
    }

    let req = Timespec {
        tv_sec: total_seconds as i64,
        tv_nsec: 0,
    };

    let result = nanosleep(&req, None);
    if result < 0 {
        eprintln("sleep: cannot sleep");
        return 1;
    }

    0
}
