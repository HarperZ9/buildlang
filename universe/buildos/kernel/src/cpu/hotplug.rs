// ===============================================================================
// BUILDOS KERNEL - CPU HOTPLUG
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! CPU hotplug support for dynamic CPU online/offline.
//!
//! This module provides:
//! - CPU state machine for hotplug operations
//! - Notification callbacks for subsystems
//! - Safe CPU bring-up and tear-down
//! - Per-CPU resource management

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicBool, Ordering};
use spin::{Mutex, RwLock};

use super::smp::{self, MAX_CPUS};

// =============================================================================
// HOTPLUG STATES
// =============================================================================

/// CPU hotplug state machine states
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum HotplugState {
    /// CPU is offline
    Offline = 0,

    /// Bring-up: AP processor started
    BringupApOnline = 10,

    /// Bring-up: AP ready for boot
    BringupApBootReady = 20,

    /// Bring-up: AP thread ready
    BringupApThreadReady = 30,

    /// Bring-up: Timer active
    ApTimerStarting = 40,

    /// Bring-up: Scheduler starting
    ApSchedulerStarting = 50,

    /// Bring-up: Workqueue active
    ApWorkqueueOnline = 60,

    /// Bring-up: RCU active
    ApRcuOnline = 70,

    /// Bring-up: Perf active
    ApPerfOnline = 80,

    /// CPU is online
    Online = 100,

    /// Tear-down: Perf offline
    TeardownPerfOffline = 110,

    /// Tear-down: RCU offline
    TeardownRcuOffline = 120,

    /// Tear-down: Workqueue offline
    TeardownWorkqueueOffline = 130,

    /// Tear-down: Scheduler offline
    TeardownSchedulerOffline = 140,

    /// Tear-down: Timer offline
    TeardownTimerOffline = 150,

    /// Tear-down: AP cleanup
    TeardownApCleanup = 160,
}

// =============================================================================
// CALLBACKS
// =============================================================================

/// Hotplug callback type
pub type HotplugCallback = fn(cpu: u32) -> Result<(), HotplugError>;

/// Hotplug callback entry
#[derive(Clone)]
pub struct CallbackEntry {
    /// Callback name (for debugging)
    pub name: String,
    /// State when callback runs during bring-up
    pub state_up: HotplugState,
    /// State when callback runs during tear-down
    pub state_down: HotplugState,
    /// Bring-up callback
    pub startup: Option<HotplugCallback>,
    /// Tear-down callback
    pub teardown: Option<HotplugCallback>,
    /// Priority (lower = earlier)
    pub priority: i32,
}

/// Registered callbacks
static CALLBACKS: RwLock<Vec<CallbackEntry>> = RwLock::new(Vec::new());

/// Per-CPU hotplug state
static CPU_HOTPLUG_STATE: [AtomicU32; MAX_CPUS] = {
    const INIT: AtomicU32 = AtomicU32::new(HotplugState::Offline as u32);
    [INIT; MAX_CPUS]
};

/// Hotplug in progress flag
static HOTPLUG_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Hotplug lock
static HOTPLUG_LOCK: Mutex<()> = Mutex::new(());

// =============================================================================
// HOTPLUG ERRORS
// =============================================================================

/// Hotplug operation errors
#[derive(Clone, Debug)]
pub enum HotplugError {
    /// CPU is already in target state
    AlreadyInState,
    /// Callback failed
    CallbackFailed { name: String, state: HotplugState },
    /// Operation timed out
    Timeout,
    /// Invalid CPU
    InvalidCpu,
    /// Cannot offline BSP
    CannotOfflineBsp,
    /// Resource allocation failed
    ResourceAllocation,
    /// Another hotplug in progress
    Busy,
}

// =============================================================================
// PUBLIC INTERFACE
// =============================================================================

/// Register a hotplug callback
pub fn register_callback(
    name: &str,
    state_up: HotplugState,
    state_down: HotplugState,
    startup: Option<HotplugCallback>,
    teardown: Option<HotplugCallback>,
    priority: i32,
) {
    let entry = CallbackEntry {
        name: String::from(name),
        state_up,
        state_down,
        startup,
        teardown,
        priority,
    };

    let mut callbacks = CALLBACKS.write();
    callbacks.push(entry);
    callbacks.sort_by_key(|c| c.priority);
}

/// Unregister a hotplug callback
pub fn unregister_callback(name: &str) {
    let mut callbacks = CALLBACKS.write();
    callbacks.retain(|c| c.name != name);
}

