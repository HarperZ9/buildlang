// ===============================================================================
// BUILDOS KERNEL - USB HID (HUMAN INTERFACE DEVICE) DRIVER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================
//
// USB HID class driver supporting keyboards, mice, and generic HID devices.
// Implements HID 1.11 specification.
//
// ===============================================================================

#![allow(dead_code)]

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::AtomicBool;
use spin::Mutex;

use super::{
    UsbDevice, UsbDriver, SetupPacket, EndpointDirection,
    EndpointType, UsbInterface, USB_CLASS_HID,
};

// =============================================================================
// HID CONSTANTS
// =============================================================================

// HID Descriptor Types
const HID_DT_HID: u8 = 0x21;
const HID_DT_REPORT: u8 = 0x22;
const HID_DT_PHYSICAL: u8 = 0x23;

// HID Request Types
const HID_REQ_GET_REPORT: u8 = 0x01;
const HID_REQ_GET_IDLE: u8 = 0x02;
const HID_REQ_GET_PROTOCOL: u8 = 0x03;
const HID_REQ_SET_REPORT: u8 = 0x09;
const HID_REQ_SET_IDLE: u8 = 0x0A;
const HID_REQ_SET_PROTOCOL: u8 = 0x0B;

// HID Report Types
const HID_REPORT_INPUT: u8 = 1;
const HID_REPORT_OUTPUT: u8 = 2;
const HID_REPORT_FEATURE: u8 = 3;

// HID Protocol Values
const HID_PROTOCOL_BOOT: u8 = 0;
const HID_PROTOCOL_REPORT: u8 = 1;

// HID Subclass
const HID_SUBCLASS_NONE: u8 = 0;
const HID_SUBCLASS_BOOT: u8 = 1;

// HID Protocol (for boot interface)
const HID_PROTO_KEYBOARD: u8 = 1;
const HID_PROTO_MOUSE: u8 = 2;

// =============================================================================
// HID DESCRIPTORS
// =============================================================================

/// HID Descriptor
#[derive(Clone, Copy, Debug)]
#[repr(C, packed)]
pub struct HidDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub hid_version: u16,
    pub country_code: u8,
    pub num_descriptors: u8,
    pub report_descriptor_type: u8,
    pub report_descriptor_length: u16,
}

impl HidDescriptor {
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 9 {
            return None;
        }

        Some(Self {
            length: data[0],
            descriptor_type: data[1],
            hid_version: u16::from_le_bytes([data[2], data[3]]),
            country_code: data[4],
            num_descriptors: data[5],
            report_descriptor_type: data[6],
            report_descriptor_length: u16::from_le_bytes([data[7], data[8]]),
        })
    }
}

// =============================================================================
// HID REPORT PARSER
// =============================================================================

/// Report Item Type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportItemType {
    Main,
    Global,
    Local,
    Reserved,
}

/// Report Item Tag (Main items)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainTag {
    Input,
    Output,
    Feature,
    Collection,
    EndCollection,
}

/// Report Item Tag (Global items)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalTag {
    UsagePage,
    LogicalMinimum,
    LogicalMaximum,
    PhysicalMinimum,
    PhysicalMaximum,
    UnitExponent,
    Unit,
    ReportSize,
    ReportId,
    ReportCount,
    Push,
    Pop,
}

/// Report Item Tag (Local items)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalTag {
    Usage,
    UsageMinimum,
    UsageMaximum,
    DesignatorIndex,
    DesignatorMinimum,
    DesignatorMaximum,
    StringIndex,
    StringMinimum,
    StringMaximum,
    Delimiter,
}

/// Collection Type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionType {
    Physical,
    Application,
    Logical,
    Report,
    NamedArray,
    UsageSwitch,
    UsageModifier,
    Reserved(u8),
    VendorDefined(u8),
}

impl From<u8> for CollectionType {
    fn from(val: u8) -> Self {
        match val {
            0x00 => CollectionType::Physical,
            0x01 => CollectionType::Application,
            0x02 => CollectionType::Logical,
            0x03 => CollectionType::Report,
            0x04 => CollectionType::NamedArray,
            0x05 => CollectionType::UsageSwitch,
            0x06 => CollectionType::UsageModifier,
            0x07..=0x7F => CollectionType::Reserved(val),
            _ => CollectionType::VendorDefined(val),
        }
    }
}

