// ===============================================================================
// BUILDOS COREUTILS - COMMON LIBRARY
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================
//
// Shared functionality for BuildOS coreutils
//
// ===============================================================================

#![no_std]
#![allow(dead_code)]

use core::panic::PanicInfo;

// =============================================================================
// SYSCALL NUMBERS (matching kernel)
// =============================================================================

pub const SYS_READ: u64 = 0;
pub const SYS_WRITE: u64 = 1;
pub const SYS_OPEN: u64 = 2;
pub const SYS_CLOSE: u64 = 3;
pub const SYS_STAT: u64 = 4;
pub const SYS_FSTAT: u64 = 5;
pub const SYS_LSTAT: u64 = 6;
pub const SYS_LSEEK: u64 = 8;
pub const SYS_MMAP: u64 = 9;
pub const SYS_MUNMAP: u64 = 11;
pub const SYS_BRK: u64 = 12;
pub const SYS_IOCTL: u64 = 16;
pub const SYS_DUP: u64 = 32;
pub const SYS_DUP2: u64 = 33;
pub const SYS_NANOSLEEP: u64 = 35;
pub const SYS_GETPID: u64 = 39;
pub const SYS_CLONE: u64 = 56;
pub const SYS_FORK: u64 = 57;
pub const SYS_EXECVE: u64 = 59;
pub const SYS_EXIT: u64 = 60;
pub const SYS_WAIT4: u64 = 61;
pub const SYS_KILL: u64 = 62;
pub const SYS_UNAME: u64 = 63;
pub const SYS_FCNTL: u64 = 72;
pub const SYS_FSYNC: u64 = 74;
pub const SYS_TRUNCATE: u64 = 76;
pub const SYS_FTRUNCATE: u64 = 77;
pub const SYS_GETCWD: u64 = 79;
pub const SYS_CHDIR: u64 = 80;
pub const SYS_FCHDIR: u64 = 81;
pub const SYS_RENAME: u64 = 82;
pub const SYS_MKDIR: u64 = 83;
pub const SYS_RMDIR: u64 = 84;
pub const SYS_CREAT: u64 = 85;
pub const SYS_LINK: u64 = 86;
pub const SYS_UNLINK: u64 = 87;
pub const SYS_SYMLINK: u64 = 88;
pub const SYS_READLINK: u64 = 89;
pub const SYS_CHMOD: u64 = 90;
pub const SYS_FCHMOD: u64 = 91;
pub const SYS_CHOWN: u64 = 92;
pub const SYS_FCHOWN: u64 = 93;
pub const SYS_LCHOWN: u64 = 94;
pub const SYS_UMASK: u64 = 95;
pub const SYS_GETTIMEOFDAY: u64 = 96;
pub const SYS_GETUID: u64 = 102;
pub const SYS_GETGID: u64 = 104;
pub const SYS_GETEUID: u64 = 107;
pub const SYS_GETEGID: u64 = 108;
pub const SYS_GETPPID: u64 = 110;
pub const SYS_SETHOSTNAME: u64 = 170;
pub const SYS_GETDENTS64: u64 = 217;
pub const SYS_CLOCK_GETTIME: u64 = 228;
pub const SYS_UTIMENSAT: u64 = 280;

// =============================================================================
// OPEN FLAGS
// =============================================================================

pub const O_RDONLY: u32 = 0;
pub const O_WRONLY: u32 = 1;
pub const O_RDWR: u32 = 2;
pub const O_CREAT: u32 = 0o100;
pub const O_EXCL: u32 = 0o200;
pub const O_TRUNC: u32 = 0o1000;
pub const O_APPEND: u32 = 0o2000;
pub const O_DIRECTORY: u32 = 0o200000;

// =============================================================================
// SEEK CONSTANTS
// =============================================================================

pub const SEEK_SET: i32 = 0;
pub const SEEK_CUR: i32 = 1;
pub const SEEK_END: i32 = 2;

// =============================================================================
// FILE MODE CONSTANTS
// =============================================================================

pub const S_IFMT: u32 = 0o170000;
pub const S_IFSOCK: u32 = 0o140000;
pub const S_IFLNK: u32 = 0o120000;
pub const S_IFREG: u32 = 0o100000;
pub const S_IFBLK: u32 = 0o060000;
pub const S_IFDIR: u32 = 0o040000;
pub const S_IFCHR: u32 = 0o020000;
pub const S_IFIFO: u32 = 0o010000;

