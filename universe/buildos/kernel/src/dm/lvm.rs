// ===============================================================================
// BUILDOS KERNEL - LOGICAL VOLUME MANAGER (LVM2)
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! LVM2 Support
//!
//! Provides logical volume management structures and utilities.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use super::table::TableBuilder;
use super::{DmError, SECTOR_SIZE};
use crate::sync::RwLock;

/// LVM label sector
pub const LVM_LABEL_SECTOR: u64 = 1;

/// Physical volume
#[derive(Clone, Debug)]
pub struct PhysicalVolume {
    /// Device path
    pub device: String,
    /// PV UUID
    pub uuid: [u8; 32],
    /// Device size in bytes
    pub size: u64,
    /// Metadata area start
    pub mda_start: u64,
    /// Metadata area size
    pub mda_size: u64,
    /// Physical extent size
    pub pe_size: u64,
    /// First physical extent
    pub pe_start: u64,
    /// Total physical extents
    pub pe_count: u64,
    /// Used physical extents
    pub pe_used: u64,
    /// Volume group UUID (if assigned)
    pub vg_uuid: Option<[u8; 32]>,
    /// Status flags
    pub status: PvStatus,
}

impl PhysicalVolume {
    /// Create a new PV
    pub fn new(device: &str, size: u64, pe_size: u64) -> Self {
        let mut uuid = [0u8; 32];
        // Generate random UUID
        for byte in uuid.iter_mut() {
            *byte = crate::random::random_u8();
        }

        // Calculate PE start (leave room for label + metadata)
        let mda_size = 1024 * 1024; // 1MB metadata area
        let pe_start = ((mda_size + pe_size - 1) / pe_size) * pe_size;
        let pe_count = (size - pe_start) / pe_size;

        Self {
            device: device.to_string(),
            uuid,
            size,
            mda_start: SECTOR_SIZE,
            mda_size,
            pe_size,
            pe_start,
            pe_count,
            pe_used: 0,
            vg_uuid: None,
            status: PvStatus::ALLOCATABLE,
        }
    }

    /// Get free extent count
    pub fn pe_free(&self) -> u64 {
        self.pe_count - self.pe_used
    }

    /// Get free size in bytes
    pub fn free_size(&self) -> u64 {
        self.pe_free() * self.pe_size
    }

    /// Format UUID as string
    pub fn uuid_string(&self) -> String {
        let mut s = String::with_capacity(38);
        for (i, byte) in self.uuid.iter().take(32).enumerate() {
            if i == 6 || i == 10 || i == 14 || i == 18 || i == 22 || i == 26 {
                s.push('-');
            }
            s.push_str(&alloc::format!("{:02x}", byte));
        }
        s
    }
}

bitflags::bitflags! {
    /// Physical volume status flags
    #[derive(Clone, Copy, Debug)]
    pub struct PvStatus: u32 {
        const ALLOCATABLE = 0x0001;
        const EXPORTED = 0x0002;
        const MISSING = 0x0004;
    }
}

/// Volume group
pub struct VolumeGroup {
    /// VG name
    pub name: String,
    /// VG UUID
    pub uuid: [u8; 32],
    /// Sequence number
    pub seqno: AtomicU64,
    /// Physical extent size
    pub pe_size: u64,
    /// Physical volumes
    pub pvs: RwLock<Vec<PhysicalVolume>>,
    /// Logical volumes
    pub lvs: RwLock<BTreeMap<String, LogicalVolume>>,
    /// Status flags
    pub status: VgStatus,
    /// Max LV count (0 = unlimited)
    pub max_lv: u32,
    /// Max PV count (0 = unlimited)
    pub max_pv: u32,
}

