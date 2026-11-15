// ===============================================================================
// BUILDOS KERNEL - USB SUBSYSTEM
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================
//
// Universal Serial Bus (USB) host controller and device management.
// Supports USB 1.1/2.0/3.x via UHCI/OHCI/EHCI/xHCI controllers.
//
// ===============================================================================

#![allow(dead_code)]

pub mod usb_core;
pub mod xhci;
pub mod ehci;
pub mod hid;
pub mod mass_storage;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::AtomicU32;
use spin::RwLock;

pub use self::usb_core::*;

// =============================================================================
// USB SPEED DEFINITIONS
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum UsbSpeed {
    Low = 0,        // 1.5 Mbps (USB 1.0)
    Full = 1,       // 12 Mbps (USB 1.1)
    High = 2,       // 480 Mbps (USB 2.0)
    Super = 3,      // 5 Gbps (USB 3.0)
    SuperPlus = 4,  // 10 Gbps (USB 3.1)
    SuperPlus2 = 5, // 20 Gbps (USB 3.2)
}

impl UsbSpeed {
    pub fn max_packet_size(&self, endpoint_type: EndpointType) -> u16 {
        match (self, endpoint_type) {
            (UsbSpeed::Low, EndpointType::Control) => 8,
            (UsbSpeed::Low, EndpointType::Interrupt) => 8,
            (UsbSpeed::Full, EndpointType::Control) => 64,
            (UsbSpeed::Full, EndpointType::Bulk) => 64,
            (UsbSpeed::Full, EndpointType::Interrupt) => 64,
            (UsbSpeed::Full, EndpointType::Isochronous) => 1023,
            (UsbSpeed::High, EndpointType::Control) => 64,
            (UsbSpeed::High, EndpointType::Bulk) => 512,
            (UsbSpeed::High, EndpointType::Interrupt) => 1024,
            (UsbSpeed::High, EndpointType::Isochronous) => 1024,
            (UsbSpeed::Super, EndpointType::Control) => 512,
            (UsbSpeed::Super, EndpointType::Bulk) => 1024,
            (UsbSpeed::Super, EndpointType::Interrupt) => 1024,
            (UsbSpeed::Super, EndpointType::Isochronous) => 1024,
            _ => 1024,
        }
    }
}

