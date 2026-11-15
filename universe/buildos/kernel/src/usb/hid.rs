//! USB HID (Human Interface Device) Driver
//!
//! HID class driver for keyboards, mice, and other input devices:
//! - HID descriptor parsing
//! - Report descriptor parsing
//! - Boot protocol support
//! - Report protocol support
//! - Input event generation

#![allow(dead_code)]

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::{Mutex, RwLock};
use super::{
    UsbError, UsbDevice, UsbDriver, UsbClass,
    SetupPacket, TransferDirection,
    device::EndpointTransferType,
};

/// HID class requests
#[repr(u8)]
pub enum HidRequest {
    GetReport = 0x01,
    GetIdle = 0x02,
    GetProtocol = 0x03,
    SetReport = 0x09,
    SetIdle = 0x0A,
    SetProtocol = 0x0B,
}

/// HID report types
#[repr(u8)]
pub enum ReportType {
    Input = 1,
    Output = 2,
    Feature = 3,
}

/// HID protocol
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HidProtocol {
    Boot = 0,
    Report = 1,
}

/// HID subclass
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HidSubclass {
    None = 0,
    Boot = 1,
}

impl From<u8> for HidSubclass {
    fn from(val: u8) -> Self {
        match val {
            1 => Self::Boot,
            _ => Self::None,
        }
    }
}

/// HID boot protocol
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootProtocol {
    None = 0,
    Keyboard = 1,
    Mouse = 2,
}

impl From<u8> for BootProtocol {
    fn from(val: u8) -> Self {
        match val {
            1 => Self::Keyboard,
            2 => Self::Mouse,
            _ => Self::None,
        }
    }
}

/// HID descriptor
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct HidDescriptor {
    /// Descriptor length
    pub length: u8,
    /// Descriptor type (0x21)
    pub descriptor_type: u8,
    /// HID specification release (BCD)
    pub bcd_hid: u16,
    /// Country code
    pub country_code: u8,
    /// Number of HID class descriptors
    pub num_descriptors: u8,
    /// Class descriptor type (0x22 = Report)
    pub class_descriptor_type: u8,
    /// Class descriptor length
    pub class_descriptor_length: u16,
}

impl HidDescriptor {
    /// Create from raw bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, UsbError> {
        if data.len() < 9 {
            return Err(UsbError::InvalidDescriptor);
        }

        if data[1] != 0x21 {
            return Err(UsbError::InvalidDescriptor);
        }

        Ok(Self {
            length: data[0],
            descriptor_type: data[1],
            bcd_hid: u16::from_le_bytes([data[2], data[3]]),
            country_code: data[4],
            num_descriptors: data[5],
            class_descriptor_type: data[6],
            class_descriptor_length: u16::from_le_bytes([data[7], data[8]]),
        })
    }
}

/// HID report field
#[derive(Clone, Debug)]
pub struct ReportField {
    /// Usage page
    pub usage_page: u16,
    /// Usage
    pub usage: u16,
    /// Logical minimum
    pub logical_min: i32,
    /// Logical maximum
    pub logical_max: i32,
    /// Physical minimum
    pub physical_min: i32,
    /// Physical maximum
    pub physical_max: i32,
    /// Report size (bits)
    pub report_size: u32,
    /// Report count
    pub report_count: u32,
    /// Is array (vs variable)
    pub is_array: bool,
    /// Is absolute (vs relative)
    pub is_absolute: bool,
    /// Has null state
    pub has_null: bool,
    /// Bit offset in report
    pub bit_offset: u32,
}

/// HID report
#[derive(Clone, Debug)]
pub struct HidReport {
    /// Report ID (0 if none)
    pub id: u8,
    /// Report type
    pub report_type: u8,
    /// Total bit length
    pub bit_length: u32,
    /// Fields
    pub fields: Vec<ReportField>,
}

/// HID report parser
pub struct ReportParser {
    /// Global state stack
    global_stack: Vec<GlobalState>,
    /// Local state
    local: LocalState,
    /// Current global state
    global: GlobalState,
    /// Reports
    reports: Vec<HidReport>,
    /// Current report type
    current_type: u8,
    /// Bit offset
    bit_offset: u32,
}

#[derive(Clone, Default)]
struct GlobalState {
    usage_page: u16,
    logical_min: i32,
    logical_max: i32,
    physical_min: i32,
    physical_max: i32,
    report_size: u32,
    report_count: u32,
    report_id: u8,
}

