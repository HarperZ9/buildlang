//! SMP Load Balancing
//!
//! Distributes tasks across CPUs to maximize throughput and minimize latency.

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use crate::process::Tid;
use super::affinity::{CpuSet, SchedDomain};

/// Load balancing interval (in nanoseconds)
pub const BALANCE_INTERVAL_NS: u64 = 4_000_000; // 4ms

/// Migration cost threshold (nanoseconds of cache warmup)
pub const MIGRATION_COST_NS: u64 = 500_000; // 0.5ms

/// Imbalance threshold (percentage)
pub const IMBALANCE_PCT: u32 = 25;

/// CPU load statistics
#[derive(Clone, Debug, Default)]
pub struct CpuLoad {
    /// Number of runnable tasks
    pub nr_running: u32,
    /// Weighted load (sum of task weights)
    pub load: u64,
    /// Average load (exponentially weighted)
    pub avg_load: u64,
    /// CPU capacity (relative to reference CPU)
    pub capacity: u64,
    /// CPU utilization (0-100)
    pub util: u32,
    /// Idle time (nanoseconds)
    pub idle_time: u64,
    /// Last update timestamp
    pub last_update: u64,
}

impl CpuLoad {
    /// Create new CPU load tracking
    pub fn new(capacity: u64) -> Self {
        Self {
            capacity,
            ..Default::default()
        }
    }

    /// Update load statistics
    pub fn update(&mut self, nr_running: u32, load: u64, now: u64) {
        self.nr_running = nr_running;
        self.load = load;

        // Exponential moving average: avg = alpha * current + (1-alpha) * old_avg
        // Using alpha = 1/8 for stability
        self.avg_load = (load + 7 * self.avg_load) / 8;

        // Calculate utilization
        if self.capacity > 0 {
            self.util = ((load * 100) / self.capacity).min(100) as u32;
        }

        self.last_update = now;
    }

    /// Get load per capacity (for comparing different CPU types)
    pub fn load_per_capacity(&self) -> u64 {
        if self.capacity > 0 {
            (self.load << 10) / self.capacity
        } else {
            self.load << 10
        }
    }

    /// Is this CPU idle?
    pub fn is_idle(&self) -> bool {
        self.nr_running == 0
    }

    /// Is this CPU overloaded?
    pub fn is_overloaded(&self, threshold: u32) -> bool {
        self.util > threshold
    }
}

/// Group load statistics (for scheduling domains)
#[derive(Clone, Debug, Default)]
pub struct GroupLoad {
    /// CPUs in this group
    pub cpus: CpuSet,
    /// Total load
    pub load: u64,
    /// Total capacity
    pub capacity: u64,
    /// Number of runnable tasks
    pub nr_running: u32,
    /// Number of idle CPUs
    pub idle_cpus: u32,
    /// Average load per CPU
    pub avg_load: u64,
    /// Group type
    pub group_type: GroupType,
}

/// Classification of scheduling group load
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GroupType {
    /// Group has no runnable tasks
    #[default]
    HasSpare,
    /// Group is fully utilized but not overloaded
    FullyBusy,
    /// Group is overloaded (more tasks than CPUs)
    Overloaded,
    /// Group has misfit tasks (needs more capacity)
    Misfit,
    /// Group has imbalanced NUMA tasks
    Imbalanced,
}

impl GroupLoad {
    /// Calculate group statistics
    pub fn calculate(&mut self, cpu_loads: &BTreeMap<u32, CpuLoad>) {
        self.load = 0;
        self.capacity = 0;
        self.nr_running = 0;
        self.idle_cpus = 0;

        for cpu in self.cpus.iter() {
            if let Some(load) = cpu_loads.get(&cpu) {
                self.load += load.load;
                self.capacity += load.capacity;
                self.nr_running += load.nr_running;
                if load.is_idle() {
                    self.idle_cpus += 1;
                }
            }
        }

        let nr_cpus = self.cpus.count();
        self.avg_load = if nr_cpus > 0 {
            self.load / nr_cpus as u64
        } else {
            0
        };

        // Classify group
        self.group_type = if self.nr_running == 0 {
            GroupType::HasSpare
        } else if self.nr_running > nr_cpus {
            GroupType::Overloaded
        } else if self.idle_cpus == 0 {
            GroupType::FullyBusy
        } else {
            GroupType::HasSpare
        };
    }

