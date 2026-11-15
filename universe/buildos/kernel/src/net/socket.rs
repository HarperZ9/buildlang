// ===============================================================================
// BUILDOS KERNEL - SOCKET API
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! BSD-style socket API implementation.

use alloc::sync::Arc;
use spin::Mutex;

use super::ip::Ipv4Address;
use super::tcp::{self, TcpConnection, TcpState};
use super::udp::{UdpDatagram, UdpSocket};
use super::{NetworkError, NetworkInterface, NetworkStack};

// =============================================================================
// SOCKET CONSTANTS
// =============================================================================

/// Socket address family
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum AddressFamily {
    /// Unspecified
    Unspec = 0,
    /// Local/Unix domain
    Unix = 1,
    /// IPv4
    Inet = 2,
    /// IPv6
    Inet6 = 10,
}

impl AddressFamily {
    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            0 => Some(AddressFamily::Unspec),
            1 => Some(AddressFamily::Unix),
            2 => Some(AddressFamily::Inet),
            10 => Some(AddressFamily::Inet6),
            _ => None,
        }
    }
}

/// Socket type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SocketType {
    /// Stream socket (TCP)
    Stream = 1,
    /// Datagram socket (UDP)
    Dgram = 2,
    /// Raw socket
    Raw = 3,
    /// Reliably-delivered message
    Rdm = 4,
    /// Sequenced packet stream
    SeqPacket = 5,
}

impl SocketType {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(SocketType::Stream),
            2 => Some(SocketType::Dgram),
            3 => Some(SocketType::Raw),
            4 => Some(SocketType::Rdm),
            5 => Some(SocketType::SeqPacket),
            _ => None,
        }
    }
}

/// Socket protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SocketProtocol {
    /// Default protocol
    Default = 0,
    /// ICMP
    Icmp = 1,
    /// TCP
    Tcp = 6,
    /// UDP
    Udp = 17,
}

impl SocketProtocol {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(SocketProtocol::Default),
            1 => Some(SocketProtocol::Icmp),
            6 => Some(SocketProtocol::Tcp),
            17 => Some(SocketProtocol::Udp),
            _ => None,
        }
    }
}

/// Socket options level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum SocketLevel {
    /// Socket level options
    Socket = 1,
    /// IP level options
    Ip = 0,
    /// TCP level options
    Tcp = 6,
    /// UDP level options
    Udp = 17,
}

/// Socket options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum SocketOption {
    /// Reuse address
    ReuseAddr = 2,
    /// Reuse port
    ReusePort = 15,
    /// Keep alive
    KeepAlive = 9,
    /// Broadcast
    Broadcast = 6,
    /// Receive buffer size
    RcvBuf = 8,
    /// Send buffer size
    SndBuf = 7,
    /// Receive timeout
    RcvTimeo = 20,
    /// Send timeout
    SndTimeo = 21,
    /// Linger on close
    Linger = 13,
    /// Out-of-band data inline
    OobInline = 10,
    /// Get error status
    Error = 4,
    /// Get socket type
    Type = 3,
    /// Non-blocking I/O
    NonBlock = 0x800,
}

/// Shutdown modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ShutdownMode {
    /// Shutdown read
    Read = 0,
    /// Shutdown write
    Write = 1,
    /// Shutdown both
    Both = 2,
}

// =============================================================================
// SOCKET ADDRESS
// =============================================================================

/// Socket address (IPv4)
#[derive(Debug, Clone, Copy, Default)]
pub struct SocketAddr {
    /// Address family
    pub family: u16,
    /// Port (network byte order)
    pub port: u16,
    /// IPv4 address
    pub addr: Ipv4Address,
}

impl SocketAddr {
    /// Create a new socket address
    pub fn new(addr: Ipv4Address, port: u16) -> Self {
        Self {
            family: AddressFamily::Inet as u16,
            port,
            addr,
        }
    }

    /// Create from raw sockaddr_in structure
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 8 {
            return None;
        }

        let family = u16::from_ne_bytes([data[0], data[1]]);
        let port = u16::from_be_bytes([data[2], data[3]]);
        let addr = Ipv4Address::from_bytes(&data[4..8])?;

        Some(Self { family, port, addr })
    }

    /// Convert to bytes
    pub fn to_bytes(&self) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes[0..2].copy_from_slice(&self.family.to_ne_bytes());
        bytes[2..4].copy_from_slice(&self.port.to_be_bytes());
        bytes[4..8].copy_from_slice(&self.addr.octets());
        bytes
    }
}

