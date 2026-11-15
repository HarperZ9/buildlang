// ===============================================================================
// TEE - READ FROM STDIN AND WRITE TO STDOUT AND FILES
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
    let mut append = false;
    let mut ignore_interrupts = false;
    let mut files: [&[u8]; 16] = [&[]; 16];
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
                if str_eq(arg, b"--append") {
                    append = true;
                } else if str_eq(arg, b"--ignore-interrupts") {
                    ignore_interrupts = true;
                } else if str_eq(arg, b"--help") {
                    println("Usage: tee [OPTION]... [FILE]...");
                    println("Copy standard input to each FILE, and also to standard output.");
                    println("");
                    println("  -a, --append             append to given FILEs, do not overwrite");
                    println("  -i, --ignore-interrupts  ignore interrupt signals");
                    return 0;
                }
            } else {
                for i in 1..arg.len() {
                    match arg[i] {
                        b'a' => append = true,
                        b'i' => ignore_interrupts = true,
                        _ => {
                            eprint("tee: invalid option -- '");
                            write(STDERR, &[arg[i]]);
                            eprintln("'");
                            return 1;
                        }
                    }
                }
            }
        } else if file_count < 16 {
            files[file_count] = arg;
            file_count += 1;
        }
    }

    // Open output files
    let mut fds: [i32; 16] = [-1; 16];
    let mut fd_count = 0;

    for i in 0..file_count {
        let mut path_buf = [0u8; 512];
        let len = files[i].len().min(511);
        path_buf[..len].copy_from_slice(&files[i][..len]);
        path_buf[len] = 0;

        let flags = if append {
            O_WRONLY | O_CREAT | O_APPEND
        } else {
            O_WRONLY | O_CREAT | O_TRUNC
        };

        let fd = open(&path_buf[..len + 1], flags, 0o644);
        if fd < 0 {
            eprint("tee: ");
            print_bytes(files[i]);
            eprintln(": cannot open for writing");
        } else {
            fds[fd_count] = fd;
            fd_count += 1;
        }
    }

    // Read from stdin and write to stdout and files
    let mut buf = [0u8; 8192];
    let mut exit_code = 0;

    loop {
        let n = read(STDIN, &mut buf);
        if n <= 0 {
            break;
        }

        // Write to stdout
        let written = write(STDOUT, &buf[..n as usize]);
        if written != n {
            exit_code = 1;
        }

        // Write to all files
        for i in 0..fd_count {
            let written = write(fds[i], &buf[..n as usize]);
            if written != n {
                exit_code = 1;
            }
        }
    }

    // Close all files
    for i in 0..fd_count {
        close(fds[i]);
    }

    exit_code
}
