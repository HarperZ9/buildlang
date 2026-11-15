//! USB Subsystem
//!
//! Provides USB host controller and device management:
//! - Host controller drivers (UHCI, OHCI, EHCI, xHCI)
//! - Device enumeration and configuration
//! - USB Request Blocks (URBs)
//! - Class drivers (HID, Storage, etc.)
//! - Hub support
//! - Power management

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::RwLock;

pub mod host;
pub mod device;
pub mod hub;
pub mod transfer;
pub mod hid;
pub mod storage;
pub mod usbfs;

pub use device::{UsbDevice, UsbDeviceDescriptor, UsbConfigDescriptor, UsbInterfaceDescriptor};
pub use transfer::{Urb, UrbStatus, TransferType, TransferDirection};
pub use host::{UsbHc, HcType};

/// USB error types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsbError {
    /// Device not found
    DeviceNotFound,
    /// No such endpoint
    NoEndpoint,
    /// Transfer failed
    TransferFailed,
    /// Timeout
    Timeout,
    /// Stall
    Stall,
    /// Babble (data overrun)
    Babble,
    /// CRC error
    CrcError,
    /// Bit stuffing error
    BitStuffing,
    /// Data toggle mismatch
    DataToggle,
    /// Buffer overrun
    BufferOverrun,
    /// Buffer underrun
    BufferUnderrun,
    /// Not accessed
    NotAccessed,
    /// Host controller error
    HostControllerError,
    /// Invalid descriptor
    InvalidDescriptor,
    /// No memory
    NoMemory,
    /// Device removed
    DeviceRemoved,
    /// Invalid state
    InvalidState,
    /// Busy
    Busy,
    /// No bandwidth
    NoBandwidth,
    /// Short packet
    ShortPacket,
    /// Invalid argument
    InvalidArgument,
    /// Not supported
    NotSupported,
    /// Protocol error
    ProtocolError,
    /// Power error
    PowerError,
}

/// USB speed
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsbSpeed {
    /// Low speed (1.5 Mbps)
    Low,
    /// Full speed (12 Mbps)
    Full,
    /// High speed (480 Mbps)
    High,
    /// Super speed (5 Gbps)
    Super,
    /// Super speed plus (10+ Gbps)
    SuperPlus,
}

impl UsbSpeed {
    /// Get speed in Mbps
    pub fn mbps(&self) -> u32 {
        match self {
            Self::Low => 1,
            Self::Full => 12,
            Self::High => 480,
            Self::Super => 5000,
            Self::SuperPlus => 10000,
        }
    }

    /// Get maximum packet size for control endpoint
    pub fn max_control_packet(&self) -> u16 {
        match self {
            Self::Low => 8,
            Self::Full => 64,
            Self::High => 64,
            Self::Super | Self::SuperPlus => 512,
        }
    }
}

/// USB device state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceState {
    /// Not attached
    NotAttached,
    /// Attached, awaiting reset
    Attached,
    /// Powered
    Powered,
    /// Default (addressed 0)
    Default,
    /// Address assigned
    Address,
    /// Configured
    Configured,
    /// Suspended
    Suspended,
}

/// USB class codes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsbClass {
    /// Use interface descriptors
    PerInterface = 0x00,
    /// Audio
    Audio = 0x01,
    /// Communications and CDC Control
    Comm = 0x02,
    /// HID (Human Interface Device)
    Hid = 0x03,
    /// Physical
    Physical = 0x05,
    /// Image
    Image = 0x06,
    /// Printer
    Printer = 0x07,
    /// Mass Storage
    MassStorage = 0x08,
    /// Hub
    Hub = 0x09,
    /// CDC-Data
    CdcData = 0x0A,
    /// Smart Card
    SmartCard = 0x0B,
    /// Content Security
    ContentSecurity = 0x0D,
    /// Video
    Video = 0x0E,
    /// Personal Healthcare
    PersonalHealthcare = 0x0F,
    /// Audio/Video Devices
    AudioVideo = 0x10,
    /// Billboard Device
    Billboard = 0x11,
    /// USB Type-C Bridge
    TypeCBridge = 0x12,
    /// Diagnostic Device
    Diagnostic = 0xDC,
    /// Wireless Controller
    Wireless = 0xE0,
    /// Miscellaneous
    Misc = 0xEF,
    /// Application Specific
    ApplicationSpecific = 0xFE,
    /// Vendor Specific
    VendorSpecific = 0xFF,
}

