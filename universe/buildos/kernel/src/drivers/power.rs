//! Power Management Subsystem
//!
//! Provides ACPI-based power management including:
//! - System sleep states (S0-S5)
//! - CPU power states (C-states, P-states)
//! - Device power management (D-states)
//! - Thermal management
//! - Battery management
//! - Power button handling

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicBool, Ordering};
use crate::sync::{Mutex, RwLock};

/// System sleep states (ACPI S-states)
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SleepState {
    /// S0: Working state (normal operation)
    S0Working = 0,
    /// S1: Power On Suspend (CPU stops, memory refreshed)
    S1PowerOnSuspend = 1,
    /// S2: CPU powered off, memory refreshed
    S2 = 2,
    /// S3: Suspend to RAM (STR)
    S3SuspendToRam = 3,
    /// S4: Suspend to Disk / Hibernate (STD)
    S4Hibernate = 4,
    /// S5: Soft Off (requires full boot to wake)
    S5SoftOff = 5,
}

impl SleepState {
    /// Get state name
    pub fn name(&self) -> &'static str {
        match self {
            Self::S0Working => "S0 (Working)",
            Self::S1PowerOnSuspend => "S1 (Power On Suspend)",
            Self::S2 => "S2",
            Self::S3SuspendToRam => "S3 (Suspend to RAM)",
            Self::S4Hibernate => "S4 (Hibernate)",
            Self::S5SoftOff => "S5 (Soft Off)",
        }
    }
}

/// CPU C-states (idle power states)
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CState {
    /// C0: Active (executing instructions)
    C0Active = 0,
    /// C1: Halt (CPU clock stopped, cache maintained)
    C1Halt = 1,
    /// C1E: Enhanced Halt (lower voltage)
    C1E = 2,
    /// C2: Stop Clock
    C2StopClock = 3,
    /// C3: Deep Sleep (cache may be flushed)
    C3DeepSleep = 4,
    /// C6: Deep Power Down
    C6DeepPowerDown = 5,
    /// C7: Deeper Power Down
    C7 = 6,
    /// C8: Deepest Power Down
    C8 = 7,
    /// C10: Package C10
    C10 = 8,
}

impl CState {
    /// Get exit latency in microseconds
    pub fn exit_latency(&self) -> u32 {
        match self {
            Self::C0Active => 0,
            Self::C1Halt => 1,
            Self::C1E => 2,
            Self::C2StopClock => 10,
            Self::C3DeepSleep => 50,
            Self::C6DeepPowerDown => 100,
            Self::C7 => 150,
            Self::C8 => 200,
            Self::C10 => 500,
        }
    }

    /// Get power consumption (relative, C0 = 100)
    pub fn power(&self) -> u32 {
        match self {
            Self::C0Active => 100,
            Self::C1Halt => 80,
            Self::C1E => 70,
            Self::C2StopClock => 50,
            Self::C3DeepSleep => 30,
            Self::C6DeepPowerDown => 15,
            Self::C7 => 10,
            Self::C8 => 5,
            Self::C10 => 2,
        }
    }
}

/// CPU P-states (performance states)
#[derive(Clone, Debug)]
pub struct PState {
    /// Frequency in MHz
    pub frequency: u32,
    /// Voltage in mV
    pub voltage: u32,
    /// Power consumption in mW
    pub power: u32,
    /// Control value (to write to MSR)
    pub control: u64,
    /// Status value
    pub status: u64,
}

/// Device power states (D-states)
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DState {
    /// D0: Fully On (device operational)
    D0 = 0,
    /// D1: Intermediate power state
    D1 = 1,
    /// D2: Intermediate power state (lower than D1)
    D2 = 2,
    /// D3hot: Device off but power maintained
    D3Hot = 3,
    /// D3cold: Device off, power removed
    D3Cold = 4,
}

/// Power source type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerSource {
    /// AC power (plugged in)
    AC,
    /// Battery power
    Battery,
    /// UPS
    UPS,
    /// Unknown
    Unknown,
}

