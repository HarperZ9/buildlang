// ===============================================================================
// BUILDOS KERNEL - REGISTER ACCESS FOR DEBUGGING
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Register Access for Debugging
//!
//! Provides access to CPU registers for ptrace and debugging:
//! - General purpose registers
//! - Floating point registers
//! - Debug registers (DR0-DR7)
//! - Segment registers
//! - System registers

#![allow(dead_code)]

use core::arch::asm;

// =============================================================================
// GENERAL PURPOSE REGISTERS (x86_64)
// =============================================================================

/// General purpose registers for x86_64
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct GeneralRegs {
    /// R15
    pub r15: u64,
    /// R14
    pub r14: u64,
    /// R13
    pub r13: u64,
    /// R12
    pub r12: u64,
    /// RBP (frame pointer)
    pub rbp: u64,
    /// RBX
    pub rbx: u64,
    /// R11
    pub r11: u64,
    /// R10
    pub r10: u64,
    /// R9
    pub r9: u64,
    /// R8
    pub r8: u64,
    /// RAX (accumulator)
    pub rax: u64,
    /// RCX (counter)
    pub rcx: u64,
    /// RDX (data)
    pub rdx: u64,
    /// RSI (source index)
    pub rsi: u64,
    /// RDI (destination index)
    pub rdi: u64,
    /// Original RAX (for syscall restart)
    pub orig_rax: u64,
    /// RIP (instruction pointer)
    pub rip: u64,
    /// CS (code segment)
    pub cs: u64,
    /// RFLAGS
    pub rflags: u64,
    /// RSP (stack pointer)
    pub rsp: u64,
    /// SS (stack segment)
    pub ss: u64,
    /// FS base
    pub fs_base: u64,
    /// GS base
    pub gs_base: u64,
    /// DS (data segment)
    pub ds: u64,
    /// ES (extra segment)
    pub es: u64,
    /// FS
    pub fs: u64,
    /// GS
    pub gs: u64,
}

impl GeneralRegs {
    /// Create new empty register set
    pub fn new() -> Self {
        Self::default()
    }

    /// Get register by index (GDB ordering)
    pub fn get(&self, index: usize) -> Option<u64> {
        match index {
            0 => Some(self.rax),
            1 => Some(self.rbx),
            2 => Some(self.rcx),
            3 => Some(self.rdx),
            4 => Some(self.rsi),
            5 => Some(self.rdi),
            6 => Some(self.rbp),
            7 => Some(self.rsp),
            8 => Some(self.r8),
            9 => Some(self.r9),
            10 => Some(self.r10),
            11 => Some(self.r11),
            12 => Some(self.r12),
            13 => Some(self.r13),
            14 => Some(self.r14),
            15 => Some(self.r15),
            16 => Some(self.rip),
            17 => Some(self.rflags),
            18 => Some(self.cs),
            19 => Some(self.ss),
            20 => Some(self.ds),
            21 => Some(self.es),
            22 => Some(self.fs),
            23 => Some(self.gs),
            _ => None,
        }
    }

    /// Set register by index (GDB ordering)
    pub fn set(&mut self, index: usize, value: u64) -> bool {
        match index {
            0 => self.rax = value,
            1 => self.rbx = value,
            2 => self.rcx = value,
            3 => self.rdx = value,
            4 => self.rsi = value,
            5 => self.rdi = value,
            6 => self.rbp = value,
            7 => self.rsp = value,
            8 => self.r8 = value,
            9 => self.r9 = value,
            10 => self.r10 = value,
            11 => self.r11 = value,
            12 => self.r12 = value,
            13 => self.r13 = value,
            14 => self.r14 = value,
            15 => self.r15 = value,
            16 => self.rip = value,
            17 => self.rflags = value,
            18 => self.cs = value,
            19 => self.ss = value,
            20 => self.ds = value,
            21 => self.es = value,
            22 => self.fs = value,
            23 => self.gs = value,
            _ => return false,
        }
        true
    }

    /// Get syscall number
    pub fn syscall_number(&self) -> u64 {
        self.orig_rax
    }

    /// Get syscall return value
    pub fn syscall_return(&self) -> i64 {
        self.rax as i64
    }

    /// Set syscall return value
    pub fn set_syscall_return(&mut self, value: i64) {
        self.rax = value as u64;
    }

