// ===============================================================================
// BUILDOS KERNEL - DHCP CLIENT
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! DHCP (Dynamic Host Configuration Protocol) Client.
//!
//! Implements RFC 2131 DHCP for automatic network configuration:
//! - IP address acquisition via DORA (Discover, Offer, Request, Ack)
//! - Lease management with renewal and rebinding
//! - Option parsing for subnet, gateway, DNS, etc.
//! - Lease persistence for fast recovery after reboot
//!
//! Supported options:
//! - Subnet mask (1)
//! - Router/gateway (3)
//! - DNS servers (6)
//! - Domain name (15)
//! - Broadcast address (28)
//! - Lease time (51)
//! - DHCP message type (53)
//! - Server identifier (54)
//! - Renewal time T1 (58)
//! - Rebinding time T2 (59)

use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use spin::{Mutex, RwLock};

use super::{Ipv4Address, MacAddress, NetworkInterface, NetworkError};

// =============================================================================
// DHCP CONSTANTS
// =============================================================================

/// DHCP server port
pub const DHCP_SERVER_PORT: u16 = 67;

/// DHCP client port
pub const DHCP_CLIENT_PORT: u16 = 68;

/// DHCP magic cookie
pub const DHCP_MAGIC_COOKIE: u32 = 0x63825363;

/// Maximum DHCP message size
pub const DHCP_MAX_MESSAGE_SIZE: usize = 576;

/// DHCP operation codes
mod op {
    pub const BOOTREQUEST: u8 = 1;
    pub const BOOTREPLY: u8 = 2;
}

/// Hardware types
mod htype {
    pub const ETHERNET: u8 = 1;
}

/// DHCP message types (option 53)
mod msg_type {
    pub const DISCOVER: u8 = 1;
    pub const OFFER: u8 = 2;
    pub const REQUEST: u8 = 3;
    pub const DECLINE: u8 = 4;
    pub const ACK: u8 = 5;
    pub const NAK: u8 = 6;
    pub const RELEASE: u8 = 7;
    pub const INFORM: u8 = 8;
}

/// DHCP options
mod option {
    pub const PAD: u8 = 0;
    pub const SUBNET_MASK: u8 = 1;
    pub const TIME_OFFSET: u8 = 2;
    pub const ROUTER: u8 = 3;
    pub const TIME_SERVER: u8 = 4;
    pub const NAME_SERVER: u8 = 5;
    pub const DNS_SERVER: u8 = 6;
    pub const LOG_SERVER: u8 = 7;
    pub const HOSTNAME: u8 = 12;
    pub const DOMAIN_NAME: u8 = 15;
    pub const MTU: u8 = 26;
    pub const BROADCAST: u8 = 28;
    pub const NTP_SERVER: u8 = 42;
    pub const REQUESTED_IP: u8 = 50;
    pub const LEASE_TIME: u8 = 51;
    pub const MESSAGE_TYPE: u8 = 53;
    pub const SERVER_ID: u8 = 54;
    pub const PARAM_LIST: u8 = 55;
    pub const MESSAGE: u8 = 56;
    pub const MAX_SIZE: u8 = 57;
    pub const RENEWAL_TIME: u8 = 58;
    pub const REBINDING_TIME: u8 = 59;
    pub const CLIENT_ID: u8 = 61;
    pub const END: u8 = 255;
}

// =============================================================================
// DHCP CLIENT STATE
// =============================================================================

/// DHCP client states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DhcpState {
    /// Initial state, no lease
    Init = 0,
    /// Selecting - sent DISCOVER, waiting for OFFERs
    Selecting = 1,
    /// Requesting - sent REQUEST, waiting for ACK
    Requesting = 2,
    /// Bound - have valid lease
    Bound = 3,
    /// Renewing - attempting to renew with server
    Renewing = 4,
    /// Rebinding - attempting to rebind with any server
    Rebinding = 5,
    /// Rebooting - have previous lease, requesting same IP
    Rebooting = 6,
}

