// ===============================================================================
// BUILDOS KERNEL - VIRTUALIZATION MEMORY MANAGEMENT
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Memory Virtualization
//!
//! Handles guest memory management:
//! - EPT/NPT page table management
//! - Memory slot management
//! - Dirty page tracking
//! - Memory ballooning

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::sync::{Mutex, RwLock};
use super::KvmError;

// =============================================================================
// MEMORY SLOT
// =============================================================================

/// Memory slot
#[derive(Debug, Clone)]
pub struct MemorySlot {
    /// Slot ID
    id: u32,
    /// Guest physical address start
    base_gfn: u64,
    /// Number of pages
    npages: u64,
    /// Host virtual address
    userspace_addr: u64,
    /// Flags
    flags: u32,
    /// Dirty bitmap
    dirty_bitmap: Option<Vec<u64>>,
    /// Write tracking enabled
    write_tracking: bool,
}

impl MemorySlot {
    /// Create new memory slot
    pub fn new(
        id: u32,
        base_gfn: u64,
        npages: u64,
        userspace_addr: u64,
        flags: u32,
    ) -> Self {
        Self {
            id,
            base_gfn,
            npages,
            userspace_addr,
            flags,
            dirty_bitmap: None,
            write_tracking: false,
        }
    }

    /// Check if GFN is in this slot
    pub fn contains_gfn(&self, gfn: u64) -> bool {
        gfn >= self.base_gfn && gfn < self.base_gfn + self.npages
    }

    /// Get host virtual address for GFN
    pub fn gfn_to_hva(&self, gfn: u64) -> Option<u64> {
        if self.contains_gfn(gfn) {
            let offset = (gfn - self.base_gfn) * 4096;
            Some(self.userspace_addr + offset)
        } else {
            None
        }
    }

    /// Enable dirty tracking
    pub fn enable_dirty_tracking(&mut self) {
        if self.dirty_bitmap.is_none() {
            let bitmap_size = ((self.npages + 63) / 64) as usize;
            self.dirty_bitmap = Some(alloc::vec![0u64; bitmap_size]);
        }
        self.write_tracking = true;
    }

    /// Disable dirty tracking
    pub fn disable_dirty_tracking(&mut self) {
        self.write_tracking = false;
        self.dirty_bitmap = None;
    }

    /// Mark page as dirty
    pub fn mark_dirty(&mut self, gfn: u64) {
        // Check gfn range before borrowing bitmap to avoid borrow conflict
        let in_range = gfn >= self.base_gfn && gfn < self.base_gfn + self.npages;
        if !in_range {
            return;
        }

        if let Some(ref mut bitmap) = self.dirty_bitmap {
            let offset = gfn - self.base_gfn;
            let index = (offset / 64) as usize;
            let bit = offset % 64;
            if index < bitmap.len() {
                bitmap[index] |= 1 << bit;
            }
        }
    }

    /// Get and clear dirty bitmap
    pub fn get_and_clear_dirty(&mut self) -> Option<Vec<u64>> {
        if let Some(ref mut bitmap) = self.dirty_bitmap {
            let result = bitmap.clone();
            for word in bitmap.iter_mut() {
                *word = 0;
            }
            Some(result)
        } else {
            None
        }
    }

    /// Check if page is dirty
    pub fn is_dirty(&self, gfn: u64) -> bool {
        if let Some(ref bitmap) = self.dirty_bitmap {
            if self.contains_gfn(gfn) {
                let offset = gfn - self.base_gfn;
                let index = (offset / 64) as usize;
                let bit = offset % 64;
                if index < bitmap.len() {
                    return (bitmap[index] & (1 << bit)) != 0;
                }
            }
        }
        false
    }
}

// =============================================================================
// MEMORY REGION MANAGER
// =============================================================================

/// Memory region manager for a VM
pub struct MemoryManager {
    /// Memory slots indexed by ID
    slots: RwLock<BTreeMap<u32, MemorySlot>>,
    /// Maximum number of slots
    max_slots: u32,
    /// Total mapped memory
    total_memory: AtomicU64,
    /// EPT/NPT root
    page_table_root: AtomicU64,
}

impl MemoryManager {
    /// Create new memory manager
    pub fn new(max_slots: u32) -> Self {
        Self {
            slots: RwLock::new(BTreeMap::new()),
            max_slots,
            total_memory: AtomicU64::new(0),
            page_table_root: AtomicU64::new(0),
        }
    }

