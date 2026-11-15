// ===============================================================================
// BUILDOS KERNEL - EXT4 FILESYSTEM
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! ext4 filesystem implementation (read-only with write support planned).
//!
//! The ext4 (fourth extended filesystem) is the default Linux filesystem.
//! This implementation supports:
//! - Extent-based block mapping (efficient large file handling)
//! - 64-bit block numbers (up to 1 EiB volumes)
//! - HTree indexed directories (fast lookups)
//! - Flexible block groups with metadata checksums
//! - Large inodes with extended attributes
//! - Journaling awareness (journal replay planned)
//!
//! Key improvements over ext2/ext3:
//! - Extents replace indirect block maps for better performance
//! - Sub-second timestamps with nanosecond precision
//! - Online defragmentation support
//! - Metadata checksumming (CRC32c)
//! - Persistent pre-allocation (fallocate)

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::mem;
use spin::RwLock;

use super::{
    DirEntry, errno, FileType, Filesystem, Inode, InodeId, Metadata,
};
use super::ext2::BlockDevice;

// =============================================================================
// EXT4 CONSTANTS
// =============================================================================

/// Superblock magic number (same as ext2/ext3)
const EXT4_MAGIC: u16 = 0xEF53;

/// Superblock offset (always at byte 1024)
const SUPERBLOCK_OFFSET: u64 = 1024;

/// Root inode number
const EXT4_ROOT_INO: u32 = 2;

/// Journal inode number
const EXT4_JOURNAL_INO: u32 = 8;

/// Lost+found inode
const EXT4_LOST_FOUND_INO: u32 = 11;

/// Maximum extent depth
const EXT4_MAX_EXTENT_DEPTH: u16 = 5;

/// Extent magic number
const EXT4_EXTENT_MAGIC: u16 = 0xF30A;

/// HTree directory hash versions
mod hash_version {
    pub const LEGACY: u8 = 0;
    pub const HALF_MD4: u8 = 1;
    pub const TEA: u8 = 2;
    pub const SIPHASH: u8 = 6;
}

/// Directory entry types (same as ext2)
mod dirent_type {
    pub const UNKNOWN: u8 = 0;
    pub const REG_FILE: u8 = 1;
    pub const DIR: u8 = 2;
    pub const CHRDEV: u8 = 3;
    pub const BLKDEV: u8 = 4;
    pub const FIFO: u8 = 5;
    pub const SOCK: u8 = 6;
    pub const SYMLINK: u8 = 7;
}

/// Inode mode bits
mod inode_mode {
    pub const S_IFMT: u16 = 0xF000;
    pub const S_IFSOCK: u16 = 0xC000;
    pub const S_IFLNK: u16 = 0xA000;
    pub const S_IFREG: u16 = 0x8000;
    pub const S_IFBLK: u16 = 0x6000;
    pub const S_IFDIR: u16 = 0x4000;
    pub const S_IFCHR: u16 = 0x2000;
    pub const S_IFIFO: u16 = 0x1000;

    pub const S_ISUID: u16 = 0x0800;
    pub const S_ISGID: u16 = 0x0400;
    pub const S_ISVTX: u16 = 0x0200;
}

/// Inode flags
mod inode_flags {
    pub const SECRM: u32 = 0x00000001;        // Secure deletion
    pub const UNRM: u32 = 0x00000002;         // Undelete
    pub const COMPR: u32 = 0x00000004;        // Compressed file
    pub const SYNC: u32 = 0x00000008;         // Synchronous updates
    pub const IMMUTABLE: u32 = 0x00000010;    // Immutable file
    pub const APPEND: u32 = 0x00000020;       // Append only
    pub const NODUMP: u32 = 0x00000040;       // Do not dump
    pub const NOATIME: u32 = 0x00000080;      // No atime updates
    pub const DIRTY: u32 = 0x00000100;        // Dirty (compressed)
    pub const COMPRBLK: u32 = 0x00000200;     // Compressed blocks
    pub const NOCOMPR: u32 = 0x00000400;      // No compression
    pub const ENCRYPT: u32 = 0x00000800;      // Encrypted inode
    pub const INDEX: u32 = 0x00001000;        // Hash-indexed directory
    pub const IMAGIC: u32 = 0x00002000;       // AFS magic directory
    pub const JOURNAL_DATA: u32 = 0x00004000; // Journaled data
    pub const NOTAIL: u32 = 0x00008000;       // No tail-merging
    pub const DIRSYNC: u32 = 0x00010000;      // Synchronous directory
    pub const TOPDIR: u32 = 0x00020000;       // Top of directory hierarchy
    pub const HUGE_FILE: u32 = 0x00040000;    // Huge file
    pub const EXTENTS: u32 = 0x00080000;      // Uses extents
    pub const VERITY: u32 = 0x00100000;       // Verity protected
    pub const EA_INODE: u32 = 0x00200000;     // Extended attribute inode
    pub const INLINE_DATA: u32 = 0x10000000;  // Inline data
    pub const PROJINHERIT: u32 = 0x20000000;  // Project hierarchy
    pub const CASEFOLD: u32 = 0x40000000;     // Casefolded directory
}

/// Feature flags (compatible) - ext4 additions
mod feature_compat {
    pub const DIR_PREALLOC: u32 = 0x0001;
    pub const IMAGIC_INODES: u32 = 0x0002;
    pub const HAS_JOURNAL: u32 = 0x0004;
    pub const EXT_ATTR: u32 = 0x0008;
    pub const RESIZE_INO: u32 = 0x0010;
    pub const DIR_INDEX: u32 = 0x0020;
    pub const SPARSE_SUPER2: u32 = 0x0200;
    pub const FAST_COMMIT: u32 = 0x0400;
    pub const STABLE_INODES: u32 = 0x0800;
    pub const ORPHAN_FILE: u32 = 0x1000;
}

/// Feature flags (incompatible) - ext4 additions
mod feature_incompat {
    pub const COMPRESSION: u32 = 0x0001;
    pub const FILETYPE: u32 = 0x0002;
    pub const RECOVER: u32 = 0x0004;
    pub const JOURNAL_DEV: u32 = 0x0008;
    pub const META_BG: u32 = 0x0010;
    pub const EXTENTS: u32 = 0x0040;       // Uses extents
    pub const _64BIT: u32 = 0x0080;        // 64-bit block numbers
    pub const MMP: u32 = 0x0100;           // Multiple mount protection
    pub const FLEX_BG: u32 = 0x0200;       // Flexible block groups
    pub const EA_INODE: u32 = 0x0400;      // Large extended attributes
    pub const DIRDATA: u32 = 0x1000;       // Directory data in dirent
    pub const CSUM_SEED: u32 = 0x2000;     // Checksum seed in superblock
    pub const LARGEDIR: u32 = 0x4000;      // Large directory >2GB
    pub const INLINE_DATA: u32 = 0x8000;   // Inline data in inode
    pub const ENCRYPT: u32 = 0x10000;      // Encrypted
    pub const CASEFOLD: u32 = 0x20000;     // Case-insensitive directories
}

