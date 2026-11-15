//! BuildOS Init System
//!
//! System initialization and service management, similar to systemd.
//! Provides:
//! - Service unit definitions
//! - Dependency resolution
//! - Parallel service startup
//! - Service monitoring and restart
//! - Target (runlevel) management

pub mod unit;
pub mod service;
pub mod target;
pub mod dependency;
pub mod manager;
pub mod cgroup;
pub mod journal;

pub use unit::{Unit, UnitConfig, UnitState, UnitType};
pub use service::{Service, ServiceConfig, ServiceState, RestartPolicy};
pub use target::{Target, TargetConfig};
pub use manager::{ServiceManager, ManagerConfig};

use alloc::string::String;
use alloc::vec::Vec;
use crate::sync::RwLock;

/// Service manager instance
static SERVICE_MANAGER: RwLock<Option<ServiceManager>> = RwLock::new(None);

/// System state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemState {
    /// System is starting up
    Starting,
    /// System is running normally
    Running,
    /// System is degraded (some services failed)
    Degraded,
    /// System is shutting down
    Stopping,
    /// System is in maintenance mode
    Maintenance,
    /// System is in emergency mode
    Emergency,
}

/// Boot statistics
#[derive(Clone, Debug, Default)]
pub struct BootStats {
    /// Time when boot started (ns since epoch)
    pub boot_start: u64,
    /// Time when userspace started (ns since epoch)
    pub userspace_start: u64,
    /// Time when boot completed (ns since epoch)
    pub boot_complete: u64,
    /// Number of services started
    pub services_started: u32,
    /// Number of services failed
    pub services_failed: u32,
}

/// Initialize the init system
pub fn init() {
    let mut manager = SERVICE_MANAGER.write();
    *manager = Some(ServiceManager::new(ManagerConfig::default()));

    if let Some(ref mut mgr) = *manager {
        // Register built-in services
        register_builtin_services(mgr);

        // Load unit files from /etc/buildos/system/
        mgr.load_units();
    }
}

/// Register built-in system services
fn register_builtin_services(manager: &mut ServiceManager) {
    // Register essential services
    let units = vec![
        // System logging
        unit::Unit::service(
            "syslog.service",
            ServiceConfig {
                description: String::from("System Logging Service"),
                exec_start: String::from("/usr/sbin/syslogd"),
                restart: RestartPolicy::Always,
                ..Default::default()
            },
        ),
        // Device manager
        unit::Unit::service(
            "udevd.service",
            ServiceConfig {
                description: String::from("Device Manager"),
                exec_start: String::from("/usr/sbin/udevd"),
                service_type: service::ServiceType::Notify,
                ..Default::default()
            },
        ),
        // Network manager
        unit::Unit::service(
            "networkd.service",
            ServiceConfig {
                description: String::from("Network Manager"),
                exec_start: String::from("/usr/sbin/networkd"),
                after: vec![String::from("udevd.service")],
                wants: vec![String::from("syslog.service")],
                ..Default::default()
            },
        ),
        // Login service
        unit::Unit::service(
            "logind.service",
            ServiceConfig {
                description: String::from("Login Manager"),
                exec_start: String::from("/usr/sbin/logind"),
                after: vec![String::from("udevd.service")],
                ..Default::default()
            },
        ),
        // Console getty
        unit::Unit::service(
            "getty@tty1.service",
            ServiceConfig {
                description: String::from("Virtual Console"),
                exec_start: String::from("/usr/sbin/agetty tty1"),
                restart: RestartPolicy::Always,
                ..Default::default()
            },
        ),
    ];

    for unit in units {
        manager.register_unit(unit);
    }

    // Register targets
    let targets = vec![
        // Basic target
        Target::new(
            "basic.target",
            TargetConfig {
                description: String::from("Basic System"),
                wants: vec![
                    String::from("syslog.service"),
                    String::from("udevd.service"),
                ],
                ..Default::default()
            },
        ),
        // Multi-user target
        Target::new(
            "multi-user.target",
            TargetConfig {
                description: String::from("Multi-User System"),
                requires: vec![String::from("basic.target")],
                wants: vec![
                    String::from("networkd.service"),
                    String::from("logind.service"),
                    String::from("getty@tty1.service"),
                ],
                ..Default::default()
            },
        ),
        // Graphical target
        Target::new(
            "graphical.target",
            TargetConfig {
                description: String::from("Graphical Interface"),
                requires: vec![String::from("multi-user.target")],
                ..Default::default()
            },
        ),
        // Rescue target
        Target::new(
            "rescue.target",
            TargetConfig {
                description: String::from("Rescue Mode"),
                requires: vec![String::from("basic.target")],
                ..Default::default()
            },
        ),
        // Emergency target
        Target::new(
            "emergency.target",
            TargetConfig {
                description: String::from("Emergency Mode"),
                ..Default::default()
            },
        ),
    ];

    for target in targets {
        manager.register_target(target);
    }
}

