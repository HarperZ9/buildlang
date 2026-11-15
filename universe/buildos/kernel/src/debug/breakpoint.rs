// ===============================================================================
// BUILDOS KERNEL - BREAKPOINT MANAGEMENT
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Software and Hardware Breakpoint Support
//!
//! Implements:
//! - Software breakpoints (INT3)
//! - Hardware breakpoints (DR0-DR3)
//! - Conditional breakpoints

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::sync::RwLock;

// =============================================================================
// BREAKPOINT TYPES
// =============================================================================

/// Breakpoint type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakpointType {
    /// Software breakpoint (INT3)
    Software,
    /// Hardware execution breakpoint (DR0-DR3)
    HardwareExec,
    /// Hardware write watchpoint
    HardwareWrite,
    /// Hardware read/write watchpoint
    HardwareReadWrite,
    /// Hardware I/O breakpoint
    HardwareIo,
}

/// Breakpoint state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakpointState {
    /// Enabled
    Enabled,
    /// Disabled
    Disabled,
    /// Pending (waiting to be installed)
    Pending,
    /// One-shot (will be removed after hit)
    OneShot,
}

// =============================================================================
// SOFTWARE BREAKPOINT
// =============================================================================

/// Software breakpoint
#[derive(Debug, Clone)]
pub struct SoftwareBreakpoint {
    /// Breakpoint ID
    id: u32,
    /// Address
    address: u64,
    /// Original instruction byte(s)
    original_bytes: [u8; 16],
    /// Length of original instruction
    original_len: u8,
    /// State
    state: BreakpointState,
    /// Hit count
    hit_count: u32,
    /// Ignore count
    ignore_count: u32,
    /// Condition (expression to evaluate)
    condition: Option<BreakpointCondition>,
}

