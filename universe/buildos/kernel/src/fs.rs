// ===============================================================================
// BUILDOS KERNEL - VIRTUAL FILE SYSTEM
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! Virtual File System (VFS) layer.
//!
//! Provides a unified interface for all filesystems:
//! - Mount table management
//! - Path resolution with mount points
//! - File descriptor management
//! - Inode caching
//! - Directory operations

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::{Mutex, RwLock};

pub mod initramfs;
pub mod ext2;
pub mod ext4;
pub mod fat32;
pub mod xattr;
pub mod inotify;
pub mod fanotify;
pub mod eventfd;
pub mod signalfd;
pub mod timerfd;
pub mod epoll;
pub mod shmfs;
pub mod io_uring;
pub mod fuse;

// =============================================================================
// FILE TYPES AND FLAGS
// =============================================================================

/// File types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FileType {
    /// Regular file
    Regular = 1,
    /// Directory
    Directory = 2,
    /// Symbolic link
    Symlink = 3,
    /// Block device
    BlockDevice = 4,
    /// Character device
    CharDevice = 5,
    /// Named pipe (FIFO)
    Pipe = 6,
    /// Unix socket
    Socket = 7,
}

impl FileType {
    /// Convert from mode bits (S_IFMT)
    pub fn from_mode(mode: u32) -> Self {
        match (mode >> 12) & 0xF {
            0x1 => FileType::Pipe,
            0x2 => FileType::CharDevice,
            0x4 => FileType::Directory,
            0x6 => FileType::BlockDevice,
            0x8 => FileType::Regular,
            0xA => FileType::Symlink,
            0xC => FileType::Socket,
            _ => FileType::Regular,
        }
    }

    /// Convert to mode bits
    pub fn to_mode(&self) -> u32 {
        match self {
            FileType::Pipe => 0x1000,
            FileType::CharDevice => 0x2000,
            FileType::Directory => 0x4000,
            FileType::BlockDevice => 0x6000,
            FileType::Regular => 0x8000,
            FileType::Symlink => 0xA000,
            FileType::Socket => 0xC000,
        }
    }
}

/// Open flags
pub mod flags {
    /// Read only
    pub const O_RDONLY: u32 = 0;
    /// Write only
    pub const O_WRONLY: u32 = 1;
    /// Read and write
    pub const O_RDWR: u32 = 2;
    /// Access mode mask
    pub const O_ACCMODE: u32 = 3;
    /// Create file if it doesn't exist
    pub const O_CREAT: u32 = 0o100;
    /// Fail if file exists (with O_CREAT)
    pub const O_EXCL: u32 = 0o200;
    /// Don't assign controlling terminal
    pub const O_NOCTTY: u32 = 0o400;
    /// Truncate file to zero length
    pub const O_TRUNC: u32 = 0o1000;
    /// Append on each write
    pub const O_APPEND: u32 = 0o2000;
    /// Non-blocking mode
    pub const O_NONBLOCK: u32 = 0o4000;
    /// Synchronous I/O
    pub const O_SYNC: u32 = 0o10000;
    /// Directory only
    pub const O_DIRECTORY: u32 = 0o200000;
    /// Don't follow symlinks
    pub const O_NOFOLLOW: u32 = 0o400000;
    /// Close on exec
    pub const O_CLOEXEC: u32 = 0o2000000;
}

/// Seek origins
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum SeekFrom {
    /// From beginning of file
    Start = 0,
    /// From current position
    Current = 1,
    /// From end of file
    End = 2,
}

/// Seek constants (lseek whence values)
pub const SEEK_SET: i32 = 0;
pub const SEEK_CUR: i32 = 1;
pub const SEEK_END: i32 = 2;

