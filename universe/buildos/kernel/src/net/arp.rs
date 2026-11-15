// ===============================================================================
// BUILDOS KERNEL - ARP (ADDRESS RESOLUTION PROTOCOL)
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! ARP protocol implementation (RFC 826).

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

use super::ethernet::{MacAddress, EtherType, EthernetFrame};
use super::ip::Ipv4Address;
use super::{NetworkStack, NetworkInterface, ARP_CACHE_TIMEOUT};
use crate::drivers::timer;

// =============================================================================
// CONSTANTS
// =============================================================================

/// ARP packet size
pub const ARP_PACKET_SIZE: usize = 28;

/// Hardware type: Ethernet
pub const HARDWARE_TYPE_ETHERNET: u16 = 1;

/// Protocol type: IPv4
pub const PROTOCOL_TYPE_IPV4: u16 = 0x0800;

/// Hardware address length (Ethernet)
pub const HARDWARE_ADDR_LEN: u8 = 6;

/// Protocol address length (IPv4)
pub const PROTOCOL_ADDR_LEN: u8 = 4;

// =============================================================================
// ARP OPERATIONS
// =============================================================================

/// ARP operation codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ArpOperation {
    /// ARP request
    Request = 1,
    /// ARP reply
    Reply = 2,
    /// RARP request (obsolete)
    RarpRequest = 3,
    /// RARP reply (obsolete)
    RarpReply = 4,
}

impl ArpOperation {
    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            1 => Some(ArpOperation::Request),
            2 => Some(ArpOperation::Reply),
            3 => Some(ArpOperation::RarpRequest),
            4 => Some(ArpOperation::RarpReply),
            _ => None,
        }
    }
}

// =============================================================================
// ARP PACKET
// =============================================================================

/// ARP packet structure
#[derive(Debug, Clone)]
pub struct ArpPacket {
    /// Hardware type (1 for Ethernet)
    pub hardware_type: u16,
    /// Protocol type (0x0800 for IPv4)
    pub protocol_type: u16,
    /// Hardware address length
    pub hardware_len: u8,
    /// Protocol address length
    pub protocol_len: u8,
    /// Operation
    pub operation: ArpOperation,
    /// Sender hardware address (MAC)
    pub sender_mac: MacAddress,
    /// Sender protocol address (IP)
    pub sender_ip: Ipv4Address,
    /// Target hardware address (MAC)
    pub target_mac: MacAddress,
    /// Target protocol address (IP)
    pub target_ip: Ipv4Address,
}

impl ArpPacket {
    /// Parse an ARP packet from bytes
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < ARP_PACKET_SIZE {
            return None;
        }

        let hardware_type = u16::from_be_bytes([data[0], data[1]]);
        let protocol_type = u16::from_be_bytes([data[2], data[3]]);
        let hardware_len = data[4];
        let protocol_len = data[5];
        let operation = ArpOperation::from_u16(u16::from_be_bytes([data[6], data[7]]))?;

        // Verify expected lengths
        if hardware_len != HARDWARE_ADDR_LEN || protocol_len != PROTOCOL_ADDR_LEN {
            return None;
        }

        let sender_mac = MacAddress::from_bytes(&data[8..14])?;
        let sender_ip = Ipv4Address::from_bytes(&data[14..18])?;
        let target_mac = MacAddress::from_bytes(&data[18..24])?;
        let target_ip = Ipv4Address::from_bytes(&data[24..28])?;

        Some(Self {
            hardware_type,
            protocol_type,
            hardware_len,
            protocol_len,
            operation,
            sender_mac,
            sender_ip,
            target_mac,
            target_ip,
        })
    }

    /// Create an ARP request
    pub fn new_request(sender_mac: MacAddress, sender_ip: Ipv4Address, target_ip: Ipv4Address) -> Self {
        Self {
            hardware_type: HARDWARE_TYPE_ETHERNET,
            protocol_type: PROTOCOL_TYPE_IPV4,
            hardware_len: HARDWARE_ADDR_LEN,
            protocol_len: PROTOCOL_ADDR_LEN,
            operation: ArpOperation::Request,
            sender_mac,
            sender_ip,
            target_mac: MacAddress::ZERO,
            target_ip,
        }
    }

    /// Create an ARP reply
    pub fn new_reply(
        sender_mac: MacAddress,
        sender_ip: Ipv4Address,
        target_mac: MacAddress,
        target_ip: Ipv4Address,
    ) -> Self {
        Self {
            hardware_type: HARDWARE_TYPE_ETHERNET,
            protocol_type: PROTOCOL_TYPE_IPV4,
            hardware_len: HARDWARE_ADDR_LEN,
            protocol_len: PROTOCOL_ADDR_LEN,
            operation: ArpOperation::Reply,
            sender_mac,
            sender_ip,
            target_mac,
            target_ip,
        }
    }

    /// Serialize to bytes
    pub fn serialize(&self) -> Vec<u8> {
        let mut packet = Vec::with_capacity(ARP_PACKET_SIZE);

        packet.extend_from_slice(&self.hardware_type.to_be_bytes());
        packet.extend_from_slice(&self.protocol_type.to_be_bytes());
        packet.push(self.hardware_len);
        packet.push(self.protocol_len);
        packet.extend_from_slice(&(self.operation as u16).to_be_bytes());
        packet.extend_from_slice(self.sender_mac.as_bytes());
        packet.extend_from_slice(&self.sender_ip.octets());
        packet.extend_from_slice(self.target_mac.as_bytes());
        packet.extend_from_slice(&self.target_ip.octets());

        packet
    }
}