// =============================================================================
// ENDPOINT TYPES
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EndpointType {
    Control = 0,
    Isochronous = 1,
    Bulk = 2,
    Interrupt = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointDirection {
    Out = 0,
    In = 1,
}

// =============================================================================
// USB REQUEST TYPES
// =============================================================================

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum RequestType {
    Standard = 0,
    Class = 1,
    Vendor = 2,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum RequestRecipient {
    Device = 0,
    Interface = 1,
    Endpoint = 2,
    Other = 3,
}

// Standard USB requests
pub const USB_REQ_GET_STATUS: u8 = 0x00;
pub const USB_REQ_CLEAR_FEATURE: u8 = 0x01;
pub const USB_REQ_SET_FEATURE: u8 = 0x03;
pub const USB_REQ_SET_ADDRESS: u8 = 0x05;
pub const USB_REQ_GET_DESCRIPTOR: u8 = 0x06;
pub const USB_REQ_SET_DESCRIPTOR: u8 = 0x07;
pub const USB_REQ_GET_CONFIGURATION: u8 = 0x08;
pub const USB_REQ_SET_CONFIGURATION: u8 = 0x09;
pub const USB_REQ_GET_INTERFACE: u8 = 0x0A;
pub const USB_REQ_SET_INTERFACE: u8 = 0x0B;
pub const USB_REQ_SYNCH_FRAME: u8 = 0x0C;

// Descriptor types
pub const USB_DESC_DEVICE: u8 = 0x01;
pub const USB_DESC_CONFIGURATION: u8 = 0x02;
pub const USB_DESC_STRING: u8 = 0x03;
pub const USB_DESC_INTERFACE: u8 = 0x04;
pub const USB_DESC_ENDPOINT: u8 = 0x05;
pub const USB_DESC_DEVICE_QUALIFIER: u8 = 0x06;
pub const USB_DESC_OTHER_SPEED: u8 = 0x07;
pub const USB_DESC_INTERFACE_POWER: u8 = 0x08;
pub const USB_DESC_HID: u8 = 0x21;
pub const USB_DESC_HID_REPORT: u8 = 0x22;

// USB class codes
pub const USB_CLASS_AUDIO: u8 = 0x01;
pub const USB_CLASS_CDC: u8 = 0x02;
pub const USB_CLASS_HID: u8 = 0x03;
pub const USB_CLASS_PHYSICAL: u8 = 0x05;
pub const USB_CLASS_IMAGE: u8 = 0x06;
pub const USB_CLASS_PRINTER: u8 = 0x07;
pub const USB_CLASS_MASS_STORAGE: u8 = 0x08;
pub const USB_CLASS_HUB: u8 = 0x09;
pub const USB_CLASS_CDC_DATA: u8 = 0x0A;
pub const USB_CLASS_VENDOR: u8 = 0xFF;

// =============================================================================
// USB SETUP PACKET
// =============================================================================

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct SetupPacket {
    pub request_type: u8,
    pub request: u8,
    pub value: u16,
    pub index: u16,
    pub length: u16,
}

impl SetupPacket {
    pub fn new(
        direction: EndpointDirection,
        req_type: RequestType,
        recipient: RequestRecipient,
        request: u8,
        value: u16,
        index: u16,
        length: u16,
    ) -> Self {
        let request_type = ((direction as u8) << 7)
            | ((req_type as u8) << 5)
            | (recipient as u8);

        Self {
            request_type,
            request,
            value,
            index,
            length,
        }
    }

    /// Create a SetupPacket with raw request_type byte
    pub fn from_raw(request_type: u8, request: u8, value: u16, index: u16, length: u16) -> Self {
        Self {
            request_type,
            request,
            value,
            index,
            length,
        }
    }

    pub fn get_descriptor(desc_type: u8, desc_index: u8, length: u16) -> Self {
        Self::new(
            EndpointDirection::In,
            RequestType::Standard,
            RequestRecipient::Device,
            USB_REQ_GET_DESCRIPTOR,
            ((desc_type as u16) << 8) | (desc_index as u16),
            0,
            length,
        )
    }

    pub fn set_address(address: u8) -> Self {
        Self::new(
            EndpointDirection::Out,
            RequestType::Standard,
            RequestRecipient::Device,
            USB_REQ_SET_ADDRESS,
            address as u16,
            0,
            0,
        )
    }

    pub fn set_configuration(config: u8) -> Self {
        Self::new(
            EndpointDirection::Out,
            RequestType::Standard,
            RequestRecipient::Device,
            USB_REQ_SET_CONFIGURATION,
            config as u16,
            0,
            0,
        )
    }

    pub fn get_string(index: u8, lang_id: u16, length: u16) -> Self {
        Self::new(
            EndpointDirection::In,
            RequestType::Standard,
            RequestRecipient::Device,
            USB_REQ_GET_DESCRIPTOR,
            ((USB_DESC_STRING as u16) << 8) | (index as u16),
            lang_id,
            length,
        )
    }
}

// =============================================================================
// USB DESCRIPTORS
// =============================================================================

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct DeviceDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub usb_version: u16,
    pub device_class: u8,
    pub device_subclass: u8,
    pub device_protocol: u8,
    pub max_packet_size: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_version: u16,
    pub manufacturer_index: u8,
    pub product_index: u8,
    pub serial_index: u8,
    pub num_configurations: u8,
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct ConfigurationDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub total_length: u16,
    pub num_interfaces: u8,
    pub configuration_value: u8,
    pub configuration_index: u8,
    pub attributes: u8,
    pub max_power: u8,
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct InterfaceDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub interface_number: u8,
    pub alternate_setting: u8,
    pub num_endpoints: u8,
    pub interface_class: u8,
    pub interface_subclass: u8,
    pub interface_protocol: u8,
    pub interface_index: u8,
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct EndpointDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub endpoint_address: u8,
    pub attributes: u8,
    pub max_packet_size: u16,
    pub interval: u8,
}

impl EndpointDescriptor {
    pub fn number(&self) -> u8 {
        self.endpoint_address & 0x0F
    }

    pub fn direction(&self) -> EndpointDirection {
        if self.endpoint_address & 0x80 != 0 {
            EndpointDirection::In
        } else {
            EndpointDirection::Out
        }
    }

    pub fn transfer_type(&self) -> EndpointType {
        match self.attributes & 0x03 {
            0 => EndpointType::Control,
            1 => EndpointType::Isochronous,
            2 => EndpointType::Bulk,
            3 => EndpointType::Interrupt,
            _ => EndpointType::Control,
        }
    }
}

// =============================================================================
// USB TRANSFER
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferStatus {
    Pending,
    Complete,
    Error,
    Stall,
    Timeout,
    Cancelled,
}

/// Transfer type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferType {
    /// Control transfer
    Control,
    /// Bulk transfer
    Bulk,
    /// Interrupt transfer
    Interrupt,
    /// Isochronous transfer
    Isochronous,
}

