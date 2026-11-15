// ===============================================================================
// BUILDOS KERNEL - EXT2 FILESYSTEM
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! ext2 filesystem implementation (read-only).
//!
//! The ext2 (second extended filesystem) is a widely-used Linux filesystem.
//! This implementation supports:
//! - Superblock parsing
//! - Block group descriptors
//! - Inode reading
//! - Directory traversal
//! - Regular file reading
//! - Symbolic link resolution

use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::mem;

use super::{
    DirEntry, errno, FileType, Filesystem, Inode, InodeId, Metadata,
};

// =============================================================================
// EXT2 CONSTANTS
// =============================================================================

/// Superblock magic number
const EXT2_MAGIC: u16 = 0xEF53;

/// Superblock offset (always at byte 1024)
const SUPERBLOCK_OFFSET: u64 = 1024;

/// Root inode number
const EXT2_ROOT_INO: u32 = 2;

/// Bad blocks inode
const EXT2_BAD_INO: u32 = 1;

/// Directory entry types
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
    // File type
    pub const S_IFMT: u16 = 0xF000;
    pub const S_IFSOCK: u16 = 0xC000;
    pub const S_IFLNK: u16 = 0xA000;
    pub const S_IFREG: u16 = 0x8000;
    pub const S_IFBLK: u16 = 0x6000;
    pub const S_IFDIR: u16 = 0x4000;
    pub const S_IFCHR: u16 = 0x2000;
    pub const S_IFIFO: u16 = 0x1000;

    // Permissions
    pub const S_ISUID: u16 = 0x0800;
    pub const S_ISGID: u16 = 0x0400;
    pub const S_ISVTX: u16 = 0x0200;
}

/// Feature flags (compatible)
mod feature_compat {
    pub const DIR_PREALLOC: u32 = 0x0001;
    pub const IMAGIC_INODES: u32 = 0x0002;
    pub const HAS_JOURNAL: u32 = 0x0004;
    pub const EXT_ATTR: u32 = 0x0008;
    pub const RESIZE_INO: u32 = 0x0010;
    pub const DIR_INDEX: u32 = 0x0020;
}

/// Feature flags (incompatible)
mod feature_incompat {
    pub const COMPRESSION: u32 = 0x0001;
    pub const FILETYPE: u32 = 0x0002;
    pub const RECOVER: u32 = 0x0004;
    pub const JOURNAL_DEV: u32 = 0x0008;
    pub const META_BG: u32 = 0x0010;
}

/// Feature flags (read-only compatible)
mod feature_ro_compat {
    pub const SPARSE_SUPER: u32 = 0x0001;
    pub const LARGE_FILE: u32 = 0x0002;
    pub const BTREE_DIR: u32 = 0x0004;
}

// =============================================================================
// EXT2 ON-DISK STRUCTURES
// =============================================================================

/// ext2 superblock
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Ext2Superblock {
    /// Total inodes count
    pub inodes_count: u32,
    /// Total blocks count
    pub blocks_count: u32,
    /// Reserved blocks count (for superuser)
    pub r_blocks_count: u32,
    /// Free blocks count
    pub free_blocks_count: u32,
    /// Free inodes count
    pub free_inodes_count: u32,
    /// First data block
    pub first_data_block: u32,
    /// Block size (log2(block_size) - 10)
    pub log_block_size: u32,
    /// Fragment size (log2)
    pub log_frag_size: u32,
    /// Blocks per group
    pub blocks_per_group: u32,
    /// Fragments per group
    pub frags_per_group: u32,
    /// Inodes per group
    pub inodes_per_group: u32,
    /// Last mount time
    pub mtime: u32,
    /// Last write time
    pub wtime: u32,
    /// Mount count since last fsck
    pub mnt_count: u16,
    /// Max mount count before fsck
    pub max_mnt_count: u16,
    /// Magic number (0xEF53)
    pub magic: u16,
    /// Filesystem state
    pub state: u16,
    /// Error handling behavior
    pub errors: u16,
    /// Minor revision level
    pub minor_rev_level: u16,
    /// Last check time
    pub lastcheck: u32,
    /// Check interval
    pub checkinterval: u32,
    /// Creator OS
    pub creator_os: u32,
    /// Revision level
    pub rev_level: u32,
    /// Default UID for reserved blocks
    pub def_resuid: u16,
    /// Default GID for reserved blocks
    pub def_resgid: u16,
    // -- EXT2_DYNAMIC_REV fields --
    /// First non-reserved inode
    pub first_ino: u32,
    /// Inode size
    pub inode_size: u16,
    /// Block group number of this superblock
    pub block_group_nr: u16,
    /// Compatible feature set
    pub feature_compat: u32,
    /// Incompatible feature set
    pub feature_incompat: u32,
    /// Read-only compatible feature set
    pub feature_ro_compat: u32,
    /// UUID
    pub uuid: [u8; 16],
    /// Volume name
    pub volume_name: [u8; 16],
    /// Last mounted path
    pub last_mounted: [u8; 64],
    /// Compression algorithm
    pub algorithm_usage_bitmap: u32,
    // Performance hints
    pub prealloc_blocks: u8,
    pub prealloc_dir_blocks: u8,
    _padding: u16,
    // Journaling (ext3)
    pub journal_uuid: [u8; 16],
    pub journal_inum: u32,
    pub journal_dev: u32,
    pub last_orphan: u32,
    // Directory indexing
    pub hash_seed: [u32; 4],
    pub def_hash_version: u8,
    _reserved: [u8; 3],
    // Other options
    pub default_mount_options: u32,
    pub first_meta_bg: u32,
    _reserved2: [u8; 760],
}

