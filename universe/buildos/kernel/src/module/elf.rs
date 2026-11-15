// ===============================================================================
// BUILDOS KERNEL - MODULE ELF PARSER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! ELF Parser for Kernel Modules
//!
//! Parses ELF relocatable objects (.ko files) for kernel module loading.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use super::{ModuleError, Symbol, SymbolType, SymbolVisibility};

/// ELF magic number
pub const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

/// ELF class (64-bit)
pub const ELFCLASS64: u8 = 2;

/// ELF data encoding (little endian)
pub const ELFDATA2LSB: u8 = 1;

/// ELF file types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum ElfType {
    None = 0,
    Rel = 1,      // Relocatable
    Exec = 2,     // Executable
    Dyn = 3,      // Shared object
    Core = 4,     // Core file
}

/// ELF machine types
#[repr(u16)]
pub enum ElfMachine {
    X8664 = 0x3e,
    Aarch64 = 0xb7,
}

/// ELF64 file header
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Elf64Header {
    pub e_ident: [u8; 16],
    pub e_type: u16,
    pub e_machine: u16,
    pub e_version: u32,
    pub e_entry: u64,
    pub e_phoff: u64,
    pub e_shoff: u64,
    pub e_flags: u32,
    pub e_ehsize: u16,
    pub e_phentsize: u16,
    pub e_phnum: u16,
    pub e_shentsize: u16,
    pub e_shnum: u16,
    pub e_shstrndx: u16,
}

impl Elf64Header {
    /// Parse header from bytes
    pub fn from_bytes(data: &[u8]) -> Option<&Self> {
        if data.len() < core::mem::size_of::<Self>() {
            return None;
        }

        let header = unsafe {
            &*(data.as_ptr() as *const Self)
        };

        // Validate magic
        if header.e_ident[0..4] != ELF_MAGIC {
            return None;
        }

        // Check 64-bit
        if header.e_ident[4] != ELFCLASS64 {
            return None;
        }

        // Check little endian
        if header.e_ident[5] != ELFDATA2LSB {
            return None;
        }

        Some(header)
    }

    /// Check if this is a relocatable file
    pub fn is_relocatable(&self) -> bool {
        self.e_type == ElfType::Rel as u16
    }
}

/// ELF64 section header
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Elf64SectionHeader {
    pub sh_name: u32,
    pub sh_type: u32,
    pub sh_flags: u64,
    pub sh_addr: u64,
    pub sh_offset: u64,
    pub sh_size: u64,
    pub sh_link: u32,
    pub sh_info: u32,
    pub sh_addralign: u64,
    pub sh_entsize: u64,
}

/// Section types
pub mod sh_type {
    pub const SHT_NULL: u32 = 0;
    pub const SHT_PROGBITS: u32 = 1;
    pub const SHT_SYMTAB: u32 = 2;
    pub const SHT_STRTAB: u32 = 3;
    pub const SHT_RELA: u32 = 4;
    pub const SHT_HASH: u32 = 5;
    pub const SHT_DYNAMIC: u32 = 6;
    pub const SHT_NOTE: u32 = 7;
    pub const SHT_NOBITS: u32 = 8;
    pub const SHT_REL: u32 = 9;
    pub const SHT_DYNSYM: u32 = 11;
}

/// Section flags
pub mod sh_flags {
    pub const SHF_WRITE: u64 = 1 << 0;
    pub const SHF_ALLOC: u64 = 1 << 1;
    pub const SHF_EXECINSTR: u64 = 1 << 2;
    pub const SHF_MERGE: u64 = 1 << 4;
    pub const SHF_STRINGS: u64 = 1 << 5;
    pub const SHF_INFO_LINK: u64 = 1 << 6;
}

/// ELF64 symbol entry
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Elf64Sym {
    pub st_name: u32,
    pub st_info: u8,
    pub st_other: u8,
    pub st_shndx: u16,
    pub st_value: u64,
    pub st_size: u64,
}

