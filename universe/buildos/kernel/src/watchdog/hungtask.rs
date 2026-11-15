// ===============================================================================
// BUILDOS KERNEL - HUNG TASK DETECTOR
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Hung task detection.
//!
//! Detects tasks that have been stuck in uninterruptible sleep (D state)
//! for too long. This typically indicates a bug or deadlock.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::{Mutex, RwLock};

use crate::process::Tid;

// =============================================================================
// CONSTANTS
// =============================================================================

/// Default check interval (60 seconds)
const CHECK_INTERVAL_NS: u64 = 60_000_000_000;

/// Maximum number of warnings per task
const MAX_WARNINGS_PER_TASK: u32 = 3;

// =============================================================================
// STATE
// =============================================================================

/// Global hung task detector
static HUNG_TASK_DETECTOR: RwLock<HungTaskDetector> = RwLock::new(HungTaskDetector::new());

/// Hung task detector
pub struct HungTaskDetector {
    /// Is enabled?
    enabled: AtomicBool,

    /// Timeout threshold (ns)
    timeout_ns: AtomicU64,

    /// Warning threshold (ns) - warn before full timeout
    warning_ns: AtomicU64,

    /// Check interval (ns)
    check_interval_ns: AtomicU64,

    /// Last check timestamp
    last_check_ts: AtomicU64,

    /// Panic on hung task
    panic_on_detect: AtomicBool,

    /// Number of hung tasks detected
    hung_count: AtomicU64,

    /// Number of warnings issued
    warning_count: AtomicU64,

    /// Tracked tasks in D state
    tracked_tasks: Mutex<BTreeMap<Tid, TaskSleepInfo>>,

    /// Tasks exempted from hung task detection
    exempt_tasks: Mutex<Vec<Tid>>,
}

impl HungTaskDetector {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(true),
            timeout_ns: AtomicU64::new(super::DEFAULT_HUNG_TASK_TIMEOUT_NS),
            warning_ns: AtomicU64::new(super::DEFAULT_HUNG_TASK_TIMEOUT_NS / 2),
            check_interval_ns: AtomicU64::new(CHECK_INTERVAL_NS),
            last_check_ts: AtomicU64::new(0),
            panic_on_detect: AtomicBool::new(false),
            hung_count: AtomicU64::new(0),
            warning_count: AtomicU64::new(0),
            tracked_tasks: Mutex::new(BTreeMap::new()),
            exempt_tasks: Mutex::new(Vec::new()),
        }
    }
}

/// Information about a task's sleep
#[derive(Clone)]
pub struct TaskSleepInfo {
    /// Task ID
    pub tid: Tid,

    /// Task name/command
    pub comm: String,

    /// When sleep started (ns since boot)
    pub sleep_start_ns: u64,

    /// What the task is waiting for
    pub wait_channel: WaitChannel,

    /// Number of warnings issued
    pub warnings: u32,

    /// Is exempted from timeout
    pub exempted: bool,

    /// Stack trace at sleep
    pub backtrace: [u64; 16],
}

/// What a task is waiting for
#[derive(Clone)]
pub enum WaitChannel {
    /// Unknown wait channel
    Unknown,
    /// Waiting for disk I/O
    DiskIo { device: u32 },
    /// Waiting for network I/O
    NetworkIo,
    /// Waiting for lock
    Lock { address: u64 },
    /// Waiting for page fault
    PageFault { address: u64 },
    /// Waiting for signal
    Signal,
    /// Waiting for child process
    Wait,
    /// Waiting for memory
    Memory,
    /// Waiting for pipe
    Pipe,
    /// Custom wait
    Custom(String),
}

/// Hung task event
pub struct HungTaskEvent {
    pub tid: Tid,
    pub comm: String,
    pub duration_ns: u64,
    pub wait_channel: WaitChannel,
    pub backtrace: [u64; 16],
}

// =============================================================================
// INTERFACE
// =============================================================================

/// Initialize hung task detector
pub fn init() {
    let detector = HUNG_TASK_DETECTOR.write();
    detector.enabled.store(true, Ordering::Release);
    detector.last_check_ts.store(crate::time::now_ns(), Ordering::Release);
}

/// Check for hung tasks (called periodically)
pub fn check() -> Vec<HungTaskEvent> {
    let detector = HUNG_TASK_DETECTOR.read();

    if !detector.enabled.load(Ordering::Acquire) {
        return Vec::new();
    }

    let now = crate::time::now_ns();
    let last_check = detector.last_check_ts.load(Ordering::Acquire);
    let interval = detector.check_interval_ns.load(Ordering::Relaxed);

    // Check if it's time for a check
    if now.saturating_sub(last_check) < interval {
        return Vec::new();
    }

    detector.last_check_ts.store(now, Ordering::Release);

    let timeout = detector.timeout_ns.load(Ordering::Relaxed);
    let warning = detector.warning_ns.load(Ordering::Relaxed);
    let panic_on_detect = detector.panic_on_detect.load(Ordering::Relaxed);

    let mut events = Vec::new();
    let mut tasks = detector.tracked_tasks.lock();

    for (tid, info) in tasks.iter_mut() {
        if info.exempted {
            continue;
        }

        let duration = now.saturating_sub(info.sleep_start_ns);

        if duration > timeout {
            // Full timeout - task is hung
            detector.hung_count.fetch_add(1, Ordering::Relaxed);

            events.push(HungTaskEvent {
                tid: *tid,
                comm: info.comm.clone(),
                duration_ns: duration,
                wait_channel: info.wait_channel.clone(),
                backtrace: info.backtrace,
            });

            crate::kprintln!(
                "!!! HUNG TASK: {} ({}) blocked for {}s in {:?}",
                info.comm,
                tid.as_u64(),
                duration / 1_000_000_000,
                wait_channel_name(&info.wait_channel)
            );

            if panic_on_detect {
                panic!("Hung task detected: {} ({})", info.comm, tid.as_u64());
            }
        } else if duration > warning && info.warnings < MAX_WARNINGS_PER_TASK {
            // Warning threshold
            info.warnings += 1;
            detector.warning_count.fetch_add(1, Ordering::Relaxed);

            crate::kprintln!(
                "[WATCHDOG] Warning: {} ({}) blocked for {}s in {:?}",
                info.comm,
                tid.as_u64(),
                duration / 1_000_000_000,
                wait_channel_name(&info.wait_channel)
            );
        }
    }

    events
}