    /// Get syscall arguments (arg0-arg5)
    pub fn syscall_args(&self) -> [u64; 6] {
        [self.rdi, self.rsi, self.rdx, self.r10, self.r8, self.r9]
    }
}

// =============================================================================
// FLOATING POINT REGISTERS
// =============================================================================

/// x87 FPU register (80-bit)
#[derive(Debug, Clone, Copy)]
#[repr(C, align(16))]
pub struct FpReg {
    /// Significand
    pub significand: [u16; 4],
    /// Exponent
    pub exponent: u16,
    /// Padding
    pub _padding: [u16; 3],
}

impl Default for FpReg {
    fn default() -> Self {
        Self {
            significand: [0; 4],
            exponent: 0,
            _padding: [0; 3],
        }
    }
}

/// XMM register (128-bit)
#[derive(Debug, Clone, Copy, Default)]
#[repr(C, align(16))]
pub struct XmmReg {
    /// Low 64 bits
    pub low: u64,
    /// High 64 bits
    pub high: u64,
}

/// YMM register (256-bit) - for AVX
#[derive(Debug, Clone, Copy, Default)]
#[repr(C, align(32))]
pub struct YmmReg {
    /// Low 128 bits (XMM portion)
    pub xmm: XmmReg,
    /// High 128 bits
    pub high: XmmReg,
}

/// ZMM register (512-bit) - for AVX-512
#[derive(Debug, Clone, Copy, Default)]
#[repr(C, align(64))]
pub struct ZmmReg {
    /// YMM portion (256 bits)
    pub ymm: YmmReg,
    /// High 256 bits
    pub high: YmmReg,
}

/// Floating point state (FXSAVE format)
#[derive(Debug, Clone, Copy)]
#[repr(C, align(16))]
pub struct FpState {
    /// Control word
    pub fcw: u16,
    /// Status word
    pub fsw: u16,
    /// Tag word
    pub ftw: u8,
    /// Reserved
    pub _reserved1: u8,
    /// FPU opcode
    pub fop: u16,
    /// FPU instruction pointer
    pub fip: u64,
    /// FPU data pointer
    pub fdp: u64,
    /// MXCSR register
    pub mxcsr: u32,
    /// MXCSR mask
    pub mxcsr_mask: u32,
    /// x87 FPU registers (ST0-ST7)
    pub st: [FpReg; 8],
    /// XMM registers (XMM0-XMM15)
    pub xmm: [XmmReg; 16],
    /// Reserved area
    pub _reserved2: [u8; 96],
}

impl Default for FpState {
    fn default() -> Self {
        Self {
            fcw: 0x37F,  // Default control word
            fsw: 0,
            ftw: 0xFF,
            _reserved1: 0,
            fop: 0,
            fip: 0,
            fdp: 0,
            mxcsr: 0x1F80,  // Default MXCSR
            mxcsr_mask: 0xFFFF,
            st: [FpReg::default(); 8],
            xmm: [XmmReg::default(); 16],
            _reserved2: [0; 96],
        }
    }
}

impl FpState {
    /// Create new FP state with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Save current FP state (FXSAVE)
    pub unsafe fn save(&mut self) {
        asm!(
            "fxsave [{}]",
            in(reg) self as *mut Self,
            options(nostack, preserves_flags)
        );
    }

    /// Restore FP state (FXRSTOR)
    pub unsafe fn restore(&self) {
        asm!(
            "fxrstor [{}]",
            in(reg) self as *const Self,
            options(nostack, preserves_flags)
        );
    }
}

/// Extended FP state (XSAVE format)
#[derive(Clone)]
#[repr(C, align(64))]
pub struct XSaveState {
    /// Legacy FXSAVE area
    pub fxsave: FpState,
    /// XSAVE header
    pub header: XSaveHeader,
    /// Extended state components (dynamically sized)
    pub extended: [u8; 2048],
}

/// XSAVE header
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct XSaveHeader {
    /// State components bitmap
    pub xstate_bv: u64,
    /// Compaction mode bitmap
    pub xcomp_bv: u64,
    /// Reserved
    pub _reserved: [u64; 6],
}

// =============================================================================
// DEBUG REGISTERS
// =============================================================================

