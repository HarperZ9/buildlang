//! SMP (Security Manager Protocol)
//!
//! Bluetooth LE pairing and key distribution.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use spin::{Mutex, RwLock};

use super::l2cap::{L2capManager, L2CAP_CID_SMP};
use super::{BdAddr, BluetoothError};

// =============================================================================
// SMP CONSTANTS
// =============================================================================

/// SMP MTU
pub const SMP_MTU: usize = 65;

/// SMP timeout (30 seconds)
pub const SMP_TIMEOUT_MS: u64 = 30000;

// =============================================================================
// SMP COMMAND CODES
// =============================================================================

/// SMP command codes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SmpCode {
    /// Pairing request
    PairingRequest = 0x01,
    /// Pairing response
    PairingResponse = 0x02,
    /// Pairing confirm
    PairingConfirm = 0x03,
    /// Pairing random
    PairingRandom = 0x04,
    /// Pairing failed
    PairingFailed = 0x05,
    /// Encryption information
    EncryptionInformation = 0x06,
    /// Central identification
    CentralIdentification = 0x07,
    /// Identity information
    IdentityInformation = 0x08,
    /// Identity address information
    IdentityAddressInformation = 0x09,
    /// Signing information
    SigningInformation = 0x0A,
    /// Security request
    SecurityRequest = 0x0B,
    /// Pairing public key
    PairingPublicKey = 0x0C,
    /// Pairing DHKey check
    PairingDhKeyCheck = 0x0D,
    /// Pairing keypress notification
    PairingKeypressNotification = 0x0E,
}

// =============================================================================
// SMP ERROR CODES
// =============================================================================

/// SMP error codes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SmpError {
    /// Passkey entry failed
    PasskeyEntryFailed = 0x01,
    /// OOB not available
    OobNotAvailable = 0x02,
    /// Authentication requirements
    AuthenticationRequirements = 0x03,
    /// Confirm value failed
    ConfirmValueFailed = 0x04,
    /// Pairing not supported
    PairingNotSupported = 0x05,
    /// Encryption key size
    EncryptionKeySize = 0x06,
    /// Command not supported
    CommandNotSupported = 0x07,
    /// Unspecified reason
    UnspecifiedReason = 0x08,
    /// Repeated attempts
    RepeatedAttempts = 0x09,
    /// Invalid parameters
    InvalidParameters = 0x0A,
    /// DHKey check failed
    DhKeyCheckFailed = 0x0B,
    /// Numeric comparison failed
    NumericComparisonFailed = 0x0C,
    /// BR/EDR pairing in progress
    BredrPairingInProgress = 0x0D,
    /// Cross-transport key derivation not allowed
    CrossTransportNotAllowed = 0x0E,
}

// =============================================================================
// IO CAPABILITIES
// =============================================================================

/// IO capability
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum IoCapability {
    /// Display only
    DisplayOnly = 0x00,
    /// Display with yes/no
    DisplayYesNo = 0x01,
    /// Keyboard only
    KeyboardOnly = 0x02,
    /// No input, no output
    NoInputNoOutput = 0x03,
    /// Keyboard with display
    KeyboardDisplay = 0x04,
}

/// OOB data flag
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum OobDataFlag {
    /// No OOB data
    NotPresent = 0x00,
    /// OOB data from remote device present
    Present = 0x01,
}

/// Authentication requirements
#[derive(Clone, Copy, Debug, Default)]
pub struct AuthReq {
    /// Bonding flags (0 = no bonding, 1 = bonding)
    pub bonding: bool,
    /// MITM protection required
    pub mitm: bool,
    /// Secure connections
    pub sc: bool,
    /// Keypress notifications
    pub keypress: bool,
    /// CT2 (cross-transport key derivation)
    pub ct2: bool,
}

impl AuthReq {
    /// Parse from byte
    pub fn from_byte(byte: u8) -> Self {
        Self {
            bonding: (byte & 0x03) != 0,
            mitm: (byte & 0x04) != 0,
            sc: (byte & 0x08) != 0,
            keypress: (byte & 0x10) != 0,
            ct2: (byte & 0x20) != 0,
        }
    }

    /// Convert to byte
    pub fn to_byte(&self) -> u8 {
        let mut byte = 0u8;
        if self.bonding {
            byte |= 0x01;
        }
        if self.mitm {
            byte |= 0x04;
        }
        if self.sc {
            byte |= 0x08;
        }
        if self.keypress {
            byte |= 0x10;
        }
        if self.ct2 {
            byte |= 0x20;
        }
        byte
    }
}

/// Key distribution
#[derive(Clone, Copy, Debug, Default)]
pub struct KeyDist {
    /// Encryption key (LTK)
    pub enc_key: bool,
    /// Identity key (IRK)
    pub id_key: bool,
    /// Signature key (CSRK)
    pub sign_key: bool,
    /// Link key
    pub link_key: bool,
}

impl KeyDist {
    /// Parse from byte
    pub fn from_byte(byte: u8) -> Self {
        Self {
            enc_key: (byte & 0x01) != 0,
            id_key: (byte & 0x02) != 0,
            sign_key: (byte & 0x04) != 0,
            link_key: (byte & 0x08) != 0,
        }
    }

    /// Convert to byte
    pub fn to_byte(&self) -> u8 {
        let mut byte = 0u8;
        if self.enc_key {
            byte |= 0x01;
        }
        if self.id_key {
            byte |= 0x02;
        }
        if self.sign_key {
            byte |= 0x04;
        }
        if self.link_key {
            byte |= 0x08;
        }
        byte
    }
}

