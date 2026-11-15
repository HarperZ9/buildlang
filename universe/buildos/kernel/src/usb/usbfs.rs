//! USB Filesystem Interface (usbfs)
//!
//! Provides userspace access to USB devices:
//! - /sys/bus/usb/devices/ device enumeration
//! - /dev/bus/usb/<bus>/<dev> raw access
//! - ioctl interface for device control
//! - Async URB submission

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use spin::{Mutex, RwLock};
use super::{
    UsbError, UsbDevice, UsbBus, SetupPacket,
    TransferDirection, TransferType,
    transfer::{Urb, UrbStatus},
};

/// USBFS ioctl commands
#[repr(u32)]
pub enum UsbfsIoctl {
    /// Get device descriptor
    GetDescriptor = 0x8038550C,
    /// Set configuration
    SetConfiguration = 0x80045505,
    /// Claim interface
    ClaimInterface = 0x8004550F,
    /// Release interface
    ReleaseInterface = 0x80045510,
    /// Set interface alternate setting
    SetInterface = 0x80085504,
    /// Clear halt on endpoint
    ClearHalt = 0x80045515,
    /// Reset device
    Reset = 0x00005514,
    /// Get driver name
    GetDriver = 0x8108551A,
    /// Disconnect driver
    DisconnectDriver = 0x00005516,
    /// Connect driver
    ConnectDriver = 0x00005517,
    /// Submit URB
    SubmitUrb = 0x8054550A,
    /// Reap URB (blocking)
    ReapUrb = 0x4008550C,
    /// Reap URB (non-blocking)
    ReapUrbNonblock = 0x4008550D,
    /// Discard URB
    DiscardUrb = 0x8008550B,
    /// Control transfer
    Control = 0xC0185500,
    /// Bulk transfer
    Bulk = 0xC0185502,
    /// Get capabilities
    GetCapabilities = 0x8004551A,
    /// Disconnect claim interface
    DisconnectClaim = 0x8108551B,
    /// Alloc streams
    AllocStreams = 0x8010551C,
    /// Free streams
    FreeStreams = 0x8010551D,
    /// Drop privileges
    DropPrivileges = 0x4004551E,
    /// Get speed
    GetSpeed = 0x8004551F,
}

/// USBFS control transfer structure
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct UsbfsCtrlTransfer {
    /// Request type
    pub request_type: u8,
    /// Request
    pub request: u8,
    /// Value
    pub value: u16,
    /// Index
    pub index: u16,
    /// Length
    pub length: u16,
    /// Timeout in milliseconds
    pub timeout: u32,
    /// Data pointer (userspace)
    pub data: u64,
}

/// USBFS bulk transfer structure
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct UsbfsBulkTransfer {
    /// Endpoint
    pub endpoint: u32,
    /// Length
    pub length: u32,
    /// Timeout in milliseconds
    pub timeout: u32,
    /// Data pointer (userspace)
    pub data: u64,
}

/// USBFS set interface structure
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct UsbfsSetInterface {
    /// Interface number
    pub interface: u32,
    /// Alternate setting
    pub alt_setting: u32,
}

/// USBFS URB structure
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct UsbfsUrb {
    /// URB type
    pub urb_type: u8,
    /// Endpoint
    pub endpoint: u8,
    /// Status
    pub status: i32,
    /// Flags
    pub flags: u32,
    /// Buffer pointer
    pub buffer: u64,
    /// Buffer length
    pub buffer_length: i32,
    /// Actual length
    pub actual_length: i32,
    /// Start frame (isochronous)
    pub start_frame: i32,
    /// Stream ID or number of packets
    pub stream_id_or_packets: u32,
    /// Error count
    pub error_count: i32,
    /// Signal number for completion
    pub signr: u32,
    /// User context
    pub usercontext: u64,
    // Isochronous packets follow (variable-length)
}

/// USBFS URB types
#[repr(u8)]
pub enum UsbfsUrbType {
    Iso = 0,
    Interrupt = 1,
    Control = 2,
    Bulk = 3,
}

