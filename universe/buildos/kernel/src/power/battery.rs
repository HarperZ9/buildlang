//! Battery Management
//!
//! Provides battery monitoring and power supply management:
//! - Battery status and capacity
//! - Charging control
//! - Power supply enumeration
//! - AC adapter status
//! - Battery health monitoring

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::{Mutex, RwLock};
use super::PowerError;

/// Power supply type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerSupplyType {
    /// Battery
    Battery,
    /// AC adapter (mains)
    Mains,
    /// USB power
    Usb,
    /// USB Type-C PD
    UsbPd,
    /// Wireless charging
    Wireless,
    /// Unknown
    Unknown,
}

/// Battery status
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatteryStatus {
    /// Unknown
    Unknown,
    /// Charging
    Charging,
    /// Discharging
    Discharging,
    /// Not charging (full or limited)
    NotCharging,
    /// Full
    Full,
}

/// Battery health
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatteryHealth {
    /// Unknown
    Unknown,
    /// Good
    Good,
    /// Overheat
    Overheat,
    /// Dead
    Dead,
    /// OverVoltage
    OverVoltage,
    /// Cold
    Cold,
    /// WatchdogTimerExpire
    WatchdogTimerExpire,
    /// SafetyTimerExpire
    SafetyTimerExpire,
}

/// Battery technology
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatteryTechnology {
    /// Unknown
    Unknown,
    /// Lithium Ion
    LiIon,
    /// Lithium Polymer
    LiPoly,
    /// Lithium Iron Phosphate
    LiFe,
    /// Nickel Metal Hydride
    NiMH,
    /// Nickel Cadmium
    NiCd,
}

/// Power supply information
#[derive(Clone, Debug)]
pub struct PowerSupplyInfo {
    /// Name
    pub name: String,
    /// Type
    pub supply_type: PowerSupplyType,
    /// Is online (connected)
    pub online: bool,
    /// Status
    pub status: BatteryStatus,
    /// Present
    pub present: bool,
    /// Voltage (microvolts)
    pub voltage_uv: u32,
    /// Current (microamps, positive = charging)
    pub current_ua: i32,
    /// Power (microwatts)
    pub power_uw: u32,
    /// Capacity (percentage)
    pub capacity: u8,
    /// Capacity level
    pub capacity_level: CapacityLevel,
    /// Temperature (millidegrees C)
    pub temperature: i32,
    /// Health
    pub health: BatteryHealth,
    /// Technology
    pub technology: BatteryTechnology,
    /// Design capacity (microampere-hours)
    pub charge_full_design: u32,
    /// Full charge capacity
    pub charge_full: u32,
    /// Current charge
    pub charge_now: u32,
    /// Energy design (microwatt-hours)
    pub energy_full_design: u32,
    /// Energy full
    pub energy_full: u32,
    /// Energy now
    pub energy_now: u32,
    /// Cycle count
    pub cycle_count: u32,
    /// Manufacturer
    pub manufacturer: String,
    /// Model
    pub model: String,
    /// Serial number
    pub serial: String,
}

/// Capacity level
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapacityLevel {
    /// Unknown
    Unknown,
    /// Critical
    Critical,
    /// Low
    Low,
    /// Normal
    Normal,
    /// High
    High,
    /// Full
    Full,
}

impl CapacityLevel {
    /// From percentage
    pub fn from_percentage(pct: u8) -> Self {
        match pct {
            0..=5 => Self::Critical,
            6..=20 => Self::Low,
            21..=80 => Self::Normal,
            81..=99 => Self::High,
            100 => Self::Full,
            _ => Self::Unknown,
        }
    }
}

/// Battery subsystem
pub struct BatterySubsystem {
    /// Is initialized
    initialized: AtomicBool,
    /// Power supplies
    supplies: RwLock<BTreeMap<u32, PowerSupplyInfo>>,
    /// Next supply ID
    next_id: AtomicU32,
    /// AC online
    ac_online: AtomicBool,
    /// On battery power
    on_battery: AtomicBool,
    /// Low battery threshold (%)
    low_threshold: AtomicU32,
    /// Critical battery threshold (%)
    critical_threshold: AtomicU32,
    /// Statistics
    stats: BatteryStats,
}