/// Battery information
#[derive(Clone, Debug)]
pub struct BatteryInfo {
    /// Battery present
    pub present: bool,
    /// Is charging
    pub charging: bool,
    /// Is discharging
    pub discharging: bool,
    /// Battery level (0-100%)
    pub level: u8,
    /// Design capacity (mWh)
    pub design_capacity: u32,
    /// Current capacity (mWh)
    pub current_capacity: u32,
    /// Full charge capacity (mWh)
    pub full_capacity: u32,
    /// Current voltage (mV)
    pub voltage: u32,
    /// Discharge rate (mW)
    pub discharge_rate: u32,
    /// Time remaining (minutes)
    pub time_remaining: Option<u32>,
    /// Battery health (0-100%)
    pub health: u8,
    /// Cycle count
    pub cycles: u32,
    /// Manufacturer
    pub manufacturer: String,
    /// Model
    pub model: String,
    /// Serial number
    pub serial: String,
}

impl Default for BatteryInfo {
    fn default() -> Self {
        Self {
            present: false,
            charging: false,
            discharging: false,
            level: 0,
            design_capacity: 0,
            current_capacity: 0,
            full_capacity: 0,
            voltage: 0,
            discharge_rate: 0,
            time_remaining: None,
            health: 100,
            cycles: 0,
            manufacturer: String::new(),
            model: String::new(),
            serial: String::new(),
        }
    }
}

/// Thermal zone information
#[derive(Clone, Debug)]
pub struct ThermalZone {
    /// Zone ID
    pub id: u32,
    /// Zone name
    pub name: String,
    /// Current temperature (millidegrees Celsius)
    pub temperature: i32,
    /// Trip points
    pub trip_points: Vec<TripPoint>,
    /// Cooling devices
    pub cooling_devices: Vec<u32>,
    /// Policy
    pub policy: ThermalPolicy,
}

/// Thermal trip point
#[derive(Clone, Debug)]
pub struct TripPoint {
    /// Trip point type
    pub trip_type: TripType,
    /// Temperature threshold (millidegrees)
    pub temperature: i32,
    /// Hysteresis
    pub hysteresis: i32,
}

/// Trip point types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TripType {
    /// Critical - system must shut down
    Critical,
    /// Hot - system should throttle
    Hot,
    /// Passive cooling (throttle)
    Passive,
    /// Active cooling (fan)
    Active,
}

/// Thermal policy
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThermalPolicy {
    /// Step-wise (discrete cooling steps)
    StepWise,
    /// Fair-share (proportional control)
    FairShare,
    /// Power allocator (power budget)
    PowerAllocator,
    /// User space control
    UserSpace,
}

/// Power event
#[derive(Clone, Debug)]
pub enum PowerEvent {
    /// Power button pressed
    PowerButton,
    /// Sleep button pressed
    SleepButton,
    /// Lid opened
    LidOpen,
    /// Lid closed
    LidClose,
    /// AC connected
    ACConnect,
    /// AC disconnected
    ACDisconnect,
    /// Battery level changed
    BatteryLevel(u8),
    /// Battery low
    BatteryLow,
    /// Battery critical
    BatteryCritical,
    /// Thermal event
    Thermal(u32, i32), // zone, temperature
    /// Thermal critical
    ThermalCritical(u32),
}

/// Power management governor
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Governor {
    /// Performance - maximum frequency
    Performance,
    /// Powersave - minimum frequency
    Powersave,
    /// Ondemand - scale with load
    Ondemand,
    /// Conservative - gradual scaling
    Conservative,
    /// Userspace - manual control
    Userspace,
    /// Schedutil - scheduler-driven
    Schedutil,
}

