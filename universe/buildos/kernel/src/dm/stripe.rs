// ===============================================================================
// BUILDOS KERNEL - DEVICE MAPPER STRIPE TARGET
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Stripe Target Implementation
//!
//! Provides RAID-0 style striping across multiple devices.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::math::F64Ext;
use super::{DmError, SECTOR_SIZE};
use super::table::TableBuilder;

/// Stripe device configuration
#[derive(Clone, Debug)]
pub struct StripeConfig {
    /// Chunk size in sectors
    pub chunk_size: u64,
    /// Stripe members
    pub stripes: Vec<StripeMember>,
}

/// A member of a stripe set
#[derive(Clone, Debug)]
pub struct StripeMember {
    /// Device path
    pub device: String,
    /// Start offset on device
    pub offset: u64,
}

impl StripeConfig {
    /// Create new stripe configuration
    pub fn new(chunk_size: u64) -> Self {
        Self {
            chunk_size,
            stripes: Vec::new(),
        }
    }

    /// Add a stripe member
    pub fn add_member(&mut self, device: &str, offset: u64) {
        self.stripes.push(StripeMember {
            device: device.to_string(),
            offset,
        });
    }

    /// Calculate usable size (min of all members)
    pub fn usable_size(&self, member_sizes: &[u64]) -> u64 {
        if member_sizes.is_empty() || self.stripes.is_empty() {
            return 0;
        }

        let min_size = member_sizes.iter().min().copied().unwrap_or(0);
        // Round down to chunk boundary
        let chunks_per_member = min_size / self.chunk_size;
        chunks_per_member * self.chunk_size * self.stripes.len() as u64
    }

    /// Map sector to stripe member and local sector
    pub fn map_sector(&self, sector: u64) -> Option<(usize, u64)> {
        if self.stripes.is_empty() || self.chunk_size == 0 {
            return None;
        }

        let chunk = sector / self.chunk_size;
        let stripe_idx = (chunk % self.stripes.len() as u64) as usize;
        let stripe_chunk = chunk / self.stripes.len() as u64;
        let offset_in_chunk = sector % self.chunk_size;

        let local_sector = self.stripes[stripe_idx].offset
            + stripe_chunk * self.chunk_size
            + offset_in_chunk;

        Some((stripe_idx, local_sector))
    }
}

/// Create a striped device
pub fn create_stripe_device(
    name: &str,
    config: &StripeConfig,
    total_sectors: u64,
) -> Result<(), DmError> {
    if config.stripes.is_empty() {
        return Err(DmError::InvalidArgument);
    }

    let stripes: Vec<(&str, u64)> = config
        .stripes
        .iter()
        .map(|s| (s.device.as_str(), s.offset))
        .collect();

    let table = TableBuilder::new()
        .striped(config.chunk_size, &stripes, total_sectors)
        .build()?;

    super::create_device(name, table)?;
    Ok(())
}

/// Optimal chunk size calculation based on device characteristics
pub fn calculate_optimal_chunk_size(
    _stripe_count: usize,
    io_size: u64,
    device_io_opt: u64,
) -> u64 {
    // Default chunk sizes (in sectors)
    const MIN_CHUNK: u64 = 8;      // 4KB
    const DEFAULT_CHUNK: u64 = 128; // 64KB
    const MAX_CHUNK: u64 = 4096;    // 2MB

    let mut chunk = if device_io_opt > 0 {
        device_io_opt / SECTOR_SIZE
    } else if io_size > 0 {
        io_size / SECTOR_SIZE
    } else {
        DEFAULT_CHUNK
    };

    // Round up to power of 2
    chunk = chunk.next_power_of_two();

    // Clamp to valid range
    chunk.clamp(MIN_CHUNK, MAX_CHUNK)
}

/// Stripe statistics
#[derive(Clone, Debug, Default)]
pub struct StripeStats {
    pub reads: u64,
    pub writes: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub per_member_reads: Vec<u64>,
    pub per_member_writes: Vec<u64>,
}

impl StripeStats {
    pub fn new(member_count: usize) -> Self {
        Self {
            per_member_reads: vec![0; member_count],
            per_member_writes: vec![0; member_count],
            ..Default::default()
        }
    }

    pub fn record_read(&mut self, member: usize, bytes: u64) {
        self.reads += 1;
        self.read_bytes += bytes;
        if member < self.per_member_reads.len() {
            self.per_member_reads[member] += 1;
        }
    }

    pub fn record_write(&mut self, member: usize, bytes: u64) {
        self.writes += 1;
        self.write_bytes += bytes;
        if member < self.per_member_writes.len() {
            self.per_member_writes[member] += 1;
        }
    }

    /// Calculate balance ratio (1.0 = perfectly balanced)
    pub fn balance_ratio(&self) -> f64 {
        let total_ops: u64 = self.per_member_reads.iter().chain(self.per_member_writes.iter()).sum();
        if total_ops == 0 {
            return 1.0;
        }

        let member_count = self.per_member_reads.len();
        if member_count == 0 {
            return 1.0;
        }

        let expected = total_ops / member_count as u64;
        let mut variance: f64 = 0.0;

        for i in 0..member_count {
            let ops = self.per_member_reads[i] + self.per_member_writes[i];
            let diff = ops as f64 - expected as f64;
            variance += diff * diff;
        }

        let std_dev = (variance / member_count as f64).sqrt();
        let cv = std_dev / expected as f64;

        1.0 / (1.0 + cv)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stripe_mapping() {
        let mut config = StripeConfig::new(8); // 8 sector chunks
        config.add_member("/dev/sda", 0);
        config.add_member("/dev/sdb", 0);

        // Sector 0-7 should go to member 0
        let (member, sector) = config.map_sector(0).unwrap();
        assert_eq!(member, 0);
        assert_eq!(sector, 0);

        // Sector 8-15 should go to member 1
        let (member, sector) = config.map_sector(8).unwrap();
        assert_eq!(member, 1);
        assert_eq!(sector, 0);

        // Sector 16-23 should go back to member 0
        let (member, sector) = config.map_sector(16).unwrap();
        assert_eq!(member, 0);
        assert_eq!(sector, 8);
    }

    #[test]
    fn test_optimal_chunk_size() {
        let chunk = calculate_optimal_chunk_size(4, 0, 0);
        assert_eq!(chunk, 128); // Default

        let chunk = calculate_optimal_chunk_size(4, 4096, 0);
        assert_eq!(chunk, 8); // 4KB = 8 sectors

        let chunk = calculate_optimal_chunk_size(4, 0, 262144);
        assert_eq!(chunk, 512); // 256KB = 512 sectors
    }
}
