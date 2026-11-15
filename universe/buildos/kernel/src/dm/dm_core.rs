// ===============================================================================
// BUILDOS KERNEL - DEVICE MAPPER CORE
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Device Mapper Core
//!
//! Core device management and I/O handling.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::sync::{Spinlock, RwLock};
use super::table::DmTable;
use super::target::{MappedIo, StatusType};
use super::{DmError, IoRequest, IoResult, IoOp, TableEntry, SECTOR_SIZE};

/// A mapped device
pub struct DmDevice {
    /// Device name
    name: String,
    /// Minor number
    minor: u32,
    /// Active table
    active_table: RwLock<Option<DmTable>>,
    /// Inactive table (for atomic table swap)
    inactive_table: Spinlock<Option<DmTable>>,
    /// Suspended flag
    suspended: AtomicBool,
    /// Open count
    open_count: AtomicU32,
    /// Read-only flag
    read_only: AtomicBool,
    /// Event counter
    event_nr: AtomicU32,
    /// UUID
    uuid: String,
}

impl DmDevice {
    /// Create a new device
    pub fn new(name: &str, minor: u32, table: DmTable) -> Result<Self, DmError> {
        Ok(Self {
            name: name.to_string(),
            minor,
            active_table: RwLock::new(Some(table)),
            inactive_table: Spinlock::new(None),
            suspended: AtomicBool::new(false),
            open_count: AtomicU32::new(0),
            read_only: AtomicBool::new(false),
            event_nr: AtomicU32::new(0),
            uuid: String::new(),
        })
    }

    /// Get device name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get minor number
    pub fn minor(&self) -> u32 {
        self.minor
    }

    /// Get device info
    pub fn info(&self) -> DeviceInfo {
        let table_guard = self.active_table.read();
        let target_count = table_guard.as_ref().map(|t| t.target_count()).unwrap_or(0);
        let size = table_guard.as_ref().map(|t| t.size()).unwrap_or(0);

        DeviceInfo {
            name: self.name.clone(),
            minor: self.minor,
            size,
            target_count,
            open_count: self.open_count.load(Ordering::Relaxed),
            suspended: self.suspended.load(Ordering::Relaxed),
            read_only: self.read_only.load(Ordering::Relaxed),
            uuid: self.uuid.clone(),
        }
    }

    /// Check if device is open
    pub fn is_open(&self) -> bool {
        self.open_count.load(Ordering::Relaxed) > 0
    }

