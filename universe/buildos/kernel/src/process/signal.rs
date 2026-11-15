//! Signal Handling
//!
//! POSIX-compatible signal implementation for process control.

use alloc::vec::Vec;

/// Signal number type
pub type Signal = i32;

// Standard signals (POSIX)
pub const SIGHUP: Signal = 1;      // Hangup
pub const SIGINT: Signal = 2;      // Interrupt (Ctrl+C)
pub const SIGQUIT: Signal = 3;     // Quit (Ctrl+\)
pub const SIGILL: Signal = 4;      // Illegal instruction
pub const SIGTRAP: Signal = 5;     // Trace/breakpoint trap
pub const SIGABRT: Signal = 6;     // Abort
pub const SIGBUS: Signal = 7;      // Bus error
pub const SIGFPE: Signal = 8;      // Floating point exception
pub const SIGKILL: Signal = 9;     // Kill (cannot be caught)
pub const SIGUSR1: Signal = 10;    // User-defined 1
pub const SIGSEGV: Signal = 11;    // Segmentation fault
pub const SIGUSR2: Signal = 12;    // User-defined 2
pub const SIGPIPE: Signal = 13;    // Broken pipe
pub const SIGALRM: Signal = 14;    // Alarm clock
pub const SIGTERM: Signal = 15;    // Termination
pub const SIGSTKFLT: Signal = 16;  // Stack fault
pub const SIGCHLD: Signal = 17;    // Child status changed
pub const SIGCONT: Signal = 18;    // Continue if stopped
pub const SIGSTOP: Signal = 19;    // Stop (cannot be caught)
pub const SIGTSTP: Signal = 20;    // Terminal stop (Ctrl+Z)
pub const SIGTTIN: Signal = 21;    // Background read from tty
pub const SIGTTOU: Signal = 22;    // Background write to tty
pub const SIGURG: Signal = 23;     // Urgent data on socket
pub const SIGXCPU: Signal = 24;    // CPU time limit exceeded
pub const SIGXFSZ: Signal = 25;    // File size limit exceeded
pub const SIGVTALRM: Signal = 26;  // Virtual timer expired
pub const SIGPROF: Signal = 27;    // Profiling timer expired
pub const SIGWINCH: Signal = 28;   // Window size changed
pub const SIGIO: Signal = 29;      // I/O possible
pub const SIGPWR: Signal = 30;     // Power failure
pub const SIGSYS: Signal = 31;     // Bad system call

// Real-time signals
pub const SIGRTMIN: Signal = 32;
pub const SIGRTMAX: Signal = 64;

/// Number of standard signals
pub const NSIG: usize = 65;

/// Signal set (bitmask)
#[derive(Clone, Copy, Debug, Default)]
pub struct SigSet(u64);

impl SigSet {
    /// Empty signal set
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Full signal set
    pub const fn full() -> Self {
        Self(!0)
    }

    /// Add a signal to the set
    pub fn add(&mut self, sig: Signal) {
        if sig >= 1 && sig < 64 {
            self.0 |= 1 << (sig - 1);
        }
    }

    /// Remove a signal from the set
    pub fn remove(&mut self, sig: Signal) {
        if sig >= 1 && sig < 64 {
            self.0 &= !(1 << (sig - 1));
        }
    }

    /// Check if signal is in set
    pub fn contains(&self, sig: Signal) -> bool {
        if sig >= 1 && sig < 64 {
            (self.0 & (1 << (sig - 1))) != 0
        } else {
            false
        }
    }

