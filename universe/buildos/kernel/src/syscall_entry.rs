// ===============================================================================
// BUILDOS KERNEL - SYSCALL ENTRY/EXIT
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Fast system call entry/exit using SYSCALL/SYSRET instructions.
//!
//! x86_64 syscall convention:
//! - RAX: syscall number
//! - RDI, RSI, RDX, R10, R8, R9: arguments
//! - RAX: return value
//! - RCX: saved RIP (by CPU)
//! - R11: saved RFLAGS (by CPU)

use core::arch::{asm, naked_asm};

use crate::gdt::{KERNEL_CS, USER_CS};

// =============================================================================
// MSR ADDRESSES
// =============================================================================

/// Extended Feature Enable Register
const MSR_EFER: u32 = 0xC0000080;

/// STAR - Segment selectors for syscall/sysret
const MSR_STAR: u32 = 0xC0000081;

/// LSTAR - Long mode syscall entry point
const MSR_LSTAR: u32 = 0xC0000082;

/// CSTAR - Compatibility mode syscall entry point (unused)
const MSR_CSTAR: u32 = 0xC0000083;

/// SFMASK - Flags mask for syscall
const MSR_SFMASK: u32 = 0xC0000084;

// =============================================================================
// EFER BITS
// =============================================================================

/// System call extensions enable
const EFER_SCE: u64 = 1 << 0;

// =============================================================================
// RFLAGS BITS TO CLEAR ON SYSCALL
// =============================================================================

/// Interrupt flag
const RFLAGS_IF: u64 = 1 << 9;

/// Direction flag
const RFLAGS_DF: u64 = 1 << 10;

/// Trap flag
const RFLAGS_TF: u64 = 1 << 8;

/// Alignment check flag
const RFLAGS_AC: u64 = 1 << 18;

/// Mask for syscall - clear IF, DF, TF, AC
const SYSCALL_MASK: u64 = RFLAGS_IF | RFLAGS_DF | RFLAGS_TF | RFLAGS_AC;

// =============================================================================
// INITIALIZATION
// =============================================================================

/// Initialize syscall/sysret mechanism
///
/// # Safety
///
/// Must only be called once during kernel initialization.
pub unsafe fn init() {
    // Enable syscall extensions in EFER
    let efer = rdmsr(MSR_EFER);
    wrmsr(MSR_EFER, efer | EFER_SCE);

    // Set up STAR register
    // Bits 32-47: Kernel CS (and SS = CS + 8)
    // Bits 48-63: User CS (32-bit, and SS = CS + 8)
    // For 64-bit user mode, CS = User CS + 16
    let star = ((KERNEL_CS as u64) << 32) | ((USER_CS as u64 - 16) << 48);
    wrmsr(MSR_STAR, star);

    // Set LSTAR to our syscall entry point
    wrmsr(MSR_LSTAR, syscall_entry as *const () as u64);

    // Set CSTAR (compatibility mode - not used)
    wrmsr(MSR_CSTAR, 0);

    // Set SFMASK - flags to clear on syscall
    wrmsr(MSR_SFMASK, SYSCALL_MASK);
}

// =============================================================================
// MSR HELPERS
// =============================================================================

#[inline]
unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;

    asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") low,
        out("edx") high,
        options(nostack, nomem)
    );

    ((high as u64) << 32) | (low as u64)
}

#[inline]
unsafe fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;

    asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") low,
        in("edx") high,
        options(nostack, nomem)
    );
}

// =============================================================================
// SYSCALL ENTRY POINT
// =============================================================================

