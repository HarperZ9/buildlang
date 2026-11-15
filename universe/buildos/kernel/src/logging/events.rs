// ===============================================================================
// BUILDOS KERNEL - EVENT RECORDING
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Event Recording System
//!
//! Provides structured event recording for kernel instrumentation:
//! - System events (boot, shutdown, suspend, resume)
//! - Hardware events (hotplug, errors)
//! - Performance events (counters, samples)
//! - Security events (auth, access)

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::sync::Spinlock;
use super::ringbuf::RingBuffer;

/// Event sequence counter
static EVENT_SEQ: AtomicU64 = AtomicU64::new(0);

/// Event recording enabled
static EVENTS_ENABLED: AtomicBool = AtomicBool::new(false);

/// Global event ring buffer
static EVENT_BUFFER: Spinlock<Option<RingBuffer<EventRecord>>> = Spinlock::new(None);

/// Event handlers
static EVENT_HANDLERS: Spinlock<Vec<Box<dyn EventHandler>>> = Spinlock::new(Vec::new());

/// Event types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum EventType {
    // System events (0x0000 - 0x00FF)
    SystemBoot = 0x0001,
    SystemShutdown = 0x0002,
    SystemSuspend = 0x0003,
    SystemResume = 0x0004,
    SystemPanic = 0x0005,
    SystemOops = 0x0006,
    SystemReboot = 0x0007,

    // Process events (0x0100 - 0x01FF)
    ProcessCreate = 0x0100,
    ProcessExit = 0x0101,
    ProcessExec = 0x0102,
    ProcessFork = 0x0103,
    ProcessSignal = 0x0104,
    ProcessCoredump = 0x0105,
    ProcessOom = 0x0106,

    // Memory events (0x0200 - 0x02FF)
    MemoryAlloc = 0x0200,
    MemoryFree = 0x0201,
    MemoryOom = 0x0202,
    MemoryPageFault = 0x0203,
    MemoryMmap = 0x0204,
    MemoryMunmap = 0x0205,
    MemoryBrk = 0x0206,

    // File events (0x0300 - 0x03FF)
    FileOpen = 0x0300,
    FileClose = 0x0301,
    FileRead = 0x0302,
    FileWrite = 0x0303,
    FileCreate = 0x0304,
    FileDelete = 0x0305,
    FileRename = 0x0306,
    FileChmod = 0x0307,
    FileChown = 0x0308,

    // Network events (0x0400 - 0x04FF)
    NetConnect = 0x0400,
    NetDisconnect = 0x0401,
    NetBind = 0x0402,
    NetListen = 0x0403,
    NetAccept = 0x0404,
    NetSend = 0x0405,
    NetRecv = 0x0406,
    NetError = 0x0407,

    // Device events (0x0500 - 0x05FF)
    DeviceAdd = 0x0500,
    DeviceRemove = 0x0501,
    DeviceError = 0x0502,
    DeviceSuspend = 0x0503,
    DeviceResume = 0x0504,

    // Security events (0x0600 - 0x06FF)
    SecurityLogin = 0x0600,
    SecurityLogout = 0x0601,
    SecurityAuthFail = 0x0602,
    SecurityAccessDenied = 0x0603,
    SecurityPrivEsc = 0x0604,
    SecurityCapChange = 0x0605,
    SecuritySelinux = 0x0606,
    SecuritySeccomp = 0x0607,

    // Scheduler events (0x0700 - 0x07FF)
    SchedSwitch = 0x0700,
    SchedWakeup = 0x0701,
    SchedMigrate = 0x0702,
    SchedBlock = 0x0703,
    SchedUnblock = 0x0704,

    // IRQ events (0x0800 - 0x08FF)
    IrqEntry = 0x0800,
    IrqExit = 0x0801,
    IrqDisable = 0x0802,
    IrqEnable = 0x0803,

    // Timer events (0x0900 - 0x09FF)
    TimerExpire = 0x0900,
    TimerStart = 0x0901,
    TimerCancel = 0x0902,

    // Block I/O events (0x0A00 - 0x0AFF)
    BlockRead = 0x0A00,
    BlockWrite = 0x0A01,
    BlockComplete = 0x0A02,
    BlockError = 0x0A03,

    // Custom events (0xFF00 - 0xFFFF)
    Custom = 0xFF00,
}

