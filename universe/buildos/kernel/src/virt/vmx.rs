// ===============================================================================
// BUILDOS KERNEL - VMX (Intel VT-x) SUPPORT
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Intel VT-x (VMX) Support
//!
//! Implements Intel virtualization extensions:
//! - VMXON/VMXOFF operations
//! - VMCS management
//! - VM entries and exits
//! - EPT (Extended Page Tables)

#![allow(dead_code)]

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use super::KvmError;

// =============================================================================
// VMX MSRs
// =============================================================================

/// MSR addresses
mod msr {
    pub const IA32_FEATURE_CONTROL: u32 = 0x3A;
    pub const IA32_VMX_BASIC: u32 = 0x480;
    pub const IA32_VMX_PINBASED_CTLS: u32 = 0x481;
    pub const IA32_VMX_PROCBASED_CTLS: u32 = 0x482;
    pub const IA32_VMX_EXIT_CTLS: u32 = 0x483;
    pub const IA32_VMX_ENTRY_CTLS: u32 = 0x484;
    pub const IA32_VMX_MISC: u32 = 0x485;
    pub const IA32_VMX_CR0_FIXED0: u32 = 0x486;
    pub const IA32_VMX_CR0_FIXED1: u32 = 0x487;
    pub const IA32_VMX_CR4_FIXED0: u32 = 0x488;
    pub const IA32_VMX_CR4_FIXED1: u32 = 0x489;
    pub const IA32_VMX_VMCS_ENUM: u32 = 0x48A;
    pub const IA32_VMX_PROCBASED_CTLS2: u32 = 0x48B;
    pub const IA32_VMX_EPT_VPID_CAP: u32 = 0x48C;
    pub const IA32_VMX_TRUE_PINBASED_CTLS: u32 = 0x48D;
    pub const IA32_VMX_TRUE_PROCBASED_CTLS: u32 = 0x48E;
    pub const IA32_VMX_TRUE_EXIT_CTLS: u32 = 0x48F;
    pub const IA32_VMX_TRUE_ENTRY_CTLS: u32 = 0x490;
    pub const IA32_VMX_VMFUNC: u32 = 0x491;
}

/// Feature control MSR bits
mod feature_control {
    pub const LOCK: u64 = 1 << 0;
    pub const VMXON_INSIDE_SMX: u64 = 1 << 1;
    pub const VMXON_OUTSIDE_SMX: u64 = 1 << 2;
}

// =============================================================================
// VMX STATE
// =============================================================================

/// VMX enabled on current CPU
static VMX_ENABLED: AtomicBool = AtomicBool::new(false);

/// VMXON region physical address
static VMXON_REGION: AtomicU64 = AtomicU64::new(0);

// =============================================================================
// VMCS ENCODING
// =============================================================================

/// VMCS field encodings
pub mod vmcs {
    // 16-bit control fields
    pub const VIRTUAL_PROCESSOR_ID: u32 = 0x0000;
    pub const POSTED_INTR_NV: u32 = 0x0002;
    pub const EPTP_INDEX: u32 = 0x0004;

    // 16-bit guest state fields
    pub const GUEST_ES_SELECTOR: u32 = 0x0800;
    pub const GUEST_CS_SELECTOR: u32 = 0x0802;
    pub const GUEST_SS_SELECTOR: u32 = 0x0804;
    pub const GUEST_DS_SELECTOR: u32 = 0x0806;
    pub const GUEST_FS_SELECTOR: u32 = 0x0808;
    pub const GUEST_GS_SELECTOR: u32 = 0x080A;
    pub const GUEST_LDTR_SELECTOR: u32 = 0x080C;
    pub const GUEST_TR_SELECTOR: u32 = 0x080E;
    pub const GUEST_INTR_STATUS: u32 = 0x0810;
    pub const GUEST_PML_INDEX: u32 = 0x0812;

