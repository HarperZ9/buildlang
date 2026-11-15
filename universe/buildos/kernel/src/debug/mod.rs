// ===============================================================================
// BUILDOS KERNEL - DEBUG/PTRACE SUBSYSTEM
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Process Tracing and Debugging Support
//!
//! Implements ptrace functionality for debuggers:
//! - Process tracing and control
//! - Breakpoint management
//! - Single-stepping
//! - Register access
//! - Memory inspection and modification

#![allow(dead_code)]

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::sync::{Mutex, RwLock};

pub mod breakpoint;
pub mod watchpoint;
pub mod registers;

// =============================================================================
// PTRACE REQUESTS
// =============================================================================

/// Ptrace request types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum PtraceRequest {
    /// Indicate that the process making this request should be traced
    TraceMe = 0,
    /// Return the word in the process's text space at address addr
    PeekText = 1,
    /// Return the word in the process's data space at address addr
    PeekData = 2,
    /// Return the word in the process's user area at offset addr
    PeekUser = 3,
    /// Copy the word data to address addr in the process's text space
    PokeText = 4,
    /// Copy the word data to address addr in the process's data space
    PokeData = 5,
    /// Copy the word data to offset addr in the process's user area
    PokeUser = 6,
    /// Restart the stopped tracee process
    Continue = 7,
    /// Kill the tracee
    Kill = 8,
    /// Restart the stopped tracee with single-step
    SingleStep = 9,
    /// Copy the tracee's general-purpose registers to data
    GetRegs = 12,
    /// Modify the tracee's general-purpose registers
    SetRegs = 13,
    /// Copy the tracee's floating-point registers to data
    GetFpRegs = 14,
    /// Modify the tracee's floating-point registers
    SetFpRegs = 15,
    /// Attach to the process specified in pid
    Attach = 16,
    /// Detach from the tracee
    Detach = 17,
    /// Copy the tracee's extended floating-point registers to data
    GetFpXRegs = 18,
    /// Modify the tracee's extended floating-point registers
    SetFpXRegs = 19,
    /// Continue and stop at the next entry/exit of a system call
    Syscall = 24,
    /// Set ptrace options
    SetOptions = 0x4200,
    /// Retrieve a message about the ptrace event that just happened
    GetEventMsg = 0x4201,
    /// Retrieve siginfo_t structure about signal
    GetSigInfo = 0x4202,
    /// Set siginfo_t structure about signal
    SetSigInfo = 0x4203,
    /// Get register set
    GetRegset = 0x4204,
    /// Set register set
    SetRegset = 0x4205,
    /// Seize the process
    Seize = 0x4206,
    /// Interrupt the process
    Interrupt = 0x4207,
    /// Listen for events
    Listen = 0x4208,
    /// Peek siginfo
    PeekSigInfo = 0x4209,
    /// Get seccomp metadata
    GetSeccompMetadata = 0x420D,
    /// Get syscall info
    GetSyscallInfo = 0x420E,
}

impl PtraceRequest {
    /// Convert from i32
    pub fn from_i32(val: i32) -> Option<Self> {
        match val {
            0 => Some(PtraceRequest::TraceMe),
            1 => Some(PtraceRequest::PeekText),
            2 => Some(PtraceRequest::PeekData),
            3 => Some(PtraceRequest::PeekUser),
            4 => Some(PtraceRequest::PokeText),
            5 => Some(PtraceRequest::PokeData),
            6 => Some(PtraceRequest::PokeUser),
            7 => Some(PtraceRequest::Continue),
            8 => Some(PtraceRequest::Kill),
            9 => Some(PtraceRequest::SingleStep),
            12 => Some(PtraceRequest::GetRegs),
            13 => Some(PtraceRequest::SetRegs),
            14 => Some(PtraceRequest::GetFpRegs),
            15 => Some(PtraceRequest::SetFpRegs),
            16 => Some(PtraceRequest::Attach),
            17 => Some(PtraceRequest::Detach),
            18 => Some(PtraceRequest::GetFpXRegs),
            19 => Some(PtraceRequest::SetFpXRegs),
            24 => Some(PtraceRequest::Syscall),
            0x4200 => Some(PtraceRequest::SetOptions),
            0x4201 => Some(PtraceRequest::GetEventMsg),
            0x4202 => Some(PtraceRequest::GetSigInfo),
            0x4203 => Some(PtraceRequest::SetSigInfo),
            0x4204 => Some(PtraceRequest::GetRegset),
            0x4205 => Some(PtraceRequest::SetRegset),
            0x4206 => Some(PtraceRequest::Seize),
            0x4207 => Some(PtraceRequest::Interrupt),
            0x4208 => Some(PtraceRequest::Listen),
            0x4209 => Some(PtraceRequest::PeekSigInfo),
            0x420D => Some(PtraceRequest::GetSeccompMetadata),
            0x420E => Some(PtraceRequest::GetSyscallInfo),
            _ => None,
        }
    }
}

