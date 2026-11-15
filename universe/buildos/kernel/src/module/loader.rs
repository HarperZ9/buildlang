// ===============================================================================
// BUILDOS KERNEL - MODULE LOADER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Module Loader
//!
//! Loads ELF relocatable modules into kernel memory.

#![allow(dead_code)]

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::AtomicU32;

use super::elf::{self, ParsedElf, Elf64Rela, r_x86_64, sh_flags};
use super::symbol;
use super::{
    Module, ModuleError, ModuleMemory, ModuleState,
    ModuleInitFn, ModuleExitFn, next_module_id,
};

/// Minimum allocation alignment
const MODULE_ALIGN: usize = 4096;

/// Load a module from bytes
pub fn load_from_bytes(name: &str, data: &[u8]) -> Result<Module, ModuleError> {
    // Parse ELF
    let parsed = ParsedElf::parse(data)?;

    // Get module info
    let mod_info = elf::parse_modinfo(&parsed);

    // Check dependencies
    if !mod_info.depends.is_empty() {
        for dep in mod_info.depends.split(',') {
            let dep = dep.trim();
            if !dep.is_empty() && !super::is_loaded(dep) {
                return Err(ModuleError::MissingDependency(dep.to_string()));
            }
        }
    }

    // Check undefined symbols can be resolved
    let undefined = parsed.get_undefined();
    symbol::can_resolve_all(&undefined)?;

    // Allocate memory for module
    let (text_size, data_size, bss_size, rodata_size) = parsed.calculate_sizes();
    let total_size = text_size + data_size + bss_size + rodata_size;

    let mem = allocate_module_memory(total_size, text_size, data_size, bss_size, rodata_size)?;

    // Load sections
    load_sections(&parsed, &mem)?;

    // Perform relocations
    apply_relocations(&parsed, &mem)?;

    // Get exported symbols with relocated addresses
    let mut exports = parsed.get_exports();
    for sym in &mut exports {
        sym.address = relocate_symbol_address(sym.address, &parsed, &mem);
        sym.module = Some(name.to_string());
    }

    // Register exports
    symbol::register_module_symbols(name, exports.clone());

    // Find init and exit functions
    let init_fn = find_init_fn(&parsed, &mem);
    let exit_fn = find_exit_fn(&parsed, &mem);

    // Parse dependencies
    let dependencies: Vec<String> = mod_info.depends
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // Create module structure
    let module = Module {
        id: next_module_id(),
        name: name.to_string(),
        version: mod_info.version,
        description: mod_info.description,
        author: mod_info.author,
        license: mod_info.license,
        state: ModuleState::Loading,
        mem,
        exports,
        requires: undefined,
        dependents: Vec::new(),
        dependencies,
        params: Vec::new(),
        refcount: AtomicU32::new(0),
        init_fn,
        exit_fn,
    };

    Ok(module)
}

/// Allocate memory for a module
fn allocate_module_memory(
    total: usize,
    text: usize,
    data: usize,
    bss: usize,
    rodata: usize,
) -> Result<ModuleMemory, ModuleError> {
    // Allocate contiguous memory
    // In a real implementation, this would use vmalloc or similar
    let base = crate::memory::vmalloc(total)
        .ok_or(ModuleError::OutOfMemory)?;

    let mut offset = base;

    let code_base = offset;
    offset += text as u64;

    let rodata_base = offset;
    offset += rodata as u64;

    let data_base = offset;
    offset += data as u64;

    let bss_base = offset;
    // Zero the BSS section
    unsafe {
        core::ptr::write_bytes(bss_base as *mut u8, 0, bss);
    }

    Ok(ModuleMemory {
        code_base: code_base as usize,
        code_size: text,
        data_base: data_base as usize,
        data_size: data,
        bss_base: bss_base as usize,
        bss_size: bss,
        rodata_base: rodata_base as usize,
        rodata_size: rodata,
        total_size: total,
    })
}

/// Free module memory
pub fn free_module_memory(module: &Module) {
    if module.mem.total_size > 0 {
        crate::memory::vfree(module.mem.code_base as u64, module.mem.total_size);
    }
}

/// Load sections into allocated memory
fn load_sections(elf: &ParsedElf, mem: &ModuleMemory) -> Result<(), ModuleError> {
    for section in &elf.sections {
        if section.sh_flags & sh_flags::SHF_ALLOC == 0 {
            continue;
        }

        let name = elf.section_name(section);
        let data = elf.section_data(section);

        let dest = if name.starts_with(".text") {
            mem.code_base
        } else if name.starts_with(".rodata") {
            mem.rodata_base
        } else if name.starts_with(".data") {
            mem.data_base
        } else if name.starts_with(".bss") {
            // BSS is already zeroed
            continue;
        } else {
            continue;
        };

        if !data.is_empty() {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    data.as_ptr(),
                    dest as *mut u8,
                    data.len()
                );
            }
        }
    }

    Ok(())
}

