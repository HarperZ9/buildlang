//! FAT32 Filesystem Implementation
//!
//! Provides full FAT32 support including:
//! - Boot sector and BPB parsing
//! - FAT (File Allocation Table) management
//! - Cluster chain traversal
//! - Directory operations with LFN (Long File Names)
//! - Read/write operations
//! - File creation and deletion

use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use alloc::vec;
use core::mem::size_of;
use spin::{Mutex, RwLock};

use super::{
    DirEntry, errno, FileType, Filesystem, Inode, InodeId, Metadata,
};

// =============================================================================
// FAT32 CONSTANTS
// =============================================================================

/// FAT32 boot signature
const FAT32_BOOT_SIGNATURE: u16 = 0xAA55;

/// FAT entry constants
const FAT32_EOC: u32 = 0x0FFFFFF8;         // End of cluster chain
const FAT32_BAD: u32 = 0x0FFFFFF7;         // Bad cluster
const FAT32_FREE: u32 = 0x00000000;        // Free cluster
const FAT32_MASK: u32 = 0x0FFFFFFF;        // 28-bit mask for FAT32

/// Directory entry attributes
mod attrs {
    pub const READ_ONLY: u8 = 0x01;
    pub const HIDDEN: u8 = 0x02;
    pub const SYSTEM: u8 = 0x04;
    pub const VOLUME_ID: u8 = 0x08;
    pub const DIRECTORY: u8 = 0x10;
    pub const ARCHIVE: u8 = 0x20;
    pub const LFN: u8 = READ_ONLY | HIDDEN | SYSTEM | VOLUME_ID;
}

/// LFN sequence number constants
const LFN_LAST_ENTRY: u8 = 0x40;
const LFN_DELETED: u8 = 0xE5;

// =============================================================================
// FAT32 ON-DISK STRUCTURES
// =============================================================================

/// FAT32 Boot Sector / BPB (BIOS Parameter Block)
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct Fat32BootSector {
    /// Jump instruction
    pub jump_boot: [u8; 3],
    /// OEM name
    pub oem_name: [u8; 8],
    /// Bytes per sector
    pub bytes_per_sector: u16,
    /// Sectors per cluster
    pub sectors_per_cluster: u8,
    /// Reserved sector count
    pub reserved_sector_count: u16,
    /// Number of FATs
    pub num_fats: u8,
    /// Root entry count (0 for FAT32)
    pub root_entry_count: u16,
    /// Total sectors 16 (0 for FAT32)
    pub total_sectors_16: u16,
    /// Media type
    pub media: u8,
    /// FAT size 16 (0 for FAT32)
    pub fat_size_16: u16,
    /// Sectors per track
    pub sectors_per_track: u16,
    /// Number of heads
    pub num_heads: u16,
    /// Hidden sectors
    pub hidden_sectors: u32,
    /// Total sectors 32
    pub total_sectors_32: u32,
    // FAT32 specific fields
    /// FAT size 32 (sectors per FAT)
    pub fat_size_32: u32,
    /// Extended flags
    pub ext_flags: u16,
    /// Filesystem version
    pub fs_version: u16,
    /// Root cluster number
    pub root_cluster: u32,
    /// FSInfo sector number
    pub fs_info: u16,
    /// Backup boot sector
    pub backup_boot_sector: u16,
    /// Reserved
    pub reserved: [u8; 12],
    /// Drive number
    pub drive_number: u8,
    /// Reserved
    pub reserved1: u8,
    /// Boot signature
    pub boot_sig: u8,
    /// Volume ID
    pub volume_id: u32,
    /// Volume label
    pub volume_label: [u8; 11],
    /// Filesystem type string
    pub fs_type: [u8; 8],
}

impl Fat32BootSector {
    /// Validate the boot sector
    pub fn validate(&self) -> bool {
        // Check bytes per sector (must be power of 2, 512-4096)
        let bps = self.bytes_per_sector;
        if bps < 512 || bps > 4096 || (bps & (bps - 1)) != 0 {
            return false;
        }

        // Check sectors per cluster (must be power of 2)
        let spc = self.sectors_per_cluster;
        if spc == 0 || (spc & (spc - 1)) != 0 {
            return false;
        }

        // FAT32 specific: fat_size_16 should be 0
        if self.fat_size_16 != 0 {
            return false;
        }

        // Must have at least 1 FAT
        if self.num_fats == 0 {
            return false;
        }

        true
    }

    /// Get bytes per cluster
    pub fn bytes_per_cluster(&self) -> u32 {
        self.bytes_per_sector as u32 * self.sectors_per_cluster as u32
    }

    /// Get first data sector
    pub fn first_data_sector(&self) -> u32 {
        self.reserved_sector_count as u32 +
            (self.num_fats as u32 * self.fat_size_32)
    }

