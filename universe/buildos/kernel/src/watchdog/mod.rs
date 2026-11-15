// ===============================================================================
// BUILDOS KERNEL - WATCHDOG SUBSYSTEM
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Kernel watchdog for detecting soft and hard lockups.
//!
//! This module provides:
//! - Per-CPU watchdog timers
//! - Soft lockup detection (scheduler not running)
//! - Hard lockup detection (CPU not responding to NMI)
//! - Hung task detection
//! - Automatic recovery actions

#![allow(dead_code)]

pub mod softlockup;
pub mod hardlockup;
pub mod hungtask;

use core::sync::atomic::{AtomicBool, AtomicU64, AtomicU32, Ordering};
use spin::{Mutex, RwLock};
use alloc::vec::Vec;

use crate::sched::MAX_CPUS;

// =============================================================================
// CONSTANTS
// =============================================================================

/// Default soft lockup threshold (10 seconds)
pub const DEFAULT_SOFT_LOCKUP_THRESHOLD_NS: u64 = 10_000_000_000;

/// Default hard lockup threshold (10 seconds)
pub const DEFAULT_HARD_LOCKUP_THRESHOLD_NS: u64 = 10_000_000_000;

/// Default hung task timeout (120 seconds)
pub const DEFAULT_HUNG_TASK_TIMEOUT_NS: u64 = 120_000_000_000;

/// Watchdog sample period (1 second)
pub const WATCHDOG_SAMPLE_PERIOD_NS: u64 = 1_000_000_000;

// =============================================================================
// GLOBAL STATE
// =============================================================================

/// Global watchdog state
static WATCHDOG: RwLock<Watchdog> = RwLock::new(Watchdog::new());

/// Per-CPU watchdog data
static mut CPU_WATCHDOG: [CpuWatchdog; MAX_CPUS] = {
    const INIT: CpuWatchdog = CpuWatchdog::new();
    [INIT; MAX_CPUS]
};

/// Watchdog subsystem
pub struct Watchdog {
    /// Is watchdog enabled?
    enabled: AtomicBool,

    /// Soft lockup detection enabled
    soft_lockup_enabled: AtomicBool,

    /// Hard lockup detection enabled
    hard_lockup_enabled: AtomicBool,

    /// Hung task detection enabled
    hung_task_enabled: AtomicBool,

    /// Soft lockup threshold (ns)
    soft_lockup_threshold: AtomicU64,

    /// Hard lockup threshold (ns)
    hard_lockup_threshold: AtomicU64,

    /// Hung task timeout (ns)
    hung_task_timeout: AtomicU64,

    /// Panic on soft lockup
    soft_lockup_panic: AtomicBool,

    /// Panic on hard lockup
    hard_lockup_panic: AtomicBool,

    /// Panic on hung task
    hung_task_panic: AtomicBool,

    /// Number of soft lockups detected
    soft_lockup_count: AtomicU64,

    /// Number of hard lockups detected
    hard_lockup_count: AtomicU64,

    /// Number of hung tasks detected
    hung_task_count: AtomicU64,

    /// Recovery actions
    recovery_actions: Mutex<Vec<RecoveryAction>>,
}

impl Watchdog {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            soft_lockup_enabled: AtomicBool::new(true),
            hard_lockup_enabled: AtomicBool::new(true),
            hung_task_enabled: AtomicBool::new(true),
            soft_lockup_threshold: AtomicU64::new(DEFAULT_SOFT_LOCKUP_THRESHOLD_NS),
            hard_lockup_threshold: AtomicU64::new(DEFAULT_HARD_LOCKUP_THRESHOLD_NS),
            hung_task_timeout: AtomicU64::new(DEFAULT_HUNG_TASK_TIMEOUT_NS),
            soft_lockup_panic: AtomicBool::new(false),
            hard_lockup_panic: AtomicBool::new(true),
            hung_task_panic: AtomicBool::new(false),
            soft_lockup_count: AtomicU64::new(0),
            hard_lockup_count: AtomicU64::new(0),
            hung_task_count: AtomicU64::new(0),
            recovery_actions: Mutex::new(Vec::new()),
        }
    }
}

/// Per-CPU watchdog state
pub struct CpuWatchdog {
    /// CPU ID
    cpu: AtomicU32,

    /// Last timestamp scheduler ran
    last_sched_timestamp: AtomicU64,

    /// Last timestamp of NMI response
    last_nmi_timestamp: AtomicU64,

    /// Is CPU in soft lockup?
    soft_lockup: AtomicBool,

    /// Is CPU in hard lockup?
    hard_lockup: AtomicBool,

    /// Soft lockup duration (ns)
    soft_lockup_duration: AtomicU64,

