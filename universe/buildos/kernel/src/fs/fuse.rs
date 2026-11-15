// ===============================================================================
// BUILDOS KERNEL - FUSE (FILESYSTEM IN USERSPACE)
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! FUSE - Filesystem in Userspace
//!
//! Allows filesystem implementations to run in userspace by forwarding
//! VFS operations to a userspace daemon via a special device.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::sync::RwLock;

// =============================================================================
// FUSE PROTOCOL VERSION
// =============================================================================

/// FUSE kernel protocol major version
pub const FUSE_KERNEL_VERSION: u32 = 7;
/// FUSE kernel protocol minor version
pub const FUSE_KERNEL_MINOR_VERSION: u32 = 38;

// =============================================================================
// FUSE OPCODES
// =============================================================================

/// FUSE operation codes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum FuseOpcode {
    /// Look up file by name
    Lookup = 1,
    /// Forget inode
    Forget = 2,
    /// Get file attributes
    Getattr = 3,
    /// Set file attributes
    Setattr = 4,
    /// Read symbolic link
    Readlink = 5,
    /// Create symbolic link
    Symlink = 6,
    /// Create regular file
    Mknod = 8,
    /// Create directory
    Mkdir = 9,
    /// Remove file
    Unlink = 10,
    /// Remove directory
    Rmdir = 11,
    /// Rename file
    Rename = 12,
    /// Create hard link
    Link = 13,
    /// Open file
    Open = 14,
    /// Read data
    Read = 15,
    /// Write data
    Write = 16,
    /// Sync file data
    Fsync = 20,
    /// Open directory
    Opendir = 27,
    /// Read directory
    Readdir = 28,
    /// Release directory
    Releasedir = 29,
    /// Sync directory
    Fsyncdir = 30,
    /// Get filesystem statistics
    Statfs = 17,
    /// Release file
    Release = 18,
    /// Flush file
    Flush = 25,
    /// Initialize connection
    Init = 26,
    /// Interrupt operation
    Interrupt = 36,
    /// Memory mapping
    Bmap = 37,
    /// Destroy filesystem
    Destroy = 38,
    /// ioctl
    Ioctl = 39,
    /// Poll for events
    Poll = 40,
    /// Notify reply
    NotifyReply = 41,
    /// Batch forget
    BatchForget = 42,
    /// fallocate
    Fallocate = 43,
    /// Read directory (plus attributes)
    Readdirplus = 44,
    /// Rename2
    Rename2 = 45,
    /// lseek
    Lseek = 46,
    /// Copy file range
    CopyFileRange = 47,
    /// Setup mapping
    Setupmapping = 48,
    /// Remove mapping
    Removemapping = 49,
    /// Get extended attribute
    Getxattr = 22,
    /// List extended attributes
    Listxattr = 23,
    /// Set extended attribute
    Setxattr = 21,
    /// Remove extended attribute
    Removexattr = 24,
    /// Access check
    Access = 34,
    /// Create and open
    Create = 35,
}

impl FuseOpcode {
    /// Convert from u32
    pub fn from_u32(val: u32) -> Option<Self> {
        match val {
            1 => Some(Self::Lookup),
            2 => Some(Self::Forget),
            3 => Some(Self::Getattr),
            4 => Some(Self::Setattr),
            5 => Some(Self::Readlink),
            6 => Some(Self::Symlink),
            8 => Some(Self::Mknod),
            9 => Some(Self::Mkdir),
            10 => Some(Self::Unlink),
            11 => Some(Self::Rmdir),
            12 => Some(Self::Rename),
            13 => Some(Self::Link),
            14 => Some(Self::Open),
            15 => Some(Self::Read),
            16 => Some(Self::Write),
            17 => Some(Self::Statfs),
            18 => Some(Self::Release),
            20 => Some(Self::Fsync),
            21 => Some(Self::Setxattr),
            22 => Some(Self::Getxattr),
            23 => Some(Self::Listxattr),
            24 => Some(Self::Removexattr),
            25 => Some(Self::Flush),
            26 => Some(Self::Init),
            27 => Some(Self::Opendir),
            28 => Some(Self::Readdir),
            29 => Some(Self::Releasedir),
            30 => Some(Self::Fsyncdir),
            34 => Some(Self::Access),
            35 => Some(Self::Create),
            36 => Some(Self::Interrupt),
            37 => Some(Self::Bmap),
            38 => Some(Self::Destroy),
            39 => Some(Self::Ioctl),
            40 => Some(Self::Poll),
            42 => Some(Self::BatchForget),
            43 => Some(Self::Fallocate),
            44 => Some(Self::Readdirplus),
            45 => Some(Self::Rename2),
            46 => Some(Self::Lseek),
            47 => Some(Self::CopyFileRange),
            _ => None,
        }
    }
}

