// ===============================================================================
// BUILDOS KERNEL - TCP (TRANSMISSION CONTROL PROTOCOL)
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! TCP protocol implementation (RFC 793).

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use super::checksum;
use super::ip::{Ipv4Address, Ipv4Packet, IpProtocol, PseudoHeader};
use super::{NetworkError, NetworkInterface, NetworkStack};
use crate::drivers::timer;

// =============================================================================
// CONSTANTS
// =============================================================================

/// TCP header minimum size (without options)
pub const TCP_HEADER_MIN_SIZE: usize = 20;

/// TCP header maximum size (with options)
pub const TCP_HEADER_MAX_SIZE: usize = 60;

/// Default window size
pub const TCP_DEFAULT_WINDOW: u16 = 65535;

/// Maximum segment size (default)
pub const TCP_DEFAULT_MSS: u16 = 1460;

/// Initial sequence number generator
static ISN_COUNTER: AtomicU32 = AtomicU32::new(0x12345678);

/// Retransmission timeout (milliseconds)
pub const TCP_RTO_INIT: u64 = 1000;

/// Maximum retransmission attempts
pub const TCP_MAX_RETRIES: u32 = 5;

/// Time-wait timeout (milliseconds)
pub const TCP_TIME_WAIT: u64 = 60_000; // 60 seconds

// =============================================================================
// TCP FLAGS
// =============================================================================

/// TCP flags
#[derive(Debug, Clone, Copy, Default)]
pub struct TcpFlags {
    /// FIN - No more data from sender
    pub fin: bool,
    /// SYN - Synchronize sequence numbers
    pub syn: bool,
    /// RST - Reset the connection
    pub rst: bool,
    /// PSH - Push function
    pub psh: bool,
    /// ACK - Acknowledgment field significant
    pub ack: bool,
    /// URG - Urgent pointer field significant
    pub urg: bool,
    /// ECE - ECN-Echo
    pub ece: bool,
    /// CWR - Congestion Window Reduced
    pub cwr: bool,
}

impl TcpFlags {
    pub fn from_u8(value: u8) -> Self {
        Self {
            fin: (value & 0x01) != 0,
            syn: (value & 0x02) != 0,
            rst: (value & 0x04) != 0,
            psh: (value & 0x08) != 0,
            ack: (value & 0x10) != 0,
            urg: (value & 0x20) != 0,
            ece: (value & 0x40) != 0,
            cwr: (value & 0x80) != 0,
        }
    }

    pub fn to_u8(&self) -> u8 {
        let mut flags = 0u8;
        if self.fin {
            flags |= 0x01;
        }
        if self.syn {
            flags |= 0x02;
        }
        if self.rst {
            flags |= 0x04;
        }
        if self.psh {
            flags |= 0x08;
        }
        if self.ack {
            flags |= 0x10;
        }
        if self.urg {
            flags |= 0x20;
        }
        if self.ece {
            flags |= 0x40;
        }
        if self.cwr {
            flags |= 0x80;
        }
        flags
    }

    /// Create SYN flag
    pub fn syn() -> Self {
        Self {
            syn: true,
            ..Default::default()
        }
    }

    /// Create SYN-ACK flags
    pub fn syn_ack() -> Self {
        Self {
            syn: true,
            ack: true,
            ..Default::default()
        }
    }

    /// Create ACK flag
    pub fn ack() -> Self {
        Self {
            ack: true,
            ..Default::default()
        }
    }

    /// Create FIN-ACK flags
    pub fn fin_ack() -> Self {
        Self {
            fin: true,
            ack: true,
            ..Default::default()
        }
    }

    /// Create RST flag
    pub fn rst() -> Self {
        Self {
            rst: true,
            ..Default::default()
        }
    }

    /// Create PSH-ACK flags
    pub fn psh_ack() -> Self {
        Self {
            psh: true,
            ack: true,
            ..Default::default()
        }
    }
}

// =============================================================================
// TCP OPTIONS
// =============================================================================

/// TCP option kinds
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TcpOptionKind {
    /// End of options list
    End = 0,
    /// No operation (padding)
    Nop = 1,
    /// Maximum segment size
    Mss = 2,
    /// Window scale
    WindowScale = 3,
    /// SACK permitted
    SackPermitted = 4,
    /// SACK
    Sack = 5,
    /// Timestamp
    Timestamp = 8,
}

/// TCP option
#[derive(Debug, Clone)]
pub enum TcpOption {
    /// Maximum segment size
    Mss(u16),
    /// Window scale
    WindowScale(u8),
    /// SACK permitted
    SackPermitted,
    /// Timestamp (value, echo reply)
    Timestamp(u32, u32),
    /// Unknown option
    Unknown(u8, Vec<u8>),
}