/// Error codes
pub mod errno {
    pub const EPERM: i32 = 1;      // Operation not permitted
    pub const ENOENT: i32 = 2;     // No such file or directory
    pub const EIO: i32 = 5;        // I/O error
    pub const ENXIO: i32 = 6;      // No such device or address
    pub const EBADF: i32 = 9;      // Bad file descriptor
    pub const ENOMEM: i32 = 12;    // Out of memory
    pub const EACCES: i32 = 13;    // Permission denied
    pub const EEXIST: i32 = 17;    // File exists
    pub const ENOTDIR: i32 = 20;   // Not a directory
    pub const EISDIR: i32 = 21;    // Is a directory
    pub const EINVAL: i32 = 22;    // Invalid argument
    pub const EMFILE: i32 = 24;    // Too many open files
    pub const ENOSPC: i32 = 28;    // No space left on device
    pub const EROFS: i32 = 30;     // Read-only file system
    pub const ELOOP: i32 = 40;     // Too many symbolic links
    pub const ENAMETOOLONG: i32 = 36; // File name too long
    pub const ENOTEMPTY: i32 = 39; // Directory not empty
    pub const ENOSYS: i32 = 38;    // Function not implemented
    pub const ESPIPE: i32 = 29;    // Illegal seek
    pub const ERANGE: i32 = 34;    // Math result not representable
    pub const ESRCH: i32 = 3;      // No such process
    pub const ECHILD: i32 = 10;    // No child processes
    pub const ENOEXEC: i32 = 8;    // Exec format error
    pub const EFAULT: i32 = 14;    // Bad address
}

// =============================================================================
// FILE METADATA
// =============================================================================

/// File metadata (stat structure)
#[derive(Debug, Clone)]
pub struct Metadata {
    /// Device ID
    pub dev: u64,
    /// Inode number
    pub ino: u64,
    /// File type and permissions
    pub mode: u32,
    /// Number of hard links
    pub nlink: u32,
    /// Owner user ID
    pub uid: u32,
    /// Owner group ID
    pub gid: u32,
    /// Device ID (if special file)
    pub rdev: u64,
    /// File size in bytes
    pub size: u64,
    /// Block size for filesystem I/O
    pub blksize: u32,
    /// Number of 512-byte blocks allocated
    pub blocks: u64,
    /// Last access time (seconds since epoch)
    pub atime: u64,
    /// Last modification time
    pub mtime: u64,
    /// Last status change time
    pub ctime: u64,
}

impl Metadata {
    /// Create new metadata
    pub fn new(file_type: FileType, size: u64) -> Self {
        Self {
            dev: 0,
            ino: 0,
            mode: file_type.to_mode() | 0o644,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            size,
            blksize: 4096,
            blocks: (size + 511) / 512,
            atime: 0,
            mtime: 0,
            ctime: 0,
        }
    }

    /// Get file type
    pub fn file_type(&self) -> FileType {
        FileType::from_mode(self.mode)
    }

    /// Check if directory
    pub fn is_dir(&self) -> bool {
        self.file_type() == FileType::Directory
    }

    /// Check if regular file
    pub fn is_file(&self) -> bool {
        self.file_type() == FileType::Regular
    }

    /// Check if symlink
    pub fn is_symlink(&self) -> bool {
        self.file_type() == FileType::Symlink
    }
}

// =============================================================================
// DIRECTORY ENTRY
// =============================================================================

/// Directory entry
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// Entry name
    pub name: String,
    /// Inode number
    pub inode: u64,
    /// File type
    pub file_type: FileType,
}

impl DirEntry {
    /// Create new directory entry
    pub fn new(name: String, inode: u64, file_type: FileType) -> Self {
        Self { name, inode, file_type }
    }
}

// =============================================================================
// INODE (VFS NODE)
// =============================================================================

/// Inode identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InodeId {
    /// Filesystem ID
    pub fs_id: u64,
    /// Inode number within filesystem
    pub ino: u64,
}

/// VFS Inode - represents a file/directory in the VFS
pub struct Inode {
    /// Unique identifier
    pub id: InodeId,
    /// File metadata
    pub metadata: Metadata,
    /// Filesystem operations
    pub ops: Arc<dyn Filesystem>,
    /// Private filesystem data
    pub private: u64,
}