impl VolumeGroup {
    /// Create a new VG
    pub fn new(name: &str, pe_size: u64) -> Self {
        let mut uuid = [0u8; 32];
        for byte in uuid.iter_mut() {
            *byte = crate::random::random_u8();
        }

        Self {
            name: name.to_string(),
            uuid,
            seqno: AtomicU64::new(1),
            pe_size,
            pvs: RwLock::new(Vec::new()),
            lvs: RwLock::new(BTreeMap::new()),
            status: VgStatus::RESIZEABLE | VgStatus::READ | VgStatus::WRITE,
            max_lv: 0,
            max_pv: 0,
        }
    }

    /// Add a PV to the VG
    pub fn add_pv(&self, mut pv: PhysicalVolume) -> Result<(), DmError> {
        if pv.pe_size != self.pe_size {
            return Err(DmError::InvalidArgument);
        }

        pv.vg_uuid = Some(self.uuid);

        let mut pvs = self.pvs.write();
        if pvs.iter().any(|p| p.device == pv.device) {
            return Err(DmError::DeviceExists);
        }

        pvs.push(pv);
        self.seqno.fetch_add(1, Ordering::AcqRel);

        Ok(())
    }

    /// Remove a PV from the VG
    pub fn remove_pv(&self, device: &str) -> Result<(), DmError> {
        let mut pvs = self.pvs.write();

        // Check if PV has allocated extents
        let pv = pvs.iter().find(|p| p.device == device)
            .ok_or(DmError::DeviceNotFound)?;

        if pv.pe_used > 0 {
            return Err(DmError::DeviceBusy);
        }

        pvs.retain(|p| p.device != device);
        self.seqno.fetch_add(1, Ordering::AcqRel);

        Ok(())
    }

    /// Get total extent count
    pub fn pe_count(&self) -> u64 {
        self.pvs.read().iter().map(|pv| pv.pe_count).sum()
    }

    /// Get used extent count
    pub fn pe_used(&self) -> u64 {
        self.pvs.read().iter().map(|pv| pv.pe_used).sum()
    }

    /// Get free extent count
    pub fn pe_free(&self) -> u64 {
        self.pe_count() - self.pe_used()
    }

    /// Get total size in bytes
    pub fn size(&self) -> u64 {
        self.pe_count() * self.pe_size
    }

    /// Get free size in bytes
    pub fn free(&self) -> u64 {
        self.pe_free() * self.pe_size
    }

    /// Create a logical volume
    pub fn create_lv(
        &self,
        name: &str,
        size_extents: u64,
        lv_type: LvType,
    ) -> Result<(), DmError> {
        if size_extents > self.pe_free() {
            return Err(DmError::OutOfMemory);
        }

        let mut lvs = self.lvs.write();
        if lvs.contains_key(name) {
            return Err(DmError::DeviceExists);
        }

        let mut uuid = [0u8; 32];
        for byte in uuid.iter_mut() {
            *byte = crate::random::random_u8();
        }

        // Allocate extents
        let segments = self.allocate_extents(size_extents, lv_type)?;

        let lv = LogicalVolume {
            name: name.to_string(),
            uuid,
            vg_name: self.name.clone(),
            size: size_extents * self.pe_size,
            le_count: size_extents,
            segments,
            lv_type,
            status: LvStatus::VISIBLE | LvStatus::READ | LvStatus::WRITE,
            snapshot_of: None,
            origin_size: 0,
        };

        lvs.insert(name.to_string(), lv);
        self.seqno.fetch_add(1, Ordering::AcqRel);

        Ok(())
    }

