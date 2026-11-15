//! BuildOS GUI Event System
//!
//! Input events, window events, and event dispatch.

use super::{Point, Rect, WindowId};
use alloc::vec::Vec;
use alloc::boxed::Box;
use alloc::collections::VecDeque;
use crate::sync::Mutex;

/// Maximum events in queue
const MAX_EVENT_QUEUE: usize = 256;

/// Mouse button constants
#[allow(non_snake_case)]
pub mod MouseButton {
    pub const LEFT: u8 = 1;
    pub const RIGHT: u8 = 2;
    pub const MIDDLE: u8 = 4;
    pub const X1: u8 = 8;
    pub const X2: u8 = 16;
}

/// Key modifier flags
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyModifiers {
    bits: u8,
}

impl KeyModifiers {
    pub const NONE: Self = Self { bits: 0 };
    pub const SHIFT: Self = Self { bits: 1 << 0 };
    pub const CTRL: Self = Self { bits: 1 << 1 };
    pub const ALT: Self = Self { bits: 1 << 2 };
    pub const META: Self = Self { bits: 1 << 3 };
    pub const CAPS_LOCK: Self = Self { bits: 1 << 4 };
    pub const NUM_LOCK: Self = Self { bits: 1 << 5 };

    pub fn shift(self) -> bool {
        (self.bits & Self::SHIFT.bits) != 0
    }

    pub fn ctrl(self) -> bool {
        (self.bits & Self::CTRL.bits) != 0
    }

    pub fn alt(self) -> bool {
        (self.bits & Self::ALT.bits) != 0
    }

    pub fn meta(self) -> bool {
        (self.bits & Self::META.bits) != 0
    }
}

impl core::ops::BitOr for KeyModifiers {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self { bits: self.bits | rhs.bits }
    }
}

impl core::ops::BitAnd for KeyModifiers {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self {
        Self { bits: self.bits & rhs.bits }
    }
}

/// Key code (virtual key code)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum KeyCode {
    Unknown = 0,
    // Letters
    A = 1, B = 2, C = 3, D = 4, E = 5, F = 6, G = 7, H = 8,
    I = 9, J = 10, K = 11, L = 12, M = 13, N = 14, O = 15,
    P = 16, Q = 17, R = 18, S = 19, T = 20, U = 21, V = 22,
    W = 23, X = 24, Y = 25, Z = 26,
    // Numbers
    Key0 = 27, Key1 = 28, Key2 = 29, Key3 = 30, Key4 = 31,
    Key5 = 32, Key6 = 33, Key7 = 34, Key8 = 35, Key9 = 36,
    // Function keys
    F1 = 37, F2 = 38, F3 = 39, F4 = 40, F5 = 41, F6 = 42,
    F7 = 43, F8 = 44, F9 = 45, F10 = 46, F11 = 47, F12 = 48,
    // Special keys
    Escape = 49,
    Tab = 50,
    CapsLock = 51,
    Shift = 52,
    Ctrl = 53,
    Alt = 54,
    Meta = 55,
    Space = 56,
    Enter = 57,
    Backspace = 58,
    Delete = 59,
    Insert = 60,
    Home = 61,
    End = 62,
    PageUp = 63,
    PageDown = 64,
    Left = 65,
    Right = 66,
    Up = 67,
    Down = 68,
    PrintScreen = 69,
    ScrollLock = 70,
    Pause = 71,
    NumLock = 72,
    // Punctuation
    Grave = 73,
    Minus = 74,
    Equal = 75,
    LeftBracket = 76,
    RightBracket = 77,
    Backslash = 78,
    Semicolon = 79,
    Apostrophe = 80,
    Comma = 81,
    Period = 82,
    Slash = 83,
    // Numpad
    Num0 = 84, Num1 = 85, Num2 = 86, Num3 = 87, Num4 = 88,
    Num5 = 89, Num6 = 90, Num7 = 91, Num8 = 92, Num9 = 93,
    NumAdd = 94,
    NumSubtract = 95,
    NumMultiply = 96,
    NumDivide = 97,
    NumDecimal = 98,
    NumEnter = 99,
}

