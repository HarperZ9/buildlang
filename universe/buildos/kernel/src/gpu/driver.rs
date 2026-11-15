//! GPU Driver Abstraction
//!
//! Trait definitions and driver implementations for various GPUs.

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use super::{
    GpuError, GpuVendor, GpuType, OutputType, ConnectorStatus,
    drm::{DrmDevice, DrmCaps, GammaRamp, VblankEvent},
    kms::{Crtc, Connector, Encoder, Plane, ModeInfo, Mode, PlaneType, EncoderType},
    gem::GemHandle,
    fb::DrmFramebuffer,
};

/// GPU capabilities
#[derive(Clone, Debug, Default)]
pub struct GpuCapabilities {
    /// Maximum resolution width
    pub max_width: u32,
    /// Maximum resolution height
    pub max_height: u32,
    /// Number of CRTCs
    pub num_crtcs: u32,
    /// Number of encoders
    pub num_encoders: u32,
    /// Number of connectors
    pub num_connectors: u32,
    /// Number of planes
    pub num_planes: u32,
    /// Supports cursor
    pub cursor: bool,
    /// Cursor width
    pub cursor_width: u32,
    /// Cursor height
    pub cursor_height: u32,
    /// Supports gamma
    pub gamma: bool,
    /// Gamma size
    pub gamma_size: u32,
    /// Supports page flip
    pub page_flip: bool,
    /// Supports async page flip
    pub async_page_flip: bool,
    /// Supports atomic modesetting
    pub atomic: bool,
    /// Supports universal planes
    pub universal_planes: bool,
    /// Supports overlay planes
    pub overlay_planes: u32,
    /// Supports 3D acceleration
    pub render_3d: bool,
    /// VRAM size
    pub vram_size: u64,
    /// GTT/GART size
    pub gtt_size: u64,
}

/// GPU driver trait
pub trait GpuDriver: Send + Sync {
    /// Get driver name
    fn name(&self) -> &str;

    /// Get driver description
    fn description(&self) -> &str;

    /// Get capabilities
    fn capabilities(&self) -> &GpuCapabilities;

    /// Set display mode
    fn mode_set(&self, crtc: &Crtc, mode: &ModeInfo) -> Result<(), GpuError>;

    /// Disable CRTC
    fn crtc_disable(&self, crtc: &Crtc) -> Result<(), GpuError>;

    /// Page flip
    fn page_flip(&self, crtc: &Crtc, fb: &DrmFramebuffer) -> Result<(), GpuError>;

    /// Wait for vblank
    fn wait_vblank(&self, crtc: &Crtc) -> Result<VblankEvent, GpuError>;

    /// Set gamma
    fn set_gamma(&self, crtc: &Crtc, gamma: &GammaRamp) -> Result<(), GpuError>;

    /// Set cursor
    fn set_cursor(&self, crtc: &Crtc, handle: GemHandle, width: u32, height: u32) -> Result<(), GpuError>;

    /// Move cursor
    fn move_cursor(&self, crtc: &Crtc, x: i32, y: i32) -> Result<(), GpuError>;

    /// Update plane
    fn update_plane(&self, plane: &Plane, fb: &DrmFramebuffer) -> Result<(), GpuError>;

    /// Disable plane
    fn disable_plane(&self, plane: &Plane) -> Result<(), GpuError>;

    /// Detect connector
    fn detect_connector(&self, connector: &mut Connector) -> ConnectorStatus;

    /// Get connector modes
    fn get_modes(&self, connector: &Connector) -> Vec<Mode>;

    /// Set DPMS state
    fn set_dpms(&self, connector: &mut Connector, state: super::DpmsState) -> Result<(), GpuError>;

    /// Handle interrupt
    fn handle_interrupt(&self) -> bool;

    /// Suspend
    fn suspend(&self) -> Result<(), GpuError> {
        Ok(())
    }

    /// Resume
    fn resume(&self) -> Result<(), GpuError> {
        Ok(())
    }
}

