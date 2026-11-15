//! BuildOS Virtual Terminal (TTY) Subsystem
//!
//! Provides virtual terminal support with:
//! - Multiple virtual consoles
//! - ANSI/VT100 escape sequence processing
//! - Line discipline and terminal modes
//! - Pseudo-terminal (PTY) support

#![allow(dead_code)]

pub mod console;
pub mod pty;
pub mod line;
pub mod ansi;

use crate::sync::RwLock;
use crate::drivers::graphics::font::VgaFont;
use crate::drivers::graphics::{Color, Framebuffer};
use alloc::vec::Vec;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::collections::VecDeque;
use core::sync::atomic::{AtomicU32, Ordering};

/// Maximum number of virtual terminals
pub const MAX_TTYS: usize = 12;

/// Default terminal width
pub const DEFAULT_WIDTH: usize = 80;

/// Default terminal height
pub const DEFAULT_HEIGHT: usize = 25;

/// Terminal buffer size
pub const BUFFER_SIZE: usize = 4096;

/// TTY ID type
pub type TtyId = u32;

/// Next TTY ID
static NEXT_TTY_ID: AtomicU32 = AtomicU32::new(1);

/// Terminal mode flags
#[derive(Clone, Copy, Debug, Default)]
pub struct TerminalMode {
    bits: u32,
}

impl TerminalMode {
    pub const ECHO: Self = Self { bits: 1 << 0 };       // Echo input
    pub const ICANON: Self = Self { bits: 1 << 1 };     // Canonical mode (line editing)
    pub const ISIG: Self = Self { bits: 1 << 2 };       // Enable signals
    pub const ICRNL: Self = Self { bits: 1 << 3 };      // Map CR to NL
    pub const OPOST: Self = Self { bits: 1 << 4 };      // Output processing
    pub const ONLCR: Self = Self { bits: 1 << 5 };      // Map NL to CR-NL
    pub const RAW: Self = Self { bits: 1 << 6 };        // Raw mode (no processing)
    pub const CBREAK: Self = Self { bits: 1 << 7 };     // Break on character
    pub const NOFLSH: Self = Self { bits: 1 << 8 };     // Don't flush after interrupt

    pub const DEFAULT: Self = Self {
        bits: Self::ECHO.bits | Self::ICANON.bits | Self::ISIG.bits
            | Self::ICRNL.bits | Self::OPOST.bits | Self::ONLCR.bits,
    };

    pub fn contains(self, other: Self) -> bool {
        (self.bits & other.bits) == other.bits
    }

    pub fn insert(&mut self, other: Self) {
        self.bits |= other.bits;
    }

    pub fn remove(&mut self, other: Self) {
        self.bits &= !other.bits;
    }
}

/// Terminal attributes (termios-like)
#[derive(Clone, Copy, Debug)]
pub struct TerminalAttrs {
    /// Input mode flags
    pub mode: TerminalMode,
    /// Input speed (baud rate)
    pub ispeed: u32,
    /// Output speed (baud rate)
    pub ospeed: u32,
    /// Control characters
    pub cc: ControlChars,
}

impl Default for TerminalAttrs {
    fn default() -> Self {
        Self {
            mode: TerminalMode::DEFAULT,
            ispeed: 38400,
            ospeed: 38400,
            cc: ControlChars::default(),
        }
    }
}

/// Control characters
#[derive(Clone, Copy, Debug)]
pub struct ControlChars {
    /// Interrupt (^C)
    pub vintr: u8,
    /// Quit (^\)
    pub vquit: u8,
    /// Erase (^H / ^?)
    pub verase: u8,
    /// Kill (^U)
    pub vkill: u8,
    /// End of file (^D)
    pub veof: u8,
    /// Time for non-canonical read
    pub vtime: u8,
    /// Min chars for non-canonical read
    pub vmin: u8,
    /// Suspend (^Z)
    pub vsusp: u8,
    /// Start (^Q)
    pub vstart: u8,
    /// Stop (^S)
    pub vstop: u8,
    /// Reprint (^R)
    pub vreprint: u8,
    /// Word erase (^W)
    pub vwerase: u8,
    /// Next (^V)
    pub vlnext: u8,
}

impl Default for ControlChars {
    fn default() -> Self {
        Self {
            vintr: 0x03,    // ^C
            vquit: 0x1C,    // ^\
            verase: 0x7F,   // DEL
            vkill: 0x15,    // ^U
            veof: 0x04,     // ^D
            vtime: 0,
            vmin: 1,
            vsusp: 0x1A,    // ^Z
            vstart: 0x11,   // ^Q
            vstop: 0x13,    // ^S
            vreprint: 0x12, // ^R
            vwerase: 0x17,  // ^W
            vlnext: 0x16,   // ^V
        }
    }
}

