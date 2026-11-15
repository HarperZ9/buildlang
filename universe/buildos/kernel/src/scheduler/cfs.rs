//! Completely Fair Scheduler (CFS) Implementation
//!
//! CFS provides fair CPU time distribution using virtual runtime.

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use crate::process::Tid;
use super::{SchedEntity, TIME_SLICE_NS};

/// CFS scheduling period (default 6ms * nr_running, min 6ms, max 100ms)
const SCHED_PERIOD_MIN: u64 = 6_000_000; // 6ms
const SCHED_PERIOD_MAX: u64 = 100_000_000; // 100ms

/// Minimum granularity (prevent too frequent switches)
const SCHED_GRANULARITY: u64 = 750_000; // 0.75ms

/// Latency nice scaling factor
const LATENCY_WEIGHT: u64 = 2;

/// CFS run queue (red-black tree by vruntime)
pub struct CfsRunQueue {
    /// Tasks ordered by vruntime
    tasks: BTreeMap<u64, Vec<Tid>>,
    /// Minimum vruntime in the tree
    min_vruntime: u64,
    /// Number of runnable tasks
    nr_running: u32,
    /// Total weight
    total_weight: u64,
    /// Load (weighted average)
    load: u64,
    /// Current period
    period: u64,
}

impl CfsRunQueue {
    /// Create a new CFS run queue
    pub fn new() -> Self {
        Self {
            tasks: BTreeMap::new(),
            min_vruntime: 0,
            nr_running: 0,
            total_weight: 0,
            load: 0,
            period: SCHED_PERIOD_MIN,
        }
    }

    /// Enqueue a task
    pub fn enqueue(&mut self, entity: &mut SchedEntity) {
        // Place at min_vruntime to prevent starvation of new tasks
        let vruntime = entity.vruntime.max(self.min_vruntime);
        entity.vruntime = vruntime;

        self.tasks
            .entry(vruntime)
            .or_insert_with(Vec::new)
            .push(entity.tid);

        self.nr_running += 1;
        self.total_weight += entity.weight;
        self.update_period();
    }

    /// Dequeue a task
    pub fn dequeue(&mut self, entity: &SchedEntity) {
        if let Some(tasks) = self.tasks.get_mut(&entity.vruntime) {
            tasks.retain(|&tid| tid != entity.tid);
            if tasks.is_empty() {
                self.tasks.remove(&entity.vruntime);
            }
        }

        self.nr_running = self.nr_running.saturating_sub(1);
        self.total_weight = self.total_weight.saturating_sub(entity.weight);
        self.update_period();
        self.update_min_vruntime();
    }

    /// Pick the next task to run (leftmost in tree = minimum vruntime)
    pub fn pick_next(&mut self) -> Option<Tid> {
        // Get first entry (minimum vruntime)
        if let Some((&_vruntime, tasks)) = self.tasks.first_key_value() {
            if let Some(&tid) = tasks.first() {
                return Some(tid);
            }
        }
        None
    }

    /// Put a task back (was picked but not run)
    pub fn put_prev(&mut self, _entity: &SchedEntity) {
        // Just update min_vruntime
        self.update_min_vruntime();
    }

    /// Update vruntime after running
    pub fn update_curr(&mut self, entity: &mut SchedEntity, delta: u64) {
        // Remove from old position
        if let Some(tasks) = self.tasks.get_mut(&entity.vruntime) {
            tasks.retain(|&tid| tid != entity.tid);
            if tasks.is_empty() {
                self.tasks.remove(&entity.vruntime);
            }
        }

        // Update vruntime
        entity.update_vruntime(delta);

        // Re-insert at new position
        self.tasks
            .entry(entity.vruntime)
            .or_insert_with(Vec::new)
            .push(entity.tid);

        self.update_min_vruntime();
    }

    /// Update minimum vruntime
    fn update_min_vruntime(&mut self) {
        if let Some((&vruntime, _)) = self.tasks.first_key_value() {
            // min_vruntime can only increase
            self.min_vruntime = self.min_vruntime.max(vruntime);
        }
    }

    /// Update scheduling period
    fn update_period(&mut self) {
        let nr = self.nr_running as u64;
        self.period = (SCHED_PERIOD_MIN * nr)
            .max(SCHED_PERIOD_MIN)
            .min(SCHED_PERIOD_MAX);
    }

    /// Calculate time slice for a task
    pub fn calc_time_slice(&self, entity: &SchedEntity) -> u64 {
        if self.total_weight == 0 {
            return TIME_SLICE_NS;
        }

        let slice = self.period * entity.weight / self.total_weight;
        slice.max(SCHED_GRANULARITY)
    }

    /// Check if current task should be preempted
    pub fn check_preempt(&self, current: &SchedEntity) -> bool {
        if let Some((&min_vruntime, _)) = self.tasks.first_key_value() {
            // Preempt if leftmost has much lower vruntime
            let ideal_runtime = self.calc_time_slice(current);
            let delta = current.vruntime.saturating_sub(min_vruntime);
            delta > ideal_runtime
        } else {
            false
        }
    }

    /// Get load (weighted runnable count)
    pub fn load(&self) -> u64 {
        self.total_weight
    }

    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.nr_running == 0
    }

    /// Number of runnable tasks
    pub fn nr_running(&self) -> u32 {
        self.nr_running
    }
}

/// CFS bandwidth control (CPU quota)
pub struct CfsBandwidth {
    /// Quota (microseconds per period)
    quota: u64,
    /// Period (microseconds)
    period: u64,
    /// Runtime remaining this period
    runtime: u64,
    /// Period expires at
    period_expires: u64,
    /// Is throttled
    throttled: bool,
}

