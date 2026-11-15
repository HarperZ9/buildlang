// ===============================================================================
// BUILDOS KERNEL - NAMESPACE SUBSYSTEM
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Namespace Subsystem
//!
//! Provides process isolation through namespaces:
//! - PID namespace: Process ID isolation
//! - NET namespace: Network isolation
//! - MNT namespace: Mount point isolation
//! - UTS namespace: Hostname/domain isolation
//! - IPC namespace: IPC resource isolation
//! - USER namespace: User/group ID isolation
//! - CGROUP namespace: Cgroup root isolation
//! - TIME namespace: Clock isolation

#![allow(dead_code)]

pub mod pid;
pub mod net;
pub mod mnt;
pub mod uts;
pub mod ipc;
pub mod user;

use alloc::sync::Arc;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::sync::RwLock;

/// Namespace subsystem initialized
static NS_INITIALIZED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// Initial namespaces
static INIT_NS: RwLock<Option<Namespaces>> = RwLock::new(None);

/// All namespaces by ID
static ALL_NAMESPACES: RwLock<BTreeMap<u64, Arc<dyn Namespace>>> =
    RwLock::new(BTreeMap::new());

/// Namespace ID counter
static NS_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Clone flags for namespaces
pub mod flags {
    pub const CLONE_NEWNS: u64 = 0x00020000;      // Mount namespace
    pub const CLONE_NEWUTS: u64 = 0x04000000;     // UTS namespace
    pub const CLONE_NEWIPC: u64 = 0x08000000;     // IPC namespace
    pub const CLONE_NEWUSER: u64 = 0x10000000;    // User namespace
    pub const CLONE_NEWPID: u64 = 0x20000000;     // PID namespace
    pub const CLONE_NEWNET: u64 = 0x40000000;     // Network namespace
    pub const CLONE_NEWCGROUP: u64 = 0x02000000;  // Cgroup namespace
    pub const CLONE_NEWTIME: u64 = 0x00000080;    // Time namespace
}

/// Initialize namespace subsystem
pub fn init() {
    // Create initial namespaces
    let init_ns = Namespaces::new_initial();
    *INIT_NS.write() = Some(init_ns);

    NS_INITIALIZED.store(true, Ordering::Release);
    crate::kprintln!("[NS] Namespace subsystem initialized");
}

/// Namespace types
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NsType {
    Pid,
    Net,
    Mnt,
    Uts,
    Ipc,
    User,
    Cgroup,
    Time,
}

impl NsType {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Pid => "pid",
            Self::Net => "net",
            Self::Mnt => "mnt",
            Self::Uts => "uts",
            Self::Ipc => "ipc",
            Self::User => "user",
            Self::Cgroup => "cgroup",
            Self::Time => "time",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "pid" => Some(Self::Pid),
            "net" => Some(Self::Net),
            "mnt" | "ns" => Some(Self::Mnt),
            "uts" => Some(Self::Uts),
            "ipc" => Some(Self::Ipc),
            "user" => Some(Self::User),
            "cgroup" => Some(Self::Cgroup),
            "time" => Some(Self::Time),
            _ => None,
        }
    }

    pub fn clone_flag(&self) -> u64 {
        match self {
            Self::Pid => flags::CLONE_NEWPID,
            Self::Net => flags::CLONE_NEWNET,
            Self::Mnt => flags::CLONE_NEWNS,
            Self::Uts => flags::CLONE_NEWUTS,
            Self::Ipc => flags::CLONE_NEWIPC,
            Self::User => flags::CLONE_NEWUSER,
            Self::Cgroup => flags::CLONE_NEWCGROUP,
            Self::Time => flags::CLONE_NEWTIME,
        }
    }
}

/// Namespace trait
pub trait Namespace: Send + Sync {
    /// Get namespace type
    fn ns_type(&self) -> NsType;

    /// Get namespace ID
    fn id(&self) -> u64;

    /// Get user namespace that owns this
    fn user_ns(&self) -> Option<Arc<user::UserNamespace>>;

    /// Clone this namespace
    fn clone_ns(&self) -> Arc<dyn Namespace>;
}

/// Collection of namespaces for a process
#[derive(Clone)]
pub struct Namespaces {
    pub pid: Arc<pid::PidNamespace>,
    pub net: Arc<net::NetNamespace>,
    pub mnt: Arc<mnt::MntNamespace>,
    pub uts: Arc<uts::UtsNamespace>,
    pub ipc: Arc<ipc::IpcNamespace>,
    pub user: Arc<user::UserNamespace>,
}

