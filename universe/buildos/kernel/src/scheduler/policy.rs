//! Scheduling Policy Management
//!
//! Implements scheduling policies, priority management, and nice values.

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use crate::process::Tid;

/// Scheduling policy
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchedPolicy {
    /// Normal time-sharing (CFS)
    Normal = 0,
    /// FIFO real-time
    Fifo = 1,
    /// Round-robin real-time
    RoundRobin = 2,
    /// Batch processing
    Batch = 3,
    /// Idle priority (only runs when nothing else)
    Idle = 5,
    /// Deadline scheduling
    Deadline = 6,
}

impl Default for SchedPolicy {
    fn default() -> Self {
        Self::Normal
    }
}

impl SchedPolicy {
    /// Parse from integer
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Normal),
            1 => Some(Self::Fifo),
            2 => Some(Self::RoundRobin),
            3 => Some(Self::Batch),
            5 => Some(Self::Idle),
            6 => Some(Self::Deadline),
            _ => None,
        }
    }

    /// Is this a real-time policy?
    pub fn is_realtime(&self) -> bool {
        matches!(self, Self::Fifo | Self::RoundRobin | Self::Deadline)
    }

    /// Is this a normal policy?
    pub fn is_normal(&self) -> bool {
        matches!(self, Self::Normal | Self::Batch | Self::Idle)
    }

    /// Is this deadline policy?
    pub fn is_deadline(&self) -> bool {
        matches!(self, Self::Deadline)
    }

    /// Get policy name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Normal => "SCHED_NORMAL",
            Self::Fifo => "SCHED_FIFO",
            Self::RoundRobin => "SCHED_RR",
            Self::Batch => "SCHED_BATCH",
            Self::Idle => "SCHED_IDLE",
            Self::Deadline => "SCHED_DEADLINE",
        }
    }
}

/// Nice value range
pub const NICE_MIN: i32 = -20;
pub const NICE_MAX: i32 = 19;
pub const NICE_DEFAULT: i32 = 0;

/// Real-time priority range
pub const RT_PRIO_MIN: u8 = 1;
pub const RT_PRIO_MAX: u8 = 99;

/// Priority weights for nice values (-20 to +19)
/// Higher weight = more CPU time
static NICE_TO_WEIGHT: [u64; 40] = [
    /* -20 */ 88761, 71755, 56483, 46273, 36291,
    /* -15 */ 29154, 23254, 18705, 14949, 11916,
    /* -10 */ 9548, 7620, 6100, 4904, 3906,
    /*  -5 */ 3121, 2501, 1991, 1586, 1277,
    /*   0 */ 1024, 820, 655, 526, 423,
    /*   5 */ 335, 272, 215, 172, 137,
    /*  10 */ 110, 87, 70, 56, 45,
    /*  15 */ 36, 29, 23, 18, 15,
];

/// Inverse weights for calculating vruntime
static NICE_TO_INV_WEIGHT: [u64; 40] = [
    /* -20 */ 48388, 59856, 76040, 92818, 118348,
    /* -15 */ 147320, 184698, 229616, 287308, 360437,
    /* -10 */ 449829, 563644, 704093, 875809, 1099582,
    /*  -5 */ 1376151, 1717300, 2157191, 2708050, 3363326,
    /*   0 */ 4194304, 5237765, 6557202, 8165337, 10153587,
    /*   5 */ 12820798, 15790321, 19976592, 24970740, 31350126,
    /*  10 */ 39045157, 49367440, 61356676, 76695844, 95443717,
    /*  15 */ 119304647, 148102320, 186737708, 238609294, 286331153,
];

/// Convert nice value to weight
pub fn nice_to_weight(nice: i32) -> u64 {
    let index = (nice + 20).clamp(0, 39) as usize;
    NICE_TO_WEIGHT[index]
}

/// Convert nice value to inverse weight
pub fn nice_to_inv_weight(nice: i32) -> u64 {
    let index = (nice + 20).clamp(0, 39) as usize;
    NICE_TO_INV_WEIGHT[index]
}

/// Convert weight to approximate nice value
pub fn weight_to_nice(weight: u64) -> i32 {
    for (i, &w) in NICE_TO_WEIGHT.iter().enumerate() {
        if weight >= w {
            return i as i32 - 20;
        }
    }
    NICE_MAX
}

