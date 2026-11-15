// ===============================================================================
// BUILDOS KERNEL - STORAGE DRIVERS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! Storage device drivers (AHCI/SATA).
//!
//! Implements AHCI (Advanced Host Controller Interface) for SATA devices.

use alloc::boxed::Box;
use alloc::vec::Vec;
use spin::Mutex;

use super::pci::{self, DeviceClass, PciDevice, Bar};

// =============================================================================
// AHCI CONSTANTS
// =============================================================================

/// AHCI class code
const AHCI_CLASS: u8 = 0x01;
const AHCI_SUBCLASS: u8 = 0x06;
const AHCI_PROG_IF: u8 = 0x01;

/// Maximum number of ports
const MAX_PORTS: usize = 32;

/// Maximum commands per port
const MAX_COMMANDS: usize = 32;

/// Sector size
const SECTOR_SIZE: usize = 512;

// =============================================================================
// HBA MEMORY REGISTERS
// =============================================================================

/// AHCI Host Bus Adapter memory-mapped registers
#[repr(C)]
pub struct HbaMemory {
    /// Host Capabilities
    pub cap: u32,
    /// Global Host Control
    pub ghc: u32,
    /// Interrupt Status
    pub is: u32,
    /// Ports Implemented
    pub pi: u32,
    /// Version
    pub vs: u32,
    /// Command Completion Coalescing Control
    pub ccc_ctl: u32,
    /// Command Completion Coalescing Ports
    pub ccc_ports: u32,
    /// Enclosure Management Location
    pub em_loc: u32,
    /// Enclosure Management Control
    pub em_ctl: u32,
    /// Host Capabilities Extended
    pub cap2: u32,
    /// BIOS/OS Handoff Control and Status
    pub bohc: u32,
    /// Reserved
    _reserved: [u8; 0xA0 - 0x2C],
    /// Vendor Specific
    _vendor: [u8; 0x100 - 0xA0],
    /// Port registers (32 ports max)
    pub ports: [HbaPort; MAX_PORTS],
}

/// Global Host Control bits
const GHC_HR: u32 = 1 << 0;      // HBA Reset
const GHC_IE: u32 = 1 << 1;      // Interrupt Enable
const GHC_MRSM: u32 = 1 << 2;    // MSI Revert to Single Message
const GHC_AE: u32 = 1 << 31;     // AHCI Enable

/// HBA Port registers
#[repr(C)]
pub struct HbaPort {
    /// Command List Base Address
    pub clb: u32,
    /// Command List Base Address Upper 32-bits
    pub clbu: u32,
    /// FIS Base Address
    pub fb: u32,
    /// FIS Base Address Upper 32-bits
    pub fbu: u32,
    /// Interrupt Status
    pub is: u32,
    /// Interrupt Enable
    pub ie: u32,
    /// Command and Status
    pub cmd: u32,
    /// Reserved
    _reserved0: u32,
    /// Task File Data
    pub tfd: u32,
    /// Signature
    pub sig: u32,
    /// Serial ATA Status
    pub ssts: u32,
    /// Serial ATA Control
    pub sctl: u32,
    /// Serial ATA Error
    pub serr: u32,
    /// Serial ATA Active
    pub sact: u32,
    /// Command Issue
    pub ci: u32,
    /// Serial ATA Notification
    pub sntf: u32,
    /// FIS-based Switching Control
    pub fbs: u32,
    /// Device Sleep
    pub devslp: u32,
    /// Reserved
    _reserved1: [u32; 10],
    /// Vendor Specific
    _vendor: [u32; 4],
}

/// Port Command bits
const PORT_CMD_ST: u32 = 1 << 0;     // Start
const PORT_CMD_SUD: u32 = 1 << 1;    // Spin-Up Device
const PORT_CMD_POD: u32 = 1 << 2;    // Power On Device
const PORT_CMD_CLO: u32 = 1 << 3;    // Command List Override
const PORT_CMD_FRE: u32 = 1 << 4;    // FIS Receive Enable
const PORT_CMD_FR: u32 = 1 << 14;    // FIS Receive Running
const PORT_CMD_CR: u32 = 1 << 15;    // Command List Running
const PORT_CMD_ICC_ACTIVE: u32 = 1 << 28;  // Interface Communication Control - Active

