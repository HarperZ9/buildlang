// ===============================================================================
// BUILDOS KERNEL - KPROBES
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Dynamic kernel probing with kprobes and kretprobes.
//!
//! Kprobes allow inserting probe points at any kernel address
//! for debugging and tracing without modifying kernel code.

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::{Mutex, RwLock};

// =============================================================================
// CONSTANTS
// =============================================================================

/// INT3 breakpoint instruction
const INT3_INSN: u8 = 0xCC;

/// Maximum kprobes
pub const MAX_KPROBES: usize = 1024;

/// Maximum kretprobes
pub const MAX_KRETPROBES: usize = 512;

// =============================================================================
// STATE
// =============================================================================

/// Registered kprobes
static KPROBES: RwLock<BTreeMap<u64, KProbe>> = RwLock::new(BTreeMap::new());

/// Registered kretprobes
static KRETPROBES: RwLock<BTreeMap<u64, KRetProbe>> = RwLock::new(BTreeMap::new());

/// Next probe ID
static NEXT_PROBE_ID: AtomicU64 = AtomicU64::new(1);

/// Is kprobe subsystem initialized?
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Kprobe definition
pub struct KProbe {
    /// Unique probe ID
    pub id: u64,

    /// Probe name
    pub name: String,

    /// Address to probe
    pub addr: u64,

    /// Symbol name (if known)
    pub symbol: Option<String>,

    /// Offset within symbol
    pub offset: u32,

    /// Is probe enabled?
    pub enabled: AtomicBool,

    /// Pre-handler (called before instruction)
    pub pre_handler: Option<fn(&mut ProbeContext) -> i32>,

    /// Post-handler (called after instruction)
    pub post_handler: Option<fn(&mut ProbeContext, u64)>,

    /// Fault handler
    pub fault_handler: Option<fn(&mut ProbeContext, i32) -> i32>,

    /// Original instruction bytes
    pub saved_insn: [u8; 16],

    /// Length of saved instruction
    pub saved_insn_len: usize,

    /// Hit count
    pub hit_count: AtomicU64,

    /// Miss count (handler not called)
    pub miss_count: AtomicU64,

    /// Private data
    pub private_data: u64,
}

/// Kretprobe definition (probes function returns)
pub struct KRetProbe {
    /// Base kprobe at function entry
    pub kprobe: KProbe,

    /// Return handler
    pub handler: fn(&mut RetProbeContext) -> i32,

    /// Max active instances
    pub max_active: u32,

    /// Currently active instances
    pub active_count: AtomicU64,

    /// Return instances pool
    pub instances: Mutex<Vec<RetProbeInstance>>,
}

/// Return probe instance (tracks one function invocation)
pub struct RetProbeInstance {
    /// Original return address
    pub ret_addr: u64,

    /// Entry timestamp
    pub entry_stamp: u64,

    /// Task that called the function
    pub task_pid: u32,

    /// Stack pointer at entry
    pub stack: u64,

    /// Private data
    pub data: u64,
}

/// Context passed to kprobe handlers
#[repr(C)]
pub struct ProbeContext {
    /// Saved registers
    pub regs: Registers,

    /// Probe that fired
    pub probe_addr: u64,

    /// Flags
    pub flags: ProbeFlags,
}

/// Context for return probes
#[repr(C)]
pub struct RetProbeContext {
    /// Saved registers at return
    pub regs: Registers,

    /// Original return address
    pub ret_addr: u64,

    /// Entry timestamp
    pub entry_stamp: u64,

    /// Function entry address
    pub func_addr: u64,

    /// Private data from entry
    pub data: u64,
}

/// Saved registers
#[repr(C)]
#[derive(Clone, Default)]
pub struct Registers {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
    pub rflags: u64,
    pub cs: u64,
    pub ss: u64,
}

bitflags::bitflags! {
    /// Probe context flags
    #[derive(Clone, Copy, Debug, Default)]
    pub struct ProbeFlags: u32 {
        /// Single-stepping
        const STEPPING = 1 << 0;
        /// In fault handler
        const IN_FAULT = 1 << 1;
        /// Called from NMI
        const IN_NMI = 1 << 2;
        /// Post handler needed
        const POST_HANDLER = 1 << 3;
    }
}

// =============================================================================
// INTERFACE
// =============================================================================

