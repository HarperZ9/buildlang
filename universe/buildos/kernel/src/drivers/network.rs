// ===============================================================================
// BUILDOS KERNEL - NETWORK DRIVERS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! Network device drivers (virtio-net).
//!
//! Implements virtio-net for paravirtualized network access in VMs.

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::mem;
use spin::Mutex;

use super::pci::{self, PciDevice, Bar};

// =============================================================================
// VIRTIO CONSTANTS
// =============================================================================

/// Virtio vendor ID
const VIRTIO_VENDOR_ID: u16 = 0x1AF4;

/// Virtio network device ID (transitional)
const VIRTIO_NET_DEVICE_ID: u16 = 0x1000;

/// Virtio network device ID (modern)
const VIRTIO_NET_MODERN_DEVICE_ID: u16 = 0x1041;

/// Virtio PCI capability types
const VIRTIO_PCI_CAP_COMMON_CFG: u8 = 1;
const VIRTIO_PCI_CAP_NOTIFY_CFG: u8 = 2;
const VIRTIO_PCI_CAP_ISR_CFG: u8 = 3;
const VIRTIO_PCI_CAP_DEVICE_CFG: u8 = 4;
const VIRTIO_PCI_CAP_PCI_CFG: u8 = 5;

/// Device status bits
const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 1;
const VIRTIO_STATUS_DRIVER: u8 = 2;
const VIRTIO_STATUS_DRIVER_OK: u8 = 4;
const VIRTIO_STATUS_FEATURES_OK: u8 = 8;
const VIRTIO_STATUS_FAILED: u8 = 128;

/// Feature bits
const VIRTIO_NET_F_MAC: u64 = 1 << 5;
const VIRTIO_NET_F_STATUS: u64 = 1 << 16;
const VIRTIO_NET_F_MRG_RXBUF: u64 = 1 << 15;
const VIRTIO_F_VERSION_1: u64 = 1 << 32;

/// Virtqueue indices
const RX_QUEUE: u16 = 0;
const TX_QUEUE: u16 = 1;

/// Queue sizes
const QUEUE_SIZE: u16 = 256;

/// Maximum packet size
const MAX_PACKET_SIZE: usize = 1514;

/// Ethernet header size
const ETH_HEADER_SIZE: usize = 14;

/// Virtio net header size
const VIRTIO_NET_HDR_SIZE: usize = 12;

// =============================================================================
// VIRTQUEUE STRUCTURES
// =============================================================================

/// Virtqueue descriptor
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct VirtqDesc {
    /// Buffer address
    pub addr: u64,
    /// Buffer length
    pub len: u32,
    /// Flags
    pub flags: u16,
    /// Next descriptor index
    pub next: u16,
}

/// Descriptor flags
const VIRTQ_DESC_F_NEXT: u16 = 1;
const VIRTQ_DESC_F_WRITE: u16 = 2;

/// Virtqueue available ring
#[repr(C)]
pub struct VirtqAvail {
    /// Flags
    pub flags: u16,
    /// Index
    pub idx: u16,
    /// Ring entries
    pub ring: [u16; QUEUE_SIZE as usize],
    /// Used event
    pub used_event: u16,
}

/// Virtqueue used ring element
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct VirtqUsedElem {
    /// Descriptor chain start
    pub id: u32,
    /// Length of data written
    pub len: u32,
}

/// Virtqueue used ring
#[repr(C)]
pub struct VirtqUsed {
    /// Flags
    pub flags: u16,
    /// Index
    pub idx: u16,
    /// Ring entries
    pub ring: [VirtqUsedElem; QUEUE_SIZE as usize],
    /// Available event
    pub avail_event: u16,
}

/// Complete virtqueue structure
pub struct Virtqueue {
    /// Queue size
    size: u16,
    /// Descriptors
    descs: Box<[VirtqDesc; QUEUE_SIZE as usize]>,
    /// Available ring
    avail: Box<VirtqAvail>,
    /// Used ring
    used: Box<VirtqUsed>,
    /// Next free descriptor
    free_head: u16,
    /// Number of free descriptors
    num_free: u16,
    /// Last seen used index
    last_used_idx: u16,
    /// Buffer tracking
    buffers: [Option<Box<[u8]>>; QUEUE_SIZE as usize],
}

