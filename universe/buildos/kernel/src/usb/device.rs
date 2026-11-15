//! USB Device Model
//!
//! USB device descriptors and device management:
//! - Device descriptors
//! - Configuration descriptors
//! - Interface descriptors
//! - Endpoint descriptors
//! - String descriptors
//! - Device lifecycle management

use alloc::string::String;
use alloc::vec::Vec;
use super::{UsbError, UsbSpeed, UsbClass, DeviceState};

/// USB Device Descriptor
#[repr(C, packed)]
#[derive(Clone, Copy, Debug, Default)]
pub struct UsbDeviceDescriptor {
    /// Descriptor length (18 bytes)
    pub length: u8,
    /// Descriptor type (1 = Device)
    pub descriptor_type: u8,
    /// USB specification release (BCD)
    pub bcd_usb: u16,
    /// Device class
    pub device_class: u8,
    /// Device subclass
    pub device_subclass: u8,
    /// Device protocol
    pub device_protocol: u8,
    /// Max packet size for endpoint 0
    pub max_packet_size0: u8,
    /// Vendor ID
    pub vendor_id: u16,
    /// Product ID
    pub product_id: u16,
    /// Device release number (BCD)
    pub bcd_device: u16,
    /// Index of manufacturer string
    pub manufacturer_index: u8,
    /// Index of product string
    pub product_index: u8,
    /// Index of serial number string
    pub serial_index: u8,
    /// Number of configurations
    pub num_configurations: u8,
}

impl UsbDeviceDescriptor {
    /// Create from raw bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, UsbError> {
        if data.len() < 18 {
            return Err(UsbError::InvalidDescriptor);
        }

        if data[1] != 1 {
            return Err(UsbError::InvalidDescriptor);
        }

        Ok(Self {
            length: data[0],
            descriptor_type: data[1],
            bcd_usb: u16::from_le_bytes([data[2], data[3]]),
            device_class: data[4],
            device_subclass: data[5],
            device_protocol: data[6],
            max_packet_size0: data[7],
            vendor_id: u16::from_le_bytes([data[8], data[9]]),
            product_id: u16::from_le_bytes([data[10], data[11]]),
            bcd_device: u16::from_le_bytes([data[12], data[13]]),
            manufacturer_index: data[14],
            product_index: data[15],
            serial_index: data[16],
            num_configurations: data[17],
        })
    }

    /// Get USB version string
    pub fn usb_version(&self) -> String {
        let major = (self.bcd_usb >> 8) & 0xFF;
        let minor = (self.bcd_usb >> 4) & 0x0F;
        let patch = self.bcd_usb & 0x0F;
        alloc::format!("{}.{}.{}", major, minor, patch)
    }

    /// Get device class
    pub fn class(&self) -> UsbClass {
        UsbClass::from(self.device_class)
    }
}

/// USB Configuration Descriptor
#[derive(Clone, Debug, Default)]
pub struct UsbConfigDescriptor {
    /// Descriptor length
    pub length: u8,
    /// Descriptor type (2 = Configuration)
    pub descriptor_type: u8,
    /// Total length of configuration data
    pub total_length: u16,
    /// Number of interfaces
    pub num_interfaces: u8,
    /// Configuration value
    pub configuration_value: u8,
    /// Configuration string index
    pub configuration_index: u8,
    /// Attributes
    pub attributes: u8,
    /// Maximum power (in 2mA units)
    pub max_power: u8,
    /// Interfaces
    pub interfaces: Vec<UsbInterfaceDescriptor>,
}

impl UsbConfigDescriptor {
    /// Create from raw bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, UsbError> {
        if data.len() < 9 {
            return Err(UsbError::InvalidDescriptor);
        }

        if data[1] != 2 {
            return Err(UsbError::InvalidDescriptor);
        }

        let total_length = u16::from_le_bytes([data[2], data[3]]);
        let num_interfaces = data[4];

        let mut config = Self {
            length: data[0],
            descriptor_type: data[1],
            total_length,
            num_interfaces,
            configuration_value: data[5],
            configuration_index: data[6],
            attributes: data[7],
            max_power: data[8],
            interfaces: Vec::new(),
        };

        // Parse interface and endpoint descriptors
        let mut offset = 9usize;
        let mut current_interface: Option<UsbInterfaceDescriptor> = None;

