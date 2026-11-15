//! BuildOS Line Discipline
//!
//! Implements terminal line discipline for processing input/output
//! through TTY devices. Handles canonical mode editing, echo, and
//! special character processing.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

/// Line discipline types
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineDiscipline {
    /// No discipline (raw mode)
    None = 0,
    /// Standard terminal discipline (N_TTY)
    Tty = 1,
    /// SLIP (Serial Line IP)
    Slip = 2,
    /// PPP (Point-to-Point Protocol)
    Ppp = 3,
    /// HDLC
    Hdlc = 4,
}

impl Default for LineDiscipline {
    fn default() -> Self {
        Self::Tty
    }
}

/// Terminal input flags
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default)]
pub struct InputFlags(u32);

impl InputFlags {
    /// Ignore break condition
    pub const IGNBRK: u32 = 0x0001;
    /// Signal on break
    pub const BRKINT: u32 = 0x0002;
    /// Ignore parity errors
    pub const IGNPAR: u32 = 0x0004;
    /// Mark parity errors
    pub const PARMRK: u32 = 0x0008;
    /// Enable input parity checking
    pub const INPCK: u32 = 0x0010;
    /// Strip 8th bit
    pub const ISTRIP: u32 = 0x0020;
    /// Map NL to CR
    pub const INLCR: u32 = 0x0040;
    /// Ignore CR
    pub const IGNCR: u32 = 0x0080;
    /// Map CR to NL
    pub const ICRNL: u32 = 0x0100;
    /// Translate uppercase to lowercase
    pub const IUCLC: u32 = 0x0200;
    /// Enable start/stop output control
    pub const IXON: u32 = 0x0400;
    /// Any character restarts output
    pub const IXANY: u32 = 0x0800;
    /// Enable start/stop input control
    pub const IXOFF: u32 = 0x1000;
    /// Ring bell on input queue full
    pub const IMAXBEL: u32 = 0x2000;
    /// Enable UTF-8 processing
    pub const IUTF8: u32 = 0x4000;

    pub fn new() -> Self {
        Self(Self::ICRNL | Self::IXON)
    }

    pub fn set(&mut self, flag: u32, value: bool) {
        if value {
            self.0 |= flag;
        } else {
            self.0 &= !flag;
        }
    }

    pub fn is_set(&self, flag: u32) -> bool {
        (self.0 & flag) != 0
    }
}

/// Terminal output flags
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default)]
pub struct OutputFlags(u32);

impl OutputFlags {
    /// Perform output processing
    pub const OPOST: u32 = 0x0001;
    /// Map lowercase to uppercase
    pub const OLCUC: u32 = 0x0002;
    /// Map NL to CR-NL
    pub const ONLCR: u32 = 0x0004;
    /// Map CR to NL
    pub const OCRNL: u32 = 0x0008;
    /// No CR output at column 0
    pub const ONOCR: u32 = 0x0010;
    /// NL performs CR function
    pub const ONLRET: u32 = 0x0020;
    /// Use fill characters
    pub const OFILL: u32 = 0x0040;
    /// Fill is DEL
    pub const OFDEL: u32 = 0x0080;

    pub fn new() -> Self {
        Self(Self::OPOST | Self::ONLCR)
    }

    pub fn set(&mut self, flag: u32, value: bool) {
        if value {
            self.0 |= flag;
        } else {
            self.0 &= !flag;
        }
    }

    pub fn is_set(&self, flag: u32) -> bool {
        (self.0 & flag) != 0
    }
}

/// Terminal control flags
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default)]
pub struct ControlFlags(u32);

impl ControlFlags {
    /// Character size mask
    pub const CSIZE: u32 = 0x0030;
    /// 5-bit characters
    pub const CS5: u32 = 0x0000;
    /// 6-bit characters
    pub const CS6: u32 = 0x0010;
    /// 7-bit characters
    pub const CS7: u32 = 0x0020;
    /// 8-bit characters
    pub const CS8: u32 = 0x0030;
    /// Send two stop bits
    pub const CSTOPB: u32 = 0x0040;
    /// Enable receiver
    pub const CREAD: u32 = 0x0080;
    /// Enable parity
    pub const PARENB: u32 = 0x0100;
    /// Odd parity
    pub const PARODD: u32 = 0x0200;
    /// Hang up on last close
    pub const HUPCL: u32 = 0x0400;
    /// Ignore modem status
    pub const CLOCAL: u32 = 0x0800;

