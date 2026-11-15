// ===============================================================================
// BUILDOS KERNEL - MEMORY MANAGEMENT
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Memory management subsystem.
//!
//! This module implements:
//! - Physical memory manager (buddy allocator)
//! - Virtual memory management (4-level paging)
//! - Kernel heap allocator
//! - User-space memory allocator

mod physical;
mod virtual_mem;
mod heap;
mod page;
pub mod slab;
pub mod numa;
pub mod memstats;
pub mod oom;

pub use physical::PhysicalMemoryManager;
pub use virtual_mem::VirtualMemoryManager;
pub use heap::KernelHeap;
pub use page::{Page, PageTable, PageFlags};
pub use slab::{SlabAllocator, SlabCache, SlabFlags, KmemCache};

use crate::boot::{BootInfo, MemoryRegionType};
use spin::Mutex;

// =============================================================================
// ADDRESS TYPES
// =============================================================================

/// Physical memory address
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct PhysicalAddress(pub u64);

impl PhysicalAddress {
    /// Create a new physical address
    pub const fn new(addr: u64) -> Self {
        Self(addr)
    }

    /// Get the raw address value
    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    /// Check if the address is page-aligned
    pub const fn is_aligned(&self) -> bool {
        self.0 & (PAGE_SIZE as u64 - 1) == 0
    }

    /// Align address down to page boundary
    pub const fn align_down(&self) -> Self {
        Self(self.0 & !(PAGE_SIZE as u64 - 1))
    }

    /// Align address up to page boundary
    pub const fn align_up(&self) -> Self {
        Self((self.0 + PAGE_SIZE as u64 - 1) & !(PAGE_SIZE as u64 - 1))
    }
}

impl From<u64> for PhysicalAddress {
    fn from(addr: u64) -> Self {
        Self(addr)
    }
}

impl From<PhysicalAddress> for u64 {
    fn from(addr: PhysicalAddress) -> Self {
        addr.0
    }
}

/// Virtual memory address
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct VirtualAddress(pub u64);

impl VirtualAddress {
    /// Create a new virtual address
    pub const fn new(addr: u64) -> Self {
        Self(addr)
    }

    /// Get the raw address value
    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    /// Check if the address is page-aligned
    pub const fn is_aligned(&self) -> bool {
        self.0 & (PAGE_SIZE as u64 - 1) == 0
    }

    /// Align address down to page boundary
    pub const fn align_down(&self) -> Self {
        Self(self.0 & !(PAGE_SIZE as u64 - 1))
    }

    /// Align address up to page boundary
    pub const fn align_up(&self) -> Self {
        Self((self.0 + PAGE_SIZE as u64 - 1) & !(PAGE_SIZE as u64 - 1))
    }

    /// Get page table indices for 4-level paging
    pub const fn page_table_indices(&self) -> (usize, usize, usize, usize) {
        let addr = self.0;
        let p4 = ((addr >> 39) & 0x1FF) as usize;
        let p3 = ((addr >> 30) & 0x1FF) as usize;
        let p2 = ((addr >> 21) & 0x1FF) as usize;
        let p1 = ((addr >> 12) & 0x1FF) as usize;
        (p4, p3, p2, p1)
    }

    /// Get page offset
    pub const fn page_offset(&self) -> usize {
        (self.0 & (PAGE_SIZE as u64 - 1)) as usize
    }
}

impl From<u64> for VirtualAddress {
    fn from(addr: u64) -> Self {
        Self(addr)
    }
}

impl From<VirtualAddress> for u64 {
    fn from(addr: VirtualAddress) -> Self {
        addr.0
    }
}

impl From<usize> for VirtualAddress {
    fn from(addr: usize) -> Self {
        Self(addr as u64)
    }
}

impl<T> From<*const T> for VirtualAddress {
    fn from(ptr: *const T) -> Self {
        Self(ptr as u64)
    }
}

impl<T> From<*mut T> for VirtualAddress {
    fn from(ptr: *mut T) -> Self {
        Self(ptr as u64)
    }
}

// =============================================================================
// CONSTANTS
// =============================================================================

/// Page size (4KB)
pub const PAGE_SIZE: usize = 4096;

/// Page shift (log2 of page size)
pub const PAGE_SHIFT: usize = 12;

/// Large page size (2MB)
pub const LARGE_PAGE_SIZE: usize = 2 * 1024 * 1024;

