// ===============================================================================
// BUILDOS KERNEL - INOTIFY FILE MONITORING
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Inotify - File System Event Monitoring
//!
//! Provides per-file/directory monitoring of filesystem events:
//! - File access, modification, and metadata changes
//! - Directory entry creation, deletion, moves
//! - Efficient event coalescing

use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::sync::RwLock;

/// Maximum watches per inotify instance
pub const INOTIFY_MAX_WATCHES: usize = 8192;
/// Maximum events queued per instance
pub const INOTIFY_MAX_QUEUED_EVENTS: usize = 16384;
/// Maximum event name length
pub const INOTIFY_NAME_MAX: usize = 255;

// =============================================================================
// INOTIFY EVENT MASKS
// =============================================================================

/// Inotify event mask flags
pub mod events {
    /// File was accessed
    pub const IN_ACCESS: u32 = 0x00000001;
    /// File was modified
    pub const IN_MODIFY: u32 = 0x00000002;
    /// Metadata changed
    pub const IN_ATTRIB: u32 = 0x00000004;
    /// File opened for writing was closed
    pub const IN_CLOSE_WRITE: u32 = 0x00000008;
    /// File not opened for writing was closed
    pub const IN_CLOSE_NOWRITE: u32 = 0x00000010;
    /// File was opened
    pub const IN_OPEN: u32 = 0x00000020;
    /// File moved from watched directory
    pub const IN_MOVED_FROM: u32 = 0x00000040;
    /// File moved to watched directory
    pub const IN_MOVED_TO: u32 = 0x00000080;
    /// File/directory created in watched directory
    pub const IN_CREATE: u32 = 0x00000100;
    /// File/directory deleted from watched directory
    pub const IN_DELETE: u32 = 0x00000200;
    /// Watched file/directory was deleted
    pub const IN_DELETE_SELF: u32 = 0x00000400;
    /// Watched file/directory was moved
    pub const IN_MOVE_SELF: u32 = 0x00000800;

    // Combination masks
    /// Close events combined
    pub const IN_CLOSE: u32 = IN_CLOSE_WRITE | IN_CLOSE_NOWRITE;
    /// Move events combined
    pub const IN_MOVE: u32 = IN_MOVED_FROM | IN_MOVED_TO;
    /// All events
    pub const IN_ALL_EVENTS: u32 = IN_ACCESS | IN_MODIFY | IN_ATTRIB |
        IN_CLOSE_WRITE | IN_CLOSE_NOWRITE | IN_OPEN |
        IN_MOVED_FROM | IN_MOVED_TO | IN_CREATE | IN_DELETE |
        IN_DELETE_SELF | IN_MOVE_SELF;

    // Special flags
    /// Only watch path if it is a directory
    pub const IN_ONLYDIR: u32 = 0x01000000;
    /// Don't dereference symlink
    pub const IN_DONT_FOLLOW: u32 = 0x02000000;
    /// Ignore events for unlinked files
    pub const IN_EXCL_UNLINK: u32 = 0x04000000;
    /// Add to existing watch mask
    pub const IN_MASK_ADD: u32 = 0x20000000;
    /// Event occurred against dir
    pub const IN_ISDIR: u32 = 0x40000000;
    /// Only send event once
    pub const IN_ONESHOT: u32 = 0x80000000;

    // Output-only events
    /// Watch was removed
    pub const IN_IGNORED: u32 = 0x00008000;
    /// Event queue overflowed
    pub const IN_Q_OVERFLOW: u32 = 0x00004000;
    /// Filesystem unmounted
    pub const IN_UNMOUNT: u32 = 0x00002000;
}

// =============================================================================
// INOTIFY EVENT
// =============================================================================

/// Inotify event structure
#[derive(Clone, Debug)]
pub struct InotifyEvent {
    /// Watch descriptor
    pub wd: i32,
    /// Event mask
    pub mask: u32,
    /// Cookie for rename correlation
    pub cookie: u32,
    /// Optional filename (for directory events)
    pub name: Option<String>,
}

impl InotifyEvent {
    /// Create new event
    pub fn new(wd: i32, mask: u32, cookie: u32, name: Option<String>) -> Self {
        Self { wd, mask, cookie, name }
    }