    pub fn new() -> Self {
        Self(Self::CS8 | Self::CREAD | Self::CLOCAL)
    }

    pub fn set(&mut self, flag: u32, value: bool) {
        if value {
            self.0 |= flag;
        } else {
            self.0 &= !flag;
        }
    }

    pub fn is_set(&self, flag: u32) -> bool {
        (self.0 & flag) != 0
    }
}

/// Terminal local flags
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default)]
pub struct LocalFlags(u32);

impl LocalFlags {
    /// Generate signals
    pub const ISIG: u32 = 0x0001;
    /// Canonical mode
    pub const ICANON: u32 = 0x0002;
    /// Map uppercase to lowercase on input
    pub const XCASE: u32 = 0x0004;
    /// Echo input
    pub const ECHO: u32 = 0x0008;
    /// Echo erase as BS-SP-BS
    pub const ECHOE: u32 = 0x0010;
    /// Echo NL after kill
    pub const ECHOK: u32 = 0x0020;
    /// Echo NL
    pub const ECHONL: u32 = 0x0040;
    /// Disable flush after interrupt
    pub const NOFLSH: u32 = 0x0080;
    /// Stop background jobs on output
    pub const TOSTOP: u32 = 0x0100;
    /// Echo control characters as ^X
    pub const ECHOCTL: u32 = 0x0200;
    /// Visual erase for line kill
    pub const ECHOKE: u32 = 0x0400;
    /// Echo pending input
    pub const ECHOPRT: u32 = 0x0800;
    /// Enable extended processing
    pub const IEXTEN: u32 = 0x8000;

    pub fn new() -> Self {
        Self(Self::ISIG | Self::ICANON | Self::ECHO | Self::ECHOE | Self::ECHOK | Self::ECHOCTL | Self::ECHOKE | Self::IEXTEN)
    }

    pub fn set(&mut self, flag: u32, value: bool) {
        if value {
            self.0 |= flag;
        } else {
            self.0 &= !flag;
        }
    }

    pub fn is_set(&self, flag: u32) -> bool {
        (self.0 & flag) != 0
    }
}

/// Special control characters
#[repr(C)]
#[derive(Clone, Debug)]
pub struct ControlChars {
    /// End of file (Ctrl-D)
    pub eof: u8,
    /// End of line
    pub eol: u8,
    /// Second end of line
    pub eol2: u8,
    /// Erase (backspace)
    pub erase: u8,
    /// Word erase (Ctrl-W)
    pub werase: u8,
    /// Kill line (Ctrl-U)
    pub kill: u8,
    /// Reprint (Ctrl-R)
    pub reprint: u8,
    /// Interrupt (Ctrl-C)
    pub intr: u8,
    /// Quit (Ctrl-\)
    pub quit: u8,
    /// Suspend (Ctrl-Z)
    pub susp: u8,
    /// Delayed suspend (Ctrl-Y)
    pub dsusp: u8,
    /// Start output (Ctrl-Q)
    pub start: u8,
    /// Stop output (Ctrl-S)
    pub stop: u8,
    /// Literal next (Ctrl-V)
    pub lnext: u8,
    /// Discard (Ctrl-O)
    pub discard: u8,
    /// Minimum chars for non-canonical read
    pub min: u8,
    /// Timeout for non-canonical read
    pub time: u8,
}

impl Default for ControlChars {
    fn default() -> Self {
        Self {
            eof: 0x04,      // Ctrl-D
            eol: 0x00,      // Disabled
            eol2: 0x00,     // Disabled
            erase: 0x7F,    // DEL
            werase: 0x17,   // Ctrl-W
            kill: 0x15,     // Ctrl-U
            reprint: 0x12,  // Ctrl-R
            intr: 0x03,     // Ctrl-C
            quit: 0x1C,     // Ctrl-\
            susp: 0x1A,     // Ctrl-Z
            dsusp: 0x19,    // Ctrl-Y
            start: 0x11,    // Ctrl-Q
            stop: 0x13,     // Ctrl-S
            lnext: 0x16,    // Ctrl-V
            discard: 0x0F,  // Ctrl-O
            min: 1,
            time: 0,
        }
    }
}

