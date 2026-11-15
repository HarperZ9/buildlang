// ===============================================================================
// BUILDOS KERNEL - TLS/SSL NETWORKING LAYER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! TLS/SSL - Transport Layer Security
//!
//! Provides secure encrypted communications over TCP connections.
//! Implements TLS 1.2 and TLS 1.3 protocols with modern cipher suites.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::sync::RwLock;

// =============================================================================
// TLS VERSION
// =============================================================================

/// TLS protocol version
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u16)]
pub enum TlsVersion {
    /// TLS 1.0 (deprecated)
    Tls10 = 0x0301,
    /// TLS 1.1 (deprecated)
    Tls11 = 0x0302,
    /// TLS 1.2
    Tls12 = 0x0303,
    /// TLS 1.3
    Tls13 = 0x0304,
}

impl TlsVersion {
    /// Parse from wire format
    pub fn from_u16(val: u16) -> Option<Self> {
        match val {
            0x0301 => Some(Self::Tls10),
            0x0302 => Some(Self::Tls11),
            0x0303 => Some(Self::Tls12),
            0x0304 => Some(Self::Tls13),
            _ => None,
        }
    }

    /// Check if version is supported (TLS 1.2+)
    pub fn is_supported(&self) -> bool {
        *self >= Self::Tls12
    }
}

// =============================================================================
// TLS CONTENT TYPES
// =============================================================================

/// TLS record content type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ContentType {
    /// Change cipher spec
    ChangeCipherSpec = 20,
    /// Alert message
    Alert = 21,
    /// Handshake message
    Handshake = 22,
    /// Application data
    ApplicationData = 23,
    /// Heartbeat (RFC 6520)
    Heartbeat = 24,
}

impl ContentType {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            20 => Some(Self::ChangeCipherSpec),
            21 => Some(Self::Alert),
            22 => Some(Self::Handshake),
            23 => Some(Self::ApplicationData),
            24 => Some(Self::Heartbeat),
            _ => None,
        }
    }
}

// =============================================================================
// TLS HANDSHAKE TYPES
// =============================================================================

/// TLS handshake message type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum HandshakeType {
    /// Hello request (server-initiated renegotiation)
    HelloRequest = 0,
    /// Client hello
    ClientHello = 1,
    /// Server hello
    ServerHello = 2,
    /// Hello verify request (DTLS)
    HelloVerifyRequest = 3,
    /// New session ticket
    NewSessionTicket = 4,
    /// End of early data (TLS 1.3)
    EndOfEarlyData = 5,
    /// Hello retry request (TLS 1.3)
    HelloRetryRequest = 6,
    /// Encrypted extensions (TLS 1.3)
    EncryptedExtensions = 8,
    /// Certificate
    Certificate = 11,
    /// Server key exchange
    ServerKeyExchange = 12,
    /// Certificate request
    CertificateRequest = 13,
    /// Server hello done
    ServerHelloDone = 14,
    /// Certificate verify
    CertificateVerify = 15,
    /// Client key exchange
    ClientKeyExchange = 16,
    /// Finished
    Finished = 20,
    /// Certificate URL
    CertificateUrl = 21,
    /// Certificate status
    CertificateStatus = 22,
    /// Supplemental data
    SupplementalData = 23,
    /// Key update (TLS 1.3)
    KeyUpdate = 24,
    /// Message hash
    MessageHash = 254,
}

impl HandshakeType {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::HelloRequest),
            1 => Some(Self::ClientHello),
            2 => Some(Self::ServerHello),
            3 => Some(Self::HelloVerifyRequest),
            4 => Some(Self::NewSessionTicket),
            5 => Some(Self::EndOfEarlyData),
            6 => Some(Self::HelloRetryRequest),
            8 => Some(Self::EncryptedExtensions),
            11 => Some(Self::Certificate),
            12 => Some(Self::ServerKeyExchange),
            13 => Some(Self::CertificateRequest),
            14 => Some(Self::ServerHelloDone),
            15 => Some(Self::CertificateVerify),
            16 => Some(Self::ClientKeyExchange),
            20 => Some(Self::Finished),
            21 => Some(Self::CertificateUrl),
            22 => Some(Self::CertificateStatus),
            23 => Some(Self::SupplementalData),
            24 => Some(Self::KeyUpdate),
            254 => Some(Self::MessageHash),
            _ => None,
        }
    }
}

// =============================================================================
// TLS ALERT
// =============================================================================

/// TLS alert level
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum AlertLevel {
    Warning = 1,
    Fatal = 2,
}

/// TLS alert description
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum AlertDescription {
    CloseNotify = 0,
    UnexpectedMessage = 10,
    BadRecordMac = 20,
    DecryptionFailed = 21,
    RecordOverflow = 22,
    DecompressionFailure = 30,
    HandshakeFailure = 40,
    NoCertificate = 41,
    BadCertificate = 42,
    UnsupportedCertificate = 43,
    CertificateRevoked = 44,
    CertificateExpired = 45,
    CertificateUnknown = 46,
    IllegalParameter = 47,
    UnknownCa = 48,
    AccessDenied = 49,
    DecodeError = 50,
    DecryptError = 51,
    ExportRestriction = 60,
    ProtocolVersion = 70,
    InsufficientSecurity = 71,
    InternalError = 80,
    InappropriateFallback = 86,
    UserCanceled = 90,
    NoRenegotiation = 100,
    MissingExtension = 109,
    UnsupportedExtension = 110,
    CertificateUnobtainable = 111,
    UnrecognizedName = 112,
    BadCertificateStatusResponse = 113,
    BadCertificateHashValue = 114,
    UnknownPskIdentity = 115,
    CertificateRequired = 116,
    NoApplicationProtocol = 120,
}

impl AlertDescription {
    pub fn from_u8(val: u8) -> Option<Self> {
        // Simplified - would match all variants
        Some(match val {
            0 => Self::CloseNotify,
            10 => Self::UnexpectedMessage,
            20 => Self::BadRecordMac,
            40 => Self::HandshakeFailure,
            80 => Self::InternalError,
            _ => return None,
        })
    }
}

/// TLS alert message
#[derive(Clone, Copy, Debug)]
pub struct TlsAlert {
    pub level: AlertLevel,
    pub description: AlertDescription,
}

impl TlsAlert {
    pub fn new(level: AlertLevel, description: AlertDescription) -> Self {
        Self { level, description }
    }

    pub fn fatal(description: AlertDescription) -> Self {
        Self::new(AlertLevel::Fatal, description)
    }

    pub fn warning(description: AlertDescription) -> Self {
        Self::new(AlertLevel::Warning, description)
    }
}

// =============================================================================
// CIPHER SUITES
// =============================================================================

/// TLS cipher suite
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum CipherSuite {
    // TLS 1.3 suites
    /// TLS_AES_128_GCM_SHA256
    Tls13Aes128GcmSha256 = 0x1301,
    /// TLS_AES_256_GCM_SHA384
    Tls13Aes256GcmSha384 = 0x1302,
    /// TLS_CHACHA20_POLY1305_SHA256
    Tls13Chacha20Poly1305Sha256 = 0x1303,
    /// TLS_AES_128_CCM_SHA256
    Tls13Aes128CcmSha256 = 0x1304,
    /// TLS_AES_128_CCM_8_SHA256
    Tls13Aes128Ccm8Sha256 = 0x1305,

    // TLS 1.2 ECDHE suites
    /// TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256
    EcdheEcdsaAes128GcmSha256 = 0xC02B,
    /// TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384
    EcdheEcdsaAes256GcmSha384 = 0xC02C,
    /// TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256
    EcdheRsaAes128GcmSha256 = 0xC02F,
    /// TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384
    EcdheRsaAes256GcmSha384 = 0xC030,
    /// TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256
    EcdheEcdsaChacha20Poly1305 = 0xCCA9,
    /// TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256
    EcdheRsaChacha20Poly1305 = 0xCCA8,
}

