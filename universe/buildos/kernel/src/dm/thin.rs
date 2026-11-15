// ===============================================================================
// BUILDOS KERNEL - DEVICE MAPPER THIN PROVISIONING
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Thin Provisioning Target Implementation
//!
//! Provides thin provisioning and over-provisioning support.

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, AtomicBool, Ordering};

use super::{DmError, SECTOR_SIZE};
use crate::sync::RwLock;

/// Block size for thin provisioning (in sectors)
pub const THIN_BLOCK_SIZE_MIN: u64 = 128;      // 64KB
pub const THIN_BLOCK_SIZE_DEFAULT: u64 = 256;  // 128KB
pub const THIN_BLOCK_SIZE_MAX: u64 = 2097152;  // 1GB

/// Thin pool metadata
pub struct ThinPoolMetadata {
    /// Metadata device
    metadata_dev: String,
    /// Data device
    data_dev: String,
    /// Block size in sectors
    block_size: u64,
    /// Total data blocks
    nr_data_blocks: u64,
    /// Used data blocks
    used_data_blocks: AtomicU64,
    /// Next free block
    next_free_block: AtomicU64,
    /// Thin devices in pool
    thin_devices: RwLock<BTreeMap<u64, ThinDeviceMetadata>>,
    /// Space map (free block bitmap)
    space_map: RwLock<SpaceMap>,
    /// Transaction ID
    transaction_id: AtomicU64,
    /// Read-only flag
    read_only: AtomicBool,
    /// Needs check flag
    needs_check: AtomicBool,
}

impl ThinPoolMetadata {
    /// Create new thin pool metadata
    pub fn new(
        metadata_dev: &str,
        data_dev: &str,
        block_size: u64,
        data_size_sectors: u64,
    ) -> Self {
        let nr_data_blocks = data_size_sectors / block_size;

        Self {
            metadata_dev: metadata_dev.to_string(),
            data_dev: data_dev.to_string(),
            block_size,
            nr_data_blocks,
            used_data_blocks: AtomicU64::new(0),
            next_free_block: AtomicU64::new(0),
            thin_devices: RwLock::new(BTreeMap::new()),
            space_map: RwLock::new(SpaceMap::new(nr_data_blocks)),
            transaction_id: AtomicU64::new(0),
            read_only: AtomicBool::new(false),
            needs_check: AtomicBool::new(false),
        }
    }

    /// Create a new thin device
    pub fn create_thin(&self, dev_id: u64, virtual_size: u64) -> Result<(), DmError> {
        let mut devices = self.thin_devices.write();

        if devices.contains_key(&dev_id) {
            return Err(DmError::DeviceExists);
        }

        let virtual_blocks = virtual_size / self.block_size;

        let thin = ThinDeviceMetadata {
            dev_id,
            virtual_blocks,
            mapped_blocks: AtomicU64::new(0),
            mappings: RwLock::new(BTreeMap::new()),
            snapshot_origin: None,
            creation_time: 0, // Would use real timestamp
        };

        devices.insert(dev_id, thin);
        self.transaction_id.fetch_add(1, Ordering::AcqRel);

        Ok(())
    }

    /// Create a snapshot of a thin device
    pub fn create_snapshot(&self, origin_id: u64, snap_id: u64) -> Result<(), DmError> {
        let mut devices = self.thin_devices.write();

        if !devices.contains_key(&origin_id) {
            return Err(DmError::DeviceNotFound);
        }

        if devices.contains_key(&snap_id) {
            return Err(DmError::DeviceExists);
        }

        let origin = devices.get(&origin_id).unwrap();

        // Create snapshot that shares mappings with origin
        let snap = ThinDeviceMetadata {
            dev_id: snap_id,
            virtual_blocks: origin.virtual_blocks,
            mapped_blocks: AtomicU64::new(origin.mapped_blocks.load(Ordering::Relaxed)),
            mappings: RwLock::new(origin.mappings.read().clone()),
            snapshot_origin: Some(origin_id),
            creation_time: 0,
        };

        // Increment reference counts for shared blocks
        let mut space_map = self.space_map.write();
        for (_vblock, &dblock) in snap.mappings.read().iter() {
            space_map.inc_ref(dblock);
        }

        devices.insert(snap_id, snap);
        self.transaction_id.fetch_add(1, Ordering::AcqRel);

        Ok(())
    }