/// Bring a CPU online
pub fn cpu_up(cpu: u32) -> Result<(), HotplugError> {
    if cpu as usize >= MAX_CPUS {
        return Err(HotplugError::InvalidCpu);
    }

    // Take hotplug lock
    let _lock = HOTPLUG_LOCK.lock();

    if HOTPLUG_IN_PROGRESS.swap(true, Ordering::Acquire) {
        return Err(HotplugError::Busy);
    }

    let result = do_cpu_up(cpu);

    HOTPLUG_IN_PROGRESS.store(false, Ordering::Release);

    result
}

/// Bring a CPU offline
pub fn cpu_down(cpu: u32) -> Result<(), HotplugError> {
    if cpu as usize >= MAX_CPUS {
        return Err(HotplugError::InvalidCpu);
    }

    if cpu == smp::manager().bsp_id() {
        return Err(HotplugError::CannotOfflineBsp);
    }

    // Take hotplug lock
    let _lock = HOTPLUG_LOCK.lock();

    if HOTPLUG_IN_PROGRESS.swap(true, Ordering::Acquire) {
        return Err(HotplugError::Busy);
    }

    let result = do_cpu_down(cpu);

    HOTPLUG_IN_PROGRESS.store(false, Ordering::Release);

    result
}

/// Do the actual CPU bring-up
fn do_cpu_up(cpu: u32) -> Result<(), HotplugError> {
    let current_state = get_state(cpu);

    if current_state == HotplugState::Online {
        return Err(HotplugError::AlreadyInState);
    }

    // Walk through bring-up states
    let states = [
        HotplugState::BringupApOnline,
        HotplugState::BringupApBootReady,
        HotplugState::BringupApThreadReady,
        HotplugState::ApTimerStarting,
        HotplugState::ApSchedulerStarting,
        HotplugState::ApWorkqueueOnline,
        HotplugState::ApRcuOnline,
        HotplugState::ApPerfOnline,
        HotplugState::Online,
    ];

    for &target_state in &states {
        if (current_state as u32) >= (target_state as u32) {
            continue;
        }

        // Run callbacks for this state
        run_callbacks_up(cpu, target_state)?;

        // Update state
        set_state(cpu, target_state);
    }

    Ok(())
}

/// Do the actual CPU tear-down
fn do_cpu_down(cpu: u32) -> Result<(), HotplugError> {
    let current_state = get_state(cpu);

    if current_state == HotplugState::Offline {
        return Err(HotplugError::AlreadyInState);
    }

    // Walk through tear-down states
    let states = [
        HotplugState::TeardownPerfOffline,
        HotplugState::TeardownRcuOffline,
        HotplugState::TeardownWorkqueueOffline,
        HotplugState::TeardownSchedulerOffline,
        HotplugState::TeardownTimerOffline,
        HotplugState::TeardownApCleanup,
        HotplugState::Offline,
    ];

    for &target_state in &states {
        // Run callbacks for this state
        run_callbacks_down(cpu, target_state)?;

        // Update state
        set_state(cpu, target_state);
    }

    Ok(())
}

/// Run bring-up callbacks for a state
fn run_callbacks_up(cpu: u32, state: HotplugState) -> Result<(), HotplugError> {
    let callbacks = CALLBACKS.read();

    for entry in callbacks.iter() {
        if entry.state_up == state {
            if let Some(callback) = entry.startup {
                callback(cpu).map_err(|_| HotplugError::CallbackFailed {
                    name: entry.name.clone(),
                    state,
                })?;
            }
        }
    }

    Ok(())
}

/// Run tear-down callbacks for a state
fn run_callbacks_down(cpu: u32, state: HotplugState) -> Result<(), HotplugError> {
    let callbacks = CALLBACKS.read();

    // Run in reverse order for tear-down
    for entry in callbacks.iter().rev() {
        if entry.state_down == state {
            if let Some(callback) = entry.teardown {
                callback(cpu).map_err(|_| HotplugError::CallbackFailed {
                    name: entry.name.clone(),
                    state,
                })?;
            }
        }
    }

    Ok(())
}

/// Get CPU hotplug state
pub fn get_state(cpu: u32) -> HotplugState {
    if cpu as usize >= MAX_CPUS {
        return HotplugState::Offline;
    }

    let state = CPU_HOTPLUG_STATE[cpu as usize].load(Ordering::Acquire);
    match state {
        0 => HotplugState::Offline,
        10 => HotplugState::BringupApOnline,
        20 => HotplugState::BringupApBootReady,
        30 => HotplugState::BringupApThreadReady,
        40 => HotplugState::ApTimerStarting,
        50 => HotplugState::ApSchedulerStarting,
        60 => HotplugState::ApWorkqueueOnline,
        70 => HotplugState::ApRcuOnline,
        80 => HotplugState::ApPerfOnline,
        100 => HotplugState::Online,
        110 => HotplugState::TeardownPerfOffline,
        120 => HotplugState::TeardownRcuOffline,
        130 => HotplugState::TeardownWorkqueueOffline,
        140 => HotplugState::TeardownSchedulerOffline,
        150 => HotplugState::TeardownTimerOffline,
        160 => HotplugState::TeardownApCleanup,
        _ => HotplugState::Offline,
    }
}

