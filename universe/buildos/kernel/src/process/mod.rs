// ===============================================================================
// BUILDOS KERNEL - PROCESS MANAGEMENT
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Process and thread management.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::RwLock;

// Submodules
pub mod signal;
pub mod futex;

// Re-export important types from submodules
pub use signal::{SigSet, SigAction, SigHandler, SigInfo, SignalState, DefaultAction, check_signals};
pub use futex::{FutexOp, FutexKey, FutexTable, FutexError};

// =============================================================================
// CONSTANTS
// =============================================================================

/// Maximum number of processes
pub const MAX_PROCESSES: usize = 65536;

/// Maximum threads per process
pub const MAX_THREADS: usize = 1024;

/// Maximum file descriptors per process
pub const MAX_FDS: usize = 4096;

/// Kernel stack size (64KB)
pub const KERNEL_STACK_SIZE: usize = 64 * 1024;

/// User stack size (8MB)
pub const USER_STACK_SIZE: usize = 8 * 1024 * 1024;

// =============================================================================
// PROCESS ID
// =============================================================================

/// Process ID type
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Pid(u64);

impl Pid {
    /// Create a new PID
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the raw value
    pub fn as_u64(&self) -> u64 {
        self.0
    }

    /// Get as i32 for syscall compatibility
    pub fn as_i32(&self) -> i32 {
        self.0 as i32
    }

    /// Kernel PID (always 0)
    pub const KERNEL: Pid = Pid(0);
}

impl core::fmt::Display for Pid {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Thread ID type
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tid(u64);

impl Tid {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

// =============================================================================
// PROCESS STATE
// =============================================================================

/// Process state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    /// Process is newly created
    New,

    /// Process is ready to run
    Ready,

    /// Process is currently running
    Running,

    /// Process is blocked waiting for something
    Blocked,

    /// Process has terminated
    Terminated,

    /// Process is a zombie (terminated but not yet reaped)
    Zombie,

    /// Process is stopped (by signal)
    Stopped,
}

/// Thread state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Ready,
    Running,
    Blocked,
    Terminated,
}

// =============================================================================
// PROCESS STRUCTURE
// =============================================================================

/// Process control block
pub struct Process {
    /// Process ID
    pub pid: Pid,

    /// Parent process ID
    pub parent: Option<Pid>,

    /// Process name
    pub name: String,

    /// Current state
    pub state: ProcessState,

    /// Threads in this process
    pub threads: Vec<Tid>,

    /// Memory space (page table root)
    pub memory_space: u64,

    /// File descriptors
    pub fds: [Option<FileDescriptor>; MAX_FDS],

    /// Exit code (if terminated)
    pub exit_code: Option<i32>,

    /// Priority (for scheduler)
    pub priority: i32,

    /// Dynamic priority boost
    pub ai_priority_boost: f32,

    /// CPU time used (in nanoseconds)
    pub cpu_time: u64,

    /// Working directory
    pub cwd: String,

    /// Environment variables
    pub env: Vec<(String, String)>,

    /// Signal state (using new signal subsystem)
    pub signal_state: SignalState,

    /// Process group ID
    pub pgid: Pid,

    /// Session ID
    pub sid: Pid,

    /// User ID (owner)
    pub uid: u32,

    /// Group ID
    pub gid: u32,

    /// Effective user ID
    pub euid: u32,

    /// Effective group ID
    pub egid: u32,

    /// Saved set-user-ID
    pub suid: u32,

    /// Saved set-group-ID
    pub sgid: u32,

    /// Supplementary groups
    pub groups: Vec<u32>,

    /// Resource limits
    pub rlimits: ResourceLimits,

    /// Robust futex list head (userspace pointer)
    pub robust_list: Option<usize>,

    /// Child reaper (for subreaping orphaned processes)
    pub child_subreaper: bool,

    /// Creation time
    pub start_time: u64,
}

impl Process {
    /// Check if process has pending signals
    pub fn has_pending_signals(&self) -> bool {
        self.signal_state.has_pending()
    }

    /// Post a signal to this process
    pub fn post_signal(&mut self, sig: signal::Signal, info: Option<SigInfo>) {
        self.signal_state.post(sig, info);
    }

    /// Get next deliverable signal
    pub fn dequeue_signal(&mut self) -> Option<(signal::Signal, Option<SigInfo>)> {
        self.signal_state.dequeue()
    }
}

/// Resource limits
#[derive(Clone, Debug)]
pub struct ResourceLimits {
    /// Maximum CPU time (seconds)
    pub cpu: (u64, u64),
    /// Maximum file size (bytes)
    pub fsize: (u64, u64),
    /// Maximum data segment size
    pub data: (u64, u64),
    /// Maximum stack size
    pub stack: (u64, u64),
    /// Maximum core dump size
    pub core: (u64, u64),
    /// Maximum resident set size
    pub rss: (u64, u64),
    /// Maximum number of processes
    pub nproc: (u64, u64),
    /// Maximum open files
    pub nofile: (u64, u64),
    /// Maximum locked memory
    pub memlock: (u64, u64),
    /// Maximum address space
    pub as_: (u64, u64),
    /// Maximum file locks
    pub locks: (u64, u64),
    /// Maximum pending signals
    pub sigpending: (u64, u64),
    /// Maximum message queue size
    pub msgqueue: (u64, u64),
    /// Nice ceiling
    pub nice: (u64, u64),
    /// Realtime priority ceiling
    pub rtprio: (u64, u64),
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpu: (u64::MAX, u64::MAX),
            fsize: (u64::MAX, u64::MAX),
            data: (u64::MAX, u64::MAX),
            stack: (8 * 1024 * 1024, u64::MAX),
            core: (0, u64::MAX),
            rss: (u64::MAX, u64::MAX),
            nproc: (65536, 65536),
            nofile: (1024, 1024 * 1024),
            memlock: (64 * 1024, 64 * 1024),
            as_: (u64::MAX, u64::MAX),
            locks: (u64::MAX, u64::MAX),
            sigpending: (32768, 32768),
            msgqueue: (819200, 819200),
            nice: (0, 0),
            rtprio: (0, 0),
        }
    }
}

/// Thread control block
pub struct Thread {
    /// Thread ID
    pub tid: Tid,

    /// Parent process
    pub process: Pid,

    /// Thread state
    pub state: ThreadState,

    /// Kernel stack pointer
    pub kernel_stack: u64,

