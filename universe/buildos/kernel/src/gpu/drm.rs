//! DRM (Direct Rendering Manager) Core
//!
//! Core DRM device and file abstractions.

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::RwLock;

use super::{
    GpuError, GpuVendor, GpuType,
    kms::{Connector, Crtc, Encoder, Plane, ModeInfo},
    gem::{GemObject, GemHandle, GemManager},
    driver::GpuDriver,
    fb::DrmFramebuffer,
};

/// DRM device capabilities
#[derive(Clone, Copy, Debug, Default)]
pub struct DrmCaps {
    /// Supports dumb buffers
    pub dumb_buffer: bool,
    /// Supports vblank high CRTC
    pub vblank_high_crtc: bool,
    /// Supports dumb buffer preferred depth
    pub dumb_preferred_depth: u32,
    /// Supports dumb buffer prefer shadow
    pub dumb_prefer_shadow: bool,
    /// Supports prime (fd passing)
    pub prime: bool,
    /// Supports async page flip
    pub async_page_flip: bool,
    /// Supports cursor width
    pub cursor_width: u32,
    /// Supports cursor height
    pub cursor_height: u32,
    /// Supports addfb2 modifiers
    pub addfb2_modifiers: bool,
    /// Supports page flip target
    pub page_flip_target: bool,
    /// Supports CRTC in vblank event
    pub crtc_in_vblank_event: bool,
    /// Supports syncobj
    pub syncobj: bool,
    /// Supports syncobj timeline
    pub syncobj_timeline: bool,
}

/// DRM device
pub struct DrmDevice {
    /// Minor device number
    pub minor: u32,
    /// Device name
    pub name: String,
    /// Driver name
    pub driver_name: String,
    /// Driver description
    pub driver_desc: String,
    /// Driver version (major, minor, patchlevel)
    pub driver_version: (u32, u32, u32),
    /// Driver date
    pub driver_date: String,
    /// Vendor
    pub vendor: GpuVendor,
    /// Device type
    pub device_type: GpuType,
    /// PCI bus/slot/function
    pub pci_location: (u8, u8, u8),
    /// PCI device ID
    pub pci_device_id: u16,
    /// Capabilities
    pub caps: DrmCaps,
    /// Is master (has modesetting rights)
    pub is_master: AtomicBool,
    /// Open count
    pub open_count: AtomicU32,
    /// Connectors
    connectors: RwLock<Vec<Arc<RwLock<Connector>>>>,
    /// CRTCs
    crtcs: RwLock<Vec<Arc<RwLock<Crtc>>>>,
    /// Encoders
    encoders: RwLock<Vec<Arc<RwLock<Encoder>>>>,
    /// Planes
    planes: RwLock<Vec<Arc<RwLock<Plane>>>>,
    /// Framebuffers
    framebuffers: RwLock<BTreeMap<u32, Arc<RwLock<DrmFramebuffer>>>>,
    /// Next framebuffer ID
    next_fb_id: AtomicU32,
    /// GEM manager
    gem_manager: Arc<RwLock<GemManager>>,
    /// Driver implementation
    driver: Option<Box<dyn GpuDriver>>,
    /// MMIO base address
    pub mmio_base: u64,
    /// MMIO size
    pub mmio_size: usize,
    /// Video memory base
    pub vram_base: u64,
    /// Video memory size
    pub vram_size: usize,
    /// GTT/GART size
    pub gart_size: usize,
    /// IRQ number
    pub irq: u32,
}

