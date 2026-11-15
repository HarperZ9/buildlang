// ===============================================================================
// BUILDOS KERNEL - PERF/TRACING SUBSYSTEM
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Performance Monitoring and Tracing Subsystem
//!
//! This module implements Linux-compatible performance monitoring and tracing:
//! - Hardware performance counters (PMU)
//! - Software events (context switches, page faults)
//! - Tracepoints for kernel instrumentation
//! - Kprobes for dynamic kernel probing
//! - Uprobes for user-space probing
//! - Ring buffer for efficient sample collection
//! - perf_event_open() syscall support

#![allow(dead_code)]

pub mod events;
pub mod pmu;
pub mod tracepoints;
pub mod kprobes;
pub mod uprobes;
pub mod ring_buffer;
pub mod sampling;
pub mod syscall;

pub use events::{PerfEvent, PerfEventType, PerfEventAttr, PerfEventConfig, HardwareEvent, CacheEvent};
pub use pmu::{Pmu, PmuType};
pub use tracepoints::{Tracepoint, TracepointId, TracepointCallback};
pub use kprobes::{Kprobe, KprobeHandler};
pub use uprobes::{Uprobe, UprobeHandler};
pub use ring_buffer::PerfRingBuffer;
pub use sampling::{Sample, SampleType, SampleData};

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use crate::sync::{Mutex, RwLock};

/// Perf subsystem error types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerfError {
    /// Invalid event type
    InvalidEvent,
    /// Event not supported
    NotSupported,
    /// Resource busy
    Busy,
    /// Out of resources
    NoResources,
    /// Permission denied
    PermissionDenied,
    /// Invalid argument
    InvalidArgument,
    /// Event disabled
    Disabled,
    /// Buffer overflow
    Overflow,
    /// CPU not available
    CpuNotAvailable,
    /// File descriptor limit
    TooManyFiles,
}

impl PerfError {
    /// Convert to errno
    pub fn to_errno(self) -> i32 {
        match self {
            Self::InvalidEvent => -22,       // EINVAL
            Self::NotSupported => -95,       // EOPNOTSUPP
            Self::Busy => -16,               // EBUSY
            Self::NoResources => -12,        // ENOMEM
            Self::PermissionDenied => -1,    // EPERM
            Self::InvalidArgument => -22,    // EINVAL
            Self::Disabled => -22,           // EINVAL
            Self::Overflow => -75,           // EOVERFLOW
            Self::CpuNotAvailable => -19,    // ENODEV
            Self::TooManyFiles => -24,       // EMFILE
        }
    }
}

/// Perf event file descriptor wrapper
pub struct PerfEventFd {
    /// Event ID
    pub id: u64,
    /// Event attributes
    pub attr: PerfEventAttr,
    /// Target CPU (-1 for any)
    pub cpu: i32,
    /// Target PID (-1 for any, 0 for self)
    pub pid: i32,
    /// Group leader FD (-1 for group leader)
    pub group_fd: i32,
    /// Flags
    pub flags: PerfEventFlags,
    /// Ring buffer for samples
    pub ring_buffer: Option<Arc<PerfRingBuffer>>,
    /// Current event count
    pub count: AtomicU64,
    /// Time enabled
    pub time_enabled: AtomicU64,
    /// Time running
    pub time_running: AtomicU64,
    /// Enabled state
    pub enabled: AtomicBool,
    /// PMU index (for hardware events)
    pub pmu_index: Option<u32>,
}

bitflags::bitflags! {
    /// Perf event flags
    #[derive(Clone, Copy, Debug, Default)]
    pub struct PerfEventFlags: u64 {
        /// Close-on-exec
        const CLOEXEC = 1 << 0;
        /// No CPU migration
        const NO_GROUP = 1 << 1;
        /// File descriptor output
        const FD_OUTPUT = 1 << 2;
        /// Request PID checks on mmap
        const PID_CGROUP = 1 << 3;
    }
}

impl PerfEventFd {
    /// Create new perf event file descriptor
    pub fn new(
        id: u64,
        attr: PerfEventAttr,
        cpu: i32,
        pid: i32,
        group_fd: i32,
        flags: PerfEventFlags,
    ) -> Self {
        Self {
            id,
            attr,
            cpu,
            pid,
            group_fd,
            flags,
            ring_buffer: None,
            count: AtomicU64::new(0),
            time_enabled: AtomicU64::new(0),
            time_running: AtomicU64::new(0),
            enabled: AtomicBool::new(false),
            pmu_index: None,
        }
    }

