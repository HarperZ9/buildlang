// ===============================================================================
// BUILDOS KERNEL - VIRTUAL CPU (vCPU)
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Virtual CPU (vCPU) Implementation
//!
//! Manages virtual CPU state and execution:
//! - Register state management
//! - vCPU scheduling
//! - Interrupt injection
//! - State save/restore

#![allow(dead_code)]

use core::sync::atomic::{AtomicU32, AtomicU64, AtomicBool, Ordering};

use super::{KvmError, KvmRegs, KvmSregs, KvmFpu, KvmRun, KvmExitReason};
use super::vmx::Vmcs;
use super::svm::Vmcb;

// =============================================================================
// VCPU STATE
// =============================================================================

/// vCPU MP state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MpState {
    /// Running
    Runnable = 0,
    /// Uninitialized
    Uninitialized = 1,
    /// Init received
    InitReceived = 2,
    /// Halted
    Halted = 3,
    /// Waiting for SIPI
    SipiReceived = 4,
    /// Stopped
    Stopped = 5,
    /// Check stop
    CheckStop = 6,
    /// Operating
    Operating = 7,
    /// Load
    Load = 8,
}

/// vCPU run state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    /// Ready to run
    Ready,
    /// Currently running
    Running,
    /// Paused
    Paused,
    /// Exited
    Exited,
}

/// Architecture-specific vCPU state
pub enum ArchVcpu {
    /// Intel VMX
    Vmx(VmxVcpu),
    /// AMD SVM
    Svm(SvmVcpu),
}

/// VMX-specific vCPU state
pub struct VmxVcpu {
    /// VMCS
    vmcs: Vmcs,
    /// VPID
    vpid: u16,
    /// EPT pointer
    eptp: u64,
    /// MSR bitmap address
    msr_bitmap: u64,
}

/// SVM-specific vCPU state
pub struct SvmVcpu {
    /// VMCB
    vmcb: Vmcb,
    /// ASID
    asid: u32,
    /// Nested page table root
    npt_root: u64,
    /// MSR permission map address
    msrpm: u64,
}

// =============================================================================
// VCPU STRUCTURE
// =============================================================================

/// Virtual CPU
pub struct VCpu {
    /// vCPU ID
    id: u32,
    /// Parent VM ID
    vm_id: u32,
    /// Architecture-specific state
    arch: ArchVcpu,
    /// General-purpose registers (cached)
    regs: KvmRegs,
    /// Special registers (cached)
    sregs: KvmSregs,
    /// FPU state (cached)
    fpu: KvmFpu,
    /// Run structure for userspace communication
    kvm_run: KvmRun,
    /// MP state
    mp_state: AtomicU32,
    /// Run state
    run_state: AtomicU32,
    /// Pending interrupt
    pending_interrupt: AtomicU32,
    /// Interrupt pending flag
    has_interrupt: AtomicBool,
    /// NMI pending
    nmi_pending: AtomicBool,
    /// Number of exits
    exit_count: AtomicU64,
    /// Host CPU affinity
    host_cpu: AtomicU32,
}

impl VCpu {
    /// Create new VMX vCPU
    pub fn new_vmx(vm_id: u32, vcpu_id: u32) -> Result<Self, KvmError> {
        let vmcs = Vmcs::new()?;

        // Allocate VPID
        let vpid = allocate_vpid();

        Ok(Self {
            id: vcpu_id,
            vm_id,
            arch: ArchVcpu::Vmx(VmxVcpu {
                vmcs,
                vpid,
                eptp: 0,
                msr_bitmap: 0,
            }),
            regs: KvmRegs::default(),
            sregs: KvmSregs::default(),
            fpu: KvmFpu::default(),
            kvm_run: unsafe { core::mem::zeroed() },
            mp_state: AtomicU32::new(MpState::Runnable as u32),
            run_state: AtomicU32::new(RunState::Ready as u32),
            pending_interrupt: AtomicU32::new(0),
            has_interrupt: AtomicBool::new(false),
            nmi_pending: AtomicBool::new(false),
            exit_count: AtomicU64::new(0),
            host_cpu: AtomicU32::new(0xFFFFFFFF),
        })
    }