/// Debug registers (DR0-DR7)
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct DebugRegs {
    /// DR0 - Linear address breakpoint 0
    pub dr0: u64,
    /// DR1 - Linear address breakpoint 1
    pub dr1: u64,
    /// DR2 - Linear address breakpoint 2
    pub dr2: u64,
    /// DR3 - Linear address breakpoint 3
    pub dr3: u64,
    /// DR6 - Debug status
    pub dr6: u64,
    /// DR7 - Debug control
    pub dr7: u64,
}

impl DebugRegs {
    /// Create new debug register set
    pub fn new() -> Self {
        Self::default()
    }

    /// Read current debug registers
    pub unsafe fn read_current() -> Self {
        let mut regs = Self::new();

        asm!("mov {}, dr0", out(reg) regs.dr0, options(nomem, nostack));
        asm!("mov {}, dr1", out(reg) regs.dr1, options(nomem, nostack));
        asm!("mov {}, dr2", out(reg) regs.dr2, options(nomem, nostack));
        asm!("mov {}, dr3", out(reg) regs.dr3, options(nomem, nostack));
        asm!("mov {}, dr6", out(reg) regs.dr6, options(nomem, nostack));
        asm!("mov {}, dr7", out(reg) regs.dr7, options(nomem, nostack));

        regs
    }

    /// Write debug registers to CPU
    pub unsafe fn write_current(&self) {
        asm!("mov dr0, {}", in(reg) self.dr0, options(nomem, nostack));
        asm!("mov dr1, {}", in(reg) self.dr1, options(nomem, nostack));
        asm!("mov dr2, {}", in(reg) self.dr2, options(nomem, nostack));
        asm!("mov dr3, {}", in(reg) self.dr3, options(nomem, nostack));
        asm!("mov dr6, {}", in(reg) self.dr6, options(nomem, nostack));
        asm!("mov dr7, {}", in(reg) self.dr7, options(nomem, nostack));
    }

    /// Get breakpoint address by index
    pub fn get_address(&self, index: u8) -> Option<u64> {
        match index {
            0 => Some(self.dr0),
            1 => Some(self.dr1),
            2 => Some(self.dr2),
            3 => Some(self.dr3),
            _ => None,
        }
    }

    /// Set breakpoint address by index
    pub fn set_address(&mut self, index: u8, addr: u64) -> bool {
        match index {
            0 => self.dr0 = addr,
            1 => self.dr1 = addr,
            2 => self.dr2 = addr,
            3 => self.dr3 = addr,
            _ => return false,
        }
        true
    }

    /// Check if breakpoint triggered
    pub fn breakpoint_hit(&self, index: u8) -> bool {
        if index < 4 {
            (self.dr6 & (1 << index)) != 0
        } else {
            false
        }
    }

    /// Clear breakpoint status
    pub fn clear_status(&mut self) {
        self.dr6 = 0;
    }

    /// Enable local breakpoint
    pub fn enable_local(&mut self, index: u8) {
        if index < 4 {
            self.dr7 |= 1 << (index * 2);
        }
    }

    /// Disable local breakpoint
    pub fn disable_local(&mut self, index: u8) {
        if index < 4 {
            self.dr7 &= !(1 << (index * 2));
        }
    }

    /// Enable global breakpoint
    pub fn enable_global(&mut self, index: u8) {
        if index < 4 {
            self.dr7 |= 1 << (index * 2 + 1);
        }
    }

    /// Disable global breakpoint
    pub fn disable_global(&mut self, index: u8) {
        if index < 4 {
            self.dr7 &= !(1 << (index * 2 + 1));
        }
    }

    /// Set breakpoint condition (0=exec, 1=write, 2=I/O, 3=read/write)
    pub fn set_condition(&mut self, index: u8, condition: u8) {
        if index < 4 && condition < 4 {
            let shift = 16 + index * 4;
            self.dr7 &= !(0b11 << shift);
            self.dr7 |= (condition as u64) << shift;
        }
    }

    /// Set breakpoint length (0=1 byte, 1=2 bytes, 2=8 bytes, 3=4 bytes)
    pub fn set_length(&mut self, index: u8, length: u8) {
        if index < 4 && length < 4 {
            let shift = 18 + index * 4;
            self.dr7 &= !(0b11 << shift);
            self.dr7 |= (length as u64) << shift;
        }
    }
}

// =============================================================================
// SEGMENT REGISTERS
// =============================================================================

