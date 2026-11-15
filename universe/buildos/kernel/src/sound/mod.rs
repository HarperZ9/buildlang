//! Sound Subsystem (ALSA-like)
//!
//! Provides audio playback and capture functionality:
//! - Sound card management
//! - PCM (Pulse Code Modulation) streams
//! - Mixer controls
//! - Hardware abstraction
//! - Intel HDA and AC'97 codec support

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::{Mutex, RwLock};

pub mod pcm;
pub mod mixer;
pub mod hda;
pub mod ac97;
pub mod sound_core;

pub use pcm::{PcmStream, PcmFormat, PcmState, PcmHwParams, PcmSwParams};
pub use mixer::{MixerControl, MixerElement, ControlType};
pub use sound_core::{SoundCard, SoundDevice};

/// Sound error types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundError {
    /// No such device
    NoDevice,
    /// Device busy
    DeviceBusy,
    /// Invalid parameter
    InvalidParameter,
    /// Buffer too small
    BufferTooSmall,
    /// Buffer overrun
    Overrun,
    /// Buffer underrun
    Underrun,
    /// Operation not supported
    NotSupported,
    /// Device not ready
    NotReady,
    /// Hardware error
    HardwareError,
    /// No memory
    NoMemory,
    /// Timeout
    Timeout,
    /// Interrupted
    Interrupted,
    /// Invalid state
    InvalidState,
    /// Format not supported
    FormatNotSupported,
    /// Rate not supported
    RateNotSupported,
    /// Channels not supported
    ChannelsNotSupported,
}

/// Sample format
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleFormat {
    /// Signed 8-bit
    S8,
    /// Unsigned 8-bit
    U8,
    /// Signed 16-bit little endian
    S16Le,
    /// Signed 16-bit big endian
    S16Be,
    /// Unsigned 16-bit little endian
    U16Le,
    /// Unsigned 16-bit big endian
    U16Be,
    /// Signed 24-bit little endian (3 bytes)
    S24Le,
    /// Signed 24-bit big endian (3 bytes)
    S24Be,
    /// Signed 24-bit in 32-bit container LE
    S24Le32,
    /// Signed 24-bit in 32-bit container BE
    S24Be32,
    /// Signed 32-bit little endian
    S32Le,
    /// Signed 32-bit big endian
    S32Be,
    /// Float 32-bit little endian
    FloatLe,
    /// Float 32-bit big endian
    FloatBe,
    /// Float 64-bit little endian
    Float64Le,
    /// Float 64-bit big endian
    Float64Be,
    /// IEC958 subframe LE
    Iec958SubframeLe,
    /// IEC958 subframe BE
    Iec958SubframeBe,
    /// Mu-law
    MuLaw,
    /// A-law
    ALaw,
    /// IMA ADPCM
    ImaAdpcm,
    /// MPEG
    Mpeg,
    /// GSM
    Gsm,
}

impl SampleFormat {
    /// Get bytes per sample
    pub fn bytes_per_sample(&self) -> usize {
        match self {
            Self::S8 | Self::U8 | Self::MuLaw | Self::ALaw => 1,
            Self::S16Le | Self::S16Be | Self::U16Le | Self::U16Be => 2,
            Self::S24Le | Self::S24Be => 3,
            Self::S24Le32 | Self::S24Be32 | Self::S32Le | Self::S32Be |
            Self::FloatLe | Self::FloatBe | Self::Iec958SubframeLe |
            Self::Iec958SubframeBe => 4,
            Self::Float64Le | Self::Float64Be => 8,
            _ => 4,
        }
    }

    /// Is format signed
    pub fn is_signed(&self) -> bool {
        matches!(self, Self::S8 | Self::S16Le | Self::S16Be |
                       Self::S24Le | Self::S24Be | Self::S24Le32 | Self::S24Be32 |
                       Self::S32Le | Self::S32Be)
    }

    /// Is format little endian
    pub fn is_little_endian(&self) -> bool {
        matches!(self, Self::S16Le | Self::U16Le | Self::S24Le |
                       Self::S24Le32 | Self::S32Le | Self::FloatLe |
                       Self::Float64Le | Self::Iec958SubframeLe)
    }

    /// Get physical width in bits
    pub fn physical_width(&self) -> u32 {
        (self.bytes_per_sample() * 8) as u32
    }

