// ===============================================================================
// BUILDOS KERNEL - NETWORK NAMESPACE
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Network Namespace Implementation
//!
//! Provides network stack isolation. Each network namespace has its own:
//! - Network devices (loopback, etc.)
//! - IP addresses
//! - Routing tables
//! - Firewall rules
//! - Socket port space

#![allow(dead_code)]

use alloc::sync::Arc;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::sync::RwLock;
use super::{Namespace, NsType, NsError, next_ns_id};
use super::user::UserNamespace;

/// Network namespace structure
pub struct NetNamespace {
    /// Namespace ID
    id: u64,
    /// Owning user namespace
    user_ns: Arc<UserNamespace>,
    /// Network devices
    devices: RwLock<BTreeMap<String, NetDevice>>,
    /// Interface index counter
    ifindex: AtomicU32,
    /// Loopback device name
    loopback: RwLock<Option<String>>,
    /// Routing table
    routes: RwLock<Vec<Route>>,
    /// Network statistics
    stats: RwLock<NetStats>,
    /// Is initial namespace?
    is_initial: bool,
}

impl NetNamespace {
    /// Create initial (root) network namespace
    pub fn new_initial(user_ns: Arc<UserNamespace>) -> Self {
        let ns = Self {
            id: next_ns_id(),
            user_ns,
            devices: RwLock::new(BTreeMap::new()),
            ifindex: AtomicU32::new(1),
            loopback: RwLock::new(None),
            routes: RwLock::new(Vec::new()),
            stats: RwLock::new(NetStats::default()),
            is_initial: true,
        };

        // Create loopback device
        ns.create_loopback();

        ns
    }

    /// Create child network namespace
    pub fn new_child(user_ns: Arc<UserNamespace>) -> Self {
        let ns = Self {
            id: next_ns_id(),
            user_ns,
            devices: RwLock::new(BTreeMap::new()),
            ifindex: AtomicU32::new(1),
            loopback: RwLock::new(None),
            routes: RwLock::new(Vec::new()),
            stats: RwLock::new(NetStats::default()),
            is_initial: false,
        };

        // Create loopback device
        ns.create_loopback();

        ns
    }

    /// Create loopback device
    fn create_loopback(&self) {
        let lo = NetDevice {
            name: "lo".into(),
            ifindex: self.alloc_ifindex(),
            mac: [0; 6],
            mtu: 65536,
            flags: DeviceFlags::UP | DeviceFlags::LOOPBACK,
            ipv4_addrs: vec![Ipv4Addr::new(127, 0, 0, 1, 8)],
            ipv6_addrs: vec![Ipv6Addr::loopback()],
            stats: DeviceStats::default(),
        };

        self.devices.write().insert("lo".into(), lo);
        *self.loopback.write() = Some("lo".into());

        // Add loopback route
        self.routes.write().push(Route {
            dest: IpNetwork::V4 { addr: [127, 0, 0, 0], prefix: 8 },
            gateway: None,
            device: "lo".into(),
            metric: 0,
            flags: RouteFlags::LOCAL,
        });
    }

    /// Allocate interface index
    fn alloc_ifindex(&self) -> u32 {
        self.ifindex.fetch_add(1, Ordering::Relaxed)
    }

    /// Add network device
    pub fn add_device(&self, device: NetDevice) -> Result<(), NsError> {
        let name = device.name.clone();

        let mut devices = self.devices.write();
        if devices.contains_key(&name) {
            return Err(NsError::InvalidOperation);
        }

        devices.insert(name, device);
        Ok(())
    }

    /// Remove network device
    pub fn remove_device(&self, name: &str) -> Result<NetDevice, NsError> {
        // Cannot remove loopback
        if name == "lo" {
            return Err(NsError::PermissionDenied);
        }

        self.devices.write()
            .remove(name)
            .ok_or(NsError::NotFound)
    }

    /// Get network device by name
    pub fn get_device(&self, name: &str) -> Option<NetDevice> {
        self.devices.read().get(name).cloned()
    }

    /// Get network device by index
    pub fn get_device_by_index(&self, ifindex: u32) -> Option<NetDevice> {
        self.devices.read()
            .values()
            .find(|d| d.ifindex == ifindex)
            .cloned()
    }

