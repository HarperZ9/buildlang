//! Bluetooth HID (Human Interface Device) Profile
//!
//! This module implements the Bluetooth HID profile for keyboards,
//! mice, gamepads, and other input devices.

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use spin::{Mutex, RwLock};

use super::{BluetoothError, BdAddr};
use super::l2cap::{L2capChannel, L2capManager, PSM_HID_CONTROL, PSM_HID_INTERRUPT};

/// HID protocol modes
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidProtocol {
    /// Boot protocol mode (simplified for BIOS)
    Boot = 0x00,
    /// Report protocol mode (full feature set)
    Report = 0x01,
}

/// HID report types
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidReportType {
    /// Reserved
    Reserved = 0x00,
    /// Input report (device to host)
    Input = 0x01,
    /// Output report (host to device)
    Output = 0x02,
    /// Feature report (bidirectional)
    Feature = 0x03,
}

/// HID handshake result codes
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidHandshake {
    /// Successful
    Successful = 0x00,
    /// Not ready
    NotReady = 0x01,
    /// Invalid report ID
    ErrInvalidReportId = 0x02,
    /// Unsupported request
    ErrUnsupportedRequest = 0x03,
    /// Invalid parameter
    ErrInvalidParameter = 0x04,
    /// Unknown error
    ErrUnknown = 0x0E,
    /// Fatal error
    ErrFatal = 0x0F,
}

/// HID transaction message types
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidMessageType {
    /// Handshake message
    Handshake = 0x00,
    /// HID control message
    HidControl = 0x01,
    /// Reserved
    Reserved2 = 0x02,
    /// Reserved
    Reserved3 = 0x03,
    /// Get report request
    GetReport = 0x04,
    /// Set report request
    SetReport = 0x05,
    /// Get protocol request
    GetProtocol = 0x06,
    /// Set protocol request
    SetProtocol = 0x07,
    /// Get idle request
    GetIdle = 0x08,
    /// Set idle request
    SetIdle = 0x09,
    /// Data message (output)
    DataOutput = 0x0A,
    /// Data message (input)
    DataInput = 0x0B,
    /// Datc message
    Datc = 0x0C,
}

/// HID control operations
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidControlOp {
    /// No operation
    Nop = 0x00,
    /// Hard reset
    HardReset = 0x01,
    /// Soft reset
    SoftReset = 0x02,
    /// Suspend
    Suspend = 0x03,
    /// Exit suspend
    ExitSuspend = 0x04,
    /// Virtual cable unplug
    VirtualCableUnplug = 0x05,
}

/// HID device subclass
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidDeviceSubclass {
    /// No subclass
    None = 0x00,
    /// Boot interface subclass
    BootInterface = 0x01,
}

/// HID device types (for boot interface)
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidDeviceType {
    /// Generic HID device
    Generic = 0x00,
    /// Keyboard
    Keyboard = 0x01,
    /// Mouse
    Mouse = 0x02,
    /// Reserved (3-255)
    Reserved = 0x03,
}

/// HID usage page definitions
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidUsagePage {
    /// Generic desktop controls
    GenericDesktop = 0x01,
    /// Simulation controls
    Simulation = 0x02,
    /// VR controls
    Vr = 0x03,
    /// Sport controls
    Sport = 0x04,
    /// Game controls
    Game = 0x05,
    /// Generic device controls
    GenericDevice = 0x06,
    /// Keyboard/Keypad
    Keyboard = 0x07,
    /// LEDs
    Led = 0x08,
    /// Button
    Button = 0x09,
    /// Ordinal
    Ordinal = 0x0A,
    /// Telephony
    Telephony = 0x0B,
    /// Consumer
    Consumer = 0x0C,
    /// Digitizer
    Digitizer = 0x0D,
    /// Haptics
    Haptics = 0x0E,
    /// Physical input device
    Pid = 0x0F,
    /// Unicode
    Unicode = 0x10,
    /// Eye and head tracker
    EyeHeadTracker = 0x12,
    /// Auxiliary display
    AuxDisplay = 0x14,
    /// Sensors
    Sensor = 0x20,
    /// Medical instrument
    Medical = 0x40,
    /// Braille display
    Braille = 0x41,
    /// Lighting and illumination
    Lighting = 0x59,
    /// Monitor
    Monitor = 0x80,
    /// Monitor enumerated
    MonitorEnum = 0x81,
    /// VESA virtual controls
    VesaVc = 0x82,
    /// Power device
    Power = 0x84,
    /// Battery system
    Battery = 0x85,
    /// Bar code scanner
    BarCode = 0x8C,
    /// Weighing device
    Scale = 0x8D,
    /// Magnetic stripe reader
    Msr = 0x8E,
    /// Camera control
    Camera = 0x90,
    /// Arcade
    Arcade = 0x91,
    /// Gaming device
    Gaming = 0x92,
    /// FIDO alliance
    Fido = 0xF1D0,
    /// Vendor defined start
    VendorMin = 0xFF00,
    /// Vendor defined end
    VendorMax = 0xFFFF,
}

/// Generic desktop usage IDs
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenericDesktopUsage {
    /// Pointer
    Pointer = 0x01,
    /// Mouse
    Mouse = 0x02,
    /// Reserved
    Reserved = 0x03,
    /// Joystick
    Joystick = 0x04,
    /// Gamepad
    Gamepad = 0x05,
    /// Keyboard
    Keyboard = 0x06,
    /// Keypad
    Keypad = 0x07,
    /// Multi-axis controller
    MultiAxis = 0x08,
    /// Tablet PC system controls
    TabletPc = 0x09,
    /// Water cooling device
    WaterCooling = 0x0A,
    /// Computer chassis device
    ChassisDev = 0x0B,
    /// Wireless radio controls
    WirelessRadio = 0x0C,
    /// Portable device control
    PortableDevice = 0x0D,
    /// System multi-axis controller
    SystemMultiAxis = 0x0E,
    /// Spatial controller
    Spatial = 0x0F,
    /// X axis
    X = 0x30,
    /// Y axis
    Y = 0x31,
    /// Z axis
    Z = 0x32,
    /// Rx axis (rotation)
    Rx = 0x33,
    /// Ry axis (rotation)
    Ry = 0x34,
    /// Rz axis (rotation)
    Rz = 0x35,
    /// Slider
    Slider = 0x36,
    /// Dial
    Dial = 0x37,
    /// Wheel
    Wheel = 0x38,
    /// Hat switch
    HatSwitch = 0x39,
    /// Counted buffer
    CountedBuffer = 0x3A,
    /// Byte count
    ByteCount = 0x3B,
    /// Motion wakeup
    MotionWakeup = 0x3C,
    /// Start
    Start = 0x3D,
    /// Select
    Select = 0x3E,
}

/// Keyboard modifier keys bitmap
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyboardModifiers(pub u8);

impl KeyboardModifiers {
    pub const LEFT_CTRL: u8 = 0x01;
    pub const LEFT_SHIFT: u8 = 0x02;
    pub const LEFT_ALT: u8 = 0x04;
    pub const LEFT_GUI: u8 = 0x08;
    pub const RIGHT_CTRL: u8 = 0x10;
    pub const RIGHT_SHIFT: u8 = 0x20;
    pub const RIGHT_ALT: u8 = 0x40;
    pub const RIGHT_GUI: u8 = 0x80;

