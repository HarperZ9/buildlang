// ===============================================================================
// BUILDOS KERNEL - SVM (AMD-V) SUPPORT
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! AMD-V (SVM) Support
//!
//! Implements AMD Secure Virtual Machine extensions:
//! - VMRUN/VMEXIT operations
//! - VMCB management
//! - NPT (Nested Page Tables)
//! - AVIC (Advanced Virtual Interrupt Controller)

#![allow(dead_code)]

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use super::KvmError;

// =============================================================================
// SVM MSRs
// =============================================================================

mod msr {
    pub const AMD64_EFER: u32 = 0xC0000080;
    pub const AMD64_VM_CR: u32 = 0xC0010114;
    pub const AMD64_VM_HSAVE_PA: u32 = 0xC0010117;
}

/// EFER MSR bits
mod efer {
    pub const SVME: u64 = 1 << 12;
}

/// VM_CR MSR bits
mod vm_cr {
    pub const SVM_DIS: u64 = 1 << 4;
}

// =============================================================================
// SVM STATE
// =============================================================================

/// SVM enabled on current CPU
static SVM_ENABLED: AtomicBool = AtomicBool::new(false);

/// Host save area physical address
static HOST_SAVE_AREA: AtomicU64 = AtomicU64::new(0);

// =============================================================================
// VMCB STRUCTURE
// =============================================================================

/// VMCB control area offset
pub const VMCB_CONTROL_OFFSET: usize = 0;
/// VMCB state save area offset
pub const VMCB_STATE_OFFSET: usize = 0x400;
/// Total VMCB size
pub const VMCB_SIZE: usize = 0x1000;

/// VMCB Control Area
#[repr(C)]
#[derive(Debug, Clone)]
pub struct VmcbControl {
    /// Intercept reads of CR0-15
    pub intercept_cr_reads: u16,
    /// Intercept writes of CR0-15
    pub intercept_cr_writes: u16,
    /// Intercept reads of DR0-15
    pub intercept_dr_reads: u16,
    /// Intercept writes of DR0-15
    pub intercept_dr_writes: u16,
    /// Exception intercepts
    pub intercept_exceptions: u32,
    /// Miscellaneous intercepts (low)
    pub intercept_misc1: u32,
    /// Miscellaneous intercepts (high)
    pub intercept_misc2: u32,
    /// Padding
    pub reserved1: [u8; 40],
    /// Pause filter threshold
    pub pause_filter_thresh: u16,
    /// Pause filter count
    pub pause_filter_count: u16,
    /// Physical address of I/O permission map
    pub iopm_base_pa: u64,
    /// Physical address of MSR permission map
    pub msrpm_base_pa: u64,
    /// TSC offset
    pub tsc_offset: u64,
    /// Guest ASID
    pub guest_asid: u32,
    /// TLB control
    pub tlb_ctl: u8,
    /// Padding
    pub reserved2: [u8; 3],
    /// Virtual interrupt control
    pub v_intr: u64,
    /// Interrupt shadow
    pub interrupt_shadow: u64,
    /// Exit code
    pub exit_code: u64,
    /// Exit info 1
    pub exit_info_1: u64,
    /// Exit info 2
    pub exit_info_2: u64,
    /// Exit interrupt info
    pub exit_int_info: u64,
    /// Nested paging enable
    pub np_enable: u64,
    /// AVIC APIC bar
    pub avic_apic_bar: u64,
    /// Guest PA of GHCB
    pub ghcb_gpa: u64,
    /// Event injection
    pub event_inj: u64,
    /// Nested CR3
    pub n_cr3: u64,
    /// LBR virtualization enable
    pub lbr_ctl: u64,
    /// VMCB clean bits
    pub vmcb_clean: u32,
    /// Reserved
    pub reserved3: u32,
    /// Next RIP (on #VMEXIT)
    pub next_rip: u64,
    /// Number of bytes fetched
    pub insn_len: u8,
    /// Guest instruction bytes
    pub insn_bytes: [u8; 15],
    /// AVIC backing page pointer
    pub avic_backing_page: u64,
    /// Reserved
    pub reserved4: u64,
    /// AVIC logical table pointer
    pub avic_logical_id: u64,
    /// AVIC physical table pointer
    pub avic_physical_id: u64,
    /// Reserved
    pub reserved5: u64,
    /// VMSA pointer for SEV-ES
    pub vmsa_ptr: u64,
    /// Padding to 0x400
    pub reserved6: [u8; 720],
}

