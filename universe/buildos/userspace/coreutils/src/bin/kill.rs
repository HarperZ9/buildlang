// ===============================================================================
// KILL - SEND A SIGNAL TO A PROCESS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]

use coreutils_common::*;

entry!(main);

fn signal_from_name(name: &[u8]) -> Option<i32> {
    // Handle signal names (case insensitive)
    let name_upper: [u8; 16] = {
        let mut buf = [0u8; 16];
        for i in 0..name.len().min(16) {
            buf[i] = name[i].to_ascii_uppercase();
        }
        buf
    };
    let len = name.len().min(16);
    let n = &name_upper[..len];

    if str_eq(n, b"HUP") || str_eq(n, b"SIGHUP") { Some(SIGHUP) }
    else if str_eq(n, b"INT") || str_eq(n, b"SIGINT") { Some(SIGINT) }
    else if str_eq(n, b"KILL") || str_eq(n, b"SIGKILL") { Some(SIGKILL) }
    else if str_eq(n, b"TERM") || str_eq(n, b"SIGTERM") { Some(SIGTERM) }
    else if str_eq(n, b"STOP") || str_eq(n, b"SIGSTOP") { Some(SIGSTOP) }
    else if str_eq(n, b"CONT") || str_eq(n, b"SIGCONT") { Some(SIGCONT) }
    else { None }
}

fn main(argc: usize, argv: *const *const u8) -> i32 {
    let mut signal = SIGTERM;
    let mut list_signals = false;
    let mut pids: [i32; 64] = [0; 64];
    let mut pid_count = 0;

    let args = unsafe { ArgIter::new(argc, argv) };
    let mut skip_first = true;
    let mut expect_signal = false;

    for arg in args {
        if skip_first {
            skip_first = false;
            continue;
        }

        if expect_signal {
            if let Some(n) = parse_num(arg) {
                signal = n as i32;
            } else if let Some(sig) = signal_from_name(arg) {
                signal = sig;
            } else {
                eprint("kill: invalid signal specification: ");
                print_bytes(arg);
                eprintln("");
                return 1;
            }
            expect_signal = false;
            continue;
        }

        if arg.len() > 1 && arg[0] == b'-' {
            if arg[1] == b'-' {
                if str_eq(arg, b"--list") {
                    list_signals = true;
                } else if str_starts_with(arg, b"--signal=") {
                    let sig_arg = &arg[9..];
                    if let Some(n) = parse_num(sig_arg) {
                        signal = n as i32;
                    } else if let Some(sig) = signal_from_name(sig_arg) {
                        signal = sig;
                    }
                } else if str_eq(arg, b"--help") {
                    println("Usage: kill [OPTION]... PID...");
                    println("Send a signal to a process.");
                    println("");
                    println("  -s, --signal=SIGNAL  specify signal to send");
                    println("  -l, --list           list signal names");
                    println("  -9                   send SIGKILL");
                    println("  -15                  send SIGTERM (default)");
                    return 0;
                }
            } else if arg[1] == b'l' {
                list_signals = true;
            } else if arg[1] == b's' {
                expect_signal = true;
            } else if arg[1] >= b'0' && arg[1] <= b'9' {
                // -N syntax for signal number
                if let Some(n) = parse_num(&arg[1..]) {
                    signal = n as i32;
                }
            } else if let Some(sig) = signal_from_name(&arg[1..]) {
                signal = sig;
            } else {
                eprint("kill: invalid option -- '");
                write(STDERR, &[arg[1]]);
                eprintln("'");
                return 1;
            }
        } else {
            // Parse PID
            let is_negative = arg.len() > 0 && arg[0] == b'-';
            let num_start = if is_negative { 1 } else { 0 };

            if let Some(n) = parse_num(&arg[num_start..]) {
                let pid = if is_negative { -(n as i32) } else { n as i32 };
                if pid_count < 64 {
                    pids[pid_count] = pid;
                    pid_count += 1;
                }
            } else {
                eprint("kill: invalid process ID: ");
                print_bytes(arg);
                eprintln("");
                return 1;
            }
        }
    }

    if list_signals {
        println(" 1) SIGHUP       2) SIGINT       3) SIGQUIT      4) SIGILL");
        println(" 5) SIGTRAP      6) SIGABRT      7) SIGBUS       8) SIGFPE");
        println(" 9) SIGKILL     10) SIGUSR1     11) SIGSEGV     12) SIGUSR2");
        println("13) SIGPIPE     14) SIGALRM     15) SIGTERM     16) SIGSTKFLT");
        println("17) SIGCHLD     18) SIGCONT     19) SIGSTOP     20) SIGTSTP");
        return 0;
    }

    if pid_count == 0 {
        eprintln("kill: missing operand");
        return 1;
    }

    let mut exit_code = 0;

    for i in 0..pid_count {
        let result = kill(pids[i], signal);
        if result < 0 {
            eprint("kill: (");
            print_num(pids[i] as u64);
            eprintln("): No such process");
            exit_code = 1;
        }
    }

    exit_code
}
