// ===============================================================================
// BUILDOS KERNEL - INTER-PROCESS COMMUNICATION (IPC)
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Inter-Process Communication Subsystem
//!
//! This module provides:
//! - System V shared memory (shmget, shmat, shmdt, shmctl)
//! - System V message queues (msgget, msgsnd, msgrcv, msgctl)
//! - System V semaphores (semget, semop, semctl)
//! - POSIX shared memory (shm_open, shm_unlink)
//! - POSIX message queues (mq_open, mq_send, mq_receive)
//! - Named pipes (FIFOs)
//! - Unix domain sockets
//! - Eventfd for event notification
//! - Signalfd for signal delivery via file descriptor

#![allow(dead_code)]

pub mod shm;
pub mod msg;
pub mod sem;
pub mod pipe;
pub mod eventfd;
pub mod signalfd;
pub mod socket;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::process::Pid;
use crate::sync::RwLock;

// =============================================================================
// CONSTANTS
// =============================================================================

/// Maximum IPC key value
pub const IPC_KEY_MAX: u32 = 0x7FFFFFFF;

/// Private key (generate new ID)
pub const IPC_PRIVATE: u32 = 0;

/// IPC permission flags
pub const IPC_CREAT: u32 = 0o1000;   // Create if not exists
pub const IPC_EXCL: u32 = 0o2000;    // Fail if exists
pub const IPC_NOWAIT: u32 = 0o4000;  // Don't wait

/// IPC control commands
pub const IPC_RMID: u32 = 0;  // Remove
pub const IPC_SET: u32 = 1;   // Set options
pub const IPC_STAT: u32 = 2;  // Get status
pub const IPC_INFO: u32 = 3;  // Get info

/// Maximum IPC resources
pub const SHMMNI: usize = 4096;  // Max shared memory segments
pub const MSGMNI: usize = 32000; // Max message queues
pub const SEMMNI: usize = 32000; // Max semaphore sets

// =============================================================================
// IPC KEY AND ID
// =============================================================================

/// IPC key type (System V style)
pub type IpcKey = u32;

/// IPC identifier
pub type IpcId = u32;

/// Generate IPC key from pathname and project ID
pub fn ftok(pathname: &str, proj_id: u8) -> IpcKey {
    // Simplified: hash pathname and combine with proj_id
    let mut hash: u32 = 0;
    for byte in pathname.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(byte as u32);
    }
    (hash & 0x00FFFFFF) | ((proj_id as u32) << 24)
}

// =============================================================================
// IPC PERMISSIONS
// =============================================================================

/// IPC permission structure
#[derive(Clone, Debug)]
pub struct IpcPerm {
    /// Key
    pub key: IpcKey,
    /// Owner's user ID
    pub uid: u32,
    /// Owner's group ID
    pub gid: u32,
    /// Creator's user ID
    pub cuid: u32,
    /// Creator's group ID
    pub cgid: u32,
    /// Access permissions
    pub mode: u32,
    /// Sequence number
    pub seq: u32,
}

impl IpcPerm {
    /// Create new IPC permissions
    pub fn new(key: IpcKey, uid: u32, gid: u32, mode: u32) -> Self {
        Self {
            key,
            uid,
            gid,
            cuid: uid,
            cgid: gid,
            mode: mode & 0o777,
            seq: 0,
        }
    }

    /// Check if access is allowed
    pub fn check_access(&self, uid: u32, gid: u32, access: u32) -> bool {
        // Owner permissions
        if uid == self.uid {
            return (self.mode >> 6) & access == access;
        }
        // Group permissions
        if gid == self.gid {
            return (self.mode >> 3) & access == access;
        }
        // Other permissions
        self.mode & access == access
    }

    /// Read permission
    pub fn can_read(&self, uid: u32, gid: u32) -> bool {
        self.check_access(uid, gid, 4)
    }

    /// Write permission
    pub fn can_write(&self, uid: u32, gid: u32) -> bool {
        self.check_access(uid, gid, 2)
    }
}

// =============================================================================
// IPC NAMESPACE
// =============================================================================

