//! Mouse and Pointer Input Driver
//!
//! This module implements comprehensive mouse support including:
//!
//! - PS/2 mouse driver (standard and IntelliMouse)
//! - USB HID mouse support
//! - Relative and absolute positioning
//! - Multiple button support
//! - Scroll wheel handling (vertical and horizontal)
//! - Mouse acceleration and sensitivity
//! - Pointer movement smoothing

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use spin::{RwLock, Mutex};

use crate::math::F32Ext;
use super::events::*;
use super::{InputDevice, InputId, InputHandler, InputError, InputEvent, input_manager, BUS_I8042};

/// PS/2 mouse I/O ports
const PS2_DATA_PORT: u16 = 0x60;
const PS2_STATUS_PORT: u16 = 0x64;
const PS2_COMMAND_PORT: u16 = 0x64;

/// PS/2 mouse commands
const MOUSE_CMD_RESET: u8 = 0xFF;
const MOUSE_CMD_RESEND: u8 = 0xFE;
const MOUSE_CMD_SET_DEFAULTS: u8 = 0xF6;
const MOUSE_CMD_DISABLE_DATA: u8 = 0xF5;
const MOUSE_CMD_ENABLE_DATA: u8 = 0xF4;
const MOUSE_CMD_SET_SAMPLE_RATE: u8 = 0xF3;
const MOUSE_CMD_GET_DEVICE_ID: u8 = 0xF2;
const MOUSE_CMD_SET_REMOTE_MODE: u8 = 0xF0;
const MOUSE_CMD_SET_WRAP_MODE: u8 = 0xEE;
const MOUSE_CMD_RESET_WRAP_MODE: u8 = 0xEC;
const MOUSE_CMD_READ_DATA: u8 = 0xEB;
const MOUSE_CMD_SET_STREAM_MODE: u8 = 0xEA;
const MOUSE_CMD_STATUS_REQUEST: u8 = 0xE9;
const MOUSE_CMD_SET_RESOLUTION: u8 = 0xE8;
const MOUSE_CMD_SET_SCALING_2_1: u8 = 0xE7;
const MOUSE_CMD_SET_SCALING_1_1: u8 = 0xE6;

/// PS/2 mouse responses
const MOUSE_ACK: u8 = 0xFA;
const MOUSE_RESEND: u8 = 0xFE;
const MOUSE_SELF_TEST_PASS: u8 = 0xAA;

/// Mouse device types
const MOUSE_ID_STANDARD: u8 = 0x00;
const MOUSE_ID_INTELLIMOUSE: u8 = 0x03;
const MOUSE_ID_INTELLIMOUSE_EXPLORER: u8 = 0x04;

/// Mouse button state
#[derive(Debug, Clone, Copy, Default)]
pub struct MouseButtons {
    /// Left button pressed
    pub left: bool,
    /// Right button pressed
    pub right: bool,
    /// Middle button pressed
    pub middle: bool,
    /// Side button (button 4) pressed
    pub side: bool,
    /// Extra button (button 5) pressed
    pub extra: bool,
}

impl MouseButtons {
    pub const fn new() -> Self {
        Self {
            left: false,
            right: false,
            middle: false,
            side: false,
            extra: false,
        }
    }

    /// Create from PS/2 button byte
    pub fn from_ps2(byte: u8) -> Self {
        Self {
            left: (byte & 0x01) != 0,
            right: (byte & 0x02) != 0,
            middle: (byte & 0x04) != 0,
            side: false,
            extra: false,
        }
    }

    /// Create from IntelliMouse Explorer button byte
    pub fn from_explorer(byte: u8, extra_byte: u8) -> Self {
        Self {
            left: (byte & 0x01) != 0,
            right: (byte & 0x02) != 0,
            middle: (byte & 0x04) != 0,
            side: (extra_byte & 0x10) != 0,
            extra: (extra_byte & 0x20) != 0,
        }
    }

    /// Create from USB HID button byte
    pub fn from_hid(buttons: u8) -> Self {
        Self {
            left: (buttons & 0x01) != 0,
            right: (buttons & 0x02) != 0,
            middle: (buttons & 0x04) != 0,
            side: (buttons & 0x08) != 0,
            extra: (buttons & 0x10) != 0,
        }
    }

