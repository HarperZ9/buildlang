//! A2DP (Advanced Audio Distribution Profile)
//!
//! High-quality audio streaming over Bluetooth.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use spin::RwLock;

use super::l2cap::{L2capChannel, L2capManager, L2CAP_PSM_AVDTP};
use super::{BluetoothError, Uuid, uuid};

// =============================================================================
// A2DP CONSTANTS
// =============================================================================

/// A2DP source UUID (Audio Source = 0x110A)
pub const A2DP_SOURCE_UUID: Uuid = uuid::AUDIO_SOURCE;

/// A2DP sink UUID (Audio Sink = 0x110B)
pub const A2DP_SINK_UUID: Uuid = uuid::AUDIO_SINK;

// =============================================================================
// AVDTP CONSTANTS
// =============================================================================

/// AVDTP version 1.3
pub const AVDTP_VERSION: u16 = 0x0103;

/// Default L2CAP MTU
pub const AVDTP_DEFAULT_MTU: u16 = 672;

/// Maximum SEID value
pub const AVDTP_MAX_SEID: u8 = 0x3E;

// =============================================================================
// AVDTP SIGNAL COMMANDS
// =============================================================================

/// AVDTP signal identifiers
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum AvdtpSignal {
    /// Discover endpoints
    Discover = 0x01,
    /// Get endpoint capabilities
    GetCapabilities = 0x02,
    /// Set configuration
    SetConfiguration = 0x03,
    /// Get configuration
    GetConfiguration = 0x04,
    /// Reconfigure
    Reconfigure = 0x05,
    /// Open stream
    Open = 0x06,
    /// Start streaming
    Start = 0x07,
    /// Close stream
    Close = 0x08,
    /// Suspend streaming
    Suspend = 0x09,
    /// Abort stream
    Abort = 0x0A,
    /// Security control
    SecurityControl = 0x0B,
    /// Get all capabilities
    GetAllCapabilities = 0x0C,
    /// Delay report
    DelayReport = 0x0D,
}

/// AVDTP message types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum AvdtpMessageType {
    /// Command
    Command = 0x00,
    /// General reject
    GeneralReject = 0x01,
    /// Response accept
    ResponseAccept = 0x02,
    /// Response reject
    ResponseReject = 0x03,
}

/// AVDTP packet types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum AvdtpPacketType {
    /// Single packet
    Single = 0x00,
    /// Start of fragmented message
    Start = 0x01,
    /// Continuation of fragmented message
    Continue = 0x02,
    /// End of fragmented message
    End = 0x03,
}

/// AVDTP error codes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum AvdtpError {
    /// Bad header format
    BadHeaderFormat = 0x01,
    /// Bad length
    BadLength = 0x11,
    /// Bad ACP SEID
    BadAcpSeid = 0x12,
    /// SEP in use
    SepInUse = 0x13,
    /// SEP not in use
    SepNotInUse = 0x14,
    /// Bad service category
    BadServiceCategory = 0x17,
    /// Bad payload format
    BadPayloadFormat = 0x18,
    /// Not supported command
    NotSupportedCommand = 0x19,
    /// Invalid capabilities
    InvalidCapabilities = 0x1A,
    /// Bad recovery type
    BadRecoveryType = 0x22,
    /// Bad media transport format
    BadMediaTransportFormat = 0x23,
    /// Bad recovery format
    BadRecoveryFormat = 0x25,
    /// Bad ROHC format
    BadRohcFormat = 0x26,
    /// Bad CP format
    BadCpFormat = 0x27,
    /// Bad multiplexing format
    BadMultiplexingFormat = 0x28,
    /// Unsupported configuration
    UnsupportedConfiguration = 0x29,
    /// Bad state
    BadState = 0x31,
}

// =============================================================================
// SERVICE CATEGORIES
// =============================================================================

/// Service category types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ServiceCategory {
    /// Media transport
    MediaTransport = 0x01,
    /// Reporting
    Reporting = 0x02,
    /// Recovery
    Recovery = 0x03,
    /// Content protection
    ContentProtection = 0x04,
    /// Header compression
    HeaderCompression = 0x05,
    /// Multiplexing
    Multiplexing = 0x06,
    /// Media codec
    MediaCodec = 0x07,
    /// Delay reporting
    DelayReporting = 0x08,
}

// =============================================================================
// CODECS
// =============================================================================

/// Media type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MediaType {
    /// Audio
    Audio = 0x00,
    /// Video
    Video = 0x01,
    /// Multimedia
    Multimedia = 0x02,
}

/// Audio codec types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum AudioCodec {
    /// SBC (mandatory)
    Sbc = 0x00,
    /// MPEG-1,2 Audio
    Mpeg12Audio = 0x01,
    /// MPEG-2,4 AAC
    Mpeg24Aac = 0x02,
    /// ATRAC family
    Atrac = 0x04,
    /// Non-A2DP (vendor-specific)
    NonA2dp = 0xFF,
}

/// SBC configuration
#[derive(Clone, Debug)]
pub struct SbcConfig {
    /// Sampling frequency (16kHz, 32kHz, 44.1kHz, 48kHz)
    pub sampling_freq: SbcSamplingFreq,
    /// Channel mode
    pub channel_mode: SbcChannelMode,
    /// Block length
    pub block_length: SbcBlockLength,
    /// Subbands
    pub subbands: SbcSubbands,
    /// Allocation method
    pub allocation: SbcAllocation,
    /// Minimum bitpool
    pub min_bitpool: u8,
    /// Maximum bitpool
    pub max_bitpool: u8,
}

