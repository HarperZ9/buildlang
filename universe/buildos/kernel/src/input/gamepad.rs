//! Gamepad/Controller Input Driver
//!
//! This module implements comprehensive gamepad support including:
//!
//! - Xbox controllers (360, One, Series X/S)
//! - PlayStation controllers (DualShock, DualSense)
//! - Nintendo controllers (Switch Pro, Joy-Con)
//! - Generic USB HID gamepads
//! - Analog sticks with deadzone handling
//! - Trigger axes
//! - D-pad (HAT switch)
//! - Force feedback/rumble

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU16, Ordering};
use spin::RwLock;

use crate::math::F32Ext;
use super::events::*;
use super::{InputDevice, InputId, InputHandler, InputError, InputEvent, input_manager};

/// Gamepad button mapping
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamepadButton {
    /// South button (A on Xbox, X on PlayStation, B on Nintendo)
    South,
    /// East button (B on Xbox, Circle on PlayStation, A on Nintendo)
    East,
    /// West button (X on Xbox, Square on PlayStation, Y on Nintendo)
    West,
    /// North button (Y on Xbox, Triangle on PlayStation, X on Nintendo)
    North,
    /// Left shoulder button (LB)
    LeftShoulder,
    /// Right shoulder button (RB)
    RightShoulder,
    /// Left trigger (LT) as button
    LeftTrigger,
    /// Right trigger (RT) as button
    RightTrigger,
    /// Select/Back/Share button
    Select,
    /// Start/Menu/Options button
    Start,
    /// Mode/Guide/PS/Home button
    Mode,
    /// Left stick click (L3)
    LeftStick,
    /// Right stick click (R3)
    RightStick,
    /// D-pad up
    DpadUp,
    /// D-pad down
    DpadDown,
    /// D-pad left
    DpadLeft,
    /// D-pad right
    DpadRight,
}

impl GamepadButton {
    /// Convert to evdev button code
    pub fn to_evdev_code(&self) -> u16 {
        match self {
            GamepadButton::South => BTN_A,
            GamepadButton::East => BTN_B,
            GamepadButton::West => BTN_X,
            GamepadButton::North => BTN_Y,
            GamepadButton::LeftShoulder => BTN_TL,
            GamepadButton::RightShoulder => BTN_TR,
            GamepadButton::LeftTrigger => BTN_TL2,
            GamepadButton::RightTrigger => BTN_TR2,
            GamepadButton::Select => BTN_SELECT,
            GamepadButton::Start => BTN_START,
            GamepadButton::Mode => BTN_MODE,
            GamepadButton::LeftStick => BTN_THUMBL,
            GamepadButton::RightStick => BTN_THUMBR,
            GamepadButton::DpadUp => BTN_DPAD_UP,
            GamepadButton::DpadDown => BTN_DPAD_DOWN,
            GamepadButton::DpadLeft => BTN_DPAD_LEFT,
            GamepadButton::DpadRight => BTN_DPAD_RIGHT,
        }
    }

    /// Create from evdev button code
    pub fn from_evdev_code(code: u16) -> Option<Self> {
        Some(match code {
            BTN_SOUTH => GamepadButton::South,  // BTN_A is an alias
            BTN_EAST => GamepadButton::East,    // BTN_B is an alias
            BTN_NORTH => GamepadButton::West,   // BTN_X is an alias
            BTN_WEST => GamepadButton::North,   // BTN_Y is an alias
            BTN_TL => GamepadButton::LeftShoulder,
            BTN_TR => GamepadButton::RightShoulder,
            BTN_TL2 => GamepadButton::LeftTrigger,
            BTN_TR2 => GamepadButton::RightTrigger,
            BTN_SELECT => GamepadButton::Select,
            BTN_START => GamepadButton::Start,
            BTN_MODE => GamepadButton::Mode,
            BTN_THUMBL => GamepadButton::LeftStick,
            BTN_THUMBR => GamepadButton::RightStick,
            BTN_DPAD_UP => GamepadButton::DpadUp,
            BTN_DPAD_DOWN => GamepadButton::DpadDown,
            BTN_DPAD_LEFT => GamepadButton::DpadLeft,
            BTN_DPAD_RIGHT => GamepadButton::DpadRight,
            _ => return None,
        })
    }
}

/// Gamepad axis mapping
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamepadAxis {
    /// Left stick X axis
    LeftStickX,
    /// Left stick Y axis
    LeftStickY,
    /// Right stick X axis
    RightStickX,
    /// Right stick Y axis
    RightStickY,
    /// Left trigger (0-1 range)
    LeftTrigger,
    /// Right trigger (0-1 range)
    RightTrigger,
    /// D-pad X axis (-1, 0, 1)
    DpadX,
    /// D-pad Y axis (-1, 0, 1)
    DpadY,
}

