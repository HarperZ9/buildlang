// ===============================================================================
// BUILDOS KERNEL - CPU MANAGEMENT
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! CPU initialization and management.
//!
//! This module provides:
//! - Bootstrap Processor (BSP) initialization
//! - Application Processor (AP) startup
//! - SMP support and IPI handling
//! - CPU feature detection
//! - Per-CPU data management
//! - CPU hotplug support

#![allow(static_mut_refs)]

pub mod smp;
pub mod hotplug;
pub mod io;

pub use io::{inb, outb, inw, outw, inl, outl, io_wait};

pub use smp::{
    current_cpu, this_cpu, per_cpu, nr_cpus, nr_online,
    kick_cpu, tlb_shootdown_all, stop_all_cpus,
    CpuState, PerCpu, CpuTopology, IpiType,
};


/// CPU features detected via CPUID
#[derive(Clone, Debug)]
pub struct CpuFeatures {
    /// Vendor string (e.g., "GenuineIntel", "AuthenticAMD")
    pub vendor: [u8; 12],
    /// CPU family
    pub family: u8,
    /// CPU model
    pub model: u8,
    /// CPU stepping
    pub stepping: u8,
    /// Brand string
    pub brand: [u8; 48],
    /// Has SSE
    pub sse: bool,
    /// Has SSE2
    pub sse2: bool,
    /// Has SSE3
    pub sse3: bool,
    /// Has SSSE3
    pub ssse3: bool,
    /// Has SSE4.1
    pub sse4_1: bool,
    /// Has SSE4.2
    pub sse4_2: bool,
    /// Has AVX
    pub avx: bool,
    /// Has AVX2
    pub avx2: bool,
    /// Has AVX-512
    pub avx512: bool,
    /// Has AES-NI
    pub aes: bool,
    /// Has RDRAND
    pub rdrand: bool,
    /// Has RDSEED
    pub rdseed: bool,
    /// Has TSC
    pub tsc: bool,
    /// Has invariant TSC
    pub tsc_invariant: bool,
    /// Has APIC
    pub apic: bool,
    /// Has x2APIC
    pub x2apic: bool,
    /// Has FSGSBASE
    pub fsgsbase: bool,
    /// Has PCID
    pub pcid: bool,
    /// Has SMEP
    pub smep: bool,
    /// Has SMAP
    pub smap: bool,
    /// Has PKU (Memory Protection Keys)
    pub pku: bool,
    /// Has 1GB pages
    pub pages_1gb: bool,
    /// Has NX (No-Execute)
    pub nx: bool,
    /// Has syscall/sysret
    pub syscall: bool,
    /// Number of physical address bits
    pub phys_bits: u8,
    /// Number of virtual address bits
    pub virt_bits: u8,
    /// Maximum supported CPUs
    pub max_cpus: u32,
}

impl Default for CpuFeatures {
    fn default() -> Self {
        Self {
            vendor: [0; 12],
            family: 0,
            model: 0,
            stepping: 0,
            brand: [0; 48],
            sse: false,
            sse2: false,
            sse3: false,
            ssse3: false,
            sse4_1: false,
            sse4_2: false,
            avx: false,
            avx2: false,
            avx512: false,
            aes: false,
            rdrand: false,
            rdseed: false,
            tsc: false,
            tsc_invariant: false,
            apic: false,
            x2apic: false,
            fsgsbase: false,
            pcid: false,
            smep: false,
            smap: false,
            pku: false,
            pages_1gb: false,
            nx: false,
            syscall: false,
            phys_bits: 48,
            virt_bits: 48,
            max_cpus: 1,
        }
    }
}

/// Global CPU features (detected on BSP)
static mut CPU_FEATURES: CpuFeatures = CpuFeatures {
    vendor: [0; 12],
    family: 0,
    model: 0,
    stepping: 0,
    brand: [0; 48],
    sse: false,
    sse2: false,
    sse3: false,
    ssse3: false,
    sse4_1: false,
    sse4_2: false,
    avx: false,
    avx2: false,
    avx512: false,
    aes: false,
    rdrand: false,
    rdseed: false,
    tsc: false,
    tsc_invariant: false,
    apic: false,
    x2apic: false,
    fsgsbase: false,
    pcid: false,
    smep: false,
    smap: false,
    pku: false,
    pages_1gb: false,
    nx: false,
    syscall: false,
    phys_bits: 48,
    virt_bits: 48,
    max_cpus: 1,
};

