// ===============================================================================
// BUILDOS KERNEL - SLAB ALLOCATOR
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! SLAB/SLUB memory allocator for efficient kernel object allocation.
//!
//! This module implements a high-performance SLAB allocator inspired by
//! the Linux kernel's SLUB allocator, optimized for:
//! - Fast allocation and deallocation of fixed-size objects
//! - Cache-friendly memory layout
//! - Per-CPU caching for lock-free fast path
//! - Memory debugging and leak detection

use core::ptr::NonNull;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use alloc::vec::Vec;
use spin::Mutex;

use super::{PAGE_SIZE, phys_to_virt};

// =============================================================================
// CONSTANTS
// =============================================================================

/// Maximum object size for SLAB allocation (larger objects use page allocator)
pub const SLAB_MAX_SIZE: usize = PAGE_SIZE / 2;

/// Minimum object size (for alignment)
pub const SLAB_MIN_SIZE: usize = 16;

/// Number of objects per CPU cache
pub const CPU_CACHE_SIZE: usize = 64;

/// Maximum number of partial slabs to keep
pub const MAX_PARTIAL_SLABS: usize = 16;

/// Number of size classes (16, 32, 64, 128, 256, 512, 1024, 2048)
pub const NUM_SIZE_CLASSES: usize = 8;

/// Red zone size for debugging (poison bytes around objects)
pub const RED_ZONE_SIZE: usize = 16;

/// Poison value for free objects
pub const POISON_FREE: u8 = 0x6B;

/// Poison value for red zone
pub const POISON_REDZONE: u8 = 0xBB;

/// Poison value for uninitialized memory
pub const POISON_UNINIT: u8 = 0x5A;

// =============================================================================
// SLAB ALLOCATOR
// =============================================================================

/// Global SLAB allocator
static SLAB_ALLOCATOR: Mutex<SlabAllocator> = Mutex::new(SlabAllocator::new());

/// SLAB allocator managing multiple object caches
pub struct SlabAllocator {
    /// Size-class caches (for general allocations)
    size_classes: [Option<SlabCache>; NUM_SIZE_CLASSES],

    /// Named object caches (for specific kernel objects)
    named_caches: Vec<SlabCache>,

    /// Allocator statistics
    stats: SlabStats,

    /// Debug mode enabled
    debug_mode: bool,
}

/// Statistics for SLAB allocator
#[derive(Default, Clone, Copy)]
pub struct SlabStats {
    /// Total allocations
    pub allocations: u64,

    /// Total deallocations
    pub deallocations: u64,

    /// Cache hits (from CPU cache)
    pub cache_hits: u64,

    /// Cache misses (from slab)
    pub cache_misses: u64,

    /// New slabs allocated
    pub slabs_allocated: u64,

    /// Slabs freed
    pub slabs_freed: u64,

    /// Current memory usage in bytes
    pub memory_usage: u64,

    /// Peak memory usage
    pub peak_usage: u64,
}

impl SlabAllocator {
    /// Create a new SLAB allocator
    pub const fn new() -> Self {
        Self {
            size_classes: [None, None, None, None, None, None, None, None],
            named_caches: Vec::new(),
            stats: SlabStats {
                allocations: 0,
                deallocations: 0,
                cache_hits: 0,
                cache_misses: 0,
                slabs_allocated: 0,
                slabs_freed: 0,
                memory_usage: 0,
                peak_usage: 0,
            },
            debug_mode: false,
        }
    }

    /// Initialize the SLAB allocator
    pub fn init(&mut self) {
        // Create size-class caches
        let sizes = [16, 32, 64, 128, 256, 512, 1024, 2048];

        for (i, &size) in sizes.iter().enumerate() {
            self.size_classes[i] = Some(SlabCache::new(
                SlabCacheConfig {
                    name: match i {
                        0 => "kmalloc-16",
                        1 => "kmalloc-32",
                        2 => "kmalloc-64",
                        3 => "kmalloc-128",
                        4 => "kmalloc-256",
                        5 => "kmalloc-512",
                        6 => "kmalloc-1024",
                        7 => "kmalloc-2048",
                        _ => "kmalloc-unknown",
                    },
                    object_size: size,
                    alignment: core::cmp::min(size, 64),
                    flags: SlabFlags::empty(),
                },
            ));
        }
    }

