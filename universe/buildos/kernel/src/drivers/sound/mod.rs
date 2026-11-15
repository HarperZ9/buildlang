// ===============================================================================
// BUILDOS KERNEL - SOUND SUBSYSTEM
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================
//
// Audio subsystem supporting AC'97 and Intel High Definition Audio (HDA).
// Provides unified audio API for playback and recording.
//
// ===============================================================================

pub mod ac97;
pub mod hda;
pub mod mixer;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::{Mutex, RwLock};

// =============================================================================
// AUDIO FORMAT DEFINITIONS
// =============================================================================

/// Sample format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    /// Unsigned 8-bit
    U8,
    /// Signed 16-bit little-endian
    S16Le,
    /// Signed 16-bit big-endian
    S16Be,
    /// Signed 24-bit little-endian (packed)
    S24Le,
    /// Signed 24-bit big-endian (packed)
    S24Be,
    /// Signed 32-bit little-endian
    S32Le,
    /// Signed 32-bit big-endian
    S32Be,
    /// 32-bit float little-endian
    F32Le,
    /// 32-bit float big-endian
    F32Be,
}

impl SampleFormat {
    /// Get bytes per sample
    pub fn bytes_per_sample(&self) -> usize {
        match self {
            SampleFormat::U8 => 1,
            SampleFormat::S16Le | SampleFormat::S16Be => 2,
            SampleFormat::S24Le | SampleFormat::S24Be => 3,
            SampleFormat::S32Le | SampleFormat::S32Be => 4,
            SampleFormat::F32Le | SampleFormat::F32Be => 4,
        }
    }

    /// Get bits per sample
    pub fn bits_per_sample(&self) -> u32 {
        (self.bytes_per_sample() * 8) as u32
    }

    /// Check if format is signed
    pub fn is_signed(&self) -> bool {
        !matches!(self, SampleFormat::U8)
    }
}

/// Audio stream format
#[derive(Debug, Clone, Copy)]
pub struct AudioFormat {
    /// Sample format
    pub sample_format: SampleFormat,
    /// Number of channels (1=mono, 2=stereo, etc.)
    pub channels: u8,
    /// Sample rate in Hz
    pub sample_rate: u32,
}

impl AudioFormat {
    pub fn new(sample_format: SampleFormat, channels: u8, sample_rate: u32) -> Self {
        Self {
            sample_format,
            channels,
            sample_rate,
        }
    }

    /// CD quality: 16-bit stereo 44.1kHz
    pub fn cd_quality() -> Self {
        Self::new(SampleFormat::S16Le, 2, 44100)
    }

    /// DVD quality: 16-bit stereo 48kHz
    pub fn dvd_quality() -> Self {
        Self::new(SampleFormat::S16Le, 2, 48000)
    }

    /// Bytes per frame (all channels for one sample)
    pub fn bytes_per_frame(&self) -> usize {
        self.sample_format.bytes_per_sample() * self.channels as usize
    }

    /// Bytes per second
    pub fn bytes_per_second(&self) -> usize {
        self.bytes_per_frame() * self.sample_rate as usize
    }

    /// Duration in microseconds for given number of bytes
    pub fn bytes_to_usec(&self, bytes: usize) -> u64 {
        let frames = bytes / self.bytes_per_frame();
        (frames as u64 * 1_000_000) / self.sample_rate as u64
    }

    /// Bytes for given duration in microseconds
    pub fn usec_to_bytes(&self, usec: u64) -> usize {
        let frames = (usec * self.sample_rate as u64) / 1_000_000;
        frames as usize * self.bytes_per_frame()
    }
}

impl Default for AudioFormat {
    fn default() -> Self {
        Self::cd_quality()
    }
}

// =============================================================================
// AUDIO STREAM STATE
// =============================================================================

/// Stream state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    /// Stream is closed
    Closed,
    /// Stream is open but not running
    Open,
    /// Stream is prepared and ready to run
    Prepared,
    /// Stream is running
    Running,
    /// Stream is paused
    Paused,
    /// Stream encountered an error
    Error,
}