/// Terminal I/O settings (termios structure)
#[derive(Clone, Debug)]
pub struct Termios {
    /// Input flags
    pub iflag: InputFlags,
    /// Output flags
    pub oflag: OutputFlags,
    /// Control flags
    pub cflag: ControlFlags,
    /// Local flags
    pub lflag: LocalFlags,
    /// Line discipline
    pub line: LineDiscipline,
    /// Control characters
    pub cc: ControlChars,
    /// Input baud rate
    pub ispeed: u32,
    /// Output baud rate
    pub ospeed: u32,
}

impl Default for Termios {
    fn default() -> Self {
        Self {
            iflag: InputFlags::new(),
            oflag: OutputFlags::new(),
            cflag: ControlFlags::new(),
            lflag: LocalFlags::new(),
            line: LineDiscipline::default(),
            cc: ControlChars::default(),
            ispeed: 38400,
            ospeed: 38400,
        }
    }
}

impl Termios {
    /// Create raw mode settings
    pub fn make_raw(&mut self) {
        self.iflag = InputFlags(0);
        self.oflag = OutputFlags(0);
        self.lflag = LocalFlags(0);
        self.cflag.set(ControlFlags::CSIZE, false);
        self.cflag.0 |= ControlFlags::CS8;
        self.cc.min = 1;
        self.cc.time = 0;
    }

    /// Check if in canonical mode
    pub fn is_canonical(&self) -> bool {
        self.lflag.is_set(LocalFlags::ICANON)
    }

    /// Check if echo is enabled
    pub fn is_echo(&self) -> bool {
        self.lflag.is_set(LocalFlags::ECHO)
    }

    /// Check if signals are enabled
    pub fn is_isig(&self) -> bool {
        self.lflag.is_set(LocalFlags::ISIG)
    }
}

/// Signal types that can be generated
#[derive(Clone, Copy, Debug)]
pub enum TtySignal {
    /// Interrupt (SIGINT)
    Interrupt,
    /// Quit (SIGQUIT)
    Quit,
    /// Suspend (SIGTSTP)
    Suspend,
}

/// Line discipline processor
pub struct LineProcessor {
    /// Terminal settings
    termios: Termios,
    /// Input line buffer (for canonical mode)
    line_buffer: Vec<u8>,
    /// Maximum line buffer size
    max_line: usize,
    /// Cooked input queue (complete lines in canonical mode)
    cooked_queue: VecDeque<u8>,
    /// Raw input queue
    raw_queue: VecDeque<u8>,
    /// Output queue
    output_queue: VecDeque<u8>,
    /// Next character is literal
    literal_next: bool,
    /// Column position for echo
    column: usize,
    /// Output discarded
    discarding: bool,
    /// Pending signal
    pending_signal: Option<TtySignal>,
}

impl LineProcessor {
    /// Maximum line buffer size
    const MAX_LINE_SIZE: usize = 4096;
    /// Maximum queue size
    const MAX_QUEUE_SIZE: usize = 8192;

    /// Create a new line processor
    pub fn new() -> Self {
        Self {
            termios: Termios::default(),
            line_buffer: Vec::with_capacity(Self::MAX_LINE_SIZE),
            max_line: Self::MAX_LINE_SIZE,
            cooked_queue: VecDeque::with_capacity(Self::MAX_QUEUE_SIZE),
            raw_queue: VecDeque::with_capacity(Self::MAX_QUEUE_SIZE),
            output_queue: VecDeque::with_capacity(Self::MAX_QUEUE_SIZE),
            literal_next: false,
            column: 0,
            discarding: false,
            pending_signal: None,
        }
    }

    /// Get terminal settings
    pub fn get_termios(&self) -> &Termios {
        &self.termios
    }

    /// Set terminal settings
    pub fn set_termios(&mut self, termios: Termios) {
        self.termios = termios;
    }

