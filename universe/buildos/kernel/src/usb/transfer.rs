//! USB Transfer Management
//!
//! USB Request Blocks (URBs) and transfer handling:
//! - Control transfers
//! - Bulk transfers
//! - Interrupt transfers
//! - Isochronous transfers
//! - Transfer completion callbacks

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;
use super::{UsbError, SetupPacket};

/// Transfer direction
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferDirection {
    /// Host to device
    Out,
    /// Device to host
    In,
}

/// Transfer type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferType {
    /// Control transfer
    Control,
    /// Bulk transfer
    Bulk,
    /// Interrupt transfer
    Interrupt,
    /// Isochronous transfer
    Isochronous,
}

/// URB status
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum UrbStatus {
    #[default]
    /// Not yet submitted
    Idle,
    /// Submitted, awaiting completion
    Pending,
    /// Completed successfully
    Completed,
    /// Transfer timed out
    Timeout,
    /// Transfer stalled
    Stalled,
    /// CRC error
    CrcError,
    /// Babble (data overrun)
    Babble,
    /// Bit stuffing error
    BitStuffError,
    /// Data toggle mismatch
    DataToggleError,
    /// Buffer overrun
    BufferOverrun,
    /// Buffer underrun
    BufferUnderrun,
    /// Device not responding
    DeviceNotResponding,
    /// Short packet
    ShortPacket,
    /// Cancelled
    Cancelled,
    /// Host controller error
    HostError,
    /// Unknown error
    Unknown,
}

impl From<UrbStatus> for UsbError {
    fn from(status: UrbStatus) -> Self {
        match status {
            UrbStatus::Timeout => UsbError::Timeout,
            UrbStatus::Stalled => UsbError::Stall,
            UrbStatus::CrcError => UsbError::CrcError,
            UrbStatus::Babble => UsbError::Babble,
            UrbStatus::BitStuffError => UsbError::BitStuffing,
            UrbStatus::DataToggleError => UsbError::DataToggle,
            UrbStatus::BufferOverrun => UsbError::BufferOverrun,
            UrbStatus::BufferUnderrun => UsbError::BufferUnderrun,
            UrbStatus::DeviceNotResponding => UsbError::DeviceRemoved,
            UrbStatus::ShortPacket => UsbError::ShortPacket,
            UrbStatus::Cancelled => UsbError::InvalidState,
            UrbStatus::HostError => UsbError::HostControllerError,
            _ => UsbError::TransferFailed,
        }
    }
}

/// USB Request Block (URB)
pub struct Urb {
    /// URB ID
    pub id: u64,
    /// Device address
    pub device_address: u8,
    /// Endpoint number
    pub endpoint: u8,
    /// Transfer type
    pub transfer_type: TransferType,
    /// Transfer direction
    pub direction: TransferDirection,
    /// Setup packet (for control transfers)
    pub setup: Option<SetupPacket>,
    /// Data buffer
    pub buffer: Vec<u8>,
    /// Actual length transferred
    pub actual_length: usize,
    /// Status
    pub status: UrbStatus,
    /// URB flags
    pub flags: UrbFlags,
    /// Timeout in milliseconds (0 = no timeout)
    pub timeout_ms: u32,
    /// Interval (for interrupt/isochronous)
    pub interval: u32,
    /// Start frame (for isochronous)
    pub start_frame: u32,
    /// Number of packets (for isochronous)
    pub number_of_packets: u32,
    /// Isochronous packet descriptors
    pub iso_packets: Vec<IsoPacketDescriptor>,
    /// Completion callback
    pub complete: Option<Box<dyn FnOnce(&mut Urb) + Send>>,
    /// User context
    pub context: u64,
    /// Submit timestamp
    pub submit_time: u64,
    /// Completion timestamp
    pub complete_time: u64,
}

/// URB flags
#[derive(Clone, Copy, Debug, Default)]
pub struct UrbFlags {
    /// Short packet is not an error
    pub short_not_ok: bool,
    /// Allow partial zero-length packet
    pub zero_packet: bool,
    /// Don't complete URB until buffer is full
    pub no_interrupt: bool,
    /// Free buffer on completion
    pub free_buffer: bool,
    /// URB is unlinked
    pub unlinked: bool,
}