    // 16-bit host state fields
    pub const HOST_ES_SELECTOR: u32 = 0x0C00;
    pub const HOST_CS_SELECTOR: u32 = 0x0C02;
    pub const HOST_SS_SELECTOR: u32 = 0x0C04;
    pub const HOST_DS_SELECTOR: u32 = 0x0C06;
    pub const HOST_FS_SELECTOR: u32 = 0x0C08;
    pub const HOST_GS_SELECTOR: u32 = 0x0C0A;
    pub const HOST_TR_SELECTOR: u32 = 0x0C0C;

    // 64-bit control fields
    pub const IO_BITMAP_A: u32 = 0x2000;
    pub const IO_BITMAP_B: u32 = 0x2002;
    pub const MSR_BITMAP: u32 = 0x2004;
    pub const VM_EXIT_MSR_STORE_ADDR: u32 = 0x2006;
    pub const VM_EXIT_MSR_LOAD_ADDR: u32 = 0x2008;
    pub const VM_ENTRY_MSR_LOAD_ADDR: u32 = 0x200A;
    pub const EXECUTIVE_VMCS_PTR: u32 = 0x200C;
    pub const PML_ADDRESS: u32 = 0x200E;
    pub const TSC_OFFSET: u32 = 0x2010;
    pub const VIRTUAL_APIC_PAGE_ADDR: u32 = 0x2012;
    pub const APIC_ACCESS_ADDR: u32 = 0x2014;
    pub const POSTED_INTR_DESC_ADDR: u32 = 0x2016;
    pub const VM_FUNCTION_CONTROLS: u32 = 0x2018;
    pub const EPT_POINTER: u32 = 0x201A;
    pub const EOI_EXIT_BITMAP0: u32 = 0x201C;
    pub const EOI_EXIT_BITMAP1: u32 = 0x201E;
    pub const EOI_EXIT_BITMAP2: u32 = 0x2020;
    pub const EOI_EXIT_BITMAP3: u32 = 0x2022;
    pub const EPTP_LIST_ADDR: u32 = 0x2024;
    pub const VMREAD_BITMAP: u32 = 0x2026;
    pub const VMWRITE_BITMAP: u32 = 0x2028;
    pub const XSS_EXIT_BITMAP: u32 = 0x202C;
    pub const ENCLS_EXITING_BITMAP: u32 = 0x202E;
    pub const TSC_MULTIPLIER: u32 = 0x2032;

    // 64-bit read-only data fields
    pub const GUEST_PHYSICAL_ADDRESS: u32 = 0x2400;

    // 64-bit guest state fields
    pub const VMCS_LINK_POINTER: u32 = 0x2800;
    pub const GUEST_IA32_DEBUGCTL: u32 = 0x2802;
    pub const GUEST_IA32_PAT: u32 = 0x2804;
    pub const GUEST_IA32_EFER: u32 = 0x2806;
    pub const GUEST_IA32_PERF_GLOBAL_CTRL: u32 = 0x2808;
    pub const GUEST_PDPTE0: u32 = 0x280A;
    pub const GUEST_PDPTE1: u32 = 0x280C;
    pub const GUEST_PDPTE2: u32 = 0x280E;
    pub const GUEST_PDPTE3: u32 = 0x2810;
    pub const GUEST_BNDCFGS: u32 = 0x2812;
    pub const GUEST_IA32_RTIT_CTL: u32 = 0x2814;

    // 64-bit host state fields
    pub const HOST_IA32_PAT: u32 = 0x2C00;
    pub const HOST_IA32_EFER: u32 = 0x2C02;
    pub const HOST_IA32_PERF_GLOBAL_CTRL: u32 = 0x2C04;