    pub fn new() -> Self {
        Self(0)
    }

    pub fn left_ctrl(&self) -> bool {
        self.0 & Self::LEFT_CTRL != 0
    }

    pub fn left_shift(&self) -> bool {
        self.0 & Self::LEFT_SHIFT != 0
    }

    pub fn left_alt(&self) -> bool {
        self.0 & Self::LEFT_ALT != 0
    }

    pub fn left_gui(&self) -> bool {
        self.0 & Self::LEFT_GUI != 0
    }

    pub fn right_ctrl(&self) -> bool {
        self.0 & Self::RIGHT_CTRL != 0
    }

    pub fn right_shift(&self) -> bool {
        self.0 & Self::RIGHT_SHIFT != 0
    }

    pub fn right_alt(&self) -> bool {
        self.0 & Self::RIGHT_ALT != 0
    }

    pub fn right_gui(&self) -> bool {
        self.0 & Self::RIGHT_GUI != 0
    }

    pub fn any_ctrl(&self) -> bool {
        self.0 & (Self::LEFT_CTRL | Self::RIGHT_CTRL) != 0
    }

    pub fn any_shift(&self) -> bool {
        self.0 & (Self::LEFT_SHIFT | Self::RIGHT_SHIFT) != 0
    }

    pub fn any_alt(&self) -> bool {
        self.0 & (Self::LEFT_ALT | Self::RIGHT_ALT) != 0
    }

    pub fn any_gui(&self) -> bool {
        self.0 & (Self::LEFT_GUI | Self::RIGHT_GUI) != 0
    }
}

/// Mouse button bitmap
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseButtons(pub u8);

impl MouseButtons {
    pub const LEFT: u8 = 0x01;
    pub const RIGHT: u8 = 0x02;
    pub const MIDDLE: u8 = 0x04;
    pub const BACK: u8 = 0x08;
    pub const FORWARD: u8 = 0x10;

    pub fn new() -> Self {
        Self(0)
    }

    pub fn left(&self) -> bool {
        self.0 & Self::LEFT != 0
    }

    pub fn right(&self) -> bool {
        self.0 & Self::RIGHT != 0
    }

    pub fn middle(&self) -> bool {
        self.0 & Self::MIDDLE != 0
    }

    pub fn back(&self) -> bool {
        self.0 & Self::BACK != 0
    }

    pub fn forward(&self) -> bool {
        self.0 & Self::FORWARD != 0
    }
}

/// Keyboard LED bitmap
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyboardLeds(pub u8);

impl KeyboardLeds {
    pub const NUM_LOCK: u8 = 0x01;
    pub const CAPS_LOCK: u8 = 0x02;
    pub const SCROLL_LOCK: u8 = 0x04;
    pub const COMPOSE: u8 = 0x08;
    pub const KANA: u8 = 0x10;

    pub fn new() -> Self {
        Self(0)
    }

    pub fn num_lock(&self) -> bool {
        self.0 & Self::NUM_LOCK != 0
    }

    pub fn caps_lock(&self) -> bool {
        self.0 & Self::CAPS_LOCK != 0
    }

    pub fn scroll_lock(&self) -> bool {
        self.0 & Self::SCROLL_LOCK != 0
    }

    pub fn compose(&self) -> bool {
        self.0 & Self::COMPOSE != 0
    }

    pub fn kana(&self) -> bool {
        self.0 & Self::KANA != 0
    }
}

/// Boot protocol keyboard report
#[derive(Debug, Clone, Copy)]
pub struct BootKeyboardReport {
    /// Modifier keys (Ctrl, Shift, Alt, GUI)
    pub modifiers: KeyboardModifiers,
    /// Reserved byte
    pub reserved: u8,
    /// Up to 6 key codes currently pressed
    pub keys: [u8; 6],
}

impl BootKeyboardReport {
    pub fn new() -> Self {
        Self {
            modifiers: KeyboardModifiers::new(),
            reserved: 0,
            keys: [0; 6],
        }
    }

    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 8 {
            return None;
        }
        Some(Self {
            modifiers: KeyboardModifiers(data[0]),
            reserved: data[1],
            keys: [data[2], data[3], data[4], data[5], data[6], data[7]],
        })
    }

    pub fn to_bytes(&self) -> [u8; 8] {
        [
            self.modifiers.0,
            self.reserved,
            self.keys[0],
            self.keys[1],
            self.keys[2],
            self.keys[3],
            self.keys[4],
            self.keys[5],
        ]
    }

    /// Check if a key is pressed
    pub fn is_key_pressed(&self, keycode: u8) -> bool {
        self.keys.contains(&keycode) && keycode != 0
    }

    /// Get all pressed keys
    pub fn pressed_keys(&self) -> Vec<u8> {
        self.keys.iter().copied().filter(|&k| k != 0).collect()
    }
}

/// Boot protocol mouse report
#[derive(Debug, Clone, Copy)]
pub struct BootMouseReport {
    /// Button states
    pub buttons: MouseButtons,
    /// X displacement (signed)
    pub x: i8,
    /// Y displacement (signed)
    pub y: i8,
}

impl BootMouseReport {
    pub fn new() -> Self {
        Self {
            buttons: MouseButtons::new(),
            x: 0,
            y: 0,
        }
    }

    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 3 {
            return None;
        }
        Some(Self {
            buttons: MouseButtons(data[0]),
            x: data[1] as i8,
            y: data[2] as i8,
        })
    }

    pub fn to_bytes(&self) -> [u8; 3] {
        [self.buttons.0, self.x as u8, self.y as u8]
    }
}

/// Extended mouse report (with wheel)
#[derive(Debug, Clone, Copy)]
pub struct ExtendedMouseReport {
    /// Button states
    pub buttons: MouseButtons,
    /// X displacement (signed)
    pub x: i8,
    /// Y displacement (signed)
    pub y: i8,
    /// Vertical wheel (signed)
    pub wheel: i8,
    /// Horizontal wheel (signed)
    pub hwheel: i8,
}

impl ExtendedMouseReport {
    pub fn new() -> Self {
        Self {
            buttons: MouseButtons::new(),
            x: 0,
            y: 0,
            wheel: 0,
            hwheel: 0,
        }
    }

    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 5 {
            return None;
        }
        Some(Self {
            buttons: MouseButtons(data[0]),
            x: data[1] as i8,
            y: data[2] as i8,
            wheel: data[3] as i8,
            hwheel: data[4] as i8,
        })
    }

    pub fn to_bytes(&self) -> [u8; 5] {
        [
            self.buttons.0,
            self.x as u8,
            self.y as u8,
            self.wheel as u8,
            self.hwheel as u8,
        ]
    }
}

