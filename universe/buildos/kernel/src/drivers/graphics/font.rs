//! Font Rendering for BuildOS Graphics Subsystem
//!
//! Provides bitmap font rendering, TrueType-style glyph support,
//! and text layout capabilities.

use super::{Color, Framebuffer};
use alloc::vec::Vec;
use alloc::string::String;
use alloc::boxed::Box;

/// Maximum number of loaded fonts
pub const MAX_FONTS: usize = 16;

/// Maximum glyphs in a bitmap font
pub const MAX_GLYPHS: usize = 256;

/// Font style flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontStyle {
    bits: u8,
}

impl FontStyle {
    pub const NORMAL: Self = Self { bits: 0 };
    pub const BOLD: Self = Self { bits: 1 << 0 };
    pub const ITALIC: Self = Self { bits: 1 << 1 };
    pub const UNDERLINE: Self = Self { bits: 1 << 2 };
    pub const STRIKETHROUGH: Self = Self { bits: 1 << 3 };

    pub fn is_bold(self) -> bool {
        (self.bits & Self::BOLD.bits) != 0
    }

    pub fn is_italic(self) -> bool {
        (self.bits & Self::ITALIC.bits) != 0
    }

    pub fn is_underline(self) -> bool {
        (self.bits & Self::UNDERLINE.bits) != 0
    }

    pub fn is_strikethrough(self) -> bool {
        (self.bits & Self::STRIKETHROUGH.bits) != 0
    }

    pub fn with_bold(mut self) -> Self {
        self.bits |= Self::BOLD.bits;
        self
    }

    pub fn with_italic(mut self) -> Self {
        self.bits |= Self::ITALIC.bits;
        self
    }
}

/// Text alignment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

/// Vertical alignment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalAlign {
    Top,
    Middle,
    Bottom,
}

/// A single glyph in a bitmap font
#[derive(Clone)]
pub struct BitmapGlyph {
    /// Character code
    pub code: char,
    /// Glyph width in pixels
    pub width: u8,
    /// Glyph height in pixels
    pub height: u8,
    /// X bearing (offset from cursor)
    pub bearing_x: i8,
    /// Y bearing (offset from baseline)
    pub bearing_y: i8,
    /// Advance width (cursor movement)
    pub advance: u8,
    /// Bitmap data (1 bit per pixel, row-major)
    pub bitmap: Vec<u8>,
}

impl BitmapGlyph {
    /// Create a new empty glyph
    pub fn new(code: char, width: u8, height: u8) -> Self {
        let bitmap_size = ((width as usize + 7) / 8) * height as usize;
        Self {
            code,
            width,
            height,
            bearing_x: 0,
            bearing_y: height as i8,
            advance: width,
            bitmap: alloc::vec![0u8; bitmap_size],
        }
    }

    /// Get pixel value at (x, y)
    pub fn get_pixel(&self, x: u32, y: u32) -> bool {
        if x >= self.width as u32 || y >= self.height as u32 {
            return false;
        }

        let row_bytes = (self.width as usize + 7) / 8;
        let byte_index = y as usize * row_bytes + (x as usize / 8);
        let bit_index = 7 - (x as usize % 8);

        if byte_index < self.bitmap.len() {
            (self.bitmap[byte_index] >> bit_index) & 1 != 0
        } else {
            false
        }
    }

    /// Set pixel value at (x, y)
    pub fn set_pixel(&mut self, x: u32, y: u32, value: bool) {
        if x >= self.width as u32 || y >= self.height as u32 {
            return;
        }

        let row_bytes = (self.width as usize + 7) / 8;
        let byte_index = y as usize * row_bytes + (x as usize / 8);
        let bit_index = 7 - (x as usize % 8);

        if byte_index < self.bitmap.len() {
            if value {
                self.bitmap[byte_index] |= 1 << bit_index;
            } else {
                self.bitmap[byte_index] &= !(1 << bit_index);
            }
        }
    }
}

/// A bitmap font containing fixed or variable-width glyphs
pub struct BitmapFont {
    /// Font name
    pub name: String,
    /// Font size (height in pixels)
    pub size: u8,
    /// Line height
    pub line_height: u8,
    /// Baseline offset from top
    pub baseline: u8,
    /// Is this a fixed-width font?
    pub monospace: bool,
    /// Average character width
    pub avg_width: u8,
    /// Font style
    pub style: FontStyle,
    /// Glyph table (indexed by character code for ASCII, or use glyph_map for Unicode)
    glyphs: [Option<BitmapGlyph>; MAX_GLYPHS],
    /// Number of glyphs
    glyph_count: usize,
}

impl BitmapFont {
    /// Create a new empty bitmap font
    pub fn new(name: &str, size: u8) -> Self {
        Self {
            name: String::from(name),
            size,
            line_height: size + 2,
            baseline: size,
            monospace: true,
            avg_width: size / 2,
            style: FontStyle::NORMAL,
            glyphs: core::array::from_fn(|_| None),
            glyph_count: 0,
        }
    }

    /// Add a glyph to the font
    pub fn add_glyph(&mut self, glyph: BitmapGlyph) {
        let code = glyph.code as usize;
        if code < MAX_GLYPHS {
            self.glyphs[code] = Some(glyph);
            self.glyph_count += 1;
        }
    }

    /// Get a glyph by character
    pub fn get_glyph(&self, c: char) -> Option<&BitmapGlyph> {
        let code = c as usize;
        if code < MAX_GLYPHS {
            self.glyphs[code].as_ref()
        } else {
            None
        }
    }