/// Power manager
pub struct PowerManager {
    /// Current sleep state
    current_state: AtomicU32,
    /// Available sleep states
    available_states: Vec<SleepState>,
    /// Current power source
    power_source: RwLock<PowerSource>,
    /// Battery info
    battery: RwLock<BatteryInfo>,
    /// Thermal zones
    thermal_zones: RwLock<Vec<ThermalZone>>,
    /// P-states
    p_states: Vec<PState>,
    /// Current P-state index
    current_p_state: AtomicU32,
    /// C-states enabled
    c_states_enabled: Vec<CState>,
    /// Current governor
    governor: RwLock<Governor>,
    /// Power event handlers
    event_handlers: Mutex<Vec<fn(PowerEvent)>>,
    /// Suspend hooks (called before suspend)
    suspend_hooks: Mutex<Vec<fn() -> Result<(), PowerError>>>,
    /// Resume hooks (called after resume)
    resume_hooks: Mutex<Vec<fn()>>,
    /// AC adapter present
    ac_present: AtomicBool,
    /// Lid open
    lid_open: AtomicBool,
}

impl PowerManager {
    /// Create a new power manager
    pub fn new() -> Self {
        Self {
            current_state: AtomicU32::new(SleepState::S0Working as u32),
            available_states: vec![
                SleepState::S0Working,
                SleepState::S3SuspendToRam,
                SleepState::S4Hibernate,
                SleepState::S5SoftOff,
            ],
            power_source: RwLock::new(PowerSource::AC),
            battery: RwLock::new(BatteryInfo::default()),
            thermal_zones: RwLock::new(Vec::new()),
            p_states: Vec::new(),
            current_p_state: AtomicU32::new(0),
            c_states_enabled: vec![
                CState::C0Active,
                CState::C1Halt,
                CState::C1E,
                CState::C3DeepSleep,
            ],
            governor: RwLock::new(Governor::Ondemand),
            event_handlers: Mutex::new(Vec::new()),
            suspend_hooks: Mutex::new(Vec::new()),
            resume_hooks: Mutex::new(Vec::new()),
            ac_present: AtomicBool::new(true),
            lid_open: AtomicBool::new(true),
        }
    }

    /// Initialize from ACPI
    pub fn init_from_acpi(&mut self) {
        // Detect available sleep states from ACPI
        self.detect_sleep_states();

        // Detect P-states
        self.detect_p_states();

        // Detect C-states
        self.detect_c_states();

        // Initialize thermal zones
        self.init_thermal_zones();

        // Detect power source
        self.update_power_source();
    }

    /// Detect available sleep states
    fn detect_sleep_states(&mut self) {
        // Would parse ACPI \_S0-\_S5 objects
        self.available_states = vec![
            SleepState::S0Working,
            SleepState::S3SuspendToRam,
            SleepState::S5SoftOff,
        ];
    }

    /// Detect P-states
    fn detect_p_states(&mut self) {
        // Would read from ACPI _PSS or MSR
        // Example P-states for a modern CPU
        self.p_states = vec![
            PState { frequency: 3600, voltage: 1100, power: 95000, control: 0x24, status: 0 },
            PState { frequency: 3200, voltage: 1050, power: 80000, control: 0x20, status: 0 },
            PState { frequency: 2800, voltage: 1000, power: 65000, control: 0x1C, status: 0 },
            PState { frequency: 2400, voltage: 950, power: 50000, control: 0x18, status: 0 },
            PState { frequency: 2000, voltage: 900, power: 35000, control: 0x14, status: 0 },
            PState { frequency: 1600, voltage: 850, power: 25000, control: 0x10, status: 0 },
            PState { frequency: 1200, voltage: 800, power: 18000, control: 0x0C, status: 0 },
            PState { frequency: 800, voltage: 750, power: 12000, control: 0x08, status: 0 },
        ];
    }

    /// Detect C-states
    fn detect_c_states(&mut self) {
        // Would read from ACPI _CST
        self.c_states_enabled = vec![
            CState::C0Active,
            CState::C1Halt,
            CState::C1E,
            CState::C3DeepSleep,
            CState::C6DeepPowerDown,
        ];
    }

