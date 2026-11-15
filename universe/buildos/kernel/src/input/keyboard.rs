//! Keyboard Input Driver
//!
//! This module implements comprehensive keyboard support including:
//!
//! - PS/2 keyboard driver
//! - USB HID keyboard support
//! - Scancode to keycode translation
//! - Key repeat handling
//! - LED control (Caps Lock, Num Lock, Scroll Lock)
//! - Keyboard layout support
//! - Modifier key tracking

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
use spin::{RwLock, Mutex};

use super::events::*;
use super::{InputDevice, InputId, InputHandler, InputError, InputEvent, input_manager, BUS_I8042};

/// PS/2 keyboard I/O ports
const PS2_DATA_PORT: u16 = 0x60;
const PS2_STATUS_PORT: u16 = 0x64;
const PS2_COMMAND_PORT: u16 = 0x64;

/// PS/2 keyboard commands
const PS2_CMD_SET_LEDS: u8 = 0xED;
const PS2_CMD_ECHO: u8 = 0xEE;
const PS2_CMD_SCANCODE_SET: u8 = 0xF0;
const PS2_CMD_IDENTIFY: u8 = 0xF2;
const PS2_CMD_SET_RATE: u8 = 0xF3;
const PS2_CMD_ENABLE: u8 = 0xF4;
const PS2_CMD_DISABLE: u8 = 0xF5;
const PS2_CMD_SET_DEFAULT: u8 = 0xF6;
const PS2_CMD_RESEND: u8 = 0xFE;
const PS2_CMD_RESET: u8 = 0xFF;

/// PS/2 keyboard responses
const PS2_ACK: u8 = 0xFA;
const PS2_RESEND: u8 = 0xFE;
const PS2_ECHO: u8 = 0xEE;
const PS2_BAT_SUCCESS: u8 = 0xAA;
const PS2_BAT_FAILURE: u8 = 0xFC;

/// Keyboard modifier flags
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyboardModifiers {
    /// Left Shift pressed
    pub left_shift: bool,
    /// Right Shift pressed
    pub right_shift: bool,
    /// Left Control pressed
    pub left_ctrl: bool,
    /// Right Control pressed
    pub right_ctrl: bool,
    /// Left Alt pressed
    pub left_alt: bool,
    /// Right Alt (AltGr) pressed
    pub right_alt: bool,
    /// Left Meta (Windows/Super) pressed
    pub left_meta: bool,
    /// Right Meta pressed
    pub right_meta: bool,
    /// Caps Lock active
    pub caps_lock: bool,
    /// Num Lock active
    pub num_lock: bool,
    /// Scroll Lock active
    pub scroll_lock: bool,
}

impl KeyboardModifiers {
    pub const fn new() -> Self {
        Self {
            left_shift: false,
            right_shift: false,
            left_ctrl: false,
            right_ctrl: false,
            left_alt: false,
            right_alt: false,
            left_meta: false,
            right_meta: false,
            caps_lock: false,
            num_lock: false,
            scroll_lock: false,
        }
    }

    /// Check if any shift is pressed
    pub fn shift(&self) -> bool {
        self.left_shift || self.right_shift
    }

    /// Check if any control is pressed
    pub fn ctrl(&self) -> bool {
        self.left_ctrl || self.right_ctrl
    }

    /// Check if any alt is pressed
    pub fn alt(&self) -> bool {
        self.left_alt || self.right_alt
    }

    /// Check if any meta is pressed
    pub fn meta(&self) -> bool {
        self.left_meta || self.right_meta
    }

    /// Check if AltGr is pressed (Right Alt)
    pub fn altgr(&self) -> bool {
        self.right_alt
    }

    /// Update modifier state based on key event
    pub fn update(&mut self, key: u16, pressed: bool) {
        match key {
            KEY_LEFTSHIFT => self.left_shift = pressed,
            KEY_RIGHTSHIFT => self.right_shift = pressed,
            KEY_LEFTCTRL => self.left_ctrl = pressed,
            KEY_RIGHTCTRL => self.right_ctrl = pressed,
            KEY_LEFTALT => self.left_alt = pressed,
            KEY_RIGHTALT => self.right_alt = pressed,
            KEY_LEFTMETA => self.left_meta = pressed,
            KEY_RIGHTMETA => self.right_meta = pressed,
            KEY_CAPSLOCK if pressed => self.caps_lock = !self.caps_lock,
            KEY_NUMLOCK if pressed => self.num_lock = !self.num_lock,
            KEY_SCROLLLOCK if pressed => self.scroll_lock = !self.scroll_lock,
            _ => {}
        }
    }

    /// Get LED state byte for PS/2 keyboard
    pub fn led_state(&self) -> u8 {
        let mut state = 0u8;
        if self.scroll_lock { state |= 0x01; }
        if self.num_lock { state |= 0x02; }
        if self.caps_lock { state |= 0x04; }
        state
    }

    /// Convert to USB HID modifier byte
    pub fn to_hid_modifiers(&self) -> u8 {
        let mut mods = 0u8;
        if self.left_ctrl { mods |= 0x01; }
        if self.left_shift { mods |= 0x02; }
        if self.left_alt { mods |= 0x04; }
        if self.left_meta { mods |= 0x08; }
        if self.right_ctrl { mods |= 0x10; }
        if self.right_shift { mods |= 0x20; }
        if self.right_alt { mods |= 0x40; }
        if self.right_meta { mods |= 0x80; }
        mods
    }
}

