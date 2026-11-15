//! Linux-style Capabilities
//!
//! Provides fine-grained privilege control as an alternative to the
//! all-or-nothing root/non-root model.

use core::fmt;

/// Capability constants (matching Linux capabilities)
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Capability {
    /// Override permission checks (DAC)
    DacOverride = 1,
    /// Bypass file read permission checks
    DacReadSearch = 2,
    /// Override permission checks for file ownership
    Fowner = 3,
    /// Don't clear set-user-ID and set-group-ID mode bits on file modification
    Fsetid = 4,
    /// Bypass permission checks for sending signals
    Kill = 5,
    /// Set arbitrary process GIDs and supplementary GIDs
    Setgid = 6,
    /// Set arbitrary process UIDs
    Setuid = 7,
    /// Transfer capabilities to non-root processes
    Setpcap = 8,
    /// Set file capabilities
    SetFcap = 31,
    /// Bypass sticky bit for directories
    Chown = 0,
    /// Bind to privileged ports (<1024)
    NetBindService = 10,
    /// Broadcast and multicast
    NetBroadcast = 11,
    /// Perform network admin tasks
    NetAdmin = 12,
    /// Use raw sockets
    NetRaw = 13,
    /// Lock memory
    IpcLock = 14,
    /// Bypass SysV IPC permission checks
    IpcOwner = 15,
    /// Load kernel modules
    SysModule = 16,
    /// Raw I/O operations
    SysRawio = 17,
    /// Use chroot
    SysChroot = 18,
    /// Trace processes
    SysPtrace = 19,
    /// Configure accounting
    SysPacct = 20,
    /// System admin operations
    SysAdmin = 21,
    /// Use reboot/kexec
    SysBoot = 22,
    /// Set nice/priority
    SysNice = 23,
    /// Override resource limits
    SysResource = 24,
    /// Set system time
    SysTime = 25,
    /// Configure tty
    SysTtyConfig = 26,
    /// Create device special files
    Mknod = 27,
    /// Set file leases
    Lease = 28,
    /// Write audit log
    AuditWrite = 29,
    /// Configure audit
    AuditControl = 30,
    /// Immutable and append-only file attributes
    LinuxImmutable = 9,
    /// MAC admin
    MacAdmin = 33,
    /// MAC override
    MacOverride = 32,
    /// Create user namespaces
    SyslogRead = 34,
    /// Wake from suspend
    WakeAlarm = 35,
    /// Block device suspend
    BlockSuspend = 36,
    /// Audit read
    AuditRead = 37,
    /// Checkpoint/restore
    CheckpointRestore = 40,
}

impl Capability {
    /// Get capability from number
    pub fn from_u8(n: u8) -> Option<Self> {
        if n <= 40 {
            // Safety: all values 0-40 are valid capabilities
            Some(unsafe { core::mem::transmute(n) })
        } else {
            None
        }
    }