/// USBFS isochronous packet
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct UsbfsIsoPacket {
    /// Length
    pub length: u32,
    /// Actual length
    pub actual_length: u32,
    /// Status
    pub status: u32,
}

/// USBFS disconnect claim structure
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct UsbfsDisconnectClaim {
    /// Interface number
    pub interface: u32,
    /// Flags
    pub flags: u32,
    /// Driver name
    pub driver: [u8; 256],
}

/// USBFS capabilities flags
pub mod caps {
    pub const ZERO_PACKET: u32 = 1 << 0;
    pub const BULK_CONTINUATION: u32 = 1 << 1;
    pub const NO_PACKET_SIZE_LIM: u32 = 1 << 2;
    pub const BULK_SCATTER_GATHER: u32 = 1 << 3;
    pub const REAP_AFTER_DISCONNECT: u32 = 1 << 4;
    pub const MMAP: u32 = 1 << 5;
    pub const DROP_PRIVILEGES: u32 = 1 << 6;
    pub const CONNINFO_EX: u32 = 1 << 7;
    pub const SUSPEND: u32 = 1 << 8;
}

/// USB device file handle
pub struct UsbDeviceHandle {
    /// Device
    device: Arc<RwLock<UsbDevice>>,
    /// Bus
    bus: Arc<RwLock<UsbBus>>,
    /// Claimed interfaces
    claimed_interfaces: Mutex<Vec<u8>>,
    /// Pending URBs
    pending_urbs: Mutex<BTreeMap<u64, Box<Urb>>>,
    /// Completed URBs
    completed_urbs: Mutex<Vec<Box<Urb>>>,
    /// Next URB ID
    next_urb_id: AtomicU64,
    /// Privileges dropped
    privileges_dropped: AtomicU32,
}

impl UsbDeviceHandle {
    /// Create new device handle
    pub fn new(device: Arc<RwLock<UsbDevice>>, bus: Arc<RwLock<UsbBus>>) -> Self {
        Self {
            device,
            bus,
            claimed_interfaces: Mutex::new(Vec::new()),
            pending_urbs: Mutex::new(BTreeMap::new()),
            completed_urbs: Mutex::new(Vec::new()),
            next_urb_id: AtomicU64::new(1),
            privileges_dropped: AtomicU32::new(0),
        }
    }

    /// Handle ioctl
    pub fn ioctl(&self, cmd: u32, arg: u64) -> Result<i32, UsbError> {
        match cmd {
            x if x == UsbfsIoctl::GetDescriptor as u32 => {
                self.get_descriptor(arg)
            }
            x if x == UsbfsIoctl::SetConfiguration as u32 => {
                self.set_configuration(arg as u8)
            }
            x if x == UsbfsIoctl::ClaimInterface as u32 => {
                self.claim_interface(arg as u8)
            }
            x if x == UsbfsIoctl::ReleaseInterface as u32 => {
                self.release_interface(arg as u8)
            }
            x if x == UsbfsIoctl::SetInterface as u32 => {
                self.set_interface(arg)
            }
            x if x == UsbfsIoctl::ClearHalt as u32 => {
                self.clear_halt(arg as u8)
            }
            x if x == UsbfsIoctl::Reset as u32 => {
                self.reset()
            }
            x if x == UsbfsIoctl::Control as u32 => {
                self.control_transfer(arg)
            }
            x if x == UsbfsIoctl::Bulk as u32 => {
                self.bulk_transfer(arg)
            }
            x if x == UsbfsIoctl::SubmitUrb as u32 => {
                self.submit_urb(arg)
            }
            x if x == UsbfsIoctl::ReapUrb as u32 => {
                self.reap_urb(true)
            }
            x if x == UsbfsIoctl::ReapUrbNonblock as u32 => {
                self.reap_urb(false)
            }
            x if x == UsbfsIoctl::DiscardUrb as u32 => {
                self.discard_urb(arg)
            }
            x if x == UsbfsIoctl::GetCapabilities as u32 => {
                self.get_capabilities()
            }
            x if x == UsbfsIoctl::GetSpeed as u32 => {
                self.get_speed()
            }
            x if x == UsbfsIoctl::DropPrivileges as u32 => {
                self.drop_privileges(arg as u32)
            }
            _ => Err(UsbError::NotSupported),
        }
    }

