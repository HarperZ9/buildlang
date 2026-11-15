// ===============================================================================
// BUILDOS KERNEL - IPv4 (INTERNET PROTOCOL VERSION 4)
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! IPv4 protocol implementation (RFC 791).

use alloc::vec::Vec;
use core::fmt;
use core::sync::atomic::{AtomicU16, Ordering};

use super::checksum;

// =============================================================================
// CONSTANTS
// =============================================================================

/// Minimum IPv4 header size
pub const IPV4_HEADER_MIN_SIZE: usize = 20;

/// Maximum IPv4 header size (with options)
pub const IPV4_HEADER_MAX_SIZE: usize = 60;

/// IPv4 version
pub const IPV4_VERSION: u8 = 4;

/// Default TTL
pub const DEFAULT_TTL: u8 = 64;

/// Global packet ID counter
static PACKET_ID: AtomicU16 = AtomicU16::new(1);

// =============================================================================
// IPv4 ADDRESS
// =============================================================================

/// IPv4 address (32-bit)
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Ipv4Address(pub [u8; 4]);

impl Ipv4Address {
    /// All zeros (0.0.0.0)
    pub const ZERO: Ipv4Address = Ipv4Address([0, 0, 0, 0]);

    /// Broadcast (255.255.255.255)
    pub const BROADCAST: Ipv4Address = Ipv4Address([255, 255, 255, 255]);

    /// Localhost (127.0.0.1)
    pub const LOCALHOST: Ipv4Address = Ipv4Address([127, 0, 0, 1]);

    /// Any address (0.0.0.0)
    pub const ANY: Ipv4Address = Ipv4Address([0, 0, 0, 0]);

    /// Create a new IPv4 address
    pub const fn new(a: u8, b: u8, c: u8, d: u8) -> Self {
        Self([a, b, c, d])
    }

    /// Create from 32-bit integer (network byte order)
    pub fn from_u32(value: u32) -> Self {
        Self(value.to_be_bytes())
    }

    /// Convert to 32-bit integer (network byte order)
    pub fn to_u32(&self) -> u32 {
        u32::from_be_bytes(self.0)
    }

    /// Create from bytes
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 4 {
            return None;
        }
        let mut addr = [0u8; 4];
        addr.copy_from_slice(&bytes[..4]);
        Some(Self(addr))
    }

    /// Check if this is the zero address (0.0.0.0)
    pub fn is_zero(&self) -> bool {
        self.0 == [0, 0, 0, 0]
    }

    /// Get octets
    pub fn octets(&self) -> [u8; 4] {
        self.0
    }

    /// Get bytes (alias for octets)
    pub fn to_bytes(&self) -> [u8; 4] {
        self.0
    }

    /// Check if address is in same subnet
    pub fn is_local(&self, local_ip: &Ipv4Address, netmask: &Ipv4Address) -> bool {
        let self_net = self.to_u32() & netmask.to_u32();
        let local_net = local_ip.to_u32() & netmask.to_u32();
        self_net == local_net
    }

    /// Check if loopback (127.x.x.x)
    pub fn is_loopback(&self) -> bool {
        self.0[0] == 127
    }

    /// Check if multicast (224.0.0.0 - 239.255.255.255)
    pub fn is_multicast(&self) -> bool {
        self.0[0] >= 224 && self.0[0] <= 239
    }

    /// Check if broadcast
    pub fn is_broadcast(&self) -> bool {
        *self == Self::BROADCAST
    }

    /// Check if unspecified (0.0.0.0)
    pub fn is_unspecified(&self) -> bool {
        *self == Self::ZERO
    }

    /// Check if private address
    pub fn is_private(&self) -> bool {
        // 10.0.0.0/8
        self.0[0] == 10 ||
        // 172.16.0.0/12
        (self.0[0] == 172 && (self.0[1] >= 16 && self.0[1] <= 31)) ||
        // 192.168.0.0/16
        (self.0[0] == 192 && self.0[1] == 168)
    }

    /// Check if link-local (169.254.0.0/16)
    pub fn is_link_local(&self) -> bool {
        self.0[0] == 169 && self.0[1] == 254
    }
}

impl fmt::Debug for Ipv4Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}.{}", self.0[0], self.0[1], self.0[2], self.0[3])
    }
}

impl fmt::Display for Ipv4Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

// =============================================================================
// IP PROTOCOL NUMBERS
// =============================================================================