    /// Get size class index for a given size
    fn size_class_index(size: usize) -> Option<usize> {
        match size {
            0..=16 => Some(0),
            17..=32 => Some(1),
            33..=64 => Some(2),
            65..=128 => Some(3),
            129..=256 => Some(4),
            257..=512 => Some(5),
            513..=1024 => Some(6),
            1025..=2048 => Some(7),
            _ => None,
        }
    }

    /// Allocate memory from SLAB
    pub fn alloc(&mut self, size: usize, align: usize) -> Option<NonNull<u8>> {
        self.stats.allocations += 1;

        // Find appropriate size class
        let effective_size = core::cmp::max(size, align);

        if let Some(idx) = Self::size_class_index(effective_size) {
            if let Some(cache) = &mut self.size_classes[idx] {
                let ptr = cache.alloc(self.debug_mode);
                if ptr.is_some() {
                    self.stats.memory_usage += cache.config.object_size as u64;
                    if self.stats.memory_usage > self.stats.peak_usage {
                        self.stats.peak_usage = self.stats.memory_usage;
                    }
                }
                return ptr;
            }
        }

        // Size too large for SLAB, use page allocator
        None
    }

    /// Deallocate memory back to SLAB
    pub fn dealloc(&mut self, ptr: NonNull<u8>, size: usize) {
        self.stats.deallocations += 1;

        if let Some(idx) = Self::size_class_index(size) {
            if let Some(cache) = &mut self.size_classes[idx] {
                cache.dealloc(ptr, self.debug_mode);
                self.stats.memory_usage = self.stats.memory_usage
                    .saturating_sub(cache.config.object_size as u64);
            }
        }
    }

    /// Create a named cache for specific object type
    pub fn create_cache(&mut self, config: SlabCacheConfig) -> Option<usize> {
        let cache = SlabCache::new(config);
        let id = self.named_caches.len();
        self.named_caches.push(cache);
        Some(id)
    }

    /// Destroy a named cache
    pub fn destroy_cache(&mut self, id: usize) {
        if id < self.named_caches.len() {
            self.named_caches[id].destroy();
        }
    }

    /// Allocate from named cache
    pub fn alloc_from_cache(&mut self, id: usize) -> Option<NonNull<u8>> {
        if id < self.named_caches.len() {
            self.stats.allocations += 1;
            self.named_caches[id].alloc(self.debug_mode)
        } else {
            None
        }
    }

    /// Deallocate to named cache
    pub fn dealloc_to_cache(&mut self, id: usize, ptr: NonNull<u8>) {
        if id < self.named_caches.len() {
            self.stats.deallocations += 1;
            self.named_caches[id].dealloc(ptr, self.debug_mode);
        }
    }

    /// Get allocator statistics
    pub fn stats(&self) -> SlabStats {
        self.stats
    }

    /// Enable debug mode
    pub fn enable_debug(&mut self) {
        self.debug_mode = true;
    }

    /// Shrink all caches (release empty slabs)
    pub fn shrink_all(&mut self) {
        for cache in self.size_classes.iter_mut().flatten() {
            cache.shrink();
        }
        for cache in &mut self.named_caches {
            cache.shrink();
        }
    }
}

// =============================================================================
// SLAB CACHE
// =============================================================================

/// Configuration for a SLAB cache
#[derive(Clone)]
pub struct SlabCacheConfig {
    /// Cache name (for debugging)
    pub name: &'static str,

    /// Size of each object
    pub object_size: usize,

    /// Alignment requirement
    pub alignment: usize,

    /// Cache flags
    pub flags: SlabFlags,
}

bitflags::bitflags! {
    /// SLAB cache flags
    #[derive(Clone, Copy, Debug, Default)]
    pub struct SlabFlags: u32 {
        /// Zero memory on allocation
        const ZERO = 1 << 0;
        /// Use red zones for debugging
        const RED_ZONE = 1 << 1;
        /// Poison memory on free
        const POISON = 1 << 2;
        /// Track allocation caller
        const TRACK_CALLER = 1 << 3;
        /// Disable CPU caching
        const NO_CPU_CACHE = 1 << 4;
        /// Reclaim aggressively
        const RECLAIM_AGGRESSIVE = 1 << 5;
        /// Objects are DMA-able
        const DMA = 1 << 6;
    }
}

