// ===============================================================================
// BUILDOS KERNEL - NUMA SUPPORT
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! Non-Uniform Memory Access (NUMA) support for multi-socket systems.
//!
//! This module provides:
//! - NUMA node discovery from ACPI SRAT table
//! - Per-node memory allocation
//! - NUMA-aware scheduler hints
//! - Memory interleaving policies

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::RwLock;


// =============================================================================
// CONSTANTS
// =============================================================================

/// Maximum number of NUMA nodes
pub const MAX_NUMA_NODES: usize = 64;

/// Maximum number of CPUs
pub const MAX_CPUS: usize = 256;

/// Invalid NUMA node ID
pub const NUMA_NO_NODE: u32 = u32::MAX;

/// Default memory allocation policy
pub const DEFAULT_POLICY: NumaPolicy = NumaPolicy::Local;

// =============================================================================
// NUMA TOPOLOGY
// =============================================================================

/// Global NUMA topology
static NUMA_TOPOLOGY: RwLock<NumaTopology> = RwLock::new(NumaTopology::new());

/// NUMA system topology
pub struct NumaTopology {
    /// NUMA nodes
    nodes: [Option<NumaNode>; MAX_NUMA_NODES],

    /// Number of active nodes
    node_count: usize,

    /// CPU to node mapping
    cpu_to_node: [u32; MAX_CPUS],

    /// Distance matrix (node to node latency)
    distances: [[u8; MAX_NUMA_NODES]; MAX_NUMA_NODES],

    /// Is NUMA available?
    available: bool,
}

/// A single NUMA node
pub struct NumaNode {
    /// Node ID
    pub id: u32,

    /// CPUs on this node
    pub cpus: Vec<u32>,

    /// Memory ranges on this node
    pub memory_ranges: Vec<MemoryRange>,

    /// Total memory in bytes
    pub total_memory: u64,

    /// Free memory in bytes
    pub free_memory: AtomicU64,

    /// Node state
    pub state: NodeState,
}

/// Memory range on a NUMA node
#[derive(Clone)]
pub struct MemoryRange {
    /// Start physical address
    pub start: u64,

    /// End physical address
    pub end: u64,

    /// Flags
    pub flags: MemoryRangeFlags,
}

bitflags::bitflags! {
    /// Memory range flags
    #[derive(Clone, Copy, Debug, Default)]
    pub struct MemoryRangeFlags: u32 {
        /// Hot-pluggable memory
        const HOTPLUG = 1 << 0;
        /// Non-volatile memory
        const NVDIMM = 1 << 1;
        /// Memory is online
        const ONLINE = 1 << 2;
    }
}

/// NUMA node state
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NodeState {
    /// Node is online and usable
    Online,
    /// Node is offline
    Offline,
    /// Node is being hot-added
    HotAdd,
    /// Node is being hot-removed
    HotRemove,
}

impl NumaTopology {
    /// Create a new NUMA topology
    pub const fn new() -> Self {
        const NONE: Option<NumaNode> = None;

        Self {
            nodes: [NONE; MAX_NUMA_NODES],
            node_count: 0,
            cpu_to_node: [NUMA_NO_NODE; MAX_CPUS],
            distances: [[255u8; MAX_NUMA_NODES]; MAX_NUMA_NODES],
            available: false,
        }
    }

    /// Initialize NUMA topology from ACPI SRAT
    pub fn init_from_acpi(&mut self, srat_addr: u64) {
        if srat_addr == 0 {
            // No SRAT table, single node system
            self.init_single_node();
            return;
        }

        // Parse SRAT table
        unsafe {
            self.parse_srat(srat_addr);
        }

        // Parse SLIT table for distances if available
        // self.parse_slit(slit_addr);

        self.available = self.node_count > 1;
    }

    /// Initialize as single-node system (non-NUMA)
    fn init_single_node(&mut self) {
        let node = NumaNode {
            id: 0,
            cpus: Vec::new(),
            memory_ranges: Vec::new(),
            total_memory: 0,
            free_memory: AtomicU64::new(0),
            state: NodeState::Online,
        };

        self.nodes[0] = Some(node);
        self.node_count = 1;

        // All CPUs on node 0
        for cpu in 0..MAX_CPUS {
            self.cpu_to_node[cpu] = 0;
        }

        // Distance to self is 10
        self.distances[0][0] = 10;
    }

    /// Parse ACPI SRAT table
    unsafe fn parse_srat(&mut self, _srat_addr: u64) {
        // Would parse actual SRAT table structure:
        // - Processor Local APIC/SAPIC Affinity Structure
        // - Memory Affinity Structure
        // - Processor Local x2APIC Affinity Structure

        // For now, initialize as single node
        self.init_single_node();
    }

    /// Get node for a CPU
    pub fn cpu_node(&self, cpu: u32) -> u32 {
        if (cpu as usize) < MAX_CPUS {
            self.cpu_to_node[cpu as usize]
        } else {
            0
        }
    }