/// Battery statistics
#[derive(Debug, Default)]
struct BatteryStats {
    /// Total discharge cycles
    discharge_cycles: AtomicU64,
    /// Total time on battery (seconds)
    battery_time: AtomicU64,
    /// Total time on AC (seconds)
    ac_time: AtomicU64,
    /// Low battery events
    low_events: AtomicU64,
    /// Critical battery events
    critical_events: AtomicU64,
}

/// Battery event
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatteryEvent {
    /// AC connected
    AcConnected,
    /// AC disconnected
    AcDisconnected,
    /// Battery inserted
    BatteryInserted,
    /// Battery removed
    BatteryRemoved,
    /// Low battery
    LowBattery,
    /// Critical battery
    CriticalBattery,
    /// Fully charged
    FullyCharged,
    /// Overheating
    Overheating,
}

/// Event handler
static EVENT_HANDLER: Mutex<Option<fn(BatteryEvent)>> = Mutex::new(None);

impl BatterySubsystem {
    /// Create new battery subsystem
    pub const fn new() -> Self {
        Self {
            initialized: AtomicBool::new(false),
            supplies: RwLock::new(BTreeMap::new()),
            next_id: AtomicU32::new(1),
            ac_online: AtomicBool::new(true),
            on_battery: AtomicBool::new(false),
            low_threshold: AtomicU32::new(20),
            critical_threshold: AtomicU32::new(5),
            stats: BatteryStats {
                discharge_cycles: AtomicU64::new(0),
                battery_time: AtomicU64::new(0),
                ac_time: AtomicU64::new(0),
                low_events: AtomicU64::new(0),
                critical_events: AtomicU64::new(0),
            },
        }
    }

    /// Initialize battery subsystem
    pub fn init(&self) -> Result<(), PowerError> {
        // Probe for power supplies via ACPI
        self.probe_acpi_supplies();

        // Initial poll
        self.poll();

        self.initialized.store(true, Ordering::Release);

        let supplies = self.supplies.read();
        let battery_count = supplies.values()
            .filter(|s| s.supply_type == PowerSupplyType::Battery)
            .count();
        let ac_count = supplies.values()
            .filter(|s| s.supply_type == PowerSupplyType::Mains)
            .count();

        crate::kprintln!("[BATTERY] Found {} batteries, {} AC adapters", battery_count, ac_count);

        Ok(())
    }

    /// Probe ACPI power supplies
    fn probe_acpi_supplies(&self) {
        // Would enumerate ACPI devices
        // For now, create default supplies

        // AC adapter
        let ac = PowerSupplyInfo {
            name: String::from("AC0"),
            supply_type: PowerSupplyType::Mains,
            online: true,
            status: BatteryStatus::Unknown,
            present: true,
            voltage_uv: 0,
            current_ua: 0,
            power_uw: 0,
            capacity: 0,
            capacity_level: CapacityLevel::Unknown,
            temperature: 0,
            health: BatteryHealth::Unknown,
            technology: BatteryTechnology::Unknown,
            charge_full_design: 0,
            charge_full: 0,
            charge_now: 0,
            energy_full_design: 0,
            energy_full: 0,
            energy_now: 0,
            cycle_count: 0,
            manufacturer: String::new(),
            model: String::new(),
            serial: String::new(),
        };

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        self.supplies.write().insert(id, ac);

        // Battery (if laptop)
        let battery = PowerSupplyInfo {
            name: String::from("BAT0"),
            supply_type: PowerSupplyType::Battery,
            online: true,
            status: BatteryStatus::Discharging,
            present: true,
            voltage_uv: 11400000,  // 11.4V
            current_ua: -1500000,  // -1.5A (discharging)
            power_uw: 17100000,    // 17.1W
            capacity: 75,
            capacity_level: CapacityLevel::Normal,
            temperature: 28000,    // 28°C
            health: BatteryHealth::Good,
            technology: BatteryTechnology::LiIon,
            charge_full_design: 6800000,  // 6800mAh
            charge_full: 6500000,          // 6500mAh (degraded)
            charge_now: 4875000,           // 4875mAh (75%)
            energy_full_design: 77520000,  // 77.52Wh
            energy_full: 74100000,         // 74.1Wh
            energy_now: 55575000,          // 55.575Wh
            cycle_count: 250,
            manufacturer: String::from("SMP"),
            model: String::from("DELL 7FXNW"),
            serial: String::from("12345"),
        };

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        self.supplies.write().insert(id, battery);
    }

