// ===============================================================================
// BUILDOS KERNEL - POSIX SHARED MEMORY (SHMFS)
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! POSIX Shared Memory Filesystem (shmfs/tmpfs)
//!
//! Provides POSIX shared memory objects via shm_open/shm_unlink.
//! Backed by anonymous memory pages.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::sync::RwLock;

/// Maximum shared memory name length
pub const SHM_NAME_MAX: usize = 255;
/// Default maximum size (can be overridden)
pub const SHM_SIZE_MAX: u64 = 1024 * 1024 * 1024; // 1GB

// =============================================================================
// SHM FLAGS
// =============================================================================

/// Shared memory open flags
pub mod flags {
    /// Create if doesn't exist
    pub const O_CREAT: i32 = 0o100;
    /// Exclusive create
    pub const O_EXCL: i32 = 0o200;
    /// Read only
    pub const O_RDONLY: i32 = 0;
    /// Read/write
    pub const O_RDWR: i32 = 2;
    /// Truncate
    pub const O_TRUNC: i32 = 0o1000;
}

// =============================================================================
// SHM OBJECT
// =============================================================================

/// Shared memory object
pub struct ShmObject {
    /// Object name
    name: String,
    /// Size in bytes
    size: AtomicU64,
    /// Mode (permissions)
    mode: u32,
    /// Owner UID
    uid: u32,
    /// Owner GID
    gid: u32,
    /// Creation time
    ctime: u64,
    /// Modification time
    mtime: AtomicU64,
    /// Access time
    atime: u64,
    /// Reference count
    refs: AtomicU32,
    /// Data pages (simplified as Vec<u8>)
    data: RwLock<Vec<u8>>,
    /// Is unlinked (will be deleted when refs = 0)
    unlinked: core::sync::atomic::AtomicBool,
}