/// Scheduling parameters
#[derive(Clone, Debug)]
pub struct SchedAttr {
    /// Size of this structure
    pub size: u32,
    /// Scheduling policy
    pub policy: SchedPolicy,
    /// Flags
    pub flags: SchedFlags,
    /// Nice value (for SCHED_NORMAL/BATCH)
    pub nice: i32,
    /// Real-time priority (for SCHED_FIFO/RR)
    pub rt_priority: u8,
    /// Deadline runtime (nanoseconds)
    pub dl_runtime: u64,
    /// Deadline (nanoseconds)
    pub dl_deadline: u64,
    /// Deadline period (nanoseconds)
    pub dl_period: u64,
    /// Utilization hint (0-1024)
    pub util_min: u32,
    /// Utilization hint (0-1024)
    pub util_max: u32,
}

impl Default for SchedAttr {
    fn default() -> Self {
        Self {
            size: core::mem::size_of::<Self>() as u32,
            policy: SchedPolicy::Normal,
            flags: SchedFlags::empty(),
            nice: NICE_DEFAULT,
            rt_priority: 0,
            dl_runtime: 0,
            dl_deadline: 0,
            dl_period: 0,
            util_min: 0,
            util_max: 1024,
        }
    }
}

impl SchedAttr {
    /// Create for real-time FIFO
    pub fn fifo(priority: u8) -> Self {
        Self {
            policy: SchedPolicy::Fifo,
            rt_priority: priority.clamp(RT_PRIO_MIN, RT_PRIO_MAX),
            ..Default::default()
        }
    }

    /// Create for real-time round-robin
    pub fn rr(priority: u8) -> Self {
        Self {
            policy: SchedPolicy::RoundRobin,
            rt_priority: priority.clamp(RT_PRIO_MIN, RT_PRIO_MAX),
            ..Default::default()
        }
    }

    /// Create for deadline
    pub fn deadline(runtime: u64, deadline: u64, period: u64) -> Self {
        Self {
            policy: SchedPolicy::Deadline,
            dl_runtime: runtime,
            dl_deadline: deadline,
            dl_period: period,
            ..Default::default()
        }
    }

    /// Create for batch
    pub fn batch(nice: i32) -> Self {
        Self {
            policy: SchedPolicy::Batch,
            nice: nice.clamp(NICE_MIN, NICE_MAX),
            ..Default::default()
        }
    }

    /// Create for idle
    pub fn idle() -> Self {
        Self {
            policy: SchedPolicy::Idle,
            ..Default::default()
        }
    }

    /// Validate parameters
    pub fn validate(&self) -> Result<(), PolicyError> {
        match self.policy {
            SchedPolicy::Normal | SchedPolicy::Batch => {
                if self.nice < NICE_MIN || self.nice > NICE_MAX {
                    return Err(PolicyError::InvalidNice);
                }
            }
            SchedPolicy::Fifo | SchedPolicy::RoundRobin => {
                if self.rt_priority < RT_PRIO_MIN || self.rt_priority > RT_PRIO_MAX {
                    return Err(PolicyError::InvalidPriority);
                }
            }
            SchedPolicy::Deadline => {
                if self.dl_runtime == 0 || self.dl_deadline == 0 || self.dl_period == 0 {
                    return Err(PolicyError::InvalidDeadline);
                }
                if self.dl_runtime > self.dl_deadline {
                    return Err(PolicyError::InvalidDeadline);
                }
                if self.dl_deadline > self.dl_period {
                    return Err(PolicyError::InvalidDeadline);
                }
            }
            SchedPolicy::Idle => {}
        }
        Ok(())
    }

    /// Get weight for this scheduling attribute
    pub fn weight(&self) -> u64 {
        match self.policy {
            SchedPolicy::Normal | SchedPolicy::Batch => nice_to_weight(self.nice),
            SchedPolicy::Idle => 3, // Very low weight
            _ => 1024, // Default for RT
        }
    }
}

bitflags::bitflags! {
    /// Scheduling flags
    #[derive(Clone, Copy, Debug, Default)]
    pub struct SchedFlags: u64 {
        /// Reset on fork
        const RESET_ON_FORK = 0x01;
        /// Reclaim unused bandwidth
        const RECLAIM = 0x02;
        /// Use deadline inheritance
        const DL_OVERRUN = 0x04;
        /// Keep utilization clamps on exec
        const KEEP_POLICY = 0x08;
        /// Keep nice on exec
        const KEEP_PARAMS = 0x10;
        /// Utilization clamping
        const UTIL_CLAMP_MIN = 0x20;
        const UTIL_CLAMP_MAX = 0x40;
    }
}

