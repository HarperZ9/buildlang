// ===============================================================================
// BUILDOS KERNEL - NFTABLES
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.

//! nftables Implementation
//!
//! Modern netfilter configuration framework:
//! - Tables with arbitrary chains
//! - Sets and maps for efficient matching
//! - Verdict maps
//! - Concatenated ranges
//! - Stateful objects
#![allow(dead_code)]
extern crate alloc;
use alloc::collections::BTreeMap;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use super::{NfError, NfHook, NfHookState, NfProto, NfVerdict};
use crate::sync::RwLock;
// =============================================================================
// NFT TABLE
/// nftables table
pub struct NftTable {
    /// Table name
    pub name: String,
    /// Table family
    pub family: NfProto,
    /// Chains
    chains: RwLock<BTreeMap<String, Arc<NftChain>>>,
    /// Sets
    sets: RwLock<BTreeMap<String, Arc<NftSet>>>,
    /// Flowtables
    flowtables: RwLock<BTreeMap<String, Arc<NftFlowtable>>>,
    /// Objects (counters, quotas, etc.)
    objects: RwLock<BTreeMap<String, Arc<NftObject>>>,
    /// Flags
    pub flags: u32,
    /// Handle
    pub handle: u64,
}
impl NftTable {
    /// Dormant flag (table is inactive)
    pub const FLAG_DORMANT: u32 = 1 << 0;
    /// Owner flag (has owner)
    pub const FLAG_OWNER: u32 = 1 << 1;
    /// Create new table
    pub fn new(name: &str, family: NfProto) -> Self {
        Self {
            name: String::from(name),
            family,
            chains: RwLock::new(BTreeMap::new()),
            sets: RwLock::new(BTreeMap::new()),
            flowtables: RwLock::new(BTreeMap::new()),
            objects: RwLock::new(BTreeMap::new()),
            flags: 0,
            handle: 0,
        }
    }
    /// Add chain
    pub fn add_chain(&self, chain: NftChain) -> Result<(), NfError> {
        let name = chain.name.clone();
        let mut chains = self.chains.write();
        if chains.contains_key(&name) {
            return Err(NfError::ChainExists);
        }
        chains.insert(name, Arc::new(chain));
        Ok(())
    }

    /// Get chain
    pub fn get_chain(&self, name: &str) -> Option<Arc<NftChain>> {
        self.chains.read().get(name).cloned()
    }

    /// Delete chain
    pub fn delete_chain(&self, name: &str) -> Result<(), NfError> {
        let mut chains = self.chains.write();
        if !chains.contains_key(name) {
            return Err(NfError::ChainNotFound);
        }
        chains.remove(name);
        Ok(())
    }

    /// Add set
    pub fn add_set(&self, set: NftSet) -> Result<(), NfError> {
        let name = set.name.clone();
        let mut sets = self.sets.write();
        if sets.contains_key(&name) {
            return Err(NfError::InvalidArg);
        }
        sets.insert(name, Arc::new(set));
        Ok(())
    }

    /// Get set
    pub fn get_set(&self, name: &str) -> Option<Arc<NftSet>> {
        self.sets.read().get(name).cloned()
    }

    /// List chains
    pub fn list_chains(&self) -> Vec<String> {
        self.chains.read().keys().cloned().collect()
    }

    /// List sets
    pub fn list_sets(&self) -> Vec<String> {
        self.sets.read().keys().cloned().collect()
    }

    /// Set dormant flag
    pub fn set_dormant(&mut self, dormant: bool) {
        if dormant {
            self.flags |= Self::FLAG_DORMANT;
        } else {
            self.flags &= !Self::FLAG_DORMANT;
        }
    }

    /// Check if dormant
    pub fn is_dormant(&self) -> bool {
        (self.flags & Self::FLAG_DORMANT) != 0
    }
}

// NFT CHAIN
/// nftables chain type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NftChainType {
    /// Filter chain
    Filter,
    /// NAT chain
    Nat,
    /// Route chain
    Route,
}

/// nftables chain
pub struct NftChain {
    /// Chain name
    pub name: String,
    /// Chain type
    pub chain_type: Option<NftChainType>,
    /// Hook
    pub hook: Option<NfHook>,
    /// Priority
    pub priority: i32,
    /// Policy
    pub policy: NfVerdict,
    /// Rules
    rules: RwLock<Vec<NftRule>>,
    /// Use count
    use_count: AtomicU32,
    /// Counters
    pub packets: AtomicU64,
    pub bytes: AtomicU64,
}

