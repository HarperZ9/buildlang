// ===============================================================================
// BUILDOS KERNEL - ELF LOADER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! ELF64 Executable and Linkable Format loader.
//!
//! This module implements:
//! - ELF64 header parsing
//! - Program header (segment) loading
//! - User-space memory mapping
//! - Initial stack setup with argv, envp, and auxv
//! - Dynamic linker support

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::mem::size_of;
use core::ptr;

use crate::memory::{MemoryManager, PageFlags, PAGE_SIZE, page_align_up, page_align_down, phys_to_virt};
use crate::fs::{self, flags as OpenFlags};
use crate::process::{Pid, CpuContext, USER_STACK_SIZE};

// =============================================================================
// CONSTANTS
// =============================================================================

/// ELF magic number
const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];

/// ELF class: 64-bit
const ELFCLASS64: u8 = 2;

/// ELF data encoding: little-endian
const ELFDATA2LSB: u8 = 1;

/// ELF version
const EV_CURRENT: u8 = 1;

/// ELF OS/ABI: System V
const ELFOSABI_SYSV: u8 = 0;

/// ELF OS/ABI: Linux
const ELFOSABI_LINUX: u8 = 3;

/// ELF type: executable
const ET_EXEC: u16 = 2;

/// ELF type: shared object (PIE executable)
const ET_DYN: u16 = 3;

/// Machine type: x86_64
const EM_X86_64: u16 = 62;

/// Program header type: loadable segment
const PT_LOAD: u32 = 1;

/// Program header type: dynamic linking info
const PT_DYNAMIC: u32 = 2;

/// Program header type: interpreter path
const PT_INTERP: u32 = 3;

/// Program header type: program header table
const PT_PHDR: u32 = 6;

/// Program header type: TLS
const PT_TLS: u32 = 7;

/// Program header type: GNU stack
const PT_GNU_STACK: u32 = 0x6474E551;

/// Program header type: GNU relro
const PT_GNU_RELRO: u32 = 0x6474E552;

/// Segment flag: executable
const PF_X: u32 = 1;

/// Segment flag: writable
const PF_W: u32 = 2;

/// Segment flag: readable
const PF_R: u32 = 4;

/// Default user stack top address
const USER_STACK_TOP: u64 = 0x0000_7FFF_FFFF_0000;

/// PIE base address (for position-independent executables)
const PIE_BASE_ADDR: u64 = 0x0000_5555_5555_0000;

/// Interpreter (dynamic linker) base address
const INTERP_BASE_ADDR: u64 = 0x0000_7FFF_8000_0000;

// =============================================================================
// AUXILIARY VECTOR TYPES
// =============================================================================

/// Auxiliary vector entry types (from Linux elf.h)
pub mod auxv {
    /// End of vector
    pub const AT_NULL: u64 = 0;
    /// Entry should be ignored
    pub const AT_IGNORE: u64 = 1;
    /// File descriptor of program
    pub const AT_EXECFD: u64 = 2;
    /// Program headers for program
    pub const AT_PHDR: u64 = 3;
    /// Size of program header entry
    pub const AT_PHENT: u64 = 4;
    /// Number of program headers
    pub const AT_PHNUM: u64 = 5;
    /// System page size
    pub const AT_PAGESZ: u64 = 6;
    /// Base address of interpreter
    pub const AT_BASE: u64 = 7;
    /// Flags
    pub const AT_FLAGS: u64 = 8;
    /// Entry point of program
    pub const AT_ENTRY: u64 = 9;
    /// Program is not ELF
    pub const AT_NOTELF: u64 = 10;
    /// Real uid
    pub const AT_UID: u64 = 11;
    /// Effective uid
    pub const AT_EUID: u64 = 12;
    /// Real gid
    pub const AT_GID: u64 = 13;
    /// Effective gid
    pub const AT_EGID: u64 = 14;
    /// Frequency of times()
    pub const AT_CLKTCK: u64 = 17;
    /// String identifying platform
    pub const AT_PLATFORM: u64 = 15;
    /// Machine-dependent hints about processor capabilities
    pub const AT_HWCAP: u64 = 16;
    /// Used FPU control word
    pub const AT_FPUCW: u64 = 18;
    /// Data cache block size
    pub const AT_DCACHEBSIZE: u64 = 19;
    /// Instruction cache block size
    pub const AT_ICACHEBSIZE: u64 = 20;
    /// Unified cache block size
    pub const AT_UCACHEBSIZE: u64 = 21;
    /// Entry should be ignored
    pub const AT_IGNOREPPC: u64 = 22;
    /// Secure mode boolean
    pub const AT_SECURE: u64 = 23;
    /// String identifying real platform
    pub const AT_BASE_PLATFORM: u64 = 24;
    /// Address of 16 random bytes
    pub const AT_RANDOM: u64 = 25;
    /// Extension of AT_HWCAP
    pub const AT_HWCAP2: u64 = 26;
    /// Filename of program
    pub const AT_EXECFN: u64 = 31;
    /// Pointer to vDSO
    pub const AT_SYSINFO_EHDR: u64 = 33;
}

