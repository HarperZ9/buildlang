// ===============================================================================
// CP - COPY FILES AND DIRECTORIES
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]

use coreutils_common::*;

entry!(main);

struct CpOptions {
    recursive: bool,
    force: bool,
    verbose: bool,
    preserve: bool,
    no_clobber: bool,
    interactive: bool,
}

impl CpOptions {
    fn new() -> Self {
        Self {
            recursive: false,
            force: false,
            verbose: false,
            preserve: false,
            no_clobber: false,
            interactive: false,
        }
    }
}

fn copy_file(src: &[u8], dst: &[u8], opts: &CpOptions) -> i32 {
    let mut src_stat = Stat::zeroed();
    if stat(src, &mut src_stat) < 0 {
        eprint("cp: cannot stat '");
        print_bytes(&src[..src.len().saturating_sub(1)]);
        eprintln("': No such file or directory");
        return 1;
    }

    if s_isdir(src_stat.st_mode) {
        if opts.recursive {
            return copy_directory(src, dst, opts);
        } else {
            eprint("cp: -r not specified; omitting directory '");
            print_bytes(&src[..src.len().saturating_sub(1)]);
            eprintln("'");
            return 1;
        }
    }

    // Check if destination exists
    let mut dst_stat = Stat::zeroed();
    let dst_exists = stat(dst, &mut dst_stat) == 0;

    if dst_exists {
        if opts.no_clobber {
            return 0;
        }
        if s_isdir(dst_stat.st_mode) {
            // Copy into directory
            let src_name = basename(&src[..src.len().saturating_sub(1)]);
            let mut new_dst = [0u8; 512];
            let dst_len = dst.len().saturating_sub(1);
            new_dst[..dst_len].copy_from_slice(&dst[..dst_len]);

            let mut pos = dst_len;
            if pos > 0 && new_dst[pos - 1] != b'/' {
                new_dst[pos] = b'/';
                pos += 1;
            }
            new_dst[pos..pos + src_name.len()].copy_from_slice(src_name);
            pos += src_name.len();
            new_dst[pos] = 0;

            return copy_file_content(src, &new_dst[..pos + 1], &src_stat, opts);
        }
    }

    copy_file_content(src, dst, &src_stat, opts)
}

fn copy_file_content(src: &[u8], dst: &[u8], src_stat: &Stat, opts: &CpOptions) -> i32 {
    let src_fd = open(src, O_RDONLY, 0);
    if src_fd < 0 {
        eprint("cp: cannot open '");
        print_bytes(&src[..src.len().saturating_sub(1)]);
        eprintln("'");
        return 1;
    }

    let mode = if opts.preserve { src_stat.st_mode & 0o777 } else { 0o644 };
    let dst_fd = open(dst, O_WRONLY | O_CREAT | O_TRUNC, mode);
    if dst_fd < 0 {
        close(src_fd);
        eprint("cp: cannot create '");
        print_bytes(&dst[..dst.len().saturating_sub(1)]);
        eprintln("'");
        return 1;
    }

    let mut buf = [0u8; 8192];
    let mut exit_code = 0;

    loop {
        let n = read(src_fd, &mut buf);
        if n <= 0 {
            break;
        }

        let written = write(dst_fd, &buf[..n as usize]);
        if written != n {
            eprint("cp: error writing '");
            print_bytes(&dst[..dst.len().saturating_sub(1)]);
            eprintln("'");
            exit_code = 1;
            break;
        }
    }

    close(src_fd);
    close(dst_fd);

    if opts.verbose && exit_code == 0 {
        eprint("'");
        print_bytes(&src[..src.len().saturating_sub(1)]);
        eprint("' -> '");
        print_bytes(&dst[..dst.len().saturating_sub(1)]);
        eprintln("'");
    }

    exit_code
}