impl Virtqueue {
    /// Create a new virtqueue
    pub fn new() -> Self {
        let mut descs = Box::new([VirtqDesc::default(); QUEUE_SIZE as usize]);

        // Link descriptors into free list
        for i in 0..(QUEUE_SIZE - 1) {
            descs[i as usize].next = i + 1;
        }

        let avail = unsafe { Box::new(mem::zeroed()) };
        let used = unsafe { Box::new(mem::zeroed()) };

        const NONE: Option<Box<[u8]>> = None;

        Self {
            size: QUEUE_SIZE,
            descs,
            avail,
            used,
            free_head: 0,
            num_free: QUEUE_SIZE,
            last_used_idx: 0,
            buffers: [NONE; QUEUE_SIZE as usize],
        }
    }

    /// Get descriptor table address
    pub fn desc_addr(&self) -> u64 {
        self.descs.as_ptr() as u64
    }

    /// Get available ring address
    pub fn avail_addr(&self) -> u64 {
        self.avail.as_ref() as *const _ as u64
    }

    /// Get used ring address
    pub fn used_addr(&self) -> u64 {
        self.used.as_ref() as *const _ as u64
    }

    /// Allocate a descriptor
    fn alloc_desc(&mut self) -> Option<u16> {
        if self.num_free == 0 {
            return None;
        }

        let idx = self.free_head;
        self.free_head = self.descs[idx as usize].next;
        self.num_free -= 1;

        Some(idx)
    }

    /// Free a descriptor
    fn free_desc(&mut self, idx: u16) {
        self.descs[idx as usize].next = self.free_head;
        self.free_head = idx;
        self.num_free += 1;
        self.buffers[idx as usize] = None;
    }

    /// Add buffer to queue (for receive)
    pub fn add_buf_recv(&mut self, buf: Box<[u8]>) -> Option<u16> {
        let desc_idx = self.alloc_desc()?;

        let desc = &mut self.descs[desc_idx as usize];
        desc.addr = buf.as_ptr() as u64;
        desc.len = buf.len() as u32;
        desc.flags = VIRTQ_DESC_F_WRITE;
        desc.next = 0;

        self.buffers[desc_idx as usize] = Some(buf);

        // Add to available ring
        let avail_idx = self.avail.idx;
        self.avail.ring[(avail_idx % self.size) as usize] = desc_idx;

        // Memory barrier
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

        self.avail.idx = avail_idx.wrapping_add(1);

        Some(desc_idx)
    }

    /// Add buffer to queue (for transmit with header)
    pub fn add_buf_send(&mut self, header: &[u8], data: &[u8]) -> Option<u16> {
        // Allocate two descriptors (header + data)
        let header_idx = self.alloc_desc()?;
        let data_idx = match self.alloc_desc() {
            Some(idx) => idx,
            None => {
                self.free_desc(header_idx);
                return None;
            }
        };

        // Set up header descriptor
        let header_buf: Box<[u8]> = header.into();
        let header_desc = &mut self.descs[header_idx as usize];
        header_desc.addr = header_buf.as_ptr() as u64;
        header_desc.len = header_buf.len() as u32;
        header_desc.flags = VIRTQ_DESC_F_NEXT;
        header_desc.next = data_idx;
        self.buffers[header_idx as usize] = Some(header_buf);

        // Set up data descriptor
        let data_buf: Box<[u8]> = data.into();
        let data_desc = &mut self.descs[data_idx as usize];
        data_desc.addr = data_buf.as_ptr() as u64;
        data_desc.len = data_buf.len() as u32;
        data_desc.flags = 0;
        data_desc.next = 0;
        self.buffers[data_idx as usize] = Some(data_buf);

        // Add to available ring
        let avail_idx = self.avail.idx;
        self.avail.ring[(avail_idx % self.size) as usize] = header_idx;

        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

        self.avail.idx = avail_idx.wrapping_add(1);

        Some(header_idx)
    }