/// Stream direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamDirection {
    /// Playback (output)
    Playback,
    /// Capture (input)
    Capture,
}

// =============================================================================
// AUDIO BUFFER
// =============================================================================

/// Audio buffer for DMA transfers
pub struct AudioBuffer {
    data: Box<[u8]>,
    physical: u64,
    size: usize,
    position: usize,
}

impl AudioBuffer {
    pub fn new(size: usize) -> Self {
        let data = alloc::vec![0u8; size].into_boxed_slice();
        let physical = data.as_ptr() as u64;
        Self {
            data,
            physical,
            size,
            position: 0,
        }
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.data.as_ptr()
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.data.as_mut_ptr()
    }

    pub fn physical_address(&self) -> u64 {
        self.physical
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn position(&self) -> usize {
        self.position
    }

    pub fn set_position(&mut self, pos: usize) {
        self.position = pos.min(self.size);
    }

    pub fn write(&mut self, data: &[u8]) -> usize {
        let available = self.size - self.position;
        let to_write = data.len().min(available);
        self.data[self.position..self.position + to_write].copy_from_slice(&data[..to_write]);
        self.position += to_write;
        to_write
    }

    pub fn read(&mut self, data: &mut [u8]) -> usize {
        let available = self.size - self.position;
        let to_read = data.len().min(available);
        data[..to_read].copy_from_slice(&self.data[self.position..self.position + to_read]);
        self.position += to_read;
        to_read
    }

    pub fn clear(&mut self) {
        self.data.fill(0);
        self.position = 0;
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

// =============================================================================
// RING BUFFER FOR AUDIO STREAMING
// =============================================================================

/// Thread-safe ring buffer for audio streaming
pub struct AudioRingBuffer {
    buffer: Box<[u8]>,
    size: usize,
    read_pos: AtomicU32,
    write_pos: AtomicU32,
}

impl AudioRingBuffer {
    pub fn new(size: usize) -> Self {
        Self {
            buffer: alloc::vec![0u8; size].into_boxed_slice(),
            size,
            read_pos: AtomicU32::new(0),
            write_pos: AtomicU32::new(0),
        }
    }

    /// Available bytes to read
    pub fn available(&self) -> usize {
        let write = self.write_pos.load(Ordering::Acquire) as usize;
        let read = self.read_pos.load(Ordering::Acquire) as usize;

        if write >= read {
            write - read
        } else {
            self.size - read + write
        }
    }

    /// Free space for writing
    pub fn free(&self) -> usize {
        self.size - self.available() - 1
    }

    /// Write data to ring buffer
    pub fn write(&self, data: &[u8]) -> usize {
        let free = self.free();
        let to_write = data.len().min(free);

        if to_write == 0 {
            return 0;
        }

        let write_pos = self.write_pos.load(Ordering::Acquire) as usize;

        // Calculate how much we can write before wrapping
        let first_chunk = (self.size - write_pos).min(to_write);
        let second_chunk = to_write - first_chunk;

        // Safety: We're modifying disjoint parts of the buffer
        unsafe {
            let ptr = self.buffer.as_ptr() as *mut u8;
            core::ptr::copy_nonoverlapping(data.as_ptr(), ptr.add(write_pos), first_chunk);
            if second_chunk > 0 {
                core::ptr::copy_nonoverlapping(data.as_ptr().add(first_chunk), ptr, second_chunk);
            }
        }

        let new_pos = (write_pos + to_write) % self.size;
        self.write_pos.store(new_pos as u32, Ordering::Release);

        to_write
    }

    /// Read data from ring buffer
    pub fn read(&self, data: &mut [u8]) -> usize {
        let available = self.available();
        let to_read = data.len().min(available);

        if to_read == 0 {
            return 0;
        }

        let read_pos = self.read_pos.load(Ordering::Acquire) as usize;

        let first_chunk = (self.size - read_pos).min(to_read);
        let second_chunk = to_read - first_chunk;

        data[..first_chunk].copy_from_slice(&self.buffer[read_pos..read_pos + first_chunk]);
        if second_chunk > 0 {
            data[first_chunk..to_read].copy_from_slice(&self.buffer[..second_chunk]);
        }

        let new_pos = (read_pos + to_read) % self.size;
        self.read_pos.store(new_pos as u32, Ordering::Release);

        to_read
    }

    /// Clear the ring buffer
    pub fn clear(&self) {
        self.read_pos.store(0, Ordering::Release);
        self.write_pos.store(0, Ordering::Release);
    }
}

// =============================================================================
// AUDIO DEVICE TRAIT
// =============================================================================

/// Audio device capabilities
#[derive(Debug, Clone)]
pub struct AudioDeviceInfo {
    /// Device name
    pub name: String,
    /// Supported sample rates
    pub sample_rates: Vec<u32>,
    /// Supported sample formats
    pub sample_formats: Vec<SampleFormat>,
    /// Minimum channels
    pub min_channels: u8,
    /// Maximum channels
    pub max_channels: u8,
    /// Supports playback
    pub playback: bool,
    /// Supports capture
    pub capture: bool,
}

/// Audio device trait
pub trait AudioDevice: Send + Sync {
    /// Get device info
    fn info(&self) -> AudioDeviceInfo;

    /// Open a stream
    fn open(
        &self,
        direction: StreamDirection,
        format: AudioFormat,
    ) -> Result<Box<dyn AudioStream>, AudioError>;

    /// Get current volume (0-100)
    fn volume(&self) -> u8;

    /// Set volume (0-100)
    fn set_volume(&self, volume: u8);

    /// Check if muted
    fn is_muted(&self) -> bool;

    /// Set mute state
    fn set_muted(&self, muted: bool);
}

// =============================================================================
// AUDIO STREAM TRAIT
// =============================================================================

/// Audio stream trait
pub trait AudioStream: Send + Sync {
    /// Get stream format
    fn format(&self) -> AudioFormat;

    /// Get stream state
    fn state(&self) -> StreamState;

    /// Get stream direction
    fn direction(&self) -> StreamDirection;

    /// Prepare stream for playback/capture
    fn prepare(&self) -> Result<(), AudioError>;

    /// Start stream
    fn start(&self) -> Result<(), AudioError>;

    /// Stop stream
    fn stop(&self) -> Result<(), AudioError>;

    /// Pause stream
    fn pause(&self) -> Result<(), AudioError>;

    /// Resume stream
    fn resume(&self) -> Result<(), AudioError>;

    /// Write audio data (playback)
    fn write(&self, data: &[u8]) -> Result<usize, AudioError>;

    /// Read audio data (capture)
    fn read(&self, data: &mut [u8]) -> Result<usize, AudioError>;

    /// Get available space for writing
    fn available_write(&self) -> usize;

    /// Get available data for reading
    fn available_read(&self) -> usize;

    /// Drain stream (wait for all data to play)
    fn drain(&self) -> Result<(), AudioError>;

    /// Get current delay in frames
    fn delay(&self) -> usize;
}

// =============================================================================
// AUDIO ERRORS
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioError {
    /// Device not found
    DeviceNotFound,
    /// Invalid format
    InvalidFormat,
    /// Format not supported
    FormatNotSupported,
    /// Stream not open
    StreamNotOpen,
    /// Stream already open
    StreamAlreadyOpen,
    /// Invalid state for operation
    InvalidState,
    /// Buffer underrun (playback)
    Underrun,
    /// Buffer overrun (capture)
    Overrun,
    /// Hardware error
    HardwareError,
    /// Timeout
    Timeout,
    /// No memory
    NoMemory,
    /// Not implemented
    NotImplemented,
}

// =============================================================================
// AUDIO MIXER
// =============================================================================

/// Mixer control type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixerControlType {
    Volume,
    Mute,
    Switch,
    Enum,
}

/// Mixer control
pub struct MixerControl {
    pub name: String,
    pub control_type: MixerControlType,
    pub min: i32,
    pub max: i32,
    pub value: i32,
    pub is_writable: bool,
}

// =============================================================================
// AUDIO SUBSYSTEM
// =============================================================================

/// Global audio subsystem
pub struct AudioSubsystem {
    devices: RwLock<Vec<Arc<dyn AudioDevice>>>,
    default_playback: Mutex<Option<usize>>,
    default_capture: Mutex<Option<usize>>,
    master_volume: AtomicU32,
    muted: AtomicBool,
}

impl AudioSubsystem {
    pub const fn new() -> Self {
        Self {
            devices: RwLock::new(Vec::new()),
            default_playback: Mutex::new(None),
            default_capture: Mutex::new(None),
            master_volume: AtomicU32::new(100),
            muted: AtomicBool::new(false),
        }
    }

    /// Register an audio device
    pub fn register_device(&self, device: Arc<dyn AudioDevice>) {
        let info = device.info();
        let mut devices = self.devices.write();
        let index = devices.len();

        devices.push(device);

        // Set as default if first device
        if info.playback && self.default_playback.lock().is_none() {
            *self.default_playback.lock() = Some(index);
        }
        if info.capture && self.default_capture.lock().is_none() {
            *self.default_capture.lock() = Some(index);
        }

        crate::log::info!("Audio: Registered device '{}'", info.name);
    }

    /// Get all devices
    pub fn devices(&self) -> Vec<Arc<dyn AudioDevice>> {
        self.devices.read().clone()
    }

    /// Get device by index
    pub fn device(&self, index: usize) -> Option<Arc<dyn AudioDevice>> {
        self.devices.read().get(index).cloned()
    }

    /// Get default playback device
    pub fn default_playback_device(&self) -> Option<Arc<dyn AudioDevice>> {
        let index = (*self.default_playback.lock())?;
        self.device(index)
    }

    /// Get default capture device
    pub fn default_capture_device(&self) -> Option<Arc<dyn AudioDevice>> {
        let index = (*self.default_capture.lock())?;
        self.device(index)
    }

    /// Set default playback device
    pub fn set_default_playback(&self, index: usize) {
        *self.default_playback.lock() = Some(index);
    }

    /// Set default capture device
    pub fn set_default_capture(&self, index: usize) {
        *self.default_capture.lock() = Some(index);
    }

    /// Get master volume (0-100)
    pub fn master_volume(&self) -> u8 {
        self.master_volume.load(Ordering::Acquire) as u8
    }

    /// Set master volume (0-100)
    pub fn set_master_volume(&self, volume: u8) {
        self.master_volume.store(volume.min(100) as u32, Ordering::Release);
    }

    /// Check if muted
    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Acquire)
    }

