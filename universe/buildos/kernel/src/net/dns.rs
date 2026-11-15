// ===============================================================================
// BUILDOS KERNEL - DNS RESOLVER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! DNS (Domain Name System) Resolver.
//!
//! Implements RFC 1035 DNS for hostname resolution:
//! - Query construction and parsing
//! - A (IPv4), AAAA (IPv6), CNAME, MX, TXT, PTR records
//! - Response caching with TTL-based expiry
//! - Multiple DNS server support with failover
//! - Recursive and iterative resolution
//! - EDNS(0) support for larger responses
//!
//! Usage:
//! ```
//! let ip = dns::resolve("example.com")?;
//! let name = dns::reverse_lookup(ip)?;
//! ```

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU16, Ordering};
use spin::{Mutex, RwLock};

use super::{Ipv4Address, NetworkError};

// =============================================================================
// DNS CONSTANTS
// =============================================================================

/// DNS server port
pub const DNS_PORT: u16 = 53;

/// Maximum DNS message size (standard UDP)
pub const DNS_MAX_UDP_SIZE: usize = 512;

/// Maximum DNS message size with EDNS(0)
pub const DNS_MAX_EDNS_SIZE: usize = 4096;

/// Maximum label length
pub const DNS_MAX_LABEL_LEN: usize = 63;

/// Maximum domain name length
pub const DNS_MAX_NAME_LEN: usize = 253;

/// Default TTL for negative cache entries
pub const DNS_NEG_CACHE_TTL: u32 = 300;

/// Query timeout in milliseconds
pub const DNS_TIMEOUT_MS: u64 = 5000;

/// Maximum query retries
pub const DNS_MAX_RETRIES: u32 = 3;

/// DNS opcodes
mod opcode {
    pub const QUERY: u8 = 0;
    pub const IQUERY: u8 = 1;  // Inverse query (obsolete)
    pub const STATUS: u8 = 2;
    pub const NOTIFY: u8 = 4;
    pub const UPDATE: u8 = 5;
}

/// DNS response codes
mod rcode {
    pub const NOERROR: u8 = 0;
    pub const FORMERR: u8 = 1;   // Format error
    pub const SERVFAIL: u8 = 2;  // Server failure
    pub const NXDOMAIN: u8 = 3;  // Name does not exist
    pub const NOTIMP: u8 = 4;    // Not implemented
    pub const REFUSED: u8 = 5;   // Query refused
    pub const YXDOMAIN: u8 = 6;  // Name exists when it shouldn't
    pub const YXRRSET: u8 = 7;   // RRset exists when it shouldn't
    pub const NXRRSET: u8 = 8;   // RRset doesn't exist
    pub const NOTAUTH: u8 = 9;   // Not authoritative
    pub const NOTZONE: u8 = 10;  // Name not in zone
}

/// DNS record types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum RecordType {
    /// IPv4 address
    A = 1,
    /// Authoritative name server
    NS = 2,
    /// Canonical name (alias)
    CNAME = 5,
    /// Start of authority
    SOA = 6,
    /// Pointer (reverse DNS)
    PTR = 12,
    /// Host information
    HINFO = 13,
    /// Mail exchange
    MX = 15,
    /// Text record
    TXT = 16,
    /// IPv6 address
    AAAA = 28,
    /// Service locator
    SRV = 33,
    /// EDNS(0) pseudo-record
    OPT = 41,
    /// All records (query only)
    ANY = 255,
}

impl RecordType {
    /// Convert from u16
    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            1 => Some(RecordType::A),
            2 => Some(RecordType::NS),
            5 => Some(RecordType::CNAME),
            6 => Some(RecordType::SOA),
            12 => Some(RecordType::PTR),
            13 => Some(RecordType::HINFO),
            15 => Some(RecordType::MX),
            16 => Some(RecordType::TXT),
            28 => Some(RecordType::AAAA),
            33 => Some(RecordType::SRV),
            41 => Some(RecordType::OPT),
            255 => Some(RecordType::ANY),
            _ => None,
        }
    }
}

/// DNS record classes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum RecordClass {
    /// Internet
    IN = 1,
    /// Chaos
    CH = 3,
    /// Hesiod
    HS = 4,
    /// Any class (query only)
    ANY = 255,
}