impl Namespaces {
    /// Create initial namespaces
    fn new_initial() -> Self {
        let user = Arc::new(user::UserNamespace::new_initial());
        let pid = Arc::new(pid::PidNamespace::new_initial(user.clone()));
        let net = Arc::new(net::NetNamespace::new_initial(user.clone()));
        let mnt = Arc::new(mnt::MntNamespace::new_initial(user.clone()));
        let uts = Arc::new(uts::UtsNamespace::new_initial(user.clone()));
        let ipc = Arc::new(ipc::IpcNamespace::new_initial(user.clone()));

        Self { pid, net, mnt, uts, ipc, user }
    }

    /// Create new namespaces based on clone flags
    pub fn new_from_flags(&self, flags: u64) -> Self {
        let user = if flags & flags::CLONE_NEWUSER != 0 {
            Arc::new(user::UserNamespace::new_child(self.user.clone()))
        } else {
            self.user.clone()
        };

        let pid = if flags & flags::CLONE_NEWPID != 0 {
            Arc::new(pid::PidNamespace::new_child(self.pid.clone(), user.clone()))
        } else {
            self.pid.clone()
        };

        let net = if flags & flags::CLONE_NEWNET != 0 {
            Arc::new(net::NetNamespace::new_child(user.clone()))
        } else {
            self.net.clone()
        };

        let mnt = if flags & flags::CLONE_NEWNS != 0 {
            Arc::new(mnt::MntNamespace::new_child(self.mnt.clone(), user.clone()))
        } else {
            self.mnt.clone()
        };

        let uts = if flags & flags::CLONE_NEWUTS != 0 {
            Arc::new(uts::UtsNamespace::new_child(self.uts.clone(), user.clone()))
        } else {
            self.uts.clone()
        };

        let ipc = if flags & flags::CLONE_NEWIPC != 0 {
            Arc::new(ipc::IpcNamespace::new_child(user.clone()))
        } else {
            self.ipc.clone()
        };

        Self { pid, net, mnt, uts, ipc, user }
    }

    /// Get initial namespaces
    pub fn initial() -> Self {
        INIT_NS.read().clone().unwrap_or_else(|| Self::new_initial())
    }
}

/// Namespace error
#[derive(Clone, Debug)]
pub enum NsError {
    /// Permission denied
    PermissionDenied,
    /// Namespace not found
    NotFound,
    /// Invalid operation
    InvalidOperation,
    /// Too many nested namespaces
    TooManyLevels,
    /// Resource limit
    ResourceLimit,
}

impl NsError {
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::PermissionDenied => -1,    // EPERM
            Self::NotFound => -2,             // ENOENT
            Self::InvalidOperation => -22,    // EINVAL
            Self::TooManyLevels => -40,       // ELOOP
            Self::ResourceLimit => -12,       // ENOMEM
        }
    }
}

/// Generate new namespace ID
pub fn next_ns_id() -> u64 {
    NS_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Enter a namespace
pub fn setns(_fd: i32, ns_type: NsType) -> Result<(), NsError> {
    // Would validate fd refers to a namespace file
    // and update current process's namespaces

    crate::kprintln!("[NS] setns to {} namespace", ns_type.name());
    Ok(())
}

/// Unshare namespaces
pub fn unshare(flags: u64) -> Result<(), NsError> {
    // Get current process's namespaces
    // Create new namespaces based on flags
    // Update current process

    crate::kprintln!("[NS] unshare flags: 0x{:x}", flags);
    Ok(())
}

/// Get namespace info
pub fn get_ns_info(_pid: u32, ns_type: NsType) -> Option<NsInfo> {
    // Would look up process's namespace of given type
    Some(NsInfo {
        ns_type,
        id: 1,
        user_ns_id: 1,
    })
}

/// Namespace info
#[derive(Clone, Debug)]
pub struct NsInfo {
    pub ns_type: NsType,
    pub id: u64,
    pub user_ns_id: u64,
}

/// Procfs-style namespace listing for a process
pub fn proc_ns(_pid: u32) -> Vec<(String, u64)> {
    let mut ns_list = Vec::new();

    // Would look up actual namespace IDs
    ns_list.push(("pid".to_string(), 1));
    ns_list.push(("net".to_string(), 1));
    ns_list.push(("mnt".to_string(), 1));
    ns_list.push(("uts".to_string(), 1));
    ns_list.push(("ipc".to_string(), 1));
    ns_list.push(("user".to_string(), 1));

    ns_list
}

/// Check if caller can enter a user namespace
pub fn can_enter_user_ns(_target: &user::UserNamespace) -> bool {
    // Check capabilities and permissions
    true
}

/// Maximum user namespace nesting level
pub const MAX_USER_NS_LEVEL: u32 = 32;

/// Maximum PID namespace nesting level
pub const MAX_PID_NS_LEVEL: u32 = 32;
