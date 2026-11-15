// ===============================================================================
// BUILDOS KERNEL - FANOTIFY FILE MONITORING
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Fanotify - Filesystem-wide Event Monitoring
//!
//! Provides global filesystem monitoring with:
//! - Per-mount monitoring
//! - Pre-event permission decisions
//! - File access control
//! - Directory modification events

use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::sync::RwLock;

/// Maximum marks per fanotify group
pub const FANOTIFY_MAX_MARKS: usize = 8192;
/// Maximum events queued
pub const FANOTIFY_MAX_QUEUED_EVENTS: usize = 16384;

// =============================================================================
// FANOTIFY EVENT MASKS
// =============================================================================

/// Fanotify event mask flags
pub mod events {
    /// File was accessed
    pub const FAN_ACCESS: u64 = 0x00000001;
    /// File was modified
    pub const FAN_MODIFY: u64 = 0x00000002;
    /// Metadata changed
    pub const FAN_ATTRIB: u64 = 0x00000004;
    /// File closed (write)
    pub const FAN_CLOSE_WRITE: u64 = 0x00000008;
    /// File closed (no write)
    pub const FAN_CLOSE_NOWRITE: u64 = 0x00000010;
    /// File was opened
    pub const FAN_OPEN: u64 = 0x00000020;
    /// File moved from
    pub const FAN_MOVED_FROM: u64 = 0x00000040;
    /// File moved to
    pub const FAN_MOVED_TO: u64 = 0x00000080;
    /// File created
    pub const FAN_CREATE: u64 = 0x00000100;
    /// File deleted
    pub const FAN_DELETE: u64 = 0x00000200;
    /// Self was deleted
    pub const FAN_DELETE_SELF: u64 = 0x00000400;
    /// Self was moved
    pub const FAN_MOVE_SELF: u64 = 0x00000800;
    /// File opened for exec
    pub const FAN_OPEN_EXEC: u64 = 0x00001000;

    // Permission events (require response)
    /// Permission to open
    pub const FAN_OPEN_PERM: u64 = 0x00010000;
    /// Permission to access
    pub const FAN_ACCESS_PERM: u64 = 0x00020000;
    /// Permission to open for exec
    pub const FAN_OPEN_EXEC_PERM: u64 = 0x00040000;

    // Info record types
    /// Filename info
    pub const FAN_EVENT_INFO_TYPE_FID: u8 = 1;
    /// Directory handle info
    pub const FAN_EVENT_INFO_TYPE_DFID_NAME: u8 = 2;
    /// Directory handle info
    pub const FAN_EVENT_INFO_TYPE_DFID: u8 = 3;
    /// PID info
    pub const FAN_EVENT_INFO_TYPE_PIDFD: u8 = 4;
    /// Error info
    pub const FAN_EVENT_INFO_TYPE_ERROR: u8 = 5;

    // Special flags
    /// Event for directory
    pub const FAN_ONDIR: u64 = 0x40000000;
    /// Event queue overflowed
    pub const FAN_Q_OVERFLOW: u64 = 0x00004000;

    // Close combination
    pub const FAN_CLOSE: u64 = FAN_CLOSE_WRITE | FAN_CLOSE_NOWRITE;
    /// Move combination
    pub const FAN_MOVE: u64 = FAN_MOVED_FROM | FAN_MOVED_TO;

    // Init flags
    /// Use fd for event info
    pub const FAN_CLASS_NOTIF: u32 = 0x00000000;
    /// Pre-content class
    pub const FAN_CLASS_CONTENT: u32 = 0x00000004;
    /// Pre-exec class
    pub const FAN_CLASS_PRE_CONTENT: u32 = 0x00000008;
    /// Close-on-exec
    pub const FAN_CLOEXEC: u32 = 0x00000001;
    /// Non-blocking
    pub const FAN_NONBLOCK: u32 = 0x00000002;
    /// Unlimited queue
    pub const FAN_UNLIMITED_QUEUE: u32 = 0x00000010;
    /// Unlimited marks
    pub const FAN_UNLIMITED_MARKS: u32 = 0x00000020;
    /// Report TID
    pub const FAN_ENABLE_AUDIT: u32 = 0x00000040;
    /// Report FID
    pub const FAN_REPORT_FID: u32 = 0x00000200;
    /// Report directory FID
    pub const FAN_REPORT_DIR_FID: u32 = 0x00000400;
    /// Report name
    pub const FAN_REPORT_NAME: u32 = 0x00000800;
    /// Report target FID
    pub const FAN_REPORT_TARGET_FID: u32 = 0x00001000;
    /// Report PIDFD
    pub const FAN_REPORT_PIDFD: u32 = 0x00000080;

