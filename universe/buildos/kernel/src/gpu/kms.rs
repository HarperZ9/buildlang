//! KMS (Kernel Mode Setting)
//!
//! Implements display mode setting and configuration.

use alloc::string::String;
use alloc::vec::Vec;

use super::{
    OutputType, ConnectorStatus, DpmsState,
    drm::GammaRamp,
};

/// Display mode timing flags
#[derive(Clone, Copy, Debug, Default)]
pub struct ModeFlags {
    /// Positive horizontal sync
    pub phsync: bool,
    /// Negative horizontal sync
    pub nhsync: bool,
    /// Positive vertical sync
    pub pvsync: bool,
    /// Negative vertical sync
    pub nvsync: bool,
    /// Interlaced mode
    pub interlace: bool,
    /// Double scan
    pub dblscan: bool,
    /// Composite sync
    pub csync: bool,
    /// Positive composite sync
    pub pcsync: bool,
    /// Negative composite sync
    pub ncsync: bool,
    /// Horizontal sync positive
    pub hskew: bool,
    /// Broadcast
    pub bcast: bool,
    /// Pixel multiplex
    pub pixmux: bool,
    /// Double clock
    pub dblclk: bool,
    /// Clock divide by 2
    pub clkdiv2: bool,
}

impl ModeFlags {
    pub fn to_u32(&self) -> u32 {
        let mut flags = 0u32;
        if self.phsync { flags |= 1 << 0; }
        if self.nhsync { flags |= 1 << 1; }
        if self.pvsync { flags |= 1 << 2; }
        if self.nvsync { flags |= 1 << 3; }
        if self.interlace { flags |= 1 << 4; }
        if self.dblscan { flags |= 1 << 5; }
        if self.csync { flags |= 1 << 6; }
        if self.pcsync { flags |= 1 << 7; }
        if self.ncsync { flags |= 1 << 8; }
        if self.hskew { flags |= 1 << 9; }
        if self.bcast { flags |= 1 << 10; }
        if self.pixmux { flags |= 1 << 11; }
        if self.dblclk { flags |= 1 << 12; }
        if self.clkdiv2 { flags |= 1 << 13; }
        flags
    }

    pub fn from_u32(flags: u32) -> Self {
        Self {
            phsync: (flags & (1 << 0)) != 0,
            nhsync: (flags & (1 << 1)) != 0,
            pvsync: (flags & (1 << 2)) != 0,
            nvsync: (flags & (1 << 3)) != 0,
            interlace: (flags & (1 << 4)) != 0,
            dblscan: (flags & (1 << 5)) != 0,
            csync: (flags & (1 << 6)) != 0,
            pcsync: (flags & (1 << 7)) != 0,
            ncsync: (flags & (1 << 8)) != 0,
            hskew: (flags & (1 << 9)) != 0,
            bcast: (flags & (1 << 10)) != 0,
            pixmux: (flags & (1 << 11)) != 0,
            dblclk: (flags & (1 << 12)) != 0,
            clkdiv2: (flags & (1 << 13)) != 0,
        }
    }
}

/// Display mode information
#[derive(Clone, Debug)]
pub struct ModeInfo {
    /// Pixel clock in kHz
    pub clock: u32,
    /// Horizontal display size
    pub hdisplay: u16,
    /// Horizontal sync start
    pub hsync_start: u16,
    /// Horizontal sync end
    pub hsync_end: u16,
    /// Horizontal total
    pub htotal: u16,
    /// Horizontal skew
    pub hskew: u16,
    /// Vertical display size
    pub vdisplay: u16,
    /// Vertical sync start
    pub vsync_start: u16,
    /// Vertical sync end
    pub vsync_end: u16,
    /// Vertical total
    pub vtotal: u16,
    /// Vertical scan
    pub vscan: u16,
    /// Vertical refresh rate * 1000
    pub vrefresh: u32,
    /// Mode flags
    pub flags: ModeFlags,
    /// Mode type
    pub mode_type: u32,
    /// Mode name
    pub name: String,
}

impl ModeInfo {
    /// Create a new mode
    pub fn new(
        width: u16,
        height: u16,
        refresh: u32,
        clock: u32,
    ) -> Self {
        // Simple timing calculation
        let hdisplay = width;
        let htotal = width + width / 4;
        let hsync_start = width + width / 16;
        let hsync_end = hsync_start + width / 16;

        let vdisplay = height;
        let vtotal = height + height / 20;
        let vsync_start = height + height / 40;
        let vsync_end = vsync_start + 4;

        Self {
            clock,
            hdisplay,
            hsync_start,
            hsync_end,
            htotal,
            hskew: 0,
            vdisplay,
            vsync_start,
            vsync_end,
            vtotal,
            vscan: 0,
            vrefresh: refresh * 1000,
            flags: ModeFlags::default(),
            mode_type: MODE_TYPE_PREFERRED,
            name: alloc::format!("{}x{}@{}", width, height, refresh),
        }
    }

