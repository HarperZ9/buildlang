//! BuildOS Advanced Scheduler
//!
//! Enhanced scheduling with:
//! - Completely Fair Scheduler (CFS) implementation
//! - Real-time scheduling policies (FIFO, RR)
//! - CPU affinity and NUMA awareness
//! - SMP load balancing
//! - Priority inheritance for mutexes
//! - Deadline scheduling

#![allow(dead_code)]

pub mod cfs;
pub mod realtime;
pub mod affinity;
pub mod load_balance;
pub mod policy;

pub use policy::{SchedPolicy, SchedAttr, SchedFlags as PolicyFlags, PolicyManager};
pub use cfs::{CfsRunQueue, CfsScheduler, CfsBandwidth, CfsTaskGroup};
pub use realtime::{RtRunQueue, RtScheduler, DeadlineRunQueue, DeadlineParams as RtDeadlineParams};
pub use affinity::{CpuSet, AffinityManager, NumaTopology, SchedDomain};
pub use load_balance::{LoadBalancer, CpuLoad, BalanceDecision};

/// Scheduling policy (alias for backwards compatibility)
pub type SchedulingPolicy = SchedPolicy;

/// Scheduling class (derived from policy)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchedulingClass {
    /// Deadline scheduling (highest priority)
    Deadline,
    /// Real-time scheduling
    RealTime,
    /// Normal fair scheduling
    Fair,
    /// Idle scheduling (lowest priority)
    Idle,
}

impl From<SchedPolicy> for SchedulingClass {
    fn from(policy: SchedPolicy) -> Self {
        match policy {
            SchedPolicy::Deadline => Self::Deadline,
            SchedPolicy::Fifo | SchedPolicy::RoundRobin => Self::RealTime,
            SchedPolicy::Normal | SchedPolicy::Batch => Self::Fair,
            SchedPolicy::Idle => Self::Idle,
        }
    }
}

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// CPU affinity wrapper around CpuSet with convenience methods
#[derive(Clone, Debug)]
pub struct CpuAffinity(CpuSet);

impl CpuAffinity {
    /// Create affinity allowing all CPUs
    pub fn all() -> Self {
        // Default to 256 CPUs, will be masked by actual available
        Self(CpuSet::all(256))
    }

    /// Create affinity for a single CPU
    pub fn single(cpu: u32) -> Self {
        Self(CpuSet::single(cpu))
    }

    /// Create from a list of CPUs
    pub fn from_cpus(cpus: &[u32]) -> Self {
        Self(CpuSet::from_cpus(cpus))
    }

    /// Check if CPU is allowed
    pub fn contains(&self, cpu: u32) -> bool {
        self.0.contains(cpu)
    }

    /// Set a CPU as allowed
    pub fn set(&mut self, cpu: u32) {
        self.0.set(cpu)
    }

    /// Clear a CPU from allowed
    pub fn clear(&mut self, cpu: u32) {
        self.0.clear(cpu)
    }

    /// Get underlying CpuSet
    pub fn as_cpuset(&self) -> &CpuSet {
        &self.0
    }
}
use core::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use crate::sync::{Mutex, RwLock, Spinlock};
use crate::process::Tid;

/// Time slice duration in nanoseconds (default 10ms)
pub const TIME_SLICE_NS: u64 = 10_000_000;

/// Minimum time slice (1ms)
pub const MIN_TIME_SLICE_NS: u64 = 1_000_000;

/// Maximum time slice (100ms)
pub const MAX_TIME_SLICE_NS: u64 = 100_000_000;

/// Number of priority levels for normal scheduling
pub const NUM_NORMAL_PRIORITIES: usize = 40;

/// Number of real-time priority levels
pub const NUM_RT_PRIORITIES: usize = 100;

/// Default nice value
pub const DEFAULT_NICE: i32 = 0;

/// Minimum nice value (highest priority)
pub const MIN_NICE: i32 = -20;

/// Maximum nice value (lowest priority)
pub const MAX_NICE: i32 = 19;

/// Thread scheduling parameters
#[derive(Clone, Debug)]
pub struct SchedParams {
    /// Scheduling policy
    pub policy: SchedulingPolicy,
    /// Priority (for RT) or nice value (for normal)
    pub priority: i32,
    /// CPU affinity mask
    pub affinity: CpuAffinity,
    /// Deadline parameters (for deadline scheduling)
    pub deadline: Option<realtime::DeadlineParams>,
    /// Flags
    pub flags: SchedFlags,
}

