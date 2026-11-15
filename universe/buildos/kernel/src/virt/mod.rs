// ===============================================================================
// BUILDOS KERNEL - VIRTUALIZATION SUBSYSTEM
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! KVM-Compatible Virtualization Support
//!
//! Provides hardware-assisted virtualization using Intel VT-x/AMD-V:
//! - Virtual machine creation and management
//! - vCPU scheduling and execution
//! - Memory virtualization with EPT/NPT
//! - I/O virtualization and device emulation
//! - Nested virtualization support

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicBool, Ordering};

use crate::sync::{Mutex, RwLock};

pub mod vmx;
pub mod svm;
pub mod vcpu;
pub mod memory;
pub mod io;
pub mod irq;

// =============================================================================
// VIRTUALIZATION CAPABILITY DETECTION
// =============================================================================

/// Virtualization technology
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtTech {
    /// Intel VT-x
    IntelVmx,
    /// AMD-V (SVM)
    AmdSvm,
    /// No hardware virtualization
    None,
}

/// Virtualization capabilities
#[derive(Debug, Clone)]
pub struct VirtCapabilities {
    /// Technology available
    pub tech: VirtTech,
    /// EPT/NPT support
    pub ept_supported: bool,
    /// Unrestricted guest support
    pub unrestricted_guest: bool,
    /// VPID support
    pub vpid_supported: bool,
    /// Posted interrupts support
    pub posted_interrupts: bool,
    /// Nested virtualization
    pub nested_virt: bool,
    /// Maximum vCPUs per VM
    pub max_vcpus: u32,
    /// Maximum VMs
    pub max_vms: u32,
    /// VMCS revision ID (Intel)
    pub vmcs_revision: u32,
}

impl VirtCapabilities {
    /// Detect virtualization capabilities
    pub fn detect() -> Self {
        let tech = Self::detect_tech();

        match tech {
            VirtTech::IntelVmx => Self::detect_vmx_caps(),
            VirtTech::AmdSvm => Self::detect_svm_caps(),
            VirtTech::None => Self::none(),
        }
    }

    fn detect_tech() -> VirtTech {
        // Check CPUID for VMX/SVM support
        let cpuid = unsafe { core::arch::x86_64::__cpuid(1) };

        // Check VMX bit (Intel)
        if (cpuid.ecx & (1 << 5)) != 0 {
            return VirtTech::IntelVmx;
        }

        // Check for AMD
        let cpuid_ext = unsafe { core::arch::x86_64::__cpuid(0x80000001) };
        if (cpuid_ext.ecx & (1 << 2)) != 0 {
            return VirtTech::AmdSvm;
        }

        VirtTech::None
    }

    fn detect_vmx_caps() -> Self {
        // Read VMX capability MSRs
        let vmx_basic = unsafe { crate::cpu::rdmsr(0x480) }; // IA32_VMX_BASIC
        let vmcs_revision = (vmx_basic & 0x7FFFFFFF) as u32;

        let vmx_procbased = unsafe { crate::cpu::rdmsr(0x482) };
        let secondary_allowed = (vmx_procbased >> 32) & (1 << 31) != 0;

        let mut ept_supported = false;
        let mut unrestricted_guest = false;
        let mut vpid_supported = false;

        if secondary_allowed {
            let vmx_procbased2 = unsafe { crate::cpu::rdmsr(0x48B) };
            ept_supported = (vmx_procbased2 >> 32) & (1 << 1) != 0;
            unrestricted_guest = (vmx_procbased2 >> 32) & (1 << 7) != 0;
            vpid_supported = (vmx_procbased2 >> 32) & (1 << 5) != 0;
        }

        Self {
            tech: VirtTech::IntelVmx,
            ept_supported,
            unrestricted_guest,
            vpid_supported,
            posted_interrupts: false, // Would check PIN_BASED_CTLS
            nested_virt: false, // Would check VMCS shadowing
            max_vcpus: 256,
            max_vms: 64,
            vmcs_revision,
        }
    }

    fn detect_svm_caps() -> Self {
        let svm_features = unsafe { core::arch::x86_64::__cpuid(0x8000000A) };

        Self {
            tech: VirtTech::AmdSvm,
            ept_supported: (svm_features.edx & (1 << 0)) != 0, // NPT
            unrestricted_guest: true, // SVM always supports real mode
            vpid_supported: (svm_features.edx & (1 << 10)) != 0, // ASID
            posted_interrupts: (svm_features.edx & (1 << 4)) != 0, // AVIC
            nested_virt: (svm_features.edx & (1 << 5)) != 0,
            max_vcpus: 256,
            max_vms: 64,
            vmcs_revision: 0,
        }
    }