/// Gamepad report
#[derive(Debug, Clone, Copy)]
pub struct GamepadReport {
    /// Digital buttons (16 buttons)
    pub buttons: u16,
    /// Left stick X (-128 to 127)
    pub left_x: i8,
    /// Left stick Y (-128 to 127)
    pub left_y: i8,
    /// Right stick X (-128 to 127)
    pub right_x: i8,
    /// Right stick Y (-128 to 127)
    pub right_y: i8,
    /// Left trigger (0-255)
    pub left_trigger: u8,
    /// Right trigger (0-255)
    pub right_trigger: u8,
    /// D-pad / Hat switch (0-8, 0=neutral)
    pub dpad: u8,
}

impl GamepadReport {
    pub const BTN_A: u16 = 0x0001;
    pub const BTN_B: u16 = 0x0002;
    pub const BTN_X: u16 = 0x0004;
    pub const BTN_Y: u16 = 0x0008;
    pub const BTN_LB: u16 = 0x0010;
    pub const BTN_RB: u16 = 0x0020;
    pub const BTN_BACK: u16 = 0x0040;
    pub const BTN_START: u16 = 0x0080;
    pub const BTN_GUIDE: u16 = 0x0100;
    pub const BTN_LS: u16 = 0x0200;
    pub const BTN_RS: u16 = 0x0400;

    pub fn new() -> Self {
        Self {
            buttons: 0,
            left_x: 0,
            left_y: 0,
            right_x: 0,
            right_y: 0,
            left_trigger: 0,
            right_trigger: 0,
            dpad: 0,
        }
    }

    pub fn button_pressed(&self, button: u16) -> bool {
        self.buttons & button != 0
    }
}

/// HID report descriptor item types
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidItemType {
    /// Main item
    Main = 0x00,
    /// Global item
    Global = 0x01,
    /// Local item
    Local = 0x02,
    /// Reserved
    Reserved = 0x03,
}

/// HID main item tags
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidMainTag {
    /// Input item
    Input = 0x08,
    /// Output item
    Output = 0x09,
    /// Collection start
    Collection = 0x0A,
    /// Feature item
    Feature = 0x0B,
    /// End collection
    EndCollection = 0x0C,
}

/// HID global item tags
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidGlobalTag {
    /// Usage page
    UsagePage = 0x00,
    /// Logical minimum
    LogicalMinimum = 0x01,
    /// Logical maximum
    LogicalMaximum = 0x02,
    /// Physical minimum
    PhysicalMinimum = 0x03,
    /// Physical maximum
    PhysicalMaximum = 0x04,
    /// Unit exponent
    UnitExponent = 0x05,
    /// Unit
    Unit = 0x06,
    /// Report size (in bits)
    ReportSize = 0x07,
    /// Report ID
    ReportId = 0x08,
    /// Report count
    ReportCount = 0x09,
    /// Push state
    Push = 0x0A,
    /// Pop state
    Pop = 0x0B,
}

/// HID local item tags
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidLocalTag {
    /// Usage
    Usage = 0x00,
    /// Usage minimum
    UsageMinimum = 0x01,
    /// Usage maximum
    UsageMaximum = 0x02,
    /// Designator index
    DesignatorIndex = 0x03,
    /// Designator minimum
    DesignatorMinimum = 0x04,
    /// Designator maximum
    DesignatorMaximum = 0x05,
    /// String index
    StringIndex = 0x07,
    /// String minimum
    StringMinimum = 0x08,
    /// String maximum
    StringMaximum = 0x09,
    /// Delimiter
    Delimiter = 0x0A,
}

/// HID collection types
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidCollectionType {
    /// Physical (group of axes)
    Physical = 0x00,
    /// Application (mouse, keyboard)
    Application = 0x01,
    /// Logical (interrelated data)
    Logical = 0x02,
    /// Report
    Report = 0x03,
    /// Named array
    NamedArray = 0x04,
    /// Usage switch
    UsageSwitch = 0x05,
    /// Usage modifier
    UsageModifier = 0x06,
}

/// HID report descriptor item
#[derive(Debug, Clone)]
pub struct HidItem {
    /// Item type
    pub item_type: HidItemType,
    /// Item tag
    pub tag: u8,
    /// Item data
    pub data: Vec<u8>,
}

impl HidItem {
    pub fn new(item_type: HidItemType, tag: u8, data: Vec<u8>) -> Self {
        Self {
            item_type,
            tag,
            data,
        }
    }

    /// Encode item to bytes
    pub fn encode(&self) -> Vec<u8> {
        let mut result = Vec::new();
        let size = match self.data.len() {
            0 => 0,
            1 => 1,
            2 => 2,
            _ => 3, // 4-byte data
        };

        let header = (self.tag << 4) | ((self.item_type as u8) << 2) | size;
        result.push(header);
        result.extend_from_slice(&self.data);
        result
    }

    /// Decode item from bytes
    pub fn decode(data: &[u8]) -> Option<(Self, usize)> {
        if data.is_empty() {
            return None;
        }

        let header = data[0];
        let size = match header & 0x03 {
            0 => 0,
            1 => 1,
            2 => 2,
            3 => 4,
            _ => return None,
        };

        if data.len() < 1 + size {
            return None;
        }

        let item_type = match (header >> 2) & 0x03 {
            0 => HidItemType::Main,
            1 => HidItemType::Global,
            2 => HidItemType::Local,
            _ => HidItemType::Reserved,
        };

        let tag = (header >> 4) & 0x0F;
        let item_data = data[1..1 + size].to_vec();

        Some((Self::new(item_type, tag, item_data), 1 + size))
    }

    /// Get data as signed integer
    pub fn as_signed(&self) -> i32 {
        match self.data.len() {
            0 => 0,
            1 => self.data[0] as i8 as i32,
            2 => i16::from_le_bytes([self.data[0], self.data[1]]) as i32,
            _ => i32::from_le_bytes([
                self.data[0],
                self.data.get(1).copied().unwrap_or(0),
                self.data.get(2).copied().unwrap_or(0),
                self.data.get(3).copied().unwrap_or(0),
            ]),
        }
    }

    /// Get data as unsigned integer
    pub fn as_unsigned(&self) -> u32 {
        match self.data.len() {
            0 => 0,
            1 => self.data[0] as u32,
            2 => u16::from_le_bytes([self.data[0], self.data[1]]) as u32,
            _ => u32::from_le_bytes([
                self.data[0],
                self.data.get(1).copied().unwrap_or(0),
                self.data.get(2).copied().unwrap_or(0),
                self.data.get(3).copied().unwrap_or(0),
            ]),
        }
    }
}

/// HID report descriptor parser state
#[derive(Debug, Clone)]
pub struct HidParserState {
    /// Current usage page
    pub usage_page: u16,
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
    /// Report ID
    pub report_id: u8,
    /// Current usages
    pub usages: Vec<u32>,
    /// Usage minimum
    pub usage_min: u32,
    /// Usage maximum
    pub usage_max: u32,
}

impl HidParserState {
    pub fn new() -> Self {
        Self {
            usage_page: 0,
            logical_min: 0,
            logical_max: 0,
            physical_min: 0,
            physical_max: 0,
            report_size: 0,
            report_count: 0,
            report_id: 0,
            usages: Vec::new(),
            usage_min: 0,
            usage_max: 0,
        }
    }
}