impl Default for SchedParams {
    fn default() -> Self {
        Self {
            policy: SchedulingPolicy::Normal,
            priority: DEFAULT_NICE,
            affinity: CpuAffinity::all(),
            deadline: None,
            flags: SchedFlags::empty(),
        }
    }
}

/// Deadline scheduling parameters
#[derive(Clone, Copy, Debug)]
pub struct DeadlineParams {
    /// Runtime (execution budget) in nanoseconds
    pub runtime: u64,
    /// Period in nanoseconds
    pub period: u64,
    /// Deadline in nanoseconds (relative to period start)
    pub deadline: u64,
}

/// Scheduling flags
#[derive(Clone, Copy, Debug, Default)]
pub struct SchedFlags(u32);

impl SchedFlags {
    /// No flags
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Reset on fork (child gets default params)
    pub const RESET_ON_FORK: u32 = 0x01;
    /// Reclaim unused bandwidth
    pub const RECLAIM: u32 = 0x02;
    /// Run on idle CPUs only
    pub const IDLE: u32 = 0x04;

    pub fn has(&self, flag: u32) -> bool {
        (self.0 & flag) != 0
    }
}

/// Scheduler entity - per-thread scheduling state
#[derive(Clone)]
pub struct SchedEntity {
    /// Thread ID
    pub tid: Tid,
    /// Scheduling parameters
    pub params: SchedParams,
    /// Virtual runtime (for CFS)
    pub vruntime: u64,
    /// Time slice remaining (nanoseconds)
    pub time_slice: u64,
    /// Last time scheduled
    pub last_scheduled: u64,
    /// Total runtime (nanoseconds)
    pub sum_exec_runtime: u64,
    /// Number of times scheduled
    pub nr_switches: u64,
    /// Wait time accumulated
    pub wait_sum: u64,
    /// Number of migrations between CPUs
    pub nr_migrations: u64,
    /// On which CPU this entity is enqueued
    pub cpu: u32,
    /// Weight (based on nice value)
    pub weight: u64,
    /// Is currently running
    pub on_cpu: bool,
    /// Is on run queue
    pub on_rq: bool,
}

impl SchedEntity {
    /// Create a new scheduling entity
    pub fn new(tid: Tid) -> Self {
        Self {
            tid,
            params: SchedParams::default(),
            vruntime: 0,
            time_slice: TIME_SLICE_NS,
            last_scheduled: 0,
            sum_exec_runtime: 0,
            nr_switches: 0,
            wait_sum: 0,
            nr_migrations: 0,
            cpu: 0,
            weight: nice_to_weight(DEFAULT_NICE),
            on_cpu: false,
            on_rq: false,
        }
    }

    /// Calculate time slice based on weight
    pub fn calc_time_slice(&mut self, total_weight: u64, period: u64) {
        if total_weight > 0 {
            self.time_slice = (period * self.weight / total_weight).max(MIN_TIME_SLICE_NS);
        } else {
            self.time_slice = TIME_SLICE_NS;
        }
    }

    /// Update virtual runtime
    pub fn update_vruntime(&mut self, delta: u64) {
        // Scale by inverse weight for fairness
        let delta_weighted = delta * NICE_0_WEIGHT / self.weight.max(1);
        self.vruntime = self.vruntime.saturating_add(delta_weighted);
        self.sum_exec_runtime = self.sum_exec_runtime.saturating_add(delta);
    }
}

/// Weight for nice value 0
const NICE_0_WEIGHT: u64 = 1024;

/// Convert nice value to weight
fn nice_to_weight(nice: i32) -> u64 {
    // Each nice level is ~1.25x
    let nice = nice.clamp(MIN_NICE, MAX_NICE);
    if nice == 0 {
        NICE_0_WEIGHT
    } else if nice < 0 {
        NICE_0_WEIGHT << ((-nice as u32) / 5)
    } else {
        NICE_0_WEIGHT >> ((nice as u32) / 5)
    }
}