    /// Set memory region
    pub fn set_memory_region(
        &self,
        slot_id: u32,
        guest_phys_addr: u64,
        memory_size: u64,
        userspace_addr: u64,
        flags: u32,
    ) -> Result<(), KvmError> {
        if slot_id >= self.max_slots {
            return Err(KvmError::InvalidArgument);
        }

        let mut slots = self.slots.write();

        if memory_size == 0 {
            // Delete slot
            if let Some(old_slot) = slots.remove(&slot_id) {
                self.total_memory.fetch_sub(old_slot.npages * 4096, Ordering::SeqCst);
                // Would unmap from EPT/NPT
            }
        } else {
            // Check alignment
            if guest_phys_addr & 0xFFF != 0 || memory_size & 0xFFF != 0 {
                return Err(KvmError::InvalidArgument);
            }

            let npages = memory_size / 4096;
            let base_gfn = guest_phys_addr / 4096;

            // Check for overlaps
            for slot in slots.values() {
                if slot.id != slot_id {
                    let slot_end = slot.base_gfn + slot.npages;
                    let new_end = base_gfn + npages;

                    if !(new_end <= slot.base_gfn || base_gfn >= slot_end) {
                        return Err(KvmError::AlreadyExists);
                    }
                }
            }

            // Update total memory
            if let Some(old_slot) = slots.get(&slot_id) {
                self.total_memory.fetch_sub(old_slot.npages * 4096, Ordering::SeqCst);
            }

            let slot = MemorySlot::new(slot_id, base_gfn, npages, userspace_addr, flags);
            slots.insert(slot_id, slot);
            self.total_memory.fetch_add(memory_size, Ordering::SeqCst);

            // Would map in EPT/NPT
        }

        Ok(())
    }

    /// Get memory slot by ID
    pub fn get_slot(&self, slot_id: u32) -> Option<MemorySlot> {
        self.slots.read().get(&slot_id).cloned()
    }

    /// Translate GFN to HVA
    pub fn gfn_to_hva(&self, gfn: u64) -> Option<u64> {
        let slots = self.slots.read();
        for slot in slots.values() {
            if let Some(hva) = slot.gfn_to_hva(gfn) {
                return Some(hva);
            }
        }
        None
    }

    /// Translate GPA to HVA
    pub fn gpa_to_hva(&self, gpa: u64) -> Option<u64> {
        let gfn = gpa / 4096;
        let offset = gpa & 0xFFF;
        self.gfn_to_hva(gfn).map(|hva| hva + offset)
    }

    /// Get total mapped memory
    pub fn total_memory(&self) -> u64 {
        self.total_memory.load(Ordering::SeqCst)
    }

    /// Get slot count
    pub fn slot_count(&self) -> usize {
        self.slots.read().len()
    }

    /// Enable dirty tracking for slot
    pub fn enable_dirty_tracking(&self, slot_id: u32) -> Result<(), KvmError> {
        let mut slots = self.slots.write();
        if let Some(slot) = slots.get_mut(&slot_id) {
            slot.enable_dirty_tracking();
            Ok(())
        } else {
            Err(KvmError::NotFound)
        }
    }

    /// Get dirty log for slot
    pub fn get_dirty_log(&self, slot_id: u32) -> Result<Vec<u64>, KvmError> {
        let mut slots = self.slots.write();
        if let Some(slot) = slots.get_mut(&slot_id) {
            slot.get_and_clear_dirty().ok_or(KvmError::InvalidState)
        } else {
            Err(KvmError::NotFound)
        }
    }

    /// Set page table root
    pub fn set_page_table_root(&self, root: u64) {
        self.page_table_root.store(root, Ordering::SeqCst);
    }

    /// Get page table root
    pub fn page_table_root(&self) -> u64 {
        self.page_table_root.load(Ordering::SeqCst)
    }
}

// =============================================================================
// EPT/NPT PAGE TABLE
// =============================================================================

/// Page table level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageLevel {
    /// PML4 (512 GB pages)
    Pml4 = 4,
    /// PDPT (1 GB pages)
    Pdpt = 3,
    /// PD (2 MB pages)
    Pd = 2,
    /// PT (4 KB pages)
    Pt = 1,
}

/// Page table entry
#[derive(Debug, Clone, Copy)]
pub struct PageTableEntry {
    /// Raw entry value
    value: u64,
}

impl PageTableEntry {
    /// Create empty entry
    pub const fn empty() -> Self {
        Self { value: 0 }
    }

    /// Create entry with flags
    pub fn new(addr: u64, flags: u64) -> Self {
        Self {
            value: (addr & !0xFFF) | flags,
        }
    }

