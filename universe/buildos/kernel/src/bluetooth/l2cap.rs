//! L2CAP (Logical Link Control and Adaptation Protocol)
//!
//! Provides protocol multiplexing, segmentation and reassembly for Bluetooth.

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU16, AtomicU32, AtomicU8, Ordering};
use spin::{Mutex, RwLock};

use super::hci::{AclPacket, AclPacketBoundary, HciController};
use super::BluetoothError;

// =============================================================================
// L2CAP CONSTANTS
// =============================================================================

/// Minimum L2CAP MTU
pub const L2CAP_MIN_MTU: u16 = 48;

/// Default L2CAP MTU
pub const L2CAP_DEFAULT_MTU: u16 = 672;

/// Maximum L2CAP MTU
pub const L2CAP_MAX_MTU: u16 = 65535;

/// L2CAP header length
pub const L2CAP_HDR_LEN: usize = 4;

/// L2CAP signaling MTU
pub const L2CAP_SIG_MTU: u16 = 48;

// =============================================================================
// L2CAP CIDs (Channel Identifiers)
// =============================================================================

/// Null CID
pub const L2CAP_CID_NULL: u16 = 0x0000;

/// Signaling channel (BR/EDR)
pub const L2CAP_CID_SIGNALING: u16 = 0x0001;

/// Connectionless reception
pub const L2CAP_CID_CONNLESS: u16 = 0x0002;

/// AMP Manager protocol
pub const L2CAP_CID_AMP_MANAGER: u16 = 0x0003;

/// Attribute protocol (ATT)
pub const L2CAP_CID_ATT: u16 = 0x0004;

/// LE signaling channel
pub const L2CAP_CID_LE_SIGNALING: u16 = 0x0005;

/// Security Manager Protocol
pub const L2CAP_CID_SMP: u16 = 0x0006;

/// BR/EDR Security Manager
pub const L2CAP_CID_SMP_BREDR: u16 = 0x0007;

/// First dynamically allocated CID
pub const L2CAP_CID_DYN_START: u16 = 0x0040;

/// Last dynamically allocated CID
pub const L2CAP_CID_DYN_END: u16 = 0xFFFF;

/// LE first dynamically allocated CID
pub const L2CAP_CID_LE_DYN_START: u16 = 0x0040;

/// LE last dynamically allocated CID
pub const L2CAP_CID_LE_DYN_END: u16 = 0x007F;

// =============================================================================
// L2CAP PSMs (Protocol/Service Multiplexers)
// =============================================================================

/// Service Discovery Protocol
pub const L2CAP_PSM_SDP: u16 = 0x0001;

/// RFCOMM
pub const L2CAP_PSM_RFCOMM: u16 = 0x0003;

/// TCS-BIN (Telephony Control Specification)
pub const L2CAP_PSM_TCS: u16 = 0x0005;

/// TCS-BIN-CORDLESS
pub const L2CAP_PSM_TCS_CORDLESS: u16 = 0x0007;

/// BNEP (Bluetooth Network Encapsulation Protocol)
pub const L2CAP_PSM_BNEP: u16 = 0x000F;

/// HID Control
pub const L2CAP_PSM_HID_CONTROL: u16 = 0x0011;

/// HID Interrupt
pub const L2CAP_PSM_HID_INTERRUPT: u16 = 0x0013;

/// AVCTP (Audio/Video Control Transport Protocol)
pub const L2CAP_PSM_AVCTP: u16 = 0x0017;

/// AVDTP (Audio/Video Distribution Transport Protocol)
pub const L2CAP_PSM_AVDTP: u16 = 0x0019;

/// AVCTP Browse
pub const L2CAP_PSM_AVCTP_BROWSE: u16 = 0x001B;

/// ATT (Attribute Protocol)
pub const L2CAP_PSM_ATT: u16 = 0x001F;

/// 3DSP (3D Synchronization Profile)
pub const L2CAP_PSM_3DSP: u16 = 0x0021;

/// LE PSM IPSP (Internet Protocol Support Profile)
pub const L2CAP_PSM_IPSP: u16 = 0x0023;

/// Object Transfer Service
pub const L2CAP_PSM_OTS: u16 = 0x0025;

// Aliases without L2CAP_ prefix for compatibility
pub const PSM_SDP: u16 = L2CAP_PSM_SDP;
pub const PSM_RFCOMM: u16 = L2CAP_PSM_RFCOMM;
pub const PSM_HID_CONTROL: u16 = L2CAP_PSM_HID_CONTROL;
pub const PSM_HID_INTERRUPT: u16 = L2CAP_PSM_HID_INTERRUPT;
pub const PSM_AVDTP: u16 = L2CAP_PSM_AVDTP;
pub const PSM_AVCTP: u16 = L2CAP_PSM_AVCTP;

// =============================================================================
// L2CAP SIGNALING COMMANDS
// =============================================================================

/// L2CAP signaling command codes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum L2capSigCode {
    /// Command reject
    CommandReject = 0x01,
    /// Connection request
    ConnectionRequest = 0x02,
    /// Connection response
    ConnectionResponse = 0x03,
    /// Configuration request
    ConfigurationRequest = 0x04,
    /// Configuration response
    ConfigurationResponse = 0x05,
    /// Disconnection request
    DisconnectionRequest = 0x06,
    /// Disconnection response
    DisconnectionResponse = 0x07,
    /// Echo request
    EchoRequest = 0x08,
    /// Echo response
    EchoResponse = 0x09,
    /// Information request
    InformationRequest = 0x0A,
    /// Information response
    InformationResponse = 0x0B,
    /// Create channel request
    CreateChannelRequest = 0x0C,
    /// Create channel response
    CreateChannelResponse = 0x0D,
    /// Move channel request
    MoveChannelRequest = 0x0E,
    /// Move channel response
    MoveChannelResponse = 0x0F,
    /// Move channel confirmation
    MoveChannelConfirmation = 0x10,
    /// Move channel confirmation response
    MoveChannelConfirmationResponse = 0x11,
    /// Connection parameter update request
    ConnParamUpdateRequest = 0x12,
    /// Connection parameter update response
    ConnParamUpdateResponse = 0x13,
    /// LE credit based connection request
    LeCreditBasedConnRequest = 0x14,
    /// LE credit based connection response
    LeCreditBasedConnResponse = 0x15,
    /// Flow control credit
    FlowControlCredit = 0x16,
    /// Credit based connection request
    CreditBasedConnRequest = 0x17,
    /// Credit based connection response
    CreditBasedConnResponse = 0x18,
    /// Credit based reconfigure request
    CreditBasedReconfigRequest = 0x19,
    /// Credit based reconfigure response
    CreditBasedReconfigResponse = 0x1A,
}