impl TcpOption {
    /// Parse options from bytes
    pub fn parse_all(data: &[u8]) -> Vec<Self> {
        let mut options = Vec::new();
        let mut i = 0;

        while i < data.len() {
            match data[i] {
                0 => break, // End of options
                1 => {
                    i += 1;
                } // NOP
                2 if i + 4 <= data.len() && data[i + 1] == 4 => {
                    let mss = u16::from_be_bytes([data[i + 2], data[i + 3]]);
                    options.push(TcpOption::Mss(mss));
                    i += 4;
                }
                3 if i + 3 <= data.len() && data[i + 1] == 3 => {
                    options.push(TcpOption::WindowScale(data[i + 2]));
                    i += 3;
                }
                4 if i + 2 <= data.len() && data[i + 1] == 2 => {
                    options.push(TcpOption::SackPermitted);
                    i += 2;
                }
                8 if i + 10 <= data.len() && data[i + 1] == 10 => {
                    let ts_val = u32::from_be_bytes([data[i + 2], data[i + 3], data[i + 4], data[i + 5]]);
                    let ts_ecr = u32::from_be_bytes([data[i + 6], data[i + 7], data[i + 8], data[i + 9]]);
                    options.push(TcpOption::Timestamp(ts_val, ts_ecr));
                    i += 10;
                }
                kind => {
                    if i + 1 >= data.len() {
                        break;
                    }
                    let len = data[i + 1] as usize;
                    if len < 2 || i + len > data.len() {
                        break;
                    }
                    let opt_data = data[i + 2..i + len].to_vec();
                    options.push(TcpOption::Unknown(kind, opt_data));
                    i += len;
                }
            }
        }

        options
    }

    /// Serialize options to bytes
    pub fn serialize_all(options: &[TcpOption]) -> Vec<u8> {
        let mut bytes = Vec::new();

        for opt in options {
            match opt {
                TcpOption::Mss(mss) => {
                    bytes.push(2);
                    bytes.push(4);
                    bytes.extend_from_slice(&mss.to_be_bytes());
                }
                TcpOption::WindowScale(scale) => {
                    bytes.push(3);
                    bytes.push(3);
                    bytes.push(*scale);
                }
                TcpOption::SackPermitted => {
                    bytes.push(4);
                    bytes.push(2);
                }
                TcpOption::Timestamp(val, ecr) => {
                    bytes.push(8);
                    bytes.push(10);
                    bytes.extend_from_slice(&val.to_be_bytes());
                    bytes.extend_from_slice(&ecr.to_be_bytes());
                }
                TcpOption::Unknown(kind, data) => {
                    bytes.push(*kind);
                    bytes.push((2 + data.len()) as u8);
                    bytes.extend_from_slice(data);
                }
            }
        }

        // Pad to 4-byte boundary
        while bytes.len() % 4 != 0 {
            bytes.push(0); // End or NOP
        }

        bytes
    }
}

// =============================================================================
// TCP PACKET
// =============================================================================

/// TCP segment
#[derive(Debug, Clone)]
pub struct TcpPacket<'a> {
    /// Source port
    pub src_port: u16,
    /// Destination port
    pub dest_port: u16,
    /// Sequence number
    pub seq: u32,
    /// Acknowledgment number
    pub ack: u32,
    /// Data offset (header length in 32-bit words)
    pub data_offset: u8,
    /// Flags
    pub flags: TcpFlags,
    /// Window size
    pub window: u16,
    /// Checksum
    pub checksum: u16,
    /// Urgent pointer
    pub urgent_ptr: u16,
    /// Options
    pub options: Vec<TcpOption>,
    /// Payload data
    pub data: &'a [u8],
}

impl<'a> TcpPacket<'a> {
    /// Parse a TCP packet from bytes
    pub fn parse(data: &'a [u8]) -> Option<Self> {
        if data.len() < TCP_HEADER_MIN_SIZE {
            return None;
        }

        let src_port = u16::from_be_bytes([data[0], data[1]]);
        let dest_port = u16::from_be_bytes([data[2], data[3]]);
        let seq = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let ack = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
        let data_offset = data[12] >> 4;
        let flags = TcpFlags::from_u8(data[13]);
        let window = u16::from_be_bytes([data[14], data[15]]);
        let checksum = u16::from_be_bytes([data[16], data[17]]);
        let urgent_ptr = u16::from_be_bytes([data[18], data[19]]);

        let header_len = (data_offset as usize) * 4;
        if header_len < TCP_HEADER_MIN_SIZE || data.len() < header_len {
            return None;
        }

        let options = if header_len > TCP_HEADER_MIN_SIZE {
            TcpOption::parse_all(&data[TCP_HEADER_MIN_SIZE..header_len])
        } else {
            Vec::new()
        };

        let payload = &data[header_len..];

        Some(Self {
            src_port,
            dest_port,
            seq,
            ack,
            data_offset,
            flags,
            window,
            checksum,
            urgent_ptr,
            options,
            data: payload,
        })
    }

    /// Create a new TCP packet
    pub fn new(
        src_port: u16,
        dest_port: u16,
        seq: u32,
        ack: u32,
        flags: TcpFlags,
        window: u16,
        data: &'a [u8],
    ) -> Self {
        Self {
            src_port,
            dest_port,
            seq,
            ack,
            data_offset: 5, // No options by default
            flags,
            window,
            checksum: 0,
            urgent_ptr: 0,
            options: Vec::new(),
            data,
        }
    }

    /// Create with options
    pub fn with_options(mut self, options: Vec<TcpOption>) -> Self {
        let opt_len = TcpOption::serialize_all(&options).len();
        self.data_offset = ((TCP_HEADER_MIN_SIZE + opt_len) / 4) as u8;
        self.options = options;
        self
    }

