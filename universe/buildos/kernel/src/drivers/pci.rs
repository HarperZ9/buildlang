// ===============================================================================
// BUILDOS KERNEL - PCI BUS DRIVER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! PCI (Peripheral Component Interconnect) bus enumeration and management.
//!
//! Supports:
//! - Legacy PCI configuration space access (I/O ports)
//! - PCIe ECAM (Enhanced Configuration Access Mechanism)
//! - Device enumeration and capability parsing
//! - MSI/MSI-X interrupt configuration

use alloc::vec::Vec;
use core::fmt;
use spin::Mutex;

// =============================================================================
// CONFIGURATION SPACE ACCESS
// =============================================================================

/// PCI configuration address port
const PCI_CONFIG_ADDRESS: u16 = 0xCF8;

/// PCI configuration data port
const PCI_CONFIG_DATA: u16 = 0xCFC;

/// Configuration address register format
const CONFIG_ENABLE: u32 = 1 << 31;

// =============================================================================
// CONFIGURATION SPACE REGISTERS
// =============================================================================

/// Vendor ID register (16-bit)
const PCI_VENDOR_ID: u8 = 0x00;

/// Device ID register (16-bit)
const PCI_DEVICE_ID: u8 = 0x02;

/// Command register (16-bit)
const PCI_COMMAND: u8 = 0x04;

/// Status register (16-bit)
const PCI_STATUS: u8 = 0x06;

/// Revision ID register (8-bit)
const PCI_REVISION_ID: u8 = 0x08;

/// Programming interface (8-bit)
const PCI_PROG_IF: u8 = 0x09;

/// Subclass code (8-bit)
const PCI_SUBCLASS: u8 = 0x0A;

/// Class code (8-bit)
const PCI_CLASS: u8 = 0x0B;

/// Cache line size (8-bit)
const PCI_CACHE_LINE_SIZE: u8 = 0x0C;

/// Latency timer (8-bit)
const PCI_LATENCY_TIMER: u8 = 0x0D;

/// Header type (8-bit)
const PCI_HEADER_TYPE: u8 = 0x0E;

/// BIST register (8-bit)
const PCI_BIST: u8 = 0x0F;

/// Base Address Registers (6 x 32-bit for type 0)
const PCI_BAR0: u8 = 0x10;
const PCI_BAR1: u8 = 0x14;
const PCI_BAR2: u8 = 0x18;
const PCI_BAR3: u8 = 0x1C;
const PCI_BAR4: u8 = 0x20;
const PCI_BAR5: u8 = 0x24;

/// Cardbus CIS pointer (type 0)
const PCI_CARDBUS_CIS: u8 = 0x28;

/// Subsystem vendor ID (type 0)
const PCI_SUBSYSTEM_VENDOR_ID: u8 = 0x2C;

/// Subsystem ID (type 0)
const PCI_SUBSYSTEM_ID: u8 = 0x2E;

/// Expansion ROM base address
const PCI_EXPANSION_ROM_BASE: u8 = 0x30;

/// Capabilities pointer
const PCI_CAPABILITIES_PTR: u8 = 0x34;

/// Interrupt line
const PCI_INTERRUPT_LINE: u8 = 0x3C;

/// Interrupt pin
const PCI_INTERRUPT_PIN: u8 = 0x3D;

/// Min grant
const PCI_MIN_GNT: u8 = 0x3E;

/// Max latency
const PCI_MAX_LAT: u8 = 0x3F;

// =============================================================================
// COMMAND REGISTER BITS
// =============================================================================

/// I/O space enable
const CMD_IO_SPACE: u16 = 1 << 0;

/// Memory space enable
const CMD_MEMORY_SPACE: u16 = 1 << 1;

/// Bus master enable
const CMD_BUS_MASTER: u16 = 1 << 2;

/// Special cycles
const CMD_SPECIAL_CYCLES: u16 = 1 << 3;

/// Memory write and invalidate
const CMD_MWI: u16 = 1 << 4;

/// VGA palette snoop
const CMD_VGA_PALETTE: u16 = 1 << 5;

/// Parity error response
const CMD_PARITY_ERROR: u16 = 1 << 6;

/// SERR# enable
const CMD_SERR: u16 = 1 << 8;

