//! BuildOS Pseudo-Terminal (PTY) Implementation
//!
//! Provides pseudo-terminal pairs (master/slave) for terminal emulators
//! and remote login sessions.

use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use crate::sync::Mutex;
use super::{TerminalSize, TerminalAttrs, VirtualTerminal};

/// PTY pair identifier
pub type PtyId = u32;

/// Next PTY ID counter
static NEXT_PTY_ID: AtomicU32 = AtomicU32::new(0);

/// PTY master side
pub struct PtyMaster {
    /// PTY pair ID
    pub id: PtyId,
    /// Input buffer (from slave to master - slave's output)
    input_buffer: VecDeque<u8>,
    /// Output buffer (from master to slave - slave's input)
    output_buffer: VecDeque<u8>,
    /// Terminal size
    size: TerminalSize,
    /// Is the slave side open?
    slave_open: AtomicBool,
    /// Packet mode enabled
    packet_mode: bool,
    /// Flow control state
    flow_control: FlowControl,
}

/// PTY slave side
pub struct PtySlave {
    /// PTY pair ID
    pub id: PtyId,
    /// Reference to master
    master: Arc<Mutex<PtyMaster>>,
    /// Terminal attributes
    attrs: TerminalAttrs,
    /// Foreground process group
    foreground_pgid: Option<u32>,
    /// Session ID
    session_id: Option<u32>,
    /// Is controlling terminal
    is_ctty: bool,
}

/// PTY pair (master and slave together)
pub struct PtyPair {
    /// Master side
    pub master: Arc<Mutex<PtyMaster>>,
    /// Slave side
    pub slave: Arc<Mutex<PtySlave>>,
    /// Pair ID
    pub id: PtyId,
}

/// Flow control state
#[derive(Clone, Copy, Debug, Default)]
pub struct FlowControl {
    /// Output stopped (XOFF received)
    pub stopped: bool,
    /// Hardware flow control enabled
    pub hardware: bool,
    /// Software flow control enabled
    pub software: bool,
}

/// PTY packet mode flags
#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum PtyPacket {
    /// Data packet
    Data = 0,
    /// Flush read
    FlushRead = 1,
    /// Flush write
    FlushWrite = 2,
    /// Stop output
    Stop = 4,
    /// Start output
    Start = 8,
    /// No carrier
    NoStop = 16,
    /// Output stopped
    DoStop = 32,
    /// Input overflow
    IoCtl = 64,
}

impl PtyMaster {
    /// Create a new PTY master
    pub fn new(id: PtyId) -> Self {
        Self {
            id,
            input_buffer: VecDeque::with_capacity(4096),
            output_buffer: VecDeque::with_capacity(4096),
            size: TerminalSize::default(),
            slave_open: AtomicBool::new(true),
            packet_mode: false,
            flow_control: FlowControl::default(),
        }
    }

    /// Read from master (get slave's output)
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, PtyError> {
        if self.input_buffer.is_empty() {
            if !self.slave_open.load(Ordering::Acquire) {
                return Ok(0); // EOF
            }
            return Err(PtyError::WouldBlock);
        }

        let count = buf.len().min(self.input_buffer.len());
        for i in 0..count {
            buf[i] = self.input_buffer.pop_front().unwrap();
        }
        Ok(count)
    }

    /// Write to master (send to slave's input)
    pub fn write(&mut self, buf: &[u8]) -> Result<usize, PtyError> {
        if !self.slave_open.load(Ordering::Acquire) {
            return Err(PtyError::SlaveNotOpen);
        }

        if self.flow_control.stopped {
            return Err(PtyError::WouldBlock);
        }

        let space = 4096 - self.output_buffer.len();
        let count = buf.len().min(space);

        for i in 0..count {
            self.output_buffer.push_back(buf[i]);
        }

        Ok(count)
    }

    /// Set terminal size
    pub fn set_size(&mut self, size: TerminalSize) {
        self.size = size;
    }

    /// Get terminal size
    pub fn size(&self) -> TerminalSize {
        self.size
    }

    /// Enable packet mode
    pub fn set_packet_mode(&mut self, enabled: bool) {
        self.packet_mode = enabled;
    }

    /// Check if slave is open
    pub fn slave_is_open(&self) -> bool {
        self.slave_open.load(Ordering::Acquire)
    }

    /// Signal slave closed
    pub fn close_slave(&self) {
        self.slave_open.store(false, Ordering::Release);
    }

    /// Push data from slave
    pub fn push_from_slave(&mut self, data: &[u8]) {
        for &byte in data {
            if self.input_buffer.len() < 4096 {
                self.input_buffer.push_back(byte);
            }
        }
    }

    /// Pop data for slave
    pub fn pop_for_slave(&mut self, buf: &mut [u8]) -> usize {
        let count = buf.len().min(self.output_buffer.len());
        for i in 0..count {
            buf[i] = self.output_buffer.pop_front().unwrap();
        }
        count
    }

    /// Check if there's data available to read
    pub fn has_data(&self) -> bool {
        !self.input_buffer.is_empty()
    }

