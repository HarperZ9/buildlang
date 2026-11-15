//! DRM Framebuffer
//!
//! Framebuffer management for display output.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use super::GpuError;
use super::gem::GemHandle;

/// Pixel format FourCC codes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PixelFormat {
    /// RGB with 8 bits per component, no alpha (RGB888, 24bpp packed)
    Rgb888 = fourcc(b'R', b'G', b'2', b'4'),
    /// BGR with 8 bits per component, no alpha (BGR888, 24bpp packed)
    Bgr888 = fourcc(b'B', b'G', b'2', b'4'),
    /// RGB with 8 bits per component, 8-bit alpha (RGBA8888)
    Rgba8888 = fourcc(b'R', b'A', b'2', b'4'),
    /// ARGB with 8 bits per component (ARGB8888)
    Argb8888 = fourcc(b'A', b'R', b'2', b'4'),
    /// ABGR with 8 bits per component (ABGR8888)
    Abgr8888 = fourcc(b'A', b'B', b'2', b'4'),
    /// BGRA with 8 bits per component (BGRA8888)
    Bgra8888 = fourcc(b'B', b'A', b'2', b'4'),
    /// XRGB with 8 bits per component, unused alpha (XRGB8888)
    Xrgb8888 = fourcc(b'X', b'R', b'2', b'4'),
    /// XBGR with 8 bits per component (XBGR8888)
    Xbgr8888 = fourcc(b'X', b'B', b'2', b'4'),
    /// RGBX with 8 bits per component (RGBX8888)
    Rgbx8888 = fourcc(b'R', b'X', b'2', b'4'),
    /// BGRX with 8 bits per component (BGRX8888)
    Bgrx8888 = fourcc(b'B', b'X', b'2', b'4'),
    /// RGB 5:6:5 (RGB565)
    Rgb565 = fourcc(b'R', b'G', b'1', b'6'),
    /// BGR 5:6:5 (BGR565)
    Bgr565 = fourcc(b'B', b'G', b'1', b'6'),
    /// ARGB 1:5:5:5 (ARGB1555)
    Argb1555 = fourcc(b'A', b'R', b'1', b'5'),
    /// XRGB 1:5:5:5 (XRGB1555)
    Xrgb1555 = fourcc(b'X', b'R', b'1', b'5'),
    /// RGB 2:10:10:10 (RGB30)
    Xrgb2101010 = fourcc(b'X', b'R', b'3', b'0'),
    /// ARGB 2:10:10:10 (ARGB30)
    Argb2101010 = fourcc(b'A', b'R', b'3', b'0'),
    /// C8 indexed color
    C8 = fourcc(b'C', b'8', b' ', b' '),
    /// YUV 4:2:2 packed (YUYV)
    Yuyv = fourcc(b'Y', b'U', b'Y', b'V'),
    /// YUV 4:2:2 packed (UYVY)
    Uyvy = fourcc(b'U', b'Y', b'V', b'Y'),
    /// NV12 (Y plane + interleaved UV)
    Nv12 = fourcc(b'N', b'V', b'1', b'2'),
    /// NV21 (Y plane + interleaved VU)
    Nv21 = fourcc(b'N', b'V', b'2', b'1'),
    /// YUV 4:2:0 planar (I420/YV12)
    Yuv420 = fourcc(b'Y', b'U', b'1', b'2'),
}

/// Create FourCC code
const fn fourcc(a: u8, b: u8, c: u8, d: u8) -> u32 {
    (a as u32) | ((b as u32) << 8) | ((c as u32) << 16) | ((d as u32) << 24)
}

impl PixelFormat {
    /// Get FourCC code
    pub fn fourcc(&self) -> u32 {
        *self as u32
    }

