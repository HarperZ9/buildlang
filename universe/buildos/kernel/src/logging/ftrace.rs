// ===============================================================================
// BUILDOS KERNEL - FUNCTION TRACING (FTRACE)
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Function Tracing (ftrace)
//!
//! Provides function-level tracing for kernel performance analysis:
//! - Function entry/exit tracing
//! - Call graph recording
//! - Latency measurement
//! - Function filtering

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::sync::Spinlock;
use super::ringbuf::PerCpuRingBuffer;

/// Maximum trace depth
const MAX_TRACE_DEPTH: usize = 64;

/// Per-CPU trace buffer size
const TRACE_BUFFER_SIZE: usize = 4096;

/// Global ftrace state
static FTRACE_ENABLED: AtomicBool = AtomicBool::new(false);

/// Function tracing mode
static TRACE_MODE: AtomicU64 = AtomicU64::new(TraceMode::None as u64);

/// Function filter (if set, only trace matching functions)
static FUNCTION_FILTER: Spinlock<Option<FunctionFilter>> = Spinlock::new(None);

/// Per-CPU trace buffers
static TRACE_BUFFERS: Spinlock<Vec<TraceBuffer>> = Spinlock::new(Vec::new());

/// Function entry statistics
static FUNCTION_STATS: Spinlock<BTreeMap<u64, FunctionStats>> = Spinlock::new(BTreeMap::new());

/// Trace modes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum TraceMode {
    /// No tracing
    None = 0,
    /// Function entry only
    Function = 1,
    /// Function entry and exit (call graph)
    FunctionGraph = 2,
    /// Latency tracing
    Latency = 3,
    /// Stack tracing
    Stack = 4,
}

impl TraceMode {
    pub fn from_u64(val: u64) -> Self {
        match val {
            1 => Self::Function,
            2 => Self::FunctionGraph,
            3 => Self::Latency,
            4 => Self::Stack,
            _ => Self::None,
        }
    }
}

/// Trace entry
#[derive(Clone, Copy, Debug)]
pub struct TraceEntry {
    /// Timestamp (ns)
    pub timestamp: u64,
    /// Entry type
    pub entry_type: TraceEntryType,
    /// Function address
    pub function: u64,
    /// Parent function address (caller)
    pub parent: u64,
    /// CPU ID
    pub cpu: u32,
    /// Process ID
    pub pid: u32,
    /// Call depth
    pub depth: u16,
    /// Flags
    pub flags: TraceFlags,
}

impl Default for TraceEntry {
    fn default() -> Self {
        Self {
            timestamp: 0,
            entry_type: TraceEntryType::Enter,
            function: 0,
            parent: 0,
            cpu: 0,
            pid: 0,
            depth: 0,
            flags: TraceFlags::empty(),
        }
    }
}

/// Trace entry type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TraceEntryType {
    /// Function entry
    Enter = 0,
    /// Function exit
    Exit = 1,
    /// Context switch
    Switch = 2,
    /// Wakeup
    Wakeup = 3,
    /// IRQ entry
    IrqEnter = 4,
    /// IRQ exit
    IrqExit = 5,
    /// User annotation
    Annotation = 6,
}

bitflags::bitflags! {
    /// Trace entry flags
    #[derive(Clone, Copy, Debug, Default)]
    pub struct TraceFlags: u8 {
        /// In interrupt context
        const HARDIRQ = 0x01;
        /// In softirq context
        const SOFTIRQ = 0x02;
        /// Preemption disabled
        const PREEMPT_DISABLED = 0x04;
        /// IRQs disabled
        const IRQS_DISABLED = 0x08;
        /// Tracing nested call
        const NESTED = 0x10;
    }
}

/// Per-CPU trace buffer
struct TraceBuffer {
    cpu: u32,
    buffer: PerCpuRingBuffer<TraceEntry>,
    depth: usize,
    call_stack: [u64; MAX_TRACE_DEPTH],
    entry_times: [u64; MAX_TRACE_DEPTH],
    enabled: bool,
}

impl TraceBuffer {
    fn new(cpu: u32) -> Self {
        Self {
            cpu,
            buffer: PerCpuRingBuffer::new(TRACE_BUFFER_SIZE),
            depth: 0,
            call_stack: [0; MAX_TRACE_DEPTH],
            entry_times: [0; MAX_TRACE_DEPTH],
            enabled: true,
        }
    }

    fn push_entry(&mut self, entry: TraceEntry) {
        if self.enabled {
            let _ = self.buffer.push(entry);
        }
    }

