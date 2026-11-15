//! GPU/DRM Subsystem
//!
//! Implements the Direct Rendering Manager (DRM) infrastructure for GPU management:
//! - DRM core with device management
//! - KMS (Kernel Mode Setting) for display control
//! - GEM (Graphics Execution Manager) for buffer management
//! - GPU driver abstraction layer
//! - Framebuffer integration

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::RwLock;

pub mod drm;
pub mod kms;
pub mod gem;
pub mod driver;
pub mod fb;

pub use drm::{DrmDevice, DrmFile, DrmIoctl};
pub use kms::{Connector, Crtc, Encoder, Plane, Mode, ModeInfo};
pub use gem::{GemObject, GemHandle};
pub use driver::{GpuDriver, GpuCapabilities};
pub use fb::{DrmFramebuffer, PixelFormat};

/// GPU subsystem errors
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuError {
    /// No GPU device found
    NoDevice,
    /// Device is busy
    DeviceBusy,
    /// Invalid parameter
    InvalidParameter,
    /// Invalid mode
    InvalidMode,
    /// Out of memory
    NoMemory,
    /// Permission denied
    PermissionDenied,
    /// Operation not supported
    NotSupported,
    /// Resource not found
    NotFound,
    /// Hardware error
    HardwareError,
    /// Timeout
    Timeout,
    /// Buffer too small
    BufferTooSmall,
    /// Invalid handle
    InvalidHandle,
    /// Invalid object
    InvalidObject,
    /// Mode setting failed
    ModeFailed,
    /// EDID read failed
    EdidError,
    /// Connector not connected
    NotConnected,
    /// Invalid CRTC
    InvalidCrtc,
    /// DPMS error
    DpmsError,
}

/// GPU vendor IDs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuVendor {
    Intel,
    Amd,
    Nvidia,
    VirtIO,
    Bochs,
    VMware,
    QXL,
    Unknown(u16),
}

impl From<u16> for GpuVendor {
    fn from(vendor: u16) -> Self {
        match vendor {
            0x8086 => GpuVendor::Intel,
            0x1002 => GpuVendor::Amd,
            0x10DE => GpuVendor::Nvidia,
            0x1AF4 => GpuVendor::VirtIO,
            0x1234 => GpuVendor::Bochs,
            0x15AD => GpuVendor::VMware,
            0x1B36 => GpuVendor::QXL,
            other => GpuVendor::Unknown(other),
        }
    }
}

/// GPU device type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuType {
    /// Integrated GPU
    Integrated,
    /// Discrete GPU
    Discrete,
    /// Virtual GPU (VM)
    Virtual,
}

/// Display output type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputType {
    /// Unknown output
    Unknown,
    /// VGA output
    Vga,
    /// DVI output
    Dvi,
    /// DVII (DVI integrated)
    DviI,
    /// DVID (DVI digital)
    DviD,
    /// DVIA (DVI analog)
    DviA,
    /// Composite video
    Composite,
    /// S-Video
    SVideo,
    /// LVDS (laptop display)
    Lvds,
    /// Component video
    Component,
    /// DisplayPort
    DisplayPort,
    /// HDMI
    Hdmi,
    /// Mini DisplayPort
    MiniDisplayPort,
    /// eDP (embedded DisplayPort)
    Edp,
    /// Virtual output
    Virtual,
    /// Writeback connector
    Writeback,
}

/// Connector status
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectorStatus {
    /// Connector is connected
    Connected,
    /// Connector is disconnected
    Disconnected,
    /// Connection status unknown
    Unknown,
}

/// DPMS state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DpmsState {
    /// Display on
    On,
    /// Display standby
    Standby,
    /// Display suspend
    Suspend,
    /// Display off
    Off,
}

/// Color depth
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorDepth {
    /// 8-bit color (256 colors)
    Depth8,
    /// 15-bit color (32768 colors)
    Depth15,
    /// 16-bit color (65536 colors)
    Depth16,
    /// 24-bit color (16M colors)
    Depth24,
    /// 30-bit color (1B colors)
    Depth30,
    /// 32-bit color (with alpha)
    Depth32,
}