    /// Get glyph or fallback to '?'
    pub fn get_glyph_or_default(&self, c: char) -> Option<&BitmapGlyph> {
        self.get_glyph(c).or_else(|| self.get_glyph('?'))
    }

    /// Measure text width in pixels
    pub fn measure_text(&self, text: &str) -> u32 {
        text.chars()
            .filter_map(|c| self.get_glyph_or_default(c))
            .map(|g| g.advance as u32)
            .sum()
    }

    /// Measure text with maximum width (for word wrapping)
    pub fn measure_text_wrapped(&self, text: &str, max_width: u32) -> (u32, u32) {
        let mut width = 0u32;
        let mut max_line_width = 0u32;
        let mut lines = 1u32;

        for c in text.chars() {
            if c == '\n' {
                max_line_width = max_line_width.max(width);
                width = 0;
                lines += 1;
                continue;
            }

            if let Some(glyph) = self.get_glyph_or_default(c) {
                let new_width = width + glyph.advance as u32;
                if new_width > max_width && width > 0 {
                    max_line_width = max_line_width.max(width);
                    width = glyph.advance as u32;
                    lines += 1;
                } else {
                    width = new_width;
                }
            }
        }

        max_line_width = max_line_width.max(width);
        (max_line_width, lines * self.line_height as u32)
    }
}

/// Built-in 8x16 VGA font
pub struct VgaFont;

impl VgaFont {
    /// VGA font width
    pub const WIDTH: u8 = 8;
    /// VGA font height
    pub const HEIGHT: u8 = 16;

    /// Get VGA font bitmap for a character
    pub fn get_bitmap(c: char) -> Option<&'static [u8; 16]> {
        let code = c as usize;
        if code < 256 {
            Some(&VGA_FONT_DATA[code])
        } else {
            None
        }
    }

    /// Create a BitmapFont from VGA font data
    pub fn create_bitmap_font() -> BitmapFont {
        let mut font = BitmapFont::new("VGA", 16);
        font.monospace = true;
        font.avg_width = 8;
        font.line_height = 16;
        font.baseline = 14;

        for code in 0..=255u8 {
            let mut glyph = BitmapGlyph::new(code as char, 8, 16);
            glyph.advance = 8;
            glyph.bearing_y = 14;

            // Copy bitmap data
            for y in 0..16 {
                glyph.bitmap[y] = VGA_FONT_DATA[code as usize][y];
            }

            font.add_glyph(glyph);
        }

        font
    }
}

/// Font renderer for drawing text to a framebuffer
pub struct FontRenderer {
    /// Current font
    font: Option<Box<BitmapFont>>,
    /// Foreground color
    fg_color: Color,
    /// Background color (None for transparent)
    bg_color: Option<Color>,
    /// Current cursor X position
    cursor_x: u32,
    /// Current cursor Y position
    cursor_y: u32,
    /// Tab width in characters
    tab_width: u8,
}

impl FontRenderer {
    /// Create a new font renderer
    pub fn new() -> Self {
        Self {
            font: None,
            fg_color: Color::WHITE,
            bg_color: None,
            cursor_x: 0,
            cursor_y: 0,
            tab_width: 4,
        }
    }

    /// Set the current font
    pub fn set_font(&mut self, font: BitmapFont) {
        self.font = Some(Box::new(font));
    }

    /// Set foreground color
    pub fn set_fg_color(&mut self, color: Color) {
        self.fg_color = color;
    }

    /// Set background color
    pub fn set_bg_color(&mut self, color: Option<Color>) {
        self.bg_color = color;
    }

    /// Set cursor position
    pub fn set_cursor(&mut self, x: u32, y: u32) {
        self.cursor_x = x;
        self.cursor_y = y;
    }

    /// Get cursor position
    pub fn cursor(&self) -> (u32, u32) {
        (self.cursor_x, self.cursor_y)
    }

    /// Draw a single character using VGA font
    pub fn draw_char_vga(&self, fb: &mut Framebuffer, x: u32, y: u32, c: char) {
        if let Some(bitmap) = VgaFont::get_bitmap(c) {
            for row in 0..16 {
                let bits = bitmap[row];
                for col in 0..8 {
                    if (bits >> (7 - col)) & 1 != 0 {
                        fb.put_pixel(x + col, y + row as u32, self.fg_color);
                    } else if let Some(bg) = self.bg_color {
                        fb.put_pixel(x + col, y + row as u32, bg);
                    }
                }
            }
        }
    }

    /// Draw a character using current font
    pub fn draw_char(&self, fb: &mut Framebuffer, x: u32, y: u32, c: char) -> u32 {
        if let Some(font) = &self.font {
            if let Some(glyph) = font.get_glyph_or_default(c) {
                let draw_x = (x as i32 + glyph.bearing_x as i32).max(0) as u32;
                let draw_y = (y as i32 + font.baseline as i32 - glyph.bearing_y as i32).max(0) as u32;

                for gy in 0..glyph.height as u32 {
                    for gx in 0..glyph.width as u32 {
                        if glyph.get_pixel(gx, gy) {
                            fb.put_pixel(draw_x + gx, draw_y + gy, self.fg_color);
                        } else if let Some(bg) = self.bg_color {
                            fb.put_pixel(draw_x + gx, draw_y + gy, bg);
                        }
                    }
                }

                return glyph.advance as u32;
            }
        } else {
            // Fall back to VGA font
            self.draw_char_vga(fb, x, y, c);
            return VgaFont::WIDTH as u32;
        }
        0
    }

