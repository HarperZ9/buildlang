// ===============================================================================
// BUILDOS KERNEL - OUT OF MEMORY (OOM) KILLER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! Out of Memory (OOM) Killer and Memory Pressure Handling
//!
//! This module provides:
//! - OOM killer for emergency memory reclamation
//! - Memory pressure notifications
//! - Per-process OOM score calculation
//! - Memory reclaim policies
//! - Cgroup-aware OOM handling

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::AtomicU64;

use crate::process::Pid;
use crate::sync::RwLock;

// =============================================================================
// CONSTANTS
// =============================================================================

/// OOM score range (Linux compatible)
pub const OOM_SCORE_MIN: i64 = -1000;
pub const OOM_SCORE_MAX: i64 = 1000;

/// OOM score adjustment range
pub const OOM_SCORE_ADJ_MIN: i64 = -1000;
pub const OOM_SCORE_ADJ_MAX: i64 = 1000;

/// Special value to disable OOM killing for a process
pub const OOM_SCORE_ADJ_DISABLE: i64 = -1000;

/// Memory pressure thresholds (percentage of total memory)
pub const PRESSURE_LOW_THRESHOLD: u64 = 80;    // 80% used
pub const PRESSURE_MEDIUM_THRESHOLD: u64 = 90; // 90% used
pub const PRESSURE_CRITICAL_THRESHOLD: u64 = 95; // 95% used

/// Minimum pages to free before considering OOM resolved
pub const OOM_MIN_FREE_PAGES: usize = 256;  // 1MB for 4KB pages

/// Time between OOM kills (ms) to prevent rapid kills
pub const OOM_KILL_INTERVAL_MS: u64 = 1000;

// =============================================================================
// MEMORY PRESSURE LEVELS
// =============================================================================

/// Memory pressure level
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum MemoryPressure {
    /// No pressure - plenty of free memory
    None = 0,
    /// Low pressure - some allocation delays
    Low = 1,
    /// Medium pressure - significant reclaim activity
    Medium = 2,
    /// Critical pressure - approaching OOM
    Critical = 3,
    /// OOM - out of memory, must kill processes
    Oom = 4,
}

impl Default for MemoryPressure {
    fn default() -> Self {
        MemoryPressure::None
    }
}

// =============================================================================
// OOM CONTROL FLAGS
// =============================================================================

/// OOM control flags for processes
#[derive(Clone, Copy, Debug)]
pub struct OomControl {
    /// OOM score adjustment (-1000 to 1000)
    pub score_adj: i64,
    /// OOM killing disabled for this process
    pub oom_kill_disable: bool,
    /// Process is an OOM victim (being killed)
    pub oom_victim: bool,
    /// Process requested OOM reaper
    pub oom_reaper_requested: bool,
}

impl Default for OomControl {
    fn default() -> Self {
        Self {
            score_adj: 0,
            oom_kill_disable: false,
            oom_victim: false,
            oom_reaper_requested: false,
        }
    }
}

impl OomControl {
    /// Create OOM-immune control (for essential processes)
    pub fn oom_immune() -> Self {
        Self {
            score_adj: OOM_SCORE_ADJ_DISABLE,
            oom_kill_disable: true,
            oom_victim: false,
            oom_reaper_requested: false,
        }
    }

    /// Create high-priority victim (for memory hog processes)
    pub fn high_priority_victim() -> Self {
        Self {
            score_adj: OOM_SCORE_ADJ_MAX,
            oom_kill_disable: false,
            oom_victim: false,
            oom_reaper_requested: false,
        }
    }
}

// =============================================================================
// OOM POLICY
// =============================================================================

/// OOM policy determines how victims are selected
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OomPolicy {
    /// Kill the process with highest OOM score
    Standard,
    /// Kill processes in FIFO order
    Fifo,
    /// Kill processes with most memory first
    MemoryHog,
    /// Kill youngest processes first
    Youngest,
    /// Kill oldest processes first
    Oldest,
    /// Panic instead of killing processes
    Panic,
}

impl Default for OomPolicy {
    fn default() -> Self {
        OomPolicy::Standard
    }
}

// =============================================================================
// OOM CONTEXT
// =============================================================================

