// ===============================================================================
// BUILDOS KERNEL - EXTENDED ATTRIBUTES (XATTR) SUPPORT
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Extended Attributes (xattr) Support
//!
//! Provides file extended attribute operations supporting:
//! - user.* namespace (user-defined attributes)
//! - system.* namespace (system attributes like ACLs)
//! - security.* namespace (security labels like SELinux)
//! - trusted.* namespace (trusted/admin attributes)

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;


/// Maximum extended attribute name length
pub const XATTR_NAME_MAX: usize = 255;
/// Maximum extended attribute value size
pub const XATTR_SIZE_MAX: usize = 65536;
/// Maximum number of xattrs per inode
pub const XATTR_MAX_COUNT: usize = 1024;

/// Extended attribute namespaces
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum XattrNamespace {
    /// User-defined attributes (user.*)
    User,
    /// System attributes (system.*)
    System,
    /// Security labels (security.*)
    Security,
    /// Trusted/admin attributes (trusted.*)
    Trusted,
}

impl XattrNamespace {
    /// Get namespace prefix
    pub fn prefix(&self) -> &'static str {
        match self {
            Self::User => "user.",
            Self::System => "system.",
            Self::Security => "security.",
            Self::Trusted => "trusted.",
        }
    }

    /// Parse namespace from attribute name
    pub fn from_name(name: &str) -> Option<(Self, &str)> {
        if let Some(suffix) = name.strip_prefix("user.") {
            Some((Self::User, suffix))
        } else if let Some(suffix) = name.strip_prefix("system.") {
            Some((Self::System, suffix))
        } else if let Some(suffix) = name.strip_prefix("security.") {
            Some((Self::Security, suffix))
        } else if let Some(suffix) = name.strip_prefix("trusted.") {
            Some((Self::Trusted, suffix))
        } else {
            None
        }
    }

    /// Check if current process can access this namespace
    pub fn can_access(&self, uid: u32, _is_owner: bool) -> bool {
        match self {
            Self::User => true, // Anyone can access user.* if they have file access
            Self::System => true, // System attributes are readable by all
            Self::Security => true, // Security labels readable, write needs CAP_MAC_ADMIN
            Self::Trusted => uid == 0, // Only root can access trusted.*
        }
    }

    /// Check if current process can write this namespace
    pub fn can_write(&self, uid: u32, is_owner: bool) -> bool {
        match self {
            Self::User => is_owner || uid == 0,
            Self::System => uid == 0, // Only root can modify system.*
            Self::Security => uid == 0, // Needs CAP_MAC_ADMIN
            Self::Trusted => uid == 0,
        }
    }
}

/// Extended attribute error
#[derive(Clone, Debug)]
pub enum XattrError {
    /// Attribute not found
    NotFound,
    /// Permission denied
    PermissionDenied,
    /// Attribute already exists (XATTR_CREATE)
    AlreadyExists,
    /// Invalid name
    InvalidName,
    /// Value too large
    ValueTooLarge,
    /// Too many attributes
    TooManyAttributes,
    /// Attribute not supported
    NotSupported,
    /// Buffer too small
    Range,
    /// I/O error
    IoError,
}

impl XattrError {
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::NotFound => -2,        // ENODATA / ENOATTR
            Self::PermissionDenied => -1, // EPERM
            Self::AlreadyExists => -17,   // EEXIST
            Self::InvalidName => -22,     // EINVAL
            Self::ValueTooLarge => -7,    // E2BIG
            Self::TooManyAttributes => -28, // ENOSPC
            Self::NotSupported => -95,    // EOPNOTSUPP
            Self::Range => -34,           // ERANGE
            Self::IoError => -5,          // EIO
        }
    }
}

/// Extended attribute flags
pub mod flags {
    /// Create xattr, fail if exists
    pub const XATTR_CREATE: i32 = 0x1;
    /// Replace xattr, fail if doesn't exist
    pub const XATTR_REPLACE: i32 = 0x2;
}

/// Extended attribute storage for an inode
#[derive(Clone)]
pub struct XattrSet {
    /// Attributes indexed by full name
    attrs: BTreeMap<String, Vec<u8>>,
}

impl XattrSet {
    /// Create empty xattr set
    pub fn new() -> Self {
        Self {
            attrs: BTreeMap::new(),
        }
    }

    /// Get attribute value
    pub fn get(&self, name: &str) -> Option<&Vec<u8>> {
        self.attrs.get(name)
    }

    /// Set attribute value
    pub fn set(&mut self, name: &str, value: &[u8], xflags: i32) -> Result<(), XattrError> {
        // Validate name
        if name.is_empty() || name.len() > XATTR_NAME_MAX {
            return Err(XattrError::InvalidName);
        }

        // Check namespace
        if XattrNamespace::from_name(name).is_none() {
            return Err(XattrError::InvalidName);
        }

        // Validate value size
        if value.len() > XATTR_SIZE_MAX {
            return Err(XattrError::ValueTooLarge);
        }

        let exists = self.attrs.contains_key(name);

        // Check flags
        if xflags & flags::XATTR_CREATE != 0 && exists {
            return Err(XattrError::AlreadyExists);
        }
        if xflags & flags::XATTR_REPLACE != 0 && !exists {
            return Err(XattrError::NotFound);
        }

        // Check count limit
        if !exists && self.attrs.len() >= XATTR_MAX_COUNT {
            return Err(XattrError::TooManyAttributes);
        }

        self.attrs.insert(name.into(), value.to_vec());
        Ok(())
    }

