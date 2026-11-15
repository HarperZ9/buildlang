//! User-space Probes (Uprobes)
//!
//! Dynamic instrumentation for user-space functions.

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use crate::sync::{RwLock, Mutex};

/// Uprobe handler function type
pub type UprobeHandler = fn(regs: &mut UprobeRegs, pid: u32) -> i32;

/// Uretprobe handler function type
pub type UretprobeHandler = fn(regs: &mut UprobeRegs, ret_value: u64, pid: u32) -> i32;

/// Uprobe register state (similar to Kprobe but for user space)
#[repr(C)]
#[derive(Clone, Debug, Default)]
pub struct UprobeRegs {
    /// R15
    pub r15: u64,
    /// R14
    pub r14: u64,
    /// R13
    pub r13: u64,
    /// R12
    pub r12: u64,
    /// RBP
    pub rbp: u64,
    /// RBX
    pub rbx: u64,
    /// R11
    pub r11: u64,
    /// R10
    pub r10: u64,
    /// R9
    pub r9: u64,
    /// R8
    pub r8: u64,
    /// RAX
    pub rax: u64,
    /// RCX
    pub rcx: u64,
    /// RDX
    pub rdx: u64,
    /// RSI
    pub rsi: u64,
    /// RDI
    pub rdi: u64,
    /// Original RIP
    pub rip: u64,
    /// CS
    pub cs: u64,
    /// RFLAGS
    pub rflags: u64,
    /// RSP
    pub rsp: u64,
    /// SS
    pub ss: u64,
}

impl UprobeRegs {
    /// Get argument by index (System V AMD64 ABI)
    pub fn arg(&self, n: usize) -> u64 {
        match n {
            0 => self.rdi,
            1 => self.rsi,
            2 => self.rdx,
            3 => self.rcx,
            4 => self.r8,
            5 => self.r9,
            _ => 0, // Would need to read from user stack
        }
    }

    /// Get return value
    pub fn ret_value(&self) -> u64 {
        self.rax
    }

    /// Set return value
    pub fn set_ret_value(&mut self, value: u64) {
        self.rax = value;
    }
}

/// Uprobe state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UprobeState {
    /// Uprobe is disabled
    Disabled,
    /// Uprobe is active
    Active,
    /// Uprobe hit, executing handler
    Running,
}

/// Uprobe definition
pub struct Uprobe {
    /// Unique ID
    pub id: u64,
    /// Target inode (file backing)
    pub inode: u64,
    /// Offset in file
    pub offset: u64,
    /// Reference count
    pub ref_count: AtomicU64,
    /// Original instruction bytes
    pub insn: [u8; 16],
    /// Instruction length
    pub insn_len: u8,
    /// State
    state: AtomicU64,
    /// Pre-handler
    pub handler: Option<UprobeHandler>,
    /// Return handler
    pub ret_handler: Option<UretprobeHandler>,
    /// Hit count
    hits: AtomicU64,
}

impl Uprobe {
    /// Create a new uprobe
    pub fn new(id: u64, inode: u64, offset: u64) -> Self {
        Self {
            id,
            inode,
            offset,
            ref_count: AtomicU64::new(1),
            insn: [0; 16],
            insn_len: 0,
            state: AtomicU64::new(UprobeState::Disabled as u64),
            handler: None,
            ret_handler: None,
            hits: AtomicU64::new(0),
        }
    }

    /// Set handler
    pub fn with_handler(mut self, handler: UprobeHandler) -> Self {
        self.handler = Some(handler);
        self
    }

    /// Set return handler
    pub fn with_ret_handler(mut self, handler: UretprobeHandler) -> Self {
        self.ret_handler = Some(handler);
        self
    }

    /// Enable the uprobe
    pub fn enable(&self) -> Result<(), UprobeError> {
        self.state.store(UprobeState::Active as u64, Ordering::Release);
        Ok(())
    }

    /// Disable the uprobe
    pub fn disable(&self) -> Result<(), UprobeError> {
        self.state.store(UprobeState::Disabled as u64, Ordering::Release);
        Ok(())
    }

    /// Get state
    pub fn state(&self) -> UprobeState {
        match self.state.load(Ordering::Acquire) {
            0 => UprobeState::Disabled,
            1 => UprobeState::Active,
            2 => UprobeState::Running,
            _ => UprobeState::Disabled,
        }
    }

