//! BuildOS Input Subsystem
//!
//! This module implements a comprehensive input subsystem compatible with
//! the Linux evdev (event device) interface. It provides unified handling
//! for keyboards, mice, touchscreens, gamepads, and other input devices.
//!
//! # Architecture
//!
//! The input subsystem follows a layered architecture:
//!
//! ```text
//! +------------------+  +------------------+  +------------------+
//! |   Applications   |  |   Applications   |  |   Applications   |
//! +--------+---------+  +--------+---------+  +--------+---------+
//!          |                     |                     |
//!          v                     v                     v
//! +------------------------------------------------------------------+
//! |                    Input Event Interface                          |
//! |              (evdev - /dev/input/eventN)                         |
//! +------------------------------------------------------------------+
//!          ^                     ^                     ^
//!          |                     |                     |
//! +--------+---------+  +--------+---------+  +--------+---------+
//! |  Input Handler   |  |  Input Handler   |  |  Input Handler   |
//! |   (keyboard)     |  |    (mouse)       |  |  (touchscreen)   |
//! +--------+---------+  +--------+---------+  +--------+---------+
//!          ^                     ^                     ^
//!          |                     |                     |
//! +------------------------------------------------------------------+
//! |                    Input Core Layer                               |
//! |           (device registration, event dispatch)                   |
//! +------------------------------------------------------------------+
//!          ^                     ^                     ^
//!          |                     |                     |
//! +--------+---------+  +--------+---------+  +--------+---------+
//! |   USB HID Driver |  |   PS/2 Driver    |  |   I2C Touch      |
//! +------------------+  +------------------+  +------------------+
//! ```
//!
//! # Features
//!
//! - **Event Types**: KEY, REL, ABS, MSC, SW, LED, SND, REP, FF
//! - **Device Support**: Keyboards, mice, touchscreens, gamepads, tablets
//! - **Multi-touch**: Full MT protocol support (Type A and Type B)
//! - **Force Feedback**: Haptic feedback for gamepads and wheels
//! - **Hot-plug**: Dynamic device registration and removal

#![allow(dead_code)]

pub mod events;
pub mod keyboard;
pub mod mouse;
pub mod touch;
pub mod gamepad;
pub mod ff;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicBool, Ordering};
use spin::{RwLock, Mutex};

use events::*;

/// Input subsystem version
pub const INPUT_VERSION: u32 = 0x010100; // 1.1.0

/// Maximum number of input devices
pub const INPUT_MAX_DEVICES: usize = 256;

/// Maximum number of handlers per device
pub const INPUT_MAX_HANDLERS: usize = 16;

/// Maximum event queue size per device
pub const INPUT_EVENT_QUEUE_SIZE: usize = 256;

/// Input device ID structure
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct InputId {
    /// Bus type (USB, Bluetooth, etc.)
    pub bustype: u16,
    /// Vendor ID
    pub vendor: u16,
    /// Product ID
    pub product: u16,
    /// Version
    pub version: u16,
}

impl InputId {
    pub const fn new(bustype: u16, vendor: u16, product: u16, version: u16) -> Self {
        Self { bustype, vendor, product, version }
    }

    pub const fn usb(vendor: u16, product: u16, version: u16) -> Self {
        Self::new(BUS_USB, vendor, product, version)
    }

    pub const fn bluetooth(vendor: u16, product: u16, version: u16) -> Self {
        Self::new(BUS_BLUETOOTH, vendor, product, version)
    }

    pub const fn virtual_device() -> Self {
        Self::new(BUS_VIRTUAL, 0, 0, 1)
    }
}

/// Bus types
pub const BUS_PCI: u16 = 0x01;
pub const BUS_ISAPNP: u16 = 0x02;
pub const BUS_USB: u16 = 0x03;
pub const BUS_HIL: u16 = 0x04;
pub const BUS_BLUETOOTH: u16 = 0x05;
pub const BUS_VIRTUAL: u16 = 0x06;
pub const BUS_ISA: u16 = 0x10;
pub const BUS_I8042: u16 = 0x11;
pub const BUS_XTKBD: u16 = 0x12;
pub const BUS_RS232: u16 = 0x13;
pub const BUS_GAMEPORT: u16 = 0x14;
pub const BUS_PARPORT: u16 = 0x15;
pub const BUS_AMIGA: u16 = 0x16;
pub const BUS_ADB: u16 = 0x17;
pub const BUS_I2C: u16 = 0x18;
pub const BUS_HOST: u16 = 0x19;
pub const BUS_GSC: u16 = 0x1A;
pub const BUS_ATARI: u16 = 0x1B;
pub const BUS_SPI: u16 = 0x1C;
pub const BUS_RMI: u16 = 0x1D;
pub const BUS_CEC: u16 = 0x1E;
pub const BUS_INTEL_ISHTP: u16 = 0x1F;

/// Input device capabilities bitmap
#[derive(Debug, Clone)]
pub struct InputCapabilities {
    /// Event types supported (EV_KEY, EV_REL, etc.)
    pub evbit: [u64; 1],
    /// Keys/buttons supported
    pub keybit: [u64; 12],  // KEY_CNT / 64
    /// Relative axes supported
    pub relbit: [u64; 1],
    /// Absolute axes supported
    pub absbit: [u64; 1],
    /// Miscellaneous events supported
    pub mscbit: [u64; 1],
    /// LEDs supported
    pub ledbit: [u64; 1],
    /// Sounds supported
    pub sndbit: [u64; 1],
    /// Force feedback effects supported
    pub ffbit: [u64; 2],
    /// Switches supported
    pub swbit: [u64; 1],
}

