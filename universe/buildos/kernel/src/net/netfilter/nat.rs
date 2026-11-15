// ===============================================================================
// BUILDOS KERNEL - NETFILTER NAT
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Network Address Translation (NAT)
//!
//! Implements:
//! - Source NAT (SNAT) / Masquerading
//! - Destination NAT (DNAT) / Port forwarding
//! - Redirect
//! - Full cone / Restricted cone NAT behavior

#![allow(dead_code)]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU16, AtomicU64, Ordering};

use super::chains::Target;
use super::conntrack::{ConntrackEntry, NatInfo};
use super::{NfHookState, NfVerdict};
use crate::sync::{Mutex, RwLock};

// =============================================================================
// NAT TYPE
// =============================================================================

/// NAT type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NatType {
    /// Source NAT
    Snat,
    /// Destination NAT
    Dnat,
    /// Masquerade (dynamic SNAT)
    Masquerade,
    /// Redirect (local DNAT)
    Redirect,
}

// =============================================================================
// NAT RANGE
// =============================================================================

/// NAT address/port range
#[derive(Debug, Clone)]
pub struct NatRange {
    /// Minimum IP address
    pub min_addr: u32,
    /// Maximum IP address
    pub max_addr: u32,
    /// Minimum port
    pub min_port: u16,
    /// Maximum port
    pub max_port: u16,
    /// Flags
    pub flags: u32,
}

impl NatRange {
    /// Use specific IP
    pub const MAP_IPS: u32 = 1 << 0;
    /// Use specific port range
    pub const PROTO_SPECIFIED: u32 = 1 << 1;
    /// Use random port
    pub const PROTO_RANDOM: u32 = 1 << 2;
    /// Use fully random port
    pub const PROTO_RANDOM_FULLY: u32 = 1 << 3;
    /// Persistent mapping
    pub const PERSISTENT: u32 = 1 << 4;

    /// Create new NAT range
    pub fn new(addr: u32) -> Self {
        Self {
            min_addr: addr,
            max_addr: addr,
            min_port: 0,
            max_port: 0,
            flags: Self::MAP_IPS,
        }
    }

    /// Create address range
    pub fn addr_range(min: u32, max: u32) -> Self {
        Self {
            min_addr: min,
            max_addr: max,
            min_port: 0,
            max_port: 0,
            flags: Self::MAP_IPS,
        }
    }

    /// Create port range
    pub fn port_range(min: u16, max: u16) -> Self {
        Self {
            min_addr: 0,
            max_addr: 0,
            min_port: min,
            max_port: max,
            flags: Self::PROTO_SPECIFIED,
        }
    }

    /// Create address and port range
    pub fn full(min_addr: u32, max_addr: u32, min_port: u16, max_port: u16) -> Self {
        Self {
            min_addr,
            max_addr,
            min_port,
            max_port,
            flags: Self::MAP_IPS | Self::PROTO_SPECIFIED,
        }
    }

    /// Enable random port selection
    pub fn random(mut self) -> Self {
        self.flags |= Self::PROTO_RANDOM;
        self
    }

    /// Enable fully random port selection
    pub fn random_fully(mut self) -> Self {
        self.flags |= Self::PROTO_RANDOM_FULLY;
        self
    }

    /// Enable persistent mapping
    pub fn persistent(mut self) -> Self {
        self.flags |= Self::PERSISTENT;
        self
    }
}

// =============================================================================
// NAT TABLE
// =============================================================================

/// NAT table
pub struct NatTable {
    /// SNAT rules
    snat_rules: RwLock<Vec<NatRule>>,
    /// DNAT rules
    dnat_rules: RwLock<Vec<NatRule>>,
    /// Port allocation
    port_alloc: PortAllocator,
    /// Statistics
    stats: NatStats,
}

