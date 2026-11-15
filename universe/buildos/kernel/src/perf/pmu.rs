//! Performance Monitoring Unit (PMU) Support
//!
//! Hardware performance counter management for x86_64 processors.

#![allow(dead_code)]

use super::{PerfError, PerfEventAttr, PerfEventType};
use super::events::{HardwareEvent, CacheEvent};
use core::sync::atomic::{AtomicBool, Ordering};
use crate::sync::Mutex;

/// PMU types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PmuType {
    /// Core PMU (per-CPU performance counters)
    Core,
    /// Uncore PMU (shared resources like memory controller)
    Uncore,
    /// Software PMU
    Software,
    /// Tracepoint PMU
    Tracepoint,
    /// Kprobe PMU
    Kprobe,
    /// Uprobe PMU
    Uprobe,
    /// Raw breakpoint PMU
    Breakpoint,
}

/// x86 MSR addresses for performance monitoring
mod msr {
    // Core performance monitoring MSRs (Intel)
    pub const IA32_PMC0: u32 = 0xC1;
    pub const IA32_PMC1: u32 = 0xC2;
    pub const IA32_PMC2: u32 = 0xC3;
    pub const IA32_PMC3: u32 = 0xC4;
    pub const IA32_PMC4: u32 = 0xC5;
    pub const IA32_PMC5: u32 = 0xC6;
    pub const IA32_PMC6: u32 = 0xC7;
    pub const IA32_PMC7: u32 = 0xC8;

    pub const IA32_PERFEVTSEL0: u32 = 0x186;
    pub const IA32_PERFEVTSEL1: u32 = 0x187;
    pub const IA32_PERFEVTSEL2: u32 = 0x188;
    pub const IA32_PERFEVTSEL3: u32 = 0x189;
    pub const IA32_PERFEVTSEL4: u32 = 0x18A;
    pub const IA32_PERFEVTSEL5: u32 = 0x18B;
    pub const IA32_PERFEVTSEL6: u32 = 0x18C;
    pub const IA32_PERFEVTSEL7: u32 = 0x18D;

    pub const IA32_FIXED_CTR0: u32 = 0x309; // Instructions retired
    pub const IA32_FIXED_CTR1: u32 = 0x30A; // CPU cycles
    pub const IA32_FIXED_CTR2: u32 = 0x30B; // Reference cycles
    pub const IA32_FIXED_CTR_CTRL: u32 = 0x38D;

    pub const IA32_PERF_GLOBAL_CTRL: u32 = 0x38F;
    pub const IA32_PERF_GLOBAL_STATUS: u32 = 0x38E;
    pub const IA32_PERF_GLOBAL_OVF_CTRL: u32 = 0x390;

    pub const IA32_PEBS_ENABLE: u32 = 0x3F1;
    pub const IA32_DS_AREA: u32 = 0x600;

    // AMD MSRs
    pub const AMD_PERF_CTL0: u32 = 0xC0010000;
    pub const AMD_PERF_CTR0: u32 = 0xC0010004;
}

/// Performance event select bits
mod evtsel {
    pub const USR: u64 = 1 << 16;    // Count user mode
    pub const OS: u64 = 1 << 17;     // Count kernel mode
    pub const EDGE: u64 = 1 << 18;   // Edge detect
    pub const PC: u64 = 1 << 19;     // Pin control
    pub const INT: u64 = 1 << 20;    // APIC interrupt enable
    pub const ANY: u64 = 1 << 21;    // Count any thread (HT)
    pub const EN: u64 = 1 << 22;     // Enable counter
    pub const INV: u64 = 1 << 23;    // Invert counter mask
    pub const CMASK_SHIFT: u64 = 24; // Counter mask shift
}

/// Architectural performance events (Intel)
mod arch_events {
    pub const UNHALTED_CORE_CYCLES: u64 = 0x003C;
    pub const INSTRUCTION_RETIRED: u64 = 0x00C0;
    pub const UNHALTED_REFERENCE_CYCLES: u64 = 0x013C;
    pub const LLC_REFERENCE: u64 = 0x4F2E;
    pub const LLC_MISSES: u64 = 0x412E;
    pub const BRANCH_INSTRUCTION_RETIRED: u64 = 0x00C4;
    pub const BRANCH_MISSES_RETIRED: u64 = 0x00C5;
}

/// PMU type alias for backward compatibility
pub type Pmu = PmuInfo;