/// Fast back-to-back enable
const CMD_FAST_B2B: u16 = 1 << 9;

/// Interrupt disable
const CMD_INTERRUPT_DISABLE: u16 = 1 << 10;

// =============================================================================
// STATUS REGISTER BITS
// =============================================================================

/// Capabilities list present
const STATUS_CAPABILITIES: u16 = 1 << 4;

/// 66 MHz capable
const STATUS_66MHZ: u16 = 1 << 5;

/// Fast back-to-back capable
const STATUS_FAST_B2B: u16 = 1 << 7;

/// Master data parity error
const STATUS_PARITY_ERROR: u16 = 1 << 8;

/// DEVSEL timing
const STATUS_DEVSEL_MASK: u16 = 3 << 9;

/// Signaled target abort
const STATUS_SIG_TARGET_ABORT: u16 = 1 << 11;

/// Received target abort
const STATUS_RCV_TARGET_ABORT: u16 = 1 << 12;

/// Received master abort
const STATUS_RCV_MASTER_ABORT: u16 = 1 << 13;

/// Signaled system error
const STATUS_SIG_SYSTEM_ERROR: u16 = 1 << 14;

/// Detected parity error
const STATUS_DETECTED_PARITY_ERROR: u16 = 1 << 15;

// =============================================================================
// CAPABILITY IDS
// =============================================================================

/// Power Management
const CAP_PM: u8 = 0x01;

/// AGP
const CAP_AGP: u8 = 0x02;

/// Vital Product Data
const CAP_VPD: u8 = 0x03;

/// Slot Identification
const CAP_SLOT_ID: u8 = 0x04;

/// MSI
const CAP_MSI: u8 = 0x05;

/// CompactPCI Hot Swap
const CAP_CHSWP: u8 = 0x06;

/// PCI-X
const CAP_PCIX: u8 = 0x07;

/// HyperTransport
const CAP_HT: u8 = 0x08;

/// Vendor Specific
const CAP_VENDOR: u8 = 0x09;

/// Debug Port
const CAP_DEBUG: u8 = 0x0A;

/// CompactPCI Central Resource Control
const CAP_CCRC: u8 = 0x0B;

/// PCI Hot Plug
const CAP_HOTPLUG: u8 = 0x0C;

/// PCI Bridge Subsystem Vendor ID
const CAP_SSVID: u8 = 0x0D;

/// AGP 8x
const CAP_AGP8X: u8 = 0x0E;

/// Secure Device
const CAP_SECURE: u8 = 0x0F;

/// PCI Express
const CAP_PCIE: u8 = 0x10;

/// MSI-X
const CAP_MSIX: u8 = 0x11;

/// SATA Data/Index Configuration
const CAP_SATA: u8 = 0x12;

/// Advanced Features
const CAP_AF: u8 = 0x13;

// =============================================================================
// DEVICE CLASSES
// =============================================================================

/// Device class codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DeviceClass {
    Unclassified = 0x00,
    MassStorage = 0x01,
    Network = 0x02,
    Display = 0x03,
    Multimedia = 0x04,
    Memory = 0x05,
    Bridge = 0x06,
    SimpleCommunication = 0x07,
    BaseSystemPeripheral = 0x08,
    InputDevice = 0x09,
    DockingStation = 0x0A,
    Processor = 0x0B,
    SerialBus = 0x0C,
    Wireless = 0x0D,
    IntelligentController = 0x0E,
    SatelliteCommunication = 0x0F,
    Encryption = 0x10,
    SignalProcessing = 0x11,
    ProcessingAccelerator = 0x12,
    NonEssentialInstrumentation = 0x13,
    Unknown = 0xFF,
}

impl From<u8> for DeviceClass {
    fn from(value: u8) -> Self {
        match value {
            0x00 => Self::Unclassified,
            0x01 => Self::MassStorage,
            0x02 => Self::Network,
            0x03 => Self::Display,
            0x04 => Self::Multimedia,
            0x05 => Self::Memory,
            0x06 => Self::Bridge,
            0x07 => Self::SimpleCommunication,
            0x08 => Self::BaseSystemPeripheral,
            0x09 => Self::InputDevice,
            0x0A => Self::DockingStation,
            0x0B => Self::Processor,
            0x0C => Self::SerialBus,
            0x0D => Self::Wireless,
            0x0E => Self::IntelligentController,
            0x0F => Self::SatelliteCommunication,
            0x10 => Self::Encryption,
            0x11 => Self::SignalProcessing,
            0x12 => Self::ProcessingAccelerator,
            0x13 => Self::NonEssentialInstrumentation,
            _ => Self::Unknown,
        }
    }
}

