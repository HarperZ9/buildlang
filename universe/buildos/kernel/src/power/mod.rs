//! Power Management Subsystem
//!
//! Provides comprehensive power management including:
//! - ACPI power states (S0-S5)
//! - CPU frequency scaling (cpufreq)
//! - Device power management
//! - Suspend/Resume support
//! - Thermal management
//! - Battery monitoring

pub mod acpi;
pub mod cpufreq;
pub mod suspend;
pub mod thermal;
pub mod battery;
pub mod governor;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicBool, AtomicU64, Ordering};
use spin::RwLock;

/// Power management error
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerError {
    /// Device not found
    DeviceNotFound,
    /// Operation not supported
    NotSupported,
    /// Invalid state
    InvalidState,
    /// Permission denied
    PermissionDenied,
    /// Hardware error
    HardwareError,
    /// Busy
    Busy,
    /// Timeout
    Timeout,
    /// ACPI error
    AcpiError,
    /// Thermal limit exceeded
    ThermalLimit,
    /// Battery error
    BatteryError,
}

/// Power state
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PowerState {
    /// S0: Working state
    S0Working,
    /// S0ix: Low power idle (connected standby)
    S0ix,
    /// S1: Power on suspend (CPU stops, memory refreshed)
    S1Standby,
    /// S2: CPU off, dirty cache flushed
    S2Sleep,
    /// S3: Suspend to RAM
    S3SuspendToRam,
    /// S4: Suspend to disk (hibernate)
    S4Hibernate,
    /// S5: Soft off
    S5SoftOff,
    /// G2/S5: Mechanical off
    G2MechanicalOff,
    /// G3: Hard off
    G3HardOff,
}

impl PowerState {
    /// Get string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::S0Working => "S0 (Working)",
            Self::S0ix => "S0ix (Low Power Idle)",
            Self::S1Standby => "S1 (Standby)",
            Self::S2Sleep => "S2 (Sleep)",
            Self::S3SuspendToRam => "S3 (Suspend to RAM)",
            Self::S4Hibernate => "S4 (Hibernate)",
            Self::S5SoftOff => "S5 (Soft Off)",
            Self::G2MechanicalOff => "G2 (Mechanical Off)",
            Self::G3HardOff => "G3 (Hard Off)",
        }
    }

    /// Check if system is running
    pub fn is_running(&self) -> bool {
        matches!(self, Self::S0Working | Self::S0ix)
    }
}

/// Device power state (D-states)
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DevicePowerState {
    /// D0: Fully on
    D0,
    /// D1: Light sleep
    D1,
    /// D2: Medium sleep
    D2,
    /// D3hot: Deep sleep with power
    D3Hot,
    /// D3cold: Off
    D3Cold,
}

impl DevicePowerState {
    /// Get power consumption level (relative)
    pub fn power_level(&self) -> u8 {
        match self {
            Self::D0 => 100,
            Self::D1 => 50,
            Self::D2 => 25,
            Self::D3Hot => 10,
            Self::D3Cold => 0,
        }
    }
}

/// CPU power state (C-states)
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CpuCState {
    /// C0: Active
    C0,
    /// C1: Halt
    C1,
    /// C1E: Enhanced halt
    C1E,
    /// C2: Stop clock
    C2,
    /// C3: Sleep
    C3,
    /// C6: Deep power down
    C6,
    /// C7: Package C7
    C7,
    /// C8: Package C8
    C8,
    /// C10: Package C10
    C10,
}

impl CpuCState {
    /// Get exit latency in microseconds
    pub fn exit_latency_us(&self) -> u32 {
        match self {
            Self::C0 => 0,
            Self::C1 => 1,
            Self::C1E => 2,
            Self::C2 => 10,
            Self::C3 => 100,
            Self::C6 => 200,
            Self::C7 => 400,
            Self::C8 => 600,
            Self::C10 => 1000,
        }
    }

    /// Get target residency in microseconds
    pub fn target_residency_us(&self) -> u32 {
        match self {
            Self::C0 => 0,
            Self::C1 => 1,
            Self::C1E => 10,
            Self::C2 => 80,
            Self::C3 => 800,
            Self::C6 => 1600,
            Self::C7 => 3200,
            Self::C8 => 6400,
            Self::C10 => 12800,
        }
    }
}