    /// Handle probe hit
    pub fn handle(&self, regs: &mut UprobeRegs, pid: u32) -> i32 {
        self.hits.fetch_add(1, Ordering::Relaxed);

        if let Some(handler) = self.handler {
            handler(regs, pid)
        } else {
            0
        }
    }

    /// Handle return probe
    pub fn handle_return(&self, regs: &mut UprobeRegs, ret_value: u64, pid: u32) -> i32 {
        if let Some(handler) = self.ret_handler {
            handler(regs, ret_value, pid)
        } else {
            0
        }
    }

    /// Get hit count
    pub fn hit_count(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }
}

/// Uprobe error types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UprobeError {
    /// Invalid file or inode
    InvalidFile,
    /// Invalid offset
    InvalidOffset,
    /// Cannot probe this location
    NotProbeable,
    /// Already probed
    AlreadyProbed,
    /// Out of resources
    NoResources,
    /// Permission denied
    PermissionDenied,
    /// Invalid state
    InvalidState,
    /// File not found
    FileNotFound,
}

/// Uprobe registry key (inode, offset)
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct UprobeKey {
    inode: u64,
    offset: u64,
}

/// Uprobe registry
pub struct UprobeRegistry {
    /// Registered uprobes by ID
    probes: RwLock<BTreeMap<u64, Arc<Uprobe>>>,
    /// Probes by (inode, offset)
    by_location: RwLock<BTreeMap<UprobeKey, u64>>,
    /// Next ID
    next_id: AtomicU64,
    /// Enabled flag
    enabled: AtomicBool,
}

impl UprobeRegistry {
    /// Create new registry
    pub const fn new() -> Self {
        Self {
            probes: RwLock::new(BTreeMap::new()),
            by_location: RwLock::new(BTreeMap::new()),
            next_id: AtomicU64::new(1),
            enabled: AtomicBool::new(true),
        }
    }

    /// Register a uprobe by path and offset
    pub fn register(&self, path: &str, offset: u64, handler: UprobeHandler) -> Result<u64, UprobeError> {
        // Get inode for path
        let inode = path_to_inode(path).ok_or(UprobeError::FileNotFound)?;

        let key = UprobeKey { inode, offset };

        // Check if already probed
        if self.by_location.read().contains_key(&key) {
            return Err(UprobeError::AlreadyProbed);
        }

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let uprobe = Arc::new(Uprobe::new(id, inode, offset).with_handler(handler));

        // Install probe (would write int3 to all mapped instances)
        install_uprobe(inode, offset)?;

        // Register
        self.probes.write().insert(id, uprobe);
        self.by_location.write().insert(key, id);

        Ok(id)
    }

    /// Register with return handler
    pub fn register_ret(&self, path: &str, offset: u64, handler: UprobeHandler, ret_handler: UretprobeHandler) -> Result<u64, UprobeError> {
        let inode = path_to_inode(path).ok_or(UprobeError::FileNotFound)?;

        let key = UprobeKey { inode, offset };

        if self.by_location.read().contains_key(&key) {
            return Err(UprobeError::AlreadyProbed);
        }

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let uprobe = Arc::new(
            Uprobe::new(id, inode, offset)
                .with_handler(handler)
                .with_ret_handler(ret_handler)
        );

        install_uprobe(inode, offset)?;

        self.probes.write().insert(id, uprobe);
        self.by_location.write().insert(key, id);

        Ok(id)
    }

    /// Unregister a uprobe
    pub fn unregister(&self, id: u64) -> Result<(), UprobeError> {
        let probe = self.probes.write().remove(&id)
            .ok_or(UprobeError::InvalidState)?;

        let key = UprobeKey { inode: probe.inode, offset: probe.offset };
        self.by_location.write().remove(&key);

        // Uninstall probe
        uninstall_uprobe(probe.inode, probe.offset)?;

        Ok(())
    }

    /// Get uprobe by location
    pub fn get_by_location(&self, inode: u64, offset: u64) -> Option<Arc<Uprobe>> {
        let key = UprobeKey { inode, offset };
        let id = *self.by_location.read().get(&key)?;
        self.probes.read().get(&id).cloned()
    }

