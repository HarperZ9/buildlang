// ===============================================================================
// BUILDOS KERNEL - PIPES AND FIFOS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Pipe and FIFO Implementation
//!
//! Provides unidirectional data channels between processes.

#![allow(dead_code)]

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU32, AtomicBool, Ordering};

use super::IpcError;
use crate::sync::Mutex;

// =============================================================================
// CONSTANTS
// =============================================================================

/// Default pipe buffer size (64KB)
pub const PIPE_BUF_SIZE: usize = 65536;

/// Maximum pipe buffer size (1MB)
pub const PIPE_MAX_SIZE: usize = 1024 * 1024;

/// Atomic write size (POSIX guarantee)
pub const PIPE_BUF: usize = 4096;

/// Pipe flags
pub const O_NONBLOCK: u32 = 0o4000;
pub const O_CLOEXEC: u32 = 0o2000000;
pub const O_DIRECT: u32 = 0o40000;

// =============================================================================
// PIPE BUFFER
// =============================================================================

/// Pipe buffer
pub struct PipeBuffer {
    /// Data buffer
    data: VecDeque<u8>,
    /// Maximum size
    max_size: usize,
    /// Number of readers
    readers: AtomicU32,
    /// Number of writers
    writers: AtomicU32,
    /// Broken (writer closed)
    broken: AtomicBool,
}

impl PipeBuffer {
    /// Create new pipe buffer
    pub fn new(size: usize) -> Self {
        Self {
            data: VecDeque::with_capacity(size),
            max_size: size,
            readers: AtomicU32::new(0),
            writers: AtomicU32::new(0),
            broken: AtomicBool::new(false),
        }
    }

    /// Available space
    pub fn space_available(&self) -> usize {
        self.max_size.saturating_sub(self.data.len())
    }

    /// Data available
    pub fn data_available(&self) -> usize {
        self.data.len()
    }

    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Is full
    pub fn is_full(&self) -> bool {
        self.data.len() >= self.max_size
    }

    /// Write data
    pub fn write(&mut self, data: &[u8]) -> usize {
        let space = self.space_available();
        let to_write = data.len().min(space);

        for &byte in &data[..to_write] {
            self.data.push_back(byte);
        }

        to_write
    }

    /// Read data
    pub fn read(&mut self, buf: &mut [u8]) -> usize {
        let available = self.data_available();
        let to_read = buf.len().min(available);

        for i in 0..to_read {
            buf[i] = self.data.pop_front().unwrap();
        }

        to_read
    }

    /// Peek data without removing
    pub fn peek(&self, buf: &mut [u8]) -> usize {
        let available = self.data_available();
        let to_peek = buf.len().min(available);

        for (i, byte) in self.data.iter().take(to_peek).enumerate() {
            buf[i] = *byte;
        }

        to_peek
    }
}

// =============================================================================
// PIPE
// =============================================================================

/// Pipe structure
pub struct Pipe {
    /// Pipe buffer
    buffer: Mutex<PipeBuffer>,
    /// Read end closed
    read_closed: AtomicBool,
    /// Write end closed
    write_closed: AtomicBool,
    /// Non-blocking read
    nonblock_read: AtomicBool,
    /// Non-blocking write
    nonblock_write: AtomicBool,
    /// Pipe packet mode (O_DIRECT)
    packet_mode: bool,
}

impl Pipe {
    /// Create new pipe
    pub fn new(size: usize) -> Arc<Self> {
        Arc::new(Self {
            buffer: Mutex::new(PipeBuffer::new(size)),
            read_closed: AtomicBool::new(false),
            write_closed: AtomicBool::new(false),
            nonblock_read: AtomicBool::new(false),
            nonblock_write: AtomicBool::new(false),
            packet_mode: false,
        })
    }