impl Default for SbcConfig {
    fn default() -> Self {
        Self {
            sampling_freq: SbcSamplingFreq::Freq44100,
            channel_mode: SbcChannelMode::JointStereo,
            block_length: SbcBlockLength::Sixteen,
            subbands: SbcSubbands::Eight,
            allocation: SbcAllocation::Loudness,
            min_bitpool: 2,
            max_bitpool: 53,
        }
    }
}

impl SbcConfig {
    /// Parse from capability bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 4 {
            return None;
        }

        let freq_mode = data[0];
        let blocks_subbands = data[1];
        let min_bitpool = data[2];
        let max_bitpool = data[3];

        Some(Self {
            sampling_freq: SbcSamplingFreq::from_byte(freq_mode >> 4),
            channel_mode: SbcChannelMode::from_byte(freq_mode & 0x0F),
            block_length: SbcBlockLength::from_byte(blocks_subbands >> 4),
            subbands: SbcSubbands::from_byte((blocks_subbands >> 2) & 0x03),
            allocation: SbcAllocation::from_byte(blocks_subbands & 0x03),
            min_bitpool,
            max_bitpool,
        })
    }

    /// Convert to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        alloc::vec![
            (self.sampling_freq.to_byte() << 4) | self.channel_mode.to_byte(),
            (self.block_length.to_byte() << 4) | (self.subbands.to_byte() << 2) | self.allocation.to_byte(),
            self.min_bitpool,
            self.max_bitpool,
        ]
    }
}

/// SBC sampling frequency
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SbcSamplingFreq {
    Freq16000 = 0x08,
    Freq32000 = 0x04,
    Freq44100 = 0x02,
    Freq48000 = 0x01,
}

impl SbcSamplingFreq {
    fn from_byte(byte: u8) -> Self {
        match byte & 0x0F {
            0x08 => Self::Freq16000,
            0x04 => Self::Freq32000,
            0x02 => Self::Freq44100,
            _ => Self::Freq48000,
        }
    }

    fn to_byte(&self) -> u8 {
        *self as u8
    }

    pub fn to_hz(&self) -> u32 {
        match self {
            Self::Freq16000 => 16000,
            Self::Freq32000 => 32000,
            Self::Freq44100 => 44100,
            Self::Freq48000 => 48000,
        }
    }
}

/// SBC channel mode
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SbcChannelMode {
    Mono = 0x08,
    DualChannel = 0x04,
    Stereo = 0x02,
    JointStereo = 0x01,
}

impl SbcChannelMode {
    fn from_byte(byte: u8) -> Self {
        match byte & 0x0F {
            0x08 => Self::Mono,
            0x04 => Self::DualChannel,
            0x02 => Self::Stereo,
            _ => Self::JointStereo,
        }
    }

    fn to_byte(&self) -> u8 {
        *self as u8
    }

    pub fn channels(&self) -> u8 {
        match self {
            Self::Mono => 1,
            _ => 2,
        }
    }
}

/// SBC block length
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SbcBlockLength {
    Four = 0x08,
    Eight = 0x04,
    Twelve = 0x02,
    Sixteen = 0x01,
}

impl SbcBlockLength {
    fn from_byte(byte: u8) -> Self {
        match byte & 0x0F {
            0x08 => Self::Four,
            0x04 => Self::Eight,
            0x02 => Self::Twelve,
            _ => Self::Sixteen,
        }
    }

    fn to_byte(&self) -> u8 {
        *self as u8
    }

    pub fn blocks(&self) -> u8 {
        match self {
            Self::Four => 4,
            Self::Eight => 8,
            Self::Twelve => 12,
            Self::Sixteen => 16,
        }
    }
}

/// SBC subbands
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SbcSubbands {
    Four = 0x02,
    Eight = 0x01,
}

impl SbcSubbands {
    fn from_byte(byte: u8) -> Self {
        if byte & 0x02 != 0 {
            Self::Four
        } else {
            Self::Eight
        }
    }

    fn to_byte(&self) -> u8 {
        *self as u8
    }

    pub fn subbands(&self) -> u8 {
        match self {
            Self::Four => 4,
            Self::Eight => 8,
        }
    }
}

/// SBC allocation method
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SbcAllocation {
    Snr = 0x02,
    Loudness = 0x01,
}

impl SbcAllocation {
    fn from_byte(byte: u8) -> Self {
        if byte & 0x02 != 0 {
            Self::Snr
        } else {
            Self::Loudness
        }
    }

    fn to_byte(&self) -> u8 {
        *self as u8
    }
}

// =============================================================================
// STREAM ENDPOINT
// =============================================================================

/// Stream endpoint type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SepType {
    /// Source (sends audio)
    Source = 0x00,
    /// Sink (receives audio)
    Sink = 0x01,
}

/// Stream endpoint state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SepState {
    /// Idle (not configured)
    Idle,
    /// Configured
    Configured,
    /// Open (ready to stream)
    Open,
    /// Streaming
    Streaming,
    /// Closing
    Closing,
    /// Aborting
    Aborting,
}

/// Stream endpoint capabilities
#[derive(Clone, Debug)]
pub struct SepCapabilities {
    /// Media transport
    pub media_transport: bool,
    /// Reporting
    pub reporting: bool,
    /// Recovery
    pub recovery: Option<RecoveryCapability>,
    /// Content protection
    pub content_protection: Option<ContentProtection>,
    /// Header compression
    pub header_compression: bool,
    /// Multiplexing
    pub multiplexing: bool,
    /// Media codec
    pub media_codec: Option<MediaCodecCapability>,
    /// Delay reporting
    pub delay_reporting: bool,
}

