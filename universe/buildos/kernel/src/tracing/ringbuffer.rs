// ===============================================================================
// BUILDOS KERNEL - TRACE RING BUFFER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Per-CPU ring buffers for trace events.
//!
//! Lock-free ring buffers allow recording trace events with minimal
//! overhead and no interference between CPUs.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, AtomicU64, AtomicBool, Ordering};
use spin::Mutex;

use crate::sched::MAX_CPUS;
use super::TraceEvent;

// =============================================================================
// CONSTANTS
// =============================================================================

/// Number of events per ring buffer page
const EVENTS_PER_PAGE: usize = 64;

/// Maximum pages per buffer
const MAX_PAGES: usize = 1024;

// =============================================================================
// STATE
// =============================================================================

/// Per-CPU ring buffers
static mut CPU_BUFFERS: [Option<RingBuffer>; MAX_CPUS] = {
    const NONE: Option<RingBuffer> = None;
    [NONE; MAX_CPUS]
};

/// Ring buffer configuration
static BUFFER_CONFIG: BufferConfig = BufferConfig::new();

/// Buffer configuration
pub struct BufferConfig {
    /// Size per CPU (bytes)
    size_per_cpu: AtomicUsize,
    /// Overwrite old entries when full
    overwrite: AtomicBool,
    /// Number of CPUs
    num_cpus: AtomicUsize,
}

impl BufferConfig {
    const fn new() -> Self {
        Self {
            size_per_cpu: AtomicUsize::new(super::DEFAULT_BUFFER_SIZE),
            overwrite: AtomicBool::new(true),
            num_cpus: AtomicUsize::new(1),
        }
    }
}

/// Per-CPU ring buffer
pub struct RingBuffer {
    /// CPU ID
    cpu: usize,

    /// Buffer pages
    pages: Vec<BufferPage>,

    /// Head page index (write)
    head: AtomicUsize,

    /// Tail page index (read)
    tail: AtomicUsize,

    /// Events written
    entries: AtomicU64,

    /// Events dropped (overwritten)
    overwritten: AtomicU64,

    /// Bytes written
    bytes_written: AtomicU64,

    /// Is buffer enabled?
    enabled: AtomicBool,

    /// Snapshot pages (for snapshot mode)
    snapshot: Mutex<Vec<BufferPage>>,
}

/// A page in the ring buffer
pub struct BufferPage {
    /// Events on this page
    events: Vec<TraceEvent>,

    /// Page index in ring
    index: usize,

    /// Timestamp of first event
    first_ts: u64,

    /// Timestamp of last event
    last_ts: u64,

    /// Number of events on page
    count: usize,
}

impl BufferPage {
    fn new(index: usize) -> Self {
        Self {
            events: Vec::with_capacity(EVENTS_PER_PAGE),
            index,
            first_ts: 0,
            last_ts: 0,
            count: 0,
        }
    }

    fn clear(&mut self) {
        self.events.clear();
        self.first_ts = 0;
        self.last_ts = 0;
        self.count = 0;
    }

    fn push(&mut self, event: TraceEvent) -> bool {
        if self.count >= EVENTS_PER_PAGE {
            return false;
        }

        if self.count == 0 {
            self.first_ts = event.timestamp;
        }
        self.last_ts = event.timestamp;

        self.events.push(event);
        self.count += 1;
        true
    }

    fn is_full(&self) -> bool {
        self.count >= EVENTS_PER_PAGE
    }
}

impl RingBuffer {
    fn new(cpu: usize, num_pages: usize) -> Self {
        let mut pages = Vec::with_capacity(num_pages);
        for i in 0..num_pages {
            pages.push(BufferPage::new(i));
        }

        Self {
            cpu,
            pages,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            entries: AtomicU64::new(0),
            overwritten: AtomicU64::new(0),
            bytes_written: AtomicU64::new(0),
            enabled: AtomicBool::new(true),
            snapshot: Mutex::new(Vec::new()),
        }
    }

