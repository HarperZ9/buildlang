//! BuildOS Target Management
//!
//! Targets are synchronization points that group units together,
//! similar to runlevels in SysV init.

use alloc::string::String;
use alloc::vec::Vec;
use super::unit::{Unit, UnitState};

/// Target configuration
#[derive(Clone, Debug, Default)]
pub struct TargetConfig {
    /// Human-readable description
    pub description: String,
    /// Required units (must start successfully)
    pub requires: Vec<String>,
    /// Wanted units (should start but not required)
    pub wants: Vec<String>,
    /// Conflicting units
    pub conflicts: Vec<String>,
    /// Start after these units
    pub after: Vec<String>,
    /// Start before these units
    pub before: Vec<String>,
    /// Units that want this target installed
    pub wanted_by: Vec<String>,
    /// Units that require this target installed
    pub required_by: Vec<String>,
    /// Allow isolating to this target
    pub allow_isolate: bool,
    /// Is this the default target
    pub default_target: bool,
}

/// Runtime target state
pub struct Target {
    /// Target name
    pub name: String,
    /// Configuration
    pub config: TargetConfig,
    /// Current state
    pub state: TargetState,
    /// Timestamp when reached
    pub reached_timestamp: u64,
}

/// Target state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetState {
    /// Not active
    Dead,
    /// Being activated
    Activating,
    /// All dependencies met
    Active,
    /// Being deactivated
    Deactivating,
}

impl Target {
    /// Create a new target
    pub fn new(name: &str, config: TargetConfig) -> Self {
        Self {
            name: String::from(name),
            config,
            state: TargetState::Dead,
            reached_timestamp: 0,
        }
    }

    /// Get unit for this target
    pub fn to_unit(&self) -> Unit {
        Unit::target(&self.name, self.config.clone())
    }

    /// Check if target is active
    pub fn is_active(&self) -> bool {
        self.state == TargetState::Active
    }

    /// Get all dependencies
    pub fn all_dependencies(&self) -> Vec<&String> {
        let mut deps = Vec::new();
        deps.extend(&self.config.requires);
        deps.extend(&self.config.wants);
        deps
    }

    /// Check if all required dependencies are met
    pub fn check_dependencies<F>(&self, is_active: F) -> bool
    where
        F: Fn(&str) -> bool,
    {
        self.config.requires.iter().all(|dep| is_active(dep))
    }
}

impl From<TargetState> for UnitState {
    fn from(state: TargetState) -> Self {
        match state {
            TargetState::Dead => UnitState::Inactive,
            TargetState::Activating => UnitState::Activating,
            TargetState::Active => UnitState::Active,
            TargetState::Deactivating => UnitState::Deactivating,
        }
    }
}

/// Predefined system targets
pub mod predefined {
    use super::*;

    /// Sysinit target - early boot
    pub fn sysinit() -> Target {
        Target::new(
            "sysinit.target",
            TargetConfig {
                description: String::from("System Initialization"),
                ..Default::default()
            },
        )
    }

    /// Basic target - basic system
    pub fn basic() -> Target {
        Target::new(
            "basic.target",
            TargetConfig {
                description: String::from("Basic System"),
                requires: vec![String::from("sysinit.target")],
                after: vec![String::from("sysinit.target")],
                ..Default::default()
            },
        )
    }

    /// Network target - network is up
    pub fn network() -> Target {
        Target::new(
            "network.target",
            TargetConfig {
                description: String::from("Network"),
                after: vec![String::from("basic.target")],
                ..Default::default()
            },
        )
    }

    /// Network online target - network is configured
    pub fn network_online() -> Target {
        Target::new(
            "network-online.target",
            TargetConfig {
                description: String::from("Network is Online"),
                requires: vec![String::from("network.target")],
                after: vec![String::from("network.target")],
                ..Default::default()
            },
        )
    }

    /// Multi-user target - multi-user system
    pub fn multi_user() -> Target {
        Target::new(
            "multi-user.target",
            TargetConfig {
                description: String::from("Multi-User System"),
                requires: vec![String::from("basic.target")],
                after: vec![String::from("basic.target")],
                allow_isolate: true,
                ..Default::default()
            },
        )
    }

    /// Graphical target - graphical UI
    pub fn graphical() -> Target {
        Target::new(
            "graphical.target",
            TargetConfig {
                description: String::from("Graphical Interface"),
                requires: vec![String::from("multi-user.target")],
                after: vec![String::from("multi-user.target")],
                allow_isolate: true,
                default_target: true,
                ..Default::default()
            },
        )
    }