impl Inode {
    /// Create new inode
    pub fn new(id: InodeId, metadata: Metadata, ops: Arc<dyn Filesystem>) -> Self {
        Self {
            id,
            metadata,
            ops,
            private: 0,
        }
    }
}

// =============================================================================
// FILE HANDLE
// =============================================================================

/// Open file handle
pub struct FileHandle {
    /// Associated inode
    pub inode: Arc<RwLock<Inode>>,
    /// Current file position
    pub position: AtomicU64,
    /// Open flags
    pub flags: u32,
}

impl FileHandle {
    /// Create new file handle
    pub fn new(inode: Arc<RwLock<Inode>>, flags: u32) -> Self {
        Self {
            inode,
            position: AtomicU64::new(0),
            flags,
        }
    }

    /// Read from file
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, i32> {
        let inode = self.inode.read();
        let pos = self.position.load(Ordering::SeqCst);

        let result = inode.ops.read(&inode, pos, buf)?;
        self.position.fetch_add(result as u64, Ordering::SeqCst);

        Ok(result)
    }

    /// Write to file
    pub fn write(&self, buf: &[u8]) -> Result<usize, i32> {
        // Check if writable
        if (self.flags & flags::O_ACCMODE) == flags::O_RDONLY {
            return Err(errno::EBADF);
        }

        let inode = self.inode.read();
        let mut pos = self.position.load(Ordering::SeqCst);

        // Handle append mode
        if (self.flags & flags::O_APPEND) != 0 {
            pos = inode.metadata.size;
        }

        let result = inode.ops.write(&inode, pos, buf)?;
        self.position.store(pos + result as u64, Ordering::SeqCst);

        Ok(result)
    }

    /// Seek to position
    pub fn seek(&self, offset: i64, whence: SeekFrom) -> Result<u64, i32> {
        let inode = self.inode.read();
        let current = self.position.load(Ordering::SeqCst);

        let new_pos = match whence {
            SeekFrom::Start => offset as u64,
            SeekFrom::Current => {
                if offset < 0 {
                    current.saturating_sub((-offset) as u64)
                } else {
                    current.saturating_add(offset as u64)
                }
            }
            SeekFrom::End => {
                if offset < 0 {
                    inode.metadata.size.saturating_sub((-offset) as u64)
                } else {
                    inode.metadata.size.saturating_add(offset as u64)
                }
            }
        };

        self.position.store(new_pos, Ordering::SeqCst);
        Ok(new_pos)
    }
}

// =============================================================================
// FILESYSTEM TRAIT
// =============================================================================

/// Filesystem operations trait
pub trait Filesystem: Send + Sync {
    /// Get filesystem name
    fn name(&self) -> &str;

    /// Read from file
    fn read(&self, inode: &Inode, offset: u64, buf: &mut [u8]) -> Result<usize, i32>;

    /// Write to file
    fn write(&self, inode: &Inode, offset: u64, buf: &[u8]) -> Result<usize, i32> {
        let _ = (inode, offset, buf);
        Err(errno::EROFS)
    }

    /// Read directory entries
    fn readdir(&self, inode: &Inode) -> Result<Vec<DirEntry>, i32>;

    /// Look up entry in directory
    fn lookup(&self, parent: &Inode, name: &str) -> Result<Inode, i32>;

    /// Get file metadata
    fn getattr(&self, inode: &Inode) -> Result<Metadata, i32> {
        Ok(inode.metadata.clone())
    }

    /// Create file
    fn create(&self, parent: &Inode, name: &str, mode: u32) -> Result<Inode, i32> {
        let _ = (parent, name, mode);
        Err(errno::EROFS)
    }

    /// Create directory
    fn mkdir(&self, parent: &Inode, name: &str, mode: u32) -> Result<Inode, i32> {
        let _ = (parent, name, mode);
        Err(errno::EROFS)
    }

    /// Remove file
    fn unlink(&self, parent: &Inode, name: &str) -> Result<(), i32> {
        let _ = (parent, name);
        Err(errno::EROFS)
    }