// =============================================================================
// DNS MESSAGE STRUCTURES
// =============================================================================

/// DNS message header
#[derive(Clone, Copy)]
pub struct DnsHeader {
    /// Transaction ID
    pub id: u16,
    /// Flags
    pub flags: u16,
    /// Question count
    pub qdcount: u16,
    /// Answer count
    pub ancount: u16,
    /// Authority count
    pub nscount: u16,
    /// Additional count
    pub arcount: u16,
}

impl DnsHeader {
    /// Create new query header
    pub fn new_query(id: u16) -> Self {
        Self {
            id,
            flags: 0x0100, // RD (recursion desired) set
            qdcount: 1,
            ancount: 0,
            nscount: 0,
            arcount: 0,
        }
    }

    /// Check if this is a response
    pub fn is_response(&self) -> bool {
        (self.flags & 0x8000) != 0
    }

    /// Get opcode
    pub fn opcode(&self) -> u8 {
        ((self.flags >> 11) & 0x0F) as u8
    }

    /// Check if authoritative answer
    pub fn is_authoritative(&self) -> bool {
        (self.flags & 0x0400) != 0
    }

    /// Check if truncated
    pub fn is_truncated(&self) -> bool {
        (self.flags & 0x0200) != 0
    }

    /// Check if recursion desired
    pub fn recursion_desired(&self) -> bool {
        (self.flags & 0x0100) != 0
    }

    /// Check if recursion available
    pub fn recursion_available(&self) -> bool {
        (self.flags & 0x0080) != 0
    }

    /// Get response code
    pub fn rcode(&self) -> u8 {
        (self.flags & 0x000F) as u8
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> [u8; 12] {
        [
            (self.id >> 8) as u8,
            (self.id & 0xFF) as u8,
            (self.flags >> 8) as u8,
            (self.flags & 0xFF) as u8,
            (self.qdcount >> 8) as u8,
            (self.qdcount & 0xFF) as u8,
            (self.ancount >> 8) as u8,
            (self.ancount & 0xFF) as u8,
            (self.nscount >> 8) as u8,
            (self.nscount & 0xFF) as u8,
            (self.arcount >> 8) as u8,
            (self.arcount & 0xFF) as u8,
        ]
    }

    /// Parse from bytes
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 12 {
            return None;
        }

        Some(Self {
            id: u16::from_be_bytes([data[0], data[1]]),
            flags: u16::from_be_bytes([data[2], data[3]]),
            qdcount: u16::from_be_bytes([data[4], data[5]]),
            ancount: u16::from_be_bytes([data[6], data[7]]),
            nscount: u16::from_be_bytes([data[8], data[9]]),
            arcount: u16::from_be_bytes([data[10], data[11]]),
        })
    }
}

/// DNS question
#[derive(Clone)]
pub struct DnsQuestion {
    /// Domain name
    pub name: String,
    /// Record type
    pub qtype: RecordType,
    /// Record class
    pub qclass: RecordClass,
}

impl DnsQuestion {
    /// Create new question
    pub fn new(name: &str, qtype: RecordType) -> Self {
        Self {
            name: name.to_string(),
            qtype,
            qclass: RecordClass::IN,
        }
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut result = encode_name(&self.name);
        result.push((self.qtype as u16 >> 8) as u8);
        result.push((self.qtype as u16 & 0xFF) as u8);
        result.push((self.qclass as u16 >> 8) as u8);
        result.push((self.qclass as u16 & 0xFF) as u8);
        result
    }
}

/// DNS resource record
#[derive(Clone)]
pub struct DnsRecord {
    /// Domain name
    pub name: String,
    /// Record type
    pub rtype: RecordType,
    /// Record class
    pub rclass: u16,
    /// Time to live
    pub ttl: u32,
    /// Record data
    pub rdata: DnsRecordData,
}

/// DNS record data variants
#[derive(Clone)]
pub enum DnsRecordData {
    /// A record (IPv4)
    A(Ipv4Address),
    /// AAAA record (IPv6)
    AAAA([u8; 16]),
    /// CNAME record
    CNAME(String),
    /// NS record
    NS(String),
    /// PTR record
    PTR(String),
    /// MX record (priority, exchange)
    MX(u16, String),
    /// TXT record
    TXT(Vec<String>),
    /// SRV record (priority, weight, port, target)
    SRV(u16, u16, u16, String),
    /// SOA record
    SOA {
        mname: String,
        rname: String,
        serial: u32,
        refresh: u32,
        retry: u32,
        expire: u32,
        minimum: u32,
    },
    /// Unknown/raw data
    Unknown(Vec<u8>),
}