    /// Create new SVM vCPU
    pub fn new_svm(vm_id: u32, vcpu_id: u32) -> Result<Self, KvmError> {
        let vmcb = Vmcb::new()?;

        // Allocate ASID
        let asid = allocate_asid();

        Ok(Self {
            id: vcpu_id,
            vm_id,
            arch: ArchVcpu::Svm(SvmVcpu {
                vmcb,
                asid,
                npt_root: 0,
                msrpm: 0,
            }),
            regs: KvmRegs::default(),
            sregs: KvmSregs::default(),
            fpu: KvmFpu::default(),
            kvm_run: unsafe { core::mem::zeroed() },
            mp_state: AtomicU32::new(MpState::Runnable as u32),
            run_state: AtomicU32::new(RunState::Ready as u32),
            pending_interrupt: AtomicU32::new(0),
            has_interrupt: AtomicBool::new(false),
            nmi_pending: AtomicBool::new(false),
            exit_count: AtomicU64::new(0),
            host_cpu: AtomicU32::new(0xFFFFFFFF),
        })
    }

    /// Get vCPU ID
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Get VM ID
    pub fn vm_id(&self) -> u32 {
        self.vm_id
    }

    /// Get registers
    pub fn get_regs(&self) -> &KvmRegs {
        &self.regs
    }

    /// Set registers
    pub fn set_regs(&mut self, regs: &KvmRegs) -> Result<(), KvmError> {
        self.regs = regs.clone();
        self.sync_regs_to_arch()?;
        Ok(())
    }

    /// Get special registers
    pub fn get_sregs(&self) -> &KvmSregs {
        &self.sregs
    }

    /// Set special registers
    pub fn set_sregs(&mut self, sregs: &KvmSregs) -> Result<(), KvmError> {
        self.sregs = sregs.clone();
        self.sync_sregs_to_arch()?;
        Ok(())
    }

    /// Get FPU state
    pub fn get_fpu(&self) -> &KvmFpu {
        &self.fpu
    }

    /// Set FPU state
    pub fn set_fpu(&mut self, fpu: &KvmFpu) -> Result<(), KvmError> {
        self.fpu = fpu.clone();
        Ok(())
    }

    /// Get MP state
    pub fn get_mp_state(&self) -> MpState {
        let state = self.mp_state.load(Ordering::SeqCst);
        unsafe { core::mem::transmute(state) }
    }

    /// Set MP state
    pub fn set_mp_state(&self, state: MpState) {
        self.mp_state.store(state as u32, Ordering::SeqCst);
    }

    /// Get KVM run structure pointer
    pub fn kvm_run_ptr(&mut self) -> *mut KvmRun {
        &mut self.kvm_run
    }

    /// Run vCPU
    pub fn run(&mut self) -> Result<KvmExitReason, KvmError> {
        // Check MP state
        let mp_state = self.get_mp_state();
        match mp_state {
            MpState::Halted => {
                // Check for pending interrupt
                if !self.has_interrupt.load(Ordering::SeqCst) &&
                   !self.nmi_pending.load(Ordering::SeqCst) {
                    return Ok(KvmExitReason::Hlt);
                }
            }
            MpState::Uninitialized | MpState::InitReceived => {
                return Err(KvmError::InvalidState);
            }
            _ => {}
        }

        // Inject pending interrupt if possible
        self.try_inject_interrupt()?;

        // Run the vCPU
        let exit_reason = self.enter_guest()?;

        // Handle VM exit
        self.handle_exit(exit_reason)
    }

