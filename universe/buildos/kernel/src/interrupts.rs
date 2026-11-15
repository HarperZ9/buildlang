// ===============================================================================
// BUILDOS KERNEL - INTERRUPT HANDLING
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Interrupt handling: IDT, exceptions, IRQs, and APIC.

use core::arch::asm;
use spin::Mutex;

// =============================================================================
// CONSTANTS
// =============================================================================

/// Number of IDT entries (256 vectors)
const IDT_ENTRIES: usize = 256;

/// Exception vectors (0-31)
pub const EXCEPTION_DIVIDE_ERROR: u8 = 0;
pub const EXCEPTION_DEBUG: u8 = 1;
pub const EXCEPTION_NMI: u8 = 2;
pub const EXCEPTION_BREAKPOINT: u8 = 3;
pub const EXCEPTION_OVERFLOW: u8 = 4;
pub const EXCEPTION_BOUND_RANGE: u8 = 5;
pub const EXCEPTION_INVALID_OPCODE: u8 = 6;
pub const EXCEPTION_DEVICE_NOT_AVAILABLE: u8 = 7;
pub const EXCEPTION_DOUBLE_FAULT: u8 = 8;
pub const EXCEPTION_COPROCESSOR_SEGMENT: u8 = 9;
pub const EXCEPTION_INVALID_TSS: u8 = 10;
pub const EXCEPTION_SEGMENT_NOT_PRESENT: u8 = 11;
pub const EXCEPTION_STACK_FAULT: u8 = 12;
pub const EXCEPTION_GENERAL_PROTECTION: u8 = 13;
pub const EXCEPTION_PAGE_FAULT: u8 = 14;
pub const EXCEPTION_X87_FPU: u8 = 16;
pub const EXCEPTION_ALIGNMENT_CHECK: u8 = 17;
pub const EXCEPTION_MACHINE_CHECK: u8 = 18;
pub const EXCEPTION_SIMD: u8 = 19;
pub const EXCEPTION_VIRTUALIZATION: u8 = 20;
pub const EXCEPTION_CONTROL_PROTECTION: u8 = 21;
pub const EXCEPTION_HYPERVISOR_INJECTION: u8 = 28;
pub const EXCEPTION_VMM_COMMUNICATION: u8 = 29;
pub const EXCEPTION_SECURITY: u8 = 30;

/// IRQ vectors (remapped to 32-47)
pub const IRQ_TIMER: u8 = 32;
pub const IRQ_KEYBOARD: u8 = 33;
pub const IRQ_CASCADE: u8 = 34;
pub const IRQ_COM2: u8 = 35;
pub const IRQ_COM1: u8 = 36;
pub const IRQ_LPT2: u8 = 37;
pub const IRQ_FLOPPY: u8 = 38;
pub const IRQ_LPT1: u8 = 39;
pub const IRQ_RTC: u8 = 40;
pub const IRQ_FREE1: u8 = 41;
pub const IRQ_FREE2: u8 = 42;
pub const IRQ_FREE3: u8 = 43;
pub const IRQ_MOUSE: u8 = 44;
pub const IRQ_FPU: u8 = 45;
pub const IRQ_PRIMARY_ATA: u8 = 46;
pub const IRQ_SECONDARY_ATA: u8 = 47;

/// Syscall vector
pub const SYSCALL_VECTOR: u8 = 0x80;

/// APIC timer vector
pub const APIC_TIMER_VECTOR: u8 = 0xFE;

/// APIC spurious vector
pub const APIC_SPURIOUS_VECTOR: u8 = 0xFF;

// =============================================================================
// IDT STRUCTURES
// =============================================================================

/// IDT entry (gate descriptor)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct IdtEntry {
    /// Lower 16 bits of handler offset
    offset_low: u16,

    /// Kernel code segment selector
    selector: u16,

    /// Interrupt stack table offset (bits 0-2)
    ist: u8,

    /// Type and attributes
    type_attr: u8,

    /// Middle 16 bits of handler offset
    offset_mid: u16,

    /// Upper 32 bits of handler offset
    offset_high: u32,

    /// Reserved (must be zero)
    reserved: u32,
}

