//! Real-Time Scheduler Implementation
//!
//! Implements SCHED_FIFO, SCHED_RR, and SCHED_DEADLINE scheduling policies.

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use crate::process::Tid;
use super::SchedEntity;

/// Maximum real-time priority (0-99)
pub const MAX_RT_PRIO: u8 = 99;

/// Default round-robin time slice (100ms)
pub const RR_TIME_SLICE_NS: u64 = 100_000_000;

/// Real-time run queue
pub struct RtRunQueue {
    /// Per-priority FIFO queues (0-99)
    queues: [Vec<Tid>; 100],
    /// Bitmap of non-empty queues
    bitmap: RtPrioBitmap,
    /// Number of runnable RT tasks
    nr_running: u32,
    /// Current highest priority
    highest_prio: Option<u8>,
}

/// Bitmap for tracking non-empty priority queues
pub struct RtPrioBitmap {
    /// 100 bits packed into 4 u32s (128 bits, 100 used)
    words: [u32; 4],
}

impl RtPrioBitmap {
    /// Create empty bitmap
    pub const fn new() -> Self {
        Self { words: [0; 4] }
    }

    /// Set a priority bit
    pub fn set(&mut self, prio: u8) {
        let idx = prio as usize / 32;
        let bit = prio as usize % 32;
        self.words[idx] |= 1 << bit;
    }

    /// Clear a priority bit
    pub fn clear(&mut self, prio: u8) {
        let idx = prio as usize / 32;
        let bit = prio as usize % 32;
        self.words[idx] &= !(1 << bit);
    }

    /// Test a priority bit
    pub fn test(&self, prio: u8) -> bool {
        let idx = prio as usize / 32;
        let bit = prio as usize % 32;
        (self.words[idx] & (1 << bit)) != 0
    }

    /// Find highest set priority (highest number = highest priority for RT)
    pub fn find_first_set(&self) -> Option<u8> {
        // Search from high to low priority
        for word_idx in (0..4).rev() {
            if self.words[word_idx] != 0 {
                let bit = 31 - self.words[word_idx].leading_zeros();
                let prio = (word_idx * 32) as u8 + bit as u8;
                if prio < 100 {
                    return Some(prio);
                }
            }
        }
        None
    }
}

impl RtRunQueue {
    /// Create a new RT run queue
    pub fn new() -> Self {
        const EMPTY_VEC: Vec<Tid> = Vec::new();
        Self {
            queues: [EMPTY_VEC; 100],
            bitmap: RtPrioBitmap::new(),
            nr_running: 0,
            highest_prio: None,
        }
    }

    /// Enqueue a task
    pub fn enqueue(&mut self, entity: &SchedEntity) {
        let prio = entity.params.priority.min(MAX_RT_PRIO as i32) as usize;
        self.queues[prio].push(entity.tid);
        self.bitmap.set(prio as u8);
        self.nr_running += 1;
        self.update_highest();
    }

    /// Dequeue a task
    pub fn dequeue(&mut self, entity: &SchedEntity) {
        let prio = entity.params.priority.min(MAX_RT_PRIO as i32) as usize;
        self.queues[prio].retain(|&tid| tid != entity.tid);
        if self.queues[prio].is_empty() {
            self.bitmap.clear(prio as u8);
        }
        self.nr_running = self.nr_running.saturating_sub(1);
        self.update_highest();
    }

    /// Pick the next task (highest priority, first in queue)
    pub fn pick_next(&mut self) -> Option<Tid> {
        if let Some(prio) = self.highest_prio {
            self.queues[prio as usize].first().copied()
        } else {
            None
        }
    }

    /// Put previous task back (for RR, move to end of queue)
    pub fn put_prev(&mut self, entity: &SchedEntity, is_rr: bool) {
        if is_rr {
            let prio = entity.params.priority.min(MAX_RT_PRIO as i32) as usize;
            if let Some(pos) = self.queues[prio].iter().position(|&t| t == entity.tid) {
                let tid = self.queues[prio].remove(pos);
                self.queues[prio].push(tid);
            }
        }
    }

    /// Update highest priority
    fn update_highest(&mut self) {
        self.highest_prio = self.bitmap.find_first_set();
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.nr_running == 0
    }

