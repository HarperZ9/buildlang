// ===============================================================================
// CHOWN - CHANGE FILE OWNER AND GROUP
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]
#![allow(unused_variables)]
#![allow(unused_assignments)]

use coreutils_common::*;

entry!(main);

fn main(argc: usize, argv: *const *const u8) -> i32 {
    let mut verbose = false;
    let mut recursive = false;
    let mut owner_arg: Option<&[u8]> = None;
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
                if str_eq(arg, b"--verbose") {
                    verbose = true;
                } else if str_eq(arg, b"--recursive") {
                    recursive = true;
                } else if str_eq(arg, b"--help") {
                    println("Usage: chown [OPTION]... OWNER[:GROUP] FILE...");
                    println("Change the owner and/or group of each FILE.");
                    println("");
                    println("  -R, --recursive  operate on files recursively");
                    println("  -v, --verbose    output a diagnostic for every file");
                    return 0;
                }
            } else {
                for i in 1..arg.len() {
                    match arg[i] {
                        b'R' => recursive = true,
                        b'v' => verbose = true,
                        _ => {
                            eprint("chown: invalid option -- '");
                            write(STDERR, &[arg[i]]);
                            eprintln("'");
                            return 1;
                        }
                    }
                }
            }
        } else if owner_arg.is_none() {
            owner_arg = Some(arg);
        } else if file_count < 64 {
            files[file_count] = arg;
            file_count += 1;
        }
    }

    let owner_str = match owner_arg {
        Some(o) => o,
        None => {
            eprintln("chown: missing operand");
            return 1;
        }
    };

    if file_count == 0 {
        eprintln("chown: missing operand after owner");
        return 1;
    }

    // Parse owner:group or owner.group
    let mut uid: u32 = u32::MAX;
    let mut gid: u32 = u32::MAX;

    let mut colon_pos = None;
    for i in 0..owner_str.len() {
        if owner_str[i] == b':' || owner_str[i] == b'.' {
            colon_pos = Some(i);
            break;
        }
    }

    if let Some(pos) = colon_pos {
        // Parse uid and gid
        if pos > 0 {
            if let Some(u) = parse_num(&owner_str[..pos]) {
                uid = u as u32;
            }
        }
        if pos + 1 < owner_str.len() {
            if let Some(g) = parse_num(&owner_str[pos + 1..]) {
                gid = g as u32;
            }
        }
    } else {
        // Just uid
        if let Some(u) = parse_num(owner_str) {
            uid = u as u32;
        }
    }

    let mut exit_code = 0;

    for i in 0..file_count {
        let mut path_buf = [0u8; 512];
        let len = files[i].len().min(511);
        path_buf[..len].copy_from_slice(&files[i][..len]);
        path_buf[len] = 0;

        let result = chown(&path_buf[..len + 1], uid, gid);
        if result < 0 {
            eprint("chown: cannot access '");
            print_bytes(files[i]);
            eprintln("'");
            exit_code = 1;
        } else if verbose {
            eprint("ownership of '");
            print_bytes(files[i]);
            eprint("' changed to ");
            print_num(uid as u64);
            print(":");
            print_num(gid as u64);
            println("");
        }
    }

    exit_code
}