/// Terminal size
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalSize {
    pub rows: u16,
    pub cols: u16,
    pub xpixel: u16,
    pub ypixel: u16,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self {
            rows: DEFAULT_HEIGHT as u16,
            cols: DEFAULT_WIDTH as u16,
            xpixel: DEFAULT_WIDTH as u16 * 8,
            ypixel: DEFAULT_HEIGHT as u16 * 16,
        }
    }
}

/// Terminal colors (ANSI)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum AnsiColor {
    Black = 0,
    Red = 1,
    Green = 2,
    Yellow = 3,
    Blue = 4,
    Magenta = 5,
    Cyan = 6,
    White = 7,
    BrightBlack = 8,
    BrightRed = 9,
    BrightGreen = 10,
    BrightYellow = 11,
    BrightBlue = 12,
    BrightMagenta = 13,
    BrightCyan = 14,
    BrightWhite = 15,
}

impl AnsiColor {
    pub fn to_color(self) -> Color {
        match self {
            Self::Black => Color::rgb(0, 0, 0),
            Self::Red => Color::rgb(170, 0, 0),
            Self::Green => Color::rgb(0, 170, 0),
            Self::Yellow => Color::rgb(170, 85, 0),
            Self::Blue => Color::rgb(0, 0, 170),
            Self::Magenta => Color::rgb(170, 0, 170),
            Self::Cyan => Color::rgb(0, 170, 170),
            Self::White => Color::rgb(170, 170, 170),
            Self::BrightBlack => Color::rgb(85, 85, 85),
            Self::BrightRed => Color::rgb(255, 85, 85),
            Self::BrightGreen => Color::rgb(85, 255, 85),
            Self::BrightYellow => Color::rgb(255, 255, 85),
            Self::BrightBlue => Color::rgb(85, 85, 255),
            Self::BrightMagenta => Color::rgb(255, 85, 255),
            Self::BrightCyan => Color::rgb(85, 255, 255),
            Self::BrightWhite => Color::rgb(255, 255, 255),
        }
    }

    pub fn from_code(code: u8) -> Self {
        match code {
            0 => Self::Black,
            1 => Self::Red,
            2 => Self::Green,
            3 => Self::Yellow,
            4 => Self::Blue,
            5 => Self::Magenta,
            6 => Self::Cyan,
            7 => Self::White,
            8 => Self::BrightBlack,
            9 => Self::BrightRed,
            10 => Self::BrightGreen,
            11 => Self::BrightYellow,
            12 => Self::BrightBlue,
            13 => Self::BrightMagenta,
            14 => Self::BrightCyan,
            _ => Self::BrightWhite,
        }
    }
}

/// Character attributes
#[derive(Clone, Copy, Debug, Default)]
pub struct CharAttrs {
    pub fg: u8,        // Foreground color (ANSI code)
    pub bg: u8,        // Background color (ANSI code)
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub blink: bool,
    pub reverse: bool,
    pub hidden: bool,
    pub strikethrough: bool,
}

impl CharAttrs {
    pub fn default_attrs() -> Self {
        Self {
            fg: AnsiColor::White as u8,
            bg: AnsiColor::Black as u8,
            bold: false,
            dim: false,
            italic: false,
            underline: false,
            blink: false,
            reverse: false,
            hidden: false,
            strikethrough: false,
        }
    }

    pub fn fg_color(&self) -> Color {
        let code = if self.bold && self.fg < 8 {
            self.fg + 8 // Make bright if bold
        } else {
            self.fg
        };
        AnsiColor::from_code(code).to_color()
    }

    pub fn bg_color(&self) -> Color {
        AnsiColor::from_code(self.bg).to_color()
    }
}

/// Character cell in terminal buffer
#[derive(Clone, Copy, Debug)]
pub struct Cell {
    pub ch: char,
    pub attrs: CharAttrs,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            attrs: CharAttrs::default_attrs(),
        }
    }
}

/// Terminal screen buffer
pub struct ScreenBuffer {
    /// Cell buffer
    cells: Vec<Cell>,
    /// Width in characters
    width: usize,
    /// Height in characters
    height: usize,
    /// Cursor X position
    cursor_x: usize,
    /// Cursor Y position
    cursor_y: usize,
    /// Cursor visible
    cursor_visible: bool,
    /// Current character attributes
    current_attrs: CharAttrs,
    /// Scroll region top
    scroll_top: usize,
    /// Scroll region bottom
    scroll_bottom: usize,
    /// Saved cursor position
    saved_cursor: (usize, usize),
    /// Tab stops
    tab_stops: Vec<bool>,
}

impl ScreenBuffer {
    /// Create a new screen buffer
    pub fn new(width: usize, height: usize) -> Self {
        let size = width * height;
        let mut tab_stops = vec![false; width];
        for i in (0..width).step_by(8) {
            tab_stops[i] = true;
        }

        Self {
            cells: vec![Cell::default(); size],
            width,
            height,
            cursor_x: 0,
            cursor_y: 0,
            cursor_visible: true,
            current_attrs: CharAttrs::default_attrs(),
            scroll_top: 0,
            scroll_bottom: height,
            saved_cursor: (0, 0),
            tab_stops,
        }
    }

