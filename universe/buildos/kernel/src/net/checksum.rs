// ===============================================================================
// BUILDOS KERNEL - NETWORK CHECKSUM UTILITIES
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Internet checksum (RFC 1071) implementation.

/// Calculate Internet checksum (one's complement sum)
///
/// This is used for IP, ICMP, TCP, and UDP checksums.
pub fn internet_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;

    // Sum 16-bit words
    while i + 1 < data.len() {
        let word = u16::from_be_bytes([data[i], data[i + 1]]);
        sum = sum.wrapping_add(word as u32);
        i += 2;
    }

    // Handle odd byte
    if i < data.len() {
        sum = sum.wrapping_add((data[i] as u32) << 8);
    }

    // Fold 32-bit sum to 16 bits
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    // One's complement
    !(sum as u16)
}

/// Calculate checksum with pseudo header (for TCP/UDP)
pub fn checksum_with_pseudo(pseudo: &[u8], data: &[u8]) -> u16 {
    let mut sum: u32 = 0;

    // Sum pseudo header
    let mut i = 0;
    while i + 1 < pseudo.len() {
        let word = u16::from_be_bytes([pseudo[i], pseudo[i + 1]]);
        sum = sum.wrapping_add(word as u32);
        i += 2;
    }
    if i < pseudo.len() {
        sum = sum.wrapping_add((pseudo[i] as u32) << 8);
    }

    // Sum data
    i = 0;
    while i + 1 < data.len() {
        let word = u16::from_be_bytes([data[i], data[i + 1]]);
        sum = sum.wrapping_add(word as u32);
        i += 2;
    }
    if i < data.len() {
        sum = sum.wrapping_add((data[i] as u32) << 8);
    }

    // Fold 32-bit sum to 16 bits
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    // One's complement
    !(sum as u16)
}

/// Verify checksum (returns true if valid)
pub fn verify_checksum(data: &[u8]) -> bool {
    internet_checksum(data) == 0
}

/// Incremental checksum update (RFC 1624)
///
/// Used when only a few fields change (e.g., TTL decrement in routing).
pub fn update_checksum(old_checksum: u16, old_value: u16, new_value: u16) -> u16 {
    let old_check = !old_checksum as u32;
    let old_val = old_value as u32;
    let new_val = new_value as u32;

    let mut sum = old_check.wrapping_sub(old_val).wrapping_add(new_val);

    // Handle borrow
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    !(sum as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ip_checksum() {
        // Example IPv4 header from RFC 1071
        let header: [u8; 20] = [
            0x45, 0x00, 0x00, 0x73, // Version, IHL, TOS, Total Length
            0x00, 0x00, 0x40, 0x00, // ID, Flags, Fragment Offset
            0x40, 0x11, 0x00, 0x00, // TTL, Protocol, Checksum (zero for calculation)
            0xC0, 0xA8, 0x00, 0x01, // Source IP (192.168.0.1)
            0xC0, 0xA8, 0x00, 0xC7, // Dest IP (192.168.0.199)
        ];

        let checksum = internet_checksum(&header);
        // Re-calculate with checksum included should give 0
        let mut header_with_check = header;
        header_with_check[10] = (checksum >> 8) as u8;
        header_with_check[11] = checksum as u8;
        assert_eq!(internet_checksum(&header_with_check), 0);
    }
}
