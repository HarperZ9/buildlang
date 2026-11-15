//! Bluetooth Subsystem
//!
//! Implements Bluetooth Host Controller Interface (HCI) and profiles:
//! - HCI core for controller communication
//! - L2CAP for logical link control
//! - SDP for service discovery
//! - RFCOMM for serial port emulation
//! - A2DP for audio streaming
//! - HID for input devices

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::RwLock;

pub mod hci;
pub mod l2cap;
pub mod smp;
pub mod sdp;
pub mod rfcomm;
pub mod a2dp;
pub mod hid;

pub use hci::{HciController, HciCommand, HciEvent, AclPacket};
pub use l2cap::{L2capChannel, L2capPacket};

/// Type alias for backwards compatibility
pub type HciPacket = AclPacket;
/// Type alias for backwards compatibility
pub type L2capSocket = L2capChannel;

/// Bluetooth errors
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BluetoothError {
    /// No adapter found
    NoAdapter,
    /// Adapter busy
    AdapterBusy,
    /// Device not found
    DeviceNotFound,
    /// Connection failed
    ConnectionFailed,
    /// Connection refused
    ConnectionRefused,
    /// Connection timeout
    ConnectionTimeout,
    /// Authentication failed
    AuthenticationFailed,
    /// Pairing failed
    PairingFailed,
    /// Protocol error
    ProtocolError,
    /// Invalid parameter
    InvalidParameter,
    /// Not supported
    NotSupported,
    /// Not connected
    NotConnected,
    /// Already connected
    AlreadyConnected,
    /// Resource exhausted
    ResourceExhausted,
    /// Hardware error
    HardwareError,
    /// Invalid state
    InvalidState,
    /// Command disallowed
    CommandDisallowed,
    /// Resource not found
    NotFound,
    /// Operation timeout
    Timeout,
    /// Operation would block
    WouldBlock,
}

/// Bluetooth device address (6 bytes)
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct BdAddr(pub [u8; 6]);

impl BdAddr {
    /// Zero address constant
    pub const ZERO: Self = Self([0u8; 6]);

    /// Create from bytes
    pub fn from_bytes(bytes: [u8; 6]) -> Self {
        Self(bytes)
    }

    /// Get bytes
    pub fn to_bytes(&self) -> [u8; 6] {
        self.0
    }

    /// Check if address is all zeros
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 6]
    }

    /// Check if this is a random address
    pub fn is_random(&self) -> bool {
        (self.0[5] & 0xC0) == 0xC0
    }

    /// Check if this is a public address
    pub fn is_public(&self) -> bool {
        !self.is_random()
    }
}

impl core::fmt::Display for BdAddr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            self.0[5], self.0[4], self.0[3],
            self.0[2], self.0[1], self.0[0])
    }
}

/// Address type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressType {
    /// BR/EDR (Classic Bluetooth)
    BrEdr,
    /// LE Public
    LePublic,
    /// LE Random
    LeRandom,
}

/// Bluetooth device class (24 bits)
#[derive(Clone, Copy, Debug, Default)]
pub struct DeviceClass(pub u32);

impl DeviceClass {
    /// Major device class
    pub fn major(&self) -> MajorDeviceClass {
        match (self.0 >> 8) & 0x1F {
            0 => MajorDeviceClass::Miscellaneous,
            1 => MajorDeviceClass::Computer,
            2 => MajorDeviceClass::Phone,
            3 => MajorDeviceClass::LanAccessPoint,
            4 => MajorDeviceClass::AudioVideo,
            5 => MajorDeviceClass::Peripheral,
            6 => MajorDeviceClass::Imaging,
            7 => MajorDeviceClass::Wearable,
            8 => MajorDeviceClass::Toy,
            9 => MajorDeviceClass::Health,
            _ => MajorDeviceClass::Uncategorized,
        }
    }

    /// Minor device class (depends on major)
    pub fn minor(&self) -> u8 {
        ((self.0 >> 2) & 0x3F) as u8
    }