// =============================================================================
// FUSE HEADERS
// =============================================================================

/// FUSE request header
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FuseInHeader {
    /// Request length
    pub len: u32,
    /// Opcode
    pub opcode: u32,
    /// Unique request ID
    pub unique: u64,
    /// Inode number
    pub nodeid: u64,
    /// UID of caller
    pub uid: u32,
    /// GID of caller
    pub gid: u32,
    /// PID of caller
    pub pid: u32,
    /// Padding
    pub padding: u32,
}

/// FUSE response header
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FuseOutHeader {
    /// Response length
    pub len: u32,
    /// Error code (negative errno or 0)
    pub error: i32,
    /// Unique request ID (from request)
    pub unique: u64,
}

// =============================================================================
// FUSE INIT
// =============================================================================

/// FUSE init request
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FuseInitIn {
    /// Protocol major version
    pub major: u32,
    /// Protocol minor version
    pub minor: u32,
    /// Maximum readahead
    pub max_readahead: u32,
    /// Init flags
    pub flags: u32,
    /// More flags (high bits)
    pub flags2: u32,
    /// Unused
    pub unused: [u32; 11],
}

/// FUSE init response
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FuseInitOut {
    /// Protocol major version
    pub major: u32,
    /// Protocol minor version
    pub minor: u32,
    /// Maximum readahead
    pub max_readahead: u32,
    /// Capabilities
    pub flags: u32,
    /// Max background requests
    pub max_background: u16,
    /// Congestion threshold
    pub congestion_threshold: u16,
    /// Maximum write size
    pub max_write: u32,
    /// Timestamp granularity
    pub time_gran: u32,
    /// Maximum pages for read/write
    pub max_pages: u16,
    /// MAP_ALIGNMENT
    pub map_alignment: u16,
    /// Flags (high bits)
    pub flags2: u32,
    /// Unused
    pub unused: [u32; 7],
}

// =============================================================================
// FUSE CAPABILITY FLAGS
// =============================================================================

