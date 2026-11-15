//! BuildOS Console Driver
//!
//! Low-level console output for early boot and kernel messages.

use super::AnsiColor;
use crate::sync::Mutex;
use core::fmt::{self, Write};

/// Early boot console (before framebuffer is available)
pub struct EarlyConsole {
    /// VGA text mode buffer address
    vga_buffer: *mut u8,
    /// Current position
    x: usize,
    y: usize,
    /// Screen dimensions
    width: usize,
    height: usize,
    /// Current attribute
    attr: u8,
}

impl EarlyConsole {
    /// VGA text mode buffer address
    const VGA_BUFFER: usize = 0xB8000;
    /// Default width
    const WIDTH: usize = 80;
    /// Default height
    const HEIGHT: usize = 25;

    /// Create a new early console
    pub const fn new() -> Self {
        Self {
            vga_buffer: Self::VGA_BUFFER as *mut u8,
            x: 0,
            y: 0,
            width: Self::WIDTH,
            height: Self::HEIGHT,
            attr: 0x07, // Light gray on black
        }
    }

    /// Initialize VGA text mode
    pub fn init(&mut self) {
        self.clear();
    }

    /// Clear screen
    pub fn clear(&mut self) {
        for i in 0..(self.width * self.height) {
            unsafe {
                *self.vga_buffer.add(i * 2) = b' ';
                *self.vga_buffer.add(i * 2 + 1) = self.attr;
            }
        }
        self.x = 0;
        self.y = 0;
    }

    /// Scroll up one line
    fn scroll(&mut self) {
        // Move lines up
        for y in 0..(self.height - 1) {
            for x in 0..self.width {
                unsafe {
                    let src = ((y + 1) * self.width + x) * 2;
                    let dst = (y * self.width + x) * 2;
                    *self.vga_buffer.add(dst) = *self.vga_buffer.add(src);
                    *self.vga_buffer.add(dst + 1) = *self.vga_buffer.add(src + 1);
                }
            }
        }

        // Clear last line
        let y = self.height - 1;
        for x in 0..self.width {
            unsafe {
                let offset = (y * self.width + x) * 2;
                *self.vga_buffer.add(offset) = b' ';
                *self.vga_buffer.add(offset + 1) = self.attr;
            }
        }
    }

    /// Put character at current position
    pub fn put_char(&mut self, c: char) {
        match c {
            '\n' => {
                self.x = 0;
                self.y += 1;
            }
            '\r' => {
                self.x = 0;
            }
            '\t' => {
                self.x = (self.x + 8) & !7;
            }
            _ => {
                if self.x < self.width && self.y < self.height {
                    unsafe {
                        let offset = (self.y * self.width + self.x) * 2;
                        *self.vga_buffer.add(offset) = c as u8;
                        *self.vga_buffer.add(offset + 1) = self.attr;
                    }
                    self.x += 1;
                }
            }
        }

        // Handle wrapping
        if self.x >= self.width {
            self.x = 0;
            self.y += 1;
        }

        // Handle scrolling
        if self.y >= self.height {
            self.scroll();
            self.y = self.height - 1;
        }
    }

    /// Write string
    pub fn write_str(&mut self, s: &str) {
        for c in s.chars() {
            self.put_char(c);
        }
    }

    /// Set foreground color
    pub fn set_fg(&mut self, color: AnsiColor) {
        self.attr = (self.attr & 0xF0) | (color as u8);
    }

    /// Set background color
    pub fn set_bg(&mut self, color: AnsiColor) {
        self.attr = (self.attr & 0x0F) | ((color as u8) << 4);
    }

    /// Move cursor
    pub fn move_cursor(&mut self, x: usize, y: usize) {
        self.x = x.min(self.width - 1);
        self.y = y.min(self.height - 1);
    }

    /// Update hardware cursor position
    pub fn update_cursor(&self) {
        let pos = self.y * self.width + self.x;
        unsafe {
            // CRT Controller Index Register
            core::arch::asm!(
                "out dx, al",
                in("dx") 0x3D4u16,
                in("al") 0x0Fu8,
            );
            // CRT Controller Data Register - cursor low byte
            core::arch::asm!(
                "out dx, al",
                in("dx") 0x3D5u16,
                in("al") (pos & 0xFF) as u8,
            );
            // CRT Controller Index Register
            core::arch::asm!(
                "out dx, al",
                in("dx") 0x3D4u16,
                in("al") 0x0Eu8,
            );
            // CRT Controller Data Register - cursor high byte
            core::arch::asm!(
                "out dx, al",
                in("dx") 0x3D5u16,
                in("al") ((pos >> 8) & 0xFF) as u8,
            );
        }
    }
}

impl Write for EarlyConsole {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_str(s);
        Ok(())
    }
}

unsafe impl Send for EarlyConsole {}
unsafe impl Sync for EarlyConsole {}

