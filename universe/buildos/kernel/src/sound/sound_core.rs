//! Sound Core - Card and Device Management
//!
//! Provides the core abstractions for sound cards and devices.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicBool, Ordering};
use spin::RwLock;

use super::{
    CardType, SoundError, StreamDirection, SampleFormat,
    mixer::Mixer,
};

/// Sound card capabilities
#[derive(Clone, Copy, Debug)]
pub struct CardCapabilities {
    /// Supports playback
    pub playback: bool,
    /// Supports capture
    pub capture: bool,
    /// Supports full duplex
    pub full_duplex: bool,
    /// Supports hardware mixing
    pub hardware_mixing: bool,
    /// Supports hardware volume
    pub hardware_volume: bool,
    /// Maximum playback channels
    pub max_playback_channels: u32,
    /// Maximum capture channels
    pub max_capture_channels: u32,
    /// Supported sample rates
    pub sample_rates: SampleRateCapability,
    /// Supported formats
    pub formats: FormatCapability,
}

impl Default for CardCapabilities {
    fn default() -> Self {
        Self {
            playback: true,
            capture: true,
            full_duplex: true,
            hardware_mixing: false,
            hardware_volume: true,
            max_playback_channels: 2,
            max_capture_channels: 2,
            sample_rates: SampleRateCapability::default(),
            formats: FormatCapability::default(),
        }
    }
}

/// Sample rate capabilities
#[derive(Clone, Copy, Debug, Default)]
pub struct SampleRateCapability {
    /// Minimum rate
    pub min: u32,
    /// Maximum rate
    pub max: u32,
    /// Rate is continuous
    pub continuous: bool,
    /// Supported discrete rates (if not continuous)
    pub discrete_rates: [u32; 16],
    /// Number of discrete rates
    pub discrete_count: usize,
}

impl SampleRateCapability {
    /// Standard rates
    pub const STANDARD_RATES: [u32; 11] = [
        8000, 11025, 16000, 22050, 32000, 44100, 48000, 88200, 96000, 176400, 192000
    ];

    /// Create with continuous range
    pub fn continuous(min: u32, max: u32) -> Self {
        Self {
            min,
            max,
            continuous: true,
            discrete_rates: [0; 16],
            discrete_count: 0,
        }
    }

    /// Create with standard rates
    pub fn standard() -> Self {
        let mut rates = [0u32; 16];
        for (i, &rate) in Self::STANDARD_RATES.iter().take(11).enumerate() {
            rates[i] = rate;
        }
        Self {
            min: 8000,
            max: 192000,
            continuous: false,
            discrete_rates: rates,
            discrete_count: 11,
        }
    }

    /// Check if rate is supported
    pub fn supports(&self, rate: u32) -> bool {
        if self.continuous {
            rate >= self.min && rate <= self.max
        } else {
            self.discrete_rates[..self.discrete_count].contains(&rate)
        }
    }
}

/// Format capabilities
#[derive(Clone, Copy, Debug, Default)]
pub struct FormatCapability {
    /// Supported formats as bitmask
    formats: u32,
}

impl FormatCapability {
    /// Format bits
    pub const S8: u32 = 1 << 0;
    pub const U8: u32 = 1 << 1;
    pub const S16_LE: u32 = 1 << 2;
    pub const S16_BE: u32 = 1 << 3;
    pub const U16_LE: u32 = 1 << 4;
    pub const U16_BE: u32 = 1 << 5;
    pub const S24_LE: u32 = 1 << 6;
    pub const S24_BE: u32 = 1 << 7;
    pub const S24_LE_32: u32 = 1 << 8;
    pub const S24_BE_32: u32 = 1 << 9;
    pub const S32_LE: u32 = 1 << 10;
    pub const S32_BE: u32 = 1 << 11;
    pub const FLOAT_LE: u32 = 1 << 12;
    pub const FLOAT_BE: u32 = 1 << 13;

    /// Common formats (S16LE, S24LE, S32LE)
    pub const COMMON: u32 = Self::S16_LE | Self::S24_LE | Self::S32_LE;