/// Context for an OOM event
#[derive(Clone, Debug)]
pub struct OomContext {
    /// Triggering allocation order
    pub order: u32,
    /// Requested allocation size
    pub requested_pages: usize,
    /// GFP flags from allocation
    pub gfp_mask: u32,
    /// Cgroup constraint (if any)
    pub cgroup_path: Option<String>,
    /// NUMA node constraint (if any)
    pub numa_node: Option<u32>,
    /// Current memory pressure level
    pub pressure: MemoryPressure,
    /// Total memory (pages)
    pub total_pages: usize,
    /// Free memory (pages)
    pub free_pages: usize,
    /// Reclaimable memory (pages)
    pub reclaimable_pages: usize,
}

impl OomContext {
    /// Create a new OOM context
    pub fn new(requested_pages: usize) -> Self {
        // Would read actual memory stats
        Self {
            order: 0,
            requested_pages,
            gfp_mask: 0,
            cgroup_path: None,
            numa_node: None,
            pressure: MemoryPressure::Oom,
            total_pages: 0,
            free_pages: 0,
            reclaimable_pages: 0,
        }
    }

    /// Check if we're under cgroup constraint
    pub fn is_cgroup_oom(&self) -> bool {
        self.cgroup_path.is_some()
    }

    /// Get available memory for OOM calculation
    pub fn available_pages(&self) -> usize {
        self.free_pages + self.reclaimable_pages
    }
}

// =============================================================================
// OOM VICTIM
// =============================================================================

/// Information about an OOM victim
#[derive(Clone, Debug)]
pub struct OomVictim {
    /// Process ID
    pub pid: Pid,
    /// Process name
    pub name: String,
    /// OOM score (0-1000)
    pub oom_score: i64,
    /// RSS memory in pages
    pub rss_pages: usize,
    /// Page tables memory
    pub pgtable_pages: usize,
    /// Swap usage in pages
    pub swap_pages: usize,
    /// OOM score adjustment
    pub score_adj: i64,
    /// User ID
    pub uid: u32,
    /// Cgroup path
    pub cgroup: String,
    /// Was selected as victim
    pub selected: bool,
    /// Kill timestamp
    pub kill_time: u64,
}

impl OomVictim {
    /// Create from process info
    pub fn from_process(pid: Pid, name: String, rss: usize, score_adj: i64) -> Self {
        Self {
            pid,
            name,
            oom_score: 0, // Calculated later
            rss_pages: rss,
            pgtable_pages: 0,
            swap_pages: 0,
            score_adj,
            uid: 0,
            cgroup: "/".to_string(),
            selected: false,
            kill_time: 0,
        }
    }

    /// Calculate OOM score (0-1000)
    pub fn calculate_score(&mut self, total_pages: usize) {
        if self.score_adj == OOM_SCORE_ADJ_DISABLE {
            self.oom_score = 0;
            return;
        }

        // Base score is percentage of memory used (0-1000)
        let mem_usage = self.rss_pages + self.swap_pages + self.pgtable_pages;
        let base_score = if total_pages > 0 {
            (mem_usage as i64 * 1000) / total_pages as i64
        } else {
            0
        };

        // Apply adjustment
        let adj_score = base_score + self.score_adj;

        // Clamp to valid range
        self.oom_score = adj_score.clamp(0, OOM_SCORE_MAX);
    }

    /// Total memory usage in pages
    pub fn total_memory(&self) -> usize {
        self.rss_pages + self.swap_pages + self.pgtable_pages
    }
}

// =============================================================================
// OOM KILLER STATE
// =============================================================================

/// Global OOM killer state
static OOM_STATE: RwLock<OomState> = RwLock::new(OomState::new());

/// OOM killer state
pub struct OomState {
    /// Current memory pressure
    pressure: MemoryPressure,
    /// OOM policy
    policy: OomPolicy,
    /// Panic on OOM instead of killing
    panic_on_oom: bool,
    /// OOM killer enabled
    enabled: bool,
    /// Last OOM kill timestamp
    last_kill_time: u64,
    /// Total OOM kills
    kill_count: u64,
    /// Recent victims (for logging)
    recent_victims: Vec<OomVictim>,
    /// OOM control per process
    process_control: BTreeMap<Pid, OomControl>,
    /// Pressure listeners (PIDs)
    pressure_listeners: Vec<Pid>,
    /// Memory reclaim in progress
    reclaim_in_progress: bool,
}

