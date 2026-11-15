// ===============================================================================
// BUILDOS KERNEL - FUNCTION TRACER (FTRACE)
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Function tracing (ftrace) for profiling and debugging.
//!
//! Provides function entry/exit tracing with minimal overhead
//! using compiler instrumentation (-finstrument-functions).

#![allow(dead_code)]

use alloc::collections::BTreeSet;
use alloc::string::String;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::{Mutex, RwLock};

use crate::sched::MAX_CPUS;

// =============================================================================
// CONSTANTS
// =============================================================================

/// Maximum function graph depth
pub const MAX_GRAPH_DEPTH: usize = 64;

/// Maximum traced functions
pub const MAX_TRACED_FUNCTIONS: usize = 65536;

// =============================================================================
// STATE
// =============================================================================

/// Global ftrace state
static FTRACE: RwLock<FtraceState> = RwLock::new(FtraceState::new());

/// Per-CPU ftrace data
static mut PER_CPU_FTRACE: [PerCpuFtrace; MAX_CPUS] = {
    const INIT: PerCpuFtrace = PerCpuFtrace::new();
    [INIT; MAX_CPUS]
};

/// Ftrace state
pub struct FtraceState {
    /// Is ftrace enabled?
    enabled: AtomicBool,

    /// Tracing mode
    mode: FtraceMode,

    /// Filter by function (if empty, trace all)
    filter_functions: BTreeSet<u64>,

    /// Exclude these functions
    notrace_functions: BTreeSet<u64>,

    /// Trace only these PIDs
    filter_pids: BTreeSet<u32>,

    /// Graph tracer options
    graph_opts: GraphOptions,

    /// Total function entries traced
    total_entries: AtomicU64,

    /// Total function exits traced
    total_exits: AtomicU64,
}

impl FtraceState {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            mode: FtraceMode::Off,
            filter_functions: BTreeSet::new(),
            notrace_functions: BTreeSet::new(),
            filter_pids: BTreeSet::new(),
            graph_opts: GraphOptions::new(),
            total_entries: AtomicU64::new(0),
            total_exits: AtomicU64::new(0),
        }
    }
}

/// Per-CPU ftrace data
pub struct PerCpuFtrace {
    /// Is tracing on this CPU?
    tracing: AtomicBool,

    /// Recursion depth (to prevent self-tracing)
    recursion: AtomicU64,

    /// Graph depth
    graph_depth: AtomicU64,

    /// Graph stack (return addresses and timestamps)
    graph_stack: Mutex<GraphStack>,

    /// Disabled depth (for atomic sections)
    disabled_depth: AtomicU64,
}

impl PerCpuFtrace {
    const fn new() -> Self {
        Self {
            tracing: AtomicBool::new(true),
            recursion: AtomicU64::new(0),
            graph_depth: AtomicU64::new(0),
            graph_stack: Mutex::new(GraphStack::new()),
            disabled_depth: AtomicU64::new(0),
        }
    }
}

/// Graph stack for function graph tracer
pub struct GraphStack {
    /// Stack entries
    entries: [GraphEntry; MAX_GRAPH_DEPTH],
    /// Current depth
    depth: usize,
}

impl GraphStack {
    const fn new() -> Self {
        const EMPTY: GraphEntry = GraphEntry {
            func_addr: 0,
            ret_addr: 0,
            entry_time: 0,
            pid: 0,
        };
        Self {
            entries: [EMPTY; MAX_GRAPH_DEPTH],
            depth: 0,
        }
    }
}

/// Graph stack entry
#[derive(Clone, Copy)]
pub struct GraphEntry {
    /// Function address
    pub func_addr: u64,
    /// Return address
    pub ret_addr: u64,
    /// Entry timestamp
    pub entry_time: u64,
    /// PID at entry
    pub pid: u32,
}

/// Ftrace mode
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FtraceMode {
    /// Ftrace disabled
    Off,
    /// Simple function tracing
    Function,
    /// Function graph tracer (entry + exit)
    FunctionGraph,
}

/// Function graph options
#[derive(Clone)]
pub struct GraphOptions {
    /// Show function duration
    pub show_duration: bool,
    /// Minimum duration to display (ns)
    pub duration_threshold_ns: u64,
    /// Show overhead markers
    pub show_overhead: bool,
    /// Show IRQ context
    pub show_irq_context: bool,
    /// Show absolute timestamps
    pub show_abs_time: bool,
    /// Show CPU number
    pub show_cpu: bool,
    /// Show task name
    pub show_task: bool,
    /// Max graph depth
    pub max_depth: usize,
}

impl GraphOptions {
    const fn new() -> Self {
        Self {
            show_duration: true,
            duration_threshold_ns: 0,
            show_overhead: true,
            show_irq_context: true,
            show_abs_time: false,
            show_cpu: true,
            show_task: true,
            max_depth: MAX_GRAPH_DEPTH,
        }
    }
}

// =============================================================================
// INTERFACE
// =============================================================================

