// ===============================================================================
// BUILDOS KERNEL - READ-COPY-UPDATE (RCU)
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Read-Copy-Update (RCU) synchronization primitive.
//!
//! RCU is a lock-free synchronization mechanism that allows:
//! - Extremely fast read-side critical sections (no locks)
//! - Writers can update data structures without blocking readers
//! - Safe memory reclamation after all readers complete
//!
//! This is essential for high-performance kernel data structures.

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use spin::Mutex;

use crate::sched::MAX_CPUS;

// =============================================================================
// CONSTANTS
// =============================================================================

/// Maximum pending callbacks
const MAX_CALLBACKS: usize = 4096;

/// Grace period batch size
const GP_BATCH_SIZE: u64 = 1000;

// =============================================================================
// STATE
// =============================================================================

/// Global RCU state
static RCU_STATE: RcuState = RcuState::new();

/// Per-CPU RCU data
static mut PER_CPU_RCU: [PerCpuRcu; MAX_CPUS] = {
    const INIT: PerCpuRcu = PerCpuRcu::new();
    [INIT; MAX_CPUS]
};

/// RCU initialized flag
static RCU_INITIALIZED: AtomicBool = AtomicBool::new(false);

// =============================================================================
// RCU STATE
// =============================================================================

/// Global RCU state
struct RcuState {
    /// Current grace period number
    gp_seq: AtomicU64,

    /// Completed grace period number
    gp_completed: AtomicU64,

    /// Number of CPUs that need to pass through quiescent state
    gp_waiting: AtomicUsize,

    /// Number of online CPUs
    num_cpus: AtomicUsize,

    /// Grace period in progress
    gp_in_progress: AtomicBool,

    /// Callbacks awaiting grace period
    callbacks: Mutex<VecDeque<RcuCallback>>,
}

impl RcuState {
    const fn new() -> Self {
        Self {
            gp_seq: AtomicU64::new(1),
            gp_completed: AtomicU64::new(0),
            gp_waiting: AtomicUsize::new(0),
            num_cpus: AtomicUsize::new(1),
            gp_in_progress: AtomicBool::new(false),
            callbacks: Mutex::new(VecDeque::new()),
        }
    }
}

/// Per-CPU RCU data
struct PerCpuRcu {
    /// CPU is in quiescent state
    quiescent: AtomicBool,

    /// Last grace period this CPU passed
    last_gp: AtomicU64,

    /// RCU read lock nesting depth
    read_lock_nesting: AtomicUsize,

    /// CPU is online
    online: AtomicBool,

    /// Local callback queue
    local_callbacks: Mutex<Vec<RcuCallback>>,

    /// Callbacks processed
    callbacks_processed: AtomicU64,
}

impl PerCpuRcu {
    const fn new() -> Self {
        Self {
            quiescent: AtomicBool::new(true),
            last_gp: AtomicU64::new(0),
            read_lock_nesting: AtomicUsize::new(0),
            online: AtomicBool::new(false),
            local_callbacks: Mutex::new(Vec::new()),
            callbacks_processed: AtomicU64::new(0),
        }
    }
}

/// RCU callback
struct RcuCallback {
    /// Function to call
    func: Box<dyn FnOnce() + Send>,

    /// Grace period when this callback was registered
    gp_registered: u64,
}

// =============================================================================
// RCU PROTECTED POINTER
// =============================================================================

/// RCU-protected pointer
///
/// Allows lock-free read access to shared data while writers
/// can safely update the pointer.
pub struct RcuPtr<T: Send + Sync> {
    /// Pointer to current data
    ptr: AtomicUsize,

    /// Phantom data for type
    _marker: PhantomData<T>,
}

impl<T: Send + Sync> RcuPtr<T> {
    /// Create new RCU pointer
    pub fn new(data: T) -> Self {
        let boxed = Box::new(data);
        Self {
            ptr: AtomicUsize::new(Box::into_raw(boxed) as usize),
            _marker: PhantomData,
        }
    }