/// Segment descriptor
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct SegmentDescriptor {
    /// Segment selector
    pub selector: u16,
    /// Base address
    pub base: u64,
    /// Limit
    pub limit: u32,
    /// Segment type
    pub stype: u8,
    /// Descriptor privilege level
    pub dpl: u8,
    /// Present bit
    pub present: bool,
    /// Long mode (64-bit)
    pub long_mode: bool,
    /// Default operation size (32-bit)
    pub db: bool,
    /// Granularity
    pub granularity: bool,
}

impl SegmentDescriptor {
    /// Create from raw descriptor bytes
    pub fn from_raw(desc: u64, selector: u16) -> Self {
        let base = ((desc >> 16) & 0xFFFFFF) | ((desc >> 32) & 0xFF000000);
        let limit = (desc & 0xFFFF) | ((desc >> 32) & 0xF0000);

        Self {
            selector,
            base,
            limit: limit as u32,
            stype: ((desc >> 40) & 0xF) as u8,
            dpl: ((desc >> 45) & 0x3) as u8,
            present: (desc & (1 << 47)) != 0,
            long_mode: (desc & (1 << 53)) != 0,
            db: (desc & (1 << 54)) != 0,
            granularity: (desc & (1 << 55)) != 0,
        }
    }

    /// Convert to raw descriptor
    pub fn to_raw(&self) -> u64 {
        let mut desc: u64 = 0;

        desc |= (self.limit as u64) & 0xFFFF;
        desc |= ((self.limit as u64) & 0xF0000) << 32;
        desc |= (self.base & 0xFFFFFF) << 16;
        desc |= (self.base & 0xFF000000) << 32;
        desc |= (self.stype as u64) << 40;
        desc |= (self.dpl as u64) << 45;
        if self.present { desc |= 1 << 47; }
        if self.long_mode { desc |= 1 << 53; }
        if self.db { desc |= 1 << 54; }
        if self.granularity { desc |= 1 << 55; }

        desc
    }
}

/// All segment registers
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct SegmentRegs {
    /// CS (code segment)
    pub cs: SegmentDescriptor,
    /// DS (data segment)
    pub ds: SegmentDescriptor,
    /// ES (extra segment)
    pub es: SegmentDescriptor,
    /// FS
    pub fs: SegmentDescriptor,
    /// GS
    pub gs: SegmentDescriptor,
    /// SS (stack segment)
    pub ss: SegmentDescriptor,
    /// TR (task register)
    pub tr: SegmentDescriptor,
    /// LDTR (local descriptor table register)
    pub ldtr: SegmentDescriptor,
}

// =============================================================================
// SYSTEM REGISTERS
// =============================================================================

/// Control registers
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct ControlRegs {
    /// CR0 - System control
    pub cr0: u64,
    /// CR2 - Page fault linear address
    pub cr2: u64,
    /// CR3 - Page directory base
    pub cr3: u64,
    /// CR4 - Extensions control
    pub cr4: u64,
    /// CR8 - Task priority (x86_64)
    pub cr8: u64,
}

impl ControlRegs {
    /// Read current control registers
    pub unsafe fn read_current() -> Self {
        let mut regs = Self::default();

        asm!("mov {}, cr0", out(reg) regs.cr0, options(nomem, nostack));
        asm!("mov {}, cr2", out(reg) regs.cr2, options(nomem, nostack));
        asm!("mov {}, cr3", out(reg) regs.cr3, options(nomem, nostack));
        asm!("mov {}, cr4", out(reg) regs.cr4, options(nomem, nostack));
        asm!("mov {}, cr8", out(reg) regs.cr8, options(nomem, nostack));

        regs
    }
}

/// Model-specific registers for debugging
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MsrRegs {
    /// EFER (Extended Feature Enable Register)
    pub efer: u64,
    /// STAR (legacy mode SYSCALL target)
    pub star: u64,
    /// LSTAR (long mode SYSCALL target)
    pub lstar: u64,
    /// CSTAR (compatibility mode SYSCALL target)
    pub cstar: u64,
    /// FMASK (SYSCALL flag mask)
    pub fmask: u64,
    /// FS base
    pub fs_base: u64,
    /// GS base
    pub gs_base: u64,
    /// Kernel GS base
    pub kernel_gs_base: u64,
    /// Debug control
    pub debug_ctl: u64,
    /// Last branch from
    pub lbr_from: u64,
    /// Last branch to
    pub lbr_to: u64,
    /// Last exception from
    pub ler_from: u64,
    /// Last exception to
    pub ler_to: u64,
}