    /// Get significant bits
    pub fn significant_bits(&self) -> u32 {
        match self {
            Self::S8 | Self::U8 => 8,
            Self::S16Le | Self::S16Be | Self::U16Le | Self::U16Be => 16,
            Self::S24Le | Self::S24Be | Self::S24Le32 | Self::S24Be32 => 24,
            Self::S32Le | Self::S32Be => 32,
            Self::FloatLe | Self::FloatBe => 32,
            Self::Float64Le | Self::Float64Be => 64,
            _ => self.physical_width(),
        }
    }
}

/// Stream direction
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamDirection {
    /// Playback
    Playback,
    /// Capture
    Capture,
}

/// Sound card type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardType {
    /// Intel HDA
    IntelHda,
    /// AC'97
    Ac97,
    /// USB Audio
    UsbAudio,
    /// PCI Sound
    PciSound,
    /// Virtual
    Virtual,
}

/// Sound subsystem
pub struct SoundSubsystem {
    /// Is initialized
    initialized: AtomicBool,
    /// Sound cards
    cards: RwLock<BTreeMap<u32, Arc<RwLock<SoundCard>>>>,
    /// Next card ID
    next_card_id: AtomicU32,
    /// Default playback card
    default_playback: AtomicU32,
    /// Default capture card
    default_capture: AtomicU32,
    /// Statistics
    stats: SoundStats,
}

/// Sound statistics
#[derive(Debug, Default)]
struct SoundStats {
    /// Frames played
    frames_played: AtomicU64,
    /// Frames captured
    frames_captured: AtomicU64,
    /// Xruns (underruns + overruns)
    xruns: AtomicU64,
    /// Active streams
    active_streams: AtomicU32,
}

impl SoundSubsystem {
    /// Create new sound subsystem
    pub const fn new() -> Self {
        Self {
            initialized: AtomicBool::new(false),
            cards: RwLock::new(BTreeMap::new()),
            next_card_id: AtomicU32::new(0),
            default_playback: AtomicU32::new(0),
            default_capture: AtomicU32::new(0),
            stats: SoundStats {
                frames_played: AtomicU64::new(0),
                frames_captured: AtomicU64::new(0),
                xruns: AtomicU64::new(0),
                active_streams: AtomicU32::new(0),
            },
        }
    }

    /// Initialize sound subsystem
    pub fn init(&self) -> Result<(), SoundError> {
        // Probe for sound cards
        self.probe_pci_cards();

        self.initialized.store(true, Ordering::Release);

        let cards = self.cards.read();
        crate::kprintln!("[SOUND] Sound subsystem initialized, {} card(s)", cards.len());

        Ok(())
    }

