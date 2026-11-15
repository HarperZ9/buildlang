// ===============================================================================
// BUILDOS KERNEL - NETFILTER PACKET FILTERING FRAMEWORK
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Netfilter Packet Filtering Framework
//!
//! Linux-compatible netfilter implementation:
//! - Hook points (PREROUTING, INPUT, FORWARD, OUTPUT, POSTROUTING)
//! - Tables (filter, nat, mangle, raw)
//! - Chains and rules
//! - Match and target extensions
//! - Connection tracking (conntrack)
//! - Network Address Translation (NAT)

#![allow(dead_code)]

extern crate alloc;

pub mod tables;
pub mod chains;
pub mod rules;
pub mod matches;
pub mod targets;
pub mod conntrack;
pub mod nat;
pub mod nftables;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::sync::RwLock;

// =============================================================================
// NETFILTER HOOKS
// =============================================================================

/// Netfilter hook points
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u32)]
pub enum NfHook {
    /// Before routing decision (incoming packets)
    PreRouting = 0,
    /// After routing, destined for local delivery
    LocalIn = 1,
    /// After routing, destined for forwarding
    Forward = 2,
    /// Locally generated packets, before routing
    LocalOut = 3,
    /// After routing, about to be transmitted
    PostRouting = 4,
}

impl NfHook {
    /// Number of hook points
    pub const COUNT: usize = 5;

    /// Get hook name
    pub fn name(&self) -> &'static str {
        match self {
            NfHook::PreRouting => "PREROUTING",
            NfHook::LocalIn => "INPUT",
            NfHook::Forward => "FORWARD",
            NfHook::LocalOut => "OUTPUT",
            NfHook::PostRouting => "POSTROUTING",
        }
    }

    /// From index
    pub fn from_index(index: u32) -> Option<Self> {
        match index {
            0 => Some(NfHook::PreRouting),
            1 => Some(NfHook::LocalIn),
            2 => Some(NfHook::Forward),
            3 => Some(NfHook::LocalOut),
            4 => Some(NfHook::PostRouting),
            _ => None,
        }
    }
}

/// Protocol family
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum NfProto {
    /// Unspecified
    Unspec = 0,
    /// IPv4 (also known as Inet)
    Inet = 2,
    /// ARP
    Arp = 3,
    /// Netdev (ingress/egress)
    Netdev = 5,
    /// Bridge
    Bridge = 7,
    /// IPv6
    Ipv6 = 10,
    /// DECnet
    Decnet = 12,
}

impl NfProto {
    /// Ipv4 is an alias for Inet
    #[allow(non_upper_case_globals)]
    pub const Ipv4: NfProto = NfProto::Inet;
}

impl PartialOrd for NfProto {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for NfProto {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (*self as u32).cmp(&(*other as u32))
    }
}

/// Netfilter verdict
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum NfVerdict {
    /// Drop the packet
    Drop = 0,
    /// Accept the packet
    Accept = 1,
    /// Queue to userspace (deprecated)
    Stolen = 2,
    /// Packet consumed, don't continue
    Queue = 3,
    /// Repeat hook (reprocess)
    Repeat = 4,
    /// Stop processing (used internally)
    Stop = 5,
    /// Continue to next hook
    Continue = 0xFFFF,
}

// =============================================================================
// HOOK OPERATIONS
// =============================================================================

/// Hook operation function type
pub type NfHookFn = fn(&mut NfHookState) -> NfVerdict;

/// Hook registration priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct NfPriority(pub i32);

