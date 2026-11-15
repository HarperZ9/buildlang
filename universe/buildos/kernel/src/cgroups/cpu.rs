// ===============================================================================
// BUILDOS KERNEL - CGROUPS CPU CONTROLLER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! CPU Controller for cgroups v2
//!
//! Provides CPU resource control:
//! - CPU bandwidth control (cpu.max)
//! - CPU weight for fair scheduling
//! - CPU usage accounting
//! - Burst capacity

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, AtomicI64, Ordering};

use super::CgroupError;

/// Default period (100ms in microseconds)
pub const DEFAULT_PERIOD: u64 = 100_000;

/// Max CPU weight
pub const CPU_WEIGHT_MAX: u64 = 10000;

/// Default CPU weight
pub const CPU_WEIGHT_DEFAULT: u64 = 100;

/// Initialize CPU controller
pub fn init() {
    crate::kprintln!("[CGROUPS] CPU controller initialized");
}

/// CPU controller state
pub struct CpuController {
    /// CPU weight (1-10000)
    pub weight: AtomicU64,
    /// Nice weight (for compatibility)
    pub weight_nice: AtomicI64,
    /// Max bandwidth quota (microseconds per period)
    pub max_quota: AtomicU64,
    /// Period (microseconds)
    pub max_period: AtomicU64,
    /// Burst capacity (microseconds)
    pub burst: AtomicU64,
    /// Total CPU time used (microseconds)
    pub usage_usec: AtomicU64,
    /// User CPU time
    pub user_usec: AtomicU64,
    /// System CPU time
    pub system_usec: AtomicU64,
    /// Number of periods
    pub nr_periods: AtomicU64,
    /// Number of throttled periods
    pub nr_throttled: AtomicU64,
    /// Total throttled time
    pub throttled_usec: AtomicU64,
    /// Number of bursts
    pub nr_bursts: AtomicU64,
    /// Total burst time
    pub burst_usec: AtomicU64,
    /// Statistics
    pub stats: CpuStats,
}

impl CpuController {
    /// Create new CPU controller
    pub fn new() -> Self {
        Self {
            weight: AtomicU64::new(CPU_WEIGHT_DEFAULT),
            weight_nice: AtomicI64::new(0),
            max_quota: AtomicU64::new(u64::MAX), // No limit
            max_period: AtomicU64::new(DEFAULT_PERIOD),
            burst: AtomicU64::new(0),
            usage_usec: AtomicU64::new(0),
            user_usec: AtomicU64::new(0),
            system_usec: AtomicU64::new(0),
            nr_periods: AtomicU64::new(0),
            nr_throttled: AtomicU64::new(0),
            throttled_usec: AtomicU64::new(0),
            nr_bursts: AtomicU64::new(0),
            burst_usec: AtomicU64::new(0),
            stats: CpuStats::new(),
        }
    }

    /// Account CPU time
    pub fn account_time(&self, usec: u64, is_user: bool) {
        self.usage_usec.fetch_add(usec, Ordering::Relaxed);
        if is_user {
            self.user_usec.fetch_add(usec, Ordering::Relaxed);
        } else {
            self.system_usec.fetch_add(usec, Ordering::Relaxed);
        }
    }

    /// Check if bandwidth limit allows running
    pub fn can_run(&self) -> bool {
        let quota = self.max_quota.load(Ordering::Relaxed);
        if quota == u64::MAX {
            return true; // No limit
        }

        let usage = self.usage_usec.load(Ordering::Relaxed);
        let _period = self.max_period.load(Ordering::Relaxed);
        let periods = self.nr_periods.load(Ordering::Relaxed);

        // Check if within quota for current period
        let period_usage = usage - (periods * quota);
        period_usage < quota
    }

    /// Start a new period
    pub fn new_period(&self) {
        self.nr_periods.fetch_add(1, Ordering::Relaxed);
    }

    /// Record throttling
    pub fn throttle(&self, usec: u64) {
        self.nr_throttled.fetch_add(1, Ordering::Relaxed);
        self.throttled_usec.fetch_add(usec, Ordering::Relaxed);
    }

    /// Calculate effective weight relative to other cgroups
    pub fn effective_weight(&self, total_weight: u64) -> f64 {
        let weight = self.weight.load(Ordering::Relaxed);
        if total_weight == 0 {
            1.0
        } else {
            weight as f64 / total_weight as f64
        }
    }

    /// Get CPU utilization (0.0 - 1.0)
    pub fn utilization(&self) -> f64 {
        let quota = self.max_quota.load(Ordering::Relaxed);
        if quota == u64::MAX {
            return 0.0;
        }

        let usage = self.usage_usec.load(Ordering::Relaxed);
        let periods = self.nr_periods.load(Ordering::Relaxed);
        let period = self.max_period.load(Ordering::Relaxed);

        if periods == 0 || period == 0 {
            0.0
        } else {
            (usage as f64) / (periods as f64 * quota as f64)
        }
    }
}

impl Default for CpuController {
    fn default() -> Self {
        Self::new()
    }
}

/// CPU statistics
pub struct CpuStats {
    /// Number of context switches
    pub nr_switches: AtomicU64,
    /// Voluntary context switches
    pub nr_voluntary_switches: AtomicU64,
    /// Involuntary context switches
    pub nr_involuntary_switches: AtomicU64,
    /// Wait time
    pub wait_sum: AtomicU64,
    /// Time waiting for IO
    pub iowait_sum: AtomicU64,
}

impl CpuStats {
    pub fn new() -> Self {
        Self {
            nr_switches: AtomicU64::new(0),
            nr_voluntary_switches: AtomicU64::new(0),
            nr_involuntary_switches: AtomicU64::new(0),
            wait_sum: AtomicU64::new(0),
            iowait_sum: AtomicU64::new(0),
        }
    }

