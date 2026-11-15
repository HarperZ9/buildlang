// ===============================================================================
// BUILDOS KERNEL - EVENTFD
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Event File Descriptor (eventfd)
//!
//! Provides a mechanism for event notification via file descriptors.
//! Used for:
//! - Inter-process/thread signaling
//! - Event loop integration
//! - Semaphore-like synchronization

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::sync::RwLock;

/// Maximum eventfd value before overflow
pub const EVENTFD_MAX: u64 = u64::MAX - 1;

// =============================================================================
// EVENTFD FLAGS
// =============================================================================

/// Eventfd flags
pub mod flags {
    /// Semaphore mode (read returns 1, decrements counter)
    pub const EFD_SEMAPHORE: u32 = 0o00000001;
    /// Close-on-exec
    pub const EFD_CLOEXEC: u32 = 0o02000000;
    /// Non-blocking
    pub const EFD_NONBLOCK: u32 = 0o00004000;
}

// =============================================================================
// EVENTFD INSTANCE
// =============================================================================

/// Eventfd instance
pub struct Eventfd {
    /// File descriptor
    fd: i32,
    /// Counter value
    counter: AtomicU64,
    /// Flags
    flags: u32,
    /// Wait queue (simplified - just track if there are waiters)
    has_readers: core::sync::atomic::AtomicBool,
    has_writers: core::sync::atomic::AtomicBool,
}

impl Eventfd {
    /// Create new eventfd
    pub fn new(fd: i32, initval: u32, flags: u32) -> Self {
        Self {
            fd,
            counter: AtomicU64::new(initval as u64),
            flags,
            has_readers: core::sync::atomic::AtomicBool::new(false),
            has_writers: core::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Read from eventfd
    pub fn read(&self) -> Result<u64, EventfdError> {
        loop {
            let current = self.counter.load(Ordering::Acquire);

            if current == 0 {
                if self.flags & flags::EFD_NONBLOCK != 0 {
                    return Err(EventfdError::WouldBlock);
                }
                // Would block and wait
                return Err(EventfdError::WouldBlock);
            }

            if self.flags & flags::EFD_SEMAPHORE != 0 {
                // Semaphore mode: decrement by 1, return 1
                if self.counter.compare_exchange(
                    current,
                    current - 1,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                ).is_ok() {
                    return Ok(1);
                }
            } else {
                // Normal mode: return counter, reset to 0
                if self.counter.compare_exchange(
                    current,
                    0,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                ).is_ok() {
                    return Ok(current);
                }
            }
            // CAS failed, retry
        }
    }

    /// Write to eventfd
    pub fn write(&self, value: u64) -> Result<(), EventfdError> {
        if value == u64::MAX {
            return Err(EventfdError::InvalidValue);
        }

        loop {
            let current = self.counter.load(Ordering::Acquire);

            // Check for overflow
            if current > EVENTFD_MAX - value {
                if self.flags & flags::EFD_NONBLOCK != 0 {
                    return Err(EventfdError::WouldBlock);
                }
                // Would block and wait for space
                return Err(EventfdError::WouldBlock);
            }

            if self.counter.compare_exchange(
                current,
                current + value,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ).is_ok() {
                return Ok(());
            }
            // CAS failed, retry
        }
    }

    /// Get current value (for polling)
    pub fn get_value(&self) -> u64 {
        self.counter.load(Ordering::Relaxed)
    }

    /// Check if readable (for poll/select)
    pub fn is_readable(&self) -> bool {
        self.counter.load(Ordering::Relaxed) > 0
    }

    /// Check if writable (for poll/select)
    pub fn is_writable(&self) -> bool {
        self.counter.load(Ordering::Relaxed) < EVENTFD_MAX
    }

    /// Signal the eventfd (increment by 1)
    pub fn signal(&self) -> Result<(), EventfdError> {
        self.write(1)
    }
}

// =============================================================================
// EVENTFD ERROR
// =============================================================================

/// Eventfd error
#[derive(Clone, Debug)]
pub enum EventfdError {
    /// Would block
    WouldBlock,
    /// Invalid value
    InvalidValue,
    /// Invalid file descriptor
    InvalidFd,
}

impl EventfdError {
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::WouldBlock => -11,  // EAGAIN
            Self::InvalidValue => -22, // EINVAL
            Self::InvalidFd => -9,    // EBADF
        }
    }
}

// =============================================================================
// GLOBAL STATE
// =============================================================================

/// Global eventfd instances
static EVENTFDS: RwLock<BTreeMap<i32, Arc<Eventfd>>> = RwLock::new(BTreeMap::new());

/// Next eventfd ID
static NEXT_FD: AtomicU32 = AtomicU32::new(1000);

// =============================================================================
// SYSCALL IMPLEMENTATIONS
// =============================================================================

/// eventfd syscall
pub fn sys_eventfd(initval: u32) -> Result<i32, EventfdError> {
    sys_eventfd2(initval, 0)
}

/// eventfd2 syscall
pub fn sys_eventfd2(initval: u32, flags: u32) -> Result<i32, EventfdError> {
    let fd = NEXT_FD.fetch_add(1, Ordering::Relaxed) as i32;
    let eventfd = Arc::new(Eventfd::new(fd, initval, flags));

    EVENTFDS.write().insert(fd, eventfd);

    crate::kprintln!("[EVENTFD] Created eventfd {} (initval: {}, flags: 0x{:x})",
        fd, initval, flags);

    Ok(fd)
}

/// Read from eventfd
pub fn read(fd: i32) -> Result<u64, EventfdError> {
    let eventfds = EVENTFDS.read();
    let eventfd = eventfds.get(&fd).ok_or(EventfdError::InvalidFd)?;
    eventfd.read()
}

/// Write to eventfd
pub fn write(fd: i32, value: u64) -> Result<(), EventfdError> {
    let eventfds = EVENTFDS.read();
    let eventfd = eventfds.get(&fd).ok_or(EventfdError::InvalidFd)?;
    eventfd.write(value)
}

/// Close eventfd
pub fn close(fd: i32) -> Result<(), EventfdError> {
    EVENTFDS.write().remove(&fd)
        .map(|_| ())
        .ok_or(EventfdError::InvalidFd)
}

/// Check if fd is eventfd
pub fn is_eventfd(fd: i32) -> bool {
    EVENTFDS.read().contains_key(&fd)
}

/// Initialize eventfd subsystem
pub fn init() {
    crate::kprintln!("[FS] Eventfd initialized");
}
