//! Kernel Tracepoints
//!
//! Static instrumentation points in the kernel for tracing.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use crate::sync::{RwLock, Mutex};

/// Tracepoint identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TracepointId(pub u64);

/// Tracepoint callback function type
pub type TracepointCallback = fn(data: &[u8]);

/// Tracepoint definition
pub struct Tracepoint {
    /// Unique ID
    pub id: TracepointId,
    /// Subsystem name (e.g., "sched", "syscalls")
    pub subsystem: String,
    /// Event name
    pub name: String,
    /// Full path (subsystem:name)
    pub path: String,
    /// Enabled state
    enabled: AtomicBool,
    /// Registered callbacks
    callbacks: RwLock<Vec<TracepointCallback>>,
    /// Hit count
    hits: AtomicU64,
}

impl Tracepoint {
    /// Create a new tracepoint
    pub fn new(id: TracepointId, subsystem: &str, name: &str) -> Self {
        Self {
            id,
            subsystem: String::from(subsystem),
            name: String::from(name),
            path: alloc::format!("{}:{}", subsystem, name),
            enabled: AtomicBool::new(false),
            callbacks: RwLock::new(Vec::new()),
            hits: AtomicU64::new(0),
        }
    }

    /// Enable the tracepoint
    pub fn enable(&self) {
        self.enabled.store(true, Ordering::Release);
    }

    /// Disable the tracepoint
    pub fn disable(&self) {
        self.enabled.store(false, Ordering::Release);
    }

    /// Check if enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    /// Register a callback
    pub fn register(&self, callback: TracepointCallback) {
        self.callbacks.write().push(callback);
    }

    /// Unregister all callbacks
    pub fn unregister_all(&self) {
        self.callbacks.write().clear();
    }

    /// Fire the tracepoint
    #[inline]
    pub fn fire(&self, data: &[u8]) {
        if self.enabled.load(Ordering::Relaxed) {
            self.hits.fetch_add(1, Ordering::Relaxed);

            let callbacks = self.callbacks.read();
            for callback in callbacks.iter() {
                callback(data);
            }
        }
    }

    /// Get hit count
    pub fn hit_count(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }
}

/// Tracepoint registry
pub struct TracepointRegistry {
    /// All registered tracepoints
    tracepoints: RwLock<BTreeMap<TracepointId, Arc<Tracepoint>>>,
    /// Tracepoints by path
    by_path: RwLock<BTreeMap<String, TracepointId>>,
    /// Next ID
    next_id: AtomicU64,
}

