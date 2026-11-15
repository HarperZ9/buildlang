//! PCM (Pulse Code Modulation) Streams
//!
//! PCM audio playback and capture:
//! - Stream state management
//! - Hardware and software parameters
//! - Buffer management
//! - Period handling
//! - Timestamp support

#![allow(dead_code)]

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::{Mutex, RwLock};
use super::{SoundError, SampleFormat, StreamDirection};
use super::sound_core::SoundDevice;

/// PCM stream state
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PcmState {
    /// Open, not configured
    #[default]
    Open,
    /// Setup complete
    Setup,
    /// Prepared, ready to start
    Prepared,
    /// Running
    Running,
    /// Xrun (underrun or overrun)
    Xrun,
    /// Draining (playback only)
    Draining,
    /// Paused
    Paused,
    /// Suspended
    Suspended,
    /// Disconnected
    Disconnected,
}

/// PCM format (runtime selected format)
#[derive(Clone, Copy, Debug)]
pub struct PcmFormat {
    /// Sample format
    pub format: SampleFormat,
    /// Sample rate (Hz)
    pub rate: u32,
    /// Number of channels
    pub channels: u32,
}

impl Default for PcmFormat {
    fn default() -> Self {
        Self {
            format: SampleFormat::S16Le,
            rate: 44100,
            channels: 2,
        }
    }
}

/// Hardware parameters
#[derive(Clone, Debug)]
pub struct PcmHwParams {
    /// Sample format
    pub format: SampleFormat,
    /// Sample rate (Hz)
    pub rate: u32,
    /// Number of channels
    pub channels: u32,
    /// Period size (frames)
    pub period_size: u32,
    /// Number of periods
    pub periods: u32,
    /// Buffer size (frames)
    pub buffer_size: u32,
    /// Access type
    pub access: PcmAccess,
    /// Rate resample
    pub rate_resample: bool,
}

impl Default for PcmHwParams {
    fn default() -> Self {
        Self {
            format: SampleFormat::S16Le,
            rate: 44100,
            channels: 2,
            period_size: 1024,
            periods: 4,
            buffer_size: 4096,
            access: PcmAccess::MmapInterleaved,
            rate_resample: true,
        }
    }
}

/// PCM access type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PcmAccess {
    /// Mmap, interleaved
    MmapInterleaved,
    /// Mmap, non-interleaved
    MmapNoninterleaved,
    /// Mmap, complex
    MmapComplex,
    /// Read/write, interleaved
    RwInterleaved,
    /// Read/write, non-interleaved
    RwNoninterleaved,
}

/// Software parameters
#[derive(Clone, Debug)]
pub struct PcmSwParams {
    /// Minimum available frames to wake up
    pub avail_min: u32,
    /// Silence threshold (frames)
    pub silence_threshold: u32,
    /// Silence size (frames)
    pub silence_size: u32,
    /// Start threshold (frames)
    pub start_threshold: u32,
    /// Stop threshold (frames)
    pub stop_threshold: u32,
    /// Boundary (for wrapping)
    pub boundary: u64,
    /// Timestamp mode
    pub tstamp_mode: TstampMode,
    /// Timestamp type
    pub tstamp_type: TstampType,
}

impl Default for PcmSwParams {
    fn default() -> Self {
        Self {
            avail_min: 1,
            silence_threshold: 0,
            silence_size: 0,
            start_threshold: 1,
            stop_threshold: 0,
            boundary: u64::MAX,
            tstamp_mode: TstampMode::None,
            tstamp_type: TstampType::Monotonic,
        }
    }
}

/// Timestamp mode
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TstampMode {
    /// No timestamps
    None,
    /// Enable timestamps
    Enable,
    /// Absolute timestamps
    Mmap,
}

/// Timestamp type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TstampType {
    /// CLOCK_REALTIME
    Realtime,
    /// CLOCK_MONOTONIC
    Monotonic,
    /// CLOCK_MONOTONIC_RAW
    MonotonicRaw,
}

