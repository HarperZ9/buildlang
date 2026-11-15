//! BuildOS Spinlock Implementation
//!
//! Spin-based locking primitives for interrupt handlers and
//! other contexts where blocking is not possible.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use super::{cpu_pause, disable_interrupts, restore_interrupts};

/// Raw spinlock without data protection
pub struct RawSpinlock {
    locked: AtomicBool,
}

impl RawSpinlock {
    /// Create a new raw spinlock
    pub const fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
        }
    }

    /// Acquire the lock
    #[inline]
    pub fn lock(&self) {
        while self.locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            // Spin with pause hint
            while self.locked.load(Ordering::Relaxed) {
                cpu_pause();
            }
        }
    }

    /// Try to acquire the lock
    #[inline]
    pub fn try_lock(&self) -> bool {
        self.locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    /// Release the lock
    #[inline]
    pub fn unlock(&self) {
        self.locked.store(false, Ordering::Release);
    }

    /// Check if locked
    #[inline]
    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Relaxed)
    }
}

impl Default for RawSpinlock {
    fn default() -> Self {
        Self::new()
    }
}

/// Spinlock with data protection
pub struct Spinlock<T: ?Sized> {
    lock: RawSpinlock,
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for Spinlock<T> {}
unsafe impl<T: ?Sized + Send> Sync for Spinlock<T> {}

impl<T> Spinlock<T> {
    /// Create a new spinlock
    pub const fn new(data: T) -> Self {
        Self {
            lock: RawSpinlock::new(),
            data: UnsafeCell::new(data),
        }
    }

    /// Consume the lock and return inner data
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }
}

impl<T: ?Sized> Spinlock<T> {
    /// Acquire the lock
    #[inline]
    pub fn lock(&self) -> SpinlockGuard<'_, T> {
        self.lock.lock();
        SpinlockGuard { lock: self }
    }

    /// Try to acquire the lock
    #[inline]
    pub fn try_lock(&self) -> Option<SpinlockGuard<'_, T>> {
        if self.lock.try_lock() {
            Some(SpinlockGuard { lock: self })
        } else {
            None
        }
    }

    /// Check if locked
    #[inline]
    pub fn is_locked(&self) -> bool {
        self.lock.is_locked()
    }

    /// Get mutable reference (requires exclusive access)
    pub fn get_mut(&mut self) -> &mut T {
        self.data.get_mut()
    }
}

impl<T: ?Sized + Default> Default for Spinlock<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> From<T> for Spinlock<T> {
    fn from(data: T) -> Self {
        Self::new(data)
    }
}

/// RAII guard for spinlock
pub struct SpinlockGuard<'a, T: ?Sized + 'a> {
    lock: &'a Spinlock<T>,
}

impl<T: ?Sized> Deref for SpinlockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> DerefMut for SpinlockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T: ?Sized> Drop for SpinlockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.lock.unlock();
    }
}

/// Spinlock that disables interrupts
pub struct IrqSpinlock<T: ?Sized> {
    lock: RawSpinlock,
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for IrqSpinlock<T> {}
unsafe impl<T: ?Sized + Send> Sync for IrqSpinlock<T> {}

impl<T> IrqSpinlock<T> {
    /// Create a new IRQ spinlock
    pub const fn new(data: T) -> Self {
        Self {
            lock: RawSpinlock::new(),
            data: UnsafeCell::new(data),
        }
    }
}

impl<T: ?Sized> IrqSpinlock<T> {
    /// Acquire the lock and disable interrupts
    #[inline]
    pub fn lock(&self) -> IrqSpinlockGuard<'_, T> {
        let irq_enabled = disable_interrupts();
        self.lock.lock();
        IrqSpinlockGuard {
            lock: self,
            irq_enabled,
        }
    }

    /// Try to acquire the lock
    #[inline]
    pub fn try_lock(&self) -> Option<IrqSpinlockGuard<'_, T>> {
        let irq_enabled = disable_interrupts();
        if self.lock.try_lock() {
            Some(IrqSpinlockGuard {
                lock: self,
                irq_enabled,
            })
        } else {
            restore_interrupts(irq_enabled);
            None
        }
    }
}

