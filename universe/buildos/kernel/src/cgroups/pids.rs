// ===============================================================================
// BUILDOS KERNEL - CGROUPS PIDS CONTROLLER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! PIDs Controller for cgroups v2
//!
//! Provides process count limits to prevent fork bombs.

use alloc::string::{String, ToString};
use core::sync::atomic::{AtomicU64, Ordering};

use super::CgroupError;

/// Max PIDs value (unlimited)
pub const PIDS_MAX: u64 = u64::MAX;

/// Initialize PIDs controller
pub fn init() {
    crate::kprintln!("[CGROUPS] PIDs controller initialized");
}

/// PIDs controller state
pub struct PidsController {
    /// Current number of processes
    pub current: AtomicU64,
    /// Maximum number of processes
    pub max: AtomicU64,
    /// Peak process count
    pub peak: AtomicU64,
    /// Number of times max was hit
    pub events_max: AtomicU64,
}

impl PidsController {
    /// Create new PIDs controller
    pub fn new() -> Self {
        Self {
            current: AtomicU64::new(0),
            max: AtomicU64::new(PIDS_MAX),
            peak: AtomicU64::new(0),
            events_max: AtomicU64::new(0),
        }
    }

    /// Try to allocate a PID slot
    pub fn try_charge(&self) -> bool {
        let max = self.max.load(Ordering::Relaxed);
        let mut current = self.current.load(Ordering::Relaxed);

        loop {
            if current >= max {
                self.events_max.fetch_add(1, Ordering::Relaxed);
                return false;
            }

            match self.current.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // Update peak
                    let new_current = current + 1;
                    let mut peak = self.peak.load(Ordering::Relaxed);
                    while new_current > peak {
                        match self.peak.compare_exchange_weak(
                            peak,
                            new_current,
                            Ordering::AcqRel,
                            Ordering::Relaxed,
                        ) {
                            Ok(_) => break,
                            Err(p) => peak = p,
                        }
                    }
                    return true;
                }
                Err(c) => current = c,
            }
        }
    }

    /// Release a PID slot
    pub fn uncharge(&self) {
        self.current.fetch_sub(1, Ordering::AcqRel);
    }

    /// Check if at limit
    pub fn at_limit(&self) -> bool {
        let max = self.max.load(Ordering::Relaxed);
        let current = self.current.load(Ordering::Relaxed);
        current >= max
    }
}

impl Default for PidsController {
    fn default() -> Self {
        Self::new()
    }
}

/// Read a PIDs controller file
pub fn read_file(controller: &PidsController, file: &str) -> Result<String, CgroupError> {
    match file {
        "current" => Ok(controller.current.load(Ordering::Relaxed).to_string()),
        "max" => {
            let max = controller.max.load(Ordering::Relaxed);
            if max == PIDS_MAX {
                Ok("max".to_string())
            } else {
                Ok(max.to_string())
            }
        }
        "peak" => Ok(controller.peak.load(Ordering::Relaxed).to_string()),
        "events" => {
            Ok(alloc::format!(
                "max {}",
                controller.events_max.load(Ordering::Relaxed)
            ))
        }
        _ => Err(CgroupError::NotFound),
    }
}

/// Write a PIDs controller file
pub fn write_file(controller: &mut PidsController, file: &str, value: &str) -> Result<(), CgroupError> {
    let value = value.trim();

    match file {
        "max" => {
            let max = if value == "max" {
                PIDS_MAX
            } else {
                value.parse().map_err(|_| CgroupError::InvalidPath)?
            };
            controller.max.store(max, Ordering::Release);
            Ok(())
        }
        _ => Err(CgroupError::NotFound),
    }
}

/// Apply PIDs limits to a process
pub fn apply_to_process(pid: u32, controller: &PidsController) -> Result<(), CgroupError> {
    let max = controller.max.load(Ordering::Relaxed);

    if max != PIDS_MAX {
        crate::kprintln!("[CGROUPS] Process {} PID limit: {}", pid, max);
    }

    Ok(())
}

/// Check if a fork is allowed
pub fn can_fork(controller: &PidsController) -> bool {
    controller.try_charge()
}

/// Called when a process exits
pub fn process_exit(controller: &PidsController) {
    controller.uncharge();
}
