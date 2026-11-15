// ===============================================================================
// LS - LIST DIRECTORY CONTENTS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]

use coreutils_common::*;

entry!(main);

struct LsOptions {
    long_format: bool,
    all: bool,
    human_readable: bool,
    reverse: bool,
    sort_by_size: bool,
    sort_by_time: bool,
    one_per_line: bool,
    show_inode: bool,
    classify: bool,
    recursive: bool,
}

impl LsOptions {
    fn new() -> Self {
        Self {
            long_format: false,
            all: false,
            human_readable: false,
            reverse: false,
            sort_by_size: false,
            sort_by_time: false,
            one_per_line: false,
            show_inode: false,
            classify: false,
            recursive: false,
        }
    }
}

struct DirEntry {
    name: [u8; 256],
    name_len: usize,
    d_type: u8,
    stat: Stat,
    has_stat: bool,
}

impl DirEntry {
    fn new() -> Self {
        Self {
            name: [0; 256],
            name_len: 0,
            d_type: DT_UNKNOWN,
            stat: Stat::zeroed(),
            has_stat: false,
        }
    }
}

fn format_size_human(size: i64) -> ([u8; 8], usize) {
    let mut buf = [0u8; 8];
    let units = [b'B', b'K', b'M', b'G', b'T', b'P'];

    let mut value = size as f64;
    let mut unit_idx = 0;

    while value >= 1024.0 && unit_idx < units.len() - 1 {
        value /= 1024.0;
        unit_idx += 1;
    }

    // Simple integer formatting for now
    let int_val = value as u64;
    let mut i = 0;

    if int_val == 0 {
        buf[0] = b'0';
        i = 1;
    } else {
        let mut temp = [0u8; 8];
        let mut j = 0;
        let mut n = int_val;
        while n > 0 {
            temp[j] = b'0' + (n % 10) as u8;
            n /= 10;
            j += 1;
        }
        // Reverse
        while j > 0 {
            j -= 1;
            buf[i] = temp[j];
            i += 1;
        }
    }

    buf[i] = units[unit_idx];
    i += 1;

    (buf, i)
}

fn print_long_entry(entry: &DirEntry, opts: &LsOptions) {
    if opts.show_inode {
        print_num(entry.stat.st_ino);
        print(" ");
    }

    // Mode string
    let mut mode_buf = [0u8; 10];
    format_mode(entry.stat.st_mode, &mut mode_buf);
    print_bytes(&mode_buf);
    print(" ");

    // Link count
    let nlink = entry.stat.st_nlink;
    if nlink < 10 {
        print(" ");
    }
    print_num(nlink);
    print(" ");

    // Owner/group (just show numeric for now)
    print_num(entry.stat.st_uid as u64);
    print(" ");
    print_num(entry.stat.st_gid as u64);
    print(" ");

    // Size
    if opts.human_readable {
        let (size_buf, len) = format_size_human(entry.stat.st_size);
        // Right-align to 5 chars
        for _ in len..5 {
            print(" ");
        }
        print_bytes(&size_buf[..len]);
    } else {
        // Right-align size to 8 chars
        let size = entry.stat.st_size as u64;
        let mut temp = size;
        let mut digits = if temp == 0 { 1 } else { 0 };
        while temp > 0 {
            digits += 1;
            temp /= 10;
        }
        for _ in digits..8 {
            print(" ");
        }
        print_num(size);
    }
    print(" ");

    // Date (simplified - just show timestamp)
    print_num(entry.stat.st_mtime as u64);
    print(" ");

    // Name
    print_bytes(&entry.name[..entry.name_len]);

    // Classify suffix
    if opts.classify {
        if s_isdir(entry.stat.st_mode) {
            print("/");
        } else if s_islnk(entry.stat.st_mode) {
            print("@");
        } else if entry.stat.st_mode & (S_IXUSR | S_IXGRP | S_IXOTH) != 0 {
            print("*");
        } else if s_isfifo(entry.stat.st_mode) {
            print("|");
        } else if s_issock(entry.stat.st_mode) {
            print("=");
        }
    }

    println("");
}