    /// Get button count
    pub fn count(&self) -> u8 {
        let mut count = 0;
        if self.left { count += 1; }
        if self.right { count += 1; }
        if self.middle { count += 1; }
        if self.side { count += 1; }
        if self.extra { count += 1; }
        count
    }
}

/// Mouse movement data
#[derive(Debug, Clone, Copy, Default)]
pub struct MouseMovement {
    /// X movement (positive = right)
    pub dx: i32,
    /// Y movement (positive = down in screen coordinates)
    pub dy: i32,
    /// Vertical scroll (positive = up)
    pub wheel: i32,
    /// Horizontal scroll (positive = right)
    pub hwheel: i32,
}

impl MouseMovement {
    pub const fn new() -> Self {
        Self {
            dx: 0,
            dy: 0,
            wheel: 0,
            hwheel: 0,
        }
    }

    pub fn is_zero(&self) -> bool {
        self.dx == 0 && self.dy == 0 && self.wheel == 0 && self.hwheel == 0
    }
}

/// Mouse acceleration profile
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAcceleration {
    /// No acceleration (linear)
    None,
    /// Standard acceleration curve
    Standard,
    /// Adaptive acceleration
    Adaptive,
    /// Gaming (low acceleration, high precision)
    Gaming,
}

impl Default for MouseAcceleration {
    fn default() -> Self {
        Self::Standard
    }
}

/// Mouse sensitivity settings
#[derive(Debug, Clone, Copy)]
pub struct MouseSensitivity {
    /// Base sensitivity multiplier (1.0 = default)
    pub multiplier: f32,
    /// Acceleration profile
    pub acceleration: MouseAcceleration,
    /// Acceleration factor (for adaptive mode)
    pub accel_factor: f32,
    /// Speed threshold for acceleration
    pub accel_threshold: f32,
    /// Scroll sensitivity
    pub scroll_multiplier: f32,
}

impl Default for MouseSensitivity {
    fn default() -> Self {
        Self {
            multiplier: 1.0,
            acceleration: MouseAcceleration::Standard,
            accel_factor: 0.5,
            accel_threshold: 5.0,
            scroll_multiplier: 1.0,
        }
    }
}

impl MouseSensitivity {
    /// Apply sensitivity to movement
    pub fn apply(&self, dx: i32, dy: i32) -> (i32, i32) {
        let fx = dx as f32;
        let fy = dy as f32;

        let (ax, ay) = match self.acceleration {
            MouseAcceleration::None => {
                (fx * self.multiplier, fy * self.multiplier)
            }
            MouseAcceleration::Standard => {
                let speed = (fx * fx + fy * fy).sqrt();
                let accel = 1.0 + (speed * 0.1).min(2.0);
                (fx * self.multiplier * accel, fy * self.multiplier * accel)
            }
            MouseAcceleration::Adaptive => {
                let speed = (fx * fx + fy * fy).sqrt();
                let accel = if speed > self.accel_threshold {
                    1.0 + (speed - self.accel_threshold) * self.accel_factor
                } else {
                    1.0
                };
                (fx * self.multiplier * accel, fy * self.multiplier * accel)
            }
            MouseAcceleration::Gaming => {
                // No acceleration, just sensitivity
                (fx * self.multiplier, fy * self.multiplier)
            }
        };

        (ax.round() as i32, ay.round() as i32)
    }

    /// Apply sensitivity to scroll
    pub fn apply_scroll(&self, wheel: i32) -> i32 {
        (wheel as f32 * self.scroll_multiplier).round() as i32
    }
}

/// PS/2 mouse protocol type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ps2MouseType {
    /// Standard 3-byte protocol
    Standard,
    /// IntelliMouse 4-byte protocol (scroll wheel)
    IntelliMouse,
    /// IntelliMouse Explorer 4-byte protocol (5 buttons + scroll)
    IntelliMouseExplorer,
}

