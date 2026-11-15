// ===============================================================================
// BUILDOS KERNEL - TIMER DRIVER (HPET + PIT + APIC Timer)
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! High-precision timer subsystem supporting HPET, APIC Timer, and PIT fallback.
//!
//! Provides:
//! - Monotonic clock (nanosecond precision)
//! - Wall clock time (with RTC synchronization)
//! - Timer interrupts for scheduling
//! - Sleep and delay functions
//! - One-shot and periodic timer support

use alloc::collections::BinaryHeap;
use alloc::vec::Vec;
use core::cmp::Ordering;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering};
use spin::Mutex;

use super::acpi;

// =============================================================================
// CONSTANTS
// =============================================================================

/// HPET base address (from ACPI)
const HPET_DEFAULT_BASE: u64 = 0xFED00000;

/// PIT base I/O port
const PIT_CHANNEL0: u16 = 0x40;
const PIT_COMMAND: u16 = 0x43;

/// PIT frequency (Hz)
const PIT_FREQUENCY: u32 = 1193182;

/// Default timer frequency for scheduling (Hz)
const SCHEDULER_FREQUENCY: u32 = 1000;

/// Nanoseconds per second
const NANOS_PER_SEC: u64 = 1_000_000_000;

/// Nanoseconds per millisecond
const NANOS_PER_MS: u64 = 1_000_000;

/// Nanoseconds per microsecond
const NANOS_PER_US: u64 = 1_000;

// =============================================================================
// HPET REGISTERS
// =============================================================================

/// HPET General Capabilities and ID Register
const HPET_REG_CAP: usize = 0x000;
/// HPET General Configuration Register
const HPET_REG_CONFIG: usize = 0x010;
/// HPET General Interrupt Status Register
const HPET_REG_INT_STATUS: usize = 0x020;
/// HPET Main Counter Value Register
const HPET_REG_COUNTER: usize = 0x0F0;

/// Timer N Configuration and Capability Register
const HPET_TIMER_CONFIG: usize = 0x100;
/// Timer N Comparator Value Register
const HPET_TIMER_COMPARATOR: usize = 0x108;
/// Timer N FSB Interrupt Route Register
const HPET_TIMER_FSB: usize = 0x110;

/// Timer configuration spacing
const HPET_TIMER_STRIDE: usize = 0x20;

/// HPET Config: Enable Counter
const HPET_CONFIG_ENABLE: u64 = 1 << 0;
/// HPET Config: Legacy Replacement Route
const HPET_CONFIG_LEGACY: u64 = 1 << 1;

/// Timer Config: Interrupt Type (level = 1)
const TIMER_CONF_INT_TYPE_LEVEL: u64 = 1 << 1;
/// Timer Config: Interrupt Enable
const TIMER_CONF_INT_ENABLE: u64 = 1 << 2;
/// Timer Config: Periodic Mode
const TIMER_CONF_PERIODIC: u64 = 1 << 3;
/// Timer Config: Periodic Capable
const TIMER_CONF_PERIODIC_CAP: u64 = 1 << 4;
/// Timer Config: 64-bit Capable
const TIMER_CONF_64BIT_CAP: u64 = 1 << 5;
/// Timer Config: Value Set (for periodic)
const TIMER_CONF_VAL_SET: u64 = 1 << 6;
/// Timer Config: 32-bit Mode Force
const TIMER_CONF_32BIT_MODE: u64 = 1 << 8;
/// Timer Config: Interrupt Route
const TIMER_CONF_INT_ROUTE_SHIFT: u64 = 9;

// =============================================================================
// LOCAL APIC TIMER
// =============================================================================

/// Local APIC base address
const LAPIC_BASE: u64 = 0xFEE00000;

/// APIC Timer registers
const LAPIC_TIMER_INITIAL: usize = 0x380;
const LAPIC_TIMER_CURRENT: usize = 0x390;
const LAPIC_TIMER_DIVIDE: usize = 0x3E0;
const LAPIC_LVT_TIMER: usize = 0x320;