impl EventType {
    pub fn category(&self) -> &'static str {
        let val = *self as u16;
        match val >> 8 {
            0x00 => "system",
            0x01 => "process",
            0x02 => "memory",
            0x03 => "file",
            0x04 => "network",
            0x05 => "device",
            0x06 => "security",
            0x07 => "scheduler",
            0x08 => "irq",
            0x09 => "timer",
            0x0A => "block",
            _ => "custom",
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::SystemBoot => "boot",
            Self::SystemShutdown => "shutdown",
            Self::SystemSuspend => "suspend",
            Self::SystemResume => "resume",
            Self::SystemPanic => "panic",
            Self::SystemOops => "oops",
            Self::SystemReboot => "reboot",

            Self::ProcessCreate => "create",
            Self::ProcessExit => "exit",
            Self::ProcessExec => "exec",
            Self::ProcessFork => "fork",
            Self::ProcessSignal => "signal",
            Self::ProcessCoredump => "coredump",
            Self::ProcessOom => "oom",

            Self::MemoryAlloc => "alloc",
            Self::MemoryFree => "free",
            Self::MemoryOom => "oom",
            Self::MemoryPageFault => "page_fault",
            Self::MemoryMmap => "mmap",
            Self::MemoryMunmap => "munmap",
            Self::MemoryBrk => "brk",

            Self::FileOpen => "open",
            Self::FileClose => "close",
            Self::FileRead => "read",
            Self::FileWrite => "write",
            Self::FileCreate => "create",
            Self::FileDelete => "delete",
            Self::FileRename => "rename",
            Self::FileChmod => "chmod",
            Self::FileChown => "chown",

            Self::NetConnect => "connect",
            Self::NetDisconnect => "disconnect",
            Self::NetBind => "bind",
            Self::NetListen => "listen",
            Self::NetAccept => "accept",
            Self::NetSend => "send",
            Self::NetRecv => "recv",
            Self::NetError => "error",

            Self::DeviceAdd => "add",
            Self::DeviceRemove => "remove",
            Self::DeviceError => "error",
            Self::DeviceSuspend => "suspend",
            Self::DeviceResume => "resume",

            Self::SecurityLogin => "login",
            Self::SecurityLogout => "logout",
            Self::SecurityAuthFail => "auth_fail",
            Self::SecurityAccessDenied => "access_denied",
            Self::SecurityPrivEsc => "priv_esc",
            Self::SecurityCapChange => "cap_change",
            Self::SecuritySelinux => "selinux",
            Self::SecuritySeccomp => "seccomp",

            Self::SchedSwitch => "switch",
            Self::SchedWakeup => "wakeup",
            Self::SchedMigrate => "migrate",
            Self::SchedBlock => "block",
            Self::SchedUnblock => "unblock",

            Self::IrqEntry => "entry",
            Self::IrqExit => "exit",
            Self::IrqDisable => "disable",
            Self::IrqEnable => "enable",

            Self::TimerExpire => "expire",
            Self::TimerStart => "start",
            Self::TimerCancel => "cancel",

            Self::BlockRead => "read",
            Self::BlockWrite => "write",
            Self::BlockComplete => "complete",
            Self::BlockError => "error",

            Self::Custom => "custom",
        }
    }
}

/// Event record
#[derive(Clone, Debug)]
pub struct EventRecord {
    /// Sequence number
    pub sequence: u64,
    /// Timestamp (ns since boot)
    pub timestamp: u64,
    /// Event type
    pub event_type: EventType,
    /// CPU ID
    pub cpu: u32,
    /// Process ID
    pub pid: u32,
    /// Thread ID
    pub tid: u32,
    /// User ID
    pub uid: u32,
    /// Event-specific data
    pub data: EventData,
}

impl EventRecord {
    pub fn new(event_type: EventType, data: EventData) -> Self {
        Self {
            sequence: EVENT_SEQ.fetch_add(1, Ordering::Relaxed),
            timestamp: crate::time::now_ns(),
            event_type,
            cpu: current_cpu(),
            pid: current_pid(),
            tid: current_tid(),
            uid: current_uid(),
            data,
        }
    }
}

/// Event data payload
#[derive(Clone, Debug)]
pub enum EventData {
    /// No additional data
    Empty,

    /// Process event data
    Process {
        target_pid: u32,
        parent_pid: u32,
        exit_code: i32,
        comm: String,
    },

    /// File event data
    File {
        path: String,
        fd: i32,
        flags: u32,
        mode: u32,
        size: u64,
    },