#[derive(Clone, Default)]
struct LocalState {
    usages: Vec<u16>,
    usage_min: u16,
    usage_max: u16,
}

impl ReportParser {
    /// Create new report parser
    pub fn new() -> Self {
        Self {
            global_stack: Vec::new(),
            local: LocalState::default(),
            global: GlobalState::default(),
            reports: Vec::new(),
            current_type: 0,
            bit_offset: 0,
        }
    }

    /// Parse report descriptor
    pub fn parse(&mut self, data: &[u8]) -> Result<Vec<HidReport>, UsbError> {
        let mut offset = 0;

        while offset < data.len() {
            let prefix = data[offset];
            offset += 1;

            let size = match prefix & 0x03 {
                0 => 0,
                1 => 1,
                2 => 2,
                3 => 4,
                _ => 0,
            };

            if offset + size > data.len() {
                break;
            }

            let item_data: u32 = match size {
                0 => 0,
                1 => data[offset] as u32,
                2 => u16::from_le_bytes([data[offset], data[offset + 1]]) as u32,
                4 => u32::from_le_bytes([
                    data[offset],
                    data[offset + 1],
                    data[offset + 2],
                    data[offset + 3],
                ]),
                _ => 0,
            };
            offset += size;

            let item_type = (prefix >> 2) & 0x03;
            let item_tag = (prefix >> 4) & 0x0F;

            match item_type {
                0 => self.parse_main_item(item_tag, item_data)?,
                1 => self.parse_global_item(item_tag, item_data),
                2 => self.parse_local_item(item_tag, item_data),
                _ => {}
            }
        }

        Ok(self.reports.clone())
    }

    /// Parse main item
    fn parse_main_item(&mut self, tag: u8, data: u32) -> Result<(), UsbError> {
        match tag {
            0x08 => {
                // Input
                self.add_field(ReportType::Input as u8, data);
            }
            0x09 => {
                // Output
                self.add_field(ReportType::Output as u8, data);
            }
            0x0B => {
                // Feature
                self.add_field(ReportType::Feature as u8, data);
            }
            0x0A => {
                // Collection
                // Push state
            }
            0x0C => {
                // End collection
                // Pop state
            }
            _ => {}
        }

        // Clear local state
        self.local = LocalState::default();

        Ok(())
    }

    /// Parse global item
    fn parse_global_item(&mut self, tag: u8, data: u32) {
        match tag {
            0x00 => self.global.usage_page = data as u16,
            0x01 => self.global.logical_min = data as i32,
            0x02 => self.global.logical_max = data as i32,
            0x03 => self.global.physical_min = data as i32,
            0x04 => self.global.physical_max = data as i32,
            0x07 => self.global.report_size = data,
            0x09 => self.global.report_count = data,
            0x08 => self.global.report_id = data as u8,
            0x0A => {
                // Push
                self.global_stack.push(self.global.clone());
            }
            0x0B => {
                // Pop
                if let Some(state) = self.global_stack.pop() {
                    self.global = state;
                }
            }
            _ => {}
        }
    }

    /// Parse local item
    fn parse_local_item(&mut self, tag: u8, data: u32) {
        match tag {
            0x00 => self.local.usages.push(data as u16),
            0x01 => self.local.usage_min = data as u16,
            0x02 => self.local.usage_max = data as u16,
            _ => {}
        }
    }

    /// Add field to current report
    fn add_field(&mut self, report_type: u8, flags: u32) {
        let is_constant = flags & 0x01 != 0;

        if is_constant {
            // Constant padding, just advance bit offset
            self.bit_offset += self.global.report_size * self.global.report_count;
            return;
        }

        let is_array = flags & 0x02 == 0;
        let is_absolute = flags & 0x04 == 0;
        let has_null = flags & 0x40 != 0;

        // Find or create report
        let report_id = self.global.report_id;
        let report = self.reports.iter_mut()
            .find(|r| r.id == report_id && r.report_type == report_type);

        let report = if let Some(r) = report {
            r
        } else {
            self.reports.push(HidReport {
                id: report_id,
                report_type,
                bit_length: 0,
                fields: Vec::new(),
            });
            self.reports.last_mut().unwrap()
        };

        // Add fields
        for i in 0..self.global.report_count {
            let usage = if is_array {
                0 // Usage is in data for arrays
            } else if i < self.local.usages.len() as u32 {
                self.local.usages[i as usize]
            } else if self.local.usage_min <= self.local.usage_max {
                let usage = self.local.usage_min + i as u16;
                if usage <= self.local.usage_max {
                    usage
                } else {
                    self.local.usages.last().copied().unwrap_or(0)
                }
            } else {
                self.local.usages.last().copied().unwrap_or(0)
            };

            let field = ReportField {
                usage_page: self.global.usage_page,
                usage,
                logical_min: self.global.logical_min,
                logical_max: self.global.logical_max,
                physical_min: self.global.physical_min,
                physical_max: self.global.physical_max,
                report_size: self.global.report_size,
                report_count: 1,
                is_array,
                is_absolute,
                has_null,
                bit_offset: self.bit_offset,
            };

            report.fields.push(field);
            self.bit_offset += self.global.report_size;
        }

        report.bit_length = self.bit_offset;
    }
}