/// PS/2 mouse state
pub struct Ps2MouseState {
    /// Mouse type
    mouse_type: Ps2MouseType,
    /// Packet buffer
    buffer: [u8; 4],
    /// Current byte index in packet
    byte_index: usize,
    /// Previous button state
    prev_buttons: MouseButtons,
    /// Sensitivity settings
    sensitivity: MouseSensitivity,
}

impl Ps2MouseState {
    pub const fn new() -> Self {
        Self {
            mouse_type: Ps2MouseType::Standard,
            buffer: [0; 4],
            byte_index: 0,
            prev_buttons: MouseButtons::new(),
            sensitivity: MouseSensitivity {
                multiplier: 1.0,
                acceleration: MouseAcceleration::Standard,
                accel_factor: 0.5,
                accel_threshold: 5.0,
                scroll_multiplier: 1.0,
            },
        }
    }

    /// Get packet size based on mouse type
    fn packet_size(&self) -> usize {
        match self.mouse_type {
            Ps2MouseType::Standard => 3,
            Ps2MouseType::IntelliMouse | Ps2MouseType::IntelliMouseExplorer => 4,
        }
    }

    /// Process a byte from the mouse
    pub fn process_byte(&mut self, byte: u8) -> Option<(MouseButtons, MouseMovement)> {
        // First byte must have bit 3 set (alignment check)
        if self.byte_index == 0 && (byte & 0x08) == 0 {
            // Out of sync, discard
            return None;
        }

        self.buffer[self.byte_index] = byte;
        self.byte_index += 1;

        if self.byte_index < self.packet_size() {
            return None;
        }

        // Complete packet received
        self.byte_index = 0;

        let buttons = match self.mouse_type {
            Ps2MouseType::IntelliMouseExplorer => {
                MouseButtons::from_explorer(self.buffer[0], self.buffer[3])
            }
            _ => MouseButtons::from_ps2(self.buffer[0]),
        };

        // Extract movement with sign extension
        let mut dx = self.buffer[1] as i32;
        let mut dy = self.buffer[2] as i32;

        // Apply sign from flags byte
        if (self.buffer[0] & 0x10) != 0 {
            dx -= 256;
        }
        if (self.buffer[0] & 0x20) != 0 {
            dy -= 256;
        }

        // PS/2 Y is inverted (positive = up, we want positive = down)
        dy = -dy;

        // Check overflow bits
        if (self.buffer[0] & 0xC0) != 0 {
            // X or Y overflow, discard packet
            dx = 0;
            dy = 0;
        }

        // Extract scroll wheel
        let wheel = match self.mouse_type {
            Ps2MouseType::IntelliMouse => {
                // 4-bit signed value
                let w = self.buffer[3] as i8 as i32;
                -w // Invert: positive scroll = up
            }
            Ps2MouseType::IntelliMouseExplorer => {
                // 4-bit signed value in lower nibble
                let w = (self.buffer[3] & 0x0F) as i8;
                let w = if w > 7 { w - 16 } else { w };
                -w as i32
            }
            _ => 0,
        };

        let movement = MouseMovement {
            dx,
            dy,
            wheel,
            hwheel: 0,
        };

        self.prev_buttons = buttons;

        Some((buttons, movement))
    }

    /// Set mouse type
    pub fn set_type(&mut self, mouse_type: Ps2MouseType) {
        self.mouse_type = mouse_type;
        self.byte_index = 0;
    }
}

/// PS/2 mouse driver
pub struct Ps2Mouse {
    /// Input device
    device: Arc<InputDevice>,
    /// Mouse state
    state: Mutex<Ps2MouseState>,
    /// Previous button state for change detection
    prev_buttons: Mutex<MouseButtons>,
    /// Initialized flag
    initialized: AtomicBool,
}

impl Ps2Mouse {
    pub fn new() -> Self {
        let device = InputDevice::mouse(
            "PS/2 Mouse",
            InputId::new(BUS_I8042, 0x0002, 0x0001, 1),
        );

        Self {
            device: Arc::new(device),
            state: Mutex::new(Ps2MouseState::new()),
            prev_buttons: Mutex::new(MouseButtons::new()),
            initialized: AtomicBool::new(false),
        }
    }

