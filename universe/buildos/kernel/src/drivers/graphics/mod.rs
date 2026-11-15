// ===============================================================================
// BUILDOS KERNEL - GRAPHICS SUBSYSTEM
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! Graphics and Display Driver Framework.
//!
//! This module provides:
//! - Framebuffer abstraction
//! - VESA/VBE mode setting
//! - GOP (UEFI Graphics Output Protocol) support
//! - 2D graphics primitives
//! - Font rendering
//! - Double buffering

pub mod vesa;
pub mod font;

use alloc::boxed::Box;
use alloc::vec::Vec;
use spin::Mutex;
use core::sync::atomic::{AtomicBool, Ordering};

// =============================================================================
// CONSTANTS
// =============================================================================

/// Maximum supported display width
pub const MAX_WIDTH: u32 = 4096;

/// Maximum supported display height
pub const MAX_HEIGHT: u32 = 4096;

/// Default display width
pub const DEFAULT_WIDTH: u32 = 1024;

/// Default display height
pub const DEFAULT_HEIGHT: u32 = 768;

/// Default bits per pixel
pub const DEFAULT_BPP: u8 = 32;

// =============================================================================
// COLOR TYPES
// =============================================================================

/// 32-bit ARGB color
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct Color {
    pub b: u8,
    pub g: u8,
    pub r: u8,
    pub a: u8,
}

impl Color {
    /// Create a new color from RGB values
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Create a new color from RGBA values
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Create from a 32-bit packed value (0xAARRGGBB)
    pub const fn from_u32(value: u32) -> Self {
        Self {
            a: ((value >> 24) & 0xFF) as u8,
            r: ((value >> 16) & 0xFF) as u8,
            g: ((value >> 8) & 0xFF) as u8,
            b: (value & 0xFF) as u8,
        }
    }

    /// Convert to 32-bit packed value
    pub const fn to_u32(self) -> u32 {
        ((self.a as u32) << 24)
            | ((self.r as u32) << 16)
            | ((self.g as u32) << 8)
            | (self.b as u32)
    }

    /// Convert to 16-bit RGB565
    pub const fn to_rgb565(self) -> u16 {
        ((self.r as u16 >> 3) << 11)
            | ((self.g as u16 >> 2) << 5)
            | (self.b as u16 >> 3)
    }

    /// Blend with another color using alpha
    pub fn blend(self, other: Color) -> Color {
        let alpha = other.a as u32;
        let inv_alpha = 255 - alpha;

        Color {
            r: ((self.r as u32 * inv_alpha + other.r as u32 * alpha) / 255) as u8,
            g: ((self.g as u32 * inv_alpha + other.g as u32 * alpha) / 255) as u8,
            b: ((self.b as u32 * inv_alpha + other.b as u32 * alpha) / 255) as u8,
            a: 255,
        }
    }

    /// Predefined colors
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const WHITE: Color = Color::rgb(255, 255, 255);
    pub const RED: Color = Color::rgb(255, 0, 0);
    pub const GREEN: Color = Color::rgb(0, 255, 0);
    pub const BLUE: Color = Color::rgb(0, 0, 255);
    pub const YELLOW: Color = Color::rgb(255, 255, 0);
    pub const CYAN: Color = Color::rgb(0, 255, 255);
    pub const MAGENTA: Color = Color::rgb(255, 0, 255);
    pub const GRAY: Color = Color::rgb(128, 128, 128);
    pub const DARK_GRAY: Color = Color::rgb(64, 64, 64);
    pub const LIGHT_GRAY: Color = Color::rgb(192, 192, 192);
    pub const TRANSPARENT: Color = Color::rgba(0, 0, 0, 0);
}

// =============================================================================
// PIXEL FORMAT
// =============================================================================