/// Connection response results
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum L2capConnResult {
    /// Connection successful
    Success = 0x0000,
    /// Connection pending
    Pending = 0x0001,
    /// PSM not supported
    PsmNotSupported = 0x0002,
    /// Security block
    SecurityBlock = 0x0003,
    /// No resources available
    NoResources = 0x0004,
    /// Invalid source CID
    InvalidSourceCid = 0x0006,
    /// Source CID already allocated
    SourceCidAlreadyAllocated = 0x0007,
}

/// Configuration response results
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum L2capConfResult {
    /// Success
    Success = 0x0000,
    /// Unacceptable parameters
    Unacceptable = 0x0001,
    /// Rejected
    Rejected = 0x0002,
    /// Unknown options
    Unknown = 0x0003,
    /// Pending
    Pending = 0x0004,
    /// Flow spec rejected
    FlowSpecRejected = 0x0005,
}

/// Information request types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum L2capInfoType {
    /// Connectionless MTU
    ConnectionlessMtu = 0x0001,
    /// Extended features mask
    ExtendedFeatures = 0x0002,
    /// Fixed channels supported
    FixedChannels = 0x0003,
}

// =============================================================================
// L2CAP PACKETS
// =============================================================================

/// L2CAP packet header
#[derive(Clone, Copy, Debug)]
pub struct L2capHeader {
    /// Length (excluding header)
    pub length: u16,
    /// Channel ID
    pub cid: u16,
}

impl L2capHeader {
    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 4 {
            return None;
        }
        Some(Self {
            length: u16::from_le_bytes([data[0], data[1]]),
            cid: u16::from_le_bytes([data[2], data[3]]),
        })
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> [u8; 4] {
        let mut bytes = [0u8; 4];
        bytes[0..2].copy_from_slice(&self.length.to_le_bytes());
        bytes[2..4].copy_from_slice(&self.cid.to_le_bytes());
        bytes
    }
}

/// L2CAP packet
#[derive(Clone, Debug)]
pub struct L2capPacket {
    /// Header
    pub header: L2capHeader,
    /// Payload
    pub payload: Vec<u8>,
}

impl L2capPacket {
    /// Create new L2CAP packet
    pub fn new(cid: u16, payload: Vec<u8>) -> Self {
        Self {
            header: L2capHeader {
                length: payload.len() as u16,
                cid,
            },
            payload,
        }
    }

    /// Parse from ACL data
    pub fn from_acl_data(data: &[u8]) -> Option<Self> {
        let header = L2capHeader::from_bytes(data)?;
        if data.len() < 4 + header.length as usize {
            return None;
        }
        Some(Self {
            header,
            payload: data[4..4 + header.length as usize].to_vec(),
        })
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(4 + self.payload.len());
        bytes.extend_from_slice(&self.header.to_bytes());
        bytes.extend_from_slice(&self.payload);
        bytes
    }
}

/// L2CAP signaling packet
#[derive(Clone, Debug)]
pub struct L2capSignal {
    /// Command code
    pub code: u8,
    /// Identifier
    pub id: u8,
    /// Data
    pub data: Vec<u8>,
}

impl L2capSignal {
    /// Create new signaling packet
    pub fn new(code: L2capSigCode, id: u8, data: Vec<u8>) -> Self {
        Self {
            code: code as u8,
            id,
            data,
        }
    }

