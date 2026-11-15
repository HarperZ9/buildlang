// ===============================================================================
// RM - REMOVE FILES OR DIRECTORIES
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]

use coreutils_common::*;

entry!(main);

struct RmOptions {
    force: bool,
    recursive: bool,
    verbose: bool,
    interactive: bool,
    dir: bool,
}

impl RmOptions {
    fn new() -> Self {
        Self {
            force: false,
            recursive: false,
            verbose: false,
            interactive: false,
            dir: false,
        }
    }
}

fn remove_file(path: &[u8], opts: &RmOptions) -> i32 {
    let mut st = Stat::zeroed();
    let stat_result = lstat(path, &mut st);

    if stat_result < 0 {
        if !opts.force {
            eprint("rm: cannot remove '");
            print_bytes(&path[..path.len().saturating_sub(1)]);
            eprintln("': No such file or directory");
            return 1;
        }
        return 0;
    }

    if s_isdir(st.st_mode) {
        if opts.recursive {
            return remove_directory_recursive(path, opts);
        } else if opts.dir {
            let result = rmdir(path);
            if result < 0 {
                eprint("rm: cannot remove '");
                print_bytes(&path[..path.len().saturating_sub(1)]);
                eprintln("': Directory not empty");
                return 1;
            }
            if opts.verbose {
                eprint("removed directory '");
                print_bytes(&path[..path.len().saturating_sub(1)]);
                eprintln("'");
            }
            return 0;
        } else {
            eprint("rm: cannot remove '");
            print_bytes(&path[..path.len().saturating_sub(1)]);
            eprintln("': Is a directory");
            return 1;
        }
    }

    let result = unlink(path);
    if result < 0 {
        if !opts.force {
            eprint("rm: cannot remove '");
            print_bytes(&path[..path.len().saturating_sub(1)]);
            eprintln("'");
            return 1;
        }
        return 0;
    }

    if opts.verbose {
        eprint("removed '");
        print_bytes(&path[..path.len().saturating_sub(1)]);
        eprintln("'");
    }

    0
}

fn remove_directory_recursive(path: &[u8], opts: &RmOptions) -> i32 {
    // Open directory
    let fd = open(path, O_RDONLY | O_DIRECTORY, 0);
    if fd < 0 {
        if !opts.force {
            eprint("rm: cannot remove '");
            print_bytes(&path[..path.len().saturating_sub(1)]);
            eprintln("'");
            return 1;
        }
        return 0;
    }

    let mut buf = [0u8; 4096];
    let mut exit_code = 0;

    loop {
        let n = getdents64(fd, &mut buf);
        if n <= 0 {
            break;
        }

        let mut pos = 0usize;
        while pos < n as usize {
            let dirent = unsafe { &*(buf.as_ptr().add(pos) as *const Dirent64) };
            let name_ptr = unsafe { buf.as_ptr().add(pos + 19) };
            let name_len = cstr_len(name_ptr);
            let name = unsafe { core::slice::from_raw_parts(name_ptr, name_len) };

            // Skip . and ..
            if (name_len == 1 && name[0] == b'.') ||
               (name_len == 2 && name[0] == b'.' && name[1] == b'.') {
                pos += dirent.d_reclen as usize;
                continue;
            }

            // Build full path
            let mut child_path = [0u8; 512];
            let path_len = path.len().saturating_sub(1);  // Remove null
            child_path[..path_len].copy_from_slice(&path[..path_len]);
            if path_len > 0 && path[path_len - 1] != b'/' {
                child_path[path_len] = b'/';
                child_path[path_len + 1..path_len + 1 + name_len].copy_from_slice(name);
                child_path[path_len + 1 + name_len] = 0;
            } else {
                child_path[path_len..path_len + name_len].copy_from_slice(name);
                child_path[path_len + name_len] = 0;
            }

            let result = remove_file(&child_path, opts);
            if result != 0 {
                exit_code = result;
            }

            pos += dirent.d_reclen as usize;
        }
    }

    close(fd);

    // Remove the directory itself
    let result = rmdir(path);
    if result < 0 {
        if !opts.force {
            eprint("rm: cannot remove '");
            print_bytes(&path[..path.len().saturating_sub(1)]);
            eprintln("'");
            return 1;
        }
    } else if opts.verbose {
        eprint("removed directory '");
        print_bytes(&path[..path.len().saturating_sub(1)]);
        eprintln("'");
    }

    exit_code
}

fn main(argc: usize, argv: *const *const u8) -> i32 {
    let mut opts = RmOptions::new();
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
                } else if str_eq(arg, b"--recursive") {
                    opts.recursive = true;
                } else if str_eq(arg, b"--verbose") {
                    opts.verbose = true;
                } else if str_eq(arg, b"--interactive") {
                    opts.interactive = true;
                } else if str_eq(arg, b"--dir") {
                    opts.dir = true;
                } else if str_eq(arg, b"--help") {
                    println("Usage: rm [OPTION]... FILE...");
                    println("Remove (unlink) the FILE(s).");
                    println("");
                    println("  -f, --force       ignore nonexistent files");
                    println("  -r, -R, --recursive  remove directories recursively");
                    println("  -d, --dir         remove empty directories");
                    println("  -v, --verbose     explain what is being done");
                    return 0;
                }
            } else {
                for i in 1..arg.len() {
                    match arg[i] {
                        b'f' => opts.force = true,
                        b'r' | b'R' => opts.recursive = true,
                        b'v' => opts.verbose = true,
                        b'i' => opts.interactive = true,
                        b'd' => opts.dir = true,
                        _ => {
                            eprint("rm: invalid option -- '");
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
        if !opts.force {
            eprintln("rm: missing operand");
            return 1;
        }
        return 0;
    }

    let mut exit_code = 0;

    for i in 0..file_count {
        let mut path_buf = [0u8; 512];
        let len = files[i].len().min(511);
        path_buf[..len].copy_from_slice(&files[i][..len]);
        path_buf[len] = 0;

        let result = remove_file(&path_buf[..len + 1], &opts);
        if result != 0 {
            exit_code = result;
        }
    }

    exit_code
}
