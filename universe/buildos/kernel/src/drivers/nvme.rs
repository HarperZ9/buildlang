// ===============================================================================
// BUILDOS KERNEL - NVMe DRIVER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! NVMe (Non-Volatile Memory Express) storage driver.
//!
//! NVMe is a high-performance protocol for SSDs connected via PCIe.
//! This driver supports:
//! - Controller detection and initialization
//! - Admin queue for controller commands
//! - I/O queues for read/write operations
//! - Namespace management
//! - Interrupt handling (MSI-X or polling)

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::ptr;
use core::sync::atomic::{AtomicU16, AtomicU32, Ordering};
use spin::Mutex;

use super::pci::{self, Bar, DeviceClass, PciDevice};

// =============================================================================
// NVMe CONSTANTS
// =============================================================================

/// NVMe class code
const NVME_CLASS: u8 = 0x01;      // Mass storage
const NVME_SUBCLASS: u8 = 0x08;   // NVMe
const NVME_PROGIF: u8 = 0x02;     // NVMe

/// Controller capabilities register bits
mod cap_bits {
    pub const MQES_MASK: u64 = 0xFFFF;           // Maximum Queue Entries Supported
    pub const CQR: u64 = 1 << 16;                // Contiguous Queues Required
    pub const AMS_SHIFT: u64 = 17;               // Arbitration Mechanism Supported
    pub const AMS_MASK: u64 = 0x3;
    pub const TO_SHIFT: u64 = 24;                // Timeout (500ms units)
    pub const TO_MASK: u64 = 0xFF;
    pub const DSTRD_SHIFT: u64 = 32;             // Doorbell Stride
    pub const DSTRD_MASK: u64 = 0xF;
    pub const CSS_SHIFT: u64 = 37;               // Command Sets Supported
    pub const CSS_NVM: u64 = 1 << 37;            // NVM command set
    pub const MPSMIN_SHIFT: u64 = 48;            // Memory Page Size Minimum
    pub const MPSMIN_MASK: u64 = 0xF;
    pub const MPSMAX_SHIFT: u64 = 52;            // Memory Page Size Maximum
    pub const MPSMAX_MASK: u64 = 0xF;
}

/// Controller configuration register bits
mod cc_bits {
    pub const EN: u32 = 1 << 0;                  // Enable
    pub const CSS_SHIFT: u32 = 4;                // Command Set Selected
    pub const CSS_NVM: u32 = 0 << 4;             // NVM command set
    pub const MPS_SHIFT: u32 = 7;                // Memory Page Size
    pub const AMS_SHIFT: u32 = 11;               // Arbitration Mechanism Selected
    pub const SHN_SHIFT: u32 = 14;               // Shutdown Notification
    pub const SHN_NONE: u32 = 0 << 14;
    pub const SHN_NORMAL: u32 = 1 << 14;
    pub const SHN_ABRUPT: u32 = 2 << 14;
    pub const IOSQES_SHIFT: u32 = 16;            // I/O Submission Queue Entry Size
    pub const IOCQES_SHIFT: u32 = 20;            // I/O Completion Queue Entry Size
}

/// Controller status register bits
mod csts_bits {
    pub const RDY: u32 = 1 << 0;                 // Ready
    pub const CFS: u32 = 1 << 1;                 // Controller Fatal Status
    pub const SHST_SHIFT: u32 = 2;               // Shutdown Status
    pub const SHST_MASK: u32 = 0x3;
    pub const SHST_NORMAL: u32 = 0;              // Normal operation
    pub const SHST_PROCESSING: u32 = 1;          // Shutdown processing
    pub const SHST_COMPLETE: u32 = 2;            // Shutdown complete
}