    /// Get completed buffer
    pub fn get_buf(&mut self) -> Option<(u16, Box<[u8]>, u32)> {
        if self.last_used_idx == self.used.idx {
            return None;
        }

        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

        let used_elem = &self.used.ring[(self.last_used_idx % self.size) as usize];
        let desc_idx = used_elem.id as u16;
        let len = used_elem.len;

        self.last_used_idx = self.last_used_idx.wrapping_add(1);

        // Free descriptor chain and get buffer
        let mut idx = desc_idx;
        let mut result_buf = None;

        loop {
            let desc = &self.descs[idx as usize];
            let has_next = (desc.flags & VIRTQ_DESC_F_NEXT) != 0;
            let next = desc.next;

            if result_buf.is_none() {
                result_buf = self.buffers[idx as usize].take();
            } else {
                self.buffers[idx as usize] = None;
            }

            self.free_desc(idx);

            if !has_next {
                break;
            }
            idx = next;
        }

        result_buf.map(|buf| (desc_idx, buf, len))
    }
}

// =============================================================================
// VIRTIO NET HEADER
// =============================================================================

/// Virtio network header
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct VirtioNetHdr {
    pub flags: u8,
    pub gso_type: u8,
    pub hdr_len: u16,
    pub gso_size: u16,
    pub csum_start: u16,
    pub csum_offset: u16,
    pub num_buffers: u16,
}

// =============================================================================
// VIRTIO-NET DEVICE (LEGACY)
// =============================================================================

/// Legacy virtio-net BAR0 registers
#[repr(C)]
struct LegacyRegs {
    /// Device features (read-only)
    device_features: u32,
    /// Guest features
    guest_features: u32,
    /// Queue address (page number)
    queue_address: u32,
    /// Queue size
    queue_size: u16,
    /// Queue select
    queue_select: u16,
    /// Queue notify
    queue_notify: u16,
    /// Device status
    device_status: u8,
    /// ISR status
    isr_status: u8,
}

/// Virtio-net device
pub struct VirtioNet {
    /// PCI device
    pci_device: PciDevice,
    /// Legacy registers base
    legacy_base: u16,
    /// RX queue
    rx_queue: Virtqueue,
    /// TX queue
    tx_queue: Virtqueue,
    /// MAC address
    mac: [u8; 6],
    /// Device is running
    running: bool,
}

impl VirtioNet {
    /// Create a new virtio-net device
    pub fn new(device: &PciDevice) -> Option<Self> {
        // Get I/O base from BAR0
        let io_base = match &device.bars[0] {
            Bar::Io { port, .. } => *port as u16,
            _ => return None,
        };

        let mut net = Self {
            pci_device: device.clone(),
            legacy_base: io_base,
            rx_queue: Virtqueue::new(),
            tx_queue: Virtqueue::new(),
            mac: [0; 6],
            running: false,
        };

        // Initialize device
        if !net.init() {
            return None;
        }

        Some(net)
    }

    /// Read register
    fn read_reg<T>(&self, offset: u16) -> T
    where
        T: Copy,
    {
        unsafe {
            match mem::size_of::<T>() {
                1 => {
                    let v = inb(self.legacy_base + offset);
                    *(&v as *const u8 as *const T)
                }
                2 => {
                    let v = inw(self.legacy_base + offset);
                    *(&v as *const u16 as *const T)
                }
                4 => {
                    let v = inl(self.legacy_base + offset);
                    *(&v as *const u32 as *const T)
                }
                _ => panic!("Invalid register size"),
            }
        }
    }

    /// Write register
    fn write_reg<T>(&self, offset: u16, value: T)
    where
        T: Copy,
    {
        unsafe {
            match mem::size_of::<T>() {
                1 => outb(self.legacy_base + offset, *(&value as *const T as *const u8)),
                2 => outw(self.legacy_base + offset, *(&value as *const T as *const u16)),
                4 => outl(self.legacy_base + offset, *(&value as *const T as *const u32)),
                _ => panic!("Invalid register size"),
            }
        }
    }

