// ===============================================================================
// BUILDOS KERNEL - TIME MANAGEMENT
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Time and clock management.

#![allow(dead_code)]

use core::sync::atomic::{AtomicU64, Ordering};

/// Nanoseconds per second
pub const NANOS_PER_SEC: u64 = 1_000_000_000;

/// Nanoseconds per millisecond
pub const NANOS_PER_MILLI: u64 = 1_000_000;

/// Nanoseconds per microsecond
pub const NANOS_PER_MICRO: u64 = 1_000;

/// System boot time (nanoseconds since epoch)
static BOOT_TIME: AtomicU64 = AtomicU64::new(0);

/// Monotonic time counter (nanoseconds since boot)
static MONOTONIC_NS: AtomicU64 = AtomicU64::new(0);

/// Real-time clock offset
static RTC_OFFSET: AtomicU64 = AtomicU64::new(0);

/// TSC frequency in Hz
static TSC_FREQ: AtomicU64 = AtomicU64::new(0);

/// Initialize time subsystem
pub fn init() {
    // Read TSC frequency from CPUID or calibrate
    let freq = calibrate_tsc();
    TSC_FREQ.store(freq, Ordering::Release);

    // Read RTC for boot time
    let rtc_time = read_rtc();
    BOOT_TIME.store(rtc_time, Ordering::Release);
}

/// Get current monotonic time in nanoseconds
pub fn now_ns() -> u64 {
    let tsc_freq = TSC_FREQ.load(Ordering::Acquire);
    if tsc_freq > 0 {
        // Use TSC for high precision
        let tsc = read_tsc();
        (tsc * NANOS_PER_SEC) / tsc_freq
    } else {
        // Fallback to stored monotonic time
        MONOTONIC_NS.load(Ordering::Acquire)
    }
}

/// Get current monotonic time in milliseconds
pub fn now_ms() -> u64 {
    now_ns() / NANOS_PER_MILLI
}

/// Get current monotonic time in seconds
pub fn now_secs() -> u64 {
    now_ns() / NANOS_PER_SEC
}

/// Get current time in nanoseconds (alias for now_ns)
pub fn current_time_ns() -> u64 {
    now_ns()
}

/// Get monotonic time in nanoseconds (alias for now_ns)
pub fn monotonic_ns() -> u64 {
    now_ns()
}

/// Get monotonic time in nanoseconds (alias for now_ns)
pub fn monotonic_nanos() -> u64 {
    now_ns()
}

/// Get current real time (nanoseconds since Unix epoch)
pub fn realtime_ns() -> u64 {
    BOOT_TIME.load(Ordering::Acquire) + now_ns() + RTC_OFFSET.load(Ordering::Acquire)
}

/// Get current real time as Unix timestamp (seconds)
pub fn unix_timestamp() -> u64 {
    realtime_ns() / NANOS_PER_SEC
}

/// Update monotonic time (called from timer interrupt)
pub fn tick(nanos: u64) {
    MONOTONIC_NS.fetch_add(nanos, Ordering::AcqRel);
}

/// Read TSC (Time Stamp Counter)
#[inline]
pub fn read_tsc() -> u64 {
    unsafe {
        let lo: u32;
        let hi: u32;
        core::arch::asm!(
            "rdtsc",
            out("eax") lo,
            out("edx") hi,
            options(nostack, nomem)
        );
        ((hi as u64) << 32) | (lo as u64)
    }
}

/// Read TSC with serialization (RDTSCP)
#[inline]
pub fn read_tsc_serialized() -> u64 {
    unsafe {
        let lo: u32;
        let hi: u32;
        let _aux: u32;
        core::arch::asm!(
            "rdtscp",
            out("eax") lo,
            out("edx") hi,
            out("ecx") _aux,
            options(nostack, nomem)
        );
        ((hi as u64) << 32) | (lo as u64)
    }
}

/// Calibrate TSC frequency
fn calibrate_tsc() -> u64 {
    // Try to get frequency from CPUID first
    if let Some(freq) = cpuid_tsc_freq() {
        return freq;
    }

    // Fallback: calibrate against PIT
    calibrate_against_pit()
}

