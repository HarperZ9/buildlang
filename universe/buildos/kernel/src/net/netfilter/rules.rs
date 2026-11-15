// ===============================================================================
// BUILDOS KERNEL - NETFILTER RULES
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Netfilter Rules
//!
//! Rule structure and matching logic

#![allow(dead_code)]

extern crate alloc;

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use super::chains::RuleTarget;
use super::matches::Match;
use super::NfHookState;

// =============================================================================
// RULE STRUCTURE
// =============================================================================

/// Netfilter rule
#[derive(Clone)]
pub struct Rule {
    /// Rule number (assigned by chain)
    pub number: u32,
    /// Protocol (0 = any)
    pub protocol: u8,
    /// Source address and mask
    pub src: Option<IpMatch>,
    /// Destination address and mask
    pub dst: Option<IpMatch>,
    /// Input interface
    pub in_iface: Option<String>,
    /// Output interface
    pub out_iface: Option<String>,
    /// Match extensions
    pub matches: Vec<Arc<dyn Match>>,
    /// Target action
    pub target: RuleTarget,
    /// Invert source match
    pub src_inv: bool,
    /// Invert destination match
    pub dst_inv: bool,
    /// Invert protocol match
    pub proto_inv: bool,
    /// Counters
    counters: RuleCounters,
}

/// Rule counters
#[derive(Default)]
pub struct RuleCounters {
    /// Packet count
    packets: AtomicU64,
    /// Byte count
    bytes: AtomicU64,
}

impl Clone for RuleCounters {
    fn clone(&self) -> Self {
        Self {
            packets: AtomicU64::new(self.packets.load(Ordering::Relaxed)),
            bytes: AtomicU64::new(self.bytes.load(Ordering::Relaxed)),
        }
    }
}

impl core::fmt::Debug for RuleCounters {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RuleCounters")
            .field("packets", &self.packets.load(Ordering::Relaxed))
            .field("bytes", &self.bytes.load(Ordering::Relaxed))
            .finish()
    }
}

impl core::fmt::Debug for Rule {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Rule")
            .field("number", &self.number)
            .field("protocol", &self.protocol)
            .field("src", &self.src)
            .field("dst", &self.dst)
            .field("in_iface", &self.in_iface)
            .field("out_iface", &self.out_iface)
            .field("matches", &self.matches.len())
            .field("target", &self.target)
            .field("counters", &self.counters)
            .finish()
    }
}

/// IP address match
#[derive(Debug, Clone)]
pub struct IpMatch {
    /// Address (network byte order)
    pub addr: u32,
    /// Netmask (network byte order)
    pub mask: u32,
}

impl IpMatch {
    /// Create new IP match
    pub fn new(addr: u32, prefix_len: u8) -> Self {
        let mask = if prefix_len >= 32 {
            0xFFFFFFFF
        } else if prefix_len == 0 {
            0
        } else {
            !((1u32 << (32 - prefix_len)) - 1)
        };

        Self {
            addr: addr & mask,
            mask,
        }
    }

    /// Check if address matches
    pub fn matches(&self, addr: u32) -> bool {
        (addr & self.mask) == self.addr
    }
}

impl Rule {
    /// Create new rule
    pub fn new(target: RuleTarget) -> Self {
        Self {
            number: 0,
            protocol: 0,
            src: None,
            dst: None,
            in_iface: None,
            out_iface: None,
            matches: Vec::new(),
            target,
            src_inv: false,
            dst_inv: false,
            proto_inv: false,
            counters: RuleCounters::default(),
        }
    }

    /// Set protocol
    pub fn with_protocol(mut self, protocol: u8, invert: bool) -> Self {
        self.protocol = protocol;
        self.proto_inv = invert;
        self
    }

    /// Set source address
    pub fn with_src(mut self, addr: u32, prefix: u8, invert: bool) -> Self {
        self.src = Some(IpMatch::new(addr, prefix));
        self.src_inv = invert;
        self
    }

