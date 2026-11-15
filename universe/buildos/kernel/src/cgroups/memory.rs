// ===============================================================================
// BUILDOS KERNEL - CGROUPS MEMORY CONTROLLER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Memory Controller for cgroups v2
//!
//! Provides memory resource control:
//! - Memory limits (hard and soft)
//! - Memory usage accounting
//! - OOM control
//! - Memory pressure events
//! - Swap control

use alloc::string::{String, ToString};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use super::CgroupError;

/// Maximum memory value
pub const MEMORY_MAX: u64 = u64::MAX;

/// Initialize memory controller
pub fn init() {
    crate::kprintln!("[CGROUPS] Memory controller initialized");
}

/// Memory controller state
pub struct MemoryController {
    /// Current memory usage (bytes)
    pub current: AtomicU64,
    /// Memory minimum (protection)
    pub min: AtomicU64,
    /// Memory low (soft protection)
    pub low: AtomicU64,
    /// Memory high (throttle point)
    pub high: AtomicU64,
    /// Memory max (hard limit)
    pub max: AtomicU64,
    /// Swap current usage
    pub swap_current: AtomicU64,
    /// Swap high (throttle)
    pub swap_high: AtomicU64,
    /// Swap max (hard limit)
    pub swap_max: AtomicU64,
    /// OOM group flag
    pub oom_group: AtomicBool,
    /// Statistics
    pub stats: MemoryStats,
    /// Events
    pub events: MemoryEvents,
}

impl MemoryController {
    /// Create new memory controller
    pub fn new() -> Self {
        Self {
            current: AtomicU64::new(0),
            min: AtomicU64::new(0),
            low: AtomicU64::new(0),
            high: AtomicU64::new(MEMORY_MAX),
            max: AtomicU64::new(MEMORY_MAX),
            swap_current: AtomicU64::new(0),
            swap_high: AtomicU64::new(MEMORY_MAX),
            swap_max: AtomicU64::new(MEMORY_MAX),
            oom_group: AtomicBool::new(false),
            stats: MemoryStats::new(),
            events: MemoryEvents::new(),
        }
    }

    /// Charge memory to this controller
    pub fn charge(&self, bytes: u64) -> Result<(), CgroupError> {
        let current = self.current.load(Ordering::Relaxed);
        let max = self.max.load(Ordering::Relaxed);

        if current + bytes > max {
            self.events.max.fetch_add(1, Ordering::Relaxed);
            return Err(CgroupError::ResourceLimitExceeded);
        }

        let high = self.high.load(Ordering::Relaxed);
        if current + bytes > high {
            self.events.high.fetch_add(1, Ordering::Relaxed);
            // Throttle but allow
        }

        self.current.fetch_add(bytes, Ordering::AcqRel);
        self.stats.anon.fetch_add(bytes, Ordering::Relaxed);

        Ok(())
    }

    /// Uncharge memory from this controller
    pub fn uncharge(&self, bytes: u64) {
        let prev = self.current.fetch_sub(bytes, Ordering::AcqRel);
        if prev < bytes {
            self.current.store(0, Ordering::Release);
        }
    }

    /// Check if memory is below minimum
    pub fn is_protected(&self) -> bool {
        let current = self.current.load(Ordering::Relaxed);
        let min = self.min.load(Ordering::Relaxed);
        current <= min
    }

    /// Check if memory is low
    pub fn is_low(&self) -> bool {
        let current = self.current.load(Ordering::Relaxed);
        let low = self.low.load(Ordering::Relaxed);
        current <= low
    }

    /// Calculate memory pressure
    pub fn pressure(&self) -> MemoryPressure {
        let current = self.current.load(Ordering::Relaxed);
        let max = self.max.load(Ordering::Relaxed);
        let high = self.high.load(Ordering::Relaxed);

        if current >= max {
            MemoryPressure::Critical
        } else if current >= high {
            MemoryPressure::High
        } else if current >= high / 2 {
            MemoryPressure::Medium
        } else {
            MemoryPressure::Low
        }
    }
}

