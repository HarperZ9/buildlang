//! Thermal Management
//!
//! Provides temperature monitoring and thermal throttling:
//! - CPU temperature sensors
//! - Thermal zones
//! - Cooling devices
//! - Thermal trip points
//! - Thermal throttling

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::boxed::Box;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use spin::RwLock;
use super::PowerError;

/// Temperature in millidegrees Celsius
pub type Temperature = i32;

/// MSR addresses
mod msr {
    pub const IA32_THERM_STATUS: u32 = 0x19C;
    pub const IA32_TEMPERATURE_TARGET: u32 = 0x1A2;
    pub const IA32_PACKAGE_THERM_STATUS: u32 = 0x1B1;
    pub const MSR_TEMPERATURE_TARGET: u32 = 0x1A2;
}

/// Thermal zone type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThermalZoneType {
    /// CPU package
    CpuPackage,
    /// CPU core
    CpuCore,
    /// GPU
    Gpu,
    /// Memory
    Memory,
    /// Chipset
    Chipset,
    /// SSD/NVMe
    Storage,
    /// Battery
    Battery,
    /// Skin/chassis
    Skin,
    /// ACPI thermal zone
    Acpi,
}

/// Trip point type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TripPointType {
    /// Active cooling (fan on)
    Active,
    /// Passive cooling (throttle)
    Passive,
    /// Hot (warning)
    Hot,
    /// Critical (emergency shutdown)
    Critical,
}

/// Trip point
#[derive(Clone, Debug)]
pub struct TripPoint {
    /// Type
    pub trip_type: TripPointType,
    /// Temperature threshold (millidegrees C)
    pub temperature: Temperature,
    /// Hysteresis (millidegrees C)
    pub hysteresis: Temperature,
    /// Is tripped
    pub tripped: bool,
}

/// Cooling device type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoolingDeviceType {
    /// Fan
    Fan,
    /// Processor throttle
    Processor,
    /// LCD brightness
    LcdBrightness,
    /// Power limit
    PowerLimit,
}

/// Cooling device
pub trait CoolingDevice: Send + Sync {
    /// Get device type
    fn device_type(&self) -> CoolingDeviceType;

    /// Get name
    fn name(&self) -> &str;

    /// Get max cooling state
    fn max_state(&self) -> u32;

    /// Get current cooling state
    fn current_state(&self) -> u32;

    /// Set cooling state
    fn set_state(&self, state: u32) -> Result<(), PowerError>;
}

/// Thermal zone
pub struct ThermalZone {
    /// Zone name
    pub name: String,
    /// Zone type
    pub zone_type: ThermalZoneType,
    /// Zone ID
    pub id: u32,
    /// Current temperature
    current_temp: AtomicI32,
    /// Trip points
    pub trip_points: Vec<TripPoint>,
    /// Associated cooling devices
    pub cooling_devices: Vec<u32>,
    /// Polling interval (ms)
    pub polling_interval: u32,
    /// Is enabled
    pub enabled: bool,
    /// Last poll time
    last_poll: AtomicU32,
    /// Temperature offset
    pub temp_offset: i32,
}

impl ThermalZone {
    /// Create new thermal zone
    pub fn new(name: &str, zone_type: ThermalZoneType, id: u32) -> Self {
        Self {
            name: String::from(name),
            zone_type,
            id,
            current_temp: AtomicI32::new(0),
            trip_points: Vec::new(),
            cooling_devices: Vec::new(),
            polling_interval: 1000,
            enabled: true,
            last_poll: AtomicU32::new(0),
            temp_offset: 0,
        }
    }

    /// Get current temperature
    pub fn temperature(&self) -> Temperature {
        self.current_temp.load(Ordering::Acquire)
    }

    /// Update temperature
    pub fn update_temperature(&self, temp: Temperature) {
        self.current_temp.store(temp + self.temp_offset, Ordering::Release);
    }

    /// Add trip point
    pub fn add_trip_point(&mut self, trip_type: TripPointType, temp: Temperature, hysteresis: Temperature) {
        self.trip_points.push(TripPoint {
            trip_type,
            temperature: temp,
            hysteresis,
            tripped: false,
        });
    }

    /// Check trip points
    pub fn check_trips(&mut self) -> Vec<(usize, TripPointType, bool)> {
        let temp = self.temperature();
        let mut events = Vec::new();

        for (i, trip) in self.trip_points.iter_mut().enumerate() {
            let _was_tripped = trip.tripped;

            if !trip.tripped && temp >= trip.temperature {
                trip.tripped = true;
                events.push((i, trip.trip_type, true));
            } else if trip.tripped && temp < (trip.temperature - trip.hysteresis) {
                trip.tripped = false;
                events.push((i, trip.trip_type, false));
            }
        }

        events
    }
}

