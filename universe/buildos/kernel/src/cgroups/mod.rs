// ===============================================================================
// BUILDOS KERNEL - CONTROL GROUPS (CGROUPS) SUBSYSTEM
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Control Groups (cgroups) Subsystem
//!
//! Provides resource control and isolation for processes:
//! - Memory limits and accounting
//! - CPU scheduling constraints
//! - I/O bandwidth control
//! - Process count limits
//! - Device access control
//! - Freezer control

#![allow(dead_code)]

pub mod memory;
pub mod cpu;
pub mod io;
pub mod pids;
pub mod devices;
pub mod freezer;

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::sync::RwLock;

/// Cgroups subsystem initialized
static CGROUPS_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Root cgroup
static ROOT_CGROUP: RwLock<Option<Cgroup>> = RwLock::new(None);

/// Cgroups v2 unified hierarchy
static HIERARCHY: RwLock<BTreeMap<String, Cgroup>> = RwLock::new(BTreeMap::new());

/// Cgroup ID counter
static CGROUP_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Initialize cgroups subsystem
pub fn init() {
    // Create root cgroup and add to hierarchy
    let root = Cgroup::new_root();
    HIERARCHY.write().insert("/".to_string(), root);

    // ROOT_CGROUP is no longer used - we access via HIERARCHY

    // Register controllers
    memory::init();
    cpu::init();
    io::init();
    pids::init();
    devices::init();
    freezer::init();

    CGROUPS_INITIALIZED.store(true, Ordering::Release);
    crate::kprintln!("[CGROUPS] Control groups v2 initialized");
}

/// A control group
pub struct Cgroup {
    /// Cgroup ID
    pub id: u64,
    /// Cgroup path
    pub path: String,
    /// Parent cgroup path
    pub parent: Option<String>,
    /// Child cgroups
    pub children: Vec<String>,
    /// Member processes
    pub procs: Vec<u32>,
    /// Enabled controllers
    pub controllers: Vec<ControllerType>,
    /// Memory controller
    pub memory: memory::MemoryController,
    /// CPU controller
    pub cpu: cpu::CpuController,
    /// I/O controller
    pub io: io::IoController,
    /// PIDs controller
    pub pids: pids::PidsController,
    /// Devices controller
    pub devices: devices::DevicesController,
    /// Freezer controller
    pub freezer: freezer::FreezerController,
    /// Frozen state
    pub frozen: bool,
    /// Populated (has processes)
    pub populated: bool,
}

impl Cgroup {
    /// Create root cgroup
    fn new_root() -> Self {
        Self {
            id: 0,
            path: "/".to_string(),
            parent: None,
            children: Vec::new(),
            procs: Vec::new(),
            controllers: vec![
                ControllerType::Memory,
                ControllerType::Cpu,
                ControllerType::Io,
                ControllerType::Pids,
                ControllerType::Devices,
                ControllerType::Freezer,
            ],
            memory: memory::MemoryController::new(),
            cpu: cpu::CpuController::new(),
            io: io::IoController::new(),
            pids: pids::PidsController::new(),
            devices: devices::DevicesController::new(),
            freezer: freezer::FreezerController::new(),
            frozen: false,
            populated: false,
        }
    }

    /// Create child cgroup
    fn new_child(parent_path: &str, name: &str) -> Self {
        let path = if parent_path == "/" {
            alloc::format!("/{}", name)
        } else {
            alloc::format!("{}/{}", parent_path, name)
        };

        Self {
            id: CGROUP_ID_COUNTER.fetch_add(1, Ordering::Relaxed),
            path,
            parent: Some(parent_path.to_string()),
            children: Vec::new(),
            procs: Vec::new(),
            controllers: Vec::new(),
            memory: memory::MemoryController::new(),
            cpu: cpu::CpuController::new(),
            io: io::IoController::new(),
            pids: pids::PidsController::new(),
            devices: devices::DevicesController::new(),
            freezer: freezer::FreezerController::new(),
            frozen: false,
            populated: false,
        }
    }
}

