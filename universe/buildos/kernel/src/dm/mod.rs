// ===============================================================================
// BUILDOS KERNEL - DEVICE MAPPER SUBSYSTEM
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Device Mapper (dm) Subsystem
//!
//! Provides virtual block device mapping for:
//! - Linear device concatenation
//! - Striping (RAID-0)
//! - Mirroring (RAID-1)
//! - Snapshots
//! - Thin provisioning
//! - LVM2 support

pub mod target;
pub mod table;
pub mod dm_core;
pub mod linear;
pub mod stripe;
pub mod snapshot;
pub mod thin;
pub mod lvm;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::sync::RwLock;

pub use target::{DmTarget, TargetType};
pub use table::DmTable;
pub use dm_core::{DmDevice, DeviceInfo};

/// Device mapper subsystem state
static DM_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Registered devices
static DEVICES: RwLock<BTreeMap<String, DmDevice>> = RwLock::new(BTreeMap::new());

/// Registered target types
static TARGET_TYPES: RwLock<BTreeMap<String, Box<dyn TargetType>>> = RwLock::new(BTreeMap::new());

/// Device minor number counter
static MINOR_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Device mapper major number
pub const DM_MAJOR: u32 = 253;

/// Maximum devices
pub const DM_MAX_DEVICES: usize = 256;

/// Sector size
pub const SECTOR_SIZE: u64 = 512;

/// Initialize device mapper subsystem
pub fn init() {
    // Register built-in target types
    register_builtin_targets();

    DM_INITIALIZED.store(true, Ordering::Release);
    crate::kprintln!("[DM] Device mapper initialized");
}

/// Register built-in target types
fn register_builtin_targets() {
    use target::*;

    // Linear target
    register_target_type(Box::new(LinearTargetType::new()));

    // Stripe target
    register_target_type(Box::new(StripeTargetType::new()));

    // Mirror target
    register_target_type(Box::new(MirrorTargetType::new()));

    // Snapshot target
    register_target_type(Box::new(SnapshotTargetType::new()));

    // Snapshot-origin target
    register_target_type(Box::new(SnapshotOriginTargetType::new()));

    // Zero target (returns zeros)
    register_target_type(Box::new(ZeroTargetType::new()));

    // Error target (always returns errors)
    register_target_type(Box::new(ErrorTargetType::new()));

    // Thin target
    register_target_type(Box::new(ThinTargetType::new()));

    // Thin-pool target
    register_target_type(Box::new(ThinPoolTargetType::new()));
}

/// Register a target type
pub fn register_target_type(target_type: Box<dyn TargetType>) {
    let name = target_type.name().to_string();
    TARGET_TYPES.write().insert(name, target_type);
}

/// Get a target type by name
pub fn get_target_type(name: &str) -> Option<Box<dyn DmTarget>> {
    TARGET_TYPES.read().get(name).map(|t| t.create())
}

/// Create a new mapped device
pub fn create_device(name: &str, table: DmTable) -> Result<DeviceInfo, DmError> {
    if !DM_INITIALIZED.load(Ordering::Acquire) {
        return Err(DmError::NotInitialized);
    }

    let mut devices = DEVICES.write();

    if devices.contains_key(name) {
        return Err(DmError::DeviceExists);
    }

    if devices.len() >= DM_MAX_DEVICES {
        return Err(DmError::TooManyDevices);
    }

    let minor = MINOR_COUNTER.fetch_add(1, Ordering::Relaxed);
    let device = DmDevice::new(name, minor, table)?;
    let info = device.info();

    devices.insert(name.to_string(), device);

    crate::kprintln!("[DM] Created device '{}' (dm-{})", name, minor);

    Ok(info)
}

/// Remove a mapped device
pub fn remove_device(name: &str) -> Result<(), DmError> {
    let mut devices = DEVICES.write();

    if let Some(device) = devices.remove(name) {
        if device.is_open() {
            devices.insert(name.to_string(), device);
            return Err(DmError::DeviceBusy);
        }
        crate::kprintln!("[DM] Removed device '{}'", name);
        Ok(())
    } else {
        Err(DmError::DeviceNotFound)
    }
}

/// Suspend a device
pub fn suspend_device(name: &str) -> Result<(), DmError> {
    let devices = DEVICES.read();
    if let Some(device) = devices.get(name) {
        device.suspend()
    } else {
        Err(DmError::DeviceNotFound)
    }
}

/// Resume a device
pub fn resume_device(name: &str) -> Result<(), DmError> {
    let devices = DEVICES.read();
    if let Some(device) = devices.get(name) {
        device.resume()
    } else {
        Err(DmError::DeviceNotFound)
    }
}