/// Apply relocations
fn apply_relocations(elf: &ParsedElf, mem: &ModuleMemory) -> Result<(), ModuleError> {
    // Process relocations for each section
    for section in &elf.sections {
        if section.sh_flags & sh_flags::SHF_ALLOC == 0 {
            continue;
        }

        let name = elf.section_name(section);
        let base = section_base(&name, mem);

        let relas = elf.get_relocations(name);
        for rela in relas {
            apply_relocation(elf, mem, base, rela)?;
        }
    }

    Ok(())
}

/// Get base address for a section name
fn section_base(name: &str, mem: &ModuleMemory) -> usize {
    if name.starts_with(".text") {
        mem.code_base
    } else if name.starts_with(".rodata") {
        mem.rodata_base
    } else if name.starts_with(".data") {
        mem.data_base
    } else if name.starts_with(".bss") {
        mem.bss_base
    } else {
        0
    }
}

/// Apply a single relocation
fn apply_relocation(
    elf: &ParsedElf,
    mem: &ModuleMemory,
    section_base: usize,
    rela: &Elf64Rela,
) -> Result<(), ModuleError> {
    let sym_idx = rela.symbol() as usize;
    let rel_type = rela.rel_type();

    // Get symbol
    let sym = elf.symtab.get(sym_idx);
    let sym_name = sym.map(|s| elf.symbol_name(s)).unwrap_or("");

    // Get symbol value
    let sym_value = if sym.map(|s| s.is_undefined()).unwrap_or(true) {
        // External symbol - look up in symbol table
        symbol::resolve_symbol(sym_name)?
    } else {
        // Internal symbol - relocate
        let sym = sym.unwrap();
        relocate_symbol_address(sym.st_value as usize, elf, mem)
    };

    // Calculate relocation address
    let rel_addr = section_base + rela.r_offset as usize;
    let addend = rela.r_addend;

    // Apply relocation based on type
    match rel_type {
        r_x86_64::R_X86_64_NONE => {}

        r_x86_64::R_X86_64_64 => {
            // S + A
            let value = (sym_value as i64 + addend) as u64;
            unsafe {
                *(rel_addr as *mut u64) = value;
            }
        }

        r_x86_64::R_X86_64_PC32 => {
            // S + A - P
            let value = (sym_value as i64 + addend - rel_addr as i64) as i32;
            unsafe {
                *(rel_addr as *mut i32) = value;
            }
        }

        r_x86_64::R_X86_64_PLT32 => {
            // L + A - P (same as PC32 for us since we link statically)
            let value = (sym_value as i64 + addend - rel_addr as i64) as i32;
            unsafe {
                *(rel_addr as *mut i32) = value;
            }
        }

        r_x86_64::R_X86_64_32 => {
            // S + A (32-bit)
            let value = (sym_value as i64 + addend) as u32;
            unsafe {
                *(rel_addr as *mut u32) = value;
            }
        }

        r_x86_64::R_X86_64_32S => {
            // S + A (signed 32-bit)
            let value = (sym_value as i64 + addend) as i32;
            unsafe {
                *(rel_addr as *mut i32) = value;
            }
        }

        _ => {
            crate::kprintln!("[MODULE] Unknown relocation type: {}", rel_type);
            return Err(ModuleError::InvalidFormat);
        }
    }

    Ok(())
}

/// Relocate a symbol address from ELF to memory
fn relocate_symbol_address(addr: usize, elf: &ParsedElf, mem: &ModuleMemory) -> usize {
    // Find which section this address is in
    for section in &elf.sections {
        if section.sh_flags & sh_flags::SHF_ALLOC == 0 {
            continue;
        }

        let sec_start = section.sh_addr as usize;
        let sec_end = sec_start + section.sh_size as usize;

        if addr >= sec_start && addr < sec_end {
            let offset = addr - sec_start;
            let name = elf.section_name(section);
            let base = section_base(name, mem);
            return base + offset;
        }
    }

    addr
}

/// Find the init function
fn find_init_fn(elf: &ParsedElf, mem: &ModuleMemory) -> Option<ModuleInitFn> {
    // Look for "init_module" or "__init" symbol
    for sym in &elf.symtab {
        let name = elf.symbol_name(sym);
        if name == "init_module" || name == "__init" {
            let addr = relocate_symbol_address(sym.st_value as usize, elf, mem);
            if addr != 0 {
                return Some(unsafe { core::mem::transmute(addr) });
            }
        }
    }

    // Check .init.text section
    if let Some(_section) = elf.find_section(".init.text") {
        let addr = mem.code_base; // Simplified - would need offset
        if addr != 0 {
            return Some(unsafe { core::mem::transmute(addr) });
        }
    }

    None
}

/// Find the exit function
fn find_exit_fn(elf: &ParsedElf, mem: &ModuleMemory) -> Option<ModuleExitFn> {
    // Look for "cleanup_module" or "__exit" symbol
    for sym in &elf.symtab {
        let name = elf.symbol_name(sym);
        if name == "cleanup_module" || name == "__exit" {
            let addr = relocate_symbol_address(sym.st_value as usize, elf, mem);
            if addr != 0 {
                return Some(unsafe { core::mem::transmute(addr) });
            }
        }
    }

    None
}

