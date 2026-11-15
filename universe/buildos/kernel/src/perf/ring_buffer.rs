//! Perf Ring Buffer
//!
//! Lock-free ring buffer for perf event samples.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering, fence};
use super::sampling::Sample;

/// Ring buffer header (shared with userspace via mmap)
#[repr(C)]
pub struct PerfEventMmapPage {
    /// Version number
    pub version: u32,
    /// Compatibility version
    pub compat_version: u32,
    /// Lock for seqlock
    pub lock: u32,
    /// Index into data buffer
    pub index: u32,
    /// Offset of data buffer from header
    pub data_offset: u64,
    /// Size of data buffer
    pub data_size: u64,
    /// Head pointer (producer writes)
    pub data_head: AtomicU64,
    /// Tail pointer (consumer updates)
    pub data_tail: AtomicU64,
    /// AUX offset (for aux buffer)
    pub aux_offset: u64,
    /// AUX size
    pub aux_size: u64,
    /// AUX head
    pub aux_head: AtomicU64,
    /// AUX tail
    pub aux_tail: AtomicU64,
    /// Time-related fields
    pub time_enabled: u64,
    pub time_running: u64,
    /// Capabilities
    pub cap_user_rdpmc: u64,
    pub cap_user_time: u64,
    pub cap_user_time_zero: u64,
    /// PMU index
    pub pmc_width: u16,
    /// Time shift
    pub time_shift: u16,
    /// Time mult
    pub time_mult: u32,
    /// Time offset
    pub time_offset: u64,
    /// Time zero
    pub time_zero: u64,
    /// Size of this page
    pub size: u32,
    /// Time cycles (for RDTSC conversion)
    pub time_cycles: u64,
    /// Time mask
    pub time_mask: u64,
    /// Reserved for future use
    pub reserved: [u8; 928],
}

impl Default for PerfEventMmapPage {
    fn default() -> Self {
        Self {
            version: 0,
            compat_version: 0,
            lock: 0,
            index: 0,
            data_offset: 4096, // Page size
            data_size: 0,
            data_head: AtomicU64::new(0),
            data_tail: AtomicU64::new(0),
            aux_offset: 0,
            aux_size: 0,
            aux_head: AtomicU64::new(0),
            aux_tail: AtomicU64::new(0),
            time_enabled: 0,
            time_running: 0,
            cap_user_rdpmc: 0,
            cap_user_time: 1,
            cap_user_time_zero: 1,
            pmc_width: 48,
            time_shift: 0,
            time_mult: 0,
            time_offset: 0,
            time_zero: 0,
            size: core::mem::size_of::<PerfEventMmapPage>() as u32,
            time_cycles: 0,
            time_mask: u64::MAX,
            reserved: [0; 928],
        }
    }
}

/// Perf event header in ring buffer
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PerfEventHeader {
    /// Event type
    pub type_: u32,
    /// Miscellaneous flags
    pub misc: u16,
    /// Total size including header
    pub size: u16,
}

/// Perf event types
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerfEventRecordType {
    /// Mmap event
    Mmap = 1,
    /// Lost samples
    Lost = 2,
    /// Comm (process name change)
    Comm = 3,
    /// Exit
    Exit = 4,
    /// Throttle
    Throttle = 5,
    /// Unthrottle
    Unthrottle = 6,
    /// Fork
    Fork = 7,
    /// Read
    Read = 8,
    /// Sample
    Sample = 9,
    /// Mmap2
    Mmap2 = 10,
    /// Aux
    Aux = 11,
    /// Itrace start
    ItraceStart = 12,
    /// Lost samples
    LostSamples = 13,
    /// Switch
    Switch = 14,
    /// Switch CPU-wide
    SwitchCpuWide = 15,
    /// Namespaces
    Namespaces = 16,
    /// Ksymbol
    Ksymbol = 17,
    /// BPF event
    BpfEvent = 18,
    /// Cgroup
    Cgroup = 19,
    /// Text poke
    TextPoke = 20,
    /// Aux output hw id
    AuxOutputHwId = 21,
}

/// Perf ring buffer
pub struct PerfRingBuffer {
    /// Header page
    header: Box<PerfEventMmapPage>,
    /// Data buffer
    data: Vec<u8>,
    /// Buffer size (power of 2)
    size: usize,
    /// Size mask for wrap-around
    mask: usize,
    /// Lost samples count
    lost: AtomicU64,
    /// Overwrite mode
    overwrite: bool,
}