    // 32-bit control fields
    pub const PIN_BASED_VM_EXEC_CONTROL: u32 = 0x4000;
    pub const CPU_BASED_VM_EXEC_CONTROL: u32 = 0x4002;
    pub const EXCEPTION_BITMAP: u32 = 0x4004;
    pub const PAGE_FAULT_ERROR_CODE_MASK: u32 = 0x4006;
    pub const PAGE_FAULT_ERROR_CODE_MATCH: u32 = 0x4008;
    pub const CR3_TARGET_COUNT: u32 = 0x400A;
    pub const VM_EXIT_CONTROLS: u32 = 0x400C;
    pub const VM_EXIT_MSR_STORE_COUNT: u32 = 0x400E;
    pub const VM_EXIT_MSR_LOAD_COUNT: u32 = 0x4010;
    pub const VM_ENTRY_CONTROLS: u32 = 0x4012;
    pub const VM_ENTRY_MSR_LOAD_COUNT: u32 = 0x4014;
    pub const VM_ENTRY_INTR_INFO_FIELD: u32 = 0x4016;
    pub const VM_ENTRY_EXCEPTION_ERROR_CODE: u32 = 0x4018;
    pub const VM_ENTRY_INSTRUCTION_LEN: u32 = 0x401A;
    pub const TPR_THRESHOLD: u32 = 0x401C;
    pub const SECONDARY_VM_EXEC_CONTROL: u32 = 0x401E;
    pub const PLE_GAP: u32 = 0x4020;
    pub const PLE_WINDOW: u32 = 0x4022;

    // 32-bit read-only data fields
    pub const VM_INSTRUCTION_ERROR: u32 = 0x4400;
    pub const VM_EXIT_REASON: u32 = 0x4402;
    pub const VM_EXIT_INTR_INFO: u32 = 0x4404;
    pub const VM_EXIT_INTR_ERROR_CODE: u32 = 0x4406;
    pub const IDT_VECTORING_INFO_FIELD: u32 = 0x4408;
    pub const IDT_VECTORING_ERROR_CODE: u32 = 0x440A;
    pub const VM_EXIT_INSTRUCTION_LEN: u32 = 0x440C;
    pub const VM_EXIT_INSTRUCTION_INFO: u32 = 0x440E;

    // 32-bit guest state fields
    pub const GUEST_ES_LIMIT: u32 = 0x4800;
    pub const GUEST_CS_LIMIT: u32 = 0x4802;
    pub const GUEST_SS_LIMIT: u32 = 0x4804;
    pub const GUEST_DS_LIMIT: u32 = 0x4806;
    pub const GUEST_FS_LIMIT: u32 = 0x4808;
    pub const GUEST_GS_LIMIT: u32 = 0x480A;
    pub const GUEST_LDTR_LIMIT: u32 = 0x480C;
    pub const GUEST_TR_LIMIT: u32 = 0x480E;
    pub const GUEST_GDTR_LIMIT: u32 = 0x4810;
    pub const GUEST_IDTR_LIMIT: u32 = 0x4812;
    pub const GUEST_ES_AR_BYTES: u32 = 0x4814;
    pub const GUEST_CS_AR_BYTES: u32 = 0x4816;
    pub const GUEST_SS_AR_BYTES: u32 = 0x4818;
    pub const GUEST_DS_AR_BYTES: u32 = 0x481A;
    pub const GUEST_FS_AR_BYTES: u32 = 0x481C;
    pub const GUEST_GS_AR_BYTES: u32 = 0x481E;
    pub const GUEST_LDTR_AR_BYTES: u32 = 0x4820;
    pub const GUEST_TR_AR_BYTES: u32 = 0x4822;
    pub const GUEST_INTERRUPTIBILITY_INFO: u32 = 0x4824;
    pub const GUEST_ACTIVITY_STATE: u32 = 0x4826;
    pub const GUEST_SMBASE: u32 = 0x4828;
    pub const GUEST_SYSENTER_CS: u32 = 0x482A;
    pub const GUEST_PREEMPTION_TIMER: u32 = 0x482E;

    // 32-bit host state fields
    pub const HOST_IA32_SYSENTER_CS: u32 = 0x4C00;

    // Natural-width control fields
    pub const CR0_GUEST_HOST_MASK: u32 = 0x6000;
    pub const CR4_GUEST_HOST_MASK: u32 = 0x6002;
    pub const CR0_READ_SHADOW: u32 = 0x6004;
    pub const CR4_READ_SHADOW: u32 = 0x6006;
    pub const CR3_TARGET_VALUE0: u32 = 0x6008;
    pub const CR3_TARGET_VALUE1: u32 = 0x600A;
    pub const CR3_TARGET_VALUE2: u32 = 0x600C;
    pub const CR3_TARGET_VALUE3: u32 = 0x600E;