impl InputCapabilities {
    pub const fn new() -> Self {
        Self {
            evbit: [0; 1],
            keybit: [0; 12],
            relbit: [0; 1],
            absbit: [0; 1],
            mscbit: [0; 1],
            ledbit: [0; 1],
            sndbit: [0; 1],
            ffbit: [0; 2],
            swbit: [0; 1],
        }
    }

    /// Set event type capability
    pub fn set_evbit(&mut self, ev_type: u16) {
        let idx = (ev_type / 64) as usize;
        let bit = ev_type % 64;
        if idx < self.evbit.len() {
            self.evbit[idx] |= 1u64 << bit;
        }
    }

    /// Check if event type is supported
    pub fn has_evbit(&self, ev_type: u16) -> bool {
        let idx = (ev_type / 64) as usize;
        let bit = ev_type % 64;
        if idx < self.evbit.len() {
            (self.evbit[idx] & (1u64 << bit)) != 0
        } else {
            false
        }
    }

    /// Set key/button capability
    pub fn set_keybit(&mut self, code: u16) {
        let idx = (code / 64) as usize;
        let bit = code % 64;
        if idx < self.keybit.len() {
            self.keybit[idx] |= 1u64 << bit;
        }
    }

    /// Check if key/button is supported
    pub fn has_keybit(&self, code: u16) -> bool {
        let idx = (code / 64) as usize;
        let bit = code % 64;
        if idx < self.keybit.len() {
            (self.keybit[idx] & (1u64 << bit)) != 0
        } else {
            false
        }
    }

    /// Set relative axis capability
    pub fn set_relbit(&mut self, code: u16) {
        let idx = (code / 64) as usize;
        let bit = code % 64;
        if idx < self.relbit.len() {
            self.relbit[idx] |= 1u64 << bit;
        }
    }

    /// Set absolute axis capability
    pub fn set_absbit(&mut self, code: u16) {
        let idx = (code / 64) as usize;
        let bit = code % 64;
        if idx < self.absbit.len() {
            self.absbit[idx] |= 1u64 << bit;
        }
    }

    /// Set LED capability
    pub fn set_ledbit(&mut self, code: u16) {
        let idx = (code / 64) as usize;
        let bit = code % 64;
        if idx < self.ledbit.len() {
            self.ledbit[idx] |= 1u64 << bit;
        }
    }

    /// Set force feedback capability
    pub fn set_ffbit(&mut self, code: u16) {
        let idx = (code / 64) as usize;
        let bit = code % 64;
        if idx < self.ffbit.len() {
            self.ffbit[idx] |= 1u64 << bit;
        }
    }

    /// Set switch capability
    pub fn set_swbit(&mut self, code: u16) {
        let idx = (code / 64) as usize;
        let bit = code % 64;
        if idx < self.swbit.len() {
            self.swbit[idx] |= 1u64 << bit;
        }
    }

    /// Create keyboard capabilities
    pub fn keyboard() -> Self {
        let mut caps = Self::new();
        caps.set_evbit(EV_KEY);
        caps.set_evbit(EV_REP);
        caps.set_evbit(EV_LED);
        caps.set_evbit(EV_MSC);

        // Standard keyboard keys
        for key in 0..256u16 {
            caps.set_keybit(key);
        }

        // LEDs
        caps.set_ledbit(LED_NUML);
        caps.set_ledbit(LED_CAPSL);
        caps.set_ledbit(LED_SCROLLL);

        caps
    }

    /// Create mouse capabilities
    pub fn mouse() -> Self {
        let mut caps = Self::new();
        caps.set_evbit(EV_KEY);
        caps.set_evbit(EV_REL);

        // Mouse buttons
        caps.set_keybit(BTN_LEFT);
        caps.set_keybit(BTN_RIGHT);
        caps.set_keybit(BTN_MIDDLE);
        caps.set_keybit(BTN_SIDE);
        caps.set_keybit(BTN_EXTRA);

        // Relative axes
        caps.set_relbit(REL_X);
        caps.set_relbit(REL_Y);
        caps.set_relbit(REL_WHEEL);
        caps.set_relbit(REL_HWHEEL);

        caps
    }

    /// Create touchscreen capabilities
    pub fn touchscreen() -> Self {
        let mut caps = Self::new();
        caps.set_evbit(EV_KEY);
        caps.set_evbit(EV_ABS);

        // Touch button
        caps.set_keybit(BTN_TOUCH);

        // Absolute axes
        caps.set_absbit(ABS_X);
        caps.set_absbit(ABS_Y);
        caps.set_absbit(ABS_PRESSURE);

        // Multi-touch
        caps.set_absbit(ABS_MT_SLOT);
        caps.set_absbit(ABS_MT_POSITION_X);
        caps.set_absbit(ABS_MT_POSITION_Y);
        caps.set_absbit(ABS_MT_TRACKING_ID);
        caps.set_absbit(ABS_MT_PRESSURE);
        caps.set_absbit(ABS_MT_TOUCH_MAJOR);
        caps.set_absbit(ABS_MT_TOUCH_MINOR);

        caps
    }