/// DHCP lease information
#[derive(Clone)]
pub struct DhcpLease {
    /// Assigned IP address
    pub client_ip: Ipv4Address,
    /// Server IP address
    pub server_ip: Ipv4Address,
    /// Subnet mask
    pub subnet_mask: Ipv4Address,
    /// Default gateway
    pub gateway: Ipv4Address,
    /// Broadcast address
    pub broadcast: Ipv4Address,
    /// DNS servers
    pub dns_servers: Vec<Ipv4Address>,
    /// Domain name
    pub domain: String,
    /// Lease time in seconds
    pub lease_time: u32,
    /// Renewal time (T1) in seconds
    pub renewal_time: u32,
    /// Rebinding time (T2) in seconds
    pub rebinding_time: u32,
    /// Timestamp when lease was obtained (kernel ticks)
    pub obtained_at: u64,
}

impl Default for DhcpLease {
    fn default() -> Self {
        Self {
            client_ip: Ipv4Address::ZERO,
            server_ip: Ipv4Address::ZERO,
            subnet_mask: Ipv4Address::new(255, 255, 255, 0),
            gateway: Ipv4Address::ZERO,
            broadcast: Ipv4Address::BROADCAST,
            dns_servers: Vec::new(),
            domain: String::new(),
            lease_time: 0,
            renewal_time: 0,
            rebinding_time: 0,
            obtained_at: 0,
        }
    }
}

impl DhcpLease {
    /// Check if lease is valid (not expired)
    pub fn is_valid(&self, current_time: u64) -> bool {
        if self.lease_time == 0 {
            return false;
        }
        let elapsed = current_time.saturating_sub(self.obtained_at);
        elapsed < (self.lease_time as u64 * 1000) // Convert to ms
    }

    /// Check if lease needs renewal
    pub fn needs_renewal(&self, current_time: u64) -> bool {
        if self.renewal_time == 0 {
            return false;
        }
        let elapsed = current_time.saturating_sub(self.obtained_at);
        elapsed >= (self.renewal_time as u64 * 1000)
    }

    /// Check if lease needs rebinding
    pub fn needs_rebinding(&self, current_time: u64) -> bool {
        if self.rebinding_time == 0 {
            return false;
        }
        let elapsed = current_time.saturating_sub(self.obtained_at);
        elapsed >= (self.rebinding_time as u64 * 1000)
    }
}

// =============================================================================
// DHCP MESSAGE
// =============================================================================

/// DHCP message header (fixed portion)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct DhcpHeader {
    /// Operation (1 = request, 2 = reply)
    pub op: u8,
    /// Hardware type (1 = Ethernet)
    pub htype: u8,
    /// Hardware address length (6 for Ethernet)
    pub hlen: u8,
    /// Hops
    pub hops: u8,
    /// Transaction ID
    pub xid: u32,
    /// Seconds elapsed
    pub secs: u16,
    /// Flags (broadcast = 0x8000)
    pub flags: u16,
    /// Client IP address (if client knows it)
    pub ciaddr: [u8; 4],
    /// Your IP address (server fills this in)
    pub yiaddr: [u8; 4],
    /// Server IP address
    pub siaddr: [u8; 4],
    /// Gateway IP address
    pub giaddr: [u8; 4],
    /// Client hardware address
    pub chaddr: [u8; 16],
    /// Server hostname (optional)
    pub sname: [u8; 64],
    /// Boot filename (optional)
    pub file: [u8; 128],
}

impl DhcpHeader {
    /// Create a new DHCP request header
    pub fn new_request(xid: u32, mac: &MacAddress) -> Self {
        let mut header = Self {
            op: op::BOOTREQUEST,
            htype: htype::ETHERNET,
            hlen: 6,
            hops: 0,
            xid,
            secs: 0,
            flags: 0x8000u16.to_be(), // Broadcast flag
            ciaddr: [0; 4],
            yiaddr: [0; 4],
            siaddr: [0; 4],
            giaddr: [0; 4],
            chaddr: [0; 16],
            sname: [0; 64],
            file: [0; 128],
        };
        header.chaddr[..6].copy_from_slice(&mac.0);
        header
    }

    /// Get client hardware address as MacAddress
    pub fn client_mac(&self) -> MacAddress {
        let mut mac = [0u8; 6];
        mac.copy_from_slice(&self.chaddr[..6]);
        MacAddress(mac)
    }

    /// Get assigned IP address
    pub fn your_ip(&self) -> Ipv4Address {
        Ipv4Address::from_bytes(&self.yiaddr).unwrap_or(Ipv4Address::ZERO)
    }