// =============================================================================
// BAR (BASE ADDRESS REGISTER)
// =============================================================================

/// Base Address Register type
#[derive(Debug, Clone, Copy)]
pub enum Bar {
    /// Memory-mapped BAR
    Memory {
        address: u64,
        size: u64,
        prefetchable: bool,
        is_64bit: bool,
    },
    /// I/O port BAR
    Io {
        port: u32,
        size: u32,
    },
    /// Unused BAR
    Unused,
}

impl Bar {
    /// Parse a BAR from its raw value
    fn parse(device: &PciDevice, bar_index: usize) -> (Self, bool) {
        let offset = PCI_BAR0 + (bar_index as u8 * 4);
        let bar_value = device.read_config_u32(offset);

        if bar_value == 0 {
            return (Bar::Unused, false);
        }

        let is_io = (bar_value & 1) != 0;

        if is_io {
            // I/O BAR
            let port = bar_value & !0x03;

            // Determine size
            device.write_config_u32(offset, 0xFFFFFFFF);
            let size_mask = device.read_config_u32(offset);
            device.write_config_u32(offset, bar_value);

            let size = !((size_mask & !0x03) | 0x03).wrapping_add(1);

            (Bar::Io { port, size }, false)
        } else {
            // Memory BAR
            let bar_type = (bar_value >> 1) & 0x03;
            let prefetchable = (bar_value & 0x08) != 0;
            let is_64bit = bar_type == 2;

            let address = if is_64bit {
                let high_offset = offset + 4;
                let high = device.read_config_u32(high_offset) as u64;
                (high << 32) | ((bar_value & !0x0F) as u64)
            } else {
                (bar_value & !0x0F) as u64
            };

            // Determine size
            device.write_config_u32(offset, 0xFFFFFFFF);
            let mut size_mask = device.read_config_u32(offset) as u64;
            device.write_config_u32(offset, bar_value);

            if is_64bit {
                let high_offset = offset + 4;
                let high_value = device.read_config_u32(high_offset);
                device.write_config_u32(high_offset, 0xFFFFFFFF);
                let high_mask = device.read_config_u32(high_offset) as u64;
                device.write_config_u32(high_offset, high_value);
                size_mask |= high_mask << 32;
            }

            let size = !((size_mask & !0x0F) | 0x0F).wrapping_add(1);

            (Bar::Memory {
                address,
                size,
                prefetchable,
                is_64bit,
            }, is_64bit)
        }
    }
}

// =============================================================================
// PCI DEVICE
// =============================================================================

/// PCI device location
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciLocation {
    pub segment: u16,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

impl PciLocation {
    pub fn new(bus: u8, device: u8, function: u8) -> Self {
        Self {
            segment: 0,
            bus,
            device,
            function,
        }
    }

    /// Create configuration address for legacy I/O access
    fn config_address(&self, offset: u8) -> u32 {
        CONFIG_ENABLE
            | ((self.bus as u32) << 16)
            | ((self.device as u32) << 11)
            | ((self.function as u32) << 8)
            | ((offset as u32) & 0xFC)
    }
}

impl fmt::Display for PciLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04x}:{:02x}:{:02x}.{}",
            self.segment, self.bus, self.device, self.function)
    }
}

/// PCI capability
#[derive(Debug, Clone)]
pub struct PciCapability {
    /// Capability ID
    pub id: u8,
    /// Offset in configuration space
    pub offset: u8,
}