/// Parsed HID report field
#[derive(Debug, Clone)]
pub struct HidField {
    /// Report ID
    pub report_id: u8,
    /// Usage page
    pub usage_page: u16,
    /// Usage(s)
    pub usages: Vec<u32>,
    /// Logical minimum
    pub logical_min: i32,
    /// Logical maximum
    pub logical_max: i32,
    /// Physical minimum
    pub physical_min: i32,
    /// Physical maximum
    pub physical_max: i32,
    /// Size in bits
    pub size: u32,
    /// Count
    pub count: u32,
    /// Flags from Input/Output/Feature
    pub flags: u32,
}

/// HID report descriptor
#[derive(Debug, Clone)]
pub struct HidReportDescriptor {
    /// Raw descriptor bytes
    pub raw: Vec<u8>,
    /// Parsed items
    pub items: Vec<HidItem>,
    /// Input fields
    pub input_fields: Vec<HidField>,
    /// Output fields
    pub output_fields: Vec<HidField>,
    /// Feature fields
    pub feature_fields: Vec<HidField>,
}

impl HidReportDescriptor {
    pub fn new(raw: Vec<u8>) -> Self {
        let mut descriptor = Self {
            raw: raw.clone(),
            items: Vec::new(),
            input_fields: Vec::new(),
            output_fields: Vec::new(),
            feature_fields: Vec::new(),
        };
        descriptor.parse();
        descriptor
    }

    /// Parse the raw descriptor
    fn parse(&mut self) {
        let mut offset = 0;
        while offset < self.raw.len() {
            if let Some((item, size)) = HidItem::decode(&self.raw[offset..]) {
                self.items.push(item);
                offset += size;
            } else {
                break;
            }
        }

        // Parse items into fields
        let mut state = HidParserState::new();
        let mut state_stack: Vec<HidParserState> = Vec::new();

        for item in &self.items {
            match item.item_type {
                HidItemType::Global => {
                    match item.tag {
                        0x00 => state.usage_page = item.as_unsigned() as u16,
                        0x01 => state.logical_min = item.as_signed(),
                        0x02 => state.logical_max = item.as_signed(),
                        0x03 => state.physical_min = item.as_signed(),
                        0x04 => state.physical_max = item.as_signed(),
                        0x07 => state.report_size = item.as_unsigned(),
                        0x08 => state.report_id = item.as_unsigned() as u8,
                        0x09 => state.report_count = item.as_unsigned(),
                        0x0A => state_stack.push(state.clone()),
                        0x0B => {
                            if let Some(s) = state_stack.pop() {
                                state = s;
                            }
                        }
                        _ => {}
                    }
                }
                HidItemType::Local => {
                    match item.tag {
                        0x00 => {
                            let usage = item.as_unsigned();
                            state.usages.push(usage);
                        }
                        0x01 => state.usage_min = item.as_unsigned(),
                        0x02 => state.usage_max = item.as_unsigned(),
                        _ => {}
                    }
                }
                HidItemType::Main => {
                    let flags = item.as_unsigned();
                    let field = HidField {
                        report_id: state.report_id,
                        usage_page: state.usage_page,
                        usages: if state.usages.is_empty() {
                            (state.usage_min..=state.usage_max).collect()
                        } else {
                            state.usages.clone()
                        },
                        logical_min: state.logical_min,
                        logical_max: state.logical_max,
                        physical_min: state.physical_min,
                        physical_max: state.physical_max,
                        size: state.report_size,
                        count: state.report_count,
                        flags,
                    };

                    match item.tag {
                        0x08 => self.input_fields.push(field),
                        0x09 => self.output_fields.push(field),
                        0x0B => self.feature_fields.push(field),
                        _ => {}
                    }

                    // Clear local state
                    state.usages.clear();
                    state.usage_min = 0;
                    state.usage_max = 0;
                }
                _ => {}
            }
        }
    }

    /// Create a boot keyboard descriptor
    pub fn boot_keyboard() -> Self {
        let raw = vec![
            0x05, 0x01, // Usage Page (Generic Desktop)
            0x09, 0x06, // Usage (Keyboard)
            0xA1, 0x01, // Collection (Application)
            0x05, 0x07, //   Usage Page (Keyboard)
            0x19, 0xE0, //   Usage Minimum (Left Control)
            0x29, 0xE7, //   Usage Maximum (Right GUI)
            0x15, 0x00, //   Logical Minimum (0)
            0x25, 0x01, //   Logical Maximum (1)
            0x75, 0x01, //   Report Size (1)
            0x95, 0x08, //   Report Count (8)
            0x81, 0x02, //   Input (Data, Variable, Absolute)
            0x95, 0x01, //   Report Count (1)
            0x75, 0x08, //   Report Size (8)
            0x81, 0x01, //   Input (Constant)
            0x95, 0x05, //   Report Count (5)
            0x75, 0x01, //   Report Size (1)
            0x05, 0x08, //   Usage Page (LEDs)
            0x19, 0x01, //   Usage Minimum (Num Lock)
            0x29, 0x05, //   Usage Maximum (Kana)
            0x91, 0x02, //   Output (Data, Variable, Absolute)
            0x95, 0x01, //   Report Count (1)
            0x75, 0x03, //   Report Size (3)
            0x91, 0x01, //   Output (Constant)
            0x95, 0x06, //   Report Count (6)
            0x75, 0x08, //   Report Size (8)
            0x15, 0x00, //   Logical Minimum (0)
            0x26, 0xFF, 0x00, // Logical Maximum (255)
            0x05, 0x07, //   Usage Page (Keyboard)
            0x19, 0x00, //   Usage Minimum (0)
            0x29, 0xFF, //   Usage Maximum (255)
            0x81, 0x00, //   Input (Data, Array)
            0xC0,       // End Collection
        ];
        Self::new(raw)
    }

    /// Create a boot mouse descriptor
    pub fn boot_mouse() -> Self {
        let raw = vec![
            0x05, 0x01, // Usage Page (Generic Desktop)
            0x09, 0x02, // Usage (Mouse)
            0xA1, 0x01, // Collection (Application)
            0x09, 0x01, //   Usage (Pointer)
            0xA1, 0x00, //   Collection (Physical)
            0x05, 0x09, //     Usage Page (Buttons)
            0x19, 0x01, //     Usage Minimum (Button 1)
            0x29, 0x03, //     Usage Maximum (Button 3)
            0x15, 0x00, //     Logical Minimum (0)
            0x25, 0x01, //     Logical Maximum (1)
            0x95, 0x03, //     Report Count (3)
            0x75, 0x01, //     Report Size (1)
            0x81, 0x02, //     Input (Data, Variable, Absolute)
            0x95, 0x01, //     Report Count (1)
            0x75, 0x05, //     Report Size (5)
            0x81, 0x01, //     Input (Constant)
            0x05, 0x01, //     Usage Page (Generic Desktop)
            0x09, 0x30, //     Usage (X)
            0x09, 0x31, //     Usage (Y)
            0x15, 0x81, //     Logical Minimum (-127)
            0x25, 0x7F, //     Logical Maximum (127)
            0x75, 0x08, //     Report Size (8)
            0x95, 0x02, //     Report Count (2)
            0x81, 0x06, //     Input (Data, Variable, Relative)
            0xC0,       //   End Collection
            0xC0,       // End Collection
        ];
        Self::new(raw)
    }
}

