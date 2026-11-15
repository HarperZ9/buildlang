//! USB Hub Support
//!
//! USB hub device driver:
//! - Hub descriptor parsing
//! - Port status monitoring
//! - Port power management
//! - Device connect/disconnect handling
//! - Port reset and enable

#![allow(dead_code)]

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::{Mutex, RwLock};
use super::{
    UsbError, UsbDevice, UsbDriver, UsbSpeed, UsbClass,
    SetupPacket,
};

/// Hub class request codes
#[repr(u8)]
pub enum HubRequest {
    GetStatus = 0,
    ClearFeature = 1,
    SetFeature = 3,
    GetDescriptor = 6,
    SetDescriptor = 7,
    ClearTtBuffer = 8,
    ResetTt = 9,
    GetTtState = 10,
    StopTt = 11,
}

/// Hub features
#[repr(u16)]
pub enum HubFeature {
    CHubLocalPower = 0,
    CHubOverCurrent = 1,
}

/// Port features
#[repr(u16)]
pub enum PortFeature {
    Connection = 0,
    Enable = 1,
    Suspend = 2,
    OverCurrent = 3,
    Reset = 4,
    Power = 8,
    LowSpeed = 9,
    CConnection = 16,
    CEnable = 17,
    CSuspend = 18,
    COverCurrent = 19,
    CReset = 20,
    Test = 21,
    Indicator = 22,
}

/// Hub descriptor
#[derive(Clone, Debug)]
pub struct HubDescriptor {
    /// Descriptor length
    pub length: u8,
    /// Descriptor type (0x29 for USB 2.0, 0x2A for USB 3.0)
    pub descriptor_type: u8,
    /// Number of downstream ports
    pub num_ports: u8,
    /// Hub characteristics
    pub characteristics: u16,
    /// Power on to power good time (2ms units)
    pub power_on_time: u8,
    /// Hub controller current (mA)
    pub hub_current: u8,
    /// Device removable bitmap
    pub device_removable: Vec<u8>,
}

impl HubDescriptor {
    /// Create from raw bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, UsbError> {
        if data.len() < 7 {
            return Err(UsbError::InvalidDescriptor);
        }

        if data[1] != 0x29 && data[1] != 0x2A {
            return Err(UsbError::InvalidDescriptor);
        }

        let num_ports = data[2];
        let removable_len = (num_ports as usize + 1 + 7) / 8;

        let device_removable = if data.len() > 7 {
            data[7..7 + removable_len.min(data.len() - 7)].to_vec()
        } else {
            Vec::new()
        };

        Ok(Self {
            length: data[0],
            descriptor_type: data[1],
            num_ports,
            characteristics: u16::from_le_bytes([data[3], data[4]]),
            power_on_time: data[5],
            hub_current: data[6],
            device_removable,
        })
    }

    /// Is this a compound device?
    pub fn is_compound(&self) -> bool {
        self.characteristics & 0x04 != 0
    }

    /// Get power switching mode
    pub fn power_switching(&self) -> PowerSwitching {
        match self.characteristics & 0x03 {
            0 => PowerSwitching::Ganged,
            1 => PowerSwitching::Individual,
            _ => PowerSwitching::None,
        }
    }

    /// Get overcurrent protection mode
    pub fn overcurrent_protection(&self) -> OverCurrentProtection {
        match (self.characteristics >> 3) & 0x03 {
            0 => OverCurrentProtection::Global,
            1 => OverCurrentProtection::Individual,
            _ => OverCurrentProtection::None,
        }
    }

    /// Is port removable?
    pub fn is_port_removable(&self, port: u8) -> bool {
        if port == 0 || port > self.num_ports {
            return false;
        }
        let byte_idx = port as usize / 8;
        let bit_idx = port as usize % 8;
        if byte_idx < self.device_removable.len() {
            self.device_removable[byte_idx] & (1 << bit_idx) == 0
        } else {
            true
        }
    }
}

/// Power switching mode
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerSwitching {
    /// All ports powered together
    Ganged,
    /// Each port powered individually
    Individual,
    /// No power switching
    None,
}