    /// All integer formats
    pub const ALL_INTEGER: u32 = Self::S8 | Self::U8 | Self::S16_LE | Self::S16_BE |
                                  Self::U16_LE | Self::U16_BE | Self::S24_LE | Self::S24_BE |
                                  Self::S24_LE_32 | Self::S24_BE_32 | Self::S32_LE | Self::S32_BE;

    /// Create with formats
    pub fn new(formats: u32) -> Self {
        Self { formats }
    }

    /// Check if format is supported
    pub fn supports(&self, format: SampleFormat) -> bool {
        let bit = match format {
            SampleFormat::S8 => Self::S8,
            SampleFormat::U8 => Self::U8,
            SampleFormat::S16Le => Self::S16_LE,
            SampleFormat::S16Be => Self::S16_BE,
            SampleFormat::U16Le => Self::U16_LE,
            SampleFormat::U16Be => Self::U16_BE,
            SampleFormat::S24Le => Self::S24_LE,
            SampleFormat::S24Be => Self::S24_BE,
            SampleFormat::S24Le32 => Self::S24_LE_32,
            SampleFormat::S24Be32 => Self::S24_BE_32,
            SampleFormat::S32Le => Self::S32_LE,
            SampleFormat::S32Be => Self::S32_BE,
            SampleFormat::FloatLe => Self::FLOAT_LE,
            SampleFormat::FloatBe => Self::FLOAT_BE,
            _ => return false,
        };
        (self.formats & bit) != 0
    }

    /// Add format
    pub fn add(&mut self, format: u32) {
        self.formats |= format;
    }

    /// Get raw value
    pub fn raw(&self) -> u32 {
        self.formats
    }
}

/// Sound device (subdevice of a card)
pub struct SoundDevice {
    /// Device ID
    pub id: u32,
    /// Device name
    pub name: String,
    /// Device type
    pub device_type: DeviceType,
    /// Stream direction
    pub direction: StreamDirection,
    /// Is opened
    pub opened: AtomicBool,
    /// Number of substreams
    pub substreams: u32,
    /// Available substreams
    pub available_substreams: AtomicU32,
    /// Capabilities
    pub capabilities: CardCapabilities,
    /// Hardware operations
    pub ops: Option<Box<dyn SoundDeviceOps>>,
}

/// Device type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceType {
    /// PCM device
    Pcm,
    /// Raw MIDI
    RawMidi,
    /// Timer
    Timer,
    /// Sequencer
    Sequencer,
    /// Hardware dependent
    Hwdep,
}

/// Sound device operations trait
pub trait SoundDeviceOps: Send + Sync {
    /// Open device
    fn open(&self, direction: StreamDirection) -> Result<(), SoundError>;

    /// Close device
    fn close(&self) -> Result<(), SoundError>;

    /// Get hardware parameters
    fn get_hw_params(&self) -> CardCapabilities;

    /// Set hardware parameters
    fn set_hw_params(
        &self,
        rate: u32,
        channels: u32,
        format: SampleFormat,
        buffer_size: u32,
        period_size: u32,
    ) -> Result<(), SoundError>;

    /// Prepare for playback/capture
    fn prepare(&self) -> Result<(), SoundError>;

    /// Start DMA
    fn start(&self) -> Result<(), SoundError>;

    /// Stop DMA
    fn stop(&self) -> Result<(), SoundError>;

    /// Get DMA position (frames)
    fn get_position(&self) -> u64;

    /// Get available frames
    fn get_available(&self) -> u32;

    /// Write frames (playback)
    fn write(&self, data: &[u8]) -> Result<usize, SoundError>;

    /// Read frames (capture)
    fn read(&self, data: &mut [u8]) -> Result<usize, SoundError>;

    /// Drain buffer
    fn drain(&self) -> Result<(), SoundError>;

    /// Pause/resume
    fn pause(&self, pause: bool) -> Result<(), SoundError>;
}

impl SoundDevice {
    /// Create new sound device
    pub fn new(
        id: u32,
        name: String,
        device_type: DeviceType,
        direction: StreamDirection,
        substreams: u32,
    ) -> Self {
        Self {
            id,
            name,
            device_type,
            direction,
            opened: AtomicBool::new(false),
            substreams,
            available_substreams: AtomicU32::new(substreams),
            capabilities: CardCapabilities::default(),
            ops: None,
        }
    }