    /// Delete a thin device
    pub fn delete_thin(&self, dev_id: u64) -> Result<(), DmError> {
        let mut devices = self.thin_devices.write();

        let thin = devices.remove(&dev_id)
            .ok_or(DmError::DeviceNotFound)?;

        // Release all blocks
        let mut space_map = self.space_map.write();
        for (_vblock, dblock) in thin.mappings.read().iter() {
            if space_map.dec_ref(*dblock) == 0 {
                // Block is now free
                self.used_data_blocks.fetch_sub(1, Ordering::AcqRel);
            }
        }

        self.transaction_id.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    /// Allocate a data block
    pub fn alloc_block(&self) -> Result<u64, DmError> {
        let mut space_map = self.space_map.write();

        if let Some(block) = space_map.alloc() {
            self.used_data_blocks.fetch_add(1, Ordering::AcqRel);
            Ok(block)
        } else {
            Err(DmError::OutOfMemory)
        }
    }

    /// Insert a mapping for a thin device
    pub fn insert_mapping(
        &self,
        dev_id: u64,
        virtual_block: u64,
        data_block: u64,
    ) -> Result<(), DmError> {
        let devices = self.thin_devices.read();
        let thin = devices.get(&dev_id)
            .ok_or(DmError::DeviceNotFound)?;

        thin.mappings.write().insert(virtual_block, data_block);
        thin.mapped_blocks.fetch_add(1, Ordering::AcqRel);

        Ok(())
    }

    /// Lookup a mapping
    pub fn lookup_mapping(&self, dev_id: u64, virtual_block: u64) -> Result<Option<u64>, DmError> {
        let devices = self.thin_devices.read();
        let thin = devices.get(&dev_id)
            .ok_or(DmError::DeviceNotFound)?;

        let result = thin.mappings.read().get(&virtual_block).copied();
        Ok(result)
    }

    /// Get pool status
    pub fn status(&self) -> ThinPoolStatus {
        ThinPoolStatus {
            transaction_id: self.transaction_id.load(Ordering::Relaxed),
            used_data_blocks: self.used_data_blocks.load(Ordering::Relaxed),
            total_data_blocks: self.nr_data_blocks,
            used_metadata_blocks: 0, // Would calculate from metadata
            total_metadata_blocks: 0,
            held_root: None,
            read_only: self.read_only.load(Ordering::Relaxed),
            needs_check: self.needs_check.load(Ordering::Relaxed),
        }
    }

    /// Get thin device status
    pub fn thin_status(&self, dev_id: u64) -> Result<ThinDeviceStatus, DmError> {
        let devices = self.thin_devices.read();
        let thin = devices.get(&dev_id)
            .ok_or(DmError::DeviceNotFound)?;

        let mapped_blocks = thin.mapped_blocks.load(Ordering::Relaxed);
        let highest_mapped = thin.mappings.read().keys().last().copied().unwrap_or(0);

        Ok(ThinDeviceStatus {
            mapped_blocks,
            highest_mapped,
        })
    }

    /// Reserve metadata for discard
    pub fn reserve_metadata_snap(&self) -> Result<(), DmError> {
        // Would reserve a metadata snapshot
        self.transaction_id.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    /// Release metadata snap
    pub fn release_metadata_snap(&self) -> Result<(), DmError> {
        Ok(())
    }
}

/// Thin device metadata
pub struct ThinDeviceMetadata {
    /// Device ID
    dev_id: u64,
    /// Virtual size in blocks
    virtual_blocks: u64,
    /// Mapped block count
    mapped_blocks: AtomicU64,
    /// Block mappings: virtual block -> data block
    mappings: RwLock<BTreeMap<u64, u64>>,
    /// Snapshot origin (if this is a snapshot)
    snapshot_origin: Option<u64>,
    /// Creation timestamp
    creation_time: u64,
}

/// Space map for tracking data block usage
pub struct SpaceMap {
    /// Reference counts for each block
    ref_counts: Vec<u32>,
    /// Number of free blocks
    nr_free: u64,
    /// Search start hint
    search_start: u64,
}

impl SpaceMap {
    /// Create new space map
    pub fn new(nr_blocks: u64) -> Self {
        Self {
            ref_counts: vec![0; nr_blocks as usize],
            nr_free: nr_blocks,
            search_start: 0,
        }
    }

    /// Allocate a free block
    pub fn alloc(&mut self) -> Option<u64> {
        if self.nr_free == 0 {
            return None;
        }

        let nr_blocks = self.ref_counts.len() as u64;
        let start = self.search_start;

        // Search from hint
        for i in 0..nr_blocks {
            let block = (start + i) % nr_blocks;
            if self.ref_counts[block as usize] == 0 {
                self.ref_counts[block as usize] = 1;
                self.nr_free -= 1;
                self.search_start = (block + 1) % nr_blocks;
                return Some(block);
            }
        }

        None
    }

    /// Increment reference count
    pub fn inc_ref(&mut self, block: u64) {
        if (block as usize) < self.ref_counts.len() {
            if self.ref_counts[block as usize] == 0 {
                self.nr_free -= 1;
            }
            self.ref_counts[block as usize] = self.ref_counts[block as usize].saturating_add(1);
        }
    }

    /// Decrement reference count, returns new count
    pub fn dec_ref(&mut self, block: u64) -> u32 {
        if (block as usize) < self.ref_counts.len() {
            let new_count = self.ref_counts[block as usize].saturating_sub(1);
            self.ref_counts[block as usize] = new_count;
            if new_count == 0 {
                self.nr_free += 1;
            }
            new_count
        } else {
            0
        }
    }

    /// Get reference count
    pub fn get_ref(&self, block: u64) -> u32 {
        self.ref_counts.get(block as usize).copied().unwrap_or(0)
    }

    /// Get free block count
    pub fn free_count(&self) -> u64 {
        self.nr_free
    }
}

/// Thin pool status
#[derive(Clone, Debug)]
pub struct ThinPoolStatus {
    pub transaction_id: u64,
    pub used_data_blocks: u64,
    pub total_data_blocks: u64,
    pub used_metadata_blocks: u64,
    pub total_metadata_blocks: u64,
    pub held_root: Option<u64>,
    pub read_only: bool,
    pub needs_check: bool,
}

impl ThinPoolStatus {
    pub fn data_percent_used(&self) -> f64 {
        if self.total_data_blocks == 0 {
            0.0
        } else {
            (self.used_data_blocks as f64 / self.total_data_blocks as f64) * 100.0
        }
    }

    pub fn metadata_percent_used(&self) -> f64 {
        if self.total_metadata_blocks == 0 {
            0.0
        } else {
            (self.used_metadata_blocks as f64 / self.total_metadata_blocks as f64) * 100.0
        }
    }
}

/// Thin device status
#[derive(Clone, Debug)]
pub struct ThinDeviceStatus {
    pub mapped_blocks: u64,
    pub highest_mapped: u64,
}

/// Thin pool message types
#[derive(Clone, Debug)]
pub enum ThinPoolMessage {
    CreateThin(u64),
    CreateSnap(u64, u64),
    DeleteThin(u64),
    SetTransactionId(u64, u64),
    ReserveMetadataSnap,
    ReleaseMetadataSnap,
}

/// Process a thin pool message
pub fn process_pool_message(
    pool: &ThinPoolMetadata,
    message: ThinPoolMessage,
) -> Result<(), DmError> {
    match message {
        ThinPoolMessage::CreateThin(dev_id) => {
            // Virtual size would be passed separately
            pool.create_thin(dev_id, 1024 * 1024 * 1024 / SECTOR_SIZE)
        }
        ThinPoolMessage::CreateSnap(origin_id, snap_id) => {
            pool.create_snapshot(origin_id, snap_id)
        }
        ThinPoolMessage::DeleteThin(dev_id) => {
            pool.delete_thin(dev_id)
        }
        ThinPoolMessage::SetTransactionId(old, new) => {
            let current = pool.transaction_id.load(Ordering::Relaxed);
            if current != old {
                return Err(DmError::InvalidArgument);
            }
            pool.transaction_id.store(new, Ordering::Release);
            Ok(())
        }
        ThinPoolMessage::ReserveMetadataSnap => {
            pool.reserve_metadata_snap()
        }
        ThinPoolMessage::ReleaseMetadataSnap => {
            pool.release_metadata_snap()
        }
    }
}

/// Thin provisioning superblock (on-disk format)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ThinSuperblock {
    /// Checksum
    pub csum: u32,
    /// Flags
    pub flags: u32,
    /// Magic number
    pub magic: u64,
    /// Version
    pub version: u32,
    /// Time of creation
    pub time: u32,
    /// Transaction ID
    pub transaction_id: u64,
    /// Metadata snapshot block (0 if none)
    pub metadata_snap: u64,
    /// Data space map root
    pub data_space_map_root: [u8; 128],
    /// Metadata space map root
    pub metadata_space_map_root: [u8; 128],
    /// Data mapping root
    pub data_mapping_root: u64,
    /// Device details root
    pub device_details_root: u64,
    /// Data block size (in sectors)
    pub data_block_size: u32,
    /// Metadata block size
    pub metadata_block_size: u32,
    /// Metadata nr blocks
    pub metadata_nr_blocks: u64,
    /// Compat flags
    pub compat_flags: u32,
    /// Compat_ro flags
    pub compat_ro_flags: u32,
    /// Incompat flags
    pub incompat_flags: u32,
}

impl ThinSuperblock {
    pub const MAGIC: u64 = 0x1a231a27a7adb1e;
    pub const VERSION: u32 = 2;