/// APIC Timer modes
const APIC_TIMER_ONESHOT: u32 = 0x00000000;
const APIC_TIMER_PERIODIC: u32 = 0x00020000;
const APIC_TIMER_TSC_DEADLINE: u32 = 0x00040000;
const APIC_TIMER_MASKED: u32 = 0x00010000;

/// APIC Timer divider values
const APIC_TIMER_DIV_1: u32 = 0xB;
const APIC_TIMER_DIV_2: u32 = 0x0;
const APIC_TIMER_DIV_4: u32 = 0x1;
const APIC_TIMER_DIV_8: u32 = 0x2;
const APIC_TIMER_DIV_16: u32 = 0x3;
const APIC_TIMER_DIV_32: u32 = 0x8;
const APIC_TIMER_DIV_64: u32 = 0x9;
const APIC_TIMER_DIV_128: u32 = 0xA;

// =============================================================================
// TIMER STATE
// =============================================================================

/// Timer source type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerSource {
    /// High Precision Event Timer
    Hpet,
    /// Local APIC Timer
    ApicTimer,
    /// Programmable Interval Timer (legacy)
    Pit,
    /// TSC with known frequency
    Tsc,
}

/// Timer subsystem state
pub struct TimerState {
    /// Active timer source
    source: TimerSource,
    /// HPET base address (if available)
    hpet_base: Option<u64>,
    /// HPET period in femtoseconds
    hpet_period_fs: u64,
    /// HPET number of timers
    hpet_num_timers: u8,
    /// APIC timer frequency (ticks per second)
    apic_timer_freq: u64,
    /// TSC frequency (Hz)
    tsc_freq: u64,
    /// Base monotonic time (nanoseconds)
    boot_time_ns: u64,
    /// Wall clock offset from boot
    wall_clock_offset: i64,
    /// Timer interrupts enabled
    interrupts_enabled: bool,
}

impl TimerState {
    const fn new() -> Self {
        Self {
            source: TimerSource::Pit,
            hpet_base: None,
            hpet_period_fs: 0,
            hpet_num_timers: 0,
            apic_timer_freq: 0,
            tsc_freq: 0,
            boot_time_ns: 0,
            wall_clock_offset: 0,
            interrupts_enabled: false,
        }
    }
}

/// Global timer state
static TIMER_STATE: Mutex<TimerState> = Mutex::new(TimerState::new());

/// Monotonic tick counter (updated by timer interrupt)
static TICK_COUNT: AtomicU64 = AtomicU64::new(0);

/// Timer initialized flag
static TIMER_INITIALIZED: AtomicBool = AtomicBool::new(false);

// =============================================================================
// TIMER CALLBACK SYSTEM
// =============================================================================

/// Timer callback ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimerId(pub u64);

/// Timer callback entry
struct TimerCallback {
    /// Unique ID
    id: TimerId,
    /// Expiration time (nanoseconds since boot)
    expires_ns: u64,
    /// Callback function
    callback: fn(TimerId),
    /// Whether to repeat
    periodic: bool,
    /// Period for repeating timers
    period_ns: u64,
}

impl PartialEq for TimerCallback {
    fn eq(&self, other: &Self) -> bool {
        self.expires_ns == other.expires_ns
    }
}

impl Eq for TimerCallback {}

impl PartialOrd for TimerCallback {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TimerCallback {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap behavior
        other.expires_ns.cmp(&self.expires_ns)
    }
}

/// Timer callback queue
static TIMER_QUEUE: Mutex<BinaryHeap<TimerCallback>> = Mutex::new(BinaryHeap::new());

/// Next timer ID
static NEXT_TIMER_ID: AtomicU64 = AtomicU64::new(1);

// =============================================================================
// INITIALIZATION
// =============================================================================