    /// Parse from L2CAP payload
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 4 {
            return None;
        }
        let code = data[0];
        let id = data[1];
        let length = u16::from_le_bytes([data[2], data[3]]) as usize;
        if data.len() < 4 + length {
            return None;
        }
        Some(Self {
            code,
            id,
            data: data[4..4 + length].to_vec(),
        })
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(4 + self.data.len());
        bytes.push(self.code);
        bytes.push(self.id);
        bytes.extend_from_slice(&(self.data.len() as u16).to_le_bytes());
        bytes.extend_from_slice(&self.data);
        bytes
    }

    /// Create connection request
    pub fn connection_request(id: u8, psm: u16, source_cid: u16) -> Self {
        let mut data = Vec::with_capacity(4);
        data.extend_from_slice(&psm.to_le_bytes());
        data.extend_from_slice(&source_cid.to_le_bytes());
        Self::new(L2capSigCode::ConnectionRequest, id, data)
    }

    /// Create connection response
    pub fn connection_response(id: u8, dest_cid: u16, source_cid: u16, result: L2capConnResult, status: u16) -> Self {
        let mut data = Vec::with_capacity(8);
        data.extend_from_slice(&dest_cid.to_le_bytes());
        data.extend_from_slice(&source_cid.to_le_bytes());
        data.extend_from_slice(&(result as u16).to_le_bytes());
        data.extend_from_slice(&status.to_le_bytes());
        Self::new(L2capSigCode::ConnectionResponse, id, data)
    }

    /// Create configuration request
    pub fn configuration_request(id: u8, dest_cid: u16, flags: u16, options: &[u8]) -> Self {
        let mut data = Vec::with_capacity(4 + options.len());
        data.extend_from_slice(&dest_cid.to_le_bytes());
        data.extend_from_slice(&flags.to_le_bytes());
        data.extend_from_slice(options);
        Self::new(L2capSigCode::ConfigurationRequest, id, data)
    }

    /// Create configuration response
    pub fn configuration_response(id: u8, source_cid: u16, flags: u16, result: L2capConfResult, options: &[u8]) -> Self {
        let mut data = Vec::with_capacity(6 + options.len());
        data.extend_from_slice(&source_cid.to_le_bytes());
        data.extend_from_slice(&flags.to_le_bytes());
        data.extend_from_slice(&(result as u16).to_le_bytes());
        data.extend_from_slice(options);
        Self::new(L2capSigCode::ConfigurationResponse, id, data)
    }

    /// Create disconnection request
    pub fn disconnection_request(id: u8, dest_cid: u16, source_cid: u16) -> Self {
        let mut data = Vec::with_capacity(4);
        data.extend_from_slice(&dest_cid.to_le_bytes());
        data.extend_from_slice(&source_cid.to_le_bytes());
        Self::new(L2capSigCode::DisconnectionRequest, id, data)
    }

    /// Create disconnection response
    pub fn disconnection_response(id: u8, dest_cid: u16, source_cid: u16) -> Self {
        let mut data = Vec::with_capacity(4);
        data.extend_from_slice(&dest_cid.to_le_bytes());
        data.extend_from_slice(&source_cid.to_le_bytes());
        Self::new(L2capSigCode::DisconnectionResponse, id, data)
    }

    /// Create information request
    pub fn information_request(id: u8, info_type: L2capInfoType) -> Self {
        let mut data = Vec::with_capacity(2);
        data.extend_from_slice(&(info_type as u16).to_le_bytes());
        Self::new(L2capSigCode::InformationRequest, id, data)
    }

    /// Create command reject
    pub fn command_reject(id: u8, reason: u16, data: Vec<u8>) -> Self {
        let mut payload = Vec::with_capacity(2 + data.len());
        payload.extend_from_slice(&reason.to_le_bytes());
        payload.extend_from_slice(&data);
        Self::new(L2capSigCode::CommandReject, id, payload)
    }

    /// Create LE connection parameter update request
    pub fn le_conn_param_update_request(id: u8, min_interval: u16, max_interval: u16, latency: u16, timeout: u16) -> Self {
        let mut data = Vec::with_capacity(8);
        data.extend_from_slice(&min_interval.to_le_bytes());
        data.extend_from_slice(&max_interval.to_le_bytes());
        data.extend_from_slice(&latency.to_le_bytes());
        data.extend_from_slice(&timeout.to_le_bytes());
        Self::new(L2capSigCode::ConnParamUpdateRequest, id, data)
    }

    /// Create LE credit based connection request
    pub fn le_credit_conn_request(id: u8, psm: u16, source_cid: u16, mtu: u16, mps: u16, initial_credits: u16) -> Self {
        let mut data = Vec::with_capacity(10);
        data.extend_from_slice(&psm.to_le_bytes());
        data.extend_from_slice(&source_cid.to_le_bytes());
        data.extend_from_slice(&mtu.to_le_bytes());
        data.extend_from_slice(&mps.to_le_bytes());
        data.extend_from_slice(&initial_credits.to_le_bytes());
        Self::new(L2capSigCode::LeCreditBasedConnRequest, id, data)
    }
}

// =============================================================================
// L2CAP CHANNEL
// =============================================================================

/// L2CAP channel mode
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum L2capMode {
    /// Basic mode (default)
    Basic,
    /// Retransmission mode
    Retransmission,
    /// Flow control mode
    FlowControl,
    /// Enhanced retransmission mode
    EnhancedRetransmission,
    /// Streaming mode
    Streaming,
    /// LE credit-based flow control
    LeCreditBased,
    /// Enhanced credit-based flow control
    EnhancedCreditBased,
}

/// L2CAP channel state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum L2capState {
    /// Closed
    Closed,
    /// Wait connection confirm
    WaitConnect,
    /// Wait connection confirm from peer
    WaitConnectRsp,
    /// Config (both sides)
    Config,
    /// Open
    Open,
    /// Wait disconnection confirm
    WaitDisconnect,
}

/// L2CAP channel configuration
#[derive(Clone, Debug)]
pub struct L2capConfig {
    /// Local MTU
    pub mtu: u16,
    /// Flush timeout
    pub flush_timeout: u16,
    /// Quality of service
    pub qos: Option<L2capQos>,
    /// Retransmission and flow control
    pub rfc: Option<L2capRfc>,
    /// Frame check sequence
    pub fcs: u8,
    /// Extended flow specification
    pub efs: Option<L2capEfs>,
    /// Extended window size
    pub ext_window: u16,
}

impl Default for L2capConfig {
    fn default() -> Self {
        Self {
            mtu: L2CAP_DEFAULT_MTU,
            flush_timeout: 0xFFFF,
            qos: None,
            rfc: None,
            fcs: 1,
            efs: None,
            ext_window: 0,
        }
    }
}

/// L2CAP QoS configuration
#[derive(Clone, Debug)]
pub struct L2capQos {
    /// Service type
    pub service_type: u8,
    /// Token rate
    pub token_rate: u32,
    /// Token bucket size
    pub token_bucket_size: u32,
    /// Peak bandwidth
    pub peak_bandwidth: u32,
    /// Latency
    pub latency: u32,
    /// Delay variation
    pub delay_variation: u32,
}

/// L2CAP retransmission and flow control
#[derive(Clone, Debug)]
pub struct L2capRfc {
    /// Mode
    pub mode: L2capMode,
    /// TxWindow size
    pub tx_window: u8,
    /// Max transmit
    pub max_transmit: u8,
    /// Retransmission timeout
    pub retrans_timeout: u16,
    /// Monitor timeout
    pub monitor_timeout: u16,
    /// Maximum PDU size
    pub max_pdu_size: u16,
}