/// Controller types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControllerType {
    Memory,
    Cpu,
    Io,
    Pids,
    Devices,
    Freezer,
    CpuSet,
    Hugetlb,
    Rdma,
    Misc,
}

impl ControllerType {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Cpu => "cpu",
            Self::Io => "io",
            Self::Pids => "pids",
            Self::Devices => "devices",
            Self::Freezer => "freezer",
            Self::CpuSet => "cpuset",
            Self::Hugetlb => "hugetlb",
            Self::Rdma => "rdma",
            Self::Misc => "misc",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "memory" => Some(Self::Memory),
            "cpu" => Some(Self::Cpu),
            "io" => Some(Self::Io),
            "pids" => Some(Self::Pids),
            "devices" => Some(Self::Devices),
            "freezer" => Some(Self::Freezer),
            "cpuset" => Some(Self::CpuSet),
            "hugetlb" => Some(Self::Hugetlb),
            "rdma" => Some(Self::Rdma),
            "misc" => Some(Self::Misc),
            _ => None,
        }
    }
}

/// Cgroup errors
#[derive(Clone, Debug)]
pub enum CgroupError {
    /// Cgroup not found
    NotFound,
    /// Cgroup already exists
    AlreadyExists,
    /// Invalid path
    InvalidPath,
    /// Not empty (has children or processes)
    NotEmpty,
    /// Controller not available
    ControllerNotAvailable,
    /// Resource limit exceeded
    ResourceLimitExceeded,
    /// Permission denied
    PermissionDenied,
    /// Operation not supported
    NotSupported,
}

impl CgroupError {
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::NotFound => -2,             // ENOENT
            Self::AlreadyExists => -17,       // EEXIST
            Self::InvalidPath => -22,         // EINVAL
            Self::NotEmpty => -39,            // ENOTEMPTY
            Self::ControllerNotAvailable => -95, // ENOTSUP
            Self::ResourceLimitExceeded => -12,  // ENOMEM
            Self::PermissionDenied => -1,     // EPERM
            Self::NotSupported => -95,        // ENOTSUP
        }
    }
}

/// Create a cgroup
pub fn create_cgroup(path: &str) -> Result<(), CgroupError> {
    if !path.starts_with('/') {
        return Err(CgroupError::InvalidPath);
    }

    let mut hierarchy = HIERARCHY.write();

    if hierarchy.contains_key(path) {
        return Err(CgroupError::AlreadyExists);
    }

    // Find parent
    let parent_path = parent_path(path).ok_or(CgroupError::InvalidPath)?;
    if !hierarchy.contains_key(&parent_path) {
        return Err(CgroupError::NotFound);
    }

    // Create cgroup
    let name = path.rsplit('/').next().unwrap_or("");
    let cgroup = Cgroup::new_child(&parent_path, name);

    // Update parent
    if let Some(parent) = hierarchy.get_mut(&parent_path) {
        parent.children.push(path.to_string());
    }

    hierarchy.insert(path.to_string(), cgroup);

    crate::kprintln!("[CGROUPS] Created cgroup: {}", path);

    Ok(())
}

/// Remove a cgroup
pub fn remove_cgroup(path: &str) -> Result<(), CgroupError> {
    if path == "/" {
        return Err(CgroupError::PermissionDenied);
    }

    let mut hierarchy = HIERARCHY.write();

    let cgroup = hierarchy.get(path).ok_or(CgroupError::NotFound)?;

    // Check if empty
    if !cgroup.children.is_empty() || !cgroup.procs.is_empty() {
        return Err(CgroupError::NotEmpty);
    }

    // Update parent
    if let Some(parent_path) = &cgroup.parent.clone() {
        if let Some(parent) = hierarchy.get_mut(parent_path) {
            parent.children.retain(|c| c != path);
        }
    }

    hierarchy.remove(path);

    crate::kprintln!("[CGROUPS] Removed cgroup: {}", path);

    Ok(())
}