pub const S_ISUID: u32 = 0o4000;
pub const S_ISGID: u32 = 0o2000;
pub const S_ISVTX: u32 = 0o1000;

pub const S_IRWXU: u32 = 0o700;
pub const S_IRUSR: u32 = 0o400;
pub const S_IWUSR: u32 = 0o200;
pub const S_IXUSR: u32 = 0o100;

pub const S_IRWXG: u32 = 0o070;
pub const S_IRGRP: u32 = 0o040;
pub const S_IWGRP: u32 = 0o020;
pub const S_IXGRP: u32 = 0o010;

pub const S_IRWXO: u32 = 0o007;
pub const S_IROTH: u32 = 0o004;
pub const S_IWOTH: u32 = 0o002;
pub const S_IXOTH: u32 = 0o001;

// =============================================================================
// CLOCK IDS
// =============================================================================

pub const CLOCK_REALTIME: u32 = 0;
pub const CLOCK_MONOTONIC: u32 = 1;

// =============================================================================
// SIGNAL NUMBERS
// =============================================================================

pub const SIGTERM: i32 = 15;
pub const SIGKILL: i32 = 9;
pub const SIGINT: i32 = 2;
pub const SIGHUP: i32 = 1;
pub const SIGSTOP: i32 = 19;
pub const SIGCONT: i32 = 18;

// =============================================================================
// STAT STRUCTURE
// =============================================================================

#[repr(C)]
pub struct Stat {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_nlink: u64,
    pub st_mode: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub _pad0: u32,
    pub st_rdev: u64,
    pub st_size: i64,
    pub st_blksize: i64,
    pub st_blocks: i64,
    pub st_atime: i64,
    pub st_atime_nsec: i64,
    pub st_mtime: i64,
    pub st_mtime_nsec: i64,
    pub st_ctime: i64,
    pub st_ctime_nsec: i64,
    pub _unused: [i64; 3],
}

impl Stat {
    pub const fn zeroed() -> Self {
        Self {
            st_dev: 0,
            st_ino: 0,
            st_nlink: 0,
            st_mode: 0,
            st_uid: 0,
            st_gid: 0,
            _pad0: 0,
            st_rdev: 0,
            st_size: 0,
            st_blksize: 0,
            st_blocks: 0,
            st_atime: 0,
            st_atime_nsec: 0,
            st_mtime: 0,
            st_mtime_nsec: 0,
            st_ctime: 0,
            st_ctime_nsec: 0,
            _unused: [0; 3],
        }
    }
}

// =============================================================================
// DIRENT STRUCTURE
// =============================================================================

#[repr(C)]
pub struct Dirent64 {
    pub d_ino: u64,
    pub d_off: i64,
    pub d_reclen: u16,
    pub d_type: u8,
    // d_name follows (variable length)
}

pub const DT_UNKNOWN: u8 = 0;
pub const DT_FIFO: u8 = 1;
pub const DT_CHR: u8 = 2;
pub const DT_DIR: u8 = 4;
pub const DT_BLK: u8 = 6;
pub const DT_REG: u8 = 8;
pub const DT_LNK: u8 = 10;
pub const DT_SOCK: u8 = 12;

// =============================================================================
// TIMESPEC STRUCTURE
// =============================================================================

#[repr(C)]
pub struct Timespec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

impl Timespec {
    pub const fn zeroed() -> Self {
        Self { tv_sec: 0, tv_nsec: 0 }
    }
}

// =============================================================================
// UTSNAME STRUCTURE
// =============================================================================

#[repr(C)]
pub struct Utsname {
    pub sysname: [u8; 65],
    pub nodename: [u8; 65],
    pub release: [u8; 65],
    pub version: [u8; 65],
    pub machine: [u8; 65],
    pub domainname: [u8; 65],
}

impl Utsname {
    pub const fn zeroed() -> Self {
        Self {
            sysname: [0; 65],
            nodename: [0; 65],
            release: [0; 65],
            version: [0; 65],
            machine: [0; 65],
            domainname: [0; 65],
        }
    }
}

// =============================================================================
// RAW SYSCALL INTERFACE
// =============================================================================

