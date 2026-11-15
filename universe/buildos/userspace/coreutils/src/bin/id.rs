// ===============================================================================
// ID - PRINT REAL AND EFFECTIVE USER AND GROUP IDS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]

use coreutils_common::*;

entry!(main);

fn main(argc: usize, argv: *const *const u8) -> i32 {
    let mut print_user = false;
    let mut print_group = false;
    let mut print_groups = false;
    let mut print_name = false;
    let mut print_real = false;

    let args = unsafe { ArgIter::new(argc, argv) };
    let mut skip_first = true;

    for arg in args {
        if skip_first {
            skip_first = false;
            continue;
        }

        if arg.len() > 1 && arg[0] == b'-' {
            if arg[1] == b'-' {
                if str_eq(arg, b"--user") {
                    print_user = true;
                } else if str_eq(arg, b"--group") {
                    print_group = true;
                } else if str_eq(arg, b"--groups") {
                    print_groups = true;
                } else if str_eq(arg, b"--name") {
                    print_name = true;
                } else if str_eq(arg, b"--real") {
                    print_real = true;
                } else if str_eq(arg, b"--help") {
                    println("Usage: id [OPTION]... [USER]");
                    println("Print user and group information.");
                    println("");
                    println("  -g, --group   print only the effective group ID");
                    println("  -G, --groups  print all group IDs");
                    println("  -n, --name    print a name instead of a number");
                    println("  -r, --real    print the real ID instead of the effective ID");
                    println("  -u, --user    print only the effective user ID");
                    return 0;
                }
            } else {
                for i in 1..arg.len() {
                    match arg[i] {
                        b'u' => print_user = true,
                        b'g' => print_group = true,
                        b'G' => print_groups = true,
                        b'n' => print_name = true,
                        b'r' => print_real = true,
                        _ => {
                            eprint("id: invalid option -- '");
                            write(STDERR, &[arg[i]]);
                            eprintln("'");
                            return 1;
                        }
                    }
                }
            }
        }
    }

    let uid = if print_real { getuid() } else { geteuid() };
    let gid = if print_real { getgid() } else { getegid() };

    if print_user {
        if print_name {
            // Would need /etc/passwd lookup - just print uid for now
            if uid == 0 {
                println("root");
            } else {
                print("user");
                print_num(uid as u64);
                println("");
            }
        } else {
            print_num(uid as u64);
            println("");
        }
        return 0;
    }

    if print_group {
        if print_name {
            // Would need /etc/group lookup
            if gid == 0 {
                println("root");
            } else {
                print("group");
                print_num(gid as u64);
                println("");
            }
        } else {
            print_num(gid as u64);
            println("");
        }
        return 0;
    }

    if print_groups {
        // Just print primary group for now
        print_num(gid as u64);
        println("");
        return 0;
    }

    // Default: print all info
    print("uid=");
    print_num(uid as u64);
    print("(");
    if uid == 0 {
        print("root");
    } else {
        print("user");
        print_num(uid as u64);
    }
    print(") gid=");
    print_num(gid as u64);
    print("(");
    if gid == 0 {
        print("root");
    } else {
        print("group");
        print_num(gid as u64);
    }
    print(") groups=");
    print_num(gid as u64);
    print("(");
    if gid == 0 {
        print("root");
    } else {
        print("group");
        print_num(gid as u64);
    }
    println(")");

    0
}
