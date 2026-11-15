// ===============================================================================
// BUILDOS KERNEL - USB MASS STORAGE DRIVER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================
//
// USB Mass Storage Class driver supporting Bulk-Only Transport (BOT) protocol.
// Implements SCSI command set for flash drives and external hard drives.
//
// ===============================================================================

#![allow(dead_code)]

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;

use super::{
    UsbDevice, UsbDriver, SetupPacket, EndpointDirection,
    EndpointType, UsbInterface, USB_CLASS_MASS_STORAGE,
};

// =============================================================================
// MASS STORAGE CONSTANTS
// =============================================================================

// Mass Storage Subclasses
const MSC_SUBCLASS_RBC: u8 = 0x01;        // Reduced Block Commands
const MSC_SUBCLASS_MMC5: u8 = 0x02;       // MMC-5 (ATAPI)
const MSC_SUBCLASS_QIC157: u8 = 0x03;     // QIC-157 (tape)
const MSC_SUBCLASS_UFI: u8 = 0x04;        // UFI (floppy)
const MSC_SUBCLASS_SFF8070I: u8 = 0x05;   // SFF-8070i
const MSC_SUBCLASS_SCSI: u8 = 0x06;       // SCSI transparent

// Mass Storage Protocols
const MSC_PROTO_CBI_INT: u8 = 0x00;       // CBI with interrupt
const MSC_PROTO_CBI_NO_INT: u8 = 0x01;    // CBI without interrupt
const MSC_PROTO_BBB: u8 = 0x50;           // Bulk-Only (BOT)
const MSC_PROTO_UAS: u8 = 0x62;           // USB Attached SCSI

// Bulk-Only Mass Storage Requests
const MSC_REQ_GET_MAX_LUN: u8 = 0xFE;
const MSC_REQ_RESET: u8 = 0xFF;

// Command Block Wrapper (CBW) signature
const CBW_SIGNATURE: u32 = 0x43425355;  // "USBC"

// Command Status Wrapper (CSW) signature
const CSW_SIGNATURE: u32 = 0x53425355;  // "USBS"

// CSW Status values
const CSW_STATUS_PASSED: u8 = 0x00;
const CSW_STATUS_FAILED: u8 = 0x01;
const CSW_STATUS_PHASE_ERROR: u8 = 0x02;

// =============================================================================
// SCSI COMMANDS
// =============================================================================

// SCSI Operation Codes
const SCSI_TEST_UNIT_READY: u8 = 0x00;
const SCSI_REQUEST_SENSE: u8 = 0x03;
const SCSI_INQUIRY: u8 = 0x12;
const SCSI_MODE_SELECT_6: u8 = 0x15;
const SCSI_MODE_SENSE_6: u8 = 0x1A;
const SCSI_START_STOP_UNIT: u8 = 0x1B;
const SCSI_PREVENT_ALLOW_MEDIUM_REMOVAL: u8 = 0x1E;
const SCSI_READ_FORMAT_CAPACITIES: u8 = 0x23;
const SCSI_READ_CAPACITY_10: u8 = 0x25;
const SCSI_READ_10: u8 = 0x28;
const SCSI_WRITE_10: u8 = 0x2A;
const SCSI_VERIFY_10: u8 = 0x2F;
const SCSI_SYNCHRONIZE_CACHE_10: u8 = 0x35;
const SCSI_MODE_SELECT_10: u8 = 0x55;
const SCSI_MODE_SENSE_10: u8 = 0x5A;
const SCSI_READ_16: u8 = 0x88;
const SCSI_WRITE_16: u8 = 0x8A;
const SCSI_SERVICE_ACTION_IN_16: u8 = 0x9E;
const SCSI_REPORT_LUNS: u8 = 0xA0;