    /// Get bits per pixel
    pub fn bits_per_pixel(&self) -> u32 {
        match self {
            PixelFormat::C8 => 8,
            PixelFormat::Rgb565 | PixelFormat::Bgr565 |
            PixelFormat::Argb1555 | PixelFormat::Xrgb1555 => 16,
            PixelFormat::Rgb888 | PixelFormat::Bgr888 => 24,
            PixelFormat::Rgba8888 | PixelFormat::Argb8888 |
            PixelFormat::Abgr8888 | PixelFormat::Bgra8888 |
            PixelFormat::Xrgb8888 | PixelFormat::Xbgr8888 |
            PixelFormat::Rgbx8888 | PixelFormat::Bgrx8888 |
            PixelFormat::Xrgb2101010 | PixelFormat::Argb2101010 => 32,
            PixelFormat::Yuyv | PixelFormat::Uyvy => 16,
            PixelFormat::Nv12 | PixelFormat::Nv21 | PixelFormat::Yuv420 => 12,
        }
    }

    /// Get bytes per pixel
    pub fn bytes_per_pixel(&self) -> u32 {
        (self.bits_per_pixel() + 7) / 8
    }

    /// Is format RGB
    pub fn is_rgb(&self) -> bool {
        matches!(self,
            PixelFormat::Rgb888 | PixelFormat::Bgr888 |
            PixelFormat::Rgba8888 | PixelFormat::Argb8888 |
            PixelFormat::Abgr8888 | PixelFormat::Bgra8888 |
            PixelFormat::Xrgb8888 | PixelFormat::Xbgr8888 |
            PixelFormat::Rgbx8888 | PixelFormat::Bgrx8888 |
            PixelFormat::Rgb565 | PixelFormat::Bgr565 |
            PixelFormat::Argb1555 | PixelFormat::Xrgb1555 |
            PixelFormat::Xrgb2101010 | PixelFormat::Argb2101010
        )
    }

    /// Is format YUV
    pub fn is_yuv(&self) -> bool {
        matches!(self,
            PixelFormat::Yuyv | PixelFormat::Uyvy |
            PixelFormat::Nv12 | PixelFormat::Nv21 |
            PixelFormat::Yuv420
        )
    }

    /// Has alpha channel
    pub fn has_alpha(&self) -> bool {
        matches!(self,
            PixelFormat::Rgba8888 | PixelFormat::Argb8888 |
            PixelFormat::Abgr8888 | PixelFormat::Bgra8888 |
            PixelFormat::Argb1555 | PixelFormat::Argb2101010
        )
    }

    /// Number of planes
    pub fn num_planes(&self) -> u32 {
        match self {
            PixelFormat::Nv12 | PixelFormat::Nv21 => 2,
            PixelFormat::Yuv420 => 3,
            _ => 1,
        }
    }

    /// From FourCC
    pub fn from_fourcc(fourcc: u32) -> Option<Self> {
        match fourcc {
            x if x == PixelFormat::Rgb888 as u32 => Some(PixelFormat::Rgb888),
            x if x == PixelFormat::Bgr888 as u32 => Some(PixelFormat::Bgr888),
            x if x == PixelFormat::Rgba8888 as u32 => Some(PixelFormat::Rgba8888),
            x if x == PixelFormat::Argb8888 as u32 => Some(PixelFormat::Argb8888),
            x if x == PixelFormat::Abgr8888 as u32 => Some(PixelFormat::Abgr8888),
            x if x == PixelFormat::Bgra8888 as u32 => Some(PixelFormat::Bgra8888),
            x if x == PixelFormat::Xrgb8888 as u32 => Some(PixelFormat::Xrgb8888),
            x if x == PixelFormat::Xbgr8888 as u32 => Some(PixelFormat::Xbgr8888),
            x if x == PixelFormat::Rgbx8888 as u32 => Some(PixelFormat::Rgbx8888),
            x if x == PixelFormat::Bgrx8888 as u32 => Some(PixelFormat::Bgrx8888),
            x if x == PixelFormat::Rgb565 as u32 => Some(PixelFormat::Rgb565),
            x if x == PixelFormat::Bgr565 as u32 => Some(PixelFormat::Bgr565),
            x if x == PixelFormat::Argb1555 as u32 => Some(PixelFormat::Argb1555),
            x if x == PixelFormat::Xrgb1555 as u32 => Some(PixelFormat::Xrgb1555),
            x if x == PixelFormat::Xrgb2101010 as u32 => Some(PixelFormat::Xrgb2101010),
            x if x == PixelFormat::Argb2101010 as u32 => Some(PixelFormat::Argb2101010),
            x if x == PixelFormat::C8 as u32 => Some(PixelFormat::C8),
            x if x == PixelFormat::Yuyv as u32 => Some(PixelFormat::Yuyv),
            x if x == PixelFormat::Uyvy as u32 => Some(PixelFormat::Uyvy),
            x if x == PixelFormat::Nv12 as u32 => Some(PixelFormat::Nv12),
            x if x == PixelFormat::Nv21 as u32 => Some(PixelFormat::Nv21),
            x if x == PixelFormat::Yuv420 as u32 => Some(PixelFormat::Yuv420),
            _ => None,
        }
    }
}

