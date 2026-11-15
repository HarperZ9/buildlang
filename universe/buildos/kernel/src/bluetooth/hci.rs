//! HCI (Host Controller Interface)
//!
//! Core Bluetooth controller communication protocol.

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU16, AtomicU8, Ordering};
use spin::{Mutex, RwLock};

use super::{AddressType, BdAddr, BluetoothError};

// =============================================================================
// HCI PACKET TYPES
// =============================================================================

/// HCI packet type indicators
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum HciPacketType {
    /// Command packet (host to controller)
    Command = 0x01,
    /// ACL data packet (bidirectional)
    AclData = 0x02,
    /// SCO data packet (bidirectional)
    ScoData = 0x03,
    /// Event packet (controller to host)
    Event = 0x04,
    /// ISO data packet (LE audio)
    IsoData = 0x05,
}

// =============================================================================
// HCI OPCODES
// =============================================================================

/// HCI command opcode groups (OGF)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum HciOgf {
    /// Link control commands
    LinkControl = 0x01,
    /// Link policy commands
    LinkPolicy = 0x02,
    /// Controller & baseband commands
    ControllerBaseband = 0x03,
    /// Informational parameters
    InfoParams = 0x04,
    /// Status parameters
    StatusParams = 0x05,
    /// Testing commands
    Testing = 0x06,
    /// LE controller commands
    LeController = 0x08,
    /// Vendor specific commands
    VendorSpecific = 0x3F,
}

/// Create HCI opcode from OGF and OCF
pub const fn hci_opcode(ogf: u8, ocf: u16) -> u16 {
    ((ogf as u16) << 10) | (ocf & 0x3FF)
}

/// Extract OGF from opcode
pub const fn hci_ogf(opcode: u16) -> u8 {
    ((opcode >> 10) & 0x3F) as u8
}

/// Extract OCF from opcode
pub const fn hci_ocf(opcode: u16) -> u16 {
    opcode & 0x3FF
}

// Link Control Commands (OGF 0x01)
pub const HCI_OP_INQUIRY: u16 = hci_opcode(0x01, 0x0001);
pub const HCI_OP_INQUIRY_CANCEL: u16 = hci_opcode(0x01, 0x0002);
pub const HCI_OP_CREATE_CONN: u16 = hci_opcode(0x01, 0x0005);
pub const HCI_OP_DISCONNECT: u16 = hci_opcode(0x01, 0x0006);
pub const HCI_OP_CREATE_CONN_CANCEL: u16 = hci_opcode(0x01, 0x0008);
pub const HCI_OP_ACCEPT_CONN_REQ: u16 = hci_opcode(0x01, 0x0009);
pub const HCI_OP_REJECT_CONN_REQ: u16 = hci_opcode(0x01, 0x000A);
pub const HCI_OP_LINK_KEY_REPLY: u16 = hci_opcode(0x01, 0x000B);
pub const HCI_OP_LINK_KEY_NEG_REPLY: u16 = hci_opcode(0x01, 0x000C);
pub const HCI_OP_PIN_CODE_REPLY: u16 = hci_opcode(0x01, 0x000D);
pub const HCI_OP_PIN_CODE_NEG_REPLY: u16 = hci_opcode(0x01, 0x000E);
pub const HCI_OP_AUTH_REQUESTED: u16 = hci_opcode(0x01, 0x0011);
pub const HCI_OP_SET_CONN_ENCRYPT: u16 = hci_opcode(0x01, 0x0013);
pub const HCI_OP_REMOTE_NAME_REQ: u16 = hci_opcode(0x01, 0x0019);
pub const HCI_OP_REMOTE_NAME_REQ_CANCEL: u16 = hci_opcode(0x01, 0x001A);
pub const HCI_OP_READ_REMOTE_FEATURES: u16 = hci_opcode(0x01, 0x001B);
pub const HCI_OP_READ_REMOTE_EXT_FEATURES: u16 = hci_opcode(0x01, 0x001C);
pub const HCI_OP_READ_REMOTE_VERSION: u16 = hci_opcode(0x01, 0x001D);
pub const HCI_OP_IO_CAPABILITY_REPLY: u16 = hci_opcode(0x01, 0x002B);
pub const HCI_OP_USER_CONFIRM_REPLY: u16 = hci_opcode(0x01, 0x002C);
pub const HCI_OP_USER_CONFIRM_NEG_REPLY: u16 = hci_opcode(0x01, 0x002D);
pub const HCI_OP_USER_PASSKEY_REPLY: u16 = hci_opcode(0x01, 0x002E);
pub const HCI_OP_USER_PASSKEY_NEG_REPLY: u16 = hci_opcode(0x01, 0x002F);
pub const HCI_OP_IO_CAPABILITY_NEG_REPLY: u16 = hci_opcode(0x01, 0x0034);

// Link Policy Commands (OGF 0x02)
pub const HCI_OP_HOLD_MODE: u16 = hci_opcode(0x02, 0x0001);
pub const HCI_OP_SNIFF_MODE: u16 = hci_opcode(0x02, 0x0003);
pub const HCI_OP_EXIT_SNIFF_MODE: u16 = hci_opcode(0x02, 0x0004);
pub const HCI_OP_QOS_SETUP: u16 = hci_opcode(0x02, 0x0007);
pub const HCI_OP_ROLE_DISCOVERY: u16 = hci_opcode(0x02, 0x0009);
pub const HCI_OP_SWITCH_ROLE: u16 = hci_opcode(0x02, 0x000B);
pub const HCI_OP_READ_LINK_POLICY: u16 = hci_opcode(0x02, 0x000C);
pub const HCI_OP_WRITE_LINK_POLICY: u16 = hci_opcode(0x02, 0x000D);
pub const HCI_OP_READ_DEFAULT_LINK_POLICY: u16 = hci_opcode(0x02, 0x000E);
pub const HCI_OP_WRITE_DEFAULT_LINK_POLICY: u16 = hci_opcode(0x02, 0x000F);
pub const HCI_OP_SNIFF_SUBRATING: u16 = hci_opcode(0x02, 0x0011);