/// Port Interrupt Status bits
const PORT_IS_DHRS: u32 = 1 << 0;    // Device to Host Register FIS
const PORT_IS_PSS: u32 = 1 << 1;     // PIO Setup FIS
const PORT_IS_DSS: u32 = 1 << 2;     // DMA Setup FIS
const PORT_IS_SDBS: u32 = 1 << 3;    // Set Device Bits FIS
const PORT_IS_TFES: u32 = 1 << 30;   // Task File Error Status

/// Device signatures
const SATA_SIG_ATA: u32 = 0x00000101;    // SATA drive
const SATA_SIG_ATAPI: u32 = 0xEB140101;  // SATAPI device
const SATA_SIG_SEMB: u32 = 0xC33C0101;   // Enclosure management bridge
const SATA_SIG_PM: u32 = 0x96690101;     // Port multiplier

// =============================================================================
// COMMAND STRUCTURES
// =============================================================================

/// Command Header
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CommandHeader {
    /// Flags and CFL
    pub flags: u16,
    /// Physical Region Descriptor Table Length
    pub prdtl: u16,
    /// Physical Region Descriptor Byte Count
    pub prdbc: u32,
    /// Command Table Base Address
    pub ctba: u32,
    /// Command Table Base Address Upper 32-bits
    pub ctbau: u32,
    /// Reserved
    _reserved: [u32; 4],
}

impl CommandHeader {
    /// Set Command FIS Length (in DWORDs)
    pub fn set_cfl(&mut self, len: u8) {
        self.flags = (self.flags & !0x1F) | ((len as u16) & 0x1F);
    }

    /// Set Write bit
    pub fn set_write(&mut self, write: bool) {
        if write {
            self.flags |= 1 << 6;
        } else {
            self.flags &= !(1 << 6);
        }
    }

    /// Set PRDT Length
    pub fn set_prdtl(&mut self, len: u16) {
        self.prdtl = len;
    }

    /// Set Command Table Base Address
    pub fn set_ctba(&mut self, addr: u64) {
        self.ctba = addr as u32;
        self.ctbau = (addr >> 32) as u32;
    }
}

/// Command Table
#[repr(C)]
pub struct CommandTable {
    /// Command FIS
    pub cfis: [u8; 64],
    /// ATAPI Command
    pub acmd: [u8; 16],
    /// Reserved
    _reserved: [u8; 48],
    /// Physical Region Descriptor Table
    pub prdt: [PrdtEntry; 8],
}

/// Physical Region Descriptor Table Entry
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct PrdtEntry {
    /// Data Base Address
    pub dba: u32,
    /// Data Base Address Upper 32-bits
    pub dbau: u32,
    /// Reserved
    _reserved: u32,
    /// Byte Count and Interrupt on Completion
    pub dbc: u32,
}

impl PrdtEntry {
    /// Set data base address
    pub fn set_dba(&mut self, addr: u64) {
        self.dba = addr as u32;
        self.dbau = (addr >> 32) as u32;
    }

    /// Set byte count (0-based, max 4MB)
    pub fn set_byte_count(&mut self, count: u32) {
        self.dbc = (count - 1) & 0x3FFFFF;
    }

    /// Set interrupt on completion
    pub fn set_ioc(&mut self, ioc: bool) {
        if ioc {
            self.dbc |= 1 << 31;
        }
    }
}

/// FIS Register - Host to Device
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FisRegH2D {
    /// FIS Type (0x27)
    pub fis_type: u8,
    /// Port Multiplier and Command/Control
    pub flags: u8,
    /// Command
    pub command: u8,
    /// Features Low
    pub featurel: u8,
    /// LBA Low
    pub lba0: u8,
    /// LBA Mid
    pub lba1: u8,
    /// LBA High
    pub lba2: u8,
    /// Device
    pub device: u8,
    /// LBA Low (exp)
    pub lba3: u8,
    /// LBA Mid (exp)
    pub lba4: u8,
    /// LBA High (exp)
    pub lba5: u8,
    /// Features High
    pub featureh: u8,
    /// Count Low
    pub countl: u8,
    /// Count High
    pub counth: u8,
    /// Isochronous Command Completion
    pub icc: u8,
    /// Control
    pub control: u8,
    /// Reserved
    _reserved: [u8; 4],
}

impl Default for FisRegH2D {
    fn default() -> Self {
        Self {
            fis_type: 0x27,  // H2D FIS
            flags: 0x80,     // Command
            command: 0,
            featurel: 0,
            lba0: 0, lba1: 0, lba2: 0,
            device: 0,
            lba3: 0, lba4: 0, lba5: 0,
            featureh: 0,
            countl: 0, counth: 0,
            icc: 0,
            control: 0,
            _reserved: [0; 4],
        }
    }
}