    /// Set destination address
    pub fn with_dst(mut self, addr: u32, prefix: u8, invert: bool) -> Self {
        self.dst = Some(IpMatch::new(addr, prefix));
        self.dst_inv = invert;
        self
    }

    /// Set input interface
    pub fn with_in_iface(mut self, iface: &str) -> Self {
        self.in_iface = Some(String::from(iface));
        self
    }

    /// Set output interface
    pub fn with_out_iface(mut self, iface: &str) -> Self {
        self.out_iface = Some(String::from(iface));
        self
    }

    /// Add match extension
    pub fn with_match(mut self, m: Arc<dyn Match>) -> Self {
        self.matches.push(m);
        self
    }

    /// Get target
    pub fn target(&self) -> &RuleTarget {
        &self.target
    }

    /// Check if rule jumps to a chain
    pub fn jumps_to(&self, chain_name: &str) -> bool {
        match &self.target {
            RuleTarget::Jump(name) | RuleTarget::Goto(name) => name == chain_name,
            _ => false,
        }
    }

    /// Check if packet matches this rule
    pub fn matches(&self, state: &NfHookState) -> bool {
        // Protocol match
        if self.protocol != 0 {
            if let Some(proto) = state.skb.protocol() {
                let matched = proto == self.protocol;
                if matched == self.proto_inv {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Source address match
        if let Some(ref src) = self.src {
            if let Some(pkt_src) = state.skb.src_ip() {
                let matched = src.matches(pkt_src);
                if matched == self.src_inv {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Destination address match
        if let Some(ref dst) = self.dst {
            if let Some(pkt_dst) = state.skb.dst_ip() {
                let matched = dst.matches(pkt_dst);
                if matched == self.dst_inv {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Interface matches
        if let Some(ref iface) = self.in_iface {
            if let Some(in_dev) = state.in_dev {
                // Would need to look up interface name from device index
                let _ = (iface, in_dev);
            }
        }

        if let Some(ref iface) = self.out_iface {
            if let Some(out_dev) = state.out_dev {
                let _ = (iface, out_dev);
            }
        }

        // Match extensions
        for m in &self.matches {
            if !m.matches(state) {
                return false;
            }
        }

        true
    }

    /// Increment counters
    pub fn increment_counters(&self, bytes: usize) {
        self.counters.packets.fetch_add(1, Ordering::Relaxed);
        self.counters.bytes.fetch_add(bytes as u64, Ordering::Relaxed);
    }

    /// Get packet count
    pub fn packet_count(&self) -> u64 {
        self.counters.packets.load(Ordering::Relaxed)
    }

    /// Get byte count
    pub fn byte_count(&self) -> u64 {
        self.counters.bytes.load(Ordering::Relaxed)
    }

    /// Zero counters
    pub fn zero_counters(&mut self) {
        self.counters.packets.store(0, Ordering::SeqCst);
        self.counters.bytes.store(0, Ordering::SeqCst);
    }
}

// =============================================================================
// RULE BUILDER
// =============================================================================

/// Rule builder for easier rule construction
pub struct RuleBuilder {
    rule: Rule,
}

impl RuleBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            rule: Rule::new(RuleTarget::Accept),
        }
    }

    /// Set target
    pub fn target(mut self, target: RuleTarget) -> Self {
        self.rule.target = target;
        self
    }

    /// Set protocol (TCP = 6, UDP = 17, ICMP = 1)
    pub fn protocol(mut self, proto: u8) -> Self {
        self.rule.protocol = proto;
        self
    }

    /// Invert protocol match
    pub fn not_protocol(mut self, proto: u8) -> Self {
        self.rule.protocol = proto;
        self.rule.proto_inv = true;
        self
    }

    /// Set source address
    pub fn source(mut self, addr: u32, prefix: u8) -> Self {
        self.rule.src = Some(IpMatch::new(addr, prefix));
        self
    }

    /// Invert source match
    pub fn not_source(mut self, addr: u32, prefix: u8) -> Self {
        self.rule.src = Some(IpMatch::new(addr, prefix));
        self.rule.src_inv = true;
        self
    }

    /// Set destination address
    pub fn dest(mut self, addr: u32, prefix: u8) -> Self {
        self.rule.dst = Some(IpMatch::new(addr, prefix));
        self
    }

    /// Invert destination match
    pub fn not_dest(mut self, addr: u32, prefix: u8) -> Self {
        self.rule.dst = Some(IpMatch::new(addr, prefix));
        self.rule.dst_inv = true;
        self
    }

    /// Set input interface
    pub fn in_interface(mut self, iface: &str) -> Self {
        self.rule.in_iface = Some(String::from(iface));
        self
    }

    /// Set output interface
    pub fn out_interface(mut self, iface: &str) -> Self {
        self.rule.out_iface = Some(String::from(iface));
        self
    }

    /// Add match extension
    pub fn add_match(mut self, m: Arc<dyn Match>) -> Self {
        self.rule.matches.push(m);
        self
    }

    /// Build the rule
    pub fn build(self) -> Rule {
        self.rule
    }
}

impl Default for RuleBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// RULE ENTRY (FOR STORAGE)
// =============================================================================

/// Serializable rule entry
#[derive(Debug, Clone)]
pub struct RuleEntry {
    /// Protocol
    pub protocol: u8,
    /// Source address
    pub src_addr: u32,
    /// Source mask
    pub src_mask: u32,
    /// Destination address
    pub dst_addr: u32,
    /// Destination mask
    pub dst_mask: u32,
    /// Input interface name
    pub in_iface: [u8; 16],
    /// Output interface name
    pub out_iface: [u8; 16],
    /// Flags (inversions, etc.)
    pub flags: u32,
    /// Target offset
    pub target_offset: u16,
    /// Next rule offset
    pub next_offset: u16,
    /// Packet counter
    pub pcnt: u64,
    /// Byte counter
    pub bcnt: u64,
}

impl RuleEntry {
    /// Flag: invert source
    pub const INV_SRC: u32 = 1 << 0;
    /// Flag: invert destination
    pub const INV_DST: u32 = 1 << 1;
    /// Flag: invert protocol
    pub const INV_PROTO: u32 = 1 << 2;
    /// Flag: invert input interface
    pub const INV_IN: u32 = 1 << 3;
    /// Flag: invert output interface
    pub const INV_OUT: u32 = 1 << 4;

    /// Create from rule
    pub fn from_rule(rule: &Rule) -> Self {
        let mut entry = Self {
            protocol: rule.protocol,
            src_addr: rule.src.as_ref().map(|s| s.addr).unwrap_or(0),
            src_mask: rule.src.as_ref().map(|s| s.mask).unwrap_or(0),
            dst_addr: rule.dst.as_ref().map(|d| d.addr).unwrap_or(0),
            dst_mask: rule.dst.as_ref().map(|d| d.mask).unwrap_or(0),
            in_iface: [0; 16],
            out_iface: [0; 16],
            flags: 0,
            target_offset: 0,
            next_offset: 0,
            pcnt: rule.packet_count(),
            bcnt: rule.byte_count(),
        };

        if rule.src_inv {
            entry.flags |= Self::INV_SRC;
        }
        if rule.dst_inv {
            entry.flags |= Self::INV_DST;
        }
        if rule.proto_inv {
            entry.flags |= Self::INV_PROTO;
        }

        if let Some(ref iface) = rule.in_iface {
            let bytes = iface.as_bytes();
            let len = bytes.len().min(15);
            entry.in_iface[..len].copy_from_slice(&bytes[..len]);
        }

        if let Some(ref iface) = rule.out_iface {
            let bytes = iface.as_bytes();
            let len = bytes.len().min(15);
            entry.out_iface[..len].copy_from_slice(&bytes[..len]);
        }

        entry
    }
}
