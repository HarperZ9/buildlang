// ===============================================================================
// BUILDOS KERNEL - TIMERFD
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Timer File Descriptor (timerfd)
//!
//! Provides timer notifications via file descriptors.
//! Enables integration with poll/select/epoll for unified event handling.

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::sync::RwLock;

// =============================================================================
// CLOCK TYPES
// =============================================================================

/// Clock types for timerfd
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum ClockId {
    /// System-wide realtime clock
    Realtime = 0,
    /// Monotonic clock
    Monotonic = 1,
    /// Boot time (includes suspend)
    BoottimeAlarm = 9,
    /// Realtime alarm
    RealtimeAlarm = 8,
}

impl ClockId {
    pub fn from_i32(val: i32) -> Option<Self> {
        match val {
            0 => Some(Self::Realtime),
            1 => Some(Self::Monotonic),
            9 => Some(Self::BoottimeAlarm),
            8 => Some(Self::RealtimeAlarm),
            _ => None,
        }
    }
}

// =============================================================================
// TIMERFD FLAGS
// =============================================================================

/// Timerfd flags
pub mod flags {
    /// Non-blocking
    pub const TFD_NONBLOCK: u32 = 0o00004000;
    /// Close-on-exec
    pub const TFD_CLOEXEC: u32 = 0o02000000;
    /// Absolute time (for settime)
    pub const TFD_TIMER_ABSTIME: u32 = 1 << 0;
    /// Cancel on set (for realtime clock)
    pub const TFD_TIMER_CANCEL_ON_SET: u32 = 1 << 1;
}

// =============================================================================
// TIME STRUCTURES
// =============================================================================

/// Time specification
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Timespec {
    /// Seconds
    pub tv_sec: i64,
    /// Nanoseconds
    pub tv_nsec: i64,
}

impl Timespec {
    pub fn new(sec: i64, nsec: i64) -> Self {
        Self { tv_sec: sec, tv_nsec: nsec }
    }

    pub fn zero() -> Self {
        Self { tv_sec: 0, tv_nsec: 0 }
    }

    pub fn is_zero(&self) -> bool {
        self.tv_sec == 0 && self.tv_nsec == 0
    }

    /// Convert to nanoseconds
    pub fn to_nanos(&self) -> i64 {
        self.tv_sec * 1_000_000_000 + self.tv_nsec
    }

    /// Create from nanoseconds
    pub fn from_nanos(nanos: i64) -> Self {
        Self {
            tv_sec: nanos / 1_000_000_000,
            tv_nsec: nanos % 1_000_000_000,
        }
    }

    /// Add duration
    pub fn add(&self, other: &Timespec) -> Self {
        let mut nsec = self.tv_nsec + other.tv_nsec;
        let mut sec = self.tv_sec + other.tv_sec;

        if nsec >= 1_000_000_000 {
            sec += 1;
            nsec -= 1_000_000_000;
        }

        Self { tv_sec: sec, tv_nsec: nsec }
    }
}

/// Timer specification (initial expiration + interval)
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ItimerSpec {
    /// Timer interval (for repeating timers)
    pub it_interval: Timespec,
    /// Initial expiration
    pub it_value: Timespec,
}

impl ItimerSpec {
    pub fn new(value: Timespec, interval: Timespec) -> Self {
        Self {
            it_interval: interval,
            it_value: value,
        }
    }

    /// Check if timer is disarmed
    pub fn is_disarmed(&self) -> bool {
        self.it_value.is_zero()
    }

    /// Check if timer is repeating
    pub fn is_repeating(&self) -> bool {
        !self.it_interval.is_zero()
    }
}

// =============================================================================
// TIMERFD INSTANCE
// =============================================================================

/// Timerfd instance
pub struct Timerfd {
    /// File descriptor
    fd: i32,
    /// Clock type
    clock: ClockId,
    /// Flags
    flags: u32,
    /// Timer specification
    timer_spec: RwLock<ItimerSpec>,
    /// Expiration time (absolute, in clock ticks)
    expiration: AtomicU64,
    /// Number of expirations since last read
    expirations: AtomicU64,
    /// Is timer armed?
    armed: core::sync::atomic::AtomicBool,
}