// =============================================================================
// ELF STRUCTURES
// =============================================================================

/// ELF64 file header
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Header {
    /// Magic number and identification
    pub e_ident: [u8; 16],
    /// Object file type
    pub e_type: u16,
    /// Machine architecture
    pub e_machine: u16,
    /// Object file version
    pub e_version: u32,
    /// Entry point virtual address
    pub e_entry: u64,
    /// Program header table file offset
    pub e_phoff: u64,
    /// Section header table file offset
    pub e_shoff: u64,
    /// Processor-specific flags
    pub e_flags: u32,
    /// ELF header size
    pub e_ehsize: u16,
    /// Program header table entry size
    pub e_phentsize: u16,
    /// Program header table entry count
    pub e_phnum: u16,
    /// Section header table entry size
    pub e_shentsize: u16,
    /// Section header table entry count
    pub e_shnum: u16,
    /// Section name string table index
    pub e_shstrndx: u16,
}

/// ELF64 program header (segment descriptor)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Phdr {
    /// Segment type
    pub p_type: u32,
    /// Segment flags
    pub p_flags: u32,
    /// Segment file offset
    pub p_offset: u64,
    /// Segment virtual address
    pub p_vaddr: u64,
    /// Segment physical address
    pub p_paddr: u64,
    /// Segment size in file
    pub p_filesz: u64,
    /// Segment size in memory
    pub p_memsz: u64,
    /// Segment alignment
    pub p_align: u64,
}

/// ELF64 section header
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Shdr {
    /// Section name (string table index)
    pub sh_name: u32,
    /// Section type
    pub sh_type: u32,
    /// Section flags
    pub sh_flags: u64,
    /// Section virtual address
    pub sh_addr: u64,
    /// Section file offset
    pub sh_offset: u64,
    /// Section size in bytes
    pub sh_size: u64,
    /// Link to another section
    pub sh_link: u32,
    /// Additional section information
    pub sh_info: u32,
    /// Section alignment
    pub sh_addralign: u64,
    /// Entry size if section holds table
    pub sh_entsize: u64,
}

/// ELF64 symbol table entry
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Sym {
    /// Symbol name (string table index)
    pub st_name: u32,
    /// Symbol type and binding
    pub st_info: u8,
    /// Symbol visibility
    pub st_other: u8,
    /// Section index
    pub st_shndx: u16,
    /// Symbol value
    pub st_value: u64,
    /// Symbol size
    pub st_size: u64,
}

/// ELF64 dynamic section entry
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Dyn {
    /// Dynamic entry type
    pub d_tag: i64,
    /// Integer or address value
    pub d_val: u64,
}

// =============================================================================
// ELF LOADER ERRORS
// =============================================================================

