// ===============================================================================
// BUILDOS KERNEL - KERNEL MODULE SUBSYSTEM
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Kernel Module Subsystem
//!
//! Provides dynamic loading and unloading of kernel modules:
//! - ELF module parsing and relocation
//! - Symbol resolution and export
//! - Module dependencies
//! - Module parameters
//! - Lifecycle management (init/exit)

pub mod elf;
pub mod symbol;
pub mod loader;
pub mod params;

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::sync::RwLock;

/// Module subsystem initialized flag
static MODULE_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Loaded modules
static MODULES: RwLock<BTreeMap<String, Module>> = RwLock::new(BTreeMap::new());

/// Module ID counter
static MODULE_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

/// Module states
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleState {
    /// Module is unloaded
    Unloaded,
    /// Module is being loaded
    Loading,
    /// Module is live
    Live,
    /// Module is being unloaded
    Unloading,
}

/// A loaded kernel module
pub struct Module {
    /// Module ID
    pub id: u32,
    /// Module name
    pub name: String,
    /// Module version
    pub version: String,
    /// Module description
    pub description: String,
    /// Module author
    pub author: String,
    /// Module license
    pub license: String,
    /// Module state
    pub state: ModuleState,
    /// Memory region where module code is loaded
    pub mem: ModuleMemory,
    /// Exported symbols
    pub exports: Vec<Symbol>,
    /// Required symbols (from other modules)
    pub requires: Vec<String>,
    /// Dependent modules (modules that depend on this one)
    pub dependents: Vec<String>,
    /// Module dependencies (modules this one depends on)
    pub dependencies: Vec<String>,
    /// Module parameters
    pub params: Vec<ModuleParam>,
    /// Reference count
    pub refcount: AtomicU32,
    /// Init function pointer
    init_fn: Option<ModuleInitFn>,
    /// Exit function pointer
    exit_fn: Option<ModuleExitFn>,
}

/// Module memory layout
#[derive(Clone, Debug)]
pub struct ModuleMemory {
    /// Code section base address
    pub code_base: usize,
    /// Code section size
    pub code_size: usize,
    /// Data section base address
    pub data_base: usize,
    /// Data section size
    pub data_size: usize,
    /// BSS section base address
    pub bss_base: usize,
    /// BSS section size
    pub bss_size: usize,
    /// Read-only data base address
    pub rodata_base: usize,
    /// Read-only data size
    pub rodata_size: usize,
    /// Total allocation
    pub total_size: usize,
}

impl Default for ModuleMemory {
    fn default() -> Self {
        Self {
            code_base: 0,
            code_size: 0,
            data_base: 0,
            data_size: 0,
            bss_base: 0,
            bss_size: 0,
            rodata_base: 0,
            rodata_size: 0,
            total_size: 0,
        }
    }
}

/// A kernel symbol
#[derive(Clone, Debug)]
pub struct Symbol {
    /// Symbol name
    pub name: String,
    /// Symbol address
    pub address: usize,
    /// Symbol size
    pub size: usize,
    /// Symbol type
    pub sym_type: SymbolType,
    /// Visibility
    pub visibility: SymbolVisibility,
    /// Owning module (None for kernel symbols)
    pub module: Option<String>,
}

/// Symbol types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolType {
    /// Function
    Function,
    /// Data object
    Object,
    /// Section
    Section,
    /// File
    File,
    /// Common
    Common,
    /// Thread-local
    Tls,
    /// Unknown
    Unknown,
}

/// Symbol visibility
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolVisibility {
    /// Default visibility (exported)
    Default,
    /// Hidden (not exported)
    Hidden,
    /// Protected
    Protected,
    /// Internal
    Internal,
}

/// Module parameter
#[derive(Clone, Debug)]
pub struct ModuleParam {
    /// Parameter name
    pub name: String,
    /// Parameter type
    pub param_type: ParamType,
    /// Default value
    pub default: String,
    /// Current value
    pub value: String,
    /// Description
    pub description: String,
    /// Permissions (for sysfs)
    pub perm: u16,
}

/// Parameter types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamType {
    Bool,
    Int,
    UInt,
    Long,
    ULong,
    String,
    CharPtr,
    ByteArray,
}

/// Module init function type
pub type ModuleInitFn = fn() -> i32;

/// Module exit function type
pub type ModuleExitFn = fn();

