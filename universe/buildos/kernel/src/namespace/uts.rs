// ===============================================================================
// BUILDOS KERNEL - UTS NAMESPACE
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! UTS Namespace Implementation
//!
//! Provides hostname and domain name isolation. Each UTS namespace
//! has its own hostname and NIS domain name.

use alloc::sync::Arc;
use alloc::string::String;

use crate::sync::RwLock;
use super::{Namespace, NsType, NsError, next_ns_id};
use super::user::UserNamespace;

/// Maximum hostname length
pub const HOSTNAME_MAX_LEN: usize = 64;
/// Maximum domain name length
pub const DOMAINNAME_MAX_LEN: usize = 64;

/// UTS namespace structure
pub struct UtsNamespace {
    /// Namespace ID
    id: u64,
    /// Owning user namespace
    user_ns: Arc<UserNamespace>,
    /// Hostname
    hostname: RwLock<String>,
    /// NIS domain name
    domainname: RwLock<String>,
}

impl UtsNamespace {
    /// Create initial (root) UTS namespace
    pub fn new_initial(user_ns: Arc<UserNamespace>) -> Self {
        Self {
            id: next_ns_id(),
            user_ns,
            hostname: RwLock::new("buildos".into()),
            domainname: RwLock::new("(none)".into()),
        }
    }

    /// Create child UTS namespace (copies values from parent)
    pub fn new_child(parent: Arc<UtsNamespace>, user_ns: Arc<UserNamespace>) -> Self {
        Self {
            id: next_ns_id(),
            user_ns,
            hostname: RwLock::new(parent.hostname.read().clone()),
            domainname: RwLock::new(parent.domainname.read().clone()),
        }
    }

    /// Get hostname
    pub fn hostname(&self) -> String {
        self.hostname.read().clone()
    }

    /// Set hostname
    pub fn set_hostname(&self, name: &str) -> Result<(), NsError> {
        if name.len() > HOSTNAME_MAX_LEN {
            return Err(NsError::InvalidOperation);
        }

        // Validate hostname characters
        if !is_valid_hostname(name) {
            return Err(NsError::InvalidOperation);
        }

        *self.hostname.write() = name.into();

        crate::kprintln!("[NS] Set hostname to '{}'", name);

        Ok(())
    }

    /// Get NIS domain name
    pub fn domainname(&self) -> String {
        self.domainname.read().clone()
    }

    /// Set NIS domain name
    pub fn set_domainname(&self, name: &str) -> Result<(), NsError> {
        if name.len() > DOMAINNAME_MAX_LEN {
            return Err(NsError::InvalidOperation);
        }

        *self.domainname.write() = name.into();

        crate::kprintln!("[NS] Set domainname to '{}'", name);

        Ok(())
    }

    /// Get uname info
    pub fn uname(&self) -> UtsName {
        UtsName {
            sysname: "BuildOS".into(),
            nodename: self.hostname.read().clone(),
            release: "2.0.0".into(),
            version: "#1 SMP PREEMPT".into(),
            machine: "x86_64".into(),
            domainname: self.domainname.read().clone(),
        }
    }
}

impl Namespace for UtsNamespace {
    fn ns_type(&self) -> NsType {
        NsType::Uts
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
            hostname: RwLock::new(self.hostname.read().clone()),
            domainname: RwLock::new(self.domainname.read().clone()),
        })
    }
}

/// Check if hostname is valid
fn is_valid_hostname(name: &str) -> bool {
    if name.is_empty() || name.len() > HOSTNAME_MAX_LEN {
        return false;
    }

    // Must start with alphanumeric
    let first = name.chars().next().unwrap();
    if !first.is_ascii_alphanumeric() {
        return false;
    }

    // Can contain alphanumeric, hyphen, period
    for c in name.chars() {
        if !c.is_ascii_alphanumeric() && c != '-' && c != '.' {
            return false;
        }
    }

    // Cannot end with hyphen
    if name.ends_with('-') {
        return false;
    }

    true
}

/// UTS name structure (for uname syscall)
#[derive(Clone)]
pub struct UtsName {
    /// OS name
    pub sysname: String,
    /// Node name (hostname)
    pub nodename: String,
    /// OS release
    pub release: String,
    /// OS version
    pub version: String,
    /// Machine architecture
    pub machine: String,
    /// NIS domain name
    pub domainname: String,
}

impl UtsName {
    /// Format for display
    pub fn format(&self) -> String {
        alloc::format!(
            "{} {} {} {} {}",
            self.sysname,
            self.nodename,
            self.release,
            self.version,
            self.machine
        )
    }
}

/// Syscall implementations

/// sethostname syscall
pub fn sys_sethostname(ns: &UtsNamespace, name: &str) -> Result<(), NsError> {
    // Would check CAP_SYS_ADMIN capability
    ns.set_hostname(name)
}

/// gethostname syscall
pub fn sys_gethostname(ns: &UtsNamespace) -> String {
    ns.hostname()
}

/// setdomainname syscall
pub fn sys_setdomainname(ns: &UtsNamespace, name: &str) -> Result<(), NsError> {
    // Would check CAP_SYS_ADMIN capability
    ns.set_domainname(name)
}

/// getdomainname syscall
pub fn sys_getdomainname(ns: &UtsNamespace) -> String {
    ns.domainname()
}

/// uname syscall
pub fn sys_uname(ns: &UtsNamespace) -> UtsName {
    ns.uname()
}
