//! CPU Affinity Management
//!
//! Manages CPU affinity masks and NUMA topology awareness.

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use crate::process::Tid;

/// Maximum supported CPUs
pub const MAX_CPUS: usize = 256;

/// CPU set (affinity mask)
#[derive(Clone, Debug)]
pub struct CpuSet {
    /// Bitmap of allowed CPUs (256 bits = 4 u64s)
    mask: [u64; 4],
}

impl CpuSet {
    /// Create an empty CPU set
    pub const fn empty() -> Self {
        Self { mask: [0; 4] }
    }

    /// Create a CPU set with all CPUs enabled
    pub fn all(nr_cpus: u32) -> Self {
        let mut set = Self::empty();
        for cpu in 0..nr_cpus {
            set.set(cpu);
        }
        set
    }

    /// Create a CPU set with a single CPU
    pub fn single(cpu: u32) -> Self {
        let mut set = Self::empty();
        set.set(cpu);
        set
    }

    /// Create from a slice of CPU IDs
    pub fn from_cpus(cpus: &[u32]) -> Self {
        let mut set = Self::empty();
        for &cpu in cpus {
            set.set(cpu);
        }
        set
    }

    /// Set a CPU in the mask
    pub fn set(&mut self, cpu: u32) {
        if (cpu as usize) < MAX_CPUS {
            let idx = cpu as usize / 64;
            let bit = cpu as usize % 64;
            self.mask[idx] |= 1 << bit;
        }
    }

    /// Clear a CPU from the mask
    pub fn clear(&mut self, cpu: u32) {
        if (cpu as usize) < MAX_CPUS {
            let idx = cpu as usize / 64;
            let bit = cpu as usize % 64;
            self.mask[idx] &= !(1 << bit);
        }
    }

    /// Test if a CPU is in the set
    pub fn contains(&self, cpu: u32) -> bool {
        if (cpu as usize) >= MAX_CPUS {
            return false;
        }
        let idx = cpu as usize / 64;
        let bit = cpu as usize % 64;
        (self.mask[idx] & (1 << bit)) != 0
    }

    /// Count the number of CPUs in the set
    pub fn count(&self) -> u32 {
        self.mask.iter().map(|w| w.count_ones()).sum()
    }

    /// Check if the set is empty
    pub fn is_empty(&self) -> bool {
        self.mask.iter().all(|&w| w == 0)
    }

    /// Intersect with another set
    pub fn intersect(&self, other: &Self) -> Self {
        let mut result = Self::empty();
        for i in 0..4 {
            result.mask[i] = self.mask[i] & other.mask[i];
        }
        result
    }

    /// Union with another set
    pub fn union(&self, other: &Self) -> Self {
        let mut result = Self::empty();
        for i in 0..4 {
            result.mask[i] = self.mask[i] | other.mask[i];
        }
        result
    }

    /// Get the first CPU in the set
    pub fn first(&self) -> Option<u32> {
        for (idx, &word) in self.mask.iter().enumerate() {
            if word != 0 {
                return Some((idx * 64 + word.trailing_zeros() as usize) as u32);
            }
        }
        None
    }

    /// Get the last CPU in the set
    pub fn last(&self) -> Option<u32> {
        for (idx, &word) in self.mask.iter().enumerate().rev() {
            if word != 0 {
                return Some((idx * 64 + 63 - word.leading_zeros() as usize) as u32);
            }
        }
        None
    }

    /// Iterate over CPUs in the set
    pub fn iter(&self) -> CpuSetIter<'_> {
        CpuSetIter {
            set: self,
            current: 0,
        }
    }

    /// Get the raw mask as bytes
    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(
                self.mask.as_ptr() as *const u8,
                32,
            )
        }
    }
}

impl Default for CpuSet {
    fn default() -> Self {
        Self::empty()
    }
}

/// Iterator over CPUs in a CPU set
pub struct CpuSetIter<'a> {
    set: &'a CpuSet,
    current: u32,
}

impl<'a> Iterator for CpuSetIter<'a> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        while (self.current as usize) < MAX_CPUS {
            let cpu = self.current;
            self.current += 1;
            if self.set.contains(cpu) {
                return Some(cpu);
            }
        }
        None
    }
}

