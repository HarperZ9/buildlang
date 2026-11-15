//! BuildOS Semaphore Implementation
//!
//! Counting and binary semaphores for resource management.

#![allow(dead_code)]

use core::sync::atomic::{AtomicU32, AtomicI32, Ordering};
use super::{WaitQueue, cpu_pause, SpinWait};

/// Counting semaphore
pub struct Semaphore {
    /// Current count
    count: AtomicI32,
    /// Maximum count (0 = unlimited)
    max_count: u32,
    /// Waiters
    waiters: WaitQueue,
}

impl Semaphore {
    /// Create a new semaphore with initial count
    pub const fn new(initial: u32) -> Self {
        Self {
            count: AtomicI32::new(initial as i32),
            max_count: 0,
            waiters: WaitQueue::new(),
        }
    }

    /// Create a bounded semaphore
    pub const fn bounded(initial: u32, max: u32) -> Self {
        Self {
            count: AtomicI32::new(initial as i32),
            max_count: max,
            waiters: WaitQueue::new(),
        }
    }

    /// Create a binary semaphore (mutex-like)
    pub const fn binary(initial: bool) -> Self {
        Self {
            count: AtomicI32::new(if initial { 1 } else { 0 }),
            max_count: 1,
            waiters: WaitQueue::new(),
        }
    }

    /// Acquire (wait/down/P operation)
    pub fn acquire(&self) {
        let mut spin = SpinWait::new();

        loop {
            let count = self.count.load(Ordering::Relaxed);

            if count > 0 {
                if self.count
                    .compare_exchange_weak(
                        count,
                        count - 1,
                        Ordering::Acquire,
                        Ordering::Relaxed,
                    )
                    .is_ok()
                {
                    return;
                }
            } else {
                if !spin.spin_once() {
                    self.waiters.wait();
                    spin.reset();
                }
            }
        }
    }

    /// Try to acquire without blocking
    pub fn try_acquire(&self) -> bool {
        let mut count = self.count.load(Ordering::Relaxed);

        loop {
            if count <= 0 {
                return false;
            }

            match self.count.compare_exchange_weak(
                count,
                count - 1,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(c) => count = c,
            }
        }
    }

    /// Acquire multiple permits
    pub fn acquire_many(&self, n: u32) {
        for _ in 0..n {
            self.acquire();
        }
    }

    /// Try to acquire multiple permits
    pub fn try_acquire_many(&self, n: u32) -> bool {
        let mut spin = SpinWait::new();
        let n = n as i32;

        loop {
            let count = self.count.load(Ordering::Relaxed);

            if count >= n {
                if self.count
                    .compare_exchange_weak(
                        count,
                        count - n,
                        Ordering::Acquire,
                        Ordering::Relaxed,
                    )
                    .is_ok()
                {
                    return true;
                }
                spin.reset();
            } else {
                return false;
            }
        }
    }

    /// Release (signal/up/V operation)
    pub fn release(&self) {
        let old = self.count.fetch_add(1, Ordering::Release);

        // Check for overflow if bounded
        if self.max_count > 0 && old >= self.max_count as i32 {
            // Undo the increment
            self.count.fetch_sub(1, Ordering::Relaxed);
            return;
        }

        // Wake one waiter
        self.waiters.wake_one();
    }

    /// Release multiple permits
    pub fn release_many(&self, n: u32) {
        for _ in 0..n {
            self.release();
        }
    }

    /// Get current count
    pub fn available(&self) -> i32 {
        self.count.load(Ordering::Relaxed)
    }

    /// Check if the semaphore can be acquired
    pub fn is_available(&self) -> bool {
        self.count.load(Ordering::Relaxed) > 0
    }
}

impl Default for Semaphore {
    fn default() -> Self {
        Self::new(0)
    }
}

/// A permit from a semaphore (RAII guard)
pub struct SemaphorePermit<'a> {
    semaphore: &'a Semaphore,
    count: u32,
}