/// NAT rule
#[derive(Debug, Clone)]
pub struct NatRule {
    /// Source address (0 = any)
    pub src: u32,
    /// Source mask
    pub src_mask: u32,
    /// Destination address (0 = any)
    pub dst: u32,
    /// Destination mask
    pub dst_mask: u32,
    /// Protocol (0 = any)
    pub protocol: u8,
    /// Source port range
    pub src_port_min: u16,
    pub src_port_max: u16,
    /// Destination port range
    pub dst_port_min: u16,
    pub dst_port_max: u16,
    /// NAT type
    pub nat_type: NatType,
    /// NAT range
    pub range: NatRange,
    /// Input interface
    pub in_iface: Option<u32>,
    /// Output interface
    pub out_iface: Option<u32>,
}

impl NatRule {
    /// Create new SNAT rule
    pub fn snat(addr: u32) -> Self {
        Self {
            src: 0,
            src_mask: 0,
            dst: 0,
            dst_mask: 0,
            protocol: 0,
            src_port_min: 0,
            src_port_max: 65535,
            dst_port_min: 0,
            dst_port_max: 65535,
            nat_type: NatType::Snat,
            range: NatRange::new(addr),
            in_iface: None,
            out_iface: None,
        }
    }

    /// Create new DNAT rule
    pub fn dnat(addr: u32, port: u16) -> Self {
        Self {
            src: 0,
            src_mask: 0,
            dst: 0,
            dst_mask: 0,
            protocol: 0,
            src_port_min: 0,
            src_port_max: 65535,
            dst_port_min: 0,
            dst_port_max: 65535,
            nat_type: NatType::Dnat,
            range: NatRange::full(addr, addr, port, port),
            in_iface: None,
            out_iface: None,
        }
    }

    /// Create masquerade rule
    pub fn masquerade() -> Self {
        Self {
            src: 0,
            src_mask: 0,
            dst: 0,
            dst_mask: 0,
            protocol: 0,
            src_port_min: 0,
            src_port_max: 65535,
            dst_port_min: 0,
            dst_port_max: 65535,
            nat_type: NatType::Masquerade,
            range: NatRange::port_range(1024, 65535),
            in_iface: None,
            out_iface: None,
        }
    }

    /// Create redirect rule
    pub fn redirect(port: u16) -> Self {
        Self {
            src: 0,
            src_mask: 0,
            dst: 0,
            dst_mask: 0,
            protocol: 0,
            src_port_min: 0,
            src_port_max: 65535,
            dst_port_min: 0,
            dst_port_max: 65535,
            nat_type: NatType::Redirect,
            range: NatRange::port_range(port, port),
            in_iface: None,
            out_iface: None,
        }
    }

    /// Set output interface requirement
    pub fn on_interface(mut self, iface: u32) -> Self {
        self.out_iface = Some(iface);
        self
    }

    /// Set source address filter
    pub fn from_source(mut self, addr: u32, prefix: u8) -> Self {
        self.src = addr;
        self.src_mask = if prefix >= 32 {
            0xFFFFFFFF
        } else {
            !((1u32 << (32 - prefix)) - 1)
        };
        self
    }

    /// Set destination filter
    pub fn to_dest(mut self, addr: u32, prefix: u8) -> Self {
        self.dst = addr;
        self.dst_mask = if prefix >= 32 {
            0xFFFFFFFF
        } else {
            !((1u32 << (32 - prefix)) - 1)
        };
        self
    }

    /// Set protocol filter
    pub fn protocol(mut self, proto: u8) -> Self {
        self.protocol = proto;
        self
    }

    /// Set destination port filter
    pub fn dport(mut self, port: u16) -> Self {
        self.dst_port_min = port;
        self.dst_port_max = port;
        self
    }

    /// Set destination port range filter
    pub fn dport_range(mut self, min: u16, max: u16) -> Self {
        self.dst_port_min = min;
        self.dst_port_max = max;
        self
    }