/// Module error types
#[derive(Clone, Debug)]
pub enum ModuleError {
    /// Module not found
    NotFound,
    /// Module already loaded
    AlreadyLoaded,
    /// Module is in use
    InUse,
    /// Invalid module format
    InvalidFormat,
    /// Missing symbol
    MissingSymbol(String),
    /// Missing dependency
    MissingDependency(String),
    /// Invalid parameter
    InvalidParameter(String),
    /// Init failed
    InitFailed(i32),
    /// Out of memory
    OutOfMemory,
    /// Permission denied
    PermissionDenied,
    /// Version mismatch
    VersionMismatch,
    /// Circular dependency
    CircularDependency,
}

impl ModuleError {
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::NotFound => -2,           // ENOENT
            Self::AlreadyLoaded => -17,     // EEXIST
            Self::InUse => -16,             // EBUSY
            Self::InvalidFormat => -8,      // ENOEXEC
            Self::MissingSymbol(_) => -2,   // ENOENT
            Self::MissingDependency(_) => -2,
            Self::InvalidParameter(_) => -22, // EINVAL
            Self::InitFailed(_) => -5,      // EIO
            Self::OutOfMemory => -12,       // ENOMEM
            Self::PermissionDenied => -1,   // EPERM
            Self::VersionMismatch => -22,
            Self::CircularDependency => -22,
        }
    }
}

/// Initialize the module subsystem
pub fn init() {
    // Initialize symbol table with kernel exports
    symbol::init_kernel_symbols();

    MODULE_INITIALIZED.store(true, Ordering::Release);
    crate::kprintln!("[MODULE] Kernel module subsystem initialized");
}

/// Load a module from bytes
pub fn load_module(name: &str, data: &[u8]) -> Result<u32, ModuleError> {
    if !MODULE_INITIALIZED.load(Ordering::Acquire) {
        return Err(ModuleError::InvalidFormat);
    }

    // Check if already loaded
    if MODULES.read().contains_key(name) {
        return Err(ModuleError::AlreadyLoaded);
    }

    // Parse ELF and create module
    let module = loader::load_from_bytes(name, data)?;
    let id = module.id;

    // Insert module
    MODULES.write().insert(name.to_string(), module);

    // Initialize module
    if let Err(e) = init_module(name) {
        // Rollback
        MODULES.write().remove(name);
        return Err(e);
    }

    crate::kprintln!("[MODULE] Loaded module '{}' (id={})", name, id);

    Ok(id)
}

/// Initialize a loaded module
fn init_module(name: &str) -> Result<(), ModuleError> {
    let modules = MODULES.read();
    let module = modules.get(name).ok_or(ModuleError::NotFound)?;

    // Check dependencies
    for dep in &module.dependencies {
        if !modules.contains_key(dep) {
            return Err(ModuleError::MissingDependency(dep.clone()));
        }
    }

    // Call init function
    if let Some(init_fn) = module.init_fn {
        let result = init_fn();
        if result != 0 {
            return Err(ModuleError::InitFailed(result));
        }
    }

    drop(modules);

    // Update state
    let mut modules = MODULES.write();
    if let Some(module) = modules.get_mut(name) {
        module.state = ModuleState::Live;
    }

    Ok(())
}

/// Unload a module
pub fn unload_module(name: &str) -> Result<(), ModuleError> {
    let modules = MODULES.read();
    let module = modules.get(name).ok_or(ModuleError::NotFound)?;

    // Check refcount
    if module.refcount.load(Ordering::Relaxed) > 0 {
        return Err(ModuleError::InUse);
    }

    // Check if other modules depend on this
    if !module.dependents.is_empty() {
        return Err(ModuleError::InUse);
    }

    drop(modules);

    // Update state
    {
        let mut modules = MODULES.write();
        if let Some(module) = modules.get_mut(name) {
            module.state = ModuleState::Unloading;
        }
    }

    // Call exit function
    {
        let modules = MODULES.read();
        if let Some(module) = modules.get(name) {
            if let Some(exit_fn) = module.exit_fn {
                exit_fn();
            }
        }
    }

    // Remove module
    let mut modules = MODULES.write();
    if let Some(module) = modules.remove(name) {
        // Free module memory
        loader::free_module_memory(&module);

        // Remove from symbol table
        symbol::remove_module_symbols(name);

        crate::kprintln!("[MODULE] Unloaded module '{}'", name);
    }

    Ok(())
}

/// Get module info
pub fn get_module_info(name: &str) -> Option<ModuleInfo> {
    let modules = MODULES.read();
    modules.get(name).map(|m| ModuleInfo {
        id: m.id,
        name: m.name.clone(),
        version: m.version.clone(),
        description: m.description.clone(),
        author: m.author.clone(),
        license: m.license.clone(),
        state: m.state,
        size: m.mem.total_size,
        refcount: m.refcount.load(Ordering::Relaxed),
        dependencies: m.dependencies.clone(),
    })
}

