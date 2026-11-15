// ===============================================================================
// BUILDOS KERNEL - HARD LOCKUP DETECTOR (NMI WATCHDOG)
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Hard lockup detection using NMI watchdog.
//!
//! Detects when a CPU is completely stuck, not even servicing interrupts.
//! Uses performance counter overflow to generate NMIs at regular intervals.

#![allow(dead_code)]

use core::sync::atomic::{AtomicBool, AtomicU64, AtomicU32, Ordering};

use crate::sched::MAX_CPUS;

// =============================================================================
// CONSTANTS
// =============================================================================

/// NMI watchdog sample period (in CPU cycles)
const NMI_SAMPLE_CYCLES: u64 = 2_000_000_000; // ~1 second at 2GHz

/// Maximum samples without progress before declaring hard lockup
const HARD_LOCKUP_THRESHOLD_SAMPLES: u32 = 10;

// =============================================================================
// STATE
// =============================================================================

/// Per-CPU hard lockup detector
static mut HARD_LOCKUP: [HardLockupDetector; MAX_CPUS] = {
    const INIT: HardLockupDetector = HardLockupDetector::new();
    [INIT; MAX_CPUS]
};

/// Is NMI watchdog supported?
static NMI_WATCHDOG_SUPPORTED: AtomicBool = AtomicBool::new(false);

/// Is NMI watchdog enabled globally?
static NMI_WATCHDOG_ENABLED: AtomicBool = AtomicBool::new(false);

/// Hard lockup detector for a CPU
pub struct HardLockupDetector {
    /// CPU ID
    cpu: AtomicU32,

    /// Is enabled on this CPU?
    enabled: AtomicBool,

    /// Last hrtimer interrupt count
    last_hrtimer_count: AtomicU64,

    /// Current hrtimer interrupt count
    current_hrtimer_count: AtomicU64,

    /// Samples without progress
    no_progress_samples: AtomicU32,

    /// Is in hard lockup?
    in_lockup: AtomicBool,

    /// Performance counter MSR
    perf_counter_msr: AtomicU32,

    /// NMI count
    nmi_count: AtomicU64,

    /// Last NMI timestamp
    last_nmi_ts: AtomicU64,

    /// RIP when lockup detected
    lockup_rip: AtomicU64,
}

impl HardLockupDetector {
    const fn new() -> Self {
        Self {
            cpu: AtomicU32::new(0),
            enabled: AtomicBool::new(false),
            last_hrtimer_count: AtomicU64::new(0),
            current_hrtimer_count: AtomicU64::new(0),
            no_progress_samples: AtomicU32::new(0),
            in_lockup: AtomicBool::new(false),
            perf_counter_msr: AtomicU32::new(0),
            nmi_count: AtomicU64::new(0),
            last_nmi_ts: AtomicU64::new(0),
            lockup_rip: AtomicU64::new(0),
        }
    }

    /// Initialize for a CPU
    pub fn init(&mut self, cpu: u32) {
        self.cpu.store(cpu, Ordering::Release);
    }

    /// Enable NMI watchdog on this CPU
    pub fn enable(&self) {
        if !NMI_WATCHDOG_SUPPORTED.load(Ordering::Relaxed) {
            return;
        }

        // Program performance counter to overflow at regular intervals
        // This generates an NMI when the counter overflows
        self.setup_perf_counter();

        self.enabled.store(true, Ordering::Release);
    }

    /// Disable NMI watchdog on this CPU
    pub fn disable(&self) {
        self.enabled.store(false, Ordering::Release);
        self.teardown_perf_counter();
    }

    /// Setup performance counter for NMI generation
    fn setup_perf_counter(&self) {
        // Would program the PMU:
        // 1. Select unhalted cycles event
        // 2. Enable interrupt on overflow
        // 3. Set initial count to generate NMI at desired rate

        // Example for Intel:
        // MSR_PERF_GLOBAL_CTRL = enable counter
        // MSR_PERFEVTSEL0 = UNHALTED_CYCLES | USR | OS | INT
        // MSR_PMC0 = -NMI_SAMPLE_CYCLES (to overflow)
    }

    /// Teardown performance counter
    fn teardown_perf_counter(&self) {
        // Would disable the PMU counter
    }

    /// Record hrtimer interrupt (for progress tracking)
    pub fn record_hrtimer(&self) {
        self.current_hrtimer_count.fetch_add(1, Ordering::Relaxed);
    }

