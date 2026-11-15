// ===============================================================================
// BUILDOS KERNEL - WORKQUEUE
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! Kernel workqueue for deferred work execution.
//!
//! Workqueues allow kernel code to defer work to be executed later
//! in process context, making them essential for:
//! - Interrupt bottom halves (work that can't run in IRQ context)
//! - Timer callbacks that need to sleep
//! - Async I/O completion
//! - Periodic background tasks
//!
//! Types of workqueues:
//! - System workqueue: Shared by all kernel code
//! - Bound workqueue: Tied to specific CPUs
//! - Unbound workqueue: Can run on any CPU
//! - High-priority: For time-sensitive work
//! - Freezable: Suspended during system suspend

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::{Mutex, RwLock};


// =============================================================================
// CONSTANTS
// =============================================================================

/// Maximum pending work items per queue
const MAX_WORK_ITEMS: usize = 4096;

/// Default worker pool size
const DEFAULT_WORKERS: usize = 4;

/// High priority worker count
const HIGH_PRIO_WORKERS: usize = 2;

// =============================================================================
// WORK ITEM
// =============================================================================

/// Work item callback type
pub type WorkFn = Box<dyn FnOnce() + Send + 'static>;

/// Work item
pub struct Work {
    /// Unique ID
    id: u64,

    /// Work function
    func: WorkFn,

    /// Priority (lower = higher priority)
    priority: i32,

    /// Delay before execution (in ticks)
    delay: u64,

    /// Target CPU (-1 for any)
    cpu: i32,

    /// Enqueue timestamp
    enqueue_time: u64,

    /// Work is pending
    pending: AtomicBool,
}

impl Work {
    /// Create new work item
    pub fn new<F: FnOnce() + Send + 'static>(func: F) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);

        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            func: Box::new(func),
            priority: 0,
            delay: 0,
            cpu: -1,
            enqueue_time: crate::time::now_ns(),
            pending: AtomicBool::new(true),
        }
    }

    /// Create delayed work item
    pub fn delayed<F: FnOnce() + Send + 'static>(func: F, delay_ns: u64) -> Self {
        let mut work = Self::new(func);
        work.delay = delay_ns;
        work
    }

    /// Set priority
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Set target CPU
    pub fn with_cpu(mut self, cpu: i32) -> Self {
        self.cpu = cpu;
        self
    }

    /// Execute the work
    fn execute(self) {
        (self.func)();
    }

    /// Check if work is ready to execute
    fn is_ready(&self) -> bool {
        if self.delay == 0 {
            return true;
        }

        let elapsed = crate::time::now_ns().saturating_sub(self.enqueue_time);
        elapsed >= self.delay
    }
}

// =============================================================================
// WORKQUEUE
// =============================================================================

/// Workqueue flags
#[derive(Clone, Copy, Debug, Default)]
pub struct WqFlags {
    /// Unbound (can run on any CPU)
    pub unbound: bool,
    /// High priority
    pub high_priority: bool,
    /// Freezable (for suspend)
    pub freezable: bool,
    /// Single-threaded (only one worker)
    pub single_threaded: bool,
    /// Memory reclaim safe
    pub mem_reclaim: bool,
    /// Allow running on draining CPU
    pub cpu_intensive: bool,
}

impl WqFlags {
    pub const fn bound() -> Self {
        Self {
            unbound: false,
            high_priority: false,
            freezable: false,
            single_threaded: false,
            mem_reclaim: false,
            cpu_intensive: false,
        }
    }

    pub const fn unbound() -> Self {
        Self {
            unbound: true,
            high_priority: false,
            freezable: false,
            single_threaded: false,
            mem_reclaim: false,
            cpu_intensive: false,
        }
    }

    pub const fn high_priority() -> Self {
        Self {
            unbound: false,
            high_priority: true,
            freezable: false,
            single_threaded: false,
            mem_reclaim: false,
            cpu_intensive: false,
        }
    }
}

/// Per-CPU work pool
struct WorkPool {
    /// CPU this pool is bound to (-1 for unbound)
    cpu: i32,

    /// Pending work items
    pending: Mutex<VecDeque<Work>>,

    /// Delayed work items
    delayed: Mutex<Vec<Work>>,

    /// Number of idle workers
    idle_workers: AtomicU32,

    /// Number of busy workers
    busy_workers: AtomicU32,

    /// Work items processed
    work_processed: AtomicU64,

    /// Total execution time (ns)
    total_exec_time: AtomicU64,

    /// Pool is active
    active: AtomicBool,
}