/// HID connection state
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidState {
    /// Disconnected
    Disconnected = 0,
    /// Connecting control channel
    ConnectingControl = 1,
    /// Connecting interrupt channel
    ConnectingInterrupt = 2,
    /// Connected
    Connected = 3,
    /// Suspended
    Suspended = 4,
}

/// HID device info
#[derive(Debug, Clone)]
pub struct HidDeviceInfo {
    /// Vendor ID
    pub vendor_id: u16,
    /// Product ID
    pub product_id: u16,
    /// Device version
    pub version: u16,
    /// Country code
    pub country: u8,
    /// Device subclass
    pub subclass: HidDeviceSubclass,
    /// Device type
    pub device_type: HidDeviceType,
    /// Device name
    pub name: String,
    /// Report descriptor
    pub descriptor: HidReportDescriptor,
}

impl HidDeviceInfo {
    pub fn new() -> Self {
        Self {
            vendor_id: 0,
            product_id: 0,
            version: 0x0100,
            country: 0,
            subclass: HidDeviceSubclass::None,
            device_type: HidDeviceType::Generic,
            name: String::new(),
            descriptor: HidReportDescriptor::new(Vec::new()),
        }
    }
}

/// HID device connection
pub struct HidDevice {
    /// Remote device address
    pub address: BdAddr,
    /// Device info
    pub info: RwLock<HidDeviceInfo>,
    /// Connection state
    state: AtomicU8,
    /// Current protocol mode
    protocol: AtomicU8,
    /// Control channel (PSM 0x0011)
    control_channel: RwLock<Option<Arc<RwLock<L2capChannel>>>>,
    /// Interrupt channel (PSM 0x0013)
    interrupt_channel: RwLock<Option<Arc<RwLock<L2capChannel>>>>,
    /// Input report callback
    input_callback: Mutex<Option<Box<dyn Fn(&[u8]) + Send + Sync>>>,
    /// Boot keyboard state
    keyboard_state: RwLock<BootKeyboardReport>,
    /// Boot mouse state
    mouse_state: RwLock<BootMouseReport>,
    /// Idle rate (4ms units, 0 = infinite)
    idle_rate: AtomicU8,
    /// Reconnect enabled
    reconnect_enabled: AtomicBool,
}

impl HidDevice {
    pub fn new(address: BdAddr) -> Self {
        Self {
            address,
            info: RwLock::new(HidDeviceInfo::new()),
            state: AtomicU8::new(HidState::Disconnected as u8),
            protocol: AtomicU8::new(HidProtocol::Report as u8),
            control_channel: RwLock::new(None),
            interrupt_channel: RwLock::new(None),
            input_callback: Mutex::new(None),
            keyboard_state: RwLock::new(BootKeyboardReport::new()),
            mouse_state: RwLock::new(BootMouseReport::new()),
            idle_rate: AtomicU8::new(0),
            reconnect_enabled: AtomicBool::new(true),
        }
    }

    pub fn state(&self) -> HidState {
        match self.state.load(Ordering::Acquire) {
            0 => HidState::Disconnected,
            1 => HidState::ConnectingControl,
            2 => HidState::ConnectingInterrupt,
            3 => HidState::Connected,
            4 => HidState::Suspended,
            _ => HidState::Disconnected,
        }
    }

    pub fn protocol(&self) -> HidProtocol {
        match self.protocol.load(Ordering::Acquire) {
            0 => HidProtocol::Boot,
            _ => HidProtocol::Report,
        }
    }

    /// Set input report callback
    pub fn set_input_callback<F>(&self, callback: F)
    where
        F: Fn(&[u8]) + Send + Sync + 'static,
    {
        *self.input_callback.lock() = Some(Box::new(callback));
    }

    /// Build HID message header
    fn build_header(msg_type: HidMessageType, param: u8) -> u8 {
        ((msg_type as u8) << 4) | (param & 0x0F)
    }

    /// Parse HID message header
    fn parse_header(header: u8) -> (HidMessageType, u8) {
        let msg_type = match header >> 4 {
            0x00 => HidMessageType::Handshake,
            0x01 => HidMessageType::HidControl,
            0x04 => HidMessageType::GetReport,
            0x05 => HidMessageType::SetReport,
            0x06 => HidMessageType::GetProtocol,
            0x07 => HidMessageType::SetProtocol,
            0x08 => HidMessageType::GetIdle,
            0x09 => HidMessageType::SetIdle,
            0x0A => HidMessageType::DataOutput,
            0x0B => HidMessageType::DataInput,
            _ => HidMessageType::Reserved2,
        };
        let param = header & 0x0F;
        (msg_type, param)
    }

    /// Send control message
    pub fn send_control(&self, msg_type: HidMessageType, param: u8, data: &[u8]) -> Result<(), BluetoothError> {
        let channel = self.control_channel.read();
        let channel = channel.as_ref().ok_or(BluetoothError::NotConnected)?;

        let mut packet = Vec::with_capacity(1 + data.len());
        packet.push(Self::build_header(msg_type, param));
        packet.extend_from_slice(data);

        let chan = channel.read();
        chan.send(packet)
    }

    /// Send interrupt data
    pub fn send_interrupt(&self, data: &[u8]) -> Result<(), BluetoothError> {
        let channel = self.interrupt_channel.read();
        let channel = channel.as_ref().ok_or(BluetoothError::NotConnected)?;

        let chan = channel.read();
        chan.send(data.to_vec())
    }

    /// Get report request
    pub fn get_report(&self, report_type: HidReportType, report_id: u8) -> Result<Vec<u8>, BluetoothError> {
        let param = (report_type as u8) | 0x08; // Size bit set
        self.send_control(HidMessageType::GetReport, param, &[report_id])?;

        // Wait for response (simplified - would need async handling)
        Ok(Vec::new())
    }

    /// Set report request
    pub fn set_report(&self, report_type: HidReportType, report_id: u8, data: &[u8]) -> Result<(), BluetoothError> {
        let param = report_type as u8;
        let mut payload = vec![report_id];
        payload.extend_from_slice(data);
        self.send_control(HidMessageType::SetReport, param, &payload)
    }

    /// Get current protocol
    pub fn get_protocol_request(&self) -> Result<HidProtocol, BluetoothError> {
        self.send_control(HidMessageType::GetProtocol, 0, &[])?;
        Ok(self.protocol())
    }

    /// Set protocol mode
    pub fn set_protocol(&self, protocol: HidProtocol) -> Result<(), BluetoothError> {
        self.send_control(HidMessageType::SetProtocol, protocol as u8, &[])?;
        self.protocol.store(protocol as u8, Ordering::Release);
        Ok(())
    }

    /// Get idle rate
    pub fn get_idle(&self) -> Result<u8, BluetoothError> {
        self.send_control(HidMessageType::GetIdle, 0, &[])?;
        Ok(self.idle_rate.load(Ordering::Acquire))
    }

