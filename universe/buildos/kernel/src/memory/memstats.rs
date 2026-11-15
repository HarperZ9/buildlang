// ===============================================================================
// BUILDOS KERNEL - MEMORY STATISTICS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Memory statistics and monitoring.
//!
//! This module provides comprehensive memory usage tracking:
//! - System-wide memory statistics
//! - Per-process memory accounting
//! - Memory pressure detection
//! - Memory usage trends and predictions

use core::sync::atomic::{AtomicU64, Ordering};
use alloc::vec::Vec;

use super::PAGE_SIZE;

// =============================================================================
// GLOBAL STATISTICS
// =============================================================================

/// Global memory statistics
static MEMSTATS: MemStats = MemStats::new();

/// System-wide memory statistics
pub struct MemStats {
    /// Total physical memory in bytes
    pub total: AtomicU64,

    /// Free physical memory in bytes
    pub free: AtomicU64,

    /// Available memory (free + reclaimable)
    pub available: AtomicU64,

    /// Memory used by kernel
    pub kernel: AtomicU64,

    /// Memory used by user processes
    pub user: AtomicU64,

    /// Memory used for page cache
    pub cached: AtomicU64,

    /// Memory used for buffers
    pub buffers: AtomicU64,

    /// Memory used for slab allocator
    pub slab: AtomicU64,

    /// Reclaimable slab memory
    pub slab_reclaimable: AtomicU64,

    /// Unreclaimable slab memory
    pub slab_unreclaimable: AtomicU64,

    /// Shared memory
    pub shared: AtomicU64,

    /// Anonymous memory
    pub anon: AtomicU64,

    /// Mapped memory
    pub mapped: AtomicU64,

    /// Dirty pages
    pub dirty: AtomicU64,

    /// Pages being written back
    pub writeback: AtomicU64,

    /// Swap total
    pub swap_total: AtomicU64,

    /// Swap free
    pub swap_free: AtomicU64,

    /// Swap cached
    pub swap_cached: AtomicU64,

    /// Huge pages total
    pub huge_pages_total: AtomicU64,

    /// Huge pages free
    pub huge_pages_free: AtomicU64,

    /// Huge page size
    pub huge_page_size: AtomicU64,

    /// Page faults (minor)
    pub page_faults_minor: AtomicU64,

    /// Page faults (major)
    pub page_faults_major: AtomicU64,

    /// Pages swapped in
    pub pages_swapped_in: AtomicU64,

    /// Pages swapped out
    pub pages_swapped_out: AtomicU64,

    /// OOM kill count
    pub oom_kills: AtomicU64,
}

impl MemStats {
    /// Create new memory statistics
    pub const fn new() -> Self {
        Self {
            total: AtomicU64::new(0),
            free: AtomicU64::new(0),
            available: AtomicU64::new(0),
            kernel: AtomicU64::new(0),
            user: AtomicU64::new(0),
            cached: AtomicU64::new(0),
            buffers: AtomicU64::new(0),
            slab: AtomicU64::new(0),
            slab_reclaimable: AtomicU64::new(0),
            slab_unreclaimable: AtomicU64::new(0),
            shared: AtomicU64::new(0),
            anon: AtomicU64::new(0),
            mapped: AtomicU64::new(0),
            dirty: AtomicU64::new(0),
            writeback: AtomicU64::new(0),
            swap_total: AtomicU64::new(0),
            swap_free: AtomicU64::new(0),
            swap_cached: AtomicU64::new(0),
            huge_pages_total: AtomicU64::new(0),
            huge_pages_free: AtomicU64::new(0),
            huge_page_size: AtomicU64::new(2 * 1024 * 1024),
            page_faults_minor: AtomicU64::new(0),
            page_faults_major: AtomicU64::new(0),
            pages_swapped_in: AtomicU64::new(0),
            pages_swapped_out: AtomicU64::new(0),
            oom_kills: AtomicU64::new(0),
        }
    }

    /// Get memory usage percentage
    pub fn usage_percent(&self) -> u32 {
        let total = self.total.load(Ordering::Relaxed);
        let free = self.free.load(Ordering::Relaxed);
        if total == 0 {
            return 0;
        }
        ((total - free) * 100 / total) as u32
    }

    /// Get available percentage
    pub fn available_percent(&self) -> u32 {
        let total = self.total.load(Ordering::Relaxed);
        let available = self.available.load(Ordering::Relaxed);
        if total == 0 {
            return 0;
        }
        (available * 100 / total) as u32
    }

    /// Check if under memory pressure
    pub fn is_under_pressure(&self) -> bool {
        self.available_percent() < 10
    }

    /// Check if critically low on memory
    pub fn is_critical(&self) -> bool {
        self.available_percent() < 5
    }
}