impl WorkPool {
    fn new(cpu: i32) -> Self {
        Self {
            cpu,
            pending: Mutex::new(VecDeque::new()),
            delayed: Mutex::new(Vec::new()),
            idle_workers: AtomicU32::new(0),
            busy_workers: AtomicU32::new(0),
            work_processed: AtomicU64::new(0),
            total_exec_time: AtomicU64::new(0),
            active: AtomicBool::new(true),
        }
    }

    /// Queue work to this pool
    fn queue_work(&self, work: Work) {
        if work.delay > 0 && !work.is_ready() {
            self.delayed.lock().push(work);
        } else {
            self.pending.lock().push_back(work);
        }
    }

    /// Get next work item
    fn get_work(&self) -> Option<Work> {
        // Check pending queue first
        if let Some(work) = self.pending.lock().pop_front() {
            return Some(work);
        }

        // Check delayed work
        let mut delayed = self.delayed.lock();
        let mut ready_idx = None;

        for (i, work) in delayed.iter().enumerate() {
            if work.is_ready() {
                ready_idx = Some(i);
                break;
            }
        }

        if let Some(idx) = ready_idx {
            return Some(delayed.remove(idx));
        }

        None
    }

    /// Execute work from this pool
    fn process_work(&self) {
        while let Some(work) = self.get_work() {
            self.idle_workers.fetch_sub(1, Ordering::Relaxed);
            self.busy_workers.fetch_add(1, Ordering::Relaxed);

            let start = crate::time::now_ns();
            work.execute();
            let elapsed = crate::time::now_ns().saturating_sub(start);

            self.busy_workers.fetch_sub(1, Ordering::Relaxed);
            self.idle_workers.fetch_add(1, Ordering::Relaxed);

            self.work_processed.fetch_add(1, Ordering::Relaxed);
            self.total_exec_time.fetch_add(elapsed, Ordering::Relaxed);
        }
    }
}

/// Workqueue structure
pub struct Workqueue {
    /// Workqueue name
    name: String,

    /// Flags
    flags: WqFlags,

    /// Per-CPU work pools (for bound workqueues)
    cpu_pools: Vec<WorkPool>,

    /// Unbound work pool
    unbound_pool: Option<WorkPool>,

    /// Max concurrent workers
    max_workers: u32,

    /// Workqueue is running
    running: AtomicBool,

    /// Work items queued
    queued: AtomicU64,

    /// Work items completed
    completed: AtomicU64,
}

impl Workqueue {
    /// Create new workqueue
    pub fn new(name: &str, flags: WqFlags, max_workers: u32) -> Arc<Self> {
        let mut wq = Self {
            name: String::from(name),
            flags,
            cpu_pools: Vec::new(),
            unbound_pool: None,
            max_workers,
            running: AtomicBool::new(true),
            queued: AtomicU64::new(0),
            completed: AtomicU64::new(0),
        };

        if flags.unbound {
            wq.unbound_pool = Some(WorkPool::new(-1));
        } else {
            // Create per-CPU pools
            let num_cpus = crate::cpu::smp::nr_cpus();
            for cpu in 0..num_cpus {
                wq.cpu_pools.push(WorkPool::new(cpu as i32));
            }
        }

        Arc::new(wq)
    }

    /// Queue work to this workqueue
    pub fn queue(&self, work: Work) -> bool {
        if !self.running.load(Ordering::Relaxed) {
            return false;
        }

        self.queued.fetch_add(1, Ordering::Relaxed);

        if let Some(ref pool) = self.unbound_pool {
            pool.queue_work(work);
            return true;
        }

        // For bound workqueues, pick the target CPU's pool
        let cpu = if work.cpu >= 0 {
            work.cpu as usize
        } else {
            crate::cpu::current_cpu_id() as usize
        };

        if cpu < self.cpu_pools.len() {
            self.cpu_pools[cpu].queue_work(work);
            true
        } else {
            false
        }
    }

