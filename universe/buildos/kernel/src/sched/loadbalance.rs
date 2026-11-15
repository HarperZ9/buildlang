// ===============================================================================
// BUILDOS KERNEL - SCHEDULER LOAD BALANCING
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! Load balancing and work stealing for SMP scheduler.
//!
//! This module provides:
//! - Periodic load balancing across CPUs
//! - Work stealing from busy to idle CPUs
//! - NUMA-aware migration decisions
//! - Scheduler domains for hierarchical balancing

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use spin::RwLock;

use crate::process::Tid;
use super::percpu;

// =============================================================================
// CONSTANTS
// =============================================================================

/// Minimum load imbalance to trigger migration
const IMBALANCE_THRESHOLD: u64 = 25;

/// Maximum threads to migrate at once
const MAX_MIGRATE_BATCH: usize = 4;

/// Work stealing search depth
const STEAL_SEARCH_DEPTH: usize = 4;

/// NUMA migration cost factor
const NUMA_MIGRATION_COST: u64 = 10;

/// Cache hot threshold (ns since last run)
const CACHE_HOT_NS: u64 = 2_000_000; // 2ms

// =============================================================================
// SCHEDULER DOMAINS
// =============================================================================

/// Scheduler domain hierarchy for load balancing
static SCHED_DOMAINS: RwLock<SchedDomains> = RwLock::new(SchedDomains::new());

/// Scheduler domain hierarchy
pub struct SchedDomains {
    /// Domain levels
    levels: Vec<DomainLevel>,

    /// Is initialized
    initialized: bool,
}

/// A level in the scheduler domain hierarchy
pub struct DomainLevel {
    /// Level name (e.g., "SMT", "MC", "NUMA")
    pub name: &'static str,

    /// Domains at this level
    pub domains: Vec<SchedDomain>,

    /// Balance interval (ticks)
    pub balance_interval: u64,
}

/// A scheduler domain (group of CPUs)
pub struct SchedDomain {
    /// Domain ID
    pub id: usize,

    /// CPUs in this domain
    pub cpus: Vec<usize>,

    /// Child domains (lower level)
    pub children: Vec<usize>,

    /// Parent domain (higher level)
    pub parent: Option<usize>,

    /// Domain flags
    pub flags: DomainFlags,

    /// Last balance timestamp
    pub last_balance: AtomicU64,

    /// Balance in progress
    pub balancing: AtomicBool,
}

bitflags::bitflags! {
    /// Scheduler domain flags
    #[derive(Clone, Copy, Debug, Default)]
    pub struct DomainFlags: u32 {
        /// Balance on exec
        const BALANCE_EXEC = 1 << 0;
        /// Balance on fork
        const BALANCE_FORK = 1 << 1;
        /// Balance on wake
        const BALANCE_WAKE = 1 << 2;
        /// This is a NUMA domain
        const NUMA = 1 << 3;
        /// Prefer local CPU
        const PREFER_LOCAL = 1 << 4;
    }
}

impl SchedDomains {
    const fn new() -> Self {
        Self {
            levels: Vec::new(),
            initialized: false,
        }
    }

    /// Build scheduler domains from topology
    pub fn build(&mut self, num_cpus: usize) {
        // Level 0: Per-CPU (SMT siblings if hyperthreading)
        let mut cpu_domain = DomainLevel {
            name: "CPU",
            domains: Vec::new(),
            balance_interval: 1,
        };

        for cpu in 0..num_cpus {
            cpu_domain.domains.push(SchedDomain {
                id: cpu,
                cpus: vec![cpu],
                children: Vec::new(),
                parent: Some(0), // MC level
                flags: DomainFlags::PREFER_LOCAL | DomainFlags::BALANCE_WAKE,
                last_balance: AtomicU64::new(0),
                balancing: AtomicBool::new(false),
            });
        }
        self.levels.push(cpu_domain);

        // Level 1: Multi-core (all CPUs on same socket/die)
        let mut mc_domain = DomainLevel {
            name: "MC",
            domains: Vec::new(),
            balance_interval: 4,
        };

        mc_domain.domains.push(SchedDomain {
            id: 0,
            cpus: (0..num_cpus).collect(),
            children: (0..num_cpus).collect(),
            parent: None, // Top level for now
            flags: DomainFlags::BALANCE_EXEC | DomainFlags::BALANCE_FORK,
            last_balance: AtomicU64::new(0),
            balancing: AtomicBool::new(false),
        });
        self.levels.push(mc_domain);

        // Level 2: NUMA (if applicable)
        // Would be built from NUMA topology

        self.initialized = true;
    }
}