/// Detect CPU features using CPUID
pub fn detect_features() -> CpuFeatures {
    let mut features = CpuFeatures::default();

    unsafe {
        // CPUID function 0: Get vendor ID
        let (max_func, ebx, ecx, edx) = cpuid(0);

        // Vendor: EBX-EDX-ECX (in that order)
        features.vendor[0..4].copy_from_slice(&ebx.to_le_bytes());
        features.vendor[4..8].copy_from_slice(&edx.to_le_bytes());
        features.vendor[8..12].copy_from_slice(&ecx.to_le_bytes());

        if max_func >= 1 {
            // CPUID function 1: Processor Info and Feature Bits
            let (eax, _ebx, ecx, edx) = cpuid(1);

            features.stepping = (eax & 0xF) as u8;
            features.model = ((eax >> 4) & 0xF) as u8;
            features.family = ((eax >> 8) & 0xF) as u8;

            // Extended model and family
            if features.family == 0xF {
                features.family += ((eax >> 20) & 0xFF) as u8;
            }
            if features.family == 0x6 || features.family == 0xF {
                features.model += (((eax >> 16) & 0xF) << 4) as u8;
            }

            // EDX features
            features.tsc = (edx & (1 << 4)) != 0;
            features.apic = (edx & (1 << 9)) != 0;
            features.sse = (edx & (1 << 25)) != 0;
            features.sse2 = (edx & (1 << 26)) != 0;

            // ECX features
            features.sse3 = (ecx & (1 << 0)) != 0;
            features.ssse3 = (ecx & (1 << 9)) != 0;
            features.sse4_1 = (ecx & (1 << 19)) != 0;
            features.sse4_2 = (ecx & (1 << 20)) != 0;
            features.x2apic = (ecx & (1 << 21)) != 0;
            features.aes = (ecx & (1 << 25)) != 0;
            features.avx = (ecx & (1 << 28)) != 0;
            features.rdrand = (ecx & (1 << 30)) != 0;
        }

        if max_func >= 7 {
            // CPUID function 7: Extended Features
            let (_eax, ebx, ecx, _edx) = cpuid_with_ecx(7, 0);

            features.fsgsbase = (ebx & (1 << 0)) != 0;
            features.smep = (ebx & (1 << 7)) != 0;
            features.avx2 = (ebx & (1 << 5)) != 0;
            features.avx512 = (ebx & (1 << 16)) != 0;
            features.smap = (ebx & (1 << 20)) != 0;
            features.rdseed = (ebx & (1 << 18)) != 0;
            features.pku = (ecx & (1 << 3)) != 0;
        }

        // Extended CPUID
        let (max_ext, _, _, _) = cpuid(0x8000_0000);

        if max_ext >= 0x8000_0001 {
            // Extended Processor Info
            let (_eax, _ebx, _ecx, edx) = cpuid(0x8000_0001);

            features.nx = (edx & (1 << 20)) != 0;
            features.syscall = (edx & (1 << 11)) != 0;
            features.pages_1gb = (edx & (1 << 26)) != 0;
        }

        if max_ext >= 0x8000_0004 {
            // Brand string (3 functions)
            for i in 0..3 {
                let (eax, ebx, ecx, edx) = cpuid(0x8000_0002 + i);
                let offset = (i * 16) as usize;
                features.brand[offset..offset + 4].copy_from_slice(&eax.to_le_bytes());
                features.brand[offset + 4..offset + 8].copy_from_slice(&ebx.to_le_bytes());
                features.brand[offset + 8..offset + 12].copy_from_slice(&ecx.to_le_bytes());
                features.brand[offset + 12..offset + 16].copy_from_slice(&edx.to_le_bytes());
            }
        }

        if max_ext >= 0x8000_0007 {
            // Advanced Power Management
            let (_eax, _ebx, _ecx, edx) = cpuid(0x8000_0007);
            features.tsc_invariant = (edx & (1 << 8)) != 0;
        }

        if max_ext >= 0x8000_0008 {
            // Virtual and Physical address sizes
            let (eax, _ebx, _ecx, _edx) = cpuid(0x8000_0008);
            features.phys_bits = (eax & 0xFF) as u8;
            features.virt_bits = ((eax >> 8) & 0xFF) as u8;
        }
    }

    features
}

/// Execute CPUID instruction
#[inline]
pub unsafe fn cpuid(leaf: u32) -> (u32, u32, u32, u32) {
    let eax: u32;
    let ebx: u32;
    let ecx: u32;
    let edx: u32;

    core::arch::asm!(
        "push rbx",
        "cpuid",
        "mov {ebx_out:e}, ebx",
        "pop rbx",
        inlateout("eax") leaf => eax,
        ebx_out = lateout(reg) ebx,
        lateout("ecx") ecx,
        lateout("edx") edx,
    );

    (eax, ebx, ecx, edx)
}

