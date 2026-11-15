// ===============================================================================
// BUILDOS KERNEL - FRAMEBUFFER DRIVER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Framebuffer console driver for text output.

use core::fmt::{self, Write};
use spin::Mutex;

use crate::boot::FramebufferInfo;

// =============================================================================
// CONSTANTS
// =============================================================================

/// Font width in pixels
const FONT_WIDTH: u32 = 8;

/// Font height in pixels
const FONT_HEIGHT: u32 = 16;

/// Default foreground color (white)
const DEFAULT_FG: u32 = 0xFFFFFF;

/// Default background color (black)
const DEFAULT_BG: u32 = 0x000000;

// =============================================================================
// GLOBAL WRITER
// =============================================================================

/// Global framebuffer writer
pub static WRITER: Mutex<FramebufferWriter> = Mutex::new(FramebufferWriter::new());

/// Framebuffer console writer
pub struct FramebufferWriter {
    /// Framebuffer info
    info: Option<FramebufferInfo>,

    /// Current cursor X position (in characters)
    cursor_x: u32,

    /// Current cursor Y position (in characters)
    cursor_y: u32,

    /// Number of columns
    cols: u32,

    /// Number of rows
    rows: u32,

    /// Foreground color
    fg_color: u32,

    /// Background color
    bg_color: u32,
}

impl FramebufferWriter {
    /// Create a new writer (uninitialized)
    const fn new() -> Self {
        Self {
            info: None,
            cursor_x: 0,
            cursor_y: 0,
            cols: 0,
            rows: 0,
            fg_color: DEFAULT_FG,
            bg_color: DEFAULT_BG,
        }
    }

    /// Initialize with framebuffer info
    pub fn init(&mut self, info: &FramebufferInfo) {
        self.info = Some(*info);
        self.cols = info.width / FONT_WIDTH;
        self.rows = info.height / FONT_HEIGHT;
        self.cursor_x = 0;
        self.cursor_y = 0;

        // Clear screen
        self.clear();
    }

    /// Clear the screen
    pub fn clear(&mut self) {
        if let Some(ref info) = self.info {
            let fb = unsafe { info.as_slice_mut() };
            let bg = self.translate_color(self.bg_color);

            for pixel in fb.iter_mut() {
                *pixel = bg;
            }

            self.cursor_x = 0;
            self.cursor_y = 0;
        }
    }

    /// Write a character
    pub fn write_char(&mut self, c: char) {
        match c {
            '\n' => {
                self.cursor_x = 0;
                self.cursor_y += 1;
                if self.cursor_y >= self.rows {
                    self.scroll();
                }
            }
            '\r' => {
                self.cursor_x = 0;
            }
            '\t' => {
                let spaces = 4 - (self.cursor_x % 4);
                for _ in 0..spaces {
                    self.write_char(' ');
                }
            }
            _ => {
                self.draw_char(c, self.cursor_x, self.cursor_y);
                self.cursor_x += 1;
                if self.cursor_x >= self.cols {
                    self.cursor_x = 0;
                    self.cursor_y += 1;
                    if self.cursor_y >= self.rows {
                        self.scroll();
                    }
                }
            }
        }
    }

    /// Draw a character at position
    fn draw_char(&mut self, c: char, x: u32, y: u32) {
        let Some(ref info) = self.info else { return };

        let glyph = get_glyph(c);
        let fg = self.translate_color(self.fg_color);
        let bg = self.translate_color(self.bg_color);

        let px_x = x * FONT_WIDTH;
        let px_y = y * FONT_HEIGHT;

        let fb = unsafe { info.as_slice_mut() };

        for row in 0..FONT_HEIGHT {
            let glyph_row = glyph[row as usize];

            for col in 0..FONT_WIDTH {
                let pixel = if (glyph_row >> (7 - col)) & 1 != 0 {
                    fg
                } else {
                    bg
                };

                let offset = info.pixel_offset(px_x + col, px_y + row);
                if offset < fb.len() {
                    fb[offset] = pixel;
                }
            }
        }
    }

    /// Scroll the screen up by one line
    fn scroll(&mut self) {
        let Some(ref info) = self.info else { return };

        let fb = unsafe { info.as_slice_mut() };
        let line_size = (info.pitch / 4) * FONT_HEIGHT;
        let total_lines = self.rows;

        // Copy lines up
        for y in 0..(total_lines - 1) {
            let src_start = ((y + 1) * line_size) as usize;
            let dst_start = (y * line_size) as usize;
            let count = line_size as usize;

            // Manual copy (can't use copy_within due to overlap handling)
            for i in 0..count {
                fb[dst_start + i] = fb[src_start + i];
            }
        }

        // Clear last line
        let last_line_start = ((total_lines - 1) * line_size) as usize;
        let bg = self.translate_color(self.bg_color);

        for i in 0..(line_size as usize) {
            fb[last_line_start + i] = bg;
        }

        self.cursor_y = self.rows - 1;
    }