    /// Service classes
    pub fn services(&self) -> u16 {
        ((self.0 >> 13) & 0x7FF) as u16
    }
}

/// Major device class
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MajorDeviceClass {
    Miscellaneous,
    Computer,
    Phone,
    LanAccessPoint,
    AudioVideo,
    Peripheral,
    Imaging,
    Wearable,
    Toy,
    Health,
    Uncategorized,
}

/// Bluetooth device info
#[derive(Clone, Debug)]
pub struct DeviceInfo {
    /// Device address
    pub address: BdAddr,
    /// Address type
    pub address_type: AddressType,
    /// Device name
    pub name: String,
    /// Device class
    pub class: DeviceClass,
    /// Is paired
    pub paired: bool,
    /// Is connected
    pub connected: bool,
    /// Is trusted
    pub trusted: bool,
    /// Is blocked
    pub blocked: bool,
    /// RSSI (signal strength)
    pub rssi: i8,
    /// TX power level
    pub tx_power: i8,
    /// Manufacturer data
    pub manufacturer_data: Vec<(u16, Vec<u8>)>,
    /// Service UUIDs
    pub service_uuids: Vec<Uuid>,
    /// Service data
    pub service_data: Vec<(Uuid, Vec<u8>)>,
    /// Last seen timestamp
    pub last_seen: u64,
}

impl DeviceInfo {
    pub fn new(address: BdAddr) -> Self {
        Self {
            address,
            address_type: AddressType::BrEdr,
            name: String::new(),
            class: DeviceClass::default(),
            paired: false,
            connected: false,
            trusted: false,
            blocked: false,
            rssi: 0,
            tx_power: 0,
            manufacturer_data: Vec::new(),
            service_uuids: Vec::new(),
            service_data: Vec::new(),
            last_seen: 0,
        }
    }
}

/// Bluetooth UUID (128 bits)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Uuid {
    pub bytes: [u8; 16],
}

/// Type alias for compatibility
pub type UUID = Uuid;

impl Uuid {
    // Well-known UUIDs as associated constants
    pub const SDP: Self = Self::from_u16_const(0x0001);
    pub const RFCOMM: Self = Self::from_u16_const(0x0003);
    pub const OBEX: Self = Self::from_u16_const(0x0008);
    pub const L2CAP: Self = Self::from_u16_const(0x0100);
    pub const BNEP: Self = Self::from_u16_const(0x000F);
    pub const HIDP: Self = Self::from_u16_const(0x0011);
    pub const AVDTP: Self = Self::from_u16_const(0x0019);
    pub const AVCTP: Self = Self::from_u16_const(0x0017);
    pub const SERIAL_PORT: Self = Self::from_u16_const(0x1101);
    pub const OBEX_OPP: Self = Self::from_u16_const(0x1105);
    pub const OBEX_FTP: Self = Self::from_u16_const(0x1106);
    pub const HEADSET: Self = Self::from_u16_const(0x1108);
    pub const HEADSET_AG: Self = Self::from_u16_const(0x1112);
    pub const AUDIO_SOURCE: Self = Self::from_u16_const(0x110A);
    pub const AUDIO_SINK: Self = Self::from_u16_const(0x110B);
    pub const A2DP_SOURCE: Self = Self::from_u16_const(0x110A);
    pub const A2DP_SINK: Self = Self::from_u16_const(0x110B);
    pub const AV_REMOTE: Self = Self::from_u16_const(0x110E);
    pub const AV_REMOTE_TARGET: Self = Self::from_u16_const(0x110C);
    pub const HANDS_FREE: Self = Self::from_u16_const(0x111E);
    pub const HANDS_FREE_AG: Self = Self::from_u16_const(0x111F);
    pub const HID: Self = Self::from_u16_const(0x1124);
    pub const PNP_INFO: Self = Self::from_u16_const(0x1200);

