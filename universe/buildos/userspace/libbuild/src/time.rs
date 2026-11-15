// ===============================================================================
// TIME OPERATIONS
// ===============================================================================

use crate::syscall::*;

// =============================================================================
// CLOCK IDS
// =============================================================================

pub const CLOCK_REALTIME: i32 = 0;
pub const CLOCK_MONOTONIC: i32 = 1;
pub const CLOCK_PROCESS_CPUTIME_ID: i32 = 2;
pub const CLOCK_THREAD_CPUTIME_ID: i32 = 3;
pub const CLOCK_MONOTONIC_RAW: i32 = 4;
pub const CLOCK_REALTIME_COARSE: i32 = 5;
pub const CLOCK_MONOTONIC_COARSE: i32 = 6;
pub const CLOCK_BOOTTIME: i32 = 7;

// =============================================================================
// TIME STRUCTURES
// =============================================================================

/// Timespec structure for nanosecond precision
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Timespec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

impl Timespec {
    pub const fn new(secs: i64, nsecs: i64) -> Self {
        Self {
            tv_sec: secs,
            tv_nsec: nsecs,
        }
    }

    pub const fn from_secs(secs: u64) -> Self {
        Self {
            tv_sec: secs as i64,
            tv_nsec: 0,
        }
    }

    pub const fn from_millis(millis: u64) -> Self {
        Self {
            tv_sec: (millis / 1000) as i64,
            tv_nsec: ((millis % 1000) * 1_000_000) as i64,
        }
    }

    pub const fn from_micros(micros: u64) -> Self {
        Self {
            tv_sec: (micros / 1_000_000) as i64,
            tv_nsec: ((micros % 1_000_000) * 1_000) as i64,
        }
    }

    pub const fn from_nanos(nanos: u64) -> Self {
        Self {
            tv_sec: (nanos / 1_000_000_000) as i64,
            tv_nsec: (nanos % 1_000_000_000) as i64,
        }
    }

    pub const fn zero() -> Self {
        Self { tv_sec: 0, tv_nsec: 0 }
    }

    /// Convert to total milliseconds
    pub const fn as_millis(&self) -> u64 {
        (self.tv_sec as u64) * 1000 + (self.tv_nsec as u64) / 1_000_000
    }

    /// Convert to total microseconds
    pub const fn as_micros(&self) -> u64 {
        (self.tv_sec as u64) * 1_000_000 + (self.tv_nsec as u64) / 1_000
    }

    /// Convert to total nanoseconds
    pub const fn as_nanos(&self) -> u64 {
        (self.tv_sec as u64) * 1_000_000_000 + (self.tv_nsec as u64)
    }
}

impl core::ops::Add for Timespec {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        let mut nsec = self.tv_nsec + other.tv_nsec;
        let mut sec = self.tv_sec + other.tv_sec;

        if nsec >= 1_000_000_000 {
            nsec -= 1_000_000_000;
            sec += 1;
        }

        Self { tv_sec: sec, tv_nsec: nsec }
    }
}

impl core::ops::Sub for Timespec {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        let mut nsec = self.tv_nsec - other.tv_nsec;
        let mut sec = self.tv_sec - other.tv_sec;

        if nsec < 0 {
            nsec += 1_000_000_000;
            sec -= 1;
        }

        Self { tv_sec: sec, tv_nsec: nsec }
    }
}

// =============================================================================
// TIME FUNCTIONS
// =============================================================================

/// Sleep for specified duration
pub fn nanosleep(duration: &Timespec) -> i32 {
    unsafe {
        syscall2(SYS_NANOSLEEP, duration as *const Timespec as u64, 0) as i32
    }
}

/// Sleep for specified duration, returning remaining time on interrupt
pub fn nanosleep_rem(duration: &Timespec, remaining: &mut Timespec) -> i32 {
    unsafe {
        syscall2(
            SYS_NANOSLEEP,
            duration as *const Timespec as u64,
            remaining as *mut Timespec as u64,
        ) as i32
    }
}

/// Sleep for specified seconds
pub fn sleep(secs: u64) {
    let ts = Timespec::from_secs(secs);
    nanosleep(&ts);
}

/// Sleep for specified milliseconds
pub fn sleep_ms(millis: u64) {
    let ts = Timespec::from_millis(millis);
    nanosleep(&ts);
}

/// Sleep for specified microseconds
pub fn usleep(micros: u64) {
    let ts = Timespec::from_micros(micros);
    nanosleep(&ts);
}

/// Get current time from specified clock
pub fn clock_gettime(clock_id: i32, tp: &mut Timespec) -> i32 {
    unsafe {
        syscall2(SYS_CLOCK_GETTIME, clock_id as u64, tp as *mut Timespec as u64) as i32
    }
}

/// Get current real time
pub fn time_now() -> Timespec {
    let mut ts = Timespec::zero();
    clock_gettime(CLOCK_REALTIME, &mut ts);
    ts
}

/// Get monotonic time (for measuring durations)
pub fn monotonic_now() -> Timespec {
    let mut ts = Timespec::zero();
    clock_gettime(CLOCK_MONOTONIC, &mut ts);
    ts
}

/// Get system uptime
pub fn uptime() -> Timespec {
    let mut ts = Timespec::zero();
    clock_gettime(CLOCK_BOOTTIME, &mut ts);
    ts
}

// =============================================================================
// CONVENIENCE FUNCTIONS
// =============================================================================

/// Get Unix timestamp (seconds since epoch)
pub fn unix_timestamp() -> i64 {
    time_now().tv_sec
}

/// Measure elapsed time between two monotonic timestamps
pub fn elapsed(start: &Timespec, end: &Timespec) -> Timespec {
    *end - *start
}