    /// Remove directory
    fn rmdir(&self, parent: &Inode, name: &str) -> Result<(), i32> {
        let _ = (parent, name);
        Err(errno::EROFS)
    }

    /// Rename file
    fn rename(&self, old_parent: &Inode, old_name: &str,
              new_parent: &Inode, new_name: &str) -> Result<(), i32> {
        let _ = (old_parent, old_name, new_parent, new_name);
        Err(errno::EROFS)
    }

    /// Read symbolic link
    fn readlink(&self, inode: &Inode) -> Result<String, i32> {
        let _ = inode;
        Err(errno::EINVAL)
    }

    /// Create symbolic link
    fn symlink(&self, parent: &Inode, name: &str, target: &str) -> Result<Inode, i32> {
        let _ = (parent, name, target);
        Err(errno::EROFS)
    }

    /// Truncate file
    fn truncate(&self, inode: &Inode, size: u64) -> Result<(), i32> {
        let _ = (inode, size);
        Err(errno::EROFS)
    }

    /// Sync file data
    fn sync(&self, inode: &Inode) -> Result<(), i32> {
        let _ = inode;
        Ok(())
    }
}

// =============================================================================
// MOUNT POINT
// =============================================================================

/// Mount point entry
struct MountPoint {
    /// Mount path
    path: String,
    /// Mounted filesystem
    filesystem: Arc<dyn Filesystem>,
    /// Root inode of mounted filesystem
    root_inode: Arc<RwLock<Inode>>,
    /// Mount flags
    flags: u32,
}

/// Mount flags
pub mod mount_flags {
    pub const MS_RDONLY: u32 = 1;      // Read-only
    pub const MS_NOSUID: u32 = 2;      // Ignore suid/sgid bits
    pub const MS_NODEV: u32 = 4;       // Disallow device access
    pub const MS_NOEXEC: u32 = 8;      // Disallow program execution
    pub const MS_NOATIME: u32 = 1024;  // Don't update access times
}

// =============================================================================
// VFS CORE
// =============================================================================

/// VFS global state
struct VfsState {
    /// Mount table (path -> mount point)
    mounts: BTreeMap<String, MountPoint>,
    /// Next filesystem ID
    next_fs_id: u64,
    /// Inode cache
    inode_cache: BTreeMap<InodeId, Arc<RwLock<Inode>>>,
}

impl VfsState {
    const fn new() -> Self {
        Self {
            mounts: BTreeMap::new(),
            next_fs_id: 1,
            inode_cache: BTreeMap::new(),
        }
    }
}

/// Global VFS state
static VFS: Mutex<VfsState> = Mutex::new(VfsState::new());

/// File descriptor table (per-process, simplified to global for now)
static FD_TABLE: RwLock<BTreeMap<u64, Arc<FileHandle>>> = RwLock::new(BTreeMap::new());

/// Next file descriptor
static NEXT_FD: AtomicU64 = AtomicU64::new(3); // 0, 1, 2 reserved for stdin/out/err

// =============================================================================
// PATH UTILITIES
// =============================================================================

/// Normalize path (remove . and .., handle //)
pub fn normalize_path(path: &str) -> String {
    let mut components: Vec<&str> = Vec::new();

    for component in path.split('/') {
        match component {
            "" | "." => continue,
            ".." => { components.pop(); }
            _ => components.push(component),
        }
    }

    if components.is_empty() {
        String::from("/")
    } else {
        let mut result = String::new();
        for component in components {
            result.push('/');
            result.push_str(component);
        }
        result
    }
}

/// Split path into parent and name
pub fn split_path(path: &str) -> (&str, &str) {
    let path = path.trim_end_matches('/');
    match path.rfind('/') {
        Some(pos) if pos == 0 => ("/", &path[1..]),
        Some(pos) => (&path[..pos], &path[pos + 1..]),
        None => (".", path),
    }
}