// Controller & Baseband Commands (OGF 0x03)
pub const HCI_OP_SET_EVENT_MASK: u16 = hci_opcode(0x03, 0x0001);
pub const HCI_OP_RESET: u16 = hci_opcode(0x03, 0x0003);
pub const HCI_OP_SET_EVENT_FLT: u16 = hci_opcode(0x03, 0x0005);
pub const HCI_OP_DELETE_STORED_LINK_KEY: u16 = hci_opcode(0x03, 0x0012);
pub const HCI_OP_WRITE_LOCAL_NAME: u16 = hci_opcode(0x03, 0x0013);
pub const HCI_OP_READ_LOCAL_NAME: u16 = hci_opcode(0x03, 0x0014);
pub const HCI_OP_READ_CONN_ACCEPT_TIMEOUT: u16 = hci_opcode(0x03, 0x0015);
pub const HCI_OP_WRITE_CONN_ACCEPT_TIMEOUT: u16 = hci_opcode(0x03, 0x0016);
pub const HCI_OP_READ_PAGE_TIMEOUT: u16 = hci_opcode(0x03, 0x0017);
pub const HCI_OP_WRITE_PAGE_TIMEOUT: u16 = hci_opcode(0x03, 0x0018);
pub const HCI_OP_READ_SCAN_ENABLE: u16 = hci_opcode(0x03, 0x0019);
pub const HCI_OP_WRITE_SCAN_ENABLE: u16 = hci_opcode(0x03, 0x001A);
pub const HCI_OP_READ_PAGE_SCAN_ACTIVITY: u16 = hci_opcode(0x03, 0x001B);
pub const HCI_OP_WRITE_PAGE_SCAN_ACTIVITY: u16 = hci_opcode(0x03, 0x001C);
pub const HCI_OP_READ_INQUIRY_SCAN_ACTIVITY: u16 = hci_opcode(0x03, 0x001D);
pub const HCI_OP_WRITE_INQUIRY_SCAN_ACTIVITY: u16 = hci_opcode(0x03, 0x001E);
pub const HCI_OP_READ_AUTH_ENABLE: u16 = hci_opcode(0x03, 0x001F);
pub const HCI_OP_WRITE_AUTH_ENABLE: u16 = hci_opcode(0x03, 0x0020);
pub const HCI_OP_READ_CLASS_OF_DEV: u16 = hci_opcode(0x03, 0x0023);
pub const HCI_OP_WRITE_CLASS_OF_DEV: u16 = hci_opcode(0x03, 0x0024);
pub const HCI_OP_READ_VOICE_SETTING: u16 = hci_opcode(0x03, 0x0025);
pub const HCI_OP_WRITE_VOICE_SETTING: u16 = hci_opcode(0x03, 0x0026);
pub const HCI_OP_READ_NUM_SUPPORTED_IAC: u16 = hci_opcode(0x03, 0x0038);
pub const HCI_OP_READ_CURRENT_IAC_LAP: u16 = hci_opcode(0x03, 0x0039);
pub const HCI_OP_WRITE_CURRENT_IAC_LAP: u16 = hci_opcode(0x03, 0x003A);
pub const HCI_OP_READ_INQ_RESP_TX_POWER: u16 = hci_opcode(0x03, 0x0058);
pub const HCI_OP_WRITE_INQ_TX_POWER: u16 = hci_opcode(0x03, 0x0059);
pub const HCI_OP_READ_SSP_MODE: u16 = hci_opcode(0x03, 0x0055);
pub const HCI_OP_WRITE_SSP_MODE: u16 = hci_opcode(0x03, 0x0056);
pub const HCI_OP_READ_LE_HOST_SUPPORTED: u16 = hci_opcode(0x03, 0x006C);
pub const HCI_OP_WRITE_LE_HOST_SUPPORTED: u16 = hci_opcode(0x03, 0x006D);
pub const HCI_OP_READ_SC_SUPPORT: u16 = hci_opcode(0x03, 0x0079);
pub const HCI_OP_WRITE_SC_SUPPORT: u16 = hci_opcode(0x03, 0x007A);

// Informational Parameters (OGF 0x04)
pub const HCI_OP_READ_LOCAL_VERSION: u16 = hci_opcode(0x04, 0x0001);
pub const HCI_OP_READ_LOCAL_COMMANDS: u16 = hci_opcode(0x04, 0x0002);
pub const HCI_OP_READ_LOCAL_FEATURES: u16 = hci_opcode(0x04, 0x0003);
pub const HCI_OP_READ_LOCAL_EXT_FEATURES: u16 = hci_opcode(0x04, 0x0004);
pub const HCI_OP_READ_BUFFER_SIZE: u16 = hci_opcode(0x04, 0x0005);
pub const HCI_OP_READ_BD_ADDR: u16 = hci_opcode(0x04, 0x0009);
pub const HCI_OP_READ_DATA_BLOCK_SIZE: u16 = hci_opcode(0x04, 0x000A);
pub const HCI_OP_READ_LOCAL_CODECS: u16 = hci_opcode(0x04, 0x000B);
pub const HCI_OP_READ_LOCAL_CODECS_V2: u16 = hci_opcode(0x04, 0x000D);

// Status Parameters (OGF 0x05)
pub const HCI_OP_READ_RSSI: u16 = hci_opcode(0x05, 0x0005);
pub const HCI_OP_READ_CLOCK: u16 = hci_opcode(0x05, 0x0007);

// LE Controller Commands (OGF 0x08)
pub const HCI_OP_LE_SET_EVENT_MASK: u16 = hci_opcode(0x08, 0x0001);
pub const HCI_OP_LE_READ_BUFFER_SIZE: u16 = hci_opcode(0x08, 0x0002);
pub const HCI_OP_LE_READ_LOCAL_FEATURES: u16 = hci_opcode(0x08, 0x0003);
pub const HCI_OP_LE_SET_RANDOM_ADDR: u16 = hci_opcode(0x08, 0x0005);
pub const HCI_OP_LE_SET_ADV_PARAM: u16 = hci_opcode(0x08, 0x0006);
pub const HCI_OP_LE_READ_ADV_TX_POWER: u16 = hci_opcode(0x08, 0x0007);
pub const HCI_OP_LE_SET_ADV_DATA: u16 = hci_opcode(0x08, 0x0008);
pub const HCI_OP_LE_SET_SCAN_RSP_DATA: u16 = hci_opcode(0x08, 0x0009);
pub const HCI_OP_LE_SET_ADV_ENABLE: u16 = hci_opcode(0x08, 0x000A);
pub const HCI_OP_LE_SET_SCAN_PARAM: u16 = hci_opcode(0x08, 0x000B);
pub const HCI_OP_LE_SET_SCAN_ENABLE: u16 = hci_opcode(0x08, 0x000C);
pub const HCI_OP_LE_CREATE_CONN: u16 = hci_opcode(0x08, 0x000D);
pub const HCI_OP_LE_CREATE_CONN_CANCEL: u16 = hci_opcode(0x08, 0x000E);
pub const HCI_OP_LE_READ_ACCEPT_LIST_SIZE: u16 = hci_opcode(0x08, 0x000F);
pub const HCI_OP_LE_CLEAR_ACCEPT_LIST: u16 = hci_opcode(0x08, 0x0010);
pub const HCI_OP_LE_ADD_TO_ACCEPT_LIST: u16 = hci_opcode(0x08, 0x0011);
pub const HCI_OP_LE_REMOVE_FROM_ACCEPT_LIST: u16 = hci_opcode(0x08, 0x0012);
pub const HCI_OP_LE_CONN_UPDATE: u16 = hci_opcode(0x08, 0x0013);
pub const HCI_OP_LE_READ_REMOTE_FEATURES: u16 = hci_opcode(0x08, 0x0016);
pub const HCI_OP_LE_ENCRYPT: u16 = hci_opcode(0x08, 0x0017);
pub const HCI_OP_LE_RAND: u16 = hci_opcode(0x08, 0x0018);
pub const HCI_OP_LE_START_ENC: u16 = hci_opcode(0x08, 0x0019);
pub const HCI_OP_LE_LTK_REPLY: u16 = hci_opcode(0x08, 0x001A);
pub const HCI_OP_LE_LTK_NEG_REPLY: u16 = hci_opcode(0x08, 0x001B);
pub const HCI_OP_LE_READ_SUPPORTED_STATES: u16 = hci_opcode(0x08, 0x001C);
pub const HCI_OP_LE_SET_DATA_LEN: u16 = hci_opcode(0x08, 0x0022);
pub const HCI_OP_LE_READ_DEF_DATA_LEN: u16 = hci_opcode(0x08, 0x0023);
pub const HCI_OP_LE_WRITE_DEF_DATA_LEN: u16 = hci_opcode(0x08, 0x0024);
pub const HCI_OP_LE_ADD_TO_RESOLV_LIST: u16 = hci_opcode(0x08, 0x0027);
pub const HCI_OP_LE_REMOVE_FROM_RESOLV_LIST: u16 = hci_opcode(0x08, 0x0028);
pub const HCI_OP_LE_CLEAR_RESOLV_LIST: u16 = hci_opcode(0x08, 0x0029);
pub const HCI_OP_LE_READ_RESOLV_LIST_SIZE: u16 = hci_opcode(0x08, 0x002A);
pub const HCI_OP_LE_SET_ADDR_RESOLV_ENABLE: u16 = hci_opcode(0x08, 0x002D);
pub const HCI_OP_LE_READ_MAX_DATA_LEN: u16 = hci_opcode(0x08, 0x002F);
pub const HCI_OP_LE_READ_PHY: u16 = hci_opcode(0x08, 0x0030);
pub const HCI_OP_LE_SET_DEFAULT_PHY: u16 = hci_opcode(0x08, 0x0031);
pub const HCI_OP_LE_SET_PHY: u16 = hci_opcode(0x08, 0x0032);
pub const HCI_OP_LE_SET_EXT_ADV_PARAM: u16 = hci_opcode(0x08, 0x0036);
pub const HCI_OP_LE_SET_EXT_ADV_DATA: u16 = hci_opcode(0x08, 0x0037);
pub const HCI_OP_LE_SET_EXT_SCAN_RSP_DATA: u16 = hci_opcode(0x08, 0x0038);
pub const HCI_OP_LE_SET_EXT_ADV_ENABLE: u16 = hci_opcode(0x08, 0x0039);
pub const HCI_OP_LE_SET_EXT_SCAN_PARAM: u16 = hci_opcode(0x08, 0x0041);
pub const HCI_OP_LE_SET_EXT_SCAN_ENABLE: u16 = hci_opcode(0x08, 0x0042);
pub const HCI_OP_LE_EXT_CREATE_CONN: u16 = hci_opcode(0x08, 0x0043);
pub const HCI_OP_LE_READ_TX_POWER: u16 = hci_opcode(0x08, 0x004B);
pub const HCI_OP_LE_SET_PRIVACY_MODE: u16 = hci_opcode(0x08, 0x004E);