    /// Process input byte
    pub fn process_input(&mut self, byte: u8) {
        // Handle literal next
        if self.literal_next {
            self.literal_next = false;
            self.add_to_input(byte);
            return;
        }

        // Input mapping
        let byte = self.map_input(byte);
        if byte.is_none() {
            return;
        }
        let byte = byte.unwrap();

        // Check for special characters if signals enabled
        if self.termios.is_isig() {
            if byte == self.termios.cc.intr {
                self.pending_signal = Some(TtySignal::Interrupt);
                self.flush_input();
                self.echo_control(byte);
                return;
            }
            if byte == self.termios.cc.quit {
                self.pending_signal = Some(TtySignal::Quit);
                self.flush_input();
                self.echo_control(byte);
                return;
            }
            if byte == self.termios.cc.susp {
                self.pending_signal = Some(TtySignal::Suspend);
                self.flush_input();
                self.echo_control(byte);
                return;
            }
        }

        // Flow control
        if self.termios.iflag.is_set(InputFlags::IXON) {
            if byte == self.termios.cc.stop {
                // Stop output
                return;
            }
            if byte == self.termios.cc.start {
                // Start output
                return;
            }
        }

        // Extended processing
        if self.termios.lflag.is_set(LocalFlags::IEXTEN) {
            if byte == self.termios.cc.lnext {
                self.literal_next = true;
                if self.termios.lflag.is_set(LocalFlags::ECHOCTL) {
                    self.echo_str("^");
                }
                return;
            }
            if byte == self.termios.cc.discard {
                self.discarding = !self.discarding;
                return;
            }
        }

        // Canonical mode processing
        if self.termios.is_canonical() {
            self.process_canonical(byte);
        } else {
            self.add_to_input(byte);
        }
    }

    /// Process byte in canonical mode
    fn process_canonical(&mut self, byte: u8) {
        // End of file
        if byte == self.termios.cc.eof {
            if self.line_buffer.is_empty() {
                // EOF with empty buffer signals end of input
                self.cooked_queue.push_back(0xFF); // Special EOF marker
            } else {
                self.commit_line();
            }
            return;
        }

        // End of line
        if byte == b'\n' || byte == self.termios.cc.eol || byte == self.termios.cc.eol2 {
            self.line_buffer.push(byte);
            self.commit_line();
            self.echo_byte(byte);
            return;
        }

        // Erase character
        if byte == self.termios.cc.erase {
            self.erase_char();
            return;
        }

        // Word erase
        if byte == self.termios.cc.werase {
            self.erase_word();
            return;
        }

        // Kill line
        if byte == self.termios.cc.kill {
            self.kill_line();
            return;
        }

        // Reprint line
        if byte == self.termios.cc.reprint {
            self.reprint_line();
            return;
        }

        // Regular character
        if self.line_buffer.len() < self.max_line {
            self.line_buffer.push(byte);
            self.echo_byte(byte);
        } else if self.termios.iflag.is_set(InputFlags::IMAXBEL) {
            // Ring bell
            self.output_queue.push_back(0x07);
        }
    }

    /// Map input byte according to flags
    fn map_input(&self, byte: u8) -> Option<u8> {
        let mut byte = byte;

        // Strip 8th bit
        if self.termios.iflag.is_set(InputFlags::ISTRIP) {
            byte &= 0x7F;
        }

        // CR handling
        if byte == b'\r' {
            if self.termios.iflag.is_set(InputFlags::IGNCR) {
                return None;
            }
            if self.termios.iflag.is_set(InputFlags::ICRNL) {
                byte = b'\n';
            }
        } else if byte == b'\n' && self.termios.iflag.is_set(InputFlags::INLCR) {
            byte = b'\r';
        }

        // Uppercase to lowercase
        if self.termios.iflag.is_set(InputFlags::IUCLC) {
            if byte >= b'A' && byte <= b'Z' {
                byte = byte - b'A' + b'a';
            }
        }

        Some(byte)
    }

    /// Add byte to input queue
    fn add_to_input(&mut self, byte: u8) {
        if self.termios.is_canonical() {
            if self.line_buffer.len() < self.max_line {
                self.line_buffer.push(byte);
            }
        } else {
            if self.raw_queue.len() < Self::MAX_QUEUE_SIZE {
                self.raw_queue.push_back(byte);
            }
        }
        self.echo_byte(byte);
    }

    /// Commit line buffer to cooked queue
    fn commit_line(&mut self) {
        for &byte in &self.line_buffer {
            self.cooked_queue.push_back(byte);
        }
        self.line_buffer.clear();
    }