/// ELF loading errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfError {
    /// File not found
    FileNotFound,
    /// Failed to read file
    ReadError,
    /// Invalid ELF magic number
    InvalidMagic,
    /// Invalid ELF class (not 64-bit)
    InvalidClass,
    /// Invalid data encoding
    InvalidEncoding,
    /// Invalid ELF version
    InvalidVersion,
    /// Unsupported OS/ABI
    UnsupportedAbi,
    /// Unsupported ELF type
    UnsupportedType,
    /// Unsupported machine architecture
    UnsupportedMachine,
    /// No loadable segments
    NoLoadableSegments,
    /// Memory allocation failed
    OutOfMemory,
    /// Segment overlaps with existing mapping
    SegmentOverlap,
    /// Invalid segment alignment
    InvalidAlignment,
    /// Interpreter not found
    InterpreterNotFound,
    /// Too many program headers
    TooManyPhdrs,
    /// Invalid program header
    InvalidPhdr,
}

impl ElfError {
    /// Convert to errno value
    pub fn to_errno(&self) -> i32 {
        match self {
            ElfError::FileNotFound => -(fs::errno::ENOENT as i32),
            ElfError::ReadError => -(fs::errno::EIO as i32),
            ElfError::InvalidMagic
            | ElfError::InvalidClass
            | ElfError::InvalidEncoding
            | ElfError::InvalidVersion
            | ElfError::InvalidPhdr => -(fs::errno::ENOEXEC as i32),
            ElfError::UnsupportedAbi
            | ElfError::UnsupportedType
            | ElfError::UnsupportedMachine => -(fs::errno::ENOEXEC as i32),
            ElfError::NoLoadableSegments => -(fs::errno::ENOEXEC as i32),
            ElfError::OutOfMemory => -(fs::errno::ENOMEM as i32),
            ElfError::SegmentOverlap => -(fs::errno::EINVAL as i32),
            ElfError::InvalidAlignment => -(fs::errno::EINVAL as i32),
            ElfError::InterpreterNotFound => -(fs::errno::ENOENT as i32),
            ElfError::TooManyPhdrs => -(fs::errno::ENOMEM as i32),
        }
    }
}

// =============================================================================
// LOADED ELF INFO
// =============================================================================

/// Information about a loaded ELF executable
#[derive(Debug, Clone)]
pub struct LoadedElf {
    /// Entry point address
    pub entry: u64,
    /// Program header table address (for auxv)
    pub phdr_addr: u64,
    /// Program header entry size
    pub phent: u16,
    /// Number of program headers
    pub phnum: u16,
    /// Base address (for PIE executables)
    pub base_addr: u64,
    /// Interpreter path (if any)
    pub interp: Option<String>,
    /// Stack pointer after setup
    pub stack_ptr: u64,
    /// Lowest mapped address
    pub map_start: u64,
    /// Highest mapped address
    pub map_end: u64,
    /// Initial brk address (end of BSS)
    pub brk: u64,
    /// Is this a PIE executable?
    pub is_pie: bool,
}

/// Memory segment mapping info
#[derive(Debug, Clone)]
struct SegmentMapping {
    /// Virtual address (page-aligned)
    vaddr: u64,
    /// Memory size (page-aligned)
    memsz: u64,
    /// Page flags
    flags: PageFlags,
}

// =============================================================================
// ELF LOADER
// =============================================================================