/// Usage Page values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsagePage {
    Undefined,
    GenericDesktop,
    Simulation,
    VR,
    Sport,
    Game,
    GenericDevice,
    Keyboard,
    LED,
    Button,
    Ordinal,
    Telephony,
    Consumer,
    Digitizer,
    Unicode,
    AlphanumericDisplay,
    MedicalInstrument,
    Monitor(u8),
    Power(u8),
    BarCodeScanner,
    Scale,
    MagneticStripeReader,
    Camera,
    Arcade,
    VendorDefined(u16),
    Reserved(u16),
}

impl From<u16> for UsagePage {
    fn from(val: u16) -> Self {
        match val {
            0x00 => UsagePage::Undefined,
            0x01 => UsagePage::GenericDesktop,
            0x02 => UsagePage::Simulation,
            0x03 => UsagePage::VR,
            0x04 => UsagePage::Sport,
            0x05 => UsagePage::Game,
            0x06 => UsagePage::GenericDevice,
            0x07 => UsagePage::Keyboard,
            0x08 => UsagePage::LED,
            0x09 => UsagePage::Button,
            0x0A => UsagePage::Ordinal,
            0x0B => UsagePage::Telephony,
            0x0C => UsagePage::Consumer,
            0x0D => UsagePage::Digitizer,
            0x10 => UsagePage::Unicode,
            0x14 => UsagePage::AlphanumericDisplay,
            0x40 => UsagePage::MedicalInstrument,
            0x80..=0x83 => UsagePage::Monitor((val - 0x80) as u8),
            0x84..=0x87 => UsagePage::Power((val - 0x84) as u8),
            0x8C => UsagePage::BarCodeScanner,
            0x8D => UsagePage::Scale,
            0x8E => UsagePage::MagneticStripeReader,
            0x90 => UsagePage::Camera,
            0x91 => UsagePage::Arcade,
            0xFF00..=0xFFFF => UsagePage::VendorDefined(val),
            _ => UsagePage::Reserved(val),
        }
    }
}

/// Generic Desktop Usage values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenericDesktopUsage {
    Undefined,
    Pointer,
    Mouse,
    Joystick,
    GamePad,
    Keyboard,
    Keypad,
    MultiAxisController,
    TabletPCSystemControls,
    X,
    Y,
    Z,
    Rx,
    Ry,
    Rz,
    Slider,
    Dial,
    Wheel,
    HatSwitch,
    SystemControl,
    SystemPowerDown,
    SystemSleep,
    SystemWakeUp,
    Other(u16),
}

impl From<u16> for GenericDesktopUsage {
    fn from(val: u16) -> Self {
        match val {
            0x00 => GenericDesktopUsage::Undefined,
            0x01 => GenericDesktopUsage::Pointer,
            0x02 => GenericDesktopUsage::Mouse,
            0x04 => GenericDesktopUsage::Joystick,
            0x05 => GenericDesktopUsage::GamePad,
            0x06 => GenericDesktopUsage::Keyboard,
            0x07 => GenericDesktopUsage::Keypad,
            0x08 => GenericDesktopUsage::MultiAxisController,
            0x09 => GenericDesktopUsage::TabletPCSystemControls,
            0x30 => GenericDesktopUsage::X,
            0x31 => GenericDesktopUsage::Y,
            0x32 => GenericDesktopUsage::Z,
            0x33 => GenericDesktopUsage::Rx,
            0x34 => GenericDesktopUsage::Ry,
            0x35 => GenericDesktopUsage::Rz,
            0x36 => GenericDesktopUsage::Slider,
            0x37 => GenericDesktopUsage::Dial,
            0x38 => GenericDesktopUsage::Wheel,
            0x39 => GenericDesktopUsage::HatSwitch,
            0x80 => GenericDesktopUsage::SystemControl,
            0x81 => GenericDesktopUsage::SystemPowerDown,
            0x82 => GenericDesktopUsage::SystemSleep,
            0x83 => GenericDesktopUsage::SystemWakeUp,
            _ => GenericDesktopUsage::Other(val),
        }
    }
}