    fn none() -> Self {
        Self {
            tech: VirtTech::None,
            ept_supported: false,
            unrestricted_guest: false,
            vpid_supported: false,
            posted_interrupts: false,
            nested_virt: false,
            max_vcpus: 0,
            max_vms: 0,
            vmcs_revision: 0,
        }
    }
}

// =============================================================================
// KVM IOCTL DEFINITIONS
// =============================================================================

/// KVM ioctl commands
pub mod kvm_ioctl {
    /// Get KVM API version
    pub const KVM_GET_API_VERSION: u64 = 0xAE00;
    /// Create VM
    pub const KVM_CREATE_VM: u64 = 0xAE01;
    /// Check extension
    pub const KVM_CHECK_EXTENSION: u64 = 0xAE03;
    /// Get vCPU mmap size
    pub const KVM_GET_VCPU_MMAP_SIZE: u64 = 0xAE04;
    /// Get supported CPUID
    pub const KVM_GET_SUPPORTED_CPUID: u64 = 0xAE05;

    // VM ioctls
    /// Create vCPU
    pub const KVM_CREATE_VCPU: u64 = 0xAE41;
    /// Set user memory region
    pub const KVM_SET_USER_MEMORY_REGION: u64 = 0xAE46;
    /// Create interrupt controller
    pub const KVM_CREATE_IRQCHIP: u64 = 0xAE60;
    /// Create PIT
    pub const KVM_CREATE_PIT2: u64 = 0xAE77;
    /// Set identity map address
    pub const KVM_SET_IDENTITY_MAP_ADDR: u64 = 0xAE48;
    /// Set TSS address
    pub const KVM_SET_TSS_ADDR: u64 = 0xAE47;
    /// IRQ line
    pub const KVM_IRQ_LINE: u64 = 0xAE61;
    /// Signal MSI
    pub const KVM_SIGNAL_MSI: u64 = 0xAE65;

    // vCPU ioctls
    /// Run vCPU
    pub const KVM_RUN: u64 = 0xAE80;
    /// Get registers
    pub const KVM_GET_REGS: u64 = 0xAE81;
    /// Set registers
    pub const KVM_SET_REGS: u64 = 0xAE82;
    /// Get special registers
    pub const KVM_GET_SREGS: u64 = 0xAE83;
    /// Set special registers
    pub const KVM_SET_SREGS: u64 = 0xAE84;
    /// Translate address
    pub const KVM_TRANSLATE: u64 = 0xAE85;
    /// Interrupt vCPU
    pub const KVM_INTERRUPT: u64 = 0xAE86;
    /// Get CPUID
    pub const KVM_GET_CPUID2: u64 = 0xAE91;
    /// Set CPUID
    pub const KVM_SET_CPUID2: u64 = 0xAE90;
    /// Get MSRs
    pub const KVM_GET_MSRS: u64 = 0xAE88;
    /// Set MSRs
    pub const KVM_SET_MSRS: u64 = 0xAE89;
    /// Get FPU state
    pub const KVM_GET_FPU: u64 = 0xAE8C;
    /// Set FPU state
    pub const KVM_SET_FPU: u64 = 0xAE8D;
    /// Get LAPIC state
    pub const KVM_GET_LAPIC: u64 = 0xAE8E;
    /// Set LAPIC state
    pub const KVM_SET_LAPIC: u64 = 0xAE8F;
}