/// IPC namespace for containerization
pub struct IpcNamespace {
    /// Namespace ID
    pub id: u64,
    /// Shared memory segments
    pub shm_ids: BTreeMap<IpcId, shm::ShmSegment>,
    /// Message queues
    pub msg_ids: BTreeMap<IpcId, msg::MsgQueue>,
    /// Semaphore sets
    pub sem_ids: BTreeMap<IpcId, sem::SemSet>,
    /// Next IPC ID
    next_id: AtomicU32,
}

impl IpcNamespace {
    /// Create new IPC namespace
    pub fn new(id: u64) -> Self {
        Self {
            id,
            shm_ids: BTreeMap::new(),
            msg_ids: BTreeMap::new(),
            sem_ids: BTreeMap::new(),
            next_id: AtomicU32::new(1),
        }
    }

    /// Allocate next IPC ID
    pub fn alloc_id(&self) -> IpcId {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }
}

/// Global IPC namespace (init namespace)
static INIT_IPC_NS: RwLock<Option<IpcNamespace>> = RwLock::new(None);

/// IPC namespace counter
static IPC_NS_COUNTER: AtomicU64 = AtomicU64::new(1);

// =============================================================================
// IPC ERRORS
// =============================================================================

/// IPC error types
#[derive(Clone, Debug)]
pub enum IpcError {
    /// Permission denied
    PermissionDenied,
    /// Resource exists
    Exists,
    /// Resource not found
    NotFound,
    /// Invalid argument
    InvalidArgument,
    /// No memory
    NoMemory,
    /// Resource limit exceeded
    ResourceLimit,
    /// Would block
    WouldBlock,
    /// Interrupted
    Interrupted,
    /// Invalid identifier
    InvalidId,
    /// Message too long
    MessageTooLong,
    /// No message
    NoMessage,
}

impl IpcError {
    /// Convert to errno
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::PermissionDenied => -1,   // EPERM
            Self::Exists => -17,            // EEXIST
            Self::NotFound => -2,           // ENOENT
            Self::InvalidArgument => -22,   // EINVAL
            Self::NoMemory => -12,          // ENOMEM
            Self::ResourceLimit => -28,     // ENOSPC
            Self::WouldBlock => -11,        // EAGAIN
            Self::Interrupted => -4,        // EINTR
            Self::InvalidId => -22,         // EINVAL
            Self::MessageTooLong => -90,    // EMSGSIZE
            Self::NoMessage => -42,         // ENOMSG
        }
    }
}

// =============================================================================
// IPC INFO STRUCTURES
// =============================================================================

/// System-wide IPC info
#[derive(Clone, Debug, Default)]
pub struct IpcInfo {
    /// Shared memory info
    pub shm: shm::ShmInfo,
    /// Message queue info
    pub msg: msg::MsgInfo,
    /// Semaphore info
    pub sem: sem::SemInfo,
}

/// Get system-wide IPC info
pub fn get_info() -> IpcInfo {
    let ns = INIT_IPC_NS.read();
    let ns = ns.as_ref();

    IpcInfo {
        shm: shm::get_info(ns),
        msg: msg::get_info(ns),
        sem: sem::get_info(ns),
    }
}

// =============================================================================
// INITIALIZATION
// =============================================================================

/// Initialize IPC subsystem
pub fn init() {
    // Create init IPC namespace
    let ns = IpcNamespace::new(0);
    *INIT_IPC_NS.write() = Some(ns);

    // Initialize submodules
    shm::init();
    msg::init();
    sem::init();
    pipe::init();
    eventfd::init();
    signalfd::init();
    socket::init();

    crate::kprintln!("[IPC] Inter-process communication subsystem initialized");
}

/// Get init IPC namespace
pub fn init_ns() -> &'static RwLock<Option<IpcNamespace>> {
    &INIT_IPC_NS
}

/// Create new IPC namespace
pub fn new_namespace() -> IpcNamespace {
    let id = IPC_NS_COUNTER.fetch_add(1, Ordering::Relaxed);
    IpcNamespace::new(id)
}