/// Intel GPU driver module
pub mod intel {
    use super::*;

    /// Intel GPU driver
    pub struct IntelDriver {
        caps: GpuCapabilities,
        mmio_base: usize,
        vram_base: u64,
        device_id: u16,
        gen: u8,
    }

    impl IntelDriver {
        pub fn new(mmio_base: usize, vram_base: u64, device_id: u16) -> Self {
            let gen = match device_id {
                0x0102..=0x0126 => 6, // Sandy Bridge
                0x0152..=0x016A => 7, // Ivy Bridge
                0x0402..=0x0416 => 7, // Haswell
                0x1602..=0x162B => 8, // Broadwell
                0x1902..=0x193D => 9, // Skylake
                0x5902..=0x593B => 9, // Kaby Lake
                0x3E90..=0x3EA9 => 9, // Coffee Lake
                0x9A40..=0x9AF8 => 12, // Tiger Lake
                0x4680..=0x46D2 => 12, // Alder Lake
                _ => 9,
            };

            Self {
                caps: GpuCapabilities {
                    max_width: 8192,
                    max_height: 8192,
                    num_crtcs: 3,
                    num_encoders: 3,
                    num_connectors: 4,
                    num_planes: 9,
                    cursor: true,
                    cursor_width: 256,
                    cursor_height: 256,
                    gamma: true,
                    gamma_size: 256,
                    page_flip: true,
                    async_page_flip: gen >= 9,
                    atomic: gen >= 9,
                    universal_planes: true,
                    overlay_planes: 3,
                    render_3d: true,
                    vram_size: 0, // Shared memory
                    gtt_size: 512 * 1024 * 1024,
                },
                mmio_base,
                vram_base,
                device_id,
                gen,
            }
        }
    }

    impl GpuDriver for IntelDriver {
        fn name(&self) -> &str {
            "i915"
        }

        fn description(&self) -> &str {
            "Intel Graphics"
        }

        fn capabilities(&self) -> &GpuCapabilities {
            &self.caps
        }

        fn mode_set(&self, _crtc: &Crtc, _mode: &ModeInfo) -> Result<(), GpuError> {
            // Program pipe timing registers
            // In a real driver, this would configure:
            // - HTOTAL, HBLANK, HSYNC
            // - VTOTAL, VBLANK, VSYNC
            // - PIPESRC, DSPSIZE, DSPPOS
            // - PLL configuration
            Ok(())
        }

        fn crtc_disable(&self, _crtc: &Crtc) -> Result<(), GpuError> {
            Ok(())
        }

        fn page_flip(&self, _crtc: &Crtc, _fb: &DrmFramebuffer) -> Result<(), GpuError> {
            // Update DSPSURF register with new buffer address
            Ok(())
        }

        fn wait_vblank(&self, crtc: &Crtc) -> Result<VblankEvent, GpuError> {
            // Wait for vertical blank interrupt
            Ok(VblankEvent {
                sequence: 0,
                timestamp_sec: 0,
                timestamp_usec: 0,
                crtc_id: crtc.id,
                user_data: 0,
            })
        }

        fn set_gamma(&self, _crtc: &Crtc, _gamma: &GammaRamp) -> Result<(), GpuError> {
            // Program LUT registers
            Ok(())
        }

        fn set_cursor(&self, _crtc: &Crtc, _handle: GemHandle, _width: u32, _height: u32) -> Result<(), GpuError> {
            // Program cursor registers (CURCNTR, CURBASE, etc.)
            Ok(())
        }

        fn move_cursor(&self, _crtc: &Crtc, _x: i32, _y: i32) -> Result<(), GpuError> {
            // Update CURPOS register
            Ok(())
        }

        fn update_plane(&self, _plane: &Plane, _fb: &DrmFramebuffer) -> Result<(), GpuError> {
            Ok(())
        }