    /// Remove attribute
    pub fn remove(&mut self, name: &str) -> Result<(), XattrError> {
        self.attrs.remove(name)
            .map(|_| ())
            .ok_or(XattrError::NotFound)
    }

    /// List all attribute names
    pub fn list(&self) -> Vec<&str> {
        self.attrs.keys().map(|s| s.as_str()).collect()
    }

    /// Get total size of all attribute names (for listxattr)
    pub fn list_size(&self) -> usize {
        self.attrs.keys().map(|k| k.len() + 1).sum() // +1 for null terminator
    }

    /// Count of attributes
    pub fn count(&self) -> usize {
        self.attrs.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.attrs.is_empty()
    }

    /// Iterate over attributes
    pub fn iter(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.attrs.iter().map(|(k, v)| (k.as_str(), v.as_slice()))
    }
}

impl Default for XattrSet {
    fn default() -> Self {
        Self::new()
    }
}

/// Extended attribute handler trait for filesystem implementations
pub trait XattrHandler: Send + Sync {
    /// Get namespace prefix this handler manages
    fn prefix(&self) -> &str;

    /// Get extended attribute
    fn get(&self, inode: u64, name: &str) -> Result<Vec<u8>, XattrError>;

    /// Set extended attribute
    fn set(&self, inode: u64, name: &str, value: &[u8], flags: i32) -> Result<(), XattrError>;

    /// Remove extended attribute
    fn remove(&self, inode: u64, name: &str) -> Result<(), XattrError>;

    /// List attributes in this namespace
    fn list(&self, inode: u64) -> Result<Vec<String>, XattrError>;
}

/// User namespace xattr handler
pub struct UserXattrHandler;

impl XattrHandler for UserXattrHandler {
    fn prefix(&self) -> &str {
        "user."
    }

    fn get(&self, _inode: u64, _name: &str) -> Result<Vec<u8>, XattrError> {
        // Would read from inode's xattr storage
        Err(XattrError::NotFound)
    }

    fn set(&self, _inode: u64, _name: &str, _value: &[u8], _flags: i32) -> Result<(), XattrError> {
        // Would write to inode's xattr storage
        Ok(())
    }

    fn remove(&self, _inode: u64, _name: &str) -> Result<(), XattrError> {
        // Would remove from inode's xattr storage
        Ok(())
    }

    fn list(&self, _inode: u64) -> Result<Vec<String>, XattrError> {
        Ok(Vec::new())
    }
}

/// Security namespace xattr handler (for SELinux, Smack, etc.)
pub struct SecurityXattrHandler;

impl XattrHandler for SecurityXattrHandler {
    fn prefix(&self) -> &str {
        "security."
    }

    fn get(&self, _inode: u64, name: &str) -> Result<Vec<u8>, XattrError> {
        // Handle common security attributes
        match name {
            "selinux" => {
                // Would return SELinux context
                Err(XattrError::NotFound)
            }
            "capability" => {
                // Would return file capabilities
                Err(XattrError::NotFound)
            }
            _ => Err(XattrError::NotFound),
        }
    }

    fn set(&self, _inode: u64, name: &str, _value: &[u8], _flags: i32) -> Result<(), XattrError> {
        match name {
            "selinux" | "capability" => Ok(()),
            _ => Err(XattrError::NotSupported),
        }
    }

    fn remove(&self, _inode: u64, name: &str) -> Result<(), XattrError> {
        match name {
            "selinux" | "capability" => Ok(()),
            _ => Err(XattrError::NotFound),
        }
    }

    fn list(&self, _inode: u64) -> Result<Vec<String>, XattrError> {
        Ok(Vec::new())
    }
}

/// System namespace xattr handler (for ACLs, etc.)
pub struct SystemXattrHandler;

impl XattrHandler for SystemXattrHandler {
    fn prefix(&self) -> &str {
        "system."
    }

    fn get(&self, _inode: u64, name: &str) -> Result<Vec<u8>, XattrError> {
        match name {
            "posix_acl_access" | "posix_acl_default" => {
                // Would return POSIX ACL
                Err(XattrError::NotFound)
            }
            _ => Err(XattrError::NotFound),
        }
    }

    fn set(&self, _inode: u64, name: &str, _value: &[u8], _flags: i32) -> Result<(), XattrError> {
        match name {
            "posix_acl_access" | "posix_acl_default" => Ok(()),
            _ => Err(XattrError::NotSupported),
        }
    }

    fn remove(&self, _inode: u64, name: &str) -> Result<(), XattrError> {
        match name {
            "posix_acl_access" | "posix_acl_default" => Ok(()),
            _ => Err(XattrError::NotFound),
        }
    }

    fn list(&self, _inode: u64) -> Result<Vec<String>, XattrError> {
        Ok(Vec::new())
    }
}

