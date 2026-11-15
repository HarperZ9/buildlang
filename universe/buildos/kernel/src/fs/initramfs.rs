// ===============================================================================
// BUILDOS KERNEL - INITRAMFS FILESYSTEM
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! Initramfs filesystem implementation.
//!
//! Parses CPIO archives (newc format) to provide an initial root filesystem.
//! The initramfs is typically loaded by the bootloader and contains essential
//! files needed during early boot.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::str;
use spin::RwLock;

use super::{
    DirEntry, errno, FileType, Filesystem, Inode, InodeId, Metadata,
};

// =============================================================================
// CPIO NEWC FORMAT
// =============================================================================

/// CPIO newc header magic
const CPIO_MAGIC: &[u8; 6] = b"070701";

/// CPIO newc header (ASCII hex format)
#[repr(C)]
struct CpioHeader {
    /// Magic "070701"
    magic: [u8; 6],
    /// Inode number
    ino: [u8; 8],
    /// File mode
    mode: [u8; 8],
    /// User ID
    uid: [u8; 8],
    /// Group ID
    gid: [u8; 8],
    /// Number of links
    nlink: [u8; 8],
    /// Modification time
    mtime: [u8; 8],
    /// File size
    filesize: [u8; 8],
    /// Device major number
    devmajor: [u8; 8],
    /// Device minor number
    devminor: [u8; 8],
    /// Device major number (for special files)
    rdevmajor: [u8; 8],
    /// Device minor number (for special files)
    rdevminor: [u8; 8],
    /// Name length (including null terminator)
    namesize: [u8; 8],
    /// Checksum (always 0 for newc)
    check: [u8; 8],
}

impl CpioHeader {
    const SIZE: usize = 110;

    /// Parse header from bytes
    fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < Self::SIZE {
            return None;
        }

        let mut header = Self {
            magic: [0; 6],
            ino: [0; 8],
            mode: [0; 8],
            uid: [0; 8],
            gid: [0; 8],
            nlink: [0; 8],
            mtime: [0; 8],
            filesize: [0; 8],
            devmajor: [0; 8],
            devminor: [0; 8],
            rdevmajor: [0; 8],
            rdevminor: [0; 8],
            namesize: [0; 8],
            check: [0; 8],
        };

        header.magic.copy_from_slice(&data[0..6]);
        header.ino.copy_from_slice(&data[6..14]);
        header.mode.copy_from_slice(&data[14..22]);
        header.uid.copy_from_slice(&data[22..30]);
        header.gid.copy_from_slice(&data[30..38]);
        header.nlink.copy_from_slice(&data[38..46]);
        header.mtime.copy_from_slice(&data[46..54]);
        header.filesize.copy_from_slice(&data[54..62]);
        header.devmajor.copy_from_slice(&data[62..70]);
        header.devminor.copy_from_slice(&data[70..78]);
        header.rdevmajor.copy_from_slice(&data[78..86]);
        header.rdevminor.copy_from_slice(&data[86..94]);
        header.namesize.copy_from_slice(&data[94..102]);
        header.check.copy_from_slice(&data[102..110]);

        Some(header)
    }

    /// Validate magic
    fn is_valid(&self) -> bool {
        &self.magic == CPIO_MAGIC
    }

    /// Parse hex field
    fn parse_hex(field: &[u8]) -> u32 {
        let s = str::from_utf8(field).unwrap_or("0");
        u32::from_str_radix(s, 16).unwrap_or(0)
    }

    fn ino(&self) -> u64 { Self::parse_hex(&self.ino) as u64 }
    fn mode(&self) -> u32 { Self::parse_hex(&self.mode) }
    fn uid(&self) -> u32 { Self::parse_hex(&self.uid) }
    fn gid(&self) -> u32 { Self::parse_hex(&self.gid) }
    fn nlink(&self) -> u32 { Self::parse_hex(&self.nlink) }
    fn mtime(&self) -> u64 { Self::parse_hex(&self.mtime) as u64 }
    fn filesize(&self) -> u64 { Self::parse_hex(&self.filesize) as u64 }
    fn namesize(&self) -> usize { Self::parse_hex(&self.namesize) as usize }
}