/// Admin commands (opcodes)
mod admin_opcode {
    pub const DELETE_SQ: u8 = 0x00;
    pub const CREATE_SQ: u8 = 0x01;
    pub const GET_LOG_PAGE: u8 = 0x02;
    pub const DELETE_CQ: u8 = 0x04;
    pub const CREATE_CQ: u8 = 0x05;
    pub const IDENTIFY: u8 = 0x06;
    pub const ABORT: u8 = 0x08;
    pub const SET_FEATURES: u8 = 0x09;
    pub const GET_FEATURES: u8 = 0x0A;
    pub const ASYNC_EVENT: u8 = 0x0C;
    pub const NS_MANAGEMENT: u8 = 0x0D;
    pub const FW_COMMIT: u8 = 0x10;
    pub const FW_DOWNLOAD: u8 = 0x11;
    pub const FORMAT_NVM: u8 = 0x80;
    pub const SECURITY_SEND: u8 = 0x81;
    pub const SECURITY_RECV: u8 = 0x82;
}

/// NVM I/O commands (opcodes)
mod io_opcode {
    pub const FLUSH: u8 = 0x00;
    pub const WRITE: u8 = 0x01;
    pub const READ: u8 = 0x02;
    pub const WRITE_UNCORRECTABLE: u8 = 0x04;
    pub const COMPARE: u8 = 0x05;
    pub const WRITE_ZEROES: u8 = 0x08;
    pub const DATASET_MANAGEMENT: u8 = 0x09;
}

/// Identify CNS values
mod identify_cns {
    pub const NAMESPACE: u8 = 0x00;
    pub const CONTROLLER: u8 = 0x01;
    pub const ACTIVE_NSID_LIST: u8 = 0x02;
}

/// Queue entry sizes
const SQE_SIZE: usize = 64;  // Submission queue entry size
const CQE_SIZE: usize = 16;  // Completion queue entry size

/// Maximum queue depth (entries - 1)
const MAX_QUEUE_DEPTH: u16 = 1024;

/// Admin queue size
const ADMIN_QUEUE_SIZE: u16 = 32;

// =============================================================================
// NVMe REGISTER LAYOUT
// =============================================================================

/// NVMe controller registers (BAR0)
#[repr(C)]
struct NvmeRegs {
    /// Controller Capabilities
    cap: u64,
    /// Version
    vs: u32,
    /// Interrupt Mask Set
    intms: u32,
    /// Interrupt Mask Clear
    intmc: u32,
    /// Controller Configuration
    cc: u32,
    /// Reserved
    _rsvd1: u32,
    /// Controller Status
    csts: u32,
    /// NVM Subsystem Reset
    nssr: u32,
    /// Admin Queue Attributes
    aqa: u32,
    /// Admin Submission Queue Base Address
    asq: u64,
    /// Admin Completion Queue Base Address
    acq: u64,
    /// Controller Memory Buffer Location
    cmbloc: u32,
    /// Controller Memory Buffer Size
    cmbsz: u32,
    /// Boot Partition Information
    bpinfo: u32,
    /// Boot Partition Read Select
    bprsel: u32,
    /// Boot Partition Memory Buffer Location
    bpmbl: u64,
    /// Reserved
    _rsvd2: [u8; 0xE00 - 0x58],
    /// Command Set Specific
    _css: [u8; 0x100],
}

// =============================================================================
// NVMe COMMAND STRUCTURES
// =============================================================================

/// Submission queue entry (64 bytes)
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct NvmeCmd {
    /// Command Dword 0: Opcode, Fused, PSDT, CID
    pub cdw0: u32,
    /// Namespace ID
    pub nsid: u32,
    /// Reserved
    pub cdw2: u32,
    pub cdw3: u32,
    /// Metadata pointer
    pub mptr: u64,
    /// Data pointer (PRP1)
    pub prp1: u64,
    /// Data pointer (PRP2) or PRP list pointer
    pub prp2: u64,
    /// Command specific dwords
    pub cdw10: u32,
    pub cdw11: u32,
    pub cdw12: u32,
    pub cdw13: u32,
    pub cdw14: u32,
    pub cdw15: u32,
}

impl NvmeCmd {
    /// Create new command
    pub fn new(opcode: u8, nsid: u32) -> Self {
        Self {
            cdw0: opcode as u32,
            nsid,
            ..Default::default()
        }
    }

    /// Set command ID
    pub fn with_cid(mut self, cid: u16) -> Self {
        self.cdw0 = (self.cdw0 & 0xFFFF) | ((cid as u32) << 16);
        self
    }

    /// Set PRP1
    pub fn with_prp1(mut self, addr: u64) -> Self {
        self.prp1 = addr;
        self
    }