/// L2CAP extended flow specification
#[derive(Clone, Debug)]
pub struct L2capEfs {
    /// Identifier
    pub id: u8,
    /// Service type
    pub service_type: u8,
    /// Maximum SDU size
    pub max_sdu_size: u16,
    /// SDU inter-arrival time
    pub sdu_itime: u32,
    /// Access latency
    pub access_latency: u32,
    /// Flush timeout
    pub flush_timeout: u32,
}

/// L2CAP channel
pub struct L2capChannel {
    /// Local CID
    pub local_cid: u16,
    /// Remote CID
    pub remote_cid: u16,
    /// Connection handle
    pub handle: u16,
    /// PSM
    pub psm: u16,
    /// State
    state: AtomicU8,
    /// Mode
    pub mode: L2capMode,
    /// Local configuration
    pub local_config: RwLock<L2capConfig>,
    /// Remote configuration
    pub remote_config: RwLock<L2capConfig>,
    /// Pending identifier
    pending_id: AtomicU8,
    /// TX credits (for credit-based modes)
    tx_credits: AtomicU16,
    /// RX credits (for credit-based modes)
    rx_credits: AtomicU16,
    /// TX buffer
    tx_buffer: Mutex<Vec<Vec<u8>>>,
    /// RX buffer
    rx_buffer: Mutex<Vec<Vec<u8>>>,
    /// Callback for received data
    rx_callback: RwLock<Option<Box<dyn Fn(&[u8]) + Send + Sync>>>,
    /// Is LE channel
    pub is_le: bool,
}

impl L2capChannel {
    /// Create new channel
    pub fn new(local_cid: u16, handle: u16, psm: u16, is_le: bool) -> Self {
        Self {
            local_cid,
            remote_cid: 0,
            handle,
            psm,
            state: AtomicU8::new(L2capState::Closed as u8),
            mode: L2capMode::Basic,
            local_config: RwLock::new(L2capConfig::default()),
            remote_config: RwLock::new(L2capConfig::default()),
            pending_id: AtomicU8::new(0),
            tx_credits: AtomicU16::new(0),
            rx_credits: AtomicU16::new(0),
            tx_buffer: Mutex::new(Vec::new()),
            rx_buffer: Mutex::new(Vec::new()),
            rx_callback: RwLock::new(None),
            is_le,
        }
    }

    /// Get state
    pub fn state(&self) -> L2capState {
        match self.state.load(Ordering::Acquire) {
            0 => L2capState::Closed,
            1 => L2capState::WaitConnect,
            2 => L2capState::WaitConnectRsp,
            3 => L2capState::Config,
            4 => L2capState::Open,
            _ => L2capState::WaitDisconnect,
        }
    }

    /// Set state
    pub fn set_state(&self, state: L2capState) {
        self.state.store(state as u8, Ordering::Release);
    }

    /// Is connected
    pub fn is_connected(&self) -> bool {
        self.state() == L2capState::Open
    }

    /// Set remote CID
    pub fn set_remote_cid(&self, _cid: u16) {
        // Note: In real implementation, this would need interior mutability
    }

    /// Add TX credits
    pub fn add_tx_credits(&self, credits: u16) {
        self.tx_credits.fetch_add(credits, Ordering::SeqCst);
    }

    /// Consume TX credit
    pub fn consume_tx_credit(&self) -> bool {
        loop {
            let current = self.tx_credits.load(Ordering::Acquire);
            if current == 0 {
                return false;
            }
            if self.tx_credits.compare_exchange(
                current,
                current - 1,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ).is_ok() {
                return true;
            }
        }
    }

    /// Add RX credits
    pub fn add_rx_credits(&self, credits: u16) {
        self.rx_credits.fetch_add(credits, Ordering::SeqCst);
    }

    /// Get MTU
    pub fn mtu(&self) -> u16 {
        self.local_config.read().mtu
    }

    /// Set receive callback
    pub fn set_rx_callback<F>(&self, callback: F)
    where
        F: Fn(&[u8]) + Send + Sync + 'static,
    {
        *self.rx_callback.write() = Some(Box::new(callback));
    }

    /// Handle received data
    pub fn receive(&self, data: &[u8]) {
        if let Some(callback) = self.rx_callback.read().as_ref() {
            callback(data);
        } else {
            self.rx_buffer.lock().push(data.to_vec());
        }
    }

    /// Queue data for transmission
    pub fn send(&self, data: Vec<u8>) -> Result<(), BluetoothError> {
        if self.state() != L2capState::Open {
            return Err(BluetoothError::NotConnected);
        }
        self.tx_buffer.lock().push(data);
        Ok(())
    }

    /// Get pending TX data
    pub fn pending_tx(&self) -> Option<Vec<u8>> {
        self.tx_buffer.lock().pop()
    }

    /// Get pending RX data
    pub fn pending_rx(&self) -> Option<Vec<u8>> {
        self.rx_buffer.lock().pop()
    }

    /// Disconnect channel
    pub fn disconnect(&self) -> Result<(), BluetoothError> {
        self.set_state(L2capState::Closed);
        Ok(())
    }
}

// =============================================================================
// L2CAP MANAGER
// =============================================================================

/// L2CAP manager
pub struct L2capManager {
    /// HCI controller reference
    hci: Arc<RwLock<HciController>>,
    /// Channels by local CID
    channels: RwLock<BTreeMap<u16, Arc<RwLock<L2capChannel>>>>,
    /// Fixed channels by (handle, CID)
    fixed_channels: RwLock<BTreeMap<(u16, u16), Arc<RwLock<L2capChannel>>>>,
    /// Next local CID
    next_cid: AtomicU16,
    /// Next signal identifier
    next_sig_id: AtomicU8,
    /// PSM handlers
    psm_handlers: RwLock<BTreeMap<u16, Box<dyn Fn(Arc<RwLock<L2capChannel>>) + Send + Sync>>>,
    /// Extended features
    pub extended_features: AtomicU32,
    /// Fixed channels mask
    pub fixed_channels_mask: AtomicU32,
    /// Segmentation buffers (handle -> partial packet)
    segment_buffers: RwLock<BTreeMap<u16, Vec<u8>>>,
}

