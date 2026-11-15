// ===============================================================================
// BUILDOS KERNEL - SYSTEM CALLS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! System call interface including AI-specific syscalls (500 series).

use alloc::string::String;
use alloc::vec::Vec;
use crate::process::{Pid, Tid};

// =============================================================================
// SYSTEM CALL NUMBERS
// =============================================================================

// Standard POSIX-like syscalls
pub const SYS_READ: u64 = 0;
pub const SYS_WRITE: u64 = 1;
pub const SYS_OPEN: u64 = 2;
pub const SYS_CLOSE: u64 = 3;
pub const SYS_STAT: u64 = 4;
pub const SYS_FSTAT: u64 = 5;
pub const SYS_LSTAT: u64 = 6;
pub const SYS_POLL: u64 = 7;
pub const SYS_LSEEK: u64 = 8;
pub const SYS_MMAP: u64 = 9;
pub const SYS_MPROTECT: u64 = 10;
pub const SYS_MUNMAP: u64 = 11;
pub const SYS_BRK: u64 = 12;
pub const SYS_IOCTL: u64 = 16;
pub const SYS_PIPE: u64 = 22;
pub const SYS_SELECT: u64 = 23;
pub const SYS_SCHED_YIELD: u64 = 24;
pub const SYS_DUP: u64 = 32;
pub const SYS_DUP2: u64 = 33;
pub const SYS_NANOSLEEP: u64 = 35;
pub const SYS_GETITIMER: u64 = 36;
pub const SYS_ALARM: u64 = 37;
pub const SYS_SETITIMER: u64 = 38;
pub const SYS_GETPID: u64 = 39;
pub const SYS_CLONE: u64 = 56;
pub const SYS_FORK: u64 = 57;
pub const SYS_VFORK: u64 = 58;
pub const SYS_EXECVE: u64 = 59;
pub const SYS_EXIT: u64 = 60;
pub const SYS_WAIT4: u64 = 61;
pub const SYS_KILL: u64 = 62;
pub const SYS_UNAME: u64 = 63;
pub const SYS_GETCWD: u64 = 79;
pub const SYS_CHDIR: u64 = 80;
pub const SYS_MKDIR: u64 = 83;
pub const SYS_RMDIR: u64 = 84;
pub const SYS_UNLINK: u64 = 87;
pub const SYS_READLINK: u64 = 89;
pub const SYS_GETTIMEOFDAY: u64 = 96;
pub const SYS_GETUID: u64 = 102;
pub const SYS_GETGID: u64 = 104;
pub const SYS_GETEUID: u64 = 107;
pub const SYS_GETEGID: u64 = 108;
pub const SYS_GETPPID: u64 = 110;
pub const SYS_ARCH_PRCTL: u64 = 158;
pub const SYS_CLOCK_GETTIME: u64 = 228;
pub const SYS_CLOCK_GETRES: u64 = 229;
pub const SYS_CLOCK_NANOSLEEP: u64 = 230;
pub const SYS_EXIT_GROUP: u64 = 231;
pub const SYS_TIMER_CREATE: u64 = 222;
pub const SYS_TIMER_SETTIME: u64 = 223;
pub const SYS_TIMER_GETTIME: u64 = 224;
pub const SYS_TIMER_GETOVERRUN: u64 = 225;
pub const SYS_TIMER_DELETE: u64 = 226;

// Socket syscalls
pub const SYS_SOCKET: u64 = 41;
pub const SYS_CONNECT: u64 = 42;
pub const SYS_ACCEPT: u64 = 43;
pub const SYS_SENDTO: u64 = 44;
pub const SYS_RECVFROM: u64 = 45;
pub const SYS_SENDMSG: u64 = 46;
pub const SYS_RECVMSG: u64 = 47;
pub const SYS_SHUTDOWN: u64 = 48;
pub const SYS_BIND: u64 = 49;
pub const SYS_LISTEN: u64 = 50;
pub const SYS_GETSOCKNAME: u64 = 51;
pub const SYS_GETPEERNAME: u64 = 52;
pub const SYS_SOCKETPAIR: u64 = 53;
pub const SYS_SETSOCKOPT: u64 = 54;
pub const SYS_GETSOCKOPT: u64 = 55;
pub const SYS_ACCEPT4: u64 = 288;

// IPC syscalls
pub const SYS_PIPE2: u64 = 293;
pub const SYS_MSGGET: u64 = 68;
pub const SYS_MSGSND: u64 = 69;
pub const SYS_MSGRCV: u64 = 70;
pub const SYS_MSGCTL: u64 = 71;
pub const SYS_SHMGET: u64 = 29;
pub const SYS_SHMAT: u64 = 30;
pub const SYS_SHMDT: u64 = 67;
pub const SYS_SHMCTL: u64 = 31;
pub const SYS_SEMGET: u64 = 64;
pub const SYS_SEMOP: u64 = 65;
pub const SYS_SEMCTL: u64 = 66;
pub const SYS_RT_SIGACTION: u64 = 13;
pub const SYS_RT_SIGPROCMASK: u64 = 14;
pub const SYS_RT_SIGRETURN: u64 = 15;
pub const SYS_FUTEX: u64 = 202;
pub const SYS_EVENTFD2: u64 = 290;
pub const SYS_MQ_OPEN: u64 = 240;
pub const SYS_MQ_UNLINK: u64 = 241;
pub const SYS_MQ_TIMEDSEND: u64 = 242;
pub const SYS_MQ_TIMEDRECEIVE: u64 = 243;

// AI-specific syscalls (500 series) - BuildOS exclusive
pub const SYS_AI_QUERY: u64 = 500;
pub const SYS_AI_INFER: u64 = 501;
pub const SYS_AI_TRAIN_STEP: u64 = 502;
pub const SYS_AI_LOAD_MODEL: u64 = 503;
pub const SYS_AI_UNLOAD_MODEL: u64 = 504;
pub const SYS_AI_TENSOR_ALLOC: u64 = 505;
pub const SYS_AI_TENSOR_FREE: u64 = 506;
pub const SYS_AI_TENSOR_SHARE: u64 = 507;
pub const SYS_AI_PRIORITY_BOOST: u64 = 508;
pub const SYS_AI_REGISTER_CALLBACK: u64 = 509;
pub const SYS_CHECKPOINT: u64 = 510;
pub const SYS_RESTORE: u64 = 511;
pub const SYS_HEAL: u64 = 512;

// =============================================================================
// SYSTEM CALL HANDLER
// =============================================================================

