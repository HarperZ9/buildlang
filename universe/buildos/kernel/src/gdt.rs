// ===============================================================================
// BUILDOS KERNEL - GLOBAL DESCRIPTOR TABLE
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Global Descriptor Table (GDT) and Task State Segment (TSS) setup.
//!
//! The GDT defines memory segments for x86_64. In 64-bit mode, segmentation
//! is mostly disabled, but we still need:
//! - Null descriptor
//! - Kernel code segment (CS)
//! - Kernel data segment (DS, SS, ES, FS, GS)
//! - User code segment
//! - User data segment
//! - TSS descriptor

use core::arch::asm;
use core::mem::size_of;
use core::ptr::{addr_of, addr_of_mut};

// =============================================================================
// CONSTANTS
// =============================================================================

/// Kernel code segment selector
pub const KERNEL_CS: u16 = 0x08;

/// Kernel data segment selector
pub const KERNEL_DS: u16 = 0x10;

/// User code segment selector (RPL = 3)
pub const USER_CS: u16 = 0x18 | 3;

/// User data segment selector (RPL = 3)
pub const USER_DS: u16 = 0x20 | 3;

/// TSS segment selector
pub const TSS_SELECTOR: u16 = 0x28;

/// Number of interrupt stacks
pub const IST_STACK_COUNT: usize = 7;

/// Stack size for each IST stack (16KB)
pub const IST_STACK_SIZE: usize = 16 * 1024;

// =============================================================================
// GDT ENTRY
// =============================================================================

/// GDT entry (segment descriptor)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct GdtEntry {
    limit_low: u16,
    base_low: u16,
    base_middle: u8,
    access: u8,
    granularity: u8,
    base_high: u8,
}

impl GdtEntry {
    /// Create a null descriptor
    const fn null() -> Self {
        Self {
            limit_low: 0,
            base_low: 0,
            base_middle: 0,
            access: 0,
            granularity: 0,
            base_high: 0,
        }
    }

    /// Create a kernel code segment descriptor
    const fn kernel_code() -> Self {
        Self {
            limit_low: 0xFFFF,
            base_low: 0,
            base_middle: 0,
            access: 0x9A, // Present, Ring 0, Code, Execute/Read
            granularity: 0xAF, // 4KB granularity, 64-bit, limit high
            base_high: 0,
        }
    }

    /// Create a kernel data segment descriptor
    const fn kernel_data() -> Self {
        Self {
            limit_low: 0xFFFF,
            base_low: 0,
            base_middle: 0,
            access: 0x92, // Present, Ring 0, Data, Read/Write
            granularity: 0xCF, // 4KB granularity, 32-bit, limit high
            base_high: 0,
        }
    }

    /// Create a user code segment descriptor
    const fn user_code() -> Self {
        Self {
            limit_low: 0xFFFF,
            base_low: 0,
            base_middle: 0,
            access: 0xFA, // Present, Ring 3, Code, Execute/Read
            granularity: 0xAF, // 4KB granularity, 64-bit, limit high
            base_high: 0,
        }
    }

    /// Create a user data segment descriptor
    const fn user_data() -> Self {
        Self {
            limit_low: 0xFFFF,
            base_low: 0,
            base_middle: 0,
            access: 0xF2, // Present, Ring 3, Data, Read/Write
            granularity: 0xCF, // 4KB granularity, 32-bit, limit high
            base_high: 0,
        }
    }
}

// =============================================================================
// TSS ENTRY
// =============================================================================

/// TSS descriptor (16 bytes in long mode)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct TssEntry {
    limit_low: u16,
    base_low: u16,
    base_middle: u8,
    access: u8,
    limit_flags: u8,
    base_high: u8,
    base_upper: u32,
    reserved: u32,
}

impl TssEntry {
    /// Create a TSS descriptor
    fn new(tss: &Tss) -> Self {
        let base = tss as *const _ as u64;
        let limit = (size_of::<Tss>() - 1) as u16;

        Self {
            limit_low: limit,
            base_low: base as u16,
            base_middle: (base >> 16) as u8,
            access: 0x89, // Present, 64-bit TSS (available)
            limit_flags: 0x00,
            base_high: (base >> 24) as u8,
            base_upper: (base >> 32) as u32,
            reserved: 0,
        }
    }
}

// =============================================================================
// TASK STATE SEGMENT
// =============================================================================

/// Task State Segment
#[repr(C, packed)]
pub struct Tss {
    reserved1: u32,
    /// Privilege stack pointers (RSP0, RSP1, RSP2)
    pub rsp: [u64; 3],
    reserved2: u64,
    /// Interrupt stack table pointers (IST1-IST7)
    pub ist: [u64; 7],
    reserved3: u64,
    reserved4: u16,
    /// I/O map base address
    pub iomap_base: u16,
}