// =============================================================================
// PTRACE OPTIONS
// =============================================================================

/// Ptrace options
pub mod options {
    /// Trace fork events
    pub const TRACEFORK: u32 = 1 << 1;
    /// Trace vfork events
    pub const TRACEVFORK: u32 = 1 << 2;
    /// Trace clone events
    pub const TRACECLONE: u32 = 1 << 3;
    /// Trace exec events
    pub const TRACEEXEC: u32 = 1 << 4;
    /// Trace vfork done events
    pub const TRACEVFORKDONE: u32 = 1 << 5;
    /// Trace exit events
    pub const TRACEEXIT: u32 = 1 << 6;
    /// Trace seccomp events
    pub const TRACESECCOMP: u32 = 1 << 7;
    /// Kill tracee on exit
    pub const EXITKILL: u32 = 1 << 20;
    /// Suspend seccomp filtering
    pub const SUSPEND_SECCOMP: u32 = 1 << 21;
}

/// Ptrace event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum PtraceEvent {
    /// Fork event
    Fork = 1,
    /// Vfork event
    Vfork = 2,
    /// Clone event
    Clone = 3,
    /// Exec event
    Exec = 4,
    /// Vfork done event
    VforkDone = 5,
    /// Exit event
    Exit = 6,
    /// Seccomp event
    Seccomp = 7,
    /// Stop event
    Stop = 128,
}

// =============================================================================
// TRACEE STATE
// =============================================================================

/// Tracee stop reason
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// Stopped by signal
    Signal(i32),
    /// Stopped at syscall entry
    SyscallEntry,
    /// Stopped at syscall exit
    SyscallExit,
    /// Stopped at event
    Event(PtraceEvent),
    /// Single-step
    SingleStep,
    /// Breakpoint hit
    Breakpoint(u64),
    /// Watchpoint hit
    Watchpoint(u64),
    /// Group stop
    GroupStop,
}

/// Tracee state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceeState {
    /// Running
    Running,
    /// Stopped
    Stopped(StopReason),
    /// Exited
    Exited(i32),
    /// Terminated by signal
    Signaled(i32),
}

/// Tracee information
pub struct Tracee {
    /// Process ID
    pid: u32,
    /// Tracer PID
    tracer_pid: u32,
    /// Current state
    state: TraceeState,
    /// Options
    options: u32,
    /// Single-step mode
    single_step: AtomicBool,
    /// Syscall tracing
    syscall_trace: AtomicBool,
    /// Last syscall entry
    syscall_entry: AtomicBool,
    /// Event message
    event_msg: AtomicU32,
    /// Saved signal
    saved_signal: i32,
    /// Breakpoints
    breakpoints: BTreeSet<u64>,
    /// Watchpoints
    watchpoints: Vec<watchpoint::Watchpoint>,
}

impl Tracee {
    /// Create new tracee
    pub fn new(pid: u32, tracer_pid: u32) -> Self {
        Self {
            pid,
            tracer_pid,
            state: TraceeState::Stopped(StopReason::Signal(19)), // SIGSTOP
            options: 0,
            single_step: AtomicBool::new(false),
            syscall_trace: AtomicBool::new(false),
            syscall_entry: AtomicBool::new(true),
            event_msg: AtomicU32::new(0),
            saved_signal: 0,
            breakpoints: BTreeSet::new(),
            watchpoints: Vec::new(),
        }
    }

    /// Get PID
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Get tracer PID
    pub fn tracer_pid(&self) -> u32 {
        self.tracer_pid
    }

    /// Get state
    pub fn state(&self) -> TraceeState {
        self.state
    }

    /// Set state
    pub fn set_state(&mut self, state: TraceeState) {
        self.state = state;
    }

    /// Check if stopped
    pub fn is_stopped(&self) -> bool {
        matches!(self.state, TraceeState::Stopped(_))
    }

    /// Set options
    pub fn set_options(&mut self, options: u32) {
        self.options = options;
    }

    /// Get options
    pub fn options(&self) -> u32 {
        self.options
    }

    /// Enable single-step
    pub fn enable_single_step(&self) {
        self.single_step.store(true, Ordering::SeqCst);
    }

    /// Disable single-step
    pub fn disable_single_step(&self) {
        self.single_step.store(false, Ordering::SeqCst);
    }