    /// Number of runnable tasks
    pub fn nr_running(&self) -> u32 {
        self.nr_running
    }

    /// Get the highest priority
    pub fn highest_priority(&self) -> Option<u8> {
        self.highest_prio
    }
}

/// Deadline scheduling parameters
#[derive(Clone, Debug)]
pub struct DeadlineParams {
    /// Runtime in nanoseconds (how much CPU time needed)
    pub runtime: u64,
    /// Deadline in nanoseconds (relative to period start)
    pub deadline: u64,
    /// Period in nanoseconds
    pub period: u64,
    /// Flags
    pub flags: DeadlineFlags,
}

impl Default for DeadlineParams {
    fn default() -> Self {
        Self {
            runtime: 10_000_000,    // 10ms
            deadline: 30_000_000,   // 30ms
            period: 100_000_000,    // 100ms
            flags: DeadlineFlags::empty(),
        }
    }
}

bitflags::bitflags! {
    /// Deadline scheduling flags
    #[derive(Clone, Copy, Debug)]
    pub struct DeadlineFlags: u32 {
        /// Reset the runtime at period boundary
        const RESET_ON_FORK = 0x01;
        /// Use single-shot mode (no period repetition)
        const ONESHOT = 0x02;
        /// Allow runtime reclamation
        const RECLAIM = 0x04;
    }
}

/// Deadline task state
#[derive(Clone, Debug)]
pub struct DeadlineTask {
    /// Thread ID
    pub tid: Tid,
    /// Scheduling parameters
    pub params: DeadlineParams,
    /// Absolute deadline (when current job must complete)
    pub abs_deadline: u64,
    /// Remaining runtime for current period
    pub runtime_remaining: u64,
    /// Period start time
    pub period_start: u64,
    /// Is throttled (used up runtime)
    pub throttled: bool,
    /// Bandwidth fraction (runtime / period * 2^20)
    pub bw: u64,
}

impl DeadlineTask {
    /// Create a new deadline task
    pub fn new(tid: Tid, params: DeadlineParams) -> Self {
        let bw = (params.runtime << 20) / params.period;
        Self {
            tid,
            params: params.clone(),
            abs_deadline: 0,
            runtime_remaining: params.runtime,
            period_start: 0,
            throttled: false,
            bw,
        }
    }

    /// Start a new period
    pub fn start_period(&mut self, now: u64) {
        self.period_start = now;
        self.abs_deadline = now + self.params.deadline;
        self.runtime_remaining = self.params.runtime;
        self.throttled = false;
    }

    /// Account runtime
    pub fn account(&mut self, delta: u64) {
        if delta >= self.runtime_remaining {
            self.runtime_remaining = 0;
            self.throttled = true;
        } else {
            self.runtime_remaining -= delta;
        }
    }

    /// Check if deadline missed
    pub fn deadline_missed(&self, now: u64) -> bool {
        now > self.abs_deadline && self.runtime_remaining > 0
    }

    /// Check if period expired
    pub fn period_expired(&self, now: u64) -> bool {
        now >= self.period_start + self.params.period
    }
}

/// Deadline run queue (Earliest Deadline First)
pub struct DeadlineRunQueue {
    /// Tasks ordered by absolute deadline
    tasks: BTreeMap<u64, Vec<Tid>>,
    /// Task details
    task_map: BTreeMap<Tid, DeadlineTask>,
    /// Number of runnable tasks
    nr_running: u32,
    /// Total bandwidth (sum of task bw, scaled by 2^20)
    total_bw: u64,
    /// Maximum allowed bandwidth (default 95%)
    max_bw: u64,
    /// Earliest deadline
    earliest_deadline: u64,
}

impl DeadlineRunQueue {
    /// Create a new deadline run queue
    pub fn new() -> Self {
        Self {
            tasks: BTreeMap::new(),
            task_map: BTreeMap::new(),
            nr_running: 0,
            total_bw: 0,
            max_bw: (95 << 20) / 100, // 95% of CPU
            earliest_deadline: u64::MAX,
        }
    }

    /// Check if admission is possible
    pub fn admission_test(&self, params: &DeadlineParams) -> bool {
        let bw = (params.runtime << 20) / params.period;
        self.total_bw + bw <= self.max_bw
    }