// =============================================================================
// HCI EVENTS
// =============================================================================

/// HCI event codes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum HciEventCode {
    /// Inquiry complete
    InquiryComplete = 0x01,
    /// Inquiry result
    InquiryResult = 0x02,
    /// Connection complete
    ConnectionComplete = 0x03,
    /// Connection request
    ConnectionRequest = 0x04,
    /// Disconnection complete
    DisconnectionComplete = 0x05,
    /// Authentication complete
    AuthenticationComplete = 0x06,
    /// Remote name request complete
    RemoteNameReqComplete = 0x07,
    /// Encryption change
    EncryptionChange = 0x08,
    /// Change connection link key complete
    ChangeConnLinkKeyComplete = 0x09,
    /// Read remote features complete
    ReadRemoteFeaturesComplete = 0x0B,
    /// Read remote version complete
    ReadRemoteVersionComplete = 0x0C,
    /// QoS setup complete
    QosSetupComplete = 0x0D,
    /// Command complete
    CommandComplete = 0x0E,
    /// Command status
    CommandStatus = 0x0F,
    /// Hardware error
    HardwareError = 0x10,
    /// Role change
    RoleChange = 0x12,
    /// Number of completed packets
    NumCompletedPackets = 0x13,
    /// Mode change
    ModeChange = 0x14,
    /// PIN code request
    PinCodeRequest = 0x16,
    /// Link key request
    LinkKeyRequest = 0x17,
    /// Link key notification
    LinkKeyNotification = 0x18,
    /// Data buffer overflow
    DataBufferOverflow = 0x1A,
    /// Max slots change
    MaxSlotsChange = 0x1B,
    /// Read clock offset complete
    ReadClockOffsetComplete = 0x1C,
    /// Connection packet type changed
    ConnPacketTypeChanged = 0x1D,
    /// Page scan repetition mode change
    PageScanRepModeChange = 0x20,
    /// Inquiry result with RSSI
    InquiryResultRssi = 0x22,
    /// Read remote extended features complete
    ReadRemoteExtFeaturesComplete = 0x23,
    /// Synchronous connection complete
    SyncConnectionComplete = 0x2C,
    /// Synchronous connection changed
    SyncConnectionChanged = 0x2D,
    /// Sniff subrating
    SniffSubrating = 0x2E,
    /// Extended inquiry result
    ExtendedInquiryResult = 0x2F,
    /// Encryption key refresh complete
    EncryptionKeyRefreshComplete = 0x30,
    /// IO capability request
    IoCapabilityRequest = 0x31,
    /// IO capability response
    IoCapabilityResponse = 0x32,
    /// User confirmation request
    UserConfirmationRequest = 0x33,
    /// User passkey request
    UserPasskeyRequest = 0x34,
    /// Remote OOB data request
    RemoteOobDataRequest = 0x35,
    /// Simple pairing complete
    SimplePairingComplete = 0x36,
    /// Link supervision timeout changed
    LinkSupervisionTimeoutChanged = 0x38,
    /// User passkey notification
    UserPasskeyNotification = 0x3B,
    /// Keypress notification
    KeypressNotification = 0x3C,
    /// Remote host supported features notification
    RemoteHostSupportedFeaturesNotification = 0x3D,
    /// LE meta event
    LeMeta = 0x3E,
    /// Number of completed data blocks
    NumCompletedDataBlocks = 0x48,
    /// Vendor specific
    VendorSpecific = 0xFF,
}

/// LE meta event sub-codes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum LeMetaEvent {
    /// LE connection complete
    ConnectionComplete = 0x01,
    /// LE advertising report
    AdvertisingReport = 0x02,
    /// LE connection update complete
    ConnectionUpdateComplete = 0x03,
    /// LE read remote features complete
    ReadRemoteFeaturesComplete = 0x04,
    /// LE long term key request
    LongTermKeyRequest = 0x05,
    /// LE remote connection parameter request
    RemoteConnParamRequest = 0x06,
    /// LE data length change
    DataLengthChange = 0x07,
    /// LE read local P-256 public key complete
    ReadLocalP256PublicKeyComplete = 0x08,
    /// LE generate DHKey complete
    GenerateDhKeyComplete = 0x09,
    /// LE enhanced connection complete
    EnhancedConnectionComplete = 0x0A,
    /// LE direct advertising report
    DirectAdvertisingReport = 0x0B,
    /// LE PHY update complete
    PhyUpdateComplete = 0x0C,
    /// LE extended advertising report
    ExtendedAdvertisingReport = 0x0D,
    /// LE periodic advertising sync established
    PeriodicAdvSyncEstablished = 0x0E,
    /// LE periodic advertising report
    PeriodicAdvReport = 0x0F,
    /// LE periodic advertising sync lost
    PeriodicAdvSyncLost = 0x10,
    /// LE scan timeout
    ScanTimeout = 0x11,
    /// LE advertising set terminated
    AdvSetTerminated = 0x12,
    /// LE scan request received
    ScanRequestReceived = 0x13,
    /// LE channel selection algorithm
    ChannelSelAlgorithm = 0x14,
}

// =============================================================================
// HCI ERROR CODES
// =============================================================================

