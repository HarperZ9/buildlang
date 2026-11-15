//! VESA VBE (Video BIOS Extensions) Driver for BuildOS
//!
//! Provides framebuffer access through VESA BIOS calls and VBE 2.0+ features.

use super::{
    DisplayDriver, DisplayError, DisplayMode, Framebuffer,
    PixelFormat,
};
use crate::cpu::io::inb;
use alloc::vec::Vec;

/// VBE info block returned by VBE function 00h
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct VbeInfoBlock {
    /// VBE signature, should be "VESA"
    pub signature: [u8; 4],
    /// VBE version (high byte = major, low byte = minor)
    pub version: u16,
    /// Pointer to OEM string
    pub oem_string_ptr: u32,
    /// Capabilities flags
    pub capabilities: u32,
    /// Pointer to video mode list
    pub video_modes_ptr: u32,
    /// Total video memory in 64KB blocks
    pub total_memory: u16,
    /// VBE 2.0+ software revision
    pub oem_software_rev: u16,
    /// Pointer to vendor name
    pub oem_vendor_name_ptr: u32,
    /// Pointer to product name
    pub oem_product_name_ptr: u32,
    /// Pointer to product revision
    pub oem_product_rev_ptr: u32,
    /// Reserved space
    pub reserved: [u8; 222],
    /// OEM data area
    pub oem_data: [u8; 256],
}

/// VBE mode info block returned by VBE function 01h
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct VbeModeInfo {
    /// Mode attributes
    pub mode_attributes: u16,
    /// Window A attributes
    pub win_a_attributes: u8,
    /// Window B attributes
    pub win_b_attributes: u8,
    /// Window granularity in KB
    pub win_granularity: u16,
    /// Window size in KB
    pub win_size: u16,
    /// Window A segment
    pub win_a_segment: u16,
    /// Window B segment
    pub win_b_segment: u16,
    /// Pointer to window function
    pub win_func_ptr: u32,
    /// Bytes per scan line
    pub bytes_per_scan_line: u16,

    // VBE 1.2+ fields
    /// Horizontal resolution in pixels
    pub x_resolution: u16,
    /// Vertical resolution in pixels
    pub y_resolution: u16,
    /// Character cell width
    pub x_char_size: u8,
    /// Character cell height
    pub y_char_size: u8,
    /// Number of memory planes
    pub number_of_planes: u8,
    /// Bits per pixel
    pub bits_per_pixel: u8,
    /// Number of memory banks
    pub number_of_banks: u8,
    /// Memory model type
    pub memory_model: u8,
    /// Bank size in KB
    pub bank_size: u8,
    /// Number of image pages
    pub number_of_image_pages: u8,
    /// Reserved
    pub reserved1: u8,

    // Direct color fields
    /// Red mask size
    pub red_mask_size: u8,
    /// Red field position
    pub red_field_position: u8,
    /// Green mask size
    pub green_mask_size: u8,
    /// Green field position
    pub green_field_position: u8,
    /// Blue mask size
    pub blue_mask_size: u8,
    /// Blue field position
    pub blue_field_position: u8,
    /// Reserved mask size
    pub rsvd_mask_size: u8,
    /// Reserved field position
    pub rsvd_field_position: u8,
    /// Direct color mode info
    pub direct_color_mode_info: u8,

    // VBE 2.0+ fields
    /// Physical address of linear frame buffer
    pub phys_base_ptr: u32,
    /// Reserved
    pub reserved2: u32,
    /// Reserved
    pub reserved3: u16,

    // VBE 3.0+ fields
    /// Bytes per scan line for linear modes
    pub lin_bytes_per_scan_line: u16,
    /// Number of image pages for banked modes
    pub bnk_number_of_image_pages: u8,
    /// Number of image pages for linear modes
    pub lin_number_of_image_pages: u8,
    /// Red mask size for linear modes
    pub lin_red_mask_size: u8,
    /// Red field position for linear modes
    pub lin_red_field_position: u8,
    /// Green mask size for linear modes
    pub lin_green_mask_size: u8,
    /// Green field position for linear modes
    pub lin_green_field_position: u8,
    /// Blue mask size for linear modes
    pub lin_blue_mask_size: u8,
    /// Blue field position for linear modes
    pub lin_blue_field_position: u8,
    /// Reserved mask size for linear modes
    pub lin_rsvd_mask_size: u8,
    /// Reserved field position for linear modes
    pub lin_rsvd_field_position: u8,
    /// Maximum pixel clock in Hz
    pub max_pixel_clock: u32,

    /// Reserved space
    pub reserved4: [u8; 189],
}