/// PCM status
#[derive(Clone, Debug, Default)]
pub struct PcmStatus {
    /// Current state
    pub state: u32,
    /// Hardware pointer position (frames)
    pub hw_ptr: u64,
    /// Application pointer position (frames)
    pub appl_ptr: u64,
    /// Delay (frames)
    pub delay: i64,
    /// Available frames
    pub avail: u64,
    /// Maximum available frames
    pub avail_max: u64,
    /// Overrun count
    pub overrun: u64,
    /// Trigger timestamp
    pub trigger_tstamp: Timestamp,
    /// Audio timestamp
    pub audio_tstamp: Timestamp,
}

/// Timestamp
#[derive(Clone, Copy, Debug, Default)]
pub struct Timestamp {
    /// Seconds
    pub sec: i64,
    /// Nanoseconds
    pub nsec: i64,
}

/// PCM stream
pub struct PcmStream {
    /// Device
    device: Arc<RwLock<SoundDevice>>,
    /// Direction
    direction: StreamDirection,
    /// Current state
    state: PcmState,
    /// Hardware parameters
    hw_params: PcmHwParams,
    /// Software parameters
    sw_params: PcmSwParams,
    /// DMA buffer (physical address)
    dma_buffer: u64,
    /// DMA buffer size
    dma_buffer_size: usize,
    /// Ring buffer
    ring_buffer: Option<RingBuffer>,
    /// Hardware pointer
    hw_ptr: AtomicU64,
    /// Application pointer
    appl_ptr: AtomicU64,
    /// Is running
    running: AtomicBool,
    /// Trigger timestamp
    trigger_tstamp: Mutex<Timestamp>,
}

/// Ring buffer for PCM data
struct RingBuffer {
    /// Buffer data
    data: Vec<u8>,
    /// Buffer size
    size: usize,
    /// Write pointer
    write_ptr: AtomicU32,
    /// Read pointer
    read_ptr: AtomicU32,
}

impl RingBuffer {
    /// Create new ring buffer
    fn new(size: usize) -> Self {
        Self {
            data: alloc::vec![0u8; size],
            size,
            write_ptr: AtomicU32::new(0),
            read_ptr: AtomicU32::new(0),
        }
    }

    /// Available space for writing
    fn available_write(&self) -> usize {
        let write = self.write_ptr.load(Ordering::Acquire) as usize;
        let read = self.read_ptr.load(Ordering::Acquire) as usize;

        if write >= read {
            self.size - write + read - 1
        } else {
            read - write - 1
        }
    }

    /// Available data for reading
    fn available_read(&self) -> usize {
        let write = self.write_ptr.load(Ordering::Acquire) as usize;
        let read = self.read_ptr.load(Ordering::Acquire) as usize;

        if write >= read {
            write - read
        } else {
            self.size - read + write
        }
    }

    /// Write data to buffer
    fn write(&mut self, data: &[u8]) -> usize {
        let available = self.available_write();
        let to_write = data.len().min(available);

        let write_ptr = self.write_ptr.load(Ordering::Acquire) as usize;
        let first_chunk = (self.size - write_ptr).min(to_write);

        self.data[write_ptr..write_ptr + first_chunk].copy_from_slice(&data[..first_chunk]);

        if first_chunk < to_write {
            let second_chunk = to_write - first_chunk;
            self.data[..second_chunk].copy_from_slice(&data[first_chunk..to_write]);
        }

        let new_ptr = (write_ptr + to_write) % self.size;
        self.write_ptr.store(new_ptr as u32, Ordering::Release);

        to_write
    }

    /// Read data from buffer
    fn read(&mut self, data: &mut [u8]) -> usize {
        let available = self.available_read();
        let to_read = data.len().min(available);

        let read_ptr = self.read_ptr.load(Ordering::Acquire) as usize;
        let first_chunk = (self.size - read_ptr).min(to_read);

        data[..first_chunk].copy_from_slice(&self.data[read_ptr..read_ptr + first_chunk]);

        if first_chunk < to_read {
            let second_chunk = to_read - first_chunk;
            data[first_chunk..to_read].copy_from_slice(&self.data[..second_chunk]);
        }

        let new_ptr = (read_ptr + to_read) % self.size;
        self.read_ptr.store(new_ptr as u32, Ordering::Release);

        to_read
    }