/// Execute CPUID with sub-leaf
#[inline]
pub unsafe fn cpuid_with_ecx(leaf: u32, subleaf: u32) -> (u32, u32, u32, u32) {
    let eax: u32;
    let ebx: u32;
    let ecx_out: u32;
    let edx: u32;

    core::arch::asm!(
        "push rbx",
        "cpuid",
        "mov {ebx_out:e}, ebx",
        "pop rbx",
        inlateout("eax") leaf => eax,
        ebx_out = lateout(reg) ebx,
        inlateout("ecx") subleaf => ecx_out,
        lateout("edx") edx,
    );

    (eax, ebx, ecx_out, edx)
}

/// Initialize the bootstrap processor (BSP)
///
/// # Safety
///
/// Must only be called once during early boot.
pub unsafe fn init_bsp() {
    // Detect CPU features
    let features = detect_features();
    CPU_FEATURES = features;

    // Enable SSE/SSE2
    enable_sse();

    // Enable syscall/sysret
    enable_syscall();

    // Set up GDT and TSS
    init_gdt();

    // Initialize SMP subsystem with BSP APIC ID
    let apic_id = read_apic_id();
    smp::init(apic_id);
}

/// Initialize an Application Processor (AP)
///
/// # Safety
///
/// Called by AP trampoline code.
pub unsafe fn init_ap(cpu_id: u32) {
    // Enable SSE/SSE2
    enable_sse();

    // Enable syscall/sysret
    enable_syscall();

    // Set up GDT and TSS for this CPU
    init_gdt_ap(cpu_id);

    // Notify SMP manager
    smp::manager().ap_entry(cpu_id);
}

/// Enable SSE instructions
unsafe fn enable_sse() {
    // Set CR0.EM = 0, CR0.MP = 1
    let mut cr0: u64;
    core::arch::asm!("mov {}, cr0", out(reg) cr0);
    cr0 &= !(1 << 2); // Clear EM
    cr0 |= 1 << 1;     // Set MP
    core::arch::asm!("mov cr0, {}", in(reg) cr0);

    // Set CR4.OSFXSR = 1, CR4.OSXMMEXCPT = 1
    let mut cr4: u64;
    core::arch::asm!("mov {}, cr4", out(reg) cr4);
    cr4 |= (1 << 9) | (1 << 10);
    core::arch::asm!("mov cr4, {}", in(reg) cr4);
}

/// Enable syscall/sysret instructions
unsafe fn enable_syscall() {
    // Set EFER.SCE = 1
    const EFER_MSR: u32 = 0xC0000080;
    const EFER_SCE: u64 = 1 << 0;

    let efer_lo: u32;
    let efer_hi: u32;
    core::arch::asm!(
        "rdmsr",
        in("ecx") EFER_MSR,
        out("eax") efer_lo,
        out("edx") efer_hi,
    );
    let efer = (efer_hi as u64) << 32 | (efer_lo as u64);

    core::arch::asm!(
        "wrmsr",
        in("ecx") EFER_MSR,
        in("eax") (efer | EFER_SCE) as u32,
        in("edx") ((efer | EFER_SCE) >> 32) as u32,
    );
}

/// Initialize Global Descriptor Table and TSS
unsafe fn init_gdt() {
    crate::gdt::init();
}

/// Initialize GDT for AP
unsafe fn init_gdt_ap(_cpu_id: u32) {
    // Would set up per-CPU GDT/TSS
    crate::gdt::init();
}

/// Read APIC ID from local APIC
fn read_apic_id() -> u32 {
    unsafe {
        // APIC ID register at offset 0x20
        let apic_base: *const u32 = 0xFEE0_0020 as *const u32;
        let id = core::ptr::read_volatile(apic_base);
        (id >> 24) & 0xFF
    }
}

/// Get detected CPU features
pub fn features() -> &'static CpuFeatures {
    unsafe { &CPU_FEATURES }
}

/// Get current CPU ID
pub fn cpu_id() -> usize {
    current_cpu() as usize
}

/// Halt the CPU
#[inline]
pub fn halt() {
    unsafe {
        core::arch::asm!("hlt", options(nostack, nomem));
    }
}

/// Disable interrupts
#[inline]
pub unsafe fn cli() {
    core::arch::asm!("cli", options(nostack, nomem));
}

/// Enable interrupts
#[inline]
pub unsafe fn sti() {
    core::arch::asm!("sti", options(nostack, nomem));
}

/// Read timestamp counter
#[inline]
pub fn rdtsc() -> u64 {
    let low: u32;
    let high: u32;

    unsafe {
        core::arch::asm!(
            "rdtsc",
            out("eax") low,
            out("edx") high,
            options(nostack, nomem)
        );
    }

    ((high as u64) << 32) | (low as u64)
}

/// Read timestamp counter (serializing)
#[inline]
pub fn rdtscp() -> (u64, u32) {
    let low: u32;
    let high: u32;
    let aux: u32;

    unsafe {
        core::arch::asm!(
            "rdtscp",
            out("eax") low,
            out("edx") high,
            out("ecx") aux,
            options(nostack, nomem)
        );
    }

    (((high as u64) << 32) | (low as u64), aux)
}