/// Isochronous packet descriptor
#[derive(Clone, Copy, Debug, Default)]
pub struct IsoPacketDescriptor {
    /// Offset in buffer
    pub offset: u32,
    /// Length
    pub length: u32,
    /// Actual length transferred
    pub actual_length: u32,
    /// Status
    pub status: UrbStatus,
}

impl Urb {
    /// Create new URB
    pub fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);

        Self {
            id: NEXT_ID.fetch_add(1, Ordering::SeqCst),
            device_address: 0,
            endpoint: 0,
            transfer_type: TransferType::Control,
            direction: TransferDirection::Out,
            setup: None,
            buffer: Vec::new(),
            actual_length: 0,
            status: UrbStatus::Idle,
            flags: UrbFlags::default(),
            timeout_ms: 5000,
            interval: 0,
            start_frame: 0,
            number_of_packets: 0,
            iso_packets: Vec::new(),
            complete: None,
            context: 0,
            submit_time: 0,
            complete_time: 0,
        }
    }

    /// Create control URB
    pub fn control(device: u8, setup: SetupPacket, data: Option<Vec<u8>>) -> Self {
        let direction = if setup.request_type & 0x80 != 0 {
            TransferDirection::In
        } else {
            TransferDirection::Out
        };

        Self {
            device_address: device,
            endpoint: 0,
            transfer_type: TransferType::Control,
            direction,
            setup: Some(setup),
            buffer: data.unwrap_or_default(),
            ..Self::new()
        }
    }

    /// Create bulk URB
    pub fn bulk(device: u8, endpoint: u8, direction: TransferDirection, data: Vec<u8>) -> Self {
        Self {
            device_address: device,
            endpoint,
            transfer_type: TransferType::Bulk,
            direction,
            buffer: data,
            ..Self::new()
        }
    }

    /// Create interrupt URB
    pub fn interrupt(device: u8, endpoint: u8, direction: TransferDirection, data: Vec<u8>, interval: u32) -> Self {
        Self {
            device_address: device,
            endpoint,
            transfer_type: TransferType::Interrupt,
            direction,
            buffer: data,
            interval,
            ..Self::new()
        }
    }

    /// Create isochronous URB
    pub fn isochronous(
        device: u8,
        endpoint: u8,
        direction: TransferDirection,
        data: Vec<u8>,
        packets: Vec<IsoPacketDescriptor>,
    ) -> Self {
        let number_of_packets = packets.len() as u32;

        Self {
            device_address: device,
            endpoint,
            transfer_type: TransferType::Isochronous,
            direction,
            buffer: data,
            number_of_packets,
            iso_packets: packets,
            ..Self::new()
        }
    }

    /// Set completion callback
    pub fn set_complete<F>(&mut self, callback: F)
    where
        F: FnOnce(&mut Urb) + Send + 'static,
    {
        self.complete = Some(Box::new(callback));
    }

    /// Set timeout
    pub fn set_timeout(&mut self, timeout_ms: u32) {
        self.timeout_ms = timeout_ms;
    }

    /// Is completed
    pub fn is_completed(&self) -> bool {
        matches!(self.status, UrbStatus::Completed | UrbStatus::ShortPacket)
    }

    /// Is pending
    pub fn is_pending(&self) -> bool {
        self.status == UrbStatus::Pending
    }

    /// Has error
    pub fn has_error(&self) -> bool {
        !matches!(self.status, UrbStatus::Idle | UrbStatus::Pending | UrbStatus::Completed | UrbStatus::ShortPacket)
    }

    /// Get error
    pub fn error(&self) -> Option<UsbError> {
        if self.has_error() {
            Some(self.status.into())
        } else {
            None
        }
    }

    /// Complete the URB
    pub fn complete_urb(&mut self, status: UrbStatus, actual_length: usize) {
        self.status = status;
        self.actual_length = actual_length;
        self.complete_time = crate::time::current_time_ns();

        if let Some(callback) = self.complete.take() {
            callback(self);
        }
    }
}