// =============================================================================
// PER-PROCESS MEMORY STATS
// =============================================================================

/// Per-process memory statistics
#[derive(Default, Clone)]
pub struct ProcessMemStats {
    /// Virtual memory size
    pub vsize: u64,

    /// Resident set size
    pub rss: u64,

    /// Shared memory
    pub shared: u64,

    /// Text (code) size
    pub text: u64,

    /// Data + stack size
    pub data: u64,

    /// Peak resident set size
    pub rss_peak: u64,

    /// Number of page faults (minor)
    pub minor_faults: u64,

    /// Number of page faults (major)
    pub major_faults: u64,

    /// Swap usage
    pub swap: u64,

    /// Anonymous pages
    pub anon_pages: u64,

    /// File-backed pages
    pub file_pages: u64,

    /// Dirty pages
    pub dirty_pages: u64,

    /// Locked pages
    pub locked_pages: u64,

    /// Pinned pages
    pub pinned_pages: u64,
}

impl ProcessMemStats {
    /// Create new process memory stats
    pub const fn new() -> Self {
        Self {
            vsize: 0,
            rss: 0,
            shared: 0,
            text: 0,
            data: 0,
            rss_peak: 0,
            minor_faults: 0,
            major_faults: 0,
            swap: 0,
            anon_pages: 0,
            file_pages: 0,
            dirty_pages: 0,
            locked_pages: 0,
            pinned_pages: 0,
        }
    }

    /// Update RSS peak if current RSS is higher
    pub fn update_rss_peak(&mut self) {
        if self.rss > self.rss_peak {
            self.rss_peak = self.rss;
        }
    }

    /// Get RSS in pages
    pub fn rss_pages(&self) -> u64 {
        self.rss / PAGE_SIZE as u64
    }

    /// Get proportional set size (estimate)
    pub fn pss(&self) -> u64 {
        // PSS = private + shared / sharing_count
        // Simplified: assume shared is split equally
        self.rss - self.shared / 2
    }

    /// Get unique set size
    pub fn uss(&self) -> u64 {
        self.rss - self.shared
    }
}

// =============================================================================
// MEMORY PRESSURE
// =============================================================================

/// Memory pressure levels
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemoryPressure {
    /// No pressure - plenty of memory available
    None = 0,
    /// Low pressure - some reclamation may be needed
    Low = 1,
    /// Medium pressure - active reclamation recommended
    Medium = 2,
    /// High pressure - aggressive reclamation needed
    High = 3,
    /// Critical - OOM imminent
    Critical = 4,
}

/// Memory pressure monitor
pub struct PressureMonitor {
    /// Current pressure level
    current_level: MemoryPressure,

    /// Pressure history (for averaging)
    history: [MemoryPressure; 60],

    /// History index
    history_idx: usize,

    /// Registered callbacks
    callbacks: Vec<PressureCallback>,

    /// Last notification level
    last_notified: MemoryPressure,
}

/// Pressure notification callback
pub struct PressureCallback {
    /// Callback ID
    pub id: u64,

    /// Minimum level to trigger
    pub min_level: MemoryPressure,

    /// Callback function
    pub callback: fn(MemoryPressure),
}

impl PressureMonitor {
    /// Create a new pressure monitor
    pub const fn new() -> Self {
        Self {
            current_level: MemoryPressure::None,
            history: [MemoryPressure::None; 60],
            history_idx: 0,
            callbacks: Vec::new(),
            last_notified: MemoryPressure::None,
        }
    }

    /// Update pressure level based on memory stats
    pub fn update(&mut self, stats: &MemStats) {
        let available_percent = stats.available_percent();

        self.current_level = match available_percent {
            0..=5 => MemoryPressure::Critical,
            6..=10 => MemoryPressure::High,
            11..=20 => MemoryPressure::Medium,
            21..=30 => MemoryPressure::Low,
            _ => MemoryPressure::None,
        };

        // Record in history
        self.history[self.history_idx] = self.current_level;
        self.history_idx = (self.history_idx + 1) % 60;

        // Notify if level changed
        if self.current_level != self.last_notified {
            self.notify_callbacks();
            self.last_notified = self.current_level;
        }
    }

    /// Get current pressure level
    pub fn current(&self) -> MemoryPressure {
        self.current_level
    }

    /// Get average pressure over last minute
    pub fn average(&self) -> MemoryPressure {
        let sum: u32 = self.history.iter().map(|&p| p as u32).sum();
        let avg = sum / 60;
        match avg {
            0 => MemoryPressure::None,
            1 => MemoryPressure::Low,
            2 => MemoryPressure::Medium,
            3 => MemoryPressure::High,
            _ => MemoryPressure::Critical,
        }
    }