// =============================================================================
// SYSTEM CALL INTERFACE
// =============================================================================

/// shmget - get shared memory segment
pub fn sys_shmget(key: IpcKey, size: usize, flags: u32) -> Result<IpcId, IpcError> {
    shm::shmget(key, size, flags)
}

/// shmat - attach shared memory segment
pub fn sys_shmat(shmid: IpcId, shmaddr: Option<u64>, flags: u32) -> Result<u64, IpcError> {
    shm::shmat(shmid, shmaddr, flags)
}

/// shmdt - detach shared memory segment
pub fn sys_shmdt(shmaddr: u64) -> Result<(), IpcError> {
    shm::shmdt(shmaddr)
}

/// shmctl - shared memory control
pub fn sys_shmctl(shmid: IpcId, cmd: u32, buf: Option<&mut shm::ShmidDs>) -> Result<i32, IpcError> {
    shm::shmctl(shmid, cmd, buf)
}

/// msgget - get message queue
pub fn sys_msgget(key: IpcKey, flags: u32) -> Result<IpcId, IpcError> {
    msg::msgget(key, flags)
}

/// msgsnd - send message
pub fn sys_msgsnd(msqid: IpcId, msgp: &msg::MsgBuf, flags: u32) -> Result<(), IpcError> {
    msg::msgsnd(msqid, msgp, flags)
}

/// msgrcv - receive message
pub fn sys_msgrcv(msqid: IpcId, msgp: &mut msg::MsgBuf, msgtyp: i64, flags: u32) -> Result<usize, IpcError> {
    msg::msgrcv(msqid, msgp, msgtyp, flags)
}

/// msgctl - message queue control
pub fn sys_msgctl(msqid: IpcId, cmd: u32, buf: Option<&mut msg::MsqidDs>) -> Result<i32, IpcError> {
    msg::msgctl(msqid, cmd, buf)
}

/// semget - get semaphore set
pub fn sys_semget(key: IpcKey, nsems: u32, flags: u32) -> Result<IpcId, IpcError> {
    sem::semget(key, nsems, flags)
}

/// semop - semaphore operations
pub fn sys_semop(semid: IpcId, sops: &[sem::SemBuf]) -> Result<(), IpcError> {
    sem::semop(semid, sops)
}

/// semctl - semaphore control
pub fn sys_semctl(semid: IpcId, semnum: u32, cmd: u32, arg: Option<sem::SemArg>) -> Result<i32, IpcError> {
    sem::semctl(semid, semnum, cmd, arg)
}

/// eventfd - create event file descriptor
pub fn sys_eventfd(initval: u64, flags: u32) -> Result<i32, IpcError> {
    eventfd::eventfd(initval, flags)
}

/// signalfd - create signal file descriptor
pub fn sys_signalfd(fd: i32, mask: u64, flags: u32) -> Result<i32, IpcError> {
    signalfd::signalfd(fd, mask, flags)
}

/// pipe - create pipe
pub fn sys_pipe(flags: u32) -> Result<(i32, i32), IpcError> {
    pipe::pipe(flags)
}

// =============================================================================
// PROC INTERFACE
// =============================================================================

/// Read /proc/sysvipc/shm
pub fn proc_sysvipc_shm() -> String {
    shm::proc_list()
}

/// Read /proc/sysvipc/msg
pub fn proc_sysvipc_msg() -> String {
    msg::proc_list()
}

/// Read /proc/sysvipc/sem
pub fn proc_sysvipc_sem() -> String {
    sem::proc_list()
}

// =============================================================================
// RESOURCE CLEANUP
// =============================================================================

/// Clean up IPC resources for exiting process
pub fn process_exit(pid: Pid) {
    // Detach all shared memory
    shm::process_exit(pid);

    // Remove from semaphore undo lists
    sem::process_exit(pid);
}

/// Clean up IPC resources for exec
pub fn process_exec(pid: Pid) {
    // Close IPC file descriptors marked close-on-exec
    let _ = pid;
}