/// Get TSC frequency from CPUID
fn cpuid_tsc_freq() -> Option<u64> {
    // CPUID leaf 0x15 contains TSC frequency info
    let cpuid_result = cpuid(0x15);

    if cpuid_result.eax == 0 || cpuid_result.ebx == 0 {
        return None;
    }

    // Nominal frequency = ECX * EBX / EAX
    if cpuid_result.ecx != 0 {
        Some((cpuid_result.ecx as u64 * cpuid_result.ebx as u64) / cpuid_result.eax as u64)
    } else {
        // Try leaf 0x16 for processor frequency
        let cpuid_16 = cpuid(0x16);
        if cpuid_16.eax != 0 {
            Some(cpuid_16.eax as u64 * 1_000_000) // MHz to Hz
        } else {
            None
        }
    }
}

/// CPUID result
struct CpuidResult {
    eax: u32,
    ebx: u32,
    ecx: u32,
    #[allow(dead_code)]
    edx: u32,
}

/// Execute CPUID instruction
fn cpuid(leaf: u32) -> CpuidResult {
    let eax: u32;
    let ebx: u32;
    let ecx: u32;
    let edx: u32;

    unsafe {
        core::arch::asm!(
            "push rbx",
            "cpuid",
            "mov {ebx_out:e}, ebx",
            "pop rbx",
            inout("eax") leaf => eax,
            ebx_out = out(reg) ebx,
            out("ecx") ecx,
            out("edx") edx,
            options(nomem)
        );
    }

    CpuidResult { eax, ebx, ecx, edx }
}

/// Calibrate TSC against PIT
fn calibrate_against_pit() -> u64 {
    // Default to 2.4 GHz if calibration not possible
    // In a real implementation, this would calibrate against PIT or HPET
    2_400_000_000
}

/// Read RTC time (seconds since epoch)
fn read_rtc() -> u64 {
    // Read CMOS RTC registers
    let second = cmos_read(0x00) as u64;
    let minute = cmos_read(0x02) as u64;
    let hour = cmos_read(0x04) as u64;
    let day = cmos_read(0x07) as u64;
    let month = cmos_read(0x08) as u64;
    let year = cmos_read(0x09) as u64;
    let century = cmos_read(0x32) as u64;

    // Convert BCD if needed
    let register_b = cmos_read(0x0B);
    let (second, minute, hour, day, month, year, century) = if register_b & 0x04 == 0 {
        // BCD mode
        (
            bcd_to_binary(second as u8) as u64,
            bcd_to_binary(minute as u8) as u64,
            bcd_to_binary(hour as u8) as u64,
            bcd_to_binary(day as u8) as u64,
            bcd_to_binary(month as u8) as u64,
            bcd_to_binary(year as u8) as u64,
            bcd_to_binary(century as u8) as u64,
        )
    } else {
        (second, minute, hour, day, month, year, century)
    };

    // Calculate full year
    let full_year = century * 100 + year;

    // Convert to Unix timestamp (simplified)
    let days_since_epoch = days_from_civil(full_year as i32, month as u32, day as u32);
    let seconds_since_epoch = (days_since_epoch as u64 * 86400) + (hour * 3600) + (minute * 60) + second;

    seconds_since_epoch * NANOS_PER_SEC
}

/// Read CMOS register
fn cmos_read(register: u8) -> u8 {
    unsafe {
        // Select register
        core::arch::asm!(
            "out 0x70, al",
            in("al") register,
            options(nostack, nomem)
        );

        // Read value
        let value: u8;
        core::arch::asm!(
            "in al, 0x71",
            out("al") value,
            options(nostack, nomem)
        );
        value
    }
}

/// Convert BCD to binary
fn bcd_to_binary(bcd: u8) -> u8 {
    ((bcd >> 4) * 10) + (bcd & 0x0F)
}

