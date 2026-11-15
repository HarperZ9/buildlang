// ===============================================================================
// BUILDOS KERNEL - I/O VIRTUALIZATION
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! I/O Virtualization
//!
//! Handles virtualized I/O operations:
//! - Port I/O interception
//! - MMIO region management
//! - I/O event file descriptors
//! - Coalesced MMIO

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::sync::{Mutex, RwLock};
use super::KvmError;

// =============================================================================
// PORT I/O
// =============================================================================

/// I/O port range
#[derive(Debug, Clone, Copy)]
pub struct IoPortRange {
    /// Start port
    pub start: u16,
    /// End port (exclusive)
    pub end: u16,
}

impl IoPortRange {
    /// Create new port range
    pub fn new(start: u16, count: u16) -> Self {
        Self {
            start,
            end: start.saturating_add(count),
        }
    }

    /// Check if port is in range
    pub fn contains(&self, port: u16) -> bool {
        port >= self.start && port < self.end
    }
}

/// Port I/O handler
pub trait PortIoHandler: Send + Sync {
    /// Handle port read
    fn read(&self, port: u16, size: u8) -> u32;

    /// Handle port write
    fn write(&self, port: u16, size: u8, value: u32);
}

/// Port I/O manager
pub struct PortIoManager {
    /// Registered handlers
    handlers: RwLock<BTreeMap<u16, Arc<dyn PortIoHandler>>>,
}

impl PortIoManager {
    /// Create new port I/O manager
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(BTreeMap::new()),
        }
    }

    /// Register port I/O handler
    pub fn register(&self, range: IoPortRange, handler: Arc<dyn PortIoHandler>) {
        let mut handlers = self.handlers.write();
        for port in range.start..range.end {
            handlers.insert(port, handler.clone());
        }
    }

    /// Unregister port I/O handler
    pub fn unregister(&self, range: IoPortRange) {
        let mut handlers = self.handlers.write();
        for port in range.start..range.end {
            handlers.remove(&port);
        }
    }

    /// Handle port read
    pub fn handle_read(&self, port: u16, size: u8) -> Option<u32> {
        let handlers = self.handlers.read();
        handlers.get(&port).map(|h| h.read(port, size))
    }

    /// Handle port write
    pub fn handle_write(&self, port: u16, size: u8, value: u32) -> bool {
        let handlers = self.handlers.read();
        if let Some(handler) = handlers.get(&port) {
            handler.write(port, size, value);
            true
        } else {
            false
        }
    }
}

// =============================================================================
// MMIO
// =============================================================================

/// MMIO region
#[derive(Debug, Clone)]
pub struct MmioRegion {
    /// Guest physical address start
    pub gpa: u64,
    /// Size in bytes
    pub size: u64,
    /// Handler ID
    pub handler_id: u32,
}

impl MmioRegion {
    /// Check if address is in region
    pub fn contains(&self, gpa: u64) -> bool {
        gpa >= self.gpa && gpa < self.gpa + self.size
    }
}

/// MMIO handler
pub trait MmioHandler: Send + Sync {
    /// Handle MMIO read
    fn read(&self, offset: u64, size: u8) -> u64;

    /// Handle MMIO write
    fn write(&self, offset: u64, size: u8, value: u64);
}

/// MMIO manager
pub struct MmioManager {
    /// Registered regions
    regions: RwLock<Vec<(MmioRegion, Arc<dyn MmioHandler>)>>,
    /// Next handler ID
    next_id: AtomicU64,
}

impl MmioManager {
    /// Create new MMIO manager
    pub fn new() -> Self {
        Self {
            regions: RwLock::new(Vec::new()),
            next_id: AtomicU64::new(1),
        }
    }

    /// Register MMIO region
    pub fn register(&self, gpa: u64, size: u64, handler: Arc<dyn MmioHandler>) -> u32 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) as u32;
        let region = MmioRegion {
            gpa,
            size,
            handler_id: id,
        };

        self.regions.write().push((region, handler));
        id
    }

    /// Unregister MMIO region by ID
    pub fn unregister(&self, handler_id: u32) {
        let mut regions = self.regions.write();
        regions.retain(|(r, _)| r.handler_id != handler_id);
    }

    /// Handle MMIO access
    pub fn handle(&self, gpa: u64, size: u8, is_write: bool, value: u64) -> Option<u64> {
        let regions = self.regions.read();

        for (region, handler) in regions.iter() {
            if region.contains(gpa) {
                let offset = gpa - region.gpa;
                if is_write {
                    handler.write(offset, size, value);
                    return Some(0);
                } else {
                    return Some(handler.read(offset, size));
                }
            }
        }

        None
    }
}