/// Initialize timer subsystem
///
/// # Safety
///
/// Must be called once during kernel initialization, after ACPI parsing.
pub unsafe fn init() {
    let mut state = TIMER_STATE.lock();

    // Try HPET first (highest precision)
    if let Some(hpet_addr) = acpi::get_info().and_then(|info| info.hpet_addr) {
        if init_hpet(&mut state, hpet_addr) {
            crate::kprintln!("[TIMER] Using HPET at {:#x}", hpet_addr);
            state.source = TimerSource::Hpet;
        }
    }

    // If HPET not available, try to calibrate APIC timer
    if state.source == TimerSource::Pit {
        if init_apic_timer(&mut state) {
            crate::kprintln!("[TIMER] Using Local APIC Timer");
            state.source = TimerSource::ApicTimer;
        }
    }

    // Fallback to PIT
    if state.source == TimerSource::Pit {
        init_pit(&mut state);
        crate::kprintln!("[TIMER] Using PIT (legacy fallback)");
    }

    // Read initial wall clock from RTC
    state.wall_clock_offset = read_rtc_time() as i64 * NANOS_PER_SEC as i64;

    // Calibrate TSC if available
    calibrate_tsc(&mut state);

    drop(state);

    TIMER_INITIALIZED.store(true, AtomicOrdering::Release);

    crate::kprintln!("[TIMER] Timer subsystem initialized");
}

/// Initialize HPET
unsafe fn init_hpet(state: &mut TimerState, base_addr: u64) -> bool {
    // Map HPET registers (assuming identity mapping for now)
    let base = base_addr as *mut u64;

    // Read capabilities
    let cap = read_hpet(base, HPET_REG_CAP);

    // Extract period (bits 63:32) - femtoseconds per tick
    let period_fs = cap >> 32;
    if period_fs == 0 || period_fs > 100_000_000 {
        // Invalid period (must be <= 100ns)
        return false;
    }

    // Extract number of timers (bits 12:8)
    let num_timers = ((cap >> 8) & 0x1F) as u8 + 1;

    state.hpet_base = Some(base_addr);
    state.hpet_period_fs = period_fs;
    state.hpet_num_timers = num_timers;

    // Stop HPET counter
    let config = read_hpet(base, HPET_REG_CONFIG);
    write_hpet(base, HPET_REG_CONFIG, config & !HPET_CONFIG_ENABLE);

    // Reset main counter
    write_hpet(base, HPET_REG_COUNTER, 0);

    // Configure timer 0 for periodic interrupts
    if num_timers > 0 {
        let timer0_config = read_hpet(base, HPET_TIMER_CONFIG);

        if (timer0_config & TIMER_CONF_PERIODIC_CAP) != 0 {
            // Calculate comparator value for scheduler frequency
            let ticks_per_interrupt = NANOS_PER_SEC * 1_000_000 /
                (period_fs * SCHEDULER_FREQUENCY as u64);

            // Enable periodic mode, level-triggered, IRQ 0
            let new_config = TIMER_CONF_INT_ENABLE | TIMER_CONF_PERIODIC |
                TIMER_CONF_VAL_SET | (0 << TIMER_CONF_INT_ROUTE_SHIFT);

            write_hpet(base, HPET_TIMER_CONFIG, new_config);
            write_hpet(base, HPET_TIMER_COMPARATOR, ticks_per_interrupt);
        }
    }

    // Enable HPET with legacy replacement routing
    write_hpet(base, HPET_REG_CONFIG, HPET_CONFIG_ENABLE | HPET_CONFIG_LEGACY);

    state.interrupts_enabled = true;
    true
}

