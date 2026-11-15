// ===============================================================================
// BUILDOS KERNEL - TRACING INFRASTRUCTURE
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Kernel tracing infrastructure.
//!
//! This module provides:
//! - Static tracepoints for kernel events
//! - Dynamic probes (kprobes/kretprobes)
//! - Function tracing (ftrace)
//! - Event filtering and callbacks
//! - Ring buffer for trace output

#![allow(dead_code)]

pub mod tracepoint;
pub mod kprobe;
pub mod ftrace;
pub mod ringbuffer;
pub mod events;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicU32, Ordering};
use spin::{Mutex, RwLock};

// =============================================================================
// CONSTANTS
// =============================================================================

/// Default ring buffer size (per CPU, 1MB)
pub const DEFAULT_BUFFER_SIZE: usize = 1024 * 1024;

/// Maximum number of registered tracepoints
pub const MAX_TRACEPOINTS: usize = 4096;

/// Maximum number of active probes
pub const MAX_PROBES: usize = 1024;

/// Maximum event filter length
pub const MAX_FILTER_LEN: usize = 256;

// =============================================================================
// GLOBAL STATE
// =============================================================================

/// Global tracing state
static TRACING: RwLock<TracingState> = RwLock::new(TracingState::new());

/// Tracing subsystem state
pub struct TracingState {
    /// Is tracing enabled globally?
    enabled: AtomicBool,

    /// Tracing mode
    mode: TracingMode,

    /// Registered tracepoints
    tracepoints: BTreeMap<String, TracepointDef>,

    /// Active kprobes
    kprobes: BTreeMap<u64, kprobe::KProbe>,

    /// Active kretprobes
    kretprobes: BTreeMap<u64, kprobe::KRetProbe>,

    /// Function tracer state
    ftrace_enabled: AtomicBool,

    /// Current tracer
    current_tracer: TracerType,

    /// Events lost count
    events_lost: AtomicU64,

    /// Total events recorded
    total_events: AtomicU64,
}

impl TracingState {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            mode: TracingMode::Off,
            tracepoints: BTreeMap::new(),
            kprobes: BTreeMap::new(),
            kretprobes: BTreeMap::new(),
            ftrace_enabled: AtomicBool::new(false),
            current_tracer: TracerType::Nop,
            events_lost: AtomicU64::new(0),
            total_events: AtomicU64::new(0),
        }
    }
}

/// Tracing mode
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TracingMode {
    /// Tracing disabled
    Off,
    /// Recording to ring buffer
    Recording,
    /// Live streaming
    Streaming,
    /// Snapshot mode
    Snapshot,
}

/// Tracer type
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TracerType {
    /// No-op tracer
    Nop,
    /// Function tracer
    Function,
    /// Function graph tracer
    FunctionGraph,
    /// Irq off tracer
    IrqOff,
    /// Preempt off tracer
    PreemptOff,
    /// Wakeup tracer
    Wakeup,
    /// Block tracer
    Block,
    /// Hardware latency tracer
    HwLat,
    /// OSNOISE tracer
    OsNoise,
}

/// Tracepoint definition
pub struct TracepointDef {
    /// Tracepoint name
    pub name: String,
    /// Tracepoint category
    pub category: String,
    /// Is enabled?
    pub enabled: AtomicBool,
    /// Reference count
    pub refcount: AtomicU32,
    /// Callbacks
    pub callbacks: Mutex<Vec<TracepointCallback>>,
    /// Format string
    pub format: &'static str,
}

/// Tracepoint callback
pub struct TracepointCallback {
    /// Callback ID
    pub id: u64,
    /// Callback function
    pub func: fn(&TraceEvent),
    /// Priority (lower = earlier)
    pub priority: i32,
}

/// Trace event data
#[derive(Clone)]
pub struct TraceEvent {
    /// Event timestamp (ns)
    pub timestamp: u64,
    /// CPU ID
    pub cpu: u32,
    /// Process/thread ID
    pub pid: u32,
    /// Event type
    pub event_type: EventType,
    /// Event name
    pub name: String,
    /// Event data
    pub data: EventData,
}