/// Find mount point for path
fn find_mount(path: &str) -> Option<(String, Arc<dyn Filesystem>, Arc<RwLock<Inode>>)> {
    let vfs = VFS.lock();
    let normalized = normalize_path(path);

    // Find longest matching mount point
    let mut best_match: Option<&MountPoint> = None;
    let mut best_len = 0;

    for (mount_path, mount) in &vfs.mounts {
        if normalized.starts_with(mount_path.as_str()) ||
           (mount_path == "/" && !normalized.is_empty()) {
            let len = mount_path.len();
            if len > best_len || (len == 1 && mount_path == "/") {
                best_match = Some(mount);
                best_len = len;
            }
        }
    }

    best_match.map(|m| (m.path.clone(), m.filesystem.clone(), m.root_inode.clone()))
}

/// Resolve path to inode
fn resolve_path(path: &str, follow_symlinks: bool) -> Result<Arc<RwLock<Inode>>, i32> {
    resolve_path_with_depth(path, follow_symlinks, 0)
}

fn resolve_path_with_depth(path: &str, follow_symlinks: bool, depth: u32) -> Result<Arc<RwLock<Inode>>, i32> {
    const MAX_SYMLINK_DEPTH: u32 = 40;

    if depth > MAX_SYMLINK_DEPTH {
        return Err(errno::ELOOP);
    }

    let normalized = normalize_path(path);

    // Find mount point
    let (mount_path, fs, root_inode) = find_mount(&normalized)
        .ok_or(errno::ENOENT)?;

    // Get path relative to mount point
    let relative = if normalized.len() > mount_path.len() {
        &normalized[mount_path.len()..]
    } else {
        ""
    };

    // Start from root inode
    let mut current = root_inode;

    // Traverse path components
    for component in relative.split('/').filter(|s| !s.is_empty()) {
        let inode = current.read();

        // Must be a directory to traverse
        if !inode.metadata.is_dir() {
            return Err(errno::ENOTDIR);
        }

        // Look up component
        let child = fs.lookup(&inode, component)?;
        drop(inode);

        // Cache and wrap inode
        let child = Arc::new(RwLock::new(child));
        current = child;

        // Handle symlinks
        if follow_symlinks {
            let inode = current.read();
            if inode.metadata.is_symlink() {
                let target = fs.readlink(&inode)?;
                drop(inode);

                // Resolve symlink target
                let target_path = if target.starts_with('/') {
                    target
                } else {
                    let (parent, _) = split_path(&normalized);
                    format!("{}/{}", parent, target)
                };

                return resolve_path_with_depth(&target_path, true, depth + 1);
            }
        }
    }

    Ok(current)
}

// =============================================================================
// VFS PUBLIC API
// =============================================================================

/// Initialize VFS subsystem
pub fn init() {
    crate::kprintln!("[VFS] Initializing Virtual File System...");

    // Initialize initramfs if present
    initramfs::init();

    crate::kprintln!("[VFS] VFS initialized");
}

/// Mount filesystem
pub fn mount(path: &str, fs: Arc<dyn Filesystem>, root_inode: Inode) -> Result<(), i32> {
    let mut vfs = VFS.lock();
    let normalized = normalize_path(path);

    // Check if already mounted
    if vfs.mounts.contains_key(&normalized) {
        return Err(errno::EEXIST);
    }

    let _fs_id = vfs.next_fs_id;
    vfs.next_fs_id += 1;

    let root_inode = Arc::new(RwLock::new(root_inode));

    vfs.mounts.insert(normalized.clone(), MountPoint {
        path: normalized.clone(),
        filesystem: fs.clone(),
        root_inode: root_inode.clone(),
        flags: 0,
    });

    crate::kprintln!("[VFS] Mounted {} at {}", fs.name(), normalized);
    Ok(())
}

/// Unmount filesystem
pub fn umount(path: &str) -> Result<(), i32> {
    let mut vfs = VFS.lock();
    let normalized = normalize_path(path);

    vfs.mounts.remove(&normalized)
        .map(|_| ())
        .ok_or(errno::EINVAL)
}