impl MsrRegs {
    /// MSR addresses
    pub const EFER: u32 = 0xC0000080;
    pub const STAR: u32 = 0xC0000081;
    pub const LSTAR: u32 = 0xC0000082;
    pub const CSTAR: u32 = 0xC0000083;
    pub const FMASK: u32 = 0xC0000084;
    pub const FS_BASE: u32 = 0xC0000100;
    pub const GS_BASE: u32 = 0xC0000101;
    pub const KERNEL_GS_BASE: u32 = 0xC0000102;
    pub const DEBUG_CTL: u32 = 0x1D9;

    /// Read MSR
    pub unsafe fn read_msr(msr: u32) -> u64 {
        let low: u32;
        let high: u32;
        asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack)
        );
        ((high as u64) << 32) | (low as u64)
    }

    /// Write MSR
    pub unsafe fn write_msr(msr: u32, value: u64) {
        let low = value as u32;
        let high = (value >> 32) as u32;
        asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") low,
            in("edx") high,
            options(nomem, nostack)
        );
    }

    /// Read current MSR values
    pub unsafe fn read_current() -> Self {
        Self {
            efer: Self::read_msr(Self::EFER),
            star: Self::read_msr(Self::STAR),
            lstar: Self::read_msr(Self::LSTAR),
            cstar: Self::read_msr(Self::CSTAR),
            fmask: Self::read_msr(Self::FMASK),
            fs_base: Self::read_msr(Self::FS_BASE),
            gs_base: Self::read_msr(Self::GS_BASE),
            kernel_gs_base: Self::read_msr(Self::KERNEL_GS_BASE),
            debug_ctl: Self::read_msr(Self::DEBUG_CTL),
            ..Self::default()
        }
    }
}

// =============================================================================
// REGISTER CONTEXT (COMPLETE CPU STATE)
// =============================================================================

/// Complete CPU register context for debugging
#[derive(Clone)]
#[repr(C)]
pub struct RegisterContext {
    /// General purpose registers
    pub gp: GeneralRegs,
    /// Debug registers
    pub debug: DebugRegs,
    /// Control registers
    pub control: ControlRegs,
    /// Floating point state
    pub fp: FpState,
}

impl Default for RegisterContext {
    fn default() -> Self {
        Self {
            gp: GeneralRegs::default(),
            debug: DebugRegs::default(),
            control: ControlRegs::default(),
            fp: FpState::default(),
        }
    }
}

impl RegisterContext {
    /// Create new empty context
    pub fn new() -> Self {
        Self::default()
    }

    /// Read current CPU context
    pub unsafe fn read_current() -> Self {
        let mut ctx = Self::new();
        ctx.debug = DebugRegs::read_current();
        ctx.control = ControlRegs::read_current();
        ctx.fp.save();
        ctx
    }

    /// Write context to CPU
    pub unsafe fn write_current(&self) {
        self.debug.write_current();
        self.fp.restore();
    }
}

// =============================================================================
// REGISTER ACCESS HELPERS
// =============================================================================

/// Read a register by name
pub fn read_register_by_name(ctx: &RegisterContext, name: &str) -> Option<u64> {
    match name.to_lowercase().as_str() {
        "rax" => Some(ctx.gp.rax),
        "rbx" => Some(ctx.gp.rbx),
        "rcx" => Some(ctx.gp.rcx),
        "rdx" => Some(ctx.gp.rdx),
        "rsi" => Some(ctx.gp.rsi),
        "rdi" => Some(ctx.gp.rdi),
        "rbp" => Some(ctx.gp.rbp),
        "rsp" => Some(ctx.gp.rsp),
        "r8" => Some(ctx.gp.r8),
        "r9" => Some(ctx.gp.r9),
        "r10" => Some(ctx.gp.r10),
        "r11" => Some(ctx.gp.r11),
        "r12" => Some(ctx.gp.r12),
        "r13" => Some(ctx.gp.r13),
        "r14" => Some(ctx.gp.r14),
        "r15" => Some(ctx.gp.r15),
        "rip" | "pc" => Some(ctx.gp.rip),
        "rflags" | "eflags" => Some(ctx.gp.rflags),
        "cs" => Some(ctx.gp.cs),
        "ss" => Some(ctx.gp.ss),
        "ds" => Some(ctx.gp.ds),
        "es" => Some(ctx.gp.es),
        "fs" => Some(ctx.gp.fs),
        "gs" => Some(ctx.gp.gs),
        "fsbase" | "fs_base" => Some(ctx.gp.fs_base),
        "gsbase" | "gs_base" => Some(ctx.gp.gs_base),
        "dr0" => Some(ctx.debug.dr0),
        "dr1" => Some(ctx.debug.dr1),
        "dr2" => Some(ctx.debug.dr2),
        "dr3" => Some(ctx.debug.dr3),
        "dr6" => Some(ctx.debug.dr6),
        "dr7" => Some(ctx.debug.dr7),
        "cr0" => Some(ctx.control.cr0),
        "cr2" => Some(ctx.control.cr2),
        "cr3" => Some(ctx.control.cr3),
        "cr4" => Some(ctx.control.cr4),
        "cr8" => Some(ctx.control.cr8),
        _ => None,
    }
}