fn print_short_entry(entry: &DirEntry, opts: &LsOptions, is_last: bool) {
    if opts.show_inode {
        print_num(entry.stat.st_ino);
        print(" ");
    }

    print_bytes(&entry.name[..entry.name_len]);

    if opts.classify {
        if entry.d_type == DT_DIR || (entry.has_stat && s_isdir(entry.stat.st_mode)) {
            print("/");
        } else if entry.d_type == DT_LNK || (entry.has_stat && s_islnk(entry.stat.st_mode)) {
            print("@");
        } else if entry.has_stat && entry.stat.st_mode & (S_IXUSR | S_IXGRP | S_IXOTH) != 0 {
            print("*");
        }
    }

    if opts.one_per_line || is_last {
        println("");
    } else {
        print("  ");
    }
}

fn list_directory(path: &[u8], opts: &LsOptions, show_header: bool) -> i32 {
    // Open directory
    let fd = open(path, O_RDONLY | O_DIRECTORY, 0);
    if fd < 0 {
        eprint("ls: cannot access '");
        print_bytes(path);
        eprintln("': No such file or directory");
        return 1;
    }

    if show_header {
        print_bytes(path);
        println(":");
    }

    // Read directory entries
    let mut entries: [DirEntry; 256] = core::array::from_fn(|_| DirEntry::new());
    let mut entry_count = 0;

    let mut buf = [0u8; 4096];
    loop {
        let n = getdents64(fd, &mut buf);
        if n <= 0 {
            break;
        }

        let mut pos = 0usize;
        while pos < n as usize && entry_count < 256 {
            let dirent = unsafe { &*(buf.as_ptr().add(pos) as *const Dirent64) };
            let name_ptr = unsafe { buf.as_ptr().add(pos + 19) };
            let name_len = cstr_len(name_ptr);
            let name = unsafe { core::slice::from_raw_parts(name_ptr, name_len) };

            // Skip . and .. unless -a
            if !opts.all {
                if name_len > 0 && name[0] == b'.' {
                    pos += dirent.d_reclen as usize;
                    continue;
                }
            }

            // Store entry
            let entry = &mut entries[entry_count];
            entry.name_len = name_len.min(255);
            entry.name[..entry.name_len].copy_from_slice(&name[..entry.name_len]);
            entry.d_type = dirent.d_type;

            // Get stat info if needed for long format
            if opts.long_format || opts.sort_by_size || opts.sort_by_time {
                let mut full_path = [0u8; 512];
                let path_len = path.len();
                full_path[..path_len].copy_from_slice(path);
                if path_len > 0 && path[path_len - 1] != b'/' {
                    full_path[path_len] = b'/';
                    full_path[path_len + 1..path_len + 1 + entry.name_len]
                        .copy_from_slice(&entry.name[..entry.name_len]);
                    full_path[path_len + 1 + entry.name_len] = 0;
                } else {
                    full_path[path_len..path_len + entry.name_len]
                        .copy_from_slice(&entry.name[..entry.name_len]);
                    full_path[path_len + entry.name_len] = 0;
                }

                if lstat(&full_path, &mut entry.stat) == 0 {
                    entry.has_stat = true;
                }
            }

            entry_count += 1;
            pos += dirent.d_reclen as usize;
        }
    }

    close(fd);

    // Simple bubble sort by name (or size/time if specified)
    for i in 0..entry_count {
        for j in i + 1..entry_count {
            let swap = if opts.sort_by_size {
                if opts.reverse {
                    entries[i].stat.st_size < entries[j].stat.st_size
                } else {
                    entries[i].stat.st_size > entries[j].stat.st_size
                }
            } else if opts.sort_by_time {
                if opts.reverse {
                    entries[i].stat.st_mtime < entries[j].stat.st_mtime
                } else {
                    entries[i].stat.st_mtime > entries[j].stat.st_mtime
                }
            } else {
                // Sort by name
                let cmp = compare_names(&entries[i].name[..entries[i].name_len],
                                        &entries[j].name[..entries[j].name_len]);
                if opts.reverse { cmp > 0 } else { cmp < 0 }
            };

            if swap {
                // Swap entire entries
                entries.swap(i, j);
            }
        }
    }

    // Print entries
    for i in 0..entry_count {
        if opts.long_format {
            print_long_entry(&entries[i], opts);
        } else {
            print_short_entry(&entries[i], opts, i == entry_count - 1);
        }
    }

    // Handle recursive listing
    if opts.recursive {
        for i in 0..entry_count {
            let entry = &entries[i];
            if entry.d_type == DT_DIR || (entry.has_stat && s_isdir(entry.stat.st_mode)) {
                // Skip . and ..
                if entry.name_len == 1 && entry.name[0] == b'.' {
                    continue;
                }
                if entry.name_len == 2 && entry.name[0] == b'.' && entry.name[1] == b'.' {
                    continue;
                }

                // Build path
                let mut sub_path = [0u8; 512];
                let path_len = path.len();
                sub_path[..path_len].copy_from_slice(path);
                if path_len > 0 && path[path_len - 1] != b'/' {
                    sub_path[path_len] = b'/';
                    sub_path[path_len + 1..path_len + 1 + entry.name_len]
                        .copy_from_slice(&entry.name[..entry.name_len]);
                    sub_path[path_len + 1 + entry.name_len] = 0;

                    println("");
                    list_directory(&sub_path[..path_len + 1 + entry.name_len + 1], opts, true);
                }
            }
        }
    }

    0
}

