// ===============================================================================
// BUILDOS KERNEL - PHYSICAL MEMORY MANAGER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Physical memory manager using a buddy allocator.
//!
//! The buddy allocator manages physical memory in power-of-two sized blocks.
//! This provides O(log n) allocation and deallocation with minimal fragmentation.

use super::PAGE_SIZE;

// =============================================================================
// CONSTANTS
// =============================================================================

/// Maximum order (2^MAX_ORDER pages per block = 4MB blocks)
const MAX_ORDER: usize = 10;

/// Maximum number of memory regions
const MAX_REGIONS: usize = 64;

/// Bitmap size per region (supports up to 1GB per region)
const BITMAP_SIZE: usize = 32768; // 32KB = 256K bits = 256K pages = 1GB

// =============================================================================
// PHYSICAL MEMORY MANAGER
// =============================================================================

/// Physical memory manager using buddy allocation
pub struct PhysicalMemoryManager {
    /// Memory regions
    regions: [Option<MemoryRegion>; MAX_REGIONS],

    /// Number of active regions
    region_count: usize,

    /// Total pages available
    total_pages: usize,

    /// Free pages available
    free_pages: usize,
}

/// A contiguous region of physical memory
struct MemoryRegion {
    /// Base physical address
    base: u64,

    /// Number of pages in this region
    page_count: usize,

    /// Free lists for each order
    free_lists: [FreeList; MAX_ORDER + 1],

    /// Bitmap tracking allocated pages
    bitmap: [u64; BITMAP_SIZE / 8],
}

/// Free list for a particular order
struct FreeList {
    /// Head of the free list
    head: Option<u64>,

    /// Number of free blocks at this order
    count: usize,
}

impl PhysicalMemoryManager {
    /// Create a new physical memory manager
    pub const fn new() -> Self {
        const NONE: Option<MemoryRegion> = None;

        Self {
            regions: [NONE; MAX_REGIONS],
            region_count: 0,
            total_pages: 0,
            free_pages: 0,
        }
    }

    /// Add a memory region to the manager
    pub fn add_region(&mut self, base: u64, page_count: usize) {
        if self.region_count >= MAX_REGIONS {
            return;
        }

        // Align base to page boundary
        let aligned_base = (base + PAGE_SIZE as u64 - 1) & !(PAGE_SIZE as u64 - 1);
        let offset = (aligned_base - base) as usize / PAGE_SIZE;
        let adjusted_count = page_count.saturating_sub(offset);

        if adjusted_count == 0 {
            return;
        }

        let mut region = MemoryRegion {
            base: aligned_base,
            page_count: adjusted_count,
            free_lists: core::array::from_fn(|_| FreeList { head: None, count: 0 }),
            bitmap: [0; BITMAP_SIZE / 8],
        };

        // Initialize free lists - add all pages as free
        let mut addr = aligned_base;
        let mut remaining = adjusted_count;

        // Add blocks from largest to smallest order
        for order in (0..=MAX_ORDER).rev() {
            let block_pages = 1 << order;

            while remaining >= block_pages {
                region.add_to_free_list(order, addr);
                addr += (block_pages * PAGE_SIZE) as u64;
                remaining -= block_pages;
            }
        }

        self.total_pages += adjusted_count;
        self.free_pages += adjusted_count;
        self.regions[self.region_count] = Some(region);
        self.region_count += 1;
    }

