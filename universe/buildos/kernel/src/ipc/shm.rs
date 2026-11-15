// ===============================================================================
// BUILDOS KERNEL - SYSTEM V SHARED MEMORY
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! System V Shared Memory Implementation
//!
//! Provides shared memory segments that can be attached to multiple processes.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use super::{IpcKey, IpcId, IpcPerm, IpcError, IpcNamespace, IPC_PRIVATE, IPC_CREAT, IPC_EXCL};
use super::{IPC_RMID, IPC_SET, IPC_STAT, IPC_INFO, SHMMNI};
use crate::process::Pid;
use crate::sync::RwLock;

// =============================================================================
// CONSTANTS
// =============================================================================

/// Maximum shared memory segment size (1GB)
pub const SHMMAX: usize = 1024 * 1024 * 1024;

/// Minimum shared memory segment size (1 byte)
pub const SHMMIN: usize = 1;

/// Maximum total shared memory (8GB)
pub const SHMALL: usize = 8 * 1024 * 1024 * 1024;

/// Maximum attachments per segment
pub const SHMSEG: usize = 4096;

/// Shared memory flags
pub const SHM_RDONLY: u32 = 0o10000;  // Attach read-only
pub const SHM_RND: u32 = 0o20000;     // Round attach address
pub const SHM_REMAP: u32 = 0o40000;   // Replace existing mapping
pub const SHM_EXEC: u32 = 0o100000;   // Executable mapping

/// Shared memory control commands
pub const SHM_LOCK: u32 = 11;    // Lock pages in memory
pub const SHM_UNLOCK: u32 = 12;  // Unlock pages
pub const SHM_STAT: u32 = 13;    // Like IPC_STAT but uses index
pub const SHM_INFO: u32 = 14;    // Get system info
pub const SHM_STAT_ANY: u32 = 15; // Like SHM_STAT, no permission check

// =============================================================================
// SHARED MEMORY SEGMENT
// =============================================================================

/// Shared memory segment
pub struct ShmSegment {
    /// IPC permissions
    pub perm: IpcPerm,
    /// Segment size in bytes
    pub size: usize,
    /// Attach time
    pub atime: u64,
    /// Detach time
    pub dtime: u64,
    /// Change time
    pub ctime: u64,
    /// Creator PID
    pub cpid: Pid,
    /// Last operation PID
    pub lpid: Pid,
    /// Number of current attaches
    pub nattch: AtomicU32,
    /// Physical pages
    pub pages: Vec<u64>,
    /// Segment locked in memory
    pub locked: bool,
    /// Marked for deletion
    pub destroyed: bool,
    /// Huge pages
    pub huge_pages: bool,
    /// NUMA node (-1 for any)
    pub numa_node: i32,
    /// Attachments (PID -> address)
    pub attachments: BTreeMap<Pid, Vec<u64>>,
}

impl ShmSegment {
    /// Create new shared memory segment
    pub fn new(key: IpcKey, size: usize, uid: u32, gid: u32, mode: u32) -> Self {
        let pages_needed = (size + 4095) / 4096;

        Self {
            perm: IpcPerm::new(key, uid, gid, mode),
            size,
            atime: 0,
            dtime: 0,
            ctime: current_time(),
            cpid: current_pid(),
            lpid: Pid::default(),
            nattch: AtomicU32::new(0),
            pages: Vec::with_capacity(pages_needed),
            locked: false,
            destroyed: false,
            huge_pages: false,
            numa_node: -1,
            attachments: BTreeMap::new(),
        }
    }

    /// Allocate physical pages for segment
    pub fn allocate_pages(&mut self) -> Result<(), IpcError> {
        let pages_needed = (self.size + 4095) / 4096;

        for _ in 0..pages_needed {
            // Would allocate physical page
            self.pages.push(0);
        }

        Ok(())
    }

    /// Add attachment
    pub fn attach(&mut self, pid: Pid, addr: u64) {
        self.nattch.fetch_add(1, Ordering::Relaxed);
        self.atime = current_time();
        self.lpid = pid;

        self.attachments.entry(pid).or_insert_with(Vec::new).push(addr);
    }

    /// Remove attachment
    pub fn detach(&mut self, pid: Pid, addr: u64) {
        self.nattch.fetch_sub(1, Ordering::Relaxed);
        self.dtime = current_time();
        self.lpid = pid;

        if let Some(addrs) = self.attachments.get_mut(&pid) {
            addrs.retain(|&a| a != addr);
            if addrs.is_empty() {
                self.attachments.remove(&pid);
            }
        }
    }
}

