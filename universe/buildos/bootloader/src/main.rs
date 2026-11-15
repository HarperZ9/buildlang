// ===============================================================================
// BUILDOS BOOTLOADER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================
//
// UEFI bootloader for BuildOS. Handles:
// - UEFI entry and initialization
// - Memory map acquisition
// - Kernel loading from disk
// - Framebuffer setup for graphics output
// - Handoff to kernel entry point
//
// ===============================================================================

#![no_std]
#![no_main]
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_assignments)]

extern crate alloc;

#[global_allocator]
static ALLOCATOR: uefi::allocator::Allocator = uefi::allocator::Allocator;

use core::ptr;
use core::slice;

use log::info;
use uefi::prelude::*;
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat};
use uefi::proto::media::file::{File, FileAttribute, FileMode, FileInfo};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::table::boot::{MemoryType, AllocateType};
use uefi::table::cfg::ACPI2_GUID;
use uefi::mem::memory_map::MemoryMap;
use uefi::CStr16;

// =============================================================================
// CONSTANTS
// =============================================================================

/// Kernel file path on the EFI partition
#[allow(dead_code)]
const KERNEL_PATH: &str = "\\EFI\\BUILDOS\\KERNEL.ELF";

/// Magic number expected in kernel header
#[allow(dead_code)]
const KERNEL_MAGIC: u64 = 0x51554E5441_4F5321; // "QUNTAOS!" in hex

/// Page size (4KB)
const PAGE_SIZE: usize = 4096;

/// Physical memory offset for higher-half kernel mapping
#[allow(dead_code)]
const KERNEL_PHYS_OFFSET: u64 = 0xFFFF_8000_0000_0000;

/// Kernel stack size (64KB)
const KERNEL_STACK_SIZE: usize = 64 * 1024;

/// Maximum memory map entries
const MAX_MEMORY_MAP_ENTRIES: usize = 512;

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

    /// Initial ramdisk (initrd)
    pub initrd_phys: u64,
    pub initrd_size: u64,

    /// Secure Boot status
    pub secure_boot: u8,

    /// Reserved for future use
    pub reserved: [u64; 6],
}

