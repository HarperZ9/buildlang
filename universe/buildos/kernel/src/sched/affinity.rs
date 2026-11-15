// ===============================================================================
// BUILDOS KERNEL - CPU AFFINITY MANAGEMENT
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! CPU affinity and cpuset management.
//!
//! This module provides:
//! - Per-thread CPU affinity masks
//! - Cpusets for resource partitioning
//! - CPU isolation for real-time workloads

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::{Mutex, RwLock};

use crate::process::Tid;
use super::{CpuMask, MAX_CPUS};

// =============================================================================
// AFFINITY STORAGE
// =============================================================================

/// Per-thread affinity storage
static THREAD_AFFINITY: RwLock<BTreeMap<Tid, CpuMask>> = RwLock::new(BTreeMap::new());

/// Set thread CPU affinity
pub fn set_affinity(tid: Tid, mask: CpuMask) {
    let mut affinity_map = THREAD_AFFINITY.write();
    affinity_map.insert(tid, mask);

    // If thread is currently running on disallowed CPU, trigger migration
    // check_migration_needed(tid, &mask);
}

/// Get thread CPU affinity
pub fn get_affinity(tid: Tid) -> CpuMask {
    let affinity_map = THREAD_AFFINITY.read();
    affinity_map.get(&tid).cloned().unwrap_or_else(CpuMask::all)
}

/// Clear affinity when thread exits
pub fn clear_affinity(tid: Tid) {
    let mut affinity_map = THREAD_AFFINITY.write();
    affinity_map.remove(&tid);
}

/// Check if thread can run on CPU
pub fn can_run_on(tid: Tid, cpu: usize) -> bool {
    get_affinity(tid).is_set(cpu)
}

// =============================================================================
// CPUSETS
// =============================================================================

/// Cpuset hierarchy
static CPUSETS: RwLock<CpusetHierarchy> = RwLock::new(CpusetHierarchy::new());

/// Cpuset hierarchy (cgroup-like)
pub struct CpusetHierarchy {
    /// Root cpuset
    root: Option<Cpuset>,

    /// Named cpusets
    sets: BTreeMap<String, Cpuset>,

    /// Next cpuset ID
    next_id: AtomicU32,
}

impl CpusetHierarchy {
    const fn new() -> Self {
        Self {
            root: None,
            sets: BTreeMap::new(),
            next_id: AtomicU32::new(1),
        }
    }

    /// Initialize with root cpuset
    pub fn init(&mut self, num_cpus: usize) {
        let mut cpus = CpuMask::empty();
        for cpu in 0..num_cpus {
            cpus.set(cpu);
        }

        self.root = Some(Cpuset {
            id: 0,
            name: String::from("root"),
            cpus,
            mems: MemMask::all(),
            threads: Vec::new(),
            children: Vec::new(),
            parent: None,
            flags: CpusetFlags::empty(),
            exclusive: false,
        });
    }
}

/// A cpuset (group of CPUs and memory nodes)
pub struct Cpuset {
    /// Cpuset ID
    pub id: u32,

    /// Name
    pub name: String,

    /// Allowed CPUs
    pub cpus: CpuMask,

    /// Allowed memory nodes
    pub mems: MemMask,

    /// Threads in this cpuset
    pub threads: Vec<Tid>,

    /// Child cpusets
    pub children: Vec<u32>,

    /// Parent cpuset
    pub parent: Option<u32>,

    /// Flags
    pub flags: CpusetFlags,

    /// Exclusive (no overlap with siblings)
    pub exclusive: bool,
}

/// Memory node mask
#[derive(Clone)]
pub struct MemMask {
    bits: u64,
}

impl MemMask {
    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    pub const fn all() -> Self {
        Self { bits: u64::MAX }
    }

    pub fn set(&mut self, node: usize) {
        if node < 64 {
            self.bits |= 1 << node;
        }
    }

    pub fn is_set(&self, node: usize) -> bool {
        if node < 64 {
            (self.bits & (1 << node)) != 0
        } else {
            false
        }
    }
}

bitflags::bitflags! {
    /// Cpuset flags
    #[derive(Clone, Copy, Debug, Default)]
    pub struct CpusetFlags: u32 {
        /// Balance load within cpuset
        const LOAD_BALANCE = 1 << 0;
        /// Memory pressure spread
        const MEMORY_PRESSURE = 1 << 1;
        /// Memory spread across nodes
        const MEMORY_SPREAD = 1 << 2;
        /// CPU hardwall (strict isolation)
        const CPU_HARDWALL = 1 << 3;
        /// Memory hardwall
        const MEM_HARDWALL = 1 << 4;
        /// Notify on changes
        const NOTIFY_ON_RELEASE = 1 << 5;
    }
}