/// PMU information
#[derive(Clone, Copy, Debug)]
pub struct PmuInfo {
    /// PMU version
    pub version: u8,
    /// Number of general purpose counters
    pub num_counters: u8,
    /// Counter bit width
    pub counter_width: u8,
    /// Number of fixed counters
    pub num_fixed: u8,
    /// Fixed counter bit width
    pub fixed_width: u8,
    /// Supported events mask
    pub events_mask: u32,
    /// Vendor (Intel=1, AMD=2)
    pub vendor: u8,
}

/// Counter allocation state
#[derive(Clone, Copy, Debug)]
pub struct CounterState {
    /// Counter is allocated
    pub allocated: bool,
    /// Event type configured
    pub event_config: u64,
    /// Current count
    pub count: u64,
    /// Overflow count
    pub overflows: u64,
}

impl Default for CounterState {
    fn default() -> Self {
        Self {
            allocated: false,
            event_config: 0,
            count: 0,
            overflows: 0,
        }
    }
}

/// PMU state per CPU
pub struct PmuState {
    /// Counter states (general purpose)
    pub counters: [CounterState; 8],
    /// Fixed counter states
    pub fixed_counters: [CounterState; 4],
    /// Global enable mask
    pub global_ctrl: u64,
    /// PMU initialized
    pub initialized: bool,
}

impl PmuState {
    /// Create new PMU state
    pub const fn new() -> Self {
        Self {
            counters: [CounterState {
                allocated: false,
                event_config: 0,
                count: 0,
                overflows: 0,
            }; 8],
            fixed_counters: [CounterState {
                allocated: false,
                event_config: 0,
                count: 0,
                overflows: 0,
            }; 4],
            global_ctrl: 0,
            initialized: false,
        }
    }
}

/// Global PMU info
static mut PMU_INFO: PmuInfo = PmuInfo {
    version: 0,
    num_counters: 0,
    counter_width: 0,
    num_fixed: 0,
    fixed_width: 0,
    events_mask: 0,
    vendor: 0,
};

/// Per-CPU PMU state
static PMU_STATE: Mutex<[PmuState; 256]> = Mutex::new([const { PmuState::new() }; 256]);

/// PMU initialized flag
static PMU_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize PMU subsystem
pub fn init() {
    // Detect CPU vendor and PMU capabilities
    let cpuid_0 = cpuid(0);

    // Check vendor
    let vendor = detect_vendor(cpuid_0);

    // Get PMU info from CPUID
    let cpuid_a = cpuid(0x0A);

    let version = (cpuid_a.eax & 0xFF) as u8;
    let num_counters = ((cpuid_a.eax >> 8) & 0xFF) as u8;
    let counter_width = ((cpuid_a.eax >> 16) & 0xFF) as u8;
    let events_mask = cpuid_a.ebx;

    let num_fixed = (cpuid_a.edx & 0x1F) as u8;
    let fixed_width = ((cpuid_a.edx >> 5) & 0xFF) as u8;

    unsafe {
        PMU_INFO = PmuInfo {
            version,
            num_counters: num_counters.min(8),
            counter_width,
            num_fixed: num_fixed.min(4),
            fixed_width,
            events_mask,
            vendor,
        };
    }

    // Initialize per-CPU state for BSP
    {
        let mut state = PMU_STATE.lock();
        state[0].initialized = true;
    }

    // Disable all counters initially
    unsafe {
        if version > 0 {
            write_msr(msr::IA32_PERF_GLOBAL_CTRL, 0);
        }
    }

    PMU_INITIALIZED.store(true, Ordering::Release);

    crate::kprintln!("[PMU] Version {}, {} GP counters, {} fixed counters",
        version, num_counters, num_fixed);
}

/// Get PMU info
pub fn info() -> PmuInfo {
    unsafe { PMU_INFO }
}

/// Allocate a performance counter
pub fn allocate_counter(attr: &PerfEventAttr) -> Result<u32, PerfError> {
    if !PMU_INITIALIZED.load(Ordering::Acquire) {
        return Err(PerfError::NotSupported);
    }

    let cpu = crate::sync::get_cpu_id();
    let mut state = PMU_STATE.lock();
    let pmu = &mut state[cpu];

    let info = unsafe { PMU_INFO };

    // Try to use fixed counters for common events
    if attr.event_type == PerfEventType::Hardware {
        if let Some(hw_event) = HardwareEvent::from_config(attr.config) {
            match hw_event {
                HardwareEvent::Instructions => {
                    // Fixed counter 0
                    if !pmu.fixed_counters[0].allocated {
                        pmu.fixed_counters[0].allocated = true;
                        configure_fixed_counter(0, true, true)?;
                        return Ok(0x100); // Fixed counter marker
                    }
                }
                HardwareEvent::CpuCycles => {
                    // Fixed counter 1
                    if !pmu.fixed_counters[1].allocated {
                        pmu.fixed_counters[1].allocated = true;
                        configure_fixed_counter(1, true, true)?;
                        return Ok(0x101);
                    }
                }
                HardwareEvent::RefCpuCycles => {
                    // Fixed counter 2
                    if !pmu.fixed_counters[2].allocated {
                        pmu.fixed_counters[2].allocated = true;
                        configure_fixed_counter(2, true, true)?;
                        return Ok(0x102);
                    }
                }
                _ => {}
            }
        }
    }

    // Allocate general purpose counter
    for i in 0..info.num_counters as usize {
        if !pmu.counters[i].allocated {
            pmu.counters[i].allocated = true;

            // Configure the counter
            let event_config = translate_event(attr)?;
            pmu.counters[i].event_config = event_config;

            configure_gp_counter(i as u32, event_config, attr)?;

            return Ok(i as u32);
        }
    }

    Err(PerfError::NoResources)
}