// =============================================================================
// SMP PAIRING
// =============================================================================

/// Pairing method
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairingMethod {
    /// Just works (no user interaction)
    JustWorks,
    /// Passkey entry
    PasskeyEntry,
    /// Numeric comparison
    NumericComparison,
    /// Out of band
    OutOfBand,
}

/// Pairing state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairingState {
    /// Idle
    Idle,
    /// Waiting for pairing response
    WaitPairingResponse,
    /// Waiting for public key
    WaitPublicKey,
    /// Waiting for confirm
    WaitConfirm,
    /// Waiting for random
    WaitRandom,
    /// Waiting for DHKey check
    WaitDhKeyCheck,
    /// Distributing keys
    KeyDistribution,
    /// Complete
    Complete,
    /// Failed
    Failed,
}

/// Pairing request/response
#[derive(Clone, Debug)]
pub struct PairingParams {
    /// IO capability
    pub io_capability: IoCapability,
    /// OOB data flag
    pub oob_data_flag: OobDataFlag,
    /// Authentication requirements
    pub auth_req: AuthReq,
    /// Maximum encryption key size
    pub max_key_size: u8,
    /// Initiator key distribution
    pub init_key_dist: KeyDist,
    /// Responder key distribution
    pub resp_key_dist: KeyDist,
}

impl PairingParams {
    /// Create default pairing parameters
    pub fn default_params() -> Self {
        Self {
            io_capability: IoCapability::NoInputNoOutput,
            oob_data_flag: OobDataFlag::NotPresent,
            auth_req: AuthReq {
                bonding: true,
                mitm: false,
                sc: true,
                keypress: false,
                ct2: false,
            },
            max_key_size: 16,
            init_key_dist: KeyDist {
                enc_key: true,
                id_key: true,
                sign_key: false,
                link_key: false,
            },
            resp_key_dist: KeyDist {
                enc_key: true,
                id_key: true,
                sign_key: false,
                link_key: false,
            },
        }
    }

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 6 {
            return None;
        }
        Some(Self {
            io_capability: match data[0] {
                0 => IoCapability::DisplayOnly,
                1 => IoCapability::DisplayYesNo,
                2 => IoCapability::KeyboardOnly,
                3 => IoCapability::NoInputNoOutput,
                4 => IoCapability::KeyboardDisplay,
                _ => IoCapability::NoInputNoOutput,
            },
            oob_data_flag: if data[1] == 0 {
                OobDataFlag::NotPresent
            } else {
                OobDataFlag::Present
            },
            auth_req: AuthReq::from_byte(data[2]),
            max_key_size: data[3],
            init_key_dist: KeyDist::from_byte(data[4]),
            resp_key_dist: KeyDist::from_byte(data[5]),
        })
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        alloc::vec![
            self.io_capability as u8,
            self.oob_data_flag as u8,
            self.auth_req.to_byte(),
            self.max_key_size,
            self.init_key_dist.to_byte(),
            self.resp_key_dist.to_byte(),
        ]
    }
}

// =============================================================================
// SECURITY KEYS
// =============================================================================

/// Long Term Key (LTK)
#[derive(Clone, Debug)]
pub struct Ltk {
    /// Key value (16 bytes)
    pub value: [u8; 16],
    /// Encrypted diversifier
    pub ediv: u16,
    /// Random number
    pub rand: u64,
    /// Key size
    pub key_size: u8,
    /// Is authenticated
    pub authenticated: bool,
    /// Is secure connections
    pub secure_connections: bool,
}

impl Ltk {
    /// Create new LTK
    pub fn new(value: [u8; 16], ediv: u16, rand: u64) -> Self {
        Self {
            value,
            ediv,
            rand,
            key_size: 16,
            authenticated: false,
            secure_connections: false,
        }
    }
}

/// Identity Resolving Key (IRK)
#[derive(Clone, Debug)]
pub struct Irk {
    /// Key value (16 bytes)
    pub value: [u8; 16],
    /// Identity address
    pub identity_addr: BdAddr,
    /// Address type
    pub addr_type: u8,
}

impl Irk {
    /// Create new IRK
    pub fn new(value: [u8; 16], identity_addr: BdAddr, addr_type: u8) -> Self {
        Self {
            value,
            identity_addr,
            addr_type,
        }
    }
}

/// Connection Signature Resolving Key (CSRK)
#[derive(Clone, Debug)]
pub struct Csrk {
    /// Key value (16 bytes)
    pub value: [u8; 16],
    /// Sign counter
    pub sign_counter: u32,
    /// Is local key
    pub local: bool,
}

impl Csrk {
    /// Create new CSRK
    pub fn new(value: [u8; 16]) -> Self {
        Self {
            value,
            sign_counter: 0,
            local: true,
        }
    }
}

/// Security keys for a device
#[derive(Clone, Debug, Default)]
pub struct SecurityKeys {
    /// Long Term Key
    pub ltk: Option<Ltk>,
    /// Identity Resolving Key
    pub irk: Option<Irk>,
    /// Connection Signature Resolving Key
    pub csrk: Option<Csrk>,
    /// Remote LTK
    pub remote_ltk: Option<Ltk>,
    /// Remote IRK
    pub remote_irk: Option<Irk>,
    /// Remote CSRK
    pub remote_csrk: Option<Csrk>,
}

// =============================================================================
// SMP SESSION
// =============================================================================