pub struct UsbTransfer {
    pub device_address: u8,
    pub endpoint: u8,
    pub direction: EndpointDirection,
    pub transfer_type: EndpointType,
    pub buffer: *mut u8,
    pub buffer_length: usize,
    pub actual_length: usize,
    pub status: TransferStatus,
    pub setup_packet: Option<SetupPacket>,
}

impl UsbTransfer {
    pub fn control(
        device_address: u8,
        setup: SetupPacket,
        buffer: *mut u8,
        length: usize,
    ) -> Self {
        let direction = if setup.request_type & 0x80 != 0 {
            EndpointDirection::In
        } else {
            EndpointDirection::Out
        };

        Self {
            device_address,
            endpoint: 0,
            direction,
            transfer_type: EndpointType::Control,
            buffer,
            buffer_length: length,
            actual_length: 0,
            status: TransferStatus::Pending,
            setup_packet: Some(setup),
        }
    }

    pub fn bulk(
        device_address: u8,
        endpoint: u8,
        direction: EndpointDirection,
        buffer: *mut u8,
        length: usize,
    ) -> Self {
        Self {
            device_address,
            endpoint,
            direction,
            transfer_type: EndpointType::Bulk,
            buffer,
            buffer_length: length,
            actual_length: 0,
            status: TransferStatus::Pending,
            setup_packet: None,
        }
    }

    pub fn interrupt(
        device_address: u8,
        endpoint: u8,
        direction: EndpointDirection,
        buffer: *mut u8,
        length: usize,
    ) -> Self {
        Self {
            device_address,
            endpoint,
            direction,
            transfer_type: EndpointType::Interrupt,
            buffer,
            buffer_length: length,
            actual_length: 0,
            status: TransferStatus::Pending,
            setup_packet: None,
        }
    }
}

// =============================================================================
// USB DEVICE
// =============================================================================

pub struct UsbDevice {
    pub address: u8,
    pub speed: UsbSpeed,
    pub port: u8,
    pub hub_address: u8,
    pub device_descriptor: DeviceDescriptor,
    pub configuration: Option<u8>,
    pub interfaces: Vec<UsbInterface>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub serial: Option<String>,
    controller: Arc<dyn UsbController>,
}

pub struct UsbInterface {
    pub number: u8,
    pub class: u8,
    pub subclass: u8,
    pub protocol: u8,
    pub endpoints: Vec<UsbEndpoint>,
    pub driver: Option<Arc<dyn UsbDriver>>,
}

pub struct UsbEndpoint {
    pub address: u8,
    pub direction: EndpointDirection,
    pub transfer_type: EndpointType,
    pub max_packet_size: u16,
    pub interval: u8,
}

impl UsbEndpoint {
    /// Get endpoint number (without direction bit)
    pub fn number(&self) -> u8 {
        self.address & 0x0F
    }

    /// Get transfer type
    pub fn transfer_type(&self) -> EndpointType {
        self.transfer_type
    }

    /// Get endpoint direction
    pub fn direction(&self) -> EndpointDirection {
        self.direction
    }
}

