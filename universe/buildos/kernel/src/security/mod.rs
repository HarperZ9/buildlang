// ===============================================================================
// BUILDOS KERNEL - SECURITY SUBSYSTEM
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Security subsystem providing capabilities, seccomp, and access control.
//!
//! This module implements:
//! - POSIX capabilities (fine-grained privilege control)
//! - Seccomp (secure computing mode for syscall filtering)
//! - Mandatory Access Control (MAC) hooks
//! - Namespace isolation
//! - Audit logging

#![allow(static_mut_refs)]

pub mod capabilities;
pub mod seccomp;
pub mod namespace;
pub mod lsm;
pub mod audit;
pub mod credentials;

// Re-export commonly used types
pub use capabilities::{CapabilitySet, Capability, CapBound};
pub use seccomp::{SeccompMode, SeccompFilter, SeccompAction};
pub use namespace::{Namespace, NamespaceType, NamespaceSet};
pub use lsm::{LsmHook, SecurityModule};
pub use audit::{AuditEvent, AuditLog};
pub use credentials::{Credentials, CredentialsBuilder};

use alloc::vec::Vec;

/// Security context for a process
#[derive(Clone, Debug)]
pub struct SecurityContext {
    /// Process credentials
    pub creds: Credentials,
    /// Capability sets
    pub caps: ProcessCapabilities,
    /// Seccomp state
    pub seccomp: SeccompState,
    /// Namespace membership
    pub namespaces: NamespaceSet,
    /// SELinux-style security label
    pub label: Option<SecurityLabel>,
    /// No-new-privileges flag
    pub no_new_privs: bool,
    /// Dumpable flag (for core dumps)
    pub dumpable: bool,
}

impl SecurityContext {
    /// Create a new security context for init
    pub fn init() -> Self {
        Self {
            creds: Credentials::root(),
            caps: ProcessCapabilities::full(),
            seccomp: SeccompState::disabled(),
            namespaces: NamespaceSet::init(),
            label: None,
            no_new_privs: false,
            dumpable: true,
        }
    }

    /// Create a new security context for a regular user
    pub fn user(uid: u32, gid: u32) -> Self {
        Self {
            creds: Credentials::new(uid, gid),
            caps: ProcessCapabilities::empty(),
            seccomp: SeccompState::disabled(),
            namespaces: NamespaceSet::init(),
            label: None,
            no_new_privs: false,
            dumpable: true,
        }
    }

    /// Fork security context
    pub fn fork(&self) -> Self {
        Self {
            creds: self.creds.clone(),
            caps: self.caps.clone(),
            seccomp: self.seccomp.clone(),
            namespaces: self.namespaces.clone(),
            label: self.label.clone(),
            no_new_privs: self.no_new_privs,
            dumpable: self.dumpable,
        }
    }

    /// Exec security context transformation
    pub fn exec(&mut self, file_caps: Option<&FileCaps>) {
        // Apply capability transformation for exec
        self.caps.transform_on_exec(file_caps, &self.creds);

        // Seccomp filters are inherited (SECCOMP_FILTER_FLAG_TSYNC)
        // but strict mode is reset

        // Clear dumpable if setuid/setgid
        if self.creds.is_setuid() || self.creds.is_setgid() {
            self.dumpable = false;
        }
    }

    /// Check if process has a capability
    pub fn has_capability(&self, cap: Capability) -> bool {
        self.caps.effective.contains(cap)
    }

    /// Check if process can perform privileged operation
    pub fn capable(&self, cap: Capability) -> bool {
        // Check capability
        if !self.has_capability(cap) {
            return false;
        }

        // In user namespace, capabilities are relative to that namespace
        // This is a simplified check
        true
    }
}

/// Process capability sets
#[derive(Clone, Debug)]
pub struct ProcessCapabilities {
    /// Permitted: caps that can be assumed
    pub permitted: CapabilitySet,
    /// Inheritable: caps preserved across exec
    pub inheritable: CapabilitySet,
    /// Effective: caps currently in use
    pub effective: CapabilitySet,
    /// Bounding set: caps that can be gained
    pub bounding: CapabilitySet,
    /// Ambient: caps preserved across non-setuid exec
    pub ambient: CapabilitySet,
}

