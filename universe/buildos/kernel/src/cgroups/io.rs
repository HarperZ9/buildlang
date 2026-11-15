// ===============================================================================
// BUILDOS KERNEL - CGROUPS I/O CONTROLLER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! I/O Controller for cgroups v2
//!
//! Provides I/O resource control:
//! - I/O bandwidth limits
//! - I/O weight for fair scheduling
//! - I/O latency control
//! - Device-specific limits

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use super::CgroupError;

/// Default I/O weight
pub const IO_WEIGHT_DEFAULT: u64 = 100;

/// Max I/O weight
pub const IO_WEIGHT_MAX: u64 = 10000;

/// Initialize I/O controller
pub fn init() {
    crate::kprintln!("[CGROUPS] I/O controller initialized");
}

/// I/O controller state
pub struct IoController {
    /// Default weight for all devices
    pub weight: AtomicU64,
    /// Per-device weights (major:minor -> weight)
    pub device_weights: BTreeMap<(u32, u32), u64>,
    /// Per-device max limits
    pub device_max: BTreeMap<(u32, u32), IoMax>,
    /// Per-device latency targets
    pub device_latency: BTreeMap<(u32, u32), u64>,
    /// Statistics per device
    pub stats: BTreeMap<(u32, u32), IoStats>,
    /// Total bytes read
    pub rbytes: AtomicU64,
    /// Total bytes written
    pub wbytes: AtomicU64,
    /// Total read operations
    pub rios: AtomicU64,
    /// Total write operations
    pub wios: AtomicU64,
    /// Total discard bytes
    pub dbytes: AtomicU64,
    /// Total discard operations
    pub dios: AtomicU64,
}

impl IoController {
    /// Create new I/O controller
    pub fn new() -> Self {
        Self {
            weight: AtomicU64::new(IO_WEIGHT_DEFAULT),
            device_weights: BTreeMap::new(),
            device_max: BTreeMap::new(),
            device_latency: BTreeMap::new(),
            stats: BTreeMap::new(),
            rbytes: AtomicU64::new(0),
            wbytes: AtomicU64::new(0),
            rios: AtomicU64::new(0),
            wios: AtomicU64::new(0),
            dbytes: AtomicU64::new(0),
            dios: AtomicU64::new(0),
        }
    }