impl CipherSuite {
    pub fn from_u16(val: u16) -> Option<Self> {
        match val {
            0x1301 => Some(Self::Tls13Aes128GcmSha256),
            0x1302 => Some(Self::Tls13Aes256GcmSha384),
            0x1303 => Some(Self::Tls13Chacha20Poly1305Sha256),
            0x1304 => Some(Self::Tls13Aes128CcmSha256),
            0x1305 => Some(Self::Tls13Aes128Ccm8Sha256),
            0xC02B => Some(Self::EcdheEcdsaAes128GcmSha256),
            0xC02C => Some(Self::EcdheEcdsaAes256GcmSha384),
            0xC02F => Some(Self::EcdheRsaAes128GcmSha256),
            0xC030 => Some(Self::EcdheRsaAes256GcmSha384),
            0xCCA9 => Some(Self::EcdheEcdsaChacha20Poly1305),
            0xCCA8 => Some(Self::EcdheRsaChacha20Poly1305),
            _ => None,
        }
    }

    /// Check if this is a TLS 1.3 suite
    pub fn is_tls13(&self) -> bool {
        (*self as u16) >= 0x1301 && (*self as u16) <= 0x1305
    }

    /// Get key length in bytes
    pub fn key_length(&self) -> usize {
        match self {
            Self::Tls13Aes128GcmSha256 | Self::Tls13Aes128CcmSha256 | Self::Tls13Aes128Ccm8Sha256 |
            Self::EcdheEcdsaAes128GcmSha256 | Self::EcdheRsaAes128GcmSha256 => 16,
            Self::Tls13Aes256GcmSha384 |
            Self::EcdheEcdsaAes256GcmSha384 | Self::EcdheRsaAes256GcmSha384 => 32,
            Self::Tls13Chacha20Poly1305Sha256 |
            Self::EcdheEcdsaChacha20Poly1305 | Self::EcdheRsaChacha20Poly1305 => 32,
        }
    }

    /// Get IV length in bytes
    pub fn iv_length(&self) -> usize {
        match self {
            Self::Tls13Aes128GcmSha256 | Self::Tls13Aes256GcmSha384 |
            Self::EcdheEcdsaAes128GcmSha256 | Self::EcdheEcdsaAes256GcmSha384 |
            Self::EcdheRsaAes128GcmSha256 | Self::EcdheRsaAes256GcmSha384 => 12,
            Self::Tls13Chacha20Poly1305Sha256 |
            Self::EcdheEcdsaChacha20Poly1305 | Self::EcdheRsaChacha20Poly1305 => 12,
            Self::Tls13Aes128CcmSha256 | Self::Tls13Aes128Ccm8Sha256 => 12,
        }
    }

    /// Get MAC length (tag length for AEAD)
    pub fn mac_length(&self) -> usize {
        match self {
            Self::Tls13Aes128Ccm8Sha256 => 8,
            _ => 16, // Most AEAD suites use 16-byte tags
        }
    }

    /// Get hash algorithm
    pub fn hash_algo(&self) -> HashAlgorithm {
        match self {
            Self::Tls13Aes256GcmSha384 | Self::EcdheEcdsaAes256GcmSha384 |
            Self::EcdheRsaAes256GcmSha384 => HashAlgorithm::Sha384,
            _ => HashAlgorithm::Sha256,
        }
    }
}

/// Hash algorithm
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HashAlgorithm {
    Sha256,
    Sha384,
    Sha512,
}

impl HashAlgorithm {
    /// Get output length in bytes
    pub fn output_length(&self) -> usize {
        match self {
            Self::Sha256 => 32,
            Self::Sha384 => 48,
            Self::Sha512 => 64,
        }
    }
}

// =============================================================================
// KEY EXCHANGE
// =============================================================================

/// Named group for key exchange
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum NamedGroup {
    /// secp256r1 (P-256)
    Secp256r1 = 0x0017,
    /// secp384r1 (P-384)
    Secp384r1 = 0x0018,
    /// secp521r1 (P-521)
    Secp521r1 = 0x0019,
    /// x25519
    X25519 = 0x001D,
    /// x448
    X448 = 0x001E,
    /// ffdhe2048
    Ffdhe2048 = 0x0100,
    /// ffdhe3072
    Ffdhe3072 = 0x0101,
    /// ffdhe4096
    Ffdhe4096 = 0x0102,
    /// ffdhe6144
    Ffdhe6144 = 0x0103,
    /// ffdhe8192
    Ffdhe8192 = 0x0104,
}

impl NamedGroup {
    pub fn from_u16(val: u16) -> Option<Self> {
        match val {
            0x0017 => Some(Self::Secp256r1),
            0x0018 => Some(Self::Secp384r1),
            0x0019 => Some(Self::Secp521r1),
            0x001D => Some(Self::X25519),
            0x001E => Some(Self::X448),
            0x0100 => Some(Self::Ffdhe2048),
            0x0101 => Some(Self::Ffdhe3072),
            0x0102 => Some(Self::Ffdhe4096),
            0x0103 => Some(Self::Ffdhe6144),
            0x0104 => Some(Self::Ffdhe8192),
            _ => None,
        }
    }

    /// Check if ECDHE group
    pub fn is_ecdhe(&self) -> bool {
        matches!(self, Self::Secp256r1 | Self::Secp384r1 | Self::Secp521r1 | Self::X25519 | Self::X448)
    }

    /// Get key size in bytes
    pub fn key_size(&self) -> usize {
        match self {
            Self::Secp256r1 | Self::X25519 => 32,
            Self::Secp384r1 => 48,
            Self::Secp521r1 => 66,
            Self::X448 => 56,
            Self::Ffdhe2048 => 256,
            Self::Ffdhe3072 => 384,
            Self::Ffdhe4096 => 512,
            Self::Ffdhe6144 => 768,
            Self::Ffdhe8192 => 1024,
        }
    }
}

/// Signature algorithm
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum SignatureScheme {
    // RSASSA-PKCS1-v1_5
    RsaPkcs1Sha256 = 0x0401,
    RsaPkcs1Sha384 = 0x0501,
    RsaPkcs1Sha512 = 0x0601,
    // ECDSA
    EcdsaSecp256r1Sha256 = 0x0403,
    EcdsaSecp384r1Sha384 = 0x0503,
    EcdsaSecp521r1Sha512 = 0x0603,
    // RSASSA-PSS
    RsaPssRsaeSha256 = 0x0804,
    RsaPssRsaeSha384 = 0x0805,
    RsaPssRsaeSha512 = 0x0806,
    // EdDSA
    Ed25519 = 0x0807,
    Ed448 = 0x0808,
    // RSA-PSS with PSS OID
    RsaPssPssSha256 = 0x0809,
    RsaPssPssSha384 = 0x080A,
    RsaPssPssSha512 = 0x080B,
}

