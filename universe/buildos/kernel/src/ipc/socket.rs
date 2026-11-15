// ===============================================================================
// BUILDOS KERNEL - UNIX DOMAIN SOCKETS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Unix Domain Socket Implementation
//!
//! Provides local inter-process communication via sockets.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicBool, Ordering};

use super::IpcError;
use crate::process::Pid;
use crate::sync::{Mutex, RwLock};

// =============================================================================
// CONSTANTS
// =============================================================================

/// Socket types
pub const SOCK_STREAM: i32 = 1;   // Stream (connection-oriented)
pub const SOCK_DGRAM: i32 = 2;    // Datagram (connectionless)
pub const SOCK_SEQPACKET: i32 = 5; // Sequenced packet

/// Socket flags
pub const SOCK_CLOEXEC: i32 = 0o2000000;
pub const SOCK_NONBLOCK: i32 = 0o4000;

/// Address family
pub const AF_UNIX: i32 = 1;
pub const AF_LOCAL: i32 = AF_UNIX;

/// Maximum connections in listen queue
pub const SOMAXCONN: usize = 4096;

/// Maximum path length for Unix socket
pub const UNIX_PATH_MAX: usize = 108;

/// Socket buffer size
pub const SOCK_BUF_SIZE: usize = 212992;

/// SCM (Socket Control Message) types
pub const SCM_RIGHTS: i32 = 1;      // File descriptors
pub const SCM_CREDENTIALS: i32 = 2;  // Process credentials

// =============================================================================
// SOCKET ADDRESS
// =============================================================================

/// Unix socket address
#[derive(Clone, Debug)]
pub enum UnixAddress {
    /// Unnamed socket (socketpair)
    Unnamed,
    /// Pathname socket (filesystem path)
    Pathname(String),
    /// Abstract socket (Linux extension)
    Abstract(Vec<u8>),
}

impl UnixAddress {
    /// Create from path
    pub fn from_path(path: &str) -> Self {
        UnixAddress::Pathname(path.into())
    }

    /// Create abstract address
    pub fn abstract_addr(name: &[u8]) -> Self {
        UnixAddress::Abstract(name.to_vec())
    }

    /// Is unnamed
    pub fn is_unnamed(&self) -> bool {
        matches!(self, UnixAddress::Unnamed)
    }

    /// Get path if pathname socket
    pub fn path(&self) -> Option<&str> {
        match self {
            UnixAddress::Pathname(p) => Some(p),
            _ => None,
        }
    }
}

// =============================================================================
// SOCKET CREDENTIALS
// =============================================================================

/// Socket credentials (SCM_CREDENTIALS)
#[derive(Clone, Copy, Debug, Default)]
pub struct SocketCredentials {
    /// Process ID
    pub pid: Pid,
    /// User ID
    pub uid: u32,
    /// Group ID
    pub gid: u32,
}

// =============================================================================
// ANCILLARY DATA
// =============================================================================

/// Ancillary (control) message
#[derive(Clone)]
pub enum AncillaryData {
    /// File descriptors
    Rights(Vec<i32>),
    /// Credentials
    Credentials(SocketCredentials),
}

// =============================================================================
// SOCKET BUFFER
// =============================================================================

/// Message in socket buffer
#[derive(Clone)]
pub struct SocketMessage {
    /// Data
    pub data: Vec<u8>,
    /// Source address (for datagram)
    pub source: Option<UnixAddress>,
    /// Ancillary data
    pub ancillary: Vec<AncillaryData>,
}

/// Socket buffer
pub struct SocketBuffer {
    /// Messages (for SOCK_DGRAM/SOCK_SEQPACKET)
    messages: VecDeque<SocketMessage>,
    /// Stream buffer (for SOCK_STREAM)
    stream: VecDeque<u8>,
    /// Ancillary data for stream
    ancillary: VecDeque<AncillaryData>,
    /// Maximum size
    max_size: usize,
    /// Current size
    current_size: usize,
    /// Is stream socket
    is_stream: bool,
}

impl SocketBuffer {
    /// Create new buffer
    pub fn new(is_stream: bool) -> Self {
        Self {
            messages: VecDeque::new(),
            stream: VecDeque::new(),
            ancillary: VecDeque::new(),
            max_size: SOCK_BUF_SIZE,
            current_size: 0,
            is_stream,
        }
    }

    /// Available data
    pub fn available(&self) -> usize {
        if self.is_stream {
            self.stream.len()
        } else {
            self.messages.front().map(|m| m.data.len()).unwrap_or(0)
        }
    }

