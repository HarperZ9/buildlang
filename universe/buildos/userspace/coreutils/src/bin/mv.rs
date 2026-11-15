// ===============================================================================
// MV - MOVE (RENAME) FILES
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]
#![allow(unused_mut)]

use coreutils_common::*;

entry!(main);

struct MvOptions {
    force: bool,
    verbose: bool,
    no_clobber: bool,
    interactive: bool,
}

impl MvOptions {
    fn new() -> Self {
        Self {
            force: false,
            verbose: false,
            no_clobber: false,
            interactive: false,
        }
    }
}

fn move_file(src: &[u8], dst: &[u8], opts: &MvOptions) -> i32 {
    let mut src_stat = Stat::zeroed();
    if lstat(src, &mut src_stat) < 0 {
        eprint("mv: cannot stat '");
        print_bytes(&src[..src.len().saturating_sub(1)]);
        eprintln("': No such file or directory");
        return 1;
    }

    // Check if destination is a directory
    let mut dst_stat = Stat::zeroed();
    let dst_exists = stat(dst, &mut dst_stat) == 0;

    let mut actual_dst = [0u8; 512];
    let mut actual_dst_len: usize;

    if dst_exists && s_isdir(dst_stat.st_mode) {
        // Move into directory
        let src_name = basename(&src[..src.len().saturating_sub(1)]);
        let dst_len = dst.len().saturating_sub(1);
        actual_dst[..dst_len].copy_from_slice(&dst[..dst_len]);

        let mut pos = dst_len;
        if pos > 0 && actual_dst[pos - 1] != b'/' {
            actual_dst[pos] = b'/';
            pos += 1;
        }
        actual_dst[pos..pos + src_name.len()].copy_from_slice(src_name);
        pos += src_name.len();
        actual_dst[pos] = 0;
        actual_dst_len = pos + 1;
    } else {
        let dst_len = dst.len();
        actual_dst[..dst_len].copy_from_slice(dst);
        actual_dst_len = dst_len;
    }

    // Check if destination exists (after resolving directory)
    if stat(&actual_dst[..actual_dst_len], &mut dst_stat) == 0 {
        if opts.no_clobber {
            return 0;
        }
    }

    // Try rename first (same filesystem)
    let result = rename(src, &actual_dst[..actual_dst_len]);
    if result == 0 {
        if opts.verbose {
            eprint("renamed '");
            print_bytes(&src[..src.len().saturating_sub(1)]);
            eprint("' -> '");
            print_bytes(&actual_dst[..actual_dst_len.saturating_sub(1)]);
            eprintln("'");
        }
        return 0;
    }

    // If rename fails (cross-filesystem), fall back to copy + delete
    // This is a simplified version - in reality you'd copy the file then delete
    eprint("mv: cannot move '");
    print_bytes(&src[..src.len().saturating_sub(1)]);
    eprint("' to '");
    print_bytes(&actual_dst[..actual_dst_len.saturating_sub(1)]);
    eprintln("'");
    1
}

fn main(argc: usize, argv: *const *const u8) -> i32 {
    let mut opts = MvOptions::new();
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
                if str_eq(arg, b"--force") {
                    opts.force = true;
                } else if str_eq(arg, b"--verbose") {
                    opts.verbose = true;
                } else if str_eq(arg, b"--no-clobber") {
                    opts.no_clobber = true;
                } else if str_eq(arg, b"--interactive") {
                    opts.interactive = true;
                } else if str_eq(arg, b"--help") {
                    println("Usage: mv [OPTION]... SOURCE... DEST");
                    println("Rename SOURCE to DEST, or move SOURCE(s) to DIRECTORY.");
                    println("");
                    println("  -f, --force       do not prompt before overwriting");
                    println("  -n, --no-clobber  do not overwrite existing file");
                    println("  -v, --verbose     explain what is being done");
                    return 0;
                }
            } else {
                for i in 1..arg.len() {
                    match arg[i] {
                        b'f' => opts.force = true,
                        b'v' => opts.verbose = true,
                        b'n' => opts.no_clobber = true,
                        b'i' => opts.interactive = true,
                        _ => {
                            eprint("mv: invalid option -- '");
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

    if file_count < 2 {
        eprintln("mv: missing destination operand");
        return 1;
    }

    let dst = files[file_count - 1];
    let mut dst_buf = [0u8; 512];
    let dst_len = dst.len().min(511);
    dst_buf[..dst_len].copy_from_slice(&dst[..dst_len]);
    dst_buf[dst_len] = 0;

    let mut exit_code = 0;

    for i in 0..file_count - 1 {
        let mut src_buf = [0u8; 512];
        let src_len = files[i].len().min(511);
        src_buf[..src_len].copy_from_slice(&files[i][..src_len]);
        src_buf[src_len] = 0;

        let result = move_file(&src_buf[..src_len + 1], &dst_buf[..dst_len + 1], &opts);
        if result != 0 {
            exit_code = result;
        }
    }

    exit_code
}