impl IdtEntry {
    /// Create an empty IDT entry
    const fn empty() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            type_attr: 0,
            offset_mid: 0,
            offset_high: 0,
            reserved: 0,
        }
    }

    /// Set the handler for this entry
    fn set_handler(&mut self, handler: u64, selector: u16, ist: u8, gate_type: GateType, dpl: u8) {
        self.offset_low = handler as u16;
        self.offset_mid = (handler >> 16) as u16;
        self.offset_high = (handler >> 32) as u32;
        self.selector = selector;
        self.ist = ist;
        self.type_attr = (1 << 7) | ((dpl & 3) << 5) | (gate_type as u8);
        self.reserved = 0;
    }
}

/// Gate type
#[repr(u8)]
#[derive(Clone, Copy)]
pub enum GateType {
    Interrupt = 0xE,
    Trap = 0xF,
}

/// IDT pointer for LIDT instruction
#[repr(C, packed)]
pub struct IdtPointer {
    limit: u16,
    base: u64,
}

/// The IDT itself
#[repr(C, align(16))]
pub struct Idt {
    entries: [IdtEntry; IDT_ENTRIES],
}

impl Idt {
    /// Create a new empty IDT
    const fn new() -> Self {
        Self {
            entries: [IdtEntry::empty(); IDT_ENTRIES],
        }
    }

    /// Set a handler for a vector
    fn set_handler(&mut self, vector: u8, handler: u64, ist: u8, gate_type: GateType, dpl: u8) {
        self.entries[vector as usize].set_handler(
            handler,
            KERNEL_CS,
            ist,
            gate_type,
            dpl,
        );
    }

    /// Load the IDT
    unsafe fn load(&self) {
        let ptr = IdtPointer {
            limit: (core::mem::size_of::<Self>() - 1) as u16,
            base: self as *const _ as u64,
        };

        asm!("lidt [{}]", in(reg) &ptr, options(nostack));
    }
}

/// Kernel code segment selector
const KERNEL_CS: u16 = 0x08;

// =============================================================================
// INTERRUPT FRAME
// =============================================================================

/// Interrupt stack frame pushed by CPU
#[repr(C)]
#[derive(Debug, Clone)]
pub struct InterruptFrame {
    // Pushed by handler stub
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,