    /// Create gamepad capabilities
    pub fn gamepad() -> Self {
        let mut caps = Self::new();
        caps.set_evbit(EV_KEY);
        caps.set_evbit(EV_ABS);
        caps.set_evbit(EV_FF);

        // Gamepad buttons
        caps.set_keybit(BTN_A);
        caps.set_keybit(BTN_B);
        caps.set_keybit(BTN_X);
        caps.set_keybit(BTN_Y);
        caps.set_keybit(BTN_TL);
        caps.set_keybit(BTN_TR);
        caps.set_keybit(BTN_TL2);
        caps.set_keybit(BTN_TR2);
        caps.set_keybit(BTN_SELECT);
        caps.set_keybit(BTN_START);
        caps.set_keybit(BTN_MODE);
        caps.set_keybit(BTN_THUMBL);
        caps.set_keybit(BTN_THUMBR);
        caps.set_keybit(BTN_DPAD_UP);
        caps.set_keybit(BTN_DPAD_DOWN);
        caps.set_keybit(BTN_DPAD_LEFT);
        caps.set_keybit(BTN_DPAD_RIGHT);

        // Analog sticks
        caps.set_absbit(ABS_X);
        caps.set_absbit(ABS_Y);
        caps.set_absbit(ABS_RX);
        caps.set_absbit(ABS_RY);
        caps.set_absbit(ABS_Z);  // Left trigger
        caps.set_absbit(ABS_RZ); // Right trigger
        caps.set_absbit(ABS_HAT0X);
        caps.set_absbit(ABS_HAT0Y);

        // Force feedback
        caps.set_ffbit(FF_RUMBLE);
        caps.set_ffbit(FF_PERIODIC);
        caps.set_ffbit(FF_CONSTANT);

        caps
    }
}

/// Absolute axis information
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct AbsInfo {
    /// Current value
    pub value: i32,
    /// Minimum value
    pub minimum: i32,
    /// Maximum value
    pub maximum: i32,
    /// Fuzz (noise filter)
    pub fuzz: i32,
    /// Flat (deadzone)
    pub flat: i32,
    /// Resolution (units per mm)
    pub resolution: i32,
}

impl AbsInfo {
    pub const fn new(minimum: i32, maximum: i32) -> Self {
        Self {
            value: 0,
            minimum,
            maximum,
            fuzz: 0,
            flat: 0,
            resolution: 0,
        }
    }

    pub const fn with_fuzz(mut self, fuzz: i32) -> Self {
        self.fuzz = fuzz;
        self
    }

    pub const fn with_flat(mut self, flat: i32) -> Self {
        self.flat = flat;
        self
    }

    pub const fn with_resolution(mut self, resolution: i32) -> Self {
        self.resolution = resolution;
        self
    }

    /// Normalize value to 0.0-1.0 range
    pub fn normalize(&self) -> f32 {
        if self.maximum == self.minimum {
            return 0.0;
        }
        (self.value - self.minimum) as f32 / (self.maximum - self.minimum) as f32
    }

    /// Normalize value to -1.0 to 1.0 range (for centered axes)
    pub fn normalize_centered(&self) -> f32 {
        if self.maximum == self.minimum {
            return 0.0;
        }
        let center = (self.minimum + self.maximum) / 2;
        let half_range = (self.maximum - self.minimum) / 2;
        if half_range == 0 {
            return 0.0;
        }
        (self.value - center) as f32 / half_range as f32
    }
}

/// Input event with timestamp
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct InputEvent {
    /// Timestamp seconds
    pub time_sec: u64,
    /// Timestamp microseconds
    pub time_usec: u64,
    /// Event type
    pub ev_type: u16,
    /// Event code
    pub code: u16,
    /// Event value
    pub value: i32,
}

impl InputEvent {
    pub const fn new(ev_type: u16, code: u16, value: i32) -> Self {
        Self {
            time_sec: 0,
            time_usec: 0,
            ev_type,
            code,
            value,
        }
    }

    pub fn with_timestamp(mut self, sec: u64, usec: u64) -> Self {
        self.time_sec = sec;
        self.time_usec = usec;
        self
    }

    /// Create a key event
    pub const fn key(code: u16, pressed: bool) -> Self {
        Self::new(EV_KEY, code, if pressed { 1 } else { 0 })
    }

    /// Create a key repeat event
    pub const fn key_repeat(code: u16) -> Self {
        Self::new(EV_KEY, code, 2)
    }

    /// Create a relative axis event
    pub const fn rel(code: u16, value: i32) -> Self {
        Self::new(EV_REL, code, value)
    }

    /// Create an absolute axis event
    pub const fn abs(code: u16, value: i32) -> Self {
        Self::new(EV_ABS, code, value)
    }

    /// Create a synchronization event
    pub const fn syn() -> Self {
        Self::new(EV_SYN, SYN_REPORT, 0)
    }

    /// Create a dropped events marker
    pub const fn syn_dropped() -> Self {
        Self::new(EV_SYN, SYN_DROPPED, 0)
    }

    /// Check if this is a sync event
    pub fn is_syn(&self) -> bool {
        self.ev_type == EV_SYN && self.code == SYN_REPORT
    }

    /// Check if this is a key press
    pub fn is_key_press(&self) -> bool {
        self.ev_type == EV_KEY && self.value == 1
    }

    /// Check if this is a key release
    pub fn is_key_release(&self) -> bool {
        self.ev_type == EV_KEY && self.value == 0
    }
}

/// Input device state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputDeviceState {
    /// Device is registered but not opened
    Registered,
    /// Device is open and active
    Open,
    /// Device is suspended (power management)
    Suspended,
    /// Device has been removed (hot-unplug)
    Removed,
}