impl KeyCode {
    /// Convert to character (if applicable)
    pub fn to_char(self, modifiers: KeyModifiers) -> Option<char> {
        let shifted = modifiers.shift();
        match self {
            Self::A => Some(if shifted { 'A' } else { 'a' }),
            Self::B => Some(if shifted { 'B' } else { 'b' }),
            Self::C => Some(if shifted { 'C' } else { 'c' }),
            Self::D => Some(if shifted { 'D' } else { 'd' }),
            Self::E => Some(if shifted { 'E' } else { 'e' }),
            Self::F => Some(if shifted { 'F' } else { 'f' }),
            Self::G => Some(if shifted { 'G' } else { 'g' }),
            Self::H => Some(if shifted { 'H' } else { 'h' }),
            Self::I => Some(if shifted { 'I' } else { 'i' }),
            Self::J => Some(if shifted { 'J' } else { 'j' }),
            Self::K => Some(if shifted { 'K' } else { 'k' }),
            Self::L => Some(if shifted { 'L' } else { 'l' }),
            Self::M => Some(if shifted { 'M' } else { 'm' }),
            Self::N => Some(if shifted { 'N' } else { 'n' }),
            Self::O => Some(if shifted { 'O' } else { 'o' }),
            Self::P => Some(if shifted { 'P' } else { 'p' }),
            Self::Q => Some(if shifted { 'Q' } else { 'q' }),
            Self::R => Some(if shifted { 'R' } else { 'r' }),
            Self::S => Some(if shifted { 'S' } else { 's' }),
            Self::T => Some(if shifted { 'T' } else { 't' }),
            Self::U => Some(if shifted { 'U' } else { 'u' }),
            Self::V => Some(if shifted { 'V' } else { 'v' }),
            Self::W => Some(if shifted { 'W' } else { 'w' }),
            Self::X => Some(if shifted { 'X' } else { 'x' }),
            Self::Y => Some(if shifted { 'Y' } else { 'y' }),
            Self::Z => Some(if shifted { 'Z' } else { 'z' }),
            Self::Key0 => Some(if shifted { ')' } else { '0' }),
            Self::Key1 => Some(if shifted { '!' } else { '1' }),
            Self::Key2 => Some(if shifted { '@' } else { '2' }),
            Self::Key3 => Some(if shifted { '#' } else { '3' }),
            Self::Key4 => Some(if shifted { '$' } else { '4' }),
            Self::Key5 => Some(if shifted { '%' } else { '5' }),
            Self::Key6 => Some(if shifted { '^' } else { '6' }),
            Self::Key7 => Some(if shifted { '&' } else { '7' }),
            Self::Key8 => Some(if shifted { '*' } else { '8' }),
            Self::Key9 => Some(if shifted { '(' } else { '9' }),
            Self::Space => Some(' '),
            Self::Enter | Self::NumEnter => Some('\n'),
            Self::Tab => Some('\t'),
            Self::Grave => Some(if shifted { '~' } else { '`' }),
            Self::Minus => Some(if shifted { '_' } else { '-' }),
            Self::Equal => Some(if shifted { '+' } else { '=' }),
            Self::LeftBracket => Some(if shifted { '{' } else { '[' }),
            Self::RightBracket => Some(if shifted { '}' } else { ']' }),
            Self::Backslash => Some(if shifted { '|' } else { '\\' }),
            Self::Semicolon => Some(if shifted { ':' } else { ';' }),
            Self::Apostrophe => Some(if shifted { '"' } else { '\'' }),
            Self::Comma => Some(if shifted { '<' } else { ',' }),
            Self::Period => Some(if shifted { '>' } else { '.' }),
            Self::Slash => Some(if shifted { '?' } else { '/' }),
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
            Self::NumAdd => Some('+'),
            Self::NumSubtract => Some('-'),
            Self::NumMultiply => Some('*'),
            Self::NumDivide => Some('/'),
            Self::NumDecimal => Some('.'),
            _ => None,
        }
    }
}

