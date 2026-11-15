// ===============================================================================
// BUILDOS KERNEL - LOGGING AND TRACING INFRASTRUCTURE
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Kernel Logging and Tracing Infrastructure
//!
//! Provides comprehensive logging and tracing capabilities:
//! - Structured logging with severity levels
//! - Ring buffer for kernel log storage
//! - Dynamic trace points (ftrace-like)
//! - Function tracing
//! - Event recording
//! - Multiple log destinations

pub mod ringbuf;
pub mod trace;
pub mod ftrace;
pub mod events;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{self, Write};
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use crate::sync::Spinlock;

pub use ringbuf::RingBuffer;
pub use trace::{TracePoint, TraceEvent, Tracer};
pub use ftrace::{FunctionTracer, TraceEntry};
pub use events::{EventType, EventRecord};

/// Log levels
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum LogLevel {
    /// Emergency: system is unusable
    Emergency = 0,
    /// Alert: action must be taken immediately
    Alert = 1,
    /// Critical: critical conditions
    Critical = 2,
    /// Error: error conditions
    Error = 3,
    /// Warning: warning conditions
    Warning = 4,
    /// Notice: normal but significant condition
    Notice = 5,
    /// Info: informational
    Info = 6,
    /// Debug: debug-level messages
    Debug = 7,
}

impl LogLevel {
    pub fn from_u8(val: u8) -> Self {
        match val {
            0 => Self::Emergency,
            1 => Self::Alert,
            2 => Self::Critical,
            3 => Self::Error,
            4 => Self::Warning,
            5 => Self::Notice,
            6 => Self::Info,
            _ => Self::Debug,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Emergency => "EMERG",
            Self::Alert => "ALERT",
            Self::Critical => "CRIT",
            Self::Error => "ERROR",
            Self::Warning => "WARN",
            Self::Notice => "NOTICE",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
        }
    }

    pub fn short_str(&self) -> &'static str {
        match self {
            Self::Emergency => "E",
            Self::Alert => "A",
            Self::Critical => "C",
            Self::Error => "E",
            Self::Warning => "W",
            Self::Notice => "N",
            Self::Info => "I",
            Self::Debug => "D",
        }
    }
}

/// Log record
#[derive(Clone)]
pub struct LogRecord {
    /// Timestamp (nanoseconds since boot)
    pub timestamp: u64,
    /// Log level
    pub level: LogLevel,
    /// Facility/subsystem
    pub facility: u16,
    /// Sequence number
    pub sequence: u64,
    /// CPU ID
    pub cpu: u32,
    /// Task ID (if applicable)
    pub task_id: Option<u32>,
    /// Message
    pub message: String,
    /// Continuation flags
    pub flags: LogFlags,
}

bitflags::bitflags! {
    /// Log record flags
    #[derive(Clone, Copy, Debug)]
    pub struct LogFlags: u8 {
        /// Message continues on next record
        const CONT = 0x01;
        /// First fragment of continuation
        const PREFIX = 0x02;
        /// Newline at end
        const NEWLINE = 0x04;
    }
}

/// Facility codes (following syslog convention)
pub mod facility {
    pub const KERN: u16 = 0;      // Kernel messages
    pub const USER: u16 = 1;      // User-level messages
    pub const MAIL: u16 = 2;      // Mail system
    pub const DAEMON: u16 = 3;    // System daemons
    pub const AUTH: u16 = 4;      // Security/auth
    pub const SYSLOG: u16 = 5;    // Syslog internal
    pub const LPR: u16 = 6;       // Printing
    pub const NEWS: u16 = 7;      // News subsystem
    pub const UUCP: u16 = 8;      // UUCP
    pub const CRON: u16 = 9;      // Cron daemon
    pub const LOCAL0: u16 = 16;   // Local use 0
    pub const LOCAL7: u16 = 23;   // Local use 7
}

/// Global kernel logger
static LOGGER: Spinlock<Option<KernelLogger>> = Spinlock::new(None);

/// Log sequence counter
static LOG_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Current log level filter
static LOG_LEVEL: AtomicU32 = AtomicU32::new(LogLevel::Info as u32);

/// Logging enabled flag
static LOGGING_ENABLED: AtomicBool = AtomicBool::new(false);