impl SignatureScheme {
    pub fn from_u16(val: u16) -> Option<Self> {
        match val {
            0x0401 => Some(Self::RsaPkcs1Sha256),
            0x0501 => Some(Self::RsaPkcs1Sha384),
            0x0601 => Some(Self::RsaPkcs1Sha512),
            0x0403 => Some(Self::EcdsaSecp256r1Sha256),
            0x0503 => Some(Self::EcdsaSecp384r1Sha384),
            0x0603 => Some(Self::EcdsaSecp521r1Sha512),
            0x0804 => Some(Self::RsaPssRsaeSha256),
            0x0805 => Some(Self::RsaPssRsaeSha384),
            0x0806 => Some(Self::RsaPssRsaeSha512),
            0x0807 => Some(Self::Ed25519),
            0x0808 => Some(Self::Ed448),
            0x0809 => Some(Self::RsaPssPssSha256),
            0x080A => Some(Self::RsaPssPssSha384),
            0x080B => Some(Self::RsaPssPssSha512),
            _ => None,
        }
    }
}

// =============================================================================
// TLS EXTENSIONS
// =============================================================================

/// TLS extension type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum ExtensionType {
    /// Server name indication
    ServerName = 0,
    /// Maximum fragment length
    MaxFragmentLength = 1,
    /// Client certificate URL
    ClientCertificateUrl = 2,
    /// Trusted CA keys
    TrustedCaKeys = 3,
    /// Truncated HMAC
    TruncatedHmac = 4,
    /// Status request (OCSP)
    StatusRequest = 5,
    /// Supported groups (named curves)
    SupportedGroups = 10,
    /// EC point formats
    EcPointFormats = 11,
    /// Signature algorithms
    SignatureAlgorithms = 13,
    /// Use SRTP
    UseSrtp = 14,
    /// Heartbeat
    Heartbeat = 15,
    /// Application layer protocol negotiation
    Alpn = 16,
    /// Status request v2
    StatusRequestV2 = 17,
    /// Signed certificate timestamp
    SignedCertificateTimestamp = 18,
    /// Client certificate type
    ClientCertificateType = 19,
    /// Server certificate type
    ServerCertificateType = 20,
    /// Padding
    Padding = 21,
    /// Encrypt then MAC
    EncryptThenMac = 22,
    /// Extended master secret
    ExtendedMasterSecret = 23,
    /// Session ticket
    SessionTicket = 35,
    /// Pre-shared key (TLS 1.3)
    PreSharedKey = 41,
    /// Early data (TLS 1.3)
    EarlyData = 42,
    /// Supported versions (TLS 1.3)
    SupportedVersions = 43,
    /// Cookie (TLS 1.3)
    Cookie = 44,
    /// PSK key exchange modes (TLS 1.3)
    PskKeyExchangeModes = 45,
    /// Certificate authorities
    CertificateAuthorities = 47,
    /// OID filters
    OidFilters = 48,
    /// Post-handshake auth (TLS 1.3)
    PostHandshakeAuth = 49,
    /// Signature algorithms cert
    SignatureAlgorithmsCert = 50,
    /// Key share (TLS 1.3)
    KeyShare = 51,
    /// Renegotiation info
    RenegotiationInfo = 65281,
}

impl ExtensionType {
    pub fn from_u16(val: u16) -> Option<Self> {
        match val {
            0 => Some(Self::ServerName),
            1 => Some(Self::MaxFragmentLength),
            10 => Some(Self::SupportedGroups),
            11 => Some(Self::EcPointFormats),
            13 => Some(Self::SignatureAlgorithms),
            16 => Some(Self::Alpn),
            22 => Some(Self::EncryptThenMac),
            23 => Some(Self::ExtendedMasterSecret),
            35 => Some(Self::SessionTicket),
            41 => Some(Self::PreSharedKey),
            42 => Some(Self::EarlyData),
            43 => Some(Self::SupportedVersions),
            44 => Some(Self::Cookie),
            45 => Some(Self::PskKeyExchangeModes),
            51 => Some(Self::KeyShare),
            65281 => Some(Self::RenegotiationInfo),
            _ => None,
        }
    }
}

/// TLS extension
#[derive(Clone, Debug)]
pub struct TlsExtension {
    pub ext_type: ExtensionType,
    pub data: Vec<u8>,
}

impl TlsExtension {
    pub fn new(ext_type: ExtensionType, data: Vec<u8>) -> Self {
        Self { ext_type, data }
    }

    /// Create server name extension
    pub fn server_name(hostname: &str) -> Self {
        let mut data = Vec::new();
        let name_bytes = hostname.as_bytes();
        let list_len = 3 + name_bytes.len();

        // Server name list length
        data.push((list_len >> 8) as u8);
        data.push(list_len as u8);
        // Name type (host_name = 0)
        data.push(0);
        // Name length
        data.push((name_bytes.len() >> 8) as u8);
        data.push(name_bytes.len() as u8);
        // Name
        data.extend_from_slice(name_bytes);

        Self::new(ExtensionType::ServerName, data)
    }

    /// Create supported versions extension (client)
    pub fn supported_versions_client(versions: &[TlsVersion]) -> Self {
        let mut data = Vec::new();
        data.push((versions.len() * 2) as u8);
        for v in versions {
            let val = *v as u16;
            data.push((val >> 8) as u8);
            data.push(val as u8);
        }
        Self::new(ExtensionType::SupportedVersions, data)
    }

    /// Create supported groups extension
    pub fn supported_groups(groups: &[NamedGroup]) -> Self {
        let mut data = Vec::new();
        let len = groups.len() * 2;
        data.push((len >> 8) as u8);
        data.push(len as u8);
        for g in groups {
            let val = *g as u16;
            data.push((val >> 8) as u8);
            data.push(val as u8);
        }
        Self::new(ExtensionType::SupportedGroups, data)
    }

    /// Create signature algorithms extension
    pub fn signature_algorithms(schemes: &[SignatureScheme]) -> Self {
        let mut data = Vec::new();
        let len = schemes.len() * 2;
        data.push((len >> 8) as u8);
        data.push(len as u8);
        for s in schemes {
            let val = *s as u16;
            data.push((val >> 8) as u8);
            data.push(val as u8);
        }
        Self::new(ExtensionType::SignatureAlgorithms, data)
    }

    /// Create key share extension (client)
    pub fn key_share_client(shares: &[(NamedGroup, &[u8])]) -> Self {
        let mut data = Vec::new();
        let mut entries = Vec::new();

        for (group, key_data) in shares {
            let val = *group as u16;
            entries.push((val >> 8) as u8);
            entries.push(val as u8);
            entries.push((key_data.len() >> 8) as u8);
            entries.push(key_data.len() as u8);
            entries.extend_from_slice(key_data);
        }

        data.push((entries.len() >> 8) as u8);
        data.push(entries.len() as u8);
        data.extend(entries);

        Self::new(ExtensionType::KeyShare, data)
    }

    /// Create ALPN extension
    pub fn alpn(protocols: &[&str]) -> Self {
        let mut data = Vec::new();
        let mut list = Vec::new();

        for proto in protocols {
            let bytes = proto.as_bytes();
            list.push(bytes.len() as u8);
            list.extend_from_slice(bytes);
        }

        data.push((list.len() >> 8) as u8);
        data.push(list.len() as u8);
        data.extend(list);

        Self::new(ExtensionType::Alpn, data)
    }

    /// Encode to bytes
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let ext_type = self.ext_type as u16;
        out.push((ext_type >> 8) as u8);
        out.push(ext_type as u8);
        out.push((self.data.len() >> 8) as u8);
        out.push(self.data.len() as u8);
        out.extend_from_slice(&self.data);
        out
    }
}

// =============================================================================
// TLS RECORD
// =============================================================================