    /// Get server IP address
    pub fn server_ip(&self) -> Ipv4Address {
        Ipv4Address::from_bytes(&self.siaddr).unwrap_or(Ipv4Address::ZERO)
    }

    /// Serialize header to bytes
    pub fn to_bytes(&self) -> [u8; 236] {
        unsafe {
            core::mem::transmute_copy(self)
        }
    }
}

/// DHCP message (header + options)
pub struct DhcpMessage {
    /// Fixed header
    pub header: DhcpHeader,
    /// Options (variable length)
    pub options: Vec<u8>,
}

impl DhcpMessage {
    /// Create new DHCP discover message
    pub fn discover(xid: u32, mac: &MacAddress) -> Self {
        let header = DhcpHeader::new_request(xid, mac);
        let mut options = Vec::with_capacity(64);

        // Magic cookie
        options.extend_from_slice(&DHCP_MAGIC_COOKIE.to_be_bytes());

        // Message type = DISCOVER
        options.push(option::MESSAGE_TYPE);
        options.push(1);
        options.push(msg_type::DISCOVER);

        // Client identifier (MAC address)
        options.push(option::CLIENT_ID);
        options.push(7); // 1 byte type + 6 bytes MAC
        options.push(htype::ETHERNET);
        options.extend_from_slice(&mac.0);

        // Parameter request list
        options.push(option::PARAM_LIST);
        options.push(8);
        options.push(option::SUBNET_MASK);
        options.push(option::ROUTER);
        options.push(option::DNS_SERVER);
        options.push(option::DOMAIN_NAME);
        options.push(option::BROADCAST);
        options.push(option::LEASE_TIME);
        options.push(option::RENEWAL_TIME);
        options.push(option::REBINDING_TIME);

        // Maximum message size
        options.push(option::MAX_SIZE);
        options.push(2);
        options.push((DHCP_MAX_MESSAGE_SIZE >> 8) as u8);
        options.push((DHCP_MAX_MESSAGE_SIZE & 0xFF) as u8);

        // End
        options.push(option::END);

        // Pad to minimum size
        while options.len() < 64 {
            options.push(option::PAD);
        }

        Self { header, options }
    }

    /// Create new DHCP request message
    pub fn request(xid: u32, mac: &MacAddress, requested_ip: Ipv4Address, server_ip: Ipv4Address) -> Self {
        let header = DhcpHeader::new_request(xid, mac);
        let mut options = Vec::with_capacity(64);

        // Magic cookie
        options.extend_from_slice(&DHCP_MAGIC_COOKIE.to_be_bytes());

        // Message type = REQUEST
        options.push(option::MESSAGE_TYPE);
        options.push(1);
        options.push(msg_type::REQUEST);

        // Client identifier
        options.push(option::CLIENT_ID);
        options.push(7);
        options.push(htype::ETHERNET);
        options.extend_from_slice(&mac.0);

        // Requested IP address
        options.push(option::REQUESTED_IP);
        options.push(4);
        options.extend_from_slice(&requested_ip.to_bytes());

        // Server identifier
        options.push(option::SERVER_ID);
        options.push(4);
        options.extend_from_slice(&server_ip.to_bytes());

        // Parameter request list
        options.push(option::PARAM_LIST);
        options.push(8);
        options.push(option::SUBNET_MASK);
        options.push(option::ROUTER);
        options.push(option::DNS_SERVER);
        options.push(option::DOMAIN_NAME);
        options.push(option::BROADCAST);
        options.push(option::LEASE_TIME);
        options.push(option::RENEWAL_TIME);
        options.push(option::REBINDING_TIME);

        // End
        options.push(option::END);

        // Pad to minimum size
        while options.len() < 64 {
            options.push(option::PAD);
        }

        Self { header, options }
    }