#[inline(always)]
pub unsafe fn syscall(
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

#[inline(always)]
pub unsafe fn syscall0(num: u64) -> i64 {
    syscall(num, 0, 0, 0, 0, 0, 0)
}

#[inline(always)]
pub unsafe fn syscall1(num: u64, arg1: u64) -> i64 {
    syscall(num, arg1, 0, 0, 0, 0, 0)
}

#[inline(always)]
pub unsafe fn syscall2(num: u64, arg1: u64, arg2: u64) -> i64 {
    syscall(num, arg1, arg2, 0, 0, 0, 0)
}

#[inline(always)]
pub unsafe fn syscall3(num: u64, arg1: u64, arg2: u64, arg3: u64) -> i64 {
    syscall(num, arg1, arg2, arg3, 0, 0, 0)
}

#[inline(always)]
pub unsafe fn syscall4(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64) -> i64 {
    syscall(num, arg1, arg2, arg3, arg4, 0, 0)
}

#[inline(always)]
pub unsafe fn syscall6_full(
    num: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> i64 {
    syscall(num, arg1, arg2, arg3, arg4, arg5, arg6)
}

// =============================================================================
// SYSCALL WRAPPERS
// =============================================================================

pub fn read(fd: i32, buf: &mut [u8]) -> isize {
    unsafe { syscall3(SYS_READ, fd as u64, buf.as_mut_ptr() as u64, buf.len() as u64) as isize }
}

pub fn write(fd: i32, buf: &[u8]) -> isize {
    unsafe { syscall3(SYS_WRITE, fd as u64, buf.as_ptr() as u64, buf.len() as u64) as isize }
}

pub fn open(path: &[u8], flags: u32, mode: u32) -> i32 {
    unsafe { syscall3(SYS_OPEN, path.as_ptr() as u64, flags as u64, mode as u64) as i32 }
}

pub fn close(fd: i32) -> i32 {
    unsafe { syscall1(SYS_CLOSE, fd as u64) as i32 }
}

pub fn stat(path: &[u8], buf: &mut Stat) -> i32 {
    unsafe { syscall2(SYS_STAT, path.as_ptr() as u64, buf as *mut Stat as u64) as i32 }
}

pub fn fstat(fd: i32, buf: &mut Stat) -> i32 {
    unsafe { syscall2(SYS_FSTAT, fd as u64, buf as *mut Stat as u64) as i32 }
}

pub fn lstat(path: &[u8], buf: &mut Stat) -> i32 {
    unsafe { syscall2(SYS_LSTAT, path.as_ptr() as u64, buf as *mut Stat as u64) as i32 }
}

pub fn lseek(fd: i32, offset: i64, whence: i32) -> i64 {
    unsafe { syscall3(SYS_LSEEK, fd as u64, offset as u64, whence as u64) as i64 }
}

pub fn dup(fd: i32) -> i32 {
    unsafe { syscall1(SYS_DUP, fd as u64) as i32 }
}

pub fn dup2(oldfd: i32, newfd: i32) -> i32 {
    unsafe { syscall2(SYS_DUP2, oldfd as u64, newfd as u64) as i32 }
}

pub fn getcwd(buf: &mut [u8]) -> isize {
    unsafe { syscall2(SYS_GETCWD, buf.as_mut_ptr() as u64, buf.len() as u64) as isize }
}

pub fn chdir(path: &[u8]) -> i32 {
    unsafe { syscall1(SYS_CHDIR, path.as_ptr() as u64) as i32 }
}

pub fn mkdir(path: &[u8], mode: u32) -> i32 {
    unsafe { syscall2(SYS_MKDIR, path.as_ptr() as u64, mode as u64) as i32 }
}

pub fn rmdir(path: &[u8]) -> i32 {
    unsafe { syscall1(SYS_RMDIR, path.as_ptr() as u64) as i32 }
}

pub fn unlink(path: &[u8]) -> i32 {
    unsafe { syscall1(SYS_UNLINK, path.as_ptr() as u64) as i32 }
}

pub fn link(oldpath: &[u8], newpath: &[u8]) -> i32 {
    unsafe { syscall2(SYS_LINK, oldpath.as_ptr() as u64, newpath.as_ptr() as u64) as i32 }
}

pub fn symlink(target: &[u8], linkpath: &[u8]) -> i32 {
    unsafe { syscall2(SYS_SYMLINK, target.as_ptr() as u64, linkpath.as_ptr() as u64) as i32 }
}

pub fn readlink(path: &[u8], buf: &mut [u8]) -> isize {
    unsafe {
        syscall3(SYS_READLINK, path.as_ptr() as u64, buf.as_mut_ptr() as u64, buf.len() as u64)
            as isize
    }
}

pub fn rename(oldpath: &[u8], newpath: &[u8]) -> i32 {
    unsafe { syscall2(SYS_RENAME, oldpath.as_ptr() as u64, newpath.as_ptr() as u64) as i32 }
}

pub fn chmod(path: &[u8], mode: u32) -> i32 {
    unsafe { syscall2(SYS_CHMOD, path.as_ptr() as u64, mode as u64) as i32 }
}

pub fn fchmod(fd: i32, mode: u32) -> i32 {
    unsafe { syscall2(SYS_FCHMOD, fd as u64, mode as u64) as i32 }
}

pub fn chown(path: &[u8], uid: u32, gid: u32) -> i32 {
    unsafe { syscall3(SYS_CHOWN, path.as_ptr() as u64, uid as u64, gid as u64) as i32 }
}

pub fn fchown(fd: i32, uid: u32, gid: u32) -> i32 {
    unsafe { syscall3(SYS_FCHOWN, fd as u64, uid as u64, gid as u64) as i32 }
}

pub fn truncate(path: &[u8], length: i64) -> i32 {
    unsafe { syscall2(SYS_TRUNCATE, path.as_ptr() as u64, length as u64) as i32 }
}

pub fn ftruncate(fd: i32, length: i64) -> i32 {
    unsafe { syscall2(SYS_FTRUNCATE, fd as u64, length as u64) as i32 }
}

pub fn getdents64(fd: i32, buf: &mut [u8]) -> isize {
    unsafe {
        syscall3(SYS_GETDENTS64, fd as u64, buf.as_mut_ptr() as u64, buf.len() as u64) as isize
    }
}

pub fn getpid() -> i32 {
    unsafe { syscall0(SYS_GETPID) as i32 }
}

pub fn getppid() -> i32 {
    unsafe { syscall0(SYS_GETPPID) as i32 }
}

pub fn getuid() -> u32 {
    unsafe { syscall0(SYS_GETUID) as u32 }
}

pub fn getgid() -> u32 {
    unsafe { syscall0(SYS_GETGID) as u32 }
}

pub fn geteuid() -> u32 {
    unsafe { syscall0(SYS_GETEUID) as u32 }
}

pub fn getegid() -> u32 {
    unsafe { syscall0(SYS_GETEGID) as u32 }
}

pub fn kill(pid: i32, sig: i32) -> i32 {
    unsafe { syscall2(SYS_KILL, pid as u64, sig as u64) as i32 }
}

pub fn uname(buf: &mut Utsname) -> i32 {
    unsafe { syscall1(SYS_UNAME, buf as *mut Utsname as u64) as i32 }
}

pub fn nanosleep(req: &Timespec, rem: Option<&mut Timespec>) -> i32 {
    let rem_ptr = match rem {
        Some(r) => r as *mut Timespec as u64,
        None => 0,
    };
    unsafe { syscall2(SYS_NANOSLEEP, req as *const Timespec as u64, rem_ptr) as i32 }
}

pub fn clock_gettime(clock_id: u32, tp: &mut Timespec) -> i32 {
    unsafe { syscall2(SYS_CLOCK_GETTIME, clock_id as u64, tp as *mut Timespec as u64) as i32 }
}

pub fn utimensat(dirfd: i32, path: &[u8], times: &[Timespec; 2], flags: i32) -> i32 {
    unsafe {
        syscall4(
            SYS_UTIMENSAT,
            dirfd as u64,
            path.as_ptr() as u64,
            times.as_ptr() as u64,
            flags as u64,
        ) as i32
    }
}

pub fn exit(code: i32) -> ! {
    unsafe { syscall1(SYS_EXIT, code as u64) };
    loop {}
}

// =============================================================================
// FILE TYPE CHECKS
// =============================================================================

pub fn s_isreg(mode: u32) -> bool {
    (mode & S_IFMT) == S_IFREG
}

pub fn s_isdir(mode: u32) -> bool {
    (mode & S_IFMT) == S_IFDIR
}

pub fn s_islnk(mode: u32) -> bool {
    (mode & S_IFMT) == S_IFLNK
}

pub fn s_ischr(mode: u32) -> bool {
    (mode & S_IFMT) == S_IFCHR
}

pub fn s_isblk(mode: u32) -> bool {
    (mode & S_IFMT) == S_IFBLK
}

pub fn s_isfifo(mode: u32) -> bool {
    (mode & S_IFMT) == S_IFIFO
}

pub fn s_issock(mode: u32) -> bool {
    (mode & S_IFMT) == S_IFSOCK
}

// =============================================================================
// OUTPUT UTILITIES
// =============================================================================

pub const STDOUT: i32 = 1;
pub const STDERR: i32 = 2;
pub const STDIN: i32 = 0;

pub fn print(s: &str) {
    write(STDOUT, s.as_bytes());
}

pub fn println(s: &str) {
    print(s);
    print("\n");
}

pub fn eprint(s: &str) {
    write(STDERR, s.as_bytes());
}

pub fn eprintln(s: &str) {
    eprint(s);
    eprint("\n");
}

pub fn print_bytes(buf: &[u8]) {
    write(STDOUT, buf);
}

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

pub fn print_num_signed(n: i64) {
    if n < 0 {
        print("-");
        print_num((-n) as u64);
    } else {
        print_num(n as u64);
    }
}

pub fn print_hex(mut n: u64) {
    if n == 0 {
        print("0");
        return;
    }

    let mut buf = [0u8; 16];
    let mut i = 16;
    let hex_chars = b"0123456789abcdef";

    while n > 0 {
        i -= 1;
        buf[i] = hex_chars[(n & 0xf) as usize];
        n >>= 4;
    }

    write(STDOUT, &buf[i..]);
}

pub fn print_octal(mut n: u64) {
    if n == 0 {
        print("0");
        return;
    }

    let mut buf = [0u8; 22];
    let mut i = 22;

    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n & 7) as u8;
        n >>= 3;
    }

    write(STDOUT, &buf[i..]);
}

