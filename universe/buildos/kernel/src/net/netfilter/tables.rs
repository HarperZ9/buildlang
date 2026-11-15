// ===============================================================================
// BUILDOS KERNEL - NETFILTER TABLES
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Netfilter Tables
//!
//! Implements iptables-style tables:
//! - filter: Default table for packet filtering
//! - nat: Network Address Translation
//! - mangle: Packet alteration
//! - raw: Early packet processing

#![allow(dead_code)]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use super::chains::Chain;
use super::rules::Rule;
use super::{NfError, NfHook, NfVerdict};
use crate::sync::RwLock;

// =============================================================================
// TABLE TYPES
// =============================================================================

/// Table type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableType {
    /// Packet filtering
    Filter,
    /// Network Address Translation
    Nat,
    /// Packet mangling
    Mangle,
    /// Raw processing (before conntrack)
    Raw,
    /// Security (SELinux)
    Security,
}

impl TableType {
    /// Get valid hooks for this table type
    pub fn valid_hooks(&self) -> &'static [NfHook] {
        match self {
            TableType::Filter => &[NfHook::LocalIn, NfHook::Forward, NfHook::LocalOut],
            TableType::Nat => &[
                NfHook::PreRouting,
                NfHook::LocalIn,
                NfHook::LocalOut,
                NfHook::PostRouting,
            ],
            TableType::Mangle => &[
                NfHook::PreRouting,
                NfHook::LocalIn,
                NfHook::Forward,
                NfHook::LocalOut,
                NfHook::PostRouting,
            ],
            TableType::Raw => &[NfHook::PreRouting, NfHook::LocalOut],
            TableType::Security => &[NfHook::LocalIn, NfHook::Forward, NfHook::LocalOut],
        }
    }

    /// Get table name
    pub fn name(&self) -> &'static str {
        match self {
            TableType::Filter => "filter",
            TableType::Nat => "nat",
            TableType::Mangle => "mangle",
            TableType::Raw => "raw",
            TableType::Security => "security",
        }
    }
}

// =============================================================================
// TABLE STRUCTURE
// =============================================================================

/// Netfilter table
pub struct Table {
    /// Table name
    pub name: String,
    /// Table type
    pub table_type: TableType,
    /// Chains in this table
    chains: RwLock<BTreeMap<String, Arc<Chain>>>,
    /// Valid hook points
    valid_hooks: Vec<NfHook>,
    /// Statistics
    stats: TableStats,
}

/// Table statistics
#[derive(Default)]
pub struct TableStats {
    /// Packets matched
    pub packets: AtomicU64,
    /// Bytes matched
    pub bytes: AtomicU64,
}

impl Table {
    /// Create new table
    pub fn new(name: &str, table_type: TableType) -> Self {
        let valid_hooks = table_type.valid_hooks().to_vec();

        let table = Self {
            name: String::from(name),
            table_type,
            chains: RwLock::new(BTreeMap::new()),
            valid_hooks,
            stats: TableStats::default(),
        };

        // Create built-in chains
        table.init_builtin_chains();

        table
    }

    /// Create filter table
    pub fn new_filter() -> Self {
        Self::new("filter", TableType::Filter)
    }

    /// Create nat table
    pub fn new_nat() -> Self {
        Self::new("nat", TableType::Nat)
    }

    /// Create mangle table
    pub fn new_mangle() -> Self {
        Self::new("mangle", TableType::Mangle)
    }

    /// Create raw table
    pub fn new_raw() -> Self {
        Self::new("raw", TableType::Raw)
    }

    /// Initialize built-in chains
    fn init_builtin_chains(&self) {
        let mut chains = self.chains.write();

        for hook in &self.valid_hooks {
            let chain_name = hook.name();
            let chain = Chain::new_builtin(chain_name, *hook);
            chains.insert(String::from(chain_name), Arc::new(chain));
        }
    }

    /// Get chain by name
    pub fn get_chain(&self, name: &str) -> Option<Arc<Chain>> {
        self.chains.read().get(name).cloned()
    }

    /// Create user chain
    pub fn create_chain(&self, name: &str) -> Result<(), NfError> {
        let mut chains = self.chains.write();

        if chains.contains_key(name) {
            return Err(NfError::ChainExists);
        }

        let chain = Chain::new_user(name);
        chains.insert(String::from(name), Arc::new(chain));
        Ok(())
    }