    /// Get cell at position
    pub fn get(&self, x: usize, y: usize) -> Option<&Cell> {
        if x < self.width && y < self.height {
            Some(&self.cells[y * self.width + x])
        } else {
            None
        }
    }

    /// Set cell at position
    pub fn set(&mut self, x: usize, y: usize, cell: Cell) {
        if x < self.width && y < self.height {
            self.cells[y * self.width + x] = cell;
        }
    }

    /// Put character at cursor
    pub fn put_char(&mut self, ch: char) {
        if self.cursor_x < self.width && self.cursor_y < self.height {
            self.cells[self.cursor_y * self.width + self.cursor_x] = Cell {
                ch,
                attrs: self.current_attrs,
            };
            self.cursor_x += 1;
            if self.cursor_x >= self.width {
                self.cursor_x = 0;
                self.newline();
            }
        }
    }

    /// Newline
    pub fn newline(&mut self) {
        self.cursor_y += 1;
        if self.cursor_y >= self.scroll_bottom {
            self.scroll_up(1);
            self.cursor_y = self.scroll_bottom - 1;
        }
    }

    /// Carriage return
    pub fn carriage_return(&mut self) {
        self.cursor_x = 0;
    }

    /// Tab
    pub fn tab(&mut self) {
        // Find next tab stop
        for x in (self.cursor_x + 1)..self.width {
            if self.tab_stops.get(x).copied().unwrap_or(false) {
                self.cursor_x = x;
                return;
            }
        }
        self.cursor_x = self.width - 1;
    }

    /// Backspace
    pub fn backspace(&mut self) {
        if self.cursor_x > 0 {
            self.cursor_x -= 1;
        }
    }

    /// Scroll up n lines
    pub fn scroll_up(&mut self, n: usize) {
        for _ in 0..n {
            // Move lines up within scroll region
            for y in self.scroll_top..(self.scroll_bottom - 1) {
                for x in 0..self.width {
                    self.cells[y * self.width + x] = self.cells[(y + 1) * self.width + x];
                }
            }
            // Clear bottom line
            let y = self.scroll_bottom - 1;
            for x in 0..self.width {
                self.cells[y * self.width + x] = Cell::default();
            }
        }
    }

    /// Scroll down n lines
    pub fn scroll_down(&mut self, n: usize) {
        for _ in 0..n {
            // Move lines down within scroll region
            for y in ((self.scroll_top + 1)..self.scroll_bottom).rev() {
                for x in 0..self.width {
                    self.cells[y * self.width + x] = self.cells[(y - 1) * self.width + x];
                }
            }
            // Clear top line
            let y = self.scroll_top;
            for x in 0..self.width {
                self.cells[y * self.width + x] = Cell::default();
            }
        }
    }