/// Format modifier
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatModifier(pub u64);

impl FormatModifier {
    /// No modifier / linear
    pub const LINEAR: FormatModifier = FormatModifier(0);
    /// Invalid modifier
    pub const INVALID: FormatModifier = FormatModifier(0x00FFFFFFFFFFFFFF);

    /// Intel X-tiling
    pub const I915_X_TILED: FormatModifier = FormatModifier((1 << 56) | 1);
    /// Intel Y-tiling
    pub const I915_Y_TILED: FormatModifier = FormatModifier((1 << 56) | 2);
    /// Intel Y-tiling with CCS
    pub const I915_Y_TILED_CCS: FormatModifier = FormatModifier((1 << 56) | 4);
    /// Intel Tile4
    pub const I915_TILE4: FormatModifier = FormatModifier((1 << 56) | 9);
}

/// DRM framebuffer
pub struct DrmFramebuffer {
    /// Framebuffer ID
    pub id: u32,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Pixel format
    pub format: PixelFormat,
    /// Format modifier
    pub modifier: FormatModifier,
    /// Flags
    pub flags: u32,
    /// Planes (handles, pitches, offsets)
    pub planes: [FramebufferPlane; 4],
    /// Number of planes
    pub num_planes: u32,
    /// Reference count
    ref_count: AtomicU32,
    /// Hot spot X (for cursor)
    pub hot_x: u32,
    /// Hot spot Y (for cursor)
    pub hot_y: u32,
}

/// Framebuffer plane info
#[derive(Clone, Copy, Debug, Default)]
pub struct FramebufferPlane {
    /// GEM handle
    pub handle: GemHandle,
    /// Pitch (bytes per row)
    pub pitch: u32,
    /// Offset into buffer
    pub offset: u32,
}

impl DrmFramebuffer {
    /// Create new framebuffer
    pub fn new(width: u32, height: u32, format: PixelFormat) -> Self {
        Self {
            id: 0,
            width,
            height,
            format,
            modifier: FormatModifier::LINEAR,
            flags: 0,
            planes: [FramebufferPlane::default(); 4],
            num_planes: 1,
            ref_count: AtomicU32::new(1),
            hot_x: 0,
            hot_y: 0,
        }
    }

    /// Create framebuffer with handle
    pub fn with_handle(
        width: u32,
        height: u32,
        format: PixelFormat,
        handle: GemHandle,
        pitch: u32,
    ) -> Self {
        let mut fb = Self::new(width, height, format);
        fb.planes[0] = FramebufferPlane {
            handle,
            pitch,
            offset: 0,
        };
        fb
    }

    /// Get reference
    pub fn get(&self) -> u32 {
        self.ref_count.fetch_add(1, Ordering::SeqCst)
    }

    /// Put reference
    pub fn put(&self) -> u32 {
        self.ref_count.fetch_sub(1, Ordering::SeqCst)
    }

    /// Get reference count
    pub fn refcount(&self) -> u32 {
        self.ref_count.load(Ordering::Acquire)
    }

    /// Calculate minimum size needed
    pub fn min_size(&self) -> u64 {
        let mut size = 0u64;
        for i in 0..self.num_planes as usize {
            let plane = &self.planes[i];
            let plane_size = (plane.offset as u64) +
                            (plane.pitch as u64) * (self.plane_height(i) as u64);
            size = size.max(plane_size);
        }
        size
    }

    /// Get plane height
    fn plane_height(&self, plane_idx: usize) -> u32 {
        match self.format {
            PixelFormat::Nv12 | PixelFormat::Nv21 | PixelFormat::Yuv420 => {
                if plane_idx == 0 {
                    self.height
                } else {
                    self.height / 2
                }
            }
            _ => self.height,
        }
    }

