//! BuildOS Read-Write Lock Implementation
//!
//! Allows multiple readers or a single writer.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicU32, Ordering};
use super::{cpu_pause, SpinWait, WaitQueue};

/// Reader count when write-locked
const WRITER: u32 = 0x80000000;
/// Maximum readers
const MAX_READERS: u32 = WRITER - 1;

/// A read-write lock
pub struct RwLock<T: ?Sized> {
    /// State: bits 0-30 = reader count, bit 31 = writer present
    state: AtomicU32,
    /// Writer waiters
    writer_waiters: WaitQueue,
    /// Reader waiters
    reader_waiters: WaitQueue,
    /// Protected data
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for RwLock<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for RwLock<T> {}

impl<T> RwLock<T> {
    /// Create a new read-write lock
    pub const fn new(data: T) -> Self {
        Self {
            state: AtomicU32::new(0),
            writer_waiters: WaitQueue::new(),
            reader_waiters: WaitQueue::new(),
            data: UnsafeCell::new(data),
        }
    }

    /// Consume the lock and return inner data
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }
}

impl<T: ?Sized> RwLock<T> {
    /// Acquire a read lock
    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        let mut spin = SpinWait::new();

        loop {
            let state = self.state.load(Ordering::Relaxed);

            // If no writer and not at max readers
            if state < MAX_READERS {
                if self.state
                    .compare_exchange_weak(
                        state,
                        state + 1,
                        Ordering::Acquire,
                        Ordering::Relaxed,
                    )
                    .is_ok()
                {
                    return RwLockReadGuard { lock: self };
                }
            } else {
                // Writer present, wait
                if !spin.spin_once() {
                    self.reader_waiters.wait();
                    spin.reset();
                }
            }
        }
    }

    /// Try to acquire a read lock
    pub fn try_read(&self) -> Option<RwLockReadGuard<'_, T>> {
        let mut state = self.state.load(Ordering::Relaxed);

        loop {
            if state >= MAX_READERS {
                return None;
            }

            match self.state.compare_exchange_weak(
                state,
                state + 1,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Some(RwLockReadGuard { lock: self }),
                Err(s) => state = s,
            }
        }
    }

    /// Acquire a write lock
    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        let mut spin = SpinWait::new();

        loop {
            // Try to set writer bit
            if self.state
                .compare_exchange_weak(0, WRITER, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return RwLockWriteGuard { lock: self };
            }

            if !spin.spin_once() {
                self.writer_waiters.wait();
                spin.reset();
            }
        }
    }

    /// Try to acquire a write lock
    pub fn try_write(&self) -> Option<RwLockWriteGuard<'_, T>> {
        if self.state
            .compare_exchange(0, WRITER, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(RwLockWriteGuard { lock: self })
        } else {
            None
        }
    }

    /// Release a read lock
    fn read_unlock(&self) {
        let state = self.state.fetch_sub(1, Ordering::Release);

        // If this was the last reader, wake a writer
        if state == 1 {
            self.writer_waiters.wake_one();
        }
    }

    /// Release a write lock
    fn write_unlock(&self) {
        self.state.store(0, Ordering::Release);

        // Wake all readers first, then a writer
        self.reader_waiters.wake_all();
        self.writer_waiters.wake_one();
    }

    /// Get mutable reference (requires exclusive access)
    pub fn get_mut(&mut self) -> &mut T {
        self.data.get_mut()
    }

    /// Check if locked for writing
    pub fn is_write_locked(&self) -> bool {
        (self.state.load(Ordering::Relaxed) & WRITER) != 0
    }

    /// Get current reader count
    pub fn reader_count(&self) -> u32 {
        self.state.load(Ordering::Relaxed) & MAX_READERS
    }
}

impl<T: ?Sized + Default> Default for RwLock<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> From<T> for RwLock<T> {
    fn from(data: T) -> Self {
        Self::new(data)
    }
}

/// RAII read guard
pub struct RwLockReadGuard<'a, T: ?Sized + 'a> {
    lock: &'a RwLock<T>,
}

impl<T: ?Sized> Deref for RwLockReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.read_unlock();
    }
}

/// RAII write guard
pub struct RwLockWriteGuard<'a, T: ?Sized + 'a> {
    lock: &'a RwLock<T>,
}

impl<T: ?Sized> Deref for RwLockWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T: ?Sized> Drop for RwLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.write_unlock();
    }
}

