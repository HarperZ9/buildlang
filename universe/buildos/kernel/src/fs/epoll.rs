// ===============================================================================
// BUILDOS KERNEL - EPOLL
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Epoll - Scalable I/O Event Notification
//!
//! Provides efficient I/O multiplexing for large numbers of file descriptors.
//! Superior to poll/select for high-connection scenarios.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::sync::RwLock;

/// Maximum epoll instances per process
pub const EPOLL_MAX_INSTANCES: usize = 1024;
/// Maximum fds per epoll instance
pub const EPOLL_MAX_FDS: usize = 65536;
/// Maximum events per epoll_wait call
pub const EPOLL_MAX_EVENTS: usize = 4096;

// =============================================================================
// EPOLL EVENT MASKS
// =============================================================================

/// Epoll event flags
pub mod events {
    /// File descriptor is readable
    pub const EPOLLIN: u32 = 0x001;
    /// File descriptor is writable
    pub const EPOLLOUT: u32 = 0x004;
    /// Error condition
    pub const EPOLLERR: u32 = 0x008;
    /// Hang up (peer closed connection)
    pub const EPOLLHUP: u32 = 0x010;
    /// Urgent data available
    pub const EPOLLPRI: u32 = 0x002;
    /// Stream socket peer closed connection
    pub const EPOLLRDHUP: u32 = 0x2000;
    /// Exclusive wakeup
    pub const EPOLLEXCLUSIVE: u32 = 1 << 28;
    /// Wake once
    pub const EPOLLWAKEUP: u32 = 1 << 29;
    /// One-shot behavior
    pub const EPOLLONESHOT: u32 = 1 << 30;
    /// Edge-triggered
    pub const EPOLLET: u32 = 1 << 31;
}

/// Epoll control operations
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum EpollOp {
    /// Add file descriptor
    Add = 1,
    /// Remove file descriptor
    Del = 2,
    /// Modify file descriptor
    Mod = 3,
}

impl EpollOp {
    pub fn from_i32(val: i32) -> Option<Self> {
        match val {
            1 => Some(Self::Add),
            2 => Some(Self::Del),
            3 => Some(Self::Mod),
            _ => None,
        }
    }
}

// =============================================================================
// EPOLL EVENT
// =============================================================================

/// Epoll event structure
#[repr(C)]
#[derive(Clone, Copy)]
pub struct EpollEvent {
    /// Events mask
    pub events: u32,
    /// User data
    pub data: EpollData,
}

/// Epoll data union (simplified as u64)
#[repr(C)]
#[derive(Clone, Copy)]
pub union EpollData {
    pub ptr: u64,
    pub fd: i32,
    pub u32_val: u32,
    pub u64_val: u64,
}

impl Default for EpollData {
    fn default() -> Self {
        Self { u64_val: 0 }
    }
}

impl EpollEvent {
    pub fn new(events: u32, data: u64) -> Self {
        Self {
            events,
            data: EpollData { u64_val: data },
        }
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> [u8; 12] {
        let mut buf = [0u8; 12];
        buf[0..4].copy_from_slice(&self.events.to_ne_bytes());
        buf[4..12].copy_from_slice(&unsafe { self.data.u64_val }.to_ne_bytes());
        buf
    }
}

// =============================================================================
// EPOLL ITEM
// =============================================================================

/// Epoll item (registered fd)
#[derive(Clone)]
struct EpollItem {
    /// File descriptor
    fd: i32,
    /// Event mask
    events: u32,
    /// User data
    data: u64,
    /// Is edge-triggered
    edge_triggered: bool,
    /// Is one-shot
    one_shot: bool,
    /// Has pending events (for one-shot)
    pending: bool,
}

impl EpollItem {
    fn new(fd: i32, event: &EpollEvent) -> Self {
        Self {
            fd,
            events: event.events & 0x0FFFFFFF, // Mask out flags
            data: unsafe { event.data.u64_val },
            edge_triggered: (event.events & events::EPOLLET) != 0,
            one_shot: (event.events & events::EPOLLONESHOT) != 0,
            pending: false,
        }
    }

    fn update(&mut self, event: &EpollEvent) {
        self.events = event.events & 0x0FFFFFFF;
        self.data = unsafe { event.data.u64_val };
        self.edge_triggered = (event.events & events::EPOLLET) != 0;
        self.one_shot = (event.events & events::EPOLLONESHOT) != 0;
        self.pending = false;
    }
}

// =============================================================================
// EPOLL INSTANCE
// =============================================================================

/// Epoll instance
pub struct EpollInstance {
    /// Epoll file descriptor
    fd: i32,
    /// Registered file descriptors
    items: RwLock<BTreeMap<i32, EpollItem>>,
    /// Ready list (fds with pending events)
    ready: RwLock<VecDeque<i32>>,
    /// Flags
    flags: u32,
}

impl EpollInstance {
    /// Create new epoll instance
    pub fn new(fd: i32, flags: u32) -> Self {
        Self {
            fd,
            items: RwLock::new(BTreeMap::new()),
            ready: RwLock::new(VecDeque::new()),
            flags,
        }
    }