/// Initialize Local APIC Timer
unsafe fn init_apic_timer(state: &mut TimerState) -> bool {
    let lapic = acpi::local_apic_addr() as *mut u32;

    // Set divide configuration (divide by 16)
    write_lapic(lapic, LAPIC_TIMER_DIVIDE, APIC_TIMER_DIV_16);

    // Calibrate using PIT
    // Set PIT channel 0 to one-shot mode for calibration
    let calibration_ticks = 10000; // ~8.4ms at standard PIT frequency

    // Prepare PIT for calibration
    outb(PIT_COMMAND, 0x30); // Channel 0, lo/hi byte, mode 0
    outb(PIT_CHANNEL0, (calibration_ticks & 0xFF) as u8);
    outb(PIT_CHANNEL0, ((calibration_ticks >> 8) & 0xFF) as u8);

    // Start APIC timer with max count
    write_lapic(lapic, LAPIC_LVT_TIMER, APIC_TIMER_MASKED);
    write_lapic(lapic, LAPIC_TIMER_INITIAL, 0xFFFFFFFF);

    // Wait for PIT countdown
    loop {
        outb(PIT_COMMAND, 0x00); // Latch count
        let lo = inb(PIT_CHANNEL0) as u16;
        let hi = inb(PIT_CHANNEL0) as u16;
        let count = (hi << 8) | lo;
        if count <= 1 {
            break;
        }
    }

    // Read APIC timer current value
    let elapsed = 0xFFFFFFFF - read_lapic(lapic, LAPIC_TIMER_CURRENT);

    // Calculate frequency
    // calibration_ticks / PIT_FREQUENCY = elapsed / apic_freq
    // apic_freq = elapsed * PIT_FREQUENCY / calibration_ticks
    let freq = (elapsed as u64 * PIT_FREQUENCY as u64) / calibration_ticks as u64;

    if freq == 0 {
        return false;
    }

    state.apic_timer_freq = freq * 16; // Account for divider

    // Configure APIC timer for periodic mode
    let ticks_per_interrupt = state.apic_timer_freq / SCHEDULER_FREQUENCY as u64;

    // Timer vector 32, periodic mode
    write_lapic(lapic, LAPIC_LVT_TIMER, 32 | APIC_TIMER_PERIODIC);
    write_lapic(lapic, LAPIC_TIMER_INITIAL, ticks_per_interrupt as u32);

    state.interrupts_enabled = true;
    true
}

/// Initialize PIT (fallback)
unsafe fn init_pit(state: &mut TimerState) {
    // Calculate divisor for scheduler frequency
    let divisor = PIT_FREQUENCY / SCHEDULER_FREQUENCY;

    // Configure channel 0 for rate generator (mode 2)
    outb(PIT_COMMAND, 0x34); // Channel 0, lo/hi byte, mode 2

    // Send divisor
    outb(PIT_CHANNEL0, (divisor & 0xFF) as u8);
    outb(PIT_CHANNEL0, ((divisor >> 8) & 0xFF) as u8);

    state.interrupts_enabled = true;
}

/// Calibrate TSC
unsafe fn calibrate_tsc(state: &mut TimerState) {
    // Check if TSC is available
    let (_, _, ecx, _) = cpuid(1);
    if (ecx & (1 << 24)) == 0 {
        // TSC deadline mode not supported, try basic TSC
    }

    // Calibrate TSC using PIT or HPET
    let start_tsc = rdtsc();

    // Wait approximately 10ms using current timer
    if let Some(base) = state.hpet_base {
        let base_ptr = base as *const u64;
        let start = read_hpet(base_ptr, HPET_REG_COUNTER);
        let wait_fs = 10 * NANOS_PER_MS * 1_000_000; // 10ms in femtoseconds
        let wait_ticks = wait_fs / state.hpet_period_fs;

        while read_hpet(base_ptr, HPET_REG_COUNTER) - start < wait_ticks {
            core::hint::spin_loop();
        }
    } else {
        // Use PIT for calibration
        let calibration_ticks = 11932; // ~10ms
        outb(PIT_COMMAND, 0x30);
        outb(PIT_CHANNEL0, (calibration_ticks & 0xFF) as u8);
        outb(PIT_CHANNEL0, ((calibration_ticks >> 8) & 0xFF) as u8);

        loop {
            outb(PIT_COMMAND, 0x00);
            let lo = inb(PIT_CHANNEL0) as u16;
            let hi = inb(PIT_CHANNEL0) as u16;
            if (hi << 8) | lo <= 1 {
                break;
            }
        }
    }

    let end_tsc = rdtsc();
    let elapsed = end_tsc - start_tsc;

    // TSC frequency = elapsed ticks / 10ms = elapsed * 100 Hz
    state.tsc_freq = elapsed * 100;

    if state.tsc_freq > 0 {
        crate::kprintln!("[TIMER] TSC frequency: {} MHz",
            state.tsc_freq / 1_000_000);
    }
}