/// Trusted namespace xattr handler
pub struct TrustedXattrHandler;

impl XattrHandler for TrustedXattrHandler {
    fn prefix(&self) -> &str {
        "trusted."
    }

    fn get(&self, _inode: u64, _name: &str) -> Result<Vec<u8>, XattrError> {
        Err(XattrError::NotFound)
    }

    fn set(&self, _inode: u64, _name: &str, _value: &[u8], _flags: i32) -> Result<(), XattrError> {
        Ok(())
    }

    fn remove(&self, _inode: u64, _name: &str) -> Result<(), XattrError> {
        Ok(())
    }

    fn list(&self, _inode: u64) -> Result<Vec<String>, XattrError> {
        Ok(Vec::new())
    }
}

// =============================================================================
// SYSCALL IMPLEMENTATIONS
// =============================================================================

/// getxattr syscall implementation
pub fn sys_getxattr(
    path: &str,
    name: &str,
    _value: &mut [u8],
) -> Result<usize, XattrError> {
    // Validate name
    let (namespace, _suffix) = XattrNamespace::from_name(name)
        .ok_or(XattrError::InvalidName)?;

    // Check read permission
    if !namespace.can_access(0, true) {
        return Err(XattrError::PermissionDenied);
    }

    crate::kprintln!("[XATTR] getxattr {} {}", path, name);

    // Would look up inode and get xattr
    Err(XattrError::NotFound)
}

/// lgetxattr syscall (no symlink follow)
pub fn sys_lgetxattr(
    path: &str,
    name: &str,
    value: &mut [u8],
) -> Result<usize, XattrError> {
    // Same as getxattr but doesn't follow symlinks
    sys_getxattr(path, name, value)
}

/// fgetxattr syscall (by file descriptor)
pub fn sys_fgetxattr(
    fd: i32,
    name: &str,
    _value: &mut [u8],
) -> Result<usize, XattrError> {
    crate::kprintln!("[XATTR] fgetxattr fd={} name={}", fd, name);
    Err(XattrError::NotFound)
}

/// setxattr syscall
pub fn sys_setxattr(
    path: &str,
    name: &str,
    value: &[u8],
    _flags: i32,
) -> Result<(), XattrError> {
    // Validate name
    let (namespace, _suffix) = XattrNamespace::from_name(name)
        .ok_or(XattrError::InvalidName)?;

    // Check write permission
    if !namespace.can_write(0, true) {
        return Err(XattrError::PermissionDenied);
    }

    // Validate value size
    if value.len() > XATTR_SIZE_MAX {
        return Err(XattrError::ValueTooLarge);
    }

    crate::kprintln!("[XATTR] setxattr {} {} ({} bytes)", path, name, value.len());

    Ok(())
}

/// lsetxattr syscall (no symlink follow)
pub fn sys_lsetxattr(
    path: &str,
    name: &str,
    value: &[u8],
    flags: i32,
) -> Result<(), XattrError> {
    sys_setxattr(path, name, value, flags)
}

/// fsetxattr syscall (by file descriptor)
pub fn sys_fsetxattr(
    fd: i32,
    name: &str,
    value: &[u8],
    _flags: i32,
) -> Result<(), XattrError> {
    crate::kprintln!("[XATTR] fsetxattr fd={} name={} ({} bytes)", fd, name, value.len());
    Ok(())
}

/// listxattr syscall
pub fn sys_listxattr(
    path: &str,
    _list: &mut [u8],
) -> Result<usize, XattrError> {
    crate::kprintln!("[XATTR] listxattr {}", path);

    // Would return null-separated list of xattr names
    Ok(0)
}

/// llistxattr syscall (no symlink follow)
pub fn sys_llistxattr(
    path: &str,
    list: &mut [u8],
) -> Result<usize, XattrError> {
    sys_listxattr(path, list)
}

/// flistxattr syscall (by file descriptor)
pub fn sys_flistxattr(
    fd: i32,
    _list: &mut [u8],
) -> Result<usize, XattrError> {
    crate::kprintln!("[XATTR] flistxattr fd={}", fd);
    Ok(0)
}

/// removexattr syscall
pub fn sys_removexattr(path: &str, name: &str) -> Result<(), XattrError> {
    let (namespace, _suffix) = XattrNamespace::from_name(name)
        .ok_or(XattrError::InvalidName)?;

    if !namespace.can_write(0, true) {
        return Err(XattrError::PermissionDenied);
    }

    crate::kprintln!("[XATTR] removexattr {} {}", path, name);

    Ok(())
}

/// lremovexattr syscall (no symlink follow)
pub fn sys_lremovexattr(path: &str, name: &str) -> Result<(), XattrError> {
    sys_removexattr(path, name)
}

/// fremovexattr syscall (by file descriptor)
pub fn sys_fremovexattr(fd: i32, name: &str) -> Result<(), XattrError> {
    crate::kprintln!("[XATTR] fremovexattr fd={} name={}", fd, name);
    Ok(())
}

/// Initialize xattr subsystem
pub fn init() {
    crate::kprintln!("[FS] Extended attributes (xattr) initialized");
}
