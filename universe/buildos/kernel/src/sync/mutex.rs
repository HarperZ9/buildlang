//! BuildOS Mutex Implementation
//!
//! A mutual exclusion primitive for protecting shared data.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use super::{cpu_pause, SpinWait, WaitQueue};

/// A mutual exclusion lock
pub struct Mutex<T: ?Sized> {
    /// Lock state
    locked: AtomicBool,
    /// Owner thread ID (for debugging)
    owner: AtomicU32,
    /// Wait queue for blocked threads
    waiters: WaitQueue,
    /// Protected data
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for Mutex<T> {}
unsafe impl<T: ?Sized + Send> Sync for Mutex<T> {}

impl<T> Mutex<T> {
    /// Create a new mutex
    pub const fn new(data: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            owner: AtomicU32::new(0),
            waiters: WaitQueue::new(),
            data: UnsafeCell::new(data),
        }
    }

    /// Consume the mutex and return the inner data
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }
}

impl<T: ?Sized> Mutex<T> {
    /// Acquire the lock
    pub fn lock(&self) -> MutexGuard<'_, T> {
        let mut spin = SpinWait::new();

        loop {
            // Try to acquire
            if self.try_lock_inner() {
                return MutexGuard { mutex: self };
            }

            // Spin or yield
            if !spin.spin_once() {
                // Would block on waiters queue
                self.waiters.wait();
                spin.reset();
            }
        }
    }

    /// Try to acquire the lock without blocking
    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        if self.try_lock_inner() {
            Some(MutexGuard { mutex: self })
        } else {
            None
        }
    }

    /// Internal try lock
    fn try_lock_inner(&self) -> bool {
        self.locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    /// Release the lock
    fn unlock(&self) {
        self.owner.store(0, Ordering::Relaxed);
        self.locked.store(false, Ordering::Release);
        self.waiters.wake_one();
    }

    /// Check if the mutex is locked
    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Relaxed)
    }

    /// Get a mutable reference to the underlying data
    pub fn get_mut(&mut self) -> &mut T {
        self.data.get_mut()
    }
}

impl<T: ?Sized + Default> Default for Mutex<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> From<T> for Mutex<T> {
    fn from(data: T) -> Self {
        Self::new(data)
    }
}

/// RAII guard for mutex
pub struct MutexGuard<'a, T: ?Sized + 'a> {
    mutex: &'a Mutex<T>,
}

impl<T: ?Sized> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.data.get() }
    }
}

impl<T: ?Sized> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.unlock();
    }
}

// MutexGuard is not Send
impl<T: ?Sized> !Send for MutexGuard<'_, T> {}

/// A ticket-based fair mutex
pub struct TicketMutex<T: ?Sized> {
    /// Next ticket to be served
    serving: AtomicU32,
    /// Next ticket to be taken
    next: AtomicU32,
    /// Protected data
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for TicketMutex<T> {}
unsafe impl<T: ?Sized + Send> Sync for TicketMutex<T> {}

impl<T> TicketMutex<T> {
    /// Create a new ticket mutex
    pub const fn new(data: T) -> Self {
        Self {
            serving: AtomicU32::new(0),
            next: AtomicU32::new(0),
            data: UnsafeCell::new(data),
        }
    }
}

impl<T: ?Sized> TicketMutex<T> {
    /// Acquire the lock
    pub fn lock(&self) -> TicketMutexGuard<'_, T> {
        let ticket = self.next.fetch_add(1, Ordering::Relaxed);

        while self.serving.load(Ordering::Acquire) != ticket {
            cpu_pause();
        }

        TicketMutexGuard { mutex: self }
    }

    /// Release the lock
    fn unlock(&self) {
        self.serving.fetch_add(1, Ordering::Release);
    }
}

/// RAII guard for ticket mutex
pub struct TicketMutexGuard<'a, T: ?Sized + 'a> {
    mutex: &'a TicketMutex<T>,
}

impl<T: ?Sized> Deref for TicketMutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<T: ?Sized> DerefMut for TicketMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.data.get() }
    }
}

impl<T: ?Sized> Drop for TicketMutexGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.unlock();
    }
}

/// A recursive mutex (re-entrant)
pub struct RecursiveMutex<T: ?Sized> {
    /// Lock state
    locked: AtomicBool,
    /// Owner thread ID
    owner: AtomicU32,
    /// Recursion count
    count: AtomicU32,
    /// Wait queue
    waiters: WaitQueue,
    /// Protected data
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for RecursiveMutex<T> {}
unsafe impl<T: ?Sized + Send> Sync for RecursiveMutex<T> {}

impl<T> RecursiveMutex<T> {
    /// Create a new recursive mutex
    pub const fn new(data: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            owner: AtomicU32::new(0),
            count: AtomicU32::new(0),
            waiters: WaitQueue::new(),
            data: UnsafeCell::new(data),
        }
    }
}

impl<T: ?Sized> RecursiveMutex<T> {
    /// Acquire the lock
    pub fn lock(&self) -> RecursiveMutexGuard<'_, T> {
        let current_thread = current_thread_id();

        // Check if we already own it
        if self.owner.load(Ordering::Relaxed) == current_thread {
            self.count.fetch_add(1, Ordering::Relaxed);
            return RecursiveMutexGuard { mutex: self };
        }

        // Acquire the lock
        let mut spin = SpinWait::new();
        loop {
            if self.locked
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                self.owner.store(current_thread, Ordering::Relaxed);
                self.count.store(1, Ordering::Relaxed);
                return RecursiveMutexGuard { mutex: self };
            }

            if !spin.spin_once() {
                self.waiters.wait();
                spin.reset();
            }
        }
    }

    /// Release the lock
    fn unlock(&self) {
        let count = self.count.fetch_sub(1, Ordering::Relaxed);
        if count == 1 {
            self.owner.store(0, Ordering::Relaxed);
            self.locked.store(false, Ordering::Release);
            self.waiters.wake_one();
        }
    }
}

/// RAII guard for recursive mutex
pub struct RecursiveMutexGuard<'a, T: ?Sized + 'a> {
    mutex: &'a RecursiveMutex<T>,
}

impl<T: ?Sized> Deref for RecursiveMutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<T: ?Sized> DerefMut for RecursiveMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.data.get() }
    }
}

impl<T: ?Sized> Drop for RecursiveMutexGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.unlock();
    }
}

/// Get current thread ID (placeholder)
fn current_thread_id() -> u32 {
    // Would read from thread-local or per-CPU data
    1
}
