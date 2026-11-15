//! Process Credentials
//!
//! Manages process identity including UIDs, GIDs, and supplementary groups.

use alloc::vec::Vec;

/// Process credentials
#[derive(Clone, Debug)]
pub struct Credentials {
    /// Real user ID
    pub uid: u32,
    /// Real group ID
    pub gid: u32,
    /// Saved set-user-ID
    pub suid: u32,
    /// Saved set-group-ID
    pub sgid: u32,
    /// Effective user ID
    pub euid: u32,
    /// Effective group ID
    pub egid: u32,
    /// Filesystem user ID
    pub fsuid: u32,
    /// Filesystem group ID
    pub fsgid: u32,
    /// Supplementary groups
    pub groups: Vec<u32>,
    /// Securebits
    pub securebits: u32,
    /// User namespace
    pub user_ns: u64,
}

impl Credentials {
    /// Create root credentials
    pub fn root() -> Self {
        Self {
            uid: 0,
            gid: 0,
            suid: 0,
            sgid: 0,
            euid: 0,
            egid: 0,
            fsuid: 0,
            fsgid: 0,
            groups: Vec::new(),
            securebits: 0,
            user_ns: 0,
        }
    }

    /// Create credentials for a regular user
    pub fn new(uid: u32, gid: u32) -> Self {
        Self {
            uid,
            gid,
            suid: uid,
            sgid: gid,
            euid: uid,
            egid: gid,
            fsuid: uid,
            fsgid: gid,
            groups: Vec::new(),
            securebits: 0,
            user_ns: 0,
        }
    }

    /// Check if credentials represent root
    pub fn is_root(&self) -> bool {
        self.euid == 0
    }

    /// Check if this is a setuid process
    pub fn is_setuid(&self) -> bool {
        self.uid != self.euid
    }

    /// Check if this is a setgid process
    pub fn is_setgid(&self) -> bool {
        self.gid != self.egid
    }

    /// Check if user has access to a file
    pub fn can_access(&self, owner_uid: u32, owner_gid: u32, mode: u32, access: u32) -> bool {
        // Root can access anything
        if self.fsuid == 0 {
            return true;
        }

        if self.fsuid == owner_uid {
            // Owner permissions (bits 8-6)
            let owner_perm = (mode >> 6) & 0x7;
            return (owner_perm & access) == access;
        }

        if self.fsgid == owner_gid || self.groups.contains(&owner_gid) {
            // Group permissions (bits 5-3)
            let group_perm = (mode >> 3) & 0x7;
            return (group_perm & access) == access;
        }

        // Other permissions (bits 2-0)
        let other_perm = mode & 0x7;
        (other_perm & access) == access
    }

    /// Set real UID
    pub fn setuid(&mut self, uid: u32) -> Result<(), CredError> {
        if self.euid == 0 {
            // Root can set any UID
            self.uid = uid;
            self.euid = uid;
            self.suid = uid;
            self.fsuid = uid;
        } else if uid == self.uid || uid == self.suid {
            // Can set to real or saved UID
            self.euid = uid;
            self.fsuid = uid;
        } else {
            return Err(CredError::PermissionDenied);
        }
        Ok(())
    }

    /// Set effective UID
    pub fn seteuid(&mut self, euid: u32) -> Result<(), CredError> {
        if self.euid == 0 || euid == self.uid || euid == self.suid {
            self.euid = euid;
            self.fsuid = euid;
            Ok(())
        } else {
            Err(CredError::PermissionDenied)
        }
    }

    /// Set real and effective UID
    pub fn setreuid(&mut self, ruid: i32, euid: i32) -> Result<(), CredError> {
        let old_ruid = self.uid;

        if ruid != -1 {
            let ruid = ruid as u32;
            if self.euid == 0 || ruid == self.uid || ruid == self.euid {
                self.uid = ruid;
            } else {
                return Err(CredError::PermissionDenied);
            }
        }

        if euid != -1 {
            let euid = euid as u32;
            if self.euid == 0 || euid == self.uid || euid == old_ruid || euid == self.suid {
                self.euid = euid;
                self.fsuid = euid;
            } else {
                return Err(CredError::PermissionDenied);
            }
        }

        // Update saved UID if real or effective changed
        if ruid != -1 || (euid != -1 && self.euid != old_ruid) {
            self.suid = self.euid;
        }

        Ok(())
    }

