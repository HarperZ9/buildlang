// ===============================================================================
// BUILDOS KERNEL - NETWORK STACK
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! TCP/IP Network Stack Implementation.
//!
//! This module implements a complete TCP/IP network stack:
//! - Ethernet frame processing
//! - ARP (Address Resolution Protocol)
//! - IPv4 (Internet Protocol version 4)
//! - ICMP (Internet Control Message Protocol)
//! - UDP (User Datagram Protocol)
//! - TCP (Transmission Control Protocol)
//! - BSD-style socket API

pub mod ethernet;
pub mod arp;
pub mod ip;
pub mod icmp;
pub mod udp;
pub mod tcp;
pub mod socket;
pub mod dhcp;
pub mod dns;
pub mod http;
pub mod tls;
pub mod netfilter;
mod checksum;

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use alloc::sync::Arc;
use spin::{Mutex, RwLock};
use core::sync::atomic::{AtomicU16, AtomicU32, Ordering};

pub use ethernet::{EthernetFrame, MacAddress, EtherType};
pub use arp::{ArpPacket, ArpCache};
pub use ip::{Ipv4Packet, Ipv4Address, IpProtocol};
pub use icmp::{IcmpPacket, IcmpType};
pub use udp::UdpPacket;
pub use tcp::{TcpPacket, TcpConnection, TcpState};
pub use socket::{Socket, SocketType, SocketAddr};

// =============================================================================
// CONSTANTS
// =============================================================================

/// Maximum Transmission Unit (standard Ethernet)
pub const MTU: usize = 1500;

/// Maximum segment size for TCP
pub const TCP_MSS: usize = MTU - 40; // IP header (20) + TCP header (20)

/// Maximum number of sockets
pub const MAX_SOCKETS: usize = 1024;

/// Maximum receive queue size
pub const RX_QUEUE_SIZE: usize = 256;

/// Maximum transmit queue size
pub const TX_QUEUE_SIZE: usize = 256;

/// ARP cache timeout in seconds
pub const ARP_CACHE_TIMEOUT: u64 = 300;

/// TCP retransmission timeout in milliseconds
pub const TCP_RTO_INIT: u64 = 1000;

/// TCP maximum retransmissions
pub const TCP_MAX_RETRIES: u32 = 5;

// =============================================================================
// NETWORK INTERFACE
// =============================================================================

/// Network interface configuration
#[derive(Debug, Clone)]
pub struct InterfaceConfig {
    /// Interface name
    pub name: [u8; 16],
    /// MAC address
    pub mac: MacAddress,
    /// IPv4 address
    pub ipv4: Ipv4Address,
    /// Subnet mask
    pub netmask: Ipv4Address,
    /// Default gateway
    pub gateway: Ipv4Address,
    /// MTU
    pub mtu: u16,
    /// Is interface up?
    pub is_up: bool,
}

impl Default for InterfaceConfig {
    fn default() -> Self {
        Self {
            name: *b"eth0\0\0\0\0\0\0\0\0\0\0\0\0",
            mac: MacAddress::ZERO,
            ipv4: Ipv4Address::ZERO,
            netmask: Ipv4Address::new(255, 255, 255, 0),
            gateway: Ipv4Address::ZERO,
            mtu: MTU as u16,
            is_up: false,
        }
    }
}

/// Network interface
pub struct NetworkInterface {
    /// Configuration
    pub config: RwLock<InterfaceConfig>,
    /// Receive queue
    rx_queue: Mutex<VecDeque<Vec<u8>>>,
    /// Transmit queue
    tx_queue: Mutex<VecDeque<Vec<u8>>>,
    /// Statistics
    stats: NetworkStats,
}

/// Network statistics
#[derive(Default)]
pub struct NetworkStats {
    /// Packets received
    pub rx_packets: AtomicU32,
    /// Packets transmitted
    pub tx_packets: AtomicU32,
    /// Bytes received
    pub rx_bytes: AtomicU32,
    /// Bytes transmitted
    pub tx_bytes: AtomicU32,
    /// Receive errors
    pub rx_errors: AtomicU32,
    /// Transmit errors
    pub tx_errors: AtomicU32,
    /// Packets dropped
    pub dropped: AtomicU32,
}

impl NetworkInterface {
    /// Create a new network interface
    pub fn new() -> Self {
        Self {
            config: RwLock::new(InterfaceConfig::default()),
            rx_queue: Mutex::new(VecDeque::with_capacity(RX_QUEUE_SIZE)),
            tx_queue: Mutex::new(VecDeque::with_capacity(TX_QUEUE_SIZE)),
            stats: NetworkStats::default(),
        }
    }