/// Align to 4-byte boundary
fn align4(n: usize) -> usize {
    (n + 3) & !3
}

// =============================================================================
// INITRAMFS FILE ENTRY
// =============================================================================

/// A file entry in the initramfs
#[derive(Clone)]
struct InitramfsEntry {
    /// File name (full path)
    name: String,
    /// File metadata
    metadata: Metadata,
    /// File data (for regular files)
    data: Vec<u8>,
    /// Symlink target (for symlinks)
    link_target: Option<String>,
    /// Children (for directories)
    children: Vec<u64>,
}

impl InitramfsEntry {
    /// Create directory entry
    fn directory(name: String, ino: u64, mode: u32, uid: u32, gid: u32, mtime: u64) -> Self {
        Self {
            name,
            metadata: Metadata {
                dev: 1,
                ino,
                mode: (mode & 0o7777) | FileType::Directory.to_mode(),
                nlink: 2,
                uid,
                gid,
                rdev: 0,
                size: 0,
                blksize: 4096,
                blocks: 0,
                atime: mtime,
                mtime,
                ctime: mtime,
            },
            data: Vec::new(),
            link_target: None,
            children: Vec::new(),
        }
    }

    /// Create file entry
    fn file(name: String, ino: u64, mode: u32, uid: u32, gid: u32, mtime: u64, data: Vec<u8>) -> Self {
        let size = data.len() as u64;
        Self {
            name,
            metadata: Metadata {
                dev: 1,
                ino,
                mode: (mode & 0o7777) | FileType::Regular.to_mode(),
                nlink: 1,
                uid,
                gid,
                rdev: 0,
                size,
                blksize: 4096,
                blocks: (size + 511) / 512,
                atime: mtime,
                mtime,
                ctime: mtime,
            },
            data,
            link_target: None,
            children: Vec::new(),
        }
    }

    /// Create symlink entry
    fn symlink(name: String, ino: u64, uid: u32, gid: u32, mtime: u64, target: String) -> Self {
        let size = target.len() as u64;
        Self {
            name,
            metadata: Metadata {
                dev: 1,
                ino,
                mode: 0o777 | FileType::Symlink.to_mode(),
                nlink: 1,
                uid,
                gid,
                rdev: 0,
                size,
                blksize: 4096,
                blocks: 0,
                atime: mtime,
                mtime,
                ctime: mtime,
            },
            data: Vec::new(),
            link_target: Some(target),
            children: Vec::new(),
        }
    }
}

// =============================================================================
// INITRAMFS FILESYSTEM
// =============================================================================

/// Initramfs filesystem
pub struct InitramfsFilesystem {
    /// All entries indexed by inode number
    entries: RwLock<BTreeMap<u64, InitramfsEntry>>,
    /// Path to inode mapping
    path_to_inode: RwLock<BTreeMap<String, u64>>,
    /// Filesystem ID
    fs_id: u64,
}

impl InitramfsFilesystem {
    /// Create new empty initramfs
    pub fn new(fs_id: u64) -> Self {
        let mut entries = BTreeMap::new();
        let mut path_to_inode = BTreeMap::new();

        // Create root directory
        let root = InitramfsEntry::directory(
            String::from("/"),
            1,
            0o755,
            0,
            0,
            0,
        );
        entries.insert(1, root);
        path_to_inode.insert(String::from("/"), 1);

        Self {
            entries: RwLock::new(entries),
            path_to_inode: RwLock::new(path_to_inode),
            fs_id,
        }
    }

    /// Parse CPIO archive and populate filesystem
    pub fn parse_cpio(&self, data: &[u8]) -> Result<usize, &'static str> {
        let mut offset = 0;
        let mut count = 0;
        let mut next_ino = 2u64;

