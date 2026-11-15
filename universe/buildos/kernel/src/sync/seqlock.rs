// ===============================================================================
// BUILDOS KERNEL - SEQUENCE LOCKS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Sequence locks for reader-writer synchronization.
//!
//! Seqlocks provide a mechanism for very fast reads and slower writes.
//! Writers increment a sequence counter before and after writing.
//! Readers check the sequence number to detect concurrent modifications.
//!
//! Unlike RwLock, readers never block writers, making seqlocks ideal
//! for frequently-read, rarely-written data like time.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU64, Ordering};

// =============================================================================
// SEQLOCK
// =============================================================================

/// Sequence lock
///
/// Provides reader-writer synchronization where writers never block.
/// Readers retry if a write occurred during their read.
pub struct SeqLock<T> {
    /// Sequence counter (odd = write in progress)
    seq: AtomicU64,

    /// Protected data
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Send for SeqLock<T> {}
unsafe impl<T: Send> Sync for SeqLock<T> {}

impl<T: Copy> SeqLock<T> {
    /// Create a new seqlock
    pub const fn new(value: T) -> Self {
        Self {
            seq: AtomicU64::new(0),
            data: UnsafeCell::new(value),
        }
    }

    /// Read the value
    ///
    /// Retries automatically if a write occurred during the read.
    pub fn read(&self) -> T {
        loop {
            // Read sequence before reading data
            let seq1 = self.seq.load(Ordering::Acquire);

            // If odd, writer is active - spin
            if seq1 & 1 != 0 {
                core::hint::spin_loop();
                continue;
            }

            // Read the data
            let value = unsafe { *self.data.get() };

            // Memory barrier to ensure we see writes before sequence update
            core::sync::atomic::fence(Ordering::Acquire);

            // Read sequence after reading data
            let seq2 = self.seq.load(Ordering::Relaxed);

            // If sequences match and even, read was consistent
            if seq1 == seq2 {
                return value;
            }

            // Writer was active during read, retry
            core::hint::spin_loop();
        }
    }

    /// Write a new value
    pub fn write(&self, value: T) {
        // Increment sequence (now odd = write in progress)
        self.seq.fetch_add(1, Ordering::Release);

        // Write the data
        unsafe {
            *self.data.get() = value;
        }

        // Memory barrier to ensure data is visible
        core::sync::atomic::fence(Ordering::Release);

        // Increment sequence again (now even = write complete)
        self.seq.fetch_add(1, Ordering::Release);
    }

    /// Read sequence number
    ///
    /// Returns the current sequence (for low-level use).
    pub fn read_begin(&self) -> u64 {
        loop {
            let seq = self.seq.load(Ordering::Acquire);
            if seq & 1 == 0 {
                return seq;
            }
            core::hint::spin_loop();
        }
    }

    /// Check if read needs retry
    pub fn read_retry(&self, start_seq: u64) -> bool {
        core::sync::atomic::fence(Ordering::Acquire);
        self.seq.load(Ordering::Relaxed) != start_seq
    }

    /// Try to read without blocking
    ///
    /// Returns None if writer is active.
    pub fn try_read(&self) -> Option<T> {
        let seq1 = self.seq.load(Ordering::Acquire);

        // If odd, writer is active
        if seq1 & 1 != 0 {
            return None;
        }

        let value = unsafe { *self.data.get() };

        core::sync::atomic::fence(Ordering::Acquire);

        let seq2 = self.seq.load(Ordering::Relaxed);

        if seq1 == seq2 {
            Some(value)
        } else {
            None
        }
    }
}

// =============================================================================
// SEQCOUNT
// =============================================================================

/// Sequence counter (for protecting data protected by another lock)
///
/// Use when data is already protected by a lock but you want lock-free reads.
pub struct SeqCount {
    /// Sequence counter
    seq: AtomicU64,
}

impl SeqCount {
    /// Create a new sequence counter
    pub const fn new() -> Self {
        Self {
            seq: AtomicU64::new(0),
        }
    }

    /// Start a read section
    ///
    /// Returns the sequence number to pass to read_retry().
    pub fn read_begin(&self) -> u64 {
        loop {
            let seq = self.seq.load(Ordering::Acquire);
            if seq & 1 == 0 {
                return seq;
            }
            core::hint::spin_loop();
        }
    }

    /// Check if read needs to be retried
    pub fn read_retry(&self, start_seq: u64) -> bool {
        core::sync::atomic::fence(Ordering::Acquire);
        self.seq.load(Ordering::Relaxed) != start_seq
    }

    /// Start a write section (caller must hold exclusive lock)
    pub fn write_begin(&self) {
        // Increment to odd value
        let seq = self.seq.load(Ordering::Relaxed);
        self.seq.store(seq + 1, Ordering::Release);
    }

    /// End a write section
    pub fn write_end(&self) {
        core::sync::atomic::fence(Ordering::Release);
        // Increment to even value
        let seq = self.seq.load(Ordering::Relaxed);
        self.seq.store(seq + 1, Ordering::Release);
    }

    /// Get current sequence number
    pub fn sequence(&self) -> u64 {
        self.seq.load(Ordering::Relaxed)
    }
}

// =============================================================================
// SEQLOCK WITH GUARD
// =============================================================================

/// Seqlock with RAII write guard
pub struct SeqLockGuarded<T> {
    /// The seqlock
    inner: SeqLock<T>,