    /// NMI watchdog hrtimer
    nmi_watchdog_enabled: AtomicBool,

    /// Number of hrtimer interrupts
    hrtimer_interrupts: AtomicU64,

    /// Number of NMIs received
    nmi_count: AtomicU64,

    /// Touch timestamp (last pet)
    touch_timestamp: AtomicU64,
}

impl CpuWatchdog {
    const fn new() -> Self {
        Self {
            cpu: AtomicU32::new(0),
            last_sched_timestamp: AtomicU64::new(0),
            last_nmi_timestamp: AtomicU64::new(0),
            soft_lockup: AtomicBool::new(false),
            hard_lockup: AtomicBool::new(false),
            soft_lockup_duration: AtomicU64::new(0),
            nmi_watchdog_enabled: AtomicBool::new(false),
            hrtimer_interrupts: AtomicU64::new(0),
            nmi_count: AtomicU64::new(0),
            touch_timestamp: AtomicU64::new(0),
        }
    }

    /// Touch the watchdog (reset timer)
    pub fn touch(&self) {
        let now = crate::time::now_ns();
        self.touch_timestamp.store(now, Ordering::Release);
        self.last_sched_timestamp.store(now, Ordering::Release);
    }

    /// Check for soft lockup
    pub fn check_soft_lockup(&self) -> bool {
        let wd = WATCHDOG.read();
        if !wd.soft_lockup_enabled.load(Ordering::Relaxed) {
            return false;
        }

        let now = crate::time::now_ns();
        let last = self.last_sched_timestamp.load(Ordering::Acquire);
        let threshold = wd.soft_lockup_threshold.load(Ordering::Relaxed);

        if now.saturating_sub(last) > threshold {
            self.soft_lockup.store(true, Ordering::Release);
            self.soft_lockup_duration.store(now - last, Ordering::Release);
            return true;
        }

        self.soft_lockup.store(false, Ordering::Release);
        false
    }

    /// Check for hard lockup
    pub fn check_hard_lockup(&self) -> bool {
        let wd = WATCHDOG.read();
        if !wd.hard_lockup_enabled.load(Ordering::Relaxed) {
            return false;
        }

        let hrtimer = self.hrtimer_interrupts.load(Ordering::Acquire);
        let nmi = self.nmi_count.load(Ordering::Acquire);

        // If hrtimer interrupts increased but NMI didn't, we have a hard lockup
        // (This is simplified - real implementation tracks across samples)

        let _ = (hrtimer, nmi);
        false
    }

    /// Record hrtimer interrupt
    pub fn record_hrtimer(&self) {
        self.hrtimer_interrupts.fetch_add(1, Ordering::Relaxed);
        self.last_sched_timestamp.store(crate::time::now_ns(), Ordering::Release);
    }

    /// Record NMI
    pub fn record_nmi(&self) {
        self.nmi_count.fetch_add(1, Ordering::Relaxed);
        self.last_nmi_timestamp.store(crate::time::now_ns(), Ordering::Release);
    }
}

/// Recovery action for lockup
#[derive(Clone)]
pub enum RecoveryAction {
    /// Log the event
    Log,
    /// Dump CPU state
    DumpCpuState,
    /// Dump all tasks
    DumpTasks,
    /// Kill the offending task
    KillTask,
    /// Reset the CPU
    ResetCpu,
    /// Trigger panic
    Panic,
    /// Call custom handler
    Custom(fn(cpu: usize)),
}

// =============================================================================
// LOCKUP INFO
// =============================================================================

/// Information about a detected lockup
#[derive(Clone)]
pub struct LockupInfo {
    /// Type of lockup
    pub lockup_type: LockupType,
    /// CPU that locked up
    pub cpu: usize,
    /// Duration of lockup (ns)
    pub duration_ns: u64,
    /// Task that was running (if known)
    pub task_id: Option<u64>,
    /// Task name (if known)
    pub task_name: Option<&'static str>,
    /// Instruction pointer
    pub rip: u64,
    /// Stack pointer
    pub rsp: u64,
    /// Stack trace
    pub backtrace: [u64; 16],
}

/// Type of lockup detected
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LockupType {
    /// Soft lockup (scheduler not running)
    Soft,
    /// Hard lockup (CPU not responding)
    Hard,
    /// RCU stall
    RcuStall,
    /// Hung task
    HungTask,
}

// =============================================================================
// PUBLIC INTERFACE
// =============================================================================