// =============================================================================
// LOAD STATISTICS
// =============================================================================

/// Load statistics for a CPU or domain
#[derive(Default, Clone)]
pub struct LoadStats {
    /// Number of running threads
    pub nr_running: u32,
    /// Weighted load
    pub load: u64,
    /// Recent average load
    pub load_avg: u64,
    /// CPU capacity
    pub capacity: u64,
    /// Utilization (load / capacity)
    pub utilization: u32,
}

/// Calculate load statistics for a CPU
pub fn cpu_load_stats(cpu: usize) -> LoadStats {
    let stats = percpu::get_stats(cpu);

    LoadStats {
        nr_running: stats.nr_running,
        load: stats.load,
        load_avg: stats.load, // Would use EWMA
        capacity: 1024, // Normalized capacity
        utilization: ((stats.load * 100) / 1024).min(100) as u32,
    }
}

/// Calculate aggregate load for a domain
pub fn domain_load_stats(domain: &SchedDomain) -> LoadStats {
    let mut stats = LoadStats::default();

    for &cpu in &domain.cpus {
        let cpu_stats = cpu_load_stats(cpu);
        stats.nr_running += cpu_stats.nr_running;
        stats.load += cpu_stats.load;
        stats.load_avg += cpu_stats.load_avg;
        stats.capacity += cpu_stats.capacity;
    }

    if stats.capacity > 0 {
        stats.utilization = ((stats.load * 100) / stats.capacity).min(100) as u32;
    }

    stats
}

// =============================================================================
// LOAD BALANCING
// =============================================================================

/// Global load balancer state
static LOAD_BALANCER: LoadBalancer = LoadBalancer::new();

pub struct LoadBalancer {
    /// Total migrations performed
    migrations: AtomicU64,
    /// Failed migration attempts
    failed_migrations: AtomicU64,
    /// Work steal successes
    work_steals: AtomicU64,
    /// Active rebalancing
    active: AtomicBool,
}

impl LoadBalancer {
    const fn new() -> Self {
        Self {
            migrations: AtomicU64::new(0),
            failed_migrations: AtomicU64::new(0),
            work_steals: AtomicU64::new(0),
            active: AtomicBool::new(false),
        }
    }
}

/// Initialize load balancing
pub fn init() {
    let mut domains = SCHED_DOMAINS.write();
    let num_cpus = super::online_cpus();
    domains.build(num_cpus);
}

/// Perform load balancing across all CPUs
pub fn balance_all() {
    // Prevent concurrent balancing
    if LOAD_BALANCER.active.swap(true, Ordering::Acquire) {
        return;
    }

    let domains = SCHED_DOMAINS.read();
    if !domains.initialized {
        LOAD_BALANCER.active.store(false, Ordering::Release);
        return;
    }

    // Balance at each domain level
    for level in &domains.levels {
        for domain in &level.domains {
            balance_domain(domain);
        }
    }

    LOAD_BALANCER.active.store(false, Ordering::Release);
}

/// Balance load within a scheduler domain
fn balance_domain(domain: &SchedDomain) {
    // Check if we need to balance
    if domain.balancing.swap(true, Ordering::Acquire) {
        return;
    }

    // Find busiest and idlest CPUs
    let (busiest, idlest) = find_busiest_and_idlest(&domain.cpus);

    if let (Some(busy_cpu), Some(idle_cpu)) = (busiest, idlest) {
        let busy_load = percpu::get_load(busy_cpu);
        let idle_load = percpu::get_load(idle_cpu);

        // Check if imbalance exceeds threshold
        if busy_load > idle_load + IMBALANCE_THRESHOLD {
            let to_migrate = calculate_migration_amount(busy_load, idle_load);
            migrate_tasks(busy_cpu, idle_cpu, to_migrate);
        }
    }

    domain.balancing.store(false, Ordering::Release);
}