    /// Allocate extents for an LV
    fn allocate_extents(
        &self,
        count: u64,
        lv_type: LvType,
    ) -> Result<Vec<LvSegment>, DmError> {
        let mut segments = Vec::new();
        let mut remaining = count;
        let mut le = 0u64;

        let mut pvs = self.pvs.write();

        match lv_type {
            LvType::Linear => {
                // Simple linear allocation
                for pv in pvs.iter_mut() {
                    if remaining == 0 {
                        break;
                    }

                    let available = pv.pe_free();
                    if available == 0 {
                        continue;
                    }

                    let alloc = available.min(remaining);
                    let pe_start = pv.pe_used;

                    segments.push(LvSegment {
                        le_start: le,
                        le_count: alloc,
                        seg_type: SegmentType::Striped,
                        stripe_count: 1,
                        stripe_size: 0,
                        stripes: vec![StripeSpec {
                            pv_name: pv.device.clone(),
                            pe_start,
                        }],
                    });

                    pv.pe_used += alloc;
                    le += alloc;
                    remaining -= alloc;
                }
            }
            LvType::Striped { stripe_count, stripe_size } => {
                // Striped allocation across multiple PVs
                let per_stripe = (count + stripe_count as u64 - 1) / stripe_count as u64;

                // Find PVs with enough space
                let stripe_pvs: Vec<(usize, u64)> = pvs.iter()
                    .enumerate()
                    .filter(|(_, pv)| pv.pe_free() >= per_stripe)
                    .take(stripe_count as usize)
                    .map(|(i, pv)| (i, pv.pe_used))
                    .collect();

                if stripe_pvs.len() < stripe_count as usize {
                    return Err(DmError::OutOfMemory);
                }

                let stripes: Vec<StripeSpec> = stripe_pvs.iter()
                    .map(|&(i, pe)| StripeSpec {
                        pv_name: pvs[i].device.clone(),
                        pe_start: pe,
                    })
                    .collect();

                segments.push(LvSegment {
                    le_start: 0,
                    le_count: count,
                    seg_type: SegmentType::Striped,
                    stripe_count: stripe_count as u32,
                    stripe_size,
                    stripes,
                });

                // Update PV usage
                for (i, _) in stripe_pvs {
                    pvs[i].pe_used += per_stripe;
                }

                remaining = 0;
            }
            LvType::Mirror { mirror_count, .. } => {
                // Mirror allocation
                let per_mirror = count;

                // Find PVs for each mirror leg
                let mut mirror_pvs = Vec::new();
                for pv in pvs.iter() {
                    if pv.pe_free() >= per_mirror {
                        mirror_pvs.push((pv.device.clone(), pv.pe_used));
                        if mirror_pvs.len() >= mirror_count as usize {
                            break;
                        }
                    }
                }

                if mirror_pvs.len() < mirror_count as usize {
                    return Err(DmError::OutOfMemory);
                }

                segments.push(LvSegment {
                    le_start: 0,
                    le_count: count,
                    seg_type: SegmentType::Mirror,
                    stripe_count: mirror_count as u32,
                    stripe_size: 0,
                    stripes: mirror_pvs.iter().map(|(name, pe)| StripeSpec {
                        pv_name: name.clone(),
                        pe_start: *pe,
                    }).collect(),
                });

                // Update PV usage for each mirror
                for (pv_name, _) in &mirror_pvs {
                    if let Some(pv) = pvs.iter_mut().find(|p| &p.device == pv_name) {
                        pv.pe_used += per_mirror;
                    }
                }

                remaining = 0;
            }
            LvType::ThinPool { .. } => {
                // Thin pool uses linear allocation
                return self.allocate_extents(count, LvType::Linear);
            }
            LvType::Thin { .. } => {
                // Thin volumes don't consume space directly
                segments.push(LvSegment {
                    le_start: 0,
                    le_count: count,
                    seg_type: SegmentType::Thin,
                    stripe_count: 1,
                    stripe_size: 0,
                    stripes: Vec::new(),
                });
                remaining = 0;
            }
        }

        if remaining > 0 {
            return Err(DmError::OutOfMemory);
        }

        Ok(segments)
    }

