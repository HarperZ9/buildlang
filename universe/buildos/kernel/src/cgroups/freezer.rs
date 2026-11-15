// ===============================================================================
// BUILDOS KERNEL - CGROUPS FREEZER CONTROLLER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Freezer Controller for cgroups v2
//!
//! Provides the ability to freeze/thaw all processes in a cgroup.

use alloc::string::{String, ToString};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use super::{Cgroup, CgroupError};

/// Initialize freezer controller
pub fn init() {
    crate::kprintln!("[CGROUPS] Freezer controller initialized");
}

/// Freezer controller state
pub struct FreezerController {
    /// Requested frozen state
    pub requested: AtomicBool,
    /// Actual frozen state (all processes frozen)
    pub frozen: AtomicBool,
    /// Self-frozen flag (for debugging)
    pub self_freezing: AtomicBool,
    /// Number of frozen processes
    pub frozen_count: AtomicU32,
    /// Number of processes in cgroup
    pub total_count: AtomicU32,
}

impl FreezerController {
    /// Create new freezer controller
    pub fn new() -> Self {
        Self {
            requested: AtomicBool::new(false),
            frozen: AtomicBool::new(false),
            self_freezing: AtomicBool::new(false),
            frozen_count: AtomicU32::new(0),
            total_count: AtomicU32::new(0),
        }
    }

    /// Check if frozen
    pub fn is_frozen(&self) -> bool {
        self.frozen.load(Ordering::Relaxed)
    }

    /// Check if freezing was requested
    pub fn is_freezing(&self) -> bool {
        self.requested.load(Ordering::Relaxed) && !self.frozen.load(Ordering::Relaxed)
    }

    /// Get freezer state string
    pub fn state(&self) -> &'static str {
        let requested = self.requested.load(Ordering::Relaxed);
        let frozen = self.frozen.load(Ordering::Relaxed);

        if frozen {
            "FROZEN"
        } else if requested {
            "FREEZING"
        } else {
            "THAWED"
        }
    }
}

impl Default for FreezerController {
    fn default() -> Self {
        Self::new()
    }
}

/// Read a freezer controller file
pub fn read_file(controller: &FreezerController, file: &str) -> Result<String, CgroupError> {
    match file {
        "state" => Ok(controller.state().to_string()),
        "self_freezing" => {
            Ok(if controller.self_freezing.load(Ordering::Relaxed) { "1" } else { "0" }.to_string())
        }
        _ => Err(CgroupError::NotFound),
    }
}

/// Write a freezer controller file
pub fn write_file(controller: &mut FreezerController, file: &str, value: &str) -> Result<(), CgroupError> {
    let value = value.trim();

    match file {
        "state" => {
            match value {
                "FROZEN" | "1" => {
                    controller.requested.store(true, Ordering::Release);
                    // Would trigger freeze operation
                }
                "THAWED" | "0" => {
                    controller.requested.store(false, Ordering::Release);
                    controller.frozen.store(false, Ordering::Release);
                    // Would trigger thaw operation
                }
                _ => return Err(CgroupError::InvalidPath),
            }
            Ok(())
        }
        _ => Err(CgroupError::NotFound),
    }
}

/// Freeze all processes in a cgroup
pub fn freeze_cgroup(cgroup: &Cgroup) -> Result<(), CgroupError> {
    crate::kprintln!("[CGROUPS] Freezing cgroup: {}", cgroup.path);

    // Send SIGSTOP to all processes
    for &pid in &cgroup.procs {
        freeze_process(pid)?;
    }

    // Update controller state
    let total = cgroup.procs.len() as u32;
    cgroup.freezer.total_count.store(total, Ordering::Release);
    cgroup.freezer.frozen_count.store(total, Ordering::Release);
    cgroup.freezer.frozen.store(true, Ordering::Release);

    Ok(())
}

/// Thaw all processes in a cgroup
pub fn thaw_cgroup(cgroup: &Cgroup) -> Result<(), CgroupError> {
    crate::kprintln!("[CGROUPS] Thawing cgroup: {}", cgroup.path);

    // Send SIGCONT to all processes
    for &pid in &cgroup.procs {
        thaw_process(pid)?;
    }

    // Update controller state
    cgroup.freezer.frozen_count.store(0, Ordering::Release);
    cgroup.freezer.frozen.store(false, Ordering::Release);
    cgroup.freezer.requested.store(false, Ordering::Release);

    Ok(())
}

/// Freeze a single process
fn freeze_process(pid: u32) -> Result<(), CgroupError> {
    // Would send SIGSTOP or use kernel task freezer
    crate::kprintln!("[CGROUPS] Freezing process {}", pid);

    // Mark process as frozen in task struct
    // process::get_process(pid).map(|p| p.freeze());

    Ok(())
}

/// Thaw a single process
fn thaw_process(pid: u32) -> Result<(), CgroupError> {
    // Would send SIGCONT or use kernel task thaw
    crate::kprintln!("[CGROUPS] Thawing process {}", pid);

    // Mark process as running in task struct
    // process::get_process(pid).map(|p| p.thaw());

    Ok(())
}

/// Check if a process should be frozen (for scheduler)
pub fn should_freeze(pid: u32) -> bool {
    // Check if process's cgroup is frozen
    if let Some(path) = super::get_cgroup(pid) {
        let hierarchy = super::HIERARCHY.read();
        if let Some(cgroup) = hierarchy.get(&path) {
            return cgroup.freezer.requested.load(Ordering::Relaxed);
        }
    }

    false
}

/// Called when process enters frozen state
pub fn process_frozen(pid: u32) {
    if let Some(path) = super::get_cgroup(pid) {
        let hierarchy = super::HIERARCHY.read();
        if let Some(cgroup) = hierarchy.get(&path) {
            let count = cgroup.freezer.frozen_count.fetch_add(1, Ordering::AcqRel);
            let total = cgroup.freezer.total_count.load(Ordering::Relaxed);

            // Check if all processes are frozen
            if count + 1 >= total {
                cgroup.freezer.frozen.store(true, Ordering::Release);
            }
        }
    }
}

/// Called when process exits frozen state (thawed or exits)
pub fn process_thawed(pid: u32) {
    if let Some(path) = super::get_cgroup(pid) {
        let hierarchy = super::HIERARCHY.read();
        if let Some(cgroup) = hierarchy.get(&path) {
            let prev = cgroup.freezer.frozen_count.fetch_sub(1, Ordering::AcqRel);
            if prev <= 1 {
                cgroup.freezer.frozen_count.store(0, Ordering::Release);
            }
        }
    }
}

/// Apply freezer state to a process
pub fn apply_to_process(pid: u32, controller: &FreezerController) -> Result<(), CgroupError> {
    if controller.requested.load(Ordering::Relaxed) {
        freeze_process(pid)?;
    }
    Ok(())
}