    pub fn format(&self) -> String {
        alloc::format!(
            "nr_switches {}\nnr_voluntary_switches {}\nnr_involuntary_switches {}",
            self.nr_switches.load(Ordering::Relaxed),
            self.nr_voluntary_switches.load(Ordering::Relaxed),
            self.nr_involuntary_switches.load(Ordering::Relaxed),
        )
    }
}

impl Default for CpuStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Read a CPU controller file
pub fn read_file(controller: &CpuController, file: &str) -> Result<String, CgroupError> {
    match file {
        "weight" => Ok(controller.weight.load(Ordering::Relaxed).to_string()),
        "weight.nice" => Ok(controller.weight_nice.load(Ordering::Relaxed).to_string()),
        "max" => {
            let quota = controller.max_quota.load(Ordering::Relaxed);
            let period = controller.max_period.load(Ordering::Relaxed);
            if quota == u64::MAX {
                Ok(alloc::format!("max {}", period))
            } else {
                Ok(alloc::format!("{} {}", quota, period))
            }
        }
        "max.burst" => Ok(controller.burst.load(Ordering::Relaxed).to_string()),
        "stat" => {
            Ok(alloc::format!(
                "usage_usec {}\n\
                 user_usec {}\n\
                 system_usec {}\n\
                 nr_periods {}\n\
                 nr_throttled {}\n\
                 throttled_usec {}\n\
                 nr_bursts {}\n\
                 burst_usec {}",
                controller.usage_usec.load(Ordering::Relaxed),
                controller.user_usec.load(Ordering::Relaxed),
                controller.system_usec.load(Ordering::Relaxed),
                controller.nr_periods.load(Ordering::Relaxed),
                controller.nr_throttled.load(Ordering::Relaxed),
                controller.throttled_usec.load(Ordering::Relaxed),
                controller.nr_bursts.load(Ordering::Relaxed),
                controller.burst_usec.load(Ordering::Relaxed),
            ))
        }
        "pressure" => {
            // PSI-style pressure info
            Ok("some avg10=0.00 avg60=0.00 avg300=0.00 total=0\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=0".to_string())
        }
        _ => Err(CgroupError::NotFound),
    }
}

/// Write a CPU controller file
pub fn write_file(controller: &mut CpuController, file: &str, value: &str) -> Result<(), CgroupError> {
    let value = value.trim();

    match file {
        "weight" => {
            let weight: u64 = value.parse()
                .map_err(|_| CgroupError::InvalidPath)?;
            if weight < 1 || weight > CPU_WEIGHT_MAX {
                return Err(CgroupError::InvalidPath);
            }
            controller.weight.store(weight, Ordering::Release);
            Ok(())
        }
        "weight.nice" => {
            let nice: i64 = value.parse()
                .map_err(|_| CgroupError::InvalidPath)?;
            if nice < -20 || nice > 19 {
                return Err(CgroupError::InvalidPath);
            }
            controller.weight_nice.store(nice, Ordering::Release);
            // Convert nice to weight
            let weight = nice_to_weight(nice);
            controller.weight.store(weight, Ordering::Release);
            Ok(())
        }
        "max" => {
            let parts: Vec<&str> = value.split_whitespace().collect();
            let quota = if parts[0] == "max" {
                u64::MAX
            } else {
                parts[0].parse().map_err(|_| CgroupError::InvalidPath)?
            };

            let period = if parts.len() > 1 {
                parts[1].parse().map_err(|_| CgroupError::InvalidPath)?
            } else {
                DEFAULT_PERIOD
            };

            controller.max_quota.store(quota, Ordering::Release);
            controller.max_period.store(period, Ordering::Release);
            Ok(())
        }
        "max.burst" => {
            let burst: u64 = value.parse()
                .map_err(|_| CgroupError::InvalidPath)?;
            controller.burst.store(burst, Ordering::Release);
            Ok(())
        }
        _ => Err(CgroupError::NotFound),
    }
}

/// Convert nice value to weight
fn nice_to_weight(nice: i64) -> u64 {
    // Nice -20 to 19 maps to weight 88761 to 3
    // Using exponential decay similar to Linux kernel
    const NICE_TO_WEIGHT: [u64; 40] = [
        88761, 71755, 56483, 46273, 36291,
        29154, 23254, 18705, 14949, 11916,
        9548,  7620,  6100,  4904,  3906,
        3121,  2501,  1991,  1586,  1277,
        1024,  820,   655,   526,   423,
        335,   272,   215,   172,   137,
        110,   87,    70,    56,    45,
        36,    29,    23,    18,    15,
    ];

    let index = (nice + 20) as usize;
    if index < 40 {
        NICE_TO_WEIGHT[index]
    } else {
        CPU_WEIGHT_DEFAULT
    }
}

/// Apply CPU limits to a process
pub fn apply_to_process(pid: u32, controller: &CpuController) -> Result<(), CgroupError> {
    let weight = controller.weight.load(Ordering::Relaxed);
    let quota = controller.max_quota.load(Ordering::Relaxed);
    let period = controller.max_period.load(Ordering::Relaxed);

    crate::kprintln!("[CGROUPS] Process {} CPU weight: {}, quota: {}/{}us",
        pid, weight,
        if quota == u64::MAX { "max".to_string() } else { quota.to_string() },
        period);

    // Would integrate with scheduler here
    Ok(())
}

/// Calculate CPU bandwidth limit as percentage
pub fn bandwidth_percent(controller: &CpuController) -> Option<f64> {
    let quota = controller.max_quota.load(Ordering::Relaxed);
    let period = controller.max_period.load(Ordering::Relaxed);

    if quota == u64::MAX || period == 0 {
        None
    } else {
        Some((quota as f64 / period as f64) * 100.0)
    }
}