    /// Memory event data
    Memory {
        address: u64,
        size: u64,
        prot: u32,
        flags: u32,
    },

    /// Network event data
    Network {
        protocol: u8,
        local_addr: [u8; 16],
        local_port: u16,
        remote_addr: [u8; 16],
        remote_port: u16,
        bytes: u64,
    },

    /// Security event data
    Security {
        action: String,
        target: String,
        result: i32,
        extra: String,
    },

    /// Device event data
    Device {
        name: String,
        bus: String,
        vendor: u16,
        product: u16,
    },

    /// Scheduler event data
    Scheduler {
        prev_pid: u32,
        prev_comm: String,
        prev_state: u32,
        next_pid: u32,
        next_comm: String,
    },

    /// Block I/O event data
    BlockIo {
        device: u32,
        sector: u64,
        count: u32,
        flags: u32,
        latency_ns: u64,
    },

    /// IRQ event data
    Irq {
        irq: u32,
        name: String,
        handled: bool,
    },

    /// Timer event data
    Timer {
        timer_id: u64,
        expires: u64,
        function: u64,
    },

    /// Custom event data
    Custom {
        name: String,
        data: Vec<u8>,
    },
}

/// Event handler trait
pub trait EventHandler: Send + Sync {
    /// Handle an event
    fn handle(&self, event: &EventRecord);

    /// Event types this handler is interested in (empty = all)
    fn filter(&self) -> Vec<EventType> {
        Vec::new()
    }

    /// Handler name
    fn name(&self) -> &str;
}

/// Logging event handler
pub struct LoggingHandler {
    name: String,
}