/// Start the init system
pub fn start() {
    let manager = SERVICE_MANAGER.read();
    if let Some(ref mgr) = *manager {
        // Set default target
        mgr.set_default_target("multi-user.target");

        // Start the default target
        mgr.start_default_target();
    }
}

/// Get system state
pub fn system_state() -> SystemState {
    let manager = SERVICE_MANAGER.read();
    if let Some(ref mgr) = *manager {
        mgr.state()
    } else {
        SystemState::Starting
    }
}

/// Request system shutdown
pub fn shutdown() {
    let manager = SERVICE_MANAGER.read();
    if let Some(ref mgr) = *manager {
        mgr.shutdown();
    }
}

/// Request system reboot
pub fn reboot() {
    let manager = SERVICE_MANAGER.read();
    if let Some(ref mgr) = *manager {
        mgr.reboot();
    }
}

/// Start a service by name
pub fn start_service(name: &str) -> Result<(), InitError> {
    let manager = SERVICE_MANAGER.read();
    if let Some(ref mgr) = *manager {
        mgr.start_unit(name)
    } else {
        Err(InitError::NotInitialized)
    }
}

/// Stop a service by name
pub fn stop_service(name: &str) -> Result<(), InitError> {
    let manager = SERVICE_MANAGER.read();
    if let Some(ref mgr) = *manager {
        mgr.stop_unit(name)
    } else {
        Err(InitError::NotInitialized)
    }
}

/// Restart a service by name
pub fn restart_service(name: &str) -> Result<(), InitError> {
    let manager = SERVICE_MANAGER.read();
    if let Some(ref mgr) = *manager {
        mgr.restart_unit(name)
    } else {
        Err(InitError::NotInitialized)
    }
}

/// Get service status
pub fn service_status(name: &str) -> Result<ServiceState, InitError> {
    let manager = SERVICE_MANAGER.read();
    if let Some(ref mgr) = *manager {
        mgr.unit_state(name)
    } else {
        Err(InitError::NotInitialized)
    }
}

/// List all services
pub fn list_services() -> Vec<(String, UnitState)> {
    let manager = SERVICE_MANAGER.read();
    if let Some(ref mgr) = *manager {
        mgr.list_units()
    } else {
        Vec::new()
    }
}

/// Init system errors
#[derive(Clone, Debug)]
pub enum InitError {
    /// System not initialized
    NotInitialized,
    /// Unit not found
    UnitNotFound(String),
    /// Unit already exists
    UnitExists(String),
    /// Dependency error
    DependencyError(String),
    /// Failed to start unit
    StartFailed(String),
    /// Failed to stop unit
    StopFailed(String),
    /// Configuration error
    ConfigError(String),
    /// Permission denied
    PermissionDenied,
    /// I/O error
    IoError(String),
}

/// Power action
#[derive(Clone, Copy, Debug)]
pub enum PowerAction {
    /// Power off the system
    PowerOff,
    /// Reboot the system
    Reboot,
    /// Halt the system (don't power off)
    Halt,
    /// Suspend to RAM
    Suspend,
    /// Hibernate to disk
    Hibernate,
    /// Hybrid suspend (hibernate + suspend)
    HybridSleep,
}

/// Execute a power action
pub fn power_action(action: PowerAction) {
    let manager = SERVICE_MANAGER.read();
    if let Some(ref mgr) = *manager {
        match action {
            PowerAction::PowerOff => mgr.poweroff(),
            PowerAction::Reboot => mgr.reboot(),
            PowerAction::Halt => mgr.halt(),
            PowerAction::Suspend => mgr.suspend(),
            PowerAction::Hibernate => mgr.hibernate(),
            PowerAction::HybridSleep => mgr.hybrid_sleep(),
        }
    }
}
