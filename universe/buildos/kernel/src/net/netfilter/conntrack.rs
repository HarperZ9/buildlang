// ===============================================================================
// BUILDOS KERNEL - NETFILTER CONNECTION TRACKING
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Connection Tracking (conntrack)
//!
//! Stateful packet inspection and connection tracking:
//! - Track TCP/UDP/ICMP connections
//! - Connection state management
//! - Timeout handling
//! - Expectation support (for FTP, SIP, etc.)

#![allow(dead_code)]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use super::{NfHookState, NfVerdict};
use crate::sync::{Mutex, RwLock};

// =============================================================================
// CONNECTION STATE
// =============================================================================

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnState {
    /// No connection
    None,
    /// New connection
    New,
    /// Established connection
    Established,
    /// Related connection
    Related,
    /// Related reply
    RelatedReply,
    /// Time wait
    TimeWait,
    /// Close wait
    CloseWait,
    /// Last ACK
    LastAck,
    /// SYN sent
    SynSent,
    /// SYN received
    SynRecv,
    /// FIN wait 1
    FinWait1,
    /// FIN wait 2
    FinWait2,
    /// Closing
    Closing,
}

/// Connection status flags
#[derive(Debug, Clone, Copy, Default)]
pub struct ConnStatus {
    /// Inner flags
    flags: u32,
}

impl ConnStatus {
    /// Expected connection
    pub const EXPECTED: u32 = 1 << 0;
    /// Seen reply
    pub const SEEN_REPLY: u32 = 1 << 1;
    /// Assured (seen traffic both ways)
    pub const ASSURED: u32 = 1 << 2;
    /// Confirmed
    pub const CONFIRMED: u32 = 1 << 3;
    /// Source NAT performed
    pub const SRC_NAT: u32 = 1 << 4;
    /// Destination NAT performed
    pub const DST_NAT: u32 = 1 << 5;
    /// Sequence number adjusted
    pub const SEQ_ADJUST: u32 = 1 << 6;
    /// Source NAT done
    pub const SRC_NAT_DONE: u32 = 1 << 7;
    /// Destination NAT done
    pub const DST_NAT_DONE: u32 = 1 << 8;
    /// Dying (being deleted)
    pub const DYING: u32 = 1 << 9;
    /// Fixed timeout
    pub const FIXED_TIMEOUT: u32 = 1 << 10;
    /// Template (not a real connection)
    pub const TEMPLATE: u32 = 1 << 11;
    /// Untracked
    pub const UNTRACKED: u32 = 1 << 12;
    /// Helper assigned
    pub const HELPER: u32 = 1 << 13;
    /// Offloaded
    pub const OFFLOAD: u32 = 1 << 14;

    /// Check flag
    pub fn has(&self, flag: u32) -> bool {
        (self.flags & flag) != 0
    }

    /// Set flag
    pub fn set(&mut self, flag: u32) {
        self.flags |= flag;
    }

    /// Clear flag
    pub fn clear(&mut self, flag: u32) {
        self.flags &= !flag;
    }
}

// =============================================================================
// CONNECTION TUPLE
// =============================================================================

/// Connection tuple (uniquely identifies a connection)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnTuple {
    /// Source address
    pub src_addr: u32,
    /// Destination address
    pub dst_addr: u32,
    /// Source port/ID
    pub src_port: u16,
    /// Destination port/ID
    pub dst_port: u16,
    /// Protocol
    pub protocol: u8,
    /// Direction (0 = original, 1 = reply)
    pub direction: u8,
}

impl ConnTuple {
    /// Create new tuple
    pub fn new(
        src_addr: u32,
        dst_addr: u32,
        src_port: u16,
        dst_port: u16,
        protocol: u8,
    ) -> Self {
        Self {
            src_addr,
            dst_addr,
            src_port,
            dst_port,
            protocol,
            direction: 0,
        }
    }

    /// Create reply tuple (swap src/dst)
    pub fn reply(&self) -> Self {
        Self {
            src_addr: self.dst_addr,
            dst_addr: self.src_addr,
            src_port: self.dst_port,
            dst_port: self.src_port,
            protocol: self.protocol,
            direction: 1 - self.direction,
        }
    }