/// Initialize ftrace
pub fn init() {
    // Would patch function prologues with trampoline calls
}

/// Enable ftrace
pub fn enable() {
    FTRACE.read().enabled.store(true, Ordering::Release);
}

/// Disable ftrace
pub fn disable() {
    FTRACE.read().enabled.store(false, Ordering::Release);
}

/// Check if enabled
pub fn is_enabled() -> bool {
    FTRACE.read().enabled.load(Ordering::Relaxed)
}

/// Set ftrace mode
pub fn set_mode(mode: FtraceMode) {
    let mut state = FTRACE.write();
    state.mode = mode;
}

/// Get ftrace mode
pub fn get_mode() -> FtraceMode {
    FTRACE.read().mode
}

/// Add function to filter (trace only this function)
pub fn add_filter(func_addr: u64) {
    FTRACE.write().filter_functions.insert(func_addr);
}

/// Remove function from filter
pub fn remove_filter(func_addr: u64) {
    FTRACE.write().filter_functions.remove(&func_addr);
}

/// Clear all filters (trace all functions)
pub fn clear_filters() {
    FTRACE.write().filter_functions.clear();
}

/// Add function to notrace (never trace this function)
pub fn add_notrace(func_addr: u64) {
    FTRACE.write().notrace_functions.insert(func_addr);
}

/// Remove function from notrace
pub fn remove_notrace(func_addr: u64) {
    FTRACE.write().notrace_functions.remove(&func_addr);
}

/// Add PID to trace
pub fn add_pid_filter(pid: u32) {
    FTRACE.write().filter_pids.insert(pid);
}

/// Remove PID from trace
pub fn remove_pid_filter(pid: u32) {
    FTRACE.write().filter_pids.remove(&pid);
}

/// Clear PID filters
pub fn clear_pid_filters() {
    FTRACE.write().filter_pids.clear();
}

/// Set graph options
pub fn set_graph_options(opts: GraphOptions) {
    FTRACE.write().graph_opts = opts;
}

/// Get graph options
pub fn get_graph_options() -> GraphOptions {
    FTRACE.read().graph_opts.clone()
}

// =============================================================================
// TRACING FUNCTIONS
// =============================================================================

/// Called on function entry (by compiler instrumentation)
#[no_mangle]
pub extern "C" fn __cyg_profile_func_enter(func: u64, caller: u64) {
    ftrace_enter(func, caller);
}

/// Called on function exit (by compiler instrumentation)
#[no_mangle]
pub extern "C" fn __cyg_profile_func_exit(func: u64, caller: u64) {
    ftrace_exit(func, caller);
}

/// Handle function entry
pub fn ftrace_enter(func: u64, caller: u64) {
    let state = FTRACE.read();

    if !state.enabled.load(Ordering::Relaxed) {
        return;
    }

    let cpu = crate::cpu::current_cpu_id() as usize;
    if cpu >= MAX_CPUS {
        return;
    }

    // Check recursion guard
    unsafe {
        let recursion = PER_CPU_FTRACE[cpu].recursion.fetch_add(1, Ordering::Relaxed);
        if recursion > 0 {
            PER_CPU_FTRACE[cpu].recursion.fetch_sub(1, Ordering::Relaxed);
            return;
        }
    }

    // Check if disabled on this CPU
    unsafe {
        if PER_CPU_FTRACE[cpu].disabled_depth.load(Ordering::Relaxed) > 0 {
            PER_CPU_FTRACE[cpu].recursion.fetch_sub(1, Ordering::Relaxed);
            return;
        }
    }

    // Check function filter
    if !state.filter_functions.is_empty() && !state.filter_functions.contains(&func) {
        unsafe { PER_CPU_FTRACE[cpu].recursion.fetch_sub(1, Ordering::Relaxed); }
        return;
    }

    // Check notrace
    if state.notrace_functions.contains(&func) {
        unsafe { PER_CPU_FTRACE[cpu].recursion.fetch_sub(1, Ordering::Relaxed); }
        return;
    }

    // Check PID filter
    if !state.filter_pids.is_empty() {
        let pid = crate::process::current_pid().unwrap_or(0) as u32;
        if !state.filter_pids.contains(&pid) {
            unsafe { PER_CPU_FTRACE[cpu].recursion.fetch_sub(1, Ordering::Relaxed); }
            return;
        }
    }

    state.total_entries.fetch_add(1, Ordering::Relaxed);

    match state.mode {
        FtraceMode::Function => {
            record_function_entry(cpu, func, caller);
        }
        FtraceMode::FunctionGraph => {
            push_graph_entry(cpu, func, caller);
        }
        FtraceMode::Off => {}
    }

    unsafe { PER_CPU_FTRACE[cpu].recursion.fetch_sub(1, Ordering::Relaxed); }
}