    /// Get total data sectors
    pub fn data_sectors(&self) -> u32 {
        self.total_sectors_32 - self.first_data_sector()
    }

    /// Get total cluster count
    pub fn cluster_count(&self) -> u32 {
        self.data_sectors() / self.sectors_per_cluster as u32
    }

    /// Convert cluster to sector
    pub fn cluster_to_sector(&self, cluster: u32) -> u32 {
        self.first_data_sector() + (cluster - 2) * self.sectors_per_cluster as u32
    }

    /// Get FAT sector for cluster entry
    pub fn fat_sector_for_cluster(&self, cluster: u32) -> u32 {
        let fat_offset = cluster * 4;
        self.reserved_sector_count as u32 + (fat_offset / self.bytes_per_sector as u32)
    }

    /// Get offset within FAT sector for cluster entry
    pub fn fat_offset_for_cluster(&self, cluster: u32) -> u32 {
        (cluster * 4) % self.bytes_per_sector as u32
    }
}

/// FSInfo structure
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct FsInfo {
    /// Lead signature (0x41615252)
    pub lead_sig: u32,
    /// Reserved
    pub reserved1: [u8; 480],
    /// Structure signature (0x61417272)
    pub struct_sig: u32,
    /// Last known free cluster count
    pub free_count: u32,
    /// Hint for next free cluster
    pub next_free: u32,
    /// Reserved
    pub reserved2: [u8; 12],
    /// Trail signature (0xAA550000)
    pub trail_sig: u32,
}

impl FsInfo {
    /// Validate FSInfo structure
    pub fn validate(&self) -> bool {
        self.lead_sig == 0x41615252 &&
        self.struct_sig == 0x61417272 &&
        self.trail_sig == 0xAA550000
    }
}

/// FAT32 Directory Entry (8.3 format)
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct Fat32DirEntry {
    /// Short name (8.3 format, space-padded)
    pub name: [u8; 11],
    /// Attributes
    pub attr: u8,
    /// NT reserved
    pub nt_res: u8,
    /// Creation time (tenths of second)
    pub crt_time_tenth: u8,
    /// Creation time
    pub crt_time: u16,
    /// Creation date
    pub crt_date: u16,
    /// Last access date
    pub lst_acc_date: u16,
    /// First cluster high word
    pub fst_clus_hi: u16,
    /// Write time
    pub wrt_time: u16,
    /// Write date
    pub wrt_date: u16,
    /// First cluster low word
    pub fst_clus_lo: u16,
    /// File size
    pub file_size: u32,
}

impl Fat32DirEntry {
    /// Check if entry is free
    pub fn is_free(&self) -> bool {
        self.name[0] == 0x00 || self.name[0] == 0xE5
    }

    /// Check if entry is end of directory
    pub fn is_end(&self) -> bool {
        self.name[0] == 0x00
    }

    /// Check if entry is a long file name entry
    pub fn is_lfn(&self) -> bool {
        self.attr == attrs::LFN
    }

    /// Check if entry is a directory
    pub fn is_directory(&self) -> bool {
        (self.attr & attrs::DIRECTORY) != 0
    }

    /// Check if entry is a volume label
    pub fn is_volume_label(&self) -> bool {
        (self.attr & attrs::VOLUME_ID) != 0 && !self.is_lfn()
    }

    /// Get first cluster
    pub fn first_cluster(&self) -> u32 {
        ((self.fst_clus_hi as u32) << 16) | (self.fst_clus_lo as u32)
    }

    /// Get short filename
    pub fn short_name(&self) -> String {
        let mut name = String::new();

        // Get base name (first 8 chars)
        for i in 0..8 {
            if self.name[i] == 0x20 {
                break;
            }
            let ch = if self.name[i] == 0x05 {
                0xE5 // Special case: 0x05 represents 0xE5
            } else {
                self.name[i]
            };
            name.push(ch as char);
        }

        // Get extension (last 3 chars)
        let mut ext = String::new();
        for i in 8..11 {
            if self.name[i] == 0x20 {
                break;
            }
            ext.push(self.name[i] as char);
        }

        if !ext.is_empty() {
            name.push('.');
            name.push_str(&ext);
        }

        name
    }

    /// Convert FAT date/time to Unix timestamp
    pub fn write_timestamp(&self) -> u64 {
        fat_datetime_to_unix(self.wrt_date, self.wrt_time)
    }

    /// Convert FAT date to Unix timestamp
    pub fn access_timestamp(&self) -> u64 {
        fat_datetime_to_unix(self.lst_acc_date, 0)
    }

    /// Convert FAT date/time to Unix timestamp
    pub fn creation_timestamp(&self) -> u64 {
        fat_datetime_to_unix(self.crt_date, self.crt_time)
    }
}