/// Per-CPU run queue
pub struct CpuRunQueue {
    /// CPU ID
    pub cpu_id: u32,
    /// Lock for this run queue
    lock: Spinlock<()>,
    /// CFS run queue
    pub cfs: cfs::CfsRunQueue,
    /// Real-time run queue
    pub rt: realtime::RtRunQueue,
    /// Deadline run queue
    pub dl: realtime::DeadlineRunQueue,
    /// Currently running entity
    pub current: Option<Tid>,
    /// Idle entity
    pub idle: Option<Tid>,
    /// Number of runnable threads
    pub nr_running: u32,
    /// Total weight of runnable threads
    pub total_weight: u64,
    /// Clock (nanoseconds)
    pub clock: AtomicU64,
    /// Need to reschedule flag
    pub need_resched: AtomicBool,
    /// Statistics
    pub stats: RunQueueStats,
}

/// Run queue statistics
#[derive(Default)]
pub struct RunQueueStats {
    /// Total context switches
    pub nr_switches: u64,
    /// Forced switches (preemption)
    pub nr_preempt: u64,
    /// Voluntary switches (yield, block)
    pub nr_voluntary: u64,
    /// Load balancing events
    pub nr_load_balance: u64,
    /// Tasks pulled from other CPUs
    pub nr_pulled: u64,
    /// Tasks pushed to other CPUs
    pub nr_pushed: u64,
}

impl CpuRunQueue {
    /// Create a new per-CPU run queue
    pub fn new(cpu_id: u32) -> Self {
        Self {
            cpu_id,
            lock: Spinlock::new(()),
            cfs: cfs::CfsRunQueue::new(),
            rt: realtime::RtRunQueue::new(),
            dl: realtime::DeadlineRunQueue::new(),
            current: None,
            idle: None,
            nr_running: 0,
            total_weight: 0,
            clock: AtomicU64::new(0),
            need_resched: AtomicBool::new(false),
            stats: RunQueueStats::default(),
        }
    }

    /// Lock the run queue
    pub fn lock(&self) -> impl Drop + '_ {
        self.lock.lock()
    }

    /// Update the clock
    pub fn update_clock(&self, now: u64) {
        self.clock.store(now, Ordering::Release);
    }

    /// Get current clock value
    pub fn clock(&self) -> u64 {
        self.clock.load(Ordering::Acquire)
    }

    /// Mark as needing reschedule
    pub fn set_need_resched(&self) {
        self.need_resched.store(true, Ordering::Release);
    }

    /// Clear need reschedule and return previous value
    pub fn clear_need_resched(&self) -> bool {
        self.need_resched.swap(false, Ordering::AcqRel)
    }

    /// Enqueue a thread
    pub fn enqueue(&mut self, entity: &mut SchedEntity) {
        // Note: lock not needed here since we already have &mut self

        match entity.params.policy {
            SchedulingPolicy::Fifo | SchedulingPolicy::RoundRobin => {
                self.rt.enqueue(entity);
            }
            SchedulingPolicy::Deadline => {
                if let Some(dl_params) = entity.params.deadline.clone() {
                    let now = crate::drivers::timer::monotonic_ns();
                    let _ = self.dl.enqueue(entity.tid, dl_params, now);
                }
            }
            _ => {
                self.cfs.enqueue(entity);
            }
        }

        entity.on_rq = true;
        entity.cpu = self.cpu_id;
        self.nr_running += 1;
        self.total_weight += entity.weight;
    }

    /// Dequeue a thread
    pub fn dequeue(&mut self, entity: &mut SchedEntity) {
        // Note: lock not needed here since we already have &mut self

        match entity.params.policy {
            SchedulingPolicy::Fifo | SchedulingPolicy::RoundRobin => {
                self.rt.dequeue(entity);
            }
            SchedulingPolicy::Deadline => {
                self.dl.dequeue(entity.tid);
            }
            _ => {
                self.cfs.dequeue(entity);
            }
        }

        entity.on_rq = false;
        self.nr_running -= 1;
        self.total_weight = self.total_weight.saturating_sub(entity.weight);
    }

    /// Pick next thread to run
    pub fn pick_next(&mut self) -> Option<Tid> {
        // Note: lock not needed here since we already have &mut self

        // Priority order: Deadline > RT > CFS > Idle

        // Check deadline tasks first
        if let Some(tid) = self.dl.pick_next() {
            return Some(tid);
        }

        // Check real-time tasks
        if let Some(tid) = self.rt.pick_next() {
            return Some(tid);
        }

        // Check CFS tasks
        if let Some(tid) = self.cfs.pick_next() {
            return Some(tid);
        }

        // Return idle task
        self.idle
    }

    /// Calculate load average
    pub fn load(&self) -> u64 {
        // Weighted load based on total weight
        self.total_weight
    }
}