    /// Check if single-stepping
    pub fn is_single_stepping(&self) -> bool {
        self.single_step.load(Ordering::SeqCst)
    }

    /// Enable syscall tracing
    pub fn enable_syscall_trace(&self) {
        self.syscall_trace.store(true, Ordering::SeqCst);
    }

    /// Disable syscall tracing
    pub fn disable_syscall_trace(&self) {
        self.syscall_trace.store(false, Ordering::SeqCst);
    }

    /// Check if tracing syscalls
    pub fn is_tracing_syscalls(&self) -> bool {
        self.syscall_trace.load(Ordering::SeqCst)
    }

    /// Set event message
    pub fn set_event_msg(&self, msg: u32) {
        self.event_msg.store(msg, Ordering::SeqCst);
    }

    /// Get event message
    pub fn event_msg(&self) -> u32 {
        self.event_msg.load(Ordering::SeqCst)
    }

    /// Add breakpoint
    pub fn add_breakpoint(&mut self, addr: u64) {
        self.breakpoints.insert(addr);
    }

    /// Remove breakpoint
    pub fn remove_breakpoint(&mut self, addr: u64) -> bool {
        self.breakpoints.remove(&addr)
    }

    /// Check if breakpoint exists
    pub fn has_breakpoint(&self, addr: u64) -> bool {
        self.breakpoints.contains(&addr)
    }
}

// =============================================================================
// PTRACE MANAGER
// =============================================================================

/// Ptrace error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtraceError {
    /// No such process
    NoSuchProcess,
    /// Permission denied
    PermissionDenied,
    /// Invalid argument
    InvalidArgument,
    /// Process not stopped
    NotStopped,
    /// Already being traced
    AlreadyTraced,
    /// Not being traced
    NotTraced,
    /// Operation not supported
    NotSupported,
    /// Address not mapped
    BadAddress,
}

impl From<PtraceError> for i32 {
    fn from(err: PtraceError) -> i32 {
        match err {
            PtraceError::NoSuchProcess => -3,    // ESRCH
            PtraceError::PermissionDenied => -1, // EPERM
            PtraceError::InvalidArgument => -22, // EINVAL
            PtraceError::NotStopped => -22,      // EINVAL
            PtraceError::AlreadyTraced => -16,   // EBUSY
            PtraceError::NotTraced => -3,        // ESRCH
            PtraceError::NotSupported => -95,    // ENOTSUP
            PtraceError::BadAddress => -14,      // EFAULT
        }
    }
}

/// Ptrace manager
pub struct PtraceManager {
    /// Tracees indexed by PID
    tracees: RwLock<BTreeMap<u32, Arc<Mutex<Tracee>>>>,
    /// Tracers indexed by PID (maps to list of tracees)
    tracers: RwLock<BTreeMap<u32, Vec<u32>>>,
}

impl PtraceManager {
    /// Create new manager
    pub const fn new() -> Self {
        Self {
            tracees: RwLock::new(BTreeMap::new()),
            tracers: RwLock::new(BTreeMap::new()),
        }
    }

    /// Request tracing of current process by parent
    pub fn trace_me(&self, pid: u32, parent_pid: u32) -> Result<(), PtraceError> {
        let mut tracees = self.tracees.write();

        // Check if already being traced
        if tracees.contains_key(&pid) {
            return Err(PtraceError::AlreadyTraced);
        }

        // Create tracee entry
        let tracee = Arc::new(Mutex::new(Tracee::new(pid, parent_pid)));
        tracees.insert(pid, tracee);

        // Add to tracer's list
        let mut tracers = self.tracers.write();
        tracers.entry(parent_pid).or_insert_with(Vec::new).push(pid);

        Ok(())
    }

    /// Attach to process
    pub fn attach(&self, tracer_pid: u32, tracee_pid: u32) -> Result<(), PtraceError> {
        let mut tracees = self.tracees.write();

        // Check if already being traced
        if tracees.contains_key(&tracee_pid) {
            return Err(PtraceError::AlreadyTraced);
        }

        // Check if process exists
        // Would check process table here

        // Create tracee entry
        let tracee = Arc::new(Mutex::new(Tracee::new(tracee_pid, tracer_pid)));
        tracees.insert(tracee_pid, tracee);

        // Add to tracer's list
        let mut tracers = self.tracers.write();
        tracers.entry(tracer_pid).or_insert_with(Vec::new).push(tracee_pid);

        // Send SIGSTOP to tracee
        // Would send signal here

        Ok(())
    }