    /// Const version of from_u16 for static initialization
    pub const fn from_u16_const(uuid: u16) -> Self {
        Self {
            bytes: [
                0x00, 0x00, (uuid >> 8) as u8, uuid as u8,
                0x00, 0x00, 0x10, 0x00,
                0x80, 0x00, 0x00, 0x80, 0x5F, 0x9B, 0x34, 0xFB,
            ],
        }
    }

    /// Create from 16-bit UUID (Bluetooth Base UUID)
    pub fn from_u16(uuid: u16) -> Self {
        Self::from_u16_const(uuid)
    }

    /// Create from 32-bit UUID
    pub fn from_u32(uuid: u32) -> Self {
        let mut bytes = [0u8; 16];
        bytes[0..4].copy_from_slice(&uuid.to_be_bytes());
        bytes[4..8].copy_from_slice(&[0x00, 0x00, 0x10, 0x00]);
        bytes[8..16].copy_from_slice(&[0x80, 0x00, 0x00, 0x80, 0x5F, 0x9B, 0x34, 0xFB]);
        Self { bytes }
    }

    /// Create from 128-bit bytes
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self { bytes }
    }

    /// Try to get 16-bit representation
    pub fn to_u16(&self) -> Option<u16> {
        // Check if it's a standard Bluetooth UUID
        if &self.bytes[4..16] == &[0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5F, 0x9B, 0x34, 0xFB] {
            if self.bytes[0] == 0 && self.bytes[1] == 0 {
                return Some(((self.bytes[2] as u16) << 8) | (self.bytes[3] as u16));
            }
        }
        None
    }

    /// Try to get 32-bit representation
    pub fn to_u32(&self) -> Option<u32> {
        // Check if it's a standard Bluetooth UUID
        if &self.bytes[4..16] == &[0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5F, 0x9B, 0x34, 0xFB] {
            return Some(u32::from_be_bytes([self.bytes[0], self.bytes[1], self.bytes[2], self.bytes[3]]));
        }
        None
    }

    /// Check if this is based on the Bluetooth Base UUID
    pub fn is_base_uuid(&self) -> bool {
        &self.bytes[4..16] == &[0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5F, 0x9B, 0x34, 0xFB]
    }
}

impl core::fmt::Display for Uuid {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            self.bytes[0], self.bytes[1], self.bytes[2], self.bytes[3],
            self.bytes[4], self.bytes[5],
            self.bytes[6], self.bytes[7],
            self.bytes[8], self.bytes[9],
            self.bytes[10], self.bytes[11], self.bytes[12], self.bytes[13], self.bytes[14], self.bytes[15])
    }
}

/// Well-known Bluetooth UUIDs
pub mod uuid {
    use super::Uuid;

    pub const SDP: Uuid = Uuid::from_u16_const(0x0001);
    pub const RFCOMM: Uuid = Uuid::from_u16_const(0x0003);
    pub const OBEX: Uuid = Uuid::from_u16_const(0x0008);
    pub const L2CAP: Uuid = Uuid::from_u16_const(0x0100);
    pub const BNEP: Uuid = Uuid::from_u16_const(0x000F);
    pub const HIDP: Uuid = Uuid::from_u16_const(0x0011);
    pub const AVDTP: Uuid = Uuid::from_u16_const(0x0019);
    pub const AVCTP: Uuid = Uuid::from_u16_const(0x0017);

    pub const SERIAL_PORT: Uuid = Uuid::from_u16_const(0x1101);
    pub const OBEX_OPP: Uuid = Uuid::from_u16_const(0x1105);
    pub const OBEX_FTP: Uuid = Uuid::from_u16_const(0x1106);
    pub const HEADSET: Uuid = Uuid::from_u16_const(0x1108);
    pub const HEADSET_AG: Uuid = Uuid::from_u16_const(0x1112);
    pub const AUDIO_SOURCE: Uuid = Uuid::from_u16_const(0x110A);
    pub const AUDIO_SINK: Uuid = Uuid::from_u16_const(0x110B);
    pub const AV_REMOTE: Uuid = Uuid::from_u16_const(0x110E);
    pub const AV_REMOTE_TARGET: Uuid = Uuid::from_u16_const(0x110C);
    pub const HANDS_FREE: Uuid = Uuid::from_u16_const(0x111E);
    pub const HANDS_FREE_AG: Uuid = Uuid::from_u16_const(0x111F);
    pub const HID: Uuid = Uuid::from_u16_const(0x1124);
    pub const PNP_INFO: Uuid = Uuid::from_u16_const(0x1200);