    /// Poll power supplies
    pub fn poll(&self) {
        let mut supplies = self.supplies.write();

        // Update AC status
        let was_on_battery = self.on_battery.load(Ordering::Acquire);
        let mut ac_online = false;
        let mut has_battery = false;
        let mut min_capacity = 100u8;

        for (_id, supply) in supplies.iter_mut() {
            match supply.supply_type {
                PowerSupplyType::Mains | PowerSupplyType::Usb | PowerSupplyType::UsbPd => {
                    // Would read actual status from hardware
                    ac_online |= supply.online;
                }
                PowerSupplyType::Battery => {
                    has_battery = true;
                    if supply.capacity < min_capacity {
                        min_capacity = supply.capacity;
                    }

                    // Update status based on AC
                    if ac_online {
                        if supply.capacity >= 100 {
                            supply.status = BatteryStatus::Full;
                        } else {
                            supply.status = BatteryStatus::Charging;
                        }
                        supply.current_ua = supply.current_ua.abs();  // Positive for charging
                    } else {
                        supply.status = BatteryStatus::Discharging;
                        supply.current_ua = -supply.current_ua.abs(); // Negative for discharging
                    }

                    supply.capacity_level = CapacityLevel::from_percentage(supply.capacity);
                }
                _ => {}
            }
        }

        self.ac_online.store(ac_online, Ordering::Release);
        self.on_battery.store(has_battery && !ac_online, Ordering::Release);

        // Generate events
        let now_on_battery = self.on_battery.load(Ordering::Acquire);

        if was_on_battery != now_on_battery {
            if now_on_battery {
                self.fire_event(BatteryEvent::AcDisconnected);
            } else {
                self.fire_event(BatteryEvent::AcConnected);
            }
        }

        // Check thresholds
        if has_battery && now_on_battery {
            let low = self.low_threshold.load(Ordering::Acquire) as u8;
            let critical = self.critical_threshold.load(Ordering::Acquire) as u8;

            if min_capacity <= critical {
                self.stats.critical_events.fetch_add(1, Ordering::Relaxed);
                self.fire_event(BatteryEvent::CriticalBattery);
            } else if min_capacity <= low {
                self.stats.low_events.fetch_add(1, Ordering::Relaxed);
                self.fire_event(BatteryEvent::LowBattery);
            }
        }
    }

    /// Fire battery event
    fn fire_event(&self, event: BatteryEvent) {
        if let Some(handler) = *EVENT_HANDLER.lock() {
            handler(event);
        }
    }

    /// Get power supply info
    pub fn get_supply(&self, id: u32) -> Option<PowerSupplyInfo> {
        self.supplies.read().get(&id).cloned()
    }

    /// Get all supplies
    pub fn get_all_supplies(&self) -> Vec<(u32, PowerSupplyInfo)> {
        self.supplies.read()
            .iter()
            .map(|(id, info)| (*id, info.clone()))
            .collect()
    }

    /// Get batteries only
    pub fn get_batteries(&self) -> Vec<(u32, PowerSupplyInfo)> {
        self.supplies.read()
            .iter()
            .filter(|(_, info)| info.supply_type == PowerSupplyType::Battery)
            .map(|(id, info)| (*id, info.clone()))
            .collect()
    }

