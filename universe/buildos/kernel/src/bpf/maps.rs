// ===============================================================================
// BUILDOS KERNEL - BPF MAPS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! BPF Maps - Key-Value Data Structures
//!
//! Provides various map types for BPF programs to store and share data.

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::sync::RwLock;
use super::BpfError;

// =============================================================================
// MAP TYPES
// =============================================================================

/// BPF map types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum BpfMapType {
    /// Unspecified
    Unspec = 0,
    /// Hash table
    Hash = 1,
    /// Array
    Array = 2,
    /// Program array (tail calls)
    ProgArray = 3,
    /// Perf event array
    PerfEventArray = 4,
    /// Per-CPU hash
    PercpuHash = 5,
    /// Per-CPU array
    PercpuArray = 6,
    /// Stack trace
    StackTrace = 7,
    /// cgroup array
    CgroupArray = 8,
    /// LRU hash
    LruHash = 9,
    /// LRU per-CPU hash
    LruPercpuHash = 10,
    /// LPM trie
    LpmTrie = 11,
    /// Array of maps
    ArrayOfMaps = 12,
    /// Hash of maps
    HashOfMaps = 13,
    /// Device map
    Devmap = 14,
    /// Socket map
    Sockmap = 15,
    /// CPU map
    Cpumap = 16,
    /// Xsk map
    Xskmap = 17,
    /// Socket hash
    Sockhash = 18,
    /// cgroup storage
    CgroupStorage = 19,
    /// Reuseport socket array
    ReuseportSockarray = 20,
    /// Per-CPU cgroup storage
    PercpuCgroupStorage = 21,
    /// Queue
    Queue = 22,
    /// Stack
    Stack = 23,
    /// Socket local storage
    SkStorage = 24,
    /// Device map hash
    DevmapHash = 25,
    /// Struct ops
    StructOps = 26,
    /// Ring buffer
    Ringbuf = 27,
    /// Inode storage
    InodeStorage = 28,
    /// Task storage
    TaskStorage = 29,
    /// Bloom filter
    BloomFilter = 30,
    /// User ring buffer
    UserRingbuf = 31,
    /// cgroup storage (v2)
    CgroupStorageV2 = 32,
}

impl BpfMapType {
    /// Convert from u32
    pub fn from_u32(val: u32) -> Option<Self> {
        if val <= 32 {
            Some(unsafe { core::mem::transmute(val) })
        } else {
            None
        }
    }
}

// =============================================================================
// MAP DEFINITION
// =============================================================================

/// Map definition (for ELF sections)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BpfMapDef {
    /// Map type
    pub type_: u32,
    /// Key size in bytes
    pub key_size: u32,
    /// Value size in bytes
    pub value_size: u32,
    /// Maximum entries
    pub max_entries: u32,
    /// Map flags
    pub map_flags: u32,
}

/// Map flags
pub mod map_flags {
    /// No pre-allocation
    pub const BPF_F_NO_PREALLOC: u32 = 1 << 0;
    /// No common LRU
    pub const BPF_F_NO_COMMON_LRU: u32 = 1 << 1;
    /// NUMA node aware
    pub const BPF_F_NUMA_NODE: u32 = 1 << 2;
    /// Read-only (userspace)
    pub const BPF_F_RDONLY: u32 = 1 << 3;
    /// Write-only (userspace)
    pub const BPF_F_WRONLY: u32 = 1 << 4;
    /// Stack build ID
    pub const BPF_F_STACK_BUILD_ID: u32 = 1 << 5;
    /// Zero seed
    pub const BPF_F_ZERO_SEED: u32 = 1 << 6;
    /// Read-only (program)
    pub const BPF_F_RDONLY_PROG: u32 = 1 << 7;
    /// Write-only (program)
    pub const BPF_F_WRONLY_PROG: u32 = 1 << 8;
    /// Clone
    pub const BPF_F_CLONE: u32 = 1 << 9;
    /// Memory-mapped
    pub const BPF_F_MMAPABLE: u32 = 1 << 10;
    /// Preserve elem on fail
    pub const BPF_F_PRESERVE_ELEMS: u32 = 1 << 11;
    /// Inner map
    pub const BPF_F_INNER_MAP: u32 = 1 << 12;
}