    fn function_enter(&mut self, func: u64, parent: u64, timestamp: u64, pid: u32) {
        if self.depth < MAX_TRACE_DEPTH {
            self.call_stack[self.depth] = func;
            self.entry_times[self.depth] = timestamp;
        }

        let entry = TraceEntry {
            timestamp,
            entry_type: TraceEntryType::Enter,
            function: func,
            parent,
            cpu: self.cpu,
            pid,
            depth: self.depth as u16,
            flags: TraceFlags::empty(),
        };

        self.push_entry(entry);
        self.depth = (self.depth + 1).min(MAX_TRACE_DEPTH - 1);
    }

    fn function_exit(&mut self, func: u64, timestamp: u64, pid: u32) {
        if self.depth > 0 {
            self.depth -= 1;
        }

        let entry = TraceEntry {
            timestamp,
            entry_type: TraceEntryType::Exit,
            function: func,
            parent: if self.depth > 0 { self.call_stack[self.depth - 1] } else { 0 },
            cpu: self.cpu,
            pid,
            depth: self.depth as u16,
            flags: TraceFlags::empty(),
        };

        self.push_entry(entry);
    }

    fn drain(&mut self) -> Vec<TraceEntry> {
        let mut entries = Vec::new();
        while let Some(entry) = self.buffer.pop() {
            entries.push(entry);
        }
        entries
    }
}

/// Function statistics
#[derive(Clone, Debug, Default)]
pub struct FunctionStats {
    /// Function address
    pub address: u64,
    /// Function name (if resolved)
    pub name: Option<String>,
    /// Number of calls
    pub call_count: u64,
    /// Total time spent in function (ns)
    pub total_time: u64,
    /// Minimum call time (ns)
    pub min_time: u64,
    /// Maximum call time (ns)
    pub max_time: u64,
}

impl FunctionStats {
    fn new(address: u64) -> Self {
        Self {
            address,
            name: None,
            call_count: 0,
            total_time: 0,
            min_time: u64::MAX,
            max_time: 0,
        }
    }

    fn record_call(&mut self, duration: u64) {
        self.call_count += 1;
        self.total_time += duration;
        self.min_time = self.min_time.min(duration);
        self.max_time = self.max_time.max(duration);
    }

    pub fn avg_time(&self) -> u64 {
        if self.call_count > 0 {
            self.total_time / self.call_count
        } else {
            0
        }
    }
}

/// Function filter
pub struct FunctionFilter {
    /// Include patterns
    includes: Vec<FunctionPattern>,
    /// Exclude patterns
    excludes: Vec<FunctionPattern>,
}

impl FunctionFilter {
    pub fn new() -> Self {
        Self {
            includes: Vec::new(),
            excludes: Vec::new(),
        }
    }

    pub fn include(&mut self, pattern: FunctionPattern) {
        self.includes.push(pattern);
    }

    pub fn exclude(&mut self, pattern: FunctionPattern) {
        self.excludes.push(pattern);
    }

    pub fn matches(&self, func: u64, name: Option<&str>) -> bool {
        // Check excludes first
        for pattern in &self.excludes {
            if pattern.matches(func, name) {
                return false;
            }
        }

        // If no includes, match everything not excluded
        if self.includes.is_empty() {
            return true;
        }

        // Check includes
        for pattern in &self.includes {
            if pattern.matches(func, name) {
                return true;
            }
        }

        false
    }
}

impl Default for FunctionFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Function pattern for filtering
pub enum FunctionPattern {
    /// Match by address
    Address(u64),
    /// Match by address range
    AddressRange(u64, u64),
    /// Match by name prefix
    NamePrefix(String),
    /// Match by name suffix
    NameSuffix(String),
    /// Match by name contains
    NameContains(String),
    /// Match by exact name
    NameExact(String),
}

impl FunctionPattern {
    fn matches(&self, func: u64, name: Option<&str>) -> bool {
        match self {
            Self::Address(addr) => func == *addr,
            Self::AddressRange(start, end) => func >= *start && func <= *end,
            Self::NamePrefix(prefix) => name.map_or(false, |n| n.starts_with(prefix)),
            Self::NameSuffix(suffix) => name.map_or(false, |n| n.ends_with(suffix)),
            Self::NameContains(substr) => name.map_or(false, |n| n.contains(substr)),
            Self::NameExact(exact) => name.map_or(false, |n| n == exact),
        }
    }
}

