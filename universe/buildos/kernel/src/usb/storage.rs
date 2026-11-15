//! USB Mass Storage Class Driver
//!
//! USB mass storage device support:
//! - Bulk-Only Transport (BOT)
//! - SCSI transparent command set
//! - UFI (USB Floppy Interface)
//! - Block device interface

#![allow(dead_code)]

use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::{Mutex, RwLock};
use super::{
    UsbError, UsbDevice, UsbDriver, UsbClass,
    SetupPacket, TransferDirection,
    device::EndpointTransferType,
};

/// Mass storage subclass codes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MassStorageSubclass {
    /// RBC (Reduced Block Commands)
    Rbc = 0x01,
    /// ATAPI (CD/DVD)
    Atapi = 0x02,
    /// QIC-157 tape
    Qic157 = 0x03,
    /// UFI (Floppy)
    Ufi = 0x04,
    /// SFF-8070i
    Sff8070i = 0x05,
    /// SCSI transparent
    ScsiTransparent = 0x06,
    /// Unknown
    Unknown,
}

impl From<u8> for MassStorageSubclass {
    fn from(val: u8) -> Self {
        match val {
            0x01 => Self::Rbc,
            0x02 => Self::Atapi,
            0x03 => Self::Qic157,
            0x04 => Self::Ufi,
            0x05 => Self::Sff8070i,
            0x06 => Self::ScsiTransparent,
            _ => Self::Unknown,
        }
    }
}

/// Mass storage protocol
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MassStorageProtocol {
    /// CBI with command completion interrupt
    CbiWithInterrupt = 0x00,
    /// CBI without command completion interrupt
    CbiWithoutInterrupt = 0x01,
    /// Bulk-Only Transport
    BulkOnly = 0x50,
    /// UAS (USB Attached SCSI)
    Uas = 0x62,
    /// Unknown
    Unknown,
}

impl From<u8> for MassStorageProtocol {
    fn from(val: u8) -> Self {
        match val {
            0x00 => Self::CbiWithInterrupt,
            0x01 => Self::CbiWithoutInterrupt,
            0x50 => Self::BulkOnly,
            0x62 => Self::Uas,
            _ => Self::Unknown,
        }
    }
}

/// Mass storage class requests
#[repr(u8)]
pub enum MassStorageRequest {
    /// Get max LUN
    GetMaxLun = 0xFE,
    /// Bulk-only mass storage reset
    BulkOnlyReset = 0xFF,
}

/// Command Block Wrapper (CBW)
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct CommandBlockWrapper {
    /// Signature (0x43425355 "USBC")
    pub signature: u32,
    /// Tag
    pub tag: u32,
    /// Data transfer length
    pub data_transfer_length: u32,
    /// Flags (bit 7 = direction: 0=out, 1=in)
    pub flags: u8,
    /// LUN (bits 3:0)
    pub lun: u8,
    /// Command block length (1-16)
    pub cb_length: u8,
    /// Command block
    pub cb: [u8; 16],
}

impl CommandBlockWrapper {
    /// CBW signature
    pub const SIGNATURE: u32 = 0x43425355; // "USBC"

    /// Create new CBW
    pub fn new(tag: u32, transfer_length: u32, direction: TransferDirection, lun: u8, command: &[u8]) -> Self {
        let mut cb = [0u8; 16];
        let len = command.len().min(16);
        cb[..len].copy_from_slice(&command[..len]);

        Self {
            signature: Self::SIGNATURE,
            tag,
            data_transfer_length: transfer_length,
            flags: if direction == TransferDirection::In { 0x80 } else { 0x00 },
            lun,
            cb_length: len as u8,
            cb,
        }
    }

    /// Convert to bytes
    pub fn to_bytes(&self) -> [u8; 31] {
        let mut bytes = [0u8; 31];
        bytes[0..4].copy_from_slice(&self.signature.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.tag.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.data_transfer_length.to_le_bytes());
        bytes[12] = self.flags;
        bytes[13] = self.lun;
        bytes[14] = self.cb_length;
        bytes[15..31].copy_from_slice(&self.cb);
        bytes
    }
}

