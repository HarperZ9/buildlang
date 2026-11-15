// ===============================================================================
// BUILDOS KERNEL - SIGNALFD
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Signal File Descriptor (signalfd)
//!
//! Allows receiving signals via file descriptor instead of signal handlers.
//! Enables integration with poll/select/epoll for unified event handling.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::sync::RwLock;

/// Maximum queued signals per signalfd
pub const SIGNALFD_MAX_QUEUE: usize = 1024;

// =============================================================================
// SIGNALFD FLAGS
// =============================================================================

/// Signalfd flags
pub mod flags {
    /// Non-blocking
    pub const SFD_NONBLOCK: u32 = 0o00004000;
    /// Close-on-exec
    pub const SFD_CLOEXEC: u32 = 0o02000000;
}

// =============================================================================
// SIGNAL SET
// =============================================================================

/// Signal set (bitmask for 64 signals)
#[derive(Clone, Copy, Default)]
pub struct SigSet {
    bits: u64,
}

impl SigSet {
    /// Create empty signal set
    pub fn new() -> Self {
        Self { bits: 0 }
    }

    /// Create signal set with all signals
    pub fn full() -> Self {
        Self { bits: !0 }
    }

    /// Add signal to set
    pub fn add(&mut self, signo: i32) {
        if signo >= 1 && signo <= 64 {
            self.bits |= 1 << (signo - 1);
        }
    }

    /// Remove signal from set
    pub fn del(&mut self, signo: i32) {
        if signo >= 1 && signo <= 64 {
            self.bits &= !(1 << (signo - 1));
        }
    }

    /// Check if signal is in set
    pub fn contains(&self, signo: i32) -> bool {
        if signo >= 1 && signo <= 64 {
            (self.bits & (1 << (signo - 1))) != 0
        } else {
            false
        }
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.bits == 0
    }

    /// Get raw bits
    pub fn bits(&self) -> u64 {
        self.bits
    }

    /// Create from raw bits
    pub fn from_bits(bits: u64) -> Self {
        Self { bits }
    }
}

// =============================================================================
// SIGNAL INFO
// =============================================================================

/// Signal information structure
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SignalfdSiginfo {
    /// Signal number
    pub ssi_signo: u32,
    /// Error number
    pub ssi_errno: i32,
    /// Signal code
    pub ssi_code: i32,
    /// PID of sender
    pub ssi_pid: u32,
    /// UID of sender
    pub ssi_uid: u32,
    /// File descriptor (for SIGIO)
    pub ssi_fd: i32,
    /// Kernel timer ID
    pub ssi_tid: u32,
    /// Band event
    pub ssi_band: u32,
    /// POSIX timer overrun count
    pub ssi_overrun: u32,
    /// Trap number
    pub ssi_trapno: u32,
    /// Exit status or signal
    pub ssi_status: i32,
    /// Integer sent with signal
    pub ssi_int: i32,
    /// Pointer sent with signal
    pub ssi_ptr: u64,
    /// User CPU time consumed
    pub ssi_utime: u64,
    /// System CPU time consumed
    pub ssi_stime: u64,
    /// Address that generated signal
    pub ssi_addr: u64,
    /// LSB of address
    pub ssi_addr_lsb: u16,
    /// Padding
    _pad: [u8; 46],
}

impl SignalfdSiginfo {
    /// Size of structure
    pub const SIZE: usize = 128;

    /// Create new signal info
    pub fn new(signo: u32, pid: u32, uid: u32) -> Self {
        let info = Self {
            ssi_signo: signo,
            ssi_errno: 0,
            ssi_code: 0,
            ssi_pid: pid,
            ssi_uid: uid,
            ssi_fd: -1,
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
        };
        info
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        unsafe { core::mem::transmute_copy(self) }
    }
}

// =============================================================================
// SIGNALFD INSTANCE
// =============================================================================

/// Signalfd instance
pub struct Signalfd {
    /// File descriptor
    fd: i32,
    /// Signal mask
    mask: RwLock<SigSet>,
    /// Queued signals
    queue: RwLock<VecDeque<SignalfdSiginfo>>,
    /// Flags
    flags: u32,
}

impl Signalfd {
    /// Create new signalfd
    pub fn new(fd: i32, mask: SigSet, flags: u32) -> Self {
        Self {
            fd,
            mask: RwLock::new(mask),
            queue: RwLock::new(VecDeque::new()),
            flags,
        }
    }

    /// Update signal mask
    pub fn set_mask(&self, mask: SigSet) {
        *self.mask.write() = mask;
    }

    /// Get signal mask
    pub fn get_mask(&self) -> SigSet {
        *self.mask.read()
    }