impl LoggingHandler {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

impl EventHandler for LoggingHandler {
    fn handle(&self, event: &EventRecord) {
        crate::kprintln!(
            "[EVENT] seq={} type={}.{} pid={} {:?}",
            event.sequence,
            event.event_type.category(),
            event.event_type.name(),
            event.pid,
            event.data
        );
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Initialize event system
pub fn init(buffer_size: usize) {
    *EVENT_BUFFER.lock() = Some(RingBuffer::new(buffer_size));
    EVENTS_ENABLED.store(true, Ordering::Release);
}

/// Record an event
pub fn record(event_type: EventType, data: EventData) {
    if !EVENTS_ENABLED.load(Ordering::Acquire) {
        return;
    }

    let record = EventRecord::new(event_type, data);

    // Store in buffer
    if let Some(ref mut buffer) = *EVENT_BUFFER.lock() {
        buffer.push(record.clone());
    }

    // Call handlers
    let handlers = EVENT_HANDLERS.lock();
    for handler in handlers.iter() {
        let filter = handler.filter();
        if filter.is_empty() || filter.contains(&event_type) {
            handler.handle(&record);
        }
    }
}

/// Register an event handler
pub fn register_handler(handler: Box<dyn EventHandler>) {
    EVENT_HANDLERS.lock().push(handler);
}

/// Read events from buffer
pub fn read_events(start_seq: u64, max_count: usize) -> Vec<EventRecord> {
    if let Some(ref buffer) = *EVENT_BUFFER.lock() {
        buffer.read_from(start_seq, max_count)
    } else {
        Vec::new()
    }
}

/// Get event count
pub fn event_count() -> u64 {
    EVENT_SEQ.load(Ordering::Relaxed)
}

/// Clear event buffer
pub fn clear() {
    if let Some(ref mut buffer) = *EVENT_BUFFER.lock() {
        buffer.clear();
    }
}

/// Enable/disable event recording
pub fn set_enabled(enabled: bool) {
    EVENTS_ENABLED.store(enabled, Ordering::Release);
}

/// Check if enabled
pub fn is_enabled() -> bool {
    EVENTS_ENABLED.load(Ordering::Acquire)
}

// Helper functions
fn current_cpu() -> u32 {
    0 // TODO
}

fn current_pid() -> u32 {
    0 // TODO
}

fn current_tid() -> u32 {
    0 // TODO
}

fn current_uid() -> u32 {
    0 // TODO
}

// Convenience functions for common events

/// Record process creation
pub fn process_create(pid: u32, parent_pid: u32, comm: &str) {
    record(EventType::ProcessCreate, EventData::Process {
        target_pid: pid,
        parent_pid,
        exit_code: 0,
        comm: comm.into(),
    });
}

/// Record process exit
pub fn process_exit(pid: u32, exit_code: i32) {
    record(EventType::ProcessExit, EventData::Process {
        target_pid: pid,
        parent_pid: 0,
        exit_code,
        comm: String::new(),
    });
}

/// Record file open
pub fn file_open(path: &str, fd: i32, flags: u32) {
    record(EventType::FileOpen, EventData::File {
        path: path.into(),
        fd,
        flags,
        mode: 0,
        size: 0,
    });
}

/// Record memory allocation
pub fn memory_alloc(address: u64, size: u64) {
    record(EventType::MemoryAlloc, EventData::Memory {
        address,
        size,
        prot: 0,
        flags: 0,
    });
}

/// Record scheduler context switch
pub fn sched_switch(
    prev_pid: u32, prev_comm: &str, prev_state: u32,
    next_pid: u32, next_comm: &str,
) {
    record(EventType::SchedSwitch, EventData::Scheduler {
        prev_pid,
        prev_comm: prev_comm.into(),
        prev_state,
        next_pid,
        next_comm: next_comm.into(),
    });
}

/// Record network connection
pub fn net_connect(
    protocol: u8,
    local_addr: [u8; 16], local_port: u16,
    remote_addr: [u8; 16], remote_port: u16,
) {
    record(EventType::NetConnect, EventData::Network {
        protocol,
        local_addr,
        local_port,
        remote_addr,
        remote_port,
        bytes: 0,
    });
}

/// Record security event
pub fn security_event(event_type: EventType, action: &str, target: &str, result: i32) {
    record(event_type, EventData::Security {
        action: action.into(),
        target: target.into(),
        result,
        extra: String::new(),
    });
}

/// Record block I/O
pub fn block_io(event_type: EventType, device: u32, sector: u64, count: u32, latency_ns: u64) {
    record(event_type, EventData::BlockIo {
        device,
        sector,
        count,
        flags: 0,
        latency_ns,
    });
}

/// Record device event
pub fn device_event(event_type: EventType, name: &str, bus: &str, vendor: u16, product: u16) {
    record(event_type, EventData::Device {
        name: name.into(),
        bus: bus.into(),
        vendor,
        product,
    });
}

/// Record IRQ event
pub fn irq_event(event_type: EventType, irq: u32, name: &str, handled: bool) {
    record(event_type, EventData::Irq {
        irq,
        name: name.into(),
        handled,
    });
}

/// Record custom event
pub fn custom(name: &str, data: &[u8]) {
    record(EventType::Custom, EventData::Custom {
        name: name.into(),
        data: data.to_vec(),
    });
}

/// Event filter builder
pub struct EventFilter {
    event_types: Vec<EventType>,
    pids: Vec<u32>,
    uids: Vec<u32>,
    cpus: Vec<u32>,
}

impl EventFilter {
    pub fn new() -> Self {
        Self {
            event_types: Vec::new(),
            pids: Vec::new(),
            uids: Vec::new(),
            cpus: Vec::new(),
        }
    }

    pub fn event_type(mut self, t: EventType) -> Self {
        self.event_types.push(t);
        self
    }

    pub fn pid(mut self, pid: u32) -> Self {
        self.pids.push(pid);
        self
    }

    pub fn uid(mut self, uid: u32) -> Self {
        self.uids.push(uid);
        self
    }

    pub fn cpu(mut self, cpu: u32) -> Self {
        self.cpus.push(cpu);
        self
    }

    pub fn matches(&self, event: &EventRecord) -> bool {
        if !self.event_types.is_empty() && !self.event_types.contains(&event.event_type) {
            return false;
        }
        if !self.pids.is_empty() && !self.pids.contains(&event.pid) {
            return false;
        }
        if !self.uids.is_empty() && !self.uids.contains(&event.uid) {
            return false;
        }
        if !self.cpus.is_empty() && !self.cpus.contains(&event.cpu) {
            return false;
        }
        true
    }
}

impl Default for EventFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Filtered event handler wrapper
pub struct FilteredHandler {
    inner: Box<dyn EventHandler>,
    filter: EventFilter,
}

impl FilteredHandler {
    pub fn new(inner: Box<dyn EventHandler>, filter: EventFilter) -> Self {
        Self { inner, filter }
    }
}

impl EventHandler for FilteredHandler {
    fn handle(&self, event: &EventRecord) {
        if self.filter.matches(event) {
            self.inner.handle(event);
        }
    }

    fn name(&self) -> &str {
        self.inner.name()
    }
}