// =============================================================================
// BPF MAP
// =============================================================================

/// BPF map
pub struct BpfMap {
    /// Map ID
    id: u32,
    /// Map type
    map_type: BpfMapType,
    /// Key size
    key_size: u32,
    /// Value size
    value_size: u32,
    /// Maximum entries
    max_entries: u32,
    /// Map flags
    flags: u32,
    /// Name
    name: [u8; 16],
    /// Data storage (type-dependent)
    data: RwLock<MapData>,
    /// Reference count
    refs: AtomicU64,
}

/// Internal map data
enum MapData {
    /// Hash map
    Hash(BTreeMap<Vec<u8>, Vec<u8>>),
    /// Array
    Array(Vec<Vec<u8>>),
    /// Per-CPU array
    PercpuArray(Vec<Vec<Vec<u8>>>),
    /// Ring buffer
    RingBuf(RingBuffer),
    /// Queue
    Queue(Queue),
    /// Stack
    Stack(Stack),
}

impl BpfMap {
    /// Create new map
    pub fn new(
        id: u32,
        map_type: BpfMapType,
        key_size: u32,
        value_size: u32,
        max_entries: u32,
    ) -> Self {
        let data = match map_type {
            BpfMapType::Hash | BpfMapType::LruHash |
            BpfMapType::PercpuHash | BpfMapType::LruPercpuHash => {
                MapData::Hash(BTreeMap::new())
            }
            BpfMapType::Array | BpfMapType::ProgArray |
            BpfMapType::PerfEventArray | BpfMapType::CgroupArray => {
                let entries = (0..max_entries)
                    .map(|_| vec![0u8; value_size as usize])
                    .collect();
                MapData::Array(entries)
            }
            BpfMapType::PercpuArray => {
                // Assume 8 CPUs for now
                let cpu_count = 8;
                let entries = (0..max_entries)
                    .map(|_| {
                        (0..cpu_count)
                            .map(|_| vec![0u8; value_size as usize])
                            .collect()
                    })
                    .collect();
                MapData::PercpuArray(entries)
            }
            BpfMapType::Ringbuf => {
                MapData::RingBuf(RingBuffer::new(max_entries as usize))
            }
            BpfMapType::Queue => {
                MapData::Queue(Queue::new(max_entries as usize))
            }
            BpfMapType::Stack => {
                MapData::Stack(Stack::new(max_entries as usize))
            }
            _ => MapData::Hash(BTreeMap::new()),
        };

        Self {
            id,
            map_type,
            key_size,
            value_size,
            max_entries,
            flags: 0,
            name: [0; 16],
            data: RwLock::new(data),
            refs: AtomicU64::new(1),
        }
    }

    /// Get map ID
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Get map type
    pub fn map_type(&self) -> BpfMapType {
        self.map_type
    }

    /// Lookup element
    pub fn lookup(&self, key: &[u8]) -> Result<Option<Vec<u8>>, BpfError> {
        if key.len() != self.key_size as usize {
            return Err(BpfError::InvalidProgram);
        }

        let data = self.data.read();
        match &*data {
            MapData::Hash(map) => {
                Ok(map.get(key).cloned())
            }
            MapData::Array(arr) => {
                if key.len() < 4 {
                    return Err(BpfError::InvalidProgram);
                }
                let index = u32::from_ne_bytes([key[0], key[1], key[2], key[3]]) as usize;
                if index >= arr.len() {
                    return Err(BpfError::OutOfBounds);
                }
                Ok(Some(arr[index].clone()))
            }
            _ => Err(BpfError::InvalidMapType),
        }
    }