impl PerfRingBuffer {
    /// Create new ring buffer with given number of pages
    pub fn new(pages: usize) -> Self {
        let size = pages * 4096;
        let mask = size - 1;

        let mut header = Box::new(PerfEventMmapPage::default());
        header.data_size = size as u64;

        Self {
            header,
            data: vec![0u8; size],
            size,
            mask,
            lost: AtomicU64::new(0),
            overwrite: false,
        }
    }

    /// Create with overwrite mode
    pub fn new_overwrite(pages: usize) -> Self {
        let mut rb = Self::new(pages);
        rb.overwrite = true;
        rb
    }

    /// Get head position
    pub fn head(&self) -> u64 {
        self.header.data_head.load(Ordering::Acquire)
    }

    /// Get tail position
    pub fn tail(&self) -> u64 {
        self.header.data_tail.load(Ordering::Acquire)
    }

    /// Get available space
    pub fn available(&self) -> usize {
        let head = self.head();
        let tail = self.tail();

        if head >= tail {
            self.size - (head - tail) as usize
        } else {
            (tail - head) as usize
        }
    }

    /// Get used space
    pub fn used(&self) -> usize {
        self.size - self.available()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.head() == self.tail()
    }

    /// Check if buffer is full
    pub fn is_full(&self) -> bool {
        self.available() < core::mem::size_of::<PerfEventHeader>()
    }

    /// Push a sample to the ring buffer
    pub fn push_sample(&self, sample: &Sample) -> Result<(), RingBufferError> {
        let sample_data = sample.encode();
        self.push_record(PerfEventRecordType::Sample, &sample_data)
    }

    /// Push a record to the ring buffer
    pub fn push_record(&self, record_type: PerfEventRecordType, data: &[u8]) -> Result<(), RingBufferError> {
        let header_size = core::mem::size_of::<PerfEventHeader>();
        let total_size = align_up(header_size + data.len(), 8);

        if total_size > self.size {
            return Err(RingBufferError::TooLarge);
        }

        // Check available space
        if !self.overwrite && self.available() < total_size {
            self.lost.fetch_add(1, Ordering::Relaxed);
            return Err(RingBufferError::Full);
        }

        // In overwrite mode, advance tail if needed
        if self.overwrite && self.available() < total_size {
            // Drop oldest records
            while self.available() < total_size {
                self.drop_oldest()?;
            }
        }

        // Get current head position
        let head = self.head();
        let offset = (head as usize) & self.mask;

        // Write header
        let header = PerfEventHeader {
            type_: record_type as u32,
            misc: 0,
            size: total_size as u16,
        };

        self.write_at(offset, &header_bytes(&header));

        // Write data
        self.write_at(offset + header_size, data);

        // Pad to alignment
        let pad_size = total_size - header_size - data.len();
        if pad_size > 0 {
            let padding = [0u8; 8];
            self.write_at(offset + header_size + data.len(), &padding[..pad_size]);
        }

        // Update head (release barrier ensures writes are visible)
        fence(Ordering::Release);
        self.header.data_head.store(head + total_size as u64, Ordering::Release);

        Ok(())
    }

    /// Write data at offset (handles wrap-around)
    fn write_at(&self, offset: usize, data: &[u8]) {
        let data_ptr = self.data.as_ptr() as *mut u8;

        for (i, &byte) in data.iter().enumerate() {
            let pos = (offset + i) & self.mask;
            unsafe {
                *data_ptr.add(pos) = byte;
            }
        }
    }

    /// Read data at offset (handles wrap-around)
    fn read_at(&self, offset: usize, len: usize) -> Vec<u8> {
        let mut result = Vec::with_capacity(len);

        for i in 0..len {
            let pos = (offset + i) & self.mask;
            result.push(self.data[pos]);
        }

        result
    }

    /// Drop the oldest record
    fn drop_oldest(&self) -> Result<(), RingBufferError> {
        let tail = self.tail();
        let offset = (tail as usize) & self.mask;

        // Read header
        let header_bytes = self.read_at(offset, core::mem::size_of::<PerfEventHeader>());
        let header = unsafe {
            *(header_bytes.as_ptr() as *const PerfEventHeader)
        };

        if header.size == 0 {
            return Err(RingBufferError::Corrupted);
        }

        // Advance tail
        self.header.data_tail.store(tail + header.size as u64, Ordering::Release);

        Ok(())
    }

