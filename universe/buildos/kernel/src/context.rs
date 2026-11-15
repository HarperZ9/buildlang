// ===============================================================================
// BUILDOS KERNEL - CONTEXT SWITCHING
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================
//
// Low-level context switching implementation for x86_64.
// Handles saving and restoring all CPU state during thread switches.
//
// ===============================================================================

use core::arch::{asm, naked_asm};

use crate::process::CpuContext;

// =============================================================================
// EXTENDED CONTEXT (FPU/SSE/AVX)
// =============================================================================

/// Extended CPU state for FPU/SSE/AVX registers
#[repr(C, align(64))]
#[derive(Clone)]
pub struct ExtendedContext {
    /// FXSAVE area (512 bytes for SSE)
    pub fxsave_area: [u8; 512],

    /// Reserved for XSAVE extension
    pub xsave_extension: [u8; 256],
}

impl Default for ExtendedContext {
    fn default() -> Self {
        Self {
            fxsave_area: [0; 512],
            xsave_extension: [0; 256],
        }
    }
}

// =============================================================================
// CONTEXT SWITCH IMPLEMENTATION
// =============================================================================

/// Perform a context switch from the current thread to a new thread.
///
/// This function:
/// 1. Saves all general-purpose registers to the old context
/// 2. Saves the current stack pointer
/// 3. Loads the new stack pointer
/// 4. Restores all general-purpose registers from the new context
/// 5. Returns to the new thread's execution point
///
/// # Safety
///
/// - Both contexts must be valid and properly aligned
/// - This function must be called with interrupts disabled
/// - The new context must have a valid stack pointer and return address
#[unsafe(naked)]
pub unsafe extern "C" fn context_switch(old: *mut CpuContext, new: *const CpuContext) {
    naked_asm!(
        // Save callee-saved registers to old context
        // old context is in rdi, new context is in rsi

        // Save general purpose registers
        "mov [rdi + 0x00], rax",      // rax
        "mov [rdi + 0x08], rbx",      // rbx
        "mov [rdi + 0x10], rcx",      // rcx
        "mov [rdi + 0x18], rdx",      // rdx
        "mov [rdi + 0x20], rsi",      // rsi
        "mov [rdi + 0x28], rdi",      // rdi (save original)
        "mov [rdi + 0x30], rbp",      // rbp
        "mov [rdi + 0x38], rsp",      // rsp
        "mov [rdi + 0x40], r8",       // r8
        "mov [rdi + 0x48], r9",       // r9
        "mov [rdi + 0x50], r10",      // r10
        "mov [rdi + 0x58], r11",      // r11
        "mov [rdi + 0x60], r12",      // r12
        "mov [rdi + 0x68], r13",      // r13
        "mov [rdi + 0x70], r14",      // r14
        "mov [rdi + 0x78], r15",      // r15

        // Save instruction pointer (return address is on stack)
        "mov rax, [rsp]",
        "mov [rdi + 0x80], rax",      // rip

        // Save rflags
        "pushfq",
        "pop rax",
        "mov [rdi + 0x88], rax",      // rflags

        // Save segment registers
        "mov ax, cs",
        "movzx rax, ax",
        "mov [rdi + 0x90], rax",      // cs
        "mov ax, ss",
        "movzx rax, ax",
        "mov [rdi + 0x98], rax",      // ss

        // Now load new context from rsi

        // Load segment registers first (if changing privilege levels)
        // For now, we stay in ring 0, so skip segment register changes

        // Load rflags
        "mov rax, [rsi + 0x88]",
        "push rax",
        "popfq",

        // Load general purpose registers
        "mov r15, [rsi + 0x78]",
        "mov r14, [rsi + 0x70]",
        "mov r13, [rsi + 0x68]",
        "mov r12, [rsi + 0x60]",
        "mov r11, [rsi + 0x58]",
        "mov r10, [rsi + 0x50]",
        "mov r9, [rsi + 0x48]",
        "mov r8, [rsi + 0x40]",
        "mov rbp, [rsi + 0x30]",
        "mov rdx, [rsi + 0x18]",
        "mov rcx, [rsi + 0x10]",
        "mov rbx, [rsi + 0x08]",
        "mov rax, [rsi + 0x00]",

        // Load new stack pointer
        "mov rsp, [rsi + 0x38]",

        // Push return address for ret
        "mov rdi, [rsi + 0x80]",      // rip
        "push rdi",

        // Load rsi and rdi last (we were using them)
        "mov rdi, [rsi + 0x28]",
        "mov rsi, [rsi + 0x20]",

        // Return to new context
        "ret",
    );
}