/// IP protocol numbers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IpProtocol {
    /// ICMP
    Icmp = 1,
    /// IGMP
    Igmp = 2,
    /// TCP
    Tcp = 6,
    /// UDP
    Udp = 17,
    /// GRE
    Gre = 47,
    /// ESP (IPsec)
    Esp = 50,
    /// AH (IPsec)
    Ah = 51,
    /// ICMPv6
    Icmpv6 = 58,
    /// SCTP
    Sctp = 132,
    /// Unknown
    Unknown = 255,
}

impl IpProtocol {
    pub fn from_u8(value: u8) -> Self {
        match value {
            1 => IpProtocol::Icmp,
            2 => IpProtocol::Igmp,
            6 => IpProtocol::Tcp,
            17 => IpProtocol::Udp,
            47 => IpProtocol::Gre,
            50 => IpProtocol::Esp,
            51 => IpProtocol::Ah,
            58 => IpProtocol::Icmpv6,
            132 => IpProtocol::Sctp,
            _ => IpProtocol::Unknown,
        }
    }
}

// =============================================================================
// IPv4 FLAGS
// =============================================================================

/// IPv4 header flags
#[derive(Debug, Clone, Copy, Default)]
pub struct Ipv4Flags {
    /// Don't Fragment
    pub dont_fragment: bool,
    /// More Fragments
    pub more_fragments: bool,
}

impl Ipv4Flags {
    pub fn from_u8(value: u8) -> Self {
        Self {
            dont_fragment: (value & 0x02) != 0,
            more_fragments: (value & 0x01) != 0,
        }
    }

    pub fn to_u8(&self) -> u8 {
        let mut flags = 0u8;
        if self.dont_fragment {
            flags |= 0x02;
        }
        if self.more_fragments {
            flags |= 0x01;
        }
        flags
    }
}

// =============================================================================
// IPv4 PACKET
// =============================================================================

/// IPv4 packet
#[derive(Debug, Clone)]
pub struct Ipv4Packet<'a> {
    /// Version (should be 4)
    pub version: u8,
    /// Header length in 32-bit words
    pub ihl: u8,
    /// Differentiated Services Code Point
    pub dscp: u8,
    /// Explicit Congestion Notification
    pub ecn: u8,
    /// Total length (header + payload)
    pub total_length: u16,
    /// Identification
    pub identification: u16,
    /// Flags
    pub flags: Ipv4Flags,
    /// Fragment offset (in 8-byte units)
    pub fragment_offset: u16,
    /// Time to Live
    pub ttl: u8,
    /// Protocol
    pub protocol: IpProtocol,
    /// Header checksum
    pub checksum: u16,
    /// Source address
    pub src: Ipv4Address,
    /// Destination address
    pub dest: Ipv4Address,
    /// Options (if any)
    pub options: Option<&'a [u8]>,
    /// Payload
    pub payload: &'a [u8],
}

impl<'a> Ipv4Packet<'a> {
    /// Parse an IPv4 packet from bytes
    pub fn parse(data: &'a [u8]) -> Option<Self> {
        if data.len() < IPV4_HEADER_MIN_SIZE {
            return None;
        }

        let version = data[0] >> 4;
        if version != IPV4_VERSION {
            return None;
        }

        let ihl = data[0] & 0x0F;
        let header_len = (ihl as usize) * 4;

        if header_len < IPV4_HEADER_MIN_SIZE || data.len() < header_len {
            return None;
        }

        let dscp = data[1] >> 2;
        let ecn = data[1] & 0x03;
        let total_length = u16::from_be_bytes([data[2], data[3]]);
        let identification = u16::from_be_bytes([data[4], data[5]]);

        let flags_frag = u16::from_be_bytes([data[6], data[7]]);
        let flags = Ipv4Flags::from_u8((flags_frag >> 13) as u8);
        let fragment_offset = flags_frag & 0x1FFF;

        let ttl = data[8];
        let protocol = IpProtocol::from_u8(data[9]);
        let checksum = u16::from_be_bytes([data[10], data[11]]);
        let src = Ipv4Address::from_bytes(&data[12..16])?;
        let dest = Ipv4Address::from_bytes(&data[16..20])?;

        let options = if header_len > IPV4_HEADER_MIN_SIZE {
            Some(&data[IPV4_HEADER_MIN_SIZE..header_len])
        } else {
            None
        };

        let payload_len = (total_length as usize).saturating_sub(header_len);
        let payload = if data.len() >= header_len + payload_len {
            &data[header_len..header_len + payload_len]
        } else {
            &data[header_len..]
        };

        Some(Self {
            version,
            ihl,
            dscp,
            ecn,
            total_length,
            identification,
            flags,
            fragment_offset,
            ttl,
            protocol,
            checksum,
            src,
            dest,
            options,
            payload,
        })
    }

