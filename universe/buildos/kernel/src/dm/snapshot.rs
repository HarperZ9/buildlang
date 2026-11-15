// ===============================================================================
// BUILDOS KERNEL - DEVICE MAPPER SNAPSHOT TARGET
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Snapshot Target Implementation
//!
//! Provides copy-on-write snapshots of block devices.

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use super::{DmError, SECTOR_SIZE};
use crate::sync::RwLock;

/// Snapshot exception store
pub struct ExceptionStore {
    /// COW device
    cow_device: String,
    /// Chunk size in sectors
    chunk_size: u64,
    /// Persistent or transient
    persistent: bool,
    /// Exception map: origin chunk -> COW chunk
    exceptions: RwLock<BTreeMap<u64, u64>>,
    /// Next free chunk on COW device
    next_free_chunk: AtomicU64,
    /// Total COW space in chunks
    cow_chunks: u64,
    /// Valid flag
    valid: bool,
}

impl ExceptionStore {
    /// Create new exception store
    pub fn new(
        cow_device: &str,
        chunk_size: u64,
        cow_size_sectors: u64,
        persistent: bool,
    ) -> Self {
        let cow_chunks = cow_size_sectors / chunk_size;

        // Reserve first chunk for header if persistent
        let first_free = if persistent { 1 } else { 0 };

        Self {
            cow_device: cow_device.to_string(),
            chunk_size,
            persistent,
            exceptions: RwLock::new(BTreeMap::new()),
            next_free_chunk: AtomicU64::new(first_free),
            cow_chunks,
            valid: true,
        }
    }

    /// Look up exception for a chunk
    pub fn lookup(&self, origin_chunk: u64) -> Option<u64> {
        self.exceptions.read().get(&origin_chunk).copied()
    }

    /// Allocate exception for a chunk
    pub fn allocate(&self, origin_chunk: u64) -> Result<u64, DmError> {
        if !self.valid {
            return Err(DmError::IoError);
        }

        // Check if already allocated
        {
            let map = self.exceptions.read();
            if let Some(&cow_chunk) = map.get(&origin_chunk) {
                return Ok(cow_chunk);
            }
        }

        // Allocate new chunk
        let cow_chunk = self.next_free_chunk.fetch_add(1, Ordering::AcqRel);

        if cow_chunk >= self.cow_chunks {
            // Out of COW space
            return Err(DmError::OutOfMemory);
        }

        // Insert exception
        self.exceptions.write().insert(origin_chunk, cow_chunk);

        Ok(cow_chunk)
    }

    /// Get COW device sector for an exception
    pub fn cow_sector(&self, cow_chunk: u64) -> u64 {
        cow_chunk * self.chunk_size
    }

    /// Get chunk for a sector
    pub fn sector_to_chunk(&self, sector: u64) -> u64 {
        sector / self.chunk_size
    }

    /// Get offset within chunk
    pub fn sector_offset(&self, sector: u64) -> u64 {
        sector % self.chunk_size
    }

    /// Get usage statistics
    pub fn usage(&self) -> SnapshotUsage {
        let used = self.next_free_chunk.load(Ordering::Relaxed);
        let reserved = if self.persistent { 1 } else { 0 };

        SnapshotUsage {
            used_chunks: used - reserved,
            total_chunks: self.cow_chunks - reserved,
            chunk_size: self.chunk_size,
            persistent: self.persistent,
        }
    }

    /// Invalidate the snapshot (after overflow)
    pub fn invalidate(&mut self) {
        self.valid = false;
    }

    /// Check if valid
    pub fn is_valid(&self) -> bool {
        self.valid
    }

    /// Get exception count
    pub fn exception_count(&self) -> usize {
        self.exceptions.read().len()
    }

    /// Merge snapshot back to origin (for merge target)
    pub fn get_exceptions(&self) -> Vec<(u64, u64)> {
        self.exceptions
            .read()
            .iter()
            .map(|(&origin, &cow)| (origin, cow))
            .collect()
    }
}

/// Snapshot usage statistics
#[derive(Clone, Debug)]
pub struct SnapshotUsage {
    pub used_chunks: u64,
    pub total_chunks: u64,
    pub chunk_size: u64,
    pub persistent: bool,
}