impl Default for MemoryController {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory statistics
pub struct MemoryStats {
    /// Anonymous memory
    pub anon: AtomicU64,
    /// File-backed memory
    pub file: AtomicU64,
    /// Kernel memory
    pub kernel: AtomicU64,
    /// Kernel stack
    pub kernel_stack: AtomicU64,
    /// Page tables
    pub pagetables: AtomicU64,
    /// Slab reclaimable
    pub slab_reclaimable: AtomicU64,
    /// Slab unreclaimable
    pub slab_unreclaimable: AtomicU64,
    /// Sockets
    pub sock: AtomicU64,
    /// Shmem
    pub shmem: AtomicU64,
    /// Active anonymous
    pub active_anon: AtomicU64,
    /// Inactive anonymous
    pub inactive_anon: AtomicU64,
    /// Active file
    pub active_file: AtomicU64,
    /// Inactive file
    pub inactive_file: AtomicU64,
    /// Page faults
    pub pgfault: AtomicU64,
    /// Major page faults
    pub pgmajfault: AtomicU64,
    /// Pages refilled
    pub pgrefill: AtomicU64,
    /// Pages scanned
    pub pgscan: AtomicU64,
    /// Pages stolen
    pub pgsteal: AtomicU64,
    /// Pages activated
    pub pgactivate: AtomicU64,
    /// Pages deactivated
    pub pgdeactivate: AtomicU64,
    /// Pages lazyfree'd
    pub pglazyfree: AtomicU64,
    /// THP fault allocations
    pub thp_fault_alloc: AtomicU64,
    /// THP collapse allocations
    pub thp_collapse_alloc: AtomicU64,
}

impl MemoryStats {
    pub fn new() -> Self {
        Self {
            anon: AtomicU64::new(0),
            file: AtomicU64::new(0),
            kernel: AtomicU64::new(0),
            kernel_stack: AtomicU64::new(0),
            pagetables: AtomicU64::new(0),
            slab_reclaimable: AtomicU64::new(0),
            slab_unreclaimable: AtomicU64::new(0),
            sock: AtomicU64::new(0),
            shmem: AtomicU64::new(0),
            active_anon: AtomicU64::new(0),
            inactive_anon: AtomicU64::new(0),
            active_file: AtomicU64::new(0),
            inactive_file: AtomicU64::new(0),
            pgfault: AtomicU64::new(0),
            pgmajfault: AtomicU64::new(0),
            pgrefill: AtomicU64::new(0),
            pgscan: AtomicU64::new(0),
            pgsteal: AtomicU64::new(0),
            pgactivate: AtomicU64::new(0),
            pgdeactivate: AtomicU64::new(0),
            pglazyfree: AtomicU64::new(0),
            thp_fault_alloc: AtomicU64::new(0),
            thp_collapse_alloc: AtomicU64::new(0),
        }
    }

    pub fn format(&self) -> String {
        alloc::format!(
            "anon {}\n\
             file {}\n\
             kernel {}\n\
             kernel_stack {}\n\
             pagetables {}\n\
             slab_reclaimable {}\n\
             slab_unreclaimable {}\n\
             sock {}\n\
             shmem {}\n\
             active_anon {}\n\
             inactive_anon {}\n\
             active_file {}\n\
             inactive_file {}\n\
             pgfault {}\n\
             pgmajfault {}",
            self.anon.load(Ordering::Relaxed),
            self.file.load(Ordering::Relaxed),
            self.kernel.load(Ordering::Relaxed),
            self.kernel_stack.load(Ordering::Relaxed),
            self.pagetables.load(Ordering::Relaxed),
            self.slab_reclaimable.load(Ordering::Relaxed),
            self.slab_unreclaimable.load(Ordering::Relaxed),
            self.sock.load(Ordering::Relaxed),
            self.shmem.load(Ordering::Relaxed),
            self.active_anon.load(Ordering::Relaxed),
            self.inactive_anon.load(Ordering::Relaxed),
            self.active_file.load(Ordering::Relaxed),
            self.inactive_file.load(Ordering::Relaxed),
            self.pgfault.load(Ordering::Relaxed),
            self.pgmajfault.load(Ordering::Relaxed),
        )
    }
}

impl Default for MemoryStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory events
pub struct MemoryEvents {
    /// Times memory.low was exceeded
    pub low: AtomicU64,
    /// Times memory.high was exceeded
    pub high: AtomicU64,
    /// Times memory.max was hit
    pub max: AtomicU64,
    /// OOM kills
    pub oom: AtomicU64,
    /// OOM group kills
    pub oom_group_kill: AtomicU64,
    /// OOM kills (local)
    pub oom_kill: AtomicU64,
}

impl MemoryEvents {
    pub fn new() -> Self {
        Self {
            low: AtomicU64::new(0),
            high: AtomicU64::new(0),
            max: AtomicU64::new(0),
            oom: AtomicU64::new(0),
            oom_group_kill: AtomicU64::new(0),
            oom_kill: AtomicU64::new(0),
        }
    }

