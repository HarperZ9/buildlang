//! Security Audit Subsystem
//!
//! Provides comprehensive audit logging for security events.

use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use crate::sync::Mutex;

/// Audit event types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum AuditEventType {
    // Kernel/Syscall events (1300-1399)
    Syscall = 1300,
    SyscallExit = 1301,
    Path = 1302,
    Ipc = 1303,
    Socketcall = 1304,
    ConfigChange = 1305,
    Sockaddr = 1306,
    Cwd = 1307,
    Execve = 1309,
    ExecveArg = 1310,
    Netfilter = 1325,
    Seccomp = 1326,
    Proctitle = 1327,
    FeatureChange = 1328,
    TimeAdj = 1333,
    BpfProgLoad = 1334,
    BpfMapCreate = 1335,

    // Login/logout events (1100-1199)
    UserAuth = 1100,
    UserAcct = 1101,
    UserMgmt = 1102,
    UserStart = 1103,
    UserEnd = 1104,
    UserLogin = 1112,
    UserLogout = 1113,

    // Access events (1400-1499)
    DacRead = 1400,
    DacWrite = 1401,
    DacExec = 1402,
    MacDecision = 1403,

    // Anomaly events (1700-1799)
    Anomaly = 1700,
    AnomalyLink = 1702,

    // Integrity events (1800-1899)
    IntegrityData = 1800,
    IntegrityMetadata = 1801,
    IntegrityStatus = 1802,
    IntegrityHash = 1803,
    IntegrityPcr = 1804,
    IntegrityRule = 1805,

    // Crypto events (2400-2499)
    CryptoKeyUser = 2400,
    CryptoLogin = 2401,
    CryptoLogout = 2402,
    CryptoKeyGen = 2403,
    CryptoSession = 2404,
}

#[allow(non_upper_case_globals)]
impl AuditEventType {
    /// KernelOther is an alias for Syscall
    pub const KernelOther: AuditEventType = AuditEventType::Syscall;
    /// Kernel is an alias for SyscallExit
    pub const Kernel: AuditEventType = AuditEventType::SyscallExit;
    /// Avc is an alias for DacRead
    pub const Avc: AuditEventType = AuditEventType::DacRead;
    /// FanotifyResponse is an alias for Netfilter
    pub const FanotifyResponse: AuditEventType = AuditEventType::Netfilter;
    /// TimeInjoffset is an alias for BpfProgLoad
    pub const TimeInjoffset: AuditEventType = AuditEventType::BpfProgLoad;
}

/// Audit event
#[derive(Clone, Debug)]
pub struct AuditEvent {
    /// Event type
    pub event_type: AuditEventType,
    /// Sequence number
    pub sequence: u64,
    /// Timestamp (nanoseconds since boot)
    pub timestamp: u64,
    /// Process ID
    pub pid: u32,
    /// User ID
    pub uid: u32,
    /// Audit ID (login UID)
    pub auid: u32,
    /// Session ID
    pub ses: u32,
    /// Subject context (SELinux label)
    pub subj: Option<String>,
    /// Object context
    pub obj: Option<String>,
    /// Result (0 = success, else errno)
    pub result: i32,
    /// Message text
    pub message: String,
    /// Additional fields
    pub fields: Vec<(String, String)>,
}

impl AuditEvent {
    /// Create a new audit event
    pub fn new(event_type: AuditEventType, message: String) -> Self {
        Self {
            event_type,
            sequence: NEXT_SEQUENCE.fetch_add(1, Ordering::SeqCst),
            timestamp: crate::time::now_ns(),
            pid: crate::process::current().map(|p| p.as_u64() as u32).unwrap_or(0),
            uid: crate::process::getuid(),
            auid: u32::MAX, // Not set
            ses: u32::MAX,  // Not set
            subj: None,
            obj: None,
            result: 0,
            message,
            fields: Vec::new(),
        }
    }