    /// Verify checksum
    pub fn verify_checksum(&self, src_ip: Ipv4Address, dest_ip: Ipv4Address, total_len: u16) -> bool {
        let pseudo = PseudoHeader::new(src_ip, dest_ip, IpProtocol::Tcp, total_len);
        let header_and_data = self.serialize_no_checksum();
        let computed = checksum::checksum_with_pseudo(&pseudo.to_bytes(), &header_and_data);
        computed == 0
    }

    /// Calculate checksum
    pub fn calculate_checksum(&self, src_ip: Ipv4Address, dest_ip: Ipv4Address) -> u16 {
        let header_and_data = self.serialize_no_checksum();
        let total_len = header_and_data.len() as u16;
        let pseudo = PseudoHeader::new(src_ip, dest_ip, IpProtocol::Tcp, total_len);
        checksum::checksum_with_pseudo(&pseudo.to_bytes(), &header_and_data)
    }

    /// Serialize without checksum
    fn serialize_no_checksum(&self) -> Vec<u8> {
        let opt_bytes = TcpOption::serialize_all(&self.options);
        let header_len = TCP_HEADER_MIN_SIZE + opt_bytes.len();
        let data_offset = (header_len / 4) as u8;

        let mut packet = Vec::with_capacity(header_len + self.data.len());
        packet.extend_from_slice(&self.src_port.to_be_bytes());
        packet.extend_from_slice(&self.dest_port.to_be_bytes());
        packet.extend_from_slice(&self.seq.to_be_bytes());
        packet.extend_from_slice(&self.ack.to_be_bytes());
        packet.push(data_offset << 4);
        packet.push(self.flags.to_u8());
        packet.extend_from_slice(&self.window.to_be_bytes());
        packet.push(0); // Checksum placeholder
        packet.push(0);
        packet.extend_from_slice(&self.urgent_ptr.to_be_bytes());
        packet.extend_from_slice(&opt_bytes);
        packet.extend_from_slice(self.data);

        packet
    }

    /// Serialize to bytes
    pub fn serialize(&self, src_ip: Ipv4Address, dest_ip: Ipv4Address) -> Vec<u8> {
        let mut packet = self.serialize_no_checksum();
        let checksum = self.calculate_checksum(src_ip, dest_ip);
        packet[16] = (checksum >> 8) as u8;
        packet[17] = checksum as u8;
        packet
    }
}

// =============================================================================
// TCP CONNECTION STATE
// =============================================================================

/// TCP connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpState {
    /// Closed (initial state)
    Closed,
    /// Listening for incoming connections
    Listen,
    /// SYN sent, waiting for SYN-ACK
    SynSent,
    /// SYN received, SYN-ACK sent
    SynReceived,
    /// Connection established
    Established,
    /// FIN sent, waiting for ACK
    FinWait1,
    /// FIN acknowledged, waiting for FIN from peer
    FinWait2,
    /// Received FIN, waiting for application to close
    CloseWait,
    /// Both sides sent FIN simultaneously
    Closing,
    /// Waiting for final ACK after sending FIN
    LastAck,
    /// Time-wait before fully closing
    TimeWait,
}

impl TcpState {
    /// Check if connection is open for data
    pub fn is_established(&self) -> bool {
        matches!(self, TcpState::Established | TcpState::CloseWait)
    }

    /// Check if connection can receive data
    pub fn can_receive(&self) -> bool {
        matches!(
            self,
            TcpState::Established | TcpState::FinWait1 | TcpState::FinWait2
        )
    }

    /// Check if connection can send data
    pub fn can_send(&self) -> bool {
        matches!(
            self,
            TcpState::Established | TcpState::CloseWait
        )
    }
}

// =============================================================================
// TCP CONNECTION
// =============================================================================

/// TCP connection 4-tuple
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TcpEndpoint {
    pub local_ip: Ipv4Address,
    pub local_port: u16,
    pub remote_ip: Ipv4Address,
    pub remote_port: u16,
}

/// Send sequence space
#[derive(Debug)]
pub struct SendSequence {
    /// Unacknowledged
    pub una: u32,
    /// Next to send
    pub nxt: u32,
    /// Send window
    pub wnd: u16,
    /// Send urgent pointer
    pub up: u16,
    /// Initial sequence number
    pub iss: u32,
}

/// Receive sequence space
#[derive(Debug)]
pub struct RecvSequence {
    /// Next expected
    pub nxt: u32,
    /// Receive window
    pub wnd: u16,
    /// Initial sequence number
    pub irs: u32,
}

/// Segment in retransmission queue
#[derive(Debug)]
pub struct TcpSegment {
    /// Sequence number
    pub seq: u32,
    /// Data
    pub data: Vec<u8>,
    /// Flags
    pub flags: TcpFlags,
    /// Timestamp when sent
    pub sent_at: u64,
    /// Retransmission count
    pub retries: u32,
}