/// Event type categories
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    /// Scheduler events
    Sched,
    /// IRQ events
    Irq,
    /// Syscall events
    Syscall,
    /// Block I/O events
    Block,
    /// Network events
    Net,
    /// Memory events
    Mem,
    /// Filesystem events
    Fs,
    /// Power events
    Power,
    /// Kprobe events
    Kprobe,
    /// Function trace events
    Ftrace,
    /// Custom events
    Custom,
}

/// Event data union
#[derive(Clone)]
pub enum EventData {
    /// Empty data
    None,
    /// Scheduler switch
    SchedSwitch {
        prev_pid: u32,
        prev_comm: String,
        prev_state: i32,
        next_pid: u32,
        next_comm: String,
    },
    /// Scheduler wakeup
    SchedWakeup {
        pid: u32,
        comm: String,
        target_cpu: u32,
    },
    /// IRQ handler entry
    IrqEntry {
        irq: u32,
        name: String,
    },
    /// IRQ handler exit
    IrqExit {
        irq: u32,
        ret: i32,
    },
    /// Syscall entry
    SyscallEntry {
        nr: u64,
        args: [u64; 6],
    },
    /// Syscall exit
    SyscallExit {
        nr: u64,
        ret: i64,
    },
    /// Function entry
    FunctionEntry {
        ip: u64,
        parent_ip: u64,
    },
    /// Function exit
    FunctionExit {
        ip: u64,
        ret: u64,
    },
    /// Block I/O
    BlockIo {
        dev: u32,
        sector: u64,
        nr_sectors: u32,
        op: u32,
    },
    /// Memory allocation
    MemAlloc {
        ptr: u64,
        size: usize,
        gfp_flags: u32,
    },
    /// Memory free
    MemFree {
        ptr: u64,
    },
    /// Kprobe hit
    KprobeHit {
        ip: u64,
        regs: [u64; 16],
    },
    /// Raw data
    Raw(Vec<u8>),
}

// =============================================================================
// PUBLIC INTERFACE
// =============================================================================

/// Initialize tracing subsystem
pub fn init() {
    let state = TRACING.write();

    // Initialize ring buffers for each CPU
    let num_cpus = crate::sched::online_cpus();
    ringbuffer::init(num_cpus, DEFAULT_BUFFER_SIZE);

    // Initialize tracepoint infrastructure
    tracepoint::init();

    // Initialize kprobe infrastructure
    kprobe::init();

    // Initialize ftrace
    ftrace::init();

    // Initialize event subsystem
    events::init();

    state.enabled.store(true, Ordering::Release);
    drop(state);

    crate::kprintln!("[TRACING] Initialized: {} CPUs, {} buffer per CPU",
        num_cpus, format_size(DEFAULT_BUFFER_SIZE));
}

/// Format size in human-readable form
fn format_size(size: usize) -> &'static str {
    if size >= 1024 * 1024 {
        "1MB"
    } else if size >= 1024 {
        "1KB"
    } else {
        "<1KB"
    }
}

/// Enable tracing
pub fn enable() {
    TRACING.read().enabled.store(true, Ordering::Release);
}

/// Disable tracing
pub fn disable() {
    TRACING.read().enabled.store(false, Ordering::Release);
}

/// Check if tracing is enabled
pub fn is_enabled() -> bool {
    TRACING.read().enabled.load(Ordering::Relaxed)
}

/// Set tracing mode
pub fn set_mode(mode: TracingMode) {
    let mut state = TRACING.write();
    state.mode = mode;
}

/// Get current tracing mode
pub fn get_mode() -> TracingMode {
    TRACING.read().mode
}