    /// Enqueue a task
    pub fn enqueue(&mut self, tid: Tid, params: DeadlineParams, now: u64) -> Result<(), DeadlineError> {
        // Admission control
        if !self.admission_test(&params) {
            return Err(DeadlineError::BandwidthExceeded);
        }

        let mut task = DeadlineTask::new(tid, params);
        task.start_period(now);

        // Add to deadline tree
        self.tasks
            .entry(task.abs_deadline)
            .or_insert_with(Vec::new)
            .push(tid);

        self.total_bw += task.bw;
        self.task_map.insert(tid, task);
        self.nr_running += 1;
        self.update_earliest();

        Ok(())
    }

    /// Dequeue a task
    pub fn dequeue(&mut self, tid: Tid) {
        if let Some(task) = self.task_map.remove(&tid) {
            // Remove from deadline tree
            if let Some(tasks) = self.tasks.get_mut(&task.abs_deadline) {
                tasks.retain(|&t| t != tid);
                if tasks.is_empty() {
                    self.tasks.remove(&task.abs_deadline);
                }
            }

            self.total_bw = self.total_bw.saturating_sub(task.bw);
            self.nr_running = self.nr_running.saturating_sub(1);
            self.update_earliest();
        }
    }

    /// Pick the next task (earliest deadline first)
    pub fn pick_next(&self) -> Option<Tid> {
        self.tasks.first_key_value()
            .and_then(|(_, tids)| tids.first().copied())
    }

    /// Update task after running
    pub fn update_curr(&mut self, tid: Tid, delta: u64, now: u64) {
        // First, check if task exists and gather info
        let (old_deadline, period_expired, new_deadline) = {
            if let Some(task) = self.task_map.get_mut(&tid) {
                let old_deadline = task.abs_deadline;

                // Account runtime
                task.account(delta);

                // Check if period expired
                if task.period_expired(now) {
                    // Start new period
                    task.start_period(now);
                    (old_deadline, true, task.abs_deadline)
                } else {
                    return; // No further work needed
                }
            } else {
                return; // Task not found
            }
        };

        // Now we can safely modify the tree structures
        if period_expired {
            // Remove from old position
            self.remove_from_tree(tid, old_deadline);

            // Re-insert at new deadline
            self.tasks
                .entry(new_deadline)
                .or_insert_with(Vec::new)
                .push(tid);

            self.update_earliest();
        }
    }

    /// Remove from deadline tree
    fn remove_from_tree(&mut self, tid: Tid, deadline: u64) {
        if let Some(tasks) = self.tasks.get_mut(&deadline) {
            tasks.retain(|&t| t != tid);
            if tasks.is_empty() {
                self.tasks.remove(&deadline);
            }
        }
    }

    /// Update earliest deadline
    fn update_earliest(&mut self) {
        self.earliest_deadline = self.tasks
            .first_key_value()
            .map(|(&d, _)| d)
            .unwrap_or(u64::MAX);
    }

    /// Get earliest deadline
    pub fn earliest(&self) -> u64 {
        self.earliest_deadline
    }

    /// Check if any task is throttled
    pub fn has_throttled(&self) -> bool {
        self.task_map.values().any(|t| t.throttled)
    }

    /// Get throttled tasks
    pub fn throttled_tasks(&self) -> Vec<Tid> {
        self.task_map.values()
            .filter(|t| t.throttled)
            .map(|t| t.tid)
            .collect()
    }

    /// Check for deadline misses
    pub fn check_deadlines(&self, now: u64) -> Vec<Tid> {
        self.task_map.values()
            .filter(|t| t.deadline_missed(now))
            .map(|t| t.tid)
            .collect()
    }

    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.nr_running == 0
    }

    /// Number of runnable tasks
    pub fn nr_running(&self) -> u32 {
        self.nr_running
    }

    /// Current bandwidth utilization
    pub fn bandwidth_utilization(&self) -> u64 {
        (self.total_bw * 100) >> 20
    }
}

/// Deadline errors
#[derive(Clone, Debug)]
pub enum DeadlineError {
    /// Would exceed bandwidth limit
    BandwidthExceeded,
    /// Invalid parameters
    InvalidParams,
    /// Task not found
    NotFound,
}