    /// Set mute state
    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Release);
    }

    /// Open a playback stream on default device
    pub fn open_playback(&self, format: AudioFormat) -> Result<Box<dyn AudioStream>, AudioError> {
        let device = self.default_playback_device().ok_or(AudioError::DeviceNotFound)?;
        device.open(StreamDirection::Playback, format)
    }

    /// Open a capture stream on default device
    pub fn open_capture(&self, format: AudioFormat) -> Result<Box<dyn AudioStream>, AudioError> {
        let device = self.default_capture_device().ok_or(AudioError::DeviceNotFound)?;
        device.open(StreamDirection::Capture, format)
    }
}

// Global instance
static AUDIO_SUBSYSTEM: AudioSubsystem = AudioSubsystem::new();

pub fn audio_subsystem() -> &'static AudioSubsystem {
    &AUDIO_SUBSYSTEM
}

// =============================================================================
// INITIALIZATION
// =============================================================================

/// Initialize the audio subsystem
pub fn init() -> Result<(), &'static str> {
    crate::log::info!("Audio: Initializing audio subsystem");

    // Probe for audio controllers
    if let Err(e) = hda::probe_and_init() {
        crate::log::debug!("Audio: HDA probe failed: {}", e);
    }

    if let Err(e) = ac97::probe_and_init() {
        crate::log::debug!("Audio: AC97 probe failed: {}", e);
    }

    let device_count = AUDIO_SUBSYSTEM.devices.read().len();
    if device_count > 0 {
        crate::log::info!("Audio: {} audio device(s) found", device_count);
    } else {
        crate::log::warn!("Audio: No audio devices found");
    }

    Ok(())
}
