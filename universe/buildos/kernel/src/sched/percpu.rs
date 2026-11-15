// ===============================================================================
// BUILDOS KERNEL - PER-CPU RUN QUEUES
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! Per-CPU run queues for SMP scheduling.
//!
//! Each CPU has its own run queue to minimize lock contention.
//! Threads are typically scheduled on the same CPU (cache affinity).

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, AtomicU32, AtomicBool, Ordering};
use spin::Mutex;

use crate::process::Tid;
use super::{SchedClass, SchedParams, PRIORITY_LEVELS, MAX_CPUS, DEFAULT_TIME_SLICE_NS};

// =============================================================================
// PER-CPU STRUCTURES
// =============================================================================

/// Per-CPU scheduler data
static mut CPU_RQS: [Option<CpuRunQueue>; MAX_CPUS] = {
    const NONE: Option<CpuRunQueue> = None;
    [NONE; MAX_CPUS]
};

/// Per-CPU run queue
pub struct CpuRunQueue {
    /// CPU ID
    cpu: usize,

    /// Per-class run queues
    class_queues: [ClassQueue; 7],

    /// Currently running thread
    current: Mutex<Option<Tid>>,

    /// Idle thread for this CPU
    idle_thread: Option<Tid>,

    /// Run queue lock
    lock: Mutex<()>,

    /// Number of runnable threads
    nr_running: AtomicU32,

    /// Total load (weighted)
    load: AtomicU64,

    /// CPU statistics
    stats: CpuStats,

    /// Last schedule timestamp
    last_schedule: AtomicU64,

    /// Time slice remaining for current thread
    time_slice_remaining: AtomicU64,

    /// Need reschedule flag
    need_resched: AtomicBool,

    /// CPU is idle
    is_idle: AtomicBool,
}

/// Per-scheduling-class queue
pub struct ClassQueue {
    /// Scheduling class
    class: SchedClass,

    /// Priority queues (for RT) or CFS tree
    queues: PriorityQueues,

    /// Number of threads in this class
    nr_threads: AtomicU32,
}

/// Priority-based run queues
pub struct PriorityQueues {
    /// Priority bitmap (which levels have threads)
    bitmap: AtomicU64,

    /// Per-priority thread lists
    levels: [Mutex<Vec<Tid>>; PRIORITY_LEVELS],

    /// For CFS: threads sorted by vruntime
    cfs_tree: Mutex<BTreeMap<u64, Tid>>,

    /// Min vruntime (for CFS)
    min_vruntime: AtomicU64,
}

/// Per-CPU statistics
#[derive(Default)]
pub struct CpuStats {
    /// Context switches on this CPU
    pub context_switches: AtomicU64,
    /// Voluntary switches
    pub voluntary_switches: AtomicU64,
    /// Involuntary switches (preemptions)
    pub involuntary_switches: AtomicU64,
    /// Times CPU went idle
    pub idle_entries: AtomicU64,
    /// Total idle time (ns)
    pub idle_time_ns: AtomicU64,
    /// Migrations into this CPU
    pub migrations: AtomicU64,
    /// Total runtime on this CPU (ns)
    pub total_runtime: AtomicU64,
}

// =============================================================================
// IMPLEMENTATION
// =============================================================================

impl CpuRunQueue {
    /// Create new per-CPU run queue
    pub fn new(cpu: usize) -> Self {
        Self {
            cpu,
            class_queues: [
                ClassQueue::new(SchedClass::Idle),
                ClassQueue::new(SchedClass::Normal),
                ClassQueue::new(SchedClass::Batch),
                ClassQueue::new(SchedClass::AI),
                ClassQueue::new(SchedClass::RealTimeRR),
                ClassQueue::new(SchedClass::RealTimeFIFO),
                ClassQueue::new(SchedClass::Deadline),
            ],
            current: Mutex::new(None),
            idle_thread: None,
            lock: Mutex::new(()),
            nr_running: AtomicU32::new(0),
            load: AtomicU64::new(0),
            stats: CpuStats::default(),
            last_schedule: AtomicU64::new(0),
            time_slice_remaining: AtomicU64::new(DEFAULT_TIME_SLICE_NS),
            need_resched: AtomicBool::new(false),
            is_idle: AtomicBool::new(true),
        }
    }

    /// Enqueue a thread
    pub fn enqueue(&self, tid: Tid, params: &SchedParams) {
        let _lock = self.lock.lock();

        let class_idx = params.class as usize;
        if class_idx < self.class_queues.len() {
            self.class_queues[class_idx].enqueue(tid, params);
            self.nr_running.fetch_add(1, Ordering::Relaxed);
            self.update_load(params, true);

            // Set need_resched if higher priority than current
            if self.should_preempt(params) {
                self.need_resched.store(true, Ordering::Release);
            }
        }
    }