impl ProcessCapabilities {
    /// Empty capability sets
    pub fn empty() -> Self {
        Self {
            permitted: CapabilitySet::empty(),
            inheritable: CapabilitySet::empty(),
            effective: CapabilitySet::empty(),
            bounding: CapabilitySet::full(),
            ambient: CapabilitySet::empty(),
        }
    }

    /// Full capability sets (for init)
    pub fn full() -> Self {
        Self {
            permitted: CapabilitySet::full(),
            inheritable: CapabilitySet::full(),
            effective: CapabilitySet::full(),
            bounding: CapabilitySet::full(),
            ambient: CapabilitySet::empty(),
        }
    }

    /// Transform capabilities on exec
    pub fn transform_on_exec(&mut self, file_caps: Option<&FileCaps>, creds: &Credentials) {
        // Formula from capabilities(7):
        // P'(ambient) = (file is privileged) ? 0 : P(ambient)
        // P'(permitted) = (P(inheritable) & F(inheritable)) |
        //                 (F(permitted) & P(bounding)) | P'(ambient)
        // P'(effective) = F(effective) ? P'(permitted) : P'(ambient)
        // P'(inheritable) = P(inheritable)

        let file_privileged = creds.is_setuid() || file_caps.map(|c| c.effective).unwrap_or(false);

        if file_privileged {
            self.ambient = CapabilitySet::empty();
        }

        if let Some(fc) = file_caps {
            let new_permitted = (self.inheritable.intersect(&fc.inheritable))
                .union(&fc.permitted.intersect(&self.bounding))
                .union(&self.ambient);

            let new_effective = if fc.effective {
                new_permitted.clone()
            } else {
                self.ambient.clone()
            };

            self.permitted = new_permitted;
            self.effective = new_effective;
        } else if !file_privileged {
            // Non-setuid, no file caps: use ambient
            self.permitted = self.permitted.intersect(&self.bounding).union(&self.ambient);
            self.effective = self.ambient.clone();
        } else {
            // Setuid without file caps: traditional behavior
            self.permitted = CapabilitySet::full();
            self.effective = CapabilitySet::full();
        }
    }

    /// Drop a capability from the bounding set
    pub fn drop_bounding(&mut self, cap: Capability) -> Result<(), SecurityError> {
        self.bounding.remove(cap);
        // Also remove from permitted/effective
        self.permitted.remove(cap);
        self.effective.remove(cap);
        self.ambient.remove(cap);
        Ok(())
    }

    /// Raise an ambient capability
    pub fn raise_ambient(&mut self, cap: Capability) -> Result<(), SecurityError> {
        // Cap must be in both permitted and inheritable
        if !self.permitted.contains(cap) || !self.inheritable.contains(cap) {
            return Err(SecurityError::PermissionDenied);
        }
        // Cap must be in bounding set
        if !self.bounding.contains(cap) {
            return Err(SecurityError::PermissionDenied);
        }
        self.ambient.add(cap);
        Ok(())
    }

    /// Lower an ambient capability
    pub fn lower_ambient(&mut self, cap: Capability) {
        self.ambient.remove(cap);
    }
}

/// File capability attachment
#[derive(Clone, Debug)]
pub struct FileCaps {
    /// Permitted caps from file
    pub permitted: CapabilitySet,
    /// Inheritable caps from file
    pub inheritable: CapabilitySet,
    /// Effective flag
    pub effective: bool,
    /// Root ID for namespace
    pub rootid: Option<u32>,
}

/// Seccomp state for a process
#[derive(Clone, Debug)]
pub struct SeccompState {
    /// Current mode
    pub mode: SeccompMode,
    /// BPF filters (in filter mode)
    pub filters: Vec<SeccompFilter>,
}

impl SeccompState {
    /// Disabled seccomp
    pub fn disabled() -> Self {
        Self {
            mode: SeccompMode::Disabled,
            filters: Vec::new(),
        }
    }

    /// Enable strict mode
    pub fn enable_strict(&mut self) -> Result<(), SecurityError> {
        if self.mode != SeccompMode::Disabled {
            return Err(SecurityError::AlreadyEnabled);
        }
        self.mode = SeccompMode::Strict;
        Ok(())
    }