/// Shared memory data structure (for shmctl)
#[derive(Clone, Debug, Default)]
pub struct ShmidDs {
    /// IPC permissions
    pub perm: ShmPerm,
    /// Segment size
    pub segsz: usize,
    /// Last attach time
    pub atime: u64,
    /// Last detach time
    pub dtime: u64,
    /// Last change time
    pub ctime: u64,
    /// Creator PID
    pub cpid: u32,
    /// Last operation PID
    pub lpid: u32,
    /// Number of attaches
    pub nattch: u32,
}

/// Simplified IPC perm for shmctl
#[derive(Clone, Debug, Default)]
pub struct ShmPerm {
    pub key: u32,
    pub uid: u32,
    pub gid: u32,
    pub cuid: u32,
    pub cgid: u32,
    pub mode: u32,
    pub seq: u32,
}

/// Shared memory info
#[derive(Clone, Debug, Default)]
pub struct ShmInfo {
    /// Number of segments
    pub used_ids: usize,
    /// Total shared memory (pages)
    pub shm_tot: usize,
    /// Resident shared memory (pages)
    pub shm_rss: usize,
    /// Swapped shared memory (pages)
    pub shm_swp: usize,
    /// Maximum segment size
    pub shmmax: usize,
    /// Minimum segment size
    pub shmmin: usize,
    /// Maximum segments
    pub shmmni: usize,
    /// Maximum total size
    pub shmall: usize,
}

// =============================================================================
// GLOBAL STATE
// =============================================================================

/// Shared memory segments by key
static SHM_BY_KEY: RwLock<BTreeMap<IpcKey, IpcId>> = RwLock::new(BTreeMap::new());

/// Shared memory segments by ID
static SHM_BY_ID: RwLock<BTreeMap<IpcId, ShmSegment>> = RwLock::new(BTreeMap::new());

/// Next shared memory ID
static SHM_NEXT_ID: AtomicU32 = AtomicU32::new(1);

/// Attachment tracking (address -> (shmid, pid))
static SHM_ATTACHMENTS: RwLock<BTreeMap<u64, (IpcId, Pid)>> = RwLock::new(BTreeMap::new());

// =============================================================================
// INITIALIZATION
// =============================================================================

/// Initialize shared memory subsystem
pub fn init() {
    crate::kprintln!("[IPC/SHM] Shared memory subsystem initialized");
}

// =============================================================================
// SYSTEM CALLS
// =============================================================================

/// shmget - get or create shared memory segment
pub fn shmget(key: IpcKey, size: usize, flags: u32) -> Result<IpcId, IpcError> {
    // Validate size
    if size < SHMMIN || size > SHMMAX {
        return Err(IpcError::InvalidArgument);
    }

    let mode = flags & 0o777;
    let create = (flags & IPC_CREAT) != 0;
    let exclusive = (flags & IPC_EXCL) != 0;

    // Check for existing segment
    if key != IPC_PRIVATE {
        let by_key = SHM_BY_KEY.read();
        if let Some(&shmid) = by_key.get(&key) {
            if exclusive {
                return Err(IpcError::Exists);
            }

            // Check size matches
            let segments = SHM_BY_ID.read();
            if let Some(seg) = segments.get(&shmid) {
                if seg.size < size {
                    return Err(IpcError::InvalidArgument);
                }
                // Check permissions
                let (uid, gid) = current_credentials();
                if !seg.perm.can_read(uid, gid) {
                    return Err(IpcError::PermissionDenied);
                }
            }
            return Ok(shmid);
        }
        drop(by_key);
    }

    // Create if needed
    if !create && key != IPC_PRIVATE {
        return Err(IpcError::NotFound);
    }

    // Check limits
    if SHM_BY_ID.read().len() >= SHMMNI {
        return Err(IpcError::ResourceLimit);
    }

    // Create segment
    let (uid, gid) = current_credentials();
    let mut segment = ShmSegment::new(key, size, uid, gid, mode);
    segment.allocate_pages()?;

    // Allocate ID
    let shmid = SHM_NEXT_ID.fetch_add(1, Ordering::Relaxed);

    // Store segment
    if key != IPC_PRIVATE {
        SHM_BY_KEY.write().insert(key, shmid);
    }
    SHM_BY_ID.write().insert(shmid, segment);

    Ok(shmid)
}