impl GamepadAxis {
    /// Convert to evdev axis code
    pub fn to_evdev_code(&self) -> u16 {
        match self {
            GamepadAxis::LeftStickX => ABS_X,
            GamepadAxis::LeftStickY => ABS_Y,
            GamepadAxis::RightStickX => ABS_RX,
            GamepadAxis::RightStickY => ABS_RY,
            GamepadAxis::LeftTrigger => ABS_Z,
            GamepadAxis::RightTrigger => ABS_RZ,
            GamepadAxis::DpadX => ABS_HAT0X,
            GamepadAxis::DpadY => ABS_HAT0Y,
        }
    }
}

/// Gamepad type/layout
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamepadType {
    /// Xbox-style layout (ABXY with A on bottom)
    Xbox,
    /// PlayStation-style layout (Cross/Circle/Square/Triangle)
    PlayStation,
    /// Nintendo-style layout (ABXY with B on bottom, reversed)
    Nintendo,
    /// Generic/unknown layout
    Generic,
}

impl Default for GamepadType {
    fn default() -> Self {
        Self::Generic
    }
}

/// Gamepad button state
#[derive(Debug, Clone, Copy, Default)]
pub struct GamepadButtons {
    /// Button state bitmap
    state: u32,
}

impl GamepadButtons {
    pub const fn new() -> Self {
        Self { state: 0 }
    }

    /// Check if button is pressed
    pub fn is_pressed(&self, button: GamepadButton) -> bool {
        let bit = button as u32;
        (self.state & (1 << bit)) != 0
    }

    /// Set button state
    pub fn set(&mut self, button: GamepadButton, pressed: bool) {
        let bit = button as u32;
        if pressed {
            self.state |= 1 << bit;
        } else {
            self.state &= !(1 << bit);
        }
    }

    /// Get number of pressed buttons
    pub fn pressed_count(&self) -> u32 {
        self.state.count_ones()
    }

    /// Clear all buttons
    pub fn clear(&mut self) {
        self.state = 0;
    }
}

/// Stick position with deadzone handling
#[derive(Debug, Clone, Copy, Default)]
pub struct StickPosition {
    /// Raw X value (-32768 to 32767)
    pub raw_x: i16,
    /// Raw Y value (-32768 to 32767)
    pub raw_y: i16,
    /// Deadzone radius (0-32767)
    pub deadzone: i16,
}

impl StickPosition {
    pub const fn new() -> Self {
        Self {
            raw_x: 0,
            raw_y: 0,
            deadzone: 8000,  // ~24% default deadzone
        }
    }

    /// Get X position normalized to -1.0 to 1.0 with deadzone applied
    pub fn x(&self) -> f32 {
        self.apply_deadzone(self.raw_x as f32 / 32767.0)
    }

    /// Get Y position normalized to -1.0 to 1.0 with deadzone applied
    pub fn y(&self) -> f32 {
        self.apply_deadzone(self.raw_y as f32 / 32767.0)
    }

    /// Get magnitude (0.0 to 1.0)
    pub fn magnitude(&self) -> f32 {
        let x = self.x();
        let y = self.y();
        (x * x + y * y).sqrt().min(1.0)
    }

    /// Get angle in radians
    pub fn angle(&self) -> f32 {
        libm::atan2f(self.y(), self.x())
    }

    /// Apply circular deadzone
    fn apply_deadzone(&self, value: f32) -> f32 {
        let dz = self.deadzone as f32 / 32767.0;
        if value.abs() < dz {
            0.0
        } else {
            let sign = if value >= 0.0 { 1.0 } else { -1.0 };
            sign * (value.abs() - dz) / (1.0 - dz)
        }
    }

    /// Check if stick is in neutral position
    pub fn is_neutral(&self) -> bool {
        self.magnitude() < 0.01
    }

    /// Get direction as discrete value (for D-pad emulation)
    pub fn direction(&self) -> (i8, i8) {
        let x = if self.x() > 0.5 {
            1
        } else if self.x() < -0.5 {
            -1
        } else {
            0
        };

        let y = if self.y() > 0.5 {
            1
        } else if self.y() < -0.5 {
            -1
        } else {
            0
        };

        (x, y)
    }
}

/// Trigger state
#[derive(Debug, Clone, Copy, Default)]
pub struct TriggerState {
    /// Raw value (0 to 255 typically)
    pub raw: u8,
    /// Threshold for digital button activation
    pub threshold: u8,
}

impl TriggerState {
    pub const fn new() -> Self {
        Self {
            raw: 0,
            threshold: 30,  // ~12% activation threshold
        }
    }

    /// Get normalized value (0.0 to 1.0)
    pub fn value(&self) -> f32 {
        self.raw as f32 / 255.0
    }

    /// Check if trigger is pressed (above threshold)
    pub fn is_pressed(&self) -> bool {
        self.raw >= self.threshold
    }
}

