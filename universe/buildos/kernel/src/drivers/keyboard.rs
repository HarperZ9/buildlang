// ===============================================================================
// BUILDOS KERNEL - PS/2 KEYBOARD DRIVER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! PS/2 keyboard driver with full scancode set 1 translation.

use alloc::collections::VecDeque;
use spin::Mutex;

// =============================================================================
// I/O PORTS
// =============================================================================

/// PS/2 data port
const PS2_DATA_PORT: u16 = 0x60;

/// PS/2 status/command port
const PS2_STATUS_PORT: u16 = 0x64;
const PS2_COMMAND_PORT: u16 = 0x64;

// =============================================================================
// STATUS REGISTER BITS
// =============================================================================

/// Output buffer full (data available to read)
const STATUS_OUTPUT_FULL: u8 = 0x01;

/// Input buffer full (controller busy)
const STATUS_INPUT_FULL: u8 = 0x02;

/// System flag (POST passed)
const STATUS_SYSTEM_FLAG: u8 = 0x04;

/// Command/data (0 = data, 1 = command)
const STATUS_COMMAND: u8 = 0x08;

/// Timeout error
const STATUS_TIMEOUT: u8 = 0x40;

/// Parity error
const STATUS_PARITY: u8 = 0x80;

// =============================================================================
// CONTROLLER COMMANDS
// =============================================================================

/// Read controller configuration byte
const CMD_READ_CONFIG: u8 = 0x20;

/// Write controller configuration byte
const CMD_WRITE_CONFIG: u8 = 0x60;

/// Disable second PS/2 port
const CMD_DISABLE_PORT2: u8 = 0xA7;

/// Enable second PS/2 port
const CMD_ENABLE_PORT2: u8 = 0xA8;

/// Test second PS/2 port
const CMD_TEST_PORT2: u8 = 0xA9;

/// Test PS/2 controller
const CMD_TEST_CONTROLLER: u8 = 0xAA;

/// Test first PS/2 port
const CMD_TEST_PORT1: u8 = 0xAB;

/// Disable first PS/2 port
const CMD_DISABLE_PORT1: u8 = 0xAD;

/// Enable first PS/2 port
const CMD_ENABLE_PORT1: u8 = 0xAE;

// =============================================================================
// KEYBOARD COMMANDS
// =============================================================================

/// Set LEDs
const KBD_CMD_SET_LEDS: u8 = 0xED;

/// Echo (for testing)
const KBD_CMD_ECHO: u8 = 0xEE;

/// Set scancode set
const KBD_CMD_SET_SCANCODE: u8 = 0xF0;

/// Identify keyboard
const KBD_CMD_IDENTIFY: u8 = 0xF2;

/// Set typematic rate/delay
const KBD_CMD_SET_TYPEMATIC: u8 = 0xF3;

/// Enable scanning
const KBD_CMD_ENABLE: u8 = 0xF4;

/// Disable scanning
const KBD_CMD_DISABLE: u8 = 0xF5;

/// Reset and self-test
const KBD_CMD_RESET: u8 = 0xFF;

// =============================================================================
// KEY CODES
// =============================================================================

/// Key code representing a keyboard key
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    // Letters
    A, B, C, D, E, F, G, H, I, J, K, L, M,
    N, O, P, Q, R, S, T, U, V, W, X, Y, Z,

    // Numbers
    Key0, Key1, Key2, Key3, Key4, Key5, Key6, Key7, Key8, Key9,

    // Function keys
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,

    // Special keys
    Escape,
    Tab,
    CapsLock,
    LeftShift,
    RightShift,
    LeftCtrl,
    RightCtrl,
    LeftAlt,
    RightAlt,
    Space,
    Enter,
    Backspace,

    // Punctuation
    Minus,
    Equals,
    LeftBracket,
    RightBracket,
    Backslash,
    Semicolon,
    Quote,
    Backtick,
    Comma,
    Period,
    Slash,

    // Navigation
    Insert,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,

    // Numpad
    NumLock,
    NumSlash,
    NumStar,
    NumMinus,
    NumPlus,
    NumEnter,
    Num0, Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9,
    NumDot,

    // Extended keys
    PrintScreen,
    ScrollLock,
    Pause,

    // Unknown
    Unknown,
}

impl KeyCode {
    /// Convert scancode to KeyCode
    pub fn from_scancode(scancode: u8, extended: bool) -> Self {
        if extended {
            Self::from_extended_scancode(scancode)
        } else {
            Self::from_normal_scancode(scancode)
        }
    }