/// KVM extensions
pub mod kvm_cap {
    pub const KVM_CAP_IRQCHIP: u32 = 0;
    pub const KVM_CAP_HLT: u32 = 1;
    pub const KVM_CAP_MMU_SHADOW_CACHE_CONTROL: u32 = 2;
    pub const KVM_CAP_USER_MEMORY: u32 = 3;
    pub const KVM_CAP_SET_TSS_ADDR: u32 = 4;
    pub const KVM_CAP_VAPIC: u32 = 6;
    pub const KVM_CAP_EXT_CPUID: u32 = 7;
    pub const KVM_CAP_CLOCKSOURCE: u32 = 8;
    pub const KVM_CAP_NR_VCPUS: u32 = 9;
    pub const KVM_CAP_NR_MEMSLOTS: u32 = 10;
    pub const KVM_CAP_PIT: u32 = 11;
    pub const KVM_CAP_NOP_IO_DELAY: u32 = 12;
    pub const KVM_CAP_PV_MMU: u32 = 13;
    pub const KVM_CAP_MP_STATE: u32 = 14;
    pub const KVM_CAP_COALESCED_MMIO: u32 = 15;
    pub const KVM_CAP_SYNC_MMU: u32 = 16;
    pub const KVM_CAP_IOMMU: u32 = 18;
    pub const KVM_CAP_DESTROY_MEMORY_REGION_WORKS: u32 = 21;
    pub const KVM_CAP_USER_NMI: u32 = 22;
    pub const KVM_CAP_SET_GUEST_DEBUG: u32 = 23;
    pub const KVM_CAP_REINJECT_CONTROL: u32 = 24;
    pub const KVM_CAP_IRQ_ROUTING: u32 = 25;
    pub const KVM_CAP_IRQ_INJECT_STATUS: u32 = 26;
    pub const KVM_CAP_ASSIGN_DEV_IRQ: u32 = 29;
    pub const KVM_CAP_JOIN_MEMORY_REGIONS_WORKS: u32 = 30;
    pub const KVM_CAP_MCE: u32 = 31;
    pub const KVM_CAP_IRQFD: u32 = 32;
    pub const KVM_CAP_PIT2: u32 = 33;
    pub const KVM_CAP_SET_BOOT_CPU_ID: u32 = 34;
    pub const KVM_CAP_PIT_STATE2: u32 = 35;
    pub const KVM_CAP_IOEVENTFD: u32 = 36;
    pub const KVM_CAP_SET_IDENTITY_MAP_ADDR: u32 = 37;
    pub const KVM_CAP_XEN_HVM: u32 = 38;
    pub const KVM_CAP_X86_SMM: u32 = 117;
    pub const KVM_CAP_MULTI_ADDRESS_SPACE: u32 = 118;
}

// =============================================================================
// KVM RUN STRUCTURE
// =============================================================================

/// KVM run exit reasons
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum KvmExitReason {
    Unknown = 0,
    Exception = 1,
    Io = 2,
    Hypercall = 3,
    Debug = 4,
    Hlt = 5,
    Mmio = 6,
    IrqWindowOpen = 7,
    Shutdown = 8,
    FailEntry = 9,
    Intr = 10,
    SetTpr = 11,
    TprAccess = 12,
    S390Sieic = 13,
    S390Reset = 14,
    Dcr = 15,
    Nmi = 16,
    InternalError = 17,
    Osi = 18,
    PaprHcall = 19,
    S390Ucontrol = 20,
    Watchdog = 21,
    S390Tsch = 22,
    Epr = 23,
    SystemEvent = 24,
    S390Stsi = 25,
    IoapicEoi = 26,
    Hyperv = 27,
}

/// KVM run structure (shared between kernel and userspace)
#[repr(C)]
pub struct KvmRun {
    /// Request interrupt window
    pub request_interrupt_window: u8,
    /// Immediate exit
    pub immediate_exit: u8,
    /// Padding
    pub padding1: [u8; 6],

    /// Exit reason
    pub exit_reason: u32,
    /// Ready for interrupt injection
    pub ready_for_interrupt_injection: u8,
    /// If set, the CPU halted
    pub if_flag: u8,
    /// Flags
    pub flags: u16,

    /// CR8 value
    pub cr8: u64,
    /// APIC base
    pub apic_base: u64,

    /// Exit-specific data (union)
    pub exit_data: [u8; 256],

    /// Shared region for hypercall
    pub kvm_valid_regs: u64,
    pub kvm_dirty_regs: u64,

    /// Padding
    pub padding2: [u8; 2048],
}

/// I/O exit information
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct KvmExitIo {
    /// Direction: 0 = in, 1 = out
    pub direction: u8,
    /// Size: 1, 2, or 4
    pub size: u8,
    /// Port number
    pub port: u16,
    /// Repeat count
    pub count: u32,
    /// Data offset in KvmRun
    pub data_offset: u64,
}