    // Pushed by CPU
    pub vector: u64,
    pub error_code: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

// =============================================================================
// GLOBAL IDT
// =============================================================================

/// Global IDT instance
static IDT: Mutex<Idt> = Mutex::new(Idt::new());

/// Whether interrupts are initialized
static mut INITIALIZED: bool = false;

// =============================================================================
// INITIALIZATION
// =============================================================================

/// Initialize interrupt handling
///
/// # Safety
///
/// Must only be called once during kernel initialization.
pub unsafe fn init() {
    let mut idt = IDT.lock();

    // Set up exception handlers (0-31)
    idt.set_handler(EXCEPTION_DIVIDE_ERROR, exception_divide_error as *const () as u64, 0, GateType::Trap, 0);
    idt.set_handler(EXCEPTION_DEBUG, exception_debug as *const () as u64, 0, GateType::Trap, 0);
    idt.set_handler(EXCEPTION_NMI, exception_nmi as *const () as u64, 0, GateType::Interrupt, 0);
    idt.set_handler(EXCEPTION_BREAKPOINT, exception_breakpoint as *const () as u64, 0, GateType::Trap, 3);
    idt.set_handler(EXCEPTION_OVERFLOW, exception_overflow as *const () as u64, 0, GateType::Trap, 0);
    idt.set_handler(EXCEPTION_BOUND_RANGE, exception_bound_range as *const () as u64, 0, GateType::Trap, 0);
    idt.set_handler(EXCEPTION_INVALID_OPCODE, exception_invalid_opcode as *const () as u64, 0, GateType::Trap, 0);
    idt.set_handler(EXCEPTION_DEVICE_NOT_AVAILABLE, exception_device_not_available as *const () as u64, 0, GateType::Trap, 0);
    idt.set_handler(EXCEPTION_DOUBLE_FAULT, exception_double_fault as *const () as u64, 1, GateType::Trap, 0); // IST 1
    idt.set_handler(EXCEPTION_INVALID_TSS, exception_invalid_tss as *const () as u64, 0, GateType::Trap, 0);
    idt.set_handler(EXCEPTION_SEGMENT_NOT_PRESENT, exception_segment_not_present as *const () as u64, 0, GateType::Trap, 0);
    idt.set_handler(EXCEPTION_STACK_FAULT, exception_stack_fault as *const () as u64, 0, GateType::Trap, 0);
    idt.set_handler(EXCEPTION_GENERAL_PROTECTION, exception_general_protection as *const () as u64, 0, GateType::Trap, 0);
    idt.set_handler(EXCEPTION_PAGE_FAULT, exception_page_fault as *const () as u64, 0, GateType::Trap, 0);
    idt.set_handler(EXCEPTION_X87_FPU, exception_x87_fpu as *const () as u64, 0, GateType::Trap, 0);
    idt.set_handler(EXCEPTION_ALIGNMENT_CHECK, exception_alignment_check as *const () as u64, 0, GateType::Trap, 0);
    idt.set_handler(EXCEPTION_MACHINE_CHECK, exception_machine_check as *const () as u64, 0, GateType::Trap, 0);
    idt.set_handler(EXCEPTION_SIMD, exception_simd as *const () as u64, 0, GateType::Trap, 0);

    // Set up IRQ handlers (32-47)
    idt.set_handler(IRQ_TIMER, irq_timer as *const () as u64, 0, GateType::Interrupt, 0);
    idt.set_handler(IRQ_KEYBOARD, irq_keyboard as *const () as u64, 0, GateType::Interrupt, 0);
    idt.set_handler(IRQ_COM1, irq_com1 as *const () as u64, 0, GateType::Interrupt, 0);
    idt.set_handler(IRQ_COM2, irq_com2 as *const () as u64, 0, GateType::Interrupt, 0);

    // Syscall handler
    idt.set_handler(SYSCALL_VECTOR, syscall_handler as *const () as u64, 0, GateType::Interrupt, 3);

    // Load IDT
    idt.load();

    // Initialize PIC
    init_pic_8259();

    INITIALIZED = true;
}

/// Initialize 8259 PIC (legacy mode)
unsafe fn init_pic_8259() {
    const PIC1_COMMAND: u16 = 0x20;
    const PIC1_DATA: u16 = 0x21;
    const PIC2_COMMAND: u16 = 0xA0;
    const PIC2_DATA: u16 = 0xA1;

    const ICW1_INIT: u8 = 0x10;
    const ICW1_ICW4: u8 = 0x01;
    const ICW4_8086: u8 = 0x01;

    // Save masks
    let _mask1 = inb(PIC1_DATA);
    let _mask2 = inb(PIC2_DATA);

    // Start initialization sequence
    outb(PIC1_COMMAND, ICW1_INIT | ICW1_ICW4);
    io_wait();
    outb(PIC2_COMMAND, ICW1_INIT | ICW1_ICW4);
    io_wait();

    // Set vector offsets
    outb(PIC1_DATA, 32); // IRQ 0-7 -> vectors 32-39
    io_wait();
    outb(PIC2_DATA, 40); // IRQ 8-15 -> vectors 40-47
    io_wait();

    // Tell PICs about each other
    outb(PIC1_DATA, 4); // IRQ2 has slave
    io_wait();
    outb(PIC2_DATA, 2); // Slave identity
    io_wait();

    // Set 8086 mode
    outb(PIC1_DATA, ICW4_8086);
    io_wait();
    outb(PIC2_DATA, ICW4_8086);
    io_wait();

    // Restore masks (mask all except timer and keyboard)
    outb(PIC1_DATA, 0xFC); // Enable IRQ0 (timer) and IRQ1 (keyboard)
    outb(PIC2_DATA, 0xFF); // Mask all on slave
}

/// Send EOI to PIC
pub fn send_eoi(irq: u8) {
    const PIC1_COMMAND: u16 = 0x20;
    const PIC2_COMMAND: u16 = 0xA0;
    const EOI: u8 = 0x20;

    unsafe {
        if irq >= 8 {
            outb(PIC2_COMMAND, EOI);
        }
        outb(PIC1_COMMAND, EOI);
    }
}

// =============================================================================
// I/O PORT HELPERS
// =============================================================================

#[inline]
unsafe fn outb(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nostack, nomem));
}

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    asm!("in al, dx", in("dx") port, out("al") value, options(nostack, nomem));
    value
}

