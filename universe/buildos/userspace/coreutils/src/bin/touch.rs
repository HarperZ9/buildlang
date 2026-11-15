// ===============================================================================
// TOUCH - CHANGE FILE TIMESTAMPS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]

use coreutils_common::*;

entry!(main);

const UTIME_NOW: i64 = (1 << 30) - 1;
const UTIME_OMIT: i64 = (1 << 30) - 2;
const AT_FDCWD: i32 = -100;

fn main(argc: usize, argv: *const *const u8) -> i32 {
    let mut no_create = false;
    let mut access_only = false;
    let mut modify_only = false;
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
                if str_eq(arg, b"--no-create") {
                    no_create = true;
                } else if str_eq(arg, b"--help") {
                    println("Usage: touch [OPTION]... FILE...");
                    println("Update access and modification times of each FILE.");
                    println("");
                    println("  -a         change only the access time");
                    println("  -c, --no-create  do not create any files");
                    println("  -m         change only the modification time");
                    return 0;
                }
            } else {
                for i in 1..arg.len() {
                    match arg[i] {
                        b'a' => access_only = true,
                        b'c' => no_create = true,
                        b'm' => modify_only = true,
                        _ => {
                            eprint("touch: invalid option -- '");
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
        eprintln("touch: missing file operand");
        return 1;
    }

    let mut exit_code = 0;

    for i in 0..file_count {
        let mut path_buf = [0u8; 512];
        let len = files[i].len().min(511);
        path_buf[..len].copy_from_slice(&files[i][..len]);
        path_buf[len] = 0;

        // Check if file exists
        let mut st = Stat::zeroed();
        let exists = stat(&path_buf[..len + 1], &mut st) == 0;

        if !exists {
            if no_create {
                continue;
            }
            // Create the file
            let fd = open(&path_buf[..len + 1], O_CREAT | O_WRONLY, 0o644);
            if fd < 0 {
                eprint("touch: cannot touch '");
                print_bytes(files[i]);
                eprintln("': Permission denied");
                exit_code = 1;
                continue;
            }
            close(fd);
        }

        // Update timestamps
        let times = if access_only {
            [
                Timespec { tv_sec: 0, tv_nsec: UTIME_NOW },
                Timespec { tv_sec: 0, tv_nsec: UTIME_OMIT },
            ]
        } else if modify_only {
            [
                Timespec { tv_sec: 0, tv_nsec: UTIME_OMIT },
                Timespec { tv_sec: 0, tv_nsec: UTIME_NOW },
            ]
        } else {
            [
                Timespec { tv_sec: 0, tv_nsec: UTIME_NOW },
                Timespec { tv_sec: 0, tv_nsec: UTIME_NOW },
            ]
        };

        let result = utimensat(AT_FDCWD, &path_buf[..len + 1], &times, 0);
        if result < 0 {
            eprint("touch: cannot touch '");
            print_bytes(files[i]);
            eprintln("'");
            exit_code = 1;
        }
    }

    exit_code
}
