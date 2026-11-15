//! SDP (Service Discovery Protocol)
//!
//! Bluetooth service discovery and registration.

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

use super::l2cap::L2capManager;
use super::{BluetoothError, Uuid, UUID};

// =============================================================================
// SDP CONSTANTS
// =============================================================================

/// SDP MTU
pub const SDP_MTU: u16 = 672;

/// Maximum attribute byte count
pub const SDP_MAX_ATTR_BYTE_COUNT: u16 = 512;

// =============================================================================
// SDP PDU IDs
// =============================================================================

/// SDP PDU types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SdpPduId {
    /// Error response
    ErrorResponse = 0x01,
    /// Service search request
    ServiceSearchRequest = 0x02,
    /// Service search response
    ServiceSearchResponse = 0x03,
    /// Service attribute request
    ServiceAttributeRequest = 0x04,
    /// Service attribute response
    ServiceAttributeResponse = 0x05,
    /// Service search attribute request
    ServiceSearchAttributeRequest = 0x06,
    /// Service search attribute response
    ServiceSearchAttributeResponse = 0x07,
}

/// SDP error codes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum SdpError {
    /// Invalid version
    InvalidVersion = 0x0001,
    /// Invalid service record handle
    InvalidServiceRecordHandle = 0x0002,
    /// Invalid request syntax
    InvalidRequestSyntax = 0x0003,
    /// Invalid PDU size
    InvalidPduSize = 0x0004,
    /// Invalid continuation state
    InvalidContinuationState = 0x0005,
    /// Insufficient resources
    InsufficientResources = 0x0006,
}

// =============================================================================
// SDP ATTRIBUTE IDS
// =============================================================================

/// Well-known attribute IDs
pub mod attr {
    /// Service record handle
    pub const SERVICE_RECORD_HANDLE: u16 = 0x0000;
    /// Service class ID list
    pub const SERVICE_CLASS_ID_LIST: u16 = 0x0001;
    /// Service record state
    pub const SERVICE_RECORD_STATE: u16 = 0x0002;
    /// Service ID
    pub const SERVICE_ID: u16 = 0x0003;
    /// Protocol descriptor list
    pub const PROTOCOL_DESCRIPTOR_LIST: u16 = 0x0004;
    /// Browse group list
    pub const BROWSE_GROUP_LIST: u16 = 0x0005;
    /// Language base attribute ID list
    pub const LANGUAGE_BASE_ATTRIBUTE_ID_LIST: u16 = 0x0006;
    /// Service info time to live
    pub const SERVICE_INFO_TIME_TO_LIVE: u16 = 0x0007;
    /// Service availability
    pub const SERVICE_AVAILABILITY: u16 = 0x0008;
    /// Bluetooth profile descriptor list
    pub const BLUETOOTH_PROFILE_DESCRIPTOR_LIST: u16 = 0x0009;
    /// Documentation URL
    pub const DOCUMENTATION_URL: u16 = 0x000A;
    /// Client executable URL
    pub const CLIENT_EXECUTABLE_URL: u16 = 0x000B;
    /// Icon URL
    pub const ICON_URL: u16 = 0x000C;
    /// Additional protocol descriptor lists
    pub const ADDITIONAL_PROTOCOL_DESCRIPTOR_LISTS: u16 = 0x000D;

    // Language-dependent attributes (base + offset)
    /// Service name offset
    pub const SERVICE_NAME_OFFSET: u16 = 0x0000;
    /// Service description offset
    pub const SERVICE_DESCRIPTION_OFFSET: u16 = 0x0001;
    /// Provider name offset
    pub const PROVIDER_NAME_OFFSET: u16 = 0x0002;

    /// Default language base (English)
    pub const PRIMARY_LANGUAGE_BASE: u16 = 0x0100;
}

// =============================================================================
// SDP DATA ELEMENTS
// =============================================================================

/// SDP data element types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DataElementType {
    /// Nil (null)
    Nil = 0,
    /// Unsigned integer
    UnsignedInt = 1,
    /// Signed integer
    SignedInt = 2,
    /// UUID
    Uuid = 3,
    /// Text string
    TextString = 4,
    /// Boolean
    Boolean = 5,
    /// Sequence
    Sequence = 6,
    /// Alternative
    Alternative = 7,
    /// URL
    Url = 8,
}

/// SDP data element
#[derive(Clone, Debug)]
pub enum DataElement {
    /// Nil value
    Nil,
    /// Unsigned integer (various sizes)
    UInt8(u8),
    UInt16(u16),
    UInt32(u32),
    UInt64(u64),
    UInt128([u8; 16]),
    /// Signed integer (various sizes)
    Int8(i8),
    Int16(i16),
    Int32(i32),
    Int64(i64),
    Int128([u8; 16]),
    /// UUID
    Uuid(UUID),
    /// Text string
    Text(String),
    /// Boolean
    Bool(bool),
    /// Sequence of elements
    Sequence(Vec<DataElement>),
    /// Alternative of elements
    Alternative(Vec<DataElement>),
    /// URL
    Url(String),
}