    /// Remove a logical volume
    pub fn remove_lv(&self, name: &str) -> Result<(), DmError> {
        let mut lvs = self.lvs.write();
        let lv = lvs.remove(name)
            .ok_or(DmError::DeviceNotFound)?;

        // Free extents
        let mut pvs = self.pvs.write();
        for segment in &lv.segments {
            for stripe in &segment.stripes {
                if let Some(pv) = pvs.iter_mut().find(|p| p.device == stripe.pv_name) {
                    pv.pe_used = pv.pe_used.saturating_sub(segment.le_count);
                }
            }
        }

        self.seqno.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    /// Activate a logical volume
    pub fn activate_lv(&self, name: &str) -> Result<(), DmError> {
        let lvs = self.lvs.read();
        let lv = lvs.get(name)
            .ok_or(DmError::DeviceNotFound)?;

        // Build device mapper table
        let dm_name = alloc::format!("{}-{}", self.name, name);
        let table = lv.build_dm_table(self.pe_size)?;

        super::create_device(&dm_name, table)?;

        Ok(())
    }

    /// Deactivate a logical volume
    pub fn deactivate_lv(&self, name: &str) -> Result<(), DmError> {
        let dm_name = alloc::format!("{}-{}", self.name, name);
        super::remove_device(&dm_name)
    }

    /// Resize a logical volume
    pub fn resize_lv(&self, name: &str, new_size_extents: u64) -> Result<(), DmError> {
        let mut lvs = self.lvs.write();
        let lv = lvs.get_mut(name)
            .ok_or(DmError::DeviceNotFound)?;

        let current = lv.le_count;

        if new_size_extents == current {
            return Ok(());
        }

        if new_size_extents > current {
            // Extend
            let additional = new_size_extents - current;
            if additional > self.pe_free() {
                return Err(DmError::OutOfMemory);
            }

            let new_segments = self.allocate_extents(additional, lv.lv_type)?;

            // Add segments
            for mut seg in new_segments {
                seg.le_start = lv.le_count;
                let seg_le_count = seg.le_count;
                lv.segments.push(seg);
                lv.le_count += seg_le_count;
            }
        } else {
            // Shrink - would need to free extents
            return Err(DmError::InvalidArgument); // Simplified
        }

        lv.size = lv.le_count * self.pe_size;
        self.seqno.fetch_add(1, Ordering::AcqRel);

        Ok(())
    }

    /// Create a snapshot of an LV
    pub fn create_snapshot(
        &self,
        origin_name: &str,
        snap_name: &str,
        size_extents: u64,
    ) -> Result<(), DmError> {
        let origin_size = {
            let lvs = self.lvs.read();
            let origin = lvs.get(origin_name)
                .ok_or(DmError::DeviceNotFound)?;
            origin.size
        };

        // Create COW device for snapshot
        self.create_lv(snap_name, size_extents, LvType::Linear)?;

        // Mark as snapshot
        let mut lvs = self.lvs.write();
        let snap = lvs.get_mut(snap_name).unwrap();
        snap.snapshot_of = Some(origin_name.to_string());
        snap.origin_size = origin_size;

        Ok(())
    }
}

bitflags::bitflags! {
    /// Volume group status flags
    #[derive(Clone, Copy, Debug)]
    pub struct VgStatus: u32 {
        const RESIZEABLE = 0x0001;
        const EXPORTED = 0x0002;
        const PARTIAL = 0x0004;
        const READ = 0x0010;
        const WRITE = 0x0020;
        const CLUSTERED = 0x0040;
        const SHARED = 0x0080;
    }
}

/// Logical volume
#[derive(Clone, Debug)]
pub struct LogicalVolume {
    /// LV name
    pub name: String,
    /// LV UUID
    pub uuid: [u8; 32],
    /// VG name
    pub vg_name: String,
    /// Size in bytes
    pub size: u64,
    /// Logical extent count
    pub le_count: u64,
    /// Segments
    pub segments: Vec<LvSegment>,
    /// LV type
    pub lv_type: LvType,
    /// Status flags
    pub status: LvStatus,
    /// Snapshot origin (if this is a snapshot)
    pub snapshot_of: Option<String>,
    /// Origin size (for snapshots)
    pub origin_size: u64,
}

impl LogicalVolume {
    /// Build a device mapper table for this LV
    pub fn build_dm_table(&self, pe_size: u64) -> Result<super::table::DmTable, DmError> {
        let mut builder = TableBuilder::new();

        for segment in &self.segments {
            let len = segment.le_count * pe_size / SECTOR_SIZE;

            match segment.seg_type {
                SegmentType::Striped if segment.stripe_count == 1 => {
                    // Linear
                    let stripe = &segment.stripes[0];
                    let offset = stripe.pe_start * pe_size / SECTOR_SIZE;
                    builder = builder.linear(&stripe.pv_name, offset, len);
                }
                SegmentType::Striped => {
                    // Striped
                    let stripes: Vec<(&str, u64)> = segment.stripes.iter()
                        .map(|s| (s.pv_name.as_str(), s.pe_start * pe_size / SECTOR_SIZE))
                        .collect();
                    let chunk_sectors = segment.stripe_size / SECTOR_SIZE;
                    builder = builder.striped(chunk_sectors, &stripes, len);
                }
                SegmentType::Mirror => {
                    // Mirror would use mirror target
                    // Simplified: use first stripe as linear
                    if let Some(stripe) = segment.stripes.first() {
                        let offset = stripe.pe_start * pe_size / SECTOR_SIZE;
                        builder = builder.linear(&stripe.pv_name, offset, len);
                    }
                }
                SegmentType::Thin => {
                    // Thin uses thin target - would need pool device
                    builder = builder.zero(len);
                }
                SegmentType::Cache | SegmentType::Raid => {
                    // Simplified
                    builder = builder.error(len);
                }
            }
        }

        builder.build()
    }

