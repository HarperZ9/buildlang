//! CPU Frequency Governors
//!
//! Provides CPU frequency scaling policies:
//! - Performance governor
//! - Powersave governor
//! - Ondemand governor
//! - Conservative governor
//! - Schedutil governor
//! - Userspace governor

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::boxed::Box;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::{Mutex, RwLock};
use super::{PowerError, cpufreq};

/// Governor trait
pub trait Governor: Send + Sync {
    /// Get governor name
    fn name(&self) -> &str;

    /// Start governing
    fn start(&mut self, cpu_id: u32) -> Result<(), PowerError>;

    /// Stop governing
    fn stop(&mut self, cpu_id: u32) -> Result<(), PowerError>;

    /// Called periodically
    fn update(&mut self, cpu_id: u32, load: u32) -> Result<(), PowerError>;

    /// Get target frequency
    fn get_target(&self, cpu_id: u32) -> Option<u32>;

    /// Set limits
    fn set_limits(&mut self, min_khz: u32, max_khz: u32);

    /// Get tunable value
    fn get_tunable(&self, name: &str) -> Option<u32>;

    /// Set tunable value
    fn set_tunable(&mut self, name: &str, value: u32) -> Result<(), PowerError>;
}

/// Governor factory
pub type GovernorFactory = fn() -> Box<dyn Governor>;

/// Performance governor - always max frequency
pub struct PerformanceGovernor {
    target_freq: AtomicU32,
    max_freq: AtomicU32,
}

impl PerformanceGovernor {
    pub fn new() -> Self {
        Self {
            target_freq: AtomicU32::new(0),
            max_freq: AtomicU32::new(0),
        }
    }
}

impl Governor for PerformanceGovernor {
    fn name(&self) -> &str {
        "performance"
    }

    fn start(&mut self, cpu_id: u32) -> Result<(), PowerError> {
        if let Some(info) = cpufreq::get_info(cpu_id) {
            self.max_freq.store(info.turbo_freq, Ordering::Release);
            self.target_freq.store(info.turbo_freq, Ordering::Release);
            cpufreq::set_frequency(cpu_id, info.turbo_freq)?;
        }
        Ok(())
    }

    fn stop(&mut self, _cpu_id: u32) -> Result<(), PowerError> {
        Ok(())
    }

    fn update(&mut self, _cpu_id: u32, _load: u32) -> Result<(), PowerError> {
        // Performance governor doesn't change frequency based on load
        Ok(())
    }

    fn get_target(&self, _cpu_id: u32) -> Option<u32> {
        Some(self.target_freq.load(Ordering::Acquire))
    }

    fn set_limits(&mut self, _min_khz: u32, max_khz: u32) {
        self.max_freq.store(max_khz, Ordering::Release);
        self.target_freq.store(max_khz, Ordering::Release);
    }

    fn get_tunable(&self, _name: &str) -> Option<u32> {
        None
    }

    fn set_tunable(&mut self, _name: &str, _value: u32) -> Result<(), PowerError> {
        Err(PowerError::NotSupported)
    }
}

/// Powersave governor - always min frequency
pub struct PowersaveGovernor {
    target_freq: AtomicU32,
    min_freq: AtomicU32,
}

impl PowersaveGovernor {
    pub fn new() -> Self {
        Self {
            target_freq: AtomicU32::new(0),
            min_freq: AtomicU32::new(0),
        }
    }
}

impl Governor for PowersaveGovernor {
    fn name(&self) -> &str {
        "powersave"
    }

    fn start(&mut self, cpu_id: u32) -> Result<(), PowerError> {
        if let Some(info) = cpufreq::get_info(cpu_id) {
            self.min_freq.store(info.min_freq, Ordering::Release);
            self.target_freq.store(info.min_freq, Ordering::Release);
            cpufreq::set_frequency(cpu_id, info.min_freq)?;
        }
        Ok(())
    }

    fn stop(&mut self, _cpu_id: u32) -> Result<(), PowerError> {
        Ok(())
    }

    fn update(&mut self, _cpu_id: u32, _load: u32) -> Result<(), PowerError> {
        Ok(())
    }

    fn get_target(&self, _cpu_id: u32) -> Option<u32> {
        Some(self.target_freq.load(Ordering::Acquire))
    }

    fn set_limits(&mut self, min_khz: u32, _max_khz: u32) {
        self.min_freq.store(min_khz, Ordering::Release);
        self.target_freq.store(min_khz, Ordering::Release);
    }

