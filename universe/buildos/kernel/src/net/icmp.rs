// ===============================================================================
// BUILDOS KERNEL - ICMP (INTERNET CONTROL MESSAGE PROTOCOL)
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! ICMP protocol implementation (RFC 792).

use alloc::sync::Arc;
use alloc::vec::Vec;

use super::checksum;
use super::ip::{Ipv4Packet, Ipv4Address, IpProtocol};
use super::{NetworkStack, NetworkInterface};

// =============================================================================
// ICMP TYPES
// =============================================================================

/// ICMP message types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IcmpType {
    /// Echo Reply (pong)
    EchoReply = 0,
    /// Destination Unreachable
    DestinationUnreachable = 3,
    /// Source Quench (deprecated)
    SourceQuench = 4,
    /// Redirect
    Redirect = 5,
    /// Echo Request (ping)
    EchoRequest = 8,
    /// Router Advertisement
    RouterAdvertisement = 9,
    /// Router Solicitation
    RouterSolicitation = 10,
    /// Time Exceeded
    TimeExceeded = 11,
    /// Parameter Problem
    ParameterProblem = 12,
    /// Timestamp Request
    TimestampRequest = 13,
    /// Timestamp Reply
    TimestampReply = 14,
    /// Unknown
    Unknown = 255,
}

impl IcmpType {
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => IcmpType::EchoReply,
            3 => IcmpType::DestinationUnreachable,
            4 => IcmpType::SourceQuench,
            5 => IcmpType::Redirect,
            8 => IcmpType::EchoRequest,
            9 => IcmpType::RouterAdvertisement,
            10 => IcmpType::RouterSolicitation,
            11 => IcmpType::TimeExceeded,
            12 => IcmpType::ParameterProblem,
            13 => IcmpType::TimestampRequest,
            14 => IcmpType::TimestampReply,
            _ => IcmpType::Unknown,
        }
    }
}

/// ICMP Destination Unreachable codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DestUnreachableCode {
    NetworkUnreachable = 0,
    HostUnreachable = 1,
    ProtocolUnreachable = 2,
    PortUnreachable = 3,
    FragmentationNeeded = 4,
    SourceRouteFailed = 5,
    DestNetworkUnknown = 6,
    DestHostUnknown = 7,
    SourceHostIsolated = 8,
    NetworkProhibited = 9,
    HostProhibited = 10,
    NetworkTosUnreachable = 11,
    HostTosUnreachable = 12,
    CommProhibited = 13,
    HostPrecedenceViolation = 14,
    PrecedenceCutoff = 15,
}

/// ICMP Time Exceeded codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TimeExceededCode {
    TtlExceeded = 0,
    FragmentReassemblyExceeded = 1,
}

// =============================================================================
// ICMP PACKET
// =============================================================================

/// ICMP header size
pub const ICMP_HEADER_SIZE: usize = 8;

/// ICMP packet
#[derive(Debug, Clone)]
pub struct IcmpPacket<'a> {
    /// Message type
    pub icmp_type: IcmpType,
    /// Code
    pub code: u8,
    /// Checksum
    pub checksum: u16,
    /// Rest of header (type-specific, 4 bytes)
    pub rest_of_header: [u8; 4],
    /// Data
    pub data: &'a [u8],
}

impl<'a> IcmpPacket<'a> {
    /// Parse an ICMP packet from bytes
    pub fn parse(data: &'a [u8]) -> Option<Self> {
        if data.len() < ICMP_HEADER_SIZE {
            return None;
        }

        let icmp_type = IcmpType::from_u8(data[0]);
        let code = data[1];
        let checksum = u16::from_be_bytes([data[2], data[3]]);
        let mut rest_of_header = [0u8; 4];
        rest_of_header.copy_from_slice(&data[4..8]);
        let payload = &data[8..];

        Some(Self {
            icmp_type,
            code,
            checksum,
            rest_of_header,
            data: payload,
        })
    }

    /// Create an Echo Request (ping)
    pub fn echo_request(identifier: u16, sequence: u16, data: &'a [u8]) -> Self {
        let mut rest = [0u8; 4];
        rest[0..2].copy_from_slice(&identifier.to_be_bytes());
        rest[2..4].copy_from_slice(&sequence.to_be_bytes());

        Self {
            icmp_type: IcmpType::EchoRequest,
            code: 0,
            checksum: 0,
            rest_of_header: rest,
            data,
        }
    }

    /// Create an Echo Reply (pong)
    pub fn echo_reply(identifier: u16, sequence: u16, data: &'a [u8]) -> Self {
        let mut rest = [0u8; 4];
        rest[0..2].copy_from_slice(&identifier.to_be_bytes());
        rest[2..4].copy_from_slice(&sequence.to_be_bytes());

        Self {
            icmp_type: IcmpType::EchoReply,
            code: 0,
            checksum: 0,
            rest_of_header: rest,
            data,
        }
    }

    /// Create a Destination Unreachable message
    pub fn destination_unreachable(code: DestUnreachableCode, original_packet: &'a [u8]) -> Self {
        Self {
            icmp_type: IcmpType::DestinationUnreachable,
            code: code as u8,
            checksum: 0,
            rest_of_header: [0; 4],
            data: original_packet,
        }
    }