    /// Initialize the PS/2 mouse
    pub fn init(&self) -> Result<(), InputError> {
        if self.initialized.swap(true, Ordering::SeqCst) {
            return Ok(());
        }

        // In a real implementation, we would:
        // 1. Reset the mouse
        // 2. Detect mouse type (try to enable IntelliMouse)
        // 3. Set sample rate and resolution
        // 4. Enable data reporting
        // 5. Set up IRQ handler

        // Try to detect IntelliMouse by setting magic sample rates
        // (200, 100, 80) -> ID should become 0x03

        // Register with input subsystem
        input_manager().register_device(self.device.clone())?;

        Ok(())
    }

    /// Handle mouse interrupt
    pub fn handle_interrupt(&self, byte: u8) {
        let mut state = self.state.lock();

        if let Some((buttons, movement)) = state.process_byte(byte) {
            self.report_changes(buttons, movement);
        }
    }

    /// Report button and movement changes
    fn report_changes(&self, buttons: MouseButtons, movement: MouseMovement) {
        let mut prev = self.prev_buttons.lock();

        // Report button changes
        if buttons.left != prev.left {
            self.device.report_key(BTN_LEFT, buttons.left);
        }
        if buttons.right != prev.right {
            self.device.report_key(BTN_RIGHT, buttons.right);
        }
        if buttons.middle != prev.middle {
            self.device.report_key(BTN_MIDDLE, buttons.middle);
        }
        if buttons.side != prev.side {
            self.device.report_key(BTN_SIDE, buttons.side);
        }
        if buttons.extra != prev.extra {
            self.device.report_key(BTN_EXTRA, buttons.extra);
        }

        *prev = buttons;

        // Report movement
        if movement.dx != 0 {
            self.device.report_rel(REL_X, movement.dx);
        }
        if movement.dy != 0 {
            self.device.report_rel(REL_Y, movement.dy);
        }
        if movement.wheel != 0 {
            self.device.report_rel(REL_WHEEL, movement.wheel);
        }
        if movement.hwheel != 0 {
            self.device.report_rel(REL_HWHEEL, movement.hwheel);
        }

        self.device.sync();
    }

    /// Get the input device
    pub fn device(&self) -> &Arc<InputDevice> {
        &self.device
    }

    /// Set mouse type
    pub fn set_type(&self, mouse_type: Ps2MouseType) {
        self.state.lock().set_type(mouse_type);
    }
}

/// USB HID mouse driver
pub struct UsbHidMouse {
    /// Input device
    device: Arc<InputDevice>,
    /// Previous button state
    prev_buttons: Mutex<MouseButtons>,
    /// Sensitivity settings
    sensitivity: RwLock<MouseSensitivity>,
    /// Report descriptor indicates absolute positioning
    is_absolute: AtomicBool,
}

impl UsbHidMouse {
    pub fn new(name: &str, vendor_id: u16, product_id: u16) -> Self {
        let device = InputDevice::mouse(
            name,
            InputId::usb(vendor_id, product_id, 1),
        );

        Self {
            device: Arc::new(device),
            prev_buttons: Mutex::new(MouseButtons::new()),
            sensitivity: RwLock::new(MouseSensitivity::default()),
            is_absolute: AtomicBool::new(false),
        }
    }

    /// Register with input subsystem
    pub fn register(&self) -> Result<(), InputError> {
        input_manager().register_device(self.device.clone())
    }

    /// Process USB HID mouse report (boot protocol)
    pub fn process_boot_report(&self, report: &[u8]) {
        if report.len() < 3 {
            return;
        }

        let buttons = MouseButtons::from_hid(report[0]);
        let dx = report[1] as i8 as i32;
        let dy = report[2] as i8 as i32;
        let wheel = if report.len() >= 4 {
            -(report[3] as i8 as i32) // Invert for natural scrolling
        } else {
            0
        };

        // Apply sensitivity
        let sens = self.sensitivity.read();
        let (dx, dy) = sens.apply(dx, dy);
        let wheel = sens.apply_scroll(wheel);

        self.report_changes(buttons, MouseMovement { dx, dy, wheel, hwheel: 0 });
    }