    /// Draw a string
    pub fn draw_string(&mut self, fb: &mut Framebuffer, text: &str) {
        let line_height = self.font.as_ref()
            .map(|f| f.line_height as u32)
            .unwrap_or(VgaFont::HEIGHT as u32);

        let char_width = self.font.as_ref()
            .map(|f| f.avg_width as u32)
            .unwrap_or(VgaFont::WIDTH as u32);

        for c in text.chars() {
            match c {
                '\n' => {
                    self.cursor_x = 0;
                    self.cursor_y += line_height;
                }
                '\r' => {
                    self.cursor_x = 0;
                }
                '\t' => {
                    let tab_stop = char_width * self.tab_width as u32;
                    self.cursor_x = ((self.cursor_x / tab_stop) + 1) * tab_stop;
                }
                _ => {
                    let advance = self.draw_char(fb, self.cursor_x, self.cursor_y, c);
                    self.cursor_x += advance;
                }
            }
        }
    }

    /// Draw text at specific position
    pub fn draw_text_at(&mut self, fb: &mut Framebuffer, x: u32, y: u32, text: &str) {
        self.cursor_x = x;
        self.cursor_y = y;
        self.draw_string(fb, text);
    }

    /// Draw text with alignment
    pub fn draw_text_aligned(
        &mut self,
        fb: &mut Framebuffer,
        x: u32,
        y: u32,
        width: u32,
        text: &str,
        align: TextAlign,
    ) {
        let text_width = if let Some(font) = &self.font {
            font.measure_text(text)
        } else {
            text.len() as u32 * VgaFont::WIDTH as u32
        };

        let start_x = match align {
            TextAlign::Left => x,
            TextAlign::Center => x + (width.saturating_sub(text_width)) / 2,
            TextAlign::Right => x + width.saturating_sub(text_width),
        };

        self.draw_text_at(fb, start_x, y, text);
    }

    /// Draw text with word wrapping
    pub fn draw_text_wrapped(
        &mut self,
        fb: &mut Framebuffer,
        x: u32,
        y: u32,
        max_width: u32,
        text: &str,
    ) {
        let line_height = self.font.as_ref()
            .map(|f| f.line_height as u32)
            .unwrap_or(VgaFont::HEIGHT as u32);

        self.cursor_x = x;
        self.cursor_y = y;
        let start_x = x;

        for c in text.chars() {
            if c == '\n' {
                self.cursor_x = start_x;
                self.cursor_y += line_height;
                continue;
            }

            let char_width = if let Some(font) = &self.font {
                font.get_glyph_or_default(c)
                    .map(|g| g.advance as u32)
                    .unwrap_or(0)
            } else {
                VgaFont::WIDTH as u32
            };

            if self.cursor_x + char_width > start_x + max_width {
                self.cursor_x = start_x;
                self.cursor_y += line_height;
            }

            let advance = self.draw_char(fb, self.cursor_x, self.cursor_y, c);
            self.cursor_x += advance;
        }
    }
}

impl Default for FontRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// PSF (PC Screen Font) parser
pub mod psf {
    use super::*;

    /// PSF1 magic bytes
    pub const PSF1_MAGIC: [u8; 2] = [0x36, 0x04];

    /// PSF2 magic bytes
    pub const PSF2_MAGIC: [u8; 4] = [0x72, 0xb5, 0x4a, 0x86];

    /// PSF1 header
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct Psf1Header {
        pub magic: [u8; 2],
        pub mode: u8,
        pub charsize: u8,
    }

    /// PSF2 header
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct Psf2Header {
        pub magic: [u8; 4],
        pub version: u32,
        pub header_size: u32,
        pub flags: u32,
        pub num_glyphs: u32,
        pub bytes_per_glyph: u32,
        pub height: u32,
        pub width: u32,
    }

    /// PSF1 modes
    pub const PSF1_MODE512: u8 = 0x01;
    pub const PSF1_MODEHASTAB: u8 = 0x02;
    pub const PSF1_MODEHASSEQ: u8 = 0x04;

    /// PSF2 flags
    pub const PSF2_HAS_UNICODE_TABLE: u32 = 0x01;

    /// Parse PSF1 font
    pub fn parse_psf1(data: &[u8]) -> Option<BitmapFont> {
        if data.len() < 4 || data[0..2] != PSF1_MAGIC {
            return None;
        }

        let mode = data[2];
        let charsize = data[3] as usize;
        let num_glyphs = if (mode & PSF1_MODE512) != 0 { 512 } else { 256 };

        let expected_size = 4 + num_glyphs * charsize;
        if data.len() < expected_size {
            return None;
        }

        let mut font = BitmapFont::new("PSF1", charsize as u8);
        font.monospace = true;
        font.avg_width = 8;
        font.line_height = charsize as u8;
        font.baseline = charsize as u8 - 2;

        for i in 0..num_glyphs.min(256) {
            let glyph_offset = 4 + i * charsize;
            let mut glyph = BitmapGlyph::new(i as u8 as char, 8, charsize as u8);
            glyph.advance = 8;
            glyph.bearing_y = charsize as i8 - 2;

            for row in 0..charsize {
                glyph.bitmap[row] = data[glyph_offset + row];
            }

            font.add_glyph(glyph);
        }

        Some(font)
    }