    /// Clear screen
    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            *cell = Cell::default();
        }
        self.cursor_x = 0;
        self.cursor_y = 0;
    }

    /// Clear to end of line
    pub fn clear_to_eol(&mut self) {
        for x in self.cursor_x..self.width {
            self.cells[self.cursor_y * self.width + x] = Cell::default();
        }
    }

    /// Clear to beginning of line
    pub fn clear_to_bol(&mut self) {
        for x in 0..=self.cursor_x {
            if x < self.width {
                self.cells[self.cursor_y * self.width + x] = Cell::default();
            }
        }
    }

    /// Clear entire line
    pub fn clear_line(&mut self) {
        for x in 0..self.width {
            self.cells[self.cursor_y * self.width + x] = Cell::default();
        }
    }

    /// Clear to end of screen
    pub fn clear_to_eos(&mut self) {
        self.clear_to_eol();
        for y in (self.cursor_y + 1)..self.height {
            for x in 0..self.width {
                self.cells[y * self.width + x] = Cell::default();
            }
        }
    }

    /// Clear to beginning of screen
    pub fn clear_to_bos(&mut self) {
        self.clear_to_bol();
        for y in 0..self.cursor_y {
            for x in 0..self.width {
                self.cells[y * self.width + x] = Cell::default();
            }
        }
    }

    /// Move cursor
    pub fn move_cursor(&mut self, x: usize, y: usize) {
        self.cursor_x = x.min(self.width - 1);
        self.cursor_y = y.min(self.height - 1);
    }

    /// Move cursor relative
    pub fn move_cursor_rel(&mut self, dx: i32, dy: i32) {
        let new_x = (self.cursor_x as i32 + dx).clamp(0, self.width as i32 - 1) as usize;
        let new_y = (self.cursor_y as i32 + dy).clamp(0, self.height as i32 - 1) as usize;
        self.cursor_x = new_x;
        self.cursor_y = new_y;
    }

    /// Save cursor position
    pub fn save_cursor(&mut self) {
        self.saved_cursor = (self.cursor_x, self.cursor_y);
    }

    /// Restore cursor position
    pub fn restore_cursor(&mut self) {
        self.cursor_x = self.saved_cursor.0;
        self.cursor_y = self.saved_cursor.1;
    }

    /// Set scroll region
    pub fn set_scroll_region(&mut self, top: usize, bottom: usize) {
        self.scroll_top = top.min(self.height - 1);
        self.scroll_bottom = bottom.min(self.height).max(self.scroll_top + 1);
    }

    /// Insert n blank lines
    pub fn insert_lines(&mut self, n: usize) {
        if self.cursor_y >= self.scroll_top && self.cursor_y < self.scroll_bottom {
            for _ in 0..n {
                // Move lines down
                for y in ((self.cursor_y + 1)..self.scroll_bottom).rev() {
                    for x in 0..self.width {
                        self.cells[y * self.width + x] = self.cells[(y - 1) * self.width + x];
                    }
                }
                // Clear current line
                for x in 0..self.width {
                    self.cells[self.cursor_y * self.width + x] = Cell::default();
                }
            }
        }
    }

    /// Delete n lines
    pub fn delete_lines(&mut self, n: usize) {
        if self.cursor_y >= self.scroll_top && self.cursor_y < self.scroll_bottom {
            for _ in 0..n {
                // Move lines up
                for y in self.cursor_y..(self.scroll_bottom - 1) {
                    for x in 0..self.width {
                        self.cells[y * self.width + x] = self.cells[(y + 1) * self.width + x];
                    }
                }
                // Clear bottom line
                let y = self.scroll_bottom - 1;
                for x in 0..self.width {
                    self.cells[y * self.width + x] = Cell::default();
                }
            }
        }
    }

    /// Insert n blank characters
    pub fn insert_chars(&mut self, n: usize) {
        // Move characters right
        for x in ((self.cursor_x + n)..self.width).rev() {
            self.cells[self.cursor_y * self.width + x] =
                self.cells[self.cursor_y * self.width + x - n];
        }
        // Clear inserted positions
        for x in self.cursor_x..(self.cursor_x + n).min(self.width) {
            self.cells[self.cursor_y * self.width + x] = Cell::default();
        }
    }

    /// Delete n characters
    pub fn delete_chars(&mut self, n: usize) {
        // Move characters left
        for x in self.cursor_x..(self.width - n) {
            self.cells[self.cursor_y * self.width + x] =
                self.cells[self.cursor_y * self.width + x + n];
        }
        // Clear end of line
        for x in (self.width - n)..self.width {
            self.cells[self.cursor_y * self.width + x] = Cell::default();
        }
    }

    /// Resize buffer
    pub fn resize(&mut self, new_width: usize, new_height: usize) {
        let mut new_cells = vec![Cell::default(); new_width * new_height];

        // Copy existing content
        let copy_width = self.width.min(new_width);
        let copy_height = self.height.min(new_height);

        for y in 0..copy_height {
            for x in 0..copy_width {
                new_cells[y * new_width + x] = self.cells[y * self.width + x];
            }
        }

        self.cells = new_cells;
        self.width = new_width;
        self.height = new_height;
        self.scroll_bottom = new_height;

        // Adjust cursor
        self.cursor_x = self.cursor_x.min(new_width - 1);
        self.cursor_y = self.cursor_y.min(new_height - 1);

        // Reset tab stops
        self.tab_stops = vec![false; new_width];
        for i in (0..new_width).step_by(8) {
            self.tab_stops[i] = true;
        }
    }
}

/// Virtual terminal
pub struct VirtualTerminal {
    /// Terminal ID
    pub id: TtyId,
    /// Terminal name
    pub name: String,
    /// Screen buffer
    screen: ScreenBuffer,
    /// Terminal attributes
    attrs: TerminalAttrs,
    /// Terminal size
    size: TerminalSize,
    /// Input buffer
    input_buffer: VecDeque<u8>,
    /// Output buffer
    output_buffer: VecDeque<u8>,
    /// Line buffer for canonical mode
    line_buffer: Vec<u8>,
    /// Is active (displayed)
    active: bool,
    /// Foreground process group ID
    fg_pgrp: u32,
    /// Session ID
    session_id: u32,
    /// Bell enabled
    bell_enabled: bool,
    /// Dirty (needs redraw)
    dirty: bool,
}

impl VirtualTerminal {
    /// Create a new virtual terminal
    pub fn new(name: &str) -> Self {
        let id = NEXT_TTY_ID.fetch_add(1, Ordering::SeqCst);
        let size = TerminalSize::default();

        Self {
            id,
            name: String::from(name),
            screen: ScreenBuffer::new(size.cols as usize, size.rows as usize),
            attrs: TerminalAttrs::default(),
            size,
            input_buffer: VecDeque::with_capacity(BUFFER_SIZE),
            output_buffer: VecDeque::with_capacity(BUFFER_SIZE),
            line_buffer: Vec::with_capacity(256),
            active: false,
            fg_pgrp: 0,
            session_id: 0,
            bell_enabled: true,
            dirty: true,
        }
    }