// =============================================================================
// IOEVENTFD
// =============================================================================

/// I/O event file descriptor entry
#[derive(Clone)]
pub struct IoEventFd {
    /// Guest address (GPA or port)
    pub addr: u64,
    /// Size (0 for port, 1/2/4/8 for MMIO)
    pub size: u8,
    /// Is port I/O (false = MMIO)
    pub is_pio: bool,
    /// Data match (optional)
    pub datamatch: Option<u64>,
    /// Event FD
    pub fd: i32,
}

/// IoEventFd manager
pub struct IoEventFdManager {
    /// Registered ioeventfds
    entries: RwLock<Vec<IoEventFd>>,
}

impl IoEventFdManager {
    /// Create new manager
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(Vec::new()),
        }
    }

    /// Register ioeventfd
    pub fn register(&self, entry: IoEventFd) {
        self.entries.write().push(entry);
    }

    /// Unregister ioeventfd
    pub fn unregister(&self, addr: u64, size: u8, is_pio: bool) {
        let mut entries = self.entries.write();
        entries.retain(|e| !(e.addr == addr && e.size == size && e.is_pio == is_pio));
    }

    /// Check for matching ioeventfd and trigger
    pub fn check_and_trigger(&self, addr: u64, size: u8, is_pio: bool, value: u64) -> bool {
        let entries = self.entries.read();

        for entry in entries.iter() {
            if entry.addr == addr && entry.size == size && entry.is_pio == is_pio {
                // Check datamatch if present
                if let Some(expected) = entry.datamatch {
                    if value != expected {
                        continue;
                    }
                }

                // Would signal the eventfd
                // signal_eventfd(entry.fd);
                return true;
            }
        }

        false
    }
}

// =============================================================================
// COALESCED MMIO
// =============================================================================

/// Coalesced MMIO ring entry
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CoalescedMmioEntry {
    /// Physical address
    pub phys_addr: u64,
    /// Data
    pub data: u32,
    /// Length
    pub len: u32,
}

/// Coalesced MMIO zone
#[derive(Clone)]
pub struct CoalescedMmioZone {
    /// Address
    pub addr: u64,
    /// Size
    pub size: u64,
}

/// Coalesced MMIO ring
pub struct CoalescedMmioRing {
    /// Ring buffer
    entries: Mutex<Vec<CoalescedMmioEntry>>,
    /// Maximum entries
    max_entries: usize,
    /// Zones
    zones: RwLock<Vec<CoalescedMmioZone>>,
}

impl CoalescedMmioRing {
    /// Create new ring
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Mutex::new(Vec::with_capacity(max_entries)),
            max_entries,
            zones: RwLock::new(Vec::new()),
        }
    }

    /// Register zone
    pub fn register_zone(&self, zone: CoalescedMmioZone) {
        self.zones.write().push(zone);
    }

    /// Unregister zone
    pub fn unregister_zone(&self, addr: u64) {
        self.zones.write().retain(|z| z.addr != addr);
    }

    /// Check if address is in coalesced zone
    pub fn is_coalesced(&self, addr: u64) -> bool {
        let zones = self.zones.read();
        zones.iter().any(|z| addr >= z.addr && addr < z.addr + z.size)
    }

    /// Add entry to ring
    pub fn add_entry(&self, entry: CoalescedMmioEntry) -> bool {
        let mut entries = self.entries.lock();
        if entries.len() >= self.max_entries {
            return false;
        }
        entries.push(entry);
        true
    }

    /// Get and clear entries
    pub fn take_entries(&self) -> Vec<CoalescedMmioEntry> {
        let mut entries = self.entries.lock();
        core::mem::take(&mut *entries)
    }
}

// =============================================================================
// PCI PASSTHROUGH
// =============================================================================

/// PCI device for passthrough
#[derive(Debug, Clone)]
pub struct PassthroughDevice {
    /// Domain
    pub domain: u16,
    /// Bus
    pub bus: u8,
    /// Device
    pub device: u8,
    /// Function
    pub function: u8,
    /// Assigned to VM
    pub assigned: bool,
}

impl PassthroughDevice {
    /// Create new passthrough device
    pub fn new(domain: u16, bus: u8, device: u8, function: u8) -> Self {
        Self {
            domain,
            bus,
            device,
            function,
            assigned: false,
        }
    }