/// Add a process to a cgroup
pub fn attach_process(path: &str, pid: u32) -> Result<(), CgroupError> {
    let mut hierarchy = HIERARCHY.write();

    // Remove from old cgroup
    for cgroup in hierarchy.values_mut() {
        cgroup.procs.retain(|&p| p != pid);
        cgroup.populated = !cgroup.procs.is_empty();
    }

    // Add to new cgroup
    let cgroup = hierarchy.get_mut(path).ok_or(CgroupError::NotFound)?;
    cgroup.procs.push(pid);
    cgroup.populated = true;

    // Apply controller limits
    apply_limits(pid, cgroup)?;

    Ok(())
}

/// Apply cgroup limits to a process
fn apply_limits(pid: u32, cgroup: &Cgroup) -> Result<(), CgroupError> {
    // Memory limits
    if cgroup.controllers.contains(&ControllerType::Memory) {
        memory::apply_to_process(pid, &cgroup.memory)?;
    }

    // CPU limits
    if cgroup.controllers.contains(&ControllerType::Cpu) {
        cpu::apply_to_process(pid, &cgroup.cpu)?;
    }

    // PID limits
    if cgroup.controllers.contains(&ControllerType::Pids) {
        pids::apply_to_process(pid, &cgroup.pids)?;
    }

    Ok(())
}

/// Get cgroup for a process
pub fn get_cgroup(pid: u32) -> Option<String> {
    let hierarchy = HIERARCHY.read();

    for (path, cgroup) in hierarchy.iter() {
        if cgroup.procs.contains(&pid) {
            return Some(path.clone());
        }
    }

    Some("/".to_string()) // Default to root
}

/// List cgroups
pub fn list_cgroups() -> Vec<CgroupInfo> {
    HIERARCHY.read()
        .values()
        .map(|c| CgroupInfo {
            id: c.id,
            path: c.path.clone(),
            controllers: c.controllers.iter().map(|ct| ct.name().to_string()).collect(),
            process_count: c.procs.len(),
            frozen: c.frozen,
        })
        .collect()
}

/// Cgroup info for queries
#[derive(Clone, Debug)]
pub struct CgroupInfo {
    pub id: u64,
    pub path: String,
    pub controllers: Vec<String>,
    pub process_count: usize,
    pub frozen: bool,
}

/// Enable a controller for a cgroup
pub fn enable_controller(path: &str, controller: ControllerType) -> Result<(), CgroupError> {
    let mut hierarchy = HIERARCHY.write();

    // First check parent controller availability
    let parent_path = {
        let cgroup = hierarchy.get(path).ok_or(CgroupError::NotFound)?;
        cgroup.parent.clone()
    };

    if let Some(parent_path) = parent_path {
        if let Some(parent) = hierarchy.get(&parent_path) {
            if !parent.controllers.contains(&controller) {
                return Err(CgroupError::ControllerNotAvailable);
            }
        }
    }

    // Now get mutable reference and add controller
    let cgroup = hierarchy.get_mut(path).ok_or(CgroupError::NotFound)?;
    if !cgroup.controllers.contains(&controller) {
        cgroup.controllers.push(controller);
    }

    Ok(())
}

/// Disable a controller for a cgroup
pub fn disable_controller(path: &str, controller: ControllerType) -> Result<(), CgroupError> {
    let mut hierarchy = HIERARCHY.write();

    // First extract children paths
    let children = {
        let cgroup = hierarchy.get(path).ok_or(CgroupError::NotFound)?;
        cgroup.children.clone()
    };

    // Check if any children use this controller
    for child_path in &children {
        if let Some(child) = hierarchy.get(child_path) {
            if child.controllers.contains(&controller) {
                return Err(CgroupError::NotEmpty);
            }
        }
    }

    // Now get mutable reference and remove controller
    let cgroup = hierarchy.get_mut(path).ok_or(CgroupError::NotFound)?;
    cgroup.controllers.retain(|&c| c != controller);

    Ok(())
}

/// Get parent path
fn parent_path(path: &str) -> Option<String> {
    if path == "/" {
        return None;
    }

    let last_slash = path.rfind('/')?;
    if last_slash == 0 {
        Some("/".to_string())
    } else {
        Some(path[..last_slash].to_string())
    }
}

