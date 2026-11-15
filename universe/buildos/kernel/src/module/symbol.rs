// ===============================================================================
// BUILDOS KERNEL - MODULE SYMBOL TABLE
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Kernel Symbol Table
//!
//! Manages exported kernel symbols and module symbols for linking.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::sync::RwLock;
use super::{Symbol, SymbolType, SymbolVisibility};

/// Global symbol table
static SYMBOLS: RwLock<SymbolTable> = RwLock::new(SymbolTable::new());

/// Symbol table
pub struct SymbolTable {
    /// Symbols indexed by name
    by_name: BTreeMap<String, Symbol>,
    /// Symbols indexed by address (for kallsyms-like lookup)
    by_addr: BTreeMap<usize, String>,
}

impl SymbolTable {
    /// Create new symbol table
    pub const fn new() -> Self {
        Self {
            by_name: BTreeMap::new(),
            by_addr: BTreeMap::new(),
        }
    }

    /// Insert a symbol
    pub fn insert(&mut self, sym: Symbol) {
        self.by_addr.insert(sym.address, sym.name.clone());
        self.by_name.insert(sym.name.clone(), sym);
    }

    /// Remove a symbol
    pub fn remove(&mut self, name: &str) {
        if let Some(sym) = self.by_name.remove(name) {
            self.by_addr.remove(&sym.address);
        }
    }

    /// Find by name
    pub fn find(&self, name: &str) -> Option<&Symbol> {
        self.by_name.get(name)
    }

    /// Find by address (exact match)
    pub fn find_by_addr(&self, addr: usize) -> Option<&Symbol> {
        self.by_addr.get(&addr)
            .and_then(|name| self.by_name.get(name))
    }

    /// Find by address (nearest lower)
    pub fn find_by_addr_nearest(&self, addr: usize) -> Option<(&Symbol, usize)> {
        // Find the symbol with the highest address <= addr
        let mut best: Option<(&Symbol, usize)> = None;

        for (sym_addr, name) in &self.by_addr {
            if *sym_addr <= addr {
                let offset = addr - *sym_addr;
                if let Some(sym) = self.by_name.get(name) {
                    match best {
                        None => best = Some((sym, offset)),
                        Some((_, best_offset)) if offset < best_offset => {
                            best = Some((sym, offset));
                        }
                        _ => {}
                    }
                }
            }
        }

        best
    }

    /// Iterate all symbols
    pub fn iter(&self) -> impl Iterator<Item = &Symbol> {
        self.by_name.values()
    }

    /// Count
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }
}

/// Initialize kernel symbols
pub fn init_kernel_symbols() {
    let mut table = SYMBOLS.write();

    // Export core kernel symbols
    // In a real implementation, these would be generated from the kernel's
    // symbol table during build. Here we add some essential ones manually.

    let kernel_exports = [
        // Memory allocation
        ("kmalloc", 0usize, SymbolType::Function),
        ("kfree", 0usize, SymbolType::Function),
        ("krealloc", 0usize, SymbolType::Function),
        ("kzalloc", 0usize, SymbolType::Function),
        ("vmalloc", 0usize, SymbolType::Function),
        ("vfree", 0usize, SymbolType::Function),

        // Printing
        ("kprintln", 0usize, SymbolType::Function),
        ("printk", 0usize, SymbolType::Function),

        // Spinlocks
        ("spin_lock", 0usize, SymbolType::Function),
        ("spin_unlock", 0usize, SymbolType::Function),
        ("spin_lock_irqsave", 0usize, SymbolType::Function),
        ("spin_unlock_irqrestore", 0usize, SymbolType::Function),

        // Mutexes
        ("mutex_lock", 0usize, SymbolType::Function),
        ("mutex_unlock", 0usize, SymbolType::Function),
        ("mutex_trylock", 0usize, SymbolType::Function),

        // Wait queues
        ("wake_up", 0usize, SymbolType::Function),
        ("wake_up_all", 0usize, SymbolType::Function),
        ("wait_event", 0usize, SymbolType::Function),

        // Workqueues
        ("queue_work", 0usize, SymbolType::Function),
        ("flush_work", 0usize, SymbolType::Function),

        // Timers
        ("jiffies", 0usize, SymbolType::Object),
        ("msleep", 0usize, SymbolType::Function),
        ("usleep_range", 0usize, SymbolType::Function),

        // DMA
        ("dma_alloc_coherent", 0usize, SymbolType::Function),
        ("dma_free_coherent", 0usize, SymbolType::Function),
        ("dma_map_single", 0usize, SymbolType::Function),
        ("dma_unmap_single", 0usize, SymbolType::Function),

        // PCI
        ("pci_read_config_byte", 0usize, SymbolType::Function),
        ("pci_write_config_byte", 0usize, SymbolType::Function),
        ("pci_read_config_word", 0usize, SymbolType::Function),
        ("pci_write_config_word", 0usize, SymbolType::Function),
        ("pci_read_config_dword", 0usize, SymbolType::Function),
        ("pci_write_config_dword", 0usize, SymbolType::Function),
        ("pci_enable_device", 0usize, SymbolType::Function),
        ("pci_disable_device", 0usize, SymbolType::Function),
        ("pci_set_master", 0usize, SymbolType::Function),

        // Interrupts
        ("request_irq", 0usize, SymbolType::Function),
        ("free_irq", 0usize, SymbolType::Function),
        ("enable_irq", 0usize, SymbolType::Function),
        ("disable_irq", 0usize, SymbolType::Function),

        // Device registration
        ("register_chrdev", 0usize, SymbolType::Function),
        ("unregister_chrdev", 0usize, SymbolType::Function),
        ("register_blkdev", 0usize, SymbolType::Function),
        ("unregister_blkdev", 0usize, SymbolType::Function),

        // Filesystem
        ("register_filesystem", 0usize, SymbolType::Function),
        ("unregister_filesystem", 0usize, SymbolType::Function),

        // Module
        ("module_put", 0usize, SymbolType::Function),
        ("try_module_get", 0usize, SymbolType::Function),
    ];

    for (name, addr, sym_type) in kernel_exports {
        table.insert(Symbol {
            name: name.to_string(),
            address: addr,
            size: 0,
            sym_type,
            visibility: SymbolVisibility::Default,
            module: None,
        });
    }

    crate::kprintln!("[SYMBOL] Initialized {} kernel symbols", table.len());
}