    /// Enable the event
    pub fn enable(&self) -> Result<(), PerfError> {
        if self.attr.disabled && !self.enabled.load(Ordering::Acquire) {
            // Enable hardware counter if applicable
            if let Some(pmu_idx) = self.pmu_index {
                pmu::enable_counter(pmu_idx)?;
            }
            self.enabled.store(true, Ordering::Release);
        }
        Ok(())
    }

    /// Disable the event
    pub fn disable(&self) -> Result<(), PerfError> {
        if self.enabled.load(Ordering::Acquire) {
            // Disable hardware counter if applicable
            if let Some(pmu_idx) = self.pmu_index {
                pmu::disable_counter(pmu_idx)?;
            }
            self.enabled.store(false, Ordering::Release);
        }
        Ok(())
    }

    /// Reset the event counter
    pub fn reset(&self) -> Result<(), PerfError> {
        self.count.store(0, Ordering::Release);
        self.time_enabled.store(0, Ordering::Release);
        self.time_running.store(0, Ordering::Release);

        // Reset hardware counter if applicable
        if let Some(pmu_idx) = self.pmu_index {
            pmu::reset_counter(pmu_idx)?;
        }
        Ok(())
    }

    /// Read current count
    pub fn read(&self) -> PerfEventReadValue {
        PerfEventReadValue {
            value: self.count.load(Ordering::Acquire),
            time_enabled: self.time_enabled.load(Ordering::Acquire),
            time_running: self.time_running.load(Ordering::Acquire),
            id: self.id,
        }
    }

    /// Add sample to ring buffer
    pub fn add_sample(&self, sample: Sample) {
        if let Some(ref rb) = self.ring_buffer {
            let _ = rb.push_sample(&sample);
        }
    }

    /// Increment counter
    pub fn increment(&self, delta: u64) {
        self.count.fetch_add(delta, Ordering::AcqRel);
    }
}

/// Perf event read value
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct PerfEventReadValue {
    /// Counter value
    pub value: u64,
    /// Time enabled (nanoseconds)
    pub time_enabled: u64,
    /// Time running (nanoseconds)
    pub time_running: u64,
    /// Event ID
    pub id: u64,
}

/// Global perf subsystem state
pub struct PerfSubsystem {
    /// Next event ID
    next_id: AtomicU64,
    /// Active events by ID
    events: RwLock<BTreeMap<u64, Arc<PerfEventFd>>>,
    /// Events by CPU
    cpu_events: RwLock<[Vec<Arc<PerfEventFd>>; 256]>,
    /// Events by PID
    pid_events: RwLock<BTreeMap<u32, Vec<Arc<PerfEventFd>>>>,
    /// Global enabled flag
    enabled: AtomicBool,
    /// Statistics
    stats: PerfStats,
    /// PMU state
    pmu: pmu::PmuState,
}

/// Perf subsystem statistics
#[derive(Debug, Default)]
pub struct PerfStats {
    /// Total events created
    pub events_created: AtomicU64,
    /// Total samples collected
    pub samples_collected: AtomicU64,
    /// Lost samples (overflow)
    pub samples_lost: AtomicU64,
    /// Total reads
    pub reads: AtomicU64,
    /// Context switches traced
    pub context_switches: AtomicU64,
    /// Page faults traced
    pub page_faults: AtomicU64,
}

/// Global perf subsystem instance
static PERF: Mutex<Option<PerfSubsystem>> = Mutex::new(None);

/// Initialize perf subsystem
pub fn init() {
    // Initialize PMU
    pmu::init();

    // Initialize tracepoints
    tracepoints::init();

    // Initialize kprobes
    kprobes::init();

    // Initialize uprobes
    uprobes::init();

    // Create subsystem instance
    let subsystem = PerfSubsystem {
        next_id: AtomicU64::new(1),
        events: RwLock::new(BTreeMap::new()),
        cpu_events: RwLock::new(core::array::from_fn(|_| Vec::new())),
        pid_events: RwLock::new(BTreeMap::new()),
        enabled: AtomicBool::new(true),
        stats: PerfStats::default(),
        pmu: pmu::PmuState::new(),
    };

    *PERF.lock() = Some(subsystem);

    crate::kprintln!("[PERF] Performance monitoring subsystem initialized");

    // Log PMU capabilities
    let pmu_info = pmu::info();
    crate::kprintln!("[PERF] PMU: {} counters, version {}",
        pmu_info.num_counters, pmu_info.version);
}

