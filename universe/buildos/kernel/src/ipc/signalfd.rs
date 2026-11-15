// ===============================================================================
// BUILDOS KERNEL - SIGNALFD
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Signal File Descriptor Implementation
//!
//! Provides signal delivery via file descriptors.

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU64, AtomicU32, AtomicBool, Ordering};

use super::IpcError;
use crate::sync::Mutex;

// =============================================================================
// CONSTANTS
// =============================================================================

/// Signalfd flags
pub const SFD_CLOEXEC: u32 = 0o2000000;
pub const SFD_NONBLOCK: u32 = 0o4000;

/// Maximum queued signals
pub const MAX_QUEUED_SIGNALS: usize = 64;

/// Signal numbers (POSIX)
pub const SIGHUP: u32 = 1;
pub const SIGINT: u32 = 2;
pub const SIGQUIT: u32 = 3;
pub const SIGILL: u32 = 4;
pub const SIGTRAP: u32 = 5;
pub const SIGABRT: u32 = 6;
pub const SIGBUS: u32 = 7;
pub const SIGFPE: u32 = 8;
pub const SIGKILL: u32 = 9;
pub const SIGUSR1: u32 = 10;
pub const SIGSEGV: u32 = 11;
pub const SIGUSR2: u32 = 12;
pub const SIGPIPE: u32 = 13;
pub const SIGALRM: u32 = 14;
pub const SIGTERM: u32 = 15;
pub const SIGCHLD: u32 = 17;
pub const SIGCONT: u32 = 18;
pub const SIGSTOP: u32 = 19;
pub const SIGTSTP: u32 = 20;
pub const SIGTTIN: u32 = 21;
pub const SIGTTOU: u32 = 22;
pub const SIGURG: u32 = 23;
pub const SIGXCPU: u32 = 24;
pub const SIGXFSZ: u32 = 25;
pub const SIGVTALRM: u32 = 26;
pub const SIGPROF: u32 = 27;
pub const SIGWINCH: u32 = 28;
pub const SIGIO: u32 = 29;
pub const SIGPWR: u32 = 30;
pub const SIGSYS: u32 = 31;

/// Real-time signal range
pub const SIGRTMIN: u32 = 32;
pub const SIGRTMAX: u32 = 64;

// =============================================================================
// SIGNAL INFO
// =============================================================================

/// Signal information structure
#[derive(Clone, Debug)]
#[repr(C)]
pub struct SignalfdSiginfo {
    /// Signal number
    pub ssi_signo: u32,
    /// Error number
    pub ssi_errno: i32,
    /// Signal code
    pub ssi_code: i32,
    /// Sending process ID
    pub ssi_pid: u32,
    /// Sending user ID
    pub ssi_uid: u32,
    /// File descriptor (SIGIO)
    pub ssi_fd: i32,
    /// Timer ID (SIGALRM, etc)
    pub ssi_tid: u32,
    /// Band event (SIGIO)
    pub ssi_band: u32,
    /// Timer overrun
    pub ssi_overrun: u32,
    /// Trap number
    pub ssi_trapno: u32,
    /// Exit status (SIGCHLD)
    pub ssi_status: i32,
    /// Integer sent with signal
    pub ssi_int: i32,
    /// Pointer sent with signal
    pub ssi_ptr: u64,
    /// User time consumed (SIGCHLD)
    pub ssi_utime: u64,
    /// System time consumed (SIGCHLD)
    pub ssi_stime: u64,
    /// Faulting address
    pub ssi_addr: u64,
    /// Least significant bit of address
    pub ssi_addr_lsb: u16,
    /// Padding
    _pad: [u8; 46],
}

impl Default for SignalfdSiginfo {
    fn default() -> Self {
        Self {
            ssi_signo: 0,
            ssi_errno: 0,
            ssi_code: 0,
            ssi_pid: 0,
            ssi_uid: 0,
            ssi_fd: 0,
            ssi_tid: 0,
            ssi_band: 0,
            ssi_overrun: 0,
            ssi_trapno: 0,
            ssi_status: 0,
            ssi_int: 0,
            ssi_ptr: 0,
            ssi_utime: 0,
            ssi_stime: 0,
            ssi_addr: 0,
            ssi_addr_lsb: 0,
            _pad: [0; 46],
        }
    }
}