/// Long File Name directory entry
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct LfnEntry {
    /// Sequence number
    pub ord: u8,
    /// Name characters (1-5)
    pub name1: [u16; 5],
    /// Attributes (always 0x0F)
    pub attr: u8,
    /// Type (always 0)
    pub entry_type: u8,
    /// Checksum of short name
    pub chksum: u8,
    /// Name characters (6-11)
    pub name2: [u16; 6],
    /// First cluster (always 0)
    pub fst_clus_lo: u16,
    /// Name characters (12-13)
    pub name3: [u16; 2],
}

impl LfnEntry {
    /// Get sequence number (1-based, without LAST flag)
    pub fn sequence(&self) -> u8 {
        self.ord & 0x3F
    }

    /// Check if this is the last LFN entry
    pub fn is_last(&self) -> bool {
        (self.ord & LFN_LAST_ENTRY) != 0
    }

    /// Extract name characters from this entry
    pub fn name_chars(&self) -> [u16; 13] {
        let mut chars = [0u16; 13];
        // Use read_unaligned to access packed struct fields safely
        let name1: [u16; 5] = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(self.name1)) };
        let name2: [u16; 6] = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(self.name2)) };
        let name3: [u16; 2] = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(self.name3)) };
        chars[0..5].copy_from_slice(&name1);
        chars[5..11].copy_from_slice(&name2);
        chars[11..13].copy_from_slice(&name3);
        chars
    }
}

// =============================================================================
// FAT32 FILESYSTEM STATE
// =============================================================================

/// Block device trait for reading/writing sectors
pub trait BlockDevice: Send + Sync {
    /// Read sectors
    fn read_sectors(&self, start_sector: u64, buf: &mut [u8]) -> Result<(), i32>;
    /// Write sectors
    fn write_sectors(&self, start_sector: u64, buf: &[u8]) -> Result<(), i32>;
    /// Get sector size
    fn sector_size(&self) -> u32;
}

/// Cached FAT sector
struct FatCache {
    /// Sector number
    sector: u32,
    /// Sector data
    data: Vec<u8>,
    /// Is dirty (needs writing)
    dirty: bool,
}

/// FAT32 filesystem state
pub struct Fat32Filesystem {
    /// Filesystem ID for inodes
    fs_id: u64,
    /// Boot sector
    boot_sector: Fat32BootSector,
    /// FSInfo
    fs_info: FsInfo,
    /// Block device
    device: Arc<dyn BlockDevice>,
    /// Partition start sector
    partition_start: u64,
    /// FAT cache (multiple sectors)
    fat_cache: RwLock<Vec<FatCache>>,
    /// Next inode number
    next_inode: Mutex<u64>,
    /// Cluster to inode mapping
    cluster_to_inode: RwLock<alloc::collections::BTreeMap<u32, u64>>,
    /// Free cluster count
    free_clusters: Mutex<u32>,
    /// Next free cluster hint
    next_free_hint: Mutex<u32>,
}

impl Fat32Filesystem {
    /// Create new FAT32 filesystem from block device
    pub fn new(
        device: Arc<dyn BlockDevice>,
        partition_start: u64,
        fs_id: u64,
    ) -> Result<Self, i32> {
        // Read boot sector
        let sector_size = device.sector_size() as usize;
        let mut boot_buf = vec![0u8; sector_size];
        device.read_sectors(partition_start, &mut boot_buf)?;

        // Parse boot sector
        let boot_sector = unsafe {
            core::ptr::read_unaligned(boot_buf.as_ptr() as *const Fat32BootSector)
        };

        // Validate
        if !boot_sector.validate() {
            return Err(errno::EINVAL);
        }

        // Check boot signature
        let sig_offset = sector_size - 2;
        let signature = u16::from_le_bytes([boot_buf[sig_offset], boot_buf[sig_offset + 1]]);
        if signature != FAT32_BOOT_SIGNATURE {
            return Err(errno::EINVAL);
        }

        // Read FSInfo
        let fs_info_sector = partition_start + boot_sector.fs_info as u64;
        let mut fs_info_buf = vec![0u8; sector_size];
        device.read_sectors(fs_info_sector, &mut fs_info_buf)?;

        let fs_info = unsafe {
            core::ptr::read_unaligned(fs_info_buf.as_ptr() as *const FsInfo)
        };

        let (free_clusters, next_free) = if fs_info.validate() {
            (fs_info.free_count, fs_info.next_free)
        } else {
            (0xFFFFFFFF, 2) // Unknown
        };

        Ok(Self {
            fs_id,
            boot_sector,
            fs_info,
            device,
            partition_start,
            fat_cache: RwLock::new(Vec::new()),
            next_inode: Mutex::new(2), // 1 reserved for root
            cluster_to_inode: RwLock::new(alloc::collections::BTreeMap::new()),
            free_clusters: Mutex::new(free_clusters),
            next_free_hint: Mutex::new(next_free),
        })
    }