/// Pixel format enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// 32-bit ARGB (8 bits per channel)
    Argb32,
    /// 32-bit RGBA (8 bits per channel)
    Rgba32,
    /// 32-bit BGRA (8 bits per channel)
    Bgra32,
    /// 24-bit RGB (8 bits per channel)
    Rgb24,
    /// 24-bit BGR (8 bits per channel)
    Bgr24,
    /// 16-bit RGB565
    Rgb565,
    /// 8-bit indexed color
    Indexed8,
}

impl PixelFormat {
    /// Get bytes per pixel
    pub const fn bytes_per_pixel(&self) -> usize {
        match self {
            PixelFormat::Argb32 | PixelFormat::Rgba32 | PixelFormat::Bgra32 => 4,
            PixelFormat::Rgb24 | PixelFormat::Bgr24 => 3,
            PixelFormat::Rgb565 => 2,
            PixelFormat::Indexed8 => 1,
        }
    }

    /// Get bits per pixel
    pub const fn bits_per_pixel(&self) -> u8 {
        (self.bytes_per_pixel() * 8) as u8
    }
}

// =============================================================================
// DISPLAY MODE
// =============================================================================

/// Display mode information
#[derive(Debug, Clone, Copy)]
pub struct DisplayMode {
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Bits per pixel
    pub bpp: u8,
    /// Pixel format
    pub format: PixelFormat,
    /// Bytes per scanline (pitch)
    pub pitch: u32,
    /// Refresh rate in Hz (0 if unknown)
    pub refresh_rate: u8,
}

impl DisplayMode {
    /// Calculate total framebuffer size
    pub const fn framebuffer_size(&self) -> usize {
        (self.pitch as usize) * (self.height as usize)
    }
}

// =============================================================================
// FRAMEBUFFER
// =============================================================================

/// Framebuffer abstraction
pub struct Framebuffer {
    /// Physical address of framebuffer
    phys_addr: u64,
    /// Virtual address of framebuffer
    virt_addr: u64,
    /// Display mode
    mode: DisplayMode,
    /// Back buffer for double buffering
    back_buffer: Option<Box<[u8]>>,
    /// Double buffering enabled
    double_buffered: bool,
    /// Dirty region tracking
    dirty: DirtyRegion,
}

/// Dirty region for partial updates
#[derive(Debug, Clone, Copy)]
struct DirtyRegion {
    x1: u32,
    y1: u32,
    x2: u32,
    y2: u32,
    dirty: bool,
}

impl DirtyRegion {
    const fn new() -> Self {
        Self {
            x1: u32::MAX,
            y1: u32::MAX,
            x2: 0,
            y2: 0,
            dirty: false,
        }
    }

    fn mark(&mut self, x: u32, y: u32) {
        self.x1 = self.x1.min(x);
        self.y1 = self.y1.min(y);
        self.x2 = self.x2.max(x + 1);
        self.y2 = self.y2.max(y + 1);
        self.dirty = true;
    }

    fn mark_rect(&mut self, x: u32, y: u32, w: u32, h: u32) {
        self.x1 = self.x1.min(x);
        self.y1 = self.y1.min(y);
        self.x2 = self.x2.max(x + w);
        self.y2 = self.y2.max(y + h);
        self.dirty = true;
    }

    fn mark_all(&mut self, width: u32, height: u32) {
        self.x1 = 0;
        self.y1 = 0;
        self.x2 = width;
        self.y2 = height;
        self.dirty = true;
    }

    fn clear(&mut self) {
        self.x1 = u32::MAX;
        self.y1 = u32::MAX;
        self.x2 = 0;
        self.y2 = 0;
        self.dirty = false;
    }
}

impl Framebuffer {
    /// Create a new framebuffer
    pub fn new(phys_addr: u64, virt_addr: u64, mode: DisplayMode) -> Self {
        Self {
            phys_addr,
            virt_addr,
            mode,
            back_buffer: None,
            double_buffered: false,
            dirty: DirtyRegion::new(),
        }
    }