/// Global scheduler state
pub struct GlobalScheduler {
    /// Per-CPU run queues
    run_queues: Vec<CpuRunQueue>,
    /// Number of CPUs
    nr_cpus: u32,
    /// All scheduling entities
    entities: RwLock<BTreeMap<Tid, SchedEntity>>,
    /// Load balancer
    load_balancer: LoadBalancer,
    /// Scheduler is running
    running: AtomicBool,
    /// Global statistics
    stats: Mutex<GlobalSchedStats>,
}

/// Global scheduler statistics
#[derive(Default)]
pub struct GlobalSchedStats {
    /// Total threads scheduled
    pub total_scheduled: u64,
    /// Total migrations
    pub total_migrations: u64,
    /// Load balance iterations
    pub load_balance_count: u64,
}

impl GlobalScheduler {
    /// Create a new global scheduler
    pub fn new(nr_cpus: u32) -> Self {
        let mut run_queues = Vec::with_capacity(nr_cpus as usize);
        for cpu_id in 0..nr_cpus {
            run_queues.push(CpuRunQueue::new(cpu_id));
        }

        Self {
            run_queues,
            nr_cpus,
            entities: RwLock::new(BTreeMap::new()),
            load_balancer: LoadBalancer::new(nr_cpus, Vec::new()),
            running: AtomicBool::new(false),
            stats: Mutex::new(GlobalSchedStats::default()),
        }
    }

    /// Register a thread with the scheduler
    pub fn register(&self, tid: Tid, params: SchedParams) {
        let mut entity = SchedEntity::new(tid);
        entity.params = params.clone();
        entity.weight = nice_to_weight(params.priority);

        // Select initial CPU
        let cpu = self.select_cpu(&entity);
        entity.cpu = cpu;

        self.entities.write().insert(tid, entity);
    }

    /// Unregister a thread
    pub fn unregister(&self, tid: Tid) {
        self.entities.write().remove(&tid);
    }

    /// Enqueue a thread to run
    pub fn enqueue(&self, tid: Tid) {
        let mut entities = self.entities.write();
        if let Some(entity) = entities.get_mut(&tid) {
            let cpu = entity.cpu as usize;
            if cpu < self.run_queues.len() {
                // Would need interior mutability for run_queues
                // self.run_queues[cpu].enqueue(entity);
            }
        }
    }

    /// Dequeue a thread
    pub fn dequeue(&self, tid: Tid) {
        let mut entities = self.entities.write();
        if let Some(entity) = entities.get_mut(&tid) {
            let cpu = entity.cpu as usize;
            if cpu < self.run_queues.len() {
                // Would need interior mutability for run_queues
                // self.run_queues[cpu].dequeue(entity);
            }
        }
    }

    /// Select CPU for a new thread
    fn select_cpu(&self, entity: &SchedEntity) -> u32 {
        // Check affinity mask
        let allowed = &entity.params.affinity;

        // Find least loaded allowed CPU
        let mut best_cpu = 0;
        let mut min_load = u64::MAX;

        for (cpu, rq) in self.run_queues.iter().enumerate() {
            if allowed.contains(cpu as u32) {
                let load = rq.load();
                if load < min_load {
                    min_load = load;
                    best_cpu = cpu as u32;
                }
            }
        }

        best_cpu
    }

    /// Schedule on current CPU
    pub fn schedule(&self, cpu: u32) -> Option<Tid> {
        if (cpu as usize) < self.run_queues.len() {
            // Would pick next from run queue
            None
        } else {
            None
        }
    }

    /// Timer tick
    pub fn tick(&self, cpu: u32) {
        if (cpu as usize) < self.run_queues.len() {
            let rq = &self.run_queues[cpu as usize];
            rq.set_need_resched();
        }
    }

    /// Trigger load balancing on a specific CPU
    pub fn balance(&self, _cpu: u32, _now: u64) -> Option<BalanceDecision> {
        // Would need mutable access to load_balancer
        // self.load_balancer.balance(cpu, now)
        None
    }

    /// Trigger load balancing on all CPUs
    pub fn balance_all(&self, now: u64) {
        for cpu in 0..self.nr_cpus {
            let _ = self.balance(cpu, now);
        }
    }

    /// Set thread scheduling parameters
    pub fn set_params(&self, tid: Tid, params: SchedParams) {
        let mut entities = self.entities.write();
        if let Some(entity) = entities.get_mut(&tid) {
            entity.params = params.clone();
            entity.weight = nice_to_weight(params.priority);
        }
    }