// =============================================================================
// ARP CACHE
// =============================================================================

/// ARP cache entry
#[derive(Debug, Clone)]
pub struct ArpCacheEntry {
    /// MAC address
    pub mac: MacAddress,
    /// Entry creation time (monotonic ns)
    pub timestamp: u64,
    /// Is entry complete?
    pub complete: bool,
}

/// ARP cache
pub struct ArpCache {
    /// Cache entries: IP -> MAC
    entries: Mutex<BTreeMap<Ipv4Address, ArpCacheEntry>>,
}

impl ArpCache {
    /// Create a new ARP cache
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(BTreeMap::new()),
        }
    }

    /// Lookup MAC address for IP
    pub fn lookup(&self, ip: Ipv4Address) -> Option<MacAddress> {
        let entries = self.entries.lock();
        entries.get(&ip).and_then(|entry| {
            // Check if expired
            let now = timer::monotonic_ns();
            let age = (now - entry.timestamp) / 1_000_000_000; // Convert to seconds
            if age > ARP_CACHE_TIMEOUT {
                None
            } else if entry.complete {
                Some(entry.mac)
            } else {
                None
            }
        })
    }

    /// Insert or update cache entry
    pub fn insert(&self, ip: Ipv4Address, mac: MacAddress) {
        let mut entries = self.entries.lock();
        entries.insert(ip, ArpCacheEntry {
            mac,
            timestamp: timer::monotonic_ns(),
            complete: true,
        });
    }

    /// Insert incomplete entry (waiting for reply)
    pub fn insert_pending(&self, ip: Ipv4Address) {
        let mut entries = self.entries.lock();
        if !entries.contains_key(&ip) {
            entries.insert(ip, ArpCacheEntry {
                mac: MacAddress::ZERO,
                timestamp: timer::monotonic_ns(),
                complete: false,
            });
        }
    }

    /// Remove entry
    pub fn remove(&self, ip: Ipv4Address) {
        let mut entries = self.entries.lock();
        entries.remove(&ip);
    }

    /// Clear expired entries
    pub fn gc(&self) {
        let now = timer::monotonic_ns();
        let mut entries = self.entries.lock();
        entries.retain(|_, entry| {
            let age = (now - entry.timestamp) / 1_000_000_000;
            age <= ARP_CACHE_TIMEOUT
        });
    }

    /// Get all entries (for debugging)
    pub fn all_entries(&self) -> Vec<(Ipv4Address, MacAddress)> {
        let entries = self.entries.lock();
        entries
            .iter()
            .filter(|(_, e)| e.complete)
            .map(|(ip, e)| (*ip, e.mac))
            .collect()
    }
}

// =============================================================================
// ARP HANDLING
// =============================================================================

/// Handle an incoming ARP packet
pub fn handle_arp(stack: &NetworkStack, iface: &Arc<NetworkInterface>, arp: &ArpPacket) {
    let config = iface.config.read();

    // Only handle Ethernet/IPv4 ARP
    if arp.hardware_type != HARDWARE_TYPE_ETHERNET || arp.protocol_type != PROTOCOL_TYPE_IPV4 {
        return;
    }

    // Update cache with sender's info (even if not for us)
    stack.arp_cache().insert(arp.sender_ip, arp.sender_mac);

    // Check if this is for us
    if arp.target_ip != config.ipv4 {
        return;
    }

    match arp.operation {
        ArpOperation::Request => {
            // Send ARP reply
            let reply = ArpPacket::new_reply(
                config.mac,
                config.ipv4,
                arp.sender_mac,
                arp.sender_ip,
            );

            let reply_data = reply.serialize();
            let frame = EthernetFrame::new(
                arp.sender_mac,
                config.mac,
                EtherType::Arp,
                &reply_data,
            );
            let frame_data = frame.serialize();

            drop(config);
            let _ = iface.transmit(frame_data);
        }
        ArpOperation::Reply => {
            // Already updated cache above
        }
        _ => {}
    }
}

/// Send an ARP request
pub fn send_arp_request(iface: &Arc<NetworkInterface>, target_ip: Ipv4Address) {
    let config = iface.config.read();

    let request = ArpPacket::new_request(config.mac, config.ipv4, target_ip);
    let request_data = request.serialize();

    let frame = EthernetFrame::new(
        MacAddress::BROADCAST,
        config.mac,
        EtherType::Arp,
        &request_data,
    );
    let frame_data = frame.serialize();

    drop(config);
    let _ = iface.transmit(frame_data);
}

/// Resolve IP address to MAC (may send ARP request)
pub fn resolve(stack: &NetworkStack, iface: &Arc<NetworkInterface>, ip: Ipv4Address) -> Option<MacAddress> {
    // Check cache first
    if let Some(mac) = stack.arp_cache().lookup(ip) {
        return Some(mac);
    }

    // Send ARP request
    stack.arp_cache().insert_pending(ip);
    send_arp_request(iface, ip);

    // Would need to wait/retry in a real implementation
    None
}
