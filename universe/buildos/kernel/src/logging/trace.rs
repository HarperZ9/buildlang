// ===============================================================================
// BUILDOS KERNEL - DYNAMIC TRACING
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Dynamic Tracing Infrastructure
//!
//! Provides tracepoints for kernel instrumentation:
//! - Static tracepoints at compile time
//! - Dynamic tracepoint activation
//! - Probe handlers
//! - Trace event formatting

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::sync::Spinlock;

/// Trace point identifier
pub type TracePointId = u32;

/// Global tracepoint registry
static TRACEPOINTS: Spinlock<BTreeMap<TracePointId, TracePoint>> = Spinlock::new(BTreeMap::new());

/// Tracing enabled flag
static TRACING_ENABLED: AtomicBool = AtomicBool::new(false);

/// Trace sequence counter
static TRACE_SEQ: AtomicU64 = AtomicU64::new(0);

/// Trace point definition
pub struct TracePoint {
    /// Unique ID
    pub id: TracePointId,
    /// Name
    pub name: String,
    /// Subsystem
    pub subsystem: String,
    /// Description
    pub description: String,
    /// Enabled state
    enabled: AtomicBool,
    /// Probe handlers
    probes: Vec<Box<dyn TraceProbe>>,
    /// Hit count
    hits: AtomicU64,
}

impl TracePoint {
    /// Create new tracepoint
    pub fn new(
        id: TracePointId,
        name: impl Into<String>,
        subsystem: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            subsystem: subsystem.into(),
            description: description.into(),
            enabled: AtomicBool::new(false),
            probes: Vec::new(),
            hits: AtomicU64::new(0),
        }
    }

    /// Check if enabled
    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Enable tracepoint
    pub fn enable(&self) {
        self.enabled.store(true, Ordering::Release);
    }

    /// Disable tracepoint
    pub fn disable(&self) {
        self.enabled.store(false, Ordering::Release);
    }

    /// Get hit count
    pub fn hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    /// Reset hit count
    pub fn reset_hits(&self) {
        self.hits.store(0, Ordering::Relaxed);
    }

    /// Fire tracepoint with event data
    pub fn fire(&self, event: &TraceEvent) {
        if !self.is_enabled() {
            return;
        }

        self.hits.fetch_add(1, Ordering::Relaxed);

        for probe in &self.probes {
            probe.handle(event);
        }
    }

    /// Add a probe handler
    pub fn add_probe(&mut self, probe: Box<dyn TraceProbe>) {
        self.probes.push(probe);
    }
}

/// Trace event data
#[derive(Clone, Debug)]
pub struct TraceEvent {
    /// Sequence number
    pub sequence: u64,
    /// Timestamp (ns since boot)
    pub timestamp: u64,
    /// CPU ID
    pub cpu: u32,
    /// Process ID
    pub pid: u32,
    /// Thread ID
    pub tid: u32,
    /// Tracepoint ID
    pub tracepoint_id: TracePointId,
    /// Event-specific data
    pub data: TraceData,
}

impl TraceEvent {
    pub fn new(tracepoint_id: TracePointId, data: TraceData) -> Self {
        Self {
            sequence: TRACE_SEQ.fetch_add(1, Ordering::Relaxed),
            timestamp: crate::time::now_ns(),
            cpu: current_cpu(),
            pid: current_pid(),
            tid: current_tid(),
            tracepoint_id,
            data,
        }
    }
}

/// Trace data payload
#[derive(Clone, Debug)]
pub enum TraceData {
    /// Empty event
    Empty,
    /// Integer values
    Integers(Vec<i64>),
    /// String message
    Message(String),
    /// Key-value pairs
    Fields(Vec<(String, TraceValue)>),
    /// Binary data
    Binary(Vec<u8>),
}

/// Trace field value
#[derive(Clone, Debug)]
pub enum TraceValue {
    Bool(bool),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    F32(f32),
    F64(f64),
    String(String),
    Bytes(Vec<u8>),
    Pointer(u64),
}

impl TraceValue {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Bool(_) => "bool",
            Self::I8(_) => "i8",
            Self::I16(_) => "i16",
            Self::I32(_) => "i32",
            Self::I64(_) => "i64",
            Self::U8(_) => "u8",
            Self::U16(_) => "u16",
            Self::U32(_) => "u32",
            Self::U64(_) => "u64",
            Self::F32(_) => "f32",
            Self::F64(_) => "f64",
            Self::String(_) => "string",
            Self::Bytes(_) => "bytes",
            Self::Pointer(_) => "ptr",
        }
    }
}

/// Trace probe handler trait
pub trait TraceProbe: Send + Sync {
    /// Handle trace event
    fn handle(&self, event: &TraceEvent);

    /// Probe name
    fn name(&self) -> &str;
}

/// Simple logging probe
pub struct LogProbe {
    name: String,
}