    /// Enable double buffering
    pub fn enable_double_buffering(&mut self) {
        let size = self.mode.framebuffer_size();
        self.back_buffer = Some(vec![0u8; size].into_boxed_slice());
        self.double_buffered = true;
    }

    /// Get display mode
    pub fn mode(&self) -> &DisplayMode {
        &self.mode
    }

    /// Get width
    pub fn width(&self) -> u32 {
        self.mode.width
    }

    /// Get height
    pub fn height(&self) -> u32 {
        self.mode.height
    }

    /// Get the drawing buffer (back buffer if double buffered, otherwise front)
    fn draw_buffer(&mut self) -> &mut [u8] {
        if let Some(ref mut back) = self.back_buffer {
            back
        } else {
            unsafe {
                core::slice::from_raw_parts_mut(
                    self.virt_addr as *mut u8,
                    self.mode.framebuffer_size(),
                )
            }
        }
    }

    /// Get the front buffer (actual framebuffer memory)
    fn front_buffer(&mut self) -> &mut [u8] {
        unsafe {
            core::slice::from_raw_parts_mut(
                self.virt_addr as *mut u8,
                self.mode.framebuffer_size(),
            )
        }
    }

    /// Put a pixel at (x, y)
    pub fn put_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x >= self.mode.width || y >= self.mode.height {
            return;
        }

        let offset = (y * self.mode.pitch + x * 4) as usize;
        let format = self.mode.format;
        let buffer = self.draw_buffer();

        if offset + 3 < buffer.len() {
            match format {
                PixelFormat::Argb32 | PixelFormat::Bgra32 => {
                    buffer[offset] = color.b;
                    buffer[offset + 1] = color.g;
                    buffer[offset + 2] = color.r;
                    buffer[offset + 3] = color.a;
                }
                PixelFormat::Rgba32 => {
                    buffer[offset] = color.r;
                    buffer[offset + 1] = color.g;
                    buffer[offset + 2] = color.b;
                    buffer[offset + 3] = color.a;
                }
                _ => {
                    // Default to BGRA
                    buffer[offset] = color.b;
                    buffer[offset + 1] = color.g;
                    buffer[offset + 2] = color.r;
                    buffer[offset + 3] = color.a;
                }
            }
        }