        fn disable_plane(&self, _plane: &Plane) -> Result<(), GpuError> {
            Ok(())
        }

        fn detect_connector(&self, _connector: &mut Connector) -> ConnectorStatus {
            // Check HPD status
            ConnectorStatus::Connected
        }

        fn get_modes(&self, _connector: &Connector) -> Vec<Mode> {
            // Read EDID and parse modes
            super::super::kms::get_standard_modes()
                .into_iter()
                .map(|info| Mode {
                    info,
                    preferred: false,
                    status: super::super::kms::ModeStatus::Ok,
                })
                .collect()
        }

        fn set_dpms(&self, connector: &mut Connector, state: super::super::DpmsState) -> Result<(), GpuError> {
            connector.dpms = state;
            Ok(())
        }

        fn handle_interrupt(&self) -> bool {
            false
        }
    }

    /// Initialize Intel GPU
    pub fn init_device(bus: u8, slot: u8, func: u8, device_id: u16) -> Option<DrmDevice> {
        let bar0 = crate::drivers::pci::read_bar(bus, slot, func, 0)?;
        let bar2 = crate::drivers::pci::read_bar(bus, slot, func, 2)?;

        // Enable bus mastering
        crate::drivers::pci::enable_bus_master(bus, slot, func);
        crate::drivers::pci::enable_memory_space(bus, slot, func);

        // Map MMIO
        let mmio_base = crate::memory::map_mmio(bar0, 4 * 1024 * 1024)?;

        let driver = IntelDriver::new(mmio_base as usize, bar2, device_id);
        let caps = driver.capabilities().clone();

        let mut device = DrmDevice::new("card0", "i915");
        device.vendor = GpuVendor::Intel;
        device.device_type = GpuType::Integrated;
        device.pci_location = (bus, slot, func);
        device.pci_device_id = device_id;
        device.driver_desc = String::from("Intel Graphics");
        device.mmio_base = bar0;
        device.mmio_size = 4 * 1024 * 1024;
        device.vram_base = bar2;
        device.gart_size = caps.gtt_size as usize;

        device.caps = DrmCaps {
            dumb_buffer: true,
            dumb_preferred_depth: 24,
            cursor_width: caps.cursor_width,
            cursor_height: caps.cursor_height,
            prime: true,
            async_page_flip: caps.async_page_flip,
            ..Default::default()
        };

        // Add CRTCs
        for i in 0..caps.num_crtcs {
            let crtc = Crtc::new(i + 1);
            device.add_crtc(crtc);
        }

        // Add encoders
        device.add_encoder(Encoder::new(1, EncoderType::Dac));
        device.add_encoder(Encoder::new(2, EncoderType::Tmds));
        device.add_encoder(Encoder::new(3, EncoderType::Lvds));

        // Add connectors
        let mut vga = Connector::new(1, OutputType::Vga);
        vga.encoder_ids = alloc::vec![1];
        device.add_connector(vga);

        let mut hdmi = Connector::new(2, OutputType::Hdmi);
        hdmi.encoder_ids = alloc::vec![2];
        hdmi.status = ConnectorStatus::Connected;
        device.add_connector(hdmi);

        let mut edp = Connector::new(3, OutputType::Edp);
        edp.encoder_ids = alloc::vec![3];
        edp.status = ConnectorStatus::Connected;
        device.add_connector(edp);

        // Add planes
        for i in 0..caps.num_crtcs {
            let primary = Plane::new(i * 3 + 1, PlaneType::Primary);
            device.add_plane(primary);
            let cursor = Plane::new(i * 3 + 2, PlaneType::Cursor);
            device.add_plane(cursor);
            let overlay = Plane::new(i * 3 + 3, PlaneType::Overlay);
            device.add_plane(overlay);
        }

        device.set_driver(Box::new(driver));
        Some(device)
    }
}

/// AMD GPU driver module
pub mod amd {
    use super::*;