/// Open file
pub fn open(path: &str, flags: u32, _mode: u32) -> Result<u64, i32> {
    // Check for O_CREAT
    let follow = (flags & flags::O_NOFOLLOW) == 0;

    let inode = match resolve_path(path, follow) {
        Ok(inode) => inode,
        Err(errno::ENOENT) if (flags & flags::O_CREAT) != 0 => {
            // Create file
            let (parent_path, name) = split_path(path);
            let parent = resolve_path(parent_path, true)?;
            let parent_inode = parent.read();

            let new_inode = parent_inode.ops.create(&parent_inode, name, 0o644)?;
            Arc::new(RwLock::new(new_inode))
        }
        Err(e) => return Err(e),
    };

    // Check O_DIRECTORY flag
    if (flags & flags::O_DIRECTORY) != 0 {
        let inode_guard = inode.read();
        if !inode_guard.metadata.is_dir() {
            return Err(errno::ENOTDIR);
        }
    }

    // Handle O_TRUNC
    if (flags & flags::O_TRUNC) != 0 && (flags & flags::O_ACCMODE) != flags::O_RDONLY {
        let inode_guard = inode.read();
        inode_guard.ops.truncate(&inode_guard, 0)?;
    }

    // Allocate file descriptor
    let fd = NEXT_FD.fetch_add(1, Ordering::SeqCst);
    let handle = Arc::new(FileHandle::new(inode, flags));

    FD_TABLE.write().insert(fd, handle);

    Ok(fd)
}

/// Close file
pub fn close(fd: u64) -> Result<(), i32> {
    FD_TABLE.write().remove(&fd)
        .map(|_| ())
        .ok_or(errno::EBADF)
}

/// Read from file
pub fn read(fd: u64, buf: &mut [u8]) -> Result<usize, i32> {
    let handle = FD_TABLE.read().get(&fd).cloned()
        .ok_or(errno::EBADF)?;
    handle.read(buf)
}

/// Write to file
pub fn write(fd: u64, buf: &[u8]) -> Result<usize, i32> {
    let handle = FD_TABLE.read().get(&fd).cloned()
        .ok_or(errno::EBADF)?;
    handle.write(buf)
}

/// Seek in file
pub fn lseek(fd: u64, offset: i64, whence: i32) -> Result<u64, i32> {
    let handle = FD_TABLE.read().get(&fd).cloned()
        .ok_or(errno::EBADF)?;

    let whence = match whence {
        0 => SeekFrom::Start,
        1 => SeekFrom::Current,
        2 => SeekFrom::End,
        _ => return Err(errno::EINVAL),
    };

    handle.seek(offset, whence)
}

/// Read entire file contents
pub fn read_file(path: &str) -> Result<Vec<u8>, i32> {
    

    // Open the file for reading
    let fd = open(path, flags::O_RDONLY, 0)?;

    // Get file size
    let meta = fstat(fd)?;
    let size = meta.size as usize;

    // Read file contents
    let mut buf = alloc::vec![0u8; size];
    let mut offset = 0;
    while offset < size {
        let n = read(fd, &mut buf[offset..])?;
        if n == 0 {
            break;
        }
        offset += n;
    }

    close(fd)?;
    buf.truncate(offset);
    Ok(buf)
}

/// Get file metadata by path
pub fn stat(path: &str) -> Result<Metadata, i32> {
    let inode = resolve_path(path, true)?;
    let inode_guard = inode.read();
    inode_guard.ops.getattr(&inode_guard)
}

/// Get file metadata by path (no symlink follow)
pub fn lstat(path: &str) -> Result<Metadata, i32> {
    let inode = resolve_path(path, false)?;
    let inode_guard = inode.read();
    inode_guard.ops.getattr(&inode_guard)
}

/// Get file metadata by fd
pub fn fstat(fd: u64) -> Result<Metadata, i32> {
    let handle = FD_TABLE.read().get(&fd).cloned()
        .ok_or(errno::EBADF)?;
    let inode = handle.inode.read();
    inode.ops.getattr(&inode)
}