/// Policy errors
#[derive(Clone, Debug)]
pub enum PolicyError {
    /// Invalid nice value
    InvalidNice,
    /// Invalid RT priority
    InvalidPriority,
    /// Invalid deadline parameters
    InvalidDeadline,
    /// Permission denied
    PermissionDenied,
    /// Operation not supported
    NotSupported,
    /// Resource limit exceeded
    LimitExceeded,
}

/// Resource limits for scheduling
#[derive(Clone, Debug)]
pub struct SchedLimits {
    /// Maximum real-time priority allowed
    pub rtprio_max: u8,
    /// Maximum nice (lower bound, e.g., -20)
    pub nice_max: i32,
    /// Can use deadline scheduling
    pub can_deadline: bool,
    /// Maximum CPU percentage
    pub cpu_max_pct: u32,
}

impl Default for SchedLimits {
    fn default() -> Self {
        Self {
            rtprio_max: 0, // No RT access by default
            nice_max: 0,   // Can only go lower priority
            can_deadline: false,
            cpu_max_pct: 100,
        }
    }
}

impl SchedLimits {
    /// Root limits (no restrictions)
    pub fn root() -> Self {
        Self {
            rtprio_max: RT_PRIO_MAX,
            nice_max: NICE_MIN,
            can_deadline: true,
            cpu_max_pct: 100,
        }
    }

    /// Check if policy change is allowed
    pub fn check(&self, attr: &SchedAttr) -> Result<(), PolicyError> {
        match attr.policy {
            SchedPolicy::Normal | SchedPolicy::Batch => {
                if attr.nice < self.nice_max {
                    return Err(PolicyError::PermissionDenied);
                }
            }
            SchedPolicy::Fifo | SchedPolicy::RoundRobin => {
                if attr.rt_priority > self.rtprio_max {
                    return Err(PolicyError::PermissionDenied);
                }
            }
            SchedPolicy::Deadline => {
                if !self.can_deadline {
                    return Err(PolicyError::PermissionDenied);
                }
            }
            SchedPolicy::Idle => {} // Always allowed
        }
        Ok(())
    }
}

/// Autogroup for automatic session grouping
pub struct PolicyAutogroup {
    /// Session groups
    groups: BTreeMap<u32, AutogroupEntry>,
    /// Task to group mapping
    task_groups: BTreeMap<Tid, u32>,
    /// Next group ID
    next_id: u32,
    /// Enabled
    enabled: bool,
}

/// Autogroup entry
#[derive(Clone, Debug)]
pub struct AutogroupEntry {
    /// Group ID (session ID)
    pub id: u32,
    /// Nice value for the group
    pub nice: i32,
    /// Member tasks
    pub tasks: Vec<Tid>,
    /// Effective weight
    pub weight: u64,
}

impl PolicyAutogroup {
    /// Create new autogroup manager
    pub fn new() -> Self {
        Self {
            groups: BTreeMap::new(),
            task_groups: BTreeMap::new(),
            next_id: 1,
            enabled: true,
        }
    }

    /// Enable/disable autogrouping
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Is autogrouping enabled?
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Create or get group for session
    pub fn get_or_create(&mut self, session: u32) -> &mut AutogroupEntry {
        if !self.groups.contains_key(&session) {
            let entry = AutogroupEntry {
                id: session,
                nice: NICE_DEFAULT,
                tasks: Vec::new(),
                weight: nice_to_weight(NICE_DEFAULT),
            };
            self.groups.insert(session, entry);
        }
        self.groups.get_mut(&session).unwrap()
    }

    /// Add task to group
    pub fn add_task(&mut self, tid: Tid, session: u32) {
        // Remove from old group
        if let Some(old_session) = self.task_groups.remove(&tid) {
            if let Some(group) = self.groups.get_mut(&old_session) {
                group.tasks.retain(|&t| t != tid);
            }
        }

        // Add to new group
        let group = self.get_or_create(session);
        if !group.tasks.contains(&tid) {
            group.tasks.push(tid);
        }
        self.task_groups.insert(tid, session);
    }

    /// Remove task from groups
    pub fn remove_task(&mut self, tid: Tid) {
        if let Some(session) = self.task_groups.remove(&tid) {
            if let Some(group) = self.groups.get_mut(&session) {
                group.tasks.retain(|&t| t != tid);
            }
        }
    }

    /// Set group nice value
    pub fn set_nice(&mut self, session: u32, nice: i32) {
        if let Some(group) = self.groups.get_mut(&session) {
            group.nice = nice.clamp(NICE_MIN, NICE_MAX);
            group.weight = nice_to_weight(group.nice);
        }
    }