/// Real-time bandwidth control
pub struct RtBandwidth {
    /// Runtime allowed per period
    pub runtime: u64,
    /// Period in nanoseconds
    pub period: u64,
    /// Runtime remaining this period
    runtime_remaining: u64,
    /// Period expires at
    period_expires: u64,
    /// Is throttled
    throttled: bool,
}

impl RtBandwidth {
    /// Create with default settings (95% of CPU)
    pub fn default_settings() -> Self {
        Self {
            runtime: 950_000_000, // 950ms per 1s
            period: 1_000_000_000, // 1 second
            runtime_remaining: 950_000_000,
            period_expires: 0,
            throttled: false,
        }
    }

    /// Create unlimited bandwidth
    pub fn unlimited() -> Self {
        Self {
            runtime: u64::MAX,
            period: 1_000_000_000,
            runtime_remaining: u64::MAX,
            period_expires: 0,
            throttled: false,
        }
    }

    /// Create custom bandwidth
    pub fn new(runtime: u64, period: u64) -> Self {
        Self {
            runtime,
            period,
            runtime_remaining: runtime,
            period_expires: 0,
            throttled: false,
        }
    }

    /// Account runtime
    pub fn account(&mut self, runtime: u64, now: u64) {
        // Check if period expired
        if now >= self.period_expires {
            self.runtime_remaining = self.runtime;
            self.period_expires = now + self.period;
            self.throttled = false;
        }

        if runtime >= self.runtime_remaining {
            self.runtime_remaining = 0;
            self.throttled = true;
        } else {
            self.runtime_remaining -= runtime;
        }
    }

    /// Is throttled
    pub fn is_throttled(&self) -> bool {
        self.throttled
    }

    /// Refill (called on period expiry)
    pub fn refill(&mut self) {
        self.runtime_remaining = self.runtime;
        self.throttled = false;
    }
}

/// RT scheduler combining FIFO, RR, and Deadline
pub struct RtScheduler {
    /// Per-CPU FIFO/RR run queues
    rt_rq: Vec<RtRunQueue>,
    /// Per-CPU deadline run queues
    dl_rq: Vec<DeadlineRunQueue>,
    /// RT bandwidth control
    rt_bandwidth: RtBandwidth,
    /// Number of CPUs
    nr_cpus: u32,
}

impl RtScheduler {
    /// Create a new RT scheduler
    pub fn new(nr_cpus: u32) -> Self {
        let mut rt_rq = Vec::with_capacity(nr_cpus as usize);
        let mut dl_rq = Vec::with_capacity(nr_cpus as usize);

        for _ in 0..nr_cpus {
            rt_rq.push(RtRunQueue::new());
            dl_rq.push(DeadlineRunQueue::new());
        }

        Self {
            rt_rq,
            dl_rq,
            rt_bandwidth: RtBandwidth::default_settings(),
            nr_cpus,
        }
    }

    /// Get RT run queue for CPU
    pub fn rt_rq(&self, cpu: u32) -> Option<&RtRunQueue> {
        self.rt_rq.get(cpu as usize)
    }

    /// Get mutable RT run queue
    pub fn rt_rq_mut(&mut self, cpu: u32) -> Option<&mut RtRunQueue> {
        self.rt_rq.get_mut(cpu as usize)
    }

    /// Get deadline run queue for CPU
    pub fn dl_rq(&self, cpu: u32) -> Option<&DeadlineRunQueue> {
        self.dl_rq.get(cpu as usize)
    }

    /// Get mutable deadline run queue
    pub fn dl_rq_mut(&mut self, cpu: u32) -> Option<&mut DeadlineRunQueue> {
        self.dl_rq.get_mut(cpu as usize)
    }

    /// Set RT bandwidth
    pub fn set_bandwidth(&mut self, runtime: u64, period: u64) {
        self.rt_bandwidth = RtBandwidth::new(runtime, period);
    }

    /// Disable RT bandwidth limiting
    pub fn disable_bandwidth(&mut self) {
        self.rt_bandwidth = RtBandwidth::unlimited();
    }

    /// Check if RT is throttled
    pub fn is_rt_throttled(&self) -> bool {
        self.rt_bandwidth.is_throttled()
    }
}

/// Priority inheritance for real-time mutex
pub struct PriorityInheritance {
    /// Original priority by task
    original_priority: BTreeMap<Tid, u8>,
    /// Boosted priority by task
    boosted_priority: BTreeMap<Tid, u8>,
    /// Tasks waiting on each task
    waiters: BTreeMap<Tid, Vec<Tid>>,
}

