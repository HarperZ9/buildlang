//! Seccomp (Secure Computing Mode)
//!
//! Provides system call filtering using BPF programs.

use alloc::vec::Vec;

/// Seccomp operation modes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeccompMode {
    /// Seccomp disabled
    Disabled,
    /// Strict mode: only read, write, exit, sigreturn allowed
    Strict,
    /// Filter mode: BPF program decides
    Filter,
}

/// Seccomp action return values
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeccompAction {
    /// Kill the thread (default kill action)
    KillThread,
    /// Kill the process
    KillProcess,
    /// Send SIGSYS to the thread
    Trap,
    /// Return an errno value
    Errno(u16),
    /// Notify userspace (requires seccomp notify fd)
    UserNotify,
    /// Allow after logging
    Log,
    /// Allow the syscall
    Allow,
}

impl SeccompAction {
    /// Kill is an alias for KillThread
    #[allow(non_upper_case_globals)]
    pub const Kill: SeccompAction = SeccompAction::KillThread;
}

impl SeccompAction {
    /// Parse from BPF return value
    pub fn from_ret(ret: u32) -> Self {
        let action = ret & 0xFFFF0000;
        let data = (ret & 0xFFFF) as u16;

        match action {
            0x00000000 => Self::KillThread,
            0x80000000 => Self::KillProcess,
            0x00030000 => Self::Trap,
            0x00050000 => Self::Errno(data),
            0x7FC00000 => Self::UserNotify,
            0x7FFC0000 => Self::Log,
            0x7FFF0000 => Self::Allow,
            _ => Self::KillProcess,
        }
    }

    /// Convert to BPF return value
    pub fn to_ret(&self) -> u32 {
        match self {
            Self::KillThread => 0x00000000,
            Self::KillProcess => 0x80000000,
            Self::Trap => 0x00030000,
            Self::Errno(e) => 0x00050000 | (*e as u32),
            Self::UserNotify => 0x7FC00000,
            Self::Log => 0x7FFC0000,
            Self::Allow => 0x7FFF0000,
        }
    }
}

/// Seccomp data passed to BPF filter
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SeccompData {
    /// System call number
    pub nr: i32,
    /// CPU architecture (AUDIT_ARCH_*)
    pub arch: u32,
    /// Instruction pointer
    pub instruction_pointer: u64,
    /// System call arguments
    pub args: [u64; 6],
}

impl SeccompData {
    /// Create from syscall parameters
    pub fn new(nr: u32, args: &[u64; 6]) -> Self {
        Self {
            nr: nr as i32,
            arch: AUDIT_ARCH_X86_64,
            instruction_pointer: 0,
            args: *args,
        }
    }
}

/// Architecture constants
pub const AUDIT_ARCH_X86_64: u32 = 0xC000003E;
pub const AUDIT_ARCH_I386: u32 = 0x40000003;
pub const AUDIT_ARCH_ARM: u32 = 0x40000028;
pub const AUDIT_ARCH_AARCH64: u32 = 0xC00000B7;

/// BPF instruction
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BpfInsn {
    /// Operation code
    pub code: u16,
    /// Jump true offset
    pub jt: u8,
    /// Jump false offset
    pub jf: u8,
    /// Constant/immediate value
    pub k: u32,
}

impl BpfInsn {
    /// Create a new instruction
    pub const fn new(code: u16, jt: u8, jf: u8, k: u32) -> Self {
        Self { code, jt, jf, k }
    }

    /// Return instruction
    pub const fn ret(val: u32) -> Self {
        Self::new(BPF_RET | BPF_K, 0, 0, val)
    }

    /// Return with accumulator value
    pub const fn ret_a() -> Self {
        Self::new(BPF_RET | BPF_A, 0, 0, 0)
    }

    /// Load word from data at offset
    pub const fn ld_w(offset: u32) -> Self {
        Self::new(BPF_LD | BPF_W | BPF_ABS, 0, 0, offset)
    }

    /// Load half-word from data at offset
    pub const fn ld_h(offset: u32) -> Self {
        Self::new(BPF_LD | BPF_H | BPF_ABS, 0, 0, offset)
    }

    /// Jump if equal
    pub const fn jeq(val: u32, jt: u8, jf: u8) -> Self {
        Self::new(BPF_JMP | BPF_JEQ | BPF_K, jt, jf, val)
    }