    /// Enter guest mode
    fn enter_guest(&mut self) -> Result<u32, KvmError> {
        self.run_state.store(RunState::Running as u32, Ordering::SeqCst);
        self.exit_count.fetch_add(1, Ordering::Relaxed);

        let exit_reason = match &mut self.arch {
            ArchVcpu::Vmx(vmx) => {
                vmx.vmcs.load()?;
                vmx.vmcs.launch()?
            }
            ArchVcpu::Svm(svm) => {
                svm.vmcb.run() as u32
            }
        };

        self.run_state.store(RunState::Ready as u32, Ordering::SeqCst);

        // Sync registers from hardware
        self.sync_regs_from_arch()?;

        Ok(exit_reason)
    }

    /// Handle VM exit
    fn handle_exit(&mut self, exit_reason: u32) -> Result<KvmExitReason, KvmError> {
        match &self.arch {
            ArchVcpu::Vmx(_) => self.handle_vmx_exit(exit_reason),
            ArchVcpu::Svm(_) => self.handle_svm_exit(exit_reason as u64),
        }
    }

    /// Handle VMX exit
    fn handle_vmx_exit(&mut self, exit_reason: u32) -> Result<KvmExitReason, KvmError> {
        use super::vmx::exit_reason;

        match exit_reason {
            exit_reason::EXCEPTION_NMI => {
                Ok(KvmExitReason::Exception)
            }
            exit_reason::EXTERNAL_INTERRUPT => {
                Ok(KvmExitReason::Intr)
            }
            exit_reason::TRIPLE_FAULT => {
                Ok(KvmExitReason::Shutdown)
            }
            exit_reason::HLT => {
                self.set_mp_state(MpState::Halted);
                Ok(KvmExitReason::Hlt)
            }
            exit_reason::IO_INSTRUCTION => {
                self.handle_io_exit()
            }
            exit_reason::CPUID => {
                self.handle_cpuid()?;
                // Re-enter guest
                self.run()
            }
            exit_reason::RDMSR | exit_reason::WRMSR => {
                // Would handle MSR access
                Ok(KvmExitReason::Unknown)
            }
            exit_reason::EPT_VIOLATION => {
                self.handle_mmio_exit()
            }
            exit_reason::VMCALL => {
                Ok(KvmExitReason::Hypercall)
            }
            exit_reason::PREEMPTION_TIMER => {
                Ok(KvmExitReason::Intr)
            }
            _ => {
                Ok(KvmExitReason::Unknown)
            }
        }
    }

    /// Handle SVM exit
    fn handle_svm_exit(&mut self, exit_code: u64) -> Result<KvmExitReason, KvmError> {
        use super::svm::exit_code;

        match exit_code {
            exit_code::INTR => {
                Ok(KvmExitReason::Intr)
            }
            exit_code::NMI => {
                Ok(KvmExitReason::Nmi)
            }
            exit_code::HLT => {
                self.set_mp_state(MpState::Halted);
                Ok(KvmExitReason::Hlt)
            }
            exit_code::IOIO => {
                self.handle_io_exit()
            }
            exit_code::CPUID => {
                self.handle_cpuid()?;
                self.run()
            }
            exit_code::MSR => {
                Ok(KvmExitReason::Unknown)
            }
            exit_code::NPF => {
                self.handle_mmio_exit()
            }
            exit_code::VMMCALL => {
                Ok(KvmExitReason::Hypercall)
            }
            exit_code::SHUTDOWN => {
                Ok(KvmExitReason::Shutdown)
            }
            _ => {
                Ok(KvmExitReason::Unknown)
            }
        }
    }

    /// Handle I/O exit
    fn handle_io_exit(&mut self) -> Result<KvmExitReason, KvmError> {
        // Would populate kvm_run with I/O details
        self.kvm_run.exit_reason = KvmExitReason::Io as u32;
        Ok(KvmExitReason::Io)
    }