    fn from_normal_scancode(scancode: u8) -> Self {
        match scancode {
            0x01 => Self::Escape,
            0x02 => Self::Key1, 0x03 => Self::Key2, 0x04 => Self::Key3,
            0x05 => Self::Key4, 0x06 => Self::Key5, 0x07 => Self::Key6,
            0x08 => Self::Key7, 0x09 => Self::Key8, 0x0A => Self::Key9,
            0x0B => Self::Key0,
            0x0C => Self::Minus, 0x0D => Self::Equals,
            0x0E => Self::Backspace, 0x0F => Self::Tab,
            0x10 => Self::Q, 0x11 => Self::W, 0x12 => Self::E,
            0x13 => Self::R, 0x14 => Self::T, 0x15 => Self::Y,
            0x16 => Self::U, 0x17 => Self::I, 0x18 => Self::O,
            0x19 => Self::P, 0x1A => Self::LeftBracket, 0x1B => Self::RightBracket,
            0x1C => Self::Enter, 0x1D => Self::LeftCtrl,
            0x1E => Self::A, 0x1F => Self::S, 0x20 => Self::D,
            0x21 => Self::F, 0x22 => Self::G, 0x23 => Self::H,
            0x24 => Self::J, 0x25 => Self::K, 0x26 => Self::L,
            0x27 => Self::Semicolon, 0x28 => Self::Quote, 0x29 => Self::Backtick,
            0x2A => Self::LeftShift, 0x2B => Self::Backslash,
            0x2C => Self::Z, 0x2D => Self::X, 0x2E => Self::C,
            0x2F => Self::V, 0x30 => Self::B, 0x31 => Self::N,
            0x32 => Self::M, 0x33 => Self::Comma, 0x34 => Self::Period,
            0x35 => Self::Slash, 0x36 => Self::RightShift,
            0x37 => Self::NumStar, 0x38 => Self::LeftAlt, 0x39 => Self::Space,
            0x3A => Self::CapsLock,
            0x3B => Self::F1, 0x3C => Self::F2, 0x3D => Self::F3,
            0x3E => Self::F4, 0x3F => Self::F5, 0x40 => Self::F6,
            0x41 => Self::F7, 0x42 => Self::F8, 0x43 => Self::F9,
            0x44 => Self::F10, 0x45 => Self::NumLock, 0x46 => Self::ScrollLock,
            0x47 => Self::Num7, 0x48 => Self::Num8, 0x49 => Self::Num9,
            0x4A => Self::NumMinus, 0x4B => Self::Num4, 0x4C => Self::Num5,
            0x4D => Self::Num6, 0x4E => Self::NumPlus, 0x4F => Self::Num1,
            0x50 => Self::Num2, 0x51 => Self::Num3, 0x52 => Self::Num0,
            0x53 => Self::NumDot,
            0x57 => Self::F11, 0x58 => Self::F12,
            _ => Self::Unknown,
        }
    }

    fn from_extended_scancode(scancode: u8) -> Self {
        match scancode {
            0x1C => Self::NumEnter,
            0x1D => Self::LeftCtrl, // Actually RightCtrl
            0x35 => Self::NumSlash,
            0x38 => Self::LeftAlt,  // Actually RightAlt
            0x47 => Self::Home,
            0x48 => Self::ArrowUp,
            0x49 => Self::PageUp,
            0x4B => Self::ArrowLeft,
            0x4D => Self::ArrowRight,
            0x4F => Self::End,
            0x50 => Self::ArrowDown,
            0x51 => Self::PageDown,
            0x52 => Self::Insert,
            0x53 => Self::Delete,
            _ => Self::Unknown,
        }
    }