    /// Open device
    pub fn open(&self) -> Result<(), DmError> {
        if self.suspended.load(Ordering::Acquire) {
            return Err(DmError::DeviceSuspended);
        }
        self.open_count.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    /// Close device
    pub fn close(&self) {
        let prev = self.open_count.fetch_sub(1, Ordering::AcqRel);
        if prev == 0 {
            // Underflow - shouldn't happen
            self.open_count.store(0, Ordering::Release);
        }
    }

    /// Suspend device
    pub fn suspend(&self) -> Result<(), DmError> {
        if self.suspended.load(Ordering::Acquire) {
            return Ok(()); // Already suspended
        }

        // Call presuspend on all targets
        let table_guard = self.active_table.read();
        if let Some(ref table) = *table_guard {
            for _target in table.targets() {
                // Note: In real implementation, would call presuspend
            }
        }

        self.suspended.store(true, Ordering::Release);

        // Call postsuspend
        if let Some(ref table) = *table_guard {
            for _target in table.targets() {
                // Note: In real implementation, would call postsuspend
            }
        }

        self.event_nr.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Resume device
    pub fn resume(&self) -> Result<(), DmError> {
        if !self.suspended.load(Ordering::Acquire) {
            return Ok(()); // Not suspended
        }

        // Swap tables if inactive table is loaded
        {
            let mut inactive = self.inactive_table.lock();
            if inactive.is_some() {
                let mut active = self.active_table.write();
                *active = inactive.take();
            }
        }

        // Call preresume/resume on all targets
        let table_guard = self.active_table.read();
        if let Some(ref table) = *table_guard {
            for _target in table.targets() {
                // Note: In real implementation, would call preresume/resume
            }
        }

        self.suspended.store(false, Ordering::Release);
        self.event_nr.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Load new table (into inactive slot)
    pub fn load_table(&self, table: DmTable) -> Result<(), DmError> {
        if !self.suspended.load(Ordering::Acquire) {
            return Err(DmError::DeviceBusy);
        }

        *self.inactive_table.lock() = Some(table);
        Ok(())
    }

    /// Clear inactive table
    pub fn clear_table(&self) {
        *self.inactive_table.lock() = None;
    }

    /// Get table entries for display
    pub fn table_entries(&self) -> Vec<TableEntry> {
        let guard = self.active_table.read();
        if let Some(ref table) = *guard {
            table.entries()
        } else {
            Vec::new()
        }
    }

    /// Process I/O request
    pub fn process_io(&self, request: IoRequest) -> Result<IoResult, DmError> {
        if self.suspended.load(Ordering::Acquire) {
            return Err(DmError::DeviceSuspended);
        }

        if self.read_only.load(Ordering::Acquire) {
            match request.op {
                IoOp::Write | IoOp::Discard | IoOp::SecureErase => {
                    return Err(DmError::IoError);
                }
                _ => {}
            }
        }

        let table_guard = self.active_table.read();
        let table = table_guard.as_ref().ok_or(DmError::InvalidTable)?;

        // Find target for this sector
        let target = table.find_target(request.sector).ok_or(DmError::IoError)?;

        // Map the I/O
        let mapped = target.map(&request)?;

        // Process the mapped I/O
        match mapped {
            MappedIo::Remap { device: _, sector: _ } => {
                // Would dispatch to underlying device
                // For now, return success placeholder
                Ok(IoResult {
                    success: true,
                    bytes: request.count as u64 * SECTOR_SIZE,
                    data: if request.op == IoOp::Read {
                        Some(vec![0u8; request.count as usize * SECTOR_SIZE as usize])
                    } else {
                        None
                    },
                    error: None,
                })
            }
            MappedIo::Complete(result) => Ok(result),
            MappedIo::Split(remaps) => {
                // Process each remap
                let mut total_bytes = 0u64;
                let all_data = Vec::new();

                for remap in remaps {
                    if let MappedIo::Remap { device: _, sector: _ } = remap {
                        // Would dispatch to underlying device
                        total_bytes += request.count as u64 * SECTOR_SIZE;
                    }
                }

                Ok(IoResult {
                    success: true,
                    bytes: total_bytes,
                    data: if request.op == IoOp::Read { Some(all_data) } else { None },
                    error: None,
                })
            }
            MappedIo::Queue => {
                // Would queue for later processing
                Ok(IoResult {
                    success: true,
                    bytes: 0,
                    data: None,
                    error: None,
                })
            }
            MappedIo::Requeue => {
                // Would requeue
                Err(DmError::IoError)
            }
        }
    }

    /// Set UUID
    pub fn set_uuid(&mut self, uuid: &str) {
        self.uuid = uuid.to_string();
    }

    /// Set read-only
    pub fn set_read_only(&self, ro: bool) {
        self.read_only.store(ro, Ordering::Release);
    }

    /// Get status
    pub fn status(&self, status_type: StatusType) -> String {
        let guard = self.active_table.read();
        if let Some(ref table) = *guard {
            table.status(status_type)
        } else {
            String::new()
        }
    }

    /// Get dependencies
    pub fn deps(&self) -> Vec<(u32, u32)> {
        let guard = self.active_table.read();
        if let Some(ref table) = *guard {
            table.deps()
        } else {
            Vec::new()
        }
    }
}

/// Device information
#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub name: String,
    pub minor: u32,
    pub size: u64,
    pub target_count: u32,
    pub open_count: u32,
    pub suspended: bool,
    pub read_only: bool,
    pub uuid: String,
}

/// Block device reference
#[derive(Clone, Debug)]
pub struct BlockDevice {
    /// Major number
    pub major: u32,
    /// Minor number
    pub minor: u32,
    /// Device path
    pub path: String,
    /// Sector count
    pub sectors: u64,
}

impl BlockDevice {
    pub fn new(major: u32, minor: u32, path: &str) -> Self {
        Self {
            major,
            minor,
            path: path.to_string(),
            sectors: 0,
        }
    }

    /// Get device number (combined major/minor)
    pub fn dev(&self) -> u64 {
        ((self.major as u64) << 20) | (self.minor as u64)
    }
}

/// I/O queue for a device
pub struct IoQueue {
    pending: Vec<IoRequest>,
    max_size: usize,
}

impl IoQueue {
    pub fn new(max_size: usize) -> Self {
        Self {
            pending: Vec::with_capacity(max_size),
            max_size,
        }
    }

    pub fn enqueue(&mut self, request: IoRequest) -> Result<(), DmError> {
        if self.pending.len() >= self.max_size {
            return Err(DmError::IoError);
        }
        self.pending.push(request);
        Ok(())
    }

    pub fn dequeue(&mut self) -> Option<IoRequest> {
        if self.pending.is_empty() {
            None
        } else {
            Some(self.pending.remove(0))
        }
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

/// Event for device mapper
#[derive(Clone, Debug)]
pub struct DmEvent {
    pub device: String,
    pub event_nr: u32,
    pub event_type: DmEventType,
}

#[derive(Clone, Copy, Debug)]
pub enum DmEventType {
    TableChange,
    Suspend,
    Resume,
    Error,
}

/// Wait for device event
pub fn wait_event(name: &str, _event_nr: u32) -> Result<u32, DmError> {
    // Would block until event_nr changes
    // For now, just return current
    let devices = super::DEVICES.read();
    if let Some(device) = devices.get(name) {
        Ok(device.event_nr.load(Ordering::Relaxed))
    } else {
        Err(DmError::DeviceNotFound)
    }
}
