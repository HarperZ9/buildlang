// ===============================================================================
// BUILDOS KERNEL - RING BUFFER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Lock-free Ring Buffer
//!
//! A circular buffer for storing log records and trace events.
//! Supports multiple producers with atomic operations.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// Ring buffer for storing records
pub struct RingBuffer<T: Clone> {
    /// Storage
    buffer: Vec<Option<T>>,
    /// Capacity
    capacity: usize,
    /// Write position
    write_pos: AtomicUsize,
    /// First valid sequence number
    first_seq: AtomicU64,
    /// Number of items
    count: AtomicUsize,
}

impl<T: Clone> RingBuffer<T> {
    /// Create new ring buffer with given capacity
    pub fn new(capacity: usize) -> Self {
        let mut buffer = Vec::with_capacity(capacity);
        buffer.resize_with(capacity, || None);

        Self {
            buffer,
            capacity,
            write_pos: AtomicUsize::new(0),
            first_seq: AtomicU64::new(0),
            count: AtomicUsize::new(0),
        }
    }

    /// Push an item to the buffer
    pub fn push(&mut self, item: T) {
        let pos = self.write_pos.fetch_add(1, Ordering::AcqRel) % self.capacity;
        self.buffer[pos] = Some(item);

        let current_count = self.count.fetch_add(1, Ordering::AcqRel);
        if current_count >= self.capacity {
            // Overwriting old entry
            self.first_seq.fetch_add(1, Ordering::AcqRel);
            self.count.store(self.capacity, Ordering::Release);
        }
    }

    /// Read items starting from sequence number
    pub fn read_from(&self, start_seq: u64, max_count: usize) -> Vec<T> {
        let first = self.first_seq.load(Ordering::Acquire);
        let count = self.count.load(Ordering::Acquire);

        if count == 0 {
            return Vec::new();
        }

        let actual_start = if start_seq < first {
            first
        } else {
            start_seq
        };

        let available = (first + count as u64).saturating_sub(actual_start) as usize;
        let to_read = available.min(max_count);

        let mut result = Vec::with_capacity(to_read);
        let start_offset = (actual_start - first) as usize;

        for i in 0..to_read {
            let pos = (start_offset + i) % self.capacity;
            if let Some(ref item) = self.buffer[pos] {
                result.push(item.clone());
            }
        }

        result
    }

    /// Get number of items
    pub fn len(&self) -> usize {
        self.count.load(Ordering::Acquire).min(self.capacity)
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        for slot in &mut self.buffer {
            *slot = None;
        }
        self.write_pos.store(0, Ordering::Release);
        self.first_seq.store(0, Ordering::Release);
        self.count.store(0, Ordering::Release);
    }

    /// Get first sequence number
    pub fn first_sequence(&self) -> u64 {
        self.first_seq.load(Ordering::Acquire)
    }

    /// Get next sequence number
    pub fn next_sequence(&self) -> u64 {
        let first = self.first_seq.load(Ordering::Acquire);
        let count = self.count.load(Ordering::Acquire);
        first + count as u64
    }
}

/// Lock-free single-producer ring buffer (for per-CPU use)
pub struct PerCpuRingBuffer<T: Copy + Default> {
    buffer: Vec<T>,
    capacity: usize,
    head: AtomicUsize,
    tail: AtomicUsize,
}

impl<T: Copy + Default> PerCpuRingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        let mut buffer = Vec::with_capacity(capacity);
        buffer.resize(capacity, T::default());

        Self {
            buffer,
            capacity,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// Push (single producer)
    pub fn push(&mut self, item: T) -> bool {
        let head = self.head.load(Ordering::Relaxed);
        let next_head = (head + 1) % self.capacity;
        let tail = self.tail.load(Ordering::Acquire);

        if next_head == tail {
            // Buffer full
            return false;
        }

        self.buffer[head] = item;
        self.head.store(next_head, Ordering::Release);
        true
    }

    /// Pop (single consumer)
    pub fn pop(&mut self) -> Option<T> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);

        if tail == head {
            // Buffer empty
            return None;
        }

        let item = self.buffer[tail];
        self.tail.store((tail + 1) % self.capacity, Ordering::Release);
        Some(item)
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Acquire) == self.tail.load(Ordering::Acquire)
    }

    /// Check if full
    pub fn is_full(&self) -> bool {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        (head + 1) % self.capacity == tail
    }

    /// Get count
    pub fn len(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        if head >= tail {
            head - tail
        } else {
            self.capacity - tail + head
        }
    }

    /// Clear buffer
    pub fn clear(&mut self) {
        self.head.store(0, Ordering::Release);
        self.tail.store(0, Ordering::Release);
    }
}

