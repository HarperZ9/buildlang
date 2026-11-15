// ===============================================================================
// BUILDOS KERNEL - PID NAMESPACE
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! PID Namespace Implementation
//!
//! Provides process ID isolation. Each PID namespace has its own
//! PID number space, where PIDs start from 1.

use alloc::sync::Arc;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::sync::RwLock;
use super::{Namespace, NsType, NsError, next_ns_id, MAX_PID_NS_LEVEL};
use super::user::UserNamespace;

/// PID namespace structure
pub struct PidNamespace {
    /// Namespace ID
    id: u64,
    /// Nesting level
    level: u32,
    /// Parent namespace
    parent: Option<Arc<PidNamespace>>,
    /// Owning user namespace
    user_ns: Arc<UserNamespace>,
    /// PID allocator
    pid_allocator: RwLock<PidAllocator>,
    /// PID to process mapping (virtual PID -> global PID)
    pid_map: RwLock<BTreeMap<u32, u32>>,
    /// Reverse mapping (global PID -> virtual PID)
    reverse_map: RwLock<BTreeMap<u32, u32>>,
    /// Init process PID (PID 1 in this namespace)
    init_pid: AtomicU32,
    /// Child reaper for orphaned processes
    child_reaper: AtomicU32,
    /// Is this namespace active?
    active: core::sync::atomic::AtomicBool,
}

impl PidNamespace {
    /// Create initial (root) PID namespace
    pub fn new_initial(user_ns: Arc<UserNamespace>) -> Self {
        Self {
            id: next_ns_id(),
            level: 0,
            parent: None,
            user_ns,
            pid_allocator: RwLock::new(PidAllocator::new()),
            pid_map: RwLock::new(BTreeMap::new()),
            reverse_map: RwLock::new(BTreeMap::new()),
            init_pid: AtomicU32::new(1),
            child_reaper: AtomicU32::new(1),
            active: core::sync::atomic::AtomicBool::new(true),
        }
    }

    /// Create child PID namespace
    pub fn new_child(parent: Arc<PidNamespace>, user_ns: Arc<UserNamespace>) -> Self {
        let level = parent.level + 1;

        Self {
            id: next_ns_id(),
            level,
            parent: Some(parent),
            user_ns,
            pid_allocator: RwLock::new(PidAllocator::new()),
            pid_map: RwLock::new(BTreeMap::new()),
            reverse_map: RwLock::new(BTreeMap::new()),
            init_pid: AtomicU32::new(0),
            child_reaper: AtomicU32::new(0),
            active: core::sync::atomic::AtomicBool::new(true),
        }
    }

    /// Get nesting level
    pub fn level(&self) -> u32 {
        self.level
    }

    /// Get parent namespace
    pub fn parent(&self) -> Option<&Arc<PidNamespace>> {
        self.parent.as_ref()
    }

    /// Allocate a new PID in this namespace
    pub fn alloc_pid(&self, global_pid: u32) -> Result<u32, NsError> {
        let virtual_pid = self.pid_allocator.write().alloc()?;

        self.pid_map.write().insert(virtual_pid, global_pid);
        self.reverse_map.write().insert(global_pid, virtual_pid);

        // First process becomes init
        if virtual_pid == 1 {
            self.init_pid.store(global_pid, Ordering::Release);
            self.child_reaper.store(global_pid, Ordering::Release);
        }

        Ok(virtual_pid)
    }

    /// Free a PID
    pub fn free_pid(&self, virtual_pid: u32) {
        if let Some(global_pid) = self.pid_map.write().remove(&virtual_pid) {
            self.reverse_map.write().remove(&global_pid);
        }
        self.pid_allocator.write().free(virtual_pid);
    }

    /// Translate virtual PID to global PID
    pub fn to_global(&self, virtual_pid: u32) -> Option<u32> {
        self.pid_map.read().get(&virtual_pid).copied()
    }

    /// Translate global PID to virtual PID
    pub fn to_virtual(&self, global_pid: u32) -> Option<u32> {
        self.reverse_map.read().get(&global_pid).copied()
    }

    /// Get PID as seen from ancestor namespace
    pub fn pid_nr_ns(&self, global_pid: u32, target_ns: &PidNamespace) -> Option<u32> {
        if self.id == target_ns.id {
            return self.to_virtual(global_pid);
        }

        // Walk up to find common ancestor
        if let Some(parent) = &self.parent {
            return parent.pid_nr_ns(global_pid, target_ns);
        }

        None
    }