impl Ext2Superblock {
    /// Get block size in bytes
    pub fn block_size(&self) -> u32 {
        1024 << self.log_block_size
    }

    /// Get number of block groups
    pub fn block_group_count(&self) -> u32 {
        (self.blocks_count + self.blocks_per_group - 1) / self.blocks_per_group
    }

    /// Get inode size
    pub fn inode_size(&self) -> u32 {
        if self.rev_level >= 1 {
            self.inode_size as u32
        } else {
            128
        }
    }
}

/// Block group descriptor
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Ext2BlockGroupDesc {
    /// Block bitmap block
    pub block_bitmap: u32,
    /// Inode bitmap block
    pub inode_bitmap: u32,
    /// Inode table block
    pub inode_table: u32,
    /// Free blocks count
    pub free_blocks_count: u16,
    /// Free inodes count
    pub free_inodes_count: u16,
    /// Directories count
    pub used_dirs_count: u16,
    /// Padding
    pub pad: u16,
    /// Reserved
    pub reserved: [u8; 12],
}

/// ext2 inode structure
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Ext2Inode {
    /// File mode
    pub mode: u16,
    /// Owner UID
    pub uid: u16,
    /// Size in bytes (low 32 bits)
    pub size: u32,
    /// Access time
    pub atime: u32,
    /// Creation time
    pub ctime: u32,
    /// Modification time
    pub mtime: u32,
    /// Deletion time
    pub dtime: u32,
    /// Owner GID
    pub gid: u16,
    /// Links count
    pub links_count: u16,
    /// Blocks count (512-byte blocks)
    pub blocks: u32,
    /// File flags
    pub flags: u32,
    /// OS specific value 1
    pub osd1: u32,
    /// Block pointers (0-11: direct, 12: indirect, 13: double indirect, 14: triple indirect)
    pub block: [u32; 15],
    /// File version (for NFS)
    pub generation: u32,
    /// File ACL
    pub file_acl: u32,
    /// Directory ACL (or size_high for files)
    pub dir_acl: u32,
    /// Fragment address
    pub faddr: u32,
    /// OS specific value 2
    pub osd2: [u8; 12],
}

impl Ext2Inode {
    /// Get file size (handles large files)
    pub fn file_size(&self) -> u64 {
        let low = self.size as u64;
        // For regular files, dir_acl contains high 32 bits
        if (self.mode & inode_mode::S_IFMT) == inode_mode::S_IFREG {
            let high = (self.dir_acl as u64) << 32;
            low | high
        } else {
            low
        }
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
}

/// ext2 directory entry (variable size)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Ext2DirEntry {
    /// Inode number
    pub inode: u32,
    /// Entry length
    pub rec_len: u16,
    /// Name length
    pub name_len: u8,
    /// File type (if feature enabled)
    pub file_type: u8,
    // Name follows (variable length)
}

// =============================================================================
// BLOCK DEVICE TRAIT
// =============================================================================