    /// Create a new virtual terminal with specified size
    pub fn with_size(id: TtyId, cols: usize, rows: usize) -> Self {
        let size = TerminalSize {
            rows: rows as u16,
            cols: cols as u16,
            xpixel: cols as u16 * 8,
            ypixel: rows as u16 * 16,
        };

        Self {
            id,
            name: alloc::format!("vt{}", id),
            screen: ScreenBuffer::new(cols, rows),
            attrs: TerminalAttrs::default(),
            size,
            input_buffer: VecDeque::with_capacity(BUFFER_SIZE),
            output_buffer: VecDeque::with_capacity(BUFFER_SIZE),
            line_buffer: Vec::with_capacity(256),
            active: false,
            fg_pgrp: 0,
            session_id: 0,
            bell_enabled: true,
            dirty: true,
        }
    }

    /// Process a single byte of output
    pub fn process_byte(&mut self, byte: u8) {
        self.output_buffer.push_back(byte);
        self.process_output();
        self.dirty = true;
    }

    /// Write output to terminal
    pub fn write(&mut self, data: &[u8]) {
        for &byte in data {
            self.output_buffer.push_back(byte);
        }
        self.process_output();
        self.dirty = true;
    }

    /// Write string to terminal
    pub fn write_str(&mut self, s: &str) {
        self.write(s.as_bytes());
    }

    /// Process output buffer (handle escape sequences)
    fn process_output(&mut self) {
        while let Some(byte) = self.output_buffer.pop_front() {
            match byte {
                // Control characters
                0x07 => self.bell(),
                0x08 => self.screen.backspace(),
                0x09 => self.screen.tab(),
                0x0A => self.screen.newline(),
                0x0D => self.screen.carriage_return(),
                0x1B => self.process_escape(),
                // Printable characters
                0x20..=0x7E => self.screen.put_char(byte as char),
                _ => {} // Ignore other control chars
            }
        }
    }

    /// Process escape sequence
    fn process_escape(&mut self) {
        // Get next character
        let Some(next) = self.output_buffer.pop_front() else { return };

        match next {
            b'[' => self.process_csi(),
            b']' => self.process_osc(),
            b'(' | b')' => { self.output_buffer.pop_front(); } // Charset selection
            b'7' => self.screen.save_cursor(),
            b'8' => self.screen.restore_cursor(),
            b'M' => self.screen.scroll_down(1),
            b'D' => self.screen.scroll_up(1),
            b'E' => { self.screen.carriage_return(); self.screen.newline(); }
            b'c' => self.reset(),
            _ => {}
        }
    }

    /// Process CSI (Control Sequence Introducer) sequence
    fn process_csi(&mut self) {
        let mut params: Vec<u32> = Vec::new();
        let mut current_param: u32 = 0;
        let mut private_mode = false;

        // Parse parameters
        loop {
            let Some(byte) = self.output_buffer.pop_front() else { return };

            match byte {
                b'?' => private_mode = true,
                b'0'..=b'9' => {
                    current_param = current_param * 10 + (byte - b'0') as u32;
                }
                b';' => {
                    params.push(current_param);
                    current_param = 0;
                }
                b'A'..=b'~' => {
                    params.push(current_param);
                    self.execute_csi(byte, &params, private_mode);
                    return;
                }
                _ => return,
            }
        }
    }

    /// Execute CSI command
    fn execute_csi(&mut self, cmd: u8, params: &[u32], private: bool) {
        let p1 = params.first().copied().unwrap_or(1).max(1) as usize;
        let p2 = params.get(1).copied().unwrap_or(1).max(1) as usize;

        match cmd {
            b'A' => self.screen.move_cursor_rel(0, -(p1 as i32)), // Cursor up
            b'B' => self.screen.move_cursor_rel(0, p1 as i32),    // Cursor down
            b'C' => self.screen.move_cursor_rel(p1 as i32, 0),    // Cursor right
            b'D' => self.screen.move_cursor_rel(-(p1 as i32), 0), // Cursor left
            b'E' => { // Cursor next line
                self.screen.cursor_x = 0;
                self.screen.move_cursor_rel(0, p1 as i32);
            }
            b'F' => { // Cursor previous line
                self.screen.cursor_x = 0;
                self.screen.move_cursor_rel(0, -(p1 as i32));
            }
            b'G' => self.screen.cursor_x = (p1 - 1).min(self.screen.width - 1), // Cursor column
            b'H' | b'f' => { // Cursor position
                self.screen.move_cursor(p2 - 1, p1 - 1);
            }
            b'J' => { // Erase display
                match params.first().copied().unwrap_or(0) {
                    0 => self.screen.clear_to_eos(),
                    1 => self.screen.clear_to_bos(),
                    2 | 3 => self.screen.clear(),
                    _ => {}
                }
            }
            b'K' => { // Erase line
                match params.first().copied().unwrap_or(0) {
                    0 => self.screen.clear_to_eol(),
                    1 => self.screen.clear_to_bol(),
                    2 => self.screen.clear_line(),
                    _ => {}
                }
            }
            b'L' => self.screen.insert_lines(p1),
            b'M' => self.screen.delete_lines(p1),
            b'P' => self.screen.delete_chars(p1),
            b'@' => self.screen.insert_chars(p1),
            b'S' => self.screen.scroll_up(p1),
            b'T' => self.screen.scroll_down(p1),
            b'd' => self.screen.cursor_y = (p1 - 1).min(self.screen.height - 1), // Line position
            b'm' => self.process_sgr(params), // SGR (Select Graphic Rendition)
            b'r' => { // Set scroll region
                let top = params.first().copied().unwrap_or(1).max(1) as usize - 1;
                let bottom = params.get(1).copied().unwrap_or(self.screen.height as u32) as usize;
                self.screen.set_scroll_region(top, bottom);
            }
            b'h' | b'l' => { // Set/reset mode
                if private {
                    self.set_private_mode(params.first().copied().unwrap_or(0), cmd == b'h');
                }
            }
            b's' => self.screen.save_cursor(),
            b'u' => self.screen.restore_cursor(),
            _ => {}
        }
    }