/// Set memory protections for module sections
pub fn set_module_permissions(mem: &ModuleMemory) -> Result<(), ModuleError> {
    // Make code section executable (no NX bit)
    crate::memory::set_page_flags(
        mem.code_base as u64,
        mem.code_size,
        crate::memory::PageFlags::PRESENT,
    ).map_err(|_| ModuleError::OutOfMemory)?;

    // Make rodata read-only (with NX bit)
    crate::memory::set_page_flags(
        mem.rodata_base as u64,
        mem.rodata_size,
        crate::memory::PageFlags::PRESENT | crate::memory::PageFlags::NO_EXECUTE,
    ).map_err(|_| ModuleError::OutOfMemory)?;

    // Make data read-write (with NX bit)
    crate::memory::set_page_flags(
        mem.data_base as u64,
        mem.data_size,
        crate::memory::PageFlags::PRESENT | crate::memory::PageFlags::WRITABLE | crate::memory::PageFlags::NO_EXECUTE,
    ).map_err(|_| ModuleError::OutOfMemory)?;

    // Make BSS read-write (with NX bit)
    crate::memory::set_page_flags(
        mem.bss_base as u64,
        mem.bss_size,
        crate::memory::PageFlags::PRESENT | crate::memory::PageFlags::WRITABLE | crate::memory::PageFlags::NO_EXECUTE,
    ).map_err(|_| ModuleError::OutOfMemory)?;

    // Flush TLB
    unsafe { crate::cpu::flush_tlb(); }

    Ok(())
}

/// Verify module signature (if required)
pub fn verify_signature(data: &[u8]) -> Result<(), ModuleError> {
    // Look for module signature at end of file
    // Format: "~Module signature appended~\n"
    const SIG_MAGIC: &[u8] = b"~Module signature appended~\n";

    if data.len() < SIG_MAGIC.len() {
        // No signature present - check if signatures are required
        #[cfg(feature = "module_sig_force")]
        return Err(ModuleError::PermissionDenied);

        return Ok(());
    }

    let potential_magic = &data[data.len() - SIG_MAGIC.len()..];
    if potential_magic == SIG_MAGIC {
        // Signature present - verify it
        // In a real implementation, this would verify the cryptographic signature
        // For now, just accept it
        Ok(())
    } else {
        #[cfg(feature = "module_sig_force")]
        return Err(ModuleError::PermissionDenied);

        Ok(())
    }
}

/// Load module from filesystem path
pub fn load_from_path(path: &str) -> Result<Module, ModuleError> {
    let data = crate::fs::read_file(path)
        .map_err(|_| ModuleError::NotFound)?;

    // Extract module name from path
    let name = path.rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".ko");

    load_from_bytes(name, &data)
}

/// Build module info from filesystem
pub fn list_available_modules() -> Vec<AvailableModule> {
    let mut modules = Vec::new();

    // Scan /lib/modules directory
    if let Ok(entries) = crate::fs::readdir("/lib/modules") {
        for entry in entries {
            if entry.name.ends_with(".ko") {
                let name = entry.name.trim_end_matches(".ko").to_string();
                modules.push(AvailableModule {
                    name,
                    path: alloc::format!("/lib/modules/{}", entry.name),
                    loaded: false,
                });
            }
        }
    }

    // Mark loaded modules
    let loaded = super::list_modules();
    for module in &mut modules {
        module.loaded = loaded.iter().any(|m| m.name == module.name);
    }

    modules
}

/// Available module info
#[derive(Clone, Debug)]
pub struct AvailableModule {
    pub name: String,
    pub path: String,
    pub loaded: bool,
}

/// Module load options
#[derive(Clone, Debug, Default)]
pub struct LoadOptions {
    /// Force load even if version mismatch
    pub force: bool,
    /// Force load even if signature check fails
    pub force_sig: bool,
    /// Don't run init function
    pub skip_init: bool,
    /// Module parameters
    pub params: Vec<(String, String)>,
}

/// Load module with options
pub fn load_with_options(name: &str, data: &[u8], opts: &LoadOptions) -> Result<u32, ModuleError> {
    // Verify signature unless forced
    if !opts.force_sig {
        verify_signature(data)?;
    }

    // Load the module
    let mut module = load_from_bytes(name, data)?;

    // Apply parameters
    for (key, value) in &opts.params {
        if let Some(param) = module.params.iter_mut().find(|p| &p.name == key) {
            param.value = value.clone();
        }
    }

    let id = module.id;

    // Insert and initialize
    super::MODULES.write().insert(name.to_string(), module);

    if !opts.skip_init {
        if let Err(e) = super::init_module(name) {
            super::MODULES.write().remove(name);
            return Err(e);
        }
    }

    Ok(id)
}