    /// User stack pointer
    pub user_stack: u64,

    /// Saved CPU context
    pub context: CpuContext,

    /// Priority
    pub priority: i32,

    /// Thread-local storage pointer
    pub tls: u64,

    /// Clear child TID address (for futex wake on exit)
    pub clear_child_tid: Option<*mut u32>,

    /// Set child TID address
    pub set_child_tid: Option<*mut u32>,

    /// Per-thread signal mask
    pub signal_mask: SigSet,

    /// Alternate signal stack
    pub alt_stack: Option<signal::SignalStack>,
}

/// Saved CPU context for context switching
#[repr(C)]
#[derive(Default, Clone)]
pub struct CpuContext {
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
    pub fs_base: u64,
    pub gs_base: u64,
}

/// File descriptor entry
#[derive(Clone)]
pub struct FileDescriptor {
    pub flags: u32,
    pub offset: u64,
    pub close_on_exec: bool,
}

// =============================================================================
// PROCESS TABLE
// =============================================================================

/// Global process table
static PROCESS_TABLE: RwLock<ProcessTable> = RwLock::new(ProcessTable::new());

/// Global thread table
static THREAD_TABLE: RwLock<ThreadTable> = RwLock::new(ThreadTable::new());

/// Next PID to allocate
static NEXT_PID: AtomicU64 = AtomicU64::new(1);

/// Next TID to allocate
static NEXT_TID: AtomicU64 = AtomicU64::new(1);

/// Process table
pub struct ProcessTable {
    /// Current running process
    pub current: Option<Pid>,

    /// All processes
    pub processes: [Option<Box<Process>>; MAX_PROCESSES],
}

/// Thread table
pub struct ThreadTable {
    /// Current running thread
    pub current: Option<Tid>,

    /// All threads
    pub threads: [Option<Box<Thread>>; MAX_PROCESSES * 4],
}

impl ProcessTable {
    const fn new() -> Self {
        const NONE: Option<Box<Process>> = None;
        Self {
            current: None,
            processes: [NONE; MAX_PROCESSES],
        }
    }
}

impl ThreadTable {
    const fn new() -> Self {
        const NONE: Option<Box<Thread>> = None;
        Self {
            current: None,
            threads: [NONE; MAX_PROCESSES * 4],
        }
    }
}

// Safety: Thread and ThreadTable are protected by RwLock.
// The raw pointers (clear_child_tid, set_child_tid) are user-space addresses
// that are properly synchronized through the lock.
unsafe impl Send for Thread {}
unsafe impl Sync for Thread {}
unsafe impl Send for ThreadTable {}
unsafe impl Sync for ThreadTable {}

// =============================================================================
// PROCESS FUNCTIONS
// =============================================================================

/// Allocate a new PID
pub fn alloc_pid() -> Pid {
    Pid::new(NEXT_PID.fetch_add(1, Ordering::SeqCst))
}

/// Allocate a new TID
pub fn alloc_tid() -> Tid {
    Tid::new(NEXT_TID.fetch_add(1, Ordering::SeqCst))
}

/// Get current process
pub fn current() -> Option<Pid> {
    PROCESS_TABLE.read().current
}

/// Get current process ID as u64
pub fn current_pid() -> Option<u64> {
    current().map(|pid| pid.as_u64())
}

/// Get current thread
pub fn current_thread() -> Option<Tid> {
    THREAD_TABLE.read().current
}

/// Initialize the futex subsystem
pub fn init_futex() {
    futex::init();
}

/// Start the init process
pub fn start_init() {
    use crate::elf;
    use crate::memory::{MemoryManager, PAGE_SIZE};

    // Allocate PID 1 for init
    let pid = alloc_pid();
    let tid = alloc_tid();

    // Allocate kernel stack for the init thread
    let kernel_stack = {
        let mut mm_guard = MemoryManager::get().lock();
        if let Some(mm) = mm_guard.as_mut() {
            let pages = KERNEL_STACK_SIZE / PAGE_SIZE;
            mm.alloc_pages(pages).unwrap_or(0)
        } else {
            0
        }
    };

    // Create the init process control block
    let mut process = Process {
        pid,
        parent: None,
        name: String::from("init"),
        state: ProcessState::New,
        threads: Vec::new(),
        memory_space: 0,
        fds: core::array::from_fn(|_| None),
        exit_code: None,
        priority: 0,
        ai_priority_boost: 0.0,
        cpu_time: 0,
        cwd: String::from("/"),
        env: Vec::new(),
        signal_state: SignalState::new(),
        pgid: pid,
        sid: pid,
        uid: 0,
        gid: 0,
        euid: 0,
        egid: 0,
        suid: 0,
        sgid: 0,
        groups: Vec::new(),
        rlimits: ResourceLimits::default(),
        robust_list: None,
        child_subreaper: false,
        start_time: 0,
    };

    // Set up standard file descriptors
    process.fds[0] = Some(FileDescriptor { flags: 0, offset: 0, close_on_exec: false });
    process.fds[1] = Some(FileDescriptor { flags: 1, offset: 0, close_on_exec: false });
    process.fds[2] = Some(FileDescriptor { flags: 1, offset: 0, close_on_exec: false });

    // Create the main thread
    let thread = Thread {
        tid,
        process: pid,
        state: ThreadState::Ready,
        kernel_stack: kernel_stack + KERNEL_STACK_SIZE as u64,
        user_stack: 0,
        context: CpuContext::default(),
        priority: 0,
        tls: 0,
        clear_child_tid: None,
        set_child_tid: None,
        signal_mask: SigSet::empty(),
        alt_stack: None,
    };

    process.threads.push(tid);

    // Store in tables
    {
        let mut proc_table = PROCESS_TABLE.write();
        proc_table.processes[pid.as_u64() as usize] = Some(Box::new(process));
        proc_table.current = Some(pid);
    }

    {
        let mut thread_table = THREAD_TABLE.write();
        thread_table.threads[tid.as_u64() as usize] = Some(Box::new(thread));
        thread_table.current = Some(tid);
    }

    // Try to load /sbin/init, fallback to /init, then to built-in shell
    let init_paths = ["/sbin/init", "/init", "/bin/sh"];
    let init_args: &[&str] = &["init"];
    let init_env: &[(&str, &str)] = &[
        ("PATH", "/bin:/sbin:/usr/bin:/usr/sbin"),
        ("HOME", "/"),
        ("TERM", "buildos-console"),
    ];

    for init_path in init_paths {
        if elf::is_elf(init_path) {
            match elf::exec(init_path, init_args, init_env, pid) {
                Ok(context) => {
                    // Update thread context
                    let user_stack = context.rsp;
                    let mut thread_table = THREAD_TABLE.write();
                    if let Some(ref mut thread) = thread_table.threads[tid.as_u64() as usize] {
                        thread.context = context;
                        thread.user_stack = user_stack;
                        thread.state = ThreadState::Ready;
                    }

                    // Update process state
                    let mut proc_table = PROCESS_TABLE.write();
                    if let Some(ref mut proc) = proc_table.processes[pid.as_u64() as usize] {
                        proc.state = ProcessState::Ready;
                        proc.name = String::from(init_path);
                    }

                    crate::kprintln!("[PROCESS] Loaded {} as PID 1", init_path);
                    return;
                }
                Err(e) => {
                    crate::kprintln!("[PROCESS] Failed to load {}: {:?}", init_path, e);
                }
            }
        }
    }

    // No init found
    crate::kprintln!("[PROCESS] Warning: No init process found, running in kernel mode");

    let mut proc_table = PROCESS_TABLE.write();
    if let Some(ref mut proc) = proc_table.processes[pid.as_u64() as usize] {
        proc.state = ProcessState::Ready;
    }
}