impl UsbDevice {
    pub fn control_transfer(&self, setup: SetupPacket, buffer: Option<&mut [u8]>) -> Result<usize, &'static str> {
        match buffer {
            Some(buf) => self.controller.control_transfer(self.address, setup, buf),
            None => {
                let mut empty = [0u8; 0];
                self.controller.control_transfer(self.address, setup, &mut empty)
            }
        }
    }

    pub fn bulk_transfer(
        &self,
        endpoint: u8,
        direction: EndpointDirection,
        buffer: &mut [u8],
    ) -> Result<usize, &'static str> {
        self.controller.bulk_transfer(self.address, endpoint, direction, buffer)
    }

    pub fn interrupt_transfer(
        &self,
        endpoint: u8,
        direction: EndpointDirection,
        buffer: &mut [u8],
    ) -> Result<usize, &'static str> {
        self.controller.interrupt_transfer(self.address, endpoint, direction, buffer)
    }

    /// Get interface by number
    pub fn interface(&self, num: u8) -> Option<&UsbInterface> {
        self.interfaces.iter().find(|i| i.number == num)
    }

    /// Get endpoints for an interface
    pub fn endpoints(&self, interface_num: u8) -> impl Iterator<Item = &UsbEndpoint> {
        self.interfaces
            .iter()
            .filter(move |i| i.number == interface_num)
            .flat_map(|i| i.endpoints.iter())
    }

    pub fn get_string(&self, index: u8) -> Option<String> {
        if index == 0 {
            return None;
        }

        // Get language ID first
        let mut lang_buf = [0u8; 4];
        let setup = SetupPacket::get_string(0, 0, 4);
        if self.control_transfer(setup, Some(&mut lang_buf)).is_err() {
            return None;
        }

        let lang_id = u16::from_le_bytes([lang_buf[2], lang_buf[3]]);

        // Get actual string
        let mut str_buf = [0u8; 256];
        let setup = SetupPacket::get_string(index, lang_id, 256);
        if let Ok(len) = self.control_transfer(setup, Some(&mut str_buf)) {
            if len >= 2 && str_buf[1] == USB_DESC_STRING {
                let str_len = (str_buf[0] as usize).saturating_sub(2) / 2;
                let mut chars = Vec::with_capacity(str_len);
                for i in 0..str_len {
                    let c = u16::from_le_bytes([str_buf[2 + i * 2], str_buf[3 + i * 2]]);
                    if let Some(ch) = char::from_u32(c as u32) {
                        chars.push(ch);
                    }
                }
                return Some(chars.into_iter().collect());
            }
        }
        None
    }
}

// =============================================================================
// USB CONTROLLER TRAIT
// =============================================================================

pub trait UsbController: Send + Sync {
    fn name(&self) -> &'static str;
    fn init(&self) -> Result<(), &'static str>;
    fn start(&self) -> Result<(), &'static str>;
    fn stop(&self) -> Result<(), &'static str>;

    fn port_count(&self) -> u8;
    fn port_status(&self, port: u8) -> PortStatus;
    fn port_reset(&self, port: u8) -> Result<(), &'static str>;
    fn port_enable(&self, port: u8) -> Result<(), &'static str>;
    fn port_disable(&self, port: u8) -> Result<(), &'static str>;

    fn allocate_address(&self) -> Option<u8>;
    fn free_address(&self, address: u8);

    fn control_transfer(
        &self,
        address: u8,
        setup: SetupPacket,
        buffer: &mut [u8],
    ) -> Result<usize, &'static str>;

    fn bulk_transfer(
        &self,
        address: u8,
        endpoint: u8,
        direction: EndpointDirection,
        buffer: &mut [u8],
    ) -> Result<usize, &'static str>;

    fn interrupt_transfer(
        &self,
        address: u8,
        endpoint: u8,
        direction: EndpointDirection,
        buffer: &mut [u8],
    ) -> Result<usize, &'static str>;

    fn configure_endpoint(
        &self,
        address: u8,
        endpoint: &EndpointDescriptor,
    ) -> Result<(), &'static str>;
}

#[derive(Debug, Clone, Copy)]
pub struct PortStatus {
    pub connected: bool,
    pub enabled: bool,
    pub suspended: bool,
    pub reset: bool,
    pub power: bool,
    pub speed: UsbSpeed,
    pub changed: bool,
}