impl SignalfdSiginfo {
    /// Size of structure
    pub const SIZE: usize = 128;

    /// Create from signal number
    pub fn from_signo(signo: u32) -> Self {
        Self {
            ssi_signo: signo,
            ..Default::default()
        }
    }
}

// =============================================================================
// SIGNAL MASK
// =============================================================================

/// Signal mask (64 bits for signals 1-64)
#[derive(Clone, Copy, Default)]
pub struct SigSet(u64);

impl SigSet {
    /// Empty set
    pub const fn empty() -> Self {
        SigSet(0)
    }

    /// Full set
    pub const fn full() -> Self {
        SigSet(!0)
    }

    /// Add signal
    pub fn add(&mut self, sig: u32) {
        if sig > 0 && sig <= 64 {
            self.0 |= 1 << (sig - 1);
        }
    }

    /// Remove signal
    pub fn remove(&mut self, sig: u32) {
        if sig > 0 && sig <= 64 {
            self.0 &= !(1 << (sig - 1));
        }
    }

    /// Check if signal is in set
    pub fn contains(&self, sig: u32) -> bool {
        if sig > 0 && sig <= 64 {
            (self.0 & (1 << (sig - 1))) != 0
        } else {
            false
        }
    }

    /// From raw bits
    pub fn from_bits(bits: u64) -> Self {
        SigSet(bits)
    }

    /// To raw bits
    pub fn to_bits(&self) -> u64 {
        self.0
    }

    /// Union
    pub fn union(&self, other: &Self) -> Self {
        SigSet(self.0 | other.0)
    }

    /// Intersection
    pub fn intersect(&self, other: &Self) -> Self {
        SigSet(self.0 & other.0)
    }
}

// =============================================================================
// SIGNALFD
// =============================================================================

/// Signal file descriptor
pub struct Signalfd {
    /// Signal mask
    mask: AtomicU64,
    /// Queued signals
    queue: Mutex<VecDeque<SignalfdSiginfo>>,
    /// Non-blocking
    nonblock: AtomicBool,
}

impl Signalfd {
    /// Create new signalfd
    pub fn new(mask: u64, flags: u32) -> Arc<Self> {
        Arc::new(Self {
            mask: AtomicU64::new(mask),
            queue: Mutex::new(VecDeque::with_capacity(MAX_QUEUED_SIGNALS)),
            nonblock: AtomicBool::new((flags & SFD_NONBLOCK) != 0),
        })
    }

    /// Set signal mask
    pub fn set_mask(&self, mask: u64) {
        self.mask.store(mask, Ordering::Release);
    }

    /// Get signal mask
    pub fn get_mask(&self) -> SigSet {
        SigSet::from_bits(self.mask.load(Ordering::Acquire))
    }

    /// Queue a signal
    pub fn queue_signal(&self, info: SignalfdSiginfo) {
        let mask = SigSet::from_bits(self.mask.load(Ordering::Acquire));

        if mask.contains(info.ssi_signo) {
            let mut queue = self.queue.lock();
            if queue.len() < MAX_QUEUED_SIGNALS {
                queue.push_back(info);
            }
        }
    }

    /// Read signal info
    pub fn read(&self) -> Result<SignalfdSiginfo, IpcError> {
        loop {
            {
                let mut queue = self.queue.lock();
                if let Some(info) = queue.pop_front() {
                    return Ok(info);
                }
            }

            if self.nonblock.load(Ordering::Relaxed) {
                return Err(IpcError::WouldBlock);
            }

            // Would block waiting for signal
            core::hint::spin_loop();
        }
    }