    /// Rescue target - single-user mode
    pub fn rescue() -> Target {
        Target::new(
            "rescue.target",
            TargetConfig {
                description: String::from("Rescue Mode"),
                requires: vec![String::from("sysinit.target")],
                after: vec![String::from("sysinit.target")],
                allow_isolate: true,
                conflicts: vec![String::from("multi-user.target")],
                ..Default::default()
            },
        )
    }

    /// Emergency target - emergency shell
    pub fn emergency() -> Target {
        Target::new(
            "emergency.target",
            TargetConfig {
                description: String::from("Emergency Mode"),
                allow_isolate: true,
                conflicts: vec![
                    String::from("rescue.target"),
                    String::from("multi-user.target"),
                ],
                ..Default::default()
            },
        )
    }

    /// Shutdown target
    pub fn shutdown() -> Target {
        Target::new(
            "shutdown.target",
            TargetConfig {
                description: String::from("Shutdown"),
                conflicts: vec![
                    String::from("multi-user.target"),
                    String::from("graphical.target"),
                ],
                ..Default::default()
            },
        )
    }

    /// Reboot target
    pub fn reboot() -> Target {
        Target::new(
            "reboot.target",
            TargetConfig {
                description: String::from("Reboot"),
                requires: vec![String::from("shutdown.target")],
                after: vec![String::from("shutdown.target")],
                ..Default::default()
            },
        )
    }

    /// Poweroff target
    pub fn poweroff() -> Target {
        Target::new(
            "poweroff.target",
            TargetConfig {
                description: String::from("Power Off"),
                requires: vec![String::from("shutdown.target")],
                after: vec![String::from("shutdown.target")],
                ..Default::default()
            },
        )
    }

    /// Halt target
    pub fn halt() -> Target {
        Target::new(
            "halt.target",
            TargetConfig {
                description: String::from("Halt"),
                requires: vec![String::from("shutdown.target")],
                after: vec![String::from("shutdown.target")],
                ..Default::default()
            },
        )
    }

    /// Suspend target
    pub fn suspend() -> Target {
        Target::new(
            "suspend.target",
            TargetConfig {
                description: String::from("Suspend"),
                ..Default::default()
            },
        )
    }

    /// Hibernate target
    pub fn hibernate() -> Target {
        Target::new(
            "hibernate.target",
            TargetConfig {
                description: String::from("Hibernate"),
                ..Default::default()
            },
        )
    }

    /// Hybrid sleep target
    pub fn hybrid_sleep() -> Target {
        Target::new(
            "hybrid-sleep.target",
            TargetConfig {
                description: String::from("Hybrid Sleep"),
                ..Default::default()
            },
        )
    }

    /// Local filesystems target
    pub fn local_fs() -> Target {
        Target::new(
            "local-fs.target",
            TargetConfig {
                description: String::from("Local File Systems"),
                after: vec![String::from("sysinit.target")],
                ..Default::default()
            },
        )
    }

    /// Remote filesystems target
    pub fn remote_fs() -> Target {
        Target::new(
            "remote-fs.target",
            TargetConfig {
                description: String::from("Remote File Systems"),
                after: vec![
                    String::from("local-fs.target"),
                    String::from("network-online.target"),
                ],
                ..Default::default()
            },
        )
    }

    /// Sound target
    pub fn sound() -> Target {
        Target::new(
            "sound.target",
            TargetConfig {
                description: String::from("Sound Card"),
                after: vec![String::from("sysinit.target")],
                ..Default::default()
            },
        )
    }

    /// Bluetooth target
    pub fn bluetooth() -> Target {
        Target::new(
            "bluetooth.target",
            TargetConfig {
                description: String::from("Bluetooth"),
                after: vec![String::from("sysinit.target")],
                ..Default::default()
            },
        )
    }

    /// Printer target
    pub fn printer() -> Target {
        Target::new(
            "printer.target",
            TargetConfig {
                description: String::from("Printer"),
                after: vec![String::from("basic.target")],
                ..Default::default()
            },
        )
    }

    /// Time sync target
    pub fn time_sync() -> Target {
        Target::new(
            "time-sync.target",
            TargetConfig {
                description: String::from("System Time Synchronized"),
                after: vec![String::from("network-online.target")],
                ..Default::default()
            },
        )
    }

    /// Get all predefined targets
    pub fn all() -> Vec<Target> {
        vec![
            sysinit(),
            basic(),
            network(),
            network_online(),
            multi_user(),
            graphical(),
            rescue(),
            emergency(),
            shutdown(),
            reboot(),
            poweroff(),
            halt(),
            suspend(),
            hibernate(),
            hybrid_sleep(),
            local_fs(),
            remote_fs(),
            sound(),
            bluetooth(),
            printer(),
            time_sync(),
        ]
    }
}