    fn get_tunable(&self, _name: &str) -> Option<u32> {
        None
    }

    fn set_tunable(&mut self, _name: &str, _value: u32) -> Result<(), PowerError> {
        Err(PowerError::NotSupported)
    }
}

/// Ondemand governor - scales frequency based on load
pub struct OndemandGovernor {
    /// Current target frequency
    target_freq: AtomicU32,
    /// Min frequency
    min_freq: AtomicU32,
    /// Max frequency
    max_freq: AtomicU32,
    /// Up threshold (%)
    up_threshold: AtomicU32,
    /// Down differential (%)
    down_differential: AtomicU32,
    /// Sampling rate (ms)
    sampling_rate: AtomicU32,
    /// Ignore nice load
    ignore_nice: AtomicBool,
    /// Powersave bias
    powersave_bias: AtomicU32,
}

impl OndemandGovernor {
    pub fn new() -> Self {
        Self {
            target_freq: AtomicU32::new(0),
            min_freq: AtomicU32::new(0),
            max_freq: AtomicU32::new(0),
            up_threshold: AtomicU32::new(80),      // Scale up when load > 80%
            down_differential: AtomicU32::new(10), // Scale down when load < 70%
            sampling_rate: AtomicU32::new(10),     // Sample every 10ms
            ignore_nice: AtomicBool::new(false),
            powersave_bias: AtomicU32::new(0),
        }
    }
}

impl Governor for OndemandGovernor {
    fn name(&self) -> &str {
        "ondemand"
    }

    fn start(&mut self, cpu_id: u32) -> Result<(), PowerError> {
        if let Some(info) = cpufreq::get_info(cpu_id) {
            self.min_freq.store(info.min_freq, Ordering::Release);
            self.max_freq.store(info.turbo_freq, Ordering::Release);
            self.target_freq.store(info.current_freq, Ordering::Release);
        }
        Ok(())
    }

    fn stop(&mut self, _cpu_id: u32) -> Result<(), PowerError> {
        Ok(())
    }

    fn update(&mut self, cpu_id: u32, load: u32) -> Result<(), PowerError> {
        let up_threshold = self.up_threshold.load(Ordering::Acquire);
        let down_diff = self.down_differential.load(Ordering::Acquire);
        let min_freq = self.min_freq.load(Ordering::Acquire);
        let max_freq = self.max_freq.load(Ordering::Acquire);
        let current = self.target_freq.load(Ordering::Acquire);

        let new_freq = if load >= up_threshold {
            // Jump to max
            max_freq
        } else if load < up_threshold - down_diff {
            // Calculate target proportionally
            let range = max_freq - min_freq;
            let target = min_freq + (range * load) / 100;

            // Apply powersave bias
            let bias = self.powersave_bias.load(Ordering::Acquire);
            if bias > 0 {
                let bias_freq = target - (target * bias) / 1000;
                core::cmp::max(bias_freq, min_freq)
            } else {
                target
            }
        } else {
            // Keep current
            current
        };

        if new_freq != current {
            self.target_freq.store(new_freq, Ordering::Release);
            cpufreq::set_frequency(cpu_id, new_freq)?;
        }

        Ok(())
    }

    fn get_target(&self, _cpu_id: u32) -> Option<u32> {
        Some(self.target_freq.load(Ordering::Acquire))
    }

    fn set_limits(&mut self, min_khz: u32, max_khz: u32) {
        self.min_freq.store(min_khz, Ordering::Release);
        self.max_freq.store(max_khz, Ordering::Release);
    }

    fn get_tunable(&self, name: &str) -> Option<u32> {
        match name {
            "up_threshold" => Some(self.up_threshold.load(Ordering::Acquire)),
            "down_differential" => Some(self.down_differential.load(Ordering::Acquire)),
            "sampling_rate" => Some(self.sampling_rate.load(Ordering::Acquire)),
            "powersave_bias" => Some(self.powersave_bias.load(Ordering::Acquire)),
            _ => None,
        }
    }