/// Overcurrent protection mode
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverCurrentProtection {
    /// Global protection
    Global,
    /// Per-port protection
    Individual,
    /// No protection
    None,
}

/// Port status
#[derive(Clone, Copy, Debug, Default)]
pub struct PortStatus {
    /// Status bits
    pub status: u16,
    /// Change bits
    pub change: u16,
}

impl PortStatus {
    /// Create from raw bytes
    pub fn from_bytes(data: &[u8]) -> Self {
        if data.len() < 4 {
            return Self::default();
        }
        Self {
            status: u16::from_le_bytes([data[0], data[1]]),
            change: u16::from_le_bytes([data[2], data[3]]),
        }
    }

    /// Is connected
    pub fn is_connected(&self) -> bool {
        self.status & (1 << 0) != 0
    }

    /// Is enabled
    pub fn is_enabled(&self) -> bool {
        self.status & (1 << 1) != 0
    }

    /// Is suspended
    pub fn is_suspended(&self) -> bool {
        self.status & (1 << 2) != 0
    }

    /// Is over-current
    pub fn is_over_current(&self) -> bool {
        self.status & (1 << 3) != 0
    }

    /// Is reset
    pub fn is_reset(&self) -> bool {
        self.status & (1 << 4) != 0
    }

    /// Is powered
    pub fn is_powered(&self) -> bool {
        self.status & (1 << 8) != 0
    }

    /// Is low speed device
    pub fn is_low_speed(&self) -> bool {
        self.status & (1 << 9) != 0
    }

    /// Is high speed device
    pub fn is_high_speed(&self) -> bool {
        self.status & (1 << 10) != 0
    }

    /// Get device speed
    pub fn device_speed(&self) -> UsbSpeed {
        if self.is_high_speed() {
            UsbSpeed::High
        } else if self.is_low_speed() {
            UsbSpeed::Low
        } else {
            UsbSpeed::Full
        }
    }

    /// Connection changed
    pub fn connection_changed(&self) -> bool {
        self.change & (1 << 0) != 0
    }

    /// Enable changed
    pub fn enable_changed(&self) -> bool {
        self.change & (1 << 1) != 0
    }

    /// Suspend changed
    pub fn suspend_changed(&self) -> bool {
        self.change & (1 << 2) != 0
    }

    /// Over-current changed
    pub fn over_current_changed(&self) -> bool {
        self.change & (1 << 3) != 0
    }

    /// Reset changed
    pub fn reset_changed(&self) -> bool {
        self.change & (1 << 4) != 0
    }
}

/// USB Hub
pub struct UsbHub {
    /// Device reference
    device: Arc<RwLock<UsbDevice>>,
    /// Hub descriptor
    descriptor: HubDescriptor,
    /// Port statuses
    port_status: Vec<Mutex<PortStatus>>,
    /// Is initialized
    initialized: AtomicBool,
    /// Polling interval
    poll_interval: AtomicU32,
}

impl UsbHub {
    /// Create new hub
    pub fn new(device: Arc<RwLock<UsbDevice>>, descriptor: HubDescriptor) -> Self {
        let num_ports = descriptor.num_ports as usize;
        let mut port_status = Vec::with_capacity(num_ports);
        for _ in 0..num_ports {
            port_status.push(Mutex::new(PortStatus::default()));
        }

        Self {
            device,
            descriptor,
            port_status,
            initialized: AtomicBool::new(false),
            poll_interval: AtomicU32::new(256),
        }
    }

    /// Initialize hub
    pub fn init(&self) -> Result<(), UsbError> {
        // Power on all ports
        for port in 1..=self.descriptor.num_ports {
            self.set_port_feature(port, PortFeature::Power)?;
        }

        // Wait for power good
        let delay_ms = self.descriptor.power_on_time as u32 * 2;
        self.delay(delay_ms);

        // Get initial port status
        for port in 1..=self.descriptor.num_ports {
            let status = self.get_port_status(port)?;
            *self.port_status[(port - 1) as usize].lock() = status;
        }

        self.initialized.store(true, Ordering::Release);
        Ok(())
    }