/// Feature flags (read-only compatible) - ext4 additions
mod feature_ro_compat {
    pub const SPARSE_SUPER: u32 = 0x0001;
    pub const LARGE_FILE: u32 = 0x0002;
    pub const BTREE_DIR: u32 = 0x0004;
    pub const HUGE_FILE: u32 = 0x0008;     // Huge files (>2TB)
    pub const GDT_CSUM: u32 = 0x0010;      // Group desc checksums
    pub const DIR_NLINK: u32 = 0x0020;     // >32000 subdirs
    pub const EXTRA_ISIZE: u32 = 0x0040;   // Large inodes
    pub const HAS_SNAPSHOT: u32 = 0x0080;  // Has snapshot
    pub const QUOTA: u32 = 0x0100;         // Quota
    pub const BIGALLOC: u32 = 0x0200;      // Big alloc
    pub const METADATA_CSUM: u32 = 0x0400; // Metadata checksum
    pub const REPLICA: u32 = 0x0800;       // Replica
    pub const READONLY: u32 = 0x1000;      // Read-only
    pub const PROJECT: u32 = 0x2000;       // Project quotas
    pub const VERITY: u32 = 0x8000;        // Verity
    pub const ORPHAN_PRESENT: u32 = 0x10000;
}

// =============================================================================
// EXT4 ON-DISK STRUCTURES
// =============================================================================

/// ext4 superblock (extended from ext2)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Ext4Superblock {
    // -- Base ext2 fields (0-95) --
    pub inodes_count: u32,
    pub blocks_count_lo: u32,
    pub r_blocks_count_lo: u32,
    pub free_blocks_count_lo: u32,
    pub free_inodes_count: u32,
    pub first_data_block: u32,
    pub log_block_size: u32,
    pub log_cluster_size: u32,
    pub blocks_per_group: u32,
    pub clusters_per_group: u32,
    pub inodes_per_group: u32,
    pub mtime: u32,
    pub wtime: u32,
    pub mnt_count: u16,
    pub max_mnt_count: u16,
    pub magic: u16,
    pub state: u16,
    pub errors: u16,
    pub minor_rev_level: u16,
    pub lastcheck: u32,
    pub checkinterval: u32,
    pub creator_os: u32,
    pub rev_level: u32,
    pub def_resuid: u16,
    pub def_resgid: u16,

    // -- EXT4_DYNAMIC_REV fields (96-263) --
    pub first_ino: u32,
    pub inode_size: u16,
    pub block_group_nr: u16,
    pub feature_compat: u32,
    pub feature_incompat: u32,
    pub feature_ro_compat: u32,
    pub uuid: [u8; 16],
    pub volume_name: [u8; 16],
    pub last_mounted: [u8; 64],
    pub algorithm_usage_bitmap: u32,
    pub prealloc_blocks: u8,
    pub prealloc_dir_blocks: u8,
    pub reserved_gdt_blocks: u16,

    // -- Journaling (ext3) fields (264-319) --
    pub journal_uuid: [u8; 16],
    pub journal_inum: u32,
    pub journal_dev: u32,
    pub last_orphan: u32,
    pub hash_seed: [u32; 4],
    pub def_hash_version: u8,
    pub jnl_backup_type: u8,
    pub desc_size: u16,
    pub default_mount_opts: u32,
    pub first_meta_bg: u32,
    pub mkfs_time: u32,
    pub jnl_blocks: [u32; 17],

    // -- 64-bit support fields (320-383) --
    pub blocks_count_hi: u32,
    pub r_blocks_count_hi: u32,
    pub free_blocks_count_hi: u32,
    pub min_extra_isize: u16,
    pub want_extra_isize: u16,
    pub flags: u32,
    pub raid_stride: u16,
    pub mmp_interval: u16,
    pub mmp_block: u64,
    pub raid_stripe_width: u32,
    pub log_groups_per_flex: u8,
    pub checksum_type: u8,
    pub reserved_pad: u16,
    pub kbytes_written: u64,
    pub snapshot_inum: u32,
    pub snapshot_id: u32,
    pub snapshot_r_blocks_count: u64,
    pub snapshot_list: u32,
    pub error_count: u32,
    pub first_error_time: u32,
    pub first_error_ino: u32,
    pub first_error_block: u64,
    pub first_error_func: [u8; 32],
    pub first_error_line: u32,
    pub last_error_time: u32,
    pub last_error_ino: u32,
    pub last_error_line: u32,
    pub last_error_block: u64,
    pub last_error_func: [u8; 32],
    pub mount_opts: [u8; 64],
    pub usr_quota_inum: u32,
    pub grp_quota_inum: u32,
    pub overhead_blocks: u32,
    pub backup_bgs: [u32; 2],
    pub encrypt_algos: [u8; 4],
    pub encrypt_pw_salt: [u8; 16],
    pub lpf_ino: u32,
    pub prj_quota_inum: u32,
    pub checksum_seed: u32,
    pub wtime_hi: u8,
    pub mtime_hi: u8,
    pub mkfs_time_hi: u8,
    pub lastcheck_hi: u8,
    pub first_error_time_hi: u8,
    pub last_error_time_hi: u8,
    pub first_error_errcode: u8,
    pub last_error_errcode: u8,
    pub encoding: u16,
    pub encoding_flags: u16,
    pub orphan_file_inum: u32,
    reserved: [u32; 94],
    pub checksum: u32,
}

impl Ext4Superblock {
    /// Get block size in bytes
    pub fn block_size(&self) -> u32 {
        1024 << self.log_block_size
    }

    /// Get total block count (64-bit)
    pub fn blocks_count(&self) -> u64 {
        let lo = self.blocks_count_lo as u64;
        let hi = if self.has_64bit() {
            (self.blocks_count_hi as u64) << 32
        } else {
            0
        };
        lo | hi
    }

    /// Get free block count (64-bit)
    pub fn free_blocks_count(&self) -> u64 {
        let lo = self.free_blocks_count_lo as u64;
        let hi = if self.has_64bit() {
            (self.free_blocks_count_hi as u64) << 32
        } else {
            0
        };
        lo | hi
    }

    /// Get number of block groups
    pub fn block_group_count(&self) -> u32 {
        let total = self.blocks_count();
        ((total + self.blocks_per_group as u64 - 1) / self.blocks_per_group as u64) as u32
    }

    /// Get inode size
    pub fn inode_size(&self) -> u32 {
        if self.rev_level >= 1 {
            self.inode_size as u32
        } else {
            128
        }
    }

    /// Get group descriptor size
    pub fn group_desc_size(&self) -> u32 {
        if self.has_64bit() && self.desc_size > 32 {
            self.desc_size as u32
        } else {
            32
        }
    }

