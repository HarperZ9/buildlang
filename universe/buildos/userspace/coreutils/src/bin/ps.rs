// ===============================================================================
// PS - REPORT A SNAPSHOT OF CURRENT PROCESSES
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
    let mut show_all = false;
    let mut show_full = false;

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
                    println("Usage: ps [OPTION]...");
                    println("Report a snapshot of current processes.");
                    println("");
                    println("  -a       select all with a tty");
                    println("  -e, -A   select all processes");
                    println("  -f       full-format listing");
                    println("  -l       long format");
                    return 0;
                }
            } else {
                for i in 1..arg.len() {
                    match arg[i] {
                        b'a' => show_all = true,
                        b'e' | b'A' => show_all = true,
                        b'f' => show_full = true,
                        b'l' => show_full = true,
                        _ => {}
                    }
                }
            }
        }
    }

    // In BuildOS, we'd read from /proc
    // For now, just show current process

    if show_full {
        println("UID        PID  PPID  C STIME TTY          TIME CMD");
    } else {
        println("  PID TTY          TIME CMD");
    }

    let pid = getpid();
    let ppid = getppid();
    let uid = getuid();

    if show_full {
        // Full format
        print_num(uid as u64);
        for _ in 0..10 - count_digits(uid as u64) {
            print(" ");
        }
        print_num(pid as u64);
        for _ in 0..6 - count_digits(pid as u64) {
            print(" ");
        }
        print_num(ppid as u64);
        for _ in 0..6 - count_digits(ppid as u64) {
            print(" ");
        }
        print("0 00:00 ?        00:00:00 ");
    } else {
        // Short format
        for _ in 0..5 - count_digits(pid as u64) {
            print(" ");
        }
        print_num(pid as u64);
        print(" ?        00:00:00 ");
    }

    println("ps");

    // Try to read /proc to list other processes
    let fd = open(b"/proc\0", O_RDONLY | O_DIRECTORY, 0);
    if fd >= 0 {
        let mut buf = [0u8; 4096];
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

                // Check if directory name is a number (PID)
                if name_len > 0 && name[0] >= b'0' && name[0] <= b'9' {
                    if let Some(proc_pid) = parse_num(name) {
                        if proc_pid as i32 != pid {
                            // Read /proc/PID/comm for command name
                            let mut comm_path = [0u8; 64];
                            let prefix = b"/proc/";
                            comm_path[..prefix.len()].copy_from_slice(prefix);
                            let mut p = prefix.len();
                            comm_path[p..p + name_len].copy_from_slice(name);
                            p += name_len;
                            let suffix = b"/comm\0";
                            comm_path[p..p + suffix.len()].copy_from_slice(suffix);

                            let comm_fd = open(&comm_path, O_RDONLY, 0);
                            let mut cmd = [0u8; 256];
                            let mut cmd_len = 0;
                            if comm_fd >= 0 {
                                cmd_len = read(comm_fd, &mut cmd);
                                if cmd_len > 0 && cmd[cmd_len as usize - 1] == b'\n' {
                                    cmd_len -= 1;
                                }
                                close(comm_fd);
                            }

                            if show_full {
                                print("0         ");
                                print_num(proc_pid);
                                for _ in 0..6 - count_digits(proc_pid) {
                                    print(" ");
                                }
                                print("    0  0 00:00 ?        00:00:00 ");
                            } else {
                                for _ in 0..5 - count_digits(proc_pid) {
                                    print(" ");
                                }
                                print_num(proc_pid);
                                print(" ?        00:00:00 ");
                            }

                            if cmd_len > 0 {
                                print_bytes(&cmd[..cmd_len as usize]);
                            } else {
                                print("[unknown]");
                            }
                            println("");
                        }
                    }
                }

                pos += dirent.d_reclen as usize;
            }
        }
        close(fd);
    }

    0
}

fn count_digits(mut n: u64) -> usize {
    if n == 0 {
        return 1;
    }
    let mut count = 0;
    while n > 0 {
        count += 1;
        n /= 10;
    }
    count
}