/// Set current tracer
pub fn set_tracer(tracer: TracerType) {
    let mut state = TRACING.write();

    // Disable old tracer
    match state.current_tracer {
        TracerType::Function | TracerType::FunctionGraph => {
            ftrace::disable();
        }
        _ => {}
    }

    // Enable new tracer
    match tracer {
        TracerType::Function => {
            ftrace::enable();
            ftrace::set_mode(ftrace::FtraceMode::Function);
        }
        TracerType::FunctionGraph => {
            ftrace::enable();
            ftrace::set_mode(ftrace::FtraceMode::FunctionGraph);
        }
        _ => {}
    }

    state.current_tracer = tracer;
}

/// Get current tracer
pub fn current_tracer() -> TracerType {
    TRACING.read().current_tracer
}

/// Record a trace event
pub fn record_event(event: TraceEvent) {
    if !is_enabled() {
        return;
    }

    TRACING.read().total_events.fetch_add(1, Ordering::Relaxed);

    // Write to ring buffer
    let cpu = crate::cpu::current_cpu_id() as usize;
    if let Err(_) = ringbuffer::write(cpu, &event) {
        TRACING.read().events_lost.fetch_add(1, Ordering::Relaxed);
    }
}

/// Start recording
pub fn start_recording() {
    set_mode(TracingMode::Recording);
    ringbuffer::reset_all();
}

/// Stop recording
pub fn stop_recording() {
    set_mode(TracingMode::Off);
}

/// Take a snapshot
pub fn snapshot() {
    ringbuffer::snapshot();
}

/// Read events from buffer
pub fn read_events(cpu: usize, max: usize) -> Vec<TraceEvent> {
    ringbuffer::read(cpu, max)
}

/// Read all events from all CPUs
pub fn read_all_events(max: usize) -> Vec<TraceEvent> {
    ringbuffer::read_all(max)
}

/// Clear all buffers
pub fn clear() {
    ringbuffer::reset_all();
}

// =============================================================================
// STATISTICS
// =============================================================================

/// Tracing statistics
#[derive(Default)]
pub struct TracingStats {
    pub total_events: u64,
    pub events_lost: u64,
    pub buffer_used: usize,
    pub buffer_total: usize,
    pub tracepoints_registered: usize,
    pub tracepoints_enabled: usize,
    pub kprobes_active: usize,
    pub kretprobes_active: usize,
}

/// Get tracing statistics
pub fn get_stats() -> TracingStats {
    let state = TRACING.read();
    TracingStats {
        total_events: state.total_events.load(Ordering::Relaxed),
        events_lost: state.events_lost.load(Ordering::Relaxed),
        buffer_used: ringbuffer::total_used(),
        buffer_total: ringbuffer::total_size(),
        tracepoints_registered: state.tracepoints.len(),
        tracepoints_enabled: tracepoint::enabled_count(),
        kprobes_active: state.kprobes.len(),
        kretprobes_active: state.kretprobes.len(),
    }
}

// =============================================================================
// TRACEPOINT MACROS
// =============================================================================

/// Register an ftrace tracepoint
#[macro_export]
macro_rules! define_ftrace_tracepoint {
    ($name:ident, $category:literal, $format:literal) => {
        pub fn $name() {
            static ENABLED: core::sync::atomic::AtomicBool =
                core::sync::atomic::AtomicBool::new(false);

            if ENABLED.load(core::sync::atomic::Ordering::Relaxed) {
                // Fire tracepoint
            }
        }
    };
}

/// Emit an ftrace event
#[macro_export]
macro_rules! ftrace_event {
    ($category:ident, $name:literal, $($field:ident: $value:expr),*) => {
        if $crate::tracing::is_enabled() {
            let event = $crate::tracing::TraceEvent {
                timestamp: $crate::time::now_ns(),
                cpu: $crate::cpu::current_cpu_id(),
                pid: $crate::process::current_pid().unwrap_or(0) as u32,
                event_type: $crate::tracing::EventType::$category,
                name: alloc::string::String::from($name),
                data: $crate::tracing::EventData::None,
            };
            $crate::tracing::record_event(event);
        }
    };
}