impl DrmDevice {
    /// Create new DRM device
    pub fn new(name: &str, driver_name: &str) -> Self {
        Self {
            minor: 0,
            name: String::from(name),
            driver_name: String::from(driver_name),
            driver_desc: String::new(),
            driver_version: (1, 0, 0),
            driver_date: String::from("20250101"),
            vendor: GpuVendor::Unknown(0),
            device_type: GpuType::Discrete,
            pci_location: (0, 0, 0),
            pci_device_id: 0,
            caps: DrmCaps::default(),
            is_master: AtomicBool::new(false),
            open_count: AtomicU32::new(0),
            connectors: RwLock::new(Vec::new()),
            crtcs: RwLock::new(Vec::new()),
            encoders: RwLock::new(Vec::new()),
            planes: RwLock::new(Vec::new()),
            framebuffers: RwLock::new(BTreeMap::new()),
            next_fb_id: AtomicU32::new(1),
            gem_manager: Arc::new(RwLock::new(GemManager::new())),
            driver: None,
            mmio_base: 0,
            mmio_size: 0,
            vram_base: 0,
            vram_size: 0,
            gart_size: 0,
            irq: 0,
        }
    }

    /// Set driver implementation
    pub fn set_driver(&mut self, driver: Box<dyn GpuDriver>) {
        self.driver = Some(driver);
    }

    /// Get driver
    pub fn driver(&self) -> Option<&dyn GpuDriver> {
        match &self.driver {
            Some(d) => Some(d.as_ref()),
            None => None,
        }
    }

    /// Get driver mut
    pub fn driver_mut(&mut self) -> Option<&mut dyn GpuDriver> {
        match &mut self.driver {
            Some(d) => Some(d.as_mut()),
            None => None,
        }
    }

    /// Add connector
    pub fn add_connector(&self, connector: Connector) -> u32 {
        let id = connector.id;
        self.connectors.write().push(Arc::new(RwLock::new(connector)));
        id
    }

    /// Get connector
    pub fn get_connector(&self, id: u32) -> Option<Arc<RwLock<Connector>>> {
        self.connectors.read()
            .iter()
            .find(|c| c.read().id == id)
            .cloned()
    }

    /// Get all connectors
    pub fn get_connectors(&self) -> Vec<Arc<RwLock<Connector>>> {
        self.connectors.read().clone()
    }

    /// Add CRTC
    pub fn add_crtc(&self, crtc: Crtc) -> u32 {
        let id = crtc.id;
        self.crtcs.write().push(Arc::new(RwLock::new(crtc)));
        id
    }

    /// Get CRTC
    pub fn get_crtc(&self, id: u32) -> Option<Arc<RwLock<Crtc>>> {
        self.crtcs.read()
            .iter()
            .find(|c| c.read().id == id)
            .cloned()
    }

    /// Get all CRTCs
    pub fn get_crtcs(&self) -> Vec<Arc<RwLock<Crtc>>> {
        self.crtcs.read().clone()
    }

    /// Add encoder
    pub fn add_encoder(&self, encoder: Encoder) -> u32 {
        let id = encoder.id;
        self.encoders.write().push(Arc::new(RwLock::new(encoder)));
        id
    }

    /// Get encoder
    pub fn get_encoder(&self, id: u32) -> Option<Arc<RwLock<Encoder>>> {
        self.encoders.read()
            .iter()
            .find(|e| e.read().id == id)
            .cloned()
    }

    /// Get all encoders
    pub fn get_encoders(&self) -> Vec<Arc<RwLock<Encoder>>> {
        self.encoders.read().clone()
    }

    /// Add plane
    pub fn add_plane(&self, plane: Plane) -> u32 {
        let id = plane.id;
        self.planes.write().push(Arc::new(RwLock::new(plane)));
        id
    }

    /// Get plane
    pub fn get_plane(&self, id: u32) -> Option<Arc<RwLock<Plane>>> {
        self.planes.read()
            .iter()
            .find(|p| p.read().id == id)
            .cloned()
    }

    /// Get all planes
    pub fn get_planes(&self) -> Vec<Arc<RwLock<Plane>>> {
        self.planes.read().clone()
    }

    /// Add framebuffer
    pub fn add_framebuffer(&self, fb: DrmFramebuffer) -> u32 {
        let id = self.next_fb_id.fetch_add(1, Ordering::SeqCst);
        let mut fb = fb;
        fb.id = id;
        self.framebuffers.write().insert(id, Arc::new(RwLock::new(fb)));
        id
    }

