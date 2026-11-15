// ===============================================================================
// BUILDOS KERNEL - NETFILTER MATCH EXTENSIONS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Netfilter Match Extensions
//!
//! Match modules for packet inspection:
//! - TCP/UDP port matching
//! - ICMP type matching
//! - Connection state matching
//! - Rate limiting
//! - String matching
//! - And more

#![allow(dead_code)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Debug;

use super::conntrack::ConnState;
use super::NfHookState;

// =============================================================================
// MATCH TRAIT
// =============================================================================

/// Match extension trait
pub trait Match: Send + Sync + Debug {
    /// Match name
    fn name(&self) -> &str;

    /// Check if packet matches
    fn matches(&self, state: &NfHookState) -> bool;
}

// =============================================================================
// TCP MATCH
// =============================================================================

/// TCP match extension
#[derive(Debug, Clone)]
pub struct TcpMatch {
    /// Source port range
    pub src_ports: Option<PortRange>,
    /// Destination port range
    pub dst_ports: Option<PortRange>,
    /// TCP flags to match
    pub flags: Option<TcpFlags>,
    /// Invert source port match
    pub src_inv: bool,
    /// Invert destination port match
    pub dst_inv: bool,
}

/// Port range
#[derive(Debug, Clone, Copy)]
pub struct PortRange {
    /// Start port
    pub start: u16,
    /// End port (inclusive)
    pub end: u16,
}

impl PortRange {
    /// Create single port
    pub fn single(port: u16) -> Self {
        Self { start: port, end: port }
    }

    /// Create port range
    pub fn range(start: u16, end: u16) -> Self {
        Self { start, end }
    }

    /// Check if port is in range
    pub fn contains(&self, port: u16) -> bool {
        port >= self.start && port <= self.end
    }
}

/// TCP flags
#[derive(Debug, Clone, Copy)]
pub struct TcpFlags {
    /// Flags mask
    pub mask: u8,
    /// Flags to compare
    pub comp: u8,
}

impl TcpFlags {
    /// TCP flag constants
    pub const FIN: u8 = 0x01;
    pub const SYN: u8 = 0x02;
    pub const RST: u8 = 0x04;
    pub const PSH: u8 = 0x08;
    pub const ACK: u8 = 0x10;
    pub const URG: u8 = 0x20;

    /// Create new TCP flags match
    pub fn new(mask: u8, comp: u8) -> Self {
        Self { mask, comp }
    }

    /// Match SYN packets
    pub fn syn() -> Self {
        Self::new(Self::SYN | Self::ACK | Self::RST | Self::FIN, Self::SYN)
    }

    /// Match ACK packets
    pub fn ack() -> Self {
        Self::new(Self::ACK, Self::ACK)
    }

    /// Match RST packets
    pub fn rst() -> Self {
        Self::new(Self::RST, Self::RST)
    }

    /// Check if flags match
    pub fn matches(&self, flags: u8) -> bool {
        (flags & self.mask) == self.comp
    }
}

impl TcpMatch {
    /// Create new TCP match
    pub fn new() -> Self {
        Self {
            src_ports: None,
            dst_ports: None,
            flags: None,
            src_inv: false,
            dst_inv: false,
        }
    }

    /// Set source port
    pub fn src_port(mut self, port: u16) -> Self {
        self.src_ports = Some(PortRange::single(port));
        self
    }

    /// Set source port range
    pub fn src_port_range(mut self, start: u16, end: u16) -> Self {
        self.src_ports = Some(PortRange::range(start, end));
        self
    }

    /// Set destination port
    pub fn dst_port(mut self, port: u16) -> Self {
        self.dst_ports = Some(PortRange::single(port));
        self
    }

    /// Set destination port range
    pub fn dst_port_range(mut self, start: u16, end: u16) -> Self {
        self.dst_ports = Some(PortRange::range(start, end));
        self
    }

    /// Set TCP flags
    pub fn flags(mut self, flags: TcpFlags) -> Self {
        self.flags = Some(flags);
        self
    }
}

impl Default for TcpMatch {
    fn default() -> Self {
        Self::new()
    }
}

impl Match for TcpMatch {
    fn name(&self) -> &str {
        "tcp"
    }