impl Default for VmcbControl {
    fn default() -> Self {
        // Safety: VmcbControl is a #[repr(C)] structure with all fields
        // that can be safely zero-initialized
        unsafe { core::mem::zeroed() }
    }
}

/// VMCB State Save Area
#[repr(C)]
#[derive(Debug, Clone)]
pub struct VmcbStateSave {
    /// ES segment
    pub es: VmcbSegment,
    /// CS segment
    pub cs: VmcbSegment,
    /// SS segment
    pub ss: VmcbSegment,
    /// DS segment
    pub ds: VmcbSegment,
    /// FS segment
    pub fs: VmcbSegment,
    /// GS segment
    pub gs: VmcbSegment,
    /// GDTR
    pub gdtr: VmcbSegment,
    /// LDTR
    pub ldtr: VmcbSegment,
    /// IDTR
    pub idtr: VmcbSegment,
    /// TR
    pub tr: VmcbSegment,
    /// Reserved
    pub reserved1: [u8; 43],
    /// CPL
    pub cpl: u8,
    /// Reserved
    pub reserved2: [u8; 4],
    /// EFER
    pub efer: u64,
    /// Reserved
    pub reserved3: [u8; 104],
    /// XSS
    pub xss: u64,
    /// CR4
    pub cr4: u64,
    /// CR3
    pub cr3: u64,
    /// CR0
    pub cr0: u64,
    /// DR7
    pub dr7: u64,
    /// DR6
    pub dr6: u64,
    /// RFLAGS
    pub rflags: u64,
    /// RIP
    pub rip: u64,
    /// Reserved
    pub reserved4: [u8; 88],
    /// RSP
    pub rsp: u64,
    /// S_CET
    pub s_cet: u64,
    /// SSP
    pub ssp: u64,
    /// ISST address
    pub isst_addr: u64,
    /// RAX
    pub rax: u64,
    /// STAR
    pub star: u64,
    /// LSTAR
    pub lstar: u64,
    /// CSTAR
    pub cstar: u64,
    /// SFMASK
    pub sfmask: u64,
    /// KernelGsBase
    pub kernel_gs_base: u64,
    /// SYSENTER_CS
    pub sysenter_cs: u64,
    /// SYSENTER_ESP
    pub sysenter_esp: u64,
    /// SYSENTER_EIP
    pub sysenter_eip: u64,
    /// CR2
    pub cr2: u64,
    /// Reserved
    pub reserved5: [u8; 32],
    /// G_PAT
    pub g_pat: u64,
    /// DBGCTL
    pub dbgctl: u64,
    /// BR_FROM
    pub br_from: u64,
    /// BR_TO
    pub br_to: u64,
    /// LASTEXCP_FROM
    pub last_excp_from: u64,
    /// LASTEXCP_TO
    pub last_excp_to: u64,
    /// SPEC_CTRL
    pub spec_ctrl: u64,
    /// Padding
    pub reserved6: [u8; 2408],
}

impl Default for VmcbStateSave {
    fn default() -> Self {
        // Safety: VmcbStateSave is a #[repr(C)] structure with all fields
        // that can be safely zero-initialized
        unsafe { core::mem::zeroed() }
    }
}

/// VMCB segment descriptor
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct VmcbSegment {
    /// Selector
    pub selector: u16,
    /// Attributes
    pub attrib: u16,
    /// Limit
    pub limit: u32,
    /// Base address
    pub base: u64,
}

// =============================================================================
// SVM EXIT CODES
// =============================================================================