/// Multi-producer multi-consumer ring buffer
pub struct MpmcRingBuffer<T> {
    buffer: Vec<AtomicCell<T>>,
    capacity: usize,
    head: AtomicUsize,
    tail: AtomicUsize,
}

/// Atomic cell for MPMC buffer
struct AtomicCell<T> {
    sequence: AtomicUsize,
    data: core::cell::UnsafeCell<Option<T>>,
}

unsafe impl<T: Send> Send for AtomicCell<T> {}
unsafe impl<T: Send> Sync for AtomicCell<T> {}

impl<T> AtomicCell<T> {
    fn new(seq: usize) -> Self {
        Self {
            sequence: AtomicUsize::new(seq),
            data: core::cell::UnsafeCell::new(None),
        }
    }
}

impl<T: Clone> MpmcRingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        let mut buffer = Vec::with_capacity(capacity);
        for i in 0..capacity {
            buffer.push(AtomicCell::new(i));
        }

        Self {
            buffer,
            capacity,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// Push an item (may fail if full)
    pub fn push(&self, item: T) -> Result<(), T> {
        loop {
            let head = self.head.load(Ordering::Relaxed);
            let idx = head % self.capacity;
            let cell = &self.buffer[idx];
            let seq = cell.sequence.load(Ordering::Acquire);

            if seq == head {
                // Slot is available
                if self.head.compare_exchange_weak(
                    head,
                    head + 1,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                ).is_ok() {
                    unsafe {
                        *cell.data.get() = Some(item);
                    }
                    cell.sequence.store(head + 1, Ordering::Release);
                    return Ok(());
                }
            } else if seq < head {
                // Buffer is full
                return Err(item);
            }
            // Retry
            core::hint::spin_loop();
        }
    }

    /// Pop an item (may fail if empty)
    pub fn pop(&self) -> Option<T> {
        loop {
            let tail = self.tail.load(Ordering::Relaxed);
            let idx = tail % self.capacity;
            let cell = &self.buffer[idx];
            let seq = cell.sequence.load(Ordering::Acquire);

            if seq == tail + 1 {
                // Data is available
                if self.tail.compare_exchange_weak(
                    tail,
                    tail + 1,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                ).is_ok() {
                    let item = unsafe { (*cell.data.get()).take() };
                    cell.sequence.store(tail + self.capacity, Ordering::Release);
                    return item;
                }
            } else if seq == tail {
                // Buffer is empty
                return None;
            }
            // Retry
            core::hint::spin_loop();
        }
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Acquire) == self.tail.load(Ordering::Acquire)
    }
}

/// Overwrite ring buffer (never fails push, overwrites oldest)
pub struct OverwriteRingBuffer<T: Clone> {
    inner: RingBuffer<T>,
}

impl<T: Clone> OverwriteRingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: RingBuffer::new(capacity),
        }
    }

    pub fn push(&mut self, item: T) {
        self.inner.push(item);
    }

    pub fn read_all(&self) -> Vec<T> {
        self.inner.read_from(self.inner.first_sequence(), self.inner.len())
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer() {
        let mut rb = RingBuffer::new(4);

        rb.push(1);
        rb.push(2);
        rb.push(3);

        assert_eq!(rb.len(), 3);

        let items = rb.read_from(0, 10);
        assert_eq!(items, vec![1, 2, 3]);

        // Overflow
        rb.push(4);
        rb.push(5);

        assert_eq!(rb.len(), 4);
        let items = rb.read_from(0, 10);
        assert_eq!(items, vec![2, 3, 4, 5]);
    }

    #[test]
    fn test_per_cpu_ring_buffer() {
        let mut rb: PerCpuRingBuffer<i32> = PerCpuRingBuffer::new(4);

        assert!(rb.push(1));
        assert!(rb.push(2));
        assert!(rb.push(3));
        assert!(!rb.push(4)); // Full

        assert_eq!(rb.pop(), Some(1));
        assert_eq!(rb.pop(), Some(2));

        assert!(rb.push(4));
        assert!(rb.push(5));

        assert_eq!(rb.pop(), Some(3));
        assert_eq!(rb.pop(), Some(4));
        assert_eq!(rb.pop(), Some(5));
        assert_eq!(rb.pop(), None);
    }
}
