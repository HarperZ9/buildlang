// ===============================================================================
// BUILDOS KERNEL - NETFILTER CHAINS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Netfilter Chains
//!
//! Chain management for packet filtering rules

#![allow(dead_code)]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering};

use super::rules::Rule;
use super::{NfHook, NfHookState, NfVerdict};
use crate::sync::RwLock;

// =============================================================================
// CHAIN TYPE
// =============================================================================

/// Chain type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainType {
    /// Built-in chain (INPUT, OUTPUT, FORWARD, etc.)
    Builtin,
    /// User-defined chain
    User,
}

// =============================================================================
// CHAIN STRUCTURE
// =============================================================================

/// Netfilter chain
pub struct Chain {
    /// Chain name
    name: String,
    /// Chain type
    chain_type: ChainType,
    /// Associated hook (for built-in chains)
    hook: Option<NfHook>,
    /// Rules in this chain
    rules: RwLock<Vec<Rule>>,
    /// Default policy (for built-in chains)
    policy: AtomicI32,
    /// Reference count (how many rules jump to this chain)
    references: AtomicU32,
    /// Statistics
    stats: ChainStats,
}

/// Chain statistics
#[derive(Default)]
pub struct ChainStats {
    /// Packets evaluated
    pub packets: AtomicU64,
    /// Bytes evaluated
    pub bytes: AtomicU64,
    /// Policy hits
    pub policy_hits: AtomicU64,
}

impl Chain {
    /// Create new built-in chain
    pub fn new_builtin(name: &str, hook: NfHook) -> Self {
        Self {
            name: String::from(name),
            chain_type: ChainType::Builtin,
            hook: Some(hook),
            rules: RwLock::new(Vec::new()),
            policy: AtomicI32::new(NfVerdict::Accept as i32),
            references: AtomicU32::new(0),
            stats: ChainStats::default(),
        }
    }

    /// Create new user chain
    pub fn new_user(name: &str) -> Self {
        Self {
            name: String::from(name),
            chain_type: ChainType::User,
            hook: None,
            rules: RwLock::new(Vec::new()),
            policy: AtomicI32::new(NfVerdict::Continue as i32),
            references: AtomicU32::new(0),
            stats: ChainStats::default(),
        }
    }

    /// Get chain name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Check if built-in chain
    pub fn is_builtin(&self) -> bool {
        self.chain_type == ChainType::Builtin
    }

    /// Check if chain is empty
    pub fn is_empty(&self) -> bool {
        self.rules.read().is_empty()
    }

    /// Get rule count
    pub fn rule_count(&self) -> usize {
        self.rules.read().len()
    }

    /// Get policy
    pub fn policy(&self) -> NfVerdict {
        let val = self.policy.load(Ordering::SeqCst);
        match val {
            0 => NfVerdict::Drop,
            1 => NfVerdict::Accept,
            _ => NfVerdict::Accept,
        }
    }

    /// Set policy
    pub fn set_policy(&self, policy: NfVerdict) {
        self.policy.store(policy as i32, Ordering::SeqCst);
    }

    /// Append rule
    pub fn append(&self, rule: Rule) {
        self.rules.write().push(rule);
    }

    /// Insert rule at position (1-based)
    pub fn insert(&self, pos: u32, rule: Rule) {
        let mut rules = self.rules.write();
        let idx = (pos.saturating_sub(1) as usize).min(rules.len());
        rules.insert(idx, rule);
    }

    /// Delete rule at position (1-based)
    pub fn delete(&self, pos: u32) -> Option<Rule> {
        let mut rules = self.rules.write();
        let idx = pos.saturating_sub(1) as usize;
        if idx < rules.len() {
            Some(rules.remove(idx))
        } else {
            None
        }
    }

    /// Flush all rules
    pub fn flush(&self) {
        self.rules.write().clear();
    }

    /// Get all rules
    pub fn get_rules(&self) -> Vec<Rule> {
        self.rules.read().clone()
    }