/// Read a cgroup file (for cgroup filesystem)
pub fn read_file(path: &str, file: &str) -> Result<String, CgroupError> {
    let hierarchy = HIERARCHY.read();
    let cgroup = hierarchy.get(path).ok_or(CgroupError::NotFound)?;

    match file {
        "cgroup.type" => Ok("domain".to_string()),
        "cgroup.procs" => {
            let procs: Vec<String> = cgroup.procs.iter()
                .map(|p| alloc::format!("{}", p))
                .collect();
            Ok(procs.join("\n"))
        }
        "cgroup.controllers" => {
            let controllers: Vec<&str> = cgroup.controllers.iter()
                .map(|c| c.name())
                .collect();
            Ok(controllers.join(" "))
        }
        "cgroup.subtree_control" => {
            // Same as controllers for now
            let controllers: Vec<&str> = cgroup.controllers.iter()
                .map(|c| c.name())
                .collect();
            Ok(controllers.join(" "))
        }
        "cgroup.events" => {
            Ok(alloc::format!(
                "populated {}\nfrozen {}",
                if cgroup.populated { 1 } else { 0 },
                if cgroup.frozen { 1 } else { 0 }
            ))
        }
        "cgroup.freeze" => {
            Ok(if cgroup.frozen { "1".to_string() } else { "0".to_string() })
        }
        _ => {
            // Try controller-specific files
            if file.starts_with("memory.") {
                memory::read_file(&cgroup.memory, &file[7..])
            } else if file.starts_with("cpu.") {
                cpu::read_file(&cgroup.cpu, &file[4..])
            } else if file.starts_with("io.") {
                io::read_file(&cgroup.io, &file[3..])
            } else if file.starts_with("pids.") {
                pids::read_file(&cgroup.pids, &file[5..])
            } else {
                Err(CgroupError::NotFound)
            }
        }
    }
}

/// Write a cgroup file
pub fn write_file(path: &str, file: &str, value: &str) -> Result<(), CgroupError> {
    let mut hierarchy = HIERARCHY.write();
    let cgroup = hierarchy.get_mut(path).ok_or(CgroupError::NotFound)?;

    match file {
        "cgroup.procs" => {
            let pid: u32 = value.trim().parse()
                .map_err(|_| CgroupError::InvalidPath)?;
            drop(hierarchy);
            attach_process(path, pid)
        }
        "cgroup.subtree_control" => {
            // Enable/disable controllers
            for part in value.split_whitespace() {
                if let Some(name) = part.strip_prefix('+') {
                    if let Some(controller) = ControllerType::from_name(name) {
                        cgroup.controllers.push(controller);
                    }
                } else if let Some(name) = part.strip_prefix('-') {
                    if let Some(controller) = ControllerType::from_name(name) {
                        cgroup.controllers.retain(|&c| c != controller);
                    }
                }
            }
            Ok(())
        }
        "cgroup.freeze" => {
            cgroup.frozen = value.trim() == "1";
            if cgroup.frozen {
                freezer::freeze_cgroup(cgroup)?;
            } else {
                freezer::thaw_cgroup(cgroup)?;
            }
            Ok(())
        }
        _ => {
            // Try controller-specific files
            if file.starts_with("memory.") {
                memory::write_file(&mut cgroup.memory, &file[7..], value)
            } else if file.starts_with("cpu.") {
                cpu::write_file(&mut cgroup.cpu, &file[4..], value)
            } else if file.starts_with("io.") {
                io::write_file(&mut cgroup.io, &file[3..], value)
            } else if file.starts_with("pids.") {
                pids::write_file(&mut cgroup.pids, &file[5..], value)
            } else {
                Err(CgroupError::NotFound)
            }
        }
    }
}

/// Procfs-style cgroup listing for a process
pub fn proc_cgroup(pid: u32) -> String {
    if let Some(path) = get_cgroup(pid) {
        alloc::format!("0::{}\n", path)
    } else {
        "0::/\n".to_string()
    }
}
