// ===============================================================================
// BUILDOS KERNEL - MOUNT NAMESPACE
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Mount Namespace Implementation
//!
//! Provides mount point isolation. Each mount namespace has its own
//! mount tree, allowing different views of the filesystem hierarchy.

use alloc::sync::Arc;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::sync::RwLock;
use super::{Namespace, NsType, NsError, next_ns_id};
use super::user::UserNamespace;

/// Mount namespace structure
pub struct MntNamespace {
    /// Namespace ID
    id: u64,
    /// Owning user namespace
    user_ns: Arc<UserNamespace>,
    /// Parent namespace
    parent: Option<Arc<MntNamespace>>,
    /// Root mount
    root: RwLock<Option<Arc<Mount>>>,
    /// All mounts by ID
    mounts: RwLock<BTreeMap<u64, Arc<Mount>>>,
    /// Mount ID counter
    mount_id_counter: AtomicU64,
    /// Mount count (for limits)
    mount_count: AtomicU64,
}

impl MntNamespace {
    /// Maximum mounts per namespace
    pub const MAX_MOUNTS: u64 = 100000;

    /// Create initial (root) mount namespace
    pub fn new_initial(user_ns: Arc<UserNamespace>) -> Self {
        Self {
            id: next_ns_id(),
            user_ns,
            parent: None,
            root: RwLock::new(None),
            mounts: RwLock::new(BTreeMap::new()),
            mount_id_counter: AtomicU64::new(1),
            mount_count: AtomicU64::new(0),
        }
    }

    /// Create child mount namespace (copies mount tree)
    pub fn new_child(parent: Arc<MntNamespace>, user_ns: Arc<UserNamespace>) -> Self {
        let new_ns = Self {
            id: next_ns_id(),
            user_ns,
            parent: Some(parent.clone()),
            root: RwLock::new(None),
            mounts: RwLock::new(BTreeMap::new()),
            mount_id_counter: AtomicU64::new(1),
            mount_count: AtomicU64::new(0),
        };

        // Copy mount tree from parent
        new_ns.copy_mount_tree(&parent);

        new_ns
    }

    /// Copy mount tree from parent
    fn copy_mount_tree(&self, parent: &MntNamespace) {
        let parent_mounts = parent.mounts.read();
        let mut new_mounts = self.mounts.write();

        for (_, mount) in parent_mounts.iter() {
            let new_mount = Arc::new(Mount {
                id: self.alloc_mount_id(),
                parent_id: mount.parent_id,
                device: mount.device.clone(),
                mount_point: mount.mount_point.clone(),
                fs_type: mount.fs_type.clone(),
                options: mount.options.clone(),
                flags: mount.flags,
                propagation: mount.propagation,
                children: RwLock::new(Vec::new()),
            });

            new_mounts.insert(new_mount.id, new_mount.clone());

            if mount.mount_point == "/" {
                *self.root.write() = Some(new_mount);
            }
        }

        self.mount_count.store(new_mounts.len() as u64, Ordering::Release);
    }

    /// Allocate mount ID
    fn alloc_mount_id(&self) -> u64 {
        self.mount_id_counter.fetch_add(1, Ordering::Relaxed)
    }

    /// Mount a filesystem
    pub fn mount(
        &self,
        device: &str,
        mount_point: &str,
        fs_type: &str,
        flags: MountFlags,
        options: &str,
    ) -> Result<u64, NsError> {
        // Check mount limit
        let count = self.mount_count.load(Ordering::Relaxed);
        if count >= Self::MAX_MOUNTS {
            return Err(NsError::ResourceLimit);
        }

        // Find parent mount
        let parent_id = self.find_mount_at(mount_point)
            .map(|m| m.id)
            .unwrap_or(0);

        let mount_id = self.alloc_mount_id();
        let mount = Arc::new(Mount {
            id: mount_id,
            parent_id,
            device: device.into(),
            mount_point: mount_point.into(),
            fs_type: fs_type.into(),
            options: options.into(),
            flags,
            propagation: PropagationType::Private,
            children: RwLock::new(Vec::new()),
        });

        // Add to parent's children
        if let Some(parent) = self.mounts.read().get(&parent_id) {
            parent.children.write().push(mount_id);
        }

        self.mounts.write().insert(mount_id, mount.clone());
        self.mount_count.fetch_add(1, Ordering::Relaxed);

        // Set as root if mounting at /
        if mount_point == "/" {
            *self.root.write() = Some(mount);
        }

        crate::kprintln!("[NS] Mounted {} at {} (type: {}, id: {})",
            device, mount_point, fs_type, mount_id);

        Ok(mount_id)
    }