    /// Process SGR (Select Graphic Rendition)
    fn process_sgr(&mut self, params: &[u32]) {
        if params.is_empty() || params == [0] {
            self.screen.current_attrs = CharAttrs::default_attrs();
            return;
        }

        let mut i = 0;
        while i < params.len() {
            match params[i] {
                0 => self.screen.current_attrs = CharAttrs::default_attrs(),
                1 => self.screen.current_attrs.bold = true,
                2 => self.screen.current_attrs.dim = true,
                3 => self.screen.current_attrs.italic = true,
                4 => self.screen.current_attrs.underline = true,
                5 | 6 => self.screen.current_attrs.blink = true,
                7 => self.screen.current_attrs.reverse = true,
                8 => self.screen.current_attrs.hidden = true,
                9 => self.screen.current_attrs.strikethrough = true,
                22 => { self.screen.current_attrs.bold = false; self.screen.current_attrs.dim = false; }
                23 => self.screen.current_attrs.italic = false,
                24 => self.screen.current_attrs.underline = false,
                25 => self.screen.current_attrs.blink = false,
                27 => self.screen.current_attrs.reverse = false,
                28 => self.screen.current_attrs.hidden = false,
                29 => self.screen.current_attrs.strikethrough = false,
                30..=37 => self.screen.current_attrs.fg = (params[i] - 30) as u8,
                38 => {
                    // Extended foreground color
                    if params.get(i + 1) == Some(&5) && i + 2 < params.len() {
                        self.screen.current_attrs.fg = params[i + 2].min(255) as u8;
                        i += 2;
                    }
                }
                39 => self.screen.current_attrs.fg = AnsiColor::White as u8,
                40..=47 => self.screen.current_attrs.bg = (params[i] - 40) as u8,
                48 => {
                    // Extended background color
                    if params.get(i + 1) == Some(&5) && i + 2 < params.len() {
                        self.screen.current_attrs.bg = params[i + 2].min(255) as u8;
                        i += 2;
                    }
                }
                49 => self.screen.current_attrs.bg = AnsiColor::Black as u8,
                90..=97 => self.screen.current_attrs.fg = (params[i] - 90 + 8) as u8,
                100..=107 => self.screen.current_attrs.bg = (params[i] - 100 + 8) as u8,
                _ => {}
            }
            i += 1;
        }
    }

    /// Set private mode
    fn set_private_mode(&mut self, mode: u32, enable: bool) {
        match mode {
            1 => {} // Application cursor keys
            25 => self.screen.cursor_visible = enable, // Cursor visible
            1049 => { // Alternate screen buffer
                if enable {
                    self.screen.save_cursor();
                    self.screen.clear();
                } else {
                    self.screen.restore_cursor();
                }
            }
            _ => {}
        }
    }

    /// Process OSC (Operating System Command)
    fn process_osc(&mut self) {
        // Skip until ST (String Terminator) or BEL
        loop {
            let Some(byte) = self.output_buffer.pop_front() else { return };
            if byte == 0x07 || byte == 0x1B {
                if byte == 0x1B {
                    self.output_buffer.pop_front(); // Skip backslash
                }
                return;
            }
        }
    }

    /// Ring bell
    fn bell(&mut self) {
        if self.bell_enabled {
            // Would trigger audio bell here
        }
    }

    /// Reset terminal
    fn reset(&mut self) {
        self.screen = ScreenBuffer::new(self.size.cols as usize, self.size.rows as usize);
        self.attrs = TerminalAttrs::default();
        self.dirty = true;
    }

    /// Read input
    pub fn read(&mut self, buf: &mut [u8]) -> usize {
        let mut count = 0;
        while count < buf.len() {
            if let Some(byte) = self.input_buffer.pop_front() {
                buf[count] = byte;
                count += 1;
            } else {
                break;
            }
        }
        count
    }