/// Find busiest and idlest CPUs in a set
fn find_busiest_and_idlest(cpus: &[usize]) -> (Option<usize>, Option<usize>) {
    let mut busiest: Option<(usize, u64)> = None;
    let mut idlest: Option<(usize, u64)> = None;

    for &cpu in cpus {
        let load = percpu::get_load(cpu);

        match &busiest {
            None => busiest = Some((cpu, load)),
            Some((_, max_load)) if load > *max_load => busiest = Some((cpu, load)),
            _ => {}
        }

        match &idlest {
            None => idlest = Some((cpu, load)),
            Some((_, min_load)) if load < *min_load => idlest = Some((cpu, load)),
            _ => {}
        }
    }

    (busiest.map(|(c, _)| c), idlest.map(|(c, _)| c))
}

/// Calculate how much load to migrate
fn calculate_migration_amount(busy_load: u64, idle_load: u64) -> u64 {
    // Try to equalize load
    let imbalance = busy_load.saturating_sub(idle_load);
    imbalance / 2
}

/// Migrate tasks from busy CPU to idle CPU
fn migrate_tasks(from_cpu: usize, to_cpu: usize, _target_load: u64) {
    // Would select tasks to migrate based on:
    // - Not pinned to source CPU
    // - Allowed to run on dest CPU (affinity)
    // - Not cache hot (recently run)
    // - NUMA distance acceptable

    let migrated = 0;

    // For now, just record the attempt
    if migrated > 0 {
        LOAD_BALANCER.migrations.fetch_add(migrated, Ordering::Relaxed);
    }

    let _ = (from_cpu, to_cpu);
}

// =============================================================================
// WORK STEALING
// =============================================================================

/// Try to steal work for an idle CPU
pub fn try_steal_work(idle_cpu: usize) -> Option<Tid> {
    // Search nearby CPUs for stealable work
    let domains = SCHED_DOMAINS.read();
    if !domains.initialized {
        return None;
    }

    // Start with closest CPUs (same domain)
    for level in &domains.levels {
        for domain in &level.domains {
            if domain.cpus.contains(&idle_cpu) {
                if let Some(tid) = steal_from_domain(idle_cpu, domain) {
                    LOAD_BALANCER.work_steals.fetch_add(1, Ordering::Relaxed);
                    return Some(tid);
                }
            }
        }
    }

    None
}

/// Steal from CPUs in a domain
fn steal_from_domain(idle_cpu: usize, domain: &SchedDomain) -> Option<Tid> {
    for &victim_cpu in &domain.cpus {
        if victim_cpu == idle_cpu {
            continue;
        }

        // Check if victim has stealable work
        let stats = percpu::get_stats(victim_cpu);
        if stats.nr_running > 1 {
            // Try to steal oldest runnable task
            // (Would actually steal from victim's run queue)
            return None;
        }
    }
    None
}

// =============================================================================
// NUMA-AWARE BALANCING
// =============================================================================

/// NUMA balancing state
pub struct NumaBalancer {
    /// Scan rate (pages per second)
    pub scan_rate: u64,
    /// Pages scanned
    pub pages_scanned: AtomicU64,
    /// Migrations performed
    pub migrations: AtomicU64,
}

impl NumaBalancer {
    const fn new() -> Self {
        Self {
            scan_rate: 1000,
            pages_scanned: AtomicU64::new(0),
            migrations: AtomicU64::new(0),
        }
    }
}

/// Check if task should be migrated for NUMA locality
pub fn should_numa_migrate(
    tid: Tid,
    current_node: u32,
    preferred_node: u32,
) -> bool {
    if current_node == preferred_node {
        return false;
    }

    // Check NUMA distance
    let distance = crate::memory::numa::distance(current_node, preferred_node);

    // Only migrate if significant benefit
    distance > 20 && {
        // Check task's memory access pattern
        // Would analyze page fault data
        let _ = tid;
        true
    }
}

/// Get preferred NUMA node for a task based on memory accesses
pub fn get_preferred_numa_node(_tid: Tid) -> u32 {
    // Would analyze task's page fault data to determine
    // which node has most of its memory
    0
}

// =============================================================================
// STATISTICS
// =============================================================================

/// Load balancing statistics
#[derive(Default)]
pub struct BalanceStats {
    pub migrations: u64,
    pub failed_migrations: u64,
    pub work_steals: u64,
}

/// Get load balancing statistics
pub fn get_stats() -> BalanceStats {
    BalanceStats {
        migrations: LOAD_BALANCER.migrations.load(Ordering::Relaxed),
        failed_migrations: LOAD_BALANCER.failed_migrations.load(Ordering::Relaxed),
        work_steals: LOAD_BALANCER.work_steals.load(Ordering::Relaxed),
    }
}