    /// Create null RCU pointer
    pub const fn null() -> Self {
        Self {
            ptr: AtomicUsize::new(0),
            _marker: PhantomData,
        }
    }

    /// Read the pointer (must be in RCU read-side critical section)
    ///
    /// # Safety
    ///
    /// Caller must be in rcu_read_lock() critical section.
    pub unsafe fn read(&self) -> Option<&T> {
        let ptr = self.ptr.load(Ordering::Acquire);
        if ptr == 0 {
            None
        } else {
            Some(&*(ptr as *const T))
        }
    }

    /// Update the pointer (swap with new value)
    ///
    /// Returns the old value. Caller must ensure the old value
    /// is freed after a grace period (use call_rcu).
    pub fn swap(&self, new: T) -> Option<Box<T>> {
        let new_ptr = Box::into_raw(Box::new(new)) as usize;
        let old_ptr = self.ptr.swap(new_ptr, Ordering::AcqRel);

        if old_ptr == 0 {
            None
        } else {
            // Old pointer is no longer visible to new readers,
            // but existing readers may still have references.
            // Caller must wait for grace period before freeing.
            Some(unsafe { Box::from_raw(old_ptr as *mut T) })
        }
    }

    /// Set to null
    pub fn clear(&self) -> Option<Box<T>> {
        let old_ptr = self.ptr.swap(0, Ordering::AcqRel);

        if old_ptr == 0 {
            None
        } else {
            Some(unsafe { Box::from_raw(old_ptr as *mut T) })
        }
    }

    /// Check if pointer is null
    pub fn is_null(&self) -> bool {
        self.ptr.load(Ordering::Relaxed) == 0
    }
}

unsafe impl<T: Send + Sync> Send for RcuPtr<T> {}
unsafe impl<T: Send + Sync> Sync for RcuPtr<T> {}

// =============================================================================
// RCU READ GUARD
// =============================================================================

/// Guard for RCU read-side critical section
pub struct RcuReadGuard {
    /// CPU ID when lock was acquired
    cpu: usize,
}

impl Drop for RcuReadGuard {
    fn drop(&mut self) {
        rcu_read_unlock_inner(self.cpu);
    }
}

// =============================================================================
// PUBLIC INTERFACE
// =============================================================================

/// Initialize RCU subsystem
pub fn init(num_cpus: usize) {
    RCU_STATE.num_cpus.store(num_cpus, Ordering::Release);

    // Mark CPU 0 (BSP) as online
    unsafe {
        PER_CPU_RCU[0].online.store(true, Ordering::Release);
    }

    RCU_INITIALIZED.store(true, Ordering::Release);
}

/// Register a CPU as online (for SMP)
pub fn cpu_online(cpu: usize) {
    if cpu >= MAX_CPUS {
        return;
    }

    unsafe {
        PER_CPU_RCU[cpu].online.store(true, Ordering::Release);
        PER_CPU_RCU[cpu].quiescent.store(true, Ordering::Release);
    }
}

/// Register a CPU as offline
pub fn cpu_offline(cpu: usize) {
    if cpu >= MAX_CPUS {
        return;
    }

    unsafe {
        PER_CPU_RCU[cpu].online.store(false, Ordering::Release);
    }

    // Report quiescent state for offline CPU
    rcu_report_qs(cpu);
}

/// Enter RCU read-side critical section
///
/// Returns a guard that automatically unlocks when dropped.
/// No RCU-protected data may be accessed outside of this section.
pub fn rcu_read_lock() -> RcuReadGuard {
    let cpu = crate::cpu::current_cpu_id() as usize;

    unsafe {
        if cpu < MAX_CPUS {
            PER_CPU_RCU[cpu].read_lock_nesting.fetch_add(1, Ordering::Acquire);
        }
    }

    RcuReadGuard { cpu }
}