    /// Set PRP2
    pub fn with_prp2(mut self, addr: u64) -> Self {
        self.prp2 = addr;
        self
    }
}

/// Completion queue entry (16 bytes)
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct NvmeCqe {
    /// Command specific result
    pub result: u32,
    /// Reserved
    pub rsvd: u32,
    /// Submission Queue Head Pointer
    pub sq_head: u16,
    /// Submission Queue Identifier
    pub sq_id: u16,
    /// Command Identifier
    pub cid: u16,
    /// Status Field (includes Phase bit)
    pub status: u16,
}

impl NvmeCqe {
    /// Check if phase bit matches expected
    pub fn phase_match(&self, expected: bool) -> bool {
        ((self.status & 1) != 0) == expected
    }

    /// Get status code
    pub fn status_code(&self) -> u16 {
        (self.status >> 1) & 0x7FF
    }

    /// Check if command succeeded
    pub fn success(&self) -> bool {
        self.status_code() == 0
    }
}

// =============================================================================
// NVMe QUEUES
// =============================================================================

/// Submission queue
struct SubmissionQueue {
    /// Queue entries
    entries: Box<[NvmeCmd]>,
    /// Doorbell register address
    doorbell: *mut u32,
    /// Current tail index
    tail: AtomicU16,
    /// Queue size
    size: u16,
    /// Queue ID
    qid: u16,
}

impl SubmissionQueue {
    /// Create new submission queue
    fn new(qid: u16, size: u16, doorbell: *mut u32) -> Self {
        let entries = (0..size).map(|_| NvmeCmd::default()).collect::<Vec<_>>().into_boxed_slice();

        Self {
            entries,
            doorbell,
            tail: AtomicU16::new(0),
            size,
            qid,
        }
    }

    /// Submit a command
    fn submit(&mut self, cmd: NvmeCmd) -> u16 {
        let tail = self.tail.load(Ordering::SeqCst);
        let cid = tail;

        // Write command
        self.entries[tail as usize] = cmd.with_cid(cid);

        // Update tail
        let new_tail = (tail + 1) % self.size;
        self.tail.store(new_tail, Ordering::SeqCst);

        // Ring doorbell
        unsafe {
            ptr::write_volatile(self.doorbell, new_tail as u32);
        }

        cid
    }

    /// Get physical address of queue
    fn phys_addr(&self) -> u64 {
        self.entries.as_ptr() as u64
    }
}

/// Completion queue
struct CompletionQueue {
    /// Queue entries
    entries: Box<[NvmeCqe]>,
    /// Doorbell register address
    doorbell: *mut u32,
    /// Current head index
    head: AtomicU16,
    /// Expected phase bit
    phase: bool,
    /// Queue size
    size: u16,
    /// Queue ID
    qid: u16,
}

impl CompletionQueue {
    /// Create new completion queue
    fn new(qid: u16, size: u16, doorbell: *mut u32) -> Self {
        let entries = (0..size).map(|_| NvmeCqe::default()).collect::<Vec<_>>().into_boxed_slice();

        Self {
            entries,
            doorbell,
            head: AtomicU16::new(0),
            phase: true,
            size,
            qid,
        }
    }

    /// Poll for completion
    fn poll(&mut self) -> Option<NvmeCqe> {
        let head = self.head.load(Ordering::SeqCst);
        let entry = unsafe { ptr::read_volatile(&self.entries[head as usize]) };

        if entry.phase_match(self.phase) {
            // Advance head
            let new_head = (head + 1) % self.size;
            if new_head == 0 {
                self.phase = !self.phase;
            }
            self.head.store(new_head, Ordering::SeqCst);

            // Ring doorbell
            unsafe {
                ptr::write_volatile(self.doorbell, new_head as u32);
            }

            Some(entry)
        } else {
            None
        }
    }

    /// Wait for specific command completion
    fn wait_for(&mut self, cid: u16, timeout_ms: u32) -> Option<NvmeCqe> {
        let start = 0u32; // Would use timer here

        loop {
            if let Some(cqe) = self.poll() {
                if cqe.cid == cid {
                    return Some(cqe);
                }
            }

            // Simple timeout check (would use actual timer)
            if start > timeout_ms * 1000 {
                return None;
            }

            // Brief pause
            for _ in 0..1000 {
                core::hint::spin_loop();
            }
        }
    }