    /// Calculate refresh rate in Hz
    pub fn refresh_rate(&self) -> u32 {
        if self.htotal == 0 || self.vtotal == 0 {
            return 0;
        }
        let pixels = self.htotal as u64 * self.vtotal as u64;
        if pixels == 0 {
            return 0;
        }
        ((self.clock as u64 * 1000) / pixels) as u32
    }

    /// Get width
    pub fn width(&self) -> u32 {
        self.hdisplay as u32
    }

    /// Get height
    pub fn height(&self) -> u32 {
        self.vdisplay as u32
    }
}

impl Default for ModeInfo {
    fn default() -> Self {
        Self {
            clock: 0,
            hdisplay: 0,
            hsync_start: 0,
            hsync_end: 0,
            htotal: 0,
            hskew: 0,
            vdisplay: 0,
            vsync_start: 0,
            vsync_end: 0,
            vtotal: 0,
            vscan: 0,
            vrefresh: 0,
            flags: ModeFlags::default(),
            mode_type: 0,
            name: String::new(),
        }
    }
}

/// Mode type flags
pub const MODE_TYPE_BUILTIN: u32 = 1 << 0;
pub const MODE_TYPE_CLOCK_C: u32 = 1 << 1;
pub const MODE_TYPE_CRTC_C: u32 = 1 << 2;
pub const MODE_TYPE_PREFERRED: u32 = 1 << 3;
pub const MODE_TYPE_DEFAULT: u32 = 1 << 4;
pub const MODE_TYPE_USERDEF: u32 = 1 << 5;
pub const MODE_TYPE_DRIVER: u32 = 1 << 6;

/// Mode - a complete display mode
#[derive(Clone, Debug)]
pub struct Mode {
    /// Mode info
    pub info: ModeInfo,
    /// Is preferred
    pub preferred: bool,
    /// Status
    pub status: ModeStatus,
}

/// Mode status
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeStatus {
    /// Mode is OK
    Ok,
    /// Mode clock too high
    ClockHigh,
    /// Mode clock too low
    ClockLow,
    /// Horizontal display too large
    HDisplay,
    /// Horizontal sync start too large
    HSyncStart,
    /// Horizontal sync end too large
    HSyncEnd,
    /// Horizontal total too large
    HTotal,
    /// Vertical display too large
    VDisplay,
    /// Vertical sync start too large
    VSyncStart,
    /// Vertical sync end too large
    VSyncEnd,
    /// Vertical total too large
    VTotal,
    /// Bad vrefresh
    VRefresh,
    /// Bad flags
    Flags,
    /// Bad interlace
    Interlace,
    /// Bad double scan
    DoubleScan,
    /// No DPMS
    NoDpms,
    /// Bad pixel clock
    PixelClock,
    /// One width
    OneWidth,
    /// One height
    OneHeight,
    /// One size
    OneSize,
    /// Virtual too large
    VirtualX,
    /// Virtual too large
    VirtualY,
    /// Not in list
    NotInList,
    /// Error
    Error,
}

/// Connector - represents a physical display output
pub struct Connector {
    /// Connector ID
    pub id: u32,
    /// Connector type
    pub connector_type: OutputType,
    /// Connector type ID (for multiple of same type)
    pub connector_type_id: u32,
    /// Connection status
    pub status: ConnectorStatus,
    /// Physical size in mm
    pub mm_width: u32,
    pub mm_height: u32,
    /// Subpixel order
    pub subpixel: SubPixelOrder,
    /// Available modes
    pub modes: Vec<Mode>,
    /// Encoder IDs that can drive this connector
    pub encoder_ids: Vec<u32>,
    /// Currently selected encoder
    pub encoder_id: Option<u32>,
    /// Currently selected CRTC
    pub crtc_id: Option<u32>,
    /// DPMS property
    pub dpms: DpmsState,
    /// Properties
    pub properties: Vec<Property>,
    /// EDID data
    pub edid: Option<Edid>,
}