/// Exit RCU read-side critical section (internal)
fn rcu_read_unlock_inner(cpu: usize) {
    unsafe {
        if cpu < MAX_CPUS {
            let old = PER_CPU_RCU[cpu].read_lock_nesting.fetch_sub(1, Ordering::Release);

            // If we're the last reader, report quiescent state
            if old == 1 {
                PER_CPU_RCU[cpu].quiescent.store(true, Ordering::Release);
            }
        }
    }
}

/// Check if currently in RCU read-side critical section
pub fn rcu_read_lock_held() -> bool {
    let cpu = crate::cpu::current_cpu_id() as usize;

    unsafe {
        if cpu < MAX_CPUS {
            PER_CPU_RCU[cpu].read_lock_nesting.load(Ordering::Relaxed) > 0
        } else {
            false
        }
    }
}

/// Wait for all current readers to complete (synchronize_rcu)
///
/// This is the classic RCU operation. After this function returns,
/// all readers that were active when this function was called have
/// completed their RCU read-side critical sections.
pub fn synchronize() {
    if !RCU_INITIALIZED.load(Ordering::Relaxed) {
        return;
    }

    // Start a new grace period
    start_gp();

    // Wait for it to complete
    wait_gp();
}

/// Schedule a callback to run after grace period
///
/// The callback will be called after all current readers complete.
/// This is non-blocking (unlike synchronize_rcu).
pub fn call_rcu<F>(callback: F)
where
    F: FnOnce() + Send + 'static,
{
    if !RCU_INITIALIZED.load(Ordering::Relaxed) {
        // If RCU not initialized, just call immediately
        callback();
        return;
    }

    let cb = RcuCallback {
        func: Box::new(callback),
        gp_registered: RCU_STATE.gp_seq.load(Ordering::Acquire),
    };

    // Add to global callback queue
    let mut callbacks = RCU_STATE.callbacks.lock();
    callbacks.push_back(cb);

    // Start grace period if not already in progress
    if !RCU_STATE.gp_in_progress.load(Ordering::Relaxed) {
        drop(callbacks);
        start_gp();
    }
}

/// Free memory after RCU grace period
pub fn kfree_rcu<T: Send + 'static>(ptr: Box<T>) {
    call_rcu(move || {
        drop(ptr);
    });
}

/// Report quiescent state (called from scheduler)
///
/// Call this when a CPU goes through a quiescent state (context switch,
/// idle, etc.). This helps advance RCU grace periods.
pub fn rcu_qs() {
    let cpu = crate::cpu::current_cpu_id() as usize;
    rcu_report_qs(cpu);
}

/// Report quiescent state for specific CPU
fn rcu_report_qs(cpu: usize) {
    if cpu >= MAX_CPUS {
        return;
    }

    let current_gp = RCU_STATE.gp_seq.load(Ordering::Acquire);

    unsafe {
        let last_gp = PER_CPU_RCU[cpu].last_gp.load(Ordering::Relaxed);

        if last_gp < current_gp {
            PER_CPU_RCU[cpu].last_gp.store(current_gp, Ordering::Release);
            PER_CPU_RCU[cpu].quiescent.store(true, Ordering::Release);

            // Decrement waiting count
            let old = RCU_STATE.gp_waiting.fetch_sub(1, Ordering::AcqRel);

            if old == 1 {
                // All CPUs have passed through quiescent state
                complete_gp();
            }
        }
    }
}

/// Called on context switch
pub fn rcu_note_context_switch() {
    let cpu = crate::cpu::current_cpu_id() as usize;

    unsafe {
        if cpu < MAX_CPUS {
            // Context switch implies quiescent state (if not in read section)
            if PER_CPU_RCU[cpu].read_lock_nesting.load(Ordering::Relaxed) == 0 {
                rcu_report_qs(cpu);
            }
        }
    }
}

/// Called when CPU goes idle
pub fn rcu_idle_enter() {
    let cpu = crate::cpu::current_cpu_id() as usize;

    if cpu < MAX_CPUS {
        // Idle implies quiescent state
        rcu_report_qs(cpu);
    }
}