        self.dirty.mark(x, y);
    }

    /// Get a pixel at (x, y)
    pub fn get_pixel(&self, x: u32, y: u32) -> Color {
        if x >= self.mode.width || y >= self.mode.height {
            return Color::BLACK;
        }

        let offset = (y * self.mode.pitch + x * 4) as usize;
        let buffer = if let Some(ref back) = self.back_buffer {
            back.as_ref()
        } else {
            unsafe {
                core::slice::from_raw_parts(
                    self.virt_addr as *const u8,
                    self.mode.framebuffer_size(),
                )
            }
        };

        if offset + 3 < buffer.len() {
            Color {
                b: buffer[offset],
                g: buffer[offset + 1],
                r: buffer[offset + 2],
                a: buffer[offset + 3],
            }
        } else {
            Color::BLACK
        }
    }

    /// Clear the framebuffer with a color
    pub fn clear(&mut self, color: Color) {
        let pitch = self.mode.pitch as usize;
        let width = self.mode.width as usize;
        let height = self.mode.height as usize;
        let buffer = self.draw_buffer();

        for y in 0..height {
            let row_start = y * pitch;
            for x in 0..width {
                let offset = row_start + x * 4;
                if offset + 3 < buffer.len() {
                    buffer[offset] = color.b;
                    buffer[offset + 1] = color.g;
                    buffer[offset + 2] = color.r;
                    buffer[offset + 3] = color.a;
                }
            }
        }

        self.dirty.mark_all(self.mode.width, self.mode.height);
    }

    /// Draw a horizontal line
    pub fn draw_hline(&mut self, x1: u32, x2: u32, y: u32, color: Color) {
        if y >= self.mode.height {
            return;
        }

        let start_x = x1.min(x2).min(self.mode.width);
        let end_x = x1.max(x2).min(self.mode.width);

        for x in start_x..end_x {
            self.put_pixel(x, y, color);
        }
    }

    /// Draw a vertical line
    pub fn draw_vline(&mut self, x: u32, y1: u32, y2: u32, color: Color) {
        if x >= self.mode.width {
            return;
        }

        let start_y = y1.min(y2).min(self.mode.height);
        let end_y = y1.max(y2).min(self.mode.height);

        for y in start_y..end_y {
            self.put_pixel(x, y, color);
        }
    }

    /// Draw a line using Bresenham's algorithm
    pub fn draw_line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: Color) {
        let dx = (x2 - x1).abs();
        let dy = -(y2 - y1).abs();
        let sx = if x1 < x2 { 1 } else { -1 };
        let sy = if y1 < y2 { 1 } else { -1 };
        let mut err = dx + dy;
        let mut x = x1;
        let mut y = y1;

        loop {
            if x >= 0 && y >= 0 && (x as u32) < self.mode.width && (y as u32) < self.mode.height {
                self.put_pixel(x as u32, y as u32, color);
            }

            if x == x2 && y == y2 {
                break;
            }

            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Draw a rectangle outline
    pub fn draw_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: Color) {
        self.draw_hline(x, x + w, y, color);
        self.draw_hline(x, x + w, y + h - 1, color);
        self.draw_vline(x, y, y + h, color);
        self.draw_vline(x + w - 1, y, y + h, color);
    }

    /// Fill a rectangle
    pub fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: Color) {
        let x_end = (x + w).min(self.mode.width);
        let y_end = (y + h).min(self.mode.height);

        let pitch = self.mode.pitch as usize;
        let buffer = self.draw_buffer();

        for py in y..y_end {
            let row_start = (py as usize) * pitch;
            for px in x..x_end {
                let offset = row_start + (px as usize) * 4;
                if offset + 3 < buffer.len() {
                    buffer[offset] = color.b;
                    buffer[offset + 1] = color.g;
                    buffer[offset + 2] = color.r;
                    buffer[offset + 3] = color.a;
                }
            }
        }

        self.dirty.mark_rect(x, y, w, h);
    }

    /// Draw a circle outline using midpoint algorithm
    pub fn draw_circle(&mut self, cx: i32, cy: i32, radius: i32, color: Color) {
        let mut x = 0;
        let mut y = radius;
        let mut d = 1 - radius;

        while x <= y {
            self.put_circle_points(cx, cy, x, y, color);
            x += 1;

            if d < 0 {
                d += 2 * x + 1;
            } else {
                y -= 1;
                d += 2 * (x - y) + 1;
            }
        }
    }

    fn put_circle_points(&mut self, cx: i32, cy: i32, x: i32, y: i32, color: Color) {
        let points = [
            (cx + x, cy + y), (cx - x, cy + y),
            (cx + x, cy - y), (cx - x, cy - y),
            (cx + y, cy + x), (cx - y, cy + x),
            (cx + y, cy - x), (cx - y, cy - x),
        ];

        for (px, py) in points {
            if px >= 0 && py >= 0 && (px as u32) < self.mode.width && (py as u32) < self.mode.height {
                self.put_pixel(px as u32, py as u32, color);
            }
        }
    }

    /// Fill a circle
    pub fn fill_circle(&mut self, cx: i32, cy: i32, radius: i32, color: Color) {
        let mut x = 0;
        let mut y = radius;
        let mut d = 1 - radius;

        while x <= y {
            self.draw_hline_safe(cx - x, cx + x, cy + y, color);
            self.draw_hline_safe(cx - x, cx + x, cy - y, color);
            self.draw_hline_safe(cx - y, cx + y, cy + x, color);
            self.draw_hline_safe(cx - y, cx + y, cy - x, color);

            x += 1;
            if d < 0 {
                d += 2 * x + 1;
            } else {
                y -= 1;
                d += 2 * (x - y) + 1;
            }
        }
    }

    fn draw_hline_safe(&mut self, x1: i32, x2: i32, y: i32, color: Color) {
        if y < 0 || y >= self.mode.height as i32 {
            return;
        }

        let start = x1.max(0) as u32;
        let end = (x2.min(self.mode.width as i32 - 1) + 1) as u32;

        if start < end {
            self.draw_hline(start, end, y as u32, color);
        }
    }

    /// Draw a triangle outline
    pub fn draw_triangle(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, x3: i32, y3: i32, color: Color) {
        self.draw_line(x1, y1, x2, y2, color);
        self.draw_line(x2, y2, x3, y3, color);
        self.draw_line(x3, y3, x1, y1, color);
    }

    /// Blit a sprite/image
    pub fn blit(&mut self, x: u32, y: u32, width: u32, height: u32, data: &[Color]) {
        if data.len() < (width * height) as usize {
            return;
        }

        for dy in 0..height {
            for dx in 0..width {
                let px = x + dx;
                let py = y + dy;
                if px < self.mode.width && py < self.mode.height {
                    let color = data[(dy * width + dx) as usize];
                    if color.a > 0 {
                        if color.a == 255 {
                            self.put_pixel(px, py, color);
                        } else {
                            let existing = self.get_pixel(px, py);
                            self.put_pixel(px, py, existing.blend(color));
                        }
                    }
                }
            }
        }
    }

    /// Scroll the screen up by n lines
    pub fn scroll_up(&mut self, lines: u32, fill_color: Color) {
        if lines >= self.mode.height {
            self.clear(fill_color);
            return;
        }

        let pitch = self.mode.pitch as usize;
        let scroll_bytes = (lines as usize) * pitch;
        let fb_size = self.mode.framebuffer_size();
        let remaining = fb_size - scroll_bytes;
        let buffer = self.draw_buffer();

        // Copy data up
        buffer.copy_within(scroll_bytes..scroll_bytes + remaining, 0);

        // Fill bottom with color
        let fill_start = remaining;
        for i in (fill_start..buffer.len()).step_by(4) {
            if i + 3 < buffer.len() {
                buffer[i] = fill_color.b;
                buffer[i + 1] = fill_color.g;
                buffer[i + 2] = fill_color.r;
                buffer[i + 3] = fill_color.a;
            }
        }

        self.dirty.mark_all(self.mode.width, self.mode.height);
    }

    /// Swap buffers (for double buffering)
    pub fn swap_buffers(&mut self) {
        if !self.double_buffered || !self.dirty.dirty {
            return;
        }

        // Collect all values we need before mutable borrows
        let x1 = self.dirty.x1.min(self.mode.width);
        let y1 = self.dirty.y1.min(self.mode.height);
        let x2 = self.dirty.x2.min(self.mode.width);
        let y2 = self.dirty.y2.min(self.mode.height);
        let pitch = self.mode.pitch as usize;
        let bpp = 4; // Assuming 32bpp
        let fb_size = self.mode.framebuffer_size();

        if let Some(ref back) = self.back_buffer {
            // Get raw pointer and length from back buffer
            let back_ptr = back.as_ptr();
            let back_len = back.len();

            let front = unsafe {
                core::slice::from_raw_parts_mut(
                    self.virt_addr as *mut u8,
                    fb_size,
                )
            };

            for y in y1..y2 {
                let row_start = (y as usize) * pitch;
                let start = row_start + (x1 as usize) * bpp;
                let end = row_start + (x2 as usize) * bpp;

                if end <= front.len() && end <= back_len {
                    let back_slice = unsafe { core::slice::from_raw_parts(back_ptr.add(start), end - start) };
                    front[start..end].copy_from_slice(back_slice);
                }
            }
        }

        self.dirty.clear();
    }

    /// Force full buffer swap
    pub fn force_swap(&mut self) {
        let fb_size = self.mode.framebuffer_size();

        if let Some(ref back) = self.back_buffer {
            let back_ptr = back.as_ptr();
            let back_len = back.len();

            let front = unsafe {
                core::slice::from_raw_parts_mut(
                    self.virt_addr as *mut u8,
                    fb_size,
                )
            };

            let len = front.len().min(back_len);
            let back_slice = unsafe { core::slice::from_raw_parts(back_ptr, len) };
            front[..len].copy_from_slice(back_slice);
        }
        self.dirty.clear();
    }

    /// Flip the framebuffer (swap back and front buffer)
    pub fn flip(&mut self) {
        self.swap_buffers();
    }

    /// Wait for vertical sync (placeholder - actual vsync depends on hardware)
    pub fn vsync_wait(&mut self) {
        // In a real implementation, this would wait for the vertical blank interval
        // For now, just flush any pending operations
        self.swap_buffers();
    }

    /// Blit a raw byte buffer to the framebuffer
    pub fn blit_raw(&mut self, dst_x: u32, dst_y: u32, src: &[u8], width: u32, height: u32) {
        let bpp = self.mode.bpp as usize / 8;
        let dst_pitch = self.mode.pitch as usize;
        let src_pitch = width as usize * bpp;

        for row in 0..height {
            let src_offset = row as usize * src_pitch;
            let dst_offset = (dst_y as usize + row as usize) * dst_pitch + dst_x as usize * bpp;
            let copy_len = core::cmp::min(src_pitch, dst_pitch - (dst_x as usize * bpp));

            if src_offset + copy_len <= src.len() {
                let buffer = self.draw_buffer();
                if dst_offset + copy_len <= buffer.len() {
                    buffer[dst_offset..dst_offset + copy_len]
                        .copy_from_slice(&src[src_offset..src_offset + copy_len]);
                }
            }
        }

        self.dirty.mark_rect(dst_x, dst_y, width, height);
    }
}