    /// Get BDF (Bus:Device.Function) string
    pub fn bdf(&self) -> (u8, u8, u8) {
        (self.bus, self.device, self.function)
    }
}

/// VFIO group
pub struct VfioGroup {
    /// Group ID
    id: u32,
    /// Devices in group
    devices: Vec<PassthroughDevice>,
    /// Container FD
    container_fd: i32,
    /// Group FD
    group_fd: i32,
}

impl VfioGroup {
    /// Create new VFIO group
    pub fn new(id: u32) -> Self {
        Self {
            id,
            devices: Vec::new(),
            container_fd: -1,
            group_fd: -1,
        }
    }

    /// Add device to group
    pub fn add_device(&mut self, device: PassthroughDevice) {
        self.devices.push(device);
    }

    /// Assign group to VM
    pub fn assign(&mut self) -> Result<(), KvmError> {
        for device in &mut self.devices {
            device.assigned = true;
        }
        Ok(())
    }

    /// Release group from VM
    pub fn release(&mut self) {
        for device in &mut self.devices {
            device.assigned = false;
        }
    }
}

// =============================================================================
// VIRTIO TRANSPORT
// =============================================================================

/// VirtIO transport type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioTransport {
    /// Memory-mapped
    Mmio,
    /// PCI
    Pci,
    /// Channel I/O
    Ccw,
}

/// VirtIO device types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum VirtioDeviceType {
    Net = 1,
    Block = 2,
    Console = 3,
    Entropy = 4,
    Balloon = 5,
    Scsi = 8,
    Gpu = 16,
    Input = 18,
    Socket = 19,
    Fs = 26,
}

/// VirtIO queue
pub struct VirtQueue {
    /// Queue index
    index: u16,
    /// Queue size (number of descriptors)
    size: u16,
    /// Descriptor table address
    desc_addr: u64,
    /// Available ring address
    avail_addr: u64,
    /// Used ring address
    used_addr: u64,
    /// Ready flag
    ready: bool,
    /// Event index
    event_idx: u16,
}

impl VirtQueue {
    /// Create new virtqueue
    pub fn new(index: u16, size: u16) -> Self {
        Self {
            index,
            size,
            desc_addr: 0,
            avail_addr: 0,
            used_addr: 0,
            ready: false,
            event_idx: 0,
        }
    }

    /// Set addresses
    pub fn set_addresses(&mut self, desc: u64, avail: u64, used: u64) {
        self.desc_addr = desc;
        self.avail_addr = avail;
        self.used_addr = used;
    }

    /// Set ready
    pub fn set_ready(&mut self, ready: bool) {
        self.ready = ready;
    }

    /// Is ready
    pub fn is_ready(&self) -> bool {
        self.ready
    }
}

/// VirtIO MMIO registers
pub mod virtio_mmio {
    pub const MAGIC_VALUE: u64 = 0x000;
    pub const VERSION: u64 = 0x004;
    pub const DEVICE_ID: u64 = 0x008;
    pub const VENDOR_ID: u64 = 0x00C;
    pub const DEVICE_FEATURES: u64 = 0x010;
    pub const DEVICE_FEATURES_SEL: u64 = 0x014;
    pub const DRIVER_FEATURES: u64 = 0x020;
    pub const DRIVER_FEATURES_SEL: u64 = 0x024;
    pub const QUEUE_SEL: u64 = 0x030;
    pub const QUEUE_NUM_MAX: u64 = 0x034;
    pub const QUEUE_NUM: u64 = 0x038;
    pub const QUEUE_READY: u64 = 0x044;
    pub const QUEUE_NOTIFY: u64 = 0x050;
    pub const INTERRUPT_STATUS: u64 = 0x060;
    pub const INTERRUPT_ACK: u64 = 0x064;
    pub const STATUS: u64 = 0x070;
    pub const QUEUE_DESC_LOW: u64 = 0x080;
    pub const QUEUE_DESC_HIGH: u64 = 0x084;
    pub const QUEUE_AVAIL_LOW: u64 = 0x090;
    pub const QUEUE_AVAIL_HIGH: u64 = 0x094;
    pub const QUEUE_USED_LOW: u64 = 0x0A0;
    pub const QUEUE_USED_HIGH: u64 = 0x0A4;
    pub const CONFIG_GENERATION: u64 = 0x0FC;
    pub const CONFIG: u64 = 0x100;

    pub const MAGIC: u32 = 0x74726976; // "virt"
}