    /// Get device mapper name
    pub fn dm_name(&self) -> String {
        alloc::format!("{}-{}", self.vg_name, self.name)
    }
}

bitflags::bitflags! {
    /// Logical volume status flags
    #[derive(Clone, Copy, Debug)]
    pub struct LvStatus: u32 {
        const READ = 0x0001;
        const WRITE = 0x0002;
        const VISIBLE = 0x0004;
        const FIXED_MINOR = 0x0008;
        const ACTIVE = 0x0010;
        const VIRTUAL = 0x0020;
        const MERGING = 0x0040;
        const CONVERTING = 0x0080;
        const THIN_VOLUME = 0x0100;
        const THIN_POOL = 0x0200;
    }
}

/// Logical volume type
#[derive(Clone, Copy, Debug)]
pub enum LvType {
    Linear,
    Striped { stripe_count: u8, stripe_size: u64 },
    Mirror { mirror_count: u8, log_type: MirrorLogType },
    ThinPool { data_lv: u64, metadata_lv: u64 },
    Thin { pool_lv: u64 },
}

#[derive(Clone, Copy, Debug)]
pub enum MirrorLogType {
    Core,
    Disk,
    Mirrored,
}

/// LV segment
#[derive(Clone, Debug)]
pub struct LvSegment {
    /// Starting logical extent
    pub le_start: u64,
    /// Logical extent count
    pub le_count: u64,
    /// Segment type
    pub seg_type: SegmentType,
    /// Number of stripes
    pub stripe_count: u32,
    /// Stripe size in bytes
    pub stripe_size: u64,
    /// Stripe specifications
    pub stripes: Vec<StripeSpec>,
}

/// Segment type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentType {
    Striped,
    Mirror,
    Thin,
    Cache,
    Raid,
}

/// Stripe specification
#[derive(Clone, Debug)]
pub struct StripeSpec {
    /// Physical volume name
    pub pv_name: String,
    /// Starting physical extent
    pub pe_start: u64,
}

/// LVM label header
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LvmLabelHeader {
    /// Label ID ("LABELONE")
    pub id: [u8; 8],
    /// Sector number
    pub sector_xl: u64,
    /// CRC
    pub crc_xl: u32,
    /// Offset to contents
    pub offset_xl: u32,
    /// Label type ("LVM2 001")
    pub label_type: [u8; 8],
}

impl LvmLabelHeader {
    pub const LABEL_ID: &'static [u8; 8] = b"LABELONE";
    pub const LABEL_TYPE: &'static [u8; 8] = b"LVM2 001";