/// Received FIS structure
#[repr(C)]
pub struct ReceivedFis {
    /// DMA Setup FIS
    pub dsfis: [u8; 28],
    _pad0: [u8; 4],
    /// PIO Setup FIS
    pub psfis: [u8; 20],
    _pad1: [u8; 12],
    /// D2H Register FIS
    pub rfis: [u8; 20],
    _pad2: [u8; 4],
    /// Set Device Bits FIS
    pub sdbfis: [u8; 8],
    /// Unknown FIS
    pub ufis: [u8; 64],
    _reserved: [u8; 96],
}

// =============================================================================
// ATA COMMANDS
// =============================================================================

const ATA_CMD_READ_DMA_EX: u8 = 0x25;
const ATA_CMD_WRITE_DMA_EX: u8 = 0x35;
const ATA_CMD_IDENTIFY: u8 = 0xEC;
const ATA_CMD_FLUSH_CACHE_EX: u8 = 0xEA;

// =============================================================================
// AHCI PORT
// =============================================================================

/// AHCI Port state
pub struct AhciPort {
    /// Port number
    port_num: usize,
    /// HBA memory pointer
    hba: *mut HbaMemory,
    /// Command List (32 headers)
    cmd_list: Box<[CommandHeader; MAX_COMMANDS]>,
    /// Received FIS
    received_fis: Box<ReceivedFis>,
    /// Command Tables
    cmd_tables: Box<[CommandTable; MAX_COMMANDS]>,
    /// Device type
    device_type: DeviceType,
    /// Sector count
    sector_count: u64,
    /// Model name
    model: [u8; 40],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceType {
    None,
    Sata,
    Satapi,
    Semb,
    PortMultiplier,
}

impl AhciPort {
    /// Create a new AHCI port
    pub fn new(port_num: usize, hba: *mut HbaMemory) -> Option<Self> {
        let hba_ref = unsafe { &*hba };

        // Check if port is implemented
        if (hba_ref.pi & (1 << port_num)) == 0 {
            return None;
        }

        let port = &hba_ref.ports[port_num];

        // Check device presence
        let ssts = port.ssts;
        let det = ssts & 0x0F;
        let ipm = (ssts >> 8) & 0x0F;

        if det != 3 || ipm != 1 {
            return None; // No device or not active
        }

        // Determine device type
        let device_type = match port.sig {
            SATA_SIG_ATA => DeviceType::Sata,
            SATA_SIG_ATAPI => DeviceType::Satapi,
            SATA_SIG_SEMB => DeviceType::Semb,
            SATA_SIG_PM => DeviceType::PortMultiplier,
            _ => DeviceType::None,
        };

        if device_type == DeviceType::None {
            return None;
        }

        // Allocate command structures
        let cmd_list = Box::new([CommandHeader::default(); MAX_COMMANDS]);
        let received_fis = unsafe {
            let fis: Box<ReceivedFis> = Box::new(core::mem::zeroed());
            fis
        };
        let cmd_tables = unsafe {
            Box::new(core::mem::zeroed())
        };

        let mut ahci_port = Self {
            port_num,
            hba,
            cmd_list,
            received_fis,
            cmd_tables,
            device_type,
            sector_count: 0,
            model: [0; 40],
        };

        // Initialize port
        ahci_port.init_port();

        // Identify device
        if device_type == DeviceType::Sata {
            ahci_port.identify();
        }

        Some(ahci_port)
    }

    /// Initialize port
    fn init_port(&mut self) {
        // Stop command engine first
        self.stop_cmd();

        // Get the addresses before borrowing port mutably
        let clb = self.cmd_list.as_ptr() as u64;
        let fb = self.received_fis.as_ref() as *const _ as u64;

        // Configure port in a separate scope
        {
            let port = self.port_mut();

            // Set command list base
            port.clb = clb as u32;
            port.clbu = (clb >> 32) as u32;

            // Set FIS base
            port.fb = fb as u32;
            port.fbu = (fb >> 32) as u32;

            // Clear error register
            port.serr = 0xFFFFFFFF;

            // Clear interrupt status
            port.is = 0xFFFFFFFF;
        }

        // Start command engine
        self.start_cmd();
    }

    /// Get port reference
    fn port(&self) -> &HbaPort {
        unsafe { &(*self.hba).ports[self.port_num] }
    }