    /// Is empty
    pub fn is_empty(&self) -> bool {
        if self.is_stream {
            self.stream.is_empty()
        } else {
            self.messages.is_empty()
        }
    }

    /// Space available
    pub fn space(&self) -> usize {
        self.max_size.saturating_sub(self.current_size)
    }

    /// Push data (stream)
    pub fn push_stream(&mut self, data: &[u8], ancillary: Vec<AncillaryData>) -> usize {
        let space = self.space();
        let to_write = data.len().min(space);

        for &byte in &data[..to_write] {
            self.stream.push_back(byte);
        }
        self.current_size += to_write;

        for anc in ancillary {
            self.ancillary.push_back(anc);
        }

        to_write
    }

    /// Pop data (stream)
    pub fn pop_stream(&mut self, buf: &mut [u8]) -> (usize, Vec<AncillaryData>) {
        let available = self.stream.len();
        let to_read = buf.len().min(available);

        for i in 0..to_read {
            buf[i] = self.stream.pop_front().unwrap();
        }
        self.current_size -= to_read;

        let ancillary: Vec<_> = self.ancillary.drain(..).collect();

        (to_read, ancillary)
    }

    /// Push message (datagram)
    pub fn push_message(&mut self, msg: SocketMessage) -> bool {
        if self.current_size + msg.data.len() > self.max_size {
            return false;
        }

        self.current_size += msg.data.len();
        self.messages.push_back(msg);
        true
    }

    /// Pop message (datagram)
    pub fn pop_message(&mut self) -> Option<SocketMessage> {
        let msg = self.messages.pop_front()?;
        self.current_size -= msg.data.len();
        Some(msg)
    }
}

// =============================================================================
// UNIX SOCKET
// =============================================================================

/// Socket state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SocketState {
    /// Created but not bound/connected
    Unconnected,
    /// Bound to address
    Bound,
    /// Listening for connections
    Listening,
    /// Connecting (non-blocking)
    Connecting,
    /// Connected
    Connected,
    /// Closed
    Closed,
}

/// Unix domain socket
pub struct UnixSocket {
    /// Socket type
    sock_type: i32,
    /// Socket state
    state: Mutex<SocketState>,
    /// Local address
    local_addr: Mutex<UnixAddress>,
    /// Remote address (for connected sockets)
    remote_addr: Mutex<Option<UnixAddress>>,
    /// Receive buffer
    recv_buf: Mutex<SocketBuffer>,
    /// Send buffer (for stream sockets, connected)
    peer: Mutex<Option<Arc<UnixSocket>>>,
    /// Pending connections (for listening sockets)
    pending: Mutex<VecDeque<Arc<UnixSocket>>>,
    /// Backlog size
    backlog: AtomicU32,
    /// Non-blocking
    nonblock: AtomicBool,
    /// Pass credentials
    passcred: AtomicBool,
    /// Shutdown flags (1 = read, 2 = write, 3 = both)
    shutdown: AtomicU32,
}

impl UnixSocket {
    /// Create new socket
    pub fn new(sock_type: i32, flags: i32) -> Arc<Self> {
        let is_stream = sock_type == SOCK_STREAM;

        Arc::new(Self {
            sock_type,
            state: Mutex::new(SocketState::Unconnected),
            local_addr: Mutex::new(UnixAddress::Unnamed),
            remote_addr: Mutex::new(None),
            recv_buf: Mutex::new(SocketBuffer::new(is_stream)),
            peer: Mutex::new(None),
            pending: Mutex::new(VecDeque::new()),
            backlog: AtomicU32::new(0),
            nonblock: AtomicBool::new((flags & SOCK_NONBLOCK) != 0),
            passcred: AtomicBool::new(false),
            shutdown: AtomicU32::new(0),
        })
    }

    /// Bind to address
    pub fn bind(&self, addr: UnixAddress) -> Result<(), IpcError> {
        let mut state = self.state.lock();

        if *state != SocketState::Unconnected {
            return Err(IpcError::InvalidArgument);
        }

        // Register address in global namespace
        if let UnixAddress::Pathname(ref path) = addr {
            let mut bindings = SOCKET_BINDINGS.write();
            if bindings.contains_key(path) {
                return Err(IpcError::Exists);
            }
            // Would create socket file in filesystem
            bindings.insert(path.clone(), ());
        }

        *self.local_addr.lock() = addr;
        *state = SocketState::Bound;

        Ok(())
    }