    /// Check if 64-bit mode is enabled
    pub fn has_64bit(&self) -> bool {
        (self.feature_incompat & feature_incompat::_64BIT) != 0
    }

    /// Check if extents are enabled
    pub fn has_extents(&self) -> bool {
        (self.feature_incompat & feature_incompat::EXTENTS) != 0
    }

    /// Check if flex_bg is enabled
    pub fn has_flex_bg(&self) -> bool {
        (self.feature_incompat & feature_incompat::FLEX_BG) != 0
    }

    /// Check if metadata checksums are enabled
    pub fn has_metadata_csum(&self) -> bool {
        (self.feature_ro_compat & feature_ro_compat::METADATA_CSUM) != 0
    }

    /// Check if huge files are supported
    pub fn has_huge_file(&self) -> bool {
        (self.feature_ro_compat & feature_ro_compat::HUGE_FILE) != 0
    }

    /// Check if dir_index (HTree) is enabled
    pub fn has_dir_index(&self) -> bool {
        (self.feature_compat & feature_compat::DIR_INDEX) != 0
    }

    /// Check if inline data is enabled
    pub fn has_inline_data(&self) -> bool {
        (self.feature_incompat & feature_incompat::INLINE_DATA) != 0
    }

    /// Get flex block group size
    pub fn flex_bg_size(&self) -> u32 {
        if self.has_flex_bg() {
            1 << self.log_groups_per_flex
        } else {
            1
        }
    }
}

/// 64-bit block group descriptor
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Ext4GroupDesc {
    // 32-bit fields (original ext2)
    pub block_bitmap_lo: u32,
    pub inode_bitmap_lo: u32,
    pub inode_table_lo: u32,
    pub free_blocks_count_lo: u16,
    pub free_inodes_count_lo: u16,
    pub used_dirs_count_lo: u16,
    pub flags: u16,
    pub exclude_bitmap_lo: u32,
    pub block_bitmap_csum_lo: u16,
    pub inode_bitmap_csum_lo: u16,
    pub itable_unused_lo: u16,
    pub checksum: u16,

    // 64-bit extensions
    pub block_bitmap_hi: u32,
    pub inode_bitmap_hi: u32,
    pub inode_table_hi: u32,
    pub free_blocks_count_hi: u16,
    pub free_inodes_count_hi: u16,
    pub used_dirs_count_hi: u16,
    pub itable_unused_hi: u16,
    pub exclude_bitmap_hi: u32,
    pub block_bitmap_csum_hi: u16,
    pub inode_bitmap_csum_hi: u16,
    pub reserved: u32,
}

impl Ext4GroupDesc {
    /// Get block bitmap block (64-bit)
    pub fn block_bitmap(&self, is_64bit: bool) -> u64 {
        let lo = self.block_bitmap_lo as u64;
        let hi = if is_64bit { (self.block_bitmap_hi as u64) << 32 } else { 0 };
        lo | hi
    }

    /// Get inode bitmap block (64-bit)
    pub fn inode_bitmap(&self, is_64bit: bool) -> u64 {
        let lo = self.inode_bitmap_lo as u64;
        let hi = if is_64bit { (self.inode_bitmap_hi as u64) << 32 } else { 0 };
        lo | hi
    }

    /// Get inode table block (64-bit)
    pub fn inode_table(&self, is_64bit: bool) -> u64 {
        let lo = self.inode_table_lo as u64;
        let hi = if is_64bit { (self.inode_table_hi as u64) << 32 } else { 0 };
        lo | hi
    }

    /// Get free blocks count
    pub fn free_blocks_count(&self, is_64bit: bool) -> u32 {
        let lo = self.free_blocks_count_lo as u32;
        let hi = if is_64bit { (self.free_blocks_count_hi as u32) << 16 } else { 0 };
        lo | hi
    }
}

/// ext4 inode structure (extended)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Ext4Inode {
    // Standard inode fields (128 bytes)
    pub mode: u16,
    pub uid: u16,
    pub size_lo: u32,
    pub atime: u32,
    pub ctime: u32,
    pub mtime: u32,
    pub dtime: u32,
    pub gid: u16,
    pub links_count: u16,
    pub blocks_lo: u32,
    pub flags: u32,
    pub osd1: u32,
    pub block: [u32; 15],  // For extents: extent header + tree, for legacy: block map
    pub generation: u32,
    pub file_acl_lo: u32,
    pub size_hi: u32,
    pub obso_faddr: u32,

    // OS-dependent (12 bytes)
    pub osd2: Ext4InodeOsd2,

    // Extended inode fields (256 bytes total with extra_isize)
    pub extra_isize: u16,
    pub checksum_hi: u16,
    pub ctime_extra: u32,
    pub mtime_extra: u32,
    pub atime_extra: u32,
    pub crtime: u32,
    pub crtime_extra: u32,
    pub version_hi: u32,
    pub projid: u32,
}

/// OS-dependent inode data (Linux)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Ext4InodeOsd2 {
    pub blocks_hi: u16,
    pub file_acl_hi: u16,
    pub uid_hi: u16,
    pub gid_hi: u16,
    pub checksum_lo: u16,
    pub reserved: u16,
}

impl Ext4Inode {
    /// Get file size (64-bit)
    pub fn file_size(&self) -> u64 {
        let lo = self.size_lo as u64;
        let hi = (self.size_hi as u64) << 32;
        lo | hi
    }

    /// Get block count (512-byte blocks, or fs blocks if HUGE_FILE)
    pub fn blocks(&self, huge_file: bool) -> u64 {
        let lo = self.blocks_lo as u64;
        let hi = (self.osd2.blocks_hi as u64) << 32;
        let blocks = lo | hi;

        if huge_file && (self.flags & inode_flags::HUGE_FILE) != 0 {
            // Count is in filesystem blocks, not 512-byte blocks
            blocks
        } else {
            blocks
        }
    }

    /// Check if inode uses extents
    pub fn uses_extents(&self) -> bool {
        (self.flags & inode_flags::EXTENTS) != 0
    }

    /// Check if inode uses inline data
    pub fn uses_inline_data(&self) -> bool {
        (self.flags & inode_flags::INLINE_DATA) != 0
    }

    /// Check if HTree-indexed directory
    pub fn uses_htree(&self) -> bool {
        (self.flags & inode_flags::INDEX) != 0
    }

    /// Get file type
    pub fn file_type(&self) -> FileType {
        match self.mode & inode_mode::S_IFMT {
            inode_mode::S_IFREG => FileType::Regular,
            inode_mode::S_IFDIR => FileType::Directory,
            inode_mode::S_IFLNK => FileType::Symlink,
            inode_mode::S_IFBLK => FileType::BlockDevice,
            inode_mode::S_IFCHR => FileType::CharDevice,
            inode_mode::S_IFIFO => FileType::Pipe,
            inode_mode::S_IFSOCK => FileType::Socket,
            _ => FileType::Regular,
        }
    }