// =============================================================================
// SOCKET STATE
// =============================================================================

/// Socket internal state
enum SocketInner {
    /// Unconnected socket
    Unbound {
        socket_type: SocketType,
    },
    /// Bound UDP socket
    Udp(UdpSocket),
    /// TCP socket
    Tcp(Arc<TcpConnection>),
}

/// Socket structure
pub struct Socket {
    /// Socket type
    socket_type: SocketType,
    /// Internal state
    inner: Mutex<SocketInner>,
    /// Local address
    local_addr: Mutex<Option<SocketAddr>>,
    /// Remote address (for connected sockets)
    remote_addr: Mutex<Option<SocketAddr>>,
    /// Socket options
    options: Mutex<SocketOptions>,
    /// Non-blocking mode
    non_blocking: Mutex<bool>,
}

/// Socket options storage
#[derive(Debug, Clone, Default)]
struct SocketOptions {
    reuse_addr: bool,
    reuse_port: bool,
    keep_alive: bool,
    broadcast: bool,
    rcv_buf: usize,
    snd_buf: usize,
    rcv_timeo: u64,
    snd_timeo: u64,
}

impl Socket {
    /// Create a new socket
    pub fn new(socket_type: SocketType) -> Self {
        Self {
            socket_type,
            inner: Mutex::new(SocketInner::Unbound { socket_type }),
            local_addr: Mutex::new(None),
            remote_addr: Mutex::new(None),
            options: Mutex::new(SocketOptions {
                rcv_buf: 65536,
                snd_buf: 65536,
                ..Default::default()
            }),
            non_blocking: Mutex::new(false),
        }
    }

    /// Get socket type
    pub fn socket_type(&self) -> SocketType {
        self.socket_type
    }

    /// Get local IP address
    pub fn local_ip(&self) -> Ipv4Address {
        self.local_addr
            .lock()
            .map(|a| a.addr)
            .unwrap_or(Ipv4Address::ZERO)
    }

    /// Get local port
    pub fn local_port(&self) -> u16 {
        self.local_addr.lock().map(|a| a.port).unwrap_or(0)
    }

    /// Get local address
    pub fn local_addr(&self) -> Option<SocketAddr> {
        *self.local_addr.lock()
    }

    /// Get remote address
    pub fn remote_addr(&self) -> Option<SocketAddr> {
        *self.remote_addr.lock()
    }

    /// Check if socket is bound
    pub fn is_bound(&self) -> bool {
        self.local_addr.lock().is_some()
    }

    /// Check if non-blocking
    pub fn is_non_blocking(&self) -> bool {
        *self.non_blocking.lock()
    }

    /// Set non-blocking mode
    pub fn set_non_blocking(&self, non_blocking: bool) {
        *self.non_blocking.lock() = non_blocking;
    }

    /// Bind to a local address
    pub fn bind(&self, addr: SocketAddr) -> Result<(), NetworkError> {
        let mut local = self.local_addr.lock();
        if local.is_some() {
            return Err(NetworkError::AlreadyBound);
        }

        match self.socket_type {
            SocketType::Dgram => {
                let udp = UdpSocket::new(addr.addr, addr.port);
                *self.inner.lock() = SocketInner::Udp(udp);
            }
            SocketType::Stream => {
                // TCP bind is handled when listen/connect is called
            }
            _ => return Err(NetworkError::InvalidSocket),
        }

        *local = Some(addr);
        Ok(())
    }

    /// Listen for incoming connections (TCP)
    pub fn listen(&self, stack: &NetworkStack, _backlog: u32) -> Result<(), NetworkError> {
        if self.socket_type != SocketType::Stream {
            return Err(NetworkError::InvalidSocket);
        }

        let local = self.local_addr.lock();
        let addr = local.ok_or(NetworkError::NotBound)?;

        let conn = tcp::listen(stack, addr.addr, addr.port);
        *self.inner.lock() = SocketInner::Tcp(conn);

        Ok(())
    }

    /// Accept an incoming connection (TCP)
    pub fn accept(&self) -> Result<Option<Arc<TcpConnection>>, NetworkError> {
        if self.socket_type != SocketType::Stream {
            return Err(NetworkError::InvalidSocket);
        }

        let inner = self.inner.lock();
        match &*inner {
            SocketInner::Tcp(listener) => {
                if listener.state() != TcpState::Listen {
                    return Err(NetworkError::InvalidSocket);
                }
                Ok(listener.accept())
            }
            _ => Err(NetworkError::InvalidSocket),
        }
    }

