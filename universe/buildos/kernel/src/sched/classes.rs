// ===============================================================================
// BUILDOS KERNEL - SCHEDULING CLASSES
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! Scheduling class implementations.
//!
//! This module provides:
//! - CFS (Completely Fair Scheduler) for normal tasks
//! - Real-time scheduling (FIFO and Round-Robin)
//! - Deadline scheduling (EDF)
//! - AI-optimized scheduling for neural workloads

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::math::F64Ext;
use crate::process::Tid;
use super::{SchedParams, DeadlineParams};

// =============================================================================
// CFS (COMPLETELY FAIR SCHEDULER)
// =============================================================================

/// CFS scheduler state
pub struct CfsScheduler {
    /// Red-black tree of tasks by vruntime
    tasks: Mutex<BTreeMap<u64, Vec<Tid>>>,

    /// Minimum vruntime in tree
    min_vruntime: AtomicU64,

    /// Target latency (ns)
    target_latency_ns: u64,

    /// Minimum granularity (ns)
    min_granularity_ns: u64,

    /// Number of running tasks
    nr_running: AtomicU64,
}

impl CfsScheduler {
    pub const fn new() -> Self {
        Self {
            tasks: Mutex::new(BTreeMap::new()),
            min_vruntime: AtomicU64::new(0),
            target_latency_ns: 6_000_000,  // 6ms
            min_granularity_ns: 750_000,   // 0.75ms
            nr_running: AtomicU64::new(0),
        }
    }

    /// Enqueue a task
    pub fn enqueue(&self, tid: Tid, vruntime: u64) {
        let mut tasks = self.tasks.lock();
        tasks.entry(vruntime).or_insert_with(Vec::new).push(tid);
        self.nr_running.fetch_add(1, Ordering::Relaxed);

        // Update min_vruntime
        if let Some((&min_vrt, _)) = tasks.first_key_value() {
            self.min_vruntime.store(min_vrt, Ordering::Relaxed);
        }
    }

    /// Dequeue a specific task
    pub fn dequeue(&self, tid: Tid) -> bool {
        let mut tasks = self.tasks.lock();

        for (vrt, tids) in tasks.iter_mut() {
            if let Some(pos) = tids.iter().position(|&t| t == tid) {
                tids.remove(pos);
                if tids.is_empty() {
                    let vrt = *vrt;
                    tasks.remove(&vrt);
                }
                self.nr_running.fetch_sub(1, Ordering::Relaxed);
                return true;
            }
        }
        false
    }

    /// Pick next task (lowest vruntime)
    pub fn pick_next(&self) -> Option<(Tid, u64)> {
        let tasks = self.tasks.lock();

        if let Some((&vruntime, tids)) = tasks.first_key_value() {
            if !tids.is_empty() {
                let tid = tids[0];
                return Some((tid, vruntime));
            }
        }
        None
    }

    /// Calculate time slice for task
    pub fn calculate_slice(&self, _nice: i32) -> u64 {
        let nr = self.nr_running.load(Ordering::Relaxed).max(1);

        // Divide target latency among tasks
        let slice = self.target_latency_ns / nr;

        // But not less than minimum granularity
        slice.max(self.min_granularity_ns)
    }

    /// Update vruntime after running
    pub fn update_vruntime(&self, tid: Tid, delta_ns: u64, nice: i32) {
        // Dequeue from old position
        self.dequeue(tid);

        // Calculate weighted vruntime delta
        let weight = nice_to_weight(nice);
        let vruntime_delta = delta_ns * 1024 / weight;

        // New vruntime
        let new_vruntime = self.min_vruntime.load(Ordering::Relaxed) + vruntime_delta;

        // Re-enqueue at new position
        self.enqueue(tid, new_vruntime);
    }

    /// Get min vruntime
    pub fn min_vruntime(&self) -> u64 {
        self.min_vruntime.load(Ordering::Relaxed)
    }
}

/// Convert nice value to weight
fn nice_to_weight(nice: i32) -> u64 {
    // Linux-like weight table (simplified)
    // Nice -20 = weight 88761, Nice 0 = 1024, Nice 19 = 15
    let base: u64 = 1024;
    let factor = 1.25_f64.powi(-nice);
    (base as f64 * factor) as u64
}

// =============================================================================
// REAL-TIME SCHEDULER
// =============================================================================