    /// Extract tuple from packet
    pub fn from_skb(skb: &super::SkBuff) -> Option<Self> {
        let src_addr = skb.src_ip()?;
        let dst_addr = skb.dst_ip()?;
        let protocol = skb.protocol()?;
        let src_port = skb.src_port().unwrap_or(0);
        let dst_port = skb.dst_port().unwrap_or(0);

        Some(Self::new(src_addr, dst_addr, src_port, dst_port, protocol))
    }

    /// Hash key for lookup
    pub fn hash_key(&self) -> u64 {
        let mut h: u64 = 0;
        h ^= self.src_addr as u64;
        h ^= (self.dst_addr as u64) << 32;
        h ^= (self.src_port as u64) << 16;
        h ^= self.dst_port as u64;
        h ^= (self.protocol as u64) << 48;
        h
    }
}

// =============================================================================
// CONNECTION ENTRY
// =============================================================================

/// Connection tracking entry
pub struct ConntrackEntry {
    /// Original tuple
    pub original: ConnTuple,
    /// Reply tuple
    pub reply: ConnTuple,
    /// Connection state
    state: AtomicU32,
    /// Status flags
    status: Mutex<ConnStatus>,
    /// Timeout (absolute time)
    timeout: AtomicU64,
    /// Mark
    pub mark: AtomicU32,
    /// Counters (original direction)
    pub orig_packets: AtomicU64,
    pub orig_bytes: AtomicU64,
    /// Counters (reply direction)
    pub reply_packets: AtomicU64,
    pub reply_bytes: AtomicU64,
    /// Reference count
    refcount: AtomicU32,
    /// Zone ID
    pub zone: u16,
    /// NAT info
    nat_info: Mutex<Option<NatInfo>>,
}

/// NAT information
#[derive(Debug, Clone)]
pub struct NatInfo {
    /// Original source (before SNAT)
    pub orig_src: u32,
    /// Original source port
    pub orig_src_port: u16,
    /// Original destination (before DNAT)
    pub orig_dst: u32,
    /// Original destination port
    pub orig_dst_port: u16,
}

impl ConntrackEntry {
    /// Create new entry
    pub fn new(original: ConnTuple) -> Self {
        Self {
            reply: original.reply(),
            original,
            state: AtomicU32::new(ConnState::New as u32),
            status: Mutex::new(ConnStatus::default()),
            timeout: AtomicU64::new(0),
            mark: AtomicU32::new(0),
            orig_packets: AtomicU64::new(0),
            orig_bytes: AtomicU64::new(0),
            reply_packets: AtomicU64::new(0),
            reply_bytes: AtomicU64::new(0),
            refcount: AtomicU32::new(1),
            zone: 0,
            nat_info: Mutex::new(None),
        }
    }

    /// Get connection state
    pub fn state(&self) -> ConnState {
        let val = self.state.load(Ordering::SeqCst);
        match val {
            0 => ConnState::None,
            1 => ConnState::New,
            2 => ConnState::Established,
            3 => ConnState::Related,
            4 => ConnState::RelatedReply,
            5 => ConnState::TimeWait,
            6 => ConnState::CloseWait,
            7 => ConnState::LastAck,
            8 => ConnState::SynSent,
            9 => ConnState::SynRecv,
            10 => ConnState::FinWait1,
            11 => ConnState::FinWait2,
            12 => ConnState::Closing,
            _ => ConnState::None,
        }
    }

    /// Set connection state
    pub fn set_state(&self, state: ConnState) {
        self.state.store(state as u32, Ordering::SeqCst);
    }

    /// Check if confirmed
    pub fn is_confirmed(&self) -> bool {
        self.status.lock().has(ConnStatus::CONFIRMED)
    }

    /// Mark as confirmed
    pub fn confirm(&self) {
        self.status.lock().set(ConnStatus::CONFIRMED);
    }

    /// Check if dying
    pub fn is_dying(&self) -> bool {
        self.status.lock().has(ConnStatus::DYING)
    }

    /// Mark as dying
    pub fn set_dying(&self) {
        self.status.lock().set(ConnStatus::DYING);
    }

    /// Check if seen reply
    pub fn seen_reply(&self) -> bool {
        self.status.lock().has(ConnStatus::SEEN_REPLY)
    }

    /// Mark reply seen
    pub fn set_seen_reply(&self) {
        self.status.lock().set(ConnStatus::SEEN_REPLY);
    }

    /// Check if assured
    pub fn is_assured(&self) -> bool {
        self.status.lock().has(ConnStatus::ASSURED)
    }