/// VBE memory model types
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VbeMemoryModel {
    Text = 0x00,
    CGA = 0x01,
    Hercules = 0x02,
    Planar = 0x03,
    PackedPixel = 0x04,
    NonChain4 = 0x05,
    DirectColor = 0x06,
    YUV = 0x07,
}

/// VBE mode attributes flags
#[allow(non_snake_case)]
pub mod VbeModeAttributes {
    pub const SUPPORTED: u16 = 1 << 0;
    pub const TTY_OUTPUT: u16 = 1 << 2;
    pub const COLOR: u16 = 1 << 3;
    pub const GRAPHICS: u16 = 1 << 4;
    pub const NOT_VGA_COMPATIBLE: u16 = 1 << 5;
    pub const NO_WINDOWED_MODE: u16 = 1 << 6;
    pub const LINEAR_FB: u16 = 1 << 7;
    pub const DOUBLE_SCAN: u16 = 1 << 8;
    pub const INTERLACED: u16 = 1 << 9;
    pub const TRIPLE_BUFFER: u16 = 1 << 10;
    pub const STEREO: u16 = 1 << 11;
    pub const DUAL_DISPLAY: u16 = 1 << 12;
}

/// VESA VBE driver
pub struct VesaDriver {
    /// VBE info block
    vbe_info: Option<VbeInfoBlock>,
    /// Available display modes
    available_modes: Vec<VesaModeEntry>,
    /// Current mode info
    current_mode: Option<VbeModeInfo>,
    /// Current mode number
    current_mode_number: u16,
    /// Framebuffer
    framebuffer: Option<Framebuffer>,
    /// Is driver initialized
    initialized: bool,
}

/// Entry in mode list
#[derive(Clone)]
pub struct VesaModeEntry {
    /// VBE mode number
    pub mode_number: u16,
    /// Mode info
    pub mode_info: VbeModeInfo,
    /// Display mode representation
    pub display_mode: DisplayMode,
}

impl VesaDriver {
    /// Create a new VESA driver
    pub const fn new() -> Self {
        Self {
            vbe_info: None,
            available_modes: Vec::new(),
            current_mode: None,
            current_mode_number: 0,
            framebuffer: None,
            initialized: false,
        }
    }

    /// Detect VESA VBE support
    pub fn detect() -> Option<Self> {
        let mut driver = Self::new();

        // Try to get VBE info
        if driver.get_vbe_info().is_err() {
            return None;
        }

        // Enumerate available modes
        if driver.enumerate_modes().is_err() {
            return None;
        }

        driver.initialized = true;
        Some(driver)
    }

    /// Get VBE controller info
    fn get_vbe_info(&mut self) -> Result<(), DisplayError> {
        // In a real implementation, this would call BIOS int 10h, function 4F00h
        // For now, we'll check if we were passed VBE info from the bootloader

        // The bootloader typically provides this info at a known location
        // or through a multiboot info structure

        #[cfg(feature = "multiboot")]
        {
            use crate::boot::multiboot::get_vbe_info;
            if let Some(info) = get_vbe_info() {
                self.vbe_info = Some(info);
                return Ok(());
            }
        }

        // For UEFI boot, we get framebuffer info differently
        #[cfg(feature = "uefi")]
        {
            use crate::boot::uefi::get_gop_info;
            if let Some(_gop) = get_gop_info() {
                // Create a synthetic VbeInfoBlock from GOP
                return Ok(());
            }
        }

        // Try to detect from bootloader info structure
        if let Some(info) = Self::get_bootloader_vbe_info() {
            self.vbe_info = Some(info);
            return Ok(());
        }

        Err(DisplayError::NotSupported)
    }