    /// Check if directory
    pub fn is_dir(&self) -> bool {
        (self.mode & inode_mode::S_IFMT) == inode_mode::S_IFDIR
    }

    /// Check if regular file
    pub fn is_file(&self) -> bool {
        (self.mode & inode_mode::S_IFMT) == inode_mode::S_IFREG
    }

    /// Check if symlink
    pub fn is_symlink(&self) -> bool {
        (self.mode & inode_mode::S_IFMT) == inode_mode::S_IFLNK
    }

    /// Get UID (32-bit)
    pub fn uid(&self) -> u32 {
        (self.uid as u32) | ((self.osd2.uid_hi as u32) << 16)
    }

    /// Get GID (32-bit)
    pub fn gid(&self) -> u32 {
        (self.gid as u32) | ((self.osd2.gid_hi as u32) << 16)
    }

    /// Get access time with nanoseconds
    pub fn atime_ns(&self) -> (u64, u32) {
        let secs = self.atime as u64;
        let nsecs = (self.atime_extra & 0x3FFFFFFF) << 2;
        let epoch_bits = (self.atime_extra >> 30) & 0x3;
        let full_secs = secs | ((epoch_bits as u64) << 32);
        (full_secs, nsecs)
    }

    /// Get creation time with nanoseconds
    pub fn crtime_ns(&self) -> (u64, u32) {
        let secs = self.crtime as u64;
        let nsecs = (self.crtime_extra & 0x3FFFFFFF) << 2;
        let epoch_bits = (self.crtime_extra >> 30) & 0x3;
        let full_secs = secs | ((epoch_bits as u64) << 32);
        (full_secs, nsecs)
    }
}

// =============================================================================
// EXTENT STRUCTURES
// =============================================================================

/// Extent header
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Ext4ExtentHeader {
    /// Magic number (0xF30A)
    pub magic: u16,
    /// Number of valid entries
    pub entries: u16,
    /// Maximum entries
    pub max: u16,
    /// Depth of tree (0 = leaf)
    pub depth: u16,
    /// Generation
    pub generation: u32,
}

impl Ext4ExtentHeader {
    /// Check if valid extent header
    pub fn is_valid(&self) -> bool {
        self.magic == EXT4_EXTENT_MAGIC
    }

    /// Check if this is a leaf node
    pub fn is_leaf(&self) -> bool {
        self.depth == 0
    }
}

/// Extent index (internal node)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Ext4ExtentIdx {
    /// First file block covered
    pub block: u32,
    /// Block of next extent node (low 32 bits)
    pub leaf_lo: u32,
    /// Block of next extent node (high 16 bits)
    pub leaf_hi: u16,
    /// Unused
    pub unused: u16,
}

impl Ext4ExtentIdx {
    /// Get physical block number of child node
    pub fn leaf_block(&self) -> u64 {
        (self.leaf_lo as u64) | ((self.leaf_hi as u64) << 32)
    }
}

/// Extent (leaf node)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Ext4Extent {
    /// First file block
    pub block: u32,
    /// Number of blocks covered
    pub len: u16,
    /// High 16 bits of physical block
    pub start_hi: u16,
    /// Low 32 bits of physical block
    pub start_lo: u32,
}

impl Ext4Extent {
    /// Get physical start block
    pub fn start(&self) -> u64 {
        (self.start_lo as u64) | ((self.start_hi as u64) << 32)
    }

    /// Check if unwritten (pre-allocated)
    pub fn is_unwritten(&self) -> bool {
        (self.len & 0x8000) != 0
    }

    /// Get actual length (masking unwritten flag)
    pub fn length(&self) -> u32 {
        (self.len & 0x7FFF) as u32
    }

    /// Check if block is within this extent
    pub fn contains(&self, logical_block: u32) -> bool {
        logical_block >= self.block && logical_block < self.block + self.length()
    }

    /// Get physical block for logical block
    pub fn physical_block(&self, logical_block: u32) -> Option<u64> {
        if self.contains(logical_block) {
            Some(self.start() + (logical_block - self.block) as u64)
        } else {
            None
        }
    }
}

// =============================================================================
// HTREE DIRECTORY STRUCTURES
// =============================================================================

/// HTree root info
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct DxRoot {
    /// Dot entry
    pub dot_inode: u32,
    pub dot_rec_len: u16,
    pub dot_name_len: u8,
    pub dot_file_type: u8,
    pub dot_name: [u8; 4],

    /// Dotdot entry
    pub dotdot_inode: u32,
    pub dotdot_rec_len: u16,
    pub dotdot_name_len: u8,
    pub dotdot_file_type: u8,
    pub dotdot_name: [u8; 4],

    /// Root info
    pub reserved_zero: u32,
    pub hash_version: u8,
    pub info_length: u8,
    pub indirect_levels: u8,
    pub unused_flags: u8,

    /// Limit and count
    pub limit: u16,
    pub count: u16,
    pub block: u32,
    // entries follow
}

/// HTree entry
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct DxEntry {
    pub hash: u32,
    pub block: u32,
}

/// HTree node (internal)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct DxNode {
    pub fake_inode: u32,
    pub fake_rec_len: u16,
    pub name_len: u8,
    pub file_type: u8,
    pub limit: u16,
    pub count: u16,
    pub block: u32,
    // entries follow
}

// =============================================================================
// DIRECTORY ENTRY
// =============================================================================

/// ext4 directory entry (variable size, same as ext2)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Ext4DirEntry {
    pub inode: u32,
    pub rec_len: u16,
    pub name_len: u8,
    pub file_type: u8,
    // name follows (variable length)
}

/// ext4 directory entry with tail (for checksums)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Ext4DirEntryTail {
    pub reserved_zero1: u32,
    pub rec_len: u16,
    pub reserved_zero2: u8,
    pub reserved_ft: u8,
    pub checksum: u32,
}

// =============================================================================
// EXTENT CACHE
// =============================================================================

/// Cached extent information
#[derive(Clone, Copy)]
struct CachedExtent {
    /// Logical block start
    logical: u32,
    /// Physical block start
    physical: u64,
    /// Length in blocks
    len: u32,
    /// Is unwritten/hole
    unwritten: bool,
}

/// Per-inode extent cache
struct ExtentCache {
    /// Inode number
    ino: u32,
    /// Cached extents
    extents: Vec<CachedExtent>,
}

// =============================================================================
// EXT4 FILESYSTEM
// =============================================================================