/// System call entry point
///
/// Called from assembly syscall handler with:
/// - rax: syscall number
/// - rdi, rsi, rdx, r10, r8, r9: arguments
///
/// Returns result in rax.
#[no_mangle]
pub extern "sysv64" fn syscall_handler(
    syscall: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> i64 {
    let result = match syscall {
        // File operations
        SYS_READ => sys_read(arg1 as i32, arg2 as *mut u8, arg3 as usize),
        SYS_WRITE => sys_write(arg1 as i32, arg2 as *const u8, arg3 as usize),
        SYS_OPEN => sys_open(arg1 as *const u8, arg2 as i32, arg3 as u32),
        SYS_CLOSE => sys_close(arg1 as i32),
        SYS_STAT => sys_stat(arg1 as *const u8, arg2 as *mut u8),
        SYS_FSTAT => sys_fstat(arg1 as i32, arg2 as *mut u8),
        SYS_LSEEK => sys_lseek(arg1 as i32, arg2 as i64, arg3 as i32),
        SYS_DUP => sys_dup(arg1 as i32),
        SYS_DUP2 => sys_dup2(arg1 as i32, arg2 as i32),
        SYS_GETCWD => sys_getcwd(arg1 as *mut u8, arg2 as usize),
        SYS_CHDIR => sys_chdir(arg1 as *const u8),
        SYS_MKDIR => sys_mkdir(arg1 as *const u8, arg2 as u32),
        SYS_RMDIR => sys_rmdir(arg1 as *const u8),
        SYS_UNLINK => sys_unlink(arg1 as *const u8),
        SYS_READLINK => sys_readlink(arg1 as *const u8, arg2 as *mut u8, arg3 as usize),

        // I/O multiplexing
        SYS_POLL => sys_poll(arg1 as *mut PollFd, arg2 as usize, arg3 as i32),
        SYS_SELECT => sys_select(arg1 as i32, arg2 as *mut FdSet, arg3 as *mut FdSet, arg4 as *mut FdSet, arg5 as *mut Timeval),

        // Process operations
        SYS_GETPID => sys_getpid(),
        SYS_GETPPID => sys_getppid(),
        SYS_FORK => sys_fork(),
        SYS_VFORK => sys_fork(), // vfork is same as fork with COW
        SYS_EXECVE => sys_execve(arg1 as *const u8, arg2 as *const *const u8, arg3 as *const *const u8),
        SYS_EXIT => sys_exit(arg1 as i32),
        SYS_EXIT_GROUP => sys_exit(arg1 as i32), // exit_group terminates all threads
        SYS_WAIT4 => sys_wait4(arg1 as i32, arg2 as *mut i32, arg3 as i32, arg4 as *mut u8),
        SYS_CLONE => sys_clone(arg1, arg2, arg3 as *mut i32, arg4 as *mut i32, arg5),
        SYS_SCHED_YIELD => sys_sched_yield(),

        // Time operations
        SYS_NANOSLEEP => sys_nanosleep(arg1 as *const u8, arg2 as *mut u8),
        SYS_CLOCK_GETTIME => sys_clock_gettime(arg1 as i32, arg2 as *mut u8),
        SYS_CLOCK_GETRES => sys_clock_getres(arg1 as i32, arg2 as *mut u8),
        SYS_GETTIMEOFDAY => sys_gettimeofday(arg1 as *mut u8, arg2 as *mut u8),

        // Memory operations
        SYS_MMAP => sys_mmap(arg1, arg2, arg3 as i32, arg4 as i32, arg5 as i32, arg6),
        SYS_MPROTECT => sys_mprotect(arg1, arg2, arg3 as i32),
        SYS_MUNMAP => sys_munmap(arg1, arg2),
        SYS_BRK => sys_brk(arg1),

        // AI operations
        SYS_AI_QUERY => sys_ai_query(arg1 as *const u8, arg2, arg3 as *mut u8, arg4),
        SYS_AI_INFER => sys_ai_infer(arg1, arg2 as *const u8, arg3, arg4 as *mut u8, arg5),
        SYS_AI_TENSOR_ALLOC => sys_ai_tensor_alloc(arg1, arg2 as *const u64, arg3),
        SYS_AI_TENSOR_FREE => sys_ai_tensor_free(arg1),
        SYS_AI_TENSOR_SHARE => sys_ai_tensor_share(arg1, Pid::new(arg2)),
        SYS_AI_PRIORITY_BOOST => sys_ai_priority_boost(Tid::new(arg1), f32::from_bits(arg2 as u32)),

        // Self-healing operations
        SYS_CHECKPOINT => sys_checkpoint(),
        SYS_RESTORE => sys_restore(arg1),
        SYS_HEAL => sys_heal(arg1 as i32),

        // Socket operations
        SYS_SOCKET => sys_socket(arg1 as u16, arg2 as u32, arg3 as u32),
        SYS_BIND => sys_bind(arg1 as i32, arg2 as *const u8, arg3 as u32),
        SYS_LISTEN => sys_listen(arg1 as i32, arg2 as i32),
        SYS_ACCEPT | SYS_ACCEPT4 => sys_accept(arg1 as i32, arg2 as *mut u8, arg3 as *mut u32),
        SYS_CONNECT => sys_connect(arg1 as i32, arg2 as *const u8, arg3 as u32),
        SYS_SENDTO => sys_sendto(arg1 as i32, arg2 as *const u8, arg3 as usize, arg4 as i32, arg5 as *const u8, arg6 as u32),
        SYS_RECVFROM => sys_recvfrom(arg1 as i32, arg2 as *mut u8, arg3 as usize, arg4 as i32, arg5 as *mut u8, arg6 as *mut u32),
        SYS_SHUTDOWN => sys_shutdown(arg1 as i32, arg2 as i32),
        SYS_SETSOCKOPT => sys_setsockopt(arg1 as i32, arg2 as i32, arg3 as i32, arg4 as *const u8, arg5 as u32),
        SYS_GETSOCKOPT => sys_getsockopt(arg1 as i32, arg2 as i32, arg3 as i32, arg4 as *mut u8, arg5 as *mut u32),
        SYS_GETSOCKNAME => sys_getsockname(arg1 as i32, arg2 as *mut u8, arg3 as *mut u32),
        SYS_GETPEERNAME => sys_getpeername(arg1 as i32, arg2 as *mut u8, arg3 as *mut u32),

        // IPC operations
        SYS_PIPE | SYS_PIPE2 => sys_pipe(arg1 as *mut i32, arg2 as i32),
        SYS_MSGGET => sys_msgget(arg1 as i32, arg2 as i32),
        SYS_MSGSND => sys_msgsnd(arg1 as i32, arg2 as *const u8, arg3 as usize, arg4 as i32),
        SYS_MSGRCV => sys_msgrcv(arg1 as i32, arg2 as *mut u8, arg3 as usize, arg4 as i64, arg5 as i32),
        SYS_SHMGET => sys_shmget(arg1 as i32, arg2 as usize, arg3 as i32),
        SYS_SHMAT => sys_shmat(arg1 as i32, arg2, arg3 as i32),
        SYS_SHMDT => sys_shmdt(arg1),
        SYS_SEMGET => sys_semget(arg1 as i32, arg2 as i32, arg3 as i32),
        SYS_SEMOP => sys_semop(arg1 as i32, arg2 as *const u8, arg3 as usize),
        SYS_FUTEX => sys_futex(arg1, arg2 as i32, arg3 as u32, arg4, arg5, arg6 as u32),
        SYS_EVENTFD2 => sys_eventfd(arg1 as u32, arg2 as i32),
        SYS_RT_SIGACTION => sys_rt_sigaction(arg1 as i32, arg2, arg3),
        SYS_RT_SIGPROCMASK => sys_rt_sigprocmask(arg1 as i32, arg2, arg3),
        SYS_KILL => sys_kill(arg1 as i32, arg2 as i32),
        SYS_MQ_OPEN => sys_mq_open(arg1 as *const u8, arg2 as i32, arg3 as u32, arg4),
        SYS_MQ_TIMEDSEND => sys_mq_send(arg1 as i32, arg2 as *const u8, arg3 as usize, arg4 as u32),
        SYS_MQ_TIMEDRECEIVE => sys_mq_receive(arg1 as i32, arg2 as *mut u8, arg3 as usize, arg4 as *mut u32),

        // Unknown syscall
        _ => -1, // ENOSYS
    };

    // Check for pending signals before returning to user space
    crate::process::check_signals();

    result
}

// =============================================================================
// FILE SYSCALLS
// =============================================================================

fn sys_read(fd: i32, buf: *mut u8, count: usize) -> i64 {
    use crate::fs;

    if buf.is_null() || count == 0 {
        return 0;
    }

    // Handle special file descriptors
    match fd {
        0 => {
            // stdin - read from keyboard
            // For now, return 0 (EOF)
            return 0;
        }
        1 | 2 => {
            // stdout/stderr - can't read
            return -(fs::errno::EBADF as i64);
        }
        _ => {}
    }

    let buffer = unsafe { core::slice::from_raw_parts_mut(buf, count) };

    match fs::read(fd as u64, buffer) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}

fn sys_write(fd: i32, buf: *const u8, count: usize) -> i64 {
    use crate::fs;

    if buf.is_null() || count == 0 {
        return 0;
    }

    // Handle special file descriptors
    match fd {
        0 => {
            // stdin - can't write
            return -(fs::errno::EBADF as i64);
        }
        1 | 2 => {
            // stdout/stderr - write to framebuffer console
            let data = unsafe { core::slice::from_raw_parts(buf, count) };
            for &byte in data {
                if byte.is_ascii() {
                    crate::kprint!("{}", byte as char);
                }
            }
            return count as i64;
        }
        _ => {}
    }

    let buffer = unsafe { core::slice::from_raw_parts(buf, count) };

    match fs::write(fd as u64, buffer) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}

fn sys_open(path: *const u8, flags: i32, mode: u32) -> i64 {
    use crate::fs;

    if path.is_null() {
        return -(fs::errno::EINVAL as i64);
    }

    // Read path from user memory
    let path_str = unsafe {
        let mut len = 0;
        while *path.add(len) != 0 && len < 4096 {
            len += 1;
        }
        let bytes = core::slice::from_raw_parts(path, len);
        core::str::from_utf8(bytes).unwrap_or("")
    };

    if path_str.is_empty() {
        return -(fs::errno::ENOENT as i64);
    }

    match fs::open(path_str, flags as u32, mode) {
        Ok(fd) => fd as i64,
        Err(e) => -(e as i64),
    }
}

fn sys_close(fd: i32) -> i64 {
    use crate::fs;

    // Don't allow closing stdin/stdout/stderr
    if fd < 3 {
        return -(fs::errno::EBADF as i64);
    }

    match fs::close(fd as u64) {
        Ok(()) => 0,
        Err(e) => -(e as i64),
    }
}

fn sys_lseek(fd: i32, offset: i64, whence: i32) -> i64 {
    use crate::fs;

    if fd < 3 {
        return -(fs::errno::ESPIPE as i64); // Pipe not seekable
    }

    match fs::lseek(fd as u64, offset, whence) {
        Ok(pos) => pos as i64,
        Err(e) => -(e as i64),
    }
}

fn sys_stat(path: *const u8, statbuf: *mut u8) -> i64 {
    use crate::fs;

    if path.is_null() || statbuf.is_null() {
        return -(fs::errno::EINVAL as i64);
    }

    let path_str = unsafe {
        let mut len = 0;
        while *path.add(len) != 0 && len < 4096 {
            len += 1;
        }
        let bytes = core::slice::from_raw_parts(path, len);
        core::str::from_utf8(bytes).unwrap_or("")
    };

    match fs::stat(path_str) {
        Ok(metadata) => {
            // Write stat structure to user buffer
            unsafe {
                write_stat(statbuf, &metadata);
            }
            0
        }
        Err(e) => -(e as i64),
    }
}

fn sys_fstat(fd: i32, statbuf: *mut u8) -> i64 {
    use crate::fs;

    if statbuf.is_null() {
        return -(fs::errno::EINVAL as i64);
    }

    match fs::fstat(fd as u64) {
        Ok(metadata) => {
            unsafe {
                write_stat(statbuf, &metadata);
            }
            0
        }
        Err(e) => -(e as i64),
    }
}

fn sys_dup(oldfd: i32) -> i64 {
    use crate::fs;

    // Get handle for old fd and create new fd pointing to same file
    // For now, return not implemented
    let _ = oldfd;
    -(fs::errno::ENOSYS as i64)
}

fn sys_dup2(oldfd: i32, newfd: i32) -> i64 {
    use crate::fs;

    // Duplicate oldfd to specific newfd
    let _ = (oldfd, newfd);
    -(fs::errno::ENOSYS as i64)
}

fn sys_getcwd(buf: *mut u8, size: usize) -> i64 {
    use crate::fs;

    if buf.is_null() || size == 0 {
        return -(fs::errno::EINVAL as i64);
    }

    let cwd = fs::getcwd();
    let bytes = cwd.as_bytes();

    if bytes.len() + 1 > size {
        return -(fs::errno::ERANGE as i64);
    }

    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, bytes.len());
        *buf.add(bytes.len()) = 0; // null terminate
    }

    buf as i64
}

