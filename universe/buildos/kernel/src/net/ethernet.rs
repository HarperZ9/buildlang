// ===============================================================================
// BUILDOS KERNEL - ETHERNET FRAME HANDLING
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Ethernet frame parsing and construction.

use alloc::vec::Vec;
use core::fmt;

// =============================================================================
// MAC ADDRESS
// =============================================================================

/// 48-bit MAC address
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MacAddress(pub [u8; 6]);

impl MacAddress {
    /// All zeros
    pub const ZERO: MacAddress = MacAddress([0, 0, 0, 0, 0, 0]);

    /// Broadcast address (FF:FF:FF:FF:FF:FF)
    pub const BROADCAST: MacAddress = MacAddress([0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);

    /// Create a new MAC address
    pub const fn new(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8) -> Self {
        Self([a, b, c, d, e, f])
    }

    /// Create from bytes
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 6 {
            return None;
        }
        let mut mac = [0u8; 6];
        mac.copy_from_slice(&bytes[..6]);
        Some(Self(mac))
    }

    /// Get as bytes
    pub fn as_bytes(&self) -> &[u8; 6] {
        &self.0
    }

    /// Check if broadcast
    pub fn is_broadcast(&self) -> bool {
        *self == Self::BROADCAST
    }

    /// Check if multicast (bit 0 of first byte is set)
    pub fn is_multicast(&self) -> bool {
        (self.0[0] & 0x01) != 0
    }

    /// Check if unicast
    pub fn is_unicast(&self) -> bool {
        !self.is_multicast()
    }

    /// Check if locally administered
    pub fn is_local(&self) -> bool {
        (self.0[0] & 0x02) != 0
    }
}

impl fmt::Debug for MacAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

impl fmt::Display for MacAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

// =============================================================================
// ETHERTYPE
// =============================================================================

/// Ethernet type field values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum EtherType {
    /// IPv4
    Ipv4 = 0x0800,
    /// ARP
    Arp = 0x0806,
    /// Wake-on-LAN
    Wol = 0x0842,
    /// VLAN-tagged frame (IEEE 802.1Q)
    Vlan = 0x8100,
    /// IPv6
    Ipv6 = 0x86DD,
    /// LLDP (Link Layer Discovery Protocol)
    Lldp = 0x88CC,
    /// Unknown/unsupported
    Unknown = 0xFFFF,
}

impl EtherType {
    /// Parse from u16
    pub fn from_u16(value: u16) -> Self {
        match value {
            0x0800 => EtherType::Ipv4,
            0x0806 => EtherType::Arp,
            0x0842 => EtherType::Wol,
            0x8100 => EtherType::Vlan,
            0x86DD => EtherType::Ipv6,
            0x88CC => EtherType::Lldp,
            _ => EtherType::Unknown,
        }
    }

    /// Convert to u16
    pub fn to_u16(self) -> u16 {
        self as u16
    }
}

// =============================================================================
// ETHERNET FRAME
// =============================================================================

/// Ethernet frame header size
pub const ETHERNET_HEADER_SIZE: usize = 14;

/// Minimum Ethernet frame size (excluding FCS)
pub const ETHERNET_MIN_SIZE: usize = 60;

/// Maximum Ethernet frame size (excluding FCS)
pub const ETHERNET_MAX_SIZE: usize = 1514;

/// Ethernet frame
#[derive(Debug, Clone)]
pub struct EthernetFrame<'a> {
    /// Destination MAC address
    pub dest: MacAddress,
    /// Source MAC address
    pub src: MacAddress,
    /// EtherType
    pub ethertype: EtherType,
    /// Payload data
    pub payload: &'a [u8],
}

impl<'a> EthernetFrame<'a> {
    /// Parse an Ethernet frame from raw bytes
    pub fn parse(data: &'a [u8]) -> Option<Self> {
        if data.len() < ETHERNET_HEADER_SIZE {
            return None;
        }

        let dest = MacAddress::from_bytes(&data[0..6])?;
        let src = MacAddress::from_bytes(&data[6..12])?;
        let ethertype = EtherType::from_u16(u16::from_be_bytes([data[12], data[13]]));
        let payload = &data[14..];

        Some(Self {
            dest,
            src,
            ethertype,
            payload,
        })
    }