    /// Create a new IPv4 packet
    pub fn new(src: Ipv4Address, dest: Ipv4Address, protocol: IpProtocol, payload: &'a [u8]) -> Self {
        let total_length = (IPV4_HEADER_MIN_SIZE + payload.len()) as u16;

        Self {
            version: IPV4_VERSION,
            ihl: 5, // No options
            dscp: 0,
            ecn: 0,
            total_length,
            identification: PACKET_ID.fetch_add(1, Ordering::Relaxed),
            flags: Ipv4Flags {
                dont_fragment: true,
                more_fragments: false,
            },
            fragment_offset: 0,
            ttl: DEFAULT_TTL,
            protocol,
            checksum: 0, // Will be calculated
            src,
            dest,
            options: None,
            payload,
        }
    }

    /// Verify header checksum
    pub fn verify_checksum(&self) -> bool {
        let header = self.header_bytes();
        checksum::internet_checksum(&header) == 0
    }

    /// Calculate header checksum
    pub fn calculate_checksum(&self) -> u16 {
        let mut header = self.header_bytes();
        // Zero out checksum field for calculation
        header[10] = 0;
        header[11] = 0;
        checksum::internet_checksum(&header)
    }

    /// Get header as bytes
    pub fn header_bytes(&self) -> Vec<u8> {
        let header_len = (self.ihl as usize) * 4;
        let mut header = Vec::with_capacity(header_len);

        header.push((self.version << 4) | self.ihl);
        header.push((self.dscp << 2) | self.ecn);
        header.extend_from_slice(&self.total_length.to_be_bytes());
        header.extend_from_slice(&self.identification.to_be_bytes());

        let flags_frag = ((self.flags.to_u8() as u16) << 13) | self.fragment_offset;
        header.extend_from_slice(&flags_frag.to_be_bytes());

        header.push(self.ttl);
        header.push(self.protocol as u8);
        header.extend_from_slice(&self.checksum.to_be_bytes());
        header.extend_from_slice(&self.src.octets());
        header.extend_from_slice(&self.dest.octets());

        if let Some(options) = self.options {
            header.extend_from_slice(options);
        }

        header
    }

    /// Serialize packet to bytes
    pub fn serialize(&self) -> Vec<u8> {
        let header_len = (self.ihl as usize) * 4;
        let total_len = header_len + self.payload.len();
        let mut packet = Vec::with_capacity(total_len);

        // Build header
        packet.push((self.version << 4) | self.ihl);
        packet.push((self.dscp << 2) | self.ecn);
        packet.extend_from_slice(&(total_len as u16).to_be_bytes());
        packet.extend_from_slice(&self.identification.to_be_bytes());

        let flags_frag = ((self.flags.to_u8() as u16) << 13) | self.fragment_offset;
        packet.extend_from_slice(&flags_frag.to_be_bytes());

        packet.push(self.ttl);
        packet.push(self.protocol as u8);

        // Placeholder for checksum
        let checksum_pos = packet.len();
        packet.extend_from_slice(&[0, 0]);

        packet.extend_from_slice(&self.src.octets());
        packet.extend_from_slice(&self.dest.octets());

        if let Some(options) = self.options {
            packet.extend_from_slice(options);
        }

        // Calculate and insert checksum
        let checksum = checksum::internet_checksum(&packet[..header_len]);
        packet[checksum_pos] = (checksum >> 8) as u8;
        packet[checksum_pos + 1] = checksum as u8;

        // Add payload
        packet.extend_from_slice(self.payload);

        packet
    }

    /// Get header length in bytes
    pub fn header_len(&self) -> usize {
        (self.ihl as usize) * 4
    }
}

// =============================================================================
// PSEUDO HEADER (for TCP/UDP checksum)
// =============================================================================

/// IPv4 pseudo header for TCP/UDP checksum calculation
pub struct PseudoHeader {
    pub src: Ipv4Address,
    pub dest: Ipv4Address,
    pub protocol: u8,
    pub length: u16,
}

impl PseudoHeader {
    /// Create new pseudo header
    pub fn new(src: Ipv4Address, dest: Ipv4Address, protocol: IpProtocol, length: u16) -> Self {
        Self {
            src,
            dest,
            protocol: protocol as u8,
            length,
        }
    }

    /// Serialize to bytes (12 bytes)
    pub fn to_bytes(&self) -> [u8; 12] {
        let mut bytes = [0u8; 12];
        bytes[0..4].copy_from_slice(&self.src.octets());
        bytes[4..8].copy_from_slice(&self.dest.octets());
        bytes[8] = 0; // Reserved
        bytes[9] = self.protocol;
        bytes[10..12].copy_from_slice(&self.length.to_be_bytes());
        bytes
    }
}