fn sys_chdir(path: *const u8) -> i64 {
    use crate::fs;

    if path.is_null() {
        return -(fs::errno::EINVAL as i64);
    }

    let path_str = unsafe {
        let mut len = 0;
        while *path.add(len) != 0 && len < 4096 {
            len += 1;
        }
        let bytes = core::slice::from_raw_parts(path, len);
        core::str::from_utf8(bytes).unwrap_or("")
    };

    // Verify path exists and is a directory
    if !fs::is_dir(path_str) {
        if fs::exists(path_str) {
            return -(fs::errno::ENOTDIR as i64);
        } else {
            return -(fs::errno::ENOENT as i64);
        }
    }

    // TODO: Store cwd in process struct
    // For now, always succeed
    0
}

fn sys_mkdir(path: *const u8, mode: u32) -> i64 {
    use crate::fs;

    if path.is_null() {
        return -(fs::errno::EINVAL as i64);
    }

    let path_str = unsafe {
        let mut len = 0;
        while *path.add(len) != 0 && len < 4096 {
            len += 1;
        }
        let bytes = core::slice::from_raw_parts(path, len);
        core::str::from_utf8(bytes).unwrap_or("")
    };

    match fs::mkdir(path_str, mode) {
        Ok(()) => 0,
        Err(e) => -(e as i64),
    }
}

fn sys_rmdir(path: *const u8) -> i64 {
    use crate::fs;

    if path.is_null() {
        return -(fs::errno::EINVAL as i64);
    }

    let path_str = unsafe {
        let mut len = 0;
        while *path.add(len) != 0 && len < 4096 {
            len += 1;
        }
        let bytes = core::slice::from_raw_parts(path, len);
        core::str::from_utf8(bytes).unwrap_or("")
    };

    match fs::rmdir(path_str) {
        Ok(()) => 0,
        Err(e) => -(e as i64),
    }
}

fn sys_unlink(path: *const u8) -> i64 {
    use crate::fs;

    if path.is_null() {
        return -(fs::errno::EINVAL as i64);
    }

    let path_str = unsafe {
        let mut len = 0;
        while *path.add(len) != 0 && len < 4096 {
            len += 1;
        }
        let bytes = core::slice::from_raw_parts(path, len);
        core::str::from_utf8(bytes).unwrap_or("")
    };

    match fs::unlink(path_str) {
        Ok(()) => 0,
        Err(e) => -(e as i64),
    }
}

fn sys_readlink(path: *const u8, buf: *mut u8, bufsiz: usize) -> i64 {
    use crate::fs;

    if path.is_null() || buf.is_null() {
        return -(fs::errno::EINVAL as i64);
    }

    let path_str = unsafe {
        let mut len = 0;
        while *path.add(len) != 0 && len < 4096 {
            len += 1;
        }
        let bytes = core::slice::from_raw_parts(path, len);
        core::str::from_utf8(bytes).unwrap_or("")
    };

    match fs::readlink(path_str) {
        Ok(target) => {
            let bytes = target.as_bytes();
            let copy_len = bytes.len().min(bufsiz);
            unsafe {
                core::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, copy_len);
            }
            copy_len as i64
        }
        Err(e) => -(e as i64),
    }
}

/// Write metadata to stat buffer in Linux format
unsafe fn write_stat(buf: *mut u8, metadata: &crate::fs::Metadata) {
    #[repr(C)]
    struct Stat {
        st_dev: u64,
        st_ino: u64,
        st_nlink: u64,
        st_mode: u32,
        st_uid: u32,
        st_gid: u32,
        __pad0: u32,
        st_rdev: u64,
        st_size: i64,
        st_blksize: i64,
        st_blocks: i64,
        st_atime: i64,
        st_atime_nsec: i64,
        st_mtime: i64,
        st_mtime_nsec: i64,
        st_ctime: i64,
        st_ctime_nsec: i64,
        __unused: [i64; 3],
    }

    let stat = buf as *mut Stat;
    (*stat).st_dev = metadata.dev;
    (*stat).st_ino = metadata.ino;
    (*stat).st_nlink = metadata.nlink as u64;
    (*stat).st_mode = metadata.mode;
    (*stat).st_uid = metadata.uid;
    (*stat).st_gid = metadata.gid;
    (*stat).__pad0 = 0;
    (*stat).st_rdev = metadata.rdev;
    (*stat).st_size = metadata.size as i64;
    (*stat).st_blksize = metadata.blksize as i64;
    (*stat).st_blocks = metadata.blocks as i64;
    (*stat).st_atime = metadata.atime as i64;
    (*stat).st_atime_nsec = 0;
    (*stat).st_mtime = metadata.mtime as i64;
    (*stat).st_mtime_nsec = 0;
    (*stat).st_ctime = metadata.ctime as i64;
    (*stat).st_ctime_nsec = 0;
    (*stat).__unused = [0; 3];
}

// =============================================================================
// PROCESS SYSCALLS
// =============================================================================

fn sys_getpid() -> i64 {
    crate::process::current()
        .map(|p| p.as_u64() as i64)
        .unwrap_or(-1)
}

fn sys_getppid() -> i64 {
    crate::process::getppid()
        .map(|p| p.as_u64() as i64)
        .unwrap_or(0) // Return 0 if no parent (like init)
}

fn sys_fork() -> i64 {
    crate::process::fork()
        .map(|p| p.as_u64() as i64)
        .unwrap_or(-1)
}

fn sys_execve(pathname: *const u8, argv: *const *const u8, envp: *const *const u8) -> i64 {
    use alloc::string::String;
    use alloc::vec::Vec;

    // Read pathname
    let path = match read_user_string(pathname, 4096) {
        Some(s) => s,
        None => return -crate::fs::errno::EFAULT as i64,
    };

    // Read argv array
    let args: Vec<String> = if argv.is_null() {
        Vec::new()
    } else {
        read_string_array(argv, 256, 4096)
    };

    // Read envp array
    let env_pairs: Vec<(String, String)> = if envp.is_null() {
        Vec::new()
    } else {
        let env_strs = read_string_array(envp, 256, 4096);
        env_strs
            .into_iter()
            .filter_map(|s| {
                let mut parts = s.splitn(2, '=');
                let key = parts.next()?;
                let value = parts.next().unwrap_or("");
                Some((String::from(key), String::from(value)))
            })
            .collect()
    };

    // Convert to slices for execve
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let env_refs: Vec<(&str, &str)> = env_pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

    match crate::process::execve(&path, &arg_refs, &env_refs) {
        Ok(()) => 0, // Doesn't return on success
        Err(e) => e as i64,
    }
}

fn sys_exit(code: i32) -> i64 {
    crate::process::exit(code);
    0 // Never returns
}

fn sys_wait4(pid: i32, wstatus: *mut i32, options: i32, rusage: *mut u8) -> i64 {
    // Ignore rusage for now (resource usage statistics)
    let _ = rusage;

    match crate::process::waitpid(pid, options) {
        Some((child_pid, exit_code)) => {
            // Write status if pointer is provided
            if !wstatus.is_null() {
                // Encode exit status in Linux format:
                // - Normal exit: (code << 8) | 0
                // - Signal: sig_num
                let status = (exit_code << 8) & 0xFF00; // Normal exit
                unsafe {
                    *wstatus = status;
                }
            }
            child_pid.as_u64() as i64
        }
        None => {
            // Check WNOHANG
            if options & 1 != 0 {
                0 // No child changed state
            } else {
                -crate::fs::errno::ECHILD as i64 // No children
            }
        }
    }
}