/// Fork the current process (creates copy-on-write clone)
pub fn fork() -> Option<Pid> {
    use crate::memory::{MemoryManager, PAGE_SIZE};

    let parent_pid = current()?;
    let parent_tid = current_thread()?;

    let child_pid = alloc_pid();
    let child_tid = alloc_tid();

    // Allocate kernel stack for child
    let kernel_stack = {
        let mut mm_guard = MemoryManager::get().lock();
        if let Some(mm) = mm_guard.as_mut() {
            let pages = KERNEL_STACK_SIZE / PAGE_SIZE;
            mm.alloc_pages(pages)?
        } else {
            return None;
        }
    };

    // Clone parent process
    let child_process = {
        let proc_table = PROCESS_TABLE.read();
        let parent = proc_table.processes[parent_pid.as_u64() as usize].as_ref()?;

        Process {
            pid: child_pid,
            parent: Some(parent_pid),
            name: parent.name.clone(),
            state: ProcessState::Ready,
            threads: Vec::new(),
            memory_space: 0, // COW
            fds: parent.fds.clone(),
            exit_code: None,
            priority: parent.priority,
            ai_priority_boost: parent.ai_priority_boost,
            cpu_time: 0,
            cwd: parent.cwd.clone(),
            env: parent.env.clone(),
            signal_state: SignalState::new(), // Child gets fresh signal state
            pgid: parent.pgid,
            sid: parent.sid,
            uid: parent.uid,
            gid: parent.gid,
            euid: parent.euid,
            egid: parent.egid,
            suid: parent.suid,
            sgid: parent.sgid,
            groups: parent.groups.clone(),
            rlimits: parent.rlimits.clone(),
            robust_list: None,
            child_subreaper: false,
            start_time: 0, // Would set to current time
        }
    };

    // Clone parent thread context
    let child_thread = {
        let thread_table = THREAD_TABLE.read();
        let parent_thread = thread_table.threads[parent_tid.as_u64() as usize].as_ref()?;

        Thread {
            tid: child_tid,
            process: child_pid,
            state: ThreadState::Ready,
            kernel_stack: kernel_stack + KERNEL_STACK_SIZE as u64,
            user_stack: parent_thread.user_stack,
            context: CpuContext {
                rax: 0, // Child returns 0 from fork
                ..parent_thread.context.clone()
            },
            priority: parent_thread.priority,
            tls: parent_thread.tls,
            clear_child_tid: None,
            set_child_tid: None,
            signal_mask: parent_thread.signal_mask,
            alt_stack: parent_thread.alt_stack.clone(),
        }
    };

    // Store child in tables
    {
        let mut proc_table = PROCESS_TABLE.write();
        let mut process = Box::new(child_process);
        process.threads.push(child_tid);
        proc_table.processes[child_pid.as_u64() as usize] = Some(process);
    }

    {
        let mut thread_table = THREAD_TABLE.write();
        thread_table.threads[child_tid.as_u64() as usize] = Some(Box::new(child_thread));
    }

    // Add child to scheduler
    crate::scheduler::add_thread(child_tid.as_u64() as u32, 0);

    Some(child_pid)
}