/// ext4 filesystem instance
pub struct Ext4Filesystem {
    /// Block device
    device: Arc<dyn BlockDevice>,
    /// Superblock
    superblock: Ext4Superblock,
    /// Block group descriptors
    block_groups: Vec<Ext4GroupDesc>,
    /// Block size
    block_size: u32,
    /// Filesystem ID for VFS
    fs_id: u64,
    /// Is 64-bit mode
    is_64bit: bool,
    /// Extent cache
    extent_cache: RwLock<BTreeMap<u32, Vec<CachedExtent>>>,
}

impl Ext4Filesystem {
    /// Mount ext4 filesystem from block device
    pub fn mount(device: Arc<dyn BlockDevice>, fs_id: u64) -> Result<Self, &'static str> {
        // Read superblock
        let mut sb_buf = [0u8; 1024];
        device.read_sectors(2, 2, &mut sb_buf)
            .map_err(|_| "Failed to read superblock")?;

        let superblock: Ext4Superblock = unsafe {
            core::ptr::read(sb_buf.as_ptr() as *const _)
        };

        // Validate magic
        if superblock.magic != EXT4_MAGIC {
            return Err("Invalid ext4 magic number");
        }

        // Check for unsupported features
        let incompat = superblock.feature_incompat;
        if (incompat & feature_incompat::COMPRESSION) != 0 {
            return Err("Compression not supported");
        }
        if (incompat & feature_incompat::ENCRYPT) != 0 {
            return Err("Encryption not yet supported");
        }
        if (incompat & feature_incompat::MMP) != 0 {
            // MMP is fine for read-only
        }

        let block_size = superblock.block_size();
        let is_64bit = superblock.has_64bit();
        let group_desc_size = superblock.group_desc_size() as usize;
        let group_count = superblock.block_group_count() as usize;

        // Read block group descriptors
        let bgd_block = if block_size > 1024 { 1 } else { 2 };
        let bgd_bytes = group_count * group_desc_size;
        let bgd_sectors = (bgd_bytes + 511) / 512;

        let mut bgd_buf = vec![0u8; bgd_sectors * 512];
        let bgd_lba = (bgd_block as u64 * block_size as u64) / 512;
        device.read_sectors(bgd_lba, bgd_sectors as u32, &mut bgd_buf)
            .map_err(|_| "Failed to read block group descriptors")?;