    // Natural-width read-only data fields
    pub const EXIT_QUALIFICATION: u32 = 0x6400;
    pub const IO_RCX: u32 = 0x6402;
    pub const IO_RSI: u32 = 0x6404;
    pub const IO_RDI: u32 = 0x6406;
    pub const IO_RIP: u32 = 0x6408;
    pub const GUEST_LINEAR_ADDRESS: u32 = 0x640A;

    // Natural-width guest state fields
    pub const GUEST_CR0: u32 = 0x6800;
    pub const GUEST_CR3: u32 = 0x6802;
    pub const GUEST_CR4: u32 = 0x6804;
    pub const GUEST_ES_BASE: u32 = 0x6806;
    pub const GUEST_CS_BASE: u32 = 0x6808;
    pub const GUEST_SS_BASE: u32 = 0x680A;
    pub const GUEST_DS_BASE: u32 = 0x680C;
    pub const GUEST_FS_BASE: u32 = 0x680E;
    pub const GUEST_GS_BASE: u32 = 0x6810;
    pub const GUEST_LDTR_BASE: u32 = 0x6812;
    pub const GUEST_TR_BASE: u32 = 0x6814;
    pub const GUEST_GDTR_BASE: u32 = 0x6816;
    pub const GUEST_IDTR_BASE: u32 = 0x6818;
    pub const GUEST_DR7: u32 = 0x681A;
    pub const GUEST_RSP: u32 = 0x681C;
    pub const GUEST_RIP: u32 = 0x681E;
    pub const GUEST_RFLAGS: u32 = 0x6820;
    pub const GUEST_PENDING_DBG_EXCEPTIONS: u32 = 0x6822;
    pub const GUEST_SYSENTER_ESP: u32 = 0x6824;
    pub const GUEST_SYSENTER_EIP: u32 = 0x6826;
    // Note: RAX and other GPRs are not in VMCS but saved/restored manually
    // Using a custom encoding for software save area
    pub const GUEST_RAX: u32 = 0x6828;

    // Natural-width host state fields
    pub const HOST_CR0: u32 = 0x6C00;
    pub const HOST_CR3: u32 = 0x6C02;
    pub const HOST_CR4: u32 = 0x6C04;
    pub const HOST_FS_BASE: u32 = 0x6C06;
    pub const HOST_GS_BASE: u32 = 0x6C08;
    pub const HOST_TR_BASE: u32 = 0x6C0A;
    pub const HOST_GDTR_BASE: u32 = 0x6C0C;
    pub const HOST_IDTR_BASE: u32 = 0x6C0E;
    pub const HOST_IA32_SYSENTER_ESP: u32 = 0x6C10;
    pub const HOST_IA32_SYSENTER_EIP: u32 = 0x6C12;
    pub const HOST_RSP: u32 = 0x6C14;
    pub const HOST_RIP: u32 = 0x6C16;
}