    /// Add file descriptor
    pub fn add(&self, fd: i32, event: &EpollEvent) -> Result<(), EpollError> {
        let mut items = self.items.write();

        if items.contains_key(&fd) {
            return Err(EpollError::AlreadyExists);
        }

        if items.len() >= EPOLL_MAX_FDS {
            return Err(EpollError::TooManyFds);
        }

        items.insert(fd, EpollItem::new(fd, event));

        crate::kprintln!("[EPOLL] Added fd {} to epoll {} (events: 0x{:x})",
            fd, self.fd, event.events);

        Ok(())
    }

    /// Remove file descriptor
    pub fn del(&self, fd: i32) -> Result<(), EpollError> {
        self.items.write().remove(&fd)
            .map(|_| ())
            .ok_or(EpollError::NotFound)?;

        // Remove from ready list
        self.ready.write().retain(|&f| f != fd);

        crate::kprintln!("[EPOLL] Removed fd {} from epoll {}", fd, self.fd);

        Ok(())
    }

    /// Modify file descriptor
    pub fn modify(&self, fd: i32, event: &EpollEvent) -> Result<(), EpollError> {
        let mut items = self.items.write();

        let item = items.get_mut(&fd).ok_or(EpollError::NotFound)?;
        item.update(event);

        crate::kprintln!("[EPOLL] Modified fd {} in epoll {} (events: 0x{:x})",
            fd, self.fd, event.events);

        Ok(())
    }

    /// Control operation
    pub fn ctl(&self, op: EpollOp, fd: i32, event: Option<&EpollEvent>) -> Result<(), EpollError> {
        match op {
            EpollOp::Add => {
                let event = event.ok_or(EpollError::InvalidEvent)?;
                self.add(fd, event)
            }
            EpollOp::Del => self.del(fd),
            EpollOp::Mod => {
                let event = event.ok_or(EpollError::InvalidEvent)?;
                self.modify(fd, event)
            }
        }
    }

    /// Wait for events
    pub fn wait(&self, events: &mut [EpollEvent], timeout_ms: i32) -> Result<usize, EpollError> {
        if events.is_empty() {
            return Err(EpollError::InvalidEvent);
        }

        let max_events = events.len().min(EPOLL_MAX_EVENTS);

        // Check all registered fds for ready state
        self.poll_fds();

        let mut ready = self.ready.write();
        let mut count = 0;

        while count < max_events && !ready.is_empty() {
            if let Some(fd) = ready.pop_front() {
                let items = self.items.read();
                if let Some(item) = items.get(&fd) {
                    // Get current events for fd
                    let current_events = self.get_fd_events(fd);
                    let masked = current_events & item.events;

                    if masked != 0 {
                        events[count] = EpollEvent {
                            events: masked,
                            data: EpollData { u64_val: item.data },
                        };
                        count += 1;

                        // Handle one-shot
                        if item.one_shot {
                            drop(items);
                            if let Some(item) = self.items.write().get_mut(&fd) {
                                item.events = 0;
                            }
                        } else if !item.edge_triggered {
                            // Level-triggered: re-add to ready list if still has events
                            if masked != 0 {
                                ready.push_back(fd);
                            }
                        }
                    }
                }
            }
        }

        // Handle timeout
        if count == 0 && timeout_ms != 0 {
            if timeout_ms < 0 {
                // Would block indefinitely
            } else {
                // Would block with timeout
            }
        }

        Ok(count)
    }

    /// Poll all registered fds
    fn poll_fds(&self) {
        let items = self.items.read();
        let mut ready = self.ready.write();

        for (fd, item) in items.iter() {
            let current_events = self.get_fd_events(*fd);
            if (current_events & item.events) != 0 && !item.pending {
                if !ready.contains(fd) {
                    ready.push_back(*fd);
                }
            }
        }
    }

    /// Get current events for fd (would check actual fd state)
    fn get_fd_events(&self, _fd: i32) -> u32 {
        // Would check actual fd state
        // For now, return EPOLLIN for readable, EPOLLOUT for writable
        events::EPOLLIN | events::EPOLLOUT
    }

    /// Notify event on fd (called when fd state changes)
    pub fn notify(&self, fd: i32, revents: u32) {
        let items = self.items.read();

        if let Some(item) = items.get(&fd) {
            if (revents & item.events) != 0 {
                drop(items);

                let mut ready = self.ready.write();
                if !ready.contains(&fd) {
                    ready.push_back(fd);
                }
            }
        }
    }