    /// Dequeue a specific thread
    pub fn dequeue(&self, tid: Tid) -> bool {
        let _lock = self.lock.lock();

        for queue in &self.class_queues {
            if queue.dequeue(tid) {
                self.nr_running.fetch_sub(1, Ordering::Relaxed);
                return true;
            }
        }
        false
    }

    /// Pick next thread to run
    pub fn pick_next(&self) -> Option<Tid> {
        let _lock = self.lock.lock();

        // Check classes in priority order (highest first)
        for class in [
            SchedClass::Deadline,
            SchedClass::RealTimeFIFO,
            SchedClass::RealTimeRR,
            SchedClass::AI,
            SchedClass::Normal,
            SchedClass::Batch,
            SchedClass::Idle,
        ] {
            let class_idx = class as usize;
            if class_idx < self.class_queues.len() {
                if let Some(tid) = self.class_queues[class_idx].pick_next() {
                    return Some(tid);
                }
            }
        }

        // No runnable threads, return idle thread
        self.idle_thread
    }

    /// Put previous thread back (after being preempted)
    pub fn put_prev(&self, tid: Tid, params: &SchedParams) {
        let _lock = self.lock.lock();

        let class_idx = params.class as usize;
        if class_idx < self.class_queues.len() {
            self.class_queues[class_idx].put_prev(tid, params);
        }
    }

    /// Check if new thread should preempt current
    fn should_preempt(&self, new_params: &SchedParams) -> bool {
        // RT always preempts normal
        if matches!(new_params.class, SchedClass::RealTimeFIFO | SchedClass::RealTimeRR | SchedClass::Deadline) {
            return true;
        }
        false
    }

    /// Update CPU load
    fn update_load(&self, params: &SchedParams, adding: bool) {
        let weight = Self::priority_to_weight(params.priority);
        if adding {
            self.load.fetch_add(weight, Ordering::Relaxed);
        } else {
            self.load.fetch_sub(weight.min(self.load.load(Ordering::Relaxed)), Ordering::Relaxed);
        }
    }

    /// Convert priority to weight
    fn priority_to_weight(priority: i32) -> u64 {
        // Higher priority = higher weight
        (PRIORITY_LEVELS as i32 - priority).max(1) as u64 * 100
    }

    /// Timer tick - decrement time slice
    pub fn timer_tick(&self) {
        let remaining = self.time_slice_remaining.load(Ordering::Relaxed);
        if remaining > 0 {
            let new_remaining = remaining.saturating_sub(1_000_000); // 1ms tick
            self.time_slice_remaining.store(new_remaining, Ordering::Relaxed);

            if new_remaining == 0 {
                self.need_resched.store(true, Ordering::Release);
            }
        }
    }