    /// Get framebuffer
    pub fn get_framebuffer(&self, id: u32) -> Option<Arc<RwLock<DrmFramebuffer>>> {
        self.framebuffers.read().get(&id).cloned()
    }

    /// Remove framebuffer
    pub fn remove_framebuffer(&self, id: u32) -> bool {
        self.framebuffers.write().remove(&id).is_some()
    }

    /// Get GEM manager
    pub fn gem_manager(&self) -> Arc<RwLock<GemManager>> {
        self.gem_manager.clone()
    }

    /// Get resources (connectors, CRTCs, encoders, framebuffers)
    pub fn get_resources(&self) -> DrmResources {
        DrmResources {
            connector_ids: self.connectors.read().iter().map(|c| c.read().id).collect(),
            crtc_ids: self.crtcs.read().iter().map(|c| c.read().id).collect(),
            encoder_ids: self.encoders.read().iter().map(|e| e.read().id).collect(),
            fb_ids: self.framebuffers.read().keys().copied().collect(),
            min_width: 0,
            max_width: 8192,
            min_height: 0,
            max_height: 8192,
        }
    }

    /// Set mode on CRTC
    pub fn set_crtc(
        &self,
        crtc_id: u32,
        _fb_id: u32,
        x: u32,
        y: u32,
        connector_ids: &[u32],
        mode: Option<&ModeInfo>,
    ) -> Result<(), GpuError> {
        let crtc = self.get_crtc(crtc_id).ok_or(GpuError::InvalidCrtc)?;
        let mut crtc = crtc.write();

        if let Some(mode_info) = mode {
            // Set new mode
            crtc.mode = Some(mode_info.clone());
            crtc.x = x;
            crtc.y = y;
            crtc.active = true;

            // Update encoder/connector bindings
            for &conn_id in connector_ids {
                if let Some(connector) = self.get_connector(conn_id) {
                    let mut conn = connector.write();
                    conn.crtc_id = Some(crtc_id);
                }
            }

            // Apply mode via driver
            if let Some(driver) = &self.driver {
                driver.mode_set(&crtc, mode_info)?;
            }
        } else {
            // Disable CRTC
            crtc.mode = None;
            crtc.active = false;
        }

        super::GPU.record_mode_set();
        Ok(())
    }

    /// Page flip
    pub fn page_flip(
        &self,
        crtc_id: u32,
        fb_id: u32,
        _flags: u32,
        _user_data: u64,
    ) -> Result<(), GpuError> {
        let crtc = self.get_crtc(crtc_id).ok_or(GpuError::InvalidCrtc)?;
        let fb = self.get_framebuffer(fb_id).ok_or(GpuError::InvalidHandle)?;

        // Apply page flip via driver
        if let Some(driver) = &self.driver {
            driver.page_flip(&crtc.read(), &fb.read())?;
        }

        super::GPU.record_page_flip();
        Ok(())
    }

    /// Get CRTC gamma
    pub fn get_crtc_gamma(&self, crtc_id: u32) -> Result<GammaRamp, GpuError> {
        let crtc = self.get_crtc(crtc_id).ok_or(GpuError::InvalidCrtc)?;
        let crtc = crtc.read();
        Ok(crtc.gamma.clone())
    }

    /// Set CRTC gamma
    pub fn set_crtc_gamma(&self, crtc_id: u32, gamma: GammaRamp) -> Result<(), GpuError> {
        let crtc = self.get_crtc(crtc_id).ok_or(GpuError::InvalidCrtc)?;
        let mut crtc = crtc.write();
        crtc.gamma = gamma.clone();

        if let Some(driver) = &self.driver {
            driver.set_gamma(&crtc, &gamma)?;
        }

        Ok(())
    }

    /// Wait for vblank
    pub fn wait_vblank(&self, crtc_id: u32) -> Result<VblankEvent, GpuError> {
        let crtc = self.get_crtc(crtc_id).ok_or(GpuError::InvalidCrtc)?;

        if let Some(driver) = &self.driver {
            driver.wait_vblank(&crtc.read())
        } else {
            Err(GpuError::NotSupported)
        }
    }