impl Default for PortStatus {
    fn default() -> Self {
        Self {
            connected: false,
            enabled: false,
            suspended: false,
            reset: false,
            power: false,
            speed: UsbSpeed::Full,
            changed: false,
        }
    }
}

// =============================================================================
// USB DRIVER TRAIT
// =============================================================================

pub trait UsbDriver: Send + Sync {
    fn name(&self) -> &'static str;

    fn probe(&self, interface: &UsbInterface) -> bool;

    fn attach(&self, device: Arc<UsbDevice>, interface_num: u8) -> Result<(), &'static str>;

    fn detach(&self, device: Arc<UsbDevice>, interface_num: u8);

    fn suspend(&self, _device: Arc<UsbDevice>) -> Result<(), &'static str> {
        Ok(())
    }

    fn resume(&self, _device: Arc<UsbDevice>) -> Result<(), &'static str> {
        Ok(())
    }
}

// =============================================================================
// USB SUBSYSTEM MANAGER
// =============================================================================

pub struct UsbSubsystem {
    controllers: RwLock<Vec<Arc<dyn UsbController>>>,
    devices: RwLock<BTreeMap<u8, Arc<UsbDevice>>>,
    drivers: RwLock<Vec<Arc<dyn UsbDriver>>>,
    next_address: AtomicU32,
}

impl UsbSubsystem {
    pub const fn new() -> Self {
        Self {
            controllers: RwLock::new(Vec::new()),
            devices: RwLock::new(BTreeMap::new()),
            drivers: RwLock::new(Vec::new()),
            next_address: AtomicU32::new(1),
        }
    }

    pub fn register_controller(&self, controller: Arc<dyn UsbController>) {
        self.controllers.write().push(controller);
    }

    pub fn register_driver(&self, driver: Arc<dyn UsbDriver>) {
        self.drivers.write().push(driver);
    }