/// HCI status/error codes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum HciStatus {
    Success = 0x00,
    UnknownCommand = 0x01,
    NoConnection = 0x02,
    HardwareFailure = 0x03,
    PageTimeout = 0x04,
    AuthenticationFailure = 0x05,
    PinOrKeyMissing = 0x06,
    MemoryFull = 0x07,
    ConnectionTimeout = 0x08,
    MaxConnections = 0x09,
    MaxScoConnections = 0x0A,
    AclConnectionExists = 0x0B,
    CommandDisallowed = 0x0C,
    RejectedLimitedResources = 0x0D,
    RejectedSecurity = 0x0E,
    RejectedPersonal = 0x0F,
    HostTimeout = 0x10,
    UnsupportedFeature = 0x11,
    InvalidParameters = 0x12,
    RemoteUserTerminated = 0x13,
    RemoteLowResources = 0x14,
    RemotePowerOff = 0x15,
    LocalHostTerminated = 0x16,
    RepeatedAttempts = 0x17,
    PairingNotAllowed = 0x18,
    UnknownLmpPdu = 0x19,
    UnsupportedRemoteFeature = 0x1A,
    ScoOffsetRejected = 0x1B,
    ScoIntervalRejected = 0x1C,
    ScoAirModeRejected = 0x1D,
    InvalidLmpParameters = 0x1E,
    UnspecifiedError = 0x1F,
    UnsupportedLmpParameter = 0x20,
    RoleChangeNotAllowed = 0x21,
    LmpResponseTimeout = 0x22,
    LmpTransactionCollision = 0x23,
    LmpPduNotAllowed = 0x24,
    EncryptionModeNotAcceptable = 0x25,
    UnitKeyUsed = 0x26,
    QosNotSupported = 0x27,
    InstantPassed = 0x28,
    PairingUnitKeyNotSupported = 0x29,
    DifferentTransactionCollision = 0x2A,
    QosUnacceptableParameter = 0x2C,
    QosRejected = 0x2D,
    ChannelClassNotSupported = 0x2E,
    InsufficientSecurity = 0x2F,
    ParameterOutOfRange = 0x30,
    RoleSwitchPending = 0x32,
    SlotViolation = 0x34,
    RoleSwitchFailed = 0x35,
    EirTooLarge = 0x36,
    SimplePairingNotSupported = 0x37,
    HostBusyPairing = 0x38,
    ConnectionRejectedNoChannel = 0x39,
    ControllerBusy = 0x3A,
    UnacceptableConnParams = 0x3B,
    AdvertisingTimeout = 0x3C,
    ConnectionTerminatedMic = 0x3D,
    ConnectionFailedEstablishment = 0x3E,
    MacConnectionFailed = 0x3F,
    CoarseClock = 0x40,
}

impl From<u8> for HciStatus {
    fn from(value: u8) -> Self {
        match value {
            0x00 => Self::Success,
            0x01 => Self::UnknownCommand,
            0x02 => Self::NoConnection,
            0x03 => Self::HardwareFailure,
            0x04 => Self::PageTimeout,
            0x05 => Self::AuthenticationFailure,
            0x06 => Self::PinOrKeyMissing,
            0x07 => Self::MemoryFull,
            0x08 => Self::ConnectionTimeout,
            0x13 => Self::RemoteUserTerminated,
            0x16 => Self::LocalHostTerminated,
            _ => Self::UnspecifiedError,
        }
    }
}

// =============================================================================
// HCI PACKETS
// =============================================================================

/// HCI command packet
#[derive(Clone, Debug)]
pub struct HciCommand {
    /// Opcode
    pub opcode: u16,
    /// Parameters
    pub params: Vec<u8>,
}

impl HciCommand {
    /// Create new HCI command
    pub fn new(opcode: u16, params: Vec<u8>) -> Self {
        Self { opcode, params }
    }

    /// Create reset command
    pub fn reset() -> Self {
        Self::new(HCI_OP_RESET, Vec::new())
    }

    /// Create read BD_ADDR command
    pub fn read_bd_addr() -> Self {
        Self::new(HCI_OP_READ_BD_ADDR, Vec::new())
    }

    /// Create read local version command
    pub fn read_local_version() -> Self {
        Self::new(HCI_OP_READ_LOCAL_VERSION, Vec::new())
    }

    /// Create read local features command
    pub fn read_local_features() -> Self {
        Self::new(HCI_OP_READ_LOCAL_FEATURES, Vec::new())
    }

    /// Create write scan enable command
    pub fn write_scan_enable(scan_enable: u8) -> Self {
        Self::new(HCI_OP_WRITE_SCAN_ENABLE, alloc::vec![scan_enable])
    }

    /// Create set event mask command
    pub fn set_event_mask(mask: u64) -> Self {
        let mut params = Vec::with_capacity(8);
        params.extend_from_slice(&mask.to_le_bytes());
        Self::new(HCI_OP_SET_EVENT_MASK, params)
    }

    /// Create LE set scan parameters command
    pub fn le_set_scan_params(scan_type: u8, interval: u16, window: u16, own_addr_type: u8, filter_policy: u8) -> Self {
        let mut params = Vec::with_capacity(7);
        params.push(scan_type);
        params.extend_from_slice(&interval.to_le_bytes());
        params.extend_from_slice(&window.to_le_bytes());
        params.push(own_addr_type);
        params.push(filter_policy);
        Self::new(HCI_OP_LE_SET_SCAN_PARAM, params)
    }

    /// Create LE set scan enable command
    pub fn le_set_scan_enable(enable: bool, filter_duplicates: bool) -> Self {
        let params = alloc::vec![enable as u8, filter_duplicates as u8];
        Self::new(HCI_OP_LE_SET_SCAN_ENABLE, params)
    }

    /// Create LE set advertising parameters command
    pub fn le_set_adv_params(
        interval_min: u16,
        interval_max: u16,
        adv_type: u8,
        own_addr_type: u8,
        peer_addr_type: u8,
        peer_addr: &BdAddr,
        channel_map: u8,
        filter_policy: u8,
    ) -> Self {
        let mut params = Vec::with_capacity(15);
        params.extend_from_slice(&interval_min.to_le_bytes());
        params.extend_from_slice(&interval_max.to_le_bytes());
        params.push(adv_type);
        params.push(own_addr_type);
        params.push(peer_addr_type);
        params.extend_from_slice(&peer_addr.0);
        params.push(channel_map);
        params.push(filter_policy);
        Self::new(HCI_OP_LE_SET_ADV_PARAM, params)
    }

    /// Create LE set advertising enable command
    pub fn le_set_adv_enable(enable: bool) -> Self {
        Self::new(HCI_OP_LE_SET_ADV_ENABLE, alloc::vec![enable as u8])
    }

    /// Create inquiry command
    pub fn inquiry(lap: u32, inquiry_length: u8, num_responses: u8) -> Self {
        let mut params = Vec::with_capacity(5);
        params.push((lap & 0xFF) as u8);
        params.push(((lap >> 8) & 0xFF) as u8);
        params.push(((lap >> 16) & 0xFF) as u8);
        params.push(inquiry_length);
        params.push(num_responses);
        Self::new(HCI_OP_INQUIRY, params)
    }

    /// Create inquiry cancel command
    pub fn inquiry_cancel() -> Self {
        Self::new(HCI_OP_INQUIRY_CANCEL, Vec::new())
    }

    /// Create connection command
    pub fn create_connection(
        address: &BdAddr,
        packet_type: u16,
        page_scan_rep_mode: u8,
        clock_offset: u16,
        allow_role_switch: bool,
    ) -> Self {
        let mut params = Vec::with_capacity(13);
        params.extend_from_slice(&address.0);
        params.extend_from_slice(&packet_type.to_le_bytes());
        params.push(page_scan_rep_mode);
        params.push(0); // Reserved
        params.extend_from_slice(&clock_offset.to_le_bytes());
        params.push(allow_role_switch as u8);
        Self::new(HCI_OP_CREATE_CONN, params)
    }