impl OomState {
    const fn new() -> Self {
        Self {
            pressure: MemoryPressure::None,
            policy: OomPolicy::Standard,
            panic_on_oom: false,
            enabled: true,
            last_kill_time: 0,
            kill_count: 0,
            recent_victims: Vec::new(),
            process_control: BTreeMap::new(),
            pressure_listeners: Vec::new(),
            reclaim_in_progress: false,
        }
    }
}

// =============================================================================
// MEMORY PRESSURE STALL INFORMATION (PSI)
// =============================================================================

/// Pressure Stall Information (PSI) metrics
#[derive(Clone, Debug, Default)]
pub struct PressureStats {
    /// Time spent with some tasks stalled (us)
    pub some_total: u64,
    /// Time spent with all tasks stalled (us)
    pub full_total: u64,
    /// Average percentage stalled (last 10s)
    pub avg10: f32,
    /// Average percentage stalled (last 60s)
    pub avg60: f32,
    /// Average percentage stalled (last 300s)
    pub avg300: f32,
}

impl PressureStats {
    /// Format as Linux PSI format
    pub fn format(&self) -> String {
        alloc::format!(
            "some avg10={:.2} avg60={:.2} avg300={:.2} total={}\n\
             full avg10={:.2} avg60={:.2} avg300={:.2} total={}",
            self.avg10, self.avg60, self.avg300, self.some_total,
            self.avg10 / 2.0, self.avg60 / 2.0, self.avg300 / 2.0, self.full_total
        )
    }
}

/// Per-CPU pressure accounting
static PRESSURE_CPU: [AtomicU64; 256] = {
    const INIT: AtomicU64 = AtomicU64::new(0);
    [INIT; 256]
};

/// Global pressure stats
static MEMORY_PSI: RwLock<PressureStats> = RwLock::new(PressureStats {
    some_total: 0,
    full_total: 0,
    avg10: 0.0,
    avg60: 0.0,
    avg300: 0.0,
});

// =============================================================================
// RECLAIM STATE
// =============================================================================

/// Memory reclaim priority
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReclaimPriority {
    /// Low priority background reclaim
    Low = 0,
    /// Normal reclaim for allocations
    Normal = 1,
    /// High priority direct reclaim
    High = 2,
    /// Emergency reclaim before OOM
    Emergency = 3,
}

/// Reclaim statistics
#[derive(Clone, Debug, Default)]
pub struct ReclaimStats {
    /// Pages scanned
    pub scanned: u64,
    /// Pages reclaimed
    pub reclaimed: u64,
    /// Reclaim efficiency (reclaimed/scanned)
    pub efficiency: f32,
    /// Direct reclaim events
    pub direct_reclaim: u64,
    /// Background reclaim events
    pub kswapd_reclaim: u64,
    /// Pages written back
    pub writeback: u64,
}

// =============================================================================
// PUBLIC INTERFACE
// =============================================================================

/// Initialize OOM killer
pub fn init() {
    let mut state = OOM_STATE.write();
    state.enabled = true;
    state.policy = OomPolicy::Standard;

    crate::kprintln!("[OOM] OOM killer initialized");
}

/// Set OOM score adjustment for a process
pub fn set_oom_score_adj(pid: Pid, adj: i64) -> Result<(), OomError> {
    if adj < OOM_SCORE_ADJ_MIN || adj > OOM_SCORE_ADJ_MAX {
        return Err(OomError::InvalidArgument);
    }

    let mut state = OOM_STATE.write();
    let control = state.process_control.entry(pid).or_insert_with(OomControl::default);
    control.score_adj = adj;
    control.oom_kill_disable = adj == OOM_SCORE_ADJ_DISABLE;

    Ok(())
}

/// Get OOM score adjustment for a process
pub fn get_oom_score_adj(pid: Pid) -> i64 {
    OOM_STATE.read()
        .process_control
        .get(&pid)
        .map(|c| c.score_adj)
        .unwrap_or(0)
}

/// Get calculated OOM score for a process
pub fn get_oom_score(pid: Pid) -> i64 {
    // Would get process memory info and calculate
    let adj = get_oom_score_adj(pid);

    // Simplified: just return adjustment + 500 as base
    (adj + 500).clamp(0, OOM_SCORE_MAX)
}