fn sys_clone(flags: u64, _stack: u64, parent_tid: *mut i32, child_tid: *mut i32, _tls: u64) -> i64 {
    // Clone flags (from Linux)
    const CLONE_VM: u64 = 0x00000100;
    const CLONE_FS: u64 = 0x00000200;
    const CLONE_FILES: u64 = 0x00000400;
    const CLONE_SIGHAND: u64 = 0x00000800;
    const CLONE_THREAD: u64 = 0x00010000;
    const CLONE_PARENT_SETTID: u64 = 0x00100000;
    const CLONE_CHILD_CLEARTID: u64 = 0x00200000;
    const CLONE_CHILD_SETTID: u64 = 0x01000000;

    // For now, implement basic fork-like behavior
    // A full implementation would handle all flags
    if (flags & CLONE_THREAD) != 0 {
        // Thread creation (like pthread_create)
        // This is a simplified implementation
        let result = crate::process::fork();
        match result {
            Some(pid) => {
                if (flags & CLONE_PARENT_SETTID) != 0 && !parent_tid.is_null() {
                    unsafe { *parent_tid = pid.as_u64() as i32; }
                }
                if (flags & CLONE_CHILD_SETTID) != 0 && !child_tid.is_null() {
                    unsafe { *child_tid = pid.as_u64() as i32; }
                }
                pid.as_u64() as i64
            }
            None => -crate::fs::errno::ENOMEM as i64,
        }
    } else {
        // Process fork
        crate::process::fork()
            .map(|p| {
                if (flags & CLONE_PARENT_SETTID) != 0 && !parent_tid.is_null() {
                    unsafe { *parent_tid = p.as_u64() as i32; }
                }
                p.as_u64() as i64
            })
            .unwrap_or(-crate::fs::errno::ENOMEM as i64)
    }
}

fn sys_sched_yield() -> i64 {
    crate::scheduler::yield_cpu();
    0
}

/// Read a null-terminated string from user space
fn read_user_string(ptr: *const u8, max_len: usize) -> Option<String> {
    if ptr.is_null() {
        return None;
    }

    let mut bytes = Vec::new();
    for i in 0..max_len {
        let byte = unsafe { *ptr.add(i) };
        if byte == 0 {
            break;
        }
        bytes.push(byte);
    }

    String::from_utf8(bytes).ok()
}

/// Read a null-terminated string array from user space
fn read_string_array(arr: *const *const u8, max_count: usize, max_str_len: usize) -> Vec<String> {
    use alloc::vec::Vec;

    let mut result = Vec::new();

    for i in 0..max_count {
        let ptr = unsafe { *arr.add(i) };
        if ptr.is_null() {
            break;
        }
        if let Some(s) = read_user_string(ptr, max_str_len) {
            result.push(s);
        } else {
            break;
        }
    }

    result
}

// =============================================================================
// TIME SYSCALLS
// =============================================================================

fn sys_nanosleep(req: *const u8, rem: *mut u8) -> i64 {
    use crate::drivers::timer::{self, Timespec};

    if req.is_null() {
        return -22; // EINVAL
    }

    let timespec = unsafe {
        let ptr = req as *const Timespec;
        *ptr
    };

    if timespec.tv_nsec < 0 || timespec.tv_nsec >= 1_000_000_000 {
        return -22; // EINVAL
    }

    let sleep_ns = timespec.to_ns();
    let start = timer::monotonic_ns();

    timer::sleep_ns(sleep_ns);

    let elapsed = timer::monotonic_ns() - start;
    let remaining = if elapsed < sleep_ns { sleep_ns - elapsed } else { 0 };

    // Write remaining time if rem is provided
    if !rem.is_null() {
        let rem_timespec = Timespec::from_ns(remaining);
        unsafe {
            let ptr = rem as *mut Timespec;
            *ptr = rem_timespec;
        }
    }

    0
}

fn sys_clock_gettime(clock_id: i32, tp: *mut u8) -> i64 {
    use crate::drivers::timer::{self, ClockId, Timespec};

    if tp.is_null() {
        return -22; // EINVAL
    }

    let clock = match clock_id {
        0 => ClockId::Realtime,
        1 => ClockId::Monotonic,
        2 => ClockId::ProcessCputime,
        3 => ClockId::ThreadCputime,
        4 => ClockId::MonotonicRaw,
        5 => ClockId::RealtimeCoarse,
        6 => ClockId::MonotonicCoarse,
        7 => ClockId::Boottime,
        _ => return -22, // EINVAL
    };

    let timespec = timer::clock_gettime(clock);

    unsafe {
        let ptr = tp as *mut Timespec;
        *ptr = timespec;
    }

    0
}

fn sys_clock_getres(clock_id: i32, res: *mut u8) -> i64 {
    use crate::drivers::timer::{self, Timespec};

    if res.is_null() {
        return 0; // Just checking if clock is valid
    }

    // Validate clock ID
    if clock_id < 0 || clock_id > 7 {
        return -22; // EINVAL
    }

    let resolution_ns = timer::resolution_ns();
    let timespec = Timespec::from_ns(resolution_ns);

    unsafe {
        let ptr = res as *mut Timespec;
        *ptr = timespec;
    }

    0
}

fn sys_gettimeofday(tv: *mut u8, tz: *mut u8) -> i64 {
    use crate::drivers::timer::{self, Timeval};

    if tv.is_null() {
        return -22; // EINVAL
    }

    let wall_ns = timer::wall_clock_ns();
    let timeval = Timeval::from_ns(wall_ns as u64);

    unsafe {
        let ptr = tv as *mut Timeval;
        *ptr = timeval;
    }

    // Timezone is deprecated, ignore
    let _ = tz;

    0
}

// =============================================================================
// MEMORY SYSCALLS
// =============================================================================

/// mmap protection flags
mod prot_flags {
    pub const PROT_NONE: i32 = 0x0;
    pub const PROT_READ: i32 = 0x1;
    pub const PROT_WRITE: i32 = 0x2;
    pub const PROT_EXEC: i32 = 0x4;
}

/// mmap mapping flags
mod map_flags {
    pub const MAP_SHARED: i32 = 0x01;
    pub const MAP_PRIVATE: i32 = 0x02;
    pub const MAP_FIXED: i32 = 0x10;
    pub const MAP_ANONYMOUS: i32 = 0x20;
    pub const MAP_GROWSDOWN: i32 = 0x100;
    pub const MAP_LOCKED: i32 = 0x2000;
    pub const MAP_POPULATE: i32 = 0x8000;
}

/// User memory region for mmap tracking
struct UserMemoryRegion {
    start: u64,
    size: usize,
    prot: i32,
    flags: i32,
}

/// Global list of user memory regions
static USER_MEMORY_REGIONS: spin::Mutex<alloc::vec::Vec<UserMemoryRegion>> =
    spin::Mutex::new(alloc::vec::Vec::new());

/// Current program break
static PROGRAM_BREAK: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0x0000_1000_0000_0000);

/// Next mmap address hint
static NEXT_MMAP_ADDR: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0x0000_7F00_0000_0000);

fn sys_mmap(addr: u64, len: u64, prot: i32, flags: i32, fd: i32, offset: u64) -> i64 {
    use crate::memory::{self, PageFlags, PAGE_SIZE};
    use core::sync::atomic::Ordering;

    if len == 0 {
        return -(crate::fs::errno::EINVAL as i64);
    }

    // Round up to page boundary
    let aligned_len = memory::page_align_up(len);
    let num_pages = (aligned_len / PAGE_SIZE as u64) as usize;

    // Determine mapping address
    let map_addr = if (flags & map_flags::MAP_FIXED) != 0 {
        // Fixed address requested
        if addr == 0 || addr >= memory::USER_SPACE_END {
            return -(crate::fs::errno::EINVAL as i64);
        }
        memory::page_align_down(addr)
    } else if addr != 0 {
        // Hint address provided
        memory::page_align_down(addr)
    } else {
        // Allocate from mmap region
        let mmap_addr = NEXT_MMAP_ADDR.fetch_sub(aligned_len, Ordering::SeqCst) - aligned_len;
        mmap_addr
    };

    // Check user space bounds
    if map_addr + aligned_len > memory::USER_SPACE_END {
        return -(crate::fs::errno::ENOMEM as i64);
    }

    // Allocate physical pages and map them
    let mut mm = crate::memory::MemoryManager::get().lock();
    let mm = match mm.as_mut() {
        Some(m) => m,
        None => return -(crate::fs::errno::ENOMEM as i64),
    };

    // Determine page flags from protection
    let mut page_flags = PageFlags::PRESENT | PageFlags::USER;
    if (prot & prot_flags::PROT_WRITE) != 0 {
        page_flags |= PageFlags::WRITABLE;
    }
    if (prot & prot_flags::PROT_EXEC) == 0 {
        page_flags |= PageFlags::NO_EXECUTE;
    }

    // Allocate and map pages
    for i in 0..num_pages {
        let virt_addr = map_addr + (i * PAGE_SIZE) as u64;

        // Allocate physical page
        let phys_addr = match mm.alloc_pages(1) {
            Some(addr) => addr,
            None => {
                // Rollback already allocated pages
                for j in 0..i {
                    let rollback_addr = map_addr + (j * PAGE_SIZE) as u64;
                    mm.unmap_page(rollback_addr);
                    // Note: Should also free physical pages here
                }
                return -(crate::fs::errno::ENOMEM as i64);
            }
        };

        // Zero the page for anonymous mappings
        if (flags & map_flags::MAP_ANONYMOUS) != 0 {
            unsafe {
                let ptr = memory::phys_to_virt(phys_addr) as *mut u8;
                core::ptr::write_bytes(ptr, 0, PAGE_SIZE);
            }
        }

        // Map the page
        mm.map_page(virt_addr, phys_addr, page_flags);
    }

    // Track the mapping
    USER_MEMORY_REGIONS.lock().push(UserMemoryRegion {
        start: map_addr,
        size: aligned_len as usize,
        prot,
        flags,
    });

    // If file-backed, read content from file
    if fd >= 0 && (flags & map_flags::MAP_ANONYMOUS) == 0 {
        // Read file content into mapped region
        // (Simplified - full implementation would handle this properly)
        let _ = (fd, offset);
    }

    map_addr as i64
}