/// Real-time scheduler (FIFO and RR)
pub struct RtScheduler {
    /// Priority queues (0-99)
    queues: [Mutex<Vec<Tid>>; 100],

    /// Bitmap of non-empty priorities
    priority_bitmap: AtomicU64,

    /// Extended bitmap for priorities 64-99
    priority_bitmap_high: AtomicU64,

    /// Number of RT tasks
    nr_running: AtomicU64,

    /// RT bandwidth control
    rt_bandwidth: RtBandwidth,
}

/// RT bandwidth control (prevents RT starvation)
pub struct RtBandwidth {
    /// RT runtime per period (ns)
    rt_runtime_ns: u64,
    /// RT period (ns)
    rt_period_ns: u64,
    /// Runtime used in current period
    rt_time_ns: AtomicU64,
    /// Period start time
    period_start: AtomicU64,
}

impl RtScheduler {
    pub const fn new() -> Self {
        const EMPTY: Mutex<Vec<Tid>> = Mutex::new(Vec::new());

        Self {
            queues: [EMPTY; 100],
            priority_bitmap: AtomicU64::new(0),
            priority_bitmap_high: AtomicU64::new(0),
            nr_running: AtomicU64::new(0),
            rt_bandwidth: RtBandwidth {
                rt_runtime_ns: 950_000_000,  // 950ms per second
                rt_period_ns: 1_000_000_000, // 1 second
                rt_time_ns: AtomicU64::new(0),
                period_start: AtomicU64::new(0),
            },
        }
    }

    /// Enqueue RT task
    pub fn enqueue(&self, tid: Tid, priority: u32) {
        let prio = (priority as usize).min(99);
        self.queues[prio].lock().push(tid);
        self.nr_running.fetch_add(1, Ordering::Relaxed);

        // Update bitmap
        if prio < 64 {
            self.priority_bitmap.fetch_or(1 << prio, Ordering::Relaxed);
        } else {
            self.priority_bitmap_high.fetch_or(1 << (prio - 64), Ordering::Relaxed);
        }
    }

    /// Dequeue specific RT task
    pub fn dequeue(&self, tid: Tid) -> bool {
        for prio in 0..100 {
            let mut queue = self.queues[prio].lock();
            if let Some(pos) = queue.iter().position(|&t| t == tid) {
                queue.remove(pos);
                self.nr_running.fetch_sub(1, Ordering::Relaxed);

                if queue.is_empty() {
                    if prio < 64 {
                        self.priority_bitmap.fetch_and(!(1 << prio), Ordering::Relaxed);
                    } else {
                        self.priority_bitmap_high.fetch_and(!(1 << (prio - 64)), Ordering::Relaxed);
                    }
                }
                return true;
            }
        }
        false
    }

    /// Pick highest priority RT task
    pub fn pick_next(&self) -> Option<Tid> {
        // Check high priorities first (64-99)
        let high = self.priority_bitmap_high.load(Ordering::Relaxed);
        if high != 0 {
            let prio = 64 + (63 - high.leading_zeros() as usize);
            if let Some(tid) = self.queues[prio].lock().first().copied() {
                return Some(tid);
            }
        }

        // Check low priorities (0-63)
        let low = self.priority_bitmap.load(Ordering::Relaxed);
        if low != 0 {
            let prio = 63 - low.leading_zeros() as usize;
            if let Some(tid) = self.queues[prio].lock().first().copied() {
                return Some(tid);
            }
        }

        None
    }

    /// Check RT bandwidth limit
    pub fn check_bandwidth(&self, delta_ns: u64) -> bool {
        let used = self.rt_bandwidth.rt_time_ns.fetch_add(delta_ns, Ordering::Relaxed);
        used + delta_ns <= self.rt_bandwidth.rt_runtime_ns
    }

    /// Reset bandwidth for new period
    pub fn reset_bandwidth(&self) {
        self.rt_bandwidth.rt_time_ns.store(0, Ordering::Relaxed);
    }
}

// =============================================================================
// DEADLINE SCHEDULER (EDF)
// =============================================================================

/// Deadline scheduler (Earliest Deadline First)
pub struct DlScheduler {
    /// Tasks sorted by absolute deadline
    tasks: Mutex<BTreeMap<u64, Tid>>,

    /// Number of deadline tasks
    nr_running: AtomicU64,

    /// Total runtime reserved
    total_runtime_reserved: AtomicU64,
}