// SCSI Sense Keys
const SENSE_NO_SENSE: u8 = 0x00;
const SENSE_RECOVERED_ERROR: u8 = 0x01;
const SENSE_NOT_READY: u8 = 0x02;
const SENSE_MEDIUM_ERROR: u8 = 0x03;
const SENSE_HARDWARE_ERROR: u8 = 0x04;
const SENSE_ILLEGAL_REQUEST: u8 = 0x05;
const SENSE_UNIT_ATTENTION: u8 = 0x06;
const SENSE_DATA_PROTECT: u8 = 0x07;
const SENSE_BLANK_CHECK: u8 = 0x08;
const SENSE_ABORTED_COMMAND: u8 = 0x0B;
const SENSE_VOLUME_OVERFLOW: u8 = 0x0D;
const SENSE_MISCOMPARE: u8 = 0x0E;

// =============================================================================
// DATA STRUCTURES
// =============================================================================

/// Command Block Wrapper (CBW)
#[derive(Clone, Copy, Default)]
#[repr(C, packed)]
pub struct CommandBlockWrapper {
    /// Signature (0x43425355)
    pub signature: u32,
    /// Tag (host-assigned)
    pub tag: u32,
    /// Data Transfer Length
    pub data_transfer_length: u32,
    /// Flags (bit 7: direction, 0=OUT, 1=IN)
    pub flags: u8,
    /// LUN (lower 4 bits)
    pub lun: u8,
    /// Command Block Length (1-16)
    pub cb_length: u8,
    /// Command Block
    pub cb: [u8; 16],
}

impl CommandBlockWrapper {
    pub fn new(tag: u32, length: u32, direction_in: bool, lun: u8, command: &[u8]) -> Self {
        let mut cbw = Self {
            signature: CBW_SIGNATURE,
            tag,
            data_transfer_length: length,
            flags: if direction_in { 0x80 } else { 0x00 },
            lun: lun & 0x0F,
            cb_length: command.len().min(16) as u8,
            cb: [0u8; 16],
        };
        cbw.cb[..command.len().min(16)].copy_from_slice(&command[..command.len().min(16)]);
        cbw
    }

    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(
                self as *const _ as *const u8,
                core::mem::size_of::<Self>(),
            )
        }
    }
}

/// Command Status Wrapper (CSW)
#[derive(Clone, Copy, Default)]
#[repr(C, packed)]
pub struct CommandStatusWrapper {
    /// Signature (0x53425355)
    pub signature: u32,
    /// Tag (matches CBW tag)
    pub tag: u32,
    /// Data Residue
    pub data_residue: u32,
    /// Status
    pub status: u8,
}

impl CommandStatusWrapper {
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 13 {
            return None;
        }

        let csw = unsafe {
            core::ptr::read_unaligned(data.as_ptr() as *const Self)
        };

        if csw.signature != CSW_SIGNATURE {
            return None;
        }

        Some(csw)
    }

    pub fn is_valid(&self) -> bool {
        self.signature == CSW_SIGNATURE
    }

    pub fn passed(&self) -> bool {
        self.status == CSW_STATUS_PASSED
    }
}

/// SCSI Inquiry Response
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct InquiryResponse {
    /// Peripheral Device Type (lower 5 bits), Peripheral Qualifier (upper 3 bits)
    pub peripheral: u8,
    /// RMB (bit 7)
    pub rmb: u8,
    /// Version
    pub version: u8,
    /// Response Data Format (lower 4 bits)
    pub response_format: u8,
    /// Additional Length
    pub additional_length: u8,
    /// Flags
    pub flags: [u8; 3],
    /// Vendor ID (8 bytes)
    pub vendor: [u8; 8],
    /// Product ID (16 bytes)
    pub product: [u8; 16],
    /// Product Revision (4 bytes)
    pub revision: [u8; 4],
}

impl InquiryResponse {
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 36 {
            return None;
        }

        let mut response = Self::default();
        response.peripheral = data[0];
        response.rmb = data[1];
        response.version = data[2];
        response.response_format = data[3];
        response.additional_length = data[4];
        response.flags.copy_from_slice(&data[5..8]);
        response.vendor.copy_from_slice(&data[8..16]);
        response.product.copy_from_slice(&data[16..32]);
        response.revision.copy_from_slice(&data[32..36]);