    /// Set hardware operations
    pub fn set_ops(&mut self, ops: Box<dyn SoundDeviceOps>) {
        self.ops = Some(ops);
    }

    /// Open device
    pub fn open(&self) -> Result<(), SoundError> {
        if self.opened.swap(true, Ordering::SeqCst) {
            return Err(SoundError::DeviceBusy);
        }

        if let Some(ops) = &self.ops {
            if let Err(e) = ops.open(self.direction) {
                self.opened.store(false, Ordering::SeqCst);
                return Err(e);
            }
        }

        Ok(())
    }

    /// Close device
    pub fn close(&self) -> Result<(), SoundError> {
        if !self.opened.swap(false, Ordering::SeqCst) {
            return Ok(()); // Already closed
        }

        if let Some(ops) = &self.ops {
            ops.close()?;
        }

        Ok(())
    }

    /// Check if device is opened
    pub fn is_opened(&self) -> bool {
        self.opened.load(Ordering::Acquire)
    }

    /// Acquire substream
    pub fn acquire_substream(&self) -> Option<u32> {
        loop {
            let available = self.available_substreams.load(Ordering::Acquire);
            if available == 0 {
                return None;
            }
            if self.available_substreams.compare_exchange(
                available,
                available - 1,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ).is_ok() {
                return Some(self.substreams - available);
            }
        }
    }

    /// Release substream
    pub fn release_substream(&self, _substream: u32) {
        self.available_substreams.fetch_add(1, Ordering::SeqCst);
    }
}

/// Sound card
pub struct SoundCard {
    /// Card ID
    pub id: u32,
    /// Card type
    pub card_type: CardType,
    /// Driver name
    pub driver: String,
    /// Card name
    pub name: String,
    /// Long name
    pub long_name: String,
    /// Mixer ID
    pub mixer_name: String,
    /// Components
    pub components: String,
    /// Is registered
    pub registered: bool,
    /// Devices
    devices: Vec<Arc<RwLock<SoundDevice>>>,
    /// Mixer
    mixer: Option<Arc<RwLock<Mixer>>>,
    /// Private data
    private_data: Option<Box<dyn core::any::Any + Send + Sync>>,
}

impl SoundCard {
    /// Create new sound card
    pub fn new(
        card_type: CardType,
        driver: &str,
        name: &str,
    ) -> Self {
        Self {
            id: 0,
            card_type,
            driver: String::from(driver),
            name: String::from(name),
            long_name: String::from(name),
            mixer_name: String::from(""),
            components: String::new(),
            registered: false,
            devices: Vec::new(),
            mixer: None,
            private_data: None,
        }
    }

    /// Set long name
    pub fn set_long_name(&mut self, name: &str) {
        self.long_name = String::from(name);
    }

    /// Set mixer name
    pub fn set_mixer_name(&mut self, name: &str) {
        self.mixer_name = String::from(name);
    }

    /// Add component
    pub fn add_component(&mut self, component: &str) {
        if !self.components.is_empty() {
            self.components.push_str(", ");
        }
        self.components.push_str(component);
    }

    /// Add device
    pub fn add_device(&mut self, device: SoundDevice) -> u32 {
        let id = device.id;
        self.devices.push(Arc::new(RwLock::new(device)));
        id
    }

    /// Get device
    pub fn get_device(&self, device_id: u32) -> Option<Arc<RwLock<SoundDevice>>> {
        self.devices.iter()
            .find(|d| d.read().id == device_id)
            .cloned()
    }

    /// Get devices
    pub fn get_devices(&self) -> &[Arc<RwLock<SoundDevice>>] {
        &self.devices
    }

    /// Get playback devices
    pub fn get_playback_devices(&self) -> Vec<Arc<RwLock<SoundDevice>>> {
        self.devices.iter()
            .filter(|d| d.read().direction == StreamDirection::Playback)
            .cloned()
            .collect()
    }