    /// List all devices
    pub fn list_devices(&self) -> Vec<String> {
        self.devices.read().keys().cloned().collect()
    }

    /// Add route
    pub fn add_route(&self, route: Route) -> Result<(), NsError> {
        self.routes.write().push(route);
        Ok(())
    }

    /// Remove route
    pub fn remove_route(&self, dest: &IpNetwork) -> Result<(), NsError> {
        let mut routes = self.routes.write();
        let pos = routes.iter().position(|r| &r.dest == dest);

        if let Some(idx) = pos {
            routes.remove(idx);
            Ok(())
        } else {
            Err(NsError::NotFound)
        }
    }

    /// Lookup route for destination
    pub fn route_lookup(&self, dest: &[u8]) -> Option<Route> {
        let routes = self.routes.read();

        // Find most specific matching route
        let mut best: Option<&Route> = None;
        let mut best_prefix = 0;

        for route in routes.iter() {
            if route.matches(dest) {
                let prefix = route.dest.prefix_len();
                if prefix >= best_prefix {
                    best = Some(route);
                    best_prefix = prefix;
                }
            }
        }

        best.cloned()
    }

    /// Is this the initial namespace?
    pub fn is_initial(&self) -> bool {
        self.is_initial
    }

    /// Get namespace statistics
    pub fn stats(&self) -> NetStats {
        self.stats.read().clone()
    }
}

impl Namespace for NetNamespace {
    fn ns_type(&self) -> NsType {
        NsType::Net
    }

    fn id(&self) -> u64 {
        self.id
    }

    fn user_ns(&self) -> Option<Arc<UserNamespace>> {
        Some(self.user_ns.clone())
    }

    fn clone_ns(&self) -> Arc<dyn Namespace> {
        Arc::new(Self::new_child(self.user_ns.clone()))
    }
}

/// Network device
#[derive(Clone)]
pub struct NetDevice {
    /// Device name
    pub name: String,
    /// Interface index
    pub ifindex: u32,
    /// MAC address
    pub mac: [u8; 6],
    /// MTU
    pub mtu: u32,
    /// Device flags
    pub flags: DeviceFlags,
    /// IPv4 addresses
    pub ipv4_addrs: Vec<Ipv4Addr>,
    /// IPv6 addresses
    pub ipv6_addrs: Vec<Ipv6Addr>,
    /// Statistics
    pub stats: DeviceStats,
}

/// Device flags
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DeviceFlags(u32);

impl DeviceFlags {
    pub const UP: Self = Self(1 << 0);
    pub const BROADCAST: Self = Self(1 << 1);
    pub const LOOPBACK: Self = Self(1 << 3);
    pub const POINTOPOINT: Self = Self(1 << 4);
    pub const MULTICAST: Self = Self(1 << 12);
}

impl core::ops::BitOr for DeviceFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// IPv4 address with prefix length
#[derive(Clone)]
pub struct Ipv4Addr {
    pub octets: [u8; 4],
    pub prefix: u8,
}

impl Ipv4Addr {
    pub fn new(a: u8, b: u8, c: u8, d: u8, prefix: u8) -> Self {
        Self { octets: [a, b, c, d], prefix }
    }
}

/// IPv6 address with prefix length
#[derive(Clone)]
pub struct Ipv6Addr {
    pub octets: [u8; 16],
    pub prefix: u8,
}

impl Ipv6Addr {
    pub fn loopback() -> Self {
        let mut octets = [0u8; 16];
        octets[15] = 1;
        Self { octets, prefix: 128 }
    }
}

/// Device statistics
#[derive(Clone, Default)]
pub struct DeviceStats {
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_errors: u64,
    pub tx_errors: u64,
    pub rx_dropped: u64,
    pub tx_dropped: u64,
}

/// IP network (for routing)
#[derive(Clone, PartialEq, Eq)]
pub enum IpNetwork {
    V4 { addr: [u8; 4], prefix: u8 },
    V6 { addr: [u8; 16], prefix: u8 },
}

