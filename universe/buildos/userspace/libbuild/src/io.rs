// ===============================================================================
// I/O UTILITIES
// ===============================================================================

use crate::syscall::*;

// =============================================================================
// FILE DESCRIPTORS
// =============================================================================

pub const STDIN: i32 = 0;
pub const STDOUT: i32 = 1;
pub const STDERR: i32 = 2;

// =============================================================================
// BASIC I/O
// =============================================================================

/// Write bytes to a file descriptor
pub fn write(fd: i32, buf: &[u8]) -> isize {
    unsafe {
        syscall3(SYS_WRITE, fd as u64, buf.as_ptr() as u64, buf.len() as u64) as isize
    }
}

/// Read bytes from a file descriptor
pub fn read(fd: i32, buf: &mut [u8]) -> isize {
    unsafe {
        syscall3(SYS_READ, fd as u64, buf.as_mut_ptr() as u64, buf.len() as u64) as isize
    }
}

/// Close a file descriptor
pub fn close(fd: i32) -> i32 {
    unsafe { syscall1(SYS_CLOSE, fd as u64) as i32 }
}

/// Duplicate a file descriptor
pub fn dup(fd: i32) -> i32 {
    unsafe { syscall1(SYS_DUP, fd as u64) as i32 }
}

/// Duplicate a file descriptor to a specific number
pub fn dup2(oldfd: i32, newfd: i32) -> i32 {
    unsafe { syscall2(SYS_DUP2, oldfd as u64, newfd as u64) as i32 }
}

// =============================================================================
// OUTPUT UTILITIES
// =============================================================================

/// Print a string to stdout
pub fn print(s: &str) {
    write(STDOUT, s.as_bytes());
}

/// Print a string followed by newline to stdout
pub fn println(s: &str) {
    print(s);
    print("\n");
}

/// Print bytes to stdout
pub fn print_bytes(buf: &[u8]) {
    write(STDOUT, buf);
}

/// Print an unsigned number to stdout
pub fn print_num(mut n: u64) {
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

    write(STDOUT, &buf[i..]);
}

/// Print a signed number to stdout
pub fn print_signed(n: i64) {
    if n < 0 {
        print("-");
        print_num((-n) as u64);
    } else {
        print_num(n as u64);
    }
}

/// Print a number in hexadecimal
pub fn print_hex(mut n: u64) {
    print("0x");
    if n == 0 {
        print("0");
        return;
    }

    let mut buf = [0u8; 16];
    let mut i = 16;

    while n > 0 {
        i -= 1;
        let digit = (n & 0xf) as u8;
        buf[i] = if digit < 10 { b'0' + digit } else { b'a' + digit - 10 };
        n >>= 4;
    }

    write(STDOUT, &buf[i..]);
}

/// Print to stderr
pub fn eprint(s: &str) {
    write(STDERR, s.as_bytes());
}

/// Print to stderr with newline
pub fn eprintln(s: &str) {
    eprint(s);
    eprint("\n");
}

// =============================================================================
// INPUT UTILITIES
// =============================================================================

/// Read a single byte from stdin
pub fn getchar() -> Option<u8> {
    let mut buf = [0u8; 1];
    if read(STDIN, &mut buf) == 1 {
        Some(buf[0])
    } else {
        None
    }
}

/// Read a line from stdin (up to newline or buffer full)
pub fn readline(buf: &mut [u8]) -> usize {
    let mut len = 0;
    while len < buf.len() {
        let mut ch = [0u8; 1];
        if read(STDIN, &mut ch) != 1 {
            break;
        }
        if ch[0] == b'\n' {
            break;
        }
        buf[len] = ch[0];
        len += 1;
    }
    len
}
