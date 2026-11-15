// ===============================================================================
// BUILDOS KERNEL - SYSTEM V MESSAGE QUEUES
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! System V Message Queue Implementation
//!
//! Provides message queues for inter-process communication.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use super::{IpcKey, IpcId, IpcPerm, IpcError, IpcNamespace, IPC_PRIVATE, IPC_CREAT, IPC_EXCL, IPC_NOWAIT};
use super::{IPC_RMID, IPC_SET, IPC_STAT, IPC_INFO, MSGMNI};
use crate::process::Pid;
use crate::sync::RwLock;

// =============================================================================
// CONSTANTS
// =============================================================================

/// Maximum message size (8KB)
pub const MSGMAX: usize = 8192;

/// Maximum bytes in queue (16KB)
pub const MSGMNB: usize = 16384;

/// Maximum messages in system
pub const MSGTQL: usize = 16384;

/// Maximum message types
pub const MSGPOOL: usize = 1024;

/// Message queue control commands
pub const MSG_STAT: u32 = 11;
pub const MSG_INFO: u32 = 12;
pub const MSG_STAT_ANY: u32 = 13;
pub const MSG_COPY: u32 = 040000;     // Copy message
pub const MSG_EXCEPT: u32 = 020000;   // Receive any except type
pub const MSG_NOERROR: u32 = 010000;  // Truncate message

// =============================================================================
// MESSAGE STRUCTURES
// =============================================================================

/// A message
#[derive(Clone)]
pub struct Message {
    /// Message type (> 0)
    pub mtype: i64,
    /// Message data
    pub mtext: Vec<u8>,
    /// Sender PID
    pub sender: Pid,
    /// Send time
    pub time: u64,
}

impl Message {
    /// Create new message
    pub fn new(mtype: i64, data: &[u8], sender: Pid) -> Self {
        Self {
            mtype,
            mtext: data.to_vec(),
            sender,
            time: current_time(),
        }
    }

    /// Size of message
    pub fn size(&self) -> usize {
        self.mtext.len()
    }
}

/// Message buffer for send/receive
#[derive(Clone)]
pub struct MsgBuf {
    /// Message type
    pub mtype: i64,
    /// Message data
    pub mtext: Vec<u8>,
}

impl MsgBuf {
    /// Create new message buffer
    pub fn new(mtype: i64, data: Vec<u8>) -> Self {
        Self { mtype, mtext: data }
    }

    /// Size of message data
    pub fn size(&self) -> usize {
        self.mtext.len()
    }
}

// =============================================================================
// MESSAGE QUEUE
// =============================================================================

/// Message queue
#[derive(Clone)]
pub struct MsgQueue {
    /// IPC permissions
    pub perm: IpcPerm,
    /// Messages
    pub messages: VecDeque<Message>,
    /// Maximum bytes in queue
    pub msg_qbytes: usize,
    /// Current bytes in queue
    pub msg_cbytes: usize,
    /// Number of messages
    pub msg_qnum: usize,
    /// Last send time
    pub msg_stime: u64,
    /// Last receive time
    pub msg_rtime: u64,
    /// Last change time
    pub msg_ctime: u64,
    /// Last send PID
    pub msg_lspid: Pid,
    /// Last receive PID
    pub msg_lrpid: Pid,
    /// Waiting senders
    pub senders_waiting: u32,
    /// Waiting receivers
    pub receivers_waiting: u32,
}

impl MsgQueue {
    /// Create new message queue
    pub fn new(key: IpcKey, uid: u32, gid: u32, mode: u32) -> Self {
        Self {
            perm: IpcPerm::new(key, uid, gid, mode),
            messages: VecDeque::new(),
            msg_qbytes: MSGMNB,
            msg_cbytes: 0,
            msg_qnum: 0,
            msg_stime: 0,
            msg_rtime: 0,
            msg_ctime: current_time(),
            msg_lspid: Pid::default(),
            msg_lrpid: Pid::default(),
            senders_waiting: 0,
            receivers_waiting: 0,
        }
    }

    /// Check if queue is full
    pub fn is_full(&self, msg_size: usize) -> bool {
        self.msg_cbytes + msg_size > self.msg_qbytes
    }

    /// Add message to queue
    pub fn enqueue(&mut self, msg: Message) {
        self.msg_cbytes += msg.size();
        self.msg_qnum += 1;
        self.msg_stime = current_time();
        self.msg_lspid = msg.sender;
        self.messages.push_back(msg);
    }