    /// Union of two sets
    pub fn union(&self, other: &Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Intersection of two sets
    pub fn intersect(&self, other: &Self) -> Self {
        Self(self.0 & other.0)
    }

    /// Difference of two sets (remove signals in other from self)
    pub fn difference(&self, other: &Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// Is set empty?
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// Get first signal in set
    pub fn first(&self) -> Option<Signal> {
        if self.0 == 0 {
            None
        } else {
            Some((self.0.trailing_zeros() + 1) as Signal)
        }
    }

    /// Raw value
    pub fn bits(&self) -> u64 {
        self.0
    }
}

/// Signal action
#[derive(Clone)]
pub struct SigAction {
    /// Handler type
    pub handler: SigHandler,
    /// Signal mask during handler
    pub mask: SigSet,
    /// Flags
    pub flags: SigActionFlags,
    /// Restorer function (for returning from signal handler)
    pub restorer: Option<usize>,
}

impl Default for SigAction {
    fn default() -> Self {
        Self {
            handler: SigHandler::Default,
            mask: SigSet::empty(),
            flags: SigActionFlags::empty(),
            restorer: None,
        }
    }
}

/// Signal handler type
#[derive(Clone, Copy, Debug)]
pub enum SigHandler {
    /// Default action
    Default,
    /// Ignore the signal
    Ignore,
    /// Call handler function
    Handler(usize), // Function pointer in user space
    /// Call sigaction-style handler (3 args)
    SigAction(usize),
}

bitflags::bitflags! {
    /// Signal action flags
    #[derive(Clone, Copy, Debug, Default)]
    pub struct SigActionFlags: u32 {
        /// Don't add signal to mask while executing handler
        const SA_NOMASK = 0x40000000;
        /// Restart system calls
        const SA_RESTART = 0x10000000;
        /// Reset handler to default after first signal
        const SA_RESETHAND = 0x80000000;
        /// Don't create zombie on child death
        const SA_NOCLDSTOP = 0x00000001;
        /// Don't send SIGCHLD when children stop
        const SA_NOCLDWAIT = 0x00000002;
        /// Use siginfo_t handler
        const SA_SIGINFO = 0x00000004;
        /// Use alternate signal stack
        const SA_ONSTACK = 0x08000000;
    }
}

/// Signal info (extended information)
#[repr(C)]
#[derive(Clone, Debug)]
pub struct SigInfo {
    /// Signal number
    pub signo: Signal,
    /// Error number
    pub errno: i32,
    /// Signal code (reason)
    pub code: i32,
    /// Sending process ID
    pub pid: u32,
    /// Sending process UID
    pub uid: u32,
    /// Exit value or signal
    pub status: i32,
    /// User time consumed
    pub utime: u64,
    /// System time consumed
    pub stime: u64,
    /// Signal value (for sigqueue)
    pub value: SigVal,
    /// Fault address (for SIGSEGV, SIGBUS)
    pub addr: usize,
}

impl Default for SigInfo {
    fn default() -> Self {
        Self {
            signo: 0,
            errno: 0,
            code: 0,
            pid: 0,
            uid: 0,
            status: 0,
            utime: 0,
            stime: 0,
            value: SigVal { int: 0 },
            addr: 0,
        }
    }
}

/// Signal value union
#[repr(C)]
#[derive(Clone, Copy)]
pub union SigVal {
    /// Integer value
    pub int: i32,
    /// Pointer value
    pub ptr: usize,
}

impl core::fmt::Debug for SigVal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Safe to read int as it's the same size region
        f.debug_struct("SigVal")
            .field("int", unsafe { &self.int })
            .finish()
    }
}

/// Signal codes
pub mod code {
    pub const SI_USER: i32 = 0;      // Sent by kill()
    pub const SI_KERNEL: i32 = 128;  // Sent by kernel
    pub const SI_QUEUE: i32 = -1;    // Sent by sigqueue()
    pub const SI_TIMER: i32 = -2;    // Timer expired
    pub const SI_MESGQ: i32 = -3;    // Message queue
    pub const SI_ASYNCIO: i32 = -4;  // Async I/O completed
    pub const SI_SIGIO: i32 = -5;    // I/O possible

    // SIGCHLD codes
    pub const CLD_EXITED: i32 = 1;   // Child exited
    pub const CLD_KILLED: i32 = 2;   // Child killed
    pub const CLD_DUMPED: i32 = 3;   // Child dumped core
    pub const CLD_TRAPPED: i32 = 4;  // Traced child trapped
    pub const CLD_STOPPED: i32 = 5;  // Child stopped
    pub const CLD_CONTINUED: i32 = 6; // Child continued

    // SIGSEGV codes
    pub const SEGV_MAPERR: i32 = 1;  // Address not mapped
    pub const SEGV_ACCERR: i32 = 2;  // Invalid permissions

    // SIGBUS codes
    pub const BUS_ADRALN: i32 = 1;   // Invalid address alignment
    pub const BUS_ADRERR: i32 = 2;   // Nonexistent physical address
    pub const BUS_OBJERR: i32 = 3;   // Object error

    // SIGFPE codes
    pub const FPE_INTDIV: i32 = 1;   // Integer divide by zero
    pub const FPE_INTOVF: i32 = 2;   // Integer overflow
    pub const FPE_FLTDIV: i32 = 3;   // Float divide by zero
    pub const FPE_FLTOVF: i32 = 4;   // Float overflow
    pub const FPE_FLTUND: i32 = 5;   // Float underflow
    pub const FPE_FLTRES: i32 = 6;   // Float inexact result
    pub const FPE_FLTINV: i32 = 7;   // Invalid float operation
}

/// Default action for a signal
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultAction {
    /// Terminate process
    Terminate,
    /// Terminate and dump core
    CoreDump,
    /// Ignore
    Ignore,
    /// Stop process
    Stop,
    /// Continue if stopped
    Continue,
}