pub mod exit_code {
    /// Read CR0
    pub const CR0_READ: u64 = 0x000;
    /// Write CR0
    pub const CR0_WRITE: u64 = 0x010;
    /// Read CR3
    pub const CR3_READ: u64 = 0x003;
    /// Write CR3
    pub const CR3_WRITE: u64 = 0x013;
    /// Read CR4
    pub const CR4_READ: u64 = 0x004;
    /// Write CR4
    pub const CR4_WRITE: u64 = 0x014;
    /// Read DR0-7
    pub const DR0_READ: u64 = 0x020;
    /// Write DR0-7
    pub const DR0_WRITE: u64 = 0x030;
    /// Exception (vector in EXITINFO1)
    pub const EXCP_BASE: u64 = 0x040;
    /// Physical interrupt
    pub const INTR: u64 = 0x060;
    /// NMI
    pub const NMI: u64 = 0x061;
    /// SMI
    pub const SMI: u64 = 0x062;
    /// Virtual INTR
    pub const INIT: u64 = 0x063;
    /// Virtual NMI
    pub const VINTR: u64 = 0x064;
    /// CR0 selective write
    pub const CR0_SEL_WRITE: u64 = 0x065;
    /// IDTR read
    pub const IDTR_READ: u64 = 0x066;
    /// GDTR read
    pub const GDTR_READ: u64 = 0x067;
    /// LDTR read
    pub const LDTR_READ: u64 = 0x068;
    /// TR read
    pub const TR_READ: u64 = 0x069;
    /// IDTR write
    pub const IDTR_WRITE: u64 = 0x06A;
    /// GDTR write
    pub const GDTR_WRITE: u64 = 0x06B;
    /// LDTR write
    pub const LDTR_WRITE: u64 = 0x06C;
    /// TR write
    pub const TR_WRITE: u64 = 0x06D;
    /// RDTSC
    pub const RDTSC: u64 = 0x06E;
    /// RDPMC
    pub const RDPMC: u64 = 0x06F;
    /// PUSHF
    pub const PUSHF: u64 = 0x070;
    /// POPF
    pub const POPF: u64 = 0x071;
    /// CPUID
    pub const CPUID: u64 = 0x072;
    /// RSM
    pub const RSM: u64 = 0x073;
    /// IRET
    pub const IRET: u64 = 0x074;
    /// Software interrupt
    pub const SWINT: u64 = 0x075;
    /// INVD
    pub const INVD: u64 = 0x076;
    /// PAUSE
    pub const PAUSE: u64 = 0x077;
    /// HLT
    pub const HLT: u64 = 0x078;
    /// INVLPG
    pub const INVLPG: u64 = 0x079;
    /// INVLPGA
    pub const INVLPGA: u64 = 0x07A;
    /// IO instruction
    pub const IOIO: u64 = 0x07B;
    /// MSR access
    pub const MSR: u64 = 0x07C;
    /// Task switch
    pub const TASK_SWITCH: u64 = 0x07D;
    /// FP error frozen
    pub const FERR_FREEZE: u64 = 0x07E;
    /// Shutdown
    pub const SHUTDOWN: u64 = 0x07F;
    /// VMRUN
    pub const VMRUN: u64 = 0x080;
    /// VMMCALL
    pub const VMMCALL: u64 = 0x081;
    /// VMLOAD
    pub const VMLOAD: u64 = 0x082;
    /// VMSAVE
    pub const VMSAVE: u64 = 0x083;
    /// STGI
    pub const STGI: u64 = 0x084;
    /// CLGI
    pub const CLGI: u64 = 0x085;
    /// SKINIT
    pub const SKINIT: u64 = 0x086;
    /// RDTSCP
    pub const RDTSCP: u64 = 0x087;
    /// ICEBP
    pub const ICEBP: u64 = 0x088;
    /// WBINVD
    pub const WBINVD: u64 = 0x089;
    /// MONITOR
    pub const MONITOR: u64 = 0x08A;
    /// MWAIT
    pub const MWAIT: u64 = 0x08B;
    /// MWAIT conditional
    pub const MWAIT_COND: u64 = 0x08C;
    /// XSETBV
    pub const XSETBV: u64 = 0x08D;
    /// EFER write
    pub const EFER_WRITE_TRAP: u64 = 0x08F;
    /// CR0-15 write trap
    pub const CR0_WRITE_TRAP: u64 = 0x090;
    /// NPF (Nested Page Fault)
    pub const NPF: u64 = 0x400;
    /// AVIC incomplete IPI
    pub const AVIC_INCOMPLETE_IPI: u64 = 0x401;
    /// AVIC no acceleration
    pub const AVIC_NOACCEL: u64 = 0x402;
    /// VMGEXIT
    pub const VMGEXIT: u64 = 0x403;
    /// Invalid guest state
    pub const INVALID: u64 = 0xFFFFFFFFFFFFFFFF;
    /// Busy bit
    pub const BUSY: u64 = 1 << 63;
}