    /// Get descriptor
    fn get_descriptor(&self, _arg: u64) -> Result<i32, UsbError> {
        // Would copy descriptor to userspace
        Ok(0)
    }

    /// Set configuration
    fn set_configuration(&self, config: u8) -> Result<i32, UsbError> {
        let setup = SetupPacket::set_configuration(config);
        super::control_transfer(&self.device, &setup, None)?;
        Ok(0)
    }

    /// Claim interface
    fn claim_interface(&self, interface: u8) -> Result<i32, UsbError> {
        let mut claimed = self.claimed_interfaces.lock();
        if claimed.contains(&interface) {
            return Err(UsbError::Busy);
        }
        claimed.push(interface);
        Ok(0)
    }

    /// Release interface
    fn release_interface(&self, interface: u8) -> Result<i32, UsbError> {
        let mut claimed = self.claimed_interfaces.lock();
        if let Some(pos) = claimed.iter().position(|&i| i == interface) {
            claimed.remove(pos);
            Ok(0)
        } else {
            Err(UsbError::InvalidArgument)
        }
    }

    /// Set interface alternate setting
    fn set_interface(&self, arg: u64) -> Result<i32, UsbError> {
        let ptr = arg as *const UsbfsSetInterface;
        let set_iface = unsafe { &*ptr };

        let setup = SetupPacket {
            request_type: 0x01, // Host to device, standard, interface
            request: 11,       // SET_INTERFACE
            value: set_iface.alt_setting as u16,
            index: set_iface.interface as u16,
            length: 0,
        };

        super::control_transfer(&self.device, &setup, None)?;
        Ok(0)
    }

    /// Clear halt on endpoint
    fn clear_halt(&self, endpoint: u8) -> Result<i32, UsbError> {
        let setup = SetupPacket::clear_feature(
            super::RequestRecipient::Endpoint,
            0, // ENDPOINT_HALT feature
            endpoint as u16,
        );

        super::control_transfer(&self.device, &setup, None)?;
        Ok(0)
    }

    /// Reset device
    fn reset(&self) -> Result<i32, UsbError> {
        // Would perform port reset
        Ok(0)
    }

