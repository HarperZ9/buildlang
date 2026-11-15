// ===============================================================================
// BUILDOS KERNEL - DEVICE MAPPER LINEAR TARGET
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Linear Target Implementation
//!
//! Maps a contiguous range of sectors to an underlying device.

// Main implementation is in target.rs
// This module provides additional utilities

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use super::{DmError, TableEntry};
use super::table::TableBuilder;

/// Create a linear mapping for a single device
pub fn create_linear_device(
    name: &str,
    device: &str,
    offset: u64,
    size: u64,
) -> Result<(), DmError> {
    let table = TableBuilder::new()
        .linear(device, offset, size)
        .build()?;

    super::create_device(name, table)?;
    Ok(())
}

/// Concatenate multiple devices linearly
pub fn create_concat_device(
    name: &str,
    devices: &[(&str, u64, u64)], // (device, offset, size)
) -> Result<(), DmError> {
    let mut builder = TableBuilder::new();

    for (device, offset, size) in devices {
        builder = builder.linear(device, *offset, *size);
    }

    let table = builder.build()?;
    super::create_device(name, table)?;
    Ok(())
}

/// Resize a linear device by adding/removing from the end
pub fn resize_linear_device(
    name: &str,
    new_size: u64,
) -> Result<(), DmError> {
    // Get current table
    let entries = super::get_device_table(name)
        .ok_or(DmError::DeviceNotFound)?;

    if entries.is_empty() {
        return Err(DmError::InvalidTable);
    }

    // Suspend device
    super::suspend_device(name)?;

    // Create new table with adjusted last entry
    let mut new_entries = entries.clone();
    let current_size: u64 = new_entries.iter().map(|e| e.num_sectors).sum();
    let last = new_entries.last_mut().unwrap();

    if new_size > current_size {
        // Growing
        last.num_sectors += new_size - current_size;
    } else if new_size < current_size {
        // Shrinking
        let diff = current_size - new_size;
        if diff > last.num_sectors {
            return Err(DmError::InvalidArgument);
        }
        last.num_sectors -= diff;
    }

    // Build new table
    let mut builder = TableBuilder::new();
    for entry in &new_entries {
        if entry.target_type == "linear" {
            let parts: Vec<&str> = entry.target_args.split_whitespace().collect();
            if parts.len() >= 2 {
                let offset: u64 = parts[1].parse().unwrap_or(0);
                builder = builder.linear(parts[0], offset, entry.num_sectors);
            }
        }
    }

    let table = builder.build()?;
    super::load_table(name, table)?;
    super::resume_device(name)?;

    Ok(())
}

/// Split a device at a sector boundary
pub fn split_at_sector(
    table: &[TableEntry],
    sector: u64,
) -> (Vec<TableEntry>, Vec<TableEntry>) {
    let mut before = Vec::new();
    let mut after = Vec::new();
    let mut current_sector = 0u64;

    for entry in table {
        let entry_end = current_sector + entry.num_sectors;

        if entry_end <= sector {
            // Entirely before split point
            before.push(entry.clone());
        } else if current_sector >= sector {
            // Entirely after split point
            after.push(TableEntry {
                start_sector: entry.start_sector - sector,
                ..entry.clone()
            });
        } else {
            // Straddles split point
            let before_len = sector - current_sector;
            let after_len = entry_end - sector;

            before.push(TableEntry {
                num_sectors: before_len,
                ..entry.clone()
            });

            after.push(TableEntry {
                start_sector: 0,
                num_sectors: after_len,
                target_type: entry.target_type.clone(),
                target_args: adjust_linear_offset(&entry.target_args, before_len),
            });
        }

        current_sector = entry_end;
    }

    (before, after)
}

/// Adjust linear target offset
fn adjust_linear_offset(args: &str, offset_add: u64) -> String {
    let parts: Vec<&str> = args.split_whitespace().collect();
    if parts.len() >= 2 {
        let current_offset: u64 = parts[1].parse().unwrap_or(0);
        alloc::format!("{} {}", parts[0], current_offset + offset_add)
    } else {
        args.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_at_sector() {
        let entries = vec![
            TableEntry {
                start_sector: 0,
                num_sectors: 1000,
                target_type: "linear".to_string(),
                target_args: "/dev/sda 0".to_string(),
            },
            TableEntry {
                start_sector: 1000,
                num_sectors: 1000,
                target_type: "linear".to_string(),
                target_args: "/dev/sdb 0".to_string(),
            },
        ];

        let (before, after) = split_at_sector(&entries, 500);

        assert_eq!(before.len(), 1);
        assert_eq!(before[0].num_sectors, 500);

        assert_eq!(after.len(), 2);
        assert_eq!(after[0].num_sectors, 500);
        assert_eq!(after[1].num_sectors, 1000);
    }
}