    /// Get mutable port reference
    fn port_mut(&mut self) -> &mut HbaPort {
        unsafe { &mut (*self.hba).ports[self.port_num] }
    }

    /// Start command engine
    fn start_cmd(&mut self) {
        let port = self.port_mut();

        // Wait for CR to clear
        while (port.cmd & PORT_CMD_CR) != 0 {
            core::hint::spin_loop();
        }

        // Set FRE and ST
        port.cmd |= PORT_CMD_FRE;
        port.cmd |= PORT_CMD_ST;
    }

    /// Stop command engine
    fn stop_cmd(&mut self) {
        let port = self.port_mut();

        // Clear ST
        port.cmd &= !PORT_CMD_ST;

        // Wait for CR to clear
        while (port.cmd & PORT_CMD_CR) != 0 {
            core::hint::spin_loop();
        }

        // Clear FRE
        port.cmd &= !PORT_CMD_FRE;

        // Wait for FR to clear
        while (port.cmd & PORT_CMD_FR) != 0 {
            core::hint::spin_loop();
        }
    }

    /// Find free command slot
    fn find_slot(&self) -> Option<usize> {
        let port = self.port();
        let slots = port.sact | port.ci;

        for i in 0..MAX_COMMANDS {
            if (slots & (1 << i)) == 0 {
                return Some(i);
            }
        }
        None
    }

    /// Issue command and wait for completion
    fn issue_command(&mut self, slot: usize) -> Result<(), &'static str> {
        let port = self.port_mut();

        // Issue command
        port.ci = 1 << slot;

        // Wait for completion
        loop {
            if (port.ci & (1 << slot)) == 0 {
                break;
            }
            if (port.is & PORT_IS_TFES) != 0 {
                return Err("Task file error");
            }
            core::hint::spin_loop();
        }

        // Check for errors
        if (port.is & PORT_IS_TFES) != 0 {
            return Err("Task file error");
        }

        Ok(())
    }

    /// Identify device
    fn identify(&mut self) {
        let slot = match self.find_slot() {
            Some(s) => s,
            None => return,
        };

        // Allocate identify buffer
        let identify_buffer = Box::new([0u16; 256]);

        // Set up command header
        let header = &mut self.cmd_list[slot];
        header.set_cfl(5); // 5 DWORDs for H2D FIS
        header.set_write(false);
        header.set_prdtl(1);

        let table_addr = &self.cmd_tables[slot] as *const _ as u64;
        header.set_ctba(table_addr);

        // Set up command table
        let table = &mut self.cmd_tables[slot];

        // Set up command FIS
        let fis = unsafe { &mut *(table.cfis.as_mut_ptr() as *mut FisRegH2D) };
        *fis = FisRegH2D::default();
        fis.command = ATA_CMD_IDENTIFY;
        fis.device = 0;

        // Set up PRDT
        table.prdt[0].set_dba(identify_buffer.as_ptr() as u64);
        table.prdt[0].set_byte_count(512);
        table.prdt[0].set_ioc(true);

        // Issue command
        if self.issue_command(slot).is_err() {
            return;
        }

        // Parse identify data
        self.sector_count = ((identify_buffer[103] as u64) << 48)
            | ((identify_buffer[102] as u64) << 32)
            | ((identify_buffer[101] as u64) << 16)
            | (identify_buffer[100] as u64);

        // Parse model name (words 27-46)
        for i in 0..20 {
            let word = identify_buffer[27 + i];
            self.model[i * 2] = (word >> 8) as u8;
            self.model[i * 2 + 1] = (word & 0xFF) as u8;
        }
    }

    /// Read sectors
    pub fn read(&mut self, lba: u64, count: u16, buffer: &mut [u8]) -> Result<(), &'static str> {
        if buffer.len() < (count as usize) * SECTOR_SIZE {
            return Err("Buffer too small");
        }

        let slot = self.find_slot().ok_or("No free command slot")?;

        // Set up command header
        let header = &mut self.cmd_list[slot];
        header.set_cfl(5);
        header.set_write(false);
        header.set_prdtl(1);

        let table_addr = &self.cmd_tables[slot] as *const _ as u64;
        header.set_ctba(table_addr);

        // Set up command table
        let table = &mut self.cmd_tables[slot];