    /// Control transfer
    fn control_transfer(&self, arg: u64) -> Result<i32, UsbError> {
        let ptr = arg as *const UsbfsCtrlTransfer;
        let ctrl = unsafe { &*ptr };

        let setup = SetupPacket {
            request_type: ctrl.request_type,
            request: ctrl.request,
            value: ctrl.value,
            index: ctrl.index,
            length: ctrl.length,
        };

        let mut buffer = if ctrl.length > 0 {
            alloc::vec![0u8; ctrl.length as usize]
        } else {
            Vec::new()
        };

        // Copy data from userspace for OUT transfers
        if ctrl.request_type & 0x80 == 0 && ctrl.length > 0 {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    ctrl.data as *const u8,
                    buffer.as_mut_ptr(),
                    ctrl.length as usize,
                );
            }
        }

        let result = super::control_transfer(
            &self.device,
            &setup,
            if ctrl.length > 0 { Some(&mut buffer) } else { None },
        )?;

        // Copy data to userspace for IN transfers
        if ctrl.request_type & 0x80 != 0 && result > 0 {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    buffer.as_ptr(),
                    ctrl.data as *mut u8,
                    result,
                );
            }
        }

        Ok(result as i32)
    }

    /// Bulk transfer
    fn bulk_transfer(&self, arg: u64) -> Result<i32, UsbError> {
        let ptr = arg as *const UsbfsBulkTransfer;
        let bulk = unsafe { &*ptr };

        let endpoint = bulk.endpoint as u8;
        let direction = if endpoint & 0x80 != 0 {
            TransferDirection::In
        } else {
            TransferDirection::Out
        };

        let mut buffer = alloc::vec![0u8; bulk.length as usize];

        // Copy data from userspace for OUT transfers
        if direction == TransferDirection::Out {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    bulk.data as *const u8,
                    buffer.as_mut_ptr(),
                    bulk.length as usize,
                );
            }
        }

        let result = super::bulk_transfer(
            &self.device,
            endpoint,
            &mut buffer,
            direction,
        )?;

        // Copy data to userspace for IN transfers
        if direction == TransferDirection::In && result > 0 {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    buffer.as_ptr(),
                    bulk.data as *mut u8,
                    result,
                );
            }
        }

        Ok(result as i32)
    }

    /// Submit URB
    fn submit_urb(&self, arg: u64) -> Result<i32, UsbError> {
        let ptr = arg as *const UsbfsUrb;
        let usbfs_urb = unsafe { &*ptr };

        let urb_type = match usbfs_urb.urb_type {
            0 => TransferType::Isochronous,
            1 => TransferType::Interrupt,
            2 => TransferType::Control,
            3 => TransferType::Bulk,
            _ => return Err(UsbError::InvalidArgument),
        };

        let direction = if usbfs_urb.endpoint & 0x80 != 0 {
            TransferDirection::In
        } else {
            TransferDirection::Out
        };

        let urb_id = self.next_urb_id.fetch_add(1, Ordering::SeqCst);

        let mut buffer = alloc::vec![0u8; usbfs_urb.buffer_length as usize];

        // Copy data for OUT transfers
        if direction == TransferDirection::Out && usbfs_urb.buffer_length > 0 {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    usbfs_urb.buffer as *const u8,
                    buffer.as_mut_ptr(),
                    usbfs_urb.buffer_length as usize,
                );
            }
        }

        let mut urb = Box::new(Urb::new());
        urb.id = urb_id;
        urb.device_address = self.device.read().address;
        urb.endpoint = usbfs_urb.endpoint;
        urb.transfer_type = urb_type;
        urb.direction = direction;
        urb.buffer = buffer;
        urb.context = usbfs_urb.usercontext;

        self.pending_urbs.lock().insert(urb_id, urb);

        // Would queue URB to host controller

        Ok(0)
    }

    /// Reap URB
    fn reap_urb(&self, blocking: bool) -> Result<i32, UsbError> {
        loop {
            let mut completed = self.completed_urbs.lock();
            if let Some(_urb) = completed.pop() {
                // Would copy URB back to userspace
                return Ok(0);
            }
            drop(completed);

            if !blocking {
                return Err(UsbError::Timeout);
            }

            // Wait for completion
            core::hint::spin_loop();
        }
    }

    /// Discard URB
    fn discard_urb(&self, arg: u64) -> Result<i32, UsbError> {
        let urb_id = arg;

        let mut pending = self.pending_urbs.lock();
        if let Some(mut urb) = pending.remove(&urb_id) {
            urb.status = UrbStatus::Cancelled;
            self.completed_urbs.lock().push(urb);
            Ok(0)
        } else {
            Err(UsbError::InvalidArgument)
        }
    }

    /// Get capabilities
    fn get_capabilities(&self) -> Result<i32, UsbError> {
        let caps = caps::ZERO_PACKET |
                   caps::BULK_CONTINUATION |
                   caps::NO_PACKET_SIZE_LIM |
                   caps::DROP_PRIVILEGES |
                   caps::CONNINFO_EX;

        Ok(caps as i32)
    }

    /// Get speed
    fn get_speed(&self) -> Result<i32, UsbError> {
        let speed = self.device.read().speed;
        let speed_val = match speed {
            super::UsbSpeed::Low => 1,
            super::UsbSpeed::Full => 2,
            super::UsbSpeed::High => 3,
            super::UsbSpeed::Super => 4,
            super::UsbSpeed::SuperPlus => 5,
        };
        Ok(speed_val)
    }

    /// Drop privileges
    fn drop_privileges(&self, mask: u32) -> Result<i32, UsbError> {
        self.privileges_dropped.fetch_or(mask, Ordering::SeqCst);
        Ok(0)
    }

    /// Release all resources
    pub fn release(&self) {
        // Release all claimed interfaces
        self.claimed_interfaces.lock().clear();

        // Cancel all pending URBs
        let mut pending = self.pending_urbs.lock();
        let urbs: alloc::vec::Vec<_> = pending.keys().copied().collect();
        for id in urbs {
            if let Some(mut urb) = pending.remove(&id) {
                urb.status = UrbStatus::Cancelled;
            }
        }
    }
}