    /// Convert key to ASCII character (if applicable)
    pub fn to_ascii(&self, shift: bool, caps: bool) -> Option<char> {
        let uppercase = shift ^ caps;

        match self {
            // Letters
            Self::A => Some(if uppercase { 'A' } else { 'a' }),
            Self::B => Some(if uppercase { 'B' } else { 'b' }),
            Self::C => Some(if uppercase { 'C' } else { 'c' }),
            Self::D => Some(if uppercase { 'D' } else { 'd' }),
            Self::E => Some(if uppercase { 'E' } else { 'e' }),
            Self::F => Some(if uppercase { 'F' } else { 'f' }),
            Self::G => Some(if uppercase { 'G' } else { 'g' }),
            Self::H => Some(if uppercase { 'H' } else { 'h' }),
            Self::I => Some(if uppercase { 'I' } else { 'i' }),
            Self::J => Some(if uppercase { 'J' } else { 'j' }),
            Self::K => Some(if uppercase { 'K' } else { 'k' }),
            Self::L => Some(if uppercase { 'L' } else { 'l' }),
            Self::M => Some(if uppercase { 'M' } else { 'm' }),
            Self::N => Some(if uppercase { 'N' } else { 'n' }),
            Self::O => Some(if uppercase { 'O' } else { 'o' }),
            Self::P => Some(if uppercase { 'P' } else { 'p' }),
            Self::Q => Some(if uppercase { 'Q' } else { 'q' }),
            Self::R => Some(if uppercase { 'R' } else { 'r' }),
            Self::S => Some(if uppercase { 'S' } else { 's' }),
            Self::T => Some(if uppercase { 'T' } else { 't' }),
            Self::U => Some(if uppercase { 'U' } else { 'u' }),
            Self::V => Some(if uppercase { 'V' } else { 'v' }),
            Self::W => Some(if uppercase { 'W' } else { 'w' }),
            Self::X => Some(if uppercase { 'X' } else { 'x' }),
            Self::Y => Some(if uppercase { 'Y' } else { 'y' }),
            Self::Z => Some(if uppercase { 'Z' } else { 'z' }),

            // Numbers (top row)
            Self::Key1 => Some(if shift { '!' } else { '1' }),
            Self::Key2 => Some(if shift { '@' } else { '2' }),
            Self::Key3 => Some(if shift { '#' } else { '3' }),
            Self::Key4 => Some(if shift { '$' } else { '4' }),
            Self::Key5 => Some(if shift { '%' } else { '5' }),
            Self::Key6 => Some(if shift { '^' } else { '6' }),
            Self::Key7 => Some(if shift { '&' } else { '7' }),
            Self::Key8 => Some(if shift { '*' } else { '8' }),
            Self::Key9 => Some(if shift { '(' } else { '9' }),
            Self::Key0 => Some(if shift { ')' } else { '0' }),

            // Punctuation
            Self::Minus => Some(if shift { '_' } else { '-' }),
            Self::Equals => Some(if shift { '+' } else { '=' }),
            Self::LeftBracket => Some(if shift { '{' } else { '[' }),
            Self::RightBracket => Some(if shift { '}' } else { ']' }),
            Self::Backslash => Some(if shift { '|' } else { '\\' }),
            Self::Semicolon => Some(if shift { ':' } else { ';' }),
            Self::Quote => Some(if shift { '"' } else { '\'' }),
            Self::Backtick => Some(if shift { '~' } else { '`' }),
            Self::Comma => Some(if shift { '<' } else { ',' }),
            Self::Period => Some(if shift { '>' } else { '.' }),
            Self::Slash => Some(if shift { '?' } else { '/' }),

            // Special
            Self::Space => Some(' '),
            Self::Tab => Some('\t'),
            Self::Enter | Self::NumEnter => Some('\n'),
            Self::Backspace => Some('\x08'),

            // Numpad
            Self::Num0 => Some('0'),
            Self::Num1 => Some('1'),
            Self::Num2 => Some('2'),
            Self::Num3 => Some('3'),
            Self::Num4 => Some('4'),
            Self::Num5 => Some('5'),
            Self::Num6 => Some('6'),
            Self::Num7 => Some('7'),
            Self::Num8 => Some('8'),
            Self::Num9 => Some('9'),
            Self::NumDot => Some('.'),
            Self::NumStar => Some('*'),
            Self::NumSlash => Some('/'),
            Self::NumMinus => Some('-'),
            Self::NumPlus => Some('+'),

            _ => None,
        }
    }
}

// =============================================================================
// KEYBOARD EVENT
// =============================================================================

/// Keyboard event type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEvent {
    /// Key pressed
    Pressed(KeyCode),
    /// Key released
    Released(KeyCode),
}

// =============================================================================
// KEYBOARD STATE
// =============================================================================

/// Modifier key state
#[derive(Debug, Clone, Copy, Default)]
pub struct Modifiers {
    pub left_shift: bool,
    pub right_shift: bool,
    pub left_ctrl: bool,
    pub right_ctrl: bool,
    pub left_alt: bool,
    pub right_alt: bool,
    pub caps_lock: bool,
    pub num_lock: bool,
    pub scroll_lock: bool,
}