        while offset + CpioHeader::SIZE <= data.len() {
            // Parse header
            let header = CpioHeader::parse(&data[offset..])
                .ok_or("Invalid CPIO header")?;

            if !header.is_valid() {
                return Err("Invalid CPIO magic");
            }

            offset += CpioHeader::SIZE;

            // Get filename
            let namesize = header.namesize();
            if offset + namesize > data.len() {
                return Err("CPIO name extends past end");
            }

            let name_bytes = &data[offset..offset + namesize - 1]; // Exclude null terminator
            let name = str::from_utf8(name_bytes)
                .map_err(|_| "Invalid UTF-8 in filename")?;

            offset = align4(offset + namesize);

            // Check for trailer
            if name == "TRAILER!!!" {
                break;
            }

            // Get file data
            let filesize = header.filesize() as usize;
            if offset + filesize > data.len() {
                return Err("CPIO data extends past end");
            }

            let file_data = data[offset..offset + filesize].to_vec();
            offset = align4(offset + filesize);

            // Skip . entry
            if name == "." {
                continue;
            }

            // Normalize path
            let path = if name.starts_with("./") {
                format!("/{}", &name[2..])
            } else if name.starts_with('/') {
                name.to_string()
            } else {
                format!("/{}", name)
            };

            // Determine file type from mode
            let mode = header.mode();
            let file_type = FileType::from_mode(mode);

            let ino = next_ino;
            next_ino += 1;

            let entry = match file_type {
                FileType::Directory => InitramfsEntry::directory(
                    path.clone(),
                    ino,
                    mode,
                    header.uid(),
                    header.gid(),
                    header.mtime(),
                ),
                FileType::Symlink => {
                    let target = str::from_utf8(&file_data)
                        .unwrap_or("")
                        .to_string();
                    InitramfsEntry::symlink(
                        path.clone(),
                        ino,
                        header.uid(),
                        header.gid(),
                        header.mtime(),
                        target,
                    )
                }
                FileType::Regular | _ => InitramfsEntry::file(
                    path.clone(),
                    ino,
                    mode,
                    header.uid(),
                    header.gid(),
                    header.mtime(),
                    file_data,
                ),
            };

            // Add to entries
            self.entries.write().insert(ino, entry);
            self.path_to_inode.write().insert(path.clone(), ino);

            // Update parent directory
            if let Some(parent_path) = parent_of(&path) {
                let path_map = self.path_to_inode.read();
                if let Some(&parent_ino) = path_map.get(&parent_path) {
                    drop(path_map);
                    let mut entries = self.entries.write();
                    if let Some(parent) = entries.get_mut(&parent_ino) {
                        parent.children.push(ino);
                    }
                }
            }

            count += 1;
        }

        Ok(count)
    }

    /// Get root inode
    pub fn root_inode(&self) -> Inode {
        let entries = self.entries.read();
        let root = entries.get(&1).expect("Root entry missing");

        Inode {
            id: InodeId { fs_id: self.fs_id, ino: 1 },
            metadata: root.metadata.clone(),
            ops: Arc::new(InitramfsOps {
                fs: self as *const _ as *const (),
            }),
            private: 1,
        }
    }

    /// Get entry by inode
    fn get_entry(&self, ino: u64) -> Option<InitramfsEntry> {
        self.entries.read().get(&ino).cloned()
    }

    /// Lookup entry by name in parent
    fn lookup_in_parent(&self, parent_ino: u64, name: &str) -> Option<u64> {
        let entries = self.entries.read();
        let parent = entries.get(&parent_ino)?;

        let parent_path = &parent.name;
        let child_path = if parent_path == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", parent_path, name)
        };

        drop(entries);
        self.path_to_inode.read().get(&child_path).copied()
    }

    /// Make a VFS inode from an entry
    fn make_vfs_inode(&self, ino: u64, entry: &InitramfsEntry) -> Inode {
        Inode {
            id: InodeId { fs_id: self.fs_id, ino },
            metadata: entry.metadata.clone(),
            ops: Arc::new(InitramfsOps {
                fs: self as *const _ as *const (),
            }),
            private: ino,
        }
    }
}

impl Filesystem for InitramfsFilesystem {
    fn name(&self) -> &str {
        "initramfs"
    }

    fn read(&self, inode: &Inode, offset: u64, buf: &mut [u8]) -> Result<usize, i32> {
        let entry = self.get_entry(inode.private)
            .ok_or(errno::ENOENT)?;

        if entry.metadata.is_dir() {
            return Err(errno::EISDIR);
        }

        let offset = offset as usize;
        if offset >= entry.data.len() {
            return Ok(0);
        }

        let available = entry.data.len() - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&entry.data[offset..offset + to_read]);