/// Free a performance counter
pub fn free_counter(index: u32) -> Result<(), PerfError> {
    let cpu = crate::sync::get_cpu_id();
    let mut state = PMU_STATE.lock();
    let pmu = &mut state[cpu];

    if index >= 0x100 {
        // Fixed counter
        let fixed_idx = (index - 0x100) as usize;
        if fixed_idx < 4 {
            pmu.fixed_counters[fixed_idx].allocated = false;
            configure_fixed_counter(fixed_idx as u32, false, false)?;
        }
    } else {
        // GP counter
        let gp_idx = index as usize;
        if gp_idx < 8 {
            pmu.counters[gp_idx].allocated = false;
            disable_gp_counter(index)?;
        }
    }

    Ok(())
}

/// Enable a counter
pub fn enable_counter(index: u32) -> Result<(), PerfError> {
    if index >= 0x100 {
        let fixed_idx = index - 0x100;
        enable_fixed_counter(fixed_idx)
    } else {
        enable_gp_counter(index)
    }
}

/// Disable a counter
pub fn disable_counter(index: u32) -> Result<(), PerfError> {
    if index >= 0x100 {
        let fixed_idx = index - 0x100;
        disable_fixed_counter(fixed_idx)
    } else {
        disable_gp_counter(index)
    }
}

/// Read a counter value
pub fn read_counter(index: u32) -> Result<u64, PerfError> {
    if index >= 0x100 {
        let fixed_idx = index - 0x100;
        read_fixed_counter(fixed_idx)
    } else {
        read_gp_counter(index)
    }
}

/// Reset a counter
pub fn reset_counter(index: u32) -> Result<(), PerfError> {
    if index >= 0x100 {
        let fixed_idx = index - 0x100;
        write_fixed_counter(fixed_idx, 0)
    } else {
        write_gp_counter(index, 0)
    }
}

/// Configure general purpose counter
fn configure_gp_counter(index: u32, event_config: u64, attr: &PerfEventAttr) -> Result<(), PerfError> {
    let evtsel_msr = msr::IA32_PERFEVTSEL0 + index;
    let pmc_msr = msr::IA32_PMC0 + index;

    let mut config = event_config;

    // Add privilege level bits
    if !attr.exclude_user {
        config |= evtsel::USR;
    }
    if !attr.exclude_kernel {
        config |= evtsel::OS;
    }

    // Enable counter
    config |= evtsel::EN;

    unsafe {
        // Clear counter
        write_msr(pmc_msr, 0);
        // Write event select
        write_msr(evtsel_msr, config);
    }

    Ok(())
}

/// Enable general purpose counter
fn enable_gp_counter(index: u32) -> Result<(), PerfError> {
    let info = unsafe { PMU_INFO };
    if info.version > 0 {
        unsafe {
            let ctrl = read_msr(msr::IA32_PERF_GLOBAL_CTRL);
            write_msr(msr::IA32_PERF_GLOBAL_CTRL, ctrl | (1 << index));
        }
    }
    Ok(())
}

/// Disable general purpose counter
fn disable_gp_counter(index: u32) -> Result<(), PerfError> {
    let info = unsafe { PMU_INFO };
    if info.version > 0 {
        unsafe {
            let ctrl = read_msr(msr::IA32_PERF_GLOBAL_CTRL);
            write_msr(msr::IA32_PERF_GLOBAL_CTRL, ctrl & !(1 << index));
        }
    }

    // Also clear event select
    let evtsel_msr = msr::IA32_PERFEVTSEL0 + index;
    unsafe {
        write_msr(evtsel_msr, 0);
    }

    Ok(())
}