/// Load a new table into a device (while suspended)
pub fn load_table(name: &str, table: DmTable) -> Result<(), DmError> {
    let devices = DEVICES.read();
    if let Some(device) = devices.get(name) {
        device.load_table(table)
    } else {
        Err(DmError::DeviceNotFound)
    }
}

/// Get device info
pub fn get_device_info(name: &str) -> Option<DeviceInfo> {
    DEVICES.read().get(name).map(|d| d.info())
}

/// List all devices
pub fn list_devices() -> Vec<DeviceInfo> {
    DEVICES.read().values().map(|d| d.info()).collect()
}

/// Get device table
pub fn get_device_table(name: &str) -> Option<Vec<TableEntry>> {
    DEVICES.read().get(name).map(|d| d.table_entries())
}

/// Table entry for display
#[derive(Clone, Debug)]
pub struct TableEntry {
    pub start_sector: u64,
    pub num_sectors: u64,
    pub target_type: String,
    pub target_args: String,
}

/// I/O request to device mapper
pub fn dm_io(name: &str, request: IoRequest) -> Result<IoResult, DmError> {
    let devices = DEVICES.read();
    if let Some(device) = devices.get(name) {
        device.process_io(request)
    } else {
        Err(DmError::DeviceNotFound)
    }
}

/// I/O request
#[derive(Clone, Debug)]
pub struct IoRequest {
    /// Request type
    pub op: IoOp,
    /// Starting sector
    pub sector: u64,
    /// Number of sectors
    pub count: u32,
    /// Data buffer
    pub data: Option<Vec<u8>>,
}

/// I/O operation type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoOp {
    Read,
    Write,
    Flush,
    Discard,
    SecureErase,
}

/// I/O result
#[derive(Clone, Debug)]
pub struct IoResult {
    /// Success
    pub success: bool,
    /// Bytes transferred
    pub bytes: u64,
    /// Data (for reads)
    pub data: Option<Vec<u8>>,
    /// Error (if any)
    pub error: Option<DmError>,
}

/// Device mapper errors
#[derive(Clone, Debug)]
pub enum DmError {
    /// Not initialized
    NotInitialized,
    /// Device already exists
    DeviceExists,
    /// Device not found
    DeviceNotFound,
    /// Device is busy
    DeviceBusy,
    /// Device is suspended
    DeviceSuspended,
    /// Too many devices
    TooManyDevices,
    /// Invalid table
    InvalidTable,
    /// Unknown target type
    UnknownTarget,
    /// I/O error
    IoError,
    /// Out of memory
    OutOfMemory,
    /// Invalid argument
    InvalidArgument,
    /// Target-specific error
    TargetError(String),
}

impl DmError {
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::NotInitialized => -19,    // ENODEV
            Self::DeviceExists => -17,      // EEXIST
            Self::DeviceNotFound => -19,    // ENODEV
            Self::DeviceBusy => -16,        // EBUSY
            Self::DeviceSuspended => -5,    // EIO
            Self::TooManyDevices => -28,    // ENOSPC
            Self::InvalidTable => -22,      // EINVAL
            Self::UnknownTarget => -22,     // EINVAL
            Self::IoError => -5,            // EIO
            Self::OutOfMemory => -12,       // ENOMEM
            Self::InvalidArgument => -22,   // EINVAL
            Self::TargetError(_) => -5,     // EIO
        }
    }
}

/// Device mapper IOCTL commands
pub mod ioctl {
    pub const DM_VERSION: u32 = 0xC138FD00;
    pub const DM_REMOVE_ALL: u32 = 0xC138FD01;
    pub const DM_LIST_DEVICES: u32 = 0xC138FD02;
    pub const DM_DEV_CREATE: u32 = 0xC138FD03;
    pub const DM_DEV_REMOVE: u32 = 0xC138FD04;
    pub const DM_DEV_RENAME: u32 = 0xC138FD05;
    pub const DM_DEV_SUSPEND: u32 = 0xC138FD06;
    pub const DM_DEV_STATUS: u32 = 0xC138FD07;
    pub const DM_DEV_WAIT: u32 = 0xC138FD08;
    pub const DM_TABLE_LOAD: u32 = 0xC138FD09;
    pub const DM_TABLE_CLEAR: u32 = 0xC138FD0A;
    pub const DM_TABLE_DEPS: u32 = 0xC138FD0B;
    pub const DM_TABLE_STATUS: u32 = 0xC138FD0C;
    pub const DM_LIST_VERSIONS: u32 = 0xC138FD0D;
    pub const DM_TARGET_MSG: u32 = 0xC138FD0E;
    pub const DM_DEV_SET_GEOMETRY: u32 = 0xC138FD0F;
}