/// Parsed report field
#[derive(Debug, Clone)]
pub struct ReportField {
    pub usage_page: u16,
    pub usage: u16,
    pub logical_min: i32,
    pub logical_max: i32,
    pub physical_min: i32,
    pub physical_max: i32,
    pub report_size: u8,
    pub report_count: u8,
    pub report_id: u8,
    pub flags: u16,
}

/// HID Report Parser
pub struct ReportParser {
    report_descriptor: Vec<u8>,
    input_fields: Vec<ReportField>,
    output_fields: Vec<ReportField>,
    feature_fields: Vec<ReportField>,
}

impl ReportParser {
    pub fn new(descriptor: &[u8]) -> Self {
        let mut parser = Self {
            report_descriptor: descriptor.to_vec(),
            input_fields: Vec::new(),
            output_fields: Vec::new(),
            feature_fields: Vec::new(),
        };
        parser.parse();
        parser
    }

    fn parse(&mut self) {
        let mut pos = 0;
        let data = &self.report_descriptor;

        // Global state
        let mut usage_page: u16 = 0;
        let mut logical_min: i32 = 0;
        let mut logical_max: i32 = 0;
        let mut physical_min: i32 = 0;
        let mut physical_max: i32 = 0;
        let mut report_size: u8 = 0;
        let mut report_count: u8 = 0;
        let mut report_id: u8 = 0;

        // Local state
        let mut usage: u16 = 0;
        let mut _usage_min: u16 = 0;
        let mut _usage_max: u16 = 0;

        while pos < data.len() {
            let prefix = data[pos];
            pos += 1;

            // Long item
            if prefix == 0xFE {
                if pos + 2 > data.len() {
                    break;
                }
                let size = data[pos] as usize;
                pos += 2 + size;
                continue;
            }

            // Short item
            let size = match prefix & 0x03 {
                0 => 0,
                1 => 1,
                2 => 2,
                3 => 4,
                _ => 0,
            };

            if pos + size > data.len() {
                break;
            }

            let value = match size {
                0 => 0i32,
                1 => data[pos] as i8 as i32,
                2 => i16::from_le_bytes([data[pos], data[pos + 1]]) as i32,
                4 => i32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]),
                _ => 0,
            };
            let uvalue = value as u32;

            pos += size;

            let item_type = (prefix >> 2) & 0x03;
            let tag = (prefix >> 4) & 0x0F;

            match item_type {
                // Main items
                0 => match tag {
                    0x08 => {
                        // Input
                        let field = ReportField {
                            usage_page,
                            usage,
                            logical_min,
                            logical_max,
                            physical_min,
                            physical_max,
                            report_size,
                            report_count,
                            report_id,
                            flags: uvalue as u16,
                        };
                        self.input_fields.push(field);
                        // Clear local state
                        usage = 0;
                        _usage_min = 0;
                        _usage_max = 0;
                    }
                    0x09 => {
                        // Output
                        let field = ReportField {
                            usage_page,
                            usage,
                            logical_min,
                            logical_max,
                            physical_min,
                            physical_max,
                            report_size,
                            report_count,
                            report_id,
                            flags: uvalue as u16,
                        };
                        self.output_fields.push(field);
                        usage = 0;
                        _usage_min = 0;
                        _usage_max = 0;
                    }
                    0x0B => {
                        // Feature
                        let field = ReportField {
                            usage_page,
                            usage,
                            logical_min,
                            logical_max,
                            physical_min,
                            physical_max,
                            report_size,
                            report_count,
                            report_id,
                            flags: uvalue as u16,
                        };
                        self.feature_fields.push(field);
                        usage = 0;
                        _usage_min = 0;
                        _usage_max = 0;
                    }
                    0x0A => {
                        // Collection
                    }
                    0x0C => {
                        // End Collection
                    }
                    _ => {}
                },
                // Global items
                1 => match tag {
                    0x00 => usage_page = uvalue as u16,
                    0x01 => logical_min = value,
                    0x02 => logical_max = value,
                    0x03 => physical_min = value,
                    0x04 => physical_max = value,
                    0x07 => report_size = uvalue as u8,
                    0x08 => report_id = uvalue as u8,
                    0x09 => report_count = uvalue as u8,
                    _ => {}
                },
                // Local items
                2 => match tag {
                    0x00 => usage = uvalue as u16,
                    0x01 => _usage_min = uvalue as u16,
                    0x02 => _usage_max = uvalue as u16,
                    _ => {}
                },
                _ => {}
            }
        }
    }

    pub fn input_report_size(&self) -> usize {
        let mut bits = 0usize;
        for field in &self.input_fields {
            bits += (field.report_size as usize) * (field.report_count as usize);
        }
        (bits + 7) / 8
    }
}