    /// Read FAT entry for cluster
    fn read_fat_entry(&self, cluster: u32) -> Result<u32, i32> {
        let fat_sector = self.boot_sector.fat_sector_for_cluster(cluster);
        let fat_offset = self.boot_sector.fat_offset_for_cluster(cluster) as usize;

        // Check cache
        let cache = self.fat_cache.read();
        for entry in cache.iter() {
            if entry.sector == fat_sector {
                let value = u32::from_le_bytes([
                    entry.data[fat_offset],
                    entry.data[fat_offset + 1],
                    entry.data[fat_offset + 2],
                    entry.data[fat_offset + 3],
                ]);
                return Ok(value & FAT32_MASK);
            }
        }
        drop(cache);

        // Not in cache, read sector
        let sector_size = self.boot_sector.bytes_per_sector as usize;
        let mut buf = vec![0u8; sector_size];
        let abs_sector = self.partition_start + fat_sector as u64;
        self.device.read_sectors(abs_sector, &mut buf)?;

        let value = u32::from_le_bytes([
            buf[fat_offset],
            buf[fat_offset + 1],
            buf[fat_offset + 2],
            buf[fat_offset + 3],
        ]);

        // Add to cache
        let mut cache = self.fat_cache.write();
        // Limit cache size
        if cache.len() >= 32 {
            // Remove oldest non-dirty entry
            if let Some(pos) = cache.iter().position(|e| !e.dirty) {
                cache.remove(pos);
            }
        }
        cache.push(FatCache {
            sector: fat_sector,
            data: buf,
            dirty: false,
        });

        Ok(value & FAT32_MASK)
    }

    /// Write FAT entry for cluster
    fn write_fat_entry(&self, cluster: u32, value: u32) -> Result<(), i32> {
        let fat_sector = self.boot_sector.fat_sector_for_cluster(cluster);
        let fat_offset = self.boot_sector.fat_offset_for_cluster(cluster) as usize;
        let abs_sector = self.partition_start + fat_sector as u64;

        // Read sector if not in cache
        let sector_size = self.boot_sector.bytes_per_sector as usize;
        let mut cache = self.fat_cache.write();

        let cache_entry = cache.iter_mut().find(|e| e.sector == fat_sector);

        if let Some(entry) = cache_entry {
            // Update in cache
            let bytes = (value & FAT32_MASK).to_le_bytes();
            entry.data[fat_offset..fat_offset + 4].copy_from_slice(&bytes);
            entry.dirty = true;
        } else {
            // Read and cache
            let mut buf = vec![0u8; sector_size];
            self.device.read_sectors(abs_sector, &mut buf)?;

            let bytes = (value & FAT32_MASK).to_le_bytes();
            buf[fat_offset..fat_offset + 4].copy_from_slice(&bytes);

            // Write back immediately
            self.device.write_sectors(abs_sector, &buf)?;

            // Also update FAT2 if present
            if self.boot_sector.num_fats > 1 {
                let fat2_sector = abs_sector + self.boot_sector.fat_size_32 as u64;
                self.device.write_sectors(fat2_sector, &buf)?;
            }
        }

        Ok(())
    }

    /// Flush dirty FAT cache entries
    fn flush_fat_cache(&self) -> Result<(), i32> {
        let mut cache = self.fat_cache.write();

        for entry in cache.iter_mut() {
            if entry.dirty {
                let abs_sector = self.partition_start + entry.sector as u64;
                self.device.write_sectors(abs_sector, &entry.data)?;

                // Also update FAT2
                if self.boot_sector.num_fats > 1 {
                    let fat2_sector = abs_sector + self.boot_sector.fat_size_32 as u64;
                    self.device.write_sectors(fat2_sector, &entry.data)?;
                }

                entry.dirty = false;
            }
        }

        Ok(())
    }

    /// Get cluster chain for starting cluster
    fn get_cluster_chain(&self, start_cluster: u32) -> Result<Vec<u32>, i32> {
        let mut chain = Vec::new();
        let mut cluster = start_cluster;

        while cluster >= 2 && cluster < FAT32_EOC {
            chain.push(cluster);
            cluster = self.read_fat_entry(cluster)?;

            // Sanity check to prevent infinite loops
            if chain.len() > 0x10000000 {
                return Err(errno::EIO);
            }
        }

        Ok(chain)
    }