/// Command Status Wrapper (CSW)
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct CommandStatusWrapper {
    /// Signature (0x53425355 "USBS")
    pub signature: u32,
    /// Tag (must match CBW tag)
    pub tag: u32,
    /// Data residue
    pub data_residue: u32,
    /// Status
    pub status: u8,
}

impl CommandStatusWrapper {
    /// CSW signature
    pub const SIGNATURE: u32 = 0x53425355; // "USBS"

    /// Status: Command passed
    pub const STATUS_PASSED: u8 = 0x00;
    /// Status: Command failed
    pub const STATUS_FAILED: u8 = 0x01;
    /// Status: Phase error
    pub const STATUS_PHASE_ERROR: u8 = 0x02;

    /// Create from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, UsbError> {
        if data.len() < 13 {
            return Err(UsbError::InvalidDescriptor);
        }

        let signature = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        if signature != Self::SIGNATURE {
            return Err(UsbError::ProtocolError);
        }

        Ok(Self {
            signature,
            tag: u32::from_le_bytes([data[4], data[5], data[6], data[7]]),
            data_residue: u32::from_le_bytes([data[8], data[9], data[10], data[11]]),
            status: data[12],
        })
    }

    /// Is command successful
    pub fn is_success(&self) -> bool {
        self.status == Self::STATUS_PASSED
    }
}

/// SCSI command opcodes
pub mod scsi {
    pub const TEST_UNIT_READY: u8 = 0x00;
    pub const REQUEST_SENSE: u8 = 0x03;
    pub const INQUIRY: u8 = 0x12;
    pub const MODE_SELECT_6: u8 = 0x15;
    pub const MODE_SENSE_6: u8 = 0x1A;
    pub const START_STOP_UNIT: u8 = 0x1B;
    pub const PREVENT_ALLOW_MEDIUM_REMOVAL: u8 = 0x1E;
    pub const READ_FORMAT_CAPACITIES: u8 = 0x23;
    pub const READ_CAPACITY_10: u8 = 0x25;
    pub const READ_10: u8 = 0x28;
    pub const WRITE_10: u8 = 0x2A;
    pub const VERIFY_10: u8 = 0x2F;
    pub const SYNCHRONIZE_CACHE_10: u8 = 0x35;
    pub const MODE_SELECT_10: u8 = 0x55;
    pub const MODE_SENSE_10: u8 = 0x5A;
    pub const READ_16: u8 = 0x88;
    pub const WRITE_16: u8 = 0x8A;
    pub const SYNCHRONIZE_CACHE_16: u8 = 0x91;
    pub const SERVICE_ACTION_IN_16: u8 = 0x9E;
    pub const READ_CAPACITY_16: u8 = 0x10; // Service action
}

/// SCSI inquiry data
#[derive(Clone, Debug)]
pub struct ScsiInquiryData {
    /// Peripheral qualifier and device type
    pub peripheral: u8,
    /// Removable media flag
    pub removable: bool,
    /// Version
    pub version: u8,
    /// Response data format
    pub response_format: u8,
    /// Additional length
    pub additional_length: u8,
    /// Vendor identification
    pub vendor: String,
    /// Product identification
    pub product: String,
    /// Product revision
    pub revision: String,
}

impl ScsiInquiryData {
    /// Create from raw bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, UsbError> {
        if data.len() < 36 {
            return Err(UsbError::InvalidDescriptor);
        }

        let vendor = core::str::from_utf8(&data[8..16])
            .unwrap_or("")
            .trim()
            .to_string();
        let product = core::str::from_utf8(&data[16..32])
            .unwrap_or("")
            .trim()
            .to_string();
        let revision = core::str::from_utf8(&data[32..36])
            .unwrap_or("")
            .trim()
            .to_string();