/// Input device properties
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum InputProperty {
    /// Needs a pointer
    Pointer = 0x00,
    /// Direct input device (touchscreen)
    Direct = 0x01,
    /// Has button(s) under pad
    ButtonPad = 0x02,
    /// Is a semi-multitouch device (touch emulation)
    SemiMt = 0x03,
    /// Top-most surface supports input
    TopButtonPad = 0x04,
    /// Pointing stick (like TrackPoint)
    PointingStick = 0x05,
    /// Touchpad has accelerometer
    Accelerometer = 0x06,
}

/// Input event handler trait
pub trait InputHandler: Send + Sync {
    /// Get handler name
    fn name(&self) -> &str;

    /// Check if handler can handle this device
    fn match_device(&self, device: &InputDevice) -> bool;

    /// Called when device is connected
    fn connect(&self, device: &InputDevice) -> Result<(), InputError>;

    /// Called when device is disconnected
    fn disconnect(&self, device: &InputDevice);

    /// Handle input event
    fn event(&self, device: &InputDevice, event: &InputEvent);

    /// Called on device open
    fn open(&self, _device: &InputDevice) -> Result<(), InputError> {
        Ok(())
    }

    /// Called on device close
    fn close(&self, _device: &InputDevice) {}
}

/// Input device operations
pub trait InputDeviceOps: Send + Sync {
    /// Open the device
    fn open(&self) -> Result<(), InputError>;

    /// Close the device
    fn close(&self);

    /// Flush pending events
    fn flush(&self);

    /// Handle an event (for device-side processing)
    fn event(&self, event: &InputEvent) -> Result<(), InputError>;

    /// Set LED state
    fn set_led(&self, _led: u16, _value: bool) -> Result<(), InputError> {
        Err(InputError::NotSupported)
    }

    /// Set key repeat parameters
    fn set_repeat(&self, _delay: u32, _period: u32) -> Result<(), InputError> {
        Err(InputError::NotSupported)
    }

    /// Upload force feedback effect
    fn upload_ff(&self, _effect: &ff::FfEffect) -> Result<i16, InputError> {
        Err(InputError::NotSupported)
    }

    /// Erase force feedback effect
    fn erase_ff(&self, _effect_id: i16) -> Result<(), InputError> {
        Err(InputError::NotSupported)
    }

    /// Play force feedback effect
    fn play_ff(&self, _effect_id: i16, _count: u32) -> Result<(), InputError> {
        Err(InputError::NotSupported)
    }

    /// Stop force feedback effect
    fn stop_ff(&self, _effect_id: i16) -> Result<(), InputError> {
        Err(InputError::NotSupported)
    }

    /// Set force feedback gain
    fn set_ff_gain(&self, _gain: u16) -> Result<(), InputError> {
        Err(InputError::NotSupported)
    }
}

/// Null device operations (default)
pub struct NullDeviceOps;

impl InputDeviceOps for NullDeviceOps {
    fn open(&self) -> Result<(), InputError> { Ok(()) }
    fn close(&self) {}
    fn flush(&self) {}
    fn event(&self, _event: &InputEvent) -> Result<(), InputError> { Ok(()) }
}

/// Input device structure
pub struct InputDevice {
    /// Device ID (minor number)
    id: u32,
    /// Device name
    name: String,
    /// Physical path
    phys: String,
    /// Unique identifier
    uniq: String,
    /// Device ID info
    device_id: InputId,
    /// Device capabilities
    capabilities: RwLock<InputCapabilities>,
    /// Absolute axis info
    abs_info: RwLock<BTreeMap<u16, AbsInfo>>,
    /// Device properties
    properties: AtomicU32,
    /// Current state
    state: RwLock<InputDeviceState>,
    /// Key states bitmap
    key_state: RwLock<[u64; 12]>,
    /// LED states bitmap
    led_state: RwLock<[u64; 1]>,
    /// Switch states bitmap
    sw_state: RwLock<[u64; 1]>,
    /// Event queue
    event_queue: Mutex<EventQueue>,
    /// Device operations
    ops: RwLock<Option<Box<dyn InputDeviceOps>>>,
    /// Reference count
    ref_count: AtomicU32,
    /// Grab handle (exclusive access)
    grabbed: AtomicBool,
    /// Repeat parameters
    rep_delay: AtomicU32,
    rep_period: AtomicU32,
    /// Statistics
    event_count: AtomicU64,
    /// User data
    user_data: RwLock<Option<Box<dyn core::any::Any + Send + Sync>>>,
}

/// Event queue for buffering input events
struct EventQueue {
    buffer: [InputEvent; INPUT_EVENT_QUEUE_SIZE],
    head: usize,
    tail: usize,
    overflow: bool,
}

impl EventQueue {
    const fn new() -> Self {
        Self {
            buffer: [InputEvent::new(0, 0, 0); INPUT_EVENT_QUEUE_SIZE],
            head: 0,
            tail: 0,
            overflow: false,
        }
    }

    fn push(&mut self, event: InputEvent) -> bool {
        let next_head = (self.head + 1) % INPUT_EVENT_QUEUE_SIZE;
        if next_head == self.tail {
            // Queue full
            self.overflow = true;
            return false;
        }
        self.buffer[self.head] = event;
        self.head = next_head;
        true
    }

    fn pop(&mut self) -> Option<InputEvent> {
        if self.tail == self.head {
            return None;
        }
        let event = self.buffer[self.tail];
        self.tail = (self.tail + 1) % INPUT_EVENT_QUEUE_SIZE;
        Some(event)
    }

    fn is_empty(&self) -> bool {
        self.tail == self.head
    }