impl DataElement {
    /// Parse data element from bytes
    pub fn from_bytes(data: &[u8]) -> Option<(Self, usize)> {
        if data.is_empty() {
            return None;
        }

        let header = data[0];
        let element_type = (header >> 3) & 0x1F;
        let size_index = header & 0x07;

        let (value_size, header_size) = match size_index {
            0 if element_type == 0 => (0, 1), // Nil
            0 => (1, 1),
            1 => (2, 1),
            2 => (4, 1),
            3 => (8, 1),
            4 => (16, 1),
            5 => {
                if data.len() < 2 {
                    return None;
                }
                (data[1] as usize, 2)
            }
            6 => {
                if data.len() < 3 {
                    return None;
                }
                (u16::from_be_bytes([data[1], data[2]]) as usize, 3)
            }
            7 => {
                if data.len() < 5 {
                    return None;
                }
                (u32::from_be_bytes([data[1], data[2], data[3], data[4]]) as usize, 5)
            }
            _ => return None,
        };

        if data.len() < header_size + value_size {
            return None;
        }

        let value_data = &data[header_size..header_size + value_size];
        let total_size = header_size + value_size;

        let element = match element_type {
            0 => DataElement::Nil,
            1 => match value_size {
                1 => DataElement::UInt8(value_data[0]),
                2 => DataElement::UInt16(u16::from_be_bytes([value_data[0], value_data[1]])),
                4 => DataElement::UInt32(u32::from_be_bytes([
                    value_data[0],
                    value_data[1],
                    value_data[2],
                    value_data[3],
                ])),
                8 => DataElement::UInt64(u64::from_be_bytes([
                    value_data[0],
                    value_data[1],
                    value_data[2],
                    value_data[3],
                    value_data[4],
                    value_data[5],
                    value_data[6],
                    value_data[7],
                ])),
                16 => {
                    let mut arr = [0u8; 16];
                    arr.copy_from_slice(value_data);
                    DataElement::UInt128(arr)
                }
                _ => return None,
            },
            2 => match value_size {
                1 => DataElement::Int8(value_data[0] as i8),
                2 => DataElement::Int16(i16::from_be_bytes([value_data[0], value_data[1]])),
                4 => DataElement::Int32(i32::from_be_bytes([
                    value_data[0],
                    value_data[1],
                    value_data[2],
                    value_data[3],
                ])),
                8 => DataElement::Int64(i64::from_be_bytes([
                    value_data[0],
                    value_data[1],
                    value_data[2],
                    value_data[3],
                    value_data[4],
                    value_data[5],
                    value_data[6],
                    value_data[7],
                ])),
                16 => {
                    let mut arr = [0u8; 16];
                    arr.copy_from_slice(value_data);
                    DataElement::Int128(arr)
                }
                _ => return None,
            },
            3 => {
                let uuid = match value_size {
                    2 => UUID::from_u16(u16::from_be_bytes([value_data[0], value_data[1]])),
                    4 => UUID::from_u32(u32::from_be_bytes([
                        value_data[0],
                        value_data[1],
                        value_data[2],
                        value_data[3],
                    ])),
                    16 => {
                        let mut arr = [0u8; 16];
                        arr.copy_from_slice(value_data);
                        Uuid { bytes: arr }
                    }
                    _ => return None,
                };
                DataElement::Uuid(uuid)
            }
            4 => {
                let text = String::from_utf8_lossy(value_data).into_owned();
                DataElement::Text(text)
            }
            5 => DataElement::Bool(value_data.first().map_or(false, |&b| b != 0)),
            6 => {
                let mut elements = Vec::new();
                let mut offset = 0;
                while offset < value_size {
                    if let Some((elem, size)) = DataElement::from_bytes(&value_data[offset..]) {
                        elements.push(elem);
                        offset += size;
                    } else {
                        break;
                    }
                }
                DataElement::Sequence(elements)
            }
            7 => {
                let mut elements = Vec::new();
                let mut offset = 0;
                while offset < value_size {
                    if let Some((elem, size)) = DataElement::from_bytes(&value_data[offset..]) {
                        elements.push(elem);
                        offset += size;
                    } else {
                        break;
                    }
                }
                DataElement::Alternative(elements)
            }
            8 => {
                let url = String::from_utf8_lossy(value_data).into_owned();
                DataElement::Url(url)
            }
            _ => return None,
        };

        Some((element, total_size))
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            DataElement::Nil => alloc::vec![0x00],
            DataElement::UInt8(v) => alloc::vec![0x08, *v],
            DataElement::UInt16(v) => {
                let mut bytes = alloc::vec![0x09];
                bytes.extend_from_slice(&v.to_be_bytes());
                bytes
            }
            DataElement::UInt32(v) => {
                let mut bytes = alloc::vec![0x0A];
                bytes.extend_from_slice(&v.to_be_bytes());
                bytes
            }
            DataElement::UInt64(v) => {
                let mut bytes = alloc::vec![0x0B];
                bytes.extend_from_slice(&v.to_be_bytes());
                bytes
            }
            DataElement::UInt128(v) => {
                let mut bytes = alloc::vec![0x0C];
                bytes.extend_from_slice(v);
                bytes
            }
            DataElement::Int8(v) => alloc::vec![0x10, *v as u8],
            DataElement::Int16(v) => {
                let mut bytes = alloc::vec![0x11];
                bytes.extend_from_slice(&v.to_be_bytes());
                bytes
            }
            DataElement::Int32(v) => {
                let mut bytes = alloc::vec![0x12];
                bytes.extend_from_slice(&v.to_be_bytes());
                bytes
            }
            DataElement::Int64(v) => {
                let mut bytes = alloc::vec![0x13];
                bytes.extend_from_slice(&v.to_be_bytes());
                bytes
            }
            DataElement::Int128(v) => {
                let mut bytes = alloc::vec![0x14];
                bytes.extend_from_slice(v);
                bytes
            }
            DataElement::Uuid(uuid) => {
                // Try to use shortest form
                if uuid.is_base_uuid() {
                    if let Some(short) = uuid.to_u16() {
                        let mut bytes = alloc::vec![0x19];
                        bytes.extend_from_slice(&short.to_be_bytes());
                        return bytes;
                    }
                    if let Some(short) = uuid.to_u32() {
                        let mut bytes = alloc::vec![0x1A];
                        bytes.extend_from_slice(&short.to_be_bytes());
                        return bytes;
                    }
                }
                let mut bytes = alloc::vec![0x1C];
                bytes.extend_from_slice(&uuid.bytes);
                bytes
            }
            DataElement::Text(s) => {
                let str_bytes = s.as_bytes();
                let mut bytes = Vec::new();
                if str_bytes.len() < 256 {
                    bytes.push(0x25);
                    bytes.push(str_bytes.len() as u8);
                } else if str_bytes.len() < 65536 {
                    bytes.push(0x26);
                    bytes.extend_from_slice(&(str_bytes.len() as u16).to_be_bytes());
                } else {
                    bytes.push(0x27);
                    bytes.extend_from_slice(&(str_bytes.len() as u32).to_be_bytes());
                }
                bytes.extend_from_slice(str_bytes);
                bytes
            }
            DataElement::Bool(v) => alloc::vec![0x28, if *v { 1 } else { 0 }],
            DataElement::Sequence(elements) => {
                let mut content = Vec::new();
                for elem in elements {
                    content.extend_from_slice(&elem.to_bytes());
                }
                let mut bytes = Vec::new();
                if content.len() < 256 {
                    bytes.push(0x35);
                    bytes.push(content.len() as u8);
                } else if content.len() < 65536 {
                    bytes.push(0x36);
                    bytes.extend_from_slice(&(content.len() as u16).to_be_bytes());
                } else {
                    bytes.push(0x37);
                    bytes.extend_from_slice(&(content.len() as u32).to_be_bytes());
                }
                bytes.extend_from_slice(&content);
                bytes
            }
            DataElement::Alternative(elements) => {
                let mut content = Vec::new();
                for elem in elements {
                    content.extend_from_slice(&elem.to_bytes());
                }
                let mut bytes = Vec::new();
                if content.len() < 256 {
                    bytes.push(0x3D);
                    bytes.push(content.len() as u8);
                } else if content.len() < 65536 {
                    bytes.push(0x3E);
                    bytes.extend_from_slice(&(content.len() as u16).to_be_bytes());
                } else {
                    bytes.push(0x3F);
                    bytes.extend_from_slice(&(content.len() as u32).to_be_bytes());
                }
                bytes.extend_from_slice(&content);
                bytes
            }
            DataElement::Url(s) => {
                let url_bytes = s.as_bytes();
                let mut bytes = Vec::new();
                if url_bytes.len() < 256 {
                    bytes.push(0x45);
                    bytes.push(url_bytes.len() as u8);
                } else if url_bytes.len() < 65536 {
                    bytes.push(0x46);
                    bytes.extend_from_slice(&(url_bytes.len() as u16).to_be_bytes());
                } else {
                    bytes.push(0x47);
                    bytes.extend_from_slice(&(url_bytes.len() as u32).to_be_bytes());
                }
                bytes.extend_from_slice(url_bytes);
                bytes
            }
        }
    }

    /// Get as UUID if this is a UUID element
    pub fn as_uuid(&self) -> Option<&UUID> {
        match self {
            DataElement::Uuid(u) => Some(u),
            _ => None,
        }
    }

    /// Get as sequence if this is a sequence element
    pub fn as_sequence(&self) -> Option<&[DataElement]> {
        match self {
            DataElement::Sequence(s) => Some(s),
            _ => None,
        }
    }

    /// Get as text if this is a text element
    pub fn as_text(&self) -> Option<&str> {
        match self {
            DataElement::Text(s) => Some(s),
            _ => None,
        }
    }

    /// Get as u16 if this is a u16 element
    pub fn as_u16(&self) -> Option<u16> {
        match self {
            DataElement::UInt16(v) => Some(*v),
            DataElement::UInt8(v) => Some(*v as u16),
            _ => None,
        }
    }

    /// Get as u32 if this is a u32 element
    pub fn as_u32(&self) -> Option<u32> {
        match self {
            DataElement::UInt32(v) => Some(*v),
            DataElement::UInt16(v) => Some(*v as u32),
            DataElement::UInt8(v) => Some(*v as u32),
            _ => None,
        }
    }
}