impl TracepointRegistry {
    /// Create new registry
    pub const fn new() -> Self {
        Self {
            tracepoints: RwLock::new(BTreeMap::new()),
            by_path: RwLock::new(BTreeMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    /// Register a tracepoint
    pub fn register(&self, subsystem: &str, name: &str) -> TracepointId {
        let id = TracepointId(self.next_id.fetch_add(1, Ordering::SeqCst));
        let tp = Arc::new(Tracepoint::new(id, subsystem, name));
        let path = tp.path.clone();

        self.tracepoints.write().insert(id, tp);
        self.by_path.write().insert(path, id);

        id
    }

    /// Get tracepoint by ID
    pub fn get(&self, id: TracepointId) -> Option<Arc<Tracepoint>> {
        self.tracepoints.read().get(&id).cloned()
    }

    /// Get tracepoint by path
    pub fn get_by_path(&self, path: &str) -> Option<Arc<Tracepoint>> {
        let id = self.by_path.read().get(path).copied()?;
        self.get(id)
    }

    /// List all tracepoints
    pub fn list(&self) -> Vec<(TracepointId, String)> {
        self.tracepoints.read()
            .iter()
            .map(|(id, tp)| (*id, tp.path.clone()))
            .collect()
    }

    /// List tracepoints in subsystem
    pub fn list_subsystem(&self, subsystem: &str) -> Vec<Arc<Tracepoint>> {
        self.tracepoints.read()
            .values()
            .filter(|tp| tp.subsystem == subsystem)
            .cloned()
            .collect()
    }

    /// Enable tracepoint
    pub fn enable(&self, id: TracepointId) -> bool {
        if let Some(tp) = self.get(id) {
            tp.enable();
            true
        } else {
            false
        }
    }

    /// Disable tracepoint
    pub fn disable(&self, id: TracepointId) -> bool {
        if let Some(tp) = self.get(id) {
            tp.disable();
            true
        } else {
            false
        }
    }
}

/// Global tracepoint registry
static REGISTRY: Mutex<Option<TracepointRegistry>> = Mutex::new(None);

/// Initialize tracepoint subsystem
pub fn init() {
    let registry = TracepointRegistry::new();

    // Register core kernel tracepoints

    // Scheduler tracepoints
    registry.register("sched", "sched_switch");
    registry.register("sched", "sched_wakeup");
    registry.register("sched", "sched_wakeup_new");
    registry.register("sched", "sched_migrate_task");
    registry.register("sched", "sched_process_fork");
    registry.register("sched", "sched_process_exit");
    registry.register("sched", "sched_process_exec");
    registry.register("sched", "sched_stat_wait");
    registry.register("sched", "sched_stat_sleep");
    registry.register("sched", "sched_stat_runtime");

    // Syscall tracepoints
    registry.register("syscalls", "sys_enter");
    registry.register("syscalls", "sys_exit");

    // IRQ tracepoints
    registry.register("irq", "irq_handler_entry");
    registry.register("irq", "irq_handler_exit");
    registry.register("irq", "softirq_entry");
    registry.register("irq", "softirq_exit");

    // Memory tracepoints
    registry.register("kmem", "kmalloc");
    registry.register("kmem", "kfree");
    registry.register("kmem", "mm_page_alloc");
    registry.register("kmem", "mm_page_free");

    // Filesystem tracepoints
    registry.register("vfs", "vfs_read");
    registry.register("vfs", "vfs_write");
    registry.register("vfs", "vfs_open");
    registry.register("vfs", "vfs_close");

    // Block I/O tracepoints
    registry.register("block", "block_rq_issue");
    registry.register("block", "block_rq_complete");
    registry.register("block", "block_bio_queue");

    // Network tracepoints
    registry.register("net", "net_dev_xmit");
    registry.register("net", "netif_receive_skb");
    registry.register("net", "net_dev_queue");

    // Workqueue tracepoints
    registry.register("workqueue", "workqueue_queue_work");
    registry.register("workqueue", "workqueue_execute_start");
    registry.register("workqueue", "workqueue_execute_end");

    // Timer tracepoints
    registry.register("timer", "timer_start");
    registry.register("timer", "timer_expire_entry");
    registry.register("timer", "timer_expire_exit");
    registry.register("timer", "hrtimer_start");
    registry.register("timer", "hrtimer_expire_entry");

    // Power management tracepoints
    registry.register("power", "cpu_idle");
    registry.register("power", "cpu_frequency");
    registry.register("power", "suspend_resume");

    *REGISTRY.lock() = Some(registry);

    crate::kprintln!("[TRACEPOINT] Kernel tracepoints initialized");
}

/// Get the global registry
pub fn registry() -> Option<&'static TracepointRegistry> {
    // This is safe because we only initialize once and never modify the pointer
    let guard = REGISTRY.lock();
    guard.as_ref().map(|_| {
        // Return static reference through pointer
        unsafe { &*(&*guard as *const Option<TracepointRegistry>) }
            .as_ref()
            .unwrap()
    })
}

/// Fire a tracepoint by path
pub fn fire(path: &str, data: &[u8]) {
    if let Some(registry) = &*REGISTRY.lock() {
        if let Some(tp) = registry.get_by_path(path) {
            tp.fire(data);
        }
    }
}

/// Tracepoint event data structures

/// sched_switch event data
#[repr(C)]
pub struct SchedSwitchData {
    pub prev_comm: [u8; 16],
    pub prev_pid: u32,
    pub prev_prio: u32,
    pub prev_state: u64,
    pub next_comm: [u8; 16],
    pub next_pid: u32,
    pub next_prio: u32,
}

/// sched_wakeup event data
#[repr(C)]
pub struct SchedWakeupData {
    pub comm: [u8; 16],
    pub pid: u32,
    pub prio: u32,
    pub target_cpu: u32,
}

/// syscall enter event data
#[repr(C)]
pub struct SyscallEnterData {
    pub nr: u64,
    pub args: [u64; 6],
}

/// syscall exit event data
#[repr(C)]
pub struct SyscallExitData {
    pub nr: u64,
    pub ret: i64,
}

/// IRQ handler event data
#[repr(C)]
pub struct IrqHandlerData {
    pub irq: u32,
    pub name: [u8; 32],
}

/// Memory allocation event data
#[repr(C)]
pub struct KmallocData {
    pub call_site: u64,
    pub ptr: u64,
    pub bytes_req: usize,
    pub bytes_alloc: usize,
    pub gfp_flags: u32,
}

/// Page allocation event data
#[repr(C)]
pub struct PageAllocData {
    pub page: u64,
    pub order: u32,
    pub gfp_flags: u32,
    pub migratetype: u32,
}

/// Block I/O event data
#[repr(C)]
pub struct BlockRqData {
    pub dev: u64,
    pub sector: u64,
    pub nr_sector: u32,
    pub bytes: u32,
    pub rwbs: [u8; 8],
}

/// Network event data
#[repr(C)]
pub struct NetDevXmitData {
    pub skb: u64,
    pub len: u32,
    pub dev_name: [u8; 16],
}

/// Macro to fire tracepoint with typed data
#[macro_export]
macro_rules! trace_sched_switch {
    ($prev_comm:expr, $prev_pid:expr, $prev_prio:expr, $prev_state:expr,
     $next_comm:expr, $next_pid:expr, $next_prio:expr) => {{
        let data = $crate::perf::tracepoints::SchedSwitchData {
            prev_comm: $prev_comm,
            prev_pid: $prev_pid,
            prev_prio: $prev_prio,
            prev_state: $prev_state,
            next_comm: $next_comm,
            next_pid: $next_pid,
            next_prio: $next_prio,
        };
        let bytes = unsafe {
            core::slice::from_raw_parts(
                &data as *const _ as *const u8,
                core::mem::size_of::<$crate::perf::tracepoints::SchedSwitchData>()
            )
        };
        $crate::perf::tracepoints::fire("sched:sched_switch", bytes);
    }};
}

/// Format specification for tracepoint output
#[derive(Clone, Debug)]
pub struct TracepointFormat {
    /// Field definitions
    pub fields: Vec<FieldDef>,
    /// Print format string
    pub print_fmt: String,
}

/// Field definition
#[derive(Clone, Debug)]
pub struct FieldDef {
    /// Field name
    pub name: String,
    /// Field type
    pub field_type: String,
    /// Offset in struct
    pub offset: usize,
    /// Size in bytes
    pub size: usize,
    /// Is signed
    pub is_signed: bool,
}