/// Huge page size (1GB)
pub const HUGE_PAGE_SIZE: usize = 1024 * 1024 * 1024;

/// Kernel physical offset (higher half)
pub const KERNEL_PHYS_OFFSET: u64 = 0xFFFF_8000_0000_0000;

/// Kernel heap start address
pub const KERNEL_HEAP_START: u64 = 0xFFFF_8080_0000_0000;

/// Kernel heap size (256MB)
pub const KERNEL_HEAP_SIZE: usize = 256 * 1024 * 1024;

/// User space end address
pub const USER_SPACE_END: u64 = 0x0000_7FFF_FFFF_FFFF;

// =============================================================================
// GLOBAL MEMORY MANAGER
// =============================================================================

/// Global memory manager instance
static MEMORY_MANAGER: Mutex<Option<MemoryManager>> = Mutex::new(None);

/// Memory manager combining physical and virtual memory management
pub struct MemoryManager {
    /// Physical memory manager (buddy allocator)
    pub physical: PhysicalMemoryManager,

    /// Virtual memory manager
    pub virtual_mem: VirtualMemoryManager,

    /// Kernel heap
    pub heap: KernelHeap,
}

impl MemoryManager {
    /// Initialize the memory manager from boot info
    ///
    /// # Safety
    ///
    /// Must only be called once during kernel initialization.
    pub unsafe fn init(boot_info: &BootInfo) -> &'static Mutex<Option<Self>> {
        // Phase 1: Initialize physical memory manager
        let mut physical = PhysicalMemoryManager::new();

        // Add usable memory regions from boot info
        for region in boot_info.memory_map.iter() {
            match region.region_type {
                MemoryRegionType::Usable => {
                    physical.add_region(region.phys_start, region.page_count as usize);
                }
                MemoryRegionType::Bootloader => {
                    // Will be reclaimed after boot
                    physical.add_region(region.phys_start, region.page_count as usize);
                }
                _ => {
                    // Reserved regions - mark as used
                    physical.reserve_region(region.phys_start, region.page_count as usize);
                }
            }
        }

        // Reserve kernel memory
        physical.reserve_region(
            boot_info.kernel_phys_addr,
            (boot_info.kernel_size as usize + PAGE_SIZE - 1) / PAGE_SIZE,
        );

        // Phase 2: Initialize virtual memory manager
        let virtual_mem = VirtualMemoryManager::new(&mut physical);

        // Set up initial kernel mappings
        virtual_mem.map_kernel_memory(boot_info);

        // Map framebuffer
        if boot_info.framebuffer.address != 0 {
            virtual_mem.map_framebuffer(&boot_info.framebuffer);
        }

        // Phase 3: Initialize kernel heap
        let heap = KernelHeap::new(KERNEL_HEAP_START, KERNEL_HEAP_SIZE);

        // Create manager
        let manager = MemoryManager {
            physical,
            virtual_mem,
            heap,
        };

        // Store globally
        *MEMORY_MANAGER.lock() = Some(manager);

        &MEMORY_MANAGER
    }

    /// Get the global memory manager
    pub fn get() -> &'static Mutex<Option<Self>> {
        &MEMORY_MANAGER
    }

    /// Allocate physical pages
    pub fn alloc_pages(&mut self, count: usize) -> Option<u64> {
        self.physical.alloc_pages(count)
    }

    /// Free physical pages
    pub fn free_pages(&mut self, addr: u64, count: usize) {
        self.physical.free_pages(addr, count);
    }

    /// Map a virtual address to a physical address
    pub fn map_page(&mut self, virt: u64, phys: u64, flags: PageFlags) {
        unsafe {
            self.virtual_mem.map_page(virt, phys, flags);
        }
    }

    /// Unmap a virtual address
    pub fn unmap_page(&mut self, virt: u64) {
        unsafe {
            self.virtual_mem.unmap_page(virt);
        }
    }

    /// Allocate kernel heap memory
    pub fn alloc(&mut self, size: usize, align: usize) -> Option<*mut u8> {
        self.heap.alloc(size, align)
    }

    /// Free kernel heap memory
    pub fn dealloc(&mut self, ptr: *mut u8, size: usize, align: usize) {
        self.heap.dealloc(ptr, size, align);
    }
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Convert physical address to virtual address (kernel space)
#[inline]
pub fn phys_to_virt(phys: u64) -> u64 {
    phys + KERNEL_PHYS_OFFSET
}