    /// Queue simple function
    pub fn queue_fn<F: FnOnce() + Send + 'static>(&self, func: F) -> bool {
        self.queue(Work::new(func))
    }

    /// Queue delayed work
    pub fn queue_delayed<F: FnOnce() + Send + 'static>(&self, func: F, delay_ns: u64) -> bool {
        self.queue(Work::delayed(func, delay_ns))
    }

    /// Flush all pending work
    pub fn flush(&self) {
        // Process all work in all pools
        if let Some(ref pool) = self.unbound_pool {
            pool.process_work();
        }

        for pool in &self.cpu_pools {
            pool.process_work();
        }
    }

    /// Drain and stop the workqueue
    pub fn drain(&self) {
        self.running.store(false, Ordering::Release);
        self.flush();
    }

    /// Get workqueue statistics
    pub fn stats(&self) -> WorkqueueStats {
        WorkqueueStats {
            name: self.name.clone(),
            queued: self.queued.load(Ordering::Relaxed),
            completed: self.completed.load(Ordering::Relaxed),
            pending: self.pending_count(),
        }
    }

    /// Count pending work items
    fn pending_count(&self) -> u64 {
        let mut count = 0;

        if let Some(ref pool) = self.unbound_pool {
            count += pool.pending.lock().len() as u64;
            count += pool.delayed.lock().len() as u64;
        }

        for pool in &self.cpu_pools {
            count += pool.pending.lock().len() as u64;
            count += pool.delayed.lock().len() as u64;
        }

        count
    }
}

/// Workqueue statistics
#[derive(Clone, Debug)]
pub struct WorkqueueStats {
    pub name: String,
    pub queued: u64,
    pub completed: u64,
    pub pending: u64,
}

// =============================================================================
// GLOBAL WORKQUEUES
// =============================================================================

/// System workqueue
static SYSTEM_WQ: RwLock<Option<Arc<Workqueue>>> = RwLock::new(None);

/// System high-priority workqueue
static SYSTEM_HIGHPRI_WQ: RwLock<Option<Arc<Workqueue>>> = RwLock::new(None);

/// System unbound workqueue
static SYSTEM_UNBOUND_WQ: RwLock<Option<Arc<Workqueue>>> = RwLock::new(None);

/// System freezable workqueue
static SYSTEM_FREEZABLE_WQ: RwLock<Option<Arc<Workqueue>>> = RwLock::new(None);

/// Initialized flag
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize workqueue subsystem
pub fn init() {
    // Create system workqueues
    *SYSTEM_WQ.write() = Some(Workqueue::new(
        "events",
        WqFlags::bound(),
        DEFAULT_WORKERS as u32,
    ));

    *SYSTEM_HIGHPRI_WQ.write() = Some(Workqueue::new(
        "events_highpri",
        WqFlags::high_priority(),
        HIGH_PRIO_WORKERS as u32,
    ));

    *SYSTEM_UNBOUND_WQ.write() = Some(Workqueue::new(
        "events_unbound",
        WqFlags::unbound(),
        DEFAULT_WORKERS as u32,
    ));

    let mut freezable_flags = WqFlags::bound();
    freezable_flags.freezable = true;
    *SYSTEM_FREEZABLE_WQ.write() = Some(Workqueue::new(
        "events_freezable",
        freezable_flags,
        DEFAULT_WORKERS as u32,
    ));

    INITIALIZED.store(true, Ordering::Release);
}

/// Get system workqueue
pub fn system_wq() -> Option<Arc<Workqueue>> {
    SYSTEM_WQ.read().clone()
}

/// Get high-priority workqueue
pub fn system_highpri_wq() -> Option<Arc<Workqueue>> {
    SYSTEM_HIGHPRI_WQ.read().clone()
}

/// Get unbound workqueue
pub fn system_unbound_wq() -> Option<Arc<Workqueue>> {
    SYSTEM_UNBOUND_WQ.read().clone()
}

/// Get freezable workqueue
pub fn system_freezable_wq() -> Option<Arc<Workqueue>> {
    SYSTEM_FREEZABLE_WQ.read().clone()
}

// =============================================================================
// CONVENIENCE FUNCTIONS
// =============================================================================

/// Schedule work on system workqueue
pub fn schedule_work<F: FnOnce() + Send + 'static>(func: F) -> bool {
    if let Some(wq) = system_wq() {
        wq.queue_fn(func)
    } else {
        // Fallback: execute immediately
        func();
        true
    }
}

/// Schedule delayed work on system workqueue
pub fn schedule_delayed_work<F: FnOnce() + Send + 'static>(func: F, delay_ns: u64) -> bool {
    if let Some(wq) = system_wq() {
        wq.queue_delayed(func, delay_ns)
    } else {
        false
    }
}

/// Schedule high-priority work
pub fn schedule_highpri_work<F: FnOnce() + Send + 'static>(func: F) -> bool {
    if let Some(wq) = system_highpri_wq() {
        wq.queue_fn(func)
    } else {
        schedule_work(func)
    }
}

/// Flush system workqueue
pub fn flush_scheduled_work() {
    if let Some(wq) = system_wq() {
        wq.flush();
    }
}