/// Write a register by name
pub fn write_register_by_name(ctx: &mut RegisterContext, name: &str, value: u64) -> bool {
    match name.to_lowercase().as_str() {
        "rax" => ctx.gp.rax = value,
        "rbx" => ctx.gp.rbx = value,
        "rcx" => ctx.gp.rcx = value,
        "rdx" => ctx.gp.rdx = value,
        "rsi" => ctx.gp.rsi = value,
        "rdi" => ctx.gp.rdi = value,
        "rbp" => ctx.gp.rbp = value,
        "rsp" => ctx.gp.rsp = value,
        "r8" => ctx.gp.r8 = value,
        "r9" => ctx.gp.r9 = value,
        "r10" => ctx.gp.r10 = value,
        "r11" => ctx.gp.r11 = value,
        "r12" => ctx.gp.r12 = value,
        "r13" => ctx.gp.r13 = value,
        "r14" => ctx.gp.r14 = value,
        "r15" => ctx.gp.r15 = value,
        "rip" | "pc" => ctx.gp.rip = value,
        "rflags" | "eflags" => ctx.gp.rflags = value,
        "cs" => ctx.gp.cs = value,
        "ss" => ctx.gp.ss = value,
        "ds" => ctx.gp.ds = value,
        "es" => ctx.gp.es = value,
        "fs" => ctx.gp.fs = value,
        "gs" => ctx.gp.gs = value,
        "fsbase" | "fs_base" => ctx.gp.fs_base = value,
        "gsbase" | "gs_base" => ctx.gp.gs_base = value,
        "dr0" => ctx.debug.dr0 = value,
        "dr1" => ctx.debug.dr1 = value,
        "dr2" => ctx.debug.dr2 = value,
        "dr3" => ctx.debug.dr3 = value,
        "dr6" => ctx.debug.dr6 = value,
        "dr7" => ctx.debug.dr7 = value,
        _ => return false,
    }
    true
}

/// Get all register names
pub fn register_names() -> &'static [&'static str] {
    &[
        "rax", "rbx", "rcx", "rdx", "rsi", "rdi", "rbp", "rsp",
        "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15",
        "rip", "rflags",
        "cs", "ss", "ds", "es", "fs", "gs",
        "fs_base", "gs_base",
        "dr0", "dr1", "dr2", "dr3", "dr6", "dr7",
        "cr0", "cr2", "cr3", "cr4", "cr8",
    ]
}

// =============================================================================
// SIGNAL FRAME
// =============================================================================

/// Signal frame for signal delivery
#[derive(Clone)]
#[repr(C)]
pub struct SignalFrame {
    /// Return address (signal trampoline)
    pub ret_addr: u64,
    /// Signal number
    pub signum: i32,
    /// Padding
    pub _pad: u32,
    /// Signal info pointer
    pub siginfo_ptr: u64,
    /// User context pointer
    pub ucontext_ptr: u64,
    /// Saved registers
    pub regs: GeneralRegs,
    /// Saved FP state
    pub fp: FpState,
}

impl SignalFrame {
    /// Create new signal frame
    pub fn new(signum: i32, regs: &GeneralRegs, fp: &FpState) -> Self {
        Self {
            ret_addr: 0,
            signum,
            _pad: 0,
            siginfo_ptr: 0,
            ucontext_ptr: 0,
            regs: *regs,
            fp: *fp,
        }
    }
}