// =============================================================================
// GRAPHICS CONTEXT
// =============================================================================

/// Graphics context for drawing operations
pub struct GraphicsContext {
    /// Current foreground color
    pub fg_color: Color,
    /// Current background color
    pub bg_color: Color,
    /// Current clip rectangle
    pub clip_x: u32,
    pub clip_y: u32,
    pub clip_w: u32,
    pub clip_h: u32,
    /// Font for text rendering
    pub font_height: u32,
    pub font_width: u32,
}

impl Default for GraphicsContext {
    fn default() -> Self {
        Self {
            fg_color: Color::WHITE,
            bg_color: Color::BLACK,
            clip_x: 0,
            clip_y: 0,
            clip_w: u32::MAX,
            clip_h: u32::MAX,
            font_height: 16,
            font_width: 8,
        }
    }
}

// =============================================================================
// DISPLAY DRIVER TRAIT
// =============================================================================

/// Display driver interface
pub trait DisplayDriver: Send + Sync {
    /// Get available display modes
    fn get_modes(&self) -> Vec<DisplayMode>;

    /// Set display mode
    fn set_mode(&mut self, mode: &DisplayMode) -> Result<(), DisplayError>;

    /// Get current mode
    fn current_mode(&self) -> Option<DisplayMode>;

    /// Get framebuffer
    fn framebuffer(&self) -> Option<&Framebuffer>;