/// Block device trait for reading sectors
pub trait BlockDevice: Send + Sync {
    /// Read sectors from device
    fn read_sectors(&self, lba: u64, count: u32, buf: &mut [u8]) -> Result<(), i32>;

    /// Get sector size
    fn sector_size(&self) -> u32 { 512 }
}

// =============================================================================
// EXT2 FILESYSTEM
// =============================================================================

/// ext2 filesystem instance
pub struct Ext2Filesystem {
    /// Block device
    device: Arc<dyn BlockDevice>,
    /// Superblock
    superblock: Ext2Superblock,
    /// Block group descriptors
    block_groups: Vec<Ext2BlockGroupDesc>,
    /// Block size
    block_size: u32,
    /// Filesystem ID
    fs_id: u64,
}

impl Ext2Filesystem {
    /// Mount ext2 filesystem from block device
    pub fn mount(device: Arc<dyn BlockDevice>, fs_id: u64) -> Result<Self, &'static str> {
        // Read superblock
        let mut sb_buf = [0u8; 1024];
        device.read_sectors(2, 2, &mut sb_buf)
            .map_err(|_| "Failed to read superblock")?;

        let superblock: Ext2Superblock = unsafe {
            core::ptr::read(sb_buf.as_ptr() as *const _)
        };

        // Validate magic
        if superblock.magic != EXT2_MAGIC {
            return Err("Invalid ext2 magic number");
        }

        let block_size = superblock.block_size();

        // Check for unsupported incompatible features
        let incompat = superblock.feature_incompat;
        if (incompat & feature_incompat::COMPRESSION) != 0 {
            return Err("Compression not supported");
        }
        if (incompat & feature_incompat::JOURNAL_DEV) != 0 {
            return Err("Journal device not supported");
        }

        // Read block group descriptors
        // BGD table starts at block 2 (or 1 if block_size > 1024)
        let bgd_block = if block_size > 1024 { 1 } else { 2 };
        let group_count = superblock.block_group_count() as usize;
        let bgd_size = mem::size_of::<Ext2BlockGroupDesc>();
        let bgd_bytes = group_count * bgd_size;
        let bgd_sectors = (bgd_bytes + 511) / 512;

        let mut bgd_buf = vec![0u8; bgd_sectors * 512];
        let bgd_lba = (bgd_block as u64 * block_size as u64) / 512;
        device.read_sectors(bgd_lba, bgd_sectors as u32, &mut bgd_buf)
            .map_err(|_| "Failed to read block group descriptors")?;

        let mut block_groups = Vec::with_capacity(group_count);
        for i in 0..group_count {
            let bgd: Ext2BlockGroupDesc = unsafe {
                core::ptr::read(bgd_buf.as_ptr().add(i * bgd_size) as *const _)
            };
            block_groups.push(bgd);
        }