/// Create a perf event (perf_event_open equivalent)
pub fn event_open(
    attr: &PerfEventAttr,
    pid: i32,
    cpu: i32,
    group_fd: i32,
    flags: PerfEventFlags,
) -> Result<u64, PerfError> {
    let mut perf = PERF.lock();
    let subsystem = perf.as_mut().ok_or(PerfError::NotSupported)?;

    // Validate arguments
    if cpu >= 256 || cpu < -1 {
        return Err(PerfError::CpuNotAvailable);
    }

    // Check permissions for certain event types
    if attr.exclude_kernel && !can_access_kernel_events() {
        return Err(PerfError::PermissionDenied);
    }

    // Generate event ID
    let id = subsystem.next_id.fetch_add(1, Ordering::SeqCst);

    // Create event
    let mut event = PerfEventFd::new(id, attr.clone(), cpu, pid, group_fd, flags);

    // Allocate hardware counter for PMU events
    if matches!(attr.event_type, PerfEventType::Hardware | PerfEventType::HardwareCache) {
        let pmu_idx = pmu::allocate_counter(&attr)?;
        event.pmu_index = Some(pmu_idx);
    }

    // Create ring buffer if sampling
    if attr.sample_period > 0 || attr.sample_freq > 0 {
        let pages = if attr.mmap_data_pages > 0 {
            attr.mmap_data_pages as usize
        } else {
            16 // Default 16 pages
        };
        event.ring_buffer = Some(Arc::new(PerfRingBuffer::new(pages)));
    }

    // Enable if not disabled by default
    if !attr.disabled {
        event.enable()?;
    }

    let event = Arc::new(event);

    // Register event
    subsystem.events.write().insert(id, Arc::clone(&event));

    if cpu >= 0 {
        subsystem.cpu_events.write()[cpu as usize].push(Arc::clone(&event));
    }

    if pid >= 0 {
        let pid_events = &mut subsystem.pid_events.write();
        pid_events.entry(pid as u32)
            .or_insert_with(Vec::new)
            .push(Arc::clone(&event));
    }

    subsystem.stats.events_created.fetch_add(1, Ordering::Relaxed);

    Ok(id)
}

/// Close a perf event
pub fn event_close(event_id: u64) -> Result<(), PerfError> {
    let mut perf = PERF.lock();
    let subsystem = perf.as_mut().ok_or(PerfError::NotSupported)?;

    // Find and remove event
    let event = subsystem.events.write().remove(&event_id)
        .ok_or(PerfError::InvalidArgument)?;

    // Disable hardware counter
    if let Some(pmu_idx) = event.pmu_index {
        pmu::free_counter(pmu_idx)?;
    }

    // Remove from CPU list
    if event.cpu >= 0 {
        let cpu_events = &mut subsystem.cpu_events.write()[event.cpu as usize];
        cpu_events.retain(|e| e.id != event_id);
    }

    // Remove from PID list
    if event.pid >= 0 {
        if let Some(pid_events) = subsystem.pid_events.write().get_mut(&(event.pid as u32)) {
            pid_events.retain(|e| e.id != event_id);
        }
    }

    Ok(())
}

/// Read perf event value
pub fn event_read(event_id: u64) -> Result<PerfEventReadValue, PerfError> {
    let perf = PERF.lock();
    let subsystem = perf.as_ref().ok_or(PerfError::NotSupported)?;

    let events = subsystem.events.read();
    let event = events.get(&event_id).ok_or(PerfError::InvalidArgument)?;

    // For hardware events, read from PMU
    if let Some(pmu_idx) = event.pmu_index {
        let count = pmu::read_counter(pmu_idx)?;
        event.count.store(count, Ordering::Release);
    }

    subsystem.stats.reads.fetch_add(1, Ordering::Relaxed);

    Ok(event.read())
}

/// Enable perf event
pub fn event_enable(event_id: u64) -> Result<(), PerfError> {
    let perf = PERF.lock();
    let subsystem = perf.as_ref().ok_or(PerfError::NotSupported)?;

    let events = subsystem.events.read();
    let event = events.get(&event_id).ok_or(PerfError::InvalidArgument)?;

    event.enable()
}