/// SMP session for a connection
pub struct SmpSession {
    /// Connection handle
    pub handle: u16,
    /// Remote address
    pub remote_addr: BdAddr,
    /// Are we initiator
    pub initiator: bool,
    /// Current state
    state: AtomicU8,
    /// Pairing method
    pub method: RwLock<PairingMethod>,
    /// Local pairing parameters
    pub local_params: RwLock<PairingParams>,
    /// Remote pairing parameters
    pub remote_params: RwLock<Option<PairingParams>>,
    /// Local random value
    pub local_random: Mutex<[u8; 16]>,
    /// Remote random value
    pub remote_random: Mutex<[u8; 16]>,
    /// Local confirm value
    pub local_confirm: Mutex<[u8; 16]>,
    /// Remote confirm value
    pub remote_confirm: Mutex<[u8; 16]>,
    /// Passkey
    pub passkey: Mutex<u32>,
    /// Public key (X coordinate)
    pub local_pk_x: Mutex<[u8; 32]>,
    /// Public key (Y coordinate)
    pub local_pk_y: Mutex<[u8; 32]>,
    /// Remote public key (X coordinate)
    pub remote_pk_x: Mutex<[u8; 32]>,
    /// Remote public key (Y coordinate)
    pub remote_pk_y: Mutex<[u8; 32]>,
    /// DHKey
    pub dhkey: Mutex<[u8; 32]>,
    /// MacKey
    pub mackey: Mutex<[u8; 16]>,
    /// LTK
    pub ltk: Mutex<[u8; 16]>,
    /// Encryption key size
    pub enc_key_size: AtomicU8,
    /// Security keys
    pub keys: RwLock<SecurityKeys>,
    /// Is secure connections
    pub secure_connections: AtomicBool,
}

impl SmpSession {
    /// Create new SMP session
    pub fn new(handle: u16, remote_addr: BdAddr, initiator: bool) -> Self {
        Self {
            handle,
            remote_addr,
            initiator,
            state: AtomicU8::new(PairingState::Idle as u8),
            method: RwLock::new(PairingMethod::JustWorks),
            local_params: RwLock::new(PairingParams::default_params()),
            remote_params: RwLock::new(None),
            local_random: Mutex::new([0u8; 16]),
            remote_random: Mutex::new([0u8; 16]),
            local_confirm: Mutex::new([0u8; 16]),
            remote_confirm: Mutex::new([0u8; 16]),
            passkey: Mutex::new(0),
            local_pk_x: Mutex::new([0u8; 32]),
            local_pk_y: Mutex::new([0u8; 32]),
            remote_pk_x: Mutex::new([0u8; 32]),
            remote_pk_y: Mutex::new([0u8; 32]),
            dhkey: Mutex::new([0u8; 32]),
            mackey: Mutex::new([0u8; 16]),
            ltk: Mutex::new([0u8; 16]),
            enc_key_size: AtomicU8::new(16),
            keys: RwLock::new(SecurityKeys::default()),
            secure_connections: AtomicBool::new(false),
        }
    }

    /// Get state
    pub fn state(&self) -> PairingState {
        match self.state.load(Ordering::Acquire) {
            0 => PairingState::Idle,
            1 => PairingState::WaitPairingResponse,
            2 => PairingState::WaitPublicKey,
            3 => PairingState::WaitConfirm,
            4 => PairingState::WaitRandom,
            5 => PairingState::WaitDhKeyCheck,
            6 => PairingState::KeyDistribution,
            7 => PairingState::Complete,
            _ => PairingState::Failed,
        }
    }

    /// Set state
    pub fn set_state(&self, state: PairingState) {
        self.state.store(state as u8, Ordering::Release);
    }

