//! CPU Frequency Scaling (cpufreq)
//!
//! Provides dynamic CPU frequency scaling for power management:
//! - P-state management
//! - Hardware P-states (HWP/Intel Speed Shift)
//! - ACPI P-states
//! - Frequency governors

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use spin::{Mutex, RwLock};
use super::PowerError;

/// MSR addresses
mod msr {
    pub const IA32_PERF_STATUS: u32 = 0x198;
    pub const IA32_PERF_CTL: u32 = 0x199;
    pub const IA32_MISC_ENABLE: u32 = 0x1A0;
    pub const MSR_PLATFORM_INFO: u32 = 0xCE;
    pub const MSR_TURBO_RATIO_LIMIT: u32 = 0x1AD;
    pub const IA32_PM_ENABLE: u32 = 0x770;
    pub const IA32_HWP_CAPABILITIES: u32 = 0x771;
    pub const IA32_HWP_REQUEST_PKG: u32 = 0x772;
    pub const IA32_HWP_INTERRUPT: u32 = 0x773;
    pub const IA32_HWP_REQUEST: u32 = 0x774;
    pub const IA32_HWP_STATUS: u32 = 0x777;
}

/// Driver type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CpufreqDriver {
    /// Intel P-state driver
    IntelPstate,
    /// ACPI cpufreq driver
    AcpiCpufreq,
    /// Hardware P-states (Intel Speed Shift)
    IntelHwp,
    /// AMD P-state driver
    AmdPstate,
    /// Generic driver
    Generic,
}

/// Frequency policy
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CpufreqPolicy {
    /// Performance (run at max frequency)
    Performance,
    /// Powersave (run at min frequency)
    Powersave,
    /// Ondemand (dynamic based on load)
    Ondemand,
    /// Conservative (gradual changes)
    Conservative,
    /// Schedutil (scheduler-driven)
    Schedutil,
    /// Userspace (user controlled)
    Userspace,
}

impl CpufreqPolicy {
    /// Get policy name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Performance => "performance",
            Self::Powersave => "powersave",
            Self::Ondemand => "ondemand",
            Self::Conservative => "conservative",
            Self::Schedutil => "schedutil",
            Self::Userspace => "userspace",
        }
    }

    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "performance" => Some(Self::Performance),
            "powersave" => Some(Self::Powersave),
            "ondemand" => Some(Self::Ondemand),
            "conservative" => Some(Self::Conservative),
            "schedutil" => Some(Self::Schedutil),
            "userspace" => Some(Self::Userspace),
            _ => None,
        }
    }
}

/// CPU frequency information
#[derive(Clone, Debug)]
pub struct CpufreqInfo {
    /// CPU ID
    pub cpu_id: u32,
    /// Current frequency (kHz)
    pub current_freq: u32,
    /// Minimum frequency (kHz)
    pub min_freq: u32,
    /// Maximum frequency (kHz)
    pub max_freq: u32,
    /// Base frequency (kHz)
    pub base_freq: u32,
    /// Turbo frequency (kHz)
    pub turbo_freq: u32,
    /// Available frequencies
    pub available_freqs: Vec<u32>,
    /// Current policy
    pub policy: CpufreqPolicy,
    /// Policy min frequency
    pub policy_min: u32,
    /// Policy max frequency
    pub policy_max: u32,
    /// Driver
    pub driver: CpufreqDriver,
    /// HWP enabled
    pub hwp_enabled: bool,
    /// Energy performance preference
    pub epp: u8,
}

/// cpufreq subsystem
pub struct CpufreqSubsystem {
    /// Is initialized
    initialized: AtomicBool,
    /// Active driver
    driver: Mutex<CpufreqDriver>,
    /// Per-CPU info
    cpu_info: RwLock<BTreeMap<u32, CpufreqInfo>>,
    /// HWP available
    hwp_available: AtomicBool,
    /// Global policy
    global_policy: Mutex<CpufreqPolicy>,
    /// Statistics
    stats: CpufreqStats,
}

/// Statistics
#[derive(Debug, Default)]
struct CpufreqStats {
    /// Frequency transitions
    transitions: AtomicU64,
    /// Time in each P-state
    time_in_state: Mutex<BTreeMap<u32, u64>>,
}