fn sys_munmap(addr: u64, len: u64) -> i64 {
    use crate::memory::{self, PAGE_SIZE};

    if addr == 0 || len == 0 {
        return -(crate::fs::errno::EINVAL as i64);
    }

    // Must be page-aligned
    if addr & (PAGE_SIZE as u64 - 1) != 0 {
        return -(crate::fs::errno::EINVAL as i64);
    }

    let aligned_len = memory::page_align_up(len);
    let num_pages = (aligned_len / PAGE_SIZE as u64) as usize;

    // Unmap pages
    let mut mm = crate::memory::MemoryManager::get().lock();
    let mm = match mm.as_mut() {
        Some(m) => m,
        None => return -(crate::fs::errno::ENOMEM as i64),
    };

    for i in 0..num_pages {
        let virt_addr = addr + (i * PAGE_SIZE) as u64;
        mm.unmap_page(virt_addr);
        // Note: Should also free physical pages here
    }

    // Remove from tracking
    let mut regions = USER_MEMORY_REGIONS.lock();
    regions.retain(|r| {
        // Check for overlap
        let region_end = r.start + r.size as u64;
        let unmap_end = addr + aligned_len;
        !(r.start < unmap_end && addr < region_end)
    });

    0
}

fn sys_brk(addr: u64) -> i64 {
    use crate::memory::{self, PageFlags, PAGE_SIZE};
    use core::sync::atomic::Ordering;

    let current_brk = PROGRAM_BREAK.load(Ordering::SeqCst);

    // If addr is 0, return current break
    if addr == 0 {
        return current_brk as i64;
    }

    // Align new break to page boundary
    let new_brk = memory::page_align_up(addr);

    // Check bounds
    if new_brk > memory::USER_SPACE_END || new_brk < 0x0000_0000_0010_0000 {
        return current_brk as i64; // Return current on error
    }

    let mut mm = crate::memory::MemoryManager::get().lock();
    let mm = match mm.as_mut() {
        Some(m) => m,
        None => return current_brk as i64,
    };

    if new_brk > current_brk {
        // Growing - need to allocate pages
        let start_page = memory::page_align_up(current_brk);
        let end_page = memory::page_align_up(new_brk);
        let num_pages = ((end_page - start_page) / PAGE_SIZE as u64) as usize;

        let page_flags = PageFlags::PRESENT | PageFlags::WRITABLE | PageFlags::USER | PageFlags::NO_EXECUTE;

        for i in 0..num_pages {
            let virt_addr = start_page + (i * PAGE_SIZE) as u64;

            // Allocate physical page
            let phys_addr = match mm.alloc_pages(1) {
                Some(addr) => addr,
                None => return current_brk as i64, // Return current on error
            };

            // Zero the page
            unsafe {
                let ptr = memory::phys_to_virt(phys_addr) as *mut u8;
                core::ptr::write_bytes(ptr, 0, PAGE_SIZE);
            }

            // Map the page
            mm.map_page(virt_addr, phys_addr, page_flags);
        }
    } else if new_brk < current_brk {
        // Shrinking - free pages
        let start_page = memory::page_align_up(new_brk);
        let end_page = memory::page_align_up(current_brk);
        let num_pages = ((end_page - start_page) / PAGE_SIZE as u64) as usize;

        for i in 0..num_pages {
            let virt_addr = start_page + (i * PAGE_SIZE) as u64;
            mm.unmap_page(virt_addr);
        }
    }

    // Update break
    PROGRAM_BREAK.store(new_brk, Ordering::SeqCst);
    new_brk as i64
}

fn sys_mprotect(addr: u64, len: u64, prot: i32) -> i64 {
    use crate::memory::{self, PAGE_SIZE};

    // Validate address alignment
    if addr & (PAGE_SIZE as u64 - 1) != 0 {
        return -(crate::fs::errno::EINVAL as i64);
    }

    if len == 0 {
        return 0; // Nothing to do
    }

    let _aligned_len = memory::page_align_up(len);

    // Update protection in tracking
    let mut regions = USER_MEMORY_REGIONS.lock();
    for region in regions.iter_mut() {
        if region.start <= addr && addr < region.start + region.size as u64 {
            region.prot = prot;
        }
    }

    // Note: Would need to update page table entries to change protection
    // This requires re-mapping pages with new flags

    0
}

// =============================================================================
// AI SYSCALLS
// =============================================================================

fn sys_ai_query(_prompt: *const u8, _prompt_len: u64, _response: *mut u8, _response_len: u64) -> i64 {
    // AI query syscall - send prompt to kernel AI engine
    // TODO: Implement
    -1
}

fn sys_ai_infer(_model_id: u64, _input: *const u8, _input_len: u64, _output: *mut u8, _output_len: u64) -> i64 {
    // Run inference on loaded model
    // TODO: Implement
    -1
}

fn sys_ai_tensor_alloc(_dtype: u64, _shape: *const u64, _ndims: u64) -> i64 {
    // Allocate kernel tensor for zero-copy sharing
    // TODO: Implement
    -1
}

fn sys_ai_tensor_free(_tensor_id: u64) -> i64 {
    // Free kernel tensor
    // TODO: Implement
    -1
}

fn sys_ai_tensor_share(_tensor_id: u64, _target_pid: Pid) -> i64 {
    // Share tensor with another process (zero-copy)
    // TODO: Implement
    -1
}

fn sys_ai_priority_boost(tid: Tid, boost: f32) -> i64 {
    // Boost thread priority for AI workload
    crate::scheduler::ai_priority_boost(tid.as_u64() as u32, boost);
    0
}

// =============================================================================
// SELF-HEALING SYSCALLS
// =============================================================================

fn sys_checkpoint() -> i64 {
    // Create process checkpoint for recovery
    crate::healing::create_checkpoint()
}

fn sys_restore(checkpoint_id: u64) -> i64 {
    // Restore from checkpoint
    crate::healing::restore_checkpoint(checkpoint_id)
}

fn sys_heal(error_code: i32) -> i64 {
    // Request self-healing for error
    crate::healing::heal_error(error_code)
}

// =============================================================================
// IPC SYSCALLS
// =============================================================================

fn sys_pipe(fds: *mut i32, _flags: i32) -> i64 {
    use crate::ipc;

    match ipc::pipe_create() {
        Ok((read_fd, write_fd)) => {
            if !fds.is_null() {
                unsafe {
                    *fds = read_fd;
                    *fds.add(1) = write_fd;
                }
            }
            0
        }
        Err(e) => -(e as i64),
    }
}

fn sys_msgget(key: i32, flags: i32) -> i64 {
    use crate::ipc;

    let name = if key == -1 {
        alloc::format!("anon_{}", key)
    } else {
        alloc::format!("key_{}", key)
    };

    let create = (flags & 0o1000) != 0; // IPC_CREAT

    match ipc::mq_open(&name, create, 256, 8192) {
        Ok(id) => id.0 as i64,
        Err(e) => -(e as i64),
    }
}

fn sys_msgsnd(msqid: i32, msgp: *const u8, msgsz: usize, _flags: i32) -> i64 {
    use crate::ipc::{self, MqId};
    use crate::process;

    if msgp.is_null() || msgsz == 0 {
        return -(ipc::errno::EINVAL as i64);
    }

    let data = unsafe { core::slice::from_raw_parts(msgp, msgsz) };
    let pid = process::current().unwrap_or(Pid::new(0));

    match ipc::mq_send(MqId(msqid as u64), data, 0, pid) {
        Ok(()) => 0,
        Err(e) => -(e as i64),
    }
}

fn sys_msgrcv(msqid: i32, msgp: *mut u8, msgsz: usize, mtype: i64, _flags: i32) -> i64 {
    use crate::ipc::{self, MqId};

    if msgp.is_null() {
        return -(ipc::errno::EINVAL as i64);
    }

    match ipc::mq_receive(MqId(msqid as u64), mtype) {
        Ok(msg) => {
            let copy_len = msg.mtext.len().min(msgsz);
            unsafe {
                core::ptr::copy_nonoverlapping(msg.mtext.as_ptr(), msgp, copy_len);
            }
            copy_len as i64
        }
        Err(e) => -(e as i64),
    }
}

