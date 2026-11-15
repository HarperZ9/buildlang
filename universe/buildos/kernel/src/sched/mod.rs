// ===============================================================================
// BUILDOS KERNEL - ADVANCED SMP SCHEDULER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// Neural Process Scheduler
// ===============================================================================

#![allow(dead_code)]

//! Advanced SMP-aware scheduler with per-CPU queues and load balancing.
//!
//! This module provides:
//! - Per-CPU run queues for lock-free fast path
//! - Work stealing for automatic load balancing
//! - NUMA-aware thread placement
//! - Real-time scheduling classes
//! - CPU affinity and cpusets
//! - Deadline scheduling (EDF)
//! - Workqueue for deferred work execution

pub mod percpu;
pub mod loadbalance;
pub mod affinity;
pub mod classes;
pub mod workqueue;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, AtomicU32, AtomicBool, Ordering};
use spin::{Mutex, RwLock};

use crate::process::Tid;

// =============================================================================
// CONSTANTS
// =============================================================================

/// Maximum number of CPUs supported
pub const MAX_CPUS: usize = 256;

/// Number of priority levels per scheduling class
pub const PRIORITY_LEVELS: usize = 64;

/// Time slice duration in nanoseconds (10ms default)
pub const DEFAULT_TIME_SLICE_NS: u64 = 10_000_000;

/// Minimum time slice (1ms)
pub const MIN_TIME_SLICE_NS: u64 = 1_000_000;

/// Maximum time slice (100ms)
pub const MAX_TIME_SLICE_NS: u64 = 100_000_000;

/// Load balance interval in ticks
pub const LOAD_BALANCE_INTERVAL: u64 = 100;

/// Migration cost threshold (ns)
pub const MIGRATION_COST_NS: u64 = 500_000;

/// AI workload priority boost
pub const AI_PRIORITY_BOOST: i32 = 10;

// =============================================================================
// SCHEDULING CLASSES
// =============================================================================

/// Scheduling class (policy)
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum SchedClass {
    /// Idle - lowest priority, runs only when nothing else
    Idle = 0,
    /// Normal - standard time-sharing (CFS-like)
    Normal = 1,
    /// Batch - CPU-intensive, lower priority than interactive
    Batch = 2,
    /// AI - Neural workloads with ML-optimized scheduling
    AI = 3,
    /// Real-time round-robin
    RealTimeRR = 4,
    /// Real-time FIFO
    RealTimeFIFO = 5,
    /// Deadline - EDF scheduling
    Deadline = 6,
}

impl Default for SchedClass {
    fn default() -> Self {
        SchedClass::Normal
    }
}

// =============================================================================
// THREAD SCHEDULING PARAMETERS
// =============================================================================

/// Per-thread scheduling parameters
#[derive(Clone)]
pub struct SchedParams {
    /// Scheduling class
    pub class: SchedClass,

    /// Static priority (nice value -20 to 19 maps to 0-39)
    pub priority: i32,

    /// Time slice in nanoseconds
    pub time_slice_ns: u64,

    /// CPU affinity mask
    pub affinity: CpuMask,

    /// Preferred NUMA node
    pub numa_node: u32,

    /// Real-time priority (0-99)
    pub rt_priority: u32,

    /// Deadline parameters (for SCHED_DEADLINE)
    pub deadline: Option<DeadlineParams>,

    /// AI workload hint
    pub ai_workload: bool,
}

impl Default for SchedParams {
    fn default() -> Self {
        Self {
            class: SchedClass::Normal,
            priority: 20, // Nice 0
            time_slice_ns: DEFAULT_TIME_SLICE_NS,
            affinity: CpuMask::all(),
            numa_node: 0,
            rt_priority: 0,
            deadline: None,
            ai_workload: false,
        }
    }
}

/// Deadline scheduling parameters (EDF)
#[derive(Clone, Copy)]
pub struct DeadlineParams {
    /// Runtime budget per period (ns)
    pub runtime_ns: u64,
    /// Deadline relative to period start (ns)
    pub deadline_ns: u64,
    /// Period length (ns)
    pub period_ns: u64,
}