    /// Listen for connections
    pub fn listen(&self, backlog: u32) -> Result<(), IpcError> {
        let mut state = self.state.lock();

        if self.sock_type == SOCK_DGRAM {
            return Err(IpcError::InvalidArgument);
        }

        match *state {
            SocketState::Bound => {
                self.backlog.store(backlog.min(SOMAXCONN as u32), Ordering::Relaxed);
                *state = SocketState::Listening;
                Ok(())
            }
            _ => Err(IpcError::InvalidArgument),
        }
    }

    /// Connect to address
    pub fn connect(self: &Arc<Self>, addr: &UnixAddress) -> Result<(), IpcError> {
        let mut state = self.state.lock();

        if *state != SocketState::Unconnected && *state != SocketState::Bound {
            return Err(IpcError::InvalidArgument);
        }

        // Find listening socket
        let listener = find_socket(addr)?;

        let listener_state = listener.state.lock();
        if *listener_state != SocketState::Listening {
            return Err(IpcError::NotFound);
        }
        drop(listener_state);

        // Check backlog
        let mut pending = listener.pending.lock();
        if pending.len() >= listener.backlog.load(Ordering::Relaxed) as usize {
            if self.nonblock.load(Ordering::Relaxed) {
                return Err(IpcError::WouldBlock);
            }
            // Would block
        }

        // Add to pending queue
        pending.push_back(self.clone());
        *self.remote_addr.lock() = Some(addr.clone());
        *state = SocketState::Connecting;

        Ok(())
    }

    /// Accept connection
    pub fn accept(self: &Arc<Self>) -> Result<Arc<UnixSocket>, IpcError> {
        let state = self.state.lock();

        if *state != SocketState::Listening {
            return Err(IpcError::InvalidArgument);
        }
        drop(state);

        loop {
            {
                let mut pending = self.pending.lock();
                if let Some(client) = pending.pop_front() {
                    // Create connected pair
                    let server = UnixSocket::new(self.sock_type, 0);

                    // Link them
                    *server.peer.lock() = Some(client.clone());
                    *client.peer.lock() = Some(server.clone());

                    *server.state.lock() = SocketState::Connected;
                    *client.state.lock() = SocketState::Connected;

                    return Ok(server);
                }
            }

            if self.nonblock.load(Ordering::Relaxed) {
                return Err(IpcError::WouldBlock);
            }

            // Would block waiting for connection
            core::hint::spin_loop();
        }
    }

    /// Send data
    pub fn send(&self, data: &[u8], ancillary: Vec<AncillaryData>) -> Result<usize, IpcError> {
        // Check shutdown
        if (self.shutdown.load(Ordering::Relaxed) & 2) != 0 {
            return Err(IpcError::InvalidArgument);
        }

        let peer = self.peer.lock();
        let peer = peer.as_ref().ok_or(IpcError::InvalidArgument)?;

        // Check peer shutdown
        if (peer.shutdown.load(Ordering::Relaxed) & 1) != 0 {
            return Err(IpcError::InvalidArgument);
        }

        let mut buf = peer.recv_buf.lock();

        if self.sock_type == SOCK_STREAM {
            loop {
                let space = buf.space();
                if space > 0 {
                    let written = buf.push_stream(data, ancillary);
                    return Ok(written);
                }

                if self.nonblock.load(Ordering::Relaxed) {
                    return Err(IpcError::WouldBlock);
                }

                drop(buf);
                core::hint::spin_loop();
                buf = peer.recv_buf.lock();
            }
        } else {
            // Datagram
            let msg = SocketMessage {
                data: data.to_vec(),
                source: Some(self.local_addr.lock().clone()),
                ancillary,
            };

            if buf.push_message(msg) {
                Ok(data.len())
            } else if self.nonblock.load(Ordering::Relaxed) {
                Err(IpcError::WouldBlock)
            } else {
                // Would block
                Err(IpcError::WouldBlock)
            }
        }
    }