    /// Get distance between two nodes
    pub fn distance(&self, from: u32, to: u32) -> u8 {
        if (from as usize) < MAX_NUMA_NODES && (to as usize) < MAX_NUMA_NODES {
            self.distances[from as usize][to as usize]
        } else {
            255
        }
    }

    /// Find closest node with free memory
    pub fn closest_node_with_memory(&self, from: u32) -> Option<u32> {
        let mut best_node = None;
        let mut best_distance = u8::MAX;

        for (i, node) in self.nodes.iter().enumerate() {
            if let Some(n) = node {
                if n.state == NodeState::Online && n.free_memory.load(Ordering::Relaxed) > 0 {
                    let dist = self.distance(from, i as u32);
                    if dist < best_distance {
                        best_distance = dist;
                        best_node = Some(i as u32);
                    }
                }
            }
        }

        best_node
    }

    /// Get total system memory across all nodes
    pub fn total_memory(&self) -> u64 {
        self.nodes.iter()
            .flatten()
            .map(|n| n.total_memory)
            .sum()
    }

    /// Get free memory across all nodes
    pub fn free_memory(&self) -> u64 {
        self.nodes.iter()
            .flatten()
            .map(|n| n.free_memory.load(Ordering::Relaxed))
            .sum()
    }

    /// Get a node by ID
    pub fn get_node(&self, id: u32) -> Option<&NumaNode> {
        if (id as usize) < MAX_NUMA_NODES {
            self.nodes[id as usize].as_ref()
        } else {
            None
        }
    }

    /// Get a mutable node by ID
    pub fn get_node_mut(&mut self, id: u32) -> Option<&mut NumaNode> {
        if (id as usize) < MAX_NUMA_NODES {
            self.nodes[id as usize].as_mut()
        } else {
            None
        }
    }

    /// Is NUMA available?
    pub fn is_available(&self) -> bool {
        self.available
    }

    /// Get number of nodes
    pub fn node_count(&self) -> usize {
        self.node_count
    }
}

// =============================================================================
// MEMORY ALLOCATION POLICIES
// =============================================================================

/// NUMA memory allocation policy
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NumaPolicy {
    /// Allocate from local node (default)
    Local,

    /// Prefer specific node, fall back to others
    Preferred { node: u32 },

    /// Interleave across specified nodes
    Interleave { nodes: u64 },  // Bitmap of nodes

    /// Bind to specific nodes only
    Bind { nodes: u64 },  // Bitmap of nodes

    /// Allocate from any node with memory
    Default,
}

/// Per-task NUMA policy
pub struct TaskNumaPolicy {
    /// Memory allocation policy
    pub policy: NumaPolicy,

    /// Preferred node for this task
    pub preferred_node: u32,

    /// Current interleave index
    pub interleave_idx: u32,

    /// Memory migration enabled
    pub migrate_enabled: bool,
}

impl Default for TaskNumaPolicy {
    fn default() -> Self {
        Self {
            policy: DEFAULT_POLICY,
            preferred_node: 0,
            interleave_idx: 0,
            migrate_enabled: true,
        }
    }
}

// =============================================================================
// NUMA-AWARE ALLOCATION
// =============================================================================

/// NUMA-aware page allocator
pub struct NumaAllocator {
    /// Per-node free lists
    node_freelists: [NodeFreeList; MAX_NUMA_NODES],
}

/// Free list for a single node
pub struct NodeFreeList {
    /// Free pages bitmap (simplified)
    free_pages: AtomicU64,

    /// Number of free pages
    free_count: AtomicU64,
}

impl NumaAllocator {
    /// Create a new NUMA allocator
    pub const fn new() -> Self {
        const EMPTY: NodeFreeList = NodeFreeList {
            free_pages: AtomicU64::new(0),
            free_count: AtomicU64::new(0),
        };

        Self {
            node_freelists: [EMPTY; MAX_NUMA_NODES],
        }
    }

    /// Allocate pages from a specific node
    pub fn alloc_pages_node(&self, node: u32, count: usize) -> Option<u64> {
        if (node as usize) >= MAX_NUMA_NODES {
            return None;
        }

        let freelist = &self.node_freelists[node as usize];
        let current = freelist.free_count.load(Ordering::Relaxed);

        if current >= count as u64 {
            freelist.free_count.fetch_sub(count as u64, Ordering::Relaxed);
            // Would return actual page address
            Some(0)
        } else {
            None
        }
    }

