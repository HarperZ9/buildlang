// ===============================================================================
// BUILDOS KERNEL - USB CORE UTILITIES
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================
//
// Core USB utilities, buffer management, and helper functions.
//
// ===============================================================================

use alloc::boxed::Box;
use core::sync::atomic::{AtomicU64, Ordering};

// =============================================================================
// USB BUFFER POOL
// =============================================================================

/// DMA-capable buffer for USB transfers
pub struct UsbBuffer {
    data: Box<[u8]>,
    physical: u64,
}

impl UsbBuffer {
    pub fn new(size: usize) -> Self {
        let data = vec![0u8; size].into_boxed_slice();
        let physical = data.as_ptr() as u64; // In real impl, convert to physical address
        Self { data, physical }
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.data.as_ptr()
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.data.as_mut_ptr()
    }

    pub fn physical_address(&self) -> u64 {
        self.physical
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }

    pub fn copy_from(&mut self, src: &[u8]) {
        let len = src.len().min(self.data.len());
        self.data[..len].copy_from_slice(&src[..len]);
    }

    pub fn copy_to(&self, dst: &mut [u8]) {
        let len = dst.len().min(self.data.len());
        dst[..len].copy_from_slice(&self.data[..len]);
    }
}

// =============================================================================
// RING BUFFER FOR TRANSFER REQUESTS
// =============================================================================

pub struct UsbRingBuffer<T: Copy + Default, const N: usize> {
    entries: [T; N],
    enqueue: usize,
    dequeue: usize,
    cycle_bit: bool,
}

impl<T: Copy + Default, const N: usize> UsbRingBuffer<T, N> {
    pub fn new() -> Self {
        Self {
            entries: [T::default(); N],
            enqueue: 0,
            dequeue: 0,
            cycle_bit: true,
        }
    }

    pub fn enqueue(&mut self, entry: T) -> Option<usize> {
        let next = (self.enqueue + 1) % N;
        if next == self.dequeue {
            return None; // Full
        }

        let index = self.enqueue;
        self.entries[index] = entry;
        self.enqueue = next;

        if next == 0 {
            self.cycle_bit = !self.cycle_bit;
        }

        Some(index)
    }

    pub fn dequeue(&mut self) -> Option<T> {
        if self.dequeue == self.enqueue {
            return None; // Empty
        }

        let entry = self.entries[self.dequeue];
        self.dequeue = (self.dequeue + 1) % N;
        Some(entry)
    }

    pub fn is_empty(&self) -> bool {
        self.dequeue == self.enqueue
    }

    pub fn is_full(&self) -> bool {
        (self.enqueue + 1) % N == self.dequeue
    }

    pub fn len(&self) -> usize {
        if self.enqueue >= self.dequeue {
            self.enqueue - self.dequeue
        } else {
            N - self.dequeue + self.enqueue
        }
    }

    pub fn cycle_bit(&self) -> bool {
        self.cycle_bit
    }

    pub fn entries(&self) -> &[T; N] {
        &self.entries
    }

    pub fn entries_mut(&mut self) -> &mut [T; N] {
        &mut self.entries
    }
}

// =============================================================================
// USB DEVICE ADDRESS ALLOCATOR
// =============================================================================

pub struct AddressAllocator {
    bitmap: AtomicU64,
    bitmap_high: AtomicU64,
}

impl AddressAllocator {
    pub const fn new() -> Self {
        Self {
            bitmap: AtomicU64::new(1), // Address 0 is reserved
            bitmap_high: AtomicU64::new(0),
        }
    }

    pub fn allocate(&self) -> Option<u8> {
        // Try low 64 addresses first
        loop {
            let current = self.bitmap.load(Ordering::Acquire);
            if current == u64::MAX {
                break;
            }

            let bit = (!current).trailing_zeros();
            if bit >= 64 {
                break;
            }

            let new = current | (1 << bit);
            if self.bitmap.compare_exchange(current, new, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
                return Some(bit as u8);
            }
        }

        // Try high 64 addresses
        loop {
            let current = self.bitmap_high.load(Ordering::Acquire);
            if current == u64::MAX {
                return None;
            }

            let bit = (!current).trailing_zeros();
            if bit >= 64 {
                return None;
            }

            let new = current | (1 << bit);
            if self.bitmap_high.compare_exchange(current, new, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
                return Some((bit + 64) as u8);
            }
        }
    }

    pub fn free(&self, address: u8) {
        if address < 64 {
            self.bitmap.fetch_and(!(1 << address), Ordering::Release);
        } else {
            self.bitmap_high.fetch_and(!(1 << (address - 64)), Ordering::Release);
        }
    }