/// USB filesystem
pub struct UsbFs {
    /// Open handles
    handles: RwLock<BTreeMap<u64, Arc<UsbDeviceHandle>>>,
    /// Next handle ID
    next_handle: AtomicU64,
}

impl UsbFs {
    /// Create new USB filesystem
    pub const fn new() -> Self {
        Self {
            handles: RwLock::new(BTreeMap::new()),
            next_handle: AtomicU64::new(1),
        }
    }

    /// Open USB device
    pub fn open(&self, bus_id: u32, device_addr: u8) -> Result<u64, UsbError> {
        let bus = super::get_bus(bus_id).ok_or(UsbError::DeviceNotFound)?;
        let bus_ref = bus.read();
        let device = bus_ref.get_device(device_addr).ok_or(UsbError::DeviceNotFound)?;
        drop(bus_ref);

        let handle_id = self.next_handle.fetch_add(1, Ordering::SeqCst);
        let handle = Arc::new(UsbDeviceHandle::new(device, bus));

        self.handles.write().insert(handle_id, handle);

        Ok(handle_id)
    }

    /// Close USB device
    pub fn close(&self, handle_id: u64) -> Result<(), UsbError> {
        let handle = self.handles.write().remove(&handle_id)
            .ok_or(UsbError::InvalidArgument)?;

        handle.release();
        Ok(())
    }

    /// Perform ioctl on USB device
    pub fn ioctl(&self, handle_id: u64, cmd: u32, arg: u64) -> Result<i32, UsbError> {
        let handles = self.handles.read();
        let handle = handles.get(&handle_id).ok_or(UsbError::InvalidArgument)?;
        handle.ioctl(cmd, arg)
    }

    /// Read from USB device (not typically used directly)
    pub fn read(&self, _handle_id: u64, _buffer: &mut [u8]) -> Result<usize, UsbError> {
        Err(UsbError::NotSupported)
    }

    /// Write to USB device (not typically used directly)
    pub fn write(&self, _handle_id: u64, _buffer: &[u8]) -> Result<usize, UsbError> {
        Err(UsbError::NotSupported)
    }
}

/// Global USB filesystem
static USBFS: UsbFs = UsbFs::new();

/// Open USB device
pub fn open(bus_id: u32, device_addr: u8) -> Result<u64, UsbError> {
    USBFS.open(bus_id, device_addr)
}

/// Close USB device
pub fn close(handle_id: u64) -> Result<(), UsbError> {
    USBFS.close(handle_id)
}

/// Perform ioctl
pub fn ioctl(handle_id: u64, cmd: u32, arg: u64) -> Result<i32, UsbError> {
    USBFS.ioctl(handle_id, cmd, arg)
}

/// Generate sysfs entry for device
pub fn sysfs_entry(device: &UsbDevice) -> String {
    alloc::format!(
        "{}-{}",
        device.bus_id,
        device.port
    )
}

/// Generate device node path
pub fn devnode_path(bus_id: u32, device_addr: u8) -> String {
    alloc::format!("/dev/bus/usb/{:03}/{:03}", bus_id, device_addr)
}