    // LE GATT services
    pub const GENERIC_ACCESS: Uuid = Uuid::from_u16_const(0x1800);
    pub const GENERIC_ATTRIBUTE: Uuid = Uuid::from_u16_const(0x1801);
    pub const IMMEDIATE_ALERT: Uuid = Uuid::from_u16_const(0x1802);
    pub const LINK_LOSS: Uuid = Uuid::from_u16_const(0x1803);
    pub const TX_POWER: Uuid = Uuid::from_u16_const(0x1804);
    pub const HEART_RATE: Uuid = Uuid::from_u16_const(0x180D);
    pub const BATTERY: Uuid = Uuid::from_u16_const(0x180F);
    pub const BLOOD_PRESSURE: Uuid = Uuid::from_u16_const(0x1810);
    pub const HID_SERVICE: Uuid = Uuid::from_u16_const(0x1812);
}

/// Connection state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
}

/// Connection parameters
#[derive(Clone, Debug)]
pub struct ConnectionParams {
    /// Connection interval minimum (in 1.25ms units)
    pub interval_min: u16,
    /// Connection interval maximum
    pub interval_max: u16,
    /// Slave latency
    pub latency: u16,
    /// Supervision timeout (in 10ms units)
    pub timeout: u16,
}

impl Default for ConnectionParams {
    fn default() -> Self {
        Self {
            interval_min: 6,   // 7.5ms
            interval_max: 40,  // 50ms
            latency: 0,
            timeout: 100,      // 1 second
        }
    }
}

/// Bluetooth adapter/controller
pub struct BluetoothAdapter {
    /// Adapter ID
    pub id: u32,
    /// Adapter name
    pub name: String,
    /// Device address
    pub address: BdAddr,
    /// Is powered on
    pub powered: AtomicBool,
    /// Is discoverable
    pub discoverable: AtomicBool,
    /// Is pairable
    pub pairable: AtomicBool,
    /// Is scanning
    pub scanning: AtomicBool,
    /// Supported features
    pub features: AdapterFeatures,
    /// HCI controller
    hci: Arc<RwLock<HciController>>,
    /// Known devices
    devices: RwLock<BTreeMap<BdAddr, Arc<RwLock<DeviceInfo>>>>,
    /// L2CAP channels
    l2cap_channels: RwLock<Vec<Arc<RwLock<L2capChannel>>>>,
    /// Next L2CAP CID
    next_cid: AtomicU32,
    /// Statistics
    stats: AdapterStats,
}

/// Adapter features
#[derive(Clone, Debug, Default)]
pub struct AdapterFeatures {
    /// Supports BR/EDR
    pub br_edr: bool,
    /// Supports LE
    pub le: bool,
    /// Supports Secure Simple Pairing
    pub ssp: bool,
    /// Supports Secure Connections
    pub secure_connections: bool,
    /// Supports LE Extended Advertising
    pub le_extended_advertising: bool,
    /// Supports LE 2M PHY
    pub le_2m_phy: bool,
    /// Supports LE Coded PHY
    pub le_coded_phy: bool,
    /// Maximum LE advertising data length
    pub max_adv_len: u16,
}

/// Adapter statistics
#[derive(Debug, Default)]
pub struct AdapterStats {
    pub bytes_sent: AtomicU64,
    pub bytes_received: AtomicU64,
    pub packets_sent: AtomicU64,
    pub packets_received: AtomicU64,
    pub connections_established: AtomicU64,
    pub connections_failed: AtomicU64,
    pub errors: AtomicU64,
}