impl Default for Urb {
    fn default() -> Self {
        Self::new()
    }
}

/// URB queue for managing pending transfers
pub struct UrbQueue {
    /// Queue name
    name: &'static str,
    /// Pending URBs
    pending: Mutex<Vec<Box<Urb>>>,
    /// Total URBs submitted
    total_submitted: AtomicU64,
    /// Total URBs completed
    total_completed: AtomicU64,
    /// Total URBs failed
    total_failed: AtomicU64,
}

impl UrbQueue {
    /// Create new URB queue
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            pending: Mutex::new(Vec::new()),
            total_submitted: AtomicU64::new(0),
            total_completed: AtomicU64::new(0),
            total_failed: AtomicU64::new(0),
        }
    }

    /// Submit URB to queue
    pub fn submit(&self, urb: Box<Urb>) -> u64 {
        let id = urb.id;
        self.pending.lock().push(urb);
        self.total_submitted.fetch_add(1, Ordering::Relaxed);
        id
    }

    /// Cancel URB
    pub fn cancel(&self, urb_id: u64) -> bool {
        let mut pending = self.pending.lock();
        if let Some(pos) = pending.iter().position(|u| u.id == urb_id) {
            let mut urb = pending.remove(pos);
            urb.complete_urb(UrbStatus::Cancelled, 0);
            true
        } else {
            false
        }
    }

    /// Get next pending URB
    pub fn next(&self) -> Option<Box<Urb>> {
        self.pending.lock().pop()
    }

    /// Complete URB by ID
    pub fn complete(&self, urb_id: u64, status: UrbStatus, actual_length: usize) -> bool {
        let mut pending = self.pending.lock();
        if let Some(pos) = pending.iter().position(|u| u.id == urb_id) {
            let mut urb = pending.remove(pos);
            urb.complete_urb(status, actual_length);

            if urb.has_error() {
                self.total_failed.fetch_add(1, Ordering::Relaxed);
            } else {
                self.total_completed.fetch_add(1, Ordering::Relaxed);
            }
            true
        } else {
            false
        }
    }

    /// Get queue depth
    pub fn depth(&self) -> usize {
        self.pending.lock().len()
    }

    /// Check for timeouts
    pub fn check_timeouts(&self, current_time: u64) {
        let mut pending = self.pending.lock();
        let mut timed_out = Vec::new();

        for (i, urb) in pending.iter().enumerate() {
            if urb.timeout_ms > 0 {
                let elapsed_ms = (current_time - urb.submit_time) / 1_000_000;
                if elapsed_ms >= urb.timeout_ms as u64 {
                    timed_out.push(i);
                }
            }
        }

        // Complete timed out URBs (in reverse order to maintain indices)
        for i in timed_out.into_iter().rev() {
            let mut urb = pending.remove(i);
            urb.complete_urb(UrbStatus::Timeout, 0);
            self.total_failed.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Transfer ring for xHCI-style ring buffer
pub struct TransferRing {
    /// Ring buffer base (physical)
    base: u64,
    /// Ring size
    size: usize,
    /// Enqueue pointer
    enqueue: AtomicU32,
    /// Dequeue pointer
    dequeue: AtomicU32,
    /// Cycle state
    cycle: AtomicBool,
}

impl TransferRing {
    /// Create new transfer ring
    pub fn new(base: u64, size: usize) -> Self {
        Self {
            base,
            size,
            enqueue: AtomicU32::new(0),
            dequeue: AtomicU32::new(0),
            cycle: AtomicBool::new(true),
        }
    }

    /// Enqueue TRB
    pub fn enqueue(&self, trb: &[u8; 16]) -> Option<u64> {
        let enqueue = self.enqueue.load(Ordering::Acquire) as usize;
        let next = (enqueue + 1) % self.size;

        // Check if ring is full
        if next == self.dequeue.load(Ordering::Acquire) as usize {
            return None;
        }

        let addr = self.base + (enqueue * 16) as u64;

        // Write TRB with cycle bit
        unsafe {
            let ptr = addr as *mut u8;
            core::ptr::copy_nonoverlapping(trb.as_ptr(), ptr, 16);

            // Set cycle bit
            let control_ptr = (addr + 12) as *mut u32;
            let mut control = core::ptr::read_volatile(control_ptr);
            if self.cycle.load(Ordering::Acquire) {
                control |= 1;
            } else {
                control &= !1;
            }
            core::ptr::write_volatile(control_ptr, control);
        }

        // Update enqueue pointer
        self.enqueue.store(next as u32, Ordering::Release);

        // Handle link TRB at end of ring
        if next == self.size - 1 {
            self.cycle.fetch_xor(true, Ordering::AcqRel);
        }

        Some(addr)
    }

    /// Dequeue TRB
    pub fn dequeue(&self) -> Option<[u8; 16]> {
        let dequeue = self.dequeue.load(Ordering::Acquire) as usize;

        // Check if ring is empty
        if dequeue == self.enqueue.load(Ordering::Acquire) as usize {
            return None;
        }

        let addr = self.base + (dequeue * 16) as u64;
        let mut trb = [0u8; 16];

        unsafe {
            let ptr = addr as *const u8;
            core::ptr::copy_nonoverlapping(ptr, trb.as_mut_ptr(), 16);
        }

        // Update dequeue pointer
        let next = (dequeue + 1) % self.size;
        self.dequeue.store(next as u32, Ordering::Release);

        Some(trb)
    }

    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.enqueue.load(Ordering::Acquire) == self.dequeue.load(Ordering::Acquire)
    }

    /// Is full
    pub fn is_full(&self) -> bool {
        let enqueue = self.enqueue.load(Ordering::Acquire) as usize;
        let next = (enqueue + 1) % self.size;
        next == self.dequeue.load(Ordering::Acquire) as usize
    }
}

/// Async completion handler
pub trait AsyncCompletion: Send + Sync {
    /// Handle URB completion
    fn complete(&self, urb: &mut Urb);

    /// Handle error
    fn error(&self, urb: &mut Urb, error: UsbError);
}

/// Synchronous transfer helper
pub struct SyncTransfer {
    /// Completion flag
    completed: AtomicBool,
    /// Result status
    status: Mutex<Option<UrbStatus>>,
    /// Actual length
    actual_length: AtomicU32,
    /// Data buffer
    data: Mutex<Vec<u8>>,
}

impl SyncTransfer {
    /// Create new sync transfer
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            completed: AtomicBool::new(false),
            status: Mutex::new(None),
            actual_length: AtomicU32::new(0),
            data: Mutex::new(Vec::new()),
        })
    }

    /// Wait for completion
    pub fn wait(&self, timeout_ms: u32) -> Result<usize, UsbError> {
        let start = crate::time::current_time_ns();
        let timeout_ns = timeout_ms as u64 * 1_000_000;

        while !self.completed.load(Ordering::Acquire) {
            if timeout_ms > 0 {
                let elapsed = crate::time::current_time_ns() - start;
                if elapsed >= timeout_ns {
                    return Err(UsbError::Timeout);
                }
            }
            core::hint::spin_loop();
        }

        let status = self.status.lock().take().unwrap_or(UrbStatus::Unknown);
        let actual_length = self.actual_length.load(Ordering::Acquire) as usize;

        if status == UrbStatus::Completed || status == UrbStatus::ShortPacket {
            Ok(actual_length)
        } else {
            Err(status.into())
        }
    }

    /// Get data
    pub fn data(&self) -> Vec<u8> {
        self.data.lock().clone()
    }

    /// Mark as completed
    pub fn complete(&self, status: UrbStatus, actual_length: usize, data: Vec<u8>) {
        *self.status.lock() = Some(status);
        self.actual_length.store(actual_length as u32, Ordering::Release);
        *self.data.lock() = data;
        self.completed.store(true, Ordering::Release);
    }
}