    /// Get imbalance amount
    pub fn imbalance(&self, other: &GroupLoad) -> u64 {
        if self.capacity == 0 || other.capacity == 0 {
            return 0;
        }

        let self_load_per_cap = (self.load << 10) / self.capacity;
        let other_load_per_cap = (other.load << 10) / other.capacity;

        if self_load_per_cap > other_load_per_cap {
            self_load_per_cap - other_load_per_cap
        } else {
            0
        }
    }
}

/// Load balancer
pub struct LoadBalancer {
    /// Per-CPU load statistics
    cpu_loads: BTreeMap<u32, CpuLoad>,
    /// Scheduling domains
    domains: Vec<SchedDomain>,
    /// Last balance time per domain
    last_balance: Vec<u64>,
    /// Migration statistics
    stats: MigrationStats,
    /// Number of CPUs
    nr_cpus: u32,
    /// Load balancing enabled
    enabled: bool,
}

/// Migration statistics
#[derive(Clone, Debug, Default)]
pub struct MigrationStats {
    /// Total migrations
    pub total_migrations: u64,
    /// Pull migrations (idle CPU pulls work)
    pub pull_migrations: u64,
    /// Push migrations (busy CPU pushes work)
    pub push_migrations: u64,
    /// Failed migrations (no suitable task)
    pub failed_migrations: u64,
    /// Forced migrations (affinity change)
    pub forced_migrations: u64,
    /// NUMA migrations
    pub numa_migrations: u64,
}

impl LoadBalancer {
    /// Create a new load balancer
    pub fn new(nr_cpus: u32, domains: Vec<SchedDomain>) -> Self {
        let mut cpu_loads = BTreeMap::new();
        for cpu in 0..nr_cpus {
            cpu_loads.insert(cpu, CpuLoad::new(1024)); // Assume uniform capacity
        }

        let last_balance = vec![0u64; domains.len()];

        Self {
            cpu_loads,
            domains,
            last_balance,
            stats: MigrationStats::default(),
            nr_cpus,
            enabled: true,
        }
    }

    /// Enable/disable load balancing
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Update CPU load
    pub fn update_load(&mut self, cpu: u32, nr_running: u32, load: u64, now: u64) {
        if let Some(cpu_load) = self.cpu_loads.get_mut(&cpu) {
            cpu_load.update(nr_running, load, now);
        }
    }

    /// Get CPU load
    pub fn get_load(&self, cpu: u32) -> Option<&CpuLoad> {
        self.cpu_loads.get(&cpu)
    }

    /// Find busiest CPU in a set
    pub fn find_busiest_cpu(&self, cpus: &CpuSet) -> Option<u32> {
        cpus.iter()
            .filter_map(|cpu| {
                self.cpu_loads.get(&cpu).map(|load| (cpu, load))
            })
            .max_by_key(|(_, load)| load.load)
            .map(|(cpu, _)| cpu)
    }

    /// Find idlest CPU in a set
    pub fn find_idlest_cpu(&self, cpus: &CpuSet) -> Option<u32> {
        cpus.iter()
            .filter_map(|cpu| {
                self.cpu_loads.get(&cpu).map(|load| (cpu, load))
            })
            .min_by_key(|(_, load)| load.load)
            .map(|(cpu, _)| cpu)
    }

    /// Find an idle CPU in a set
    pub fn find_idle_cpu(&self, cpus: &CpuSet) -> Option<u32> {
        cpus.iter()
            .find(|&cpu| {
                self.cpu_loads.get(&cpu)
                    .map(|load| load.is_idle())
                    .unwrap_or(false)
            })
    }