    /// Get thread scheduling parameters
    pub fn get_params(&self, tid: Tid) -> Option<SchedParams> {
        self.entities.read().get(&tid).map(|e| e.params.clone())
    }

    /// Set thread nice value
    pub fn set_nice(&self, tid: Tid, nice: i32) {
        let mut entities = self.entities.write();
        if let Some(entity) = entities.get_mut(&tid) {
            let nice = nice.clamp(MIN_NICE, MAX_NICE);
            entity.params.priority = nice;
            entity.weight = nice_to_weight(nice);
        }
    }

    /// Get thread nice value
    pub fn get_nice(&self, tid: Tid) -> i32 {
        self.entities.read()
            .get(&tid)
            .map(|e| e.params.priority)
            .unwrap_or(DEFAULT_NICE)
    }

    /// Set thread affinity
    pub fn set_affinity(&self, tid: Tid, affinity: CpuAffinity) {
        let mut entities = self.entities.write();
        if let Some(entity) = entities.get_mut(&tid) {
            entity.params.affinity = affinity;
        }
    }

    /// Get thread affinity
    pub fn get_affinity(&self, tid: Tid) -> CpuAffinity {
        self.entities.read()
            .get(&tid)
            .map(|e| e.params.affinity.clone())
            .unwrap_or_else(CpuAffinity::all)
    }

    /// Yield current thread on CPU
    pub fn yield_to(&self, cpu: u32, next: Option<Tid>) {
        // Yield to specific thread or next in queue
        let _ = (cpu, next);
    }

    /// Get scheduler statistics
    pub fn stats(&self) -> GlobalSchedStats {
        let stats = self.stats.lock();
        GlobalSchedStats {
            total_scheduled: stats.total_scheduled,
            total_migrations: stats.total_migrations,
            load_balance_count: stats.load_balance_count,
        }
    }
}

/// Global scheduler instance
static GLOBAL_SCHEDULER: RwLock<Option<GlobalScheduler>> = RwLock::new(None);

/// Initialize the scheduler
pub fn init() {
    // Initialize with 1 CPU by default, can be updated later for SMP
    init_advanced(1);
}

/// Initialize the advanced scheduler
pub fn init_advanced(nr_cpus: u32) {
    let mut sched = GLOBAL_SCHEDULER.write();
    *sched = Some(GlobalScheduler::new(nr_cpus));
}

/// Get the global scheduler
pub fn global() -> impl core::ops::Deref<Target = Option<GlobalScheduler>> + 'static {
    GLOBAL_SCHEDULER.read()
}

/// Run the scheduler - main scheduler loop
pub fn run() -> ! {
    loop {
        // Run scheduler tick
        let cpu = crate::cpu::current_cpu();
        if let Some(ref sched) = *global() {
            sched.yield_to(cpu, None);
        }

        // Wait for next interrupt
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack));
        }
    }
}

/// Yield CPU to scheduler - voluntarily give up time slice
pub fn yield_now() {
    let cpu = crate::cpu::current_cpu();
    if let Some(ref sched) = *global() {
        sched.yield_to(cpu, None);
    }
}

/// Timer tick handler - called from timer interrupt
pub fn timer_tick() {
    crate::sched::timer_tick();
}

/// Timer tick handler with interrupt frame
pub fn timer_tick_with_frame(_frame: &crate::interrupts::InterruptFrame) {
    timer_tick();
}

/// Add a thread to the scheduler
pub fn add_thread(tid: u32, priority: u8) {
    use crate::sched::{SchedParams, SchedClass, CpuMask};
    let params = SchedParams {
        class: SchedClass::Normal,
        priority: priority as i32,
        time_slice_ns: 4_000_000, // 4ms default
        affinity: CpuMask::all(),
        numa_node: 0,
        rt_priority: 0,
        deadline: None,
        ai_workload: false,
    };
    crate::sched::enqueue(crate::process::Tid::new(tid as u64), &params);
}

/// Remove a thread from the scheduler
pub fn remove_thread(tid: u32) {
    crate::sched::dequeue(crate::process::Tid::new(tid as u64));
}

/// Yield CPU (alias for yield_now)
pub fn yield_cpu() {
    yield_now();
}

/// AI priority boost for a thread
pub fn ai_priority_boost(tid: u32, boost: f32) {
    crate::sched::ai_boost(crate::process::Tid::new(tid as u64), boost);
}