/// Clone with flags (like Linux clone())
pub fn clone_process(flags: CloneFlags, stack: Option<u64>) -> Option<Tid> {
    use crate::memory::{MemoryManager, PAGE_SIZE};

    let parent_pid = current()?;
    let parent_tid = current_thread()?;

    let new_tid = alloc_tid();

    // Allocate kernel stack
    let kernel_stack = {
        let mut mm_guard = MemoryManager::get().lock();
        if let Some(mm) = mm_guard.as_mut() {
            let pages = KERNEL_STACK_SIZE / PAGE_SIZE;
            mm.alloc_pages(pages)?
        } else {
            return None;
        }
    };

    // Get parent info
    let (child_pid, new_process) = if flags.contains(CloneFlags::THREAD) {
        // Creating a thread in the same process
        (parent_pid, false)
    } else {
        // Creating a new process
        (alloc_pid(), true)
    };

    if new_process {
        // Clone process structure
        let child_process = {
            let proc_table = PROCESS_TABLE.read();
            let parent = proc_table.processes[parent_pid.as_u64() as usize].as_ref()?;

            let mut proc = Process {
                pid: child_pid,
                parent: Some(parent_pid),
                name: parent.name.clone(),
                state: ProcessState::Ready,
                threads: Vec::new(),
                memory_space: if flags.contains(CloneFlags::VM) {
                    parent.memory_space // Share memory
                } else {
                    0 // Would COW clone
                },
                fds: if flags.contains(CloneFlags::FILES) {
                    parent.fds.clone() // Share file descriptors
                } else {
                    parent.fds.clone() // Clone file descriptors
                },
                exit_code: None,
                priority: parent.priority,
                ai_priority_boost: 0.0,
                cpu_time: 0,
                cwd: parent.cwd.clone(),
                env: parent.env.clone(),
                signal_state: if flags.contains(CloneFlags::SIGHAND) {
                    parent.signal_state.clone()
                } else {
                    SignalState::new()
                },
                pgid: if flags.contains(CloneFlags::THREAD) {
                    parent.pgid
                } else {
                    child_pid
                },
                sid: parent.sid,
                uid: parent.uid,
                gid: parent.gid,
                euid: parent.euid,
                egid: parent.egid,
                suid: parent.suid,
                sgid: parent.sgid,
                groups: parent.groups.clone(),
                rlimits: parent.rlimits.clone(),
                robust_list: None,
                child_subreaper: false,
                start_time: 0,
            };
            proc.threads.push(new_tid);
            proc
        };

        let mut proc_table = PROCESS_TABLE.write();
        proc_table.processes[child_pid.as_u64() as usize] = Some(Box::new(child_process));
    } else {
        // Just add thread to existing process
        let mut proc_table = PROCESS_TABLE.write();
        if let Some(ref mut process) = proc_table.processes[parent_pid.as_u64() as usize] {
            process.threads.push(new_tid);
        }
    }

    // Clone thread
    let new_thread = {
        let thread_table = THREAD_TABLE.read();
        let parent_thread = thread_table.threads[parent_tid.as_u64() as usize].as_ref()?;

        Thread {
            tid: new_tid,
            process: child_pid,
            state: ThreadState::Ready,
            kernel_stack: kernel_stack + KERNEL_STACK_SIZE as u64,
            user_stack: stack.unwrap_or(parent_thread.user_stack),
            context: CpuContext {
                rax: 0,
                rsp: stack.unwrap_or(parent_thread.context.rsp),
                ..parent_thread.context.clone()
            },
            priority: parent_thread.priority,
            tls: parent_thread.tls,
            clear_child_tid: if flags.contains(CloneFlags::CHILD_CLEARTID) {
                None // Would be set via syscall arg
            } else {
                None
            },
            set_child_tid: if flags.contains(CloneFlags::CHILD_SETTID) {
                None // Would be set via syscall arg
            } else {
                None
            },
            signal_mask: parent_thread.signal_mask,
            alt_stack: None,
        }
    };

    {
        let mut thread_table = THREAD_TABLE.write();
        thread_table.threads[new_tid.as_u64() as usize] = Some(Box::new(new_thread));
    }

    // Add to scheduler
    crate::scheduler::add_thread(new_tid.as_u64() as u32, 0);

    Some(new_tid)
}

bitflags::bitflags! {
    /// Clone flags
    #[derive(Clone, Copy, Debug)]
    pub struct CloneFlags: u64 {
        /// Share memory space
        const VM = 0x00000100;
        /// Share file system info
        const FS = 0x00000200;
        /// Share file descriptors
        const FILES = 0x00000400;
        /// Share signal handlers
        const SIGHAND = 0x00000800;
        /// Create in new PID namespace
        const NEWPID = 0x20000000;
        /// Share parent
        const PARENT = 0x00008000;
        /// Same thread group
        const THREAD = 0x00010000;
        /// New mount namespace
        const NEWNS = 0x00020000;
        /// Share System V semaphore undo
        const SYSVSEM = 0x00040000;
        /// Set TLS
        const SETTLS = 0x00080000;
        /// Store child TID in parent
        const PARENT_SETTID = 0x00100000;
        /// Clear child TID in child
        const CHILD_CLEARTID = 0x00200000;
        /// Set child TID in child
        const CHILD_SETTID = 0x01000000;
        /// New cgroup namespace
        const NEWCGROUP = 0x02000000;
        /// New UTS namespace
        const NEWUTS = 0x04000000;
        /// New IPC namespace
        const NEWIPC = 0x08000000;
        /// New user namespace
        const NEWUSER = 0x10000000;
        /// New network namespace
        const NEWNET = 0x40000000;
        /// Share I/O context
        const IO = 0x80000000;
    }
}

/// Get thread by TID
pub fn get_thread(tid: Tid) -> Option<CpuContext> {
    let thread_table = THREAD_TABLE.read();
    thread_table.threads[tid.as_u64() as usize]
        .as_ref()
        .map(|t| t.context.clone())
}

/// Update thread context
pub fn set_thread_context(tid: Tid, context: CpuContext) {
    let mut thread_table = THREAD_TABLE.write();
    if let Some(ref mut thread) = thread_table.threads[tid.as_u64() as usize] {
        thread.context = context;
    }
}

/// Get process by PID
pub fn get_process(pid: Pid) -> Option<ProcessInfo> {
    let proc_table = PROCESS_TABLE.read();
    proc_table.processes[pid.as_u64() as usize].as_ref().map(|p| {
        ProcessInfo {
            pid: p.pid,
            parent: p.parent,
            name: p.name.clone(),
            state: p.state,
            priority: p.priority,
            cpu_time: p.cpu_time,
            uid: p.uid,
            gid: p.gid,
        }
    })
}

/// Basic process info (read-only snapshot)
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: Pid,
    pub parent: Option<Pid>,
    pub name: String,
    pub state: ProcessState,
    pub priority: i32,
    pub cpu_time: u64,
    pub uid: u32,
    pub gid: u32,
}