impl NfPriority {
    /// First priority (connection tracking)
    pub const FIRST: Self = Self(i32::MIN);
    /// Connection tracking (early)
    pub const CONNTRACK_DEFRAG: Self = Self(-400);
    /// Raw table
    pub const RAW: Self = Self(-300);
    /// SELinux first
    pub const SELINUX_FIRST: Self = Self(-225);
    /// Connection tracking
    pub const CONNTRACK: Self = Self(-200);
    /// Mangle table
    pub const MANGLE: Self = Self(-150);
    /// Destination NAT
    pub const NAT_DST: Self = Self(-100);
    /// Routing
    pub const ROUTING: Self = Self(-75);
    /// Filter table
    pub const FILTER: Self = Self(0);
    /// Security
    pub const SECURITY: Self = Self(50);
    /// Source NAT
    pub const NAT_SRC: Self = Self(100);
    /// SELinux last
    pub const SELINUX_LAST: Self = Self(225);
    /// Connection tracking confirm
    pub const CONNTRACK_CONFIRM: Self = Self(i32::MAX);
    /// Last priority
    pub const LAST: Self = Self(i32::MAX);
}

/// Registered hook operation
pub struct NfHookOps {
    /// Hook function
    pub hook: NfHookFn,
    /// Owner module
    pub owner: Option<&'static str>,
    /// Protocol family
    pub pf: NfProto,
    /// Hook point
    pub hooknum: NfHook,
    /// Priority
    pub priority: NfPriority,
}

// =============================================================================
// HOOK STATE
// =============================================================================

/// Packet information for hook processing
pub struct NfHookState {
    /// Hook point
    pub hook: NfHook,
    /// Protocol family
    pub pf: NfProto,
    /// Input device
    pub in_dev: Option<u32>,
    /// Output device
    pub out_dev: Option<u32>,
    /// Socket (if available)
    pub sk: Option<usize>,
    /// Network namespace
    pub net_ns: u32,
    /// Packet buffer (simplified)
    pub skb: SkBuff,
    /// Original destination address (for NAT)
    pub orig_dst: Option<u32>,
    /// Mark value
    pub mark: u32,
}

impl NfHookState {
    /// Create new hook state
    pub fn new(hook: NfHook, pf: NfProto, skb: SkBuff) -> Self {
        Self {
            hook,
            pf,
            in_dev: None,
            out_dev: None,
            sk: None,
            net_ns: 0,
            skb,
            orig_dst: None,
            mark: 0,
        }
    }
}

/// Simplified socket buffer
pub struct SkBuff {
    /// Packet data
    pub data: Vec<u8>,
    /// Data offset (start of current header)
    pub data_off: usize,
    /// Total length
    pub len: usize,
    /// L3 header offset (network layer)
    pub network_header: usize,
    /// L4 header offset (transport layer)
    pub transport_header: usize,
    /// Packet mark
    pub mark: u32,
    /// Priority
    pub priority: u32,
    /// Connection tracking entry
    pub nfct: Option<Arc<conntrack::ConntrackEntry>>,
    /// Connection tracking info
    pub nfctinfo: u8,
}

impl SkBuff {
    /// Create new socket buffer
    pub fn new(data: Vec<u8>) -> Self {
        let len = data.len();
        Self {
            data,
            data_off: 0,
            len,
            network_header: 0,
            transport_header: 0,
            mark: 0,
            priority: 0,
            nfct: None,
            nfctinfo: 0,
        }
    }

    /// Get IP header (assumes IPv4)
    pub fn ip_header(&self) -> Option<&[u8]> {
        if self.network_header + 20 <= self.data.len() {
            Some(&self.data[self.network_header..self.network_header + 20])
        } else {
            None
        }
    }

    /// Get source IP (IPv4)
    pub fn src_ip(&self) -> Option<u32> {
        let hdr = self.ip_header()?;
        Some(u32::from_be_bytes([hdr[12], hdr[13], hdr[14], hdr[15]]))
    }

    /// Get destination IP (IPv4)
    pub fn dst_ip(&self) -> Option<u32> {
        let hdr = self.ip_header()?;
        Some(u32::from_be_bytes([hdr[16], hdr[17], hdr[18], hdr[19]]))
    }

    /// Get IP protocol
    pub fn protocol(&self) -> Option<u8> {
        let hdr = self.ip_header()?;
        Some(hdr[9])
    }

