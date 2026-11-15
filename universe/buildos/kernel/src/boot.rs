// ===============================================================================
// BUILDOS KERNEL - BOOT INFORMATION
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Boot information structures passed from bootloader to kernel.

use core::slice;

// =============================================================================
// BOOT INFORMATION STRUCTURES
// =============================================================================

/// Information passed from bootloader to kernel
#[repr(C)]
#[derive(Debug, Clone)]
pub struct BootInfo {
    /// Magic number for validation
    pub magic: u64,

    /// Version of the boot info structure
    pub version: u32,

    /// Size of this structure
    pub size: u32,

    /// Physical address of the kernel entry point
    pub kernel_entry: u64,

    /// Physical address of kernel image
    pub kernel_phys_addr: u64,

    /// Size of loaded kernel image
    pub kernel_size: u64,

    /// Framebuffer information
    pub framebuffer: FramebufferInfo,

    /// Memory map
    pub memory_map: MemoryMapInfo,

    /// ACPI RSDP address
    pub acpi_rsdp: u64,

    /// Kernel command line
    pub cmdline_phys: u64,
    pub cmdline_len: u32,

    /// Reserved for future use
    pub reserved: [u64; 8],
}

impl BootInfo {
    pub const MAGIC: u64 = 0x424F4F54_494E464F; // "BOOTINFO"
    pub const VERSION: u32 = 1;

    /// Get the command line as a string slice
    pub fn cmdline(&self) -> Option<&str> {
        if self.cmdline_phys == 0 || self.cmdline_len == 0 {
            return None;
        }

        unsafe {
            let slice = slice::from_raw_parts(
                self.cmdline_phys as *const u8,
                self.cmdline_len as usize,
            );
            core::str::from_utf8(slice).ok()
        }
    }
}

/// Framebuffer information
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FramebufferInfo {
    /// Physical address of framebuffer
    pub address: u64,

    /// Width in pixels
    pub width: u32,

    /// Height in pixels
    pub height: u32,

    /// Pixels per scan line (may be > width due to padding)
    pub pitch: u32,

    /// Bits per pixel
    pub bpp: u8,

    /// Pixel format
    pub pixel_format: PixelFormatInfo,

    /// Size of framebuffer in bytes
    pub size: u64,
}

impl FramebufferInfo {
    /// Get framebuffer as a mutable slice
    pub unsafe fn as_slice_mut(&self) -> &mut [u32] {
        slice::from_raw_parts_mut(
            self.address as *mut u32,
            (self.size / 4) as usize,
        )
    }

    /// Calculate pixel offset for given coordinates
    #[inline]
    pub fn pixel_offset(&self, x: u32, y: u32) -> usize {
        (y * (self.pitch / 4) + x) as usize
    }
}

/// Pixel format details
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PixelFormatInfo {
    pub red_mask_size: u8,
    pub red_mask_shift: u8,
    pub green_mask_size: u8,
    pub green_mask_shift: u8,
    pub blue_mask_size: u8,
    pub blue_mask_shift: u8,
    pub reserved_mask_size: u8,
    pub reserved_mask_shift: u8,
}

impl PixelFormatInfo {
    /// Create a pixel value from RGB components
    #[inline]
    pub fn rgb(&self, r: u8, g: u8, b: u8) -> u32 {
        ((r as u32) << self.red_mask_shift) |
        ((g as u32) << self.green_mask_shift) |
        ((b as u32) << self.blue_mask_shift)
    }
}

/// Memory map information
#[repr(C)]
#[derive(Debug, Clone)]
pub struct MemoryMapInfo {
    /// Physical address of memory map entries
    pub entries_phys: u64,

    /// Number of entries
    pub entry_count: u32,

    /// Size of each entry
    pub entry_size: u32,

    /// Total usable memory in bytes
    pub total_memory: u64,
}

impl MemoryMapInfo {
    /// Iterate over memory regions
    pub fn iter(&self) -> MemoryRegionIter {
        MemoryRegionIter {
            current: self.entries_phys as *const MemoryRegion,
            remaining: self.entry_count as usize,
        }
    }
}

/// Memory region type
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryRegionType {
    /// Available for general use
    Usable = 0,

    /// Reserved by firmware
    Reserved = 1,

    /// ACPI tables (can be reclaimed after parsing)
    AcpiReclaimable = 2,

    /// ACPI NVS memory
    AcpiNvs = 3,

    /// Memory-mapped I/O
    Mmio = 4,

    /// Kernel code and data
    Kernel = 5,

    /// Bootloader data
    Bootloader = 6,

    /// Framebuffer
    Framebuffer = 7,
}

/// Memory region descriptor
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MemoryRegion {
    /// Physical start address
    pub phys_start: u64,

    /// Number of pages
    pub page_count: u64,

    /// Region type
    pub region_type: MemoryRegionType,
}

impl MemoryRegion {
    /// Get the end address of this region
    #[inline]
    pub fn end(&self) -> u64 {
        self.phys_start + self.page_count * 4096
    }

    /// Get the size in bytes
    #[inline]
    pub fn size(&self) -> u64 {
        self.page_count * 4096
    }

    /// Check if this region contains a physical address
    #[inline]
    pub fn contains(&self, addr: u64) -> bool {
        addr >= self.phys_start && addr < self.end()
    }
}

/// Iterator over memory regions
pub struct MemoryRegionIter {
    current: *const MemoryRegion,
    remaining: usize,
}

impl Iterator for MemoryRegionIter {
    type Item = MemoryRegion;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let region = unsafe { *self.current };
        self.current = unsafe { self.current.add(1) };
        self.remaining -= 1;

        Some(region)
    }
}