    /// Check if present
    pub fn is_present(&self) -> bool {
        self.value & 0x7 != 0  // Read, Write, or Execute
    }

    /// Check if large page
    pub fn is_large(&self) -> bool {
        self.value & (1 << 7) != 0
    }

    /// Get physical address
    pub fn addr(&self) -> u64 {
        self.value & 0x000F_FFFF_FFFF_F000
    }

    /// Get flags
    pub fn flags(&self) -> u64 {
        self.value & 0xFFF
    }

    /// Set accessed
    pub fn set_accessed(&mut self) {
        self.value |= 1 << 8;
    }

    /// Set dirty
    pub fn set_dirty(&mut self) {
        self.value |= 1 << 9;
    }
}

/// Extended page table for VM
pub struct ExtendedPageTable {
    /// Root page table physical address
    root: u64,
    /// Allocated page table pages
    pages: Mutex<Vec<u64>>,
    /// Memory manager reference
    memory: u64, // Would be Arc<MemoryManager>
}

impl ExtendedPageTable {
    /// Create new extended page table
    pub fn new() -> Result<Self, KvmError> {
        let root = allocate_page()?;

        // Zero the root page
        unsafe {
            core::ptr::write_bytes(root as *mut u8, 0, 4096);
        }

        Ok(Self {
            root,
            pages: Mutex::new(alloc::vec![root]),
            memory: 0,
        })
    }

    /// Get root physical address
    pub fn root(&self) -> u64 {
        self.root
    }

    /// Map guest frame to host frame
    pub fn map_page(
        &mut self,
        gfn: u64,
        hfn: u64,
        level: PageLevel,
        flags: u64,
    ) -> Result<(), KvmError> {
        let gpa = gfn * 4096;

        // Get page table indices
        let pml4_idx = (gpa >> 39) & 0x1FF;
        let pdpt_idx = (gpa >> 30) & 0x1FF;
        let pd_idx = (gpa >> 21) & 0x1FF;
        let pt_idx = (gpa >> 12) & 0x1FF;

        // Walk/create page tables
        let pml4 = self.root as *mut PageTableEntry;
        let pdpt = self.ensure_entry(pml4, pml4_idx as usize)?;

        if level == PageLevel::Pdpt {
            // 1GB page
            unsafe {
                let entry = pdpt.add(pdpt_idx as usize);
                *entry = PageTableEntry::new(hfn * 4096, flags | (1 << 7));
            }
            return Ok(());
        }

        let pd = self.ensure_entry(pdpt, pdpt_idx as usize)?;

        if level == PageLevel::Pd {
            // 2MB page
            unsafe {
                let entry = pd.add(pd_idx as usize);
                *entry = PageTableEntry::new(hfn * 4096, flags | (1 << 7));
            }
            return Ok(());
        }

        let pt = self.ensure_entry(pd, pd_idx as usize)?;

        // 4KB page
        unsafe {
            let entry = pt.add(pt_idx as usize);
            *entry = PageTableEntry::new(hfn * 4096, flags);
        }

        Ok(())
    }

    /// Ensure page table entry exists, creating if needed
    fn ensure_entry(
        &mut self,
        table: *mut PageTableEntry,
        index: usize,
    ) -> Result<*mut PageTableEntry, KvmError> {
        unsafe {
            let entry = table.add(index);
            if !(*entry).is_present() {
                let new_page = allocate_page()?;
                core::ptr::write_bytes(new_page as *mut u8, 0, 4096);

                *entry = PageTableEntry::new(new_page, 0x7); // RWX

                self.pages.lock().push(new_page);
            }

            Ok((*entry).addr() as *mut PageTableEntry)
        }
    }

    /// Unmap guest frame
    pub fn unmap_page(&mut self, gfn: u64) -> Result<(), KvmError> {
        let gpa = gfn * 4096;

        // Get page table indices
        let pml4_idx = (gpa >> 39) & 0x1FF;
        let pdpt_idx = (gpa >> 30) & 0x1FF;
        let pd_idx = (gpa >> 21) & 0x1FF;
        let pt_idx = (gpa >> 12) & 0x1FF;

        unsafe {
            let pml4 = self.root as *mut PageTableEntry;
            let pml4_entry = pml4.add(pml4_idx as usize);

            if !(*pml4_entry).is_present() {
                return Ok(());
            }

            let pdpt = (*pml4_entry).addr() as *mut PageTableEntry;
            let pdpt_entry = pdpt.add(pdpt_idx as usize);

            if !(*pdpt_entry).is_present() {
                return Ok(());
            }

            if (*pdpt_entry).is_large() {
                *pdpt_entry = PageTableEntry::empty();
                return Ok(());
            }

            let pd = (*pdpt_entry).addr() as *mut PageTableEntry;
            let pd_entry = pd.add(pd_idx as usize);

            if !(*pd_entry).is_present() {
                return Ok(());
            }

            if (*pd_entry).is_large() {
                *pd_entry = PageTableEntry::empty();
                return Ok(());
            }

            let pt = (*pd_entry).addr() as *mut PageTableEntry;
            let pt_entry = pt.add(pt_idx as usize);
            *pt_entry = PageTableEntry::empty();
        }

        Ok(())
    }

