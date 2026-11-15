// ===============================================================================
// BUILDOS KERNEL - USER NAMESPACE
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! User Namespace Implementation
//!
//! Provides user and group ID isolation. Each user namespace has its own
//! set of UIDs and GIDs that map to different IDs in the parent namespace.
//! Also provides capability isolation.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::sync::RwLock;
use super::{Namespace, NsType, NsError, next_ns_id, MAX_USER_NS_LEVEL};

/// User namespace structure
pub struct UserNamespace {
    /// Namespace ID
    id: u64,
    /// Nesting level
    level: u32,
    /// Parent namespace
    parent: Option<Arc<UserNamespace>>,
    /// UID mapping
    uid_map: RwLock<Vec<IdMapping>>,
    /// GID mapping
    gid_map: RwLock<Vec<IdMapping>>,
    /// Owner UID (in parent namespace)
    owner: AtomicU32,
    /// Owner GID (in parent namespace)
    group: AtomicU32,
    /// Capabilities in this namespace
    capabilities: RwLock<CapabilitySet>,
    /// Is mapping set?
    uid_map_set: core::sync::atomic::AtomicBool,
    gid_map_set: core::sync::atomic::AtomicBool,
}

impl UserNamespace {
    /// Create initial (root) user namespace
    pub fn new_initial() -> Self {
        let mut caps = CapabilitySet::new();
        caps.set_all(); // Root namespace has all capabilities

        Self {
            id: next_ns_id(),
            level: 0,
            parent: None,
            uid_map: RwLock::new(vec![IdMapping::identity()]),
            gid_map: RwLock::new(vec![IdMapping::identity()]),
            owner: AtomicU32::new(0),
            group: AtomicU32::new(0),
            capabilities: RwLock::new(caps),
            uid_map_set: core::sync::atomic::AtomicBool::new(true),
            gid_map_set: core::sync::atomic::AtomicBool::new(true),
        }
    }