/// PCI device
#[derive(Clone)]
pub struct PciDevice {
    /// Device location
    pub location: PciLocation,
    /// Vendor ID
    pub vendor_id: u16,
    /// Device ID
    pub device_id: u16,
    /// Class code
    pub class: DeviceClass,
    /// Subclass code
    pub subclass: u8,
    /// Programming interface
    pub prog_if: u8,
    /// Revision ID
    pub revision: u8,
    /// Header type
    pub header_type: u8,
    /// Subsystem vendor ID
    pub subsystem_vendor_id: u16,
    /// Subsystem ID
    pub subsystem_id: u16,
    /// Interrupt line
    pub interrupt_line: u8,
    /// Interrupt pin
    pub interrupt_pin: u8,
    /// Base Address Registers
    pub bars: [Bar; 6],
    /// Capabilities
    pub capabilities: Vec<PciCapability>,
    /// ECAM base address (if PCIe)
    ecam_base: Option<u64>,
}

impl PciDevice {
    /// Read 8-bit value from configuration space
    pub fn read_config_u8(&self, offset: u8) -> u8 {
        let value = self.read_config_u32(offset & 0xFC);
        ((value >> ((offset & 3) * 8)) & 0xFF) as u8
    }

    /// Read 16-bit value from configuration space
    pub fn read_config_u16(&self, offset: u8) -> u16 {
        let value = self.read_config_u32(offset & 0xFC);
        ((value >> ((offset & 2) * 8)) & 0xFFFF) as u16
    }

    /// Read 32-bit value from configuration space
    pub fn read_config_u32(&self, offset: u8) -> u32 {
        if let Some(ecam) = self.ecam_base {
            // PCIe ECAM access
            let addr = ecam
                + ((self.location.bus as u64) << 20)
                + ((self.location.device as u64) << 15)
                + ((self.location.function as u64) << 12)
                + (offset as u64);
            unsafe { core::ptr::read_volatile(addr as *const u32) }
        } else {
            // Legacy I/O access
            unsafe {
                let addr = self.location.config_address(offset);
                outl(PCI_CONFIG_ADDRESS, addr);
                inl(PCI_CONFIG_DATA)
            }
        }
    }

    /// Write 8-bit value to configuration space
    pub fn write_config_u8(&self, offset: u8, value: u8) {
        let old = self.read_config_u32(offset & 0xFC);
        let shift = (offset & 3) * 8;
        let mask = !(0xFF << shift);
        let new = (old & mask) | ((value as u32) << shift);
        self.write_config_u32(offset & 0xFC, new);
    }

    /// Write 16-bit value to configuration space
    pub fn write_config_u16(&self, offset: u8, value: u16) {
        let old = self.read_config_u32(offset & 0xFC);
        let shift = (offset & 2) * 8;
        let mask = !(0xFFFF << shift);
        let new = (old & mask) | ((value as u32) << shift);
        self.write_config_u32(offset & 0xFC, new);
    }

    /// Write 32-bit value to configuration space
    pub fn write_config_u32(&self, offset: u8, value: u32) {
        if let Some(ecam) = self.ecam_base {
            // PCIe ECAM access
            let addr = ecam
                + ((self.location.bus as u64) << 20)
                + ((self.location.device as u64) << 15)
                + ((self.location.function as u64) << 12)
                + (offset as u64);
            unsafe { core::ptr::write_volatile(addr as *mut u32, value) }
        } else {
            // Legacy I/O access
            unsafe {
                let addr = self.location.config_address(offset);
                outl(PCI_CONFIG_ADDRESS, addr);
                outl(PCI_CONFIG_DATA, value);
            }
        }
    }

    /// Enable bus mastering
    pub fn enable_bus_master(&self) {
        let cmd = self.read_config_u16(PCI_COMMAND);
        self.write_config_u16(PCI_COMMAND, cmd | CMD_BUS_MASTER);
    }

    /// Enable memory space access
    pub fn enable_memory_space(&self) {
        let cmd = self.read_config_u16(PCI_COMMAND);
        self.write_config_u16(PCI_COMMAND, cmd | CMD_MEMORY_SPACE);
    }

    /// Enable I/O space access
    pub fn enable_io_space(&self) {
        let cmd = self.read_config_u16(PCI_COMMAND);
        self.write_config_u16(PCI_COMMAND, cmd | CMD_IO_SPACE);
    }

    /// Disable interrupts
    pub fn disable_interrupts(&self) {
        let cmd = self.read_config_u16(PCI_COMMAND);
        self.write_config_u16(PCI_COMMAND, cmd | CMD_INTERRUPT_DISABLE);
    }

    /// Find capability by ID
    pub fn find_capability(&self, cap_id: u8) -> Option<&PciCapability> {
        self.capabilities.iter().find(|c| c.id == cap_id)
    }