/// Kernel logger
pub struct KernelLogger {
    /// Ring buffer for log storage
    ring: RingBuffer<LogRecord>,
    /// Log destinations
    destinations: Vec<Box<dyn LogDestination>>,
    /// Rate limiting (messages per second)
    rate_limit: Option<RateLimiter>,
    /// Dropped message count
    dropped: u64,
}

impl KernelLogger {
    /// Create new logger with given buffer size
    pub fn new(capacity: usize) -> Self {
        Self {
            ring: RingBuffer::new(capacity),
            destinations: Vec::new(),
            rate_limit: None,
            dropped: 0,
        }
    }

    /// Add a log destination
    pub fn add_destination(&mut self, dest: Box<dyn LogDestination>) {
        self.destinations.push(dest);
    }

    /// Log a message
    pub fn log(&mut self, record: LogRecord) {
        // Check rate limit
        if let Some(ref mut limiter) = self.rate_limit {
            if !limiter.allow() {
                self.dropped += 1;
                return;
            }
        }

        // Store in ring buffer
        self.ring.push(record.clone());

        // Send to destinations
        for dest in &mut self.destinations {
            dest.write(&record);
        }
    }

    /// Get log records
    pub fn read(&self, start_seq: u64, max_count: usize) -> Vec<LogRecord> {
        self.ring.read_from(start_seq, max_count)
    }

    /// Get statistics
    pub fn stats(&self) -> LogStats {
        LogStats {
            total_messages: LOG_SEQUENCE.load(Ordering::Relaxed),
            dropped_messages: self.dropped,
            buffer_used: self.ring.len(),
            buffer_capacity: self.ring.capacity(),
        }
    }

    /// Clear log buffer
    pub fn clear(&mut self) {
        self.ring.clear();
    }

    /// Set rate limit
    pub fn set_rate_limit(&mut self, messages_per_second: u32) {
        if messages_per_second > 0 {
            self.rate_limit = Some(RateLimiter::new(messages_per_second));
        } else {
            self.rate_limit = None;
        }
    }
}

/// Log destination trait
pub trait LogDestination: Send {
    fn write(&mut self, record: &LogRecord);
    fn flush(&mut self) {}
    fn name(&self) -> &'static str;
}

/// Serial console destination
pub struct SerialDestination;

impl LogDestination for SerialDestination {
    fn write(&mut self, record: &LogRecord) {
        // Format: [timestamp] LEVEL: message
        let _ = crate::drivers::serial::write_fmt(format_args!(
            "[{:>10}.{:06}] {}: {}\n",
            record.timestamp / 1_000_000_000,
            (record.timestamp / 1000) % 1_000_000,
            record.level.as_str(),
            record.message
        ));
    }

    fn name(&self) -> &'static str {
        "serial"
    }
}

/// Framebuffer console destination
pub struct FramebufferDestination;

impl LogDestination for FramebufferDestination {
    fn write(&mut self, record: &LogRecord) {
        // Only show warnings and above on screen
        if record.level <= LogLevel::Warning {
            crate::kprintln!("[{}] {}", record.level.as_str(), record.message);
        }
    }

    fn name(&self) -> &'static str {
        "framebuffer"
    }
}

/// Memory buffer destination (for dmesg)
pub struct MemoryDestination {
    buffer: Vec<u8>,
    max_size: usize,
}

impl MemoryDestination {
    pub fn new(max_size: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(max_size),
            max_size,
        }
    }

    pub fn get_buffer(&self) -> &[u8] {
        &self.buffer
    }
}

impl LogDestination for MemoryDestination {
    fn write(&mut self, record: &LogRecord) {
        let formatted = alloc::format!(
            "[{:>10}.{:06}] {}: {}\n",
            record.timestamp / 1_000_000_000,
            (record.timestamp / 1000) % 1_000_000,
            record.level.as_str(),
            record.message
        );

        // Ensure we don't exceed max size
        let bytes = formatted.as_bytes();
        if self.buffer.len() + bytes.len() > self.max_size {
            // Remove old entries (first half)
            let drain_len = self.buffer.len() / 2;
            self.buffer.drain(0..drain_len);
        }

        self.buffer.extend_from_slice(bytes);
    }

    fn name(&self) -> &'static str {
        "memory"
    }
}

/// Rate limiter
struct RateLimiter {
    messages_per_second: u32,
    window_start: u64,
    count_in_window: u32,
}