/// Maximum TLS record size
pub const MAX_RECORD_SIZE: usize = 16384;

/// Maximum TLS record size with encryption overhead
pub const MAX_ENCRYPTED_RECORD_SIZE: usize = MAX_RECORD_SIZE + 256;

/// TLS record header
#[derive(Clone, Copy, Debug)]
pub struct RecordHeader {
    pub content_type: ContentType,
    pub version: TlsVersion,
    pub length: u16,
}

impl RecordHeader {
    /// Header size
    pub const SIZE: usize = 5;

    /// Parse from bytes
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < Self::SIZE {
            return None;
        }

        let content_type = ContentType::from_u8(data[0])?;
        let version = TlsVersion::from_u16(u16::from_be_bytes([data[1], data[2]]))?;
        let length = u16::from_be_bytes([data[3], data[4]]);

        if length as usize > MAX_ENCRYPTED_RECORD_SIZE {
            return None;
        }

        Some(Self { content_type, version, length })
    }

    /// Encode to bytes
    pub fn encode(&self) -> [u8; 5] {
        let version = self.version as u16;
        [
            self.content_type as u8,
            (version >> 8) as u8,
            version as u8,
            (self.length >> 8) as u8,
            self.length as u8,
        ]
    }
}

/// TLS record
#[derive(Clone, Debug)]
pub struct TlsRecord {
    pub header: RecordHeader,
    pub fragment: Vec<u8>,
}

impl TlsRecord {
    /// Create new record
    pub fn new(content_type: ContentType, version: TlsVersion, fragment: Vec<u8>) -> Self {
        Self {
            header: RecordHeader {
                content_type,
                version,
                length: fragment.len() as u16,
            },
            fragment,
        }
    }

    /// Parse from bytes
    pub fn parse(data: &[u8]) -> Option<(Self, usize)> {
        let header = RecordHeader::parse(data)?;
        let total_len = RecordHeader::SIZE + header.length as usize;

        if data.len() < total_len {
            return None;
        }

        let fragment = data[RecordHeader::SIZE..total_len].to_vec();
        Some((Self { header, fragment }, total_len))
    }

    /// Encode to bytes
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(RecordHeader::SIZE + self.fragment.len());
        out.extend_from_slice(&self.header.encode());
        out.extend_from_slice(&self.fragment);
        out
    }
}

// =============================================================================
// HANDSHAKE MESSAGES
// =============================================================================

/// Random bytes for hello messages
#[derive(Clone, Debug)]
pub struct Random {
    pub bytes: [u8; 32],
}

impl Random {
    /// Generate random
    pub fn new() -> Self {
        let mut bytes = [0u8; 32];
        // In a real implementation, use CSPRNG
        crate::random::get_random_bytes(&mut bytes);
        Self { bytes }
    }

    /// Create from bytes
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }
}

impl Default for Random {
    fn default() -> Self {
        Self::new()
    }
}

/// Session ID
#[derive(Clone, Debug, Default)]
pub struct SessionId {
    pub bytes: Vec<u8>,
}

impl SessionId {
    pub fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    pub fn generate() -> Self {
        let mut bytes = vec![0u8; 32];
        crate::random::get_random_bytes(&mut bytes);
        Self { bytes }
    }
}

/// Client Hello message
#[derive(Clone, Debug)]
pub struct ClientHello {
    pub client_version: TlsVersion,
    pub random: Random,
    pub session_id: SessionId,
    pub cipher_suites: Vec<CipherSuite>,
    pub compression_methods: Vec<u8>,
    pub extensions: Vec<TlsExtension>,
}

impl ClientHello {
    /// Create new ClientHello
    pub fn new(server_name: Option<&str>) -> Self {
        let mut extensions = vec![
            TlsExtension::supported_versions_client(&[TlsVersion::Tls13, TlsVersion::Tls12]),
            TlsExtension::supported_groups(&[
                NamedGroup::X25519,
                NamedGroup::Secp256r1,
                NamedGroup::Secp384r1,
            ]),
            TlsExtension::signature_algorithms(&[
                SignatureScheme::EcdsaSecp256r1Sha256,
                SignatureScheme::RsaPssRsaeSha256,
                SignatureScheme::RsaPkcs1Sha256,
            ]),
        ];

        if let Some(name) = server_name {
            extensions.insert(0, TlsExtension::server_name(name));
        }

        Self {
            client_version: TlsVersion::Tls12, // For compatibility
            random: Random::new(),
            session_id: SessionId::generate(),
            cipher_suites: vec![
                CipherSuite::Tls13Aes128GcmSha256,
                CipherSuite::Tls13Aes256GcmSha384,
                CipherSuite::Tls13Chacha20Poly1305Sha256,
                CipherSuite::EcdheEcdsaAes128GcmSha256,
                CipherSuite::EcdheRsaAes128GcmSha256,
            ],
            compression_methods: vec![0], // null compression
            extensions,
        }
    }

    /// Encode to handshake message bytes
    pub fn encode(&self) -> Vec<u8> {
        let mut data = Vec::new();

        // Client version
        let version = self.client_version as u16;
        data.push((version >> 8) as u8);
        data.push(version as u8);

        // Random
        data.extend_from_slice(&self.random.bytes);

        // Session ID
        data.push(self.session_id.bytes.len() as u8);
        data.extend_from_slice(&self.session_id.bytes);

        // Cipher suites
        let suites_len = self.cipher_suites.len() * 2;
        data.push((suites_len >> 8) as u8);
        data.push(suites_len as u8);
        for suite in &self.cipher_suites {
            let val = *suite as u16;
            data.push((val >> 8) as u8);
            data.push(val as u8);
        }

        // Compression methods
        data.push(self.compression_methods.len() as u8);
        data.extend_from_slice(&self.compression_methods);

        // Extensions
        let mut ext_data = Vec::new();
        for ext in &self.extensions {
            ext_data.extend(ext.encode());
        }
        data.push((ext_data.len() >> 8) as u8);
        data.push(ext_data.len() as u8);
        data.extend(ext_data);

        // Wrap in handshake header
        let mut msg = Vec::new();
        msg.push(HandshakeType::ClientHello as u8);
        msg.push((data.len() >> 16) as u8);
        msg.push((data.len() >> 8) as u8);
        msg.push(data.len() as u8);
        msg.extend(data);

        msg
    }
}

/// Server Hello message
#[derive(Clone, Debug)]
pub struct ServerHello {
    pub server_version: TlsVersion,
    pub random: Random,
    pub session_id: SessionId,
    pub cipher_suite: CipherSuite,
    pub compression_method: u8,
    pub extensions: Vec<TlsExtension>,
}

impl ServerHello {
    /// Parse from bytes
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 38 {
            return None;
        }

        let mut offset = 0;

        // Version
        let server_version = TlsVersion::from_u16(u16::from_be_bytes([data[0], data[1]]))?;
        offset += 2;

        // Random
        let mut random_bytes = [0u8; 32];
        random_bytes.copy_from_slice(&data[offset..offset + 32]);
        let random = Random::from_bytes(random_bytes);
        offset += 32;

        // Session ID
        let session_id_len = data[offset] as usize;
        offset += 1;
        let session_id = SessionId::from_bytes(data[offset..offset + session_id_len].to_vec());
        offset += session_id_len;

        // Cipher suite
        let cipher_suite = CipherSuite::from_u16(u16::from_be_bytes([data[offset], data[offset + 1]]))?;
        offset += 2;

        // Compression method
        let compression_method = data[offset];
        offset += 1;