        Ok(Self {
            peripheral: data[0],
            removable: data[1] & 0x80 != 0,
            version: data[2],
            response_format: data[3] & 0x0F,
            additional_length: data[4],
            vendor: String::from(vendor),
            product: String::from(product),
            revision: String::from(revision),
        })
    }

    /// Get device type
    pub fn device_type(&self) -> ScsiDeviceType {
        ScsiDeviceType::from(self.peripheral & 0x1F)
    }
}

/// SCSI device type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScsiDeviceType {
    DirectAccess = 0x00,
    SequentialAccess = 0x01,
    Printer = 0x02,
    Processor = 0x03,
    WriteOnce = 0x04,
    CdRom = 0x05,
    Scanner = 0x06,
    OpticalMemory = 0x07,
    MediumChanger = 0x08,
    Communications = 0x09,
    StorageArray = 0x0C,
    Enclosure = 0x0D,
    SimplifiedDirectAccess = 0x0E,
    OpticalCard = 0x0F,
    BridgeController = 0x10,
    ObjectStorage = 0x11,
    Unknown,
}

impl From<u8> for ScsiDeviceType {
    fn from(val: u8) -> Self {
        match val {
            0x00 => Self::DirectAccess,
            0x01 => Self::SequentialAccess,
            0x02 => Self::Printer,
            0x03 => Self::Processor,
            0x04 => Self::WriteOnce,
            0x05 => Self::CdRom,
            0x06 => Self::Scanner,
            0x07 => Self::OpticalMemory,
            0x08 => Self::MediumChanger,
            0x09 => Self::Communications,
            0x0C => Self::StorageArray,
            0x0D => Self::Enclosure,
            0x0E => Self::SimplifiedDirectAccess,
            0x0F => Self::OpticalCard,
            0x10 => Self::BridgeController,
            0x11 => Self::ObjectStorage,
            _ => Self::Unknown,
        }
    }
}

/// USB Mass Storage device
pub struct UsbMassStorageDevice {
    /// USB device
    device: Arc<RwLock<UsbDevice>>,
    /// Interface number
    interface: u8,
    /// Subclass
    subclass: MassStorageSubclass,
    /// Protocol
    protocol: MassStorageProtocol,
    /// Bulk IN endpoint
    bulk_in: u8,
    /// Bulk OUT endpoint
    bulk_out: u8,
    /// Max LUN
    max_lun: u8,
    /// Current tag
    tag: AtomicU32,
    /// Inquiry data
    inquiry: RwLock<Option<ScsiInquiryData>>,
    /// Block size
    block_size: AtomicU32,
    /// Block count
    block_count: AtomicU64,
    /// Is ready
    ready: AtomicBool,
    /// Transfer lock
    transfer_lock: Mutex<()>,
}

impl UsbMassStorageDevice {
    /// Create new mass storage device
    pub fn new(
        device: Arc<RwLock<UsbDevice>>,
        interface: u8,
        subclass: MassStorageSubclass,
        protocol: MassStorageProtocol,
        bulk_in: u8,
        bulk_out: u8,
    ) -> Self {
        Self {
            device,
            interface,
            subclass,
            protocol,
            bulk_in,
            bulk_out,
            max_lun: 0,
            tag: AtomicU32::new(1),
            inquiry: RwLock::new(None),
            block_size: AtomicU32::new(512),
            block_count: AtomicU64::new(0),
            ready: AtomicBool::new(false),
            transfer_lock: Mutex::new(()),
        }
    }

    /// Get max LUN
    pub fn get_max_lun(&mut self) -> Result<u8, UsbError> {
        let setup = SetupPacket {
            request_type: 0xA1, // Device to host, class, interface
            request: MassStorageRequest::GetMaxLun as u8,
            value: 0,
            index: self.interface as u16,
            length: 1,
        };

        let mut buffer = [0u8; 1];
        match super::control_transfer(&self.device, &setup, Some(&mut buffer)) {
            Ok(_) => {
                self.max_lun = buffer[0];
                Ok(buffer[0])
            }
            Err(UsbError::Stall) => {
                // Device has single LUN
                self.max_lun = 0;
                Ok(0)
            }
            Err(e) => Err(e),
        }
    }