    /// Delete user chain
    pub fn delete_chain(&self, name: &str) -> Result<(), NfError> {
        let mut chains = self.chains.write();

        let chain = chains.get(name).ok_or(NfError::ChainNotFound)?;

        if chain.is_builtin() {
            return Err(NfError::PermissionDenied);
        }

        if !chain.is_empty() {
            return Err(NfError::ChainNotEmpty);
        }

        // Check if any other chain references this one
        for other_chain in chains.values() {
            if other_chain.references_chain(name) {
                return Err(NfError::ChainNotEmpty);
            }
        }

        chains.remove(name);
        Ok(())
    }

    /// Append rule to chain
    pub fn append_rule(&self, chain_name: &str, rule: Rule) -> Result<(), NfError> {
        let chains = self.chains.read();
        let chain = chains.get(chain_name).ok_or(NfError::ChainNotFound)?;
        chain.append(rule);
        Ok(())
    }

    /// Insert rule at position
    pub fn insert_rule(&self, chain_name: &str, pos: u32, rule: Rule) -> Result<(), NfError> {
        let chains = self.chains.read();
        let chain = chains.get(chain_name).ok_or(NfError::ChainNotFound)?;
        chain.insert(pos, rule);
        Ok(())
    }

    /// Delete rule by number
    pub fn delete_rule_by_num(&self, chain_name: &str, num: u32) -> Result<(), NfError> {
        let chains = self.chains.read();
        let chain = chains.get(chain_name).ok_or(NfError::ChainNotFound)?;
        chain.delete(num).ok_or(NfError::RuleNotFound)?;
        Ok(())
    }

    /// Flush chain (remove all rules)
    pub fn flush_chain(&self, chain_name: &str) -> Result<(), NfError> {
        let chains = self.chains.read();
        let chain = chains.get(chain_name).ok_or(NfError::ChainNotFound)?;
        chain.flush();
        Ok(())
    }

    /// Set chain policy
    pub fn set_policy(&self, chain_name: &str, policy: NfVerdict) -> Result<(), NfError> {
        let chains = self.chains.read();
        let chain = chains.get(chain_name).ok_or(NfError::ChainNotFound)?;

        if !chain.is_builtin() {
            return Err(NfError::PermissionDenied);
        }

        chain.set_policy(policy);
        Ok(())
    }

    /// List all chains
    pub fn list_chains(&self) -> Vec<String> {
        self.chains.read().keys().cloned().collect()
    }

    /// Check if hook is valid for this table
    pub fn is_valid_hook(&self, hook: NfHook) -> bool {
        self.valid_hooks.contains(&hook)
    }

    /// Process packet through table at hook point
    pub fn process(&self, hook: NfHook, state: &mut super::NfHookState) -> NfVerdict {
        if !self.is_valid_hook(hook) {
            return NfVerdict::Accept;
        }

        let chains = self.chains.read();
        let chain_name = hook.name();

        let Some(chain) = chains.get(chain_name) else {
            return NfVerdict::Accept;
        };

        self.stats.packets.fetch_add(1, Ordering::Relaxed);
        self.stats.bytes.fetch_add(state.skb.len as u64, Ordering::Relaxed);

        chain.evaluate(state, &chains)
    }

    /// Get rule count
    pub fn rule_count(&self) -> usize {
        self.chains.read().values().map(|c| c.rule_count()).sum()
    }

    /// Zero counters
    pub fn zero_counters(&self) {
        self.stats.packets.store(0, Ordering::SeqCst);
        self.stats.bytes.store(0, Ordering::SeqCst);

        for chain in self.chains.read().values() {
            chain.zero_counters();
        }
    }
}

// =============================================================================
// TABLE REGISTRY
// =============================================================================

/// Table registry for managing all tables
pub struct TableRegistry {
    /// Tables by name
    tables: RwLock<BTreeMap<String, Arc<Table>>>,
}

impl TableRegistry {
    /// Create new registry
    pub fn new() -> Self {
        Self {
            tables: RwLock::new(BTreeMap::new()),
        }
    }

    /// Register table
    pub fn register(&self, table: Table) -> Result<(), NfError> {
        let name = table.name.clone();
        let mut tables = self.tables.write();

        if tables.contains_key(&name) {
            return Err(NfError::TableExists);
        }

        tables.insert(name, Arc::new(table));
        Ok(())
    }

    /// Unregister table
    pub fn unregister(&self, name: &str) -> Result<Arc<Table>, NfError> {
        self.tables.write().remove(name).ok_or(NfError::TableNotFound)
    }

    /// Get table by name
    pub fn get(&self, name: &str) -> Option<Arc<Table>> {
        self.tables.read().get(name).cloned()
    }

    /// List all tables
    pub fn list(&self) -> Vec<String> {
        self.tables.read().keys().cloned().collect()
    }
}

impl Default for TableRegistry {
    fn default() -> Self {
        Self::new()
    }
}