    /// Process USB HID mouse report (report protocol with descriptor)
    pub fn process_report(&self, report: &[u8], _descriptor: &HidReportDescriptor) {
        // Parse according to descriptor
        // For now, use boot protocol format
        self.process_boot_report(report);
    }

    /// Report button and movement changes
    fn report_changes(&self, buttons: MouseButtons, movement: MouseMovement) {
        let mut prev = self.prev_buttons.lock();

        // Report button changes
        if buttons.left != prev.left {
            self.device.report_key(BTN_LEFT, buttons.left);
        }
        if buttons.right != prev.right {
            self.device.report_key(BTN_RIGHT, buttons.right);
        }
        if buttons.middle != prev.middle {
            self.device.report_key(BTN_MIDDLE, buttons.middle);
        }
        if buttons.side != prev.side {
            self.device.report_key(BTN_SIDE, buttons.side);
        }
        if buttons.extra != prev.extra {
            self.device.report_key(BTN_EXTRA, buttons.extra);
        }

        *prev = buttons;

        // Report movement
        if movement.dx != 0 {
            self.device.report_rel(REL_X, movement.dx);
        }
        if movement.dy != 0 {
            self.device.report_rel(REL_Y, movement.dy);
        }
        if movement.wheel != 0 {
            self.device.report_rel(REL_WHEEL, movement.wheel);
        }
        if movement.hwheel != 0 {
            self.device.report_rel(REL_HWHEEL, movement.hwheel);
        }

        self.device.sync();
    }

    /// Set sensitivity
    pub fn set_sensitivity(&self, sensitivity: MouseSensitivity) {
        *self.sensitivity.write() = sensitivity;
    }

    /// Get the input device
    pub fn device(&self) -> &Arc<InputDevice> {
        &self.device
    }
}

/// HID report descriptor (simplified)
pub struct HidReportDescriptor {
    /// Has X axis
    pub has_x: bool,
    /// Has Y axis
    pub has_y: bool,
    /// Has wheel
    pub has_wheel: bool,
    /// Has horizontal wheel
    pub has_hwheel: bool,
    /// Number of buttons
    pub button_count: u8,
    /// X is absolute
    pub x_absolute: bool,
    /// Y is absolute
    pub y_absolute: bool,
    /// X minimum value
    pub x_min: i32,
    /// X maximum value
    pub x_max: i32,
    /// Y minimum value
    pub y_min: i32,
    /// Y maximum value
    pub y_max: i32,
}

impl Default for HidReportDescriptor {
    fn default() -> Self {
        Self {
            has_x: true,
            has_y: true,
            has_wheel: true,
            has_hwheel: false,
            button_count: 5,
            x_absolute: false,
            y_absolute: false,
            x_min: -127,
            x_max: 127,
            y_min: -127,
            y_max: 127,
        }
    }
}

/// Mouse input handler
pub struct MouseHandler {
    /// Handler name
    name: String,
    /// Accumulated position (for absolute positioning)
    x: AtomicI32,
    y: AtomicI32,
    /// Screen bounds
    screen_width: AtomicI32,
    screen_height: AtomicI32,
    /// Event callback
    callback: RwLock<Option<Box<dyn Fn(&InputDevice, &InputEvent) + Send + Sync>>>,
}

impl MouseHandler {
    pub fn new(name: &str) -> Self {
        Self {
            name: String::from(name),
            x: AtomicI32::new(0),
            y: AtomicI32::new(0),
            screen_width: AtomicI32::new(1920),
            screen_height: AtomicI32::new(1080),
            callback: RwLock::new(None),
        }
    }

    /// Set screen bounds
    pub fn set_screen_bounds(&self, width: i32, height: i32) {
        self.screen_width.store(width, Ordering::Relaxed);
        self.screen_height.store(height, Ordering::Relaxed);
    }

    /// Get current position
    pub fn position(&self) -> (i32, i32) {
        (
            self.x.load(Ordering::Relaxed),
            self.y.load(Ordering::Relaxed),
        )
    }