    /// Check if load balancing is needed
    pub fn needs_balance(&self, cpu: u32, now: u64) -> bool {
        if !self.enabled {
            return false;
        }

        // Find domains containing this CPU
        for (idx, domain) in self.domains.iter().enumerate() {
            if domain.contains(cpu) {
                let interval = domain.balance_interval * BALANCE_INTERVAL_NS;
                if now - self.last_balance[idx] >= interval {
                    return true;
                }
            }
        }

        false
    }

    /// Perform load balancing
    pub fn balance(&mut self, cpu: u32, now: u64) -> Option<BalanceDecision> {
        if !self.enabled {
            return None;
        }

        let cpu_load = self.cpu_loads.get(&cpu)?;

        // If this CPU is idle, try to pull work
        if cpu_load.is_idle() {
            return self.idle_balance(cpu, now);
        }

        // If this CPU is busy, try periodic rebalance
        self.periodic_balance(cpu, now)
    }

    /// Idle CPU trying to find work (pull migration)
    fn idle_balance(&mut self, idle_cpu: u32, now: u64) -> Option<BalanceDecision> {
        // Search through domains from smallest to largest
        for (idx, domain) in self.domains.iter().enumerate() {
            if !domain.contains(idle_cpu) {
                continue;
            }

            // Find busiest CPU in this domain
            if let Some(busiest) = self.find_busiest_in_domain(domain) {
                if busiest != idle_cpu {
                    let busiest_load = self.cpu_loads.get(&busiest)?;
                    if busiest_load.nr_running > 1 {
                        self.last_balance[idx] = now;
                        self.stats.pull_migrations += 1;
                        self.stats.total_migrations += 1;

                        return Some(BalanceDecision {
                            src_cpu: busiest,
                            dst_cpu: idle_cpu,
                            reason: BalanceReason::IdlePull,
                            task_count: 1,
                        });
                    }
                }
            }
        }

        None
    }

    /// Periodic load balancing
    fn periodic_balance(&mut self, cpu: u32, now: u64) -> Option<BalanceDecision> {
        for (idx, domain) in self.domains.iter().enumerate() {
            if !domain.contains(cpu) {
                continue;
            }

            let interval = domain.balance_interval * BALANCE_INTERVAL_NS;
            if now - self.last_balance[idx] < interval {
                continue;
            }

            self.last_balance[idx] = now;

            // Calculate group loads
            let mut local_group = GroupLoad {
                cpus: CpuSet::single(cpu),
                ..Default::default()
            };
            local_group.calculate(&self.cpu_loads);

            let mut domain_group = GroupLoad {
                cpus: domain.span.clone(),
                ..Default::default()
            };
            domain_group.calculate(&self.cpu_loads);

            // Check for imbalance
            let imbalance = local_group.imbalance(&domain_group);
            if imbalance > (IMBALANCE_PCT as u64) << 10 / 100 {
                if let Some(idlest) = self.find_idlest_cpu(&domain.span) {
                    if idlest != cpu && local_group.load > 0 {
                        self.stats.push_migrations += 1;
                        self.stats.total_migrations += 1;

                        return Some(BalanceDecision {
                            src_cpu: cpu,
                            dst_cpu: idlest,
                            reason: BalanceReason::LoadBalance,
                            task_count: 1,
                        });
                    }
                }
            }
        }

        None
    }

    /// Find busiest CPU in a domain
    fn find_busiest_in_domain(&self, domain: &SchedDomain) -> Option<u32> {
        self.find_busiest_cpu(&domain.span)
    }

    /// Get migration statistics
    pub fn stats(&self) -> &MigrationStats {
        &self.stats
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = MigrationStats::default();
    }
}

/// Load balance decision
#[derive(Clone, Debug)]
pub struct BalanceDecision {
    /// Source CPU
    pub src_cpu: u32,
    /// Destination CPU
    pub dst_cpu: u32,
    /// Reason for migration
    pub reason: BalanceReason,
    /// Number of tasks to migrate
    pub task_count: u32,
}

/// Reason for task migration
#[derive(Clone, Copy, Debug)]
pub enum BalanceReason {
    /// Idle CPU pulling work
    IdlePull,
    /// Periodic load balancing
    LoadBalance,
    /// Affinity change forced migration
    AffinityChange,
    /// NUMA balancing
    NumaBalance,
    /// Cache hot migration
    CacheHot,
    /// Power management
    PowerSaving,
}