/// Complete gamepad state
#[derive(Debug, Clone)]
pub struct GamepadState {
    /// Button states
    pub buttons: GamepadButtons,
    /// Left stick position
    pub left_stick: StickPosition,
    /// Right stick position
    pub right_stick: StickPosition,
    /// Left trigger
    pub left_trigger: TriggerState,
    /// Right trigger
    pub right_trigger: TriggerState,
    /// D-pad state
    pub dpad_x: i8,
    pub dpad_y: i8,
    /// Controller type
    pub gamepad_type: GamepadType,
    /// Connected flag
    pub connected: bool,
    /// Battery level (0-100, or -1 if unknown)
    pub battery_level: i8,
}

impl Default for GamepadState {
    fn default() -> Self {
        Self::new()
    }
}

impl GamepadState {
    pub const fn new() -> Self {
        Self {
            buttons: GamepadButtons::new(),
            left_stick: StickPosition::new(),
            right_stick: StickPosition::new(),
            left_trigger: TriggerState::new(),
            right_trigger: TriggerState::new(),
            dpad_x: 0,
            dpad_y: 0,
            gamepad_type: GamepadType::Generic,
            connected: false,
            battery_level: -1,
        }
    }

    /// Update from evdev event
    pub fn update_from_event(&mut self, event: &InputEvent) {
        match event.ev_type {
            EV_KEY => {
                let pressed = event.value != 0;
                if let Some(button) = GamepadButton::from_evdev_code(event.code) {
                    self.buttons.set(button, pressed);
                }
            }
            EV_ABS => {
                match event.code {
                    ABS_X => self.left_stick.raw_x = event.value as i16,
                    ABS_Y => self.left_stick.raw_y = event.value as i16,
                    ABS_RX => self.right_stick.raw_x = event.value as i16,
                    ABS_RY => self.right_stick.raw_y = event.value as i16,
                    ABS_Z => self.left_trigger.raw = event.value as u8,
                    ABS_RZ => self.right_trigger.raw = event.value as u8,
                    ABS_HAT0X => self.dpad_x = event.value as i8,
                    ABS_HAT0Y => self.dpad_y = event.value as i8,
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

/// Gamepad driver
pub struct GamepadDriver {
    /// Input device
    device: Arc<InputDevice>,
    /// Current state
    state: RwLock<GamepadState>,
    /// Controller type
    gamepad_type: GamepadType,
    /// Force feedback supported
    ff_supported: AtomicBool,
    /// Currently playing rumble effects
    rumble_left: AtomicU16,
    rumble_right: AtomicU16,
    /// Rumble gain (0-65535)
    rumble_gain: AtomicU16,
}

impl GamepadDriver {
    pub fn new(name: &str, vendor_id: u16, product_id: u16, gamepad_type: GamepadType) -> Self {
        let device = InputDevice::gamepad(name, InputId::usb(vendor_id, product_id, 1));

        Self {
            device: Arc::new(device),
            state: RwLock::new(GamepadState::new()),
            gamepad_type,
            ff_supported: AtomicBool::new(false),
            rumble_left: AtomicU16::new(0),
            rumble_right: AtomicU16::new(0),
            rumble_gain: AtomicU16::new(65535),
        }
    }

    /// Create Xbox controller
    pub fn xbox(name: &str, vendor_id: u16, product_id: u16) -> Self {
        Self::new(name, vendor_id, product_id, GamepadType::Xbox)
    }

    /// Create PlayStation controller
    pub fn playstation(name: &str, vendor_id: u16, product_id: u16) -> Self {
        Self::new(name, vendor_id, product_id, GamepadType::PlayStation)
    }

    /// Create Nintendo controller
    pub fn nintendo(name: &str, vendor_id: u16, product_id: u16) -> Self {
        Self::new(name, vendor_id, product_id, GamepadType::Nintendo)
    }

    /// Register with input subsystem
    pub fn register(&self) -> Result<(), InputError> {
        input_manager().register_device(self.device.clone())
    }

    /// Process button event
    pub fn report_button(&self, button: GamepadButton, pressed: bool) {
        let code = button.to_evdev_code();
        self.device.report_key(code, pressed);
        self.state.write().buttons.set(button, pressed);
    }

    /// Process axis event
    pub fn report_axis(&self, axis: GamepadAxis, value: i32) {
        let code = axis.to_evdev_code();
        self.device.report_abs(code, value);

        let mut state = self.state.write();
        match axis {
            GamepadAxis::LeftStickX => state.left_stick.raw_x = value as i16,
            GamepadAxis::LeftStickY => state.left_stick.raw_y = value as i16,
            GamepadAxis::RightStickX => state.right_stick.raw_x = value as i16,
            GamepadAxis::RightStickY => state.right_stick.raw_y = value as i16,
            GamepadAxis::LeftTrigger => state.left_trigger.raw = value as u8,
            GamepadAxis::RightTrigger => state.right_trigger.raw = value as u8,
            GamepadAxis::DpadX => state.dpad_x = value as i8,
            GamepadAxis::DpadY => state.dpad_y = value as i8,
        }
    }

    /// Sync events
    pub fn sync(&self) {
        self.device.sync();
    }

    /// Get current state
    pub fn state(&self) -> GamepadState {
        self.state.read().clone()
    }

    /// Get controller type
    pub fn gamepad_type(&self) -> GamepadType {
        self.gamepad_type
    }

    /// Check if button is pressed
    pub fn is_button_pressed(&self, button: GamepadButton) -> bool {
        self.state.read().buttons.is_pressed(button)
    }

    /// Get left stick position
    pub fn left_stick(&self) -> (f32, f32) {
        let state = self.state.read();
        (state.left_stick.x(), state.left_stick.y())
    }

    /// Get right stick position
    pub fn right_stick(&self) -> (f32, f32) {
        let state = self.state.read();
        (state.right_stick.x(), state.right_stick.y())
    }

    /// Get trigger values
    pub fn triggers(&self) -> (f32, f32) {
        let state = self.state.read();
        (state.left_trigger.value(), state.right_trigger.value())
    }

    /// Set deadzone for sticks
    pub fn set_deadzone(&self, deadzone: i16) {
        let mut state = self.state.write();
        state.left_stick.deadzone = deadzone;
        state.right_stick.deadzone = deadzone;
    }

    /// Set trigger threshold
    pub fn set_trigger_threshold(&self, threshold: u8) {
        let mut state = self.state.write();
        state.left_trigger.threshold = threshold;
        state.right_trigger.threshold = threshold;
    }

    /// Enable force feedback
    pub fn enable_force_feedback(&self) {
        self.ff_supported.store(true, Ordering::Relaxed);
    }

    /// Set rumble intensity
    pub fn set_rumble(&self, left: u16, right: u16) {
        let gain = self.rumble_gain.load(Ordering::Relaxed) as u32;
        let left = ((left as u32 * gain) / 65535) as u16;
        let right = ((right as u32 * gain) / 65535) as u16;

        self.rumble_left.store(left, Ordering::Relaxed);
        self.rumble_right.store(right, Ordering::Relaxed);

        // In real implementation, send rumble command to device
    }

    /// Stop rumble
    pub fn stop_rumble(&self) {
        self.rumble_left.store(0, Ordering::Relaxed);
        self.rumble_right.store(0, Ordering::Relaxed);
    }

    /// Set rumble gain
    pub fn set_rumble_gain(&self, gain: u16) {
        self.rumble_gain.store(gain, Ordering::Relaxed);
    }

    /// Get the input device
    pub fn device(&self) -> &Arc<InputDevice> {
        &self.device
    }
}

/// Xbox controller driver
pub struct XboxController {
    /// Base gamepad driver
    driver: GamepadDriver,
    /// Wireless adapter connected
    is_wireless: AtomicBool,
    /// Controller index (0-3 for wireless)
    controller_index: AtomicU8,
}

impl XboxController {
    /// Xbox 360 controller VID/PID
    pub const XBOX360_VID: u16 = 0x045E;
    pub const XBOX360_WIRED_PID: u16 = 0x028E;
    pub const XBOX360_WIRELESS_PID: u16 = 0x0719;

    /// Xbox One controller VID/PID
    pub const XBOXONE_VID: u16 = 0x045E;
    pub const XBOXONE_PID: u16 = 0x02D1;
    pub const XBOXONE_S_PID: u16 = 0x02EA;
    pub const XBOXONE_ELITE_PID: u16 = 0x02E3;

    /// Xbox Series X|S controller
    pub const XBOXSX_PID: u16 = 0x0B12;

    pub fn new(name: &str, vendor_id: u16, product_id: u16) -> Self {
        Self {
            driver: GamepadDriver::xbox(name, vendor_id, product_id),
            is_wireless: AtomicBool::new(false),
            controller_index: AtomicU8::new(0),
        }
    }

    /// Create Xbox 360 wired controller
    pub fn xbox360_wired() -> Self {
        Self::new("Xbox 360 Controller", Self::XBOX360_VID, Self::XBOX360_WIRED_PID)
    }

    /// Create Xbox One controller
    pub fn xbox_one() -> Self {
        Self::new("Xbox One Controller", Self::XBOXONE_VID, Self::XBOXONE_PID)
    }

    /// Create Xbox Series X|S controller
    pub fn xbox_series() -> Self {
        Self::new("Xbox Series X|S Controller", Self::XBOXONE_VID, Self::XBOXSX_PID)
    }

    /// Register with input subsystem
    pub fn register(&self) -> Result<(), InputError> {
        self.driver.register()
    }

    /// Process USB HID report
    pub fn process_report(&self, report: &[u8]) {
        if report.len() < 14 {
            return;
        }

        // Xbox controller report format (varies by controller type)
        // Byte 0: Report ID
        // Byte 1: ?
        // Byte 2-3: Buttons
        // Byte 4: Left trigger
        // Byte 5: Right trigger
        // Byte 6-7: Left stick X
        // Byte 8-9: Left stick Y
        // Byte 10-11: Right stick X
        // Byte 12-13: Right stick Y

        let buttons1 = report[2];
        let buttons2 = report[3];

        // Process buttons
        self.driver.report_button(GamepadButton::DpadUp, (buttons1 & 0x01) != 0);
        self.driver.report_button(GamepadButton::DpadDown, (buttons1 & 0x02) != 0);
        self.driver.report_button(GamepadButton::DpadLeft, (buttons1 & 0x04) != 0);
        self.driver.report_button(GamepadButton::DpadRight, (buttons1 & 0x08) != 0);
        self.driver.report_button(GamepadButton::Start, (buttons1 & 0x10) != 0);
        self.driver.report_button(GamepadButton::Select, (buttons1 & 0x20) != 0);
        self.driver.report_button(GamepadButton::LeftStick, (buttons1 & 0x40) != 0);
        self.driver.report_button(GamepadButton::RightStick, (buttons1 & 0x80) != 0);

        self.driver.report_button(GamepadButton::LeftShoulder, (buttons2 & 0x01) != 0);
        self.driver.report_button(GamepadButton::RightShoulder, (buttons2 & 0x02) != 0);
        self.driver.report_button(GamepadButton::Mode, (buttons2 & 0x04) != 0);
        self.driver.report_button(GamepadButton::South, (buttons2 & 0x10) != 0);
        self.driver.report_button(GamepadButton::East, (buttons2 & 0x20) != 0);
        self.driver.report_button(GamepadButton::West, (buttons2 & 0x40) != 0);
        self.driver.report_button(GamepadButton::North, (buttons2 & 0x80) != 0);

        // Process triggers (0-255)
        self.driver.report_axis(GamepadAxis::LeftTrigger, report[4] as i32);
        self.driver.report_axis(GamepadAxis::RightTrigger, report[5] as i32);

        // Process sticks (16-bit signed)
        let left_x = i16::from_le_bytes([report[6], report[7]]) as i32;
        let left_y = i16::from_le_bytes([report[8], report[9]]) as i32;
        let right_x = i16::from_le_bytes([report[10], report[11]]) as i32;
        let right_y = i16::from_le_bytes([report[12], report[13]]) as i32;

        self.driver.report_axis(GamepadAxis::LeftStickX, left_x);
        self.driver.report_axis(GamepadAxis::LeftStickY, -left_y); // Invert Y
        self.driver.report_axis(GamepadAxis::RightStickX, right_x);
        self.driver.report_axis(GamepadAxis::RightStickY, -right_y); // Invert Y

        self.driver.sync();
    }

    /// Set LED pattern (Xbox 360)
    pub fn set_led(&self, _pattern: u8) {
        // Send LED command
    }

    /// Get driver reference
    pub fn driver(&self) -> &GamepadDriver {
        &self.driver
    }
}

/// PlayStation controller driver
pub struct PlayStationController {
    /// Base gamepad driver
    driver: GamepadDriver,
    /// Controller generation
    generation: PlayStationGeneration,
    /// Touchpad data
    touchpad: RwLock<DualSenseTouchpad>,
    /// Motion sensor data
    motion: RwLock<MotionData>,
    /// Light bar color
    light_bar: RwLock<(u8, u8, u8)>,
}

/// PlayStation controller generation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayStationGeneration {
    /// DualShock 3
    DS3,
    /// DualShock 4
    DS4,
    /// DualSense (PS5)
    DualSense,
}

/// DualSense touchpad data
#[derive(Debug, Clone, Copy, Default)]
pub struct DualSenseTouchpad {
    /// Touch 1 active
    pub touch1_active: bool,
    /// Touch 1 X
    pub touch1_x: u16,
    /// Touch 1 Y
    pub touch1_y: u16,
    /// Touch 2 active
    pub touch2_active: bool,
    /// Touch 2 X
    pub touch2_x: u16,
    /// Touch 2 Y
    pub touch2_y: u16,
}

/// Motion sensor data
#[derive(Debug, Clone, Copy, Default)]
pub struct MotionData {
    /// Accelerometer X
    pub accel_x: i16,
    /// Accelerometer Y
    pub accel_y: i16,
    /// Accelerometer Z
    pub accel_z: i16,
    /// Gyroscope X
    pub gyro_x: i16,
    /// Gyroscope Y
    pub gyro_y: i16,
    /// Gyroscope Z
    pub gyro_z: i16,
}

impl PlayStationController {
    /// DualShock 4 VID/PID
    pub const DS4_VID: u16 = 0x054C;
    pub const DS4_PID_V1: u16 = 0x05C4;
    pub const DS4_PID_V2: u16 = 0x09CC;

    /// DualSense VID/PID
    pub const DUALSENSE_VID: u16 = 0x054C;
    pub const DUALSENSE_PID: u16 = 0x0CE6;
    pub const DUALSENSE_EDGE_PID: u16 = 0x0DF2;

    pub fn new(name: &str, vendor_id: u16, product_id: u16, generation: PlayStationGeneration) -> Self {
        Self {
            driver: GamepadDriver::playstation(name, vendor_id, product_id),
            generation,
            touchpad: RwLock::new(DualSenseTouchpad::default()),
            motion: RwLock::new(MotionData::default()),
            light_bar: RwLock::new((0, 0, 255)), // Default blue
        }
    }

    /// Create DualShock 4 controller
    pub fn dualshock4() -> Self {
        Self::new("DualShock 4", Self::DS4_VID, Self::DS4_PID_V2, PlayStationGeneration::DS4)
    }

    /// Create DualSense controller
    pub fn dualsense() -> Self {
        Self::new("DualSense", Self::DUALSENSE_VID, Self::DUALSENSE_PID, PlayStationGeneration::DualSense)
    }

    /// Register with input subsystem
    pub fn register(&self) -> Result<(), InputError> {
        self.driver.register()
    }

    /// Process DualSense USB report
    pub fn process_dualsense_report(&self, report: &[u8]) {
        if report.len() < 64 {
            return;
        }

        // DualSense USB report format
        let left_x = report[1] as i32 - 128;
        let left_y = report[2] as i32 - 128;
        let right_x = report[3] as i32 - 128;
        let right_y = report[4] as i32 - 128;
        let left_trigger = report[5] as i32;
        let right_trigger = report[6] as i32;

        let buttons1 = report[8];
        let buttons2 = report[9];
        let buttons3 = report[10];

        // Process D-pad (lower 4 bits of buttons1)
        let dpad = buttons1 & 0x0F;
        let (dpad_x, dpad_y) = match dpad {
            0 => (0, -1),   // Up
            1 => (1, -1),   // Up-Right
            2 => (1, 0),    // Right
            3 => (1, 1),    // Down-Right
            4 => (0, 1),    // Down
            5 => (-1, 1),   // Down-Left
            6 => (-1, 0),   // Left
            7 => (-1, -1),  // Up-Left
            _ => (0, 0),    // Neutral
        };

        // Report axes
        self.driver.report_axis(GamepadAxis::LeftStickX, left_x * 256);
        self.driver.report_axis(GamepadAxis::LeftStickY, -left_y * 256);
        self.driver.report_axis(GamepadAxis::RightStickX, right_x * 256);
        self.driver.report_axis(GamepadAxis::RightStickY, -right_y * 256);
        self.driver.report_axis(GamepadAxis::LeftTrigger, left_trigger);
        self.driver.report_axis(GamepadAxis::RightTrigger, right_trigger);
        self.driver.report_axis(GamepadAxis::DpadX, dpad_x);
        self.driver.report_axis(GamepadAxis::DpadY, dpad_y);

        // Process buttons
        self.driver.report_button(GamepadButton::West, (buttons1 & 0x10) != 0);   // Square
        self.driver.report_button(GamepadButton::South, (buttons1 & 0x20) != 0);  // Cross
        self.driver.report_button(GamepadButton::East, (buttons1 & 0x40) != 0);   // Circle
        self.driver.report_button(GamepadButton::North, (buttons1 & 0x80) != 0);  // Triangle

        self.driver.report_button(GamepadButton::LeftShoulder, (buttons2 & 0x01) != 0);
        self.driver.report_button(GamepadButton::RightShoulder, (buttons2 & 0x02) != 0);
        self.driver.report_button(GamepadButton::LeftTrigger, (buttons2 & 0x04) != 0);
        self.driver.report_button(GamepadButton::RightTrigger, (buttons2 & 0x08) != 0);
        self.driver.report_button(GamepadButton::Select, (buttons2 & 0x10) != 0);  // Create
        self.driver.report_button(GamepadButton::Start, (buttons2 & 0x20) != 0);   // Options
        self.driver.report_button(GamepadButton::LeftStick, (buttons2 & 0x40) != 0);
        self.driver.report_button(GamepadButton::RightStick, (buttons2 & 0x80) != 0);

        self.driver.report_button(GamepadButton::Mode, (buttons3 & 0x01) != 0);  // PS button

        // Process motion sensors
        {
            let mut motion = self.motion.write();
            motion.gyro_x = i16::from_le_bytes([report[16], report[17]]);
            motion.gyro_y = i16::from_le_bytes([report[18], report[19]]);
            motion.gyro_z = i16::from_le_bytes([report[20], report[21]]);
            motion.accel_x = i16::from_le_bytes([report[22], report[23]]);
            motion.accel_y = i16::from_le_bytes([report[24], report[25]]);
            motion.accel_z = i16::from_le_bytes([report[26], report[27]]);
        }

        // Process touchpad
        {
            let mut touchpad = self.touchpad.write();
            touchpad.touch1_active = (report[33] & 0x80) == 0;
            if touchpad.touch1_active {
                touchpad.touch1_x = ((report[35] & 0x0F) as u16) << 8 | report[34] as u16;
                touchpad.touch1_y = (report[36] as u16) << 4 | ((report[35] & 0xF0) >> 4) as u16;
            }
            touchpad.touch2_active = (report[37] & 0x80) == 0;
            if touchpad.touch2_active {
                touchpad.touch2_x = ((report[39] & 0x0F) as u16) << 8 | report[38] as u16;
                touchpad.touch2_y = (report[40] as u16) << 4 | ((report[39] & 0xF0) >> 4) as u16;
            }
        }

        self.driver.sync();
    }

    /// Set light bar color
    pub fn set_light_bar(&self, r: u8, g: u8, b: u8) {
        *self.light_bar.write() = (r, g, b);
        // In real implementation, send output report to controller
    }

    /// Set adaptive trigger effect (DualSense)
    pub fn set_adaptive_trigger(&self, _trigger: u8, _mode: AdaptiveTriggerMode) {
        // In real implementation, send output report
    }

    /// Get motion data
    pub fn motion(&self) -> MotionData {
        *self.motion.read()
    }

    /// Get touchpad data
    pub fn touchpad(&self) -> DualSenseTouchpad {
        *self.touchpad.read()
    }

    /// Get driver reference
    pub fn driver(&self) -> &GamepadDriver {
        &self.driver
    }
}

/// DualSense adaptive trigger mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdaptiveTriggerMode {
    /// No resistance
    Off,
    /// Continuous resistance
    Continuous { start: u8, force: u8 },
    /// Resistance at specific section
    Section { start: u8, end: u8, force: u8 },
    /// Vibration effect
    Vibrate { position: u8, amplitude: u8, frequency: u8 },
}

/// Nintendo Switch controller driver
pub struct SwitchController {
    /// Base gamepad driver
    driver: GamepadDriver,
    /// Controller type
    controller_type: SwitchControllerType,
    /// Player LED pattern
    player_leds: AtomicU8,
    /// Home LED brightness
    home_led: AtomicU8,
    /// Motion data
    motion: RwLock<MotionData>,
}

/// Nintendo Switch controller type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchControllerType {
    /// Pro Controller
    Pro,
    /// Left Joy-Con
    JoyConL,
    /// Right Joy-Con
    JoyConR,
    /// Joy-Con pair
    JoyConPair,
}

impl SwitchController {
    /// Nintendo Switch Pro Controller
    pub const PRO_VID: u16 = 0x057E;
    pub const PRO_PID: u16 = 0x2009;

    /// Joy-Con
    pub const JOYCON_L_PID: u16 = 0x2006;
    pub const JOYCON_R_PID: u16 = 0x2007;

    pub fn new(name: &str, vendor_id: u16, product_id: u16, controller_type: SwitchControllerType) -> Self {
        Self {
            driver: GamepadDriver::nintendo(name, vendor_id, product_id),
            controller_type,
            player_leds: AtomicU8::new(0),
            home_led: AtomicU8::new(0),
            motion: RwLock::new(MotionData::default()),
        }
    }

    /// Create Pro Controller
    pub fn pro_controller() -> Self {
        Self::new("Nintendo Switch Pro Controller", Self::PRO_VID, Self::PRO_PID, SwitchControllerType::Pro)
    }

    /// Create left Joy-Con
    pub fn joycon_left() -> Self {
        Self::new("Joy-Con (L)", Self::PRO_VID, Self::JOYCON_L_PID, SwitchControllerType::JoyConL)
    }

    /// Create right Joy-Con
    pub fn joycon_right() -> Self {
        Self::new("Joy-Con (R)", Self::PRO_VID, Self::JOYCON_R_PID, SwitchControllerType::JoyConR)
    }

    /// Register with input subsystem
    pub fn register(&self) -> Result<(), InputError> {
        self.driver.register()
    }

    /// Set player LED pattern
    pub fn set_player_leds(&self, pattern: u8) {
        self.player_leds.store(pattern, Ordering::Relaxed);
        // Send command to controller
    }

    /// Set home LED brightness
    pub fn set_home_led(&self, brightness: u8) {
        self.home_led.store(brightness, Ordering::Relaxed);
        // Send command to controller
    }

    /// Get driver reference
    pub fn driver(&self) -> &GamepadDriver {
        &self.driver
    }
}

/// Gamepad input handler
pub struct GamepadHandler {
    /// Handler name
    name: String,
    /// Connected gamepads
    gamepads: RwLock<Vec<Arc<GamepadDriver>>>,
    /// Event callback
    callback: RwLock<Option<Box<dyn Fn(usize, &InputEvent) + Send + Sync>>>,
}

impl GamepadHandler {
    pub fn new(name: &str) -> Self {
        Self {
            name: String::from(name),
            gamepads: RwLock::new(Vec::new()),
            callback: RwLock::new(None),
        }
    }

    /// Set event callback
    pub fn set_callback<F>(&self, callback: F)
    where
        F: Fn(usize, &InputEvent) + Send + Sync + 'static,
    {
        *self.callback.write() = Some(Box::new(callback));
    }

    /// Get connected gamepad count
    pub fn count(&self) -> usize {
        self.gamepads.read().len()
    }
}

impl InputHandler for GamepadHandler {
    fn name(&self) -> &str {
        &self.name
    }

    fn match_device(&self, device: &InputDevice) -> bool {
        let caps = device.capabilities.read();
        // Must have gamepad buttons and analog axes
        caps.has_evbit(EV_KEY) &&
        caps.has_evbit(EV_ABS) &&
        caps.has_keybit(BTN_A)
    }

    fn connect(&self, _device: &InputDevice) -> Result<(), InputError> {
        Ok(())
    }

    fn disconnect(&self, _device: &InputDevice) {}

    fn event(&self, _device: &InputDevice, event: &InputEvent) {
        if let Some(ref callback) = *self.callback.read() {
            callback(0, event);
        }
    }
}

/// Virtual gamepad for testing and software input
pub struct VirtualGamepad {
    /// Gamepad driver
    driver: GamepadDriver,
}

impl VirtualGamepad {
    pub fn new() -> Self {
        Self {
            driver: GamepadDriver::new("Virtual Gamepad", 0, 0, GamepadType::Xbox),
        }
    }

    /// Register with input subsystem
    pub fn register(&self) -> Result<(), InputError> {
        self.driver.register()
    }

    /// Press button
    pub fn press(&self, button: GamepadButton) {
        self.driver.report_button(button, true);
        self.driver.sync();
    }

    /// Release button
    pub fn release(&self, button: GamepadButton) {
        self.driver.report_button(button, false);
        self.driver.sync();
    }

    /// Set stick position
    pub fn set_stick(&self, left: bool, x: f32, y: f32) {
        let x_val = (x * 32767.0) as i32;
        let y_val = (y * 32767.0) as i32;

        if left {
            self.driver.report_axis(GamepadAxis::LeftStickX, x_val);
            self.driver.report_axis(GamepadAxis::LeftStickY, y_val);
        } else {
            self.driver.report_axis(GamepadAxis::RightStickX, x_val);
            self.driver.report_axis(GamepadAxis::RightStickY, y_val);
        }
        self.driver.sync();
    }

    /// Set trigger value
    pub fn set_trigger(&self, left: bool, value: f32) {
        let val = (value * 255.0) as i32;
        if left {
            self.driver.report_axis(GamepadAxis::LeftTrigger, val);
        } else {
            self.driver.report_axis(GamepadAxis::RightTrigger, val);
        }
        self.driver.sync();
    }

    /// Get driver reference
    pub fn driver(&self) -> &GamepadDriver {
        &self.driver
    }
}

/// Initialize gamepad subsystem
pub fn init() {
    // Gamepads are initialized when detected via USB enumeration
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gamepad_buttons() {
        let mut buttons = GamepadButtons::new();
        assert!(!buttons.is_pressed(GamepadButton::South));

        buttons.set(GamepadButton::South, true);
        assert!(buttons.is_pressed(GamepadButton::South));
        assert_eq!(buttons.pressed_count(), 1);

        buttons.set(GamepadButton::South, false);
        assert!(!buttons.is_pressed(GamepadButton::South));
    }

    #[test]
    fn test_stick_position() {
        let mut stick = StickPosition::new();
        stick.deadzone = 0; // Disable deadzone for test

        stick.raw_x = 32767;
        stick.raw_y = 0;
        assert!((stick.x() - 1.0).abs() < 0.01);
        assert!(stick.y().abs() < 0.01);

        stick.raw_x = 0;
        stick.raw_y = -32768;
        assert!(stick.x().abs() < 0.01);
        assert!((stick.y() + 1.0).abs() < 0.01);
    }

    #[test]
    fn test_stick_deadzone() {
        let mut stick = StickPosition::new();
        stick.deadzone = 8000;
        stick.raw_x = 4000;
        stick.raw_y = 4000;

        // Should be in deadzone
        assert!(stick.x().abs() < 0.01);
        assert!(stick.y().abs() < 0.01);
        assert!(stick.is_neutral());
    }

    #[test]
    fn test_trigger() {
        let mut trigger = TriggerState::new();
        trigger.threshold = 30;
        trigger.raw = 0;
        assert!(!trigger.is_pressed());
        assert!(trigger.value() < 0.01);

        trigger.raw = 255;
        assert!(trigger.is_pressed());
        assert!((trigger.value() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_button_codes() {
        assert_eq!(GamepadButton::South.to_evdev_code(), BTN_A);
        assert_eq!(GamepadButton::from_evdev_code(BTN_A), Some(GamepadButton::South));
    }
}