impl L2capManager {
    /// Create new L2CAP manager
    pub fn new(hci: Arc<RwLock<HciController>>) -> Self {
        Self {
            hci,
            channels: RwLock::new(BTreeMap::new()),
            fixed_channels: RwLock::new(BTreeMap::new()),
            next_cid: AtomicU16::new(L2CAP_CID_DYN_START),
            next_sig_id: AtomicU8::new(1),
            psm_handlers: RwLock::new(BTreeMap::new()),
            extended_features: AtomicU32::new(0),
            fixed_channels_mask: AtomicU32::new(
                (1 << L2CAP_CID_SIGNALING) | (1 << L2CAP_CID_CONNLESS)
            ),
            segment_buffers: RwLock::new(BTreeMap::new()),
        }
    }

    /// Allocate local CID
    fn alloc_cid(&self) -> u16 {
        self.next_cid.fetch_add(1, Ordering::SeqCst)
    }

    /// Allocate signal identifier
    fn alloc_sig_id(&self) -> u8 {
        let id = self.next_sig_id.fetch_add(1, Ordering::SeqCst);
        if id == 0 {
            self.next_sig_id.store(1, Ordering::Release);
            1
        } else {
            id
        }
    }

    /// Register PSM handler
    pub fn register_psm<F>(&self, psm: u16, handler: F)
    where
        F: Fn(Arc<RwLock<L2capChannel>>) + Send + Sync + 'static,
    {
        self.psm_handlers.write().insert(psm, Box::new(handler));
    }

    /// Create channel
    pub fn create_channel(&self, handle: u16, psm: u16, is_le: bool) -> Arc<RwLock<L2capChannel>> {
        let cid = self.alloc_cid();
        let channel = Arc::new(RwLock::new(L2capChannel::new(cid, handle, psm, is_le)));
        self.channels.write().insert(cid, channel.clone());
        channel
    }

    /// Get channel by local CID
    pub fn get_channel(&self, cid: u16) -> Option<Arc<RwLock<L2capChannel>>> {
        self.channels.read().get(&cid).cloned()
    }

    /// Get fixed channel
    pub fn get_fixed_channel(&self, handle: u16, cid: u16) -> Option<Arc<RwLock<L2capChannel>>> {
        self.fixed_channels.read().get(&(handle, cid)).cloned()
    }

    /// Create fixed channel
    pub fn create_fixed_channel(&self, handle: u16, cid: u16) -> Arc<RwLock<L2capChannel>> {
        let channel = Arc::new(RwLock::new(L2capChannel::new(cid, handle, 0, cid >= L2CAP_CID_ATT)));
        channel.write().set_state(L2capState::Open);
        self.fixed_channels.write().insert((handle, cid), channel.clone());
        channel
    }

    /// Remove channel
    pub fn remove_channel(&self, cid: u16) {
        self.channels.write().remove(&cid);
    }

    /// Connect to PSM
    pub fn connect(&self, handle: u16, psm: u16) -> Result<Arc<RwLock<L2capChannel>>, BluetoothError> {
        let channel = self.create_channel(handle, psm, false);
        let local_cid = channel.read().local_cid;

        // Set state
        channel.write().set_state(L2capState::WaitConnectRsp);

        // Send connection request
        let sig_id = self.alloc_sig_id();
        let signal = L2capSignal::connection_request(sig_id, psm, local_cid);
        self.send_signal(handle, &signal)?;

        Ok(channel)
    }

    /// Connect LE credit-based
    pub fn le_connect(&self, handle: u16, psm: u16, mtu: u16, mps: u16, credits: u16) -> Result<Arc<RwLock<L2capChannel>>, BluetoothError> {
        let channel = self.create_channel(handle, psm, true);
        let local_cid = channel.read().local_cid;

        // Update config
        channel.write().local_config.write().mtu = mtu;

        // Set state
        channel.write().set_state(L2capState::WaitConnectRsp);

        // Send LE credit-based connection request
        let sig_id = self.alloc_sig_id();
        let signal = L2capSignal::le_credit_conn_request(sig_id, psm, local_cid, mtu, mps, credits);
        self.send_le_signal(handle, &signal)?;

        Ok(channel)
    }

    /// Disconnect channel
    pub fn disconnect(&self, cid: u16) -> Result<(), BluetoothError> {
        let channel = self.get_channel(cid).ok_or(BluetoothError::NotFound)?;
        let channel = channel.read();

        if channel.state() != L2capState::Open {
            return Err(BluetoothError::NotConnected);
        }

        // Send disconnection request
        let sig_id = self.alloc_sig_id();
        let signal = L2capSignal::disconnection_request(sig_id, channel.remote_cid, cid);

        if channel.is_le {
            self.send_le_signal(channel.handle, &signal)?;
        } else {
            self.send_signal(channel.handle, &signal)?;
        }

        Ok(())
    }

    /// Send data on channel
    pub fn send(&self, cid: u16, data: &[u8]) -> Result<(), BluetoothError> {
        let channel = self.get_channel(cid).ok_or(BluetoothError::NotFound)?;
        let channel = channel.read();

        if channel.state() != L2capState::Open {
            return Err(BluetoothError::NotConnected);
        }

        // Create L2CAP packet
        let packet = L2capPacket::new(channel.remote_cid, data.to_vec());
        self.send_l2cap(channel.handle, &packet)
    }

    /// Send L2CAP packet
    fn send_l2cap(&self, handle: u16, packet: &L2capPacket) -> Result<(), BluetoothError> {
        let data = packet.to_bytes();
        let acl = AclPacket::new(handle, AclPacketBoundary::FirstAutoFlush, data);
        self.hci.read().send_acl(&acl)
    }