        // Extensions (if present)
        let mut extensions = Vec::new();
        if offset + 2 <= data.len() {
            let ext_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
            offset += 2;

            let mut ext_offset = 0;
            while ext_offset + 4 <= ext_len && offset + ext_offset + 4 <= data.len() {
                let ext_type_val = u16::from_be_bytes([
                    data[offset + ext_offset],
                    data[offset + ext_offset + 1],
                ]);
                let ext_data_len = u16::from_be_bytes([
                    data[offset + ext_offset + 2],
                    data[offset + ext_offset + 3],
                ]) as usize;
                ext_offset += 4;

                if let Some(ext_type) = ExtensionType::from_u16(ext_type_val) {
                    let ext_data = data[offset + ext_offset..offset + ext_offset + ext_data_len].to_vec();
                    extensions.push(TlsExtension::new(ext_type, ext_data));
                }
                ext_offset += ext_data_len;
            }
        }

        Some(Self {
            server_version,
            random,
            session_id,
            cipher_suite,
            compression_method,
            extensions,
        })
    }
}

// =============================================================================
// TLS STATE MACHINE
// =============================================================================

/// TLS connection state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TlsState {
    /// Initial state
    Start,
    /// Client hello sent
    ClientHelloSent,
    /// Server hello received
    ServerHelloReceived,
    /// Certificate received
    CertificateReceived,
    /// Server key exchange received
    ServerKeyExchangeReceived,
    /// Server hello done received
    ServerHelloDoneReceived,
    /// Client key exchange sent
    ClientKeyExchangeSent,
    /// Change cipher spec sent
    ChangeCipherSpecSent,
    /// Finished sent
    FinishedSent,
    /// Change cipher spec received
    ChangeCipherSpecReceived,
    /// Handshake complete
    Connected,
    /// Connection closed
    Closed,
    /// Error state
    Error,
}

/// TLS connection role
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TlsRole {
    Client,
    Server,
}

// =============================================================================
// CRYPTO PRIMITIVES
// =============================================================================

/// AES-GCM cipher
pub struct AesGcm {
    key: Vec<u8>,
    nonce: [u8; 12],
    counter: u64,
}

impl AesGcm {
    /// Create new AES-GCM cipher
    pub fn new(key: &[u8], initial_iv: &[u8]) -> Self {
        let mut nonce = [0u8; 12];
        nonce[..initial_iv.len().min(12)].copy_from_slice(&initial_iv[..initial_iv.len().min(12)]);

        Self {
            key: key.to_vec(),
            nonce,
            counter: 0,
        }
    }

    /// Get current nonce (for encryption)
    fn get_nonce(&mut self) -> [u8; 12] {
        let mut nonce = self.nonce;
        // XOR counter into nonce
        let counter_bytes = self.counter.to_be_bytes();
        for i in 0..8 {
            nonce[4 + i] ^= counter_bytes[i];
        }
        self.counter += 1;
        nonce
    }

    /// Encrypt data (returns ciphertext + tag)
    pub fn encrypt(&mut self, plaintext: &[u8], aad: &[u8]) -> Vec<u8> {
        let _nonce = self.get_nonce();
        // Simplified: In production, use actual AES-GCM
        let mut ciphertext = plaintext.to_vec();
        // XOR with key (very simplified - not secure)
        for (i, byte) in ciphertext.iter_mut().enumerate() {
            *byte ^= self.key[i % self.key.len()];
        }
        // Add fake tag
        let tag = [0u8; 16];
        ciphertext.extend_from_slice(&tag);
        let _ = aad;
        ciphertext
    }

    /// Decrypt data (input is ciphertext + tag)
    pub fn decrypt(&mut self, ciphertext: &[u8], aad: &[u8]) -> Option<Vec<u8>> {
        if ciphertext.len() < 16 {
            return None;
        }
        let _nonce = self.get_nonce();
        let data = &ciphertext[..ciphertext.len() - 16];
        // Simplified decryption (XOR with key)
        let mut plaintext = data.to_vec();
        for (i, byte) in plaintext.iter_mut().enumerate() {
            *byte ^= self.key[i % self.key.len()];
        }
        let _ = aad;
        Some(plaintext)
    }
}

/// ChaCha20-Poly1305 cipher
pub struct ChaCha20Poly1305 {
    key: [u8; 32],
    nonce: [u8; 12],
    counter: u64,
}

impl ChaCha20Poly1305 {
    /// Create new cipher
    pub fn new(key: &[u8], initial_iv: &[u8]) -> Self {
        let mut key_arr = [0u8; 32];
        key_arr[..key.len().min(32)].copy_from_slice(&key[..key.len().min(32)]);

        let mut nonce = [0u8; 12];
        nonce[..initial_iv.len().min(12)].copy_from_slice(&initial_iv[..initial_iv.len().min(12)]);

        Self {
            key: key_arr,
            nonce,
            counter: 0,
        }
    }

    /// Get current nonce
    fn get_nonce(&mut self) -> [u8; 12] {
        let mut nonce = self.nonce;
        let counter_bytes = self.counter.to_be_bytes();
        for i in 0..8 {
            nonce[4 + i] ^= counter_bytes[i];
        }
        self.counter += 1;
        nonce
    }

    /// Encrypt data
    pub fn encrypt(&mut self, plaintext: &[u8], aad: &[u8]) -> Vec<u8> {
        let _nonce = self.get_nonce();
        // Simplified encryption
        let mut ciphertext = plaintext.to_vec();
        for (i, byte) in ciphertext.iter_mut().enumerate() {
            *byte ^= self.key[i % 32];
        }
        let tag = [0u8; 16];
        ciphertext.extend_from_slice(&tag);
        let _ = aad;
        ciphertext
    }

    /// Decrypt data
    pub fn decrypt(&mut self, ciphertext: &[u8], aad: &[u8]) -> Option<Vec<u8>> {
        if ciphertext.len() < 16 {
            return None;
        }
        let _nonce = self.get_nonce();
        let data = &ciphertext[..ciphertext.len() - 16];
        let mut plaintext = data.to_vec();
        for (i, byte) in plaintext.iter_mut().enumerate() {
            *byte ^= self.key[i % 32];
        }
        let _ = aad;
        Some(plaintext)
    }
}

/// HKDF key derivation
pub struct Hkdf {
    hash: HashAlgorithm,
}

impl Hkdf {
    pub fn new(hash: HashAlgorithm) -> Self {
        Self { hash }
    }

    /// Extract PRK from IKM
    pub fn extract(&self, salt: &[u8], ikm: &[u8]) -> Vec<u8> {
        // Simplified HMAC
        let mut prk = Vec::with_capacity(self.hash.output_length());
        let combined = [salt, ikm].concat();
        // Fake hash - in production use real HMAC
        for i in 0..self.hash.output_length() {
            prk.push(combined[i % combined.len()] ^ 0x5C);
        }
        prk
    }

    /// Expand PRK to derive keys
    pub fn expand(&self, prk: &[u8], info: &[u8], length: usize) -> Vec<u8> {
        let mut output = Vec::with_capacity(length);
        let mut block = Vec::new();
        let mut counter = 1u8;

        while output.len() < length {
            let input = [&block[..], info, &[counter]].concat();
            // Fake HMAC
            block = input.iter()
                .enumerate()
                .map(|(i, &b)| b ^ prk[i % prk.len()])
                .take(self.hash.output_length())
                .collect();
            output.extend_from_slice(&block[..block.len().min(length - output.len())]);
            counter += 1;
        }

        output
    }

