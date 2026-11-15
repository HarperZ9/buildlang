//! BuildOS Condition Variable Implementation
//!
//! Allows threads to wait for a condition to become true.

use core::sync::atomic::{AtomicU32, Ordering};
use super::{Mutex, MutexGuard, WaitQueue, cpu_pause};

/// Condition variable
pub struct Condvar {
    /// Wait queue
    waiters: WaitQueue,
    /// Waiter count
    count: AtomicU32,
    /// Generation counter for spurious wakeup prevention
    generation: AtomicU32,
}

impl Condvar {
    /// Create a new condition variable
    pub const fn new() -> Self {
        Self {
            waiters: WaitQueue::new(),
            count: AtomicU32::new(0),
            generation: AtomicU32::new(0),
        }
    }

    /// Wait on the condition variable
    ///
    /// The mutex is released while waiting and reacquired before returning.
    pub fn wait<'a, T>(&self, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
        self.count.fetch_add(1, Ordering::Relaxed);
        let gen = self.generation.load(Ordering::Acquire);

        // Get reference to mutex before dropping guard
        let mutex: &Mutex<T> = unsafe {
            // SAFETY: We have the guard, so the mutex exists
            &*(&*guard as *const T).cast::<()>().cast::<Mutex<T>>().sub(1)
        };

        // Drop the guard (releases mutex)
        drop(guard);

        // Wait for signal
        while self.generation.load(Ordering::Acquire) == gen {
            self.waiters.wait();
        }

        self.count.fetch_sub(1, Ordering::Relaxed);

        // Reacquire mutex
        mutex.lock()
    }

    /// Wait with a predicate
    pub fn wait_while<'a, T, F>(
        &self,
        mut guard: MutexGuard<'a, T>,
        mut condition: F,
    ) -> MutexGuard<'a, T>
    where
        F: FnMut(&mut T) -> bool,
    {
        while condition(&mut *guard) {
            guard = self.wait(guard);
        }
        guard
    }

    /// Signal one waiting thread
    pub fn notify_one(&self) {
        if self.count.load(Ordering::Relaxed) > 0 {
            self.generation.fetch_add(1, Ordering::Release);
            self.waiters.wake_one();
        }
    }

    /// Signal all waiting threads
    pub fn notify_all(&self) {
        if self.count.load(Ordering::Relaxed) > 0 {
            self.generation.fetch_add(1, Ordering::Release);
            self.waiters.wake_all();
        }
    }

    /// Check if any threads are waiting
    pub fn has_waiters(&self) -> bool {
        self.count.load(Ordering::Relaxed) > 0
    }
}

impl Default for Condvar {
    fn default() -> Self {
        Self::new()
    }
}

/// A simpler version that works with raw spinlocks
pub struct SpinCondvar {
    /// Wait flag
    waiting: AtomicU32,
    /// Signaled count
    signaled: AtomicU32,
}

impl SpinCondvar {
    /// Create a new spin condition variable
    pub const fn new() -> Self {
        Self {
            waiting: AtomicU32::new(0),
            signaled: AtomicU32::new(0),
        }
    }

    /// Wait for signal
    pub fn wait(&self) {
        let ticket = self.waiting.fetch_add(1, Ordering::Relaxed);

        while self.signaled.load(Ordering::Acquire) <= ticket {
            cpu_pause();
        }
    }

    /// Signal one waiter
    pub fn notify_one(&self) {
        if self.waiting.load(Ordering::Relaxed) > self.signaled.load(Ordering::Relaxed) {
            self.signaled.fetch_add(1, Ordering::Release);
        }
    }

    /// Signal all waiters
    pub fn notify_all(&self) {
        self.signaled.store(self.waiting.load(Ordering::Relaxed), Ordering::Release);
    }
}

impl Default for SpinCondvar {
    fn default() -> Self {
        Self::new()
    }
}