impl Default for SepCapabilities {
    fn default() -> Self {
        Self {
            media_transport: true,
            reporting: false,
            recovery: None,
            content_protection: None,
            header_compression: false,
            multiplexing: false,
            media_codec: None,
            delay_reporting: false,
        }
    }
}

impl SepCapabilities {
    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Self {
        let mut caps = Self::default();
        let mut offset = 0;

        while offset + 2 <= data.len() {
            let category = data[offset];
            let length = data[offset + 1] as usize;
            let cap_data = &data[offset + 2..offset + 2 + length.min(data.len() - offset - 2)];

            match category {
                0x01 => caps.media_transport = true,
                0x02 => caps.reporting = true,
                0x03 => caps.recovery = RecoveryCapability::from_bytes(cap_data),
                0x04 => caps.content_protection = ContentProtection::from_bytes(cap_data),
                0x05 => caps.header_compression = true,
                0x06 => caps.multiplexing = true,
                0x07 => caps.media_codec = MediaCodecCapability::from_bytes(cap_data),
                0x08 => caps.delay_reporting = true,
                _ => {}
            }

            offset += 2 + length;
        }

        caps
    }

    /// Convert to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        if self.media_transport {
            bytes.push(ServiceCategory::MediaTransport as u8);
            bytes.push(0); // Length
        }

        if let Some(ref codec) = self.media_codec {
            bytes.push(ServiceCategory::MediaCodec as u8);
            let codec_bytes = codec.to_bytes();
            bytes.push(codec_bytes.len() as u8);
            bytes.extend_from_slice(&codec_bytes);
        }

        if self.delay_reporting {
            bytes.push(ServiceCategory::DelayReporting as u8);
            bytes.push(0);
        }

        bytes
    }
}

/// Recovery capability
#[derive(Clone, Debug)]
pub struct RecoveryCapability {
    pub recovery_type: u8,
    pub mrws: u8,
    pub mnmp: u8,
}

impl RecoveryCapability {
    fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() >= 3 {
            Some(Self {
                recovery_type: data[0],
                mrws: data[1],
                mnmp: data[2],
            })
        } else {
            None
        }
    }
}

/// Content protection
#[derive(Clone, Debug)]
pub struct ContentProtection {
    pub cp_type: u16,
    pub cp_value: Vec<u8>,
}

impl ContentProtection {
    fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() >= 2 {
            Some(Self {
                cp_type: u16::from_le_bytes([data[0], data[1]]),
                cp_value: data[2..].to_vec(),
            })
        } else {
            None
        }
    }
}

/// Media codec capability
#[derive(Clone, Debug)]
pub struct MediaCodecCapability {
    pub media_type: MediaType,
    pub codec_type: u8,
    pub codec_info: Vec<u8>,
}

impl MediaCodecCapability {
    fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() >= 2 {
            Some(Self {
                media_type: match data[0] >> 4 {
                    0 => MediaType::Audio,
                    1 => MediaType::Video,
                    _ => MediaType::Multimedia,
                },
                codec_type: data[1],
                codec_info: data[2..].to_vec(),
            })
        } else {
            None
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(2 + self.codec_info.len());
        bytes.push((self.media_type as u8) << 4);
        bytes.push(self.codec_type);
        bytes.extend_from_slice(&self.codec_info);
        bytes
    }

    /// Get SBC config if this is SBC codec
    pub fn as_sbc(&self) -> Option<SbcConfig> {
        if self.media_type == MediaType::Audio && self.codec_type == AudioCodec::Sbc as u8 {
            SbcConfig::from_bytes(&self.codec_info)
        } else {
            None
        }
    }
}

/// Stream endpoint
pub struct StreamEndpoint {
    /// Stream Endpoint Identifier (1-62)
    pub seid: u8,
    /// Type (source/sink)
    pub sep_type: SepType,
    /// Media type
    pub media_type: MediaType,
    /// Is in use
    in_use: AtomicBool,
    /// State
    state: AtomicU8,
    /// Capabilities
    pub capabilities: RwLock<SepCapabilities>,
    /// Current configuration
    pub configuration: RwLock<Option<SepCapabilities>>,
    /// Remote SEID (when configured)
    pub remote_seid: AtomicU8,
    /// Transport channel
    pub transport: RwLock<Option<Arc<RwLock<L2capChannel>>>>,
}

impl StreamEndpoint {
    /// Create new stream endpoint
    pub fn new(seid: u8, sep_type: SepType, media_type: MediaType) -> Self {
        Self {
            seid,
            sep_type,
            media_type,
            in_use: AtomicBool::new(false),
            state: AtomicU8::new(SepState::Idle as u8),
            capabilities: RwLock::new(SepCapabilities::default()),
            configuration: RwLock::new(None),
            remote_seid: AtomicU8::new(0),
            transport: RwLock::new(None),
        }
    }

    /// Get state
    pub fn state(&self) -> SepState {
        match self.state.load(Ordering::Acquire) {
            0 => SepState::Idle,
            1 => SepState::Configured,
            2 => SepState::Open,
            3 => SepState::Streaming,
            4 => SepState::Closing,
            _ => SepState::Aborting,
        }
    }

    /// Set state
    pub fn set_state(&self, state: SepState) {
        self.state.store(state as u8, Ordering::Release);
    }