    /// Get event size (for reading from fd)
    pub fn size(&self) -> usize {
        // inotify_event struct size + name length + null terminator
        16 + self.name.as_ref().map(|n| n.len() + 1).unwrap_or(0)
    }

    /// Check if event is for directory
    pub fn is_dir(&self) -> bool {
        (self.mask & events::IN_ISDIR) != 0
    }

    /// Serialize event to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let name_len = self.name.as_ref().map(|n| n.len() + 1).unwrap_or(0);
        let padded_len = (name_len + 3) & !3; // Align to 4 bytes

        let mut buf = Vec::with_capacity(16 + padded_len);

        // wd (i32)
        buf.extend_from_slice(&self.wd.to_ne_bytes());
        // mask (u32)
        buf.extend_from_slice(&self.mask.to_ne_bytes());
        // cookie (u32)
        buf.extend_from_slice(&self.cookie.to_ne_bytes());
        // len (u32)
        buf.extend_from_slice(&(padded_len as u32).to_ne_bytes());
        // name (if present)
        if let Some(ref name) = self.name {
            buf.extend_from_slice(name.as_bytes());
            buf.push(0); // Null terminator
            // Pad to 4-byte boundary
            while buf.len() < 16 + padded_len {
                buf.push(0);
            }
        }

        buf
    }
}

// =============================================================================
// WATCH DESCRIPTOR
// =============================================================================

/// Watch entry
struct Watch {
    /// Watch descriptor
    wd: i32,
    /// Watched path
    path: String,
    /// Watched inode
    inode: u64,
    /// Event mask
    mask: u32,
}

impl Watch {
    fn new(wd: i32, path: String, inode: u64, mask: u32) -> Self {
        Self { wd, path, inode, mask }
    }

    /// Check if event matches this watch
    fn matches(&self, mask: u32) -> bool {
        (self.mask & mask) != 0
    }
}

// =============================================================================
// INOTIFY INSTANCE
// =============================================================================

/// Inotify instance
pub struct InotifyInstance {
    /// Instance ID (file descriptor)
    id: i32,
    /// Next watch descriptor
    next_wd: AtomicU32,
    /// Watches by descriptor
    watches: RwLock<BTreeMap<i32, Watch>>,
    /// Watches by inode (for fast event delivery)
    inode_watches: RwLock<BTreeMap<u64, Vec<i32>>>,
    /// Event queue
    events: RwLock<VecDeque<InotifyEvent>>,
    /// Event cookie counter (for move correlation)
    cookie_counter: AtomicU32,
    /// Flags (O_NONBLOCK, O_CLOEXEC)
    flags: u32,
}

impl InotifyInstance {
    /// Create new inotify instance
    pub fn new(id: i32, flags: u32) -> Self {
        Self {
            id,
            next_wd: AtomicU32::new(1),
            watches: RwLock::new(BTreeMap::new()),
            inode_watches: RwLock::new(BTreeMap::new()),
            events: RwLock::new(VecDeque::new()),
            cookie_counter: AtomicU32::new(1),
            flags,
        }
    }

    /// Add watch
    pub fn add_watch(&self, path: &str, inode: u64, mask: u32) -> Result<i32, InotifyError> {
        // Check for existing watch on same path
        let existing_wd = {
            let watches = self.watches.read();
            if watches.len() >= INOTIFY_MAX_WATCHES {
                return Err(InotifyError::TooManyWatches);
            }

            let mut found_wd = None;
            for (wd, watch) in watches.iter() {
                if watch.path == path {
                    found_wd = Some(*wd);
                    break;
                }
            }
            found_wd
        };

        if let Some(wd) = existing_wd {
            let mut watches = self.watches.write();
            if let Some(watch) = watches.get_mut(&wd) {
                if mask & events::IN_MASK_ADD != 0 {
                    // Add to existing mask
                    watch.mask |= mask & !events::IN_MASK_ADD;
                } else {
                    // Replace existing watch
                    watch.mask = mask;
                }
            }
            return Ok(wd);
        }

        // Create new watch
        let wd = self.next_wd.fetch_add(1, Ordering::Relaxed) as i32;
        let watch = Watch::new(wd, path.into(), inode, mask);

        self.watches.write().insert(wd, watch);

        // Add to inode index
        self.inode_watches.write()
            .entry(inode)
            .or_insert_with(Vec::new)
            .push(wd);

        crate::kprintln!("[INOTIFY] Added watch {} on {}", wd, path);

        Ok(wd)
    }