/// Writer-preferred read-write lock
pub struct WriterPreferRwLock<T: ?Sized> {
    /// Reader count
    readers: AtomicU32,
    /// Writer waiting
    writer_waiting: AtomicU32,
    /// Write lock held
    write_locked: AtomicU32,
    /// Wait queues
    reader_waiters: WaitQueue,
    writer_waiters: WaitQueue,
    /// Data
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for WriterPreferRwLock<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for WriterPreferRwLock<T> {}

impl<T> WriterPreferRwLock<T> {
    /// Create a new writer-preferred lock
    pub const fn new(data: T) -> Self {
        Self {
            readers: AtomicU32::new(0),
            writer_waiting: AtomicU32::new(0),
            write_locked: AtomicU32::new(0),
            reader_waiters: WaitQueue::new(),
            writer_waiters: WaitQueue::new(),
            data: UnsafeCell::new(data),
        }
    }
}

impl<T: ?Sized> WriterPreferRwLock<T> {
    /// Acquire read lock
    pub fn read(&self) -> WriterPreferReadGuard<'_, T> {
        let mut spin = SpinWait::new();

        loop {
            // Wait if writers are waiting or holding
            while self.writer_waiting.load(Ordering::Relaxed) > 0
                || self.write_locked.load(Ordering::Relaxed) > 0
            {
                if !spin.spin_once() {
                    self.reader_waiters.wait();
                    spin.reset();
                }
            }

            self.readers.fetch_add(1, Ordering::Acquire);

            // Double-check no writer snuck in
            if self.write_locked.load(Ordering::Relaxed) == 0 {
                return WriterPreferReadGuard { lock: self };
            }

            // Oops, writer got in, undo
            self.readers.fetch_sub(1, Ordering::Release);
        }
    }

    /// Acquire write lock
    pub fn write(&self) -> WriterPreferWriteGuard<'_, T> {
        // Signal we're waiting
        self.writer_waiting.fetch_add(1, Ordering::Relaxed);

        let mut spin = SpinWait::new();

        loop {
            // Wait for no readers
            while self.readers.load(Ordering::Relaxed) > 0 {
                if !spin.spin_once() {
                    self.writer_waiters.wait();
                    spin.reset();
                }
            }

            // Try to acquire
            if self.write_locked
                .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                // Double-check no readers
                if self.readers.load(Ordering::Relaxed) == 0 {
                    self.writer_waiting.fetch_sub(1, Ordering::Relaxed);
                    return WriterPreferWriteGuard { lock: self };
                }

                // Readers snuck in, release and retry
                self.write_locked.store(0, Ordering::Release);
            }
        }
    }
}

/// Read guard for writer-preferred lock
pub struct WriterPreferReadGuard<'a, T: ?Sized + 'a> {
    lock: &'a WriterPreferRwLock<T>,
}

impl<T: ?Sized> Deref for WriterPreferReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> Drop for WriterPreferReadGuard<'_, T> {
    fn drop(&mut self) {
        let readers = self.lock.readers.fetch_sub(1, Ordering::Release);
        if readers == 1 {
            self.lock.writer_waiters.wake_one();
        }
    }
}

/// Write guard for writer-preferred lock
pub struct WriterPreferWriteGuard<'a, T: ?Sized + 'a> {
    lock: &'a WriterPreferRwLock<T>,
}

impl<T: ?Sized> Deref for WriterPreferWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> DerefMut for WriterPreferWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T: ?Sized> Drop for WriterPreferWriteGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.write_locked.store(0, Ordering::Release);
        self.lock.reader_waiters.wake_all();
        self.lock.writer_waiters.wake_one();
    }
}

/// Sequence lock for read-mostly data
pub struct SeqLock<T: Copy> {
    /// Sequence counter (odd = write in progress)
    seq: AtomicU32,
    /// Data
    data: UnsafeCell<T>,
}

unsafe impl<T: Copy + Send> Send for SeqLock<T> {}
unsafe impl<T: Copy + Send> Sync for SeqLock<T> {}

impl<T: Copy> SeqLock<T> {
    /// Create a new sequence lock
    pub const fn new(data: T) -> Self {
        Self {
            seq: AtomicU32::new(0),
            data: UnsafeCell::new(data),
        }
    }

    /// Read the data (may retry on conflict)
    pub fn read(&self) -> T {
        loop {
            // Read sequence before data
            let seq1 = self.seq.load(Ordering::Acquire);

            // If write in progress, spin
            if (seq1 & 1) != 0 {
                cpu_pause();
                continue;
            }

            // Read data
            let data = unsafe { *self.data.get() };

            // Memory barrier
            core::sync::atomic::fence(Ordering::Acquire);

            // Check sequence didn't change
            let seq2 = self.seq.load(Ordering::Relaxed);
            if seq1 == seq2 {
                return data;
            }
        }
    }

    /// Write the data
    pub fn write(&self, data: T) {
        // Increment to odd (write in progress)
        self.seq.fetch_add(1, Ordering::Release);

        unsafe {
            *self.data.get() = data;
        }

        // Increment to even (write done)
        self.seq.fetch_add(1, Ordering::Release);
    }
}