impl IpNetwork {
    pub fn prefix_len(&self) -> u8 {
        match self {
            Self::V4 { prefix, .. } => *prefix,
            Self::V6 { prefix, .. } => *prefix,
        }
    }

    pub fn contains(&self, addr: &[u8]) -> bool {
        match self {
            Self::V4 { addr: net, prefix } => {
                if addr.len() != 4 {
                    return false;
                }
                let mask = if *prefix == 0 { 0 } else { !0u32 << (32 - prefix) };
                let net_u32 = u32::from_be_bytes(*net);
                let addr_u32 = u32::from_be_bytes([addr[0], addr[1], addr[2], addr[3]]);
                (net_u32 & mask) == (addr_u32 & mask)
            }
            Self::V6 { addr: net, prefix } => {
                if addr.len() != 16 {
                    return false;
                }
                let full_bytes = (*prefix / 8) as usize;
                let remaining_bits = *prefix % 8;

                // Check full bytes
                if net[..full_bytes] != addr[..full_bytes] {
                    return false;
                }

                // Check remaining bits
                if remaining_bits > 0 && full_bytes < 16 {
                    let mask = !0u8 << (8 - remaining_bits);
                    if (net[full_bytes] & mask) != (addr[full_bytes] & mask) {
                        return false;
                    }
                }

                true
            }
        }
    }
}

/// Route entry
#[derive(Clone)]
pub struct Route {
    /// Destination network
    pub dest: IpNetwork,
    /// Gateway (None for direct)
    pub gateway: Option<[u8; 4]>,
    /// Output device
    pub device: String,
    /// Metric
    pub metric: u32,
    /// Route flags
    pub flags: RouteFlags,
}

impl Route {
    pub fn matches(&self, dest: &[u8]) -> bool {
        self.dest.contains(dest)
    }
}

/// Route flags
#[derive(Clone, Copy)]
pub struct RouteFlags(u32);

impl RouteFlags {
    pub const LOCAL: Self = Self(1 << 0);
    pub const GATEWAY: Self = Self(1 << 1);
    pub const HOST: Self = Self(1 << 2);
}

/// Network namespace statistics
#[derive(Clone, Default)]
pub struct NetStats {
    pub sockets: u64,
    pub tcp_connections: u64,
    pub udp_sockets: u64,
}

/// Move device between namespaces
pub fn move_device(
    from_ns: &NetNamespace,
    to_ns: &NetNamespace,
    name: &str,
) -> Result<(), NsError> {
    let device = from_ns.remove_device(name)?;
    to_ns.add_device(device)?;

    crate::kprintln!("[NS] Moved device {} from ns {} to ns {}",
        name, from_ns.id, to_ns.id);

    Ok(())
}

/// Create veth pair
pub fn create_veth_pair(
    ns1: &NetNamespace,
    name1: &str,
    ns2: &NetNamespace,
    name2: &str,
) -> Result<(), NsError> {
    let ifindex1 = ns1.alloc_ifindex();
    let ifindex2 = ns2.alloc_ifindex();

    let dev1 = NetDevice {
        name: name1.into(),
        ifindex: ifindex1,
        mac: generate_mac(),
        mtu: 1500,
        flags: DeviceFlags::BROADCAST | DeviceFlags::MULTICAST,
        ipv4_addrs: Vec::new(),
        ipv6_addrs: Vec::new(),
        stats: DeviceStats::default(),
    };

    let dev2 = NetDevice {
        name: name2.into(),
        ifindex: ifindex2,
        mac: generate_mac(),
        mtu: 1500,
        flags: DeviceFlags::BROADCAST | DeviceFlags::MULTICAST,
        ipv4_addrs: Vec::new(),
        ipv6_addrs: Vec::new(),
        stats: DeviceStats::default(),
    };

    ns1.add_device(dev1)?;
    ns2.add_device(dev2)?;

    crate::kprintln!("[NS] Created veth pair: {} <-> {}", name1, name2);

    Ok(())
}

/// Generate random MAC address (locally administered)
fn generate_mac() -> [u8; 6] {
    // Would use actual random generator
    [0x02, 0x00, 0x00, 0x00, 0x00, 0x01]
}