impl RateLimiter {
    fn new(messages_per_second: u32) -> Self {
        Self {
            messages_per_second,
            window_start: 0,
            count_in_window: 0,
        }
    }

    fn allow(&mut self) -> bool {
        let now = crate::time::now_ns();
        let window = now / 1_000_000_000; // 1 second windows

        if window != self.window_start {
            self.window_start = window;
            self.count_in_window = 0;
        }

        if self.count_in_window < self.messages_per_second {
            self.count_in_window += 1;
            true
        } else {
            false
        }
    }
}

/// Log statistics
#[derive(Clone, Debug)]
pub struct LogStats {
    pub total_messages: u64,
    pub dropped_messages: u64,
    pub buffer_used: usize,
    pub buffer_capacity: usize,
}

/// Initialize logging subsystem
pub fn init() {
    let mut logger = KernelLogger::new(4096);

    // Add default destinations
    logger.add_destination(Box::new(SerialDestination));
    logger.add_destination(Box::new(MemoryDestination::new(256 * 1024)));

    *LOGGER.lock() = Some(logger);
    LOGGING_ENABLED.store(true, Ordering::Release);
}

/// Log a message
pub fn log(level: LogLevel, facility: u16, message: String) {
    if !LOGGING_ENABLED.load(Ordering::Acquire) {
        return;
    }

    // Check log level filter
    if (level as u32) > LOG_LEVEL.load(Ordering::Relaxed) {
        return;
    }

    let record = LogRecord {
        timestamp: crate::time::now_ns(),
        level,
        facility,
        sequence: LOG_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        cpu: current_cpu(),
        task_id: current_task_id(),
        message,
        flags: LogFlags::NEWLINE,
    };

    let mut guard = LOGGER.lock();
    if let Some(ref mut logger) = *guard {
        logger.log(record);
    }
}

/// Convenience functions
pub fn emergency(msg: impl Into<String>) {
    log(LogLevel::Emergency, facility::KERN, msg.into());
}

pub fn alert(msg: impl Into<String>) {
    log(LogLevel::Alert, facility::KERN, msg.into());
}

pub fn critical(msg: impl Into<String>) {
    log(LogLevel::Critical, facility::KERN, msg.into());
}

pub fn error(msg: impl Into<String>) {
    log(LogLevel::Error, facility::KERN, msg.into());
}

pub fn warning(msg: impl Into<String>) {
    log(LogLevel::Warning, facility::KERN, msg.into());
}

pub fn notice(msg: impl Into<String>) {
    log(LogLevel::Notice, facility::KERN, msg.into());
}

pub fn info(msg: impl Into<String>) {
    log(LogLevel::Info, facility::KERN, msg.into());
}

pub fn debug(msg: impl Into<String>) {
    log(LogLevel::Debug, facility::KERN, msg.into());
}

/// Set log level filter
pub fn set_level(level: LogLevel) {
    LOG_LEVEL.store(level as u32, Ordering::Relaxed);
}

/// Get current log level
pub fn get_level() -> LogLevel {
    LogLevel::from_u8(LOG_LEVEL.load(Ordering::Relaxed) as u8)
}

/// Read log records (for dmesg)
pub fn read_log(start_seq: u64, max_count: usize) -> Vec<LogRecord> {
    let guard = LOGGER.lock();
    if let Some(ref logger) = *guard {
        logger.read(start_seq, max_count)
    } else {
        Vec::new()
    }
}

/// Get log statistics
pub fn get_stats() -> Option<LogStats> {
    let guard = LOGGER.lock();
    guard.as_ref().map(|l| l.stats())
}

/// Clear log buffer
pub fn clear_log() {
    let mut guard = LOGGER.lock();
    if let Some(ref mut logger) = *guard {
        logger.clear();
    }
}

// Helper functions
fn current_cpu() -> u32 {
    // TODO: Get actual CPU ID
    0
}

fn current_task_id() -> Option<u32> {
    // TODO: Get from current process
    None
}

/// Printf-style logging macro
#[macro_export]
macro_rules! klog {
    ($level:expr, $($arg:tt)*) => {{
        $crate::logging::log($level, $crate::logging::facility::KERN, alloc::format!($($arg)*));
    }};
}