    /// Set idle rate (4ms units)
    pub fn set_idle(&self, rate: u8) -> Result<(), BluetoothError> {
        self.send_control(HidMessageType::SetIdle, rate, &[])?;
        self.idle_rate.store(rate, Ordering::Release);
        Ok(())
    }

    /// Send keyboard LED state
    pub fn set_keyboard_leds(&self, leds: KeyboardLeds) -> Result<(), BluetoothError> {
        self.set_report(HidReportType::Output, 0, &[leds.0])
    }

    /// Handle incoming control message
    pub fn handle_control(&self, data: &[u8]) -> Result<(), BluetoothError> {
        if data.is_empty() {
            return Err(BluetoothError::InvalidParameter);
        }

        let (msg_type, param) = Self::parse_header(data[0]);
        let payload = &data[1..];

        match msg_type {
            HidMessageType::Handshake => {
                let result = match param {
                    0x00 => HidHandshake::Successful,
                    0x01 => HidHandshake::NotReady,
                    0x02 => HidHandshake::ErrInvalidReportId,
                    0x03 => HidHandshake::ErrUnsupportedRequest,
                    0x04 => HidHandshake::ErrInvalidParameter,
                    0x0E => HidHandshake::ErrUnknown,
                    0x0F => HidHandshake::ErrFatal,
                    _ => HidHandshake::ErrUnknown,
                };
                // Handle handshake result
                if result != HidHandshake::Successful {
                    crate::log::warn!("HID handshake error: {:?}", result);
                }
            }
            HidMessageType::HidControl => {
                match param {
                    0x03 => {
                        // Suspend
                        self.state.store(HidState::Suspended as u8, Ordering::Release);
                    }
                    0x04 => {
                        // Exit suspend
                        self.state.store(HidState::Connected as u8, Ordering::Release);
                    }
                    0x05 => {
                        // Virtual cable unplug
                        self.disconnect();
                    }
                    _ => {}
                }
            }
            HidMessageType::DataInput => {
                // Input report on control channel
                self.handle_input_report(payload)?;
            }
            _ => {
                crate::log::debug!("Unhandled HID control message: {:?}", msg_type);
            }
        }

        Ok(())
    }

    /// Handle incoming interrupt data
    pub fn handle_interrupt(&self, data: &[u8]) -> Result<(), BluetoothError> {
        if data.is_empty() {
            return Err(BluetoothError::InvalidParameter);
        }

        let (msg_type, _param) = Self::parse_header(data[0]);

        if msg_type == HidMessageType::DataInput {
            self.handle_input_report(&data[1..])?;
        }

        Ok(())
    }

    /// Handle input report
    fn handle_input_report(&self, data: &[u8]) -> Result<(), BluetoothError> {
        // Update state based on device type
        let info = self.info.read();
        match info.device_type {
            HidDeviceType::Keyboard => {
                if let Some(report) = BootKeyboardReport::from_bytes(data) {
                    *self.keyboard_state.write() = report;
                }
            }
            HidDeviceType::Mouse => {
                if let Some(report) = BootMouseReport::from_bytes(data) {
                    *self.mouse_state.write() = report;
                }
            }
            _ => {}
        }
        drop(info);

        // Call input callback
        if let Some(callback) = self.input_callback.lock().as_ref() {
            callback(data);
        }

        Ok(())
    }

    /// Connect to device
    pub fn connect(&self, l2cap: &Arc<L2capManager>, handle: u16) -> Result<(), BluetoothError> {
        if self.state() != HidState::Disconnected {
            return Err(BluetoothError::AlreadyConnected);
        }

        self.state.store(HidState::ConnectingControl as u8, Ordering::Release);

        // Connect control channel
        let control = l2cap.connect(handle, PSM_HID_CONTROL)?;
        *self.control_channel.write() = Some(control);

        self.state.store(HidState::ConnectingInterrupt as u8, Ordering::Release);

        // Connect interrupt channel
        let interrupt = l2cap.connect(handle, PSM_HID_INTERRUPT)?;
        *self.interrupt_channel.write() = Some(interrupt);

        self.state.store(HidState::Connected as u8, Ordering::Release);

        Ok(())
    }

    /// Disconnect from device
    pub fn disconnect(&self) {
        self.state.store(HidState::Disconnected as u8, Ordering::Release);

        if let Some(channel) = self.control_channel.write().take() {
            let chan = channel.read();
            let _ = chan.disconnect();
        }

        if let Some(channel) = self.interrupt_channel.write().take() {
            let chan = channel.read();
            let _ = chan.disconnect();
        }
    }

    /// Get current keyboard state
    pub fn keyboard_state(&self) -> BootKeyboardReport {
        *self.keyboard_state.read()
    }

    /// Get current mouse state
    pub fn mouse_state(&self) -> BootMouseReport {
        *self.mouse_state.read()
    }
}

/// HID host manager
pub struct HidHost {
    /// L2CAP manager reference
    l2cap: Arc<L2capManager>,
    /// Connected devices
    devices: RwLock<BTreeMap<BdAddr, Arc<HidDevice>>>,
    /// Device discovery callback
    discovery_callback: Mutex<Option<Box<dyn Fn(&HidDeviceInfo) + Send + Sync>>>,
}

impl HidHost {
    pub fn new(l2cap: Arc<L2capManager>) -> Self {
        Self {
            l2cap,
            devices: RwLock::new(BTreeMap::new()),
            discovery_callback: Mutex::new(None),
        }
    }

    /// Set device discovery callback
    pub fn set_discovery_callback<F>(&self, callback: F)
    where
        F: Fn(&HidDeviceInfo) + Send + Sync + 'static,
    {
        *self.discovery_callback.lock() = Some(Box::new(callback));
    }

    /// Connect to HID device
    pub fn connect(&self, address: BdAddr, handle: u16) -> Result<Arc<HidDevice>, BluetoothError> {
        // Check if already connected
        if let Some(device) = self.devices.read().get(&address) {
            if device.state() == HidState::Connected {
                return Ok(device.clone());
            }
        }

        // Create new device
        let device = Arc::new(HidDevice::new(address));
        device.connect(&self.l2cap, handle)?;

        // Add to connected devices
        self.devices.write().insert(address, device.clone());

        Ok(device)
    }

    /// Disconnect HID device
    pub fn disconnect(&self, address: BdAddr) -> Result<(), BluetoothError> {
        if let Some(device) = self.devices.write().remove(&address) {
            device.disconnect();
            Ok(())
        } else {
            Err(BluetoothError::NotConnected)
        }
    }

    /// Get connected device
    pub fn get_device(&self, address: &BdAddr) -> Option<Arc<HidDevice>> {
        self.devices.read().get(address).cloned()
    }

    /// Get all connected devices
    pub fn devices(&self) -> Vec<Arc<HidDevice>> {
        self.devices.read().values().cloned().collect()
    }

    /// Handle incoming control connection
    pub fn handle_control_connection(&self, channel: Arc<RwLock<L2capChannel>>, address: BdAddr) -> Result<(), BluetoothError> {
        let device = self.devices.read().get(&address).cloned();

        if let Some(device) = device {
            *device.control_channel.write() = Some(channel);
            device.state.store(HidState::ConnectingInterrupt as u8, Ordering::Release);
        } else {
            // New incoming connection
            let device = Arc::new(HidDevice::new(address));
            *device.control_channel.write() = Some(channel);
            device.state.store(HidState::ConnectingInterrupt as u8, Ordering::Release);
            self.devices.write().insert(address, device);
        }

        Ok(())
    }