// =============================================================================
// KEYBOARD DRIVER
// =============================================================================

/// Keyboard modifier keys
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyboardModifiers {
    pub left_ctrl: bool,
    pub left_shift: bool,
    pub left_alt: bool,
    pub left_gui: bool,
    pub right_ctrl: bool,
    pub right_shift: bool,
    pub right_alt: bool,
    pub right_gui: bool,
}

impl KeyboardModifiers {
    pub fn from_byte(b: u8) -> Self {
        Self {
            left_ctrl: (b & 0x01) != 0,
            left_shift: (b & 0x02) != 0,
            left_alt: (b & 0x04) != 0,
            left_gui: (b & 0x08) != 0,
            right_ctrl: (b & 0x10) != 0,
            right_shift: (b & 0x20) != 0,
            right_alt: (b & 0x40) != 0,
            right_gui: (b & 0x80) != 0,
        }
    }

    pub fn shift(&self) -> bool {
        self.left_shift || self.right_shift
    }

    pub fn ctrl(&self) -> bool {
        self.left_ctrl || self.right_ctrl
    }

    pub fn alt(&self) -> bool {
        self.left_alt || self.right_alt
    }
}

/// Keyboard LED states
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyboardLeds {
    pub num_lock: bool,
    pub caps_lock: bool,
    pub scroll_lock: bool,
    pub compose: bool,
    pub kana: bool,
}

impl KeyboardLeds {
    pub fn to_byte(&self) -> u8 {
        let mut b = 0u8;
        if self.num_lock { b |= 0x01; }
        if self.caps_lock { b |= 0x02; }
        if self.scroll_lock { b |= 0x04; }
        if self.compose { b |= 0x08; }
        if self.kana { b |= 0x10; }
        b
    }
}

/// USB HID Keyboard
pub struct HidKeyboard {
    device: Arc<UsbDevice>,
    interface: u8,
    interrupt_endpoint: u8,
    interval: u8,
    report_buffer: Mutex<[u8; 8]>,
    prev_keys: Mutex<[u8; 6]>,
    modifiers: Mutex<KeyboardModifiers>,
    leds: Mutex<KeyboardLeds>,
    key_queue: Mutex<VecDeque<KeyEvent>>,
    running: AtomicBool,
}

/// Key event
#[derive(Debug, Clone, Copy)]
pub struct KeyEvent {
    pub scancode: u8,
    pub pressed: bool,
    pub modifiers: KeyboardModifiers,
}

impl HidKeyboard {
    pub fn new(device: Arc<UsbDevice>, interface: u8, endpoint: u8, interval: u8) -> Self {
        Self {
            device,
            interface,
            interrupt_endpoint: endpoint,
            interval,
            report_buffer: Mutex::new([0u8; 8]),
            prev_keys: Mutex::new([0u8; 6]),
            modifiers: Mutex::new(KeyboardModifiers::default()),
            leds: Mutex::new(KeyboardLeds::default()),
            key_queue: Mutex::new(VecDeque::with_capacity(64)),
            running: AtomicBool::new(false),
        }
    }