    /// Allocate pages with policy
    pub fn alloc_pages_policy(&self, count: usize, policy: &TaskNumaPolicy) -> Option<u64> {
        match policy.policy {
            NumaPolicy::Local => {
                // Allocate from local node
                self.alloc_pages_node(policy.preferred_node, count)
            }
            NumaPolicy::Preferred { node } => {
                // Try preferred, then fall back
                self.alloc_pages_node(node, count)
                    .or_else(|| self.alloc_pages_any(count))
            }
            NumaPolicy::Interleave { nodes } => {
                // Round-robin across nodes
                let node_list: Vec<u32> = (0..64)
                    .filter(|i| (nodes & (1 << i)) != 0)
                    .map(|i| i as u32)
                    .collect();

                if node_list.is_empty() {
                    return self.alloc_pages_any(count);
                }

                let idx = policy.interleave_idx as usize % node_list.len();
                self.alloc_pages_node(node_list[idx], count)
            }
            NumaPolicy::Bind { nodes } => {
                // Only allocate from specified nodes
                for i in 0..64 {
                    if (nodes & (1 << i)) != 0 {
                        if let Some(addr) = self.alloc_pages_node(i as u32, count) {
                            return Some(addr);
                        }
                    }
                }
                None
            }
            NumaPolicy::Default => {
                self.alloc_pages_any(count)
            }
        }
    }

    /// Allocate pages from any node
    pub fn alloc_pages_any(&self, count: usize) -> Option<u64> {
        for node in 0..MAX_NUMA_NODES {
            if let Some(addr) = self.alloc_pages_node(node as u32, count) {
                return Some(addr);
            }
        }
        None
    }

    /// Free pages to a node
    pub fn free_pages_node(&self, node: u32, _addr: u64, count: usize) {
        if (node as usize) < MAX_NUMA_NODES {
            let freelist = &self.node_freelists[node as usize];
            freelist.free_count.fetch_add(count as u64, Ordering::Relaxed);
        }
    }
}

// =============================================================================
// MEMORY MIGRATION
// =============================================================================

/// Migrate pages between NUMA nodes
pub struct PageMigrator;

impl PageMigrator {
    /// Migrate a range of pages to a different node
    pub fn migrate_pages(
        _vaddr_start: u64,
        _page_count: usize,
        _dest_node: u32,
    ) -> Result<usize, MigrationError> {
        // Would:
        // 1. Allocate pages on destination node
        // 2. Copy data
        // 3. Update page tables
        // 4. Free source pages

        Ok(0)
    }

    /// Migrate task's pages to follow CPU affinity
    pub fn migrate_task_pages(_task_id: u64, _target_node: u32) -> Result<usize, MigrationError> {
        // Migrate all of task's pages to target node
        Ok(0)
    }
}

/// Page migration error
#[derive(Debug)]
pub enum MigrationError {
    /// No memory on target node
    NoMemory,
    /// Page is pinned
    PagePinned,
    /// Migration in progress
    InProgress,
    /// Invalid node
    InvalidNode,
}

// =============================================================================
// PUBLIC INTERFACE
// =============================================================================

/// Initialize NUMA subsystem
pub fn init() {
    // Would read SRAT from ACPI
    let mut topo = NUMA_TOPOLOGY.write();
    topo.init_from_acpi(0);
}

/// Get NUMA topology
pub fn topology() -> &'static RwLock<NumaTopology> {
    &NUMA_TOPOLOGY
}

/// Check if NUMA is available
pub fn is_available() -> bool {
    NUMA_TOPOLOGY.read().is_available()
}

/// Get node count
pub fn node_count() -> usize {
    NUMA_TOPOLOGY.read().node_count()
}

/// Get node for current CPU
pub fn current_node() -> u32 {
    let cpu = crate::cpu::current_cpu_id();
    NUMA_TOPOLOGY.read().cpu_node(cpu)
}

/// Get distance between nodes
pub fn distance(from: u32, to: u32) -> u8 {
    NUMA_TOPOLOGY.read().distance(from, to)
}

/// Get total system memory
pub fn total_memory() -> u64 {
    NUMA_TOPOLOGY.read().total_memory()
}

/// Get free system memory
pub fn free_memory() -> u64 {
    NUMA_TOPOLOGY.read().free_memory()
}

// =============================================================================
// SYSCALL INTERFACE
// =============================================================================

/// Get NUMA node for a memory address
pub fn get_mempolicy(_addr: u64) -> (NumaPolicy, u32) {
    // Return current policy and preferred node
    (NumaPolicy::Default, 0)
}

/// Set NUMA memory policy
pub fn set_mempolicy(_policy: NumaPolicy) -> Result<(), ()> {
    // Set policy for current task
    Ok(())
}

/// Migrate pages to nodes
pub fn migrate_pages(
    _pages: &[u64],
    _dest_nodes: &[u32],
) -> Result<usize, MigrationError> {
    // Migrate specified pages to specified nodes
    Ok(0)
}

/// Move pages between nodes
pub fn move_pages(
    _pages: &[u64],
    _nodes: &[u32],
) -> Result<Vec<i32>, MigrationError> {
    // Move pages and return status per page
    Ok(Vec::new())
}