    /// Configure the interface
    pub fn configure(&self, config: InterfaceConfig) {
        *self.config.write() = config;
    }

    /// Receive a packet (called by driver)
    pub fn receive(&self, data: &[u8]) {
        let mut rx = self.rx_queue.lock();
        if rx.len() >= RX_QUEUE_SIZE {
            self.stats.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        rx.push_back(data.to_vec());
        self.stats.rx_packets.fetch_add(1, Ordering::Relaxed);
        self.stats.rx_bytes.fetch_add(data.len() as u32, Ordering::Relaxed);
    }

    /// Transmit a packet
    pub fn transmit(&self, data: Vec<u8>) -> Result<(), NetworkError> {
        let mut tx = self.tx_queue.lock();
        if tx.len() >= TX_QUEUE_SIZE {
            self.stats.dropped.fetch_add(1, Ordering::Relaxed);
            return Err(NetworkError::QueueFull);
        }
        self.stats.tx_packets.fetch_add(1, Ordering::Relaxed);
        self.stats.tx_bytes.fetch_add(data.len() as u32, Ordering::Relaxed);
        tx.push_back(data);
        Ok(())
    }

    /// Get next received packet
    pub fn poll_rx(&self) -> Option<Vec<u8>> {
        self.rx_queue.lock().pop_front()
    }

    /// Get next packet to transmit (called by driver)
    pub fn poll_tx(&self) -> Option<Vec<u8>> {
        self.tx_queue.lock().pop_front()
    }

    /// Get interface statistics
    pub fn get_stats(&self) -> (u32, u32, u32, u32) {
        (
            self.stats.rx_packets.load(Ordering::Relaxed),
            self.stats.tx_packets.load(Ordering::Relaxed),
            self.stats.rx_bytes.load(Ordering::Relaxed),
            self.stats.tx_bytes.load(Ordering::Relaxed),
        )
    }
}

// =============================================================================
// NETWORK STACK
// =============================================================================

/// Global network stack instance
static NETWORK_STACK: RwLock<Option<NetworkStack>> = RwLock::new(None);

/// The main network stack
pub struct NetworkStack {
    /// Network interfaces
    interfaces: Vec<Arc<NetworkInterface>>,
    /// ARP cache
    arp_cache: Arc<ArpCache>,
    /// Socket table
    sockets: RwLock<[Option<Arc<Socket>>; MAX_SOCKETS]>,
    /// Next ephemeral port
    next_port: AtomicU16,
    /// TCP connections
    tcp_connections: RwLock<Vec<Arc<TcpConnection>>>,
}

impl NetworkStack {
    /// Create a new network stack
    pub fn new() -> Self {
        const NONE_SOCKET: Option<Arc<Socket>> = None;
        Self {
            interfaces: Vec::new(),
            arp_cache: Arc::new(ArpCache::new()),
            sockets: RwLock::new([NONE_SOCKET; MAX_SOCKETS]),
            next_port: AtomicU16::new(49152), // Ephemeral port range start
            tcp_connections: RwLock::new(Vec::new()),
        }
    }

    /// Add a network interface
    pub fn add_interface(&mut self, iface: Arc<NetworkInterface>) {
        self.interfaces.push(iface);
    }

    /// Get interface by index
    pub fn get_interface(&self, index: usize) -> Option<&Arc<NetworkInterface>> {
        self.interfaces.get(index)
    }

    /// Get the primary interface
    pub fn primary_interface(&self) -> Option<&Arc<NetworkInterface>> {
        self.interfaces.first()
    }

    /// Allocate an ephemeral port
    pub fn allocate_port(&self) -> u16 {
        self.alloc_port()
    }

    /// Allocate an ephemeral port (internal)
    pub fn alloc_port(&self) -> u16 {
        loop {
            let port = self.next_port.fetch_add(1, Ordering::SeqCst);
            // Reset if port wrapped around or is below ephemeral range
            if port < 49152 {
                self.next_port.store(49152, Ordering::SeqCst);
                continue;
            }
            // Check if port is in use
            let sockets = self.sockets.read();
            let in_use = sockets.iter().any(|s| {
                s.as_ref().map(|sock| sock.local_port() == port).unwrap_or(false)
            });
            if !in_use {
                return port;
            }
        }
    }