    /// Get physical address of queue
    fn phys_addr(&self) -> u64 {
        self.entries.as_ptr() as u64
    }
}

// SAFETY: Queues are protected by controller mutex
unsafe impl Send for SubmissionQueue {}
unsafe impl Sync for SubmissionQueue {}
unsafe impl Send for CompletionQueue {}
unsafe impl Sync for CompletionQueue {}

// =============================================================================
// NVMe NAMESPACE
// =============================================================================

/// NVMe namespace information
#[derive(Clone)]
pub struct NvmeNamespace {
    /// Namespace ID
    pub nsid: u32,
    /// Namespace size (logical blocks)
    pub size: u64,
    /// Namespace capacity (logical blocks)
    pub capacity: u64,
    /// LBA format index
    pub lba_format: u8,
    /// Logical block size
    pub block_size: u32,
    /// Metadata size
    pub metadata_size: u16,
}

/// Identify Namespace data (first 384 bytes)
#[repr(C)]
#[derive(Clone, Copy)]
struct IdentifyNamespace {
    /// Namespace Size (in blocks)
    nsze: u64,
    /// Namespace Capacity
    ncap: u64,
    /// Namespace Utilization
    nuse: u64,
    /// Namespace Features
    nsfeat: u8,
    /// Number of LBA Formats
    nlbaf: u8,
    /// Formatted LBA Size
    flbas: u8,
    /// Metadata Capabilities
    mc: u8,
    /// End-to-end Data Protection Capabilities
    dpc: u8,
    /// End-to-end Data Protection Type Settings
    dps: u8,
    /// Namespace Multi-path I/O and Namespace Sharing Capabilities
    nmic: u8,
    /// Reservation Capabilities
    rescap: u8,
    /// Format Progress Indicator
    fpi: u8,
    /// Deallocate Logical Block Features
    dlfeat: u8,
    /// Namespace Atomic Write Unit Normal
    nawun: u16,
    /// Namespace Atomic Write Unit Power Fail
    nawupf: u16,
    /// Namespace Atomic Compare & Write Unit
    nacwu: u16,
    /// Namespace Atomic Boundary Size Normal
    nabsn: u16,
    /// Namespace Atomic Boundary Offset
    nabo: u16,
    /// Namespace Atomic Boundary Size Power Fail
    nabspf: u16,
    /// Namespace Optimal IO Boundary
    noiob: u16,
    /// NVM Capacity
    nvmcap: [u8; 16],
    /// Reserved
    _reserved: [u8; 40],
    /// Namespace GUID
    nguid: [u8; 16],
    /// IEEE Extended Unique Identifier
    eui64: [u8; 8],
    /// LBA Format Support (up to 16 formats)
    lbaf: [u32; 16],
}

/// LBA format
#[derive(Clone, Copy)]
struct LbaFormat {
    /// Metadata size
    ms: u16,
    /// LBA data size (power of 2)
    lbads: u8,
    /// Relative Performance
    rp: u8,
}

impl From<u32> for LbaFormat {
    fn from(val: u32) -> Self {
        Self {
            ms: (val & 0xFFFF) as u16,
            lbads: ((val >> 16) & 0xFF) as u8,
            rp: ((val >> 24) & 0x3) as u8,
        }
    }
}

// =============================================================================
// NVMe CONTROLLER
// =============================================================================

/// NVMe controller
pub struct NvmeController {
    /// PCI device
    pci_device: PciDevice,
    /// Register base address
    regs: *mut NvmeRegs,
    /// Admin submission queue
    admin_sq: SubmissionQueue,
    /// Admin completion queue
    admin_cq: CompletionQueue,
    /// I/O submission queues
    io_sqs: Vec<SubmissionQueue>,
    /// I/O completion queues
    io_cqs: Vec<CompletionQueue>,
    /// Discovered namespaces
    namespaces: Vec<NvmeNamespace>,
    /// Doorbell stride (in bytes)
    doorbell_stride: u32,
    /// Maximum queue entries
    max_queue_entries: u16,
    /// Controller serial number
    serial_number: String,
    /// Controller model
    model: String,
    /// Next command ID
    next_cid: AtomicU32,
}