// =============================================================================
// SVM INTERCEPT BITS
// =============================================================================

/// Intercept misc1 bits
pub mod intercept1 {
    pub const INTERCEPT_INTR: u32 = 1 << 0;
    pub const INTERCEPT_NMI: u32 = 1 << 1;
    pub const INTERCEPT_SMI: u32 = 1 << 2;
    pub const INTERCEPT_INIT: u32 = 1 << 3;
    pub const INTERCEPT_VINTR: u32 = 1 << 4;
    pub const INTERCEPT_CR0_SEL_WRITE: u32 = 1 << 5;
    pub const INTERCEPT_SIDT: u32 = 1 << 6;
    pub const INTERCEPT_SGDT: u32 = 1 << 7;
    pub const INTERCEPT_SLDT: u32 = 1 << 8;
    pub const INTERCEPT_STR: u32 = 1 << 9;
    pub const INTERCEPT_LIDT: u32 = 1 << 10;
    pub const INTERCEPT_LGDT: u32 = 1 << 11;
    pub const INTERCEPT_LLDT: u32 = 1 << 12;
    pub const INTERCEPT_LTR: u32 = 1 << 13;
    pub const INTERCEPT_RDTSC: u32 = 1 << 14;
    pub const INTERCEPT_RDPMC: u32 = 1 << 15;
    pub const INTERCEPT_PUSHF: u32 = 1 << 16;
    pub const INTERCEPT_POPF: u32 = 1 << 17;
    pub const INTERCEPT_CPUID: u32 = 1 << 18;
    pub const INTERCEPT_RSM: u32 = 1 << 19;
    pub const INTERCEPT_IRET: u32 = 1 << 20;
    pub const INTERCEPT_SWINT: u32 = 1 << 21;
    pub const INTERCEPT_INVD: u32 = 1 << 22;
    pub const INTERCEPT_PAUSE: u32 = 1 << 23;
    pub const INTERCEPT_HLT: u32 = 1 << 24;
    pub const INTERCEPT_INVLPG: u32 = 1 << 25;
    pub const INTERCEPT_INVLPGA: u32 = 1 << 26;
    pub const INTERCEPT_IOIO: u32 = 1 << 27;
    pub const INTERCEPT_MSR: u32 = 1 << 28;
    pub const INTERCEPT_TASK_SWITCH: u32 = 1 << 29;
    pub const INTERCEPT_FERR_FREEZE: u32 = 1 << 30;
    pub const INTERCEPT_SHUTDOWN: u32 = 1 << 31;
}

/// Intercept misc2 bits
pub mod intercept2 {
    pub const INTERCEPT_VMRUN: u32 = 1 << 0;
    pub const INTERCEPT_VMMCALL: u32 = 1 << 1;
    pub const INTERCEPT_VMLOAD: u32 = 1 << 2;
    pub const INTERCEPT_VMSAVE: u32 = 1 << 3;
    pub const INTERCEPT_STGI: u32 = 1 << 4;
    pub const INTERCEPT_CLGI: u32 = 1 << 5;
    pub const INTERCEPT_SKINIT: u32 = 1 << 6;
    pub const INTERCEPT_RDTSCP: u32 = 1 << 7;
    pub const INTERCEPT_ICEBP: u32 = 1 << 8;
    pub const INTERCEPT_WBINVD: u32 = 1 << 9;
    pub const INTERCEPT_MONITOR: u32 = 1 << 10;
    pub const INTERCEPT_MWAIT: u32 = 1 << 11;
    pub const INTERCEPT_MWAIT_COND: u32 = 1 << 12;
    pub const INTERCEPT_XSETBV: u32 = 1 << 13;
    pub const INTERCEPT_EFER_WRITE: u32 = 1 << 15;
}

// =============================================================================
// SVM OPERATIONS
// =============================================================================