    /// Get mutable framebuffer
    fn framebuffer_mut(&mut self) -> Option<&mut Framebuffer>;

    /// Check if display is available
    fn is_available(&self) -> bool;

    /// Get display name
    fn name(&self) -> &str;
}

/// Display driver errors
#[derive(Debug, Clone, Copy)]
pub enum DisplayError {
    NotSupported,
    InvalidMode,
    InitFailed,
    NoFramebuffer,
    OutOfMemory,
}

// =============================================================================
// GRAPHICS SUBSYSTEM
// =============================================================================

/// Global graphics subsystem
static GRAPHICS: Mutex<Option<GraphicsSubsystem>> = Mutex::new(None);

/// Graphics subsystem manager
pub struct GraphicsSubsystem {
    /// Primary display driver
    primary: Option<Box<dyn DisplayDriver>>,
    /// Secondary displays
    secondary: Vec<Box<dyn DisplayDriver>>,
    /// Initialized flag
    initialized: AtomicBool,
}

impl GraphicsSubsystem {
    /// Create new graphics subsystem
    pub fn new() -> Self {
        Self {
            primary: None,
            secondary: Vec::new(),
            initialized: AtomicBool::new(false),
        }
    }

    /// Register primary display
    pub fn register_primary(&mut self, driver: Box<dyn DisplayDriver>) {
        self.primary = Some(driver);
    }