/// RAII guard for IRQ spinlock
pub struct IrqSpinlockGuard<'a, T: ?Sized + 'a> {
    lock: &'a IrqSpinlock<T>,
    irq_enabled: bool,
}

impl<T: ?Sized> Deref for IrqSpinlockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> DerefMut for IrqSpinlockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T: ?Sized> Drop for IrqSpinlockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.lock.unlock();
        restore_interrupts(self.irq_enabled);
    }
}

/// Ticket spinlock for fairness
pub struct TicketSpinlock<T: ?Sized> {
    serving: AtomicU32,
    next: AtomicU32,
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for TicketSpinlock<T> {}
unsafe impl<T: ?Sized + Send> Sync for TicketSpinlock<T> {}

impl<T> TicketSpinlock<T> {
    /// Create a new ticket spinlock
    pub const fn new(data: T) -> Self {
        Self {
            serving: AtomicU32::new(0),
            next: AtomicU32::new(0),
            data: UnsafeCell::new(data),
        }
    }
}

impl<T: ?Sized> TicketSpinlock<T> {
    /// Acquire the lock
    #[inline]
    pub fn lock(&self) -> TicketSpinlockGuard<'_, T> {
        let ticket = self.next.fetch_add(1, Ordering::Relaxed);

        while self.serving.load(Ordering::Acquire) != ticket {
            cpu_pause();
        }

        TicketSpinlockGuard { lock: self }
    }
}

/// RAII guard for ticket spinlock
pub struct TicketSpinlockGuard<'a, T: ?Sized + 'a> {
    lock: &'a TicketSpinlock<T>,
}

impl<T: ?Sized> Deref for TicketSpinlockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> DerefMut for TicketSpinlockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T: ?Sized> Drop for TicketSpinlockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.serving.fetch_add(1, Ordering::Release);
    }
}

/// Queued spinlock (MCS lock) for better scalability
pub struct QueuedSpinlock {
    tail: AtomicU64,
}

/// Node in the queued spinlock queue
#[repr(align(64))]
pub struct QueuedSpinlockNode {
    next: AtomicU64,
    locked: AtomicBool,
}

use core::sync::atomic::AtomicU64;

impl QueuedSpinlock {
    /// Create a new queued spinlock
    pub const fn new() -> Self {
        Self {
            tail: AtomicU64::new(0),
        }
    }

    /// Acquire the lock
    pub fn lock(&self, node: &mut QueuedSpinlockNode) {
        node.next.store(0, Ordering::Relaxed);
        node.locked.store(true, Ordering::Relaxed);

        let prev = self.tail.swap(node as *mut _ as u64, Ordering::AcqRel);

        if prev != 0 {
            // Link to predecessor
            let prev_node = unsafe { &*(prev as *const QueuedSpinlockNode) };
            prev_node.next.store(node as *mut _ as u64, Ordering::Release);

            // Spin on local flag
            while node.locked.load(Ordering::Acquire) {
                cpu_pause();
            }
        }
    }

    /// Release the lock
    pub fn unlock(&self, node: &QueuedSpinlockNode) {
        let next = node.next.load(Ordering::Acquire);

        if next == 0 {
            // Try to clear tail
            if self.tail
                .compare_exchange(
                    node as *const _ as u64,
                    0,
                    Ordering::Release,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                return;
            }

            // Wait for successor to link
            loop {
                let next = node.next.load(Ordering::Acquire);
                if next != 0 {
                    let next_node = unsafe { &*(next as *const QueuedSpinlockNode) };
                    next_node.locked.store(false, Ordering::Release);
                    return;
                }
                cpu_pause();
            }
        } else {
            let next_node = unsafe { &*(next as *const QueuedSpinlockNode) };
            next_node.locked.store(false, Ordering::Release);
        }
    }
}

impl QueuedSpinlockNode {
    /// Create a new node
    pub const fn new() -> Self {
        Self {
            next: AtomicU64::new(0),
            locked: AtomicBool::new(false),
        }
    }
}