impl Elf64Sym {
    /// Get symbol binding
    pub fn binding(&self) -> u8 {
        self.st_info >> 4
    }

    /// Get symbol type
    pub fn sym_type(&self) -> u8 {
        self.st_info & 0xf
    }

    /// Get symbol visibility
    pub fn visibility(&self) -> u8 {
        self.st_other & 0x3
    }

    /// Check if undefined
    pub fn is_undefined(&self) -> bool {
        self.st_shndx == 0
    }
}

/// Symbol bindings
pub mod stb {
    pub const STB_LOCAL: u8 = 0;
    pub const STB_GLOBAL: u8 = 1;
    pub const STB_WEAK: u8 = 2;
}

/// Symbol types
pub mod stt {
    pub const STT_NOTYPE: u8 = 0;
    pub const STT_OBJECT: u8 = 1;
    pub const STT_FUNC: u8 = 2;
    pub const STT_SECTION: u8 = 3;
    pub const STT_FILE: u8 = 4;
    pub const STT_COMMON: u8 = 5;
    pub const STT_TLS: u8 = 6;
}

/// Symbol visibility
pub mod stv {
    pub const STV_DEFAULT: u8 = 0;
    pub const STV_INTERNAL: u8 = 1;
    pub const STV_HIDDEN: u8 = 2;
    pub const STV_PROTECTED: u8 = 3;
}

/// ELF64 relocation entry (with addend)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Elf64Rela {
    pub r_offset: u64,
    pub r_info: u64,
    pub r_addend: i64,
}

impl Elf64Rela {
    /// Get symbol index
    pub fn symbol(&self) -> u32 {
        (self.r_info >> 32) as u32
    }

    /// Get relocation type
    pub fn rel_type(&self) -> u32 {
        self.r_info as u32
    }
}

/// x86-64 relocation types
pub mod r_x86_64 {
    pub const R_X86_64_NONE: u32 = 0;
    pub const R_X86_64_64: u32 = 1;
    pub const R_X86_64_PC32: u32 = 2;
    pub const R_X86_64_GOT32: u32 = 3;
    pub const R_X86_64_PLT32: u32 = 4;
    pub const R_X86_64_COPY: u32 = 5;
    pub const R_X86_64_GLOB_DAT: u32 = 6;
    pub const R_X86_64_JUMP_SLOT: u32 = 7;
    pub const R_X86_64_RELATIVE: u32 = 8;
    pub const R_X86_64_GOTPCREL: u32 = 9;
    pub const R_X86_64_32: u32 = 10;
    pub const R_X86_64_32S: u32 = 11;
}

/// Parsed ELF module
pub struct ParsedElf<'a> {
    /// Raw data
    pub data: &'a [u8],
    /// ELF header
    pub header: &'a Elf64Header,
    /// Section headers
    pub sections: Vec<&'a Elf64SectionHeader>,
    /// Section name string table
    pub shstrtab: &'a [u8],
    /// Symbol table
    pub symtab: Vec<&'a Elf64Sym>,
    /// Symbol string table
    pub strtab: &'a [u8],
}

