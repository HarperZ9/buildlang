// ===============================================================================
// TAIL - OUTPUT THE LAST PART OF FILES
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]

use coreutils_common::*;

entry!(main);

fn tail_fd(fd: i32, lines: u64, bytes: Option<u64>, quiet: bool, filename: Option<&[u8]>) -> i32 {
    if !quiet {
        if let Some(name) = filename {
            print("==> ");
            print_bytes(name);
            println(" <==");
        }
    }

    if let Some(byte_count) = bytes {
        // Output last N bytes - seek to end minus N
        let file_size = lseek(fd, 0, SEEK_END);
        if file_size >= 0 {
            let start = if (file_size as u64) > byte_count {
                file_size - byte_count as i64
            } else {
                0
            };
            lseek(fd, start, SEEK_SET);

            let mut buf = [0u8; 8192];
            loop {
                let n = read(fd, &mut buf);
                if n <= 0 {
                    break;
                }
                write(STDOUT, &buf[..n as usize]);
            }
        }
    } else {
        // Output last N lines
        // We need to buffer the file to find the last N lines
        // For simplicity, we'll use a circular buffer approach

        const MAX_LINE_LEN: usize = 4096;
        const MAX_LINES: usize = 256;

        let mut line_buf: [[u8; MAX_LINE_LEN]; MAX_LINES] = [[0u8; MAX_LINE_LEN]; MAX_LINES];
        let mut line_lens: [usize; MAX_LINES] = [0; MAX_LINES];
        let mut line_idx: usize = 0;
        let mut total_lines: u64 = 0;

        let mut buf = [0u8; 8192];
        let mut current_line = [0u8; MAX_LINE_LEN];
        let mut current_len: usize = 0;

        loop {
            let n = read(fd, &mut buf);
            if n <= 0 {
                break;
            }

            for i in 0..n as usize {
                if buf[i] == b'\n' {
                    // End of line - store it
                    if current_len < MAX_LINE_LEN {
                        current_line[current_len] = b'\n';
                        current_len += 1;
                    }

                    // Store in circular buffer
                    let idx = (line_idx % MAX_LINES) as usize;
                    let copy_len = current_len.min(MAX_LINE_LEN);
                    line_buf[idx][..copy_len].copy_from_slice(&current_line[..copy_len]);
                    line_lens[idx] = copy_len;
                    line_idx += 1;
                    total_lines += 1;
                    current_len = 0;
                } else if current_len < MAX_LINE_LEN {
                    current_line[current_len] = buf[i];
                    current_len += 1;
                }
            }
        }

        // Handle last line without newline
        if current_len > 0 {
            let idx = (line_idx % MAX_LINES) as usize;
            line_buf[idx][..current_len].copy_from_slice(&current_line[..current_len]);
            line_lens[idx] = current_len;
            line_idx += 1;
            total_lines += 1;
        }

        // Output last N lines
        let lines_to_print = lines.min(total_lines) as usize;
        let start_idx = if total_lines > lines {
            (line_idx - lines_to_print) % MAX_LINES
        } else {
            0
        };

        for i in 0..lines_to_print {
            let idx = (start_idx + i) % MAX_LINES;
            write(STDOUT, &line_buf[idx][..line_lens[idx]]);
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
                    println("Usage: tail [OPTION]... [FILE]...");
                    println("Print the last 10 lines of each FILE.");
                    println("");
                    println("  -c, --bytes=NUM   output last NUM bytes");
                    println("  -n, --lines=NUM   output last NUM lines");
                    println("  -q, --quiet       never print headers");
                    println("  -v, --verbose     always print headers");
                    return 0;
                }
            } else if arg.len() > 1 && arg[1] >= b'0' && arg[1] <= b'9' {
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
                        b'f' => {
                            // Follow mode - not implemented for simplicity
                        }
                        _ => {
                            eprint("tail: invalid option -- '");
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
        return tail_fd(STDIN, lines, bytes, true, None);
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
            tail_fd(STDIN, lines, bytes, !print_headers, if print_headers { Some(b"standard input") } else { None });
        } else {
            let fd = open(&path_buf[..len + 1], O_RDONLY, 0);
            if fd < 0 {
                eprint("tail: cannot open '");
                print_bytes(files[i]);
                eprintln("' for reading");
                exit_code = 1;
                continue;
            }

            tail_fd(fd, lines, bytes, !print_headers, if print_headers { Some(files[i]) } else { None });
            close(fd);
        }
    }

    exit_code
}