    /// Account I/O operation
    pub fn account(&self, bytes: u64, is_write: bool) {
        if is_write {
            self.wbytes.fetch_add(bytes, Ordering::Relaxed);
            self.wios.fetch_add(1, Ordering::Relaxed);
        } else {
            self.rbytes.fetch_add(bytes, Ordering::Relaxed);
            self.rios.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Check if I/O is allowed (bandwidth limit check)
    pub fn check_limit(&self, device: (u32, u32), bytes: u64, is_write: bool) -> bool {
        if let Some(max) = self.device_max.get(&device) {
            if is_write {
                if max.wbps > 0 {
                    // Check write bandwidth
                    let current = self.wbytes.load(Ordering::Relaxed);
                    if current + bytes > max.wbps {
                        return false;
                    }
                }
                if max.wiops > 0 {
                    let current = self.wios.load(Ordering::Relaxed);
                    if current >= max.wiops {
                        return false;
                    }
                }
            } else {
                if max.rbps > 0 {
                    let current = self.rbytes.load(Ordering::Relaxed);
                    if current + bytes > max.rbps {
                        return false;
                    }
                }
                if max.riops > 0 {
                    let current = self.rios.load(Ordering::Relaxed);
                    if current >= max.riops {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Get weight for a device
    pub fn get_weight(&self, device: (u32, u32)) -> u64 {
        self.device_weights.get(&device)
            .copied()
            .unwrap_or_else(|| self.weight.load(Ordering::Relaxed))
    }
}

impl Default for IoController {
    fn default() -> Self {
        Self::new()
    }
}

/// I/O max limits for a device
#[derive(Clone, Debug, Default)]
pub struct IoMax {
    /// Read bytes per second
    pub rbps: u64,
    /// Write bytes per second
    pub wbps: u64,
    /// Read I/O operations per second
    pub riops: u64,
    /// Write I/O operations per second
    pub wiops: u64,
}

impl IoMax {
    pub fn format(&self) -> String {
        let mut parts = Vec::new();
        if self.rbps > 0 {
            parts.push(alloc::format!("rbps={}", self.rbps));
        }
        if self.wbps > 0 {
            parts.push(alloc::format!("wbps={}", self.wbps));
        }
        if self.riops > 0 {
            parts.push(alloc::format!("riops={}", self.riops));
        }
        if self.wiops > 0 {
            parts.push(alloc::format!("wiops={}", self.wiops));
        }
        parts.join(" ")
    }
}

/// I/O statistics for a device
#[derive(Default)]
pub struct IoStats {
    pub rbytes: AtomicU64,
    pub wbytes: AtomicU64,
    pub rios: AtomicU64,
    pub wios: AtomicU64,
    pub dbytes: AtomicU64,
    pub dios: AtomicU64,
}

impl IoStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn format(&self, major: u32, minor: u32) -> String {
        alloc::format!(
            "{}:{} rbytes={} wbytes={} rios={} wios={} dbytes={} dios={}",
            major, minor,
            self.rbytes.load(Ordering::Relaxed),
            self.wbytes.load(Ordering::Relaxed),
            self.rios.load(Ordering::Relaxed),
            self.wios.load(Ordering::Relaxed),
            self.dbytes.load(Ordering::Relaxed),
            self.dios.load(Ordering::Relaxed),
        )
    }
}

/// Read an I/O controller file
pub fn read_file(controller: &IoController, file: &str) -> Result<String, CgroupError> {
    match file {
        "weight" => {
            let mut output = alloc::format!(
                "default {}\n",
                controller.weight.load(Ordering::Relaxed)
            );
            for ((major, minor), weight) in &controller.device_weights {
                output.push_str(&alloc::format!("{}:{} {}\n", major, minor, weight));
            }
            Ok(output.trim_end().to_string())
        }
        "max" => {
            let mut output = String::new();
            for ((major, minor), max) in &controller.device_max {
                output.push_str(&alloc::format!(
                    "{}:{} {}\n",
                    major, minor, max.format()
                ));
            }
            Ok(output.trim_end().to_string())
        }
        "latency" => {
            let mut output = String::new();
            for ((major, minor), lat) in &controller.device_latency {
                output.push_str(&alloc::format!(
                    "{}:{} target={}\n",
                    major, minor, lat
                ));
            }
            Ok(output.trim_end().to_string())
        }
        "stat" => {
            let mut output = String::new();
            for ((major, minor), stats) in &controller.stats {
                output.push_str(&stats.format(*major, *minor));
                output.push('\n');
            }

            // Add totals
            output.push_str(&alloc::format!(
                "Total: rbytes={} wbytes={} rios={} wios={}",
                controller.rbytes.load(Ordering::Relaxed),
                controller.wbytes.load(Ordering::Relaxed),
                controller.rios.load(Ordering::Relaxed),
                controller.wios.load(Ordering::Relaxed),
            ));

            Ok(output)
        }
        "pressure" => {
            Ok("some avg10=0.00 avg60=0.00 avg300=0.00 total=0\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=0".to_string())
        }
        _ => Err(CgroupError::NotFound),
    }
}

/// Write an I/O controller file
pub fn write_file(controller: &mut IoController, file: &str, value: &str) -> Result<(), CgroupError> {
    let value = value.trim();

    match file {
        "weight" => {
            // Parse "default N" or "MAJ:MIN N"
            let parts: Vec<&str> = value.split_whitespace().collect();
            if parts.len() != 2 {
                return Err(CgroupError::InvalidPath);
            }

            let weight: u64 = parts[1].parse()
                .map_err(|_| CgroupError::InvalidPath)?;

            if weight < 1 || weight > IO_WEIGHT_MAX {
                return Err(CgroupError::InvalidPath);
            }

            if parts[0] == "default" {
                controller.weight.store(weight, Ordering::Release);
            } else {
                let (major, minor) = parse_device(parts[0])?;
                controller.device_weights.insert((major, minor), weight);
            }
            Ok(())
        }
        "max" => {
            // Parse "MAJ:MIN rbps=N wbps=N riops=N wiops=N"
            let parts: Vec<&str> = value.split_whitespace().collect();
            if parts.is_empty() {
                return Err(CgroupError::InvalidPath);
            }

            let (major, minor) = parse_device(parts[0])?;
            let mut max = IoMax::default();

            for part in &parts[1..] {
                if let Some((key, val)) = part.split_once('=') {
                    let v = if val == "max" {
                        0
                    } else {
                        val.parse().map_err(|_| CgroupError::InvalidPath)?
                    };

                    match key {
                        "rbps" => max.rbps = v,
                        "wbps" => max.wbps = v,
                        "riops" => max.riops = v,
                        "wiops" => max.wiops = v,
                        _ => return Err(CgroupError::InvalidPath),
                    }
                }
            }

            controller.device_max.insert((major, minor), max);
            Ok(())
        }
        "latency" => {
            // Parse "MAJ:MIN target=N"
            let parts: Vec<&str> = value.split_whitespace().collect();
            if parts.len() != 2 {
                return Err(CgroupError::InvalidPath);
            }

            let (major, minor) = parse_device(parts[0])?;

            if let Some(target_str) = parts[1].strip_prefix("target=") {
                let target: u64 = target_str.parse()
                    .map_err(|_| CgroupError::InvalidPath)?;
                controller.device_latency.insert((major, minor), target);
                Ok(())
            } else {
                Err(CgroupError::InvalidPath)
            }
        }
        _ => Err(CgroupError::NotFound),
    }
}

/// Parse device string (MAJ:MIN)
fn parse_device(s: &str) -> Result<(u32, u32), CgroupError> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return Err(CgroupError::InvalidPath);
    }

    let major: u32 = parts[0].parse().map_err(|_| CgroupError::InvalidPath)?;
    let minor: u32 = parts[1].parse().map_err(|_| CgroupError::InvalidPath)?;

    Ok((major, minor))
}

/// Apply I/O limits to a process
pub fn apply_to_process(pid: u32, controller: &IoController) -> Result<(), CgroupError> {
    let weight = controller.weight.load(Ordering::Relaxed);

    crate::kprintln!("[CGROUPS] Process {} I/O weight: {}", pid, weight);

    // Would integrate with block I/O layer
    Ok(())
}