/// Disable OOM killing for a process
pub fn set_oom_kill_disable(pid: Pid, disable: bool) -> Result<(), OomError> {
    let mut state = OOM_STATE.write();
    let control = state.process_control.entry(pid).or_insert_with(OomControl::default);
    control.oom_kill_disable = disable;

    if disable {
        control.score_adj = OOM_SCORE_ADJ_DISABLE;
    }

    Ok(())
}

/// Check if OOM killing is disabled for a process
pub fn is_oom_kill_disabled(pid: Pid) -> bool {
    OOM_STATE.read()
        .process_control
        .get(&pid)
        .map(|c| c.oom_kill_disable)
        .unwrap_or(false)
}

/// Set OOM policy
pub fn set_policy(policy: OomPolicy) {
    OOM_STATE.write().policy = policy;
}

/// Get current OOM policy
pub fn get_policy() -> OomPolicy {
    OOM_STATE.read().policy
}

/// Enable/disable panic on OOM
pub fn set_panic_on_oom(panic: bool) {
    OOM_STATE.write().panic_on_oom = panic;
}

/// Get current memory pressure level
pub fn get_pressure() -> MemoryPressure {
    OOM_STATE.read().pressure
}

/// Update memory pressure level
pub fn update_pressure(free_percent: u64) {
    let pressure = if free_percent < (100 - PRESSURE_CRITICAL_THRESHOLD) {
        MemoryPressure::Critical
    } else if free_percent < (100 - PRESSURE_MEDIUM_THRESHOLD) {
        MemoryPressure::Medium
    } else if free_percent < (100 - PRESSURE_LOW_THRESHOLD) {
        MemoryPressure::Low
    } else {
        MemoryPressure::None
    };

    let mut state = OOM_STATE.write();
    let old_pressure = state.pressure;
    state.pressure = pressure;

    // Notify listeners if pressure changed
    if pressure != old_pressure {
        drop(state);
        notify_pressure_change(pressure);
    }
}

/// Register for memory pressure notifications
pub fn register_pressure_listener(pid: Pid) {
    OOM_STATE.write().pressure_listeners.push(pid);
}

/// Unregister from pressure notifications
pub fn unregister_pressure_listener(pid: Pid) {
    OOM_STATE.write().pressure_listeners.retain(|&p| p != pid);
}

/// Notify listeners of pressure change
fn notify_pressure_change(pressure: MemoryPressure) {
    let listeners = OOM_STATE.read().pressure_listeners.clone();

    for pid in listeners {
        // Would send signal to process
        let _ = (pid, pressure);
    }
}

/// Try to reclaim memory before OOM
pub fn try_reclaim(pages_needed: usize, priority: ReclaimPriority) -> usize {
    let mut state = OOM_STATE.write();

    if state.reclaim_in_progress {
        return 0;
    }

    state.reclaim_in_progress = true;
    drop(state);

    let mut reclaimed = 0;

    // Try different reclaim strategies based on priority
    match priority {
        ReclaimPriority::Low => {
            // Just shrink caches
            reclaimed += shrink_slab_caches();
        }
        ReclaimPriority::Normal => {
            reclaimed += shrink_slab_caches();
            reclaimed += reclaim_page_cache(pages_needed);
        }
        ReclaimPriority::High => {
            reclaimed += shrink_slab_caches();
            reclaimed += reclaim_page_cache(pages_needed);
            reclaimed += reclaim_inactive_pages(pages_needed);
        }
        ReclaimPriority::Emergency => {
            reclaimed += shrink_slab_caches();
            reclaimed += reclaim_page_cache(pages_needed);
            reclaimed += reclaim_inactive_pages(pages_needed);
            reclaimed += drop_caches();
        }
    }

    OOM_STATE.write().reclaim_in_progress = false;

    reclaimed
}

/// Shrink slab caches
fn shrink_slab_caches() -> usize {
    // Would iterate slab caches and shrink
    // For now, simulate some reclaim
    0
}

/// Reclaim page cache pages
fn reclaim_page_cache(target: usize) -> usize {
    // Would reclaim clean page cache pages
    let _ = target;
    0
}

/// Reclaim inactive anonymous pages
fn reclaim_inactive_pages(target: usize) -> usize {
    // Would swap out inactive pages
    let _ = target;
    0
}

/// Drop all clean caches
fn drop_caches() -> usize {
    // Would drop all reclaimable caches
    0
}