    /// Get message from queue by type
    pub fn dequeue(&mut self, msgtyp: i64, except: bool) -> Option<Message> {
        let idx = if msgtyp == 0 {
            // Any message
            Some(0)
        } else if msgtyp > 0 {
            if except {
                // Any message except msgtyp
                self.messages.iter().position(|m| m.mtype != msgtyp)
            } else {
                // Specific type
                self.messages.iter().position(|m| m.mtype == msgtyp)
            }
        } else {
            // Lowest type <= |msgtyp|
            let max_type = -msgtyp;
            let mut best_idx = None;
            let mut best_type = i64::MAX;
            for (i, m) in self.messages.iter().enumerate() {
                if m.mtype <= max_type && m.mtype < best_type {
                    best_idx = Some(i);
                    best_type = m.mtype;
                }
            }
            best_idx
        }?;

        let msg = self.messages.remove(idx)?;
        self.msg_cbytes -= msg.size();
        self.msg_qnum -= 1;
        self.msg_rtime = current_time();

        Some(msg)
    }

    /// Peek at message without removing
    pub fn peek(&self, msgtyp: i64, except: bool) -> Option<&Message> {
        if msgtyp == 0 {
            self.messages.front()
        } else if msgtyp > 0 {
            if except {
                self.messages.iter().find(|m| m.mtype != msgtyp)
            } else {
                self.messages.iter().find(|m| m.mtype == msgtyp)
            }
        } else {
            let max_type = -msgtyp;
            self.messages.iter()
                .filter(|m| m.mtype <= max_type)
                .min_by_key(|m| m.mtype)
        }
    }
}

/// Message queue status structure
#[derive(Clone, Debug, Default)]
pub struct MsqidDs {
    /// IPC permissions
    pub perm: MsgPerm,
    /// Last send time
    pub stime: u64,
    /// Last receive time
    pub rtime: u64,
    /// Last change time
    pub ctime: u64,
    /// Current bytes in queue
    pub cbytes: u64,
    /// Number of messages
    pub qnum: u64,
    /// Maximum bytes
    pub qbytes: u64,
    /// Last send PID
    pub lspid: u32,
    /// Last receive PID
    pub lrpid: u32,
}

/// Simplified IPC perm
#[derive(Clone, Debug, Default)]
pub struct MsgPerm {
    pub key: u32,
    pub uid: u32,
    pub gid: u32,
    pub cuid: u32,
    pub cgid: u32,
    pub mode: u32,
    pub seq: u32,
}

/// Message info
#[derive(Clone, Debug, Default)]
pub struct MsgInfo {
    /// Number of message queues
    pub msgpool: usize,
    /// Maximum messages
    pub msgmap: usize,
    /// Maximum message size
    pub msgmax: usize,
    /// Maximum queue size
    pub msgmnb: usize,
    /// Maximum queues
    pub msgmni: usize,
    /// Size of message segment
    pub msgssz: usize,
    /// Number of messages
    pub msgtql: usize,
    /// Max segments
    pub msgseg: usize,
}

// =============================================================================
// GLOBAL STATE
// =============================================================================

/// Message queues by key
static MSG_BY_KEY: RwLock<BTreeMap<IpcKey, IpcId>> = RwLock::new(BTreeMap::new());

/// Message queues by ID
static MSG_BY_ID: RwLock<BTreeMap<IpcId, MsgQueue>> = RwLock::new(BTreeMap::new());

/// Next message queue ID
static MSG_NEXT_ID: AtomicU32 = AtomicU32::new(1);

/// Total messages in system
static MSG_TOTAL: AtomicU64 = AtomicU64::new(0);

// =============================================================================
// INITIALIZATION
// =============================================================================

/// Initialize message queue subsystem
pub fn init() {
    crate::kprintln!("[IPC/MSG] Message queue subsystem initialized");
}

// =============================================================================
// SYSTEM CALLS
// =============================================================================