    /// Reset buffer
    fn reset(&mut self) {
        self.write_ptr.store(0, Ordering::Release);
        self.read_ptr.store(0, Ordering::Release);
    }
}

impl PcmStream {
    /// Create new PCM stream
    pub fn new(device: Arc<RwLock<SoundDevice>>, direction: StreamDirection) -> Self {
        Self {
            device,
            direction,
            state: PcmState::Open,
            hw_params: PcmHwParams::default(),
            sw_params: PcmSwParams::default(),
            dma_buffer: 0,
            dma_buffer_size: 0,
            ring_buffer: None,
            hw_ptr: AtomicU64::new(0),
            appl_ptr: AtomicU64::new(0),
            running: AtomicBool::new(false),
            trigger_tstamp: Mutex::new(Timestamp::default()),
        }
    }

    /// Get current state
    pub fn state(&self) -> PcmState {
        self.state
    }

    /// Get direction
    pub fn direction(&self) -> StreamDirection {
        self.direction
    }

    /// Set hardware parameters
    pub fn set_hw_params(&mut self, params: &PcmHwParams) -> Result<(), SoundError> {
        if self.state != PcmState::Open {
            return Err(SoundError::InvalidState);
        }

        // Validate parameters
        self.validate_hw_params(params)?;

        self.hw_params = params.clone();

        // Allocate buffer
        let buffer_size = super::buffer_size_bytes(
            params.buffer_size,
            params.channels,
            params.format,
        );

        self.ring_buffer = Some(RingBuffer::new(buffer_size));
        self.dma_buffer_size = buffer_size;

        // Allocate DMA buffer
        // self.dma_buffer = allocate_dma_buffer(buffer_size);

        self.state = PcmState::Setup;

        Ok(())
    }

    /// Get hardware parameters
    pub fn hw_params(&self) -> &PcmHwParams {
        &self.hw_params
    }

    /// Validate hardware parameters
    fn validate_hw_params(&self, params: &PcmHwParams) -> Result<(), SoundError> {
        // Check format
        let _device = self.device.read();

        // For now, accept common formats
        match params.format {
            SampleFormat::S8 | SampleFormat::U8 |
            SampleFormat::S16Le | SampleFormat::S16Be |
            SampleFormat::S24Le | SampleFormat::S24Le32 |
            SampleFormat::S32Le | SampleFormat::FloatLe => {}
            _ => return Err(SoundError::FormatNotSupported),
        }

        // Check rate (8kHz - 192kHz)
        if params.rate < 8000 || params.rate > 192000 {
            return Err(SoundError::RateNotSupported);
        }

        // Check channels (1-8)
        if params.channels == 0 || params.channels > 8 {
            return Err(SoundError::ChannelsNotSupported);
        }

        // Check buffer size
        if params.buffer_size == 0 || params.buffer_size > 1024 * 1024 {
            return Err(SoundError::InvalidParameter);
        }

        Ok(())
    }

    /// Set software parameters
    pub fn set_sw_params(&mut self, params: &PcmSwParams) -> Result<(), SoundError> {
        if self.state == PcmState::Open {
            return Err(SoundError::InvalidState);
        }

        self.sw_params = params.clone();

        // Calculate boundary
        let mut boundary = self.hw_params.buffer_size as u64;
        while boundary * 2 <= (i64::MAX as u64) - self.hw_params.buffer_size as u64 {
            boundary *= 2;
        }
        self.sw_params.boundary = boundary;

        Ok(())
    }

    /// Get software parameters
    pub fn sw_params(&self) -> &PcmSwParams {
        &self.sw_params
    }

    /// Prepare stream
    pub fn prepare(&mut self) -> Result<(), SoundError> {
        match self.state {
            PcmState::Setup | PcmState::Prepared | PcmState::Xrun | PcmState::Suspended => {}
            _ => return Err(SoundError::InvalidState),
        }

        // Reset pointers
        self.hw_ptr.store(0, Ordering::Release);
        self.appl_ptr.store(0, Ordering::Release);

        // Reset buffer
        if let Some(ref mut buffer) = self.ring_buffer {
            buffer.reset();
        }

        // Configure hardware
        let _device = self.device.write();
        // device.configure_pcm(&self.hw_params)?;

        self.state = PcmState::Prepared;

        Ok(())
    }