        // Set up command FIS
        let fis = unsafe { &mut *(table.cfis.as_mut_ptr() as *mut FisRegH2D) };
        *fis = FisRegH2D::default();
        fis.command = ATA_CMD_READ_DMA_EX;
        fis.device = 1 << 6; // LBA mode

        // Set LBA
        fis.lba0 = lba as u8;
        fis.lba1 = (lba >> 8) as u8;
        fis.lba2 = (lba >> 16) as u8;
        fis.lba3 = (lba >> 24) as u8;
        fis.lba4 = (lba >> 32) as u8;
        fis.lba5 = (lba >> 40) as u8;

        // Set count
        fis.countl = count as u8;
        fis.counth = (count >> 8) as u8;

        // Set up PRDT
        table.prdt[0].set_dba(buffer.as_ptr() as u64);
        table.prdt[0].set_byte_count((count as u32) * (SECTOR_SIZE as u32));
        table.prdt[0].set_ioc(true);

        // Issue command
        self.issue_command(slot)
    }

    /// Write sectors
    pub fn write(&mut self, lba: u64, count: u16, buffer: &[u8]) -> Result<(), &'static str> {
        if buffer.len() < (count as usize) * SECTOR_SIZE {
            return Err("Buffer too small");
        }

        let slot = self.find_slot().ok_or("No free command slot")?;

        // Set up command header
        let header = &mut self.cmd_list[slot];
        header.set_cfl(5);
        header.set_write(true);
        header.set_prdtl(1);

        let table_addr = &self.cmd_tables[slot] as *const _ as u64;
        header.set_ctba(table_addr);

        // Set up command table
        let table = &mut self.cmd_tables[slot];

        // Set up command FIS
        let fis = unsafe { &mut *(table.cfis.as_mut_ptr() as *mut FisRegH2D) };
        *fis = FisRegH2D::default();
        fis.command = ATA_CMD_WRITE_DMA_EX;
        fis.device = 1 << 6; // LBA mode

        // Set LBA
        fis.lba0 = lba as u8;
        fis.lba1 = (lba >> 8) as u8;
        fis.lba2 = (lba >> 16) as u8;
        fis.lba3 = (lba >> 24) as u8;
        fis.lba4 = (lba >> 32) as u8;
        fis.lba5 = (lba >> 40) as u8;

        // Set count
        fis.countl = count as u8;
        fis.counth = (count >> 8) as u8;

        // Set up PRDT
        table.prdt[0].set_dba(buffer.as_ptr() as u64);
        table.prdt[0].set_byte_count((count as u32) * (SECTOR_SIZE as u32));
        table.prdt[0].set_ioc(true);

        // Issue command
        self.issue_command(slot)
    }

    /// Flush cache
    pub fn flush(&mut self) -> Result<(), &'static str> {
        let slot = self.find_slot().ok_or("No free command slot")?;

        // Set up command header
        let header = &mut self.cmd_list[slot];
        header.set_cfl(5);
        header.set_write(false);
        header.set_prdtl(0);

        let table_addr = &self.cmd_tables[slot] as *const _ as u64;
        header.set_ctba(table_addr);

        // Set up command FIS
        let table = &mut self.cmd_tables[slot];
        let fis = unsafe { &mut *(table.cfis.as_mut_ptr() as *mut FisRegH2D) };
        *fis = FisRegH2D::default();
        fis.command = ATA_CMD_FLUSH_CACHE_EX;
        fis.device = 1 << 6;

        // Issue command
        self.issue_command(slot)
    }

    /// Get sector count
    pub fn sector_count(&self) -> u64 {
        self.sector_count
    }

    /// Get capacity in bytes
    pub fn capacity(&self) -> u64 {
        self.sector_count * SECTOR_SIZE as u64
    }

    /// Get model name
    pub fn model(&self) -> &str {
        let len = self.model.iter().position(|&c| c == 0).unwrap_or(40);
        core::str::from_utf8(&self.model[..len]).unwrap_or("Unknown")
    }
}

// =============================================================================
// AHCI CONTROLLER
// =============================================================================

/// AHCI Controller
pub struct AhciController {
    /// PCI device
    pci_device: PciDevice,
    /// HBA memory pointer
    hba: *mut HbaMemory,
    /// Active ports
    ports: Vec<AhciPort>,
}

// SAFETY: AhciController is only accessed through a Mutex, and the raw pointer
// to HbaMemory is only dereferenced within synchronized code.
unsafe impl Send for AhciController {}
unsafe impl Sync for AhciController {}