    /// Pop a record from the ring buffer
    pub fn pop_record(&self) -> Option<(PerfEventRecordType, Vec<u8>)> {
        if self.is_empty() {
            return None;
        }

        let tail = self.tail();
        let offset = (tail as usize) & self.mask;

        // Read header
        let header_bytes = self.read_at(offset, core::mem::size_of::<PerfEventHeader>());
        let header = unsafe {
            *(header_bytes.as_ptr() as *const PerfEventHeader)
        };

        if header.size == 0 {
            return None;
        }

        let header_size = core::mem::size_of::<PerfEventHeader>();
        let data_len = header.size as usize - header_size;

        // Read data
        let data = self.read_at(offset + header_size, data_len);

        // Advance tail
        self.header.data_tail.store(tail + header.size as u64, Ordering::Release);

        let record_type = match header.type_ {
            1 => PerfEventRecordType::Mmap,
            2 => PerfEventRecordType::Lost,
            3 => PerfEventRecordType::Comm,
            4 => PerfEventRecordType::Exit,
            5 => PerfEventRecordType::Throttle,
            6 => PerfEventRecordType::Unthrottle,
            7 => PerfEventRecordType::Fork,
            8 => PerfEventRecordType::Read,
            9 => PerfEventRecordType::Sample,
            10 => PerfEventRecordType::Mmap2,
            _ => return None,
        };

        Some((record_type, data))
    }

    /// Get lost count
    pub fn lost_count(&self) -> u64 {
        self.lost.load(Ordering::Relaxed)
    }

    /// Reset the buffer
    pub fn reset(&self) {
        self.header.data_head.store(0, Ordering::Release);
        self.header.data_tail.store(0, Ordering::Release);
        self.lost.store(0, Ordering::Release);
    }

    /// Get header page for mmap
    pub fn header_page(&self) -> &PerfEventMmapPage {
        &self.header
    }

    /// Get data buffer for mmap
    pub fn data_buffer(&self) -> &[u8] {
        &self.data
    }

    /// Iterate over all records (non-consuming)
    pub fn iter(&self) -> RingBufferIterator<'_> {
        RingBufferIterator {
            buffer: self,
            pos: self.tail(),
            end: self.head(),
        }
    }
}

/// Ring buffer iterator
pub struct RingBufferIterator<'a> {
    buffer: &'a PerfRingBuffer,
    pos: u64,
    end: u64,
}

impl<'a> Iterator for RingBufferIterator<'a> {
    type Item = (PerfEventRecordType, Vec<u8>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.end {
            return None;
        }

        let offset = (self.pos as usize) & self.buffer.mask;

        // Read header
        let header_bytes = self.buffer.read_at(offset, core::mem::size_of::<PerfEventHeader>());
        let header = unsafe {
            *(header_bytes.as_ptr() as *const PerfEventHeader)
        };

        if header.size == 0 {
            return None;
        }

        let header_size = core::mem::size_of::<PerfEventHeader>();
        let data_len = header.size as usize - header_size;

        // Read data
        let data = self.buffer.read_at(offset + header_size, data_len);

        // Advance position
        self.pos += header.size as u64;

        let record_type = match header.type_ {
            9 => PerfEventRecordType::Sample,
            _ => return self.next(), // Skip unknown types
        };

        Some((record_type, data))
    }
}

/// Ring buffer error
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RingBufferError {
    /// Buffer is full
    Full,
    /// Record too large
    TooLarge,
    /// Buffer corrupted
    Corrupted,
}

/// Convert header to bytes
fn header_bytes(header: &PerfEventHeader) -> [u8; 8] {
    let mut bytes = [0u8; 8];
    bytes[0..4].copy_from_slice(&header.type_.to_le_bytes());
    bytes[4..6].copy_from_slice(&header.misc.to_le_bytes());
    bytes[6..8].copy_from_slice(&header.size.to_le_bytes());
    bytes
}

/// Align up to boundary
fn align_up(size: usize, align: usize) -> usize {
    (size + align - 1) & !(align - 1)
}