/// Invoke the OOM killer
pub fn oom_kill(context: &OomContext) -> Result<Pid, OomError> {
    let state = OOM_STATE.read();

    if !state.enabled {
        return Err(OomError::Disabled);
    }

    if state.panic_on_oom {
        panic!("Out of memory! Policy set to panic.");
    }

    // Check kill rate limiting
    let now = current_time_ms();
    if now - state.last_kill_time < OOM_KILL_INTERVAL_MS {
        return Err(OomError::TooFrequent);
    }

    drop(state);

    // Select victim based on policy
    let victim = select_victim(context)?;

    // Kill the victim
    do_oom_kill(&victim)?;

    // Update state
    let mut state = OOM_STATE.write();
    state.last_kill_time = now;
    state.kill_count += 1;
    state.recent_victims.push(victim.clone());

    // Keep only last 10 victims
    while state.recent_victims.len() > 10 {
        state.recent_victims.remove(0);
    }

    crate::kprintln!(
        "[OOM] Killed process {} ({}), score={}, freed ~{} pages",
        victim.pid, victim.name, victim.oom_score, victim.rss_pages
    );

    Ok(victim.pid)
}

/// Select an OOM victim
fn select_victim(context: &OomContext) -> Result<OomVictim, OomError> {
    let state = OOM_STATE.read();
    let policy = state.policy;
    drop(state);

    // Get list of candidate processes
    let mut candidates = list_oom_candidates(context);

    if candidates.is_empty() {
        return Err(OomError::NoVictim);
    }

    // Calculate OOM scores
    for candidate in &mut candidates {
        candidate.calculate_score(context.total_pages);
    }

    // Sort based on policy
    match policy {
        OomPolicy::Standard => {
            // Highest OOM score first
            candidates.sort_by(|a, b| b.oom_score.cmp(&a.oom_score));
        }
        OomPolicy::MemoryHog => {
            // Most memory first
            candidates.sort_by(|a, b| b.total_memory().cmp(&a.total_memory()));
        }
        OomPolicy::Youngest => {
            // Would sort by start time (youngest first)
            candidates.sort_by(|a, b| b.pid.cmp(&a.pid));
        }
        OomPolicy::Oldest => {
            // Would sort by start time (oldest first)
            candidates.sort_by(|a, b| a.pid.cmp(&b.pid));
        }
        OomPolicy::Fifo => {
            // Keep original order
        }
        OomPolicy::Panic => {
            panic!("Out of memory!");
        }
    }

    // Select first candidate that can be killed
    for mut candidate in candidates {
        if can_kill_process(candidate.pid) {
            candidate.selected = true;
            return Ok(candidate);
        }
    }

    Err(OomError::NoVictim)
}

/// List OOM candidate processes
fn list_oom_candidates(context: &OomContext) -> Vec<OomVictim> {
    let candidates = Vec::new();

    // Would iterate all processes
    // For now, return empty list (would be populated by process iterator)

    // Filter by cgroup if constrained
    if let Some(ref _cgroup_path) = context.cgroup_path {
        // Only include processes in that cgroup
    }

    candidates
}

/// Check if a process can be killed
fn can_kill_process(pid: Pid) -> bool {
    let state = OOM_STATE.read();

    // Check if process has OOM kill disabled
    if let Some(control) = state.process_control.get(&pid) {
        if control.oom_kill_disable {
            return false;
        }
        if control.oom_victim {
            return false; // Already being killed
        }
    }

    // Don't kill PID 1 (init)
    if pid == Pid::new(1) {
        return false;
    }

    // Don't kill kernel threads (PID < 2 typically)
    if pid < Pid::new(2) {
        return false;
    }

    true
}

/// Perform the OOM kill
fn do_oom_kill(victim: &OomVictim) -> Result<(), OomError> {
    // Mark as OOM victim
    {
        let mut state = OOM_STATE.write();
        let control = state.process_control.entry(victim.pid).or_insert_with(OomControl::default);
        control.oom_victim = true;
    }

    // Send SIGKILL to process
    // Would call: signal::send_signal(victim.pid, Signal::SIGKILL);

    // Request OOM reaper to clean up
    request_oom_reaper(victim.pid);

    Ok(())
}

/// Request OOM reaper to clean up victim's memory
fn request_oom_reaper(pid: Pid) {
    // The OOM reaper runs as a kernel thread
    // It reaps the victim's memory mappings before the process fully exits
    // This speeds up memory recovery
    let mut state = OOM_STATE.write();
    if let Some(control) = state.process_control.get_mut(&pid) {
        control.oom_reaper_requested = true;
    }
}