impl NvmeController {
    /// Initialize NVMe controller from PCI device
    pub unsafe fn new(pci_device: PciDevice) -> Result<Self, &'static str> {
        // Get BAR0
        let bar0 = match pci_device.bars[0] {
            Bar::Memory { address, size, .. } => (address, size),
            _ => return Err("NVMe requires memory-mapped BAR0"),
        };

        let regs = bar0.0 as *mut NvmeRegs;

        // Read capabilities
        let cap = ptr::read_volatile(&(*regs).cap);

        // Check for NVM command set support
        if (cap & cap_bits::CSS_NVM) == 0 {
            return Err("NVMe controller doesn't support NVM command set");
        }

        // Get configuration parameters
        let max_queue_entries = ((cap & cap_bits::MQES_MASK) + 1) as u16;
        let max_queue_entries = max_queue_entries.min(MAX_QUEUE_DEPTH);

        let doorbell_stride = 4 << ((cap >> cap_bits::DSTRD_SHIFT) & cap_bits::DSTRD_MASK);

        let timeout = ((cap >> cap_bits::TO_SHIFT) & cap_bits::TO_MASK) * 500; // ms

        // Disable controller first
        let cc = ptr::read_volatile(&(*regs).cc);
        if (cc & cc_bits::EN) != 0 {
            ptr::write_volatile(&mut (*regs).cc, cc & !cc_bits::EN);

            // Wait for not ready
            while (ptr::read_volatile(&(*regs).csts) & csts_bits::RDY) != 0 {
                core::hint::spin_loop();
            }
        }

        // Calculate doorbell addresses
        let doorbell_base = (regs as *mut u8).add(0x1000);
        let admin_sq_doorbell = doorbell_base as *mut u32;
        let admin_cq_doorbell = doorbell_base.add(doorbell_stride as usize) as *mut u32;

        // Create admin queues
        let admin_sq = SubmissionQueue::new(0, ADMIN_QUEUE_SIZE, admin_sq_doorbell);
        let admin_cq = CompletionQueue::new(0, ADMIN_QUEUE_SIZE, admin_cq_doorbell);

        // Configure admin queue attributes
        let aqa = ((ADMIN_QUEUE_SIZE as u32 - 1) << 16) | (ADMIN_QUEUE_SIZE as u32 - 1);
        ptr::write_volatile(&mut (*regs).aqa, aqa);

        // Set admin queue base addresses
        ptr::write_volatile(&mut (*regs).asq, admin_sq.phys_addr());
        ptr::write_volatile(&mut (*regs).acq, admin_cq.phys_addr());

        // Configure and enable controller
        let mps = 0u32; // 4KB page size (2^(12+0) = 4096)
        let iosqes = 6u32; // 64 byte SQ entries (2^6)
        let iocqes = 4u32; // 16 byte CQ entries (2^4)

        let cc = cc_bits::EN
            | cc_bits::CSS_NVM
            | (mps << cc_bits::MPS_SHIFT)
            | (iosqes << cc_bits::IOSQES_SHIFT)
            | (iocqes << cc_bits::IOCQES_SHIFT);

        ptr::write_volatile(&mut (*regs).cc, cc);

        // Wait for ready
        let mut timeout_count = 0u64;
        while (ptr::read_volatile(&(*regs).csts) & csts_bits::RDY) == 0 {
            core::hint::spin_loop();
            timeout_count += 1;
            if timeout_count > timeout * 1000 {
                return Err("NVMe controller timeout during initialization");
            }
        }

        // Check for fatal status
        if (ptr::read_volatile(&(*regs).csts) & csts_bits::CFS) != 0 {
            return Err("NVMe controller fatal status");
        }

        // Enable bus mastering
        pci_device.enable_bus_master();

        let mut controller = Self {
            pci_device,
            regs,
            admin_sq,
            admin_cq,
            io_sqs: Vec::new(),
            io_cqs: Vec::new(),
            namespaces: Vec::new(),
            doorbell_stride,
            max_queue_entries,
            serial_number: String::new(),
            model: String::new(),
            next_cid: AtomicU32::new(0),
        };