/// TCP connection
pub struct TcpConnection {
    /// Connection endpoint
    pub endpoint: TcpEndpoint,
    /// Connection state
    state: Mutex<TcpState>,
    /// Send sequence space
    send: Mutex<SendSequence>,
    /// Receive sequence space
    recv: Mutex<RecvSequence>,
    /// Receive buffer
    rx_buffer: Mutex<VecDeque<u8>>,
    /// Send buffer
    tx_buffer: Mutex<VecDeque<u8>>,
    /// Retransmission queue
    retx_queue: Mutex<VecDeque<TcpSegment>>,
    /// Maximum segment size
    mss: u16,
    /// Window scale
    window_scale: u8,
    /// Retransmission timeout (ms)
    rto: Mutex<u64>,
    /// Last activity timestamp
    last_activity: Mutex<u64>,
    /// Parent socket (for accept queue)
    parent: Mutex<Option<Arc<TcpConnection>>>,
    /// Pending connections (for listening sockets)
    accept_queue: Mutex<VecDeque<Arc<TcpConnection>>>,
}

impl TcpConnection {
    /// Create a new connection (client-side connect)
    pub fn new_client(
        local_ip: Ipv4Address,
        local_port: u16,
        remote_ip: Ipv4Address,
        remote_port: u16,
    ) -> Self {
        let isn = generate_isn();

        Self {
            endpoint: TcpEndpoint {
                local_ip,
                local_port,
                remote_ip,
                remote_port,
            },
            state: Mutex::new(TcpState::Closed),
            send: Mutex::new(SendSequence {
                una: isn,
                nxt: isn,
                wnd: 0,
                up: 0,
                iss: isn,
            }),
            recv: Mutex::new(RecvSequence {
                nxt: 0,
                wnd: TCP_DEFAULT_WINDOW,
                irs: 0,
            }),
            rx_buffer: Mutex::new(VecDeque::with_capacity(65536)),
            tx_buffer: Mutex::new(VecDeque::with_capacity(65536)),
            retx_queue: Mutex::new(VecDeque::new()),
            mss: TCP_DEFAULT_MSS,
            window_scale: 0,
            rto: Mutex::new(TCP_RTO_INIT),
            last_activity: Mutex::new(timer::monotonic_ns() / 1_000_000),
            parent: Mutex::new(None),
            accept_queue: Mutex::new(VecDeque::new()),
        }
    }

    /// Create a new listening socket
    pub fn new_listen(local_ip: Ipv4Address, local_port: u16) -> Self {
        Self {
            endpoint: TcpEndpoint {
                local_ip,
                local_port,
                remote_ip: Ipv4Address::ZERO,
                remote_port: 0,
            },
            state: Mutex::new(TcpState::Listen),
            send: Mutex::new(SendSequence {
                una: 0,
                nxt: 0,
                wnd: 0,
                up: 0,
                iss: 0,
            }),
            recv: Mutex::new(RecvSequence {
                nxt: 0,
                wnd: TCP_DEFAULT_WINDOW,
                irs: 0,
            }),
            rx_buffer: Mutex::new(VecDeque::new()),
            tx_buffer: Mutex::new(VecDeque::new()),
            retx_queue: Mutex::new(VecDeque::new()),
            mss: TCP_DEFAULT_MSS,
            window_scale: 0,
            rto: Mutex::new(TCP_RTO_INIT),
            last_activity: Mutex::new(timer::monotonic_ns() / 1_000_000),
            parent: Mutex::new(None),
            accept_queue: Mutex::new(VecDeque::with_capacity(128)),
        }
    }

    /// Create from incoming SYN (server-side)
    pub fn from_syn(
        local_ip: Ipv4Address,
        local_port: u16,
        remote_ip: Ipv4Address,
        remote_port: u16,
        irs: u32,
    ) -> Self {
        let isn = generate_isn();

        Self {
            endpoint: TcpEndpoint {
                local_ip,
                local_port,
                remote_ip,
                remote_port,
            },
            state: Mutex::new(TcpState::SynReceived),
            send: Mutex::new(SendSequence {
                una: isn,
                nxt: isn.wrapping_add(1),
                wnd: 0,
                up: 0,
                iss: isn,
            }),
            recv: Mutex::new(RecvSequence {
                nxt: irs.wrapping_add(1),
                wnd: TCP_DEFAULT_WINDOW,
                irs,
            }),
            rx_buffer: Mutex::new(VecDeque::with_capacity(65536)),
            tx_buffer: Mutex::new(VecDeque::with_capacity(65536)),
            retx_queue: Mutex::new(VecDeque::new()),
            mss: TCP_DEFAULT_MSS,
            window_scale: 0,
            rto: Mutex::new(TCP_RTO_INIT),
            last_activity: Mutex::new(timer::monotonic_ns() / 1_000_000),
            parent: Mutex::new(None),
            accept_queue: Mutex::new(VecDeque::new()),
        }
    }

    /// Connect to remote host (convenience function)
    pub fn connect(
        stack: &NetworkStack,
        remote_ip: Ipv4Address,
        remote_port: u16,
    ) -> Result<Arc<Self>, NetworkError> {
        // Get first interface for local IP
        let iface = stack.primary_interface()
            .cloned()
            .ok_or(NetworkError::NoRoute)?;
        let local_ip = iface.config.read().ipv4;
        // Allocate ephemeral port
        let local_port = stack.allocate_port();

        let conn = Arc::new(Self::new_client(local_ip, local_port, remote_ip, remote_port));
        stack.tcp_connections.write().push(Arc::clone(&conn));

        // Send SYN
        send_syn(stack, &iface, &conn)?;

        Ok(conn)
    }