impl<'a> ParsedElf<'a> {
    /// Parse ELF from bytes
    pub fn parse(data: &'a [u8]) -> Result<Self, ModuleError> {
        let header = Elf64Header::from_bytes(data)
            .ok_or(ModuleError::InvalidFormat)?;

        if !header.is_relocatable() {
            return Err(ModuleError::InvalidFormat);
        }

        // Parse section headers
        let shoff = header.e_shoff as usize;
        let shnum = header.e_shnum as usize;
        let shentsize = header.e_shentsize as usize;

        let mut sections = Vec::with_capacity(shnum);
        for i in 0..shnum {
            let offset = shoff + i * shentsize;
            if offset + shentsize > data.len() {
                return Err(ModuleError::InvalidFormat);
            }

            let section = unsafe {
                &*(data.as_ptr().add(offset) as *const Elf64SectionHeader)
            };
            sections.push(section);
        }

        // Get section name string table
        let shstrndx = header.e_shstrndx as usize;
        if shstrndx >= sections.len() {
            return Err(ModuleError::InvalidFormat);
        }

        let shstrtab_section = sections[shstrndx];
        let shstrtab = &data[shstrtab_section.sh_offset as usize..
            (shstrtab_section.sh_offset + shstrtab_section.sh_size) as usize];

        // Find symbol table and string table
        let mut symtab = Vec::new();
        let mut strtab: &[u8] = &[];

        for section in &sections {
            if section.sh_type == sh_type::SHT_SYMTAB {
                let sym_count = section.sh_size / section.sh_entsize;
                let sym_offset = section.sh_offset as usize;

                for i in 0..sym_count {
                    let offset = sym_offset + (i as usize) * (section.sh_entsize as usize);
                    let sym = unsafe {
                        &*(data.as_ptr().add(offset) as *const Elf64Sym)
                    };
                    symtab.push(sym);
                }

                // Get associated string table
                let strtab_section = sections[section.sh_link as usize];
                strtab = &data[strtab_section.sh_offset as usize..
                    (strtab_section.sh_offset + strtab_section.sh_size) as usize];
            }
        }

        Ok(Self {
            data,
            header,
            sections,
            shstrtab,
            symtab,
            strtab,
        })
    }

    /// Get section name
    pub fn section_name(&self, section: &Elf64SectionHeader) -> &str {
        let start = section.sh_name as usize;
        let end = self.shstrtab[start..].iter()
            .position(|&c| c == 0)
            .map(|p| start + p)
            .unwrap_or(self.shstrtab.len());

        core::str::from_utf8(&self.shstrtab[start..end])
            .unwrap_or("")
    }

    /// Get symbol name
    pub fn symbol_name(&self, sym: &Elf64Sym) -> &str {
        let start = sym.st_name as usize;
        if start >= self.strtab.len() {
            return "";
        }

        let end = self.strtab[start..].iter()
            .position(|&c| c == 0)
            .map(|p| start + p)
            .unwrap_or(self.strtab.len());

        core::str::from_utf8(&self.strtab[start..end])
            .unwrap_or("")
    }

    /// Find section by name
    pub fn find_section(&self, name: &str) -> Option<&Elf64SectionHeader> {
        self.sections.iter()
            .find(|s| self.section_name(s) == name)
            .copied()
    }

    /// Get section data
    pub fn section_data(&self, section: &Elf64SectionHeader) -> &[u8] {
        if section.sh_type == sh_type::SHT_NOBITS {
            return &[];
        }

        let start = section.sh_offset as usize;
        let end = start + section.sh_size as usize;

        if end <= self.data.len() {
            &self.data[start..end]
        } else {
            &[]
        }
    }

    /// Get relocations for a section
    pub fn get_relocations(&self, section_name: &str) -> Vec<&Elf64Rela> {
        let rela_name = alloc::format!(".rela{}", section_name);
        let mut relas = Vec::new();

        for section in &self.sections {
            if section.sh_type == sh_type::SHT_RELA {
                let name = self.section_name(section);
                if name == rela_name {
                    let count = section.sh_size / section.sh_entsize;
                    let offset = section.sh_offset as usize;

                    for i in 0..count {
                        let rela_offset = offset + (i as usize) * (section.sh_entsize as usize);
                        let rela = unsafe {
                            &*(self.data.as_ptr().add(rela_offset) as *const Elf64Rela)
                        };
                        relas.push(rela);
                    }
                }
            }
        }

        relas
    }