    /// Get TCP/UDP source port
    pub fn src_port(&self) -> Option<u16> {
        if self.transport_header + 2 <= self.data.len() {
            Some(u16::from_be_bytes([
                self.data[self.transport_header],
                self.data[self.transport_header + 1],
            ]))
        } else {
            None
        }
    }

    /// Get TCP/UDP destination port
    pub fn dst_port(&self) -> Option<u16> {
        if self.transport_header + 4 <= self.data.len() {
            Some(u16::from_be_bytes([
                self.data[self.transport_header + 2],
                self.data[self.transport_header + 3],
            ]))
        } else {
            None
        }
    }
}

// =============================================================================
// NETFILTER CORE
// =============================================================================

/// Netfilter core state
pub struct Netfilter {
    /// Registered hooks per protocol family and hook point
    hooks: RwLock<BTreeMap<(NfProto, NfHook), Vec<Arc<NfHookOps>>>>,
    /// Tables
    tables: RwLock<BTreeMap<String, Arc<tables::Table>>>,
    /// Connection tracking
    conntrack: Arc<conntrack::ConntrackTable>,
    /// NAT subsystem
    nat: Arc<nat::NatTable>,
    /// Statistics
    stats: NetfilterStats,
    /// Enabled flag
    enabled: AtomicU32,
}

/// Netfilter statistics
#[derive(Default)]
pub struct NetfilterStats {
    /// Packets processed
    pub packets: AtomicU64,
    /// Bytes processed
    pub bytes: AtomicU64,
    /// Packets dropped
    pub dropped: AtomicU64,
    /// Packets accepted
    pub accepted: AtomicU64,
}

impl Netfilter {
    /// Create new netfilter instance
    pub fn new() -> Self {
        Self {
            hooks: RwLock::new(BTreeMap::new()),
            tables: RwLock::new(BTreeMap::new()),
            conntrack: Arc::new(conntrack::ConntrackTable::new()),
            nat: Arc::new(nat::NatTable::new()),
            stats: NetfilterStats::default(),
            enabled: AtomicU32::new(1),
        }
    }

    /// Register a hook
    pub fn register_hook(&self, ops: NfHookOps) -> Result<(), NfError> {
        let key = (ops.pf, ops.hooknum);
        let mut hooks = self.hooks.write();

        let list = hooks.entry(key).or_insert_with(Vec::new);

        // Insert in priority order
        let pos = list.iter().position(|h| h.priority > ops.priority);
        let arc_ops = Arc::new(ops);

        match pos {
            Some(idx) => list.insert(idx, arc_ops),
            None => list.push(arc_ops),
        }

        Ok(())
    }

    /// Unregister a hook
    pub fn unregister_hook(&self, pf: NfProto, hooknum: NfHook, hook: NfHookFn) -> bool {
        let key = (pf, hooknum);
        let mut hooks = self.hooks.write();

        if let Some(list) = hooks.get_mut(&key) {
            let len_before = list.len();
            list.retain(|h| h.hook as usize != hook as usize);
            return list.len() < len_before;
        }

        false
    }

    /// Run hooks for a packet
    pub fn hook_slow(&self, state: &mut NfHookState) -> NfVerdict {
        if self.enabled.load(Ordering::Relaxed) == 0 {
            return NfVerdict::Accept;
        }

        let key = (state.pf, state.hook);
        let hooks = self.hooks.read();

        let Some(list) = hooks.get(&key) else {
            return NfVerdict::Accept;
        };

        self.stats.packets.fetch_add(1, Ordering::Relaxed);
        self.stats.bytes.fetch_add(state.skb.len as u64, Ordering::Relaxed);

        for ops in list.iter() {
            let verdict = (ops.hook)(state);

            match verdict {
                NfVerdict::Accept | NfVerdict::Continue => continue,
                NfVerdict::Drop => {
                    self.stats.dropped.fetch_add(1, Ordering::Relaxed);
                    return NfVerdict::Drop;
                }
                _ => return verdict,
            }
        }

        self.stats.accepted.fetch_add(1, Ordering::Relaxed);
        NfVerdict::Accept
    }