    /// Add a field
    pub fn add_field(&mut self, key: &str, value: &str) {
        self.fields.push((String::from(key), String::from(value)));
    }

    /// Set result
    pub fn with_result(mut self, result: i32) -> Self {
        self.result = result;
        self
    }

    /// Format as audit log line
    pub fn format(&self) -> String {
        use alloc::format;

        let mut line = format!(
            "type={:?} msg=audit({}.{}:{}): ",
            self.event_type,
            self.timestamp / 1_000_000_000,
            self.timestamp % 1_000_000_000,
            self.sequence
        );

        line.push_str(&format!("pid={} uid={} ", self.pid, self.uid));

        if self.auid != u32::MAX {
            line.push_str(&format!("auid={} ", self.auid));
        }

        if self.ses != u32::MAX {
            line.push_str(&format!("ses={} ", self.ses));
        }

        if let Some(ref subj) = self.subj {
            line.push_str(&format!("subj={} ", subj));
        }

        for (key, value) in &self.fields {
            line.push_str(&format!("{}={} ", key, value));
        }

        line.push_str(&format!("res={}", if self.result == 0 { "success" } else { "failed" }));

        line
    }
}

/// Audit log buffer
pub struct AuditLog {
    /// Event buffer
    events: VecDeque<AuditEvent>,
    /// Maximum events to keep
    max_events: usize,
    /// Total events logged
    total_logged: u64,
    /// Events dropped due to overflow
    dropped: u64,
}

impl AuditLog {
    /// Create a new audit log
    pub fn new(max_events: usize) -> Self {
        Self {
            events: VecDeque::with_capacity(max_events),
            max_events,
            total_logged: 0,
            dropped: 0,
        }
    }

    /// Log an event
    pub fn log(&mut self, event: AuditEvent) {
        if self.events.len() >= self.max_events {
            self.events.pop_front();
            self.dropped += 1;
        }
        self.events.push_back(event);
        self.total_logged += 1;
    }

    /// Get recent events
    pub fn recent(&self, count: usize) -> Vec<&AuditEvent> {
        self.events.iter().rev().take(count).collect()
    }

    /// Get all events
    pub fn all(&self) -> impl Iterator<Item = &AuditEvent> {
        self.events.iter()
    }

    /// Clear the log
    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// Get statistics
    pub fn stats(&self) -> AuditStats {
        AuditStats {
            total_logged: self.total_logged,
            dropped: self.dropped,
            buffer_size: self.events.len(),
            buffer_capacity: self.max_events,
        }
    }
}

/// Audit statistics
#[derive(Clone, Debug)]
pub struct AuditStats {
    pub total_logged: u64,
    pub dropped: u64,
    pub buffer_size: usize,
    pub buffer_capacity: usize,
}

/// Next event sequence number
static NEXT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Audit enabled flag
static AUDIT_ENABLED: AtomicBool = AtomicBool::new(true);

/// Global audit log
static AUDIT_LOG: Mutex<Option<AuditLog>> = Mutex::new(None);

/// Initialize audit subsystem
pub fn init() {
    *AUDIT_LOG.lock() = Some(AuditLog::new(10000));
}

/// Log an audit event
pub fn log(event: AuditEvent) {
    if !AUDIT_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    // Print to kernel log
    crate::kprintln!("[AUDIT] {}", event.format());

    // Store in buffer
    if let Some(ref mut log) = *AUDIT_LOG.lock() {
        log.log(event);
    }
}

/// Log a denial
pub fn log_denial(
    subject: &super::SecurityContext,
    object: &super::SecurityLabel,
    access: u32,
) {
    use alloc::format;

    let mut event = AuditEvent::new(
        AuditEventType::Avc,
        format!("avc: denied {{ {} }}", access_to_string(access)),
    );

    event.result = -1; // EPERM
    event.add_field("scontext", &format!("{:?}", subject.label));
    event.add_field("tcontext", &format!("{:?}", object));

    log(event);
}