    /// Get current state
    pub fn state(&self) -> TcpState {
        *self.state.lock()
    }

    /// Set state
    pub fn set_state(&self, state: TcpState) {
        *self.state.lock() = state;
    }

    /// Read data from receive buffer
    pub fn read(&self, buf: &mut [u8]) -> usize {
        let mut rx = self.rx_buffer.lock();
        let len = core::cmp::min(buf.len(), rx.len());
        for (i, byte) in rx.drain(..len).enumerate() {
            buf[i] = byte;
        }
        len
    }

    /// Write data to send buffer
    pub fn write(&self, data: &[u8]) -> usize {
        if !self.state().can_send() {
            return 0;
        }

        let mut tx = self.tx_buffer.lock();
        let available = 65536 - tx.len();
        let len = core::cmp::min(data.len(), available);
        tx.extend(&data[..len]);
        len
    }

    /// Send data (Result-based API for compatibility)
    pub fn send(&self, data: &[u8]) -> Result<usize, NetworkError> {
        if !self.state().can_send() {
            return Err(NetworkError::NotConnected);
        }
        let written = self.write(data);
        if written == 0 && !data.is_empty() {
            Err(NetworkError::QueueFull)
        } else {
            Ok(written)
        }
    }

    /// Receive data (Result-based API for compatibility)
    pub fn recv(&self, buf: &mut [u8]) -> Result<usize, NetworkError> {
        let len = self.read(buf);
        Ok(len)
    }

    /// Get available data in receive buffer
    pub fn rx_available(&self) -> usize {
        self.rx_buffer.lock().len()
    }

    /// Check if receive buffer is empty
    pub fn rx_empty(&self) -> bool {
        self.rx_buffer.lock().is_empty()
    }

    /// Queue received data
    pub fn queue_rx(&self, data: &[u8]) {
        let mut rx = self.rx_buffer.lock();
        rx.extend(data);
    }

    /// Get data to send
    pub fn get_tx_data(&self, max_len: usize) -> Vec<u8> {
        let tx = self.tx_buffer.lock();
        let len = core::cmp::min(max_len, tx.len());
        tx.iter().take(len).copied().collect()
    }

    /// Acknowledge sent data
    pub fn ack_tx_data(&self, len: usize) {
        let mut tx = self.tx_buffer.lock();
        let drain_len = len.min(tx.len());
        tx.drain(..drain_len);
    }

    /// Accept a pending connection
    pub fn accept(&self) -> Option<Arc<TcpConnection>> {
        self.accept_queue.lock().pop_front()
    }

    /// Queue a connection for accept
    pub fn queue_accept(&self, conn: Arc<TcpConnection>) {
        self.accept_queue.lock().push_back(conn);
    }

    /// Update last activity
    pub fn touch(&self) {
        *self.last_activity.lock() = timer::monotonic_ns() / 1_000_000;
    }

    /// Get send sequence numbers
    pub fn get_send_seq(&self) -> (u32, u32) {
        let send = self.send.lock();
        (send.una, send.nxt)
    }

    /// Get next sequence number to send
    pub fn send_nxt(&self) -> u32 {
        self.send.lock().nxt
    }

    /// Get receive next
    pub fn recv_nxt(&self) -> u32 {
        self.recv.lock().nxt
    }

    /// Update send window
    pub fn update_send_window(&self, ack: u32, window: u16) {
        let mut send = self.send.lock();
        if seq_gte(ack, send.una) {
            send.una = ack;
        }
        send.wnd = window;
    }

    /// Advance send next
    pub fn advance_send_nxt(&self, len: u32) {
        let mut send = self.send.lock();
        send.nxt = send.nxt.wrapping_add(len);
    }

    /// Update receive next
    pub fn update_recv_nxt(&self, seq: u32) {
        let mut recv = self.recv.lock();
        recv.nxt = seq;
    }

    /// Get receive window
    pub fn recv_window(&self) -> u16 {
        let rx = self.rx_buffer.lock();
        (65536 - rx.len()) as u16
    }

    /// Get initial send sequence
    pub fn iss(&self) -> u32 {
        self.send.lock().iss
    }

    /// Check if there are pending connections waiting to be accepted
    pub fn has_pending_accept(&self) -> bool {
        !self.accept_queue.lock().is_empty()
    }

    /// Close the connection
    pub fn close(&self) -> Result<(), NetworkError> {
        let mut state = self.state.lock();
        match *state {
            TcpState::Established => {
                *state = TcpState::FinWait1;
                Ok(())
            }
            TcpState::CloseWait => {
                *state = TcpState::LastAck;
                Ok(())
            }
            TcpState::Listen | TcpState::SynSent | TcpState::SynReceived => {
                *state = TcpState::Closed;
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

// =============================================================================
// TCP SEQUENCE NUMBER HELPERS
// =============================================================================

/// Generate initial sequence number
fn generate_isn() -> u32 {
    // Simple ISN: combine counter with time
    let counter = ISN_COUNTER.fetch_add(1, Ordering::Relaxed);
    let time = (timer::monotonic_ns() / 1000) as u32;
    counter.wrapping_add(time)
}

/// Sequence number comparison: a < b (handling wraparound)
pub fn seq_lt(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) < 0
}

/// Sequence number comparison: a <= b
pub fn seq_lte(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) <= 0
}

/// Sequence number comparison: a > b
pub fn seq_gt(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) > 0
}

/// Sequence number comparison: a >= b
pub fn seq_gte(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) >= 0
}