    /// Write an event to the buffer
    fn write(&mut self, event: TraceEvent) -> Result<(), ()> {
        if !self.enabled.load(Ordering::Relaxed) {
            return Err(());
        }

        let head = self.head.load(Ordering::Acquire);

        // Try to write to current page
        if !self.pages[head].is_full() {
            self.pages[head].push(event);
            self.entries.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }

        // Need to advance to next page
        let num_pages = self.pages.len();
        let next_head = (head + 1) % num_pages;

        let tail = self.tail.load(Ordering::Acquire);
        if next_head == tail {
            // Buffer full
            if BUFFER_CONFIG.overwrite.load(Ordering::Relaxed) {
                // Overwrite oldest page
                let overwritten = self.pages[tail].count as u64;
                self.overwritten.fetch_add(overwritten, Ordering::Relaxed);
                self.pages[tail].clear();
                self.tail.store((tail + 1) % num_pages, Ordering::Release);
            } else {
                return Err(()); // Drop event
            }
        }

        // Advance head and write
        self.head.store(next_head, Ordering::Release);
        self.pages[next_head].push(event);
        self.entries.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Read events from buffer
    fn read(&mut self, max: usize) -> Vec<TraceEvent> {
        let mut events = Vec::new();
        let mut count = 0;

        let head = self.head.load(Ordering::Acquire);
        let mut tail = self.tail.load(Ordering::Acquire);

        while tail != head && count < max {
            // Drain events from this page
            for event in self.pages[tail].events.drain(..) {
                events.push(event);
                count += 1;
                if count >= max {
                    break;
                }
            }
            self.pages[tail].count = 0;

            if count >= max {
                break;
            }

            // Advance tail
            tail = (tail + 1) % self.pages.len();
            self.tail.store(tail, Ordering::Release);
        }

        events
    }

    /// Reset buffer
    fn reset(&mut self) {
        for page in &mut self.pages {
            page.clear();
        }
        self.head.store(0, Ordering::Release);
        self.tail.store(0, Ordering::Release);
        self.entries.store(0, Ordering::Release);
        self.overwritten.store(0, Ordering::Release);
    }

    /// Take snapshot
    fn snapshot(&mut self) {
        let mut snap = self.snapshot.lock();
        snap.clear();

        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);

        let mut idx = tail;
        while idx != head {
            let mut page = BufferPage::new(idx);
            for event in &self.pages[idx].events {
                page.push(event.clone());
            }
            snap.push(page);
            idx = (idx + 1) % self.pages.len();
        }
    }

    /// Get used bytes
    fn used(&self) -> usize {
        self.entries.load(Ordering::Relaxed) as usize * core::mem::size_of::<TraceEvent>()
    }

    /// Get total size
    fn size(&self) -> usize {
        self.pages.len() * EVENTS_PER_PAGE * core::mem::size_of::<TraceEvent>()
    }
}

// =============================================================================
// INTERFACE
// =============================================================================

/// Initialize ring buffers
pub fn init(num_cpus: usize, size_per_cpu: usize) {
    BUFFER_CONFIG.num_cpus.store(num_cpus, Ordering::Release);
    BUFFER_CONFIG.size_per_cpu.store(size_per_cpu, Ordering::Release);

    let events_per_cpu = size_per_cpu / core::mem::size_of::<TraceEvent>();
    let pages_per_cpu = (events_per_cpu / EVENTS_PER_PAGE).max(1).min(MAX_PAGES);

    for cpu in 0..num_cpus {
        unsafe {
            CPU_BUFFERS[cpu] = Some(RingBuffer::new(cpu, pages_per_cpu));
        }
    }
}

/// Write event to buffer
pub fn write(cpu: usize, event: &TraceEvent) -> Result<(), ()> {
    if cpu >= MAX_CPUS {
        return Err(());
    }

    unsafe {
        if let Some(ref mut buffer) = CPU_BUFFERS[cpu] {
            return buffer.write(event.clone());
        }
    }

    Err(())
}