/// FUSE capability flags
pub mod cap_flags {
    /// Asynchronous read
    pub const FUSE_ASYNC_READ: u32 = 1 << 0;
    /// POSIX locks
    pub const FUSE_POSIX_LOCKS: u32 = 1 << 1;
    /// File handles
    pub const FUSE_FILE_OPS: u32 = 1 << 2;
    /// Atomic O_TRUNC
    pub const FUSE_ATOMIC_O_TRUNC: u32 = 1 << 3;
    /// Export support
    pub const FUSE_EXPORT_SUPPORT: u32 = 1 << 4;
    /// Big writes
    pub const FUSE_BIG_WRITES: u32 = 1 << 5;
    /// Don't apply umask
    pub const FUSE_DONT_MASK: u32 = 1 << 6;
    /// Splice write
    pub const FUSE_SPLICE_WRITE: u32 = 1 << 7;
    /// Splice move
    pub const FUSE_SPLICE_MOVE: u32 = 1 << 8;
    /// Splice read
    pub const FUSE_SPLICE_READ: u32 = 1 << 9;
    /// BSD-style locks
    pub const FUSE_FLOCK_LOCKS: u32 = 1 << 10;
    /// ioctl is 32-bit
    pub const FUSE_HAS_IOCTL_DIR: u32 = 1 << 11;
    /// Auto inval data
    pub const FUSE_AUTO_INVAL_DATA: u32 = 1 << 12;
    /// Readdirplus
    pub const FUSE_DO_READDIRPLUS: u32 = 1 << 13;
    /// Adaptive readdirplus
    pub const FUSE_READDIRPLUS_AUTO: u32 = 1 << 14;
    /// Async direct I/O
    pub const FUSE_ASYNC_DIO: u32 = 1 << 15;
    /// Writeback cache
    pub const FUSE_WRITEBACK_CACHE: u32 = 1 << 16;
    /// No open support
    pub const FUSE_NO_OPEN_SUPPORT: u32 = 1 << 17;
    /// Parallel dirops
    pub const FUSE_PARALLEL_DIROPS: u32 = 1 << 18;
    /// Handle killpriv
    pub const FUSE_HANDLE_KILLPRIV: u32 = 1 << 19;
    /// POSIX ACL
    pub const FUSE_POSIX_ACL: u32 = 1 << 20;
    /// Abort I/O
    pub const FUSE_ABORT_ERROR: u32 = 1 << 21;
    /// Max pages
    pub const FUSE_MAX_PAGES: u32 = 1 << 22;
    /// Cache symlinks
    pub const FUSE_CACHE_SYMLINKS: u32 = 1 << 23;
    /// No opendir support
    pub const FUSE_NO_OPENDIR_SUPPORT: u32 = 1 << 24;
    /// Explicit inval data
    pub const FUSE_EXPLICIT_INVAL_DATA: u32 = 1 << 25;
    /// Map alignment
    pub const FUSE_MAP_ALIGNMENT: u32 = 1 << 26;
    /// Submounts
    pub const FUSE_SUBMOUNTS: u32 = 1 << 27;
    /// Handle killpriv v2
    pub const FUSE_HANDLE_KILLPRIV_V2: u32 = 1 << 28;
    /// Setxattr ext
    pub const FUSE_SETXATTR_EXT: u32 = 1 << 29;
}

// =============================================================================
// FUSE ATTR
// =============================================================================

/// FUSE file attributes
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct FuseAttr {
    /// Inode number
    pub ino: u64,
    /// File size
    pub size: u64,
    /// Blocks
    pub blocks: u64,
    /// Access time (seconds)
    pub atime: u64,
    /// Modification time (seconds)
    pub mtime: u64,
    /// Status change time (seconds)
    pub ctime: u64,
    /// Access time (nanoseconds)
    pub atimensec: u32,
    /// Modification time (nanoseconds)
    pub mtimensec: u32,
    /// Status change time (nanoseconds)
    pub ctimensec: u32,
    /// File mode
    pub mode: u32,
    /// Number of links
    pub nlink: u32,
    /// Owner UID
    pub uid: u32,
    /// Owner GID
    pub gid: u32,
    /// Device ID (if device)
    pub rdev: u32,
    /// Block size
    pub blksize: u32,
    /// Flags
    pub flags: u32,
}

/// FUSE entry response
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FuseEntryOut {
    /// Inode number
    pub nodeid: u64,
    /// Generation
    pub generation: u64,
    /// Entry timeout (seconds)
    pub entry_valid: u64,
    /// Attribute timeout (seconds)
    pub attr_valid: u64,
    /// Entry timeout (nanoseconds)
    pub entry_valid_nsec: u32,
    /// Attribute timeout (nanoseconds)
    pub attr_valid_nsec: u32,
    /// Attributes
    pub attr: FuseAttr,
}

// =============================================================================
// FUSE READ/WRITE
// =============================================================================

/// FUSE open request
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FuseOpenIn {
    /// Open flags
    pub flags: u32,
    /// Unused
    pub unused: u32,
}

/// FUSE open response
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FuseOpenOut {
    /// File handle
    pub fh: u64,
    /// Open flags
    pub open_flags: u32,
    /// Padding
    pub padding: u32,
}