/// shmat - attach shared memory segment
pub fn shmat(shmid: IpcId, shmaddr: Option<u64>, flags: u32) -> Result<u64, IpcError> {
    let read_only = (flags & SHM_RDONLY) != 0;
    let round_addr = (flags & SHM_RND) != 0;

    let mut segments = SHM_BY_ID.write();
    let segment = segments.get_mut(&shmid).ok_or(IpcError::InvalidId)?;

    // Check permissions
    let (uid, gid) = current_credentials();
    if read_only {
        if !segment.perm.can_read(uid, gid) {
            return Err(IpcError::PermissionDenied);
        }
    } else {
        if !segment.perm.can_write(uid, gid) {
            return Err(IpcError::PermissionDenied);
        }
    }

    // Check if marked for deletion
    if segment.destroyed {
        return Err(IpcError::InvalidId);
    }

    // Determine attach address
    let addr = if let Some(mut addr) = shmaddr {
        if round_addr {
            // Round down to page boundary
            addr &= !0xFFF;
        }
        // Would validate address is free in process address space
        addr
    } else {
        // Find free address in process
        find_free_address(segment.size)?
    };

    // Map pages into process address space
    // Would call mmap internally

    // Record attachment
    let pid = current_pid();
    segment.attach(pid, addr);
    SHM_ATTACHMENTS.write().insert(addr, (shmid, pid));

    Ok(addr)
}

/// shmdt - detach shared memory segment
pub fn shmdt(shmaddr: u64) -> Result<(), IpcError> {
    // Find attachment
    let mut attachments = SHM_ATTACHMENTS.write();
    let (shmid, _pid) = attachments.remove(&shmaddr).ok_or(IpcError::InvalidArgument)?;
    drop(attachments);

    // Update segment
    let mut segments = SHM_BY_ID.write();
    if let Some(segment) = segments.get_mut(&shmid) {
        let pid = current_pid();
        segment.detach(pid, shmaddr);

        // If marked for deletion and no more attachments, remove
        if segment.destroyed && segment.nattch.load(Ordering::Relaxed) == 0 {
            let key = segment.perm.key;
            segments.remove(&shmid);
            if key != IPC_PRIVATE {
                SHM_BY_KEY.write().remove(&key);
            }
        }
    }

    // Unmap from process address space
    // Would call munmap internally

    Ok(())
}

/// shmctl - shared memory control
pub fn shmctl(shmid: IpcId, cmd: u32, buf: Option<&mut ShmidDs>) -> Result<i32, IpcError> {
    match cmd {
        IPC_STAT | SHM_STAT | SHM_STAT_ANY => {
            let segments = SHM_BY_ID.read();
            let segment = segments.get(&shmid).ok_or(IpcError::InvalidId)?;

            // Permission check (except for SHM_STAT_ANY)
            if cmd != SHM_STAT_ANY {
                let (uid, gid) = current_credentials();
                if !segment.perm.can_read(uid, gid) {
                    return Err(IpcError::PermissionDenied);
                }
            }

            if let Some(buf) = buf {
                buf.perm = ShmPerm {
                    key: segment.perm.key,
                    uid: segment.perm.uid,
                    gid: segment.perm.gid,
                    cuid: segment.perm.cuid,
                    cgid: segment.perm.cgid,
                    mode: segment.perm.mode,
                    seq: segment.perm.seq,
                };
                buf.segsz = segment.size;
                buf.atime = segment.atime;
                buf.dtime = segment.dtime;
                buf.ctime = segment.ctime;
                buf.cpid = segment.cpid.as_u64() as u32;
                buf.lpid = segment.lpid.as_u64() as u32;
                buf.nattch = segment.nattch.load(Ordering::Relaxed);
            }

            Ok(0)
        }
        IPC_SET => {
            let mut segments = SHM_BY_ID.write();
            let segment = segments.get_mut(&shmid).ok_or(IpcError::InvalidId)?;

            // Must be owner or root
            let (uid, _gid) = current_credentials();
            if uid != segment.perm.uid && uid != 0 {
                return Err(IpcError::PermissionDenied);
            }

            if let Some(buf) = buf {
                segment.perm.uid = buf.perm.uid;
                segment.perm.gid = buf.perm.gid;
                segment.perm.mode = buf.perm.mode & 0o777;
                segment.ctime = current_time();
            }

            Ok(0)
        }
        IPC_RMID => {
            let mut segments = SHM_BY_ID.write();
            let segment = segments.get_mut(&shmid).ok_or(IpcError::InvalidId)?;

            // Must be owner or root
            let (uid, _gid) = current_credentials();
            if uid != segment.perm.uid && uid != 0 {
                return Err(IpcError::PermissionDenied);
            }

            if segment.nattch.load(Ordering::Relaxed) == 0 {
                // No attachments, remove immediately
                let key = segment.perm.key;
                segments.remove(&shmid);
                if key != IPC_PRIVATE {
                    SHM_BY_KEY.write().remove(&key);
                }
            } else {
                // Mark for deletion when last process detaches
                segment.destroyed = true;
            }

            Ok(0)
        }
        IPC_INFO | SHM_INFO => {
            // Return system-wide info
            if let Some(buf) = buf {
                let info = get_info(None);
                buf.segsz = info.shmmax;
                // Other fields would be populated
            }
            Ok(SHM_BY_ID.read().len() as i32)
        }
        SHM_LOCK => {
            let mut segments = SHM_BY_ID.write();
            let segment = segments.get_mut(&shmid).ok_or(IpcError::InvalidId)?;

            // Requires CAP_IPC_LOCK
            segment.locked = true;
            segment.ctime = current_time();

            Ok(0)
        }
        SHM_UNLOCK => {
            let mut segments = SHM_BY_ID.write();
            let segment = segments.get_mut(&shmid).ok_or(IpcError::InvalidId)?;

            segment.locked = false;
            segment.ctime = current_time();

            Ok(0)
        }
        _ => Err(IpcError::InvalidArgument),
    }
}