        while offset + 2 <= data.len() {
            let desc_len = data[offset] as usize;
            let desc_type = data[offset + 1];

            if desc_len == 0 || offset + desc_len > data.len() {
                break;
            }

            match desc_type {
                4 => {
                    // Interface descriptor
                    if let Some(iface) = current_interface.take() {
                        config.interfaces.push(iface);
                    }

                    if offset + 9 <= data.len() {
                        current_interface = Some(UsbInterfaceDescriptor::from_bytes(&data[offset..])?);
                    }
                }
                5 => {
                    // Endpoint descriptor
                    if let Some(ref mut iface) = current_interface {
                        if offset + 7 <= data.len() {
                            iface.endpoints.push(UsbEndpointDescriptor::from_bytes(&data[offset..])?);
                        }
                    }
                }
                _ => {
                    // Skip other descriptors (HID, etc.)
                }
            }

            offset += desc_len;
        }

        if let Some(iface) = current_interface {
            config.interfaces.push(iface);
        }

        Ok(config)
    }

    /// Is self-powered
    pub fn is_self_powered(&self) -> bool {
        self.attributes & 0x40 != 0
    }

    /// Supports remote wakeup
    pub fn remote_wakeup(&self) -> bool {
        self.attributes & 0x20 != 0
    }

    /// Get max power in milliamps
    pub fn max_power_ma(&self) -> u16 {
        self.max_power as u16 * 2
    }
}

/// USB Interface Descriptor
#[derive(Clone, Debug, Default)]
pub struct UsbInterfaceDescriptor {
    /// Descriptor length
    pub length: u8,
    /// Descriptor type (4 = Interface)
    pub descriptor_type: u8,
    /// Interface number
    pub interface_number: u8,
    /// Alternate setting
    pub alternate_setting: u8,
    /// Number of endpoints
    pub num_endpoints: u8,
    /// Interface class
    pub interface_class: u8,
    /// Interface subclass
    pub interface_subclass: u8,
    /// Interface protocol
    pub interface_protocol: u8,
    /// Interface string index
    pub interface_index: u8,
    /// Endpoints
    pub endpoints: Vec<UsbEndpointDescriptor>,
}

impl UsbInterfaceDescriptor {
    /// Create from raw bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, UsbError> {
        if data.len() < 9 {
            return Err(UsbError::InvalidDescriptor);
        }

        if data[1] != 4 {
            return Err(UsbError::InvalidDescriptor);
        }

        Ok(Self {
            length: data[0],
            descriptor_type: data[1],
            interface_number: data[2],
            alternate_setting: data[3],
            num_endpoints: data[4],
            interface_class: data[5],
            interface_subclass: data[6],
            interface_protocol: data[7],
            interface_index: data[8],
            endpoints: Vec::new(),
        })
    }

    /// Get interface class
    pub fn class(&self) -> UsbClass {
        UsbClass::from(self.interface_class)
    }
}

/// USB Endpoint Descriptor
#[repr(C, packed)]
#[derive(Clone, Copy, Debug, Default)]
pub struct UsbEndpointDescriptor {
    /// Descriptor length
    pub length: u8,
    /// Descriptor type (5 = Endpoint)
    pub descriptor_type: u8,
    /// Endpoint address
    pub endpoint_address: u8,
    /// Attributes
    pub attributes: u8,
    /// Max packet size
    pub max_packet_size: u16,
    /// Interval
    pub interval: u8,
}

impl UsbEndpointDescriptor {
    /// Create from raw bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, UsbError> {
        if data.len() < 7 {
            return Err(UsbError::InvalidDescriptor);
        }

        if data[1] != 5 {
            return Err(UsbError::InvalidDescriptor);
        }

        Ok(Self {
            length: data[0],
            descriptor_type: data[1],
            endpoint_address: data[2],
            attributes: data[3],
            max_packet_size: u16::from_le_bytes([data[4], data[5]]),
            interval: data[6],
        })
    }

    /// Get endpoint number
    pub fn number(&self) -> u8 {
        self.endpoint_address & 0x0F
    }

    /// Is IN endpoint
    pub fn is_in(&self) -> bool {
        self.endpoint_address & 0x80 != 0
    }

    /// Is OUT endpoint
    pub fn is_out(&self) -> bool {
        !self.is_in()
    }

    /// Get transfer type
    pub fn transfer_type(&self) -> EndpointTransferType {
        match self.attributes & 0x03 {
            0 => EndpointTransferType::Control,
            1 => EndpointTransferType::Isochronous,
            2 => EndpointTransferType::Bulk,
            3 => EndpointTransferType::Interrupt,
            _ => EndpointTransferType::Control,
        }
    }

    /// Get synchronization type (for isochronous)
    pub fn sync_type(&self) -> SyncType {
        match (self.attributes >> 2) & 0x03 {
            0 => SyncType::None,
            1 => SyncType::Asynchronous,
            2 => SyncType::Adaptive,
            3 => SyncType::Synchronous,
            _ => SyncType::None,
        }
    }

    /// Get usage type (for isochronous)
    pub fn usage_type(&self) -> UsageType {
        match (self.attributes >> 4) & 0x03 {
            0 => UsageType::Data,
            1 => UsageType::Feedback,
            2 => UsageType::ImplicitFeedback,
            _ => UsageType::Data,
        }
    }
}