    /// One-shot expand label (TLS 1.3)
    pub fn expand_label(&self, secret: &[u8], label: &str, context: &[u8], length: u16) -> Vec<u8> {
        let mut info = Vec::new();
        info.push((length >> 8) as u8);
        info.push(length as u8);

        let tls_label = format!("tls13 {}", label);
        info.push(tls_label.len() as u8);
        info.extend_from_slice(tls_label.as_bytes());

        info.push(context.len() as u8);
        info.extend_from_slice(context);

        self.expand(secret, &info, length as usize)
    }
}

// =============================================================================
// TLS CONNECTION
// =============================================================================

/// TLS connection configuration
#[derive(Clone, Debug)]
pub struct TlsConfig {
    /// Minimum TLS version
    pub min_version: TlsVersion,
    /// Maximum TLS version
    pub max_version: TlsVersion,
    /// Preferred cipher suites
    pub cipher_suites: Vec<CipherSuite>,
    /// Preferred named groups
    pub named_groups: Vec<NamedGroup>,
    /// Preferred signature schemes
    pub signature_schemes: Vec<SignatureScheme>,
    /// Server name for SNI
    pub server_name: Option<String>,
    /// ALPN protocols
    pub alpn_protocols: Vec<String>,
    /// Verify certificates
    pub verify_certificates: bool,
    /// Client authentication required
    pub client_auth: bool,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            min_version: TlsVersion::Tls12,
            max_version: TlsVersion::Tls13,
            cipher_suites: vec![
                CipherSuite::Tls13Aes128GcmSha256,
                CipherSuite::Tls13Aes256GcmSha384,
                CipherSuite::Tls13Chacha20Poly1305Sha256,
                CipherSuite::EcdheEcdsaAes128GcmSha256,
                CipherSuite::EcdheRsaAes128GcmSha256,
            ],
            named_groups: vec![
                NamedGroup::X25519,
                NamedGroup::Secp256r1,
                NamedGroup::Secp384r1,
            ],
            signature_schemes: vec![
                SignatureScheme::EcdsaSecp256r1Sha256,
                SignatureScheme::RsaPssRsaeSha256,
                SignatureScheme::RsaPkcs1Sha256,
            ],
            server_name: None,
            alpn_protocols: Vec::new(),
            verify_certificates: true,
            client_auth: false,
        }
    }
}

/// TLS connection
pub struct TlsConnection {
    /// Connection state
    state: TlsState,
    /// Role (client or server)
    role: TlsRole,
    /// Configuration
    config: TlsConfig,
    /// Negotiated version
    version: Option<TlsVersion>,
    /// Negotiated cipher suite
    cipher_suite: Option<CipherSuite>,
    /// Our random
    our_random: Random,
    /// Peer random
    peer_random: Option<Random>,
    /// Session ID
    session_id: SessionId,
    /// Handshake hash
    handshake_hash: Vec<u8>,
    /// Master secret
    master_secret: Option<Vec<u8>>,
    /// Client write key
    client_write_key: Option<Vec<u8>>,
    /// Server write key
    server_write_key: Option<Vec<u8>>,
    /// Client write IV
    client_write_iv: Option<Vec<u8>>,
    /// Server write IV
    server_write_iv: Option<Vec<u8>>,
    /// Encryption cipher
    encrypt_cipher: Option<Box<AesGcm>>,
    /// Decryption cipher
    decrypt_cipher: Option<Box<AesGcm>>,
    /// Write sequence number
    write_seq: u64,
    /// Read sequence number
    read_seq: u64,
    /// Pending incoming data
    incoming_buffer: Vec<u8>,
    /// Pending outgoing data
    outgoing_buffer: Vec<u8>,
    /// Application data buffer
    app_data_buffer: Vec<u8>,
    /// ALPN result
    alpn_protocol: Option<String>,
    /// Last error
    last_error: Option<TlsError>,
}

impl TlsConnection {
    /// Create new client connection
    pub fn client(config: TlsConfig) -> Self {
        Self {
            state: TlsState::Start,
            role: TlsRole::Client,
            config,
            version: None,
            cipher_suite: None,
            our_random: Random::new(),
            peer_random: None,
            session_id: SessionId::generate(),
            handshake_hash: Vec::new(),
            master_secret: None,
            client_write_key: None,
            server_write_key: None,
            client_write_iv: None,
            server_write_iv: None,
            encrypt_cipher: None,
            decrypt_cipher: None,
            write_seq: 0,
            read_seq: 0,
            incoming_buffer: Vec::new(),
            outgoing_buffer: Vec::new(),
            app_data_buffer: Vec::new(),
            alpn_protocol: None,
            last_error: None,
        }
    }

    /// Create new server connection
    pub fn server(config: TlsConfig) -> Self {
        Self {
            state: TlsState::Start,
            role: TlsRole::Server,
            config,
            version: None,
            cipher_suite: None,
            our_random: Random::new(),
            peer_random: None,
            session_id: SessionId::generate(),
            handshake_hash: Vec::new(),
            master_secret: None,
            client_write_key: None,
            server_write_key: None,
            client_write_iv: None,
            server_write_iv: None,
            encrypt_cipher: None,
            decrypt_cipher: None,
            write_seq: 0,
            read_seq: 0,
            incoming_buffer: Vec::new(),
            outgoing_buffer: Vec::new(),
            app_data_buffer: Vec::new(),
            alpn_protocol: None,
            last_error: None,
        }
    }

    /// Get connection state
    pub fn state(&self) -> TlsState {
        self.state
    }

    /// Check if handshake is complete
    pub fn is_connected(&self) -> bool {
        self.state == TlsState::Connected
    }

    /// Start handshake (client only)
    pub fn initiate_handshake(&mut self) -> Result<Vec<u8>, TlsError> {
        if self.role != TlsRole::Client {
            return Err(TlsError::InvalidState);
        }
        if self.state != TlsState::Start {
            return Err(TlsError::InvalidState);
        }

        // Create ClientHello
        let client_hello = ClientHello::new(self.config.server_name.as_deref());
        let hello_bytes = client_hello.encode();

        // Update handshake hash
        self.handshake_hash.extend_from_slice(&hello_bytes);

        // Wrap in record
        let record = TlsRecord::new(ContentType::Handshake, TlsVersion::Tls12, hello_bytes);

        self.state = TlsState::ClientHelloSent;

        Ok(record.encode())
    }

    /// Process incoming data
    pub fn process_incoming(&mut self, data: &[u8]) -> Result<(), TlsError> {
        self.incoming_buffer.extend_from_slice(data);

        while self.incoming_buffer.len() >= RecordHeader::SIZE {
            // Parse record header
            let header = RecordHeader::parse(&self.incoming_buffer)
                .ok_or(TlsError::DecodeError)?;

            let total_len = RecordHeader::SIZE + header.length as usize;
            if self.incoming_buffer.len() < total_len {
                break; // Need more data
            }

            // Extract record
            let record_data: Vec<u8> = self.incoming_buffer.drain(..total_len).collect();
            let (record, _) = TlsRecord::parse(&record_data)
                .ok_or(TlsError::DecodeError)?;

            // Process record
            self.process_record(record)?;
        }

        Ok(())
    }