    /// Connect to a remote address
    pub fn connect(
        &self,
        stack: &NetworkStack,
        iface: &Arc<NetworkInterface>,
        addr: SocketAddr,
    ) -> Result<(), NetworkError> {
        let local = {
            let local_guard = self.local_addr.lock();
            if let Some(local) = *local_guard {
                local
            } else {
                // Auto-bind to ephemeral port
                let port = stack.allocate_port();
                let config = iface.config.read();
                SocketAddr::new(config.ipv4, port)
            }
        };

        // Update local address if we auto-bound
        if self.local_addr.lock().is_none() {
            *self.local_addr.lock() = Some(local);
        }

        match self.socket_type {
            SocketType::Stream => {
                let conn = tcp::connect(
                    stack,
                    iface,
                    local.addr,
                    local.port,
                    addr.addr,
                    addr.port,
                )?;
                *self.inner.lock() = SocketInner::Tcp(conn);
            }
            SocketType::Dgram => {
                // For UDP, just store the remote address
                let mut inner = self.inner.lock();
                if let SocketInner::Udp(udp) = &*inner {
                    udp.connect(addr.addr, addr.port);
                } else {
                    let udp = UdpSocket::new(local.addr, local.port);
                    udp.connect(addr.addr, addr.port);
                    *inner = SocketInner::Udp(udp);
                }
            }
            _ => return Err(NetworkError::InvalidSocket),
        }

        *self.remote_addr.lock() = Some(addr);
        Ok(())
    }

    /// Send data (for connected sockets)
    pub fn send(
        &self,
        stack: &NetworkStack,
        iface: &Arc<NetworkInterface>,
        data: &[u8],
    ) -> Result<usize, NetworkError> {
        let remote = self.remote_addr.lock().ok_or(NetworkError::NotConnected)?;
        self.sendto(stack, iface, data, remote)
    }

    /// Send data to a specific address
    pub fn sendto(
        &self,
        stack: &NetworkStack,
        iface: &Arc<NetworkInterface>,
        data: &[u8],
        addr: SocketAddr,
    ) -> Result<usize, NetworkError> {
        let local = self.local_addr.lock();
        let local_addr = local.ok_or(NetworkError::NotBound)?;

        match self.socket_type {
            SocketType::Dgram => {
                super::udp::send_udp(
                    stack,
                    iface,
                    local_addr.addr,
                    local_addr.port,
                    addr.addr,
                    addr.port,
                    data,
                )?;
                Ok(data.len())
            }
            SocketType::Stream => {
                let inner = self.inner.lock();
                if let SocketInner::Tcp(conn) = &*inner {
                    tcp::send_data(stack, iface, conn, data)
                } else {
                    Err(NetworkError::NotConnected)
                }
            }
            _ => Err(NetworkError::InvalidSocket),
        }
    }

    /// Receive data (for connected sockets)
    pub fn recv(&self, buf: &mut [u8]) -> Result<usize, NetworkError> {
        match self.socket_type {
            SocketType::Dgram => {
                let inner = self.inner.lock();
                if let SocketInner::Udp(udp) = &*inner {
                    if let Some(datagram) = udp.recv() {
                        let len = core::cmp::min(buf.len(), datagram.data.len());
                        buf[..len].copy_from_slice(&datagram.data[..len]);
                        Ok(len)
                    } else if *self.non_blocking.lock() {
                        Err(NetworkError::WouldBlock)
                    } else {
                        Ok(0) // Would block in blocking mode
                    }
                } else {
                    Err(NetworkError::NotBound)
                }
            }
            SocketType::Stream => {
                let inner = self.inner.lock();
                if let SocketInner::Tcp(conn) = &*inner {
                    let len = conn.read(buf);
                    if len == 0 && *self.non_blocking.lock() {
                        Err(NetworkError::WouldBlock)
                    } else {
                        Ok(len)
                    }
                } else {
                    Err(NetworkError::NotConnected)
                }
            }
            _ => Err(NetworkError::InvalidSocket),
        }
    }