    /// Jump if greater or equal
    pub const fn jge(val: u32, jt: u8, jf: u8) -> Self {
        Self::new(BPF_JMP | BPF_JGE | BPF_K, jt, jf, val)
    }

    /// Jump if greater than
    pub const fn jgt(val: u32, jt: u8, jf: u8) -> Self {
        Self::new(BPF_JMP | BPF_JGT | BPF_K, jt, jf, val)
    }

    /// Jump unconditionally
    pub const fn ja(offset: u32) -> Self {
        Self::new(BPF_JMP | BPF_JA, 0, 0, offset)
    }

    /// Bitwise AND with constant
    pub const fn and(val: u32) -> Self {
        Self::new(BPF_ALU | BPF_AND | BPF_K, 0, 0, val)
    }
}

// BPF instruction class codes
pub const BPF_LD: u16 = 0x00;
pub const BPF_LDX: u16 = 0x01;
pub const BPF_ST: u16 = 0x02;
pub const BPF_STX: u16 = 0x03;
pub const BPF_ALU: u16 = 0x04;
pub const BPF_JMP: u16 = 0x05;
pub const BPF_RET: u16 = 0x06;
pub const BPF_MISC: u16 = 0x07;

// BPF size modifiers
pub const BPF_W: u16 = 0x00;  // 32-bit
pub const BPF_H: u16 = 0x08;  // 16-bit
pub const BPF_B: u16 = 0x10;  // 8-bit

// BPF addressing mode modifiers
pub const BPF_IMM: u16 = 0x00;
pub const BPF_ABS: u16 = 0x20;
pub const BPF_IND: u16 = 0x40;
pub const BPF_MEM: u16 = 0x60;
pub const BPF_LEN: u16 = 0x80;
pub const BPF_MSH: u16 = 0xA0;

// BPF ALU operations
pub const BPF_ADD: u16 = 0x00;
pub const BPF_SUB: u16 = 0x10;
pub const BPF_MUL: u16 = 0x20;
pub const BPF_DIV: u16 = 0x30;
pub const BPF_OR: u16 = 0x40;
pub const BPF_AND: u16 = 0x50;
pub const BPF_LSH: u16 = 0x60;
pub const BPF_RSH: u16 = 0x70;
pub const BPF_NEG: u16 = 0x80;

// BPF source modifiers
pub const BPF_K: u16 = 0x00;  // Constant
pub const BPF_X: u16 = 0x08;  // Index register
pub const BPF_A: u16 = 0x10;  // Accumulator

// BPF jump operations
pub const BPF_JA: u16 = 0x00;
pub const BPF_JEQ: u16 = 0x10;
pub const BPF_JGT: u16 = 0x20;
pub const BPF_JGE: u16 = 0x30;
pub const BPF_JSET: u16 = 0x40;

/// Seccomp filter program
#[derive(Clone, Debug)]
pub struct SeccompFilter {
    /// BPF program
    pub program: Vec<BpfInsn>,
    /// Filter flags
    pub flags: SeccompFlags,
}

bitflags::bitflags! {
    /// Seccomp filter flags
    #[derive(Clone, Copy, Debug, Default)]
    pub struct SeccompFlags: u32 {
        /// Log allowed syscalls
        const LOG = 1 << 0;
        /// Disable speculative store bypass mitigation
        const SPEC_ALLOW = 1 << 2;
        /// Create a notify fd
        const NEW_LISTENER = 1 << 3;
        /// Synchronize all threads
        const TSYNC = 1 << 0;
        /// Don't require CAP_SYS_ADMIN
        const TSYNC_ESRCH = 1 << 4;
        /// Wait for notification response
        const WAIT_KILLABLE_RECV = 1 << 5;
    }
}

impl SeccompFilter {
    /// Create a new filter
    pub fn new(program: Vec<BpfInsn>, flags: SeccompFlags) -> Self {
        Self { program, flags }
    }