/// USB HID device
pub struct UsbHidDevice {
    /// USB device
    device: Arc<RwLock<UsbDevice>>,
    /// Interface number
    interface: u8,
    /// HID descriptor
    hid_desc: HidDescriptor,
    /// Parsed reports
    reports: Vec<HidReport>,
    /// Boot protocol type
    boot_protocol: BootProtocol,
    /// Using boot protocol
    using_boot: AtomicBool,
    /// Interrupt IN endpoint
    int_in_endpoint: u8,
    /// Last report data
    last_report: Mutex<Vec<u8>>,
    /// Is polling
    polling: AtomicBool,
}

impl UsbHidDevice {
    /// Create new HID device
    pub fn new(
        device: Arc<RwLock<UsbDevice>>,
        interface: u8,
        hid_desc: HidDescriptor,
        boot_protocol: BootProtocol,
        int_in_endpoint: u8,
    ) -> Self {
        Self {
            device,
            interface,
            hid_desc,
            reports: Vec::new(),
            boot_protocol,
            using_boot: AtomicBool::new(true),
            int_in_endpoint,
            last_report: Mutex::new(Vec::new()),
            polling: AtomicBool::new(false),
        }
    }

    /// Get report descriptor
    pub fn get_report_descriptor(&mut self) -> Result<(), UsbError> {
        let length = self.hid_desc.class_descriptor_length as usize;
        let mut buffer = alloc::vec![0u8; length];

        let setup = SetupPacket {
            request_type: 0x81, // Device to host, standard, interface
            request: 0x06,     // GET_DESCRIPTOR
            value: 0x2200,     // Report descriptor
            index: self.interface as u16,
            length: length as u16,
        };

        super::control_transfer(&self.device, &setup, Some(&mut buffer))?;

        // Parse report descriptor
        let mut parser = ReportParser::new();
        self.reports = parser.parse(&buffer)?;

        Ok(())
    }

    /// Set protocol (boot or report)
    pub fn set_protocol(&self, protocol: HidProtocol) -> Result<(), UsbError> {
        let setup = SetupPacket {
            request_type: 0x21, // Host to device, class, interface
            request: HidRequest::SetProtocol as u8,
            value: protocol as u16,
            index: self.interface as u16,
            length: 0,
        };

        super::control_transfer(&self.device, &setup, None)?;
        self.using_boot.store(protocol == HidProtocol::Boot, Ordering::Release);

        Ok(())
    }

    /// Set idle rate
    pub fn set_idle(&self, report_id: u8, duration: u8) -> Result<(), UsbError> {
        let setup = SetupPacket {
            request_type: 0x21,
            request: HidRequest::SetIdle as u8,
            value: ((duration as u16) << 8) | (report_id as u16),
            index: self.interface as u16,
            length: 0,
        };

        super::control_transfer(&self.device, &setup, None)?;
        Ok(())
    }

    /// Get report
    pub fn get_report(&self, report_type: ReportType, report_id: u8) -> Result<Vec<u8>, UsbError> {
        let length = 64; // Max report size
        let mut buffer = alloc::vec![0u8; length];

        let setup = SetupPacket {
            request_type: 0xA1, // Device to host, class, interface
            request: HidRequest::GetReport as u8,
            value: ((report_type as u16) << 8) | (report_id as u16),
            index: self.interface as u16,
            length: length as u16,
        };

        let len = super::control_transfer(&self.device, &setup, Some(&mut buffer))?;
        buffer.truncate(len);

        Ok(buffer)
    }