impl ShmObject {
    /// Create new shared memory object
    pub fn new(name: String, mode: u32) -> Self {
        let now = get_current_time();

        Self {
            name,
            size: AtomicU64::new(0),
            mode,
            uid: 0,
            gid: 0,
            ctime: now,
            mtime: AtomicU64::new(now),
            atime: now,
            refs: AtomicU32::new(1),
            data: RwLock::new(Vec::new()),
            unlinked: core::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Get size
    pub fn size(&self) -> u64 {
        self.size.load(Ordering::Relaxed)
    }

    /// Truncate/resize object
    pub fn truncate(&self, size: u64) -> Result<(), ShmError> {
        if size > SHM_SIZE_MAX {
            return Err(ShmError::TooBig);
        }

        let mut data = self.data.write();
        data.resize(size as usize, 0);
        self.size.store(size, Ordering::Release);
        self.mtime.store(get_current_time(), Ordering::Release);

        Ok(())
    }

    /// Read from object
    pub fn read(&self, offset: u64, buf: &mut [u8]) -> Result<usize, ShmError> {
        let data = self.data.read();
        let size = self.size.load(Ordering::Relaxed);

        if offset >= size {
            return Ok(0);
        }

        let available = (size - offset) as usize;
        let to_read = buf.len().min(available);

        buf[..to_read].copy_from_slice(&data[offset as usize..offset as usize + to_read]);

        Ok(to_read)
    }

    /// Write to object
    pub fn write(&self, offset: u64, buf: &[u8]) -> Result<usize, ShmError> {
        let mut data = self.data.write();
        let end = offset as usize + buf.len();

        // Extend if necessary
        if end > data.len() {
            if end as u64 > SHM_SIZE_MAX {
                return Err(ShmError::TooBig);
            }
            data.resize(end, 0);
            self.size.store(end as u64, Ordering::Release);
        }

        data[offset as usize..end].copy_from_slice(buf);
        self.mtime.store(get_current_time(), Ordering::Release);

        Ok(buf.len())
    }

    /// Increment reference count
    pub fn add_ref(&self) {
        self.refs.fetch_add(1, Ordering::AcqRel);
    }

    /// Decrement reference count
    pub fn release(&self) -> bool {
        self.refs.fetch_sub(1, Ordering::AcqRel) == 1
    }

    /// Get reference count
    pub fn ref_count(&self) -> u32 {
        self.refs.load(Ordering::Relaxed)
    }

    /// Mark as unlinked
    pub fn unlink(&self) {
        self.unlinked.store(true, Ordering::Release);
    }

    /// Check if unlinked
    pub fn is_unlinked(&self) -> bool {
        self.unlinked.load(Ordering::Relaxed)
    }

    /// Get metadata
    pub fn stat(&self) -> ShmStat {
        ShmStat {
            size: self.size.load(Ordering::Relaxed),
            mode: self.mode,
            uid: self.uid,
            gid: self.gid,
            atime: self.atime,
            mtime: self.mtime.load(Ordering::Relaxed),
            ctime: self.ctime,
        }
    }
}

/// Shared memory statistics
#[derive(Clone)]
pub struct ShmStat {
    pub size: u64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub atime: u64,
    pub mtime: u64,
    pub ctime: u64,
}

// =============================================================================
// SHM FILE DESCRIPTOR
// =============================================================================

/// Shared memory file descriptor
pub struct ShmFd {
    /// File descriptor number
    fd: i32,
    /// Shared memory object
    object: Arc<ShmObject>,
    /// Open flags
    flags: i32,
    /// Current position
    position: AtomicU64,
}

impl ShmFd {
    fn new(fd: i32, object: Arc<ShmObject>, flags: i32) -> Self {
        Self {
            fd,
            object,
            flags,
            position: AtomicU64::new(0),
        }
    }

    /// Read from shm
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, ShmError> {
        let pos = self.position.load(Ordering::Relaxed);
        let read = self.object.read(pos, buf)?;
        self.position.fetch_add(read as u64, Ordering::Relaxed);
        Ok(read)
    }

    /// Write to shm
    pub fn write(&self, buf: &[u8]) -> Result<usize, ShmError> {
        if self.flags & flags::O_RDWR == 0 && self.flags & 1 == 0 {
            return Err(ShmError::ReadOnly);
        }

        let pos = self.position.load(Ordering::Relaxed);
        let written = self.object.write(pos, buf)?;
        self.position.fetch_add(written as u64, Ordering::Relaxed);
        Ok(written)
    }

    /// Seek
    pub fn seek(&self, offset: i64, whence: i32) -> Result<u64, ShmError> {
        let size = self.object.size();
        let current = self.position.load(Ordering::Relaxed);

        let new_pos = match whence {
            0 => offset as u64, // SEEK_SET
            1 => { // SEEK_CUR
                if offset < 0 {
                    current.saturating_sub((-offset) as u64)
                } else {
                    current.saturating_add(offset as u64)
                }
            }
            2 => { // SEEK_END
                if offset < 0 {
                    size.saturating_sub((-offset) as u64)
                } else {
                    size.saturating_add(offset as u64)
                }
            }
            _ => return Err(ShmError::InvalidArg),
        };

        self.position.store(new_pos, Ordering::Relaxed);
        Ok(new_pos)
    }

    /// Truncate
    pub fn ftruncate(&self, size: u64) -> Result<(), ShmError> {
        if self.flags & flags::O_RDWR == 0 && self.flags & 1 == 0 {
            return Err(ShmError::ReadOnly);
        }
        self.object.truncate(size)
    }

    /// Get stat
    pub fn fstat(&self) -> ShmStat {
        self.object.stat()
    }
}

impl Drop for ShmFd {
    fn drop(&mut self) {
        if self.object.release() && self.object.is_unlinked() {
            // Object was unlinked and this was last reference
            // Would free backing pages
        }
    }
}

// =============================================================================
// SHM ERROR
// =============================================================================

/// Shared memory error
#[derive(Clone, Debug)]
pub enum ShmError {
    /// Object not found
    NotFound,
    /// Object already exists
    AlreadyExists,
    /// Permission denied
    PermissionDenied,
    /// Name too long
    NameTooLong,
    /// Invalid name
    InvalidName,
    /// Size too big
    TooBig,
    /// Read only
    ReadOnly,
    /// Invalid argument
    InvalidArg,
    /// Too many open files
    TooManyFds,
}

impl ShmError {
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::NotFound => -2,        // ENOENT
            Self::AlreadyExists => -17,  // EEXIST
            Self::PermissionDenied => -13, // EACCES
            Self::NameTooLong => -36,    // ENAMETOOLONG
            Self::InvalidName => -22,    // EINVAL
            Self::TooBig => -27,         // EFBIG
            Self::ReadOnly => -30,       // EROFS
            Self::InvalidArg => -22,     // EINVAL
            Self::TooManyFds => -24,     // EMFILE
        }
    }
}

// =============================================================================
// GLOBAL STATE
// =============================================================================

/// Global shared memory objects
static SHM_OBJECTS: RwLock<BTreeMap<String, Arc<ShmObject>>> = RwLock::new(BTreeMap::new());