        Ok(to_read)
    }

    fn readdir(&self, inode: &Inode) -> Result<Vec<DirEntry>, i32> {
        let entry = self.get_entry(inode.private)
            .ok_or(errno::ENOENT)?;

        if !entry.metadata.is_dir() {
            return Err(errno::ENOTDIR);
        }

        let mut entries_vec = Vec::new();

        // Add . and ..
        entries_vec.push(DirEntry::new(String::from("."), inode.private, FileType::Directory));
        entries_vec.push(DirEntry::new(String::from(".."), inode.private, FileType::Directory));

        // Add children
        let fs_entries = self.entries.read();
        for &child_ino in &entry.children {
            if let Some(child) = fs_entries.get(&child_ino) {
                entries_vec.push(DirEntry::new(
                    filename_of(&child.name).to_string(),
                    child_ino,
                    child.metadata.file_type(),
                ));
            }
        }

        Ok(entries_vec)
    }

    fn lookup(&self, parent: &Inode, name: &str) -> Result<Inode, i32> {
        // Handle . and ..
        if name == "." {
            return Ok(Inode {
                id: InodeId { fs_id: self.fs_id, ino: parent.private },
                metadata: parent.metadata.clone(),
                ops: parent.ops.clone(),
                private: parent.private,
            });
        }

        if name == ".." {
            let entry = self.get_entry(parent.private)
                .ok_or(errno::ENOENT)?;
            if let Some(parent_path) = parent_of(&entry.name) {
                let parent_ino = *self.path_to_inode.read()
                    .get(&parent_path)
                    .unwrap_or(&1);
                let parent_entry = self.get_entry(parent_ino)
                    .ok_or(errno::ENOENT)?;
                return Ok(Inode {
                    id: InodeId { fs_id: self.fs_id, ino: parent_ino },
                    metadata: parent_entry.metadata.clone(),
                    ops: parent.ops.clone(),
                    private: parent_ino,
                });
            }
        }

        // Look up child
        let child_ino = self.lookup_in_parent(parent.private, name)
            .ok_or(errno::ENOENT)?;

        let child_entry = self.get_entry(child_ino)
            .ok_or(errno::ENOENT)?;

        Ok(Inode {
            id: InodeId { fs_id: self.fs_id, ino: child_ino },
            metadata: child_entry.metadata.clone(),
            ops: parent.ops.clone(),
            private: child_ino,
        })
    }

    fn readlink(&self, inode: &Inode) -> Result<String, i32> {
        let entry = self.get_entry(inode.private)
            .ok_or(errno::ENOENT)?;

        entry.link_target.ok_or(errno::EINVAL)
    }
}

/// Get parent path
fn parent_of(path: &str) -> Option<String> {
    let path = path.trim_end_matches('/');
    match path.rfind('/') {
        Some(0) => Some(String::from("/")),
        Some(pos) => Some(path[..pos].to_string()),
        None => None,
    }
}

/// Get filename from path
fn filename_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

// =============================================================================
// FILESYSTEM OPS
// =============================================================================

/// Initramfs filesystem operations (wrapper for trait object)
struct InitramfsOps {
    fs: *const (),
}

// SAFETY: InitramfsFilesystem is behind RwLock
unsafe impl Send for InitramfsOps {}
unsafe impl Sync for InitramfsOps {}

impl InitramfsOps {
    fn fs(&self) -> &InitramfsFilesystem {
        unsafe { &*(self.fs as *const InitramfsFilesystem) }
    }
}

impl Filesystem for InitramfsOps {
    fn name(&self) -> &str {
        "initramfs"
    }

    fn read(&self, inode: &Inode, offset: u64, buf: &mut [u8]) -> Result<usize, i32> {
        let entry = self.fs().get_entry(inode.private)
            .ok_or(errno::ENOENT)?;

        if entry.metadata.is_dir() {
            return Err(errno::EISDIR);
        }

        let offset = offset as usize;
        if offset >= entry.data.len() {
            return Ok(0);
        }

        let available = entry.data.len() - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&entry.data[offset..offset + to_read]);