    /// Set boot protocol mode
    pub fn set_boot_protocol(&self) -> bool {
        let setup = SetupPacket::from_raw(
            0x21, // Class, Interface, Host-to-Device
            HID_REQ_SET_PROTOCOL,
            HID_PROTOCOL_BOOT as u16,
            self.interface as u16,
            0,
        );

        self.device.control_transfer(setup, None).is_ok()
    }

    /// Set idle rate
    pub fn set_idle(&self, report_id: u8, duration: u8) -> bool {
        let setup = SetupPacket::from_raw(
            0x21,
            HID_REQ_SET_IDLE,
            ((duration as u16) << 8) | (report_id as u16),
            self.interface as u16,
            0,
        );

        self.device.control_transfer(setup, None).is_ok()
    }

    /// Set LED state
    pub fn set_leds(&self, leds: KeyboardLeds) {
        let mut led_byte = [leds.to_byte()];

        let setup = SetupPacket::from_raw(
            0x21,
            HID_REQ_SET_REPORT,
            ((HID_REPORT_OUTPUT as u16) << 8) | 0,
            self.interface as u16,
            1,
        );

        if self.device.control_transfer(setup, Some(&mut led_byte)).is_ok() {
            *self.leds.lock() = leds;
        }
    }

    /// Poll for keyboard input
    pub fn poll(&self) {
        let mut buffer = [0u8; 8];

        if let Ok(len) = self.device.interrupt_transfer(
            self.interrupt_endpoint,
            EndpointDirection::In,
            &mut buffer,
        ) {
            if len >= 8 {
                self.process_report(&buffer);
            }
        }
    }

    fn process_report(&self, report: &[u8]) {
        let new_modifiers = KeyboardModifiers::from_byte(report[0]);
        let new_keys = &report[2..8];

        let prev_keys = self.prev_keys.lock().clone();
        let mut queue = self.key_queue.lock();

        // Check for key releases
        for &key in prev_keys.iter() {
            if key != 0 && !new_keys.contains(&key) {
                queue.push_back(KeyEvent {
                    scancode: key,
                    pressed: false,
                    modifiers: new_modifiers,
                });
            }
        }

        // Check for key presses
        for &key in new_keys.iter() {
            if key != 0 && !prev_keys.contains(&key) {
                queue.push_back(KeyEvent {
                    scancode: key,
                    pressed: true,
                    modifiers: new_modifiers,
                });
            }
        }

        // Update state
        *self.modifiers.lock() = new_modifiers;
        let mut prev = self.prev_keys.lock();
        prev.copy_from_slice(new_keys);
    }

    /// Get next key event
    pub fn next_event(&self) -> Option<KeyEvent> {
        self.key_queue.lock().pop_front()
    }

    /// Convert HID scancode to ASCII
    pub fn scancode_to_ascii(scancode: u8, modifiers: KeyboardModifiers) -> Option<char> {
        let shift = modifiers.shift();

        match scancode {
            0x04..=0x1D => {
                // a-z
                let base = b'a' + (scancode - 0x04);
                Some(if shift || modifiers.left_gui { base.to_ascii_uppercase() } else { base } as char)
            }
            0x1E..=0x27 => {
                // 1-0
                if shift {
                    Some(['!', '@', '#', '$', '%', '^', '&', '*', '(', ')'][(scancode - 0x1E) as usize])
                } else {
                    Some((b'1' + (scancode - 0x1E).min(8)) as char)
                }
            }
            0x28 => Some('\n'),  // Enter
            0x29 => Some('\x1B'), // Escape
            0x2A => Some('\x08'), // Backspace
            0x2B => Some('\t'),   // Tab
            0x2C => Some(' '),    // Space
            0x2D => Some(if shift { '_' } else { '-' }),
            0x2E => Some(if shift { '+' } else { '=' }),
            0x2F => Some(if shift { '{' } else { '[' }),
            0x30 => Some(if shift { '}' } else { ']' }),
            0x31 => Some(if shift { '|' } else { '\\' }),
            0x33 => Some(if shift { ':' } else { ';' }),
            0x34 => Some(if shift { '"' } else { '\'' }),
            0x35 => Some(if shift { '~' } else { '`' }),
            0x36 => Some(if shift { '<' } else { ',' }),
            0x37 => Some(if shift { '>' } else { '.' }),
            0x38 => Some(if shift { '?' } else { '/' }),
            _ => None,
        }
    }
}