        // Identify controller
        controller.identify_controller()?;

        // Create I/O queue pair
        controller.create_io_queue(1)?;

        // Discover namespaces
        controller.discover_namespaces()?;

        Ok(controller)
    }

    /// Get next command ID
    fn next_cid(&self) -> u16 {
        (self.next_cid.fetch_add(1, Ordering::SeqCst) & 0xFFFF) as u16
    }

    /// Send admin command and wait for completion
    fn admin_cmd(&mut self, cmd: NvmeCmd) -> Result<NvmeCqe, &'static str> {
        let cid = self.admin_sq.submit(cmd);

        self.admin_cq.wait_for(cid, 5000)
            .ok_or("Admin command timeout")
    }

    /// Identify controller
    fn identify_controller(&mut self) -> Result<(), &'static str> {
        // Allocate buffer for identify data
        let mut data = Box::new([0u8; 4096]);
        let data_ptr = data.as_mut_ptr() as u64;

        let cmd = NvmeCmd::new(admin_opcode::IDENTIFY, 0)
            .with_prp1(data_ptr)
            .with_cid(self.next_cid());

        let mut cmd = cmd;
        cmd.cdw10 = identify_cns::CONTROLLER as u32;

        let cqe = self.admin_cmd(cmd)?;
        if !cqe.success() {
            return Err("Identify controller failed");
        }

        // Parse serial number (bytes 4-23)
        self.serial_number = core::str::from_utf8(&data[4..24])
            .unwrap_or("")
            .trim()
            .to_string();

        // Parse model (bytes 24-63)
        self.model = core::str::from_utf8(&data[24..64])
            .unwrap_or("")
            .trim()
            .to_string();

        Ok(())
    }

    /// Create I/O queue pair
    fn create_io_queue(&mut self, qid: u16) -> Result<(), &'static str> {
        let queue_size = self.max_queue_entries.min(256);

        // Calculate doorbell addresses
        let doorbell_base = unsafe { (self.regs as *mut u8).add(0x1000) };
        let sq_doorbell = unsafe {
            doorbell_base.add((2 * qid as usize) * self.doorbell_stride as usize) as *mut u32
        };
        let cq_doorbell = unsafe {
            doorbell_base.add((2 * qid as usize + 1) * self.doorbell_stride as usize) as *mut u32
        };

        // Create completion queue first
        let cq = CompletionQueue::new(qid, queue_size, cq_doorbell);

        let cmd = NvmeCmd::new(admin_opcode::CREATE_CQ, 0)
            .with_prp1(cq.phys_addr())
            .with_cid(self.next_cid());

        let mut cmd = cmd;
        cmd.cdw10 = ((queue_size as u32 - 1) << 16) | qid as u32;
        cmd.cdw11 = 1; // Physically contiguous, interrupts disabled

        let cqe = self.admin_cmd(cmd)?;
        if !cqe.success() {
            return Err("Create CQ failed");
        }

        self.io_cqs.push(cq);

        // Create submission queue
        let sq = SubmissionQueue::new(qid, queue_size, sq_doorbell);

        let cmd = NvmeCmd::new(admin_opcode::CREATE_SQ, 0)
            .with_prp1(sq.phys_addr())
            .with_cid(self.next_cid());

        let mut cmd = cmd;
        cmd.cdw10 = ((queue_size as u32 - 1) << 16) | qid as u32;
        cmd.cdw11 = ((qid as u32) << 16) | 1; // CQ ID, physically contiguous

        let cqe = self.admin_cmd(cmd)?;
        if !cqe.success() {
            return Err("Create SQ failed");
        }

        self.io_sqs.push(sq);

        Ok(())
    }

    /// Discover namespaces
    fn discover_namespaces(&mut self) -> Result<(), &'static str> {
        // Get active namespace list
        let mut nsid_list = Box::new([0u32; 1024]);
        let list_ptr = nsid_list.as_mut_ptr() as u64;

        let cmd = NvmeCmd::new(admin_opcode::IDENTIFY, 0)
            .with_prp1(list_ptr)
            .with_cid(self.next_cid());

        let mut cmd = cmd;
        cmd.cdw10 = identify_cns::ACTIVE_NSID_LIST as u32;

        let cqe = self.admin_cmd(cmd)?;
        if !cqe.success() {
            // Fallback to namespace 1
            self.identify_namespace(1)?;
            return Ok(());
        }

        // Identify each namespace
        for &nsid in nsid_list.iter() {
            if nsid == 0 {
                break;
            }
            if let Err(_) = self.identify_namespace(nsid) {
                continue;
            }
        }

        Ok(())
    }

    /// Identify namespace
    fn identify_namespace(&mut self, nsid: u32) -> Result<(), &'static str> {
        let mut data = Box::new([0u8; 4096]);
        let data_ptr = data.as_mut_ptr() as u64;

        let cmd = NvmeCmd::new(admin_opcode::IDENTIFY, nsid)
            .with_prp1(data_ptr)
            .with_cid(self.next_cid());

        let mut cmd = cmd;
        cmd.cdw10 = identify_cns::NAMESPACE as u32;

        let cqe = self.admin_cmd(cmd)?;
        if !cqe.success() {
            return Err("Identify namespace failed");
        }

        // Parse namespace data
        let ns_data: &IdentifyNamespace = unsafe {
            &*(data.as_ptr() as *const IdentifyNamespace)
        };

        let lba_format_idx = ns_data.flbas & 0xF;
        let lba_format = LbaFormat::from(ns_data.lbaf[lba_format_idx as usize]);
        let block_size = 1u32 << lba_format.lbads;

        let namespace = NvmeNamespace {
            nsid,
            size: ns_data.nsze,
            capacity: ns_data.ncap,
            lba_format: lba_format_idx,
            block_size,
            metadata_size: lba_format.ms,
        };

        self.namespaces.push(namespace);

        Ok(())
    }

    /// Read blocks from namespace
    pub fn read(&mut self, nsid: u32, lba: u64, count: u16, buf: &mut [u8]) -> Result<(), &'static str> {
        if self.io_sqs.is_empty() || self.io_cqs.is_empty() {
            return Err("No I/O queues");
        }

        let ns = self.namespaces.iter().find(|n| n.nsid == nsid)
            .ok_or("Namespace not found")?;

        let expected_len = count as usize * ns.block_size as usize;
        if buf.len() < expected_len {
            return Err("Buffer too small");
        }

        // Create read command
        let buf_ptr = buf.as_mut_ptr() as u64;

        let cmd = NvmeCmd::new(io_opcode::READ, nsid)
            .with_prp1(buf_ptr)
            .with_cid(self.next_cid());

        let mut cmd = cmd;
        cmd.cdw10 = lba as u32;
        cmd.cdw11 = (lba >> 32) as u32;
        cmd.cdw12 = (count - 1) as u32; // 0-based count

        // Submit to I/O queue
        let cid = self.io_sqs[0].submit(cmd);

        // Wait for completion
        let cqe = self.io_cqs[0].wait_for(cid, 30000)
            .ok_or("Read timeout")?;

        if !cqe.success() {
            return Err("Read failed");
        }

        Ok(())
    }

    /// Write blocks to namespace
    pub fn write(&mut self, nsid: u32, lba: u64, count: u16, buf: &[u8]) -> Result<(), &'static str> {
        if self.io_sqs.is_empty() || self.io_cqs.is_empty() {
            return Err("No I/O queues");
        }

        let ns = self.namespaces.iter().find(|n| n.nsid == nsid)
            .ok_or("Namespace not found")?;

        let expected_len = count as usize * ns.block_size as usize;
        if buf.len() < expected_len {
            return Err("Buffer too small");
        }

        // Create write command
        let buf_ptr = buf.as_ptr() as u64;

        let cmd = NvmeCmd::new(io_opcode::WRITE, nsid)
            .with_prp1(buf_ptr)
            .with_cid(self.next_cid());

        let mut cmd = cmd;
        cmd.cdw10 = lba as u32;
        cmd.cdw11 = (lba >> 32) as u32;
        cmd.cdw12 = (count - 1) as u32;

        // Submit to I/O queue
        let cid = self.io_sqs[0].submit(cmd);

        // Wait for completion
        let cqe = self.io_cqs[0].wait_for(cid, 30000)
            .ok_or("Write timeout")?;

        if !cqe.success() {
            return Err("Write failed");
        }

        Ok(())
    }

    /// Flush namespace
    pub fn flush(&mut self, nsid: u32) -> Result<(), &'static str> {
        if self.io_sqs.is_empty() || self.io_cqs.is_empty() {
            return Err("No I/O queues");
        }

        let cmd = NvmeCmd::new(io_opcode::FLUSH, nsid)
            .with_cid(self.next_cid());

        let cid = self.io_sqs[0].submit(cmd);

        let cqe = self.io_cqs[0].wait_for(cid, 30000)
            .ok_or("Flush timeout")?;

        if !cqe.success() {
            return Err("Flush failed");
        }

        Ok(())
    }

    /// Get namespace count
    pub fn namespace_count(&self) -> usize {
        self.namespaces.len()
    }

    /// Get namespace info
    pub fn get_namespace(&self, index: usize) -> Option<&NvmeNamespace> {
        self.namespaces.get(index)
    }

    /// Get controller model
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Get controller serial number
    pub fn serial_number(&self) -> &str {
        &self.serial_number
    }
}