/// Handle function exit
pub fn ftrace_exit(func: u64, _caller: u64) {
    let state = FTRACE.read();

    if !state.enabled.load(Ordering::Relaxed) {
        return;
    }

    if state.mode != FtraceMode::FunctionGraph {
        return;
    }

    let cpu = crate::cpu::current_cpu_id() as usize;
    if cpu >= MAX_CPUS {
        return;
    }

    // Check recursion guard
    unsafe {
        let recursion = PER_CPU_FTRACE[cpu].recursion.fetch_add(1, Ordering::Relaxed);
        if recursion > 0 {
            PER_CPU_FTRACE[cpu].recursion.fetch_sub(1, Ordering::Relaxed);
            return;
        }
    }

    state.total_exits.fetch_add(1, Ordering::Relaxed);

    pop_graph_entry(cpu, func);

    unsafe { PER_CPU_FTRACE[cpu].recursion.fetch_sub(1, Ordering::Relaxed); }
}

/// Record function entry (simple mode)
fn record_function_entry(cpu: usize, func: u64, caller: u64) {
    let event = super::TraceEvent {
        timestamp: crate::time::now_ns(),
        cpu: cpu as u32,
        pid: crate::process::current_pid().unwrap_or(0) as u32,
        event_type: super::EventType::Ftrace,
        name: String::from("function"),
        data: super::EventData::FunctionEntry {
            ip: func,
            parent_ip: caller,
        },
    };

    super::record_event(event);
}

/// Push function graph entry
fn push_graph_entry(cpu: usize, func: u64, caller: u64) {
    unsafe {
        let mut stack = PER_CPU_FTRACE[cpu].graph_stack.lock();

        if stack.depth >= MAX_GRAPH_DEPTH {
            return;
        }

        let entry = GraphEntry {
            func_addr: func,
            ret_addr: caller,
            entry_time: crate::time::now_ns(),
            pid: crate::process::current_pid().unwrap_or(0) as u32,
        };

        let depth = stack.depth;
        stack.entries[depth] = entry;
        stack.depth += 1;

        PER_CPU_FTRACE[cpu].graph_depth.store(stack.depth as u64, Ordering::Release);
    }

    // Record entry event
    let event = super::TraceEvent {
        timestamp: crate::time::now_ns(),
        cpu: cpu as u32,
        pid: crate::process::current_pid().unwrap_or(0) as u32,
        event_type: super::EventType::Ftrace,
        name: String::from("funcgraph_entry"),
        data: super::EventData::FunctionEntry {
            ip: func,
            parent_ip: caller,
        },
    };

    super::record_event(event);
}

/// Pop function graph entry
fn pop_graph_entry(cpu: usize, func: u64) {
    let (duration, entry_func) = unsafe {
        let mut stack = PER_CPU_FTRACE[cpu].graph_stack.lock();

        if stack.depth == 0 {
            return;
        }

        stack.depth -= 1;
        let entry = &stack.entries[stack.depth];

        let duration = crate::time::now_ns().saturating_sub(entry.entry_time);
        let entry_func = entry.func_addr;

        PER_CPU_FTRACE[cpu].graph_depth.store(stack.depth as u64, Ordering::Release);

        (duration, entry_func)
    };

    // Verify function matches
    if entry_func != func {
        // Stack mismatch - corruption or tail call
        return;
    }

    // Record exit event with duration
    let event = super::TraceEvent {
        timestamp: crate::time::now_ns(),
        cpu: cpu as u32,
        pid: crate::process::current_pid().unwrap_or(0) as u32,
        event_type: super::EventType::Ftrace,
        name: String::from("funcgraph_exit"),
        data: super::EventData::FunctionExit {
            ip: func,
            ret: duration,
        },
    };

    super::record_event(event);
}

// =============================================================================
// CPU CONTROL
// =============================================================================

/// Disable tracing on current CPU (for critical sections)
pub fn disable_cpu() {
    let cpu = crate::cpu::current_cpu_id() as usize;
    if cpu < MAX_CPUS {
        unsafe {
            PER_CPU_FTRACE[cpu].disabled_depth.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Enable tracing on current CPU
pub fn enable_cpu() {
    let cpu = crate::cpu::current_cpu_id() as usize;
    if cpu < MAX_CPUS {
        unsafe {
            PER_CPU_FTRACE[cpu].disabled_depth.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

// =============================================================================
// STATISTICS
// =============================================================================

/// Ftrace statistics
#[derive(Default)]
pub struct FtraceStats {
    pub enabled: bool,
    pub mode: &'static str,
    pub total_entries: u64,
    pub total_exits: u64,
    pub filter_count: usize,
    pub notrace_count: usize,
}

/// Get ftrace statistics
pub fn get_stats() -> FtraceStats {
    let state = FTRACE.read();
    FtraceStats {
        enabled: state.enabled.load(Ordering::Relaxed),
        mode: match state.mode {
            FtraceMode::Off => "off",
            FtraceMode::Function => "function",
            FtraceMode::FunctionGraph => "function_graph",
        },
        total_entries: state.total_entries.load(Ordering::Relaxed),
        total_exits: state.total_exits.load(Ordering::Relaxed),
        filter_count: state.filter_functions.len(),
        notrace_count: state.notrace_functions.len(),
    }
}