    /// Handle incoming interrupt connection
    pub fn handle_interrupt_connection(&self, channel: Arc<RwLock<L2capChannel>>, address: BdAddr) -> Result<(), BluetoothError> {
        let device = self.devices.read().get(&address).cloned();

        if let Some(device) = device {
            *device.interrupt_channel.write() = Some(channel);
            device.state.store(HidState::Connected as u8, Ordering::Release);
        }

        Ok(())
    }
}

/// HID device (peripheral) implementation
pub struct HidPeripheral {
    /// Device info
    pub info: RwLock<HidDeviceInfo>,
    /// Current protocol mode
    protocol: AtomicU8,
    /// L2CAP manager reference
    l2cap: Arc<L2capManager>,
    /// Connected host
    host_address: RwLock<Option<BdAddr>>,
    /// Control channel
    control_channel: RwLock<Option<Arc<RwLock<L2capChannel>>>>,
    /// Interrupt channel
    interrupt_channel: RwLock<Option<Arc<RwLock<L2capChannel>>>>,
    /// Idle rate (4ms units)
    idle_rate: AtomicU8,
}

impl HidPeripheral {
    pub fn new(l2cap: Arc<L2capManager>, info: HidDeviceInfo) -> Self {
        Self {
            info: RwLock::new(info),
            protocol: AtomicU8::new(HidProtocol::Report as u8),
            l2cap,
            host_address: RwLock::new(None),
            control_channel: RwLock::new(None),
            interrupt_channel: RwLock::new(None),
            idle_rate: AtomicU8::new(0),
        }
    }

    /// Create a keyboard peripheral
    pub fn keyboard(l2cap: Arc<L2capManager>, name: &str) -> Self {
        let mut info = HidDeviceInfo::new();
        info.subclass = HidDeviceSubclass::BootInterface;
        info.device_type = HidDeviceType::Keyboard;
        info.name = String::from(name);
        info.descriptor = HidReportDescriptor::boot_keyboard();
        Self::new(l2cap, info)
    }

    /// Create a mouse peripheral
    pub fn mouse(l2cap: Arc<L2capManager>, name: &str) -> Self {
        let mut info = HidDeviceInfo::new();
        info.subclass = HidDeviceSubclass::BootInterface;
        info.device_type = HidDeviceType::Mouse;
        info.name = String::from(name);
        info.descriptor = HidReportDescriptor::boot_mouse();
        Self::new(l2cap, info)
    }