    /// Check if there's space to write
    pub fn can_write(&self) -> bool {
        self.output_buffer.len() < 4096 && !self.flow_control.stopped
    }

    /// Handle flow control
    pub fn handle_flow_control(&mut self, byte: u8) {
        if self.flow_control.software {
            match byte {
                0x13 => self.flow_control.stopped = true,  // XOFF (Ctrl-S)
                0x11 => self.flow_control.stopped = false, // XON (Ctrl-Q)
                _ => {}
            }
        }
    }
}

impl PtySlave {
    /// Create a new PTY slave
    pub fn new(id: PtyId, master: Arc<Mutex<PtyMaster>>) -> Self {
        Self {
            id,
            master,
            attrs: TerminalAttrs::default(),
            foreground_pgid: None,
            session_id: None,
            is_ctty: false,
        }
    }

    /// Read from slave (get master's output)
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, PtyError> {
        let mut master = self.master.lock();
        let count = master.pop_for_slave(buf);
        if count == 0 {
            return Err(PtyError::WouldBlock);
        }
        Ok(count)
    }

    /// Write to slave (send to master's input)
    pub fn write(&mut self, buf: &[u8]) -> Result<usize, PtyError> {
        let mut master = self.master.lock();
        master.push_from_slave(buf);
        Ok(buf.len())
    }

    /// Get terminal attributes
    pub fn get_attrs(&self) -> TerminalAttrs {
        self.attrs.clone()
    }

    /// Set terminal attributes
    pub fn set_attrs(&mut self, attrs: TerminalAttrs) {
        self.attrs = attrs;
    }

    /// Get terminal size
    pub fn get_size(&self) -> TerminalSize {
        self.master.lock().size()
    }

    /// Set foreground process group
    pub fn set_foreground_pgid(&mut self, pgid: u32) {
        self.foreground_pgid = Some(pgid);
    }

    /// Get foreground process group
    pub fn foreground_pgid(&self) -> Option<u32> {
        self.foreground_pgid
    }

    /// Set session ID
    pub fn set_session(&mut self, sid: u32) {
        self.session_id = Some(sid);
    }

    /// Make controlling terminal
    pub fn make_ctty(&mut self) {
        self.is_ctty = true;
    }

    /// Is controlling terminal
    pub fn is_ctty(&self) -> bool {
        self.is_ctty
    }
}

impl PtyPair {
    /// Create a new PTY pair
    pub fn new() -> Self {
        let id = NEXT_PTY_ID.fetch_add(1, Ordering::Relaxed);
        let master = Arc::new(Mutex::new(PtyMaster::new(id)));
        let slave = Arc::new(Mutex::new(PtySlave::new(id, master.clone())));

        Self { master, slave, id }
    }

    /// Create with specific size
    pub fn with_size(cols: u16, rows: u16) -> Self {
        let pair = Self::new();
        pair.master.lock().set_size(TerminalSize { cols, rows, xpixel: 0, ypixel: 0 });
        pair
    }
}

/// PTY errors
#[derive(Clone, Copy, Debug)]
pub enum PtyError {
    /// Operation would block
    WouldBlock,
    /// Slave side not open
    SlaveNotOpen,
    /// Master side not open
    MasterNotOpen,
    /// Invalid operation
    InvalidOperation,
    /// Buffer full
    BufferFull,
    /// No such PTY
    NoSuchPty,
}

/// PTY multiplexer - manages all PTY pairs
pub struct PtyMultiplexer {
    /// Active PTY pairs
    pairs: Vec<Arc<PtyPair>>,
    /// Maximum PTY count
    max_ptys: usize,
}

impl PtyMultiplexer {
    /// Create a new PTY multiplexer
    pub const fn new() -> Self {
        Self {
            pairs: Vec::new(),
            max_ptys: 256,
        }
    }

    /// Open a new PTY pair
    pub fn open_pty(&mut self) -> Result<Arc<PtyPair>, PtyError> {
        if self.pairs.len() >= self.max_ptys {
            return Err(PtyError::BufferFull);
        }

        let pair = Arc::new(PtyPair::new());
        self.pairs.push(pair.clone());
        Ok(pair)
    }

    /// Open a new PTY pair with specific size
    pub fn open_pty_with_size(&mut self, cols: u16, rows: u16) -> Result<Arc<PtyPair>, PtyError> {
        if self.pairs.len() >= self.max_ptys {
            return Err(PtyError::BufferFull);
        }

        let pair = Arc::new(PtyPair::with_size(cols, rows));
        self.pairs.push(pair.clone());
        Ok(pair)
    }

    /// Close a PTY pair
    pub fn close_pty(&mut self, id: PtyId) -> Result<(), PtyError> {
        if let Some(pos) = self.pairs.iter().position(|p| p.id == id) {
            let pair = self.pairs.remove(pos);
            pair.master.lock().close_slave();
            Ok(())
        } else {
            Err(PtyError::NoSuchPty)
        }
    }

    /// Get PTY pair by ID
    pub fn get_pty(&self, id: PtyId) -> Option<Arc<PtyPair>> {
        self.pairs.iter().find(|p| p.id == id).cloned()
    }