    // Mark flags
    /// Add to mark
    pub const FAN_MARK_ADD: u32 = 0x00000001;
    /// Remove from mark
    pub const FAN_MARK_REMOVE: u32 = 0x00000002;
    /// Don't follow symlinks
    pub const FAN_MARK_DONT_FOLLOW: u32 = 0x00000004;
    /// Only dir
    pub const FAN_MARK_ONLYDIR: u32 = 0x00000008;
    /// Ignore mask
    pub const FAN_MARK_IGNORED_MASK: u32 = 0x00000020;
    /// Ignore survives modify
    pub const FAN_MARK_IGNORED_SURV_MODIFY: u32 = 0x00000040;
    /// Flush marks
    pub const FAN_MARK_FLUSH: u32 = 0x00000080;
    /// Evictable mark
    pub const FAN_MARK_EVICTABLE: u32 = 0x00000200;

    // Mark types
    /// Mark inode
    pub const FAN_MARK_INODE: u32 = 0x00000000;
    /// Mark mount
    pub const FAN_MARK_MOUNT: u32 = 0x00000010;
    /// Mark filesystem
    pub const FAN_MARK_FILESYSTEM: u32 = 0x00000100;
}

// =============================================================================
// FANOTIFY EVENT
// =============================================================================

/// Fanotify event structure
#[derive(Clone, Debug)]
pub struct FanotifyEvent {
    /// Event mask
    pub mask: u64,
    /// File descriptor (for permission events)
    pub fd: i32,
    /// Process ID that triggered event
    pub pid: u32,
    /// Optional path
    pub path: Option<String>,
    /// Optional filename
    pub name: Option<String>,
}

impl FanotifyEvent {
    /// Create new event
    pub fn new(mask: u64, fd: i32, pid: u32) -> Self {
        Self {
            mask,
            fd,
            pid,
            path: None,
            name: None,
        }
    }

    /// Create event with path info
    pub fn with_path(mask: u64, fd: i32, pid: u32, path: String, name: Option<String>) -> Self {
        Self {
            mask,
            fd,
            pid,
            path: Some(path),
            name,
        }
    }

    /// Check if this is a permission event
    pub fn is_permission_event(&self) -> bool {
        (self.mask & (events::FAN_OPEN_PERM | events::FAN_ACCESS_PERM | events::FAN_OPEN_EXEC_PERM)) != 0
    }

    /// Serialize to event metadata structure
    pub fn to_metadata(&self) -> FanotifyEventMetadata {
        FanotifyEventMetadata {
            event_len: 24, // Size of metadata structure
            vers: 3,       // FANOTIFY_METADATA_VERSION
            reserved: 0,
            metadata_len: 24,
            mask: self.mask,
            fd: self.fd,
            pid: self.pid as i32,
        }
    }
}

/// Fanotify event metadata (as returned to userspace)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FanotifyEventMetadata {
    pub event_len: u32,
    pub vers: u8,
    pub reserved: u8,
    pub metadata_len: u16,
    pub mask: u64,
    pub fd: i32,
    pub pid: i32,
}

// =============================================================================
// FANOTIFY RESPONSE
// =============================================================================

/// Response to permission event
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FanotifyResponse {
    /// File descriptor from event
    pub fd: i32,
    /// Response: FAN_ALLOW or FAN_DENY
    pub response: u32,
}

/// Allow access
pub const FAN_ALLOW: u32 = 0x01;
/// Deny access
pub const FAN_DENY: u32 = 0x02;
/// Audit the response
pub const FAN_AUDIT: u32 = 0x10;

// =============================================================================
// MARK TYPES
// =============================================================================

/// Mark entry
struct Mark {
    /// Mark type
    mark_type: MarkType,
    /// Target identifier
    target: u64,
    /// Event mask
    mask: u64,
    /// Ignored mask
    ignored_mask: u64,
    /// Flags
    flags: u32,
}

/// Mark type
#[derive(Clone, Copy, PartialEq, Eq)]
enum MarkType {
    /// Inode mark
    Inode,
    /// Mount mark
    Mount,
    /// Filesystem mark
    Filesystem,
}

// =============================================================================
// FANOTIFY GROUP
// =============================================================================

/// Fanotify group (instance)
pub struct FanotifyGroup {
    /// Group ID (file descriptor)
    id: i32,
    /// Initialization flags
    init_flags: u32,
    /// Event flags
    event_flags: u32,
    /// Marks
    marks: RwLock<Vec<Mark>>,
    /// Event queue
    events: RwLock<VecDeque<FanotifyEvent>>,
    /// Pending permission events
    pending_perms: RwLock<BTreeMap<i32, FanotifyEvent>>,
    /// Next permission event fd
    next_perm_fd: AtomicU32,
    /// Notification class
    class: u32,
}