// =============================================================================
// TCP PACKET HANDLING
// =============================================================================

/// Handle an incoming TCP packet
pub fn handle_tcp(
    stack: &NetworkStack,
    iface: &Arc<NetworkInterface>,
    ip: &Ipv4Packet,
    tcp: &TcpPacket,
) {
    // Find matching connection
    if let Some(conn) = find_connection(stack, ip.dest, tcp.dest_port, ip.src, tcp.src_port) {
        handle_segment(stack, iface, ip, tcp, &conn);
    } else if tcp.flags.syn && !tcp.flags.ack {
        // New connection attempt - look for listening socket
        if let Some(listener) = find_listener(stack, ip.dest, tcp.dest_port) {
            handle_syn(stack, iface, ip, tcp, &listener);
        } else {
            // No listener - send RST
            send_rst(stack, iface, ip, tcp);
        }
    } else if !tcp.flags.rst {
        // No connection and not SYN - send RST
        send_rst(stack, iface, ip, tcp);
    }
}

/// Find a connection by 4-tuple
fn find_connection(
    stack: &NetworkStack,
    local_ip: Ipv4Address,
    local_port: u16,
    remote_ip: Ipv4Address,
    remote_port: u16,
) -> Option<Arc<TcpConnection>> {
    let connections = stack.tcp_connections.read();
    for conn in connections.iter() {
        let ep = &conn.endpoint;
        if ep.local_port == local_port
            && ep.remote_port == remote_port
            && (ep.local_ip.is_unspecified() || ep.local_ip == local_ip)
            && ep.remote_ip == remote_ip
        {
            return Some(Arc::clone(conn));
        }
    }
    None
}

/// Find a listening socket
fn find_listener(
    stack: &NetworkStack,
    local_ip: Ipv4Address,
    local_port: u16,
) -> Option<Arc<TcpConnection>> {
    let connections = stack.tcp_connections.read();
    for conn in connections.iter() {
        let ep = &conn.endpoint;
        if ep.local_port == local_port
            && conn.state() == TcpState::Listen
            && (ep.local_ip.is_unspecified() || ep.local_ip == local_ip)
        {
            return Some(Arc::clone(conn));
        }
    }
    None
}

/// Handle a segment for an established connection
fn handle_segment(
    stack: &NetworkStack,
    iface: &Arc<NetworkInterface>,
    _ip: &Ipv4Packet,
    tcp: &TcpPacket,
    conn: &Arc<TcpConnection>,
) {
    conn.touch();
    let state = conn.state();

    // Handle RST
    if tcp.flags.rst {
        conn.set_state(TcpState::Closed);
        return;
    }

    match state {
        TcpState::SynSent => {
            if tcp.flags.syn && tcp.flags.ack {
                // SYN-ACK received
                conn.update_send_window(tcp.ack, tcp.window);
                conn.update_recv_nxt(tcp.seq.wrapping_add(1));
                conn.set_state(TcpState::Established);

                // Send ACK
                send_ack(stack, iface, conn);
            }
        }
        TcpState::SynReceived => {
            if tcp.flags.ack {
                // ACK for our SYN-ACK
                conn.update_send_window(tcp.ack, tcp.window);
                conn.set_state(TcpState::Established);

                // Notify parent listener
                if let Some(parent) = conn.parent.lock().as_ref() {
                    parent.queue_accept(Arc::clone(conn));
                }
            }
        }
        TcpState::Established => {
            // Handle data
            if !tcp.data.is_empty() {
                let recv_nxt = conn.recv_nxt();
                if tcp.seq == recv_nxt {
                    // In-order data
                    conn.queue_rx(tcp.data);
                    conn.update_recv_nxt(recv_nxt.wrapping_add(tcp.data.len() as u32));
                }
            }

            // Handle ACK
            if tcp.flags.ack {
                conn.update_send_window(tcp.ack, tcp.window);
            }

            // Handle FIN
            if tcp.flags.fin {
                conn.update_recv_nxt(conn.recv_nxt().wrapping_add(1));
                conn.set_state(TcpState::CloseWait);
                send_ack(stack, iface, conn);
            } else if !tcp.data.is_empty() {
                // Send ACK for data
                send_ack(stack, iface, conn);
            }
        }
        TcpState::FinWait1 => {
            if tcp.flags.ack {
                conn.update_send_window(tcp.ack, tcp.window);
                if tcp.flags.fin {
                    // FIN + ACK
                    conn.update_recv_nxt(conn.recv_nxt().wrapping_add(1));
                    conn.set_state(TcpState::TimeWait);
                    send_ack(stack, iface, conn);
                } else {
                    conn.set_state(TcpState::FinWait2);
                }
            } else if tcp.flags.fin {
                // Simultaneous close
                conn.update_recv_nxt(conn.recv_nxt().wrapping_add(1));
                conn.set_state(TcpState::Closing);
                send_ack(stack, iface, conn);
            }
        }
        TcpState::FinWait2 => {
            if tcp.flags.fin {
                conn.update_recv_nxt(conn.recv_nxt().wrapping_add(1));
                conn.set_state(TcpState::TimeWait);
                send_ack(stack, iface, conn);
            }
        }
        TcpState::Closing => {
            if tcp.flags.ack {
                conn.set_state(TcpState::TimeWait);
            }
        }
        TcpState::LastAck => {
            if tcp.flags.ack {
                conn.set_state(TcpState::Closed);
            }
        }
        TcpState::CloseWait => {
            // Waiting for application to close
            if tcp.flags.ack {
                conn.update_send_window(tcp.ack, tcp.window);
            }
        }
        _ => {}
    }
}