        Some(response)
    }

    pub fn device_type(&self) -> u8 {
        self.peripheral & 0x1F
    }

    pub fn is_removable(&self) -> bool {
        (self.rmb & 0x80) != 0
    }

    pub fn vendor_string(&self) -> String {
        String::from_utf8_lossy(&self.vendor).trim().into()
    }

    pub fn product_string(&self) -> String {
        String::from_utf8_lossy(&self.product).trim().into()
    }

    pub fn revision_string(&self) -> String {
        String::from_utf8_lossy(&self.revision).trim().into()
    }
}

/// SCSI Request Sense Response
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct RequestSenseResponse {
    pub response_code: u8,
    pub segment_number: u8,
    pub sense_key: u8,
    pub information: [u8; 4],
    pub additional_length: u8,
    pub command_specific: [u8; 4],
    pub asc: u8,
    pub ascq: u8,
    pub fruc: u8,
    pub sense_key_specific: [u8; 3],
}

impl RequestSenseResponse {
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 18 {
            return None;
        }

        let mut response = Self::default();
        response.response_code = data[0];
        response.segment_number = data[1];
        response.sense_key = data[2];
        response.information.copy_from_slice(&data[3..7]);
        response.additional_length = data[7];
        response.command_specific.copy_from_slice(&data[8..12]);
        response.asc = data[12];
        response.ascq = data[13];
        response.fruc = data[14];
        response.sense_key_specific.copy_from_slice(&data[15..18]);

        Some(response)
    }

    pub fn sense_key(&self) -> u8 {
        self.sense_key & 0x0F
    }
}

/// Read Capacity (10) Response
#[derive(Clone, Copy, Default)]
#[repr(C, packed)]
pub struct ReadCapacity10Response {
    /// Last Logical Block Address
    pub last_lba: u32,
    /// Block Length
    pub block_length: u32,
}

impl ReadCapacity10Response {
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 8 {
            return None;
        }

        Some(Self {
            last_lba: u32::from_be_bytes([data[0], data[1], data[2], data[3]]),
            block_length: u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
        })
    }

    pub fn total_blocks(&self) -> u64 {
        self.last_lba as u64 + 1
    }

    pub fn total_bytes(&self) -> u64 {
        self.total_blocks() * self.block_length as u64
    }
}

// =============================================================================
// MASS STORAGE DEVICE
// =============================================================================

/// Mass Storage Error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MassStorageError {
    TransferFailed,
    InvalidResponse,
    CommandFailed,
    PhaseError,
    DeviceNotReady,
    MediaError,
    WriteProtected,
    InvalidLba,
    Timeout,
}

/// USB Mass Storage Device
pub struct MassStorageDevice {
    device: Arc<UsbDevice>,
    interface: u8,
    bulk_in_endpoint: u8,
    bulk_out_endpoint: u8,
    max_lun: u8,
    tag: AtomicU32,
    block_size: Mutex<u32>,
    block_count: Mutex<u64>,
    vendor: Mutex<String>,
    product: Mutex<String>,
    ready: AtomicBool,
    write_protected: AtomicBool,
}

impl MassStorageDevice {
    pub fn new(
        device: Arc<UsbDevice>,
        interface: u8,
        bulk_in: u8,
        bulk_out: u8,
    ) -> Self {
        Self {
            device,
            interface,
            bulk_in_endpoint: bulk_in,
            bulk_out_endpoint: bulk_out,
            max_lun: 0,
            tag: AtomicU32::new(1),
            block_size: Mutex::new(512),
            block_count: Mutex::new(0),
            vendor: Mutex::new(String::new()),
            product: Mutex::new(String::new()),
            ready: AtomicBool::new(false),
            write_protected: AtomicBool::new(false),
        }
    }

    fn next_tag(&self) -> u32 {
        self.tag.fetch_add(1, Ordering::Relaxed)
    }

    /// Reset the device using Bulk-Only Mass Storage Reset
    pub fn reset(&self) -> Result<(), MassStorageError> {
        let setup = SetupPacket::from_raw(
            0x21, // Class, Interface, Host-to-Device
            MSC_REQ_RESET,
            0,
            self.interface as u16,
            0,
        );

        self.device
            .control_transfer(setup, None)
            .map_err(|_| MassStorageError::TransferFailed)?;

        // Clear HALT on both endpoints using control transfers
        let clear_in = SetupPacket::from_raw(0x02, 0x01, 0, (self.bulk_in_endpoint | 0x80) as u16, 0);
        let clear_out = SetupPacket::from_raw(0x02, 0x01, 0, self.bulk_out_endpoint as u16, 0);
        let _ = self.device.control_transfer(clear_in, None);
        let _ = self.device.control_transfer(clear_out, None);

        Ok(())
    }