    /// AMD GPU driver
    pub struct AmdDriver {
        caps: GpuCapabilities,
        mmio_base: usize,
        vram_base: u64,
        vram_size: u64,
        device_id: u16,
        family: AmdFamily,
    }

    #[derive(Clone, Copy, Debug)]
    pub enum AmdFamily {
        Si,         // Southern Islands (GCN 1.0)
        Ci,         // Sea Islands (GCN 2.0)
        Vi,         // Volcanic Islands (GCN 3.0)
        Ai,         // Arctic Islands / Vega
        Navi,       // RDNA
        Navi2,      // RDNA 2
        Navi3,      // RDNA 3
    }

    impl AmdDriver {
        pub fn new(mmio_base: usize, vram_base: u64, vram_size: u64, device_id: u16) -> Self {
            let family = match device_id {
                0x6780..=0x679F => AmdFamily::Si,
                0x6600..=0x66FF => AmdFamily::Ci,
                0x67C0..=0x67DF => AmdFamily::Vi,
                0x6860..=0x687F => AmdFamily::Ai,
                0x73A0..=0x73EF => AmdFamily::Navi2,
                0x7310..=0x73FF => AmdFamily::Navi,
                0x7400..=0x74FF => AmdFamily::Navi3,
                _ => AmdFamily::Navi,
            };

            Self {
                caps: GpuCapabilities {
                    max_width: 16384,
                    max_height: 16384,
                    num_crtcs: 6,
                    num_encoders: 6,
                    num_connectors: 6,
                    num_planes: 18,
                    cursor: true,
                    cursor_width: 256,
                    cursor_height: 256,
                    gamma: true,
                    gamma_size: 4096,
                    page_flip: true,
                    async_page_flip: true,
                    atomic: true,
                    universal_planes: true,
                    overlay_planes: 6,
                    render_3d: true,
                    vram_size,
                    gtt_size: 1024 * 1024 * 1024,
                },
                mmio_base,
                vram_base,
                vram_size,
                device_id,
                family,
            }
        }
    }

    impl GpuDriver for AmdDriver {
        fn name(&self) -> &str {
            "amdgpu"
        }

        fn description(&self) -> &str {
            "AMD Graphics"
        }

        fn capabilities(&self) -> &GpuCapabilities {
            &self.caps
        }

        fn mode_set(&self, _crtc: &Crtc, _mode: &ModeInfo) -> Result<(), GpuError> {
            Ok(())
        }

        fn crtc_disable(&self, _crtc: &Crtc) -> Result<(), GpuError> {
            Ok(())
        }

        fn page_flip(&self, _crtc: &Crtc, _fb: &DrmFramebuffer) -> Result<(), GpuError> {
            Ok(())
        }

        fn wait_vblank(&self, crtc: &Crtc) -> Result<VblankEvent, GpuError> {
            Ok(VblankEvent {
                sequence: 0,
                timestamp_sec: 0,
                timestamp_usec: 0,
                crtc_id: crtc.id,
                user_data: 0,
            })
        }

        fn set_gamma(&self, _crtc: &Crtc, _gamma: &GammaRamp) -> Result<(), GpuError> {
            Ok(())
        }

        fn set_cursor(&self, _crtc: &Crtc, _handle: GemHandle, _width: u32, _height: u32) -> Result<(), GpuError> {
            Ok(())
        }

        fn move_cursor(&self, _crtc: &Crtc, _x: i32, _y: i32) -> Result<(), GpuError> {
            Ok(())
        }

        fn update_plane(&self, _plane: &Plane, _fb: &DrmFramebuffer) -> Result<(), GpuError> {
            Ok(())
        }

        fn disable_plane(&self, _plane: &Plane) -> Result<(), GpuError> {
            Ok(())
        }

        fn detect_connector(&self, _connector: &mut Connector) -> ConnectorStatus {
            ConnectorStatus::Connected
        }