    /// Is in use
    pub fn is_in_use(&self) -> bool {
        self.in_use.load(Ordering::Acquire)
    }

    /// Set in use
    pub fn set_in_use(&self, in_use: bool) {
        self.in_use.store(in_use, Ordering::Release);
    }

    /// Set SBC capabilities
    pub fn set_sbc_capabilities(&self, config: SbcConfig) {
        let codec = MediaCodecCapability {
            media_type: MediaType::Audio,
            codec_type: AudioCodec::Sbc as u8,
            codec_info: config.to_bytes(),
        };

        let mut caps = self.capabilities.write();
        caps.media_transport = true;
        caps.media_codec = Some(codec);
    }
}

// =============================================================================
// AVDTP SESSION
// =============================================================================

/// AVDTP signal packet
#[derive(Clone, Debug)]
pub struct AvdtpPacket {
    /// Transaction label (0-15)
    pub transaction: u8,
    /// Packet type
    pub packet_type: AvdtpPacketType,
    /// Message type
    pub message_type: AvdtpMessageType,
    /// Signal identifier
    pub signal: u8,
    /// Payload
    pub payload: Vec<u8>,
}

impl AvdtpPacket {
    /// Create new packet
    pub fn new(
        transaction: u8,
        message_type: AvdtpMessageType,
        signal: AvdtpSignal,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            transaction,
            packet_type: AvdtpPacketType::Single,
            message_type,
            signal: signal as u8,
            payload,
        }
    }

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 2 {
            return None;
        }

        let transaction = (data[0] >> 4) & 0x0F;
        let packet_type = match (data[0] >> 2) & 0x03 {
            0 => AvdtpPacketType::Single,
            1 => AvdtpPacketType::Start,
            2 => AvdtpPacketType::Continue,
            _ => AvdtpPacketType::End,
        };
        let message_type = match data[0] & 0x03 {
            0 => AvdtpMessageType::Command,
            1 => AvdtpMessageType::GeneralReject,
            2 => AvdtpMessageType::ResponseAccept,
            _ => AvdtpMessageType::ResponseReject,
        };

        let signal = data[1] & 0x3F;
        let payload = if data.len() > 2 {
            data[2..].to_vec()
        } else {
            Vec::new()
        };

        Some(Self {
            transaction,
            packet_type,
            message_type,
            signal,
            payload,
        })
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(2 + self.payload.len());

        let header = ((self.transaction & 0x0F) << 4)
            | ((self.packet_type as u8) << 2)
            | (self.message_type as u8);
        bytes.push(header);
        bytes.push(self.signal & 0x3F);
        bytes.extend_from_slice(&self.payload);

        bytes
    }
}

/// AVDTP session
pub struct AvdtpSession {
    /// Signaling L2CAP channel
    signaling_channel: Arc<RwLock<L2capChannel>>,
    /// Are we initiator
    pub initiator: bool,
    /// Next transaction label
    next_transaction: AtomicU8,
    /// Local stream endpoints
    local_seps: RwLock<Vec<Arc<StreamEndpoint>>>,
    /// Remote stream endpoints
    remote_seps: RwLock<Vec<RemoteSep>>,
    /// Pending transactions
    pending: RwLock<BTreeMap<u8, PendingTransaction>>,
    /// Stream state callback
    state_callback: RwLock<Option<Box<dyn Fn(u8, SepState) + Send + Sync>>>,
}

/// Remote SEP info from discovery
#[derive(Clone, Debug)]
pub struct RemoteSep {
    pub seid: u8,
    pub in_use: bool,
    pub media_type: MediaType,
    pub sep_type: SepType,
    pub capabilities: Option<SepCapabilities>,
}

struct PendingTransaction {
    signal: u8,
    seid: Option<u8>,
}

impl AvdtpSession {
    /// Create new session
    pub fn new(signaling_channel: Arc<RwLock<L2capChannel>>, initiator: bool) -> Self {
        Self {
            signaling_channel,
            initiator,
            next_transaction: AtomicU8::new(0),
            local_seps: RwLock::new(Vec::new()),
            remote_seps: RwLock::new(Vec::new()),
            pending: RwLock::new(BTreeMap::new()),
            state_callback: RwLock::new(None),
        }
    }

    /// Allocate transaction label
    fn alloc_transaction(&self) -> u8 {
        self.next_transaction.fetch_add(1, Ordering::SeqCst) & 0x0F
    }

    /// Register local SEP
    pub fn register_sep(&self, sep: Arc<StreamEndpoint>) {
        self.local_seps.write().push(sep);
    }

    /// Discover remote endpoints
    pub fn discover(&self) -> Result<(), BluetoothError> {
        let transaction = self.alloc_transaction();
        let packet = AvdtpPacket::new(
            transaction,
            AvdtpMessageType::Command,
            AvdtpSignal::Discover,
            Vec::new(),
        );

        self.pending.write().insert(transaction, PendingTransaction {
            signal: AvdtpSignal::Discover as u8,
            seid: None,
        });

        self.send_packet(&packet)
    }

    /// Get capabilities of remote endpoint
    pub fn get_capabilities(&self, seid: u8) -> Result<(), BluetoothError> {
        let transaction = self.alloc_transaction();
        let packet = AvdtpPacket::new(
            transaction,
            AvdtpMessageType::Command,
            AvdtpSignal::GetCapabilities,
            alloc::vec![seid << 2],
        );

        self.pending.write().insert(transaction, PendingTransaction {
            signal: AvdtpSignal::GetCapabilities as u8,
            seid: Some(seid),
        });

        self.send_packet(&packet)
    }