    /// Get VBE info from bootloader
    fn get_bootloader_vbe_info() -> Option<VbeInfoBlock> {
        // This would read from bootloader-provided structure
        // For now, return None (will be implemented per boot protocol)
        None
    }

    /// Enumerate available video modes
    fn enumerate_modes(&mut self) -> Result<(), DisplayError> {
        let vbe_info = self.vbe_info.as_ref().ok_or(DisplayError::NotSupported)?;

        // Mode list is at video_modes_ptr, terminated by 0xFFFF
        let _mode_list_addr = vbe_info.video_modes_ptr as usize;

        // Clear existing modes
        self.available_modes.clear();

        // In real implementation, we'd iterate through the mode list
        // and query each mode with VBE function 01h

        // For now, add some common modes if we have framebuffer access
        let common_modes = [
            (640, 480, 32),
            (800, 600, 32),
            (1024, 768, 32),
            (1280, 720, 32),
            (1280, 800, 32),
            (1280, 1024, 32),
            (1366, 768, 32),
            (1440, 900, 32),
            (1600, 900, 32),
            (1680, 1050, 32),
            (1920, 1080, 32),
            (2560, 1440, 32),
            (3840, 2160, 32),
        ];

        for (i, &(width, height, bpp)) in common_modes.iter().enumerate() {
            let mode_info = Self::create_mode_info(width, height, bpp);
            let display_mode = DisplayMode {
                width,
                height,
                bpp: bpp as u8,
                pitch: width * (bpp / 8),
                format: PixelFormat::Argb32,
                refresh_rate: 60,
            };

            self.available_modes.push(VesaModeEntry {
                mode_number: 0x4100 + i as u16, // LFB modes start at 0x4100
                mode_info,
                display_mode,
            });
        }

        Ok(())
    }

    /// Create a mode info structure for a given resolution
    fn create_mode_info(width: u32, height: u32, bpp: u32) -> VbeModeInfo {
        let info = VbeModeInfo {
            mode_attributes: VbeModeAttributes::SUPPORTED
                | VbeModeAttributes::COLOR
                | VbeModeAttributes::GRAPHICS
                | VbeModeAttributes::LINEAR_FB,
            win_a_attributes: 0,
            win_b_attributes: 0,
            win_granularity: 64,
            win_size: 64,
            win_a_segment: 0xA000,
            win_b_segment: 0,
            win_func_ptr: 0,
            bytes_per_scan_line: (width * (bpp / 8)) as u16,
            x_resolution: width as u16,
            y_resolution: height as u16,
            x_char_size: 8,
            y_char_size: 16,
            number_of_planes: 1,
            bits_per_pixel: bpp as u8,
            number_of_banks: 1,
            memory_model: VbeMemoryModel::DirectColor as u8,
            bank_size: 0,
            number_of_image_pages: 0,
            reserved1: 0,
            red_mask_size: 8,
            red_field_position: 16,
            green_mask_size: 8,
            green_field_position: 8,
            blue_mask_size: 8,
            blue_field_position: 0,
            rsvd_mask_size: 8,
            rsvd_field_position: 24,
            direct_color_mode_info: 0,
            phys_base_ptr: 0, // Will be set when mode is activated
            reserved2: 0,
            reserved3: 0,
            lin_bytes_per_scan_line: (width * (bpp / 8)) as u16,
            bnk_number_of_image_pages: 0,
            lin_number_of_image_pages: 0,
            lin_red_mask_size: 8,
            lin_red_field_position: 16,
            lin_green_mask_size: 8,
            lin_green_field_position: 8,
            lin_blue_mask_size: 8,
            lin_blue_field_position: 0,
            lin_rsvd_mask_size: 8,
            lin_rsvd_field_position: 24,
            max_pixel_clock: 0,
            reserved4: [0; 189],
        };
        info
    }