// =============================================================================
// STRING UTILITIES
// =============================================================================

pub fn str_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for i in 0..a.len() {
        if a[i] != b[i] {
            return false;
        }
    }
    true
}

pub fn str_starts_with(s: &[u8], prefix: &[u8]) -> bool {
    if s.len() < prefix.len() {
        return false;
    }
    for i in 0..prefix.len() {
        if s[i] != prefix[i] {
            return false;
        }
    }
    true
}

pub fn cstr_len(s: *const u8) -> usize {
    let mut len = 0;
    unsafe {
        while *s.add(len) != 0 {
            len += 1;
        }
    }
    len
}

pub fn cstr_to_slice(s: *const u8) -> &'static [u8] {
    let len = cstr_len(s);
    unsafe { core::slice::from_raw_parts(s, len) }
}

pub fn trim(s: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = s.len();

    while start < end && (s[start] == b' ' || s[start] == b'\t' || s[start] == b'\n') {
        start += 1;
    }

    while end > start && (s[end - 1] == b' ' || s[end - 1] == b'\t' || s[end - 1] == b'\n') {
        end -= 1;
    }

    &s[start..end]
}

/// Parse a decimal number from bytes
pub fn parse_num(s: &[u8]) -> Option<u64> {
    if s.is_empty() {
        return None;
    }

    let mut result: u64 = 0;
    for &c in s {
        if c < b'0' || c > b'9' {
            return None;
        }
        result = result.checked_mul(10)?.checked_add((c - b'0') as u64)?;
    }
    Some(result)
}