        Ok(Self {
            device,
            superblock,
            block_groups,
            block_size,
            fs_id,
        })
    }

    /// Read a block from disk
    fn read_block(&self, block_num: u32, buf: &mut [u8]) -> Result<(), i32> {
        if buf.len() < self.block_size as usize {
            return Err(errno::EINVAL);
        }

        let lba = (block_num as u64 * self.block_size as u64) / 512;
        let sectors = self.block_size / 512;

        self.device.read_sectors(lba, sectors, &mut buf[..self.block_size as usize])
    }

    /// Read inode from disk
    fn read_inode(&self, inode_num: u32) -> Result<Ext2Inode, i32> {
        if inode_num == 0 || inode_num > self.superblock.inodes_count {
            return Err(errno::EINVAL);
        }

        // Calculate block group and offset
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

        let block_num = bgd.inode_table + block_offset;

        // Read block containing inode
        let mut block_buf = vec![0u8; self.block_size as usize];
        self.read_block(block_num, &mut block_buf)?;

        // Copy inode
        let inode: Ext2Inode = unsafe {
            core::ptr::read(block_buf.as_ptr().add(offset_in_block as usize) as *const _)
        };

        Ok(inode)
    }

    /// Read file data at offset
    fn read_file_data(&self, inode: &Ext2Inode, offset: u64, buf: &mut [u8]) -> Result<usize, i32> {
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

            // Get physical block number
            let block_num = self.get_block_num(inode, block_index)?;

            if block_num == 0 {
                // Sparse file - fill with zeros
                let available = self.block_size as usize - offset_in_block;
                let copy_size = (to_read - bytes_read).min(available);
                buf[bytes_read..bytes_read + copy_size].fill(0);
                bytes_read += copy_size;
            } else {
                // Read block
                let mut block_buf = vec![0u8; self.block_size as usize];
                self.read_block(block_num, &mut block_buf)?;

                let available = self.block_size as usize - offset_in_block;
                let copy_size = (to_read - bytes_read).min(available);
                buf[bytes_read..bytes_read + copy_size]
                    .copy_from_slice(&block_buf[offset_in_block..offset_in_block + copy_size]);
                bytes_read += copy_size;
            }
        }

        Ok(bytes_read)
    }

    /// Get physical block number for logical block
    fn get_block_num(&self, inode: &Ext2Inode, block_index: u32) -> Result<u32, i32> {
        let ptrs_per_block = self.block_size / 4;

        // Direct blocks (0-11)
        if block_index < 12 {
            return Ok(inode.block[block_index as usize]);
        }

        let block_index = block_index - 12;

        // Indirect block (12)
        if block_index < ptrs_per_block {
            let indirect_block = inode.block[12];
            if indirect_block == 0 {
                return Ok(0);
            }
            return self.read_indirect_block(indirect_block, block_index);
        }

        let block_index = block_index - ptrs_per_block;

        // Double indirect block (13)
        if block_index < ptrs_per_block * ptrs_per_block {
            let dindirect_block = inode.block[13];
            if dindirect_block == 0 {
                return Ok(0);
            }

            let indirect_index = block_index / ptrs_per_block;
            let indirect_block = self.read_indirect_block(dindirect_block, indirect_index)?;
            if indirect_block == 0 {
                return Ok(0);
            }

            let offset = block_index % ptrs_per_block;
            return self.read_indirect_block(indirect_block, offset);
        }

        let block_index = block_index - ptrs_per_block * ptrs_per_block;

        // Triple indirect block (14)
        let tindirect_block = inode.block[14];
        if tindirect_block == 0 {
            return Ok(0);
        }

        let dindirect_index = block_index / (ptrs_per_block * ptrs_per_block);
        let dindirect_block = self.read_indirect_block(tindirect_block, dindirect_index)?;
        if dindirect_block == 0 {
            return Ok(0);
        }

        let remaining = block_index % (ptrs_per_block * ptrs_per_block);
        let indirect_index = remaining / ptrs_per_block;
        let indirect_block = self.read_indirect_block(dindirect_block, indirect_index)?;
        if indirect_block == 0 {
            return Ok(0);
        }

        let offset = remaining % ptrs_per_block;
        self.read_indirect_block(indirect_block, offset)
    }

    /// Read block pointer from indirect block
    fn read_indirect_block(&self, block_num: u32, index: u32) -> Result<u32, i32> {
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

        Ok(ptr)
    }

    /// Read directory entries
    fn read_directory(&self, inode: &Ext2Inode) -> Result<Vec<(String, u32, u8)>, i32> {
        if !inode.is_dir() {
            return Err(errno::ENOTDIR);
        }

        let mut entries = Vec::new();
        let dir_size = inode.file_size();
        let mut offset = 0u64;

        while offset < dir_size {
            // Read directory block
            let block_index = (offset / self.block_size as u64) as u32;
            let block_num = self.get_block_num(inode, block_index)?;

            if block_num == 0 {
                break;
            }

            let mut block_buf = vec![0u8; self.block_size as usize];
            self.read_block(block_num, &mut block_buf)?;

            let block_offset = (offset % self.block_size as u64) as usize;
            let mut pos = block_offset;

            while pos + 8 <= self.block_size as usize {
                let entry: Ext2DirEntry = unsafe {
                    core::ptr::read(block_buf.as_ptr().add(pos) as *const _)
                };

                if entry.rec_len == 0 {
                    break;
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

    /// Read symlink target
    fn read_symlink(&self, inode: &Ext2Inode) -> Result<String, i32> {
        if !inode.is_symlink() {
            return Err(errno::EINVAL);
        }

        let size = inode.file_size() as usize;

        // Fast symlinks store target in block pointers (size <= 60)
        if size <= 60 {
            // Copy block array to avoid unaligned reference in packed struct
            let block_copy = inode.block;
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

    /// Create VFS inode from ext2 inode
    fn make_vfs_inode(&self, ino: u32, ext2_inode: &Ext2Inode, ops: Arc<dyn Filesystem>) -> Inode {
        Inode {
            id: InodeId { fs_id: self.fs_id, ino: ino as u64 },
            metadata: Metadata {
                dev: self.fs_id,
                ino: ino as u64,
                mode: ext2_inode.mode as u32,
                nlink: ext2_inode.links_count as u32,
                uid: ext2_inode.uid as u32,
                gid: ext2_inode.gid as u32,
                rdev: 0,
                size: ext2_inode.file_size(),
                blksize: self.block_size,
                blocks: ext2_inode.blocks as u64,
                atime: ext2_inode.atime as u64,
                mtime: ext2_inode.mtime as u64,
                ctime: ext2_inode.ctime as u64,
            },
            ops,
            private: ino as u64,
        }
    }

    /// Get root inode
    pub fn root_inode(self: &Arc<Self>) -> Result<Inode, i32> {
        let ext2_inode = self.read_inode(EXT2_ROOT_INO)?;
        Ok(self.make_vfs_inode(EXT2_ROOT_INO, &ext2_inode, self.clone()))
    }
}

// =============================================================================
// FILESYSTEM TRAIT IMPLEMENTATION
// =============================================================================

impl Filesystem for Ext2Filesystem {
    fn name(&self) -> &str {
        "ext2"
    }

    fn read(&self, inode: &Inode, offset: u64, buf: &mut [u8]) -> Result<usize, i32> {
        let ext2_inode = self.read_inode(inode.private as u32)?;
        self.read_file_data(&ext2_inode, offset, buf)
    }

    fn readdir(&self, inode: &Inode) -> Result<Vec<DirEntry>, i32> {
        let ext2_inode = self.read_inode(inode.private as u32)?;
        let raw_entries = self.read_directory(&ext2_inode)?;

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
                // Need to read inode to get file type
                if let Ok(ext2_ino) = self.read_inode(ino) {
                    ext2_ino.file_type()
                } else {
                    FileType::Regular
                }
            };

            DirEntry::new(name, ino as u64, file_type)
        }).collect();

        Ok(entries)
    }

    fn lookup(&self, parent: &Inode, name: &str) -> Result<Inode, i32> {
        let ext2_inode = self.read_inode(parent.private as u32)?;
        let entries = self.read_directory(&ext2_inode)?;

        for (entry_name, ino, _) in entries {
            if entry_name == name {
                let child_inode = self.read_inode(ino)?;
                return Ok(self.make_vfs_inode(ino, &child_inode, parent.ops.clone()));
            }
        }

        Err(errno::ENOENT)
    }

    fn readlink(&self, inode: &Inode) -> Result<String, i32> {
        let ext2_inode = self.read_inode(inode.private as u32)?;
        self.read_symlink(&ext2_inode)
    }
}

// =============================================================================
// AHCI BLOCK DEVICE ADAPTER
// =============================================================================

/// AHCI disk as block device
pub struct AhciBlockDevice {
    disk_index: usize,
}

impl AhciBlockDevice {
    /// Create new AHCI block device
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

// =============================================================================
// MOUNT HELPER
// =============================================================================

/// Mount ext2 filesystem from AHCI disk
pub fn mount_ahci(disk_index: usize, mount_path: &str) -> Result<(), i32> {
    let device = Arc::new(AhciBlockDevice::new(disk_index));

    // Get filesystem ID
    let fs_id = super::mount_count() as u64 + 1;

    let fs = Arc::new(
        Ext2Filesystem::mount(device, fs_id)
            .map_err(|_| errno::EIO)?
    );

    let root_inode = fs.root_inode()?;

    super::mount(mount_path, fs, root_inode)
}

/// Check if partition looks like ext2
pub fn probe(device: &dyn BlockDevice) -> bool {
    let mut buf = [0u8; 1024];
    if device.read_sectors(2, 2, &mut buf).is_err() {
        return false;
    }

    let magic = u16::from_le_bytes([buf[56], buf[57]]);
    magic == EXT2_MAGIC
}