/// msgget - get or create message queue
pub fn msgget(key: IpcKey, flags: u32) -> Result<IpcId, IpcError> {
    let mode = flags & 0o777;
    let create = (flags & IPC_CREAT) != 0;
    let exclusive = (flags & IPC_EXCL) != 0;

    // Check for existing queue
    if key != IPC_PRIVATE {
        let by_key = MSG_BY_KEY.read();
        if let Some(&msqid) = by_key.get(&key) {
            if exclusive {
                return Err(IpcError::Exists);
            }

            // Check permissions
            let queues = MSG_BY_ID.read();
            if let Some(queue) = queues.get(&msqid) {
                let (uid, gid) = current_credentials();
                if !queue.perm.can_read(uid, gid) {
                    return Err(IpcError::PermissionDenied);
                }
            }
            return Ok(msqid);
        }
        drop(by_key);
    }

    // Create if needed
    if !create && key != IPC_PRIVATE {
        return Err(IpcError::NotFound);
    }

    // Check limits
    if MSG_BY_ID.read().len() >= MSGMNI {
        return Err(IpcError::ResourceLimit);
    }

    // Create queue
    let (uid, gid) = current_credentials();
    let queue = MsgQueue::new(key, uid, gid, mode);

    // Allocate ID
    let msqid = MSG_NEXT_ID.fetch_add(1, Ordering::Relaxed);

    // Store queue
    if key != IPC_PRIVATE {
        MSG_BY_KEY.write().insert(key, msqid);
    }
    MSG_BY_ID.write().insert(msqid, queue);

    Ok(msqid)
}

/// msgsnd - send message
pub fn msgsnd(msqid: IpcId, msgp: &MsgBuf, flags: u32) -> Result<(), IpcError> {
    let nowait = (flags & IPC_NOWAIT) != 0;

    // Validate message type
    if msgp.mtype <= 0 {
        return Err(IpcError::InvalidArgument);
    }

    // Validate message size
    if msgp.size() > MSGMAX {
        return Err(IpcError::MessageTooLong);
    }

    loop {
        let mut queues = MSG_BY_ID.write();
        let queue = queues.get_mut(&msqid).ok_or(IpcError::InvalidId)?;

        // Check write permission
        let (uid, gid) = current_credentials();
        if !queue.perm.can_write(uid, gid) {
            return Err(IpcError::PermissionDenied);
        }

        // Check if queue has space
        if queue.is_full(msgp.size()) {
            if nowait {
                return Err(IpcError::WouldBlock);
            }
            // Would block waiting for space
            queue.senders_waiting += 1;
            drop(queues);
            // Wait...
            continue;
        }

        // Create and enqueue message
        let pid = current_pid();
        let msg = Message::new(msgp.mtype, &msgp.mtext, pid);
        queue.enqueue(msg);

        MSG_TOTAL.fetch_add(1, Ordering::Relaxed);

        // Wake any waiting receivers
        // queue.receiver_wait.wake_one();

        return Ok(());
    }
}

/// msgrcv - receive message
pub fn msgrcv(msqid: IpcId, msgp: &mut MsgBuf, msgtyp: i64, flags: u32) -> Result<usize, IpcError> {
    let nowait = (flags & IPC_NOWAIT) != 0;
    let noerror = (flags & MSG_NOERROR) != 0;
    let except = (flags & MSG_EXCEPT) != 0;

    loop {
        let mut queues = MSG_BY_ID.write();
        let queue = queues.get_mut(&msqid).ok_or(IpcError::InvalidId)?;

        // Check read permission
        let (uid, gid) = current_credentials();
        if !queue.perm.can_read(uid, gid) {
            return Err(IpcError::PermissionDenied);
        }

        // Try to get message
        if let Some(msg) = queue.dequeue(msgtyp, except) {
            queue.msg_lrpid = current_pid();

            MSG_TOTAL.fetch_sub(1, Ordering::Relaxed);

            // Check size
            if msg.size() > msgp.mtext.capacity() {
                if noerror {
                    msgp.mtext = msg.mtext[..msgp.mtext.capacity()].to_vec();
                } else {
                    // Put message back (simplified - would use different approach)
                    return Err(IpcError::MessageTooLong);
                }
            } else {
                msgp.mtext = msg.mtext;
            }
            msgp.mtype = msg.mtype;

            // Wake any waiting senders
            // queue.sender_wait.wake_one();

            return Ok(msgp.size());
        }

        // No matching message
        if nowait {
            return Err(IpcError::NoMessage);
        }

        // Block waiting for message
        queue.receivers_waiting += 1;
        drop(queues);
        // Wait...
    }
}