fn sys_shmget(key: i32, size: usize, flags: i32) -> i64 {
    use crate::ipc;

    let name = if key == -1 {
        alloc::format!("anon_shm_{}", key)
    } else {
        alloc::format!("shm_key_{}", key)
    };

    let create = (flags & 0o1000) != 0; // IPC_CREAT

    match ipc::shm_open(&name, create, size) {
        Ok(id) => id.0 as i64,
        Err(e) => -(e as i64),
    }
}

fn sys_shmat(shmid: i32, addr: u64, _flags: i32) -> i64 {
    use crate::ipc::{self, ShmId};
    use crate::process;

    let pid = process::current().unwrap_or(Pid::new(0));

    match ipc::shm_attach(ShmId(shmid as u64), pid, addr) {
        Ok(vaddr) => vaddr as i64,
        Err(e) => -(e as i64),
    }
}

fn sys_shmdt(addr: u64) -> i64 {
    // Would need to look up segment by address
    let _ = addr;
    0
}

fn sys_semget(key: i32, nsems: i32, flags: i32) -> i64 {
    use crate::ipc;

    let name = if key == -1 {
        alloc::format!("anon_sem_{}", key)
    } else {
        alloc::format!("sem_key_{}", key)
    };

    let create = (flags & 0o1000) != 0;
    let initial = nsems.max(1);

    match ipc::sem_open(&name, create, initial) {
        Ok(id) => id.0 as i64,
        Err(e) => -(e as i64),
    }
}

fn sys_semop(semid: i32, _sops: *const u8, _nsops: usize) -> i64 {
    use crate::ipc::{self, SemId};

    // Simplified: single wait/post operation
    match ipc::sem_trywait(SemId(semid as u64)) {
        Ok(()) => 0,
        Err(e) => -(e as i64),
    }
}

fn sys_futex(uaddr: u64, op: i32, val: u32, _timeout: u64, _uaddr2: u64, val3: u32) -> i64 {
    use crate::ipc::{self, futex_op};
    use crate::process;

    let op_cmd = op & 0x7f; // Remove FUTEX_PRIVATE_FLAG

    match op_cmd {
        futex_op::FUTEX_WAIT | futex_op::FUTEX_WAIT_BITSET => {
            let pid = process::current().unwrap_or(Pid::new(0));
            let bitset = if op_cmd == futex_op::FUTEX_WAIT_BITSET { val3 } else { !0 };
            match ipc::futex_wait(uaddr, val, bitset, pid) {
                Ok(()) => 0,
                Err(e) => -(e as i64),
            }
        }
        futex_op::FUTEX_WAKE | futex_op::FUTEX_WAKE_BITSET => {
            let bitset = if op_cmd == futex_op::FUTEX_WAKE_BITSET { val3 } else { !0 };
            match ipc::futex_wake(uaddr, val as i32, bitset) {
                Ok(woken) => woken as i64,
                Err(e) => -(e as i64),
            }
        }
        futex_op::FUTEX_REQUEUE | futex_op::FUTEX_CMP_REQUEUE => {
            // Requeue waiters from uaddr to uaddr2
            match ipc::futex_wake(uaddr, val as i32, !0) {
                Ok(woken) => woken as i64,
                Err(e) => -(e as i64),
            }
        }
        _ => -(ipc::errno::EINVAL as i64),
    }
}

fn sys_eventfd(initval: u32, flags: i32) -> i64 {
    use crate::ipc;

    match ipc::eventfd_create(initval as u64, flags as u32) {
        Ok(fd) => fd as i64,
        Err(e) => -(e as i64),
    }
}

fn sys_rt_sigaction(sig: i32, act: u64, oldact: u64) -> i64 {
    use crate::ipc::{self, SigAction};

    let action = if act != 0 {
        // Parse action from user memory
        SigAction::Default // Simplified
    } else {
        SigAction::Default
    };

    match ipc::sigaction(sig, action) {
        Ok(_old) => {
            if oldact != 0 {
                // Write old action to user memory
            }
            0
        }
        Err(e) => -(e as i64),
    }
}

fn sys_rt_sigprocmask(how: i32, set: u64, oldset: u64) -> i64 {
    use crate::ipc::{self, SigSet};

    let sigset = if set != 0 {
        // Read set from user memory
        SigSet::empty()
    } else {
        SigSet::empty()
    };

    match ipc::sigprocmask(how, &sigset) {
        Ok(_old) => {
            if oldset != 0 {
                // Write old set to user memory
            }
            0
        }
        Err(e) => -(e as i64),
    }
}

fn sys_kill(pid: i32, sig: i32) -> i64 {
    use crate::ipc;

    match ipc::kill(Pid::new(pid as u64), sig) {
        Ok(()) => 0,
        Err(e) => -(e as i64),
    }
}

fn sys_mq_open(name: *const u8, oflag: i32, _mode: u32, attr: u64) -> i64 {
    use crate::ipc;

    if name.is_null() {
        return -(ipc::errno::EINVAL as i64);
    }

    // Read name from user memory (simplified)
    let name_str = "mqueue"; // Placeholder

    let create = (oflag & 0x40) != 0; // O_CREAT
    let (max_msgs, max_size) = if attr != 0 {
        // Would read from attr structure
        (256, 8192)
    } else {
        (256, 8192)
    };

    match ipc::mq_open(name_str, create, max_msgs, max_size) {
        Ok(id) => id.0 as i64,
        Err(e) => -(e as i64),
    }
}

fn sys_mq_send(mqdes: i32, msg_ptr: *const u8, msg_len: usize, msg_prio: u32) -> i64 {
    use crate::ipc::{self, MqId};
    use crate::process;

    if msg_ptr.is_null() {
        return -(ipc::errno::EINVAL as i64);
    }

    let data = unsafe { core::slice::from_raw_parts(msg_ptr, msg_len) };
    let pid = process::current().unwrap_or(Pid::new(0));

    match ipc::mq_send(MqId(mqdes as u64), data, msg_prio, pid) {
        Ok(()) => 0,
        Err(e) => -(e as i64),
    }
}

fn sys_mq_receive(mqdes: i32, msg_ptr: *mut u8, msg_len: usize, msg_prio: *mut u32) -> i64 {
    use crate::ipc::{self, MqId};

    if msg_ptr.is_null() {
        return -(ipc::errno::EINVAL as i64);
    }

    match ipc::mq_receive(MqId(mqdes as u64), 0) {
        Ok(msg) => {
            let copy_len = msg.mtext.len().min(msg_len);
            unsafe {
                core::ptr::copy_nonoverlapping(msg.mtext.as_ptr(), msg_ptr, copy_len);
                if !msg_prio.is_null() {
                    *msg_prio = msg.mtype as u32;
                }
            }
            copy_len as i64
        }
        Err(e) => -(e as i64),
    }
}

// =============================================================================
// SOCKET SYSCALLS
// =============================================================================

/// Socket file descriptor base (to distinguish from regular files)
const SOCKET_FD_BASE: i32 = 1000;

fn sys_socket(domain: u16, socket_type: u32, protocol: u32) -> i64 {
    use crate::net::{self, socket};

    let stack = match net::get_stack() {
        Some(s) => s,
        None => return -97, // EAFNOSUPPORT
    };

    match socket::sys_socket(stack, domain, socket_type, protocol) {
        Ok(_sock) => {
            match stack.alloc_socket(net::Socket::new(
                socket::SocketType::from_u32(socket_type & 0xFF).unwrap_or(socket::SocketType::Stream)
            )) {
                Ok(fd) => (SOCKET_FD_BASE + fd as i32) as i64,
                Err(e) => e.to_errno() as i64,
            }
        }
        Err(e) => e.to_errno() as i64,
    }
}

fn sys_bind(sockfd: i32, addr: *const u8, addrlen: u32) -> i64 {
    use crate::net::{self, socket};

    if addr.is_null() || addrlen < 8 {
        return -22; // EINVAL
    }

    let stack = match net::get_stack() {
        Some(s) => s,
        None => return -97, // EAFNOSUPPORT
    };

    let fd = (sockfd - SOCKET_FD_BASE) as usize;
    let sock = match stack.get_socket(fd) {
        Some(s) => s,
        None => return -88, // ENOTSOCK
    };

    let addr_bytes = unsafe { core::slice::from_raw_parts(addr, addrlen as usize) };
    match socket::sys_bind(&sock, addr_bytes) {
        Ok(()) => 0,
        Err(e) => e.to_errno() as i64,
    }
}

fn sys_listen(sockfd: i32, backlog: i32) -> i64 {
    use crate::net::{self, socket};

    let stack = match net::get_stack() {
        Some(s) => s,
        None => return -97,
    };

    let fd = (sockfd - SOCKET_FD_BASE) as usize;
    let sock = match stack.get_socket(fd) {
        Some(s) => s,
        None => return -88,
    };

    match socket::sys_listen(stack, &sock, backlog as u32) {
        Ok(()) => 0,
        Err(e) => e.to_errno() as i64,
    }
}