    /// Find mode by resolution
    pub fn find_mode(&self, width: u32, height: u32, bpp: u8) -> Option<&VesaModeEntry> {
        self.available_modes.iter().find(|m| {
            m.display_mode.width == width
                && m.display_mode.height == height
                && m.display_mode.bpp == bpp
        })
    }

    /// Find best matching mode
    pub fn find_best_mode(&self, min_width: u32, min_height: u32) -> Option<&VesaModeEntry> {
        self.available_modes.iter()
            .filter(|m| m.display_mode.width >= min_width && m.display_mode.height >= min_height)
            .min_by_key(|m| m.display_mode.width * m.display_mode.height)
    }

    /// Set video mode by mode number
    pub fn set_mode_by_number(&mut self, mode_number: u16) -> Result<(), DisplayError> {
        // Find mode entry
        let mode_entry = self.available_modes.iter()
            .find(|m| m.mode_number == mode_number)
            .ok_or(DisplayError::InvalidMode)?;

        let mode_info = mode_entry.mode_info;
        let display_mode = mode_entry.display_mode.clone();

        // Set the VBE mode (would call BIOS int 10h, function 4F02h)
        // In protected/long mode, we typically use the pre-set mode from bootloader
        // or use UEFI GOP

        self.current_mode = Some(mode_info);
        self.current_mode_number = mode_number;

        // Setup framebuffer
        self.setup_framebuffer(&display_mode, mode_info.phys_base_ptr as u64)?;

        Ok(())
    }

    /// Setup framebuffer for current mode
    fn setup_framebuffer(&mut self, mode: &DisplayMode, phys_addr: u64) -> Result<(), DisplayError> {
        let fb_size = mode.pitch as usize * mode.height as usize;

        // Map framebuffer memory
        let virt_addr = crate::memory::map_mmio(phys_addr, fb_size)
            .ok_or(DisplayError::OutOfMemory)?;

        // Create framebuffer
        let mut fb = Framebuffer::new(phys_addr, virt_addr, mode.clone());

        // Enable double buffering
        fb.enable_double_buffering();

        self.framebuffer = Some(fb);

        Ok(())
    }

    /// Get current mode info
    pub fn current_mode_info(&self) -> Option<&VbeModeInfo> {
        self.current_mode.as_ref()
    }

    /// Get total video memory in bytes
    pub fn total_video_memory(&self) -> usize {
        self.vbe_info.as_ref()
            .map(|info| (info.total_memory as usize) * 64 * 1024)
            .unwrap_or(0)
    }

    /// Check if linear framebuffer is available for mode
    pub fn is_lfb_available(&self, mode_number: u16) -> bool {
        self.available_modes.iter()
            .find(|m| m.mode_number == mode_number)
            .map(|m| (m.mode_info.mode_attributes & VbeModeAttributes::LINEAR_FB) != 0)
            .unwrap_or(false)
    }
}

