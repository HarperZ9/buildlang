// ===============================================================================
// BUILDOS KERNEL - SOFT LOCKUP DETECTOR
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Soft lockup detection.
//!
//! Detects when a CPU is stuck in kernel mode and not scheduling.
//! Uses a high-resolution timer to periodically check if the scheduler
//! is still running on each CPU.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::sched::MAX_CPUS;

// =============================================================================
// STATE
// =============================================================================

/// Per-CPU soft lockup detector
static mut SOFT_LOCKUP: [SoftLockupDetector; MAX_CPUS] = {
    const INIT: SoftLockupDetector = SoftLockupDetector::new();
    [INIT; MAX_CPUS]
};

/// Soft lockup detector for a CPU
pub struct SoftLockupDetector {
    /// CPU ID
    cpu: u32,

    /// Is detector active?
    active: AtomicBool,

    /// Last scheduler timestamp
    last_sched_ts: AtomicU64,

    /// Last check timestamp
    last_check_ts: AtomicU64,

    /// Timestamp when lockup started
    lockup_start_ts: AtomicU64,

    /// Is currently in soft lockup?
    in_lockup: AtomicBool,

    /// Lockup duration (ns)
    lockup_duration: AtomicU64,

    /// Refractory period (don't report again too soon)
    refractory_until: AtomicU64,

    /// RIP when lockup detected
    lockup_rip: AtomicU64,

    /// Task that was running
    lockup_task: AtomicU64,
}

impl SoftLockupDetector {
    const fn new() -> Self {
        Self {
            cpu: 0,
            active: AtomicBool::new(false),
            last_sched_ts: AtomicU64::new(0),
            last_check_ts: AtomicU64::new(0),
            lockup_start_ts: AtomicU64::new(0),
            in_lockup: AtomicBool::new(false),
            lockup_duration: AtomicU64::new(0),
            refractory_until: AtomicU64::new(0),
            lockup_rip: AtomicU64::new(0),
            lockup_task: AtomicU64::new(0),
        }
    }

    /// Initialize for a CPU
    pub fn init(&mut self, cpu: u32) {
        self.cpu = cpu;
        self.active.store(true, Ordering::Release);
        self.touch();
    }

    /// Touch the detector (called from scheduler)
    pub fn touch(&self) {
        let now = crate::time::now_ns();
        self.last_sched_ts.store(now, Ordering::Release);

        // Clear lockup state
        if self.in_lockup.load(Ordering::Acquire) {
            self.in_lockup.store(false, Ordering::Release);
            self.lockup_duration.store(0, Ordering::Release);
        }
    }

    /// Check for soft lockup (called from timer interrupt)
    pub fn check(&self) -> Option<SoftLockupEvent> {
        if !self.active.load(Ordering::Acquire) {
            return None;
        }

        let now = crate::time::now_ns();
        self.last_check_ts.store(now, Ordering::Release);

        let last_sched = self.last_sched_ts.load(Ordering::Acquire);
        let threshold = super::WATCHDOG.read().soft_lockup_threshold.load(Ordering::Relaxed);

        let delta = now.saturating_sub(last_sched);

        if delta > threshold {
            // In refractory period?
            let refractory = self.refractory_until.load(Ordering::Acquire);
            if now < refractory {
                return None;
            }

            // New lockup or ongoing?
            if !self.in_lockup.load(Ordering::Acquire) {
                self.lockup_start_ts.store(last_sched, Ordering::Release);
                self.in_lockup.store(true, Ordering::Release);
            }

            self.lockup_duration.store(delta, Ordering::Release);

            // Set refractory period (don't report again for 30 seconds)
            self.refractory_until.store(now + 30_000_000_000, Ordering::Release);

            // Capture current state
            let rip = Self::capture_rip();
            self.lockup_rip.store(rip, Ordering::Release);

            return Some(SoftLockupEvent {
                cpu: self.cpu as usize,
                duration_ns: delta,
                threshold_ns: threshold,
                rip,
                task_id: self.lockup_task.load(Ordering::Relaxed),
            });
        }

        None
    }

    /// Capture current instruction pointer
    fn capture_rip() -> u64 {
        // Would read RIP from current stack frame
        0
    }

    /// Set the currently running task
    pub fn set_current_task(&self, task_id: u64) {
        self.lockup_task.store(task_id, Ordering::Release);
    }

    /// Disable detector temporarily
    pub fn disable(&self) {
        self.active.store(false, Ordering::Release);
    }

    /// Enable detector
    pub fn enable(&self) {
        self.active.store(true, Ordering::Release);
        self.touch();
    }
}

/// Soft lockup event
pub struct SoftLockupEvent {
    pub cpu: usize,
    pub duration_ns: u64,
    pub threshold_ns: u64,
    pub rip: u64,
    pub task_id: u64,
}

// =============================================================================
// INTERFACE
// =============================================================================

/// Initialize soft lockup detection
pub fn init() {
    let num_cpus = crate::sched::online_cpus();

    for cpu in 0..num_cpus {
        unsafe {
            SOFT_LOCKUP[cpu].init(cpu as u32);
        }
    }
}

/// Initialize for current CPU
pub fn init_cpu() {
    let cpu = crate::cpu::current_cpu_id() as usize;
    if cpu < MAX_CPUS {
        unsafe {
            SOFT_LOCKUP[cpu].init(cpu as u32);
        }
    }
}

/// Touch soft lockup detector (called by scheduler)
pub fn touch() {
    let cpu = crate::cpu::current_cpu_id() as usize;
    if cpu < MAX_CPUS {
        unsafe {
            SOFT_LOCKUP[cpu].touch();
        }
    }
}

/// Touch for specific CPU
pub fn touch_cpu(cpu: usize) {
    if cpu < MAX_CPUS {
        unsafe {
            SOFT_LOCKUP[cpu].touch();
        }
    }
}

/// Check for soft lockup (called from timer interrupt)
pub fn check() -> Option<SoftLockupEvent> {
    let cpu = crate::cpu::current_cpu_id() as usize;
    if cpu < MAX_CPUS {
        unsafe {
            SOFT_LOCKUP[cpu].check()
        }
    } else {
        None
    }
}

/// Set current task for lockup reporting
pub fn set_current_task(task_id: u64) {
    let cpu = crate::cpu::current_cpu_id() as usize;
    if cpu < MAX_CPUS {
        unsafe {
            SOFT_LOCKUP[cpu].set_current_task(task_id);
        }
    }
}

/// Disable soft lockup detection on current CPU
pub fn disable() {
    let cpu = crate::cpu::current_cpu_id() as usize;
    if cpu < MAX_CPUS {
        unsafe {
            SOFT_LOCKUP[cpu].disable();
        }
    }
}

/// Enable soft lockup detection on current CPU
pub fn enable() {
    let cpu = crate::cpu::current_cpu_id() as usize;
    if cpu < MAX_CPUS {
        unsafe {
            SOFT_LOCKUP[cpu].enable();
        }
    }
}

/// Check if CPU is in soft lockup
pub fn is_in_lockup(cpu: usize) -> bool {
    if cpu < MAX_CPUS {
        unsafe {
            SOFT_LOCKUP[cpu].in_lockup.load(Ordering::Relaxed)
        }
    } else {
        false
    }
}

/// Get lockup duration for CPU
pub fn lockup_duration(cpu: usize) -> u64 {
    if cpu < MAX_CPUS {
        unsafe {
            SOFT_LOCKUP[cpu].lockup_duration.load(Ordering::Relaxed)
        }
    } else {
        0
    }
}