impl From<u8> for UsbClass {
    fn from(val: u8) -> Self {
        match val {
            0x00 => Self::PerInterface,
            0x01 => Self::Audio,
            0x02 => Self::Comm,
            0x03 => Self::Hid,
            0x05 => Self::Physical,
            0x06 => Self::Image,
            0x07 => Self::Printer,
            0x08 => Self::MassStorage,
            0x09 => Self::Hub,
            0x0A => Self::CdcData,
            0x0B => Self::SmartCard,
            0x0D => Self::ContentSecurity,
            0x0E => Self::Video,
            0x0F => Self::PersonalHealthcare,
            0x10 => Self::AudioVideo,
            0x11 => Self::Billboard,
            0x12 => Self::TypeCBridge,
            0xDC => Self::Diagnostic,
            0xE0 => Self::Wireless,
            0xEF => Self::Misc,
            0xFE => Self::ApplicationSpecific,
            0xFF => Self::VendorSpecific,
            _ => Self::VendorSpecific,
        }
    }
}

/// USB request type
#[derive(Clone, Copy, Debug)]
pub struct UsbRequestType {
    /// Direction (0 = host to device, 1 = device to host)
    pub direction: TransferDirection,
    /// Type (0 = standard, 1 = class, 2 = vendor)
    pub request_type: RequestType,
    /// Recipient (0 = device, 1 = interface, 2 = endpoint, 3 = other)
    pub recipient: RequestRecipient,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestType {
    Standard = 0,
    Class = 1,
    Vendor = 2,
    Reserved = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestRecipient {
    Device = 0,
    Interface = 1,
    Endpoint = 2,
    Other = 3,
}

impl UsbRequestType {
    /// Create from byte
    pub fn from_byte(byte: u8) -> Self {
        Self {
            direction: if byte & 0x80 != 0 {
                TransferDirection::In
            } else {
                TransferDirection::Out
            },
            request_type: match (byte >> 5) & 0x03 {
                0 => RequestType::Standard,
                1 => RequestType::Class,
                2 => RequestType::Vendor,
                _ => RequestType::Reserved,
            },
            recipient: match byte & 0x1F {
                0 => RequestRecipient::Device,
                1 => RequestRecipient::Interface,
                2 => RequestRecipient::Endpoint,
                _ => RequestRecipient::Other,
            },
        }
    }

    /// Convert to byte
    pub fn to_byte(&self) -> u8 {
        let mut byte = 0u8;
        if self.direction == TransferDirection::In {
            byte |= 0x80;
        }
        byte |= (self.request_type as u8) << 5;
        byte |= self.recipient as u8;
        byte
    }
}

/// Standard USB requests
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum StandardRequest {
    GetStatus = 0,
    ClearFeature = 1,
    SetFeature = 3,
    SetAddress = 5,
    GetDescriptor = 6,
    SetDescriptor = 7,
    GetConfiguration = 8,
    SetConfiguration = 9,
    GetInterface = 10,
    SetInterface = 11,
    SynchFrame = 12,
}

/// Descriptor types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DescriptorType {
    Device = 1,
    Configuration = 2,
    String = 3,
    Interface = 4,
    Endpoint = 5,
    DeviceQualifier = 6,
    OtherSpeedConfiguration = 7,
    InterfacePower = 8,
    Otg = 9,
    Debug = 10,
    InterfaceAssociation = 11,
    Bos = 15,
    DeviceCapability = 16,
    HidReport = 0x22,
    HidPhysical = 0x23,
    Hub = 0x29,
    SuperSpeedHub = 0x2A,
    SsEndpointCompanion = 48,
}

/// Setup packet for control transfers
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct SetupPacket {
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
}

impl SetupPacket {
    /// Create GET_DESCRIPTOR request
    pub fn get_descriptor(desc_type: DescriptorType, index: u8, length: u16) -> Self {
        Self {
            request_type: 0x80, // Device to host, standard, device
            request: StandardRequest::GetDescriptor as u8,
            value: ((desc_type as u16) << 8) | (index as u16),
            index: 0,
            length,
        }
    }