/// Deadline task state
pub struct DlTask {
    /// Thread ID
    pub tid: Tid,
    /// Runtime budget per period
    pub runtime_ns: u64,
    /// Deadline relative to period start
    pub deadline_ns: u64,
    /// Period length
    pub period_ns: u64,
    /// Remaining runtime in current period
    pub remaining_runtime: AtomicU64,
    /// Absolute deadline (timestamp)
    pub abs_deadline: AtomicU64,
    /// Period start timestamp
    pub period_start: AtomicU64,
    /// Flags
    pub flags: DlFlags,
}

bitflags::bitflags! {
    /// Deadline task flags
    #[derive(Clone, Copy, Debug, Default)]
    pub struct DlFlags: u32 {
        /// Task is throttled (exceeded runtime)
        const THROTTLED = 1 << 0;
        /// New period needed
        const NEW_PERIOD = 1 << 1;
        /// Non-yielding
        const NON_YIELD = 1 << 2;
    }
}

impl DlScheduler {
    pub const fn new() -> Self {
        Self {
            tasks: Mutex::new(BTreeMap::new()),
            nr_running: AtomicU64::new(0),
            total_runtime_reserved: AtomicU64::new(0),
        }
    }

    /// Check if task can be admitted (admission control)
    pub fn admission_check(&self, params: &DeadlineParams) -> bool {
        // Check if total utilization would exceed 100%
        let current_util = self.total_utilization();
        let new_util = (params.runtime_ns * 1000) / params.period_ns;

        current_util + new_util <= 1000 // 100% = 1000
    }

    /// Calculate total CPU utilization
    fn total_utilization(&self) -> u64 {
        // Sum of runtime/period for all tasks
        // Would track this properly
        0
    }

    /// Enqueue deadline task
    pub fn enqueue(&self, tid: Tid, abs_deadline: u64) {
        let mut tasks = self.tasks.lock();
        tasks.insert(abs_deadline, tid);
        self.nr_running.fetch_add(1, Ordering::Relaxed);
    }

