// ===============================================================================
// FILESYSTEM OPERATIONS
// ===============================================================================

use crate::syscall::*;

// =============================================================================
// OPEN FLAGS
// =============================================================================

pub const O_RDONLY: u32 = 0;
pub const O_WRONLY: u32 = 1;
pub const O_RDWR: u32 = 2;
pub const O_CREAT: u32 = 0o100;
pub const O_EXCL: u32 = 0o200;
pub const O_TRUNC: u32 = 0o1000;
pub const O_APPEND: u32 = 0o2000;
pub const O_NONBLOCK: u32 = 0o4000;
pub const O_DIRECTORY: u32 = 0o200000;
pub const O_CLOEXEC: u32 = 0o2000000;

// =============================================================================
// SEEK CONSTANTS
// =============================================================================

pub const SEEK_SET: i32 = 0;
pub const SEEK_CUR: i32 = 1;
pub const SEEK_END: i32 = 2;

// =============================================================================
// FILE OPERATIONS
// =============================================================================

/// Open a file
pub fn open(path: &[u8], flags: u32, mode: u32) -> i32 {
    unsafe {
        syscall3(SYS_OPEN, path.as_ptr() as u64, flags as u64, mode as u64) as i32
    }
}

/// Open a file (null-terminated path)
pub fn open_cstr(path: *const u8, flags: u32, mode: u32) -> i32 {
    unsafe {
        syscall3(SYS_OPEN, path as u64, flags as u64, mode as u64) as i32
    }
}

/// Create a new file
pub fn creat(path: &[u8], mode: u32) -> i32 {
    unsafe {
        syscall2(SYS_CREAT, path.as_ptr() as u64, mode as u64) as i32
    }
}

/// Seek in a file
pub fn lseek(fd: i32, offset: i64, whence: i32) -> i64 {
    unsafe {
        syscall3(SYS_LSEEK, fd as u64, offset as u64, whence as u64) as i64
    }
}

/// Sync file to disk
pub fn fsync(fd: i32) -> i32 {
    unsafe { syscall1(SYS_FSYNC, fd as u64) as i32 }
}

// =============================================================================
// DIRECTORY OPERATIONS
// =============================================================================

/// Get current working directory
pub fn getcwd(buf: &mut [u8]) -> isize {
    unsafe {
        syscall2(SYS_GETCWD, buf.as_mut_ptr() as u64, buf.len() as u64) as isize
    }
}

/// Change current working directory
pub fn chdir(path: &[u8]) -> i32 {
    unsafe { syscall1(SYS_CHDIR, path.as_ptr() as u64) as i32 }
}

/// Change current working directory by fd
pub fn fchdir(fd: i32) -> i32 {
    unsafe { syscall1(SYS_FCHDIR, fd as u64) as i32 }
}

/// Create a directory
pub fn mkdir(path: &[u8], mode: u32) -> i32 {
    unsafe {
        syscall2(SYS_MKDIR, path.as_ptr() as u64, mode as u64) as i32
    }
}

/// Remove a directory
pub fn rmdir(path: &[u8]) -> i32 {
    unsafe { syscall1(SYS_RMDIR, path.as_ptr() as u64) as i32 }
}

// =============================================================================
// FILE METADATA
// =============================================================================

/// File stat structure
#[repr(C)]
pub struct Stat {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_nlink: u64,
    pub st_mode: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub _pad0: u32,
    pub st_rdev: u64,
    pub st_size: i64,
    pub st_blksize: i64,
    pub st_blocks: i64,
    pub st_atime: i64,
    pub st_atime_nsec: i64,
    pub st_mtime: i64,
    pub st_mtime_nsec: i64,
    pub st_ctime: i64,
    pub st_ctime_nsec: i64,
    pub _unused: [i64; 3],
}

impl Stat {
    pub const fn zeroed() -> Self {
        Self {
            st_dev: 0,
            st_ino: 0,
            st_nlink: 0,
            st_mode: 0,
            st_uid: 0,
            st_gid: 0,
            _pad0: 0,
            st_rdev: 0,
            st_size: 0,
            st_blksize: 0,
            st_blocks: 0,
            st_atime: 0,
            st_atime_nsec: 0,
            st_mtime: 0,
            st_mtime_nsec: 0,
            st_ctime: 0,
            st_ctime_nsec: 0,
            _unused: [0; 3],
        }
    }
}

/// Stat a file by path
pub fn stat(path: &[u8], buf: &mut Stat) -> i32 {
    unsafe {
        syscall2(SYS_STAT, path.as_ptr() as u64, buf as *mut Stat as u64) as i32
    }
}