    /// Create SET_ADDRESS request
    pub fn set_address(address: u8) -> Self {
        Self {
            request_type: 0x00, // Host to device, standard, device
            request: StandardRequest::SetAddress as u8,
            value: address as u16,
            index: 0,
            length: 0,
        }
    }

    /// Create SET_CONFIGURATION request
    pub fn set_configuration(config: u8) -> Self {
        Self {
            request_type: 0x00,
            request: StandardRequest::SetConfiguration as u8,
            value: config as u16,
            index: 0,
            length: 0,
        }
    }

    /// Create GET_STATUS request
    pub fn get_status(recipient: RequestRecipient) -> Self {
        Self {
            request_type: 0x80 | (recipient as u8),
            request: StandardRequest::GetStatus as u8,
            value: 0,
            index: 0,
            length: 2,
        }
    }

    /// Create CLEAR_FEATURE request
    pub fn clear_feature(recipient: RequestRecipient, feature: u16, index: u16) -> Self {
        Self {
            request_type: recipient as u8,
            request: StandardRequest::ClearFeature as u8,
            value: feature,
            index,
            length: 0,
        }
    }

    /// Create SET_FEATURE request
    pub fn set_feature(recipient: RequestRecipient, feature: u16, index: u16) -> Self {
        Self {
            request_type: recipient as u8,
            request: StandardRequest::SetFeature as u8,
            value: feature,
            index,
            length: 0,
        }
    }
}

/// USB bus
pub struct UsbBus {
    /// Bus ID
    id: u32,
    /// Host controller
    hc: Option<Arc<dyn UsbHc>>,
    /// Root hub
    root_hub: Option<u32>,
    /// Devices on this bus
    devices: RwLock<BTreeMap<u8, Arc<RwLock<UsbDevice>>>>,
    /// Next device address
    next_address: AtomicU32,
    /// Bus bandwidth used (microseconds per frame)
    bandwidth_used: AtomicU32,
}

impl UsbBus {
    /// Create new USB bus
    pub fn new(id: u32) -> Self {
        Self {
            id,
            hc: None,
            root_hub: None,
            devices: RwLock::new(BTreeMap::new()),
            next_address: AtomicU32::new(1),
            bandwidth_used: AtomicU32::new(0),
        }
    }

    /// Set host controller
    pub fn set_hc(&mut self, hc: Arc<dyn UsbHc>) {
        self.hc = Some(hc);
    }

    /// Allocate device address
    pub fn allocate_address(&self) -> Result<u8, UsbError> {
        let addr = self.next_address.fetch_add(1, Ordering::SeqCst);
        if addr > 127 {
            self.next_address.store(1, Ordering::SeqCst);
            return Err(UsbError::NoMemory);
        }
        Ok(addr as u8)
    }

    /// Register device
    pub fn register_device(&self, address: u8, device: Arc<RwLock<UsbDevice>>) {
        self.devices.write().insert(address, device);
    }

    /// Unregister device
    pub fn unregister_device(&self, address: u8) {
        self.devices.write().remove(&address);
    }

    /// Get device by address
    pub fn get_device(&self, address: u8) -> Option<Arc<RwLock<UsbDevice>>> {
        self.devices.read().get(&address).cloned()
    }