    /// Determine pairing method based on IO capabilities
    pub fn determine_method(&self) -> PairingMethod {
        let local = &self.local_params.read();
        let remote = match self.remote_params.read().as_ref() {
            Some(r) => r.clone(),
            None => return PairingMethod::JustWorks,
        };

        // Check for OOB
        if local.oob_data_flag == OobDataFlag::Present
            || remote.oob_data_flag == OobDataFlag::Present
        {
            return PairingMethod::OutOfBand;
        }

        // Check for MITM requirement
        let mitm_required = local.auth_req.mitm || remote.auth_req.mitm;

        if !mitm_required {
            return PairingMethod::JustWorks;
        }

        // Use IO capability mapping table
        let local_io = local.io_capability;
        let remote_io = remote.io_capability;

        match (local_io, remote_io) {
            (IoCapability::DisplayOnly, IoCapability::DisplayOnly) => PairingMethod::JustWorks,
            (IoCapability::DisplayOnly, IoCapability::DisplayYesNo) => PairingMethod::JustWorks,
            (IoCapability::DisplayOnly, IoCapability::KeyboardOnly) => PairingMethod::PasskeyEntry,
            (IoCapability::DisplayOnly, IoCapability::NoInputNoOutput) => PairingMethod::JustWorks,
            (IoCapability::DisplayOnly, IoCapability::KeyboardDisplay) => {
                PairingMethod::PasskeyEntry
            }

            (IoCapability::DisplayYesNo, IoCapability::DisplayOnly) => PairingMethod::JustWorks,
            (IoCapability::DisplayYesNo, IoCapability::DisplayYesNo) => {
                if local.auth_req.sc && remote.auth_req.sc {
                    PairingMethod::NumericComparison
                } else {
                    PairingMethod::JustWorks
                }
            }
            (IoCapability::DisplayYesNo, IoCapability::KeyboardOnly) => PairingMethod::PasskeyEntry,
            (IoCapability::DisplayYesNo, IoCapability::NoInputNoOutput) => PairingMethod::JustWorks,
            (IoCapability::DisplayYesNo, IoCapability::KeyboardDisplay) => {
                if local.auth_req.sc && remote.auth_req.sc {
                    PairingMethod::NumericComparison
                } else {
                    PairingMethod::PasskeyEntry
                }
            }

            (IoCapability::KeyboardOnly, IoCapability::DisplayOnly) => PairingMethod::PasskeyEntry,
            (IoCapability::KeyboardOnly, IoCapability::DisplayYesNo) => PairingMethod::PasskeyEntry,
            (IoCapability::KeyboardOnly, IoCapability::KeyboardOnly) => PairingMethod::PasskeyEntry,
            (IoCapability::KeyboardOnly, IoCapability::NoInputNoOutput) => PairingMethod::JustWorks,
            (IoCapability::KeyboardOnly, IoCapability::KeyboardDisplay) => {
                PairingMethod::PasskeyEntry
            }

            (IoCapability::NoInputNoOutput, _) => PairingMethod::JustWorks,

            (IoCapability::KeyboardDisplay, IoCapability::DisplayOnly) => {
                PairingMethod::PasskeyEntry
            }
            (IoCapability::KeyboardDisplay, IoCapability::DisplayYesNo) => {
                if local.auth_req.sc && remote.auth_req.sc {
                    PairingMethod::NumericComparison
                } else {
                    PairingMethod::PasskeyEntry
                }
            }
            (IoCapability::KeyboardDisplay, IoCapability::KeyboardOnly) => {
                PairingMethod::PasskeyEntry
            }
            (IoCapability::KeyboardDisplay, IoCapability::NoInputNoOutput) => {
                PairingMethod::JustWorks
            }
            (IoCapability::KeyboardDisplay, IoCapability::KeyboardDisplay) => {
                if local.auth_req.sc && remote.auth_req.sc {
                    PairingMethod::NumericComparison
                } else {
                    PairingMethod::PasskeyEntry
                }
            }
        }
    }
}

// =============================================================================
// SMP MANAGER
// =============================================================================

/// SMP manager
pub struct SmpManager {
    /// L2CAP manager reference
    l2cap: Arc<RwLock<L2capManager>>,
    /// Active sessions
    sessions: RwLock<BTreeMap<u16, Arc<RwLock<SmpSession>>>>,
    /// Bonded devices
    bonded_devices: RwLock<BTreeMap<BdAddr, SecurityKeys>>,
    /// IO capability
    io_capability: RwLock<IoCapability>,
    /// Pairing callback (passkey display/input)
    pairing_callback: RwLock<Option<Box<dyn Fn(u16, PairingMethod, u32) + Send + Sync>>>,
    /// Bond callback
    bond_callback: RwLock<Option<Box<dyn Fn(BdAddr, bool) + Send + Sync>>>,
}

impl SmpManager {
    /// Create new SMP manager
    pub fn new(l2cap: Arc<RwLock<L2capManager>>) -> Self {
        Self {
            l2cap,
            sessions: RwLock::new(BTreeMap::new()),
            bonded_devices: RwLock::new(BTreeMap::new()),
            io_capability: RwLock::new(IoCapability::NoInputNoOutput),
            pairing_callback: RwLock::new(None),
            bond_callback: RwLock::new(None),
        }
    }

    /// Set IO capability
    pub fn set_io_capability(&self, cap: IoCapability) {
        *self.io_capability.write() = cap;
    }

    /// Set pairing callback
    pub fn set_pairing_callback<F>(&self, callback: F)
    where
        F: Fn(u16, PairingMethod, u32) + Send + Sync + 'static,
    {
        *self.pairing_callback.write() = Some(Box::new(callback));
    }

    /// Set bond callback
    pub fn set_bond_callback<F>(&self, callback: F)
    where
        F: Fn(BdAddr, bool) + Send + Sync + 'static,
    {
        *self.bond_callback.write() = Some(Box::new(callback));
    }

    /// Get or create session
    fn get_or_create_session(
        &self,
        handle: u16,
        remote_addr: BdAddr,
        initiator: bool,
    ) -> Arc<RwLock<SmpSession>> {
        let mut sessions = self.sessions.write();
        if let Some(session) = sessions.get(&handle) {
            return session.clone();
        }

        let session = Arc::new(RwLock::new(SmpSession::new(handle, remote_addr, initiator)));

        // Set IO capability
        {
            let session_guard = session.write();
            let mut params = session_guard.local_params.write();
            params.io_capability = *self.io_capability.read();
        }

        sessions.insert(handle, session.clone());
        session
    }

    /// Remove session
    fn remove_session(&self, handle: u16) {
        self.sessions.write().remove(&handle);
    }

    /// Initiate pairing
    pub fn pair(&self, handle: u16, remote_addr: BdAddr) -> Result<(), BluetoothError> {
        let session = self.get_or_create_session(handle, remote_addr, true);

        // Build pairing request
        let params = session.read().local_params.read().clone();
        let mut data = Vec::with_capacity(7);
        data.push(SmpCode::PairingRequest as u8);
        data.extend_from_slice(&params.to_bytes());

        // Send via L2CAP
        self.send_smp(handle, &data)?;

        session.write().set_state(PairingState::WaitPairingResponse);

        Ok(())
    }

