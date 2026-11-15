// ===============================================================================
// ECHO - DISPLAY A LINE OF TEXT
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]
#![allow(unused_variables)]
#![allow(unused_assignments)]

use coreutils_common::*;

entry!(main);

fn main(argc: usize, argv: *const *const u8) -> i32 {
    let mut no_newline = false;
    let mut interpret_escapes = false;
    let mut first_arg = true;
    let mut first_output = true;

    let args = unsafe { ArgIter::new(argc, argv) };
    let mut skip_first = true;
    let mut options_done = false;

    for arg in args {
        if skip_first {
            skip_first = false;
            continue;
        }

        // Check for options (only before any non-option argument)
        if !options_done && arg.len() > 1 && arg[0] == b'-' {
            let mut valid_option = true;
            let mut temp_no_newline = no_newline;
            let mut temp_interpret = interpret_escapes;

            for i in 1..arg.len() {
                match arg[i] {
                    b'n' => temp_no_newline = true,
                    b'e' => temp_interpret = true,
                    b'E' => temp_interpret = false,
                    _ => {
                        valid_option = false;
                        break;
                    }
                }
            }

            if valid_option {
                no_newline = temp_no_newline;
                interpret_escapes = temp_interpret;
                continue;
            } else {
                options_done = true;
            }
        } else {
            options_done = true;
        }

        // Print separator
        if !first_output {
            print(" ");
        }
        first_output = false;

        // Print argument
        if interpret_escapes {
            let mut i = 0;
            while i < arg.len() {
                if arg[i] == b'\\' && i + 1 < arg.len() {
                    match arg[i + 1] {
                        b'n' => {
                            print("\n");
                            i += 2;
                        }
                        b't' => {
                            print("\t");
                            i += 2;
                        }
                        b'r' => {
                            print("\r");
                            i += 2;
                        }
                        b'\\' => {
                            print("\\");
                            i += 2;
                        }
                        b'a' => {
                            write(STDOUT, &[0x07]);
                            i += 2;
                        }
                        b'b' => {
                            write(STDOUT, &[0x08]);
                            i += 2;
                        }
                        b'f' => {
                            write(STDOUT, &[0x0c]);
                            i += 2;
                        }
                        b'v' => {
                            write(STDOUT, &[0x0b]);
                            i += 2;
                        }
                        b'0' => {
                            // Octal escape
                            let mut val: u8 = 0;
                            let mut j = i + 2;
                            while j < arg.len() && j < i + 5 && arg[j] >= b'0' && arg[j] <= b'7' {
                                val = val * 8 + (arg[j] - b'0');
                                j += 1;
                            }
                            write(STDOUT, &[val]);
                            i = j;
                        }
                        b'x' => {
                            // Hex escape
                            if i + 3 < arg.len() {
                                let h1 = hex_digit(arg[i + 2]);
                                let h2 = hex_digit(arg[i + 3]);
                                if let (Some(d1), Some(d2)) = (h1, h2) {
                                    write(STDOUT, &[d1 * 16 + d2]);
                                    i += 4;
                                    continue;
                                }
                            }
                            write(STDOUT, &[arg[i]]);
                            i += 1;
                        }
                        b'c' => {
                            // Suppress further output
                            return 0;
                        }
                        _ => {
                            write(STDOUT, &[arg[i]]);
                            i += 1;
                        }
                    }
                } else {
                    write(STDOUT, &[arg[i]]);
                    i += 1;
                }
            }
        } else {
            print_bytes(arg);
        }

        first_arg = false;
    }

    if !no_newline {
        println("");
    }

    0
}

fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}