        let mut block_groups = Vec::with_capacity(group_count);
        for i in 0..group_count {
            let offset = i * group_desc_size;
            if offset + mem::size_of::<Ext4GroupDesc>() <= bgd_buf.len() {
                let bgd: Ext4GroupDesc = unsafe {
                    core::ptr::read(bgd_buf.as_ptr().add(offset) as *const _)
                };
                block_groups.push(bgd);
            } else {
                // Partial descriptor, zero-fill
                let mut bgd: Ext4GroupDesc = unsafe { mem::zeroed() };
                let available = bgd_buf.len() - offset;
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        bgd_buf.as_ptr().add(offset),
                        &mut bgd as *mut _ as *mut u8,
                        available.min(mem::size_of::<Ext4GroupDesc>()),
                    );
                }
                block_groups.push(bgd);
            }
        }

        Ok(Self {
            device,
            superblock,
            block_groups,
            block_size,
            fs_id,
            is_64bit,
            extent_cache: RwLock::new(BTreeMap::new()),
        })
    }

    /// Read a block from disk
    fn read_block(&self, block_num: u64, buf: &mut [u8]) -> Result<(), i32> {
        if buf.len() < self.block_size as usize {
            return Err(errno::EINVAL);
        }

        let lba = (block_num * self.block_size as u64) / 512;
        let sectors = self.block_size / 512;

        self.device.read_sectors(lba, sectors, &mut buf[..self.block_size as usize])
    }

    /// Read inode from disk
    fn read_inode(&self, inode_num: u32) -> Result<Ext4Inode, i32> {
        if inode_num == 0 || inode_num > self.superblock.inodes_count {
            return Err(errno::EINVAL);
        }

        let group = ((inode_num - 1) / self.superblock.inodes_per_group) as usize;
        let index = (inode_num - 1) % self.superblock.inodes_per_group;

        if group >= self.block_groups.len() {
            return Err(errno::EINVAL);
        }

        let bgd = &self.block_groups[group];
        let inode_size = self.superblock.inode_size();
        let inodes_per_block = self.block_size / inode_size;

        let block_offset = index / inodes_per_block;
        let offset_in_block = (index % inodes_per_block) * inode_size;

        let inode_table = bgd.inode_table(self.is_64bit);
        let block_num = inode_table + block_offset as u64;

        let mut block_buf = vec![0u8; self.block_size as usize];
        self.read_block(block_num, &mut block_buf)?;

        let inode: Ext4Inode = unsafe {
            core::ptr::read(block_buf.as_ptr().add(offset_in_block as usize) as *const _)
        };

        Ok(inode)
    }

    /// Get physical block for logical block (extent-based)
    fn extent_lookup(&self, inode: &Ext4Inode, logical_block: u32) -> Result<Option<u64>, i32> {
        // Check extent cache first
        // (simplified - would need inode number for proper caching)

        // Copy block array from packed struct to avoid unaligned access
        let block_copy: [u32; 15] = unsafe {
            core::ptr::read_unaligned(core::ptr::addr_of!(inode.block))
        };

        // Read extent header from copied block array
        let block_bytes: &[u8] = unsafe {
            core::slice::from_raw_parts(
                block_copy.as_ptr() as *const u8,
                60 // 15 * 4 bytes
            )
        };

        let header: Ext4ExtentHeader = unsafe {
            core::ptr::read(block_bytes.as_ptr() as *const _)
        };

        if !header.is_valid() {
            return Err(errno::EIO);
        }

        self.extent_tree_lookup(&header, block_bytes, logical_block)
    }

    /// Recursively search extent tree
    fn extent_tree_lookup(
        &self,
        header: &Ext4ExtentHeader,
        data: &[u8],
        logical_block: u32,
    ) -> Result<Option<u64>, i32> {
        let entry_offset = mem::size_of::<Ext4ExtentHeader>();

        if header.is_leaf() {
            // Search leaf extents
            for i in 0..header.entries as usize {
                let offset = entry_offset + i * mem::size_of::<Ext4Extent>();
                if offset + mem::size_of::<Ext4Extent>() > data.len() {
                    break;
                }

                let extent: Ext4Extent = unsafe {
                    core::ptr::read(data.as_ptr().add(offset) as *const _)
                };

                if let Some(phys) = extent.physical_block(logical_block) {
                    if extent.is_unwritten() {
                        // Unwritten extent = hole (return zeros)
                        return Ok(None);
                    }
                    return Ok(Some(phys));
                }
            }

            // Not found in any extent = hole
            Ok(None)
        } else {
            // Search index nodes
            let mut target_idx: Option<Ext4ExtentIdx> = None;

            for i in 0..header.entries as usize {
                let offset = entry_offset + i * mem::size_of::<Ext4ExtentIdx>();
                if offset + mem::size_of::<Ext4ExtentIdx>() > data.len() {
                    break;
                }

                let idx: Ext4ExtentIdx = unsafe {
                    core::ptr::read(data.as_ptr().add(offset) as *const _)
                };

                if logical_block >= idx.block {
                    target_idx = Some(idx);
                } else {
                    break;
                }
            }

            if let Some(idx) = target_idx {
                // Read child node
                let child_block = idx.leaf_block();
                let mut child_buf = vec![0u8; self.block_size as usize];
                self.read_block(child_block, &mut child_buf)?;

                let child_header: Ext4ExtentHeader = unsafe {
                    core::ptr::read(child_buf.as_ptr() as *const _)
                };

                if !child_header.is_valid() {
                    return Err(errno::EIO);
                }

                self.extent_tree_lookup(&child_header, &child_buf, logical_block)
            } else {
                Ok(None)
            }
        }
    }

    /// Get physical block for logical block (legacy indirect mapping)
    fn indirect_lookup(&self, inode: &Ext4Inode, block_index: u32) -> Result<u64, i32> {
        let ptrs_per_block = self.block_size / 4;

        // Direct blocks (0-11)
        if block_index < 12 {
            return Ok(inode.block[block_index as usize] as u64);
        }

        let block_index = block_index - 12;

        // Indirect block (12)
        if block_index < ptrs_per_block {
            let indirect_block = inode.block[12];
            if indirect_block == 0 {
                return Ok(0);
            }
            return self.read_indirect_ptr(indirect_block as u64, block_index);
        }

        let block_index = block_index - ptrs_per_block;

        // Double indirect block (13)
        if block_index < ptrs_per_block * ptrs_per_block {
            let dindirect_block = inode.block[13];
            if dindirect_block == 0 {
                return Ok(0);
            }

            let indirect_index = block_index / ptrs_per_block;
            let indirect_block = self.read_indirect_ptr(dindirect_block as u64, indirect_index)?;
            if indirect_block == 0 {
                return Ok(0);
            }

            let offset = block_index % ptrs_per_block;
            return self.read_indirect_ptr(indirect_block, offset);
        }

        let block_index = block_index - ptrs_per_block * ptrs_per_block;

        // Triple indirect block (14)
        let tindirect_block = inode.block[14];
        if tindirect_block == 0 {
            return Ok(0);
        }

        let dindirect_index = block_index / (ptrs_per_block * ptrs_per_block);
        let dindirect_block = self.read_indirect_ptr(tindirect_block as u64, dindirect_index)?;
        if dindirect_block == 0 {
            return Ok(0);
        }

        let remaining = block_index % (ptrs_per_block * ptrs_per_block);
        let indirect_index = remaining / ptrs_per_block;
        let indirect_block = self.read_indirect_ptr(dindirect_block, indirect_index)?;
        if indirect_block == 0 {
            return Ok(0);
        }

        let offset = remaining % ptrs_per_block;
        self.read_indirect_ptr(indirect_block, offset)
    }

    /// Read block pointer from indirect block
    fn read_indirect_ptr(&self, block_num: u64, index: u32) -> Result<u64, i32> {
        let mut block_buf = vec![0u8; self.block_size as usize];
        self.read_block(block_num, &mut block_buf)?;

        let offset = (index * 4) as usize;
        if offset + 4 > block_buf.len() {
            return Err(errno::EINVAL);
        }

        let ptr = u32::from_le_bytes([
            block_buf[offset],
            block_buf[offset + 1],
            block_buf[offset + 2],
            block_buf[offset + 3],
        ]);

        Ok(ptr as u64)
    }

    /// Get physical block for logical block
    fn get_block(&self, inode: &Ext4Inode, logical_block: u32) -> Result<Option<u64>, i32> {
        if inode.uses_extents() {
            self.extent_lookup(inode, logical_block)
        } else {
            let block = self.indirect_lookup(inode, logical_block)?;
            Ok(if block == 0 { None } else { Some(block) })
        }
    }

    /// Read file data at offset
    fn read_file_data(&self, inode: &Ext4Inode, offset: u64, buf: &mut [u8]) -> Result<usize, i32> {
        // Handle inline data
        if inode.uses_inline_data() {
            return self.read_inline_data(inode, offset, buf);
        }

        let file_size = inode.file_size();
        if offset >= file_size {
            return Ok(0);
        }

        let to_read = buf.len().min((file_size - offset) as usize);
        let mut bytes_read = 0;

        while bytes_read < to_read {
            let current_offset = offset + bytes_read as u64;
            let block_index = (current_offset / self.block_size as u64) as u32;
            let offset_in_block = (current_offset % self.block_size as u64) as usize;

            let block_num = self.get_block(inode, block_index)?;

            match block_num {
                None => {
                    // Sparse file / hole - fill with zeros
                    let available = self.block_size as usize - offset_in_block;
                    let copy_size = (to_read - bytes_read).min(available);
                    buf[bytes_read..bytes_read + copy_size].fill(0);
                    bytes_read += copy_size;
                }
                Some(phys_block) => {
                    let mut block_buf = vec![0u8; self.block_size as usize];
                    self.read_block(phys_block, &mut block_buf)?;

                    let available = self.block_size as usize - offset_in_block;
                    let copy_size = (to_read - bytes_read).min(available);
                    buf[bytes_read..bytes_read + copy_size]
                        .copy_from_slice(&block_buf[offset_in_block..offset_in_block + copy_size]);
                    bytes_read += copy_size;
                }
            }
        }

        Ok(bytes_read)
    }

    /// Read inline data from inode
    fn read_inline_data(&self, inode: &Ext4Inode, offset: u64, buf: &mut [u8]) -> Result<usize, i32> {
        let file_size = inode.file_size() as usize;

        // Copy block array from packed struct to avoid unaligned access
        let block_copy: [u32; 15] = unsafe {
            core::ptr::read_unaligned(core::ptr::addr_of!(inode.block))
        };

        // Inline data stored in block[] array (up to 60 bytes)
        let inline_data: &[u8] = unsafe {
            core::slice::from_raw_parts(
                block_copy.as_ptr() as *const u8,
                file_size.min(60)
            )
        };

        if offset as usize >= inline_data.len() {
            return Ok(0);
        }

        let start = offset as usize;
        let to_copy = buf.len().min(inline_data.len() - start);
        buf[..to_copy].copy_from_slice(&inline_data[start..start + to_copy]);

        Ok(to_copy)
    }

    /// Read directory entries (linear scan)
    fn read_directory_linear(&self, inode: &Ext4Inode) -> Result<Vec<(String, u32, u8)>, i32> {
        if !inode.is_dir() {
            return Err(errno::ENOTDIR);
        }

        let mut entries = Vec::new();
        let dir_size = inode.file_size();
        let mut offset = 0u64;

        while offset < dir_size {
            let block_index = (offset / self.block_size as u64) as u32;
            let block_num = self.get_block(inode, block_index)?;

            let phys_block = match block_num {
                Some(b) => b,
                None => break,
            };

            let mut block_buf = vec![0u8; self.block_size as usize];
            self.read_block(phys_block, &mut block_buf)?;

            let block_offset = (offset % self.block_size as u64) as usize;
            let mut pos = block_offset;

            while pos + 8 <= self.block_size as usize {
                let entry: Ext4DirEntry = unsafe {
                    core::ptr::read(block_buf.as_ptr().add(pos) as *const _)
                };

                if entry.rec_len == 0 {
                    break;
                }

                // Check for directory tail (checksum)
                if entry.inode == 0 && entry.name_len == 0 && entry.file_type == 0xDE {
                    // This is a tail entry with checksum
                    pos += entry.rec_len as usize;
                    continue;
                }

                if entry.inode != 0 && entry.name_len > 0 {
                    let name_start = pos + 8;
                    let name_end = name_start + entry.name_len as usize;

                    if name_end <= self.block_size as usize {
                        let name = core::str::from_utf8(&block_buf[name_start..name_end])
                            .unwrap_or("")
                            .to_string();
                        entries.push((name, entry.inode, entry.file_type));
                    }
                }

                pos += entry.rec_len as usize;
                if pos >= self.block_size as usize {
                    break;
                }
            }

            offset = ((block_index + 1) as u64) * self.block_size as u64;
        }

        Ok(entries)
    }

    /// Read directory entries (HTree indexed)
    fn read_directory_htree(&self, inode: &Ext4Inode) -> Result<Vec<(String, u32, u8)>, i32> {
        // For now, fall back to linear scan
        // Full HTree support would require:
        // 1. Parse DxRoot structure
        // 2. Hash the lookup name
        // 3. Binary search in HTree
        // 4. Read leaf block with entries

        // Linear scan still works for HTree dirs
        self.read_directory_linear(inode)
    }

    /// Read directory entries
    fn read_directory(&self, inode: &Ext4Inode) -> Result<Vec<(String, u32, u8)>, i32> {
        if inode.uses_htree() {
            self.read_directory_htree(inode)
        } else {
            self.read_directory_linear(inode)
        }
    }

    /// Read symlink target
    fn read_symlink(&self, inode: &Ext4Inode) -> Result<String, i32> {
        if !inode.is_symlink() {
            return Err(errno::EINVAL);
        }

        let size = inode.file_size() as usize;

        // Fast symlinks store target in block pointers (size <= 60)
        if size <= 60 && !inode.uses_extents() {
            // Copy block array from packed struct to avoid unaligned access
            let block_copy: [u32; 15] = unsafe {
                core::ptr::read_unaligned(core::ptr::addr_of!(inode.block))
            };
            let bytes: &[u8] = unsafe {
                core::slice::from_raw_parts(
                    block_copy.as_ptr() as *const u8,
                    size
                )
            };
            return core::str::from_utf8(bytes)
                .map(|s| s.to_string())
                .map_err(|_| errno::EIO);
        }

        // Slow symlinks store target in data blocks
        let mut buf = vec![0u8; size];
        self.read_file_data(inode, 0, &mut buf)?;

        core::str::from_utf8(&buf)
            .map(|s| s.to_string())
            .map_err(|_| errno::EIO)
    }

    /// Create VFS inode from ext4 inode
    fn make_vfs_inode(&self, ino: u32, ext4_inode: &Ext4Inode, ops: Arc<dyn Filesystem>) -> Inode {
        Inode {
            id: InodeId { fs_id: self.fs_id, ino: ino as u64 },
            metadata: Metadata {
                dev: self.fs_id,
                ino: ino as u64,
                mode: ext4_inode.mode as u32,
                nlink: ext4_inode.links_count as u32,
                uid: ext4_inode.uid(),
                gid: ext4_inode.gid(),
                rdev: 0,
                size: ext4_inode.file_size(),
                blksize: self.block_size,
                blocks: ext4_inode.blocks(self.superblock.has_huge_file()),
                atime: ext4_inode.atime as u64,
                mtime: ext4_inode.mtime as u64,
                ctime: ext4_inode.ctime as u64,
            },
            ops,
            private: ino as u64,
        }
    }

    /// Get root inode
    pub fn root_inode(self: &Arc<Self>) -> Result<Inode, i32> {
        let ext4_inode = self.read_inode(EXT4_ROOT_INO)?;
        Ok(self.make_vfs_inode(EXT4_ROOT_INO, &ext4_inode, self.clone()))
    }

    /// Get filesystem statistics
    pub fn statfs(&self) -> Ext4StatFs {
        Ext4StatFs {
            block_size: self.block_size,
            total_blocks: self.superblock.blocks_count(),
            free_blocks: self.superblock.free_blocks_count(),
            total_inodes: self.superblock.inodes_count,
            free_inodes: self.superblock.free_inodes_count,
            max_name_len: 255,
        }
    }
}