// =============================================================================
// MOUSE DRIVER
// =============================================================================

/// Mouse buttons
#[derive(Debug, Clone, Copy, Default)]
pub struct MouseButtons {
    pub left: bool,
    pub right: bool,
    pub middle: bool,
    pub button4: bool,
    pub button5: bool,
}

impl MouseButtons {
    pub fn from_byte(b: u8) -> Self {
        Self {
            left: (b & 0x01) != 0,
            right: (b & 0x02) != 0,
            middle: (b & 0x04) != 0,
            button4: (b & 0x08) != 0,
            button5: (b & 0x10) != 0,
        }
    }
}

/// Mouse event
#[derive(Debug, Clone, Copy)]
pub struct MouseEvent {
    pub buttons: MouseButtons,
    pub x: i16,
    pub y: i16,
    pub wheel: i8,
}

/// USB HID Mouse
pub struct HidMouse {
    device: Arc<UsbDevice>,
    interface: u8,
    interrupt_endpoint: u8,
    interval: u8,
    buttons: Mutex<MouseButtons>,
    event_queue: Mutex<VecDeque<MouseEvent>>,
    accumulated_x: Mutex<i32>,
    accumulated_y: Mutex<i32>,
    running: AtomicBool,
}

impl HidMouse {
    pub fn new(device: Arc<UsbDevice>, interface: u8, endpoint: u8, interval: u8) -> Self {
        Self {
            device,
            interface,
            interrupt_endpoint: endpoint,
            interval,
            buttons: Mutex::new(MouseButtons::default()),
            event_queue: Mutex::new(VecDeque::with_capacity(64)),
            accumulated_x: Mutex::new(0),
            accumulated_y: Mutex::new(0),
            running: AtomicBool::new(false),
        }
    }

    /// Set boot protocol mode
    pub fn set_boot_protocol(&self) -> bool {
        let setup = SetupPacket::from_raw(
            0x21,
            HID_REQ_SET_PROTOCOL,
            HID_PROTOCOL_BOOT as u16,
            self.interface as u16,
            0,
        );

        self.device.control_transfer(setup, None).is_ok()
    }

    /// Poll for mouse input
    pub fn poll(&self) {
        let mut buffer = [0u8; 8];

        if let Ok(len) = self.device.interrupt_transfer(
            self.interrupt_endpoint,
            EndpointDirection::In,
            &mut buffer,
        ) {
            if len >= 3 {
                self.process_report(&buffer[..len]);
            }
        }
    }

    fn process_report(&self, report: &[u8]) {
        let buttons = MouseButtons::from_byte(report[0]);
        let x = report[1] as i8 as i16;
        let y = report[2] as i8 as i16;
        let wheel = if report.len() > 3 { report[3] as i8 } else { 0 };

        let event = MouseEvent { buttons, x, y, wheel };

        self.event_queue.lock().push_back(event);
        *self.buttons.lock() = buttons;

        // Accumulate movement
        *self.accumulated_x.lock() += x as i32;
        *self.accumulated_y.lock() += y as i32;
    }

    /// Get next mouse event
    pub fn next_event(&self) -> Option<MouseEvent> {
        self.event_queue.lock().pop_front()
    }

    /// Get current button state
    pub fn buttons(&self) -> MouseButtons {
        *self.buttons.lock()
    }

    /// Get accumulated movement and reset
    pub fn take_movement(&self) -> (i32, i32) {
        let x = core::mem::replace(&mut *self.accumulated_x.lock(), 0);
        let y = core::mem::replace(&mut *self.accumulated_y.lock(), 0);
        (x, y)
    }
}

// =============================================================================
// HID DRIVER
// =============================================================================

/// HID Device Type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidDeviceType {
    Keyboard,
    Mouse,
    Joystick,
    Gamepad,
    Generic,
}