    /// Unmount a filesystem
    pub fn umount(&self, mount_point: &str, flags: UmountFlags) -> Result<(), NsError> {
        let mount = self.find_mount_at(mount_point)
            .ok_or(NsError::NotFound)?;

        // Check if has children
        if !mount.children.read().is_empty() && !flags.contains(UmountFlags::DETACH) {
            return Err(NsError::InvalidOperation);
        }

        // Remove from parent
        if let Some(parent) = self.mounts.read().get(&mount.parent_id) {
            parent.children.write().retain(|&id| id != mount.id);
        }

        self.mounts.write().remove(&mount.id);
        self.mount_count.fetch_sub(1, Ordering::Relaxed);

        crate::kprintln!("[NS] Unmounted {}", mount_point);

        Ok(())
    }

    /// Find mount at path
    pub fn find_mount_at(&self, path: &str) -> Option<Arc<Mount>> {
        self.mounts.read()
            .values()
            .find(|m| m.mount_point == path)
            .cloned()
    }

    /// Find mount containing path
    pub fn find_mount_for(&self, path: &str) -> Option<Arc<Mount>> {
        let mounts = self.mounts.read();
        let mut best: Option<&Arc<Mount>> = None;
        let mut best_len = 0;

        for mount in mounts.values() {
            if path.starts_with(&mount.mount_point) {
                let len = mount.mount_point.len();
                if len > best_len {
                    best = Some(mount);
                    best_len = len;
                }
            }
        }

        best.cloned()
    }

    /// Get all mounts
    pub fn list_mounts(&self) -> Vec<MountInfo> {
        self.mounts.read()
            .values()
            .map(|m| MountInfo {
                mount_id: m.id,
                parent_id: m.parent_id,
                device: m.device.clone(),
                mount_point: m.mount_point.clone(),
                fs_type: m.fs_type.clone(),
                options: m.options.clone(),
            })
            .collect()
    }

    /// Pivot root
    pub fn pivot_root(&self, new_root: &str, put_old: &str) -> Result<(), NsError> {
        // Validate paths
        let new_root_mount = self.find_mount_at(new_root)
            .ok_or(NsError::NotFound)?;

        crate::kprintln!("[NS] Pivoting root: {} -> {}", new_root, put_old);

        // Would move old root under new root at put_old
        *self.root.write() = Some(new_root_mount);

        Ok(())
    }

    /// Make mount shared
    pub fn make_shared(&self, mount_point: &str) -> Result<(), NsError> {
        self.set_propagation(mount_point, PropagationType::Shared)
    }

    /// Make mount private
    pub fn make_private(&self, mount_point: &str) -> Result<(), NsError> {
        self.set_propagation(mount_point, PropagationType::Private)
    }

    /// Make mount slave
    pub fn make_slave(&self, mount_point: &str) -> Result<(), NsError> {
        self.set_propagation(mount_point, PropagationType::Slave)
    }

    /// Make mount unbindable
    pub fn make_unbindable(&self, mount_point: &str) -> Result<(), NsError> {
        self.set_propagation(mount_point, PropagationType::Unbindable)
    }

    /// Set mount propagation type
    fn set_propagation(&self, mount_point: &str, prop: PropagationType) -> Result<(), NsError> {
        let _mount = self.find_mount_at(mount_point)
            .ok_or(NsError::NotFound)?;

        // Would update mount propagation
        crate::kprintln!("[NS] Set {} propagation to {:?}", mount_point, prop);

        Ok(())
    }

