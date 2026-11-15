//! BuildOS Barrier Implementation
//!
//! Synchronization barrier for coordinating multiple threads.

#![allow(dead_code)]

use core::sync::atomic::{AtomicU32, Ordering};
use crate::math::F32Ext;
use super::{WaitQueue, cpu_pause, SpinWait};

/// A barrier that blocks threads until all have arrived
pub struct Barrier {
    /// Number of threads to wait for
    threshold: u32,
    /// Current count
    count: AtomicU32,
    /// Generation (to handle reuse)
    generation: AtomicU32,
    /// Waiters
    waiters: WaitQueue,
}

impl Barrier {
    /// Create a new barrier
    pub const fn new(n: u32) -> Self {
        Self {
            threshold: n,
            count: AtomicU32::new(0),
            generation: AtomicU32::new(0),
            waiters: WaitQueue::new(),
        }
    }

    /// Wait at the barrier
    ///
    /// Returns true for exactly one thread (the "leader")
    pub fn wait(&self) -> bool {
        let gen = self.generation.load(Ordering::Acquire);
        let arrived = self.count.fetch_add(1, Ordering::AcqRel) + 1;

        if arrived == self.threshold {
            // We're the last one - reset and wake everyone
            self.count.store(0, Ordering::Release);
            self.generation.fetch_add(1, Ordering::Release);
            self.waiters.wake_all();
            true
        } else {
            // Wait for the barrier to be released
            let mut spin = SpinWait::new();
            while self.generation.load(Ordering::Acquire) == gen {
                if !spin.spin_once() {
                    self.waiters.wait();
                    spin.reset();
                }
            }
            false
        }
    }

    /// Get the number of threads that have arrived
    pub fn arrived(&self) -> u32 {
        self.count.load(Ordering::Relaxed)
    }

    /// Get the threshold
    pub fn threshold(&self) -> u32 {
        self.threshold
    }

    /// Check if the barrier is complete
    pub fn is_complete(&self) -> bool {
        self.count.load(Ordering::Relaxed) == 0
    }
}

/// Result of waiting at a barrier
pub struct BarrierWaitResult {
    is_leader: bool,
}

impl BarrierWaitResult {
    /// Returns true if this thread is the "leader"
    pub fn is_leader(&self) -> bool {
        self.is_leader
    }
}

/// Spin-based barrier
pub struct SpinBarrier {
    threshold: u32,
    count: AtomicU32,
    generation: AtomicU32,
}

impl SpinBarrier {
    /// Create a new spin barrier
    pub const fn new(n: u32) -> Self {
        Self {
            threshold: n,
            count: AtomicU32::new(0),
            generation: AtomicU32::new(0),
        }
    }

    /// Wait at the barrier
    pub fn wait(&self) -> bool {
        let gen = self.generation.load(Ordering::Acquire);
        let arrived = self.count.fetch_add(1, Ordering::AcqRel) + 1;

        if arrived == self.threshold {
            self.count.store(0, Ordering::Release);
            self.generation.fetch_add(1, Ordering::Release);
            true
        } else {
            while self.generation.load(Ordering::Acquire) == gen {
                cpu_pause();
            }
            false
        }
    }
}

/// Sense-reversing barrier for better cache behavior
pub struct SenseBarrier {
    threshold: u32,
    count: AtomicU32,
    sense: AtomicU32,
}

impl SenseBarrier {
    /// Create a new sense barrier
    pub const fn new(n: u32) -> Self {
        Self {
            threshold: n,
            count: AtomicU32::new(0),
            sense: AtomicU32::new(0),
        }
    }

    /// Wait at the barrier with local sense
    pub fn wait(&self, local_sense: &mut bool) {
        *local_sense = !*local_sense;
        let my_sense = if *local_sense { 1 } else { 0 };

        let arrived = self.count.fetch_add(1, Ordering::AcqRel) + 1;

        if arrived == self.threshold {
            self.count.store(0, Ordering::Relaxed);
            self.sense.store(my_sense, Ordering::Release);
        } else {
            while self.sense.load(Ordering::Acquire) != my_sense {
                cpu_pause();
            }
        }
    }
}

/// Tree barrier for better scalability with many threads
pub struct TreeBarrier {
    /// Number of threads
    n: u32,
    /// Log2 of radix
    log_radix: u32,
    /// Radix (number of children per node)
    radix: u32,
    /// Nodes in the tree
    nodes: [BarrierNode; 64], // Max 64 nodes
}

/// Node in tree barrier
struct BarrierNode {
    parent: Option<usize>,
    children: u32,
    arrived: AtomicU32,
    sense: AtomicU32,
}