impl CfsBandwidth {
    /// Create unlimited bandwidth
    pub fn unlimited() -> Self {
        Self {
            quota: u64::MAX,
            period: 100_000, // 100ms
            runtime: u64::MAX,
            period_expires: 0,
            throttled: false,
        }
    }

    /// Create limited bandwidth
    pub fn new(quota: u64, period: u64) -> Self {
        Self {
            quota,
            period,
            runtime: quota,
            period_expires: 0,
            throttled: false,
        }
    }

    /// Check if throttled
    pub fn is_throttled(&self) -> bool {
        self.throttled
    }

    /// Account runtime
    pub fn account(&mut self, runtime: u64, now: u64) {
        // Check if period expired
        if now >= self.period_expires {
            self.runtime = self.quota;
            self.period_expires = now + self.period;
            self.throttled = false;
        }

        // Deduct runtime
        if runtime >= self.runtime {
            self.runtime = 0;
            self.throttled = true;
        } else {
            self.runtime -= runtime;
        }
    }

    /// Refill quota (called when period expires)
    pub fn refill(&mut self) {
        self.runtime = self.quota;
        self.throttled = false;
    }
}

/// CFS group scheduling (task groups / cgroups)
pub struct CfsTaskGroup {
    /// Group identifier
    pub id: u64,
    /// Parent group
    pub parent: Option<u64>,
    /// Weight (shares)
    pub shares: u64,
    /// Per-CPU run queues
    pub cfs_rq: Vec<CfsRunQueue>,
    /// Bandwidth control
    pub bandwidth: CfsBandwidth,
}

impl CfsTaskGroup {
    /// Create a new task group
    pub fn new(id: u64, nr_cpus: u32) -> Self {
        let mut cfs_rq = Vec::with_capacity(nr_cpus as usize);
        for _ in 0..nr_cpus {
            cfs_rq.push(CfsRunQueue::new());
        }

        Self {
            id,
            parent: None,
            shares: 1024, // Default shares
            cfs_rq,
            bandwidth: CfsBandwidth::unlimited(),
        }
    }

    /// Set shares (weight relative to other groups)
    pub fn set_shares(&mut self, shares: u64) {
        self.shares = shares.max(2); // Minimum 2 shares
    }

    /// Set CPU quota
    pub fn set_quota(&mut self, quota: u64, period: u64) {
        self.bandwidth = CfsBandwidth::new(quota, period);
    }

    /// Remove CPU quota
    pub fn remove_quota(&mut self) {
        self.bandwidth = CfsBandwidth::unlimited();
    }
}

/// Autogroup (automatic task grouping)
pub struct Autogroup {
    /// Session ID
    pub session: u32,
    /// Task group
    pub tg: CfsTaskGroup,
    /// Nice value for the group
    pub nice: i32,
}

impl Autogroup {
    /// Create autogroup for session
    pub fn new(session: u32, nr_cpus: u32) -> Self {
        Self {
            session,
            tg: CfsTaskGroup::new(session as u64, nr_cpus),
            nice: 0,
        }
    }

    /// Set nice value (affects all tasks in group)
    pub fn set_nice(&mut self, nice: i32) {
        self.nice = nice.clamp(-20, 19);
        // Adjust shares based on nice
        self.tg.shares = super::nice_to_weight(self.nice);
    }
}

/// CFS scheduler (combines run queues and groups)
pub struct CfsScheduler {
    /// Per-CPU run queues
    run_queues: Vec<CfsRunQueue>,
    /// Task groups
    groups: BTreeMap<u64, CfsTaskGroup>,
    /// Autogroups by session
    autogroups: BTreeMap<u32, Autogroup>,
    /// Number of CPUs
    nr_cpus: u32,
}

impl CfsScheduler {
    /// Create a new CFS scheduler
    pub fn new(nr_cpus: u32) -> Self {
        let mut run_queues = Vec::with_capacity(nr_cpus as usize);
        for _ in 0..nr_cpus {
            run_queues.push(CfsRunQueue::new());
        }

        Self {
            run_queues,
            groups: BTreeMap::new(),
            autogroups: BTreeMap::new(),
            nr_cpus,
        }
    }

    /// Get run queue for CPU
    pub fn rq(&self, cpu: u32) -> Option<&CfsRunQueue> {
        self.run_queues.get(cpu as usize)
    }

    /// Get mutable run queue for CPU
    pub fn rq_mut(&mut self, cpu: u32) -> Option<&mut CfsRunQueue> {
        self.run_queues.get_mut(cpu as usize)
    }

    /// Create a task group
    pub fn create_group(&mut self, id: u64, parent: Option<u64>) {
        let mut tg = CfsTaskGroup::new(id, self.nr_cpus);
        tg.parent = parent;
        self.groups.insert(id, tg);
    }

    /// Set group shares
    pub fn set_group_shares(&mut self, id: u64, shares: u64) {
        if let Some(tg) = self.groups.get_mut(&id) {
            tg.set_shares(shares);
        }
    }

    /// Set group quota
    pub fn set_group_quota(&mut self, id: u64, quota: u64, period: u64) {
        if let Some(tg) = self.groups.get_mut(&id) {
            tg.set_quota(quota, period);
        }
    }

    /// Get or create autogroup for session
    pub fn autogroup(&mut self, session: u32) -> &mut Autogroup {
        if !self.autogroups.contains_key(&session) {
            self.autogroups.insert(session, Autogroup::new(session, self.nr_cpus));
        }
        self.autogroups.get_mut(&session).unwrap()
    }
}