/// Create a new process and load an executable
pub fn spawn(path: &str, args: &[&str], env: &[(&str, &str)]) -> Result<Pid, i32> {
    use crate::elf;
    use crate::memory::{MemoryManager, PAGE_SIZE};

    if !elf::is_elf(path) {
        return Err(-(crate::fs::errno::ENOEXEC as i32));
    }

    let parent_pid = current();
    let pid = alloc_pid();
    let tid = alloc_tid();

    // Allocate kernel stack
    let kernel_stack = {
        let mut mm_guard = MemoryManager::get().lock();
        if let Some(mm) = mm_guard.as_mut() {
            let pages = KERNEL_STACK_SIZE / PAGE_SIZE;
            mm.alloc_pages(pages).ok_or(-(crate::fs::errno::ENOMEM as i32))?
        } else {
            return Err(-(crate::fs::errno::ENOMEM as i32));
        }
    };

    // Get parent's working directory
    let cwd = if let Some(ppid) = parent_pid {
        let proc_table = PROCESS_TABLE.read();
        proc_table.processes[ppid.as_u64() as usize]
            .as_ref()
            .map(|p| p.cwd.clone())
            .unwrap_or_else(|| String::from("/"))
    } else {
        String::from("/")
    };

    // Get parent's credentials
    let (uid, gid, euid, egid) = if let Some(ppid) = parent_pid {
        let proc_table = PROCESS_TABLE.read();
        proc_table.processes[ppid.as_u64() as usize]
            .as_ref()
            .map(|p| (p.uid, p.gid, p.euid, p.egid))
            .unwrap_or((0, 0, 0, 0))
    } else {
        (0, 0, 0, 0)
    };

    // Create process
    let mut process = Process {
        pid,
        parent: parent_pid,
        name: String::from(path),
        state: ProcessState::New,
        threads: Vec::new(),
        memory_space: 0,
        fds: core::array::from_fn(|_| None),
        exit_code: None,
        priority: 0,
        ai_priority_boost: 0.0,
        cpu_time: 0,
        cwd,
        env: env.iter().map(|(k, v)| (String::from(*k), String::from(*v))).collect(),
        signal_state: SignalState::new(),
        pgid: pid,
        sid: pid,
        uid,
        gid,
        euid,
        egid,
        suid: uid,
        sgid: gid,
        groups: Vec::new(),
        rlimits: ResourceLimits::default(),
        robust_list: None,
        child_subreaper: false,
        start_time: 0,
    };

    // Set up standard file descriptors
    process.fds[0] = Some(FileDescriptor { flags: 0, offset: 0, close_on_exec: false });
    process.fds[1] = Some(FileDescriptor { flags: 1, offset: 0, close_on_exec: false });
    process.fds[2] = Some(FileDescriptor { flags: 1, offset: 0, close_on_exec: false });

    // Create thread
    let thread = Thread {
        tid,
        process: pid,
        state: ThreadState::Ready,
        kernel_stack: kernel_stack + KERNEL_STACK_SIZE as u64,
        user_stack: 0,
        context: CpuContext::default(),
        priority: 0,
        tls: 0,
        clear_child_tid: None,
        set_child_tid: None,
        signal_mask: SigSet::empty(),
        alt_stack: None,
    };

    process.threads.push(tid);

    // Store in tables
    {
        let mut proc_table = PROCESS_TABLE.write();
        proc_table.processes[pid.as_u64() as usize] = Some(Box::new(process));
    }

    {
        let mut thread_table = THREAD_TABLE.write();
        thread_table.threads[tid.as_u64() as usize] = Some(Box::new(thread));
    }

    // Load the executable
    match elf::exec(path, args, env, pid) {
        Ok(context) => {
            let user_stack = context.rsp;
            let mut thread_table = THREAD_TABLE.write();
            if let Some(ref mut thread) = thread_table.threads[tid.as_u64() as usize] {
                thread.context = context;
                thread.user_stack = user_stack;
                thread.state = ThreadState::Ready;
            }

            let mut proc_table = PROCESS_TABLE.write();
            if let Some(ref mut proc) = proc_table.processes[pid.as_u64() as usize] {
                proc.state = ProcessState::Ready;
            }

            crate::scheduler::add_thread(tid.as_u64() as u32, 0);
            Ok(pid)
        }
        Err(e) => {
            {
                let mut proc_table = PROCESS_TABLE.write();
                proc_table.processes[pid.as_u64() as usize] = None;
            }
            {
                let mut thread_table = THREAD_TABLE.write();
                thread_table.threads[tid.as_u64() as usize] = None;
            }
            Err(e.to_errno())
        }
    }
}

/// Execute a new program (replaces current process image)
pub fn execve(path: &str, args: &[&str], env: &[(&str, &str)]) -> Result<(), i32> {
    use crate::elf;

    let pid = match current() {
        Some(p) => p,
        None => return Err(-(crate::fs::errno::ESRCH as i32)),
    };

    match elf::exec(path, args, env, pid) {
        Ok(_context) => {
            let mut table = PROCESS_TABLE.write();
            if let Some(ref mut process) = table.processes[pid.as_u64() as usize] {
                process.state = ProcessState::Ready;
                process.name = String::from(path);
                // Reset signal handlers to default on exec
                process.signal_state = SignalState::new();
                // Close close-on-exec file descriptors
                for fd in process.fds.iter_mut() {
                    if let Some(ref f) = fd {
                        if f.close_on_exec {
                            *fd = None;
                        }
                    }
                }
            }
            Ok(())
        }
        Err(e) => Err(e.to_errno()),
    }
}

/// Exit the current process
pub fn exit(code: i32) {
    let pid = match current() {
        Some(p) => p,
        None => return,
    };

    {
        let mut proc_table = PROCESS_TABLE.write();

        // Phase 1: Update current process and collect info we need
        let (parent_pid, is_subreaper, threads_to_terminate) = {
            if let Some(ref mut process) = proc_table.processes[pid.as_u64() as usize] {
                process.state = ProcessState::Zombie;
                process.exit_code = Some(code);

                // Handle robust futex list
                if let Some(robust_head) = process.robust_list {
                    // Would walk and cleanup robust futexes
                    let _ = robust_head;
                }

                // Collect thread info before releasing borrow
                let threads: Vec<_> = process.threads.iter().copied().collect();

                (process.parent, process.child_subreaper, threads)
            } else {
                return;
            }
        };

        // Phase 2: Terminate all threads
        for tid in threads_to_terminate {
            let mut thread_table = THREAD_TABLE.write();
            if let Some(ref mut thread) = thread_table.threads[tid.as_u64() as usize] {
                thread.state = ThreadState::Terminated;
                // Wake any waiters on clear_child_tid
                if let Some(ctid_ptr) = thread.clear_child_tid {
                    unsafe {
                        core::ptr::write_volatile(ctid_ptr, 0);
                    }
                    // Would wake futex on this address
                }
            }
            drop(thread_table);
            crate::scheduler::remove_thread(tid.as_u64() as u32);
        }

        // Phase 3: Send SIGCHLD to parent
        if let Some(parent_pid) = parent_pid {
            if let Some(ref mut parent) = proc_table.processes[parent_pid.as_u64() as usize] {
                let mut info = SigInfo::default();
                info.signo = signal::SIGCHLD;
                info.code = signal::code::CLD_EXITED;
                info.pid = pid.as_u64() as u32;
                info.status = code;
                parent.post_signal(signal::SIGCHLD, Some(info));
            }
        }

        // Phase 4: Reparent children
        let init_pid = Pid::new(1);
        for i in 0..MAX_PROCESSES {
            if let Some(ref mut child) = proc_table.processes[i] {
                if child.parent == Some(pid) {
                    child.parent = if is_subreaper { Some(pid) } else { Some(init_pid) };
                }
            }
        }
    }

    crate::scheduler::yield_now();
}