    /// Set configuration
    pub fn set_configuration(
        &self,
        acp_seid: u8,
        int_seid: u8,
        caps: &SepCapabilities,
    ) -> Result<(), BluetoothError> {
        let transaction = self.alloc_transaction();

        let mut payload = Vec::new();
        payload.push(acp_seid << 2);
        payload.push(int_seid << 2);
        payload.extend_from_slice(&caps.to_bytes());

        let packet = AvdtpPacket::new(
            transaction,
            AvdtpMessageType::Command,
            AvdtpSignal::SetConfiguration,
            payload,
        );

        self.pending.write().insert(transaction, PendingTransaction {
            signal: AvdtpSignal::SetConfiguration as u8,
            seid: Some(acp_seid),
        });

        self.send_packet(&packet)
    }

    /// Open stream
    pub fn open(&self, seid: u8) -> Result<(), BluetoothError> {
        let transaction = self.alloc_transaction();
        let packet = AvdtpPacket::new(
            transaction,
            AvdtpMessageType::Command,
            AvdtpSignal::Open,
            alloc::vec![seid << 2],
        );

        self.pending.write().insert(transaction, PendingTransaction {
            signal: AvdtpSignal::Open as u8,
            seid: Some(seid),
        });

        self.send_packet(&packet)
    }

    /// Start streaming
    pub fn start(&self, seid: u8) -> Result<(), BluetoothError> {
        let transaction = self.alloc_transaction();
        let packet = AvdtpPacket::new(
            transaction,
            AvdtpMessageType::Command,
            AvdtpSignal::Start,
            alloc::vec![seid << 2],
        );

        self.pending.write().insert(transaction, PendingTransaction {
            signal: AvdtpSignal::Start as u8,
            seid: Some(seid),
        });

        self.send_packet(&packet)
    }

    /// Suspend streaming
    pub fn suspend(&self, seid: u8) -> Result<(), BluetoothError> {
        let transaction = self.alloc_transaction();
        let packet = AvdtpPacket::new(
            transaction,
            AvdtpMessageType::Command,
            AvdtpSignal::Suspend,
            alloc::vec![seid << 2],
        );

        self.pending.write().insert(transaction, PendingTransaction {
            signal: AvdtpSignal::Suspend as u8,
            seid: Some(seid),
        });

        self.send_packet(&packet)
    }

    /// Close stream
    pub fn close(&self, seid: u8) -> Result<(), BluetoothError> {
        let transaction = self.alloc_transaction();
        let packet = AvdtpPacket::new(
            transaction,
            AvdtpMessageType::Command,
            AvdtpSignal::Close,
            alloc::vec![seid << 2],
        );

        self.pending.write().insert(transaction, PendingTransaction {
            signal: AvdtpSignal::Close as u8,
            seid: Some(seid),
        });

        self.send_packet(&packet)
    }

    /// Abort stream
    pub fn abort(&self, seid: u8) -> Result<(), BluetoothError> {
        let transaction = self.alloc_transaction();
        let packet = AvdtpPacket::new(
            transaction,
            AvdtpMessageType::Command,
            AvdtpSignal::Abort,
            alloc::vec![seid << 2],
        );

        self.send_packet(&packet)
    }

    /// Send delay report
    pub fn delay_report(&self, seid: u8, delay: u16) -> Result<(), BluetoothError> {
        let transaction = self.alloc_transaction();
        let mut payload = Vec::with_capacity(3);
        payload.push(seid << 2);
        payload.extend_from_slice(&delay.to_be_bytes());

        let packet = AvdtpPacket::new(
            transaction,
            AvdtpMessageType::Command,
            AvdtpSignal::DelayReport,
            payload,
        );

        self.send_packet(&packet)
    }

    /// Send packet
    fn send_packet(&self, packet: &AvdtpPacket) -> Result<(), BluetoothError> {
        let data = packet.to_bytes();
        self.signaling_channel.read().send(data)
    }

    /// Send response
    fn send_response(&self, transaction: u8, signal: u8, accept: bool, payload: Vec<u8>) -> Result<(), BluetoothError> {
        let message_type = if accept {
            AvdtpMessageType::ResponseAccept
        } else {
            AvdtpMessageType::ResponseReject
        };

        let packet = AvdtpPacket {
            transaction,
            packet_type: AvdtpPacketType::Single,
            message_type,
            signal,
            payload,
        };

        self.send_packet(&packet)
    }

    /// Handle incoming signaling data
    pub fn handle_signaling(&self, data: &[u8]) {
        if let Some(packet) = AvdtpPacket::from_bytes(data) {
            match packet.message_type {
                AvdtpMessageType::Command => self.handle_command(&packet),
                AvdtpMessageType::ResponseAccept => self.handle_response_accept(&packet),
                AvdtpMessageType::ResponseReject => self.handle_response_reject(&packet),
                AvdtpMessageType::GeneralReject => self.handle_general_reject(&packet),
            }
        }
    }