impl<'a> SemaphorePermit<'a> {
    /// Acquire a permit
    pub fn acquire(semaphore: &'a Semaphore) -> Self {
        semaphore.acquire();
        Self {
            semaphore,
            count: 1,
        }
    }

    /// Acquire multiple permits
    pub fn acquire_many(semaphore: &'a Semaphore, n: u32) -> Self {
        semaphore.acquire_many(n);
        Self {
            semaphore,
            count: n,
        }
    }

    /// Try to acquire a permit
    pub fn try_acquire(semaphore: &'a Semaphore) -> Option<Self> {
        if semaphore.try_acquire() {
            Some(Self {
                semaphore,
                count: 1,
            })
        } else {
            None
        }
    }

    /// Forget the permit (don't release on drop)
    pub fn forget(self) {
        core::mem::forget(self);
    }
}

impl Drop for SemaphorePermit<'_> {
    fn drop(&mut self) {
        self.semaphore.release_many(self.count);
    }
}

/// Lightweight spinlock-based semaphore
pub struct SpinSemaphore {
    count: AtomicI32,
}

impl SpinSemaphore {
    /// Create a new spin semaphore
    pub const fn new(initial: u32) -> Self {
        Self {
            count: AtomicI32::new(initial as i32),
        }
    }

    /// Acquire
    pub fn acquire(&self) {
        loop {
            let count = self.count.load(Ordering::Relaxed);

            if count > 0 {
                if self.count
                    .compare_exchange_weak(
                        count,
                        count - 1,
                        Ordering::Acquire,
                        Ordering::Relaxed,
                    )
                    .is_ok()
                {
                    return;
                }
            }

            cpu_pause();
        }
    }

    /// Try acquire
    pub fn try_acquire(&self) -> bool {
        let mut count = self.count.load(Ordering::Relaxed);

        while count > 0 {
            match self.count.compare_exchange_weak(
                count,
                count - 1,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(c) => count = c,
            }
        }

        false
    }

    /// Release
    pub fn release(&self) {
        self.count.fetch_add(1, Ordering::Release);
    }

    /// Get count
    pub fn available(&self) -> i32 {
        self.count.load(Ordering::Relaxed)
    }
}

impl Default for SpinSemaphore {
    fn default() -> Self {
        Self::new(0)
    }
}

/// Resource pool with fixed capacity
pub struct ResourcePool<T, const N: usize> {
    /// Available resources
    resources: [Option<T>; N],
    /// Semaphore for availability
    semaphore: Semaphore,
    /// Index of next available
    next: AtomicU32,
    /// Mutex for allocation
    alloc_lock: super::Spinlock<()>,
}

impl<T, const N: usize> ResourcePool<T, N> {
    /// Acquire a resource from the pool
    pub fn acquire(&self) -> Option<PoolGuard<'_, T, N>> {
        self.semaphore.acquire();

        let _lock = self.alloc_lock.lock();

        for i in 0..N {
            let _idx = (self.next.load(Ordering::Relaxed) as usize + i) % N;
            // Would need interior mutability for this to work properly
            // This is a simplified version
        }

        None // Placeholder
    }

    /// Release a resource back to the pool
    fn release(&self, _resource: T) {
        // Would add resource back
        self.semaphore.release();
    }
}

/// Guard for pool resource
pub struct PoolGuard<'a, T, const N: usize> {
    pool: &'a ResourcePool<T, N>,
    resource: T,
}

impl<T, const N: usize> core::ops::Deref for PoolGuard<'_, T, N> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.resource
    }
}

impl<T, const N: usize> core::ops::DerefMut for PoolGuard<'_, T, N> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.resource
    }
}

impl<T, const N: usize> Drop for PoolGuard<'_, T, N> {
    fn drop(&mut self) {
        // Would release back to pool
        self.pool.semaphore.release();
    }
}
