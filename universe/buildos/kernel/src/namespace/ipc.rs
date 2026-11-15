// ===============================================================================
// BUILDOS KERNEL - IPC NAMESPACE
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! IPC Namespace Implementation
//!
//! Provides IPC resource isolation. Each IPC namespace has its own:
//! - System V IPC objects (message queues, semaphores, shared memory)
//! - POSIX message queues

#![allow(dead_code)]

use alloc::sync::Arc;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::sync::RwLock;
use super::{Namespace, NsType, NsError, next_ns_id};
use super::user::UserNamespace;

/// IPC namespace structure
pub struct IpcNamespace {
    /// Namespace ID
    id: u64,
    /// Owning user namespace
    user_ns: Arc<UserNamespace>,
    /// Message queues
    msg_queues: RwLock<BTreeMap<i32, MessageQueue>>,
    /// Semaphore sets
    semaphores: RwLock<BTreeMap<i32, SemaphoreSet>>,
    /// Shared memory segments
    shm_segments: RwLock<BTreeMap<i32, SharedMemory>>,
    /// POSIX message queues
    mqueues: RwLock<BTreeMap<String, PosixMqueue>>,
    /// IPC ID counter
    ipc_id_counter: AtomicU32,
    /// Resource limits
    limits: RwLock<IpcLimits>,
    /// Statistics
    stats: RwLock<IpcStats>,
}

impl IpcNamespace {
    /// Create initial (root) IPC namespace
    pub fn new_initial(user_ns: Arc<UserNamespace>) -> Self {
        Self {
            id: next_ns_id(),
            user_ns,
            msg_queues: RwLock::new(BTreeMap::new()),
            semaphores: RwLock::new(BTreeMap::new()),
            shm_segments: RwLock::new(BTreeMap::new()),
            mqueues: RwLock::new(BTreeMap::new()),
            ipc_id_counter: AtomicU32::new(0),
            limits: RwLock::new(IpcLimits::default()),
            stats: RwLock::new(IpcStats::default()),
        }
    }

    /// Create child IPC namespace (empty, not copied)
    pub fn new_child(user_ns: Arc<UserNamespace>) -> Self {
        Self {
            id: next_ns_id(),
            user_ns,
            msg_queues: RwLock::new(BTreeMap::new()),
            semaphores: RwLock::new(BTreeMap::new()),
            shm_segments: RwLock::new(BTreeMap::new()),
            mqueues: RwLock::new(BTreeMap::new()),
            ipc_id_counter: AtomicU32::new(0),
            limits: RwLock::new(IpcLimits::default()),
            stats: RwLock::new(IpcStats::default()),
        }
    }

    /// Allocate IPC ID
    fn alloc_id(&self) -> i32 {
        self.ipc_id_counter.fetch_add(1, Ordering::Relaxed) as i32
    }

    // =========================================================================
    // Message Queue Operations
    // =========================================================================

    /// Create message queue
    pub fn msgget(&self, key: i32, flags: i32) -> Result<i32, NsError> {
        let queues = self.msg_queues.read();

        if key != IPC_PRIVATE {
            // Look for existing queue with this key
            for (id, q) in queues.iter() {
                if q.key == key {
                    if flags & IPC_CREAT != 0 && flags & IPC_EXCL != 0 {
                        return Err(NsError::InvalidOperation);
                    }
                    return Ok(*id);
                }
            }
        }

        // Create new queue
        if flags & IPC_CREAT == 0 && key != IPC_PRIVATE {
            return Err(NsError::NotFound);
        }

        drop(queues);

        let id = self.alloc_id();
        let queue = MessageQueue {
            key,
            mode: (flags & 0o777) as u16,
            uid: 0,
            gid: 0,
            messages: Vec::new(),
            max_bytes: self.limits.read().msgmnb,
            current_bytes: 0,
        };

        self.msg_queues.write().insert(id, queue);
        self.stats.write().msg_queues += 1;

        Ok(id)
    }

    /// Send message
    pub fn msgsnd(&self, msqid: i32, msg: &[u8], msgtype: i64) -> Result<(), NsError> {
        let mut queues = self.msg_queues.write();
        let queue = queues.get_mut(&msqid).ok_or(NsError::NotFound)?;

        let limits = self.limits.read();
        if queue.current_bytes + msg.len() as u64 > queue.max_bytes {
            return Err(NsError::ResourceLimit);
        }
        if msg.len() as u64 > limits.msgmax {
            return Err(NsError::InvalidOperation);
        }

        queue.messages.push(Message {
            mtype: msgtype,
            data: msg.to_vec(),
        });
        queue.current_bytes += msg.len() as u64;

        Ok(())
    }