    pub fn is_allocated(&self, address: u8) -> bool {
        if address < 64 {
            (self.bitmap.load(Ordering::Acquire) & (1 << address)) != 0
        } else {
            (self.bitmap_high.load(Ordering::Acquire) & (1 << (address - 64))) != 0
        }
    }
}

// =============================================================================
// USB TRANSFER COMPLETION QUEUE
// =============================================================================

#[derive(Clone, Copy, Default)]
pub struct CompletionEntry {
    pub transfer_id: u64,
    pub status: u32,
    pub bytes_transferred: u32,
}

pub type CompletionQueue = UsbRingBuffer<CompletionEntry, 256>;

// =============================================================================
// USB PACKET UTILITIES
// =============================================================================

/// Calculate CRC5 for USB token packets
pub fn crc5(data: u16) -> u8 {
    const POLY: u8 = 0x05;
    let mut crc: u8 = 0x1F;

    for i in 0..11 {
        let bit = ((data >> i) & 1) as u8;
        let feedback = (crc & 1) ^ bit;
        crc >>= 1;
        if feedback != 0 {
            crc ^= POLY;
        }
    }

    !crc & 0x1F
}

/// Calculate CRC16 for USB data packets
pub fn crc16(data: &[u8]) -> u16 {
    const POLY: u16 = 0x8005;
    let mut crc: u16 = 0xFFFF;

    for &byte in data {
        for i in 0..8 {
            let bit = ((byte >> i) & 1) as u16;
            let feedback = (crc & 1) ^ bit;
            crc >>= 1;
            if feedback != 0 {
                crc ^= POLY;
            }
        }
    }

    !crc
}

// =============================================================================
// USB TIMING UTILITIES
// =============================================================================

/// Microframe duration in nanoseconds (125 microseconds)
pub const MICROFRAME_NS: u64 = 125_000;

/// Frame duration in nanoseconds (1 millisecond)
pub const FRAME_NS: u64 = 1_000_000;

/// Calculate number of transactions per microframe for given bandwidth
pub fn transactions_per_microframe(max_packet_size: u16, speed: super::UsbSpeed) -> u8 {
    match speed {
        super::UsbSpeed::High => {
            if max_packet_size <= 1024 {
                ((max_packet_size - 1) / 512 + 1) as u8
            } else {
                3
            }
        }
        _ => 1,
    }
}

/// Calculate polling interval in microframes
pub fn interval_to_microframes(interval: u8, speed: super::UsbSpeed) -> u32 {
    match speed {
        super::UsbSpeed::High | super::UsbSpeed::Super | super::UsbSpeed::SuperPlus => {
            // Interval is expressed as 2^(interval-1) microframes
            if interval > 0 && interval <= 16 {
                1 << (interval - 1)
            } else {
                1
            }
        }
        super::UsbSpeed::Full | super::UsbSpeed::Low => {
            // Interval is in milliseconds (frames)
            (interval as u32) * 8 // Convert to microframes
        }
        _ => interval as u32,
    }
}

// =============================================================================
// USB STRING HANDLING
// =============================================================================

/// Decode USB string descriptor (UTF-16LE to UTF-8)
pub fn decode_string_descriptor(data: &[u8]) -> Option<alloc::string::String> {
    if data.len() < 2 || data[1] != super::USB_DESC_STRING {
        return None;
    }

    let len = data[0] as usize;
    if len < 2 || len > data.len() {
        return None;
    }

    let mut result = alloc::string::String::new();
    let char_count = (len - 2) / 2;

    for i in 0..char_count {
        let offset = 2 + i * 2;
        if offset + 1 >= data.len() {
            break;
        }
        let code = u16::from_le_bytes([data[offset], data[offset + 1]]);
        if let Some(c) = char::from_u32(code as u32) {
            result.push(c);
        }
    }

    Some(result)
}

// =============================================================================
// USB CLASS HELPERS
// =============================================================================