    /// Mark as assured
    pub fn set_assured(&self) {
        self.status.lock().set(ConnStatus::ASSURED);
    }

    /// Update counters
    pub fn update_counters(&self, bytes: u64, is_reply: bool) {
        if is_reply {
            self.reply_packets.fetch_add(1, Ordering::Relaxed);
            self.reply_bytes.fetch_add(bytes, Ordering::Relaxed);
        } else {
            self.orig_packets.fetch_add(1, Ordering::Relaxed);
            self.orig_bytes.fetch_add(bytes, Ordering::Relaxed);
        }
    }

    /// Set timeout
    pub fn set_timeout(&self, timeout: u64) {
        self.timeout.store(timeout, Ordering::SeqCst);
    }

    /// Check if expired
    pub fn is_expired(&self, now: u64) -> bool {
        now >= self.timeout.load(Ordering::SeqCst)
    }

    /// Increment reference count
    pub fn get(&self) {
        self.refcount.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrement reference count
    pub fn put(&self) -> bool {
        self.refcount.fetch_sub(1, Ordering::SeqCst) == 1
    }

    /// Set NAT info
    pub fn set_nat_info(&self, info: NatInfo) {
        *self.nat_info.lock() = Some(info);
    }

    /// Get NAT info
    pub fn nat_info(&self) -> Option<NatInfo> {
        self.nat_info.lock().clone()
    }
}

// =============================================================================
// CONNECTION TABLE
// =============================================================================

/// Connection tracking table
pub struct ConntrackTable {
    /// Connections by original tuple
    by_original: RwLock<BTreeMap<u64, Arc<ConntrackEntry>>>,
    /// Connections by reply tuple
    by_reply: RwLock<BTreeMap<u64, Arc<ConntrackEntry>>>,
    /// Expectations
    expectations: RwLock<Vec<Expectation>>,
    /// Statistics
    stats: ConntrackStats,
    /// Configuration
    config: ConntrackConfig,
    /// Next garbage collection time
    next_gc: AtomicU64,
}

/// Connection tracking statistics
#[derive(Default)]
pub struct ConntrackStats {
    /// Current entries
    pub entries: AtomicU32,
    /// Searches performed
    pub searches: AtomicU64,
    /// Search hits
    pub found: AtomicU64,
    /// New connections
    pub new: AtomicU64,
    /// Connections confirmed
    pub confirmed: AtomicU64,
    /// Connections deleted
    pub deleted: AtomicU64,
    /// Invalid packets
    pub invalid: AtomicU64,
    /// Table full drops
    pub dropped: AtomicU64,
    /// Early drops (under memory pressure)
    pub early_drop: AtomicU64,
    /// ICMP errors
    pub icmp_error: AtomicU64,
    /// Expectation matches
    pub expect_new: AtomicU64,
    /// Expectation deletions
    pub expect_delete: AtomicU64,
}

/// Connection tracking configuration
#[derive(Clone)]
pub struct ConntrackConfig {
    /// Maximum entries
    pub max_entries: u32,
    /// Hash table size
    pub hash_size: u32,
    /// Generic timeout
    pub generic_timeout: u64,
    /// TCP timeouts
    pub tcp_timeout_established: u64,
    pub tcp_timeout_syn_sent: u64,
    pub tcp_timeout_syn_recv: u64,
    pub tcp_timeout_fin_wait: u64,
    pub tcp_timeout_close_wait: u64,
    pub tcp_timeout_last_ack: u64,
    pub tcp_timeout_time_wait: u64,
    pub tcp_timeout_close: u64,
    /// UDP timeout
    pub udp_timeout: u64,
    pub udp_timeout_stream: u64,
    /// ICMP timeout
    pub icmp_timeout: u64,
    /// Enable TCP liberal mode
    pub tcp_liberal: bool,
    /// Enable loose tracking
    pub tcp_loose: bool,
}

impl Default for ConntrackConfig {
    fn default() -> Self {
        Self {
            max_entries: 65536,
            hash_size: 16384,
            generic_timeout: 600,
            tcp_timeout_established: 432000, // 5 days
            tcp_timeout_syn_sent: 120,
            tcp_timeout_syn_recv: 60,
            tcp_timeout_fin_wait: 120,
            tcp_timeout_close_wait: 60,
            tcp_timeout_last_ack: 30,
            tcp_timeout_time_wait: 120,
            tcp_timeout_close: 10,
            udp_timeout: 30,
            udp_timeout_stream: 180,
            icmp_timeout: 30,
            tcp_liberal: false,
            tcp_loose: true,
        }
    }
}

impl ConntrackTable {
    /// Create new connection tracking table
    pub fn new() -> Self {
        Self {
            by_original: RwLock::new(BTreeMap::new()),
            by_reply: RwLock::new(BTreeMap::new()),
            expectations: RwLock::new(Vec::new()),
            stats: ConntrackStats::default(),
            config: ConntrackConfig::default(),
            next_gc: AtomicU64::new(0),
        }
    }