/// Exit a specific thread
pub fn thread_exit(code: i32) {
    let tid = match current_thread() {
        Some(t) => t,
        None => return,
    };

    let pid = {
        let thread_table = THREAD_TABLE.read();
        thread_table.threads[tid.as_u64() as usize]
            .as_ref()
            .map(|t| t.process)
    };

    {
        let mut thread_table = THREAD_TABLE.write();
        if let Some(ref mut thread) = thread_table.threads[tid.as_u64() as usize] {
            thread.state = ThreadState::Terminated;
            if let Some(ctid_ptr) = thread.clear_child_tid {
                unsafe {
                    core::ptr::write_volatile(ctid_ptr, 0);
                }
            }
        }
    }

    crate::scheduler::remove_thread(tid.as_u64() as u32);

    // Check if last thread
    if let Some(pid) = pid {
        let should_exit = {
            let proc_table = PROCESS_TABLE.read();
            let thread_table = THREAD_TABLE.read();

            if let Some(ref process) = proc_table.processes[pid.as_u64() as usize] {
                process.threads.iter().all(|tid| {
                    thread_table.threads[tid.as_u64() as usize]
                        .as_ref()
                        .map(|t| t.state == ThreadState::Terminated)
                        .unwrap_or(true)
                })
            } else {
                false
            }
        };

        if should_exit {
            exit(code);
        }
    }

    crate::scheduler::yield_now();
}

/// Wait for a child process to exit
pub fn wait(wait_pid: Option<Pid>) -> Option<(Pid, i32)> {
    let parent_pid = current()?;

    loop {
        let result = {
            let proc_table = PROCESS_TABLE.read();

            let mut found = None;
            for i in 0..MAX_PROCESSES {
                if let Some(ref process) = proc_table.processes[i] {
                    if process.parent == Some(parent_pid) {
                        if let Some(wpid) = wait_pid {
                            if process.pid != wpid {
                                continue;
                            }
                        }

                        if process.state == ProcessState::Zombie {
                            found = Some((process.pid, process.exit_code.unwrap_or(0)));
                            break;
                        }
                    }
                }
            }
            found
        };

        if let Some((pid, code)) = result {
            {
                let mut proc_table = PROCESS_TABLE.write();
                let mut thread_table = THREAD_TABLE.write();

                if let Some(process) = proc_table.processes[pid.as_u64() as usize].take() {
                    for tid in &process.threads {
                        thread_table.threads[tid.as_u64() as usize] = None;
                    }
                }
            }

            return Some((pid, code));
        }

        let has_children = {
            let proc_table = PROCESS_TABLE.read();
            (0..MAX_PROCESSES).any(|i| {
                proc_table.processes[i]
                    .as_ref()
                    .map(|p| p.parent == Some(parent_pid))
                    .unwrap_or(false)
            })
        };

        if !has_children {
            return None;
        }

        crate::scheduler::yield_now();
    }
}

/// Wait for a child process with options (waitpid)
pub fn waitpid(pid: i32, options: i32) -> Option<(Pid, i32)> {
    const WNOHANG: i32 = 1;
    const WUNTRACED: i32 = 2;
    const WCONTINUED: i32 = 8;

    let _ = (WUNTRACED, WCONTINUED); // Would handle stopped/continued states

    if pid == -1 {
        if options & WNOHANG != 0 {
            let proc_table = PROCESS_TABLE.read();
            let parent_pid = current()?;

            for i in 0..MAX_PROCESSES {
                if let Some(ref process) = proc_table.processes[i] {
                    if process.parent == Some(parent_pid) && process.state == ProcessState::Zombie {
                        return Some((process.pid, process.exit_code.unwrap_or(0)));
                    }
                }
            }
            None
        } else {
            wait(None)
        }
    } else if pid > 0 {
        let target_pid = Pid::new(pid as u64);
        if options & WNOHANG != 0 {
            let proc_table = PROCESS_TABLE.read();
            proc_table.processes[target_pid.as_u64() as usize]
                .as_ref()
                .filter(|p| p.state == ProcessState::Zombie)
                .map(|p| (p.pid, p.exit_code.unwrap_or(0)))
        } else {
            wait(Some(target_pid))
        }
    } else {
        None
    }
}

/// Get process state
pub fn get_process_state(pid: Pid) -> Option<ProcessState> {
    let proc_table = PROCESS_TABLE.read();
    proc_table.processes[pid.as_u64() as usize]
        .as_ref()
        .map(|p| p.state)
}

/// Get parent process ID
pub fn getppid() -> Option<Pid> {
    let pid = current()?;
    let proc_table = PROCESS_TABLE.read();
    proc_table.processes[pid.as_u64() as usize]
        .as_ref()
        .and_then(|p| p.parent)
}

/// Get process group ID
pub fn getpgid(pid: Pid) -> Option<Pid> {
    let proc_table = PROCESS_TABLE.read();
    proc_table.processes[pid.as_u64() as usize]
        .as_ref()
        .map(|p| p.pgid)
}

/// Set process group ID
pub fn setpgid(pid: Pid, pgid: Pid) -> Result<(), i32> {
    let mut proc_table = PROCESS_TABLE.write();
    if let Some(ref mut process) = proc_table.processes[pid.as_u64() as usize] {
        process.pgid = pgid;
        Ok(())
    } else {
        Err(-3) // ESRCH
    }
}

/// Get session ID
pub fn getsid(pid: Pid) -> Option<Pid> {
    let proc_table = PROCESS_TABLE.read();
    proc_table.processes[pid.as_u64() as usize]
        .as_ref()
        .map(|p| p.sid)
}

/// Create new session
pub fn setsid() -> Result<Pid, i32> {
    let pid = current().ok_or(-3i32)?;

    let mut proc_table = PROCESS_TABLE.write();
    if let Some(ref mut process) = proc_table.processes[pid.as_u64() as usize] {
        // Check if already a session leader
        if process.sid == pid {
            return Err(-1); // EPERM
        }
        process.sid = pid;
        process.pgid = pid;
        Ok(pid)
    } else {
        Err(-3)
    }
}

/// Get a mutable pointer to a thread's CPU context for context switching.
pub fn get_thread_context_ptr(tid: Tid) -> Option<*mut CpuContext> {
    let mut thread_table = THREAD_TABLE.write();
    thread_table.threads[tid.as_u64() as usize]
        .as_mut()
        .map(|t| &mut t.context as *mut CpuContext)
}