impl BarrierNode {
    const fn new() -> Self {
        Self {
            parent: None,
            children: 0,
            arrived: AtomicU32::new(0),
            sense: AtomicU32::new(0),
        }
    }
}

impl TreeBarrier {
    /// Create a new tree barrier
    pub fn new(n: u32, radix: u32) -> Self {
        let log_radix = (radix as f32).log2() as u32;

        let mut barrier = Self {
            n,
            log_radix,
            radix,
            nodes: core::array::from_fn(|_| BarrierNode::new()),
        };

        // Initialize tree structure
        for i in 0..n as usize {
            if i > 0 {
                barrier.nodes[i].parent = Some((i - 1) / radix as usize);
                barrier.nodes[(i - 1) / radix as usize].children += 1;
            }
        }

        barrier
    }

    /// Wait at the barrier
    pub fn wait(&self, id: usize, local_sense: &mut bool) {
        *local_sense = !*local_sense;
        let my_sense = if *local_sense { 1 } else { 0 };

        // Arrive at leaf
        let node = &self.nodes[id];

        // Wait for children
        while node.arrived.load(Ordering::Acquire) != node.children {
            cpu_pause();
        }
        node.arrived.store(0, Ordering::Relaxed);

        // Signal parent
        if let Some(parent_id) = node.parent {
            self.nodes[parent_id].arrived.fetch_add(1, Ordering::Release);
        }

        // If root, broadcast
        if node.parent.is_none() {
            self.nodes[id].sense.store(my_sense, Ordering::Release);
        } else {
            // Wait for broadcast
            while self.nodes[0].sense.load(Ordering::Acquire) != my_sense {
                cpu_pause();
            }
        }
    }
}

/// Phaser - advanced barrier with phases and optional participation
pub struct Phaser {
    /// Current phase
    phase: AtomicU32,
    /// Number of registered parties
    parties: AtomicU32,
    /// Number of arrived parties
    arrived: AtomicU32,
    /// Waiters
    waiters: WaitQueue,
}

impl Phaser {
    /// Create a new phaser
    pub const fn new(parties: u32) -> Self {
        Self {
            phase: AtomicU32::new(0),
            parties: AtomicU32::new(parties),
            arrived: AtomicU32::new(0),
            waiters: WaitQueue::new(),
        }
    }

    /// Register a new party
    pub fn register(&self) -> u32 {
        self.parties.fetch_add(1, Ordering::AcqRel)
    }

    /// Deregister a party
    pub fn deregister(&self) {
        self.parties.fetch_sub(1, Ordering::AcqRel);
    }

    /// Arrive at the barrier (without waiting)
    pub fn arrive(&self) -> u32 {
        let arrived = self.arrived.fetch_add(1, Ordering::AcqRel) + 1;
        let parties = self.parties.load(Ordering::Acquire);

        if arrived == parties {
            self.arrived.store(0, Ordering::Release);
            let phase = self.phase.fetch_add(1, Ordering::Release);
            self.waiters.wake_all();
            phase + 1
        } else {
            self.phase.load(Ordering::Acquire)
        }
    }

    /// Arrive and wait for others
    pub fn arrive_and_await(&self) -> u32 {
        let current_phase = self.phase.load(Ordering::Acquire);
        let arrived = self.arrived.fetch_add(1, Ordering::AcqRel) + 1;
        let parties = self.parties.load(Ordering::Acquire);

        if arrived == parties {
            self.arrived.store(0, Ordering::Release);
            let new_phase = self.phase.fetch_add(1, Ordering::Release) + 1;
            self.waiters.wake_all();
            new_phase
        } else {
            // Wait for phase to advance
            let mut spin = SpinWait::new();
            while self.phase.load(Ordering::Acquire) == current_phase {
                if !spin.spin_once() {
                    self.waiters.wait();
                    spin.reset();
                }
            }
            self.phase.load(Ordering::Acquire)
        }
    }

    /// Arrive and deregister
    pub fn arrive_and_deregister(&self) -> u32 {
        self.parties.fetch_sub(1, Ordering::AcqRel);
        self.arrive()
    }

    /// Get current phase
    pub fn phase(&self) -> u32 {
        self.phase.load(Ordering::Acquire)
    }

    /// Get number of registered parties
    pub fn registered_parties(&self) -> u32 {
        self.parties.load(Ordering::Acquire)
    }

    /// Get number of arrived parties
    pub fn arrived_parties(&self) -> u32 {
        self.arrived.load(Ordering::Acquire)
    }
}