#[inline]
unsafe fn io_wait() {
    outb(0x80, 0);
}

// =============================================================================
// PUBLIC API
// =============================================================================

/// Enable interrupts
#[inline]
pub unsafe fn enable() {
    asm!("sti", options(nostack, nomem));
}

/// Disable interrupts
#[inline]
pub unsafe fn disable() {
    asm!("cli", options(nostack, nomem));
}

/// Check if interrupts are enabled
pub fn are_enabled() -> bool {
    let flags: u64;
    unsafe {
        asm!("pushfq; pop {}", out(reg) flags, options(nomem));
    }
    (flags & (1 << 9)) != 0
}

/// Execute closure with interrupts disabled
pub fn without_interrupts<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    let enabled = are_enabled();

    if enabled {
        unsafe { disable(); }
    }

    let result = f();

    if enabled {
        unsafe { enable(); }
    }

    result
}

// =============================================================================
// EXCEPTION HANDLERS
// =============================================================================

extern "x86-interrupt" fn exception_divide_error(frame: InterruptFrame) {
    crate::kprintln!("EXCEPTION: Divide Error at {:#X}", frame.rip);
    panic!("Divide by zero");
}

extern "x86-interrupt" fn exception_debug(frame: InterruptFrame) {
    crate::kprintln!("EXCEPTION: Debug at {:#X}", frame.rip);
}

extern "x86-interrupt" fn exception_nmi(_frame: InterruptFrame) {
    crate::kprintln!("EXCEPTION: NMI received");
}

extern "x86-interrupt" fn exception_breakpoint(frame: InterruptFrame) {
    crate::kprintln!("BREAKPOINT at {:#X}", frame.rip);
}

extern "x86-interrupt" fn exception_overflow(frame: InterruptFrame) {
    crate::kprintln!("EXCEPTION: Overflow at {:#X}", frame.rip);
    panic!("Overflow");
}

extern "x86-interrupt" fn exception_bound_range(frame: InterruptFrame) {
    crate::kprintln!("EXCEPTION: Bound Range at {:#X}", frame.rip);
    panic!("Bound range exceeded");
}

extern "x86-interrupt" fn exception_invalid_opcode(frame: InterruptFrame) {
    crate::kprintln!("EXCEPTION: Invalid Opcode at {:#X}", frame.rip);
    panic!("Invalid opcode");
}

extern "x86-interrupt" fn exception_device_not_available(frame: InterruptFrame) {
    crate::kprintln!("EXCEPTION: Device Not Available at {:#X}", frame.rip);
}

extern "x86-interrupt" fn exception_double_fault(frame: InterruptFrame, error_code: u64) -> ! {
    crate::kprintln!("EXCEPTION: Double Fault at {:#X}, error={:#X}", frame.rip, error_code);
    panic!("Double fault");
}

extern "x86-interrupt" fn exception_invalid_tss(frame: InterruptFrame, error_code: u64) {
    crate::kprintln!("EXCEPTION: Invalid TSS at {:#X}, error={:#X}", frame.rip, error_code);
    panic!("Invalid TSS");
}