    fn handle_command(&self, packet: &AvdtpPacket) {
        match packet.signal {
            x if x == AvdtpSignal::Discover as u8 => self.handle_discover_cmd(packet),
            x if x == AvdtpSignal::GetCapabilities as u8 => self.handle_get_caps_cmd(packet),
            x if x == AvdtpSignal::GetAllCapabilities as u8 => self.handle_get_caps_cmd(packet),
            x if x == AvdtpSignal::SetConfiguration as u8 => self.handle_set_config_cmd(packet),
            x if x == AvdtpSignal::Open as u8 => self.handle_open_cmd(packet),
            x if x == AvdtpSignal::Start as u8 => self.handle_start_cmd(packet),
            x if x == AvdtpSignal::Suspend as u8 => self.handle_suspend_cmd(packet),
            x if x == AvdtpSignal::Close as u8 => self.handle_close_cmd(packet),
            x if x == AvdtpSignal::Abort as u8 => self.handle_abort_cmd(packet),
            x if x == AvdtpSignal::DelayReport as u8 => self.handle_delay_report_cmd(packet),
            _ => {
                // Send general reject
                let _ = self.send_response(packet.transaction, packet.signal, false, Vec::new());
            }
        }
    }

    fn handle_discover_cmd(&self, packet: &AvdtpPacket) {
        let mut response = Vec::new();

        for sep in self.local_seps.read().iter() {
            let info = ((sep.seid & 0x3F) << 2) | (if sep.is_in_use() { 0x02 } else { 0 });
            let type_media = ((sep.media_type as u8) << 4) | ((sep.sep_type as u8) << 3);
            response.push(info);
            response.push(type_media);
        }

        let _ = self.send_response(packet.transaction, packet.signal, true, response);
    }

    fn handle_get_caps_cmd(&self, packet: &AvdtpPacket) {
        if packet.payload.is_empty() {
            let _ = self.send_response(packet.transaction, packet.signal, false, alloc::vec![AvdtpError::BadAcpSeid as u8]);
            return;
        }

        let seid = packet.payload[0] >> 2;

        let caps = self.local_seps.read()
            .iter()
            .find(|s| s.seid == seid)
            .map(|s| s.capabilities.read().to_bytes());

        if let Some(caps_data) = caps {
            let _ = self.send_response(packet.transaction, packet.signal, true, caps_data);
        } else {
            let _ = self.send_response(packet.transaction, packet.signal, false, alloc::vec![AvdtpError::BadAcpSeid as u8]);
        }
    }

    fn handle_set_config_cmd(&self, packet: &AvdtpPacket) {
        if packet.payload.len() < 4 {
            let _ = self.send_response(packet.transaction, packet.signal, false, alloc::vec![AvdtpError::BadLength as u8]);
            return;
        }

        let acp_seid = packet.payload[0] >> 2;
        let int_seid = packet.payload[1] >> 2;
        let caps = SepCapabilities::from_bytes(&packet.payload[2..]);

        // Find local SEP
        let sep = self.local_seps.read()
            .iter()
            .find(|s| s.seid == acp_seid)
            .cloned();

        if let Some(sep) = sep {
            if sep.is_in_use() {
                let _ = self.send_response(packet.transaction, packet.signal, false, alloc::vec![AvdtpError::SepInUse as u8]);
                return;
            }

            // Configure
            sep.set_in_use(true);
            sep.remote_seid.store(int_seid, Ordering::Release);
            *sep.configuration.write() = Some(caps);
            sep.set_state(SepState::Configured);

            let _ = self.send_response(packet.transaction, packet.signal, true, Vec::new());
        } else {
            let _ = self.send_response(packet.transaction, packet.signal, false, alloc::vec![AvdtpError::BadAcpSeid as u8]);
        }
    }

    fn handle_open_cmd(&self, packet: &AvdtpPacket) {
        if packet.payload.is_empty() {
            let _ = self.send_response(packet.transaction, packet.signal, false, alloc::vec![AvdtpError::BadAcpSeid as u8]);
            return;
        }

        let seid = packet.payload[0] >> 2;

        let sep = self.local_seps.read()
            .iter()
            .find(|s| s.seid == seid)
            .cloned();

        if let Some(sep) = sep {
            if sep.state() != SepState::Configured {
                let _ = self.send_response(packet.transaction, packet.signal, false, alloc::vec![AvdtpError::BadState as u8]);
                return;
            }

            sep.set_state(SepState::Open);
            let _ = self.send_response(packet.transaction, packet.signal, true, Vec::new());

            if let Some(callback) = self.state_callback.read().as_ref() {
                callback(seid, SepState::Open);
            }
        } else {
            let _ = self.send_response(packet.transaction, packet.signal, false, alloc::vec![AvdtpError::BadAcpSeid as u8]);
        }
    }

    fn handle_start_cmd(&self, packet: &AvdtpPacket) {
        for &seid_byte in &packet.payload {
            let seid = seid_byte >> 2;

            if let Some(sep) = self.local_seps.read().iter().find(|s| s.seid == seid).cloned() {
                if sep.state() == SepState::Open {
                    sep.set_state(SepState::Streaming);

                    if let Some(callback) = self.state_callback.read().as_ref() {
                        callback(seid, SepState::Streaming);
                    }
                }
            }
        }

        let _ = self.send_response(packet.transaction, packet.signal, true, Vec::new());
    }

    fn handle_suspend_cmd(&self, packet: &AvdtpPacket) {
        for &seid_byte in &packet.payload {
            let seid = seid_byte >> 2;

            if let Some(sep) = self.local_seps.read().iter().find(|s| s.seid == seid).cloned() {
                if sep.state() == SepState::Streaming {
                    sep.set_state(SepState::Open);

                    if let Some(callback) = self.state_callback.read().as_ref() {
                        callback(seid, SepState::Open);
                    }
                }
            }
        }

        let _ = self.send_response(packet.transaction, packet.signal, true, Vec::new());
    }