    /// Validate the BPF program
    pub fn validate(&self) -> Result<(), SeccompError> {
        if self.program.is_empty() {
            return Err(SeccompError::InvalidProgram);
        }
        if self.program.len() > BPF_MAXINSNS {
            return Err(SeccompError::ProgramTooLarge);
        }

        // Verify program terminates and all jumps are valid
        for (i, insn) in self.program.iter().enumerate() {
            let remaining = self.program.len() - i - 1;

            if (insn.code & 0x07) == BPF_RET as u16 {
                continue; // Return is always valid
            }

            if (insn.code & 0x07) == BPF_JMP as u16 {
                if insn.jt as usize > remaining || insn.jf as usize > remaining {
                    return Err(SeccompError::InvalidProgram);
                }
            }
        }

        Ok(())
    }

    /// Evaluate the filter for a syscall
    pub fn evaluate(&self, syscall_nr: u32, args: &[u64; 6]) -> SeccompAction {
        let data = SeccompData::new(syscall_nr, args);
        let data_bytes = unsafe {
            core::slice::from_raw_parts(
                &data as *const _ as *const u8,
                core::mem::size_of::<SeccompData>(),
            )
        };

        let mut a: u32 = 0;  // Accumulator
        let mut x: u32 = 0;  // Index register
        let mut mem = [0u32; 16];  // Scratch memory
        let mut pc: usize = 0;

        while pc < self.program.len() {
            let insn = &self.program[pc];
            let class = insn.code & 0x07;

            match class {
                0x00 => {
                    // BPF_LD
                    let size = insn.code & 0x18;
                    let mode = insn.code & 0xE0;

                    match mode {
                        0x20 => {
                            // BPF_ABS
                            let offset = insn.k as usize;
                            a = match size {
                                0x00 => {
                                    // BPF_W (32-bit)
                                    if offset + 4 <= data_bytes.len() {
                                        u32::from_ne_bytes([
                                            data_bytes[offset],
                                            data_bytes[offset + 1],
                                            data_bytes[offset + 2],
                                            data_bytes[offset + 3],
                                        ])
                                    } else {
                                        return SeccompAction::KillProcess;
                                    }
                                }
                                0x08 => {
                                    // BPF_H (16-bit)
                                    if offset + 2 <= data_bytes.len() {
                                        u16::from_ne_bytes([
                                            data_bytes[offset],
                                            data_bytes[offset + 1],
                                        ]) as u32
                                    } else {
                                        return SeccompAction::KillProcess;
                                    }
                                }
                                0x10 => {
                                    // BPF_B (8-bit)
                                    if offset < data_bytes.len() {
                                        data_bytes[offset] as u32
                                    } else {
                                        return SeccompAction::KillProcess;
                                    }
                                }
                                _ => return SeccompAction::KillProcess,
                            };
                        }
                        0x60 => {
                            // BPF_MEM
                            a = mem[insn.k as usize & 0xF];
                        }
                        _ => return SeccompAction::KillProcess,
                    }
                }
                0x01 => {
                    // BPF_LDX
                    let mode = insn.code & 0xE0;
                    match mode {
                        0x60 => {
                            // BPF_MEM
                            x = mem[insn.k as usize & 0xF];
                        }
                        0x00 => {
                            // BPF_IMM
                            x = insn.k;
                        }
                        _ => return SeccompAction::KillProcess,
                    }
                }
                0x02 => {
                    // BPF_ST
                    mem[insn.k as usize & 0xF] = a;
                }
                0x03 => {
                    // BPF_STX
                    mem[insn.k as usize & 0xF] = x;
                }
                0x04 => {
                    // BPF_ALU
                    let op = insn.code & 0xF0;
                    let src = if (insn.code & 0x08) != 0 { x } else { insn.k };

                    a = match op {
                        0x00 => a.wrapping_add(src),       // ADD
                        0x10 => a.wrapping_sub(src),       // SUB
                        0x20 => a.wrapping_mul(src),       // MUL
                        0x30 => {
                            if src == 0 {
                                return SeccompAction::KillProcess;
                            }
                            a / src
                        }                                    // DIV
                        0x40 => a | src,                   // OR
                        0x50 => a & src,                   // AND
                        0x60 => a << src,                  // LSH
                        0x70 => a >> src,                  // RSH
                        0x80 => (-(a as i32)) as u32,      // NEG
                        0x90 => a % src,                   // MOD
                        0xA0 => a ^ src,                   // XOR
                        _ => return SeccompAction::KillProcess,
                    };
                }
                0x05 => {
                    // BPF_JMP
                    let op = insn.code & 0xF0;
                    let src = if (insn.code & 0x08) != 0 { x } else { insn.k };

                    let result = match op {
                        0x00 => true,                        // JA (unconditional)
                        0x10 => a == src,                    // JEQ
                        0x20 => a > src,                     // JGT
                        0x30 => a >= src,                    // JGE
                        0x40 => (a & src) != 0,              // JSET
                        _ => return SeccompAction::KillProcess,
                    };

                    if op == 0x00 {
                        pc += insn.k as usize;
                    } else if result {
                        pc += insn.jt as usize;
                    } else {
                        pc += insn.jf as usize;
                    }
                }
                0x06 => {
                    // BPF_RET
                    let val = if (insn.code & 0x10) != 0 { a } else { insn.k };
                    return SeccompAction::from_ret(val);
                }
                0x07 => {
                    // BPF_MISC
                    if insn.code == 0x87 {
                        // TAX
                        x = a;
                    } else if insn.code == 0x97 {
                        // TXA
                        a = x;
                    }
                }
                _ => return SeccompAction::KillProcess,
            }

            pc += 1;
        }

        // Should not reach here (program should end with RET)
        SeccompAction::KillProcess
    }
}