    /// Receive data
    pub fn recv(&self, buf: &mut [u8]) -> Result<(usize, Vec<AncillaryData>), IpcError> {
        // Check shutdown
        if (self.shutdown.load(Ordering::Relaxed) & 1) != 0 {
            return Ok((0, Vec::new()));
        }

        loop {
            {
                let mut recv_buf = self.recv_buf.lock();

                if !recv_buf.is_empty() {
                    if self.sock_type == SOCK_STREAM {
                        let (n, anc) = recv_buf.pop_stream(buf);
                        return Ok((n, anc));
                    } else {
                        let msg = recv_buf.pop_message().unwrap();
                        let n = msg.data.len().min(buf.len());
                        buf[..n].copy_from_slice(&msg.data[..n]);
                        return Ok((n, msg.ancillary));
                    }
                }
            }

            // Check if peer closed
            let peer = self.peer.lock();
            if peer.is_none() {
                return Ok((0, Vec::new())); // EOF
            }
            drop(peer);

            if self.nonblock.load(Ordering::Relaxed) {
                return Err(IpcError::WouldBlock);
            }

            // Would block waiting for data
            core::hint::spin_loop();
        }
    }

    /// Shutdown socket
    pub fn shutdown(&self, how: i32) -> Result<(), IpcError> {
        let mask = match how {
            0 => 1,  // SHUT_RD
            1 => 2,  // SHUT_WR
            2 => 3,  // SHUT_RDWR
            _ => return Err(IpcError::InvalidArgument),
        };

        self.shutdown.fetch_or(mask, Ordering::Relaxed);
        Ok(())
    }

    /// Poll for events
    pub fn poll(&self) -> u32 {
        let mut events = 0;

        // Readable?
        if !self.recv_buf.lock().is_empty() {
            events |= 1; // POLLIN
        }

        // Writable?
        if let Some(ref peer) = *self.peer.lock() {
            if peer.recv_buf.lock().space() > 0 {
                events |= 4; // POLLOUT
            }
        }

        // Pending connections?
        if !self.pending.lock().is_empty() {
            events |= 1; // POLLIN
        }

        events
    }

    /// Set non-blocking
    pub fn set_nonblock(&self, nonblock: bool) {
        self.nonblock.store(nonblock, Ordering::Relaxed);
    }

    /// Enable credential passing
    pub fn set_passcred(&self, enable: bool) {
        self.passcred.store(enable, Ordering::Relaxed);
    }
}

// =============================================================================
// SOCKET PAIR
// =============================================================================

/// Create connected socket pair
pub fn socketpair(sock_type: i32, flags: i32) -> Result<(Arc<UnixSocket>, Arc<UnixSocket>), IpcError> {
    let sock1 = UnixSocket::new(sock_type, flags);
    let sock2 = UnixSocket::new(sock_type, flags);

    *sock1.peer.lock() = Some(sock2.clone());
    *sock2.peer.lock() = Some(sock1.clone());

    *sock1.state.lock() = SocketState::Connected;
    *sock2.state.lock() = SocketState::Connected;

    Ok((sock1, sock2))
}

// =============================================================================
// GLOBAL SOCKET NAMESPACE
// =============================================================================

/// Bound socket paths
static SOCKET_BINDINGS: RwLock<BTreeMap<String, ()>> = RwLock::new(BTreeMap::new());

/// Listening sockets by path
static LISTENING_SOCKETS: RwLock<BTreeMap<String, Arc<UnixSocket>>> = RwLock::new(BTreeMap::new());

/// Find socket by address
fn find_socket(addr: &UnixAddress) -> Result<Arc<UnixSocket>, IpcError> {
    match addr {
        UnixAddress::Pathname(path) => {
            LISTENING_SOCKETS.read()
                .get(path)
                .cloned()
                .ok_or(IpcError::NotFound)
        }
        _ => Err(IpcError::NotFound),
    }
}

// =============================================================================
// SYSTEM CALLS
// =============================================================================

/// socket - create socket
pub fn socket(domain: i32, sock_type: i32, _protocol: i32) -> Result<i32, IpcError> {
    if domain != AF_UNIX {
        return Err(IpcError::InvalidArgument);
    }

    let base_type = sock_type & 0xF;
    let flags = sock_type & !0xF;

    match base_type {
        SOCK_STREAM | SOCK_DGRAM | SOCK_SEQPACKET => {}
        _ => return Err(IpcError::InvalidArgument),
    }

    let _sock = UnixSocket::new(base_type, flags);

    // Would allocate fd and register
    let fd = allocate_fd()?;

    Ok(fd)
}

/// Initialize socket subsystem
pub fn init() {
    crate::kprintln!("[IPC/SOCKET] Unix domain socket subsystem initialized");
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

fn allocate_fd() -> Result<i32, IpcError> {
    static NEXT_FD: AtomicU32 = AtomicU32::new(300);
    Ok(NEXT_FD.fetch_add(1, Ordering::Relaxed) as i32)
}