    /// Translate RGB color to framebuffer format
    fn translate_color(&self, rgb: u32) -> u32 {
        let Some(ref info) = self.info else { return rgb };

        let r = ((rgb >> 16) & 0xFF) as u8;
        let g = ((rgb >> 8) & 0xFF) as u8;
        let b = (rgb & 0xFF) as u8;

        info.pixel_format.rgb(r, g, b)
    }

    /// Set foreground color
    pub fn set_fg_color(&mut self, color: u32) {
        self.fg_color = color;
    }

    /// Set background color
    pub fn set_bg_color(&mut self, color: u32) {
        self.bg_color = color;
    }
}

impl Write for FramebufferWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            self.write_char(c);
        }
        Ok(())
    }
}

// =============================================================================
// PUBLIC INTERFACE
// =============================================================================

/// Initialize the framebuffer driver
///
/// # Safety
///
/// Must only be called once with valid framebuffer info.
pub unsafe fn init(info: &FramebufferInfo) {
    WRITER.lock().init(info);
}

// =============================================================================
// FONT DATA
// =============================================================================

/// Get font glyph for character (8x16 bitmap)
fn get_glyph(c: char) -> &'static [u8; 16] {
    let idx = c as usize;

    if idx < 128 {
        &FONT_8X16[idx]
    } else {
        &FONT_8X16[0] // Default to null char for non-ASCII
    }
}

/// 8x16 font bitmap (ASCII characters 0-127)
static FONT_8X16: [[u8; 16]; 128] = {
    let mut font = [[0u8; 16]; 128];

    // Space
    font[b' ' as usize] = [0; 16];

    // Basic ASCII printable characters (simplified subset)
    // '0'
    font[b'0' as usize] = [
        0x00, 0x00, 0x3C, 0x66, 0x66, 0x6E, 0x76, 0x66,
        0x66, 0x66, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    // '1'
    font[b'1' as usize] = [
        0x00, 0x00, 0x18, 0x38, 0x18, 0x18, 0x18, 0x18,
        0x18, 0x18, 0x7E, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    // 'A'
    font[b'A' as usize] = [
        0x00, 0x00, 0x18, 0x3C, 0x66, 0x66, 0x7E, 0x66,
        0x66, 0x66, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    // 'Q'
    font[b'Q' as usize] = [
        0x00, 0x00, 0x3C, 0x66, 0x66, 0x66, 0x66, 0x66,
        0x66, 0x36, 0x1C, 0x0C, 0x00, 0x00, 0x00, 0x00,
    ];

    // 'u'
    font[b'u' as usize] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x66, 0x66, 0x66,
        0x66, 0x66, 0x3E, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    // 'a'
    font[b'a' as usize] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x3C, 0x06, 0x3E,
        0x66, 0x66, 0x3E, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    // 'n'
    font[b'n' as usize] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x7C, 0x66, 0x66,
        0x66, 0x66, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    // 't'
    font[b't' as usize] = [
        0x00, 0x00, 0x08, 0x08, 0x3E, 0x08, 0x08, 0x08,
        0x08, 0x08, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    // 'O'
    font[b'O' as usize] = [
        0x00, 0x00, 0x3C, 0x66, 0x66, 0x66, 0x66, 0x66,
        0x66, 0x66, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    // 'S'
    font[b'S' as usize] = [
        0x00, 0x00, 0x3C, 0x66, 0x60, 0x30, 0x18, 0x0C,
        0x06, 0x66, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    // Additional common characters would go here...

    // '='
    font[b'=' as usize] = [
        0x00, 0x00, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x7E,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    // '-'
    font[b'-' as usize] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x7E, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    // '['
    font[b'[' as usize] = [
        0x00, 0x00, 0x1E, 0x18, 0x18, 0x18, 0x18, 0x18,
        0x18, 0x18, 0x1E, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    // ']'
    font[b']' as usize] = [
        0x00, 0x00, 0x78, 0x18, 0x18, 0x18, 0x18, 0x18,
        0x18, 0x18, 0x78, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    // ':'
    font[b':' as usize] = [
        0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00,
        0x18, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    font
};