    /// Initialize thermal zones
    fn init_thermal_zones(&mut self) {
        // Would parse ACPI thermal zone objects
        let mut zones = self.thermal_zones.write();
        zones.push(ThermalZone {
            id: 0,
            name: "CPU".into(),
            temperature: 45000, // 45°C
            trip_points: vec![
                TripPoint { trip_type: TripType::Passive, temperature: 85000, hysteresis: 3000 },
                TripPoint { trip_type: TripType::Active, temperature: 70000, hysteresis: 2000 },
                TripPoint { trip_type: TripType::Critical, temperature: 100000, hysteresis: 0 },
            ],
            cooling_devices: vec![0],
            policy: ThermalPolicy::StepWise,
        });
    }

    /// Update power source
    fn update_power_source(&self) {
        // Would read ACPI _PSR method
        let mut source = self.power_source.write();
        *source = if self.ac_present.load(Ordering::Relaxed) {
            PowerSource::AC
        } else {
            PowerSource::Battery
        };
    }

    /// Get current sleep state
    pub fn current_state(&self) -> SleepState {
        match self.current_state.load(Ordering::Acquire) {
            0 => SleepState::S0Working,
            1 => SleepState::S1PowerOnSuspend,
            2 => SleepState::S2,
            3 => SleepState::S3SuspendToRam,
            4 => SleepState::S4Hibernate,
            _ => SleepState::S5SoftOff,
        }
    }

    /// Get available sleep states
    pub fn available_states(&self) -> &[SleepState] {
        &self.available_states
    }

    /// Check if sleep state is supported
    pub fn supports_state(&self, state: SleepState) -> bool {
        self.available_states.contains(&state)
    }

    /// Suspend to RAM (S3)
    pub fn suspend(&self) -> Result<(), PowerError> {
        if !self.supports_state(SleepState::S3SuspendToRam) {
            return Err(PowerError::NotSupported);
        }

        // Call suspend hooks
        for hook in self.suspend_hooks.lock().iter() {
            hook()?;
        }

        // Prepare devices for suspend
        self.prepare_devices_for_suspend()?;

        // Save CPU state
        save_cpu_state();

        // Enter S3
        self.enter_sleep_state(SleepState::S3SuspendToRam)?;

        // --- System is suspended here ---

        // Restore CPU state (after wake)
        restore_cpu_state();

        // Resume devices
        self.resume_devices();

        // Call resume hooks
        for hook in self.resume_hooks.lock().iter() {
            hook();
        }

        Ok(())
    }

    /// Hibernate (S4)
    pub fn hibernate(&self) -> Result<(), PowerError> {
        if !self.supports_state(SleepState::S4Hibernate) {
            return Err(PowerError::NotSupported);
        }

        // Save memory to disk
        save_memory_to_disk()?;

        // Call suspend hooks
        for hook in self.suspend_hooks.lock().iter() {
            hook()?;
        }

        // Prepare devices
        self.prepare_devices_for_suspend()?;

        // Enter S4
        self.enter_sleep_state(SleepState::S4Hibernate)?;

        Ok(())
    }

    /// Shutdown (S5)
    pub fn shutdown(&self) -> Result<(), PowerError> {
        // Call suspend hooks
        for hook in self.suspend_hooks.lock().iter() {
            let _ = hook();
        }

        // Prepare devices
        let _ = self.prepare_devices_for_suspend();

        // Enter S5
        self.enter_sleep_state(SleepState::S5SoftOff)?;

        // Should not return
        Ok(())
    }

    /// Reboot
    pub fn reboot(&self) -> Result<(), PowerError> {
        // Call suspend hooks
        for hook in self.suspend_hooks.lock().iter() {
            let _ = hook();
        }

        // Trigger reset
        trigger_reset();

        // Should not return
        Ok(())
    }