/// Read events from CPU buffer
pub fn read(cpu: usize, max: usize) -> Vec<TraceEvent> {
    if cpu >= MAX_CPUS {
        return Vec::new();
    }

    unsafe {
        if let Some(ref mut buffer) = CPU_BUFFERS[cpu] {
            return buffer.read(max);
        }
    }

    Vec::new()
}

/// Read all events from all CPUs
pub fn read_all(max: usize) -> Vec<TraceEvent> {
    let mut all_events = Vec::new();
    let num_cpus = BUFFER_CONFIG.num_cpus.load(Ordering::Relaxed);

    let per_cpu = max / num_cpus.max(1);

    for cpu in 0..num_cpus {
        let events = read(cpu, per_cpu);
        all_events.extend(events);
    }

    // Sort by timestamp
    all_events.sort_by_key(|e| e.timestamp);

    all_events
}

/// Reset buffer for CPU
pub fn reset(cpu: usize) {
    if cpu >= MAX_CPUS {
        return;
    }

    unsafe {
        if let Some(ref mut buffer) = CPU_BUFFERS[cpu] {
            buffer.reset();
        }
    }
}

/// Reset all buffers
pub fn reset_all() {
    let num_cpus = BUFFER_CONFIG.num_cpus.load(Ordering::Relaxed);
    for cpu in 0..num_cpus {
        reset(cpu);
    }
}

/// Take snapshot of all buffers
pub fn snapshot() {
    let num_cpus = BUFFER_CONFIG.num_cpus.load(Ordering::Relaxed);
    for cpu in 0..num_cpus {
        unsafe {
            if let Some(ref mut buffer) = CPU_BUFFERS[cpu] {
                buffer.snapshot();
            }
        }
    }
}

/// Get total used bytes
pub fn total_used() -> usize {
    let mut total = 0;
    let num_cpus = BUFFER_CONFIG.num_cpus.load(Ordering::Relaxed);

    for cpu in 0..num_cpus {
        unsafe {
            if let Some(ref buffer) = CPU_BUFFERS[cpu] {
                total += buffer.used();
            }
        }
    }

    total
}

/// Get total buffer size
pub fn total_size() -> usize {
    let mut total = 0;
    let num_cpus = BUFFER_CONFIG.num_cpus.load(Ordering::Relaxed);

    for cpu in 0..num_cpus {
        unsafe {
            if let Some(ref buffer) = CPU_BUFFERS[cpu] {
                total += buffer.size();
            }
        }
    }

    total
}

/// Enable/disable buffer
pub fn set_enabled(cpu: usize, enabled: bool) {
    if cpu >= MAX_CPUS {
        return;
    }

    unsafe {
        if let Some(ref buffer) = CPU_BUFFERS[cpu] {
            buffer.enabled.store(enabled, Ordering::Release);
        }
    }
}

/// Set overwrite mode
pub fn set_overwrite(overwrite: bool) {
    BUFFER_CONFIG.overwrite.store(overwrite, Ordering::Release);
}

/// Get buffer statistics for CPU
pub fn get_stats(cpu: usize) -> BufferStats {
    if cpu >= MAX_CPUS {
        return BufferStats::default();
    }

    unsafe {
        if let Some(ref buffer) = CPU_BUFFERS[cpu] {
            return BufferStats {
                cpu,
                entries: buffer.entries.load(Ordering::Relaxed),
                overwritten: buffer.overwritten.load(Ordering::Relaxed),
                bytes_used: buffer.used(),
                bytes_total: buffer.size(),
                enabled: buffer.enabled.load(Ordering::Relaxed),
            };
        }
    }

    BufferStats::default()
}

/// Buffer statistics
#[derive(Default)]
pub struct BufferStats {
    pub cpu: usize,
    pub entries: u64,
    pub overwritten: u64,
    pub bytes_used: usize,
    pub bytes_total: usize,
    pub enabled: bool,
}