    /// Bind mount
    pub fn bind_mount(
        &self,
        source: &str,
        target: &str,
        recursive: bool,
    ) -> Result<u64, NsError> {
        let source_mount = self.find_mount_for(source)
            .ok_or(NsError::NotFound)?;

        let flags = if recursive {
            MountFlags::BIND | MountFlags::REC
        } else {
            MountFlags::BIND
        };

        self.mount(&source_mount.device, target, &source_mount.fs_type, flags, "")
    }
}

impl Namespace for MntNamespace {
    fn ns_type(&self) -> NsType {
        NsType::Mnt
    }

    fn id(&self) -> u64 {
        self.id
    }

    fn user_ns(&self) -> Option<Arc<UserNamespace>> {
        Some(self.user_ns.clone())
    }

    fn clone_ns(&self) -> Arc<dyn Namespace> {
        Arc::new(Self {
            id: next_ns_id(),
            user_ns: self.user_ns.clone(),
            parent: self.parent.clone(),
            root: RwLock::new(self.root.read().clone()),
            mounts: RwLock::new(self.mounts.read().clone()),
            mount_id_counter: AtomicU64::new(
                self.mount_id_counter.load(Ordering::Relaxed)
            ),
            mount_count: AtomicU64::new(
                self.mount_count.load(Ordering::Relaxed)
            ),
        })
    }
}

/// Mount structure
pub struct Mount {
    /// Mount ID
    pub id: u64,
    /// Parent mount ID
    pub parent_id: u64,
    /// Device path
    pub device: String,
    /// Mount point
    pub mount_point: String,
    /// Filesystem type
    pub fs_type: String,
    /// Mount options
    pub options: String,
    /// Mount flags
    pub flags: MountFlags,
    /// Propagation type
    pub propagation: PropagationType,
    /// Child mounts
    pub children: RwLock<Vec<u64>>,
}

/// Mount flags
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct MountFlags(u64);

impl MountFlags {
    pub const RDONLY: Self = Self(1 << 0);
    pub const NOSUID: Self = Self(1 << 1);
    pub const NODEV: Self = Self(1 << 2);
    pub const NOEXEC: Self = Self(1 << 3);
    pub const SYNCHRONOUS: Self = Self(1 << 4);
    pub const REMOUNT: Self = Self(1 << 5);
    pub const MANDLOCK: Self = Self(1 << 6);
    pub const DIRSYNC: Self = Self(1 << 7);
    pub const NOATIME: Self = Self(1 << 10);
    pub const NODIRATIME: Self = Self(1 << 11);
    pub const BIND: Self = Self(1 << 12);
    pub const MOVE: Self = Self(1 << 13);
    pub const REC: Self = Self(1 << 14);
    pub const SILENT: Self = Self(1 << 15);
    pub const SHARED: Self = Self(1 << 20);
    pub const SLAVE: Self = Self(1 << 19);
    pub const PRIVATE: Self = Self(1 << 18);
    pub const UNBINDABLE: Self = Self(1 << 17);
}

impl core::ops::BitOr for MountFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// Unmount flags
#[derive(Clone, Copy)]
pub struct UmountFlags(u32);

impl UmountFlags {
    pub const FORCE: Self = Self(1 << 0);
    pub const DETACH: Self = Self(1 << 1);
    pub const EXPIRE: Self = Self(1 << 2);
    pub const NOFOLLOW: Self = Self(1 << 3);

    pub fn contains(&self, flag: UmountFlags) -> bool {
        (self.0 & flag.0) != 0
    }
}

/// Mount propagation type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropagationType {
    Private,
    Shared,
    Slave,
    Unbindable,
}

/// Mount info for /proc/mounts
#[derive(Clone)]
pub struct MountInfo {
    pub mount_id: u64,
    pub parent_id: u64,
    pub device: String,
    pub mount_point: String,
    pub fs_type: String,
    pub options: String,
}

impl MountInfo {
    /// Format as /proc/mounts line
    pub fn format(&self) -> String {
        alloc::format!(
            "{} {} {} {} 0 0",
            self.device,
            self.mount_point,
            self.fs_type,
            self.options
        )
    }
}