    /// Allocate a socket
    pub fn alloc_socket(&self, socket: Socket) -> Result<usize, NetworkError> {
        let mut sockets = self.sockets.write();
        for (i, slot) in sockets.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(Arc::new(socket));
                return Ok(i);
            }
        }
        Err(NetworkError::TooManySockets)
    }

    /// Get socket by descriptor
    pub fn get_socket(&self, fd: usize) -> Option<Arc<Socket>> {
        let sockets = self.sockets.read();
        sockets.get(fd).and_then(|s| s.clone())
    }

    /// Free a socket
    pub fn free_socket(&self, fd: usize) {
        let mut sockets = self.sockets.write();
        if let Some(slot) = sockets.get_mut(fd) {
            *slot = None;
        }
    }

    /// Get ARP cache
    pub fn arp_cache(&self) -> &Arc<ArpCache> {
        &self.arp_cache
    }

    /// Process incoming packets
    pub fn process_rx(&self) {
        for iface in &self.interfaces {
            while let Some(data) = iface.poll_rx() {
                self.handle_packet(iface, &data);
            }
        }
    }

    /// Handle a received packet
    fn handle_packet(&self, iface: &Arc<NetworkInterface>, data: &[u8]) {
        // Parse Ethernet frame
        let frame = match EthernetFrame::parse(data) {
            Some(f) => f,
            None => return,
        };

        match frame.ethertype {
            EtherType::Arp => {
                // Handle ARP
                if let Some(arp) = ArpPacket::parse(frame.payload) {
                    arp::handle_arp(self, iface, &arp);
                }
            }
            EtherType::Ipv4 => {
                // Handle IPv4
                if let Some(ip) = Ipv4Packet::parse(frame.payload) {
                    self.handle_ipv4(iface, &ip);
                }
            }
            EtherType::Ipv6 => {
                // IPv6 not yet implemented
            }
            _ => {}
        }
    }

    /// Handle an IPv4 packet
    fn handle_ipv4(&self, iface: &Arc<NetworkInterface>, ip: &Ipv4Packet) {
        // Verify checksum
        if !ip.verify_checksum() {
            return;
        }

        match ip.protocol {
            IpProtocol::Icmp => {
                if let Some(icmp) = IcmpPacket::parse(ip.payload) {
                    icmp::handle_icmp(self, iface, ip, &icmp);
                }
            }
            IpProtocol::Tcp => {
                if let Some(tcp) = TcpPacket::parse(ip.payload) {
                    tcp::handle_tcp(self, iface, ip, &tcp);
                }
            }
            IpProtocol::Udp => {
                if let Some(udp) = UdpPacket::parse(ip.payload) {
                    udp::handle_udp(self, iface, ip, &udp);
                }
            }
            _ => {}
        }
    }

    /// Send an IP packet
    pub fn send_ip(&self, iface: &Arc<NetworkInterface>, dest_ip: Ipv4Address, protocol: IpProtocol, payload: &[u8]) -> Result<(), NetworkError> {
        let config = iface.config.read();

        // Build IP packet
        let ip_packet = Ipv4Packet::new(
            config.ipv4,
            dest_ip,
            protocol,
            payload,
        );

        // Resolve MAC address
        let dest_mac = if dest_ip.is_local(&config.ipv4, &config.netmask) {
            // Same network - resolve directly
            self.arp_cache.lookup(dest_ip).ok_or(NetworkError::NoRoute)?
        } else {
            // Different network - use gateway
            self.arp_cache.lookup(config.gateway).ok_or(NetworkError::NoRoute)?
        };

        drop(config);

        // Build Ethernet frame
        let config = iface.config.read();
        let ip_data = ip_packet.serialize();
        let frame = EthernetFrame::new(
            dest_mac,
            config.mac,
            EtherType::Ipv4,
            &ip_data,
        );
        let frame_data = frame.serialize();
        drop(config);

        iface.transmit(frame_data)
    }
}

// =============================================================================
// ERRORS
// =============================================================================

/// Network errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkError {
    /// No route to host
    NoRoute,
    /// Connection refused
    ConnectionRefused,
    /// Connection reset
    ConnectionReset,
    /// Connection timed out
    TimedOut,
    /// Address in use
    AddressInUse,
    /// Address not available
    AddressNotAvailable,
    /// Network unreachable
    NetworkUnreachable,
    /// Host unreachable
    HostUnreachable,
    /// Operation would block
    WouldBlock,
    /// Invalid argument
    InvalidArgument,
    /// Not connected
    NotConnected,
    /// Already connected
    AlreadyConnected,
    /// Queue full
    QueueFull,
    /// Too many sockets
    TooManySockets,
    /// Buffer too small
    BufferTooSmall,
    /// Protocol error
    ProtocolError,
    /// Not supported
    NotSupported,
    /// Socket already bound
    AlreadyBound,
    /// Socket not bound
    NotBound,
    /// Invalid socket
    InvalidSocket,
    /// Invalid address family
    InvalidFamily,
    /// Invalid socket type
    InvalidType,
    /// Invalid address
    InvalidAddress,
    /// Invalid socket option
    InvalidOption,
}

