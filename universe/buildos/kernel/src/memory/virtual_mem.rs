// ===============================================================================
// BUILDOS KERNEL - VIRTUAL MEMORY MANAGER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! Virtual memory management with 4-level paging (x86_64).
//!
//! Implements the x86_64 page table hierarchy:
//! - PML4 (Page Map Level 4) - 512 entries
//! - PDPT (Page Directory Pointer Table) - 512 entries
//! - PD (Page Directory) - 512 entries
//! - PT (Page Table) - 512 entries

use super::{PAGE_SIZE, KERNEL_PHYS_OFFSET, PhysicalMemoryManager};
use super::page::{PageTableEntry, PageFlags};
use crate::boot::{BootInfo, FramebufferInfo};
use core::ptr;

// =============================================================================
// CONSTANTS
// =============================================================================

/// Number of entries per page table
const ENTRIES_PER_TABLE: usize = 512;

/// Page table entry size
const ENTRY_SIZE: usize = 8;

/// PML4 index shift
const PML4_SHIFT: usize = 39;

/// PDPT index shift
const PDPT_SHIFT: usize = 30;

/// PD index shift
const PD_SHIFT: usize = 21;

/// PT index shift
const PT_SHIFT: usize = 12;

/// Index mask (9 bits)
const INDEX_MASK: usize = 0x1FF;

// =============================================================================
// VIRTUAL MEMORY MANAGER
// =============================================================================

/// Virtual memory manager handling page tables
pub struct VirtualMemoryManager {
    /// Physical address of the PML4 table
    pml4_phys: u64,
}

impl VirtualMemoryManager {
    /// Create a new virtual memory manager
    pub fn new(physical: &mut PhysicalMemoryManager) -> Self {
        // Allocate PML4 table
        let pml4_phys = physical.alloc_pages(1)
            .expect("Failed to allocate PML4");

        // Zero out PML4
        unsafe {
            ptr::write_bytes(pml4_phys as *mut u8, 0, PAGE_SIZE);
        }

        Self { pml4_phys }
    }

    /// Map kernel memory (identity map + higher half)
    pub fn map_kernel_memory(&self, boot_info: &BootInfo) {
        // Map all physical memory to higher half
        for region in boot_info.memory_map.iter() {
            let pages = region.page_count as usize;

            for i in 0..pages {
                let phys = region.phys_start + (i * PAGE_SIZE) as u64;
                let virt = phys + KERNEL_PHYS_OFFSET;

                unsafe {
                    self.map_page(virt, phys, PageFlags::PRESENT | PageFlags::WRITABLE);
                }
            }
        }

        // Identity map first 4GB for bootloader compatibility
        for i in 0..(4 * 1024 * 1024 * 1024 / PAGE_SIZE as u64) {
            let addr = i * PAGE_SIZE as u64;
            unsafe {
                self.map_page(addr, addr, PageFlags::PRESENT | PageFlags::WRITABLE);
            }
        }
    }

    /// Map the framebuffer
    pub fn map_framebuffer(&self, fb: &FramebufferInfo) {
        let pages = ((fb.size as usize) + PAGE_SIZE - 1) / PAGE_SIZE;

        for i in 0..pages {
            let phys = fb.address + (i * PAGE_SIZE) as u64;
            let virt = phys + KERNEL_PHYS_OFFSET;

            unsafe {
                self.map_page(
                    virt,
                    phys,
                    PageFlags::PRESENT | PageFlags::WRITABLE | PageFlags::NO_CACHE | PageFlags::WRITE_THROUGH,
                );
            }
        }
    }

    /// Map a virtual address to a physical address
    ///
    /// # Safety
    ///
    /// Caller must ensure the mapping is valid and doesn't conflict
    /// with existing mappings.
    pub unsafe fn map_page(&self, virt: u64, phys: u64, flags: PageFlags) {
        let pml4_idx = ((virt >> PML4_SHIFT) as usize) & INDEX_MASK;
        let pdpt_idx = ((virt >> PDPT_SHIFT) as usize) & INDEX_MASK;
        let pd_idx = ((virt >> PD_SHIFT) as usize) & INDEX_MASK;
        let pt_idx = ((virt >> PT_SHIFT) as usize) & INDEX_MASK;

        // Get or create PDPT
        let pml4 = self.pml4_phys as *mut PageTableEntry;
        let pdpt_phys = self.get_or_create_table(pml4, pml4_idx);

        // Get or create PD
        let pdpt = pdpt_phys as *mut PageTableEntry;
        let pd_phys = self.get_or_create_table(pdpt, pdpt_idx);

        // Get or create PT
        let pd = pd_phys as *mut PageTableEntry;
        let pt_phys = self.get_or_create_table(pd, pd_idx);

        // Set PT entry
        let pt = pt_phys as *mut PageTableEntry;
        let entry = pt.add(pt_idx);
        (*entry).set(phys, flags);

        // Flush TLB for this address
        core::arch::asm!("invlpg [{}]", in(reg) virt, options(nostack, preserves_flags));
    }