    /// Send security request (peripheral)
    pub fn security_request(&self, handle: u16, remote_addr: BdAddr) -> Result<(), BluetoothError> {
        let session = self.get_or_create_session(handle, remote_addr, false);

        let auth_req = session.read().local_params.read().auth_req.to_byte();

        let data = alloc::vec![SmpCode::SecurityRequest as u8, auth_req];
        self.send_smp(handle, &data)?;

        Ok(())
    }

    /// Handle incoming SMP packet
    pub fn handle_smp(&self, handle: u16, remote_addr: BdAddr, data: &[u8]) {
        if data.is_empty() {
            return;
        }

        let code = data[0];
        let payload = &data[1..];

        match code {
            x if x == SmpCode::PairingRequest as u8 => {
                self.handle_pairing_request(handle, remote_addr, payload);
            }
            x if x == SmpCode::PairingResponse as u8 => {
                self.handle_pairing_response(handle, payload);
            }
            x if x == SmpCode::PairingConfirm as u8 => {
                self.handle_pairing_confirm(handle, payload);
            }
            x if x == SmpCode::PairingRandom as u8 => {
                self.handle_pairing_random(handle, payload);
            }
            x if x == SmpCode::PairingFailed as u8 => {
                self.handle_pairing_failed(handle, payload);
            }
            x if x == SmpCode::EncryptionInformation as u8 => {
                self.handle_encryption_info(handle, payload);
            }
            x if x == SmpCode::CentralIdentification as u8 => {
                self.handle_central_id(handle, payload);
            }
            x if x == SmpCode::IdentityInformation as u8 => {
                self.handle_identity_info(handle, payload);
            }
            x if x == SmpCode::IdentityAddressInformation as u8 => {
                self.handle_identity_addr_info(handle, payload);
            }
            x if x == SmpCode::SigningInformation as u8 => {
                self.handle_signing_info(handle, payload);
            }
            x if x == SmpCode::SecurityRequest as u8 => {
                self.handle_security_request(handle, remote_addr, payload);
            }
            x if x == SmpCode::PairingPublicKey as u8 => {
                self.handle_pairing_public_key(handle, payload);
            }
            x if x == SmpCode::PairingDhKeyCheck as u8 => {
                self.handle_pairing_dhkey_check(handle, payload);
            }
            x if x == SmpCode::PairingKeypressNotification as u8 => {
                self.handle_keypress_notification(handle, payload);
            }
            _ => {
                // Unknown command
                self.send_pairing_failed(handle, SmpError::CommandNotSupported);
            }
        }
    }

    /// Handle pairing request
    fn handle_pairing_request(&self, handle: u16, remote_addr: BdAddr, data: &[u8]) {
        let remote_params = match PairingParams::from_bytes(data) {
            Some(p) => p,
            None => {
                self.send_pairing_failed(handle, SmpError::InvalidParameters);
                return;
            }
        };

        let session = self.get_or_create_session(handle, remote_addr, false);

        // Store remote params
        *session.write().remote_params.write() = Some(remote_params);

        // Determine pairing method
        let method = session.read().determine_method();
        *session.write().method.write() = method;

        // Check for secure connections
        let use_sc = session.read().local_params.read().auth_req.sc
            && session
                .read()
                .remote_params
                .read()
                .as_ref()
                .map_or(false, |p| p.auth_req.sc);
        session
            .write()
            .secure_connections
            .store(use_sc, Ordering::Release);

        // Send pairing response
        let local_params = session.read().local_params.read().clone();
        let mut response = Vec::with_capacity(7);
        response.push(SmpCode::PairingResponse as u8);
        response.extend_from_slice(&local_params.to_bytes());

        if self.send_smp(handle, &response).is_ok() {
            if use_sc {
                session.write().set_state(PairingState::WaitPublicKey);
            } else {
                session.write().set_state(PairingState::WaitConfirm);
            }
        }
    }

    /// Handle pairing response
    fn handle_pairing_response(&self, handle: u16, data: &[u8]) {
        let session = match self.sessions.read().get(&handle).cloned() {
            Some(s) => s,
            None => return,
        };

        let remote_params = match PairingParams::from_bytes(data) {
            Some(p) => p,
            None => {
                self.send_pairing_failed(handle, SmpError::InvalidParameters);
                return;
            }
        };

        // Store remote params
        *session.write().remote_params.write() = Some(remote_params.clone());

        // Determine pairing method
        let method = session.read().determine_method();
        *session.write().method.write() = method;

        // Check for secure connections
        let use_sc = session.read().local_params.read().auth_req.sc && remote_params.auth_req.sc;
        session
            .write()
            .secure_connections
            .store(use_sc, Ordering::Release);

        if use_sc {
            // Generate and send public key
            self.send_public_key(handle);
            session.write().set_state(PairingState::WaitPublicKey);
        } else {
            // Legacy pairing - generate and send confirm
            self.generate_confirm(handle);
            self.send_confirm(handle);
            session.write().set_state(PairingState::WaitConfirm);
        }
    }

    /// Handle pairing confirm
    fn handle_pairing_confirm(&self, handle: u16, data: &[u8]) {
        let session = match self.sessions.read().get(&handle).cloned() {
            Some(s) => s,
            None => return,
        };

        if data.len() < 16 {
            self.send_pairing_failed(handle, SmpError::InvalidParameters);
            return;
        }

        // Store remote confirm
        let mut confirm = [0u8; 16];
        confirm.copy_from_slice(&data[..16]);
        *session.write().remote_confirm.lock() = confirm;

        if session.read().initiator {
            // Initiator: send random
            self.send_random(handle);
            session.write().set_state(PairingState::WaitRandom);
        } else {
            // Responder: generate and send confirm, then wait for random
            self.generate_confirm(handle);
            self.send_confirm(handle);
            session.write().set_state(PairingState::WaitRandom);
        }
    }