    /// Get maximum LUN
    pub fn get_max_lun(&self) -> Result<u8, MassStorageError> {
        let setup = SetupPacket::from_raw(
            0xA1, // Class, Interface, Device-to-Host
            MSC_REQ_GET_MAX_LUN,
            0,
            self.interface as u16,
            1,
        );

        let mut buffer = [0u8; 1];
        match self.device.control_transfer(setup, Some(&mut buffer)) {
            Ok(_) => Ok(buffer[0]),
            Err(_) => Ok(0), // Some devices don't support GET_MAX_LUN
        }
    }

    /// Execute a SCSI command
    fn execute_command(
        &self,
        lun: u8,
        command: &[u8],
        data: Option<&mut [u8]>,
        direction_in: bool,
    ) -> Result<usize, MassStorageError> {
        let tag = self.next_tag();
        let data_length = data.as_ref().map(|d| d.len()).unwrap_or(0) as u32;

        // Send CBW
        let cbw = CommandBlockWrapper::new(tag, data_length, direction_in, lun, command);
        let cbw_bytes = cbw.as_bytes();

        self.device
            .bulk_transfer(self.bulk_out_endpoint, EndpointDirection::Out,
                unsafe { core::slice::from_raw_parts_mut(cbw_bytes.as_ptr() as *mut u8, cbw_bytes.len()) })
            .map_err(|_| MassStorageError::TransferFailed)?;

        // Data phase (if any)
        let mut transferred = 0usize;
        if let Some(buf) = data {
            if direction_in {
                transferred = self.device
                    .bulk_transfer(self.bulk_in_endpoint, EndpointDirection::In, buf)
                    .map_err(|_| MassStorageError::TransferFailed)?;
            } else {
                transferred = self.device
                    .bulk_transfer(self.bulk_out_endpoint, EndpointDirection::Out, buf)
                    .map_err(|_| MassStorageError::TransferFailed)?;
            }
        }

        // Receive CSW
        let mut csw_buffer = [0u8; 13];
        self.device
            .bulk_transfer(self.bulk_in_endpoint, EndpointDirection::In, &mut csw_buffer)
            .map_err(|_| MassStorageError::TransferFailed)?;

        let csw = CommandStatusWrapper::from_bytes(&csw_buffer)
            .ok_or(MassStorageError::InvalidResponse)?;

        if csw.tag != tag {
            return Err(MassStorageError::InvalidResponse);
        }

        match csw.status {
            CSW_STATUS_PASSED => Ok(transferred - csw.data_residue as usize),
            CSW_STATUS_FAILED => Err(MassStorageError::CommandFailed),
            CSW_STATUS_PHASE_ERROR => {
                self.reset()?;
                Err(MassStorageError::PhaseError)
            }
            _ => Err(MassStorageError::InvalidResponse),
        }
    }

    /// Test Unit Ready
    pub fn test_unit_ready(&self, lun: u8) -> Result<(), MassStorageError> {
        let command = [SCSI_TEST_UNIT_READY, 0, 0, 0, 0, 0];
        self.execute_command(lun, &command, None, false)?;
        Ok(())
    }

    /// Request Sense
    pub fn request_sense(&self, lun: u8) -> Result<RequestSenseResponse, MassStorageError> {
        let command = [SCSI_REQUEST_SENSE, 0, 0, 0, 18, 0];
        let mut buffer = [0u8; 18];

        self.execute_command(lun, &command, Some(&mut buffer), true)?;
        RequestSenseResponse::from_bytes(&buffer).ok_or(MassStorageError::InvalidResponse)
    }