/// Parse an octal number from bytes
pub fn parse_octal(s: &[u8]) -> Option<u32> {
    if s.is_empty() {
        return None;
    }

    let mut result: u32 = 0;
    for &c in s {
        if c < b'0' || c > b'7' {
            return None;
        }
        result = result.checked_mul(8)?.checked_add((c - b'0') as u32)?;
    }
    Some(result)
}

// =============================================================================
// PATH UTILITIES
// =============================================================================

/// Extract the base name (filename) from a path
pub fn basename(path: &[u8]) -> &[u8] {
    if path.is_empty() {
        return b".";
    }

    // Remove trailing slashes
    let mut end = path.len();
    while end > 1 && path[end - 1] == b'/' {
        end -= 1;
    }

    if end == 1 && path[0] == b'/' {
        return b"/";
    }

    // Find the last slash
    let mut last_slash = None;
    for i in 0..end {
        if path[i] == b'/' {
            last_slash = Some(i);
        }
    }

    match last_slash {
        Some(pos) => &path[pos + 1..end],
        None => &path[..end],
    }
}

/// Extract the directory name from a path
pub fn dirname(path: &[u8]) -> &[u8] {
    if path.is_empty() {
        return b".";
    }

    // Remove trailing slashes
    let mut end = path.len();
    while end > 1 && path[end - 1] == b'/' {
        end -= 1;
    }

    // Find the last slash
    let mut last_slash = None;
    for i in 0..end {
        if path[i] == b'/' {
            last_slash = Some(i);
        }
    }

    match last_slash {
        Some(0) => b"/",
        Some(pos) => &path[..pos],
        None => b".",
    }
}