/// Enable SVM on current CPU
pub fn enable_svm() -> Result<(), KvmError> {
    if SVM_ENABLED.load(Ordering::SeqCst) {
        return Ok(());
    }

    // Check VM_CR.SVMDIS
    let vm_cr = unsafe { crate::cpu::rdmsr(msr::AMD64_VM_CR) };
    if (vm_cr & vm_cr::SVM_DIS) != 0 {
        return Err(KvmError::NotSupported);
    }

    // Set EFER.SVME
    let efer = unsafe { crate::cpu::rdmsr(msr::AMD64_EFER) };
    let new_efer = efer | efer::SVME;
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") msr::AMD64_EFER as u32,
            in("eax") (new_efer & 0xFFFF_FFFF) as u32,
            in("edx") (new_efer >> 32) as u32,
            options(nostack, nomem)
        );
    }

    // Allocate and set host save area
    let host_save = allocate_page()?;
    HOST_SAVE_AREA.store(host_save, Ordering::SeqCst);

    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") msr::AMD64_VM_HSAVE_PA as u32,
            in("eax") (host_save & 0xFFFF_FFFF) as u32,
            in("edx") (host_save >> 32) as u32,
            options(nostack, nomem)
        );
    }

    SVM_ENABLED.store(true, Ordering::SeqCst);
    Ok(())
}

/// Disable SVM on current CPU
pub fn disable_svm() -> Result<(), KvmError> {
    if !SVM_ENABLED.load(Ordering::SeqCst) {
        return Ok(());
    }

    // Clear EFER.SVME
    let efer = unsafe { crate::cpu::rdmsr(msr::AMD64_EFER) };
    let new_efer = efer & !efer::SVME;
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") msr::AMD64_EFER as u32,
            in("eax") (new_efer & 0xFFFF_FFFF) as u32,
            in("edx") (new_efer >> 32) as u32,
            options(nostack, nomem)
        );
    }

    // Free host save area
    let host_save = HOST_SAVE_AREA.swap(0, Ordering::SeqCst);
    if host_save != 0 {
        free_page(host_save);
    }

    SVM_ENABLED.store(false, Ordering::SeqCst);
    Ok(())
}

/// Allocate a 4KB page
fn allocate_page() -> Result<u64, KvmError> {
    // Would allocate a page from physical memory
    Ok(0x2000_0000)
}

/// Free a page
fn free_page(_addr: u64) {
    // Would free the page
}

// =============================================================================
// VMCB MANAGEMENT
// =============================================================================

/// VMCB (Virtual Machine Control Block)
pub struct Vmcb {
    /// Physical address
    phys_addr: u64,
    /// Pointer to control area
    control: *mut VmcbControl,
    /// Pointer to state save area
    state: *mut VmcbStateSave,
}

impl Vmcb {
    /// Allocate new VMCB
    pub fn new() -> Result<Self, KvmError> {
        let phys_addr = allocate_page()?;

        // Zero the page
        unsafe {
            core::ptr::write_bytes(phys_addr as *mut u8, 0, VMCB_SIZE);
        }

        let control = phys_addr as *mut VmcbControl;
        let state = (phys_addr + VMCB_STATE_OFFSET as u64) as *mut VmcbStateSave;

        Ok(Self {
            phys_addr,
            control,
            state,
        })
    }

    /// Get control area
    pub fn control(&self) -> &VmcbControl {
        unsafe { &*self.control }
    }

    /// Get mutable control area
    pub fn control_mut(&mut self) -> &mut VmcbControl {
        unsafe { &mut *self.control }
    }

    /// Get state save area
    pub fn state(&self) -> &VmcbStateSave {
        unsafe { &*self.state }
    }

    /// Get mutable state save area
    pub fn state_mut(&mut self) -> &mut VmcbStateSave {
        unsafe { &mut *self.state }
    }

    /// Get physical address
    pub fn phys_addr(&self) -> u64 {
        self.phys_addr
    }

    /// Mark all VMCB fields dirty
    pub fn mark_all_dirty(&mut self) {
        unsafe {
            (*self.control).vmcb_clean = 0;
        }
    }

    /// Mark all VMCB fields clean
    pub fn mark_all_clean(&mut self) {
        unsafe {
            (*self.control).vmcb_clean = 0xFFFFFFFF;
        }
    }

    /// Set intercepts
    pub fn set_intercept(&mut self, intercept1: u32, intercept2: u32) {
        let control = self.control_mut();
        control.intercept_misc1 = intercept1;
        control.intercept_misc2 = intercept2;
    }

    /// Set ASID
    pub fn set_asid(&mut self, asid: u32) {
        self.control_mut().guest_asid = asid;
    }

    /// Enable nested paging
    pub fn enable_npt(&mut self, ncr3: u64) {
        let control = self.control_mut();
        control.np_enable = 1;
        control.n_cr3 = ncr3;
    }