/// Switch to a new context without saving the old one.
/// Used when starting a new thread for the first time.
///
/// # Safety
///
/// - The new context must be valid and properly initialized
/// - This function must be called with interrupts disabled
#[unsafe(naked)]
pub unsafe extern "C" fn context_switch_first(new: *const CpuContext) {
    naked_asm!(
        // new context is in rdi

        // Load rflags
        "mov rax, [rdi + 0x88]",
        "push rax",
        "popfq",

        // Load general purpose registers
        "mov r15, [rdi + 0x78]",
        "mov r14, [rdi + 0x70]",
        "mov r13, [rdi + 0x68]",
        "mov r12, [rdi + 0x60]",
        "mov r11, [rdi + 0x58]",
        "mov r10, [rdi + 0x50]",
        "mov r9, [rdi + 0x48]",
        "mov r8, [rdi + 0x40]",
        "mov rbp, [rdi + 0x30]",
        "mov rdx, [rdi + 0x18]",
        "mov rcx, [rdi + 0x10]",
        "mov rbx, [rdi + 0x08]",
        "mov rax, [rdi + 0x00]",
        "mov rsi, [rdi + 0x20]",

        // Load new stack pointer
        "mov rsp, [rdi + 0x38]",

        // Get return address
        "mov rdi, [rdi + 0x80]",
        "push rdi",

        // Load rdi last
        "mov rdi, [rdi + 0x28]",

        // Jump to new context
        "ret",
    );
}

/// Switch to user mode using SYSRET instruction.
/// Used to transition from kernel mode to user mode.
///
/// # Safety
///
/// - The context must contain valid user-space addresses
/// - The page tables must be properly configured for user access
#[unsafe(naked)]
pub unsafe extern "C" fn switch_to_user(ctx: *const CpuContext) {
    naked_asm!(
        // ctx is in rdi

        // Load user segment selectors
        // User data segment: 0x23 (ring 3, GDT index 4)
        // User code segment: 0x2B (ring 3, GDT index 5)
        "mov ax, 0x23",
        "mov ds, ax",
        "mov es, ax",
        "mov fs, ax",
        "mov gs, ax",

        // Load general purpose registers
        "mov r15, [rdi + 0x78]",
        "mov r14, [rdi + 0x70]",
        "mov r13, [rdi + 0x68]",
        "mov r12, [rdi + 0x60]",
        "mov r11, [rdi + 0x58]",      // Will be overwritten by SYSRET
        "mov r10, [rdi + 0x50]",
        "mov r9, [rdi + 0x48]",
        "mov r8, [rdi + 0x40]",
        "mov rbp, [rdi + 0x30]",
        "mov rdx, [rdi + 0x18]",
        "mov rbx, [rdi + 0x08]",
        "mov rax, [rdi + 0x00]",
        "mov rsi, [rdi + 0x20]",

        // Set up for SYSRET
        // RCX = user RIP
        // R11 = user RFLAGS
        "mov rcx, [rdi + 0x80]",      // RIP -> RCX for SYSRET
        "mov r11, [rdi + 0x88]",      // RFLAGS -> R11 for SYSRET

        // Load user stack pointer
        "mov rsp, [rdi + 0x38]",

        // Load rdi last
        "mov rdi, [rdi + 0x28]",

        // SYSRETQ returns to user mode
        // Sets RIP = RCX, RFLAGS = R11, CS = 0x33, SS = 0x2B
        "sysretq",
    );
}