/// Convert access bits to string
fn access_to_string(access: u32) -> &'static str {
    match access {
        0x1 => "execute",
        0x2 => "write",
        0x4 => "read",
        0x6 => "read write",
        0x5 => "read execute",
        0x7 => "read write execute",
        _ => "unknown",
    }
}

/// Log syscall entry
pub fn log_syscall(
    syscall_nr: u32,
    args: &[u64; 6],
    result: i64,
) {
    use alloc::format;

    let mut event = AuditEvent::new(
        AuditEventType::Syscall,
        format!("syscall={}", syscall_nr),
    );

    event.result = if result < 0 { result as i32 } else { 0 };
    event.add_field("a0", &format!("{:#x}", args[0]));
    event.add_field("a1", &format!("{:#x}", args[1]));
    event.add_field("a2", &format!("{:#x}", args[2]));
    event.add_field("a3", &format!("{:#x}", args[3]));

    log(event);
}

/// Log process execution
pub fn log_execve(path: &str, args: &[&str]) {
    use alloc::format;

    let mut event = AuditEvent::new(
        AuditEventType::Execve,
        format!("execve path=\"{}\"", path),
    );

    for (i, arg) in args.iter().enumerate() {
        event.add_field(&format!("a{}", i), arg);
    }

    log(event);
}

/// Log file open
pub fn log_file_open(path: &str, flags: u32, result: i32) {
    use alloc::format;

    let mut event = AuditEvent::new(
        AuditEventType::Path,
        format!("item=0 name=\"{}\"", path),
    );

    event.result = result;
    event.add_field("flags", &format!("{:#x}", flags));

    log(event);
}

/// Log user authentication
pub fn log_user_auth(user: &str, success: bool, service: &str) {
    use alloc::format;

    let mut event = AuditEvent::new(
        AuditEventType::UserAuth,
        format!("PAM: authentication {}", if success { "success" } else { "failure" }),
    );

    event.result = if success { 0 } else { -1 };
    event.add_field("acct", user);
    event.add_field("exe", service);

    log(event);
}

/// Log capability use
pub fn log_capability(cap: super::capabilities::Capability, granted: bool) {
    use alloc::format;

    let mut event = AuditEvent::new(
        AuditEventType::Avc,
        format!("capability {} {}", cap.name(), if granted { "granted" } else { "denied" }),
    );

    event.result = if granted { 0 } else { -1 };

    log(event);
}

/// Log seccomp action
pub fn log_seccomp(syscall_nr: u32, action: super::seccomp::SeccompAction) {
    use alloc::format;

    let event = AuditEvent::new(
        AuditEventType::Seccomp,
        format!("sig=0 syscall={} action={:?}", syscall_nr, action),
    );

    log(event);
}

/// Log network connection
pub fn log_network(
    action: &str,
    family: i32,
    addr: &[u8],
    port: u16,
    result: i32,
) {
    use alloc::format;

    let mut event = AuditEvent::new(
        AuditEventType::Sockaddr,
        format!("network {}", action),
    );

    event.result = result;
    event.add_field("family", &format!("{}", family));
    event.add_field("laddr", &format_addr(addr));
    event.add_field("lport", &format!("{}", port));

    log(event);
}

fn format_addr(addr: &[u8]) -> String {
    use alloc::format;

    if addr.len() == 4 {
        format!("{}.{}.{}.{}", addr[0], addr[1], addr[2], addr[3])
    } else if addr.len() == 16 {
        String::from("ipv6")
    } else {
        String::from("unknown")
    }
}

/// Audit rules
pub struct AuditRule {
    /// Rule flags
    pub flags: AuditRuleFlags,
    /// Action
    pub action: AuditAction,
    /// Field filters
    pub filters: Vec<AuditFilter>,
}

