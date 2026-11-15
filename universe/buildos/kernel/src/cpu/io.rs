// ===============================================================================
// BUILDOS KERNEL - PORT I/O OPERATIONS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! x86-64 Port I/O operations
//!
//! Provides low-level access to I/O ports for hardware communication.

/// Read a byte from an I/O port
#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    core::arch::asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack));
    value
}

/// Write a byte to an I/O port
#[inline]
pub unsafe fn outb(port: u16, value: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack));
}

/// Read a word (16-bit) from an I/O port
#[inline]
pub unsafe fn inw(port: u16) -> u16 {
    let value: u16;
    core::arch::asm!("in ax, dx", out("ax") value, in("dx") port, options(nomem, nostack));
    value
}

/// Write a word (16-bit) to an I/O port
#[inline]
pub unsafe fn outw(port: u16, value: u16) {
    core::arch::asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack));
}

/// Read a double word (32-bit) from an I/O port
#[inline]
pub unsafe fn inl(port: u16) -> u32 {
    let value: u32;
    core::arch::asm!("in eax, dx", out("eax") value, in("dx") port, options(nomem, nostack));
    value
}

/// Write a double word (32-bit) to an I/O port
#[inline]
pub unsafe fn outl(port: u16, value: u32) {
    core::arch::asm!("out dx, eax", in("dx") port, in("eax") value, options(nomem, nostack));
}

/// I/O wait (for slow devices)
#[inline]
pub unsafe fn io_wait() {
    outb(0x80, 0);
}

/// Read a string of bytes from an I/O port
#[inline]
pub unsafe fn insb(port: u16, buffer: &mut [u8]) {
    core::arch::asm!(
        "rep insb",
        in("dx") port,
        inout("rdi") buffer.as_mut_ptr() => _,
        inout("rcx") buffer.len() => _,
        options(nostack)
    );
}

/// Write a string of bytes to an I/O port
#[inline]
pub unsafe fn outsb(port: u16, buffer: &[u8]) {
    core::arch::asm!(
        "rep outsb",
        in("dx") port,
        inout("rsi") buffer.as_ptr() => _,
        inout("rcx") buffer.len() => _,
        options(nostack)
    );
}

/// Read a string of words from an I/O port
#[inline]
pub unsafe fn insw(port: u16, buffer: &mut [u16]) {
    core::arch::asm!(
        "rep insw",
        in("dx") port,
        inout("rdi") buffer.as_mut_ptr() => _,
        inout("rcx") buffer.len() => _,
        options(nostack)
    );
}

/// Write a string of words to an I/O port
#[inline]
pub unsafe fn outsw(port: u16, buffer: &[u16]) {
    core::arch::asm!(
        "rep outsw",
        in("dx") port,
        inout("rsi") buffer.as_ptr() => _,
        inout("rcx") buffer.len() => _,
        options(nostack)
    );
}

/// Read a string of dwords from an I/O port
#[inline]
pub unsafe fn insl(port: u16, buffer: &mut [u32]) {
    core::arch::asm!(
        "rep insd",
        in("dx") port,
        inout("rdi") buffer.as_mut_ptr() => _,
        inout("rcx") buffer.len() => _,
        options(nostack)
    );
}

/// Write a string of dwords to an I/O port
#[inline]
pub unsafe fn outsl(port: u16, buffer: &[u32]) {
    core::arch::asm!(
        "rep outsd",
        in("dx") port,
        inout("rsi") buffer.as_ptr() => _,
        inout("rcx") buffer.len() => _,
        options(nostack)
    );
}