    /// Dequeue deadline task
    pub fn dequeue(&self, tid: Tid) -> bool {
        let mut tasks = self.tasks.lock();

        let mut to_remove = None;
        for (&deadline, &t) in tasks.iter() {
            if t == tid {
                to_remove = Some(deadline);
                break;
            }
        }

        if let Some(deadline) = to_remove {
            tasks.remove(&deadline);
            self.nr_running.fetch_sub(1, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// Pick task with earliest deadline
    pub fn pick_next(&self) -> Option<Tid> {
        let tasks = self.tasks.lock();
        tasks.first_key_value().map(|(_, &tid)| tid)
    }

    /// Update task's deadline for new period
    pub fn update_deadline(&self, tid: Tid, new_deadline: u64) {
        self.dequeue(tid);
        self.enqueue(tid, new_deadline);
    }
}

// =============================================================================
// AI SCHEDULER
// =============================================================================

/// AI-optimized scheduler for neural workloads
pub struct AiScheduler {
    /// AI tasks by priority
    tasks: Mutex<BTreeMap<i32, Vec<Tid>>>,

    /// Neural predictor weights
    weights: [f32; 16],

    /// Number of AI tasks
    nr_running: AtomicU64,

    /// AI boost factor
    boost_factor: f32,
}

/// AI workload characteristics
#[derive(Clone)]
pub struct AiWorkloadHint {
    /// Is this a training workload?
    pub is_training: bool,
    /// Is this inference?
    pub is_inference: bool,
    /// Batch size
    pub batch_size: u32,
    /// Memory bandwidth requirements
    pub memory_bw: MemoryBwHint,
    /// Preferred accelerator
    pub accelerator: AcceleratorHint,
}

#[derive(Clone, Copy)]
pub enum MemoryBwHint {
    Low,
    Medium,
    High,
    Extreme,
}

#[derive(Clone, Copy)]
pub enum AcceleratorHint {
    None,
    Gpu,
    Tpu,
    Npu,
    Fpga,
}

impl AiScheduler {
    pub const fn new() -> Self {
        Self {
            tasks: Mutex::new(BTreeMap::new()),
            weights: [0.0; 16],
            nr_running: AtomicU64::new(0),
            boost_factor: 1.5,
        }
    }

    /// Enqueue AI task
    pub fn enqueue(&self, tid: Tid, priority: i32) {
        let mut tasks = self.tasks.lock();
        tasks.entry(priority).or_insert_with(Vec::new).push(tid);
        self.nr_running.fetch_add(1, Ordering::Relaxed);
    }

    /// Dequeue AI task
    pub fn dequeue(&self, tid: Tid) -> bool {
        let mut tasks = self.tasks.lock();

        for (prio, tids) in tasks.iter_mut() {
            if let Some(pos) = tids.iter().position(|&t| t == tid) {
                tids.remove(pos);
                if tids.is_empty() {
                    let prio = *prio;
                    tasks.remove(&prio);
                }
                self.nr_running.fetch_sub(1, Ordering::Relaxed);
                return true;
            }
        }
        false
    }

    /// Pick next AI task
    pub fn pick_next(&self) -> Option<Tid> {
        let tasks = self.tasks.lock();
        // Highest priority first (reverse order)
        if let Some((_, tids)) = tasks.iter().next_back() {
            return tids.first().copied();
        }
        None
    }

    /// Predict optimal priority for AI workload
    pub fn predict_priority(&self, _hint: &AiWorkloadHint) -> i32 {
        // Would use neural network to predict
        // For now, boost AI workloads
        (32.0 * self.boost_factor) as i32
    }

    /// Apply ML-based scheduling decision
    pub fn ml_schedule(&self, _candidates: &[Tid]) -> Option<Tid> {
        // Would use trained model to select best candidate
        // Based on predicted completion time, cache efficiency, etc.
        None
    }
}

// =============================================================================
// SCHEDULING CLASS OPERATIONS
// =============================================================================

/// Scheduling class operations
pub trait SchedClassOps {
    /// Enqueue a task
    fn enqueue(&self, tid: Tid, params: &SchedParams);

    /// Dequeue a task
    fn dequeue(&self, tid: Tid) -> bool;

    /// Pick next task to run
    fn pick_next(&self) -> Option<Tid>;

    /// Put previous task back
    fn put_prev(&self, tid: Tid, params: &SchedParams);

    /// Task woken up
    fn task_woken(&self, tid: Tid, params: &SchedParams);

    /// Check if should preempt current
    fn should_preempt(&self, current_tid: Tid, new_tid: Tid) -> bool;
}

// =============================================================================
// BATCH SCHEDULER
// =============================================================================

/// Batch scheduler (lower priority than normal)
pub struct BatchScheduler {
    /// Tasks by priority
    tasks: Mutex<BTreeMap<i32, Vec<Tid>>>,
    /// Number of batch tasks
    nr_running: AtomicU64,
}

impl BatchScheduler {
    pub const fn new() -> Self {
        Self {
            tasks: Mutex::new(BTreeMap::new()),
            nr_running: AtomicU64::new(0),
        }
    }

    pub fn enqueue(&self, tid: Tid, priority: i32) {
        let mut tasks = self.tasks.lock();
        tasks.entry(priority).or_insert_with(Vec::new).push(tid);
        self.nr_running.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dequeue(&self, tid: Tid) -> bool {
        let mut tasks = self.tasks.lock();
        for (prio, tids) in tasks.iter_mut() {
            if let Some(pos) = tids.iter().position(|&t| t == tid) {
                tids.remove(pos);
                if tids.is_empty() {
                    let prio = *prio;
                    tasks.remove(&prio);
                }
                self.nr_running.fetch_sub(1, Ordering::Relaxed);
                return true;
            }
        }
        false
    }

    pub fn pick_next(&self) -> Option<Tid> {
        let tasks = self.tasks.lock();
        if let Some((_, tids)) = tasks.iter().next_back() {
            return tids.first().copied();
        }
        None
    }
}

// =============================================================================
// IDLE SCHEDULER
// =============================================================================

/// Idle scheduler (runs when nothing else to do)
pub struct IdleScheduler {
    /// Idle tasks per CPU
    idle_threads: Mutex<Vec<Tid>>,
}

impl IdleScheduler {
    pub const fn new() -> Self {
        Self {
            idle_threads: Mutex::new(Vec::new()),
        }
    }

    pub fn set_idle_thread(&self, cpu: usize, tid: Tid) {
        let mut threads = self.idle_threads.lock();
        if cpu >= threads.len() {
            threads.resize(cpu + 1, Tid::new(0));
        }
        threads[cpu] = tid;
    }

    pub fn get_idle_thread(&self, cpu: usize) -> Option<Tid> {
        let threads = self.idle_threads.lock();
        threads.get(cpu).copied()
    }
}