/// Stat a file by fd
pub fn fstat(fd: i32, buf: &mut Stat) -> i32 {
    unsafe {
        syscall2(SYS_FSTAT, fd as u64, buf as *mut Stat as u64) as i32
    }
}

/// Stat a symlink
pub fn lstat(path: &[u8], buf: &mut Stat) -> i32 {
    unsafe {
        syscall2(SYS_LSTAT, path.as_ptr() as u64, buf as *mut Stat as u64) as i32
    }
}

// =============================================================================
// LINK OPERATIONS
// =============================================================================

/// Create a hard link
pub fn link(oldpath: &[u8], newpath: &[u8]) -> i32 {
    unsafe {
        syscall2(SYS_LINK, oldpath.as_ptr() as u64, newpath.as_ptr() as u64) as i32
    }
}

/// Delete a file
pub fn unlink(path: &[u8]) -> i32 {
    unsafe { syscall1(SYS_UNLINK, path.as_ptr() as u64) as i32 }
}

/// Rename a file
pub fn rename(oldpath: &[u8], newpath: &[u8]) -> i32 {
    unsafe {
        syscall2(SYS_RENAME, oldpath.as_ptr() as u64, newpath.as_ptr() as u64) as i32
    }
}

/// Read symlink target
pub fn readlink(path: &[u8], buf: &mut [u8]) -> isize {
    unsafe {
        syscall3(
            SYS_READLINK,
            path.as_ptr() as u64,
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
        ) as isize
    }
}

// =============================================================================
// PERMISSIONS
// =============================================================================

/// Change file mode
pub fn chmod(path: &[u8], mode: u32) -> i32 {
    unsafe {
        syscall2(SYS_CHMOD, path.as_ptr() as u64, mode as u64) as i32
    }
}

/// Change file mode by fd
pub fn fchmod(fd: i32, mode: u32) -> i32 {
    unsafe {
        syscall2(SYS_FCHMOD, fd as u64, mode as u64) as i32
    }
}

/// Change file owner
pub fn chown(path: &[u8], uid: u32, gid: u32) -> i32 {
    unsafe {
        syscall3(SYS_CHOWN, path.as_ptr() as u64, uid as u64, gid as u64) as i32
    }
}

// =============================================================================
// MODE MACROS
// =============================================================================

pub const S_IFMT: u32 = 0o170000;
pub const S_IFSOCK: u32 = 0o140000;
pub const S_IFLNK: u32 = 0o120000;
pub const S_IFREG: u32 = 0o100000;
pub const S_IFBLK: u32 = 0o060000;
pub const S_IFDIR: u32 = 0o040000;
pub const S_IFCHR: u32 = 0o020000;
pub const S_IFIFO: u32 = 0o010000;

pub const S_ISUID: u32 = 0o4000;
pub const S_ISGID: u32 = 0o2000;
pub const S_ISVTX: u32 = 0o1000;

pub const S_IRWXU: u32 = 0o700;
pub const S_IRUSR: u32 = 0o400;
pub const S_IWUSR: u32 = 0o200;
pub const S_IXUSR: u32 = 0o100;

pub const S_IRWXG: u32 = 0o070;
pub const S_IRGRP: u32 = 0o040;
pub const S_IWGRP: u32 = 0o020;
pub const S_IXGRP: u32 = 0o010;

pub const S_IRWXO: u32 = 0o007;
pub const S_IROTH: u32 = 0o004;
pub const S_IWOTH: u32 = 0o002;
pub const S_IXOTH: u32 = 0o001;

/// Check if mode indicates a regular file
pub fn s_isreg(mode: u32) -> bool {
    (mode & S_IFMT) == S_IFREG
}

/// Check if mode indicates a directory
pub fn s_isdir(mode: u32) -> bool {
    (mode & S_IFMT) == S_IFDIR
}

/// Check if mode indicates a symlink
pub fn s_islnk(mode: u32) -> bool {
    (mode & S_IFMT) == S_IFLNK
}

/// Check if mode indicates a character device
pub fn s_ischr(mode: u32) -> bool {
    (mode & S_IFMT) == S_IFCHR
}

/// Check if mode indicates a block device
pub fn s_isblk(mode: u32) -> bool {
    (mode & S_IFMT) == S_IFBLK
}

/// Check if mode indicates a FIFO
pub fn s_isfifo(mode: u32) -> bool {
    (mode & S_IFMT) == S_IFIFO
}

/// Check if mode indicates a socket
pub fn s_issock(mode: u32) -> bool {
    (mode & S_IFMT) == S_IFSOCK
}