    /// Unmap a virtual address
    ///
    /// # Safety
    ///
    /// Caller must ensure the address is currently mapped.
    pub unsafe fn unmap_page(&self, virt: u64) {
        let pml4_idx = ((virt >> PML4_SHIFT) as usize) & INDEX_MASK;
        let pdpt_idx = ((virt >> PDPT_SHIFT) as usize) & INDEX_MASK;
        let pd_idx = ((virt >> PD_SHIFT) as usize) & INDEX_MASK;
        let pt_idx = ((virt >> PT_SHIFT) as usize) & INDEX_MASK;

        let pml4 = self.pml4_phys as *mut PageTableEntry;
        let pml4_entry = pml4.add(pml4_idx);

        if !(*pml4_entry).is_present() {
            return;
        }

        let pdpt = (*pml4_entry).address() as *mut PageTableEntry;
        let pdpt_entry = pdpt.add(pdpt_idx);

        if !(*pdpt_entry).is_present() {
            return;
        }

        let pd = (*pdpt_entry).address() as *mut PageTableEntry;
        let pd_entry = pd.add(pd_idx);

        if !(*pd_entry).is_present() {
            return;
        }

        let pt = (*pd_entry).address() as *mut PageTableEntry;
        let pt_entry = pt.add(pt_idx);

        (*pt_entry).clear();

        // Flush TLB
        core::arch::asm!("invlpg [{}]", in(reg) virt, options(nostack, preserves_flags));
    }

    /// Translate virtual to physical address
    pub fn translate(&self, virt: u64) -> Option<u64> {
        let pml4_idx = ((virt >> PML4_SHIFT) as usize) & INDEX_MASK;
        let pdpt_idx = ((virt >> PDPT_SHIFT) as usize) & INDEX_MASK;
        let pd_idx = ((virt >> PD_SHIFT) as usize) & INDEX_MASK;
        let pt_idx = ((virt >> PT_SHIFT) as usize) & INDEX_MASK;
        let offset = virt & 0xFFF;

        unsafe {
            let pml4 = self.pml4_phys as *const PageTableEntry;
            let pml4_entry = pml4.add(pml4_idx);

            if !(*pml4_entry).is_present() {
                return None;
            }

            let pdpt = (*pml4_entry).address() as *const PageTableEntry;
            let pdpt_entry = pdpt.add(pdpt_idx);

            if !(*pdpt_entry).is_present() {
                return None;
            }

            // Check for 1GB huge page
            if (*pdpt_entry).is_huge() {
                let base = (*pdpt_entry).address();
                return Some(base + (virt & 0x3FFFFFFF)); // 30-bit offset
            }

            let pd = (*pdpt_entry).address() as *const PageTableEntry;
            let pd_entry = pd.add(pd_idx);

            if !(*pd_entry).is_present() {
                return None;
            }

            // Check for 2MB large page
            if (*pd_entry).is_huge() {
                let base = (*pd_entry).address();
                return Some(base + (virt & 0x1FFFFF)); // 21-bit offset
            }

            let pt = (*pd_entry).address() as *const PageTableEntry;
            let pt_entry = pt.add(pt_idx);

            if !(*pt_entry).is_present() {
                return None;
            }

            Some((*pt_entry).address() + offset)
        }
    }

    /// Load this page table into CR3
    pub unsafe fn load(&self) {
        core::arch::asm!(
            "mov cr3, {}",
            in(reg) self.pml4_phys,
            options(nostack, preserves_flags)
        );
    }

    /// Get or create a page table at the given index
    unsafe fn get_or_create_table(&self, table: *mut PageTableEntry, idx: usize) -> u64 {
        let entry = table.add(idx);

        if (*entry).is_present() {
            return (*entry).address();
        }

        // Allocate new table
        // Note: In a real implementation, we'd use the global allocator
        // For now, we use a simple bump allocator from a reserved region
        let new_table = self.alloc_page_table();
        ptr::write_bytes(new_table as *mut u8, 0, PAGE_SIZE);

        (*entry).set(new_table, PageFlags::PRESENT | PageFlags::WRITABLE | PageFlags::USER);

        new_table
    }

    /// Allocate a page table (simplified - uses static allocation)
    fn alloc_page_table(&self) -> u64 {
        // This is a simplified allocator for bootstrap
        // In production, this would use the physical memory manager
        static mut NEXT_PAGE: u64 = 0x200000; // Start at 2MB

        unsafe {
            let page = NEXT_PAGE;
            NEXT_PAGE += PAGE_SIZE as u64;
            page
        }
    }
}

impl Drop for VirtualMemoryManager {
    fn drop(&mut self) {
        // In a real implementation, we'd free all page tables
    }
}
