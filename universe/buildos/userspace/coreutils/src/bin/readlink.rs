// ===============================================================================
// READLINK - PRINT RESOLVED SYMBOLIC LINKS OR CANONICAL FILE NAMES
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]

use coreutils_common::*;

entry!(main);

fn main(argc: usize, argv: *const *const u8) -> i32 {
    let mut no_newline = false;
    let mut verbose = false;
    let mut files: [&[u8]; 64] = [&[]; 64];
    let mut file_count = 0;

    let args = unsafe { ArgIter::new(argc, argv) };
    let mut skip_first = true;

    for arg in args {
        if skip_first {
            skip_first = false;
            continue;
        }

        if arg.len() > 1 && arg[0] == b'-' {
            if arg[1] == b'-' {
                if str_eq(arg, b"--no-newline") {
                    no_newline = true;
                } else if str_eq(arg, b"--verbose") {
                    verbose = true;
                } else if str_eq(arg, b"--help") {
                    println("Usage: readlink [OPTION]... FILE...");
                    println("Print value of a symbolic link or canonical file name.");
                    println("");
                    println("  -n, --no-newline  do not output trailing newline");
                    println("  -v, --verbose     report error messages");
                    return 0;
                }
            } else {
                for i in 1..arg.len() {
                    match arg[i] {
                        b'n' => no_newline = true,
                        b'v' => verbose = true,
                        _ => {
                            eprint("readlink: invalid option -- '");
                            write(STDERR, &[arg[i]]);
                            eprintln("'");
                            return 1;
                        }
                    }
                }
            }
        } else if file_count < 64 {
            files[file_count] = arg;
            file_count += 1;
        }
    }

    if file_count == 0 {
        eprintln("readlink: missing operand");
        return 1;
    }

    let mut exit_code = 0;

    for i in 0..file_count {
        let mut path_buf = [0u8; 512];
        let len = files[i].len().min(511);
        path_buf[..len].copy_from_slice(&files[i][..len]);
        path_buf[len] = 0;

        let mut link_buf = [0u8; 4096];
        let result = readlink(&path_buf[..len + 1], &mut link_buf);

        if result < 0 {
            if verbose {
                eprint("readlink: ");
                print_bytes(files[i]);
                eprintln(": No such file or directory");
            }
            exit_code = 1;
            continue;
        }

        print_bytes(&link_buf[..result as usize]);
        if !no_newline {
            println("");
        }
    }

    exit_code
}