    /// Create DHCP release message
    pub fn release(xid: u32, mac: &MacAddress, client_ip: Ipv4Address, server_ip: Ipv4Address) -> Self {
        let mut header = DhcpHeader::new_request(xid, mac);
        header.ciaddr = client_ip.to_bytes();
        header.flags = 0; // No broadcast for release

        let mut options = Vec::with_capacity(32);

        // Magic cookie
        options.extend_from_slice(&DHCP_MAGIC_COOKIE.to_be_bytes());

        // Message type = RELEASE
        options.push(option::MESSAGE_TYPE);
        options.push(1);
        options.push(msg_type::RELEASE);

        // Server identifier
        options.push(option::SERVER_ID);
        options.push(4);
        options.extend_from_slice(&server_ip.to_bytes());

        // Client identifier
        options.push(option::CLIENT_ID);
        options.push(7);
        options.push(htype::ETHERNET);
        options.extend_from_slice(&mac.0);

        // End
        options.push(option::END);

        Self { header, options }
    }

    /// Create DHCP renew message (unicast to server)
    pub fn renew(xid: u32, mac: &MacAddress, client_ip: Ipv4Address) -> Self {
        let mut header = DhcpHeader::new_request(xid, mac);
        header.ciaddr = client_ip.to_bytes();
        header.flags = 0; // No broadcast for renew

        let mut options = Vec::with_capacity(64);

        // Magic cookie
        options.extend_from_slice(&DHCP_MAGIC_COOKIE.to_be_bytes());

        // Message type = REQUEST (renew is a REQUEST with ciaddr set)
        options.push(option::MESSAGE_TYPE);
        options.push(1);
        options.push(msg_type::REQUEST);

        // Client identifier
        options.push(option::CLIENT_ID);
        options.push(7);
        options.push(htype::ETHERNET);
        options.extend_from_slice(&mac.0);

        // Parameter request list
        options.push(option::PARAM_LIST);
        options.push(8);
        options.push(option::SUBNET_MASK);
        options.push(option::ROUTER);
        options.push(option::DNS_SERVER);
        options.push(option::DOMAIN_NAME);
        options.push(option::BROADCAST);
        options.push(option::LEASE_TIME);
        options.push(option::RENEWAL_TIME);
        options.push(option::REBINDING_TIME);

        // End
        options.push(option::END);

        Self { header, options }
    }

    /// Serialize message to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let header_bytes = self.header.to_bytes();
        let mut result = Vec::with_capacity(header_bytes.len() + self.options.len());
        result.extend_from_slice(&header_bytes);
        result.extend_from_slice(&self.options);
        result
    }

    /// Parse DHCP message from bytes
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 240 { // 236 header + 4 magic cookie minimum
            return None;
        }

        // Parse header
        let header: DhcpHeader = unsafe {
            core::ptr::read(data.as_ptr() as *const DhcpHeader)
        };

        // Verify it's a reply
        if header.op != op::BOOTREPLY {
            return None;
        }

        // Verify magic cookie
        let cookie_bytes = &data[236..240];
        let cookie = u32::from_be_bytes([
            cookie_bytes[0],
            cookie_bytes[1],
            cookie_bytes[2],
            cookie_bytes[3],
        ]);

        if cookie != DHCP_MAGIC_COOKIE {
            return None;
        }

        // Options start after cookie
        let options = data[240..].to_vec();

        Some(Self { header, options })
    }

    /// Get DHCP message type from options
    pub fn message_type(&self) -> Option<u8> {
        self.get_option_u8(option::MESSAGE_TYPE)
    }

    /// Get server identifier from options
    pub fn server_id(&self) -> Option<Ipv4Address> {
        self.get_option_ip(option::SERVER_ID)
    }

    /// Get subnet mask from options
    pub fn subnet_mask(&self) -> Option<Ipv4Address> {
        self.get_option_ip(option::SUBNET_MASK)
    }

    /// Get router/gateway from options
    pub fn router(&self) -> Option<Ipv4Address> {
        self.get_option_ip(option::ROUTER)
    }

    /// Get broadcast address from options
    pub fn broadcast(&self) -> Option<Ipv4Address> {
        self.get_option_ip(option::BROADCAST)
    }

    /// Get DNS servers from options
    pub fn dns_servers(&self) -> Vec<Ipv4Address> {
        let mut servers = Vec::new();
        if let Some(data) = self.get_option_data(option::DNS_SERVER) {
            let mut i = 0;
            while i + 4 <= data.len() {
                if let Some(ip) = Ipv4Address::from_bytes(&data[i..i+4]) {
                    servers.push(ip);
                }
                i += 4;
            }
        }
        servers
    }

    /// Get domain name from options
    pub fn domain_name(&self) -> Option<String> {
        self.get_option_data(option::DOMAIN_NAME)
            .and_then(|data| {
                core::str::from_utf8(&data)
                    .ok()
                    .map(|s| s.trim_end_matches('\0').to_string())
            })
    }

    /// Get lease time from options
    pub fn lease_time(&self) -> Option<u32> {
        self.get_option_u32(option::LEASE_TIME)
    }

    /// Get renewal time (T1) from options
    pub fn renewal_time(&self) -> Option<u32> {
        self.get_option_u32(option::RENEWAL_TIME)
    }

    /// Get rebinding time (T2) from options
    pub fn rebinding_time(&self) -> Option<u32> {
        self.get_option_u32(option::REBINDING_TIME)
    }

    /// Get a single byte option
    fn get_option_u8(&self, opt_code: u8) -> Option<u8> {
        self.get_option_data(opt_code)
            .and_then(|data| data.first().copied())
    }

    /// Get a 32-bit option
    fn get_option_u32(&self, opt_code: u8) -> Option<u32> {
        self.get_option_data(opt_code)
            .filter(|data| data.len() >= 4)
            .map(|data| u32::from_be_bytes([data[0], data[1], data[2], data[3]]))
    }

    /// Get an IP address option
    fn get_option_ip(&self, opt_code: u8) -> Option<Ipv4Address> {
        self.get_option_data(opt_code)
            .filter(|data| data.len() >= 4)
            .and_then(|data| Ipv4Address::from_bytes(&data[..4]))
    }

    /// Get raw option data
    fn get_option_data(&self, opt_code: u8) -> Option<Vec<u8>> {
        let mut i = 0;
        while i < self.options.len() {
            let code = self.options[i];

            if code == option::END {
                break;
            }

            if code == option::PAD {
                i += 1;
                continue;
            }

            if i + 1 >= self.options.len() {
                break;
            }

            let len = self.options[i + 1] as usize;

            if i + 2 + len > self.options.len() {
                break;
            }

            if code == opt_code {
                return Some(self.options[i + 2..i + 2 + len].to_vec());
            }

            i += 2 + len;
        }

        None
    }
}