/// Keyboard layout
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardLayout {
    /// US English (QWERTY)
    UsQwerty,
    /// UK English
    UkQwerty,
    /// German (QWERTZ)
    DeQwertz,
    /// French (AZERTY)
    FrAzerty,
    /// Spanish
    EsQwerty,
    /// Dvorak
    Dvorak,
    /// Colemak
    Colemak,
}

impl Default for KeyboardLayout {
    fn default() -> Self {
        Self::UsQwerty
    }
}

/// Character mapping for a key
#[derive(Debug, Clone, Copy)]
pub struct KeyMapping {
    /// Normal (unshifted) character
    pub normal: char,
    /// Shifted character
    pub shifted: char,
    /// AltGr character (optional)
    pub altgr: Option<char>,
    /// Shift+AltGr character (optional)
    pub shift_altgr: Option<char>,
}

impl KeyMapping {
    pub const fn new(normal: char, shifted: char) -> Self {
        Self {
            normal,
            shifted,
            altgr: None,
            shift_altgr: None,
        }
    }

    pub const fn with_altgr(mut self, altgr: char) -> Self {
        self.altgr = Some(altgr);
        self
    }

    pub const fn with_shift_altgr(mut self, shift_altgr: char) -> Self {
        self.shift_altgr = Some(shift_altgr);
        self
    }

    /// Get character based on modifier state
    pub fn get_char(&self, mods: &KeyboardModifiers) -> Option<char> {
        if mods.altgr() {
            if mods.shift() {
                self.shift_altgr.or(self.altgr)
            } else {
                self.altgr
            }
        } else if mods.shift() ^ mods.caps_lock {
            Some(self.shifted)
        } else {
            Some(self.normal)
        }
    }
}

/// Key repeat state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RepeatState {
    /// No key is being held
    Idle,
    /// Waiting for initial delay
    Delay,
    /// Actively repeating
    Repeating,
}

/// Key repeat handler
pub struct KeyRepeatHandler {
    /// Currently held key
    current_key: AtomicU32,
    /// Repeat state
    state: AtomicU8,
    /// Initial delay in milliseconds
    delay_ms: AtomicU32,
    /// Repeat period in milliseconds
    period_ms: AtomicU32,
    /// Last event timestamp
    last_time: AtomicU32,
    /// Enabled flag
    enabled: AtomicBool,
}

impl KeyRepeatHandler {
    pub const fn new() -> Self {
        Self {
            current_key: AtomicU32::new(0),
            state: AtomicU8::new(RepeatState::Idle as u8),
            delay_ms: AtomicU32::new(500),
            period_ms: AtomicU32::new(50),
            last_time: AtomicU32::new(0),
            enabled: AtomicBool::new(true),
        }
    }

    /// Set repeat parameters
    pub fn set_params(&self, delay_ms: u32, period_ms: u32) {
        self.delay_ms.store(delay_ms, Ordering::Relaxed);
        self.period_ms.store(period_ms, Ordering::Relaxed);
    }

    /// Get repeat parameters
    pub fn get_params(&self) -> (u32, u32) {
        (
            self.delay_ms.load(Ordering::Relaxed),
            self.period_ms.load(Ordering::Relaxed),
        )
    }