    /// Reserve a memory region (mark as used)
    pub fn reserve_region(&mut self, base: u64, page_count: usize) {
        for i in 0..self.region_count {
            if let Some(ref mut region) = self.regions[i] {
                // Check if this reservation overlaps with the region
                let region_end = region.base + (region.page_count * PAGE_SIZE) as u64;

                if base < region_end && base + (page_count * PAGE_SIZE) as u64 > region.base {
                    // Calculate overlap
                    let overlap_start = base.max(region.base);
                    let overlap_end = (base + (page_count * PAGE_SIZE) as u64).min(region_end);
                    let overlap_pages = ((overlap_end - overlap_start) as usize) / PAGE_SIZE;

                    // Mark pages as used in bitmap
                    for page in 0..overlap_pages {
                        let page_addr = overlap_start + (page * PAGE_SIZE) as u64;
                        let page_idx = ((page_addr - region.base) as usize) / PAGE_SIZE;
                        region.set_bit(page_idx);
                    }

                    self.free_pages = self.free_pages.saturating_sub(overlap_pages);
                }
            }
        }
    }

    /// Allocate contiguous physical pages
    pub fn alloc_pages(&mut self, count: usize) -> Option<u64> {
        if count == 0 {
            return None;
        }

        // Calculate minimum order needed
        let order = Self::size_to_order(count);

        if order > MAX_ORDER {
            return None;
        }

        // Try each region
        for i in 0..self.region_count {
            if let Some(ref mut region) = self.regions[i] {
                if let Some(addr) = region.alloc_order(order) {
                    self.free_pages -= 1 << order;
                    return Some(addr);
                }
            }
        }

        None
    }

    /// Free physical pages
    pub fn free_pages(&mut self, addr: u64, count: usize) {
        if count == 0 {
            return;
        }

        let order = Self::size_to_order(count);

        // Find the region containing this address
        for i in 0..self.region_count {
            if let Some(ref mut region) = self.regions[i] {
                if addr >= region.base && addr < region.base + (region.page_count * PAGE_SIZE) as u64 {
                    region.free_order(addr, order);
                    self.free_pages += 1 << order;
                    return;
                }
            }
        }
    }

    /// Convert page count to minimum order
    fn size_to_order(pages: usize) -> usize {
        if pages <= 1 {
            return 0;
        }

        let mut order = 0;
        let mut size = 1;

        while size < pages && order < MAX_ORDER {
            order += 1;
            size <<= 1;
        }

        order
    }

    /// Get total memory in bytes
    pub fn total_memory(&self) -> usize {
        self.total_pages * PAGE_SIZE
    }

    /// Get free memory in bytes
    pub fn free_memory(&self) -> usize {
        self.free_pages * PAGE_SIZE
    }

    /// Get used memory in bytes
    pub fn used_memory(&self) -> usize {
        (self.total_pages - self.free_pages) * PAGE_SIZE
    }
}

impl MemoryRegion {
    /// Add a block to the free list
    fn add_to_free_list(&mut self, order: usize, addr: u64) {
        // Write pointer to next free block at the start of this block
        let ptr = addr as *mut u64;
        unsafe {
            *ptr = match self.free_lists[order].head {
                Some(next) => next,
                None => 0,
            };
        }

        self.free_lists[order].head = Some(addr);
        self.free_lists[order].count += 1;
    }

    /// Remove a block from the free list
    fn remove_from_free_list(&mut self, order: usize) -> Option<u64> {
        let head = self.free_lists[order].head?;

        // Read next pointer
        let next = unsafe { *(head as *const u64) };
        self.free_lists[order].head = if next == 0 { None } else { Some(next) };
        self.free_lists[order].count -= 1;

        Some(head)
    }

    /// Allocate a block of given order
    fn alloc_order(&mut self, order: usize) -> Option<u64> {
        // Try to find a free block at this order
        if self.free_lists[order].count > 0 {
            let addr = self.remove_from_free_list(order)?;
            self.mark_allocated(addr, order);
            return Some(addr);
        }

        // Try to split a larger block
        for larger_order in (order + 1)..=MAX_ORDER {
            if self.free_lists[larger_order].count > 0 {
                let block = self.remove_from_free_list(larger_order)?;

                // Split the block down to the required order
                let mut current_order = larger_order;
                while current_order > order {
                    current_order -= 1;
                    let buddy = block + ((1 << current_order) * PAGE_SIZE) as u64;
                    self.add_to_free_list(current_order, buddy);
                }

                self.mark_allocated(block, order);
                return Some(block);
            }
        }

        None
    }