    /// NMI handler - check for hard lockup
    pub fn nmi_handler(&self) -> Option<HardLockupEvent> {
        if !self.enabled.load(Ordering::Acquire) {
            return None;
        }

        self.nmi_count.fetch_add(1, Ordering::Relaxed);
        self.last_nmi_ts.store(crate::time::now_ns(), Ordering::Release);

        // Reprogram counter for next NMI
        self.reprogram_counter();

        // Check if hrtimer made progress since last NMI
        let current = self.current_hrtimer_count.load(Ordering::Acquire);
        let last = self.last_hrtimer_count.load(Ordering::Acquire);

        if current == last {
            // No progress - increment counter
            let no_progress = self.no_progress_samples.fetch_add(1, Ordering::Relaxed) + 1;

            if no_progress >= HARD_LOCKUP_THRESHOLD_SAMPLES {
                // Hard lockup detected!
                self.in_lockup.store(true, Ordering::Release);

                let rip = Self::capture_nmi_rip();
                self.lockup_rip.store(rip, Ordering::Release);

                return Some(HardLockupEvent {
                    cpu: self.cpu.load(Ordering::Relaxed) as usize,
                    samples_stuck: no_progress,
                    rip,
                    nmi_count: self.nmi_count.load(Ordering::Relaxed),
                });
            }
        } else {
            // Progress made - reset counter
            self.last_hrtimer_count.store(current, Ordering::Release);
            self.no_progress_samples.store(0, Ordering::Release);

            if self.in_lockup.load(Ordering::Acquire) {
                self.in_lockup.store(false, Ordering::Release);
            }
        }

        None
    }

    /// Reprogram counter for next NMI
    fn reprogram_counter(&self) {
        // Would write -NMI_SAMPLE_CYCLES to performance counter
    }

    /// Capture RIP from NMI stack frame
    fn capture_nmi_rip() -> u64 {
        // Would read RIP from NMI stack frame
        0
    }
}

/// Hard lockup event
pub struct HardLockupEvent {
    pub cpu: usize,
    pub samples_stuck: u32,
    pub rip: u64,
    pub nmi_count: u64,
}

// =============================================================================
// INTERFACE
// =============================================================================

/// Initialize hard lockup detection
pub fn init() {
    // Check if CPU supports performance counters for NMI
    if check_pmu_support() {
        NMI_WATCHDOG_SUPPORTED.store(true, Ordering::Release);
        NMI_WATCHDOG_ENABLED.store(true, Ordering::Release);
    }

    // Initialize per-CPU detectors
    let num_cpus = crate::sched::online_cpus();
    for cpu in 0..num_cpus {
        unsafe {
            HARD_LOCKUP[cpu].init(cpu as u32);
        }
    }
}

/// Check if PMU supports NMI watchdog
fn check_pmu_support() -> bool {
    // Would check CPUID for performance monitoring support
    // and verify we can use PMU for NMI generation
    true
}

/// Initialize for current CPU
pub fn init_cpu() {
    let cpu = crate::cpu::current_cpu_id() as usize;
    if cpu < MAX_CPUS {
        unsafe {
            HARD_LOCKUP[cpu].init(cpu as u32);
        }
    }
}

/// Enable NMI watchdog on a CPU
pub fn enable_cpu(cpu: usize) {
    if cpu < MAX_CPUS {
        unsafe {
            HARD_LOCKUP[cpu].enable();
        }
    }
}

/// Disable NMI watchdog on a CPU
pub fn disable_cpu(cpu: usize) {
    if cpu < MAX_CPUS {
        unsafe {
            HARD_LOCKUP[cpu].disable();
        }
    }
}

/// Record hrtimer interrupt
pub fn record_hrtimer() {
    let cpu = crate::cpu::current_cpu_id() as usize;
    if cpu < MAX_CPUS {
        unsafe {
            HARD_LOCKUP[cpu].record_hrtimer();
        }
    }
}

/// NMI handler
pub fn nmi_handler() -> Option<HardLockupEvent> {
    let cpu = crate::cpu::current_cpu_id() as usize;
    if cpu < MAX_CPUS {
        unsafe {
            HARD_LOCKUP[cpu].nmi_handler()
        }
    } else {
        None
    }
}

/// Check if NMI watchdog is supported
pub fn is_supported() -> bool {
    NMI_WATCHDOG_SUPPORTED.load(Ordering::Relaxed)
}

/// Check if NMI watchdog is enabled globally
pub fn is_enabled() -> bool {
    NMI_WATCHDOG_ENABLED.load(Ordering::Relaxed)
}

/// Enable NMI watchdog globally
pub fn enable() {
    NMI_WATCHDOG_ENABLED.store(true, Ordering::Release);
}

/// Disable NMI watchdog globally
pub fn disable() {
    NMI_WATCHDOG_ENABLED.store(false, Ordering::Release);
}

/// Check if CPU is in hard lockup
pub fn is_in_lockup(cpu: usize) -> bool {
    if cpu < MAX_CPUS {
        unsafe {
            HARD_LOCKUP[cpu].in_lockup.load(Ordering::Relaxed)
        }
    } else {
        false
    }
}

/// Get NMI count for CPU
pub fn nmi_count(cpu: usize) -> u64 {
    if cpu < MAX_CPUS {
        unsafe {
            HARD_LOCKUP[cpu].nmi_count.load(Ordering::Relaxed)
        }
    } else {
        0
    }
}