    /// Get group for task
    pub fn get_group(&self, tid: Tid) -> Option<&AutogroupEntry> {
        self.task_groups.get(&tid)
            .and_then(|session| self.groups.get(session))
    }

    /// Get effective weight for task
    pub fn effective_weight(&self, tid: Tid, task_weight: u64) -> u64 {
        if !self.enabled {
            return task_weight;
        }

        if let Some(group) = self.get_group(tid) {
            // Combine group and task weights
            (group.weight * task_weight) / 1024
        } else {
            task_weight
        }
    }
}

/// IO priority
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoPriority {
    /// Real-time IO
    RealTime(u8), // 0-7
    /// Best effort
    BestEffort(u8), // 0-7
    /// Idle IO (only when nothing else)
    Idle,
    /// No priority set (inherit from scheduling)
    None,
}

impl Default for IoPriority {
    fn default() -> Self {
        Self::None
    }
}

impl IoPriority {
    /// Create from class and data
    pub fn from_class_data(class: u8, data: u8) -> Self {
        match class {
            1 => Self::RealTime(data & 0x07),
            2 => Self::BestEffort(data & 0x07),
            3 => Self::Idle,
            _ => Self::None,
        }
    }

    /// Convert to class and data
    pub fn to_class_data(&self) -> (u8, u8) {
        match self {
            Self::RealTime(d) => (1, *d),
            Self::BestEffort(d) => (2, *d),
            Self::Idle => (3, 0),
            Self::None => (0, 0),
        }
    }

    /// Derive from nice value
    pub fn from_nice(nice: i32) -> Self {
        // Map nice -20..19 to BE priority 0..7
        let prio = ((nice + 20) * 8 / 40).min(7) as u8;
        Self::BestEffort(prio)
    }
}

/// CPU bandwidth controller
pub struct BandwidthController {
    /// Per-task bandwidth limits
    limits: BTreeMap<Tid, BandwidthLimit>,
    /// Global RT bandwidth
    rt_runtime: u64,
    rt_period: u64,
}

/// Bandwidth limit for a task
#[derive(Clone, Debug)]
pub struct BandwidthLimit {
    /// Runtime allowed per period (nanoseconds)
    pub runtime: u64,
    /// Period (nanoseconds)
    pub period: u64,
    /// Used runtime this period
    pub used: u64,
    /// Period start time
    pub period_start: u64,
}

impl BandwidthController {
    /// Create new bandwidth controller
    pub fn new() -> Self {
        Self {
            limits: BTreeMap::new(),
            rt_runtime: 950_000_000, // 950ms per 1s (95%)
            rt_period: 1_000_000_000,
        }
    }

    /// Set global RT bandwidth
    pub fn set_rt_bandwidth(&mut self, runtime: u64, period: u64) {
        self.rt_runtime = runtime;
        self.rt_period = period;
    }

    /// Set per-task limit
    pub fn set_limit(&mut self, tid: Tid, runtime: u64, period: u64) {
        self.limits.insert(tid, BandwidthLimit {
            runtime,
            period,
            used: 0,
            period_start: 0,
        });
    }

    /// Remove limit
    pub fn remove_limit(&mut self, tid: Tid) {
        self.limits.remove(&tid);
    }

    /// Account runtime
    pub fn account(&mut self, tid: Tid, runtime: u64, now: u64) -> bool {
        if let Some(limit) = self.limits.get_mut(&tid) {
            // Check if period expired
            if now >= limit.period_start + limit.period {
                limit.used = 0;
                limit.period_start = now;
            }

            limit.used += runtime;
            limit.used < limit.runtime
        } else {
            true // No limit
        }
    }

    /// Is task throttled?
    pub fn is_throttled(&self, tid: Tid) -> bool {
        self.limits.get(&tid)
            .map(|l| l.used >= l.runtime)
            .unwrap_or(false)
    }
}

/// Scheduling statistics
#[derive(Clone, Debug, Default)]
pub struct SchedStats {
    /// Total time running
    pub run_time: u64,
    /// Total time waiting (runnable but not running)
    pub wait_time: u64,
    /// Number of context switches
    pub nr_switches: u64,
    /// Number of voluntary context switches
    pub nr_voluntary_switches: u64,
    /// Number of involuntary switches
    pub nr_involuntary_switches: u64,
    /// Number of migrations
    pub nr_migrations: u64,
    /// Total slices run
    pub nr_slices: u64,
    /// Last arrival time
    pub last_arrival: u64,
    /// Last queued time
    pub last_queued: u64,
}