    /// Handle pairing random
    fn handle_pairing_random(&self, handle: u16, data: &[u8]) {
        let session = match self.sessions.read().get(&handle).cloned() {
            Some(s) => s,
            None => return,
        };

        if data.len() < 16 {
            self.send_pairing_failed(handle, SmpError::InvalidParameters);
            return;
        }

        // Store remote random
        let mut random = [0u8; 16];
        random.copy_from_slice(&data[..16]);
        *session.write().remote_random.lock() = random;

        // Verify confirm value
        if !self.verify_confirm(handle) {
            self.send_pairing_failed(handle, SmpError::ConfirmValueFailed);
            session.write().set_state(PairingState::Failed);
            return;
        }

        if !session.read().initiator {
            // Responder: send random
            self.send_random(handle);
        }

        // Calculate STK/LTK
        self.calculate_keys(handle);

        // Start encryption (would trigger HCI LE Start Encryption)
        session.write().set_state(PairingState::KeyDistribution);

        // Distribute keys
        self.distribute_keys(handle);
    }

    /// Handle pairing failed
    fn handle_pairing_failed(&self, handle: u16, _data: &[u8]) {
        if let Some(session) = self.sessions.read().get(&handle) {
            session.write().set_state(PairingState::Failed);
        }

        // Notify via callback
        if let Some(callback) = self.bond_callback.read().as_ref() {
            if let Some(session) = self.sessions.read().get(&handle) {
                callback(session.read().remote_addr, false);
            }
        }

        self.remove_session(handle);
    }

    /// Handle encryption information
    fn handle_encryption_info(&self, handle: u16, data: &[u8]) {
        let session = match self.sessions.read().get(&handle).cloned() {
            Some(s) => s,
            None => return,
        };

        if data.len() < 16 {
            return;
        }

        let mut ltk_value = [0u8; 16];
        ltk_value.copy_from_slice(&data[..16]);

        let ltk = Ltk {
            value: ltk_value,
            ediv: 0,
            rand: 0,
            key_size: session.read().enc_key_size.load(Ordering::Acquire),
            authenticated: *session.read().method.read() != PairingMethod::JustWorks,
            secure_connections: session.read().secure_connections.load(Ordering::Acquire),
        };

        session.write().keys.write().remote_ltk = Some(ltk);
    }

    /// Handle central identification
    fn handle_central_id(&self, handle: u16, data: &[u8]) {
        let session = match self.sessions.read().get(&handle).cloned() {
            Some(s) => s,
            None => return,
        };

        if data.len() < 10 {
            return;
        }

        let ediv = u16::from_le_bytes([data[0], data[1]]);
        let rand = u64::from_le_bytes([
            data[2], data[3], data[4], data[5], data[6], data[7], data[8], data[9],
        ]);

        {
            let session_guard = session.write();
            let mut keys = session_guard.keys.write();
            if let Some(ref mut ltk) = keys.remote_ltk {
                ltk.ediv = ediv;
                ltk.rand = rand;
            }
        }
    }

    /// Handle identity information
    fn handle_identity_info(&self, handle: u16, data: &[u8]) {
        let session = match self.sessions.read().get(&handle).cloned() {
            Some(s) => s,
            None => return,
        };

        if data.len() < 16 {
            return;
        }

        let mut irk_value = [0u8; 16];
        irk_value.copy_from_slice(&data[..16]);

        let irk = Irk {
            value: irk_value,
            identity_addr: BdAddr::ZERO,
            addr_type: 0,
        };

        session.write().keys.write().remote_irk = Some(irk);
    }

    /// Handle identity address information
    fn handle_identity_addr_info(&self, handle: u16, data: &[u8]) {
        let session = match self.sessions.read().get(&handle).cloned() {
            Some(s) => s,
            None => return,
        };

        if data.len() < 7 {
            return;
        }

        let addr_type = data[0];
        let mut addr = [0u8; 6];
        addr.copy_from_slice(&data[1..7]);

        if let Some(ref mut irk) = session.write().keys.write().remote_irk {
            irk.identity_addr = BdAddr(addr);
            irk.addr_type = addr_type;
        }

        // Check if pairing is complete
        self.check_pairing_complete(handle);
    }

    /// Handle signing information
    fn handle_signing_info(&self, handle: u16, data: &[u8]) {
        let session = match self.sessions.read().get(&handle).cloned() {
            Some(s) => s,
            None => return,
        };

        if data.len() < 16 {
            return;
        }

        let mut csrk_value = [0u8; 16];
        csrk_value.copy_from_slice(&data[..16]);

        let csrk = Csrk {
            value: csrk_value,
            sign_counter: 0,
            local: false,
        };

        session.write().keys.write().remote_csrk = Some(csrk);
    }

    /// Handle security request
    fn handle_security_request(&self, handle: u16, remote_addr: BdAddr, data: &[u8]) {
        if data.is_empty() {
            return;
        }

        let auth_req = AuthReq::from_byte(data[0]);

        // Check if we have bonding info
        if let Some(_keys) = self.bonded_devices.read().get(&remote_addr) {
            // Encrypt with existing key
            // In real implementation, would start encryption
        } else if auth_req.bonding {
            // Initiate pairing
            let _ = self.pair(handle, remote_addr);
        }
    }