/// Initialize kprobe subsystem
pub fn init() {
    // Verify CPU supports INT3 handling
    // Set up breakpoint exception handler
    INITIALIZED.store(true, Ordering::Release);
}

/// Register a kprobe
pub fn register_kprobe(
    name: &str,
    addr: u64,
    pre_handler: Option<fn(&mut ProbeContext) -> i32>,
    post_handler: Option<fn(&mut ProbeContext, u64)>,
) -> Option<u64> {
    if !INITIALIZED.load(Ordering::Acquire) {
        return None;
    }

    let id = NEXT_PROBE_ID.fetch_add(1, Ordering::Relaxed);

    let probe = KProbe {
        id,
        name: String::from(name),
        addr,
        symbol: None,
        offset: 0,
        enabled: AtomicBool::new(false),
        pre_handler,
        post_handler,
        fault_handler: None,
        saved_insn: [0u8; 16],
        saved_insn_len: 0,
        hit_count: AtomicU64::new(0),
        miss_count: AtomicU64::new(0),
        private_data: 0,
    };

    KPROBES.write().insert(addr, probe);

    Some(id)
}

/// Register a kprobe by symbol name
pub fn register_kprobe_symbol(
    name: &str,
    symbol: &str,
    offset: u32,
    pre_handler: Option<fn(&mut ProbeContext) -> i32>,
    post_handler: Option<fn(&mut ProbeContext, u64)>,
) -> Option<u64> {
    // Look up symbol address
    let addr = lookup_symbol(symbol)?;

    let id = NEXT_PROBE_ID.fetch_add(1, Ordering::Relaxed);

    let probe = KProbe {
        id,
        name: String::from(name),
        addr: addr + offset as u64,
        symbol: Some(String::from(symbol)),
        offset,
        enabled: AtomicBool::new(false),
        pre_handler,
        post_handler,
        fault_handler: None,
        saved_insn: [0u8; 16],
        saved_insn_len: 0,
        hit_count: AtomicU64::new(0),
        miss_count: AtomicU64::new(0),
        private_data: 0,
    };

    KPROBES.write().insert(addr + offset as u64, probe);

    Some(id)
}

/// Unregister a kprobe
pub fn unregister_kprobe(addr: u64) -> bool {
    let mut probes = KPROBES.write();

    if let Some(probe) = probes.get(&addr) {
        // Disable first
        if probe.enabled.load(Ordering::Acquire) {
            disable_probe(&probe);
        }

        probes.remove(&addr);
        return true;
    }

    false
}

/// Enable a kprobe
pub fn enable_kprobe(addr: u64) -> bool {
    let probes = KPROBES.read();

    if let Some(probe) = probes.get(&addr) {
        enable_probe(probe);
        return true;
    }

    false
}

/// Disable a kprobe
pub fn disable_kprobe(addr: u64) -> bool {
    let probes = KPROBES.read();

    if let Some(probe) = probes.get(&addr) {
        disable_probe(probe);
        return true;
    }

    false
}

/// Enable a probe (internal)
fn enable_probe(probe: &KProbe) {
    if probe.enabled.load(Ordering::Acquire) {
        return;
    }

    // Save original instruction
    // unsafe { save_instruction(probe); }

    // Insert INT3
    // unsafe { insert_breakpoint(probe.addr); }

    probe.enabled.store(true, Ordering::Release);
}

/// Disable a probe (internal)
fn disable_probe(probe: &KProbe) {
    if !probe.enabled.load(Ordering::Acquire) {
        return;
    }

    // Restore original instruction
    // unsafe { restore_instruction(probe); }

    probe.enabled.store(false, Ordering::Release);
}

/// Look up symbol address
fn lookup_symbol(_name: &str) -> Option<u64> {
    // Would look up in kernel symbol table
    None
}

/// Register a kretprobe
pub fn register_kretprobe(
    name: &str,
    addr: u64,
    handler: fn(&mut RetProbeContext) -> i32,
    max_active: u32,
) -> Option<u64> {
    if !INITIALIZED.load(Ordering::Acquire) {
        return None;
    }

    let id = NEXT_PROBE_ID.fetch_add(1, Ordering::Relaxed);

    // Entry kprobe
    let entry_probe = KProbe {
        id,
        name: String::from(name),
        addr,
        symbol: None,
        offset: 0,
        enabled: AtomicBool::new(false),
        pre_handler: Some(kretprobe_entry_handler),
        post_handler: None,
        fault_handler: None,
        saved_insn: [0u8; 16],
        saved_insn_len: 0,
        hit_count: AtomicU64::new(0),
        miss_count: AtomicU64::new(0),
        private_data: 0,
    };

    let rp = KRetProbe {
        kprobe: entry_probe,
        handler,
        max_active,
        active_count: AtomicU64::new(0),
        instances: Mutex::new(Vec::with_capacity(max_active as usize)),
    };

    KRETPROBES.write().insert(addr, rp);

    Some(id)
}