impl BluetoothAdapter {
    /// Create new adapter
    pub fn new(id: u32, hci: HciController) -> Self {
        Self {
            id,
            name: String::from("BuildOS Bluetooth"),
            address: BdAddr::default(),
            powered: AtomicBool::new(false),
            discoverable: AtomicBool::new(false),
            pairable: AtomicBool::new(true),
            scanning: AtomicBool::new(false),
            features: AdapterFeatures::default(),
            hci: Arc::new(RwLock::new(hci)),
            devices: RwLock::new(BTreeMap::new()),
            l2cap_channels: RwLock::new(Vec::new()),
            next_cid: AtomicU32::new(0x0040), // First dynamic CID
            stats: AdapterStats::default(),
        }
    }

    /// Power on adapter
    pub fn power_on(&self) -> Result<(), BluetoothError> {
        // Send HCI reset
        let hci = self.hci.write();
        hci.reset()?;

        // Read local address
        let _addr = hci.read_bd_addr()?;
        // Can't modify self.address through &self, would need RefCell or similar

        // Read local features
        hci.read_local_features()?;

        // Enable scan
        hci.write_scan_enable(0x03)?; // Inquiry + Page scan

        self.powered.store(true, Ordering::Release);
        Ok(())
    }

    /// Power off adapter
    pub fn power_off(&self) -> Result<(), BluetoothError> {
        self.powered.store(false, Ordering::Release);
        Ok(())
    }

    /// Start discovery
    pub fn start_discovery(&self) -> Result<(), BluetoothError> {
        if !self.powered.load(Ordering::Acquire) {
            return Err(BluetoothError::InvalidState);
        }

        let hci = self.hci.write();
        hci.inquiry(0x9E8B33, 8, 0)?; // GIAC, 8*1.28s, unlimited responses

        self.scanning.store(true, Ordering::Release);
        Ok(())
    }

    /// Stop discovery
    pub fn stop_discovery(&self) -> Result<(), BluetoothError> {
        let hci = self.hci.write();
        hci.inquiry_cancel()?;

        self.scanning.store(false, Ordering::Release);
        Ok(())
    }

    /// Start LE scan
    pub fn start_le_scan(&self) -> Result<(), BluetoothError> {
        if !self.powered.load(Ordering::Acquire) {
            return Err(BluetoothError::InvalidState);
        }

        let hci = self.hci.write();
        hci.le_set_scan_parameters(0x01, 0x0010, 0x0010, 0x00, 0x00)?;
        hci.le_set_scan_enable(true, false)?;

        self.scanning.store(true, Ordering::Release);
        Ok(())
    }

    /// Stop LE scan
    pub fn stop_le_scan(&self) -> Result<(), BluetoothError> {
        let hci = self.hci.write();
        hci.le_set_scan_enable(false, false)?;

        self.scanning.store(false, Ordering::Release);
        Ok(())
    }

    /// Connect to device
    pub fn connect(&self, address: &BdAddr) -> Result<u16, BluetoothError> {
        if !self.powered.load(Ordering::Acquire) {
            return Err(BluetoothError::InvalidState);
        }

        let hci = self.hci.write();
        let handle = hci.create_connection(address)?;

        self.stats.connections_established.fetch_add(1, Ordering::Relaxed);
        Ok(handle)
    }

    /// Connect to LE device
    pub fn le_connect(&self, address: &BdAddr, address_type: AddressType) -> Result<u16, BluetoothError> {
        if !self.powered.load(Ordering::Acquire) {
            return Err(BluetoothError::InvalidState);
        }

        let hci = self.hci.write();
        let handle = hci.le_create_connection(address, address_type)?;

        self.stats.connections_established.fetch_add(1, Ordering::Relaxed);
        Ok(handle)
    }

    /// Disconnect
    pub fn disconnect(&self, handle: u16) -> Result<(), BluetoothError> {
        let hci = self.hci.write();
        hci.disconnect(handle, 0x13)?; // Remote user terminated
        Ok(())
    }

