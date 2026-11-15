// ===============================================================================
// BUILDOS KERNEL - DEVICE MAPPER TABLE
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Device Mapper Table
//!
//! Manages the mapping table for a device.

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use super::target::{DmTarget, StatusType};
use super::{DmError, TableEntry};

/// A mapping table entry
struct TableRow {
    /// Start sector of this region
    start: u64,
    /// Length in sectors
    len: u64,
    /// Target handling this region
    target: Box<dyn DmTarget>,
    /// Target arguments string
    args: String,
}

/// Device mapper table
pub struct DmTable {
    /// Table rows (sorted by start sector)
    rows: Vec<TableRow>,
    /// Total size in sectors
    size: u64,
    /// Mode (read-write, read-only)
    mode: TableMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableMode {
    ReadWrite,
    ReadOnly,
}

impl DmTable {
    /// Create empty table
    pub fn empty() -> Self {
        Self {
            rows: Vec::new(),
            size: 0,
            mode: TableMode::ReadWrite,
        }
    }

    /// Create table from entries
    pub fn from_entries(entries: Vec<(u64, u64, &str, &str)>) -> Result<Self, DmError> {
        let mut table = Self::empty();

        for (start, len, target_type, args) in entries {
            table.add_target(start, len, target_type, args)?;
        }

        table.validate()?;
        Ok(table)
    }

    /// Add a target to the table
    pub fn add_target(
        &mut self,
        start: u64,
        len: u64,
        target_type: &str,
        args: &str,
    ) -> Result<(), DmError> {
        // Get target type
        let mut target = super::get_target_type(target_type)
            .ok_or(DmError::UnknownTarget)?;

        // Parse arguments
        let arg_parts: Vec<&str> = args.split_whitespace().collect();
        target.ctr(&arg_parts)?;

        // Insert in sorted order
        let row = TableRow {
            start,
            len,
            target,
            args: args.to_string(),
        };

        let pos = self.rows.iter().position(|r| r.start > start);
        match pos {
            Some(i) => self.rows.insert(i, row),
            None => self.rows.push(row),
        }

        // Update size
        let end = start + len;
        if end > self.size {
            self.size = end;
        }

        Ok(())
    }

    /// Validate table
    pub fn validate(&self) -> Result<(), DmError> {
        if self.rows.is_empty() {
            return Ok(()); // Empty table is valid
        }

        // Check for gaps and overlaps
        let mut expected_start = 0u64;

        for row in &self.rows {
            if row.start != expected_start {
                return Err(DmError::InvalidTable);
            }
            expected_start = row.start + row.len;
        }

        Ok(())
    }

    /// Get total size in sectors
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Get number of targets
    pub fn target_count(&self) -> u32 {
        self.rows.len() as u32
    }

    /// Find target for a sector
    pub fn find_target(&self, sector: u64) -> Option<&dyn DmTarget> {
        for row in &self.rows {
            if sector >= row.start && sector < row.start + row.len {
                return Some(row.target.as_ref());
            }
        }
        None
    }

    /// Find target for a sector (mutable)
    pub fn find_target_mut(&mut self, sector: u64) -> Option<&mut dyn DmTarget> {
        for row in &mut self.rows {
            if sector >= row.start && sector < row.start + row.len {
                return Some(row.target.as_mut());
            }
        }
        None
    }

    /// Get all targets
    pub fn targets(&self) -> impl Iterator<Item = &dyn DmTarget> {
        self.rows.iter().map(|r| r.target.as_ref())
    }

    /// Get all targets (mutable)
    pub fn targets_mut(&mut self) -> impl Iterator<Item = &mut Box<dyn DmTarget>> {
        self.rows.iter_mut().map(|r| &mut r.target)
    }

    /// Get table entries for display
    pub fn entries(&self) -> Vec<TableEntry> {
        self.rows
            .iter()
            .map(|row| TableEntry {
                start_sector: row.start,
                num_sectors: row.len,
                target_type: row.target.name().to_string(),
                target_args: row.args.clone(),
            })
            .collect()
    }

    /// Get status
    pub fn status(&self, status_type: StatusType) -> String {
        let mut output = String::new();

        for row in &self.rows {
            let target_status = row.target.status(status_type);
            output.push_str(&alloc::format!(
                "{} {} {} {}\n",
                row.start,
                row.len,
                row.target.name(),
                target_status
            ));
        }

        output
    }

    /// Get dependencies (underlying devices)
    pub fn deps(&self) -> Vec<(u32, u32)> {
        let mut deps = Vec::new();

        for row in &self.rows {
            for _dev_info in row.target.iterate_devices() {
                // Parse device major/minor from path
                // Simplified: would need actual device lookup
                deps.push((0, 0));
            }
        }

        deps.sort();
        deps.dedup();
        deps
    }