    /// Get capability number
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }

    /// Get capability name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Chown => "CAP_CHOWN",
            Self::DacOverride => "CAP_DAC_OVERRIDE",
            Self::DacReadSearch => "CAP_DAC_READ_SEARCH",
            Self::Fowner => "CAP_FOWNER",
            Self::Fsetid => "CAP_FSETID",
            Self::Kill => "CAP_KILL",
            Self::Setgid => "CAP_SETGID",
            Self::Setuid => "CAP_SETUID",
            Self::Setpcap => "CAP_SETPCAP",
            Self::LinuxImmutable => "CAP_LINUX_IMMUTABLE",
            Self::NetBindService => "CAP_NET_BIND_SERVICE",
            Self::NetBroadcast => "CAP_NET_BROADCAST",
            Self::NetAdmin => "CAP_NET_ADMIN",
            Self::NetRaw => "CAP_NET_RAW",
            Self::IpcLock => "CAP_IPC_LOCK",
            Self::IpcOwner => "CAP_IPC_OWNER",
            Self::SysModule => "CAP_SYS_MODULE",
            Self::SysRawio => "CAP_SYS_RAWIO",
            Self::SysChroot => "CAP_SYS_CHROOT",
            Self::SysPtrace => "CAP_SYS_PTRACE",
            Self::SysPacct => "CAP_SYS_PACCT",
            Self::SysAdmin => "CAP_SYS_ADMIN",
            Self::SysBoot => "CAP_SYS_BOOT",
            Self::SysNice => "CAP_SYS_NICE",
            Self::SetFcap => "CAP_SETFCAP",
            Self::SysResource => "CAP_SYS_RESOURCE",
            Self::SysTime => "CAP_SYS_TIME",
            Self::SysTtyConfig => "CAP_SYS_TTY_CONFIG",
            Self::Mknod => "CAP_MKNOD",
            Self::Lease => "CAP_LEASE",
            Self::AuditWrite => "CAP_AUDIT_WRITE",
            Self::AuditControl => "CAP_AUDIT_CONTROL",
            Self::MacOverride => "CAP_MAC_OVERRIDE",
            Self::MacAdmin => "CAP_MAC_ADMIN",
            Self::SyslogRead => "CAP_SYSLOG",
            Self::WakeAlarm => "CAP_WAKE_ALARM",
            Self::BlockSuspend => "CAP_BLOCK_SUSPEND",
            Self::AuditRead => "CAP_AUDIT_READ",
            Self::CheckpointRestore => "CAP_CHECKPOINT_RESTORE",
        }
    }
}

/// Last valid capability number
pub const CAP_LAST_CAP: u8 = 40;

/// Capability set (bitmask)
#[derive(Clone, Copy, Default)]
pub struct CapabilitySet {
    /// Lower 32 capabilities
    pub lo: u32,
    /// Upper 32 capabilities
    pub hi: u32,
}

impl CapabilitySet {
    /// Empty capability set
    pub const fn empty() -> Self {
        Self { lo: 0, hi: 0 }
    }

    /// Full capability set
    pub const fn full() -> Self {
        Self {
            lo: 0xFFFFFFFF,
            hi: (1 << (CAP_LAST_CAP - 32 + 1)) - 1,
        }
    }

    /// Create set with single capability
    pub fn single(cap: Capability) -> Self {
        let mut set = Self::empty();
        set.add(cap);
        set
    }

    /// Add a capability
    pub fn add(&mut self, cap: Capability) {
        let bit = cap.as_u8();
        if bit < 32 {
            self.lo |= 1 << bit;
        } else {
            self.hi |= 1 << (bit - 32);
        }
    }

    /// Remove a capability
    pub fn remove(&mut self, cap: Capability) {
        let bit = cap.as_u8();
        if bit < 32 {
            self.lo &= !(1 << bit);
        } else {
            self.hi &= !(1 << (bit - 32));
        }
    }

    /// Check if capability is present
    pub fn contains(&self, cap: Capability) -> bool {
        let bit = cap.as_u8();
        if bit < 32 {
            (self.lo & (1 << bit)) != 0
        } else {
            (self.hi & (1 << (bit - 32))) != 0
        }
    }

    /// Check if set is empty
    pub fn is_empty(&self) -> bool {
        self.lo == 0 && self.hi == 0
    }

    /// Check if set is full
    pub fn is_full(&self) -> bool {
        let full = Self::full();
        self.lo == full.lo && self.hi == full.hi
    }

    /// Union of two sets
    pub fn union(&self, other: &Self) -> Self {
        Self {
            lo: self.lo | other.lo,
            hi: self.hi | other.hi,
        }
    }

    /// Intersection of two sets
    pub fn intersect(&self, other: &Self) -> Self {
        Self {
            lo: self.lo & other.lo,
            hi: self.hi & other.hi,
        }
    }