/// Memory fence (all memory operations complete)
#[inline]
pub fn mfence() {
    unsafe {
        core::arch::asm!("mfence", options(nostack, nomem));
    }
}

/// Store fence
#[inline]
pub fn sfence() {
    unsafe {
        core::arch::asm!("sfence", options(nostack, nomem));
    }
}

/// Load fence
#[inline]
pub fn lfence() {
    unsafe {
        core::arch::asm!("lfence", options(nostack, nomem));
    }
}

/// Pause (for spin loops)
#[inline]
pub fn pause() {
    unsafe {
        core::arch::asm!("pause", options(nostack, nomem));
    }
}

/// Read MSR (Model Specific Register)
#[inline]
pub unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;

    core::arch::asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") low,
        out("edx") high,
        options(nostack, nomem)
    );

    ((high as u64) << 32) | (low as u64)
}

/// Write MSR (Model Specific Register)
#[inline]
pub unsafe fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;

    core::arch::asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") low,
        in("edx") high,
        options(nostack, nomem)
    );
}

/// Read CR0
#[inline]
pub unsafe fn read_cr0() -> u64 {
    let value: u64;
    core::arch::asm!("mov {}, cr0", out(reg) value, options(nostack, nomem));
    value
}

/// Write CR0
#[inline]
pub unsafe fn write_cr0(value: u64) {
    core::arch::asm!("mov cr0, {}", in(reg) value, options(nostack, nomem));
}

/// Read CR2 (page fault linear address)
#[inline]
pub unsafe fn read_cr2() -> u64 {
    let value: u64;
    core::arch::asm!("mov {}, cr2", out(reg) value, options(nostack, nomem));
    value
}

/// Read CR3 (page table base)
#[inline]
pub unsafe fn read_cr3() -> u64 {
    let value: u64;
    core::arch::asm!("mov {}, cr3", out(reg) value, options(nostack, nomem));
    value
}

/// Write CR3 (page table base)
#[inline]
pub unsafe fn write_cr3(value: u64) {
    core::arch::asm!("mov cr3, {}", in(reg) value, options(nostack, nomem));
}

/// Read CR4
#[inline]
pub unsafe fn read_cr4() -> u64 {
    let value: u64;
    core::arch::asm!("mov {}, cr4", out(reg) value, options(nostack, nomem));
    value
}

/// Write CR4
#[inline]
pub unsafe fn write_cr4(value: u64) {
    core::arch::asm!("mov cr4, {}", in(reg) value, options(nostack, nomem));
}

/// Invalidate TLB entry
#[inline]
pub unsafe fn invlpg(addr: u64) {
    core::arch::asm!("invlpg [{}]", in(reg) addr, options(nostack));
}

/// Flush entire TLB
#[inline]
pub unsafe fn flush_tlb() {
    let cr3 = read_cr3();
    write_cr3(cr3);
}

/// Flush TLB on all CPUs
pub fn flush_tlb_all() {
    unsafe { flush_tlb(); }
    smp::tlb_shootdown_all();
}

/// Read GS base
#[inline]
pub unsafe fn read_gs_base() -> u64 {
    rdmsr(0xC0000101)
}

/// Write GS base
#[inline]
pub unsafe fn write_gs_base(value: u64) {
    wrmsr(0xC0000101, value);
}

/// Read kernel GS base
#[inline]
pub unsafe fn read_kernel_gs_base() -> u64 {
    rdmsr(0xC0000102)
}

/// Write kernel GS base
#[inline]
pub unsafe fn write_kernel_gs_base(value: u64) {
    wrmsr(0xC0000102, value);
}

/// Swap GS base with kernel GS base
#[inline]
pub unsafe fn swapgs() {
    core::arch::asm!("swapgs", options(nostack, nomem));
}

/// Generate random number using RDRAND
#[inline]
pub fn rdrand64() -> Option<u64> {
    let value: u64;
    let success: u8;

    unsafe {
        core::arch::asm!(
            "rdrand {0}",
            "setc {1}",
            out(reg) value,
            out(reg_byte) success,
            options(nostack, nomem)
        );
    }

    if success != 0 { Some(value) } else { None }
}

/// Generate seed using RDSEED
#[inline]
pub fn rdseed64() -> Option<u64> {
    let value: u64;
    let success: u8;

    unsafe {
        core::arch::asm!(
            "rdseed {0}",
            "setc {1}",
            out(reg) value,
            out(reg_byte) success,
            options(nostack, nomem)
        );
    }

    if success != 0 { Some(value) } else { None }
}

/// Get current CPU ID (alias for cpu_id)
#[inline]
pub fn current_cpu_id() -> u32 {
    current_cpu()
}

// Note: rdmsr and wrmsr are defined above