/// Maximum BPF instructions
pub const BPF_MAXINSNS: usize = 4096;

/// Seccomp errors
#[derive(Clone, Debug)]
pub enum SeccompError {
    /// Invalid BPF program
    InvalidProgram,
    /// Program too large
    ProgramTooLarge,
    /// Permission denied
    PermissionDenied,
    /// Already in strict mode
    AlreadyStrict,
    /// Invalid operation
    InvalidOperation,
}

impl SeccompError {
    /// Convert to errno
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::InvalidProgram => -22,    // EINVAL
            Self::ProgramTooLarge => -7,    // E2BIG
            Self::PermissionDenied => -1,   // EPERM
            Self::AlreadyStrict => -16,     // EBUSY
            Self::InvalidOperation => -22,  // EINVAL
        }
    }
}

/// Seccomp operation for prctl/seccomp syscall
#[derive(Clone, Copy, Debug)]
pub enum SeccompOp {
    /// Set strict mode
    SetModeStrict,
    /// Set filter mode
    SetModeFilter,
    /// Get action available
    GetActionAvail,
    /// Get notif sizes
    GetNotifSizes,
}

/// Filter program passed to seccomp syscall
#[repr(C)]
pub struct SockFprog {
    /// Number of instructions
    pub len: u16,
    /// Pointer to BPF program
    pub filter: *const BpfInsn,
}

/// Helper to build common seccomp filters
pub mod builder {
    use super::*;

    /// Allow only specific syscalls
    pub fn allow_list(syscalls: &[u32]) -> SeccompFilter {
        let mut program = Vec::new();

        // Load syscall number
        program.push(BpfInsn::ld_w(0));  // Load nr at offset 0

        // Check architecture first
        // (skipped for simplicity)

        // Check each allowed syscall
        for (i, &nr) in syscalls.iter().enumerate() {
            let remaining = syscalls.len() - i;
            program.push(BpfInsn::jeq(nr, (remaining * 2) as u8, 0));
        }

        // Default: kill
        program.push(BpfInsn::ret(SeccompAction::KillProcess.to_ret()));

        // Allow
        program.push(BpfInsn::ret(SeccompAction::Allow.to_ret()));

        SeccompFilter::new(program, SeccompFlags::empty())
    }

    /// Block specific syscalls
    pub fn deny_list(syscalls: &[u32]) -> SeccompFilter {
        let mut program = Vec::new();

        // Load syscall number
        program.push(BpfInsn::ld_w(0));

        // Check each blocked syscall
        for &nr in syscalls {
            program.push(BpfInsn::jeq(nr, 0, 1));
            program.push(BpfInsn::ret(SeccompAction::KillProcess.to_ret()));
        }

        // Default: allow
        program.push(BpfInsn::ret(SeccompAction::Allow.to_ret()));

        SeccompFilter::new(program, SeccompFlags::empty())
    }

    /// Return errno for specific syscalls
    pub fn errno_list(syscalls: &[(u32, u16)]) -> SeccompFilter {
        let mut program = Vec::new();

        // Load syscall number
        program.push(BpfInsn::ld_w(0));

        // Check each syscall
        for &(nr, errno) in syscalls {
            program.push(BpfInsn::jeq(nr, 0, 1));
            program.push(BpfInsn::ret(SeccompAction::Errno(errno).to_ret()));
        }

        // Default: allow
        program.push(BpfInsn::ret(SeccompAction::Allow.to_ret()));

        SeccompFilter::new(program, SeccompFlags::empty())
    }
}