    /// Create with flags
    pub fn with_flags(size: usize, flags: u32) -> Arc<Self> {
        let pipe = Arc::new(Self {
            buffer: Mutex::new(PipeBuffer::new(size)),
            read_closed: AtomicBool::new(false),
            write_closed: AtomicBool::new(false),
            nonblock_read: AtomicBool::new((flags & O_NONBLOCK) != 0),
            nonblock_write: AtomicBool::new((flags & O_NONBLOCK) != 0),
            packet_mode: (flags & O_DIRECT) != 0,
        });
        pipe
    }

    /// Read from pipe
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, IpcError> {
        loop {
            {
                let mut buffer = self.buffer.lock();

                if !buffer.is_empty() {
                    return Ok(buffer.read(buf));
                }

                // Check if write end closed
                if self.write_closed.load(Ordering::Acquire) {
                    return Ok(0); // EOF
                }
            }

            // Buffer empty, write end open
            if self.nonblock_read.load(Ordering::Relaxed) {
                return Err(IpcError::WouldBlock);
            }

            // Would block waiting for data
            core::hint::spin_loop();
        }
    }

    /// Write to pipe
    pub fn write(&self, data: &[u8]) -> Result<usize, IpcError> {
        // Check if read end closed
        if self.read_closed.load(Ordering::Acquire) {
            // Would send SIGPIPE
            return Err(IpcError::InvalidArgument);
        }

        let mut total_written = 0;

        while total_written < data.len() {
            {
                let mut buffer = self.buffer.lock();

                // Atomic write guarantee for <= PIPE_BUF
                if data.len() <= PIPE_BUF && buffer.space_available() < data.len() {
                    // Must write all at once
                    if self.nonblock_write.load(Ordering::Relaxed) {
                        if total_written > 0 {
                            return Ok(total_written);
                        }
                        return Err(IpcError::WouldBlock);
                    }
                } else if buffer.space_available() > 0 {
                    let written = buffer.write(&data[total_written..]);
                    total_written += written;

                    if total_written >= data.len() {
                        return Ok(total_written);
                    }
                }
            }

            // Check read end again
            if self.read_closed.load(Ordering::Acquire) {
                return Err(IpcError::InvalidArgument);
            }

            if self.nonblock_write.load(Ordering::Relaxed) {
                if total_written > 0 {
                    return Ok(total_written);
                }
                return Err(IpcError::WouldBlock);
            }

            // Would block waiting for space
            core::hint::spin_loop();
        }

        Ok(total_written)
    }

    /// Close read end
    pub fn close_read(&self) {
        self.read_closed.store(true, Ordering::Release);
    }

    /// Close write end
    pub fn close_write(&self) {
        self.write_closed.store(true, Ordering::Release);
    }

    /// Set non-blocking read
    pub fn set_nonblock_read(&self, nonblock: bool) {
        self.nonblock_read.store(nonblock, Ordering::Relaxed);
    }

    /// Set non-blocking write
    pub fn set_nonblock_write(&self, nonblock: bool) {
        self.nonblock_write.store(nonblock, Ordering::Relaxed);
    }

    /// Get pipe capacity
    pub fn capacity(&self) -> usize {
        self.buffer.lock().max_size
    }

    /// Set pipe capacity
    pub fn set_capacity(&self, size: usize) -> Result<(), IpcError> {
        if size > PIPE_MAX_SIZE {
            return Err(IpcError::InvalidArgument);
        }

        let mut buffer = self.buffer.lock();
        if size < buffer.data_available() {
            return Err(IpcError::InvalidArgument);
        }

        buffer.max_size = size;
        Ok(())
    }

    /// Get available data
    pub fn available(&self) -> usize {
        self.buffer.lock().data_available()
    }

    /// Check if readable (has data or write end closed)
    pub fn poll_read(&self) -> bool {
        let buffer = self.buffer.lock();
        !buffer.is_empty() || self.write_closed.load(Ordering::Acquire)
    }

    /// Check if writable (has space and read end open)
    pub fn poll_write(&self) -> bool {
        if self.read_closed.load(Ordering::Acquire) {
            return true; // Writable (will get EPIPE)
        }
        !self.buffer.lock().is_full()
    }
}

