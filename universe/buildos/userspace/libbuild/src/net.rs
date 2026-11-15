// ===============================================================================
// NETWORK OPERATIONS
// ===============================================================================

use crate::syscall::*;

// =============================================================================
// SOCKET TYPES
// =============================================================================

pub const AF_UNSPEC: u16 = 0;
pub const AF_UNIX: u16 = 1;
pub const AF_INET: u16 = 2;
pub const AF_INET6: u16 = 10;

pub const SOCK_STREAM: u32 = 1;
pub const SOCK_DGRAM: u32 = 2;
pub const SOCK_RAW: u32 = 3;
pub const SOCK_SEQPACKET: u32 = 5;

pub const IPPROTO_TCP: u32 = 6;
pub const IPPROTO_UDP: u32 = 17;
pub const IPPROTO_RAW: u32 = 255;

// =============================================================================
// SHUTDOWN OPTIONS
// =============================================================================

pub const SHUT_RD: i32 = 0;
pub const SHUT_WR: i32 = 1;
pub const SHUT_RDWR: i32 = 2;

// =============================================================================
// SOCKET OPTIONS
// =============================================================================

pub const SOL_SOCKET: i32 = 1;
pub const SO_REUSEADDR: i32 = 2;
pub const SO_ERROR: i32 = 4;
pub const SO_KEEPALIVE: i32 = 9;
pub const SO_RCVBUF: i32 = 8;
pub const SO_SNDBUF: i32 = 7;
pub const SO_RCVTIMEO: i32 = 20;
pub const SO_SNDTIMEO: i32 = 21;

pub const IPPROTO_IP: i32 = 0;
pub const IP_TTL: i32 = 2;

pub const IPPROTO_TCP_OPT: i32 = 6;
pub const TCP_NODELAY: i32 = 1;

// =============================================================================
// ADDRESS STRUCTURES
// =============================================================================

/// IPv4 socket address
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SockaddrIn {
    pub sin_family: u16,
    pub sin_port: u16,  // Big-endian
    pub sin_addr: u32,  // Big-endian
    pub sin_zero: [u8; 8],
}

impl SockaddrIn {
    pub const fn new(addr: u32, port: u16) -> Self {
        Self {
            sin_family: AF_INET,
            sin_port: port.to_be(),
            sin_addr: addr.to_be(),
            sin_zero: [0; 8],
        }
    }

    pub const fn any(port: u16) -> Self {
        Self::new(0, port)
    }

    pub const fn loopback(port: u16) -> Self {
        Self::new(0x7f000001, port)  // 127.0.0.1
    }
}

/// IPv4 address from octets
pub const fn ipv4_addr(a: u8, b: u8, c: u8, d: u8) -> u32 {
    ((a as u32) << 24) | ((b as u32) << 16) | ((c as u32) << 8) | (d as u32)
}

// =============================================================================
// SOCKET OPERATIONS
// =============================================================================

/// Create a socket
pub fn socket(domain: u16, socket_type: u32, protocol: u32) -> i32 {
    unsafe {
        syscall3(SYS_SOCKET, domain as u64, socket_type as u64, protocol as u64) as i32
    }
}

/// Bind a socket to an address
pub fn bind(sockfd: i32, addr: &SockaddrIn) -> i32 {
    unsafe {
        syscall3(
            SYS_BIND,
            sockfd as u64,
            addr as *const SockaddrIn as u64,
            core::mem::size_of::<SockaddrIn>() as u64,
        ) as i32
    }
}

/// Listen for connections
pub fn listen(sockfd: i32, backlog: i32) -> i32 {
    unsafe {
        syscall2(SYS_LISTEN, sockfd as u64, backlog as u64) as i32
    }
}

/// Accept a connection
pub fn accept(sockfd: i32, addr: Option<&mut SockaddrIn>) -> i32 {
    let (addr_ptr, len_ptr) = match addr {
        Some(a) => {
            static mut ADDRLEN: u32 = core::mem::size_of::<SockaddrIn>() as u32;
            (a as *mut SockaddrIn as u64, &raw mut ADDRLEN as *mut u32 as u64)
        }
        None => (0, 0),
    };

    unsafe {
        syscall3(SYS_ACCEPT, sockfd as u64, addr_ptr, len_ptr) as i32
    }
}