    /// Send signaling packet
    fn send_signal(&self, handle: u16, signal: &L2capSignal) -> Result<(), BluetoothError> {
        let packet = L2capPacket::new(L2CAP_CID_SIGNALING, signal.to_bytes());
        self.send_l2cap(handle, &packet)
    }

    /// Send LE signaling packet
    fn send_le_signal(&self, handle: u16, signal: &L2capSignal) -> Result<(), BluetoothError> {
        let packet = L2capPacket::new(L2CAP_CID_LE_SIGNALING, signal.to_bytes());
        self.send_l2cap(handle, &packet)
    }

    /// Handle incoming ACL packet
    pub fn handle_acl(&self, acl: &AclPacket) {
        match acl.pb_flag {
            AclPacketBoundary::FirstAutoFlush | AclPacketBoundary::FirstNonFlushable => {
                // Start of new packet
                if let Some(l2cap) = L2capPacket::from_acl_data(&acl.data) {
                    // Check if complete
                    if l2cap.payload.len() >= l2cap.header.length as usize {
                        self.handle_l2cap(acl.handle, &l2cap);
                    } else {
                        // Store for reassembly
                        self.segment_buffers.write().insert(acl.handle, acl.data.clone());
                    }
                }
            }
            AclPacketBoundary::Continuing | AclPacketBoundary::Complete => {
                // Continuation fragment
                if let Some(buffer) = self.segment_buffers.write().get_mut(&acl.handle) {
                    buffer.extend_from_slice(&acl.data);

                    // Check if complete
                    if let Some(l2cap) = L2capPacket::from_acl_data(buffer) {
                        if l2cap.payload.len() >= l2cap.header.length as usize {
                            self.handle_l2cap(acl.handle, &l2cap);
                            self.segment_buffers.write().remove(&acl.handle);
                        }
                    }
                }
            }
        }
    }

    /// Handle L2CAP packet
    fn handle_l2cap(&self, handle: u16, packet: &L2capPacket) {
        match packet.header.cid {
            L2CAP_CID_SIGNALING => {
                self.handle_signaling(handle, &packet.payload, false);
            }
            L2CAP_CID_LE_SIGNALING => {
                self.handle_signaling(handle, &packet.payload, true);
            }
            L2CAP_CID_ATT => {
                // Forward to ATT layer
                if let Some(channel) = self.get_fixed_channel(handle, L2CAP_CID_ATT) {
                    channel.read().receive(&packet.payload);
                }
            }
            L2CAP_CID_SMP => {
                // Forward to SMP layer
                if let Some(channel) = self.get_fixed_channel(handle, L2CAP_CID_SMP) {
                    channel.read().receive(&packet.payload);
                }
            }
            cid if cid >= L2CAP_CID_DYN_START => {
                // Dynamic channel
                if let Some(channel) = self.get_channel(cid) {
                    channel.read().receive(&packet.payload);
                }
            }
            _ => {
                // Unknown CID
            }
        }
    }

    /// Handle signaling packet
    fn handle_signaling(&self, handle: u16, data: &[u8], is_le: bool) {
        if let Some(signal) = L2capSignal::from_bytes(data) {
            match signal.code {
                x if x == L2capSigCode::ConnectionRequest as u8 => {
                    self.handle_connection_request(handle, &signal);
                }
                x if x == L2capSigCode::ConnectionResponse as u8 => {
                    self.handle_connection_response(handle, &signal);
                }
                x if x == L2capSigCode::ConfigurationRequest as u8 => {
                    self.handle_configuration_request(handle, &signal);
                }
                x if x == L2capSigCode::ConfigurationResponse as u8 => {
                    self.handle_configuration_response(handle, &signal);
                }
                x if x == L2capSigCode::DisconnectionRequest as u8 => {
                    self.handle_disconnection_request(handle, &signal);
                }
                x if x == L2capSigCode::DisconnectionResponse as u8 => {
                    self.handle_disconnection_response(handle, &signal);
                }
                x if x == L2capSigCode::InformationRequest as u8 => {
                    self.handle_information_request(handle, &signal);
                }
                x if x == L2capSigCode::LeCreditBasedConnRequest as u8 => {
                    self.handle_le_credit_conn_request(handle, &signal);
                }
                x if x == L2capSigCode::LeCreditBasedConnResponse as u8 => {
                    self.handle_le_credit_conn_response(handle, &signal);
                }
                x if x == L2capSigCode::FlowControlCredit as u8 => {
                    self.handle_flow_control_credit(handle, &signal);
                }
                x if x == L2capSigCode::ConnParamUpdateRequest as u8 => {
                    self.handle_conn_param_update_request(handle, &signal);
                }
                _ => {
                    // Unknown command - send reject
                    let reject = L2capSignal::command_reject(signal.id, 0x0000, Vec::new());
                    if is_le {
                        let _ = self.send_le_signal(handle, &reject);
                    } else {
                        let _ = self.send_signal(handle, &reject);
                    }
                }
            }
        }
    }

    /// Handle connection request
    fn handle_connection_request(&self, handle: u16, signal: &L2capSignal) {
        if signal.data.len() < 4 {
            return;
        }

        let psm = u16::from_le_bytes([signal.data[0], signal.data[1]]);
        let source_cid = u16::from_le_bytes([signal.data[2], signal.data[3]]);

        // Check if PSM is registered
        let has_handler = self.psm_handlers.read().contains_key(&psm);

        if has_handler {
            // Create channel
            let channel = self.create_channel(handle, psm, false);
            {
                let mut ch = channel.write();
                ch.remote_cid = source_cid;
                ch.set_state(L2capState::Config);
            }
            let local_cid = channel.read().local_cid;

            // Send success response
            let response = L2capSignal::connection_response(
                signal.id,
                local_cid,
                source_cid,
                L2capConnResult::Success,
                0,
            );
            let _ = self.send_signal(handle, &response);

            // Notify handler
            if let Some(handler) = self.psm_handlers.read().get(&psm) {
                handler(channel);
            }
        } else {
            // PSM not supported
            let response = L2capSignal::connection_response(
                signal.id,
                0,
                source_cid,
                L2capConnResult::PsmNotSupported,
                0,
            );
            let _ = self.send_signal(handle, &response);
        }
    }