    /// Create LE connection command
    pub fn le_create_connection(
        scan_interval: u16,
        scan_window: u16,
        filter_policy: u8,
        peer_addr_type: u8,
        peer_addr: &BdAddr,
        own_addr_type: u8,
        conn_interval_min: u16,
        conn_interval_max: u16,
        conn_latency: u16,
        supervision_timeout: u16,
        min_ce_len: u16,
        max_ce_len: u16,
    ) -> Self {
        let mut params = Vec::with_capacity(25);
        params.extend_from_slice(&scan_interval.to_le_bytes());
        params.extend_from_slice(&scan_window.to_le_bytes());
        params.push(filter_policy);
        params.push(peer_addr_type);
        params.extend_from_slice(&peer_addr.0);
        params.push(own_addr_type);
        params.extend_from_slice(&conn_interval_min.to_le_bytes());
        params.extend_from_slice(&conn_interval_max.to_le_bytes());
        params.extend_from_slice(&conn_latency.to_le_bytes());
        params.extend_from_slice(&supervision_timeout.to_le_bytes());
        params.extend_from_slice(&min_ce_len.to_le_bytes());
        params.extend_from_slice(&max_ce_len.to_le_bytes());
        Self::new(HCI_OP_LE_CREATE_CONN, params)
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(3 + self.params.len());
        bytes.extend_from_slice(&self.opcode.to_le_bytes());
        bytes.push(self.params.len() as u8);
        bytes.extend_from_slice(&self.params);
        bytes
    }
}

/// HCI event packet
#[derive(Clone, Debug)]
pub struct HciEvent {
    /// Event code
    pub code: u8,
    /// Parameters
    pub params: Vec<u8>,
}

impl HciEvent {
    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 2 {
            return None;
        }
        let code = data[0];
        let len = data[1] as usize;
        if data.len() < 2 + len {
            return None;
        }
        Some(Self {
            code,
            params: data[2..2 + len].to_vec(),
        })
    }

    /// Get event code
    pub fn event_code(&self) -> Option<HciEventCode> {
        match self.code {
            0x01 => Some(HciEventCode::InquiryComplete),
            0x02 => Some(HciEventCode::InquiryResult),
            0x03 => Some(HciEventCode::ConnectionComplete),
            0x04 => Some(HciEventCode::ConnectionRequest),
            0x05 => Some(HciEventCode::DisconnectionComplete),
            0x0E => Some(HciEventCode::CommandComplete),
            0x0F => Some(HciEventCode::CommandStatus),
            0x3E => Some(HciEventCode::LeMeta),
            _ => None,
        }
    }

    /// Check if this is command complete for given opcode
    pub fn is_command_complete(&self, opcode: u16) -> bool {
        if self.code != HciEventCode::CommandComplete as u8 {
            return false;
        }
        if self.params.len() < 3 {
            return false;
        }
        let event_opcode = u16::from_le_bytes([self.params[1], self.params[2]]);
        event_opcode == opcode
    }

    /// Get status from command complete event
    pub fn command_complete_status(&self) -> Option<HciStatus> {
        if self.code != HciEventCode::CommandComplete as u8 {
            return None;
        }
        if self.params.len() < 4 {
            return None;
        }
        Some(HciStatus::from(self.params[3]))
    }
}

/// ACL packet flags
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AclPacketBoundary {
    /// First non-automatically flushable packet
    FirstNonFlushable = 0x00,
    /// Continuing fragment
    Continuing = 0x01,
    /// First automatically flushable packet
    FirstAutoFlush = 0x02,
    /// Complete L2CAP PDU
    Complete = 0x03,
}

/// ACL data packet
#[derive(Clone, Debug)]
pub struct AclPacket {
    /// Connection handle
    pub handle: u16,
    /// Packet boundary flag
    pub pb_flag: AclPacketBoundary,
    /// Broadcast flag
    pub bc_flag: u8,
    /// Data
    pub data: Vec<u8>,
}

impl AclPacket {
    /// Create new ACL packet
    pub fn new(handle: u16, pb_flag: AclPacketBoundary, data: Vec<u8>) -> Self {
        Self {
            handle,
            pb_flag,
            bc_flag: 0,
            data,
        }
    }

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 4 {
            return None;
        }
        let hdr = u16::from_le_bytes([data[0], data[1]]);
        let len = u16::from_le_bytes([data[2], data[3]]) as usize;

        if data.len() < 4 + len {
            return None;
        }

        let handle = hdr & 0x0FFF;
        let pb_flag = match (hdr >> 12) & 0x03 {
            0x00 => AclPacketBoundary::FirstNonFlushable,
            0x01 => AclPacketBoundary::Continuing,
            0x02 => AclPacketBoundary::FirstAutoFlush,
            _ => AclPacketBoundary::Complete,
        };
        let bc_flag = ((hdr >> 14) & 0x03) as u8;

        Some(Self {
            handle,
            pb_flag,
            bc_flag,
            data: data[4..4 + len].to_vec(),
        })
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let hdr = self.handle
            | ((self.pb_flag as u16) << 12)
            | ((self.bc_flag as u16) << 14);
        let len = self.data.len() as u16;

        let mut bytes = Vec::with_capacity(4 + self.data.len());
        bytes.extend_from_slice(&hdr.to_le_bytes());
        bytes.extend_from_slice(&len.to_le_bytes());
        bytes.extend_from_slice(&self.data);
        bytes
    }
}

// =============================================================================
// HCI CONTROLLER
// =============================================================================

/// HCI transport type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HciTransport {
    /// USB transport
    Usb,
    /// UART transport (H4)
    Uart,
    /// SDIO transport
    Sdio,
    /// PCI/PCIe transport
    Pci,
    /// Virtual (testing)
    Virtual,
}

/// HCI controller state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HciState {
    /// Not initialized
    Off,
    /// Initializing
    Initializing,
    /// Running
    Running,
    /// Error state
    Error,
    /// Closing
    Closing,
}

/// HCI features bitmap
#[derive(Clone, Copy, Debug, Default)]
pub struct HciFeatures {
    /// Raw features bytes
    pub raw: [u8; 8],
}

impl HciFeatures {
    /// Check if feature is supported
    pub fn has_feature(&self, byte: usize, bit: u8) -> bool {
        if byte >= 8 {
            return false;
        }
        (self.raw[byte] & (1 << bit)) != 0
    }