/// Initialize watchdog subsystem
pub fn init() {
    let wd = WATCHDOG.write();
    wd.enabled.store(true, Ordering::Release);

    // Initialize per-CPU watchdogs
    let num_cpus = crate::sched::online_cpus();
    for cpu in 0..num_cpus {
        unsafe {
            CPU_WATCHDOG[cpu].cpu.store(cpu as u32, Ordering::Release);
            CPU_WATCHDOG[cpu].touch();
        }
    }

    // Initialize soft lockup detector
    softlockup::init();

    // Initialize hard lockup detector (NMI watchdog)
    hardlockup::init();

    // Initialize hung task detector
    hungtask::init();

    drop(wd);

    crate::kprintln!("[WATCHDOG] Initialized: soft={} hard={} hung_task={}",
        is_soft_lockup_enabled(),
        is_hard_lockup_enabled(),
        is_hung_task_enabled());
}

/// Initialize watchdog for current CPU
pub fn init_cpu() {
    let cpu = crate::cpu::current_cpu_id() as usize;

    unsafe {
        CPU_WATCHDOG[cpu].cpu.store(cpu as u32, Ordering::Release);
        CPU_WATCHDOG[cpu].touch();
        CPU_WATCHDOG[cpu].nmi_watchdog_enabled.store(true, Ordering::Release);
    }

    // Enable NMI watchdog on this CPU
    hardlockup::enable_cpu(cpu);
}

/// Pet the watchdog (called by scheduler)
pub fn touch() {
    let cpu = crate::cpu::current_cpu_id() as usize;
    if cpu < MAX_CPUS {
        unsafe {
            CPU_WATCHDOG[cpu].touch();
        }
    }
}

/// Pet watchdog for specific CPU
pub fn touch_cpu(cpu: usize) {
    if cpu < MAX_CPUS {
        unsafe {
            CPU_WATCHDOG[cpu].touch();
        }
    }
}

/// Watchdog timer interrupt handler
pub fn timer_interrupt() {
    let cpu = crate::cpu::current_cpu_id() as usize;
    if cpu >= MAX_CPUS {
        return;
    }

    unsafe {
        CPU_WATCHDOG[cpu].record_hrtimer();

        // Check for soft lockup
        if CPU_WATCHDOG[cpu].check_soft_lockup() {
            handle_soft_lockup(cpu);
        }
    }
}

/// NMI handler for hard lockup detection
pub fn nmi_handler() {
    let cpu = crate::cpu::current_cpu_id() as usize;
    if cpu >= MAX_CPUS {
        return;
    }

    unsafe {
        CPU_WATCHDOG[cpu].record_nmi();

        // Check for hard lockup
        if CPU_WATCHDOG[cpu].check_hard_lockup() {
            handle_hard_lockup(cpu);
        }
    }
}

/// Handle soft lockup on CPU
fn handle_soft_lockup(cpu: usize) {
    let wd = WATCHDOG.read();
    wd.soft_lockup_count.fetch_add(1, Ordering::Relaxed);

    let duration = unsafe {
        CPU_WATCHDOG[cpu].soft_lockup_duration.load(Ordering::Acquire)
    };

    crate::kprintln!("!!! SOFT LOCKUP on CPU {} for {}ns !!!", cpu, duration);

    // Execute recovery actions
    for action in wd.recovery_actions.lock().iter() {
        match action {
            RecoveryAction::Log => {
                crate::kprintln!("[WATCHDOG] Soft lockup logged");
            }
            RecoveryAction::DumpCpuState => {
                dump_cpu_state(cpu);
            }
            RecoveryAction::DumpTasks => {
                dump_tasks();
            }
            RecoveryAction::KillTask => {
                // Would kill offending task
            }
            RecoveryAction::ResetCpu => {
                // Would reset CPU
            }
            RecoveryAction::Panic => {
                if wd.soft_lockup_panic.load(Ordering::Relaxed) {
                    panic!("Soft lockup on CPU {}", cpu);
                }
            }
            RecoveryAction::Custom(handler) => {
                handler(cpu);
            }
        }
    }
}

/// Handle hard lockup on CPU
fn handle_hard_lockup(cpu: usize) {
    let wd = WATCHDOG.read();
    wd.hard_lockup_count.fetch_add(1, Ordering::Relaxed);

    crate::kprintln!("!!! HARD LOCKUP on CPU {} !!!", cpu);

    // Hard lockups are severe - usually panic
    if wd.hard_lockup_panic.load(Ordering::Relaxed) {
        panic!("Hard lockup on CPU {}", cpu);
    }
}

/// Dump CPU state for debugging
fn dump_cpu_state(cpu: usize) {
    crate::kprintln!("CPU {} state:", cpu);
    // Would dump registers, stack, etc.
    let _ = cpu;
}

/// Dump all tasks for debugging
fn dump_tasks() {
    crate::kprintln!("Task dump:");
    // Would iterate and dump all tasks
}

// =============================================================================
// CONFIGURATION
// =============================================================================