/// Check if a thread belongs to a user-space process
pub fn is_user_thread(tid: Tid) -> bool {
    let thread_table = THREAD_TABLE.read();
    thread_table.threads[tid.as_u64() as usize]
        .as_ref()
        .map(|t| t.context.cs == 0x2B || t.context.cs == 0x33)
        .unwrap_or(false)
}

/// Set the current running thread
pub fn set_current_thread(tid: Option<Tid>) {
    let mut thread_table = THREAD_TABLE.write();
    thread_table.current = tid;
}

/// Set the current running process
pub fn set_current_process(pid: Option<Pid>) {
    let mut proc_table = PROCESS_TABLE.write();
    proc_table.current = pid;
}

/// Get thread's owning process
pub fn get_thread_process(tid: Tid) -> Option<Pid> {
    let thread_table = THREAD_TABLE.read();
    thread_table.threads[tid.as_u64() as usize]
        .as_ref()
        .map(|t| t.process)
}

/// Block the current thread
pub fn block_current_thread() {
    if let Some(tid) = current_thread() {
        let mut thread_table = THREAD_TABLE.write();
        if let Some(ref mut thread) = thread_table.threads[tid.as_u64() as usize] {
            thread.state = ThreadState::Blocked;
        }
    }
}

/// Unblock a thread
pub fn unblock_thread(tid: Tid) {
    let mut thread_table = THREAD_TABLE.write();
    if let Some(ref mut thread) = thread_table.threads[tid.as_u64() as usize] {
        if thread.state == ThreadState::Blocked {
            thread.state = ThreadState::Ready;
            crate::scheduler::add_thread(tid.as_u64() as u32, thread.priority as u8);
        }
    }
}

/// Get thread state
pub fn get_thread_state(tid: Tid) -> Option<ThreadState> {
    let thread_table = THREAD_TABLE.read();
    thread_table.threads[tid.as_u64() as usize]
        .as_ref()
        .map(|t| t.state)
}

// =============================================================================
// SIGNAL HANDLING
// =============================================================================

/// Send a signal to a process
pub fn kill(pid: Pid, sig: signal::Signal) -> Result<(), i32> {
    if sig < 0 || sig >= signal::NSIG as i32 {
        return Err(-22); // EINVAL
    }

    let mut proc_table = PROCESS_TABLE.write();
    if let Some(ref mut process) = proc_table.processes[pid.as_u64() as usize] {
        // TODO: Check permissions

        if sig == 0 {
            // Signal 0 is just a permission check
            return Ok(());
        }

        let mut info = SigInfo::default();
        info.signo = sig;
        info.code = signal::code::SI_USER;
        info.pid = current().map(|p| p.as_u64() as u32).unwrap_or(0);

        process.post_signal(sig, Some(info));
        Ok(())
    } else {
        Err(-3) // ESRCH
    }
}

/// Send signal to process group
pub fn killpg(pgid: Pid, sig: signal::Signal) -> Result<i32, i32> {
    let mut count = 0;
    let mut proc_table = PROCESS_TABLE.write();

    for i in 0..MAX_PROCESSES {
        if let Some(ref mut process) = proc_table.processes[i] {
            if process.pgid == pgid {
                let mut info = SigInfo::default();
                info.signo = sig;
                info.code = signal::code::SI_USER;
                process.post_signal(sig, Some(info));
                count += 1;
            }
        }
    }

    if count > 0 {
        Ok(count)
    } else {
        Err(-3) // ESRCH
    }
}

/// Set signal action
pub fn sigaction(
    sig: signal::Signal,
    action: Option<&SigAction>,
) -> Result<SigAction, i32> {
    let pid = current().ok_or(-3i32)?;

    if !signal::can_catch(sig) {
        return Err(-22); // EINVAL
    }

    let mut proc_table = PROCESS_TABLE.write();
    if let Some(ref mut process) = proc_table.processes[pid.as_u64() as usize] {
        let old = process.signal_state.get_action(sig)
            .cloned()
            .unwrap_or_default();

        if let Some(act) = action {
            process.signal_state.set_action(sig, act.clone());
        }

        Ok(old)
    } else {
        Err(-3)
    }
}

/// Set signal mask
pub fn sigprocmask(how: i32, set: Option<&SigSet>) -> Result<SigSet, i32> {
    const SIG_BLOCK: i32 = 0;
    const SIG_UNBLOCK: i32 = 1;
    const SIG_SETMASK: i32 = 2;

    let tid = current_thread().ok_or(-3i32)?;

    let mut thread_table = THREAD_TABLE.write();
    if let Some(ref mut thread) = thread_table.threads[tid.as_u64() as usize] {
        let old = thread.signal_mask;

        if let Some(mask) = set {
            match how {
                SIG_BLOCK => {
                    thread.signal_mask = thread.signal_mask.union(mask);
                }
                SIG_UNBLOCK => {
                    thread.signal_mask = thread.signal_mask.difference(mask);
                }
                SIG_SETMASK => {
                    thread.signal_mask = *mask;
                }
                _ => return Err(-22),
            }
            // Never mask SIGKILL/SIGSTOP
            thread.signal_mask.remove(signal::SIGKILL);
            thread.signal_mask.remove(signal::SIGSTOP);
        }

        Ok(old)
    } else {
        Err(-3)
    }
}

/// Check and deliver signals (called on return to user space)
pub fn do_signal() {
    let pid = match current() {
        Some(pid) => pid,
        None => return,
    };

    loop {
        let (sig, info, action) = {
            let mut proc_table = PROCESS_TABLE.write();
            let process = match proc_table.processes[pid.as_u64() as usize].as_mut() {
                Some(p) => p,
                None => return,
            };

            match process.dequeue_signal() {
                Some((sig, info)) => {
                    let action = process.signal_state.get_action(sig)
                        .cloned()
                        .unwrap_or_default();
                    (sig, info, action)
                }
                None => return,
            }
        };

        // Handle the signal
        match action.handler {
            SigHandler::Default => {
                match signal::default_action(sig) {
                    DefaultAction::Terminate | DefaultAction::CoreDump => {
                        exit(128 + sig);
                        return;
                    }
                    DefaultAction::Stop => {
                        let mut proc_table = PROCESS_TABLE.write();
                        if let Some(ref mut p) = proc_table.processes[pid.as_u64() as usize] {
                            p.state = ProcessState::Stopped;
                        }
                        crate::scheduler::yield_now();
                    }
                    DefaultAction::Continue => {
                        let mut proc_table = PROCESS_TABLE.write();
                        if let Some(ref mut p) = proc_table.processes[pid.as_u64() as usize] {
                            if p.state == ProcessState::Stopped {
                                p.state = ProcessState::Ready;
                            }
                        }
                    }
                    DefaultAction::Ignore => {}
                }
            }
            SigHandler::Ignore => {}
            SigHandler::Handler(handler) | SigHandler::SigAction(handler) => {
                // Would set up signal frame and return to handler
                setup_signal_frame(sig, &action, info.as_ref(), handler);
            }
        }
    }
}