    /// Write lock
    write_lock: spin::Mutex<()>,
}

impl<T: Copy> SeqLockGuarded<T> {
    /// Create a new seqlock with guard
    pub const fn new(value: T) -> Self {
        Self {
            inner: SeqLock::new(value),
            write_lock: spin::Mutex::new(()),
        }
    }

    /// Read the value
    pub fn read(&self) -> T {
        self.inner.read()
    }

    /// Acquire write access
    pub fn write(&self) -> SeqLockWriteGuard<'_, T> {
        let _lock = self.write_lock.lock();

        // Start write
        self.inner.seq.fetch_add(1, Ordering::Release);

        SeqLockWriteGuard {
            seqlock: self,
            _lock,
        }
    }
}

/// Write guard for SeqLockGuarded
pub struct SeqLockWriteGuard<'a, T: Copy> {
    seqlock: &'a SeqLockGuarded<T>,
    _lock: spin::MutexGuard<'a, ()>,
}

impl<'a, T: Copy> SeqLockWriteGuard<'a, T> {
    /// Write a new value
    pub fn write(&mut self, value: T) {
        unsafe {
            *self.seqlock.inner.data.get() = value;
        }
    }

    /// Read current value
    pub fn read(&self) -> T {
        unsafe { *self.seqlock.inner.data.get() }
    }
}

impl<'a, T: Copy> Drop for SeqLockWriteGuard<'a, T> {
    fn drop(&mut self) {
        core::sync::atomic::fence(Ordering::Release);
        self.seqlock.inner.seq.fetch_add(1, Ordering::Release);
    }
}

// =============================================================================
// RAW SEQCOUNT WITH LOCK
// =============================================================================

/// Seqcount protected by an external lock
///
/// The user must ensure the lock is held when calling write_* methods.
pub struct SeqCountLock<L> {
    /// Sequence counter
    seqcount: SeqCount,

    /// Associated lock (for debug/verification)
    lock: L,
}

impl<L> SeqCountLock<L> {
    /// Create new seqcount with lock
    pub const fn new(lock: L) -> Self {
        Self {
            seqcount: SeqCount::new(),
            lock,
        }
    }

    /// Get reference to the lock
    pub fn lock(&self) -> &L {
        &self.lock
    }

    /// Start read section
    pub fn read_begin(&self) -> u64 {
        self.seqcount.read_begin()
    }

    /// Check if read needs retry
    pub fn read_retry(&self, start: u64) -> bool {
        self.seqcount.read_retry(start)
    }

    /// Start write section (caller must hold lock)
    pub fn write_begin(&self) {
        self.seqcount.write_begin();
    }

    /// End write section
    pub fn write_end(&self) {
        self.seqcount.write_end();
    }
}

// =============================================================================
// LATCH (ONE-TIME SEQUENCE)
// =============================================================================

/// Latch for one-time initialization with seqcount semantics
///
/// Readers can spin-wait for initialization to complete.
pub struct SeqLatch<T> {
    /// Sequence counter (0 = not initialized, 1 = initializing, 2 = done)
    seq: AtomicU64,

    /// Data
    data: UnsafeCell<Option<T>>,
}

unsafe impl<T: Send> Send for SeqLatch<T> {}
unsafe impl<T: Send + Sync> Sync for SeqLatch<T> {}

impl<T> SeqLatch<T> {
    /// Create new uninitialized latch
    pub const fn new() -> Self {
        Self {
            seq: AtomicU64::new(0),
            data: UnsafeCell::new(None),
        }
    }

    /// Initialize the latch (can only be called once)
    ///
    /// Returns false if already initialized.
    pub fn init(&self, value: T) -> bool {
        // Try to transition 0 -> 1
        if self.seq.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Relaxed).is_err() {
            return false;
        }

        // Write the value
        unsafe {
            *self.data.get() = Some(value);
        }

        // Transition 1 -> 2 (complete)
        core::sync::atomic::fence(Ordering::Release);
        self.seq.store(2, Ordering::Release);

        true
    }

    /// Try to read without waiting
    pub fn try_get(&self) -> Option<&T> {
        let seq = self.seq.load(Ordering::Acquire);

        if seq == 2 {
            unsafe {
                (*self.data.get()).as_ref()
            }
        } else {
            None
        }
    }

    /// Wait for initialization and get reference
    pub fn get(&self) -> &T {
        loop {
            let seq = self.seq.load(Ordering::Acquire);

            if seq == 2 {
                return unsafe { (*self.data.get()).as_ref().unwrap() };
            }

            core::hint::spin_loop();
        }
    }

    /// Check if initialized
    pub fn is_initialized(&self) -> bool {
        self.seq.load(Ordering::Acquire) == 2
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seqlock_basic() {
        let lock = SeqLock::new(42u32);
        assert_eq!(lock.read(), 42);

        lock.write(100);
        assert_eq!(lock.read(), 100);
    }

    #[test]
    fn test_seqcount_basic() {
        let seq = SeqCount::new();

        let start = seq.read_begin();
        assert!(!seq.read_retry(start));

        seq.write_begin();
        seq.write_end();

        assert!(seq.read_retry(start));
    }
}