    /// Free a block of given order
    fn free_order(&mut self, addr: u64, order: usize) {
        self.mark_free(addr, order);

        // Try to merge with buddy
        let mut current_addr = addr;
        let mut current_order = order;

        while current_order < MAX_ORDER {
            let buddy_addr = self.buddy_address(current_addr, current_order);

            // Check if buddy is free and at the same order
            if !self.is_buddy_free(buddy_addr, current_order) {
                break;
            }

            // Remove buddy from its free list
            self.remove_buddy_from_free_list(buddy_addr, current_order);

            // Merge - use lower address as new block
            current_addr = current_addr.min(buddy_addr);
            current_order += 1;
        }

        self.add_to_free_list(current_order, current_addr);
    }

    /// Calculate buddy address for a block
    fn buddy_address(&self, addr: u64, order: usize) -> u64 {
        let block_size = ((1 << order) * PAGE_SIZE) as u64;
        let relative = addr - self.base;
        self.base + (relative ^ block_size)
    }

    /// Check if buddy block is free
    fn is_buddy_free(&self, addr: u64, order: usize) -> bool {
        if addr < self.base || addr >= self.base + (self.page_count * PAGE_SIZE) as u64 {
            return false;
        }

        let page_idx = ((addr - self.base) as usize) / PAGE_SIZE;
        let pages = 1 << order;

        // Check all pages in the block are free
        for i in 0..pages {
            if self.get_bit(page_idx + i) {
                return false;
            }
        }

        true
    }

    /// Remove a specific buddy from its free list
    fn remove_buddy_from_free_list(&mut self, addr: u64, order: usize) {
        let mut prev: Option<u64> = None;
        let mut current = self.free_lists[order].head;

        while let Some(curr_addr) = current {
            if curr_addr == addr {
                let next = unsafe { *(curr_addr as *const u64) };
                let next_opt = if next == 0 { None } else { Some(next) };

                match prev {
                    Some(prev_addr) => unsafe {
                        *(prev_addr as *mut u64) = next;
                    },
                    None => {
                        self.free_lists[order].head = next_opt;
                    }
                }

                self.free_lists[order].count -= 1;
                return;
            }

            prev = current;
            current = {
                let next = unsafe { *(curr_addr as *const u64) };
                if next == 0 { None } else { Some(next) }
            };
        }
    }

    /// Mark a block as allocated in the bitmap
    fn mark_allocated(&mut self, addr: u64, order: usize) {
        let page_idx = ((addr - self.base) as usize) / PAGE_SIZE;
        let pages = 1 << order;

        for i in 0..pages {
            self.set_bit(page_idx + i);
        }
    }

    /// Mark a block as free in the bitmap
    fn mark_free(&mut self, addr: u64, order: usize) {
        let page_idx = ((addr - self.base) as usize) / PAGE_SIZE;
        let pages = 1 << order;

        for i in 0..pages {
            self.clear_bit(page_idx + i);
        }
    }

    /// Set a bit in the bitmap
    fn set_bit(&mut self, idx: usize) {
        if idx < BITMAP_SIZE * 8 {
            self.bitmap[idx / 64] |= 1 << (idx % 64);
        }
    }

    /// Clear a bit in the bitmap
    fn clear_bit(&mut self, idx: usize) {
        if idx < BITMAP_SIZE * 8 {
            self.bitmap[idx / 64] &= !(1 << (idx % 64));
        }
    }

    /// Get a bit from the bitmap
    fn get_bit(&self, idx: usize) -> bool {
        if idx < BITMAP_SIZE * 8 {
            (self.bitmap[idx / 64] & (1 << (idx % 64))) != 0
        } else {
            true // Out of range is considered allocated
        }
    }
}