// =============================================================================
// ERROR CODES (POSIX errno)
// =============================================================================

/// IPC error codes
pub mod errno {
    pub const EPERM: i32 = 1;
    pub const ENOENT: i32 = 2;
    pub const ESRCH: i32 = 3;
    pub const EINTR: i32 = 4;
    pub const EIO: i32 = 5;
    pub const EAGAIN: i32 = 11;
    pub const ENOMEM: i32 = 12;
    pub const EACCES: i32 = 13;
    pub const EEXIST: i32 = 17;
    pub const EINVAL: i32 = 22;
    pub const EMFILE: i32 = 24;
    pub const ENOSPC: i32 = 28;
    pub const EPIPE: i32 = 32;
    pub const EWOULDBLOCK: i32 = 11;
    pub const EMSGSIZE: i32 = 90;
    pub const EIDRM: i32 = 43;
    pub const ENOMSG: i32 = 42;
}

// =============================================================================
// COMPATIBILITY TYPE ALIASES
// =============================================================================

/// Message queue ID (compatibility alias)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MqId(pub u64);

/// Shared memory ID (compatibility alias)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShmId(pub u64);

/// Semaphore ID (compatibility alias)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SemId(pub u64);

// Re-export SigSet from signalfd
pub use signalfd::SigSet;

// =============================================================================
// SIGNAL ACTION
// =============================================================================

/// Signal action
#[derive(Clone, Copy)]
pub enum SigAction {
    /// Default action
    Default,
    /// Ignore signal
    Ignore,
    /// Custom handler
    Handler(u64),
    /// Signal-specific info handler
    SigInfo(u64),
}

// =============================================================================
// FUTEX (Fast Userspace Mutex)
// =============================================================================

/// Futex operations
pub mod futex_op {
    pub const FUTEX_WAIT: i32 = 0;
    pub const FUTEX_WAKE: i32 = 1;
    pub const FUTEX_REQUEUE: i32 = 3;
    pub const FUTEX_CMP_REQUEUE: i32 = 4;
    pub const FUTEX_WAKE_OP: i32 = 5;
    pub const FUTEX_WAIT_BITSET: i32 = 9;
    pub const FUTEX_WAKE_BITSET: i32 = 10;
    pub const FUTEX_PRIVATE_FLAG: i32 = 128;
}

/// Futex waiter entry
struct FutexWaiter {
    pid: Pid,
    bitset: u32,
}

/// Futex wait queues
static FUTEX_QUEUES: RwLock<BTreeMap<u64, Vec<FutexWaiter>>> = RwLock::new(BTreeMap::new());

/// Futex wait
pub fn futex_wait(addr: u64, expected: u32, bitset: u32, pid: Pid) -> Result<(), i32> {
    // Read current value at address
    let current = unsafe { *(addr as *const u32) };

    if current != expected {
        return Err(-11); // EAGAIN
    }

    // Add to wait queue
    let mut queues = FUTEX_QUEUES.write();
    let queue = queues.entry(addr).or_insert_with(Vec::new);
    queue.push(FutexWaiter { pid, bitset });

    Ok(())
}

/// Futex wake
pub fn futex_wake(addr: u64, count: i32, bitset: u32) -> Result<i32, i32> {
    let mut queues = FUTEX_QUEUES.write();

    if let Some(queue) = queues.get_mut(&addr) {
        let mut woken = 0i32;
        let mut i = 0;

        while i < queue.len() && woken < count {
            if (queue[i].bitset & bitset) != 0 {
                let _waiter = queue.remove(i);
                woken += 1;
            } else {
                i += 1;
            }
        }

        Ok(woken)
    } else {
        Ok(0)
    }
}

// =============================================================================
// POSIX COMPATIBILITY FUNCTIONS
// =============================================================================

/// Create pipe (returns read_fd, write_fd)
pub fn pipe_create() -> Result<(i32, i32), i32> {
    pipe::pipe(0).map_err(|e| e.to_errno())
}