    /// Remove watch
    pub fn rm_watch(&self, wd: i32) -> Result<(), InotifyError> {
        let watch = self.watches.write().remove(&wd)
            .ok_or(InotifyError::InvalidWatch)?;

        // Remove from inode index
        if let Some(wds) = self.inode_watches.write().get_mut(&watch.inode) {
            wds.retain(|&w| w != wd);
        }

        // Queue IN_IGNORED event
        self.queue_event(InotifyEvent::new(wd, events::IN_IGNORED, 0, None));

        crate::kprintln!("[INOTIFY] Removed watch {}", wd);

        Ok(())
    }

    /// Queue event
    pub fn queue_event(&self, event: InotifyEvent) {
        let mut events = self.events.write();

        // Check for overflow
        if events.len() >= INOTIFY_MAX_QUEUED_EVENTS {
            // Add overflow event if not already present
            if events.back().map(|e| e.mask != events::IN_Q_OVERFLOW).unwrap_or(true) {
                events.push_back(InotifyEvent::new(-1, events::IN_Q_OVERFLOW, 0, None));
            }
            return;
        }

        // Coalesce with previous event if same
        if let Some(last) = events.back() {
            if last.wd == event.wd && last.mask == event.mask && last.name == event.name {
                // Skip duplicate event
                return;
            }
        }

        events.push_back(event);
    }

    /// Generate event for inode
    pub fn generate_event(&self, inode: u64, mask: u32, name: Option<&str>) {
        let inode_watches = self.inode_watches.read();
        let watches = self.watches.read();

        if let Some(wds) = inode_watches.get(&inode) {
            for &wd in wds {
                if let Some(watch) = watches.get(&wd) {
                    if watch.matches(mask) {
                        let event = InotifyEvent::new(
                            wd,
                            mask,
                            0,
                            name.map(String::from),
                        );
                        drop(watches);
                        drop(inode_watches);
                        self.queue_event(event);
                        return;
                    }
                }
            }
        }
    }

    /// Read events
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, InotifyError> {
        let mut events = self.events.write();

        if events.is_empty() {
            if self.flags & 0o4000 != 0 { // O_NONBLOCK
                return Err(InotifyError::WouldBlock);
            }
            // Would block and wait for events
            return Ok(0);
        }

        let mut written = 0;
        while let Some(event) = events.front() {
            let bytes = event.to_bytes();
            if written + bytes.len() > buf.len() {
                if written == 0 {
                    return Err(InotifyError::BufferTooSmall);
                }
                break;
            }

            buf[written..written + bytes.len()].copy_from_slice(&bytes);
            written += bytes.len();
            events.pop_front();
        }

        Ok(written)
    }

    /// Check if events pending
    pub fn has_events(&self) -> bool {
        !self.events.read().is_empty()
    }

    /// Generate move cookie
    pub fn next_cookie(&self) -> u32 {
        self.cookie_counter.fetch_add(1, Ordering::Relaxed)
    }
}

// =============================================================================
// INOTIFY ERROR
// =============================================================================

/// Inotify error
#[derive(Clone, Debug)]
pub enum InotifyError {
    /// Too many watches
    TooManyWatches,
    /// Invalid watch descriptor
    InvalidWatch,
    /// Would block
    WouldBlock,
    /// Buffer too small
    BufferTooSmall,
    /// Path not found
    PathNotFound,
    /// Permission denied
    PermissionDenied,
}

impl InotifyError {
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::TooManyWatches => -28,  // ENOSPC
            Self::InvalidWatch => -22,     // EINVAL
            Self::WouldBlock => -11,       // EAGAIN
            Self::BufferTooSmall => -22,   // EINVAL
            Self::PathNotFound => -2,      // ENOENT
            Self::PermissionDenied => -13, // EACCES
        }
    }
}

// =============================================================================
// GLOBAL INOTIFY STATE
// =============================================================================

/// Global inotify instances
static INSTANCES: RwLock<BTreeMap<i32, Arc<InotifyInstance>>> = RwLock::new(BTreeMap::new());

