//! BuildOS Synchronization Primitives
//!
//! Provides kernel-level synchronization primitives including mutexes,
//! spinlocks, read-write locks, condition variables, and RCU.

#![allow(dead_code)]

pub mod mutex;
pub mod spinlock;
pub mod rwlock;
pub mod condvar;
pub mod semaphore;
pub mod barrier;
pub mod once;
pub mod futex;
pub mod rcu;
pub mod seqlock;

pub use mutex::{Mutex, MutexGuard};
pub use spinlock::{Spinlock, SpinlockGuard, RawSpinlock};
pub use rwlock::{RwLock, RwLockReadGuard, RwLockWriteGuard};
pub use condvar::Condvar;
pub use semaphore::Semaphore;
pub use barrier::Barrier;
pub use once::Once;
pub use rcu::{RcuPtr, RcuList, RcuReadGuard, SrcuDomain};
pub use seqlock::{SeqLock, SeqCount, SeqLatch};

use core::sync::atomic::{AtomicU64, Ordering};

/// Disable interrupts and return previous state
#[inline]
pub fn disable_interrupts() -> bool {
    let flags: u64;
    unsafe {
        core::arch::asm!(
            "pushfq",
            "pop {0}",
            "cli",
            out(reg) flags,
            options(nomem, preserves_flags)
        );
    }
    (flags & 0x200) != 0 // IF flag
}

/// Enable interrupts
#[inline]
pub fn enable_interrupts() {
    unsafe {
        core::arch::asm!("sti", options(nomem, preserves_flags));
    }
}

/// Restore interrupt state
#[inline]
pub fn restore_interrupts(enabled: bool) {
    if enabled {
        enable_interrupts();
    }
}

/// Check if interrupts are enabled
#[inline]
pub fn interrupts_enabled() -> bool {
    let flags: u64;
    unsafe {
        core::arch::asm!(
            "pushfq",
            "pop {0}",
            out(reg) flags,
            options(nomem, preserves_flags)
        );
    }
    (flags & 0x200) != 0
}

/// Critical section guard
pub struct CriticalSection {
    interrupts_were_enabled: bool,
}

impl CriticalSection {
    /// Enter a critical section
    #[inline]
    pub fn enter() -> Self {
        let interrupts_were_enabled = disable_interrupts();
        Self { interrupts_were_enabled }
    }
}

impl Drop for CriticalSection {
    #[inline]
    fn drop(&mut self) {
        restore_interrupts(self.interrupts_were_enabled);
    }
}

/// Execute closure in critical section
#[inline]
pub fn critical_section<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    let _guard = CriticalSection::enter();
    f()
}

/// Memory fence ordering helpers
pub mod fence {
    use core::sync::atomic::Ordering;
    use core::sync::atomic;

    /// Full memory barrier
    #[inline]
    pub fn full() {
        atomic::fence(Ordering::SeqCst);
    }

    /// Acquire barrier
    #[inline]
    pub fn acquire() {
        atomic::fence(Ordering::Acquire);
    }

    /// Release barrier
    #[inline]
    pub fn release() {
        atomic::fence(Ordering::Release);
    }

    /// Compiler barrier only
    #[inline]
    pub fn compiler() {
        atomic::compiler_fence(Ordering::SeqCst);
    }
}

/// CPU pause hint for spin loops
#[inline]
pub fn cpu_pause() {
    unsafe {
        core::arch::asm!("pause", options(nomem, nostack));
    }
}

/// Spin-wait with exponential backoff
pub struct SpinWait {
    counter: u32,
}

impl SpinWait {
    /// Create a new spin-wait
    pub const fn new() -> Self {
        Self { counter: 0 }
    }

    /// Reset the spin counter
    pub fn reset(&mut self) {
        self.counter = 0;
    }

    /// Spin once with backoff
    pub fn spin_once(&mut self) -> bool {
        if self.counter < 10 {
            // Just pause
            for _ in 0..1 << self.counter {
                cpu_pause();
            }
            self.counter += 1;
            true
        } else if self.counter < 20 {
            // Yield to scheduler
            self.counter += 1;
            // Would call scheduler::yield_now() here
            true
        } else {
            // Give up spinning
            false
        }
    }

    /// Check if we should yield instead of spinning
    pub fn should_yield(&self) -> bool {
        self.counter >= 10
    }
}

/// Wait queue for blocking synchronization
pub struct WaitQueue {
    /// Head of the queue
    head: AtomicU64,
    /// Tail of the queue
    tail: AtomicU64,
}

impl WaitQueue {
    /// Create a new wait queue
    pub const fn new() -> Self {
        Self {
            head: AtomicU64::new(0),
            tail: AtomicU64::new(0),
        }
    }

    /// Add current thread to wait queue and block
    pub fn wait(&self) {
        // In a real implementation, this would:
        // 1. Add current thread to the queue
        // 2. Put thread to sleep
        // 3. Wake up when signaled
        cpu_pause();
    }

    /// Wake one waiting thread
    pub fn wake_one(&self) {
        // Would wake the first thread in the queue
    }

    /// Wake all waiting threads
    pub fn wake_all(&self) {
        // Would wake all threads in the queue
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Acquire) == self.tail.load(Ordering::Acquire)
    }
}

/// Preemption control
pub struct PreemptGuard {
    was_enabled: bool,
}

impl PreemptGuard {
    /// Disable preemption
    pub fn disable() -> Self {
        // In a real implementation, this would increment
        // a per-CPU preempt_count
        Self { was_enabled: true }
    }
}

impl Drop for PreemptGuard {
    fn drop(&mut self) {
        // Would decrement preempt_count and potentially reschedule
    }
}

/// Atomic operations on pointers
pub mod atomic_ptr {
    use core::sync::atomic::{AtomicPtr, Ordering};
    

    /// Compare and swap a pointer
    #[inline]
    pub fn compare_exchange<T>(
        atomic: &AtomicPtr<T>,
        current: *mut T,
        new: *mut T,
    ) -> Result<*mut T, *mut T> {
        atomic.compare_exchange(current, new, Ordering::AcqRel, Ordering::Acquire)
    }

    /// Swap a pointer
    #[inline]
    pub fn swap<T>(atomic: &AtomicPtr<T>, new: *mut T) -> *mut T {
        atomic.swap(new, Ordering::AcqRel)
    }
}

/// Per-CPU data
#[repr(C)]
pub struct PerCpu<T> {
    /// Data for each CPU
    data: [T; 256], // Max CPUs
}

impl<T: Default + Copy> PerCpu<T> {
    /// Create new per-CPU data
    pub const fn new(default: T) -> Self {
        Self {
            data: [default; 256],
        }
    }

    /// Get reference to current CPU's data
    pub fn get(&self) -> &T {
        let cpu_id = get_cpu_id();
        &self.data[cpu_id]
    }

    /// Get mutable reference to current CPU's data
    pub fn get_mut(&mut self) -> &mut T {
        let cpu_id = get_cpu_id();
        &mut self.data[cpu_id]
    }

    /// Get reference to specific CPU's data
    pub fn get_for(&self, cpu: usize) -> &T {
        &self.data[cpu]
    }
}

/// Get current CPU ID
pub fn get_cpu_id() -> usize {
    crate::cpu::current_cpu_id() as usize
}