    /// Bulk-only reset
    pub fn bulk_only_reset(&self) -> Result<(), UsbError> {
        let setup = SetupPacket {
            request_type: 0x21, // Host to device, class, interface
            request: MassStorageRequest::BulkOnlyReset as u8,
            value: 0,
            index: self.interface as u16,
            length: 0,
        };

        super::control_transfer(&self.device, &setup, None)?;
        Ok(())
    }

    /// Execute SCSI command
    pub fn scsi_command(
        &self,
        lun: u8,
        command: &[u8],
        direction: TransferDirection,
        data: Option<&mut [u8]>,
    ) -> Result<usize, UsbError> {
        let _lock = self.transfer_lock.lock();

        let tag = self.tag.fetch_add(1, Ordering::SeqCst);
        let transfer_length = data.as_ref().map(|d| d.len() as u32).unwrap_or(0);

        // Send CBW
        let cbw = CommandBlockWrapper::new(tag, transfer_length, direction, lun, command);
        let cbw_bytes = cbw.to_bytes();

        let mut cbw_buf = cbw_bytes.to_vec();
        super::bulk_transfer(&self.device, self.bulk_out, &mut cbw_buf, TransferDirection::Out)?;

        // Transfer data if any
        let actual_length = if let Some(data) = data {
            super::bulk_transfer(&self.device,
                if direction == TransferDirection::In { self.bulk_in } else { self.bulk_out },
                data,
                direction
            )?
        } else {
            0
        };

        // Receive CSW
        let mut csw_buf = [0u8; 13];
        super::bulk_transfer(&self.device, self.bulk_in, &mut csw_buf, TransferDirection::In)?;

        let csw = CommandStatusWrapper::from_bytes(&csw_buf)?;

        if csw.tag != tag {
            return Err(UsbError::ProtocolError);
        }

        if !csw.is_success() {
            return Err(UsbError::TransferFailed);
        }

        Ok(actual_length)
    }