/// Mouse event data
#[derive(Clone, Copy, Debug)]
pub struct MouseEvent {
    /// Mouse position
    pub position: Point,
    /// Button state
    pub buttons: u8,
    /// Button that triggered event
    pub button: u8,
    /// Scroll delta
    pub scroll_delta: Point,
    /// Modifiers
    pub modifiers: KeyModifiers,
}

/// Keyboard event data
#[derive(Clone, Copy, Debug)]
pub struct KeyEvent {
    /// Key code
    pub key: KeyCode,
    /// Scan code
    pub scan_code: u8,
    /// Is key pressed (vs released)
    pub pressed: bool,
    /// Is repeat
    pub repeat: bool,
    /// Modifiers
    pub modifiers: KeyModifiers,
    /// Character (if applicable)
    pub character: Option<char>,
}

/// Window event data
#[derive(Clone, Debug)]
pub enum WindowEvent {
    /// Window moved
    Moved { x: i32, y: i32 },
    /// Window resized
    Resized { width: u32, height: u32 },
    /// Window gained focus
    FocusGained,
    /// Window lost focus
    FocusLost,
    /// Window shown
    Shown,
    /// Window hidden
    Hidden,
    /// Window minimized
    Minimized,
    /// Window maximized
    Maximized,
    /// Window restored from minimize/maximize
    Restored,
    /// Close button clicked
    CloseRequested,
    /// Window destroyed
    Destroyed,
    /// Content needs repaint
    Paint { rect: Rect },
}

/// GUI event
#[derive(Clone, Debug)]
pub enum Event {
    /// Mouse move
    MouseMove(MouseEvent),
    /// Mouse button press
    MouseDown(MouseEvent),
    /// Mouse button release
    MouseUp(MouseEvent),
    /// Mouse double-click
    MouseDoubleClick(MouseEvent),
    /// Mouse wheel scroll
    MouseScroll(MouseEvent),
    /// Mouse entered window
    MouseEnter(MouseEvent),
    /// Mouse left window
    MouseLeave(MouseEvent),
    /// Key pressed
    KeyDown(KeyEvent),
    /// Key released
    KeyUp(KeyEvent),
    /// Text input (Unicode character)
    TextInput(char),
    /// Window event
    Window { window_id: WindowId, event: WindowEvent },
    /// Timer expired
    Timer { timer_id: u32 },
    /// Custom application event
    User { id: u32, data: u64 },
    /// System event
    System(SystemEvent),
    /// Quit request
    Quit,
}

/// System events
#[derive(Clone, Debug)]
pub enum SystemEvent {
    /// Display mode changed
    DisplayChanged { width: u32, height: u32 },
    /// Power state changed
    PowerStateChanged { on_battery: bool, charge: u8 },
    /// Clipboard updated
    ClipboardUpdated,
    /// Drag and drop
    DragDrop { x: i32, y: i32 },
}

/// Event queue
pub struct EventQueue {
    events: VecDeque<Event>,
    max_size: usize,
}

impl EventQueue {
    /// Create a new event queue
    pub fn new() -> Self {
        Self {
            events: VecDeque::with_capacity(MAX_EVENT_QUEUE),
            max_size: MAX_EVENT_QUEUE,
        }
    }