    /// Handle connection response
    fn handle_connection_response(&self, _handle: u16, signal: &L2capSignal) {
        if signal.data.len() < 8 {
            return;
        }

        let dest_cid = u16::from_le_bytes([signal.data[0], signal.data[1]]);
        let source_cid = u16::from_le_bytes([signal.data[2], signal.data[3]]);
        let result = u16::from_le_bytes([signal.data[4], signal.data[5]]);

        if let Some(channel) = self.get_channel(source_cid) {
            let mut ch = channel.write();
            if result == L2capConnResult::Success as u16 {
                ch.remote_cid = dest_cid;
                ch.set_state(L2capState::Config);
            } else {
                ch.set_state(L2capState::Closed);
            }
        }
    }

    /// Handle configuration request
    fn handle_configuration_request(&self, handle: u16, signal: &L2capSignal) {
        if signal.data.len() < 4 {
            return;
        }

        let dest_cid = u16::from_le_bytes([signal.data[0], signal.data[1]]);

        if let Some(channel) = self.get_channel(dest_cid) {
            // Parse and apply options
            // For now, just accept default config

            let response = L2capSignal::configuration_response(
                signal.id,
                channel.read().remote_cid,
                0,
                L2capConfResult::Success,
                &[],
            );
            let _ = self.send_signal(handle, &response);

            // If both sides configured, open channel
            channel.write().set_state(L2capState::Open);
        }
    }

    /// Handle configuration response
    fn handle_configuration_response(&self, _handle: u16, signal: &L2capSignal) {
        if signal.data.len() < 6 {
            return;
        }

        let source_cid = u16::from_le_bytes([signal.data[0], signal.data[1]]);
        let result = u16::from_le_bytes([signal.data[4], signal.data[5]]);

        if let Some(channel) = self.get_channel(source_cid) {
            if result == L2capConfResult::Success as u16 {
                // Configuration successful - channel may now be open
                if channel.read().state() == L2capState::Config {
                    channel.write().set_state(L2capState::Open);
                }
            }
        }
    }

    /// Handle disconnection request
    fn handle_disconnection_request(&self, handle: u16, signal: &L2capSignal) {
        if signal.data.len() < 4 {
            return;
        }

        let dest_cid = u16::from_le_bytes([signal.data[0], signal.data[1]]);
        let source_cid = u16::from_le_bytes([signal.data[2], signal.data[3]]);

        // Send response
        let response = L2capSignal::disconnection_response(signal.id, dest_cid, source_cid);
        let _ = self.send_signal(handle, &response);

        // Remove channel
        self.remove_channel(dest_cid);
    }

    /// Handle disconnection response
    fn handle_disconnection_response(&self, _handle: u16, signal: &L2capSignal) {
        if signal.data.len() < 4 {
            return;
        }

        let _dest_cid = u16::from_le_bytes([signal.data[0], signal.data[1]]);
        let source_cid = u16::from_le_bytes([signal.data[2], signal.data[3]]);

        // Remove channel
        self.remove_channel(source_cid);
    }

    /// Handle information request
    fn handle_information_request(&self, handle: u16, signal: &L2capSignal) {
        if signal.data.len() < 2 {
            return;
        }

        let info_type = u16::from_le_bytes([signal.data[0], signal.data[1]]);

        let mut response_data = Vec::new();
        response_data.extend_from_slice(&info_type.to_le_bytes());

        match info_type {
            1 => {
                // Connectionless MTU
                response_data.extend_from_slice(&0_u16.to_le_bytes()); // Success
                response_data.extend_from_slice(&L2CAP_DEFAULT_MTU.to_le_bytes());
            }
            2 => {
                // Extended features
                response_data.extend_from_slice(&0_u16.to_le_bytes()); // Success
                let features = self.extended_features.load(Ordering::Acquire);
                response_data.extend_from_slice(&features.to_le_bytes());
            }
            3 => {
                // Fixed channels
                response_data.extend_from_slice(&0_u16.to_le_bytes()); // Success
                let channels = self.fixed_channels_mask.load(Ordering::Acquire) as u64;
                response_data.extend_from_slice(&channels.to_le_bytes());
            }
            _ => {
                // Not supported
                response_data.extend_from_slice(&1_u16.to_le_bytes()); // Not supported
            }
        }

        let response = L2capSignal {
            code: L2capSigCode::InformationResponse as u8,
            id: signal.id,
            data: response_data,
        };
        let _ = self.send_signal(handle, &response);
    }

    /// Handle LE credit based connection request
    fn handle_le_credit_conn_request(&self, handle: u16, signal: &L2capSignal) {
        if signal.data.len() < 10 {
            return;
        }

        let psm = u16::from_le_bytes([signal.data[0], signal.data[1]]);
        let source_cid = u16::from_le_bytes([signal.data[2], signal.data[3]]);
        let mtu = u16::from_le_bytes([signal.data[4], signal.data[5]]);
        let mps = u16::from_le_bytes([signal.data[6], signal.data[7]]);
        let credits = u16::from_le_bytes([signal.data[8], signal.data[9]]);

        // Check if PSM is registered
        let has_handler = self.psm_handlers.read().contains_key(&psm);

        if has_handler {
            // Create channel
            let channel = self.create_channel(handle, psm, true);
            {
                let mut ch = channel.write();
                ch.remote_cid = source_cid;
                ch.remote_config.write().mtu = mtu;
                ch.add_tx_credits(credits);
                ch.set_state(L2capState::Open);
            }
            let local_cid = channel.read().local_cid;
            let local_mtu = channel.read().local_config.read().mtu;

            // Send success response
            let mut response_data = Vec::with_capacity(10);
            response_data.extend_from_slice(&local_cid.to_le_bytes());
            response_data.extend_from_slice(&local_mtu.to_le_bytes());
            response_data.extend_from_slice(&mps.to_le_bytes());
            response_data.extend_from_slice(&10_u16.to_le_bytes()); // Initial credits
            response_data.extend_from_slice(&0_u16.to_le_bytes()); // Success

            let response = L2capSignal {
                code: L2capSigCode::LeCreditBasedConnResponse as u8,
                id: signal.id,
                data: response_data,
            };
            let _ = self.send_le_signal(handle, &response);

            // Notify handler
            if let Some(handler) = self.psm_handlers.read().get(&psm) {
                handler(channel);
            }
        } else {
            // PSM not supported
            let mut response_data = Vec::with_capacity(10);
            response_data.extend_from_slice(&0_u16.to_le_bytes()); // No CID
            response_data.extend_from_slice(&0_u16.to_le_bytes());
            response_data.extend_from_slice(&0_u16.to_le_bytes());
            response_data.extend_from_slice(&0_u16.to_le_bytes());
            response_data.extend_from_slice(&2_u16.to_le_bytes()); // PSM not supported

            let response = L2capSignal {
                code: L2capSigCode::LeCreditBasedConnResponse as u8,
                id: signal.id,
                data: response_data,
            };
            let _ = self.send_le_signal(handle, &response);
        }
    }