    /// Queue signal
    pub fn queue_signal(&self, info: SignalfdSiginfo) -> bool {
        let mask = self.mask.read();
        if !mask.contains(info.ssi_signo as i32) {
            return false;
        }
        drop(mask);

        let mut queue = self.queue.write();
        if queue.len() >= SIGNALFD_MAX_QUEUE {
            return false;
        }

        queue.push_back(info);
        true
    }

    /// Read signals
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, SignalfdError> {
        let mut queue = self.queue.write();

        if queue.is_empty() {
            if self.flags & flags::SFD_NONBLOCK != 0 {
                return Err(SignalfdError::WouldBlock);
            }
            // Would block
            return Ok(0);
        }

        let mut written = 0;
        while let Some(info) = queue.front() {
            if written + SignalfdSiginfo::SIZE > buf.len() {
                break;
            }

            let bytes = info.to_bytes();
            buf[written..written + SignalfdSiginfo::SIZE].copy_from_slice(&bytes);
            written += SignalfdSiginfo::SIZE;
            queue.pop_front();
        }

        Ok(written)
    }

    /// Check if readable
    pub fn is_readable(&self) -> bool {
        !self.queue.read().is_empty()
    }
}

// =============================================================================
// SIGNALFD ERROR
// =============================================================================

/// Signalfd error
#[derive(Clone, Debug)]
pub enum SignalfdError {
    /// Would block
    WouldBlock,
    /// Invalid file descriptor
    InvalidFd,
    /// Invalid mask
    InvalidMask,
}

impl SignalfdError {
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::WouldBlock => -11,  // EAGAIN
            Self::InvalidFd => -9,    // EBADF
            Self::InvalidMask => -22, // EINVAL
        }
    }
}

// =============================================================================
// GLOBAL STATE
// =============================================================================

/// Global signalfd instances
static SIGNALFDS: RwLock<BTreeMap<i32, Arc<Signalfd>>> = RwLock::new(BTreeMap::new());

/// Process signal mask (for forwarding to signalfd)
static PROCESS_SIGNALFDS: RwLock<BTreeMap<u32, Vec<i32>>> = RwLock::new(BTreeMap::new());

/// Next signalfd ID
static NEXT_FD: AtomicU32 = AtomicU32::new(2000);

// =============================================================================
// SYSCALL IMPLEMENTATIONS
// =============================================================================

/// signalfd syscall
pub fn sys_signalfd(fd: i32, mask: &SigSet, flags: u32) -> Result<i32, SignalfdError> {
    if fd == -1 {
        // Create new signalfd
        let new_fd = NEXT_FD.fetch_add(1, Ordering::Relaxed) as i32;
        let signalfd = Arc::new(Signalfd::new(new_fd, *mask, flags));

        SIGNALFDS.write().insert(new_fd, signalfd);

        crate::kprintln!("[SIGNALFD] Created signalfd {} (mask: 0x{:x})", new_fd, mask.bits());

        Ok(new_fd)
    } else {
        // Update existing signalfd
        let signalfds = SIGNALFDS.read();
        let signalfd = signalfds.get(&fd).ok_or(SignalfdError::InvalidFd)?;

        signalfd.set_mask(*mask);

        crate::kprintln!("[SIGNALFD] Updated signalfd {} (mask: 0x{:x})", fd, mask.bits());

        Ok(fd)
    }
}

/// signalfd4 syscall
pub fn sys_signalfd4(fd: i32, mask: &SigSet, mask_size: usize, flags: u32) -> Result<i32, SignalfdError> {
    // mask_size should be sizeof(sigset_t), typically 8 bytes
    if mask_size != 8 {
        return Err(SignalfdError::InvalidMask);
    }

    sys_signalfd(fd, mask, flags)
}

/// Read from signalfd
pub fn read(fd: i32, buf: &mut [u8]) -> Result<usize, SignalfdError> {
    let signalfds = SIGNALFDS.read();
    let signalfd = signalfds.get(&fd).ok_or(SignalfdError::InvalidFd)?;
    signalfd.read(buf)
}

/// Close signalfd
pub fn close(fd: i32) -> Result<(), SignalfdError> {
    SIGNALFDS.write().remove(&fd)
        .map(|_| ())
        .ok_or(SignalfdError::InvalidFd)
}

/// Check if fd is signalfd
pub fn is_signalfd(fd: i32) -> bool {
    SIGNALFDS.read().contains_key(&fd)
}

/// Deliver signal to signalfd (called from signal delivery code)
pub fn deliver_signal(signo: i32, pid: u32, uid: u32) {
    let info = SignalfdSiginfo::new(signo as u32, pid, uid);

    let signalfds = SIGNALFDS.read();
    for signalfd in signalfds.values() {
        signalfd.queue_signal(info);
    }
}

/// Initialize signalfd subsystem
pub fn init() {
    crate::kprintln!("[FS] Signalfd initialized");
}