    pub fn protocol(&self) -> HidProtocol {
        match self.protocol.load(Ordering::Acquire) {
            0 => HidProtocol::Boot,
            _ => HidProtocol::Report,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.host_address.read().is_some()
    }

    /// Send input report
    pub fn send_input_report(&self, report_id: u8, data: &[u8]) -> Result<(), BluetoothError> {
        let channel = self.interrupt_channel.read();
        let channel = channel.as_ref().ok_or(BluetoothError::NotConnected)?;

        let mut packet = Vec::with_capacity(2 + data.len());
        let header = ((HidMessageType::DataInput as u8) << 4) | (HidReportType::Input as u8);
        packet.push(header);
        if report_id != 0 {
            packet.push(report_id);
        }
        packet.extend_from_slice(data);

        let chan = channel.read();
        chan.send(packet)
    }

    /// Send keyboard report
    pub fn send_keyboard_report(&self, report: &BootKeyboardReport) -> Result<(), BluetoothError> {
        self.send_input_report(0, &report.to_bytes())
    }

    /// Send mouse report
    pub fn send_mouse_report(&self, report: &BootMouseReport) -> Result<(), BluetoothError> {
        self.send_input_report(0, &report.to_bytes())
    }

    /// Handle incoming control message
    pub fn handle_control(&self, data: &[u8]) -> Result<Vec<u8>, BluetoothError> {
        if data.is_empty() {
            return Err(BluetoothError::InvalidParameter);
        }

        let header = data[0];
        let msg_type = header >> 4;
        let param = header & 0x0F;

        match msg_type {
            0x04 => {
                // GET_REPORT
                let report_type = param & 0x03;
                let report_id = if data.len() > 1 { data[1] } else { 0 };

                // Return appropriate report based on type
                let response_header = ((HidMessageType::DataInput as u8) << 4) | report_type;
                let mut response = vec![response_header];

                if report_id != 0 {
                    response.push(report_id);
                }

                // Add empty report data (would be filled with actual data)
                Ok(response)
            }
            0x05 => {
                // SET_REPORT
                let handshake = ((HidMessageType::Handshake as u8) << 4) | (HidHandshake::Successful as u8);
                Ok(vec![handshake])
            }
            0x06 => {
                // GET_PROTOCOL
                let response_header = (HidMessageType::DataInput as u8) << 4;
                Ok(vec![response_header, self.protocol.load(Ordering::Acquire)])
            }
            0x07 => {
                // SET_PROTOCOL
                self.protocol.store(param, Ordering::Release);
                let handshake = ((HidMessageType::Handshake as u8) << 4) | (HidHandshake::Successful as u8);
                Ok(vec![handshake])
            }
            0x08 => {
                // GET_IDLE
                let response_header = (HidMessageType::DataInput as u8) << 4;
                Ok(vec![response_header, self.idle_rate.load(Ordering::Acquire)])
            }
            0x09 => {
                // SET_IDLE
                self.idle_rate.store(param, Ordering::Release);
                let handshake = ((HidMessageType::Handshake as u8) << 4) | (HidHandshake::Successful as u8);
                Ok(vec![handshake])
            }
            _ => {
                let handshake = ((HidMessageType::Handshake as u8) << 4) | (HidHandshake::ErrUnsupportedRequest as u8);
                Ok(vec![handshake])
            }
        }
    }

    /// Accept incoming connection
    pub fn accept_connection(&self, address: BdAddr, control: Arc<RwLock<L2capChannel>>, interrupt: Arc<RwLock<L2capChannel>>) {
        *self.host_address.write() = Some(address);
        *self.control_channel.write() = Some(control);
        *self.interrupt_channel.write() = Some(interrupt);
    }

    /// Disconnect from host
    pub fn disconnect(&self) {
        *self.host_address.write() = None;

        if let Some(channel) = self.control_channel.write().take() {
            let chan = channel.read();
            let _ = chan.disconnect();
        }

        if let Some(channel) = self.interrupt_channel.write().take() {
            let chan = channel.read();
            let _ = chan.disconnect();
        }
    }
}

/// USB HID keyboard scan codes
pub mod keycodes {
    pub const KEY_NONE: u8 = 0x00;
    pub const KEY_A: u8 = 0x04;
    pub const KEY_B: u8 = 0x05;
    pub const KEY_C: u8 = 0x06;
    pub const KEY_D: u8 = 0x07;
    pub const KEY_E: u8 = 0x08;
    pub const KEY_F: u8 = 0x09;
    pub const KEY_G: u8 = 0x0A;
    pub const KEY_H: u8 = 0x0B;
    pub const KEY_I: u8 = 0x0C;
    pub const KEY_J: u8 = 0x0D;
    pub const KEY_K: u8 = 0x0E;
    pub const KEY_L: u8 = 0x0F;
    pub const KEY_M: u8 = 0x10;
    pub const KEY_N: u8 = 0x11;
    pub const KEY_O: u8 = 0x12;
    pub const KEY_P: u8 = 0x13;
    pub const KEY_Q: u8 = 0x14;
    pub const KEY_R: u8 = 0x15;
    pub const KEY_S: u8 = 0x16;
    pub const KEY_T: u8 = 0x17;
    pub const KEY_U: u8 = 0x18;
    pub const KEY_V: u8 = 0x19;
    pub const KEY_W: u8 = 0x1A;
    pub const KEY_X: u8 = 0x1B;
    pub const KEY_Y: u8 = 0x1C;
    pub const KEY_Z: u8 = 0x1D;
    pub const KEY_1: u8 = 0x1E;
    pub const KEY_2: u8 = 0x1F;
    pub const KEY_3: u8 = 0x20;
    pub const KEY_4: u8 = 0x21;
    pub const KEY_5: u8 = 0x22;
    pub const KEY_6: u8 = 0x23;
    pub const KEY_7: u8 = 0x24;
    pub const KEY_8: u8 = 0x25;
    pub const KEY_9: u8 = 0x26;
    pub const KEY_0: u8 = 0x27;
    pub const KEY_ENTER: u8 = 0x28;
    pub const KEY_ESCAPE: u8 = 0x29;
    pub const KEY_BACKSPACE: u8 = 0x2A;
    pub const KEY_TAB: u8 = 0x2B;
    pub const KEY_SPACE: u8 = 0x2C;
    pub const KEY_MINUS: u8 = 0x2D;
    pub const KEY_EQUAL: u8 = 0x2E;
    pub const KEY_LEFT_BRACKET: u8 = 0x2F;
    pub const KEY_RIGHT_BRACKET: u8 = 0x30;
    pub const KEY_BACKSLASH: u8 = 0x31;
    pub const KEY_HASH: u8 = 0x32;
    pub const KEY_SEMICOLON: u8 = 0x33;
    pub const KEY_QUOTE: u8 = 0x34;
    pub const KEY_GRAVE: u8 = 0x35;
    pub const KEY_COMMA: u8 = 0x36;
    pub const KEY_PERIOD: u8 = 0x37;
    pub const KEY_SLASH: u8 = 0x38;
    pub const KEY_CAPS_LOCK: u8 = 0x39;
    pub const KEY_F1: u8 = 0x3A;
    pub const KEY_F2: u8 = 0x3B;
    pub const KEY_F3: u8 = 0x3C;
    pub const KEY_F4: u8 = 0x3D;
    pub const KEY_F5: u8 = 0x3E;
    pub const KEY_F6: u8 = 0x3F;
    pub const KEY_F7: u8 = 0x40;
    pub const KEY_F8: u8 = 0x41;
    pub const KEY_F9: u8 = 0x42;
    pub const KEY_F10: u8 = 0x43;
    pub const KEY_F11: u8 = 0x44;
    pub const KEY_F12: u8 = 0x45;
    pub const KEY_PRINT_SCREEN: u8 = 0x46;
    pub const KEY_SCROLL_LOCK: u8 = 0x47;
    pub const KEY_PAUSE: u8 = 0x48;
    pub const KEY_INSERT: u8 = 0x49;
    pub const KEY_HOME: u8 = 0x4A;
    pub const KEY_PAGE_UP: u8 = 0x4B;
    pub const KEY_DELETE: u8 = 0x4C;
    pub const KEY_END: u8 = 0x4D;
    pub const KEY_PAGE_DOWN: u8 = 0x4E;
    pub const KEY_RIGHT: u8 = 0x4F;
    pub const KEY_LEFT: u8 = 0x50;
    pub const KEY_DOWN: u8 = 0x51;
    pub const KEY_UP: u8 = 0x52;
    pub const KEY_NUM_LOCK: u8 = 0x53;
    pub const KEY_KP_DIVIDE: u8 = 0x54;
    pub const KEY_KP_MULTIPLY: u8 = 0x55;
    pub const KEY_KP_MINUS: u8 = 0x56;
    pub const KEY_KP_PLUS: u8 = 0x57;
    pub const KEY_KP_ENTER: u8 = 0x58;
    pub const KEY_KP_1: u8 = 0x59;
    pub const KEY_KP_2: u8 = 0x5A;
    pub const KEY_KP_3: u8 = 0x5B;
    pub const KEY_KP_4: u8 = 0x5C;
    pub const KEY_KP_5: u8 = 0x5D;
    pub const KEY_KP_6: u8 = 0x5E;
    pub const KEY_KP_7: u8 = 0x5F;
    pub const KEY_KP_8: u8 = 0x60;
    pub const KEY_KP_9: u8 = 0x61;
    pub const KEY_KP_0: u8 = 0x62;
    pub const KEY_KP_DECIMAL: u8 = 0x63;
    pub const KEY_APPLICATION: u8 = 0x65;
    pub const KEY_POWER: u8 = 0x66;
    pub const KEY_KP_EQUAL: u8 = 0x67;
    pub const KEY_F13: u8 = 0x68;
    pub const KEY_F14: u8 = 0x69;
    pub const KEY_F15: u8 = 0x6A;
    pub const KEY_F16: u8 = 0x6B;
    pub const KEY_F17: u8 = 0x6C;
    pub const KEY_F18: u8 = 0x6D;
    pub const KEY_F19: u8 = 0x6E;
    pub const KEY_F20: u8 = 0x6F;
    pub const KEY_F21: u8 = 0x70;
    pub const KEY_F22: u8 = 0x71;
    pub const KEY_F23: u8 = 0x72;
    pub const KEY_F24: u8 = 0x73;

    // Modifier keys (left)
    pub const KEY_LEFT_CTRL: u8 = 0xE0;
    pub const KEY_LEFT_SHIFT: u8 = 0xE1;
    pub const KEY_LEFT_ALT: u8 = 0xE2;
    pub const KEY_LEFT_GUI: u8 = 0xE3;

    // Modifier keys (right)
    pub const KEY_RIGHT_CTRL: u8 = 0xE4;
    pub const KEY_RIGHT_SHIFT: u8 = 0xE5;
    pub const KEY_RIGHT_ALT: u8 = 0xE6;
    pub const KEY_RIGHT_GUI: u8 = 0xE7;

    /// Rollover error (too many keys pressed)
    pub const KEY_ERR_ROLLOVER: u8 = 0x01;
    /// POST fail
    pub const KEY_ERR_POST: u8 = 0x02;
    /// Undefined error
    pub const KEY_ERR_UNDEFINED: u8 = 0x03;
}

/// Initialize HID subsystem
pub fn init() {
    crate::log::info!("Bluetooth HID profile initialized");
}