bitflags::bitflags! {
    /// Audit rule flags
    #[derive(Clone, Copy, Debug, Default)]
    pub struct AuditRuleFlags: u32 {
        /// Filter on task list
        const FILTER_TASK = 1;
        /// Filter on entry
        const FILTER_ENTRY = 2;
        /// Filter on exit
        const FILTER_EXIT = 4;
        /// Filter on user
        const FILTER_USER = 8;
        /// Exclude
        const FILTER_EXCLUDE = 32;
    }
}

/// Audit action
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditAction {
    /// Never audit
    Never,
    /// Possible audit
    Possible,
    /// Always audit
    Always,
}

/// Audit filter
#[derive(Clone, Debug)]
pub struct AuditFilter {
    /// Field type
    pub field: AuditField,
    /// Operator
    pub op: AuditOp,
    /// Value
    pub value: u64,
}

/// Audit field types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditField {
    Pid,
    Uid,
    Euid,
    Suid,
    Gid,
    Egid,
    Sgid,
    LoginUid,
    Pers,
    Arch,
    MsgType,
    Ppid,
    DevMajor,
    DevMinor,
    Inode,
    Exit,
    Success,
    Perm,
    FileType,
    ObjUid,
    ObjGid,
    ObjUser,
    ObjRole,
    ObjType,
    ObjLevel,
    Watch,
    Dir,
    FilterKey,
    Syscall,
    Exe,
}

/// Audit comparison operators
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditOp {
    /// Equal
    Equal,
    /// Not equal
    NotEqual,
    /// Less than
    LessThan,
    /// Less than or equal
    LessThanOrEqual,
    /// Greater than
    GreaterThan,
    /// Greater than or equal
    GreaterThanOrEqual,
    /// Bitmask
    BitMask,
    /// Bitmask test
    BitTest,
}

/// Audit rules manager
pub struct AuditRules {
    rules: Vec<AuditRule>,
}

impl AuditRules {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    pub fn add(&mut self, rule: AuditRule) {
        self.rules.push(rule);
    }

    pub fn delete(&mut self, index: usize) -> Option<AuditRule> {
        if index < self.rules.len() {
            Some(self.rules.remove(index))
        } else {
            None
        }
    }

    pub fn clear(&mut self) {
        self.rules.clear();
    }

    pub fn matches(&self, event: &AuditEvent) -> bool {
        for rule in &self.rules {
            if self.rule_matches(rule, event) {
                return rule.action == AuditAction::Always;
            }
        }
        false
    }

    fn rule_matches(&self, rule: &AuditRule, event: &AuditEvent) -> bool {
        for filter in &rule.filters {
            if !self.filter_matches(filter, event) {
                return false;
            }
        }
        true
    }

    fn filter_matches(&self, filter: &AuditFilter, event: &AuditEvent) -> bool {
        let value = match filter.field {
            AuditField::Pid => event.pid as u64,
            AuditField::Uid => event.uid as u64,
            AuditField::LoginUid => event.auid as u64,
            AuditField::MsgType => event.event_type as u64,
            _ => return true, // Unknown field matches
        };

        match filter.op {
            AuditOp::Equal => value == filter.value,
            AuditOp::NotEqual => value != filter.value,
            AuditOp::LessThan => value < filter.value,
            AuditOp::LessThanOrEqual => value <= filter.value,
            AuditOp::GreaterThan => value > filter.value,
            AuditOp::GreaterThanOrEqual => value >= filter.value,
            AuditOp::BitMask => (value & filter.value) != 0,
            AuditOp::BitTest => (value & filter.value) == filter.value,
        }
    }
}

/// Enable/disable audit
pub fn set_enabled(enabled: bool) {
    AUDIT_ENABLED.store(enabled, Ordering::Release);
}

/// Check if audit is enabled
pub fn is_enabled() -> bool {
    AUDIT_ENABLED.load(Ordering::Acquire)
}

/// Get audit statistics
pub fn stats() -> Option<AuditStats> {
    AUDIT_LOG.lock().as_ref().map(|log| log.stats())
}