    /// Enter a sleep state
    fn enter_sleep_state(&self, state: SleepState) -> Result<(), PowerError> {
        self.current_state.store(state as u32, Ordering::Release);

        match state {
            SleepState::S3SuspendToRam => {
                // Write SLP_TYP and SLP_EN to PM1a/PM1b control registers
                unsafe {
                    enter_s3();
                }
            }
            SleepState::S4Hibernate => {
                unsafe {
                    enter_s4();
                }
            }
            SleepState::S5SoftOff => {
                unsafe {
                    enter_s5();
                }
            }
            _ => return Err(PowerError::NotSupported),
        }

        // Set state back to S0 after wake
        self.current_state.store(SleepState::S0Working as u32, Ordering::Release);
        Ok(())
    }

    /// Prepare devices for suspend
    fn prepare_devices_for_suspend(&self) -> Result<(), PowerError> {
        // Would iterate through device tree and call suspend method
        Ok(())
    }

    /// Resume devices after wake
    fn resume_devices(&self) {
        // Would iterate through device tree and call resume method
    }

    /// Set P-state (frequency scaling)
    pub fn set_p_state(&self, index: usize) -> Result<(), PowerError> {
        if index >= self.p_states.len() {
            return Err(PowerError::InvalidState);
        }

        let p_state = &self.p_states[index];

        // Write to MSR or ACPI _PPC
        unsafe {
            set_cpu_frequency(p_state.control);
        }

        self.current_p_state.store(index as u32, Ordering::Release);
        Ok(())
    }

    /// Get current P-state
    pub fn current_p_state(&self) -> usize {
        self.current_p_state.load(Ordering::Acquire) as usize
    }

    /// Get all P-states
    pub fn p_states(&self) -> &[PState] {
        &self.p_states
    }

    /// Get current frequency (MHz)
    pub fn current_frequency(&self) -> u32 {
        let idx = self.current_p_state();
        self.p_states.get(idx).map(|p| p.frequency).unwrap_or(0)
    }

    /// Set governor
    pub fn set_governor(&self, governor: Governor) {
        *self.governor.write() = governor;

        // Apply governor settings
        match governor {
            Governor::Performance => {
                let _ = self.set_p_state(0); // Highest frequency
            }
            Governor::Powersave => {
                let _ = self.set_p_state(self.p_states.len().saturating_sub(1)); // Lowest
            }
            _ => {}
        }
    }

    /// Get current governor
    pub fn governor(&self) -> Governor {
        *self.governor.read()
    }

    /// Idle CPU (enter C-state)
    pub fn idle(&self) {
        let governor = self.governor();

        // Select C-state based on expected idle time and governor
        let c_state = match governor {
            Governor::Performance => CState::C1Halt, // Shallow idle
            Governor::Powersave => self.deepest_c_state(), // Deep idle
            _ => self.select_c_state(), // Dynamic selection
        };

        // Enter C-state
        unsafe {
            enter_c_state(c_state);
        }
    }

    /// Select C-state based on idle prediction
    fn select_c_state(&self) -> CState {
        // Would use idle predictor
        CState::C1E
    }

    /// Get deepest available C-state
    fn deepest_c_state(&self) -> CState {
        self.c_states_enabled.last().copied().unwrap_or(CState::C1Halt)
    }

    /// Get power source
    pub fn power_source(&self) -> PowerSource {
        *self.power_source.read()
    }

    /// Get battery info
    pub fn battery(&self) -> BatteryInfo {
        self.battery.read().clone()
    }

    /// Update battery info
    pub fn update_battery(&self) {
        // Would read from ACPI battery methods
        let mut battery = self.battery.write();

        // Mock update
        if battery.present && battery.discharging {
            battery.current_capacity = battery.current_capacity.saturating_sub(1);
            battery.level = ((battery.current_capacity * 100) / battery.full_capacity.max(1)) as u8;
        }
    }

    /// Get thermal zones
    pub fn thermal_zones(&self) -> Vec<ThermalZone> {
        self.thermal_zones.read().clone()
    }

    /// Get temperature for a zone (millidegrees)
    pub fn temperature(&self, zone_id: u32) -> Option<i32> {
        self.thermal_zones.read()
            .iter()
            .find(|z| z.id == zone_id)
            .map(|z| z.temperature)
    }