    /// Initialize device
    fn init(&mut self) -> bool {
        // Reset device
        self.write_reg::<u8>(18, 0);

        // Acknowledge device
        self.write_reg::<u8>(18, VIRTIO_STATUS_ACKNOWLEDGE);

        // Driver loaded
        self.write_reg::<u8>(18, VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER);

        // Read device features
        let features: u32 = self.read_reg(0);

        // Negotiate features (just use MAC feature for now)
        let our_features = features & (VIRTIO_NET_F_MAC as u32);
        self.write_reg::<u32>(4, our_features);

        // Read MAC address
        for i in 0..6 {
            self.mac[i] = self.read_reg::<u8>(20 + i as u16);
        }

        // Set up RX queue
        self.write_reg::<u16>(14, RX_QUEUE);
        let rx_size: u16 = self.read_reg(12);
        if rx_size == 0 {
            return false;
        }

        // Calculate queue page
        let rx_addr = self.rx_queue.desc_addr() / 4096;
        self.write_reg::<u32>(8, rx_addr as u32);

        // Set up TX queue
        self.write_reg::<u16>(14, TX_QUEUE);
        let tx_size: u16 = self.read_reg(12);
        if tx_size == 0 {
            return false;
        }

        let tx_addr = self.tx_queue.desc_addr() / 4096;
        self.write_reg::<u32>(8, tx_addr as u32);

        // Mark driver ready
        self.write_reg::<u8>(18,
            VIRTIO_STATUS_ACKNOWLEDGE |
            VIRTIO_STATUS_DRIVER |
            VIRTIO_STATUS_DRIVER_OK
        );

        // Fill RX queue with buffers
        for _ in 0..QUEUE_SIZE {
            let buf = vec![0u8; MAX_PACKET_SIZE + VIRTIO_NET_HDR_SIZE].into_boxed_slice();
            if self.rx_queue.add_buf_recv(buf).is_none() {
                break;
            }
        }

        // Notify RX queue
        self.write_reg::<u16>(16, RX_QUEUE);

        self.running = true;
        true
    }

    /// Transmit a packet
    pub fn transmit(&mut self, data: &[u8]) -> Result<(), &'static str> {
        if !self.running {
            return Err("Device not running");
        }

        if data.len() > MAX_PACKET_SIZE {
            return Err("Packet too large");
        }

        // Create virtio net header
        let header = VirtioNetHdr::default();
        let header_bytes = unsafe {
            core::slice::from_raw_parts(
                &header as *const _ as *const u8,
                VIRTIO_NET_HDR_SIZE
            )
        };

        // Add to TX queue
        self.tx_queue.add_buf_send(header_bytes, data)
            .ok_or("TX queue full")?;

        // Notify device
        self.write_reg::<u16>(16, TX_QUEUE);

        Ok(())
    }

    /// Receive a packet
    pub fn receive(&mut self) -> Option<Vec<u8>> {
        if !self.running {
            return None;
        }

        // Get completed buffer from RX queue
        let (_, buf, len) = self.rx_queue.get_buf()?;

        // Extract packet data (skip virtio header)
        let packet_len = (len as usize).saturating_sub(VIRTIO_NET_HDR_SIZE);
        if packet_len == 0 {
            return None;
        }

        let packet = buf[VIRTIO_NET_HDR_SIZE..VIRTIO_NET_HDR_SIZE + packet_len].to_vec();

        // Refill RX queue
        let new_buf = vec![0u8; MAX_PACKET_SIZE + VIRTIO_NET_HDR_SIZE].into_boxed_slice();
        self.rx_queue.add_buf_recv(new_buf);
        self.write_reg::<u16>(16, RX_QUEUE);

        Some(packet)
    }

    /// Get MAC address
    pub fn mac(&self) -> [u8; 6] {
        self.mac
    }

    /// Get MAC address as string
    pub fn mac_str(&self) -> alloc::string::String {
        use alloc::format;
        format!("{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.mac[0], self.mac[1], self.mac[2],
            self.mac[3], self.mac[4], self.mac[5])
    }

    /// Handle interrupt
    pub fn handle_interrupt(&mut self) {
        // Read ISR status to acknowledge interrupt
        let _isr: u8 = self.read_reg(19);

        // Process completed TX buffers
        while let Some((_, _, _)) = self.tx_queue.get_buf() {
            // Buffer freed automatically
        }
    }
}

// =============================================================================
// ETHERNET FRAME
// =============================================================================

/// Ethernet header
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct EthernetHeader {
    pub dst: [u8; 6],
    pub src: [u8; 6],
    pub ethertype: u16,
}