    /// Handle pairing public key (SC)
    fn handle_pairing_public_key(&self, handle: u16, data: &[u8]) {
        let session = match self.sessions.read().get(&handle).cloned() {
            Some(s) => s,
            None => return,
        };

        if data.len() < 64 {
            self.send_pairing_failed(handle, SmpError::InvalidParameters);
            return;
        }

        // Store remote public key
        let mut pk_x = [0u8; 32];
        let mut pk_y = [0u8; 32];
        pk_x.copy_from_slice(&data[..32]);
        pk_y.copy_from_slice(&data[32..64]);
        *session.write().remote_pk_x.lock() = pk_x;
        *session.write().remote_pk_y.lock() = pk_y;

        if !session.read().initiator {
            // Responder: send our public key
            self.send_public_key(handle);
        }

        // Calculate DHKey
        self.calculate_dhkey(handle);

        // Continue with confirm/random exchange
        let method = *session.read().method.read();
        if method == PairingMethod::JustWorks || method == PairingMethod::NumericComparison {
            // Send confirm
            self.generate_confirm(handle);
            self.send_confirm(handle);
            session.write().set_state(PairingState::WaitConfirm);
        } else {
            // Passkey entry - notify user
            if let Some(callback) = self.pairing_callback.read().as_ref() {
                callback(handle, method, 0);
            }
        }
    }

    /// Handle pairing DHKey check (SC)
    fn handle_pairing_dhkey_check(&self, handle: u16, data: &[u8]) {
        let session = match self.sessions.read().get(&handle).cloned() {
            Some(s) => s,
            None => return,
        };

        if data.len() < 16 {
            self.send_pairing_failed(handle, SmpError::InvalidParameters);
            return;
        }

        // Verify DHKey check
        // In real implementation, would verify the value

        if !session.read().initiator {
            // Responder: send our DHKey check
            self.send_dhkey_check(handle);
        }

        // Pairing successful
        session.write().set_state(PairingState::KeyDistribution);
        self.distribute_keys(handle);
    }

    /// Handle keypress notification
    fn handle_keypress_notification(&self, _handle: u16, _data: &[u8]) {
        // Notification of passkey entry progress
        // Could be used to update UI
    }

    /// Send SMP packet
    fn send_smp(&self, handle: u16, data: &[u8]) -> Result<(), BluetoothError> {
        let channel = self
            .l2cap
            .read()
            .get_fixed_channel(handle, L2CAP_CID_SMP)
            .ok_or(BluetoothError::NotConnected)?;

        // In real implementation, send via L2CAP
        channel.read().send(data.to_vec())?;

        Ok(())
    }

    /// Send pairing failed
    fn send_pairing_failed(&self, handle: u16, error: SmpError) {
        let data = alloc::vec![SmpCode::PairingFailed as u8, error as u8];
        let _ = self.send_smp(handle, &data);

        if let Some(session) = self.sessions.read().get(&handle) {
            session.write().set_state(PairingState::Failed);
        }
    }

    /// Send public key
    fn send_public_key(&self, handle: u16) {
        let session = match self.sessions.read().get(&handle).cloned() {
            Some(s) => s,
            None => return,
        };

        // Generate key pair (in real implementation, use proper crypto)
        // For now, use dummy values
        let pk_x = [0u8; 32];
        let pk_y = [0u8; 32];
        // Would generate ECDH P-256 key pair
        *session.write().local_pk_x.lock() = pk_x;
        *session.write().local_pk_y.lock() = pk_y;

        let mut data = Vec::with_capacity(65);
        data.push(SmpCode::PairingPublicKey as u8);
        data.extend_from_slice(&pk_x);
        data.extend_from_slice(&pk_y);

        let _ = self.send_smp(handle, &data);
    }

    /// Generate confirm value
    fn generate_confirm(&self, handle: u16) {
        let session = match self.sessions.read().get(&handle).cloned() {
            Some(s) => s,
            None => return,
        };

        // Generate random
        let random = [0u8; 16];
        // In real implementation, get from RNG
        *session.write().local_random.lock() = random;

        // Calculate confirm value
        // In real implementation, use proper crypto (c1 function for legacy, f4 for SC)
        let confirm = [0u8; 16];
        *session.write().local_confirm.lock() = confirm;
    }

    /// Send confirm value
    fn send_confirm(&self, handle: u16) {
        let session = match self.sessions.read().get(&handle).cloned() {
            Some(s) => s,
            None => return,
        };

        let confirm = *session.read().local_confirm.lock();

        let mut data = Vec::with_capacity(17);
        data.push(SmpCode::PairingConfirm as u8);
        data.extend_from_slice(&confirm);

        let _ = self.send_smp(handle, &data);
    }

    /// Send random value
    fn send_random(&self, handle: u16) {
        let session = match self.sessions.read().get(&handle).cloned() {
            Some(s) => s,
            None => return,
        };

        let random = *session.read().local_random.lock();

        let mut data = Vec::with_capacity(17);
        data.push(SmpCode::PairingRandom as u8);
        data.extend_from_slice(&random);

        let _ = self.send_smp(handle, &data);
    }

    /// Verify confirm value
    fn verify_confirm(&self, _handle: u16) -> bool {
        // In real implementation, recalculate expected confirm and compare
        true
    }

    /// Calculate DHKey
    fn calculate_dhkey(&self, handle: u16) {
        let session = match self.sessions.read().get(&handle).cloned() {
            Some(s) => s,
            None => return,
        };

        // In real implementation, calculate ECDH shared secret
        let dhkey = [0u8; 32];
        *session.write().dhkey.lock() = dhkey;
    }