/// Performance state (P-states)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PState {
    /// Frequency in kHz
    pub frequency_khz: u32,
    /// Voltage in mV
    pub voltage_mv: u32,
    /// Power in mW
    pub power_mw: u32,
    /// Latency in us
    pub latency_us: u32,
}

/// Power domain
#[derive(Clone, Debug)]
pub struct PowerDomain {
    /// Domain name
    pub name: String,
    /// Domain ID
    pub id: u32,
    /// Current state
    pub state: DevicePowerState,
    /// Parent domain
    pub parent: Option<u32>,
    /// Child domains
    pub children: Vec<u32>,
    /// Devices in this domain
    pub devices: Vec<u64>,
    /// Is enabled
    pub enabled: bool,
}

/// Power management subsystem
pub struct PowerManager {
    /// Current system power state
    current_state: AtomicU32,
    /// Target state
    target_state: AtomicU32,
    /// Is initialized
    initialized: AtomicBool,
    /// ACPI supported
    acpi_supported: AtomicBool,
    /// Registered devices
    devices: RwLock<BTreeMap<u64, DevicePowerInfo>>,
    /// Power domains
    domains: RwLock<BTreeMap<u32, PowerDomain>>,
    /// Wake sources
    wake_sources: RwLock<Vec<WakeSource>>,
    /// Suspend blockers
    suspend_blockers: RwLock<Vec<SuspendBlocker>>,
    /// Statistics
    stats: PowerStats,
    /// Next device ID
    next_device_id: AtomicU64,
}

/// Device power info
#[derive(Clone, Debug)]
pub struct DevicePowerInfo {
    /// Device ID
    pub id: u64,
    /// Device name
    pub name: String,
    /// Current power state
    pub state: DevicePowerState,
    /// Supported states
    pub supported_states: Vec<DevicePowerState>,
    /// Power domain ID
    pub domain_id: Option<u32>,
    /// Is runtime PM enabled
    pub runtime_pm: bool,
    /// Last state change time
    pub last_state_change: u64,
    /// Total active time
    pub active_time: u64,
    /// Total suspended time
    pub suspended_time: u64,
}

/// Wake source
#[derive(Clone, Debug)]
pub struct WakeSource {
    /// Source name
    pub name: String,
    /// Device ID (if device)
    pub device_id: Option<u64>,
    /// Is enabled
    pub enabled: bool,
    /// Wakeup count
    pub wakeup_count: u64,
}

/// Suspend blocker
#[derive(Clone, Debug)]
pub struct SuspendBlocker {
    /// Blocker name
    pub name: String,
    /// Process ID
    pub pid: u32,
    /// Is active
    pub active: bool,
    /// Creation time
    pub created: u64,
}

/// Power statistics
#[derive(Debug, Default)]
pub struct PowerStats {
    /// Total suspend count
    pub suspend_count: AtomicU64,
    /// Total resume count
    pub resume_count: AtomicU64,
    /// Failed suspends
    pub suspend_failures: AtomicU64,
    /// Total suspend time (ns)
    pub total_suspend_time: AtomicU64,
    /// Last suspend duration (ns)
    pub last_suspend_duration: AtomicU64,
    /// Wakeup count
    pub wakeup_count: AtomicU64,
    /// Spurious wakeups
    pub spurious_wakeups: AtomicU64,
}

impl PowerManager {
    /// Create new power manager
    pub const fn new() -> Self {
        Self {
            current_state: AtomicU32::new(0), // S0Working
            target_state: AtomicU32::new(0),
            initialized: AtomicBool::new(false),
            acpi_supported: AtomicBool::new(false),
            devices: RwLock::new(BTreeMap::new()),
            domains: RwLock::new(BTreeMap::new()),
            wake_sources: RwLock::new(Vec::new()),
            suspend_blockers: RwLock::new(Vec::new()),
            stats: PowerStats {
                suspend_count: AtomicU64::new(0),
                resume_count: AtomicU64::new(0),
                suspend_failures: AtomicU64::new(0),
                total_suspend_time: AtomicU64::new(0),
                last_suspend_duration: AtomicU64::new(0),
                wakeup_count: AtomicU64::new(0),
                spurious_wakeups: AtomicU64::new(0),
            },
            next_device_id: AtomicU64::new(1),
        }
    }