/// Disable perf event
pub fn event_disable(event_id: u64) -> Result<(), PerfError> {
    let perf = PERF.lock();
    let subsystem = perf.as_ref().ok_or(PerfError::NotSupported)?;

    let events = subsystem.events.read();
    let event = events.get(&event_id).ok_or(PerfError::InvalidArgument)?;

    event.disable()
}

/// Reset perf event
pub fn event_reset(event_id: u64) -> Result<(), PerfError> {
    let perf = PERF.lock();
    let subsystem = perf.as_ref().ok_or(PerfError::NotSupported)?;

    let events = subsystem.events.read();
    let event = events.get(&event_id).ok_or(PerfError::InvalidArgument)?;

    event.reset()
}

/// Record a sample for all matching events
pub fn record_sample(cpu: u32, pid: u32, sample: Sample) {
    let perf = PERF.lock();
    if let Some(ref subsystem) = *perf {
        if !subsystem.enabled.load(Ordering::Relaxed) {
            return;
        }

        // Check CPU events
        if (cpu as usize) < 256 {
            let cpu_events = subsystem.cpu_events.read();
            for event in &cpu_events[cpu as usize] {
                if event.enabled.load(Ordering::Relaxed) {
                    event.add_sample(sample.clone());
                }
            }
        }

        // Check PID events
        let pid_events = subsystem.pid_events.read();
        if let Some(events) = pid_events.get(&pid) {
            for event in events {
                if event.enabled.load(Ordering::Relaxed) {
                    event.add_sample(sample.clone());
                }
            }
        }

        subsystem.stats.samples_collected.fetch_add(1, Ordering::Relaxed);
    }
}

/// Record context switch event
pub fn record_context_switch(prev_pid: u32, _next_pid: u32, cpu: u32) {
    let sample = Sample::new(SampleType::ContextSwitch)
        .with_pid(prev_pid)
        .with_cpu(cpu)
        .with_time(crate::time::now_ns());

    record_sample(cpu, prev_pid, sample);

    if let Some(ref subsystem) = *PERF.lock() {
        subsystem.stats.context_switches.fetch_add(1, Ordering::Relaxed);
    }
}

/// Record page fault event
pub fn record_page_fault(addr: u64, pid: u32, cpu: u32, _is_write: bool, is_major: bool) {
    let sample = Sample::new(if is_major {
        SampleType::PageFaultMajor
    } else {
        SampleType::PageFaultMinor
    })
        .with_pid(pid)
        .with_cpu(cpu)
        .with_addr(addr)
        .with_time(crate::time::now_ns());

    record_sample(cpu, pid, sample);

    if let Some(ref subsystem) = *PERF.lock() {
        subsystem.stats.page_faults.fetch_add(1, Ordering::Relaxed);
    }
}

/// Check if caller can access kernel events
fn can_access_kernel_events() -> bool {
    // Would check CAP_PERFMON or CAP_SYS_ADMIN
    // For now, always allow
    true
}

/// Get perf statistics
pub fn stats() -> Option<(u64, u64, u64, u64)> {
    let perf = PERF.lock();
    perf.as_ref().map(|s| {
        (
            s.stats.events_created.load(Ordering::Relaxed),
            s.stats.samples_collected.load(Ordering::Relaxed),
            s.stats.samples_lost.load(Ordering::Relaxed),
            s.stats.reads.load(Ordering::Relaxed),
        )
    })
}

/// ioctl commands for perf event FDs
pub mod ioctl {
    pub const PERF_EVENT_IOC_ENABLE: u32 = 0x2400;
    pub const PERF_EVENT_IOC_DISABLE: u32 = 0x2401;
    pub const PERF_EVENT_IOC_REFRESH: u32 = 0x2402;
    pub const PERF_EVENT_IOC_RESET: u32 = 0x2403;
    pub const PERF_EVENT_IOC_PERIOD: u32 = 0x40082404;
    pub const PERF_EVENT_IOC_SET_OUTPUT: u32 = 0x2405;
    pub const PERF_EVENT_IOC_SET_FILTER: u32 = 0x40082406;
    pub const PERF_EVENT_IOC_ID: u32 = 0x80082407;
    pub const PERF_EVENT_IOC_SET_BPF: u32 = 0x40042408;
    pub const PERF_EVENT_IOC_PAUSE_OUTPUT: u32 = 0x40042409;
    pub const PERF_EVENT_IOC_QUERY_BPF: u32 = 0xc008240a;
    pub const PERF_EVENT_IOC_MODIFY_ATTRIBUTES: u32 = 0x4008240b;
}