    /// Get all devices
    pub fn get_devices(&self) -> Vec<(u8, Arc<RwLock<UsbDevice>>)> {
        self.devices.read()
            .iter()
            .map(|(addr, dev)| (*addr, dev.clone()))
            .collect()
    }
}

/// USB subsystem
pub struct UsbSubsystem {
    /// Is initialized
    initialized: AtomicBool,
    /// USB buses
    buses: RwLock<BTreeMap<u32, Arc<RwLock<UsbBus>>>>,
    /// Next bus ID
    next_bus_id: AtomicU32,
    /// USB drivers
    drivers: RwLock<Vec<Arc<dyn UsbDriver>>>,
    /// Statistics
    stats: UsbStats,
}

/// USB driver trait
pub trait UsbDriver: Send + Sync {
    /// Driver name
    fn name(&self) -> &str;

    /// Probe device
    fn probe(&self, device: &UsbDevice) -> bool;

    /// Attach to device
    fn attach(&self, device: Arc<RwLock<UsbDevice>>) -> Result<(), UsbError>;

    /// Detach from device
    fn detach(&self, device: &UsbDevice);
}

/// USB statistics
#[derive(Debug, Default)]
struct UsbStats {
    /// Total transfers completed
    transfers_completed: AtomicU64,
    /// Total transfers failed
    transfers_failed: AtomicU64,
    /// Total bytes transferred
    bytes_transferred: AtomicU64,
    /// Control transfers
    control_transfers: AtomicU64,
    /// Bulk transfers
    bulk_transfers: AtomicU64,
    /// Interrupt transfers
    interrupt_transfers: AtomicU64,
    /// Isochronous transfers
    iso_transfers: AtomicU64,
    /// Devices enumerated
    devices_enumerated: AtomicU64,
    /// Devices removed
    devices_removed: AtomicU64,
}

impl UsbSubsystem {
    /// Create new USB subsystem
    pub const fn new() -> Self {
        Self {
            initialized: AtomicBool::new(false),
            buses: RwLock::new(BTreeMap::new()),
            next_bus_id: AtomicU32::new(1),
            drivers: RwLock::new(Vec::new()),
            stats: UsbStats {
                transfers_completed: AtomicU64::new(0),
                transfers_failed: AtomicU64::new(0),
                bytes_transferred: AtomicU64::new(0),
                control_transfers: AtomicU64::new(0),
                bulk_transfers: AtomicU64::new(0),
                interrupt_transfers: AtomicU64::new(0),
                iso_transfers: AtomicU64::new(0),
                devices_enumerated: AtomicU64::new(0),
                devices_removed: AtomicU64::new(0),
            },
        }
    }

    /// Initialize USB subsystem
    pub fn init(&self) -> Result<(), UsbError> {
        // Probe for USB host controllers
        self.probe_controllers();

        // Register built-in drivers
        self.register_builtin_drivers();

        self.initialized.store(true, Ordering::Release);

        let buses = self.buses.read();
        crate::kprintln!("[USB] USB subsystem initialized, {} bus(es)", buses.len());

        Ok(())
    }

    /// Probe for USB host controllers
    fn probe_controllers(&self) {
        // Probe PCI for USB controllers
        let pci_devices = crate::drivers::pci::get_devices();

        for (bus, slot, func, _vendor, _device, class, subclass, prog_if) in pci_devices {
            // USB controller class = 0x0C, subclass = 0x03
            if class == 0x0C && subclass == 0x03 {
                let hc_type = match prog_if {
                    0x00 => HcType::Uhci,
                    0x10 => HcType::Ohci,
                    0x20 => HcType::Ehci,
                    0x30 => HcType::Xhci,
                    _ => continue,
                };

                crate::kprintln!("[USB] Found {:?} controller at {:02x}:{:02x}.{}",
                    hc_type, bus, slot, func);

                // Initialize host controller
                if let Some(hc) = host::init_controller(hc_type, bus, slot, func) {
                    let bus_id = self.next_bus_id.fetch_add(1, Ordering::SeqCst);
                    let mut usb_bus = UsbBus::new(bus_id);
                    usb_bus.set_hc(hc);
                    self.buses.write().insert(bus_id, Arc::new(RwLock::new(usb_bus)));
                }
            }
        }
    }