/// Next instance ID
static NEXT_ID: AtomicU32 = AtomicU32::new(100);

// =============================================================================
// SYSCALL IMPLEMENTATIONS
// =============================================================================

/// inotify_init syscall
pub fn sys_inotify_init() -> Result<i32, InotifyError> {
    sys_inotify_init1(0)
}

/// inotify_init1 syscall
pub fn sys_inotify_init1(flags: u32) -> Result<i32, InotifyError> {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed) as i32;
    let instance = Arc::new(InotifyInstance::new(id, flags));

    INSTANCES.write().insert(id, instance);

    crate::kprintln!("[INOTIFY] Created instance {}", id);

    Ok(id)
}

/// inotify_add_watch syscall
pub fn sys_inotify_add_watch(fd: i32, path: &str, mask: u32) -> Result<i32, InotifyError> {
    let instances = INSTANCES.read();
    let instance = instances.get(&fd).ok_or(InotifyError::InvalidWatch)?;

    // Would resolve path to inode
    let inode = 0u64; // Placeholder

    instance.add_watch(path, inode, mask)
}

/// inotify_rm_watch syscall
pub fn sys_inotify_rm_watch(fd: i32, wd: i32) -> Result<(), InotifyError> {
    let instances = INSTANCES.read();
    let instance = instances.get(&fd).ok_or(InotifyError::InvalidWatch)?;

    instance.rm_watch(wd)
}

/// Close inotify instance
pub fn close_instance(fd: i32) {
    INSTANCES.write().remove(&fd);
}

/// Read from inotify fd
pub fn read_instance(fd: i32, buf: &mut [u8]) -> Result<usize, InotifyError> {
    let instances = INSTANCES.read();
    let instance = instances.get(&fd).ok_or(InotifyError::InvalidWatch)?;

    instance.read(buf)
}

// =============================================================================
// EVENT GENERATION (called from VFS)
// =============================================================================

/// Notify file access
pub fn notify_access(inode: u64, name: Option<&str>) {
    let instances = INSTANCES.read();
    for instance in instances.values() {
        instance.generate_event(inode, events::IN_ACCESS, name);
    }
}

/// Notify file modification
pub fn notify_modify(inode: u64, name: Option<&str>) {
    let instances = INSTANCES.read();
    for instance in instances.values() {
        instance.generate_event(inode, events::IN_MODIFY, name);
    }
}

/// Notify attribute change
pub fn notify_attrib(inode: u64, name: Option<&str>) {
    let instances = INSTANCES.read();
    for instance in instances.values() {
        instance.generate_event(inode, events::IN_ATTRIB, name);
    }
}

/// Notify file open
pub fn notify_open(inode: u64, name: Option<&str>) {
    let instances = INSTANCES.read();
    for instance in instances.values() {
        instance.generate_event(inode, events::IN_OPEN, name);
    }
}

/// Notify file close (write)
pub fn notify_close_write(inode: u64, name: Option<&str>) {
    let instances = INSTANCES.read();
    for instance in instances.values() {
        instance.generate_event(inode, events::IN_CLOSE_WRITE, name);
    }
}

/// Notify file close (no write)
pub fn notify_close_nowrite(inode: u64, name: Option<&str>) {
    let instances = INSTANCES.read();
    for instance in instances.values() {
        instance.generate_event(inode, events::IN_CLOSE_NOWRITE, name);
    }
}

/// Notify file creation
pub fn notify_create(parent_inode: u64, name: &str, is_dir: bool) {
    let mask = events::IN_CREATE | if is_dir { events::IN_ISDIR } else { 0 };
    let instances = INSTANCES.read();
    for instance in instances.values() {
        instance.generate_event(parent_inode, mask, Some(name));
    }
}

/// Notify file deletion
pub fn notify_delete(parent_inode: u64, name: &str, is_dir: bool) {
    let mask = events::IN_DELETE | if is_dir { events::IN_ISDIR } else { 0 };
    let instances = INSTANCES.read();
    for instance in instances.values() {
        instance.generate_event(parent_inode, mask, Some(name));
    }
}

/// Initialize inotify subsystem
pub fn init() {
    crate::kprintln!("[FS] Inotify file monitoring initialized");
}