impl DisplayDriver for VesaDriver {
    fn name(&self) -> &'static str {
        "VESA VBE"
    }

    fn get_modes(&self) -> Vec<DisplayMode> {
        self.available_modes.iter()
            .map(|m| m.display_mode.clone())
            .collect()
    }

    fn set_mode(&mut self, mode: &DisplayMode) -> Result<(), DisplayError> {
        // Find matching VBE mode
        let mode_entry = self.available_modes.iter()
            .find(|m| {
                m.display_mode.width == mode.width
                    && m.display_mode.height == mode.height
                    && m.display_mode.bpp == mode.bpp
            })
            .ok_or(DisplayError::InvalidMode)?;

        self.set_mode_by_number(mode_entry.mode_number)
    }

    fn current_mode(&self) -> Option<DisplayMode> {
        self.current_mode.as_ref().map(|info| DisplayMode {
            width: info.x_resolution as u32,
            height: info.y_resolution as u32,
            bpp: info.bits_per_pixel,
            pitch: info.bytes_per_scan_line as u32,
            format: PixelFormat::from_vbe_info(info),
            refresh_rate: 60, // VBE doesn't provide refresh rate
        })
    }

    fn framebuffer(&self) -> Option<&Framebuffer> {
        self.framebuffer.as_ref()
    }

    fn framebuffer_mut(&mut self) -> Option<&mut Framebuffer> {
        self.framebuffer.as_mut()
    }

    fn is_available(&self) -> bool {
        self.initialized
    }
}

impl VesaDriver {
    /// Check if the driver is initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Wait for vertical retrace
    pub fn vsync(&self) {
        // Wait for vertical retrace
        // Read Input Status Register 1 (port 0x3DA)
        unsafe {
            // Wait for vertical retrace to end
            while (inb(0x3DA) & 0x08) != 0 {}
            // Wait for vertical retrace to start
            while (inb(0x3DA) & 0x08) == 0 {}
        }
    }
}

impl PixelFormat {
    /// Create PixelFormat from VBE mode info
    pub fn from_vbe_info(info: &VbeModeInfo) -> Self {
        match info.bits_per_pixel {
            32 => {
                if info.red_field_position == 16 {
                    PixelFormat::Argb32
                } else if info.red_field_position == 0 {
                    PixelFormat::Rgba32
                } else {
                    PixelFormat::Argb32 // Default
                }
            }
            24 => PixelFormat::Rgb24,
            16 => PixelFormat::Rgb565,
            15 => PixelFormat::Rgb565, // Use RGB565 as fallback for RGB555
            8 => PixelFormat::Indexed8,
            _ => PixelFormat::Argb32,
        }
    }
}

/// EDID (Extended Display Identification Data) parsing
pub mod edid {
    /// EDID block size
    pub const EDID_BLOCK_SIZE: usize = 128;