/// Seccomp notification (for SECCOMP_RET_USER_NOTIF)
pub mod notify {
    use super::*;

    /// Notification request
    #[repr(C)]
    pub struct SeccompNotif {
        /// Unique notification ID
        pub id: u64,
        /// Process ID
        pub pid: u32,
        /// Flags
        pub flags: u32,
        /// Syscall data
        pub data: SeccompData,
    }

    /// Notification response
    #[repr(C)]
    pub struct SeccompNotifResp {
        /// Notification ID
        pub id: u64,
        /// Return value
        pub val: i64,
        /// Error number
        pub error: i32,
        /// Flags
        pub flags: u32,
    }

    /// Notification sizes
    #[repr(C)]
    pub struct SeccompNotifSizes {
        pub seccomp_notif: u16,
        pub seccomp_notif_resp: u16,
        pub seccomp_data: u16,
    }

    /// Response flags
    pub const SECCOMP_USER_NOTIF_FLAG_CONTINUE: u32 = 1 << 0;
}

/// Initialize seccomp subsystem
pub fn init() {
    // Nothing to initialize - seccomp is per-process
}

/// System call entry point for seccomp
pub fn sys_seccomp(op: u32, flags: u32, args: *const u8) -> Result<i32, SeccompError> {
    match op {
        0 => {
            // SECCOMP_SET_MODE_STRICT
            // Would enable strict mode for current process
            Ok(0)
        }
        1 => {
            // SECCOMP_SET_MODE_FILTER
            // Would install filter from args
            let _ = (flags, args);
            Ok(0)
        }
        2 => {
            // SECCOMP_GET_ACTION_AVAIL
            // Check if action is available
            Ok(0)
        }
        3 => {
            // SECCOMP_GET_NOTIF_SIZES
            Ok(0)
        }
        _ => Err(SeccompError::InvalidOperation),
    }
}