    /// Calculate session keys
    fn calculate_keys(&self, handle: u16) {
        let session = match self.sessions.read().get(&handle).cloned() {
            Some(s) => s,
            None => return,
        };

        // In real implementation, derive STK/LTK from random values
        let ltk = [0u8; 16];
        *session.write().ltk.lock() = ltk;
    }

    /// Send DHKey check
    fn send_dhkey_check(&self, handle: u16) {
        let _session = match self.sessions.read().get(&handle).cloned() {
            Some(s) => s,
            None => return,
        };

        // Calculate DHKey check value
        let check = [0u8; 16];

        let mut data = Vec::with_capacity(17);
        data.push(SmpCode::PairingDhKeyCheck as u8);
        data.extend_from_slice(&check);

        let _ = self.send_smp(handle, &data);
    }

    /// Distribute keys
    fn distribute_keys(&self, handle: u16) {
        let session = match self.sessions.read().get(&handle).cloned() {
            Some(s) => s,
            None => return,
        };

        let is_initiator = session.read().initiator;
        let key_dist = if is_initiator {
            session.read().local_params.read().init_key_dist
        } else {
            session.read().local_params.read().resp_key_dist
        };

        // Distribute LTK
        if key_dist.enc_key {
            let ltk = *session.read().ltk.lock();

            let mut data = Vec::with_capacity(17);
            data.push(SmpCode::EncryptionInformation as u8);
            data.extend_from_slice(&ltk);
            let _ = self.send_smp(handle, &data);

            // Send EDIV/Rand
            let mut data = Vec::with_capacity(11);
            data.push(SmpCode::CentralIdentification as u8);
            data.extend_from_slice(&0u16.to_le_bytes());
            data.extend_from_slice(&0u64.to_le_bytes());
            let _ = self.send_smp(handle, &data);
        }

        // Distribute IRK
        if key_dist.id_key {
            // Generate IRK
            let irk = [0u8; 16];

            let mut data = Vec::with_capacity(17);
            data.push(SmpCode::IdentityInformation as u8);
            data.extend_from_slice(&irk);
            let _ = self.send_smp(handle, &data);

            // Send identity address
            let mut data = Vec::with_capacity(8);
            data.push(SmpCode::IdentityAddressInformation as u8);
            data.push(0); // Public address
            data.extend_from_slice(&[0u8; 6]);
            let _ = self.send_smp(handle, &data);
        }

        // Distribute CSRK
        if key_dist.sign_key {
            let csrk = [0u8; 16];

            let mut data = Vec::with_capacity(17);
            data.push(SmpCode::SigningInformation as u8);
            data.extend_from_slice(&csrk);
            let _ = self.send_smp(handle, &data);
        }
    }

    /// Check if pairing is complete
    fn check_pairing_complete(&self, handle: u16) {
        let session = match self.sessions.read().get(&handle).cloned() {
            Some(s) => s,
            None => return,
        };

        // Check if we've received all expected keys
        let remote_key_dist = if session.read().initiator {
            session
                .read()
                .remote_params
                .read()
                .as_ref()
                .map_or(KeyDist::default(), |p| p.resp_key_dist)
        } else {
            session
                .read()
                .remote_params
                .read()
                .as_ref()
                .map_or(KeyDist::default(), |p| p.init_key_dist)
        };

        let keys = session.read().keys.read().clone();

        let complete = (!remote_key_dist.enc_key || keys.remote_ltk.is_some())
            && (!remote_key_dist.id_key || keys.remote_irk.is_some())
            && (!remote_key_dist.sign_key || keys.remote_csrk.is_some());

        if complete {
            session.write().set_state(PairingState::Complete);

            // Store bonding info
            let remote_addr = session.read().remote_addr;
            self.bonded_devices
                .write()
                .insert(remote_addr, keys.clone());

            // Notify via callback
            if let Some(callback) = self.bond_callback.read().as_ref() {
                callback(remote_addr, true);
            }
        }
    }

    /// Enter passkey
    pub fn enter_passkey(&self, handle: u16, passkey: u32) -> Result<(), BluetoothError> {
        let session = self
            .sessions
            .read()
            .get(&handle)
            .cloned()
            .ok_or(BluetoothError::NotFound)?;

        *session.write().passkey.lock() = passkey;

        // Continue pairing
        self.generate_confirm(handle);
        self.send_confirm(handle);
        session.write().set_state(PairingState::WaitConfirm);

        Ok(())
    }

    /// Confirm numeric comparison
    pub fn confirm_numeric(&self, handle: u16, accept: bool) -> Result<(), BluetoothError> {
        if !accept {
            self.send_pairing_failed(handle, SmpError::NumericComparisonFailed);
            return Ok(());
        }

        let session = self
            .sessions
            .read()
            .get(&handle)
            .cloned()
            .ok_or(BluetoothError::NotFound)?;

        // Send DHKey check
        self.send_dhkey_check(handle);
        session.write().set_state(PairingState::WaitDhKeyCheck);

        Ok(())
    }

    /// Get bonded device keys
    pub fn get_bond(&self, addr: &BdAddr) -> Option<SecurityKeys> {
        self.bonded_devices.read().get(addr).cloned()
    }

    /// Remove bond
    pub fn remove_bond(&self, addr: &BdAddr) -> bool {
        self.bonded_devices.write().remove(addr).is_some()
    }

    /// Get all bonded devices
    pub fn bonded_devices(&self) -> Vec<BdAddr> {
        self.bonded_devices.read().keys().cloned().collect()
    }
}