// =============================================================================
// SERVICE RECORD
// =============================================================================

/// SDP attribute
#[derive(Clone, Debug)]
pub struct SdpAttribute {
    /// Attribute ID
    pub id: u16,
    /// Attribute value
    pub value: DataElement,
}

/// SDP service record
#[derive(Clone, Debug)]
pub struct ServiceRecord {
    /// Service record handle
    pub handle: u32,
    /// Attributes
    pub attributes: Vec<SdpAttribute>,
}

impl ServiceRecord {
    /// Create new service record
    pub fn new(handle: u32) -> Self {
        Self {
            handle,
            attributes: Vec::new(),
        }
    }

    /// Add attribute
    pub fn add_attribute(&mut self, id: u16, value: DataElement) {
        // Remove existing attribute with same ID
        self.attributes.retain(|a| a.id != id);
        self.attributes.push(SdpAttribute { id, value });
        // Keep sorted
        self.attributes.sort_by_key(|a| a.id);
    }

    /// Get attribute
    pub fn get_attribute(&self, id: u16) -> Option<&DataElement> {
        self.attributes
            .iter()
            .find(|a| a.id == id)
            .map(|a| &a.value)
    }

    /// Get service class UUIDs
    pub fn service_class_ids(&self) -> Vec<UUID> {
        self.get_attribute(attr::SERVICE_CLASS_ID_LIST)
            .and_then(|v| v.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter_map(|e| e.as_uuid().cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get service name
    pub fn service_name(&self) -> Option<String> {
        self.get_attribute(attr::PRIMARY_LANGUAGE_BASE + attr::SERVICE_NAME_OFFSET)
            .and_then(|v| v.as_text())
            .map(|s| s.to_string())
    }

    /// Set service name
    pub fn set_service_name(&mut self, name: &str) {
        self.add_attribute(
            attr::PRIMARY_LANGUAGE_BASE + attr::SERVICE_NAME_OFFSET,
            DataElement::Text(name.to_string()),
        );
    }

    /// Get protocol descriptors
    pub fn protocol_descriptors(&self) -> Vec<ProtocolDescriptor> {
        self.get_attribute(attr::PROTOCOL_DESCRIPTOR_LIST)
            .and_then(|v| v.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter_map(|e| ProtocolDescriptor::from_element(e))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Serialize attribute list for SDP response
    pub fn serialize_attributes(&self, attr_ids: &[u16]) -> Vec<u8> {
        let mut elements = Vec::new();

        for &id in attr_ids {
            if let Some(value) = self.get_attribute(id) {
                elements.push(DataElement::UInt16(id));
                elements.push(value.clone());
            }
        }

        DataElement::Sequence(elements).to_bytes()
    }

    /// Serialize attribute range for SDP response
    pub fn serialize_attribute_range(&self, start: u16, end: u16) -> Vec<u8> {
        let mut elements = Vec::new();

        for attr in &self.attributes {
            if attr.id >= start && attr.id <= end {
                elements.push(DataElement::UInt16(attr.id));
                elements.push(attr.value.clone());
            }
        }

        DataElement::Sequence(elements).to_bytes()
    }
}

/// Protocol descriptor
#[derive(Clone, Debug)]
pub struct ProtocolDescriptor {
    /// Protocol UUID
    pub uuid: UUID,
    /// Protocol parameters
    pub params: Vec<DataElement>,
}

impl ProtocolDescriptor {
    /// Parse from data element
    pub fn from_element(element: &DataElement) -> Option<Self> {
        let seq = element.as_sequence()?;
        if seq.is_empty() {
            return None;
        }

        let uuid = seq[0].as_uuid()?.clone();
        let params = seq[1..].to_vec();

        Some(Self { uuid, params })
    }

    /// Convert to data element
    pub fn to_element(&self) -> DataElement {
        let mut seq = Vec::with_capacity(1 + self.params.len());
        seq.push(DataElement::Uuid(self.uuid.clone()));
        seq.extend_from_slice(&self.params);
        DataElement::Sequence(seq)
    }
}

// =============================================================================
// SDP PDU
// =============================================================================

/// SDP PDU header
#[derive(Clone, Debug)]
pub struct SdpPdu {
    /// PDU ID
    pub pdu_id: u8,
    /// Transaction ID
    pub transaction_id: u16,
    /// Parameter data
    pub params: Vec<u8>,
}

impl SdpPdu {
    /// Create new PDU
    pub fn new(pdu_id: SdpPduId, transaction_id: u16, params: Vec<u8>) -> Self {
        Self {
            pdu_id: pdu_id as u8,
            transaction_id,
            params,
        }
    }

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 5 {
            return None;
        }

        let pdu_id = data[0];
        let transaction_id = u16::from_be_bytes([data[1], data[2]]);
        let param_len = u16::from_be_bytes([data[3], data[4]]) as usize;

        if data.len() < 5 + param_len {
            return None;
        }

        Some(Self {
            pdu_id,
            transaction_id,
            params: data[5..5 + param_len].to_vec(),
        })
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(5 + self.params.len());
        bytes.push(self.pdu_id);
        bytes.extend_from_slice(&self.transaction_id.to_be_bytes());
        bytes.extend_from_slice(&(self.params.len() as u16).to_be_bytes());
        bytes.extend_from_slice(&self.params);
        bytes
    }

    /// Create service search request
    pub fn service_search_request(
        transaction_id: u16,
        service_uuids: &[UUID],
        max_records: u16,
        continuation: &[u8],
    ) -> Self {
        let mut params = Vec::new();

        // UUID list as sequence
        let uuid_elements: Vec<DataElement> = service_uuids
            .iter()
            .map(|u| DataElement::Uuid(u.clone()))
            .collect();
        params.extend_from_slice(&DataElement::Sequence(uuid_elements).to_bytes());

        // Max service record count
        params.extend_from_slice(&max_records.to_be_bytes());

        // Continuation state
        params.push(continuation.len() as u8);
        params.extend_from_slice(continuation);

        Self::new(SdpPduId::ServiceSearchRequest, transaction_id, params)
    }

    /// Create service attribute request
    pub fn service_attribute_request(
        transaction_id: u16,
        handle: u32,
        max_byte_count: u16,
        attr_ranges: &[(u16, u16)],
        continuation: &[u8],
    ) -> Self {
        let mut params = Vec::new();

        // Service record handle
        params.extend_from_slice(&handle.to_be_bytes());

        // Maximum attribute byte count
        params.extend_from_slice(&max_byte_count.to_be_bytes());

        // Attribute ID list as sequence
        let attr_elements: Vec<DataElement> = attr_ranges
            .iter()
            .map(|&(start, end)| {
                if start == end {
                    DataElement::UInt16(start)
                } else {
                    DataElement::UInt32(((start as u32) << 16) | (end as u32))
                }
            })
            .collect();
        params.extend_from_slice(&DataElement::Sequence(attr_elements).to_bytes());

        // Continuation state
        params.push(continuation.len() as u8);
        params.extend_from_slice(continuation);

        Self::new(SdpPduId::ServiceAttributeRequest, transaction_id, params)
    }

    /// Create service search attribute request
    pub fn service_search_attribute_request(
        transaction_id: u16,
        service_uuids: &[UUID],
        max_byte_count: u16,
        attr_ranges: &[(u16, u16)],
        continuation: &[u8],
    ) -> Self {
        let mut params = Vec::new();

        // UUID list as sequence
        let uuid_elements: Vec<DataElement> = service_uuids
            .iter()
            .map(|u| DataElement::Uuid(u.clone()))
            .collect();
        params.extend_from_slice(&DataElement::Sequence(uuid_elements).to_bytes());

        // Maximum attribute byte count
        params.extend_from_slice(&max_byte_count.to_be_bytes());

        // Attribute ID list as sequence
        let attr_elements: Vec<DataElement> = attr_ranges
            .iter()
            .map(|&(start, end)| {
                if start == end {
                    DataElement::UInt16(start)
                } else {
                    DataElement::UInt32(((start as u32) << 16) | (end as u32))
                }
            })
            .collect();
        params.extend_from_slice(&DataElement::Sequence(attr_elements).to_bytes());

        // Continuation state
        params.push(continuation.len() as u8);
        params.extend_from_slice(continuation);

        Self::new(
            SdpPduId::ServiceSearchAttributeRequest,
            transaction_id,
            params,
        )
    }

    /// Create error response
    pub fn error_response(transaction_id: u16, error: SdpError) -> Self {
        let params = (error as u16).to_be_bytes().to_vec();
        Self::new(SdpPduId::ErrorResponse, transaction_id, params)
    }
}

// =============================================================================
// SDP CLIENT
// =============================================================================

/// SDP client for service discovery
pub struct SdpClient {
    /// L2CAP manager reference
    l2cap: Arc<RwLock<L2capManager>>,
    /// Next transaction ID
    next_tid: AtomicU32,
    /// Pending requests
    pending: RwLock<BTreeMap<u16, PendingRequest>>,
}

struct PendingRequest {
    /// Transaction ID
    tid: u16,
    /// Response data (may be partial)
    response_data: Vec<u8>,
    /// Completed
    complete: bool,
}

impl SdpClient {
    /// Create new SDP client
    pub fn new(l2cap: Arc<RwLock<L2capManager>>) -> Self {
        Self {
            l2cap,
            next_tid: AtomicU32::new(1),
            pending: RwLock::new(BTreeMap::new()),
        }
    }

    /// Allocate transaction ID
    fn alloc_tid(&self) -> u16 {
        (self.next_tid.fetch_add(1, Ordering::SeqCst) & 0xFFFF) as u16
    }

    /// Search for services
    pub fn search_services(
        &self,
        handle: u16,
        service_uuids: &[UUID],
    ) -> Result<Vec<u32>, BluetoothError> {
        let tid = self.alloc_tid();
        let pdu = SdpPdu::service_search_request(tid, service_uuids, 100, &[]);

        // Send request
        self.send_pdu(handle, &pdu)?;

        // Wait for response (simplified - real implementation would use async)
        // For now, return empty
        Ok(Vec::new())
    }

    /// Get service attributes
    pub fn get_attributes(
        &self,
        handle: u16,
        record_handle: u32,
        attr_ranges: &[(u16, u16)],
    ) -> Result<ServiceRecord, BluetoothError> {
        let tid = self.alloc_tid();
        let pdu = SdpPdu::service_attribute_request(
            tid,
            record_handle,
            SDP_MAX_ATTR_BYTE_COUNT,
            attr_ranges,
            &[],
        );

        // Send request
        self.send_pdu(handle, &pdu)?;

        // Wait for response (simplified)
        Err(BluetoothError::Timeout)
    }

    /// Search and get attributes
    pub fn search_attributes(
        &self,
        handle: u16,
        service_uuids: &[UUID],
        attr_ranges: &[(u16, u16)],
    ) -> Result<Vec<ServiceRecord>, BluetoothError> {
        let tid = self.alloc_tid();
        let pdu = SdpPdu::service_search_attribute_request(
            tid,
            service_uuids,
            SDP_MAX_ATTR_BYTE_COUNT,
            attr_ranges,
            &[],
        );

        // Send request
        self.send_pdu(handle, &pdu)?;

        // Wait for response (simplified)
        Ok(Vec::new())
    }

    /// Send PDU
    fn send_pdu(&self, _handle: u16, pdu: &SdpPdu) -> Result<(), BluetoothError> {
        // Get or create L2CAP channel
        let _data = pdu.to_bytes();

        // In real implementation, would use L2CAP channel
        Ok(())
    }

    /// Handle incoming SDP data
    pub fn handle_data(&self, data: &[u8]) {
        if let Some(pdu) = SdpPdu::from_bytes(data) {
            self.handle_pdu(&pdu);
        }
    }

    /// Handle PDU
    fn handle_pdu(&self, pdu: &SdpPdu) {
        match pdu.pdu_id {
            x if x == SdpPduId::ServiceSearchResponse as u8 => {
                self.handle_search_response(pdu);
            }
            x if x == SdpPduId::ServiceAttributeResponse as u8 => {
                self.handle_attribute_response(pdu);
            }
            x if x == SdpPduId::ServiceSearchAttributeResponse as u8 => {
                self.handle_search_attribute_response(pdu);
            }
            x if x == SdpPduId::ErrorResponse as u8 => {
                self.handle_error_response(pdu);
            }
            _ => {}
        }
    }

    fn handle_search_response(&self, pdu: &SdpPdu) {
        // Parse service record handles
        if pdu.params.len() < 5 {
            return;
        }

        let _total_count = u16::from_be_bytes([pdu.params[0], pdu.params[1]]);
        let current_count = u16::from_be_bytes([pdu.params[2], pdu.params[3]]);

        let mut handles = Vec::new();
        let mut offset = 4;
        for _ in 0..current_count {
            if offset + 4 <= pdu.params.len() {
                let handle = u32::from_be_bytes([
                    pdu.params[offset],
                    pdu.params[offset + 1],
                    pdu.params[offset + 2],
                    pdu.params[offset + 3],
                ]);
                handles.push(handle);
                offset += 4;
            }
        }

        // Check continuation
        if offset < pdu.params.len() {
            let cont_len = pdu.params[offset] as usize;
            if cont_len > 0 {
                // More data to fetch
            }
        }
    }

    fn handle_attribute_response(&self, pdu: &SdpPdu) {
        // Parse attribute list
        if pdu.params.len() < 3 {
            return;
        }

        let _byte_count = u16::from_be_bytes([pdu.params[0], pdu.params[1]]);
        // Attribute data follows
    }

    fn handle_search_attribute_response(&self, pdu: &SdpPdu) {
        // Parse attribute list sequence
        if pdu.params.len() < 3 {
            return;
        }

        let _byte_count = u16::from_be_bytes([pdu.params[0], pdu.params[1]]);
        // Attribute data follows
    }

    fn handle_error_response(&self, pdu: &SdpPdu) {
        if pdu.params.len() >= 2 {
            let _error_code = u16::from_be_bytes([pdu.params[0], pdu.params[1]]);
        }
    }
}

// =============================================================================
// SDP SERVER
// =============================================================================

/// SDP server for service registration
pub struct SdpServer {
    /// L2CAP manager reference
    l2cap: Arc<RwLock<L2capManager>>,
    /// Registered service records
    records: RwLock<BTreeMap<u32, ServiceRecord>>,
    /// Next record handle
    next_handle: AtomicU32,
}

impl SdpServer {
    /// Create new SDP server
    pub fn new(l2cap: Arc<RwLock<L2capManager>>) -> Self {
        Self {
            l2cap,
            records: RwLock::new(BTreeMap::new()),
            next_handle: AtomicU32::new(0x10000), // Start above reserved handles
        }
    }

    /// Register service record
    pub fn register(&self, mut record: ServiceRecord) -> u32 {
        let handle = self.next_handle.fetch_add(1, Ordering::SeqCst);
        record.handle = handle;

        // Add service record handle attribute
        record.add_attribute(
            attr::SERVICE_RECORD_HANDLE,
            DataElement::UInt32(handle),
        );

        self.records.write().insert(handle, record);
        handle
    }

    /// Unregister service record
    pub fn unregister(&self, handle: u32) -> bool {
        self.records.write().remove(&handle).is_some()
    }

    /// Get service record
    pub fn get(&self, handle: u32) -> Option<ServiceRecord> {
        self.records.read().get(&handle).cloned()
    }

    /// Handle incoming SDP request
    pub fn handle_request(&self, data: &[u8]) -> Option<Vec<u8>> {
        let pdu = SdpPdu::from_bytes(data)?;

        let response = match pdu.pdu_id {
            x if x == SdpPduId::ServiceSearchRequest as u8 => {
                self.handle_service_search(&pdu)
            }
            x if x == SdpPduId::ServiceAttributeRequest as u8 => {
                self.handle_service_attribute(&pdu)
            }
            x if x == SdpPduId::ServiceSearchAttributeRequest as u8 => {
                self.handle_service_search_attribute(&pdu)
            }
            _ => SdpPdu::error_response(pdu.transaction_id, SdpError::InvalidRequestSyntax),
        };

        Some(response.to_bytes())
    }

    fn handle_service_search(&self, pdu: &SdpPdu) -> SdpPdu {
        // Parse UUID list
        let (uuid_list, offset) = match DataElement::from_bytes(&pdu.params) {
            Some((elem, off)) => (elem, off),
            None => {
                return SdpPdu::error_response(pdu.transaction_id, SdpError::InvalidRequestSyntax)
            }
        };

        let uuids: Vec<UUID> = uuid_list
            .as_sequence()
            .map(|seq| seq.iter().filter_map(|e| e.as_uuid().cloned()).collect())
            .unwrap_or_default();

        if pdu.params.len() < offset + 2 {
            return SdpPdu::error_response(pdu.transaction_id, SdpError::InvalidRequestSyntax);
        }

        let max_records =
            u16::from_be_bytes([pdu.params[offset], pdu.params[offset + 1]]) as usize;

        // Search for matching records
        let mut handles = Vec::new();
        for record in self.records.read().values() {
            let record_uuids = record.service_class_ids();
            let matches = uuids.iter().all(|u| record_uuids.contains(u));
            if matches {
                handles.push(record.handle);
                if handles.len() >= max_records {
                    break;
                }
            }
        }

        // Build response
        let mut params = Vec::new();
        params.extend_from_slice(&(handles.len() as u16).to_be_bytes()); // Total count
        params.extend_from_slice(&(handles.len() as u16).to_be_bytes()); // Current count
        for handle in &handles {
            params.extend_from_slice(&handle.to_be_bytes());
        }
        params.push(0); // No continuation

        SdpPdu::new(SdpPduId::ServiceSearchResponse, pdu.transaction_id, params)
    }

    fn handle_service_attribute(&self, pdu: &SdpPdu) -> SdpPdu {
        if pdu.params.len() < 6 {
            return SdpPdu::error_response(pdu.transaction_id, SdpError::InvalidRequestSyntax);
        }

        let handle = u32::from_be_bytes([
            pdu.params[0],
            pdu.params[1],
            pdu.params[2],
            pdu.params[3],
        ]);
        let max_bytes =
            u16::from_be_bytes([pdu.params[4], pdu.params[5]]) as usize;

        // Parse attribute list
        let (attr_list, _) = match DataElement::from_bytes(&pdu.params[6..]) {
            Some((elem, off)) => (elem, off),
            None => {
                return SdpPdu::error_response(pdu.transaction_id, SdpError::InvalidRequestSyntax)
            }
        };

        let record = match self.records.read().get(&handle).cloned() {
            Some(r) => r,
            None => {
                return SdpPdu::error_response(
                    pdu.transaction_id,
                    SdpError::InvalidServiceRecordHandle,
                )
            }
        };

        // Get attribute ranges
        let ranges = self.parse_attr_ranges(&attr_list);
        let mut attr_data = Vec::new();
        for (start, end) in ranges {
            attr_data.extend_from_slice(&record.serialize_attribute_range(start, end));
        }

        // Truncate if necessary
        if attr_data.len() > max_bytes {
            attr_data.truncate(max_bytes);
            // Would need continuation state
        }

        let mut params = Vec::new();
        params.extend_from_slice(&(attr_data.len() as u16).to_be_bytes());
        params.extend_from_slice(&attr_data);
        params.push(0); // No continuation

        SdpPdu::new(SdpPduId::ServiceAttributeResponse, pdu.transaction_id, params)
    }

    fn handle_service_search_attribute(&self, pdu: &SdpPdu) -> SdpPdu {
        // Parse UUID list
        let (uuid_list, offset) = match DataElement::from_bytes(&pdu.params) {
            Some((elem, off)) => (elem, off),
            None => {
                return SdpPdu::error_response(pdu.transaction_id, SdpError::InvalidRequestSyntax)
            }
        };

        let uuids: Vec<UUID> = uuid_list
            .as_sequence()
            .map(|seq| seq.iter().filter_map(|e| e.as_uuid().cloned()).collect())
            .unwrap_or_default();

        if pdu.params.len() < offset + 2 {
            return SdpPdu::error_response(pdu.transaction_id, SdpError::InvalidRequestSyntax);
        }

        let max_bytes =
            u16::from_be_bytes([pdu.params[offset], pdu.params[offset + 1]]) as usize;

        // Parse attribute list
        let (attr_list, _) = match DataElement::from_bytes(&pdu.params[offset + 2..]) {
            Some((elem, off)) => (elem, off),
            None => {
                return SdpPdu::error_response(pdu.transaction_id, SdpError::InvalidRequestSyntax)
            }
        };

        let ranges = self.parse_attr_ranges(&attr_list);

        // Search for matching records and collect attributes
        let mut results = Vec::new();
        for record in self.records.read().values() {
            let record_uuids = record.service_class_ids();
            let matches = uuids.iter().all(|u| record_uuids.contains(u));
            if matches {
                let mut attr_data = Vec::new();
                for (start, end) in &ranges {
                    attr_data.extend_from_slice(&record.serialize_attribute_range(*start, *end));
                }
                results.push(DataElement::Sequence(alloc::vec![
                    // Would parse attr_data back to elements
                ]));
            }
        }

        let result_data = DataElement::Sequence(results).to_bytes();

        let mut params = Vec::new();
        params.extend_from_slice(&(result_data.len().min(max_bytes) as u16).to_be_bytes());
        params.extend_from_slice(&result_data[..result_data.len().min(max_bytes)]);
        params.push(0); // No continuation

        SdpPdu::new(
            SdpPduId::ServiceSearchAttributeResponse,
            pdu.transaction_id,
            params,
        )
    }

    fn parse_attr_ranges(&self, attr_list: &DataElement) -> Vec<(u16, u16)> {
        let mut ranges = Vec::new();
        if let Some(seq) = attr_list.as_sequence() {
            for elem in seq {
                match elem {
                    DataElement::UInt16(id) => ranges.push((*id, *id)),
                    DataElement::UInt32(range) => {
                        let start = ((*range >> 16) & 0xFFFF) as u16;
                        let end = (*range & 0xFFFF) as u16;
                        ranges.push((start, end));
                    }
                    _ => {}
                }
            }
        }
        if ranges.is_empty() {
            ranges.push((0, 0xFFFF)); // All attributes
        }
        ranges
    }
}

// =============================================================================
// COMMON SERVICE RECORDS
// =============================================================================

/// Create Serial Port Profile service record
pub fn create_spp_record(channel: u8, name: &str) -> ServiceRecord {
    let mut record = ServiceRecord::new(0);

    // Service class ID list
    record.add_attribute(
        attr::SERVICE_CLASS_ID_LIST,
        DataElement::Sequence(alloc::vec![DataElement::Uuid(UUID::SERIAL_PORT)]),
    );

    // Protocol descriptor list
    record.add_attribute(
        attr::PROTOCOL_DESCRIPTOR_LIST,
        DataElement::Sequence(alloc::vec![
            // L2CAP
            DataElement::Sequence(alloc::vec![DataElement::Uuid(UUID::L2CAP)]),
            // RFCOMM
            DataElement::Sequence(alloc::vec![
                DataElement::Uuid(UUID::RFCOMM),
                DataElement::UInt8(channel),
            ]),
        ]),
    );

    // Profile descriptor list
    record.add_attribute(
        attr::BLUETOOTH_PROFILE_DESCRIPTOR_LIST,
        DataElement::Sequence(alloc::vec![DataElement::Sequence(alloc::vec![
            DataElement::Uuid(UUID::SERIAL_PORT),
            DataElement::UInt16(0x0102), // Version 1.2
        ])]),
    );

    // Service name
    record.set_service_name(name);

    record
}

/// Create HID service record
pub fn create_hid_record(name: &str) -> ServiceRecord {
    let mut record = ServiceRecord::new(0);

    // Service class ID list
    record.add_attribute(
        attr::SERVICE_CLASS_ID_LIST,
        DataElement::Sequence(alloc::vec![DataElement::Uuid(UUID::HID)]),
    );

    // Protocol descriptor list
    record.add_attribute(
        attr::PROTOCOL_DESCRIPTOR_LIST,
        DataElement::Sequence(alloc::vec![
            // L2CAP - HID control
            DataElement::Sequence(alloc::vec![
                DataElement::Uuid(UUID::L2CAP),
                DataElement::UInt16(0x0011), // PSM for HID control
            ]),
            // HIDP
            DataElement::Sequence(alloc::vec![DataElement::Uuid(UUID::HIDP)]),
        ]),
    );

    // Additional protocol descriptor list (interrupt channel)
    record.add_attribute(
        attr::ADDITIONAL_PROTOCOL_DESCRIPTOR_LISTS,
        DataElement::Sequence(alloc::vec![DataElement::Sequence(alloc::vec![
            DataElement::Sequence(alloc::vec![
                DataElement::Uuid(UUID::L2CAP),
                DataElement::UInt16(0x0013), // PSM for HID interrupt
            ]),
            DataElement::Sequence(alloc::vec![DataElement::Uuid(UUID::HIDP)]),
        ])]),
    );

    // Service name
    record.set_service_name(name);

    record
}
