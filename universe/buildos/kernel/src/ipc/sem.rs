// ===============================================================================
// BUILDOS KERNEL - SYSTEM V SEMAPHORES
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! System V Semaphore Implementation
//!
//! Provides semaphore sets for inter-process synchronization.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU16, Ordering};

use super::{IpcKey, IpcId, IpcPerm, IpcError, IpcNamespace, IPC_PRIVATE, IPC_CREAT, IPC_EXCL, IPC_NOWAIT};
use super::{IPC_RMID, IPC_SET, IPC_STAT, IPC_INFO, SEMMNI};
use crate::process::Pid;
use crate::sync::RwLock;

// =============================================================================
// CONSTANTS
// =============================================================================

/// Maximum semaphores per set
pub const SEMMSL: usize = 250;

/// Maximum semaphore value
pub const SEMVMX: u16 = 32767;

/// Maximum undo entries per process
pub const SEMMNU: usize = 128;

/// Maximum operations per semop
pub const SEMOPM: usize = 32;

/// Semaphore control commands
pub const GETVAL: u32 = 12;   // Get semaphore value
pub const SETVAL: u32 = 16;   // Set semaphore value
pub const GETPID: u32 = 11;   // Get sempid
pub const GETNCNT: u32 = 14;  // Get semncnt
pub const GETZCNT: u32 = 15;  // Get semzcnt
pub const GETALL: u32 = 13;   // Get all values
pub const SETALL: u32 = 17;   // Set all values
pub const SEM_STAT: u32 = 18;
pub const SEM_INFO: u32 = 19;
pub const SEM_STAT_ANY: u32 = 20;

/// Semaphore operation flags
pub const SEM_UNDO: u16 = 0x1000;

// =============================================================================
// SEMAPHORE STRUCTURES
// =============================================================================

/// A single semaphore
pub struct Semaphore {
    /// Semaphore value
    pub semval: AtomicU16,
    /// PID of last operation
    pub sempid: Pid,
    /// Processes waiting for increase
    pub semncnt: AtomicU16,
    /// Processes waiting for zero
    pub semzcnt: AtomicU16,
}

impl Semaphore {
    /// Create new semaphore
    pub fn new() -> Self {
        Self {
            semval: AtomicU16::new(0),
            sempid: Pid::default(),
            semncnt: AtomicU16::new(0),
            semzcnt: AtomicU16::new(0),
        }
    }

    /// Get value
    pub fn value(&self) -> u16 {
        self.semval.load(Ordering::Acquire)
    }

    /// Set value
    pub fn set_value(&self, val: u16) {
        self.semval.store(val.min(SEMVMX), Ordering::Release);
    }

    /// Add to value (may go negative for pending ops)
    pub fn add(&self, delta: i16) -> Result<u16, ()> {
        let current = self.semval.load(Ordering::Acquire) as i32;
        let new_val = current + delta as i32;

        if new_val < 0 {
            return Err(()); // Would block
        }
        if new_val > SEMVMX as i32 {
            return Err(()); // Overflow
        }

        self.semval.store(new_val as u16, Ordering::Release);
        Ok(new_val as u16)
    }
}

impl Default for Semaphore {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Semaphore {
    fn clone(&self) -> Self {
        Self {
            semval: AtomicU16::new(self.semval.load(Ordering::Acquire)),
            sempid: self.sempid,
            semncnt: AtomicU16::new(self.semncnt.load(Ordering::Acquire)),
            semzcnt: AtomicU16::new(self.semzcnt.load(Ordering::Acquire)),
        }
    }
}

/// Semaphore set
#[derive(Clone)]
pub struct SemSet {
    /// IPC permissions
    pub perm: IpcPerm,
    /// Semaphores
    pub sems: Vec<Semaphore>,
    /// Number of semaphores
    pub nsems: u32,
    /// Last semop time
    pub sem_otime: u64,
    /// Last change time
    pub sem_ctime: u64,
    /// Pending operations
    pub pending: Vec<PendingOp>,
}

impl SemSet {
    /// Create new semaphore set
    pub fn new(key: IpcKey, nsems: u32, uid: u32, gid: u32, mode: u32) -> Self {
        let mut sems = Vec::with_capacity(nsems as usize);
        for _ in 0..nsems {
            sems.push(Semaphore::new());
        }

        Self {
            perm: IpcPerm::new(key, uid, gid, mode),
            sems,
            nsems,
            sem_otime: 0,
            sem_ctime: current_time(),
            pending: Vec::new(),
        }
    }