/// DM IOCTL interface
pub fn dm_ioctl(cmd: u32, arg: &mut DmIoctlArg) -> Result<(), DmError> {
    match cmd {
        ioctl::DM_VERSION => {
            arg.version[0] = 4;
            arg.version[1] = 0;
            arg.version[2] = 0;
            Ok(())
        }
        ioctl::DM_LIST_DEVICES => {
            let devices = list_devices();
            // TODO: Fill in device list
            arg.data_size = devices.len() as u32;
            Ok(())
        }
        ioctl::DM_DEV_CREATE => {
            let name = core::str::from_utf8(&arg.name)
                .map_err(|_| DmError::InvalidArgument)?
                .trim_end_matches('\0');
            let table = DmTable::empty();
            let info = create_device(name, table)?;
            arg.dev = ((DM_MAJOR as u64) << 8) | (info.minor as u64);
            Ok(())
        }
        ioctl::DM_DEV_REMOVE => {
            let name = core::str::from_utf8(&arg.name)
                .map_err(|_| DmError::InvalidArgument)?
                .trim_end_matches('\0');
            remove_device(name)
        }
        ioctl::DM_DEV_SUSPEND => {
            let name = core::str::from_utf8(&arg.name)
                .map_err(|_| DmError::InvalidArgument)?
                .trim_end_matches('\0');
            if arg.flags & DM_SUSPEND_FLAG != 0 {
                suspend_device(name)
            } else {
                resume_device(name)
            }
        }
        ioctl::DM_DEV_STATUS => {
            let name = core::str::from_utf8(&arg.name)
                .map_err(|_| DmError::InvalidArgument)?
                .trim_end_matches('\0');
            if let Some(info) = get_device_info(name) {
                arg.dev = ((DM_MAJOR as u64) << 8) | (info.minor as u64);
                arg.target_count = info.target_count;
                arg.open_count = info.open_count;
                if info.suspended {
                    arg.flags |= DM_SUSPEND_FLAG;
                }
                Ok(())
            } else {
                Err(DmError::DeviceNotFound)
            }
        }
        _ => Err(DmError::InvalidArgument),
    }
}

/// DM IOCTL flags
pub const DM_READONLY_FLAG: u32 = 1 << 0;
pub const DM_SUSPEND_FLAG: u32 = 1 << 1;
pub const DM_PERSISTENT_DEV_FLAG: u32 = 1 << 3;
pub const DM_STATUS_TABLE_FLAG: u32 = 1 << 4;
pub const DM_ACTIVE_PRESENT_FLAG: u32 = 1 << 5;
pub const DM_INACTIVE_PRESENT_FLAG: u32 = 1 << 6;
pub const DM_BUFFER_FULL_FLAG: u32 = 1 << 8;
pub const DM_SKIP_BDGET_FLAG: u32 = 1 << 9;
pub const DM_SKIP_LOCKFS_FLAG: u32 = 1 << 10;
pub const DM_NOFLUSH_FLAG: u32 = 1 << 11;
pub const DM_QUERY_INACTIVE_TABLE_FLAG: u32 = 1 << 12;
pub const DM_UEVENT_GENERATED_FLAG: u32 = 1 << 13;
pub const DM_UUID_FLAG: u32 = 1 << 14;
pub const DM_SECURE_DATA_FLAG: u32 = 1 << 15;
pub const DM_DATA_OUT_FLAG: u32 = 1 << 16;
pub const DM_DEFERRED_REMOVE: u32 = 1 << 17;
pub const DM_INTERNAL_SUSPEND_FLAG: u32 = 1 << 18;

/// DM IOCTL argument structure
#[repr(C)]
pub struct DmIoctlArg {
    pub version: [u32; 3],
    pub data_size: u32,
    pub data_start: u32,
    pub target_count: u32,
    pub open_count: u32,
    pub flags: u32,
    pub event_nr: u32,
    pub padding: u32,
    pub dev: u64,
    pub name: [u8; 128],
    pub uuid: [u8; 129],
    pub data: [u8; 7],
}

impl Default for DmIoctlArg {
    fn default() -> Self {
        Self {
            version: [0; 3],
            data_size: 0,
            data_start: 0,
            target_count: 0,
            open_count: 0,
            flags: 0,
            event_nr: 0,
            padding: 0,
            dev: 0,
            name: [0; 128],
            uuid: [0; 129],
            data: [0; 7],
        }
    }
}