/// msgctl - message queue control
pub fn msgctl(msqid: IpcId, cmd: u32, buf: Option<&mut MsqidDs>) -> Result<i32, IpcError> {
    match cmd {
        IPC_STAT | MSG_STAT | MSG_STAT_ANY => {
            let queues = MSG_BY_ID.read();
            let queue = queues.get(&msqid).ok_or(IpcError::InvalidId)?;

            // Permission check (except for MSG_STAT_ANY)
            if cmd != MSG_STAT_ANY {
                let (uid, gid) = current_credentials();
                if !queue.perm.can_read(uid, gid) {
                    return Err(IpcError::PermissionDenied);
                }
            }

            if let Some(buf) = buf {
                buf.perm = MsgPerm {
                    key: queue.perm.key,
                    uid: queue.perm.uid,
                    gid: queue.perm.gid,
                    cuid: queue.perm.cuid,
                    cgid: queue.perm.cgid,
                    mode: queue.perm.mode,
                    seq: queue.perm.seq,
                };
                buf.stime = queue.msg_stime;
                buf.rtime = queue.msg_rtime;
                buf.ctime = queue.msg_ctime;
                buf.cbytes = queue.msg_cbytes as u64;
                buf.qnum = queue.msg_qnum as u64;
                buf.qbytes = queue.msg_qbytes as u64;
                buf.lspid = queue.msg_lspid.as_u64() as u32;
                buf.lrpid = queue.msg_lrpid.as_u64() as u32;
            }

            Ok(0)
        }
        IPC_SET => {
            let mut queues = MSG_BY_ID.write();
            let queue = queues.get_mut(&msqid).ok_or(IpcError::InvalidId)?;

            // Must be owner or root
            let (uid, _gid) = current_credentials();
            if uid != queue.perm.uid && uid != 0 {
                return Err(IpcError::PermissionDenied);
            }

            if let Some(buf) = buf {
                queue.perm.uid = buf.perm.uid;
                queue.perm.gid = buf.perm.gid;
                queue.perm.mode = buf.perm.mode & 0o777;
                queue.msg_qbytes = buf.qbytes as usize;
                queue.msg_ctime = current_time();
            }

            Ok(0)
        }
        IPC_RMID => {
            let mut queues = MSG_BY_ID.write();
            let queue = queues.get(&msqid).ok_or(IpcError::InvalidId)?;

            // Must be owner or root
            let (uid, _gid) = current_credentials();
            if uid != queue.perm.uid && uid != 0 {
                return Err(IpcError::PermissionDenied);
            }

            let key = queue.perm.key;

            // Remove queue
            queues.remove(&msqid);
            if key != IPC_PRIVATE {
                MSG_BY_KEY.write().remove(&key);
            }

            Ok(0)
        }
        IPC_INFO | MSG_INFO => {
            if let Some(buf) = buf {
                buf.qbytes = MSGMNB as u64;
                buf.qnum = MSG_BY_ID.read().len() as u64;
            }
            Ok(MSG_BY_ID.read().len() as i32)
        }
        _ => Err(IpcError::InvalidArgument),
    }
}

// =============================================================================
// INFO AND LISTING
// =============================================================================

/// Get message queue info
pub fn get_info(_ns: Option<&IpcNamespace>) -> MsgInfo {
    MsgInfo {
        msgpool: MSGPOOL,
        msgmap: 0,
        msgmax: MSGMAX,
        msgmnb: MSGMNB,
        msgmni: MSGMNI,
        msgssz: 8,
        msgtql: MSGTQL,
        msgseg: 0,
    }
}

/// Generate /proc/sysvipc/msg listing
pub fn proc_list() -> String {
    let queues = MSG_BY_ID.read();

    let mut output = String::from(
        "       key      msqid perms      cbytes       qnum lspid lrpid   uid   gid  cuid  cgid      stime      rtime      ctime\n"
    );

    for (&msqid, queue) in queues.iter() {
        output.push_str(&alloc::format!(
            "{:>10} {:>10} {:>5o} {:>11} {:>10} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} {:>10} {:>10} {:>10}\n",
            queue.perm.key, msqid, queue.perm.mode,
            queue.msg_cbytes, queue.msg_qnum,
            queue.msg_lspid.as_u64(), queue.msg_lrpid.as_u64(),
            queue.perm.uid, queue.perm.gid, queue.perm.cuid, queue.perm.cgid,
            queue.msg_stime, queue.msg_rtime, queue.msg_ctime
        ));
    }

    output
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

fn current_time() -> u64 {
    0
}

fn current_pid() -> Pid {
    Pid::default()
}

fn current_credentials() -> (u32, u32) {
    (0, 0)
}
