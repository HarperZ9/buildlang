// ===============================================================================
// MKDIR - CREATE DIRECTORIES
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]

use coreutils_common::*;

entry!(main);

fn make_directory(path: &[u8], mode: u32, parents: bool, verbose: bool) -> i32 {
    if parents {
        // Create parent directories as needed
        let mut partial = [0u8; 512];
        let mut pos = 0;

        // Skip leading slash
        let start = if path.len() > 0 && path[0] == b'/' {
            partial[0] = b'/';
            pos = 1;
            1
        } else {
            0
        };

        for i in start..path.len() {
            if path[i] == b'/' || path[i] == 0 {
                if pos > 0 {
                    partial[pos] = 0;

                    // Check if directory exists
                    let mut st = Stat::zeroed();
                    if stat(&partial[..pos + 1], &mut st) < 0 {
                        // Doesn't exist, create it
                        let result = mkdir(&partial[..pos + 1], mode);
                        if result < 0 && result != -17 {  // -17 = EEXIST
                            eprint("mkdir: cannot create directory '");
                            print_bytes(&partial[..pos]);
                            eprintln("'");
                            return 1;
                        }
                        if verbose && result == 0 {
                            eprint("mkdir: created directory '");
                            print_bytes(&partial[..pos]);
                            eprintln("'");
                        }
                    }
                }

                if path[i] == 0 {
                    break;
                }
                partial[pos] = b'/';
                pos += 1;
            } else {
                partial[pos] = path[i];
                pos += 1;
            }
        }

        // Create final directory
        if pos > 0 && partial[pos - 1] != b'/' {
            partial[pos] = 0;
            let result = mkdir(&partial[..pos + 1], mode);
            if result < 0 && result != -17 {
                eprint("mkdir: cannot create directory '");
                print_bytes(&partial[..pos]);
                eprintln("'");
                return 1;
            }
            if verbose && result == 0 {
                eprint("mkdir: created directory '");
                print_bytes(&partial[..pos]);
                eprintln("'");
            }
        }
    } else {
        // Just create the directory
        let result = mkdir(path, mode);
        if result < 0 {
            let path_str = &path[..path.len().saturating_sub(1)];
            if result == -17 {  // EEXIST
                eprint("mkdir: cannot create directory '");
                print_bytes(path_str);
                eprintln("': File exists");
            } else {
                eprint("mkdir: cannot create directory '");
                print_bytes(path_str);
                eprintln("'");
            }
            return 1;
        }
        if verbose {
            eprint("mkdir: created directory '");
            print_bytes(&path[..path.len().saturating_sub(1)]);
            eprintln("'");
        }
    }

    0
}

fn main(argc: usize, argv: *const *const u8) -> i32 {
    let mut mode: u32 = 0o755;
    let mut parents = false;
    let mut verbose = false;
    let mut dirs: [&[u8]; 64] = [&[]; 64];
    let mut dir_count = 0;

    let args = unsafe { ArgIter::new(argc, argv) };
    let mut skip_first = true;
    let mut expect_mode = false;

    for arg in args {
        if skip_first {
            skip_first = false;
            continue;
        }

        if expect_mode {
            if let Some(m) = parse_octal(arg) {
                mode = m;
            } else {
                eprint("mkdir: invalid mode '");
                print_bytes(arg);
                eprintln("'");
                return 1;
            }
            expect_mode = false;
            continue;
        }

        if arg.len() > 1 && arg[0] == b'-' {
            if arg[1] == b'-' {
                if str_eq(arg, b"--parents") {
                    parents = true;
                } else if str_eq(arg, b"--verbose") {
                    verbose = true;
                } else if str_starts_with(arg, b"--mode=") {
                    if let Some(m) = parse_octal(&arg[7..]) {
                        mode = m;
                    }
                } else if str_eq(arg, b"--help") {
                    println("Usage: mkdir [OPTION]... DIRECTORY...");
                    println("Create the DIRECTORY(ies), if they do not already exist.");
                    println("");
                    println("  -m, --mode=MODE   set file mode");
                    println("  -p, --parents     create parent directories as needed");
                    println("  -v, --verbose     print a message for each created directory");
                    return 0;
                }
            } else {
                for i in 1..arg.len() {
                    match arg[i] {
                        b'p' => parents = true,
                        b'v' => verbose = true,
                        b'm' => expect_mode = true,
                        _ => {
                            eprint("mkdir: invalid option -- '");
                            write(STDERR, &[arg[i]]);
                            eprintln("'");
                            return 1;
                        }
                    }
                }
            }
        } else if dir_count < 64 {
            dirs[dir_count] = arg;
            dir_count += 1;
        }
    }

    if dir_count == 0 {
        eprintln("mkdir: missing operand");
        return 1;
    }

    let mut exit_code = 0;

    for i in 0..dir_count {
        let mut path_buf = [0u8; 512];
        let len = dirs[i].len().min(511);
        path_buf[..len].copy_from_slice(&dirs[i][..len]);
        path_buf[len] = 0;

        let result = make_directory(&path_buf[..len + 1], mode, parents, verbose);
        if result != 0 {
            exit_code = result;
        }
    }

    exit_code
}