    /// Set real, effective, and saved UID
    pub fn setresuid(&mut self, ruid: i32, euid: i32, suid: i32) -> Result<(), CredError> {
        // Check permissions first
        if self.euid != 0 {
            if ruid != -1 {
                let ruid = ruid as u32;
                if ruid != self.uid && ruid != self.euid && ruid != self.suid {
                    return Err(CredError::PermissionDenied);
                }
            }
            if euid != -1 {
                let euid = euid as u32;
                if euid != self.uid && euid != self.euid && euid != self.suid {
                    return Err(CredError::PermissionDenied);
                }
            }
            if suid != -1 {
                let suid = suid as u32;
                if suid != self.uid && suid != self.euid && suid != self.suid {
                    return Err(CredError::PermissionDenied);
                }
            }
        }

        // Apply changes
        if ruid != -1 {
            self.uid = ruid as u32;
        }
        if euid != -1 {
            self.euid = euid as u32;
            self.fsuid = euid as u32;
        }
        if suid != -1 {
            self.suid = suid as u32;
        }

        Ok(())
    }

    /// Set real GID
    pub fn setgid(&mut self, gid: u32) -> Result<(), CredError> {
        if self.euid == 0 {
            self.gid = gid;
            self.egid = gid;
            self.sgid = gid;
            self.fsgid = gid;
        } else if gid == self.gid || gid == self.sgid {
            self.egid = gid;
            self.fsgid = gid;
        } else {
            return Err(CredError::PermissionDenied);
        }
        Ok(())
    }

    /// Set effective GID
    pub fn setegid(&mut self, egid: u32) -> Result<(), CredError> {
        if self.euid == 0 || egid == self.gid || egid == self.sgid {
            self.egid = egid;
            self.fsgid = egid;
            Ok(())
        } else {
            Err(CredError::PermissionDenied)
        }
    }

    /// Set real and effective GID
    pub fn setregid(&mut self, rgid: i32, egid: i32) -> Result<(), CredError> {
        let old_rgid = self.gid;

        if rgid != -1 {
            let rgid = rgid as u32;
            if self.euid == 0 || rgid == self.gid || rgid == self.egid {
                self.gid = rgid;
            } else {
                return Err(CredError::PermissionDenied);
            }
        }

        if egid != -1 {
            let egid = egid as u32;
            if self.euid == 0 || egid == self.gid || egid == old_rgid || egid == self.sgid {
                self.egid = egid;
                self.fsgid = egid;
            } else {
                return Err(CredError::PermissionDenied);
            }
        }

        if rgid != -1 || (egid != -1 && self.egid != old_rgid) {
            self.sgid = self.egid;
        }

        Ok(())
    }

    /// Set real, effective, and saved GID
    pub fn setresgid(&mut self, rgid: i32, egid: i32, sgid: i32) -> Result<(), CredError> {
        if self.euid != 0 {
            if rgid != -1 {
                let rgid = rgid as u32;
                if rgid != self.gid && rgid != self.egid && rgid != self.sgid {
                    return Err(CredError::PermissionDenied);
                }
            }
            if egid != -1 {
                let egid = egid as u32;
                if egid != self.gid && egid != self.egid && egid != self.sgid {
                    return Err(CredError::PermissionDenied);
                }
            }
            if sgid != -1 {
                let sgid = sgid as u32;
                if sgid != self.gid && sgid != self.egid && sgid != self.sgid {
                    return Err(CredError::PermissionDenied);
                }
            }
        }

        if rgid != -1 {
            self.gid = rgid as u32;
        }
        if egid != -1 {
            self.egid = egid as u32;
            self.fsgid = egid as u32;
        }
        if sgid != -1 {
            self.sgid = sgid as u32;
        }

        Ok(())
    }

    /// Set supplementary groups
    pub fn setgroups(&mut self, groups: &[u32]) -> Result<(), CredError> {
        if self.euid != 0 {
            return Err(CredError::PermissionDenied);
        }
        self.groups = groups.to_vec();
        Ok(())
    }

    /// Check group membership
    pub fn in_group(&self, gid: u32) -> bool {
        self.egid == gid || self.groups.contains(&gid)
    }

    /// Set filesystem UID
    pub fn setfsuid(&mut self, fsuid: u32) -> u32 {
        let old = self.fsuid;
        if self.euid == 0 || fsuid == self.uid || fsuid == self.euid || fsuid == self.suid || fsuid == self.fsuid {
            self.fsuid = fsuid;
        }
        old
    }

    /// Set filesystem GID
    pub fn setfsgid(&mut self, fsgid: u32) -> u32 {
        let old = self.fsgid;
        if self.euid == 0 || fsgid == self.gid || fsgid == self.egid || fsgid == self.sgid || fsgid == self.fsgid {
            self.fsgid = fsgid;
        }
        old
    }