    fn matches(&self, state: &NfHookState) -> bool {
        // Check protocol is TCP
        if state.skb.protocol() != Some(6) {
            return false;
        }

        // Check source port
        if let Some(ref range) = self.src_ports {
            if let Some(port) = state.skb.src_port() {
                let matched = range.contains(port);
                if matched == self.src_inv {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Check destination port
        if let Some(ref range) = self.dst_ports {
            if let Some(port) = state.skb.dst_port() {
                let matched = range.contains(port);
                if matched == self.dst_inv {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Check TCP flags (would need TCP header access)
        if let Some(ref _flags) = self.flags {
            // Would read TCP flags from header
        }

        true
    }
}

// =============================================================================
// UDP MATCH
// =============================================================================

/// UDP match extension
#[derive(Debug, Clone)]
pub struct UdpMatch {
    /// Source port range
    pub src_ports: Option<PortRange>,
    /// Destination port range
    pub dst_ports: Option<PortRange>,
    /// Invert source port match
    pub src_inv: bool,
    /// Invert destination port match
    pub dst_inv: bool,
}

impl UdpMatch {
    /// Create new UDP match
    pub fn new() -> Self {
        Self {
            src_ports: None,
            dst_ports: None,
            src_inv: false,
            dst_inv: false,
        }
    }

    /// Set destination port
    pub fn dst_port(mut self, port: u16) -> Self {
        self.dst_ports = Some(PortRange::single(port));
        self
    }
}

impl Default for UdpMatch {
    fn default() -> Self {
        Self::new()
    }
}

impl Match for UdpMatch {
    fn name(&self) -> &str {
        "udp"
    }

    fn matches(&self, state: &NfHookState) -> bool {
        // Check protocol is UDP
        if state.skb.protocol() != Some(17) {
            return false;
        }

        // Check source port
        if let Some(ref range) = self.src_ports {
            if let Some(port) = state.skb.src_port() {
                let matched = range.contains(port);
                if matched == self.src_inv {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Check destination port
        if let Some(ref range) = self.dst_ports {
            if let Some(port) = state.skb.dst_port() {
                let matched = range.contains(port);
                if matched == self.dst_inv {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }
}

// =============================================================================
// ICMP MATCH
// =============================================================================

/// ICMP match extension
#[derive(Debug, Clone)]
pub struct IcmpMatch {
    /// ICMP type
    pub icmp_type: Option<u8>,
    /// ICMP code
    pub icmp_code: Option<u8>,
}

impl IcmpMatch {
    /// ICMP types
    pub const ECHO_REPLY: u8 = 0;
    pub const DEST_UNREACHABLE: u8 = 3;
    pub const SOURCE_QUENCH: u8 = 4;
    pub const REDIRECT: u8 = 5;
    pub const ECHO_REQUEST: u8 = 8;
    pub const TIME_EXCEEDED: u8 = 11;
    pub const PARAMETER_PROBLEM: u8 = 12;
    pub const TIMESTAMP: u8 = 13;
    pub const TIMESTAMP_REPLY: u8 = 14;

    /// Create new ICMP match
    pub fn new() -> Self {
        Self {
            icmp_type: None,
            icmp_code: None,
        }
    }

    /// Set ICMP type
    pub fn icmp_type(mut self, t: u8) -> Self {
        self.icmp_type = Some(t);
        self
    }

    /// Set ICMP code
    pub fn icmp_code(mut self, c: u8) -> Self {
        self.icmp_code = Some(c);
        self
    }

    /// Match echo request (ping)
    pub fn echo_request() -> Self {
        Self::new().icmp_type(Self::ECHO_REQUEST)
    }

    /// Match echo reply
    pub fn echo_reply() -> Self {
        Self::new().icmp_type(Self::ECHO_REPLY)
    }
}

impl Default for IcmpMatch {
    fn default() -> Self {
        Self::new()
    }
}

impl Match for IcmpMatch {
    fn name(&self) -> &str {
        "icmp"
    }

    fn matches(&self, state: &NfHookState) -> bool {
        // Check protocol is ICMP
        if state.skb.protocol() != Some(1) {
            return false;
        }

        // Would read ICMP type/code from header
        true
    }
}

// =============================================================================
// CONNECTION STATE MATCH
// =============================================================================

/// Connection tracking state match
#[derive(Debug, Clone)]
pub struct StateMatch {
    /// States to match (bitmask)
    pub states: u32,
}

impl StateMatch {
    /// State bits
    pub const INVALID: u32 = 1 << 0;
    pub const NEW: u32 = 1 << 1;
    pub const ESTABLISHED: u32 = 1 << 2;
    pub const RELATED: u32 = 1 << 3;
    pub const UNTRACKED: u32 = 1 << 4;
    pub const SNAT: u32 = 1 << 5;
    pub const DNAT: u32 = 1 << 6;

    /// Create new state match
    pub fn new(states: u32) -> Self {
        Self { states }
    }

    /// Match established connections
    pub fn established() -> Self {
        Self::new(Self::ESTABLISHED)
    }

    /// Match established and related
    pub fn established_related() -> Self {
        Self::new(Self::ESTABLISHED | Self::RELATED)
    }

    /// Match new connections
    pub fn new_conn() -> Self {
        Self::new(Self::NEW)
    }
}

impl Match for StateMatch {
    fn name(&self) -> &str {
        "state"
    }

    fn matches(&self, state: &NfHookState) -> bool {
        if let Some(ref ct) = state.skb.nfct {
            let conn_state = ct.state();

            let state_bit = match conn_state {
                ConnState::None => Self::INVALID,
                ConnState::New => Self::NEW,
                ConnState::Established => Self::ESTABLISHED,
                ConnState::Related => Self::RELATED,
                ConnState::RelatedReply => Self::RELATED,
                ConnState::TimeWait => Self::ESTABLISHED,
                _ => 0,
            };

            (self.states & state_bit) != 0
        } else {
            // No conntrack, check if UNTRACKED matches
            (self.states & Self::UNTRACKED) != 0
        }
    }
}

// =============================================================================
// MULTIPORT MATCH
// =============================================================================

/// Match multiple ports
#[derive(Debug, Clone)]
pub struct MultiportMatch {
    /// Ports to match (source)
    pub src_ports: Vec<u16>,
    /// Ports to match (destination)
    pub dst_ports: Vec<u16>,
    /// Match both source or destination
    pub either: bool,
}

impl MultiportMatch {
    /// Create new multiport match
    pub fn new() -> Self {
        Self {
            src_ports: Vec::new(),
            dst_ports: Vec::new(),
            either: false,
        }
    }

    /// Add destination ports
    pub fn dports(mut self, ports: &[u16]) -> Self {
        self.dst_ports.extend_from_slice(ports);
        self
    }

    /// Add source ports
    pub fn sports(mut self, ports: &[u16]) -> Self {
        self.src_ports.extend_from_slice(ports);
        self
    }
}

impl Default for MultiportMatch {
    fn default() -> Self {
        Self::new()
    }
}

impl Match for MultiportMatch {
    fn name(&self) -> &str {
        "multiport"
    }

    fn matches(&self, state: &NfHookState) -> bool {
        let src_port = state.skb.src_port();
        let dst_port = state.skb.dst_port();

        let src_matches = if self.src_ports.is_empty() {
            true
        } else if let Some(port) = src_port {
            self.src_ports.contains(&port)
        } else {
            false
        };

        let dst_matches = if self.dst_ports.is_empty() {
            true
        } else if let Some(port) = dst_port {
            self.dst_ports.contains(&port)
        } else {
            false
        };

        if self.either {
            src_matches || dst_matches
        } else {
            src_matches && dst_matches
        }
    }
}

// =============================================================================
// LIMIT MATCH
// =============================================================================

use core::sync::atomic::{AtomicU64, Ordering};

/// Rate limiting match
#[derive(Debug)]
pub struct LimitMatch {
    /// Average rate (packets per second * 1000)
    avg: u64,
    /// Burst size
    burst: u32,
    /// Current credits
    credits: AtomicU64,
    /// Last update time
    last_time: AtomicU64,
}

impl LimitMatch {
    /// Create new limit match
    pub fn new(rate: u32, burst: u32) -> Self {
        let avg = rate as u64 * 1000;
        Self {
            avg,
            burst,
            credits: AtomicU64::new(burst as u64 * 1000),
            last_time: AtomicU64::new(0),
        }
    }

    /// 3 packets per second with burst of 5
    pub fn default_rate() -> Self {
        Self::new(3, 5)
    }
}

impl Match for LimitMatch {
    fn name(&self) -> &str {
        "limit"
    }

    fn matches(&self, _state: &NfHookState) -> bool {
        // Simplified token bucket
        let now = 0u64; // Would get current time

        let last = self.last_time.load(Ordering::SeqCst);
        let elapsed = now.saturating_sub(last);

        // Replenish credits
        let mut credits = self.credits.load(Ordering::SeqCst);
        credits = credits.saturating_add(elapsed * self.avg / 1000);
        let max_credits = self.burst as u64 * 1000;
        if credits > max_credits {
            credits = max_credits;
        }

        // Try to consume one credit
        if credits >= 1000 {
            self.credits.store(credits - 1000, Ordering::SeqCst);
            self.last_time.store(now, Ordering::SeqCst);
            true
        } else {
            false
        }
    }
}

// =============================================================================
// MAC ADDRESS MATCH
// =============================================================================

/// MAC address match
#[derive(Debug, Clone)]
pub struct MacMatch {
    /// Source MAC address
    pub mac: [u8; 6],
    /// Invert match
    pub invert: bool,
}

impl MacMatch {
    /// Create new MAC match
    pub fn new(mac: [u8; 6]) -> Self {
        Self { mac, invert: false }
    }

    /// Invert match
    pub fn inverted(mut self) -> Self {
        self.invert = true;
        self
    }
}

impl Match for MacMatch {
    fn name(&self) -> &str {
        "mac"
    }

    fn matches(&self, _state: &NfHookState) -> bool {
        // Would extract MAC from Ethernet header
        true
    }
}

// =============================================================================
// STRING MATCH
// =============================================================================

/// String pattern match
#[derive(Debug, Clone)]
pub struct StringMatch {
    /// Pattern to search for
    pub pattern: String,
    /// Algorithm (bm = Boyer-Moore, kmp = Knuth-Morris-Pratt)
    pub algo: StringAlgo,
    /// Start offset
    pub from: u16,
    /// End offset
    pub to: u16,
}

/// String matching algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringAlgo {
    /// Boyer-Moore
    BoyerMoore,
    /// Knuth-Morris-Pratt
    Kmp,
}

impl StringMatch {
    /// Create new string match
    pub fn new(pattern: &str) -> Self {
        Self {
            pattern: String::from(pattern),
            algo: StringAlgo::BoyerMoore,
            from: 0,
            to: 65535,
        }
    }
}

impl Match for StringMatch {
    fn name(&self) -> &str {
        "string"
    }

    fn matches(&self, state: &NfHookState) -> bool {
        let data = &state.skb.data;
        let pattern = self.pattern.as_bytes();

        // Simple search (would use proper algorithm)
        let start = self.from as usize;
        let end = (self.to as usize).min(data.len());

        if start >= end || pattern.is_empty() {
            return false;
        }

        data[start..end].windows(pattern.len()).any(|w| w == pattern)
    }
}

// =============================================================================
// COMMENT MATCH
// =============================================================================

/// Comment (no-op match, always succeeds)
#[derive(Debug, Clone)]
pub struct CommentMatch {
    /// Comment text
    pub comment: String,
}

impl CommentMatch {
    /// Create new comment
    pub fn new(comment: &str) -> Self {
        Self {
            comment: String::from(comment),
        }
    }
}

impl Match for CommentMatch {
    fn name(&self) -> &str {
        "comment"
    }

    fn matches(&self, _state: &NfHookState) -> bool {
        true // Always matches
    }
}

// =============================================================================
// MARK MATCH
// =============================================================================

/// Packet mark match
#[derive(Debug, Clone)]
pub struct MarkMatch {
    /// Mark value
    pub mark: u32,
    /// Mark mask
    pub mask: u32,
}

impl MarkMatch {
    /// Create new mark match
    pub fn new(mark: u32, mask: u32) -> Self {
        Self { mark, mask }
    }
}

impl Match for MarkMatch {
    fn name(&self) -> &str {
        "mark"
    }

    fn matches(&self, state: &NfHookState) -> bool {
        (state.skb.mark & self.mask) == (self.mark & self.mask)
    }
}