    /// Probe PCI for sound cards
    fn probe_pci_cards(&self) {
        let pci_devices = crate::drivers::pci::get_devices();

        for (bus, slot, func, vendor, device, class, subclass, _) in pci_devices {
            // Multimedia controller class = 0x04
            if class == 0x04 {
                match subclass {
                    0x01 => {
                        // Audio device (generic)
                        self.try_init_audio_device(bus, slot, func, vendor, device);
                    }
                    0x03 => {
                        // HD Audio controller
                        if let Some(card) = hda::init_controller(bus, slot, func) {
                            self.register_card(card);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// Try to initialize audio device
    fn try_init_audio_device(&self, bus: u8, slot: u8, func: u8, vendor: u16, device: u16) {
        // Check for known AC'97 controllers
        // Intel ICH
        if vendor == 0x8086 && (device == 0x2415 || device == 0x2425 || device == 0x2445) {
            if let Some(card) = ac97::init_controller(bus, slot, func) {
                self.register_card(card);
            }
        }
    }

    /// Register sound card
    pub fn register_card(&self, card: SoundCard) -> u32 {
        let card_id = self.next_card_id.fetch_add(1, Ordering::SeqCst);
        let mut card = card;
        card.id = card_id;

        crate::kprintln!("[SOUND] Card {}: {} - {}",
            card_id, card.driver, card.name);

        self.cards.write().insert(card_id, Arc::new(RwLock::new(card)));

        // Set as default if first card
        if card_id == 0 {
            self.default_playback.store(0, Ordering::Release);
            self.default_capture.store(0, Ordering::Release);
        }

        card_id
    }

    /// Unregister sound card
    pub fn unregister_card(&self, card_id: u32) {
        self.cards.write().remove(&card_id);
    }

    /// Get sound card
    pub fn get_card(&self, card_id: u32) -> Option<Arc<RwLock<SoundCard>>> {
        self.cards.read().get(&card_id).cloned()
    }

    /// Get all cards
    pub fn get_cards(&self) -> Vec<(u32, Arc<RwLock<SoundCard>>)> {
        self.cards.read()
            .iter()
            .map(|(id, card)| (*id, card.clone()))
            .collect()
    }

    /// Get card count
    pub fn card_count(&self) -> usize {
        self.cards.read().len()
    }

    /// Get default playback card
    pub fn default_playback_card(&self) -> Option<Arc<RwLock<SoundCard>>> {
        let card_id = self.default_playback.load(Ordering::Acquire);
        self.get_card(card_id)
    }

    /// Get default capture card
    pub fn default_capture_card(&self) -> Option<Arc<RwLock<SoundCard>>> {
        let card_id = self.default_capture.load(Ordering::Acquire);
        self.get_card(card_id)
    }

    /// Set default playback card
    pub fn set_default_playback(&self, card_id: u32) -> Result<(), SoundError> {
        if self.get_card(card_id).is_some() {
            self.default_playback.store(card_id, Ordering::Release);
            Ok(())
        } else {
            Err(SoundError::NoDevice)
        }
    }

    /// Set default capture card
    pub fn set_default_capture(&self, card_id: u32) -> Result<(), SoundError> {
        if self.get_card(card_id).is_some() {
            self.default_capture.store(card_id, Ordering::Release);
            Ok(())
        } else {
            Err(SoundError::NoDevice)
        }
    }

    /// Open PCM stream
    pub fn open_pcm(
        &self,
        card_id: u32,
        device_id: u32,
        direction: StreamDirection,
    ) -> Result<Arc<Mutex<PcmStream>>, SoundError> {
        let card = self.get_card(card_id).ok_or(SoundError::NoDevice)?;
        let card_ref = card.read();

        let device = card_ref.get_device(device_id).ok_or(SoundError::NoDevice)?;

        let stream = pcm::open_stream(device, direction)?;

        self.stats.active_streams.fetch_add(1, Ordering::Relaxed);

        Ok(stream)
    }

    /// Open mixer
    pub fn open_mixer(&self, card_id: u32) -> Result<Arc<RwLock<mixer::Mixer>>, SoundError> {
        let card = self.get_card(card_id).ok_or(SoundError::NoDevice)?;
        let card_ref = card.read();

        card_ref.get_mixer().ok_or(SoundError::NoDevice)
    }

    /// Record frames played
    pub fn record_frames_played(&self, frames: u64) {
        self.stats.frames_played.fetch_add(frames, Ordering::Relaxed);
    }

    /// Record frames captured
    pub fn record_frames_captured(&self, frames: u64) {
        self.stats.frames_captured.fetch_add(frames, Ordering::Relaxed);
    }

    /// Record xrun
    pub fn record_xrun(&self) {
        self.stats.xruns.fetch_add(1, Ordering::Relaxed);
    }
}

/// Global sound subsystem
static SOUND: SoundSubsystem = SoundSubsystem::new();

/// Initialize sound subsystem
pub fn init() {
    if let Err(e) = SOUND.init() {
        crate::kprintln!("[SOUND] Initialization failed: {:?}", e);
    }
}

/// Get sound card
pub fn get_card(card_id: u32) -> Option<Arc<RwLock<SoundCard>>> {
    SOUND.get_card(card_id)
}

/// Get all cards
pub fn get_cards() -> Vec<(u32, Arc<RwLock<SoundCard>>)> {
    SOUND.get_cards()
}

/// Get card count
pub fn card_count() -> usize {
    SOUND.card_count()
}

/// Open PCM stream
pub fn open_pcm(
    card_id: u32,
    device_id: u32,
    direction: StreamDirection,
) -> Result<Arc<Mutex<PcmStream>>, SoundError> {
    SOUND.open_pcm(card_id, device_id, direction)
}

/// Open mixer
pub fn open_mixer(card_id: u32) -> Result<Arc<RwLock<mixer::Mixer>>, SoundError> {
    SOUND.open_mixer(card_id)
}

/// Register sound card
pub fn register_card(card: SoundCard) -> u32 {
    SOUND.register_card(card)
}

/// Helper to convert sample rate to period
pub fn rate_to_period_us(rate: u32) -> u64 {
    1_000_000 / rate as u64
}

/// Helper to compute buffer size in bytes
pub fn buffer_size_bytes(frames: u32, channels: u32, format: SampleFormat) -> usize {
    frames as usize * channels as usize * format.bytes_per_sample()
}

/// Helper to compute frames from bytes
pub fn bytes_to_frames(bytes: usize, channels: u32, format: SampleFormat) -> u32 {
    (bytes / (channels as usize * format.bytes_per_sample())) as u32
}