/// Build a null-terminated path in a buffer
pub fn make_cpath(buf: &mut [u8], path: &[u8]) -> Option<usize> {
    if path.len() >= buf.len() {
        return None;
    }
    buf[..path.len()].copy_from_slice(path);
    buf[path.len()] = 0;
    Some(path.len() + 1)
}

// =============================================================================
// ARGUMENT PARSING
// =============================================================================

/// Simple argument iterator for parsing command line
pub struct ArgIter {
    args: *const *const u8,
    count: usize,
    pos: usize,
}

impl ArgIter {
    pub unsafe fn new(argc: usize, argv: *const *const u8) -> Self {
        Self {
            args: argv,
            count: argc,
            pos: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

impl Iterator for ArgIter {
    type Item = &'static [u8];

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.count {
            return None;
        }
        unsafe {
            let arg = *self.args.add(self.pos);
            self.pos += 1;
            if arg.is_null() {
                None
            } else {
                Some(cstr_to_slice(arg))
            }
        }
    }
}

// =============================================================================
// PANIC HANDLER
// =============================================================================

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    eprintln("panic!");
    exit(1)
}

// =============================================================================
// FORMAT MODE STRING
// =============================================================================

pub fn format_mode(mode: u32, buf: &mut [u8; 10]) {
    // File type
    buf[0] = match mode & S_IFMT {
        S_IFDIR => b'd',
        S_IFLNK => b'l',
        S_IFCHR => b'c',
        S_IFBLK => b'b',
        S_IFIFO => b'p',
        S_IFSOCK => b's',
        _ => b'-',
    };

    // Owner permissions
    buf[1] = if mode & S_IRUSR != 0 { b'r' } else { b'-' };
    buf[2] = if mode & S_IWUSR != 0 { b'w' } else { b'-' };
    buf[3] = if mode & S_IXUSR != 0 {
        if mode & S_ISUID != 0 { b's' } else { b'x' }
    } else {
        if mode & S_ISUID != 0 { b'S' } else { b'-' }
    };

    // Group permissions
    buf[4] = if mode & S_IRGRP != 0 { b'r' } else { b'-' };
    buf[5] = if mode & S_IWGRP != 0 { b'w' } else { b'-' };
    buf[6] = if mode & S_IXGRP != 0 {
        if mode & S_ISGID != 0 { b's' } else { b'x' }
    } else {
        if mode & S_ISGID != 0 { b'S' } else { b'-' }
    };

    // Other permissions
    buf[7] = if mode & S_IROTH != 0 { b'r' } else { b'-' };
    buf[8] = if mode & S_IWOTH != 0 { b'w' } else { b'-' };
    buf[9] = if mode & S_IXOTH != 0 {
        if mode & S_ISVTX != 0 { b't' } else { b'x' }
    } else {
        if mode & S_ISVTX != 0 { b'T' } else { b'-' }
    };
}

// =============================================================================
// ENTRY POINT MACRO
// =============================================================================

#[macro_export]
macro_rules! entry {
    ($main:ident) => {
        #[no_mangle]
        pub extern "C" fn _start(argc: usize, argv: *const *const u8) -> ! {
            let code = $main(argc, argv);
            $crate::exit(code);
        }
    };
}