    /// Initialize power management
    pub fn init(&self) -> Result<(), PowerError> {
        // Check ACPI support
        let acpi_ok = acpi::init().is_ok();
        self.acpi_supported.store(acpi_ok, Ordering::Release);

        // Initialize subsystems
        cpufreq::init();
        thermal::init();
        battery::init();
        governor::init();

        // Create default power domains
        self.create_default_domains();

        self.initialized.store(true, Ordering::Release);

        Ok(())
    }

    /// Create default power domains
    fn create_default_domains(&self) {
        let mut domains = self.domains.write();

        // System domain (root)
        domains.insert(0, PowerDomain {
            name: String::from("system"),
            id: 0,
            state: DevicePowerState::D0,
            parent: None,
            children: vec![1, 2, 3],
            devices: Vec::new(),
            enabled: true,
        });

        // CPU domain
        domains.insert(1, PowerDomain {
            name: String::from("cpu"),
            id: 1,
            state: DevicePowerState::D0,
            parent: Some(0),
            children: Vec::new(),
            devices: Vec::new(),
            enabled: true,
        });

        // Memory domain
        domains.insert(2, PowerDomain {
            name: String::from("memory"),
            id: 2,
            state: DevicePowerState::D0,
            parent: Some(0),
            children: Vec::new(),
            devices: Vec::new(),
            enabled: true,
        });

        // Peripheral domain
        domains.insert(3, PowerDomain {
            name: String::from("peripheral"),
            id: 3,
            state: DevicePowerState::D0,
            parent: Some(0),
            children: Vec::new(),
            devices: Vec::new(),
            enabled: true,
        });
    }

    /// Get current power state
    pub fn current_state(&self) -> PowerState {
        match self.current_state.load(Ordering::Acquire) {
            0 => PowerState::S0Working,
            1 => PowerState::S0ix,
            2 => PowerState::S1Standby,
            3 => PowerState::S2Sleep,
            4 => PowerState::S3SuspendToRam,
            5 => PowerState::S4Hibernate,
            6 => PowerState::S5SoftOff,
            _ => PowerState::S0Working,
        }
    }

    /// Request power state change
    pub fn request_state(&self, state: PowerState) -> Result<(), PowerError> {
        if !self.initialized.load(Ordering::Acquire) {
            return Err(PowerError::InvalidState);
        }

        // Check for suspend blockers
        let blockers = self.suspend_blockers.read();
        if blockers.iter().any(|b| b.active) {
            return Err(PowerError::Busy);
        }

        // Store target state
        self.target_state.store(state as u32, Ordering::Release);

        // Initiate transition
        match state {
            PowerState::S0Working => self.resume(),
            PowerState::S3SuspendToRam => suspend::suspend_to_ram(),
            PowerState::S4Hibernate => suspend::hibernate(),
            PowerState::S5SoftOff => self.power_off(),
            _ => Err(PowerError::NotSupported),
        }
    }