/// Create a new cpuset
pub fn create_cpuset(
    name: &str,
    parent: Option<u32>,
    cpus: CpuMask,
    mems: MemMask,
) -> Option<u32> {
    let mut hierarchy = CPUSETS.write();

    let id = hierarchy.next_id.fetch_add(1, Ordering::Relaxed);

    let cpuset = Cpuset {
        id,
        name: String::from(name),
        cpus,
        mems,
        threads: Vec::new(),
        children: Vec::new(),
        parent,
        flags: CpusetFlags::LOAD_BALANCE,
        exclusive: false,
    };

    hierarchy.sets.insert(String::from(name), cpuset);

    // Add to parent's children
    if let Some(parent_id) = parent {
        for set in hierarchy.sets.values_mut() {
            if set.id == parent_id {
                set.children.push(id);
                break;
            }
        }
    }

    Some(id)
}

/// Move thread to cpuset
pub fn move_to_cpuset(tid: Tid, cpuset_name: &str) -> bool {
    let mut hierarchy = CPUSETS.write();

    // Check if target cpuset exists
    if !hierarchy.sets.contains_key(cpuset_name) {
        return false;
    }

    // Remove from all cpusets first
    for set in hierarchy.sets.values_mut() {
        set.threads.retain(|&t| t != tid);
    }

    // Now add to the target cpuset
    if let Some(cpuset) = hierarchy.sets.get_mut(cpuset_name) {
        cpuset.threads.push(tid);

        // Get the CPUs for affinity update
        let cpus = cpuset.cpus.clone();

        // Update thread's affinity to cpuset's allowed CPUs
        drop(hierarchy);
        set_affinity(tid, cpus);

        return true;
    }

    false
}

/// Get cpuset for thread
pub fn get_cpuset(tid: Tid) -> Option<String> {
    let hierarchy = CPUSETS.read();

    for (name, cpuset) in &hierarchy.sets {
        if cpuset.threads.contains(&tid) {
            return Some(name.clone());
        }
    }

    // Default to root
    Some(String::from("root"))
}

// =============================================================================
// CPU ISOLATION
// =============================================================================

/// Isolated CPUs (removed from general scheduling)
static ISOLATED_CPUS: RwLock<CpuMask> = RwLock::new(CpuMask::empty());

/// Housekeeping CPUs (for kernel work)
static HOUSEKEEPING_CPUS: RwLock<CpuMask> = RwLock::new(CpuMask::all());

/// Isolate a CPU from general scheduling
pub fn isolate_cpu(cpu: usize) {
    let mut isolated = ISOLATED_CPUS.write();
    isolated.set(cpu);

    let mut housekeeping = HOUSEKEEPING_CPUS.write();
    housekeeping.clear(cpu);

    // Migrate any threads off this CPU
    // migrate_away_from(cpu);
}

/// Un-isolate a CPU
pub fn unisolate_cpu(cpu: usize) {
    let mut isolated = ISOLATED_CPUS.write();
    isolated.clear(cpu);

    let mut housekeeping = HOUSEKEEPING_CPUS.write();
    housekeeping.set(cpu);
}

/// Check if CPU is isolated
pub fn is_isolated(cpu: usize) -> bool {
    ISOLATED_CPUS.read().is_set(cpu)
}

/// Get housekeeping CPU mask
pub fn housekeeping_mask() -> CpuMask {
    HOUSEKEEPING_CPUS.read().clone()
}

/// Check if CPU is for housekeeping
pub fn is_housekeeping(cpu: usize) -> bool {
    HOUSEKEEPING_CPUS.read().is_set(cpu)
}

// =============================================================================
// NOHZ (TICKLESS) SUPPORT
// =============================================================================

/// CPUs in NOHZ (tickless) mode
static NOHZ_CPUS: Mutex<CpuMask> = Mutex::new(CpuMask::empty());

/// Enter NOHZ mode on CPU
pub fn enter_nohz(cpu: usize) {
    NOHZ_CPUS.lock().set(cpu);
}

/// Exit NOHZ mode on CPU
pub fn exit_nohz(cpu: usize) {
    NOHZ_CPUS.lock().clear(cpu);
}

/// Check if CPU is in NOHZ mode
pub fn is_nohz(cpu: usize) -> bool {
    NOHZ_CPUS.lock().is_set(cpu)
}

/// Get mask of CPUs in NOHZ mode
pub fn nohz_mask() -> CpuMask {
    NOHZ_CPUS.lock().clone()
}

// =============================================================================
// AFFINITY SYSCALLS
// =============================================================================

/// sched_setaffinity syscall implementation
pub fn sys_setaffinity(tid: Tid, mask_ptr: *const u64, mask_len: usize) -> Result<(), i32> {
    if mask_len == 0 || mask_len > MAX_CPUS / 64 {
        return Err(-22); // EINVAL
    }

    // Would copy mask from user space
    let mut mask = CpuMask::empty();

    // For now, use all CPUs
    for cpu in 0..super::online_cpus() {
        mask.set(cpu);
    }

    set_affinity(tid, mask);

    let _ = mask_ptr;
    Ok(())
}

/// sched_getaffinity syscall implementation
pub fn sys_getaffinity(tid: Tid, mask_ptr: *mut u64, mask_len: usize) -> Result<usize, i32> {
    if mask_len == 0 {
        return Err(-22); // EINVAL
    }

    let _mask = get_affinity(tid);

    // Would copy mask to user space
    let _ = mask_ptr;

    // Return number of bytes written
    Ok(mask_len.min(MAX_CPUS / 8))
}