    /// Echo byte to output
    fn echo_byte(&mut self, byte: u8) {
        if !self.termios.is_echo() {
            if byte == b'\n' && self.termios.lflag.is_set(LocalFlags::ECHONL) {
                self.output_queue.push_back(b'\n');
                self.column = 0;
            }
            return;
        }

        if byte < 0x20 && byte != b'\t' && byte != b'\n' && byte != b'\r' {
            // Control character
            if self.termios.lflag.is_set(LocalFlags::ECHOCTL) {
                self.output_queue.push_back(b'^');
                self.output_queue.push_back(byte + 0x40);
                self.column += 2;
            }
        } else if byte == 0x7F {
            // DEL
            if self.termios.lflag.is_set(LocalFlags::ECHOCTL) {
                self.output_queue.push_back(b'^');
                self.output_queue.push_back(b'?');
                self.column += 2;
            }
        } else if byte == b'\t' {
            let spaces = 8 - (self.column % 8);
            for _ in 0..spaces {
                self.output_queue.push_back(b' ');
            }
            self.column += spaces;
        } else if byte == b'\n' {
            self.output_queue.push_back(b'\n');
            self.column = 0;
        } else if byte == b'\r' {
            self.output_queue.push_back(b'\r');
            self.column = 0;
        } else {
            self.output_queue.push_back(byte);
            self.column += 1;
        }
    }

    /// Echo control character
    fn echo_control(&mut self, byte: u8) {
        if self.termios.is_echo() && self.termios.lflag.is_set(LocalFlags::ECHOCTL) {
            self.output_queue.push_back(b'^');
            self.output_queue.push_back(byte + 0x40);
            self.output_queue.push_back(b'\n');
            self.column = 0;
        }
    }

    /// Echo string
    fn echo_str(&mut self, s: &str) {
        if self.termios.is_echo() {
            for byte in s.bytes() {
                self.output_queue.push_back(byte);
            }
        }
    }

    /// Erase last character
    fn erase_char(&mut self) {
        if self.line_buffer.is_empty() {
            return;
        }

        let byte = self.line_buffer.pop().unwrap();

        if self.termios.is_echo() && self.termios.lflag.is_set(LocalFlags::ECHOE) {
            if byte < 0x20 || byte == 0x7F {
                // Control character takes 2 columns
                self.output_queue.push_back(0x08); // BS
                self.output_queue.push_back(b' ');
                self.output_queue.push_back(0x08);
                self.output_queue.push_back(0x08);
                self.output_queue.push_back(b' ');
                self.output_queue.push_back(0x08);
                self.column = self.column.saturating_sub(2);
            } else if byte == b'\t' {
                // Tab is complex, just reprint
                self.reprint_line();
            } else {
                self.output_queue.push_back(0x08); // BS
                self.output_queue.push_back(b' ');
                self.output_queue.push_back(0x08);
                self.column = self.column.saturating_sub(1);
            }
        }
    }

    /// Erase last word
    fn erase_word(&mut self) {
        // Skip trailing whitespace
        while !self.line_buffer.is_empty() {
            if let Some(&last) = self.line_buffer.last() {
                if last == b' ' || last == b'\t' {
                    self.erase_char();
                } else {
                    break;
                }
            }
        }

        // Erase word
        while !self.line_buffer.is_empty() {
            if let Some(&last) = self.line_buffer.last() {
                if last != b' ' && last != b'\t' {
                    self.erase_char();
                } else {
                    break;
                }
            }
        }
    }

    /// Kill entire line
    fn kill_line(&mut self) {
        if self.termios.is_echo() {
            if self.termios.lflag.is_set(LocalFlags::ECHOKE) {
                // Erase each character visually
                while !self.line_buffer.is_empty() {
                    self.erase_char();
                }
            } else if self.termios.lflag.is_set(LocalFlags::ECHOK) {
                self.line_buffer.clear();
                self.output_queue.push_back(b'\n');
                self.column = 0;
            } else {
                self.line_buffer.clear();
            }
        } else {
            self.line_buffer.clear();
        }
    }

    /// Reprint current line
    fn reprint_line(&mut self) {
        if !self.termios.is_echo() {
            return;
        }

        self.output_queue.push_back(b'^');
        self.output_queue.push_back(b'R');
        self.output_queue.push_back(b'\n');
        self.column = 0;

        let bytes: Vec<u8> = self.line_buffer.iter().copied().collect();
        for byte in bytes {
            self.echo_byte(byte);
        }
    }

    /// Flush input
    fn flush_input(&mut self) {
        self.line_buffer.clear();
        self.cooked_queue.clear();
        self.raw_queue.clear();
    }

    /// Flush output
    pub fn flush_output(&mut self) {
        self.output_queue.clear();
    }

    /// Read input
    pub fn read(&mut self, buf: &mut [u8]) -> usize {
        let queue = if self.termios.is_canonical() {
            &mut self.cooked_queue
        } else {
            &mut self.raw_queue
        };

        let count = buf.len().min(queue.len());
        for i in 0..count {
            buf[i] = queue.pop_front().unwrap();
        }
        count
    }