// =============================================================================
// INFO AND LISTING
// =============================================================================

/// Get shared memory info
pub fn get_info(_ns: Option<&IpcNamespace>) -> ShmInfo {
    let segments = SHM_BY_ID.read();

    let mut shm_tot = 0;
    let mut shm_rss = 0;

    for seg in segments.values() {
        shm_tot += seg.pages.len();
        shm_rss += seg.pages.len(); // Simplified
    }

    ShmInfo {
        used_ids: segments.len(),
        shm_tot,
        shm_rss,
        shm_swp: 0,
        shmmax: SHMMAX,
        shmmin: SHMMIN,
        shmmni: SHMMNI,
        shmall: SHMALL / 4096,
    }
}

/// Generate /proc/sysvipc/shm listing
pub fn proc_list() -> String {
    let segments = SHM_BY_ID.read();

    let mut output = String::from(
        "       key      shmid perms       size  cpid  lpid nattch   uid   gid  cuid  cgid      atime      dtime      ctime\n"
    );

    for (&shmid, seg) in segments.iter() {
        output.push_str(&alloc::format!(
            "{:>10} {:>10} {:>5o} {:>10} {:>5} {:>5} {:>6} {:>5} {:>5} {:>5} {:>5} {:>10} {:>10} {:>10}\n",
            seg.perm.key, shmid, seg.perm.mode, seg.size,
            seg.cpid, seg.lpid, seg.nattch.load(Ordering::Relaxed),
            seg.perm.uid, seg.perm.gid, seg.perm.cuid, seg.perm.cgid,
            seg.atime, seg.dtime, seg.ctime
        ));
    }

    output
}

// =============================================================================
// PROCESS EXIT CLEANUP
// =============================================================================

/// Clean up shared memory for exiting process
pub fn process_exit(pid: Pid) {
    let mut to_detach = Vec::new();

    // Find all attachments for this process
    for (&addr, &(shmid, attach_pid)) in SHM_ATTACHMENTS.read().iter() {
        if attach_pid == pid {
            to_detach.push((addr, shmid));
        }
    }

    // Detach all
    for (addr, _shmid) in to_detach {
        let _ = shmdt(addr);
    }
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

fn current_time() -> u64 {
    0 // Would get actual time
}

fn current_pid() -> Pid {
    Pid::default() // Would get current process PID
}

fn current_credentials() -> (u32, u32) {
    (0, 0) // Would get current UID/GID
}

fn find_free_address(size: usize) -> Result<u64, IpcError> {
    // Would find free region in process address space
    let _ = size;
    Ok(0x7F000000) // Placeholder
}