/// Days since Unix epoch (simplified algorithm)
fn days_from_civil(year: i32, month: u32, day: u32) -> i32 {
    let y = if month <= 2 { year - 1 } else { year };
    let m = if month <= 2 { month + 12 } else { month };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let doy = (153 * (m - 3) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;

    era * 146097 + doe as i32 - 719468
}

/// Duration type
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Duration {
    nanos: u64,
}

impl Duration {
    /// Zero duration
    pub const ZERO: Duration = Duration { nanos: 0 };

    /// One second
    pub const SECOND: Duration = Duration { nanos: NANOS_PER_SEC };

    /// One millisecond
    pub const MILLISECOND: Duration = Duration { nanos: NANOS_PER_MILLI };

    /// One microsecond
    pub const MICROSECOND: Duration = Duration { nanos: NANOS_PER_MICRO };

    /// Create from nanoseconds
    pub const fn from_nanos(nanos: u64) -> Self {
        Self { nanos }
    }

    /// Create from microseconds
    pub const fn from_micros(micros: u64) -> Self {
        Self { nanos: micros * NANOS_PER_MICRO }
    }

    /// Create from milliseconds
    pub const fn from_millis(millis: u64) -> Self {
        Self { nanos: millis * NANOS_PER_MILLI }
    }

    /// Create from seconds
    pub const fn from_secs(secs: u64) -> Self {
        Self { nanos: secs * NANOS_PER_SEC }
    }

    /// Get as nanoseconds
    pub const fn as_nanos(&self) -> u64 {
        self.nanos
    }

    /// Get as microseconds
    pub const fn as_micros(&self) -> u64 {
        self.nanos / NANOS_PER_MICRO
    }

    /// Get as milliseconds
    pub const fn as_millis(&self) -> u64 {
        self.nanos / NANOS_PER_MILLI
    }

    /// Get as seconds
    pub const fn as_secs(&self) -> u64 {
        self.nanos / NANOS_PER_SEC
    }

    /// Get subsecond nanoseconds
    pub const fn subsec_nanos(&self) -> u32 {
        (self.nanos % NANOS_PER_SEC) as u32
    }

    /// Saturating add
    pub const fn saturating_add(self, other: Self) -> Self {
        Self { nanos: self.nanos.saturating_add(other.nanos) }
    }

    /// Saturating sub
    pub const fn saturating_sub(self, other: Self) -> Self {
        Self { nanos: self.nanos.saturating_sub(other.nanos) }
    }

    /// Checked add
    pub const fn checked_add(self, other: Self) -> Option<Self> {
        match self.nanos.checked_add(other.nanos) {
            Some(nanos) => Some(Self { nanos }),
            None => None,
        }
    }

    /// Checked sub
    pub const fn checked_sub(self, other: Self) -> Option<Self> {
        match self.nanos.checked_sub(other.nanos) {
            Some(nanos) => Some(Self { nanos }),
            None => None,
        }
    }

    /// Is zero
    pub const fn is_zero(&self) -> bool {
        self.nanos == 0
    }
}

impl core::ops::Add for Duration {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self { nanos: self.nanos + other.nanos }
    }
}

impl core::ops::Sub for Duration {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self { nanos: self.nanos - other.nanos }
    }
}

impl core::ops::Mul<u64> for Duration {
    type Output = Self;

    fn mul(self, rhs: u64) -> Self {
        Self { nanos: self.nanos * rhs }
    }
}

impl core::ops::Div<u64> for Duration {
    type Output = Self;

    fn div(self, rhs: u64) -> Self {
        Self { nanos: self.nanos / rhs }
    }
}

/// Instant type (monotonic)
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Instant {
    nanos: u64,
}

impl Instant {
    /// Get current instant
    pub fn now() -> Self {
        Self { nanos: now_ns() }
    }

    /// Duration since this instant
    pub fn elapsed(&self) -> Duration {
        let now = now_ns();
        Duration::from_nanos(now.saturating_sub(self.nanos))
    }

    /// Duration since another instant
    pub fn duration_since(&self, earlier: Instant) -> Duration {
        Duration::from_nanos(self.nanos.saturating_sub(earlier.nanos))
    }

    /// Checked add
    pub fn checked_add(&self, duration: Duration) -> Option<Instant> {
        self.nanos.checked_add(duration.nanos).map(|nanos| Instant { nanos })
    }