/// FUSE read request
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FuseReadIn {
    /// File handle
    pub fh: u64,
    /// Offset
    pub offset: u64,
    /// Size
    pub size: u32,
    /// Read flags
    pub read_flags: u32,
    /// Lock owner
    pub lock_owner: u64,
    /// Flags
    pub flags: u32,
    /// Padding
    pub padding: u32,
}

/// FUSE write request
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FuseWriteIn {
    /// File handle
    pub fh: u64,
    /// Offset
    pub offset: u64,
    /// Size
    pub size: u32,
    /// Write flags
    pub write_flags: u32,
    /// Lock owner
    pub lock_owner: u64,
    /// Flags
    pub flags: u32,
    /// Padding
    pub padding: u32,
}

/// FUSE write response
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FuseWriteOut {
    /// Bytes written
    pub size: u32,
    /// Padding
    pub padding: u32,
}

// =============================================================================
// FUSE CONNECTION
// =============================================================================

/// FUSE connection (per-mount)
pub struct FuseConnection {
    /// Connection ID
    id: u32,
    /// Protocol version (major)
    proto_major: u32,
    /// Protocol version (minor)
    proto_minor: u32,
    /// Max write size
    max_write: u32,
    /// Max read size
    max_read: u32,
    /// Max readahead
    max_readahead: u32,
    /// Capability flags
    flags: u32,
    /// Pending requests
    pending: RwLock<VecDeque<FuseRequest>>,
    /// In-flight requests
    processing: RwLock<BTreeMap<u64, FuseRequest>>,
    /// Next unique ID
    next_unique: AtomicU64,
    /// Connected to userspace
    connected: core::sync::atomic::AtomicBool,
    /// Minor device number
    minor: u32,
}

/// FUSE request
#[derive(Clone)]
pub struct FuseRequest {
    /// Request header
    pub header: FuseInHeader,
    /// Request data
    pub data: Vec<u8>,
}

/// FUSE response
#[derive(Clone)]
pub struct FuseResponse {
    /// Response header
    pub header: FuseOutHeader,
    /// Response data
    pub data: Vec<u8>,
}

impl FuseConnection {
    /// Create new connection
    pub fn new(id: u32, minor: u32) -> Self {
        Self {
            id,
            proto_major: FUSE_KERNEL_VERSION,
            proto_minor: FUSE_KERNEL_MINOR_VERSION,
            max_write: 128 * 1024, // 128KB
            max_read: 128 * 1024,
            max_readahead: 128 * 1024,
            flags: 0,
            pending: RwLock::new(VecDeque::new()),
            processing: RwLock::new(BTreeMap::new()),
            next_unique: AtomicU64::new(1),
            connected: core::sync::atomic::AtomicBool::new(false),
            minor,
        }
    }

    /// Handle INIT request
    pub fn init(&mut self, init_in: &FuseInitIn) -> FuseInitOut {
        self.proto_major = init_in.major.min(FUSE_KERNEL_VERSION);
        self.proto_minor = if self.proto_major == FUSE_KERNEL_VERSION {
            init_in.minor.min(FUSE_KERNEL_MINOR_VERSION)
        } else {
            0
        };

        self.max_readahead = init_in.max_readahead.min(self.max_readahead);
        self.flags = init_in.flags & (
            cap_flags::FUSE_ASYNC_READ |
            cap_flags::FUSE_BIG_WRITES |
            cap_flags::FUSE_WRITEBACK_CACHE |
            cap_flags::FUSE_DO_READDIRPLUS |
            cap_flags::FUSE_PARALLEL_DIROPS
        );

        self.connected.store(true, Ordering::Release);

        FuseInitOut {
            major: self.proto_major,
            minor: self.proto_minor,
            max_readahead: self.max_readahead,
            flags: self.flags,
            max_background: 16,
            congestion_threshold: 12,
            max_write: self.max_write,
            time_gran: 1,
            max_pages: 256,
            map_alignment: 0,
            flags2: 0,
            unused: [0; 7],
        }
    }