/// Thermal subsystem
pub struct ThermalSubsystem {
    /// Is initialized
    initialized: AtomicBool,
    /// Thermal zones
    zones: RwLock<BTreeMap<u32, ThermalZone>>,
    /// Cooling devices
    cooling_devices: RwLock<BTreeMap<u32, Box<dyn CoolingDevice>>>,
    /// Next zone ID
    next_zone_id: AtomicU32,
    /// Next device ID
    next_device_id: AtomicU32,
    /// TJ Max (maximum junction temperature)
    tj_max: AtomicI32,
    /// Throttle active
    throttle_active: AtomicBool,
}

impl ThermalSubsystem {
    /// Create new thermal subsystem
    pub const fn new() -> Self {
        Self {
            initialized: AtomicBool::new(false),
            zones: RwLock::new(BTreeMap::new()),
            cooling_devices: RwLock::new(BTreeMap::new()),
            next_zone_id: AtomicU32::new(1),
            next_device_id: AtomicU32::new(1),
            tj_max: AtomicI32::new(100000), // 100°C default
            throttle_active: AtomicBool::new(false),
        }
    }

    /// Initialize thermal subsystem
    pub fn init(&self) -> Result<(), PowerError> {
        // Read TJ Max from MSR
        let tj_max = self.read_tj_max();
        self.tj_max.store(tj_max, Ordering::Release);

        // Create CPU thermal zones
        let cpu_count = crate::drivers::acpi::cpu_count();
        for cpu in 0..cpu_count {
            let mut zone = ThermalZone::new(
                &alloc::format!("cpu{}", cpu),
                ThermalZoneType::CpuCore,
                self.next_zone_id.fetch_add(1, Ordering::SeqCst),
            );

            // Add trip points
            zone.add_trip_point(TripPointType::Passive, tj_max - 10000, 2000);  // TJ Max - 10°C
            zone.add_trip_point(TripPointType::Hot, tj_max - 5000, 2000);       // TJ Max - 5°C
            zone.add_trip_point(TripPointType::Critical, tj_max, 2000);          // TJ Max

            self.zones.write().insert(zone.id, zone);
        }

        // Create package thermal zone
        let mut pkg_zone = ThermalZone::new(
            "cpu_package",
            ThermalZoneType::CpuPackage,
            self.next_zone_id.fetch_add(1, Ordering::SeqCst),
        );
        pkg_zone.add_trip_point(TripPointType::Passive, tj_max - 10000, 2000);
        pkg_zone.add_trip_point(TripPointType::Critical, tj_max, 2000);
        self.zones.write().insert(pkg_zone.id, pkg_zone);

        // Register CPU processor cooling device
        self.register_cooling_device(Box::new(ProcessorCooling::new()));

        self.initialized.store(true, Ordering::Release);

        crate::kprintln!("[THERMAL] Thermal subsystem initialized, TJ Max: {}°C", tj_max / 1000);

        Ok(())
    }

    /// Read TJ Max from CPU
    fn read_tj_max(&self) -> Temperature {
        let value = rdmsr(msr::MSR_TEMPERATURE_TARGET);
        let tj_max = ((value >> 16) & 0xFF) as i32;
        tj_max * 1000 // Convert to millidegrees
    }

    /// Read CPU temperature
    pub fn read_cpu_temperature(&self, _cpu: u32) -> Temperature {
        let tj_max = self.tj_max.load(Ordering::Acquire);

        // Read thermal status MSR
        // Note: This would need to be done on the specific CPU
        let status = rdmsr(msr::IA32_THERM_STATUS);

        // Digital readout is bits 22:16, represents degrees below TJ Max
        let readout = ((status >> 16) & 0x7F) as i32;

        tj_max - (readout * 1000)
    }

    /// Read package temperature
    pub fn read_package_temperature(&self) -> Temperature {
        let tj_max = self.tj_max.load(Ordering::Acquire);

        let status = rdmsr(msr::IA32_PACKAGE_THERM_STATUS);
        let readout = ((status >> 16) & 0x7F) as i32;

        tj_max - (readout * 1000)
    }

    /// Poll all thermal zones
    pub fn poll(&self) {
        if !self.initialized.load(Ordering::Acquire) {
            return;
        }

        let mut zones = self.zones.write();

        for (id, zone) in zones.iter_mut() {
            if !zone.enabled {
                continue;
            }

            // Read temperature based on zone type
            let temp = match zone.zone_type {
                ThermalZoneType::CpuCore => {
                    let cpu_id = zone.name.strip_prefix("cpu")
                        .and_then(|s| s.parse::<u32>().ok())
                        .unwrap_or(0);
                    self.read_cpu_temperature(cpu_id)
                }
                ThermalZoneType::CpuPackage => {
                    self.read_package_temperature()
                }
                _ => zone.temperature(),
            };

            zone.update_temperature(temp);

            // Check trip points
            let events = zone.check_trips();
            for (_, trip_type, tripped) in events {
                self.handle_trip_event(*id, trip_type, tripped);
            }
        }
    }