    /// Checked sub
    pub fn checked_sub(&self, duration: Duration) -> Option<Instant> {
        self.nanos.checked_sub(duration.nanos).map(|nanos| Instant { nanos })
    }
}

impl core::ops::Add<Duration> for Instant {
    type Output = Instant;

    fn add(self, rhs: Duration) -> Instant {
        Instant { nanos: self.nanos + rhs.nanos }
    }
}

impl core::ops::Sub<Duration> for Instant {
    type Output = Instant;

    fn sub(self, rhs: Duration) -> Instant {
        Instant { nanos: self.nanos - rhs.nanos }
    }
}

impl core::ops::Sub<Instant> for Instant {
    type Output = Duration;

    fn sub(self, rhs: Instant) -> Duration {
        Duration::from_nanos(self.nanos.saturating_sub(rhs.nanos))
    }
}

/// SystemTime (wall clock time)
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SystemTime {
    nanos: u64,
}

impl SystemTime {
    /// Unix epoch
    pub const UNIX_EPOCH: SystemTime = SystemTime { nanos: 0 };

    /// Get current system time
    pub fn now() -> Self {
        Self { nanos: realtime_ns() }
    }

    /// Duration since this time
    pub fn elapsed(&self) -> Result<Duration, ()> {
        let now = realtime_ns();
        if now >= self.nanos {
            Ok(Duration::from_nanos(now - self.nanos))
        } else {
            Err(())
        }
    }

    /// Duration since Unix epoch
    pub fn duration_since(&self, earlier: SystemTime) -> Result<Duration, ()> {
        if self.nanos >= earlier.nanos {
            Ok(Duration::from_nanos(self.nanos - earlier.nanos))
        } else {
            Err(())
        }
    }
}

/// Sleep for a duration
pub fn sleep(duration: Duration) {
    let end = now_ns() + duration.as_nanos();
    while now_ns() < end {
        // Pause to reduce power consumption
        unsafe {
            core::arch::asm!("pause", options(nomem, nostack));
        }
    }
}

/// High precision delay (busy wait)
pub fn delay_ns(nanos: u64) {
    let end = now_ns() + nanos;
    while now_ns() < end {
        unsafe {
            core::arch::asm!("pause", options(nomem, nostack));
        }
    }
}

/// Delay in microseconds
pub fn delay_us(micros: u64) {
    delay_ns(micros * NANOS_PER_MICRO);
}

/// Delay in milliseconds
pub fn delay_ms(millis: u64) {
    delay_ns(millis * NANOS_PER_MILLI);
}

/// Set RTC offset (for time adjustment)
pub fn set_rtc_offset(offset: i64) {
    RTC_OFFSET.store(offset as u64, Ordering::Release);
}

// =============================================================================
// HIGH-RESOLUTION TIMERS
// =============================================================================

use alloc::boxed::Box;
use alloc::collections::BinaryHeap;
use alloc::vec::Vec;
use core::cmp::Ordering as CmpOrdering;
use spin::Mutex;

/// Maximum number of timers
const MAX_TIMERS: usize = 4096;

/// Timer callback type
pub type TimerCallback = Box<dyn FnMut() + Send + 'static>;

/// High-resolution timer modes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HrtimerMode {
    /// Absolute time (fires at specific time)
    Abs,
    /// Relative time (fires after duration)
    Rel,
    /// Pinned to current CPU
    AbsPinned,
    /// Relative, pinned
    RelPinned,
}

/// Timer restart behavior
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HrtimerRestart {
    /// Don't restart timer
    NoRestart,
    /// Restart timer with same interval
    Restart,
}

/// High-resolution timer
pub struct Hrtimer {
    /// Timer ID
    id: u64,
    /// Expiration time (absolute nanoseconds)
    expires: u64,
    /// Timer callback
    callback: Option<TimerCallback>,
    /// Timer mode
    mode: HrtimerMode,
    /// Interval for periodic timers
    interval: u64,
    /// Is timer active?
    active: bool,
    /// CPU affinity (-1 for any)
    cpu: i32,
}