    /// Lookup connection by tuple
    pub fn lookup(&self, tuple: &ConnTuple) -> Option<Arc<ConntrackEntry>> {
        self.stats.searches.fetch_add(1, Ordering::Relaxed);

        let key = tuple.hash_key();

        // Try original direction
        if let Some(ct) = self.by_original.read().get(&key) {
            self.stats.found.fetch_add(1, Ordering::Relaxed);
            return Some(ct.clone());
        }

        // Try reply direction
        if let Some(ct) = self.by_reply.read().get(&key) {
            self.stats.found.fetch_add(1, Ordering::Relaxed);
            return Some(ct.clone());
        }

        None
    }

    /// Create new connection
    pub fn create(&self, tuple: ConnTuple) -> Option<Arc<ConntrackEntry>> {
        // Check limits
        let count = self.stats.entries.load(Ordering::Relaxed);
        if count >= self.config.max_entries {
            self.stats.dropped.fetch_add(1, Ordering::Relaxed);
            return None;
        }

        let ct = Arc::new(ConntrackEntry::new(tuple));

        self.stats.new.fetch_add(1, Ordering::Relaxed);
        self.stats.entries.fetch_add(1, Ordering::Relaxed);

        Some(ct)
    }

    /// Confirm connection (add to table)
    pub fn confirm(&self, ct: &Arc<ConntrackEntry>) {
        if ct.is_confirmed() {
            return;
        }

        ct.confirm();

        let orig_key = ct.original.hash_key();
        let reply_key = ct.reply.hash_key();

        self.by_original.write().insert(orig_key, ct.clone());
        self.by_reply.write().insert(reply_key, ct.clone());

        self.stats.confirmed.fetch_add(1, Ordering::Relaxed);
    }

    /// Delete connection
    pub fn delete(&self, ct: &Arc<ConntrackEntry>) {
        if ct.is_dying() {
            return;
        }

        ct.set_dying();

        let orig_key = ct.original.hash_key();
        let reply_key = ct.reply.hash_key();

        self.by_original.write().remove(&orig_key);
        self.by_reply.write().remove(&reply_key);

        self.stats.deleted.fetch_add(1, Ordering::Relaxed);
        self.stats.entries.fetch_sub(1, Ordering::Relaxed);
    }

    /// Process packet
    pub fn process(&self, state: &mut NfHookState) -> NfVerdict {
        let Some(tuple) = ConnTuple::from_skb(&state.skb) else {
            self.stats.invalid.fetch_add(1, Ordering::Relaxed);
            return NfVerdict::Accept;
        };

        // Check for existing connection
        if let Some(ct) = self.lookup(&tuple) {
            // Existing connection
            let is_reply = tuple == ct.reply;
            ct.update_counters(state.skb.len as u64, is_reply);

            if is_reply && !ct.seen_reply() {
                ct.set_seen_reply();
                ct.set_state(ConnState::Established);
            }

            state.skb.nfct = Some(ct);
            state.skb.nfctinfo = if is_reply { 1 } else { 0 };
        } else {
            // Check expectations
            if let Some(exp) = self.check_expectation(&tuple) {
                if let Some(ct) = self.create(tuple) {
                    ct.set_state(ConnState::Related);
                    state.skb.nfct = Some(ct);
                    self.stats.expect_new.fetch_add(1, Ordering::Relaxed);
                }
                let _ = exp;
            } else {
                // New connection
                if let Some(ct) = self.create(tuple) {
                    state.skb.nfct = Some(ct);
                }
            }
        }

        NfVerdict::Accept
    }

    /// Check expectation
    fn check_expectation(&self, tuple: &ConnTuple) -> Option<Expectation> {
        let expectations = self.expectations.read();

        for exp in expectations.iter() {
            if exp.matches(tuple) {
                return Some(exp.clone());
            }
        }

        None
    }