/// Get default action for a signal
pub fn default_action(sig: Signal) -> DefaultAction {
    match sig {
        SIGHUP | SIGINT | SIGKILL | SIGPIPE | SIGALRM | SIGTERM |
        SIGUSR1 | SIGUSR2 | SIGPROF | SIGVTALRM | SIGIO | SIGPWR => {
            DefaultAction::Terminate
        }
        SIGQUIT | SIGILL | SIGABRT | SIGBUS | SIGFPE | SIGSEGV |
        SIGTRAP | SIGSYS | SIGXCPU | SIGXFSZ => {
            DefaultAction::CoreDump
        }
        SIGCHLD | SIGURG | SIGWINCH => {
            DefaultAction::Ignore
        }
        SIGSTOP | SIGTSTP | SIGTTIN | SIGTTOU => {
            DefaultAction::Stop
        }
        SIGCONT => {
            DefaultAction::Continue
        }
        _ if sig >= SIGRTMIN && sig <= SIGRTMAX => {
            DefaultAction::Terminate
        }
        _ => DefaultAction::Ignore,
    }
}

/// Check if signal can be caught/ignored
pub fn can_catch(sig: Signal) -> bool {
    sig != SIGKILL && sig != SIGSTOP
}

/// Per-process signal state
#[derive(Clone)]
pub struct SignalState {
    /// Signal actions
    actions: [SigAction; NSIG],
    /// Pending signals
    pending: SigSet,
    /// Blocked signals
    blocked: SigSet,
    /// Pending signal info queue
    pending_info: Vec<SigInfo>,
    /// Alternative signal stack
    alt_stack: Option<SignalStack>,
}

impl SignalState {
    /// Create new signal state
    pub fn new() -> Self {
        Self {
            actions: core::array::from_fn(|_| SigAction::default()),
            pending: SigSet::empty(),
            blocked: SigSet::empty(),
            pending_info: Vec::new(),
            alt_stack: None,
        }
    }

    /// Get signal action
    pub fn get_action(&self, sig: Signal) -> Option<&SigAction> {
        if sig >= 1 && (sig as usize) < NSIG {
            Some(&self.actions[sig as usize])
        } else {
            None
        }
    }

    /// Set signal action
    pub fn set_action(&mut self, sig: Signal, action: SigAction) -> Option<SigAction> {
        if sig >= 1 && (sig as usize) < NSIG && can_catch(sig) {
            let old = self.actions[sig as usize].clone();
            self.actions[sig as usize] = action;
            Some(old)
        } else {
            None
        }
    }

    /// Post a signal
    pub fn post(&mut self, sig: Signal, info: Option<SigInfo>) {
        if sig >= 1 && sig < 64 {
            self.pending.add(sig);
            if let Some(info) = info {
                self.pending_info.push(info);
            }
        }
    }

    /// Get next pending signal (respecting blocked mask)
    pub fn dequeue(&mut self) -> Option<(Signal, Option<SigInfo>)> {
        // Find first non-blocked pending signal
        let deliverable = SigSet(self.pending.0 & !self.blocked.0);

        if let Some(sig) = deliverable.first() {
            self.pending.remove(sig);

            // Find matching info
            let info = self.pending_info
                .iter()
                .position(|i| i.signo == sig)
                .map(|idx| self.pending_info.remove(idx));

            Some((sig, info))
        } else {
            None
        }
    }

    /// Check if there are pending signals
    pub fn has_pending(&self) -> bool {
        !SigSet(self.pending.0 & !self.blocked.0).is_empty()
    }

    /// Block signals
    pub fn block(&mut self, mask: SigSet) {
        self.blocked = self.blocked.union(&mask);
        // Never block SIGKILL or SIGSTOP
        self.blocked.remove(SIGKILL);
        self.blocked.remove(SIGSTOP);
    }

    /// Unblock signals
    pub fn unblock(&mut self, mask: SigSet) {
        self.blocked.0 &= !mask.0;
    }

    /// Set blocked mask
    pub fn set_blocked(&mut self, mask: SigSet) {
        self.blocked = mask;
        self.blocked.remove(SIGKILL);
        self.blocked.remove(SIGSTOP);
    }

    /// Get blocked mask
    pub fn blocked(&self) -> SigSet {
        self.blocked
    }

    /// Get pending mask
    pub fn pending(&self) -> SigSet {
        self.pending
    }

    /// Set alternate signal stack
    pub fn set_alt_stack(&mut self, stack: SignalStack) {
        self.alt_stack = Some(stack);
    }

    /// Get alternate signal stack
    pub fn alt_stack(&self) -> Option<&SignalStack> {
        self.alt_stack.as_ref()
    }
}

/// Alternate signal stack
#[derive(Clone, Debug)]
pub struct SignalStack {
    /// Stack base address
    pub base: usize,
    /// Stack size
    pub size: usize,
    /// Flags
    pub flags: SignalStackFlags,
}