    /// 3-slot packets
    pub fn has_3_slot_packets(&self) -> bool { self.has_feature(0, 0) }
    /// 5-slot packets
    pub fn has_5_slot_packets(&self) -> bool { self.has_feature(0, 1) }
    /// Encryption
    pub fn has_encryption(&self) -> bool { self.has_feature(0, 2) }
    /// Slot offset
    pub fn has_slot_offset(&self) -> bool { self.has_feature(0, 3) }
    /// Timing accuracy
    pub fn has_timing_accuracy(&self) -> bool { self.has_feature(0, 4) }
    /// Role switch
    pub fn has_role_switch(&self) -> bool { self.has_feature(0, 5) }
    /// Hold mode
    pub fn has_hold_mode(&self) -> bool { self.has_feature(0, 6) }
    /// Sniff mode
    pub fn has_sniff_mode(&self) -> bool { self.has_feature(0, 7) }
    /// eSCO
    pub fn has_esco(&self) -> bool { self.has_feature(3, 7) }
    /// EV4 packets
    pub fn has_ev4(&self) -> bool { self.has_feature(4, 0) }
    /// EV5 packets
    pub fn has_ev5(&self) -> bool { self.has_feature(4, 1) }
    /// AFH capable slave
    pub fn has_afh_capable_slave(&self) -> bool { self.has_feature(4, 3) }
    /// AFH classification slave
    pub fn has_afh_class_slave(&self) -> bool { self.has_feature(4, 4) }
    /// BR/EDR not supported
    pub fn br_edr_not_supported(&self) -> bool { self.has_feature(4, 5) }
    /// LE supported (controller)
    pub fn has_le_supported(&self) -> bool { self.has_feature(4, 6) }
    /// 3-slot EDR ACL packets
    pub fn has_3_slot_edr_acl(&self) -> bool { self.has_feature(4, 7) }
    /// 5-slot EDR ACL packets
    pub fn has_5_slot_edr_acl(&self) -> bool { self.has_feature(5, 0) }
    /// Sniff subrating
    pub fn has_sniff_subrating(&self) -> bool { self.has_feature(5, 1) }
    /// Pause encryption
    pub fn has_pause_encryption(&self) -> bool { self.has_feature(5, 2) }
    /// AFH capable master
    pub fn has_afh_capable_master(&self) -> bool { self.has_feature(5, 3) }
    /// AFH classification master
    pub fn has_afh_class_master(&self) -> bool { self.has_feature(5, 4) }
    /// EDR eSCO 2M
    pub fn has_edr_esco_2m(&self) -> bool { self.has_feature(5, 5) }
    /// EDR eSCO 3M
    pub fn has_edr_esco_3m(&self) -> bool { self.has_feature(5, 6) }
    /// 3-slot EDR eSCO
    pub fn has_3_slot_edr_esco(&self) -> bool { self.has_feature(5, 7) }
    /// Extended inquiry response
    pub fn has_eir(&self) -> bool { self.has_feature(6, 0) }
    /// Simultaneous LE and BR/EDR
    pub fn has_le_bredr_same_time(&self) -> bool { self.has_feature(6, 1) }
    /// Secure simple pairing
    pub fn has_ssp(&self) -> bool { self.has_feature(6, 3) }
    /// Encapsulated PDU
    pub fn has_encapsulated_pdu(&self) -> bool { self.has_feature(6, 4) }
    /// Erroneous data reporting
    pub fn has_erroneous_data_reporting(&self) -> bool { self.has_feature(6, 5) }
    /// Non-flushable packet boundary flag
    pub fn has_non_flushable_pb(&self) -> bool { self.has_feature(6, 6) }
    /// Link supervision timeout changed event
    pub fn has_link_supervision_timeout_changed(&self) -> bool { self.has_feature(7, 0) }
    /// Inquiry TX power level
    pub fn has_inq_tx_power(&self) -> bool { self.has_feature(7, 1) }
    /// Enhanced power control
    pub fn has_enhanced_power_control(&self) -> bool { self.has_feature(7, 2) }
    /// Extended features
    pub fn has_extended_features(&self) -> bool { self.has_feature(7, 7) }
}

/// LE features bitmap
#[derive(Clone, Copy, Debug, Default)]
pub struct LeFeatures {
    /// Raw features bytes
    pub raw: [u8; 8],
}

impl LeFeatures {
    /// Check if LE feature is supported
    pub fn has_feature(&self, bit: u8) -> bool {
        let byte = (bit / 8) as usize;
        let bit = bit % 8;
        if byte >= 8 {
            return false;
        }
        (self.raw[byte] & (1 << bit)) != 0
    }

    /// LE encryption
    pub fn has_encryption(&self) -> bool { self.has_feature(0) }
    /// Connection parameters request procedure
    pub fn has_conn_param_req(&self) -> bool { self.has_feature(1) }
    /// Extended reject indication
    pub fn has_ext_reject(&self) -> bool { self.has_feature(2) }
    /// Slave-initiated features exchange
    pub fn has_slave_feat_exchange(&self) -> bool { self.has_feature(3) }
    /// LE ping
    pub fn has_ping(&self) -> bool { self.has_feature(4) }
    /// LE data packet length extension
    pub fn has_data_len_ext(&self) -> bool { self.has_feature(5) }
    /// LL privacy
    pub fn has_privacy(&self) -> bool { self.has_feature(6) }
    /// Extended scanner filter policies
    pub fn has_ext_scan_filter(&self) -> bool { self.has_feature(7) }
    /// LE 2M PHY
    pub fn has_2m_phy(&self) -> bool { self.has_feature(8) }
    /// Stable modulation index (TX)
    pub fn has_stable_mod_idx_tx(&self) -> bool { self.has_feature(9) }
    /// Stable modulation index (RX)
    pub fn has_stable_mod_idx_rx(&self) -> bool { self.has_feature(10) }
    /// LE coded PHY
    pub fn has_coded_phy(&self) -> bool { self.has_feature(11) }
    /// LE extended advertising
    pub fn has_ext_adv(&self) -> bool { self.has_feature(12) }
    /// LE periodic advertising
    pub fn has_periodic_adv(&self) -> bool { self.has_feature(13) }
    /// Channel selection algorithm #2
    pub fn has_chan_sel_alg_2(&self) -> bool { self.has_feature(14) }
    /// LE power class 1
    pub fn has_power_class_1(&self) -> bool { self.has_feature(15) }
    /// Minimum number of used channels procedure
    pub fn has_min_used_channels(&self) -> bool { self.has_feature(16) }
}

/// HCI local version information
#[derive(Clone, Debug, Default)]
pub struct HciVersion {
    /// HCI version
    pub hci_version: u8,
    /// HCI revision
    pub hci_revision: u16,
    /// LMP version
    pub lmp_version: u8,
    /// Manufacturer
    pub manufacturer: u16,
    /// LMP subversion
    pub lmp_subversion: u16,
}

/// HCI buffer information
#[derive(Clone, Debug, Default)]
pub struct HciBufferInfo {
    /// ACL MTU
    pub acl_mtu: u16,
    /// SCO MTU
    pub sco_mtu: u8,
    /// Number of ACL packets
    pub acl_pkts: u16,
    /// Number of SCO packets
    pub sco_pkts: u16,
    /// LE ACL MTU
    pub le_mtu: u16,
    /// Number of LE ACL packets
    pub le_pkts: u8,
}

/// HCI controller
pub struct HciController {
    /// Transport type
    pub transport: HciTransport,
    /// Controller state
    state: AtomicU8,
    /// BD_ADDR
    pub address: RwLock<BdAddr>,
    /// Local name
    pub name: RwLock<String>,
    /// Version info
    pub version: RwLock<HciVersion>,
    /// Features
    pub features: RwLock<HciFeatures>,
    /// LE features
    pub le_features: RwLock<LeFeatures>,
    /// Buffer info
    pub buffer_info: RwLock<HciBufferInfo>,
    /// Command queue
    cmd_queue: Mutex<VecDeque<HciCommand>>,
    /// Event handlers
    event_handlers: RwLock<Vec<Box<dyn Fn(&HciEvent) + Send + Sync>>>,
    /// Pending command opcode
    pending_cmd: AtomicU16,
    /// Pending command response
    pending_response: Mutex<Option<HciEvent>>,
    /// Active connections
    connections: RwLock<Vec<HciConnection>>,
    /// Next connection handle
    next_handle: AtomicU16,
}

impl HciController {
    /// Create new HCI controller
    pub fn new(transport: HciTransport) -> Self {
        Self {
            transport,
            state: AtomicU8::new(HciState::Off as u8),
            address: RwLock::new(BdAddr::ZERO),
            name: RwLock::new(String::new()),
            version: RwLock::new(HciVersion::default()),
            features: RwLock::new(HciFeatures::default()),
            le_features: RwLock::new(LeFeatures::default()),
            buffer_info: RwLock::new(HciBufferInfo::default()),
            cmd_queue: Mutex::new(VecDeque::new()),
            event_handlers: RwLock::new(Vec::new()),
            pending_cmd: AtomicU16::new(0),
            pending_response: Mutex::new(None),
            connections: RwLock::new(Vec::new()),
            next_handle: AtomicU16::new(1),
        }
    }

    /// Get controller state
    pub fn state(&self) -> HciState {
        match self.state.load(Ordering::Acquire) {
            0 => HciState::Off,
            1 => HciState::Initializing,
            2 => HciState::Running,
            3 => HciState::Error,
            _ => HciState::Closing,
        }
    }