    /// Register secondary display
    pub fn register_secondary(&mut self, driver: Box<dyn DisplayDriver>) {
        self.secondary.push(driver);
    }

    /// Get primary framebuffer
    pub fn framebuffer(&self) -> Option<&Framebuffer> {
        self.primary.as_ref()?.framebuffer()
    }

    /// Get mutable primary framebuffer
    pub fn framebuffer_mut(&mut self) -> Option<&mut Framebuffer> {
        self.primary.as_mut()?.framebuffer_mut()
    }

    /// Check if graphics available
    pub fn is_available(&self) -> bool {
        self.primary.as_ref().map(|d| d.is_available()).unwrap_or(false)
    }
}

// =============================================================================
// PUBLIC API
// =============================================================================

/// Initialize graphics subsystem
pub fn init() {
    let mut graphics = GraphicsSubsystem::new();

    // Try to initialize VESA driver
    if let Some(vesa_driver) = vesa::VesaDriver::detect() {
        graphics.register_primary(Box::new(vesa_driver));
    }

    graphics.initialized.store(true, Ordering::SeqCst);
    *GRAPHICS.lock() = Some(graphics);
}

/// Check if graphics available
pub fn is_available() -> bool {
    GRAPHICS.lock().as_ref().map(|g| g.is_available()).unwrap_or(false)
}

/// Get framebuffer dimensions
pub fn get_dimensions() -> Option<(u32, u32)> {
    let graphics = GRAPHICS.lock();
    let fb = graphics.as_ref()?.framebuffer()?;
    Some((fb.width(), fb.height()))
}

/// Clear screen
pub fn clear(color: Color) {
    let mut graphics = GRAPHICS.lock();
    if let Some(ref mut g) = *graphics {
        if let Some(fb) = g.framebuffer_mut() {
            fb.clear(color);
            fb.swap_buffers();
        }
    }
}

/// Put pixel
pub fn put_pixel(x: u32, y: u32, color: Color) {
    let mut graphics = GRAPHICS.lock();
    if let Some(ref mut g) = *graphics {
        if let Some(fb) = g.framebuffer_mut() {
            fb.put_pixel(x, y, color);
        }
    }
}

/// Fill rectangle
pub fn fill_rect(x: u32, y: u32, w: u32, h: u32, color: Color) {
    let mut graphics = GRAPHICS.lock();
    if let Some(ref mut g) = *graphics {
        if let Some(fb) = g.framebuffer_mut() {
            fb.fill_rect(x, y, w, h, color);
        }
    }
}

/// Swap buffers
pub fn swap_buffers() {
    let mut graphics = GRAPHICS.lock();
    if let Some(ref mut g) = *graphics {
        if let Some(fb) = g.framebuffer_mut() {
            fb.swap_buffers();
        }
    }
}

/// Draw text (using default font)
pub fn draw_text(x: u32, y: u32, text: &str, fg: Color, bg: Color) {
    let mut graphics = GRAPHICS.lock();
    if let Some(ref mut g) = *graphics {
        if let Some(fb) = g.framebuffer_mut() {
            font::draw_string(fb, x, y, text, fg, bg);
        }
    }
}