/// SLAB cache for a specific object size
pub struct SlabCache {
    /// Cache configuration
    config: SlabCacheConfig,

    /// Per-CPU object caches (simplified: single CPU for now)
    cpu_cache: CpuCache,

    /// List of partial slabs (have some free objects)
    partial_slabs: Vec<Slab>,

    /// List of full slabs (no free objects)
    full_slabs: Vec<Slab>,

    /// Cache statistics
    stats: CacheStats,
}

/// Per-CPU object cache for lock-free fast path
pub struct CpuCache {
    /// Cached free objects
    objects: [Option<NonNull<u8>>; CPU_CACHE_SIZE],

    /// Number of cached objects
    count: usize,
}

/// Statistics for a single cache
#[derive(Default)]
pub struct CacheStats {
    /// Objects allocated
    pub alloc_count: AtomicU64,

    /// Objects freed
    pub free_count: AtomicU64,

    /// Active objects
    pub active_count: AtomicUsize,

    /// Total slabs
    pub slab_count: AtomicUsize,
}

impl SlabCache {
    /// Create a new SLAB cache
    pub fn new(config: SlabCacheConfig) -> Self {
        Self {
            config,
            cpu_cache: CpuCache::new(),
            partial_slabs: Vec::new(),
            full_slabs: Vec::new(),
            stats: CacheStats::default(),
        }
    }

    /// Calculate objects per slab
    fn objects_per_slab(&self) -> usize {
        let obj_size = self.effective_object_size();
        PAGE_SIZE / obj_size
    }

    /// Get effective object size (including red zone if enabled)
    fn effective_object_size(&self) -> usize {
        let mut size = self.config.object_size;
        if self.config.flags.contains(SlabFlags::RED_ZONE) {
            size += RED_ZONE_SIZE * 2;
        }
        // Align to next power of 2 for cache efficiency
        size.next_power_of_two().max(SLAB_MIN_SIZE)
    }

    /// Allocate an object from the cache
    pub fn alloc(&mut self, debug: bool) -> Option<NonNull<u8>> {
        self.stats.alloc_count.fetch_add(1, Ordering::Relaxed);

        // Fast path: try CPU cache first
        if let Some(ptr) = self.cpu_cache.get() {
            if debug && self.config.flags.contains(SlabFlags::POISON) {
                self.verify_poison(ptr);
            }
            self.stats.active_count.fetch_add(1, Ordering::Relaxed);
            return Some(ptr);
        }

        // Slow path: get from partial slab
        if let Some(ptr) = self.alloc_from_slab(debug) {
            self.stats.active_count.fetch_add(1, Ordering::Relaxed);
            return Some(ptr);
        }

        // Allocate new slab
        if self.grow() {
            if let Some(ptr) = self.alloc_from_slab(debug) {
                self.stats.active_count.fetch_add(1, Ordering::Relaxed);
                return Some(ptr);
            }
        }

        None
    }

    /// Allocate from a slab
    fn alloc_from_slab(&mut self, debug: bool) -> Option<NonNull<u8>> {
        // Find a partial slab with free objects
        for slab in &mut self.partial_slabs {
            if let Some(ptr) = slab.alloc() {
                if debug && self.config.flags.contains(SlabFlags::POISON) {
                    self.verify_poison(ptr);
                }
                return Some(ptr);
            }
        }
        None
    }

    /// Deallocate an object back to the cache
    pub fn dealloc(&mut self, ptr: NonNull<u8>, debug: bool) {
        self.stats.free_count.fetch_add(1, Ordering::Relaxed);
        self.stats.active_count.fetch_sub(1, Ordering::Relaxed);

        if debug && self.config.flags.contains(SlabFlags::POISON) {
            self.poison_object(ptr);
        }

        // Fast path: try to put in CPU cache
        if self.cpu_cache.put(ptr) {
            return;
        }

        // Slow path: return to slab
        self.dealloc_to_slab(ptr);
    }