    /// Difference (self - other)
    pub fn difference(&self, other: &Self) -> Self {
        Self {
            lo: self.lo & !other.lo,
            hi: self.hi & !other.hi,
        }
    }

    /// Clear all capabilities
    pub fn clear(&mut self) {
        self.lo = 0;
        self.hi = 0;
    }

    /// Iterate over contained capabilities
    pub fn iter(&self) -> CapabilitySetIter {
        CapabilitySetIter {
            set: *self,
            pos: 0,
        }
    }

    /// Count capabilities in set
    pub fn count(&self) -> u32 {
        self.lo.count_ones() + self.hi.count_ones()
    }
}

impl fmt::Debug for CapabilitySet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut list = f.debug_set();
        for cap in self.iter() {
            list.entry(&cap.name());
        }
        list.finish()
    }
}

/// Iterator over capabilities in a set
pub struct CapabilitySetIter {
    set: CapabilitySet,
    pos: u8,
}

impl Iterator for CapabilitySetIter {
    type Item = Capability;

    fn next(&mut self) -> Option<Self::Item> {
        while self.pos <= CAP_LAST_CAP {
            if let Some(cap) = Capability::from_u8(self.pos) {
                self.pos += 1;
                if self.set.contains(cap) {
                    return Some(cap);
                }
            } else {
                self.pos += 1;
            }
        }
        None
    }
}

/// Capability bounding set operations
pub struct CapBound;

impl CapBound {
    /// Drop a capability from the bounding set
    pub fn drop(_cap: Capability) -> Result<(), CapError> {
        // Would modify current process bounding set
        Ok(())
    }

    /// Read a capability from the bounding set
    pub fn read(_cap: Capability) -> bool {
        // Would check current process bounding set
        true
    }
}

/// Capability header for syscalls
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CapUserHeader {
    /// Version
    pub version: u32,
    /// Process ID
    pub pid: i32,
}

/// Capability data for syscalls
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CapUserData {
    /// Effective caps
    pub effective: u32,
    /// Permitted caps
    pub permitted: u32,
    /// Inheritable caps
    pub inheritable: u32,
}

/// Capability version constants
pub const LINUX_CAPABILITY_VERSION_1: u32 = 0x19980330;
pub const LINUX_CAPABILITY_VERSION_2: u32 = 0x20071026;
pub const LINUX_CAPABILITY_VERSION_3: u32 = 0x20080522;

/// Capability errors
#[derive(Clone, Debug)]
pub enum CapError {
    /// Permission denied
    PermissionDenied,
    /// Invalid capability
    InvalidCapability,
    /// Invalid argument
    InvalidArgument,
}

impl CapError {
    /// Convert to errno
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::PermissionDenied => -1,   // EPERM
            Self::InvalidCapability => -22, // EINVAL
            Self::InvalidArgument => -22,   // EINVAL
        }
    }
}

/// Get process capabilities
pub fn capget(_header: &CapUserHeader, data: &mut [CapUserData; 2]) -> Result<(), CapError> {
    // Would get capabilities for pid in header
    data[0] = CapUserData {
        effective: 0xFFFFFFFF,
        permitted: 0xFFFFFFFF,
        inheritable: 0,
    };
    data[1] = CapUserData {
        effective: 0xFF,
        permitted: 0xFF,
        inheritable: 0,
    };
    Ok(())
}

/// Set process capabilities
pub fn capset(_header: &CapUserHeader, _data: &[CapUserData; 2]) -> Result<(), CapError> {
    // Would set capabilities for pid in header
    // Requires CAP_SETPCAP for most operations
    Ok(())
}

/// Check if process has capability in effective set
pub fn capable(_cap: Capability) -> bool {
    // Would check current process
    true
}

/// Check if process has capability relative to a user namespace
pub fn ns_capable(cap: Capability, _ns: u64) -> bool {
    // Would check capability in namespace context
    capable(cap)
}