impl Modifiers {
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

/// Keyboard driver state
pub struct Keyboard {
    /// Event queue
    events: VecDeque<KeyEvent>,

    /// Current modifier state
    modifiers: Modifiers,

    /// Extended scancode flag (0xE0 prefix received)
    extended: bool,

    /// Expecting E1 sequence
    e1_state: u8,
}

impl Keyboard {
    /// Create new keyboard driver
    const fn new() -> Self {
        Self {
            events: VecDeque::new(),
            modifiers: Modifiers {
                left_shift: false,
                right_shift: false,
                left_ctrl: false,
                right_ctrl: false,
                left_alt: false,
                right_alt: false,
                caps_lock: false,
                num_lock: false,
                scroll_lock: false,
            },
            extended: false,
            e1_state: 0,
        }
    }

    /// Initialize the PS/2 keyboard controller
    pub unsafe fn init(&mut self) {
        // Disable devices during initialization
        send_command(CMD_DISABLE_PORT1);
        send_command(CMD_DISABLE_PORT2);

        // Flush output buffer
        while (inb(PS2_STATUS_PORT) & STATUS_OUTPUT_FULL) != 0 {
            let _ = inb(PS2_DATA_PORT);
        }

        // Read controller configuration
        send_command(CMD_READ_CONFIG);
        let mut config = read_data();

        // Disable IRQs and translation
        config &= !(0x01 | 0x02 | 0x40);

        // Write back configuration
        send_command(CMD_WRITE_CONFIG);
        send_data(config);

        // Self-test controller
        send_command(CMD_TEST_CONTROLLER);
        let test_result = read_data();
        if test_result != 0x55 {
            return; // Controller test failed
        }

        // Restore configuration (in case test reset it)
        send_command(CMD_WRITE_CONFIG);
        send_data(config);

        // Test first port
        send_command(CMD_TEST_PORT1);
        let port_test = read_data();
        if port_test != 0x00 {
            return; // Port test failed
        }

        // Enable first port
        send_command(CMD_ENABLE_PORT1);

        // Enable IRQ for first port
        send_command(CMD_READ_CONFIG);
        config = read_data();
        config |= 0x01; // Enable IRQ1
        send_command(CMD_WRITE_CONFIG);
        send_data(config);

        // Reset keyboard
        send_keyboard_command(KBD_CMD_RESET);
        let _ = read_data(); // ACK
        let self_test = read_data();
        if self_test != 0xAA {
            // Self-test may have failed but continue anyway
        }

        // Set scancode set 1 (most compatible)
        send_keyboard_command(KBD_CMD_SET_SCANCODE);
        let _ = read_data(); // ACK
        send_data(0x01); // Scancode set 1
        let _ = read_data(); // ACK

        // Enable scanning
        send_keyboard_command(KBD_CMD_ENABLE);
        let _ = read_data(); // ACK

        // Update LEDs
        self.update_leds();
    }

    /// Process a scancode from the keyboard
    pub fn process_scancode(&mut self, scancode: u8) {
        // Handle E0 prefix (extended keys)
        if scancode == 0xE0 {
            self.extended = true;
            return;
        }

        // Handle E1 prefix (Pause key)
        if scancode == 0xE1 {
            self.e1_state = 1;
            return;
        }

        // Handle Pause key sequence
        if self.e1_state > 0 {
            self.e1_state += 1;
            if self.e1_state == 6 {
                self.e1_state = 0;
                self.events.push_back(KeyEvent::Pressed(KeyCode::Pause));
            }
            return;
        }

        // Check for key release (bit 7 set)
        let released = (scancode & 0x80) != 0;
        let code = scancode & 0x7F;

        // Convert to key code
        let key = KeyCode::from_scancode(code, self.extended);
        self.extended = false;

        // Update modifier state
        match key {
            KeyCode::LeftShift => self.modifiers.left_shift = !released,
            KeyCode::RightShift => self.modifiers.right_shift = !released,
            KeyCode::LeftCtrl => {
                if self.extended {
                    self.modifiers.right_ctrl = !released;
                } else {
                    self.modifiers.left_ctrl = !released;
                }
            }
            KeyCode::LeftAlt => {
                if self.extended {
                    self.modifiers.right_alt = !released;
                } else {
                    self.modifiers.left_alt = !released;
                }
            }
            KeyCode::CapsLock if !released => {
                self.modifiers.caps_lock = !self.modifiers.caps_lock;
                unsafe { self.update_leds(); }
            }
            KeyCode::NumLock if !released => {
                self.modifiers.num_lock = !self.modifiers.num_lock;
                unsafe { self.update_leds(); }
            }
            KeyCode::ScrollLock if !released => {
                self.modifiers.scroll_lock = !self.modifiers.scroll_lock;
                unsafe { self.update_leds(); }
            }
            _ => {}
        }

        // Queue event
        let event = if released {
            KeyEvent::Released(key)
        } else {
            KeyEvent::Pressed(key)
        };

        self.events.push_back(event);
    }