    /// Check if device has MSI capability
    pub fn has_msi(&self) -> bool {
        self.find_capability(CAP_MSI).is_some()
    }

    /// Check if device has MSI-X capability
    pub fn has_msix(&self) -> bool {
        self.find_capability(CAP_MSIX).is_some()
    }

    /// Configure MSI
    pub fn configure_msi(&self, vector: u8, processor: u8) -> bool {
        let Some(cap) = self.find_capability(CAP_MSI) else {
            return false;
        };

        let msg_ctrl = self.read_config_u16(cap.offset + 2);
        let is_64bit = (msg_ctrl & (1 << 7)) != 0;

        // Message address (local APIC)
        let msg_addr: u32 = 0xFEE00000 | ((processor as u32) << 12);
        self.write_config_u32(cap.offset + 4, msg_addr);

        // Message data
        let data_offset = if is_64bit {
            self.write_config_u32(cap.offset + 8, 0);
            cap.offset + 12
        } else {
            cap.offset + 8
        };

        self.write_config_u16(data_offset, vector as u16);

        // Enable MSI (bit 0 of message control)
        self.write_config_u16(cap.offset + 2, msg_ctrl | 1);

        true
    }

    /// Get device description
    pub fn description(&self) -> &'static str {
        match (self.class, self.subclass) {
            (DeviceClass::MassStorage, 0x01) => "IDE Controller",
            (DeviceClass::MassStorage, 0x06) => "SATA Controller",
            (DeviceClass::MassStorage, 0x08) => "NVMe Controller",
            (DeviceClass::Network, 0x00) => "Ethernet Controller",
            (DeviceClass::Network, 0x80) => "Network Controller",
            (DeviceClass::Display, 0x00) => "VGA Controller",
            (DeviceClass::Display, 0x02) => "3D Controller",
            (DeviceClass::Multimedia, 0x01) => "Audio Device",
            (DeviceClass::Multimedia, 0x03) => "HD Audio Controller",
            (DeviceClass::Bridge, 0x00) => "Host Bridge",
            (DeviceClass::Bridge, 0x01) => "ISA Bridge",
            (DeviceClass::Bridge, 0x04) => "PCI Bridge",
            (DeviceClass::Bridge, 0x06) => "PCIe Root Port",
            (DeviceClass::SerialBus, 0x03) => "USB Controller",
            (DeviceClass::SerialBus, 0x05) => "SD/MMC Controller",
            _ => "Unknown Device",
        }
    }
}

impl fmt::Debug for PciDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} [{:04x}:{:04x}] {} - {}",
            self.location,
            self.vendor_id,
            self.device_id,
            self.description(),
            match self.class {
                DeviceClass::MassStorage => "Storage",
                DeviceClass::Network => "Network",
                DeviceClass::Display => "Display",
                DeviceClass::Bridge => "Bridge",
                _ => "Other",
            }
        )
    }
}

// =============================================================================
// PCI BUS
// =============================================================================

/// PCI bus manager
pub struct PciBus {
    /// Discovered devices
    devices: Vec<PciDevice>,
    /// ECAM base address for PCIe
    ecam_base: Option<u64>,
}

impl PciBus {
    /// Create a new PCI bus manager
    pub const fn new() -> Self {
        Self {
            devices: Vec::new(),
            ecam_base: None,
        }
    }

    /// Set ECAM base address
    pub fn set_ecam_base(&mut self, base: u64) {
        self.ecam_base = Some(base);
    }

    /// Enumerate all PCI devices
    pub fn enumerate(&mut self) {
        self.devices.clear();

        // Check if PCI is available
        if !self.check_pci_available() {
            return;
        }

        // Enumerate all buses
        for bus in 0..=255u8 {
            self.enumerate_bus(bus);
        }

        crate::kprintln!("[PCI] Found {} devices", self.devices.len());
    }

    /// Check if PCI is available
    fn check_pci_available(&self) -> bool {
        unsafe {
            outl(PCI_CONFIG_ADDRESS, CONFIG_ENABLE);
            let value = inl(PCI_CONFIG_ADDRESS);
            value == CONFIG_ENABLE
        }
    }