/// Read general purpose counter
fn read_gp_counter(index: u32) -> Result<u64, PerfError> {
    let pmc_msr = msr::IA32_PMC0 + index;
    Ok(unsafe { read_msr(pmc_msr) })
}

/// Write general purpose counter
fn write_gp_counter(index: u32, value: u64) -> Result<(), PerfError> {
    let pmc_msr = msr::IA32_PMC0 + index;
    unsafe { write_msr(pmc_msr, value) };
    Ok(())
}

/// Configure fixed counter
fn configure_fixed_counter(index: u32, user: bool, kernel: bool) -> Result<(), PerfError> {
    let shift = index * 4;
    let mut ctrl = unsafe { read_msr(msr::IA32_FIXED_CTR_CTRL) };

    // Clear bits for this counter
    ctrl &= !(0xF << shift);

    // Set enable bits
    if user {
        ctrl |= 1 << shift; // Enable user
    }
    if kernel {
        ctrl |= 2 << shift; // Enable kernel
    }

    unsafe { write_msr(msr::IA32_FIXED_CTR_CTRL, ctrl) };
    Ok(())
}

/// Enable fixed counter in global ctrl
fn enable_fixed_counter(index: u32) -> Result<(), PerfError> {
    unsafe {
        let ctrl = read_msr(msr::IA32_PERF_GLOBAL_CTRL);
        write_msr(msr::IA32_PERF_GLOBAL_CTRL, ctrl | (1 << (32 + index)));
    }
    Ok(())
}

/// Disable fixed counter
fn disable_fixed_counter(index: u32) -> Result<(), PerfError> {
    unsafe {
        let ctrl = read_msr(msr::IA32_PERF_GLOBAL_CTRL);
        write_msr(msr::IA32_PERF_GLOBAL_CTRL, ctrl & !(1 << (32 + index)));
    }
    Ok(())
}

/// Read fixed counter
fn read_fixed_counter(index: u32) -> Result<u64, PerfError> {
    let msr_addr = msr::IA32_FIXED_CTR0 + index;
    Ok(unsafe { read_msr(msr_addr) })
}

/// Write fixed counter
fn write_fixed_counter(index: u32, value: u64) -> Result<(), PerfError> {
    let msr_addr = msr::IA32_FIXED_CTR0 + index;
    unsafe { write_msr(msr_addr, value) };
    Ok(())
}

/// Translate generic event to PMU-specific config
fn translate_event(attr: &PerfEventAttr) -> Result<u64, PerfError> {
    match attr.event_type {
        PerfEventType::Hardware => {
            let hw_event = HardwareEvent::from_config(attr.config)
                .ok_or(PerfError::InvalidEvent)?;

            // Intel architectural events
            Ok(match hw_event {
                HardwareEvent::CpuCycles => arch_events::UNHALTED_CORE_CYCLES,
                HardwareEvent::Instructions => arch_events::INSTRUCTION_RETIRED,
                HardwareEvent::CacheReferences => arch_events::LLC_REFERENCE,
                HardwareEvent::CacheMisses => arch_events::LLC_MISSES,
                HardwareEvent::BranchInstructions => arch_events::BRANCH_INSTRUCTION_RETIRED,
                HardwareEvent::BranchMisses => arch_events::BRANCH_MISSES_RETIRED,
                HardwareEvent::BusCycles => arch_events::UNHALTED_REFERENCE_CYCLES,
                HardwareEvent::RefCpuCycles => arch_events::UNHALTED_REFERENCE_CYCLES,
                _ => return Err(PerfError::NotSupported),
            })
        }
        PerfEventType::HardwareCache => {
            let cache_event = CacheEvent::decode(attr.config)
                .ok_or(PerfError::InvalidEvent)?;

            // Translate to Intel event
            translate_cache_event(&cache_event)
        }
        PerfEventType::Raw => {
            // Raw event, use config directly
            Ok(attr.config)
        }
        _ => Err(PerfError::InvalidEvent),
    }
}