    /// Translate GPA to HPA
    pub fn translate(&self, gpa: u64) -> Option<u64> {
        let pml4_idx = (gpa >> 39) & 0x1FF;
        let pdpt_idx = (gpa >> 30) & 0x1FF;
        let pd_idx = (gpa >> 21) & 0x1FF;
        let pt_idx = (gpa >> 12) & 0x1FF;
        let offset = gpa & 0xFFF;

        unsafe {
            let pml4 = self.root as *const PageTableEntry;
            let pml4_entry = *pml4.add(pml4_idx as usize);

            if !pml4_entry.is_present() {
                return None;
            }

            let pdpt = pml4_entry.addr() as *const PageTableEntry;
            let pdpt_entry = *pdpt.add(pdpt_idx as usize);

            if !pdpt_entry.is_present() {
                return None;
            }

            if pdpt_entry.is_large() {
                // 1GB page
                let hpa = pdpt_entry.addr() + (gpa & 0x3FFF_FFFF);
                return Some(hpa);
            }

            let pd = pdpt_entry.addr() as *const PageTableEntry;
            let pd_entry = *pd.add(pd_idx as usize);

            if !pd_entry.is_present() {
                return None;
            }

            if pd_entry.is_large() {
                // 2MB page
                let hpa = pd_entry.addr() + (gpa & 0x1F_FFFF);
                return Some(hpa);
            }

            let pt = pd_entry.addr() as *const PageTableEntry;
            let pt_entry = *pt.add(pt_idx as usize);

            if !pt_entry.is_present() {
                return None;
            }

            Some(pt_entry.addr() + offset)
        }
    }
}

impl Drop for ExtendedPageTable {
    fn drop(&mut self) {
        let pages = self.pages.lock();
        for page in pages.iter() {
            free_page(*page);
        }
    }
}

// =============================================================================
// MEMORY HELPERS
// =============================================================================

/// Allocate a 4KB page
fn allocate_page() -> Result<u64, KvmError> {
    // Would allocate from physical memory
    Ok(0x3000_0000)
}

/// Free a page
fn free_page(_addr: u64) {
    // Would free the page
}

// =============================================================================
// MEMORY BALLOONING
// =============================================================================

/// Memory balloon state
pub struct MemoryBalloon {
    /// Current balloon size (pages)
    current: AtomicU64,
    /// Target balloon size (pages)
    target: AtomicU64,
    /// Inflated pages
    pages: Mutex<Vec<u64>>,
}

impl MemoryBalloon {
    /// Create new balloon
    pub fn new() -> Self {
        Self {
            current: AtomicU64::new(0),
            target: AtomicU64::new(0),
            pages: Mutex::new(Vec::new()),
        }
    }

    /// Set target size
    pub fn set_target(&self, pages: u64) {
        self.target.store(pages, Ordering::SeqCst);
    }

    /// Get current size
    pub fn current(&self) -> u64 {
        self.current.load(Ordering::SeqCst)
    }

    /// Get target size
    pub fn target(&self) -> u64 {
        self.target.load(Ordering::SeqCst)
    }

    /// Inflate balloon (add pages)
    pub fn inflate(&self, pages: &[u64]) {
        let mut balloon_pages = self.pages.lock();
        for page in pages {
            balloon_pages.push(*page);
        }
        self.current.fetch_add(pages.len() as u64, Ordering::SeqCst);
    }

    /// Deflate balloon (release pages)
    pub fn deflate(&self, count: usize) -> Vec<u64> {
        let mut balloon_pages = self.pages.lock();
        let actual_count = count.min(balloon_pages.len());
        let released: Vec<u64> = balloon_pages.drain(..actual_count).collect();
        self.current.fetch_sub(released.len() as u64, Ordering::SeqCst);
        released
    }
}