        fn get_modes(&self, _connector: &Connector) -> Vec<Mode> {
            super::super::kms::get_standard_modes()
                .into_iter()
                .map(|info| Mode {
                    info,
                    preferred: false,
                    status: super::super::kms::ModeStatus::Ok,
                })
                .collect()
        }

        fn set_dpms(&self, connector: &mut Connector, state: super::super::DpmsState) -> Result<(), GpuError> {
            connector.dpms = state;
            Ok(())
        }

        fn handle_interrupt(&self) -> bool {
            false
        }
    }

    pub fn init_device(bus: u8, slot: u8, func: u8, device_id: u16) -> Option<DrmDevice> {
        let bar0 = crate::drivers::pci::read_bar(bus, slot, func, 0)?;
        let _bar2 = crate::drivers::pci::read_bar(bus, slot, func, 2)?;
        let bar5 = crate::drivers::pci::read_bar(bus, slot, func, 5).unwrap_or(0);

        crate::drivers::pci::enable_bus_master(bus, slot, func);
        crate::drivers::pci::enable_memory_space(bus, slot, func);

        let mmio_base = crate::memory::map_mmio(bar5, 512 * 1024)?;
        let vram_size = 1024 * 1024 * 1024u64; // 1GB VRAM assumed

        let driver = AmdDriver::new(mmio_base as usize, bar0, vram_size, device_id);
        let caps = driver.capabilities().clone();

        let mut device = DrmDevice::new("card0", "amdgpu");
        device.vendor = GpuVendor::Amd;
        device.device_type = GpuType::Discrete;
        device.pci_location = (bus, slot, func);
        device.pci_device_id = device_id;
        device.driver_desc = String::from("AMD Graphics");
        device.mmio_base = bar5;
        device.vram_base = bar0;
        device.vram_size = vram_size as usize;

        device.caps = DrmCaps {
            dumb_buffer: true,
            dumb_preferred_depth: 24,
            cursor_width: 256,
            cursor_height: 256,
            prime: true,
            async_page_flip: true,
            ..Default::default()
        };

        // Add CRTCs, encoders, connectors, planes
        for i in 0..caps.num_crtcs {
            device.add_crtc(Crtc::new(i + 1));
        }

        device.add_encoder(Encoder::new(1, EncoderType::Tmds));
        device.add_encoder(Encoder::new(2, EncoderType::Tmds));

        let mut hdmi1 = Connector::new(1, OutputType::Hdmi);
        hdmi1.encoder_ids = alloc::vec![1];
        hdmi1.status = ConnectorStatus::Connected;
        device.add_connector(hdmi1);

        let mut dp1 = Connector::new(2, OutputType::DisplayPort);
        dp1.encoder_ids = alloc::vec![2];
        device.add_connector(dp1);

        device.set_driver(Box::new(driver));
        Some(device)
    }
}

/// VirtIO GPU driver module
pub mod virtio {
    use super::*;

    pub struct VirtioGpuDriver {
        caps: GpuCapabilities,
    }

    impl VirtioGpuDriver {
        pub fn new() -> Self {
            Self {
                caps: GpuCapabilities {
                    max_width: 4096,
                    max_height: 4096,
                    num_crtcs: 1,
                    num_encoders: 1,
                    num_connectors: 1,
                    num_planes: 1,
                    cursor: true,
                    cursor_width: 64,
                    cursor_height: 64,
                    gamma: false,
                    gamma_size: 0,
                    page_flip: true,
                    async_page_flip: false,
                    atomic: false,
                    universal_planes: true,
                    overlay_planes: 0,
                    render_3d: false,
                    vram_size: 0,
                    gtt_size: 256 * 1024 * 1024,
                },
            }
        }
    }