/// NUMA node information
#[derive(Clone, Debug)]
pub struct NumaNode {
    /// Node ID
    pub id: u32,
    /// CPUs in this node
    pub cpus: CpuSet,
    /// Memory in bytes
    pub memory: u64,
    /// Free memory in bytes
    pub free_memory: u64,
    /// Distance to other nodes
    pub distances: Vec<u32>,
}

impl NumaNode {
    /// Create a new NUMA node
    pub fn new(id: u32) -> Self {
        Self {
            id,
            cpus: CpuSet::empty(),
            memory: 0,
            free_memory: 0,
            distances: Vec::new(),
        }
    }

    /// Add a CPU to this node
    pub fn add_cpu(&mut self, cpu: u32) {
        self.cpus.set(cpu);
    }

    /// Set memory size
    pub fn set_memory(&mut self, total: u64, free: u64) {
        self.memory = total;
        self.free_memory = free;
    }

    /// Get distance to another node
    pub fn distance_to(&self, node: u32) -> u32 {
        self.distances.get(node as usize).copied().unwrap_or(u32::MAX)
    }
}

/// NUMA topology
pub struct NumaTopology {
    /// NUMA nodes
    nodes: Vec<NumaNode>,
    /// CPU to node mapping
    cpu_to_node: [u32; MAX_CPUS],
    /// Number of nodes
    nr_nodes: u32,
    /// Number of CPUs
    nr_cpus: u32,
}

impl NumaTopology {
    /// Create a flat (non-NUMA) topology
    pub fn flat(nr_cpus: u32) -> Self {
        let mut node = NumaNode::new(0);
        for cpu in 0..nr_cpus {
            node.add_cpu(cpu);
        }
        node.distances = vec![10]; // Local distance

        Self {
            nodes: vec![node],
            cpu_to_node: [0; MAX_CPUS],
            nr_nodes: 1,
            nr_cpus,
        }
    }

    /// Create from ACPI SRAT/SLIT tables
    pub fn from_acpi(nodes: Vec<NumaNode>) -> Self {
        let nr_nodes = nodes.len() as u32;
        let nr_cpus = nodes.iter()
            .map(|n| n.cpus.count())
            .sum();

        let mut cpu_to_node = [0u32; MAX_CPUS];
        for (node_id, node) in nodes.iter().enumerate() {
            for cpu in node.cpus.iter() {
                cpu_to_node[cpu as usize] = node_id as u32;
            }
        }

        Self {
            nodes,
            cpu_to_node,
            nr_nodes,
            nr_cpus,
        }
    }

    /// Get the node for a CPU
    pub fn cpu_to_node(&self, cpu: u32) -> u32 {
        self.cpu_to_node.get(cpu as usize).copied().unwrap_or(0)
    }

    /// Get a node by ID
    pub fn node(&self, id: u32) -> Option<&NumaNode> {
        self.nodes.get(id as usize)
    }

    /// Get all nodes
    pub fn nodes(&self) -> &[NumaNode] {
        &self.nodes
    }

    /// Get CPUs in a node
    pub fn node_cpus(&self, node: u32) -> Option<&CpuSet> {
        self.nodes.get(node as usize).map(|n| &n.cpus)
    }

    /// Get distance between two nodes
    pub fn distance(&self, from: u32, to: u32) -> u32 {
        self.nodes.get(from as usize)
            .map(|n| n.distance_to(to))
            .unwrap_or(u32::MAX)
    }

    /// Find the closest node with available memory
    pub fn closest_node_with_memory(&self, from: u32, min_memory: u64) -> Option<u32> {
        let mut nodes_by_distance: Vec<_> = self.nodes.iter()
            .filter(|n| n.free_memory >= min_memory)
            .map(|n| (self.distance(from, n.id), n.id))
            .collect();

        nodes_by_distance.sort_by_key(|&(dist, _)| dist);
        nodes_by_distance.first().map(|&(_, id)| id)
    }

    /// Number of NUMA nodes
    pub fn nr_nodes(&self) -> u32 {
        self.nr_nodes
    }
}