/// VM exit reasons
pub mod exit_reason {
    pub const EXCEPTION_NMI: u32 = 0;
    pub const EXTERNAL_INTERRUPT: u32 = 1;
    pub const TRIPLE_FAULT: u32 = 2;
    pub const INIT: u32 = 3;
    pub const SIPI: u32 = 4;
    pub const IO_SMI: u32 = 5;
    pub const OTHER_SMI: u32 = 6;
    pub const PENDING_VIRT_INTR: u32 = 7;
    pub const PENDING_VIRT_NMI: u32 = 8;
    pub const TASK_SWITCH: u32 = 9;
    pub const CPUID: u32 = 10;
    pub const GETSEC: u32 = 11;
    pub const HLT: u32 = 12;
    pub const INVD: u32 = 13;
    pub const INVLPG: u32 = 14;
    pub const RDPMC: u32 = 15;
    pub const RDTSC: u32 = 16;
    pub const RSM: u32 = 17;
    pub const VMCALL: u32 = 18;
    pub const VMCLEAR: u32 = 19;
    pub const VMLAUNCH: u32 = 20;
    pub const VMPTRLD: u32 = 21;
    pub const VMPTRST: u32 = 22;
    pub const VMREAD: u32 = 23;
    pub const VMRESUME: u32 = 24;
    pub const VMWRITE: u32 = 25;
    pub const VMXOFF: u32 = 26;
    pub const VMXON: u32 = 27;
    pub const CR_ACCESS: u32 = 28;
    pub const MOV_DR: u32 = 29;
    pub const IO_INSTRUCTION: u32 = 30;
    pub const RDMSR: u32 = 31;
    pub const WRMSR: u32 = 32;
    pub const INVALID_GUEST_STATE: u32 = 33;
    pub const MSR_LOADING: u32 = 34;
    pub const MWAIT_INSTRUCTION: u32 = 36;
    pub const MONITOR_TRAP_FLAG: u32 = 37;
    pub const MONITOR_INSTRUCTION: u32 = 39;
    pub const PAUSE_INSTRUCTION: u32 = 40;
    pub const MCE_DURING_VMENTRY: u32 = 41;
    pub const TPR_BELOW_THRESHOLD: u32 = 43;
    pub const APIC_ACCESS: u32 = 44;
    pub const VIRTUALIZED_EOI: u32 = 45;
    pub const GDTR_IDTR: u32 = 46;
    pub const LDTR_TR: u32 = 47;
    pub const EPT_VIOLATION: u32 = 48;
    pub const EPT_MISCONFIG: u32 = 49;
    pub const INVEPT: u32 = 50;
    pub const RDTSCP: u32 = 51;
    pub const PREEMPTION_TIMER: u32 = 52;
    pub const INVVPID: u32 = 53;
    pub const WBINVD: u32 = 54;
    pub const XSETBV: u32 = 55;
    pub const APIC_WRITE: u32 = 56;
    pub const RDRAND: u32 = 57;
    pub const INVPCID: u32 = 58;
    pub const VMFUNC: u32 = 59;
    pub const ENCLS: u32 = 60;
    pub const RDSEED: u32 = 61;
    pub const PML_FULL: u32 = 62;
    pub const XSAVES: u32 = 63;
    pub const XRSTORS: u32 = 64;
}

/// Pin-based VM execution controls
pub mod pin_based {
    pub const EXTERNAL_INTERRUPT_EXITING: u32 = 1 << 0;
    pub const NMI_EXITING: u32 = 1 << 3;
    pub const VIRTUAL_NMIS: u32 = 1 << 5;
    pub const PREEMPTION_TIMER: u32 = 1 << 6;
    pub const POSTED_INTERRUPTS: u32 = 1 << 7;
}

/// Primary processor-based VM execution controls
pub mod cpu_based {
    pub const INTERRUPT_WINDOW_EXITING: u32 = 1 << 2;
    pub const USE_TSC_OFFSETTING: u32 = 1 << 3;
    pub const HLT_EXITING: u32 = 1 << 7;
    pub const INVLPG_EXITING: u32 = 1 << 9;
    pub const MWAIT_EXITING: u32 = 1 << 10;
    pub const RDPMC_EXITING: u32 = 1 << 11;
    pub const RDTSC_EXITING: u32 = 1 << 12;
    pub const CR3_LOAD_EXITING: u32 = 1 << 15;
    pub const CR3_STORE_EXITING: u32 = 1 << 16;
    pub const CR8_LOAD_EXITING: u32 = 1 << 19;
    pub const CR8_STORE_EXITING: u32 = 1 << 20;
    pub const USE_TPR_SHADOW: u32 = 1 << 21;
    pub const NMI_WINDOW_EXITING: u32 = 1 << 22;
    pub const MOV_DR_EXITING: u32 = 1 << 23;
    pub const UNCONDITIONAL_IO_EXITING: u32 = 1 << 24;
    pub const USE_IO_BITMAPS: u32 = 1 << 25;
    pub const MONITOR_TRAP_FLAG: u32 = 1 << 27;
    pub const USE_MSR_BITMAPS: u32 = 1 << 28;
    pub const MONITOR_EXITING: u32 = 1 << 29;
    pub const PAUSE_EXITING: u32 = 1 << 30;
    pub const ACTIVATE_SECONDARY_CONTROLS: u32 = 1 << 31;
}