impl FanotifyGroup {
    /// Create new fanotify group
    pub fn new(id: i32, init_flags: u32, event_flags: u32) -> Self {
        let class = init_flags & 0x0C; // Extract class bits

        Self {
            id,
            init_flags,
            event_flags,
            marks: RwLock::new(Vec::new()),
            events: RwLock::new(VecDeque::new()),
            pending_perms: RwLock::new(BTreeMap::new()),
            next_perm_fd: AtomicU32::new(1),
            class,
        }
    }

    /// Add mark
    pub fn add_mark(
        &self,
        flags: u32,
        mask: u64,
        _dirfd: i32,
        pathname: Option<&str>,
    ) -> Result<(), FanotifyError> {
        let marks = self.marks.read();
        if marks.len() >= FANOTIFY_MAX_MARKS {
            return Err(FanotifyError::TooManyMarks);
        }
        drop(marks);

        let mark_type = if flags & events::FAN_MARK_MOUNT != 0 {
            MarkType::Mount
        } else if flags & events::FAN_MARK_FILESYSTEM != 0 {
            MarkType::Filesystem
        } else {
            MarkType::Inode
        };

        // Would resolve pathname to target identifier
        let target = 0u64;

        let mark = Mark {
            mark_type,
            target,
            mask,
            ignored_mask: 0,
            flags,
        };

        self.marks.write().push(mark);

        crate::kprintln!("[FANOTIFY] Added mark on {:?}", pathname);

        Ok(())
    }

    /// Remove mark
    pub fn remove_mark(
        &self,
        _flags: u32,
        _mask: u64,
        _dirfd: i32,
        _pathname: Option<&str>,
    ) -> Result<(), FanotifyError> {
        // Would find and remove matching mark
        Ok(())
    }

    /// Queue event
    pub fn queue_event(&self, event: FanotifyEvent) -> Result<(), FanotifyError> {
        let mut events = self.events.write();

        if events.len() >= FANOTIFY_MAX_QUEUED_EVENTS {
            // Queue overflow event
            events.push_back(FanotifyEvent::new(events::FAN_Q_OVERFLOW, -1, 0));
            return Err(FanotifyError::QueueOverflow);
        }

        events.push_back(event);
        Ok(())
    }

    /// Queue permission event and wait for response
    pub fn queue_permission_event(&self, mut event: FanotifyEvent) -> Result<u32, FanotifyError> {
        let fd = self.next_perm_fd.fetch_add(1, Ordering::Relaxed) as i32;
        event.fd = fd;

        self.pending_perms.write().insert(fd, event.clone());
        self.queue_event(event)?;

        // Would wait for response
        Ok(FAN_ALLOW)
    }

    /// Respond to permission event
    pub fn respond(&self, response: &FanotifyResponse) -> Result<(), FanotifyError> {
        let _event = self.pending_perms.write().remove(&response.fd)
            .ok_or(FanotifyError::InvalidFd)?;

        crate::kprintln!("[FANOTIFY] Response for fd {}: {}",
            response.fd,
            if response.response == FAN_ALLOW { "ALLOW" } else { "DENY" });

        Ok(())
    }

    /// Read events
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, FanotifyError> {
        let mut events = self.events.write();

        if events.is_empty() {
            if self.init_flags & events::FAN_NONBLOCK != 0 {
                return Err(FanotifyError::WouldBlock);
            }
            return Ok(0);
        }

        let mut written = 0;
        let metadata_size = core::mem::size_of::<FanotifyEventMetadata>();

        while let Some(event) = events.front() {
            if written + metadata_size > buf.len() {
                break;
            }

            let metadata = event.to_metadata();
            let bytes = unsafe {
                core::slice::from_raw_parts(
                    &metadata as *const _ as *const u8,
                    metadata_size,
                )
            };

            buf[written..written + metadata_size].copy_from_slice(bytes);
            written += metadata_size;
            events.pop_front();
        }

        Ok(written)
    }

    /// Check if events pending
    pub fn has_events(&self) -> bool {
        !self.events.read().is_empty()
    }

    /// Check if event matches marks
    pub fn matches(&self, inode: u64, mount_id: u64, fs_id: u64, mask: u64) -> bool {
        for mark in self.marks.read().iter() {
            let target_match = match mark.mark_type {
                MarkType::Inode => mark.target == inode,
                MarkType::Mount => mark.target == mount_id,
                MarkType::Filesystem => mark.target == fs_id,
            };

            if target_match && (mark.mask & mask) != 0 && (mark.ignored_mask & mask) == 0 {
                return true;
            }
        }
        false
    }
}

// =============================================================================
// FANOTIFY ERROR
// =============================================================================