    pub fn new(data_block_size: u32) -> Self {
        Self {
            csum: 0,
            flags: 0,
            magic: Self::MAGIC,
            version: Self::VERSION,
            time: 0,
            transaction_id: 0,
            metadata_snap: 0,
            data_space_map_root: [0; 128],
            metadata_space_map_root: [0; 128],
            data_mapping_root: 0,
            device_details_root: 0,
            data_block_size,
            metadata_block_size: 8, // 4KB
            metadata_nr_blocks: 0,
            compat_flags: 0,
            compat_ro_flags: 0,
            incompat_flags: 0,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.magic == Self::MAGIC && self.version <= Self::VERSION
    }
}

/// Create a thin pool
pub fn create_thin_pool(
    name: &str,
    metadata_dev: &str,
    data_dev: &str,
    block_size: u64,
    low_water_mark: u64,
) -> Result<(), DmError> {
    // Get data device size
    let data_info = super::get_device_info(data_dev)
        .ok_or(DmError::DeviceNotFound)?;

    let args = alloc::format!(
        "{} {} {} {}",
        metadata_dev,
        data_dev,
        block_size,
        low_water_mark
    );

    let mut table = super::table::DmTable::empty();
    table.add_target(0, data_info.size, "thin-pool", &args)?;

    super::create_device(name, table)?;

    Ok(())
}

/// Create a thin device
pub fn create_thin_device(
    name: &str,
    pool_dev: &str,
    dev_id: u64,
    virtual_size: u64,
) -> Result<(), DmError> {
    let args = alloc::format!("{} {}", pool_dev, dev_id);

    let mut table = super::table::DmTable::empty();
    table.add_target(0, virtual_size, "thin", &args)?;

    super::create_device(name, table)?;

    Ok(())
}

/// Activate a thin device
pub fn activate_thin(
    pool_name: &str,
    dev_id: u64,
    name: &str,
    virtual_size: u64,
) -> Result<(), DmError> {
    let pool_path = alloc::format!("/dev/mapper/{}", pool_name);
    create_thin_device(name, &pool_path, dev_id, virtual_size)
}

/// Provision callback (for handling writes to unprovisioned areas)
pub struct ProvisionCallback {
    /// Pool metadata
    pool: *const ThinPoolMetadata,
    /// Device ID
    dev_id: u64,
}

impl ProvisionCallback {
    /// Handle a write to an unprovisioned block
    pub fn provision_block(&self, virtual_block: u64) -> Result<u64, DmError> {
        // Safety: pool pointer is valid for lifetime of callback
        let pool = unsafe { &*self.pool };

        // Check if already mapped (race condition check)
        if let Some(data_block) = pool.lookup_mapping(self.dev_id, virtual_block)? {
            return Ok(data_block);
        }

        // Allocate new block
        let data_block = pool.alloc_block()?;

        // Insert mapping
        pool.insert_mapping(self.dev_id, virtual_block, data_block)?;

        Ok(data_block)
    }