#[macro_export]
macro_rules! klog_debug {
    ($($arg:tt)*) => {{
        $crate::klog!($crate::logging::LogLevel::Debug, $($arg)*);
    }};
}

#[macro_export]
macro_rules! klog_info {
    ($($arg:tt)*) => {{
        $crate::klog!($crate::logging::LogLevel::Info, $($arg)*);
    }};
}

#[macro_export]
macro_rules! klog_warn {
    ($($arg:tt)*) => {{
        $crate::klog!($crate::logging::LogLevel::Warning, $($arg)*);
    }};
}

#[macro_export]
macro_rules! klog_error {
    ($($arg:tt)*) => {{
        $crate::klog!($crate::logging::LogLevel::Error, $($arg)*);
    }};
}

/// Printk compatibility
#[macro_export]
macro_rules! printk {
    ($($arg:tt)*) => {{
        $crate::klog!($crate::logging::LogLevel::Info, $($arg)*);
    }};
}

/// Dev debug macro (compile-time controlled)
#[cfg(debug_assertions)]
#[macro_export]
macro_rules! dev_dbg {
    ($($arg:tt)*) => {{
        $crate::klog!($crate::logging::LogLevel::Debug, $($arg)*);
    }};
}

#[cfg(not(debug_assertions))]
#[macro_export]
macro_rules! dev_dbg {
    ($($arg:tt)*) => {{}};
}

/// Kernel oops formatter
pub struct OopsWriter {
    buffer: Vec<u8>,
}

impl OopsWriter {
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(4096),
        }
    }

    pub fn finish(self) -> String {
        String::from_utf8_lossy(&self.buffer).into_owned()
    }
}

impl Write for OopsWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.buffer.extend_from_slice(s.as_bytes());
        Ok(())
    }
}

impl Default for OopsWriter {
    fn default() -> Self {
        Self::new()
    }
}

/// Dump registers for crash report
pub fn dump_registers(writer: &mut OopsWriter, regs: &CrashRegs) {
    let _ = writeln!(writer, "Registers:");
    let _ = writeln!(writer, "  RAX: {:016x}  RBX: {:016x}", regs.rax, regs.rbx);
    let _ = writeln!(writer, "  RCX: {:016x}  RDX: {:016x}", regs.rcx, regs.rdx);
    let _ = writeln!(writer, "  RSI: {:016x}  RDI: {:016x}", regs.rsi, regs.rdi);
    let _ = writeln!(writer, "  RBP: {:016x}  RSP: {:016x}", regs.rbp, regs.rsp);
    let _ = writeln!(writer, "  R8:  {:016x}  R9:  {:016x}", regs.r8, regs.r9);
    let _ = writeln!(writer, "  R10: {:016x}  R11: {:016x}", regs.r10, regs.r11);
    let _ = writeln!(writer, "  R12: {:016x}  R13: {:016x}", regs.r12, regs.r13);
    let _ = writeln!(writer, "  R14: {:016x}  R15: {:016x}", regs.r14, regs.r15);
    let _ = writeln!(writer, "  RIP: {:016x}  RFLAGS: {:016x}", regs.rip, regs.rflags);
    let _ = writeln!(writer, "  CS:  {:04x}  SS: {:04x}", regs.cs, regs.ss);
}

/// Crash registers
#[derive(Clone, Copy, Debug, Default)]
pub struct CrashRegs {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
    pub rflags: u64,
    pub cs: u16,
    pub ss: u16,
}

/// Dump stack trace
pub fn dump_stack(writer: &mut OopsWriter, sp: u64, max_frames: usize) {
    let _ = writeln!(writer, "Stack trace:");

    // Read stack frames
    let mut frame_ptr = sp;
    let mut count = 0;

    while count < max_frames && frame_ptr != 0 && frame_ptr < 0xffff_ffff_ffff_0000 {
        // Safety: We're in a crash handler, best effort
        let return_addr = unsafe {
            if frame_ptr as *const u64 >= core::ptr::null() {
                *(frame_ptr as *const u64).add(1)
            } else {
                break;
            }
        };

        let _ = writeln!(writer, "  [{:2}] {:016x}", count, return_addr);

        // Get next frame pointer
        let next_frame = unsafe { *(frame_ptr as *const u64) };
        if next_frame <= frame_ptr {
            break; // Prevent infinite loops
        }
        frame_ptr = next_frame;
        count += 1;
    }
}