    /// Parse PSF2 font
    pub fn parse_psf2(data: &[u8]) -> Option<BitmapFont> {
        if data.len() < 32 || data[0..4] != PSF2_MAGIC {
            return None;
        }

        // Parse header (little-endian)
        let header_size = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
        let num_glyphs = u32::from_le_bytes([data[16], data[17], data[18], data[19]]) as usize;
        let bytes_per_glyph = u32::from_le_bytes([data[20], data[21], data[22], data[23]]) as usize;
        let height = u32::from_le_bytes([data[24], data[25], data[26], data[27]]) as u8;
        let width = u32::from_le_bytes([data[28], data[29], data[30], data[31]]) as u8;

        let expected_size = header_size + num_glyphs * bytes_per_glyph;
        if data.len() < expected_size {
            return None;
        }

        let mut font = BitmapFont::new("PSF2", height);
        font.monospace = true;
        font.avg_width = width;
        font.line_height = height + 2;
        font.baseline = height - 2;

        for i in 0..num_glyphs.min(256) {
            let glyph_offset = header_size + i * bytes_per_glyph;
            let mut glyph = BitmapGlyph::new(i as u8 as char, width, height);
            glyph.advance = width;
            glyph.bearing_y = height as i8 - 2;

            let row_bytes = (width as usize + 7) / 8;
            for row in 0..height as usize {
                for byte in 0..row_bytes {
                    if row * row_bytes + byte < bytes_per_glyph {
                        glyph.bitmap[row * row_bytes + byte] = data[glyph_offset + row * row_bytes + byte];
                    }
                }
            }

            font.add_glyph(glyph);
        }

        Some(font)
    }

    /// Parse PSF font (auto-detect version)
    pub fn parse_psf(data: &[u8]) -> Option<BitmapFont> {
        if data.len() >= 4 && data[0..4] == PSF2_MAGIC {
            parse_psf2(data)
        } else if data.len() >= 2 && data[0..2] == PSF1_MAGIC {
            parse_psf1(data)
        } else {
            None
        }
    }
}

/// Terminal text buffer for console output
pub struct TextBuffer {
    /// Buffer dimensions
    pub width: usize,
    pub height: usize,
    /// Character buffer
    chars: Vec<char>,
    /// Foreground color buffer
    fg_colors: Vec<Color>,
    /// Background color buffer
    bg_colors: Vec<Color>,
    /// Cursor position
    cursor_x: usize,
    cursor_y: usize,
    /// Current colors
    current_fg: Color,
    current_bg: Color,
    /// Scroll region
    scroll_top: usize,
    scroll_bottom: usize,
}

impl TextBuffer {
    /// Create a new text buffer
    pub fn new(width: usize, height: usize) -> Self {
        let size = width * height;
        Self {
            width,
            height,
            chars: alloc::vec![' '; size],
            fg_colors: alloc::vec![Color::WHITE; size],
            bg_colors: alloc::vec![Color::BLACK; size],
            cursor_x: 0,
            cursor_y: 0,
            current_fg: Color::WHITE,
            current_bg: Color::BLACK,
            scroll_top: 0,
            scroll_bottom: height,
        }
    }

    /// Get index for position
    fn index(&self, x: usize, y: usize) -> usize {
        y * self.width + x
    }

    /// Put a character at position
    pub fn put_char(&mut self, x: usize, y: usize, c: char) {
        if x < self.width && y < self.height {
            let idx = self.index(x, y);
            self.chars[idx] = c;
            self.fg_colors[idx] = self.current_fg;
            self.bg_colors[idx] = self.current_bg;
        }
    }

    /// Get character at position
    pub fn get_char(&self, x: usize, y: usize) -> Option<(char, Color, Color)> {
        if x < self.width && y < self.height {
            let idx = self.index(x, y);
            Some((self.chars[idx], self.fg_colors[idx], self.bg_colors[idx]))
        } else {
            None
        }
    }

    /// Write a character at cursor and advance
    pub fn write_char(&mut self, c: char) {
        match c {
            '\n' => {
                self.cursor_x = 0;
                self.cursor_y += 1;
                if self.cursor_y >= self.scroll_bottom {
                    self.scroll_up();
                    self.cursor_y = self.scroll_bottom - 1;
                }
            }
            '\r' => {
                self.cursor_x = 0;
            }
            '\t' => {
                let tab_stop = ((self.cursor_x / 8) + 1) * 8;
                self.cursor_x = tab_stop.min(self.width - 1);
            }
            '\x08' => {
                // Backspace
                if self.cursor_x > 0 {
                    self.cursor_x -= 1;
                }
            }
            _ => {
                self.put_char(self.cursor_x, self.cursor_y, c);
                self.cursor_x += 1;
                if self.cursor_x >= self.width {
                    self.cursor_x = 0;
                    self.cursor_y += 1;
                    if self.cursor_y >= self.scroll_bottom {
                        self.scroll_up();
                        self.cursor_y = self.scroll_bottom - 1;
                    }
                }
            }
        }
    }

    /// Write a string
    pub fn write_string(&mut self, s: &str) {
        for c in s.chars() {
            self.write_char(c);
        }
    }

    /// Scroll the buffer up one line
    pub fn scroll_up(&mut self) {
        for y in self.scroll_top..(self.scroll_bottom - 1) {
            for x in 0..self.width {
                let src = self.index(x, y + 1);
                let dst = self.index(x, y);
                self.chars[dst] = self.chars[src];
                self.fg_colors[dst] = self.fg_colors[src];
                self.bg_colors[dst] = self.bg_colors[src];
            }
        }

        // Clear last line
        let y = self.scroll_bottom - 1;
        for x in 0..self.width {
            let idx = self.index(x, y);
            self.chars[idx] = ' ';
            self.fg_colors[idx] = self.current_fg;
            self.bg_colors[idx] = self.current_bg;
        }
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        for i in 0..self.chars.len() {
            self.chars[i] = ' ';
            self.fg_colors[i] = self.current_fg;
            self.bg_colors[i] = self.current_bg;
        }
        self.cursor_x = 0;
        self.cursor_y = 0;
    }

    /// Set cursor position
    pub fn set_cursor(&mut self, x: usize, y: usize) {
        self.cursor_x = x.min(self.width - 1);
        self.cursor_y = y.min(self.height - 1);
    }

