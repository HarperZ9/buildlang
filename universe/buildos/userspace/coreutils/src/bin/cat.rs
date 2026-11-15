// ===============================================================================
// CAT - CONCATENATE AND PRINT FILES
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]

use coreutils_common::*;

entry!(main);

struct CatOptions {
    number_lines: bool,
    number_nonblank: bool,
    show_ends: bool,
    show_tabs: bool,
    squeeze_blank: bool,
    show_nonprinting: bool,
}

impl CatOptions {
    fn new() -> Self {
        Self {
            number_lines: false,
            number_nonblank: false,
            show_ends: false,
            show_tabs: false,
            squeeze_blank: false,
            show_nonprinting: false,
        }
    }
}

fn cat_fd(fd: i32, opts: &CatOptions) -> i32 {
    let mut buf = [0u8; 8192];
    let mut line_num: u64 = 1;
    let mut at_line_start = true;
    let mut prev_blank = false;

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        if !opts.number_lines && !opts.number_nonblank && !opts.show_ends
            && !opts.show_tabs && !opts.squeeze_blank && !opts.show_nonprinting {
            // Fast path - just copy
            write(STDOUT, &buf[..n as usize]);
        } else {
            // Process character by character
            for i in 0..n as usize {
                let c = buf[i];

                // Check for blank line squeezing
                if opts.squeeze_blank && c == b'\n' && at_line_start {
                    if prev_blank {
                        continue;
                    }
                    prev_blank = true;
                } else {
                    prev_blank = false;
                }

                // Print line number at start of line
                if at_line_start {
                    let is_blank = c == b'\n';
                    if opts.number_lines || (opts.number_nonblank && !is_blank) {
                        // Right-align line number to 6 digits
                        let mut temp = line_num;
                        let mut digits = 0;
                        while temp > 0 {
                            digits += 1;
                            temp /= 10;
                        }
                        if digits == 0 {
                            digits = 1;
                        }
                        for _ in digits..6 {
                            print(" ");
                        }
                        print_num(line_num);
                        print("\t");
                        line_num += 1;
                    }
                    at_line_start = false;
                }

                // Handle character output
                if c == b'\n' {
                    if opts.show_ends {
                        print("$");
                    }
                    println("");
                    at_line_start = true;
                } else if c == b'\t' && opts.show_tabs {
                    print("^I");
                } else if opts.show_nonprinting && (c < 32 || c >= 127) {
                    if c < 32 {
                        print("^");
                        write(STDOUT, &[c + 64]);
                    } else if c == 127 {
                        print("^?");
                    } else {
                        print("M-");
                        if c < 160 {
                            print("^");
                            write(STDOUT, &[c - 128 + 64]);
                        } else if c == 255 {
                            print("^?");
                        } else {
                            write(STDOUT, &[c - 128]);
                        }
                    }
                } else {
                    write(STDOUT, &[c]);
                }
            }
        }
    }

    0
}

fn cat_file(path: &[u8], opts: &CatOptions) -> i32 {
    // Handle stdin
    if path.len() == 1 && path[0] == b'-' {
        return cat_fd(STDIN, opts);
    }

    let fd = open(path, O_RDONLY, 0);
    if fd < 0 {
        eprint("cat: ");
        print_bytes(&path[..path.len().saturating_sub(1)]); // Remove null terminator
        eprintln(": No such file or directory");
        return 1;
    }

    let result = cat_fd(fd, opts);
    close(fd);
    result
}

fn main(argc: usize, argv: *const *const u8) -> i32 {
    let mut opts = CatOptions::new();
    let mut files: [&[u8]; 64] = [&[]; 64];
    let mut file_count = 0;

    let args = unsafe { ArgIter::new(argc, argv) };
    let mut skip_first = true;

    for arg in args {
        if skip_first {
            skip_first = false;
            continue;
        }

        if arg.len() > 1 && arg[0] == b'-' && arg[1] != b'-' {
            for i in 1..arg.len() {
                match arg[i] {
                    b'n' => opts.number_lines = true,
                    b'b' => {
                        opts.number_nonblank = true;
                        opts.number_lines = false;
                    }
                    b'E' => opts.show_ends = true,
                    b'T' => opts.show_tabs = true,
                    b's' => opts.squeeze_blank = true,
                    b'v' => opts.show_nonprinting = true,
                    b'A' => {
                        opts.show_nonprinting = true;
                        opts.show_ends = true;
                        opts.show_tabs = true;
                    }
                    b'e' => {
                        opts.show_nonprinting = true;
                        opts.show_ends = true;
                    }
                    b't' => {
                        opts.show_nonprinting = true;
                        opts.show_tabs = true;
                    }
                    _ => {
                        eprint("cat: invalid option -- '");
                        write(STDERR, &[arg[i]]);
                        eprintln("'");
                        return 1;
                    }
                }
            }
        } else if arg.len() >= 2 && arg[0] == b'-' && arg[1] == b'-' {
            if str_eq(arg, b"--help") {
                println("Usage: cat [OPTION]... [FILE]...");
                println("Concatenate FILE(s) to standard output.");
                println("");
                println("  -A        equivalent to -vET");
                println("  -b        number nonempty output lines");
                println("  -e        equivalent to -vE");
                println("  -E        display $ at end of each line");
                println("  -n        number all output lines");
                println("  -s        suppress repeated empty output lines");
                println("  -t        equivalent to -vT");
                println("  -T        display TAB characters as ^I");
                println("  -v        use ^ and M- notation");
                return 0;
            }
        } else if file_count < 64 {
            files[file_count] = arg;
            file_count += 1;
        }
    }

    // Default to stdin
    if file_count == 0 {
        return cat_fd(STDIN, &opts);
    }

    let mut exit_code = 0;

    for i in 0..file_count {
        let mut path_buf = [0u8; 512];
        let len = files[i].len().min(511);
        path_buf[..len].copy_from_slice(&files[i][..len]);
        path_buf[len] = 0;

        let result = cat_file(&path_buf[..len + 1], &opts);
        if result != 0 {
            exit_code = result;
        }
    }

    exit_code
}