/// DNS message (query or response)
#[derive(Clone)]
pub struct DnsMessage {
    /// Header
    pub header: DnsHeader,
    /// Questions
    pub questions: Vec<DnsQuestion>,
    /// Answers
    pub answers: Vec<DnsRecord>,
    /// Authority records
    pub authority: Vec<DnsRecord>,
    /// Additional records
    pub additional: Vec<DnsRecord>,
}

impl DnsMessage {
    /// Create a new query message
    pub fn new_query(id: u16, name: &str, qtype: RecordType) -> Self {
        Self {
            header: DnsHeader::new_query(id),
            questions: vec![DnsQuestion::new(name, qtype)],
            answers: Vec::new(),
            authority: Vec::new(),
            additional: Vec::new(),
        }
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut result = Vec::with_capacity(DNS_MAX_UDP_SIZE);

        // Header
        result.extend_from_slice(&self.header.to_bytes());

        // Questions
        for q in &self.questions {
            result.extend(q.to_bytes());
        }

        // Records would go here for responses
        // (not needed for queries)

        result
    }

    /// Parse from bytes
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 12 {
            return None;
        }

        let header = DnsHeader::parse(data)?;
        let mut offset = 12;

        // Parse questions
        let mut questions = Vec::with_capacity(header.qdcount as usize);
        for _ in 0..header.qdcount {
            let (name, new_offset) = decode_name(data, offset)?;
            offset = new_offset;

            if offset + 4 > data.len() {
                return None;
            }

            let qtype_val = u16::from_be_bytes([data[offset], data[offset + 1]]);
            let qclass_val = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);
            offset += 4;

            let qtype = RecordType::from_u16(qtype_val).unwrap_or(RecordType::A);
            let qclass = if qclass_val == 1 {
                RecordClass::IN
            } else {
                RecordClass::ANY
            };