    /// Get next keyboard event
    pub fn poll_event(&mut self) -> Option<KeyEvent> {
        self.events.pop_front()
    }

    /// Get next key as ASCII character
    pub fn poll_char(&mut self) -> Option<char> {
        while let Some(event) = self.poll_event() {
            if let KeyEvent::Pressed(key) = event {
                if let Some(c) = key.to_ascii(self.modifiers.shift(), self.modifiers.caps_lock) {
                    return Some(c);
                }
            }
        }
        None
    }

    /// Get current modifier state
    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// Update keyboard LEDs
    unsafe fn update_leds(&self) {
        let mut leds = 0u8;
        if self.modifiers.scroll_lock { leds |= 0x01; }
        if self.modifiers.num_lock { leds |= 0x02; }
        if self.modifiers.caps_lock { leds |= 0x04; }

        send_keyboard_command(KBD_CMD_SET_LEDS);
        let _ = read_data(); // ACK
        send_data(leds);
        let _ = read_data(); // ACK
    }

    /// Handle keyboard interrupt
    pub fn handle_interrupt(&mut self) {
        // Read scancode from data port
        let scancode = unsafe { inb(PS2_DATA_PORT) };
        self.process_scancode(scancode);
    }
}

// =============================================================================
// GLOBAL KEYBOARD INSTANCE
// =============================================================================

/// Global keyboard driver
pub static KEYBOARD: Mutex<Keyboard> = Mutex::new(Keyboard::new());

impl Keyboard {
    /// Check if there is pending keyboard input (static method for poll/select)
    pub fn has_pending_input() -> bool {
        KEYBOARD.lock().events.len() > 0
    }
}

/// Initialize the keyboard driver
pub unsafe fn init() {
    KEYBOARD.lock().init();
}

/// Handle keyboard interrupt (called from interrupt handler)
pub fn handle_interrupt() {
    KEYBOARD.lock().handle_interrupt();
}

/// Poll for next character
pub fn poll_char() -> Option<char> {
    KEYBOARD.lock().poll_char()
}

/// Poll for next key event
pub fn poll_event() -> Option<KeyEvent> {
    KEYBOARD.lock().poll_event()
}

// =============================================================================
// I/O HELPERS
// =============================================================================

/// Read byte from port
#[inline]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    core::arch::asm!(
        "in al, dx",
        in("dx") port,
        out("al") value,
        options(nostack, nomem)
    );
    value
}

/// Write byte to port
#[inline]
unsafe fn outb(port: u16, value: u8) {
    core::arch::asm!(
        "out dx, al",
        in("dx") port,
        in("al") value,
        options(nostack, nomem)
    );
}

/// Wait for controller input buffer to be empty
unsafe fn wait_input() {
    for _ in 0..100_000 {
        if (inb(PS2_STATUS_PORT) & STATUS_INPUT_FULL) == 0 {
            return;
        }
    }
}

/// Wait for controller output buffer to be full
unsafe fn wait_output() {
    for _ in 0..100_000 {
        if (inb(PS2_STATUS_PORT) & STATUS_OUTPUT_FULL) != 0 {
            return;
        }
    }
}

/// Send command to PS/2 controller
unsafe fn send_command(cmd: u8) {
    wait_input();
    outb(PS2_COMMAND_PORT, cmd);
}

/// Send data to PS/2 controller
unsafe fn send_data(data: u8) {
    wait_input();
    outb(PS2_DATA_PORT, data);
}

/// Read data from PS/2 controller
unsafe fn read_data() -> u8 {
    wait_output();
    inb(PS2_DATA_PORT)
}

/// Send command to keyboard (via data port)
unsafe fn send_keyboard_command(cmd: u8) {
    send_data(cmd);
}