    impl GpuDriver for VirtioGpuDriver {
        fn name(&self) -> &str { "virtio-gpu" }
        fn description(&self) -> &str { "VirtIO GPU" }
        fn capabilities(&self) -> &GpuCapabilities { &self.caps }
        fn mode_set(&self, _crtc: &Crtc, _mode: &ModeInfo) -> Result<(), GpuError> { Ok(()) }
        fn crtc_disable(&self, _crtc: &Crtc) -> Result<(), GpuError> { Ok(()) }
        fn page_flip(&self, _crtc: &Crtc, _fb: &DrmFramebuffer) -> Result<(), GpuError> { Ok(()) }
        fn wait_vblank(&self, crtc: &Crtc) -> Result<VblankEvent, GpuError> {
            Ok(VblankEvent { sequence: 0, timestamp_sec: 0, timestamp_usec: 0, crtc_id: crtc.id, user_data: 0 })
        }
        fn set_gamma(&self, _crtc: &Crtc, _gamma: &GammaRamp) -> Result<(), GpuError> { Err(GpuError::NotSupported) }
        fn set_cursor(&self, _crtc: &Crtc, _handle: GemHandle, _w: u32, _h: u32) -> Result<(), GpuError> { Ok(()) }
        fn move_cursor(&self, _crtc: &Crtc, _x: i32, _y: i32) -> Result<(), GpuError> { Ok(()) }
        fn update_plane(&self, _plane: &Plane, _fb: &DrmFramebuffer) -> Result<(), GpuError> { Ok(()) }
        fn disable_plane(&self, _plane: &Plane) -> Result<(), GpuError> { Ok(()) }
        fn detect_connector(&self, _connector: &mut Connector) -> ConnectorStatus { ConnectorStatus::Connected }
        fn get_modes(&self, _connector: &Connector) -> Vec<Mode> { Vec::new() }
        fn set_dpms(&self, connector: &mut Connector, state: super::super::DpmsState) -> Result<(), GpuError> {
            connector.dpms = state;
            Ok(())
        }
        fn handle_interrupt(&self) -> bool { false }
    }

    pub fn init_device(bus: u8, slot: u8, func: u8) -> Option<DrmDevice> {
        let mut device = DrmDevice::new("card0", "virtio-gpu");
        device.vendor = GpuVendor::VirtIO;
        device.device_type = GpuType::Virtual;
        device.pci_location = (bus, slot, func);
        device.driver_desc = String::from("VirtIO GPU");

        device.caps = DrmCaps {
            dumb_buffer: true,
            dumb_preferred_depth: 32,
            ..Default::default()
        };

        device.add_crtc(Crtc::new(1));
        device.add_encoder(Encoder::new(1, EncoderType::Virtual));

        let mut conn = Connector::new(1, OutputType::Virtual);
        conn.encoder_ids = alloc::vec![1];
        conn.status = ConnectorStatus::Connected;
        device.add_connector(conn);

        device.set_driver(Box::new(VirtioGpuDriver::new()));
        Some(device)
    }
}

/// Bochs/QEMU VGA driver module
pub mod bochs {
    use super::*;

    pub struct BochsDriver {
        caps: GpuCapabilities,
        mmio_base: usize,
        fb_base: u64,
    }

    impl BochsDriver {
        pub fn new(mmio_base: usize, fb_base: u64) -> Self {
            Self {
                caps: GpuCapabilities {
                    max_width: 2560,
                    max_height: 1600,
                    num_crtcs: 1,
                    num_encoders: 1,
                    num_connectors: 1,
                    num_planes: 1,
                    cursor: true,
                    cursor_width: 64,
                    cursor_height: 64,
                    gamma: false,
                    gamma_size: 0,
                    page_flip: false,
                    async_page_flip: false,
                    atomic: false,
                    universal_planes: false,
                    overlay_planes: 0,
                    render_3d: false,
                    vram_size: 16 * 1024 * 1024,
                    gtt_size: 0,
                },
                mmio_base,
                fb_base,
            }
        }
    }

