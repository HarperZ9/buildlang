// ===============================================================================
// BUILDOS KERNEL - UDP (USER DATAGRAM PROTOCOL)
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! UDP protocol implementation (RFC 768).

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

use super::checksum;
use super::ip::{Ipv4Packet, Ipv4Address, IpProtocol, PseudoHeader};
use super::{NetworkStack, NetworkInterface, NetworkError};

// =============================================================================
// CONSTANTS
// =============================================================================

/// UDP header size
pub const UDP_HEADER_SIZE: usize = 8;

/// Maximum UDP payload size (with standard MTU)
pub const UDP_MAX_PAYLOAD: usize = 65507; // 65535 - 20 (IP) - 8 (UDP)

// =============================================================================
// UDP PACKET
// =============================================================================

/// UDP datagram
#[derive(Debug, Clone)]
pub struct UdpPacket<'a> {
    /// Source port
    pub src_port: u16,
    /// Destination port
    pub dest_port: u16,
    /// Length (header + data)
    pub length: u16,
    /// Checksum
    pub checksum: u16,
    /// Payload data
    pub data: &'a [u8],
}

impl<'a> UdpPacket<'a> {
    /// Parse a UDP packet from bytes
    pub fn parse(data: &'a [u8]) -> Option<Self> {
        if data.len() < UDP_HEADER_SIZE {
            return None;
        }

        let src_port = u16::from_be_bytes([data[0], data[1]]);
        let dest_port = u16::from_be_bytes([data[2], data[3]]);
        let length = u16::from_be_bytes([data[4], data[5]]);
        let checksum = u16::from_be_bytes([data[6], data[7]]);

        let payload_len = (length as usize).saturating_sub(UDP_HEADER_SIZE);
        let payload = if data.len() >= UDP_HEADER_SIZE + payload_len {
            &data[UDP_HEADER_SIZE..UDP_HEADER_SIZE + payload_len]
        } else {
            &data[UDP_HEADER_SIZE..]
        };

        Some(Self {
            src_port,
            dest_port,
            length,
            checksum,
            data: payload,
        })
    }

    /// Create a new UDP packet
    pub fn new(src_port: u16, dest_port: u16, data: &'a [u8]) -> Self {
        let length = (UDP_HEADER_SIZE + data.len()) as u16;

        Self {
            src_port,
            dest_port,
            length,
            checksum: 0,
            data,
        }
    }

    /// Verify checksum (requires IP addresses for pseudo header)
    pub fn verify_checksum(&self, src_ip: Ipv4Address, dest_ip: Ipv4Address) -> bool {
        if self.checksum == 0 {
            return true; // Checksum disabled
        }

        let pseudo = PseudoHeader::new(src_ip, dest_ip, IpProtocol::Udp, self.length);
        let header_and_data = self.serialize_no_checksum();

        let computed = checksum::checksum_with_pseudo(&pseudo.to_bytes(), &header_and_data);
        computed == 0
    }

    /// Calculate checksum
    pub fn calculate_checksum(&self, src_ip: Ipv4Address, dest_ip: Ipv4Address) -> u16 {
        let pseudo = PseudoHeader::new(src_ip, dest_ip, IpProtocol::Udp, self.length);
        let header_and_data = self.serialize_no_checksum();

        checksum::checksum_with_pseudo(&pseudo.to_bytes(), &header_and_data)
    }

    /// Serialize without checksum
    fn serialize_no_checksum(&self) -> Vec<u8> {
        let mut packet = Vec::with_capacity(UDP_HEADER_SIZE + self.data.len());
        packet.extend_from_slice(&self.src_port.to_be_bytes());
        packet.extend_from_slice(&self.dest_port.to_be_bytes());
        packet.extend_from_slice(&self.length.to_be_bytes());
        packet.push(0); // Checksum placeholder
        packet.push(0);
        packet.extend_from_slice(self.data);
        packet
    }

    /// Serialize to bytes (with checksum)
    pub fn serialize(&self, src_ip: Ipv4Address, dest_ip: Ipv4Address) -> Vec<u8> {
        let mut packet = self.serialize_no_checksum();
        let checksum = self.calculate_checksum(src_ip, dest_ip);
        packet[6] = (checksum >> 8) as u8;
        packet[7] = checksum as u8;
        packet
    }
}

// =============================================================================
// UDP SOCKET
// =============================================================================

