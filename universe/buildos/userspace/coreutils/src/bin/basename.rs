// ===============================================================================
// BASENAME - STRIP DIRECTORY AND SUFFIX FROM FILENAMES
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]

use coreutils_common::*;

entry!(main);

fn main(argc: usize, argv: *const *const u8) -> i32 {
    let mut suffix: Option<&[u8]> = None;
    let mut multiple = false;
    let mut zero_terminated = false;
    let mut names: [&[u8]; 64] = [&[]; 64];
    let mut name_count = 0;

    let args = unsafe { ArgIter::new(argc, argv) };
    let mut skip_first = true;
    let mut expect_suffix = false;

    for arg in args {
        if skip_first {
            skip_first = false;
            continue;
        }

        if expect_suffix {
            suffix = Some(arg);
            expect_suffix = false;
            continue;
        }

        if arg.len() > 1 && arg[0] == b'-' {
            if arg[1] == b'-' {
                if str_eq(arg, b"--multiple") {
                    multiple = true;
                } else if str_eq(arg, b"--zero") {
                    zero_terminated = true;
                } else if str_starts_with(arg, b"--suffix=") {
                    suffix = Some(&arg[9..]);
                } else if str_eq(arg, b"--help") {
                    println("Usage: basename NAME [SUFFIX]");
                    println("   or: basename OPTION... NAME...");
                    println("Print NAME with any leading directory components removed.");
                    println("");
                    println("  -a, --multiple       support multiple arguments");
                    println("  -s, --suffix=SUFFIX  remove trailing SUFFIX");
                    println("  -z, --zero           end each output line with NUL");
                    return 0;
                }
            } else {
                for i in 1..arg.len() {
                    match arg[i] {
                        b'a' => multiple = true,
                        b's' => expect_suffix = true,
                        b'z' => zero_terminated = true,
                        _ => {
                            eprint("basename: invalid option -- '");
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
        eprintln("basename: missing operand");
        return 1;
    }

    // If not in multiple mode and we have 2 args, second is suffix
    if !multiple && name_count == 2 && suffix.is_none() {
        suffix = Some(names[1]);
        name_count = 1;
    }

    for i in 0..name_count {
        let name = names[i];
        let base = basename(name);

        // Remove suffix if specified
        let output = if let Some(suf) = suffix {
            if base.len() > suf.len() {
                let potential_suffix = &base[base.len() - suf.len()..];
                if str_eq(potential_suffix, suf) {
                    &base[..base.len() - suf.len()]
                } else {
                    base
                }
            } else {
                base
            }
        } else {
            base
        };

        print_bytes(output);

        if zero_terminated {
            write(STDOUT, &[0]);
        } else {
            println("");
        }
    }

    0
}