/// Convert virtual address to physical address (kernel space)
#[inline]
pub fn virt_to_phys(virt: u64) -> u64 {
    virt - KERNEL_PHYS_OFFSET
}

/// Align address up to page boundary
#[inline]
pub fn page_align_up(addr: u64) -> u64 {
    (addr + PAGE_SIZE as u64 - 1) & !(PAGE_SIZE as u64 - 1)
}

/// Align address down to page boundary
#[inline]
pub fn page_align_down(addr: u64) -> u64 {
    addr & !(PAGE_SIZE as u64 - 1)
}

/// Calculate number of pages needed for given size
#[inline]
pub fn pages_needed(size: usize) -> usize {
    (size + PAGE_SIZE - 1) / PAGE_SIZE
}

/// Map MMIO region into virtual address space
///
/// # Arguments
/// * `phys_addr` - Physical address to map
/// * `size` - Size of region in bytes
///
/// # Returns
/// Virtual address of mapped region, or None if mapping failed
pub fn map_mmio(phys_addr: u64, size: usize) -> Option<u64> {
    // For now, use direct physical-to-virtual mapping
    // In a real implementation, this would allocate virtual address space
    // and create page table entries with appropriate caching attributes
    let aligned_phys = page_align_down(phys_addr);
    let offset = phys_addr - aligned_phys;
    let _aligned_size = pages_needed(size + offset as usize) * PAGE_SIZE;

    // Use identity mapping for MMIO in kernel space
    // Real implementation would use ioremap() equivalent
    Some(phys_to_virt(aligned_phys) + offset)
}

/// Unmap MMIO region
pub fn unmap_mmio(_virt_addr: u64, _size: usize) {
    // Placeholder - real implementation would free virtual address space
}

/// Allocate DMA-capable buffer
///
/// # Arguments
/// * `size` - Size of buffer in bytes
///
/// # Returns
/// Physical address of allocated buffer, or None if allocation failed
pub fn alloc_dma_buffer(size: usize) -> Option<u64> {
    // Allocate contiguous physical memory suitable for DMA
    // DMA buffers need to be in low memory (< 4GB for 32-bit DMA)
    // and cache-coherent
    let pages = pages_needed(size);

    // Use frame allocator to get contiguous physical pages
    // For now, allocate from kernel heap and get physical address
    // Real implementation would use a dedicated DMA allocator

    // Simplified: allocate aligned memory
    let layout = core::alloc::Layout::from_size_align(pages * PAGE_SIZE, PAGE_SIZE).ok()?;
    let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };

    if ptr.is_null() {
        None
    } else {
        Some(virt_to_phys(ptr as u64))
    }
}

/// Free DMA buffer
pub fn free_dma_buffer(phys_addr: u64, size: usize) {
    let pages = pages_needed(size);
    let layout = match core::alloc::Layout::from_size_align(pages * PAGE_SIZE, PAGE_SIZE) {
        Ok(l) => l,
        Err(_) => return,
    };
    let virt = phys_to_virt(phys_addr);
    unsafe {
        alloc::alloc::dealloc(virt as *mut u8, layout);
    }
}

// =============================================================================
// VIRTUAL MEMORY ALLOCATION (vmalloc)
// =============================================================================

/// Allocate virtual memory for kernel modules
pub fn vmalloc(size: usize) -> Option<u64> {
    let pages = pages_needed(size);
    let layout = core::alloc::Layout::from_size_align(pages * PAGE_SIZE, PAGE_SIZE).ok()?;

    let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };
    if ptr.is_null() {
        None
    } else {
        Some(ptr as u64)
    }
}

/// Free vmalloc'd memory
pub fn vfree(addr: u64, size: usize) {
    let pages = pages_needed(size);
    let layout = match core::alloc::Layout::from_size_align(pages * PAGE_SIZE, PAGE_SIZE) {
        Ok(l) => l,
        Err(_) => return,
    };
    unsafe {
        alloc::alloc::dealloc(addr as *mut u8, layout);
    }
}

/// Set page flags for a memory region
pub fn set_page_flags(addr: u64, size: usize, flags: page::PageFlags) -> Result<(), &'static str> {
    // In a real implementation, this would modify page table entries
    // For now, this is a stub that accepts the parameters
    let _ = (addr, size, flags);
    Ok(())
}