fn sys_accept(sockfd: i32, addr: *mut u8, addrlen: *mut u32) -> i64 {
    use crate::net::{self, socket};

    let stack = match net::get_stack() {
        Some(s) => s,
        None => return -97,
    };

    let fd = (sockfd - SOCKET_FD_BASE) as usize;
    let sock = match stack.get_socket(fd) {
        Some(s) => s,
        None => return -88,
    };

    match socket::sys_accept(&sock) {
        Ok(Some((_conn, remote_addr))) => {
            // Write remote address if provided
            if !addr.is_null() && !addrlen.is_null() {
                let addr_bytes = remote_addr.to_bytes();
                let len = unsafe { *addrlen as usize };
                let copy_len = len.min(addr_bytes.len());
                unsafe {
                    core::ptr::copy_nonoverlapping(addr_bytes.as_ptr(), addr, copy_len);
                    *addrlen = copy_len as u32;
                }
            }

            // Allocate new socket for the accepted connection
            let new_sock = net::Socket::new(socket::SocketType::Stream);
            match stack.alloc_socket(new_sock) {
                Ok(new_fd) => (SOCKET_FD_BASE + new_fd as i32) as i64,
                Err(e) => e.to_errno() as i64,
            }
        }
        Ok(None) => -11, // EAGAIN
        Err(e) => e.to_errno() as i64,
    }
}

fn sys_connect(sockfd: i32, addr: *const u8, addrlen: u32) -> i64 {
    use crate::net::{self, socket};

    if addr.is_null() || addrlen < 8 {
        return -22;
    }

    let stack = match net::get_stack() {
        Some(s) => s,
        None => return -97,
    };

    let iface = match stack.primary_interface() {
        Some(i) => i,
        None => return -101, // ENETUNREACH
    };

    let fd = (sockfd - SOCKET_FD_BASE) as usize;
    let sock = match stack.get_socket(fd) {
        Some(s) => s,
        None => return -88,
    };

    let addr_bytes = unsafe { core::slice::from_raw_parts(addr, addrlen as usize) };
    match socket::sys_connect(stack, iface, &sock, addr_bytes) {
        Ok(()) => 0,
        Err(e) => e.to_errno() as i64,
    }
}

fn sys_sendto(
    sockfd: i32,
    buf: *const u8,
    len: usize,
    _flags: i32,
    dest_addr: *const u8,
    addrlen: u32,
) -> i64 {
    use crate::net::{self, socket};

    if buf.is_null() || len == 0 {
        return 0;
    }

    let stack = match net::get_stack() {
        Some(s) => s,
        None => return -97,
    };

    let iface = match stack.primary_interface() {
        Some(i) => i,
        None => return -101,
    };

    let fd = (sockfd - SOCKET_FD_BASE) as usize;
    let sock = match stack.get_socket(fd) {
        Some(s) => s,
        None => return -88,
    };

    let data = unsafe { core::slice::from_raw_parts(buf, len) };

    if !dest_addr.is_null() && addrlen >= 8 {
        // sendto with destination address
        let addr_bytes = unsafe { core::slice::from_raw_parts(dest_addr, addrlen as usize) };
        if let Some(addr) = socket::SocketAddr::from_bytes(addr_bytes) {
            match sock.sendto(stack, iface, data, addr) {
                Ok(n) => n as i64,
                Err(e) => e.to_errno() as i64,
            }
        } else {
            -22 // EINVAL
        }
    } else {
        // send on connected socket
        match sock.send(stack, iface, data) {
            Ok(n) => n as i64,
            Err(e) => e.to_errno() as i64,
        }
    }
}

fn sys_recvfrom(
    sockfd: i32,
    buf: *mut u8,
    len: usize,
    _flags: i32,
    src_addr: *mut u8,
    addrlen: *mut u32,
) -> i64 {
    use crate::net::{self};

    if buf.is_null() || len == 0 {
        return 0;
    }

    let stack = match net::get_stack() {
        Some(s) => s,
        None => return -97,
    };

    let fd = (sockfd - SOCKET_FD_BASE) as usize;
    let sock = match stack.get_socket(fd) {
        Some(s) => s,
        None => return -88,
    };

    let buffer = unsafe { core::slice::from_raw_parts_mut(buf, len) };

    if !src_addr.is_null() && !addrlen.is_null() {
        // recvfrom - get source address
        match sock.recvfrom(buffer) {
            Ok((n, addr)) => {
                let addr_bytes = addr.to_bytes();
                let max_len = unsafe { *addrlen as usize };
                let copy_len = max_len.min(addr_bytes.len());
                unsafe {
                    core::ptr::copy_nonoverlapping(addr_bytes.as_ptr(), src_addr, copy_len);
                    *addrlen = copy_len as u32;
                }
                n as i64
            }
            Err(e) => e.to_errno() as i64,
        }
    } else {
        // recv on connected socket
        match sock.recv(buffer) {
            Ok(n) => n as i64,
            Err(e) => e.to_errno() as i64,
        }
    }
}

fn sys_shutdown(sockfd: i32, how: i32) -> i64 {
    use crate::net::{self, socket};

    let stack = match net::get_stack() {
        Some(s) => s,
        None => return -97,
    };

    let iface = match stack.primary_interface() {
        Some(i) => i,
        None => return -101,
    };

    let fd = (sockfd - SOCKET_FD_BASE) as usize;
    let sock = match stack.get_socket(fd) {
        Some(s) => s,
        None => return -88,
    };

    let mode = match how {
        0 => socket::ShutdownMode::Read,
        1 => socket::ShutdownMode::Write,
        2 => socket::ShutdownMode::Both,
        _ => return -22,
    };

    match sock.shutdown(stack, iface, mode) {
        Ok(()) => 0,
        Err(e) => e.to_errno() as i64,
    }
}

fn sys_setsockopt(sockfd: i32, level: i32, optname: i32, optval: *const u8, optlen: u32) -> i64 {
    use crate::net;

    let stack = match net::get_stack() {
        Some(s) => s,
        None => return -97,
    };

    let fd = (sockfd - SOCKET_FD_BASE) as usize;
    let sock = match stack.get_socket(fd) {
        Some(s) => s,
        None => return -88,
    };

    if optval.is_null() || optlen == 0 {
        return -22;
    }

    let value = unsafe { core::slice::from_raw_parts(optval, optlen as usize) };
    match sock.setsockopt(level, optname, value) {
        Ok(()) => 0,
        Err(e) => e.to_errno() as i64,
    }
}

fn sys_getsockopt(sockfd: i32, level: i32, optname: i32, optval: *mut u8, optlen: *mut u32) -> i64 {
    use crate::net;

    let stack = match net::get_stack() {
        Some(s) => s,
        None => return -97,
    };

    let fd = (sockfd - SOCKET_FD_BASE) as usize;
    let sock = match stack.get_socket(fd) {
        Some(s) => s,
        None => return -88,
    };

    if optval.is_null() || optlen.is_null() {
        return -22;
    }

    let len = unsafe { *optlen as usize };
    let buffer = unsafe { core::slice::from_raw_parts_mut(optval, len) };

    match sock.getsockopt(level, optname, buffer) {
        Ok(actual_len) => {
            unsafe { *optlen = actual_len as u32; }
            0
        }
        Err(e) => e.to_errno() as i64,
    }
}

fn sys_getsockname(sockfd: i32, addr: *mut u8, addrlen: *mut u32) -> i64 {
    use crate::net;

    let stack = match net::get_stack() {
        Some(s) => s,
        None => return -97,
    };

    let fd = (sockfd - SOCKET_FD_BASE) as usize;
    let sock = match stack.get_socket(fd) {
        Some(s) => s,
        None => return -88,
    };

    if addr.is_null() || addrlen.is_null() {
        return -22;
    }

    if let Some(local_addr) = sock.local_addr() {
        let addr_bytes = local_addr.to_bytes();
        let max_len = unsafe { *addrlen as usize };
        let copy_len = max_len.min(addr_bytes.len());
        unsafe {
            core::ptr::copy_nonoverlapping(addr_bytes.as_ptr(), addr, copy_len);
            *addrlen = copy_len as u32;
        }
        0
    } else {
        -22 // Not bound
    }
}

fn sys_getpeername(sockfd: i32, addr: *mut u8, addrlen: *mut u32) -> i64 {
    use crate::net;

    let stack = match net::get_stack() {
        Some(s) => s,
        None => return -97,
    };

    let fd = (sockfd - SOCKET_FD_BASE) as usize;
    let sock = match stack.get_socket(fd) {
        Some(s) => s,
        None => return -88,
    };

    if addr.is_null() || addrlen.is_null() {
        return -22;
    }

    if let Some(remote_addr) = sock.remote_addr() {
        let addr_bytes = remote_addr.to_bytes();
        let max_len = unsafe { *addrlen as usize };
        let copy_len = max_len.min(addr_bytes.len());
        unsafe {
            core::ptr::copy_nonoverlapping(addr_bytes.as_ptr(), addr, copy_len);
            *addrlen = copy_len as u32;
        }
        0
    } else {
        -107 // ENOTCONN
    }
}

// =============================================================================
// I/O MULTIPLEXING STRUCTURES
// =============================================================================

/// poll() file descriptor entry
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PollFd {
    /// File descriptor
    pub fd: i32,
    /// Requested events
    pub events: i16,
    /// Returned events
    pub revents: i16,
}