// =============================================================================
// DHCP CLIENT
// =============================================================================

/// DHCP client for automatic network configuration
pub struct DhcpClient {
    /// Network interface
    interface: Arc<NetworkInterface>,
    /// Current state
    state: AtomicU8,
    /// Transaction ID
    xid: AtomicU32,
    /// Current lease
    lease: RwLock<Option<DhcpLease>>,
    /// Pending offer (during SELECTING state)
    pending_offer: Mutex<Option<DhcpLease>>,
    /// Retry count
    retries: AtomicU32,
    /// Last send time (kernel ticks)
    last_send: AtomicU32,
}

impl DhcpClient {
    /// Create new DHCP client
    pub fn new(interface: Arc<NetworkInterface>) -> Self {
        // Generate random-ish XID from MAC address
        let xid = {
            let config = interface.config.read();
            let mac = &config.mac.0;
            u32::from_be_bytes([mac[2], mac[3], mac[4], mac[5]])
                ^ get_tick_count()
        };

        Self {
            interface,
            state: AtomicU8::new(DhcpState::Init as u8),
            xid: AtomicU32::new(xid),
            lease: RwLock::new(None),
            pending_offer: Mutex::new(None),
            retries: AtomicU32::new(0),
            last_send: AtomicU32::new(0),
        }
    }

    /// Get current state
    pub fn state(&self) -> DhcpState {
        match self.state.load(Ordering::Relaxed) {
            0 => DhcpState::Init,
            1 => DhcpState::Selecting,
            2 => DhcpState::Requesting,
            3 => DhcpState::Bound,
            4 => DhcpState::Renewing,
            5 => DhcpState::Rebinding,
            6 => DhcpState::Rebooting,
            _ => DhcpState::Init,
        }
    }

    /// Get current lease (if any)
    pub fn lease(&self) -> Option<DhcpLease> {
        self.lease.read().clone()
    }

