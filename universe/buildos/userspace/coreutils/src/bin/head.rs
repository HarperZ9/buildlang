// ===============================================================================
// HEAD - OUTPUT THE FIRST PART OF FILES
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]

use coreutils_common::*;

entry!(main);

fn head_fd(fd: i32, lines: u64, bytes: Option<u64>, quiet: bool, filename: Option<&[u8]>) -> i32 {
    if !quiet {
        if let Some(name) = filename {
            print("==> ");
            print_bytes(name);
            println(" <==");
        }
    }

    let mut buf = [0u8; 8192];

    if let Some(byte_count) = bytes {
        // Output first N bytes
        let mut remaining = byte_count;
        while remaining > 0 {
            let to_read = (remaining as usize).min(buf.len());
            let n = read(fd, &mut buf[..to_read]);
            if n <= 0 {
                break;
            }
            write(STDOUT, &buf[..n as usize]);
            remaining -= n as u64;
        }
    } else {
        // Output first N lines
        let mut line_count: u64 = 0;

        'outer: loop {
            let n = read(fd, &mut buf);
            if n <= 0 {
                break;
            }

            for i in 0..n as usize {
                write(STDOUT, &[buf[i]]);
                if buf[i] == b'\n' {
                    line_count += 1;
                    if line_count >= lines {
                        break 'outer;
                    }
                }
            }
        }
    }

    0
}

fn main(argc: usize, argv: *const *const u8) -> i32 {
    let mut lines: u64 = 10;
    let mut bytes: Option<u64> = None;
    let mut quiet = false;
    let mut verbose = false;
    let mut files: [&[u8]; 64] = [&[]; 64];
    let mut file_count = 0;

    let args = unsafe { ArgIter::new(argc, argv) };
    let mut skip_first = true;
    let mut expect_lines = false;
    let mut expect_bytes = false;

    for arg in args {
        if skip_first {
            skip_first = false;
            continue;
        }

        if expect_lines {
            if let Some(n) = parse_num(arg) {
                lines = n;
            }
            expect_lines = false;
            continue;
        }

        if expect_bytes {
            if let Some(n) = parse_num(arg) {
                bytes = Some(n);
            }
            expect_bytes = false;
            continue;
        }

        if arg.len() > 1 && arg[0] == b'-' {
            if arg[1] == b'-' {
                if str_eq(arg, b"--quiet") || str_eq(arg, b"--silent") {
                    quiet = true;
                } else if str_eq(arg, b"--verbose") {
                    verbose = true;
                } else if str_starts_with(arg, b"--lines=") {
                    if let Some(n) = parse_num(&arg[8..]) {
                        lines = n;
                    }
                } else if str_starts_with(arg, b"--bytes=") {
                    if let Some(n) = parse_num(&arg[8..]) {
                        bytes = Some(n);
                    }
                } else if str_eq(arg, b"--help") {
                    println("Usage: head [OPTION]... [FILE]...");
                    println("Print the first 10 lines of each FILE.");
                    println("");
                    println("  -c, --bytes=NUM   print first NUM bytes");
                    println("  -n, --lines=NUM   print first NUM lines");
                    println("  -q, --quiet       never print headers");
                    println("  -v, --verbose     always print headers");
                    return 0;
                }
            } else if arg.len() > 1 && arg[1] >= b'0' && arg[1] <= b'9' {
                // -N syntax for lines
                if let Some(n) = parse_num(&arg[1..]) {
                    lines = n;
                }
            } else {
                for i in 1..arg.len() {
                    match arg[i] {
                        b'n' => expect_lines = true,
                        b'c' => expect_bytes = true,
                        b'q' => quiet = true,
                        b'v' => verbose = true,
                        _ => {
                            eprint("head: invalid option -- '");
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
        return head_fd(STDIN, lines, bytes, true, None);
    }

    let print_headers = (file_count > 1 && !quiet) || verbose;
    let mut exit_code = 0;

    for i in 0..file_count {
        if i > 0 && print_headers {
            println("");
        }

        let mut path_buf = [0u8; 512];
        let len = files[i].len().min(511);
        path_buf[..len].copy_from_slice(&files[i][..len]);
        path_buf[len] = 0;

        if files[i].len() == 1 && files[i][0] == b'-' {
            head_fd(STDIN, lines, bytes, !print_headers, if print_headers { Some(b"standard input") } else { None });
        } else {
            let fd = open(&path_buf[..len + 1], O_RDONLY, 0);
            if fd < 0 {
                eprint("head: cannot open '");
                print_bytes(files[i]);
                eprintln("' for reading");
                exit_code = 1;
                continue;
            }

            head_fd(fd, lines, bytes, !print_headers, if print_headers { Some(files[i]) } else { None });
            close(fd);
        }
    }

    exit_code
}