    /// Create a new Ethernet frame
    pub fn new(dest: MacAddress, src: MacAddress, ethertype: EtherType, payload: &'a [u8]) -> Self {
        Self {
            dest,
            src,
            ethertype,
            payload,
        }
    }

    /// Serialize the frame to bytes
    pub fn serialize(&self) -> Vec<u8> {
        let mut frame = Vec::with_capacity(ETHERNET_HEADER_SIZE + self.payload.len());

        // Destination MAC
        frame.extend_from_slice(self.dest.as_bytes());
        // Source MAC
        frame.extend_from_slice(self.src.as_bytes());
        // EtherType
        frame.extend_from_slice(&self.ethertype.to_u16().to_be_bytes());
        // Payload
        frame.extend_from_slice(self.payload);

        // Pad to minimum size if necessary
        while frame.len() < ETHERNET_MIN_SIZE {
            frame.push(0);
        }

        frame
    }

    /// Get the header as bytes
    pub fn header_bytes(&self) -> [u8; ETHERNET_HEADER_SIZE] {
        let mut header = [0u8; ETHERNET_HEADER_SIZE];
        header[0..6].copy_from_slice(self.dest.as_bytes());
        header[6..12].copy_from_slice(self.src.as_bytes());
        let et = self.ethertype.to_u16().to_be_bytes();
        header[12] = et[0];
        header[13] = et[1];
        header
    }
}

/// Owned Ethernet frame (for construction)
#[derive(Debug, Clone)]
pub struct OwnedEthernetFrame {
    /// Destination MAC address
    pub dest: MacAddress,
    /// Source MAC address
    pub src: MacAddress,
    /// EtherType
    pub ethertype: EtherType,
    /// Payload data
    pub payload: Vec<u8>,
}

impl OwnedEthernetFrame {
    /// Create a new owned Ethernet frame
    pub fn new(dest: MacAddress, src: MacAddress, ethertype: EtherType, payload: Vec<u8>) -> Self {
        Self {
            dest,
            src,
            ethertype,
            payload,
        }
    }

    /// Serialize the frame to bytes
    pub fn serialize(&self) -> Vec<u8> {
        let mut frame = Vec::with_capacity(ETHERNET_HEADER_SIZE + self.payload.len());

        // Destination MAC
        frame.extend_from_slice(self.dest.as_bytes());
        // Source MAC
        frame.extend_from_slice(self.src.as_bytes());
        // EtherType
        frame.extend_from_slice(&self.ethertype.to_u16().to_be_bytes());
        // Payload
        frame.extend_from_slice(&self.payload);

        // Pad to minimum size if necessary
        while frame.len() < ETHERNET_MIN_SIZE {
            frame.push(0);
        }

        frame
    }
}

// =============================================================================
// VLAN TAG
// =============================================================================

/// VLAN tag structure (802.1Q)
#[derive(Debug, Clone, Copy)]
pub struct VlanTag {
    /// Priority Code Point (3 bits)
    pub pcp: u8,
    /// Drop Eligible Indicator (1 bit)
    pub dei: bool,
    /// VLAN Identifier (12 bits)
    pub vid: u16,
}

impl VlanTag {
    /// Parse from 2 bytes
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 2 {
            return None;
        }

        let tci = u16::from_be_bytes([data[0], data[1]]);
        Some(Self {
            pcp: ((tci >> 13) & 0x07) as u8,
            dei: ((tci >> 12) & 0x01) != 0,
            vid: tci & 0x0FFF,
        })
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> [u8; 2] {
        let tci = ((self.pcp as u16 & 0x07) << 13)
            | ((self.dei as u16) << 12)
            | (self.vid & 0x0FFF);
        tci.to_be_bytes()
    }
}