    /// Get capture devices
    pub fn get_capture_devices(&self) -> Vec<Arc<RwLock<SoundDevice>>> {
        self.devices.iter()
            .filter(|d| d.read().direction == StreamDirection::Capture)
            .cloned()
            .collect()
    }

    /// Device count
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// Set mixer
    pub fn set_mixer(&mut self, mixer: Mixer) {
        self.mixer = Some(Arc::new(RwLock::new(mixer)));
    }

    /// Get mixer
    pub fn get_mixer(&self) -> Option<Arc<RwLock<Mixer>>> {
        self.mixer.clone()
    }

    /// Set private data
    pub fn set_private_data<T: 'static + Send + Sync>(&mut self, data: T) {
        self.private_data = Some(Box::new(data));
    }

    /// Get private data
    pub fn get_private_data<T: 'static>(&self) -> Option<&T> {
        self.private_data.as_ref()?.downcast_ref()
    }

    /// Get private data mut
    pub fn get_private_data_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.private_data.as_mut()?.downcast_mut()
    }

    /// Register card
    pub fn register(&mut self) {
        self.registered = true;
    }

    /// Unregister card
    pub fn unregister(&mut self) {
        self.registered = false;
    }
}

/// PCM substream info
#[derive(Clone, Debug)]
pub struct PcmInfo {
    /// Card index
    pub card: u32,
    /// Device index
    pub device: u32,
    /// Subdevice index
    pub subdevice: u32,
    /// Stream direction
    pub stream: StreamDirection,
    /// Card ID
    pub id: String,
    /// Name
    pub name: String,
    /// Subname
    pub subname: String,
    /// Device class
    pub dev_class: PcmClass,
    /// Device subclass
    pub dev_subclass: PcmSubclass,
    /// Subdevices count
    pub subdevices_count: u32,
    /// Available subdevices
    pub subdevices_avail: u32,
}

/// PCM device class
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PcmClass {
    /// Generic PCM
    Generic,
    /// Multi-channel
    Multi,
    /// Modem
    Modem,
    /// Digitizer
    Digitizer,
}

/// PCM device subclass
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PcmSubclass {
    /// Generic mix
    GenericMix,
    /// Multi-channel mix
    MultiMix,
}

/// Card info
#[derive(Clone, Debug)]
pub struct CardInfo {
    /// Card index
    pub card: u32,
    /// Card ID
    pub id: String,
    /// Driver name
    pub driver: String,
    /// Card name
    pub name: String,
    /// Long name
    pub long_name: String,
    /// Mixer name
    pub mixer_name: String,
    /// Components
    pub components: String,
}

impl From<&SoundCard> for CardInfo {
    fn from(card: &SoundCard) -> Self {
        Self {
            card: card.id,
            id: card.name.clone(),
            driver: card.driver.clone(),
            name: card.name.clone(),
            long_name: card.long_name.clone(),
            mixer_name: card.mixer_name.clone(),
            components: card.components.clone(),
        }
    }
}

/// DMA buffer for sound
pub struct DmaBuffer {
    /// Physical address
    pub phys_addr: u64,
    /// Virtual address
    pub virt_addr: usize,
    /// Size in bytes
    pub size: usize,
    /// Period size in bytes
    pub period_size: usize,
    /// Number of periods
    pub periods: u32,
    /// Current write position
    write_pos: AtomicU32,
    /// Current read position
    read_pos: AtomicU32,
}

impl DmaBuffer {
    /// Create new DMA buffer
    pub fn new(phys_addr: u64, virt_addr: usize, size: usize, period_size: usize) -> Self {
        let periods = (size / period_size) as u32;
        Self {
            phys_addr,
            virt_addr,
            size,
            period_size,
            periods,
            write_pos: AtomicU32::new(0),
            read_pos: AtomicU32::new(0),
        }
    }

    /// Get buffer pointer
    pub fn as_ptr(&self) -> *mut u8 {
        self.virt_addr as *mut u8
    }