    /// Create a Time Exceeded message
    pub fn time_exceeded(code: TimeExceededCode, original_packet: &'a [u8]) -> Self {
        Self {
            icmp_type: IcmpType::TimeExceeded,
            code: code as u8,
            checksum: 0,
            rest_of_header: [0; 4],
            data: original_packet,
        }
    }

    /// Verify checksum
    pub fn verify_checksum(&self) -> bool {
        let packet = self.serialize_no_checksum();
        checksum::internet_checksum(&packet) == self.checksum
    }

    /// Get identifier (for Echo Request/Reply)
    pub fn identifier(&self) -> u16 {
        u16::from_be_bytes([self.rest_of_header[0], self.rest_of_header[1]])
    }

    /// Get sequence number (for Echo Request/Reply)
    pub fn sequence(&self) -> u16 {
        u16::from_be_bytes([self.rest_of_header[2], self.rest_of_header[3]])
    }

    /// Serialize without checksum (for calculation)
    fn serialize_no_checksum(&self) -> Vec<u8> {
        let mut packet = Vec::with_capacity(ICMP_HEADER_SIZE + self.data.len());
        packet.push(self.icmp_type as u8);
        packet.push(self.code);
        packet.push(0); // Checksum placeholder
        packet.push(0);
        packet.extend_from_slice(&self.rest_of_header);
        packet.extend_from_slice(self.data);
        packet
    }

    /// Serialize to bytes
    pub fn serialize(&self) -> Vec<u8> {
        let mut packet = self.serialize_no_checksum();

        // Calculate checksum
        let checksum = checksum::internet_checksum(&packet);
        packet[2] = (checksum >> 8) as u8;
        packet[3] = checksum as u8;

        packet
    }
}

// =============================================================================
// ICMP HANDLING
// =============================================================================

/// Handle an incoming ICMP packet
pub fn handle_icmp(
    stack: &NetworkStack,
    iface: &Arc<NetworkInterface>,
    ip: &Ipv4Packet,
    icmp: &IcmpPacket,
) {
    match icmp.icmp_type {
        IcmpType::EchoRequest => {
            // Respond with Echo Reply
            send_echo_reply(stack, iface, ip, icmp);
        }
        IcmpType::EchoReply => {
            // Handle ping response (for application layer)
            handle_echo_reply(stack, icmp);
        }
        IcmpType::DestinationUnreachable => {
            // Notify upper layers
            handle_dest_unreachable(stack, icmp);
        }
        IcmpType::TimeExceeded => {
            // Handle TTL exceeded
            handle_time_exceeded(stack, icmp);
        }
        _ => {
            // Ignore other types for now
        }
    }
}

/// Send an Echo Reply (pong response)
fn send_echo_reply(
    stack: &NetworkStack,
    iface: &Arc<NetworkInterface>,
    ip: &Ipv4Packet,
    request: &IcmpPacket,
) {
    // Create reply with same identifier, sequence, and data
    let reply = IcmpPacket::echo_reply(
        request.identifier(),
        request.sequence(),
        request.data,
    );

    // Send IP packet back to sender
    let _ = stack.send_ip(iface, ip.src, IpProtocol::Icmp, &reply.serialize());
}

/// Handle Echo Reply (ping response)
fn handle_echo_reply(_stack: &NetworkStack, icmp: &IcmpPacket) {
    // TODO: Notify waiting ping application
    let _id = icmp.identifier();
    let _seq = icmp.sequence();
}

/// Handle Destination Unreachable
fn handle_dest_unreachable(_stack: &NetworkStack, _icmp: &IcmpPacket) {
    // TODO: Notify affected connections
}

/// Handle Time Exceeded
fn handle_time_exceeded(_stack: &NetworkStack, _icmp: &IcmpPacket) {
    // TODO: Notify affected connections (traceroute uses this)
}

/// Send a ping (Echo Request)
pub fn send_ping(
    stack: &NetworkStack,
    iface: &Arc<NetworkInterface>,
    dest: Ipv4Address,
    identifier: u16,
    sequence: u16,
    data: &[u8],
) -> Result<(), super::NetworkError> {
    let request = IcmpPacket::echo_request(identifier, sequence, data);
    stack.send_ip(iface, dest, IpProtocol::Icmp, &request.serialize())
}

/// Send Destination Unreachable
pub fn send_dest_unreachable(
    stack: &NetworkStack,
    iface: &Arc<NetworkInterface>,
    dest: Ipv4Address,
    code: DestUnreachableCode,
    original: &[u8],
) -> Result<(), super::NetworkError> {
    // Include IP header + 8 bytes of original datagram
    let data_len = core::cmp::min(original.len(), 28);
    let icmp = IcmpPacket::destination_unreachable(code, &original[..data_len]);
    stack.send_ip(iface, dest, IpProtocol::Icmp, &icmp.serialize())
}

/// Send Time Exceeded
pub fn send_time_exceeded(
    stack: &NetworkStack,
    iface: &Arc<NetworkInterface>,
    dest: Ipv4Address,
    code: TimeExceededCode,
    original: &[u8],
) -> Result<(), super::NetworkError> {
    let data_len = core::cmp::min(original.len(), 28);
    let icmp = IcmpPacket::time_exceeded(code, &original[..data_len]);
    stack.send_ip(iface, dest, IpProtocol::Icmp, &icmp.serialize())
}