/// Syscall entry point (called by CPU when SYSCALL is executed)
///
/// At entry:
/// - RCX = user RIP
/// - R11 = user RFLAGS
/// - RAX = syscall number
/// - RDI, RSI, RDX, R10, R8, R9 = arguments
/// - RSP = user stack (not switched yet!)
#[unsafe(naked)]
pub unsafe extern "C" fn syscall_entry() {
    naked_asm!(
        // Swap to kernel stack
        // GS base points to per-CPU data with kernel stack pointer
        "swapgs",
        "mov gs:[{scratch_offset}], rsp",    // Save user RSP
        "mov rsp, gs:[{kstack_offset}]",     // Load kernel RSP

        // Build trap frame on kernel stack
        "push gs:[{scratch_offset}]",        // User RSP
        "push r11",                          // User RFLAGS
        "push rcx",                          // User RIP

        // Save callee-saved registers
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        // Save syscall arguments
        "push r9",
        "push r8",
        "push r10",                          // R10 instead of RCX (clobbered)
        "push rdx",
        "push rsi",
        "push rdi",
        "push rax",                          // Syscall number

        // Call Rust syscall handler
        // Args already in correct registers: rdi, rsi, rdx, r10->rcx, r8, r9
        "mov rcx, r10",                      // Fix 4th argument
        "mov rdi, rax",                      // Syscall number in first arg
        "mov rsi, [rsp + 8]",               // arg1
        "mov rdx, [rsp + 16]",              // arg2
        "mov rcx, [rsp + 24]",              // arg3
        "mov r8, [rsp + 32]",               // arg4
        "mov r9, [rsp + 40]",               // arg5
        "push [rsp + 48]",                  // arg6 on stack

        "call {handler}",

        // Clean up arg6 from stack
        "add rsp, 8",

        // Return value in RAX

        // Skip saved syscall args
        "add rsp, 56",

        // Restore callee-saved registers
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",

        // Restore user state
        "pop rcx",                           // User RIP
        "pop r11",                           // User RFLAGS
        "pop rsp",                           // User RSP

        // Return to user mode
        "swapgs",
        "sysretq",

        scratch_offset = const 0,            // Offset in per-CPU data
        kstack_offset = const 8,             // Offset in per-CPU data
        handler = sym syscall_handler_inner,
    );
}

/// Inner syscall handler (called from assembly)
#[no_mangle]
extern "C" fn syscall_handler_inner(
    syscall: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> i64 {
    crate::syscall::syscall_handler(syscall, arg1, arg2, arg3, arg4, arg5, arg6)
}

// =============================================================================
// PER-CPU DATA
// =============================================================================

/// Per-CPU data structure
#[repr(C)]
pub struct PerCpuData {
    /// Scratch space for saving user RSP during syscall
    pub scratch: u64,

    /// Kernel stack pointer
    pub kernel_stack: u64,

    /// Current thread pointer
    pub current_thread: u64,

    /// CPU ID
    pub cpu_id: u32,

    /// Padding
    pub _pad: u32,
}

impl PerCpuData {
    pub const fn new() -> Self {
        Self {
            scratch: 0,
            kernel_stack: 0,
            current_thread: 0,
            cpu_id: 0,
            _pad: 0,
        }
    }
}

/// Set up per-CPU data for the current CPU
pub unsafe fn setup_per_cpu(data: &PerCpuData) {
    // Set GS base to per-CPU data
    wrmsr(0xC0000101, data as *const _ as u64); // GS base
    wrmsr(0xC0000102, data as *const _ as u64); // Kernel GS base
}

// =============================================================================
// USER MODE TRANSITION
// =============================================================================

/// Context for returning to user mode
#[repr(C)]
pub struct UserContext {
    pub rip: u64,
    pub rsp: u64,
    pub rflags: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub r8: u64,
    pub r9: u64,
    pub rax: u64,
}

/// Jump to user mode (first time)
///
/// # Safety
///
/// Must have valid user-mode memory set up.
#[unsafe(naked)]
pub unsafe extern "C" fn jump_to_user(ctx: *const UserContext) -> ! {
    naked_asm!(
        // Load user context from pointer in RDI
        "mov rax, [rdi + 72]",               // RAX
        "mov r9,  [rdi + 64]",               // R9
        "mov r8,  [rdi + 56]",               // R8
        "mov rcx, [rdi + 48]",               // RCX
        "mov rdx, [rdi + 40]",               // RDX
        "mov rsi, [rdi + 32]",               // RSI

        // Set up SYSRET frame
        "mov r11, [rdi + 16]",               // RFLAGS -> R11
        "mov rsp, [rdi + 8]",                // User RSP

        // RIP and RDI last
        "push [rdi + 0]",                    // User RIP
        "mov rdi, [rdi + 24]",               // RDI

        // Pop RIP into RCX for SYSRET
        "pop rcx",

        // Enable interrupts in user mode
        "or r11, 0x200",

        // Switch to user mode
        "swapgs",
        "sysretq",
    );
}

/// Return to user mode using IRETQ (for interrupts)
#[unsafe(naked)]
pub unsafe extern "C" fn iret_to_user(
    rip: u64,
    cs: u64,
    rflags: u64,
    rsp: u64,
    ss: u64,
) -> ! {
    naked_asm!(
        // Build IRET frame
        // Already have: rdi=rip, rsi=cs, rdx=rflags, rcx=rsp, r8=ss
        "push r8",                           // SS
        "push rcx",                          // RSP
        "push rdx",                          // RFLAGS
        "push rsi",                          // CS
        "push rdi",                          // RIP

        "swapgs",
        "iretq",
    );
}