    /// Add a table
    pub fn add_table(&self, table: tables::Table) -> Result<(), NfError> {
        let name = table.name.clone();
        let mut tables = self.tables.write();

        if tables.contains_key(&name) {
            return Err(NfError::TableExists);
        }

        tables.insert(name, Arc::new(table));
        Ok(())
    }

    /// Get a table by name
    pub fn get_table(&self, name: &str) -> Option<Arc<tables::Table>> {
        self.tables.read().get(name).cloned()
    }

    /// Remove a table
    pub fn remove_table(&self, name: &str) -> bool {
        self.tables.write().remove(name).is_some()
    }

    /// Get connection tracking table
    pub fn conntrack(&self) -> &Arc<conntrack::ConntrackTable> {
        &self.conntrack
    }

    /// Get NAT table
    pub fn nat(&self) -> &Arc<nat::NatTable> {
        &self.nat
    }

    /// Enable netfilter
    pub fn enable(&self) {
        self.enabled.store(1, Ordering::SeqCst);
    }

    /// Disable netfilter
    pub fn disable(&self) {
        self.enabled.store(0, Ordering::SeqCst);
    }

    /// Check if enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed) != 0
    }

    /// Get statistics
    pub fn stats(&self) -> &NetfilterStats {
        &self.stats
    }
}

impl Default for Netfilter {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// ERROR TYPES
// =============================================================================

/// Netfilter error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NfError {
    /// Invalid argument
    InvalidArg,
    /// Table already exists
    TableExists,
    /// Table not found
    TableNotFound,
    /// Chain already exists
    ChainExists,
    /// Chain not found
    ChainNotFound,
    /// Chain is not empty
    ChainNotEmpty,
    /// Rule not found
    RuleNotFound,
    /// No memory
    NoMemory,
    /// Permission denied
    PermissionDenied,
    /// Operation not supported
    NotSupported,
    /// Too many entries
    TooManyEntries,
    /// Invalid protocol
    InvalidProtocol,
    /// Loop detected
    LoopDetected,
}

// =============================================================================
// IPTABLES COMPATIBILITY
// =============================================================================

/// iptables command structure
#[derive(Debug, Clone)]
pub struct IptablesCmd {
    /// Table name
    pub table: String,
    /// Chain name
    pub chain: String,
    /// Command type
    pub cmd: IptablesCmdType,
    /// Rule specification
    pub rule: Option<rules::Rule>,
    /// Rule number (for insert/delete)
    pub rulenum: Option<u32>,
    /// New chain name (for rename)
    pub new_name: Option<String>,
    /// Policy (for setting chain policy)
    pub policy: Option<NfVerdict>,
}

/// iptables command type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IptablesCmdType {
    /// Append rule
    Append,
    /// Delete rule
    Delete,
    /// Insert rule
    Insert,
    /// Replace rule
    Replace,
    /// List rules
    List,
    /// Flush chain
    Flush,
    /// Zero counters
    Zero,
    /// New chain
    NewChain,
    /// Delete chain
    DeleteChain,
    /// Rename chain
    RenameChain,
    /// Set policy
    Policy,
    /// Check if rule exists
    Check,
}