// SAFETY: Controller access is protected by mutex in global state
unsafe impl Send for NvmeController {}
unsafe impl Sync for NvmeController {}

// =============================================================================
// GLOBAL STATE
// =============================================================================

/// Global NVMe controllers
static CONTROLLERS: Mutex<Vec<NvmeController>> = Mutex::new(Vec::new());

/// Initialize NVMe subsystem
pub fn init() {
    let devices = pci::devices();

    for device in devices {
        // Check for NVMe device
        if device.class != DeviceClass::MassStorage {
            continue;
        }
        if device.subclass != NVME_SUBCLASS {
            continue;
        }

        match unsafe { NvmeController::new(device.clone()) } {
            Ok(controller) => {
                crate::kprintln!("[NVME] Found: {} ({})",
                    controller.model(), controller.serial_number());

                for ns in &controller.namespaces {
                    let size_gb = (ns.size * ns.block_size as u64) / (1024 * 1024 * 1024);
                    crate::kprintln!("[NVME]   Namespace {}: {} GB ({} byte blocks)",
                        ns.nsid, size_gb, ns.block_size);
                }

                CONTROLLERS.lock().push(controller);
            }
            Err(e) => {
                crate::kprintln!("[NVME] Failed to initialize device: {}", e);
            }
        }
    }
}