impl CpufreqSubsystem {
    /// Create new subsystem
    pub const fn new() -> Self {
        Self {
            initialized: AtomicBool::new(false),
            driver: Mutex::new(CpufreqDriver::Generic),
            cpu_info: RwLock::new(BTreeMap::new()),
            hwp_available: AtomicBool::new(false),
            global_policy: Mutex::new(CpufreqPolicy::Ondemand),
            stats: CpufreqStats {
                transitions: AtomicU64::new(0),
                time_in_state: Mutex::new(BTreeMap::new()),
            },
        }
    }

    /// Initialize cpufreq subsystem
    pub fn init(&self) -> Result<(), PowerError> {
        // Detect available drivers
        let driver = self.detect_driver();
        *self.driver.lock() = driver;

        // Check for HWP support
        let hwp = self.detect_hwp();
        self.hwp_available.store(hwp, Ordering::Release);

        // Initialize per-CPU info
        let cpu_count = crate::drivers::acpi::cpu_count();
        for cpu_id in 0..cpu_count {
            let info = self.probe_cpu(cpu_id as u32)?;
            self.cpu_info.write().insert(cpu_id as u32, info);
        }

        // Enable HWP if available
        if hwp {
            self.enable_hwp()?;
        }

        self.initialized.store(true, Ordering::Release);

        crate::kprintln!("[CPUFREQ] Driver: {:?}, HWP: {}", driver, hwp);

        Ok(())
    }

    /// Detect best driver
    fn detect_driver(&self) -> CpufreqDriver {
        // Check CPU vendor
        let (vendor, _, _) = cpuid_vendor();

        if vendor.starts_with("GenuineIntel") {
            // Check for HWP
            if self.detect_hwp() {
                return CpufreqDriver::IntelHwp;
            }
            return CpufreqDriver::IntelPstate;
        } else if vendor.starts_with("AuthenticAMD") {
            return CpufreqDriver::AmdPstate;
        }

        CpufreqDriver::AcpiCpufreq
    }

    /// Detect HWP support
    fn detect_hwp(&self) -> bool {
        // Check CPUID leaf 6
        let result = cpuid(6);
        (result.eax & (1 << 7)) != 0
    }

    /// Enable Hardware P-states
    fn enable_hwp(&self) -> Result<(), PowerError> {
        // Set IA32_PM_ENABLE bit 0
        let value = rdmsr(msr::IA32_PM_ENABLE);
        wrmsr(msr::IA32_PM_ENABLE, value | 1);

        Ok(())
    }

    /// Probe CPU for frequency info
    fn probe_cpu(&self, cpu_id: u32) -> Result<CpufreqInfo, PowerError> {
        // Read platform info
        let platform_info = rdmsr(msr::MSR_PLATFORM_INFO);

        let base_ratio = ((platform_info >> 8) & 0xFF) as u32;
        let max_ratio = ((platform_info >> 40) & 0xFF) as u32;
        let min_ratio = ((platform_info >> 48) & 0xFF) as u32;

        // Assume 100 MHz bus
        let bus_freq = 100_000; // kHz

        let base_freq = base_ratio * bus_freq;
        let max_freq = if max_ratio > 0 { max_ratio * bus_freq } else { base_freq };
        let min_freq = if min_ratio > 0 { min_ratio * bus_freq } else { base_freq / 2 };

        // Read turbo ratio
        let turbo_info = rdmsr(msr::MSR_TURBO_RATIO_LIMIT);
        let turbo_ratio = (turbo_info & 0xFF) as u32;
        let turbo_freq = turbo_ratio * bus_freq;

        // Read current frequency
        let perf_status = rdmsr(msr::IA32_PERF_STATUS);
        let current_ratio = ((perf_status >> 8) & 0xFF) as u32;
        let current_freq = current_ratio * bus_freq;

        // Build available frequencies list
        let mut available_freqs = Vec::new();
        for ratio in (min_ratio..=turbo_ratio).step_by(1) {
            available_freqs.push(ratio * bus_freq);
        }

        // Check HWP status
        let hwp_enabled = self.hwp_available.load(Ordering::Acquire);
        let epp = if hwp_enabled {
            let hwp_req = rdmsr(msr::IA32_HWP_REQUEST);
            ((hwp_req >> 24) & 0xFF) as u8
        } else {
            128 // Default balanced
        };

        Ok(CpufreqInfo {
            cpu_id,
            current_freq,
            min_freq,
            max_freq,
            base_freq,
            turbo_freq,
            available_freqs,
            policy: CpufreqPolicy::Ondemand,
            policy_min: min_freq,
            policy_max: turbo_freq,
            driver: *self.driver.lock(),
            hwp_enabled,
            epp,
        })
    }

