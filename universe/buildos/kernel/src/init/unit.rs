//! BuildOS Unit System
//!
//! Base unit types and configuration for the init system.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use super::service::ServiceConfig;

/// Unique unit ID
pub type UnitId = u64;

/// Next unit ID
static NEXT_UNIT_ID: AtomicU64 = AtomicU64::new(1);

/// Generate a new unit ID
fn next_unit_id() -> UnitId {
    NEXT_UNIT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Unit types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitType {
    /// Service unit (daemon/process)
    Service,
    /// Socket unit (socket activation)
    Socket,
    /// Target unit (grouping/synchronization)
    Target,
    /// Device unit
    Device,
    /// Mount unit (filesystem mount)
    Mount,
    /// Automount unit
    Automount,
    /// Swap unit
    Swap,
    /// Timer unit (scheduled tasks)
    Timer,
    /// Path unit (path-based activation)
    Path,
    /// Slice unit (cgroup hierarchy)
    Slice,
    /// Scope unit (externally created processes)
    Scope,
}

impl UnitType {
    /// Get the file extension for this unit type
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Service => ".service",
            Self::Socket => ".socket",
            Self::Target => ".target",
            Self::Device => ".device",
            Self::Mount => ".mount",
            Self::Automount => ".automount",
            Self::Swap => ".swap",
            Self::Timer => ".timer",
            Self::Path => ".path",
            Self::Slice => ".slice",
            Self::Scope => ".scope",
        }
    }

    /// Parse unit type from name
    pub fn from_name(name: &str) -> Option<Self> {
        if name.ends_with(".service") { Some(Self::Service) }
        else if name.ends_with(".socket") { Some(Self::Socket) }
        else if name.ends_with(".target") { Some(Self::Target) }
        else if name.ends_with(".device") { Some(Self::Device) }
        else if name.ends_with(".mount") { Some(Self::Mount) }
        else if name.ends_with(".automount") { Some(Self::Automount) }
        else if name.ends_with(".swap") { Some(Self::Swap) }
        else if name.ends_with(".timer") { Some(Self::Timer) }
        else if name.ends_with(".path") { Some(Self::Path) }
        else if name.ends_with(".slice") { Some(Self::Slice) }
        else if name.ends_with(".scope") { Some(Self::Scope) }
        else { None }
    }
}

/// Unit state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitState {
    /// Unit is inactive
    Inactive,
    /// Unit is activating
    Activating,
    /// Unit is active
    Active,
    /// Unit is deactivating
    Deactivating,
    /// Unit failed
    Failed,
    /// Unit is reloading
    Reloading,
    /// Unit is in maintenance
    Maintenance,
}

impl UnitState {
    /// Check if the unit is running
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active | Self::Reloading)
    }

    /// Check if the unit is in transition
    pub fn is_in_transition(&self) -> bool {
        matches!(self, Self::Activating | Self::Deactivating | Self::Reloading)
    }
}

/// Unit load state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadState {
    /// Unit not loaded
    Stub,
    /// Unit loaded successfully
    Loaded,
    /// Unit not found
    NotFound,
    /// Unit has bad settings
    BadSetting,
    /// Error loading unit
    Error,
    /// Unit is masked
    Masked,
}

/// Common unit configuration
#[derive(Clone, Debug, Default)]
pub struct UnitConfig {
    /// Human-readable description
    pub description: String,
    /// Documentation URLs
    pub documentation: Vec<String>,

    // Dependencies
    /// Units that must be started before this one
    pub after: Vec<String>,
    /// Units that must be started after this one
    pub before: Vec<String>,
    /// Hard dependencies (fail if these fail)
    pub requires: Vec<String>,
    /// Soft dependencies (don't fail if these fail)
    pub wants: Vec<String>,
    /// Conflicting units (stop these when starting)
    pub conflicts: Vec<String>,

    // Conditions
    /// Only start if path exists
    pub condition_path_exists: Vec<String>,
    /// Only start if path is a directory
    pub condition_directory_not_empty: Vec<String>,
    /// Only start if kernel command line matches
    pub condition_kernel_command_line: Vec<String>,
    /// Only start on specific architectures
    pub condition_architecture: Vec<String>,
    /// Only start in specific virtualization
    pub condition_virtualization: Vec<String>,

    // Install section
    /// Targets that want this unit
    pub wanted_by: Vec<String>,
    /// Targets that require this unit
    pub required_by: Vec<String>,
    /// Alias names
    pub alias: Vec<String>,
}