    /// Start stream
    pub fn start(&mut self) -> Result<(), SoundError> {
        if self.state != PcmState::Prepared {
            return Err(SoundError::InvalidState);
        }

        // Record trigger timestamp
        *self.trigger_tstamp.lock() = current_timestamp();

        // Start hardware
        let _device = self.device.write();
        // device.start_dma()?;

        self.running.store(true, Ordering::Release);
        self.state = PcmState::Running;

        Ok(())
    }

    /// Stop stream
    pub fn stop(&mut self) -> Result<(), SoundError> {
        match self.state {
            PcmState::Running | PcmState::Draining | PcmState::Paused | PcmState::Xrun => {}
            _ => return Err(SoundError::InvalidState),
        }

        // Stop hardware
        let _device = self.device.write();
        // device.stop_dma()?;

        self.running.store(false, Ordering::Release);
        self.state = PcmState::Setup;

        Ok(())
    }

    /// Pause stream
    pub fn pause(&mut self, pause: bool) -> Result<(), SoundError> {
        if pause {
            if self.state != PcmState::Running {
                return Err(SoundError::InvalidState);
            }

            // Pause hardware
            self.running.store(false, Ordering::Release);
            self.state = PcmState::Paused;
        } else {
            if self.state != PcmState::Paused {
                return Err(SoundError::InvalidState);
            }

            // Resume hardware
            self.running.store(true, Ordering::Release);
            self.state = PcmState::Running;
        }

        Ok(())
    }

    /// Drain stream (playback only)
    pub fn drain(&mut self) -> Result<(), SoundError> {
        if self.direction != StreamDirection::Playback {
            return Err(SoundError::NotSupported);
        }

        if self.state != PcmState::Running {
            return Err(SoundError::InvalidState);
        }

        self.state = PcmState::Draining;

        // Wait for buffer to empty
        while let Some(ref buffer) = self.ring_buffer {
            if buffer.available_read() == 0 {
                break;
            }
            // Would yield/sleep here
        }

        self.state = PcmState::Setup;

        Ok(())
    }

    /// Drop frames (capture) or silence (playback)
    pub fn drop_frames(&mut self) -> Result<(), SoundError> {
        if self.state != PcmState::Running && self.state != PcmState::Xrun {
            return Err(SoundError::InvalidState);
        }

        // Reset buffer
        if let Some(ref mut buffer) = self.ring_buffer {
            buffer.reset();
        }

        self.hw_ptr.store(0, Ordering::Release);
        self.appl_ptr.store(0, Ordering::Release);

        self.state = PcmState::Setup;

        Ok(())
    }

    /// Write frames (playback)
    pub fn write_frames(&mut self, data: &[u8]) -> Result<usize, SoundError> {
        if self.direction != StreamDirection::Playback {
            return Err(SoundError::NotSupported);
        }

        match self.state {
            PcmState::Prepared | PcmState::Running => {}
            PcmState::Xrun => return Err(SoundError::Underrun),
            _ => return Err(SoundError::InvalidState),
        }

        let buffer = self.ring_buffer.as_mut().ok_or(SoundError::NotReady)?;

        let frame_size = self.hw_params.channels as usize *
                         self.hw_params.format.bytes_per_sample();
        let _frames_to_write = data.len() / frame_size;

        let written = buffer.write(data);

        // Update application pointer
        let frames_written = written / frame_size;
        self.appl_ptr.fetch_add(frames_written as u64, Ordering::AcqRel);

        // Auto-start if start threshold reached
        if self.state == PcmState::Prepared {
            let avail = self.available_frames();
            if avail >= self.sw_params.start_threshold as u64 {
                self.start()?;
            }
        }

        Ok(frames_written)
    }