/// MMIO exit information
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct KvmExitMmio {
    /// Physical address
    pub phys_addr: u64,
    /// Data (8 bytes max)
    pub data: [u8; 8],
    /// Length (1, 2, 4, or 8)
    pub len: u32,
    /// Is write
    pub is_write: u8,
    /// Padding
    pub padding: [u8; 3],
}

// =============================================================================
// REGISTER STRUCTURES
// =============================================================================

/// General-purpose registers
#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct KvmRegs {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rsp: u64,
    pub rbp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
    pub rflags: u64,
}

/// Segment descriptor
#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct KvmSegment {
    pub base: u64,
    pub limit: u32,
    pub selector: u16,
    pub type_: u8,
    pub present: u8,
    pub dpl: u8,
    pub db: u8,
    pub s: u8,
    pub l: u8,
    pub g: u8,
    pub avl: u8,
    pub unusable: u8,
    pub padding: u8,
}

/// Descriptor table register
#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct KvmDtable {
    pub base: u64,
    pub limit: u16,
    pub padding: [u16; 3],
}

/// Special registers
#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct KvmSregs {
    /// Code segment
    pub cs: KvmSegment,
    /// Data segment
    pub ds: KvmSegment,
    /// Extra segment
    pub es: KvmSegment,
    /// F segment
    pub fs: KvmSegment,
    /// G segment
    pub gs: KvmSegment,
    /// Stack segment
    pub ss: KvmSegment,
    /// Task register
    pub tr: KvmSegment,
    /// LDT
    pub ldt: KvmSegment,
    /// GDT
    pub gdt: KvmDtable,
    /// IDT
    pub idt: KvmDtable,
    /// Control registers
    pub cr0: u64,
    pub cr2: u64,
    pub cr3: u64,
    pub cr4: u64,
    pub cr8: u64,
    /// EFER
    pub efer: u64,
    /// APIC base
    pub apic_base: u64,
    /// Interrupt bitmap
    pub interrupt_bitmap: [u64; 4],
}

/// FPU state
#[repr(C)]
#[derive(Debug, Clone)]
pub struct KvmFpu {
    /// FPU registers (8 x 16 bytes)
    pub fpr: [[u8; 16]; 8],
    /// Control word
    pub fcw: u16,
    /// Status word
    pub fsw: u16,
    /// Tag word
    pub ftwx: u8,
    /// Padding
    pub pad1: u8,
    /// Opcode
    pub last_opcode: u16,
    /// IP
    pub last_ip: u64,
    /// DP
    pub last_dp: u64,
    /// XMM registers (16 x 16 bytes)
    pub xmm: [[u8; 16]; 16],
    /// MXCSR
    pub mxcsr: u32,
    /// Padding
    pub pad2: u32,
}

impl Default for KvmFpu {
    fn default() -> Self {
        Self {
            fpr: [[0; 16]; 8],
            fcw: 0x37F,
            fsw: 0,
            ftwx: 0,
            pad1: 0,
            last_opcode: 0,
            last_ip: 0,
            last_dp: 0,
            xmm: [[0; 16]; 16],
            mxcsr: 0x1F80,
            pad2: 0,
        }
    }
}

// =============================================================================
// MEMORY REGION
// =============================================================================

/// User memory region
#[repr(C)]
#[derive(Debug, Clone)]
pub struct KvmUserMemoryRegion {
    /// Slot number
    pub slot: u32,
    /// Flags
    pub flags: u32,
    /// Guest physical address
    pub guest_phys_addr: u64,
    /// Memory size
    pub memory_size: u64,
    /// Userspace address
    pub userspace_addr: u64,
}

/// Memory region flags
pub mod mem_flags {
    /// Log dirty pages
    pub const KVM_MEM_LOG_DIRTY_PAGES: u32 = 1;
    /// Read-only
    pub const KVM_MEM_READONLY: u32 = 2;
}

// =============================================================================
// VM STATE
// =============================================================================

/// Virtual machine
pub struct VirtualMachine {
    /// VM ID
    id: u32,
    /// vCPUs
    vcpus: RwLock<Vec<Arc<vcpu::VCpu>>>,
    /// Memory regions
    memory_regions: RwLock<BTreeMap<u32, MemoryRegion>>,
    /// Has in-kernel IRQ chip
    irqchip: AtomicBool,
    /// Has in-kernel PIT
    pit: AtomicBool,
    /// Next memory slot
    next_slot: AtomicU32,
    /// Reference count
    refs: AtomicU32,
}

