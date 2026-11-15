// ===============================================================================
// LN - CREATE LINKS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]

use coreutils_common::*;

entry!(main);

fn main(argc: usize, argv: *const *const u8) -> i32 {
    let mut symbolic = false;
    let mut force = false;
    let mut verbose = false;
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
                if str_eq(arg, b"--symbolic") {
                    symbolic = true;
                } else if str_eq(arg, b"--force") {
                    force = true;
                } else if str_eq(arg, b"--verbose") {
                    verbose = true;
                } else if str_eq(arg, b"--help") {
                    println("Usage: ln [OPTION]... TARGET LINK_NAME");
                    println("Create a link to TARGET with the name LINK_NAME.");
                    println("");
                    println("  -s, --symbolic  make symbolic links");
                    println("  -f, --force     remove existing destination files");
                    println("  -v, --verbose   print name of each linked file");
                    return 0;
                }
            } else {
                for i in 1..arg.len() {
                    match arg[i] {
                        b's' => symbolic = true,
                        b'f' => force = true,
                        b'v' => verbose = true,
                        _ => {
                            eprint("ln: invalid option -- '");
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
        eprintln("ln: missing file operand");
        return 1;
    }

    let target = files[0];
    let link_name = files[1];

    let mut target_buf = [0u8; 512];
    let target_len = target.len().min(511);
    target_buf[..target_len].copy_from_slice(&target[..target_len]);
    target_buf[target_len] = 0;

    let mut link_buf = [0u8; 512];
    let link_len = link_name.len().min(511);
    link_buf[..link_len].copy_from_slice(&link_name[..link_len]);
    link_buf[link_len] = 0;

    // Remove existing file if force
    if force {
        let mut st = Stat::zeroed();
        if lstat(&link_buf[..link_len + 1], &mut st) == 0 {
            unlink(&link_buf[..link_len + 1]);
        }
    }

    let result = if symbolic {
        symlink(&target_buf[..target_len + 1], &link_buf[..link_len + 1])
    } else {
        link(&target_buf[..target_len + 1], &link_buf[..link_len + 1])
    };

    if result < 0 {
        eprint("ln: failed to create ");
        if symbolic {
            eprint("symbolic ");
        }
        eprint("link '");
        print_bytes(link_name);
        eprint("' -> '");
        print_bytes(target);
        eprintln("'");
        return 1;
    }

    if verbose {
        eprint("'");
        print_bytes(link_name);
        eprint("' -> '");
        print_bytes(target);
        eprintln("'");
    }

    0
}