// =============================================================================
// TIME READING
// =============================================================================

/// Get monotonic time in nanoseconds since boot
pub fn monotonic_ns() -> u64 {
    let state = TIMER_STATE.lock();

    unsafe {
        match state.source {
            TimerSource::Hpet => {
                if let Some(base) = state.hpet_base {
                    let counter = read_hpet(base as *const u64, HPET_REG_COUNTER);
                    // Convert to nanoseconds: counter * period_fs / 1_000_000
                    (counter as u128 * state.hpet_period_fs as u128 / 1_000_000) as u64
                } else {
                    tick_to_ns(TICK_COUNT.load(AtomicOrdering::Relaxed))
                }
            }
            TimerSource::Tsc if state.tsc_freq > 0 => {
                let tsc = rdtsc();
                tsc * NANOS_PER_SEC / state.tsc_freq
            }
            _ => {
                tick_to_ns(TICK_COUNT.load(AtomicOrdering::Relaxed))
            }
        }
    }
}

/// Get monotonic time in microseconds since boot
pub fn monotonic_us() -> u64 {
    monotonic_ns() / NANOS_PER_US
}

/// Get monotonic time in milliseconds since boot
pub fn monotonic_ms() -> u64 {
    monotonic_ns() / NANOS_PER_MS
}

/// Get uptime in milliseconds (alias for monotonic_ms)
pub fn uptime_ms() -> u64 {
    monotonic_ms()
}

/// Get wall clock time (Unix timestamp in nanoseconds)
pub fn wall_clock_ns() -> i64 {
    let state = TIMER_STATE.lock();
    let mono = monotonic_ns() as i64;
    mono + state.wall_clock_offset
}

/// Get wall clock time (Unix timestamp in seconds)
pub fn wall_clock_secs() -> i64 {
    wall_clock_ns() / NANOS_PER_SEC as i64
}

/// Get current tick count
pub fn tick_count() -> u64 {
    TICK_COUNT.load(AtomicOrdering::Relaxed)
}

/// Convert ticks to nanoseconds
fn tick_to_ns(ticks: u64) -> u64 {
    ticks * (NANOS_PER_SEC / SCHEDULER_FREQUENCY as u64)
}

// =============================================================================
// DELAYS AND SLEEP
// =============================================================================

/// Busy-wait for specified nanoseconds
pub fn delay_ns(ns: u64) {
    let start = monotonic_ns();
    while monotonic_ns() - start < ns {
        core::hint::spin_loop();
    }
}

/// Busy-wait for specified microseconds
pub fn delay_us(us: u64) {
    delay_ns(us * NANOS_PER_US);
}

/// Busy-wait for specified milliseconds
pub fn delay_ms(ms: u64) {
    delay_ns(ms * NANOS_PER_MS);
}

/// Sleep for specified nanoseconds (yields to scheduler)
pub fn sleep_ns(ns: u64) {
    let deadline = monotonic_ns() + ns;

    while monotonic_ns() < deadline {
        // Yield to scheduler
        crate::scheduler::yield_cpu();
    }
}

/// Sleep for specified milliseconds (yields to scheduler)
pub fn sleep_ms(ms: u64) {
    sleep_ns(ms * NANOS_PER_MS);
}

// =============================================================================
// TIMER CALLBACKS
// =============================================================================