    pub fn is_valid(&self) -> bool {
        &self.id == Self::LABEL_ID && &self.label_type == Self::LABEL_TYPE
    }
}

/// PV header
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PvHeader {
    /// PV UUID
    pub pv_uuid: [u8; 32],
    /// Device size in bytes
    pub device_size_xl: u64,
}

/// Data area descriptor
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DataAreaDescriptor {
    /// Offset
    pub offset: u64,
    /// Size
    pub size: u64,
}

/// VG metadata
#[derive(Clone, Debug)]
pub struct VgMetadata {
    /// Name
    pub name: String,
    /// UUID
    pub uuid: String,
    /// Sequence number
    pub seqno: u64,
    /// Extent size
    pub extent_size: u64,
    /// Status
    pub status: Vec<String>,
    /// Physical volumes
    pub pvs: Vec<PvMetadata>,
    /// Logical volumes
    pub lvs: Vec<LvMetadata>,
}

/// PV metadata (within VG)
#[derive(Clone, Debug)]
pub struct PvMetadata {
    /// Name
    pub name: String,
    /// UUID
    pub uuid: String,
    /// Status
    pub status: Vec<String>,
    /// PE start
    pub pe_start: u64,
    /// PE count
    pub pe_count: u64,
}

/// LV metadata
#[derive(Clone, Debug)]
pub struct LvMetadata {
    /// Name
    pub name: String,
    /// UUID
    pub uuid: String,
    /// Status
    pub status: Vec<String>,
    /// Segment count
    pub segment_count: u32,
    /// Segments
    pub segments: Vec<SegmentMetadata>,
}

/// Segment metadata
#[derive(Clone, Debug)]
pub struct SegmentMetadata {
    /// Start extent
    pub start_extent: u64,
    /// Extent count
    pub extent_count: u64,
    /// Type
    pub seg_type: String,
    /// Stripe count
    pub stripe_count: u32,
    /// Stripes
    pub stripes: Vec<(String, u64)>,
}

/// Global VG registry
static VGS: RwLock<BTreeMap<String, VolumeGroup>> = RwLock::new(BTreeMap::new());

/// Create a volume group
pub fn vgcreate(name: &str, pe_size: u64, devices: &[&str]) -> Result<(), DmError> {
    let mut vgs = VGS.write();

    if vgs.contains_key(name) {
        return Err(DmError::DeviceExists);
    }

    let vg = VolumeGroup::new(name, pe_size);

    for device in devices {
        // Would read device size from actual device
        let size = 1024 * 1024 * 1024; // 1GB placeholder
        let pv = PhysicalVolume::new(device, size, pe_size);
        vg.add_pv(pv)?;
    }

    vgs.insert(name.to_string(), vg);
    Ok(())
}

/// Remove a volume group
pub fn vgremove(name: &str) -> Result<(), DmError> {
    let mut vgs = VGS.write();

    let vg = vgs.get(name).ok_or(DmError::DeviceNotFound)?;

    // Check for active LVs
    if !vg.lvs.read().is_empty() {
        return Err(DmError::DeviceBusy);
    }

    vgs.remove(name);
    Ok(())
}

/// Extend a volume group
pub fn vgextend(name: &str, device: &str) -> Result<(), DmError> {
    let vgs = VGS.read();
    let vg = vgs.get(name).ok_or(DmError::DeviceNotFound)?;

    let size = 1024 * 1024 * 1024; // Placeholder
    let pv = PhysicalVolume::new(device, size, vg.pe_size);
    vg.add_pv(pv)
}

/// Reduce a volume group
pub fn vgreduce(name: &str, device: &str) -> Result<(), DmError> {
    let vgs = VGS.read();
    let vg = vgs.get(name).ok_or(DmError::DeviceNotFound)?;

    vg.remove_pv(device)
}