/// Called when CPU exits idle
pub fn rcu_idle_exit() {
    // Nothing special needed
}

// =============================================================================
// GRACE PERIOD MANAGEMENT
// =============================================================================

/// Start a new grace period
fn start_gp() {
    // Check if one is already in progress
    if RCU_STATE.gp_in_progress.swap(true, Ordering::AcqRel) {
        return;
    }

    // Increment grace period sequence number
    RCU_STATE.gp_seq.fetch_add(1, Ordering::Release);

    // Count online CPUs that need to report quiescent state
    let mut count = 0;
    let num_cpus = RCU_STATE.num_cpus.load(Ordering::Relaxed);

    for cpu in 0..num_cpus.min(MAX_CPUS) {
        unsafe {
            if PER_CPU_RCU[cpu].online.load(Ordering::Relaxed) {
                PER_CPU_RCU[cpu].quiescent.store(false, Ordering::Release);
                count += 1;
            }
        }
    }

    RCU_STATE.gp_waiting.store(count, Ordering::Release);

    // If no CPUs waiting, complete immediately
    if count == 0 {
        complete_gp();
    }
}

/// Wait for current grace period to complete
fn wait_gp() {
    let current_gp = RCU_STATE.gp_seq.load(Ordering::Acquire);

    // Spin until grace period completes
    while RCU_STATE.gp_completed.load(Ordering::Acquire) < current_gp {
        // Help by reporting our own quiescent state
        rcu_qs();

        // Yield CPU
        core::hint::spin_loop();
    }
}

/// Complete current grace period
fn complete_gp() {
    let current_gp = RCU_STATE.gp_seq.load(Ordering::Acquire);
    RCU_STATE.gp_completed.store(current_gp, Ordering::Release);
    RCU_STATE.gp_in_progress.store(false, Ordering::Release);

    // Process callbacks
    process_callbacks(current_gp);
}

/// Process callbacks that are now safe to run
fn process_callbacks(completed_gp: u64) {
    let mut callbacks = RCU_STATE.callbacks.lock();

    // Move callbacks that can be executed to a local list
    let mut ready: Vec<RcuCallback> = Vec::new();

    while let Some(front) = callbacks.front() {
        if front.gp_registered < completed_gp {
            if let Some(cb) = callbacks.pop_front() {
                ready.push(cb);
            }
        } else {
            break;
        }
    }

    drop(callbacks);

    // Execute callbacks (without holding lock)
    for cb in ready {
        (cb.func)();
    }
}

// =============================================================================
// RCU LIST OPERATIONS
// =============================================================================

/// RCU-protected list head
pub struct RcuList<T: Send + Sync> {
    head: RcuPtr<RcuListNode<T>>,
    len: AtomicUsize,
}

/// RCU list node
pub struct RcuListNode<T: Send + Sync> {
    data: T,
    next: RcuPtr<RcuListNode<T>>,
}

impl<T: Send + Sync> RcuList<T> {
    /// Create new empty list
    pub const fn new() -> Self {
        Self {
            head: RcuPtr::null(),
            len: AtomicUsize::new(0),
        }
    }

    /// Check if list is empty
    pub fn is_empty(&self) -> bool {
        self.head.is_null()
    }

    /// Get list length
    pub fn len(&self) -> usize {
        self.len.load(Ordering::Relaxed)
    }

    /// Push to front of list
    pub fn push_front(&self, data: T) {
        let new_node = Box::new(RcuListNode {
            data,
            next: RcuPtr::null(),
        });

        let new_ptr = Box::into_raw(new_node) as usize;

        loop {
            let old_head = self.head.ptr.load(Ordering::Acquire);

            // Set new node's next to current head
            unsafe {
                let new_node = &mut *(new_ptr as *mut RcuListNode<T>);
                new_node.next.ptr.store(old_head, Ordering::Release);
            }

            // Try to swap head
            if self.head.ptr.compare_exchange(
                old_head,
                new_ptr,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ).is_ok() {
                self.len.fetch_add(1, Ordering::Relaxed);
                break;
            }
        }
    }

