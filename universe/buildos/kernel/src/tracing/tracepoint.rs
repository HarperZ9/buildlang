// ===============================================================================
// BUILDOS KERNEL - TRACEPOINTS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Static tracepoint infrastructure.
//!
//! Tracepoints are static markers in the kernel code that can have
//! callbacks attached at runtime for tracing and debugging.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::{Mutex, RwLock};

// =============================================================================
// STATE
// =============================================================================

/// Registered tracepoints
static TRACEPOINTS: RwLock<BTreeMap<String, Tracepoint>> = RwLock::new(BTreeMap::new());

/// Next callback ID
static NEXT_CALLBACK_ID: AtomicU64 = AtomicU64::new(1);

/// Tracepoint definition
pub struct Tracepoint {
    /// Tracepoint name (e.g., "sched:sched_switch")
    pub name: String,

    /// Category/subsystem
    pub category: String,

    /// Short description
    pub description: &'static str,

    /// Is tracepoint enabled?
    pub enabled: AtomicBool,

    /// Reference count (number of attached callbacks)
    pub refcount: AtomicU32,

    /// Registered callbacks
    pub callbacks: Mutex<Vec<Callback>>,

    /// Format string for printf-style output
    pub format: &'static str,

    /// Number of times fired
    pub hit_count: AtomicU64,

    /// Tracepoint key (for static branch optimization)
    pub key: AtomicBool,
}

/// Callback registered to a tracepoint
pub struct Callback {
    /// Unique callback ID
    pub id: u64,

    /// Callback function
    pub func: fn(&TracepointArgs),

    /// Callback priority (lower = called first)
    pub priority: i32,

    /// Private data pointer
    pub data: u64,
}

/// Arguments passed to tracepoint callbacks
#[derive(Clone)]
pub struct TracepointArgs {
    /// Tracepoint name
    pub name: String,

    /// Timestamp
    pub timestamp: u64,

    /// CPU ID
    pub cpu: u32,

    /// PID
    pub pid: u32,

    /// Raw argument values
    pub args: [u64; 8],

    /// Number of valid arguments
    pub argc: usize,
}

// =============================================================================
// INTERFACE
// =============================================================================

/// Initialize tracepoint infrastructure
pub fn init() {
    // Register built-in tracepoints
    register_builtin_tracepoints();
}

/// Register built-in tracepoints
fn register_builtin_tracepoints() {
    // Scheduler tracepoints
    register("sched:sched_switch", "sched", "Context switch", "prev_comm=%s prev_pid=%d prev_state=%d => next_comm=%s next_pid=%d");
    register("sched:sched_wakeup", "sched", "Task wakeup", "comm=%s pid=%d target_cpu=%d");
    register("sched:sched_waking", "sched", "Task waking", "comm=%s pid=%d target_cpu=%d");
    register("sched:sched_process_exit", "sched", "Process exit", "comm=%s pid=%d");
    register("sched:sched_process_fork", "sched", "Process fork", "parent_comm=%s parent_pid=%d child_comm=%s child_pid=%d");

    // IRQ tracepoints
    register("irq:irq_handler_entry", "irq", "IRQ handler entry", "irq=%d name=%s");
    register("irq:irq_handler_exit", "irq", "IRQ handler exit", "irq=%d ret=%d");
    register("irq:softirq_entry", "irq", "Softirq entry", "vec=%d");
    register("irq:softirq_exit", "irq", "Softirq exit", "vec=%d");

    // Syscall tracepoints
    register("syscalls:sys_enter", "syscalls", "Syscall entry", "id=%d");
    register("syscalls:sys_exit", "syscalls", "Syscall exit", "id=%d ret=%ld");

    // Memory tracepoints
    register("kmem:kmalloc", "kmem", "kmalloc", "ptr=%p size=%zu gfp=%x");
    register("kmem:kfree", "kmem", "kfree", "ptr=%p");
    register("kmem:mm_page_alloc", "kmem", "Page alloc", "pfn=%lu order=%d");
    register("kmem:mm_page_free", "kmem", "Page free", "pfn=%lu order=%d");

    // Block I/O tracepoints
    register("block:block_rq_issue", "block", "Block request issue", "dev=%d sector=%lu nr_sectors=%u");
    register("block:block_rq_complete", "block", "Block request complete", "dev=%d sector=%lu nr_sectors=%u");

    // Network tracepoints
    register("net:net_dev_xmit", "net", "Network transmit", "dev=%s len=%u");
    register("net:netif_receive_skb", "net", "Network receive", "dev=%s len=%u");

    // Power tracepoints
    register("power:cpu_idle", "power", "CPU idle", "state=%u cpu=%u");
    register("power:cpu_frequency", "power", "CPU frequency", "freq=%u cpu=%u");
}

/// Register a new tracepoint
pub fn register(name: &str, category: &str, description: &'static str, format: &'static str) {
    let tp = Tracepoint {
        name: String::from(name),
        category: String::from(category),
        description,
        enabled: AtomicBool::new(false),
        refcount: AtomicU32::new(0),
        callbacks: Mutex::new(Vec::new()),
        format,
        hit_count: AtomicU64::new(0),
        key: AtomicBool::new(false),
    };

    TRACEPOINTS.write().insert(String::from(name), tp);
}

/// Enable a tracepoint
pub fn enable(name: &str) -> bool {
    if let Some(tp) = TRACEPOINTS.read().get(name) {
        tp.enabled.store(true, Ordering::Release);
        tp.key.store(true, Ordering::Release);
        return true;
    }
    false
}

/// Disable a tracepoint
pub fn disable(name: &str) -> bool {
    if let Some(tp) = TRACEPOINTS.read().get(name) {
        tp.enabled.store(false, Ordering::Release);
        tp.key.store(false, Ordering::Release);
        return true;
    }
    false
}