extern "x86-interrupt" fn exception_segment_not_present(frame: InterruptFrame, error_code: u64) {
    crate::kprintln!("EXCEPTION: Segment Not Present at {:#X}, error={:#X}", frame.rip, error_code);
    panic!("Segment not present");
}

extern "x86-interrupt" fn exception_stack_fault(frame: InterruptFrame, error_code: u64) {
    crate::kprintln!("EXCEPTION: Stack Fault at {:#X}, error={:#X}", frame.rip, error_code);
    panic!("Stack fault");
}

extern "x86-interrupt" fn exception_general_protection(frame: InterruptFrame, error_code: u64) {
    crate::kprintln!("EXCEPTION: General Protection Fault at {:#X}, error={:#X}", frame.rip, error_code);

    // Attempt self-healing
    if let Some(_pid) = crate::process::current() {
        crate::healing::heal_error(11); // SIGSEGV
    }

    panic!("General protection fault");
}

extern "x86-interrupt" fn exception_page_fault(frame: InterruptFrame, error_code: u64) {
    let cr2: u64;
    unsafe {
        asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack));
    }

    crate::kprintln!("EXCEPTION: Page Fault at {:#X}", frame.rip);
    crate::kprintln!("  Accessed address: {:#X}", cr2);
    crate::kprintln!("  Error code: {:#X}", error_code);
    crate::kprintln!("    Present: {}", error_code & 1);
    crate::kprintln!("    Write: {}", (error_code >> 1) & 1);
    crate::kprintln!("    User: {}", (error_code >> 2) & 1);

    // Attempt self-healing
    crate::healing::heal_error(11);

    panic!("Page fault");
}

extern "x86-interrupt" fn exception_x87_fpu(frame: InterruptFrame) {
    crate::kprintln!("EXCEPTION: x87 FPU Error at {:#X}", frame.rip);
}

extern "x86-interrupt" fn exception_alignment_check(frame: InterruptFrame, error_code: u64) {
    crate::kprintln!("EXCEPTION: Alignment Check at {:#X}, error={:#X}", frame.rip, error_code);
    panic!("Alignment check");
}

extern "x86-interrupt" fn exception_machine_check(_frame: InterruptFrame) -> ! {
    crate::kprintln!("EXCEPTION: Machine Check");
    panic!("Machine check exception");
}

extern "x86-interrupt" fn exception_simd(frame: InterruptFrame) {
    crate::kprintln!("EXCEPTION: SIMD Error at {:#X}", frame.rip);
    panic!("SIMD exception");
}

// =============================================================================
// IRQ HANDLERS
// =============================================================================

extern "x86-interrupt" fn irq_timer(frame: InterruptFrame) {
    // Timer tick - handle preemptive scheduling
    crate::scheduler::timer_tick_with_frame(&frame);

    send_eoi(0);
}

extern "x86-interrupt" fn irq_keyboard(_frame: InterruptFrame) {
    // Read scancode
    let _scancode = unsafe { inb(0x60) };

    // Handle keyboard input
    crate::drivers::keyboard::handle_interrupt();

    send_eoi(1);
}

extern "x86-interrupt" fn irq_com1(_frame: InterruptFrame) {
    // COM1 interrupt
    send_eoi(4);
}

extern "x86-interrupt" fn irq_com2(_frame: InterruptFrame) {
    // COM2 interrupt
    send_eoi(3);
}

extern "x86-interrupt" fn syscall_handler(frame: InterruptFrame) {
    // System call - registers contain arguments
    // rax = syscall number
    // rdi, rsi, rdx, r10, r8, r9 = arguments

    let _result = crate::syscall::syscall_handler(
        frame.rax,
        frame.rdi,
        frame.rsi,
        frame.rdx,
        frame.r10,
        frame.r8,
        frame.r9,
    );

    // Return value in rax (handled by interrupt return)
}