    /// Read multiple signals
    pub fn read_many(&self, buf: &mut [SignalfdSiginfo]) -> Result<usize, IpcError> {
        let mut count = 0;

        loop {
            {
                let mut queue = self.queue.lock();

                while count < buf.len() {
                    if let Some(info) = queue.pop_front() {
                        buf[count] = info;
                        count += 1;
                    } else {
                        break;
                    }
                }
            }

            if count > 0 {
                return Ok(count);
            }

            if self.nonblock.load(Ordering::Relaxed) {
                return Err(IpcError::WouldBlock);
            }

            // Would block waiting for signal
            core::hint::spin_loop();
        }
    }

    /// Poll for read
    pub fn poll_read(&self) -> bool {
        !self.queue.lock().is_empty()
    }

    /// Set non-blocking
    pub fn set_nonblock(&self, nonblock: bool) {
        self.nonblock.store(nonblock, Ordering::Relaxed);
    }

    /// Pending signals count
    pub fn pending(&self) -> usize {
        self.queue.lock().len()
    }
}

// =============================================================================
// SYSTEM CALLS
// =============================================================================

/// signalfd - create/update signalfd
pub fn signalfd(fd: i32, mask: u64, flags: u32) -> Result<i32, IpcError> {
    if fd == -1 {
        // Create new signalfd
        let _sfd = Signalfd::new(mask, flags);
        let new_fd = allocate_fd()?;
        Ok(new_fd)
    } else {
        // Update existing signalfd
        // Would look up fd and update mask
        Ok(fd)
    }
}

/// Initialize signalfd subsystem
pub fn init() {
    crate::kprintln!("[IPC/SIGNALFD] Signalfd subsystem initialized");
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

fn allocate_fd() -> Result<i32, IpcError> {
    static NEXT_FD: AtomicU32 = AtomicU32::new(200);
    Ok(NEXT_FD.fetch_add(1, Ordering::Relaxed) as i32)
}

// =============================================================================
// SIGNAL HELPERS
// =============================================================================

/// Get signal name
pub fn signame(sig: u32) -> &'static str {
    match sig {
        SIGHUP => "SIGHUP",
        SIGINT => "SIGINT",
        SIGQUIT => "SIGQUIT",
        SIGILL => "SIGILL",
        SIGTRAP => "SIGTRAP",
        SIGABRT => "SIGABRT",
        SIGBUS => "SIGBUS",
        SIGFPE => "SIGFPE",
        SIGKILL => "SIGKILL",
        SIGUSR1 => "SIGUSR1",
        SIGSEGV => "SIGSEGV",
        SIGUSR2 => "SIGUSR2",
        SIGPIPE => "SIGPIPE",
        SIGALRM => "SIGALRM",
        SIGTERM => "SIGTERM",
        SIGCHLD => "SIGCHLD",
        SIGCONT => "SIGCONT",
        SIGSTOP => "SIGSTOP",
        SIGTSTP => "SIGTSTP",
        SIGTTIN => "SIGTTIN",
        SIGTTOU => "SIGTTOU",
        SIGURG => "SIGURG",
        SIGXCPU => "SIGXCPU",
        SIGXFSZ => "SIGXFSZ",
        SIGVTALRM => "SIGVTALRM",
        SIGPROF => "SIGPROF",
        SIGWINCH => "SIGWINCH",
        SIGIO => "SIGIO",
        SIGPWR => "SIGPWR",
        SIGSYS => "SIGSYS",
        n if n >= SIGRTMIN && n <= SIGRTMAX => "SIGRT",
        _ => "UNKNOWN",
    }
}

/// Check if signal is real-time
pub fn is_rt_signal(sig: u32) -> bool {
    sig >= SIGRTMIN && sig <= SIGRTMAX
}

/// Check if signal is fatal by default
pub fn is_fatal(sig: u32) -> bool {
    matches!(sig,
        SIGKILL | SIGTERM | SIGINT | SIGQUIT | SIGILL | SIGTRAP |
        SIGABRT | SIGBUS | SIGFPE | SIGSEGV | SIGPIPE | SIGXCPU |
        SIGXFSZ | SIGSYS
    )
}

/// Check if signal can be caught
pub fn is_catchable(sig: u32) -> bool {
    sig != SIGKILL && sig != SIGSTOP
}