/// Fanotify error
#[derive(Clone, Debug)]
pub enum FanotifyError {
    /// Too many marks
    TooManyMarks,
    /// Queue overflow
    QueueOverflow,
    /// Would block
    WouldBlock,
    /// Invalid file descriptor
    InvalidFd,
    /// Permission denied
    PermissionDenied,
    /// Invalid argument
    InvalidArgument,
}

impl FanotifyError {
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::TooManyMarks => -28,  // ENOSPC
            Self::QueueOverflow => -28, // ENOSPC
            Self::WouldBlock => -11,    // EAGAIN
            Self::InvalidFd => -9,      // EBADF
            Self::PermissionDenied => -1, // EPERM
            Self::InvalidArgument => -22, // EINVAL
        }
    }
}

// =============================================================================
// GLOBAL STATE
// =============================================================================

/// Global fanotify groups
static GROUPS: RwLock<BTreeMap<i32, Arc<FanotifyGroup>>> = RwLock::new(BTreeMap::new());

/// Next group ID
static NEXT_ID: AtomicU32 = AtomicU32::new(200);

// =============================================================================
// SYSCALL IMPLEMENTATIONS
// =============================================================================

/// fanotify_init syscall
pub fn sys_fanotify_init(flags: u32, event_f_flags: u32) -> Result<i32, FanotifyError> {
    // Check for CAP_SYS_ADMIN for certain classes
    let class = flags & 0x0C;
    if class == events::FAN_CLASS_CONTENT || class == events::FAN_CLASS_PRE_CONTENT {
        // Would check CAP_SYS_ADMIN
    }

    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed) as i32;
    let group = Arc::new(FanotifyGroup::new(id, flags, event_f_flags));

    GROUPS.write().insert(id, group);

    crate::kprintln!("[FANOTIFY] Created group {} (flags: 0x{:x})", id, flags);

    Ok(id)
}

/// fanotify_mark syscall
pub fn sys_fanotify_mark(
    fanotify_fd: i32,
    flags: u32,
    mask: u64,
    dirfd: i32,
    pathname: Option<&str>,
) -> Result<(), FanotifyError> {
    let groups = GROUPS.read();
    let group = groups.get(&fanotify_fd).ok_or(FanotifyError::InvalidFd)?;

    if flags & events::FAN_MARK_ADD != 0 {
        group.add_mark(flags, mask, dirfd, pathname)
    } else if flags & events::FAN_MARK_REMOVE != 0 {
        group.remove_mark(flags, mask, dirfd, pathname)
    } else if flags & events::FAN_MARK_FLUSH != 0 {
        // Clear all marks
        group.marks.write().clear();
        Ok(())
    } else {
        Err(FanotifyError::InvalidArgument)
    }
}

/// Close fanotify group
pub fn close_group(fd: i32) {
    GROUPS.write().remove(&fd);
}

/// Read from fanotify fd
pub fn read_group(fd: i32, buf: &mut [u8]) -> Result<usize, FanotifyError> {
    let groups = GROUPS.read();
    let group = groups.get(&fd).ok_or(FanotifyError::InvalidFd)?;

    group.read(buf)
}

/// Write response to fanotify fd
pub fn write_group(fd: i32, response: &FanotifyResponse) -> Result<(), FanotifyError> {
    let groups = GROUPS.read();
    let group = groups.get(&fd).ok_or(FanotifyError::InvalidFd)?;

    group.respond(response)
}

// =============================================================================
// EVENT GENERATION
// =============================================================================

/// Check permission for file access
pub fn check_permission(inode: u64, mount_id: u64, fs_id: u64, mask: u64, pid: u32) -> u32 {
    let groups = GROUPS.read();

    for group in groups.values() {
        if group.matches(inode, mount_id, fs_id, mask) {
            let event = FanotifyEvent::new(mask, -1, pid);
            if let Ok(response) = group.queue_permission_event(event) {
                if response == FAN_DENY {
                    return FAN_DENY;
                }
            }
        }
    }

    FAN_ALLOW
}

/// Generate notification event
pub fn notify(inode: u64, mount_id: u64, fs_id: u64, mask: u64, pid: u32, path: Option<&str>) {
    let groups = GROUPS.read();

    for group in groups.values() {
        if group.matches(inode, mount_id, fs_id, mask) {
            let event = if let Some(p) = path {
                FanotifyEvent::with_path(mask, -1, pid, p.into(), None)
            } else {
                FanotifyEvent::new(mask, -1, pid)
            };
            let _ = group.queue_event(event);
        }
    }
}

/// Initialize fanotify subsystem
pub fn init() {
    crate::kprintln!("[FS] Fanotify filesystem monitoring initialized");
}