    /// Set position
    pub fn set_position(&self, x: i32, y: i32) {
        let w = self.screen_width.load(Ordering::Relaxed);
        let h = self.screen_height.load(Ordering::Relaxed);
        self.x.store(x.clamp(0, w - 1), Ordering::Relaxed);
        self.y.store(y.clamp(0, h - 1), Ordering::Relaxed);
    }

    /// Set event callback
    pub fn set_callback<F>(&self, callback: F)
    where
        F: Fn(&InputDevice, &InputEvent) + Send + Sync + 'static,
    {
        *self.callback.write() = Some(Box::new(callback));
    }
}

impl InputHandler for MouseHandler {
    fn name(&self) -> &str {
        &self.name
    }

    fn match_device(&self, device: &InputDevice) -> bool {
        let caps = device.capabilities.read();
        caps.has_evbit(EV_REL) ||
        (caps.has_evbit(EV_KEY) && caps.has_keybit(BTN_LEFT))
    }

    fn connect(&self, _device: &InputDevice) -> Result<(), InputError> {
        Ok(())
    }

    fn disconnect(&self, _device: &InputDevice) {}

    fn event(&self, device: &InputDevice, event: &InputEvent) {
        // Track position for relative movement
        if event.ev_type == EV_REL {
            match event.code {
                REL_X => {
                    let w = self.screen_width.load(Ordering::Relaxed);
                    let old_x = self.x.load(Ordering::Relaxed);
                    let new_x = (old_x + event.value).clamp(0, w - 1);
                    self.x.store(new_x, Ordering::Relaxed);
                }
                REL_Y => {
                    let h = self.screen_height.load(Ordering::Relaxed);
                    let old_y = self.y.load(Ordering::Relaxed);
                    let new_y = (old_y + event.value).clamp(0, h - 1);
                    self.y.store(new_y, Ordering::Relaxed);
                }
                _ => {}
            }
        }

        if let Some(ref callback) = *self.callback.read() {
            callback(device, event);
        }
    }
}

/// Virtual mouse for software input
pub struct VirtualMouse {
    /// Input device
    device: Arc<InputDevice>,
    /// Current button state
    buttons: Mutex<MouseButtons>,
}

impl VirtualMouse {
    pub fn new() -> Self {
        let device = InputDevice::mouse(
            "Virtual Mouse",
            InputId::virtual_device(),
        );

        Self {
            device: Arc::new(device),
            buttons: Mutex::new(MouseButtons::new()),
        }
    }

    /// Register with input subsystem
    pub fn register(&self) -> Result<(), InputError> {
        input_manager().register_device(self.device.clone())
    }

    /// Move the mouse relatively
    pub fn move_rel(&self, dx: i32, dy: i32) {
        if dx != 0 {
            self.device.report_rel(REL_X, dx);
        }
        if dy != 0 {
            self.device.report_rel(REL_Y, dy);
        }
        self.device.sync();
    }

    /// Scroll the wheel
    pub fn scroll(&self, wheel: i32, hwheel: i32) {
        if wheel != 0 {
            self.device.report_rel(REL_WHEEL, wheel);
        }
        if hwheel != 0 {
            self.device.report_rel(REL_HWHEEL, hwheel);
        }
        self.device.sync();
    }

    /// Press a button
    pub fn press(&self, button: u16) {
        let mut buttons = self.buttons.lock();
        match button {
            BTN_LEFT => buttons.left = true,
            BTN_RIGHT => buttons.right = true,
            BTN_MIDDLE => buttons.middle = true,
            BTN_SIDE => buttons.side = true,
            BTN_EXTRA => buttons.extra = true,
            _ => return,
        }
        self.device.report_key(button, true);
        self.device.sync();
    }

    /// Release a button
    pub fn release(&self, button: u16) {
        let mut buttons = self.buttons.lock();
        match button {
            BTN_LEFT => buttons.left = false,
            BTN_RIGHT => buttons.right = false,
            BTN_MIDDLE => buttons.middle = false,
            BTN_SIDE => buttons.side = false,
            BTN_EXTRA => buttons.extra = false,
            _ => return,
        }
        self.device.report_key(button, false);
        self.device.sync();
    }