/// Read directory entries
pub fn readdir(path: &str) -> Result<Vec<DirEntry>, i32> {
    let inode = resolve_path(path, true)?;
    let inode_guard = inode.read();

    if !inode_guard.metadata.is_dir() {
        return Err(errno::ENOTDIR);
    }

    inode_guard.ops.readdir(&inode_guard)
}

/// Create directory
pub fn mkdir(path: &str, mode: u32) -> Result<(), i32> {
    let (parent_path, name) = split_path(path);
    let parent = resolve_path(parent_path, true)?;
    let parent_inode = parent.read();

    parent_inode.ops.mkdir(&parent_inode, name, mode)?;
    Ok(())
}

/// Remove directory
pub fn rmdir(path: &str) -> Result<(), i32> {
    let (parent_path, name) = split_path(path);
    let parent = resolve_path(parent_path, true)?;
    let parent_inode = parent.read();

    parent_inode.ops.rmdir(&parent_inode, name)
}

/// Remove file
pub fn unlink(path: &str) -> Result<(), i32> {
    let (parent_path, name) = split_path(path);
    let parent = resolve_path(parent_path, true)?;
    let parent_inode = parent.read();

    parent_inode.ops.unlink(&parent_inode, name)
}

/// Rename file
pub fn rename(old_path: &str, new_path: &str) -> Result<(), i32> {
    let (old_parent_path, old_name) = split_path(old_path);
    let (new_parent_path, new_name) = split_path(new_path);

    let old_parent = resolve_path(old_parent_path, true)?;
    let new_parent = resolve_path(new_parent_path, true)?;

    let old_parent_inode = old_parent.read();
    let new_parent_inode = new_parent.read();

    old_parent_inode.ops.rename(&old_parent_inode, old_name,
                                 &new_parent_inode, new_name)
}

/// Read symbolic link
pub fn readlink(path: &str) -> Result<String, i32> {
    let inode = resolve_path(path, false)?;
    let inode_guard = inode.read();

    if !inode_guard.metadata.is_symlink() {
        return Err(errno::EINVAL);
    }

    inode_guard.ops.readlink(&inode_guard)
}

/// Create symbolic link
pub fn symlink(target: &str, path: &str) -> Result<(), i32> {
    let (parent_path, name) = split_path(path);
    let parent = resolve_path(parent_path, true)?;
    let parent_inode = parent.read();

    parent_inode.ops.symlink(&parent_inode, name, target)?;
    Ok(())
}

/// Truncate file
pub fn truncate(path: &str, size: u64) -> Result<(), i32> {
    let inode = resolve_path(path, true)?;
    let inode_guard = inode.read();

    inode_guard.ops.truncate(&inode_guard, size)
}

/// Sync file
pub fn fsync(fd: u64) -> Result<(), i32> {
    let handle = FD_TABLE.read().get(&fd).cloned()
        .ok_or(errno::EBADF)?;
    let inode = handle.inode.read();
    inode.ops.sync(&inode)
}

/// Get current working directory (simplified - always /)
pub fn getcwd() -> String {
    String::from("/")
}

/// Check if file exists
pub fn exists(path: &str) -> bool {
    resolve_path(path, true).is_ok()
}

/// Check if path is directory
pub fn is_dir(path: &str) -> bool {
    resolve_path(path, true)
        .map(|inode| inode.read().metadata.is_dir())
        .unwrap_or(false)
}

/// Check if path is file
pub fn is_file(path: &str) -> bool {
    resolve_path(path, true)
        .map(|inode| inode.read().metadata.is_file())
        .unwrap_or(false)
}

// =============================================================================
// MOUNT COUNT
// =============================================================================

/// Get number of mounted filesystems
pub fn mount_count() -> usize {
    VFS.lock().mounts.len()
}

/// Check if a file descriptor is valid
pub fn is_valid_fd(fd: u64) -> bool {
    // fds 0, 1, 2 are always valid (stdin, stdout, stderr)
    if fd < 3 {
        return true;
    }
    FD_TABLE.read().contains_key(&fd)
}