impl SchedStats {
    /// Record arrival (became runnable)
    pub fn arrival(&mut self, now: u64) {
        self.last_arrival = now;
    }

    /// Record being queued (waiting in run queue)
    pub fn queued(&mut self, now: u64) {
        self.last_queued = now;
    }

    /// Record running (selected to run)
    pub fn running(&mut self, now: u64) {
        if self.last_queued > 0 {
            self.wait_time += now - self.last_queued;
        }
        self.nr_slices += 1;
    }

    /// Record stopping (finished running)
    pub fn stopped(&mut self, now: u64, voluntary: bool) {
        if self.last_arrival > 0 {
            self.run_time += now - self.last_arrival;
        }
        self.nr_switches += 1;
        if voluntary {
            self.nr_voluntary_switches += 1;
        } else {
            self.nr_involuntary_switches += 1;
        }
    }

    /// Record migration
    pub fn migrated(&mut self) {
        self.nr_migrations += 1;
    }
}

/// Policy manager combining all policy features
pub struct PolicyManager {
    /// Per-task attributes
    attrs: BTreeMap<Tid, SchedAttr>,
    /// Per-task limits
    limits: BTreeMap<Tid, SchedLimits>,
    /// Per-task IO priority
    ioprio: BTreeMap<Tid, IoPriority>,
    /// Per-task statistics
    stats: BTreeMap<Tid, SchedStats>,
    /// Autogroups
    autogroups: PolicyAutogroup,
    /// Bandwidth controller
    bandwidth: BandwidthController,
}

impl PolicyManager {
    /// Create new policy manager
    pub fn new() -> Self {
        Self {
            attrs: BTreeMap::new(),
            limits: BTreeMap::new(),
            ioprio: BTreeMap::new(),
            stats: BTreeMap::new(),
            autogroups: PolicyAutogroup::new(),
            bandwidth: BandwidthController::new(),
        }
    }

    /// Register a new task
    pub fn register(&mut self, tid: Tid, attr: SchedAttr) {
        self.attrs.insert(tid, attr);
        self.limits.insert(tid, SchedLimits::default());
        self.stats.insert(tid, SchedStats::default());
    }

    /// Unregister a task
    pub fn unregister(&mut self, tid: Tid) {
        self.attrs.remove(&tid);
        self.limits.remove(&tid);
        self.ioprio.remove(&tid);
        self.stats.remove(&tid);
        self.autogroups.remove_task(tid);
        self.bandwidth.remove_limit(tid);
    }

    /// Get task attributes
    pub fn get_attr(&self, tid: Tid) -> Option<&SchedAttr> {
        self.attrs.get(&tid)
    }

    /// Set task attributes
    pub fn set_attr(&mut self, tid: Tid, attr: SchedAttr) -> Result<(), PolicyError> {
        attr.validate()?;

        if let Some(limits) = self.limits.get(&tid) {
            limits.check(&attr)?;
        }

        self.attrs.insert(tid, attr);
        Ok(())
    }

    /// Get task limits
    pub fn get_limits(&self, tid: Tid) -> Option<&SchedLimits> {
        self.limits.get(&tid)
    }

    /// Set task limits (admin only)
    pub fn set_limits(&mut self, tid: Tid, limits: SchedLimits) {
        self.limits.insert(tid, limits);
    }

    /// Get task IO priority
    pub fn get_ioprio(&self, tid: Tid) -> IoPriority {
        self.ioprio.get(&tid).copied().unwrap_or_default()
    }

    /// Set task IO priority
    pub fn set_ioprio(&mut self, tid: Tid, prio: IoPriority) {
        self.ioprio.insert(tid, prio);
    }

    /// Get task statistics
    pub fn get_stats(&self, tid: Tid) -> Option<&SchedStats> {
        self.stats.get(&tid)
    }

    /// Get mutable task statistics
    pub fn get_stats_mut(&mut self, tid: Tid) -> Option<&mut SchedStats> {
        self.stats.get_mut(&tid)
    }

    /// Get autogroup manager
    pub fn autogroups(&self) -> &PolicyAutogroup {
        &self.autogroups
    }

    /// Get mutable autogroup manager
    pub fn autogroups_mut(&mut self) -> &mut PolicyAutogroup {
        &mut self.autogroups
    }

    /// Get bandwidth controller
    pub fn bandwidth(&self) -> &BandwidthController {
        &self.bandwidth
    }

    /// Get mutable bandwidth controller
    pub fn bandwidth_mut(&mut self) -> &mut BandwidthController {
        &mut self.bandwidth
    }
}