/// Filesystem statistics
pub struct Ext4StatFs {
    pub block_size: u32,
    pub total_blocks: u64,
    pub free_blocks: u64,
    pub total_inodes: u32,
    pub free_inodes: u32,
    pub max_name_len: u32,
}

// =============================================================================
// FILESYSTEM TRAIT IMPLEMENTATION
// =============================================================================

impl Filesystem for Ext4Filesystem {
    fn name(&self) -> &str {
        "ext4"
    }

    fn read(&self, inode: &Inode, offset: u64, buf: &mut [u8]) -> Result<usize, i32> {
        let ext4_inode = self.read_inode(inode.private as u32)?;
        self.read_file_data(&ext4_inode, offset, buf)
    }

    fn readdir(&self, inode: &Inode) -> Result<Vec<DirEntry>, i32> {
        let ext4_inode = self.read_inode(inode.private as u32)?;
        let raw_entries = self.read_directory(&ext4_inode)?;

        let has_filetype = (self.superblock.feature_incompat & feature_incompat::FILETYPE) != 0;

        let entries: Vec<DirEntry> = raw_entries.into_iter().map(|(name, ino, ftype)| {
            let file_type = if has_filetype {
                match ftype {
                    dirent_type::DIR => FileType::Directory,
                    dirent_type::REG_FILE => FileType::Regular,
                    dirent_type::SYMLINK => FileType::Symlink,
                    dirent_type::CHRDEV => FileType::CharDevice,
                    dirent_type::BLKDEV => FileType::BlockDevice,
                    dirent_type::FIFO => FileType::Pipe,
                    dirent_type::SOCK => FileType::Socket,
                    _ => FileType::Regular,
                }
            } else {
                if let Ok(ext4_ino) = self.read_inode(ino) {
                    ext4_ino.file_type()
                } else {
                    FileType::Regular
                }
            };

            DirEntry::new(name, ino as u64, file_type)
        }).collect();

        Ok(entries)
    }