/// Translate cache event to PMU config
fn translate_cache_event(event: &CacheEvent) -> Result<u64, PerfError> {
    use super::events::{CacheId, CacheOp, CacheResult};

    // Intel-specific cache event encoding
    let config = match (event.cache_id, event.op, event.result) {
        // L1D events
        (CacheId::L1D, CacheOp::Read, CacheResult::Access) => 0x0143, // MEM_LOAD_RETIRED.L1_HIT
        (CacheId::L1D, CacheOp::Read, CacheResult::Miss) => 0x0108,   // MEM_LOAD_RETIRED.L1_MISS
        (CacheId::L1D, CacheOp::Write, CacheResult::Access) => 0x0243, // L1D write access
        (CacheId::L1D, CacheOp::Write, CacheResult::Miss) => 0x0128,   // L1D write miss

        // L1I events
        (CacheId::L1I, CacheOp::Read, CacheResult::Access) => 0x0380, // ICACHE read
        (CacheId::L1I, CacheOp::Read, CacheResult::Miss) => 0x0280,   // ICACHE miss

        // LLC events
        (CacheId::LL, _, CacheResult::Access) => arch_events::LLC_REFERENCE,
        (CacheId::LL, _, CacheResult::Miss) => arch_events::LLC_MISSES,

        // DTLB events
        (CacheId::DTLB, CacheOp::Read, CacheResult::Access) => 0x0108, // DTLB load
        (CacheId::DTLB, CacheOp::Read, CacheResult::Miss) => 0x0149,   // DTLB load miss
        (CacheId::DTLB, CacheOp::Write, CacheResult::Access) => 0x0208, // DTLB store
        (CacheId::DTLB, CacheOp::Write, CacheResult::Miss) => 0x0249,  // DTLB store miss

        // ITLB events
        (CacheId::ITLB, CacheOp::Read, CacheResult::Access) => 0x0185, // ITLB access
        (CacheId::ITLB, CacheOp::Read, CacheResult::Miss) => 0x0185,   // ITLB miss

        // BPU events
        (CacheId::BPU, _, CacheResult::Access) => arch_events::BRANCH_INSTRUCTION_RETIRED,
        (CacheId::BPU, _, CacheResult::Miss) => arch_events::BRANCH_MISSES_RETIRED,

        _ => return Err(PerfError::NotSupported),
    };

    Ok(config)
}

/// Handle PMU overflow interrupt
pub fn handle_overflow() {
    let info = unsafe { PMU_INFO };
    if info.version == 0 {
        return;
    }

    // Read overflow status
    let status = unsafe { read_msr(msr::IA32_PERF_GLOBAL_STATUS) };

    if status == 0 {
        return;
    }

    // Handle each overflowed counter
    for i in 0..info.num_counters {
        if status & (1 << i) != 0 {
            // Counter i overflowed - would trigger sampling here
            let cpu = crate::sync::get_cpu_id();
            let mut state = PMU_STATE.lock();
            state[cpu].counters[i as usize].overflows += 1;
        }
    }

    // Handle fixed counter overflows
    for i in 0..info.num_fixed {
        if status & (1 << (32 + i)) != 0 {
            let cpu = crate::sync::get_cpu_id();
            let mut state = PMU_STATE.lock();
            state[cpu].fixed_counters[i as usize].overflows += 1;
        }
    }

    // Clear overflow status
    unsafe {
        write_msr(msr::IA32_PERF_GLOBAL_OVF_CTRL, status);
    }
}

/// CPUID result
#[derive(Clone, Copy)]
struct CpuidResult {
    eax: u32,
    ebx: u32,
    ecx: u32,
    edx: u32,
}

/// Execute CPUID instruction
fn cpuid(leaf: u32) -> CpuidResult {
    let (eax, ebx, ecx, edx): (u32, u32, u32, u32);
    unsafe {
        core::arch::asm!(
            "push rbx",
            "cpuid",
            "mov {ebx_out:e}, ebx",
            "pop rbx",
            inout("eax") leaf => eax,
            ebx_out = out(reg) ebx,
            inout("ecx") 0u32 => ecx,
            out("edx") edx,
        );
    }
    CpuidResult { eax, ebx, ecx, edx }
}

/// Detect CPU vendor
fn detect_vendor(cpuid_0: CpuidResult) -> u8 {
    // Check vendor string
    let ebx = cpuid_0.ebx;
    let ecx = cpuid_0.ecx;
    let edx = cpuid_0.edx;

    // "GenuineIntel" -> ebx=0x756e6547, edx=0x49656e69, ecx=0x6c65746e
    if ebx == 0x756e6547 && edx == 0x49656e69 && ecx == 0x6c65746e {
        1 // Intel
    } else if ebx == 0x68747541 { // "Auth" from AuthenticAMD
        2 // AMD
    } else {
        0 // Unknown
    }
}

/// Read MSR
#[inline]
unsafe fn read_msr(msr: u32) -> u64 {
    let (low, high): (u32, u32);
    core::arch::asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") low,
        out("edx") high,
        options(nomem, nostack)
    );
    ((high as u64) << 32) | (low as u64)
}

/// Write MSR
#[inline]
unsafe fn write_msr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    core::arch::asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") low,
        in("edx") high,
        options(nomem, nostack)
    );
}