/// poll() event flags
mod poll_events {
    pub const POLLIN: i16 = 0x0001;       // Data to read
    pub const POLLPRI: i16 = 0x0002;      // Urgent data
    pub const POLLOUT: i16 = 0x0004;      // Writing possible
    pub const POLLERR: i16 = 0x0008;      // Error condition
    pub const POLLHUP: i16 = 0x0010;      // Hang up
    pub const POLLNVAL: i16 = 0x0020;     // Invalid fd
    pub const POLLRDNORM: i16 = 0x0040;   // Normal data to read
    pub const POLLRDBAND: i16 = 0x0080;   // Priority data to read
    pub const POLLWRNORM: i16 = 0x0100;   // Writing normal data possible
    pub const POLLWRBAND: i16 = 0x0200;   // Writing priority data possible
}

/// fd_set for select() - bitmap for file descriptors
#[repr(C)]
pub struct FdSet {
    /// Bitmap - each bit represents a file descriptor
    pub fds_bits: [u64; 16], // 1024 bits = max 1024 file descriptors
}

impl FdSet {
    /// Check if an fd is set
    pub fn is_set(&self, fd: i32) -> bool {
        if fd < 0 || fd >= 1024 {
            return false;
        }
        let word = fd as usize / 64;
        let bit = fd as usize % 64;
        (self.fds_bits[word] & (1 << bit)) != 0
    }

    /// Set an fd in the set
    pub fn set(&mut self, fd: i32) {
        if fd >= 0 && fd < 1024 {
            let word = fd as usize / 64;
            let bit = fd as usize % 64;
            self.fds_bits[word] |= 1 << bit;
        }
    }

    /// Clear an fd from the set
    pub fn clear(&mut self, fd: i32) {
        if fd >= 0 && fd < 1024 {
            let word = fd as usize / 64;
            let bit = fd as usize % 64;
            self.fds_bits[word] &= !(1 << bit);
        }
    }

    /// Clear all fds
    pub fn zero(&mut self) {
        for word in &mut self.fds_bits {
            *word = 0;
        }
    }
}

/// Timeval structure for select() timeout
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Timeval {
    pub tv_sec: i64,
    pub tv_usec: i64,
}

impl Timeval {
    /// Convert to nanoseconds
    pub fn to_ns(&self) -> u64 {
        (self.tv_sec as u64) * 1_000_000_000 + (self.tv_usec as u64) * 1_000
    }
}

// =============================================================================
// I/O MULTIPLEXING SYSCALLS
// =============================================================================

fn sys_poll(fds: *mut PollFd, nfds: usize, timeout_ms: i32) -> i64 {
    use crate::drivers::timer;

    if fds.is_null() && nfds > 0 {
        return -22; // EINVAL
    }

    if nfds > 1024 {
        return -22; // EINVAL - too many fds
    }

    let deadline = if timeout_ms < 0 {
        None // Infinite timeout
    } else if timeout_ms == 0 {
        Some(0) // Return immediately
    } else {
        Some(timer::monotonic_ns() + (timeout_ms as u64) * 1_000_000)
    };

    let poll_fds = if nfds > 0 {
        unsafe { core::slice::from_raw_parts_mut(fds, nfds) }
    } else {
        &mut [][..]
    };

    // Clear all revents initially
    for pfd in poll_fds.iter_mut() {
        pfd.revents = 0;
    }

    loop {
        let mut ready_count = 0i64;

        for pfd in poll_fds.iter_mut() {
            let fd = pfd.fd;

            // Check for invalid fd
            if fd < 0 {
                continue;
            }

            // Check stdin/stdout/stderr
            if fd < 3 {
                match fd {
                    0 => {
                        // stdin - check if keyboard has data
                        if crate::drivers::keyboard::Keyboard::has_pending_input() {
                            if (pfd.events & poll_events::POLLIN) != 0 {
                                pfd.revents |= poll_events::POLLIN;
                                ready_count += 1;
                            }
                        }
                    }
                    1 | 2 => {
                        // stdout/stderr - always writable
                        if (pfd.events & poll_events::POLLOUT) != 0 {
                            pfd.revents |= poll_events::POLLOUT;
                            ready_count += 1;
                        }
                    }
                    _ => {}
                }
                continue;
            }

            // Check if it's a socket
            if fd >= SOCKET_FD_BASE {
                if let Some(stack) = crate::net::get_stack() {
                    let sock_fd = (fd - SOCKET_FD_BASE) as usize;
                    if let Some(sock) = stack.get_socket(sock_fd) {
                        // Check read readiness
                        if (pfd.events & poll_events::POLLIN) != 0 {
                            if sock.has_data() || sock.has_pending_connection() {
                                pfd.revents |= poll_events::POLLIN;
                                ready_count += 1;
                            }
                        }

                        // Check write readiness
                        if (pfd.events & poll_events::POLLOUT) != 0 {
                            if sock.can_write() {
                                pfd.revents |= poll_events::POLLOUT;
                                ready_count += 1;
                            }
                        }

                        // Check for errors
                        if sock.has_error() {
                            pfd.revents |= poll_events::POLLERR;
                            ready_count += 1;
                        }

                        // Check for hangup
                        if sock.is_closed() {
                            pfd.revents |= poll_events::POLLHUP;
                            ready_count += 1;
                        }
                    } else {
                        pfd.revents = poll_events::POLLNVAL;
                        ready_count += 1;
                    }
                }
                continue;
            }

            // Regular file descriptor - check if file exists
            if crate::fs::is_valid_fd(fd as u64) {
                // Files are always readable and writable
                if (pfd.events & poll_events::POLLIN) != 0 {
                    pfd.revents |= poll_events::POLLIN;
                }
                if (pfd.events & poll_events::POLLOUT) != 0 {
                    pfd.revents |= poll_events::POLLOUT;
                }
                if pfd.revents != 0 {
                    ready_count += 1;
                }
            } else {
                pfd.revents = poll_events::POLLNVAL;
                ready_count += 1;
            }
        }

        // If we found ready fds, return count
        if ready_count > 0 {
            return ready_count;
        }

        // Check timeout
        if let Some(dl) = deadline {
            if dl == 0 || timer::monotonic_ns() >= dl {
                return 0; // Timeout
            }
        }

        // Yield and try again
        crate::scheduler::yield_now();
    }
}

fn sys_select(nfds: i32, readfds: *mut FdSet, writefds: *mut FdSet, exceptfds: *mut FdSet, timeout: *mut Timeval) -> i64 {
    use crate::drivers::timer;

    if nfds < 0 || nfds > 1024 {
        return -22; // EINVAL
    }

    let deadline = if timeout.is_null() {
        None // Infinite timeout
    } else {
        let tv = unsafe { *timeout };
        if tv.tv_sec == 0 && tv.tv_usec == 0 {
            Some(0) // Return immediately
        } else {
            Some(timer::monotonic_ns() + tv.to_ns())
        }
    };

    loop {
        let mut ready_count = 0i64;

        // Check each fd up to nfds
        for fd in 0..nfds {
            let check_read = !readfds.is_null() && unsafe { (*readfds).is_set(fd) };
            let check_write = !writefds.is_null() && unsafe { (*writefds).is_set(fd) };
            let check_except = !exceptfds.is_null() && unsafe { (*exceptfds).is_set(fd) };

            let mut is_readable = false;
            let mut is_writable = false;
            let mut has_exception = false;

            // Check stdin/stdout/stderr
            if fd < 3 {
                match fd {
                    0 => {
                        is_readable = crate::drivers::keyboard::Keyboard::has_pending_input();
                    }
                    1 | 2 => {
                        is_writable = true;
                    }
                    _ => {}
                }
            } else if fd >= SOCKET_FD_BASE as i32 {
                // Socket fd
                if let Some(stack) = crate::net::get_stack() {
                    let sock_fd = (fd - SOCKET_FD_BASE as i32) as usize;
                    if let Some(sock) = stack.get_socket(sock_fd) {
                        is_readable = sock.has_data() || sock.has_pending_connection();
                        is_writable = sock.can_write();
                        has_exception = sock.has_error();
                    }
                }
            } else {
                // Regular file - always ready
                if crate::fs::is_valid_fd(fd as u64) {
                    is_readable = true;
                    is_writable = true;
                }
            }

            // Update result sets
            if check_read {
                if is_readable {
                    ready_count += 1;
                } else {
                    unsafe { (*readfds).clear(fd); }
                }
            }

            if check_write {
                if is_writable {
                    ready_count += 1;
                } else {
                    unsafe { (*writefds).clear(fd); }
                }
            }

            if check_except {
                if has_exception {
                    ready_count += 1;
                } else {
                    unsafe { (*exceptfds).clear(fd); }
                }
            }
        }

        // If any fds are ready, return
        if ready_count > 0 {
            return ready_count;
        }

        // Check timeout
        if let Some(dl) = deadline {
            if dl == 0 || timer::monotonic_ns() >= dl {
                // Clear all sets on timeout
                if !readfds.is_null() {
                    unsafe { (*readfds).zero(); }
                }
                if !writefds.is_null() {
                    unsafe { (*writefds).zero(); }
                }
                if !exceptfds.is_null() {
                    unsafe { (*exceptfds).zero(); }
                }
                return 0;
            }
        }

        // Yield and try again
        crate::scheduler::yield_now();
    }
}
