// ===============================================================================
// BUILDOS KERNEL - WATCHPOINT SUPPORT
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Memory Watchpoint Support
//!
//! Implements hardware watchpoints using debug registers:
//! - Write watchpoints
//! - Read/Write watchpoints
//! - Data breakpoints

#![allow(dead_code)]

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::sync::RwLock;

// =============================================================================
// WATCHPOINT TYPES
// =============================================================================

/// Watchpoint type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchpointType {
    /// Trigger on write
    Write,
    /// Trigger on read or write
    ReadWrite,
    /// Trigger on execution
    Execute,
}

/// Watchpoint state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchpointState {
    /// Active
    Active,
    /// Inactive
    Inactive,
    /// Pending activation
    Pending,
}

// =============================================================================
// WATCHPOINT
// =============================================================================

/// Memory watchpoint
#[derive(Debug, Clone)]
pub struct Watchpoint {
    /// Watchpoint ID
    id: u32,
    /// Start address
    address: u64,
    /// Length (1, 2, 4, or 8 bytes)
    length: u8,
    /// Watch type
    watch_type: WatchpointType,
    /// Current state
    state: WatchpointState,
    /// Debug register assigned (-1 if none)
    dreg: i8,
    /// Hit count
    hit_count: u32,
    /// Old value (for comparison)
    old_value: u64,
}

impl Watchpoint {
    /// Create new watchpoint
    pub fn new(id: u32, address: u64, length: u8, watch_type: WatchpointType) -> Self {
        // Validate and normalize length
        let length = match length {
            0..=1 => 1,
            2 => 2,
            3..=4 => 4,
            _ => 8,
        };

        Self {
            id,
            address,
            length,
            watch_type,
            state: WatchpointState::Pending,
            dreg: -1,
            hit_count: 0,
            old_value: 0,
        }
    }

    /// Get ID
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Get address
    pub fn address(&self) -> u64 {
        self.address
    }

    /// Get length
    pub fn length(&self) -> u8 {
        self.length
    }

    /// Get type
    pub fn watch_type(&self) -> WatchpointType {
        self.watch_type
    }

    /// Get state
    pub fn state(&self) -> WatchpointState {
        self.state
    }

    /// Set state
    pub fn set_state(&mut self, state: WatchpointState) {
        self.state = state;
    }

    /// Get assigned debug register
    pub fn dreg(&self) -> i8 {
        self.dreg
    }

    /// Assign debug register
    pub fn assign_dreg(&mut self, dreg: i8) {
        self.dreg = dreg;
    }

    /// Get hit count
    pub fn hit_count(&self) -> u32 {
        self.hit_count
    }

    /// Increment hit count
    pub fn increment_hit_count(&mut self) {
        self.hit_count = self.hit_count.saturating_add(1);
    }

    /// Get old value
    pub fn old_value(&self) -> u64 {
        self.old_value
    }

    /// Set old value
    pub fn set_old_value(&mut self, value: u64) {
        self.old_value = value;
    }

    /// Check if address is in watchpoint range
    pub fn contains(&self, addr: u64) -> bool {
        addr >= self.address && addr < self.address + self.length as u64
    }

    /// Check if range overlaps with watchpoint
    pub fn overlaps(&self, addr: u64, len: u64) -> bool {
        let wp_end = self.address + self.length as u64;
        let access_end = addr + len;

        !(access_end <= self.address || addr >= wp_end)
    }

    /// Get DR7 condition bits
    pub fn dr7_condition(&self) -> u32 {
        match self.watch_type {
            WatchpointType::Execute => 0b00,
            WatchpointType::Write => 0b01,
            WatchpointType::ReadWrite => 0b11,
        }
    }

    /// Get DR7 length bits
    pub fn dr7_length(&self) -> u32 {
        match self.length {
            1 => 0b00,
            2 => 0b01,
            8 => 0b10,
            4 => 0b11,
            _ => 0b00,
        }
    }
}

// =============================================================================
// WATCHPOINT MANAGER
// =============================================================================

/// Watchpoint manager
pub struct WatchpointManager {
    /// Process ID
    pid: u32,
    /// Watchpoints
    watchpoints: RwLock<Vec<Watchpoint>>,
    /// Debug registers in use (bitmap)
    dregs_used: AtomicU32,
    /// Next watchpoint ID
    next_id: AtomicU32,
    /// Software watchpoint mode (when hardware not available)
    software_mode: bool,
}