impl NftChain {
    /// Create new base chain
    pub fn new_base(
        name: &str,
        chain_type: NftChainType,
        hook: NfHook,
        priority: i32,
    ) -> Self {
        Self {
            name: String::from(name),
            chain_type: Some(chain_type),
            hook: Some(hook),
            priority,
            policy: NfVerdict::Accept,
            rules: RwLock::new(Vec::new()),
            use_count: AtomicU32::new(0),
            packets: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    /// Create new regular chain
    pub fn new_regular(name: &str) -> Self {
        Self {
            name: String::from(name),
            chain_type: None,
            hook: None,
            priority: 0,
            policy: NfVerdict::Accept,
            rules: RwLock::new(Vec::new()),
            use_count: AtomicU32::new(0),
            packets: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    /// Is this a base chain?
    pub fn is_base(&self) -> bool {
        self.hook.is_some()
    }

    /// Set policy
    pub fn set_policy(&mut self, policy: NfVerdict) {
        self.policy = policy;
    }

    /// Add rule
    pub fn add_rule(&self, rule: NftRule) {
        self.rules.write().push(rule);
    }

    /// Insert rule at position
    pub fn insert_rule(&self, pos: usize, rule: NftRule) {
        let mut rules = self.rules.write();
        let pos = pos.min(rules.len());
        rules.insert(pos, rule);
    }

    /// Delete rule by handle
    pub fn delete_rule(&self, handle: u64) -> bool {
        let mut rules = self.rules.write();
        let len_before = rules.len();
        rules.retain(|r| r.handle != handle);
        rules.len() < len_before
    }

    /// Flush all rules
    pub fn flush(&self) {
        self.rules.write().clear();
    }

    /// Evaluate chain
    pub fn evaluate(&self, state: &mut NfHookState) -> NfVerdict {
        self.packets.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(state.skb.len as u64, Ordering::Relaxed);
        let rules = self.rules.read();
        for rule in rules.iter() {
            let verdict = rule.evaluate(state);
            match verdict {
                NfVerdict::Accept | NfVerdict::Drop => return verdict,
                NfVerdict::Continue => continue,
                _ => return verdict,
            }
        }
        self.policy
    }
}

// NFT RULE
/// nftables rule
pub struct NftRule {
    /// Rule handle
    pub handle: u64,
    /// Position handle
    pub position: u64,
    /// Expressions
    expressions: Vec<Box<dyn NftExpr>>,
    /// Comment
    pub comment: Option<String>,
    /// Packet counter
    pub packets: AtomicU64,
    /// Byte counter
    pub bytes: AtomicU64,
}

impl NftRule {
    /// Create new rule
    pub fn new(handle: u64) -> Self {
        Self {
            handle,
            position: 0,
            expressions: Vec::new(),
            comment: None,
            packets: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    /// Add expression
    pub fn add_expr(&mut self, expr: Box<dyn NftExpr>) {
        self.expressions.push(expr);
    }

    /// Set comment
    pub fn set_comment(&mut self, comment: &str) {
        self.comment = Some(String::from(comment));
    }

    /// Evaluate rule
    pub fn evaluate(&self, state: &mut NfHookState) -> NfVerdict {
        for expr in &self.expressions {
            let verdict = expr.eval(state);
            match verdict {
                NftExprResult::Continue => continue,
                NftExprResult::Break => break,
                NftExprResult::Verdict(v) => {
                    self.packets.fetch_add(1, Ordering::Relaxed);
                    self.bytes.fetch_add(state.skb.len as u64, Ordering::Relaxed);
                    return v;
                }
            }
        }
        NfVerdict::Continue
    }
}

// NFT EXPRESSION
/// Expression evaluation result
pub enum NftExprResult {
    /// Continue to next expression
    Continue,
    /// Break (match failed)
    Break,
    /// Return verdict
    Verdict(NfVerdict),
}

/// nftables expression trait
pub trait NftExpr: Send + Sync {
    /// Expression name
    fn name(&self) -> &str;
    /// Evaluate expression
    fn eval(&self, state: &mut NfHookState) -> NftExprResult;
}

// COMMON EXPRESSIONS
/// Payload expression - extract data from packet
#[derive(Debug)]
pub struct PayloadExpr {
    /// Base (network/transport/link)
    pub base: PayloadBase,
    /// Offset from base
    pub offset: u32,
    /// Length to extract
    pub len: u32,
    /// Destination register
    pub dreg: u32,
}

/// Payload base
#[derive(Debug, Clone, Copy)]
pub enum PayloadBase {
    /// Link layer header
    Link,
    /// Network layer header
    Network,
    /// Transport layer header
    Transport,
    /// Inner link
    InnerLink,
    /// Inner network
    InnerNetwork,
    /// Inner transport
    InnerTransport,
}

impl NftExpr for PayloadExpr {
    fn name(&self) -> &str {
        "payload"
    }

    fn eval(&self, _state: &mut NfHookState) -> NftExprResult {
        // Would extract payload data and store in register
        NftExprResult::Continue
    }
}

/// Compare expression
pub struct CmpExpr {
    /// Source register
    pub sreg: u32,
    /// Operation
    pub op: CmpOp,
    /// Data to compare
    pub data: Vec<u8>,
}

/// Comparison operation
pub enum CmpOp {
    /// Equal
    Eq,
    /// Not equal
    Ne,
    /// Less than
    Lt,
    /// Less than or equal
    Le,
    /// Greater than
    Gt,
    /// Greater than or equal
    Ge,
}

impl NftExpr for CmpExpr {
    fn name(&self) -> &str {
        "cmp"
    }

    fn eval(&self, _state: &mut NfHookState) -> NftExprResult {
        // Would compare register with data
        NftExprResult::Continue
    }
}

/// Immediate expression - load immediate value
pub struct ImmediateExpr {
    /// Data
    pub data: Vec<u8>,
    /// Destination register
    pub dreg: u32,
}

impl NftExpr for ImmediateExpr {
    fn name(&self) -> &str {
        "immediate"
    }

    fn eval(&self, _state: &mut NfHookState) -> NftExprResult {
        // Would load data into register
        NftExprResult::Continue
    }
}

/// Verdict expression
pub struct VerdictExpr {
    /// Verdict
    pub verdict: NfVerdict,
    /// Chain name (for jump/goto)
    pub chain: Option<String>,
}

impl VerdictExpr {
    /// Accept
    pub fn accept() -> Self {
        Self { verdict: NfVerdict::Accept, chain: None }
    }

    /// Drop
    pub fn drop() -> Self {
        Self { verdict: NfVerdict::Drop, chain: None }
    }

    /// Continue
    pub fn cont() -> Self {
        Self { verdict: NfVerdict::Continue, chain: None }
    }
}

impl NftExpr for VerdictExpr {
    fn name(&self) -> &str {
        "verdict"
    }

    fn eval(&self, _state: &mut NfHookState) -> NftExprResult {
        NftExprResult::Verdict(self.verdict)
    }
}

/// Counter expression
#[derive(Debug, Default)]
pub struct CounterExpr {
    /// Packet count
    pub packets: AtomicU64,
    /// Byte count
    pub bytes: AtomicU64,
}

impl NftExpr for CounterExpr {
    fn name(&self) -> &str {
        "counter"
    }

    fn eval(&self, state: &mut NfHookState) -> NftExprResult {
        self.packets.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(state.skb.len as u64, Ordering::Relaxed);
        NftExprResult::Continue
    }
}

/// Lookup expression - lookup in set
pub struct LookupExpr {
    /// Set name
    pub set: String,
    /// Destination register (for maps)
    pub dreg: Option<u32>,
    /// Invert match
    pub invert: bool,
}

impl NftExpr for LookupExpr {
    fn name(&self) -> &str {
        "lookup"
    }

    fn eval(&self, _state: &mut NfHookState) -> NftExprResult {
        // Would lookup value in set
        NftExprResult::Continue
    }
}

/// Log expression
pub struct LogExpr {
    /// Prefix
    pub prefix: String,
    /// Log group
    pub group: u16,
    /// Log level
    pub level: u8,
}

impl NftExpr for LogExpr {
    fn name(&self) -> &str {
        "log"
    }

    fn eval(&self, _state: &mut NfHookState) -> NftExprResult {
        // Would log packet
        NftExprResult::Continue
    }
}

/// Limit expression
pub struct LimitExpr {
    /// Rate
    pub rate: u64,
    /// Unit
    pub unit: LimitUnit,
    /// Burst
    pub burst: u32,
    /// Invert
    pub invert: bool,
    /// Current tokens
    tokens: AtomicU64,
    /// Last update
    last: AtomicU64,
}

/// Limit unit
pub enum LimitUnit {
    /// Per second
    Second,
    /// Per minute
    Minute,
    /// Per hour
    Hour,
    /// Per day
    Day,
}

impl NftExpr for LimitExpr {
    fn name(&self) -> &str {
        "limit"
    }

    fn eval(&self, _state: &mut NfHookState) -> NftExprResult {
        // Would check rate limit
        NftExprResult::Continue
    }
}

// NFT SET
/// Set key type
pub enum NftSetType {
    /// IPv4 address
    Ipv4Addr,
    /// IPv6 address
    Ipv6Addr,
    /// Ethernet address
    EtherAddr,
    /// Port
    InetService,
    /// Protocol
    InetProto,
    /// Mark
    Mark,
    /// Interface index
    Ifindex,
    /// Interface name
    Ifname,
}

/// nftables set
pub struct NftSet {
    /// Set name
    pub name: String,
    /// Key type
    pub key_type: NftSetType,
    /// Key length
    pub key_len: u32,
    /// Data type (for maps)
    pub data_type: Option<NftSetType>,
    /// Data length
    pub data_len: u32,
    /// Timeout (0 = none)
    pub timeout: u64,
    /// GC interval
    pub gc_interval: u64,
    /// Elements
    elements: RwLock<BTreeMap<Vec<u8>, NftSetElem>>,
}

impl NftSet {
    /// Anonymous set
    pub const FLAG_ANONYMOUS: u32 = 1 << 0;
    /// Constant set
    pub const FLAG_CONSTANT: u32 = 1 << 1;
    /// Interval set
    pub const FLAG_INTERVAL: u32 = 1 << 2;
    /// Map (has data)
    pub const FLAG_MAP: u32 = 1 << 3;
    /// Timeout
    pub const FLAG_TIMEOUT: u32 = 1 << 4;
    /// Dynamic
    pub const FLAG_DYNAMIC: u32 = 1 << 5;
    /// Create new set
    pub fn new(name: &str, key_type: NftSetType, key_len: u32) -> Self {
        Self {
            name: String::from(name),
            key_type,
            key_len,
            data_type: None,
            data_len: 0,
            timeout: 0,
            gc_interval: 0,
            elements: RwLock::new(BTreeMap::new()),
        }
    }

    /// Create map
    pub fn new_map(
        name: &str,
        key_type: NftSetType,
        key_len: u32,
        data_type: NftSetType,
        data_len: u32,
    ) -> Self {
        Self {
            name: String::from(name),
            key_type,
            key_len,
            data_type: Some(data_type),
            data_len,
            timeout: 0,
            gc_interval: 0,
            elements: RwLock::new(BTreeMap::new()),
        }
    }

    /// Add element
    pub fn add_elem(&self, elem: NftSetElem) {
        self.elements.write().insert(elem.key.clone(), elem);
    }

    /// Remove element
    pub fn del_elem(&self, key: &[u8]) -> bool {
        self.elements.write().remove(key).is_some()
    }

    /// Lookup element
    pub fn lookup(&self, key: &[u8]) -> Option<NftSetElem> {
        self.elements.read().get(key).cloned()
    }

    /// Check if key exists
    pub fn contains(&self, key: &[u8]) -> bool {
        self.elements.read().contains_key(key)
    }

    /// Flush all elements
    pub fn flush(&self) {
        self.elements.write().clear();
    }

    /// Get element count
    pub fn count(&self) -> usize {
        self.elements.read().len()
    }

    /// List elements
    pub fn list(&self) -> Vec<NftSetElem> {
        self.elements.read().values().cloned().collect()
    }
}

/// Set element
#[derive(Debug, Clone)]
pub struct NftSetElem {
    /// Key
    pub key: Vec<u8>,
    /// Key end (for intervals)
    pub key_end: Option<Vec<u8>>,
    /// Data (for maps)
    pub data: Option<Vec<u8>>,
    /// Expiration time
    pub expires: u64,
}

impl NftSetElem {
    /// Interval end flag
    pub const FLAG_INTERVAL_END: u32 = 1 << 0;

    /// Create new element
    pub fn new(key: Vec<u8>) -> Self {
        Self {
            key,
            key_end: None,
            data: None,
            expires: 0,
        }
    }

    /// Create map element
    pub fn map(key: Vec<u8>, data: Vec<u8>) -> Self {
        Self {
            key,
            key_end: None,
            data: Some(data),
            expires: 0,
        }
    }

    /// Create interval element
    pub fn interval(start: Vec<u8>, end: Vec<u8>) -> Self {
        Self {
            key: start,
            key_end: Some(end),
            data: None,
            expires: 0,
        }
    }
}

// NFT FLOWTABLE
/// nftables flowtable (hardware offload)
pub struct NftFlowtable {
    /// Name
    pub name: String,
    /// Hook
    pub hook: NfHook,
    /// Priority
    pub priority: i32,
    /// Device names
    pub devices: Vec<String>,
}

impl NftFlowtable {
    /// Hardware offload
    pub const FLAG_HW_OFFLOAD: u32 = 1 << 0;
    /// Counter
    pub const FLAG_COUNTER: u32 = 1 << 1;

    /// Create new flowtable
    pub fn new(name: &str, hook: NfHook, priority: i32) -> Self {
        Self {
            name: String::from(name),
            hook,
            priority,
            devices: Vec::new(),
        }
    }

    /// Add device
    pub fn add_device(&mut self, dev: &str) {
        self.devices.push(String::from(dev));
    }
}

// NFT OBJECT
/// nftables stateful object
pub struct NftObject {
    /// Name
    pub name: String,
    /// Type
    pub obj_type: NftObjType,
    /// Object-specific data
    pub data: NftObjData,
}

/// Object type
pub enum NftObjType {
    /// Counter
    Counter,
    /// Quota
    Quota,
    /// Connection tracking helper
    CtHelper,
    /// Limit
    Limit,
    /// Connection tracking timeout
    CtTimeout,
    /// Secmark
    Secmark,
    /// Synproxy
    Synproxy,
}

/// Object data
pub enum NftObjData {
    /// Counter data
    Counter { packets: AtomicU64, bytes: AtomicU64 },
    /// Quota data
    Quota { bytes: u64, used: AtomicU64, flags: u32 },
    /// Limit data
    Limit { rate: u64, unit: LimitUnit, burst: u32 },
    /// CT helper
    CtHelper { name: String, l4proto: u8 },
    /// Other
    Other,
}

impl NftObject {
    /// Create counter object
    pub fn counter(name: &str) -> Self {
        Self {
            name: String::from(name),
            obj_type: NftObjType::Counter,
            data: NftObjData::Counter {
                packets: AtomicU64::new(0),
                bytes: AtomicU64::new(0),
            },
        }
    }

    /// Create quota object
    pub fn quota(name: &str, bytes: u64) -> Self {
        Self {
            name: String::from(name),
            obj_type: NftObjType::Quota,
            data: NftObjData::Quota {
                bytes,
                used: AtomicU64::new(0),
                flags: 0,
            },
        }
    }
}

// NFT CONTEXT
/// nftables context (transaction/batch)
pub struct NftContext {
    /// Tables
    tables: RwLock<BTreeMap<(NfProto, String), Arc<NftTable>>>,
    /// Next handle
    next_handle: AtomicU64,
    /// Generation counter
    generation: AtomicU64,
}

impl NftContext {
    /// Create new context
    pub fn new() -> Self {
        Self {
            tables: RwLock::new(BTreeMap::new()),
            next_handle: AtomicU64::new(1),
            generation: AtomicU64::new(0),
        }
    }

    /// Allocate handle
    pub fn alloc_handle(&self) -> u64 {
        self.next_handle.fetch_add(1, Ordering::SeqCst)
    }

    /// Add table
    pub fn add_table(&self, table: NftTable) -> Result<u64, NfError> {
        let key = (table.family, table.name.clone());
        let handle = self.alloc_handle();
        let mut tables = self.tables.write();
        if tables.contains_key(&key) {
            return Err(NfError::TableExists);
        }
        tables.insert(key, Arc::new(table));
        self.generation.fetch_add(1, Ordering::SeqCst);
        Ok(handle)
    }

    /// Get table
    pub fn get_table(&self, family: NfProto, name: &str) -> Option<Arc<NftTable>> {
        self.tables.read().get(&(family, String::from(name))).cloned()
    }

    /// Delete table
    pub fn del_table(&self, family: NfProto, name: &str) -> Result<(), NfError> {
        let key = (family, String::from(name));
        let mut tables = self.tables.write();
        if !tables.contains_key(&key) {
            return Err(NfError::TableNotFound);
        }
        tables.remove(&key);
        Ok(())
    }

    /// List tables
    pub fn list_tables(&self) -> Vec<(NfProto, String)> {
        self.tables.read().keys().cloned().collect()
    }

    /// Flush all tables
    pub fn flush(&self) {
        self.tables.write().clear();
    }

    /// Get generation
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }
}

impl Default for NftContext {
    fn default() -> Self {
        Self::new()
    }
}

// GLOBAL INSTANCE
use spin::Once;
static NFT_CTX: Once<NftContext> = Once::new();

/// Initialize nftables
pub fn init() {
    NFT_CTX.call_once(NftContext::new);
}

/// Get nftables context
pub fn get() -> &'static NftContext {
    NFT_CTX.get().expect("nftables not initialized")
}