/// Event - signalable one-shot or auto-reset wait primitive
pub struct Event {
    /// Is signaled
    signaled: AtomicU32,
    /// Auto-reset
    auto_reset: bool,
    /// Waiters
    waiters: WaitQueue,
}

impl Event {
    /// Create a new manual-reset event
    pub const fn new() -> Self {
        Self {
            signaled: AtomicU32::new(0),
            auto_reset: false,
            waiters: WaitQueue::new(),
        }
    }

    /// Create a new auto-reset event
    pub const fn auto_reset() -> Self {
        Self {
            signaled: AtomicU32::new(0),
            auto_reset: true,
            waiters: WaitQueue::new(),
        }
    }

    /// Wait for the event to be signaled
    pub fn wait(&self) {
        loop {
            if self.auto_reset {
                // Try to consume the signal
                if self.signaled.compare_exchange(
                    1,
                    0,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                ).is_ok() {
                    return;
                }
            } else {
                if self.signaled.load(Ordering::Acquire) != 0 {
                    return;
                }
            }

            self.waiters.wait();
        }
    }

    /// Check if signaled without waiting
    pub fn try_wait(&self) -> bool {
        if self.auto_reset {
            self.signaled.compare_exchange(
                1,
                0,
                Ordering::Acquire,
                Ordering::Relaxed,
            ).is_ok()
        } else {
            self.signaled.load(Ordering::Acquire) != 0
        }
    }

    /// Signal the event
    pub fn set(&self) {
        self.signaled.store(1, Ordering::Release);
        if self.auto_reset {
            self.waiters.wake_one();
        } else {
            self.waiters.wake_all();
        }
    }

    /// Reset the event
    pub fn reset(&self) {
        self.signaled.store(0, Ordering::Release);
    }

    /// Check if signaled
    pub fn is_set(&self) -> bool {
        self.signaled.load(Ordering::Relaxed) != 0
    }
}

impl Default for Event {
    fn default() -> Self {
        Self::new()
    }
}

/// Monitor - combines mutex and condition variable
pub struct Monitor<T> {
    /// Mutex protecting the data
    mutex: Mutex<T>,
    /// Condition variable
    condvar: Condvar,
}

impl<T> Monitor<T> {
    /// Create a new monitor
    pub const fn new(data: T) -> Self {
        Self {
            mutex: Mutex::new(data),
            condvar: Condvar::new(),
        }
    }

    /// Lock the monitor
    pub fn lock(&self) -> MonitorGuard<'_, T> {
        MonitorGuard {
            guard: self.mutex.lock(),
            condvar: &self.condvar,
        }
    }

    /// Try to lock the monitor
    pub fn try_lock(&self) -> Option<MonitorGuard<'_, T>> {
        self.mutex.try_lock().map(|guard| MonitorGuard {
            guard,
            condvar: &self.condvar,
        })
    }
}

impl<T: Default> Default for Monitor<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

/// Guard for monitor
pub struct MonitorGuard<'a, T> {
    guard: MutexGuard<'a, T>,
    condvar: &'a Condvar,
}

impl<'a, T> MonitorGuard<'a, T> {
    /// Wait on the condition variable
    pub fn wait(self) -> Self {
        let guard = self.condvar.wait(self.guard);
        Self {
            guard,
            condvar: self.condvar,
        }
    }

    /// Wait with a predicate
    pub fn wait_while<F>(mut self, mut condition: F) -> Self
    where
        F: FnMut(&mut T) -> bool,
    {
        while condition(&mut *self.guard) {
            self = self.wait();
        }
        self
    }

    /// Signal one waiting thread
    pub fn notify_one(&self) {
        self.condvar.notify_one();
    }

    /// Signal all waiting threads
    pub fn notify_all(&self) {
        self.condvar.notify_all();
    }
}

impl<T> core::ops::Deref for MonitorGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.guard
    }
}

impl<T> core::ops::DerefMut for MonitorGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.guard
    }
}