impl Connector {
    /// Create new connector
    pub fn new(id: u32, connector_type: OutputType) -> Self {
        Self {
            id,
            connector_type,
            connector_type_id: 0,
            status: ConnectorStatus::Unknown,
            mm_width: 0,
            mm_height: 0,
            subpixel: SubPixelOrder::Unknown,
            modes: Vec::new(),
            encoder_ids: Vec::new(),
            encoder_id: None,
            crtc_id: None,
            dpms: DpmsState::On,
            properties: Vec::new(),
            edid: None,
        }
    }

    /// Add mode
    pub fn add_mode(&mut self, mode: Mode) {
        self.modes.push(mode);
    }

    /// Get preferred mode
    pub fn preferred_mode(&self) -> Option<&Mode> {
        self.modes.iter().find(|m| m.preferred)
            .or_else(|| self.modes.first())
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        matches!(self.status, ConnectorStatus::Connected)
    }

    /// Get name
    pub fn name(&self) -> String {
        let type_name = match self.connector_type {
            OutputType::Unknown => "Unknown",
            OutputType::Vga => "VGA",
            OutputType::Dvi => "DVI",
            OutputType::DviI => "DVI-I",
            OutputType::DviD => "DVI-D",
            OutputType::DviA => "DVI-A",
            OutputType::Composite => "Composite",
            OutputType::SVideo => "S-Video",
            OutputType::Lvds => "LVDS",
            OutputType::Component => "Component",
            OutputType::DisplayPort => "DP",
            OutputType::Hdmi => "HDMI",
            OutputType::MiniDisplayPort => "mDP",
            OutputType::Edp => "eDP",
            OutputType::Virtual => "Virtual",
            OutputType::Writeback => "Writeback",
        };
        alloc::format!("{}-{}", type_name, self.connector_type_id)
    }
}

/// Subpixel order
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubPixelOrder {
    Unknown,
    HorizontalRgb,
    HorizontalBgr,
    VerticalRgb,
    VerticalBgr,
    None,
}

/// Encoder - connects CRTCs to connectors
pub struct Encoder {
    /// Encoder ID
    pub id: u32,
    /// Encoder type
    pub encoder_type: EncoderType,
    /// Current CRTC
    pub crtc_id: Option<u32>,
    /// Possible CRTCs (bitmask)
    pub possible_crtcs: u32,
    /// Possible clones (bitmask)
    pub possible_clones: u32,
}

impl Encoder {
    /// Create new encoder
    pub fn new(id: u32, encoder_type: EncoderType) -> Self {
        Self {
            id,
            encoder_type,
            crtc_id: None,
            possible_crtcs: 0xFFFFFFFF,
            possible_clones: 0xFFFFFFFF,
        }
    }
}

/// Encoder type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncoderType {
    None,
    Dac,
    Tmds,
    Lvds,
    Tvdac,
    Virtual,
    Dsi,
    Dpmst,
    Dpi,
}

/// CRTC - display timing controller
pub struct Crtc {
    /// CRTC ID
    pub id: u32,
    /// Framebuffer ID
    pub fb_id: Option<u32>,
    /// X position
    pub x: u32,
    /// Y position
    pub y: u32,
    /// Current mode
    pub mode: Option<ModeInfo>,
    /// Is active
    pub active: bool,
    /// Gamma ramp
    pub gamma: GammaRamp,
    /// Gamma size
    pub gamma_size: u32,
    /// Possible connectors (bitmask)
    pub possible_connectors: u32,
    /// Primary plane
    pub primary_plane: Option<u32>,
    /// Cursor plane
    pub cursor_plane: Option<u32>,
}

impl Crtc {
    /// Create new CRTC
    pub fn new(id: u32) -> Self {
        Self {
            id,
            fb_id: None,
            x: 0,
            y: 0,
            mode: None,
            active: false,
            gamma: GammaRamp::linear(256),
            gamma_size: 256,
            possible_connectors: 0xFFFFFFFF,
            primary_plane: None,
            cursor_plane: None,
        }
    }

    /// Get mode width
    pub fn width(&self) -> u32 {
        self.mode.as_ref().map(|m| m.hdisplay as u32).unwrap_or(0)
    }

    /// Get mode height
    pub fn height(&self) -> u32 {
        self.mode.as_ref().map(|m| m.vdisplay as u32).unwrap_or(0)
    }
}