/// Process pending work (called from scheduler idle or timer)
pub fn run_workqueue() {
    if !INITIALIZED.load(Ordering::Relaxed) {
        return;
    }

    // Process work from each system workqueue
    if let Some(ref wq) = *SYSTEM_HIGHPRI_WQ.read() {
        process_workqueue_cpu(wq);
    }

    if let Some(ref wq) = *SYSTEM_WQ.read() {
        process_workqueue_cpu(wq);
    }

    if let Some(ref wq) = *SYSTEM_UNBOUND_WQ.read() {
        process_workqueue_cpu(wq);
    }

    if let Some(ref wq) = *SYSTEM_FREEZABLE_WQ.read() {
        process_workqueue_cpu(wq);
    }
}

/// Process workqueue for current CPU
fn process_workqueue_cpu(wq: &Workqueue) {
    let cpu = crate::cpu::current_cpu_id() as usize;

    if let Some(ref pool) = wq.unbound_pool {
        pool.process_work();
    }

    if cpu < wq.cpu_pools.len() {
        wq.cpu_pools[cpu].process_work();
    }
}

// =============================================================================
// KTHREAD WORK
// =============================================================================

/// Kthread work item (for work that needs dedicated thread)
pub struct KthreadWork {
    /// Work function
    func: WorkFn,

    /// Work queue
    queue: Mutex<VecDeque<Work>>,

    /// Thread is running
    running: AtomicBool,
}

impl KthreadWork {
    /// Create new kthread work
    pub fn new() -> Self {
        Self {
            func: Box::new(|| {}),
            queue: Mutex::new(VecDeque::new()),
            running: AtomicBool::new(false),
        }
    }

    /// Queue work
    pub fn queue(&self, work: Work) {
        self.queue.lock().push_back(work);
    }

    /// Process all queued work
    pub fn run(&self) {
        self.running.store(true, Ordering::Release);

        while let Some(work) = self.queue.lock().pop_front() {
            work.execute();
        }

        self.running.store(false, Ordering::Release);
    }
}

// =============================================================================
// TIMER-BASED DELAYED WORK
// =============================================================================

/// Delayed work with timer integration
pub struct DelayedWork {
    /// Underlying work
    work: Mutex<Option<Work>>,

    /// Timer handle
    timer_id: AtomicU64,

    /// Target workqueue
    wq: Option<Arc<Workqueue>>,
}

impl DelayedWork {
    /// Create new delayed work
    pub fn new() -> Self {
        Self {
            work: Mutex::new(None),
            timer_id: AtomicU64::new(0),
            wq: None,
        }
    }

    /// Set the work function
    pub fn set_work<F: FnOnce() + Send + 'static>(&self, func: F) {
        *self.work.lock() = Some(Work::new(func));
    }

    /// Schedule work with delay
    pub fn schedule(&self, _delay_ns: u64) {
        if self.work.lock().is_some() {
            if self.wq.is_some() {
                // Re-use scheduled work pattern
            } else if system_wq().is_some() {
                // Use system workqueue
            }
        }
    }

    /// Cancel pending work
    pub fn cancel(&self) -> bool {
        let timer_id = self.timer_id.load(Ordering::Acquire);
        if timer_id != 0 {
            // Would cancel timer here
            self.timer_id.store(0, Ordering::Release);
            true
        } else {
            false
        }
    }
}

// =============================================================================
// STATISTICS
// =============================================================================

/// Get all workqueue statistics
pub fn all_stats() -> Vec<WorkqueueStats> {
    let mut stats = Vec::new();

    if let Some(ref wq) = *SYSTEM_WQ.read() {
        stats.push(wq.stats());
    }

    if let Some(ref wq) = *SYSTEM_HIGHPRI_WQ.read() {
        stats.push(wq.stats());
    }

    if let Some(ref wq) = *SYSTEM_UNBOUND_WQ.read() {
        stats.push(wq.stats());
    }

    if let Some(ref wq) = *SYSTEM_FREEZABLE_WQ.read() {
        stats.push(wq.stats());
    }

    stats
}

/// Get total pending work across all queues
pub fn total_pending() -> u64 {
    let mut total = 0;

    if let Some(ref wq) = *SYSTEM_WQ.read() {
        total += wq.pending_count();
    }

    if let Some(ref wq) = *SYSTEM_HIGHPRI_WQ.read() {
        total += wq.pending_count();
    }

    if let Some(ref wq) = *SYSTEM_UNBOUND_WQ.read() {
        total += wq.pending_count();
    }

    if let Some(ref wq) = *SYSTEM_FREEZABLE_WQ.read() {
        total += wq.pending_count();
    }

    total
}