// =============================================================================
// PIPE PAIR (file descriptors)
// =============================================================================

/// Pipe read end
pub struct PipeRead {
    pipe: Arc<Pipe>,
}

impl PipeRead {
    pub fn new(pipe: Arc<Pipe>) -> Self {
        Self { pipe }
    }

    pub fn read(&self, buf: &mut [u8]) -> Result<usize, IpcError> {
        self.pipe.read(buf)
    }

    pub fn set_nonblock(&self, nonblock: bool) {
        self.pipe.set_nonblock_read(nonblock);
    }
}

impl Drop for PipeRead {
    fn drop(&mut self) {
        self.pipe.close_read();
    }
}

/// Pipe write end
pub struct PipeWrite {
    pipe: Arc<Pipe>,
}

impl PipeWrite {
    pub fn new(pipe: Arc<Pipe>) -> Self {
        Self { pipe }
    }

    pub fn write(&self, data: &[u8]) -> Result<usize, IpcError> {
        self.pipe.write(data)
    }

    pub fn set_nonblock(&self, nonblock: bool) {
        self.pipe.set_nonblock_write(nonblock);
    }
}

impl Drop for PipeWrite {
    fn drop(&mut self) {
        self.pipe.close_write();
    }
}

// =============================================================================
// FIFO (NAMED PIPE)
// =============================================================================

/// FIFO (named pipe)
pub struct Fifo {
    /// Underlying pipe
    pipe: Arc<Pipe>,
    /// Path
    path: alloc::string::String,
    /// Mode
    mode: u32,
}

impl Fifo {
    /// Create new FIFO
    pub fn new(path: &str, mode: u32) -> Self {
        Self {
            pipe: Pipe::new(PIPE_BUF_SIZE),
            path: path.into(),
            mode,
        }
    }

    /// Get path
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Get pipe
    pub fn pipe(&self) -> Arc<Pipe> {
        self.pipe.clone()
    }
}

// =============================================================================
// SYSTEM CALLS
// =============================================================================

/// pipe/pipe2 - create pipe
pub fn pipe(flags: u32) -> Result<(i32, i32), IpcError> {
    let _pipe = Pipe::with_flags(PIPE_BUF_SIZE, flags);

    // Would allocate file descriptors
    let read_fd = allocate_fd()?;
    let write_fd = allocate_fd()?;

    // Register file descriptors
    // ...

    Ok((read_fd, write_fd))
}

/// Initialize pipe subsystem
pub fn init() {
    crate::kprintln!("[IPC/PIPE] Pipe subsystem initialized");
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

fn allocate_fd() -> Result<i32, IpcError> {
    // Would allocate from process fd table
    static NEXT_FD: AtomicU32 = AtomicU32::new(3);
    Ok(NEXT_FD.fetch_add(1, Ordering::Relaxed) as i32)
}

// =============================================================================
// SPLICE/TEE SUPPORT
// =============================================================================

/// Splice data between pipes/sockets
pub fn splice(
    _fd_in: i32,
    _off_in: Option<u64>,
    _fd_out: i32,
    _off_out: Option<u64>,
    len: usize,
    _flags: u32,
) -> Result<usize, IpcError> {
    // Would perform zero-copy transfer
    Ok(len)
}

/// Tee - duplicate pipe data
pub fn tee(_fd_in: i32, _fd_out: i32, len: usize, _flags: u32) -> Result<usize, IpcError> {
    // Would duplicate without consuming
    Ok(len)
}

/// vmsplice - splice user pages into pipe
pub fn vmsplice(_fd: i32, _iov: &[IoVec], _flags: u32) -> Result<usize, IpcError> {
    // Would map user pages into pipe
    Ok(0)
}

/// I/O vector for scatter/gather
pub struct IoVec {
    pub base: *mut u8,
    pub len: usize,
}