    /// Get device info
    pub fn get_device(&self, address: &BdAddr) -> Option<Arc<RwLock<DeviceInfo>>> {
        self.devices.read().get(address).cloned()
    }

    /// Get all devices
    pub fn get_devices(&self) -> Vec<Arc<RwLock<DeviceInfo>>> {
        self.devices.read().values().cloned().collect()
    }

    /// Add or update device
    pub fn update_device(&self, info: DeviceInfo) {
        let address = info.address;
        let mut devices = self.devices.write();
        devices.insert(address, Arc::new(RwLock::new(info)));
    }

    /// Remove device
    pub fn remove_device(&self, address: &BdAddr) -> bool {
        self.devices.write().remove(address).is_some()
    }

    /// Pair with device
    pub fn pair(&self, address: &BdAddr) -> Result<(), BluetoothError> {
        // Initiate pairing via SMP
        if let Some(device) = self.get_device(address) {
            let mut dev = device.write();
            dev.paired = true;
            Ok(())
        } else {
            Err(BluetoothError::DeviceNotFound)
        }
    }

    /// Allocate L2CAP CID
    pub fn alloc_cid(&self) -> u16 {
        self.next_cid.fetch_add(1, Ordering::SeqCst) as u16
    }

    /// Create L2CAP channel
    pub fn create_l2cap_channel(
        &self,
        handle: u16,
        psm: u16,
        is_le: bool,
    ) -> Result<Arc<RwLock<L2capChannel>>, BluetoothError> {
        let scid = self.alloc_cid();
        let channel = L2capChannel::new(scid, handle, psm, is_le);
        let channel = Arc::new(RwLock::new(channel));

        self.l2cap_channels.write().push(channel.clone());
        Ok(channel)
    }

    /// Handle HCI event
    pub fn handle_event(&self, event: &HciEvent) {
        match event.event_code() {
            Some(hci::HciEventCode::InquiryResult) => {
                // Parse inquiry result from params
                if event.params.len() >= 14 {
                    let num_responses = event.params[0];
                    for i in 0..num_responses as usize {
                        let offset = 1 + i * 14;
                        if offset + 14 <= event.params.len() {
                            let mut addr_bytes = [0u8; 6];
                            addr_bytes.copy_from_slice(&event.params[offset..offset + 6]);
                            let address = BdAddr(addr_bytes);
                            let class_bits = u32::from_le_bytes([
                                event.params[offset + 9],
                                event.params[offset + 10],
                                event.params[offset + 11],
                                0,
                            ]);
                            let info = DeviceInfo {
                                address,
                                address_type: AddressType::BrEdr,
                                name: String::new(),
                                class: DeviceClass(class_bits),
                                paired: false,
                                connected: false,
                                trusted: false,
                                blocked: false,
                                rssi: 0,
                                tx_power: 0,
                                manufacturer_data: Vec::new(),
                                service_uuids: Vec::new(),
                                service_data: Vec::new(),
                                last_seen: crate::time::monotonic_ns(),
                            };
                            self.update_device(info);
                        }
                    }
                }
            }
            Some(hci::HciEventCode::ConnectionComplete) => {
                if event.params.len() >= 11 {
                    let status = event.params[0];
                    if status == 0 {
                        let mut addr_bytes = [0u8; 6];
                        addr_bytes.copy_from_slice(&event.params[3..9]);
                        let address = BdAddr(addr_bytes);
                        if let Some(device) = self.get_device(&address) {
                            device.write().connected = true;
                        }
                    }
                }
            }
            Some(hci::HciEventCode::DisconnectionComplete) => {
                // Mark device as disconnected
            }
            Some(hci::HciEventCode::LeMeta) => {
                // Handle LE meta events
                if !event.params.is_empty() {
                    let subevent = event.params[0];
                    if subevent == 0x02 { // LE Advertising Report
                        // Parse advertising reports
                    }
                }
            }
            _ => {}
        }
    }
}