    /// Register built-in USB drivers
    fn register_builtin_drivers(&self) {
        // Register HID driver
        self.register_driver(Arc::new(hid::HidDriver::new()));

        // Register mass storage driver
        self.register_driver(Arc::new(storage::MassStorageDriver::new()));

        // Register hub driver
        self.register_driver(Arc::new(hub::HubDriver::new()));
    }

    /// Register USB driver
    pub fn register_driver(&self, driver: Arc<dyn UsbDriver>) {
        crate::kprintln!("[USB] Registered driver: {}", driver.name());
        self.drivers.write().push(driver);
    }

    /// Unregister USB driver
    pub fn unregister_driver(&self, name: &str) {
        self.drivers.write().retain(|d| d.name() != name);
    }

    /// Get bus by ID
    pub fn get_bus(&self, bus_id: u32) -> Option<Arc<RwLock<UsbBus>>> {
        self.buses.read().get(&bus_id).cloned()
    }

    /// Get all buses
    pub fn get_buses(&self) -> Vec<(u32, Arc<RwLock<UsbBus>>)> {
        self.buses.read()
            .iter()
            .map(|(id, bus)| (*id, bus.clone()))
            .collect()
    }

    /// Enumerate devices on a bus
    pub fn enumerate_bus(&self, bus_id: u32) -> Result<(), UsbError> {
        let bus = self.get_bus(bus_id).ok_or(UsbError::DeviceNotFound)?;
        let bus = bus.read();

        if let Some(ref hc) = bus.hc {
            // Get root hub ports
            let port_count = hc.port_count();

            for port in 0..port_count {
                if let Some(device) = self.enumerate_port(&bus, hc.clone(), port)? {
                    // Find matching driver
                    self.attach_driver(device)?;
                }
            }
        }

        Ok(())
    }

    /// Enumerate a single port
    fn enumerate_port(
        &self,
        bus: &UsbBus,
        hc: Arc<dyn UsbHc>,
        port: u32,
    ) -> Result<Option<Arc<RwLock<UsbDevice>>>, UsbError> {
        // Check if device is connected
        if !hc.port_connected(port) {
            return Ok(None);
        }

        // Reset port
        hc.port_reset(port)?;

        // Get device speed
        let speed = hc.port_speed(port);

        // Allocate address
        let address = bus.allocate_address()?;

        // Create device
        let device = Arc::new(RwLock::new(UsbDevice::new(address, speed, bus.id, port)));

        // Set address
        self.set_device_address(&device, address)?;

        // Get device descriptor
        self.get_device_descriptor(&device)?;

        // Get configuration descriptors
        self.get_configuration_descriptors(&device)?;

        // Set configuration
        self.set_configuration(&device, 1)?;

        self.stats.devices_enumerated.fetch_add(1, Ordering::Relaxed);

        Ok(Some(device))
    }

    /// Set device address
    fn set_device_address(
        &self,
        device: &Arc<RwLock<UsbDevice>>,
        address: u8,
    ) -> Result<(), UsbError> {
        let setup = SetupPacket::set_address(address);
        self.control_transfer(device, &setup, None)?;

        device.write().address = address;
        device.write().state = DeviceState::Address;

        Ok(())
    }