    /// Handle uprobe hit
    pub fn handle_probe(&self, inode: u64, offset: u64, regs: &mut UprobeRegs, pid: u32) -> i32 {
        if !self.enabled.load(Ordering::Relaxed) {
            return 0;
        }

        if let Some(probe) = self.get_by_location(inode, offset) {
            probe.handle(regs, pid)
        } else {
            0
        }
    }

    /// List all uprobes
    pub fn list(&self) -> Vec<(u64, u64, u64)> {
        self.probes.read()
            .iter()
            .map(|(id, p)| (*id, p.inode, p.offset))
            .collect()
    }
}

/// Global uprobe registry
static REGISTRY: Mutex<Option<UprobeRegistry>> = Mutex::new(None);

/// Initialize uprobe subsystem
pub fn init() {
    *REGISTRY.lock() = Some(UprobeRegistry::new());
    crate::kprintln!("[UPROBE] User-space probes initialized");
}

/// Register a uprobe
pub fn register(path: &str, offset: u64, handler: UprobeHandler) -> Result<u64, UprobeError> {
    REGISTRY.lock()
        .as_ref()
        .ok_or(UprobeError::InvalidState)?
        .register(path, offset, handler)
}

/// Unregister a uprobe
pub fn unregister(id: u64) -> Result<(), UprobeError> {
    REGISTRY.lock()
        .as_ref()
        .ok_or(UprobeError::InvalidState)?
        .unregister(id)
}

/// Handle uprobe hit (called from int3 handler in user context)
pub fn handle_breakpoint(inode: u64, offset: u64, regs: &mut UprobeRegs, pid: u32) -> bool {
    if let Some(registry) = &*REGISTRY.lock() {
        if registry.get_by_location(inode, offset).is_some() {
            registry.handle_probe(inode, offset, regs, pid);
            return true;
        }
    }
    false
}

/// Get inode for path
fn path_to_inode(_path: &str) -> Option<u64> {
    // Would look up inode through VFS
    Some(0) // Placeholder
}

/// Install uprobe (write int3 to mapped pages)
fn install_uprobe(_inode: u64, _offset: u64) -> Result<(), UprobeError> {
    // Would:
    // 1. Find all processes with this file mapped
    // 2. For each mapping, write int3 at the appropriate address
    // 3. Add to XOL (execute out of line) area for single-stepping
    Ok(())
}

/// Uninstall uprobe
fn uninstall_uprobe(_inode: u64, _offset: u64) -> Result<(), UprobeError> {
    // Would restore original instruction in all mapped instances
    Ok(())
}

/// Per-task uprobe state
pub struct UprobeTaskState {
    /// Active uprobes in this task
    pub active_uprobes: Vec<u64>,
    /// Currently in single-step
    pub in_ss: bool,
    /// Return probe instance
    pub return_instance: Option<UprobeReturnInstance>,
}

/// Return probe instance for tracking return address
pub struct UprobeReturnInstance {
    /// Original return address
    pub orig_ret_addr: u64,
    /// Uprobe ID
    pub uprobe_id: u64,
    /// Entry registers snapshot
    pub entry_regs: UprobeRegs,
}

/// MMU notifier for uprobe
/// Called when a page is mapped/unmapped to apply/remove probes
pub fn mmu_notify_map(inode: u64, offset: u64, vaddr: u64, pid: u32) {
    if let Some(registry) = &*REGISTRY.lock() {
        if let Some(probe) = registry.get_by_location(inode, offset) {
            if probe.state() == UprobeState::Active {
                // Would write int3 to vaddr
                let _ = (vaddr, pid);
            }
        }
    }
}

/// Called when page is unmapped
pub fn mmu_notify_unmap(_inode: u64, _offset: u64, _vaddr: u64, _pid: u32) {
    // Nothing to do, int3 is removed with the page
}

/// Probe a library function by name
pub fn probe_library(library: &str, symbol: &str, handler: UprobeHandler) -> Result<u64, UprobeError> {
    // Would:
    // 1. Find library path
    // 2. Parse ELF to find symbol offset
    // 3. Register uprobe at that offset
    let _ = (library, symbol, handler);
    Err(UprobeError::FileNotFound)
}