/// OOM reaper work (called from kernel thread)
pub fn oom_reaper_work() {
    let state = OOM_STATE.read();

    for (&pid, control) in &state.process_control {
        if control.oom_reaper_requested && control.oom_victim {
            // Would unmap process memory mappings
            // This releases memory before process fully exits
            let _ = pid;
        }
    }
}

/// Clean up OOM control for exited process
pub fn process_exit(pid: Pid) {
    OOM_STATE.write().process_control.remove(&pid);
}

/// Get OOM kill statistics
pub fn get_stats() -> OomStats {
    let state = OOM_STATE.read();
    OomStats {
        kill_count: state.kill_count,
        current_pressure: state.pressure,
        policy: state.policy,
        panic_on_oom: state.panic_on_oom,
        recent_victims: state.recent_victims.clone(),
    }
}

/// Get memory PSI stats
pub fn get_psi() -> PressureStats {
    MEMORY_PSI.read().clone()
}

/// Update PSI stats (called from memory allocation path)
pub fn update_psi(stall_time_us: u64, full_stall: bool) {
    let mut psi = MEMORY_PSI.write();
    psi.some_total += stall_time_us;
    if full_stall {
        psi.full_total += stall_time_us;
    }
    // Would update moving averages
}

// =============================================================================
// OOM ERROR TYPES
// =============================================================================

/// OOM errors
#[derive(Clone, Debug)]
pub enum OomError {
    /// OOM killer is disabled
    Disabled,
    /// No suitable victim found
    NoVictim,
    /// Kill attempted too frequently
    TooFrequent,
    /// Invalid argument
    InvalidArgument,
    /// Process not found
    ProcessNotFound,
    /// Permission denied
    PermissionDenied,
}

impl OomError {
    /// Convert to errno
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::Disabled => -12,        // ENOMEM
            Self::NoVictim => -12,        // ENOMEM
            Self::TooFrequent => -11,     // EAGAIN
            Self::InvalidArgument => -22, // EINVAL
            Self::ProcessNotFound => -3,  // ESRCH
            Self::PermissionDenied => -1, // EPERM
        }
    }
}