/// Common ethertypes
pub const ETHERTYPE_IPV4: u16 = 0x0800;
pub const ETHERTYPE_ARP: u16 = 0x0806;
pub const ETHERTYPE_IPV6: u16 = 0x86DD;

/// Parse ethernet header
pub fn parse_ethernet(data: &[u8]) -> Option<(&EthernetHeader, &[u8])> {
    if data.len() < ETH_HEADER_SIZE {
        return None;
    }

    let header = unsafe { &*(data.as_ptr() as *const EthernetHeader) };
    let payload = &data[ETH_HEADER_SIZE..];

    Some((header, payload))
}

// =============================================================================
// GLOBAL STATE
// =============================================================================

/// Global network devices
static NETWORK_DEVICES: Mutex<Vec<VirtioNet>> = Mutex::new(Vec::new());

/// Initialize network subsystem
pub fn init() {
    let mut devices = NETWORK_DEVICES.lock();

    // Find virtio-net devices
    let pci_devices = pci::devices();

    for device in pci_devices {
        if device.vendor_id == VIRTIO_VENDOR_ID {
            match device.device_id {
                VIRTIO_NET_DEVICE_ID | VIRTIO_NET_MODERN_DEVICE_ID => {
                    if let Some(net) = VirtioNet::new(&device) {
                        crate::kprintln!("[NET] virtio-net: MAC {}", net.mac_str());
                        devices.push(net);
                    }
                }
                _ => {}
            }
        }
    }

    crate::kprintln!("[NET] {} network device(s) found", devices.len());
}

/// Transmit packet on first device
pub fn transmit(data: &[u8]) -> Result<(), &'static str> {
    let mut devices = NETWORK_DEVICES.lock();
    if let Some(device) = devices.first_mut() {
        device.transmit(data)
    } else {
        Err("No network device")
    }
}

/// Receive packet from first device
pub fn receive() -> Option<Vec<u8>> {
    let mut devices = NETWORK_DEVICES.lock();
    devices.first_mut().and_then(|d| d.receive())
}

/// Get MAC address of first device
pub fn get_mac() -> Option<[u8; 6]> {
    NETWORK_DEVICES.lock().first().map(|d| d.mac())
}

/// Get MAC address as byte array (alias for get_mac)
pub fn mac_address() -> [u8; 6] {
    get_mac().unwrap_or([0; 6])
}

/// Get device count
pub fn device_count() -> usize {
    NETWORK_DEVICES.lock().len()
}

/// Check if any network device is available
pub fn is_available() -> bool {
    !NETWORK_DEVICES.lock().is_empty()
}

/// Get MAC address as formatted string
pub fn mac_address_string() -> alloc::string::String {
    use alloc::format;
    if let Some(mac) = get_mac() {
        format!("{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5])
    } else {
        alloc::string::String::from("--:--:--:--:--:--")
    }
}

// =============================================================================
// I/O HELPERS
// =============================================================================

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    core::arch::asm!(
        "in al, dx",
        in("dx") port,
        out("al") value,
        options(nostack, nomem)
    );
    value
}

#[inline]
unsafe fn inw(port: u16) -> u16 {
    let value: u16;
    core::arch::asm!(
        "in ax, dx",
        in("dx") port,
        out("ax") value,
        options(nostack, nomem)
    );
    value
}

#[inline]
unsafe fn inl(port: u16) -> u32 {
    let value: u32;
    core::arch::asm!(
        "in eax, dx",
        in("dx") port,
        out("eax") value,
        options(nostack, nomem)
    );
    value
}

#[inline]
unsafe fn outb(port: u16, value: u8) {
    core::arch::asm!(
        "out dx, al",
        in("dx") port,
        in("al") value,
        options(nostack, nomem)
    );
}

#[inline]
unsafe fn outw(port: u16, value: u16) {
    core::arch::asm!(
        "out dx, ax",
        in("dx") port,
        in("ax") value,
        options(nostack, nomem)
    );
}

#[inline]
unsafe fn outl(port: u16, value: u32) {
    core::arch::asm!(
        "out dx, eax",
        in("dx") port,
        in("eax") value,
        options(nostack, nomem)
    );
}