impl ColorDepth {
    pub fn bits_per_pixel(&self) -> u32 {
        match self {
            ColorDepth::Depth8 => 8,
            ColorDepth::Depth15 => 16,
            ColorDepth::Depth16 => 16,
            ColorDepth::Depth24 => 24,
            ColorDepth::Depth30 => 32,
            ColorDepth::Depth32 => 32,
        }
    }

    pub fn bytes_per_pixel(&self) -> u32 {
        (self.bits_per_pixel() + 7) / 8
    }
}

/// GPU statistics
#[derive(Debug, Default)]
pub struct GpuStats {
    /// Frames rendered
    pub frames_rendered: AtomicU64,
    /// Mode sets performed
    pub mode_sets: AtomicU64,
    /// Page flips performed
    pub page_flips: AtomicU64,
    /// VBlank interrupts
    pub vblank_count: AtomicU64,
    /// GEM objects allocated
    pub gem_objects: AtomicU32,
    /// GEM memory allocated (bytes)
    pub gem_memory: AtomicU64,
}

/// GPU subsystem
pub struct GpuSubsystem {
    /// Is initialized
    initialized: AtomicBool,
    /// DRM devices
    devices: RwLock<BTreeMap<u32, Arc<RwLock<DrmDevice>>>>,
    /// Next device minor
    next_minor: AtomicU32,
    /// Primary GPU device
    primary_device: RwLock<Option<u32>>,
    /// Render-only devices
    render_devices: RwLock<Vec<u32>>,
    /// Statistics
    stats: GpuStats,
}

impl GpuSubsystem {
    /// Create new GPU subsystem
    pub const fn new() -> Self {
        Self {
            initialized: AtomicBool::new(false),
            devices: RwLock::new(BTreeMap::new()),
            next_minor: AtomicU32::new(0),
            primary_device: RwLock::new(None),
            render_devices: RwLock::new(Vec::new()),
            stats: GpuStats {
                frames_rendered: AtomicU64::new(0),
                mode_sets: AtomicU64::new(0),
                page_flips: AtomicU64::new(0),
                vblank_count: AtomicU64::new(0),
                gem_objects: AtomicU32::new(0),
                gem_memory: AtomicU64::new(0),
            },
        }
    }

    /// Initialize GPU subsystem
    pub fn init(&self) -> Result<(), GpuError> {
        // Probe PCI for GPU devices
        self.probe_pci_devices();

        self.initialized.store(true, Ordering::Release);

        let devices = self.devices.read();
        crate::kprintln!("[GPU] DRM subsystem initialized, {} device(s)", devices.len());

        Ok(())
    }

    /// Probe PCI bus for GPU devices
    fn probe_pci_devices(&self) {
        let pci_devices = crate::drivers::pci::get_devices();

        for (bus, slot, func, vendor, device, class, subclass, _) in pci_devices {
            // Display controller class = 0x03
            if class == 0x03 {
                match subclass {
                    0x00 => {
                        // VGA compatible controller
                        self.try_init_gpu(bus, slot, func, vendor, device);
                    }
                    0x01 => {
                        // XGA compatible controller
                        self.try_init_gpu(bus, slot, func, vendor, device);
                    }
                    0x02 => {
                        // 3D controller (render only)
                        self.try_init_render_gpu(bus, slot, func, vendor, device);
                    }
                    0x80 => {
                        // Other display controller
                        self.try_init_gpu(bus, slot, func, vendor, device);
                    }
                    _ => {}
                }
            }
        }
    }

    /// Try to initialize a GPU
    fn try_init_gpu(&self, bus: u8, slot: u8, func: u8, vendor: u16, device: u16) {
        let gpu_vendor = GpuVendor::from(vendor);

        let drm_device = match gpu_vendor {
            GpuVendor::Intel => {
                driver::intel::init_device(bus, slot, func, device)
            }
            GpuVendor::Amd => {
                driver::amd::init_device(bus, slot, func, device)
            }
            GpuVendor::VirtIO => {
                driver::virtio::init_device(bus, slot, func)
            }
            GpuVendor::Bochs | GpuVendor::QXL => {
                driver::bochs::init_device(bus, slot, func)
            }
            GpuVendor::VMware => {
                driver::vmware::init_device(bus, slot, func)
            }
            _ => None,
        };

        if let Some(device) = drm_device {
            self.register_device(device, true);
        }
    }