    /// Get init process
    pub fn get_init(&self) -> u32 {
        self.init_pid.load(Ordering::Relaxed)
    }

    /// Get child reaper
    pub fn get_child_reaper(&self) -> u32 {
        self.child_reaper.load(Ordering::Relaxed)
    }

    /// Set child reaper (e.g., when init exits)
    pub fn set_child_reaper(&self, pid: u32) {
        self.child_reaper.store(pid, Ordering::Release);
    }

    /// Check if namespace is active
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    /// Deactivate namespace (when init dies)
    pub fn deactivate(&self) {
        self.active.store(false, Ordering::Release);
    }

    /// Get all PIDs in this namespace
    pub fn all_pids(&self) -> Vec<u32> {
        self.pid_map.read().keys().copied().collect()
    }

    /// Check if this namespace is ancestor of another
    pub fn is_ancestor_of(&self, other: &PidNamespace) -> bool {
        if self.id == other.id {
            return true;
        }

        if let Some(parent) = &other.parent {
            return self.is_ancestor_of(parent);
        }

        false
    }
}

impl Namespace for PidNamespace {
    fn ns_type(&self) -> NsType {
        NsType::Pid
    }

    fn id(&self) -> u64 {
        self.id
    }

    fn user_ns(&self) -> Option<Arc<UserNamespace>> {
        Some(self.user_ns.clone())
    }

    fn clone_ns(&self) -> Arc<dyn Namespace> {
        Arc::new(Self::new_child(
            Arc::new(Self {
                id: self.id,
                level: self.level,
                parent: self.parent.clone(),
                user_ns: self.user_ns.clone(),
                pid_allocator: RwLock::new(PidAllocator::new()),
                pid_map: RwLock::new(BTreeMap::new()),
                reverse_map: RwLock::new(BTreeMap::new()),
                init_pid: AtomicU32::new(0),
                child_reaper: AtomicU32::new(0),
                active: core::sync::atomic::AtomicBool::new(true),
            }),
            self.user_ns.clone(),
        ))
    }
}

/// PID allocator
struct PidAllocator {
    /// Next PID to try
    next: u32,
    /// Free PIDs (recycled)
    free_list: Vec<u32>,
    /// Maximum PID value
    max_pid: u32,
}

impl PidAllocator {
    const DEFAULT_MAX_PID: u32 = 32768;

    fn new() -> Self {
        Self {
            next: 1,
            free_list: Vec::new(),
            max_pid: Self::DEFAULT_MAX_PID,
        }
    }

    fn alloc(&mut self) -> Result<u32, NsError> {
        // Try free list first
        if let Some(pid) = self.free_list.pop() {
            return Ok(pid);
        }

        // Allocate new PID
        if self.next >= self.max_pid {
            return Err(NsError::ResourceLimit);
        }

        let pid = self.next;
        self.next += 1;
        Ok(pid)
    }

    fn free(&mut self, pid: u32) {
        // Don't recycle PID 1
        if pid > 1 {
            self.free_list.push(pid);
        }
    }
}

/// PID namespace operations

/// Check if process can see another process
pub fn pid_visible(viewer_ns: &PidNamespace, target_global_pid: u32) -> bool {
    viewer_ns.to_virtual(target_global_pid).is_some()
}

/// Get task's PID as seen from given namespace
pub fn task_pid_nr_ns(global_pid: u32, ns: &PidNamespace) -> u32 {
    ns.to_virtual(global_pid).unwrap_or(0)
}

/// Get PID for /proc filesystem
pub fn pid_vnr(ns: &PidNamespace, global_pid: u32) -> u32 {
    ns.to_virtual(global_pid).unwrap_or(0)
}

/// Zap all PIDs when namespace dies
pub fn zap_pid_ns(ns: &PidNamespace) {
    ns.deactivate();

    // Would signal all processes to exit
    for &virtual_pid in ns.pid_map.read().keys() {
        crate::kprintln!("[NS] Zapping PID {} in namespace {}", virtual_pid, ns.id);
    }
}

/// Check if can create child PID namespace
pub fn can_create_pid_ns(parent: &PidNamespace) -> bool {
    parent.level < MAX_PID_NS_LEVEL
}