    /// Set controller state
    fn set_state(&self, state: HciState) {
        self.state.store(state as u8, Ordering::Release);
    }

    /// Initialize controller
    pub fn init(&self) -> Result<(), BluetoothError> {
        self.set_state(HciState::Initializing);

        // Send reset command
        self.send_command(HciCommand::reset())?;

        // Read local version
        self.send_command(HciCommand::read_local_version())?;

        // Read BD_ADDR
        self.send_command(HciCommand::read_bd_addr())?;

        // Read local features
        self.send_command(HciCommand::read_local_features())?;

        // Set event mask
        let event_mask = 0x3DBFF807FFFBFFFF_u64; // Standard event mask
        self.send_command(HciCommand::set_event_mask(event_mask))?;

        self.set_state(HciState::Running);
        Ok(())
    }

    /// Send HCI command
    pub fn send_command(&self, cmd: HciCommand) -> Result<HciEvent, BluetoothError> {
        let opcode = cmd.opcode;

        // Queue command
        self.cmd_queue.lock().push_back(cmd);
        self.pending_cmd.store(opcode, Ordering::Release);

        // In real implementation, this would send to hardware and wait for response
        // For now, return a dummy success response
        let response = HciEvent {
            code: HciEventCode::CommandComplete as u8,
            params: alloc::vec![1, (opcode & 0xFF) as u8, (opcode >> 8) as u8, 0],
        };

        Ok(response)
    }

    /// Send HCI command without waiting for response
    pub fn send_command_no_wait(&self, cmd: HciCommand) -> Result<(), BluetoothError> {
        self.cmd_queue.lock().push_back(cmd);
        Ok(())
    }

    /// Handle incoming event
    pub fn handle_event(&self, event: &HciEvent) {
        // Check if this is response to pending command
        let pending = self.pending_cmd.load(Ordering::Acquire);
        if pending != 0 && event.is_command_complete(pending) {
            *self.pending_response.lock() = Some(event.clone());
            self.pending_cmd.store(0, Ordering::Release);
        }

        // Process specific events
        match event.event_code() {
            Some(HciEventCode::ConnectionComplete) => {
                self.handle_connection_complete(event);
            }
            Some(HciEventCode::DisconnectionComplete) => {
                self.handle_disconnection_complete(event);
            }
            Some(HciEventCode::LeMeta) => {
                self.handle_le_meta_event(event);
            }
            _ => {}
        }

        // Call registered event handlers
        for handler in self.event_handlers.read().iter() {
            handler(event);
        }
    }

    /// Handle connection complete event
    fn handle_connection_complete(&self, event: &HciEvent) {
        if event.params.len() < 11 {
            return;
        }

        let status = HciStatus::from(event.params[0]);
        if status != HciStatus::Success {
            return;
        }

        let handle = u16::from_le_bytes([event.params[1], event.params[2]]);
        let mut addr = [0u8; 6];
        addr.copy_from_slice(&event.params[3..9]);
        let link_type = event.params[9];
        let encryption = event.params[10];

        let conn = HciConnection {
            handle,
            address: BdAddr(addr),
            link_type: if link_type == 0 { LinkType::Sco } else { LinkType::Acl },
            encrypted: encryption != 0,
            role: ConnectionRole::Master,
            state: ConnectionState::Connected,
        };

        self.connections.write().push(conn);
    }

    /// Handle disconnection complete event
    fn handle_disconnection_complete(&self, event: &HciEvent) {
        if event.params.len() < 4 {
            return;
        }

        let status = HciStatus::from(event.params[0]);
        if status != HciStatus::Success {
            return;
        }

        let handle = u16::from_le_bytes([event.params[1], event.params[2]]);

        self.connections.write().retain(|c| c.handle != handle);
    }

    /// Handle LE meta event
    fn handle_le_meta_event(&self, event: &HciEvent) {
        if event.params.is_empty() {
            return;
        }

        let subevent = event.params[0];
        match subevent {
            0x01 => self.handle_le_connection_complete(event),
            0x02 => self.handle_le_advertising_report(event),
            0x0A => self.handle_le_enhanced_connection_complete(event),
            _ => {}
        }
    }

    /// Handle LE connection complete
    fn handle_le_connection_complete(&self, event: &HciEvent) {
        if event.params.len() < 19 {
            return;
        }

        let status = HciStatus::from(event.params[1]);
        if status != HciStatus::Success {
            return;
        }

        let handle = u16::from_le_bytes([event.params[2], event.params[3]]);
        let role = if event.params[4] == 0 { ConnectionRole::Master } else { ConnectionRole::Slave };
        let _peer_addr_type = event.params[5];
        let mut addr = [0u8; 6];
        addr.copy_from_slice(&event.params[6..12]);

        let conn = HciConnection {
            handle,
            address: BdAddr(addr),
            link_type: LinkType::Le,
            encrypted: false,
            role,
            state: ConnectionState::Connected,
        };

        self.connections.write().push(conn);
    }

    /// Handle LE enhanced connection complete
    fn handle_le_enhanced_connection_complete(&self, event: &HciEvent) {
        // Similar to regular LE connection complete but with more fields
        self.handle_le_connection_complete(event);
    }

    /// Handle LE advertising report
    fn handle_le_advertising_report(&self, _event: &HciEvent) {
        // Process advertising reports for scanning
        // This would be used by the scanning subsystem
    }

    /// Send ACL data
    pub fn send_acl(&self, packet: &AclPacket) -> Result<(), BluetoothError> {
        // In real implementation, send to hardware
        let _bytes = packet.to_bytes();
        Ok(())
    }

    /// Get connection by handle
    pub fn get_connection(&self, handle: u16) -> Option<HciConnection> {
        self.connections.read().iter().find(|c| c.handle == handle).cloned()
    }

    /// Get connection by address
    pub fn get_connection_by_addr(&self, addr: &BdAddr) -> Option<HciConnection> {
        self.connections.read().iter().find(|c| c.address == *addr).cloned()
    }

    /// Disconnect
    pub fn disconnect(&self, handle: u16, reason: u8) -> Result<(), BluetoothError> {
        let mut params = Vec::with_capacity(3);
        params.extend_from_slice(&handle.to_le_bytes());
        params.push(reason);

        let cmd = HciCommand::new(HCI_OP_DISCONNECT, params);
        self.send_command_no_wait(cmd)?;

        Ok(())
    }

    /// Register event handler
    pub fn register_event_handler<F>(&self, handler: F)
    where
        F: Fn(&HciEvent) + Send + Sync + 'static,
    {
        self.event_handlers.write().push(Box::new(handler));
    }

    /// Check if LE is supported
    pub fn supports_le(&self) -> bool {
        self.features.read().has_le_supported()
    }

    /// Check if BR/EDR is supported
    pub fn supports_bredr(&self) -> bool {
        !self.features.read().br_edr_not_supported()
    }

    /// Check if SSP is supported
    pub fn supports_ssp(&self) -> bool {
        self.features.read().has_ssp()
    }

    /// Get all active connections
    pub fn connections(&self) -> Vec<HciConnection> {
        self.connections.read().clone()
    }

    /// Allocate connection handle
    pub fn alloc_handle(&self) -> u16 {
        self.next_handle.fetch_add(1, Ordering::SeqCst)
    }

    /// Reset controller
    pub fn reset(&self) -> Result<(), BluetoothError> {
        self.send_command(HciCommand::reset())?;
        Ok(())
    }

    /// Read BD_ADDR
    pub fn read_bd_addr(&self) -> Result<BdAddr, BluetoothError> {
        self.send_command(HciCommand::read_bd_addr())?;
        // In a real implementation, parse the response
        Ok(*self.address.read())
    }