impl SoftwareBreakpoint {
    /// Create new software breakpoint
    pub fn new(id: u32, address: u64) -> Self {
        Self {
            id,
            address,
            original_bytes: [0; 16],
            original_len: 1,
            state: BreakpointState::Pending,
            hit_count: 0,
            ignore_count: 0,
            condition: None,
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

    /// Get state
    pub fn state(&self) -> BreakpointState {
        self.state
    }

    /// Set state
    pub fn set_state(&mut self, state: BreakpointState) {
        self.state = state;
    }

    /// Get hit count
    pub fn hit_count(&self) -> u32 {
        self.hit_count
    }

    /// Increment hit count
    pub fn increment_hit_count(&mut self) {
        self.hit_count = self.hit_count.saturating_add(1);
    }

    /// Set ignore count
    pub fn set_ignore_count(&mut self, count: u32) {
        self.ignore_count = count;
    }

    /// Check if should stop
    pub fn should_stop(&self) -> bool {
        if self.ignore_count > 0 {
            return false;
        }

        if let Some(ref condition) = self.condition {
            return condition.evaluate();
        }

        true
    }

    /// Set original bytes
    pub fn set_original_bytes(&mut self, bytes: &[u8]) {
        let len = bytes.len().min(16);
        self.original_bytes[..len].copy_from_slice(&bytes[..len]);
        self.original_len = len as u8;
    }

    /// Get original bytes
    pub fn original_bytes(&self) -> &[u8] {
        &self.original_bytes[..self.original_len as usize]
    }

    /// Install breakpoint (write INT3)
    pub fn install(&mut self, memory: &mut [u8]) -> bool {
        if memory.is_empty() {
            return false;
        }

        // Save original instruction
        self.original_bytes[0] = memory[0];
        self.original_len = 1;

        // Write INT3 (0xCC)
        memory[0] = 0xCC;

        self.state = BreakpointState::Enabled;
        true
    }

    /// Uninstall breakpoint (restore original)
    pub fn uninstall(&mut self, memory: &mut [u8]) -> bool {
        if memory.is_empty() {
            return false;
        }

        // Restore original instruction
        memory[0] = self.original_bytes[0];

        self.state = BreakpointState::Disabled;
        true
    }
}

// =============================================================================
// HARDWARE BREAKPOINT
// =============================================================================

/// Hardware breakpoint (uses debug registers)
#[derive(Debug, Clone)]
pub struct HardwareBreakpoint {
    /// Breakpoint ID
    id: u32,
    /// Debug register (0-3)
    dreg: u8,
    /// Address
    address: u64,
    /// Breakpoint type
    bp_type: BreakpointType,
    /// Length (1, 2, 4, or 8 bytes)
    length: u8,
    /// State
    state: BreakpointState,
    /// Hit count
    hit_count: u32,
}

impl HardwareBreakpoint {
    /// Create new hardware breakpoint
    pub fn new(id: u32, dreg: u8, address: u64, bp_type: BreakpointType, length: u8) -> Self {
        Self {
            id,
            dreg,
            address,
            bp_type,
            length,
            state: BreakpointState::Pending,
            hit_count: 0,
        }
    }

    /// Get debug register
    pub fn dreg(&self) -> u8 {
        self.dreg
    }

    /// Get address
    pub fn address(&self) -> u64 {
        self.address
    }

    /// Get type
    pub fn bp_type(&self) -> BreakpointType {
        self.bp_type
    }

    /// Get length
    pub fn length(&self) -> u8 {
        self.length
    }

    /// Get DR7 condition bits for this breakpoint
    pub fn dr7_condition(&self) -> u32 {
        match self.bp_type {
            BreakpointType::HardwareExec => 0b00,
            BreakpointType::HardwareWrite => 0b01,
            BreakpointType::HardwareIo => 0b10,
            BreakpointType::HardwareReadWrite => 0b11,
            _ => 0,
        }
    }

    /// Get DR7 length bits for this breakpoint
    pub fn dr7_length(&self) -> u32 {
        match self.length {
            1 => 0b00,
            2 => 0b01,
            8 => 0b10,
            4 => 0b11,
            _ => 0b00,
        }
    }

    /// Get state
    pub fn state(&self) -> BreakpointState {
        self.state
    }

    /// Set state
    pub fn set_state(&mut self, state: BreakpointState) {
        self.state = state;
    }

    /// Increment hit count
    pub fn increment_hit_count(&mut self) {
        self.hit_count = self.hit_count.saturating_add(1);
    }
}

// =============================================================================
// BREAKPOINT CONDITION
// =============================================================================

/// Breakpoint condition type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionOp {
    /// Equal
    Eq,
    /// Not equal
    Ne,
    /// Less than
    Lt,
    /// Less than or equal
    Le,
    /// Greater than
    Gt,
    /// Greater than or equal
    Ge,
}

/// Breakpoint condition operand
#[derive(Debug, Clone)]
pub enum ConditionOperand {
    /// Immediate value
    Immediate(i64),
    /// Register value
    Register(u8),
    /// Memory location
    Memory(u64),
}

/// Breakpoint condition
#[derive(Debug, Clone)]
pub struct BreakpointCondition {
    /// Left operand
    left: ConditionOperand,
    /// Operator
    op: ConditionOp,
    /// Right operand
    right: ConditionOperand,
}

impl BreakpointCondition {
    /// Create new condition
    pub fn new(left: ConditionOperand, op: ConditionOp, right: ConditionOperand) -> Self {
        Self { left, op, right }
    }

    /// Evaluate condition (placeholder)
    pub fn evaluate(&self) -> bool {
        // Would evaluate the condition using current register/memory state
        true
    }
}

// =============================================================================
// BREAKPOINT MANAGER
// =============================================================================

/// Breakpoint manager for a process
pub struct BreakpointManager {
    /// Process ID
    pid: u32,
    /// Software breakpoints
    software_bps: RwLock<BTreeMap<u64, SoftwareBreakpoint>>,
    /// Hardware breakpoints
    hardware_bps: RwLock<[Option<HardwareBreakpoint>; 4]>,
    /// Next breakpoint ID
    next_id: AtomicU32,
}

impl BreakpointManager {
    /// Create new breakpoint manager
    pub fn new(pid: u32) -> Self {
        Self {
            pid,
            software_bps: RwLock::new(BTreeMap::new()),
            hardware_bps: RwLock::new([None, None, None, None]),
            next_id: AtomicU32::new(1),
        }
    }

    /// Allocate breakpoint ID
    fn alloc_id(&self) -> u32 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Add software breakpoint
    pub fn add_software_breakpoint(&self, address: u64) -> u32 {
        let id = self.alloc_id();
        let bp = SoftwareBreakpoint::new(id, address);

        self.software_bps.write().insert(address, bp);
        id
    }

    /// Remove software breakpoint
    pub fn remove_software_breakpoint(&self, address: u64) -> bool {
        self.software_bps.write().remove(&address).is_some()
    }