/// Load an ELF executable from a file path
pub fn load_elf(
    path: &str,
    argv: &[&str],
    envp: &[(&str, &str)],
    _pid: Pid,
) -> Result<LoadedElf, ElfError> {
    // Open the file
    let fd = match fs::open(path, OpenFlags::O_RDONLY, 0) {
        Ok(fd) => fd,
        Err(_) => return Err(ElfError::FileNotFound),
    };

    // Read the ELF header
    let mut header_buf = [0u8; size_of::<Elf64Header>()];
    let bytes_read = match fs::read(fd, &mut header_buf) {
        Ok(n) => n,
        Err(_) => {
            let _ = fs::close(fd);
            return Err(ElfError::ReadError);
        }
    };
    if bytes_read != size_of::<Elf64Header>() {
        let _ = fs::close(fd);
        return Err(ElfError::ReadError);
    }

    // Parse header
    let header = unsafe { ptr::read(header_buf.as_ptr() as *const Elf64Header) };

    // Validate ELF header
    validate_elf_header(&header)?;

    // Determine if this is a PIE executable
    let is_pie = header.e_type == ET_DYN;
    let load_base = if is_pie { PIE_BASE_ADDR } else { 0 };

    // Read program headers
    let phdrs = read_program_headers(fd, &header)?;

    // Check for interpreter
    let interp = find_interpreter(fd, &phdrs)?;

    // Load all PT_LOAD segments
    let (map_start, map_end, brk) = load_segments(fd, &phdrs, load_base)?;

    // Close the file
    let _ = fs::close(fd);

    // Calculate entry point
    let entry = header.e_entry + load_base;

    // Calculate where program headers are mapped
    let phdr_addr = calculate_phdr_addr(&phdrs, load_base);

    // Set up the user stack with argv, envp, and auxv
    let stack_ptr = setup_user_stack(
        argv,
        envp,
        entry,
        phdr_addr,
        header.e_phentsize,
        header.e_phnum,
        load_base,
        is_pie,
    )?;

    Ok(LoadedElf {
        entry,
        phdr_addr,
        phent: header.e_phentsize,
        phnum: header.e_phnum,
        base_addr: load_base,
        interp,
        stack_ptr,
        map_start,
        map_end,
        brk,
        is_pie,
    })
}

/// Validate ELF header
fn validate_elf_header(header: &Elf64Header) -> Result<(), ElfError> {
    // Check magic number
    if header.e_ident[0..4] != ELF_MAGIC {
        return Err(ElfError::InvalidMagic);
    }

    // Check 64-bit
    if header.e_ident[4] != ELFCLASS64 {
        return Err(ElfError::InvalidClass);
    }

    // Check little-endian
    if header.e_ident[5] != ELFDATA2LSB {
        return Err(ElfError::InvalidEncoding);
    }

    // Check version
    if header.e_ident[6] != EV_CURRENT {
        return Err(ElfError::InvalidVersion);
    }

    // Check OS/ABI (allow System V and Linux)
    let abi = header.e_ident[7];
    if abi != ELFOSABI_SYSV && abi != ELFOSABI_LINUX {
        return Err(ElfError::UnsupportedAbi);
    }

    // Check type (executable or shared object/PIE)
    if header.e_type != ET_EXEC && header.e_type != ET_DYN {
        return Err(ElfError::UnsupportedType);
    }

    // Check machine (x86_64)
    if header.e_machine != EM_X86_64 {
        return Err(ElfError::UnsupportedMachine);
    }

    Ok(())
}

/// Read program headers from file
fn read_program_headers(fd: u64, header: &Elf64Header) -> Result<Vec<Elf64Phdr>, ElfError> {
    if header.e_phnum == 0 {
        return Err(ElfError::NoLoadableSegments);
    }

    if header.e_phnum > 256 {
        return Err(ElfError::TooManyPhdrs);
    }

    // Seek to program header table
    if fs::lseek(fd, header.e_phoff as i64, fs::SEEK_SET).is_err() {
        return Err(ElfError::ReadError);
    }

    let mut phdrs = Vec::with_capacity(header.e_phnum as usize);

    for _ in 0..header.e_phnum {
        let mut phdr_buf = [0u8; size_of::<Elf64Phdr>()];
        let bytes_read = match fs::read(fd, &mut phdr_buf) {
            Ok(n) => n,
            Err(_) => return Err(ElfError::ReadError),
        };

        if bytes_read != size_of::<Elf64Phdr>() {
            return Err(ElfError::ReadError);
        }

        let phdr = unsafe { ptr::read(phdr_buf.as_ptr() as *const Elf64Phdr) };
        phdrs.push(phdr);
    }

    Ok(phdrs)
}