/// Bluetooth subsystem
pub struct BluetoothSubsystem {
    /// Is initialized
    initialized: AtomicBool,
    /// Adapters
    adapters: RwLock<BTreeMap<u32, Arc<RwLock<BluetoothAdapter>>>>,
    /// Next adapter ID
    next_adapter_id: AtomicU32,
    /// Default adapter
    default_adapter: AtomicU32,
}

impl BluetoothSubsystem {
    /// Create new subsystem
    pub const fn new() -> Self {
        Self {
            initialized: AtomicBool::new(false),
            adapters: RwLock::new(BTreeMap::new()),
            next_adapter_id: AtomicU32::new(0),
            default_adapter: AtomicU32::new(0),
        }
    }

    /// Initialize
    pub fn init(&self) -> Result<(), BluetoothError> {
        // Probe for Bluetooth controllers
        self.probe_usb_controllers();
        self.probe_pci_controllers();

        self.initialized.store(true, Ordering::Release);

        let adapters = self.adapters.read();
        crate::kprintln!("[BT] Bluetooth subsystem initialized, {} adapter(s)", adapters.len());

        Ok(())
    }

    /// Probe USB for Bluetooth dongles
    fn probe_usb_controllers(&self) {
        // Look for USB Bluetooth devices (class 0xE0, subclass 0x01, protocol 0x01)
        // This would integrate with the USB subsystem
    }

    /// Probe PCI for Bluetooth controllers
    fn probe_pci_controllers(&self) {
        // Some laptops have PCI Bluetooth controllers
    }

    /// Register adapter
    pub fn register_adapter(&self, mut adapter: BluetoothAdapter) -> u32 {
        let id = self.next_adapter_id.fetch_add(1, Ordering::SeqCst);
        adapter.id = id;

        crate::kprintln!("[BT] Adapter {}: {} ({})",
            id, adapter.name, adapter.address);

        self.adapters.write().insert(id, Arc::new(RwLock::new(adapter)));

        // Set as default if first adapter
        if id == 0 {
            self.default_adapter.store(0, Ordering::Release);
        }

        id
    }

    /// Unregister adapter
    pub fn unregister_adapter(&self, id: u32) {
        self.adapters.write().remove(&id);
    }

    /// Get adapter
    pub fn get_adapter(&self, id: u32) -> Option<Arc<RwLock<BluetoothAdapter>>> {
        self.adapters.read().get(&id).cloned()
    }

    /// Get default adapter
    pub fn get_default_adapter(&self) -> Option<Arc<RwLock<BluetoothAdapter>>> {
        let id = self.default_adapter.load(Ordering::Acquire);
        self.get_adapter(id)
    }

    /// Get all adapters
    pub fn get_adapters(&self) -> Vec<Arc<RwLock<BluetoothAdapter>>> {
        self.adapters.read().values().cloned().collect()
    }

    /// Adapter count
    pub fn adapter_count(&self) -> usize {
        self.adapters.read().len()
    }
}

/// Global Bluetooth subsystem
static BLUETOOTH: BluetoothSubsystem = BluetoothSubsystem::new();

/// Initialize Bluetooth subsystem
pub fn init() {
    if let Err(e) = BLUETOOTH.init() {
        crate::kprintln!("[BT] Initialization failed: {:?}", e);
    }
}

/// Get adapter
pub fn get_adapter(id: u32) -> Option<Arc<RwLock<BluetoothAdapter>>> {
    BLUETOOTH.get_adapter(id)
}

/// Get default adapter
pub fn get_default_adapter() -> Option<Arc<RwLock<BluetoothAdapter>>> {
    BLUETOOTH.get_default_adapter()
}

/// Adapter count
pub fn adapter_count() -> usize {
    BLUETOOTH.adapter_count()
}

/// Register adapter
pub fn register_adapter(adapter: BluetoothAdapter) -> u32 {
    BLUETOOTH.register_adapter(adapter)
}