    fn handle_close_cmd(&self, packet: &AvdtpPacket) {
        if packet.payload.is_empty() {
            return;
        }

        let seid = packet.payload[0] >> 2;

        if let Some(sep) = self.local_seps.read().iter().find(|s| s.seid == seid).cloned() {
            sep.set_in_use(false);
            sep.set_state(SepState::Idle);
            *sep.configuration.write() = None;

            if let Some(callback) = self.state_callback.read().as_ref() {
                callback(seid, SepState::Idle);
            }
        }

        let _ = self.send_response(packet.transaction, packet.signal, true, Vec::new());
    }

    fn handle_abort_cmd(&self, packet: &AvdtpPacket) {
        if packet.payload.is_empty() {
            return;
        }

        let seid = packet.payload[0] >> 2;

        if let Some(sep) = self.local_seps.read().iter().find(|s| s.seid == seid).cloned() {
            sep.set_in_use(false);
            sep.set_state(SepState::Idle);
            *sep.configuration.write() = None;
        }

        let _ = self.send_response(packet.transaction, packet.signal, true, Vec::new());
    }

    fn handle_delay_report_cmd(&self, packet: &AvdtpPacket) {
        // Accept delay report
        let _ = self.send_response(packet.transaction, packet.signal, true, Vec::new());
    }