    /// Enumerate a single bus
    fn enumerate_bus(&mut self, bus: u8) {
        for device in 0..32u8 {
            self.enumerate_device(bus, device);
        }
    }

    /// Enumerate a single device (all functions)
    fn enumerate_device(&mut self, bus: u8, device: u8) {
        let location = PciLocation::new(bus, device, 0);

        // Check if device exists
        let vendor_id = self.read_vendor_id(location);
        if vendor_id == 0xFFFF {
            return;
        }

        // Check function 0
        self.enumerate_function(bus, device, 0);

        // Check if multi-function device
        let header_type = self.read_header_type(location);
        if (header_type & 0x80) != 0 {
            // Multi-function device
            for function in 1..8u8 {
                let location = PciLocation::new(bus, device, function);
                if self.read_vendor_id(location) != 0xFFFF {
                    self.enumerate_function(bus, device, function);
                }
            }
        }
    }

    /// Enumerate a single function
    fn enumerate_function(&mut self, bus: u8, device_num: u8, function: u8) {
        let location = PciLocation::new(bus, device_num, function);

        let vendor_id = self.read_vendor_id(location);
        let device_id = self.read_device_id(location);
        let class = self.read_class(location);
        let subclass = self.read_subclass(location);
        let prog_if = self.read_prog_if(location);
        let revision = self.read_revision(location);
        let header_type = self.read_header_type(location) & 0x7F;
        let status = self.read_status(location);

        // Create device
        let mut dev = PciDevice {
            location,
            vendor_id,
            device_id,
            class: DeviceClass::from(class),
            subclass,
            prog_if,
            revision,
            header_type,
            subsystem_vendor_id: 0,
            subsystem_id: 0,
            interrupt_line: 0,
            interrupt_pin: 0,
            bars: [Bar::Unused; 6],
            capabilities: Vec::new(),
            ecam_base: self.ecam_base,
        };

        // Parse type 0 header (normal device)
        if header_type == 0 {
            dev.subsystem_vendor_id = dev.read_config_u16(PCI_SUBSYSTEM_VENDOR_ID);
            dev.subsystem_id = dev.read_config_u16(PCI_SUBSYSTEM_ID);
            dev.interrupt_line = dev.read_config_u8(PCI_INTERRUPT_LINE);
            dev.interrupt_pin = dev.read_config_u8(PCI_INTERRUPT_PIN);

            // Parse BARs
            let mut i = 0;
            while i < 6 {
                let (bar, is_64bit) = Bar::parse(&dev, i);
                dev.bars[i] = bar;
                if is_64bit {
                    i += 1; // Skip next BAR (upper 32 bits)
                    dev.bars[i] = Bar::Unused;
                }
                i += 1;
            }

            // Parse capabilities
            if (status & STATUS_CAPABILITIES) != 0 {
                self.parse_capabilities(&mut dev);
            }
        }

        self.devices.push(dev);
    }

    /// Parse device capabilities
    fn parse_capabilities(&self, device: &mut PciDevice) {
        let mut offset = device.read_config_u8(PCI_CAPABILITIES_PTR) & 0xFC;

        while offset != 0 {
            let cap_id = device.read_config_u8(offset);
            let next = device.read_config_u8(offset + 1);

            device.capabilities.push(PciCapability {
                id: cap_id,
                offset,
            });

            offset = next & 0xFC;
        }
    }

    /// Read vendor ID
    fn read_vendor_id(&self, location: PciLocation) -> u16 {
        self.read_config_u16(location, PCI_VENDOR_ID)
    }

    /// Read device ID
    fn read_device_id(&self, location: PciLocation) -> u16 {
        self.read_config_u16(location, PCI_DEVICE_ID)
    }

    /// Read class code
    fn read_class(&self, location: PciLocation) -> u8 {
        self.read_config_u8(location, PCI_CLASS)
    }

    /// Read subclass code
    fn read_subclass(&self, location: PciLocation) -> u8 {
        self.read_config_u8(location, PCI_SUBCLASS)
    }

    /// Read programming interface
    fn read_prog_if(&self, location: PciLocation) -> u8 {
        self.read_config_u8(location, PCI_PROG_IF)
    }

    /// Read revision ID
    fn read_revision(&self, location: PciLocation) -> u8 {
        self.read_config_u8(location, PCI_REVISION_ID)
    }