/// Kernel console output
pub struct KernelConsole {
    /// Virtual terminal for output
    terminal: Option<usize>, // Index into TTY subsystem
    /// Log level
    log_level: LogLevel,
    /// Buffer for early messages
    early_buffer: [u8; 4096],
    /// Buffer position
    buffer_pos: usize,
    /// Is initialized
    initialized: bool,
}

/// Log levels
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Emergency = 0,
    Alert = 1,
    Critical = 2,
    Error = 3,
    Warning = 4,
    Notice = 5,
    Info = 6,
    Debug = 7,
}

impl LogLevel {
    pub fn color(&self) -> AnsiColor {
        match self {
            Self::Emergency | Self::Alert | Self::Critical => AnsiColor::BrightRed,
            Self::Error => AnsiColor::Red,
            Self::Warning => AnsiColor::Yellow,
            Self::Notice => AnsiColor::BrightBlue,
            Self::Info => AnsiColor::White,
            Self::Debug => AnsiColor::BrightBlack,
        }
    }

    pub fn prefix(&self) -> &'static str {
        match self {
            Self::Emergency => "[EMERG]",
            Self::Alert => "[ALERT]",
            Self::Critical => "[CRIT]",
            Self::Error => "[ERROR]",
            Self::Warning => "[WARN]",
            Self::Notice => "[NOTICE]",
            Self::Info => "[INFO]",
            Self::Debug => "[DEBUG]",
        }
    }
}

impl KernelConsole {
    pub const fn new() -> Self {
        Self {
            terminal: None,
            log_level: LogLevel::Info,
            early_buffer: [0; 4096],
            buffer_pos: 0,
            initialized: false,
        }
    }

    /// Initialize with terminal
    pub fn init(&mut self, terminal_index: usize) {
        self.terminal = Some(terminal_index);
        self.initialized = true;

        // Flush early buffer to terminal
        if self.buffer_pos > 0 {
            // Would write early_buffer[..buffer_pos] to terminal
            self.buffer_pos = 0;
        }
    }

    /// Set log level
    pub fn set_log_level(&mut self, level: LogLevel) {
        self.log_level = level;
    }

    /// Write log message
    pub fn log(&mut self, level: LogLevel, message: &str) {
        if level > self.log_level {
            return;
        }

        // Format: [LEVEL] message\n
        let color_code = match level {
            LogLevel::Emergency | LogLevel::Alert | LogLevel::Critical => "\x1b[1;31m",
            LogLevel::Error => "\x1b[31m",
            LogLevel::Warning => "\x1b[33m",
            LogLevel::Notice => "\x1b[1;34m",
            LogLevel::Info => "\x1b[0m",
            LogLevel::Debug => "\x1b[90m",
        };

        if self.initialized {
            // Write to terminal
            super::write_str(color_code);
            super::write_str(level.prefix());
            super::write_str(" ");
            super::write_str(message);
            super::write_str("\x1b[0m\n");
        } else {
            // Buffer for later
            let prefix = level.prefix();
            for &b in prefix.as_bytes() {
                if self.buffer_pos < self.early_buffer.len() {
                    self.early_buffer[self.buffer_pos] = b;
                    self.buffer_pos += 1;
                }
            }
            self.early_buffer[self.buffer_pos] = b' ';
            self.buffer_pos += 1;
            for &b in message.as_bytes() {
                if self.buffer_pos < self.early_buffer.len() {
                    self.early_buffer[self.buffer_pos] = b;
                    self.buffer_pos += 1;
                }
            }
            self.early_buffer[self.buffer_pos] = b'\n';
            self.buffer_pos += 1;
        }
    }
}

/// Global early console
static EARLY_CONSOLE: Mutex<EarlyConsole> = Mutex::new(EarlyConsole::new());

/// Global kernel console
static KERNEL_CONSOLE: Mutex<KernelConsole> = Mutex::new(KernelConsole::new());

/// Initialize early console
pub fn init_early() {
    EARLY_CONSOLE.lock().init();
}

/// Print to early console
pub fn early_print(s: &str) {
    EARLY_CONSOLE.lock().write_str(s);
}

/// Print formatted to early console
#[macro_export]
macro_rules! early_print {
    ($($arg:tt)*) => {
        {
            use core::fmt::Write;
            let _ = write!($crate::tty::console::EARLY_CONSOLE.lock(), $($arg)*);
        }
    };
}

/// Print line to early console
#[macro_export]
macro_rules! early_println {
    () => { $crate::early_print!("\n") };
    ($($arg:tt)*) => {
        $crate::early_print!("{}\n", format_args!($($arg)*))
    };
}

/// Initialize kernel console
pub fn init_kernel(terminal_index: usize) {
    KERNEL_CONSOLE.lock().init(terminal_index);
}

/// Log message
pub fn klog(level: LogLevel, message: &str) {
    KERNEL_CONSOLE.lock().log(level, message);
}

// Note: klog_debug!, klog_info!, klog_warn!, klog_error! macros are defined in logging/mod.rs
