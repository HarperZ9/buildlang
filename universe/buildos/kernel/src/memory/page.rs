// ===============================================================================
// BUILDOS KERNEL - PAGE TABLE STRUCTURES
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Page table entry structures and flags for x86_64.

use bitflags::bitflags;

// =============================================================================
// PAGE TABLE STRUCTURES
// =============================================================================

/// A page table containing 512 entries
#[repr(C, align(4096))]
pub struct PageTable {
    entries: [PageTableEntry; 512],
}

impl PageTable {
    /// Create an empty page table
    pub const fn new() -> Self {
        const EMPTY: PageTableEntry = PageTableEntry(0);
        Self {
            entries: [EMPTY; 512],
        }
    }

    /// Get entry at index
    pub fn entry(&self, idx: usize) -> &PageTableEntry {
        &self.entries[idx]
    }

    /// Get mutable entry at index
    pub fn entry_mut(&mut self, idx: usize) -> &mut PageTableEntry {
        &mut self.entries[idx]
    }

    /// Zero out all entries
    pub fn zero(&mut self) {
        for entry in &mut self.entries {
            entry.clear();
        }
    }

    /// Iterate over entries
    pub fn iter(&self) -> impl Iterator<Item = &PageTableEntry> {
        self.entries.iter()
    }

    /// Iterate mutably over entries
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut PageTableEntry> {
        self.entries.iter_mut()
    }
}

// =============================================================================
// PAGE TABLE ENTRY
// =============================================================================

/// A single page table entry
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct PageTableEntry(u64);

impl PageTableEntry {
    /// Create a new empty entry
    pub const fn new() -> Self {
        Self(0)
    }

    /// Create an entry mapping to a physical address with flags
    pub const fn with_addr(addr: u64, flags: PageFlags) -> Self {
        Self((addr & ADDR_MASK) | flags.bits())
    }

    /// Set the entry to map to a physical address with flags
    pub fn set(&mut self, addr: u64, flags: PageFlags) {
        self.0 = (addr & ADDR_MASK) | flags.bits();
    }

    /// Clear the entry
    pub fn clear(&mut self) {
        self.0 = 0;
    }

    /// Get the physical address this entry points to
    pub fn address(&self) -> u64 {
        self.0 & ADDR_MASK
    }

    /// Get the flags
    pub fn flags(&self) -> PageFlags {
        PageFlags::from_bits_truncate(self.0)
    }

    /// Check if entry is present
    pub fn is_present(&self) -> bool {
        self.flags().contains(PageFlags::PRESENT)
    }

    /// Check if this is a huge page entry
    pub fn is_huge(&self) -> bool {
        self.flags().contains(PageFlags::HUGE_PAGE)
    }

    /// Check if entry is writable
    pub fn is_writable(&self) -> bool {
        self.flags().contains(PageFlags::WRITABLE)
    }

    /// Check if entry is user accessible
    pub fn is_user(&self) -> bool {
        self.flags().contains(PageFlags::USER)
    }

    /// Check if entry is executable (NX bit not set)
    pub fn is_executable(&self) -> bool {
        !self.flags().contains(PageFlags::NO_EXECUTE)
    }

    /// Set the accessed bit
    pub fn set_accessed(&mut self) {
        self.0 |= PageFlags::ACCESSED.bits();
    }

    /// Set the dirty bit
    pub fn set_dirty(&mut self) {
        self.0 |= PageFlags::DIRTY.bits();
    }

    /// Get raw value
    pub fn raw(&self) -> u64 {
        self.0
    }
}

/// Address mask (bits 12-51 for physical address)
const ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;

// =============================================================================
// PAGE FLAGS
// =============================================================================

bitflags! {
    /// Page table entry flags
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PageFlags: u64 {
        /// Page is present in memory
        const PRESENT = 1 << 0;

        /// Page is writable
        const WRITABLE = 1 << 1;

        /// Page is accessible from user mode
        const USER = 1 << 2;

        /// Page has write-through caching
        const WRITE_THROUGH = 1 << 3;

        /// Page cache is disabled
        const NO_CACHE = 1 << 4;

        /// Page has been accessed
        const ACCESSED = 1 << 5;

        /// Page has been written to
        const DIRTY = 1 << 6;

        /// This is a huge page (2MB or 1GB)
        const HUGE_PAGE = 1 << 7;

        /// Page is global (not flushed on CR3 switch)
        const GLOBAL = 1 << 8;

        /// Available for OS use (bit 9)
        const OS_BIT_9 = 1 << 9;

        /// Available for OS use (bit 10)
        const OS_BIT_10 = 1 << 10;

        /// Available for OS use (bit 11)
        const OS_BIT_11 = 1 << 11;

        /// Page is not executable (NX bit)
        const NO_EXECUTE = 1 << 63;
    }
}

impl PageFlags {
    /// Kernel code flags (readable, executable)
    pub const KERNEL_CODE: Self = Self::PRESENT;

    /// Kernel data flags (readable, writable, not executable)
    pub const KERNEL_DATA: Self = Self::PRESENT.union(Self::WRITABLE).union(Self::NO_EXECUTE);

    /// Kernel read-only data
    pub const KERNEL_RODATA: Self = Self::PRESENT.union(Self::NO_EXECUTE);

    /// User code flags
    pub const USER_CODE: Self = Self::PRESENT.union(Self::USER);

    /// User data flags
    pub const USER_DATA: Self = Self::PRESENT.union(Self::WRITABLE).union(Self::USER).union(Self::NO_EXECUTE);

    /// User read-only data
    pub const USER_RODATA: Self = Self::PRESENT.union(Self::USER).union(Self::NO_EXECUTE);

    /// MMIO mapping flags
    pub const MMIO: Self = Self::PRESENT.union(Self::WRITABLE).union(Self::NO_CACHE).union(Self::NO_EXECUTE);
}

// =============================================================================
// PAGE WRAPPER
// =============================================================================

/// Wrapper for a physical page frame
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Page {
    /// Physical frame number
    frame: u64,
}

impl Page {
    /// Create a page from a physical address
    pub fn from_address(addr: u64) -> Self {
        Self {
            frame: addr >> 12,
        }
    }

    /// Create a page from a frame number
    pub const fn from_frame(frame: u64) -> Self {
        Self { frame }
    }

    /// Get the physical address of this page
    pub fn address(&self) -> u64 {
        self.frame << 12
    }

    /// Get the frame number
    pub fn frame(&self) -> u64 {
        self.frame
    }

    /// Get a pointer to the page data
    pub fn as_ptr(&self) -> *mut u8 {
        self.address() as *mut u8
    }

    /// Get the page as a mutable slice
    ///
    /// # Safety
    ///
    /// The page must be mapped and writable.
    pub unsafe fn as_slice_mut(&self) -> &mut [u8] {
        core::slice::from_raw_parts_mut(self.as_ptr(), 4096)
    }
}