    /// Increment reference count
    pub fn add_reference(&self) {
        self.references.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrement reference count
    pub fn remove_reference(&self) {
        self.references.fetch_sub(1, Ordering::SeqCst);
    }

    /// Get reference count
    pub fn reference_count(&self) -> u32 {
        self.references.load(Ordering::SeqCst)
    }

    /// Check if chain references another chain
    pub fn references_chain(&self, name: &str) -> bool {
        self.rules.read().iter().any(|r| r.jumps_to(name))
    }

    /// Evaluate chain against packet
    pub fn evaluate(
        &self,
        state: &mut NfHookState,
        all_chains: &BTreeMap<String, Arc<Chain>>,
    ) -> NfVerdict {
        self.stats.packets.fetch_add(1, Ordering::Relaxed);
        self.stats.bytes.fetch_add(state.skb.len as u64, Ordering::Relaxed);

        let rules = self.rules.read();

        for rule in rules.iter() {
            if !rule.matches(state) {
                continue;
            }

            rule.increment_counters(state.skb.len);

            match rule.target() {
                RuleTarget::Accept => return NfVerdict::Accept,
                RuleTarget::Drop => return NfVerdict::Drop,
                RuleTarget::Return => break, // Return to calling chain
                RuleTarget::Jump(chain_name) => {
                    // Jump to user chain
                    if let Some(target_chain) = all_chains.get(chain_name) {
                        let verdict = target_chain.evaluate(state, all_chains);
                        match verdict {
                            NfVerdict::Accept | NfVerdict::Drop => return verdict,
                            NfVerdict::Continue => continue, // Return from chain
                            _ => continue,
                        }
                    }
                }
                RuleTarget::Goto(chain_name) => {
                    // Goto (no return)
                    if let Some(target_chain) = all_chains.get(chain_name) {
                        return target_chain.evaluate(state, all_chains);
                    }
                }
                RuleTarget::Queue(num) => {
                    // Queue to userspace (would need NFQUEUE)
                    let _ = num;
                    return NfVerdict::Queue;
                }
                RuleTarget::Continue => continue,
                RuleTarget::Custom(target) => {
                    // Custom target (SNAT, DNAT, MASQUERADE, etc.)
                    let verdict = target.execute(state);
                    match verdict {
                        NfVerdict::Accept | NfVerdict::Drop => return verdict,
                        _ => continue,
                    }
                }
            }
        }

        // No rule matched, apply policy (for built-in) or return (for user)
        if self.is_builtin() {
            self.stats.policy_hits.fetch_add(1, Ordering::Relaxed);
            self.policy()
        } else {
            NfVerdict::Continue
        }
    }

    /// Zero counters
    pub fn zero_counters(&self) {
        self.stats.packets.store(0, Ordering::SeqCst);
        self.stats.bytes.store(0, Ordering::SeqCst);
        self.stats.policy_hits.store(0, Ordering::SeqCst);

        for rule in self.rules.write().iter_mut() {
            rule.zero_counters();
        }
    }
}

// =============================================================================
// RULE TARGET
// =============================================================================

/// Rule target action
#[derive(Debug, Clone)]
pub enum RuleTarget {
    /// Accept packet
    Accept,
    /// Drop packet
    Drop,
    /// Return to calling chain
    Return,
    /// Jump to user chain (with return)
    Jump(String),
    /// Goto user chain (no return)
    Goto(String),
    /// Queue to userspace
    Queue(u16),
    /// Continue to next rule
    Continue,
    /// Custom target
    Custom(Arc<dyn Target>),
}

/// Custom target trait
pub trait Target: Send + Sync + core::fmt::Debug {
    /// Target name
    fn name(&self) -> &str;

    /// Execute target action
    fn execute(&self, state: &mut NfHookState) -> NfVerdict;
}

// =============================================================================
// CHAIN COUNTERS
// =============================================================================

/// Chain counters for a specific chain
#[derive(Debug, Clone, Default)]
pub struct ChainCounters {
    /// Packet count
    pub packets: u64,
    /// Byte count
    pub bytes: u64,
}

/// Get counters for all chains in a table
pub fn get_chain_counters(chains: &BTreeMap<String, Arc<Chain>>) -> BTreeMap<String, ChainCounters> {
    let mut counters = BTreeMap::new();

    for (name, chain) in chains.iter() {
        counters.insert(
            name.clone(),
            ChainCounters {
                packets: chain.stats.packets.load(Ordering::Relaxed),
                bytes: chain.stats.bytes.load(Ordering::Relaxed),
            },
        );
    }

    counters
}