        Ok(to_read)
    }

    fn readdir(&self, inode: &Inode) -> Result<Vec<DirEntry>, i32> {
        let entry = self.fs().get_entry(inode.private)
            .ok_or(errno::ENOENT)?;

        if !entry.metadata.is_dir() {
            return Err(errno::ENOTDIR);
        }

        let mut entries = Vec::new();

        // Add . and ..
        entries.push(DirEntry::new(String::from("."), inode.private, FileType::Directory));
        entries.push(DirEntry::new(String::from(".."), inode.private, FileType::Directory));

        // Add children
        let fs_entries = self.fs().entries.read();
        for &child_ino in &entry.children {
            if let Some(child) = fs_entries.get(&child_ino) {
                entries.push(DirEntry::new(
                    filename_of(&child.name).to_string(),
                    child_ino,
                    child.metadata.file_type(),
                ));
            }
        }

        Ok(entries)
    }

    fn lookup(&self, parent: &Inode, name: &str) -> Result<Inode, i32> {
        // Handle . and ..
        if name == "." {
            return Ok(Inode {
                id: InodeId { fs_id: self.fs().fs_id, ino: parent.private },
                metadata: parent.metadata.clone(),
                ops: parent.ops.clone(),
                private: parent.private,
            });
        }

        if name == ".." {
            let entry = self.fs().get_entry(parent.private)
                .ok_or(errno::ENOENT)?;
            if let Some(parent_path) = parent_of(&entry.name) {
                let parent_ino = *self.fs().path_to_inode.read()
                    .get(&parent_path)
                    .unwrap_or(&1);
                let parent_entry = self.fs().get_entry(parent_ino)
                    .ok_or(errno::ENOENT)?;
                return Ok(Inode {
                    id: InodeId { fs_id: self.fs().fs_id, ino: parent_ino },
                    metadata: parent_entry.metadata.clone(),
                    ops: parent.ops.clone(),
                    private: parent_ino,
                });
            }
        }

        // Look up child
        let child_ino = self.fs().lookup_in_parent(parent.private, name)
            .ok_or(errno::ENOENT)?;

        let child_entry = self.fs().get_entry(child_ino)
            .ok_or(errno::ENOENT)?;

        Ok(Inode {
            id: InodeId { fs_id: self.fs().fs_id, ino: child_ino },
            metadata: child_entry.metadata.clone(),
            ops: parent.ops.clone(),
            private: child_ino,
        })
    }

    fn readlink(&self, inode: &Inode) -> Result<String, i32> {
        let entry = self.fs().get_entry(inode.private)
            .ok_or(errno::ENOENT)?;

        entry.link_target.ok_or(errno::EINVAL)
    }
}

// =============================================================================
// GLOBAL INITRAMFS
// =============================================================================

/// Global initramfs instance
static INITRAMFS: RwLock<Option<Arc<InitramfsFilesystem>>> = RwLock::new(None);

/// Initialize initramfs from boot info
pub fn init() {
    // In a real system, we'd get the initramfs location from boot info
    // For now, create an empty initramfs with some default directories

    let fs = Arc::new(InitramfsFilesystem::new(1));

    // Create standard directories
    create_default_dirs(&fs);

    // Mount at root
    let root_inode = fs.root_inode();

    *INITRAMFS.write() = Some(fs.clone());

    // Mount the initramfs
    if let Err(e) = super::mount("/", fs.clone(), root_inode) {
        crate::kprintln!("[INITRAMFS] Failed to mount: error {}", e);
        return;
    }

    crate::kprintln!("[INITRAMFS] Mounted initramfs at /");
}

/// Load initramfs from memory
pub fn load_from_memory(data: &[u8]) -> Result<usize, &'static str> {
    let guard = INITRAMFS.read();
    let fs = guard.as_ref().ok_or("Initramfs not initialized")?;

    fs.parse_cpio(data)
}