    /// Get CPU frequency info
    pub fn get_info(&self, cpu_id: u32) -> Option<CpufreqInfo> {
        self.cpu_info.read().get(&cpu_id).cloned()
    }

    /// Set CPU frequency
    pub fn set_frequency(&self, cpu_id: u32, freq_khz: u32) -> Result<(), PowerError> {
        let info = self.cpu_info.read().get(&cpu_id).cloned()
            .ok_or(PowerError::DeviceNotFound)?;

        // Validate frequency
        if freq_khz < info.policy_min || freq_khz > info.policy_max {
            return Err(PowerError::InvalidState);
        }

        // Calculate ratio
        let bus_freq = 100_000u32;
        let ratio = freq_khz / bus_freq;

        // Set P-state
        if info.hwp_enabled {
            // Use HWP
            let hwp_req = rdmsr(msr::IA32_HWP_REQUEST);
            let new_req = (hwp_req & !0xFFFF) | (ratio as u64) | ((ratio as u64) << 8);
            wrmsr(msr::IA32_HWP_REQUEST, new_req);
        } else {
            // Use legacy P-state control
            let value = (ratio as u64) << 8;
            wrmsr(msr::IA32_PERF_CTL, value);
        }

        // Update info
        if let Some(info) = self.cpu_info.write().get_mut(&cpu_id) {
            info.current_freq = freq_khz;
        }

        self.stats.transitions.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    /// Set CPU policy
    pub fn set_policy(&self, cpu_id: u32, policy: CpufreqPolicy) -> Result<(), PowerError> {
        let mut cpu_info = self.cpu_info.write();
        let info = cpu_info.get_mut(&cpu_id)
            .ok_or(PowerError::DeviceNotFound)?;

        info.policy = policy;

        // Apply policy
        match policy {
            CpufreqPolicy::Performance => {
                info.policy_min = info.max_freq;
                info.policy_max = info.turbo_freq;
            }
            CpufreqPolicy::Powersave => {
                info.policy_min = info.min_freq;
                info.policy_max = info.min_freq;
            }
            CpufreqPolicy::Ondemand | CpufreqPolicy::Conservative | CpufreqPolicy::Schedutil => {
                info.policy_min = info.min_freq;
                info.policy_max = info.turbo_freq;
            }
            CpufreqPolicy::Userspace => {
                // User controls
            }
        }

        // Set EPP if HWP is enabled
        if info.hwp_enabled {
            let epp = match policy {
                CpufreqPolicy::Performance => 0,    // Highest performance
                CpufreqPolicy::Powersave => 255,     // Maximum power savings
                _ => 128,                            // Balanced
            };
            self.set_epp(cpu_id, epp)?;
        }

        Ok(())
    }

    /// Set Energy Performance Preference
    pub fn set_epp(&self, cpu_id: u32, epp: u8) -> Result<(), PowerError> {
        if !self.hwp_available.load(Ordering::Acquire) {
            return Err(PowerError::NotSupported);
        }

        let hwp_req = rdmsr(msr::IA32_HWP_REQUEST);
        let new_req = (hwp_req & !0xFF000000) | ((epp as u64) << 24);
        wrmsr(msr::IA32_HWP_REQUEST, new_req);

        if let Some(info) = self.cpu_info.write().get_mut(&cpu_id) {
            info.epp = epp;
        }

        Ok(())
    }

    /// Set policy limits
    pub fn set_limits(&self, cpu_id: u32, min_khz: u32, max_khz: u32) -> Result<(), PowerError> {
        let mut cpu_info = self.cpu_info.write();
        let info = cpu_info.get_mut(&cpu_id)
            .ok_or(PowerError::DeviceNotFound)?;

        // Validate
        if min_khz > max_khz || min_khz < info.min_freq || max_khz > info.turbo_freq {
            return Err(PowerError::InvalidState);
        }

        info.policy_min = min_khz;
        info.policy_max = max_khz;

        // Apply to HWP if enabled
        if info.hwp_enabled {
            let bus_freq = 100_000u32;
            let min_ratio = min_khz / bus_freq;
            let max_ratio = max_khz / bus_freq;

            let hwp_req = rdmsr(msr::IA32_HWP_REQUEST);
            let new_req = (hwp_req & !0xFFFF) | (min_ratio as u64) | ((max_ratio as u64) << 8);
            wrmsr(msr::IA32_HWP_REQUEST, new_req);
        }

        Ok(())
    }

    /// Get current frequency
    pub fn get_current_freq(&self, _cpu_id: u32) -> Result<u32, PowerError> {
        // Read actual frequency from hardware
        let perf_status = rdmsr(msr::IA32_PERF_STATUS);
        let ratio = ((perf_status >> 8) & 0xFF) as u32;
        let bus_freq = 100_000u32;

        Ok(ratio * bus_freq)
    }

    /// Get all available policies
    pub fn get_available_policies(&self) -> Vec<CpufreqPolicy> {
        vec![
            CpufreqPolicy::Performance,
            CpufreqPolicy::Powersave,
            CpufreqPolicy::Ondemand,
            CpufreqPolicy::Conservative,
            CpufreqPolicy::Schedutil,
            CpufreqPolicy::Userspace,
        ]
    }

    /// Boost enable/disable
    pub fn set_boost(&self, enabled: bool) -> Result<(), PowerError> {
        // Toggle turbo/boost
        let misc_enable = rdmsr(msr::IA32_MISC_ENABLE);

        let new_value = if enabled {
            misc_enable & !(1 << 38) // Clear turbo disable bit
        } else {
            misc_enable | (1 << 38)  // Set turbo disable bit
        };

        wrmsr(msr::IA32_MISC_ENABLE, new_value);

        Ok(())
    }

    /// Get boost status
    pub fn get_boost(&self) -> bool {
        let misc_enable = rdmsr(msr::IA32_MISC_ENABLE);
        (misc_enable & (1 << 38)) == 0
    }
}

/// Global cpufreq subsystem
static CPUFREQ: CpufreqSubsystem = CpufreqSubsystem::new();

/// Initialize cpufreq
pub fn init() {
    if let Err(e) = CPUFREQ.init() {
        crate::kprintln!("[CPUFREQ] Initialization failed: {:?}", e);
    }
}

/// Set CPU frequency
pub fn set_frequency(cpu_id: u32, freq_khz: u32) -> Result<(), PowerError> {
    CPUFREQ.set_frequency(cpu_id, freq_khz)
}

/// Set CPU policy
pub fn set_policy(cpu_id: u32, policy: CpufreqPolicy) -> Result<(), PowerError> {
    CPUFREQ.set_policy(cpu_id, policy)
}

/// Get CPU info
pub fn get_info(cpu_id: u32) -> Option<CpufreqInfo> {
    CPUFREQ.get_info(cpu_id)
}

/// Set boost
pub fn set_boost(enabled: bool) -> Result<(), PowerError> {
    CPUFREQ.set_boost(enabled)
}

// =============================================================================
// CPU identification helpers
// =============================================================================

fn cpuid_vendor() -> (String, u32, u32) {
    let result = cpuid(0);

    let mut vendor = [0u8; 12];
    vendor[0..4].copy_from_slice(&result.ebx.to_le_bytes());
    vendor[4..8].copy_from_slice(&result.edx.to_le_bytes());
    vendor[8..12].copy_from_slice(&result.ecx.to_le_bytes());

    (
        String::from_utf8_lossy(&vendor).into_owned(),
        result.eax, // Max standard leaf
        0,
    )
}

#[derive(Clone, Copy)]
struct CpuidResult {
    eax: u32,
    ebx: u32,
    ecx: u32,
    edx: u32,
}

fn cpuid(leaf: u32) -> CpuidResult {
    let (eax, ebx, ecx, edx): (u32, u32, u32, u32);
    unsafe {
        core::arch::asm!(
            "push rbx",
            "cpuid",
            "mov {ebx_out:e}, ebx",
            "pop rbx",
            inout("eax") leaf => eax,
            ebx_out = out(reg) ebx,
            inout("ecx") 0u32 => ecx,
            out("edx") edx,
        );
    }
    CpuidResult { eax, ebx, ecx, edx }
}

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

fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") low,
            in("edx") high,
        );
    }
}