    /// Set cursor
    pub fn set_cursor(
        &self,
        crtc_id: u32,
        handle: GemHandle,
        width: u32,
        height: u32,
    ) -> Result<(), GpuError> {
        let crtc = self.get_crtc(crtc_id).ok_or(GpuError::InvalidCrtc)?;

        if let Some(driver) = &self.driver {
            driver.set_cursor(&crtc.read(), handle, width, height)
        } else {
            Err(GpuError::NotSupported)
        }
    }

    /// Move cursor
    pub fn move_cursor(&self, crtc_id: u32, x: i32, y: i32) -> Result<(), GpuError> {
        let crtc = self.get_crtc(crtc_id).ok_or(GpuError::InvalidCrtc)?;

        if let Some(driver) = &self.driver {
            driver.move_cursor(&crtc.read(), x, y)
        } else {
            Err(GpuError::NotSupported)
        }
    }
}

/// DRM resources
#[derive(Clone, Debug)]
pub struct DrmResources {
    pub connector_ids: Vec<u32>,
    pub crtc_ids: Vec<u32>,
    pub encoder_ids: Vec<u32>,
    pub fb_ids: Vec<u32>,
    pub min_width: u32,
    pub max_width: u32,
    pub min_height: u32,
    pub max_height: u32,
}

/// Gamma ramp
#[derive(Clone, Debug, Default)]
pub struct GammaRamp {
    pub size: u32,
    pub red: Vec<u16>,
    pub green: Vec<u16>,
    pub blue: Vec<u16>,
}

impl GammaRamp {
    pub fn new(size: u32) -> Self {
        Self {
            size,
            red: alloc::vec![0; size as usize],
            green: alloc::vec![0; size as usize],
            blue: alloc::vec![0; size as usize],
        }
    }

    /// Create linear gamma ramp
    pub fn linear(size: u32) -> Self {
        let mut ramp = Self::new(size);
        for i in 0..size as usize {
            let val = ((i as u32) * 65535 / (size - 1)) as u16;
            ramp.red[i] = val;
            ramp.green[i] = val;
            ramp.blue[i] = val;
        }
        ramp
    }
}

/// VBlank event
#[derive(Clone, Debug)]
pub struct VblankEvent {
    pub sequence: u32,
    pub timestamp_sec: u32,
    pub timestamp_usec: u32,
    pub crtc_id: u32,
    pub user_data: u64,
}

/// DRM file (open handle to device)
pub struct DrmFile {
    /// Device reference
    device: Arc<RwLock<DrmDevice>>,
    /// Is authenticated
    authenticated: AtomicBool,
    /// Is master
    is_master: AtomicBool,
    /// Is render only
    is_render: bool,
    /// Open GEM handles
    gem_handles: RwLock<BTreeMap<GemHandle, Arc<RwLock<GemObject>>>>,
    /// Next GEM handle
    next_gem_handle: AtomicU32,
    /// Event queue
    events: RwLock<Vec<DrmEvent>>,
    /// Pending vblank requests
    pending_vblanks: RwLock<Vec<VblankRequest>>,
    /// Pending page flips
    pending_page_flips: RwLock<Vec<PageFlipRequest>>,
}

impl DrmFile {
    /// Create new DRM file
    pub fn new(device: Arc<RwLock<DrmDevice>>) -> Self {
        device.read().open_count.fetch_add(1, Ordering::SeqCst);

        Self {
            device,
            authenticated: AtomicBool::new(false),
            is_master: AtomicBool::new(false),
            is_render: false,
            gem_handles: RwLock::new(BTreeMap::new()),
            next_gem_handle: AtomicU32::new(1),
            events: RwLock::new(Vec::new()),
            pending_vblanks: RwLock::new(Vec::new()),
            pending_page_flips: RwLock::new(Vec::new()),
        }
    }

    /// Get device
    pub fn device(&self) -> Arc<RwLock<DrmDevice>> {
        self.device.clone()
    }