    /// Handle trip point event
    fn handle_trip_event(&self, zone_id: u32, trip_type: TripPointType, tripped: bool) {
        match (trip_type, tripped) {
            (TripPointType::Critical, true) => {
                crate::kprintln!("[THERMAL] CRITICAL: Zone {} reached critical temperature!", zone_id);
                // Would trigger emergency shutdown
            }
            (TripPointType::Hot, true) => {
                crate::kprintln!("[THERMAL] HOT: Zone {} is hot!", zone_id);
            }
            (TripPointType::Passive, true) => {
                crate::kprintln!("[THERMAL] Throttling zone {}", zone_id);
                self.throttle_active.store(true, Ordering::Release);
                self.activate_cooling();
            }
            (TripPointType::Passive, false) => {
                crate::kprintln!("[THERMAL] Unthrottling zone {}", zone_id);
                self.throttle_active.store(false, Ordering::Release);
            }
            (TripPointType::Active, true) => {
                self.activate_cooling();
            }
            _ => {}
        }
    }

    /// Activate cooling devices
    fn activate_cooling(&self) {
        let devices = self.cooling_devices.read();
        for (_id, device) in devices.iter() {
            let current = device.current_state();
            let max = device.max_state();
            if current < max {
                let _ = device.set_state(core::cmp::min(current + 1, max));
            }
        }
    }

    /// Register thermal zone
    pub fn register_zone(&self, mut zone: ThermalZone) -> u32 {
        if zone.id == 0 {
            zone.id = self.next_zone_id.fetch_add(1, Ordering::SeqCst);
        }
        let id = zone.id;
        self.zones.write().insert(id, zone);
        id
    }

    /// Unregister thermal zone
    pub fn unregister_zone(&self, id: u32) {
        self.zones.write().remove(&id);
    }

    /// Register cooling device
    pub fn register_cooling_device(&self, device: Box<dyn CoolingDevice>) -> u32 {
        let id = self.next_device_id.fetch_add(1, Ordering::SeqCst);
        self.cooling_devices.write().insert(id, device);
        id
    }

    /// Get zone temperature
    pub fn get_temperature(&self, zone_id: u32) -> Option<Temperature> {
        self.zones.read().get(&zone_id).map(|z| z.temperature())
    }

    /// Get all zone temperatures
    pub fn get_all_temperatures(&self) -> Vec<(u32, String, Temperature)> {
        self.zones.read()
            .iter()
            .map(|(id, zone)| (*id, zone.name.clone(), zone.temperature()))
            .collect()
    }

    /// Check if throttling is active
    pub fn is_throttling(&self) -> bool {
        self.throttle_active.load(Ordering::Acquire)
    }
}

/// Processor cooling device
struct ProcessorCooling {
    current_state: AtomicU32,
    max_state: u32,
}

impl ProcessorCooling {
    fn new() -> Self {
        Self {
            current_state: AtomicU32::new(0),
            max_state: 10,
        }
    }
}

impl CoolingDevice for ProcessorCooling {
    fn device_type(&self) -> CoolingDeviceType {
        CoolingDeviceType::Processor
    }

    fn name(&self) -> &str {
        "processor"
    }

    fn max_state(&self) -> u32 {
        self.max_state
    }

    fn current_state(&self) -> u32 {
        self.current_state.load(Ordering::Acquire)
    }

    fn set_state(&self, state: u32) -> Result<(), PowerError> {
        if state > self.max_state {
            return Err(PowerError::InvalidState);
        }

        self.current_state.store(state, Ordering::Release);

        // Apply throttling via cpufreq
        // Each state reduces max frequency by 10%
        let reduction = state * 10;
        let _max_pct = 100 - reduction;

        // Would set max frequency

        Ok(())
    }
}

/// Global thermal subsystem
static THERMAL: ThermalSubsystem = ThermalSubsystem::new();

fn rdmsr(msr: u32) -> u64 {
    let (low, high): (u32, u32);
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") low,
            out("edx") high,
        );
    }
    ((high as u64) << 32) | (low as u64)
}

/// Initialize thermal subsystem
pub fn init() {
    if let Err(e) = THERMAL.init() {
        crate::kprintln!("[THERMAL] Initialization failed: {:?}", e);
    }
}

/// Poll thermal zones
pub fn poll() {
    THERMAL.poll();
}

/// Get temperature for zone
pub fn get_temperature(zone_id: u32) -> Option<Temperature> {
    THERMAL.get_temperature(zone_id)
}

/// Get all temperatures
pub fn get_all_temperatures() -> Vec<(u32, String, Temperature)> {
    THERMAL.get_all_temperatures()
}

/// Check if throttling is active
pub fn is_throttling() -> bool {
    THERMAL.is_throttling()
}

/// Register thermal zone
pub fn register_zone(zone: ThermalZone) -> u32 {
    THERMAL.register_zone(zone)
}

/// Register cooling device
pub fn register_cooling_device(device: Box<dyn CoolingDevice>) -> u32 {
    THERMAL.register_cooling_device(device)
}