    /// Iterate over list (must be in RCU read section)
    ///
    /// # Safety
    ///
    /// Caller must hold rcu_read_lock().
    pub unsafe fn iter(&self) -> RcuListIter<'_, T> {
        RcuListIter {
            current: self.head.read().map(|n| n as *const RcuListNode<T>),
            _marker: PhantomData,
        }
    }
}

/// Iterator over RCU list
pub struct RcuListIter<'a, T: Send + Sync> {
    current: Option<*const RcuListNode<T>>,
    _marker: PhantomData<&'a T>,
}

impl<'a, T: Send + Sync> Iterator for RcuListIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current?;

        unsafe {
            let node = &*current;
            self.current = node.next.read().map(|n| n as *const RcuListNode<T>);
            Some(&node.data)
        }
    }
}

// =============================================================================
// SRCU (SLEEPABLE RCU)
// =============================================================================

/// Sleepable RCU domain
///
/// Unlike regular RCU, readers can sleep in SRCU critical sections.
pub struct SrcuDomain {
    /// Per-CPU reader counts
    per_cpu_counts: [AtomicUsize; MAX_CPUS],

    /// Grace period sequence
    gp_seq: AtomicU64,

    /// Lock for writers
    writer_lock: Mutex<()>,
}

impl SrcuDomain {
    /// Create new SRCU domain
    pub const fn new() -> Self {
        const ZERO: AtomicUsize = AtomicUsize::new(0);
        Self {
            per_cpu_counts: [ZERO; MAX_CPUS],
            gp_seq: AtomicU64::new(0),
            writer_lock: Mutex::new(()),
        }
    }

    /// Enter SRCU read-side critical section
    pub fn read_lock(&self) -> SrcuReadGuard<'_> {
        let cpu = crate::cpu::current_cpu_id() as usize;

        if cpu < MAX_CPUS {
            self.per_cpu_counts[cpu].fetch_add(1, Ordering::Acquire);
        }

        SrcuReadGuard {
            domain: self,
            cpu,
        }
    }

    /// Synchronize SRCU (wait for readers)
    pub fn synchronize(&self) {
        let _lock = self.writer_lock.lock();

        // Increment grace period
        self.gp_seq.fetch_add(1, Ordering::Release);

        // Wait for all readers to complete
        loop {
            let mut count = 0;
            for cpu in 0..MAX_CPUS {
                count += self.per_cpu_counts[cpu].load(Ordering::Acquire);
            }

            if count == 0 {
                break;
            }

            core::hint::spin_loop();
        }
    }
}

/// SRCU read guard
pub struct SrcuReadGuard<'a> {
    domain: &'a SrcuDomain,
    cpu: usize,
}

impl Drop for SrcuReadGuard<'_> {
    fn drop(&mut self) {
        if self.cpu < MAX_CPUS {
            self.domain.per_cpu_counts[self.cpu].fetch_sub(1, Ordering::Release);
        }
    }
}

// =============================================================================
// STATISTICS
// =============================================================================

/// RCU statistics
#[derive(Default)]
pub struct RcuStats {
    /// Current grace period
    pub gp_seq: u64,
    /// Completed grace period
    pub gp_completed: u64,
    /// CPUs waiting for quiescent state
    pub gp_waiting: usize,
    /// Grace period in progress
    pub gp_in_progress: bool,
    /// Pending callbacks
    pub callbacks_pending: usize,
}

/// Get RCU statistics
pub fn get_stats() -> RcuStats {
    RcuStats {
        gp_seq: RCU_STATE.gp_seq.load(Ordering::Relaxed),
        gp_completed: RCU_STATE.gp_completed.load(Ordering::Relaxed),
        gp_waiting: RCU_STATE.gp_waiting.load(Ordering::Relaxed),
        gp_in_progress: RCU_STATE.gp_in_progress.load(Ordering::Relaxed),
        callbacks_pending: RCU_STATE.callbacks.lock().len(),
    }
}