    /// Receive data with source address (for UDP)
    pub fn recvfrom(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr), NetworkError> {
        if self.socket_type != SocketType::Dgram {
            return Err(NetworkError::InvalidSocket);
        }

        let inner = self.inner.lock();
        if let SocketInner::Udp(udp) = &*inner {
            if let Some(datagram) = udp.recv() {
                let len = core::cmp::min(buf.len(), datagram.data.len());
                buf[..len].copy_from_slice(&datagram.data[..len]);
                let addr = SocketAddr::new(datagram.src_ip, datagram.src_port);
                Ok((len, addr))
            } else if *self.non_blocking.lock() {
                Err(NetworkError::WouldBlock)
            } else {
                Err(NetworkError::WouldBlock) // No data available
            }
        } else {
            Err(NetworkError::NotBound)
        }
    }

    /// Shutdown socket
    pub fn shutdown(
        &self,
        stack: &NetworkStack,
        iface: &Arc<NetworkInterface>,
        how: ShutdownMode,
    ) -> Result<(), NetworkError> {
        if self.socket_type == SocketType::Stream {
            let inner = self.inner.lock();
            if let SocketInner::Tcp(conn) = &*inner {
                match how {
                    ShutdownMode::Write | ShutdownMode::Both => {
                        tcp::close(stack, iface, conn)?;
                    }
                    ShutdownMode::Read => {
                        // Just stop receiving
                    }
                }
            }
        }
        Ok(())
    }

    /// Close the socket
    pub fn close(
        &self,
        stack: &NetworkStack,
        iface: &Arc<NetworkInterface>,
    ) -> Result<(), NetworkError> {
        match self.socket_type {
            SocketType::Stream => {
                let inner = self.inner.lock();
                if let SocketInner::Tcp(conn) = &*inner {
                    tcp::close(stack, iface, conn)?;
                }
            }
            SocketType::Dgram => {
                // UDP sockets can just be dropped
            }
            _ => {}
        }
        Ok(())
    }

    /// Deliver UDP datagram to socket
    pub fn deliver_udp(&self, datagram: UdpDatagram) {
        let inner = self.inner.lock();
        if let SocketInner::Udp(udp) = &*inner {
            udp.queue_rx(datagram);
        }
    }

    /// Set socket option
    pub fn setsockopt(&self, level: i32, optname: i32, value: &[u8]) -> Result<(), NetworkError> {
        let _ = level;
        let mut opts = self.options.lock();

        match optname {
            2 => {
                // SO_REUSEADDR
                opts.reuse_addr = !value.is_empty() && value[0] != 0;
            }
            15 => {
                // SO_REUSEPORT
                opts.reuse_port = !value.is_empty() && value[0] != 0;
            }
            9 => {
                // SO_KEEPALIVE
                opts.keep_alive = !value.is_empty() && value[0] != 0;
            }
            6 => {
                // SO_BROADCAST
                opts.broadcast = !value.is_empty() && value[0] != 0;
            }
            8 if value.len() >= 4 => {
                // SO_RCVBUF
                opts.rcv_buf = u32::from_ne_bytes([value[0], value[1], value[2], value[3]]) as usize;
            }
            7 if value.len() >= 4 => {
                // SO_SNDBUF
                opts.snd_buf = u32::from_ne_bytes([value[0], value[1], value[2], value[3]]) as usize;
            }
            _ => return Err(NetworkError::InvalidOption),
        }

        Ok(())
    }

    /// Get socket option
    pub fn getsockopt(&self, level: i32, optname: i32, buf: &mut [u8]) -> Result<usize, NetworkError> {
        let _ = level;
        let opts = self.options.lock();

        match optname {
            2 => {
                // SO_REUSEADDR
                if !buf.is_empty() {
                    buf[0] = opts.reuse_addr as u8;
                }
                Ok(1)
            }
            15 => {
                // SO_REUSEPORT
                if !buf.is_empty() {
                    buf[0] = opts.reuse_port as u8;
                }
                Ok(1)
            }
            9 => {
                // SO_KEEPALIVE
                if !buf.is_empty() {
                    buf[0] = opts.keep_alive as u8;
                }
                Ok(1)
            }
            3 => {
                // SO_TYPE
                if buf.len() >= 4 {
                    let t = self.socket_type as u32;
                    buf[0..4].copy_from_slice(&t.to_ne_bytes());
                }
                Ok(4)
            }
            4 => {
                // SO_ERROR
                if buf.len() >= 4 {
                    buf[0..4].copy_from_slice(&0u32.to_ne_bytes());
                }
                Ok(4)
            }
            _ => Err(NetworkError::InvalidOption),
        }
    }