    /// Receive input byte
    pub fn input(&mut self, byte: u8) {
        let cc = &self.attrs.cc;
        let mode = self.attrs.mode;

        // Check for signals
        if mode.contains(TerminalMode::ISIG) {
            if byte == cc.vintr {
                // Send SIGINT
                return;
            } else if byte == cc.vquit {
                // Send SIGQUIT
                return;
            } else if byte == cc.vsusp {
                // Send SIGTSTP
                return;
            }
        }

        if mode.contains(TerminalMode::ICANON) {
            // Canonical mode - line editing
            match byte {
                b if b == cc.verase => {
                    if !self.line_buffer.is_empty() {
                        self.line_buffer.pop();
                        if mode.contains(TerminalMode::ECHO) {
                            self.write(b"\x08 \x08"); // Backspace, space, backspace
                        }
                    }
                }
                b if b == cc.vkill => {
                    let len = self.line_buffer.len();
                    self.line_buffer.clear();
                    if mode.contains(TerminalMode::ECHO) {
                        for _ in 0..len {
                            self.write(b"\x08 \x08");
                        }
                    }
                }
                b if b == cc.veof => {
                    // EOF - make line available without newline
                    for &b in &self.line_buffer {
                        self.input_buffer.push_back(b);
                    }
                    self.line_buffer.clear();
                }
                b'\n' | b'\r' => {
                    self.line_buffer.push(b'\n');
                    for &b in &self.line_buffer {
                        self.input_buffer.push_back(b);
                    }
                    self.line_buffer.clear();
                    if mode.contains(TerminalMode::ECHO) {
                        self.write(b"\r\n");
                    }
                }
                _ => {
                    self.line_buffer.push(byte);
                    if mode.contains(TerminalMode::ECHO) {
                        self.write(&[byte]);
                    }
                }
            }
        } else {
            // Raw/cbreak mode - no line editing
            self.input_buffer.push_back(byte);
            if mode.contains(TerminalMode::ECHO) {
                self.write(&[byte]);
            }
        }

        self.dirty = true;
    }

    /// Resize terminal
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.size.cols = cols;
        self.size.rows = rows;
        self.size.xpixel = cols * 8;
        self.size.ypixel = rows * 16;
        self.screen.resize(cols as usize, rows as usize);
        self.dirty = true;
    }

    /// Set active
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
        self.dirty = true;
    }

    /// Is dirty
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Clear dirty flag
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// Get screen buffer for rendering
    pub fn screen(&self) -> &ScreenBuffer {
        &self.screen
    }

    /// Get terminal size
    pub fn size(&self) -> TerminalSize {
        self.size
    }

    /// Set terminal attributes
    pub fn set_attrs(&mut self, attrs: TerminalAttrs) {
        self.attrs = attrs;
    }

    /// Get terminal attributes
    pub fn attrs(&self) -> TerminalAttrs {
        self.attrs
    }
}

/// TTY subsystem
pub struct TtySubsystem {
    /// Virtual terminals
    terminals: Vec<VirtualTerminal>,
    /// Active terminal index
    active_index: usize,
    /// Console output framebuffer
    framebuffer: Option<Box<Framebuffer>>,
}

impl TtySubsystem {
    /// Create a new TTY subsystem
    pub fn new() -> Self {
        Self {
            terminals: Vec::new(),
            active_index: 0,
            framebuffer: None,
        }
    }

    /// Initialize with framebuffer
    pub fn init(&mut self, fb: Framebuffer) {
        self.framebuffer = Some(Box::new(fb));

        // Create default virtual terminals
        for i in 1..=6 {
            let name = alloc::format!("tty{}", i);
            self.terminals.push(VirtualTerminal::new(&name));
        }

        // Set first terminal active
        if let Some(first) = self.terminals.first_mut() {
            first.set_active(true);
        }
    }

    /// Get active terminal
    pub fn active_terminal(&self) -> Option<&VirtualTerminal> {
        self.terminals.get(self.active_index)
    }

    /// Get active terminal mutable
    pub fn active_terminal_mut(&mut self) -> Option<&mut VirtualTerminal> {
        self.terminals.get_mut(self.active_index)
    }

    /// Switch to terminal
    pub fn switch_to(&mut self, index: usize) -> bool {
        if index < self.terminals.len() && index != self.active_index {
            if let Some(current) = self.terminals.get_mut(self.active_index) {
                current.set_active(false);
            }
            self.active_index = index;
            if let Some(new) = self.terminals.get_mut(self.active_index) {
                new.set_active(true);
            }
            self.redraw();
            true
        } else {
            false
        }
    }