    /// Get slice at offset
    pub fn slice(&self, offset: usize, len: usize) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(
                (self.virt_addr + offset) as *const u8,
                len.min(self.size - offset)
            )
        }
    }

    /// Get mutable slice at offset
    pub fn slice_mut(&self, offset: usize, len: usize) -> &mut [u8] {
        unsafe {
            core::slice::from_raw_parts_mut(
                (self.virt_addr + offset) as *mut u8,
                len.min(self.size - offset)
            )
        }
    }

    /// Get write position
    pub fn write_position(&self) -> u32 {
        self.write_pos.load(Ordering::Acquire)
    }

    /// Set write position
    pub fn set_write_position(&self, pos: u32) {
        self.write_pos.store(pos % self.size as u32, Ordering::Release);
    }

    /// Advance write position
    pub fn advance_write(&self, bytes: u32) -> u32 {
        let new_pos = (self.write_pos.load(Ordering::Acquire) + bytes) % self.size as u32;
        self.write_pos.store(new_pos, Ordering::Release);
        new_pos
    }

    /// Get read position
    pub fn read_position(&self) -> u32 {
        self.read_pos.load(Ordering::Acquire)
    }

    /// Set read position
    pub fn set_read_position(&self, pos: u32) {
        self.read_pos.store(pos % self.size as u32, Ordering::Release);
    }

    /// Advance read position
    pub fn advance_read(&self, bytes: u32) -> u32 {
        let new_pos = (self.read_pos.load(Ordering::Acquire) + bytes) % self.size as u32;
        self.read_pos.store(new_pos, Ordering::Release);
        new_pos
    }

    /// Get available bytes to write
    pub fn available_write(&self) -> u32 {
        let read = self.read_pos.load(Ordering::Acquire);
        let write = self.write_pos.load(Ordering::Acquire);
        if write >= read {
            self.size as u32 - (write - read) - 1
        } else {
            read - write - 1
        }
    }

    /// Get available bytes to read
    pub fn available_read(&self) -> u32 {
        let read = self.read_pos.load(Ordering::Acquire);
        let write = self.write_pos.load(Ordering::Acquire);
        if write >= read {
            write - read
        } else {
            self.size as u32 - read + write
        }
    }

    /// Clear buffer
    pub fn clear(&self) {
        unsafe {
            core::ptr::write_bytes(self.virt_addr as *mut u8, 0, self.size);
        }
        self.write_pos.store(0, Ordering::Release);
        self.read_pos.store(0, Ordering::Release);
    }
}

/// Timestamp for audio
#[derive(Clone, Copy, Debug, Default)]
pub struct AudioTimestamp {
    /// Seconds
    pub tv_sec: i64,
    /// Nanoseconds
    pub tv_nsec: i64,
}

impl AudioTimestamp {
    /// Create from nanoseconds
    pub fn from_nanos(nanos: u64) -> Self {
        Self {
            tv_sec: (nanos / 1_000_000_000) as i64,
            tv_nsec: (nanos % 1_000_000_000) as i64,
        }
    }

    /// Convert to nanoseconds
    pub fn to_nanos(&self) -> u64 {
        (self.tv_sec as u64) * 1_000_000_000 + self.tv_nsec as u64
    }

    /// Get current timestamp
    pub fn now() -> Self {
        let nanos = crate::time::monotonic_nanos();
        Self::from_nanos(nanos)
    }
}

/// Status info for PCM
#[derive(Clone, Debug, Default)]
pub struct PcmStatus {
    /// Stream state
    pub state: super::pcm::PcmState,
    /// Trigger timestamp
    pub trigger_tstamp: AudioTimestamp,
    /// Current timestamp
    pub tstamp: AudioTimestamp,
    /// Delay in frames
    pub delay: i64,
    /// Available frames
    pub avail: u64,
    /// Maximum available frames
    pub avail_max: u64,
    /// Overrange count
    pub overrange: u64,
    /// Suspended state
    pub suspended_state: super::pcm::PcmState,
    /// Audio timestamp
    pub audio_tstamp: AudioTimestamp,
}

/// Synchronization pointer
#[derive(Debug, Default)]
pub struct SyncPtr {
    /// Hardware pointer
    pub hw_ptr: u64,
    /// Application pointer
    pub appl_ptr: u64,
    /// Available minimum
    pub avail_min: u64,
}