    /// Get port status
    pub fn get_port_status(&self, port: u8) -> Result<PortStatus, UsbError> {
        let setup = SetupPacket {
            request_type: 0xA3, // Device to host, class, other
            request: HubRequest::GetStatus as u8,
            value: 0,
            index: port as u16,
            length: 4,
        };

        let mut buffer = [0u8; 4];
        super::control_transfer(&self.device, &setup, Some(&mut buffer))?;

        Ok(PortStatus::from_bytes(&buffer))
    }

    /// Set port feature
    pub fn set_port_feature(&self, port: u8, feature: PortFeature) -> Result<(), UsbError> {
        let setup = SetupPacket {
            request_type: 0x23, // Host to device, class, other
            request: HubRequest::SetFeature as u8,
            value: feature as u16,
            index: port as u16,
            length: 0,
        };

        super::control_transfer(&self.device, &setup, None)?;
        Ok(())
    }

    /// Clear port feature
    pub fn clear_port_feature(&self, port: u8, feature: PortFeature) -> Result<(), UsbError> {
        let setup = SetupPacket {
            request_type: 0x23,
            request: HubRequest::ClearFeature as u8,
            value: feature as u16,
            index: port as u16,
            length: 0,
        };

        super::control_transfer(&self.device, &setup, None)?;
        Ok(())
    }

    /// Reset port
    pub fn reset_port(&self, port: u8) -> Result<(), UsbError> {
        self.set_port_feature(port, PortFeature::Reset)?;

        // Wait for reset to complete
        for _ in 0..100 {
            self.delay(10);
            let status = self.get_port_status(port)?;
            if status.reset_changed() {
                self.clear_port_feature(port, PortFeature::CReset)?;
                break;
            }
        }

        Ok(())
    }

    /// Enable port
    pub fn enable_port(&self, port: u8) -> Result<(), UsbError> {
        self.set_port_feature(port, PortFeature::Enable)
    }

    /// Disable port
    pub fn disable_port(&self, port: u8) -> Result<(), UsbError> {
        self.clear_port_feature(port, PortFeature::Enable)
    }

    /// Power on port
    pub fn power_on(&self, port: u8) -> Result<(), UsbError> {
        self.set_port_feature(port, PortFeature::Power)
    }

    /// Power off port
    pub fn power_off(&self, port: u8) -> Result<(), UsbError> {
        self.clear_port_feature(port, PortFeature::Power)
    }

    /// Suspend port
    pub fn suspend_port(&self, port: u8) -> Result<(), UsbError> {
        self.set_port_feature(port, PortFeature::Suspend)
    }

    /// Resume port
    pub fn resume_port(&self, port: u8) -> Result<(), UsbError> {
        self.clear_port_feature(port, PortFeature::Suspend)
    }

    /// Poll for status changes
    pub fn poll(&self) -> Result<Vec<(u8, PortStatus)>, UsbError> {
        let mut changes = Vec::new();

        for port in 1..=self.descriptor.num_ports {
            let status = self.get_port_status(port)?;
            let port_idx = (port - 1) as usize;

            let _old_status = *self.port_status[port_idx].lock();

            if status.change != 0 {
                // Clear change bits
                if status.connection_changed() {
                    self.clear_port_feature(port, PortFeature::CConnection)?;
                }
                if status.enable_changed() {
                    self.clear_port_feature(port, PortFeature::CEnable)?;
                }
                if status.suspend_changed() {
                    self.clear_port_feature(port, PortFeature::CSuspend)?;
                }
                if status.over_current_changed() {
                    self.clear_port_feature(port, PortFeature::COverCurrent)?;
                }
                if status.reset_changed() {
                    self.clear_port_feature(port, PortFeature::CReset)?;
                }

                changes.push((port, status));
            }

            *self.port_status[port_idx].lock() = status;
        }

        Ok(changes)
    }

    /// Get number of ports
    pub fn num_ports(&self) -> u8 {
        self.descriptor.num_ports
    }

    /// Get cached port status
    pub fn cached_port_status(&self, port: u8) -> Option<PortStatus> {
        if port == 0 || port > self.descriptor.num_ports {
            return None;
        }
        Some(*self.port_status[(port - 1) as usize].lock())
    }

    /// Delay in milliseconds
    fn delay(&self, _ms: u32) {
        for _ in 0..10000 {
            core::hint::spin_loop();
        }
    }
}