    /// Get exported symbols
    pub fn get_exports(&self) -> Vec<Symbol> {
        let mut exports = Vec::new();

        for sym in &self.symtab {
            // Skip undefined and local symbols
            if sym.is_undefined() {
                continue;
            }
            if sym.binding() == stb::STB_LOCAL {
                continue;
            }

            let name = self.symbol_name(sym);
            if name.is_empty() {
                continue;
            }

            // Skip section symbols
            if sym.sym_type() == stt::STT_SECTION {
                continue;
            }

            let sym_type = match sym.sym_type() {
                stt::STT_FUNC => SymbolType::Function,
                stt::STT_OBJECT => SymbolType::Object,
                stt::STT_COMMON => SymbolType::Common,
                stt::STT_TLS => SymbolType::Tls,
                stt::STT_FILE => SymbolType::File,
                stt::STT_SECTION => SymbolType::Section,
                _ => SymbolType::Unknown,
            };

            let visibility = match sym.visibility() {
                stv::STV_DEFAULT => SymbolVisibility::Default,
                stv::STV_HIDDEN => SymbolVisibility::Hidden,
                stv::STV_PROTECTED => SymbolVisibility::Protected,
                stv::STV_INTERNAL => SymbolVisibility::Internal,
                _ => SymbolVisibility::Default,
            };

            exports.push(Symbol {
                name: name.to_string(),
                address: sym.st_value as usize,
                size: sym.st_size as usize,
                sym_type,
                visibility,
                module: None,
            });
        }

        exports
    }

    /// Get required (undefined) symbols
    pub fn get_undefined(&self) -> Vec<String> {
        let mut undefined = Vec::new();

        for sym in &self.symtab {
            if sym.is_undefined() && sym.binding() != stb::STB_LOCAL {
                let name = self.symbol_name(sym);
                if !name.is_empty() {
                    undefined.push(name.to_string());
                }
            }
        }

        undefined
    }

    /// Calculate required memory sizes
    pub fn calculate_sizes(&self) -> (usize, usize, usize, usize) {
        let mut text_size = 0usize;
        let mut data_size = 0usize;
        let mut bss_size = 0usize;
        let mut rodata_size = 0usize;

        for section in &self.sections {
            if section.sh_flags & sh_flags::SHF_ALLOC == 0 {
                continue;
            }

            let name = self.section_name(section);
            let size = section.sh_size as usize;

            if name.starts_with(".text") {
                text_size += size;
            } else if name.starts_with(".data") {
                data_size += size;
            } else if name.starts_with(".bss") || section.sh_type == sh_type::SHT_NOBITS {
                bss_size += size;
            } else if name.starts_with(".rodata") {
                rodata_size += size;
            }
        }

        // Align to page boundaries
        let page_size = 4096;
        text_size = (text_size + page_size - 1) & !(page_size - 1);
        data_size = (data_size + page_size - 1) & !(page_size - 1);
        bss_size = (bss_size + page_size - 1) & !(page_size - 1);
        rodata_size = (rodata_size + page_size - 1) & !(page_size - 1);

        (text_size, data_size, bss_size, rodata_size)
    }
}

/// Find modinfo section and parse it
pub fn parse_modinfo(elf: &ParsedElf) -> ModInfo {
    let mut info = ModInfo::default();

    if let Some(section) = elf.find_section(".modinfo") {
        let data = elf.section_data(section);
        let mut i = 0;

        while i < data.len() {
            // Find key=value pair
            let start = i;
            while i < data.len() && data[i] != 0 {
                i += 1;
            }

            if i > start {
                if let Ok(entry) = core::str::from_utf8(&data[start..i]) {
                    if let Some((key, value)) = entry.split_once('=') {
                        match key {
                            "license" => info.license = value.to_string(),
                            "description" => info.description = value.to_string(),
                            "author" => info.author = value.to_string(),
                            "version" => info.version = value.to_string(),
                            "vermagic" => info.vermagic = value.to_string(),
                            "depends" => info.depends = value.to_string(),
                            "alias" => info.aliases.push(value.to_string()),
                            "srcversion" => info.srcversion = value.to_string(),
                            _ => {}
                        }
                    }
                }
            }

            i += 1; // Skip null terminator
        }
    }

    info
}

/// Module info from .modinfo section
#[derive(Clone, Debug, Default)]
pub struct ModInfo {
    pub license: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub vermagic: String,
    pub depends: String,
    pub aliases: Vec<String>,
    pub srcversion: String,
}