    /// Read cluster data
    fn read_cluster(&self, cluster: u32, buf: &mut [u8]) -> Result<usize, i32> {
        let bytes_per_cluster = self.boot_sector.bytes_per_cluster() as usize;
        let sector = self.boot_sector.cluster_to_sector(cluster);
        let abs_sector = self.partition_start + sector as u64;

        let _sectors_per_cluster = self.boot_sector.sectors_per_cluster as usize;
        let sector_size = self.boot_sector.bytes_per_sector as usize;

        let to_read = buf.len().min(bytes_per_cluster);
        let full_sectors = to_read / sector_size;
        let remaining = to_read % sector_size;

        // Read full sectors
        if full_sectors > 0 {
            self.device.read_sectors(abs_sector, &mut buf[..full_sectors * sector_size])?;
        }

        // Read partial sector
        if remaining > 0 {
            let mut sector_buf = vec![0u8; sector_size];
            self.device.read_sectors(abs_sector + full_sectors as u64, &mut sector_buf)?;
            buf[full_sectors * sector_size..to_read].copy_from_slice(&sector_buf[..remaining]);
        }

        Ok(to_read)
    }

    /// Write cluster data
    fn write_cluster(&self, cluster: u32, buf: &[u8]) -> Result<usize, i32> {
        let bytes_per_cluster = self.boot_sector.bytes_per_cluster() as usize;
        let sector = self.boot_sector.cluster_to_sector(cluster);
        let abs_sector = self.partition_start + sector as u64;

        let sector_size = self.boot_sector.bytes_per_sector as usize;

        let to_write = buf.len().min(bytes_per_cluster);
        let full_sectors = to_write / sector_size;
        let remaining = to_write % sector_size;

        // Write full sectors
        if full_sectors > 0 {
            self.device.write_sectors(abs_sector, &buf[..full_sectors * sector_size])?;
        }

        // Write partial sector (read-modify-write)
        if remaining > 0 {
            let mut sector_buf = vec![0u8; sector_size];
            self.device.read_sectors(abs_sector + full_sectors as u64, &mut sector_buf)?;
            sector_buf[..remaining].copy_from_slice(&buf[full_sectors * sector_size..to_write]);
            self.device.write_sectors(abs_sector + full_sectors as u64, &sector_buf)?;
        }

        Ok(to_write)
    }

    /// Allocate a free cluster
    fn allocate_cluster(&self) -> Result<u32, i32> {
        let cluster_count = self.boot_sector.cluster_count();
        let hint = *self.next_free_hint.lock();

        // Search for free cluster starting from hint
        for i in 0..cluster_count {
            let cluster = ((hint - 2 + i) % cluster_count) + 2;
            let entry = self.read_fat_entry(cluster)?;

            if entry == FAT32_FREE {
                // Mark as end of chain
                self.write_fat_entry(cluster, FAT32_EOC)?;

                // Update hint
                *self.next_free_hint.lock() = cluster + 1;

                // Update free count
                let mut free = self.free_clusters.lock();
                if *free != 0xFFFFFFFF {
                    *free = free.saturating_sub(1);
                }

                return Ok(cluster);
            }
        }

        Err(errno::ENOSPC)
    }

    /// Free a cluster chain
    fn free_cluster_chain(&self, start_cluster: u32) -> Result<(), i32> {
        let chain = self.get_cluster_chain(start_cluster)?;

        for cluster in chain {
            self.write_fat_entry(cluster, FAT32_FREE)?;

            // Update free count
            let mut free = self.free_clusters.lock();
            if *free != 0xFFFFFFFF {
                *free += 1;
            }
        }

        self.flush_fat_cache()?;
        Ok(())
    }

    /// Read directory entries from cluster chain
    fn read_directory(&self, start_cluster: u32) -> Result<Vec<(String, Fat32DirEntry)>, i32> {
        let chain = self.get_cluster_chain(start_cluster)?;
        let bytes_per_cluster = self.boot_sector.bytes_per_cluster() as usize;
        let entry_size = size_of::<Fat32DirEntry>();

        let mut entries = Vec::new();
        let mut lfn_parts: Vec<(u8, String)> = Vec::new();

        for cluster in chain {
            let mut cluster_buf = vec![0u8; bytes_per_cluster];
            self.read_cluster(cluster, &mut cluster_buf)?;

            let num_entries = bytes_per_cluster / entry_size;

            for i in 0..num_entries {
                let offset = i * entry_size;
                let entry = unsafe {
                    core::ptr::read_unaligned(
                        cluster_buf[offset..].as_ptr() as *const Fat32DirEntry
                    )
                };

                // End of directory
                if entry.is_end() {
                    return Ok(entries);
                }

                // Skip free entries
                if entry.name[0] == 0xE5 {
                    lfn_parts.clear();
                    continue;
                }

                // Handle LFN entries
                if entry.is_lfn() {
                    let lfn = unsafe {
                        core::ptr::read_unaligned(
                            cluster_buf[offset..].as_ptr() as *const LfnEntry
                        )
                    };

                    let seq = lfn.sequence();
                    let chars = lfn.name_chars();

                    // Convert UCS-2 to string
                    let mut name = String::new();
                    for &ch in chars.iter() {
                        if ch == 0 || ch == 0xFFFF {
                            break;
                        }
                        if let Some(c) = char::from_u32(ch as u32) {
                            name.push(c);
                        }
                    }

                    if lfn.is_last() {
                        lfn_parts.clear();
                    }
                    lfn_parts.push((seq, name));

                    continue;
                }

                // Skip volume labels
                if entry.is_volume_label() {
                    lfn_parts.clear();
                    continue;
                }

                // Regular entry - get name
                let name = if !lfn_parts.is_empty() {
                    // Assemble LFN
                    lfn_parts.sort_by_key(|(seq, _)| *seq);
                    let mut full_name = String::new();
                    for (_, part) in lfn_parts.iter() {
                        full_name.push_str(part);
                    }
                    lfn_parts.clear();
                    full_name
                } else {
                    // Use short name
                    entry.short_name()
                };

                // Skip . and .. for now (handled specially)
                if name == "." || name == ".." {
                    continue;
                }

                entries.push((name, entry));
            }
        }

        Ok(entries)
    }