impl LogProbe {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

impl TraceProbe for LogProbe {
    fn handle(&self, event: &TraceEvent) {
        crate::kprintln!(
            "[TRACE] seq={} cpu={} pid={} {:?}",
            event.sequence,
            event.cpu,
            event.pid,
            event.data
        );
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Buffer probe (stores events for later retrieval)
pub struct BufferProbe {
    name: String,
    buffer: Spinlock<Vec<TraceEvent>>,
    max_events: usize,
}

impl BufferProbe {
    pub fn new(name: impl Into<String>, max_events: usize) -> Self {
        Self {
            name: name.into(),
            buffer: Spinlock::new(Vec::with_capacity(max_events)),
            max_events,
        }
    }

    pub fn drain(&self) -> Vec<TraceEvent> {
        let mut guard = self.buffer.lock();
        core::mem::take(&mut *guard)
    }

    pub fn len(&self) -> usize {
        self.buffer.lock().len()
    }
}

impl TraceProbe for BufferProbe {
    fn handle(&self, event: &TraceEvent) {
        let mut guard = self.buffer.lock();
        if guard.len() >= self.max_events {
            guard.remove(0);
        }
        guard.push(event.clone());
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Filter probe (only passes matching events)
pub struct FilterProbe {
    name: String,
    filter: Box<dyn Fn(&TraceEvent) -> bool + Send + Sync>,
    inner: Box<dyn TraceProbe>,
}

impl FilterProbe {
    pub fn new<F>(
        name: impl Into<String>,
        filter: F,
        inner: Box<dyn TraceProbe>,
    ) -> Self
    where
        F: Fn(&TraceEvent) -> bool + Send + Sync + 'static,
    {
        Self {
            name: name.into(),
            filter: Box::new(filter),
            inner,
        }
    }
}

impl TraceProbe for FilterProbe {
    fn handle(&self, event: &TraceEvent) {
        if (self.filter)(event) {
            self.inner.handle(event);
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Global tracer interface
pub struct Tracer;

impl Tracer {
    /// Enable global tracing
    pub fn enable() {
        TRACING_ENABLED.store(true, Ordering::Release);
    }

    /// Disable global tracing
    pub fn disable() {
        TRACING_ENABLED.store(false, Ordering::Release);
    }

    /// Check if tracing is enabled
    pub fn is_enabled() -> bool {
        TRACING_ENABLED.load(Ordering::Acquire)
    }

    /// Register a tracepoint
    pub fn register(tp: TracePoint) {
        let mut guard = TRACEPOINTS.lock();
        guard.insert(tp.id, tp);
    }

    /// Get tracepoint by ID
    pub fn get(id: TracePointId) -> Option<()> {
        // Note: Returns unit for now, actual implementation would return reference
        let guard = TRACEPOINTS.lock();
        if guard.contains_key(&id) {
            Some(())
        } else {
            None
        }
    }

    /// Enable tracepoint by ID
    pub fn enable_tracepoint(id: TracePointId) -> bool {
        let guard = TRACEPOINTS.lock();
        if let Some(tp) = guard.get(&id) {
            tp.enable();
            true
        } else {
            false
        }
    }

    /// Disable tracepoint by ID
    pub fn disable_tracepoint(id: TracePointId) -> bool {
        let guard = TRACEPOINTS.lock();
        if let Some(tp) = guard.get(&id) {
            tp.disable();
            true
        } else {
            false
        }
    }

    /// Fire a tracepoint
    pub fn fire(id: TracePointId, data: TraceData) {
        if !Self::is_enabled() {
            return;
        }

        let guard = TRACEPOINTS.lock();
        if let Some(tp) = guard.get(&id) {
            let event = TraceEvent::new(id, data);
            tp.fire(&event);
        }
    }

    /// List all tracepoints
    pub fn list() -> Vec<TracePointInfo> {
        let guard = TRACEPOINTS.lock();
        guard.values().map(|tp| TracePointInfo {
            id: tp.id,
            name: tp.name.clone(),
            subsystem: tp.subsystem.clone(),
            enabled: tp.is_enabled(),
            hits: tp.hits(),
        }).collect()
    }

    /// Enable all tracepoints in subsystem
    pub fn enable_subsystem(subsystem: &str) {
        let guard = TRACEPOINTS.lock();
        for tp in guard.values() {
            if tp.subsystem == subsystem {
                tp.enable();
            }
        }
    }

    /// Disable all tracepoints in subsystem
    pub fn disable_subsystem(subsystem: &str) {
        let guard = TRACEPOINTS.lock();
        for tp in guard.values() {
            if tp.subsystem == subsystem {
                tp.disable();
            }
        }
    }
}

/// Tracepoint info (for listing)
#[derive(Clone, Debug)]
pub struct TracePointInfo {
    pub id: TracePointId,
    pub name: String,
    pub subsystem: String,
    pub enabled: bool,
    pub hits: u64,
}

// Helper functions
fn current_cpu() -> u32 {
    0 // TODO: Get actual CPU ID
}

fn current_pid() -> u32 {
    0 // TODO: Get from scheduler
}

fn current_tid() -> u32 {
    0 // TODO: Get from scheduler
}

/// Macro for defining static tracepoints
#[macro_export]
macro_rules! define_tracepoint {
    ($id:expr, $name:expr, $subsystem:expr, $desc:expr) => {{
        static INIT: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
        if !INIT.load(core::sync::atomic::Ordering::Acquire) {
            let tp = $crate::logging::trace::TracePoint::new($id, $name, $subsystem, $desc);
            $crate::logging::trace::Tracer::register(tp);
            INIT.store(true, core::sync::atomic::Ordering::Release);
        }
    }};
}

/// Macro for firing tracepoints
#[macro_export]
macro_rules! trace_event {
    ($id:expr) => {{
        $crate::logging::trace::Tracer::fire($id, $crate::logging::trace::TraceData::Empty);
    }};
    ($id:expr, $($key:ident = $val:expr),*) => {{
        let mut fields = alloc::vec::Vec::new();
        $(
            fields.push((
                alloc::string::String::from(stringify!($key)),
                $crate::logging::trace::TraceValue::from($val),
            ));
        )*
        $crate::logging::trace::Tracer::fire($id, $crate::logging::trace::TraceData::Fields(fields));
    }};
    ($id:expr, msg = $msg:expr) => {{
        $crate::logging::trace::Tracer::fire(
            $id,
            $crate::logging::trace::TraceData::Message(alloc::string::String::from($msg)),
        );
    }};
}

// TraceValue conversions
impl From<bool> for TraceValue {
    fn from(v: bool) -> Self { Self::Bool(v) }
}

impl From<i32> for TraceValue {
    fn from(v: i32) -> Self { Self::I32(v) }
}

impl From<i64> for TraceValue {
    fn from(v: i64) -> Self { Self::I64(v) }
}

impl From<u32> for TraceValue {
    fn from(v: u32) -> Self { Self::U32(v) }
}

impl From<u64> for TraceValue {
    fn from(v: u64) -> Self { Self::U64(v) }
}

impl From<&str> for TraceValue {
    fn from(v: &str) -> Self { Self::String(v.into()) }
}

impl From<String> for TraceValue {
    fn from(v: String) -> Self { Self::String(v) }
}

/// Common tracepoint IDs
pub mod tracepoint_ids {
    use super::TracePointId;

    // Scheduler tracepoints (1000-1099)
    pub const SCHED_SWITCH: TracePointId = 1000;
    pub const SCHED_WAKEUP: TracePointId = 1001;
    pub const SCHED_PROCESS_FREE: TracePointId = 1002;
    pub const SCHED_PROCESS_EXEC: TracePointId = 1003;
    pub const SCHED_PROCESS_FORK: TracePointId = 1004;
    pub const SCHED_PROCESS_EXIT: TracePointId = 1005;
    pub const SCHED_MIGRATE_TASK: TracePointId = 1006;

    // Syscall tracepoints (1100-1199)
    pub const SYSCALL_ENTER: TracePointId = 1100;
    pub const SYSCALL_EXIT: TracePointId = 1101;

    // IRQ tracepoints (1200-1299)
    pub const IRQ_HANDLER_ENTRY: TracePointId = 1200;
    pub const IRQ_HANDLER_EXIT: TracePointId = 1201;
    pub const SOFTIRQ_ENTRY: TracePointId = 1202;
    pub const SOFTIRQ_EXIT: TracePointId = 1203;

    // Memory tracepoints (1300-1399)
    pub const MM_PAGE_ALLOC: TracePointId = 1300;
    pub const MM_PAGE_FREE: TracePointId = 1301;
    pub const MM_MMAP: TracePointId = 1302;
    pub const MM_MUNMAP: TracePointId = 1303;
    pub const MM_PAGE_FAULT: TracePointId = 1304;

    // Block I/O tracepoints (1400-1499)
    pub const BLOCK_RQ_INSERT: TracePointId = 1400;
    pub const BLOCK_RQ_ISSUE: TracePointId = 1401;
    pub const BLOCK_RQ_COMPLETE: TracePointId = 1402;

    // Network tracepoints (1500-1599)
    pub const NET_DEV_QUEUE: TracePointId = 1500;
    pub const NET_DEV_XMIT: TracePointId = 1501;
    pub const NETIF_RECEIVE_SKB: TracePointId = 1502;

    // Filesystem tracepoints (1600-1699)
    pub const VFS_READ: TracePointId = 1600;
    pub const VFS_WRITE: TracePointId = 1601;
    pub const VFS_OPEN: TracePointId = 1602;
    pub const VFS_CLOSE: TracePointId = 1603;
}