    fn len(&self) -> usize {
        if self.head >= self.tail {
            self.head - self.tail
        } else {
            INPUT_EVENT_QUEUE_SIZE - self.tail + self.head
        }
    }

    fn clear(&mut self) {
        self.head = 0;
        self.tail = 0;
        self.overflow = false;
    }

    fn had_overflow(&mut self) -> bool {
        let overflow = self.overflow;
        self.overflow = false;
        overflow
    }
}

impl InputDevice {
    /// Create a new input device
    pub fn new(name: &str, id: InputId) -> Self {
        static NEXT_ID: AtomicU32 = AtomicU32::new(0);

        Self {
            id: NEXT_ID.fetch_add(1, Ordering::SeqCst),
            name: String::from(name),
            phys: String::new(),
            uniq: String::new(),
            device_id: id,
            capabilities: RwLock::new(InputCapabilities::new()),
            abs_info: RwLock::new(BTreeMap::new()),
            properties: AtomicU32::new(0),
            state: RwLock::new(InputDeviceState::Registered),
            key_state: RwLock::new([0; 12]),
            led_state: RwLock::new([0; 1]),
            sw_state: RwLock::new([0; 1]),
            event_queue: Mutex::new(EventQueue::new()),
            ops: RwLock::new(None),
            ref_count: AtomicU32::new(1),
            grabbed: AtomicBool::new(false),
            rep_delay: AtomicU32::new(250),  // Default 250ms delay
            rep_period: AtomicU32::new(33),  // Default ~30 Hz repeat
            event_count: AtomicU64::new(0),
            user_data: RwLock::new(None),
        }
    }

    /// Create a keyboard device
    pub fn keyboard(name: &str, id: InputId) -> Self {
        let dev = Self::new(name, id);
        *dev.capabilities.write() = InputCapabilities::keyboard();
        dev
    }

    /// Create a mouse device
    pub fn mouse(name: &str, id: InputId) -> Self {
        let dev = Self::new(name, id);
        *dev.capabilities.write() = InputCapabilities::mouse();
        dev
    }

    /// Create a touchscreen device
    pub fn touchscreen(name: &str, id: InputId, width: i32, height: i32) -> Self {
        let dev = Self::new(name, id);
        *dev.capabilities.write() = InputCapabilities::touchscreen();

        {
            let mut abs = dev.abs_info.write();
            abs.insert(ABS_X, AbsInfo::new(0, width));
            abs.insert(ABS_Y, AbsInfo::new(0, height));
            abs.insert(ABS_PRESSURE, AbsInfo::new(0, 255));
            abs.insert(ABS_MT_SLOT, AbsInfo::new(0, 9));
            abs.insert(ABS_MT_POSITION_X, AbsInfo::new(0, width));
            abs.insert(ABS_MT_POSITION_Y, AbsInfo::new(0, height));
            abs.insert(ABS_MT_TRACKING_ID, AbsInfo::new(0, 65535));
            abs.insert(ABS_MT_PRESSURE, AbsInfo::new(0, 255));
        }

        dev
    }

    /// Create a gamepad device
    pub fn gamepad(name: &str, id: InputId) -> Self {
        let dev = Self::new(name, id);
        *dev.capabilities.write() = InputCapabilities::gamepad();

        {
            let mut abs = dev.abs_info.write();
            // Left stick
            abs.insert(ABS_X, AbsInfo::new(-32768, 32767).with_flat(4096));
            abs.insert(ABS_Y, AbsInfo::new(-32768, 32767).with_flat(4096));
            // Right stick
            abs.insert(ABS_RX, AbsInfo::new(-32768, 32767).with_flat(4096));
            abs.insert(ABS_RY, AbsInfo::new(-32768, 32767).with_flat(4096));
            // Triggers
            abs.insert(ABS_Z, AbsInfo::new(0, 255));
            abs.insert(ABS_RZ, AbsInfo::new(0, 255));
            // D-pad
            abs.insert(ABS_HAT0X, AbsInfo::new(-1, 1));
            abs.insert(ABS_HAT0Y, AbsInfo::new(-1, 1));
        }

        dev
    }

    /// Get device ID
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Get device name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get physical path
    pub fn phys(&self) -> &str {
        &self.phys
    }

    /// Set physical path
    pub fn set_phys(&mut self, phys: &str) {
        self.phys = String::from(phys);
    }

    /// Get unique identifier
    pub fn uniq(&self) -> &str {
        &self.uniq
    }

    /// Set unique identifier
    pub fn set_uniq(&mut self, uniq: &str) {
        self.uniq = String::from(uniq);
    }

    /// Get device ID info
    pub fn device_id(&self) -> InputId {
        self.device_id
    }

    /// Get current state
    pub fn state(&self) -> InputDeviceState {
        *self.state.read()
    }

    /// Set device operations
    pub fn set_ops(&self, ops: Box<dyn InputDeviceOps>) {
        *self.ops.write() = Some(ops);
    }

    /// Set capability bit
    pub fn set_capability(&self, ev_type: u16, code: u16) {
        let mut caps = self.capabilities.write();
        caps.set_evbit(ev_type);
        match ev_type {
            EV_KEY => caps.set_keybit(code),
            EV_REL => caps.set_relbit(code),
            EV_ABS => caps.set_absbit(code),
            EV_LED => caps.set_ledbit(code),
            EV_FF => caps.set_ffbit(code),
            EV_SW => caps.set_swbit(code),
            _ => {}
        }
    }

    /// Set absolute axis info
    pub fn set_abs_info(&self, code: u16, info: AbsInfo) {
        self.capabilities.write().set_absbit(code);
        self.abs_info.write().insert(code, info);
    }