    /// Get device descriptor
    fn get_device_descriptor(&self, device: &Arc<RwLock<UsbDevice>>) -> Result<(), UsbError> {
        let setup = SetupPacket::get_descriptor(DescriptorType::Device, 0, 18);
        let mut buffer = [0u8; 18];

        self.control_transfer(device, &setup, Some(&mut buffer))?;

        // Parse descriptor
        let descriptor = UsbDeviceDescriptor::from_bytes(&buffer)?;
        device.write().descriptor = Some(descriptor);

        Ok(())
    }

    /// Get configuration descriptors
    fn get_configuration_descriptors(
        &self,
        device: &Arc<RwLock<UsbDevice>>,
    ) -> Result<(), UsbError> {
        let dev = device.read();
        let num_configs = dev.descriptor.as_ref().map(|d| d.num_configurations).unwrap_or(0);
        drop(dev);

        for i in 0..num_configs {
            // First get just the config descriptor header
            let setup = SetupPacket::get_descriptor(DescriptorType::Configuration, i, 9);
            let mut header = [0u8; 9];
            self.control_transfer(device, &setup, Some(&mut header))?;

            // Get total length
            let total_length = u16::from_le_bytes([header[2], header[3]]) as usize;

            // Now get the full configuration
            let setup = SetupPacket::get_descriptor(DescriptorType::Configuration, i, total_length as u16);
            let mut buffer = alloc::vec![0u8; total_length];
            self.control_transfer(device, &setup, Some(&mut buffer))?;

            // Parse configuration
            let config = UsbConfigDescriptor::from_bytes(&buffer)?;
            device.write().configurations.push(config);
        }

        Ok(())
    }

    /// Set configuration
    fn set_configuration(&self, device: &Arc<RwLock<UsbDevice>>, config: u8) -> Result<(), UsbError> {
        let setup = SetupPacket::set_configuration(config);
        self.control_transfer(device, &setup, None)?;

        device.write().current_config = Some(config);
        device.write().state = DeviceState::Configured;

        Ok(())
    }

    /// Perform control transfer
    pub fn control_transfer(
        &self,
        device: &Arc<RwLock<UsbDevice>>,
        setup: &SetupPacket,
        data: Option<&mut [u8]>,
    ) -> Result<usize, UsbError> {
        let dev = device.read();
        let bus = self.get_bus(dev.bus_id).ok_or(UsbError::DeviceNotFound)?;
        let bus = bus.read();

        if let Some(ref hc) = bus.hc {
            let result = hc.control_transfer(dev.address, setup, data);
            drop(dev);
            drop(bus);

            self.stats.control_transfers.fetch_add(1, Ordering::Relaxed);

            if result.is_ok() {
                self.stats.transfers_completed.fetch_add(1, Ordering::Relaxed);
            } else {
                self.stats.transfers_failed.fetch_add(1, Ordering::Relaxed);
            }

            result
        } else {
            Err(UsbError::HostControllerError)
        }
    }

    /// Perform bulk transfer
    pub fn bulk_transfer(
        &self,
        device: &Arc<RwLock<UsbDevice>>,
        endpoint: u8,
        data: &mut [u8],
        direction: TransferDirection,
    ) -> Result<usize, UsbError> {
        let dev = device.read();
        let bus = self.get_bus(dev.bus_id).ok_or(UsbError::DeviceNotFound)?;
        let bus = bus.read();

        if let Some(ref hc) = bus.hc {
            let result = hc.bulk_transfer(dev.address, endpoint, data, direction);
            drop(dev);
            drop(bus);

            self.stats.bulk_transfers.fetch_add(1, Ordering::Relaxed);

            if let Ok(len) = result {
                self.stats.transfers_completed.fetch_add(1, Ordering::Relaxed);
                self.stats.bytes_transferred.fetch_add(len as u64, Ordering::Relaxed);
            } else {
                self.stats.transfers_failed.fetch_add(1, Ordering::Relaxed);
            }

            result
        } else {
            Err(UsbError::HostControllerError)
        }
    }