impl SnapshotUsage {
    pub fn used_bytes(&self) -> u64 {
        self.used_chunks * self.chunk_size * SECTOR_SIZE
    }

    pub fn total_bytes(&self) -> u64 {
        self.total_chunks * self.chunk_size * SECTOR_SIZE
    }

    pub fn percent_full(&self) -> f64 {
        if self.total_chunks == 0 {
            100.0
        } else {
            (self.used_chunks as f64 / self.total_chunks as f64) * 100.0
        }
    }
}

/// Snapshot header (for persistent snapshots)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SnapshotHeader {
    /// Magic number
    pub magic: u32,
    /// Version
    pub version: u32,
    /// Chunk size in sectors
    pub chunk_size: u64,
    /// Number of exceptions
    pub num_exceptions: u64,
    /// Valid flag
    pub valid: u32,
    /// Reserved
    pub reserved: [u8; 236],
}

impl SnapshotHeader {
    pub const MAGIC: u32 = 0x534E4150; // "SNAP"
    pub const VERSION: u32 = 1;

    pub fn new(chunk_size: u64) -> Self {
        Self {
            magic: Self::MAGIC,
            version: Self::VERSION,
            chunk_size,
            num_exceptions: 0,
            valid: 1,
            reserved: [0; 236],
        }
    }

    pub fn is_valid(&self) -> bool {
        self.magic == Self::MAGIC && self.version == Self::VERSION && self.valid == 1
    }
}

/// Create a snapshot of a device
pub fn create_snapshot(
    name: &str,
    origin: &str,
    cow_device: &str,
    chunk_size: u64,
    persistent: bool,
) -> Result<(), DmError> {
    // Get origin size
    let origin_info = super::get_device_info(origin)
        .ok_or(DmError::DeviceNotFound)?;

    // Create snapshot-origin device first
    let origin_name = alloc::format!("{}-origin", name);
    let origin_table = super::table::TableBuilder::new()
        .linear(origin, 0, origin_info.size)
        .build()?;
    super::create_device(&origin_name, origin_table)?;

    // Create snapshot device
    let args = alloc::format!(
        "{} {} {} {}",
        origin,
        cow_device,
        if persistent { "P" } else { "N" },
        chunk_size
    );

    let mut table = super::table::DmTable::empty();
    table.add_target(0, origin_info.size, "snapshot", &args)?;
    super::create_device(name, table)?;

    Ok(())
}

/// Merge a snapshot back to origin
pub fn merge_snapshot(snapshot: &str) -> Result<(), DmError> {
    // Would trigger merge operation
    // For now, just a placeholder
    let info = super::get_device_info(snapshot)
        .ok_or(DmError::DeviceNotFound)?;

    if !info.suspended {
        return Err(DmError::DeviceBusy);
    }

    Ok(())
}

/// Delete a snapshot
pub fn delete_snapshot(name: &str) -> Result<(), DmError> {
    super::remove_device(name)?;

    // Also remove origin wrapper if exists
    let origin_name = alloc::format!("{}-origin", name);
    let _ = super::remove_device(&origin_name);

    Ok(())
}

/// Get snapshot status
pub fn snapshot_status(name: &str) -> Result<SnapshotStatus, DmError> {
    let _info = super::get_device_info(name)
        .ok_or(DmError::DeviceNotFound)?;

    // Parse status from device
    // Simplified: return dummy status
    Ok(SnapshotStatus {
        used_chunks: 0,
        total_chunks: 0,
        metadata_valid: true,
        merge_in_progress: false,
    })
}

/// Snapshot status
#[derive(Clone, Debug)]
pub struct SnapshotStatus {
    pub used_chunks: u64,
    pub total_chunks: u64,
    pub metadata_valid: bool,
    pub merge_in_progress: bool,
}

impl SnapshotStatus {
    pub fn percent_full(&self) -> f64 {
        if self.total_chunks == 0 {
            0.0
        } else {
            (self.used_chunks as f64 / self.total_chunks as f64) * 100.0
        }
    }
}