    /// Update element
    pub fn update(&self, key: &[u8], value: &[u8], flags: u64) -> Result<(), BpfError> {
        if key.len() != self.key_size as usize || value.len() != self.value_size as usize {
            return Err(BpfError::InvalidProgram);
        }

        let mut data = self.data.write();
        match &mut *data {
            MapData::Hash(map) => {
                // BPF_NOEXIST = 1, BPF_EXIST = 2
                if flags == 1 && map.contains_key(key) {
                    return Err(BpfError::MapFull);
                }
                if flags == 2 && !map.contains_key(key) {
                    return Err(BpfError::KeyNotFound);
                }
                if map.len() >= self.max_entries as usize && !map.contains_key(key) {
                    return Err(BpfError::MapFull);
                }
                map.insert(key.to_vec(), value.to_vec());
                Ok(())
            }
            MapData::Array(arr) => {
                if key.len() < 4 {
                    return Err(BpfError::InvalidProgram);
                }
                let index = u32::from_ne_bytes([key[0], key[1], key[2], key[3]]) as usize;
                if index >= arr.len() {
                    return Err(BpfError::OutOfBounds);
                }
                arr[index] = value.to_vec();
                Ok(())
            }
            _ => Err(BpfError::InvalidMapType),
        }
    }

    /// Delete element
    pub fn delete(&self, key: &[u8]) -> Result<(), BpfError> {
        if key.len() != self.key_size as usize {
            return Err(BpfError::InvalidProgram);
        }

        let mut data = self.data.write();
        match &mut *data {
            MapData::Hash(map) => {
                map.remove(key).ok_or(BpfError::KeyNotFound)?;
                Ok(())
            }
            MapData::Array(_) => {
                // Can't delete from array
                Err(BpfError::InvalidMapType)
            }
            _ => Err(BpfError::InvalidMapType),
        }
    }

    /// Get next key
    pub fn get_next_key(&self, key: Option<&[u8]>) -> Result<Option<Vec<u8>>, BpfError> {
        let data = self.data.read();
        match &*data {
            MapData::Hash(map) => {
                match key {
                    None => Ok(map.keys().next().cloned()),
                    Some(k) => {
                        let mut iter = map.keys();
                        while let Some(key) = iter.next() {
                            if key.as_slice() == k {
                                return Ok(iter.next().cloned());
                            }
                        }
                        Err(BpfError::KeyNotFound)
                    }
                }
            }
            MapData::Array(arr) => {
                match key {
                    None => {
                        if arr.is_empty() {
                            Ok(None)
                        } else {
                            Ok(Some(0u32.to_ne_bytes().to_vec()))
                        }
                    }
                    Some(k) => {
                        if k.len() < 4 {
                            return Err(BpfError::InvalidProgram);
                        }
                        let index = u32::from_ne_bytes([k[0], k[1], k[2], k[3]]) as usize;
                        if index + 1 >= arr.len() {
                            Ok(None)
                        } else {
                            Ok(Some(((index + 1) as u32).to_ne_bytes().to_vec()))
                        }
                    }
                }
            }
            _ => Err(BpfError::InvalidMapType),
        }
    }

    /// Push to queue
    pub fn push(&self, value: &[u8]) -> Result<(), BpfError> {
        if value.len() != self.value_size as usize {
            return Err(BpfError::InvalidProgram);
        }

        let mut data = self.data.write();
        match &mut *data {
            MapData::Queue(queue) => queue.push(value),
            MapData::Stack(stack) => stack.push(value),
            _ => Err(BpfError::InvalidMapType),
        }
    }

    /// Pop from queue/stack
    pub fn pop(&self) -> Result<Option<Vec<u8>>, BpfError> {
        let mut data = self.data.write();
        match &mut *data {
            MapData::Queue(queue) => Ok(queue.pop()),
            MapData::Stack(stack) => Ok(stack.pop()),
            _ => Err(BpfError::InvalidMapType),
        }
    }

    /// Peek at queue/stack
    pub fn peek(&self) -> Result<Option<Vec<u8>>, BpfError> {
        let data = self.data.read();
        match &*data {
            MapData::Queue(queue) => Ok(queue.peek()),
            MapData::Stack(stack) => Ok(stack.peek()),
            _ => Err(BpfError::InvalidMapType),
        }
    }