    /// Set table mode
    pub fn set_mode(&mut self, mode: TableMode) {
        self.mode = mode;
    }

    /// Get table mode
    pub fn mode(&self) -> TableMode {
        self.mode
    }

    /// Check if table supports discards
    pub fn supports_discard(&self) -> bool {
        self.rows.iter().all(|r| r.target.supports_discard())
    }

    /// Check if table supports secure erase
    pub fn supports_secure_erase(&self) -> bool {
        self.rows.iter().all(|r| r.target.supports_secure_erase())
    }

    /// Clear the table
    pub fn clear(&mut self) {
        // Call destructors
        for row in &mut self.rows {
            row.target.dtr();
        }
        self.rows.clear();
        self.size = 0;
    }
}

impl Drop for DmTable {
    fn drop(&mut self) {
        self.clear();
    }
}

/// Table builder
pub struct TableBuilder {
    entries: Vec<(u64, u64, String, String)>,
    mode: TableMode,
}

impl TableBuilder {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            mode: TableMode::ReadWrite,
        }
    }

    /// Add a linear target
    pub fn linear(mut self, device: &str, offset: u64, len: u64) -> Self {
        let start = self.next_start();
        let args = alloc::format!("{} {}", device, offset);
        self.entries.push((start, len, "linear".to_string(), args));
        self
    }

    /// Add a striped target
    pub fn striped(mut self, chunk_size: u64, stripes: &[(&str, u64)], len: u64) -> Self {
        let start = self.next_start();
        let mut args = alloc::format!("{} {}", stripes.len(), chunk_size);
        for (dev, offset) in stripes {
            args.push_str(&alloc::format!(" {} {}", dev, offset));
        }
        self.entries.push((start, len, "striped".to_string(), args));
        self
    }

    /// Add a zero target
    pub fn zero(mut self, len: u64) -> Self {
        let start = self.next_start();
        self.entries.push((start, len, "zero".to_string(), String::new()));
        self
    }

    /// Add an error target
    pub fn error(mut self, len: u64) -> Self {
        let start = self.next_start();
        self.entries.push((start, len, "error".to_string(), String::new()));
        self
    }

    /// Set read-only mode
    pub fn read_only(mut self) -> Self {
        self.mode = TableMode::ReadOnly;
        self
    }

    /// Build the table
    pub fn build(self) -> Result<DmTable, DmError> {
        let entries: Vec<_> = self.entries
            .iter()
            .map(|(s, l, t, a)| (*s, *l, t.as_str(), a.as_str()))
            .collect();
        let mut table = DmTable::from_entries(entries)?;
        table.set_mode(self.mode);
        Ok(table)
    }

    fn next_start(&self) -> u64 {
        self.entries
            .last()
            .map(|(s, l, _, _)| s + l)
            .unwrap_or(0)
    }
}

impl Default for TableBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse table from dmsetup format
pub fn parse_table(input: &str) -> Result<DmTable, DmError> {
    let mut table = DmTable::empty();

    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.splitn(4, ' ').collect();
        if parts.len() < 3 {
            return Err(DmError::InvalidTable);
        }

        let start: u64 = parts[0].parse().map_err(|_| DmError::InvalidTable)?;
        let len: u64 = parts[1].parse().map_err(|_| DmError::InvalidTable)?;
        let target_type = parts[2];
        let args = if parts.len() > 3 { parts[3] } else { "" };

        table.add_target(start, len, target_type, args)?;
    }

    table.validate()?;
    Ok(table)
}

/// Format table to dmsetup format
pub fn format_table(table: &DmTable) -> String {
    let mut output = String::new();

    for entry in table.entries() {
        output.push_str(&alloc::format!(
            "{} {} {} {}\n",
            entry.start_sector,
            entry.num_sectors,
            entry.target_type,
            entry.target_args
        ));
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_builder() {
        let table = TableBuilder::new()
            .linear("/dev/sda1", 0, 1024)
            .linear("/dev/sda2", 0, 1024)
            .build()
            .unwrap();

        assert_eq!(table.size(), 2048);
        assert_eq!(table.target_count(), 2);
    }

    #[test]
    fn test_parse_table() {
        let input = "0 1024 linear /dev/sda1 0\n1024 1024 linear /dev/sda2 0";
        let table = parse_table(input).unwrap();

        assert_eq!(table.size(), 2048);
        assert_eq!(table.target_count(), 2);
    }
}