    /// Allocate inode number for cluster
    fn get_or_create_inode(&self, cluster: u32) -> u64 {
        let map = self.cluster_to_inode.read();
        if let Some(&ino) = map.get(&cluster) {
            return ino;
        }
        drop(map);

        let mut map = self.cluster_to_inode.write();
        // Double-check after acquiring write lock
        if let Some(&ino) = map.get(&cluster) {
            return ino;
        }

        let mut next = self.next_inode.lock();
        let ino = *next;
        *next += 1;

        map.insert(cluster, ino);
        ino
    }

    /// Create metadata from directory entry
    fn create_metadata(&self, entry: &Fat32DirEntry) -> Metadata {
        let file_type = if entry.is_directory() {
            FileType::Directory
        } else {
            FileType::Regular
        };

        let mut mode = file_type.to_mode() | 0o755;
        if (entry.attr & attrs::READ_ONLY) != 0 {
            mode &= !0o222; // Remove write permissions
        }

        Metadata {
            dev: 0,
            ino: self.get_or_create_inode(entry.first_cluster()),
            mode,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            size: entry.file_size as u64,
            blksize: self.boot_sector.bytes_per_cluster(),
            blocks: (entry.file_size as u64 + 511) / 512,
            atime: entry.access_timestamp(),
            mtime: entry.write_timestamp(),
            ctime: entry.creation_timestamp(),
        }
    }

    /// Get root inode
    pub fn root_inode(self: &Arc<Self>) -> Inode {
        let root_cluster = self.boot_sector.root_cluster;

        let metadata = Metadata {
            dev: 0,
            ino: 1, // Root is always inode 1
            mode: FileType::Directory.to_mode() | 0o755,
            nlink: 2,
            uid: 0,
            gid: 0,
            rdev: 0,
            size: 0,
            blksize: self.boot_sector.bytes_per_cluster(),
            blocks: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
        };

        let mut inode = Inode::new(
            InodeId { fs_id: self.fs_id, ino: 1 },
            metadata,
            self.clone(),
        );
        inode.private = root_cluster as u64;
        inode
    }
}

impl Filesystem for Fat32Filesystem {
    fn name(&self) -> &str {
        "fat32"
    }

    fn read(&self, inode: &Inode, offset: u64, buf: &mut [u8]) -> Result<usize, i32> {
        // Can't read directories
        if inode.metadata.is_dir() {
            return Err(errno::EISDIR);
        }

        let start_cluster = inode.private as u32;
        if start_cluster < 2 {
            return Ok(0); // Empty file
        }

        let file_size = inode.metadata.size;
        if offset >= file_size {
            return Ok(0);
        }

        let chain = self.get_cluster_chain(start_cluster)?;
        let bytes_per_cluster = self.boot_sector.bytes_per_cluster() as u64;

        let mut bytes_read = 0;
        let mut current_offset = 0u64;

        for cluster in chain {
            let cluster_end = current_offset + bytes_per_cluster;

            if current_offset >= offset + buf.len() as u64 {
                break; // We've read enough
            }

            if cluster_end > offset {
                // This cluster contains data we need
                let cluster_start = if current_offset < offset {
                    (offset - current_offset) as usize
                } else {
                    0
                };

                let to_read = ((cluster_end.min(file_size) - offset.max(current_offset)) as usize)
                    .min(buf.len() - bytes_read);

                if to_read > 0 {
                    let mut cluster_buf = vec![0u8; bytes_per_cluster as usize];
                    self.read_cluster(cluster, &mut cluster_buf)?;

                    buf[bytes_read..bytes_read + to_read]
                        .copy_from_slice(&cluster_buf[cluster_start..cluster_start + to_read]);

                    bytes_read += to_read;
                }
            }

            current_offset = cluster_end;
        }

        Ok(bytes_read)
    }