/// Find a symbol by name
pub fn find_symbol(name: &str) -> Option<Symbol> {
    SYMBOLS.read().find(name).cloned()
}

/// Find a symbol by address
pub fn find_symbol_by_addr(addr: usize) -> Option<Symbol> {
    SYMBOLS.read().find_by_addr(addr).cloned()
}

/// Find nearest symbol to an address
pub fn find_symbol_nearest(addr: usize) -> Option<(Symbol, usize)> {
    SYMBOLS.read()
        .find_by_addr_nearest(addr)
        .map(|(s, offset)| (s.clone(), offset))
}

/// Register a symbol
pub fn register_symbol(sym: Symbol) {
    SYMBOLS.write().insert(sym);
}

/// Register multiple symbols from a module
pub fn register_module_symbols(module_name: &str, symbols: Vec<Symbol>) {
    let mut table = SYMBOLS.write();

    for mut sym in symbols {
        sym.module = Some(module_name.to_string());
        table.insert(sym);
    }
}

/// Remove all symbols from a module
pub fn remove_module_symbols(module_name: &str) {
    let mut table = SYMBOLS.write();

    let to_remove: Vec<String> = table.iter()
        .filter(|s| s.module.as_deref() == Some(module_name))
        .map(|s| s.name.clone())
        .collect();

    for name in to_remove {
        table.remove(&name);
    }
}

/// Resolve a symbol for a module
pub fn resolve_symbol(name: &str) -> Result<usize, super::ModuleError> {
    SYMBOLS.read()
        .find(name)
        .map(|s| s.address)
        .ok_or(super::ModuleError::MissingSymbol(name.to_string()))
}

/// Check if all required symbols can be resolved
pub fn can_resolve_all(symbols: &[String]) -> Result<(), super::ModuleError> {
    let table = SYMBOLS.read();

    for name in symbols {
        if table.find(name).is_none() {
            return Err(super::ModuleError::MissingSymbol(name.clone()));
        }
    }

    Ok(())
}

/// Get symbol count
pub fn symbol_count() -> usize {
    SYMBOLS.read().len()
}

/// Kallsyms-style symbol lookup
pub fn kallsyms_lookup(addr: usize) -> String {
    if let Some((sym, offset)) = find_symbol_nearest(addr) {
        if offset == 0 {
            sym.name
        } else {
            alloc::format!("{}+0x{:x}", sym.name, offset)
        }
    } else {
        alloc::format!("0x{:x}", addr)
    }
}

/// Format address with symbol info for stack traces
pub fn format_symbol(addr: usize) -> String {
    if let Some((sym, offset)) = find_symbol_nearest(addr) {
        let module = sym.module.as_deref().unwrap_or("kernel");
        if offset == 0 {
            alloc::format!("{}+0x0 [{}]", sym.name, module)
        } else {
            alloc::format!("{}+0x{:x} [{}]", sym.name, offset, module)
        }
    } else {
        alloc::format!("0x{:x}", addr)
    }
}

/// Print symbol table (for debugging)
pub fn print_symbols() {
    let table = SYMBOLS.read();

    crate::kprintln!("Symbol table ({} entries):", table.len());
    for sym in table.iter().take(20) {
        crate::kprintln!("  {:016x} {} [{:?}]",
            sym.address, sym.name, sym.sym_type);
    }

    if table.len() > 20 {
        crate::kprintln!("  ... and {} more", table.len() - 20);
    }
}

/// Procfs-style kallsyms output
pub fn proc_kallsyms() -> String {
    let table = SYMBOLS.read();
    let mut output = String::new();

    for sym in table.iter() {
        let type_char = match sym.sym_type {
            SymbolType::Function => 'T',
            SymbolType::Object => 'D',
            SymbolType::Section => 'S',
            _ => '?',
        };

        let module = sym.module.as_deref().unwrap_or("");
        let module_str = if module.is_empty() {
            String::new()
        } else {
            alloc::format!(" [{}]", module)
        };

        output.push_str(&alloc::format!(
            "{:016x} {} {}{}\n",
            sym.address, type_char, sym.name, module_str
        ));
    }

    output
}

/// CRC for symbol versioning
pub fn symbol_crc(name: &str) -> u32 {
    // Simple CRC32 for symbol versioning
    let mut crc = 0xFFFFFFFFu32;

    for byte in name.bytes() {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }

    !crc
}

/// Symbol version entry
#[derive(Clone, Debug)]
pub struct SymbolVersion {
    pub name: String,
    pub crc: u32,
    pub module: Option<String>,
}

/// Verify symbol versions
pub fn verify_versions(versions: &[SymbolVersion]) -> Result<(), super::ModuleError> {
    let table = SYMBOLS.read();

    for ver in versions {
        if let Some(_sym) = table.find(&ver.name) {
            let expected_crc = symbol_crc(&ver.name);
            if ver.crc != expected_crc && ver.crc != 0 {
                return Err(super::ModuleError::VersionMismatch);
            }
        } else {
            return Err(super::ModuleError::MissingSymbol(ver.name.clone()));
        }
    }

    Ok(())
}