    /// Get TCP connection state
    pub fn tcp_state(&self) -> Option<TcpState> {
        let inner = self.inner.lock();
        if let SocketInner::Tcp(conn) = &*inner {
            Some(conn.state())
        } else {
            None
        }
    }

    /// Check if data is available to read
    pub fn poll_read(&self) -> bool {
        let inner = self.inner.lock();
        match &*inner {
            SocketInner::Udp(udp) => !udp.rx_empty(),
            SocketInner::Tcp(conn) => !conn.rx_empty(),
            _ => false,
        }
    }

    /// Check if socket can accept writes
    pub fn poll_write(&self) -> bool {
        let inner = self.inner.lock();
        match &*inner {
            SocketInner::Udp(_) => true,
            SocketInner::Tcp(conn) => conn.state().can_send(),
            _ => false,
        }
    }

    // =========================================================================
    // POLL HELPER METHODS (for poll/select syscalls)
    // =========================================================================

    /// Check if data is available to read (alias for poll_read)
    pub fn has_data(&self) -> bool {
        self.poll_read()
    }

    /// Check if socket can accept writes (alias for poll_write)
    pub fn can_write(&self) -> bool {
        self.poll_write()
    }

    /// Check if a listening socket has pending connections
    pub fn has_pending_connection(&self) -> bool {
        let inner = self.inner.lock();
        match &*inner {
            SocketInner::Tcp(conn) => {
                conn.state() == TcpState::Listen && conn.has_pending_accept()
            }
            _ => false,
        }
    }

    /// Check if socket has an error condition
    pub fn has_error(&self) -> bool {
        let inner = self.inner.lock();
        match &*inner {
            SocketInner::Tcp(conn) => {
                matches!(conn.state(), TcpState::Closed)
            }
            _ => false,
        }
    }

    /// Check if socket is closed
    pub fn is_closed(&self) -> bool {
        let inner = self.inner.lock();
        match &*inner {
            SocketInner::Tcp(conn) => {
                matches!(conn.state(),
                    TcpState::Closed |
                    TcpState::TimeWait |
                    TcpState::LastAck
                )
            }
            SocketInner::Unbound { .. } => true,
            _ => false,
        }
    }
}

// =============================================================================
// SOCKET SYSCALL HELPERS
// =============================================================================

/// Create a new socket
pub fn sys_socket(
    _stack: &NetworkStack,
    domain: u16,
    socket_type: u32,
    _protocol: u32,
) -> Result<Arc<Socket>, NetworkError> {
    let family = AddressFamily::from_u16(domain).ok_or(NetworkError::InvalidFamily)?;

    if family != AddressFamily::Inet {
        return Err(NetworkError::InvalidFamily);
    }

    let sock_type = SocketType::from_u32(socket_type & 0xFF).ok_or(NetworkError::InvalidType)?;

    // Handle SOCK_NONBLOCK flag
    let sock = Arc::new(Socket::new(sock_type));
    if socket_type & 0x800 != 0 {
        sock.set_non_blocking(true);
    }

    Ok(sock)
}

/// Bind a socket to an address
pub fn sys_bind(socket: &Socket, addr: &[u8]) -> Result<(), NetworkError> {
    let sock_addr = SocketAddr::from_bytes(addr).ok_or(NetworkError::InvalidAddress)?;
    socket.bind(sock_addr)
}

/// Listen for connections
pub fn sys_listen(stack: &NetworkStack, socket: &Socket, backlog: u32) -> Result<(), NetworkError> {
    socket.listen(stack, backlog)
}

/// Accept a connection
pub fn sys_accept(socket: &Socket) -> Result<Option<(Arc<TcpConnection>, SocketAddr)>, NetworkError> {
    if let Some(conn) = socket.accept()? {
        let addr = SocketAddr::new(conn.endpoint.remote_ip, conn.endpoint.remote_port);
        Ok(Some((conn, addr)))
    } else {
        Ok(None)
    }
}

/// Connect to a remote address
pub fn sys_connect(
    stack: &NetworkStack,
    iface: &Arc<NetworkInterface>,
    socket: &Socket,
    addr: &[u8],
) -> Result<(), NetworkError> {
    let sock_addr = SocketAddr::from_bytes(addr).ok_or(NetworkError::InvalidAddress)?;
    socket.connect(stack, iface, sock_addr)
}