    /// Get software breakpoint
    pub fn get_software_breakpoint(&self, address: u64) -> Option<SoftwareBreakpoint> {
        self.software_bps.read().get(&address).cloned()
    }

    /// Add hardware breakpoint
    pub fn add_hardware_breakpoint(
        &self,
        address: u64,
        bp_type: BreakpointType,
        length: u8,
    ) -> Option<u32> {
        let mut bps = self.hardware_bps.write();

        // Find free debug register
        for (i, slot) in bps.iter_mut().enumerate() {
            if slot.is_none() {
                let id = self.alloc_id();
                *slot = Some(HardwareBreakpoint::new(id, i as u8, address, bp_type, length));
                return Some(id);
            }
        }

        None // No free debug registers
    }

    /// Remove hardware breakpoint
    pub fn remove_hardware_breakpoint(&self, dreg: u8) -> bool {
        if dreg < 4 {
            let mut bps = self.hardware_bps.write();
            if bps[dreg as usize].is_some() {
                bps[dreg as usize] = None;
                return true;
            }
        }
        false
    }

    /// Get hardware breakpoint
    pub fn get_hardware_breakpoint(&self, dreg: u8) -> Option<HardwareBreakpoint> {
        if dreg < 4 {
            self.hardware_bps.read()[dreg as usize].clone()
        } else {
            None
        }
    }

    /// Get all software breakpoint addresses
    pub fn software_breakpoint_addresses(&self) -> Vec<u64> {
        self.software_bps.read().keys().cloned().collect()
    }

    /// Calculate DR7 value for hardware breakpoints
    pub fn calculate_dr7(&self) -> u64 {
        let bps = self.hardware_bps.read();
        let mut dr7: u64 = 0;

        for (i, bp_opt) in bps.iter().enumerate() {
            if let Some(bp) = bp_opt {
                if matches!(bp.state(), BreakpointState::Enabled) {
                    // Local enable bit
                    dr7 |= 1 << (i * 2);

                    // Condition bits
                    let condition = bp.dr7_condition() as u64;
                    dr7 |= condition << (16 + i * 4);

                    // Length bits
                    let length = bp.dr7_length() as u64;
                    dr7 |= length << (18 + i * 4);
                }
            }
        }

        dr7
    }

    /// Handle breakpoint hit
    pub fn handle_hit(&self, address: u64) -> Option<u32> {
        let mut bps = self.software_bps.write();

        if let Some(bp) = bps.get_mut(&address) {
            bp.increment_hit_count();

            // Handle ignore count
            if bp.ignore_count > 0 {
                // Would decrement and continue
            }

            // Handle one-shot
            if matches!(bp.state(), BreakpointState::OneShot) {
                bp.set_state(BreakpointState::Disabled);
            }

            return Some(bp.id());
        }

        None
    }

    /// Handle debug exception (DR6)
    pub fn handle_debug_exception(&self, dr6: u64) -> Option<(u8, u64)> {
        let bps = self.hardware_bps.read();

        for i in 0..4 {
            if (dr6 & (1 << i)) != 0 {
                if let Some(bp) = &bps[i] {
                    return Some((i as u8, bp.address()));
                }
            }
        }

        None
    }

    /// Enable all breakpoints
    pub fn enable_all(&self) {
        for bp in self.software_bps.write().values_mut() {
            if matches!(bp.state(), BreakpointState::Disabled) {
                bp.set_state(BreakpointState::Enabled);
            }
        }

        for bp_opt in self.hardware_bps.write().iter_mut() {
            if let Some(bp) = bp_opt {
                if matches!(bp.state(), BreakpointState::Disabled) {
                    bp.set_state(BreakpointState::Enabled);
                }
            }
        }
    }

    /// Disable all breakpoints
    pub fn disable_all(&self) {
        for bp in self.software_bps.write().values_mut() {
            if matches!(bp.state(), BreakpointState::Enabled) {
                bp.set_state(BreakpointState::Disabled);
            }
        }

        for bp_opt in self.hardware_bps.write().iter_mut() {
            if let Some(bp) = bp_opt {
                if matches!(bp.state(), BreakpointState::Enabled) {
                    bp.set_state(BreakpointState::Disabled);
                }
            }
        }
    }

    /// Clear all breakpoints
    pub fn clear_all(&self) {
        self.software_bps.write().clear();
        *self.hardware_bps.write() = [None, None, None, None];
    }
}