    /// Read local features
    pub fn read_local_features(&self) -> Result<(), BluetoothError> {
        self.send_command(HciCommand::read_local_features())?;
        Ok(())
    }

    /// Write scan enable
    pub fn write_scan_enable(&self, scan_enable: u8) -> Result<(), BluetoothError> {
        self.send_command(HciCommand::write_scan_enable(scan_enable))?;
        Ok(())
    }

    /// Start inquiry
    pub fn inquiry(&self, lap: u32, inquiry_length: u8, num_responses: u8) -> Result<(), BluetoothError> {
        self.send_command_no_wait(HciCommand::inquiry(lap, inquiry_length, num_responses))?;
        Ok(())
    }

    /// Cancel inquiry
    pub fn inquiry_cancel(&self) -> Result<(), BluetoothError> {
        self.send_command(HciCommand::inquiry_cancel())?;
        Ok(())
    }

    /// Set LE scan parameters
    pub fn le_set_scan_parameters(
        &self,
        scan_type: u8,
        interval: u16,
        window: u16,
        own_addr_type: u8,
        filter_policy: u8,
    ) -> Result<(), BluetoothError> {
        self.send_command(HciCommand::le_set_scan_params(
            scan_type, interval, window, own_addr_type, filter_policy,
        ))?;
        Ok(())
    }

    /// Set LE scan enable
    pub fn le_set_scan_enable(&self, enable: bool, filter_duplicates: bool) -> Result<(), BluetoothError> {
        self.send_command(HciCommand::le_set_scan_enable(enable, filter_duplicates))?;
        Ok(())
    }

    /// Create BR/EDR connection
    pub fn create_connection(&self, address: &BdAddr) -> Result<u16, BluetoothError> {
        let cmd = HciCommand::create_connection(
            address,
            0xCC18,  // DM1, DH1, DM3, DH3, DM5, DH5
            0x02,    // R2 page scan repetition mode
            0x0000,  // Clock offset not valid
            true,    // Allow role switch
        );
        self.send_command_no_wait(cmd)?;
        // In real implementation, wait for connection complete event
        Ok(self.alloc_handle())
    }

    /// Create LE connection
    pub fn le_create_connection(&self, address: &BdAddr, address_type: AddressType) -> Result<u16, BluetoothError> {
        let cmd = HciCommand::le_create_connection(
            0x0010,  // Scan interval (10ms)
            0x0010,  // Scan window (10ms)
            0x00,    // Filter policy: use peer address
            address_type as u8,
            address,
            0x00,    // Own address type: public
            0x0006,  // Conn interval min (7.5ms)
            0x000C,  // Conn interval max (15ms)
            0x0000,  // Slave latency
            0x00C8,  // Supervision timeout (2s)
            0x0000,  // Min CE length
            0x0000,  // Max CE length
        );
        self.send_command_no_wait(cmd)?;
        // In real implementation, wait for LE connection complete event
        Ok(self.alloc_handle())
    }
}

/// Link type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkType {
    /// SCO connection
    Sco,
    /// ACL connection
    Acl,
    /// eSCO connection
    Esco,
    /// LE connection
    Le,
}

/// Connection role
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionRole {
    /// Master/central role
    Master,
    /// Slave/peripheral role
    Slave,
}

/// Connection state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionState {
    /// Connecting
    Connecting,
    /// Connected
    Connected,
    /// Disconnecting
    Disconnecting,
    /// Disconnected
    Disconnected,
}

/// HCI connection
#[derive(Clone, Debug)]
pub struct HciConnection {
    /// Connection handle
    pub handle: u16,
    /// Remote address
    pub address: BdAddr,
    /// Link type
    pub link_type: LinkType,
    /// Is encrypted
    pub encrypted: bool,
    /// Role
    pub role: ConnectionRole,
    /// State
    pub state: ConnectionState,
}

// =============================================================================
// USB TRANSPORT
// =============================================================================

/// USB HCI transport
pub struct HciUsb {
    /// Vendor ID
    pub vendor_id: u16,
    /// Product ID
    pub product_id: u16,
    /// USB device handle
    device_handle: Option<usize>,
    /// Bulk IN endpoint
    bulk_in: u8,
    /// Bulk OUT endpoint
    bulk_out: u8,
    /// Interrupt endpoint
    interrupt: u8,
}

impl HciUsb {
    /// Create new USB transport
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        Self {
            vendor_id,
            product_id,
            device_handle: None,
            bulk_in: 0x81,
            bulk_out: 0x02,
            interrupt: 0x83,
        }
    }

    /// Open USB device
    pub fn open(&mut self) -> Result<(), BluetoothError> {
        // In real implementation, find and open USB device
        self.device_handle = Some(0);
        Ok(())
    }

    /// Close USB device
    pub fn close(&mut self) {
        self.device_handle = None;
    }

    /// Send command
    pub fn send_command(&self, cmd: &HciCommand) -> Result<(), BluetoothError> {
        if self.device_handle.is_none() {
            return Err(BluetoothError::NotConnected);
        }

        // In real implementation, send via USB control transfer
        let _bytes = cmd.to_bytes();
        Ok(())
    }

    /// Send ACL data
    pub fn send_acl(&self, packet: &AclPacket) -> Result<(), BluetoothError> {
        if self.device_handle.is_none() {
            return Err(BluetoothError::NotConnected);
        }

        // In real implementation, send via bulk OUT
        let _bytes = packet.to_bytes();
        Ok(())
    }

    /// Receive event
    pub fn recv_event(&self) -> Result<HciEvent, BluetoothError> {
        if self.device_handle.is_none() {
            return Err(BluetoothError::NotConnected);
        }

        // In real implementation, receive via interrupt endpoint
        Err(BluetoothError::Timeout)
    }

    /// Receive ACL data
    pub fn recv_acl(&self) -> Result<AclPacket, BluetoothError> {
        if self.device_handle.is_none() {
            return Err(BluetoothError::NotConnected);
        }

        // In real implementation, receive via bulk IN
        Err(BluetoothError::Timeout)
    }
}

// =============================================================================
// UART TRANSPORT (H4)
// =============================================================================

/// UART HCI transport (H4 protocol)
pub struct HciUart {
    /// UART port number
    port: u8,
    /// Baud rate
    baud_rate: u32,
    /// Is open
    open: AtomicBool,
    /// RX buffer
    rx_buffer: Mutex<VecDeque<u8>>,
}

impl HciUart {
    /// Create new UART transport
    pub fn new(port: u8, baud_rate: u32) -> Self {
        Self {
            port,
            baud_rate,
            open: AtomicBool::new(false),
            rx_buffer: Mutex::new(VecDeque::new()),
        }
    }

    /// Open UART port
    pub fn open(&self) -> Result<(), BluetoothError> {
        // In real implementation, configure UART
        self.open.store(true, Ordering::Release);
        Ok(())
    }

    /// Close UART port
    pub fn close(&self) {
        self.open.store(false, Ordering::Release);
    }

    /// Send packet with H4 header
    pub fn send(&self, packet_type: HciPacketType, _data: &[u8]) -> Result<(), BluetoothError> {
        if !self.open.load(Ordering::Acquire) {
            return Err(BluetoothError::NotConnected);
        }

        // Send H4 packet type indicator followed by data
        let _indicator = packet_type as u8;
        // In real implementation, write to UART
        Ok(())
    }

    /// Receive packet
    pub fn recv(&self) -> Result<(HciPacketType, Vec<u8>), BluetoothError> {
        if !self.open.load(Ordering::Acquire) {
            return Err(BluetoothError::NotConnected);
        }

        // In real implementation, read from UART
        Err(BluetoothError::Timeout)
    }
}