/// Register a one-shot timer callback
pub fn set_timeout(delay_ns: u64, callback: fn(TimerId)) -> TimerId {
    let id = TimerId(NEXT_TIMER_ID.fetch_add(1, AtomicOrdering::Relaxed));
    let expires = monotonic_ns() + delay_ns;

    let timer = TimerCallback {
        id,
        expires_ns: expires,
        callback,
        periodic: false,
        period_ns: 0,
    };

    TIMER_QUEUE.lock().push(timer);
    id
}

/// Register a periodic timer callback
pub fn set_interval(period_ns: u64, callback: fn(TimerId)) -> TimerId {
    let id = TimerId(NEXT_TIMER_ID.fetch_add(1, AtomicOrdering::Relaxed));
    let expires = monotonic_ns() + period_ns;

    let timer = TimerCallback {
        id,
        expires_ns: expires,
        callback,
        periodic: true,
        period_ns,
    };

    TIMER_QUEUE.lock().push(timer);
    id
}

/// Cancel a timer
pub fn cancel_timer(id: TimerId) {
    let mut queue = TIMER_QUEUE.lock();
    let items: Vec<_> = queue.drain().filter(|t| t.id != id).collect();
    for item in items {
        queue.push(item);
    }
}

/// Process expired timers (called from timer interrupt)
pub fn process_timers() {
    let now = monotonic_ns();
    let mut queue = TIMER_QUEUE.lock();
    let mut reschedule = Vec::new();

    while let Some(timer) = queue.peek() {
        if timer.expires_ns > now {
            break;
        }

        let timer = queue.pop().unwrap();
        (timer.callback)(timer.id);

        // Reschedule periodic timers
        if timer.periodic {
            reschedule.push(TimerCallback {
                id: timer.id,
                expires_ns: timer.expires_ns + timer.period_ns,
                callback: timer.callback,
                periodic: true,
                period_ns: timer.period_ns,
            });
        }
    }

    for timer in reschedule {
        queue.push(timer);
    }
}

// =============================================================================
// TIMER INTERRUPT HANDLER
// =============================================================================

/// Timer interrupt handler
///
/// Called from interrupt handler for IRQ 0 (PIT) or APIC timer interrupt.
pub fn timer_interrupt() {
    // Increment tick counter
    TICK_COUNT.fetch_add(1, AtomicOrdering::Relaxed);

    // Process timer callbacks
    process_timers();

    // Notify scheduler
    crate::scheduler::timer_tick();
}

// =============================================================================
// RTC (CMOS) ACCESS
// =============================================================================

/// CMOS I/O ports
const CMOS_ADDR: u16 = 0x70;
const CMOS_DATA: u16 = 0x71;

/// CMOS RTC registers
const RTC_SECONDS: u8 = 0x00;
const RTC_MINUTES: u8 = 0x02;
const RTC_HOURS: u8 = 0x04;
const RTC_DAY: u8 = 0x07;
const RTC_MONTH: u8 = 0x08;
const RTC_YEAR: u8 = 0x09;
const RTC_CENTURY: u8 = 0x32; // May not exist
const RTC_STATUS_A: u8 = 0x0A;
const RTC_STATUS_B: u8 = 0x0B;

/// Read RTC time as Unix timestamp
fn read_rtc_time() -> u64 {
    unsafe {
        // Wait for update to complete
        while (read_cmos(RTC_STATUS_A) & 0x80) != 0 {
            core::hint::spin_loop();
        }

        let mut seconds = read_cmos(RTC_SECONDS);
        let mut minutes = read_cmos(RTC_MINUTES);
        let mut hours = read_cmos(RTC_HOURS);
        let mut day = read_cmos(RTC_DAY);
        let mut month = read_cmos(RTC_MONTH);
        let mut year = read_cmos(RTC_YEAR);

        let status_b = read_cmos(RTC_STATUS_B);

        // Convert from BCD if needed
        if (status_b & 0x04) == 0 {
            seconds = bcd_to_binary(seconds);
            minutes = bcd_to_binary(minutes);
            hours = bcd_to_binary(hours & 0x7F) | (hours & 0x80);
            day = bcd_to_binary(day);
            month = bcd_to_binary(month);
            year = bcd_to_binary(year);
        }

        // Convert 12-hour to 24-hour
        if (status_b & 0x02) == 0 && (hours & 0x80) != 0 {
            hours = ((hours & 0x7F) + 12) % 24;
        }

        // Calculate Unix timestamp
        let full_year = 2000 + year as u32; // Assume 21st century
        days_since_epoch(full_year, month as u32, day as u32) * 86400
            + hours as u64 * 3600
            + minutes as u64 * 60
            + seconds as u64
    }
}