    /// Inquiry
    pub fn inquiry(&self, lun: u8) -> Result<InquiryResponse, MassStorageError> {
        let command = [SCSI_INQUIRY, 0, 0, 0, 36, 0];
        let mut buffer = [0u8; 36];

        self.execute_command(lun, &command, Some(&mut buffer), true)?;
        InquiryResponse::from_bytes(&buffer).ok_or(MassStorageError::InvalidResponse)
    }

    /// Read Capacity (10)
    pub fn read_capacity(&self, lun: u8) -> Result<ReadCapacity10Response, MassStorageError> {
        let command = [SCSI_READ_CAPACITY_10, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let mut buffer = [0u8; 8];

        self.execute_command(lun, &command, Some(&mut buffer), true)?;
        ReadCapacity10Response::from_bytes(&buffer).ok_or(MassStorageError::InvalidResponse)
    }

    /// Read blocks
    pub fn read_blocks(&self, lun: u8, lba: u64, count: u16, buffer: &mut [u8]) -> Result<usize, MassStorageError> {
        let block_size = *self.block_size.lock();
        let expected_len = count as usize * block_size as usize;

        if buffer.len() < expected_len {
            return Err(MassStorageError::InvalidLba);
        }

        if lba > 0xFFFFFFFF {
            // Use READ(16) for large LBAs
            let count32 = count as u32;
            let command = [
                SCSI_READ_16,
                0,
                ((lba >> 56) & 0xFF) as u8,
                ((lba >> 48) & 0xFF) as u8,
                ((lba >> 40) & 0xFF) as u8,
                ((lba >> 32) & 0xFF) as u8,
                ((lba >> 24) & 0xFF) as u8,
                ((lba >> 16) & 0xFF) as u8,
                ((lba >> 8) & 0xFF) as u8,
                (lba & 0xFF) as u8,
                ((count32 >> 24) & 0xFF) as u8,
                ((count32 >> 16) & 0xFF) as u8,
                ((count32 >> 8) & 0xFF) as u8,
                (count32 & 0xFF) as u8,
                0,
                0,
            ];

            self.execute_command(lun, &command, Some(&mut buffer[..expected_len]), true)
        } else {
            // Use READ(10)
            let lba32 = lba as u32;
            let command = [
                SCSI_READ_10,
                0,
                ((lba32 >> 24) & 0xFF) as u8,
                ((lba32 >> 16) & 0xFF) as u8,
                ((lba32 >> 8) & 0xFF) as u8,
                (lba32 & 0xFF) as u8,
                0,
                ((count >> 8) & 0xFF) as u8,
                (count & 0xFF) as u8,
                0,
            ];

            self.execute_command(lun, &command, Some(&mut buffer[..expected_len]), true)
        }
    }

    /// Write blocks
    pub fn write_blocks(&self, lun: u8, lba: u64, count: u16, buffer: &mut [u8]) -> Result<usize, MassStorageError> {
        if self.write_protected.load(Ordering::Acquire) {
            return Err(MassStorageError::WriteProtected);
        }

        let block_size = *self.block_size.lock();
        let expected_len = count as usize * block_size as usize;

        if buffer.len() < expected_len {
            return Err(MassStorageError::InvalidLba);
        }

        if lba > 0xFFFFFFFF {
            // Use WRITE(16) for large LBAs
            let count32 = count as u32;
            let command = [
                SCSI_WRITE_16,
                0,
                ((lba >> 56) & 0xFF) as u8,
                ((lba >> 48) & 0xFF) as u8,
                ((lba >> 40) & 0xFF) as u8,
                ((lba >> 32) & 0xFF) as u8,
                ((lba >> 24) & 0xFF) as u8,
                ((lba >> 16) & 0xFF) as u8,
                ((lba >> 8) & 0xFF) as u8,
                (lba & 0xFF) as u8,
                ((count32 >> 24) & 0xFF) as u8,
                ((count32 >> 16) & 0xFF) as u8,
                ((count32 >> 8) & 0xFF) as u8,
                (count32 & 0xFF) as u8,
                0,
                0,
            ];

            self.execute_command(lun, &command, Some(&mut buffer[..expected_len]), false)
        } else {
            // Use WRITE(10)
            let lba32 = lba as u32;
            let command = [
                SCSI_WRITE_10,
                0,
                ((lba32 >> 24) & 0xFF) as u8,
                ((lba32 >> 16) & 0xFF) as u8,
                ((lba32 >> 8) & 0xFF) as u8,
                (lba32 & 0xFF) as u8,
                0,
                ((count >> 8) & 0xFF) as u8,
                (count & 0xFF) as u8,
                0,
            ];

            self.execute_command(lun, &command, Some(&mut buffer[..expected_len]), false)
        }
    }

    /// Synchronize cache (flush)
    pub fn sync_cache(&self, lun: u8) -> Result<(), MassStorageError> {
        let command = [SCSI_SYNCHRONIZE_CACHE_10, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        self.execute_command(lun, &command, None, false)?;
        Ok(())
    }

    /// Start/Stop unit
    pub fn start_stop_unit(&self, lun: u8, start: bool, eject: bool) -> Result<(), MassStorageError> {
        let mut flags = 0u8;
        if start { flags |= 0x01; }
        if eject { flags |= 0x02; }

        let command = [SCSI_START_STOP_UNIT, 0, 0, 0, flags, 0];
        self.execute_command(lun, &command, None, false)?;
        Ok(())
    }

    /// Initialize the device
    pub fn initialize(&self) -> Result<(), MassStorageError> {
        // Wait for device to be ready
        for _attempt in 0..10 {
            match self.test_unit_ready(0) {
                Ok(_) => break,
                Err(MassStorageError::CommandFailed) => {
                    // Check sense data
                    if let Ok(sense) = self.request_sense(0) {
                        match sense.sense_key() {
                            SENSE_NOT_READY => {
                                // Wait a bit and retry
                                for _ in 0..100000 { core::hint::spin_loop(); }
                                continue;
                            }
                            SENSE_UNIT_ATTENTION => {
                                // Media changed, retry
                                continue;
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => return Err(e),
            }
        }

        // Get device info
        let inquiry = self.inquiry(0)?;
        *self.vendor.lock() = inquiry.vendor_string();
        *self.product.lock() = inquiry.product_string();

        crate::log::info!(
            "MSC: {} {} (type: {}, removable: {})",
            inquiry.vendor_string(),
            inquiry.product_string(),
            inquiry.device_type(),
            inquiry.is_removable()
        );

        // Get capacity
        let capacity = self.read_capacity(0)?;
        // Copy packed fields to avoid unaligned access
        let block_length = { capacity.block_length };
        let total_blocks = capacity.total_blocks();
        let total_bytes = capacity.total_bytes();

        *self.block_size.lock() = block_length;
        *self.block_count.lock() = total_blocks;

        crate::log::info!(
            "MSC: {} blocks x {} bytes = {} MB",
            total_blocks,
            block_length,
            total_bytes / (1024 * 1024)
        );

        self.ready.store(true, Ordering::Release);

        Ok(())
    }

    /// Check if device is ready
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    /// Get block size
    pub fn block_size(&self) -> u32 {
        *self.block_size.lock()
    }

    /// Get block count
    pub fn block_count(&self) -> u64 {
        *self.block_count.lock()
    }

    /// Get vendor string
    pub fn vendor(&self) -> String {
        self.vendor.lock().clone()
    }

    /// Get product string
    pub fn product(&self) -> String {
        self.product.lock().clone()
    }

    /// Get capacity in bytes
    pub fn capacity(&self) -> u64 {
        *self.block_count.lock() * *self.block_size.lock() as u64
    }
}

// =============================================================================
// BLOCK DEVICE INTERFACE
// =============================================================================

/// Block device trait for integration with filesystem
pub trait BlockDevice: Send + Sync {
    fn read(&self, lba: u64, buffer: &mut [u8]) -> Result<usize, MassStorageError>;
    fn write(&self, lba: u64, buffer: &mut [u8]) -> Result<usize, MassStorageError>;
    fn block_size(&self) -> u32;
    fn block_count(&self) -> u64;
    fn flush(&self) -> Result<(), MassStorageError>;
}

impl BlockDevice for MassStorageDevice {
    fn read(&self, lba: u64, buffer: &mut [u8]) -> Result<usize, MassStorageError> {
        let block_size = self.block_size();
        let blocks = (buffer.len() as u32 + block_size - 1) / block_size;
        self.read_blocks(0, lba, blocks as u16, buffer)
    }

    fn write(&self, lba: u64, buffer: &mut [u8]) -> Result<usize, MassStorageError> {
        let block_size = self.block_size();
        let blocks = (buffer.len() as u32 + block_size - 1) / block_size;
        self.write_blocks(0, lba, blocks as u16, buffer)
    }

    fn block_size(&self) -> u32 {
        MassStorageDevice::block_size(self)
    }

    fn block_count(&self) -> u64 {
        MassStorageDevice::block_count(self)
    }

    fn flush(&self) -> Result<(), MassStorageError> {
        self.sync_cache(0)
    }
}

// =============================================================================
// MASS STORAGE DRIVER
// =============================================================================

/// Mass Storage Driver
pub struct MassStorageDriver {
    devices: Mutex<Vec<Arc<MassStorageDevice>>>,
}

impl MassStorageDriver {
    pub const fn new() -> Self {
        Self {
            devices: Mutex::new(Vec::new()),
        }
    }

    /// Get all mass storage devices
    pub fn devices(&self) -> Vec<Arc<MassStorageDevice>> {
        self.devices.lock().clone()
    }

    /// Get device by index
    pub fn device(&self, index: usize) -> Option<Arc<MassStorageDevice>> {
        self.devices.lock().get(index).cloned()
    }
}

impl UsbDriver for MassStorageDriver {
    fn name(&self) -> &'static str {
        "Mass Storage"
    }

    fn probe(&self, interface: &UsbInterface) -> bool {
        if interface.class != USB_CLASS_MASS_STORAGE {
            return false;
        }

        // Only support SCSI with Bulk-Only transport
        if interface.subclass != MSC_SUBCLASS_SCSI {
            crate::log::debug!("MSC: Unsupported subclass {:02x}", interface.subclass);
            return false;
        }

        if interface.protocol != MSC_PROTO_BBB {
            crate::log::debug!("MSC: Unsupported protocol {:02x}", interface.protocol);
            return false;
        }

        true
    }

    fn attach(&self, device: Arc<UsbDevice>, interface_num: u8) -> Result<(), &'static str> {
        crate::log::info!("MSC: Attaching mass storage device");

        // Find bulk IN and OUT endpoints
        let mut bulk_in = None;
        let mut bulk_out = None;

        for ep in device.endpoints(interface_num) {
            if ep.transfer_type() == EndpointType::Bulk {
                match ep.direction() {
                    EndpointDirection::In => bulk_in = Some(ep.number()),
                    EndpointDirection::Out => bulk_out = Some(ep.number()),
                }
            }
        }

        let (in_ep, out_ep) = match (bulk_in, bulk_out) {
            (Some(i), Some(o)) => (i, o),
            _ => {
                crate::log::warn!("MSC: Missing bulk endpoints");
                return Err("Missing bulk endpoints");
            }
        };

        let msc = Arc::new(MassStorageDevice::new(
            device.clone(),
            interface_num,
            in_ep,
            out_ep,
        ));

        // Initialize the device
        if let Err(e) = msc.initialize() {
            crate::log::error!("MSC: Failed to initialize device: {:?}", e);
            return Err("Failed to initialize device");
        }

        self.devices.lock().push(msc);
        crate::log::info!("MSC: Device attached successfully");

        Ok(())
    }

    fn detach(&self, device: Arc<UsbDevice>, _interface_num: u8) {
        let address = device.address;
        self.devices.lock().retain(|d| d.device.address != address);
        crate::log::info!("MSC: Device detached");
    }
}

// =============================================================================
// GLOBAL DRIVER INSTANCE
// =============================================================================

static MSC_DRIVER: MassStorageDriver = MassStorageDriver::new();

pub fn driver() -> &'static MassStorageDriver {
    &MSC_DRIVER
}

/// Initialize mass storage subsystem
pub fn init() {
    crate::log::info!("MSC: Subsystem initialized");
}