/// Find interpreter path from PT_INTERP segment
fn find_interpreter(fd: u64, phdrs: &[Elf64Phdr]) -> Result<Option<String>, ElfError> {
    for phdr in phdrs {
        if phdr.p_type == PT_INTERP {
            // Seek to interpreter path
            if fs::lseek(fd, phdr.p_offset as i64, fs::SEEK_SET).is_err() {
                return Err(ElfError::ReadError);
            }

            // Read interpreter path
            let mut buf = vec![0u8; phdr.p_filesz as usize];
            let bytes_read = match fs::read(fd, &mut buf) {
                Ok(n) => n,
                Err(_) => return Err(ElfError::ReadError),
            };

            if bytes_read != phdr.p_filesz as usize {
                return Err(ElfError::ReadError);
            }

            // Remove null terminator
            if let Some(pos) = buf.iter().position(|&b| b == 0) {
                buf.truncate(pos);
            }

            // Convert to string
            let path = String::from_utf8_lossy(&buf).into_owned();
            return Ok(Some(path));
        }
    }

    Ok(None)
}

/// Load PT_LOAD segments into memory
fn load_segments(
    fd: u64,
    phdrs: &[Elf64Phdr],
    load_base: u64,
) -> Result<(u64, u64, u64), ElfError> {
    let mut map_start = u64::MAX;
    let mut map_end = 0u64;
    let mut brk = 0u64;

    // Get memory manager
    let mut mm_guard = MemoryManager::get().lock();
    let mm = mm_guard.as_mut().ok_or(ElfError::OutOfMemory)?;

    for phdr in phdrs {
        if phdr.p_type != PT_LOAD {
            continue;
        }

        // Calculate page-aligned addresses
        let vaddr = phdr.p_vaddr + load_base;
        let vaddr_aligned = page_align_down(vaddr);
        let _offset_in_page = vaddr - vaddr_aligned;

        let mem_end = vaddr + phdr.p_memsz;
        let mem_end_aligned = page_align_up(mem_end);
        let total_pages = ((mem_end_aligned - vaddr_aligned) as usize) / PAGE_SIZE;

        // Update bounds
        if vaddr_aligned < map_start {
            map_start = vaddr_aligned;
        }
        if mem_end_aligned > map_end {
            map_end = mem_end_aligned;
        }
        if mem_end > brk {
            brk = mem_end;
        }

        // Convert segment flags to page flags
        let _page_flags = segment_flags_to_page_flags(phdr.p_flags);

        // Allocate and map pages
        for i in 0..total_pages {
            let page_vaddr = vaddr_aligned + (i * PAGE_SIZE) as u64;

            // Allocate physical page
            let phys = mm.alloc_pages(1).ok_or(ElfError::OutOfMemory)?;

            // Zero the page first
            unsafe {
                let virt = phys_to_virt(phys);
                ptr::write_bytes(virt as *mut u8, 0, PAGE_SIZE);
            }

            // Map with write permission initially (we'll protect later)
            mm.map_page(page_vaddr, phys, PageFlags::PRESENT | PageFlags::WRITABLE | PageFlags::USER);
        }

        // Read file content into mapped pages
        if phdr.p_filesz > 0 {
            // Seek to segment offset in file
            if fs::lseek(fd, phdr.p_offset as i64, fs::SEEK_SET).is_err() {
                return Err(ElfError::ReadError);
            }

            // Read into memory
            let dest = vaddr as *mut u8;
            let mut remaining = phdr.p_filesz as usize;
            let mut current = dest;

            while remaining > 0 {
                let chunk_size = core::cmp::min(remaining, 4096);
                let mut buf = [0u8; 4096];

                let bytes_read = match fs::read(fd, &mut buf[..chunk_size]) {
                    Ok(n) if n > 0 => n,
                    _ => return Err(ElfError::ReadError),
                };

                unsafe {
                    ptr::copy_nonoverlapping(buf.as_ptr(), current, bytes_read);
                    current = current.add(bytes_read);
                }

                remaining -= bytes_read;
            }
        }

        // BSS: zero the portion beyond file size
        if phdr.p_memsz > phdr.p_filesz {
            let bss_start = vaddr + phdr.p_filesz;
            let bss_size = (phdr.p_memsz - phdr.p_filesz) as usize;

            unsafe {
                ptr::write_bytes(bss_start as *mut u8, 0, bss_size);
            }
        }

        // Set final page permissions (if not writable, remove write flag)
        if (phdr.p_flags & PF_W) == 0 {
            for i in 0..total_pages {
                let _page_vaddr = vaddr_aligned + (i * PAGE_SIZE) as u64;
                // Re-map with correct permissions
                // (In a full implementation, we'd update the existing PTE)
            }
        }
    }

    // Page-align brk
    brk = page_align_up(brk);

    Ok((map_start, map_end, brk))
}