    /// Is authenticated
    pub fn is_authenticated(&self) -> bool {
        self.authenticated.load(Ordering::Acquire)
    }

    /// Set authenticated
    pub fn set_authenticated(&self, auth: bool) {
        self.authenticated.store(auth, Ordering::Release);
    }

    /// Is master
    pub fn is_master(&self) -> bool {
        self.is_master.load(Ordering::Acquire)
    }

    /// Set master
    pub fn set_master(&self, master: bool) -> Result<(), GpuError> {
        let device = self.device.read();
        if master {
            if device.is_master.compare_exchange(
                false, true, Ordering::SeqCst, Ordering::Relaxed
            ).is_ok() {
                self.is_master.store(true, Ordering::Release);
                Ok(())
            } else {
                Err(GpuError::PermissionDenied)
            }
        } else {
            if self.is_master.swap(false, Ordering::SeqCst) {
                device.is_master.store(false, Ordering::Release);
            }
            Ok(())
        }
    }

    /// Drop master
    pub fn drop_master(&self) -> Result<(), GpuError> {
        self.set_master(false)
    }

    /// Create GEM handle
    pub fn create_gem_handle(&self, object: Arc<RwLock<GemObject>>) -> GemHandle {
        let handle = self.next_gem_handle.fetch_add(1, Ordering::SeqCst);
        self.gem_handles.write().insert(handle, object);
        handle
    }

    /// Get GEM object by handle
    pub fn get_gem_object(&self, handle: GemHandle) -> Option<Arc<RwLock<GemObject>>> {
        self.gem_handles.read().get(&handle).cloned()
    }

    /// Close GEM handle
    pub fn close_gem_handle(&self, handle: GemHandle) -> bool {
        self.gem_handles.write().remove(&handle).is_some()
    }

    /// Add event
    pub fn add_event(&self, event: DrmEvent) {
        self.events.write().push(event);
    }

    /// Read events
    pub fn read_events(&self) -> Vec<DrmEvent> {
        let mut events = self.events.write();
        let result = events.clone();
        events.clear();
        result
    }

    /// Has pending events
    pub fn has_events(&self) -> bool {
        !self.events.read().is_empty()
    }
}

impl Drop for DrmFile {
    fn drop(&mut self) {
        self.device.read().open_count.fetch_sub(1, Ordering::SeqCst);
        let _ = self.set_master(false);
    }
}

/// DRM event
#[derive(Clone, Debug)]
pub enum DrmEvent {
    VBlank(VblankEvent),
    PageFlip {
        sequence: u32,
        timestamp_sec: u32,
        timestamp_usec: u32,
        crtc_id: u32,
        user_data: u64,
    },
}

/// VBlank request
#[derive(Clone, Debug)]
pub struct VblankRequest {
    pub crtc_id: u32,
    pub sequence: u32,
    pub signal: bool,
    pub user_data: u64,
}

/// Page flip request
#[derive(Clone, Debug)]
pub struct PageFlipRequest {
    pub crtc_id: u32,
    pub fb_id: u32,
    pub flags: u32,
    pub user_data: u64,
}