    pub fn init(&self) -> Result<(), &'static str> {
        let controllers = self.controllers.read();
        for controller in controllers.iter() {
            controller.init()?;
            controller.start()?;

            // Enumerate ports
            for port in 0..controller.port_count() {
                let status = controller.port_status(port);
                if status.connected {
                    if let Err(e) = self.enumerate_device(controller.clone(), port) {
                        crate::log::warn!("USB: Failed to enumerate device on port {}: {}", port, e);
                    }
                }
            }
        }
        Ok(())
    }

    fn enumerate_device(
        &self,
        controller: Arc<dyn UsbController>,
        port: u8,
    ) -> Result<(), &'static str> {
        // Reset port
        controller.port_reset(port)?;

        // Wait for reset complete
        for _ in 0..100 {
            let status = controller.port_status(port);
            if !status.reset && status.enabled {
                break;
            }
            // Small delay - in real impl would use timer
        }

        let status = controller.port_status(port);
        if !status.enabled {
            return Err("Port not enabled after reset");
        }

        // Allocate address
        let address = controller.allocate_address()
            .ok_or("No USB addresses available")?;

        // Get device descriptor (first 8 bytes to get max packet size)
        let mut desc_buf = [0u8; 18];
        let setup = SetupPacket::get_descriptor(USB_DESC_DEVICE, 0, 8);
        let _transfer = UsbTransfer::control(0, setup, desc_buf.as_mut_ptr(), 8);

        controller.control_transfer(0, setup, &mut desc_buf[..8])?;

        let _max_packet = desc_buf[7];

        // Set address
        let setup = SetupPacket::set_address(address);
        controller.control_transfer(0, setup, &mut [])?;

        // Get full device descriptor
        let setup = SetupPacket::get_descriptor(USB_DESC_DEVICE, 0, 18);
        controller.control_transfer(address, setup, &mut desc_buf)?;

        let device_desc = unsafe { *(desc_buf.as_ptr() as *const DeviceDescriptor) };
        // Copy packed fields to avoid unaligned access
        let vendor_id = { device_desc.vendor_id };
        let product_id = { device_desc.product_id };

        crate::log::info!(
            "USB: Device {:04x}:{:04x} at address {}",
            vendor_id,
            product_id,
            address
        );

        // Get configuration
        let mut config_buf = [0u8; 256];
        let setup = SetupPacket::get_descriptor(USB_DESC_CONFIGURATION, 0, 256);
        let config_len = controller.control_transfer(address, setup, &mut config_buf)?;

        let config_desc = unsafe { *(config_buf.as_ptr() as *const ConfigurationDescriptor) };

        // Parse interfaces and endpoints
        let mut interfaces = Vec::new();
        let mut offset = config_desc.length as usize;

        while offset < config_len {
            let desc_len = config_buf[offset] as usize;
            let desc_type = config_buf[offset + 1];

            if desc_type == USB_DESC_INTERFACE {
                let iface_desc = unsafe {
                    *(config_buf.as_ptr().add(offset) as *const InterfaceDescriptor)
                };

                let mut endpoints = Vec::new();
                let mut ep_offset = offset + desc_len;

                while ep_offset < config_len {
                    let ep_len = config_buf[ep_offset] as usize;
                    let ep_type = config_buf[ep_offset + 1];

                    if ep_type == USB_DESC_ENDPOINT {
                        let ep_desc = unsafe {
                            *(config_buf.as_ptr().add(ep_offset) as *const EndpointDescriptor)
                        };

                        endpoints.push(UsbEndpoint {
                            address: ep_desc.endpoint_address,
                            direction: ep_desc.direction(),
                            transfer_type: ep_desc.transfer_type(),
                            max_packet_size: ep_desc.max_packet_size,
                            interval: ep_desc.interval,
                        });

                        // Configure endpoint on controller
                        controller.configure_endpoint(address, &ep_desc)?;
                    } else if ep_type == USB_DESC_INTERFACE {
                        break;
                    }

                    ep_offset += ep_len;
                }

                interfaces.push(UsbInterface {
                    number: iface_desc.interface_number,
                    class: iface_desc.interface_class,
                    subclass: iface_desc.interface_subclass,
                    protocol: iface_desc.interface_protocol,
                    endpoints,
                    driver: None,
                });
            }

            offset += desc_len;
        }

        // Set configuration
        let setup = SetupPacket::set_configuration(config_desc.configuration_value);
        controller.control_transfer(address, setup, &mut [])?;

        // Create device structure
        let device = Arc::new(UsbDevice {
            address,
            speed: status.speed,
            port,
            hub_address: 0,
            device_descriptor: device_desc,
            configuration: Some(config_desc.configuration_value),
            interfaces,
            manufacturer: None, // Fetch later if needed
            product: None,
            serial: None,
            controller,
        });

        // Try to find matching drivers
        self.attach_drivers(device.clone());

        // Store device
        self.devices.write().insert(address, device);

        Ok(())
    }

    fn attach_drivers(&self, device: Arc<UsbDevice>) {
        let drivers = self.drivers.read();

        for iface in &device.interfaces {
            for driver in drivers.iter() {
                if driver.probe(iface) {
                    if let Err(e) = driver.attach(device.clone(), iface.number) {
                        crate::log::warn!("USB: Driver {} failed to attach: {}", driver.name(), e);
                    } else {
                        crate::log::info!("USB: Driver {} attached to interface {}", driver.name(), iface.number);
                        break;
                    }
                }
            }
        }
    }

    pub fn device(&self, address: u8) -> Option<Arc<UsbDevice>> {
        self.devices.read().get(&address).cloned()
    }

    pub fn devices(&self) -> Vec<Arc<UsbDevice>> {
        self.devices.read().values().cloned().collect()
    }
}

// Global USB subsystem
static USB_SUBSYSTEM: UsbSubsystem = UsbSubsystem::new();

pub fn usb_subsystem() -> &'static UsbSubsystem {
    &USB_SUBSYSTEM
}

/// Initialize USB subsystem
pub fn init() -> Result<(), &'static str> {
    crate::log::info!("USB: Initializing USB subsystem");

    // Register built-in drivers
    usb_subsystem().register_driver(Arc::new(hid::HidDriver::new()));
    usb_subsystem().register_driver(Arc::new(mass_storage::MassStorageDriver::new()));

    // Scan for USB controllers on PCI bus
    xhci::probe_and_init()?;
    ehci::probe_and_init()?;

    // Initialize subsystem (enumerate devices)
    usb_subsystem().init()?;

    crate::log::info!("USB: Initialization complete");
    Ok(())
}