/// Plane - overlay/cursor/primary plane
pub struct Plane {
    /// Plane ID
    pub id: u32,
    /// Plane type
    pub plane_type: PlaneType,
    /// Current CRTC
    pub crtc_id: Option<u32>,
    /// Current framebuffer
    pub fb_id: Option<u32>,
    /// Possible CRTCs (bitmask)
    pub possible_crtcs: u32,
    /// Source X (16.16 fixed point)
    pub src_x: u32,
    /// Source Y (16.16 fixed point)
    pub src_y: u32,
    /// Source width (16.16 fixed point)
    pub src_w: u32,
    /// Source height (16.16 fixed point)
    pub src_h: u32,
    /// Destination X
    pub crtc_x: i32,
    /// Destination Y
    pub crtc_y: i32,
    /// Destination width
    pub crtc_w: u32,
    /// Destination height
    pub crtc_h: u32,
    /// Supported formats
    pub formats: Vec<u32>,
    /// Properties
    pub properties: Vec<Property>,
}

impl Plane {
    /// Create new plane
    pub fn new(id: u32, plane_type: PlaneType) -> Self {
        Self {
            id,
            plane_type,
            crtc_id: None,
            fb_id: None,
            possible_crtcs: 0xFFFFFFFF,
            src_x: 0,
            src_y: 0,
            src_w: 0,
            src_h: 0,
            crtc_x: 0,
            crtc_y: 0,
            crtc_w: 0,
            crtc_h: 0,
            formats: Vec::new(),
            properties: Vec::new(),
        }
    }
}

/// Plane type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaneType {
    /// Overlay plane
    Overlay,
    /// Primary plane
    Primary,
    /// Cursor plane
    Cursor,
}

/// Property
#[derive(Clone, Debug)]
pub struct Property {
    /// Property ID
    pub id: u32,
    /// Property name
    pub name: String,
    /// Property flags
    pub flags: u32,
    /// Property type
    pub prop_type: PropertyType,
    /// Current value
    pub value: u64,
    /// Enum values (for enum properties)
    pub enums: Vec<PropertyEnum>,
    /// Min value (for range)
    pub min: u64,
    /// Max value (for range)
    pub max: u64,
}

/// Property type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropertyType {
    Range,
    Enum,
    Blob,
    Bitmask,
    Object,
    SignedRange,
}

/// Property enum value
#[derive(Clone, Debug)]
pub struct PropertyEnum {
    pub value: u64,
    pub name: String,
}

/// EDID data
#[derive(Clone, Debug)]
pub struct Edid {
    /// Raw EDID data
    pub raw: Vec<u8>,
    /// Manufacturer ID
    pub manufacturer: [u8; 3],
    /// Product code
    pub product_code: u16,
    /// Serial number
    pub serial: u32,
    /// Week of manufacture
    pub week: u8,
    /// Year of manufacture
    pub year: u16,
    /// EDID version
    pub version: u8,
    /// EDID revision
    pub revision: u8,
    /// Display name
    pub name: String,
    /// Physical width in cm
    pub width_cm: u8,
    /// Physical height in cm
    pub height_cm: u8,
    /// Preferred modes from EDID
    pub modes: Vec<ModeInfo>,
}

impl Edid {
    /// Parse EDID data
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 128 {
            return None;
        }

        // Check header
        if &data[0..8] != &[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00] {
            return None;
        }

        // Manufacturer ID (compressed ASCII)
        let mfg_raw = ((data[8] as u16) << 8) | (data[9] as u16);
        let manufacturer = [
            (((mfg_raw >> 10) & 0x1F) as u8 + b'A' - 1),
            (((mfg_raw >> 5) & 0x1F) as u8 + b'A' - 1),
            ((mfg_raw & 0x1F) as u8 + b'A' - 1),
        ];

        let product_code = ((data[11] as u16) << 8) | (data[10] as u16);
        let serial = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
        let week = data[16];
        let year = data[17] as u16 + 1990;
        let version = data[18];
        let revision = data[19];

        let width_cm = data[21];
        let height_cm = data[22];

        // Parse display name from descriptor blocks
        let mut name = String::new();
        for i in 0..4 {
            let desc_offset = 54 + i * 18;
            if data[desc_offset] == 0 && data[desc_offset + 1] == 0 {
                if data[desc_offset + 3] == 0xFC {
                    // Display name descriptor
                    for j in 5..18 {
                        let c = data[desc_offset + j];
                        if c == 0x0A || c == 0 {
                            break;
                        }
                        name.push(c as char);
                    }
                    break;
                }
            }
        }

        if name.is_empty() {
            name = alloc::format!("{}{}{}", manufacturer[0] as char, manufacturer[1] as char, manufacturer[2] as char);
        }

        Some(Self {
            raw: data.to_vec(),
            manufacturer,
            product_code,
            serial,
            week,
            year,
            version,
            revision,
            name: name.trim().into(),
            width_cm,
            height_cm,
            modes: Vec::new(),
        })
    }

    /// Get manufacturer string
    pub fn manufacturer_string(&self) -> String {
        String::from_utf8_lossy(&self.manufacturer).into_owned()
    }
}

