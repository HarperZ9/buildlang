// ===============================================================================
// BUILDOS KERNEL - KERNEL HEAP ALLOCATOR
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! Kernel heap allocator using a linked list free list.

use core::alloc::{GlobalAlloc, Layout};
use core::ptr;
use spin::Mutex;

// =============================================================================
// CONSTANTS
// =============================================================================

/// Minimum allocation size (for alignment and free list pointers)
const MIN_ALLOC_SIZE: usize = 16;

/// Alignment requirement
const HEAP_ALIGN: usize = 16;

// =============================================================================
// KERNEL HEAP
// =============================================================================

/// Kernel heap allocator
pub struct KernelHeap {
    /// Start address of the heap
    start: u64,

    /// Size of the heap in bytes
    size: usize,

    /// Current allocation position (for bump allocation)
    position: u64,

    /// Free list head
    free_list: Option<*mut FreeBlock>,

    /// Total bytes allocated
    allocated: usize,
}

// SAFETY: KernelHeap is only accessed through a Mutex, and raw pointers
// are only dereferenced within the allocator's synchronized code.
unsafe impl Send for KernelHeap {}
unsafe impl Sync for KernelHeap {}

/// A free block in the free list
#[repr(C)]
struct FreeBlock {
    /// Size of this free block
    size: usize,

    /// Next free block
    next: Option<*mut FreeBlock>,
}

impl KernelHeap {
    /// Create a new kernel heap
    pub fn new(start: u64, size: usize) -> Self {
        Self {
            start,
            size,
            position: start,
            free_list: None,
            allocated: 0,
        }
    }

    /// Allocate memory from the heap
    pub fn alloc(&mut self, size: usize, align: usize) -> Option<*mut u8> {
        let size = size.max(MIN_ALLOC_SIZE);
        let align = align.max(HEAP_ALIGN);

        // First, try to find a free block
        if let Some(ptr) = self.alloc_from_free_list(size, align) {
            return Some(ptr);
        }

        // Fall back to bump allocation
        self.bump_alloc(size, align)
    }

    /// Deallocate memory back to the heap
    pub fn dealloc(&mut self, ptr: *mut u8, size: usize, _align: usize) {
        let size = size.max(MIN_ALLOC_SIZE);

        // Add to free list
        let block = ptr as *mut FreeBlock;
        unsafe {
            (*block).size = size;
            (*block).next = self.free_list;
        }
        self.free_list = Some(block);
        self.allocated -= size;
    }

    /// Try to allocate from the free list
    fn alloc_from_free_list(&mut self, size: usize, align: usize) -> Option<*mut u8> {
        let mut prev: Option<*mut FreeBlock> = None;
        let mut current = self.free_list;

        while let Some(block_ptr) = current {
            let block = unsafe { &mut *block_ptr };

            // Check if this block is large enough
            let block_addr = block_ptr as usize;
            let aligned_addr = (block_addr + align - 1) & !(align - 1);
            let padding = aligned_addr - block_addr;
            let required_size = size + padding;

            if block.size >= required_size {
                // Remove from free list
                match prev {
                    Some(prev_ptr) => unsafe {
                        (*prev_ptr).next = block.next;
                    },
                    None => {
                        self.free_list = block.next;
                    }
                }

                // If there's leftover space, create a new free block
                if block.size > required_size + MIN_ALLOC_SIZE {
                    let new_block_addr = aligned_addr + size;
                    let new_block = new_block_addr as *mut FreeBlock;
                    unsafe {
                        (*new_block).size = block.size - required_size;
                        (*new_block).next = self.free_list;
                    }
                    self.free_list = Some(new_block);
                }

                self.allocated += size;
                return Some(aligned_addr as *mut u8);
            }

            prev = current;
            current = block.next;
        }

        None
    }

    /// Bump allocate from the end of the heap
    fn bump_alloc(&mut self, size: usize, align: usize) -> Option<*mut u8> {
        let aligned = (self.position as usize + align - 1) & !(align - 1);
        let new_position = aligned + size;

        if new_position > (self.start as usize + self.size) {
            return None;
        }

        self.position = new_position as u64;
        self.allocated += size;

        Some(aligned as *mut u8)
    }

    /// Get total heap size
    pub fn size(&self) -> usize {
        self.size
    }

    /// Get allocated bytes
    pub fn allocated(&self) -> usize {
        self.allocated
    }

    /// Get free bytes (approximate)
    pub fn free(&self) -> usize {
        let remaining = (self.start as usize + self.size) - self.position as usize;
        remaining + self.free_list_size()
    }

    /// Calculate size of free list
    fn free_list_size(&self) -> usize {
        let mut total = 0;
        let mut current = self.free_list;

        while let Some(block_ptr) = current {
            let block = unsafe { &*block_ptr };
            total += block.size;
            current = block.next;
        }

        total
    }
}

// =============================================================================
// GLOBAL ALLOCATOR
// =============================================================================

/// Global kernel allocator
#[global_allocator]
static ALLOCATOR: GlobalKernelAllocator = GlobalKernelAllocator::new();

/// Thread-safe wrapper around KernelHeap
pub struct GlobalKernelAllocator {
    heap: Mutex<Option<KernelHeap>>,
}

impl GlobalKernelAllocator {
    /// Create a new global allocator
    const fn new() -> Self {
        Self {
            heap: Mutex::new(None),
        }
    }

    /// Initialize the allocator with the heap
    pub fn init(&self, start: u64, size: usize) {
        *self.heap.lock() = Some(KernelHeap::new(start, size));
    }
}

unsafe impl GlobalAlloc for GlobalKernelAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut heap = self.heap.lock();

        match heap.as_mut() {
            Some(h) => h.alloc(layout.size(), layout.align()).unwrap_or(ptr::null_mut()),
            None => ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let mut heap = self.heap.lock();

        if let Some(h) = heap.as_mut() {
            h.dealloc(ptr, layout.size(), layout.align());
        }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // Simple realloc: allocate new, copy, free old
        let new_layout = Layout::from_size_align_unchecked(new_size, layout.align());
        let new_ptr = self.alloc(new_layout);

        if !new_ptr.is_null() {
            ptr::copy_nonoverlapping(ptr, new_ptr, layout.size().min(new_size));
            self.dealloc(ptr, layout);
        }

        new_ptr
    }
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Initialize the global allocator
pub fn init_global_allocator(start: u64, size: usize) {
    ALLOCATOR.init(start, size);
}