/// Switch to user mode using IRETQ instruction.
/// Used when returning from an interrupt in user mode.
///
/// # Safety
///
/// - The context must contain valid user-space addresses
/// - The page tables must be properly configured for user access
#[unsafe(naked)]
pub unsafe extern "C" fn iret_to_user(ctx: *const CpuContext) {
    naked_asm!(
        // ctx is in rdi

        // Load user segment selectors
        "mov ax, 0x23",
        "mov ds, ax",
        "mov es, ax",
        "mov fs, ax",
        "mov gs, ax",

        // Load general purpose registers
        "mov r15, [rdi + 0x78]",
        "mov r14, [rdi + 0x70]",
        "mov r13, [rdi + 0x68]",
        "mov r12, [rdi + 0x60]",
        "mov r11, [rdi + 0x58]",
        "mov r10, [rdi + 0x50]",
        "mov r9, [rdi + 0x48]",
        "mov r8, [rdi + 0x40]",
        "mov rbp, [rdi + 0x30]",
        "mov rdx, [rdi + 0x18]",
        "mov rcx, [rdi + 0x10]",
        "mov rbx, [rdi + 0x08]",
        "mov rax, [rdi + 0x00]",
        "mov rsi, [rdi + 0x20]",

        // Prepare IRET frame on stack
        // SS
        "push 0x23",
        // RSP
        "mov rdi, [rdi + 0x38]",
        "push rdi",
        // RFLAGS (ensure interrupts enabled in user mode)
        "mov rdi, [rdi + 0x88]",
        "or rdi, 0x200",              // Set IF
        "push rdi",
        // CS
        "push 0x2B",
        // RIP
        "mov rdi, [rdi + 0x80]",
        "push rdi",

        // Load rdi
        "mov rdi, [rdi + 0x28]",

        // Return to user mode
        "iretq",
    );
}

// =============================================================================
// FPU/SSE CONTEXT SAVE/RESTORE
// =============================================================================

/// Save FPU/SSE state
///
/// # Safety
///
/// - The area pointer must be valid and 16-byte aligned
#[inline]
pub unsafe fn save_fpu_state(area: *mut [u8; 512]) {
    asm!(
        "fxsave64 [{}]",
        in(reg) area,
        options(nostack)
    );
}

/// Restore FPU/SSE state
///
/// # Safety
///
/// - The area pointer must be valid and 16-byte aligned
/// - The data must have been saved with save_fpu_state
#[inline]
pub unsafe fn restore_fpu_state(area: *const [u8; 512]) {
    asm!(
        "fxrstor64 [{}]",
        in(reg) area,
        options(nostack)
    );
}

/// Initialize FPU state for a new thread
#[inline]
pub unsafe fn init_fpu_state() {
    asm!(
        "fninit",
        options(nostack, nomem)
    );
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Create a new context for a kernel thread.
///
/// Sets up the context to start executing at the given function
/// with the given stack pointer.
pub fn new_kernel_context(entry: fn(), stack_top: u64) -> CpuContext {
    CpuContext {
        rax: 0,
        rbx: 0,
        rcx: 0,
        rdx: 0,
        rsi: 0,
        rdi: 0,
        rbp: 0,
        rsp: stack_top,
        r8: 0,
        r9: 0,
        r10: 0,
        r11: 0,
        r12: 0,
        r13: 0,
        r14: 0,
        r15: 0,
        rip: entry as u64,
        rflags: 0x202, // IF enabled
        cs: 0x08,      // Kernel code segment
        ss: 0x10,      // Kernel data segment
        fs_base: 0,
        gs_base: 0,
    }
}

/// Create a new context for a user-space thread.
///
/// Sets up the context to start executing at the given entry point
/// with the given stack pointer.
pub fn new_user_context(entry: u64, stack_top: u64) -> CpuContext {
    CpuContext {
        rax: 0,
        rbx: 0,
        rcx: 0,
        rdx: 0,
        rsi: 0,
        rdi: 0,
        rbp: stack_top,
        rsp: stack_top,
        r8: 0,
        r9: 0,
        r10: 0,
        r11: 0,
        r12: 0,
        r13: 0,
        r14: 0,
        r15: 0,
        rip: entry,
        rflags: 0x202, // IF enabled
        cs: 0x2B,      // User code segment (ring 3)
        ss: 0x23,      // User data segment (ring 3)
        fs_base: 0,
        gs_base: 0,
    }
}

/// Create a context from an interrupt frame.
/// Used when returning from a syscall or interrupt.
pub fn context_from_interrupt_frame(frame: &crate::interrupts::InterruptFrame) -> CpuContext {
    CpuContext {
        rax: frame.rax,
        rbx: frame.rbx,
        rcx: frame.rcx,
        rdx: frame.rdx,
        rsi: frame.rsi,
        rdi: frame.rdi,
        rbp: frame.rbp,
        rsp: frame.rsp,
        r8: frame.r8,
        r9: frame.r9,
        r10: frame.r10,
        r11: frame.r11,
        r12: frame.r12,
        r13: frame.r13,
        r14: frame.r14,
        r15: frame.r15,
        rip: frame.rip,
        rflags: frame.rflags,
        cs: frame.cs,
        ss: frame.ss,
        fs_base: 0,
        gs_base: 0,
    }
}