/// Check if tracepoint is enabled
pub fn is_enabled(name: &str) -> bool {
    TRACEPOINTS.read()
        .get(name)
        .map(|tp| tp.enabled.load(Ordering::Relaxed))
        .unwrap_or(false)
}

/// Attach callback to tracepoint
pub fn attach(name: &str, func: fn(&TracepointArgs), priority: i32) -> Option<u64> {
    let tps = TRACEPOINTS.read();
    if let Some(tp) = tps.get(name) {
        let id = NEXT_CALLBACK_ID.fetch_add(1, Ordering::Relaxed);
        let callback = Callback {
            id,
            func,
            priority,
            data: 0,
        };

        let mut callbacks = tp.callbacks.lock();
        callbacks.push(callback);
        callbacks.sort_by_key(|c| c.priority);

        tp.refcount.fetch_add(1, Ordering::Relaxed);

        // Auto-enable if first callback
        if tp.refcount.load(Ordering::Relaxed) == 1 {
            tp.enabled.store(true, Ordering::Release);
            tp.key.store(true, Ordering::Release);
        }

        return Some(id);
    }
    None
}

/// Detach callback from tracepoint
pub fn detach(name: &str, callback_id: u64) -> bool {
    let tps = TRACEPOINTS.read();
    if let Some(tp) = tps.get(name) {
        let mut callbacks = tp.callbacks.lock();
        let initial_len = callbacks.len();
        callbacks.retain(|c| c.id != callback_id);

        if callbacks.len() < initial_len {
            tp.refcount.fetch_sub(1, Ordering::Relaxed);

            // Auto-disable if no callbacks
            if tp.refcount.load(Ordering::Relaxed) == 0 {
                tp.enabled.store(false, Ordering::Release);
                tp.key.store(false, Ordering::Release);
            }

            return true;
        }
    }
    false
}

/// Fire a tracepoint
pub fn fire(name: &str, args: TracepointArgs) {
    let tps = TRACEPOINTS.read();
    if let Some(tp) = tps.get(name) {
        if !tp.enabled.load(Ordering::Acquire) {
            return;
        }

        tp.hit_count.fetch_add(1, Ordering::Relaxed);

        // Call all registered callbacks
        let callbacks = tp.callbacks.lock();
        for callback in callbacks.iter() {
            (callback.func)(&args);
        }
    }
}

/// Fire tracepoint with raw arguments
pub fn fire_raw(name: &str, args: &[u64]) {
    let tp_args = TracepointArgs {
        name: String::from(name),
        timestamp: crate::time::now_ns(),
        cpu: crate::cpu::current_cpu_id(),
        pid: crate::process::current_pid().unwrap_or(0) as u32,
        args: {
            let mut arr = [0u64; 8];
            for (i, &arg) in args.iter().take(8).enumerate() {
                arr[i] = arg;
            }
            arr
        },
        argc: args.len().min(8),
    };

    fire(name, tp_args);
}

/// Get list of all tracepoints
pub fn list() -> Vec<TracepointInfo> {
    TRACEPOINTS.read()
        .iter()
        .map(|(name, tp)| TracepointInfo {
            name: name.clone(),
            category: tp.category.clone(),
            description: tp.description,
            enabled: tp.enabled.load(Ordering::Relaxed),
            callbacks: tp.refcount.load(Ordering::Relaxed),
            hit_count: tp.hit_count.load(Ordering::Relaxed),
        })
        .collect()
}

/// Get tracepoints by category
pub fn list_by_category(category: &str) -> Vec<TracepointInfo> {
    TRACEPOINTS.read()
        .iter()
        .filter(|(_, tp)| tp.category == category)
        .map(|(name, tp)| TracepointInfo {
            name: name.clone(),
            category: tp.category.clone(),
            description: tp.description,
            enabled: tp.enabled.load(Ordering::Relaxed),
            callbacks: tp.refcount.load(Ordering::Relaxed),
            hit_count: tp.hit_count.load(Ordering::Relaxed),
        })
        .collect()
}

/// Get count of enabled tracepoints
pub fn enabled_count() -> usize {
    TRACEPOINTS.read()
        .values()
        .filter(|tp| tp.enabled.load(Ordering::Relaxed))
        .count()
}

/// Tracepoint information
pub struct TracepointInfo {
    pub name: String,
    pub category: String,
    pub description: &'static str,
    pub enabled: bool,
    pub callbacks: u32,
    pub hit_count: u64,
}

/// Enable all tracepoints in a category
pub fn enable_category(category: &str) -> usize {
    let tps = TRACEPOINTS.read();
    let mut count = 0;

    for (_, tp) in tps.iter() {
        if tp.category == category {
            tp.enabled.store(true, Ordering::Release);
            tp.key.store(true, Ordering::Release);
            count += 1;
        }
    }

    count
}

/// Disable all tracepoints in a category
pub fn disable_category(category: &str) -> usize {
    let tps = TRACEPOINTS.read();
    let mut count = 0;

    for (_, tp) in tps.iter() {
        if tp.category == category {
            tp.enabled.store(false, Ordering::Release);
            tp.key.store(false, Ordering::Release);
            count += 1;
        }
    }

    count
}

/// Enable all tracepoints
pub fn enable_all() {
    for (_, tp) in TRACEPOINTS.read().iter() {
        tp.enabled.store(true, Ordering::Release);
        tp.key.store(true, Ordering::Release);
    }
}

/// Disable all tracepoints
pub fn disable_all() {
    for (_, tp) in TRACEPOINTS.read().iter() {
        tp.enabled.store(false, Ordering::Release);
        tp.key.store(false, Ordering::Release);
    }
}