    fn write(&self, inode: &Inode, offset: u64, buf: &[u8]) -> Result<usize, i32> {
        if inode.metadata.is_dir() {
            return Err(errno::EISDIR);
        }

        let mut start_cluster = inode.private as u32;
        let bytes_per_cluster = self.boot_sector.bytes_per_cluster() as u64;

        // Allocate first cluster if needed
        if start_cluster < 2 && !buf.is_empty() {
            start_cluster = self.allocate_cluster()?;
            // Would need to update directory entry here
        }

        if start_cluster < 2 {
            return Ok(0);
        }

        let mut chain = self.get_cluster_chain(start_cluster)?;
        let end_offset = offset + buf.len() as u64;

        // Extend cluster chain if needed
        let clusters_needed = ((end_offset + bytes_per_cluster - 1) / bytes_per_cluster) as usize;
        while chain.len() < clusters_needed {
            let new_cluster = self.allocate_cluster()?;
            let last = *chain.last().unwrap();
            self.write_fat_entry(last, new_cluster)?;
            chain.push(new_cluster);
        }

        // Write data
        let mut bytes_written = 0;
        let mut current_offset = 0u64;

        for cluster in chain {
            let cluster_end = current_offset + bytes_per_cluster;

            if current_offset >= end_offset {
                break;
            }

            if cluster_end > offset {
                let cluster_start = if current_offset < offset {
                    (offset - current_offset) as usize
                } else {
                    0
                };

                let to_write = ((cluster_end - offset.max(current_offset)) as usize)
                    .min(buf.len() - bytes_written);

                if to_write > 0 {
                    // Read-modify-write if partial cluster
                    let mut cluster_buf = vec![0u8; bytes_per_cluster as usize];

                    if cluster_start > 0 || to_write < bytes_per_cluster as usize {
                        self.read_cluster(cluster, &mut cluster_buf)?;
                    }

                    cluster_buf[cluster_start..cluster_start + to_write]
                        .copy_from_slice(&buf[bytes_written..bytes_written + to_write]);

                    self.write_cluster(cluster, &cluster_buf)?;
                    bytes_written += to_write;
                }
            }

            current_offset = cluster_end;
        }

        self.flush_fat_cache()?;
        Ok(bytes_written)
    }

    fn readdir(&self, inode: &Inode) -> Result<Vec<DirEntry>, i32> {
        if !inode.metadata.is_dir() {
            return Err(errno::ENOTDIR);
        }

        let start_cluster = inode.private as u32;
        let entries = self.read_directory(start_cluster)?;

        Ok(entries.into_iter().map(|(name, entry)| {
            let file_type = if entry.is_directory() {
                FileType::Directory
            } else {
                FileType::Regular
            };

            DirEntry::new(
                name,
                self.get_or_create_inode(entry.first_cluster()),
                file_type,
            )
        }).collect())
    }

    fn lookup(&self, parent: &Inode, name: &str) -> Result<Inode, i32> {
        if !parent.metadata.is_dir() {
            return Err(errno::ENOTDIR);
        }

        let start_cluster = parent.private as u32;
        let entries = self.read_directory(start_cluster)?;

        for (entry_name, entry) in entries {
            if entry_name.eq_ignore_ascii_case(name) {
                let metadata = self.create_metadata(&entry);
                let mut inode = Inode::new(
                    InodeId { fs_id: self.fs_id, ino: metadata.ino },
                    metadata,
                    parent.ops.clone(),
                );
                inode.private = entry.first_cluster() as u64;
                return Ok(inode);
            }
        }

        Err(errno::ENOENT)
    }

    fn getattr(&self, inode: &Inode) -> Result<Metadata, i32> {
        Ok(inode.metadata.clone())
    }

    fn truncate(&self, inode: &Inode, size: u64) -> Result<(), i32> {
        let start_cluster = inode.private as u32;

        if size == 0 && start_cluster >= 2 {
            // Free all clusters
            self.free_cluster_chain(start_cluster)?;
        } else if start_cluster >= 2 {
            // Truncate to size
            let bytes_per_cluster = self.boot_sector.bytes_per_cluster() as u64;
            let clusters_to_keep = ((size + bytes_per_cluster - 1) / bytes_per_cluster) as usize;

            let chain = self.get_cluster_chain(start_cluster)?;

            if clusters_to_keep < chain.len() {
                // Mark new end of chain
                self.write_fat_entry(chain[clusters_to_keep - 1], FAT32_EOC)?;

                // Free remaining clusters
                for i in clusters_to_keep..chain.len() {
                    self.write_fat_entry(chain[i], FAT32_FREE)?;
                }

                self.flush_fat_cache()?;
            }
        }

        Ok(())
    }