impl Tss {
    /// Create a new TSS
    const fn new() -> Self {
        Self {
            reserved1: 0,
            rsp: [0; 3],
            reserved2: 0,
            ist: [0; 7],
            reserved3: 0,
            reserved4: 0,
            iomap_base: size_of::<Tss>() as u16,
        }
    }

    /// Set the kernel stack (RSP0)
    pub fn set_kernel_stack(&mut self, stack: u64) {
        self.rsp[0] = stack;
    }

    /// Set an interrupt stack (IST1-IST7)
    pub fn set_ist(&mut self, index: usize, stack: u64) {
        if index > 0 && index <= 7 {
            self.ist[index - 1] = stack;
        }
    }
}

// =============================================================================
// GDT STRUCTURE
// =============================================================================

/// The Global Descriptor Table
#[repr(C, packed)]
pub struct Gdt {
    null: GdtEntry,
    kernel_code: GdtEntry,
    kernel_data: GdtEntry,
    user_code: GdtEntry,
    user_data: GdtEntry,
    tss: TssEntry,
}

impl Gdt {
    /// Create a new GDT
    fn new(tss: &Tss) -> Self {
        Self {
            null: GdtEntry::null(),
            kernel_code: GdtEntry::kernel_code(),
            kernel_data: GdtEntry::kernel_data(),
            user_code: GdtEntry::user_code(),
            user_data: GdtEntry::user_data(),
            tss: TssEntry::new(tss),
        }
    }
}

/// GDT pointer for LGDT instruction
#[repr(C, packed)]
struct GdtPointer {
    limit: u16,
    base: u64,
}

// =============================================================================
// GLOBAL STATE
// =============================================================================

/// Global TSS instance
static mut TSS: Tss = Tss::new();

/// Global GDT instance
static mut GDT: Option<Gdt> = None;

/// IST stacks
static mut IST_STACKS: [[u8; IST_STACK_SIZE]; IST_STACK_COUNT] = [[0; IST_STACK_SIZE]; IST_STACK_COUNT];

/// Kernel stack for syscalls
static mut KERNEL_STACK: [u8; 64 * 1024] = [0; 64 * 1024];

// =============================================================================
// INITIALIZATION
// =============================================================================

/// Initialize the GDT and TSS
///
/// # Safety
///
/// Must only be called once during kernel initialization.
pub unsafe fn init() {
    // Set up IST stacks
    for i in 0..IST_STACK_COUNT {
        let stack_top = addr_of!(IST_STACKS[i]) as u64 + IST_STACK_SIZE as u64;
        (*addr_of_mut!(TSS)).set_ist(i + 1, stack_top);
    }

    // Set up kernel stack for ring 0 transitions
    let kernel_stack_top = addr_of!(KERNEL_STACK) as u64 + (64 * 1024) as u64;
    (*addr_of_mut!(TSS)).set_kernel_stack(kernel_stack_top);

    // Create GDT
    GDT = Some(Gdt::new(&*addr_of!(TSS)));

    // Load GDT
    let gdt = (*addr_of!(GDT)).as_ref().unwrap();
    let ptr = GdtPointer {
        limit: (size_of::<Gdt>() - 1) as u16,
        base: gdt as *const _ as u64,
    };

    asm!("lgdt [{}]", in(reg) &ptr, options(nostack));

    // Reload segment registers
    reload_segments();

    // Load TSS
    asm!("ltr {:x}", in(reg) TSS_SELECTOR, options(nostack, nomem));
}

/// Reload segment registers after GDT load
unsafe fn reload_segments() {
    // Reload CS using far return
    asm!(
        "push {sel}",
        "lea {tmp}, [rip + 2f]",
        "push {tmp}",
        "retfq",
        "2:",
        sel = in(reg) KERNEL_CS as u64,
        tmp = lateout(reg) _,
        options(preserves_flags)
    );

    // Reload data segment registers
    asm!(
        "mov ds, {sel:x}",
        "mov es, {sel:x}",
        "mov fs, {sel:x}",
        "mov gs, {sel:x}",
        "mov ss, {sel:x}",
        sel = in(reg) KERNEL_DS,
        options(nostack, preserves_flags)
    );
}

/// Update the kernel stack in the TSS
pub fn set_kernel_stack(stack: u64) {
    unsafe {
        TSS.rsp[0] = stack;
    }
}

/// Get kernel code segment selector
pub const fn kernel_cs() -> u16 {
    KERNEL_CS
}

/// Get kernel data segment selector
pub const fn kernel_ds() -> u16 {
    KERNEL_DS
}

/// Get user code segment selector
pub const fn user_cs() -> u16 {
    USER_CS
}

/// Get user data segment selector
pub const fn user_ds() -> u16 {
    USER_DS
}