/// Global shm file descriptors
static SHM_FDS: RwLock<BTreeMap<i32, Arc<ShmFd>>> = RwLock::new(BTreeMap::new());

/// Next fd
static NEXT_FD: AtomicU32 = AtomicU32::new(5000);

/// Get current time (placeholder)
fn get_current_time() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Validate shm name
fn validate_name(name: &str) -> Result<&str, ShmError> {
    // Must start with /
    if !name.starts_with('/') {
        return Err(ShmError::InvalidName);
    }

    let name = &name[1..]; // Remove leading /

    if name.is_empty() || name.len() > SHM_NAME_MAX {
        return Err(ShmError::NameTooLong);
    }

    // Cannot contain another /
    if name.contains('/') {
        return Err(ShmError::InvalidName);
    }

    Ok(name)
}

// =============================================================================
// SYSCALL IMPLEMENTATIONS
// =============================================================================

/// shm_open syscall
pub fn sys_shm_open(name: &str, oflag: i32, mode: u32) -> Result<i32, ShmError> {
    let name = validate_name(name)?;

    let mut objects = SHM_OBJECTS.write();

    let object = if let Some(existing) = objects.get(name) {
        if oflag & flags::O_CREAT != 0 && oflag & flags::O_EXCL != 0 {
            return Err(ShmError::AlreadyExists);
        }

        if oflag & flags::O_TRUNC != 0 {
            existing.truncate(0)?;
        }

        existing.add_ref();
        existing.clone()
    } else {
        if oflag & flags::O_CREAT == 0 {
            return Err(ShmError::NotFound);
        }

        let object = Arc::new(ShmObject::new(name.into(), mode));
        objects.insert(name.into(), object.clone());
        object
    };

    drop(objects);

    // Create file descriptor
    let fd = NEXT_FD.fetch_add(1, Ordering::Relaxed) as i32;
    let shm_fd = Arc::new(ShmFd::new(fd, object, oflag));

    SHM_FDS.write().insert(fd, shm_fd);

    crate::kprintln!("[SHMFS] Opened shm /{} as fd {}", name, fd);

    Ok(fd)
}

/// shm_unlink syscall
pub fn sys_shm_unlink(name: &str) -> Result<(), ShmError> {
    let name = validate_name(name)?;

    let mut objects = SHM_OBJECTS.write();

    let object = objects.remove(name).ok_or(ShmError::NotFound)?;
    object.unlink();

    crate::kprintln!("[SHMFS] Unlinked shm /{}", name);

    Ok(())
}

/// Read from shm fd
pub fn read(fd: i32, buf: &mut [u8]) -> Result<usize, ShmError> {
    let fds = SHM_FDS.read();
    let shm_fd = fds.get(&fd).ok_or(ShmError::NotFound)?;
    shm_fd.read(buf)
}

/// Write to shm fd
pub fn write(fd: i32, buf: &[u8]) -> Result<usize, ShmError> {
    let fds = SHM_FDS.read();
    let shm_fd = fds.get(&fd).ok_or(ShmError::NotFound)?;
    shm_fd.write(buf)
}

/// Seek in shm fd
pub fn lseek(fd: i32, offset: i64, whence: i32) -> Result<u64, ShmError> {
    let fds = SHM_FDS.read();
    let shm_fd = fds.get(&fd).ok_or(ShmError::NotFound)?;
    shm_fd.seek(offset, whence)
}

/// Truncate shm fd
pub fn ftruncate(fd: i32, size: u64) -> Result<(), ShmError> {
    let fds = SHM_FDS.read();
    let shm_fd = fds.get(&fd).ok_or(ShmError::NotFound)?;
    shm_fd.ftruncate(size)
}

/// Stat shm fd
pub fn fstat(fd: i32) -> Result<ShmStat, ShmError> {
    let fds = SHM_FDS.read();
    let shm_fd = fds.get(&fd).ok_or(ShmError::NotFound)?;
    Ok(shm_fd.fstat())
}

/// Close shm fd
pub fn close(fd: i32) -> Result<(), ShmError> {
    SHM_FDS.write().remove(&fd)
        .map(|_| ())
        .ok_or(ShmError::NotFound)
}

/// Check if fd is shm
pub fn is_shmfd(fd: i32) -> bool {
    SHM_FDS.read().contains_key(&fd)
}

/// List all shared memory objects
pub fn list_objects() -> Vec<String> {
    SHM_OBJECTS.read().keys().cloned().collect()
}

/// Initialize shmfs
pub fn init() {
    crate::kprintln!("[FS] POSIX shared memory (shmfs) initialized");
}