    /// Update thermal zones
    pub fn update_thermal(&self) {
        let mut zones = self.thermal_zones.write();
        for zone in zones.iter_mut() {
            // Would read from ACPI _TMP method
            // Check trip points and trigger cooling if needed
            for trip in &zone.trip_points {
                if zone.temperature >= trip.temperature {
                    match trip.trip_type {
                        TripType::Critical => {
                            // Emergency shutdown
                            crate::kprintln!("[POWER] Critical temperature! Shutting down...");
                        }
                        TripType::Hot => {
                            // Throttle CPU
                        }
                        TripType::Passive => {
                            // Reduce frequency
                        }
                        TripType::Active => {
                            // Enable fan
                        }
                    }
                }
            }
        }
    }

    /// Register power event handler
    pub fn register_event_handler(&self, handler: fn(PowerEvent)) {
        self.event_handlers.lock().push(handler);
    }

    /// Register suspend hook
    pub fn register_suspend_hook(&self, hook: fn() -> Result<(), PowerError>) {
        self.suspend_hooks.lock().push(hook);
    }

    /// Register resume hook
    pub fn register_resume_hook(&self, hook: fn()) {
        self.resume_hooks.lock().push(hook);
    }

    /// Fire power event
    pub fn fire_event(&self, event: PowerEvent) {
        for handler in self.event_handlers.lock().iter() {
            handler(event.clone());
        }
    }

    /// Handle power button press
    pub fn handle_power_button(&self) {
        self.fire_event(PowerEvent::PowerButton);

        // Default action: suspend
        if self.supports_state(SleepState::S3SuspendToRam) {
            let _ = self.suspend();
        }
    }

    /// Handle sleep button press
    pub fn handle_sleep_button(&self) {
        self.fire_event(PowerEvent::SleepButton);
        let _ = self.suspend();
    }

    /// Handle lid switch
    pub fn handle_lid(&self, open: bool) {
        self.lid_open.store(open, Ordering::Release);

        if open {
            self.fire_event(PowerEvent::LidOpen);
        } else {
            self.fire_event(PowerEvent::LidClose);
            // Default action: suspend on lid close
            let _ = self.suspend();
        }
    }

    /// Handle AC adapter event
    pub fn handle_ac_adapter(&self, connected: bool) {
        self.ac_present.store(connected, Ordering::Release);
        self.update_power_source();

        if connected {
            self.fire_event(PowerEvent::ACConnect);
            // Switch to performance on AC
            self.set_governor(Governor::Ondemand);
        } else {
            self.fire_event(PowerEvent::ACDisconnect);
            // Switch to powersave on battery
            self.set_governor(Governor::Conservative);
        }
    }
}

/// Power management errors
#[derive(Clone, Debug)]
pub enum PowerError {
    /// Operation not supported
    NotSupported,
    /// Invalid state
    InvalidState,
    /// Device error
    DeviceError(String),
    /// Timeout
    Timeout,
    /// System busy
    Busy,
}

/// Save CPU state before suspend
fn save_cpu_state() {
    // Would save:
    // - MSRs
    // - Control registers
    // - Interrupt state
    // - CPU context
}

/// Restore CPU state after resume
fn restore_cpu_state() {
    // Would restore saved state
}

/// Save memory to disk for hibernate
fn save_memory_to_disk() -> Result<(), PowerError> {
    // Would:
    // - Create hibernate image
    // - Write to swap partition
    Ok(())
}

/// Enter S3 (suspend to RAM)
unsafe fn enter_s3() {
    // Would write to PM1a_CNT and PM1b_CNT registers
    // SLP_TYP = S3 type, SLP_EN = 1
    core::arch::asm!("cli; hlt");
}

/// Enter S4 (hibernate)
unsafe fn enter_s4() {
    // Similar to S3 but with S4 type
    core::arch::asm!("cli; hlt");
}