/// Memory region in VM
struct MemoryRegion {
    /// Guest physical address
    guest_addr: u64,
    /// Host virtual address
    host_addr: u64,
    /// Size
    size: u64,
    /// Flags
    flags: u32,
}

impl VirtualMachine {
    /// Create new VM
    pub fn new(id: u32) -> Self {
        Self {
            id,
            vcpus: RwLock::new(Vec::new()),
            memory_regions: RwLock::new(BTreeMap::new()),
            irqchip: AtomicBool::new(false),
            pit: AtomicBool::new(false),
            next_slot: AtomicU32::new(0),
            refs: AtomicU32::new(1),
        }
    }

    /// Get VM ID
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Create vCPU
    pub fn create_vcpu(&self, vcpu_id: u32) -> Result<Arc<vcpu::VCpu>, KvmError> {
        let caps = VIRT_CAPS.read();

        let vcpu = match caps.tech {
            VirtTech::IntelVmx => vcpu::VCpu::new_vmx(self.id, vcpu_id)?,
            VirtTech::AmdSvm => vcpu::VCpu::new_svm(self.id, vcpu_id)?,
            VirtTech::None => return Err(KvmError::NotSupported),
        };

        let vcpu = Arc::new(vcpu);
        self.vcpus.write().push(vcpu.clone());

        Ok(vcpu)
    }

    /// Set user memory region
    pub fn set_memory_region(&self, region: &KvmUserMemoryRegion) -> Result<(), KvmError> {
        let mut regions = self.memory_regions.write();

        if region.memory_size == 0 {
            // Remove region
            regions.remove(&region.slot);
        } else {
            // Add/update region
            regions.insert(region.slot, MemoryRegion {
                guest_addr: region.guest_phys_addr,
                host_addr: region.userspace_addr,
                size: region.memory_size,
                flags: region.flags,
            });
        }

        Ok(())
    }

    /// Create in-kernel IRQ chip
    pub fn create_irqchip(&self) -> Result<(), KvmError> {
        self.irqchip.store(true, Ordering::SeqCst);
        // Would initialize LAPIC and IOAPIC emulation
        Ok(())
    }

    /// Create in-kernel PIT
    pub fn create_pit(&self) -> Result<(), KvmError> {
        self.pit.store(true, Ordering::SeqCst);
        // Would initialize PIT emulation
        Ok(())
    }

    /// Inject IRQ
    pub fn irq_line(&self, irq: u32, level: bool) -> Result<(), KvmError> {
        if !self.irqchip.load(Ordering::SeqCst) {
            return Err(KvmError::InvalidState);
        }

        // Would route IRQ to appropriate vCPU
        let _ = (irq, level);
        Ok(())
    }

    /// Translate guest address to host address
    pub fn translate_gpa(&self, gpa: u64) -> Option<u64> {
        let regions = self.memory_regions.read();

        for region in regions.values() {
            if gpa >= region.guest_addr && gpa < region.guest_addr + region.size {
                let offset = gpa - region.guest_addr;
                return Some(region.host_addr + offset);
            }
        }

        None
    }

    /// Get vCPU count
    pub fn vcpu_count(&self) -> usize {
        self.vcpus.read().len()
    }
}

// =============================================================================
// KVM DEVICE
// =============================================================================

/// KVM error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KvmError {
    /// Not supported
    NotSupported,
    /// Invalid argument
    InvalidArgument,
    /// Out of memory
    OutOfMemory,
    /// Already exists
    AlreadyExists,
    /// Not found
    NotFound,
    /// Invalid state
    InvalidState,
    /// Permission denied
    PermissionDenied,
    /// Hardware error
    HardwareError,
}

/// KVM global state
struct KvmState {
    /// VMs
    vms: BTreeMap<u32, Arc<VirtualMachine>>,
    /// Next VM ID
    next_vm_id: u32,
    /// Initialized
    initialized: bool,
}

impl KvmState {
    const fn new() -> Self {
        Self {
            vms: BTreeMap::new(),
            next_vm_id: 0,
            initialized: false,
        }
    }
}

/// Global KVM state
static KVM: Mutex<KvmState> = Mutex::new(KvmState::new());