/// Secondary processor-based VM execution controls
pub mod cpu_based2 {
    pub const VIRTUALIZE_APIC_ACCESSES: u32 = 1 << 0;
    pub const ENABLE_EPT: u32 = 1 << 1;
    pub const DESCRIPTOR_TABLE_EXITING: u32 = 1 << 2;
    pub const ENABLE_RDTSCP: u32 = 1 << 3;
    pub const VIRTUALIZE_X2APIC_MODE: u32 = 1 << 4;
    pub const ENABLE_VPID: u32 = 1 << 5;
    pub const WBINVD_EXITING: u32 = 1 << 6;
    pub const UNRESTRICTED_GUEST: u32 = 1 << 7;
    pub const APIC_REGISTER_VIRTUALIZATION: u32 = 1 << 8;
    pub const VIRTUAL_INTERRUPT_DELIVERY: u32 = 1 << 9;
    pub const PAUSE_LOOP_EXITING: u32 = 1 << 10;
    pub const RDRAND_EXITING: u32 = 1 << 11;
    pub const ENABLE_INVPCID: u32 = 1 << 12;
    pub const ENABLE_VMFUNC: u32 = 1 << 13;
    pub const VMCS_SHADOWING: u32 = 1 << 14;
    pub const ENABLE_ENCLS_EXITING: u32 = 1 << 15;
    pub const RDSEED_EXITING: u32 = 1 << 16;
    pub const ENABLE_PML: u32 = 1 << 17;
    pub const EPT_VIOLATION_VE: u32 = 1 << 18;
    pub const CONCEAL_VMX_FROM_PT: u32 = 1 << 19;
    pub const ENABLE_XSAVES_XRSTORS: u32 = 1 << 20;
    pub const MODE_BASED_EPT: u32 = 1 << 22;
    pub const USE_TSC_SCALING: u32 = 1 << 25;
}

// =============================================================================
// VMX OPERATIONS
// =============================================================================

/// Enable VMX operation on current CPU
pub fn enable_vmx() -> Result<(), KvmError> {
    // Check if already enabled
    if VMX_ENABLED.load(Ordering::SeqCst) {
        return Ok(());
    }

    // Check feature control MSR
    let feature_control = unsafe { crate::cpu::rdmsr(msr::IA32_FEATURE_CONTROL as u32) };

    if (feature_control & feature_control::LOCK) != 0 {
        // Already locked
        if (feature_control & feature_control::VMXON_OUTSIDE_SMX) == 0 {
            return Err(KvmError::NotSupported);
        }
    } else {
        // Set VMXON enable and lock
        unsafe {
            let value = feature_control | feature_control::VMXON_OUTSIDE_SMX | feature_control::LOCK;
            core::arch::asm!(
                "wrmsr",
                in("ecx") msr::IA32_FEATURE_CONTROL as u32,
                in("eax") (value & 0xFFFF_FFFF) as u32,
                in("edx") (value >> 32) as u32,
                options(nostack, nomem)
            );
        }
    }

    // Set CR4.VMXE
    unsafe {
        let cr4: u64;
        core::arch::asm!("mov {}, cr4", out(reg) cr4);
        core::arch::asm!("mov cr4, {}", in(reg) cr4 | (1 << 13));
    }

    // Allocate VMXON region
    let vmxon_region = allocate_vmx_region()?;
    VMXON_REGION.store(vmxon_region, Ordering::SeqCst);

    // Initialize VMXON region with revision ID
    let vmx_basic = unsafe { crate::cpu::rdmsr(msr::IA32_VMX_BASIC as u32) };
    let revision_id = (vmx_basic & 0x7FFFFFFF) as u32;

    unsafe {
        let ptr = vmxon_region as *mut u32;
        core::ptr::write_volatile(ptr, revision_id);
    }

    // Execute VMXON
    let result: u8;
    unsafe {
        core::arch::asm!(
            "vmxon [{0}]",
            "setc {1}",
            in(reg) &vmxon_region,
            out(reg_byte) result,
            options(nostack),
        );
    }

    if result != 0 {
        return Err(KvmError::HardwareError);
    }

    VMX_ENABLED.store(true, Ordering::SeqCst);
    Ok(())
}