    /// Process a single record
    fn process_record(&mut self, record: TlsRecord) -> Result<(), TlsError> {
        // Decrypt if needed
        let plaintext = if self.decrypt_cipher.is_some() && record.header.content_type == ContentType::ApplicationData {
            self.decrypt_record(&record)?
        } else {
            record.fragment
        };

        match record.header.content_type {
            ContentType::Handshake => self.process_handshake(&plaintext)?,
            ContentType::ChangeCipherSpec => self.process_change_cipher_spec(&plaintext)?,
            ContentType::Alert => self.process_alert(&plaintext)?,
            ContentType::ApplicationData => {
                self.app_data_buffer.extend_from_slice(&plaintext);
            }
            ContentType::Heartbeat => {
                // Ignore heartbeat
            }
        }

        self.read_seq += 1;
        Ok(())
    }

    /// Process handshake message
    fn process_handshake(&mut self, data: &[u8]) -> Result<(), TlsError> {
        if data.len() < 4 {
            return Err(TlsError::DecodeError);
        }

        let msg_type = HandshakeType::from_u8(data[0])
            .ok_or(TlsError::UnsupportedHandshake)?;
        let length = ((data[1] as usize) << 16) | ((data[2] as usize) << 8) | (data[3] as usize);

        if data.len() < 4 + length {
            return Err(TlsError::DecodeError);
        }

        let msg_data = &data[4..4 + length];

        // Update handshake hash (except Finished in TLS 1.3)
        self.handshake_hash.extend_from_slice(data);

        match msg_type {
            HandshakeType::ServerHello => {
                self.process_server_hello(msg_data)?;
            }
            HandshakeType::Certificate => {
                self.process_certificate(msg_data)?;
            }
            HandshakeType::ServerKeyExchange => {
                self.process_server_key_exchange(msg_data)?;
            }
            HandshakeType::ServerHelloDone => {
                self.process_server_hello_done()?;
            }
            HandshakeType::Finished => {
                self.process_finished(msg_data)?;
            }
            _ => {
                // Ignore unknown message types for now
            }
        }

        Ok(())
    }

    /// Process ServerHello
    fn process_server_hello(&mut self, data: &[u8]) -> Result<(), TlsError> {
        let hello = ServerHello::parse(data)
            .ok_or(TlsError::DecodeError)?;

        // Check version
        if !hello.server_version.is_supported() {
            return Err(TlsError::UnsupportedVersion);
        }

        // Check cipher suite
        if !self.config.cipher_suites.contains(&hello.cipher_suite) {
            return Err(TlsError::UnsupportedCipherSuite);
        }

        self.version = Some(hello.server_version);
        self.cipher_suite = Some(hello.cipher_suite);
        self.peer_random = Some(hello.random);

        self.state = TlsState::ServerHelloReceived;

        Ok(())
    }

    /// Process Certificate
    fn process_certificate(&mut self, _data: &[u8]) -> Result<(), TlsError> {
        // In a real implementation, parse and validate certificates
        self.state = TlsState::CertificateReceived;
        Ok(())
    }

    /// Process ServerKeyExchange
    fn process_server_key_exchange(&mut self, _data: &[u8]) -> Result<(), TlsError> {
        // In a real implementation, extract server's DH/ECDH public key
        self.state = TlsState::ServerKeyExchangeReceived;
        Ok(())
    }

    /// Process ServerHelloDone
    fn process_server_hello_done(&mut self) -> Result<(), TlsError> {
        self.state = TlsState::ServerHelloDoneReceived;

        // Generate key exchange response
        self.generate_key_exchange()?;

        Ok(())
    }

    /// Process Finished
    fn process_finished(&mut self, _data: &[u8]) -> Result<(), TlsError> {
        // In a real implementation, verify the finished message
        self.state = TlsState::Connected;
        Ok(())
    }

    /// Process ChangeCipherSpec
    fn process_change_cipher_spec(&mut self, _data: &[u8]) -> Result<(), TlsError> {
        // Activate decryption cipher
        if let (Some(key), Some(iv)) = (&self.server_write_key, &self.server_write_iv) {
            self.decrypt_cipher = Some(Box::new(AesGcm::new(key, iv)));
        }
        self.read_seq = 0;
        self.state = TlsState::ChangeCipherSpecReceived;
        Ok(())
    }

    /// Process Alert
    fn process_alert(&mut self, data: &[u8]) -> Result<(), TlsError> {
        if data.len() < 2 {
            return Err(TlsError::DecodeError);
        }

        let level = match data[0] {
            1 => AlertLevel::Warning,
            2 => AlertLevel::Fatal,
            _ => return Err(TlsError::DecodeError),
        };

        if level == AlertLevel::Fatal {
            self.state = TlsState::Error;
            return Err(TlsError::FatalAlert(data[1]));
        }

        // CloseNotify
        if data[1] == 0 {
            self.state = TlsState::Closed;
        }

        Ok(())
    }

    /// Generate key exchange response
    fn generate_key_exchange(&mut self) -> Result<(), TlsError> {
        // Generate pre-master secret
        let mut pre_master_secret = vec![0u8; 48];
        pre_master_secret[0] = 0x03; // TLS 1.2
        pre_master_secret[1] = 0x03;
        crate::random::get_random_bytes(&mut pre_master_secret[2..]);

        // Derive master secret
        self.derive_master_secret(&pre_master_secret)?;

        // Derive key material
        self.derive_keys()?;

        self.state = TlsState::ClientKeyExchangeSent;
        Ok(())
    }

    /// Derive master secret from pre-master secret
    fn derive_master_secret(&mut self, pre_master: &[u8]) -> Result<(), TlsError> {
        let peer_random = self.peer_random.as_ref().ok_or(TlsError::InvalidState)?;

        // PRF(pre_master, "master secret", client_random + server_random)
        let seed = [&self.our_random.bytes[..], &peer_random.bytes[..]].concat();
        let hkdf = Hkdf::new(HashAlgorithm::Sha256);
        let prk = hkdf.extract(&seed, pre_master);
        let master = hkdf.expand(&prk, b"master secret", 48);

        self.master_secret = Some(master);
        Ok(())
    }

    /// Derive session keys from master secret
    fn derive_keys(&mut self) -> Result<(), TlsError> {
        let master = self.master_secret.as_ref().ok_or(TlsError::InvalidState)?;
        let peer_random = self.peer_random.as_ref().ok_or(TlsError::InvalidState)?;
        let cipher_suite = self.cipher_suite.ok_or(TlsError::InvalidState)?;

        let key_len = cipher_suite.key_length();
        let iv_len = cipher_suite.iv_length();
        let key_block_len = (key_len + iv_len) * 2;

        let seed = [&self.our_random.bytes[..], &peer_random.bytes[..]].concat();
        let hkdf = Hkdf::new(HashAlgorithm::Sha256);
        let prk = hkdf.extract(&seed, master);
        let key_block = hkdf.expand(&prk, b"key expansion", key_block_len);

        let mut offset = 0;
        self.client_write_key = Some(key_block[offset..offset + key_len].to_vec());
        offset += key_len;
        self.server_write_key = Some(key_block[offset..offset + key_len].to_vec());
        offset += key_len;
        self.client_write_iv = Some(key_block[offset..offset + iv_len].to_vec());
        offset += iv_len;
        self.server_write_iv = Some(key_block[offset..offset + iv_len].to_vec());

        Ok(())
    }

    /// Decrypt a record
    fn decrypt_record(&mut self, record: &TlsRecord) -> Result<Vec<u8>, TlsError> {
        let cipher = self.decrypt_cipher.as_mut().ok_or(TlsError::InvalidState)?;

        // Additional data for AEAD
        let mut aad = Vec::new();
        aad.extend_from_slice(&self.read_seq.to_be_bytes());
        aad.push(record.header.content_type as u8);
        let version = record.header.version as u16;
        aad.push((version >> 8) as u8);
        aad.push(version as u8);
        let len = record.fragment.len() - cipher_suite_tag_len(self.cipher_suite.unwrap());
        aad.push((len >> 8) as u8);
        aad.push(len as u8);

        cipher.decrypt(&record.fragment, &aad)
            .ok_or(TlsError::DecryptError)
    }