    /// Start DHCP discovery
    pub fn discover(&self) -> Result<(), NetworkError> {
        let config = self.interface.config.read();
        let mac = config.mac;
        drop(config);

        // Create DISCOVER message
        let xid = self.xid.fetch_add(1, Ordering::SeqCst);
        let message = DhcpMessage::discover(xid, &mac);

        // Send via UDP broadcast
        self.send_broadcast(&message)?;

        self.state.store(DhcpState::Selecting as u8, Ordering::SeqCst);
        self.retries.store(0, Ordering::Relaxed);
        self.last_send.store(get_tick_count(), Ordering::Relaxed);

        Ok(())
    }

    /// Send DHCP request for offered IP
    pub fn request(&self, offered_ip: Ipv4Address, server_ip: Ipv4Address) -> Result<(), NetworkError> {
        let config = self.interface.config.read();
        let mac = config.mac;
        drop(config);

        let xid = self.xid.load(Ordering::Relaxed);
        let message = DhcpMessage::request(xid, &mac, offered_ip, server_ip);

        self.send_broadcast(&message)?;

        self.state.store(DhcpState::Requesting as u8, Ordering::SeqCst);
        self.last_send.store(get_tick_count(), Ordering::Relaxed);

        Ok(())
    }

    /// Release current lease
    pub fn release(&self) -> Result<(), NetworkError> {
        let lease = self.lease.read().clone();
        if let Some(lease) = lease {
            let config = self.interface.config.read();
            let mac = config.mac;
            drop(config);

            let xid = self.xid.fetch_add(1, Ordering::SeqCst);
            let message = DhcpMessage::release(xid, &mac, lease.client_ip, lease.server_ip);

            // Send release directly to server
            self.send_to_server(&message, lease.server_ip)?;

            // Clear lease
            *self.lease.write() = None;
            self.state.store(DhcpState::Init as u8, Ordering::SeqCst);
        }

        Ok(())
    }

    /// Renew current lease
    pub fn renew(&self) -> Result<(), NetworkError> {
        let lease = self.lease.read().clone();
        if let Some(lease) = lease {
            let config = self.interface.config.read();
            let mac = config.mac;
            drop(config);

            let xid = self.xid.fetch_add(1, Ordering::SeqCst);
            let message = DhcpMessage::renew(xid, &mac, lease.client_ip);

            // Send renewal directly to server (unicast)
            self.send_to_server(&message, lease.server_ip)?;

            self.state.store(DhcpState::Renewing as u8, Ordering::SeqCst);
            self.last_send.store(get_tick_count(), Ordering::Relaxed);
        }

        Ok(())
    }

    /// Handle received DHCP message
    pub fn handle_message(&self, data: &[u8]) {
        let message = match DhcpMessage::parse(data) {
            Some(m) => m,
            None => return,
        };

        // Verify XID matches
        let expected_xid = self.xid.load(Ordering::Relaxed);
        if message.header.xid != expected_xid {
            return;
        }

        // Verify MAC matches
        let config = self.interface.config.read();
        let our_mac = config.mac;
        drop(config);

        if message.header.client_mac() != our_mac {
            return;
        }

        let msg_type = match message.message_type() {
            Some(t) => t,
            None => return,
        };

        match (self.state(), msg_type) {
            (DhcpState::Selecting, msg_type::OFFER) => {
                self.handle_offer(&message);
            }
            (DhcpState::Requesting, msg_type::ACK) |
            (DhcpState::Renewing, msg_type::ACK) |
            (DhcpState::Rebinding, msg_type::ACK) => {
                self.handle_ack(&message);
            }
            (DhcpState::Requesting, msg_type::NAK) |
            (DhcpState::Renewing, msg_type::NAK) |
            (DhcpState::Rebinding, msg_type::NAK) => {
                self.handle_nak();
            }
            _ => {}
        }
    }

    /// Handle DHCPOFFER
    fn handle_offer(&self, message: &DhcpMessage) {
        let offered_ip = message.header.your_ip();
        let server_ip = message.server_id().unwrap_or(message.header.server_ip());

        if offered_ip.is_zero() {
            return;
        }

        // Build pending lease
        let lease = DhcpLease {
            client_ip: offered_ip,
            server_ip,
            subnet_mask: message.subnet_mask().unwrap_or(Ipv4Address::new(255, 255, 255, 0)),
            gateway: message.router().unwrap_or(Ipv4Address::ZERO),
            broadcast: message.broadcast().unwrap_or(Ipv4Address::BROADCAST),
            dns_servers: message.dns_servers(),
            domain: message.domain_name().unwrap_or_default(),
            lease_time: message.lease_time().unwrap_or(86400),
            renewal_time: message.renewal_time().unwrap_or(0),
            rebinding_time: message.rebinding_time().unwrap_or(0),
            obtained_at: 0, // Will be set on ACK
        };

        // Store pending offer
        *self.pending_offer.lock() = Some(lease.clone());

        // Send REQUEST for this offer
        let _ = self.request(offered_ip, server_ip);
    }