    fn set_tunable(&mut self, name: &str, value: u32) -> Result<(), PowerError> {
        match name {
            "up_threshold" => {
                if value <= 100 && value > self.down_differential.load(Ordering::Acquire) {
                    self.up_threshold.store(value, Ordering::Release);
                    Ok(())
                } else {
                    Err(PowerError::InvalidState)
                }
            }
            "down_differential" => {
                if value < self.up_threshold.load(Ordering::Acquire) {
                    self.down_differential.store(value, Ordering::Release);
                    Ok(())
                } else {
                    Err(PowerError::InvalidState)
                }
            }
            "sampling_rate" => {
                self.sampling_rate.store(value, Ordering::Release);
                Ok(())
            }
            "powersave_bias" => {
                if value <= 1000 {
                    self.powersave_bias.store(value, Ordering::Release);
                    Ok(())
                } else {
                    Err(PowerError::InvalidState)
                }
            }
            _ => Err(PowerError::NotSupported),
        }
    }
}

/// Conservative governor - gradual frequency changes
pub struct ConservativeGovernor {
    target_freq: AtomicU32,
    min_freq: AtomicU32,
    max_freq: AtomicU32,
    up_threshold: AtomicU32,
    down_threshold: AtomicU32,
    freq_step: AtomicU32,
    sampling_rate: AtomicU32,
}

impl ConservativeGovernor {
    pub fn new() -> Self {
        Self {
            target_freq: AtomicU32::new(0),
            min_freq: AtomicU32::new(0),
            max_freq: AtomicU32::new(0),
            up_threshold: AtomicU32::new(80),
            down_threshold: AtomicU32::new(20),
            freq_step: AtomicU32::new(5),  // 5% steps
            sampling_rate: AtomicU32::new(20),
        }
    }
}

impl Governor for ConservativeGovernor {
    fn name(&self) -> &str {
        "conservative"
    }

    fn start(&mut self, cpu_id: u32) -> Result<(), PowerError> {
        if let Some(info) = cpufreq::get_info(cpu_id) {
            self.min_freq.store(info.min_freq, Ordering::Release);
            self.max_freq.store(info.turbo_freq, Ordering::Release);
            self.target_freq.store(info.current_freq, Ordering::Release);
        }
        Ok(())
    }

    fn stop(&mut self, _cpu_id: u32) -> Result<(), PowerError> {
        Ok(())
    }

    fn update(&mut self, cpu_id: u32, load: u32) -> Result<(), PowerError> {
        let up_threshold = self.up_threshold.load(Ordering::Acquire);
        let down_threshold = self.down_threshold.load(Ordering::Acquire);
        let freq_step = self.freq_step.load(Ordering::Acquire);
        let min_freq = self.min_freq.load(Ordering::Acquire);
        let max_freq = self.max_freq.load(Ordering::Acquire);
        let current = self.target_freq.load(Ordering::Acquire);

        let range = max_freq - min_freq;
        let step = (range * freq_step) / 100;

        let new_freq = if load >= up_threshold {
            // Step up
            core::cmp::min(current + step, max_freq)
        } else if load < down_threshold {
            // Step down
            if current > min_freq + step {
                current - step
            } else {
                min_freq
            }
        } else {
            // Keep current
            current
        };

        if new_freq != current {
            self.target_freq.store(new_freq, Ordering::Release);
            cpufreq::set_frequency(cpu_id, new_freq)?;
        }

        Ok(())
    }

    fn get_target(&self, _cpu_id: u32) -> Option<u32> {
        Some(self.target_freq.load(Ordering::Acquire))
    }

    fn set_limits(&mut self, min_khz: u32, max_khz: u32) {
        self.min_freq.store(min_khz, Ordering::Release);
        self.max_freq.store(max_khz, Ordering::Release);
    }

    fn get_tunable(&self, name: &str) -> Option<u32> {
        match name {
            "up_threshold" => Some(self.up_threshold.load(Ordering::Acquire)),
            "down_threshold" => Some(self.down_threshold.load(Ordering::Acquire)),
            "freq_step" => Some(self.freq_step.load(Ordering::Acquire)),
            "sampling_rate" => Some(self.sampling_rate.load(Ordering::Acquire)),
            _ => None,
        }
    }

    fn set_tunable(&mut self, name: &str, value: u32) -> Result<(), PowerError> {
        match name {
            "up_threshold" => {
                self.up_threshold.store(value, Ordering::Release);
                Ok(())
            }
            "down_threshold" => {
                self.down_threshold.store(value, Ordering::Release);
                Ok(())
            }
            "freq_step" => {
                if value <= 100 {
                    self.freq_step.store(value, Ordering::Release);
                    Ok(())
                } else {
                    Err(PowerError::InvalidState)
                }
            }
            "sampling_rate" => {
                self.sampling_rate.store(value, Ordering::Release);
                Ok(())
            }
            _ => Err(PowerError::NotSupported),
        }
    }
}