/// HID Driver
pub struct HidDriver {
    keyboards: Mutex<Vec<Arc<HidKeyboard>>>,
    mice: Mutex<Vec<Arc<HidMouse>>>,
}

impl HidDriver {
    pub const fn new() -> Self {
        Self {
            keyboards: Mutex::new(Vec::new()),
            mice: Mutex::new(Vec::new()),
        }
    }

    fn detect_device_type(interface: &UsbInterface) -> HidDeviceType {
        if interface.subclass == HID_SUBCLASS_BOOT {
            match interface.protocol {
                HID_PROTO_KEYBOARD => HidDeviceType::Keyboard,
                HID_PROTO_MOUSE => HidDeviceType::Mouse,
                _ => HidDeviceType::Generic,
            }
        } else {
            HidDeviceType::Generic
        }
    }

    /// Get all keyboards
    pub fn keyboards(&self) -> Vec<Arc<HidKeyboard>> {
        self.keyboards.lock().clone()
    }

    /// Get all mice
    pub fn mice(&self) -> Vec<Arc<HidMouse>> {
        self.mice.lock().clone()
    }

    /// Poll all HID devices
    pub fn poll_all(&self) {
        for keyboard in self.keyboards.lock().iter() {
            keyboard.poll();
        }
        for mouse in self.mice.lock().iter() {
            mouse.poll();
        }
    }
}

impl UsbDriver for HidDriver {
    fn name(&self) -> &'static str {
        "HID"
    }

    fn probe(&self, interface: &UsbInterface) -> bool {
        interface.class == USB_CLASS_HID
    }

    fn attach(&self, device: Arc<UsbDevice>, interface_num: u8) -> Result<(), &'static str> {
        let interface = match device.interface(interface_num) {
            Some(i) => i,
            None => return Err("Interface not found"),
        };

        let device_type = Self::detect_device_type(&interface);

        crate::log::info!("HID: Attaching {:?} device", device_type);

        // Find interrupt IN endpoint
        let mut interrupt_ep = None;
        let mut interval = 10u8;

        for ep in device.endpoints(interface_num) {
            if ep.transfer_type() == EndpointType::Interrupt
                && ep.direction() == EndpointDirection::In
            {
                interrupt_ep = Some(ep.number());
                interval = ep.interval;
                break;
            }
        }

        let endpoint = match interrupt_ep {
            Some(ep) => ep,
            None => {
                crate::log::warn!("HID: No interrupt IN endpoint found");
                return Err("No interrupt IN endpoint found");
            }
        };

        match device_type {
            HidDeviceType::Keyboard => {
                let keyboard = Arc::new(HidKeyboard::new(
                    device.clone(),
                    interface_num,
                    endpoint,
                    interval,
                ));

                // Set boot protocol and idle
                keyboard.set_boot_protocol();
                keyboard.set_idle(0, 0);

                self.keyboards.lock().push(keyboard);
                crate::log::info!("HID: Keyboard attached");
                Ok(())
            }
            HidDeviceType::Mouse => {
                let mouse = Arc::new(HidMouse::new(
                    device.clone(),
                    interface_num,
                    endpoint,
                    interval,
                ));

                mouse.set_boot_protocol();

                self.mice.lock().push(mouse);
                crate::log::info!("HID: Mouse attached");
                Ok(())
            }
            _ => {
                crate::log::info!("HID: Generic device (not fully supported)");
                Err("Unsupported HID device type")
            }
        }
    }

    fn detach(&self, device: Arc<UsbDevice>, _interface_num: u8) {
        let address = device.address;

        self.keyboards.lock().retain(|k| k.device.address != address);
        self.mice.lock().retain(|m| m.device.address != address);

        crate::log::info!("HID: Device detached");
    }
}

// =============================================================================
// GLOBAL HID DRIVER INSTANCE
// =============================================================================

static HID_DRIVER: HidDriver = HidDriver::new();

pub fn driver() -> &'static HidDriver {
    &HID_DRIVER
}

/// Initialize HID subsystem
pub fn init() {
    crate::log::info!("HID: Subsystem initialized");
}