/// Required capability for a filesystem operation
pub fn required_cap_for_fs_op(op: FsCapOp) -> Option<Capability> {
    match op {
        FsCapOp::Chown => Some(Capability::Chown),
        FsCapOp::Chmod => None, // Only needs file owner
        FsCapOp::SetXattr => Some(Capability::Fowner),
        FsCapOp::Mknod => Some(Capability::Mknod),
        FsCapOp::Mount => Some(Capability::SysAdmin),
        FsCapOp::Unmount => Some(Capability::SysAdmin),
        FsCapOp::Chroot => Some(Capability::SysChroot),
        FsCapOp::SetImmutable => Some(Capability::LinuxImmutable),
    }
}

/// Filesystem capability operations
#[derive(Clone, Copy, Debug)]
pub enum FsCapOp {
    Chown,
    Chmod,
    SetXattr,
    Mknod,
    Mount,
    Unmount,
    Chroot,
    SetImmutable,
}

/// Initialize capabilities subsystem
pub fn init() {
    // Nothing to initialize - capabilities are per-process
}

/// Ambient capability operations
pub mod ambient {
    use super::*;

    /// Raise an ambient capability
    pub fn raise(_cap: Capability) -> Result<(), CapError> {
        // Would check cap is in permitted & inheritable
        Ok(())
    }

    /// Lower an ambient capability
    pub fn lower(_cap: Capability) -> Result<(), CapError> {
        Ok(())
    }

    /// Clear all ambient capabilities
    pub fn clear() -> Result<(), CapError> {
        Ok(())
    }

    /// Check if ambient capability is set
    pub fn is_set(_cap: Capability) -> bool {
        false
    }
}

/// Secure bits for capability behavior
pub mod securebits {
    /// Keep capabilities on setuid
    pub const KEEP_CAPS: u32 = 1 << 4;
    /// Lock keep caps
    pub const KEEP_CAPS_LOCKED: u32 = 1 << 5;
    /// No setuid fixup
    pub const NO_SETUID_FIXUP: u32 = 1 << 2;
    /// Lock no setuid fixup
    pub const NO_SETUID_FIXUP_LOCKED: u32 = 1 << 3;
    /// No root (root has no special caps)
    pub const NOROOT: u32 = 1 << 0;
    /// Lock no root
    pub const NOROOT_LOCKED: u32 = 1 << 1;
    /// No ambient raise
    pub const NO_CAP_AMBIENT_RAISE: u32 = 1 << 6;
    /// Lock no ambient raise
    pub const NO_CAP_AMBIENT_RAISE_LOCKED: u32 = 1 << 7;

    /// Get current secure bits
    pub fn get() -> u32 {
        0
    }

    /// Set secure bits
    pub fn set(_bits: u32) -> Result<(), super::CapError> {
        // Would require CAP_SETPCAP
        Ok(())
    }
}

/// File capability xattr format
pub mod file_caps {
    use super::*;

    /// Magic number for capability xattr
    pub const VFS_CAP_REVISION_1: u32 = 0x01000000;
    pub const VFS_CAP_REVISION_2: u32 = 0x02000000;
    pub const VFS_CAP_REVISION_3: u32 = 0x03000000;

    /// File capability header
    #[repr(C)]
    pub struct VfsCap {
        pub magic_etc: u32,
        pub data: [VfsCapData; 2],
        pub rootid: u32, // Only in v3
    }

    /// File capability data
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct VfsCapData {
        pub permitted: u32,
        pub inheritable: u32,
    }

    /// Get file capabilities
    pub fn get(_path: &str) -> Option<super::super::FileCaps> {
        None
    }

    /// Set file capabilities
    pub fn set(_path: &str, _caps: &super::super::FileCaps) -> Result<(), CapError> {
        // Would require CAP_SETFCAP
        Ok(())
    }

    /// Remove file capabilities
    pub fn remove(_path: &str) -> Result<(), CapError> {
        Ok(())
    }
}