    /// Handle LE credit based connection response
    fn handle_le_credit_conn_response(&self, _handle: u16, signal: &L2capSignal) {
        if signal.data.len() < 10 {
            return;
        }

        let dest_cid = u16::from_le_bytes([signal.data[0], signal.data[1]]);
        let mtu = u16::from_le_bytes([signal.data[2], signal.data[3]]);
        let _mps = u16::from_le_bytes([signal.data[4], signal.data[5]]);
        let credits = u16::from_le_bytes([signal.data[6], signal.data[7]]);
        let result = u16::from_le_bytes([signal.data[8], signal.data[9]]);

        // Find pending channel by state
        for (_, channel) in self.channels.read().iter() {
            let mut ch = channel.write();
            if ch.state() == L2capState::WaitConnectRsp && ch.is_le {
                if result == 0 {
                    ch.remote_cid = dest_cid;
                    ch.remote_config.write().mtu = mtu;
                    ch.add_tx_credits(credits);
                    ch.set_state(L2capState::Open);
                } else {
                    ch.set_state(L2capState::Closed);
                }
                break;
            }
        }
    }

    /// Handle flow control credit
    fn handle_flow_control_credit(&self, _handle: u16, signal: &L2capSignal) {
        if signal.data.len() < 4 {
            return;
        }

        let cid = u16::from_le_bytes([signal.data[0], signal.data[1]]);
        let credits = u16::from_le_bytes([signal.data[2], signal.data[3]]);

        if let Some(channel) = self.get_channel(cid) {
            channel.read().add_tx_credits(credits);
        }
    }

    /// Handle connection parameter update request
    fn handle_conn_param_update_request(&self, handle: u16, signal: &L2capSignal) {
        // Accept the request (in real implementation, validate and apply)
        let mut response_data = Vec::with_capacity(2);
        response_data.extend_from_slice(&0_u16.to_le_bytes()); // Accepted

        let response = L2capSignal {
            code: L2capSigCode::ConnParamUpdateResponse as u8,
            id: signal.id,
            data: response_data,
        };
        let _ = self.send_le_signal(handle, &response);
    }
}

// =============================================================================
// CONFIGURATION OPTIONS
// =============================================================================

/// L2CAP configuration option types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum L2capConfOptType {
    /// MTU option
    Mtu = 0x01,
    /// Flush timeout option
    FlushTimeout = 0x02,
    /// QoS option
    Qos = 0x03,
    /// Retransmission and flow control option
    Rfc = 0x04,
    /// Frame check sequence option
    Fcs = 0x05,
    /// Extended flow specification option
    Efs = 0x06,
    /// Extended window size option
    ExtWindow = 0x07,
}

/// Parse configuration options
pub fn parse_config_options(data: &[u8]) -> Vec<(L2capConfOptType, Vec<u8>)> {
    let mut options = Vec::new();
    let mut offset = 0;

    while offset + 2 <= data.len() {
        let opt_type = data[offset];
        let opt_len = data[offset + 1] as usize;

        if offset + 2 + opt_len > data.len() {
            break;
        }

        let opt_data = data[offset + 2..offset + 2 + opt_len].to_vec();

        match opt_type {
            0x01 => options.push((L2capConfOptType::Mtu, opt_data)),
            0x02 => options.push((L2capConfOptType::FlushTimeout, opt_data)),
            0x03 => options.push((L2capConfOptType::Qos, opt_data)),
            0x04 => options.push((L2capConfOptType::Rfc, opt_data)),
            0x05 => options.push((L2capConfOptType::Fcs, opt_data)),
            0x06 => options.push((L2capConfOptType::Efs, opt_data)),
            0x07 => options.push((L2capConfOptType::ExtWindow, opt_data)),
            _ => {}
        }

        offset += 2 + opt_len;
    }

    options
}

/// Build configuration option
pub fn build_config_option(opt_type: L2capConfOptType, data: &[u8]) -> Vec<u8> {
    let mut opt = Vec::with_capacity(2 + data.len());
    opt.push(opt_type as u8);
    opt.push(data.len() as u8);
    opt.extend_from_slice(data);
    opt
}

/// Build MTU option
pub fn build_mtu_option(mtu: u16) -> Vec<u8> {
    build_config_option(L2capConfOptType::Mtu, &mtu.to_le_bytes())
}

/// Build flush timeout option
pub fn build_flush_timeout_option(timeout: u16) -> Vec<u8> {
    build_config_option(L2capConfOptType::FlushTimeout, &timeout.to_le_bytes())
}

/// Build FCS option
pub fn build_fcs_option(fcs: u8) -> Vec<u8> {
    build_config_option(L2capConfOptType::Fcs, &[fcs])
}