    /// Set report
    pub fn set_report(&self, report_type: ReportType, report_id: u8, data: &[u8]) -> Result<(), UsbError> {
        let setup = SetupPacket {
            request_type: 0x21, // Host to device, class, interface
            request: HidRequest::SetReport as u8,
            value: ((report_type as u16) << 8) | (report_id as u16),
            index: self.interface as u16,
            length: data.len() as u16,
        };

        let mut buffer = data.to_vec();
        super::control_transfer(&self.device, &setup, Some(&mut buffer))?;

        Ok(())
    }

    /// Poll for input report
    pub fn poll_input(&self) -> Result<Option<Vec<u8>>, UsbError> {
        let mut buffer = alloc::vec![0u8; 64];

        let result = super::interrupt_transfer(
            &self.device,
            self.int_in_endpoint,
            &mut buffer,
            TransferDirection::In,
        );

        match result {
            Ok(len) => {
                buffer.truncate(len);
                if len > 0 {
                    *self.last_report.lock() = buffer.clone();
                    Ok(Some(buffer))
                } else {
                    Ok(None)
                }
            }
            Err(UsbError::Timeout) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Parse boot keyboard report
    pub fn parse_boot_keyboard(&self, data: &[u8]) -> KeyboardReport {
        if data.len() < 8 {
            return KeyboardReport::default();
        }

        KeyboardReport {
            modifiers: data[0],
            reserved: data[1],
            keycodes: [data[2], data[3], data[4], data[5], data[6], data[7]],
        }
    }

    /// Parse boot mouse report
    pub fn parse_boot_mouse(&self, data: &[u8]) -> MouseReport {
        if data.len() < 3 {
            return MouseReport::default();
        }

        MouseReport {
            buttons: data[0],
            x: data[1] as i8,
            y: data[2] as i8,
            wheel: if data.len() > 3 { data[3] as i8 } else { 0 },
        }
    }
}

/// Boot keyboard report
#[derive(Clone, Copy, Debug, Default)]
pub struct KeyboardReport {
    /// Modifier keys
    pub modifiers: u8,
    /// Reserved
    pub reserved: u8,
    /// Keycodes (up to 6)
    pub keycodes: [u8; 6],
}

impl KeyboardReport {
    /// Left control pressed
    pub fn left_ctrl(&self) -> bool { self.modifiers & 0x01 != 0 }
    /// Left shift pressed
    pub fn left_shift(&self) -> bool { self.modifiers & 0x02 != 0 }
    /// Left alt pressed
    pub fn left_alt(&self) -> bool { self.modifiers & 0x04 != 0 }
    /// Left GUI (Windows) pressed
    pub fn left_gui(&self) -> bool { self.modifiers & 0x08 != 0 }
    /// Right control pressed
    pub fn right_ctrl(&self) -> bool { self.modifiers & 0x10 != 0 }
    /// Right shift pressed
    pub fn right_shift(&self) -> bool { self.modifiers & 0x20 != 0 }
    /// Right alt pressed
    pub fn right_alt(&self) -> bool { self.modifiers & 0x40 != 0 }
    /// Right GUI pressed
    pub fn right_gui(&self) -> bool { self.modifiers & 0x80 != 0 }

    /// Get pressed keys
    pub fn pressed_keys(&self) -> Vec<u8> {
        self.keycodes.iter()
            .copied()
            .filter(|&k| k != 0)
            .collect()
    }
}

/// Boot mouse report
#[derive(Clone, Copy, Debug, Default)]
pub struct MouseReport {
    /// Button states
    pub buttons: u8,
    /// X movement
    pub x: i8,
    /// Y movement
    pub y: i8,
    /// Wheel movement
    pub wheel: i8,
}

impl MouseReport {
    /// Left button pressed
    pub fn left_button(&self) -> bool { self.buttons & 0x01 != 0 }
    /// Right button pressed
    pub fn right_button(&self) -> bool { self.buttons & 0x02 != 0 }
    /// Middle button pressed
    pub fn middle_button(&self) -> bool { self.buttons & 0x04 != 0 }
}

/// USB HID Driver
pub struct HidDriver {
    /// Active HID devices
    devices: RwLock<Vec<Arc<UsbHidDevice>>>,
}

impl HidDriver {
    /// Create new HID driver
    pub fn new() -> Self {
        Self {
            devices: RwLock::new(Vec::new()),
        }
    }

    /// Find HID descriptor in interface
    fn find_hid_descriptor(_device: &UsbDevice, _interface: u8) -> Option<HidDescriptor> {
        // Would parse from configuration descriptor
        // For now, return a default
        Some(HidDescriptor {
            length: 9,
            descriptor_type: 0x21,
            bcd_hid: 0x0111,
            country_code: 0,
            num_descriptors: 1,
            class_descriptor_type: 0x22,
            class_descriptor_length: 64,
        })
    }
}

impl UsbDriver for HidDriver {
    fn name(&self) -> &str {
        "usb-hid"
    }

    fn probe(&self, device: &UsbDevice) -> bool {
        device.class() == UsbClass::Hid ||
        device.current_configuration()
            .map(|c| c.interfaces.iter().any(|i| i.class() == UsbClass::Hid))
            .unwrap_or(false)
    }

    fn attach(&self, device: Arc<RwLock<UsbDevice>>) -> Result<(), UsbError> {
        let dev = device.read();

        // Find HID interface
        let config = dev.current_configuration().ok_or(UsbError::InvalidState)?;

        for iface in &config.interfaces {
            if iface.class() != UsbClass::Hid {
                continue;
            }

            let _subclass = HidSubclass::from(iface.interface_subclass);
            let protocol = BootProtocol::from(iface.interface_protocol);

            // Find interrupt IN endpoint
            let int_in = iface.endpoints.iter()
                .find(|e| e.transfer_type() == EndpointTransferType::Interrupt && e.is_in())
                .ok_or(UsbError::NoEndpoint)?;

            // Get HID descriptor
            let hid_desc = Self::find_hid_descriptor(&dev, iface.interface_number)
                .ok_or(UsbError::InvalidDescriptor)?;

            // Extract values before dropping the read lock
            let interface_number = iface.interface_number;
            let endpoint_address = int_in.endpoint_address;
            drop(dev);

            let hid_device = Arc::new(UsbHidDevice::new(
                device.clone(),
                interface_number,
                hid_desc,
                protocol,
                endpoint_address,
            ));

            // Set idle rate to 0 (report only on change)
            let _ = hid_device.set_idle(0, 0);

            let dev = device.read();
            match protocol {
                BootProtocol::Keyboard => {
                    crate::kprintln!("[USB] HID keyboard attached: {}", dev.description());
                }
                BootProtocol::Mouse => {
                    crate::kprintln!("[USB] HID mouse attached: {}", dev.description());
                }
                _ => {
                    crate::kprintln!("[USB] HID device attached: {}", dev.description());
                }
            }
            drop(dev);

            self.devices.write().push(hid_device);
            return Ok(());
        }

        Err(UsbError::InvalidDescriptor)
    }

    fn detach(&self, device: &UsbDevice) {
        let address = device.address;
        self.devices.write().retain(|d| {
            d.device.read().address != address
        });
    }
}

/// HID usage pages
pub mod usage_page {
    pub const GENERIC_DESKTOP: u16 = 0x01;
    pub const SIMULATION: u16 = 0x02;
    pub const VR: u16 = 0x03;
    pub const SPORT: u16 = 0x04;
    pub const GAME: u16 = 0x05;
    pub const GENERIC_DEVICE: u16 = 0x06;
    pub const KEYBOARD: u16 = 0x07;
    pub const LED: u16 = 0x08;
    pub const BUTTON: u16 = 0x09;
    pub const ORDINAL: u16 = 0x0A;
    pub const TELEPHONY: u16 = 0x0B;
    pub const CONSUMER: u16 = 0x0C;
    pub const DIGITIZER: u16 = 0x0D;
    pub const UNICODE: u16 = 0x10;
    pub const ALPHANUMERIC_DISPLAY: u16 = 0x14;
    pub const MEDICAL: u16 = 0x40;
}

/// Generic desktop usages
pub mod usage_desktop {
    pub const POINTER: u16 = 0x01;
    pub const MOUSE: u16 = 0x02;
    pub const JOYSTICK: u16 = 0x04;
    pub const GAMEPAD: u16 = 0x05;
    pub const KEYBOARD: u16 = 0x06;
    pub const KEYPAD: u16 = 0x07;
    pub const X: u16 = 0x30;
    pub const Y: u16 = 0x31;
    pub const Z: u16 = 0x32;
    pub const RX: u16 = 0x33;
    pub const RY: u16 = 0x34;
    pub const RZ: u16 = 0x35;
    pub const WHEEL: u16 = 0x38;
    pub const HAT_SWITCH: u16 = 0x39;
}