/// CPU affinity manager
pub struct AffinityManager {
    /// Per-task affinity masks
    task_affinity: BTreeMap<Tid, CpuSet>,
    /// System-wide allowed CPUs
    system_cpus: CpuSet,
    /// NUMA topology
    numa: NumaTopology,
    /// Cache topology (CPUs sharing same cache)
    cache_siblings: Vec<CpuSet>,
    /// Number of CPUs
    nr_cpus: u32,
}

impl AffinityManager {
    /// Create a new affinity manager
    pub fn new(nr_cpus: u32) -> Self {
        Self {
            task_affinity: BTreeMap::new(),
            system_cpus: CpuSet::all(nr_cpus),
            numa: NumaTopology::flat(nr_cpus),
            cache_siblings: Vec::new(),
            nr_cpus,
        }
    }

    /// Set NUMA topology
    pub fn set_numa_topology(&mut self, numa: NumaTopology) {
        self.numa = numa;
    }

    /// Set cache siblings
    pub fn set_cache_siblings(&mut self, siblings: Vec<CpuSet>) {
        self.cache_siblings = siblings;
    }

    /// Get allowed CPUs for a task
    pub fn get_affinity(&self, tid: Tid) -> CpuSet {
        self.task_affinity
            .get(&tid)
            .cloned()
            .unwrap_or_else(|| self.system_cpus.clone())
    }

    /// Set affinity for a task
    pub fn set_affinity(&mut self, tid: Tid, cpus: CpuSet) -> Result<(), AffinityError> {
        // Must have at least one allowed CPU
        let effective = cpus.intersect(&self.system_cpus);
        if effective.is_empty() {
            return Err(AffinityError::InvalidMask);
        }
        self.task_affinity.insert(tid, effective);
        Ok(())
    }

    /// Clear affinity for a task (allow all CPUs)
    pub fn clear_affinity(&mut self, tid: Tid) {
        self.task_affinity.remove(&tid);
    }

    /// Check if a CPU is allowed for a task
    pub fn is_allowed(&self, tid: Tid, cpu: u32) -> bool {
        self.get_affinity(tid).contains(cpu)
    }

    /// Find the best CPU for a task
    pub fn find_best_cpu(&self, tid: Tid, hint_cpu: Option<u32>) -> Option<u32> {
        let allowed = self.get_affinity(tid);

        // If hint CPU is allowed, use it
        if let Some(hint) = hint_cpu {
            if allowed.contains(hint) {
                return Some(hint);
            }
        }

        // Find CPU in same NUMA node as hint
        if let Some(hint) = hint_cpu {
            let node = self.numa.cpu_to_node(hint);
            if let Some(node_cpus) = self.numa.node_cpus(node) {
                let local = allowed.intersect(node_cpus);
                if let Some(cpu) = local.first() {
                    return Some(cpu);
                }
            }
        }

        // Just pick first allowed CPU
        allowed.first()
    }

    /// Find CPUs that share cache with given CPU
    pub fn cache_siblings(&self, cpu: u32) -> CpuSet {
        for siblings in &self.cache_siblings {
            if siblings.contains(cpu) {
                return siblings.clone();
            }
        }
        CpuSet::single(cpu)
    }

    /// Get NUMA node for a CPU
    pub fn numa_node(&self, cpu: u32) -> u32 {
        self.numa.cpu_to_node(cpu)
    }

    /// Get CPUs in a NUMA node
    pub fn numa_cpus(&self, node: u32) -> Option<CpuSet> {
        self.numa.node_cpus(node).cloned()
    }

    /// Get NUMA topology reference
    pub fn numa_topology(&self) -> &NumaTopology {
        &self.numa
    }
}

/// Affinity errors
#[derive(Clone, Debug)]
pub enum AffinityError {
    /// No CPUs in mask are available
    InvalidMask,
    /// CPU does not exist
    InvalidCpu,
    /// Operation not permitted
    PermissionDenied,
}

/// CPU isolation for dedicated workloads
pub struct CpuIsolation {
    /// Isolated CPUs (no general scheduling)
    isolated: CpuSet,
    /// Housekeeping CPUs (handle IRQs, kernel threads)
    housekeeping: CpuSet,
    /// NOHZ full CPUs (no timer ticks)
    nohz_full: CpuSet,
    /// RCU nocb CPUs (offload RCU callbacks)
    rcu_nocbs: CpuSet,
}