    /// Return object to its slab
    fn dealloc_to_slab(&mut self, ptr: NonNull<u8>) {
        let addr = ptr.as_ptr() as u64;
        let page_addr = addr & !(PAGE_SIZE as u64 - 1);

        // Find the slab this object belongs to
        for slab in &mut self.partial_slabs {
            if slab.base_addr == page_addr {
                slab.dealloc(ptr);
                return;
            }
        }

        // Check full slabs
        for i in 0..self.full_slabs.len() {
            if self.full_slabs[i].base_addr == page_addr {
                // Move from full to partial
                let mut slab = self.full_slabs.remove(i);
                slab.dealloc(ptr);
                self.partial_slabs.push(slab);
                return;
            }
        }
    }

    /// Grow the cache by allocating a new slab
    fn grow(&mut self) -> bool {
        // Allocate a page for the new slab
        let page = unsafe { alloc_page() };
        if page == 0 {
            return false;
        }

        let slab = Slab::new(page, self.objects_per_slab(), self.effective_object_size());
        self.partial_slabs.push(slab);
        self.stats.slab_count.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// Shrink the cache by releasing empty slabs
    pub fn shrink(&mut self) {
        // Remove empty partial slabs
        self.partial_slabs.retain(|slab| {
            if slab.is_empty() {
                unsafe { free_page(slab.base_addr) };
                false
            } else {
                true
            }
        });
    }

    /// Destroy the cache
    pub fn destroy(&mut self) {
        // Free all slabs
        for slab in &self.partial_slabs {
            unsafe { free_page(slab.base_addr) };
        }
        for slab in &self.full_slabs {
            unsafe { free_page(slab.base_addr) };
        }
        self.partial_slabs.clear();
        self.full_slabs.clear();
    }

    /// Poison an object for debugging
    fn poison_object(&self, ptr: NonNull<u8>) {
        let size = self.config.object_size;
        unsafe {
            core::ptr::write_bytes(ptr.as_ptr(), POISON_FREE, size);
        }
    }

    /// Verify object poison (check for use-after-free)
    fn verify_poison(&self, ptr: NonNull<u8>) {
        let size = self.config.object_size;
        unsafe {
            let slice = core::slice::from_raw_parts(ptr.as_ptr(), size);
            for &byte in slice {
                if byte != POISON_FREE {
                    // Object was modified after free - possible use-after-free
                    panic!("SLAB: use-after-free detected at {:p}", ptr.as_ptr());
                }
            }
        }
    }
}

impl CpuCache {
    /// Create a new CPU cache
    const fn new() -> Self {
        Self {
            objects: [None; CPU_CACHE_SIZE],
            count: 0,
        }
    }

    /// Get an object from the cache
    fn get(&mut self) -> Option<NonNull<u8>> {
        if self.count > 0 {
            self.count -= 1;
            self.objects[self.count].take()
        } else {
            None
        }
    }

    /// Put an object back in the cache
    fn put(&mut self, ptr: NonNull<u8>) -> bool {
        if self.count < CPU_CACHE_SIZE {
            self.objects[self.count] = Some(ptr);
            self.count += 1;
            true
        } else {
            false
        }
    }
}

// =============================================================================
// SLAB
// =============================================================================

/// A single slab (one page of objects)
pub struct Slab {
    /// Base physical address of the slab
    base_addr: u64,

    /// Free list head
    free_head: Option<NonNull<FreeObject>>,

    /// Number of free objects
    free_count: usize,

    /// Total number of objects
    total_count: usize,

    /// Object size
    object_size: usize,
}

/// Free object in the free list
#[repr(C)]
struct FreeObject {
    next: Option<NonNull<FreeObject>>,
}

impl Slab {
    /// Create a new slab
    fn new(base_addr: u64, object_count: usize, object_size: usize) -> Self {
        let virt_addr = phys_to_virt(base_addr);

        // Initialize free list
        let mut free_head: Option<NonNull<FreeObject>> = None;

        for i in (0..object_count).rev() {
            let obj_addr = virt_addr + (i * object_size) as u64;
            let obj_ptr = obj_addr as *mut FreeObject;

            unsafe {
                (*obj_ptr).next = free_head;
                free_head = NonNull::new(obj_ptr);
            }
        }

        Self {
            base_addr,
            free_head,
            free_count: object_count,
            total_count: object_count,
            object_size,
        }
    }