/// A unit in the init system
#[derive(Clone)]
pub struct Unit {
    /// Unique ID
    pub id: UnitId,
    /// Unit name (e.g., "sshd.service")
    pub name: String,
    /// Unit type
    pub unit_type: UnitType,
    /// Common configuration
    pub config: UnitConfig,
    /// Load state
    pub load_state: LoadState,
    /// Current state
    pub state: UnitState,
    /// State change timestamp
    pub state_change_timestamp: u64,
    /// Activation timestamp
    pub active_enter_timestamp: u64,
    /// Deactivation timestamp
    pub active_exit_timestamp: u64,
    /// Number of times started
    pub n_restarts: u32,
    /// Type-specific data
    pub data: UnitData,
}

/// Type-specific unit data
#[derive(Clone)]
pub enum UnitData {
    /// Service data
    Service(ServiceConfig),
    /// Socket data
    Socket(SocketConfig),
    /// Target data
    Target(TargetData),
    /// Timer data
    Timer(TimerConfig),
    /// Mount data
    Mount(MountConfig),
    /// Path data
    Path(PathConfig),
    /// Device data
    Device(DeviceData),
    /// Swap data
    Swap(SwapConfig),
    /// Slice data
    Slice(SliceConfig),
    /// Scope data
    Scope(ScopeData),
    /// No specific data
    None,
}

/// Socket configuration
#[derive(Clone, Debug, Default)]
pub struct SocketConfig {
    /// Listen addresses
    pub listen_stream: Vec<String>,
    /// Listen datagrams
    pub listen_datagram: Vec<String>,
    /// Listen sequential packets
    pub listen_seq_packet: Vec<String>,
    /// Listen FIFO
    pub listen_fifo: Vec<String>,
    /// Accept connections
    pub accept: bool,
    /// Maximum connections
    pub max_connections: u32,
    /// Socket user
    pub socket_user: String,
    /// Socket group
    pub socket_group: String,
    /// Socket mode
    pub socket_mode: u32,
    /// Service to activate
    pub service: String,
}

/// Target data
#[derive(Clone, Debug, Default)]
pub struct TargetData {
    /// Is default target
    pub default: bool,
    /// Allow isolate
    pub allow_isolate: bool,
}

/// Timer configuration
#[derive(Clone, Debug, Default)]
pub struct TimerConfig {
    /// Time relative to boot
    pub on_boot_sec: u64,
    /// Time relative to activation
    pub on_active_sec: u64,
    /// Time relative to unit becoming active
    pub on_unit_active_sec: u64,
    /// Time relative to unit becoming inactive
    pub on_unit_inactive_sec: u64,
    /// Calendar time specification
    pub on_calendar: String,
    /// Accuracy (for batching)
    pub accuracy_sec: u64,
    /// Randomized delay
    pub randomized_delay_sec: u64,
    /// Persistent (survive reboot)
    pub persistent: bool,
    /// Wake system from suspend
    pub wake_system: bool,
    /// Unit to activate
    pub unit: String,
}

/// Mount configuration
#[derive(Clone, Debug, Default)]
pub struct MountConfig {
    /// What to mount
    pub what: String,
    /// Where to mount
    pub where_path: String,
    /// Filesystem type
    pub fs_type: String,
    /// Mount options
    pub options: String,
    /// Timeout for mounting
    pub timeout_sec: u64,
    /// Lazy unmount
    pub lazy_unmount: bool,
    /// Force unmount
    pub force_unmount: bool,
}

/// Path configuration
#[derive(Clone, Debug, Default)]
pub struct PathConfig {
    /// Watch for path existence
    pub path_exists: Vec<String>,
    /// Watch for path existence (and initial trigger)
    pub path_exists_glob: Vec<String>,
    /// Watch for changes
    pub path_changed: Vec<String>,
    /// Watch for modifications
    pub path_modified: Vec<String>,
    /// Watch for directory not empty
    pub directory_not_empty: Vec<String>,
    /// Make directory if needed
    pub make_directory: bool,
    /// Directory mode
    pub directory_mode: u32,
    /// Unit to activate
    pub unit: String,
}

/// Device data
#[derive(Clone, Debug, Default)]
pub struct DeviceData {
    /// System path
    pub sys_path: String,
    /// Device node
    pub dev_path: String,
}

/// Swap configuration
#[derive(Clone, Debug, Default)]
pub struct SwapConfig {
    /// What to mount
    pub what: String,
    /// Priority
    pub priority: i32,
    /// Options
    pub options: String,
    /// Timeout
    pub timeout_sec: u64,
}