    /// Resume from suspend
    fn resume(&self) -> Result<(), PowerError> {
        self.current_state.store(0, Ordering::Release);
        self.stats.resume_count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Power off system
    fn power_off(&self) -> Result<(), PowerError> {
        // Notify all devices
        self.notify_devices(PowerState::S5SoftOff)?;

        // Use ACPI to power off
        if self.acpi_supported.load(Ordering::Acquire) {
            acpi::power_off()
        } else {
            Err(PowerError::NotSupported)
        }
    }

    /// Notify devices of power state change
    fn notify_devices(&self, _state: PowerState) -> Result<(), PowerError> {
        let devices = self.devices.read();

        for (_id, _device) in devices.iter() {
            // Would call device-specific power management
        }

        Ok(())
    }

    /// Register a device
    pub fn register_device(&self, name: &str, domain: Option<u32>) -> u64 {
        let id = self.next_device_id.fetch_add(1, Ordering::SeqCst);

        let info = DevicePowerInfo {
            id,
            name: String::from(name),
            state: DevicePowerState::D0,
            supported_states: vec![DevicePowerState::D0, DevicePowerState::D3Cold],
            domain_id: domain,
            runtime_pm: false,
            last_state_change: 0,
            active_time: 0,
            suspended_time: 0,
        };

        self.devices.write().insert(id, info);

        // Add to domain if specified
        if let Some(domain_id) = domain {
            if let Some(d) = self.domains.write().get_mut(&domain_id) {
                d.devices.push(id);
            }
        }

        id
    }

    /// Unregister a device
    pub fn unregister_device(&self, id: u64) {
        if let Some(info) = self.devices.write().remove(&id) {
            // Remove from domain
            if let Some(domain_id) = info.domain_id {
                if let Some(d) = self.domains.write().get_mut(&domain_id) {
                    d.devices.retain(|&x| x != id);
                }
            }
        }
    }

    /// Set device power state
    pub fn set_device_state(&self, id: u64, state: DevicePowerState) -> Result<(), PowerError> {
        let mut devices = self.devices.write();

        if let Some(info) = devices.get_mut(&id) {
            if !info.supported_states.contains(&state) {
                return Err(PowerError::NotSupported);
            }

            info.state = state;
            info.last_state_change = get_timestamp();

            Ok(())
        } else {
            Err(PowerError::DeviceNotFound)
        }
    }

    /// Add wake source
    pub fn add_wake_source(&self, name: &str, device_id: Option<u64>) {
        self.wake_sources.write().push(WakeSource {
            name: String::from(name),
            device_id,
            enabled: true,
            wakeup_count: 0,
        });
    }

    /// Add suspend blocker
    pub fn add_suspend_blocker(&self, name: &str, pid: u32) {
        self.suspend_blockers.write().push(SuspendBlocker {
            name: String::from(name),
            pid,
            active: true,
            created: get_timestamp(),
        });
    }

    /// Remove suspend blocker
    pub fn remove_suspend_blocker(&self, name: &str, pid: u32) {
        self.suspend_blockers.write().retain(|b| !(b.name == name && b.pid == pid));
    }

    /// Get power statistics
    pub fn get_stats(&self) -> (u64, u64, u64, u64) {
        (
            self.stats.suspend_count.load(Ordering::Relaxed),
            self.stats.resume_count.load(Ordering::Relaxed),
            self.stats.suspend_failures.load(Ordering::Relaxed),
            self.stats.wakeup_count.load(Ordering::Relaxed),
        )
    }
}

/// Global power manager
static POWER_MANAGER: PowerManager = PowerManager::new();

/// Get timestamp
fn get_timestamp() -> u64 {
    // Would use system timer
    0
}

/// Initialize power management subsystem
pub fn init() {
    if let Err(e) = POWER_MANAGER.init() {
        crate::kprintln!("[POWER] Initialization failed: {:?}", e);
    } else {
        crate::kprintln!("[POWER] Power management subsystem initialized");
    }
}

/// Request system suspend
pub fn suspend() -> Result<(), PowerError> {
    POWER_MANAGER.request_state(PowerState::S3SuspendToRam)
}

/// Request system hibernate
pub fn hibernate() -> Result<(), PowerError> {
    POWER_MANAGER.request_state(PowerState::S4Hibernate)
}

/// Request system power off
pub fn power_off() -> Result<(), PowerError> {
    POWER_MANAGER.request_state(PowerState::S5SoftOff)
}

/// Get current power state
pub fn current_state() -> PowerState {
    POWER_MANAGER.current_state()
}

/// Register device for power management
pub fn register_device(name: &str, domain: Option<u32>) -> u64 {
    POWER_MANAGER.register_device(name, domain)
}

/// Set device power state
pub fn set_device_state(id: u64, state: DevicePowerState) -> Result<(), PowerError> {
    POWER_MANAGER.set_device_state(id, state)
}
