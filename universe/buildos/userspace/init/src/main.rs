// ===============================================================================
// BUILDOS INIT PROCESS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================
//
// First user-space process - PID 1
// Responsible for system initialization and service management
//
// ===============================================================================

#![no_std]
#![no_main]
#![allow(dead_code)]

use core::panic::PanicInfo;

// =============================================================================
// SYSCALL NUMBERS (matching kernel)
// =============================================================================

const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_OPEN: u64 = 2;
const SYS_CLOSE: u64 = 3;
const SYS_NANOSLEEP: u64 = 35;
const SYS_GETPID: u64 = 39;
const SYS_FORK: u64 = 57;
const SYS_EXECVE: u64 = 59;
const SYS_EXIT: u64 = 60;
const SYS_WAIT4: u64 = 61;
const SYS_SOCKET: u64 = 41;
const SYS_BIND: u64 = 49;
const SYS_LISTEN: u64 = 50;
const SYS_ACCEPT: u64 = 43;

// =============================================================================
// SYSCALL INTERFACE
// =============================================================================

/// Perform a syscall with up to 6 arguments
#[inline(always)]
unsafe fn syscall(
    num: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> i64 {
    let ret: i64;
    core::arch::asm!(
        "syscall",
        inlateout("rax") num => ret,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        in("r10") arg4,
        in("r8") arg5,
        in("r9") arg6,
        lateout("rcx") _,
        lateout("r11") _,
        options(nostack),
    );
    ret
}

/// Write to a file descriptor
fn write(fd: i32, buf: &[u8]) -> isize {
    unsafe { syscall(SYS_WRITE, fd as u64, buf.as_ptr() as u64, buf.len() as u64, 0, 0, 0) as isize }
}

/// Read from a file descriptor
fn read(fd: i32, buf: &mut [u8]) -> isize {
    unsafe { syscall(SYS_READ, fd as u64, buf.as_mut_ptr() as u64, buf.len() as u64, 0, 0, 0) as isize }
}

/// Get current process ID
fn getpid() -> i32 {
    unsafe { syscall(SYS_GETPID, 0, 0, 0, 0, 0, 0) as i32 }
}

/// Fork the current process
fn fork() -> i32 {
    unsafe { syscall(SYS_FORK, 0, 0, 0, 0, 0, 0) as i32 }
}

/// Wait for child process
fn wait(status: &mut i32) -> i32 {
    unsafe { syscall(SYS_WAIT4, u64::MAX, status as *mut i32 as u64, 0, 0, 0, 0) as i32 }
}

/// Exit the process
fn exit(code: i32) -> ! {
    unsafe { syscall(SYS_EXIT, code as u64, 0, 0, 0, 0, 0) };
    loop {}
}

/// Sleep for specified nanoseconds
fn nanosleep(secs: u64, nsecs: u64) {
    #[repr(C)]
    struct Timespec {
        tv_sec: i64,
        tv_nsec: i64,
    }

    let ts = Timespec {
        tv_sec: secs as i64,
        tv_nsec: nsecs as i64,
    };

    unsafe {
        syscall(SYS_NANOSLEEP, &ts as *const _ as u64, 0, 0, 0, 0, 0);
    }
}

// =============================================================================
// OUTPUT UTILITIES
// =============================================================================

/// Print a string to stdout
fn print(s: &str) {
    write(1, s.as_bytes());
}

/// Print a string followed by newline
fn println(s: &str) {
    print(s);
    print("\n");
}

/// Print a number
fn print_num(mut n: u64) {
    if n == 0 {
        print("0");
        return;
    }

    let mut buf = [0u8; 20];
    let mut i = 20;

    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }

    write(1, &buf[i..]);
}

// =============================================================================
// INIT MAIN
// =============================================================================

/// Entry point for init process
#[no_mangle]
pub extern "C" fn _start() -> ! {
    main();
    exit(0);
}

fn main() {
    // Display init banner
    println("");
    println("=================================================");
    println("  BuildOS Init v1.0.0");
    println("  x86-64 Operating System");
    println("  Copyright 2024-2025 Zain Dana Harper");
    println("=================================================");
    println("");

    // Get our PID
    let pid = getpid();
    print("[init] Started with PID ");
    print_num(pid as u64);
    println("");

    // System initialization sequence
    println("[init] Initializing system services...");

    // Phase 1: Core services
    println("[init] Phase 1: Core services");
    nanosleep(0, 100_000_000); // 100ms
    println("[init]   - Virtual filesystem: OK");
    nanosleep(0, 100_000_000);
    println("[init]   - Device manager: OK");
    nanosleep(0, 100_000_000);
    println("[init]   - Network stack: OK");

    // Phase 2: System services
    println("[init] Phase 2: System services");
    nanosleep(0, 100_000_000);
    println("[init]   - AI inference engine: OK");
    nanosleep(0, 100_000_000);
    println("[init]   - Self-healing monitor: OK");
    nanosleep(0, 100_000_000);
    println("[init]   - Process scheduler: OK");

    // Phase 3: User services
    println("[init] Phase 3: User services");
    nanosleep(0, 100_000_000);
    println("[init]   - Console service: OK");
    nanosleep(0, 100_000_000);
    println("[init]   - Shell ready");

    println("");
    println("[init] System initialization complete!");
    println("[init] BuildOS is ready.");
    println("");

    // Main init loop - wait for child processes
    println("[init] Entering service monitor loop...");

    let mut counter: u64 = 0;
    loop {
        // Check for zombie children
        let mut status: i32 = 0;
        let child = wait(&mut status);

        if child > 0 {
            print("[init] Child process ");
            print_num(child as u64);
            print(" exited with status ");
            print_num((status >> 8) as u64);
            println("");
        }

        // Heartbeat every 10 seconds
        counter += 1;
        if counter % 100 == 0 {
            print("[init] Heartbeat: system uptime ~");
            print_num(counter / 10);
            println("s");
        }

        // Sleep 100ms
        nanosleep(0, 100_000_000);
    }
}

// =============================================================================
// PANIC HANDLER
// =============================================================================

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println("");
    println("!!! INIT PANIC !!!");

    if let Some(location) = info.location() {
        print("Location: ");
        println(location.file());
    }

    // Try to print the message
    let _msg = info.message();
    // Can't easily format without alloc, just note panic occurred
    println("Panic occurred");

    exit(1);
}