/// Create a logical volume
pub fn lvcreate(vg_name: &str, lv_name: &str, size_mb: u64) -> Result<(), DmError> {
    let vgs = VGS.read();
    let vg = vgs.get(vg_name).ok_or(DmError::DeviceNotFound)?;

    let size_bytes = size_mb * 1024 * 1024;
    let size_extents = (size_bytes + vg.pe_size - 1) / vg.pe_size;

    vg.create_lv(lv_name, size_extents, LvType::Linear)
}

/// Remove a logical volume
pub fn lvremove(vg_name: &str, lv_name: &str) -> Result<(), DmError> {
    let vgs = VGS.read();
    let vg = vgs.get(vg_name).ok_or(DmError::DeviceNotFound)?;

    // Deactivate first
    let _ = vg.deactivate_lv(lv_name);

    vg.remove_lv(lv_name)
}

/// Activate a logical volume
pub fn lvchange_activate(vg_name: &str, lv_name: &str) -> Result<(), DmError> {
    let vgs = VGS.read();
    let vg = vgs.get(vg_name).ok_or(DmError::DeviceNotFound)?;

    vg.activate_lv(lv_name)
}

/// Deactivate a logical volume
pub fn lvchange_deactivate(vg_name: &str, lv_name: &str) -> Result<(), DmError> {
    let vgs = VGS.read();
    let vg = vgs.get(vg_name).ok_or(DmError::DeviceNotFound)?;

    vg.deactivate_lv(lv_name)
}

/// Display VG info
pub fn vgdisplay(name: &str) -> Option<VgInfo> {
    let vgs = VGS.read();
    let vg = vgs.get(name)?;

    // Extract values before returning to avoid lifetime issues with temporaries
    let vg_name = vg.name.clone();
    let pe_size = vg.pe_size;
    let pe_total = vg.pe_count();
    let pe_free = vg.pe_free();
    let pv_count = vg.pvs.read().len() as u32;
    let lv_count = vg.lvs.read().len() as u32;
    let size = vg.size();
    let free = vg.free();

    Some(VgInfo {
        name: vg_name,
        pe_size,
        pe_total,
        pe_free,
        pv_count,
        lv_count,
        size,
        free,
    })
}

#[derive(Clone, Debug)]
pub struct VgInfo {
    pub name: String,
    pub pe_size: u64,
    pub pe_total: u64,
    pub pe_free: u64,
    pub pv_count: u32,
    pub lv_count: u32,
    pub size: u64,
    pub free: u64,
}

/// Display LV info
pub fn lvdisplay(vg_name: &str, lv_name: &str) -> Option<LvInfo> {
    let vgs = VGS.read();
    let vg = vgs.get(vg_name)?;
    let lvs = vg.lvs.read();
    let lv = lvs.get(lv_name)?;

    Some(LvInfo {
        name: lv.name.clone(),
        vg_name: lv.vg_name.clone(),
        size: lv.size,
        le_count: lv.le_count,
        segment_count: lv.segments.len() as u32,
        dm_name: lv.dm_name(),
    })
}

#[derive(Clone, Debug)]
pub struct LvInfo {
    pub name: String,
    pub vg_name: String,
    pub size: u64,
    pub le_count: u64,
    pub segment_count: u32,
    pub dm_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pv_creation() {
        let pv = PhysicalVolume::new("/dev/sda1", 10 * 1024 * 1024 * 1024, 4 * 1024 * 1024);
        assert!(pv.pe_count > 0);
        assert_eq!(pv.pe_used, 0);
        assert_eq!(pv.pe_free(), pv.pe_count);
    }

    #[test]
    fn test_vg_creation() {
        let vg = VolumeGroup::new("test_vg", 4 * 1024 * 1024);

        let pv = PhysicalVolume::new("/dev/sda1", 10 * 1024 * 1024 * 1024, 4 * 1024 * 1024);
        vg.add_pv(pv).unwrap();

        assert!(vg.pe_count() > 0);
        assert_eq!(vg.pe_used(), 0);
    }
}