    /// Get cursor position
    pub fn cursor(&self) -> (usize, usize) {
        (self.cursor_x, self.cursor_y)
    }

    /// Set current colors
    pub fn set_colors(&mut self, fg: Color, bg: Color) {
        self.current_fg = fg;
        self.current_bg = bg;
    }

    /// Set scroll region
    pub fn set_scroll_region(&mut self, top: usize, bottom: usize) {
        self.scroll_top = top.min(self.height);
        self.scroll_bottom = bottom.min(self.height).max(self.scroll_top + 1);
    }

    /// Render to framebuffer
    pub fn render(&self, fb: &mut Framebuffer, _renderer: &FontRenderer) {
        let char_width = VgaFont::WIDTH as u32;
        let char_height = VgaFont::HEIGHT as u32;

        for y in 0..self.height {
            for x in 0..self.width {
                if let Some((c, fg, bg)) = self.get_char(x, y) {
                    let px = x as u32 * char_width;
                    let py = y as u32 * char_height;

                    // Draw background
                    fb.fill_rect(px, py, char_width, char_height, bg);

                    // Draw character
                    if let Some(bitmap) = VgaFont::get_bitmap(c) {
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
            }
        }
    }
}

// VGA font data (8x16 bitmap font)
// This is a subset - full implementation would include all 256 characters
static VGA_FONT_DATA: [[u8; 16]; 256] = {
    let mut data = [[0u8; 16]; 256];

    // Space (32)
    data[32] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];

    // ! (33)
    data[33] = [0x00, 0x00, 0x18, 0x3C, 0x3C, 0x3C, 0x18, 0x18,
                0x18, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00];

    // " (34)
    data[34] = [0x00, 0x66, 0x66, 0x66, 0x24, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];

    // # (35)
    data[35] = [0x00, 0x00, 0x00, 0x6C, 0x6C, 0xFE, 0x6C, 0x6C,
                0x6C, 0xFE, 0x6C, 0x6C, 0x00, 0x00, 0x00, 0x00];

    // 0-9 digits
    data[48] = [0x00, 0x00, 0x7C, 0xC6, 0xC6, 0xCE, 0xDE, 0xF6,
                0xE6, 0xC6, 0xC6, 0x7C, 0x00, 0x00, 0x00, 0x00]; // 0
    data[49] = [0x00, 0x00, 0x18, 0x38, 0x78, 0x18, 0x18, 0x18,
                0x18, 0x18, 0x18, 0x7E, 0x00, 0x00, 0x00, 0x00]; // 1
    data[50] = [0x00, 0x00, 0x7C, 0xC6, 0x06, 0x0C, 0x18, 0x30,
                0x60, 0xC0, 0xC6, 0xFE, 0x00, 0x00, 0x00, 0x00]; // 2
    data[51] = [0x00, 0x00, 0x7C, 0xC6, 0x06, 0x06, 0x3C, 0x06,
                0x06, 0x06, 0xC6, 0x7C, 0x00, 0x00, 0x00, 0x00]; // 3
    data[52] = [0x00, 0x00, 0x0C, 0x1C, 0x3C, 0x6C, 0xCC, 0xFE,
                0x0C, 0x0C, 0x0C, 0x1E, 0x00, 0x00, 0x00, 0x00]; // 4
    data[53] = [0x00, 0x00, 0xFE, 0xC0, 0xC0, 0xC0, 0xFC, 0x06,
                0x06, 0x06, 0xC6, 0x7C, 0x00, 0x00, 0x00, 0x00]; // 5
    data[54] = [0x00, 0x00, 0x38, 0x60, 0xC0, 0xC0, 0xFC, 0xC6,
                0xC6, 0xC6, 0xC6, 0x7C, 0x00, 0x00, 0x00, 0x00]; // 6
    data[55] = [0x00, 0x00, 0xFE, 0xC6, 0x06, 0x06, 0x0C, 0x18,
                0x30, 0x30, 0x30, 0x30, 0x00, 0x00, 0x00, 0x00]; // 7
    data[56] = [0x00, 0x00, 0x7C, 0xC6, 0xC6, 0xC6, 0x7C, 0xC6,
                0xC6, 0xC6, 0xC6, 0x7C, 0x00, 0x00, 0x00, 0x00]; // 8
    data[57] = [0x00, 0x00, 0x7C, 0xC6, 0xC6, 0xC6, 0x7E, 0x06,
                0x06, 0x06, 0x0C, 0x78, 0x00, 0x00, 0x00, 0x00]; // 9

    // A-Z uppercase
    data[65] = [0x00, 0x00, 0x10, 0x38, 0x6C, 0xC6, 0xC6, 0xFE,
                0xC6, 0xC6, 0xC6, 0xC6, 0x00, 0x00, 0x00, 0x00]; // A
    data[66] = [0x00, 0x00, 0xFC, 0x66, 0x66, 0x66, 0x7C, 0x66,
                0x66, 0x66, 0x66, 0xFC, 0x00, 0x00, 0x00, 0x00]; // B
    data[67] = [0x00, 0x00, 0x3C, 0x66, 0xC2, 0xC0, 0xC0, 0xC0,
                0xC0, 0xC2, 0x66, 0x3C, 0x00, 0x00, 0x00, 0x00]; // C
    data[68] = [0x00, 0x00, 0xF8, 0x6C, 0x66, 0x66, 0x66, 0x66,
                0x66, 0x66, 0x6C, 0xF8, 0x00, 0x00, 0x00, 0x00]; // D
    data[69] = [0x00, 0x00, 0xFE, 0x66, 0x62, 0x68, 0x78, 0x68,
                0x60, 0x62, 0x66, 0xFE, 0x00, 0x00, 0x00, 0x00]; // E
    data[70] = [0x00, 0x00, 0xFE, 0x66, 0x62, 0x68, 0x78, 0x68,
                0x60, 0x60, 0x60, 0xF0, 0x00, 0x00, 0x00, 0x00]; // F
    data[71] = [0x00, 0x00, 0x3C, 0x66, 0xC2, 0xC0, 0xC0, 0xDE,
                0xC6, 0xC6, 0x66, 0x3A, 0x00, 0x00, 0x00, 0x00]; // G
    data[72] = [0x00, 0x00, 0xC6, 0xC6, 0xC6, 0xC6, 0xFE, 0xC6,
                0xC6, 0xC6, 0xC6, 0xC6, 0x00, 0x00, 0x00, 0x00]; // H
    data[73] = [0x00, 0x00, 0x3C, 0x18, 0x18, 0x18, 0x18, 0x18,
                0x18, 0x18, 0x18, 0x3C, 0x00, 0x00, 0x00, 0x00]; // I
    data[74] = [0x00, 0x00, 0x1E, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C,
                0xCC, 0xCC, 0xCC, 0x78, 0x00, 0x00, 0x00, 0x00]; // J
    data[75] = [0x00, 0x00, 0xE6, 0x66, 0x6C, 0x6C, 0x78, 0x78,
                0x6C, 0x66, 0x66, 0xE6, 0x00, 0x00, 0x00, 0x00]; // K
    data[76] = [0x00, 0x00, 0xF0, 0x60, 0x60, 0x60, 0x60, 0x60,
                0x60, 0x62, 0x66, 0xFE, 0x00, 0x00, 0x00, 0x00]; // L
    data[77] = [0x00, 0x00, 0xC6, 0xEE, 0xFE, 0xFE, 0xD6, 0xC6,
                0xC6, 0xC6, 0xC6, 0xC6, 0x00, 0x00, 0x00, 0x00]; // M
    data[78] = [0x00, 0x00, 0xC6, 0xE6, 0xF6, 0xFE, 0xDE, 0xCE,
                0xC6, 0xC6, 0xC6, 0xC6, 0x00, 0x00, 0x00, 0x00]; // N
    data[79] = [0x00, 0x00, 0x7C, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6,
                0xC6, 0xC6, 0xC6, 0x7C, 0x00, 0x00, 0x00, 0x00]; // O
    data[80] = [0x00, 0x00, 0xFC, 0x66, 0x66, 0x66, 0x7C, 0x60,
                0x60, 0x60, 0x60, 0xF0, 0x00, 0x00, 0x00, 0x00]; // P
    data[81] = [0x00, 0x00, 0x7C, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6,
                0xC6, 0xD6, 0xDE, 0x7C, 0x0C, 0x0E, 0x00, 0x00]; // Q
    data[82] = [0x00, 0x00, 0xFC, 0x66, 0x66, 0x66, 0x7C, 0x6C,
                0x66, 0x66, 0x66, 0xE6, 0x00, 0x00, 0x00, 0x00]; // R
    data[83] = [0x00, 0x00, 0x7C, 0xC6, 0xC6, 0x60, 0x38, 0x0C,
                0x06, 0xC6, 0xC6, 0x7C, 0x00, 0x00, 0x00, 0x00]; // S
    data[84] = [0x00, 0x00, 0xFF, 0xDB, 0x99, 0x18, 0x18, 0x18,
                0x18, 0x18, 0x18, 0x3C, 0x00, 0x00, 0x00, 0x00]; // T
    data[85] = [0x00, 0x00, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6,
                0xC6, 0xC6, 0xC6, 0x7C, 0x00, 0x00, 0x00, 0x00]; // U
    data[86] = [0x00, 0x00, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6,
                0xC6, 0x6C, 0x38, 0x10, 0x00, 0x00, 0x00, 0x00]; // V
    data[87] = [0x00, 0x00, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0xD6,
                0xD6, 0xFE, 0x6C, 0x6C, 0x00, 0x00, 0x00, 0x00]; // W
    data[88] = [0x00, 0x00, 0xC6, 0xC6, 0x6C, 0x38, 0x38, 0x38,
                0x6C, 0xC6, 0xC6, 0xC6, 0x00, 0x00, 0x00, 0x00]; // X
    data[89] = [0x00, 0x00, 0xC3, 0xC3, 0x66, 0x3C, 0x18, 0x18,
                0x18, 0x18, 0x18, 0x3C, 0x00, 0x00, 0x00, 0x00]; // Y
    data[90] = [0x00, 0x00, 0xFE, 0xC6, 0x86, 0x0C, 0x18, 0x30,
                0x60, 0xC2, 0xC6, 0xFE, 0x00, 0x00, 0x00, 0x00]; // Z

    // a-z lowercase
    data[97] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x78, 0x0C, 0x7C,
                0xCC, 0xCC, 0xCC, 0x76, 0x00, 0x00, 0x00, 0x00]; // a
    data[98] = [0x00, 0x00, 0xE0, 0x60, 0x60, 0x78, 0x6C, 0x66,
                0x66, 0x66, 0x66, 0x7C, 0x00, 0x00, 0x00, 0x00]; // b
    data[99] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x7C, 0xC6, 0xC0,
                0xC0, 0xC0, 0xC6, 0x7C, 0x00, 0x00, 0x00, 0x00]; // c
    data[100] = [0x00, 0x00, 0x1C, 0x0C, 0x0C, 0x3C, 0x6C, 0xCC,
                 0xCC, 0xCC, 0xCC, 0x76, 0x00, 0x00, 0x00, 0x00]; // d
    data[101] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x7C, 0xC6, 0xFE,
                 0xC0, 0xC0, 0xC6, 0x7C, 0x00, 0x00, 0x00, 0x00]; // e
    data[102] = [0x00, 0x00, 0x38, 0x6C, 0x64, 0x60, 0xF0, 0x60,
                 0x60, 0x60, 0x60, 0xF0, 0x00, 0x00, 0x00, 0x00]; // f
    data[103] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x76, 0xCC, 0xCC,
                 0xCC, 0xCC, 0xCC, 0x7C, 0x0C, 0xCC, 0x78, 0x00]; // g
    data[104] = [0x00, 0x00, 0xE0, 0x60, 0x60, 0x6C, 0x76, 0x66,
                 0x66, 0x66, 0x66, 0xE6, 0x00, 0x00, 0x00, 0x00]; // h
    data[105] = [0x00, 0x00, 0x18, 0x18, 0x00, 0x38, 0x18, 0x18,
                 0x18, 0x18, 0x18, 0x3C, 0x00, 0x00, 0x00, 0x00]; // i
    data[106] = [0x00, 0x00, 0x06, 0x06, 0x00, 0x0E, 0x06, 0x06,
                 0x06, 0x06, 0x06, 0x06, 0x66, 0x66, 0x3C, 0x00]; // j
    data[107] = [0x00, 0x00, 0xE0, 0x60, 0x60, 0x66, 0x6C, 0x78,
                 0x78, 0x6C, 0x66, 0xE6, 0x00, 0x00, 0x00, 0x00]; // k
    data[108] = [0x00, 0x00, 0x38, 0x18, 0x18, 0x18, 0x18, 0x18,
                 0x18, 0x18, 0x18, 0x3C, 0x00, 0x00, 0x00, 0x00]; // l
    data[109] = [0x00, 0x00, 0x00, 0x00, 0x00, 0xE6, 0xFF, 0xDB,
                 0xDB, 0xDB, 0xDB, 0xDB, 0x00, 0x00, 0x00, 0x00]; // m
    data[110] = [0x00, 0x00, 0x00, 0x00, 0x00, 0xDC, 0x66, 0x66,
                 0x66, 0x66, 0x66, 0x66, 0x00, 0x00, 0x00, 0x00]; // n
    data[111] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x7C, 0xC6, 0xC6,
                 0xC6, 0xC6, 0xC6, 0x7C, 0x00, 0x00, 0x00, 0x00]; // o
    data[112] = [0x00, 0x00, 0x00, 0x00, 0x00, 0xDC, 0x66, 0x66,
                 0x66, 0x66, 0x66, 0x7C, 0x60, 0x60, 0xF0, 0x00]; // p
    data[113] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x76, 0xCC, 0xCC,
                 0xCC, 0xCC, 0xCC, 0x7C, 0x0C, 0x0C, 0x1E, 0x00]; // q
    data[114] = [0x00, 0x00, 0x00, 0x00, 0x00, 0xDC, 0x76, 0x66,
                 0x60, 0x60, 0x60, 0xF0, 0x00, 0x00, 0x00, 0x00]; // r
    data[115] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x7C, 0xC6, 0x60,
                 0x38, 0x0C, 0xC6, 0x7C, 0x00, 0x00, 0x00, 0x00]; // s
    data[116] = [0x00, 0x00, 0x10, 0x30, 0x30, 0xFC, 0x30, 0x30,
                 0x30, 0x30, 0x36, 0x1C, 0x00, 0x00, 0x00, 0x00]; // t
    data[117] = [0x00, 0x00, 0x00, 0x00, 0x00, 0xCC, 0xCC, 0xCC,
                 0xCC, 0xCC, 0xCC, 0x76, 0x00, 0x00, 0x00, 0x00]; // u
    data[118] = [0x00, 0x00, 0x00, 0x00, 0x00, 0xC3, 0xC3, 0xC3,
                 0xC3, 0x66, 0x3C, 0x18, 0x00, 0x00, 0x00, 0x00]; // v
    data[119] = [0x00, 0x00, 0x00, 0x00, 0x00, 0xC6, 0xC6, 0xC6,
                 0xD6, 0xD6, 0xFE, 0x6C, 0x00, 0x00, 0x00, 0x00]; // w
    data[120] = [0x00, 0x00, 0x00, 0x00, 0x00, 0xC6, 0x6C, 0x38,
                 0x38, 0x38, 0x6C, 0xC6, 0x00, 0x00, 0x00, 0x00]; // x
    data[121] = [0x00, 0x00, 0x00, 0x00, 0x00, 0xC6, 0xC6, 0xC6,
                 0xC6, 0xC6, 0xC6, 0x7E, 0x06, 0x0C, 0xF8, 0x00]; // y
    data[122] = [0x00, 0x00, 0x00, 0x00, 0x00, 0xFE, 0xCC, 0x18,
                 0x30, 0x60, 0xC6, 0xFE, 0x00, 0x00, 0x00, 0x00]; // z

    // Common punctuation
    data[46] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00]; // .
    data[44] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x18, 0x18, 0x08, 0x10, 0x00, 0x00]; // ,
    data[58] = [0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00,
                0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00]; // :
    data[59] = [0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00,
                0x00, 0x18, 0x18, 0x08, 0x10, 0x00, 0x00, 0x00]; // ;
    data[63] = [0x00, 0x00, 0x7C, 0xC6, 0xC6, 0x0C, 0x18, 0x18,
                0x18, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00]; // ?
    data[45] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFE,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]; // -
    data[95] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00]; // _
    data[47] = [0x00, 0x00, 0x00, 0x00, 0x02, 0x06, 0x0C, 0x18,
                0x30, 0x60, 0xC0, 0x80, 0x00, 0x00, 0x00, 0x00]; // /
    data[92] = [0x00, 0x00, 0x00, 0x00, 0x80, 0xC0, 0x60, 0x30,
                0x18, 0x0C, 0x06, 0x02, 0x00, 0x00, 0x00, 0x00]; // \
    data[40] = [0x00, 0x00, 0x0C, 0x18, 0x30, 0x30, 0x30, 0x30,
                0x30, 0x30, 0x18, 0x0C, 0x00, 0x00, 0x00, 0x00]; // (
    data[41] = [0x00, 0x00, 0x30, 0x18, 0x0C, 0x0C, 0x0C, 0x0C,
                0x0C, 0x0C, 0x18, 0x30, 0x00, 0x00, 0x00, 0x00]; // )
    data[91] = [0x00, 0x00, 0x3C, 0x30, 0x30, 0x30, 0x30, 0x30,
                0x30, 0x30, 0x30, 0x3C, 0x00, 0x00, 0x00, 0x00]; // [
    data[93] = [0x00, 0x00, 0x3C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C,
                0x0C, 0x0C, 0x0C, 0x3C, 0x00, 0x00, 0x00, 0x00]; // ]
    data[123] = [0x00, 0x00, 0x0E, 0x18, 0x18, 0x18, 0x70, 0x18,
                 0x18, 0x18, 0x18, 0x0E, 0x00, 0x00, 0x00, 0x00]; // {
    data[125] = [0x00, 0x00, 0x70, 0x18, 0x18, 0x18, 0x0E, 0x18,
                 0x18, 0x18, 0x18, 0x70, 0x00, 0x00, 0x00, 0x00]; // }
    data[60] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x0C, 0x18,
                0x30, 0x18, 0x0C, 0x06, 0x00, 0x00, 0x00, 0x00]; // <
    data[62] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x60, 0x30, 0x18,
                0x0C, 0x18, 0x30, 0x60, 0x00, 0x00, 0x00, 0x00]; // >
    data[61] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x7E, 0x00,
                0x00, 0x7E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]; // =
    data[43] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x7E,
                0x18, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]; // +
    data[42] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x66, 0x3C, 0xFF,
                0x3C, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]; // *
    data[38] = [0x00, 0x00, 0x38, 0x6C, 0x6C, 0x38, 0x76, 0xDC,
                0xCC, 0xCC, 0xCC, 0x76, 0x00, 0x00, 0x00, 0x00]; // &
    data[64] = [0x00, 0x00, 0x00, 0x7C, 0xC6, 0xC6, 0xDE, 0xDE,
                0xDE, 0xDC, 0xC0, 0x7C, 0x00, 0x00, 0x00, 0x00]; // @
    data[124] = [0x00, 0x00, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18,
                 0x18, 0x18, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00]; // |
    data[39] = [0x00, 0x0C, 0x0C, 0x0C, 0x18, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]; // '
    data[96] = [0x00, 0x30, 0x18, 0x0C, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]; // `
    data[126] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x76, 0xDC,
                 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]; // ~
    data[37] = [0x00, 0x00, 0x00, 0x00, 0xC2, 0xC6, 0x0C, 0x18,
                0x30, 0x60, 0xC6, 0x86, 0x00, 0x00, 0x00, 0x00]; // %
    data[36] = [0x00, 0x10, 0x7C, 0xD6, 0xD0, 0xD0, 0x7C, 0x16,
                0x16, 0xD6, 0x7C, 0x10, 0x10, 0x00, 0x00, 0x00]; // $
    data[94] = [0x10, 0x38, 0x6C, 0xC6, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]; // ^

    data
};