    /// Check if has pending events
    pub fn has_events(&self) -> bool {
        !self.ready.read().is_empty()
    }
}

// =============================================================================
// EPOLL ERROR
// =============================================================================

/// Epoll error
#[derive(Clone, Debug)]
pub enum EpollError {
    /// File descriptor already exists
    AlreadyExists,
    /// File descriptor not found
    NotFound,
    /// Too many file descriptors
    TooManyFds,
    /// Invalid file descriptor
    InvalidFd,
    /// Invalid event
    InvalidEvent,
    /// Would block
    WouldBlock,
    /// Interrupted
    Interrupted,
    /// Invalid operation
    InvalidOp,
}

impl EpollError {
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::AlreadyExists => -17,  // EEXIST
            Self::NotFound => -2,        // ENOENT
            Self::TooManyFds => -24,     // EMFILE
            Self::InvalidFd => -9,       // EBADF
            Self::InvalidEvent => -22,   // EINVAL
            Self::WouldBlock => -11,     // EAGAIN
            Self::Interrupted => -4,     // EINTR
            Self::InvalidOp => -22,      // EINVAL
        }
    }
}

// =============================================================================
// GLOBAL STATE
// =============================================================================

/// Global epoll instances
static EPOLLS: RwLock<BTreeMap<i32, Arc<EpollInstance>>> = RwLock::new(BTreeMap::new());

/// Next epoll fd
static NEXT_FD: AtomicU32 = AtomicU32::new(4000);

// =============================================================================
// SYSCALL IMPLEMENTATIONS
// =============================================================================

/// epoll_create syscall
pub fn sys_epoll_create(size: i32) -> Result<i32, EpollError> {
    // size is ignored but must be positive
    if size <= 0 {
        return Err(EpollError::InvalidEvent);
    }
    sys_epoll_create1(0)
}

/// epoll_create1 syscall
pub fn sys_epoll_create1(flags: u32) -> Result<i32, EpollError> {
    let fd = NEXT_FD.fetch_add(1, Ordering::Relaxed) as i32;
    let epoll = Arc::new(EpollInstance::new(fd, flags));

    EPOLLS.write().insert(fd, epoll);

    crate::kprintln!("[EPOLL] Created epoll {} (flags: 0x{:x})", fd, flags);

    Ok(fd)
}

/// epoll_ctl syscall
pub fn sys_epoll_ctl(epfd: i32, op: i32, fd: i32, event: Option<&EpollEvent>) -> Result<(), EpollError> {
    let op = EpollOp::from_i32(op).ok_or(EpollError::InvalidOp)?;

    let epolls = EPOLLS.read();
    let epoll = epolls.get(&epfd).ok_or(EpollError::InvalidFd)?;

    epoll.ctl(op, fd, event)
}

/// epoll_wait syscall
pub fn sys_epoll_wait(epfd: i32, events: &mut [EpollEvent], timeout: i32) -> Result<usize, EpollError> {
    let epolls = EPOLLS.read();
    let epoll = epolls.get(&epfd).ok_or(EpollError::InvalidFd)?;

    epoll.wait(events, timeout)
}

/// epoll_pwait syscall (with signal mask)
pub fn sys_epoll_pwait(
    epfd: i32,
    events: &mut [EpollEvent],
    timeout: i32,
    _sigmask: Option<u64>,
) -> Result<usize, EpollError> {
    // Would apply signal mask during wait
    sys_epoll_wait(epfd, events, timeout)
}

/// epoll_pwait2 syscall (with timespec timeout)
pub fn sys_epoll_pwait2(
    epfd: i32,
    events: &mut [EpollEvent],
    timeout: Option<&super::timerfd::Timespec>,
    _sigmask: Option<u64>,
) -> Result<usize, EpollError> {
    let timeout_ms = timeout
        .map(|t| (t.tv_sec * 1000 + t.tv_nsec / 1_000_000) as i32)
        .unwrap_or(-1);

    sys_epoll_wait(epfd, events, timeout_ms)
}

/// Close epoll instance
pub fn close(fd: i32) -> Result<(), EpollError> {
    EPOLLS.write().remove(&fd)
        .map(|_| ())
        .ok_or(EpollError::InvalidFd)
}

/// Check if fd is epoll
pub fn is_epoll(fd: i32) -> bool {
    EPOLLS.read().contains_key(&fd)
}

/// Notify all epolls about fd event
pub fn notify_fd(fd: i32, revents: u32) {
    for epoll in EPOLLS.read().values() {
        epoll.notify(fd, revents);
    }
}

/// Initialize epoll subsystem
pub fn init() {
    crate::kprintln!("[FS] Epoll scalable I/O initialized");
}
