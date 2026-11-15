// ===============================================================================
// PROCESS MANAGEMENT
// ===============================================================================

use crate::syscall::*;

// =============================================================================
// PROCESS INFORMATION
// =============================================================================

/// Get current process ID
pub fn getpid() -> i32 {
    unsafe { syscall0(SYS_GETPID) as i32 }
}

/// Get parent process ID
pub fn getppid() -> i32 {
    unsafe { syscall0(SYS_GETPPID) as i32 }
}

/// Get user ID
pub fn getuid() -> u32 {
    unsafe { syscall0(SYS_GETUID) as u32 }
}

/// Get group ID
pub fn getgid() -> u32 {
    unsafe { syscall0(SYS_GETGID) as u32 }
}

/// Get effective user ID
pub fn geteuid() -> u32 {
    unsafe { syscall0(SYS_GETEUID) as u32 }
}

/// Get effective group ID
pub fn getegid() -> u32 {
    unsafe { syscall0(SYS_GETEGID) as u32 }
}

/// Get process group ID
pub fn getpgid(pid: i32) -> i32 {
    unsafe { syscall1(SYS_GETPGID, pid as u64) as i32 }
}

/// Set process group ID
pub fn setpgid(pid: i32, pgid: i32) -> i32 {
    unsafe { syscall2(SYS_SETPGID, pid as u64, pgid as u64) as i32 }
}

/// Create new session
pub fn setsid() -> i32 {
    unsafe { syscall0(SYS_SETSID) as i32 }
}

// =============================================================================
// PROCESS CONTROL
// =============================================================================

/// Fork the current process
pub fn fork() -> i32 {
    unsafe { syscall0(SYS_FORK) as i32 }
}

/// Exit the current process
pub fn exit(code: i32) -> ! {
    unsafe { syscall1(SYS_EXIT, code as u64) };
    loop {}
}

/// Wait for any child process
pub fn wait(status: &mut i32) -> i32 {
    unsafe {
        syscall4(
            SYS_WAIT4,
            u64::MAX, // -1 = any child
            status as *mut i32 as u64,
            0,
            0,
        ) as i32
    }
}

/// Wait for a specific child process
pub fn waitpid(pid: i32, status: &mut i32, options: i32) -> i32 {
    unsafe {
        syscall4(
            SYS_WAIT4,
            pid as u64,
            status as *mut i32 as u64,
            options as u64,
            0,
        ) as i32
    }
}

/// Execute a program
pub fn execve(path: &[u8], argv: *const *const u8, envp: *const *const u8) -> i32 {
    unsafe {
        syscall3(
            SYS_EXECVE,
            path.as_ptr() as u64,
            argv as u64,
            envp as u64,
        ) as i32
    }
}

/// Execute a program (null-terminated path)
pub fn execve_cstr(path: *const u8, argv: *const *const u8, envp: *const *const u8) -> i32 {
    unsafe {
        syscall3(
            SYS_EXECVE,
            path as u64,
            argv as u64,
            envp as u64,
        ) as i32
    }
}

/// Send a signal to a process
pub fn kill(pid: i32, sig: i32) -> i32 {
    unsafe { syscall2(SYS_KILL, pid as u64, sig as u64) as i32 }
}

// =============================================================================
// WAIT STATUS MACROS
// =============================================================================

/// Check if child exited normally
pub fn wifexited(status: i32) -> bool {
    (status & 0x7f) == 0
}

/// Get exit status (only valid if WIFEXITED)
pub fn wexitstatus(status: i32) -> i32 {
    (status >> 8) & 0xff
}

/// Check if child was killed by signal
pub fn wifsignaled(status: i32) -> bool {
    ((status & 0x7f) + 1) >> 1 > 0
}

/// Get signal that killed child (only valid if WIFSIGNALED)
pub fn wtermsig(status: i32) -> i32 {
    status & 0x7f
}

/// Check if child is stopped
pub fn wifstopped(status: i32) -> bool {
    (status & 0xff) == 0x7f
}

/// Get signal that stopped child (only valid if WIFSTOPPED)
pub fn wstopsig(status: i32) -> i32 {
    (status >> 8) & 0xff
}

// =============================================================================
// SIGNALS
// =============================================================================

pub const SIGHUP: i32 = 1;
pub const SIGINT: i32 = 2;
pub const SIGQUIT: i32 = 3;
pub const SIGILL: i32 = 4;
pub const SIGTRAP: i32 = 5;
pub const SIGABRT: i32 = 6;
pub const SIGBUS: i32 = 7;
pub const SIGFPE: i32 = 8;
pub const SIGKILL: i32 = 9;
pub const SIGUSR1: i32 = 10;
pub const SIGSEGV: i32 = 11;
pub const SIGUSR2: i32 = 12;
pub const SIGPIPE: i32 = 13;
pub const SIGALRM: i32 = 14;
pub const SIGTERM: i32 = 15;
pub const SIGCHLD: i32 = 17;
pub const SIGCONT: i32 = 18;
pub const SIGSTOP: i32 = 19;
pub const SIGTSTP: i32 = 20;
pub const SIGTTIN: i32 = 21;
pub const SIGTTOU: i32 = 22;