/// Endpoint transfer type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointTransferType {
    Control,
    Isochronous,
    Bulk,
    Interrupt,
}

/// Synchronization type for isochronous endpoints
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncType {
    None,
    Asynchronous,
    Adaptive,
    Synchronous,
}

/// Usage type for isochronous endpoints
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageType {
    Data,
    Feedback,
    ImplicitFeedback,
}

/// USB Device Qualifier Descriptor
#[repr(C, packed)]
#[derive(Clone, Copy, Debug, Default)]
pub struct UsbDeviceQualifierDescriptor {
    /// Descriptor length (10 bytes)
    pub length: u8,
    /// Descriptor type (6 = Device Qualifier)
    pub descriptor_type: u8,
    /// USB specification release (BCD)
    pub bcd_usb: u16,
    /// Device class
    pub device_class: u8,
    /// Device subclass
    pub device_subclass: u8,
    /// Device protocol
    pub device_protocol: u8,
    /// Max packet size for endpoint 0
    pub max_packet_size0: u8,
    /// Number of configurations
    pub num_configurations: u8,
    /// Reserved
    pub reserved: u8,
}

/// USB String Descriptor
#[derive(Clone, Debug, Default)]
pub struct UsbStringDescriptor {
    /// Descriptor length
    pub length: u8,
    /// Descriptor type (3 = String)
    pub descriptor_type: u8,
    /// String (UTF-16LE)
    pub string: String,
}

impl UsbStringDescriptor {
    /// Create from raw bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, UsbError> {
        if data.len() < 2 {
            return Err(UsbError::InvalidDescriptor);
        }

        if data[1] != 3 {
            return Err(UsbError::InvalidDescriptor);
        }

        let length = data[0] as usize;
        if length < 2 || data.len() < length {
            return Err(UsbError::InvalidDescriptor);
        }

        // Convert UTF-16LE to String
        let utf16_data = &data[2..length];
        let utf16_chars: Vec<u16> = utf16_data
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();

        let string = String::from_utf16_lossy(&utf16_chars);

        Ok(Self {
            length: data[0],
            descriptor_type: data[1],
            string,
        })
    }
}

/// USB BOS (Binary Object Store) Descriptor
#[derive(Clone, Debug, Default)]
pub struct UsbBosDescriptor {
    /// Descriptor length
    pub length: u8,
    /// Descriptor type (15 = BOS)
    pub descriptor_type: u8,
    /// Total length
    pub total_length: u16,
    /// Number of device capabilities
    pub num_device_caps: u8,
    /// Device capabilities
    pub device_caps: Vec<DeviceCapability>,
}

/// Device Capability
#[derive(Clone, Debug)]
pub enum DeviceCapability {
    /// USB 2.0 Extension
    Usb2Extension {
        lpm_capable: bool,
    },
    /// SuperSpeed USB
    SuperSpeed {
        supported_speeds: u16,
        functionality_support: u8,
        u1_dev_exit_lat: u8,
        u2_dev_exit_lat: u16,
    },
    /// Container ID
    ContainerId {
        container_id: [u8; 16],
    },
    /// Platform
    Platform {
        capability_uuid: [u8; 16],
        capability_data: Vec<u8>,
    },
    /// SuperSpeed Plus
    SuperSpeedPlus {
        min_rx_lane_count: u8,
        min_tx_lane_count: u8,
    },
    /// Unknown
    Unknown {
        capability_type: u8,
        data: Vec<u8>,
    },
}

/// USB Device
#[derive(Debug)]
pub struct UsbDevice {
    /// Device address
    pub address: u8,
    /// Device speed
    pub speed: UsbSpeed,
    /// Bus ID
    pub bus_id: u32,
    /// Port number
    pub port: u32,
    /// Device state
    pub state: DeviceState,
    /// Device descriptor
    pub descriptor: Option<UsbDeviceDescriptor>,
    /// Configurations
    pub configurations: Vec<UsbConfigDescriptor>,
    /// Current configuration
    pub current_config: Option<u8>,
    /// Parent hub address (0 if root)
    pub parent_hub: u8,
    /// Parent hub port
    pub parent_port: u8,
    /// Device path
    pub path: String,
    /// Manufacturer string
    pub manufacturer: Option<String>,
    /// Product string
    pub product: Option<String>,
    /// Serial number
    pub serial: Option<String>,
}