    pub fn format(&self) -> String {
        alloc::format!(
            "low {}\nhigh {}\nmax {}\noom {}\noom_kill {}\noom_group_kill {}",
            self.low.load(Ordering::Relaxed),
            self.high.load(Ordering::Relaxed),
            self.max.load(Ordering::Relaxed),
            self.oom.load(Ordering::Relaxed),
            self.oom_kill.load(Ordering::Relaxed),
            self.oom_group_kill.load(Ordering::Relaxed),
        )
    }
}

impl Default for MemoryEvents {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory pressure level
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryPressure {
    Low,
    Medium,
    High,
    Critical,
}

/// Read a memory controller file
pub fn read_file(controller: &MemoryController, file: &str) -> Result<String, CgroupError> {
    match file {
        "current" => Ok(controller.current.load(Ordering::Relaxed).to_string()),
        "min" => Ok(format_limit(controller.min.load(Ordering::Relaxed))),
        "low" => Ok(format_limit(controller.low.load(Ordering::Relaxed))),
        "high" => Ok(format_limit(controller.high.load(Ordering::Relaxed))),
        "max" => Ok(format_limit(controller.max.load(Ordering::Relaxed))),
        "swap.current" => Ok(controller.swap_current.load(Ordering::Relaxed).to_string()),
        "swap.high" => Ok(format_limit(controller.swap_high.load(Ordering::Relaxed))),
        "swap.max" => Ok(format_limit(controller.swap_max.load(Ordering::Relaxed))),
        "oom.group" => Ok(if controller.oom_group.load(Ordering::Relaxed) { "1" } else { "0" }.to_string()),
        "stat" => Ok(controller.stats.format()),
        "events" => Ok(controller.events.format()),
        "peak" => {
            // Peak memory usage - would track this separately
            Ok(controller.current.load(Ordering::Relaxed).to_string())
        }
        "pressure" => {
            // PSI-style pressure info
            Ok("some avg10=0.00 avg60=0.00 avg300=0.00 total=0\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=0".to_string())
        }
        _ => Err(CgroupError::NotFound),
    }
}

/// Write a memory controller file
pub fn write_file(controller: &mut MemoryController, file: &str, value: &str) -> Result<(), CgroupError> {
    let value = value.trim();

    match file {
        "min" => {
            let bytes = parse_limit(value)?;
            controller.min.store(bytes, Ordering::Release);
            Ok(())
        }
        "low" => {
            let bytes = parse_limit(value)?;
            controller.low.store(bytes, Ordering::Release);
            Ok(())
        }
        "high" => {
            let bytes = parse_limit(value)?;
            controller.high.store(bytes, Ordering::Release);
            Ok(())
        }
        "max" => {
            let bytes = parse_limit(value)?;
            controller.max.store(bytes, Ordering::Release);
            Ok(())
        }
        "swap.high" => {
            let bytes = parse_limit(value)?;
            controller.swap_high.store(bytes, Ordering::Release);
            Ok(())
        }
        "swap.max" => {
            let bytes = parse_limit(value)?;
            controller.swap_max.store(bytes, Ordering::Release);
            Ok(())
        }
        "oom.group" => {
            let val = value == "1";
            controller.oom_group.store(val, Ordering::Release);
            Ok(())
        }
        _ => Err(CgroupError::NotFound),
    }
}

/// Format a limit value
fn format_limit(value: u64) -> String {
    if value == MEMORY_MAX {
        "max".to_string()
    } else {
        value.to_string()
    }
}

/// Parse a limit value (supports "max", bytes, K, M, G suffixes)
fn parse_limit(s: &str) -> Result<u64, CgroupError> {
    if s == "max" {
        return Ok(MEMORY_MAX);
    }

    let s = s.trim();
    let (num_str, multiplier) = if s.ends_with('K') || s.ends_with('k') {
        (&s[..s.len()-1], 1024u64)
    } else if s.ends_with('M') || s.ends_with('m') {
        (&s[..s.len()-1], 1024 * 1024)
    } else if s.ends_with('G') || s.ends_with('g') {
        (&s[..s.len()-1], 1024 * 1024 * 1024)
    } else if s.ends_with('T') || s.ends_with('t') {
        (&s[..s.len()-1], 1024 * 1024 * 1024 * 1024)
    } else {
        (s, 1)
    };

    let num: u64 = num_str.parse()
        .map_err(|_| CgroupError::InvalidPath)?;

    Ok(num * multiplier)
}

/// Apply memory limits to a process
pub fn apply_to_process(pid: u32, controller: &MemoryController) -> Result<(), CgroupError> {
    // Would set up memory accounting for this process
    // This integrates with the memory allocator

    let max = controller.max.load(Ordering::Relaxed);
    if max != MEMORY_MAX {
        // Set up hard limit tracking
        crate::kprintln!("[CGROUPS] Process {} memory limit: {} bytes", pid, max);
    }

    Ok(())
}

/// Check memory allocation against cgroup limits
pub fn check_allocation(pid: u32, bytes: u64) -> Result<(), CgroupError> {
    // Find process's cgroup
    if let Some(path) = super::get_cgroup(pid) {
        let hierarchy = super::HIERARCHY.read();
        if let Some(cgroup) = hierarchy.get(&path) {
            return cgroup.memory.charge(bytes);
        }
    }

    Ok(())
}

/// Track memory release
pub fn track_free(pid: u32, bytes: u64) {
    if let Some(path) = super::get_cgroup(pid) {
        let hierarchy = super::HIERARCHY.read();
        if let Some(cgroup) = hierarchy.get(&path) {
            cgroup.memory.uncharge(bytes);
        }
    }
}

/// Trigger OOM killer for a cgroup
pub fn oom_kill(path: &str) -> Result<u32, CgroupError> {
    let hierarchy = super::HIERARCHY.read();
    let cgroup = hierarchy.get(path).ok_or(CgroupError::NotFound)?;

    // Select victim process (simplified: just pick first)
    if let Some(&pid) = cgroup.procs.first() {
        cgroup.memory.events.oom_kill.fetch_add(1, Ordering::Relaxed);
        // Would actually kill the process
        crate::kprintln!("[CGROUPS] OOM killed process {} in {}", pid, path);
        return Ok(pid);
    }

    Err(CgroupError::NotFound)
}