impl PriorityInheritance {
    /// Create new PI tracking
    pub fn new() -> Self {
        Self {
            original_priority: BTreeMap::new(),
            boosted_priority: BTreeMap::new(),
            waiters: BTreeMap::new(),
        }
    }

    /// Register a task's priority
    pub fn register(&mut self, tid: Tid, priority: u8) {
        self.original_priority.insert(tid, priority);
        self.boosted_priority.insert(tid, priority);
    }

    /// Unregister a task
    pub fn unregister(&mut self, tid: Tid) {
        self.original_priority.remove(&tid);
        self.boosted_priority.remove(&tid);
        self.waiters.remove(&tid);
    }

    /// Task starts waiting on another task
    pub fn wait_on(&mut self, waiter: Tid, owner: Tid) {
        // Add waiter to owner's waiter list
        self.waiters
            .entry(owner)
            .or_insert_with(Vec::new)
            .push(waiter);

        // Boost owner's priority if needed
        if let Some(&waiter_prio) = self.boosted_priority.get(&waiter) {
            if let Some(owner_prio) = self.boosted_priority.get_mut(&owner) {
                if waiter_prio > *owner_prio {
                    *owner_prio = waiter_prio;
                }
            }
        }
    }

    /// Task stops waiting
    pub fn stop_waiting(&mut self, waiter: Tid, owner: Tid) {
        // Remove waiter from owner's list
        if let Some(waiters) = self.waiters.get_mut(&owner) {
            waiters.retain(|&t| t != waiter);
        }

        // Recalculate owner's priority
        self.recalculate_priority(owner);
    }

    /// Recalculate boosted priority
    fn recalculate_priority(&mut self, tid: Tid) {
        let original = *self.original_priority.get(&tid).unwrap_or(&0);
        let max_waiter = self.waiters
            .get(&tid)
            .map(|waiters| {
                waiters.iter()
                    .filter_map(|w| self.boosted_priority.get(w))
                    .copied()
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0);

        if let Some(prio) = self.boosted_priority.get_mut(&tid) {
            *prio = original.max(max_waiter);
        }
    }

    /// Get current (potentially boosted) priority
    pub fn current_priority(&self, tid: Tid) -> u8 {
        *self.boosted_priority.get(&tid).unwrap_or(&0)
    }
}

/// POSIX real-time signal queue
pub struct RtSignalQueue {
    /// Pending signals per task
    pending: BTreeMap<Tid, Vec<RtSignal>>,
    /// Maximum queued signals per task
    max_queued: usize,
}

/// Real-time signal
#[derive(Clone, Debug)]
pub struct RtSignal {
    /// Signal number (SIGRTMIN to SIGRTMAX)
    pub signo: i32,
    /// Signal value (union of int and pointer)
    pub value: i64,
    /// Sending process ID
    pub pid: u32,
    /// Sending user ID
    pub uid: u32,
}

impl RtSignalQueue {
    /// Create new RT signal queue
    pub fn new() -> Self {
        Self {
            pending: BTreeMap::new(),
            max_queued: 32,
        }
    }

    /// Queue a real-time signal
    pub fn queue(&mut self, tid: Tid, signal: RtSignal) -> Result<(), ()> {
        let pending = self.pending.entry(tid).or_insert_with(Vec::new);
        if pending.len() >= self.max_queued {
            return Err(());
        }
        pending.push(signal);
        // Keep sorted by signal number for delivery order
        pending.sort_by_key(|s| s.signo);
        Ok(())
    }

    /// Dequeue next RT signal
    pub fn dequeue(&mut self, tid: Tid) -> Option<RtSignal> {
        self.pending.get_mut(&tid).and_then(|pending| {
            if pending.is_empty() {
                None
            } else {
                Some(pending.remove(0))
            }
        })
    }

    /// Check if there are pending RT signals
    pub fn has_pending(&self, tid: Tid) -> bool {
        self.pending.get(&tid).map(|p| !p.is_empty()).unwrap_or(false)
    }

    /// Clear all pending signals for a task
    pub fn clear(&mut self, tid: Tid) {
        self.pending.remove(&tid);
    }
}