/// Task migration
pub struct TaskMigration {
    /// Pending migrations
    pending: BTreeMap<Tid, PendingMigration>,
    /// Migration queue per CPU
    queues: BTreeMap<u32, Vec<Tid>>,
    /// Maximum pending migrations per CPU
    max_pending: usize,
}

/// Pending migration request
#[derive(Clone, Debug)]
pub struct PendingMigration {
    /// Task to migrate
    pub tid: Tid,
    /// Target CPU
    pub dst_cpu: u32,
    /// Reason
    pub reason: BalanceReason,
    /// Request time
    pub request_time: u64,
    /// Is urgent (must complete soon)
    pub urgent: bool,
}

impl TaskMigration {
    /// Create new migration manager
    pub fn new() -> Self {
        Self {
            pending: BTreeMap::new(),
            queues: BTreeMap::new(),
            max_pending: 32,
        }
    }

    /// Request a task migration
    pub fn request(&mut self, tid: Tid, dst_cpu: u32, reason: BalanceReason, now: u64) -> bool {
        // Check if already pending
        if self.pending.contains_key(&tid) {
            return false;
        }

        // Check queue limit
        let queue = self.queues.entry(dst_cpu).or_insert_with(Vec::new);
        if queue.len() >= self.max_pending {
            return false;
        }

        let migration = PendingMigration {
            tid,
            dst_cpu,
            reason,
            request_time: now,
            urgent: matches!(reason, BalanceReason::AffinityChange),
        };

        self.pending.insert(tid, migration);
        queue.push(tid);
        true
    }

    /// Cancel a pending migration
    pub fn cancel(&mut self, tid: Tid) {
        if let Some(migration) = self.pending.remove(&tid) {
            if let Some(queue) = self.queues.get_mut(&migration.dst_cpu) {
                queue.retain(|&t| t != tid);
            }
        }
    }

    /// Get next migration for a CPU
    pub fn next_for_cpu(&mut self, cpu: u32) -> Option<PendingMigration> {
        let queue = self.queues.get_mut(&cpu)?;
        if queue.is_empty() {
            return None;
        }

        let tid = queue.remove(0);
        self.pending.remove(&tid)
    }

    /// Check if task has pending migration
    pub fn is_pending(&self, tid: Tid) -> bool {
        self.pending.contains_key(&tid)
    }

    /// Get pending migration for task
    pub fn get_pending(&self, tid: Tid) -> Option<&PendingMigration> {
        self.pending.get(&tid)
    }

    /// Complete a migration
    pub fn complete(&mut self, tid: Tid) {
        self.pending.remove(&tid);
    }

    /// Expire old migrations
    pub fn expire(&mut self, now: u64, max_age_ns: u64) {
        let expired: Vec<_> = self.pending.iter()
            .filter(|(_, m)| now - m.request_time > max_age_ns && !m.urgent)
            .map(|(&tid, _)| tid)
            .collect();

        for tid in expired {
            self.cancel(tid);
        }
    }
}

/// NUMA balancing for memory locality
pub struct NumaBalancer {
    /// Task NUMA stats
    task_stats: BTreeMap<Tid, NumaTaskStats>,
    /// Scan period (nanoseconds)
    scan_period: u64,
    /// Migration threshold
    threshold: u32,
}

/// Per-task NUMA statistics
#[derive(Clone, Debug, Default)]
pub struct NumaTaskStats {
    /// Preferred node (most memory access)
    pub preferred_node: u32,
    /// Pages accessed on each node
    pub node_pages: BTreeMap<u32, u64>,
    /// Faults on each node
    pub node_faults: BTreeMap<u32, u64>,
    /// Last scan time
    pub last_scan: u64,
    /// Total scanned pages
    pub scanned_pages: u64,
}

impl NumaBalancer {
    /// Create new NUMA balancer
    pub fn new() -> Self {
        Self {
            task_stats: BTreeMap::new(),
            scan_period: 1_000_000_000, // 1 second
            threshold: 70, // 70% of accesses on one node
        }
    }