impl NetworkError {
    /// Convert to errno
    pub fn to_errno(&self) -> i32 {
        match self {
            NetworkError::NoRoute => -101,           // ENETUNREACH
            NetworkError::ConnectionRefused => -111, // ECONNREFUSED
            NetworkError::ConnectionReset => -104,   // ECONNRESET
            NetworkError::TimedOut => -110,          // ETIMEDOUT
            NetworkError::AddressInUse => -98,       // EADDRINUSE
            NetworkError::AddressNotAvailable => -99, // EADDRNOTAVAIL
            NetworkError::NetworkUnreachable => -101, // ENETUNREACH
            NetworkError::HostUnreachable => -113,   // EHOSTUNREACH
            NetworkError::WouldBlock => -11,         // EAGAIN
            NetworkError::InvalidArgument => -22,    // EINVAL
            NetworkError::NotConnected => -107,      // ENOTCONN
            NetworkError::AlreadyConnected => -106,  // EISCONN
            NetworkError::QueueFull => -105,         // ENOBUFS
            NetworkError::TooManySockets => -24,     // EMFILE
            NetworkError::BufferTooSmall => -105,    // ENOBUFS
            NetworkError::ProtocolError => -71,      // EPROTO
            NetworkError::NotSupported => -95,       // EOPNOTSUPP
            NetworkError::AlreadyBound => -22,       // EINVAL
            NetworkError::NotBound => -22,           // EINVAL
            NetworkError::InvalidSocket => -88,      // ENOTSOCK
            NetworkError::InvalidFamily => -97,      // EAFNOSUPPORT
            NetworkError::InvalidType => -94,        // ESOCKTNOSUPPORT
            NetworkError::InvalidAddress => -22,     // EINVAL
            NetworkError::InvalidOption => -92,      // ENOPROTOOPT
        }
    }
}

// =============================================================================
// PUBLIC API
// =============================================================================

/// Initialize the network stack
pub fn init() {
    let mut stack = NetworkStack::new();

    // Create default interface
    let iface = Arc::new(NetworkInterface::new());

    // Get MAC from driver
    if crate::drivers::network::is_available() {
        let mac_bytes = crate::drivers::network::mac_address();
        let config = InterfaceConfig {
            name: *b"eth0\0\0\0\0\0\0\0\0\0\0\0\0",
            mac: MacAddress(mac_bytes),
            ipv4: Ipv4Address::new(10, 0, 2, 15), // QEMU default
            netmask: Ipv4Address::new(255, 255, 255, 0),
            gateway: Ipv4Address::new(10, 0, 2, 2),
            mtu: 1500,
            is_up: true,
        };
        iface.configure(config);
    }

    stack.add_interface(iface);

    *NETWORK_STACK.write() = Some(stack);
}

/// Get the network stack
pub fn get_stack() -> Option<&'static NetworkStack> {
    // Safety: The stack is initialized once and never moved
    unsafe {
        NETWORK_STACK.read().as_ref().map(|s| &*(s as *const _))
    }
}

/// Process network I/O (called periodically from scheduler)
pub fn poll() {
    // Poll driver for incoming packets and push to network interface
    poll_driver();

    // Process received packets through the network stack
    if let Some(stack) = get_stack() {
        stack.process_rx();

        // Process TCP timers (retransmission, TIME_WAIT cleanup)
        tcp::timer_tick(stack);

        // Process TX queue - send packets to driver
        for iface in &stack.interfaces {
            while let Some(data) = iface.poll_tx() {
                let _ = crate::drivers::network::transmit(&data);
            }
        }
    }
}

/// Poll network driver for incoming packets
fn poll_driver() {
    // Get packets from driver and push to network interface
    while let Some(data) = crate::drivers::network::receive() {
        if let Some(stack) = get_stack() {
            if let Some(iface) = stack.primary_interface() {
                iface.receive(&data);
            }
        }
    }
}

/// Get MAC address (wrapper for driver)
pub fn mac_address() -> Option<[u8; 6]> {
    crate::drivers::network::get_mac()
}