    /// Receive message
    pub fn msgrcv(&self, msqid: i32, msgtype: i64) -> Result<Message, NsError> {
        let mut queues = self.msg_queues.write();
        let queue = queues.get_mut(&msqid).ok_or(NsError::NotFound)?;

        let idx = if msgtype == 0 {
            // Return first message
            if queue.messages.is_empty() {
                return Err(NsError::NotFound);
            }
            0
        } else if msgtype > 0 {
            // Return first message of specified type
            queue.messages.iter().position(|m| m.mtype == msgtype)
                .ok_or(NsError::NotFound)?
        } else {
            // Return message with smallest type <= |msgtype|
            let abs_type = (-msgtype) as i64;
            queue.messages.iter()
                .enumerate()
                .filter(|(_, m)| m.mtype <= abs_type)
                .min_by_key(|(_, m)| m.mtype)
                .map(|(i, _)| i)
                .ok_or(NsError::NotFound)?
        };

        let msg = queue.messages.remove(idx);
        queue.current_bytes -= msg.data.len() as u64;

        Ok(msg)
    }

    /// Remove message queue
    pub fn msgctl_rmid(&self, msqid: i32) -> Result<(), NsError> {
        self.msg_queues.write().remove(&msqid)
            .ok_or(NsError::NotFound)?;
        self.stats.write().msg_queues -= 1;
        Ok(())
    }

    // =========================================================================
    // Semaphore Operations
    // =========================================================================

    /// Create semaphore set
    pub fn semget(&self, key: i32, nsems: i32, flags: i32) -> Result<i32, NsError> {
        let sems = self.semaphores.read();

        if key != IPC_PRIVATE {
            for (id, s) in sems.iter() {
                if s.key == key {
                    if flags & IPC_CREAT != 0 && flags & IPC_EXCL != 0 {
                        return Err(NsError::InvalidOperation);
                    }
                    return Ok(*id);
                }
            }
        }

        if flags & IPC_CREAT == 0 && key != IPC_PRIVATE {
            return Err(NsError::NotFound);
        }

        if nsems as u64 > self.limits.read().semmsl {
            return Err(NsError::ResourceLimit);
        }

        drop(sems);

        let id = self.alloc_id();
        let sem_set = SemaphoreSet {
            key,
            mode: (flags & 0o777) as u16,
            uid: 0,
            gid: 0,
            semaphores: vec![Semaphore::default(); nsems as usize],
        };

        self.semaphores.write().insert(id, sem_set);
        self.stats.write().semaphores += nsems as u64;

        Ok(id)
    }

    /// Semaphore operation
    pub fn semop(&self, semid: i32, ops: &[SemOp]) -> Result<(), NsError> {
        let mut sems = self.semaphores.write();
        let sem_set = sems.get_mut(&semid).ok_or(NsError::NotFound)?;

        // Check all operations first
        for op in ops {
            if op.sem_num as usize >= sem_set.semaphores.len() {
                return Err(NsError::InvalidOperation);
            }
        }

        // Perform operations
        for op in ops {
            let sem = &mut sem_set.semaphores[op.sem_num as usize];

            if op.sem_op > 0 {
                // Increment
                sem.semval += op.sem_op as i32;
            } else if op.sem_op < 0 {
                // Decrement (would block if insufficient)
                let decr = (-op.sem_op) as i32;
                if sem.semval < decr {
                    return Err(NsError::ResourceLimit); // Would block
                }
                sem.semval -= decr;
            }
            // op.sem_op == 0 would wait for zero
        }

        Ok(())
    }

    /// Remove semaphore set
    pub fn semctl_rmid(&self, semid: i32) -> Result<(), NsError> {
        let set = self.semaphores.write().remove(&semid)
            .ok_or(NsError::NotFound)?;
        self.stats.write().semaphores -= set.semaphores.len() as u64;
        Ok(())
    }

    // =========================================================================
    // Shared Memory Operations
    // =========================================================================

    /// Create shared memory segment
    pub fn shmget(&self, key: i32, size: u64, flags: i32) -> Result<i32, NsError> {
        let shms = self.shm_segments.read();

        if key != IPC_PRIVATE {
            for (id, s) in shms.iter() {
                if s.key == key {
                    if flags & IPC_CREAT != 0 && flags & IPC_EXCL != 0 {
                        return Err(NsError::InvalidOperation);
                    }
                    return Ok(*id);
                }
            }
        }

        if flags & IPC_CREAT == 0 && key != IPC_PRIVATE {
            return Err(NsError::NotFound);
        }

        if size > self.limits.read().shmmax {
            return Err(NsError::ResourceLimit);
        }

        drop(shms);

        let id = self.alloc_id();
        let shm = SharedMemory {
            key,
            size,
            mode: (flags & 0o777) as u16,
            uid: 0,
            gid: 0,
            nattach: AtomicU32::new(0),
            address: 0, // Would allocate actual memory
        };

        self.shm_segments.write().insert(id, shm);
        self.stats.write().shm_bytes += size;

        Ok(id)
    }

    /// Attach shared memory
    pub fn shmat(&self, shmid: i32) -> Result<u64, NsError> {
        let shms = self.shm_segments.read();
        let shm = shms.get(&shmid).ok_or(NsError::NotFound)?;

        shm.nattach.fetch_add(1, Ordering::Relaxed);

        // Would map into process address space
        Ok(shm.address)
    }