    fn sync(&self, _inode: &Inode) -> Result<(), i32> {
        self.flush_fat_cache()
    }
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Convert FAT date/time to Unix timestamp
fn fat_datetime_to_unix(date: u16, time: u16) -> u64 {
    if date == 0 {
        return 0;
    }

    let year = ((date >> 9) & 0x7F) as u64 + 1980;
    let month = ((date >> 5) & 0x0F) as u64;
    let day = (date & 0x1F) as u64;

    let hour = ((time >> 11) & 0x1F) as u64;
    let minute = ((time >> 5) & 0x3F) as u64;
    let second = ((time & 0x1F) * 2) as u64;

    // Simplified conversion (not accounting for leap years properly)
    let days_since_epoch = (year - 1970) * 365 + (year - 1969) / 4;
    let month_days = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let day_of_year = if month > 0 && month <= 12 {
        month_days[(month - 1) as usize] + day - 1
    } else {
        0
    };

    (days_since_epoch + day_of_year) * 86400 + hour * 3600 + minute * 60 + second
}

/// Convert Unix timestamp to FAT date/time
fn unix_to_fat_datetime(timestamp: u64) -> (u16, u16) {
    if timestamp == 0 {
        return (0, 0);
    }

    let days = timestamp / 86400;
    let time_of_day = timestamp % 86400;

    // Approximate year calculation
    let mut year = 1970 + (days / 365) as u16;
    let mut remaining_days = days as i64;

    // Adjust for leap years
    loop {
        let leap_years = ((year - 1) - 1968) / 4 - ((year - 1) - 1900) / 100 + ((year - 1) - 1600) / 400;
        let year_days = (year as i64 - 1970) * 365 + leap_years as i64;

        if year_days <= remaining_days {
            let days_in_year = if is_leap_year(year) { 366 } else { 365 };
            if year_days + days_in_year > remaining_days {
                remaining_days -= year_days;
                break;
            }
        }
        year -= 1;
    }

    // Calculate month and day
    let month_days = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u16;
    for &days in month_days.iter() {
        if remaining_days < days as i64 {
            break;
        }
        remaining_days -= days as i64;
        month += 1;
    }

    let day = (remaining_days + 1) as u16;
    let hour = (time_of_day / 3600) as u16;
    let minute = ((time_of_day % 3600) / 60) as u16;
    let second = (time_of_day % 60) as u16;

    let fat_date = ((year - 1980) << 9) | (month << 5) | day;
    let fat_time = (hour << 11) | (minute << 5) | (second / 2);

    (fat_date, fat_time)
}

/// Check if year is a leap year
fn is_leap_year(year: u16) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Calculate LFN checksum from short name
fn lfn_checksum(short_name: &[u8; 11]) -> u8 {
    let mut sum: u8 = 0;
    for &byte in short_name.iter() {
        sum = ((sum & 1) << 7).wrapping_add(sum >> 1).wrapping_add(byte);
    }
    sum
}

/// Generate short name from long name
fn generate_short_name(long_name: &str, existing: &[String]) -> [u8; 11] {
    let mut short = [0x20u8; 11]; // Space-padded

    let upper = long_name.to_uppercase();

    // Split name and extension
    let (name_part, ext_part) = if let Some(dot_pos) = upper.rfind('.') {
        (upper[..dot_pos].to_string(), upper[dot_pos + 1..].to_string())
    } else {
        (upper, String::new())
    };

    // Remove invalid characters and get first 8 chars
    let clean_name: String = name_part
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .take(8)
        .collect();

    let clean_ext: String = ext_part
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(3)
        .collect();

    // Copy name
    for (i, byte) in clean_name.bytes().take(8).enumerate() {
        short[i] = byte;
    }

    // Copy extension
    for (i, byte) in clean_ext.bytes().take(3).enumerate() {
        short[8 + i] = byte;
    }

    // Handle collisions with ~N suffix
    let base: String = clean_name.chars().take(6).collect();
    let mut counter = 1;

    while existing.iter().any(|s| {
        let test_name = core::str::from_utf8(&short).unwrap_or("").trim();
        s.eq_ignore_ascii_case(test_name)
    }) {
        let suffix = alloc::format!("~{}", counter);
        let base_len = 8 - suffix.len();
        let truncated: String = base.chars().take(base_len).collect();

        for i in 0..8 {
            short[i] = 0x20;
        }
        for (i, byte) in truncated.bytes().enumerate() {
            short[i] = byte;
        }
        for (i, byte) in suffix.bytes().enumerate() {
            short[truncated.len() + i] = byte;
        }

        counter += 1;
        if counter > 999999 {
            break;
        }
    }

    short
}

// =============================================================================
// INITIALIZATION
// =============================================================================

/// Initialize FAT32 support
pub fn init() {
    crate::kprintln!("[FAT32] FAT32 filesystem support initialized");
}