/// Convert BCD to binary
fn bcd_to_binary(bcd: u8) -> u8 {
    (bcd & 0x0F) + ((bcd >> 4) * 10)
}

/// Calculate days since Unix epoch (1970-01-01)
fn days_since_epoch(year: u32, month: u32, day: u32) -> u64 {
    // Days in each month (non-leap year)
    const DAYS_IN_MONTH: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

    let mut days = 0u64;

    // Add days for complete years
    for y in 1970..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }

    // Add days for complete months in current year
    for m in 1..month {
        days += DAYS_IN_MONTH[(m - 1) as usize] as u64;
        if m == 2 && is_leap_year(year) {
            days += 1;
        }
    }

    // Add days in current month
    days += (day - 1) as u64;

    days
}

/// Check if year is a leap year
fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Read CMOS register
unsafe fn read_cmos(reg: u8) -> u8 {
    outb(CMOS_ADDR, reg | 0x80); // NMI disable bit
    inb(CMOS_DATA)
}

// =============================================================================
// HPET REGISTER ACCESS
// =============================================================================

/// Read HPET register
#[inline]
unsafe fn read_hpet(base: *const u64, offset: usize) -> u64 {
    let ptr = (base as usize + offset) as *const u64;
    core::ptr::read_volatile(ptr)
}

/// Write HPET register
#[inline]
unsafe fn write_hpet(base: *mut u64, offset: usize, value: u64) {
    let ptr = (base as usize + offset) as *mut u64;
    core::ptr::write_volatile(ptr, value);
}

// =============================================================================
// LAPIC REGISTER ACCESS
// =============================================================================

/// Read Local APIC register
#[inline]
unsafe fn read_lapic(base: *const u32, offset: usize) -> u32 {
    let ptr = (base as usize + offset) as *const u32;
    core::ptr::read_volatile(ptr)
}

/// Write Local APIC register
#[inline]
unsafe fn write_lapic(base: *mut u32, offset: usize, value: u32) {
    let ptr = (base as usize + offset) as *mut u32;
    core::ptr::write_volatile(ptr, value);
}

// =============================================================================
// LOW-LEVEL I/O
// =============================================================================

/// Read byte from I/O port
#[inline]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    core::arch::asm!(
        "in al, dx",
        out("al") value,
        in("dx") port,
        options(nomem, nostack, preserves_flags)
    );
    value
}

/// Write byte to I/O port
#[inline]
unsafe fn outb(port: u16, value: u8) {
    core::arch::asm!(
        "out dx, al",
        in("dx") port,
        in("al") value,
        options(nomem, nostack, preserves_flags)
    );
}

/// Read TSC (Time Stamp Counter)
#[inline]
unsafe fn rdtsc() -> u64 {
    let lo: u32;
    let hi: u32;
    core::arch::asm!(
        "rdtsc",
        out("eax") lo,
        out("edx") hi,
        options(nomem, nostack)
    );
    ((hi as u64) << 32) | (lo as u64)
}

/// Execute CPUID instruction
#[inline]
unsafe fn cpuid(leaf: u32) -> (u32, u32, u32, u32) {
    let eax: u32;
    let ebx: u32;
    let ecx: u32;
    let edx: u32;
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
    (eax, ebx, ecx, edx)
}