/// Draw a string at the specified position using the VGA font
/// This is a convenience function for simple text rendering
pub fn draw_string(fb: &mut Framebuffer, x: u32, y: u32, text: &str, fg: Color, bg: Color) {
    let mut cursor_x = x;
    let cursor_y = y;

    for c in text.chars() {
        match c {
            '\n' | '\r' => {
                // Skip newlines in simple draw_string - use FontRenderer for advanced text
            }
            _ => {
                if let Some(bitmap) = VgaFont::get_bitmap(c) {
                    for row in 0..16 {
                        let bits = bitmap[row];
                        for col in 0..8 {
                            let px = cursor_x + col;
                            let py = cursor_y + row as u32;
                            if (bits >> (7 - col)) & 1 != 0 {
                                fb.put_pixel(px, py, fg);
                            } else {
                                fb.put_pixel(px, py, bg);
                            }
                        }
                    }
                }
                cursor_x += VgaFont::WIDTH as u32;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitmap_glyph_pixel() {
        let mut glyph = BitmapGlyph::new('A', 8, 8);
        glyph.set_pixel(0, 0, true);
        assert!(glyph.get_pixel(0, 0));
        assert!(!glyph.get_pixel(1, 0));
    }

    #[test]
    fn test_font_measure() {
        let font = VgaFont::create_bitmap_font();
        let width = font.measure_text("Hello");
        assert_eq!(width, 5 * 8); // 5 chars * 8 pixels
    }

    #[test]
    fn test_text_buffer() {
        let mut buf = TextBuffer::new(80, 25);
        buf.write_string("Hello\nWorld");
        assert_eq!(buf.get_char(0, 0).map(|(c, _, _)| c), Some('H'));
        assert_eq!(buf.get_char(0, 1).map(|(c, _, _)| c), Some('W'));
    }
}