/// USB Hub Driver
pub struct HubDriver {
    /// Active hubs
    hubs: RwLock<Vec<Arc<UsbHub>>>,
}

impl HubDriver {
    /// Create new hub driver
    pub fn new() -> Self {
        Self {
            hubs: RwLock::new(Vec::new()),
        }
    }

    /// Get hub descriptor
    fn get_hub_descriptor(device: &Arc<RwLock<UsbDevice>>) -> Result<HubDescriptor, UsbError> {
        let dev = device.read();
        let desc_type = if dev.speed == UsbSpeed::Super || dev.speed == UsbSpeed::SuperPlus {
            0x2A // SuperSpeed hub descriptor
        } else {
            0x29 // USB 2.0 hub descriptor
        };
        drop(dev);

        let setup = SetupPacket {
            request_type: 0xA0, // Device to host, class, device
            request: HubRequest::GetDescriptor as u8,
            value: (desc_type as u16) << 8,
            index: 0,
            length: 71, // Max hub descriptor size
        };

        let mut buffer = [0u8; 71];
        let len = super::control_transfer(device, &setup, Some(&mut buffer))?;

        HubDescriptor::from_bytes(&buffer[..len])
    }
}

impl UsbDriver for HubDriver {
    fn name(&self) -> &str {
        "usb-hub"
    }

    fn probe(&self, device: &UsbDevice) -> bool {
        device.class() == UsbClass::Hub ||
        device.current_configuration()
            .map(|c| c.interfaces.iter().any(|i| i.class() == UsbClass::Hub))
            .unwrap_or(false)
    }

    fn attach(&self, device: Arc<RwLock<UsbDevice>>) -> Result<(), UsbError> {
        let descriptor = Self::get_hub_descriptor(&device)?;

        crate::kprintln!("[USB] Hub with {} ports attached", descriptor.num_ports);

        let hub = Arc::new(UsbHub::new(device, descriptor));
        hub.init()?;

        self.hubs.write().push(hub);

        Ok(())
    }

    fn detach(&self, device: &UsbDevice) {
        let address = device.address;
        self.hubs.write().retain(|h| {
            h.device.read().address != address
        });
    }
}

/// USB 3.0 Hub Descriptor
#[derive(Clone, Debug)]
pub struct Usb3HubDescriptor {
    /// Base hub descriptor
    pub base: HubDescriptor,
    /// Hub header decode latency
    pub header_decode_latency: u8,
    /// Hub delay
    pub hub_delay: u16,
    /// Device removable bitmap
    pub device_removable: u16,
}

/// USB 3.0 Port Status
#[derive(Clone, Copy, Debug, Default)]
pub struct Usb3PortStatus {
    /// Base port status
    pub base: PortStatus,
    /// Extended status
    pub ext_status: u16,
}

impl Usb3PortStatus {
    /// Get link state
    pub fn link_state(&self) -> LinkState {
        match (self.base.status >> 5) & 0x0F {
            0 => LinkState::U0,
            1 => LinkState::U1,
            2 => LinkState::U2,
            3 => LinkState::U3,
            4 => LinkState::Disabled,
            5 => LinkState::RxDetect,
            6 => LinkState::Inactive,
            7 => LinkState::Polling,
            8 => LinkState::Recovery,
            9 => LinkState::HotReset,
            10 => LinkState::ComplianceMode,
            11 => LinkState::Loopback,
            _ => LinkState::Unknown,
        }
    }

    /// Get device speed
    pub fn device_speed(&self) -> UsbSpeed {
        match (self.base.status >> 10) & 0x07 {
            0 => UsbSpeed::Full,
            1 => UsbSpeed::Low,
            2 => UsbSpeed::High,
            3 => UsbSpeed::Super,
            4..=7 => UsbSpeed::SuperPlus,
            _ => UsbSpeed::Full,
        }
    }
}

/// USB 3.0 Link State
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkState {
    U0,
    U1,
    U2,
    U3,
    Disabled,
    RxDetect,
    Inactive,
    Polling,
    Recovery,
    HotReset,
    ComplianceMode,
    Loopback,
    Unknown,
}