/// Received UDP datagram with source info
#[derive(Debug, Clone)]
pub struct UdpDatagram {
    /// Source IP address
    pub src_ip: Ipv4Address,
    /// Source port
    pub src_port: u16,
    /// Data
    pub data: Vec<u8>,
}

/// UDP socket state
pub struct UdpSocket {
    /// Local IP (0.0.0.0 for any)
    pub local_ip: Ipv4Address,
    /// Local port
    pub local_port: u16,
    /// Receive queue
    rx_queue: Mutex<VecDeque<UdpDatagram>>,
    /// Maximum receive queue size
    rx_queue_max: usize,
    /// Connected remote address (for connected UDP)
    connected: Mutex<Option<(Ipv4Address, u16)>>,
}

impl UdpSocket {
    /// Create a new UDP socket
    pub fn new(local_ip: Ipv4Address, local_port: u16) -> Self {
        Self {
            local_ip,
            local_port,
            rx_queue: Mutex::new(VecDeque::with_capacity(64)),
            rx_queue_max: 64,
            connected: Mutex::new(None),
        }
    }

    /// Bind to a local address
    pub fn bind(&mut self, ip: Ipv4Address, port: u16) {
        self.local_ip = ip;
        self.local_port = port;
    }

    /// Connect to a remote address (for connected UDP)
    pub fn connect(&self, ip: Ipv4Address, port: u16) {
        *self.connected.lock() = Some((ip, port));
    }

    /// Disconnect (unconnect)
    pub fn disconnect(&self) {
        *self.connected.lock() = None;
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.connected.lock().is_some()
    }

    /// Get connected address
    pub fn get_connected(&self) -> Option<(Ipv4Address, u16)> {
        *self.connected.lock()
    }

    /// Receive a datagram (non-blocking)
    pub fn recv(&self) -> Option<UdpDatagram> {
        self.rx_queue.lock().pop_front()
    }

    /// Queue a received datagram
    pub fn queue_rx(&self, datagram: UdpDatagram) -> bool {
        let mut queue = self.rx_queue.lock();
        if queue.len() >= self.rx_queue_max {
            return false;
        }
        queue.push_back(datagram);
        true
    }

    /// Check if receive queue is empty
    pub fn rx_empty(&self) -> bool {
        self.rx_queue.lock().is_empty()
    }

    /// Get number of datagrams in receive queue
    pub fn rx_count(&self) -> usize {
        self.rx_queue.lock().len()
    }
}

// =============================================================================
// UDP HANDLING
// =============================================================================

/// Handle an incoming UDP packet
pub fn handle_udp(
    stack: &NetworkStack,
    _iface: &Arc<NetworkInterface>,
    ip: &Ipv4Packet,
    udp: &UdpPacket,
) {
    // Verify checksum (optional in UDP, but we check if present)
    if udp.checksum != 0 && !udp.verify_checksum(ip.src, ip.dest) {
        return; // Invalid checksum
    }

    // Find matching socket
    let sockets = stack.sockets.read();
    for socket_opt in sockets.iter() {
        if let Some(socket) = socket_opt {
            if socket.socket_type() == super::socket::SocketType::Dgram {
                // Check if this socket matches
                if socket.local_port() == udp.dest_port {
                    // Check IP filter
                    let local_ip = socket.local_ip();
                    if local_ip.is_unspecified() || local_ip == ip.dest {
                        // Deliver to socket
                        socket.deliver_udp(UdpDatagram {
                            src_ip: ip.src,
                            src_port: udp.src_port,
                            data: udp.data.to_vec(),
                        });
                        return;
                    }
                }
            }
        }
    }

    // No socket found - optionally send ICMP port unreachable
    // (Skipped for now to avoid amplification attacks)
}

/// Send a UDP datagram
pub fn send_udp(
    stack: &NetworkStack,
    iface: &Arc<NetworkInterface>,
    src_ip: Ipv4Address,
    src_port: u16,
    dest_ip: Ipv4Address,
    dest_port: u16,
    data: &[u8],
) -> Result<(), NetworkError> {
    if data.len() > UDP_MAX_PAYLOAD {
        return Err(NetworkError::BufferTooSmall);
    }

    let packet = UdpPacket::new(src_port, dest_port, data);
    let bytes = packet.serialize(src_ip, dest_ip);

    stack.send_ip(iface, dest_ip, IpProtocol::Udp, &bytes)
}