/// Handle incoming SYN (new connection)
fn handle_syn(
    stack: &NetworkStack,
    iface: &Arc<NetworkInterface>,
    ip: &Ipv4Packet,
    tcp: &TcpPacket,
    listener: &Arc<TcpConnection>,
) {
    // Create new connection
    let conn = Arc::new(TcpConnection::from_syn(
        ip.dest,
        tcp.dest_port,
        ip.src,
        tcp.src_port,
        tcp.seq,
    ));

    // Set parent
    *conn.parent.lock() = Some(Arc::clone(listener));

    // Parse MSS option
    for opt in &tcp.options {
        if let TcpOption::Mss(mss) = opt {
            // Would update conn.mss here
            let _ = mss;
        }
    }

    // Add to connection table
    stack.tcp_connections.write().push(Arc::clone(&conn));

    // Send SYN-ACK
    send_syn_ack(stack, iface, &conn);
}

/// Send a SYN packet
pub fn send_syn(
    stack: &NetworkStack,
    iface: &Arc<NetworkInterface>,
    conn: &Arc<TcpConnection>,
) -> Result<(), NetworkError> {
    let seq = conn.iss();
    let options = vec![
        TcpOption::Mss(TCP_DEFAULT_MSS),
        TcpOption::WindowScale(0),
        TcpOption::SackPermitted,
    ];

    let empty_data: &[u8] = &[];
    let packet = TcpPacket::new(
        conn.endpoint.local_port,
        conn.endpoint.remote_port,
        seq,
        0,
        TcpFlags::syn(),
        conn.recv_window(),
        empty_data,
    )
    .with_options(options);

    let bytes = packet.serialize(conn.endpoint.local_ip, conn.endpoint.remote_ip);
    conn.advance_send_nxt(1); // SYN consumes one sequence number
    conn.set_state(TcpState::SynSent);

    stack.send_ip(iface, conn.endpoint.remote_ip, IpProtocol::Tcp, &bytes)
}

/// Send a SYN-ACK packet
fn send_syn_ack(
    stack: &NetworkStack,
    iface: &Arc<NetworkInterface>,
    conn: &Arc<TcpConnection>,
) {
    let seq = conn.iss();
    let ack = conn.recv_nxt();
    let options = vec![TcpOption::Mss(TCP_DEFAULT_MSS)];

    let empty_data: &[u8] = &[];
    let packet = TcpPacket::new(
        conn.endpoint.local_port,
        conn.endpoint.remote_port,
        seq,
        ack,
        TcpFlags::syn_ack(),
        conn.recv_window(),
        empty_data,
    )
    .with_options(options);

    let bytes = packet.serialize(conn.endpoint.local_ip, conn.endpoint.remote_ip);

    let _ = stack.send_ip(iface, conn.endpoint.remote_ip, IpProtocol::Tcp, &bytes);
}

/// Send an ACK packet
fn send_ack(
    stack: &NetworkStack,
    iface: &Arc<NetworkInterface>,
    conn: &Arc<TcpConnection>,
) {
    let seq = conn.send_nxt();
    let ack = conn.recv_nxt();

    let empty_data: &[u8] = &[];
    let packet = TcpPacket::new(
        conn.endpoint.local_port,
        conn.endpoint.remote_port,
        seq,
        ack,
        TcpFlags::ack(),
        conn.recv_window(),
        empty_data,
    );

    let bytes = packet.serialize(conn.endpoint.local_ip, conn.endpoint.remote_ip);

    let _ = stack.send_ip(iface, conn.endpoint.remote_ip, IpProtocol::Tcp, &bytes);
}

/// Send a FIN packet
pub fn send_fin(
    stack: &NetworkStack,
    iface: &Arc<NetworkInterface>,
    conn: &Arc<TcpConnection>,
) -> Result<(), NetworkError> {
    let seq = conn.send_nxt();
    let ack = conn.recv_nxt();

    let empty_data: &[u8] = &[];
    let packet = TcpPacket::new(
        conn.endpoint.local_port,
        conn.endpoint.remote_port,
        seq,
        ack,
        TcpFlags::fin_ack(),
        conn.recv_window(),
        empty_data,
    );

    let bytes = packet.serialize(conn.endpoint.local_ip, conn.endpoint.remote_ip);
    conn.advance_send_nxt(1); // FIN consumes one sequence number

    let state = conn.state();
    match state {
        TcpState::Established => conn.set_state(TcpState::FinWait1),
        TcpState::CloseWait => conn.set_state(TcpState::LastAck),
        _ => {}
    }

    stack.send_ip(iface, conn.endpoint.remote_ip, IpProtocol::Tcp, &bytes)
}