/// Open POSIX message queue
pub fn mq_open(name: &str, create: bool, _max_msgs: usize, _max_msg_size: usize) -> Result<MqId, i32> {
    let flags = if create { IPC_CREAT } else { 0 };
    let key = ftok(name, 1);
    msg::msgget(key, flags)
        .map(|id| MqId(id as u64))
        .map_err(|e| e.to_errno())
}

/// Send to POSIX message queue
pub fn mq_send(id: MqId, data: &[u8], priority: u32, sender: Pid) -> Result<(), i32> {
    let _ = (priority, sender);
    let msgbuf = msg::MsgBuf {
        mtype: 1,
        mtext: data.to_vec(),
    };
    msg::msgsnd(id.0 as u32, &msgbuf, 0).map_err(|e| e.to_errno())
}

/// Receive from POSIX message queue
pub fn mq_receive(id: MqId, mtype: i64) -> Result<msg::Message, i32> {
    let mut msgbuf = msg::MsgBuf::new(0, Vec::with_capacity(8192));
    msg::msgrcv(id.0 as u32, &mut msgbuf, mtype, 0)
        .map(|_| msg::Message {
            mtype: msgbuf.mtype,
            mtext: msgbuf.mtext,
            sender: Pid::KERNEL,
            time: 0,
        })
        .map_err(|e| e.to_errno())
}

/// Open POSIX shared memory
pub fn shm_open(name: &str, create: bool, size: usize) -> Result<ShmId, i32> {
    let flags = if create { IPC_CREAT } else { 0 };
    let key = ftok(name, 2);
    shm::shmget(key, size, flags)
        .map(|id| ShmId(id as u64))
        .map_err(|e| e.to_errno())
}

/// Attach POSIX shared memory
pub fn shm_attach(id: ShmId, _pid: Pid, addr: u64) -> Result<u64, i32> {
    let shmaddr = if addr == 0 { None } else { Some(addr) };
    shm::shmat(id.0 as u32, shmaddr, 0).map_err(|e| e.to_errno())
}

/// Open POSIX semaphore
pub fn sem_open(name: &str, create: bool, initial: i32) -> Result<SemId, i32> {
    let flags = if create { IPC_CREAT } else { 0 };
    let key = ftok(name, 3);
    sem::semget(key, 1, flags)
        .map(|id| {
            // Set initial value if creating
            if create {
                let _ = sem::semctl(id, 0, sem::SETVAL, Some(sem::SemArg::Val(initial as i32)));
            }
            SemId(id as u64)
        })
        .map_err(|e| e.to_errno())
}

/// Try wait on POSIX semaphore (non-blocking)
pub fn sem_trywait(id: SemId) -> Result<(), i32> {
    let sop = sem::SemBuf {
        sem_num: 0,
        sem_op: -1,
        sem_flg: IPC_NOWAIT as u16,
    };
    sem::semop(id.0 as u32, &[sop]).map_err(|e| e.to_errno())
}

/// Post to POSIX semaphore
pub fn sem_post(id: SemId) -> Result<(), i32> {
    let sop = sem::SemBuf {
        sem_num: 0,
        sem_op: 1,
        sem_flg: 0,
    };
    sem::semop(id.0 as u32, &[sop]).map_err(|e| e.to_errno())
}

/// Create eventfd
pub fn eventfd_create(initial: u64, flags: u32) -> Result<i32, i32> {
    eventfd::eventfd(initial, flags).map_err(|e| e.to_errno())
}

/// Set signal action (stub)
pub fn sigaction(sig: i32, action: SigAction) -> Result<SigAction, i32> {
    // Would set signal action via process module
    let _ = (sig, action);
    Ok(SigAction::Default)
}

/// Set signal mask (stub)
pub fn sigprocmask(how: i32, set: &SigSet) -> Result<SigSet, i32> {
    // Would set signal mask via process module
    let _ = (how, set);
    Ok(SigSet::empty())
}

/// Send signal to process (stub)
pub fn kill(pid: Pid, sig: i32) -> Result<(), i32> {
    // Would send signal via process module
    let _ = (pid, sig);
    Ok(())
}