// =============================================================================
// CPU AFFINITY
// =============================================================================

/// CPU affinity bitmask
#[derive(Clone)]
pub struct CpuMask {
    bits: [u64; MAX_CPUS / 64],
}

impl CpuMask {
    /// Create empty mask (no CPUs)
    pub const fn empty() -> Self {
        Self { bits: [0; MAX_CPUS / 64] }
    }

    /// Create mask with all CPUs
    pub const fn all() -> Self {
        Self { bits: [u64::MAX; MAX_CPUS / 64] }
    }

    /// Create mask with single CPU
    pub fn single(cpu: usize) -> Self {
        let mut mask = Self::empty();
        mask.set(cpu);
        mask
    }

    /// Set CPU in mask
    pub fn set(&mut self, cpu: usize) {
        if cpu < MAX_CPUS {
            self.bits[cpu / 64] |= 1 << (cpu % 64);
        }
    }

    /// Clear CPU from mask
    pub fn clear(&mut self, cpu: usize) {
        if cpu < MAX_CPUS {
            self.bits[cpu / 64] &= !(1 << (cpu % 64));
        }
    }

    /// Check if CPU is in mask
    pub fn is_set(&self, cpu: usize) -> bool {
        if cpu < MAX_CPUS {
            (self.bits[cpu / 64] & (1 << (cpu % 64))) != 0
        } else {
            false
        }
    }

    /// Count set CPUs
    pub fn count(&self) -> usize {
        self.bits.iter().map(|b| b.count_ones() as usize).sum()
    }

    /// Get first set CPU
    pub fn first(&self) -> Option<usize> {
        for (i, &bits) in self.bits.iter().enumerate() {
            if bits != 0 {
                return Some(i * 64 + bits.trailing_zeros() as usize);
            }
        }
        None
    }

    /// Iterate over set CPUs
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        (0..MAX_CPUS).filter(|&cpu| self.is_set(cpu))
    }
}

// =============================================================================
// THREAD RUNTIME STATE
// =============================================================================

/// Per-thread runtime scheduling state
pub struct ThreadSchedState {
    /// Thread ID
    pub tid: Tid,

    /// Scheduling parameters
    pub params: SchedParams,

    /// Virtual runtime (for CFS)
    pub vruntime: AtomicU64,

    /// Remaining time slice (ns)
    pub time_remaining: AtomicU64,

    /// CPU this thread last ran on
    pub last_cpu: AtomicU32,

    /// Total CPU time consumed (ns)
    pub total_runtime: AtomicU64,

    /// Number of voluntary context switches
    pub voluntary_switches: AtomicU64,

    /// Number of involuntary context switches
    pub involuntary_switches: AtomicU64,

    /// Is thread currently running?
    pub running: AtomicBool,

    /// Wake timestamp (for wait time tracking)
    pub wake_timestamp: AtomicU64,

    /// Sum of wait times
    pub total_wait_time: AtomicU64,

    /// AI predicted priority adjustment
    pub ai_priority_delta: AtomicU32,
}

impl ThreadSchedState {
    /// Create new thread scheduling state
    pub fn new(tid: Tid, params: SchedParams) -> Self {
        Self {
            tid,
            params,
            vruntime: AtomicU64::new(0),
            time_remaining: AtomicU64::new(DEFAULT_TIME_SLICE_NS),
            last_cpu: AtomicU32::new(0),
            total_runtime: AtomicU64::new(0),
            voluntary_switches: AtomicU64::new(0),
            involuntary_switches: AtomicU64::new(0),
            running: AtomicBool::new(false),
            wake_timestamp: AtomicU64::new(0),
            total_wait_time: AtomicU64::new(0),
            ai_priority_delta: AtomicU32::new(0),
        }
    }

    /// Get effective priority (static + AI adjustment)
    pub fn effective_priority(&self) -> i32 {
        let base = self.params.priority;
        let ai_delta = self.ai_priority_delta.load(Ordering::Relaxed) as i32;
        (base - ai_delta).clamp(0, PRIORITY_LEVELS as i32 - 1)
    }