    /// Click a button (press + release)
    pub fn click(&self, button: u16) {
        self.press(button);
        self.release(button);
    }

    /// Double click
    pub fn double_click(&self, button: u16) {
        self.click(button);
        self.click(button);
    }

    /// Get the input device
    pub fn device(&self) -> &Arc<InputDevice> {
        &self.device
    }
}

/// Movement smoothing filter
pub struct MovementFilter {
    /// History buffer for X
    x_history: [i32; 4],
    /// History buffer for Y
    y_history: [i32; 4],
    /// Current index
    index: usize,
    /// Filter enabled
    enabled: bool,
}

impl MovementFilter {
    pub const fn new() -> Self {
        Self {
            x_history: [0; 4],
            y_history: [0; 4],
            index: 0,
            enabled: true,
        }
    }

    /// Add a sample and get filtered output
    pub fn filter(&mut self, dx: i32, dy: i32) -> (i32, i32) {
        if !self.enabled {
            return (dx, dy);
        }

        self.x_history[self.index] = dx;
        self.y_history[self.index] = dy;
        self.index = (self.index + 1) % 4;

        // Simple averaging filter
        let avg_x: i32 = self.x_history.iter().sum::<i32>() / 4;
        let avg_y: i32 = self.y_history.iter().sum::<i32>() / 4;

        (avg_x, avg_y)
    }

    /// Reset the filter
    pub fn reset(&mut self) {
        self.x_history = [0; 4];
        self.y_history = [0; 4];
        self.index = 0;
    }

    /// Enable/disable filtering
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.reset();
        }
    }
}

/// Global PS/2 mouse instance
static PS2_MOUSE: spin::Once<Ps2Mouse> = spin::Once::new();

/// Get the PS/2 mouse
pub fn ps2_mouse() -> &'static Ps2Mouse {
    PS2_MOUSE.call_once(|| Ps2Mouse::new())
}

/// Initialize mouse subsystem
pub fn init() {
    // Initialize PS/2 mouse
    let mouse = ps2_mouse();
    if let Err(_e) = mouse.init() {
        // Log error
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mouse_buttons() {
        let buttons = MouseButtons::from_ps2(0x07);
        assert!(buttons.left);
        assert!(buttons.right);
        assert!(buttons.middle);
        assert!(!buttons.side);
    }

    #[test]
    fn test_ps2_mouse_packet() {
        let mut state = Ps2MouseState::new();

        // Simulate a standard 3-byte packet: buttons=0x09, dx=10, dy=-5
        // Flags: 0x09 = left button + Y negative bit set
        state.process_byte(0x09); // Flags with bit 3 set
        state.process_byte(10);   // X movement

        // Complete packet
        let result = state.process_byte(5);  // Y movement (will be inverted)

        assert!(result.is_some());
        let (buttons, movement) = result.unwrap();
        assert!(buttons.left);
        assert!(!buttons.right);
        assert_eq!(movement.dx, 10);
    }

    #[test]
    fn test_sensitivity() {
        let sens = MouseSensitivity {
            multiplier: 2.0,
            acceleration: MouseAcceleration::None,
            ..Default::default()
        };

        let (dx, dy) = sens.apply(10, 5);
        assert_eq!(dx, 20);
        assert_eq!(dy, 10);
    }

    #[test]
    fn test_movement_filter() {
        let mut filter = MovementFilter::new();

        // Add samples
        filter.filter(10, 10);
        filter.filter(10, 10);
        filter.filter(10, 10);
        let (dx, dy) = filter.filter(10, 10);

        // Should average to 10
        assert_eq!(dx, 10);
        assert_eq!(dy, 10);
    }

    #[test]
    fn test_hid_buttons() {
        let buttons = MouseButtons::from_hid(0x1F);
        assert!(buttons.left);
        assert!(buttons.right);
        assert!(buttons.middle);
        assert!(buttons.side);
        assert!(buttons.extra);
    }
}