impl BootInfo {
    pub const MAGIC: u64 = 0x424F4F54_494E464F; // "BOOTINFO"
    pub const VERSION: u32 = 1;
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

/// Memory region type (simplified from UEFI types)
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

// =============================================================================
// ELF LOADING
// =============================================================================

/// ELF64 header
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Elf64Header {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

/// ELF64 program header
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Elf64ProgramHeader {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

const PT_LOAD: u32 = 1;
const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
const ELFCLASS64: u8 = 2;
const EM_X86_64: u16 = 62;

// =============================================================================
// BOOTLOADER ENTRY POINT
// =============================================================================

#[entry]
fn efi_main(image: Handle, system_table: SystemTable<Boot>) -> Status {
    // Initialize UEFI services
    uefi::helpers::init().unwrap();

    // Get boot services
    let boot_services = system_table.boot_services();

    // Step 1: Load boot configuration
    let boot_config = load_boot_config(boot_services, image);

    // Step 2: Set up graphics framebuffer
    let framebuffer_info = match setup_framebuffer(boot_services) {
        Ok(fb) => fb,
        Err(e) => {
            info!("[ERROR] Failed to setup framebuffer: {:?}", e);
            return Status::DEVICE_ERROR;
        }
    };

    // Step 3: Initialize early console and draw splash
    let mut early_console = EarlyConsole::new();
    early_console.init(framebuffer_info);

    // Draw boot splash
    draw_splash(&mut early_console);

    // Print boot messages
    early_console.set_color(0x00AAFFAA, 0x00000000);
    early_console.write_str("[BOOT] ");
    early_console.set_color(0x00FFFFFF, 0x00000000);
    early_console.write_str("BuildOS Bootloader v2.0.0\n");

    early_console.set_color(0x00AAFFAA, 0x00000000);
    early_console.write_str("[BOOT] ");
    early_console.set_color(0x00FFFFFF, 0x00000000);
    let (fb_w, fb_h) = (framebuffer_info.width, framebuffer_info.height);
    // Note: Can't use format! in no_std easily, so use simpler output
    early_console.write_str("Framebuffer initialized\n");

    // Step 4: Check Secure Boot status
    let secure_boot_status = check_secure_boot(&system_table);
    early_console.set_color(0x00AAFFAA, 0x00000000);
    early_console.write_str("[BOOT] ");
    early_console.set_color(0x00FFFFFF, 0x00000000);
    match secure_boot_status {
        SecureBootStatus::Enabled => early_console.write_str("Secure Boot: Enabled\n"),
        SecureBootStatus::Disabled => early_console.write_str("Secure Boot: Disabled\n"),
        SecureBootStatus::SetupMode => early_console.write_str("Secure Boot: Setup Mode\n"),
        SecureBootStatus::Unknown => early_console.write_str("Secure Boot: Unknown\n"),
    }

    // Step 5: Find ACPI RSDP
    let acpi_rsdp = find_acpi_rsdp(&system_table);
    early_console.set_color(0x00AAFFAA, 0x00000000);
    early_console.write_str("[BOOT] ");
    early_console.set_color(0x00FFFFFF, 0x00000000);
    early_console.write_str("ACPI tables located\n");

    // Step 6: Load kernel from disk
    early_console.set_color(0x00AAFFAA, 0x00000000);
    early_console.write_str("[BOOT] ");
    early_console.set_color(0x00FFFFFF, 0x00000000);
    early_console.write_str("Loading kernel...\n");

    let (kernel_entry, kernel_phys, kernel_size) = match load_kernel(boot_services, image) {
        Ok(info) => {
            early_console.set_color(0x00AAFFAA, 0x00000000);
            early_console.write_str("[BOOT] ");
            early_console.set_color(0x00FFFFFF, 0x00000000);
            early_console.write_str("Kernel loaded successfully\n");
            info
        }
        Err(e) => {
            early_console.set_color(0x00FF5555, 0x00000000);
            early_console.write_str("[ERROR] Failed to load kernel!\n");
            loop { core::hint::spin_loop(); }
        }
    };

    // Step 7: Load initrd if present
    let initrd_info = load_initrd(boot_services, image);
    if initrd_info.is_some() {
        early_console.set_color(0x00AAFFAA, 0x00000000);
        early_console.write_str("[BOOT] ");
        early_console.set_color(0x00FFFFFF, 0x00000000);
        early_console.write_str("Initial ramdisk loaded\n");
    }

    // Step 8: Allocate stack for kernel
    early_console.set_color(0x00AAFFAA, 0x00000000);
    early_console.write_str("[BOOT] ");
    early_console.set_color(0x00FFFFFF, 0x00000000);
    early_console.write_str("Allocating kernel stack...\n");
    let stack_pages = (KERNEL_STACK_SIZE + PAGE_SIZE - 1) / PAGE_SIZE;
    let stack_phys = boot_services
        .allocate_pages(
            AllocateType::AnyPages,
            MemoryType::LOADER_DATA,
            stack_pages,
        )
        .expect("Failed to allocate kernel stack");

    // Step 9: Allocate boot info structure
    let boot_info_pages = (core::mem::size_of::<BootInfo>() + PAGE_SIZE - 1) / PAGE_SIZE;
    let boot_info_phys = boot_services
        .allocate_pages(
            AllocateType::AnyPages,
            MemoryType::LOADER_DATA,
            boot_info_pages + MAX_MEMORY_MAP_ENTRIES,
        )
        .expect("Failed to allocate boot info");

    // Allocate command line buffer
    let cmdline_phys = boot_services
        .allocate_pages(
            AllocateType::AnyPages,
            MemoryType::LOADER_DATA,
            1,
        )
        .expect("Failed to allocate cmdline");

    // Copy command line from config
    if boot_config.cmdline_len > 0 {
        unsafe {
            ptr::copy_nonoverlapping(
                boot_config.cmdline.as_ptr(),
                cmdline_phys as *mut u8,
                boot_config.cmdline_len,
            );
        }
    }

    // Step 10: Exit boot services
    early_console.set_color(0x00AAFFAA, 0x00000000);
    early_console.write_str("[BOOT] ");
    early_console.set_color(0x00FFFFFF, 0x00000000);
    early_console.write_str("Exiting boot services...\n");

    // Exit boot services - this returns the memory map
    let (_runtime_table, memory_map) = unsafe {
        system_table.exit_boot_services(MemoryType::LOADER_DATA)
    };

    // Now we're in control - no more UEFI boot services

    // Build memory map for kernel
    let regions_phys = boot_info_phys + core::mem::size_of::<BootInfo>() as u64;
    let regions_ptr = regions_phys as *mut MemoryRegion;

    let mut region_count = 0u32;
    let mut total_memory = 0u64;

    for (i, desc) in memory_map.entries().enumerate() {
        if i >= MAX_MEMORY_MAP_ENTRIES {
            break;
        }

        let region_type = match desc.ty {
            MemoryType::CONVENTIONAL => MemoryRegionType::Usable,
            MemoryType::LOADER_CODE | MemoryType::LOADER_DATA => MemoryRegionType::Bootloader,
            MemoryType::BOOT_SERVICES_CODE | MemoryType::BOOT_SERVICES_DATA => MemoryRegionType::Usable,
            MemoryType::ACPI_RECLAIM => MemoryRegionType::AcpiReclaimable,
            MemoryType::ACPI_NON_VOLATILE => MemoryRegionType::AcpiNvs,
            MemoryType::MMIO | MemoryType::MMIO_PORT_SPACE => MemoryRegionType::Mmio,
            _ => MemoryRegionType::Reserved,
        };

        let region = MemoryRegion {
            phys_start: desc.phys_start,
            page_count: desc.page_count,
            region_type,
        };

        unsafe {
            ptr::write(regions_ptr.add(i), region);
        }

        if region_type == MemoryRegionType::Usable {
            total_memory += desc.page_count * PAGE_SIZE as u64;
        }

        region_count += 1;
    }

    // Build boot info structure
    let (initrd_phys, initrd_size) = match initrd_info {
        Some(info) => (info.address, info.size),
        None => (0, 0),
    };

    let secure_boot_byte = match secure_boot_status {
        SecureBootStatus::Disabled => 0,
        SecureBootStatus::SetupMode => 1,
        SecureBootStatus::Enabled => 2,
        SecureBootStatus::Unknown => 255,
    };

    let boot_info = BootInfo {
        magic: BootInfo::MAGIC,
        version: BootInfo::VERSION,
        size: core::mem::size_of::<BootInfo>() as u32,
        kernel_entry,
        kernel_phys_addr: kernel_phys,
        kernel_size,
        framebuffer: framebuffer_info,
        memory_map: MemoryMapInfo {
            entries_phys: regions_phys,
            entry_count: region_count,
            entry_size: core::mem::size_of::<MemoryRegion>() as u32,
            total_memory,
        },
        acpi_rsdp,
        cmdline_phys,
        cmdline_len: boot_config.cmdline_len as u32,
        initrd_phys,
        initrd_size,
        secure_boot: secure_boot_byte,
        reserved: [0; 6],
    };

    // Write boot info to allocated memory
    unsafe {
        ptr::write(boot_info_phys as *mut BootInfo, boot_info);
    }

    // Jump to kernel!
    // The kernel entry point signature is:
    // fn kernel_main(boot_info: *const BootInfo) -> !

    // Set up stack and jump
    let stack_top = stack_phys + KERNEL_STACK_SIZE as u64;

    unsafe {
        // Switch stack and call kernel
        core::arch::asm!(
            "mov rsp, {stack}",
            "mov rdi, {boot_info}",
            "jmp {entry}",
            stack = in(reg) stack_top,
            boot_info = in(reg) boot_info_phys,
            entry = in(reg) kernel_entry,
            options(noreturn)
        );
    }
}

// =============================================================================
// GRAPHICS SETUP
// =============================================================================

fn setup_framebuffer(boot_services: &BootServices) -> Result<FramebufferInfo, Status> {
    // Find Graphics Output Protocol
    let gop_handle = boot_services
        .get_handle_for_protocol::<GraphicsOutput>()
        .map_err(|_| Status::NOT_FOUND)?;

    let mut gop = boot_services
        .open_protocol_exclusive::<GraphicsOutput>(gop_handle)
        .map_err(|_| Status::NOT_FOUND)?;

    // Find best mode (prefer 1920x1080 or highest resolution)
    let mut best_mode = None;
    let mut best_score = 0u64;

    for mode in gop.modes(boot_services) {
        let info = mode.info();
        let (width, height) = info.resolution();

        // Score: prefer 1920x1080, then highest resolution, then RGB formats
        let mut score = (width as u64) * (height as u64);

        if width == 1920 && height == 1080 {
            score += 10_000_000; // Strong preference for 1080p
        }

        match info.pixel_format() {
            PixelFormat::Rgb | PixelFormat::Bgr => score += 1000,
            _ => {}
        }

        if score > best_score {
            best_score = score;
            best_mode = Some(mode);
        }
    }

    let mode = best_mode.ok_or(Status::NOT_FOUND)?;
    let mode_info = mode.info();

    // Set the mode
    gop.set_mode(&mode).map_err(|_| Status::DEVICE_ERROR)?;

    // Get framebuffer info
    let (width, height) = mode_info.resolution();
    let stride = mode_info.stride();

    let pixel_format_info = match mode_info.pixel_format() {
        PixelFormat::Rgb => PixelFormatInfo {
            red_mask_size: 8,
            red_mask_shift: 0,
            green_mask_size: 8,
            green_mask_shift: 8,
            blue_mask_size: 8,
            blue_mask_shift: 16,
            reserved_mask_size: 8,
            reserved_mask_shift: 24,
        },
        PixelFormat::Bgr => PixelFormatInfo {
            red_mask_size: 8,
            red_mask_shift: 16,
            green_mask_size: 8,
            green_mask_shift: 8,
            blue_mask_size: 8,
            blue_mask_shift: 0,
            reserved_mask_size: 8,
            reserved_mask_shift: 24,
        },
        PixelFormat::Bitmask => {
            // For bitmask format, we'd need to parse the pixel bitmask
            // For now, assume BGR which is most common
            PixelFormatInfo {
                red_mask_size: 8,
                red_mask_shift: 16,
                green_mask_size: 8,
                green_mask_shift: 8,
                blue_mask_size: 8,
                blue_mask_shift: 0,
                reserved_mask_size: 8,
                reserved_mask_shift: 24,
            }
        }
        PixelFormat::BltOnly => {
            return Err(Status::UNSUPPORTED);
        }
    };

    let fb_base = gop.frame_buffer().as_mut_ptr() as u64;
    let fb_size = gop.frame_buffer().size() as u64;

    Ok(FramebufferInfo {
        address: fb_base,
        width: width as u32,
        height: height as u32,
        pitch: (stride * 4) as u32, // 4 bytes per pixel
        bpp: 32,
        pixel_format: pixel_format_info,
        size: fb_size,
    })
}

// =============================================================================
// ACPI DISCOVERY
// =============================================================================

fn find_acpi_rsdp(system_table: &SystemTable<Boot>) -> u64 {
    // Look for ACPI 2.0 RSDP first
    for config_entry in system_table.config_table() {
        if config_entry.guid == ACPI2_GUID {
            return config_entry.address as u64;
        }
    }

    // Fall back to ACPI 1.0
    for config_entry in system_table.config_table() {
        if config_entry.guid == uefi::table::cfg::ACPI_GUID {
            return config_entry.address as u64;
        }
    }

    0 // Not found
}

// =============================================================================
// KERNEL LOADING
// =============================================================================

fn load_kernel(
    boot_services: &BootServices,
    image: Handle,
) -> Result<(u64, u64, u64), Status> {
    // Get the loaded image protocol to find our boot partition
    let loaded_image = boot_services
        .open_protocol_exclusive::<uefi::proto::loaded_image::LoadedImage>(image)
        .map_err(|_| Status::NOT_FOUND)?;

    let device_handle = loaded_image.device()
        .ok_or(Status::NOT_FOUND)?;

    // Open file system on the same device
    let mut fs = boot_services
        .open_protocol_exclusive::<SimpleFileSystem>(device_handle)
        .map_err(|_| Status::NOT_FOUND)?;

    // Open root directory
    let mut root = fs.open_volume().map_err(|_| Status::NOT_FOUND)?;

    // Build kernel path as wide string
    let kernel_path_wide: [u16; 32] = {
        let mut buf = [0u16; 32];
        let path = "\\EFI\\BUILDOS\\KERNEL.ELF";
        for (i, c) in path.encode_utf16().enumerate() {
            if i < 31 {
                buf[i] = c;
            }
        }
        buf
    };

    let kernel_path = unsafe { CStr16::from_u16_with_nul_unchecked(&kernel_path_wide) };

    // Open kernel file
    let kernel_file = root
        .open(kernel_path, FileMode::Read, FileAttribute::empty())
        .map_err(|_| Status::NOT_FOUND)?;

    let mut kernel_file = kernel_file.into_regular_file()
        .ok_or(Status::NOT_FOUND)?;

    // Get file size
    let mut info_buffer = [0u8; 256];
    let file_info: &FileInfo = kernel_file
        .get_info(&mut info_buffer)
        .map_err(|_| Status::DEVICE_ERROR)?;

    let file_size = file_info.file_size() as usize;

    // Allocate buffer for kernel file
    let file_pages = (file_size + PAGE_SIZE - 1) / PAGE_SIZE;
    let file_buffer = boot_services
        .allocate_pages(
            AllocateType::AnyPages,
            MemoryType::LOADER_DATA,
            file_pages,
        )
        .map_err(|_| Status::OUT_OF_RESOURCES)?;

    // Read kernel into buffer
    let file_slice = unsafe {
        slice::from_raw_parts_mut(file_buffer as *mut u8, file_size)
    };

    let bytes_read = kernel_file.read(file_slice).map_err(|_| Status::DEVICE_ERROR)?;

    if bytes_read != file_size {
        return Err(Status::DEVICE_ERROR);
    }

    // Parse ELF header
    let elf_header = unsafe { &*(file_buffer as *const Elf64Header) };

    // Validate ELF magic
    if elf_header.e_ident[0..4] != ELF_MAGIC {
        return Err(Status::INVALID_PARAMETER);
    }

    // Validate ELF class (64-bit)
    if elf_header.e_ident[4] != ELFCLASS64 {
        return Err(Status::UNSUPPORTED);
    }

    // Validate architecture (x86_64)
    if elf_header.e_machine != EM_X86_64 {
        return Err(Status::UNSUPPORTED);
    }

    // Calculate total memory needed for loadable segments
    let mut min_addr = u64::MAX;
    let mut max_addr = 0u64;

    for i in 0..elf_header.e_phnum as usize {
        let ph_offset = elf_header.e_phoff as usize + i * elf_header.e_phentsize as usize;
        let ph = unsafe { &*((file_buffer as usize + ph_offset) as *const Elf64ProgramHeader) };

        if ph.p_type == PT_LOAD {
            let seg_end = ph.p_paddr + ph.p_memsz;
            if ph.p_paddr < min_addr {
                min_addr = ph.p_paddr;
            }
            if seg_end > max_addr {
                max_addr = seg_end;
            }
        }
    }

    // Allocate memory for kernel image
    let kernel_size = max_addr - min_addr;
    let kernel_pages = ((kernel_size as usize) + PAGE_SIZE - 1) / PAGE_SIZE;

    let kernel_phys = boot_services
        .allocate_pages(
            AllocateType::AnyPages,
            MemoryType::LOADER_DATA,
            kernel_pages,
        )
        .map_err(|_| Status::OUT_OF_RESOURCES)?;

    // Clear kernel memory
    unsafe {
        ptr::write_bytes(kernel_phys as *mut u8, 0, kernel_pages * PAGE_SIZE);
    }

    // Load segments
    for i in 0..elf_header.e_phnum as usize {
        let ph_offset = elf_header.e_phoff as usize + i * elf_header.e_phentsize as usize;
        let ph = unsafe { &*((file_buffer as usize + ph_offset) as *const Elf64ProgramHeader) };

        if ph.p_type == PT_LOAD && ph.p_filesz > 0 {
            let dest = (kernel_phys + (ph.p_paddr - min_addr)) as *mut u8;
            let src = (file_buffer as usize + ph.p_offset as usize) as *const u8;

            unsafe {
                ptr::copy_nonoverlapping(src, dest, ph.p_filesz as usize);
            }
        }
    }

    // Free file buffer
    unsafe {
        boot_services.free_pages(file_buffer, file_pages).ok();
    }

    // Calculate actual entry point
    let entry_point = kernel_phys + (elf_header.e_entry - min_addr);

    Ok((entry_point, kernel_phys, kernel_size))
}

// =============================================================================
// INITRD/RAMDISK LOADING
// =============================================================================

/// Initrd information
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InitrdInfo {
    /// Physical address of initrd
    pub address: u64,
    /// Size in bytes
    pub size: u64,
}

/// Load initrd from disk if present
fn load_initrd(
    boot_services: &BootServices,
    image: Handle,
) -> Option<InitrdInfo> {
    // Get the loaded image protocol
    let loaded_image = boot_services
        .open_protocol_exclusive::<uefi::proto::loaded_image::LoadedImage>(image)
        .ok()?;

    let device_handle = loaded_image.device()?;

    // Open file system
    let mut fs = boot_services
        .open_protocol_exclusive::<SimpleFileSystem>(device_handle)
        .ok()?;

    let mut root = fs.open_volume().ok()?;

    // Build initrd path
    let initrd_path_wide: [u16; 32] = {
        let mut buf = [0u16; 32];
        let path = "\\EFI\\BUILDOS\\INITRD.IMG";
        for (i, c) in path.encode_utf16().enumerate() {
            if i < 31 {
                buf[i] = c;
            }
        }
        buf
    };

    let initrd_path = unsafe { CStr16::from_u16_with_nul_unchecked(&initrd_path_wide) };

    // Try to open initrd file
    let initrd_file = root
        .open(initrd_path, FileMode::Read, FileAttribute::empty())
        .ok()?;

    let mut initrd_file = initrd_file.into_regular_file()?;

    // Get file size
    let mut info_buffer = [0u8; 256];
    let file_info: &FileInfo = initrd_file.get_info(&mut info_buffer).ok()?;
    let file_size = file_info.file_size() as usize;

    if file_size == 0 {
        return None;
    }

    // Allocate memory for initrd
    let initrd_pages = (file_size + PAGE_SIZE - 1) / PAGE_SIZE;
    let initrd_phys = boot_services
        .allocate_pages(
            AllocateType::AnyPages,
            MemoryType::LOADER_DATA,
            initrd_pages,
        )
        .ok()?;

    // Read initrd into memory
    let initrd_slice = unsafe {
        slice::from_raw_parts_mut(initrd_phys as *mut u8, file_size)
    };

    let bytes_read = initrd_file.read(initrd_slice).ok()?;

    if bytes_read != file_size {
        // Free and return None
        unsafe {
            boot_services.free_pages(initrd_phys, initrd_pages).ok();
        }
        return None;
    }

    Some(InitrdInfo {
        address: initrd_phys,
        size: file_size as u64,
    })
}

// =============================================================================
// BOOT CONFIGURATION
// =============================================================================

/// Boot configuration options
#[repr(C)]
#[derive(Debug, Clone)]
pub struct BootConfig {
    /// Kernel command line
    pub cmdline: [u8; 256],
    pub cmdline_len: usize,

    /// Preferred video mode
    pub video_width: u32,
    pub video_height: u32,

    /// Enable verbose boot
    pub verbose: bool,

    /// Enable debug mode
    pub debug: bool,

    /// Boot timeout in seconds (0 = no timeout)
    pub timeout: u8,

    /// Enable serial console
    pub serial_console: bool,

    /// Serial port (0 = COM1, 1 = COM2, etc.)
    pub serial_port: u8,

    /// Serial baud rate
    pub serial_baud: u32,
}

impl Default for BootConfig {
    fn default() -> Self {
        Self {
            cmdline: [0u8; 256],
            cmdline_len: 0,
            video_width: 1920,
            video_height: 1080,
            verbose: false,
            debug: false,
            timeout: 3,
            serial_console: false,
            serial_port: 0,
            serial_baud: 115200,
        }
    }
}

/// Parse boot configuration from file
fn load_boot_config(
    boot_services: &BootServices,
    image: Handle,
) -> BootConfig {
    let mut config = BootConfig::default();

    // Try to load config file
    let config_data = match load_config_file(boot_services, image) {
        Some(data) => data,
        None => return config,
    };

    // Parse simple key=value format
    let config_str = core::str::from_utf8(&config_data).unwrap_or("");

    for line in config_str.lines() {
        let line = line.trim();

        // Skip comments and empty lines
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();

            match key {
                "cmdline" => {
                    let bytes = value.as_bytes();
                    let len = bytes.len().min(255);
                    config.cmdline[..len].copy_from_slice(&bytes[..len]);
                    config.cmdline_len = len;
                }
                "video_width" => {
                    config.video_width = value.parse().unwrap_or(1920);
                }
                "video_height" => {
                    config.video_height = value.parse().unwrap_or(1080);
                }
                "verbose" => {
                    config.verbose = value == "true" || value == "1";
                }
                "debug" => {
                    config.debug = value == "true" || value == "1";
                }
                "timeout" => {
                    config.timeout = value.parse().unwrap_or(3);
                }
                "serial" => {
                    config.serial_console = value == "true" || value == "1";
                }
                "serial_port" => {
                    config.serial_port = value.parse().unwrap_or(0);
                }
                "serial_baud" => {
                    config.serial_baud = value.parse().unwrap_or(115200);
                }
                _ => {}
            }
        }
    }

    config
}

fn load_config_file(boot_services: &BootServices, image: Handle) -> Option<alloc::vec::Vec<u8>> {
    let loaded_image = boot_services
        .open_protocol_exclusive::<uefi::proto::loaded_image::LoadedImage>(image)
        .ok()?;

    let device_handle = loaded_image.device()?;

    let mut fs = boot_services
        .open_protocol_exclusive::<SimpleFileSystem>(device_handle)
        .ok()?;

    let mut root = fs.open_volume().ok()?;

    let config_path_wide: [u16; 32] = {
        let mut buf = [0u16; 32];
        let path = "\\EFI\\BUILDOS\\BOOT.CFG";
        for (i, c) in path.encode_utf16().enumerate() {
            if i < 31 {
                buf[i] = c;
            }
        }
        buf
    };

    let config_path = unsafe { CStr16::from_u16_with_nul_unchecked(&config_path_wide) };

    let config_file = root
        .open(config_path, FileMode::Read, FileAttribute::empty())
        .ok()?;

    let mut config_file = config_file.into_regular_file()?;

    let mut info_buffer = [0u8; 256];
    let file_info: &FileInfo = config_file.get_info(&mut info_buffer).ok()?;
    let file_size = file_info.file_size() as usize;

    if file_size == 0 || file_size > 4096 {
        return None;
    }

    let mut data = alloc::vec![0u8; file_size];
    let bytes_read = config_file.read(&mut data).ok()?;

    if bytes_read != file_size {
        return None;
    }

    Some(data)
}

// =============================================================================
// EARLY BOOT CONSOLE
// =============================================================================

/// Early boot console for debugging
pub struct EarlyConsole {
    framebuffer: Option<FramebufferInfo>,
    cursor_x: u32,
    cursor_y: u32,
    fg_color: u32,
    bg_color: u32,
}

impl EarlyConsole {
    const CHAR_WIDTH: u32 = 8;
    const CHAR_HEIGHT: u32 = 16;

    pub fn new() -> Self {
        Self {
            framebuffer: None,
            cursor_x: 0,
            cursor_y: 0,
            fg_color: 0x00FFFFFF, // White
            bg_color: 0x00000000, // Black
        }
    }

    pub fn init(&mut self, fb: FramebufferInfo) {
        self.framebuffer = Some(fb);
        self.clear();
    }

    pub fn clear(&mut self) {
        if let Some(fb) = &self.framebuffer {
            let pixels = (fb.width * fb.height) as usize;
            let fb_ptr = fb.address as *mut u32;
            for i in 0..pixels {
                unsafe {
                    ptr::write_volatile(fb_ptr.add(i), self.bg_color);
                }
            }
        }
        self.cursor_x = 0;
        self.cursor_y = 0;
    }

    pub fn write_char(&mut self, c: char) {
        let Some(fb) = &self.framebuffer else { return };
        let fb_width = fb.width;
        let fb_height = fb.height;

        match c {
            '\n' => {
                self.cursor_x = 0;
                self.cursor_y += Self::CHAR_HEIGHT;
            }
            '\r' => {
                self.cursor_x = 0;
            }
            '\t' => {
                self.cursor_x = (self.cursor_x + 32) & !31;
            }
            _ => {
                // Simple bitmap font rendering (8x16)
                self.draw_char(c);
                self.cursor_x += Self::CHAR_WIDTH;
            }
        }

        // Handle line wrap
        if self.cursor_x >= fb_width {
            self.cursor_x = 0;
            self.cursor_y += Self::CHAR_HEIGHT;
        }

        // Handle scroll
        if self.cursor_y + Self::CHAR_HEIGHT > fb_height {
            self.scroll();
        }
    }

    fn draw_char(&mut self, c: char) {
        let Some(fb) = &self.framebuffer else { return };

        // Simple 8x16 font - just draw a box for now
        // In production, use a proper bitmap font
        let glyph = get_font_glyph(c);
        let fb_ptr = fb.address as *mut u32;
        let pitch = fb.pitch / 4; // Convert to pixels

        for row in 0..16 {
            let y = self.cursor_y + row;
            if y >= fb.height {
                break;
            }

            let glyph_row = glyph[row as usize];

            for col in 0..8 {
                let x = self.cursor_x + col;
                if x >= fb.width {
                    break;
                }

                let pixel_set = (glyph_row >> (7 - col)) & 1 != 0;
                let color = if pixel_set { self.fg_color } else { self.bg_color };

                let offset = (y * pitch + x) as usize;
                unsafe {
                    ptr::write_volatile(fb_ptr.add(offset), color);
                }
            }
        }
    }

    fn scroll(&mut self) {
        let Some(fb) = &self.framebuffer else { return };

        let fb_ptr = fb.address as *mut u32;
        let pitch = fb.pitch / 4;
        let rows_to_copy = fb.height - Self::CHAR_HEIGHT;

        // Copy rows up
        for y in 0..rows_to_copy {
            for x in 0..fb.width {
                let src_offset = ((y + Self::CHAR_HEIGHT) * pitch + x) as usize;
                let dst_offset = (y * pitch + x) as usize;
                unsafe {
                    let pixel = ptr::read_volatile(fb_ptr.add(src_offset));
                    ptr::write_volatile(fb_ptr.add(dst_offset), pixel);
                }
            }
        }

        // Clear bottom row
        for y in rows_to_copy..fb.height {
            for x in 0..fb.width {
                let offset = (y * pitch + x) as usize;
                unsafe {
                    ptr::write_volatile(fb_ptr.add(offset), self.bg_color);
                }
            }
        }

        self.cursor_y = rows_to_copy;
    }

    pub fn write_str(&mut self, s: &str) {
        for c in s.chars() {
            self.write_char(c);
        }
    }

    pub fn set_color(&mut self, fg: u32, bg: u32) {
        self.fg_color = fg;
        self.bg_color = bg;
    }
}

/// Complete 8x16 bitmap font for ASCII printable characters (32-126)
fn get_font_glyph(c: char) -> [u8; 16] {
    match c {
        // Space and punctuation
        ' ' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '!' => [0x00, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '"' => [0x00, 0x66, 0x66, 0x24, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '#' => [0x00, 0x36, 0x36, 0x7F, 0x36, 0x7F, 0x36, 0x36, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '$' => [0x00, 0x18, 0x3E, 0x60, 0x3C, 0x06, 0x7C, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '%' => [0x00, 0x62, 0x66, 0x0C, 0x18, 0x30, 0x66, 0x46, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '&' => [0x00, 0x3C, 0x66, 0x3C, 0x38, 0x67, 0x66, 0x3F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '\'' => [0x00, 0x18, 0x18, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '(' => [0x00, 0x0C, 0x18, 0x30, 0x30, 0x30, 0x18, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        ')' => [0x00, 0x30, 0x18, 0x0C, 0x0C, 0x0C, 0x18, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '*' => [0x00, 0x00, 0x66, 0x3C, 0xFF, 0x3C, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '+' => [0x00, 0x00, 0x18, 0x18, 0x7E, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        ',' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '-' => [0x00, 0x00, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '/' => [0x00, 0x02, 0x06, 0x0C, 0x18, 0x30, 0x60, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        // Numbers
        '0' => [0x00, 0x3C, 0x66, 0x6E, 0x76, 0x66, 0x66, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '1' => [0x00, 0x18, 0x38, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '2' => [0x00, 0x3C, 0x66, 0x06, 0x0C, 0x18, 0x30, 0x7E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '3' => [0x00, 0x3C, 0x66, 0x06, 0x1C, 0x06, 0x66, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '4' => [0x00, 0x0C, 0x1C, 0x3C, 0x6C, 0x7E, 0x0C, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '5' => [0x00, 0x7E, 0x60, 0x7C, 0x06, 0x06, 0x66, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '6' => [0x00, 0x1C, 0x30, 0x60, 0x7C, 0x66, 0x66, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '7' => [0x00, 0x7E, 0x06, 0x0C, 0x18, 0x30, 0x30, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '8' => [0x00, 0x3C, 0x66, 0x66, 0x3C, 0x66, 0x66, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '9' => [0x00, 0x3C, 0x66, 0x66, 0x3E, 0x06, 0x0C, 0x38, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        ':' => [0x00, 0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        ';' => [0x00, 0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '<' => [0x00, 0x06, 0x0C, 0x18, 0x30, 0x18, 0x0C, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '=' => [0x00, 0x00, 0x00, 0x7E, 0x00, 0x7E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '>' => [0x00, 0x60, 0x30, 0x18, 0x0C, 0x18, 0x30, 0x60, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '?' => [0x00, 0x3C, 0x66, 0x06, 0x0C, 0x18, 0x00, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '@' => [0x00, 0x3C, 0x66, 0x6E, 0x6A, 0x6E, 0x60, 0x3E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        // Uppercase letters
        'A' => [0x00, 0x18, 0x3C, 0x66, 0x66, 0x7E, 0x66, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'B' => [0x00, 0x7C, 0x66, 0x66, 0x7C, 0x66, 0x66, 0x7C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'C' => [0x00, 0x3C, 0x66, 0x60, 0x60, 0x60, 0x66, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'D' => [0x00, 0x78, 0x6C, 0x66, 0x66, 0x66, 0x6C, 0x78, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'E' => [0x00, 0x7E, 0x60, 0x60, 0x7C, 0x60, 0x60, 0x7E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'F' => [0x00, 0x7E, 0x60, 0x60, 0x7C, 0x60, 0x60, 0x60, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'G' => [0x00, 0x3C, 0x66, 0x60, 0x6E, 0x66, 0x66, 0x3E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'H' => [0x00, 0x66, 0x66, 0x66, 0x7E, 0x66, 0x66, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'I' => [0x00, 0x7E, 0x18, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'J' => [0x00, 0x1E, 0x06, 0x06, 0x06, 0x06, 0x66, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'K' => [0x00, 0x66, 0x6C, 0x78, 0x70, 0x78, 0x6C, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'L' => [0x00, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x7E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'M' => [0x00, 0x63, 0x77, 0x7F, 0x6B, 0x63, 0x63, 0x63, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'N' => [0x00, 0x66, 0x76, 0x7E, 0x7E, 0x6E, 0x66, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'O' => [0x00, 0x3C, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'P' => [0x00, 0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60, 0x60, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'Q' => [0x00, 0x3C, 0x66, 0x66, 0x66, 0x6A, 0x6C, 0x36, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'R' => [0x00, 0x7C, 0x66, 0x66, 0x7C, 0x6C, 0x66, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'S' => [0x00, 0x3C, 0x66, 0x60, 0x3C, 0x06, 0x66, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'T' => [0x00, 0x7E, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'U' => [0x00, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'V' => [0x00, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'W' => [0x00, 0x63, 0x63, 0x63, 0x6B, 0x7F, 0x77, 0x63, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'X' => [0x00, 0x66, 0x66, 0x3C, 0x18, 0x3C, 0x66, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'Y' => [0x00, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'Z' => [0x00, 0x7E, 0x06, 0x0C, 0x18, 0x30, 0x60, 0x7E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '[' => [0x00, 0x3C, 0x30, 0x30, 0x30, 0x30, 0x30, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '\\' => [0x00, 0x40, 0x60, 0x30, 0x18, 0x0C, 0x06, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        ']' => [0x00, 0x3C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '^' => [0x00, 0x18, 0x3C, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '_' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '`' => [0x00, 0x30, 0x18, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        // Lowercase letters
        'a' => [0x00, 0x00, 0x00, 0x3C, 0x06, 0x3E, 0x66, 0x3E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'b' => [0x00, 0x60, 0x60, 0x7C, 0x66, 0x66, 0x66, 0x7C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'c' => [0x00, 0x00, 0x00, 0x3C, 0x66, 0x60, 0x66, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'd' => [0x00, 0x06, 0x06, 0x3E, 0x66, 0x66, 0x66, 0x3E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'e' => [0x00, 0x00, 0x00, 0x3C, 0x66, 0x7E, 0x60, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'f' => [0x00, 0x1C, 0x30, 0x30, 0x7C, 0x30, 0x30, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'g' => [0x00, 0x00, 0x00, 0x3E, 0x66, 0x66, 0x3E, 0x06, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'h' => [0x00, 0x60, 0x60, 0x7C, 0x66, 0x66, 0x66, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'i' => [0x00, 0x18, 0x00, 0x38, 0x18, 0x18, 0x18, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'j' => [0x00, 0x0C, 0x00, 0x1C, 0x0C, 0x0C, 0x0C, 0x0C, 0x78, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'k' => [0x00, 0x60, 0x60, 0x66, 0x6C, 0x78, 0x6C, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'l' => [0x00, 0x38, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'm' => [0x00, 0x00, 0x00, 0x76, 0x7F, 0x6B, 0x6B, 0x63, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'n' => [0x00, 0x00, 0x00, 0x7C, 0x66, 0x66, 0x66, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'o' => [0x00, 0x00, 0x00, 0x3C, 0x66, 0x66, 0x66, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'p' => [0x00, 0x00, 0x00, 0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'q' => [0x00, 0x00, 0x00, 0x3E, 0x66, 0x66, 0x3E, 0x06, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'r' => [0x00, 0x00, 0x00, 0x6C, 0x76, 0x60, 0x60, 0x60, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        's' => [0x00, 0x00, 0x00, 0x3E, 0x60, 0x3C, 0x06, 0x7C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        't' => [0x00, 0x30, 0x30, 0x7C, 0x30, 0x30, 0x30, 0x1C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'u' => [0x00, 0x00, 0x00, 0x66, 0x66, 0x66, 0x66, 0x3E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'v' => [0x00, 0x00, 0x00, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'w' => [0x00, 0x00, 0x00, 0x63, 0x6B, 0x6B, 0x7F, 0x36, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'x' => [0x00, 0x00, 0x00, 0x66, 0x3C, 0x18, 0x3C, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'y' => [0x00, 0x00, 0x00, 0x66, 0x66, 0x66, 0x3E, 0x06, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        'z' => [0x00, 0x00, 0x00, 0x7E, 0x0C, 0x18, 0x30, 0x7E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '{' => [0x00, 0x0E, 0x18, 0x18, 0x70, 0x18, 0x18, 0x0E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '|' => [0x00, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '}' => [0x00, 0x70, 0x18, 0x18, 0x0E, 0x18, 0x18, 0x70, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '~' => [0x00, 0x32, 0x4C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        // Default: filled box for unknown characters
        _ => [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    }
}

// =============================================================================
// SECURE BOOT VERIFICATION
// =============================================================================

/// Secure Boot status
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SecureBootStatus {
    /// Secure Boot is disabled
    Disabled,
    /// Secure Boot is enabled in setup mode
    SetupMode,
    /// Secure Boot is enabled and active
    Enabled,
    /// Could not determine status
    Unknown,
}

/// Check Secure Boot status
fn check_secure_boot(system_table: &SystemTable<Boot>) -> SecureBootStatus {
    // Check EFI variables for Secure Boot status
    // In a real implementation, we'd read the SecureBoot variable

    // For now, return Unknown
    SecureBootStatus::Unknown
}

/// Verify kernel signature (placeholder for Secure Boot)
fn verify_kernel_signature(
    _boot_services: &BootServices,
    kernel_data: &[u8],
) -> bool {
    // In production:
    // 1. Parse the PE/COFF signature from the kernel
    // 2. Verify against the Secure Boot database (db)
    // 3. Check against the forbidden signatures (dbx)
    // 4. Return true only if verification succeeds

    // For now, always return true (verification disabled)
    true
}

// =============================================================================
// BOOT SPLASH
// =============================================================================

/// Draw boot splash screen
fn draw_splash(console: &mut EarlyConsole) {
    console.set_color(0x00AAAAFF, 0x00000000);

    console.write_str("\n\n");
    console.write_str("  ██████╗ ██╗   ██╗ █████╗ ███╗   ██╗████████╗ █████╗  ██████╗ ███████╗\n");
    console.write_str(" ██╔═══██╗██║   ██║██╔══██╗████╗  ██║╚══██╔══╝██╔══██╗██╔═══██╗██╔════╝\n");
    console.write_str(" ██║   ██║██║   ██║███████║██╔██╗ ██║   ██║   ███████║██║   ██║███████╗\n");
    console.write_str(" ██║▄▄ ██║██║   ██║██╔══██║██║╚██╗██║   ██║   ██╔══██║██║   ██║╚════██║\n");
    console.write_str(" ╚██████╔╝╚██████╔╝██║  ██║██║ ╚████║   ██║   ██║  ██║╚██████╔╝███████║\n");
    console.write_str("  ╚══▀▀═╝  ╚═════╝ ╚═╝  ╚═╝╚═╝  ╚═══╝   ╚═╝   ╚═╝  ╚═╝ ╚═════╝ ╚══════╝\n");
    console.write_str("\n");

    console.set_color(0x00FFFFFF, 0x00000000);
    console.write_str("                    The Operating System That Evolves\n");
    console.write_str("\n");
    console.write_str("         Copyright 2024-2025 Zain Dana Harper. All Rights Reserved.\n");
    console.write_str("\n\n");
}

// =============================================================================
// SMBIOS/DMI DETECTION
// =============================================================================

/// System information from SMBIOS
#[derive(Debug, Clone)]
pub struct SystemInfo {
    pub vendor: [u8; 64],
    pub product: [u8; 64],
    pub version: [u8; 64],
    pub serial: [u8; 64],
}

/// Get system information from SMBIOS tables
fn get_system_info(system_table: &SystemTable<Boot>) -> Option<SystemInfo> {
    // Look for SMBIOS table
    for config_entry in system_table.config_table() {
        // SMBIOS 3.0 GUID
        if config_entry.guid.to_bytes() == [
            0xF2, 0xFD, 0x32, 0xE3, 0x59, 0xD5, 0xC1, 0x48,
            0x8B, 0xAD, 0x9F, 0x7D, 0x4C, 0x4D, 0x37, 0xF5
        ] {
            // Parse SMBIOS entry point and extract system info
            // (Full implementation would parse SMBIOS structures)
            return None;
        }
    }

    None
}

// =============================================================================
// MEMORY DETECTION HELPERS
// =============================================================================

/// Get total system memory in bytes
fn get_total_memory(memory_map: &impl MemoryMap) -> u64 {
    let mut total = 0u64;

    for desc in memory_map.entries() {
        match desc.ty {
            MemoryType::CONVENTIONAL
            | MemoryType::BOOT_SERVICES_CODE
            | MemoryType::BOOT_SERVICES_DATA
            | MemoryType::LOADER_CODE
            | MemoryType::LOADER_DATA => {
                total += desc.page_count * PAGE_SIZE as u64;
            }
            _ => {}
        }
    }

    total
}

/// Format bytes as human-readable string
fn format_bytes(bytes: u64) -> (u64, &'static str) {
    if bytes >= 1024 * 1024 * 1024 {
        (bytes / (1024 * 1024 * 1024), "GB")
    } else if bytes >= 1024 * 1024 {
        (bytes / (1024 * 1024), "MB")
    } else if bytes >= 1024 {
        (bytes / 1024, "KB")
    } else {
        (bytes, "B")
    }
}

// =============================================================================
// SERIAL CONSOLE
// =============================================================================

/// Standard PC serial port addresses
const COM1_PORT: u16 = 0x3F8;
const COM2_PORT: u16 = 0x2F8;
const COM3_PORT: u16 = 0x3E8;
const COM4_PORT: u16 = 0x2E8;

/// Serial port register offsets
const SERIAL_DATA: u16 = 0;          // Data register
const SERIAL_IER: u16 = 1;           // Interrupt Enable Register
const SERIAL_FCR: u16 = 2;           // FIFO Control Register
const SERIAL_LCR: u16 = 3;           // Line Control Register
const SERIAL_MCR: u16 = 4;           // Modem Control Register
const SERIAL_LSR: u16 = 5;           // Line Status Register
const SERIAL_DLL: u16 = 0;           // Divisor Latch Low (when DLAB=1)
const SERIAL_DLH: u16 = 1;           // Divisor Latch High (when DLAB=1)

/// Serial console for debugging output
pub struct SerialConsole {
    port_base: u16,
    initialized: bool,
}

impl SerialConsole {
    pub const fn new() -> Self {
        Self {
            port_base: COM1_PORT,
            initialized: false,
        }
    }

    /// Initialize serial port with specified baud rate
    pub fn init(&mut self, port: u8, baud_rate: u32) {
        self.port_base = match port {
            0 => COM1_PORT,
            1 => COM2_PORT,
            2 => COM3_PORT,
            3 => COM4_PORT,
            _ => COM1_PORT,
        };

        // Calculate divisor for baud rate (115200 is the base)
        let divisor = (115200 / baud_rate) as u16;

        unsafe {
            // Disable interrupts
            self.outb(SERIAL_IER, 0x00);

            // Enable DLAB (Divisor Latch Access Bit)
            self.outb(SERIAL_LCR, 0x80);

            // Set divisor
            self.outb(SERIAL_DLL, (divisor & 0xFF) as u8);
            self.outb(SERIAL_DLH, ((divisor >> 8) & 0xFF) as u8);

            // 8 bits, no parity, one stop bit (8N1)
            self.outb(SERIAL_LCR, 0x03);

            // Enable FIFO, clear them, with 14-byte threshold
            self.outb(SERIAL_FCR, 0xC7);

            // Enable IRQs, RTS/DSR set
            self.outb(SERIAL_MCR, 0x0B);

            // Enable interrupts
            self.outb(SERIAL_IER, 0x01);
        }

        self.initialized = true;
    }

    /// Check if transmit buffer is empty
    fn is_transmit_empty(&self) -> bool {
        unsafe {
            (self.inb(SERIAL_LSR) & 0x20) != 0
        }
    }

    /// Write a byte to the serial port
    pub fn write_byte(&self, b: u8) {
        if !self.initialized {
            return;
        }

        // Wait for transmit buffer to be empty
        while !self.is_transmit_empty() {
            core::hint::spin_loop();
        }

        unsafe {
            self.outb(SERIAL_DATA, b);
        }
    }

    /// Write a string to the serial port
    pub fn write_str(&self, s: &str) {
        for b in s.bytes() {
            if b == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(b);
        }
    }

    /// Check if data is available to read
    pub fn data_available(&self) -> bool {
        if !self.initialized {
            return false;
        }
        unsafe {
            (self.inb(SERIAL_LSR) & 0x01) != 0
        }
    }

    /// Read a byte from the serial port (blocking)
    pub fn read_byte(&self) -> u8 {
        while !self.data_available() {
            core::hint::spin_loop();
        }
        unsafe {
            self.inb(SERIAL_DATA)
        }
    }

    /// Read a byte if available (non-blocking)
    pub fn try_read_byte(&self) -> Option<u8> {
        if self.data_available() {
            Some(unsafe { self.inb(SERIAL_DATA) })
        } else {
            None
        }
    }

    /// Output a byte to an I/O port
    #[inline]
    unsafe fn outb(&self, offset: u16, value: u8) {
        core::arch::asm!(
            "out dx, al",
            in("dx") self.port_base + offset,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }

    /// Input a byte from an I/O port
    #[inline]
    unsafe fn inb(&self, offset: u16) -> u8 {
        let value: u8;
        core::arch::asm!(
            "in al, dx",
            in("dx") self.port_base + offset,
            out("al") value,
            options(nomem, nostack, preserves_flags)
        );
        value
    }
}

// =============================================================================
// BOOT PROGRESS REPORTING
// =============================================================================

/// Boot stages for progress tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BootStage {
    Init = 0,
    LoadConfig = 1,
    InitGraphics = 2,
    CheckSecureBoot = 3,
    FindAcpi = 4,
    LoadKernel = 5,
    LoadInitrd = 6,
    AllocateMemory = 7,
    BuildMemoryMap = 8,
    ExitBootServices = 9,
    JumpToKernel = 10,
}

/// Boot progress reporter
pub struct BootProgress {
    current_stage: BootStage,
    serial: Option<SerialConsole>,
}

impl BootProgress {
    pub const fn new() -> Self {
        Self {
            current_stage: BootStage::Init,
            serial: None,
        }
    }

    pub fn enable_serial(&mut self, port: u8, baud: u32) {
        let mut serial = SerialConsole::new();
        serial.init(port, baud);
        self.serial = Some(serial);
    }

    pub fn set_stage(&mut self, stage: BootStage) {
        self.current_stage = stage;

        if let Some(serial) = &self.serial {
            serial.write_str("[STAGE ");
            serial.write_byte(b'0' + stage as u8);
            serial.write_str("] ");
            serial.write_str(match stage {
                BootStage::Init => "Initializing bootloader\n",
                BootStage::LoadConfig => "Loading boot configuration\n",
                BootStage::InitGraphics => "Initializing graphics\n",
                BootStage::CheckSecureBoot => "Checking Secure Boot status\n",
                BootStage::FindAcpi => "Locating ACPI tables\n",
                BootStage::LoadKernel => "Loading kernel\n",
                BootStage::LoadInitrd => "Loading initial ramdisk\n",
                BootStage::AllocateMemory => "Allocating kernel memory\n",
                BootStage::BuildMemoryMap => "Building memory map\n",
                BootStage::ExitBootServices => "Exiting boot services\n",
                BootStage::JumpToKernel => "Jumping to kernel\n",
            });
        }
    }

    pub fn log(&self, msg: &str) {
        if let Some(serial) = &self.serial {
            serial.write_str("[LOG] ");
            serial.write_str(msg);
            serial.write_str("\n");
        }
    }

    pub fn error(&self, msg: &str) {
        if let Some(serial) = &self.serial {
            serial.write_str("[ERROR] ");
            serial.write_str(msg);
            serial.write_str("\n");
        }
    }
}

// =============================================================================
// CPU FEATURE DETECTION
// =============================================================================

/// CPU features detected at boot
#[derive(Debug, Clone, Copy, Default)]
pub struct CpuFeatures {
    /// CPU vendor string
    pub vendor: [u8; 12],
    /// SSE support
    pub has_sse: bool,
    /// SSE2 support
    pub has_sse2: bool,
    /// SSE3 support
    pub has_sse3: bool,
    /// SSSE3 support
    pub has_ssse3: bool,
    /// SSE4.1 support
    pub has_sse41: bool,
    /// SSE4.2 support
    pub has_sse42: bool,
    /// AVX support
    pub has_avx: bool,
    /// AVX2 support
    pub has_avx2: bool,
    /// x2APIC support
    pub has_x2apic: bool,
    /// Long mode (64-bit) support
    pub has_long_mode: bool,
    /// NX (No-Execute) bit support
    pub has_nx: bool,
    /// 1GB pages support
    pub has_1gb_pages: bool,
}

/// Detect CPU features using CPUID
fn detect_cpu_features() -> CpuFeatures {
    let mut features = CpuFeatures::default();

    unsafe {
        // Get vendor string (CPUID function 0)
        let eax: u32;
        let ebx: u32;
        let ecx: u32;
        let edx: u32;

        // LLVM reserves rbx, so we need to save/restore it manually
        core::arch::asm!(
            "push rbx",
            "cpuid",
            "mov {ebx_out:e}, ebx",
            "pop rbx",
            ebx_out = out(reg) ebx,
            inout("eax") 0u32 => eax,
            out("ecx") ecx,
            out("edx") edx,
            options(nostack, preserves_flags)
        );

        // Copy vendor string (EBX, EDX, ECX order)
        features.vendor[0..4].copy_from_slice(&ebx.to_le_bytes());
        features.vendor[4..8].copy_from_slice(&edx.to_le_bytes());
        features.vendor[8..12].copy_from_slice(&ecx.to_le_bytes());

        // Get feature flags (CPUID function 1)
        let eax1: u32;
        let ebx1: u32;
        let ecx1: u32;
        let edx1: u32;

        core::arch::asm!(
            "push rbx",
            "cpuid",
            "mov {ebx_out:e}, ebx",
            "pop rbx",
            ebx_out = out(reg) ebx1,
            inout("eax") 1u32 => eax1,
            out("ecx") ecx1,
            out("edx") edx1,
            options(nostack, preserves_flags)
        );

        let _ = (eax1, ebx1); // Suppress unused warnings

        // EDX flags
        features.has_sse = (edx1 & (1 << 25)) != 0;
        features.has_sse2 = (edx1 & (1 << 26)) != 0;

        // ECX flags
        features.has_sse3 = (ecx1 & (1 << 0)) != 0;
        features.has_ssse3 = (ecx1 & (1 << 9)) != 0;
        features.has_sse41 = (ecx1 & (1 << 19)) != 0;
        features.has_sse42 = (ecx1 & (1 << 20)) != 0;
        features.has_avx = (ecx1 & (1 << 28)) != 0;
        features.has_x2apic = (ecx1 & (1 << 21)) != 0;

        // Get extended features (CPUID function 7)
        let eax7: u32;
        let ebx7: u32;
        let ecx7: u32;
        let edx7: u32;

        core::arch::asm!(
            "push rbx",
            "cpuid",
            "mov {ebx_out:e}, ebx",
            "pop rbx",
            ebx_out = out(reg) ebx7,
            inout("eax") 7u32 => eax7,
            inout("ecx") 0u32 => ecx7,
            out("edx") edx7,
            options(nostack, preserves_flags)
        );

        let _ = (eax7, ecx7, edx7); // Suppress unused warnings
        features.has_avx2 = (ebx7 & (1 << 5)) != 0;

        // Get extended features (CPUID function 0x80000001)
        let eax_ext: u32;
        let ebx_ext: u32;
        let ecx_ext: u32;
        let edx_ext: u32;

        core::arch::asm!(
            "push rbx",
            "cpuid",
            "mov {ebx_out:e}, ebx",
            "pop rbx",
            ebx_out = out(reg) ebx_ext,
            inout("eax") 0x80000001u32 => eax_ext,
            out("ecx") ecx_ext,
            out("edx") edx_ext,
            options(nostack, preserves_flags)
        );

        let _ = (eax_ext, ebx_ext, ecx_ext); // Suppress unused warnings
        features.has_long_mode = (edx_ext & (1 << 29)) != 0;
        features.has_nx = (edx_ext & (1 << 20)) != 0;
        features.has_1gb_pages = (edx_ext & (1 << 26)) != 0;
    }

    features
}

// =============================================================================
// KERNEL COMMAND LINE BUILDER
// =============================================================================

/// Build kernel command line
pub struct CmdlineBuilder {
    buffer: [u8; 512],
    len: usize,
}

impl CmdlineBuilder {
    pub const fn new() -> Self {
        Self {
            buffer: [0u8; 512],
            len: 0,
        }
    }

    pub fn append(&mut self, s: &str) {
        for byte in s.bytes() {
            if self.len < 511 {
                self.buffer[self.len] = byte;
                self.len += 1;
            }
        }
    }

    pub fn append_param(&mut self, key: &str, value: &str) {
        if self.len > 0 {
            self.append(" ");
        }
        self.append(key);
        self.append("=");
        self.append(value);
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buffer[..self.len]).unwrap_or("")
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buffer[..self.len]
    }

    pub fn len(&self) -> usize {
        self.len
    }
}

// =============================================================================
// BOOT TIMEOUT
// =============================================================================

/// Simple delay using TSC (Time Stamp Counter)
fn delay_ms(ms: u32) {
    // This is a rough approximation - actual timing depends on CPU frequency
    // For accurate timing, we'd need to calibrate using PIT or HPET
    let cycles = (ms as u64) * 1_000_000; // Assume ~1GHz CPU

    let start = read_tsc();
    while read_tsc() - start < cycles {
        core::hint::spin_loop();
    }
}

/// Read Time Stamp Counter
#[inline]
fn read_tsc() -> u64 {
    unsafe {
        let lo: u32;
        let hi: u32;
        core::arch::asm!(
            "rdtsc",
            out("eax") lo,
            out("edx") hi,
            options(nomem, nostack, preserves_flags)
        );
        ((hi as u64) << 32) | (lo as u64)
    }
}

// =============================================================================
// PANIC HANDLER
// =============================================================================

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    // Try to write panic info to serial port
    let mut serial = SerialConsole::new();
    serial.init(0, 115200);
    serial.write_str("\n\n");
    serial.write_str("!!! BOOTLOADER PANIC !!!\n");

    if let Some(location) = info.location() {
        serial.write_str("Location: ");
        serial.write_str(location.file());
        serial.write_str("\n");
    }

    serial.write_str("\nSystem halted.\n");

    loop {
        core::hint::spin_loop();
    }
}