    /// Get bytes per pixel for main plane
    pub fn bpp(&self) -> u32 {
        self.format.bytes_per_pixel()
    }

    /// Get depth (color depth)
    pub fn depth(&self) -> u32 {
        match self.format {
            PixelFormat::C8 => 8,
            PixelFormat::Rgb565 | PixelFormat::Bgr565 => 16,
            PixelFormat::Argb1555 | PixelFormat::Xrgb1555 => 15,
            PixelFormat::Rgb888 | PixelFormat::Bgr888 |
            PixelFormat::Xrgb8888 | PixelFormat::Xbgr8888 |
            PixelFormat::Rgbx8888 | PixelFormat::Bgrx8888 => 24,
            PixelFormat::Rgba8888 | PixelFormat::Argb8888 |
            PixelFormat::Abgr8888 | PixelFormat::Bgra8888 => 32,
            PixelFormat::Xrgb2101010 | PixelFormat::Argb2101010 => 30,
            _ => 24,
        }
    }
}

/// Framebuffer command (for dirty region updates)
#[derive(Clone, Debug)]
pub struct DirtyCmd {
    pub flags: u32,
    pub color: u32,
    pub clips: Vec<DirtyClip>,
}

/// Dirty region clip
#[derive(Clone, Copy, Debug)]
pub struct DirtyClip {
    pub x1: u32,
    pub y1: u32,
    pub x2: u32,
    pub y2: u32,
}

/// Framebuffer2 creation arguments (addfb2)
#[derive(Clone, Debug)]
pub struct Fb2Args {
    pub width: u32,
    pub height: u32,
    pub pixel_format: u32,
    pub flags: u32,
    pub handles: [GemHandle; 4],
    pub pitches: [u32; 4],
    pub offsets: [u32; 4],
    pub modifier: [u64; 4],
}

impl Fb2Args {
    /// Convert to DrmFramebuffer
    pub fn to_framebuffer(&self) -> Result<DrmFramebuffer, GpuError> {
        let format = PixelFormat::from_fourcc(self.pixel_format)
            .ok_or(GpuError::InvalidParameter)?;

        let mut fb = DrmFramebuffer::new(self.width, self.height, format);
        fb.flags = self.flags;
        fb.num_planes = format.num_planes();

        if self.modifier[0] != 0 {
            fb.modifier = FormatModifier(self.modifier[0]);
        }

        for i in 0..fb.num_planes as usize {
            fb.planes[i] = FramebufferPlane {
                handle: self.handles[i],
                pitch: self.pitches[i],
                offset: self.offsets[i],
            };
        }

        Ok(fb)
    }
}

/// Legacy framebuffer creation (addfb)
#[derive(Clone, Debug)]
pub struct FbArgs {
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub bpp: u32,
    pub depth: u32,
    pub handle: GemHandle,
}

impl FbArgs {
    /// Convert to DrmFramebuffer
    pub fn to_framebuffer(&self) -> Result<DrmFramebuffer, GpuError> {
        let format = match (self.bpp, self.depth) {
            (8, 8) => PixelFormat::C8,
            (16, 15) => PixelFormat::Xrgb1555,
            (16, 16) => PixelFormat::Rgb565,
            (24, 24) => PixelFormat::Rgb888,
            (32, 24) => PixelFormat::Xrgb8888,
            (32, 30) => PixelFormat::Xrgb2101010,
            (32, 32) => PixelFormat::Argb8888,
            _ => return Err(GpuError::InvalidParameter),
        };

        let mut fb = DrmFramebuffer::new(self.width, self.height, format);
        fb.planes[0] = FramebufferPlane {
            handle: self.handle,
            pitch: self.pitch,
            offset: 0,
        };

        Ok(fb)
    }
}

/// Console framebuffer (for text console)
pub struct ConsoleFb {
    /// Physical address
    pub phys_addr: u64,
    /// Virtual address
    pub virt_addr: usize,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Pitch
    pub pitch: u32,
    /// Bits per pixel
    pub bpp: u32,
    /// Red mask
    pub red: ColorMask,
    /// Green mask
    pub green: ColorMask,
    /// Blue mask
    pub blue: ColorMask,
    /// Font width
    pub font_width: u32,
    /// Font height
    pub font_height: u32,
    /// Columns
    pub cols: u32,
    /// Rows
    pub rows: u32,
    /// Cursor X
    pub cursor_x: u32,
    /// Cursor Y
    pub cursor_y: u32,
    /// Foreground color
    pub fg_color: u32,
    /// Background color
    pub bg_color: u32,
}