    /// Push an event
    pub fn push(&mut self, event: Event) {
        if self.events.len() >= self.max_size {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    /// Pop an event
    pub fn pop(&mut self) -> Option<Event> {
        self.events.pop_front()
    }

    /// Peek at front event
    pub fn peek(&self) -> Option<&Event> {
        self.events.front()
    }

    /// Is queue empty
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Get queue length
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Clear all events
    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// Filter events by predicate
    pub fn filter<F>(&mut self, predicate: F)
    where
        F: Fn(&Event) -> bool,
    {
        self.events.retain(|e| predicate(e));
    }
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Event handler trait
pub trait EventHandler {
    /// Handle an event, return true if handled
    fn handle_event(&mut self, event: &Event) -> bool;
}

/// Event dispatcher for routing events to handlers
pub struct EventDispatcher {
    /// Event queue
    queue: EventQueue,
    /// Global handlers
    global_handlers: Vec<Box<dyn EventHandler + Send>>,
    /// Window-specific handlers
    window_handlers: Vec<(WindowId, Box<dyn EventHandler + Send>)>,
    /// Current keyboard modifiers
    modifiers: KeyModifiers,
    /// Mouse button state
    mouse_buttons: u8,
    /// Mouse position
    mouse_position: Point,
}

impl EventDispatcher {
    /// Create a new event dispatcher
    pub fn new() -> Self {
        Self {
            queue: EventQueue::new(),
            global_handlers: Vec::new(),
            window_handlers: Vec::new(),
            modifiers: KeyModifiers::NONE,
            mouse_buttons: 0,
            mouse_position: Point::zero(),
        }
    }

    /// Add a global event handler
    pub fn add_global_handler(&mut self, handler: Box<dyn EventHandler + Send>) {
        self.global_handlers.push(handler);
    }

    /// Add a window event handler
    pub fn add_window_handler(&mut self, window_id: WindowId, handler: Box<dyn EventHandler + Send>) {
        self.window_handlers.push((window_id, handler));
    }

    /// Remove window handlers
    pub fn remove_window_handlers(&mut self, window_id: WindowId) {
        self.window_handlers.retain(|(id, _)| *id != window_id);
    }

    /// Post an event to the queue
    pub fn post_event(&mut self, event: Event) {
        self.queue.push(event);
    }

    /// Process keyboard input from driver
    pub fn process_keyboard(&mut self, scan_code: u8, pressed: bool) {
        let key = self.scan_code_to_key(scan_code);

        // Update modifiers
        match key {
            KeyCode::Shift => {
                if pressed {
                    self.modifiers = self.modifiers | KeyModifiers::SHIFT;
                } else {
                    self.modifiers.bits &= !KeyModifiers::SHIFT.bits;
                }
            }
            KeyCode::Ctrl => {
                if pressed {
                    self.modifiers = self.modifiers | KeyModifiers::CTRL;
                } else {
                    self.modifiers.bits &= !KeyModifiers::CTRL.bits;
                }
            }
            KeyCode::Alt => {
                if pressed {
                    self.modifiers = self.modifiers | KeyModifiers::ALT;
                } else {
                    self.modifiers.bits &= !KeyModifiers::ALT.bits;
                }
            }
            KeyCode::Meta => {
                if pressed {
                    self.modifiers = self.modifiers | KeyModifiers::META;
                } else {
                    self.modifiers.bits &= !KeyModifiers::META.bits;
                }
            }
            KeyCode::CapsLock => {
                if pressed {
                    self.modifiers.bits ^= KeyModifiers::CAPS_LOCK.bits;
                }
            }
            KeyCode::NumLock => {
                if pressed {
                    self.modifiers.bits ^= KeyModifiers::NUM_LOCK.bits;
                }
            }
            _ => {}
        }

        let character = key.to_char(self.modifiers);
        let event = KeyEvent {
            key,
            scan_code,
            pressed,
            repeat: false, // Would need tracking for this
            modifiers: self.modifiers,
            character,
        };

        if pressed {
            self.queue.push(Event::KeyDown(event));
            if let Some(c) = character {
                self.queue.push(Event::TextInput(c));
            }
        } else {
            self.queue.push(Event::KeyUp(event));
        }
    }

    /// Process mouse input from driver
    pub fn process_mouse(&mut self, x: i32, y: i32, buttons: u8) {
        let old_position = self.mouse_position;
        let old_buttons = self.mouse_buttons;

        self.mouse_position = Point::new(x, y);
        self.mouse_buttons = buttons;

        // Check for movement
        if old_position != self.mouse_position {
            let event = MouseEvent {
                position: self.mouse_position,
                buttons,
                button: 0,
                scroll_delta: Point::zero(),
                modifiers: self.modifiers,
            };
            self.queue.push(Event::MouseMove(event));
        }

        // Check for button changes
        let changed = old_buttons ^ buttons;
        for button in [MouseButton::LEFT, MouseButton::RIGHT, MouseButton::MIDDLE, MouseButton::X1, MouseButton::X2] {
            if (changed & button) != 0 {
                let event = MouseEvent {
                    position: self.mouse_position,
                    buttons,
                    button,
                    scroll_delta: Point::zero(),
                    modifiers: self.modifiers,
                };
                if (buttons & button) != 0 {
                    self.queue.push(Event::MouseDown(event));
                } else {
                    self.queue.push(Event::MouseUp(event));
                }
            }
        }
    }

    /// Process mouse scroll
    pub fn process_scroll(&mut self, delta_x: i32, delta_y: i32) {
        let event = MouseEvent {
            position: self.mouse_position,
            buttons: self.mouse_buttons,
            button: 0,
            scroll_delta: Point::new(delta_x, delta_y),
            modifiers: self.modifiers,
        };
        self.queue.push(Event::MouseScroll(event));
    }

    /// Dispatch all queued events
    pub fn dispatch(&mut self) {
        while let Some(event) = self.queue.pop() {
            self.dispatch_event(&event);
        }
    }

    /// Dispatch a single event
    fn dispatch_event(&mut self, event: &Event) {
        // Try global handlers first
        for handler in &mut self.global_handlers {
            if handler.handle_event(event) {
                return;
            }
        }

        // Try window handlers for window events
        if let Event::Window { window_id, .. } = event {
            for (id, handler) in &mut self.window_handlers {
                if *id == *window_id {
                    if handler.handle_event(event) {
                        return;
                    }
                }
            }
        }
    }

    /// Convert scan code to key code
    fn scan_code_to_key(&self, scan_code: u8) -> KeyCode {
        // Standard US keyboard layout scan codes
        match scan_code {
            0x01 => KeyCode::Escape,
            0x02 => KeyCode::Key1,
            0x03 => KeyCode::Key2,
            0x04 => KeyCode::Key3,
            0x05 => KeyCode::Key4,
            0x06 => KeyCode::Key5,
            0x07 => KeyCode::Key6,
            0x08 => KeyCode::Key7,
            0x09 => KeyCode::Key8,
            0x0A => KeyCode::Key9,
            0x0B => KeyCode::Key0,
            0x0C => KeyCode::Minus,
            0x0D => KeyCode::Equal,
            0x0E => KeyCode::Backspace,
            0x0F => KeyCode::Tab,
            0x10 => KeyCode::Q,
            0x11 => KeyCode::W,
            0x12 => KeyCode::E,
            0x13 => KeyCode::R,
            0x14 => KeyCode::T,
            0x15 => KeyCode::Y,
            0x16 => KeyCode::U,
            0x17 => KeyCode::I,
            0x18 => KeyCode::O,
            0x19 => KeyCode::P,
            0x1A => KeyCode::LeftBracket,
            0x1B => KeyCode::RightBracket,
            0x1C => KeyCode::Enter,
            0x1D => KeyCode::Ctrl,
            0x1E => KeyCode::A,
            0x1F => KeyCode::S,
            0x20 => KeyCode::D,
            0x21 => KeyCode::F,
            0x22 => KeyCode::G,
            0x23 => KeyCode::H,
            0x24 => KeyCode::J,
            0x25 => KeyCode::K,
            0x26 => KeyCode::L,
            0x27 => KeyCode::Semicolon,
            0x28 => KeyCode::Apostrophe,
            0x29 => KeyCode::Grave,
            0x2A => KeyCode::Shift,
            0x2B => KeyCode::Backslash,
            0x2C => KeyCode::Z,
            0x2D => KeyCode::X,
            0x2E => KeyCode::C,
            0x2F => KeyCode::V,
            0x30 => KeyCode::B,
            0x31 => KeyCode::N,
            0x32 => KeyCode::M,
            0x33 => KeyCode::Comma,
            0x34 => KeyCode::Period,
            0x35 => KeyCode::Slash,
            0x36 => KeyCode::Shift,
            0x37 => KeyCode::NumMultiply,
            0x38 => KeyCode::Alt,
            0x39 => KeyCode::Space,
            0x3A => KeyCode::CapsLock,
            0x3B => KeyCode::F1,
            0x3C => KeyCode::F2,
            0x3D => KeyCode::F3,
            0x3E => KeyCode::F4,
            0x3F => KeyCode::F5,
            0x40 => KeyCode::F6,
            0x41 => KeyCode::F7,
            0x42 => KeyCode::F8,
            0x43 => KeyCode::F9,
            0x44 => KeyCode::F10,
            0x45 => KeyCode::NumLock,
            0x46 => KeyCode::ScrollLock,
            0x47 => KeyCode::Num7,
            0x48 => KeyCode::Num8,
            0x49 => KeyCode::Num9,
            0x4A => KeyCode::NumSubtract,
            0x4B => KeyCode::Num4,
            0x4C => KeyCode::Num5,
            0x4D => KeyCode::Num6,
            0x4E => KeyCode::NumAdd,
            0x4F => KeyCode::Num1,
            0x50 => KeyCode::Num2,
            0x51 => KeyCode::Num3,
            0x52 => KeyCode::Num0,
            0x53 => KeyCode::NumDecimal,
            0x57 => KeyCode::F11,
            0x58 => KeyCode::F12,
            _ => KeyCode::Unknown,
        }
    }

    /// Get current modifiers
    pub fn modifiers(&self) -> KeyModifiers {
        self.modifiers
    }

    /// Get mouse position
    pub fn mouse_position(&self) -> Point {
        self.mouse_position
    }

    /// Get mouse buttons
    pub fn mouse_buttons(&self) -> u8 {
        self.mouse_buttons
    }
}

impl Default for EventDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Global event dispatcher
static EVENT_DISPATCHER: Mutex<Option<EventDispatcher>> = Mutex::new(None);

/// Initialize event system
pub fn init() {
    let mut dispatcher = EVENT_DISPATCHER.lock();
    *dispatcher = Some(EventDispatcher::new());
}

/// Post an event
pub fn post_event(event: Event) {
    if let Some(ref mut dispatcher) = *EVENT_DISPATCHER.lock() {
        dispatcher.post_event(event);
    }
}

/// Process keyboard input
pub fn process_keyboard(scan_code: u8, pressed: bool) {
    if let Some(ref mut dispatcher) = *EVENT_DISPATCHER.lock() {
        dispatcher.process_keyboard(scan_code, pressed);
    }
}

/// Process mouse input
pub fn process_mouse(x: i32, y: i32, buttons: u8) {
    if let Some(ref mut dispatcher) = *EVENT_DISPATCHER.lock() {
        dispatcher.process_mouse(x, y, buttons);
    }
}

/// Dispatch events
pub fn dispatch_events() {
    if let Some(ref mut dispatcher) = *EVENT_DISPATCHER.lock() {
        dispatcher.dispatch();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_to_char() {
        assert_eq!(KeyCode::A.to_char(KeyModifiers::NONE), Some('a'));
        assert_eq!(KeyCode::A.to_char(KeyModifiers::SHIFT), Some('A'));
        assert_eq!(KeyCode::Key1.to_char(KeyModifiers::SHIFT), Some('!'));
    }

    #[test]
    fn test_event_queue() {
        let mut queue = EventQueue::new();
        queue.push(Event::Quit);
        assert!(!queue.is_empty());
        assert!(matches!(queue.pop(), Some(Event::Quit)));
        assert!(queue.is_empty());
    }
}