    /// Allocate an object from this slab
    fn alloc(&mut self) -> Option<NonNull<u8>> {
        if let Some(obj) = self.free_head {
            unsafe {
                self.free_head = (*obj.as_ptr()).next;
            }
            self.free_count -= 1;
            Some(obj.cast())
        } else {
            None
        }
    }

    /// Deallocate an object back to this slab
    fn dealloc(&mut self, ptr: NonNull<u8>) {
        let obj = ptr.cast::<FreeObject>();
        unsafe {
            (*obj.as_ptr()).next = self.free_head;
        }
        self.free_head = Some(obj);
        self.free_count += 1;
    }

    /// Check if the slab is empty (all objects free)
    fn is_empty(&self) -> bool {
        self.free_count == self.total_count
    }

    /// Check if the slab is full (no free objects)
    fn is_full(&self) -> bool {
        self.free_count == 0
    }
}

// =============================================================================
// PAGE ALLOCATION HELPERS
// =============================================================================

/// Allocate a single page (placeholder - uses global allocator)
unsafe fn alloc_page() -> u64 {
    // This would call the physical memory manager
    // For now, return 0 to indicate failure
    0
}

/// Free a single page
unsafe fn free_page(_addr: u64) {
    // This would call the physical memory manager
}

// =============================================================================
// PUBLIC INTERFACE
// =============================================================================

/// Initialize the SLAB allocator
pub fn init() {
    let mut allocator = SLAB_ALLOCATOR.lock();
    allocator.init();
}

/// Allocate memory from SLAB
pub fn slab_alloc(size: usize, align: usize) -> Option<NonNull<u8>> {
    let mut allocator = SLAB_ALLOCATOR.lock();
    allocator.alloc(size, align)
}

/// Deallocate memory to SLAB
pub fn slab_dealloc(ptr: NonNull<u8>, size: usize) {
    let mut allocator = SLAB_ALLOCATOR.lock();
    allocator.dealloc(ptr, size);
}

/// Create a named object cache
pub fn create_cache(config: SlabCacheConfig) -> Option<usize> {
    let mut allocator = SLAB_ALLOCATOR.lock();
    allocator.create_cache(config)
}

/// Destroy a named object cache
pub fn destroy_cache(id: usize) {
    let mut allocator = SLAB_ALLOCATOR.lock();
    allocator.destroy_cache(id);
}

/// Allocate from a named cache
pub fn cache_alloc(id: usize) -> Option<NonNull<u8>> {
    let mut allocator = SLAB_ALLOCATOR.lock();
    allocator.alloc_from_cache(id)
}

/// Deallocate to a named cache
pub fn cache_free(id: usize, ptr: NonNull<u8>) {
    let mut allocator = SLAB_ALLOCATOR.lock();
    allocator.dealloc_to_cache(id, ptr);
}

/// Get SLAB allocator statistics
pub fn get_stats() -> SlabStats {
    let allocator = SLAB_ALLOCATOR.lock();
    allocator.stats()
}

/// Shrink all caches
pub fn shrink_caches() {
    let mut allocator = SLAB_ALLOCATOR.lock();
    allocator.shrink_all();
}

/// Enable debug mode
pub fn enable_debug() {
    let mut allocator = SLAB_ALLOCATOR.lock();
    allocator.enable_debug();
}

// =============================================================================
// KMEM CACHE INTERFACE (Linux-compatible)
// =============================================================================

/// Kernel memory cache (kmem_cache) - wrapper for SlabCache
pub struct KmemCache {
    id: usize,
    config: SlabCacheConfig,
}

impl KmemCache {
    /// Create a new kernel memory cache
    pub fn new(name: &'static str, size: usize, align: usize, flags: SlabFlags) -> Option<Self> {
        let config = SlabCacheConfig {
            name,
            object_size: size,
            alignment: align,
            flags,
        };

        let id = create_cache(config.clone())?;

        Some(Self { id, config })
    }

    /// Allocate an object from the cache
    pub fn alloc(&self) -> Option<NonNull<u8>> {
        cache_alloc(self.id)
    }

    /// Free an object back to the cache
    pub fn free(&self, ptr: NonNull<u8>) {
        cache_free(self.id, ptr);
    }

    /// Get the object size
    pub fn object_size(&self) -> usize {
        self.config.object_size
    }