/// Get wait channel name for display
fn wait_channel_name(wc: &WaitChannel) -> &'static str {
    match wc {
        WaitChannel::Unknown => "unknown",
        WaitChannel::DiskIo { .. } => "disk_io",
        WaitChannel::NetworkIo => "network_io",
        WaitChannel::Lock { .. } => "lock",
        WaitChannel::PageFault { .. } => "page_fault",
        WaitChannel::Signal => "signal",
        WaitChannel::Wait => "wait",
        WaitChannel::Memory => "memory",
        WaitChannel::Pipe => "pipe",
        WaitChannel::Custom(_) => "custom",
    }
}

/// Track a task entering D state
pub fn task_entering_d_state(tid: Tid, comm: &str, wait_channel: WaitChannel) {
    let detector = HUNG_TASK_DETECTOR.read();

    if !detector.enabled.load(Ordering::Acquire) {
        return;
    }

    // Check if task is exempt
    let exempt = detector.exempt_tasks.lock().contains(&tid);

    let info = TaskSleepInfo {
        tid,
        comm: String::from(comm),
        sleep_start_ns: crate::time::now_ns(),
        wait_channel,
        warnings: 0,
        exempted: exempt,
        backtrace: capture_backtrace(),
    };

    detector.tracked_tasks.lock().insert(tid, info);
}

/// Track a task leaving D state
pub fn task_leaving_d_state(tid: Tid) {
    let detector = HUNG_TASK_DETECTOR.read();
    detector.tracked_tasks.lock().remove(&tid);
}

/// Exempt a task from hung task detection
pub fn exempt_task(tid: Tid) {
    let detector = HUNG_TASK_DETECTOR.read();
    {
        let mut exempt = detector.exempt_tasks.lock();
        if !exempt.contains(&tid) {
            exempt.push(tid);
        }
    }

    // Update tracking info if already tracked
    {
        let mut tracked = detector.tracked_tasks.lock();
        if let Some(info) = tracked.get_mut(&tid) {
            info.exempted = true;
        }
    }
}

/// Remove exemption
pub fn unexempt_task(tid: Tid) {
    let detector = HUNG_TASK_DETECTOR.read();
    detector.exempt_tasks.lock().retain(|&t| t != tid);

    {
        let mut tracked = detector.tracked_tasks.lock();
        if let Some(info) = tracked.get_mut(&tid) {
            info.exempted = false;
        }
    }
}

/// Capture backtrace
fn capture_backtrace() -> [u64; 16] {
    // Would walk the stack to capture return addresses
    [0u64; 16]
}

// =============================================================================
// CONFIGURATION
// =============================================================================

/// Set timeout threshold
pub fn set_timeout(timeout_ns: u64) {
    HUNG_TASK_DETECTOR.read().timeout_ns.store(timeout_ns, Ordering::Release);
}

/// Get timeout threshold
pub fn get_timeout() -> u64 {
    HUNG_TASK_DETECTOR.read().timeout_ns.load(Ordering::Relaxed)
}

/// Set warning threshold
pub fn set_warning_threshold(warning_ns: u64) {
    HUNG_TASK_DETECTOR.read().warning_ns.store(warning_ns, Ordering::Release);
}

/// Set check interval
pub fn set_check_interval(interval_ns: u64) {
    HUNG_TASK_DETECTOR.read().check_interval_ns.store(interval_ns, Ordering::Release);
}

/// Enable/disable hung task detection
pub fn set_enabled(enabled: bool) {
    HUNG_TASK_DETECTOR.read().enabled.store(enabled, Ordering::Release);
}

/// Check if enabled
pub fn is_enabled() -> bool {
    HUNG_TASK_DETECTOR.read().enabled.load(Ordering::Relaxed)
}

/// Set panic on detect
pub fn set_panic_on_detect(panic: bool) {
    HUNG_TASK_DETECTOR.read().panic_on_detect.store(panic, Ordering::Release);
}

// =============================================================================
// STATISTICS
// =============================================================================

/// Hung task statistics
#[derive(Default)]
pub struct HungTaskStats {
    pub hung_count: u64,
    pub warning_count: u64,
    pub currently_tracked: usize,
    pub exempted_count: usize,
}

/// Get statistics
pub fn get_stats() -> HungTaskStats {
    let detector = HUNG_TASK_DETECTOR.read();
    let hung_count = detector.hung_count.load(Ordering::Relaxed);
    let warning_count = detector.warning_count.load(Ordering::Relaxed);
    let currently_tracked = detector.tracked_tasks.lock().len();
    let exempted_count = detector.exempt_tasks.lock().len();
    HungTaskStats {
        hung_count,
        warning_count,
        currently_tracked,
        exempted_count,
    }
}

/// Get list of currently tracked tasks
pub fn get_tracked_tasks() -> Vec<TaskSleepInfo> {
    HUNG_TASK_DETECTOR.read()
        .tracked_tasks
        .lock()
        .values()
        .cloned()
        .collect()
}