/// Function tracer interface
pub struct FunctionTracer;

impl FunctionTracer {
    /// Enable ftrace
    pub fn enable() {
        FTRACE_ENABLED.store(true, Ordering::Release);
    }

    /// Disable ftrace
    pub fn disable() {
        FTRACE_ENABLED.store(false, Ordering::Release);
    }

    /// Check if enabled
    pub fn is_enabled() -> bool {
        FTRACE_ENABLED.load(Ordering::Acquire)
    }

    /// Set trace mode
    pub fn set_mode(mode: TraceMode) {
        TRACE_MODE.store(mode as u64, Ordering::Release);
    }

    /// Get trace mode
    pub fn mode() -> TraceMode {
        TraceMode::from_u64(TRACE_MODE.load(Ordering::Acquire))
    }

    /// Initialize trace buffers for given number of CPUs
    pub fn init(num_cpus: usize) {
        let mut buffers = TRACE_BUFFERS.lock();
        buffers.clear();
        for cpu in 0..num_cpus {
            buffers.push(TraceBuffer::new(cpu as u32));
        }
    }

    /// Record function entry
    #[inline(always)]
    pub fn function_enter(func: u64, parent: u64) {
        if !Self::is_enabled() {
            return;
        }

        let mode = Self::mode();
        if mode == TraceMode::None {
            return;
        }

        // Check filter
        if let Some(ref filter) = *FUNCTION_FILTER.lock() {
            if !filter.matches(func, None) {
                return;
            }
        }

        let cpu = current_cpu() as usize;
        let timestamp = read_tsc_ns();
        let pid = current_pid();

        let mut buffers = TRACE_BUFFERS.lock();
        if cpu < buffers.len() {
            buffers[cpu].function_enter(func, parent, timestamp, pid);
        }
    }

    /// Record function exit
    #[inline(always)]
    pub fn function_exit(func: u64) {
        if !Self::is_enabled() {
            return;
        }

        let mode = Self::mode();
        if mode == TraceMode::None || mode == TraceMode::Function {
            return;
        }

        let cpu = current_cpu() as usize;
        let timestamp = read_tsc_ns();
        let pid = current_pid();

        let mut buffers = TRACE_BUFFERS.lock();
        if cpu < buffers.len() {
            // Record stats
            let entry_time = if buffers[cpu].depth > 0 {
                buffers[cpu].entry_times[buffers[cpu].depth - 1]
            } else {
                timestamp
            };
            let duration = timestamp.saturating_sub(entry_time);

            buffers[cpu].function_exit(func, timestamp, pid);

            // Update stats
            drop(buffers);
            let mut stats = FUNCTION_STATS.lock();
            stats.entry(func)
                .or_insert_with(|| FunctionStats::new(func))
                .record_call(duration);
        }
    }

    /// Set function filter
    pub fn set_filter(filter: FunctionFilter) {
        *FUNCTION_FILTER.lock() = Some(filter);
    }

    /// Clear function filter
    pub fn clear_filter() {
        *FUNCTION_FILTER.lock() = None;
    }

    /// Get all trace entries
    pub fn drain_all() -> Vec<TraceEntry> {
        let mut all_entries = Vec::new();
        let mut buffers = TRACE_BUFFERS.lock();

        for buffer in buffers.iter_mut() {
            all_entries.extend(buffer.drain());
        }

        // Sort by timestamp
        all_entries.sort_by_key(|e| e.timestamp);
        all_entries
    }

    /// Get function statistics
    pub fn get_stats() -> Vec<FunctionStats> {
        FUNCTION_STATS.lock().values().cloned().collect()
    }

    /// Clear function statistics
    pub fn clear_stats() {
        FUNCTION_STATS.lock().clear();
    }

    /// Get trace for specific CPU
    pub fn drain_cpu(cpu: u32) -> Vec<TraceEntry> {
        let mut buffers = TRACE_BUFFERS.lock();
        if (cpu as usize) < buffers.len() {
            buffers[cpu as usize].drain()
        } else {
            Vec::new()
        }
    }

    /// Reset all trace buffers
    pub fn reset() {
        let mut buffers = TRACE_BUFFERS.lock();
        for buffer in buffers.iter_mut() {
            buffer.buffer.clear();
            buffer.depth = 0;
            buffer.call_stack = [0; MAX_TRACE_DEPTH];
            buffer.entry_times = [0; MAX_TRACE_DEPTH];
        }
    }