    /// Perform interrupt transfer
    pub fn interrupt_transfer(
        &self,
        device: &Arc<RwLock<UsbDevice>>,
        endpoint: u8,
        data: &mut [u8],
        direction: TransferDirection,
    ) -> Result<usize, UsbError> {
        let dev = device.read();
        let bus = self.get_bus(dev.bus_id).ok_or(UsbError::DeviceNotFound)?;
        let bus = bus.read();

        if let Some(ref hc) = bus.hc {
            let result = hc.interrupt_transfer(dev.address, endpoint, data, direction);
            drop(dev);
            drop(bus);

            self.stats.interrupt_transfers.fetch_add(1, Ordering::Relaxed);

            if let Ok(len) = result {
                self.stats.transfers_completed.fetch_add(1, Ordering::Relaxed);
                self.stats.bytes_transferred.fetch_add(len as u64, Ordering::Relaxed);
            } else {
                self.stats.transfers_failed.fetch_add(1, Ordering::Relaxed);
            }

            result
        } else {
            Err(UsbError::HostControllerError)
        }
    }

    /// Attach driver to device
    fn attach_driver(&self, device: Arc<RwLock<UsbDevice>>) -> Result<(), UsbError> {
        let drivers = self.drivers.read();

        for driver in drivers.iter() {
            let dev = device.read();
            if driver.probe(&dev) {
                drop(dev);
                crate::kprintln!("[USB] Attaching driver '{}' to device", driver.name());
                driver.attach(device.clone())?;
                break;
            }
        }

        Ok(())
    }

    /// Handle device disconnect
    pub fn handle_disconnect(&self, bus_id: u32, port: u32) {
        if let Some(bus) = self.get_bus(bus_id) {
            let bus = bus.read();
            let devices = bus.get_devices();

            for (addr, device) in devices {
                let dev = device.read();
                if dev.port == port {
                    drop(dev);

                    // Notify drivers
                    let drivers = self.drivers.read();
                    for driver in drivers.iter() {
                        let dev = device.read();
                        driver.detach(&dev);
                    }

                    // Remove device
                    drop(bus);
                    if let Some(bus) = self.get_bus(bus_id) {
                        bus.write().unregister_device(addr);
                    }

                    self.stats.devices_removed.fetch_add(1, Ordering::Relaxed);
                    break;
                }
            }
        }
    }
}

/// Global USB subsystem
static USB: UsbSubsystem = UsbSubsystem::new();

/// Initialize USB subsystem
pub fn init() {
    if let Err(e) = USB.init() {
        crate::kprintln!("[USB] Initialization failed: {:?}", e);
    }
}

/// Get USB bus
pub fn get_bus(bus_id: u32) -> Option<Arc<RwLock<UsbBus>>> {
    USB.get_bus(bus_id)
}

/// Get all USB buses
pub fn get_buses() -> Vec<(u32, Arc<RwLock<UsbBus>>)> {
    USB.get_buses()
}

/// Register USB driver
pub fn register_driver(driver: Arc<dyn UsbDriver>) {
    USB.register_driver(driver);
}

/// Perform control transfer
pub fn control_transfer(
    device: &Arc<RwLock<UsbDevice>>,
    setup: &SetupPacket,
    data: Option<&mut [u8]>,
) -> Result<usize, UsbError> {
    USB.control_transfer(device, setup, data)
}

/// Perform bulk transfer
pub fn bulk_transfer(
    device: &Arc<RwLock<UsbDevice>>,
    endpoint: u8,
    data: &mut [u8],
    direction: TransferDirection,
) -> Result<usize, UsbError> {
    USB.bulk_transfer(device, endpoint, data, direction)
}

/// Perform interrupt transfer
pub fn interrupt_transfer(
    device: &Arc<RwLock<UsbDevice>>,
    endpoint: u8,
    data: &mut [u8],
    direction: TransferDirection,
) -> Result<usize, UsbError> {
    USB.interrupt_transfer(device, endpoint, data, direction)
}