    /// EDID header
    pub const EDID_HEADER: [u8; 8] = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];

    /// Parsed EDID data
    #[derive(Debug, Clone)]
    pub struct Edid {
        /// Manufacturer ID (3 chars)
        pub manufacturer: [char; 3],
        /// Product code
        pub product_code: u16,
        /// Serial number
        pub serial_number: u32,
        /// Week of manufacture
        pub week: u8,
        /// Year of manufacture
        pub year: u16,
        /// EDID version
        pub version: u8,
        /// EDID revision
        pub revision: u8,
        /// Maximum horizontal image size in cm
        pub max_h_size: u8,
        /// Maximum vertical image size in cm
        pub max_v_size: u8,
        /// Preferred timing mode
        pub preferred_width: u16,
        pub preferred_height: u16,
        pub preferred_refresh: u8,
    }

    impl Edid {
        /// Parse EDID data from raw bytes
        pub fn parse(data: &[u8; EDID_BLOCK_SIZE]) -> Option<Self> {
            // Verify header
            if data[0..8] != EDID_HEADER {
                return None;
            }

            // Verify checksum
            let checksum: u8 = data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
            if checksum != 0 {
                return None;
            }

            // Parse manufacturer ID (bytes 8-9, encoded as 5-bit chars)
            let mfg = ((data[8] as u16) << 8) | (data[9] as u16);
            let manufacturer = [
                ((mfg >> 10) & 0x1F) as u8 + b'A' - 1,
                ((mfg >> 5) & 0x1F) as u8 + b'A' - 1,
                (mfg & 0x1F) as u8 + b'A' - 1,
            ];

            // Product code (bytes 10-11, little endian)
            let product_code = (data[10] as u16) | ((data[11] as u16) << 8);

            // Serial number (bytes 12-15, little endian)
            let serial_number = (data[12] as u32)
                | ((data[13] as u32) << 8)
                | ((data[14] as u32) << 16)
                | ((data[15] as u32) << 24);

            // Week and year (bytes 16-17)
            let week = data[16];
            let year = 1990 + data[17] as u16;

            // Version and revision (bytes 18-19)
            let version = data[18];
            let revision = data[19];

            // Max image size (bytes 21-22)
            let max_h_size = data[21];
            let max_v_size = data[22];

            // Parse detailed timing descriptor for preferred mode
            let (preferred_width, preferred_height, preferred_refresh) =
                Self::parse_detailed_timing(&data[54..72]);

            Some(Self {
                manufacturer: [
                    manufacturer[0] as char,
                    manufacturer[1] as char,
                    manufacturer[2] as char,
                ],
                product_code,
                serial_number,
                week,
                year,
                version,
                revision,
                max_h_size,
                max_v_size,
                preferred_width,
                preferred_height,
                preferred_refresh,
            })
        }

        /// Parse detailed timing descriptor
        fn parse_detailed_timing(data: &[u8]) -> (u16, u16, u8) {
            if data.len() < 18 || (data[0] == 0 && data[1] == 0) {
                return (0, 0, 0);
            }

            // Pixel clock in 10 kHz units
            let pixel_clock = ((data[1] as u32) << 8) | (data[0] as u32);
            if pixel_clock == 0 {
                return (0, 0, 0);
            }

            // Horizontal active pixels
            let h_active = ((data[4] as u16 & 0xF0) << 4) | (data[2] as u16);

            // Vertical active lines
            let v_active = ((data[7] as u16 & 0xF0) << 4) | (data[5] as u16);

            // Horizontal blanking
            let h_blanking = ((data[4] as u16 & 0x0F) << 8) | (data[3] as u16);

            // Vertical blanking
            let v_blanking = ((data[7] as u16 & 0x0F) << 8) | (data[6] as u16);

            // Calculate refresh rate
            let total_pixels = (h_active + h_blanking) as u32 * (v_active + v_blanking) as u32;
            let refresh = if total_pixels > 0 {
                ((pixel_clock as u64 * 10000) / total_pixels as u64) as u8
            } else {
                60
            };

            (h_active, v_active, refresh)
        }
    }
}

/// Bochs/QEMU BGA (Bochs Graphics Adapter) support
pub mod bga {
    use crate::cpu::io::{inw, outw};

    /// BGA I/O ports
    const VBE_DISPI_IOPORT_INDEX: u16 = 0x01CE;
    const VBE_DISPI_IOPORT_DATA: u16 = 0x01CF;

    /// BGA register indices
    const VBE_DISPI_INDEX_ID: u16 = 0x0;
    const VBE_DISPI_INDEX_XRES: u16 = 0x1;
    const VBE_DISPI_INDEX_YRES: u16 = 0x2;
    const VBE_DISPI_INDEX_BPP: u16 = 0x3;
    const VBE_DISPI_INDEX_ENABLE: u16 = 0x4;
    const VBE_DISPI_INDEX_BANK: u16 = 0x5;
    const VBE_DISPI_INDEX_VIRT_WIDTH: u16 = 0x6;
    const VBE_DISPI_INDEX_VIRT_HEIGHT: u16 = 0x7;
    const VBE_DISPI_INDEX_X_OFFSET: u16 = 0x8;
    const VBE_DISPI_INDEX_Y_OFFSET: u16 = 0x9;

    /// BGA ID values
    const VBE_DISPI_ID0: u16 = 0xB0C0;
    const VBE_DISPI_ID1: u16 = 0xB0C1;
    const VBE_DISPI_ID2: u16 = 0xB0C2;
    const VBE_DISPI_ID3: u16 = 0xB0C3;
    const VBE_DISPI_ID4: u16 = 0xB0C4;
    const VBE_DISPI_ID5: u16 = 0xB0C5;