    /// Check if input is available
    pub fn input_available(&self) -> bool {
        if self.termios.is_canonical() {
            !self.cooked_queue.is_empty()
        } else {
            let min = self.termios.cc.min as usize;
            self.raw_queue.len() >= min.max(1)
        }
    }

    /// Process output byte
    pub fn process_output(&mut self, byte: u8) -> Option<u8> {
        if self.discarding {
            return None;
        }

        if !self.termios.oflag.is_set(OutputFlags::OPOST) {
            return Some(byte);
        }

        let mut byte = byte;

        // Lowercase to uppercase
        if self.termios.oflag.is_set(OutputFlags::OLCUC) {
            if byte >= b'a' && byte <= b'z' {
                byte = byte - b'a' + b'A';
            }
        }

        // NL to CR-NL
        if byte == b'\n' && self.termios.oflag.is_set(OutputFlags::ONLCR) {
            self.output_queue.push_back(b'\r');
        }

        // CR handling
        if byte == b'\r' {
            if self.termios.oflag.is_set(OutputFlags::OCRNL) {
                byte = b'\n';
            } else if self.termios.oflag.is_set(OutputFlags::ONOCR) && self.column == 0 {
                return None;
            }
        }

        Some(byte)
    }

    /// Write output
    pub fn write(&mut self, buf: &[u8]) -> usize {
        let mut count = 0;
        for &byte in buf {
            if let Some(byte) = self.process_output(byte) {
                if self.output_queue.len() < Self::MAX_QUEUE_SIZE {
                    self.output_queue.push_back(byte);
                    count += 1;

                    // Track column
                    if byte == b'\n' || byte == b'\r' {
                        self.column = 0;
                    } else if byte == b'\t' {
                        self.column = (self.column + 8) & !7;
                    } else if byte >= 0x20 && byte < 0x7F {
                        self.column += 1;
                    }
                }
            }
        }
        count
    }

    /// Get pending output
    pub fn get_output(&mut self, buf: &mut [u8]) -> usize {
        let count = buf.len().min(self.output_queue.len());
        for i in 0..count {
            buf[i] = self.output_queue.pop_front().unwrap();
        }
        count
    }

    /// Check if output is pending
    pub fn output_pending(&self) -> bool {
        !self.output_queue.is_empty()
    }

    /// Take pending signal
    pub fn take_signal(&mut self) -> Option<TtySignal> {
        self.pending_signal.take()
    }

    /// Set column position
    pub fn set_column(&mut self, col: usize) {
        self.column = col;
    }
}

/// When to apply termios changes
#[repr(i32)]
#[derive(Clone, Copy, Debug)]
pub enum SetAction {
    /// Change immediately
    Now = 0,
    /// Change after output is drained
    Drain = 1,
    /// Change after output drained and flush input
    Flush = 2,
}

/// tcgetattr/tcsetattr operations
pub mod tcattr {
    use super::*;

    /// Get terminal attributes
    pub fn get(processor: &LineProcessor) -> Termios {
        processor.termios.clone()
    }

    /// Set terminal attributes
    pub fn set(processor: &mut LineProcessor, termios: Termios, action: SetAction) {
        match action {
            SetAction::Now => {
                processor.set_termios(termios);
            }
            SetAction::Drain => {
                // Wait for output to drain, then set
                processor.set_termios(termios);
            }
            SetAction::Flush => {
                processor.flush_input();
                processor.set_termios(termios);
            }
        }
    }
}

/// cfmakeraw equivalent
pub fn cfmakeraw(termios: &mut Termios) {
    termios.make_raw();
}

/// cfgetispeed
pub fn cfgetispeed(termios: &Termios) -> u32 {
    termios.ispeed
}

/// cfgetospeed
pub fn cfgetospeed(termios: &Termios) -> u32 {
    termios.ospeed
}

/// cfsetispeed
pub fn cfsetispeed(termios: &mut Termios, speed: u32) {
    termios.ispeed = speed;
}

/// cfsetospeed
pub fn cfsetospeed(termios: &mut Termios, speed: u32) {
    termios.ospeed = speed;
}

/// cfsetspeed (both)
pub fn cfsetspeed(termios: &mut Termios, speed: u32) {
    termios.ispeed = speed;
    termios.ospeed = speed;
}