    /// Register a pressure callback
    pub fn register(&mut self, id: u64, min_level: MemoryPressure, callback: fn(MemoryPressure)) {
        self.callbacks.push(PressureCallback {
            id,
            min_level,
            callback,
        });
    }

    /// Unregister a pressure callback
    pub fn unregister(&mut self, id: u64) {
        self.callbacks.retain(|c| c.id != id);
    }

    /// Notify registered callbacks
    fn notify_callbacks(&self) {
        for cb in &self.callbacks {
            if self.current_level >= cb.min_level {
                (cb.callback)(self.current_level);
            }
        }
    }
}

// =============================================================================
// MEMORY RECLAIM
// =============================================================================

/// Memory reclaimer for low-memory situations
pub struct MemoryReclaimer {
    /// Reclaim statistics
    stats: ReclaimStats,

    /// Reclaim targets
    targets: Vec<ReclaimTarget>,
}

/// Reclaim statistics
#[derive(Default)]
pub struct ReclaimStats {
    /// Pages reclaimed
    pub pages_reclaimed: u64,

    /// Pages scanned
    pub pages_scanned: u64,

    /// Reclaim attempts
    pub reclaim_attempts: u64,

    /// Successful reclaims
    pub reclaim_success: u64,

    /// OOM invocations
    pub oom_invocations: u64,
}

/// Reclaim target
pub struct ReclaimTarget {
    /// Target name
    pub name: &'static str,

    /// Priority (higher = reclaim first)
    pub priority: u32,

    /// Reclaim function
    pub reclaim: fn(usize) -> usize,

    /// Shrink function (for slab caches)
    pub shrink: Option<fn() -> usize>,
}

impl MemoryReclaimer {
    /// Create a new memory reclaimer
    pub const fn new() -> Self {
        Self {
            stats: ReclaimStats {
                pages_reclaimed: 0,
                pages_scanned: 0,
                reclaim_attempts: 0,
                reclaim_success: 0,
                oom_invocations: 0,
            },
            targets: Vec::new(),
        }
    }

    /// Register a reclaim target
    pub fn register_target(&mut self, target: ReclaimTarget) {
        self.targets.push(target);
        // Sort by priority
        self.targets.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// Attempt to reclaim memory
    pub fn reclaim(&mut self, pages_needed: usize) -> usize {
        self.stats.reclaim_attempts += 1;

        let mut pages_reclaimed = 0;

        // Try each target in priority order
        for target in &self.targets {
            // Try shrink first (for slab caches)
            if let Some(shrink) = target.shrink {
                let shrunk = shrink();
                pages_reclaimed += shrunk;
            }

            // Then try reclaim
            let reclaimed = (target.reclaim)(pages_needed - pages_reclaimed);
            pages_reclaimed += reclaimed;

            if pages_reclaimed >= pages_needed {
                break;
            }
        }

        self.stats.pages_reclaimed += pages_reclaimed as u64;

        if pages_reclaimed > 0 {
            self.stats.reclaim_success += 1;
        }

        pages_reclaimed
    }

    /// Emergency reclaim (more aggressive)
    pub fn emergency_reclaim(&mut self, pages_needed: usize) -> usize {
        // Try harder - may block, may kill processes
        let reclaimed = self.reclaim(pages_needed);

        if reclaimed < pages_needed {
            // Invoke OOM killer
            self.stats.oom_invocations += 1;
            self.oom_kill();
        }

        reclaimed
    }

    /// Select and kill a process to free memory
    fn oom_kill(&mut self) {
        MEMSTATS.oom_kills.fetch_add(1, Ordering::Relaxed);

        // Select process with highest OOM score
        // Kill it and reclaim its memory
        crate::kprintln!("[OOM] Out of memory! Selecting victim...");

        // Would select and kill a process here
    }

    /// Get reclaim statistics
    pub fn stats(&self) -> &ReclaimStats {
        &self.stats
    }
}

// =============================================================================
// ZONE MANAGEMENT
// =============================================================================

/// Memory zone types
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    /// DMA zone (0-16MB) for legacy devices
    Dma,
    /// DMA32 zone (0-4GB) for 32-bit DMA
    Dma32,
    /// Normal zone (4GB+)
    Normal,
    /// High memory zone (for 32-bit systems)
    HighMem,
    /// Movable zone (for memory hotplug)
    Movable,
}

/// Zone statistics
#[derive(Default)]
pub struct ZoneStats {
    /// Total pages in zone
    pub total_pages: u64,

    /// Free pages in zone
    pub free_pages: u64,

    /// Low watermark
    pub watermark_low: u64,

    /// High watermark
    pub watermark_high: u64,

    /// Minimum watermark
    pub watermark_min: u64,

    /// Pages scanned
    pub pages_scanned: u64,
}

// =============================================================================
// PUBLIC INTERFACE
// =============================================================================