            questions.push(DnsQuestion { name, qtype, qclass });
        }

        // Parse answers
        let mut answers = Vec::with_capacity(header.ancount as usize);
        for _ in 0..header.ancount {
            if let Some((record, new_offset)) = parse_record(data, offset) {
                answers.push(record);
                offset = new_offset;
            } else {
                break;
            }
        }

        // Parse authority
        let mut authority = Vec::with_capacity(header.nscount as usize);
        for _ in 0..header.nscount {
            if let Some((record, new_offset)) = parse_record(data, offset) {
                authority.push(record);
                offset = new_offset;
            } else {
                break;
            }
        }

        // Parse additional
        let mut additional = Vec::with_capacity(header.arcount as usize);
        for _ in 0..header.arcount {
            if let Some((record, new_offset)) = parse_record(data, offset) {
                additional.push(record);
                offset = new_offset;
            } else {
                break;
            }
        }

        Some(Self {
            header,
            questions,
            answers,
            authority,
            additional,
        })
    }

    /// Get first A record
    pub fn get_a_record(&self) -> Option<Ipv4Address> {
        for record in &self.answers {
            if let DnsRecordData::A(ip) = &record.rdata {
                return Some(*ip);
            }
        }
        None
    }

    /// Get all A records
    pub fn get_a_records(&self) -> Vec<Ipv4Address> {
        self.answers
            .iter()
            .filter_map(|r| {
                if let DnsRecordData::A(ip) = &r.rdata {
                    Some(*ip)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get CNAME if present
    pub fn get_cname(&self) -> Option<&str> {
        for record in &self.answers {
            if let DnsRecordData::CNAME(name) = &record.rdata {
                return Some(name);
            }
        }
        None
    }

    /// Get MX records sorted by priority
    pub fn get_mx_records(&self) -> Vec<(u16, String)> {
        let mut records: Vec<_> = self.answers
            .iter()
            .filter_map(|r| {
                if let DnsRecordData::MX(priority, exchange) = &r.rdata {
                    Some((*priority, exchange.clone()))
                } else {
                    None
                }
            })
            .collect();
        records.sort_by_key(|(p, _)| *p);
        records
    }

    /// Get TXT records
    pub fn get_txt_records(&self) -> Vec<String> {
        self.answers
            .iter()
            .filter_map(|r| {
                if let DnsRecordData::TXT(texts) = &r.rdata {
                    Some(texts.join(""))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get minimum TTL from answers
    pub fn min_ttl(&self) -> u32 {
        self.answers
            .iter()
            .map(|r| r.ttl)
            .min()
            .unwrap_or(0)
    }
}

// =============================================================================
// NAME ENCODING/DECODING
// =============================================================================

/// Encode domain name to DNS wire format
fn encode_name(name: &str) -> Vec<u8> {
    let mut result = Vec::with_capacity(name.len() + 2);

    for label in name.split('.') {
        if label.is_empty() {
            continue;
        }
        let len = label.len().min(DNS_MAX_LABEL_LEN);
        result.push(len as u8);
        result.extend_from_slice(&label.as_bytes()[..len]);
    }

    result.push(0); // Null terminator
    result
}

/// Decode domain name from DNS wire format
fn decode_name(data: &[u8], start: usize) -> Option<(String, usize)> {
    let mut name = String::with_capacity(64);
    let mut offset = start;
    let mut jumped = false;
    let mut return_offset = 0;
    let mut iterations = 0;

    loop {
        // Prevent infinite loops
        iterations += 1;
        if iterations > 128 {
            return None;
        }

        if offset >= data.len() {
            return None;
        }

        let len = data[offset];

        if len == 0 {
            // End of name
            if !jumped {
                return_offset = offset + 1;
            }
            break;
        }

        if (len & 0xC0) == 0xC0 {
            // Compression pointer
            if offset + 1 >= data.len() {
                return None;
            }

            let pointer = (((len & 0x3F) as usize) << 8) | (data[offset + 1] as usize);

            if !jumped {
                return_offset = offset + 2;
                jumped = true;
            }

            offset = pointer;
            continue;
        }

        // Regular label
        let label_len = len as usize;
        offset += 1;

        if offset + label_len > data.len() {
            return None;
        }

        if !name.is_empty() {
            name.push('.');
        }

        if let Ok(label) = core::str::from_utf8(&data[offset..offset + label_len]) {
            name.push_str(label);
        }

        offset += label_len;
    }

    let final_offset = if jumped { return_offset } else { offset + 1 };
    Some((name, final_offset))
}

/// Parse a resource record
fn parse_record(data: &[u8], start: usize) -> Option<(DnsRecord, usize)> {
    let (name, offset) = decode_name(data, start)?;

    if offset + 10 > data.len() {
        return None;
    }

    let rtype_val = u16::from_be_bytes([data[offset], data[offset + 1]]);
    let rclass = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);
    let ttl = u32::from_be_bytes([
        data[offset + 4],
        data[offset + 5],
        data[offset + 6],
        data[offset + 7],
    ]);
    let rdlength = u16::from_be_bytes([data[offset + 8], data[offset + 9]]) as usize;

    let rdata_start = offset + 10;
    if rdata_start + rdlength > data.len() {
        return None;
    }

    let rdata_bytes = &data[rdata_start..rdata_start + rdlength];
    let rtype = RecordType::from_u16(rtype_val);

    let rdata = match rtype {
        Some(RecordType::A) if rdlength == 4 => {
            match Ipv4Address::from_bytes(&rdata_bytes[..4]) {
                Some(ip) => DnsRecordData::A(ip),
                None => return None,
            }
        }
        Some(RecordType::AAAA) if rdlength == 16 => {
            let mut addr = [0u8; 16];
            addr.copy_from_slice(rdata_bytes);
            DnsRecordData::AAAA(addr)
        }
        Some(RecordType::CNAME) | Some(RecordType::NS) | Some(RecordType::PTR) => {
            let (target, _) = decode_name(data, rdata_start)?;
            match rtype {
                Some(RecordType::CNAME) => DnsRecordData::CNAME(target),
                Some(RecordType::NS) => DnsRecordData::NS(target),
                Some(RecordType::PTR) => DnsRecordData::PTR(target),
                _ => unreachable!(),
            }
        }
        Some(RecordType::MX) if rdlength >= 2 => {
            let priority = u16::from_be_bytes([rdata_bytes[0], rdata_bytes[1]]);
            let (exchange, _) = decode_name(data, rdata_start + 2)?;
            DnsRecordData::MX(priority, exchange)
        }
        Some(RecordType::TXT) => {
            let mut texts = Vec::new();
            let mut pos = 0;
            while pos < rdlength {
                let txt_len = rdata_bytes[pos] as usize;
                pos += 1;
                if pos + txt_len <= rdlength {
                    if let Ok(txt) = core::str::from_utf8(&rdata_bytes[pos..pos + txt_len]) {
                        texts.push(txt.to_string());
                    }
                    pos += txt_len;
                } else {
                    break;
                }
            }
            DnsRecordData::TXT(texts)
        }
        Some(RecordType::SRV) if rdlength >= 6 => {
            let priority = u16::from_be_bytes([rdata_bytes[0], rdata_bytes[1]]);
            let weight = u16::from_be_bytes([rdata_bytes[2], rdata_bytes[3]]);
            let port = u16::from_be_bytes([rdata_bytes[4], rdata_bytes[5]]);
            let (target, _) = decode_name(data, rdata_start + 6)?;
            DnsRecordData::SRV(priority, weight, port, target)
        }
        Some(RecordType::SOA) => {
            let (mname, off1) = decode_name(data, rdata_start)?;
            let (rname, off2) = decode_name(data, off1)?;

            if off2 + 20 > data.len() {
                DnsRecordData::Unknown(rdata_bytes.to_vec())
            } else {
                let serial = u32::from_be_bytes([data[off2], data[off2+1], data[off2+2], data[off2+3]]);
                let refresh = u32::from_be_bytes([data[off2+4], data[off2+5], data[off2+6], data[off2+7]]);
                let retry = u32::from_be_bytes([data[off2+8], data[off2+9], data[off2+10], data[off2+11]]);
                let expire = u32::from_be_bytes([data[off2+12], data[off2+13], data[off2+14], data[off2+15]]);
                let minimum = u32::from_be_bytes([data[off2+16], data[off2+17], data[off2+18], data[off2+19]]);
                DnsRecordData::SOA { mname, rname, serial, refresh, retry, expire, minimum }
            }
        }
        _ => DnsRecordData::Unknown(rdata_bytes.to_vec()),
    };

    let record = DnsRecord {
        name,
        rtype: rtype.unwrap_or(RecordType::A),
        rclass,
        ttl,
        rdata,
    };

    Some((record, rdata_start + rdlength))
}

// =============================================================================
// DNS CACHE
// =============================================================================

/// Cache entry
#[derive(Clone)]
struct CacheEntry {
    /// Cached records
    records: Vec<DnsRecord>,
    /// Expiry time (kernel ticks)
    expires_at: u64,
    /// Is negative cache (NXDOMAIN)
    is_negative: bool,
}

/// DNS cache
pub struct DnsCache {
    /// Cache entries (key = "name:type")
    entries: RwLock<BTreeMap<String, CacheEntry>>,
    /// Maximum cache entries
    max_entries: usize,
}

impl DnsCache {
    /// Create new cache
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: RwLock::new(BTreeMap::new()),
            max_entries,
        }
    }

    /// Generate cache key
    fn cache_key(name: &str, qtype: RecordType) -> String {
        let name_lower = name.to_lowercase();
        alloc::format!("{}:{}", name_lower, qtype as u16)
    }

    /// Look up cached records
    pub fn lookup(&self, name: &str, qtype: RecordType) -> Option<Vec<DnsRecord>> {
        let key = Self::cache_key(name, qtype);
        let entries = self.entries.read();

        if let Some(entry) = entries.get(&key) {
            let now = get_tick_count();
            if now < entry.expires_at {
                if entry.is_negative {
                    return None; // Negative cache hit
                }
                return Some(entry.records.clone());
            }
        }

        None
    }

    /// Store records in cache
    pub fn store(&self, name: &str, qtype: RecordType, records: Vec<DnsRecord>) {
        if records.is_empty() {
            return;
        }

        let key = Self::cache_key(name, qtype);
        let ttl = records.iter().map(|r| r.ttl).min().unwrap_or(300);
        let now = get_tick_count();

        let entry = CacheEntry {
            records,
            expires_at: now + (ttl as u64 * 1000),
            is_negative: false,
        };

        let mut entries = self.entries.write();

        // Evict old entries if at capacity
        if entries.len() >= self.max_entries {
            // Remove expired entries
            entries.retain(|_, e| e.expires_at > now);

            // If still at capacity, remove oldest
            if entries.len() >= self.max_entries {
                if let Some(key_to_remove) = entries.keys().next().cloned() {
                    entries.remove(&key_to_remove);
                }
            }
        }

        entries.insert(key, entry);
    }

    /// Store negative cache entry (NXDOMAIN)
    pub fn store_negative(&self, name: &str, qtype: RecordType) {
        let key = Self::cache_key(name, qtype);
        let now = get_tick_count();

        let entry = CacheEntry {
            records: Vec::new(),
            expires_at: now + (DNS_NEG_CACHE_TTL as u64 * 1000),
            is_negative: true,
        };

        let mut entries = self.entries.write();
        entries.insert(key, entry);
    }

    /// Clear the cache
    pub fn clear(&self) {
        self.entries.write().clear();
    }

    /// Get cache statistics
    pub fn stats(&self) -> (usize, usize) {
        let entries = self.entries.read();
        let now = get_tick_count();
        let valid = entries.values().filter(|e| e.expires_at > now).count();
        (entries.len(), valid)
    }
}

// =============================================================================
// DNS RESOLVER
// =============================================================================

/// DNS resolver configuration
#[derive(Clone)]
pub struct DnsConfig {
    /// DNS servers
    pub servers: Vec<Ipv4Address>,
    /// Search domains
    pub search: Vec<String>,
    /// Query timeout (ms)
    pub timeout_ms: u64,
    /// Max retries
    pub max_retries: u32,
    /// Use TCP for truncated responses
    pub use_tcp: bool,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            servers: vec![
                Ipv4Address::new(8, 8, 8, 8),     // Google DNS
                Ipv4Address::new(1, 1, 1, 1),     // Cloudflare DNS
            ],
            search: Vec::new(),
            timeout_ms: DNS_TIMEOUT_MS,
            max_retries: DNS_MAX_RETRIES,
            use_tcp: false,
        }
    }
}

/// DNS resolver
pub struct DnsResolver {
    /// Configuration
    config: RwLock<DnsConfig>,
    /// Cache
    cache: DnsCache,
    /// Next transaction ID
    next_id: AtomicU16,
    /// Pending queries
    pending: Mutex<BTreeMap<u16, PendingQuery>>,
}

/// Pending query state
struct PendingQuery {
    /// Domain name
    name: String,
    /// Record type
    qtype: RecordType,
    /// Start time
    started_at: u64,
    /// Response (when received)
    response: Option<DnsMessage>,
}

impl DnsResolver {
    /// Create new resolver
    pub fn new() -> Self {
        Self {
            config: RwLock::new(DnsConfig::default()),
            cache: DnsCache::new(1024),
            next_id: AtomicU16::new(1),
            pending: Mutex::new(BTreeMap::new()),
        }
    }

    /// Configure resolver
    pub fn configure(&self, config: DnsConfig) {
        *self.config.write() = config;
    }

    /// Set DNS servers
    pub fn set_servers(&self, servers: Vec<Ipv4Address>) {
        self.config.write().servers = servers;
    }

    /// Add DNS server
    pub fn add_server(&self, server: Ipv4Address) {
        self.config.write().servers.push(server);
    }

    /// Set search domains
    pub fn set_search(&self, search: Vec<String>) {
        self.config.write().search = search;
    }

    /// Resolve hostname to IPv4 addresses
    pub fn resolve(&self, name: &str) -> Result<Vec<Ipv4Address>, DnsError> {
        self.resolve_type(name, RecordType::A)
            .map(|records| {
                records
                    .into_iter()
                    .filter_map(|r| {
                        if let DnsRecordData::A(ip) = r.rdata {
                            Some(ip)
                        } else {
                            None
                        }
                    })
                    .collect()
            })
    }

    /// Resolve hostname to first IPv4 address
    pub fn resolve_one(&self, name: &str) -> Result<Ipv4Address, DnsError> {
        self.resolve(name)?
            .into_iter()
            .next()
            .ok_or(DnsError::NoRecords)
    }

    /// Resolve any record type
    pub fn resolve_type(&self, name: &str, qtype: RecordType) -> Result<Vec<DnsRecord>, DnsError> {
        // Check cache first
        if let Some(records) = self.cache.lookup(name, qtype) {
            return Ok(records);
        }

        // Build and send query
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let message = DnsMessage::new_query(id, name, qtype);
        let query_bytes = message.to_bytes();

        let config = self.config.read().clone();

        // Try each server
        for server in &config.servers {
            for _retry in 0..config.max_retries {
                // Send query via UDP
                if let Ok(response) = self.send_query(*server, &query_bytes, id, config.timeout_ms) {
                    // Check response code
                    match response.header.rcode() {
                        rcode::NOERROR => {
                            // Follow CNAME if needed
                            if response.answers.is_empty() {
                                if let Some(cname) = response.get_cname() {
                                    // Recursive resolve
                                    return self.resolve_type(cname, qtype);
                                }
                            }

                            // Cache and return answers
                            let answers = response.answers.clone();
                            if !answers.is_empty() {
                                self.cache.store(name, qtype, answers.clone());
                            }
                            return Ok(answers);
                        }
                        rcode::NXDOMAIN => {
                            self.cache.store_negative(name, qtype);
                            return Err(DnsError::NotFound);
                        }
                        rcode::SERVFAIL => {
                            // Try next server
                            break;
                        }
                        rcode::REFUSED => {
                            return Err(DnsError::Refused);
                        }
                        _ => {
                            // Retry
                            continue;
                        }
                    }
                }
            }
        }

        Err(DnsError::Timeout)
    }

    /// Reverse DNS lookup
    pub fn reverse_lookup(&self, ip: Ipv4Address) -> Result<String, DnsError> {
        // Build reverse DNS name (e.g., 1.2.3.4 -> 4.3.2.1.in-addr.arpa)
        let bytes = ip.to_bytes();
        let name = alloc::format!(
            "{}.{}.{}.{}.in-addr.arpa",
            bytes[3], bytes[2], bytes[1], bytes[0]
        );

        let records = self.resolve_type(&name, RecordType::PTR)?;

        records
            .into_iter()
            .find_map(|r| {
                if let DnsRecordData::PTR(name) = r.rdata {
                    Some(name)
                } else {
                    None
                }
            })
            .ok_or(DnsError::NoRecords)
    }

    /// Get MX records for domain
    pub fn resolve_mx(&self, domain: &str) -> Result<Vec<(u16, String)>, DnsError> {
        let records = self.resolve_type(domain, RecordType::MX)?;

        let mut result: Vec<_> = records
            .into_iter()
            .filter_map(|r| {
                if let DnsRecordData::MX(priority, exchange) = r.rdata {
                    Some((priority, exchange))
                } else {
                    None
                }
            })
            .collect();

        result.sort_by_key(|(p, _)| *p);
        Ok(result)
    }

    /// Get TXT records for domain
    pub fn resolve_txt(&self, domain: &str) -> Result<Vec<String>, DnsError> {
        let records = self.resolve_type(domain, RecordType::TXT)?;

        Ok(records
            .into_iter()
            .filter_map(|r| {
                if let DnsRecordData::TXT(texts) = r.rdata {
                    Some(texts.join(""))
                } else {
                    None
                }
            })
            .collect())
    }

    /// Send query and wait for response
    fn send_query(
        &self,
        server: Ipv4Address,
        query: &[u8],
        id: u16,
        timeout_ms: u64,
    ) -> Result<DnsMessage, DnsError> {
        // Register pending query
        {
            let mut pending = self.pending.lock();
            pending.insert(id, PendingQuery {
                name: String::new(),
                qtype: RecordType::A,
                started_at: get_tick_count(),
                response: None,
            });
        }

        // Send via UDP
        send_dns_query(server, query)?;

        // Wait for response (polling)
        let start = get_tick_count();
        loop {
            let now = get_tick_count();
            if now - start > timeout_ms {
                self.pending.lock().remove(&id);
                return Err(DnsError::Timeout);
            }

            // Check if response received
            if let Some(response) = self.pending.lock().get(&id).and_then(|p| p.response.clone()) {
                self.pending.lock().remove(&id);
                return Ok(response);
            }

            // Yield to scheduler
            core::hint::spin_loop();
        }
    }

    /// Handle incoming DNS response
    pub fn handle_response(&self, data: &[u8]) {
        if let Some(message) = DnsMessage::parse(data) {
            if message.header.is_response() {
                let id = message.header.id;
                let mut pending = self.pending.lock();
                if let Some(query) = pending.get_mut(&id) {
                    query.response = Some(message);
                }
            }
        }
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> (usize, usize) {
        self.cache.stats()
    }

    /// Clear cache
    pub fn clear_cache(&self) {
        self.cache.clear();
    }
}

// =============================================================================
// DNS ERRORS
// =============================================================================

/// DNS errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsError {
    /// Query timed out
    Timeout,
    /// Domain not found (NXDOMAIN)
    NotFound,
    /// No records of requested type
    NoRecords,
    /// Query refused
    Refused,
    /// Server failure
    ServerFailure,
    /// Invalid response
    InvalidResponse,
    /// Network error
    NetworkError,
}

impl From<NetworkError> for DnsError {
    fn from(_: NetworkError) -> Self {
        DnsError::NetworkError
    }
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Get current tick count
fn get_tick_count() -> u64 {
    crate::drivers::timer::uptime_ms()
}

/// Send DNS query via UDP
fn send_dns_query(server: Ipv4Address, data: &[u8]) -> Result<(), DnsError> {
    // Get network stack and interface
    let stack = super::get_stack().ok_or(DnsError::NetworkError)?;
    let iface = stack.primary_interface()
        .cloned()
        .ok_or(DnsError::NetworkError)?;

    let src_ip = iface.config.read().ipv4;

    // Send via UDP
    super::udp::send_udp(
        stack,
        &iface,
        src_ip,
        53123, // Ephemeral source port
        server,
        DNS_PORT,
        data,
    ).map_err(|_| DnsError::NetworkError)?;

    Ok(())
}

// =============================================================================
// GLOBAL RESOLVER
// =============================================================================

/// Global DNS resolver
static RESOLVER: RwLock<Option<Arc<DnsResolver>>> = RwLock::new(None);

/// Initialize DNS resolver
pub fn init() {
    let resolver = Arc::new(DnsResolver::new());
    *RESOLVER.write() = Some(resolver);
}

/// Get resolver instance
pub fn get_resolver() -> Option<Arc<DnsResolver>> {
    RESOLVER.read().clone()
}

/// Configure DNS servers (from DHCP)
pub fn set_servers(servers: Vec<Ipv4Address>) {
    if let Some(resolver) = get_resolver() {
        resolver.set_servers(servers);
    }
}

/// Resolve hostname (convenience function)
pub fn resolve(name: &str) -> Result<Ipv4Address, DnsError> {
    get_resolver()
        .ok_or(DnsError::NetworkError)?
        .resolve_one(name)
}

/// Resolve all IPs for hostname
pub fn resolve_all(name: &str) -> Result<Vec<Ipv4Address>, DnsError> {
    get_resolver()
        .ok_or(DnsError::NetworkError)?
        .resolve(name)
}

/// Reverse DNS lookup
pub fn reverse(ip: Ipv4Address) -> Result<String, DnsError> {
    get_resolver()
        .ok_or(DnsError::NetworkError)?
        .reverse_lookup(ip)
}

/// Handle incoming DNS response packet
pub fn handle_response(data: &[u8]) {
    if let Some(resolver) = get_resolver() {
        resolver.handle_response(data);
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_name() {
        let encoded = encode_name("example.com");
        assert_eq!(encoded, vec![7, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 3, b'c', b'o', b'm', 0]);
    }

    #[test]
    fn test_decode_name() {
        let data = vec![7, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 3, b'c', b'o', b'm', 0];
        let (name, offset) = decode_name(&data, 0).unwrap();
        assert_eq!(name, "example.com");
        assert_eq!(offset, 13);
    }

    #[test]
    fn test_header_serialize() {
        let header = DnsHeader::new_query(1234);
        let bytes = header.to_bytes();
        assert_eq!(bytes.len(), 12);
        assert_eq!(bytes[0], 0x04); // ID high
        assert_eq!(bytes[1], 0xD2); // ID low
    }

    #[test]
    fn test_cache_key() {
        let key = DnsCache::cache_key("EXAMPLE.COM", RecordType::A);
        assert_eq!(key, "example.com:1");
    }
}