    /// Read header type
    fn read_header_type(&self, location: PciLocation) -> u8 {
        self.read_config_u8(location, PCI_HEADER_TYPE)
    }

    /// Read status register
    fn read_status(&self, location: PciLocation) -> u16 {
        self.read_config_u16(location, PCI_STATUS)
    }

    /// Read 8-bit config value
    fn read_config_u8(&self, location: PciLocation, offset: u8) -> u8 {
        let value = self.read_config_u32(location, offset & 0xFC);
        ((value >> ((offset & 3) * 8)) & 0xFF) as u8
    }

    /// Read 16-bit config value
    fn read_config_u16(&self, location: PciLocation, offset: u8) -> u16 {
        let value = self.read_config_u32(location, offset & 0xFC);
        ((value >> ((offset & 2) * 8)) & 0xFFFF) as u16
    }

    /// Read 32-bit config value
    fn read_config_u32(&self, location: PciLocation, offset: u8) -> u32 {
        if let Some(ecam) = self.ecam_base {
            let addr = ecam
                + ((location.bus as u64) << 20)
                + ((location.device as u64) << 15)
                + ((location.function as u64) << 12)
                + (offset as u64);
            unsafe { core::ptr::read_volatile(addr as *const u32) }
        } else {
            unsafe {
                let addr = location.config_address(offset);
                outl(PCI_CONFIG_ADDRESS, addr);
                inl(PCI_CONFIG_DATA)
            }
        }
    }

    /// Write 16-bit config value
    fn write_config_u16(&self, location: PciLocation, offset: u8, value: u16) {
        // Read-modify-write for 16-bit access
        let aligned_offset = offset & 0xFC;
        let shift = (offset & 2) * 8;
        let mask = !(0xFFFF_u32 << shift);
        let old = self.read_config_u32(location, aligned_offset);
        let new_value = (old & mask) | ((value as u32) << shift);
        self.write_config_u32(location, aligned_offset, new_value);
    }

    /// Write 32-bit config value
    fn write_config_u32(&self, location: PciLocation, offset: u8, value: u32) {
        if let Some(ecam) = self.ecam_base {
            let addr = ecam
                + ((location.bus as u64) << 20)
                + ((location.device as u64) << 15)
                + ((location.function as u64) << 12)
                + (offset as u64);
            unsafe { core::ptr::write_volatile(addr as *mut u32, value) }
        } else {
            unsafe {
                let addr = location.config_address(offset);
                outl(PCI_CONFIG_ADDRESS, addr);
                outl(PCI_CONFIG_DATA, value);
            }
        }
    }

    /// Get all devices
    pub fn devices(&self) -> &[PciDevice] {
        &self.devices
    }

    /// Find devices by class
    pub fn find_by_class(&self, class: DeviceClass) -> Vec<&PciDevice> {
        self.devices.iter().filter(|d| d.class == class).collect()
    }

    /// Find devices by vendor and device ID
    pub fn find_by_id(&self, vendor_id: u16, device_id: u16) -> Option<&PciDevice> {
        self.devices.iter().find(|d| d.vendor_id == vendor_id && d.device_id == device_id)
    }

    /// Find device by location
    pub fn find_by_location(&self, bus: u8, device: u8, function: u8) -> Option<&PciDevice> {
        self.devices.iter().find(|d| {
            d.location.bus == bus &&
            d.location.device == device &&
            d.location.function == function
        })
    }
}

// =============================================================================
// GLOBAL STATE
// =============================================================================

/// Global PCI bus instance
static PCI_BUS: Mutex<PciBus> = Mutex::new(PciBus::new());

/// Initialize PCI subsystem
pub fn init() {
    let mut bus = PCI_BUS.lock();

    // Check for ECAM from ACPI
    if let Some(acpi_info) = super::acpi::get_info() {
        if let Some(ecam_base) = acpi_info.pcie_ecam_base {
            bus.set_ecam_base(ecam_base);
            crate::kprintln!("[PCI] Using PCIe ECAM at {:#x}", ecam_base);
        }
    }

    bus.enumerate();

    // Print discovered devices
    for device in bus.devices() {
        crate::kprintln!("[PCI] {:?}", device);
    }
}