/// Get human-readable name for USB class code
pub fn class_name(class: u8) -> &'static str {
    match class {
        super::USB_CLASS_AUDIO => "Audio",
        super::USB_CLASS_CDC => "Communications",
        super::USB_CLASS_HID => "HID",
        super::USB_CLASS_PHYSICAL => "Physical",
        super::USB_CLASS_IMAGE => "Image",
        super::USB_CLASS_PRINTER => "Printer",
        super::USB_CLASS_MASS_STORAGE => "Mass Storage",
        super::USB_CLASS_HUB => "Hub",
        super::USB_CLASS_CDC_DATA => "CDC Data",
        super::USB_CLASS_VENDOR => "Vendor Specific",
        0x00 => "Device",
        0x0B => "Smart Card",
        0x0D => "Content Security",
        0x0E => "Video",
        0x0F => "Personal Healthcare",
        0x10 => "Audio/Video",
        0x11 => "Billboard",
        0xDC => "Diagnostic",
        0xE0 => "Wireless Controller",
        0xEF => "Miscellaneous",
        0xFE => "Application Specific",
        _ => "Unknown",
    }
}

/// Get human-readable name for USB speed
pub fn speed_name(speed: super::UsbSpeed) -> &'static str {
    match speed {
        super::UsbSpeed::Low => "Low Speed (1.5 Mbps)",
        super::UsbSpeed::Full => "Full Speed (12 Mbps)",
        super::UsbSpeed::High => "High Speed (480 Mbps)",
        super::UsbSpeed::Super => "SuperSpeed (5 Gbps)",
        super::UsbSpeed::SuperPlus => "SuperSpeed+ (10 Gbps)",
        super::UsbSpeed::SuperPlus2 => "SuperSpeed+ (20 Gbps)",
    }
}

// =============================================================================
// USB DEBUG HELPERS
// =============================================================================

pub fn dump_device_descriptor(desc: &super::DeviceDescriptor) {
    // Copy packed fields to avoid unaligned access
    let usb_version = { desc.usb_version };
    let device_class = { desc.device_class };
    let device_subclass = { desc.device_subclass };
    let device_protocol = { desc.device_protocol };
    let max_packet_size = { desc.max_packet_size };
    let vendor_id = { desc.vendor_id };
    let product_id = { desc.product_id };
    let device_version = { desc.device_version };
    let num_configurations = { desc.num_configurations };

    crate::log::debug!("  USB Version: {}.{}", usb_version >> 8, usb_version & 0xFF);
    crate::log::debug!("  Device Class: {} ({:02x})", class_name(device_class), device_class);
    crate::log::debug!("  Subclass: {:02x}, Protocol: {:02x}", device_subclass, device_protocol);
    crate::log::debug!("  Max Packet Size: {}", max_packet_size);
    crate::log::debug!("  Vendor ID: {:04x}, Product ID: {:04x}", vendor_id, product_id);
    crate::log::debug!("  Device Version: {}.{}", device_version >> 8, device_version & 0xFF);
    crate::log::debug!("  Configurations: {}", num_configurations);
}

pub fn dump_configuration_descriptor(desc: &super::ConfigurationDescriptor) {
    // Copy packed fields to avoid unaligned access
    let total_length = { desc.total_length };
    let num_interfaces = { desc.num_interfaces };
    let configuration_value = { desc.configuration_value };
    let attributes = { desc.attributes };
    let max_power = { desc.max_power };

    crate::log::debug!("  Total Length: {}", total_length);
    crate::log::debug!("  Interfaces: {}", num_interfaces);
    crate::log::debug!("  Configuration Value: {}", configuration_value);
    crate::log::debug!("  Attributes: {:02x}", attributes);
    crate::log::debug!("  Max Power: {} mA", max_power as u32 * 2);
}

pub fn dump_interface_descriptor(desc: &super::InterfaceDescriptor) {
    crate::log::debug!("    Interface {}, Alt {}", desc.interface_number, desc.alternate_setting);
    crate::log::debug!("    Class: {} ({:02x})", class_name(desc.interface_class), desc.interface_class);
    crate::log::debug!("    Subclass: {:02x}, Protocol: {:02x}", desc.interface_subclass, desc.interface_protocol);
    crate::log::debug!("    Endpoints: {}", desc.num_endpoints);
}

pub fn dump_endpoint_descriptor(desc: &super::EndpointDescriptor) {
    let dir = if desc.direction() == super::EndpointDirection::In { "IN" } else { "OUT" };
    let typ = match desc.transfer_type() {
        super::EndpointType::Control => "Control",
        super::EndpointType::Isochronous => "Isochronous",
        super::EndpointType::Bulk => "Bulk",
        super::EndpointType::Interrupt => "Interrupt",
    };
    // Copy packed fields to avoid unaligned access
    let number = desc.number();
    let max_packet_size = { desc.max_packet_size };
    let interval = { desc.interval };

    crate::log::debug!("      EP{} {} {}, MaxPacket: {}, Interval: {}",
        number, dir, typ, max_packet_size, interval);
}