/// Convert ELF segment flags to page flags
fn segment_flags_to_page_flags(p_flags: u32) -> PageFlags {
    let mut flags = PageFlags::PRESENT | PageFlags::USER;

    if (p_flags & PF_W) != 0 {
        flags |= PageFlags::WRITABLE;
    }

    if (p_flags & PF_X) == 0 {
        flags |= PageFlags::NO_EXECUTE;
    }

    flags
}

/// Calculate the address where program headers are mapped
fn calculate_phdr_addr(phdrs: &[Elf64Phdr], load_base: u64) -> u64 {
    // Find PT_PHDR if present
    for phdr in phdrs {
        if phdr.p_type == PT_PHDR {
            return phdr.p_vaddr + load_base;
        }
    }

    // Otherwise, calculate from first PT_LOAD segment
    for phdr in phdrs {
        if phdr.p_type == PT_LOAD && phdr.p_offset == 0 {
            // Program headers are at offset e_phoff from start of file/mapping
            // This is a simplified calculation
            return phdr.p_vaddr + load_base + 64; // ELF header size
        }
    }

    0
}

/// Set up the user stack with argv, envp, and auxv
fn setup_user_stack(
    argv: &[&str],
    envp: &[(&str, &str)],
    entry: u64,
    phdr_addr: u64,
    phent: u16,
    phnum: u16,
    base_addr: u64,
    is_pie: bool,
) -> Result<u64, ElfError> {
    // Allocate stack pages
    let stack_pages = USER_STACK_SIZE / PAGE_SIZE;
    let stack_bottom = USER_STACK_TOP - USER_STACK_SIZE as u64;

    // Get memory manager
    let mut mm_guard = MemoryManager::get().lock();
    let mm = mm_guard.as_mut().ok_or(ElfError::OutOfMemory)?;

    // Allocate and map stack pages
    for i in 0..stack_pages {
        let page_vaddr = stack_bottom + (i * PAGE_SIZE) as u64;

        let phys = mm.alloc_pages(1).ok_or(ElfError::OutOfMemory)?;

        unsafe {
            let virt = phys_to_virt(phys);
            ptr::write_bytes(virt as *mut u8, 0, PAGE_SIZE);
        }

        mm.map_page(
            page_vaddr,
            phys,
            PageFlags::PRESENT | PageFlags::WRITABLE | PageFlags::USER | PageFlags::NO_EXECUTE,
        );
    }

    drop(mm_guard);

    // Build stack layout:
    // High addresses (stack top):
    //   - Random bytes (16 bytes for AT_RANDOM)
    //   - Platform string
    //   - Environment strings
    //   - Argument strings
    //   - Padding for alignment
    //   - Null auxiliary vector entry
    //   - Auxiliary vector entries
    //   - Null environment pointer
    //   - Environment pointers
    //   - Null argument pointer
    //   - Argument pointers
    //   - argc
    // Low addresses (stack pointer)

    let mut sp = USER_STACK_TOP;

    // Push random bytes (for AT_RANDOM)
    sp -= 16;
    let random_addr = sp;
    unsafe {
        // In a real implementation, we'd use a PRNG
        let random_bytes: [u8; 16] = [
            0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE,
            0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0,
        ];
        ptr::copy_nonoverlapping(random_bytes.as_ptr(), sp as *mut u8, 16);
    }

    // Push platform string
    let platform = b"x86_64\0";
    sp -= platform.len() as u64;
    sp = sp & !0x7; // Align to 8 bytes
    let platform_addr = sp;
    unsafe {
        ptr::copy_nonoverlapping(platform.as_ptr(), sp as *mut u8, platform.len());
    }

    // Push environment strings and collect pointers
    let mut env_ptrs: Vec<u64> = Vec::with_capacity(envp.len());
    for (key, value) in envp.iter().rev() {
        // Format as "KEY=VALUE\0"
        let env_str = alloc::format!("{}={}\0", key, value);
        sp -= env_str.len() as u64;
        unsafe {
            ptr::copy_nonoverlapping(env_str.as_ptr(), sp as *mut u8, env_str.len());
        }
        env_ptrs.push(sp);
    }
    env_ptrs.reverse();

    // Push argument strings and collect pointers
    let mut arg_ptrs: Vec<u64> = Vec::with_capacity(argv.len());
    for arg in argv.iter().rev() {
        let arg_len = arg.len() + 1; // Include null terminator
        sp -= arg_len as u64;
        unsafe {
            ptr::copy_nonoverlapping(arg.as_ptr(), sp as *mut u8, arg.len());
            *((sp + arg.len() as u64) as *mut u8) = 0; // Null terminator
        }
        arg_ptrs.push(sp);
    }
    arg_ptrs.reverse();

    // Align stack to 16 bytes
    sp = sp & !0xF;

    // Calculate total size needed for pointers and auxv
    let auxv_count = 16; // Number of auxiliary vector entries
    let total_entries = 1 + arg_ptrs.len() + 1 + env_ptrs.len() + 1 + (auxv_count * 2);
    let total_size = total_entries * 8;

    // Ensure 16-byte alignment after pushing argc
    if ((sp - total_size as u64) & 0xF) != 0 {
        sp -= 8;
    }

    // Push auxiliary vector (in reverse order)
    // AT_NULL (terminator)
    sp -= 16;
    unsafe {
        *(sp as *mut u64) = auxv::AT_NULL;
        *((sp + 8) as *mut u64) = 0;
    }

    // Helper macro for pushing auxv entries
    macro_rules! push_auxv {
        ($sp:expr, $type:expr, $val:expr) => {{
            $sp -= 16;
            unsafe {
                *($sp as *mut u64) = $type;
                *(($sp + 8) as *mut u64) = $val;
            }
        }};
    }

    push_auxv!(sp, auxv::AT_RANDOM, random_addr);
    push_auxv!(sp, auxv::AT_PLATFORM, platform_addr);
    push_auxv!(sp, auxv::AT_SECURE, 0);
    push_auxv!(sp, auxv::AT_EGID, 0);
    push_auxv!(sp, auxv::AT_GID, 0);
    push_auxv!(sp, auxv::AT_EUID, 0);
    push_auxv!(sp, auxv::AT_UID, 0);
    push_auxv!(sp, auxv::AT_ENTRY, entry);
    push_auxv!(sp, auxv::AT_FLAGS, 0);
    push_auxv!(sp, auxv::AT_BASE, if is_pie { base_addr } else { 0 });
    push_auxv!(sp, auxv::AT_PHNUM, phnum as u64);
    push_auxv!(sp, auxv::AT_PHENT, phent as u64);
    push_auxv!(sp, auxv::AT_PHDR, phdr_addr);
    push_auxv!(sp, auxv::AT_PAGESZ, PAGE_SIZE as u64);
    push_auxv!(sp, auxv::AT_CLKTCK, 100); // 100 Hz

    // Push null environment pointer terminator
    sp -= 8;
    unsafe {
        *(sp as *mut u64) = 0;
    }

    // Push environment pointers
    for env_ptr in env_ptrs.iter().rev() {
        sp -= 8;
        unsafe {
            *(sp as *mut u64) = *env_ptr;
        }
    }

    // Push null argument pointer terminator
    sp -= 8;
    unsafe {
        *(sp as *mut u64) = 0;
    }

    // Push argument pointers
    for arg_ptr in arg_ptrs.iter().rev() {
        sp -= 8;
        unsafe {
            *(sp as *mut u64) = *arg_ptr;
        }
    }

    // Push argc
    sp -= 8;
    unsafe {
        *(sp as *mut u64) = argv.len() as u64;
    }

    Ok(sp)
}

