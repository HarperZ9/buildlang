// ===============================================================================
// UNAME - PRINT SYSTEM INFORMATION
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]

use coreutils_common::*;

entry!(main);

fn print_field(buf: &[u8; 65]) {
    for &c in buf.iter() {
        if c == 0 {
            break;
        }
        write(STDOUT, &[c]);
    }
}

fn main(argc: usize, argv: *const *const u8) -> i32 {
    let mut print_kernel_name = false;
    let mut print_nodename = false;
    let mut print_kernel_release = false;
    let mut print_kernel_version = false;
    let mut print_machine = false;
    let mut print_all = false;

    let args = unsafe { ArgIter::new(argc, argv) };
    let mut skip_first = true;

    for arg in args {
        if skip_first {
            skip_first = false;
            continue;
        }

        if arg.len() > 1 && arg[0] == b'-' {
            if arg[1] == b'-' {
                if str_eq(arg, b"--all") {
                    print_all = true;
                } else if str_eq(arg, b"--kernel-name") {
                    print_kernel_name = true;
                } else if str_eq(arg, b"--nodename") {
                    print_nodename = true;
                } else if str_eq(arg, b"--kernel-release") {
                    print_kernel_release = true;
                } else if str_eq(arg, b"--kernel-version") {
                    print_kernel_version = true;
                } else if str_eq(arg, b"--machine") {
                    print_machine = true;
                } else if str_eq(arg, b"--help") {
                    println("Usage: uname [OPTION]...");
                    println("Print certain system information.");
                    println("");
                    println("  -a, --all            print all information");
                    println("  -s, --kernel-name    print kernel name");
                    println("  -n, --nodename       print network node hostname");
                    println("  -r, --kernel-release print kernel release");
                    println("  -v, --kernel-version print kernel version");
                    println("  -m, --machine        print machine hardware name");
                    return 0;
                }
            } else {
                for i in 1..arg.len() {
                    match arg[i] {
                        b'a' => print_all = true,
                        b's' => print_kernel_name = true,
                        b'n' => print_nodename = true,
                        b'r' => print_kernel_release = true,
                        b'v' => print_kernel_version = true,
                        b'm' => print_machine = true,
                        _ => {
                            eprint("uname: invalid option -- '");
                            write(STDERR, &[arg[i]]);
                            eprintln("'");
                            return 1;
                        }
                    }
                }
            }
        }
    }

    // Default to kernel name only
    let default = !print_kernel_name && !print_nodename && !print_kernel_release
        && !print_kernel_version && !print_machine && !print_all;

    if default {
        print_kernel_name = true;
    }

    if print_all {
        print_kernel_name = true;
        print_nodename = true;
        print_kernel_release = true;
        print_kernel_version = true;
        print_machine = true;
    }

    let mut buf = Utsname::zeroed();
    let result = uname(&mut buf);

    if result < 0 {
        eprintln("uname: cannot get system information");
        return 1;
    }

    let mut first = true;

    if print_kernel_name {
        print_field(&buf.sysname);
        first = false;
    }

    if print_nodename {
        if !first {
            print(" ");
        }
        print_field(&buf.nodename);
        first = false;
    }

    if print_kernel_release {
        if !first {
            print(" ");
        }
        print_field(&buf.release);
        first = false;
    }

    if print_kernel_version {
        if !first {
            print(" ");
        }
        print_field(&buf.version);
        first = false;
    }

    if print_machine {
        if !first {
            print(" ");
        }
        print_field(&buf.machine);
    }

    println("");

    0
}