fn compare_names(a: &[u8], b: &[u8]) -> i32 {
    let len = a.len().min(b.len());
    for i in 0..len {
        let ca = a[i].to_ascii_lowercase();
        let cb = b[i].to_ascii_lowercase();
        if ca < cb {
            return -1;
        }
        if ca > cb {
            return 1;
        }
    }
    if a.len() < b.len() {
        -1
    } else if a.len() > b.len() {
        1
    } else {
        0
    }
}

fn main(argc: usize, argv: *const *const u8) -> i32 {
    let mut opts = LsOptions::new();
    let mut paths: [&[u8]; 32] = [&[]; 32];
    let mut path_count = 0;

    // Parse arguments
    let args = unsafe { ArgIter::new(argc, argv) };
    let mut skip_first = true;

    for arg in args {
        if skip_first {
            skip_first = false;
            continue;
        }

        if arg.len() > 0 && arg[0] == b'-' {
            // Parse options
            for i in 1..arg.len() {
                match arg[i] {
                    b'l' => opts.long_format = true,
                    b'a' => opts.all = true,
                    b'h' => opts.human_readable = true,
                    b'r' => opts.reverse = true,
                    b'S' => opts.sort_by_size = true,
                    b't' => opts.sort_by_time = true,
                    b'1' => opts.one_per_line = true,
                    b'i' => opts.show_inode = true,
                    b'F' => opts.classify = true,
                    b'R' => opts.recursive = true,
                    _ => {
                        eprint("ls: invalid option -- '");
                        write(STDERR, &[arg[i]]);
                        eprintln("'");
                        return 1;
                    }
                }
            }
        } else if path_count < 32 {
            paths[path_count] = arg;
            path_count += 1;
        }
    }

    // Default to current directory
    if path_count == 0 {
        paths[0] = b".\0";
        path_count = 1;
    }

    let mut exit_code = 0;
    let show_headers = path_count > 1;

    for i in 0..path_count {
        // Ensure null-terminated
        let mut path_buf = [0u8; 512];
        let len = paths[i].len().min(511);
        path_buf[..len].copy_from_slice(&paths[i][..len]);
        path_buf[len] = 0;

        if i > 0 && show_headers {
            println("");
        }

        let result = list_directory(&path_buf[..len + 1], &opts, show_headers);
        if result != 0 {
            exit_code = result;
        }
    }

    exit_code
}