    /// Get absolute axis info
    pub fn get_abs_info(&self, code: u16) -> Option<AbsInfo> {
        self.abs_info.read().get(&code).copied()
    }

    /// Set property
    pub fn set_property(&self, prop: InputProperty) {
        self.properties.fetch_or(1 << (prop as u32), Ordering::SeqCst);
    }

    /// Check property
    pub fn has_property(&self, prop: InputProperty) -> bool {
        (self.properties.load(Ordering::SeqCst) & (1 << (prop as u32))) != 0
    }

    /// Check if key is pressed
    pub fn is_key_pressed(&self, code: u16) -> bool {
        let idx = (code / 64) as usize;
        let bit = code % 64;
        let state = self.key_state.read();
        if idx < state.len() {
            (state[idx] & (1u64 << bit)) != 0
        } else {
            false
        }
    }

    /// Set key state
    fn set_key_state(&self, code: u16, pressed: bool) {
        let idx = (code / 64) as usize;
        let bit = code % 64;
        let mut state = self.key_state.write();
        if idx < state.len() {
            if pressed {
                state[idx] |= 1u64 << bit;
            } else {
                state[idx] &= !(1u64 << bit);
            }
        }
    }

    /// Get LED state
    pub fn is_led_on(&self, led: u16) -> bool {
        let idx = (led / 64) as usize;
        let bit = led % 64;
        let state = self.led_state.read();
        if idx < state.len() {
            (state[idx] & (1u64 << bit)) != 0
        } else {
            false
        }
    }

    /// Set LED state
    pub fn set_led(&self, led: u16, on: bool) -> Result<(), InputError> {
        let idx = (led / 64) as usize;
        let bit = led % 64;

        {
            let mut state = self.led_state.write();
            if idx < state.len() {
                if on {
                    state[idx] |= 1u64 << bit;
                } else {
                    state[idx] &= !(1u64 << bit);
                }
            }
        }

        if let Some(ref ops) = *self.ops.read() {
            ops.set_led(led, on)?;
        }

        Ok(())
    }

    /// Report an input event
    pub fn report_event(&self, ev_type: u16, code: u16, value: i32) {
        // Update internal state
        match ev_type {
            EV_KEY => {
                self.set_key_state(code, value != 0);
            }
            EV_ABS => {
                if let Some(info) = self.abs_info.write().get_mut(&code) {
                    info.value = value;
                }
            }
            _ => {}
        }

        // Create event
        let event = InputEvent::new(ev_type, code, value);

        // Queue event
        let mut queue = self.event_queue.lock();
        if !queue.push(event) {
            // Queue overflow - insert SYN_DROPPED
            queue.clear();
            queue.push(InputEvent::syn_dropped());
        }

        self.event_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Report a key event
    pub fn report_key(&self, code: u16, pressed: bool) {
        self.report_event(EV_KEY, code, if pressed { 1 } else { 0 });
    }

    /// Report relative movement
    pub fn report_rel(&self, code: u16, value: i32) {
        if value != 0 {
            self.report_event(EV_REL, code, value);
        }
    }

    /// Report absolute position
    pub fn report_abs(&self, code: u16, value: i32) {
        self.report_event(EV_ABS, code, value);
    }

    /// Report synchronization
    pub fn sync(&self) {
        self.report_event(EV_SYN, SYN_REPORT, 0);
    }

    /// Read pending events
    pub fn read_events(&self, buffer: &mut [InputEvent]) -> usize {
        let mut queue = self.event_queue.lock();
        let mut count = 0;

        // Check for overflow
        if queue.had_overflow() {
            if !buffer.is_empty() {
                buffer[0] = InputEvent::syn_dropped();
                count = 1;
            }
        }

        // Read events
        while count < buffer.len() {
            if let Some(event) = queue.pop() {
                buffer[count] = event;
                count += 1;
            } else {
                break;
            }
        }

        count
    }

    /// Check if events are pending
    pub fn has_events(&self) -> bool {
        !self.event_queue.lock().is_empty()
    }

    /// Get pending event count
    pub fn pending_events(&self) -> usize {
        self.event_queue.lock().len()
    }

    /// Flush event queue
    pub fn flush(&self) {
        self.event_queue.lock().clear();
        if let Some(ref ops) = *self.ops.read() {
            ops.flush();
        }
    }

    /// Grab exclusive access
    pub fn grab(&self) -> Result<(), InputError> {
        if self.grabbed.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            Ok(())
        } else {
            Err(InputError::DeviceBusy)
        }
    }

    /// Release exclusive access
    pub fn ungrab(&self) {
        self.grabbed.store(false, Ordering::SeqCst);
    }

    /// Check if device is grabbed
    pub fn is_grabbed(&self) -> bool {
        self.grabbed.load(Ordering::SeqCst)
    }

    /// Get event count
    pub fn event_count(&self) -> u64 {
        self.event_count.load(Ordering::Relaxed)
    }

    /// Set user data
    pub fn set_user_data<T: Send + Sync + 'static>(&self, data: T) {
        *self.user_data.write() = Some(Box::new(data));
    }

    /// Get key repeat parameters
    pub fn get_repeat(&self) -> (u32, u32) {
        (
            self.rep_delay.load(Ordering::Relaxed),
            self.rep_period.load(Ordering::Relaxed),
        )
    }

    /// Set key repeat parameters
    pub fn set_repeat(&self, delay: u32, period: u32) -> Result<(), InputError> {
        self.rep_delay.store(delay, Ordering::Relaxed);
        self.rep_period.store(period, Ordering::Relaxed);

        if let Some(ref ops) = *self.ops.read() {
            ops.set_repeat(delay, period)?;
        }

        Ok(())
    }
}

