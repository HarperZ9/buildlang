// ===============================================================================
// STAT - DISPLAY FILE STATUS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]

use coreutils_common::*;

entry!(main);

fn file_type_str(mode: u32) -> &'static str {
    match mode & S_IFMT {
        S_IFREG => "regular file",
        S_IFDIR => "directory",
        S_IFLNK => "symbolic link",
        S_IFCHR => "character special file",
        S_IFBLK => "block special file",
        S_IFIFO => "fifo",
        S_IFSOCK => "socket",
        _ => "unknown",
    }
}

fn main(argc: usize, argv: *const *const u8) -> i32 {
    let mut dereference = true;
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
                if str_eq(arg, b"--help") {
                    println("Usage: stat [OPTION]... FILE...");
                    println("Display file or file system status.");
                    println("");
                    println("  -L, --dereference  follow links");
                    println("  -f, --file-system  display file system status");
                    return 0;
                }
            } else {
                for i in 1..arg.len() {
                    match arg[i] {
                        b'L' => dereference = true,
                        _ => {
                            eprint("stat: invalid option -- '");
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
        eprintln("stat: missing operand");
        return 1;
    }

    let mut exit_code = 0;

    for i in 0..file_count {
        let mut path_buf = [0u8; 512];
        let len = files[i].len().min(511);
        path_buf[..len].copy_from_slice(&files[i][..len]);
        path_buf[len] = 0;

        let mut st = Stat::zeroed();
        let result = if dereference {
            stat(&path_buf[..len + 1], &mut st)
        } else {
            lstat(&path_buf[..len + 1], &mut st)
        };

        if result < 0 {
            eprint("stat: cannot stat '");
            print_bytes(files[i]);
            eprintln("': No such file or directory");
            exit_code = 1;
            continue;
        }

        // Print file information
        print("  File: ");
        print_bytes(files[i]);
        println("");

        print("  Size: ");
        print_num(st.st_size as u64);
        print("\tBlocks: ");
        print_num(st.st_blocks as u64);
        print("\tIO Block: ");
        print_num(st.st_blksize as u64);
        print("\t");
        println(file_type_str(st.st_mode));

        print("Device: ");
        print_hex(st.st_dev);
        print("\tInode: ");
        print_num(st.st_ino);
        print("\tLinks: ");
        print_num(st.st_nlink);
        println("");

        print("Access: (");
        print_octal((st.st_mode & 0o777) as u64);
        print("/");
        let mut mode_buf = [0u8; 10];
        format_mode(st.st_mode, &mut mode_buf);
        print_bytes(&mode_buf);
        print(")  Uid: (");
        print_num(st.st_uid as u64);
        print(")   Gid: (");
        print_num(st.st_gid as u64);
        println(")");

        print("Access: ");
        print_num(st.st_atime as u64);
        println("");

        print("Modify: ");
        print_num(st.st_mtime as u64);
        println("");

        print("Change: ");
        print_num(st.st_ctime as u64);
        println("");

        if i < file_count - 1 {
            println("");
        }
    }

    exit_code
}