impl UsbDevice {
    /// Create new USB device
    pub fn new(address: u8, speed: UsbSpeed, bus_id: u32, port: u32) -> Self {
        Self {
            address,
            speed,
            bus_id,
            port,
            state: DeviceState::Default,
            descriptor: None,
            configurations: Vec::new(),
            current_config: None,
            parent_hub: 0,
            parent_port: 0,
            path: alloc::format!("{}-{}", bus_id, port),
            manufacturer: None,
            product: None,
            serial: None,
        }
    }

    /// Get vendor ID
    pub fn vendor_id(&self) -> u16 {
        self.descriptor.as_ref().map(|d| d.vendor_id).unwrap_or(0)
    }

    /// Get product ID
    pub fn product_id(&self) -> u16 {
        self.descriptor.as_ref().map(|d| d.product_id).unwrap_or(0)
    }

    /// Get device class
    pub fn class(&self) -> UsbClass {
        self.descriptor.as_ref()
            .map(|d| UsbClass::from(d.device_class))
            .unwrap_or(UsbClass::VendorSpecific)
    }

    /// Get current configuration
    pub fn current_configuration(&self) -> Option<&UsbConfigDescriptor> {
        let config_value = self.current_config?;
        self.configurations.iter()
            .find(|c| c.configuration_value == config_value)
    }

    /// Get interface
    pub fn get_interface(&self, interface_num: u8) -> Option<&UsbInterfaceDescriptor> {
        self.current_configuration()?
            .interfaces
            .iter()
            .find(|i| i.interface_number == interface_num)
    }

    /// Find endpoint
    pub fn find_endpoint(&self, interface_num: u8, transfer_type: EndpointTransferType, is_in: bool) -> Option<&UsbEndpointDescriptor> {
        self.get_interface(interface_num)?
            .endpoints
            .iter()
            .find(|e| e.transfer_type() == transfer_type && e.is_in() == is_in)
    }

    /// Is this a hub?
    pub fn is_hub(&self) -> bool {
        self.class() == UsbClass::Hub ||
        self.current_configuration()
            .map(|c| c.interfaces.iter().any(|i| i.class() == UsbClass::Hub))
            .unwrap_or(false)
    }

    /// Is this a HID device?
    pub fn is_hid(&self) -> bool {
        self.class() == UsbClass::Hid ||
        self.current_configuration()
            .map(|c| c.interfaces.iter().any(|i| i.class() == UsbClass::Hid))
            .unwrap_or(false)
    }

    /// Is this a mass storage device?
    pub fn is_mass_storage(&self) -> bool {
        self.class() == UsbClass::MassStorage ||
        self.current_configuration()
            .map(|c| c.interfaces.iter().any(|i| i.class() == UsbClass::MassStorage))
            .unwrap_or(false)
    }

    /// Get device description
    pub fn description(&self) -> String {
        if let Some(ref product) = self.product {
            product.clone()
        } else if let Some(ref desc) = self.descriptor {
            // Use read_unaligned for packed struct fields
            let vendor_id: u16 = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(desc.vendor_id)) };
            let product_id: u16 = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(desc.product_id)) };
            alloc::format!("USB Device {:04x}:{:04x}", vendor_id, product_id)
        } else {
            String::from("Unknown USB Device")
        }
    }
}

/// USB Device Endpoint
#[derive(Debug)]
pub struct UsbEndpoint {
    /// Endpoint address
    pub address: u8,
    /// Endpoint descriptor
    pub descriptor: UsbEndpointDescriptor,
    /// Data toggle
    pub toggle: bool,
    /// Is halted
    pub halted: bool,
}

impl UsbEndpoint {
    /// Create from descriptor
    pub fn from_descriptor(desc: UsbEndpointDescriptor) -> Self {
        Self {
            address: desc.endpoint_address,
            descriptor: desc,
            toggle: false,
            halted: false,
        }
    }

    /// Toggle data toggle
    pub fn toggle_data(&mut self) {
        self.toggle = !self.toggle;
    }

    /// Reset data toggle
    pub fn reset_toggle(&mut self) {
        self.toggle = false;
    }
}