/// Kretprobe entry handler
fn kretprobe_entry_handler(ctx: &mut ProbeContext) -> i32 {
    // Find the kretprobe
    let rps = KRETPROBES.read();
    if let Some(rp) = rps.get(&ctx.probe_addr) {
        // Check if we have available instances
        let active = rp.active_count.load(Ordering::Relaxed);
        if active >= rp.max_active as u64 {
            return 0;
        }

        // Allocate instance
        let instance = RetProbeInstance {
            ret_addr: ctx.regs.rsp, // Would read actual return address
            entry_stamp: crate::time::now_ns(),
            task_pid: crate::process::current_pid().unwrap_or(0) as u32,
            stack: ctx.regs.rsp,
            data: 0,
        };

        rp.instances.lock().push(instance);
        rp.active_count.fetch_add(1, Ordering::Relaxed);

        // Hijack return address to trampoline
        // ctx.regs would be modified to return to our trampoline
    }

    0
}

/// Unregister a kretprobe
pub fn unregister_kretprobe(addr: u64) -> bool {
    let mut rps = KRETPROBES.write();

    if let Some(rp) = rps.get(&addr) {
        disable_probe(&rp.kprobe);
        rps.remove(&addr);
        return true;
    }

    false
}

// =============================================================================
// INT3 HANDLER
// =============================================================================

/// Handle INT3 (breakpoint) exception for kprobes
pub fn handle_int3(ctx: &mut ProbeContext) -> bool {
    let addr = ctx.regs.rip - 1; // INT3 is 1 byte

    let probes = KPROBES.read();
    if let Some(probe) = probes.get(&addr) {
        if !probe.enabled.load(Ordering::Acquire) {
            return false;
        }

        probe.hit_count.fetch_add(1, Ordering::Relaxed);

        // Call pre-handler
        if let Some(handler) = probe.pre_handler {
            handler(ctx);
        }

        // Set up single-step to execute original instruction
        ctx.flags |= ProbeFlags::STEPPING;
        if probe.post_handler.is_some() {
            ctx.flags |= ProbeFlags::POST_HANDLER;
        }

        return true;
    }

    false
}

/// Handle single-step completion for kprobes
pub fn handle_single_step(ctx: &mut ProbeContext) {
    if !ctx.flags.contains(ProbeFlags::STEPPING) {
        return;
    }

    let addr = ctx.probe_addr;

    let probes = KPROBES.read();
    if let Some(probe) = probes.get(&addr) {
        // Call post-handler
        if ctx.flags.contains(ProbeFlags::POST_HANDLER) {
            if let Some(handler) = probe.post_handler {
                handler(ctx, 0);
            }
        }

        // Re-insert breakpoint
        // unsafe { insert_breakpoint(addr); }
    }

    ctx.flags.remove(ProbeFlags::STEPPING);
}

// =============================================================================
// STATISTICS
// =============================================================================

/// Get list of registered kprobes
pub fn list_kprobes() -> Vec<KProbeInfo> {
    KPROBES.read()
        .iter()
        .map(|(addr, probe)| KProbeInfo {
            id: probe.id,
            name: probe.name.clone(),
            addr: *addr,
            symbol: probe.symbol.clone(),
            enabled: probe.enabled.load(Ordering::Relaxed),
            hit_count: probe.hit_count.load(Ordering::Relaxed),
            miss_count: probe.miss_count.load(Ordering::Relaxed),
        })
        .collect()
}

/// Kprobe information
pub struct KProbeInfo {
    pub id: u64,
    pub name: String,
    pub addr: u64,
    pub symbol: Option<String>,
    pub enabled: bool,
    pub hit_count: u64,
    pub miss_count: u64,
}

/// Get number of registered kprobes
pub fn kprobe_count() -> usize {
    KPROBES.read().len()
}

/// Get number of registered kretprobes
pub fn kretprobe_count() -> usize {
    KRETPROBES.read().len()
}