/// OOM statistics
#[derive(Clone, Debug)]
pub struct OomStats {
    /// Total OOM kills
    pub kill_count: u64,
    /// Current pressure level
    pub current_pressure: MemoryPressure,
    /// Current policy
    pub policy: OomPolicy,
    /// Panic on OOM enabled
    pub panic_on_oom: bool,
    /// Recent victims
    pub recent_victims: Vec<OomVictim>,
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Get current time in milliseconds
fn current_time_ms() -> u64 {
    // Would read system clock
    0
}

// =============================================================================
// CGROUP OOM INTEGRATION
// =============================================================================

/// Handle cgroup memory limit exceeded
pub fn cgroup_oom(cgroup_path: &str, pages_needed: usize) -> Result<Pid, OomError> {
    let context = OomContext {
        order: 0,
        requested_pages: pages_needed,
        gfp_mask: 0,
        cgroup_path: Some(cgroup_path.to_string()),
        numa_node: None,
        pressure: MemoryPressure::Oom,
        total_pages: 0,
        free_pages: 0,
        reclaimable_pages: 0,
    };

    oom_kill(&context)
}

/// Check cgroup memory limits before OOM
pub fn cgroup_memory_check(cgroup_path: &str, pages: usize) -> bool {
    // Would check cgroup memory limits
    let _ = (cgroup_path, pages);
    true // Allow for now
}

// =============================================================================
// SYSCTL INTERFACE
// =============================================================================

/// Read OOM-related sysctl
pub fn sysctl_read(name: &str) -> Option<String> {
    let state = OOM_STATE.read();

    match name {
        "vm.panic_on_oom" => Some(if state.panic_on_oom { "1" } else { "0" }.to_string()),
        "vm.oom_kill_allocating_task" => Some("0".to_string()),
        "vm.oom_dump_tasks" => Some("1".to_string()),
        _ => None,
    }
}

/// Write OOM-related sysctl
pub fn sysctl_write(name: &str, value: &str) -> Result<(), OomError> {
    match name {
        "vm.panic_on_oom" => {
            let panic = value.trim() == "1";
            set_panic_on_oom(panic);
            Ok(())
        }
        _ => Err(OomError::InvalidArgument),
    }
}

// =============================================================================
// PROC INTERFACE
// =============================================================================

/// Read /proc/[pid]/oom_score
pub fn proc_oom_score(pid: Pid) -> String {
    alloc::format!("{}\n", get_oom_score(pid))
}

/// Read /proc/[pid]/oom_score_adj
pub fn proc_oom_score_adj(pid: Pid) -> String {
    alloc::format!("{}\n", get_oom_score_adj(pid))
}

/// Write /proc/[pid]/oom_score_adj
pub fn proc_write_oom_score_adj(pid: Pid, value: &str) -> Result<(), OomError> {
    let adj: i64 = value.trim().parse().map_err(|_| OomError::InvalidArgument)?;
    set_oom_score_adj(pid, adj)
}

/// Read /proc/[pid]/oom_adj (deprecated, maps to oom_score_adj)
pub fn proc_oom_adj(pid: Pid) -> String {
    // Map -1000..1000 to -17..15
    let adj = get_oom_score_adj(pid);
    let oom_adj = (adj * 15) / 1000;
    alloc::format!("{}\n", oom_adj)
}

/// Read /proc/pressure/memory
pub fn proc_pressure_memory() -> String {
    get_psi().format()
}

/// Read /proc/meminfo OOM-related fields
pub fn proc_meminfo_oom() -> String {
    let state = OOM_STATE.read();
    alloc::format!(
        "OomKillCount:  {}\n",
        state.kill_count
    )
}

// =============================================================================
// MEMCG (MEMORY CGROUP) OOM
// =============================================================================

/// Memory cgroup OOM control
#[derive(Clone, Debug)]
pub struct MemcgOomControl {
    /// OOM events count
    pub oom_events: u64,
    /// OOM kill events count
    pub oom_kill: u64,
    /// Under OOM
    pub under_oom: bool,
    /// OOM killing disabled
    pub oom_kill_disable: bool,
}

impl Default for MemcgOomControl {
    fn default() -> Self {
        Self {
            oom_events: 0,
            oom_kill: 0,
            under_oom: false,
            oom_kill_disable: false,
        }
    }
}

impl MemcgOomControl {
    /// Format for memory.oom_control file
    pub fn format(&self) -> String {
        alloc::format!(
            "oom_kill_disable {}\nunder_oom {}\noom_kill {}\n",
            if self.oom_kill_disable { 1 } else { 0 },
            if self.under_oom { 1 } else { 0 },
            self.oom_kill
        )
    }
}

// =============================================================================
// LOW MEMORY KILLER (Android-style)
// =============================================================================

/// Low memory killer thresholds
#[derive(Clone, Debug)]
pub struct LmkThreshold {
    /// Free memory threshold (pages)
    pub minfree: usize,
    /// OOM score adj to kill above
    pub adj: i64,
}

/// Low memory killer configuration
static LMK_CONFIG: RwLock<Vec<LmkThreshold>> = RwLock::new(Vec::new());

/// Initialize low memory killer with thresholds
pub fn lmk_init(thresholds: Vec<LmkThreshold>) {
    *LMK_CONFIG.write() = thresholds;
}

/// Low memory killer check (call periodically)
pub fn lmk_check(free_pages: usize) -> Option<Pid> {
    let config = LMK_CONFIG.read();

    for threshold in config.iter() {
        if free_pages < threshold.minfree {
            // Find process with adj >= threshold.adj to kill
            if let Some(victim) = find_lmk_victim(threshold.adj) {
                return Some(victim);
            }
        }
    }

    None
}

/// Find LMK victim with adj >= min_adj
fn find_lmk_victim(min_adj: i64) -> Option<Pid> {
    let state = OOM_STATE.read();

    // Find process with highest adj >= min_adj
    let mut best: Option<(Pid, i64)> = None;

    for (&pid, control) in &state.process_control {
        if control.score_adj >= min_adj && !control.oom_kill_disable {
            match best {
                None => best = Some((pid, control.score_adj)),
                Some((_, adj)) if control.score_adj > adj => {
                    best = Some((pid, control.score_adj));
                }
                _ => {}
            }
        }
    }

    best.map(|(pid, _)| pid)
}