/// Disable VMX operation on current CPU
pub fn disable_vmx() -> Result<(), KvmError> {
    if !VMX_ENABLED.load(Ordering::SeqCst) {
        return Ok(());
    }

    // Execute VMXOFF
    unsafe {
        core::arch::asm!("vmxoff", options(nostack));
    }

    // Clear CR4.VMXE
    unsafe {
        let cr4: u64;
        core::arch::asm!("mov {}, cr4", out(reg) cr4);
        core::arch::asm!("mov cr4, {}", in(reg) cr4 & !(1u64 << 13));
    }

    // Free VMXON region
    let vmxon_region = VMXON_REGION.swap(0, Ordering::SeqCst);
    if vmxon_region != 0 {
        free_vmx_region(vmxon_region);
    }

    VMX_ENABLED.store(false, Ordering::SeqCst);
    Ok(())
}

/// Allocate a 4KB aligned VMX region
fn allocate_vmx_region() -> Result<u64, KvmError> {
    // Would allocate 4KB aligned page
    // For now, return placeholder
    Ok(0x1000_0000)
}

/// Free a VMX region
fn free_vmx_region(_addr: u64) {
    // Would free the page
}

// =============================================================================
// VMCS OPERATIONS
// =============================================================================

/// VMCS (Virtual Machine Control Structure)
pub struct Vmcs {
    /// Physical address of VMCS region
    phys_addr: u64,
    /// Launched
    launched: bool,
}

impl Vmcs {
    /// Allocate new VMCS
    pub fn new() -> Result<Self, KvmError> {
        let phys_addr = allocate_vmx_region()?;

        // Write revision ID
        let vmx_basic = unsafe { crate::cpu::rdmsr(msr::IA32_VMX_BASIC as u32) };
        let revision_id = (vmx_basic & 0x7FFFFFFF) as u32;

        unsafe {
            let ptr = phys_addr as *mut u32;
            core::ptr::write_volatile(ptr, revision_id);
        }

        Ok(Self {
            phys_addr,
            launched: false,
        })
    }

    /// Clear VMCS
    pub fn clear(&mut self) -> Result<(), KvmError> {
        let result: u8;
        unsafe {
            core::arch::asm!(
                "vmclear [{0}]",
                "setc {1}",
                in(reg) &self.phys_addr,
                out(reg_byte) result,
                options(nostack),
            );
        }

        if result != 0 {
            return Err(KvmError::HardwareError);
        }

        self.launched = false;
        Ok(())
    }

    /// Load VMCS as current
    pub fn load(&self) -> Result<(), KvmError> {
        let result: u8;
        unsafe {
            core::arch::asm!(
                "vmptrld [{0}]",
                "setc {1}",
                in(reg) &self.phys_addr,
                out(reg_byte) result,
                options(nostack),
            );
        }

        if result != 0 {
            return Err(KvmError::HardwareError);
        }

        Ok(())
    }

    /// Read VMCS field
    pub fn read(&self, field: u32) -> Result<u64, KvmError> {
        let value: u64;
        let result: u8;

        unsafe {
            core::arch::asm!(
                "vmread {0}, {1}",
                "setc {2}",
                out(reg) value,
                in(reg) field as u64,
                out(reg_byte) result,
                options(nostack),
            );
        }

        if result != 0 {
            return Err(KvmError::HardwareError);
        }

        Ok(value)
    }

    /// Write VMCS field
    pub fn write(&self, field: u32, value: u64) -> Result<(), KvmError> {
        let result: u8;

        unsafe {
            core::arch::asm!(
                "vmwrite {0}, {1}",
                "setc {2}",
                in(reg) field as u64,
                in(reg) value,
                out(reg_byte) result,
                options(nostack),
            );
        }

        if result != 0 {
            return Err(KvmError::HardwareError);
        }

        Ok(())
    }