/// Schedutil governor - scheduler-driven scaling
pub struct SchedutilGovernor {
    target_freq: AtomicU32,
    min_freq: AtomicU32,
    max_freq: AtomicU32,
    rate_limit_us: AtomicU32,
    last_update: AtomicU64,
}

impl SchedutilGovernor {
    pub fn new() -> Self {
        Self {
            target_freq: AtomicU32::new(0),
            min_freq: AtomicU32::new(0),
            max_freq: AtomicU32::new(0),
            rate_limit_us: AtomicU32::new(1000), // 1ms rate limit
            last_update: AtomicU64::new(0),
        }
    }
}

impl Governor for SchedutilGovernor {
    fn name(&self) -> &str {
        "schedutil"
    }

    fn start(&mut self, cpu_id: u32) -> Result<(), PowerError> {
        if let Some(info) = cpufreq::get_info(cpu_id) {
            self.min_freq.store(info.min_freq, Ordering::Release);
            self.max_freq.store(info.turbo_freq, Ordering::Release);
            self.target_freq.store(info.current_freq, Ordering::Release);
        }
        Ok(())
    }

    fn stop(&mut self, _cpu_id: u32) -> Result<(), PowerError> {
        Ok(())
    }

    fn update(&mut self, cpu_id: u32, util: u32) -> Result<(), PowerError> {
        // Schedutil uses utilization directly from scheduler
        // util is in range 0-1024 (fixed point)

        let min_freq = self.min_freq.load(Ordering::Acquire);
        let max_freq = self.max_freq.load(Ordering::Acquire);

        // Linear scaling with 1.25 margin for turbo
        let freq = ((util as u64 * max_freq as u64 * 5) / (1024 * 4)) as u32;
        let new_freq = core::cmp::max(min_freq, core::cmp::min(freq, max_freq));

        let current = self.target_freq.load(Ordering::Acquire);
        if new_freq != current {
            self.target_freq.store(new_freq, Ordering::Release);
            cpufreq::set_frequency(cpu_id, new_freq)?;
        }

        Ok(())
    }

    fn get_target(&self, _cpu_id: u32) -> Option<u32> {
        Some(self.target_freq.load(Ordering::Acquire))
    }

    fn set_limits(&mut self, min_khz: u32, max_khz: u32) {
        self.min_freq.store(min_khz, Ordering::Release);
        self.max_freq.store(max_khz, Ordering::Release);
    }

    fn get_tunable(&self, name: &str) -> Option<u32> {
        match name {
            "rate_limit_us" => Some(self.rate_limit_us.load(Ordering::Acquire)),
            _ => None,
        }
    }

    fn set_tunable(&mut self, name: &str, value: u32) -> Result<(), PowerError> {
        match name {
            "rate_limit_us" => {
                self.rate_limit_us.store(value, Ordering::Release);
                Ok(())
            }
            _ => Err(PowerError::NotSupported),
        }
    }
}

/// Userspace governor - frequency set by userspace
pub struct UserspaceGovernor {
    target_freq: AtomicU32,
    min_freq: AtomicU32,
    max_freq: AtomicU32,
}

impl UserspaceGovernor {
    pub fn new() -> Self {
        Self {
            target_freq: AtomicU32::new(0),
            min_freq: AtomicU32::new(0),
            max_freq: AtomicU32::new(0),
        }
    }

    pub fn set_speed(&mut self, cpu_id: u32, freq_khz: u32) -> Result<(), PowerError> {
        let min = self.min_freq.load(Ordering::Acquire);
        let max = self.max_freq.load(Ordering::Acquire);

        if freq_khz < min || freq_khz > max {
            return Err(PowerError::InvalidState);
        }

        self.target_freq.store(freq_khz, Ordering::Release);
        cpufreq::set_frequency(cpu_id, freq_khz)
    }
}

impl Governor for UserspaceGovernor {
    fn name(&self) -> &str {
        "userspace"
    }

    fn start(&mut self, cpu_id: u32) -> Result<(), PowerError> {
        if let Some(info) = cpufreq::get_info(cpu_id) {
            self.min_freq.store(info.min_freq, Ordering::Release);
            self.max_freq.store(info.turbo_freq, Ordering::Release);
            self.target_freq.store(info.current_freq, Ordering::Release);
        }
        Ok(())
    }