    /// Handle DHCPACK
    fn handle_ack(&self, message: &DhcpMessage) {
        let offered_ip = message.header.your_ip();
        let server_ip = message.server_id().unwrap_or(message.header.server_ip());

        // Calculate default T1/T2 if not provided
        let lease_time = message.lease_time().unwrap_or(86400);
        let renewal_time = message.renewal_time().unwrap_or(lease_time / 2);
        let rebinding_time = message.rebinding_time().unwrap_or(lease_time * 7 / 8);

        // Build final lease
        let lease = DhcpLease {
            client_ip: offered_ip,
            server_ip,
            subnet_mask: message.subnet_mask().unwrap_or(Ipv4Address::new(255, 255, 255, 0)),
            gateway: message.router().unwrap_or(Ipv4Address::ZERO),
            broadcast: message.broadcast().unwrap_or(Ipv4Address::BROADCAST),
            dns_servers: message.dns_servers(),
            domain: message.domain_name().unwrap_or_default(),
            lease_time,
            renewal_time,
            rebinding_time,
            obtained_at: get_tick_count() as u64,
        };

        // Apply configuration to interface
        self.apply_lease(&lease);

        // Store lease
        *self.lease.write() = Some(lease);
        *self.pending_offer.lock() = None;

        self.state.store(DhcpState::Bound as u8, Ordering::SeqCst);
        self.retries.store(0, Ordering::Relaxed);
    }

    /// Handle DHCPNAK
    fn handle_nak(&self) {
        // Clear pending offer and lease
        *self.pending_offer.lock() = None;
        *self.lease.write() = None;

        // Go back to INIT and restart
        self.state.store(DhcpState::Init as u8, Ordering::SeqCst);

        // Restart discovery
        let _ = self.discover();
    }

    /// Apply lease configuration to interface
    fn apply_lease(&self, lease: &DhcpLease) {
        let mut config = self.interface.config.write();
        config.ipv4 = lease.client_ip;
        config.netmask = lease.subnet_mask;
        config.gateway = lease.gateway;
        config.is_up = true;
    }

    /// Timer tick (called periodically)
    pub fn timer_tick(&self) {
        let now = get_tick_count();
        let state = self.state();

        match state {
            DhcpState::Init => {
                // Auto-start discovery
                let _ = self.discover();
            }
            DhcpState::Selecting | DhcpState::Requesting => {
                // Check for timeout (4 seconds)
                let last = self.last_send.load(Ordering::Relaxed);
                if now.wrapping_sub(last) > 4000 {
                    let retries = self.retries.fetch_add(1, Ordering::Relaxed);
                    if retries < 4 {
                        // Retry
                        if state == DhcpState::Selecting {
                            let _ = self.discover();
                        } else {
                            // Retry request
                            if let Some(offer) = self.pending_offer.lock().clone() {
                                let _ = self.request(offer.client_ip, offer.server_ip);
                            }
                        }
                    } else {
                        // Give up, restart
                        self.state.store(DhcpState::Init as u8, Ordering::SeqCst);
                        self.retries.store(0, Ordering::Relaxed);
                    }
                }
            }
            DhcpState::Bound => {
                // Check for renewal time
                if let Some(lease) = self.lease.read().clone() {
                    if lease.needs_rebinding(now as u64) {
                        // Start rebinding (broadcast)
                        self.state.store(DhcpState::Rebinding as u8, Ordering::SeqCst);
                        let _ = self.discover(); // Broadcast request
                    } else if lease.needs_renewal(now as u64) {
                        // Start renewal (unicast)
                        let _ = self.renew();
                    }
                }
            }
            DhcpState::Renewing => {
                // Check for timeout
                let last = self.last_send.load(Ordering::Relaxed);
                if now.wrapping_sub(last) > 10000 {
                    // Switch to rebinding
                    self.state.store(DhcpState::Rebinding as u8, Ordering::SeqCst);
                }
            }
            DhcpState::Rebinding => {
                // Check for lease expiry
                if let Some(lease) = self.lease.read().clone() {
                    if !lease.is_valid(now as u64) {
                        // Lease expired, restart
                        *self.lease.write() = None;
                        self.state.store(DhcpState::Init as u8, Ordering::SeqCst);
                    }
                }
            }
            _ => {}
        }
    }