/// Common syscall numbers for x86_64
pub mod syscalls {
    pub const SYS_READ: u32 = 0;
    pub const SYS_WRITE: u32 = 1;
    pub const SYS_OPEN: u32 = 2;
    pub const SYS_CLOSE: u32 = 3;
    pub const SYS_STAT: u32 = 4;
    pub const SYS_FSTAT: u32 = 5;
    pub const SYS_LSTAT: u32 = 6;
    pub const SYS_POLL: u32 = 7;
    pub const SYS_LSEEK: u32 = 8;
    pub const SYS_MMAP: u32 = 9;
    pub const SYS_MPROTECT: u32 = 10;
    pub const SYS_MUNMAP: u32 = 11;
    pub const SYS_BRK: u32 = 12;
    pub const SYS_RT_SIGACTION: u32 = 13;
    pub const SYS_RT_SIGPROCMASK: u32 = 14;
    pub const SYS_RT_SIGRETURN: u32 = 15;
    pub const SYS_IOCTL: u32 = 16;
    pub const SYS_PREAD64: u32 = 17;
    pub const SYS_PWRITE64: u32 = 18;
    pub const SYS_READV: u32 = 19;
    pub const SYS_WRITEV: u32 = 20;
    pub const SYS_ACCESS: u32 = 21;
    pub const SYS_PIPE: u32 = 22;
    pub const SYS_SELECT: u32 = 23;
    pub const SYS_SCHED_YIELD: u32 = 24;
    pub const SYS_MREMAP: u32 = 25;
    pub const SYS_MSYNC: u32 = 26;
    pub const SYS_MINCORE: u32 = 27;
    pub const SYS_MADVISE: u32 = 28;
    pub const SYS_DUP: u32 = 32;
    pub const SYS_DUP2: u32 = 33;
    pub const SYS_NANOSLEEP: u32 = 35;
    pub const SYS_GETPID: u32 = 39;
    pub const SYS_SOCKET: u32 = 41;
    pub const SYS_CONNECT: u32 = 42;
    pub const SYS_ACCEPT: u32 = 43;
    pub const SYS_SENDTO: u32 = 44;
    pub const SYS_RECVFROM: u32 = 45;
    pub const SYS_BIND: u32 = 49;
    pub const SYS_LISTEN: u32 = 50;
    pub const SYS_CLONE: u32 = 56;
    pub const SYS_FORK: u32 = 57;
    pub const SYS_VFORK: u32 = 58;
    pub const SYS_EXECVE: u32 = 59;
    pub const SYS_EXIT: u32 = 60;
    pub const SYS_WAIT4: u32 = 61;
    pub const SYS_KILL: u32 = 62;
    pub const SYS_FCNTL: u32 = 72;
    pub const SYS_FLOCK: u32 = 73;
    pub const SYS_FSYNC: u32 = 74;
    pub const SYS_GETCWD: u32 = 79;
    pub const SYS_CHDIR: u32 = 80;
    pub const SYS_MKDIR: u32 = 83;
    pub const SYS_RMDIR: u32 = 84;
    pub const SYS_CREAT: u32 = 85;
    pub const SYS_LINK: u32 = 86;
    pub const SYS_UNLINK: u32 = 87;
    pub const SYS_SYMLINK: u32 = 88;
    pub const SYS_READLINK: u32 = 89;
    pub const SYS_CHMOD: u32 = 90;
    pub const SYS_CHOWN: u32 = 92;
    pub const SYS_UMASK: u32 = 95;
    pub const SYS_GETUID: u32 = 102;
    pub const SYS_GETGID: u32 = 104;
    pub const SYS_SETUID: u32 = 105;
    pub const SYS_SETGID: u32 = 106;
    pub const SYS_GETEUID: u32 = 107;
    pub const SYS_GETEGID: u32 = 108;
    pub const SYS_GETPPID: u32 = 110;
    pub const SYS_GETPGRP: u32 = 111;
    pub const SYS_SETSID: u32 = 112;
    pub const SYS_SETREUID: u32 = 113;
    pub const SYS_SETREGID: u32 = 114;
    pub const SYS_GETGROUPS: u32 = 115;
    pub const SYS_SETGROUPS: u32 = 116;
    pub const SYS_CAPGET: u32 = 125;
    pub const SYS_CAPSET: u32 = 126;
    pub const SYS_SIGALTSTACK: u32 = 131;
    pub const SYS_PRCTL: u32 = 157;
    pub const SYS_ARCH_PRCTL: u32 = 158;
    pub const SYS_MOUNT: u32 = 165;
    pub const SYS_UMOUNT2: u32 = 166;
    pub const SYS_REBOOT: u32 = 169;
    pub const SYS_INIT_MODULE: u32 = 175;
    pub const SYS_DELETE_MODULE: u32 = 176;
    pub const SYS_QUOTACTL: u32 = 179;
    pub const SYS_GETTID: u32 = 186;
    pub const SYS_FUTEX: u32 = 202;
    pub const SYS_SET_TID_ADDRESS: u32 = 218;
    pub const SYS_EXIT_GROUP: u32 = 231;
    pub const SYS_OPENAT: u32 = 257;
    pub const SYS_MKDIRAT: u32 = 258;
    pub const SYS_FCHOWNAT: u32 = 260;
    pub const SYS_UNLINKAT: u32 = 263;
    pub const SYS_RENAMEAT: u32 = 264;
    pub const SYS_LINKAT: u32 = 265;
    pub const SYS_SYMLINKAT: u32 = 266;
    pub const SYS_READLINKAT: u32 = 267;
    pub const SYS_FCHMODAT: u32 = 268;
    pub const SYS_FACCESSAT: u32 = 269;
    pub const SYS_PSELECT6: u32 = 270;
    pub const SYS_PPOLL: u32 = 271;
    pub const SYS_SET_ROBUST_LIST: u32 = 273;
    pub const SYS_GET_ROBUST_LIST: u32 = 274;
    pub const SYS_SECCOMP: u32 = 317;
    pub const SYS_GETRANDOM: u32 = 318;
    pub const SYS_MEMFD_CREATE: u32 = 319;
    pub const SYS_COPY_FILE_RANGE: u32 = 326;
    pub const SYS_STATX: u32 = 332;
    pub const SYS_CLONE3: u32 = 435;
    pub const SYS_CLOSE_RANGE: u32 = 436;
    pub const SYS_OPENAT2: u32 = 437;
}