/// Send data on a connection
pub fn send_data(
    stack: &NetworkStack,
    iface: &Arc<NetworkInterface>,
    conn: &Arc<TcpConnection>,
    data: &[u8],
) -> Result<usize, NetworkError> {
    if !conn.state().can_send() {
        return Err(NetworkError::NotConnected);
    }

    let mss = conn.mss as usize;
    let mut sent = 0;

    while sent < data.len() {
        let chunk_size = core::cmp::min(mss, data.len() - sent);
        let chunk = &data[sent..sent + chunk_size];

        let seq = conn.send_nxt();
        let ack = conn.recv_nxt();

        let packet = TcpPacket::new(
            conn.endpoint.local_port,
            conn.endpoint.remote_port,
            seq,
            ack,
            TcpFlags::psh_ack(),
            conn.recv_window(),
            chunk,
        );

        let bytes = packet.serialize(conn.endpoint.local_ip, conn.endpoint.remote_ip);
        conn.advance_send_nxt(chunk_size as u32);

        stack.send_ip(iface, conn.endpoint.remote_ip, IpProtocol::Tcp, &bytes)?;
        sent += chunk_size;
    }

    Ok(sent)
}

/// Send RST in response to unexpected packet
fn send_rst(
    stack: &NetworkStack,
    iface: &Arc<NetworkInterface>,
    ip: &Ipv4Packet,
    tcp: &TcpPacket,
) {
    let (seq, ack) = if tcp.flags.ack {
        (tcp.ack, 0)
    } else {
        (0, tcp.seq.wrapping_add(tcp.data.len() as u32).wrapping_add(
            if tcp.flags.syn { 1 } else { 0 } + if tcp.flags.fin { 1 } else { 0 },
        ))
    };

    let flags = if tcp.flags.ack {
        TcpFlags::rst()
    } else {
        let mut f = TcpFlags::rst();
        f.ack = true;
        f
    };

    let empty_data: &[u8] = &[];
    let packet = TcpPacket::new(tcp.dest_port, tcp.src_port, seq, ack, flags, 0, empty_data);

    let bytes = packet.serialize(ip.dest, ip.src);
    let _ = stack.send_ip(iface, ip.src, IpProtocol::Tcp, &bytes);
}

/// Connect to a remote host
pub fn connect(
    stack: &NetworkStack,
    iface: &Arc<NetworkInterface>,
    local_ip: Ipv4Address,
    local_port: u16,
    remote_ip: Ipv4Address,
    remote_port: u16,
) -> Result<Arc<TcpConnection>, NetworkError> {
    let conn = Arc::new(TcpConnection::new_client(
        local_ip,
        local_port,
        remote_ip,
        remote_port,
    ));

    // Add to connection table
    stack.tcp_connections.write().push(Arc::clone(&conn));

    // Send SYN
    send_syn(stack, iface, &conn)?;

    Ok(conn)
}

/// Listen for incoming connections
pub fn listen(
    stack: &NetworkStack,
    local_ip: Ipv4Address,
    local_port: u16,
) -> Arc<TcpConnection> {
    let conn = Arc::new(TcpConnection::new_listen(local_ip, local_port));

    stack.tcp_connections.write().push(Arc::clone(&conn));

    conn
}

/// Close a connection
pub fn close(
    stack: &NetworkStack,
    iface: &Arc<NetworkInterface>,
    conn: &Arc<TcpConnection>,
) -> Result<(), NetworkError> {
    let state = conn.state();

    match state {
        TcpState::Listen | TcpState::SynSent => {
            conn.set_state(TcpState::Closed);
            Ok(())
        }
        TcpState::Established | TcpState::CloseWait => {
            send_fin(stack, iface, conn)
        }
        TcpState::SynReceived => {
            send_fin(stack, iface, conn)
        }
        _ => Ok(()), // Already closing or closed
    }
}

/// Process TCP timers (called periodically from network poll)
pub fn timer_tick(stack: &NetworkStack) {
    let now_ms = timer::monotonic_ns() / 1_000_000;

    // Get list of connections to process
    let connections: Vec<Arc<TcpConnection>> = {
        stack.tcp_connections.read().iter().cloned().collect()
    };

    // Process each connection
    let mut to_remove = Vec::new();

    for conn in &connections {
        let state = conn.state();
        let last_activity = *conn.last_activity.lock();
        let age_ms = now_ms.saturating_sub(last_activity);

        match state {
            TcpState::TimeWait => {
                // TIME_WAIT timeout (2 * MSL = 60 seconds)
                if age_ms >= TCP_TIME_WAIT {
                    conn.set_state(TcpState::Closed);
                    to_remove.push(conn.endpoint.clone());
                }
            }
            TcpState::SynSent | TcpState::SynReceived => {
                // Connection establishment timeout
                let rto = *conn.rto.lock();
                if age_ms >= rto * TCP_MAX_RETRIES as u64 {
                    conn.set_state(TcpState::Closed);
                    to_remove.push(conn.endpoint.clone());
                }
            }
            TcpState::Closed => {
                // Clean up closed connections
                to_remove.push(conn.endpoint.clone());
            }
            _ => {}
        }
    }

    // Remove closed connections
    if !to_remove.is_empty() {
        let mut connections = stack.tcp_connections.write();
        connections.retain(|c| {
            !to_remove.iter().any(|ep| c.endpoint == *ep)
        });
    }
}