/// Execute iptables command
pub fn iptables_cmd(nf: &Netfilter, cmd: &IptablesCmd) -> Result<(), NfError> {
    let table = nf.get_table(&cmd.table).ok_or(NfError::TableNotFound)?;

    match cmd.cmd {
        IptablesCmdType::Append => {
            let rule = cmd.rule.clone().ok_or(NfError::InvalidArg)?;
            table.append_rule(&cmd.chain, rule)
        }
        IptablesCmdType::Delete => {
            if let Some(num) = cmd.rulenum {
                table.delete_rule_by_num(&cmd.chain, num)
            } else {
                Err(NfError::InvalidArg)
            }
        }
        IptablesCmdType::Insert => {
            let rule = cmd.rule.clone().ok_or(NfError::InvalidArg)?;
            let num = cmd.rulenum.unwrap_or(1);
            table.insert_rule(&cmd.chain, num, rule)
        }
        IptablesCmdType::Flush => {
            table.flush_chain(&cmd.chain)
        }
        IptablesCmdType::NewChain => {
            table.create_chain(&cmd.chain)
        }
        IptablesCmdType::DeleteChain => {
            table.delete_chain(&cmd.chain)
        }
        IptablesCmdType::Policy => {
            let policy = cmd.policy.ok_or(NfError::InvalidArg)?;
            table.set_policy(&cmd.chain, policy)
        }
        IptablesCmdType::List | IptablesCmdType::Zero |
        IptablesCmdType::Replace | IptablesCmdType::RenameChain |
        IptablesCmdType::Check => {
            // These would need additional implementation
            Ok(())
        }
    }
}

// =============================================================================
// GLOBAL INSTANCE
// =============================================================================

use spin::Once;

static NETFILTER: Once<Netfilter> = Once::new();

/// Initialize netfilter
pub fn init() {
    NETFILTER.call_once(|| {
        let nf = Netfilter::new();

        // Create default tables
        let _ = nf.add_table(tables::Table::new_filter());
        let _ = nf.add_table(tables::Table::new_nat());
        let _ = nf.add_table(tables::Table::new_mangle());
        let _ = nf.add_table(tables::Table::new_raw());

        // Register core hooks
        register_core_hooks(&nf);

        nf
    });
}

/// Get global netfilter instance
pub fn get() -> &'static Netfilter {
    NETFILTER.get().expect("netfilter not initialized")
}

/// Register core netfilter hooks
fn register_core_hooks(nf: &Netfilter) {
    // Connection tracking hooks
    let _ = nf.register_hook(NfHookOps {
        hook: conntrack::hook_prerouting,
        owner: Some("nf_conntrack"),
        pf: NfProto::Inet,
        hooknum: NfHook::PreRouting,
        priority: NfPriority::CONNTRACK,
    });

    let _ = nf.register_hook(NfHookOps {
        hook: conntrack::hook_local_out,
        owner: Some("nf_conntrack"),
        pf: NfProto::Inet,
        hooknum: NfHook::LocalOut,
        priority: NfPriority::CONNTRACK,
    });

    let _ = nf.register_hook(NfHookOps {
        hook: conntrack::hook_confirm,
        owner: Some("nf_conntrack"),
        pf: NfProto::Inet,
        hooknum: NfHook::PostRouting,
        priority: NfPriority::CONNTRACK_CONFIRM,
    });

    let _ = nf.register_hook(NfHookOps {
        hook: conntrack::hook_confirm,
        owner: Some("nf_conntrack"),
        pf: NfProto::Inet,
        hooknum: NfHook::LocalIn,
        priority: NfPriority::CONNTRACK_CONFIRM,
    });

    // NAT hooks
    let _ = nf.register_hook(NfHookOps {
        hook: nat::hook_prerouting,
        owner: Some("nf_nat"),
        pf: NfProto::Inet,
        hooknum: NfHook::PreRouting,
        priority: NfPriority::NAT_DST,
    });

    let _ = nf.register_hook(NfHookOps {
        hook: nat::hook_postrouting,
        owner: Some("nf_nat"),
        pf: NfProto::Inet,
        hooknum: NfHook::PostRouting,
        priority: NfPriority::NAT_SRC,
    });

    let _ = nf.register_hook(NfHookOps {
        hook: nat::hook_local_out,
        owner: Some("nf_nat"),
        pf: NfProto::Inet,
        hooknum: NfHook::LocalOut,
        priority: NfPriority::NAT_DST,
    });

    let _ = nf.register_hook(NfHookOps {
        hook: nat::hook_local_in,
        owner: Some("nf_nat"),
        pf: NfProto::Inet,
        hooknum: NfHook::LocalIn,
        priority: NfPriority::NAT_SRC,
    });
}