    /// Schedule - pick and switch to next thread
    pub fn schedule(&self) {
        let _lock = self.lock.lock();

        // Clear need_resched
        self.need_resched.store(false, Ordering::Release);

        // Get current thread
        let mut current_guard = self.current.lock();
        let old_tid = *current_guard;

        // Pick next thread
        let new_tid = self.pick_next_unlocked();

        if new_tid != old_tid {
            // Update statistics
            self.stats.context_switches.fetch_add(1, Ordering::Relaxed);

            // Put old thread back if it was running
            if let Some(old) = old_tid {
                // Re-enqueue if still runnable
                // self.enqueue_unlocked(old, ...);
                let _ = old;
            }

            // Switch to new thread
            *current_guard = new_tid;
            drop(current_guard);

            if let Some(new) = new_tid {
                self.is_idle.store(false, Ordering::Release);
                self.time_slice_remaining.store(DEFAULT_TIME_SLICE_NS, Ordering::Relaxed);

                // Perform context switch
                self.do_context_switch(old_tid, new);
            } else {
                // Going idle
                self.is_idle.store(true, Ordering::Release);
                self.stats.idle_entries.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Pick next without lock (called with lock held)
    fn pick_next_unlocked(&self) -> Option<Tid> {
        for class in [
            SchedClass::Deadline,
            SchedClass::RealTimeFIFO,
            SchedClass::RealTimeRR,
            SchedClass::AI,
            SchedClass::Normal,
            SchedClass::Batch,
            SchedClass::Idle,
        ] {
            let class_idx = class as usize;
            if class_idx < self.class_queues.len() {
                if let Some(tid) = self.class_queues[class_idx].pick_next_unlocked() {
                    return Some(tid);
                }
            }
        }
        self.idle_thread
    }

    /// Perform actual context switch
    fn do_context_switch(&self, _old: Option<Tid>, _new: Tid) {
        // Would call architecture-specific context switch
        // crate::context::context_switch(old_ctx, new_ctx);
    }

    /// Get current thread
    pub fn current(&self) -> Option<Tid> {
        *self.current.lock()
    }

    /// Get load value
    pub fn load(&self) -> u64 {
        self.load.load(Ordering::Relaxed)
    }

    /// Get number of runnable threads
    pub fn nr_running(&self) -> u32 {
        self.nr_running.load(Ordering::Relaxed)
    }

    /// Check if CPU is idle
    pub fn is_idle(&self) -> bool {
        self.is_idle.load(Ordering::Relaxed)
    }

    /// Get statistics
    pub fn stats(&self) -> &CpuStats {
        &self.stats
    }

    /// Yield current thread
    pub fn yield_current(&self) {
        let current = *self.current.lock();
        if current.is_some() {
            self.stats.voluntary_switches.fetch_add(1, Ordering::Relaxed);
            self.need_resched.store(true, Ordering::Release);
            self.schedule();
        }
    }

    /// Block current thread
    pub fn block_current(&self) {
        let mut current_guard = self.current.lock();
        if let Some(tid) = *current_guard {
            // Remove from current
            *current_guard = None;
            self.nr_running.fetch_sub(1, Ordering::Relaxed);
            self.stats.voluntary_switches.fetch_add(1, Ordering::Relaxed);
            drop(current_guard);

            // Schedule next
            self.schedule();
            let _ = tid;
        }
    }

    /// Wake up a thread
    pub fn wake_thread(&self, tid: Tid, params: &SchedParams) {
        self.enqueue(tid, params);
    }
}

impl ClassQueue {
    const fn new(class: SchedClass) -> Self {
        Self {
            class,
            queues: PriorityQueues::new(),
            nr_threads: AtomicU32::new(0),
        }
    }

    fn enqueue(&self, tid: Tid, params: &SchedParams) {
        match self.class {
            SchedClass::Normal | SchedClass::Batch | SchedClass::AI => {
                // CFS-style: insert by vruntime
                // For simplicity, using priority queue
                self.queues.enqueue_priority(tid, params.priority);
            }
            SchedClass::RealTimeFIFO | SchedClass::RealTimeRR => {
                // RT: strict priority
                self.queues.enqueue_priority(tid, params.rt_priority as i32);
            }
            SchedClass::Deadline => {
                // EDF: sorted by deadline
                // Using priority for now
                self.queues.enqueue_priority(tid, 0);
            }
            SchedClass::Idle => {
                self.queues.enqueue_priority(tid, 0);
            }
        }
        self.nr_threads.fetch_add(1, Ordering::Relaxed);
    }

    fn dequeue(&self, tid: Tid) -> bool {
        if self.queues.dequeue(tid) {
            self.nr_threads.fetch_sub(1, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    fn pick_next(&self) -> Option<Tid> {
        self.queues.pick_next()
    }

    fn pick_next_unlocked(&self) -> Option<Tid> {
        self.queues.pick_next()
    }

    fn put_prev(&self, tid: Tid, params: &SchedParams) {
        self.enqueue(tid, params);
    }
}

impl PriorityQueues {
    const fn new() -> Self {
        const EMPTY_VEC: Mutex<Vec<Tid>> = Mutex::new(Vec::new());

        Self {
            bitmap: AtomicU64::new(0),
            levels: [EMPTY_VEC; PRIORITY_LEVELS],
            cfs_tree: Mutex::new(BTreeMap::new()),
            min_vruntime: AtomicU64::new(0),
        }
    }

    fn enqueue_priority(&self, tid: Tid, priority: i32) {
        let level = (priority.clamp(0, PRIORITY_LEVELS as i32 - 1)) as usize;
        self.levels[level].lock().push(tid);
        self.bitmap.fetch_or(1 << level, Ordering::Relaxed);
    }

    fn dequeue(&self, tid: Tid) -> bool {
        for (i, level) in self.levels.iter().enumerate() {
            let mut guard = level.lock();
            if let Some(pos) = guard.iter().position(|&t| t == tid) {
                guard.remove(pos);
                if guard.is_empty() {
                    self.bitmap.fetch_and(!(1 << i), Ordering::Relaxed);
                }
                return true;
            }
        }
        false
    }

    fn pick_next(&self) -> Option<Tid> {
        let bitmap = self.bitmap.load(Ordering::Relaxed);
        if bitmap == 0 {
            return None;
        }

        // Find highest priority with threads (highest bit set)
        let level = 63 - bitmap.leading_zeros() as usize;
        if level < PRIORITY_LEVELS {
            let mut guard = self.levels[level].lock();
            if !guard.is_empty() {
                let tid = guard.remove(0);
                if guard.is_empty() {
                    self.bitmap.fetch_and(!(1 << level), Ordering::Relaxed);
                }
                return Some(tid);
            }
        }
        None
    }
}

// =============================================================================
// MODULE INTERFACE
// =============================================================================

/// Initialize per-CPU scheduler
pub fn init(num_cpus: usize) {
    for cpu in 0..num_cpus.min(MAX_CPUS) {
        unsafe {
            CPU_RQS[cpu] = Some(CpuRunQueue::new(cpu));
        }
    }
}

/// Initialize local CPU's scheduler
pub fn init_local(cpu: usize) {
    if cpu < MAX_CPUS {
        unsafe {
            if CPU_RQS[cpu].is_none() {
                CPU_RQS[cpu] = Some(CpuRunQueue::new(cpu));
            }
        }
    }
}

/// Enqueue thread on specific CPU
pub fn enqueue_cpu(cpu: usize, tid: Tid, params: &SchedParams) {
    if cpu < MAX_CPUS {
        unsafe {
            if let Some(ref rq) = CPU_RQS[cpu] {
                rq.enqueue(tid, params);
            }
        }
    }
}

/// Dequeue thread from specific CPU
pub fn dequeue_cpu(cpu: usize, tid: Tid) -> bool {
    if cpu < MAX_CPUS {
        unsafe {
            if let Some(ref rq) = CPU_RQS[cpu] {
                return rq.dequeue(tid);
            }
        }
    }
    false
}

/// Get CPU load
pub fn get_load(cpu: usize) -> u64 {
    if cpu < MAX_CPUS {
        unsafe {
            if let Some(ref rq) = CPU_RQS[cpu] {
                return rq.load();
            }
        }
    }
    u64::MAX
}

/// Timer tick on CPU
pub fn timer_tick(cpu: usize) {
    if cpu < MAX_CPUS {
        unsafe {
            if let Some(ref rq) = CPU_RQS[cpu] {
                rq.timer_tick();
                if rq.need_resched.load(Ordering::Acquire) {
                    rq.schedule();
                }
            }
        }
    }
}

/// Schedule on CPU
pub fn schedule(cpu: usize) {
    if cpu < MAX_CPUS {
        unsafe {
            if let Some(ref rq) = CPU_RQS[cpu] {
                rq.schedule();
            }
        }
    }
}

/// Yield current thread on CPU
pub fn yield_current(cpu: usize) {
    if cpu < MAX_CPUS {
        unsafe {
            if let Some(ref rq) = CPU_RQS[cpu] {
                rq.yield_current();
            }
        }
    }
}

/// Block current thread on CPU
pub fn block_current(cpu: usize) {
    if cpu < MAX_CPUS {
        unsafe {
            if let Some(ref rq) = CPU_RQS[cpu] {
                rq.block_current();
            }
        }
    }
}

/// Wake thread on CPU
pub fn wake_thread(cpu: usize, tid: Tid) {
    if cpu < MAX_CPUS {
        unsafe {
            if let Some(ref rq) = CPU_RQS[cpu] {
                let params = SchedParams::default();
                rq.wake_thread(tid, &params);
            }
        }
    }
}

/// Get CPU statistics
pub fn get_stats(cpu: usize) -> CpuSchedStats {
    if cpu < MAX_CPUS {
        unsafe {
            if let Some(ref rq) = CPU_RQS[cpu] {
                return CpuSchedStats {
                    context_switches: rq.stats.context_switches.load(Ordering::Relaxed),
                    voluntary_switches: rq.stats.voluntary_switches.load(Ordering::Relaxed),
                    involuntary_switches: rq.stats.involuntary_switches.load(Ordering::Relaxed),
                    migrations: rq.stats.migrations.load(Ordering::Relaxed),
                    nr_running: rq.nr_running.load(Ordering::Relaxed),
                    load: rq.load.load(Ordering::Relaxed),
                };
            }
        }
    }

    CpuSchedStats::default()
}

/// Per-CPU scheduler statistics
#[derive(Default)]
pub struct CpuSchedStats {
    pub context_switches: u64,
    pub voluntary_switches: u64,
    pub involuntary_switches: u64,
    pub migrations: u64,
    pub nr_running: u32,
    pub load: u64,
}

/// Check if CPU is idle
pub fn is_cpu_idle(cpu: usize) -> bool {
    if cpu < MAX_CPUS {
        unsafe {
            if let Some(ref rq) = CPU_RQS[cpu] {
                return rq.is_idle();
            }
        }
    }
    true
}

/// Get current thread on CPU
pub fn current_on_cpu(cpu: usize) -> Option<Tid> {
    if cpu < MAX_CPUS {
        unsafe {
            if let Some(ref rq) = CPU_RQS[cpu] {
                return rq.current();
            }
        }
    }
    None
}