impl WatchpointManager {
    /// Create new watchpoint manager
    pub fn new(pid: u32) -> Self {
        Self {
            pid,
            watchpoints: RwLock::new(Vec::new()),
            dregs_used: AtomicU32::new(0),
            next_id: AtomicU32::new(1),
            software_mode: false,
        }
    }

    /// Allocate watchpoint ID
    fn alloc_id(&self) -> u32 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Allocate debug register
    fn alloc_dreg(&self) -> Option<u8> {
        loop {
            let used = self.dregs_used.load(Ordering::SeqCst);

            for i in 0..4 {
                if (used & (1 << i)) == 0 {
                    let new_used = used | (1 << i);
                    if self.dregs_used.compare_exchange(
                        used, new_used, Ordering::SeqCst, Ordering::SeqCst
                    ).is_ok() {
                        return Some(i);
                    }
                    break; // Retry
                }
            }

            if used == 0xF {
                return None; // All in use
            }
        }
    }

    /// Free debug register
    fn free_dreg(&self, dreg: u8) {
        if dreg < 4 {
            self.dregs_used.fetch_and(!(1 << dreg), Ordering::SeqCst);
        }
    }

    /// Add watchpoint
    pub fn add(&self, address: u64, length: u8, watch_type: WatchpointType) -> Option<u32> {
        // Try to allocate debug register
        let dreg = self.alloc_dreg();

        let id = self.alloc_id();
        let mut wp = Watchpoint::new(id, address, length, watch_type);

        if let Some(dreg) = dreg {
            wp.assign_dreg(dreg as i8);
            wp.set_state(WatchpointState::Active);
        } else if self.software_mode {
            // Software mode - will use page protection
            wp.set_state(WatchpointState::Active);
        } else {
            return None; // No resources available
        }

        self.watchpoints.write().push(wp);
        Some(id)
    }

    /// Remove watchpoint by ID
    pub fn remove(&self, id: u32) -> bool {
        let mut wps = self.watchpoints.write();

        if let Some(pos) = wps.iter().position(|wp| wp.id() == id) {
            let wp = wps.remove(pos);

            // Free debug register if assigned
            if wp.dreg() >= 0 {
                self.free_dreg(wp.dreg() as u8);
            }

            return true;
        }

        false
    }

    /// Remove watchpoint by address
    pub fn remove_by_address(&self, address: u64) -> bool {
        let mut wps = self.watchpoints.write();

        if let Some(pos) = wps.iter().position(|wp| wp.address() == address) {
            let wp = wps.remove(pos);

            if wp.dreg() >= 0 {
                self.free_dreg(wp.dreg() as u8);
            }

            return true;
        }

        false
    }

    /// Get watchpoint by ID
    pub fn get(&self, id: u32) -> Option<Watchpoint> {
        self.watchpoints.read().iter().find(|wp| wp.id() == id).cloned()
    }

    /// Get watchpoint by address
    pub fn get_by_address(&self, address: u64) -> Option<Watchpoint> {
        self.watchpoints.read().iter().find(|wp| wp.address() == address).cloned()
    }

    /// Check if address is watched
    pub fn is_watched(&self, address: u64, length: u64, is_write: bool) -> Option<u32> {
        let wps = self.watchpoints.read();

        for wp in wps.iter() {
            if !matches!(wp.state(), WatchpointState::Active) {
                continue;
            }

            if wp.overlaps(address, length) {
                // Check type
                match (wp.watch_type(), is_write) {
                    (WatchpointType::Write, true) => return Some(wp.id()),
                    (WatchpointType::ReadWrite, _) => return Some(wp.id()),
                    (WatchpointType::Execute, _) => {
                        // Execute watchpoints need special handling
                    }
                    _ => {}
                }
            }
        }

        None
    }

    /// Handle debug exception
    pub fn handle_exception(&self, dr6: u64) -> Option<Watchpoint> {
        let mut wps = self.watchpoints.write();

        for i in 0..4 {
            if (dr6 & (1 << i)) != 0 {
                for wp in wps.iter_mut() {
                    if wp.dreg() == i as i8 {
                        wp.increment_hit_count();
                        return Some(wp.clone());
                    }
                }
            }
        }

        None
    }