/// Enable/disable watchdog
pub fn set_enabled(enabled: bool) {
    WATCHDOG.read().enabled.store(enabled, Ordering::Release);
}

/// Check if watchdog is enabled
pub fn is_enabled() -> bool {
    WATCHDOG.read().enabled.load(Ordering::Relaxed)
}

/// Enable/disable soft lockup detection
pub fn set_soft_lockup_enabled(enabled: bool) {
    WATCHDOG.read().soft_lockup_enabled.store(enabled, Ordering::Release);
}

/// Check if soft lockup detection is enabled
pub fn is_soft_lockup_enabled() -> bool {
    WATCHDOG.read().soft_lockup_enabled.load(Ordering::Relaxed)
}

/// Enable/disable hard lockup detection
pub fn set_hard_lockup_enabled(enabled: bool) {
    WATCHDOG.read().hard_lockup_enabled.store(enabled, Ordering::Release);
}

/// Check if hard lockup detection is enabled
pub fn is_hard_lockup_enabled() -> bool {
    WATCHDOG.read().hard_lockup_enabled.load(Ordering::Relaxed)
}

/// Enable/disable hung task detection
pub fn set_hung_task_enabled(enabled: bool) {
    WATCHDOG.read().hung_task_enabled.store(enabled, Ordering::Release);
}

/// Check if hung task detection is enabled
pub fn is_hung_task_enabled() -> bool {
    WATCHDOG.read().hung_task_enabled.load(Ordering::Relaxed)
}

/// Set soft lockup threshold
pub fn set_soft_lockup_threshold(threshold_ns: u64) {
    WATCHDOG.read().soft_lockup_threshold.store(threshold_ns, Ordering::Release);
}

/// Get soft lockup threshold
pub fn soft_lockup_threshold() -> u64 {
    WATCHDOG.read().soft_lockup_threshold.load(Ordering::Relaxed)
}

/// Set hung task timeout
pub fn set_hung_task_timeout(timeout_ns: u64) {
    WATCHDOG.read().hung_task_timeout.store(timeout_ns, Ordering::Release);
}

/// Get hung task timeout
pub fn hung_task_timeout() -> u64 {
    WATCHDOG.read().hung_task_timeout.load(Ordering::Relaxed)
}

/// Set panic on soft lockup
pub fn set_soft_lockup_panic(panic: bool) {
    WATCHDOG.read().soft_lockup_panic.store(panic, Ordering::Release);
}

/// Set panic on hard lockup
pub fn set_hard_lockup_panic(panic: bool) {
    WATCHDOG.read().hard_lockup_panic.store(panic, Ordering::Release);
}

/// Add recovery action
pub fn add_recovery_action(action: RecoveryAction) {
    WATCHDOG.read().recovery_actions.lock().push(action);
}

/// Clear recovery actions
pub fn clear_recovery_actions() {
    WATCHDOG.read().recovery_actions.lock().clear();
}

// =============================================================================
// STATISTICS
// =============================================================================

/// Watchdog statistics
#[derive(Default)]
pub struct WatchdogStats {
    pub soft_lockups: u64,
    pub hard_lockups: u64,
    pub hung_tasks: u64,
}

/// Get watchdog statistics
pub fn get_stats() -> WatchdogStats {
    let wd = WATCHDOG.read();
    WatchdogStats {
        soft_lockups: wd.soft_lockup_count.load(Ordering::Relaxed),
        hard_lockups: wd.hard_lockup_count.load(Ordering::Relaxed),
        hung_tasks: wd.hung_task_count.load(Ordering::Relaxed),
    }
}

/// Get per-CPU watchdog status
pub fn get_cpu_status(cpu: usize) -> Option<CpuWatchdogStatus> {
    if cpu >= MAX_CPUS {
        return None;
    }

    unsafe {
        let wd = &CPU_WATCHDOG[cpu];
        Some(CpuWatchdogStatus {
            cpu,
            soft_lockup: wd.soft_lockup.load(Ordering::Relaxed),
            hard_lockup: wd.hard_lockup.load(Ordering::Relaxed),
            nmi_watchdog_enabled: wd.nmi_watchdog_enabled.load(Ordering::Relaxed),
            last_touch_ns: wd.touch_timestamp.load(Ordering::Relaxed),
            hrtimer_interrupts: wd.hrtimer_interrupts.load(Ordering::Relaxed),
            nmi_count: wd.nmi_count.load(Ordering::Relaxed),
        })
    }
}

/// Per-CPU watchdog status
pub struct CpuWatchdogStatus {
    pub cpu: usize,
    pub soft_lockup: bool,
    pub hard_lockup: bool,
    pub nmi_watchdog_enabled: bool,
    pub last_touch_ns: u64,
    pub hrtimer_interrupts: u64,
    pub nmi_count: u64,
}