/// Set CPU hotplug state
fn set_state(cpu: u32, state: HotplugState) {
    if (cpu as usize) < MAX_CPUS {
        CPU_HOTPLUG_STATE[cpu as usize].store(state as u32, Ordering::Release);
    }
}

/// Check if CPU is online
pub fn is_cpu_online(cpu: u32) -> bool {
    get_state(cpu) == HotplugState::Online
}

/// Get mask of online CPUs
pub fn online_mask() -> u64 {
    let mut mask = 0u64;
    for cpu in 0..MAX_CPUS.min(64) {
        if is_cpu_online(cpu as u32) {
            mask |= 1 << cpu;
        }
    }
    mask
}

/// Get count of online CPUs
pub fn online_count() -> u32 {
    let mut count = 0;
    for cpu in 0..MAX_CPUS {
        if is_cpu_online(cpu as u32) {
            count += 1;
        }
    }
    count
}

// =============================================================================
// NOTIFIER CHAIN
// =============================================================================

/// CPU notifier actions
#[derive(Clone, Copy, Debug)]
pub enum CpuNotifyAction {
    /// CPU going online
    Online,
    /// CPU now online
    OnlineComplete,
    /// CPU going offline
    Offline,
    /// CPU now offline
    OfflineComplete,
    /// CPU frozen (suspend)
    Frozen,
    /// CPU thawed (resume)
    Thawed,
}

/// CPU notifier callback
pub type CpuNotifier = fn(cpu: u32, action: CpuNotifyAction) -> bool;

/// Registered notifiers
static NOTIFIERS: RwLock<Vec<CpuNotifier>> = RwLock::new(Vec::new());

/// Register CPU notifier
pub fn register_notifier(notifier: CpuNotifier) {
    NOTIFIERS.write().push(notifier);
}

/// Unregister CPU notifier
pub fn unregister_notifier(notifier: CpuNotifier) {
    NOTIFIERS.write().retain(|&n| n as usize != notifier as usize);
}

/// Notify all registered callbacks
pub fn notify_all(cpu: u32, action: CpuNotifyAction) {
    let notifiers = NOTIFIERS.read();
    for notifier in notifiers.iter() {
        notifier(cpu, action);
    }
}

// =============================================================================
// INITIALIZATION
// =============================================================================

/// Initialize CPU hotplug subsystem
pub fn init() {
    // Register default callbacks

    // Timer callback
    register_callback(
        "timer",
        HotplugState::ApTimerStarting,
        HotplugState::TeardownTimerOffline,
        Some(timer_startup),
        Some(timer_teardown),
        0,
    );

    // Scheduler callback
    register_callback(
        "scheduler",
        HotplugState::ApSchedulerStarting,
        HotplugState::TeardownSchedulerOffline,
        Some(scheduler_startup),
        Some(scheduler_teardown),
        10,
    );

    // RCU callback
    register_callback(
        "rcu",
        HotplugState::ApRcuOnline,
        HotplugState::TeardownRcuOffline,
        Some(rcu_startup),
        Some(rcu_teardown),
        20,
    );

    // Mark BSP as online
    set_state(smp::manager().bsp_id(), HotplugState::Online);
}

/// Timer startup callback
fn timer_startup(_cpu: u32) -> Result<(), HotplugError> {
    // Initialize timer for this CPU
    Ok(())
}

/// Timer teardown callback
fn timer_teardown(_cpu: u32) -> Result<(), HotplugError> {
    // Stop timer on this CPU
    Ok(())
}

/// Scheduler startup callback
fn scheduler_startup(_cpu: u32) -> Result<(), HotplugError> {
    // Initialize scheduler for this CPU
    crate::sched::init_cpu();
    Ok(())
}

/// Scheduler teardown callback
fn scheduler_teardown(_cpu: u32) -> Result<(), HotplugError> {
    // Migrate threads off this CPU
    Ok(())
}

/// RCU startup callback
fn rcu_startup(_cpu: u32) -> Result<(), HotplugError> {
    // Initialize RCU for this CPU
    Ok(())
}

/// RCU teardown callback
fn rcu_teardown(_cpu: u32) -> Result<(), HotplugError> {
    // Cleanup RCU for this CPU
    Ok(())
}