    /// Try to initialize a render-only GPU
    fn try_init_render_gpu(&self, bus: u8, slot: u8, func: u8, vendor: u16, device: u16) {
        // For now, treat render GPUs same as display GPUs
        self.try_init_gpu(bus, slot, func, vendor, device);
    }

    /// Register a DRM device
    pub fn register_device(&self, mut device: DrmDevice, is_primary: bool) -> u32 {
        let minor = self.next_minor.fetch_add(1, Ordering::SeqCst);
        device.minor = minor;

        crate::kprintln!("[GPU] DRM device {}: {} ({})",
            minor, device.name, device.driver_name);

        let device_arc = Arc::new(RwLock::new(device));
        self.devices.write().insert(minor, device_arc);

        if is_primary {
            let mut primary = self.primary_device.write();
            if primary.is_none() {
                *primary = Some(minor);
            }
        }

        minor
    }

    /// Unregister a DRM device
    pub fn unregister_device(&self, minor: u32) {
        self.devices.write().remove(&minor);

        let mut primary = self.primary_device.write();
        if *primary == Some(minor) {
            *primary = None;
        }
    }

    /// Get DRM device by minor number
    pub fn get_device(&self, minor: u32) -> Option<Arc<RwLock<DrmDevice>>> {
        self.devices.read().get(&minor).cloned()
    }

    /// Get primary DRM device
    pub fn get_primary_device(&self) -> Option<Arc<RwLock<DrmDevice>>> {
        let minor = (*self.primary_device.read())?;
        self.get_device(minor)
    }

    /// Get all DRM devices
    pub fn get_devices(&self) -> Vec<Arc<RwLock<DrmDevice>>> {
        self.devices.read().values().cloned().collect()
    }

    /// Get device count
    pub fn device_count(&self) -> usize {
        self.devices.read().len()
    }

    /// Open a DRM device
    pub fn open(&self, minor: u32) -> Result<Arc<RwLock<DrmFile>>, GpuError> {
        let device = self.get_device(minor).ok_or(GpuError::NoDevice)?;
        let file = DrmFile::new(device);
        Ok(Arc::new(RwLock::new(file)))
    }

    /// Record frame rendered
    pub fn record_frame(&self) {
        self.stats.frames_rendered.fetch_add(1, Ordering::Relaxed);
    }

    /// Record mode set
    pub fn record_mode_set(&self) {
        self.stats.mode_sets.fetch_add(1, Ordering::Relaxed);
    }

    /// Record page flip
    pub fn record_page_flip(&self) {
        self.stats.page_flips.fetch_add(1, Ordering::Relaxed);
    }

    /// Record vblank
    pub fn record_vblank(&self) {
        self.stats.vblank_count.fetch_add(1, Ordering::Relaxed);
    }
}

/// Global GPU subsystem
static GPU: GpuSubsystem = GpuSubsystem::new();

/// Initialize GPU subsystem
pub fn init() {
    if let Err(e) = GPU.init() {
        crate::kprintln!("[GPU] Initialization failed: {:?}", e);
    }
}

/// Get DRM device
pub fn get_device(minor: u32) -> Option<Arc<RwLock<DrmDevice>>> {
    GPU.get_device(minor)
}

/// Get primary device
pub fn get_primary_device() -> Option<Arc<RwLock<DrmDevice>>> {
    GPU.get_primary_device()
}

/// Get device count
pub fn device_count() -> usize {
    GPU.device_count()
}

/// Open DRM device
pub fn open(minor: u32) -> Result<Arc<RwLock<DrmFile>>, GpuError> {
    GPU.open(minor)
}

/// Register DRM device
pub fn register_device(device: DrmDevice, is_primary: bool) -> u32 {
    GPU.register_device(device, is_primary)
}