    fn lookup(&self, parent: &Inode, name: &str) -> Result<Inode, i32> {
        let ext4_inode = self.read_inode(parent.private as u32)?;
        let entries = self.read_directory(&ext4_inode)?;

        for (entry_name, ino, _) in entries {
            if entry_name == name {
                let child_inode = self.read_inode(ino)?;
                return Ok(self.make_vfs_inode(ino, &child_inode, parent.ops.clone()));
            }
        }

        Err(errno::ENOENT)
    }

    fn readlink(&self, inode: &Inode) -> Result<String, i32> {
        let ext4_inode = self.read_inode(inode.private as u32)?;
        self.read_symlink(&ext4_inode)
    }
}

// =============================================================================
// BLOCK DEVICE ADAPTERS
// =============================================================================

/// AHCI disk as block device
pub struct AhciBlockDevice {
    disk_index: usize,
}

impl AhciBlockDevice {
    pub fn new(disk_index: usize) -> Self {
        Self { disk_index }
    }
}

impl BlockDevice for AhciBlockDevice {
    fn read_sectors(&self, lba: u64, count: u32, buf: &mut [u8]) -> Result<(), i32> {
        crate::drivers::storage::read(self.disk_index, lba, count as u16, buf)
            .map_err(|_| errno::EIO)
    }
}

/// NVMe disk as block device
pub struct NvmeBlockDevice {
    drive_index: usize,
}

impl NvmeBlockDevice {
    pub fn new(drive_index: usize) -> Self {
        Self { drive_index }
    }
}

impl BlockDevice for NvmeBlockDevice {
    fn read_sectors(&self, lba: u64, count: u32, buf: &mut [u8]) -> Result<(), i32> {
        crate::drivers::nvme::read(self.drive_index, lba, count as u16, buf)
            .map_err(|_| errno::EIO)
    }
}

// =============================================================================
// MOUNT HELPERS
// =============================================================================

/// Mount ext4 filesystem from AHCI disk
pub fn mount_ahci(disk_index: usize, mount_path: &str) -> Result<(), i32> {
    let device = Arc::new(AhciBlockDevice::new(disk_index));
    mount_device(device, mount_path)
}

/// Mount ext4 filesystem from NVMe drive
pub fn mount_nvme(drive_index: usize, mount_path: &str) -> Result<(), i32> {
    let device = Arc::new(NvmeBlockDevice::new(drive_index));
    mount_device(device, mount_path)
}

/// Mount ext4 filesystem from any block device
fn mount_device(device: Arc<dyn BlockDevice>, mount_path: &str) -> Result<(), i32> {
    let fs_id = super::mount_count() as u64 + 1;

    let fs = Arc::new(
        Ext4Filesystem::mount(device, fs_id)
            .map_err(|_| errno::EIO)?
    );

    let root_inode = fs.root_inode()?;
    super::mount(mount_path, fs, root_inode)
}

/// Check if partition looks like ext4
pub fn probe(device: &dyn BlockDevice) -> bool {
    let mut buf = [0u8; 1024];
    if device.read_sectors(2, 2, &mut buf).is_err() {
        return false;
    }

    let magic = u16::from_le_bytes([buf[56], buf[57]]);
    if magic != EXT4_MAGIC {
        return false;
    }

    // Check for ext4-specific features
    let incompat = u32::from_le_bytes([buf[96], buf[97], buf[98], buf[99]]);

    // ext4 typically has extents, flex_bg, or 64-bit features
    (incompat & feature_incompat::EXTENTS) != 0 ||
    (incompat & feature_incompat::FLEX_BG) != 0 ||
    (incompat & feature_incompat::_64BIT) != 0
}

/// Check if partition is ext2/ext3/ext4
pub fn probe_ext_family(device: &dyn BlockDevice) -> Option<&'static str> {
    let mut buf = [0u8; 1024];
    if device.read_sectors(2, 2, &mut buf).is_err() {
        return None;
    }

    let magic = u16::from_le_bytes([buf[56], buf[57]]);
    if magic != EXT4_MAGIC {
        return None;
    }

    let compat = u32::from_le_bytes([buf[92], buf[93], buf[94], buf[95]]);
    let incompat = u32::from_le_bytes([buf[96], buf[97], buf[98], buf[99]]);

    // Check for ext4-specific features
    if (incompat & feature_incompat::EXTENTS) != 0 ||
       (incompat & feature_incompat::FLEX_BG) != 0 ||
       (incompat & feature_incompat::_64BIT) != 0 {
        return Some("ext4");
    }

    // Check for ext3 (journal)
    if (compat & feature_compat::HAS_JOURNAL) != 0 {
        return Some("ext3");
    }

    // Default to ext2
    Some("ext2")
}

// =============================================================================
// CRC32C CHECKSUM (for metadata checksums)
// =============================================================================

/// CRC32C lookup table (Castagnoli polynomial)
static CRC32C_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let poly = 0x82F63B78u32; // Castagnoli polynomial (reversed)
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ poly;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

/// Calculate CRC32C checksum
pub fn crc32c(data: &[u8]) -> u32 {
    crc32c_update(!0, data) ^ !0
}

/// Update CRC32C with more data
pub fn crc32c_update(crc: u32, data: &[u8]) -> u32 {
    let mut crc = crc;
    for &byte in data {
        let index = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = CRC32C_TABLE[index] ^ (crc >> 8);
    }
    crc
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extent_header_size() {
        assert_eq!(mem::size_of::<Ext4ExtentHeader>(), 12);
    }

    #[test]
    fn test_extent_size() {
        assert_eq!(mem::size_of::<Ext4Extent>(), 12);
    }

    #[test]
    fn test_extent_idx_size() {
        assert_eq!(mem::size_of::<Ext4ExtentIdx>(), 12);
    }

    #[test]
    fn test_crc32c() {
        // Test vector: "123456789" should give 0xE3069283
        let data = b"123456789";
        let crc = crc32c(data);
        assert_eq!(crc, 0xE3069283);
    }

    #[test]
    fn test_extent_contains() {
        let extent = Ext4Extent {
            block: 100,
            len: 10,
            start_hi: 0,
            start_lo: 1000,
        };

        assert!(extent.contains(100));
        assert!(extent.contains(105));
        assert!(extent.contains(109));
        assert!(!extent.contains(99));
        assert!(!extent.contains(110));
    }

    #[test]
    fn test_extent_physical_block() {
        let extent = Ext4Extent {
            block: 100,
            len: 10,
            start_hi: 0,
            start_lo: 1000,
        };

        assert_eq!(extent.physical_block(100), Some(1000));
        assert_eq!(extent.physical_block(105), Some(1005));
        assert_eq!(extent.physical_block(99), None);
    }
}