    /// Handle MMIO exit
    fn handle_mmio_exit(&mut self) -> Result<KvmExitReason, KvmError> {
        // Would populate kvm_run with MMIO details
        self.kvm_run.exit_reason = KvmExitReason::Mmio as u32;
        Ok(KvmExitReason::Mmio)
    }

    /// Handle CPUID instruction
    fn handle_cpuid(&mut self) -> Result<(), KvmError> {
        let eax = self.regs.rax as u32;
        let ecx = self.regs.rcx as u32;

        // Execute CPUID
        let result = unsafe { core::arch::x86_64::__cpuid_count(eax, ecx) };

        // Store results
        self.regs.rax = result.eax as u64;
        self.regs.rbx = result.ebx as u64;
        self.regs.rcx = result.ecx as u64;
        self.regs.rdx = result.edx as u64;

        // Advance RIP
        self.regs.rip += 2; // CPUID is 2 bytes

        self.sync_regs_to_arch()
    }

    /// Try to inject pending interrupt
    fn try_inject_interrupt(&mut self) -> Result<(), KvmError> {
        if !self.has_interrupt.load(Ordering::SeqCst) {
            return Ok(());
        }

        // Check if interrupts are enabled in guest
        if (self.sregs.efer & 0x400) == 0 { // Check IF flag
            return Ok(());
        }

        let vector = self.pending_interrupt.load(Ordering::SeqCst);

        match &mut self.arch {
            ArchVcpu::Vmx(vmx) => {
                // Set VM-entry interrupt info
                vmx.vmcs.write(
                    super::vmx::vmcs::VM_ENTRY_INTR_INFO_FIELD,
                    (vector as u64) | (1 << 31), // Valid bit
                )?;
            }
            ArchVcpu::Svm(svm) => {
                svm.vmcb.inject_event(vector as u8, 0, None);
            }
        }

        self.has_interrupt.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Queue interrupt
    pub fn queue_interrupt(&self, vector: u32) {
        self.pending_interrupt.store(vector, Ordering::SeqCst);
        self.has_interrupt.store(true, Ordering::SeqCst);

        // Wake up halted vCPU
        if self.get_mp_state() == MpState::Halted {
            self.set_mp_state(MpState::Runnable);
        }
    }

    /// Queue NMI
    pub fn queue_nmi(&self) {
        self.nmi_pending.store(true, Ordering::SeqCst);

        if self.get_mp_state() == MpState::Halted {
            self.set_mp_state(MpState::Runnable);
        }
    }

    /// Sync registers to architecture-specific state
    fn sync_regs_to_arch(&mut self) -> Result<(), KvmError> {
        match &mut self.arch {
            ArchVcpu::Vmx(vmx) => {
                use super::vmx::vmcs;

                vmx.vmcs.write(vmcs::GUEST_RAX, self.regs.rax)?;
                vmx.vmcs.write(vmcs::GUEST_RSP, self.regs.rsp)?;
                vmx.vmcs.write(vmcs::GUEST_RIP, self.regs.rip)?;
                vmx.vmcs.write(vmcs::GUEST_RFLAGS, self.regs.rflags)?;
                // Other registers are saved in memory
            }
            ArchVcpu::Svm(svm) => {
                let state = svm.vmcb.state_mut();
                state.rax = self.regs.rax;
                state.rsp = self.regs.rsp;
                state.rip = self.regs.rip;
                state.rflags = self.regs.rflags;
            }
        }
        Ok(())
    }

    /// Sync registers from architecture-specific state
    fn sync_regs_from_arch(&mut self) -> Result<(), KvmError> {
        match &self.arch {
            ArchVcpu::Vmx(vmx) => {
                use super::vmx::vmcs;

                self.regs.rax = vmx.vmcs.read(vmcs::GUEST_RSP)?; // Note: should be guest RAX storage
                self.regs.rsp = vmx.vmcs.read(vmcs::GUEST_RSP)?;
                self.regs.rip = vmx.vmcs.read(vmcs::GUEST_RIP)?;
                self.regs.rflags = vmx.vmcs.read(vmcs::GUEST_RFLAGS)?;
            }
            ArchVcpu::Svm(svm) => {
                let state = svm.vmcb.state();
                self.regs.rax = state.rax;
                self.regs.rsp = state.rsp;
                self.regs.rip = state.rip;
                self.regs.rflags = state.rflags;
            }
        }
        Ok(())
    }

    /// Sync special registers to architecture-specific state
    fn sync_sregs_to_arch(&mut self) -> Result<(), KvmError> {
        match &mut self.arch {
            ArchVcpu::Vmx(vmx) => {
                use super::vmx::vmcs;

                vmx.vmcs.write(vmcs::GUEST_CR0, self.sregs.cr0)?;
                vmx.vmcs.write(vmcs::GUEST_CR3, self.sregs.cr3)?;
                vmx.vmcs.write(vmcs::GUEST_CR4, self.sregs.cr4)?;
            }
            ArchVcpu::Svm(svm) => {
                let state = svm.vmcb.state_mut();
                state.cr0 = self.sregs.cr0;
                state.cr3 = self.sregs.cr3;
                state.cr4 = self.sregs.cr4;
                state.efer = self.sregs.efer;
            }
        }
        Ok(())
    }

    /// Get exit count
    pub fn exit_count(&self) -> u64 {
        self.exit_count.load(Ordering::Relaxed)
    }

    /// Set host CPU affinity
    pub fn set_host_cpu(&self, cpu: u32) {
        self.host_cpu.store(cpu, Ordering::SeqCst);
    }

    /// Get host CPU affinity
    pub fn host_cpu(&self) -> Option<u32> {
        let cpu = self.host_cpu.load(Ordering::SeqCst);
        if cpu == 0xFFFFFFFF {
            None
        } else {
            Some(cpu)
        }
    }
}

// =============================================================================
// VPID/ASID ALLOCATION
// =============================================================================

/// Next VPID
static NEXT_VPID: AtomicU32 = AtomicU32::new(1);

/// Next ASID
static NEXT_ASID: AtomicU32 = AtomicU32::new(1);

/// Allocate VPID
fn allocate_vpid() -> u16 {
    let vpid = NEXT_VPID.fetch_add(1, Ordering::SeqCst);
    if vpid >= 0xFFFF {
        NEXT_VPID.store(1, Ordering::SeqCst);
        1
    } else {
        vpid as u16
    }
}

/// Allocate ASID
fn allocate_asid() -> u32 {
    let asid = NEXT_ASID.fetch_add(1, Ordering::SeqCst);
    if asid >= 0xFFFF {
        NEXT_ASID.store(1, Ordering::SeqCst);
        1
    } else {
        asid
    }
}

// =============================================================================
// VCPU DEBUG
// =============================================================================

/// Debug registers
#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct KvmDebugRegs {
    pub db: [u64; 4],
    pub dr6: u64,
    pub dr7: u64,
    pub flags: u64,
    pub reserved: [u64; 9],
}

/// Guest debug configuration
#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct KvmGuestDebug {
    pub control: u32,
    pub pad: u32,
    pub arch: KvmGuestDebugArch,
}

/// Architecture-specific debug config
#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct KvmGuestDebugArch {
    pub debugreg: [u64; 8],
}

impl VCpu {
    /// Set guest debug state
    pub fn set_guest_debug(&mut self, debug: &KvmGuestDebug) -> Result<(), KvmError> {
        // Would configure debug registers in VMCS/VMCB
        let _ = debug;
        Ok(())
    }

    /// Get debug registers
    pub fn get_debug_regs(&self) -> KvmDebugRegs {
        KvmDebugRegs::default()
    }

    /// Set debug registers
    pub fn set_debug_regs(&mut self, _regs: &KvmDebugRegs) -> Result<(), KvmError> {
        Ok(())
    }
}