    /// Create child user namespace
    pub fn new_child(parent: Arc<UserNamespace>) -> Self {
        let level = parent.level + 1;

        // Child starts with no capabilities until ID map is set
        let caps = CapabilitySet::new();

        Self {
            id: next_ns_id(),
            level,
            parent: Some(parent),
            uid_map: RwLock::new(Vec::new()),
            gid_map: RwLock::new(Vec::new()),
            owner: AtomicU32::new(0),
            group: AtomicU32::new(0),
            capabilities: RwLock::new(caps),
            uid_map_set: core::sync::atomic::AtomicBool::new(false),
            gid_map_set: core::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Get nesting level
    pub fn level(&self) -> u32 {
        self.level
    }

    /// Get parent namespace
    pub fn parent(&self) -> Option<&Arc<UserNamespace>> {
        self.parent.as_ref()
    }

    /// Set owner (creator) of this namespace
    pub fn set_owner(&self, uid: u32, gid: u32) {
        self.owner.store(uid, Ordering::Release);
        self.group.store(gid, Ordering::Release);
    }

    /// Get owner UID
    pub fn owner(&self) -> u32 {
        self.owner.load(Ordering::Relaxed)
    }

    /// Get owner GID
    pub fn group(&self) -> u32 {
        self.group.load(Ordering::Relaxed)
    }

    /// Set UID mapping
    pub fn set_uid_map(&self, mappings: Vec<IdMapping>) -> Result<(), NsError> {
        // Can only set once
        if self.uid_map_set.load(Ordering::Relaxed) {
            return Err(NsError::PermissionDenied);
        }

        // Validate mappings
        for mapping in &mappings {
            if !self.validate_mapping(mapping, true) {
                return Err(NsError::PermissionDenied);
            }
        }

        *self.uid_map.write() = mappings;
        self.uid_map_set.store(true, Ordering::Release);

        // Grant capabilities now that mapping is set
        if self.gid_map_set.load(Ordering::Relaxed) {
            self.grant_initial_caps();
        }

        Ok(())
    }

    /// Set GID mapping
    pub fn set_gid_map(&self, mappings: Vec<IdMapping>) -> Result<(), NsError> {
        // Can only set once
        if self.gid_map_set.load(Ordering::Relaxed) {
            return Err(NsError::PermissionDenied);
        }

        // Validate mappings
        for mapping in &mappings {
            if !self.validate_mapping(mapping, false) {
                return Err(NsError::PermissionDenied);
            }
        }

        *self.gid_map.write() = mappings;
        self.gid_map_set.store(true, Ordering::Release);

        // Grant capabilities now that mapping is set
        if self.uid_map_set.load(Ordering::Relaxed) {
            self.grant_initial_caps();
        }

        Ok(())
    }

    /// Validate an ID mapping
    fn validate_mapping(&self, mapping: &IdMapping, _is_uid: bool) -> bool {
        // Check for overflow
        if mapping.ns_id.checked_add(mapping.count).is_none() {
            return false;
        }
        if mapping.host_id.checked_add(mapping.count).is_none() {
            return false;
        }

        // In a full implementation, would check:
        // - Caller has CAP_SETUID/CAP_SETGID in parent ns
        // - Or mapping only maps caller's own ID
        // - Parent IDs are valid in parent's mapping

        true
    }

    /// Grant initial capabilities to namespace creator
    fn grant_initial_caps(&self) {
        let mut caps = self.capabilities.write();
        caps.set_all();
    }

    /// Map UID from this namespace to parent
    pub fn uid_to_parent(&self, ns_uid: u32) -> Option<u32> {
        for mapping in self.uid_map.read().iter() {
            if let Some(host_id) = mapping.to_host(ns_uid) {
                return Some(host_id);
            }
        }
        None
    }

    /// Map UID from parent to this namespace
    pub fn uid_from_parent(&self, host_uid: u32) -> Option<u32> {
        for mapping in self.uid_map.read().iter() {
            if let Some(ns_id) = mapping.to_ns(host_uid) {
                return Some(ns_id);
            }
        }
        None
    }

    /// Map GID from this namespace to parent
    pub fn gid_to_parent(&self, ns_gid: u32) -> Option<u32> {
        for mapping in self.gid_map.read().iter() {
            if let Some(host_id) = mapping.to_host(ns_gid) {
                return Some(host_id);
            }
        }
        None
    }

    /// Map GID from parent to this namespace
    pub fn gid_from_parent(&self, host_gid: u32) -> Option<u32> {
        for mapping in self.gid_map.read().iter() {
            if let Some(ns_id) = mapping.to_ns(host_gid) {
                return Some(ns_id);
            }
        }
        None
    }

    /// Map UID to initial user namespace
    pub fn uid_to_init(&self, ns_uid: u32) -> Option<u32> {
        let host_uid = self.uid_to_parent(ns_uid)?;

        if let Some(parent) = &self.parent {
            parent.uid_to_init(host_uid)
        } else {
            Some(host_uid)
        }
    }

    /// Map GID to initial user namespace
    pub fn gid_to_init(&self, ns_gid: u32) -> Option<u32> {
        let host_gid = self.gid_to_parent(ns_gid)?;

        if let Some(parent) = &self.parent {
            parent.gid_to_init(host_gid)
        } else {
            Some(host_gid)
        }
    }

    /// Check if process has capability in this namespace
    pub fn has_capability(&self, cap: Capability) -> bool {
        self.capabilities.read().has(cap)
    }

    /// Check if one namespace is ancestor of another
    pub fn is_ancestor_of(&self, other: &UserNamespace) -> bool {
        if self.id == other.id {
            return true;
        }

        if let Some(parent) = &other.parent {
            return self.is_ancestor_of(parent);
        }

        false
    }

    /// Check if we have privilege over target namespace
    pub fn has_privilege_over(&self, target: &UserNamespace) -> bool {
        // We have privilege if target is in our namespace or a descendant
        self.is_ancestor_of(target)
    }

    /// Get UID map for /proc/[pid]/uid_map
    pub fn get_uid_map(&self) -> Vec<IdMapping> {
        self.uid_map.read().clone()
    }

    /// Get GID map for /proc/[pid]/gid_map
    pub fn get_gid_map(&self) -> Vec<IdMapping> {
        self.gid_map.read().clone()
    }
}

impl Namespace for UserNamespace {
    fn ns_type(&self) -> NsType {
        NsType::User
    }

    fn id(&self) -> u64 {
        self.id
    }

    fn user_ns(&self) -> Option<Arc<UserNamespace>> {
        // User namespace owns itself
        None
    }

    fn clone_ns(&self) -> Arc<dyn Namespace> {
        Arc::new(Self::new_child(Arc::new(Self {
            id: self.id,
            level: self.level,
            parent: self.parent.clone(),
            uid_map: RwLock::new(self.uid_map.read().clone()),
            gid_map: RwLock::new(self.gid_map.read().clone()),
            owner: AtomicU32::new(self.owner.load(Ordering::Relaxed)),
            group: AtomicU32::new(self.group.load(Ordering::Relaxed)),
            capabilities: RwLock::new(self.capabilities.read().clone()),
            uid_map_set: core::sync::atomic::AtomicBool::new(
                self.uid_map_set.load(Ordering::Relaxed)
            ),
            gid_map_set: core::sync::atomic::AtomicBool::new(
                self.gid_map_set.load(Ordering::Relaxed)
            ),
        })))
    }
}

/// ID mapping entry
#[derive(Clone)]
pub struct IdMapping {
    /// ID in this namespace
    pub ns_id: u32,
    /// ID in parent (host) namespace
    pub host_id: u32,
    /// Number of IDs in range
    pub count: u32,
}

impl IdMapping {
    /// Create identity mapping (all IDs map to themselves)
    pub fn identity() -> Self {
        Self {
            ns_id: 0,
            host_id: 0,
            count: u32::MAX,
        }
    }

    /// Map namespace ID to host ID
    pub fn to_host(&self, ns_id: u32) -> Option<u32> {
        if ns_id >= self.ns_id && ns_id < self.ns_id + self.count {
            Some(self.host_id + (ns_id - self.ns_id))
        } else {
            None
        }
    }

    /// Map host ID to namespace ID
    pub fn to_ns(&self, host_id: u32) -> Option<u32> {
        if host_id >= self.host_id && host_id < self.host_id + self.count {
            Some(self.ns_id + (host_id - self.host_id))
        } else {
            None
        }
    }

    /// Format for /proc display
    pub fn format(&self) -> alloc::string::String {
        alloc::format!("{} {} {}", self.ns_id, self.host_id, self.count)
    }

    /// Parse from string
    pub fn parse(s: &str) -> Result<Self, NsError> {
        let parts: Vec<&str> = s.split_whitespace().collect();
        if parts.len() != 3 {
            return Err(NsError::InvalidOperation);
        }

        Ok(Self {
            ns_id: parts[0].parse().map_err(|_| NsError::InvalidOperation)?,
            host_id: parts[1].parse().map_err(|_| NsError::InvalidOperation)?,
            count: parts[2].parse().map_err(|_| NsError::InvalidOperation)?,
        })
    }
}

/// Linux capabilities
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Capability {
    Chown = 0,
    DacOverride = 1,
    DacReadSearch = 2,
    Fowner = 3,
    Fsetid = 4,
    Kill = 5,
    Setgid = 6,
    Setuid = 7,
    Setpcap = 8,
    LinuxImmutable = 9,
    NetBindService = 10,
    NetBroadcast = 11,
    NetAdmin = 12,
    NetRaw = 13,
    IpcLock = 14,
    IpcOwner = 15,
    SysModule = 16,
    SysRawio = 17,
    SysChroot = 18,
    SysPtrace = 19,
    SysPacct = 20,
    SysAdmin = 21,
    SysBoot = 22,
    SysNice = 23,
    SysResource = 24,
    SysTime = 25,
    SysTtyConfig = 26,
    Mknod = 27,
    Lease = 28,
    AuditWrite = 29,
    AuditControl = 30,
    Setfcap = 31,
    MacOverride = 32,
    MacAdmin = 33,
    Syslog = 34,
    WakeAlarm = 35,
    BlockSuspend = 36,
    AuditRead = 37,
    Perfmon = 38,
    Bpf = 39,
    CheckpointRestore = 40,
}

impl Capability {
    pub const MAX: u32 = 41;
}

/// Capability set (bitmask)
#[derive(Clone)]
pub struct CapabilitySet {
    bits: [u64; 1], // Up to 64 capabilities
}

impl CapabilitySet {
    pub fn new() -> Self {
        Self { bits: [0] }
    }

    pub fn set(&mut self, cap: Capability) {
        let bit = cap as u32;
        if bit < 64 {
            self.bits[0] |= 1 << bit;
        }
    }

    pub fn clear(&mut self, cap: Capability) {
        let bit = cap as u32;
        if bit < 64 {
            self.bits[0] &= !(1 << bit);
        }
    }

    pub fn has(&self, cap: Capability) -> bool {
        let bit = cap as u32;
        if bit < 64 {
            (self.bits[0] & (1 << bit)) != 0
        } else {
            false
        }
    }

    pub fn set_all(&mut self) {
        self.bits[0] = !0;
    }

    pub fn clear_all(&mut self) {
        self.bits[0] = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.bits[0] == 0
    }
}

impl Default for CapabilitySet {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if can create user namespace
pub fn can_create_user_ns(parent: &UserNamespace) -> bool {
    parent.level < MAX_USER_NS_LEVEL
}

/// Get effective UID/GID for a process in a namespace
pub fn get_effective_ids(ns: &UserNamespace, uid: u32, gid: u32) -> (u32, u32) {
    let eff_uid = ns.uid_to_init(uid).unwrap_or(65534);  // nobody
    let eff_gid = ns.gid_to_init(gid).unwrap_or(65534);  // nogroup
    (eff_uid, eff_gid)
}