    /// Detach shared memory
    pub fn shmdt(&self, shmid: i32) -> Result<(), NsError> {
        let shms = self.shm_segments.read();
        let shm = shms.get(&shmid).ok_or(NsError::NotFound)?;

        shm.nattach.fetch_sub(1, Ordering::Relaxed);

        Ok(())
    }

    /// Remove shared memory segment
    pub fn shmctl_rmid(&self, shmid: i32) -> Result<(), NsError> {
        let shm = self.shm_segments.write().remove(&shmid)
            .ok_or(NsError::NotFound)?;
        self.stats.write().shm_bytes -= shm.size;
        Ok(())
    }

    // =========================================================================
    // POSIX Message Queue Operations
    // =========================================================================

    /// Open POSIX message queue
    pub fn mq_open(&self, name: &str, flags: i32, maxmsg: u32, msgsize: u32) -> Result<i32, NsError> {
        let mut mqs = self.mqueues.write();

        if let Some(_) = mqs.get(name) {
            if flags & O_CREAT != 0 && flags & O_EXCL != 0 {
                return Err(NsError::InvalidOperation);
            }
            return Ok(self.alloc_id());
        }

        if flags & O_CREAT == 0 {
            return Err(NsError::NotFound);
        }

        let mq = PosixMqueue {
            name: name.into(),
            maxmsg,
            msgsize,
            messages: Vec::new(),
            mode: 0o644,
            uid: 0,
            gid: 0,
        };

        mqs.insert(name.into(), mq);

        Ok(self.alloc_id())
    }

    /// Unlink POSIX message queue
    pub fn mq_unlink(&self, name: &str) -> Result<(), NsError> {
        self.mqueues.write().remove(name)
            .ok_or(NsError::NotFound)?;
        Ok(())
    }
}

impl Namespace for IpcNamespace {
    fn ns_type(&self) -> NsType {
        NsType::Ipc
    }

    fn id(&self) -> u64 {
        self.id
    }

    fn user_ns(&self) -> Option<Arc<UserNamespace>> {
        Some(self.user_ns.clone())
    }

    fn clone_ns(&self) -> Arc<dyn Namespace> {
        Arc::new(Self::new_child(self.user_ns.clone()))
    }
}

/// IPC flags
pub const IPC_PRIVATE: i32 = 0;
pub const IPC_CREAT: i32 = 0o1000;
pub const IPC_EXCL: i32 = 0o2000;
pub const IPC_NOWAIT: i32 = 0o4000;

/// Open flags for POSIX mqueue
pub const O_CREAT: i32 = 0o100;
pub const O_EXCL: i32 = 0o200;
pub const O_RDONLY: i32 = 0;
pub const O_WRONLY: i32 = 1;
pub const O_RDWR: i32 = 2;

/// Message queue
struct MessageQueue {
    key: i32,
    mode: u16,
    uid: u32,
    gid: u32,
    messages: Vec<Message>,
    max_bytes: u64,
    current_bytes: u64,
}

/// Message
pub struct Message {
    pub mtype: i64,
    pub data: Vec<u8>,
}

/// Semaphore set
struct SemaphoreSet {
    key: i32,
    mode: u16,
    uid: u32,
    gid: u32,
    semaphores: Vec<Semaphore>,
}

/// Individual semaphore
#[derive(Clone, Default)]
struct Semaphore {
    semval: i32,
    sempid: u32,
}

/// Semaphore operation
pub struct SemOp {
    pub sem_num: u16,
    pub sem_op: i16,
    pub sem_flg: i16,
}

/// Shared memory segment
struct SharedMemory {
    key: i32,
    size: u64,
    mode: u16,
    uid: u32,
    gid: u32,
    nattach: AtomicU32,
    address: u64,
}

/// POSIX message queue
struct PosixMqueue {
    name: String,
    maxmsg: u32,
    msgsize: u32,
    messages: Vec<Message>,
    mode: u16,
    uid: u32,
    gid: u32,
}

/// IPC resource limits
#[derive(Clone)]
struct IpcLimits {
    /// Max message size
    msgmax: u64,
    /// Max bytes in queue
    msgmnb: u64,
    /// Max queues system-wide
    msgmni: u64,
    /// Max semaphores per set
    semmsl: u64,
    /// Max semaphore sets
    semmni: u64,
    /// Max shared memory segment size
    shmmax: u64,
    /// Min shared memory segment size
    shmmin: u64,
    /// Max shared memory segments
    shmmni: u64,
}

impl Default for IpcLimits {
    fn default() -> Self {
        Self {
            msgmax: 8192,
            msgmnb: 16384,
            msgmni: 32000,
            semmsl: 32000,
            semmni: 32000,
            shmmax: 18446744073692774399, // ULONG_MAX - (1UL << 24)
            shmmin: 1,
            shmmni: 4096,
        }
    }
}

/// IPC statistics
#[derive(Clone, Default)]
struct IpcStats {
    msg_queues: u64,
    semaphores: u64,
    shm_bytes: u64,
}