/// Color mask
#[derive(Clone, Copy, Debug, Default)]
pub struct ColorMask {
    pub offset: u8,
    pub length: u8,
}

impl ConsoleFb {
    /// Create from existing framebuffer
    pub fn new(
        phys_addr: u64,
        virt_addr: usize,
        width: u32,
        height: u32,
        pitch: u32,
        bpp: u32,
    ) -> Self {
        let (red, green, blue) = match bpp {
            16 => (
                ColorMask { offset: 11, length: 5 },
                ColorMask { offset: 5, length: 6 },
                ColorMask { offset: 0, length: 5 },
            ),
            24 | 32 => (
                ColorMask { offset: 16, length: 8 },
                ColorMask { offset: 8, length: 8 },
                ColorMask { offset: 0, length: 8 },
            ),
            _ => (ColorMask::default(), ColorMask::default(), ColorMask::default()),
        };

        let font_width = 8;
        let font_height = 16;
        let cols = width / font_width;
        let rows = height / font_height;

        Self {
            phys_addr,
            virt_addr,
            width,
            height,
            pitch,
            bpp,
            red,
            green,
            blue,
            font_width,
            font_height,
            cols,
            rows,
            cursor_x: 0,
            cursor_y: 0,
            fg_color: 0xFFFFFF,
            bg_color: 0x000000,
        }
    }

    /// Put pixel
    pub fn put_pixel(&self, x: u32, y: u32, color: u32) {
        if x >= self.width || y >= self.height {
            return;
        }

        let offset = (y * self.pitch) + (x * (self.bpp / 8));
        let ptr = (self.virt_addr + offset as usize) as *mut u8;

        unsafe {
            match self.bpp {
                16 => {
                    let color16 = ((color >> 8) & 0xF800) |
                                  ((color >> 5) & 0x07E0) |
                                  ((color >> 3) & 0x001F);
                    *(ptr as *mut u16) = color16 as u16;
                }
                24 => {
                    *ptr = color as u8;
                    *ptr.add(1) = (color >> 8) as u8;
                    *ptr.add(2) = (color >> 16) as u8;
                }
                32 => {
                    *(ptr as *mut u32) = color;
                }
                _ => {}
            }
        }
    }

    /// Fill rectangle
    pub fn fill_rect(&self, x: u32, y: u32, w: u32, h: u32, color: u32) {
        for dy in 0..h {
            for dx in 0..w {
                self.put_pixel(x + dx, y + dy, color);
            }
        }
    }

    /// Clear screen
    pub fn clear(&self) {
        self.fill_rect(0, 0, self.width, self.height, self.bg_color);
    }

    /// Scroll up one line
    pub fn scroll_up(&self) {
        let line_bytes = self.font_height * self.pitch;
        let total_bytes = (self.rows - 1) * line_bytes;

        unsafe {
            core::ptr::copy(
                (self.virt_addr + line_bytes as usize) as *const u8,
                self.virt_addr as *mut u8,
                total_bytes as usize,
            );
        }

        // Clear bottom line
        self.fill_rect(
            0,
            (self.rows - 1) * self.font_height,
            self.width,
            self.font_height,
            self.bg_color,
        );
    }

    /// Make RGB color
    pub fn make_color(&self, r: u8, g: u8, b: u8) -> u32 {
        ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
    }
}

/// Blend modes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendMode {
    /// No blending
    None,
    /// Pre-multiplied alpha
    PreMultiplied,
    /// Coverage (anti-aliasing)
    Coverage,
}

/// Color encoding for YUV
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorEncoding {
    /// BT.601
    Bt601,
    /// BT.709
    Bt709,
    /// BT.2020
    Bt2020,
}

/// Color range
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorRange {
    /// Limited range (16-235)
    Limited,
    /// Full range (0-255)
    Full,
}