/// Initialize memory statistics
pub fn init(total_memory: u64) {
    MEMSTATS.total.store(total_memory, Ordering::Relaxed);
    MEMSTATS.free.store(total_memory, Ordering::Relaxed);
    MEMSTATS.available.store(total_memory, Ordering::Relaxed);
}

/// Get global memory statistics
pub fn get_stats() -> MemStatsSnapshot {
    MemStatsSnapshot {
        total: MEMSTATS.total.load(Ordering::Relaxed),
        free: MEMSTATS.free.load(Ordering::Relaxed),
        available: MEMSTATS.available.load(Ordering::Relaxed),
        kernel: MEMSTATS.kernel.load(Ordering::Relaxed),
        user: MEMSTATS.user.load(Ordering::Relaxed),
        cached: MEMSTATS.cached.load(Ordering::Relaxed),
        buffers: MEMSTATS.buffers.load(Ordering::Relaxed),
        slab: MEMSTATS.slab.load(Ordering::Relaxed),
        swap_total: MEMSTATS.swap_total.load(Ordering::Relaxed),
        swap_free: MEMSTATS.swap_free.load(Ordering::Relaxed),
        dirty: MEMSTATS.dirty.load(Ordering::Relaxed),
    }
}

/// Memory statistics snapshot
#[derive(Clone)]
pub struct MemStatsSnapshot {
    pub total: u64,
    pub free: u64,
    pub available: u64,
    pub kernel: u64,
    pub user: u64,
    pub cached: u64,
    pub buffers: u64,
    pub slab: u64,
    pub swap_total: u64,
    pub swap_free: u64,
    pub dirty: u64,
}

/// Record a page allocation
pub fn record_alloc(pages: u64, is_kernel: bool) {
    MEMSTATS.free.fetch_sub(pages * PAGE_SIZE as u64, Ordering::Relaxed);
    MEMSTATS.available.fetch_sub(pages * PAGE_SIZE as u64, Ordering::Relaxed);

    if is_kernel {
        MEMSTATS.kernel.fetch_add(pages * PAGE_SIZE as u64, Ordering::Relaxed);
    } else {
        MEMSTATS.user.fetch_add(pages * PAGE_SIZE as u64, Ordering::Relaxed);
    }
}

/// Record a page deallocation
pub fn record_free(pages: u64, is_kernel: bool) {
    MEMSTATS.free.fetch_add(pages * PAGE_SIZE as u64, Ordering::Relaxed);
    MEMSTATS.available.fetch_add(pages * PAGE_SIZE as u64, Ordering::Relaxed);

    if is_kernel {
        MEMSTATS.kernel.fetch_sub(pages * PAGE_SIZE as u64, Ordering::Relaxed);
    } else {
        MEMSTATS.user.fetch_sub(pages * PAGE_SIZE as u64, Ordering::Relaxed);
    }
}

/// Record a page fault
pub fn record_page_fault(major: bool) {
    if major {
        MEMSTATS.page_faults_major.fetch_add(1, Ordering::Relaxed);
    } else {
        MEMSTATS.page_faults_minor.fetch_add(1, Ordering::Relaxed);
    }
}

/// Get memory usage percentage
pub fn usage_percent() -> u32 {
    MEMSTATS.usage_percent()
}

/// Check if under memory pressure
pub fn is_under_pressure() -> bool {
    MEMSTATS.is_under_pressure()
}

/// Check if critically low on memory
pub fn is_critical() -> bool {
    MEMSTATS.is_critical()
}

/// Format memory size for display
pub fn format_size(bytes: u64) -> (u64, &'static str) {
    if bytes >= 1024 * 1024 * 1024 {
        (bytes / (1024 * 1024 * 1024), "GB")
    } else if bytes >= 1024 * 1024 {
        (bytes / (1024 * 1024), "MB")
    } else if bytes >= 1024 {
        (bytes / 1024, "KB")
    } else {
        (bytes, "B")
    }
}

/// Print memory statistics
pub fn print_stats() {
    let stats = get_stats();
    let (total, total_unit) = format_size(stats.total);
    let (free, free_unit) = format_size(stats.free);
    let (available, avail_unit) = format_size(stats.available);
    let (kernel, kernel_unit) = format_size(stats.kernel);
    let (cached, cached_unit) = format_size(stats.cached);

    crate::kprintln!("Memory Statistics:");
    crate::kprintln!("  Total:     {} {}", total, total_unit);
    crate::kprintln!("  Free:      {} {}", free, free_unit);
    crate::kprintln!("  Available: {} {}", available, avail_unit);
    crate::kprintln!("  Kernel:    {} {}", kernel, kernel_unit);
    crate::kprintln!("  Cached:    {} {}", cached, cached_unit);
    crate::kprintln!("  Usage:     {}%", usage_percent());
}