    /// Is on battery power
    pub fn is_on_battery(&self) -> bool {
        self.on_battery.load(Ordering::Acquire)
    }

    /// Is AC online
    pub fn is_ac_online(&self) -> bool {
        self.ac_online.load(Ordering::Acquire)
    }

    /// Get combined battery capacity
    pub fn get_capacity(&self) -> u8 {
        let supplies = self.supplies.read();

        let batteries: Vec<_> = supplies.values()
            .filter(|s| s.supply_type == PowerSupplyType::Battery && s.present)
            .collect();

        if batteries.is_empty() {
            return 0;
        }

        // Weighted average by design capacity
        let total_design: u32 = batteries.iter().map(|b| b.charge_full_design).sum();
        let total_now: u32 = batteries.iter().map(|b| b.charge_now).sum();

        if total_design > 0 {
            ((total_now as u64 * 100) / total_design as u64) as u8
        } else {
            batteries.iter().map(|b| b.capacity as u32).sum::<u32>() as u8 / batteries.len() as u8
        }
    }

    /// Estimate time remaining (minutes)
    pub fn estimate_time_remaining(&self) -> Option<u32> {
        let supplies = self.supplies.read();

        let batteries: Vec<_> = supplies.values()
            .filter(|s| s.supply_type == PowerSupplyType::Battery && s.present)
            .collect();

        if batteries.is_empty() {
            return None;
        }

        // Sum power consumption
        let total_power_uw: i64 = batteries.iter()
            .map(|b| (b.voltage_uv as i64 * b.current_ua.abs() as i64) / 1000000)
            .sum();

        if total_power_uw <= 0 {
            return None;
        }

        // Total energy remaining
        let total_energy_uwh: u64 = batteries.iter()
            .map(|b| b.energy_now as u64)
            .sum();

        // Time = Energy / Power
        let minutes = (total_energy_uwh * 60) / (total_power_uw as u64);

        Some(minutes as u32)
    }

    /// Set low battery threshold
    pub fn set_low_threshold(&self, percent: u8) {
        self.low_threshold.store(percent as u32, Ordering::Release);
    }

    /// Set critical battery threshold
    pub fn set_critical_threshold(&self, percent: u8) {
        self.critical_threshold.store(percent as u32, Ordering::Release);
    }

    /// Register event handler
    pub fn set_event_handler(&self, handler: fn(BatteryEvent)) {
        *EVENT_HANDLER.lock() = Some(handler);
    }

    /// Get battery health percentage
    pub fn get_health(&self, id: u32) -> Option<u8> {
        let supplies = self.supplies.read();
        let supply = supplies.get(&id)?;

        if supply.supply_type != PowerSupplyType::Battery {
            return None;
        }

        if supply.charge_full_design == 0 {
            return None;
        }

        Some(((supply.charge_full as u64 * 100) / supply.charge_full_design as u64) as u8)
    }
}

/// Global battery subsystem
static BATTERY: BatterySubsystem = BatterySubsystem::new();

/// Initialize battery subsystem
pub fn init() {
    if let Err(e) = BATTERY.init() {
        crate::kprintln!("[BATTERY] Initialization failed: {:?}", e);
    }
}

/// Poll power supplies
pub fn poll() {
    BATTERY.poll();
}

/// Is on battery power
pub fn is_on_battery() -> bool {
    BATTERY.is_on_battery()
}

/// Get battery capacity
pub fn get_capacity() -> u8 {
    BATTERY.get_capacity()
}

/// Get time remaining (minutes)
pub fn get_time_remaining() -> Option<u32> {
    BATTERY.estimate_time_remaining()
}

/// Get all batteries
pub fn get_batteries() -> Vec<(u32, PowerSupplyInfo)> {
    BATTERY.get_batteries()
}

/// Set event handler
pub fn set_event_handler(handler: fn(BatteryEvent)) {
    BATTERY.set_event_handler(handler);
}