    /// Seize process (like attach but doesn't stop)
    pub fn seize(&self, tracer_pid: u32, tracee_pid: u32) -> Result<(), PtraceError> {
        let mut tracees = self.tracees.write();

        if tracees.contains_key(&tracee_pid) {
            return Err(PtraceError::AlreadyTraced);
        }

        let mut tracee = Tracee::new(tracee_pid, tracer_pid);
        tracee.state = TraceeState::Running;

        tracees.insert(tracee_pid, Arc::new(Mutex::new(tracee)));

        let mut tracers = self.tracers.write();
        tracers.entry(tracer_pid).or_insert_with(Vec::new).push(tracee_pid);

        Ok(())
    }

    /// Detach from process
    pub fn detach(&self, tracee_pid: u32, sig: i32) -> Result<(), PtraceError> {
        let mut tracees = self.tracees.write();

        let tracee = tracees.remove(&tracee_pid)
            .ok_or(PtraceError::NotTraced)?;

        let tracer_pid = tracee.lock().tracer_pid();

        // Remove from tracer's list
        let mut tracers = self.tracers.write();
        if let Some(list) = tracers.get_mut(&tracer_pid) {
            list.retain(|&pid| pid != tracee_pid);
        }

        // Resume process with signal
        if sig != 0 {
            // Would send signal
        }

        Ok(())
    }

    /// Get tracee
    pub fn get_tracee(&self, pid: u32) -> Option<Arc<Mutex<Tracee>>> {
        self.tracees.read().get(&pid).cloned()
    }

    /// Continue execution
    pub fn continue_execution(&self, pid: u32, sig: i32) -> Result<(), PtraceError> {
        let tracee = self.get_tracee(pid).ok_or(PtraceError::NotTraced)?;
        let mut tracee = tracee.lock();

        if !tracee.is_stopped() {
            return Err(PtraceError::NotStopped);
        }

        tracee.disable_single_step();
        tracee.disable_syscall_trace();
        tracee.saved_signal = sig;
        tracee.set_state(TraceeState::Running);

        // Would resume the process

        Ok(())
    }

    /// Single-step execution
    pub fn single_step(&self, pid: u32, sig: i32) -> Result<(), PtraceError> {
        let tracee = self.get_tracee(pid).ok_or(PtraceError::NotTraced)?;
        let mut tracee = tracee.lock();

        if !tracee.is_stopped() {
            return Err(PtraceError::NotStopped);
        }

        tracee.enable_single_step();
        tracee.saved_signal = sig;
        tracee.set_state(TraceeState::Running);

        // Would set trap flag and resume

        Ok(())
    }

    /// Syscall tracing
    pub fn syscall(&self, pid: u32, sig: i32) -> Result<(), PtraceError> {
        let tracee = self.get_tracee(pid).ok_or(PtraceError::NotTraced)?;
        let mut tracee = tracee.lock();

        if !tracee.is_stopped() {
            return Err(PtraceError::NotStopped);
        }

        tracee.enable_syscall_trace();
        tracee.saved_signal = sig;
        tracee.set_state(TraceeState::Running);

        Ok(())
    }

    /// Kill tracee
    pub fn kill(&self, pid: u32) -> Result<(), PtraceError> {
        let tracee = self.get_tracee(pid).ok_or(PtraceError::NotTraced)?;
        let mut tracee = tracee.lock();

        tracee.set_state(TraceeState::Signaled(9)); // SIGKILL

        // Would send SIGKILL

        Ok(())
    }

    /// Interrupt tracee
    pub fn interrupt(&self, pid: u32) -> Result<(), PtraceError> {
        let tracee = self.get_tracee(pid).ok_or(PtraceError::NotTraced)?;
        let tracee = tracee.lock();

        if !matches!(tracee.state(), TraceeState::Running) {
            return Err(PtraceError::NotStopped);
        }

        // Would send SIGSTOP

        Ok(())
    }

    /// Set options
    pub fn set_options(&self, pid: u32, options: u32) -> Result<(), PtraceError> {
        let tracee = self.get_tracee(pid).ok_or(PtraceError::NotTraced)?;
        let mut tracee = tracee.lock();

        tracee.set_options(options);
        Ok(())
    }

    /// Get event message
    pub fn get_event_msg(&self, pid: u32) -> Result<u32, PtraceError> {
        let tracee = self.get_tracee(pid).ok_or(PtraceError::NotTraced)?;
        let tracee = tracee.lock();

        Ok(tracee.event_msg())
    }

    /// Handle tracee stop
    pub fn handle_stop(&self, pid: u32, reason: StopReason) {
        if let Some(tracee) = self.get_tracee(pid) {
            let mut tracee = tracee.lock();
            tracee.set_state(TraceeState::Stopped(reason));
            // Would wake up tracer waiting on waitpid
        }
    }