/// Set up signal handler frame on user stack
fn setup_signal_frame(
    sig: signal::Signal,
    _action: &SigAction,
    _info: Option<&SigInfo>,
    handler: usize,
) {
    let tid = match current_thread() {
        Some(tid) => tid,
        None => return,
    };

    let mut thread_table = THREAD_TABLE.write();
    if let Some(ref mut thread) = thread_table.threads[tid.as_u64() as usize] {
        // Would build signal frame with saved context
        let frame_size = 512;
        let new_sp = thread.context.rsp - frame_size;

        // Set up to call handler
        thread.context.rsp = new_sp;
        thread.context.rip = handler as u64;
        thread.context.rdi = sig as u64;
    }
}

/// Restore context after signal handler returns
pub fn sigreturn() -> Result<(), i32> {
    // Would restore saved context from signal frame
    Ok(())
}

/// Set alternate signal stack
pub fn sigaltstack(
    ss: Option<&signal::SignalStack>,
) -> Result<Option<signal::SignalStack>, i32> {
    let tid = current_thread().ok_or(-3i32)?;

    let mut thread_table = THREAD_TABLE.write();
    if let Some(ref mut thread) = thread_table.threads[tid.as_u64() as usize] {
        let old = thread.alt_stack.clone();
        if let Some(stack) = ss {
            thread.alt_stack = Some(stack.clone());
        }
        Ok(old)
    } else {
        Err(-3)
    }
}

// =============================================================================
// CREDENTIAL MANAGEMENT
// =============================================================================

/// Get user ID
pub fn getuid() -> u32 {
    current()
        .and_then(|pid| {
            let proc_table = PROCESS_TABLE.read();
            proc_table.processes[pid.as_u64() as usize]
                .as_ref()
                .map(|p| p.uid)
        })
        .unwrap_or(0)
}

/// Get effective user ID
pub fn geteuid() -> u32 {
    current()
        .and_then(|pid| {
            let proc_table = PROCESS_TABLE.read();
            proc_table.processes[pid.as_u64() as usize]
                .as_ref()
                .map(|p| p.euid)
        })
        .unwrap_or(0)
}

/// Get group ID
pub fn getgid() -> u32 {
    current()
        .and_then(|pid| {
            let proc_table = PROCESS_TABLE.read();
            proc_table.processes[pid.as_u64() as usize]
                .as_ref()
                .map(|p| p.gid)
        })
        .unwrap_or(0)
}

/// Get effective group ID
pub fn getegid() -> u32 {
    current()
        .and_then(|pid| {
            let proc_table = PROCESS_TABLE.read();
            proc_table.processes[pid.as_u64() as usize]
                .as_ref()
                .map(|p| p.egid)
        })
        .unwrap_or(0)
}

/// Set user ID
pub fn setuid(uid: u32) -> Result<(), i32> {
    let pid = current().ok_or(-3i32)?;

    let mut proc_table = PROCESS_TABLE.write();
    if let Some(ref mut process) = proc_table.processes[pid.as_u64() as usize] {
        if process.euid == 0 {
            // Root can set any UID
            process.uid = uid;
            process.euid = uid;
            process.suid = uid;
        } else if uid == process.uid || uid == process.suid {
            process.euid = uid;
        } else {
            return Err(-1); // EPERM
        }
        Ok(())
    } else {
        Err(-3)
    }
}

/// Set group ID
pub fn setgid(gid: u32) -> Result<(), i32> {
    let pid = current().ok_or(-3i32)?;

    let mut proc_table = PROCESS_TABLE.write();
    if let Some(ref mut process) = proc_table.processes[pid.as_u64() as usize] {
        if process.euid == 0 {
            process.gid = gid;
            process.egid = gid;
            process.sgid = gid;
        } else if gid == process.gid || gid == process.sgid {
            process.egid = gid;
        } else {
            return Err(-1);
        }
        Ok(())
    } else {
        Err(-3)
    }
}

/// Set effective user ID
pub fn seteuid(euid: u32) -> Result<(), i32> {
    let pid = current().ok_or(-3i32)?;

    let mut proc_table = PROCESS_TABLE.write();
    if let Some(ref mut process) = proc_table.processes[pid.as_u64() as usize] {
        if process.euid == 0 || euid == process.uid || euid == process.suid {
            process.euid = euid;
            Ok(())
        } else {
            Err(-1)
        }
    } else {
        Err(-3)
    }
}

/// Set effective group ID
pub fn setegid(egid: u32) -> Result<(), i32> {
    let pid = current().ok_or(-3i32)?;

    let mut proc_table = PROCESS_TABLE.write();
    if let Some(ref mut process) = proc_table.processes[pid.as_u64() as usize] {
        if process.euid == 0 || egid == process.gid || egid == process.sgid {
            process.egid = egid;
            Ok(())
        } else {
            Err(-1)
        }
    } else {
        Err(-3)
    }
}

/// Get supplementary groups
pub fn getgroups() -> Vec<u32> {
    current()
        .and_then(|pid| {
            let proc_table = PROCESS_TABLE.read();
            proc_table.processes[pid.as_u64() as usize]
                .as_ref()
                .map(|p| p.groups.clone())
        })
        .unwrap_or_default()
}

/// Set supplementary groups
pub fn setgroups(groups: &[u32]) -> Result<(), i32> {
    let pid = current().ok_or(-3i32)?;

    let mut proc_table = PROCESS_TABLE.write();
    if let Some(ref mut process) = proc_table.processes[pid.as_u64() as usize] {
        if process.euid != 0 {
            return Err(-1); // EPERM
        }
        process.groups = groups.to_vec();
        Ok(())
    } else {
        Err(-3)
    }
}