    fn handle_response_accept(&self, packet: &AvdtpPacket) {
        let pending = self.pending.write().remove(&packet.transaction);

        if let Some(pending) = pending {
            match pending.signal {
                x if x == AvdtpSignal::Discover as u8 => {
                    // Parse discovered SEPs
                    let mut seps = Vec::new();
                    for chunk in packet.payload.chunks(2) {
                        if chunk.len() >= 2 {
                            let seid = chunk[0] >> 2;
                            let in_use = (chunk[0] & 0x02) != 0;
                            let media_type = match chunk[1] >> 4 {
                                0 => MediaType::Audio,
                                1 => MediaType::Video,
                                _ => MediaType::Multimedia,
                            };
                            let sep_type = if (chunk[1] & 0x08) != 0 {
                                SepType::Sink
                            } else {
                                SepType::Source
                            };

                            seps.push(RemoteSep {
                                seid,
                                in_use,
                                media_type,
                                sep_type,
                                capabilities: None,
                            });
                        }
                    }
                    *self.remote_seps.write() = seps;
                }
                x if x == AvdtpSignal::GetCapabilities as u8 => {
                    if let Some(seid) = pending.seid {
                        let caps = SepCapabilities::from_bytes(&packet.payload);
                        if let Some(sep) = self.remote_seps.write().iter_mut().find(|s| s.seid == seid) {
                            sep.capabilities = Some(caps);
                        }
                    }
                }
                x if x == AvdtpSignal::SetConfiguration as u8 => {
                    if let Some(seid) = pending.seid {
                        if let Some(sep) = self.local_seps.read().iter().find(|s| s.seid == seid).cloned() {
                            sep.set_state(SepState::Configured);
                        }
                    }
                }
                x if x == AvdtpSignal::Open as u8 => {
                    if let Some(seid) = pending.seid {
                        if let Some(sep) = self.local_seps.read().iter().find(|s| s.seid == seid).cloned() {
                            sep.set_state(SepState::Open);
                        }
                    }
                }
                x if x == AvdtpSignal::Start as u8 => {
                    if let Some(seid) = pending.seid {
                        if let Some(sep) = self.local_seps.read().iter().find(|s| s.seid == seid).cloned() {
                            sep.set_state(SepState::Streaming);
                        }
                    }
                }
                x if x == AvdtpSignal::Suspend as u8 => {
                    if let Some(seid) = pending.seid {
                        if let Some(sep) = self.local_seps.read().iter().find(|s| s.seid == seid).cloned() {
                            sep.set_state(SepState::Open);
                        }
                    }
                }
                x if x == AvdtpSignal::Close as u8 => {
                    if let Some(seid) = pending.seid {
                        if let Some(sep) = self.local_seps.read().iter().find(|s| s.seid == seid).cloned() {
                            sep.set_in_use(false);
                            sep.set_state(SepState::Idle);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn handle_response_reject(&self, packet: &AvdtpPacket) {
        self.pending.write().remove(&packet.transaction);
        // Error handling would go here
    }

    fn handle_general_reject(&self, _packet: &AvdtpPacket) {
        // General reject handling
    }

    /// Set state change callback
    pub fn set_state_callback<F>(&self, callback: F)
    where
        F: Fn(u8, SepState) + Send + Sync + 'static,
    {
        *self.state_callback.write() = Some(Box::new(callback));
    }

    /// Get remote SEPs
    pub fn remote_seps(&self) -> Vec<RemoteSep> {
        self.remote_seps.read().clone()
    }
}

// =============================================================================
// A2DP SOURCE/SINK
// =============================================================================

/// A2DP role
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum A2dpRole {
    Source,
    Sink,
}

/// A2DP connection state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum A2dpState {
    Disconnected,
    Connecting,
    Connected,
    Streaming,
}

/// A2DP endpoint
pub struct A2dpEndpoint {
    /// L2CAP manager
    l2cap: Arc<RwLock<L2capManager>>,
    /// Role
    pub role: A2dpRole,
    /// AVDTP session
    session: RwLock<Option<Arc<RwLock<AvdtpSession>>>>,
    /// Local SEP
    local_sep: Arc<StreamEndpoint>,
    /// State
    state: AtomicU8,
    /// SBC config
    sbc_config: RwLock<SbcConfig>,
    /// Audio callback
    audio_callback: RwLock<Option<Box<dyn Fn(&[u8]) + Send + Sync>>>,
}

impl A2dpEndpoint {
    /// Create new A2DP source
    pub fn new_source(l2cap: Arc<RwLock<L2capManager>>, seid: u8) -> Self {
        let sep = Arc::new(StreamEndpoint::new(seid, SepType::Source, MediaType::Audio));
        sep.set_sbc_capabilities(SbcConfig::default());

        Self {
            l2cap,
            role: A2dpRole::Source,
            session: RwLock::new(None),
            local_sep: sep,
            state: AtomicU8::new(A2dpState::Disconnected as u8),
            sbc_config: RwLock::new(SbcConfig::default()),
            audio_callback: RwLock::new(None),
        }
    }

    /// Create new A2DP sink
    pub fn new_sink(l2cap: Arc<RwLock<L2capManager>>, seid: u8) -> Self {
        let sep = Arc::new(StreamEndpoint::new(seid, SepType::Sink, MediaType::Audio));
        sep.set_sbc_capabilities(SbcConfig::default());

        Self {
            l2cap,
            role: A2dpRole::Sink,
            session: RwLock::new(None),
            local_sep: sep,
            state: AtomicU8::new(A2dpState::Disconnected as u8),
            sbc_config: RwLock::new(SbcConfig::default()),
            audio_callback: RwLock::new(None),
        }
    }

    /// Get state
    pub fn state(&self) -> A2dpState {
        match self.state.load(Ordering::Acquire) {
            0 => A2dpState::Disconnected,
            1 => A2dpState::Connecting,
            2 => A2dpState::Connected,
            _ => A2dpState::Streaming,
        }
    }

    /// Set audio callback
    pub fn set_audio_callback<F>(&self, callback: F)
    where
        F: Fn(&[u8]) + Send + Sync + 'static,
    {
        *self.audio_callback.write() = Some(Box::new(callback));
    }

    /// Connect to remote device
    pub fn connect(&self, handle: u16) -> Result<(), BluetoothError> {
        self.state.store(A2dpState::Connecting as u8, Ordering::Release);

        // Create L2CAP connection
        let l2cap_channel = self.l2cap.read().connect(handle, L2CAP_PSM_AVDTP)?;

        // Create AVDTP session
        let session = Arc::new(RwLock::new(AvdtpSession::new(l2cap_channel.clone(), true)));
        session.read().register_sep(self.local_sep.clone());

        *self.session.write() = Some(session.clone());

        // Set up data handler
        let session_clone = session.clone();
        l2cap_channel.read().set_rx_callback(move |data| {
            session_clone.read().handle_signaling(data);
        });

        // Discover remote endpoints
        session.read().discover()?;

        self.state.store(A2dpState::Connected as u8, Ordering::Release);

        Ok(())
    }

    /// Start streaming
    pub fn start_stream(&self) -> Result<(), BluetoothError> {
        let session = self.session.read().clone().ok_or(BluetoothError::NotConnected)?;

        // Find compatible remote SEP
        let remote_seps = session.read().remote_seps();
        let target_type = if self.role == A2dpRole::Source {
            SepType::Sink
        } else {
            SepType::Source
        };

        let remote_sep = remote_seps
            .iter()
            .find(|s| s.sep_type == target_type && s.media_type == MediaType::Audio && !s.in_use)
            .ok_or(BluetoothError::NotFound)?;

        let remote_seid = remote_sep.seid;

        // Set configuration
        let caps = self.local_sep.capabilities.read().clone();
        session.read().set_configuration(remote_seid, self.local_sep.seid, &caps)?;

        // Open and start
        session.read().open(remote_seid)?;
        session.read().start(remote_seid)?;

        self.state.store(A2dpState::Streaming as u8, Ordering::Release);

        Ok(())
    }

    /// Stop streaming
    pub fn stop_stream(&self) -> Result<(), BluetoothError> {
        let session = self.session.read().clone().ok_or(BluetoothError::NotConnected)?;

        if let Some(remote_seid) = session.read().remote_seps().first().map(|s| s.seid) {
            session.read().suspend(remote_seid)?;
        }

        self.state.store(A2dpState::Connected as u8, Ordering::Release);

        Ok(())
    }

    /// Disconnect
    pub fn disconnect(&self) -> Result<(), BluetoothError> {
        if let Some(session) = self.session.write().take() {
            let seps = session.read().remote_seps();
            if let Some(sep) = seps.first() {
                let _ = session.read().close(sep.seid);
            }
        }

        self.state.store(A2dpState::Disconnected as u8, Ordering::Release);

        Ok(())
    }

    /// Send audio data (source only)
    pub fn send_audio(&self, _data: &[u8]) -> Result<(), BluetoothError> {
        if self.role != A2dpRole::Source {
            return Err(BluetoothError::NotSupported);
        }

        if self.state() != A2dpState::Streaming {
            return Err(BluetoothError::NotConnected);
        }

        // Would encode to SBC and send via media transport channel
        Ok(())
    }

    /// Handle received audio (sink only)
    pub fn receive_audio(&self, data: &[u8]) {
        if self.role == A2dpRole::Sink {
            if let Some(callback) = self.audio_callback.read().as_ref() {
                // Would decode SBC first
                callback(data);
            }
        }
    }

    /// Get current SBC configuration
    pub fn sbc_config(&self) -> SbcConfig {
        self.sbc_config.read().clone()
    }
}