    /// Record a NUMA fault
    pub fn record_fault(&mut self, tid: Tid, node: u32) {
        let stats = self.task_stats.entry(tid).or_default();
        *stats.node_faults.entry(node).or_insert(0) += 1;
    }

    /// Analyze task placement
    pub fn analyze(&mut self, tid: Tid) -> Option<NumaAdvice> {
        let stats = self.task_stats.get(&tid)?;

        // Find node with most faults
        let total_faults: u64 = stats.node_faults.values().sum();
        if total_faults == 0 {
            return None;
        }

        let (best_node, best_faults) = stats.node_faults.iter()
            .max_by_key(|(_, &f)| f)?;

        let percentage = (*best_faults * 100) / total_faults;

        if percentage >= self.threshold as u64 {
            if *best_node != stats.preferred_node {
                return Some(NumaAdvice::Migrate { to_node: *best_node });
            }
        }

        None
    }

    /// Update preferred node
    pub fn update_preferred(&mut self, tid: Tid, node: u32) {
        if let Some(stats) = self.task_stats.get_mut(&tid) {
            stats.preferred_node = node;
        }
    }

    /// Clear task stats
    pub fn clear(&mut self, tid: Tid) {
        self.task_stats.remove(&tid);
    }
}

/// NUMA balancing advice
#[derive(Clone, Debug)]
pub enum NumaAdvice {
    /// Migrate task to node
    Migrate { to_node: u32 },
    /// Migrate memory to current node
    MigrateMemory,
    /// No change needed
    None,
}

/// Power-aware load balancing
pub struct PowerBalancer {
    /// Power states per CPU
    cpu_power: BTreeMap<u32, CpuPowerState>,
    /// Target power level
    target: PowerLevel,
    /// Consolidation enabled
    consolidate: bool,
}

/// CPU power state
#[derive(Clone, Debug)]
pub struct CpuPowerState {
    /// Current C-state
    pub cstate: u8,
    /// Current P-state (frequency level)
    pub pstate: u8,
    /// Time in current state
    pub time_in_state: u64,
    /// Is fully online
    pub online: bool,
}

/// Power level target
#[derive(Clone, Copy, Debug)]
pub enum PowerLevel {
    /// Maximum performance
    Performance,
    /// Balanced
    Balanced,
    /// Power saving
    PowerSave,
    /// Maximum power saving
    LowPower,
}

impl PowerBalancer {
    /// Create new power balancer
    pub fn new(nr_cpus: u32) -> Self {
        let mut cpu_power = BTreeMap::new();
        for cpu in 0..nr_cpus {
            cpu_power.insert(cpu, CpuPowerState {
                cstate: 0,
                pstate: 0,
                time_in_state: 0,
                online: true,
            });
        }

        Self {
            cpu_power,
            target: PowerLevel::Balanced,
            consolidate: false,
        }
    }

    /// Set power target
    pub fn set_target(&mut self, target: PowerLevel) {
        self.target = target;
        self.consolidate = matches!(target, PowerLevel::PowerSave | PowerLevel::LowPower);
    }

    /// Should consolidate workloads?
    pub fn should_consolidate(&self) -> bool {
        self.consolidate
    }

    /// Get CPUs to prefer for scheduling
    pub fn preferred_cpus(&self, total_load: u64, nr_cpus: u32) -> CpuSet {
        if !self.consolidate {
            return CpuSet::all(nr_cpus);
        }

        // Calculate how many CPUs we need
        let cpus_needed = ((total_load / 1024) + 1).min(nr_cpus as u64);

        let mut preferred = CpuSet::empty();
        for cpu in 0..cpus_needed as u32 {
            preferred.set(cpu);
        }
        preferred
    }

    /// Update CPU power state
    pub fn update_state(&mut self, cpu: u32, cstate: u8, pstate: u8) {
        if let Some(state) = self.cpu_power.get_mut(&cpu) {
            state.cstate = cstate;
            state.pstate = pstate;
        }
    }
}