/// Input subsystem error type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputError {
    /// Device not found
    DeviceNotFound,
    /// Device is busy
    DeviceBusy,
    /// Invalid argument
    InvalidArgument,
    /// Operation not supported
    NotSupported,
    /// No memory available
    NoMemory,
    /// Permission denied
    PermissionDenied,
    /// Device disconnected
    Disconnected,
    /// Queue overflow
    Overflow,
    /// Timeout
    Timeout,
    /// Internal error
    Internal,
}

/// Global input subsystem manager
pub struct InputManager {
    /// Registered devices
    devices: RwLock<BTreeMap<u32, Arc<InputDevice>>>,
    /// Registered handlers
    handlers: RwLock<Vec<Arc<dyn InputHandler>>>,
    /// Device count
    device_count: AtomicU32,
    /// Initialized flag
    initialized: AtomicBool,
}

impl InputManager {
    /// Create new input manager
    pub const fn new() -> Self {
        Self {
            devices: RwLock::new(BTreeMap::new()),
            handlers: RwLock::new(Vec::new()),
            device_count: AtomicU32::new(0),
            initialized: AtomicBool::new(false),
        }
    }

    /// Initialize the input manager
    pub fn init(&self) {
        if self.initialized.swap(true, Ordering::SeqCst) {
            return;
        }

        // Register built-in handlers
        // (keyboard, mouse handlers are registered by their respective modules)
    }

    /// Register an input device
    pub fn register_device(&self, device: Arc<InputDevice>) -> Result<(), InputError> {
        let id = device.id();

        // Add to device list
        self.devices.write().insert(id, device.clone());
        self.device_count.fetch_add(1, Ordering::SeqCst);

        // Notify matching handlers
        let handlers = self.handlers.read();
        for handler in handlers.iter() {
            if handler.match_device(&device) {
                let _ = handler.connect(&device);
            }
        }

        Ok(())
    }

    /// Unregister an input device
    pub fn unregister_device(&self, device: &Arc<InputDevice>) {
        let id = device.id();

        // Update state
        *device.state.write() = InputDeviceState::Removed;

        // Notify handlers
        let handlers = self.handlers.read();
        for handler in handlers.iter() {
            if handler.match_device(device) {
                handler.disconnect(device);
            }
        }

        // Remove from device list
        self.devices.write().remove(&id);
        self.device_count.fetch_sub(1, Ordering::SeqCst);
    }

    /// Get device by ID
    pub fn get_device(&self, id: u32) -> Option<Arc<InputDevice>> {
        self.devices.read().get(&id).cloned()
    }

    /// Get all devices
    pub fn get_devices(&self) -> Vec<Arc<InputDevice>> {
        self.devices.read().values().cloned().collect()
    }

    /// Get device count
    pub fn device_count(&self) -> u32 {
        self.device_count.load(Ordering::SeqCst)
    }

    /// Register an input handler
    pub fn register_handler(&self, handler: Arc<dyn InputHandler>) {
        // Connect to existing matching devices
        let devices = self.devices.read();
        for device in devices.values() {
            if handler.match_device(device) {
                let _ = handler.connect(device);
            }
        }

        self.handlers.write().push(handler);
    }

    /// Unregister an input handler
    pub fn unregister_handler(&self, name: &str) {
        let mut handlers = self.handlers.write();
        handlers.retain(|h| h.name() != name);
    }

    /// Dispatch event to handlers
    pub fn dispatch_event(&self, device: &InputDevice, event: &InputEvent) {
        let handlers = self.handlers.read();
        for handler in handlers.iter() {
            if handler.match_device(device) {
                handler.event(device, event);
            }
        }
    }
}

/// Global input manager instance
static INPUT_MANAGER: InputManager = InputManager::new();

/// Get the global input manager
pub fn input_manager() -> &'static InputManager {
    &INPUT_MANAGER
}

/// Initialize the input subsystem
pub fn init() {
    INPUT_MANAGER.init();

    // Initialize sub-modules
    keyboard::init();
    mouse::init();
    touch::init();
    gamepad::init();
    ff::init();
}

/// Input device event listener for async notification
pub trait InputEventListener: Send + Sync {
    /// Called when events are available
    fn on_events_available(&self, device: &InputDevice);
}

/// Input poll result
pub struct InputPollResult {
    /// Device with events
    pub device: Arc<InputDevice>,
    /// Number of events available
    pub event_count: usize,
}

/// Poll for input events across all devices
pub fn poll_events() -> Vec<InputPollResult> {
    let mut results = Vec::new();

    for device in INPUT_MANAGER.get_devices() {
        let count = device.pending_events();
        if count > 0 {
            results.push(InputPollResult {
                device,
                event_count: count,
            });
        }
    }

    results
}

/// Multi-touch slot state
#[derive(Debug, Clone, Copy, Default)]
pub struct MtSlot {
    /// Tracking ID (-1 = unused)
    pub tracking_id: i32,
    /// X position
    pub x: i32,
    /// Y position
    pub y: i32,
    /// Touch pressure
    pub pressure: i32,
    /// Touch major axis
    pub touch_major: i32,
    /// Touch minor axis
    pub touch_minor: i32,
    /// Orientation
    pub orientation: i32,
}