    /// Enable tracing for specific CPU
    pub fn enable_cpu(cpu: u32, enabled: bool) {
        let mut buffers = TRACE_BUFFERS.lock();
        if (cpu as usize) < buffers.len() {
            buffers[cpu as usize].enabled = enabled;
        }
    }
}

/// Format trace entries as text
pub fn format_trace(entries: &[TraceEntry]) -> String {
    use core::fmt::Write;
    let mut output = String::new();

    let _ = writeln!(output, "# tracer: function_graph");
    let _ = writeln!(output, "#");
    let _ = writeln!(output, "# CPU  DURATION                  FUNCTION CALLS");
    let _ = writeln!(output, "# |     |   |                     |   |   |   |");

    let mut prev_timestamp = 0u64;

    for entry in entries {
        let duration = if entry.entry_type == TraceEntryType::Exit && prev_timestamp > 0 {
            entry.timestamp.saturating_sub(prev_timestamp)
        } else {
            0
        };

        let indent = "  ".repeat(entry.depth as usize);

        match entry.entry_type {
            TraceEntryType::Enter => {
                let _ = writeln!(
                    output,
                    " {:>3})               |  {}{} {{",
                    entry.cpu,
                    indent,
                    format_function(entry.function)
                );
            }
            TraceEntryType::Exit => {
                let duration_str = format_duration(duration);
                let _ = writeln!(
                    output,
                    " {:>3}) {:>10} |  {}}}",
                    entry.cpu,
                    duration_str,
                    indent
                );
            }
            _ => {}
        }

        prev_timestamp = entry.timestamp;
    }

    output
}

/// Format function address (with symbol lookup if available)
fn format_function(addr: u64) -> String {
    // TODO: Implement symbol lookup
    alloc::format!("0x{:016x}", addr)
}

/// Format duration in microseconds
fn format_duration(ns: u64) -> String {
    if ns >= 1_000_000 {
        alloc::format!("{}.{:03} ms", ns / 1_000_000, (ns / 1000) % 1000)
    } else if ns >= 1000 {
        alloc::format!("{}.{:03} us", ns / 1000, ns % 1000)
    } else {
        alloc::format!("{} ns", ns)
    }
}

// Helper functions
fn current_cpu() -> u32 {
    0 // TODO: Get actual CPU ID
}

fn current_pid() -> u32 {
    0 // TODO: Get from scheduler
}

fn read_tsc_ns() -> u64 {
    crate::time::now_ns()
}

/// Macro for manual function tracing
#[macro_export]
macro_rules! trace_function {
    () => {{
        let func_addr = {
            #[inline(never)]
            fn __get_addr() -> u64 {
                __get_addr as u64
            }
            __get_addr()
        };
        $crate::logging::ftrace::FunctionTracer::function_enter(func_addr, 0);
        struct _Guard(u64);
        impl Drop for _Guard {
            fn drop(&mut self) {
                $crate::logging::ftrace::FunctionTracer::function_exit(self.0);
            }
        }
        _Guard(func_addr)
    }};
}

/// Tracefs-like interface for ftrace control
pub mod tracefs {
    use super::*;

    /// Read current tracer
    pub fn current_tracer() -> &'static str {
        match FunctionTracer::mode() {
            TraceMode::None => "nop",
            TraceMode::Function => "function",
            TraceMode::FunctionGraph => "function_graph",
            TraceMode::Latency => "latency",
            TraceMode::Stack => "stack",
        }
    }

    /// Set current tracer
    pub fn set_tracer(name: &str) -> Result<(), &'static str> {
        let mode = match name {
            "nop" => TraceMode::None,
            "function" => TraceMode::Function,
            "function_graph" => TraceMode::FunctionGraph,
            "latency" => TraceMode::Latency,
            "stack" => TraceMode::Stack,
            _ => return Err("unknown tracer"),
        };
        FunctionTracer::set_mode(mode);
        Ok(())
    }

    /// Check if tracing is on
    pub fn tracing_on() -> bool {
        FunctionTracer::is_enabled()
    }

    /// Set tracing state
    pub fn set_tracing_on(on: bool) {
        if on {
            FunctionTracer::enable();
        } else {
            FunctionTracer::disable();
        }
    }

    /// Read trace buffer
    pub fn trace() -> String {
        let entries = FunctionTracer::drain_all();
        format_trace(&entries)
    }

    /// Read available tracers
    pub fn available_tracers() -> &'static str {
        "function function_graph latency stack nop"
    }
}