impl Hrtimer {
    /// Create new one-shot timer
    pub fn new<F: FnMut() + Send + 'static>(expires: Duration, callback: F) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);

        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            expires: now_ns() + expires.as_nanos(),
            callback: Some(Box::new(callback)),
            mode: HrtimerMode::Rel,
            interval: 0,
            active: false,
            cpu: -1,
        }
    }

    /// Create periodic timer
    pub fn periodic<F: FnMut() + Send + 'static>(interval: Duration, callback: F) -> Self {
        let mut timer = Self::new(interval, callback);
        timer.interval = interval.as_nanos();
        timer
    }

    /// Set timer mode
    pub fn with_mode(mut self, mode: HrtimerMode) -> Self {
        self.mode = mode;
        self
    }

    /// Pin to specific CPU
    pub fn with_cpu(mut self, cpu: i32) -> Self {
        self.cpu = cpu;
        self.mode = match self.mode {
            HrtimerMode::Abs => HrtimerMode::AbsPinned,
            HrtimerMode::Rel => HrtimerMode::RelPinned,
            other => other,
        };
        self
    }

    /// Get timer ID
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Check if timer is active
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Get time until expiration
    pub fn remaining(&self) -> Duration {
        let now = now_ns();
        if self.expires > now {
            Duration::from_nanos(self.expires - now)
        } else {
            Duration::ZERO
        }
    }
}