/// Get NVMe drive count
pub fn drive_count() -> usize {
    CONTROLLERS.lock().iter()
        .map(|c| c.namespace_count())
        .sum()
}

/// Read from NVMe drive
pub fn read(drive_index: usize, lba: u64, count: u16, buf: &mut [u8]) -> Result<(), &'static str> {
    let mut controllers = CONTROLLERS.lock();

    let mut current_index = 0;
    for controller in controllers.iter_mut() {
        for ns in &controller.namespaces {
            if current_index == drive_index {
                return controller.read(ns.nsid, lba, count, buf);
            }
            current_index += 1;
        }
    }

    Err("Drive not found")
}

/// Write to NVMe drive
pub fn write(drive_index: usize, lba: u64, count: u16, buf: &[u8]) -> Result<(), &'static str> {
    let mut controllers = CONTROLLERS.lock();

    let mut current_index = 0;
    for controller in controllers.iter_mut() {
        for ns in &controller.namespaces {
            if current_index == drive_index {
                return controller.write(ns.nsid, lba, count, buf);
            }
            current_index += 1;
        }
    }

    Err("Drive not found")
}

/// Flush NVMe drive
pub fn flush(drive_index: usize) -> Result<(), &'static str> {
    let mut controllers = CONTROLLERS.lock();

    let mut current_index = 0;
    for controller in controllers.iter_mut() {
        for ns in &controller.namespaces {
            if current_index == drive_index {
                return controller.flush(ns.nsid);
            }
            current_index += 1;
        }
    }

    Err("Drive not found")
}