    /// Queue a request
    pub fn queue_request(&self, opcode: FuseOpcode, nodeid: u64, data: Vec<u8>) -> u64 {
        let unique = self.next_unique.fetch_add(1, Ordering::Relaxed);

        let header = FuseInHeader {
            len: (core::mem::size_of::<FuseInHeader>() + data.len()) as u32,
            opcode: opcode as u32,
            unique,
            nodeid,
            uid: 0,
            gid: 0,
            pid: 0,
            padding: 0,
        };

        let request = FuseRequest { header, data };
        self.pending.write().push_back(request);

        unique
    }

    /// Read next pending request (from userspace)
    pub fn read_request(&self) -> Option<FuseRequest> {
        let mut pending = self.pending.write();
        if let Some(request) = pending.pop_front() {
            self.processing.write().insert(request.header.unique, request.clone());
            Some(request)
        } else {
            None
        }
    }

    /// Write response (from userspace)
    pub fn write_response(&self, response: FuseResponse) -> Result<(), FuseError> {
        let mut processing = self.processing.write();

        if processing.remove(&response.header.unique).is_none() {
            return Err(FuseError::InvalidRequest);
        }

        // Would wake up waiting thread
        Ok(())
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    /// Disconnect
    pub fn disconnect(&self) {
        self.connected.store(false, Ordering::Release);
    }
}

// =============================================================================
// FUSE ERROR
// =============================================================================

/// FUSE error
#[derive(Clone, Debug)]
pub enum FuseError {
    /// Not connected
    NotConnected,
    /// Invalid request
    InvalidRequest,
    /// Operation not supported
    NotSupported,
    /// Would block
    WouldBlock,
    /// Interrupted
    Interrupted,
    /// I/O error
    IoError,
    /// Permission denied
    PermissionDenied,
}

impl FuseError {
    /// Convert to errno
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::NotConnected => -107,     // ENOTCONN
            Self::InvalidRequest => -22,    // EINVAL
            Self::NotSupported => -95,      // EOPNOTSUPP
            Self::WouldBlock => -11,        // EAGAIN
            Self::Interrupted => -4,        // EINTR
            Self::IoError => -5,            // EIO
            Self::PermissionDenied => -1,   // EPERM
        }
    }
}

// =============================================================================
// FUSE DEVICE
// =============================================================================

/// FUSE device (/dev/fuse)
pub struct FuseDevice {
    /// Open connections (fd -> connection)
    connections: RwLock<BTreeMap<i32, Arc<FuseConnection>>>,
    /// Next minor device number
    next_minor: AtomicU32,
}

impl FuseDevice {
    /// Create new FUSE device
    pub fn new() -> Self {
        Self {
            connections: RwLock::new(BTreeMap::new()),
            next_minor: AtomicU32::new(0),
        }
    }

    /// Open /dev/fuse
    pub fn open(&self, fd: i32) -> Result<(), FuseError> {
        let minor = self.next_minor.fetch_add(1, Ordering::Relaxed);
        let conn = Arc::new(FuseConnection::new(fd as u32, minor));
        self.connections.write().insert(fd, conn);

        crate::kprintln!("[FUSE] Opened /dev/fuse fd={} minor={}", fd, minor);
        Ok(())
    }

    /// Close /dev/fuse
    pub fn close(&self, fd: i32) -> Result<(), FuseError> {
        self.connections.write().remove(&fd)
            .ok_or(FuseError::InvalidRequest)?;

        crate::kprintln!("[FUSE] Closed /dev/fuse fd={}", fd);
        Ok(())
    }