/// Slice configuration
#[derive(Clone, Debug, Default)]
pub struct SliceConfig {
    /// CPU shares
    pub cpu_shares: u64,
    /// Memory limit
    pub memory_limit: u64,
    /// Task limit
    pub tasks_max: u64,
}

/// Scope data
#[derive(Clone, Debug, Default)]
pub struct ScopeData {
    /// PIDs in scope
    pub pids: Vec<u32>,
    /// Timeout for stopping
    pub timeout_stop_sec: u64,
}

impl Unit {
    /// Create a new service unit
    pub fn service(name: &str, config: ServiceConfig) -> Self {
        Self {
            id: next_unit_id(),
            name: String::from(name),
            unit_type: UnitType::Service,
            config: UnitConfig {
                description: config.description.clone(),
                after: config.after.clone(),
                wants: config.wants.clone(),
                requires: config.requires.clone(),
                ..Default::default()
            },
            load_state: LoadState::Loaded,
            state: UnitState::Inactive,
            state_change_timestamp: 0,
            active_enter_timestamp: 0,
            active_exit_timestamp: 0,
            n_restarts: 0,
            data: UnitData::Service(config),
        }
    }

    /// Create a new target unit
    pub fn target(name: &str, config: super::target::TargetConfig) -> Self {
        Self {
            id: next_unit_id(),
            name: String::from(name),
            unit_type: UnitType::Target,
            config: UnitConfig {
                description: config.description.clone(),
                requires: config.requires.clone(),
                wants: config.wants.clone(),
                conflicts: config.conflicts.clone(),
                ..Default::default()
            },
            load_state: LoadState::Loaded,
            state: UnitState::Inactive,
            state_change_timestamp: 0,
            active_enter_timestamp: 0,
            active_exit_timestamp: 0,
            n_restarts: 0,
            data: UnitData::Target(TargetData {
                default: config.default_target,
                allow_isolate: config.allow_isolate,
            }),
        }
    }

    /// Create a new timer unit
    pub fn timer(name: &str, config: TimerConfig) -> Self {
        Self {
            id: next_unit_id(),
            name: String::from(name),
            unit_type: UnitType::Timer,
            config: UnitConfig::default(),
            load_state: LoadState::Loaded,
            state: UnitState::Inactive,
            state_change_timestamp: 0,
            active_enter_timestamp: 0,
            active_exit_timestamp: 0,
            n_restarts: 0,
            data: UnitData::Timer(config),
        }
    }

    /// Get the unit name without extension
    pub fn base_name(&self) -> &str {
        let ext = self.unit_type.extension();
        if self.name.ends_with(ext) {
            &self.name[..self.name.len() - ext.len()]
        } else {
            &self.name
        }
    }

    /// Check if unit is active
    pub fn is_active(&self) -> bool {
        self.state.is_active()
    }

    /// Check if unit has failed
    pub fn is_failed(&self) -> bool {
        self.state == UnitState::Failed
    }

    /// Get all dependencies
    pub fn dependencies(&self) -> Vec<&String> {
        let mut deps = Vec::new();
        deps.extend(&self.config.requires);
        deps.extend(&self.config.wants);
        deps
    }

    /// Get hard dependencies only
    pub fn hard_dependencies(&self) -> &[String] {
        &self.config.requires
    }

    /// Check if this unit conflicts with another
    pub fn conflicts_with(&self, other: &str) -> bool {
        self.config.conflicts.iter().any(|c| c == other)
    }
}

/// Unit file parser
pub mod parser {
    use super::*;
    use alloc::collections::BTreeMap;

    /// Parse a unit file
    pub fn parse_unit_file(content: &str) -> Result<BTreeMap<String, BTreeMap<String, String>>, ParseError> {
        let mut sections: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        let mut current_section = String::new();

        for line in content.lines() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }

            // Section header
            if line.starts_with('[') && line.ends_with(']') {
                current_section = line[1..line.len()-1].to_string();
                sections.entry(current_section.clone()).or_insert_with(BTreeMap::new);
                continue;
            }

            // Key-value pair
            if let Some(eq_pos) = line.find('=') {
                let key = line[..eq_pos].trim().to_string();
                let value = line[eq_pos+1..].trim().to_string();

                if let Some(section) = sections.get_mut(&current_section) {
                    section.insert(key, value);
                }
            }
        }

        Ok(sections)
    }

    /// Parse error
    #[derive(Debug)]
    pub enum ParseError {
        InvalidFormat,
        MissingSection(String),
        InvalidValue(String),
    }
}
