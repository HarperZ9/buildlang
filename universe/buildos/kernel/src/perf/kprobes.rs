//! Kernel Probes (Kprobes)
//!
//! Dynamic instrumentation for kernel functions.

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use crate::sync::{RwLock, Mutex};

/// Kprobe handler function type
pub type KprobeHandler = fn(regs: &mut KprobeRegs) -> i32;

/// Return probe handler function type
pub type KretprobeHandler = fn(regs: &mut KprobeRegs, ret_value: u64) -> i32;

/// Kprobe register state
#[repr(C)]
#[derive(Clone, Debug, Default)]
pub struct KprobeRegs {
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

impl KprobeRegs {
    /// Get argument by index (System V AMD64 ABI)
    pub fn arg(&self, n: usize) -> u64 {
        match n {
            0 => self.rdi,
            1 => self.rsi,
            2 => self.rdx,
            3 => self.rcx,
            4 => self.r8,
            5 => self.r9,
            _ => {
                // Stack arguments
                let offset = (n - 6) * 8 + 8; // +8 for return address
                unsafe {
                    *((self.rsp + offset as u64) as *const u64)
                }
            }
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

    /// Get instruction pointer
    pub fn ip(&self) -> u64 {
        self.rip
    }

    /// Get stack pointer
    pub fn sp(&self) -> u64 {
        self.rsp
    }
}

/// Kprobe state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KprobeState {
    /// Kprobe is disabled
    Disabled,
    /// Kprobe is active
    Active,
    /// Kprobe hit, executing handler
    Running,
}

/// Kprobe definition
pub struct Kprobe {
    /// Unique ID
    pub id: u64,
    /// Symbol name (if any)
    pub symbol: Option<String>,
    /// Probe address
    pub addr: u64,
    /// Offset from symbol
    pub offset: u64,
    /// Original instruction byte
    pub opcode: u8,
    /// State
    state: AtomicU64, // Encoded KprobeState
    /// Pre-handler
    pub pre_handler: Option<KprobeHandler>,
    /// Post-handler
    pub post_handler: Option<KprobeHandler>,
    /// Hit count
    hits: AtomicU64,
    /// Miss count (handler returned non-zero)
    misses: AtomicU64,
}

impl Kprobe {
    /// Create a new kprobe
    pub fn new(id: u64, addr: u64) -> Self {
        Self {
            id,
            symbol: None,
            addr,
            offset: 0,
            opcode: 0,
            state: AtomicU64::new(KprobeState::Disabled as u64),
            pre_handler: None,
            post_handler: None,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// Create kprobe at symbol + offset
    pub fn at_symbol(id: u64, symbol: &str, offset: u64) -> Self {
        Self {
            id,
            symbol: Some(String::from(symbol)),
            addr: 0, // Will be resolved
            offset,
            opcode: 0,
            state: AtomicU64::new(KprobeState::Disabled as u64),
            pre_handler: None,
            post_handler: None,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// Set pre-handler
    pub fn with_pre_handler(mut self, handler: KprobeHandler) -> Self {
        self.pre_handler = Some(handler);
        self
    }

    /// Set post-handler
    pub fn with_post_handler(mut self, handler: KprobeHandler) -> Self {
        self.post_handler = Some(handler);
        self
    }

    /// Enable the kprobe
    pub fn enable(&self) -> Result<(), KprobeError> {
        self.state.store(KprobeState::Active as u64, Ordering::Release);
        Ok(())
    }

    /// Disable the kprobe
    pub fn disable(&self) -> Result<(), KprobeError> {
        self.state.store(KprobeState::Disabled as u64, Ordering::Release);
        Ok(())
    }

    /// Get state
    pub fn state(&self) -> KprobeState {
        match self.state.load(Ordering::Acquire) {
            0 => KprobeState::Disabled,
            1 => KprobeState::Active,
            2 => KprobeState::Running,
            _ => KprobeState::Disabled,
        }
    }

    /// Handle probe hit
    pub fn handle(&self, regs: &mut KprobeRegs) -> i32 {
        self.hits.fetch_add(1, Ordering::Relaxed);

        let mut result = 0;

        // Run pre-handler
        if let Some(handler) = self.pre_handler {
            result = handler(regs);
            if result != 0 {
                self.misses.fetch_add(1, Ordering::Relaxed);
                return result;
            }
        }

        // Run post-handler
        if let Some(handler) = self.post_handler {
            result = handler(regs);
            if result != 0 {
                self.misses.fetch_add(1, Ordering::Relaxed);
            }
        }

        result
    }

    /// Get hit count
    pub fn hit_count(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    /// Get miss count
    pub fn miss_count(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }
}

/// Kretprobe (return probe) definition
pub struct Kretprobe {
    /// Base kprobe
    pub kprobe: Kprobe,
    /// Entry handler
    pub entry_handler: Option<KprobeHandler>,
    /// Return handler
    pub ret_handler: Option<KretprobeHandler>,
    /// Max active instances
    pub maxactive: usize,
    /// Current active count
    active: AtomicU64,
    /// Missed probes due to maxactive
    nmissed: AtomicU64,
}

impl Kretprobe {
    /// Create new return probe
    pub fn new(id: u64, addr: u64, maxactive: usize) -> Self {
        Self {
            kprobe: Kprobe::new(id, addr),
            entry_handler: None,
            ret_handler: None,
            maxactive,
            active: AtomicU64::new(0),
            nmissed: AtomicU64::new(0),
        }
    }

    /// Set entry handler
    pub fn with_entry_handler(mut self, handler: KprobeHandler) -> Self {
        self.entry_handler = Some(handler);
        self
    }

    /// Set return handler
    pub fn with_ret_handler(mut self, handler: KretprobeHandler) -> Self {
        self.ret_handler = Some(handler);
        self
    }

    /// Try to acquire an instance
    pub fn acquire(&self) -> bool {
        let current = self.active.load(Ordering::Relaxed);
        if current >= self.maxactive as u64 {
            self.nmissed.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        self.active.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// Release an instance
    pub fn release(&self) {
        self.active.fetch_sub(1, Ordering::Relaxed);
    }

    /// Get missed count
    pub fn nmissed(&self) -> u64 {
        self.nmissed.load(Ordering::Relaxed)
    }
}

/// Kprobe error types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KprobeError {
    /// Invalid address
    InvalidAddress,
    /// Symbol not found
    SymbolNotFound,
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
}

/// Kprobe registry
pub struct KprobeRegistry {
    /// Registered kprobes by ID
    probes: RwLock<BTreeMap<u64, Arc<Kprobe>>>,
    /// Probes by address
    by_addr: RwLock<BTreeMap<u64, u64>>, // addr -> id
    /// Return probes by ID
    retprobes: RwLock<BTreeMap<u64, Arc<Kretprobe>>>,
    /// Next ID
    next_id: AtomicU64,
    /// Enabled flag
    enabled: AtomicBool,
}

impl KprobeRegistry {
    /// Create new registry
    pub const fn new() -> Self {
        Self {
            probes: RwLock::new(BTreeMap::new()),
            by_addr: RwLock::new(BTreeMap::new()),
            retprobes: RwLock::new(BTreeMap::new()),
            next_id: AtomicU64::new(1),
            enabled: AtomicBool::new(true),
        }
    }

    /// Register a kprobe
    pub fn register(&self, addr: u64, handler: KprobeHandler) -> Result<u64, KprobeError> {
        // Check if address is already probed
        if self.by_addr.read().contains_key(&addr) {
            return Err(KprobeError::AlreadyProbed);
        }

        // Validate address
        if !is_probeable(addr) {
            return Err(KprobeError::NotProbeable);
        }

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let kprobe = Arc::new(Kprobe::new(id, addr).with_pre_handler(handler));

        // Insert breakpoint
        install_probe(addr)?;

        // Register
        self.probes.write().insert(id, kprobe);
        self.by_addr.write().insert(addr, id);

        Ok(id)
    }

    /// Register kprobe at symbol
    pub fn register_symbol(&self, symbol: &str, offset: u64, handler: KprobeHandler) -> Result<u64, KprobeError> {
        // Resolve symbol
        let addr = resolve_symbol(symbol).ok_or(KprobeError::SymbolNotFound)?;
        let probe_addr = addr + offset;

        self.register(probe_addr, handler)
    }

    /// Unregister a kprobe
    pub fn unregister(&self, id: u64) -> Result<(), KprobeError> {
        let probe = self.probes.write().remove(&id)
            .ok_or(KprobeError::InvalidState)?;

        // Remove from address map
        self.by_addr.write().remove(&probe.addr);

        // Remove breakpoint
        uninstall_probe(probe.addr)?;

        Ok(())
    }

    /// Get kprobe by address
    pub fn get_by_addr(&self, addr: u64) -> Option<Arc<Kprobe>> {
        let id = *self.by_addr.read().get(&addr)?;
        self.probes.read().get(&id).cloned()
    }

    /// Handle kprobe hit
    pub fn handle_probe(&self, addr: u64, regs: &mut KprobeRegs) -> i32 {
        if !self.enabled.load(Ordering::Relaxed) {
            return 0;
        }

        if let Some(probe) = self.get_by_addr(addr) {
            probe.handle(regs)
        } else {
            0
        }
    }

    /// List all kprobes
    pub fn list(&self) -> Vec<(u64, u64, Option<String>)> {
        self.probes.read()
            .iter()
            .map(|(id, p)| (*id, p.addr, p.symbol.clone()))
            .collect()
    }

    /// Enable all kprobes
    pub fn enable_all(&self) {
        self.enabled.store(true, Ordering::Release);
    }

    /// Disable all kprobes
    pub fn disable_all(&self) {
        self.enabled.store(false, Ordering::Release);
    }
}

/// Global kprobe registry
static REGISTRY: Mutex<Option<KprobeRegistry>> = Mutex::new(None);

/// Initialize kprobe subsystem
pub fn init() {
    *REGISTRY.lock() = Some(KprobeRegistry::new());
    crate::kprintln!("[KPROBE] Kernel probes initialized");
}

/// Register a kprobe
pub fn register(addr: u64, handler: KprobeHandler) -> Result<u64, KprobeError> {
    REGISTRY.lock()
        .as_ref()
        .ok_or(KprobeError::InvalidState)?
        .register(addr, handler)
}

/// Register kprobe at symbol
pub fn register_symbol(symbol: &str, offset: u64, handler: KprobeHandler) -> Result<u64, KprobeError> {
    REGISTRY.lock()
        .as_ref()
        .ok_or(KprobeError::InvalidState)?
        .register_symbol(symbol, offset, handler)
}

/// Unregister a kprobe
pub fn unregister(id: u64) -> Result<(), KprobeError> {
    REGISTRY.lock()
        .as_ref()
        .ok_or(KprobeError::InvalidState)?
        .unregister(id)
}

/// Handle probe hit (called from int3 handler)
pub fn handle_breakpoint(addr: u64, regs: &mut KprobeRegs) -> bool {
    if let Some(registry) = &*REGISTRY.lock() {
        if registry.get_by_addr(addr).is_some() {
            registry.handle_probe(addr, regs);
            return true;
        }
    }
    false
}

/// Check if address is probeable
fn is_probeable(addr: u64) -> bool {
    // Check if in kernel text section
    // Check if not on blacklist (e.g., interrupt handlers, kprobe code itself)
    addr >= 0xFFFF_8000_0000_0000 && addr != 0
}

/// Resolve symbol to address
fn resolve_symbol(_symbol: &str) -> Option<u64> {
    // Would look up in kernel symbol table
    // For now, return None
    None
}

/// Install probe at address (replace with int3)
fn install_probe(addr: u64) -> Result<(), KprobeError> {
    // Save original byte
    let original = unsafe { *(addr as *const u8) };

    // Check if this is a safe location (not in middle of instruction)
    // Write int3 (0xCC) - would need to make page writable temporarily
    let ptr = addr as *mut u8;
    // core::ptr::write_volatile(ptr, 0xCC);
    let _ = ptr;

    let _ = original; // Would save this

    Ok(())
}

/// Uninstall probe at address
fn uninstall_probe(addr: u64) -> Result<(), KprobeError> {
    // Restore original byte - would restore saved original byte
    let ptr = addr as *mut u8;
    let _ = ptr;
    Ok(())
}

/// Blacklisted symbols that cannot be probed
pub const KPROBE_BLACKLIST: &[&str] = &[
    // Kprobe internals
    "kprobe_handler",
    "kretprobe_handler",
    "arch_prepare_kprobe",
    "arch_arm_kprobe",
    "arch_disarm_kprobe",
    // Exception handlers
    "do_int3",
    "do_debug",
    "do_page_fault",
    // Interrupt handlers
    "common_interrupt",
    "interrupt_entry",
    // Scheduler
    "schedule",
    "__schedule",
    "context_switch",
    // Synchronization
    "spin_lock",
    "spin_unlock",
    "mutex_lock",
    "mutex_unlock",
];