    fn stop(&mut self, _cpu_id: u32) -> Result<(), PowerError> {
        Ok(())
    }

    fn update(&mut self, _cpu_id: u32, _load: u32) -> Result<(), PowerError> {
        // Userspace governor doesn't auto-update
        Ok(())
    }

    fn get_target(&self, _cpu_id: u32) -> Option<u32> {
        Some(self.target_freq.load(Ordering::Acquire))
    }

    fn set_limits(&mut self, min_khz: u32, max_khz: u32) {
        self.min_freq.store(min_khz, Ordering::Release);
        self.max_freq.store(max_khz, Ordering::Release);
    }

    fn get_tunable(&self, _name: &str) -> Option<u32> {
        None
    }

    fn set_tunable(&mut self, _name: &str, _value: u32) -> Result<(), PowerError> {
        Err(PowerError::NotSupported)
    }
}

/// Governor registry
pub struct GovernorRegistry {
    governors: RwLock<BTreeMap<String, GovernorFactory>>,
    active: RwLock<BTreeMap<u32, Box<dyn Governor>>>,
    default_governor: Mutex<String>,
}

impl GovernorRegistry {
    pub const fn new() -> Self {
        Self {
            governors: RwLock::new(BTreeMap::new()),
            active: RwLock::new(BTreeMap::new()),
            default_governor: Mutex::new(String::new()),
        }
    }

    /// Register a governor
    pub fn register(&self, name: &str, factory: GovernorFactory) {
        self.governors.write().insert(String::from(name), factory);
    }

    /// Get available governors
    pub fn available(&self) -> Vec<String> {
        self.governors.read().keys().cloned().collect()
    }

    /// Create governor instance
    pub fn create(&self, name: &str) -> Option<Box<dyn Governor>> {
        self.governors.read().get(name).map(|f| f())
    }

    /// Set active governor for CPU
    pub fn set_governor(&self, cpu_id: u32, name: &str) -> Result<(), PowerError> {
        let governor = self.create(name).ok_or(PowerError::NotSupported)?;

        // Stop old governor
        if let Some(mut old) = self.active.write().remove(&cpu_id) {
            let _ = old.stop(cpu_id);
        }

        // Start new governor
        let mut gov = governor;
        gov.start(cpu_id)?;
        self.active.write().insert(cpu_id, gov);

        Ok(())
    }

    /// Get current governor for CPU
    pub fn get_governor(&self, cpu_id: u32) -> Option<String> {
        self.active.read().get(&cpu_id).map(|g| String::from(g.name()))
    }

    /// Update governor with load
    pub fn update(&self, cpu_id: u32, load: u32) -> Result<(), PowerError> {
        if let Some(gov) = self.active.write().get_mut(&cpu_id) {
            gov.update(cpu_id, load)
        } else {
            Ok(())
        }
    }
}

/// Global governor registry
static REGISTRY: GovernorRegistry = GovernorRegistry::new();

/// Initialize governor subsystem
pub fn init() {
    // Register built-in governors
    REGISTRY.register("performance", || Box::new(PerformanceGovernor::new()));
    REGISTRY.register("powersave", || Box::new(PowersaveGovernor::new()));
    REGISTRY.register("ondemand", || Box::new(OndemandGovernor::new()));
    REGISTRY.register("conservative", || Box::new(ConservativeGovernor::new()));
    REGISTRY.register("schedutil", || Box::new(SchedutilGovernor::new()));
    REGISTRY.register("userspace", || Box::new(UserspaceGovernor::new()));

    // Set default governor for all CPUs
    let cpu_count = crate::drivers::acpi::cpu_count();
    for cpu_id in 0..cpu_count {
        let _ = REGISTRY.set_governor(cpu_id as u32, "ondemand");
    }

    crate::kprintln!("[GOVERNOR] {} governors registered", REGISTRY.available().len());
}

/// Get available governors
pub fn available() -> Vec<String> {
    REGISTRY.available()
}

/// Set governor for CPU
pub fn set_governor(cpu_id: u32, name: &str) -> Result<(), PowerError> {
    REGISTRY.set_governor(cpu_id, name)
}

/// Get current governor for CPU
pub fn get_governor(cpu_id: u32) -> Option<String> {
    REGISTRY.get_governor(cpu_id)
}

/// Update governor with load
pub fn update(cpu_id: u32, load: u32) -> Result<(), PowerError> {
    REGISTRY.update(cpu_id, load)
}