    /// Update virtual runtime
    pub fn charge_runtime(&self, delta_ns: u64) {
        // CFS-style: vruntime += delta * weight
        let weight = self.priority_weight();
        let weighted_delta = delta_ns * 1024 / weight;
        self.vruntime.fetch_add(weighted_delta, Ordering::Relaxed);
        self.total_runtime.fetch_add(delta_ns, Ordering::Relaxed);
    }

    /// Get priority weight (inverse of nice)
    fn priority_weight(&self) -> u64 {
        // Linux-like weight table (simplified)
        let nice = self.params.priority as i64 - 20;
        let weight = 1024_i64 * (1 << (20 - nice.clamp(-20, 19))) / (1 << 20);
        weight.max(1) as u64
    }
}

// =============================================================================
// GLOBAL SCHEDULER STATE
// =============================================================================

/// Global scheduler
static GLOBAL_SCHEDULER: RwLock<GlobalScheduler> = RwLock::new(GlobalScheduler::new());

/// Global scheduler state
pub struct GlobalScheduler {
    /// Number of online CPUs
    online_cpus: usize,

    /// Per-CPU run queues
    cpu_rqs: [Option<*mut percpu::CpuRunQueue>; MAX_CPUS],

    /// Global run queue (for unbound threads)
    global_rq: Mutex<Vec<Tid>>,

    /// Load balance state
    load_balance_tick: AtomicU64,

    /// Total context switches
    total_switches: AtomicU64,

    /// Scheduler running
    running: AtomicBool,
}

// Safety: We carefully manage the raw pointers
unsafe impl Send for GlobalScheduler {}
unsafe impl Sync for GlobalScheduler {}

impl GlobalScheduler {
    const fn new() -> Self {
        Self {
            online_cpus: 1,
            cpu_rqs: [None; MAX_CPUS],
            global_rq: Mutex::new(Vec::new()),
            load_balance_tick: AtomicU64::new(0),
            total_switches: AtomicU64::new(0),
            running: AtomicBool::new(false),
        }
    }

    /// Initialize scheduler with number of CPUs
    pub fn init(&mut self, num_cpus: usize) {
        self.online_cpus = num_cpus.min(MAX_CPUS);

        // Initialize per-CPU run queues
        for cpu in 0..self.online_cpus {
            // Would allocate and initialize CpuRunQueue
            // self.cpu_rqs[cpu] = Some(Box::into_raw(Box::new(CpuRunQueue::new(cpu))));
            let _ = cpu;
        }
    }

    /// Get online CPU count
    pub fn online_cpus(&self) -> usize {
        self.online_cpus
    }
}

// =============================================================================
// SCHEDULER STATISTICS
// =============================================================================

/// Detailed scheduler statistics
#[derive(Default)]
pub struct SchedStats {
    /// Total context switches
    pub context_switches: u64,
    /// Voluntary context switches
    pub voluntary_switches: u64,
    /// Involuntary context switches (preemptions)
    pub involuntary_switches: u64,
    /// Load balance migrations
    pub migrations: u64,
    /// Work stealing events
    pub work_steals: u64,
    /// NUMA rebalances
    pub numa_rebalances: u64,
    /// RT throttle events
    pub rt_throttles: u64,
    /// Deadline misses
    pub deadline_misses: u64,
    /// AI priority adjustments
    pub ai_adjustments: u64,
}

// =============================================================================
// PUBLIC INTERFACE
// =============================================================================

/// Initialize the SMP scheduler
pub fn init(num_cpus: usize) {
    let mut sched = GLOBAL_SCHEDULER.write();
    sched.init(num_cpus);

    // Initialize per-CPU queues
    percpu::init(num_cpus);

    // Start load balancer
    loadbalance::init();
}

/// Initialize per-CPU scheduler on current CPU
pub fn init_cpu() {
    let cpu = crate::cpu::current_cpu_id() as usize;
    percpu::init_local(cpu);
}

/// Add thread to scheduler
pub fn enqueue(tid: Tid, params: &SchedParams) {
    let cpu = select_cpu(tid, params);
    percpu::enqueue_cpu(cpu, tid, params);
}