    /// Add expectation
    pub fn expect(&self, exp: Expectation) {
        self.expectations.write().push(exp);
    }

    /// Remove expectation
    pub fn unexpect(&self, tuple: &ConnTuple) {
        self.expectations.write().retain(|e| &e.tuple != tuple);
        self.stats.expect_delete.fetch_add(1, Ordering::Relaxed);
    }

    /// Garbage collect expired entries
    pub fn gc(&self, now: u64) {
        let next = self.next_gc.load(Ordering::Relaxed);
        if now < next {
            return;
        }

        self.next_gc.store(now + 10, Ordering::Relaxed);

        // Collect expired entries
        let mut to_delete = Vec::new();

        for ct in self.by_original.read().values() {
            if ct.is_expired(now) && !ct.is_dying() {
                to_delete.push(ct.clone());
            }
        }

        for ct in to_delete {
            self.delete(&ct);
        }

        // Clean up expectations
        self.expectations.write().retain(|e| !e.is_expired(now));
    }

    /// Flush all connections
    pub fn flush(&self) {
        self.by_original.write().clear();
        self.by_reply.write().clear();
        self.expectations.write().clear();
        self.stats.entries.store(0, Ordering::SeqCst);
    }

    /// Get statistics
    pub fn stats(&self) -> &ConntrackStats {
        &self.stats
    }

    /// Get configuration
    pub fn config(&self) -> &ConntrackConfig {
        &self.config
    }

    /// Get entry count
    pub fn count(&self) -> u32 {
        self.stats.entries.load(Ordering::Relaxed)
    }
}

impl Default for ConntrackTable {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// EXPECTATION
// =============================================================================

/// Connection expectation (for related connections)
#[derive(Debug, Clone)]
pub struct Expectation {
    /// Expected tuple
    pub tuple: ConnTuple,
    /// Mask tuple
    pub mask: ConnTuple,
    /// Master connection
    pub master: ConnTuple,
    /// Timeout
    pub timeout: u64,
    /// Flags
    pub flags: u32,
    /// Helper name
    pub helper: Option<&'static str>,
}

impl Expectation {
    /// Permanent expectation
    pub const PERMANENT: u32 = 1 << 0;
    /// Inactive until master is established
    pub const INACTIVE: u32 = 1 << 1;
    /// Expectation is userspace-controlled
    pub const USERSPACE: u32 = 1 << 2;

    /// Create new expectation
    pub fn new(tuple: ConnTuple, master: ConnTuple, timeout: u64) -> Self {
        Self {
            tuple,
            mask: ConnTuple::new(0xFFFFFFFF, 0xFFFFFFFF, 0xFFFF, 0xFFFF, 0xFF),
            master,
            timeout,
            flags: 0,
            helper: None,
        }
    }

    /// Check if tuple matches expectation
    pub fn matches(&self, tuple: &ConnTuple) -> bool {
        (tuple.src_addr & self.mask.src_addr) == (self.tuple.src_addr & self.mask.src_addr)
            && (tuple.dst_addr & self.mask.dst_addr) == (self.tuple.dst_addr & self.mask.dst_addr)
            && (tuple.src_port & self.mask.src_port) == (self.tuple.src_port & self.mask.src_port)
            && (tuple.dst_port & self.mask.dst_port) == (self.tuple.dst_port & self.mask.dst_port)
            && (tuple.protocol & self.mask.protocol) == (self.tuple.protocol & self.mask.protocol)
    }

    /// Check if expired
    pub fn is_expired(&self, now: u64) -> bool {
        (self.flags & Self::PERMANENT) == 0 && now >= self.timeout
    }
}

// =============================================================================
// HOOK FUNCTIONS
// =============================================================================

/// Prerouting hook for connection tracking
pub fn hook_prerouting(state: &mut NfHookState) -> NfVerdict {
    super::get().conntrack().process(state)
}

/// Local out hook for connection tracking
pub fn hook_local_out(state: &mut NfHookState) -> NfVerdict {
    super::get().conntrack().process(state)
}

/// Confirm hook (postrouting/local_in)
pub fn hook_confirm(state: &mut NfHookState) -> NfVerdict {
    if let Some(ref ct) = state.skb.nfct {
        super::get().conntrack().confirm(ct);
    }
    NfVerdict::Accept
}