/// DRM ioctl commands
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrmIoctl {
    /// Get version
    Version,
    /// Get unique (device path)
    GetUnique,
    /// Get magic (for authentication)
    GetMagic,
    /// Auth magic
    AuthMagic,
    /// Get IRQ
    GetIrq,
    /// Get bus ID
    GetBusId,
    /// Get capabilities
    GetCap,
    /// Set capabilities
    SetCap,
    /// Set master
    SetMaster,
    /// Drop master
    DropMaster,
    /// Mode get resources
    ModeGetResources,
    /// Mode get CRTC
    ModeGetCrtc,
    /// Mode set CRTC
    ModeSetCrtc,
    /// Mode get encoder
    ModeGetEncoder,
    /// Mode get connector
    ModeGetConnector,
    /// Mode add FB
    ModeAddFb,
    /// Mode add FB2
    ModeAddFb2,
    /// Mode remove FB
    ModeRmFb,
    /// Mode dirty FB
    ModeDirtyFb,
    /// Mode get plane resources
    ModeGetPlaneResources,
    /// Mode get plane
    ModeGetPlane,
    /// Mode set plane
    ModeSetPlane,
    /// Mode cursor
    ModeCursor,
    /// Mode cursor2
    ModeCursor2,
    /// Page flip
    ModePageFlip,
    /// Wait vblank
    WaitVblank,
    /// Get gamma
    ModeGetGamma,
    /// Set gamma
    ModeSetGamma,
    /// Get property
    ModeGetProperty,
    /// Set property
    ModeSetProperty,
    /// Get property blob
    ModeGetPropertyBlob,
    /// Create property blob
    ModeCreatePropertyBlob,
    /// Destroy property blob
    ModeDestroyPropertyBlob,
    /// Atomic
    ModeAtomic,
    /// Create dumb buffer
    ModeCreateDumb,
    /// Map dumb buffer
    ModeMapDumb,
    /// Destroy dumb buffer
    ModeDestroyDumb,
    /// GEM create
    GemCreate,
    /// GEM close
    GemClose,
    /// GEM open
    GemOpen,
    /// GEM flink
    GemFlink,
    /// Prime handle to FD
    PrimeHandleToFd,
    /// Prime FD to handle
    PrimeFdToHandle,
}

impl DrmIoctl {
    /// Get ioctl number
    pub fn number(&self) -> u32 {
        match self {
            DrmIoctl::Version => 0x00,
            DrmIoctl::GetUnique => 0x01,
            DrmIoctl::GetMagic => 0x02,
            DrmIoctl::AuthMagic => 0x11,
            DrmIoctl::GetIrq => 0x03,
            DrmIoctl::GetBusId => 0x05,
            DrmIoctl::GetCap => 0x0C,
            DrmIoctl::SetCap => 0x0D,
            DrmIoctl::SetMaster => 0x1E,
            DrmIoctl::DropMaster => 0x1F,
            DrmIoctl::ModeGetResources => 0xA0,
            DrmIoctl::ModeGetCrtc => 0xA1,
            DrmIoctl::ModeSetCrtc => 0xA2,
            DrmIoctl::ModeGetEncoder => 0xA6,
            DrmIoctl::ModeGetConnector => 0xA7,
            DrmIoctl::ModeAddFb => 0xAE,
            DrmIoctl::ModeAddFb2 => 0xB8,
            DrmIoctl::ModeRmFb => 0xAF,
            DrmIoctl::ModeDirtyFb => 0xB1,
            DrmIoctl::ModeGetPlaneResources => 0xB5,
            DrmIoctl::ModeGetPlane => 0xB6,
            DrmIoctl::ModeSetPlane => 0xB7,
            DrmIoctl::ModeCursor => 0xA3,
            DrmIoctl::ModeCursor2 => 0xBB,
            DrmIoctl::ModePageFlip => 0xB0,
            DrmIoctl::WaitVblank => 0x3A,
            DrmIoctl::ModeGetGamma => 0xA4,
            DrmIoctl::ModeSetGamma => 0xA5,
            DrmIoctl::ModeGetProperty => 0xAA,
            DrmIoctl::ModeSetProperty => 0xAB,
            DrmIoctl::ModeGetPropertyBlob => 0xAC,
            DrmIoctl::ModeCreatePropertyBlob => 0xBD,
            DrmIoctl::ModeDestroyPropertyBlob => 0xBE,
            DrmIoctl::ModeAtomic => 0xBC,
            DrmIoctl::ModeCreateDumb => 0xB2,
            DrmIoctl::ModeMapDumb => 0xB3,
            DrmIoctl::ModeDestroyDumb => 0xB4,
            DrmIoctl::GemCreate => 0x09,
            DrmIoctl::GemClose => 0x09,
            DrmIoctl::GemOpen => 0x0B,
            DrmIoctl::GemFlink => 0x0A,
            DrmIoctl::PrimeHandleToFd => 0x2D,
            DrmIoctl::PrimeFdToHandle => 0x2E,
        }
    }
}