/// Get all PCI devices
pub fn devices() -> Vec<PciDevice> {
    PCI_BUS.lock().devices().to_vec()
}

/// Find devices by class
pub fn find_by_class(class: DeviceClass) -> Vec<PciDevice> {
    PCI_BUS.lock().find_by_class(class).into_iter().cloned().collect()
}

/// Find device by vendor and device ID
pub fn find_by_id(vendor_id: u16, device_id: u16) -> Option<PciDevice> {
    PCI_BUS.lock().find_by_id(vendor_id, device_id).cloned()
}

/// Get number of discovered PCI devices
pub fn device_count() -> usize {
    PCI_BUS.lock().devices().len()
}

// =============================================================================
// I/O HELPERS
// =============================================================================

#[inline]
unsafe fn outl(port: u16, value: u32) {
    core::arch::asm!(
        "out dx, eax",
        in("dx") port,
        in("eax") value,
        options(nostack, nomem)
    );
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

// =============================================================================
// STANDALONE CONFIG ACCESS FUNCTIONS
// =============================================================================

/// Read 16-bit config value for a device
fn config_read16(bus: u8, slot: u8, func: u8, offset: u8) -> u16 {
    let location = PciLocation::new(bus, slot, func);
    PCI_BUS.lock().read_config_u16(location, offset)
}

/// Write 16-bit config value for a device
fn config_write16(bus: u8, slot: u8, func: u8, offset: u8, value: u16) {
    let location = PciLocation::new(bus, slot, func);
    PCI_BUS.lock().write_config_u16(location, offset, value);
}

/// Read 32-bit config value for a device
fn config_read32(bus: u8, slot: u8, func: u8, offset: u8) -> u32 {
    let location = PciLocation::new(bus, slot, func);
    PCI_BUS.lock().read_config_u32(location, offset)
}

// =============================================================================
// ADDITIONAL PUBLIC INTERFACE
// =============================================================================

/// Read a BAR (Base Address Register) value for a device
pub fn read_bar(bus: u8, slot: u8, func: u8, bar_index: u8) -> Option<u64> {
    if bar_index > 5 {
        return None;
    }

    let offset = PCI_BAR0 + (bar_index * 4);
    let value = config_read32(bus, slot, func, offset);

    if value == 0 || value == 0xFFFFFFFF {
        return None;
    }

    // Check if this is a 64-bit BAR (bit 2:1 = 10)
    if (value & 0b110) == 0b100 && bar_index < 5 {
        // 64-bit BAR - read the next one too
        let high = config_read32(bus, slot, func, offset + 4);
        Some((value & 0xFFFFFFF0) as u64 | ((high as u64) << 32))
    } else {
        // 32-bit BAR or I/O BAR
        if value & 0x1 != 0 {
            // I/O BAR
            Some((value & 0xFFFFFFFC) as u64)
        } else {
            // Memory BAR
            Some((value & 0xFFFFFFF0) as u64)
        }
    }
}

/// Get all discovered PCI devices in tuple format
/// Returns (bus, slot, func, vendor_id, device_id, class, subclass, prog_if)
pub fn get_devices() -> Vec<(u8, u8, u8, u16, u16, u8, u8, u8)> {
    PCI_BUS.lock().devices().iter().map(|dev| {
        (dev.location.bus, dev.location.device, dev.location.function,
         dev.vendor_id, dev.device_id, dev.class as u8, dev.subclass, dev.prog_if)
    }).collect()
}

/// Enable bus mastering for a device
pub fn enable_bus_master(bus: u8, slot: u8, func: u8) {
    let cmd = config_read16(bus, slot, func, PCI_COMMAND);
    config_write16(bus, slot, func, PCI_COMMAND, cmd | 0x04);
}

/// Enable memory space access for a device
pub fn enable_memory_space(bus: u8, slot: u8, func: u8) {
    let cmd = config_read16(bus, slot, func, PCI_COMMAND);
    config_write16(bus, slot, func, PCI_COMMAND, cmd | 0x02);
}

/// Enable I/O space access for a device
pub fn enable_io_space(bus: u8, slot: u8, func: u8) {
    let cmd = config_read16(bus, slot, func, PCI_COMMAND);
    config_write16(bus, slot, func, PCI_COMMAND, cmd | 0x01);
}