/// Create default directory structure
fn create_default_dirs(fs: &InitramfsFilesystem) {
    let dirs = [
        "/bin",
        "/dev",
        "/etc",
        "/home",
        "/lib",
        "/mnt",
        "/proc",
        "/root",
        "/sbin",
        "/sys",
        "/tmp",
        "/usr",
        "/usr/bin",
        "/usr/lib",
        "/usr/share",
        "/var",
        "/var/log",
        "/var/run",
    ];

    let mut next_ino = 2u64;

    for dir in &dirs {
        let ino = next_ino;
        next_ino += 1;

        let entry = InitramfsEntry::directory(
            dir.to_string(),
            ino,
            0o755,
            0,
            0,
            0,
        );

        fs.entries.write().insert(ino, entry);
        fs.path_to_inode.write().insert(dir.to_string(), ino);

        // Update parent
        if let Some(parent_path) = parent_of(dir) {
            let path_map = fs.path_to_inode.read();
            if let Some(&parent_ino) = path_map.get(&parent_path) {
                drop(path_map);
                let mut entries = fs.entries.write();
                if let Some(parent) = entries.get_mut(&parent_ino) {
                    parent.children.push(ino);
                }
            }
        }
    }

    crate::kprintln!("[INITRAMFS] Created {} default directories", dirs.len());
}

/// Get entry count
pub fn entry_count() -> usize {
    INITRAMFS.read()
        .as_ref()
        .map(|fs| fs.entries.read().len())
        .unwrap_or(0)
}

/// Arc wrapper for filesystem trait
impl Filesystem for Arc<InitramfsFilesystem> {
    fn name(&self) -> &str {
        "initramfs"
    }

    fn read(&self, inode: &Inode, offset: u64, buf: &mut [u8]) -> Result<usize, i32> {
        let entry = self.get_entry(inode.private)
            .ok_or(errno::ENOENT)?;

        if entry.metadata.is_dir() {
            return Err(errno::EISDIR);
        }

        let offset = offset as usize;
        if offset >= entry.data.len() {
            return Ok(0);
        }

        let available = entry.data.len() - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&entry.data[offset..offset + to_read]);

        Ok(to_read)
    }

    fn readdir(&self, inode: &Inode) -> Result<Vec<DirEntry>, i32> {
        let entry = self.get_entry(inode.private)
            .ok_or(errno::ENOENT)?;

        if !entry.metadata.is_dir() {
            return Err(errno::ENOTDIR);
        }

        let mut entries = Vec::new();
        entries.push(DirEntry::new(String::from("."), inode.private, FileType::Directory));
        entries.push(DirEntry::new(String::from(".."), inode.private, FileType::Directory));

        let fs_entries = self.entries.read();
        for &child_ino in &entry.children {
            if let Some(child) = fs_entries.get(&child_ino) {
                entries.push(DirEntry::new(
                    filename_of(&child.name).to_string(),
                    child_ino,
                    child.metadata.file_type(),
                ));
            }
        }

        Ok(entries)
    }

    fn lookup(&self, parent: &Inode, name: &str) -> Result<Inode, i32> {
        if name == "." {
            return Ok(Inode {
                id: InodeId { fs_id: self.fs_id, ino: parent.private },
                metadata: parent.metadata.clone(),
                ops: parent.ops.clone(),
                private: parent.private,
            });
        }

        if name == ".." {
            let entry = self.get_entry(parent.private)
                .ok_or(errno::ENOENT)?;
            if let Some(parent_path) = parent_of(&entry.name) {
                let parent_ino = *self.path_to_inode.read()
                    .get(&parent_path)
                    .unwrap_or(&1);
                let parent_entry = self.get_entry(parent_ino)
                    .ok_or(errno::ENOENT)?;
                return Ok(Inode {
                    id: InodeId { fs_id: self.fs_id, ino: parent_ino },
                    metadata: parent_entry.metadata.clone(),
                    ops: parent.ops.clone(),
                    private: parent_ino,
                });
            }
        }

        let child_ino = self.lookup_in_parent(parent.private, name)
            .ok_or(errno::ENOENT)?;

        let child_entry = self.get_entry(child_ino)
            .ok_or(errno::ENOENT)?;

        Ok(Inode {
            id: InodeId { fs_id: self.fs_id, ino: child_ino },
            metadata: child_entry.metadata.clone(),
            ops: parent.ops.clone(),
            private: child_ino,
        })
    }

    fn readlink(&self, inode: &Inode) -> Result<String, i32> {
        let entry = self.get_entry(inode.private)
            .ok_or(errno::ENOENT)?;

        entry.link_target.ok_or(errno::EINVAL)
    }
}