    /// Test unit ready
    pub fn test_unit_ready(&self, lun: u8) -> Result<bool, UsbError> {
        let command = [scsi::TEST_UNIT_READY, 0, 0, 0, 0, 0];

        match self.scsi_command(lun, &command, TransferDirection::Out, None) {
            Ok(_) => {
                self.ready.store(true, Ordering::Release);
                Ok(true)
            }
            Err(UsbError::TransferFailed) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Inquiry
    pub fn inquiry(&self, lun: u8) -> Result<ScsiInquiryData, UsbError> {
        let command = [scsi::INQUIRY, 0, 0, 0, 36, 0];
        let mut data = [0u8; 36];

        self.scsi_command(lun, &command, TransferDirection::In, Some(&mut data))?;

        let inquiry = ScsiInquiryData::from_bytes(&data)?;
        *self.inquiry.write() = Some(inquiry.clone());

        Ok(inquiry)
    }

    /// Read capacity (10)
    pub fn read_capacity(&self, lun: u8) -> Result<(u64, u32), UsbError> {
        let command = [scsi::READ_CAPACITY_10, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let mut data = [0u8; 8];

        self.scsi_command(lun, &command, TransferDirection::In, Some(&mut data))?;

        let last_lba = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let block_size = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);

        let block_count = last_lba as u64 + 1;

        self.block_size.store(block_size, Ordering::Release);
        self.block_count.store(block_count, Ordering::Release);

        Ok((block_count, block_size))
    }

    /// Read blocks
    pub fn read_blocks(
        &self,
        lun: u8,
        lba: u64,
        count: u16,
        buffer: &mut [u8],
    ) -> Result<usize, UsbError> {
        if lba > u32::MAX as u64 {
            // Use READ(16) for large LBAs
            return self.read_blocks_16(lun, lba, count as u32, buffer);
        }

        let lba_bytes = (lba as u32).to_be_bytes();
        let count_bytes = count.to_be_bytes();

        let command = [
            scsi::READ_10,
            0,
            lba_bytes[0], lba_bytes[1], lba_bytes[2], lba_bytes[3],
            0,
            count_bytes[0], count_bytes[1],
            0,
        ];

        self.scsi_command(lun, &command, TransferDirection::In, Some(buffer))
    }

    /// Read blocks (16-byte command)
    fn read_blocks_16(
        &self,
        lun: u8,
        lba: u64,
        count: u32,
        buffer: &mut [u8],
    ) -> Result<usize, UsbError> {
        let lba_bytes = lba.to_be_bytes();
        let count_bytes = count.to_be_bytes();

        let command = [
            scsi::READ_16,
            0,
            lba_bytes[0], lba_bytes[1], lba_bytes[2], lba_bytes[3],
            lba_bytes[4], lba_bytes[5], lba_bytes[6], lba_bytes[7],
            count_bytes[0], count_bytes[1], count_bytes[2], count_bytes[3],
            0, 0,
        ];

        self.scsi_command(lun, &command, TransferDirection::In, Some(buffer))
    }

    /// Write blocks
    pub fn write_blocks(
        &self,
        lun: u8,
        lba: u64,
        count: u16,
        buffer: &mut [u8],
    ) -> Result<usize, UsbError> {
        if lba > u32::MAX as u64 {
            return self.write_blocks_16(lun, lba, count as u32, buffer);
        }

        let lba_bytes = (lba as u32).to_be_bytes();
        let count_bytes = count.to_be_bytes();

        let command = [
            scsi::WRITE_10,
            0,
            lba_bytes[0], lba_bytes[1], lba_bytes[2], lba_bytes[3],
            0,
            count_bytes[0], count_bytes[1],
            0,
        ];

        self.scsi_command(lun, &command, TransferDirection::Out, Some(buffer))
    }

    /// Write blocks (16-byte command)
    fn write_blocks_16(
        &self,
        lun: u8,
        lba: u64,
        count: u32,
        buffer: &mut [u8],
    ) -> Result<usize, UsbError> {
        let lba_bytes = lba.to_be_bytes();
        let count_bytes = count.to_be_bytes();

        let command = [
            scsi::WRITE_16,
            0,
            lba_bytes[0], lba_bytes[1], lba_bytes[2], lba_bytes[3],
            lba_bytes[4], lba_bytes[5], lba_bytes[6], lba_bytes[7],
            count_bytes[0], count_bytes[1], count_bytes[2], count_bytes[3],
            0, 0,
        ];

        self.scsi_command(lun, &command, TransferDirection::Out, Some(buffer))
    }

    /// Synchronize cache
    pub fn sync_cache(&self, lun: u8) -> Result<(), UsbError> {
        let command = [scsi::SYNCHRONIZE_CACHE_10, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        self.scsi_command(lun, &command, TransferDirection::Out, None)?;
        Ok(())
    }

    /// Request sense
    pub fn request_sense(&self, lun: u8) -> Result<SenseData, UsbError> {
        let command = [scsi::REQUEST_SENSE, 0, 0, 0, 18, 0];
        let mut data = [0u8; 18];

        self.scsi_command(lun, &command, TransferDirection::In, Some(&mut data))?;

        Ok(SenseData::from_bytes(&data))
    }

    /// Get block size
    pub fn block_size(&self) -> u32 {
        self.block_size.load(Ordering::Acquire)
    }

    /// Get block count
    pub fn block_count(&self) -> u64 {
        self.block_count.load(Ordering::Acquire)
    }

    /// Get capacity in bytes
    pub fn capacity(&self) -> u64 {
        self.block_count() * self.block_size() as u64
    }
}

/// SCSI sense data
#[derive(Clone, Copy, Debug, Default)]
pub struct SenseData {
    /// Response code
    pub response_code: u8,
    /// Sense key
    pub sense_key: u8,
    /// Additional sense code
    pub asc: u8,
    /// Additional sense code qualifier
    pub ascq: u8,
}

impl SenseData {
    /// Create from bytes
    pub fn from_bytes(data: &[u8]) -> Self {
        if data.len() < 14 {
            return Self::default();
        }

        Self {
            response_code: data[0] & 0x7F,
            sense_key: data[2] & 0x0F,
            asc: data[12],
            ascq: data[13],
        }
    }

    /// Get sense key name
    pub fn sense_key_name(&self) -> &'static str {
        match self.sense_key {
            0x00 => "No Sense",
            0x01 => "Recovered Error",
            0x02 => "Not Ready",
            0x03 => "Medium Error",
            0x04 => "Hardware Error",
            0x05 => "Illegal Request",
            0x06 => "Unit Attention",
            0x07 => "Data Protect",
            0x08 => "Blank Check",
            0x09 => "Vendor Specific",
            0x0A => "Copy Aborted",
            0x0B => "Aborted Command",
            0x0D => "Volume Overflow",
            0x0E => "Miscompare",
            _ => "Unknown",
        }
    }
}

/// USB Mass Storage Driver
pub struct MassStorageDriver {
    /// Active devices
    devices: RwLock<Vec<Arc<UsbMassStorageDevice>>>,
}

impl MassStorageDriver {
    /// Create new mass storage driver
    pub fn new() -> Self {
        Self {
            devices: RwLock::new(Vec::new()),
        }
    }

    /// Get devices
    pub fn devices(&self) -> Vec<Arc<UsbMassStorageDevice>> {
        self.devices.read().clone()
    }
}

impl UsbDriver for MassStorageDriver {
    fn name(&self) -> &str {
        "usb-storage"
    }

    fn probe(&self, device: &UsbDevice) -> bool {
        device.class() == UsbClass::MassStorage ||
        device.current_configuration()
            .map(|c| c.interfaces.iter().any(|i| i.class() == UsbClass::MassStorage))
            .unwrap_or(false)
    }

    fn attach(&self, device: Arc<RwLock<UsbDevice>>) -> Result<(), UsbError> {
        let dev = device.read();

        // Find mass storage interface
        let config = dev.current_configuration().ok_or(UsbError::InvalidState)?;

        for iface in &config.interfaces {
            if iface.class() != UsbClass::MassStorage {
                continue;
            }

            let subclass = MassStorageSubclass::from(iface.interface_subclass);
            let protocol = MassStorageProtocol::from(iface.interface_protocol);

            // Only support bulk-only transport for now
            if protocol != MassStorageProtocol::BulkOnly {
                continue;
            }

            // Find bulk endpoints
            let bulk_in = iface.endpoints.iter()
                .find(|e| e.transfer_type() == EndpointTransferType::Bulk && e.is_in())
                .ok_or(UsbError::NoEndpoint)?
                .endpoint_address;

            let bulk_out = iface.endpoints.iter()
                .find(|e| e.transfer_type() == EndpointTransferType::Bulk && e.is_out())
                .ok_or(UsbError::NoEndpoint)?
                .endpoint_address;

            // Extract interface_number before dropping the read lock
            let interface_number = iface.interface_number;
            drop(dev);

            let mut msd = UsbMassStorageDevice::new(
                device.clone(),
                interface_number,
                subclass,
                protocol,
                bulk_in,
                bulk_out,
            );

            // Get max LUN
            let _ = msd.get_max_lun();

            // Inquiry
            if let Ok(inquiry) = msd.inquiry(0) {
                crate::kprintln!("[USB] Mass storage: {} {} ({})",
                    inquiry.vendor, inquiry.product,
                    if inquiry.removable { "removable" } else { "fixed" });
            }

            // Read capacity
            if let Ok((blocks, block_size)) = msd.read_capacity(0) {
                let size_mb = (blocks * block_size as u64) / (1024 * 1024);
                crate::kprintln!("[USB] Mass storage: {} MB ({} x {} byte blocks)",
                    size_mb, blocks, block_size);
            }

            let msd = Arc::new(msd);
            self.devices.write().push(msd);

            return Ok(());
        }

        Err(UsbError::InvalidDescriptor)
    }

    fn detach(&self, device: &UsbDevice) {
        let address = device.address;
        self.devices.write().retain(|d| {
            d.device.read().address != address
        });
    }
}