    /// BGA enable flags
    const VBE_DISPI_DISABLED: u16 = 0x00;
    const VBE_DISPI_ENABLED: u16 = 0x01;
    const VBE_DISPI_LFB_ENABLED: u16 = 0x40;
    const VBE_DISPI_NOCLEARMEM: u16 = 0x80;

    /// BGA framebuffer base address
    pub const BGA_LFB_ADDRESS: u64 = 0xE0000000;

    /// Write to BGA register
    fn write_register(index: u16, value: u16) {
        unsafe {
            outw(VBE_DISPI_IOPORT_INDEX, index);
            outw(VBE_DISPI_IOPORT_DATA, value);
        }
    }

    /// Read from BGA register
    fn read_register(index: u16) -> u16 {
        unsafe {
            outw(VBE_DISPI_IOPORT_INDEX, index);
            inw(VBE_DISPI_IOPORT_DATA)
        }
    }

    /// Check if BGA is available
    pub fn is_available() -> bool {
        let id = read_register(VBE_DISPI_INDEX_ID);
        id >= VBE_DISPI_ID0 && id <= VBE_DISPI_ID5
    }

    /// Get BGA version
    pub fn get_version() -> u16 {
        read_register(VBE_DISPI_INDEX_ID)
    }

    /// Set video mode
    pub fn set_mode(width: u16, height: u16, bpp: u16) -> bool {
        if !is_available() {
            return false;
        }

        // Disable display
        write_register(VBE_DISPI_INDEX_ENABLE, VBE_DISPI_DISABLED);

        // Set resolution
        write_register(VBE_DISPI_INDEX_XRES, width);
        write_register(VBE_DISPI_INDEX_YRES, height);
        write_register(VBE_DISPI_INDEX_BPP, bpp);

        // Enable display with LFB
        write_register(VBE_DISPI_INDEX_ENABLE, VBE_DISPI_ENABLED | VBE_DISPI_LFB_ENABLED);

        true
    }

    /// Get current resolution
    pub fn get_resolution() -> (u16, u16, u16) {
        (
            read_register(VBE_DISPI_INDEX_XRES),
            read_register(VBE_DISPI_INDEX_YRES),
            read_register(VBE_DISPI_INDEX_BPP),
        )
    }

    /// Set virtual resolution (for scrolling/panning)
    pub fn set_virtual_resolution(width: u16, height: u16) {
        write_register(VBE_DISPI_INDEX_VIRT_WIDTH, width);
        write_register(VBE_DISPI_INDEX_VIRT_HEIGHT, height);
    }

    /// Set display offset (for page flipping)
    pub fn set_offset(x: u16, y: u16) {
        write_register(VBE_DISPI_INDEX_X_OFFSET, x);
        write_register(VBE_DISPI_INDEX_Y_OFFSET, y);
    }

    /// Get framebuffer address
    pub fn get_framebuffer_address() -> u64 {
        BGA_LFB_ADDRESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pixel_format_from_vbe() {
        let mut info = VbeModeInfo {
            bits_per_pixel: 32,
            red_field_position: 16,
            green_field_position: 8,
            blue_field_position: 0,
            ..unsafe { core::mem::zeroed() }
        };

        assert_eq!(PixelFormat::from_vbe_info(&info), PixelFormat::Argb32);

        info.red_field_position = 0;
        info.blue_field_position = 16;
        assert_eq!(PixelFormat::from_vbe_info(&info), PixelFormat::RGBA32);
    }

    #[test]
    fn test_mode_info_creation() {
        let info = VesaDriver::create_mode_info(1920, 1080, 32);
        assert_eq!(info.x_resolution, 1920);
        assert_eq!(info.y_resolution, 1080);
        assert_eq!(info.bits_per_pixel, 32);
        assert_eq!(info.bytes_per_scan_line, 1920 * 4);
    }
}