impl MtSlot {
    pub const fn new() -> Self {
        Self {
            tracking_id: -1,
            x: 0,
            y: 0,
            pressure: 0,
            touch_major: 0,
            touch_minor: 0,
            orientation: 0,
        }
    }

    pub fn is_active(&self) -> bool {
        self.tracking_id >= 0
    }
}

/// Multi-touch device state manager
pub struct MtState {
    /// Current slot index
    current_slot: usize,
    /// Slot states
    slots: [MtSlot; 10],
    /// Number of active touches
    touch_count: usize,
}

impl MtState {
    pub const fn new() -> Self {
        Self {
            current_slot: 0,
            slots: [MtSlot::new(); 10],
            touch_count: 0,
        }
    }

    /// Process MT event
    pub fn process_event(&mut self, code: u16, value: i32) {
        match code {
            ABS_MT_SLOT => {
                if (value as usize) < self.slots.len() {
                    self.current_slot = value as usize;
                }
            }
            ABS_MT_TRACKING_ID => {
                let slot = &mut self.slots[self.current_slot];
                let was_active = slot.is_active();
                slot.tracking_id = value;
                let is_active = slot.is_active();

                if was_active && !is_active {
                    self.touch_count = self.touch_count.saturating_sub(1);
                } else if !was_active && is_active {
                    self.touch_count += 1;
                }
            }
            ABS_MT_POSITION_X => {
                self.slots[self.current_slot].x = value;
            }
            ABS_MT_POSITION_Y => {
                self.slots[self.current_slot].y = value;
            }
            ABS_MT_PRESSURE => {
                self.slots[self.current_slot].pressure = value;
            }
            ABS_MT_TOUCH_MAJOR => {
                self.slots[self.current_slot].touch_major = value;
            }
            ABS_MT_TOUCH_MINOR => {
                self.slots[self.current_slot].touch_minor = value;
            }
            ABS_MT_ORIENTATION => {
                self.slots[self.current_slot].orientation = value;
            }
            _ => {}
        }
    }

    /// Get current slot
    pub fn current_slot(&self) -> &MtSlot {
        &self.slots[self.current_slot]
    }

    /// Get slot by index
    pub fn slot(&self, index: usize) -> Option<&MtSlot> {
        self.slots.get(index)
    }

    /// Get all slots
    pub fn slots(&self) -> &[MtSlot] {
        &self.slots
    }

    /// Get active touch count
    pub fn touch_count(&self) -> usize {
        self.touch_count
    }

    /// Get active touches
    pub fn active_touches(&self) -> impl Iterator<Item = (usize, &MtSlot)> {
        self.slots.iter().enumerate().filter(|(_, slot)| slot.is_active())
    }

    /// Reset all slots
    pub fn reset(&mut self) {
        for slot in &mut self.slots {
            *slot = MtSlot::new();
        }
        self.current_slot = 0;
        self.touch_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_device_creation() {
        let dev = InputDevice::new("Test Device", InputId::virtual_device());
        assert_eq!(dev.name(), "Test Device");
        assert_eq!(dev.state(), InputDeviceState::Registered);
    }

    #[test]
    fn test_keyboard_device() {
        let dev = InputDevice::keyboard("Test Keyboard", InputId::usb(0x1234, 0x5678, 1));
        let caps = dev.capabilities.read();
        assert!(caps.has_evbit(EV_KEY));
        assert!(caps.has_evbit(EV_REP));
        assert!(caps.has_evbit(EV_LED));
    }

    #[test]
    fn test_mouse_device() {
        let dev = InputDevice::mouse("Test Mouse", InputId::usb(0x1234, 0x5678, 1));
        let caps = dev.capabilities.read();
        assert!(caps.has_evbit(EV_KEY));
        assert!(caps.has_evbit(EV_REL));
        assert!(caps.has_keybit(BTN_LEFT));
    }

    #[test]
    fn test_event_reporting() {
        let dev = InputDevice::keyboard("Test Keyboard", InputId::virtual_device());

        // Report key press
        dev.report_key(KEY_A, true);
        dev.sync();

        assert!(dev.is_key_pressed(KEY_A));
        assert!(dev.has_events());

        let mut events = [InputEvent::new(0, 0, 0); 10];
        let count = dev.read_events(&mut events);
        assert_eq!(count, 2); // KEY event + SYN event
    }

    #[test]
    fn test_abs_info() {
        let info = AbsInfo::new(0, 1000).with_fuzz(4).with_flat(8);
        assert_eq!(info.minimum, 0);
        assert_eq!(info.maximum, 1000);
        assert_eq!(info.fuzz, 4);
        assert_eq!(info.flat, 8);
    }

    #[test]
    fn test_mt_state() {
        let mut mt = MtState::new();
        assert_eq!(mt.touch_count(), 0);

        // Simulate touch down on slot 0
        mt.process_event(ABS_MT_SLOT, 0);
        mt.process_event(ABS_MT_TRACKING_ID, 1);
        mt.process_event(ABS_MT_POSITION_X, 100);
        mt.process_event(ABS_MT_POSITION_Y, 200);

        assert_eq!(mt.touch_count(), 1);
        assert!(mt.slot(0).unwrap().is_active());
        assert_eq!(mt.slot(0).unwrap().x, 100);
        assert_eq!(mt.slot(0).unwrap().y, 200);

        // Touch up
        mt.process_event(ABS_MT_TRACKING_ID, -1);
        assert_eq!(mt.touch_count(), 0);
        assert!(!mt.slot(0).unwrap().is_active());
    }
}