    /// Transform on exec
    pub fn exec_transform(&mut self, file_uid: u32, file_gid: u32, setuid: bool, setgid: bool) {
        if setuid {
            self.euid = file_uid;
            self.suid = file_uid;
            self.fsuid = file_uid;
        }

        if setgid {
            self.egid = file_gid;
            self.sgid = file_gid;
            self.fsgid = file_gid;
        }
    }
}

/// Credentials builder
pub struct CredentialsBuilder {
    creds: Credentials,
}

impl CredentialsBuilder {
    /// Create a builder with default (root) credentials
    pub fn new() -> Self {
        Self {
            creds: Credentials::root(),
        }
    }

    /// Set user ID
    pub fn uid(mut self, uid: u32) -> Self {
        self.creds.uid = uid;
        self.creds.euid = uid;
        self.creds.suid = uid;
        self.creds.fsuid = uid;
        self
    }

    /// Set group ID
    pub fn gid(mut self, gid: u32) -> Self {
        self.creds.gid = gid;
        self.creds.egid = gid;
        self.creds.sgid = gid;
        self.creds.fsgid = gid;
        self
    }

    /// Set effective user ID
    pub fn euid(mut self, euid: u32) -> Self {
        self.creds.euid = euid;
        self.creds.fsuid = euid;
        self
    }

    /// Set effective group ID
    pub fn egid(mut self, egid: u32) -> Self {
        self.creds.egid = egid;
        self.creds.fsgid = egid;
        self
    }

    /// Set supplementary groups
    pub fn groups(mut self, groups: Vec<u32>) -> Self {
        self.creds.groups = groups;
        self
    }

    /// Set user namespace
    pub fn user_ns(mut self, ns: u64) -> Self {
        self.creds.user_ns = ns;
        self
    }

    /// Build the credentials
    pub fn build(self) -> Credentials {
        self.creds
    }
}

impl Default for CredentialsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Credential errors
#[derive(Clone, Debug)]
pub enum CredError {
    /// Permission denied
    PermissionDenied,
    /// Invalid argument
    InvalidArgument,
}

impl CredError {
    /// Convert to errno
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::PermissionDenied => -1, // EPERM
            Self::InvalidArgument => -22, // EINVAL
        }
    }
}

/// Access permission bits
pub mod access {
    pub const R_OK: u32 = 4;
    pub const W_OK: u32 = 2;
    pub const X_OK: u32 = 1;
    pub const F_OK: u32 = 0;
}

/// Common user IDs
pub mod users {
    pub const ROOT: u32 = 0;
    pub const BIN: u32 = 1;
    pub const DAEMON: u32 = 2;
    pub const NOBODY: u32 = 65534;
}

/// Common group IDs
pub mod groups {
    pub const ROOT: u32 = 0;
    pub const BIN: u32 = 1;
    pub const DAEMON: u32 = 2;
    pub const WHEEL: u32 = 10;
    pub const NOGROUP: u32 = 65534;
}

/// Prepare credentials for a setuid binary
pub fn prepare_exec_creds(
    current: &Credentials,
    file_mode: u32,
    file_uid: u32,
    file_gid: u32,
) -> Credentials {
    let mut new_creds = current.clone();

    // Check setuid bit
    if file_mode & 0o4000 != 0 {
        new_creds.euid = file_uid;
        new_creds.suid = file_uid;
        new_creds.fsuid = file_uid;
    }

    // Check setgid bit
    if file_mode & 0o2000 != 0 {
        new_creds.egid = file_gid;
        new_creds.sgid = file_gid;
        new_creds.fsgid = file_gid;
    }

    new_creds
}

/// Check if credentials allow signal to target
pub fn can_signal(sender: &Credentials, target: &Credentials) -> bool {
    // Root can signal anyone
    if sender.euid == 0 {
        return true;
    }

    // Can signal own processes
    if sender.uid == target.uid || sender.euid == target.uid {
        return true;
    }

    false
}

/// Check if credentials allow ptrace of target
pub fn can_ptrace(tracer: &Credentials, tracee: &Credentials) -> bool {
    // Root can trace anyone
    if tracer.euid == 0 {
        return true;
    }

    // Can trace own processes with same credentials
    if tracer.uid == tracee.uid &&
       tracer.uid == tracee.suid &&
       tracer.gid == tracee.gid &&
       tracer.gid == tracee.sgid {
        return true;
    }

    false
}