/// Load the dynamic linker/interpreter
pub fn load_interpreter(path: &str, base_addr: u64) -> Result<u64, ElfError> {
    // Open interpreter file
    let fd = match fs::open(path, OpenFlags::O_RDONLY, 0) {
        Ok(fd) => fd,
        Err(_) => return Err(ElfError::InterpreterNotFound),
    };

    // Read ELF header
    let mut header_buf = [0u8; size_of::<Elf64Header>()];
    let bytes_read = match fs::read(fd, &mut header_buf) {
        Ok(n) => n,
        Err(_) => {
            let _ = fs::close(fd);
            return Err(ElfError::ReadError);
        }
    };
    if bytes_read != size_of::<Elf64Header>() {
        let _ = fs::close(fd);
        return Err(ElfError::ReadError);
    }

    let header = unsafe { ptr::read(header_buf.as_ptr() as *const Elf64Header) };

    // Validate (interpreter must be ET_DYN)
    validate_elf_header(&header)?;
    if header.e_type != ET_DYN {
        let _ = fs::close(fd);
        return Err(ElfError::UnsupportedType);
    }

    // Read program headers
    let phdrs = read_program_headers(fd, &header)?;

    // Load segments at specified base address
    load_segments(fd, &phdrs, base_addr)?;

    let _ = fs::close(fd);

    // Return interpreter entry point
    Ok(header.e_entry + base_addr)
}