/// Remove thread from scheduler
pub fn dequeue(tid: Tid) {
    // Try to remove from all CPUs
    let sched = GLOBAL_SCHEDULER.read();
    for cpu in 0..sched.online_cpus {
        percpu::dequeue_cpu(cpu, tid);
    }
}

/// Select best CPU for a thread
fn select_cpu(tid: Tid, params: &SchedParams) -> usize {
    // First check affinity mask
    if params.affinity.count() == 1 {
        return params.affinity.first().unwrap_or(0);
    }

    // For NUMA-aware placement
    if params.numa_node != crate::memory::numa::NUMA_NO_NODE {
        // Try to find CPU on preferred NUMA node
        if let Some(cpu) = find_cpu_on_numa(params.numa_node, &params.affinity) {
            return cpu;
        }
    }

    // Find least loaded CPU in affinity mask
    find_least_loaded_cpu(&params.affinity, tid)
}

/// Find CPU on specific NUMA node
fn find_cpu_on_numa(node: u32, affinity: &CpuMask) -> Option<usize> {
    let topo = crate::memory::numa::topology().read();

    for cpu in affinity.iter() {
        if topo.cpu_node(cpu as u32) == node {
            return Some(cpu);
        }
    }
    None
}

/// Find least loaded CPU
fn find_least_loaded_cpu(affinity: &CpuMask, _tid: Tid) -> usize {
    let sched = GLOBAL_SCHEDULER.read();
    let mut best_cpu = 0;
    let mut best_load = u64::MAX;

    for cpu in affinity.iter() {
        if cpu < sched.online_cpus {
            let load = percpu::get_load(cpu);
            if load < best_load {
                best_load = load;
                best_cpu = cpu;
            }
        }
    }

    best_cpu
}

/// Yield current thread
pub fn yield_now() {
    let cpu = crate::cpu::current_cpu_id() as usize;
    percpu::yield_current(cpu);
}

/// Schedule on current CPU
pub fn schedule() {
    let cpu = crate::cpu::current_cpu_id() as usize;
    percpu::schedule(cpu);
}

/// Timer tick on current CPU
pub fn timer_tick() {
    let cpu = crate::cpu::current_cpu_id() as usize;
    percpu::timer_tick(cpu);

    // Periodic load balancing
    let tick = GLOBAL_SCHEDULER.read().load_balance_tick.fetch_add(1, Ordering::Relaxed);
    if tick % LOAD_BALANCE_INTERVAL == 0 {
        loadbalance::balance_all();
    }
}

/// Set thread affinity
pub fn set_affinity(tid: Tid, mask: CpuMask) {
    affinity::set_affinity(tid, mask);
}

/// Get thread affinity
pub fn get_affinity(tid: Tid) -> CpuMask {
    affinity::get_affinity(tid)
}

/// Apply AI priority boost
pub fn ai_boost(tid: Tid, boost: f32) {
    let delta = (boost * AI_PRIORITY_BOOST as f32) as u32;
    // Would look up thread and apply delta
    let _ = (tid, delta);
}

/// Get scheduler statistics
pub fn get_stats() -> SchedStats {
    let mut stats = SchedStats::default();

    let sched = GLOBAL_SCHEDULER.read();
    stats.context_switches = sched.total_switches.load(Ordering::Relaxed);

    // Aggregate from per-CPU queues
    for cpu in 0..sched.online_cpus {
        let cpu_stats = percpu::get_stats(cpu);
        stats.voluntary_switches += cpu_stats.voluntary_switches;
        stats.involuntary_switches += cpu_stats.involuntary_switches;
        stats.migrations += cpu_stats.migrations;
    }

    stats
}

/// Get number of online CPUs
pub fn online_cpus() -> usize {
    GLOBAL_SCHEDULER.read().online_cpus
}

/// Wake up a sleeping thread
pub fn wake_up(tid: Tid) {
    // Find which CPU the thread should wake on
    // For now, wake on current CPU
    let cpu = crate::cpu::current_cpu_id() as usize;
    percpu::wake_thread(cpu, tid);
}

/// Block current thread
pub fn block_current() {
    let cpu = crate::cpu::current_cpu_id() as usize;
    percpu::block_current(cpu);
}