    /// Add a filter
    pub fn add_filter(&mut self, filter: SeccompFilter) -> Result<(), SecurityError> {
        match self.mode {
            SeccompMode::Disabled => {
                self.mode = SeccompMode::Filter;
            }
            SeccompMode::Filter => {}
            SeccompMode::Strict => {
                return Err(SecurityError::AlreadyEnabled);
            }
        }
        self.filters.push(filter);
        Ok(())
    }

    /// Check if syscall is allowed
    pub fn check_syscall(&self, syscall_nr: u32, args: &[u64; 6]) -> SeccompAction {
        match self.mode {
            SeccompMode::Disabled => SeccompAction::Allow,
            SeccompMode::Strict => {
                // Only allow read, write, exit, sigreturn
                match syscall_nr {
                    0 | 1 | 60 | 15 => SeccompAction::Allow, // read, write, exit, rt_sigreturn
                    _ => SeccompAction::Kill,
                }
            }
            SeccompMode::Filter => {
                // Run BPF filters
                for filter in self.filters.iter().rev() {
                    let result = filter.evaluate(syscall_nr, args);
                    if result != SeccompAction::Allow {
                        return result;
                    }
                }
                SeccompAction::Allow
            }
        }
    }
}

/// Security label (SELinux-style)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecurityLabel {
    /// User
    pub user: u32,
    /// Role
    pub role: u32,
    /// Type
    pub type_: u32,
    /// Sensitivity level
    pub level: u32,
}

impl SecurityLabel {
    /// Create unconfined label
    pub fn unconfined() -> Self {
        Self {
            user: 0,
            role: 0,
            type_: 0,
            level: 0,
        }
    }
}

/// Security errors
#[derive(Clone, Debug)]
pub enum SecurityError {
    /// Permission denied
    PermissionDenied,
    /// Invalid argument
    InvalidArgument,
    /// Operation not supported
    NotSupported,
    /// Already enabled
    AlreadyEnabled,
    /// Namespace error
    NamespaceError,
    /// Resource limit
    ResourceLimit,
}

impl SecurityError {
    /// Convert to errno
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::PermissionDenied => -1,  // EPERM
            Self::InvalidArgument => -22,   // EINVAL
            Self::NotSupported => -95,      // EOPNOTSUPP
            Self::AlreadyEnabled => -1,     // EPERM
            Self::NamespaceError => -22,    // EINVAL
            Self::ResourceLimit => -11,     // EAGAIN
        }
    }
}

/// Global security policy
pub struct SecurityPolicy {
    /// Enforce mandatory access control
    pub mac_enabled: bool,
    /// Default deny unknown permissions
    pub default_deny: bool,
    /// Require CAP_SYS_ADMIN for module loading
    pub locked_down: bool,
    /// Audit mode (log without enforce)
    pub audit_only: bool,
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            mac_enabled: false,
            default_deny: false,
            locked_down: false,
            audit_only: false,
        }
    }
}

static mut SECURITY_POLICY: SecurityPolicy = SecurityPolicy {
    mac_enabled: false,
    default_deny: false,
    locked_down: false,
    audit_only: false,
};

/// Get security policy
pub fn policy() -> &'static SecurityPolicy {
    unsafe { &SECURITY_POLICY }
}

/// Initialize security subsystem
pub fn init() {
    // Initialize audit logging
    audit::init();

    // Initialize LSM hooks
    lsm::init();

    // Initialize capability system
    capabilities::init();

    // Initialize seccomp
    seccomp::init();

    // Create initial namespaces
    namespace::init();

    crate::kprintln!("[SECURITY] Security subsystem initialized");
}

/// Check access permission
pub fn check_access(
    subject: &SecurityContext,
    object_label: &SecurityLabel,
    access: u32,
) -> Result<(), SecurityError> {
    // Run LSM hooks
    if !lsm::check_permission(subject, object_label, access) {
        audit::log_denial(subject, object_label, access);
        return Err(SecurityError::PermissionDenied);
    }

    Ok(())
}

/// Check if caller has capability
pub fn capable(_cap: Capability) -> bool {
    // Would get current process security context
    // For now, return true for simplicity
    true
}

/// Enter a security sandbox
pub fn sandbox() -> Result<(), SecurityError> {
    // Would:
    // 1. Drop all capabilities
    // 2. Enable seccomp strict mode
    // 3. Clear supplementary groups
    // 4. Set no_new_privs
    Ok(())
}