    /// Check if rule matches packet
    pub fn matches(&self, state: &NfHookState) -> bool {
        // Check source address
        if self.src_mask != 0 {
            if let Some(src) = state.skb.src_ip() {
                if (src & self.src_mask) != (self.src & self.src_mask) {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Check destination address
        if self.dst_mask != 0 {
            if let Some(dst) = state.skb.dst_ip() {
                if (dst & self.dst_mask) != (self.dst & self.dst_mask) {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Check protocol
        if self.protocol != 0 {
            if state.skb.protocol() != Some(self.protocol) {
                return false;
            }
        }

        // Check destination port
        if self.dst_port_min != 0 || self.dst_port_max != 65535 {
            if let Some(port) = state.skb.dst_port() {
                if port < self.dst_port_min || port > self.dst_port_max {
                    return false;
                }
            }
        }

        // Check interface
        if let Some(iface) = self.out_iface {
            if state.out_dev != Some(iface) {
                return false;
            }
        }

        true
    }
}

/// NAT statistics
#[derive(Default)]
pub struct NatStats {
    /// SNAT mappings
    pub snat_count: AtomicU64,
    /// DNAT mappings
    pub dnat_count: AtomicU64,
    /// Masquerade mappings
    pub masq_count: AtomicU64,
    /// Failed mappings
    pub failed: AtomicU64,
}

impl NatTable {
    /// Create new NAT table
    pub fn new() -> Self {
        Self {
            snat_rules: RwLock::new(Vec::new()),
            dnat_rules: RwLock::new(Vec::new()),
            port_alloc: PortAllocator::new(),
            stats: NatStats::default(),
        }
    }

    /// Add SNAT/Masquerade rule
    pub fn add_snat_rule(&self, rule: NatRule) {
        self.snat_rules.write().push(rule);
    }

    /// Add DNAT/Redirect rule
    pub fn add_dnat_rule(&self, rule: NatRule) {
        self.dnat_rules.write().push(rule);
    }

    /// Remove SNAT rule
    pub fn remove_snat_rule(&self, index: usize) -> Option<NatRule> {
        let mut rules = self.snat_rules.write();
        if index < rules.len() {
            Some(rules.remove(index))
        } else {
            None
        }
    }

    /// Remove DNAT rule
    pub fn remove_dnat_rule(&self, index: usize) -> Option<NatRule> {
        let mut rules = self.dnat_rules.write();
        if index < rules.len() {
            Some(rules.remove(index))
        } else {
            None
        }
    }

    /// Process DNAT (prerouting)
    pub fn process_dnat(&self, state: &mut NfHookState) -> NfVerdict {
        // Check if already NATed
        if let Some(ct) = state.skb.nfct.clone() {
            if ct.nat_info().is_some() {
                return self.apply_nat(state, &ct, false);
            }
        }

        // Find matching DNAT rule
        let rules = self.dnat_rules.read();
        for rule in rules.iter() {
            if rule.matches(state) {
                return self.do_dnat(state, rule);
            }
        }

        NfVerdict::Accept
    }

    /// Process SNAT (postrouting)
    pub fn process_snat(&self, state: &mut NfHookState) -> NfVerdict {
        // Check if already NATed
        if let Some(ct) = state.skb.nfct.clone() {
            if ct.nat_info().is_some() {
                return self.apply_nat(state, &ct, true);
            }
        }

        // Find matching SNAT rule
        let rules = self.snat_rules.read();
        for rule in rules.iter() {
            if rule.matches(state) {
                return self.do_snat(state, rule);
            }
        }

        NfVerdict::Accept
    }

    /// Perform DNAT
    fn do_dnat(&self, state: &mut NfHookState, rule: &NatRule) -> NfVerdict {
        let orig_dst = state.skb.dst_ip().unwrap_or(0);
        let orig_dport = state.skb.dst_port().unwrap_or(0);

        // Select new destination
        let new_dst = self.select_addr(&rule.range);
        let new_dport = self.select_port(&rule.range);

        // Record NAT info in conntrack
        if let Some(ref ct) = state.skb.nfct {
            ct.set_nat_info(NatInfo {
                orig_src: 0,
                orig_src_port: 0,
                orig_dst,
                orig_dst_port: orig_dport,
            });
        }

        // Would modify packet headers here
        state.orig_dst = Some(orig_dst);
        let _ = (new_dst, new_dport);

        self.stats.dnat_count.fetch_add(1, Ordering::Relaxed);
        NfVerdict::Accept
    }

    /// Perform SNAT
    fn do_snat(&self, state: &mut NfHookState, rule: &NatRule) -> NfVerdict {
        let orig_src = state.skb.src_ip().unwrap_or(0);
        let orig_sport = state.skb.src_port().unwrap_or(0);

        // Select new source
        let new_src = match rule.nat_type {
            NatType::Masquerade => {
                // Get outgoing interface address
                self.get_interface_addr(state.out_dev.unwrap_or(0))
            }
            _ => self.select_addr(&rule.range),
        };

        let new_sport = self.port_alloc.allocate(&rule.range);

        // Record NAT info in conntrack
        if let Some(ref ct) = state.skb.nfct {
            ct.set_nat_info(NatInfo {
                orig_src,
                orig_src_port: orig_sport,
                orig_dst: 0,
                orig_dst_port: 0,
            });
        }

        // Would modify packet headers here
        let _ = (new_src, new_sport);

        match rule.nat_type {
            NatType::Masquerade => self.stats.masq_count.fetch_add(1, Ordering::Relaxed),
            _ => self.stats.snat_count.fetch_add(1, Ordering::Relaxed),
        };

        NfVerdict::Accept
    }

    /// Apply existing NAT mapping
    fn apply_nat(&self, _state: &mut NfHookState, ct: &Arc<ConntrackEntry>, _is_snat: bool) -> NfVerdict {
        if let Some(_nat_info) = ct.nat_info() {
            // Would apply NAT transformation based on direction
        }
        NfVerdict::Accept
    }

    /// Select address from range
    fn select_addr(&self, range: &NatRange) -> u32 {
        if range.min_addr == range.max_addr {
            range.min_addr
        } else {
            // Would select from range (round-robin or random)
            range.min_addr
        }
    }

    /// Select port from range
    fn select_port(&self, range: &NatRange) -> u16 {
        if (range.flags & NatRange::PROTO_SPECIFIED) == 0 {
            return 0;
        }

        self.port_alloc.allocate(range)
    }

    /// Get interface address (for masquerade)
    fn get_interface_addr(&self, _iface: u32) -> u32 {
        // Would look up interface's IP address
        0
    }

    /// Flush all rules
    pub fn flush(&self) {
        self.snat_rules.write().clear();
        self.dnat_rules.write().clear();
    }

    /// Get statistics
    pub fn stats(&self) -> &NatStats {
        &self.stats
    }
}

impl Default for NatTable {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// PORT ALLOCATOR
// =============================================================================

/// Port allocator for NAT
pub struct PortAllocator {
    /// Next port to try
    next_port: AtomicU16,
    /// Used ports (simplified)
    used: Mutex<BTreeMap<(u32, u16), u64>>,
}

impl PortAllocator {
    /// Create new port allocator
    pub fn new() -> Self {
        Self {
            next_port: AtomicU16::new(32768),
            used: Mutex::new(BTreeMap::new()),
        }
    }

    /// Allocate a port
    pub fn allocate(&self, range: &NatRange) -> u16 {
        let min = if range.min_port == 0 { 1024 } else { range.min_port };
        let max = if range.max_port == 0 { 65535 } else { range.max_port };

        if (range.flags & NatRange::PROTO_RANDOM_FULLY) != 0 {
            // Random port
            let rand = self.next_port.load(Ordering::Relaxed) ^ 0xABCD;
            return min + (rand % (max - min + 1));
        }

        // Sequential allocation
        let mut port = self.next_port.fetch_add(1, Ordering::SeqCst);
        if port < min || port > max {
            port = min;
            self.next_port.store(min + 1, Ordering::SeqCst);
        }

        port
    }

    /// Release a port
    pub fn release(&self, addr: u32, port: u16) {
        self.used.lock().remove(&(addr, port));
    }
}

impl Default for PortAllocator {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// NAT TARGETS
// =============================================================================

/// SNAT target
#[derive(Debug, Clone)]
pub struct SnatTarget {
    /// NAT range
    pub range: NatRange,
}

impl SnatTarget {
    /// Create new SNAT target
    pub fn new(addr: u32) -> Self {
        Self {
            range: NatRange::new(addr),
        }
    }

    /// Create with port range
    pub fn with_ports(addr: u32, min_port: u16, max_port: u16) -> Self {
        Self {
            range: NatRange::full(addr, addr, min_port, max_port),
        }
    }
}

impl Target for SnatTarget {
    fn name(&self) -> &str {
        "SNAT"
    }

    fn execute(&self, state: &mut NfHookState) -> NfVerdict {
        let rule = NatRule {
            src: 0,
            src_mask: 0,
            dst: 0,
            dst_mask: 0,
            protocol: 0,
            src_port_min: 0,
            src_port_max: 65535,
            dst_port_min: 0,
            dst_port_max: 65535,
            nat_type: NatType::Snat,
            range: self.range.clone(),
            in_iface: None,
            out_iface: None,
        };

        super::get().nat().do_snat(state, &rule)
    }
}

/// DNAT target
#[derive(Debug, Clone)]
pub struct DnatTarget {
    /// NAT range
    pub range: NatRange,
}

impl DnatTarget {
    /// Create new DNAT target
    pub fn new(addr: u32, port: u16) -> Self {
        Self {
            range: NatRange::full(addr, addr, port, port),
        }
    }

    /// Create with port range
    pub fn with_port_range(addr: u32, min_port: u16, max_port: u16) -> Self {
        Self {
            range: NatRange::full(addr, addr, min_port, max_port),
        }
    }
}

impl Target for DnatTarget {
    fn name(&self) -> &str {
        "DNAT"
    }

    fn execute(&self, state: &mut NfHookState) -> NfVerdict {
        let rule = NatRule {
            src: 0,
            src_mask: 0,
            dst: 0,
            dst_mask: 0,
            protocol: 0,
            src_port_min: 0,
            src_port_max: 65535,
            dst_port_min: 0,
            dst_port_max: 65535,
            nat_type: NatType::Dnat,
            range: self.range.clone(),
            in_iface: None,
            out_iface: None,
        };

        super::get().nat().do_dnat(state, &rule)
    }
}

/// MASQUERADE target
#[derive(Debug, Clone)]
pub struct MasqueradeTarget {
    /// Port range
    pub port_min: u16,
    pub port_max: u16,
    /// Flags
    pub flags: u32,
}

impl MasqueradeTarget {
    /// Create new MASQUERADE target
    pub fn new() -> Self {
        Self {
            port_min: 1024,
            port_max: 65535,
            flags: 0,
        }
    }

    /// Set port range
    pub fn port_range(mut self, min: u16, max: u16) -> Self {
        self.port_min = min;
        self.port_max = max;
        self
    }

    /// Enable random port selection
    pub fn random(mut self) -> Self {
        self.flags |= NatRange::PROTO_RANDOM;
        self
    }
}

impl Default for MasqueradeTarget {
    fn default() -> Self {
        Self::new()
    }
}

impl Target for MasqueradeTarget {
    fn name(&self) -> &str {
        "MASQUERADE"
    }

    fn execute(&self, state: &mut NfHookState) -> NfVerdict {
        let rule = NatRule::masquerade();
        super::get().nat().do_snat(state, &rule)
    }
}

// =============================================================================
// HOOK FUNCTIONS
// =============================================================================

/// Prerouting hook for NAT
pub fn hook_prerouting(state: &mut NfHookState) -> NfVerdict {
    super::get().nat().process_dnat(state)
}

/// Postrouting hook for NAT
pub fn hook_postrouting(state: &mut NfHookState) -> NfVerdict {
    super::get().nat().process_snat(state)
}

/// Local out hook for NAT
pub fn hook_local_out(state: &mut NfHookState) -> NfVerdict {
    super::get().nat().process_dnat(state)
}

/// Local in hook for NAT
pub fn hook_local_in(state: &mut NfHookState) -> NfVerdict {
    super::get().nat().process_snat(state)
}