    /// Get semaphore
    pub fn get(&self, idx: u32) -> Option<&Semaphore> {
        self.sems.get(idx as usize)
    }

    /// Get mutable semaphore
    pub fn get_mut(&mut self, idx: u32) -> Option<&mut Semaphore> {
        self.sems.get_mut(idx as usize)
    }
}

/// Semaphore operation
#[derive(Clone, Copy)]
pub struct SemBuf {
    /// Semaphore index
    pub sem_num: u16,
    /// Operation (+n, -n, or 0)
    pub sem_op: i16,
    /// Flags (SEM_UNDO, IPC_NOWAIT)
    pub sem_flg: u16,
}

/// Pending operation
#[derive(Clone)]
pub struct PendingOp {
    /// Process ID
    pub pid: Pid,
    /// Operations
    pub ops: Vec<SemBuf>,
    /// Alter operations (for undo)
    pub alter: bool,
}

/// Semaphore control argument
#[derive(Clone)]
pub enum SemArg {
    /// Integer value
    Val(i32),
    /// Buffer for getall/setall
    Array(Vec<u16>),
    /// Semid_ds structure
    Buf(SemidDs),
    /// Seminfo structure
    Info(SemInfo),
}

/// Semaphore set status
#[derive(Clone, Debug, Default)]
pub struct SemidDs {
    /// IPC permissions
    pub perm: SemPerm,
    /// Last semop time
    pub otime: u64,
    /// Last change time
    pub ctime: u64,
    /// Number of semaphores
    pub nsems: u32,
}

/// Simplified IPC perm
#[derive(Clone, Debug, Default)]
pub struct SemPerm {
    pub key: u32,
    pub uid: u32,
    pub gid: u32,
    pub cuid: u32,
    pub cgid: u32,
    pub mode: u32,
    pub seq: u32,
}

/// Semaphore info
#[derive(Clone, Debug, Default)]
pub struct SemInfo {
    /// Max semaphore sets
    pub semmni: usize,
    /// Max semaphores per set
    pub semmsl: usize,
    /// Max semaphores
    pub semmns: usize,
    /// Max operations
    pub semopm: usize,
    /// Max value
    pub semvmx: usize,
    /// Max undo
    pub semmnu: usize,
    /// Used semaphore sets
    pub semusz: usize,
    /// Total semaphores
    pub semaem: usize,
}

// =============================================================================
// SEMAPHORE UNDO
// =============================================================================

/// Per-process semaphore undo list
#[derive(Clone, Default)]
pub struct SemUndo {
    /// Undo entries (semid, semnum) -> adjustment
    pub entries: BTreeMap<(IpcId, u16), i16>,
}

impl SemUndo {
    /// Add undo entry
    pub fn add(&mut self, semid: IpcId, semnum: u16, adj: i16) {
        let key = (semid, semnum);
        let current = self.entries.get(&key).copied().unwrap_or(0);
        let new_val = current.saturating_add(-adj); // Undo is opposite of op

        if new_val == 0 {
            self.entries.remove(&key);
        } else {
            self.entries.insert(key, new_val);
        }
    }
}

/// Per-process undo lists
static SEM_UNDO_MAP: RwLock<BTreeMap<Pid, SemUndo>> = RwLock::new(BTreeMap::new());

// =============================================================================
// GLOBAL STATE
// =============================================================================

/// Semaphore sets by key
static SEM_BY_KEY: RwLock<BTreeMap<IpcKey, IpcId>> = RwLock::new(BTreeMap::new());

/// Semaphore sets by ID
static SEM_BY_ID: RwLock<BTreeMap<IpcId, SemSet>> = RwLock::new(BTreeMap::new());

/// Next semaphore set ID
static SEM_NEXT_ID: AtomicU32 = AtomicU32::new(1);

// =============================================================================
// INITIALIZATION
// =============================================================================

/// Initialize semaphore subsystem
pub fn init() {
    crate::kprintln!("[IPC/SEM] Semaphore subsystem initialized");
}

// =============================================================================
// SYSTEM CALLS
// =============================================================================

/// semget - get or create semaphore set
pub fn semget(key: IpcKey, nsems: u32, flags: u32) -> Result<IpcId, IpcError> {
    // Validate nsems
    if nsems > SEMMSL as u32 {
        return Err(IpcError::InvalidArgument);
    }

    let mode = flags & 0o777;
    let create = (flags & IPC_CREAT) != 0;
    let exclusive = (flags & IPC_EXCL) != 0;

    // Check for existing set
    if key != IPC_PRIVATE {
        let by_key = SEM_BY_KEY.read();
        if let Some(&semid) = by_key.get(&key) {
            if exclusive {
                return Err(IpcError::Exists);
            }

            // Check nsems matches (if specified)
            let sets = SEM_BY_ID.read();
            if let Some(set) = sets.get(&semid) {
                if nsems != 0 && set.nsems != nsems {
                    return Err(IpcError::InvalidArgument);
                }
                // Check permissions
                let (uid, gid) = current_credentials();
                if !set.perm.can_read(uid, gid) {
                    return Err(IpcError::PermissionDenied);
                }
            }
            return Ok(semid);
        }
        drop(by_key);
    }

    // Create if needed
    if !create && key != IPC_PRIVATE {
        return Err(IpcError::NotFound);
    }

    // nsems must be positive when creating
    if nsems == 0 {
        return Err(IpcError::InvalidArgument);
    }

    // Check limits
    if SEM_BY_ID.read().len() >= SEMMNI {
        return Err(IpcError::ResourceLimit);
    }

    // Create set
    let (uid, gid) = current_credentials();
    let set = SemSet::new(key, nsems, uid, gid, mode);

    // Allocate ID
    let semid = SEM_NEXT_ID.fetch_add(1, Ordering::Relaxed);

    // Store set
    if key != IPC_PRIVATE {
        SEM_BY_KEY.write().insert(key, semid);
    }
    SEM_BY_ID.write().insert(semid, set);

    Ok(semid)
}

/// semop - semaphore operations
pub fn semop(semid: IpcId, sops: &[SemBuf]) -> Result<(), IpcError> {
    if sops.is_empty() || sops.len() > SEMOPM {
        return Err(IpcError::InvalidArgument);
    }

    let nowait = sops.iter().any(|op| op.sem_flg & IPC_NOWAIT as u16 != 0);

    loop {
        let mut sets = SEM_BY_ID.write();
        let set = sets.get_mut(&semid).ok_or(IpcError::InvalidId)?;

        // Check permissions
        let (uid, gid) = current_credentials();
        let need_write = sops.iter().any(|op| op.sem_op != 0);
        if need_write {
            if !set.perm.can_write(uid, gid) {
                return Err(IpcError::PermissionDenied);
            }
        } else {
            if !set.perm.can_read(uid, gid) {
                return Err(IpcError::PermissionDenied);
            }
        }

        // Validate semaphore numbers
        for op in sops {
            if op.sem_num >= set.nsems as u16 {
                return Err(IpcError::InvalidArgument);
            }
        }

        // Try to perform all operations atomically
        let mut can_complete = true;
        let mut new_values: Vec<(u16, u16)> = Vec::new();

        for op in sops {
            let sem = &set.sems[op.sem_num as usize];
            let current = sem.value() as i32;

            if op.sem_op > 0 {
                // Increase - always succeeds
                let new_val = current + op.sem_op as i32;
                if new_val > SEMVMX as i32 {
                    return Err(IpcError::InvalidArgument);
                }
                new_values.push((op.sem_num, new_val as u16));
            } else if op.sem_op < 0 {
                // Decrease - may block
                let new_val = current + op.sem_op as i32;
                if new_val < 0 {
                    can_complete = false;
                    break;
                }
                new_values.push((op.sem_num, new_val as u16));
            } else {
                // Wait for zero
                if current != 0 {
                    can_complete = false;
                    break;
                }
            }
        }

        if can_complete {
            // Apply all operations
            let pid = current_pid();

            for (semnum, new_val) in new_values {
                let sem = &mut set.sems[semnum as usize];
                sem.set_value(new_val);
                sem.sempid = pid;
            }

            set.sem_otime = current_time();

            // Handle SEM_UNDO
            for op in sops {
                if op.sem_flg & SEM_UNDO != 0 && op.sem_op != 0 {
                    let mut undos = SEM_UNDO_MAP.write();
                    let undo = undos.entry(pid).or_insert_with(SemUndo::default);
                    undo.add(semid, op.sem_num, op.sem_op);
                }
            }

            return Ok(());
        }

        // Would block
        if nowait {
            return Err(IpcError::WouldBlock);
        }

        // Add to pending list and wait
        drop(sets);
        // Wait for semaphore change...
    }
}

/// semctl - semaphore control
pub fn semctl(semid: IpcId, semnum: u32, cmd: u32, arg: Option<SemArg>) -> Result<i32, IpcError> {
    match cmd {
        GETVAL => {
            let sets = SEM_BY_ID.read();
            let set = sets.get(&semid).ok_or(IpcError::InvalidId)?;

            let sem = set.get(semnum).ok_or(IpcError::InvalidArgument)?;
            Ok(sem.value() as i32)
        }
        SETVAL => {
            let mut sets = SEM_BY_ID.write();
            let set = sets.get_mut(&semid).ok_or(IpcError::InvalidId)?;

            // Permission check
            let (uid, _gid) = current_credentials();
            if uid != set.perm.uid && uid != 0 {
                return Err(IpcError::PermissionDenied);
            }

            let val = match arg {
                Some(SemArg::Val(v)) if v >= 0 && v <= SEMVMX as i32 => v as u16,
                _ => return Err(IpcError::InvalidArgument),
            };

            let sem = set.get_mut(semnum).ok_or(IpcError::InvalidArgument)?;
            sem.set_value(val);
            sem.sempid = current_pid();
            set.sem_ctime = current_time();

            Ok(0)
        }
        GETPID => {
            let sets = SEM_BY_ID.read();
            let set = sets.get(&semid).ok_or(IpcError::InvalidId)?;

            let sem = set.get(semnum).ok_or(IpcError::InvalidArgument)?;
            Ok(sem.sempid.as_i32())
        }
        GETNCNT => {
            let sets = SEM_BY_ID.read();
            let set = sets.get(&semid).ok_or(IpcError::InvalidId)?;

            let sem = set.get(semnum).ok_or(IpcError::InvalidArgument)?;
            Ok(sem.semncnt.load(Ordering::Relaxed) as i32)
        }
        GETZCNT => {
            let sets = SEM_BY_ID.read();
            let set = sets.get(&semid).ok_or(IpcError::InvalidId)?;

            let sem = set.get(semnum).ok_or(IpcError::InvalidArgument)?;
            Ok(sem.semzcnt.load(Ordering::Relaxed) as i32)
        }
        GETALL => {
            let sets = SEM_BY_ID.read();
            let _set = sets.get(&semid).ok_or(IpcError::InvalidId)?;

            // Would copy values to arg array
            Ok(0)
        }
        SETALL => {
            let mut sets = SEM_BY_ID.write();
            let set = sets.get_mut(&semid).ok_or(IpcError::InvalidId)?;

            // Permission check
            let (uid, _gid) = current_credentials();
            if uid != set.perm.uid && uid != 0 {
                return Err(IpcError::PermissionDenied);
            }

            if let Some(SemArg::Array(vals)) = arg {
                if vals.len() != set.nsems as usize {
                    return Err(IpcError::InvalidArgument);
                }
                let pid = current_pid();
                for (i, &val) in vals.iter().enumerate() {
                    set.sems[i].set_value(val.min(SEMVMX));
                    set.sems[i].sempid = pid;
                }
                set.sem_ctime = current_time();
            }

            Ok(0)
        }
        IPC_STAT | SEM_STAT | SEM_STAT_ANY => {
            let sets = SEM_BY_ID.read();
            let _set = sets.get(&semid).ok_or(IpcError::InvalidId)?;

            // Would fill in SemidDs
            Ok(0)
        }
        IPC_SET => {
            let mut sets = SEM_BY_ID.write();
            let set = sets.get_mut(&semid).ok_or(IpcError::InvalidId)?;

            let (uid, _gid) = current_credentials();
            if uid != set.perm.uid && uid != 0 {
                return Err(IpcError::PermissionDenied);
            }

            if let Some(SemArg::Buf(buf)) = arg {
                set.perm.uid = buf.perm.uid;
                set.perm.gid = buf.perm.gid;
                set.perm.mode = buf.perm.mode & 0o777;
                set.sem_ctime = current_time();
            }

            Ok(0)
        }
        IPC_RMID => {
            let mut sets = SEM_BY_ID.write();
            let set = sets.get(&semid).ok_or(IpcError::InvalidId)?;

            let (uid, _gid) = current_credentials();
            if uid != set.perm.uid && uid != 0 {
                return Err(IpcError::PermissionDenied);
            }

            let key = set.perm.key;
            sets.remove(&semid);
            if key != IPC_PRIVATE {
                SEM_BY_KEY.write().remove(&key);
            }

            // Clear undo entries for this semid
            let mut undos = SEM_UNDO_MAP.write();
            for undo in undos.values_mut() {
                undo.entries.retain(|&(id, _), _| id != semid);
            }

            Ok(0)
        }
        IPC_INFO | SEM_INFO => {
            Ok(SEM_BY_ID.read().len() as i32)
        }
        _ => Err(IpcError::InvalidArgument),
    }
}

// =============================================================================
// INFO AND LISTING
// =============================================================================

/// Get semaphore info
pub fn get_info(_ns: Option<&IpcNamespace>) -> SemInfo {
    let sets = SEM_BY_ID.read();
    let total_sems: usize = sets.values().map(|s| s.nsems as usize).sum();

    SemInfo {
        semmni: SEMMNI,
        semmsl: SEMMSL,
        semmns: SEMMNI * SEMMSL,
        semopm: SEMOPM,
        semvmx: SEMVMX as usize,
        semmnu: SEMMNU,
        semusz: sets.len(),
        semaem: total_sems,
    }
}

/// Generate /proc/sysvipc/sem listing
pub fn proc_list() -> String {
    let sets = SEM_BY_ID.read();

    let mut output = String::from(
        "       key      semid perms      nsems   uid   gid  cuid  cgid      otime      ctime\n"
    );

    for (&semid, set) in sets.iter() {
        output.push_str(&alloc::format!(
            "{:>10} {:>10} {:>5o} {:>10} {:>5} {:>5} {:>5} {:>5} {:>10} {:>10}\n",
            set.perm.key, semid, set.perm.mode, set.nsems,
            set.perm.uid, set.perm.gid, set.perm.cuid, set.perm.cgid,
            set.sem_otime, set.sem_ctime
        ));
    }

    output
}

// =============================================================================
// PROCESS EXIT CLEANUP
// =============================================================================

/// Clean up semaphores for exiting process
pub fn process_exit(pid: Pid) {
    // Apply undo operations
    let undos = SEM_UNDO_MAP.write().remove(&pid);

    if let Some(undo) = undos {
        let mut sets = SEM_BY_ID.write();

        for ((semid, semnum), adj) in undo.entries {
            if let Some(set) = sets.get_mut(&semid) {
                if let Some(sem) = set.get(semnum as u32) {
                    let current = sem.value() as i32;
                    let new_val = (current + adj as i32).max(0).min(SEMVMX as i32);
                    sem.set_value(new_val as u16);
                }
            }
        }
    }
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