/// Standard modes
pub fn get_standard_modes() -> Vec<ModeInfo> {
    alloc::vec![
        // 640x480@60Hz (VGA)
        ModeInfo {
            clock: 25175,
            hdisplay: 640, hsync_start: 656, hsync_end: 752, htotal: 800, hskew: 0,
            vdisplay: 480, vsync_start: 490, vsync_end: 492, vtotal: 525, vscan: 0,
            vrefresh: 60000, flags: ModeFlags { nhsync: true, nvsync: true, ..Default::default() },
            mode_type: MODE_TYPE_DRIVER, name: String::from("640x480@60"),
        },
        // 800x600@60Hz (SVGA)
        ModeInfo {
            clock: 40000,
            hdisplay: 800, hsync_start: 840, hsync_end: 968, htotal: 1056, hskew: 0,
            vdisplay: 600, vsync_start: 601, vsync_end: 605, vtotal: 628, vscan: 0,
            vrefresh: 60000, flags: ModeFlags { phsync: true, pvsync: true, ..Default::default() },
            mode_type: MODE_TYPE_DRIVER, name: String::from("800x600@60"),
        },
        // 1024x768@60Hz (XGA)
        ModeInfo {
            clock: 65000,
            hdisplay: 1024, hsync_start: 1048, hsync_end: 1184, htotal: 1344, hskew: 0,
            vdisplay: 768, vsync_start: 771, vsync_end: 777, vtotal: 806, vscan: 0,
            vrefresh: 60000, flags: ModeFlags { nhsync: true, nvsync: true, ..Default::default() },
            mode_type: MODE_TYPE_DRIVER, name: String::from("1024x768@60"),
        },
        // 1280x720@60Hz (720p)
        ModeInfo {
            clock: 74250,
            hdisplay: 1280, hsync_start: 1390, hsync_end: 1430, htotal: 1650, hskew: 0,
            vdisplay: 720, vsync_start: 725, vsync_end: 730, vtotal: 750, vscan: 0,
            vrefresh: 60000, flags: ModeFlags { phsync: true, pvsync: true, ..Default::default() },
            mode_type: MODE_TYPE_DRIVER, name: String::from("1280x720@60"),
        },
        // 1280x1024@60Hz (SXGA)
        ModeInfo {
            clock: 108000,
            hdisplay: 1280, hsync_start: 1328, hsync_end: 1440, htotal: 1688, hskew: 0,
            vdisplay: 1024, vsync_start: 1025, vsync_end: 1028, vtotal: 1066, vscan: 0,
            vrefresh: 60000, flags: ModeFlags { phsync: true, pvsync: true, ..Default::default() },
            mode_type: MODE_TYPE_DRIVER, name: String::from("1280x1024@60"),
        },
        // 1920x1080@60Hz (1080p)
        ModeInfo {
            clock: 148500,
            hdisplay: 1920, hsync_start: 2008, hsync_end: 2052, htotal: 2200, hskew: 0,
            vdisplay: 1080, vsync_start: 1084, vsync_end: 1089, vtotal: 1125, vscan: 0,
            vrefresh: 60000, flags: ModeFlags { phsync: true, pvsync: true, ..Default::default() },
            mode_type: MODE_TYPE_DRIVER | MODE_TYPE_PREFERRED, name: String::from("1920x1080@60"),
        },
        // 2560x1440@60Hz (QHD)
        ModeInfo {
            clock: 241500,
            hdisplay: 2560, hsync_start: 2608, hsync_end: 2640, htotal: 2720, hskew: 0,
            vdisplay: 1440, vsync_start: 1443, vsync_end: 1448, vtotal: 1481, vscan: 0,
            vrefresh: 60000, flags: ModeFlags { phsync: true, nvsync: true, ..Default::default() },
            mode_type: MODE_TYPE_DRIVER, name: String::from("2560x1440@60"),
        },
        // 3840x2160@60Hz (4K)
        ModeInfo {
            clock: 594000,
            hdisplay: 3840, hsync_start: 4016, hsync_end: 4104, htotal: 4400, hskew: 0,
            vdisplay: 2160, vsync_start: 2168, vsync_end: 2178, vtotal: 2250, vscan: 0,
            vrefresh: 60000, flags: ModeFlags { phsync: true, pvsync: true, ..Default::default() },
            mode_type: MODE_TYPE_DRIVER, name: String::from("3840x2160@60"),
        },
    ]
}