    /// Reserve space in ring buffer
    pub fn ringbuf_reserve(&self, size: usize) -> Result<u64, BpfError> {
        let mut data = self.data.write();
        match &mut *data {
            MapData::RingBuf(rb) => rb.reserve(size),
            _ => Err(BpfError::InvalidMapType),
        }
    }

    /// Submit ring buffer reservation
    pub fn ringbuf_submit(&self, ptr: u64) -> Result<(), BpfError> {
        let mut data = self.data.write();
        match &mut *data {
            MapData::RingBuf(rb) => rb.submit(ptr),
            _ => Err(BpfError::InvalidMapType),
        }
    }

    /// Discard ring buffer reservation
    pub fn ringbuf_discard(&self, ptr: u64) -> Result<(), BpfError> {
        let mut data = self.data.write();
        match &mut *data {
            MapData::RingBuf(rb) => rb.discard(ptr),
            _ => Err(BpfError::InvalidMapType),
        }
    }

    /// Add reference
    pub fn add_ref(&self) {
        self.refs.fetch_add(1, Ordering::AcqRel);
    }

    /// Release reference
    pub fn release(&self) -> bool {
        self.refs.fetch_sub(1, Ordering::AcqRel) == 1
    }
}

// =============================================================================
// RING BUFFER
// =============================================================================

/// Ring buffer for BPF
struct RingBuffer {
    /// Data storage
    data: Vec<u8>,
    /// Head (producer)
    head: usize,
    /// Tail (consumer)
    tail: usize,
    /// Capacity
    capacity: usize,
}

impl RingBuffer {
    /// Create new ring buffer
    fn new(capacity: usize) -> Self {
        Self {
            data: vec![0; capacity],
            head: 0,
            tail: 0,
            capacity,
        }
    }

    /// Reserve space
    fn reserve(&mut self, size: usize) -> Result<u64, BpfError> {
        let available = if self.head >= self.tail {
            self.capacity - (self.head - self.tail)
        } else {
            self.tail - self.head
        };

        if size + 8 > available {
            return Err(BpfError::MapFull);
        }

        let ptr = self.head;
        self.head = (self.head + size + 8) % self.capacity;
        Ok(ptr as u64)
    }

    /// Submit reservation
    fn submit(&mut self, _ptr: u64) -> Result<(), BpfError> {
        // Mark data as available
        Ok(())
    }

    /// Discard reservation
    fn discard(&mut self, _ptr: u64) -> Result<(), BpfError> {
        // Revert head
        Ok(())
    }
}

// =============================================================================
// QUEUE
// =============================================================================

/// FIFO queue
struct Queue {
    data: Vec<Vec<u8>>,
    max_entries: usize,
}

impl Queue {
    fn new(max_entries: usize) -> Self {
        Self {
            data: Vec::with_capacity(max_entries),
            max_entries,
        }
    }

    fn push(&mut self, value: &[u8]) -> Result<(), BpfError> {
        if self.data.len() >= self.max_entries {
            return Err(BpfError::MapFull);
        }
        self.data.push(value.to_vec());
        Ok(())
    }

    fn pop(&mut self) -> Option<Vec<u8>> {
        if self.data.is_empty() {
            None
        } else {
            Some(self.data.remove(0))
        }
    }

    fn peek(&self) -> Option<Vec<u8>> {
        self.data.first().cloned()
    }
}

// =============================================================================
// STACK
// =============================================================================

/// LIFO stack
struct Stack {
    data: Vec<Vec<u8>>,
    max_entries: usize,
}

impl Stack {
    fn new(max_entries: usize) -> Self {
        Self {
            data: Vec::with_capacity(max_entries),
            max_entries,
        }
    }

    fn push(&mut self, value: &[u8]) -> Result<(), BpfError> {
        if self.data.len() >= self.max_entries {
            return Err(BpfError::MapFull);
        }
        self.data.push(value.to_vec());
        Ok(())
    }

    fn pop(&mut self) -> Option<Vec<u8>> {
        self.data.pop()
    }

    fn peek(&self) -> Option<Vec<u8>> {
        self.data.last().cloned()
    }
}