// =============================================================================
// TIMESPEC AND TIMEVAL STRUCTURES
// =============================================================================

/// POSIX timespec structure
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Timespec {
    /// Seconds
    pub tv_sec: i64,
    /// Nanoseconds
    pub tv_nsec: i64,
}

impl Timespec {
    /// Create from nanoseconds
    pub fn from_ns(ns: u64) -> Self {
        Self {
            tv_sec: (ns / NANOS_PER_SEC) as i64,
            tv_nsec: (ns % NANOS_PER_SEC) as i64,
        }
    }

    /// Convert to nanoseconds
    pub fn to_ns(&self) -> u64 {
        (self.tv_sec as u64 * NANOS_PER_SEC) + self.tv_nsec as u64
    }
}

/// POSIX timeval structure
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Timeval {
    /// Seconds
    pub tv_sec: i64,
    /// Microseconds
    pub tv_usec: i64,
}

impl Timeval {
    /// Create from nanoseconds
    pub fn from_ns(ns: u64) -> Self {
        Self {
            tv_sec: (ns / NANOS_PER_SEC) as i64,
            tv_usec: ((ns % NANOS_PER_SEC) / NANOS_PER_US) as i64,
        }
    }

    /// Convert to nanoseconds
    pub fn to_ns(&self) -> u64 {
        (self.tv_sec as u64 * NANOS_PER_SEC) + (self.tv_usec as u64 * NANOS_PER_US)
    }
}

// =============================================================================
// CLOCK IDS (POSIX)
// =============================================================================

/// Clock ID for clock_gettime/clock_settime
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockId {
    /// System-wide realtime clock
    Realtime = 0,
    /// Monotonic clock (cannot be set)
    Monotonic = 1,
    /// Process CPU time
    ProcessCputime = 2,
    /// Thread CPU time
    ThreadCputime = 3,
    /// Monotonic raw (no NTP adjustments)
    MonotonicRaw = 4,
    /// Realtime coarse
    RealtimeCoarse = 5,
    /// Monotonic coarse
    MonotonicCoarse = 6,
    /// Boot time
    Boottime = 7,
}

/// Get time for specified clock
pub fn clock_gettime(clock_id: ClockId) -> Timespec {
    match clock_id {
        ClockId::Realtime | ClockId::RealtimeCoarse => {
            let ns = wall_clock_ns();
            Timespec {
                tv_sec: ns / NANOS_PER_SEC as i64,
                tv_nsec: ns % NANOS_PER_SEC as i64,
            }
        }
        ClockId::Monotonic | ClockId::MonotonicRaw |
        ClockId::MonotonicCoarse | ClockId::Boottime => {
            Timespec::from_ns(monotonic_ns())
        }
        ClockId::ProcessCputime | ClockId::ThreadCputime => {
            // TODO: Implement CPU time tracking
            Timespec::default()
        }
    }
}

// =============================================================================
// PUBLIC INTERFACE
// =============================================================================

/// Check if timer subsystem is initialized
pub fn is_initialized() -> bool {
    TIMER_INITIALIZED.load(AtomicOrdering::Acquire)
}

/// Get the active timer source
pub fn source() -> TimerSource {
    TIMER_STATE.lock().source
}

/// Get TSC frequency in Hz (0 if not calibrated)
pub fn tsc_frequency() -> u64 {
    TIMER_STATE.lock().tsc_freq
}

/// Get timer resolution in nanoseconds
pub fn resolution_ns() -> u64 {
    let state = TIMER_STATE.lock();
    match state.source {
        TimerSource::Hpet => {
            // HPET resolution: period in femtoseconds / 1_000_000
            state.hpet_period_fs / 1_000_000
        }
        TimerSource::ApicTimer | TimerSource::Tsc => {
            // ~1 nanosecond effective resolution with TSC
            1
        }
        TimerSource::Pit => {
            // PIT resolution: ~838ns (1/1.193182 MHz)
            838
        }
    }
}