impl Timerfd {
    /// Create new timerfd
    pub fn new(fd: i32, clock: ClockId, flags: u32) -> Self {
        Self {
            fd,
            clock,
            flags,
            timer_spec: RwLock::new(ItimerSpec::default()),
            expiration: AtomicU64::new(0),
            expirations: AtomicU64::new(0),
            armed: core::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Set timer
    pub fn settime(&self, flags: u32, new_value: &ItimerSpec, old_value: Option<&mut ItimerSpec>) -> Result<(), TimerfdError> {
        // Save old value if requested
        if let Some(old) = old_value {
            *old = self.gettime();
        }

        // Disarm if value is zero
        if new_value.is_disarmed() {
            self.armed.store(false, Ordering::Release);
            *self.timer_spec.write() = ItimerSpec::default();
            return Ok(());
        }

        let now = get_clock_time(self.clock);
        let expiration = if flags & flags::TFD_TIMER_ABSTIME != 0 {
            // Absolute time
            new_value.it_value.to_nanos() as u64
        } else {
            // Relative time
            now + new_value.it_value.to_nanos() as u64
        };

        self.expiration.store(expiration, Ordering::Release);
        *self.timer_spec.write() = *new_value;
        self.armed.store(true, Ordering::Release);

        crate::kprintln!("[TIMERFD] Set timer fd={} expiration={}ns interval={}ns",
            self.fd,
            new_value.it_value.to_nanos(),
            new_value.it_interval.to_nanos());

        Ok(())
    }

    /// Get timer value
    pub fn gettime(&self) -> ItimerSpec {
        if !self.armed.load(Ordering::Acquire) {
            return ItimerSpec::default();
        }

        let spec = self.timer_spec.read().clone();
        let now = get_clock_time(self.clock);
        let expiration = self.expiration.load(Ordering::Relaxed);

        // Calculate time until next expiration
        let remaining = if expiration > now {
            expiration - now
        } else {
            0
        };

        ItimerSpec {
            it_interval: spec.it_interval,
            it_value: Timespec::from_nanos(remaining as i64),
        }
    }

    /// Check and handle expiration
    pub fn check_expiration(&self) {
        if !self.armed.load(Ordering::Acquire) {
            return;
        }

        let now = get_clock_time(self.clock);
        let expiration = self.expiration.load(Ordering::Relaxed);

        if now >= expiration {
            let spec = self.timer_spec.read();

            if spec.is_repeating() {
                // Calculate number of expirations
                let interval = spec.it_interval.to_nanos() as u64;
                let elapsed = now - expiration;
                let count = 1 + elapsed / interval;

                self.expirations.fetch_add(count, Ordering::AcqRel);

                // Set next expiration
                let next = expiration + count * interval;
                self.expiration.store(next, Ordering::Release);
            } else {
                // One-shot timer
                self.expirations.fetch_add(1, Ordering::AcqRel);
                self.armed.store(false, Ordering::Release);
            }
        }
    }

    /// Read from timerfd
    pub fn read(&self) -> Result<u64, TimerfdError> {
        // Check for expirations
        self.check_expiration();

        let expirations = self.expirations.swap(0, Ordering::AcqRel);

        if expirations == 0 {
            if self.flags & flags::TFD_NONBLOCK != 0 {
                return Err(TimerfdError::WouldBlock);
            }
            // Would block
            return Ok(0);
        }

        Ok(expirations)
    }

    /// Check if readable (has expirations)
    pub fn is_readable(&self) -> bool {
        self.check_expiration();
        self.expirations.load(Ordering::Relaxed) > 0
    }
}

// =============================================================================
// TIMERFD ERROR
// =============================================================================

/// Timerfd error
#[derive(Clone, Debug)]
pub enum TimerfdError {
    /// Would block
    WouldBlock,
    /// Invalid file descriptor
    InvalidFd,
    /// Invalid clock
    InvalidClock,
    /// Invalid value
    InvalidValue,
}

impl TimerfdError {
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::WouldBlock => -11,  // EAGAIN
            Self::InvalidFd => -9,    // EBADF
            Self::InvalidClock => -22, // EINVAL
            Self::InvalidValue => -22, // EINVAL
        }
    }
}

// =============================================================================
// GLOBAL STATE
// =============================================================================

/// Global timerfd instances
static TIMERFDS: RwLock<BTreeMap<i32, Arc<Timerfd>>> = RwLock::new(BTreeMap::new());

/// Next timerfd ID
static NEXT_FD: AtomicU32 = AtomicU32::new(3000);

/// Get current clock time in nanoseconds
fn get_clock_time(_clock: ClockId) -> u64 {
    // Would read actual clock
    // For now, use a simple counter
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1000000, Ordering::Relaxed)
}

// =============================================================================
// SYSCALL IMPLEMENTATIONS
// =============================================================================

/// timerfd_create syscall
pub fn sys_timerfd_create(clockid: i32, flags: u32) -> Result<i32, TimerfdError> {
    let clock = ClockId::from_i32(clockid).ok_or(TimerfdError::InvalidClock)?;

    let fd = NEXT_FD.fetch_add(1, Ordering::Relaxed) as i32;
    let timerfd = Arc::new(Timerfd::new(fd, clock, flags));

    TIMERFDS.write().insert(fd, timerfd);

    crate::kprintln!("[TIMERFD] Created timerfd {} (clock: {:?}, flags: 0x{:x})",
        fd, clock, flags);

    Ok(fd)
}

/// timerfd_settime syscall
pub fn sys_timerfd_settime(
    fd: i32,
    flags: u32,
    new_value: &ItimerSpec,
    old_value: Option<&mut ItimerSpec>,
) -> Result<(), TimerfdError> {
    let timerfds = TIMERFDS.read();
    let timerfd = timerfds.get(&fd).ok_or(TimerfdError::InvalidFd)?;

    timerfd.settime(flags, new_value, old_value)
}

/// timerfd_gettime syscall
pub fn sys_timerfd_gettime(fd: i32) -> Result<ItimerSpec, TimerfdError> {
    let timerfds = TIMERFDS.read();
    let timerfd = timerfds.get(&fd).ok_or(TimerfdError::InvalidFd)?;

    Ok(timerfd.gettime())
}

/// Read from timerfd
pub fn read(fd: i32) -> Result<u64, TimerfdError> {
    let timerfds = TIMERFDS.read();
    let timerfd = timerfds.get(&fd).ok_or(TimerfdError::InvalidFd)?;
    timerfd.read()
}

/// Close timerfd
pub fn close(fd: i32) -> Result<(), TimerfdError> {
    TIMERFDS.write().remove(&fd)
        .map(|_| ())
        .ok_or(TimerfdError::InvalidFd)
}

/// Check if fd is timerfd
pub fn is_timerfd(fd: i32) -> bool {
    TIMERFDS.read().contains_key(&fd)
}

/// Check all timers for expiration (called from timer interrupt)
pub fn check_all_timers() {
    for timerfd in TIMERFDS.read().values() {
        timerfd.check_expiration();
    }
}

/// Initialize timerfd subsystem
pub fn init() {
    crate::kprintln!("[FS] Timerfd initialized");
}