bitflags::bitflags! {
    /// Signal stack flags
    #[derive(Clone, Copy, Debug, Default)]
    pub struct SignalStackFlags: u32 {
        /// Currently on alt stack
        const SS_ONSTACK = 1;
        /// Alt stack is disabled
        const SS_DISABLE = 2;
        /// Autodisarm after signal
        const SS_AUTODISARM = (1 << 31);
    }
}

/// Signal queue for real-time signals
pub struct SignalQueue {
    /// Queued signals (real-time)
    queue: Vec<(Signal, SigInfo)>,
    /// Maximum queue depth
    max_queued: usize,
}

impl SignalQueue {
    /// Create new signal queue
    pub fn new() -> Self {
        Self {
            queue: Vec::new(),
            max_queued: 32,
        }
    }

    /// Queue a signal
    pub fn queue(&mut self, sig: Signal, info: SigInfo) -> Result<(), SignalError> {
        if self.queue.len() >= self.max_queued {
            return Err(SignalError::QueueFull);
        }
        self.queue.push((sig, info));
        Ok(())
    }

    /// Dequeue a signal
    pub fn dequeue(&mut self, sig: Signal) -> Option<SigInfo> {
        self.queue
            .iter()
            .position(|(s, _)| *s == sig)
            .map(|idx| self.queue.remove(idx).1)
    }

    /// Check if signal is queued
    pub fn is_queued(&self, sig: Signal) -> bool {
        self.queue.iter().any(|(s, _)| *s == sig)
    }
}

/// Signal errors
#[derive(Clone, Debug)]
pub enum SignalError {
    /// Invalid signal number
    InvalidSignal,
    /// Signal cannot be caught/ignored
    NotCatchable,
    /// Signal queue full
    QueueFull,
    /// Permission denied
    PermissionDenied,
    /// Process not found
    ProcessNotFound,
}

/// Signal context (saved state for handler)
#[repr(C)]
pub struct SignalContext {
    /// General purpose registers
    pub regs: [u64; 16],
    /// RIP
    pub rip: u64,
    /// RFLAGS
    pub rflags: u64,
    /// Segment registers
    pub cs: u16,
    pub ss: u16,
    /// FPU state
    pub fpu_state: [u8; 512],
    /// Signal mask to restore
    pub old_mask: SigSet,
}

/// Build signal frame on user stack
pub fn build_signal_frame(
    _sig: Signal,
    _action: &SigAction,
    _info: &SigInfo,
    _context: &SignalContext,
    user_sp: usize,
) -> Result<usize, SignalError> {
    // Would build the signal frame on the user stack:
    // 1. Push signal context
    // 2. Push siginfo (if SA_SIGINFO)
    // 3. Push signal number
    // 4. Set up return address (restorer or __restore_rt)
    // 5. Return new stack pointer

    // For now, return the adjusted stack pointer
    Ok(user_sp.saturating_sub(core::mem::size_of::<SignalContext>() + 256))
}

/// Process pending signals (called on return to user space)
pub fn deliver_pending_signals() {
    // Would:
    // 1. Check for pending non-blocked signals
    // 2. Build signal frame
    // 3. Set up execution to jump to handler
}

/// Check and deliver pending signals
/// Called on return to user space or at safe points
pub fn check_signals() {
    deliver_pending_signals();
}

/// Signal name
pub fn signal_name(sig: Signal) -> &'static str {
    match sig {
        SIGHUP => "SIGHUP",
        SIGINT => "SIGINT",
        SIGQUIT => "SIGQUIT",
        SIGILL => "SIGILL",
        SIGTRAP => "SIGTRAP",
        SIGABRT => "SIGABRT",
        SIGBUS => "SIGBUS",
        SIGFPE => "SIGFPE",
        SIGKILL => "SIGKILL",
        SIGUSR1 => "SIGUSR1",
        SIGSEGV => "SIGSEGV",
        SIGUSR2 => "SIGUSR2",
        SIGPIPE => "SIGPIPE",
        SIGALRM => "SIGALRM",
        SIGTERM => "SIGTERM",
        SIGSTKFLT => "SIGSTKFLT",
        SIGCHLD => "SIGCHLD",
        SIGCONT => "SIGCONT",
        SIGSTOP => "SIGSTOP",
        SIGTSTP => "SIGTSTP",
        SIGTTIN => "SIGTTIN",
        SIGTTOU => "SIGTTOU",
        SIGURG => "SIGURG",
        SIGXCPU => "SIGXCPU",
        SIGXFSZ => "SIGXFSZ",
        SIGVTALRM => "SIGVTALRM",
        SIGPROF => "SIGPROF",
        SIGWINCH => "SIGWINCH",
        SIGIO => "SIGIO",
        SIGPWR => "SIGPWR",
        SIGSYS => "SIGSYS",
        _ if sig >= SIGRTMIN && sig <= SIGRTMAX => "SIGRT",
        _ => "Unknown",
    }
}