/// Enter S5 (soft off)
unsafe fn enter_s5() {
    // Write to PM1a_CNT with S5 type
    // This should power off the system
    loop {
        core::arch::asm!("cli; hlt");
    }
}

/// Trigger system reset
fn trigger_reset() {
    unsafe {
        // Method 1: Keyboard controller reset
        let kbd_port: *mut u8 = 0x64 as *mut u8;
        core::ptr::write_volatile(kbd_port, 0xFE);

        // Method 2: ACPI reset register
        // Would write to ACPI RESET_REG

        // Method 3: Triple fault
        // Load null IDT and trigger interrupt

        // Fallback: loop
        loop {
            core::arch::asm!("cli; hlt");
        }
    }
}

/// Set CPU frequency via MSR
unsafe fn set_cpu_frequency(control: u64) {
    // Would write to IA32_PERF_CTL MSR (0x199)
    const IA32_PERF_CTL: u32 = 0x199;
    crate::cpu::wrmsr(IA32_PERF_CTL, control);
}

/// Enter C-state
unsafe fn enter_c_state(state: CState) {
    match state {
        CState::C0Active => {
            // Do nothing, stay active
        }
        CState::C1Halt | CState::C1E => {
            // HLT instruction
            core::arch::asm!("hlt");
        }
        _ => {
            // MWAIT instruction for deeper states
            // Would need to check MWAIT support and use proper hints
            let hint_val = state as u32;
            core::arch::asm!(
                "xor eax, eax",
                "xor ecx, ecx",
                "monitor",
                "xor eax, eax",
                "mov ecx, {hint:e}",
                "mwait",
                hint = in(reg) hint_val,
                options(nostack)
            );
        }
    }
}

/// Global power manager
static POWER_MANAGER: RwLock<Option<PowerManager>> = RwLock::new(None);

/// Initialize power management
pub fn init() {
    let mut mgr = PowerManager::new();
    mgr.init_from_acpi();
    *POWER_MANAGER.write() = Some(mgr);
}

/// Get power manager
pub fn manager() -> impl core::ops::Deref<Target = Option<PowerManager>> + 'static {
    POWER_MANAGER.read()
}

/// Suspend the system
pub fn suspend() -> Result<(), PowerError> {
    if let Some(ref mgr) = *POWER_MANAGER.read() {
        mgr.suspend()
    } else {
        Err(PowerError::NotSupported)
    }
}

/// Hibernate the system
pub fn hibernate() -> Result<(), PowerError> {
    if let Some(ref mgr) = *POWER_MANAGER.read() {
        mgr.hibernate()
    } else {
        Err(PowerError::NotSupported)
    }
}

/// Shutdown the system
pub fn shutdown() -> Result<(), PowerError> {
    if let Some(ref mgr) = *POWER_MANAGER.read() {
        mgr.shutdown()
    } else {
        Err(PowerError::NotSupported)
    }
}

/// Reboot the system
pub fn reboot() -> Result<(), PowerError> {
    if let Some(ref mgr) = *POWER_MANAGER.read() {
        mgr.reboot()
    } else {
        Err(PowerError::NotSupported)
    }
}

/// Idle the current CPU
pub fn idle() {
    if let Some(ref mgr) = *POWER_MANAGER.read() {
        mgr.idle();
    } else {
        unsafe {
            core::arch::asm!("hlt");
        }
    }
}

/// Get current power source
pub fn power_source() -> PowerSource {
    POWER_MANAGER.read()
        .as_ref()
        .map(|m| m.power_source())
        .unwrap_or(PowerSource::Unknown)
}

/// Get battery info
pub fn battery() -> BatteryInfo {
    POWER_MANAGER.read()
        .as_ref()
        .map(|m| m.battery())
        .unwrap_or_default()
}

/// Get temperature in millidegrees
pub fn temperature(zone: u32) -> Option<i32> {
    POWER_MANAGER.read()
        .as_ref()
        .and_then(|m| m.temperature(zone))
}