impl CpuIsolation {
    /// Create with no isolation
    pub fn none(nr_cpus: u32) -> Self {
        Self {
            isolated: CpuSet::empty(),
            housekeeping: CpuSet::all(nr_cpus),
            nohz_full: CpuSet::empty(),
            rcu_nocbs: CpuSet::empty(),
        }
    }

    /// Set isolated CPUs
    pub fn set_isolated(&mut self, cpus: CpuSet) {
        self.isolated = cpus.clone();
        // Update housekeeping to exclude isolated
        for cpu in cpus.iter() {
            self.housekeeping.clear(cpu);
        }
    }

    /// Check if CPU is isolated
    pub fn is_isolated(&self, cpu: u32) -> bool {
        self.isolated.contains(cpu)
    }

    /// Check if CPU is housekeeping
    pub fn is_housekeeping(&self, cpu: u32) -> bool {
        self.housekeeping.contains(cpu)
    }

    /// Get housekeeping CPUs
    pub fn housekeeping_cpus(&self) -> &CpuSet {
        &self.housekeeping
    }

    /// Set NOHZ full CPUs
    pub fn set_nohz_full(&mut self, cpus: CpuSet) {
        self.nohz_full = cpus;
    }

    /// Check if CPU is NOHZ full
    pub fn is_nohz_full(&self, cpu: u32) -> bool {
        self.nohz_full.contains(cpu)
    }
}

/// Scheduling domain (hierarchical CPU grouping)
#[derive(Clone, Debug)]
pub struct SchedDomain {
    /// Domain level
    pub level: DomainLevel,
    /// CPUs in this domain
    pub span: CpuSet,
    /// Child domains
    pub children: Vec<usize>,
    /// Parent domain index
    pub parent: Option<usize>,
    /// Load balancing interval (in jiffies)
    pub balance_interval: u64,
    /// Flags
    pub flags: DomainFlags,
}

/// Domain hierarchy level
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomainLevel {
    /// SMT siblings (hyperthreads)
    Smt,
    /// CPU cores sharing L2 cache
    Mc,
    /// CPU package / socket
    Die,
    /// NUMA node
    Numa,
    /// Cross-NUMA
    MultiNuma,
}

bitflags::bitflags! {
    /// Domain flags
    #[derive(Clone, Copy, Debug)]
    pub struct DomainFlags: u32 {
        /// Share power domain
        const SHARE_POWERDOMAIN = 0x0001;
        /// Share package resources
        const SHARE_PKG_RESOURCES = 0x0002;
        /// NUMA domain
        const NUMA = 0x0004;
        /// Overlapping domains
        const OVERLAP = 0x0008;
        /// Prefer siblings for load balancing
        const PREFER_SIBLING = 0x0010;
    }
}

impl SchedDomain {
    /// Create a new scheduling domain
    pub fn new(level: DomainLevel, span: CpuSet) -> Self {
        Self {
            level,
            span,
            children: Vec::new(),
            parent: None,
            balance_interval: match level {
                DomainLevel::Smt => 1,
                DomainLevel::Mc => 4,
                DomainLevel::Die => 8,
                DomainLevel::Numa => 16,
                DomainLevel::MultiNuma => 32,
            },
            flags: DomainFlags::empty(),
        }
    }

    /// Check if CPU is in this domain
    pub fn contains(&self, cpu: u32) -> bool {
        self.span.contains(cpu)
    }

    /// Get number of CPUs in domain
    pub fn nr_cpus(&self) -> u32 {
        self.span.count()
    }
}

/// Build scheduling domain hierarchy
pub fn build_sched_domains(topology: &NumaTopology, nr_cpus: u32) -> Vec<SchedDomain> {
    let mut domains = Vec::new();

    // Create per-node domains
    for node in topology.nodes() {
        let mut domain = SchedDomain::new(DomainLevel::Numa, node.cpus.clone());
        domain.flags |= DomainFlags::NUMA;
        domains.push(domain);
    }

    // Create top-level domain spanning all CPUs
    if topology.nr_nodes() > 1 {
        let mut top = SchedDomain::new(DomainLevel::MultiNuma, CpuSet::all(nr_cpus));
        for i in 0..domains.len() {
            top.children.push(i);
            domains[i].parent = Some(domains.len());
        }
        domains.push(top);
    }

    domains
}