impl AhciController {
    /// Create a new AHCI controller from PCI device
    pub fn new(device: &PciDevice) -> Option<Self> {
        // Get ABAR (BAR5 for AHCI)
        let abar = match &device.bars[5] {
            Bar::Memory { address, .. } => *address,
            _ => return None,
        };

        let hba = abar as *mut HbaMemory;

        // Enable bus mastering and memory space
        device.enable_bus_master();
        device.enable_memory_space();

        let mut controller = Self {
            pci_device: device.clone(),
            hba,
            ports: Vec::new(),
        };

        // Initialize controller
        controller.init();

        Some(controller)
    }

    /// Initialize controller
    fn init(&mut self) {
        let hba = unsafe { &mut *self.hba };

        // Enable AHCI mode
        hba.ghc |= GHC_AE;

        // Perform HBA reset
        hba.ghc |= GHC_HR;
        while (hba.ghc & GHC_HR) != 0 {
            core::hint::spin_loop();
        }

        // Re-enable AHCI mode after reset
        hba.ghc |= GHC_AE;

        // Enable interrupts
        hba.ghc |= GHC_IE;

        // Clear pending interrupts
        hba.is = 0xFFFFFFFF;

        // Get port count and implemented ports
        let pi = hba.pi;
        let cap = hba.cap;
        let port_count = ((cap & 0x1F) + 1) as usize;

        crate::kprintln!("[AHCI] Controller initialized, {} ports implemented", pi.count_ones());

        // Initialize each port
        for i in 0..port_count {
            if (pi & (1 << i)) != 0 {
                if let Some(port) = AhciPort::new(i, self.hba) {
                    crate::kprintln!("[AHCI] Port {}: {} ({} sectors, {} GB)",
                        i,
                        port.model().trim(),
                        port.sector_count(),
                        port.capacity() / (1024 * 1024 * 1024)
                    );
                    self.ports.push(port);
                }
            }
        }
    }

    /// Get ports
    pub fn ports(&self) -> &[AhciPort] {
        &self.ports
    }

    /// Get mutable ports
    pub fn ports_mut(&mut self) -> &mut [AhciPort] {
        &mut self.ports
    }
}

// =============================================================================
// GLOBAL STATE
// =============================================================================

/// Global storage controllers
static CONTROLLERS: Mutex<Vec<AhciController>> = Mutex::new(Vec::new());

/// Initialize storage subsystem
pub fn init() {
    let mut controllers = CONTROLLERS.lock();

    // Find AHCI controllers
    let devices = pci::find_by_class(DeviceClass::MassStorage);

    for device in devices {
        if device.subclass == AHCI_SUBCLASS && device.prog_if == AHCI_PROG_IF {
            if let Some(controller) = AhciController::new(&device) {
                controllers.push(controller);
            }
        }
    }

    let total_disks: usize = controllers.iter().map(|c| c.ports.len()).sum();
    crate::kprintln!("[STORAGE] {} AHCI controller(s), {} disk(s)", controllers.len(), total_disks);
}

/// Read sectors from disk
pub fn read(disk_index: usize, lba: u64, count: u16, buffer: &mut [u8]) -> Result<(), &'static str> {
    let mut controllers = CONTROLLERS.lock();

    let mut current_index = 0;
    for controller in controllers.iter_mut() {
        for port in controller.ports_mut() {
            if current_index == disk_index {
                return port.read(lba, count, buffer);
            }
            current_index += 1;
        }
    }

    Err("Disk not found")
}

/// Write sectors to disk
pub fn write(disk_index: usize, lba: u64, count: u16, buffer: &[u8]) -> Result<(), &'static str> {
    let mut controllers = CONTROLLERS.lock();

    let mut current_index = 0;
    for controller in controllers.iter_mut() {
        for port in controller.ports_mut() {
            if current_index == disk_index {
                return port.write(lba, count, buffer);
            }
            current_index += 1;
        }
    }

    Err("Disk not found")
}

/// Flush disk cache
pub fn flush(disk_index: usize) -> Result<(), &'static str> {
    let mut controllers = CONTROLLERS.lock();

    let mut current_index = 0;
    for controller in controllers.iter_mut() {
        for port in controller.ports_mut() {
            if current_index == disk_index {
                return port.flush();
            }
            current_index += 1;
        }
    }

    Err("Disk not found")
}

/// Get disk count
pub fn disk_count() -> usize {
    CONTROLLERS.lock().iter().map(|c| c.ports.len()).sum()
}