    /// Launch VM
    pub fn launch(&mut self) -> Result<u32, KvmError> {
        if self.launched {
            return self.resume();
        }

        let result: u8;
        unsafe {
            core::arch::asm!(
                "vmlaunch",
                "setc {0}",
                out(reg_byte) result,
                options(nostack),
            );
        }

        if result != 0 {
            let _error = self.read(vmcs::VM_INSTRUCTION_ERROR)?;
            return Err(KvmError::HardwareError);
        }

        self.launched = true;

        // Read exit reason
        let reason = self.read(vmcs::VM_EXIT_REASON)? as u32;
        Ok(reason & 0xFFFF)
    }

    /// Resume VM
    pub fn resume(&self) -> Result<u32, KvmError> {
        let result: u8;
        unsafe {
            core::arch::asm!(
                "vmresume",
                "setc {0}",
                out(reg_byte) result,
                options(nostack),
            );
        }

        if result != 0 {
            return Err(KvmError::HardwareError);
        }

        // Read exit reason
        let reason = self.read(vmcs::VM_EXIT_REASON)? as u32;
        Ok(reason & 0xFFFF)
    }
}

impl Drop for Vmcs {
    fn drop(&mut self) {
        let _ = self.clear();
        free_vmx_region(self.phys_addr);
    }
}

// =============================================================================
// EPT (Extended Page Tables)
// =============================================================================

/// EPT entry flags
pub mod ept_flags {
    pub const READ: u64 = 1 << 0;
    pub const WRITE: u64 = 1 << 1;
    pub const EXECUTE: u64 = 1 << 2;
    pub const MEMORY_TYPE_SHIFT: u64 = 3;
    pub const MEMORY_TYPE_MASK: u64 = 0x7 << 3;
    pub const IGNORE_PAT: u64 = 1 << 6;
    pub const LARGE_PAGE: u64 = 1 << 7;
    pub const ACCESSED: u64 = 1 << 8;
    pub const DIRTY: u64 = 1 << 9;
    pub const USER_EXECUTE: u64 = 1 << 10;
    pub const SUPPRESS_VE: u64 = 1 << 63;
}

/// Memory types for EPT
pub mod memory_type {
    pub const UNCACHEABLE: u64 = 0;
    pub const WRITE_COMBINING: u64 = 1;
    pub const WRITE_THROUGH: u64 = 4;
    pub const WRITE_PROTECT: u64 = 5;
    pub const WRITE_BACK: u64 = 6;
}

/// EPT pointer configuration
pub struct EptPointer {
    /// Memory type
    pub memory_type: u64,
    /// Page walk length - 1
    pub page_walk_length: u64,
    /// Accessed/dirty flags enabled
    pub ad_enabled: bool,
    /// PML4 physical address
    pub pml4_addr: u64,
}

impl EptPointer {
    /// Create new EPT pointer
    pub fn new(pml4_addr: u64) -> Self {
        Self {
            memory_type: memory_type::WRITE_BACK,
            page_walk_length: 3, // 4-level page table
            ad_enabled: true,
            pml4_addr,
        }
    }

    /// Get raw EPTP value
    pub fn raw(&self) -> u64 {
        let mut value = self.pml4_addr & !0xFFF; // Aligned address
        value |= self.memory_type & 0x7;
        value |= (self.page_walk_length & 0x7) << 3;
        if self.ad_enabled {
            value |= 1 << 6;
        }
        value
    }
}

/// Invalidate EPT
pub fn invept(eptp: u64, single_context: bool) {
    let descriptor: [u64; 2] = [eptp, 0];
    let inv_type: u64 = if single_context { 1 } else { 2 };

    unsafe {
        core::arch::asm!(
            "invept {0}, [{1}]",
            in(reg) inv_type,
            in(reg) descriptor.as_ptr(),
            options(nostack),
        );
    }
}

/// Invalidate VPID
pub fn invvpid(vpid: u16, gva: u64, all: bool) {
    let descriptor: [u64; 2] = [vpid as u64, gva];
    let inv_type: u64 = if all { 2 } else { 0 };

    unsafe {
        core::arch::asm!(
            "invvpid {0}, [{1}]",
            in(reg) inv_type,
            in(reg) descriptor.as_ptr(),
            options(nostack),
        );
    }
}