    /// Redraw active terminal
    pub fn redraw(&mut self) {
        let Some(ref mut fb) = self.framebuffer else { return };
        let Some(term) = self.terminals.get(self.active_index) else { return };

        let screen = term.screen();

        // Clear framebuffer
        fb.clear(Color::BLACK);

        // Draw each cell
        for y in 0..screen.height {
            for x in 0..screen.width {
                if let Some(cell) = screen.get(x, y) {
                    let px = x as u32 * VgaFont::WIDTH as u32;
                    let py = y as u32 * VgaFont::HEIGHT as u32;

                    // Draw background
                    let (fg, bg) = if cell.attrs.reverse {
                        (cell.attrs.bg_color(), cell.attrs.fg_color())
                    } else {
                        (cell.attrs.fg_color(), cell.attrs.bg_color())
                    };

                    fb.fill_rect(px, py, VgaFont::WIDTH as u32, VgaFont::HEIGHT as u32, bg);

                    // Draw character
                    if !cell.attrs.hidden && cell.ch != ' ' {
                        if let Some(bitmap) = VgaFont::get_bitmap(cell.ch) {
                            for row in 0..16 {
                                let bits = bitmap[row];
                                for col in 0..8 {
                                    if (bits >> (7 - col)) & 1 != 0 {
                                        fb.put_pixel(px + col, py + row as u32, fg);
                                    }
                                }
                            }
                        }
                    }

                    // Draw underline
                    if cell.attrs.underline {
                        fb.draw_hline(px, py + 15, VgaFont::WIDTH as u32, fg);
                    }

                    // Draw strikethrough
                    if cell.attrs.strikethrough {
                        fb.draw_hline(px, py + 8, VgaFont::WIDTH as u32, fg);
                    }
                }
            }
        }

        // Draw cursor
        if screen.cursor_visible {
            let cx = screen.cursor_x as u32 * VgaFont::WIDTH as u32;
            let cy = screen.cursor_y as u32 * VgaFont::HEIGHT as u32;
            fb.fill_rect(cx, cy + 14, VgaFont::WIDTH as u32, 2, Color::WHITE);
        }

        fb.flip();
    }

    /// Handle keyboard input
    pub fn keyboard_input(&mut self, byte: u8) {
        let needs_redraw = {
            if let Some(term) = self.terminals.get_mut(self.active_index) {
                term.input(byte);
                term.is_dirty()
            } else {
                false
            }
        };
        if needs_redraw {
            self.redraw();
            if let Some(term) = self.terminals.get_mut(self.active_index) {
                term.clear_dirty();
            }
        }
    }

    /// Get terminal by index
    pub fn get_terminal(&self, index: usize) -> Option<&VirtualTerminal> {
        self.terminals.get(index)
    }

    /// Get terminal by index mutable
    pub fn get_terminal_mut(&mut self, index: usize) -> Option<&mut VirtualTerminal> {
        self.terminals.get_mut(index)
    }

    /// Get number of terminals
    pub fn terminal_count(&self) -> usize {
        self.terminals.len()
    }
}

impl Default for TtySubsystem {
    fn default() -> Self {
        Self::new()
    }
}

/// Global TTY subsystem
static TTY_SUBSYSTEM: RwLock<Option<TtySubsystem>> = RwLock::new(None);

/// Initialize TTY subsystem
pub fn init(fb: Framebuffer) {
    let mut tty = TtySubsystem::new();
    tty.init(fb);

    let mut subsystem = TTY_SUBSYSTEM.write();
    *subsystem = Some(tty);
}

/// Get active terminal
pub fn active_terminal() -> Option<TtyId> {
    TTY_SUBSYSTEM.read().as_ref().and_then(|tty| {
        tty.active_terminal().map(|t| t.id)
    })
}

/// Switch terminal
pub fn switch_terminal(index: usize) -> bool {
    if let Some(ref mut tty) = *TTY_SUBSYSTEM.write() {
        tty.switch_to(index)
    } else {
        false
    }
}

/// Write to active terminal
pub fn write(data: &[u8]) {
    if let Some(ref mut tty) = *TTY_SUBSYSTEM.write() {
        let needs_redraw = {
            if let Some(term) = tty.active_terminal_mut() {
                term.write(data);
                term.is_dirty()
            } else {
                false
            }
        };
        if needs_redraw {
            tty.redraw();
            if let Some(term) = tty.active_terminal_mut() {
                term.clear_dirty();
            }
        }
    }
}

/// Write string to active terminal
pub fn write_str(s: &str) {
    write(s.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_buffer() {
        let mut screen = ScreenBuffer::new(80, 25);
        screen.put_char('A');
        assert_eq!(screen.get(0, 0).map(|c| c.ch), Some('A'));
        assert_eq!(screen.cursor_x, 1);
    }

    #[test]
    fn test_ansi_color() {
        assert_eq!(AnsiColor::Red.to_color(), Color::rgb(170, 0, 0));
        assert_eq!(AnsiColor::BrightRed.to_color(), Color::rgb(255, 85, 85));
    }

    #[test]
    fn test_terminal_mode() {
        let mode = TerminalMode::DEFAULT;
        assert!(mode.contains(TerminalMode::ECHO));
        assert!(mode.contains(TerminalMode::ICANON));
    }
}