    impl GpuDriver for BochsDriver {
        fn name(&self) -> &str { "bochs-drm" }
        fn description(&self) -> &str { "Bochs VBE VGA" }
        fn capabilities(&self) -> &GpuCapabilities { &self.caps }
        fn mode_set(&self, _crtc: &Crtc, _mode: &ModeInfo) -> Result<(), GpuError> { Ok(()) }
        fn crtc_disable(&self, _crtc: &Crtc) -> Result<(), GpuError> { Ok(()) }
        fn page_flip(&self, _crtc: &Crtc, _fb: &DrmFramebuffer) -> Result<(), GpuError> { Ok(()) }
        fn wait_vblank(&self, crtc: &Crtc) -> Result<VblankEvent, GpuError> {
            Ok(VblankEvent { sequence: 0, timestamp_sec: 0, timestamp_usec: 0, crtc_id: crtc.id, user_data: 0 })
        }
        fn set_gamma(&self, _crtc: &Crtc, _gamma: &GammaRamp) -> Result<(), GpuError> { Err(GpuError::NotSupported) }
        fn set_cursor(&self, _crtc: &Crtc, _handle: GemHandle, _w: u32, _h: u32) -> Result<(), GpuError> { Ok(()) }
        fn move_cursor(&self, _crtc: &Crtc, _x: i32, _y: i32) -> Result<(), GpuError> { Ok(()) }
        fn update_plane(&self, _plane: &Plane, _fb: &DrmFramebuffer) -> Result<(), GpuError> { Ok(()) }
        fn disable_plane(&self, _plane: &Plane) -> Result<(), GpuError> { Ok(()) }
        fn detect_connector(&self, _connector: &mut Connector) -> ConnectorStatus { ConnectorStatus::Connected }
        fn get_modes(&self, _connector: &Connector) -> Vec<Mode> {
            super::super::kms::get_standard_modes()
                .into_iter()
                .map(|info| Mode { info, preferred: false, status: super::super::kms::ModeStatus::Ok })
                .collect()
        }
        fn set_dpms(&self, connector: &mut Connector, state: super::super::DpmsState) -> Result<(), GpuError> {
            connector.dpms = state;
            Ok(())
        }
        fn handle_interrupt(&self) -> bool { false }
    }

    pub fn init_device(bus: u8, slot: u8, func: u8) -> Option<DrmDevice> {
        let bar0 = crate::drivers::pci::read_bar(bus, slot, func, 0)?;
        let bar2 = crate::drivers::pci::read_bar(bus, slot, func, 2)?;

        let mmio_base = crate::memory::map_mmio(bar2, 4096)?;

        let mut device = DrmDevice::new("card0", "bochs-drm");
        device.vendor = GpuVendor::Bochs;
        device.device_type = GpuType::Virtual;
        device.pci_location = (bus, slot, func);
        device.driver_desc = String::from("Bochs VBE VGA");
        device.vram_base = bar0;
        device.vram_size = 16 * 1024 * 1024;

        device.caps = DrmCaps {
            dumb_buffer: true,
            dumb_preferred_depth: 32,
            ..Default::default()
        };

        device.add_crtc(Crtc::new(1));
        device.add_encoder(Encoder::new(1, EncoderType::Virtual));

        let mut conn = Connector::new(1, OutputType::Virtual);
        conn.encoder_ids = alloc::vec![1];
        conn.status = ConnectorStatus::Connected;
        device.add_connector(conn);

        device.set_driver(Box::new(BochsDriver::new(mmio_base as usize, bar0)));
        Some(device)
    }
}

/// VMware SVGA driver module
pub mod vmware {
    use super::*;

    pub fn init_device(bus: u8, slot: u8, func: u8) -> Option<DrmDevice> {
        // VMware SVGA is similar to Bochs
        bochs::init_device(bus, slot, func).map(|mut dev| {
            dev.name = String::from("vmwgfx");
            dev.driver_name = String::from("vmwgfx");
            dev.driver_desc = String::from("VMware SVGA");
            dev.vendor = GpuVendor::VMware;
            dev
        })
    }
}