    /// Run the VM
    pub fn run(&mut self) -> u64 {
        // Would execute VMRUN instruction
        // For now, return HLT exit code
        exit_code::HLT
    }

    /// Inject event
    pub fn inject_event(&mut self, vector: u8, event_type: u8, error_code: Option<u32>) {
        let mut event_inj: u64 = vector as u64;
        event_inj |= (event_type as u64) << 8;
        event_inj |= 1 << 31; // Valid

        if let Some(ec) = error_code {
            event_inj |= 1 << 11; // Error code valid
            event_inj |= (ec as u64) << 32;
        }

        self.control_mut().event_inj = event_inj;
    }
}

impl Drop for Vmcb {
    fn drop(&mut self) {
        free_page(self.phys_addr);
    }
}

// =============================================================================
// NPT (Nested Page Tables)
// =============================================================================

/// NPT entry flags
pub mod npt_flags {
    pub const PRESENT: u64 = 1 << 0;
    pub const WRITE: u64 = 1 << 1;
    pub const USER: u64 = 1 << 2;
    pub const PWT: u64 = 1 << 3;
    pub const PCD: u64 = 1 << 4;
    pub const ACCESSED: u64 = 1 << 5;
    pub const DIRTY: u64 = 1 << 6;
    pub const LARGE: u64 = 1 << 7;
    pub const GLOBAL: u64 = 1 << 8;
    pub const NX: u64 = 1 << 63;
}

/// Nested page table manager
pub struct NestedPageTable {
    /// Root page table physical address
    root: u64,
}

impl NestedPageTable {
    /// Create new nested page table
    pub fn new() -> Result<Self, KvmError> {
        let root = allocate_page()?;

        // Zero the root page
        unsafe {
            core::ptr::write_bytes(root as *mut u8, 0, 4096);
        }

        Ok(Self { root })
    }

    /// Get root physical address
    pub fn root(&self) -> u64 {
        self.root
    }

    /// Map guest physical address to host physical address
    pub fn map(&mut self, gpa: u64, hpa: u64, flags: u64) -> Result<(), KvmError> {
        // Would walk and create page table entries
        let _ = (gpa, hpa, flags);
        Ok(())
    }

    /// Unmap guest physical address
    pub fn unmap(&mut self, gpa: u64) -> Result<(), KvmError> {
        let _ = gpa;
        Ok(())
    }

    /// Invalidate all TLB entries for this ASID
    pub fn invalidate_all(&self) {
        // Would issue INVLPGA or set TLB_CTL in VMCB
    }
}

impl Drop for NestedPageTable {
    fn drop(&mut self) {
        // Would free all page table pages
        free_page(self.root);
    }
}

// =============================================================================
// AVIC (Advanced Virtual Interrupt Controller)
// =============================================================================

/// AVIC state
pub struct Avic {
    /// Backing page physical address
    backing_page: u64,
    /// Logical table page
    logical_table: u64,
    /// Physical table page
    physical_table: u64,
}

impl Avic {
    /// Create new AVIC
    pub fn new() -> Result<Self, KvmError> {
        Ok(Self {
            backing_page: allocate_page()?,
            logical_table: allocate_page()?,
            physical_table: allocate_page()?,
        })
    }

    /// Configure VMCB for AVIC
    pub fn configure_vmcb(&self, vmcb: &mut Vmcb) {
        let control = vmcb.control_mut();
        control.avic_backing_page = self.backing_page | 0xFFF; // Enable AVIC
        control.avic_logical_id = self.logical_table;
        control.avic_physical_id = self.physical_table;
    }

    /// Set APIC base
    pub fn set_apic_base(&mut self, vmcb: &mut Vmcb, base: u64) {
        vmcb.control_mut().avic_apic_bar = base;
    }
}

impl Drop for Avic {
    fn drop(&mut self) {
        free_page(self.backing_page);
        free_page(self.logical_table);
        free_page(self.physical_table);
    }
}

// =============================================================================
// THREAD SAFETY
// =============================================================================

// Safety: Vmcb is protected by higher-level synchronization (VM's Mutex).
// The raw pointers point to VMCB memory that is properly synchronized.
unsafe impl Send for Vmcb {}
unsafe impl Sync for Vmcb {}