    /// Read frames (capture)
    pub fn read_frames(&mut self, data: &mut [u8]) -> Result<usize, SoundError> {
        if self.direction != StreamDirection::Capture {
            return Err(SoundError::NotSupported);
        }

        match self.state {
            PcmState::Prepared | PcmState::Running => {}
            PcmState::Xrun => return Err(SoundError::Overrun),
            _ => return Err(SoundError::InvalidState),
        }

        let buffer = self.ring_buffer.as_mut().ok_or(SoundError::NotReady)?;

        let frame_size = self.hw_params.channels as usize *
                         self.hw_params.format.bytes_per_sample();

        let read = buffer.read(data);

        // Update application pointer
        let frames_read = read / frame_size;
        self.appl_ptr.fetch_add(frames_read as u64, Ordering::AcqRel);

        Ok(frames_read)
    }

    /// Get available frames
    pub fn available_frames(&self) -> u64 {
        let hw_ptr = self.hw_ptr.load(Ordering::Acquire);
        let appl_ptr = self.appl_ptr.load(Ordering::Acquire);
        let buffer_size = self.hw_params.buffer_size as u64;

        match self.direction {
            StreamDirection::Playback => {
                // Available for writing
                if appl_ptr >= hw_ptr {
                    buffer_size - (appl_ptr - hw_ptr)
                } else {
                    hw_ptr - appl_ptr
                }
            }
            StreamDirection::Capture => {
                // Available for reading
                if hw_ptr >= appl_ptr {
                    hw_ptr - appl_ptr
                } else {
                    buffer_size - (appl_ptr - hw_ptr)
                }
            }
        }
    }

    /// Get delay in frames
    pub fn delay(&self) -> i64 {
        let hw_ptr = self.hw_ptr.load(Ordering::Acquire);
        let appl_ptr = self.appl_ptr.load(Ordering::Acquire);

        match self.direction {
            StreamDirection::Playback => {
                (appl_ptr as i64) - (hw_ptr as i64)
            }
            StreamDirection::Capture => {
                (hw_ptr as i64) - (appl_ptr as i64)
            }
        }
    }

    /// Get status
    pub fn status(&self) -> PcmStatus {
        PcmStatus {
            state: self.state as u32,
            hw_ptr: self.hw_ptr.load(Ordering::Acquire),
            appl_ptr: self.appl_ptr.load(Ordering::Acquire),
            delay: self.delay(),
            avail: self.available_frames(),
            avail_max: self.hw_params.buffer_size as u64,
            overrun: 0,
            trigger_tstamp: *self.trigger_tstamp.lock(),
            audio_tstamp: current_timestamp(),
        }
    }

    /// Handle interrupt (called by driver)
    pub fn handle_interrupt(&mut self, frames: u32) {
        self.hw_ptr.fetch_add(frames as u64, Ordering::AcqRel);

        // Check for xrun
        let delay = self.delay();
        let buffer_size = self.hw_params.buffer_size as i64;

        match self.direction {
            StreamDirection::Playback => {
                if delay <= 0 && self.state == PcmState::Running {
                    self.state = PcmState::Xrun;
                    super::SOUND.record_xrun();
                }
            }
            StreamDirection::Capture => {
                if delay >= buffer_size && self.state == PcmState::Running {
                    self.state = PcmState::Xrun;
                    super::SOUND.record_xrun();
                }
            }
        }
    }

    /// Close stream
    pub fn close(&mut self) -> Result<(), SoundError> {
        if self.state == PcmState::Running || self.state == PcmState::Draining {
            self.stop()?;
        }

        self.ring_buffer = None;
        self.state = PcmState::Disconnected;

        Ok(())
    }
}

/// Open PCM stream
pub fn open_stream(
    device: Arc<RwLock<SoundDevice>>,
    direction: StreamDirection,
) -> Result<Arc<Mutex<PcmStream>>, SoundError> {
    Ok(Arc::new(Mutex::new(PcmStream::new(device, direction))))
}

/// Get current timestamp
fn current_timestamp() -> Timestamp {
    let ns = crate::time::current_time_ns();
    Timestamp {
        sec: (ns / 1_000_000_000) as i64,
        nsec: (ns % 1_000_000_000) as i64,
    }
}