/// Module info (simplified for queries)
#[derive(Clone, Debug)]
pub struct ModuleInfo {
    pub id: u32,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub license: String,
    pub state: ModuleState,
    pub size: usize,
    pub refcount: u32,
    pub dependencies: Vec<String>,
}

/// List all loaded modules
pub fn list_modules() -> Vec<ModuleInfo> {
    MODULES.read()
        .values()
        .map(|m| ModuleInfo {
            id: m.id,
            name: m.name.clone(),
            version: m.version.clone(),
            description: m.description.clone(),
            author: m.author.clone(),
            license: m.license.clone(),
            state: m.state,
            size: m.mem.total_size,
            refcount: m.refcount.load(Ordering::Relaxed),
            dependencies: m.dependencies.clone(),
        })
        .collect()
}

/// Find a symbol by name
pub fn find_symbol(name: &str) -> Option<Symbol> {
    symbol::find_symbol(name)
}

/// Request a module (loads if not loaded, increments refcount)
pub fn request_module(name: &str) -> Result<(), ModuleError> {
    if let Some(module) = MODULES.read().get(name) {
        module.refcount.fetch_add(1, Ordering::AcqRel);
        return Ok(());
    }

    // Try to load from filesystem
    let path = alloc::format!("/lib/modules/{}.ko", name);
    if let Ok(data) = crate::fs::read_file(&path) {
        load_module(name, &data)?;
        if let Some(module) = MODULES.read().get(name) {
            module.refcount.fetch_add(1, Ordering::AcqRel);
        }
        Ok(())
    } else {
        Err(ModuleError::NotFound)
    }
}

/// Release a module reference
pub fn release_module(name: &str) {
    if let Some(module) = MODULES.read().get(name) {
        module.refcount.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Set a module parameter
pub fn set_param(module_name: &str, param_name: &str, value: &str) -> Result<(), ModuleError> {
    let mut modules = MODULES.write();
    let module = modules.get_mut(module_name)
        .ok_or(ModuleError::NotFound)?;

    let param = module.params.iter_mut()
        .find(|p| p.name == param_name)
        .ok_or(ModuleError::InvalidParameter(param_name.to_string()))?;

    param.value = value.to_string();

    // Would need to call param update callback here
    Ok(())
}

/// Get a module parameter
pub fn get_param(module_name: &str, param_name: &str) -> Result<String, ModuleError> {
    let modules = MODULES.read();
    let module = modules.get(module_name)
        .ok_or(ModuleError::NotFound)?;

    let param = module.params.iter()
        .find(|p| p.name == param_name)
        .ok_or(ModuleError::InvalidParameter(param_name.to_string()))?;

    Ok(param.value.clone())
}

/// Module metadata section
#[repr(C)]
pub struct ModuleMetadata {
    /// Module name
    pub name: [u8; 56],
    /// Module version
    pub version: [u8; 24],
    /// Init function address (relative)
    pub init: u64,
    /// Exit function address (relative)
    pub exit: u64,
    /// License string offset
    pub license: u32,
    /// Description string offset
    pub description: u32,
    /// Author string offset
    pub author: u32,
    /// Dependencies string offset
    pub depends: u32,
}

/// Generate unique module ID
pub fn next_module_id() -> u32 {
    MODULE_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Check if a module is loaded
pub fn is_loaded(name: &str) -> bool {
    MODULES.read().contains_key(name)
}

/// Get module count
pub fn module_count() -> usize {
    MODULES.read().len()
}

/// Procfs-style module listing
pub fn proc_modules() -> String {
    let mut output = String::new();
    let modules = MODULES.read();

    for module in modules.values() {
        let size_kb = (module.mem.total_size + 1023) / 1024;
        let deps = if module.dependencies.is_empty() {
            "-".to_string()
        } else {
            module.dependencies.join(",")
        };

        output.push_str(&alloc::format!(
            "{} {} {} {} {} 0x{:x}\n",
            module.name,
            size_kb,
            module.refcount.load(Ordering::Relaxed),
            deps,
            match module.state {
                ModuleState::Live => "Live",
                ModuleState::Loading => "Loading",
                ModuleState::Unloading => "Unloading",
                ModuleState::Unloaded => "Unloaded",
            },
            module.mem.code_base
        ));
    }

    output
}