fn copy_directory(src: &[u8], dst: &[u8], opts: &CpOptions) -> i32 {
    // Create destination directory
    let mut src_stat = Stat::zeroed();
    if stat(src, &mut src_stat) < 0 {
        return 1;
    }

    let mode = if opts.preserve { src_stat.st_mode & 0o777 } else { 0o755 };

    let mut dst_stat = Stat::zeroed();
    if stat(dst, &mut dst_stat) < 0 {
        // Destination doesn't exist, create it
        if mkdir(dst, mode) < 0 {
            eprint("cp: cannot create directory '");
            print_bytes(&dst[..dst.len().saturating_sub(1)]);
            eprintln("'");
            return 1;
        }
    } else if !s_isdir(dst_stat.st_mode) {
        eprint("cp: cannot overwrite non-directory '");
        print_bytes(&dst[..dst.len().saturating_sub(1)]);
        eprintln("' with directory");
        return 1;
    }

    // Open source directory
    let fd = open(src, O_RDONLY | O_DIRECTORY, 0);
    if fd < 0 {
        return 1;
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

            // Build source path
            let mut src_child = [0u8; 512];
            let src_len = src.len().saturating_sub(1);
            src_child[..src_len].copy_from_slice(&src[..src_len]);
            let mut spos = src_len;
            if spos > 0 && src_child[spos - 1] != b'/' {
                src_child[spos] = b'/';
                spos += 1;
            }
            src_child[spos..spos + name_len].copy_from_slice(name);
            spos += name_len;
            src_child[spos] = 0;

            // Build destination path
            let mut dst_child = [0u8; 512];
            let dst_len = dst.len().saturating_sub(1);
            dst_child[..dst_len].copy_from_slice(&dst[..dst_len]);
            let mut dpos = dst_len;
            if dpos > 0 && dst_child[dpos - 1] != b'/' {
                dst_child[dpos] = b'/';
                dpos += 1;
            }
            dst_child[dpos..dpos + name_len].copy_from_slice(name);
            dpos += name_len;
            dst_child[dpos] = 0;

            let result = copy_file(&src_child[..spos + 1], &dst_child[..dpos + 1], opts);
            if result != 0 {
                exit_code = result;
            }

            pos += dirent.d_reclen as usize;
        }
    }

    close(fd);

    if opts.verbose && exit_code == 0 {
        eprint("'");
        print_bytes(&src[..src.len().saturating_sub(1)]);
        eprint("' -> '");
        print_bytes(&dst[..dst.len().saturating_sub(1)]);
        eprintln("'");
    }

    exit_code
}

fn main(argc: usize, argv: *const *const u8) -> i32 {
    let mut opts = CpOptions::new();
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
                if str_eq(arg, b"--recursive") {
                    opts.recursive = true;
                } else if str_eq(arg, b"--force") {
                    opts.force = true;
                } else if str_eq(arg, b"--verbose") {
                    opts.verbose = true;
                } else if str_eq(arg, b"--preserve") {
                    opts.preserve = true;
                } else if str_eq(arg, b"--no-clobber") {
                    opts.no_clobber = true;
                } else if str_eq(arg, b"--help") {
                    println("Usage: cp [OPTION]... SOURCE... DEST");
                    println("Copy SOURCE to DEST, or multiple SOURCE(s) to DIRECTORY.");
                    println("");
                    println("  -r, -R, --recursive  copy directories recursively");
                    println("  -f, --force          if existing dest, remove it");
                    println("  -v, --verbose        explain what is being done");
                    println("  -p, --preserve       preserve mode and timestamps");
                    println("  -n, --no-clobber     do not overwrite existing file");
                    return 0;
                }
            } else {
                for i in 1..arg.len() {
                    match arg[i] {
                        b'r' | b'R' => opts.recursive = true,
                        b'f' => opts.force = true,
                        b'v' => opts.verbose = true,
                        b'p' => opts.preserve = true,
                        b'n' => opts.no_clobber = true,
                        b'i' => opts.interactive = true,
                        b'a' => {
                            opts.recursive = true;
                            opts.preserve = true;
                        }
                        _ => {
                            eprint("cp: invalid option -- '");
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
        eprintln("cp: missing destination operand");
        return 1;
    }

    // Last argument is destination
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

        let result = copy_file(&src_buf[..src_len + 1], &dst_buf[..dst_len + 1], &opts);
        if result != 0 {
            exit_code = result;
        }
    }

    exit_code
}