    /// Send DHCP message via broadcast
    fn send_broadcast(&self, message: &DhcpMessage) -> Result<(), NetworkError> {
        let data = message.to_bytes();

        // Build UDP packet (we need to bypass normal socket for broadcast from 0.0.0.0)
        // This is a simplified direct send - in practice, we'd use the UDP layer
        send_dhcp_packet(&self.interface, &data, Ipv4Address::BROADCAST)?;

        Ok(())
    }

    /// Send DHCP message to specific server
    fn send_to_server(&self, message: &DhcpMessage, server: Ipv4Address) -> Result<(), NetworkError> {
        let data = message.to_bytes();
        send_dhcp_packet(&self.interface, &data, server)?;
        Ok(())
    }
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Get current tick count (milliseconds)
fn get_tick_count() -> u32 {
    // Use timer subsystem
    crate::drivers::timer::uptime_ms() as u32
}

/// Send DHCP packet (bypasses normal socket for 0.0.0.0 source)
fn send_dhcp_packet(
    iface: &NetworkInterface,
    data: &[u8],
    dest_ip: Ipv4Address,
) -> Result<(), NetworkError> {
    use super::{EthernetFrame, EtherType, Ipv4Packet, IpProtocol};
    use super::udp::UdpPacket;

    let config = iface.config.read();
    let src_mac = config.mac;
    let src_ip = if config.ipv4.is_zero() {
        Ipv4Address::ZERO
    } else {
        config.ipv4
    };
    drop(config);

    // Build UDP packet
    let udp = UdpPacket::new(
        DHCP_CLIENT_PORT,
        DHCP_SERVER_PORT,
        data,
    );
    let udp_data = udp.serialize(src_ip, dest_ip);

    // Build IP packet
    let ip = Ipv4Packet::new(
        src_ip,
        dest_ip,
        IpProtocol::Udp,
        &udp_data,
    );
    let ip_data = ip.serialize();

    // Build Ethernet frame
    let dest_mac = if dest_ip == Ipv4Address::BROADCAST {
        MacAddress::BROADCAST
    } else {
        // Would need ARP lookup for unicast
        MacAddress::BROADCAST // Fallback
    };

    let frame = EthernetFrame::new(
        dest_mac,
        src_mac,
        EtherType::Ipv4,
        &ip_data,
    );
    let frame_data = frame.serialize();

    iface.transmit(frame_data)
}

// =============================================================================
// GLOBAL DHCP CLIENT
// =============================================================================

/// Global DHCP client instance
static DHCP_CLIENT: RwLock<Option<Arc<DhcpClient>>> = RwLock::new(None);

/// Initialize DHCP client for interface
pub fn init(interface: Arc<NetworkInterface>) {
    let client = Arc::new(DhcpClient::new(interface));
    *DHCP_CLIENT.write() = Some(client);
}

/// Get DHCP client
pub fn get_client() -> Option<Arc<DhcpClient>> {
    DHCP_CLIENT.read().clone()
}

/// Start DHCP discovery
pub fn start() -> Result<(), NetworkError> {
    if let Some(client) = get_client() {
        client.discover()
    } else {
        Err(NetworkError::NotSupported)
    }
}

/// Timer tick (call periodically)
pub fn timer_tick() {
    if let Some(client) = get_client() {
        client.timer_tick();
    }
}

/// Handle incoming DHCP packet
pub fn handle_packet(data: &[u8]) {
    if let Some(client) = get_client() {
        client.handle_message(data);
    }
}

/// Get current lease
pub fn current_lease() -> Option<DhcpLease> {
    get_client().and_then(|c| c.lease())
}

/// Get DHCP state
pub fn current_state() -> DhcpState {
    get_client().map(|c| c.state()).unwrap_or(DhcpState::Init)
}