    /// Get count of active PTYs
    pub fn count(&self) -> usize {
        self.pairs.len()
    }
}

/// Global PTY multiplexer
static PTY_MUX: Mutex<PtyMultiplexer> = Mutex::new(PtyMultiplexer::new());

/// Open a new PTY pair
pub fn openpty() -> Result<Arc<PtyPair>, PtyError> {
    PTY_MUX.lock().open_pty()
}

/// Open a new PTY pair with specific size
pub fn openpty_with_size(cols: u16, rows: u16) -> Result<Arc<PtyPair>, PtyError> {
    PTY_MUX.lock().open_pty_with_size(cols, rows)
}

/// Close a PTY pair
pub fn close_pty(id: PtyId) -> Result<(), PtyError> {
    PTY_MUX.lock().close_pty(id)
}

/// Get PTY by ID
pub fn get_pty(id: PtyId) -> Option<Arc<PtyPair>> {
    PTY_MUX.lock().get_pty(id)
}

/// PTY-backed virtual terminal
pub struct PtyTerminal {
    /// The PTY pair
    pty: Arc<PtyPair>,
    /// Virtual terminal for rendering
    terminal: VirtualTerminal,
}

impl PtyTerminal {
    /// Create a new PTY-backed terminal
    pub fn new(cols: u16, rows: u16) -> Result<Self, PtyError> {
        let pty = openpty_with_size(cols, rows)?;
        let terminal = VirtualTerminal::with_size(0, cols as usize, rows as usize);

        Ok(Self { pty, terminal })
    }

    /// Get the PTY pair
    pub fn pty(&self) -> &Arc<PtyPair> {
        &self.pty
    }

    /// Get the virtual terminal
    pub fn terminal(&self) -> &VirtualTerminal {
        &self.terminal
    }

    /// Get mutable virtual terminal
    pub fn terminal_mut(&mut self) -> &mut VirtualTerminal {
        &mut self.terminal
    }

    /// Process output from master (render to terminal)
    pub fn process_output(&mut self) {
        let mut buf = [0u8; 256];
        loop {
            let mut master = self.pty.master.lock();
            let count = master.pop_for_slave(&mut buf);
            drop(master);

            if count == 0 {
                break;
            }

            for i in 0..count {
                self.terminal.process_byte(buf[i]);
            }
        }
    }

    /// Send input to slave
    pub fn send_input(&self, data: &[u8]) {
        let mut master = self.pty.master.lock();
        for &byte in data {
            master.output_buffer.push_back(byte);
        }
    }

    /// Resize the terminal
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.pty.master.lock().set_size(TerminalSize { cols, rows, xpixel: 0, ypixel: 0 });
        self.terminal.resize(cols, rows);
    }
}

/// Unix98 PTY device nodes
pub mod devpts {
    use super::*;

    /// /dev/ptmx device - opens new PTY master
    pub struct PtmxDevice;

    impl PtmxDevice {
        /// Open and get PTY pair
        pub fn open() -> Result<Arc<PtyPair>, PtyError> {
            openpty()
        }
    }

    /// /dev/pts/N device - PTY slave
    pub struct PtsDevice {
        id: PtyId,
    }

    impl PtsDevice {
        /// Open PTY slave by number
        pub fn open(id: PtyId) -> Result<Self, PtyError> {
            if get_pty(id).is_some() {
                Ok(Self { id })
            } else {
                Err(PtyError::NoSuchPty)
            }
        }

        /// Get the PTY pair
        pub fn get_pty(&self) -> Option<Arc<PtyPair>> {
            get_pty(self.id)
        }
    }

    /// Grant access to PTY slave (grantpt equivalent)
    pub fn grantpt(_id: PtyId) -> Result<(), PtyError> {
        // In a full implementation, this would set ownership/permissions
        Ok(())
    }

    /// Unlock PTY slave (unlockpt equivalent)
    pub fn unlockpt(_id: PtyId) -> Result<(), PtyError> {
        // In a full implementation, this would unlock the slave
        Ok(())
    }

    /// Get PTY slave name (ptsname equivalent)
    pub fn ptsname(id: PtyId) -> String {
        alloc::format!("/dev/pts/{}", id)
    }
}

/// Winsize structure for TIOCGWINSZ/TIOCSWINSZ
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Winsize {
    /// Rows
    pub ws_row: u16,
    /// Columns
    pub ws_col: u16,
    /// Horizontal pixels (unused)
    pub ws_xpixel: u16,
    /// Vertical pixels (unused)
    pub ws_ypixel: u16,
}

impl From<TerminalSize> for Winsize {
    fn from(size: TerminalSize) -> Self {
        Self {
            ws_row: size.rows,
            ws_col: size.cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        }
    }
}

impl From<Winsize> for TerminalSize {
    fn from(ws: Winsize) -> Self {
        Self {
            cols: ws.ws_col,
            rows: ws.ws_row,
            xpixel: ws.ws_xpixel,
            ypixel: ws.ws_ypixel,
        }
    }
}