/// Connect to a remote address
pub fn connect(sockfd: i32, addr: &SockaddrIn) -> i32 {
    unsafe {
        syscall3(
            SYS_CONNECT,
            sockfd as u64,
            addr as *const SockaddrIn as u64,
            core::mem::size_of::<SockaddrIn>() as u64,
        ) as i32
    }
}

/// Send data on a socket
pub fn send(sockfd: i32, buf: &[u8], flags: i32) -> isize {
    unsafe {
        syscall4(
            SYS_SENDTO,
            sockfd as u64,
            buf.as_ptr() as u64,
            buf.len() as u64,
            flags as u64,
        ) as isize
    }
}

/// Receive data from a socket
pub fn recv(sockfd: i32, buf: &mut [u8], flags: i32) -> isize {
    unsafe {
        syscall4(
            SYS_RECVFROM,
            sockfd as u64,
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
            flags as u64,
        ) as isize
    }
}

/// Send data to a specific address (UDP)
pub fn sendto(sockfd: i32, buf: &[u8], flags: i32, addr: &SockaddrIn) -> isize {
    unsafe {
        syscall6(
            SYS_SENDTO,
            sockfd as u64,
            buf.as_ptr() as u64,
            buf.len() as u64,
            flags as u64,
            addr as *const SockaddrIn as u64,
            core::mem::size_of::<SockaddrIn>() as u64,
        ) as isize
    }
}

/// Receive data with source address (UDP)
pub fn recvfrom(sockfd: i32, buf: &mut [u8], flags: i32, addr: &mut SockaddrIn) -> isize {
    let mut addrlen = core::mem::size_of::<SockaddrIn>() as u32;
    unsafe {
        syscall6(
            SYS_RECVFROM,
            sockfd as u64,
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
            flags as u64,
            addr as *mut SockaddrIn as u64,
            &mut addrlen as *mut u32 as u64,
        ) as isize
    }
}

/// Shutdown socket
pub fn shutdown(sockfd: i32, how: i32) -> i32 {
    unsafe {
        syscall2(SYS_SHUTDOWN, sockfd as u64, how as u64) as i32
    }
}

/// Set socket option
pub fn setsockopt(sockfd: i32, level: i32, optname: i32, optval: &[u8]) -> i32 {
    unsafe {
        syscall5(
            SYS_SETSOCKOPT,
            sockfd as u64,
            level as u64,
            optname as u64,
            optval.as_ptr() as u64,
            optval.len() as u64,
        ) as i32
    }
}

/// Get socket option
pub fn getsockopt(sockfd: i32, level: i32, optname: i32, optval: &mut [u8]) -> i32 {
    let mut optlen = optval.len() as u32;
    unsafe {
        syscall5(
            SYS_GETSOCKOPT,
            sockfd as u64,
            level as u64,
            optname as u64,
            optval.as_mut_ptr() as u64,
            &mut optlen as *mut u32 as u64,
        ) as i32
    }
}

/// Get local socket address
pub fn getsockname(sockfd: i32, addr: &mut SockaddrIn) -> i32 {
    let mut addrlen = core::mem::size_of::<SockaddrIn>() as u32;
    unsafe {
        syscall3(
            SYS_GETSOCKNAME,
            sockfd as u64,
            addr as *mut SockaddrIn as u64,
            &mut addrlen as *mut u32 as u64,
        ) as i32
    }
}

/// Get peer socket address
pub fn getpeername(sockfd: i32, addr: &mut SockaddrIn) -> i32 {
    let mut addrlen = core::mem::size_of::<SockaddrIn>() as u32;
    unsafe {
        syscall3(
            SYS_GETPEERNAME,
            sockfd as u64,
            addr as *mut SockaddrIn as u64,
            &mut addrlen as *mut u32 as u64,
        ) as i32
    }
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Convert host byte order to network byte order (u16)
pub const fn htons(hostshort: u16) -> u16 {
    hostshort.to_be()
}

/// Convert network byte order to host byte order (u16)
pub const fn ntohs(netshort: u16) -> u16 {
    u16::from_be(netshort)
}

/// Convert host byte order to network byte order (u32)
pub const fn htonl(hostlong: u32) -> u32 {
    hostlong.to_be()
}

/// Convert network byte order to host byte order (u32)
pub const fn ntohl(netlong: u32) -> u32 {
    u32::from_be(netlong)
}