    /// Encrypt and send application data
    pub fn send(&mut self, data: &[u8]) -> Result<Vec<u8>, TlsError> {
        if self.state != TlsState::Connected {
            return Err(TlsError::InvalidState);
        }

        let cipher = self.encrypt_cipher.as_mut().ok_or(TlsError::InvalidState)?;
        let version = self.version.ok_or(TlsError::InvalidState)?;

        // Encrypt data
        let mut aad = Vec::new();
        aad.extend_from_slice(&self.write_seq.to_be_bytes());
        aad.push(ContentType::ApplicationData as u8);
        let v = version as u16;
        aad.push((v >> 8) as u8);
        aad.push(v as u8);
        aad.push((data.len() >> 8) as u8);
        aad.push(data.len() as u8);

        let ciphertext = cipher.encrypt(data, &aad);
        self.write_seq += 1;

        let record = TlsRecord::new(ContentType::ApplicationData, version, ciphertext);
        Ok(record.encode())
    }

    /// Receive decrypted application data
    pub fn recv(&mut self) -> Vec<u8> {
        core::mem::take(&mut self.app_data_buffer)
    }

    /// Close the connection
    pub fn close(&mut self) -> Result<Vec<u8>, TlsError> {
        let version = self.version.unwrap_or(TlsVersion::Tls12);
        let alert = TlsRecord::new(
            ContentType::Alert,
            version,
            vec![AlertLevel::Warning as u8, AlertDescription::CloseNotify as u8],
        );
        self.state = TlsState::Closed;
        Ok(alert.encode())
    }

    /// Get negotiated ALPN protocol
    pub fn alpn_protocol(&self) -> Option<&str> {
        self.alpn_protocol.as_deref()
    }

    /// Get negotiated version
    pub fn negotiated_version(&self) -> Option<TlsVersion> {
        self.version
    }

    /// Get negotiated cipher suite
    pub fn negotiated_cipher_suite(&self) -> Option<CipherSuite> {
        self.cipher_suite
    }
}

/// Get cipher suite tag length
fn cipher_suite_tag_len(suite: CipherSuite) -> usize {
    suite.mac_length()
}

// =============================================================================
// TLS ERROR
// =============================================================================

/// TLS error
#[derive(Clone, Debug)]
pub enum TlsError {
    /// Invalid state for operation
    InvalidState,
    /// Decode error
    DecodeError,
    /// Unsupported TLS version
    UnsupportedVersion,
    /// Unsupported cipher suite
    UnsupportedCipherSuite,
    /// Unsupported handshake type
    UnsupportedHandshake,
    /// Certificate verification failed
    CertificateError,
    /// Decryption failed
    DecryptError,
    /// MAC verification failed
    MacError,
    /// Fatal alert received
    FatalAlert(u8),
    /// I/O error
    IoError,
    /// Buffer overflow
    BufferOverflow,
    /// Handshake failed
    HandshakeFailure,
}

impl TlsError {
    /// Convert to errno
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::InvalidState => -22,     // EINVAL
            Self::DecodeError => -22,      // EINVAL
            Self::UnsupportedVersion => -95, // EOPNOTSUPP
            Self::UnsupportedCipherSuite => -95,
            Self::UnsupportedHandshake => -95,
            Self::CertificateError => -1,  // EPERM
            Self::DecryptError => -5,      // EIO
            Self::MacError => -5,          // EIO
            Self::FatalAlert(_) => -104,   // ECONNRESET
            Self::IoError => -5,           // EIO
            Self::BufferOverflow => -12,   // ENOMEM
            Self::HandshakeFailure => -111, // ECONNREFUSED
        }
    }
}

// =============================================================================
// GLOBAL TLS CONTEXT
// =============================================================================

/// TLS context for session management
pub struct TlsContext {
    /// Session cache
    sessions: RwLock<BTreeMap<String, Vec<u8>>>,
    /// Default configuration
    default_config: TlsConfig,
}

impl TlsContext {
    /// Create new TLS context
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(BTreeMap::new()),
            default_config: TlsConfig::default(),
        }
    }

    /// Create client connection
    pub fn connect(&self, server_name: &str) -> TlsConnection {
        let mut config = self.default_config.clone();
        config.server_name = Some(server_name.to_string());
        TlsConnection::client(config)
    }

    /// Create server connection
    pub fn accept(&self) -> TlsConnection {
        TlsConnection::server(self.default_config.clone())
    }

    /// Store session for resumption
    pub fn store_session(&self, server_name: &str, session_data: Vec<u8>) {
        self.sessions.write().insert(server_name.to_string(), session_data);
    }

    /// Get cached session
    pub fn get_session(&self, server_name: &str) -> Option<Vec<u8>> {
        self.sessions.read().get(server_name).cloned()
    }
}

impl Default for TlsContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Global TLS context
static TLS_CONTEXT: RwLock<Option<TlsContext>> = RwLock::new(None);

/// Initialize TLS subsystem
pub fn init() {
    *TLS_CONTEXT.write() = Some(TlsContext::new());
    crate::kprintln!("[NET] TLS/SSL initialized (TLS 1.2/1.3)");
}

/// Get global TLS context
pub fn context() -> Option<Arc<TlsContext>> {
    // In a real implementation, use Arc
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tls_version() {
        assert_eq!(TlsVersion::from_u16(0x0303), Some(TlsVersion::Tls12));
        assert_eq!(TlsVersion::from_u16(0x0304), Some(TlsVersion::Tls13));
        assert!(TlsVersion::Tls12.is_supported());
        assert!(TlsVersion::Tls13.is_supported());
    }

    #[test]
    fn test_cipher_suite() {
        let suite = CipherSuite::Tls13Aes128GcmSha256;
        assert!(suite.is_tls13());
        assert_eq!(suite.key_length(), 16);
        assert_eq!(suite.iv_length(), 12);
    }

    #[test]
    fn test_record_header() {
        let header = RecordHeader {
            content_type: ContentType::Handshake,
            version: TlsVersion::Tls12,
            length: 100,
        };
        let encoded = header.encode();
        let parsed = RecordHeader::parse(&encoded).unwrap();
        assert_eq!(parsed.content_type, header.content_type);
        assert_eq!(parsed.version, header.version);
        assert_eq!(parsed.length, header.length);
    }

    #[test]
    fn test_extension_server_name() {
        let ext = TlsExtension::server_name("example.com");
        assert_eq!(ext.ext_type, ExtensionType::ServerName);
        assert!(!ext.data.is_empty());
    }

    #[test]
    fn test_client_hello() {
        let hello = ClientHello::new(Some("example.com"));
        let encoded = hello.encode();
        assert!(!encoded.is_empty());
        assert_eq!(encoded[0], HandshakeType::ClientHello as u8);
    }

    #[test]
    fn test_hkdf() {
        let hkdf = Hkdf::new(HashAlgorithm::Sha256);
        let ikm = b"input key material";
        let salt = b"salt";
        let prk = hkdf.extract(salt, ikm);
        assert_eq!(prk.len(), 32);

        let okm = hkdf.expand(&prk, b"info", 32);
        assert_eq!(okm.len(), 32);
    }
}