    /// Read from /dev/fuse (userspace reads request)
    pub fn read(&self, fd: i32, buf: &mut [u8]) -> Result<usize, FuseError> {
        let connections = self.connections.read();
        let conn = connections.get(&fd).ok_or(FuseError::InvalidRequest)?;

        let request = conn.read_request().ok_or(FuseError::WouldBlock)?;

        // Serialize request
        let header_size = core::mem::size_of::<FuseInHeader>();
        let total_size = header_size + request.data.len();

        if buf.len() < total_size {
            return Err(FuseError::IoError);
        }

        // Copy header
        let header_bytes = unsafe {
            core::slice::from_raw_parts(
                &request.header as *const _ as *const u8,
                header_size,
            )
        };
        buf[..header_size].copy_from_slice(header_bytes);

        // Copy data
        buf[header_size..total_size].copy_from_slice(&request.data);

        Ok(total_size)
    }

    /// Write to /dev/fuse (userspace writes response)
    pub fn write(&self, fd: i32, buf: &[u8]) -> Result<usize, FuseError> {
        let connections = self.connections.read();
        let conn = connections.get(&fd).ok_or(FuseError::InvalidRequest)?;

        if buf.len() < core::mem::size_of::<FuseOutHeader>() {
            return Err(FuseError::InvalidRequest);
        }

        // Parse header
        let header = unsafe {
            *(buf.as_ptr() as *const FuseOutHeader)
        };

        // Get data
        let data_start = core::mem::size_of::<FuseOutHeader>();
        let data = buf[data_start..].to_vec();

        let response = FuseResponse { header, data };
        conn.write_response(response)?;

        Ok(buf.len())
    }

    /// Get connection
    pub fn get_connection(&self, fd: i32) -> Option<Arc<FuseConnection>> {
        self.connections.read().get(&fd).cloned()
    }
}

impl Default for FuseDevice {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// GLOBAL STATE
// =============================================================================

/// Global FUSE device
static FUSE_DEV: RwLock<Option<FuseDevice>> = RwLock::new(None);

/// FUSE mounts
static FUSE_MOUNTS: RwLock<BTreeMap<String, Arc<FuseConnection>>> = RwLock::new(BTreeMap::new());

// =============================================================================
// SYSCALL IMPLEMENTATIONS
// =============================================================================

/// Open /dev/fuse
pub fn dev_open(fd: i32) -> Result<(), FuseError> {
    let dev = FUSE_DEV.read();
    dev.as_ref().ok_or(FuseError::NotConnected)?.open(fd)
}

/// Close /dev/fuse
pub fn dev_close(fd: i32) -> Result<(), FuseError> {
    let dev = FUSE_DEV.read();
    dev.as_ref().ok_or(FuseError::NotConnected)?.close(fd)
}

/// Read from /dev/fuse
pub fn dev_read(fd: i32, buf: &mut [u8]) -> Result<usize, FuseError> {
    let dev = FUSE_DEV.read();
    dev.as_ref().ok_or(FuseError::NotConnected)?.read(fd, buf)
}

/// Write to /dev/fuse
pub fn dev_write(fd: i32, buf: &[u8]) -> Result<usize, FuseError> {
    let dev = FUSE_DEV.read();
    dev.as_ref().ok_or(FuseError::NotConnected)?.write(fd, buf)
}

/// Mount FUSE filesystem
pub fn mount(source: &str, target: &str, fd: i32) -> Result<(), FuseError> {
    let dev = FUSE_DEV.read();
    let dev = dev.as_ref().ok_or(FuseError::NotConnected)?;

    let conn = dev.get_connection(fd).ok_or(FuseError::InvalidRequest)?;

    FUSE_MOUNTS.write().insert(target.into(), conn);

    crate::kprintln!("[FUSE] Mounted {} at {} (fd={})", source, target, fd);
    Ok(())
}

/// Unmount FUSE filesystem
pub fn unmount(target: &str) -> Result<(), FuseError> {
    let conn = FUSE_MOUNTS.write().remove(target)
        .ok_or(FuseError::InvalidRequest)?;

    conn.disconnect();

    crate::kprintln!("[FUSE] Unmounted {}", target);
    Ok(())
}

/// Initialize FUSE subsystem
pub fn init() {
    *FUSE_DEV.write() = Some(FuseDevice::new());
    crate::kprintln!("[FS] FUSE (Filesystem in Userspace) initialized");
}