    /// Get all active watchpoints
    pub fn get_active(&self) -> Vec<Watchpoint> {
        self.watchpoints.read()
            .iter()
            .filter(|wp| matches!(wp.state(), WatchpointState::Active))
            .cloned()
            .collect()
    }

    /// Calculate DR7 value
    pub fn calculate_dr7(&self) -> u64 {
        let wps = self.watchpoints.read();
        let mut dr7: u64 = 0;

        for wp in wps.iter() {
            if let WatchpointState::Active = wp.state() {
                if wp.dreg() >= 0 && wp.dreg() < 4 {
                    let i = wp.dreg() as usize;

                    // Local enable
                    dr7 |= 1 << (i * 2);

                    // Condition
                    dr7 |= (wp.dr7_condition() as u64) << (16 + i * 4);

                    // Length
                    dr7 |= (wp.dr7_length() as u64) << (18 + i * 4);
                }
            }
        }

        dr7
    }

    /// Get debug register values
    pub fn get_dregs(&self) -> [u64; 4] {
        let mut dregs = [0u64; 4];
        let wps = self.watchpoints.read();

        for wp in wps.iter() {
            if wp.dreg() >= 0 && wp.dreg() < 4 {
                dregs[wp.dreg() as usize] = wp.address();
            }
        }

        dregs
    }

    /// Enable all watchpoints
    pub fn enable_all(&self) {
        for wp in self.watchpoints.write().iter_mut() {
            if matches!(wp.state(), WatchpointState::Inactive) {
                wp.set_state(WatchpointState::Active);
            }
        }
    }

    /// Disable all watchpoints
    pub fn disable_all(&self) {
        for wp in self.watchpoints.write().iter_mut() {
            if matches!(wp.state(), WatchpointState::Active) {
                wp.set_state(WatchpointState::Inactive);
            }
        }
    }

    /// Clear all watchpoints
    pub fn clear_all(&self) {
        let mut wps = self.watchpoints.write();

        for wp in wps.iter() {
            if wp.dreg() >= 0 {
                self.free_dreg(wp.dreg() as u8);
            }
        }

        wps.clear();
    }

    /// Get count
    pub fn count(&self) -> usize {
        self.watchpoints.read().len()
    }

    /// Check if at capacity
    pub fn at_capacity(&self) -> bool {
        let used = self.dregs_used.load(Ordering::SeqCst);
        used == 0xF && !self.software_mode
    }
}

// =============================================================================
// SOFTWARE WATCHPOINT
// =============================================================================

/// Software watchpoint implementation using page protection
pub struct SoftwareWatchpoint {
    /// Watchpoint ID
    id: u32,
    /// Page address
    page_addr: u64,
    /// Watched range start
    range_start: u64,
    /// Watched range length
    range_len: u64,
    /// Original page protection
    original_prot: u32,
    /// Watch type
    watch_type: WatchpointType,
}

impl SoftwareWatchpoint {
    /// Create new software watchpoint
    pub fn new(id: u32, address: u64, length: u64, watch_type: WatchpointType) -> Self {
        let page_addr = address & !0xFFF;

        Self {
            id,
            page_addr,
            range_start: address,
            range_len: length,
            original_prot: 0x7, // RWX
            watch_type,
        }
    }

    /// Install watchpoint (remove write permission)
    pub fn install(&mut self) {
        // Would modify page table to remove write permission
        // Page fault handler will check for watchpoint hit
    }

    /// Uninstall watchpoint (restore permissions)
    pub fn uninstall(&mut self) {
        // Would restore original page permissions
    }

    /// Check if access triggers watchpoint
    pub fn check_access(&self, addr: u64, len: u64, is_write: bool) -> bool {
        let range_end = self.range_start + self.range_len;
        let access_end = addr + len;

        // Check overlap
        if access_end <= self.range_start || addr >= range_end {
            return false;
        }

        // Check access type
        match (self.watch_type, is_write) {
            (WatchpointType::Write, true) => true,
            (WatchpointType::ReadWrite, _) => true,
            _ => false,
        }
    }
}