    /// Enable/disable repeat
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    /// Called when a key is pressed
    pub fn key_down(&self, key: u16, timestamp: u32) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }

        self.current_key.store(key as u32, Ordering::Relaxed);
        self.state.store(RepeatState::Delay as u8, Ordering::Relaxed);
        self.last_time.store(timestamp, Ordering::Relaxed);
    }

    /// Called when a key is released
    pub fn key_up(&self, key: u16) {
        if self.current_key.load(Ordering::Relaxed) == key as u32 {
            self.state.store(RepeatState::Idle as u8, Ordering::Relaxed);
            self.current_key.store(0, Ordering::Relaxed);
        }
    }

    /// Check if a repeat event should be generated
    pub fn check_repeat(&self, current_time: u32) -> Option<u16> {
        if !self.enabled.load(Ordering::Relaxed) {
            return None;
        }

        let state = self.state.load(Ordering::Relaxed);
        let last_time = self.last_time.load(Ordering::Relaxed);
        let key = self.current_key.load(Ordering::Relaxed);

        if key == 0 {
            return None;
        }

        let elapsed = current_time.wrapping_sub(last_time);

        match state {
            s if s == RepeatState::Delay as u8 => {
                if elapsed >= self.delay_ms.load(Ordering::Relaxed) {
                    self.state.store(RepeatState::Repeating as u8, Ordering::Relaxed);
                    self.last_time.store(current_time, Ordering::Relaxed);
                    Some(key as u16)
                } else {
                    None
                }
            }
            s if s == RepeatState::Repeating as u8 => {
                if elapsed >= self.period_ms.load(Ordering::Relaxed) {
                    self.last_time.store(current_time, Ordering::Relaxed);
                    Some(key as u16)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// Scancode set 1 to keycode translation table
static SCANCODE_SET1: [u16; 128] = [
    KEY_RESERVED,     // 0x00
    KEY_ESC,          // 0x01
    KEY_1,            // 0x02
    KEY_2,            // 0x03
    KEY_3,            // 0x04
    KEY_4,            // 0x05
    KEY_5,            // 0x06
    KEY_6,            // 0x07
    KEY_7,            // 0x08
    KEY_8,            // 0x09
    KEY_9,            // 0x0A
    KEY_0,            // 0x0B
    KEY_MINUS,        // 0x0C
    KEY_EQUAL,        // 0x0D
    KEY_BACKSPACE,    // 0x0E
    KEY_TAB,          // 0x0F
    KEY_Q,            // 0x10
    KEY_W,            // 0x11
    KEY_E,            // 0x12
    KEY_R,            // 0x13
    KEY_T,            // 0x14
    KEY_Y,            // 0x15
    KEY_U,            // 0x16
    KEY_I,            // 0x17
    KEY_O,            // 0x18
    KEY_P,            // 0x19
    KEY_LEFTBRACE,    // 0x1A
    KEY_RIGHTBRACE,   // 0x1B
    KEY_ENTER,        // 0x1C
    KEY_LEFTCTRL,     // 0x1D
    KEY_A,            // 0x1E
    KEY_S,            // 0x1F
    KEY_D,            // 0x20
    KEY_F,            // 0x21
    KEY_G,            // 0x22
    KEY_H,            // 0x23
    KEY_J,            // 0x24
    KEY_K,            // 0x25
    KEY_L,            // 0x26
    KEY_SEMICOLON,    // 0x27
    KEY_APOSTROPHE,   // 0x28
    KEY_GRAVE,        // 0x29
    KEY_LEFTSHIFT,    // 0x2A
    KEY_BACKSLASH,    // 0x2B
    KEY_Z,            // 0x2C
    KEY_X,            // 0x2D
    KEY_C,            // 0x2E
    KEY_V,            // 0x2F
    KEY_B,            // 0x30
    KEY_N,            // 0x31
    KEY_M,            // 0x32
    KEY_COMMA,        // 0x33
    KEY_DOT,          // 0x34
    KEY_SLASH,        // 0x35
    KEY_RIGHTSHIFT,   // 0x36
    KEY_KPASTERISK,   // 0x37
    KEY_LEFTALT,      // 0x38
    KEY_SPACE,        // 0x39
    KEY_CAPSLOCK,     // 0x3A
    KEY_F1,           // 0x3B
    KEY_F2,           // 0x3C
    KEY_F3,           // 0x3D
    KEY_F4,           // 0x3E
    KEY_F5,           // 0x3F
    KEY_F6,           // 0x40
    KEY_F7,           // 0x41
    KEY_F8,           // 0x42
    KEY_F9,           // 0x43
    KEY_F10,          // 0x44
    KEY_NUMLOCK,      // 0x45
    KEY_SCROLLLOCK,   // 0x46
    KEY_KP7,          // 0x47
    KEY_KP8,          // 0x48
    KEY_KP9,          // 0x49
    KEY_KPMINUS,      // 0x4A
    KEY_KP4,          // 0x4B
    KEY_KP5,          // 0x4C
    KEY_KP6,          // 0x4D
    KEY_KPPLUS,       // 0x4E
    KEY_KP1,          // 0x4F
    KEY_KP2,          // 0x50
    KEY_KP3,          // 0x51
    KEY_KP0,          // 0x52
    KEY_KPDOT,        // 0x53
    KEY_RESERVED,     // 0x54
    KEY_RESERVED,     // 0x55
    KEY_102ND,        // 0x56
    KEY_F11,          // 0x57
    KEY_F12,          // 0x58
    KEY_RESERVED,     // 0x59
    KEY_RESERVED,     // 0x5A
    KEY_RESERVED,     // 0x5B
    KEY_RESERVED,     // 0x5C
    KEY_RESERVED,     // 0x5D
    KEY_RESERVED,     // 0x5E
    KEY_RESERVED,     // 0x5F
    KEY_RESERVED,     // 0x60
    KEY_RESERVED,     // 0x61
    KEY_RESERVED,     // 0x62
    KEY_RESERVED,     // 0x63
    KEY_RESERVED,     // 0x64
    KEY_RESERVED,     // 0x65
    KEY_RESERVED,     // 0x66
    KEY_RESERVED,     // 0x67
    KEY_RESERVED,     // 0x68
    KEY_RESERVED,     // 0x69
    KEY_RESERVED,     // 0x6A
    KEY_RESERVED,     // 0x6B
    KEY_RESERVED,     // 0x6C
    KEY_RESERVED,     // 0x6D
    KEY_RESERVED,     // 0x6E
    KEY_RESERVED,     // 0x6F
    KEY_RESERVED,     // 0x70
    KEY_RESERVED,     // 0x71
    KEY_RESERVED,     // 0x72
    KEY_RESERVED,     // 0x73
    KEY_RESERVED,     // 0x74
    KEY_RESERVED,     // 0x75
    KEY_RESERVED,     // 0x76
    KEY_RESERVED,     // 0x77
    KEY_RESERVED,     // 0x78
    KEY_RESERVED,     // 0x79
    KEY_RESERVED,     // 0x7A
    KEY_RESERVED,     // 0x7B
    KEY_RESERVED,     // 0x7C
    KEY_RESERVED,     // 0x7D
    KEY_RESERVED,     // 0x7E
    KEY_RESERVED,     // 0x7F
];

/// Extended scancode (E0 prefix) translation
static SCANCODE_SET1_EXT: [u16; 128] = {
    let mut table = [KEY_RESERVED; 128];
    table[0x1C] = KEY_KPENTER;
    table[0x1D] = KEY_RIGHTCTRL;
    table[0x35] = KEY_KPSLASH;
    table[0x38] = KEY_RIGHTALT;
    table[0x47] = KEY_HOME;
    table[0x48] = KEY_UP;
    table[0x49] = KEY_PAGEUP;
    table[0x4B] = KEY_LEFT;
    table[0x4D] = KEY_RIGHT;
    table[0x4F] = KEY_END;
    table[0x50] = KEY_DOWN;
    table[0x51] = KEY_PAGEDOWN;
    table[0x52] = KEY_INSERT;
    table[0x53] = KEY_DELETE;
    table[0x5B] = KEY_LEFTMETA;
    table[0x5C] = KEY_RIGHTMETA;
    table[0x5D] = KEY_COMPOSE;
    table
};

/// USB HID keycode to evdev keycode translation
static USB_HID_TO_EVDEV: [u16; 256] = {
    let mut table = [KEY_RESERVED; 256];
    table[0x04] = KEY_A;
    table[0x05] = KEY_B;
    table[0x06] = KEY_C;
    table[0x07] = KEY_D;
    table[0x08] = KEY_E;
    table[0x09] = KEY_F;
    table[0x0A] = KEY_G;
    table[0x0B] = KEY_H;
    table[0x0C] = KEY_I;
    table[0x0D] = KEY_J;
    table[0x0E] = KEY_K;
    table[0x0F] = KEY_L;
    table[0x10] = KEY_M;
    table[0x11] = KEY_N;
    table[0x12] = KEY_O;
    table[0x13] = KEY_P;
    table[0x14] = KEY_Q;
    table[0x15] = KEY_R;
    table[0x16] = KEY_S;
    table[0x17] = KEY_T;
    table[0x18] = KEY_U;
    table[0x19] = KEY_V;
    table[0x1A] = KEY_W;
    table[0x1B] = KEY_X;
    table[0x1C] = KEY_Y;
    table[0x1D] = KEY_Z;
    table[0x1E] = KEY_1;
    table[0x1F] = KEY_2;
    table[0x20] = KEY_3;
    table[0x21] = KEY_4;
    table[0x22] = KEY_5;
    table[0x23] = KEY_6;
    table[0x24] = KEY_7;
    table[0x25] = KEY_8;
    table[0x26] = KEY_9;
    table[0x27] = KEY_0;
    table[0x28] = KEY_ENTER;
    table[0x29] = KEY_ESC;
    table[0x2A] = KEY_BACKSPACE;
    table[0x2B] = KEY_TAB;
    table[0x2C] = KEY_SPACE;
    table[0x2D] = KEY_MINUS;
    table[0x2E] = KEY_EQUAL;
    table[0x2F] = KEY_LEFTBRACE;
    table[0x30] = KEY_RIGHTBRACE;
    table[0x31] = KEY_BACKSLASH;
    table[0x33] = KEY_SEMICOLON;
    table[0x34] = KEY_APOSTROPHE;
    table[0x35] = KEY_GRAVE;
    table[0x36] = KEY_COMMA;
    table[0x37] = KEY_DOT;
    table[0x38] = KEY_SLASH;
    table[0x39] = KEY_CAPSLOCK;
    table[0x3A] = KEY_F1;
    table[0x3B] = KEY_F2;
    table[0x3C] = KEY_F3;
    table[0x3D] = KEY_F4;
    table[0x3E] = KEY_F5;
    table[0x3F] = KEY_F6;
    table[0x40] = KEY_F7;
    table[0x41] = KEY_F8;
    table[0x42] = KEY_F9;
    table[0x43] = KEY_F10;
    table[0x44] = KEY_F11;
    table[0x45] = KEY_F12;
    table[0x46] = KEY_SYSRQ;
    table[0x47] = KEY_SCROLLLOCK;
    table[0x48] = KEY_PAUSE;
    table[0x49] = KEY_INSERT;
    table[0x4A] = KEY_HOME;
    table[0x4B] = KEY_PAGEUP;
    table[0x4C] = KEY_DELETE;
    table[0x4D] = KEY_END;
    table[0x4E] = KEY_PAGEDOWN;
    table[0x4F] = KEY_RIGHT;
    table[0x50] = KEY_LEFT;
    table[0x51] = KEY_DOWN;
    table[0x52] = KEY_UP;
    table[0x53] = KEY_NUMLOCK;
    table[0x54] = KEY_KPSLASH;
    table[0x55] = KEY_KPASTERISK;
    table[0x56] = KEY_KPMINUS;
    table[0x57] = KEY_KPPLUS;
    table[0x58] = KEY_KPENTER;
    table[0x59] = KEY_KP1;
    table[0x5A] = KEY_KP2;
    table[0x5B] = KEY_KP3;
    table[0x5C] = KEY_KP4;
    table[0x5D] = KEY_KP5;
    table[0x5E] = KEY_KP6;
    table[0x5F] = KEY_KP7;
    table[0x60] = KEY_KP8;
    table[0x61] = KEY_KP9;
    table[0x62] = KEY_KP0;
    table[0x63] = KEY_KPDOT;
    table[0x64] = KEY_102ND;
    table[0x65] = KEY_COMPOSE;
    table[0xE0] = KEY_LEFTCTRL;
    table[0xE1] = KEY_LEFTSHIFT;
    table[0xE2] = KEY_LEFTALT;
    table[0xE3] = KEY_LEFTMETA;
    table[0xE4] = KEY_RIGHTCTRL;
    table[0xE5] = KEY_RIGHTSHIFT;
    table[0xE6] = KEY_RIGHTALT;
    table[0xE7] = KEY_RIGHTMETA;
    table
};

/// US QWERTY key mappings
fn us_qwerty_mapping(key: u16) -> Option<KeyMapping> {
    Some(match key {
        KEY_A => KeyMapping::new('a', 'A'),
        KEY_B => KeyMapping::new('b', 'B'),
        KEY_C => KeyMapping::new('c', 'C'),
        KEY_D => KeyMapping::new('d', 'D'),
        KEY_E => KeyMapping::new('e', 'E'),
        KEY_F => KeyMapping::new('f', 'F'),
        KEY_G => KeyMapping::new('g', 'G'),
        KEY_H => KeyMapping::new('h', 'H'),
        KEY_I => KeyMapping::new('i', 'I'),
        KEY_J => KeyMapping::new('j', 'J'),
        KEY_K => KeyMapping::new('k', 'K'),
        KEY_L => KeyMapping::new('l', 'L'),
        KEY_M => KeyMapping::new('m', 'M'),
        KEY_N => KeyMapping::new('n', 'N'),
        KEY_O => KeyMapping::new('o', 'O'),
        KEY_P => KeyMapping::new('p', 'P'),
        KEY_Q => KeyMapping::new('q', 'Q'),
        KEY_R => KeyMapping::new('r', 'R'),
        KEY_S => KeyMapping::new('s', 'S'),
        KEY_T => KeyMapping::new('t', 'T'),
        KEY_U => KeyMapping::new('u', 'U'),
        KEY_V => KeyMapping::new('v', 'V'),
        KEY_W => KeyMapping::new('w', 'W'),
        KEY_X => KeyMapping::new('x', 'X'),
        KEY_Y => KeyMapping::new('y', 'Y'),
        KEY_Z => KeyMapping::new('z', 'Z'),
        KEY_1 => KeyMapping::new('1', '!'),
        KEY_2 => KeyMapping::new('2', '@'),
        KEY_3 => KeyMapping::new('3', '#'),
        KEY_4 => KeyMapping::new('4', '$'),
        KEY_5 => KeyMapping::new('5', '%'),
        KEY_6 => KeyMapping::new('6', '^'),
        KEY_7 => KeyMapping::new('7', '&'),
        KEY_8 => KeyMapping::new('8', '*'),
        KEY_9 => KeyMapping::new('9', '('),
        KEY_0 => KeyMapping::new('0', ')'),
        KEY_MINUS => KeyMapping::new('-', '_'),
        KEY_EQUAL => KeyMapping::new('=', '+'),
        KEY_LEFTBRACE => KeyMapping::new('[', '{'),
        KEY_RIGHTBRACE => KeyMapping::new(']', '}'),
        KEY_SEMICOLON => KeyMapping::new(';', ':'),
        KEY_APOSTROPHE => KeyMapping::new('\'', '"'),
        KEY_GRAVE => KeyMapping::new('`', '~'),
        KEY_BACKSLASH => KeyMapping::new('\\', '|'),
        KEY_COMMA => KeyMapping::new(',', '<'),
        KEY_DOT => KeyMapping::new('.', '>'),
        KEY_SLASH => KeyMapping::new('/', '?'),
        KEY_SPACE => KeyMapping::new(' ', ' '),
        KEY_TAB => KeyMapping::new('\t', '\t'),
        KEY_ENTER => KeyMapping::new('\n', '\n'),
        KEY_KPENTER => KeyMapping::new('\n', '\n'),
        KEY_KP0 => KeyMapping::new('0', '0'),
        KEY_KP1 => KeyMapping::new('1', '1'),
        KEY_KP2 => KeyMapping::new('2', '2'),
        KEY_KP3 => KeyMapping::new('3', '3'),
        KEY_KP4 => KeyMapping::new('4', '4'),
        KEY_KP5 => KeyMapping::new('5', '5'),
        KEY_KP6 => KeyMapping::new('6', '6'),
        KEY_KP7 => KeyMapping::new('7', '7'),
        KEY_KP8 => KeyMapping::new('8', '8'),
        KEY_KP9 => KeyMapping::new('9', '9'),
        KEY_KPDOT => KeyMapping::new('.', '.'),
        KEY_KPSLASH => KeyMapping::new('/', '/'),
        KEY_KPASTERISK => KeyMapping::new('*', '*'),
        KEY_KPMINUS => KeyMapping::new('-', '-'),
        KEY_KPPLUS => KeyMapping::new('+', '+'),
        _ => return None,
    })
}

/// PS/2 keyboard state
pub struct Ps2KeyboardState {
    /// Extended scancode prefix received
    extended: bool,
    /// Pause sequence state
    pause_seq: u8,
    /// Current modifiers
    modifiers: KeyboardModifiers,
    /// Key repeat handler
    repeat: KeyRepeatHandler,
    /// Current layout
    layout: KeyboardLayout,
}

impl Ps2KeyboardState {
    pub const fn new() -> Self {
        Self {
            extended: false,
            pause_seq: 0,
            modifiers: KeyboardModifiers::new(),
            repeat: KeyRepeatHandler::new(),
            layout: KeyboardLayout::UsQwerty,
        }
    }

    /// Process a scancode byte
    pub fn process_scancode(&mut self, scancode: u8) -> Option<(u16, bool)> {
        // Handle pause sequence (E1 1D 45 E1 9D C5)
        if scancode == 0xE1 {
            self.pause_seq = 1;
            return None;
        }

        if self.pause_seq > 0 {
            self.pause_seq += 1;
            if self.pause_seq == 6 {
                self.pause_seq = 0;
                return Some((KEY_PAUSE, true));
            }
            return None;
        }

        // Handle extended prefix
        if scancode == 0xE0 {
            self.extended = true;
            return None;
        }

        // Determine if key press or release
        let pressed = scancode & 0x80 == 0;
        let code = scancode & 0x7F;

        // Translate scancode to keycode
        let keycode = if self.extended {
            self.extended = false;
            SCANCODE_SET1_EXT.get(code as usize).copied().unwrap_or(KEY_RESERVED)
        } else {
            SCANCODE_SET1.get(code as usize).copied().unwrap_or(KEY_RESERVED)
        };

        if keycode == KEY_RESERVED {
            return None;
        }

        // Update modifier state
        self.modifiers.update(keycode, pressed);

        Some((keycode, pressed))
    }

    /// Get current modifiers
    pub fn modifiers(&self) -> &KeyboardModifiers {
        &self.modifiers
    }

    /// Set keyboard layout
    pub fn set_layout(&mut self, layout: KeyboardLayout) {
        self.layout = layout;
    }

    /// Translate keycode to character
    pub fn keycode_to_char(&self, keycode: u16) -> Option<char> {
        match self.layout {
            KeyboardLayout::UsQwerty => {
                us_qwerty_mapping(keycode)?.get_char(&self.modifiers)
            }
            // Add other layouts here
            _ => us_qwerty_mapping(keycode)?.get_char(&self.modifiers),
        }
    }
}

/// PS/2 keyboard driver
pub struct Ps2Keyboard {
    /// Input device
    device: Arc<InputDevice>,
    /// Keyboard state
    state: Mutex<Ps2KeyboardState>,
    /// Initialized flag
    initialized: AtomicBool,
}

impl Ps2Keyboard {
    pub fn new() -> Self {
        let device = InputDevice::keyboard(
            "PS/2 Keyboard",
            InputId::new(BUS_I8042, 0x0001, 0x0001, 1),
        );

        Self {
            device: Arc::new(device),
            state: Mutex::new(Ps2KeyboardState::new()),
            initialized: AtomicBool::new(false),
        }
    }

    /// Initialize the PS/2 keyboard
    pub fn init(&self) -> Result<(), InputError> {
        if self.initialized.swap(true, Ordering::SeqCst) {
            return Ok(());
        }

        // In a real implementation, we would:
        // 1. Reset the keyboard
        // 2. Set scancode set
        // 3. Enable keyboard
        // 4. Set up IRQ handler

        // Register with input subsystem
        input_manager().register_device(self.device.clone())?;

        Ok(())
    }

    /// Handle keyboard interrupt
    pub fn handle_interrupt(&self) {
        // Read scancode from data port
        // In real implementation: let scancode = inb(PS2_DATA_PORT);
        // For now, this is a placeholder
    }

    /// Process a scancode
    pub fn process_scancode(&self, scancode: u8) {
        let mut state = self.state.lock();

        if let Some((keycode, pressed)) = state.process_scancode(scancode) {
            // Report to input device
            self.device.report_key(keycode, pressed);
            self.device.sync();

            // Update LEDs if lock key changed
            if pressed && matches!(keycode, KEY_CAPSLOCK | KEY_NUMLOCK | KEY_SCROLLLOCK) {
                let led_state = state.modifiers.led_state();
                self.set_leds(led_state);
            }
        }
    }

    /// Set keyboard LEDs
    pub fn set_leds(&self, _state: u8) {
        // In real implementation:
        // ps2_write_data(PS2_CMD_SET_LEDS);
        // ps2_wait_ack();
        // ps2_write_data(state);
        // ps2_wait_ack();
    }

    /// Get the input device
    pub fn device(&self) -> &Arc<InputDevice> {
        &self.device
    }

    /// Get current modifiers
    pub fn modifiers(&self) -> KeyboardModifiers {
        *self.state.lock().modifiers()
    }
}

/// USB HID keyboard driver
pub struct UsbHidKeyboard {
    /// Input device
    device: Arc<InputDevice>,
    /// Current modifiers
    modifiers: RwLock<KeyboardModifiers>,
    /// Currently pressed keys
    pressed_keys: RwLock<[u8; 6]>,
    /// Key repeat handler
    repeat: KeyRepeatHandler,
    /// Keyboard layout
    layout: RwLock<KeyboardLayout>,
}

impl UsbHidKeyboard {
    pub fn new(name: &str, vendor_id: u16, product_id: u16) -> Self {
        let device = InputDevice::keyboard(
            name,
            InputId::usb(vendor_id, product_id, 1),
        );

        Self {
            device: Arc::new(device),
            modifiers: RwLock::new(KeyboardModifiers::new()),
            pressed_keys: RwLock::new([0; 6]),
            repeat: KeyRepeatHandler::new(),
            layout: RwLock::new(KeyboardLayout::UsQwerty),
        }
    }

    /// Register with input subsystem
    pub fn register(&self) -> Result<(), InputError> {
        input_manager().register_device(self.device.clone())
    }

    /// Process USB HID keyboard report
    pub fn process_report(&self, report: &[u8]) {
        if report.len() < 8 {
            return;
        }

        let modifier_byte = report[0];
        // report[1] is reserved
        let keys = &report[2..8];

        // Update modifier state
        {
            let mut mods = self.modifiers.write();

            // Left modifiers
            mods.left_ctrl = (modifier_byte & 0x01) != 0;
            mods.left_shift = (modifier_byte & 0x02) != 0;
            mods.left_alt = (modifier_byte & 0x04) != 0;
            mods.left_meta = (modifier_byte & 0x08) != 0;

            // Right modifiers
            mods.right_ctrl = (modifier_byte & 0x10) != 0;
            mods.right_shift = (modifier_byte & 0x20) != 0;
            mods.right_alt = (modifier_byte & 0x40) != 0;
            mods.right_meta = (modifier_byte & 0x80) != 0;
        }

        // Report modifier key changes
        self.report_modifier_changes(modifier_byte);

        // Check for key releases
        let old_keys = *self.pressed_keys.read();
        for &old_key in &old_keys {
            if old_key != 0 && !keys.contains(&old_key) {
                let evdev_key = self.hid_to_evdev(old_key);
                if evdev_key != KEY_RESERVED {
                    self.device.report_key(evdev_key, false);
                }
            }
        }

        // Check for key presses
        for &new_key in keys {
            if new_key != 0 && !old_keys.contains(&new_key) {
                let evdev_key = self.hid_to_evdev(new_key);
                if evdev_key != KEY_RESERVED {
                    self.device.report_key(evdev_key, true);

                    // Update lock states
                    let mut mods = self.modifiers.write();
                    mods.update(evdev_key, true);
                }
            }
        }

        // Update pressed keys
        *self.pressed_keys.write() = keys.try_into().unwrap_or([0; 6]);

        // Sync events
        self.device.sync();
    }

    /// Report modifier key changes
    fn report_modifier_changes(&self, _new_modifier: u8) {
        // Compare with previous state and report changes
        // This would track the previous modifier byte and report
        // individual key press/release events
    }

    /// Convert HID keycode to evdev keycode
    fn hid_to_evdev(&self, hid_code: u8) -> u16 {
        USB_HID_TO_EVDEV.get(hid_code as usize).copied().unwrap_or(KEY_RESERVED)
    }

    /// Get the input device
    pub fn device(&self) -> &Arc<InputDevice> {
        &self.device
    }

    /// Get current modifiers
    pub fn modifiers(&self) -> KeyboardModifiers {
        *self.modifiers.read()
    }

    /// Set keyboard layout
    pub fn set_layout(&self, layout: KeyboardLayout) {
        *self.layout.write() = layout;
    }

    /// Translate keycode to character
    pub fn keycode_to_char(&self, keycode: u16) -> Option<char> {
        let mods = self.modifiers.read();
        us_qwerty_mapping(keycode)?.get_char(&mods)
    }
}

/// Keyboard input handler
pub struct KeyboardHandler {
    /// Handler name
    name: String,
    /// Keyboard devices
    keyboards: RwLock<Vec<Arc<InputDevice>>>,
    /// Event callback
    callback: RwLock<Option<Box<dyn Fn(&InputDevice, &InputEvent) + Send + Sync>>>,
}

impl KeyboardHandler {
    pub fn new(name: &str) -> Self {
        Self {
            name: String::from(name),
            keyboards: RwLock::new(Vec::new()),
            callback: RwLock::new(None),
        }
    }

    /// Set event callback
    pub fn set_callback<F>(&self, callback: F)
    where
        F: Fn(&InputDevice, &InputEvent) + Send + Sync + 'static,
    {
        *self.callback.write() = Some(Box::new(callback));
    }
}

impl InputHandler for KeyboardHandler {
    fn name(&self) -> &str {
        &self.name
    }

    fn match_device(&self, device: &InputDevice) -> bool {
        device.capabilities.read().has_evbit(EV_KEY) &&
        device.capabilities.read().has_keybit(KEY_A)
    }

    fn connect(&self, device: &InputDevice) -> Result<(), InputError> {
        self.keyboards.write().push(Arc::new(InputDevice::keyboard(
            device.name(),
            device.device_id(),
        )));
        Ok(())
    }

    fn disconnect(&self, device: &InputDevice) {
        self.keyboards.write().retain(|kb| kb.id() != device.id());
    }

    fn event(&self, device: &InputDevice, event: &InputEvent) {
        if let Some(ref callback) = *self.callback.read() {
            callback(device, event);
        }
    }
}

/// Virtual keyboard for software input
pub struct VirtualKeyboard {
    /// Input device
    device: Arc<InputDevice>,
    /// Current modifiers
    modifiers: RwLock<KeyboardModifiers>,
}

impl VirtualKeyboard {
    pub fn new() -> Self {
        let device = InputDevice::keyboard(
            "Virtual Keyboard",
            InputId::virtual_device(),
        );

        Self {
            device: Arc::new(device),
            modifiers: RwLock::new(KeyboardModifiers::new()),
        }
    }

    /// Register with input subsystem
    pub fn register(&self) -> Result<(), InputError> {
        input_manager().register_device(self.device.clone())
    }

    /// Simulate a key press
    pub fn press(&self, key: u16) {
        self.modifiers.write().update(key, true);
        self.device.report_key(key, true);
        self.device.sync();
    }

    /// Simulate a key release
    pub fn release(&self, key: u16) {
        self.modifiers.write().update(key, false);
        self.device.report_key(key, false);
        self.device.sync();
    }

    /// Simulate a key tap (press + release)
    pub fn tap(&self, key: u16) {
        self.press(key);
        self.release(key);
    }

    /// Type a string
    pub fn type_string(&self, text: &str) {
        for ch in text.chars() {
            if let Some(key) = self.char_to_key(ch) {
                let needs_shift = ch.is_ascii_uppercase() ||
                    matches!(ch, '!' | '@' | '#' | '$' | '%' | '^' | '&' | '*' | '(' | ')' |
                                  '_' | '+' | '{' | '}' | '|' | ':' | '"' | '<' | '>' | '?' | '~');

                if needs_shift {
                    self.press(KEY_LEFTSHIFT);
                }

                self.tap(key);

                if needs_shift {
                    self.release(KEY_LEFTSHIFT);
                }
            }
        }
    }

    /// Convert character to keycode
    fn char_to_key(&self, ch: char) -> Option<u16> {
        Some(match ch.to_ascii_lowercase() {
            'a' => KEY_A,
            'b' => KEY_B,
            'c' => KEY_C,
            'd' => KEY_D,
            'e' => KEY_E,
            'f' => KEY_F,
            'g' => KEY_G,
            'h' => KEY_H,
            'i' => KEY_I,
            'j' => KEY_J,
            'k' => KEY_K,
            'l' => KEY_L,
            'm' => KEY_M,
            'n' => KEY_N,
            'o' => KEY_O,
            'p' => KEY_P,
            'q' => KEY_Q,
            'r' => KEY_R,
            's' => KEY_S,
            't' => KEY_T,
            'u' => KEY_U,
            'v' => KEY_V,
            'w' => KEY_W,
            'x' => KEY_X,
            'y' => KEY_Y,
            'z' => KEY_Z,
            '1' | '!' => KEY_1,
            '2' | '@' => KEY_2,
            '3' | '#' => KEY_3,
            '4' | '$' => KEY_4,
            '5' | '%' => KEY_5,
            '6' | '^' => KEY_6,
            '7' | '&' => KEY_7,
            '8' | '*' => KEY_8,
            '9' | '(' => KEY_9,
            '0' | ')' => KEY_0,
            ' ' => KEY_SPACE,
            '\t' => KEY_TAB,
            '\n' => KEY_ENTER,
            '-' | '_' => KEY_MINUS,
            '=' | '+' => KEY_EQUAL,
            '[' | '{' => KEY_LEFTBRACE,
            ']' | '}' => KEY_RIGHTBRACE,
            ';' | ':' => KEY_SEMICOLON,
            '\'' | '"' => KEY_APOSTROPHE,
            '`' | '~' => KEY_GRAVE,
            '\\' | '|' => KEY_BACKSLASH,
            ',' | '<' => KEY_COMMA,
            '.' | '>' => KEY_DOT,
            '/' | '?' => KEY_SLASH,
            _ => return None,
        })
    }

    /// Get the input device
    pub fn device(&self) -> &Arc<InputDevice> {
        &self.device
    }
}

/// Global PS/2 keyboard instance
static PS2_KEYBOARD: spin::Once<Ps2Keyboard> = spin::Once::new();

/// Get the PS/2 keyboard
pub fn ps2_keyboard() -> &'static Ps2Keyboard {
    PS2_KEYBOARD.call_once(|| Ps2Keyboard::new())
}

/// Initialize keyboard subsystem
pub fn init() {
    // Initialize PS/2 keyboard
    let kb = ps2_keyboard();
    if let Err(_e) = kb.init() {
        // Log error
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modifiers() {
        let mut mods = KeyboardModifiers::new();
        assert!(!mods.shift());
        assert!(!mods.ctrl());

        mods.update(KEY_LEFTSHIFT, true);
        assert!(mods.shift());
        assert!(mods.left_shift);

        mods.update(KEY_LEFTSHIFT, false);
        assert!(!mods.shift());
    }

    #[test]
    fn test_caps_lock() {
        let mut mods = KeyboardModifiers::new();
        assert!(!mods.caps_lock);

        mods.update(KEY_CAPSLOCK, true);
        assert!(mods.caps_lock);

        mods.update(KEY_CAPSLOCK, true);
        assert!(!mods.caps_lock);
    }

    #[test]
    fn test_ps2_scancode() {
        let mut state = Ps2KeyboardState::new();

        // Press 'A' (scancode 0x1E)
        let result = state.process_scancode(0x1E);
        assert_eq!(result, Some((KEY_A, true)));

        // Release 'A' (scancode 0x9E = 0x1E | 0x80)
        let result = state.process_scancode(0x9E);
        assert_eq!(result, Some((KEY_A, false)));
    }

    #[test]
    fn test_extended_scancode() {
        let mut state = Ps2KeyboardState::new();

        // Press Right Ctrl (E0 1D)
        state.process_scancode(0xE0);
        let result = state.process_scancode(0x1D);
        assert_eq!(result, Some((KEY_RIGHTCTRL, true)));
    }

    #[test]
    fn test_key_mapping() {
        let mapping = KeyMapping::new('a', 'A');
        let mods = KeyboardModifiers::new();

        assert_eq!(mapping.get_char(&mods), Some('a'));

        let mut shift_mods = KeyboardModifiers::new();
        shift_mods.left_shift = true;
        assert_eq!(mapping.get_char(&shift_mods), Some('A'));
    }

    #[test]
    fn test_us_qwerty() {
        let state = Ps2KeyboardState::new();

        assert_eq!(state.keycode_to_char(KEY_A), Some('a'));
        assert_eq!(state.keycode_to_char(KEY_1), Some('1'));
        assert_eq!(state.keycode_to_char(KEY_SPACE), Some(' '));
    }
}