    /// Get the cache name
    pub fn name(&self) -> &'static str {
        self.config.name
    }
}

impl Drop for KmemCache {
    fn drop(&mut self) {
        destroy_cache(self.id);
    }
}

// =============================================================================
// MEMORY DEBUGGING
// =============================================================================

/// Memory leak tracker
pub struct LeakTracker {
    /// Active allocations with caller info
    allocations: Vec<AllocationInfo>,

    /// Total leaked bytes
    leaked_bytes: usize,
}

/// Information about an allocation
#[derive(Clone)]
pub struct AllocationInfo {
    /// Pointer to allocated memory
    pub ptr: u64,

    /// Size of allocation
    pub size: usize,

    /// Timestamp of allocation
    pub timestamp: u64,

    /// Caller address (return address)
    pub caller: u64,
}

impl LeakTracker {
    /// Create a new leak tracker
    pub const fn new() -> Self {
        Self {
            allocations: Vec::new(),
            leaked_bytes: 0,
        }
    }

    /// Record an allocation
    pub fn record_alloc(&mut self, ptr: u64, size: usize, caller: u64) {
        self.allocations.push(AllocationInfo {
            ptr,
            size,
            timestamp: 0, // Would use kernel timestamp
            caller,
        });
        self.leaked_bytes += size;
    }

    /// Record a deallocation
    pub fn record_free(&mut self, ptr: u64) {
        if let Some(idx) = self.allocations.iter().position(|a| a.ptr == ptr) {
            let info = self.allocations.remove(idx);
            self.leaked_bytes = self.leaked_bytes.saturating_sub(info.size);
        }
    }

    /// Get list of leaked allocations
    pub fn get_leaks(&self) -> &[AllocationInfo] {
        &self.allocations
    }

    /// Get total leaked bytes
    pub fn leaked_bytes(&self) -> usize {
        self.leaked_bytes
    }

    /// Print leak report
    pub fn print_report(&self) {
        if self.allocations.is_empty() {
            crate::kprintln!("[SLAB] No memory leaks detected");
        } else {
            crate::kprintln!("[SLAB] Memory leak report: {} leaks, {} bytes",
                self.allocations.len(), self.leaked_bytes);
            for (i, alloc) in self.allocations.iter().take(10).enumerate() {
                crate::kprintln!("  #{}: {:016X} ({} bytes) from {:016X}",
                    i + 1, alloc.ptr, alloc.size, alloc.caller);
            }
            if self.allocations.len() > 10 {
                crate::kprintln!("  ... and {} more", self.allocations.len() - 10);
            }
        }
    }
}

// =============================================================================
// THREAD SAFETY
// =============================================================================

// Safety: SlabAllocator is protected by a Mutex and access is properly synchronized.
// The raw pointers in CpuCache and Slab are only accessed while holding the mutex lock.
unsafe impl Send for SlabAllocator {}
unsafe impl Sync for SlabAllocator {}
unsafe impl Send for SlabCache {}
unsafe impl Sync for SlabCache {}
unsafe impl Send for CpuCache {}
unsafe impl Sync for CpuCache {}
unsafe impl Send for Slab {}
unsafe impl Sync for Slab {}
unsafe impl Send for FreeObject {}
unsafe impl Sync for FreeObject {}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_size_class_index() {
        assert_eq!(SlabAllocator::size_class_index(1), Some(0));
        assert_eq!(SlabAllocator::size_class_index(16), Some(0));
        assert_eq!(SlabAllocator::size_class_index(17), Some(1));
        assert_eq!(SlabAllocator::size_class_index(32), Some(1));
        assert_eq!(SlabAllocator::size_class_index(2048), Some(7));
        assert_eq!(SlabAllocator::size_class_index(2049), None);
    }

    #[test]
    fn test_cpu_cache() {
        let mut cache = CpuCache::new();
        assert!(cache.get().is_none());

        // Would need NonNull for full test
    }

    #[test]
    fn test_slab_flags() {
        let flags = SlabFlags::ZERO | SlabFlags::POISON;
        assert!(flags.contains(SlabFlags::ZERO));
        assert!(flags.contains(SlabFlags::POISON));
        assert!(!flags.contains(SlabFlags::RED_ZONE));
    }
}