/// Virtualization capabilities (cached)
static VIRT_CAPS: RwLock<VirtCapabilities> = RwLock::new(VirtCapabilities {
    tech: VirtTech::None,
    ept_supported: false,
    unrestricted_guest: false,
    vpid_supported: false,
    posted_interrupts: false,
    nested_virt: false,
    max_vcpus: 0,
    max_vms: 0,
    vmcs_revision: 0,
});

/// KVM API version
pub const KVM_API_VERSION: u32 = 12;

// =============================================================================
// KVM API
// =============================================================================

/// Initialize KVM subsystem
pub fn init() {
    let caps = VirtCapabilities::detect();

    match caps.tech {
        VirtTech::IntelVmx => {
            crate::kprintln!("[KVM] Intel VT-x detected");
            crate::kprintln!("[KVM] EPT: {}, VPID: {}, Unrestricted: {}",
                caps.ept_supported, caps.vpid_supported, caps.unrestricted_guest);

            // Enable VMX
            if let Err(_e) = vmx::enable_vmx() {
                crate::kprintln!("[KVM] Failed to enable VMX");
                return;
            }
        }
        VirtTech::AmdSvm => {
            crate::kprintln!("[KVM] AMD-V (SVM) detected");
            crate::kprintln!("[KVM] NPT: {}, AVIC: {}",
                caps.ept_supported, caps.posted_interrupts);

            // Enable SVM
            if let Err(_e) = svm::enable_svm() {
                crate::kprintln!("[KVM] Failed to enable SVM");
                return;
            }
        }
        VirtTech::None => {
            crate::kprintln!("[KVM] No hardware virtualization available");
            return;
        }
    }

    *VIRT_CAPS.write() = caps;

    let mut kvm = KVM.lock();
    kvm.initialized = true;

    crate::kprintln!("[KVM] Virtualization subsystem initialized");
}

/// Get API version
pub fn get_api_version() -> u32 {
    KVM_API_VERSION
}

/// Check extension
pub fn check_extension(extension: u32) -> i32 {
    let caps = VIRT_CAPS.read();

    match extension {
        kvm_cap::KVM_CAP_IRQCHIP => 1,
        kvm_cap::KVM_CAP_HLT => 1,
        kvm_cap::KVM_CAP_USER_MEMORY => 1,
        kvm_cap::KVM_CAP_SET_TSS_ADDR => 1,
        kvm_cap::KVM_CAP_EXT_CPUID => 1,
        kvm_cap::KVM_CAP_NR_VCPUS => caps.max_vcpus as i32,
        kvm_cap::KVM_CAP_NR_MEMSLOTS => 32,
        kvm_cap::KVM_CAP_PIT => 1,
        kvm_cap::KVM_CAP_PIT2 => 1,
        kvm_cap::KVM_CAP_SYNC_MMU => if caps.ept_supported { 1 } else { 0 },
        kvm_cap::KVM_CAP_COALESCED_MMIO => 1,
        kvm_cap::KVM_CAP_MP_STATE => 1,
        kvm_cap::KVM_CAP_IRQFD => 1,
        kvm_cap::KVM_CAP_IOEVENTFD => 1,
        kvm_cap::KVM_CAP_SET_IDENTITY_MAP_ADDR => 1,
        _ => 0,
    }
}

/// Create VM
pub fn create_vm() -> Result<Arc<VirtualMachine>, KvmError> {
    let mut kvm = KVM.lock();

    if !kvm.initialized {
        return Err(KvmError::NotSupported);
    }

    let vm_id = kvm.next_vm_id;
    kvm.next_vm_id += 1;

    let vm = Arc::new(VirtualMachine::new(vm_id));
    kvm.vms.insert(vm_id, vm.clone());

    Ok(vm)
}

/// Get VM by ID
pub fn get_vm(vm_id: u32) -> Option<Arc<VirtualMachine>> {
    KVM.lock().vms.get(&vm_id).cloned()
}

/// Destroy VM
pub fn destroy_vm(vm_id: u32) -> Result<(), KvmError> {
    let mut kvm = KVM.lock();
    kvm.vms.remove(&vm_id)
        .map(|_| ())
        .ok_or(KvmError::NotFound)
}

/// Get vCPU mmap size
pub fn get_vcpu_mmap_size() -> usize {
    core::mem::size_of::<KvmRun>()
}

/// Check if virtualization is available
pub fn is_available() -> bool {
    KVM.lock().initialized
}

/// Get virtualization technology
pub fn virt_tech() -> VirtTech {
    VIRT_CAPS.read().tech
}