    /// Handle tracee exit
    pub fn handle_exit(&self, pid: u32, exit_code: i32) {
        let tracees = self.tracees.write();

        if let Some(tracee) = tracees.get(&pid) {
            let mut t = tracee.lock();
            t.set_state(TraceeState::Exited(exit_code));
        }

        // Clean up is done when tracer calls wait
    }

    /// Peek at process memory
    pub fn peek_data(&self, pid: u32, addr: u64) -> Result<u64, PtraceError> {
        let _tracee = self.get_tracee(pid).ok_or(PtraceError::NotTraced)?;

        // Would read from process memory
        // For now, return placeholder
        let _ = addr;
        Ok(0)
    }

    /// Poke at process memory
    pub fn poke_data(&self, pid: u32, addr: u64, data: u64) -> Result<(), PtraceError> {
        let _tracee = self.get_tracee(pid).ok_or(PtraceError::NotTraced)?;

        // Would write to process memory
        let _ = (addr, data);
        Ok(())
    }

    /// Get tracees for a tracer
    pub fn get_tracees(&self, tracer_pid: u32) -> Vec<u32> {
        self.tracers.read()
            .get(&tracer_pid)
            .cloned()
            .unwrap_or_default()
    }

    /// Check if process is being traced
    pub fn is_traced(&self, pid: u32) -> bool {
        self.tracees.read().contains_key(&pid)
    }

    /// Get tracer of a process
    pub fn get_tracer(&self, pid: u32) -> Option<u32> {
        self.tracees.read()
            .get(&pid)
            .map(|t| t.lock().tracer_pid())
    }
}

/// Global ptrace manager
static PTRACE: PtraceManager = PtraceManager::new();

// =============================================================================
// SYSCALL INTERFACE
// =============================================================================

/// Ptrace syscall
pub fn sys_ptrace(
    request: i32,
    pid: u32,
    addr: u64,
    data: u64,
) -> Result<i64, PtraceError> {
    let request = PtraceRequest::from_i32(request)
        .ok_or(PtraceError::InvalidArgument)?;

    match request {
        PtraceRequest::TraceMe => {
            // Get current PID and parent PID
            let current_pid = 1; // Would get from process
            let parent_pid = 0;
            PTRACE.trace_me(current_pid, parent_pid)?;
            Ok(0)
        }

        PtraceRequest::Attach => {
            let tracer_pid = 1; // Would get from current process
            PTRACE.attach(tracer_pid, pid)?;
            Ok(0)
        }

        PtraceRequest::Seize => {
            let tracer_pid = 1;
            PTRACE.seize(tracer_pid, pid)?;
            Ok(0)
        }

        PtraceRequest::Detach => {
            PTRACE.detach(pid, data as i32)?;
            Ok(0)
        }

        PtraceRequest::Continue => {
            PTRACE.continue_execution(pid, data as i32)?;
            Ok(0)
        }

        PtraceRequest::SingleStep => {
            PTRACE.single_step(pid, data as i32)?;
            Ok(0)
        }

        PtraceRequest::Syscall => {
            PTRACE.syscall(pid, data as i32)?;
            Ok(0)
        }

        PtraceRequest::Kill => {
            PTRACE.kill(pid)?;
            Ok(0)
        }

        PtraceRequest::Interrupt => {
            PTRACE.interrupt(pid)?;
            Ok(0)
        }

        PtraceRequest::SetOptions => {
            PTRACE.set_options(pid, data as u32)?;
            Ok(0)
        }

        PtraceRequest::GetEventMsg => {
            let msg = PTRACE.get_event_msg(pid)?;
            Ok(msg as i64)
        }

        PtraceRequest::PeekText | PtraceRequest::PeekData => {
            let value = PTRACE.peek_data(pid, addr)?;
            Ok(value as i64)
        }

        PtraceRequest::PokeText | PtraceRequest::PokeData => {
            PTRACE.poke_data(pid, addr, data)?;
            Ok(0)
        }

        PtraceRequest::GetRegs => {
            let tracee = PTRACE.get_tracee(pid).ok_or(PtraceError::NotTraced)?;
            let _tracee = tracee.lock();
            // Would copy registers to data
            Ok(0)
        }

        PtraceRequest::SetRegs => {
            let tracee = PTRACE.get_tracee(pid).ok_or(PtraceError::NotTraced)?;
            let _tracee = tracee.lock();
            // Would copy data to registers
            Ok(0)
        }

        _ => Err(PtraceError::NotSupported),
    }
}

/// Initialize debug subsystem
pub fn init() {
    crate::kprintln!("[DEBUG] Ptrace subsystem initialized");
}
