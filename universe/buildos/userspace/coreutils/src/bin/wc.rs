// ===============================================================================
// WC - WORD, LINE, CHARACTER, AND BYTE COUNT
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]

use coreutils_common::*;

entry!(main);

struct WcOptions {
    lines: bool,
    words: bool,
    chars: bool,
    bytes: bool,
    max_line_length: bool,
}

impl WcOptions {
    fn new() -> Self {
        Self {
            lines: false,
            words: false,
            chars: false,
            bytes: false,
            max_line_length: false,
        }
    }

    fn any_set(&self) -> bool {
        self.lines || self.words || self.chars || self.bytes || self.max_line_length
    }
}

struct WcCounts {
    lines: u64,
    words: u64,
    chars: u64,
    bytes: u64,
    max_line_length: u64,
}

impl WcCounts {
    fn new() -> Self {
        Self {
            lines: 0,
            words: 0,
            chars: 0,
            bytes: 0,
            max_line_length: 0,
        }
    }

    fn add(&mut self, other: &WcCounts) {
        self.lines += other.lines;
        self.words += other.words;
        self.chars += other.chars;
        self.bytes += other.bytes;
        if other.max_line_length > self.max_line_length {
            self.max_line_length = other.max_line_length;
        }
    }
}

fn wc_fd(fd: i32) -> WcCounts {
    let mut counts = WcCounts::new();
    let mut buf = [0u8; 8192];
    let mut in_word = false;
    let mut current_line_len: u64 = 0;

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        counts.bytes += n as u64;

        for i in 0..n as usize {
            let c = buf[i];
            counts.chars += 1;

            if c == b'\n' {
                counts.lines += 1;
                if current_line_len > counts.max_line_length {
                    counts.max_line_length = current_line_len;
                }
                current_line_len = 0;
                in_word = false;
            } else {
                current_line_len += 1;

                if c == b' ' || c == b'\t' || c == b'\r' {
                    in_word = false;
                } else if !in_word {
                    in_word = true;
                    counts.words += 1;
                }
            }
        }
    }

    // Handle last line without newline
    if current_line_len > counts.max_line_length {
        counts.max_line_length = current_line_len;
    }

    counts
}

fn print_counts(counts: &WcCounts, opts: &WcOptions, filename: Option<&[u8]>) {
    let print_all = !opts.any_set();

    if opts.lines || print_all {
        print_num(counts.lines);
        print(" ");
    }

    if opts.words || print_all {
        print_num(counts.words);
        print(" ");
    }

    if opts.bytes || print_all {
        print_num(counts.bytes);
        print(" ");
    }

    if opts.chars && !print_all {
        print_num(counts.chars);
        print(" ");
    }

    if opts.max_line_length {
        print_num(counts.max_line_length);
        print(" ");
    }

    if let Some(name) = filename {
        print_bytes(name);
    }
    println("");
}

fn main(argc: usize, argv: *const *const u8) -> i32 {
    let mut opts = WcOptions::new();
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
                if str_eq(arg, b"--lines") {
                    opts.lines = true;
                } else if str_eq(arg, b"--words") {
                    opts.words = true;
                } else if str_eq(arg, b"--bytes") {
                    opts.bytes = true;
                } else if str_eq(arg, b"--chars") {
                    opts.chars = true;
                } else if str_eq(arg, b"--max-line-length") {
                    opts.max_line_length = true;
                } else if str_eq(arg, b"--help") {
                    println("Usage: wc [OPTION]... [FILE]...");
                    println("Print newline, word, and byte counts for each FILE.");
                    println("");
                    println("  -c, --bytes          print byte counts");
                    println("  -m, --chars          print character counts");
                    println("  -l, --lines          print newline counts");
                    println("  -L, --max-line-length  print maximum line length");
                    println("  -w, --words          print word counts");
                    return 0;
                }
            } else {
                for i in 1..arg.len() {
                    match arg[i] {
                        b'l' => opts.lines = true,
                        b'w' => opts.words = true,
                        b'c' => opts.bytes = true,
                        b'm' => opts.chars = true,
                        b'L' => opts.max_line_length = true,
                        _ => {
                            eprint("wc: invalid option -- '");
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

    let mut total = WcCounts::new();
    let mut exit_code = 0;

    if file_count == 0 {
        let counts = wc_fd(STDIN);
        print_counts(&counts, &opts, None);
        return 0;
    }

    for i in 0..file_count {
        let mut path_buf = [0u8; 512];
        let len = files[i].len().min(511);
        path_buf[..len].copy_from_slice(&files[i][..len]);
        path_buf[len] = 0;

        let counts = if files[i].len() == 1 && files[i][0] == b'-' {
            wc_fd(STDIN)
        } else {
            let fd = open(&path_buf[..len + 1], O_RDONLY, 0);
            if fd < 0 {
                eprint("wc: ");
                print_bytes(files[i]);
                eprintln(": No such file or directory");
                exit_code = 1;
                continue;
            }
            let c = wc_fd(fd);
            close(fd);
            c
        };

        total.add(&counts);
        print_counts(&counts, &opts, Some(files[i]));
    }

    if file_count > 1 {
        print_counts(&total, &opts, Some(b"total"));
    }

    exit_code
}