    /// Handle a write that requires COW (for snapshots)
    pub fn break_sharing(&self, virtual_block: u64, _old_block: u64) -> Result<u64, DmError> {
        let pool = unsafe { &*self.pool };

        // Allocate new block
        let new_block = pool.alloc_block()?;

        // Copy data from old block to new block
        // (Would need actual I/O here)

        // Update mapping
        pool.insert_mapping(self.dev_id, virtual_block, new_block)?;

        Ok(new_block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_space_map() {
        let mut sm = SpaceMap::new(100);

        // Allocate some blocks
        let b1 = sm.alloc().unwrap();
        let b2 = sm.alloc().unwrap();

        assert_ne!(b1, b2);
        assert_eq!(sm.free_count(), 98);

        // Inc ref on b1
        sm.inc_ref(b1);
        assert_eq!(sm.get_ref(b1), 2);

        // Dec ref
        assert_eq!(sm.dec_ref(b1), 1);
        assert_eq!(sm.dec_ref(b1), 0);
        assert_eq!(sm.free_count(), 99);
    }

    #[test]
    fn test_thin_pool() {
        let pool = ThinPoolMetadata::new(
            "/dev/sda1",
            "/dev/sda2",
            256,
            1024 * 1024,
        );

        // Create a thin device
        pool.create_thin(0, 1024 * 1024).unwrap();

        // Insert a mapping
        pool.insert_mapping(0, 0, 100).unwrap();

        // Lookup
        assert_eq!(pool.lookup_mapping(0, 0).unwrap(), Some(100));
        assert_eq!(pool.lookup_mapping(0, 1).unwrap(), None);

        // Create snapshot
        pool.create_snapshot(0, 1).unwrap();

        // Both should see the mapping
        assert_eq!(pool.lookup_mapping(0, 0).unwrap(), Some(100));
        assert_eq!(pool.lookup_mapping(1, 0).unwrap(), Some(100));
    }
}