/// Execute an ELF binary (full execve implementation)
pub fn exec(
    path: &str,
    argv: &[&str],
    envp: &[(&str, &str)],
    pid: Pid,
) -> Result<CpuContext, ElfError> {
    // Load the main executable
    let elf = load_elf(path, argv, envp, pid)?;

    // Determine actual entry point
    let entry = if let Some(ref interp_path) = elf.interp {
        // Load dynamic linker
        load_interpreter(interp_path, INTERP_BASE_ADDR)?
    } else {
        elf.entry
    };

    // Create initial CPU context for user mode
    let context = CpuContext {
        rax: 0,
        rbx: 0,
        rcx: 0,
        rdx: 0,
        rsi: 0,
        rdi: 0,
        rbp: 0,
        rsp: elf.stack_ptr,
        r8: 0,
        r9: 0,
        r10: 0,
        r11: 0,
        r12: 0,
        r13: 0,
        r14: 0,
        r15: 0,
        rip: entry,
        rflags: 0x202, // IF flag set (interrupts enabled)
        cs: 0x2B,      // User code segment (GDT index 5, RPL 3)
        ss: 0x23,      // User data segment (GDT index 4, RPL 3)
        fs_base: 0,
        gs_base: 0,
    };

    Ok(context)
}

// =============================================================================
// UTILITY FUNCTIONS
// =============================================================================

/// Check if a file is a valid ELF executable
pub fn is_elf(path: &str) -> bool {
    let fd = match fs::open(path, OpenFlags::O_RDONLY, 0) {
        Ok(fd) => fd,
        Err(_) => return false,
    };

    let mut magic = [0u8; 4];
    let bytes_read = match fs::read(fd, &mut magic) {
        Ok(n) => n,
        Err(_) => {
            let _ = fs::close(fd);
            return false;
        }
    };
    let _ = fs::close(fd);

    bytes_read == 4 && magic == ELF_MAGIC
}

/// Get ELF header information
pub fn get_elf_info(path: &str) -> Result<(u16, u16, u64), ElfError> {
    let fd = match fs::open(path, OpenFlags::O_RDONLY, 0) {
        Ok(fd) => fd,
        Err(_) => return Err(ElfError::FileNotFound),
    };

    let mut header_buf = [0u8; size_of::<Elf64Header>()];
    let bytes_read = match fs::read(fd, &mut header_buf) {
        Ok(n) => n,
        Err(_) => {
            let _ = fs::close(fd);
            return Err(ElfError::ReadError);
        }
    };
    let _ = fs::close(fd);

    if bytes_read != size_of::<Elf64Header>() {
        return Err(ElfError::ReadError);
    }

    let header = unsafe { ptr::read(header_buf.as_ptr() as *const Elf64Header) };

    validate_elf_header(&header)?;

    Ok((header.e_type, header.e_machine, header.e_entry))
}