impl PartialEq for Hrtimer {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Hrtimer {}

impl PartialOrd for Hrtimer {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl Ord for Hrtimer {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        // Reverse order for min-heap behavior
        other.expires.cmp(&self.expires)
    }
}

/// Timer entry wrapper for heap
struct TimerEntry {
    timer: Hrtimer,
}

impl PartialEq for TimerEntry {
    fn eq(&self, other: &Self) -> bool {
        self.timer.expires == other.timer.expires
    }
}

impl Eq for TimerEntry {}

impl PartialOrd for TimerEntry {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl Ord for TimerEntry {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        // Reverse for min-heap
        other.timer.expires.cmp(&self.timer.expires)
    }
}

/// Per-CPU timer queue
struct TimerQueue {
    /// Timer heap (sorted by expiration)
    timers: BinaryHeap<TimerEntry>,
    /// Next timer expiration
    next_expiry: u64,
    /// Active timer count
    count: u32,
}

impl TimerQueue {
    const fn new() -> Self {
        Self {
            timers: BinaryHeap::new(),
            next_expiry: u64::MAX,
            count: 0,
        }
    }
}

/// Global timer state
struct HrtimerState {
    /// Per-CPU timer queues
    per_cpu: [Mutex<TimerQueue>; 256],
    /// Global timer queue (for unbound timers)
    global: Mutex<TimerQueue>,
    /// Timer initialized
    initialized: bool,
}

/// Global hrtimer state
static mut HRTIMER_STATE: Option<HrtimerState> = None;

/// Initialize hrtimer subsystem
pub fn hrtimer_init() {
    unsafe {
        HRTIMER_STATE = Some(HrtimerState {
            per_cpu: core::array::from_fn(|_| Mutex::new(TimerQueue::new())),
            global: Mutex::new(TimerQueue::new()),
            initialized: true,
        });
    }
}

/// Start a high-resolution timer
pub fn hrtimer_start(timer: Hrtimer) -> u64 {
    let id = timer.id;

    unsafe {
        if let Some(ref state) = HRTIMER_STATE {
            let cpu = if timer.cpu >= 0 {
                timer.cpu as usize
            } else {
                crate::cpu::current_cpu_id() as usize
            };

            let entry = TimerEntry { timer };

            if cpu < 256 {
                let mut queue = state.per_cpu[cpu].lock();
                if queue.timers.len() < MAX_TIMERS {
                    let expires = entry.timer.expires;
                    queue.timers.push(entry);
                    queue.count += 1;
                    if expires < queue.next_expiry {
                        queue.next_expiry = expires;
                    }
                }
            }
        }
    }

    id
}

/// Cancel a timer by ID
pub fn hrtimer_cancel(id: u64) -> bool {
    unsafe {
        if let Some(ref state) = HRTIMER_STATE {
            let cpu = crate::cpu::current_cpu_id() as usize;

            if cpu < 256 {
                let mut queue = state.per_cpu[cpu].lock();
                let old_len = queue.timers.len();

                // Rebuild heap without the cancelled timer
                let timers: Vec<_> = queue.timers.drain().filter(|e| e.timer.id != id).collect();
                queue.timers = BinaryHeap::from(timers);

                if queue.timers.len() < old_len {
                    queue.count -= 1;

                    // Update next expiry
                    queue.next_expiry = queue.timers.peek()
                        .map(|e| e.timer.expires)
                        .unwrap_or(u64::MAX);

                    return true;
                }
            }
        }
    }
    false
}

/// Run expired timers (called from timer interrupt)
pub fn hrtimer_run() {
    let now = now_ns();

    unsafe {
        if let Some(ref state) = HRTIMER_STATE {
            let cpu = crate::cpu::current_cpu_id() as usize;

            if cpu < 256 {
                let mut queue = state.per_cpu[cpu].lock();

                // Process all expired timers
                while let Some(entry) = queue.timers.peek() {
                    if entry.timer.expires > now {
                        break;
                    }

                    let mut entry = queue.timers.pop().unwrap();
                    queue.count -= 1;

                    // Execute callback
                    if let Some(ref mut callback) = entry.timer.callback {
                        callback();
                    }

                    // Restart periodic timer
                    if entry.timer.interval > 0 {
                        entry.timer.expires = now + entry.timer.interval;
                        queue.timers.push(entry);
                        queue.count += 1;
                    }
                }

                // Update next expiry
                queue.next_expiry = queue.timers.peek()
                    .map(|e| e.timer.expires)
                    .unwrap_or(u64::MAX);
            }
        }
    }
}

/// Get next timer expiration for current CPU
pub fn hrtimer_next_expiry() -> Option<u64> {
    unsafe {
        if let Some(ref state) = HRTIMER_STATE {
            let cpu = crate::cpu::current_cpu_id() as usize;
            if cpu < 256 {
                let queue = state.per_cpu[cpu].lock();
                if queue.next_expiry < u64::MAX {
                    return Some(queue.next_expiry);
                }
            }
        }
    }
    None
}

/// Get pending timer count for current CPU
pub fn hrtimer_pending_count() -> u32 {
    unsafe {
        if let Some(ref state) = HRTIMER_STATE {
            let cpu = crate::cpu::current_cpu_id() as usize;
            if cpu < 256 {
                return state.per_cpu[cpu].lock().count;
            }
        }
    }
    0
}

// =============================================================================
// TIMER STATISTICS
// =============================================================================

/// Timer statistics
#[derive(Default, Clone, Debug)]
pub struct HrtimerStats {
    /// Number of timers started
    pub started: u64,
    /// Number of timers cancelled
    pub cancelled: u64,
    /// Number of timers expired
    pub expired: u64,
    /// Number of timers restarted (periodic)
    pub restarted: u64,
}

static HRTIMER_STATS: Mutex<HrtimerStats> = Mutex::new(HrtimerStats {
    started: 0,
    cancelled: 0,
    expired: 0,
    restarted: 0,
});

/// Get hrtimer statistics
pub fn hrtimer_stats() -> HrtimerStats {
    HRTIMER_STATS.lock().clone()
}

// =============================================================================
// CONVENIENCE FUNCTIONS
// =============================================================================

/// Schedule a one-shot callback after delay
pub fn schedule_timeout<F: FnMut() + Send + 'static>(delay: Duration, callback: F) -> u64 {
    let timer = Hrtimer::new(delay, callback);
    hrtimer_start(timer)
}

/// Schedule a periodic callback
pub fn schedule_periodic<F: FnMut() + Send + 'static>(interval: Duration, callback: F) -> u64 {
    let timer = Hrtimer::periodic(interval, callback);
    hrtimer_start(timer)
}

/// Cancel a scheduled timeout
pub fn cancel_timeout(id: u64) -> bool {
    hrtimer_cancel(id)
}
