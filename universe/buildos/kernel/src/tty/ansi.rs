//! BuildOS ANSI Escape Sequence Parser
//!
//! Comprehensive parser for ANSI X3.64 / ECMA-48 escape sequences,
//! including DEC private sequences and xterm extensions.

use alloc::string::String;
use alloc::vec::Vec;

/// Maximum parameters in an escape sequence
const MAX_PARAMS: usize = 16;

/// Maximum intermediate bytes
const MAX_INTERMEDIATES: usize = 4;

/// Maximum OSC string length
const MAX_OSC_LEN: usize = 256;

/// Parser state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParserState {
    /// Ground state - normal character processing
    Ground,
    /// Escape state - received ESC
    Escape,
    /// Escape intermediate - ESC followed by intermediate byte
    EscapeIntermediate,
    /// CSI entry - received CSI (ESC [)
    CsiEntry,
    /// CSI parameter - collecting parameters
    CsiParam,
    /// CSI intermediate - collecting intermediate bytes
    CsiIntermediate,
    /// CSI ignore - ignoring malformed sequence
    CsiIgnore,
    /// DCS entry - Device Control String
    DcsEntry,
    /// DCS parameter
    DcsParam,
    /// DCS intermediate
    DcsIntermediate,
    /// DCS passthrough
    DcsPassthrough,
    /// DCS ignore
    DcsIgnore,
    /// OSC string - Operating System Command
    OscString,
    /// SOS/PM/APC string
    SosString,
}

impl Default for ParserState {
    fn default() -> Self {
        Self::Ground
    }
}

/// Parsed ANSI action
#[derive(Clone, Debug)]
pub enum AnsiAction {
    /// Print character
    Print(char),
    /// Execute control character (C0/C1)
    Execute(u8),
    /// CSI dispatch
    CsiDispatch(CsiSequence),
    /// ESC dispatch
    EscDispatch(EscSequence),
    /// DCS hook
    DcsHook(DcsSequence),
    /// DCS put
    DcsPut(u8),
    /// DCS unhook
    DcsUnhook,
    /// OSC dispatch
    OscDispatch(OscSequence),
    /// Clear current sequence
    Clear,
    /// Nothing to do
    None,
}

/// CSI sequence
#[derive(Clone, Debug)]
pub struct CsiSequence {
    /// Parameters
    pub params: Vec<u32>,
    /// Intermediate bytes
    pub intermediates: Vec<u8>,
    /// Final byte
    pub final_byte: u8,
    /// Private mode indicator (?)
    pub private: bool,
    /// Greater-than indicator (>)
    pub greater_than: bool,
}

impl CsiSequence {
    /// Create new CSI sequence
    pub fn new() -> Self {
        Self {
            params: Vec::new(),
            intermediates: Vec::new(),
            final_byte: 0,
            private: false,
            greater_than: false,
        }
    }

    /// Get parameter or default
    pub fn param(&self, index: usize, default: u32) -> u32 {
        self.params.get(index).copied().unwrap_or(default)
    }

    /// Get parameter, treating 0 as default
    pub fn param_or(&self, index: usize, default: u32) -> u32 {
        match self.params.get(index).copied() {
            Some(0) | None => default,
            Some(n) => n,
        }
    }
}

/// ESC sequence
#[derive(Clone, Debug)]
pub struct EscSequence {
    /// Intermediate bytes
    pub intermediates: Vec<u8>,
    /// Final byte
    pub final_byte: u8,
}

impl EscSequence {
    pub fn new() -> Self {
        Self {
            intermediates: Vec::new(),
            final_byte: 0,
        }
    }
}

/// DCS sequence
#[derive(Clone, Debug)]
pub struct DcsSequence {
    /// Parameters
    pub params: Vec<u32>,
    /// Intermediate bytes
    pub intermediates: Vec<u8>,
    /// Final byte
    pub final_byte: u8,
}

impl DcsSequence {
    pub fn new() -> Self {
        Self {
            params: Vec::new(),
            intermediates: Vec::new(),
            final_byte: 0,
        }
    }
}

/// OSC sequence
#[derive(Clone, Debug)]
pub struct OscSequence {
    /// Command number
    pub command: u32,
    /// String argument
    pub data: String,
}

impl OscSequence {
    pub fn new() -> Self {
        Self {
            command: 0,
            data: String::new(),
        }
    }
}

/// ANSI escape sequence parser
pub struct AnsiParser {
    /// Current state
    state: ParserState,
    /// Parameter accumulator
    params: [u32; MAX_PARAMS],
    /// Number of parameters
    param_count: usize,
    /// Current parameter value
    param_value: u32,
    /// Intermediate bytes
    intermediates: [u8; MAX_INTERMEDIATES],
    /// Number of intermediates
    intermediate_count: usize,
    /// Private mode flag
    private: bool,
    /// Greater-than flag
    greater_than: bool,
    /// OSC string buffer
    osc_buffer: [u8; MAX_OSC_LEN],
    /// OSC buffer position
    osc_pos: usize,
    /// OSC command number
    osc_command: u32,
    /// Collecting OSC command number
    osc_collecting_cmd: bool,
}

impl AnsiParser {
    /// Create a new parser
    pub fn new() -> Self {
        Self {
            state: ParserState::Ground,
            params: [0; MAX_PARAMS],
            param_count: 0,
            param_value: 0,
            intermediates: [0; MAX_INTERMEDIATES],
            intermediate_count: 0,
            private: false,
            greater_than: false,
            osc_buffer: [0; MAX_OSC_LEN],
            osc_pos: 0,
            osc_command: 0,
            osc_collecting_cmd: true,
        }
    }

    /// Reset parser state
    pub fn reset(&mut self) {
        self.state = ParserState::Ground;
        self.clear_params();
    }

    /// Clear parameters
    fn clear_params(&mut self) {
        self.param_count = 0;
        self.param_value = 0;
        self.intermediate_count = 0;
        self.private = false;
        self.greater_than = false;
    }

    /// Clear OSC state
    fn clear_osc(&mut self) {
        self.osc_pos = 0;
        self.osc_command = 0;
        self.osc_collecting_cmd = true;
    }

    /// Add parameter
    fn push_param(&mut self) {
        if self.param_count < MAX_PARAMS {
            self.params[self.param_count] = self.param_value;
            self.param_count += 1;
        }
        self.param_value = 0;
    }

    /// Add intermediate byte
    fn push_intermediate(&mut self, byte: u8) {
        if self.intermediate_count < MAX_INTERMEDIATES {
            self.intermediates[self.intermediate_count] = byte;
            self.intermediate_count += 1;
        }
    }

    /// Build CSI sequence
    fn build_csi(&self, final_byte: u8) -> CsiSequence {
        let mut seq = CsiSequence::new();
        seq.params = self.params[..self.param_count].to_vec();
        seq.intermediates = self.intermediates[..self.intermediate_count].to_vec();
        seq.final_byte = final_byte;
        seq.private = self.private;
        seq.greater_than = self.greater_than;
        seq
    }

    /// Build ESC sequence
    fn build_esc(&self, final_byte: u8) -> EscSequence {
        let mut seq = EscSequence::new();
        seq.intermediates = self.intermediates[..self.intermediate_count].to_vec();
        seq.final_byte = final_byte;
        seq
    }

    /// Build DCS sequence
    fn build_dcs(&self, final_byte: u8) -> DcsSequence {
        let mut seq = DcsSequence::new();
        seq.params = self.params[..self.param_count].to_vec();
        seq.intermediates = self.intermediates[..self.intermediate_count].to_vec();
        seq.final_byte = final_byte;
        seq
    }

    /// Build OSC sequence
    fn build_osc(&self) -> OscSequence {
        let mut seq = OscSequence::new();
        seq.command = self.osc_command;
        seq.data = String::from_utf8_lossy(&self.osc_buffer[..self.osc_pos]).into_owned();
        seq
    }

    /// Process a single byte
    pub fn advance(&mut self, byte: u8) -> AnsiAction {
        // Handle C0 controls anywhere (except in strings)
        if byte < 0x20 && !matches!(self.state,
            ParserState::OscString | ParserState::DcsPassthrough | ParserState::SosString
        ) {
            return self.handle_c0(byte);
        }

        // Handle DEL
        if byte == 0x7F {
            return AnsiAction::None; // Ignore DEL in most contexts
        }

        // Handle C1 controls (0x80-0x9F)
        if byte >= 0x80 && byte <= 0x9F {
            return self.handle_c1(byte);
        }

        match self.state {
            ParserState::Ground => self.ground(byte),
            ParserState::Escape => self.escape(byte),
            ParserState::EscapeIntermediate => self.escape_intermediate(byte),
            ParserState::CsiEntry => self.csi_entry(byte),
            ParserState::CsiParam => self.csi_param(byte),
            ParserState::CsiIntermediate => self.csi_intermediate(byte),
            ParserState::CsiIgnore => self.csi_ignore(byte),
            ParserState::DcsEntry => self.dcs_entry(byte),
            ParserState::DcsParam => self.dcs_param(byte),
            ParserState::DcsIntermediate => self.dcs_intermediate(byte),
            ParserState::DcsPassthrough => self.dcs_passthrough(byte),
            ParserState::DcsIgnore => self.dcs_ignore(byte),
            ParserState::OscString => self.osc_string(byte),
            ParserState::SosString => self.sos_string(byte),
        }
    }

    /// Handle C0 control (0x00-0x1F)
    fn handle_c0(&mut self, byte: u8) -> AnsiAction {
        match byte {
            0x1B => {
                self.state = ParserState::Escape;
                self.clear_params();
                AnsiAction::None
            }
            0x18 | 0x1A => {
                // CAN or SUB - cancel current sequence
                self.state = ParserState::Ground;
                AnsiAction::Execute(byte)
            }
            _ => {
                // Other C0 controls - execute immediately
                AnsiAction::Execute(byte)
            }
        }
    }

    /// Handle C1 control (0x80-0x9F)
    fn handle_c1(&mut self, byte: u8) -> AnsiAction {
        self.clear_params();
        match byte {
            0x90 => { // DCS
                self.state = ParserState::DcsEntry;
                AnsiAction::None
            }
            0x9B => { // CSI
                self.state = ParserState::CsiEntry;
                AnsiAction::None
            }
            0x9D => { // OSC
                self.state = ParserState::OscString;
                self.clear_osc();
                AnsiAction::None
            }
            0x98 | 0x9E | 0x9F => { // SOS, PM, APC
                self.state = ParserState::SosString;
                AnsiAction::None
            }
            0x9C => { // ST
                self.state = ParserState::Ground;
                AnsiAction::None
            }
            _ => AnsiAction::Execute(byte)
        }
    }

    /// Ground state
    fn ground(&mut self, byte: u8) -> AnsiAction {
        // Printable character
        if byte >= 0x20 {
            AnsiAction::Print(byte as char)
        } else {
            AnsiAction::None
        }
    }

    /// Escape state
    fn escape(&mut self, byte: u8) -> AnsiAction {
        match byte {
            0x20..=0x2F => {
                // Intermediate byte
                self.push_intermediate(byte);
                self.state = ParserState::EscapeIntermediate;
                AnsiAction::None
            }
            0x30..=0x4F | 0x51..=0x57 | 0x59 | 0x5A | 0x5C | 0x60..=0x7E => {
                // Final byte
                self.state = ParserState::Ground;
                AnsiAction::EscDispatch(self.build_esc(byte))
            }
            0x5B => { // [
                self.state = ParserState::CsiEntry;
                AnsiAction::None
            }
            0x5D => { // ]
                self.state = ParserState::OscString;
                self.clear_osc();
                AnsiAction::None
            }
            0x50 => { // P
                self.state = ParserState::DcsEntry;
                AnsiAction::None
            }
            0x58 | 0x5E | 0x5F => { // X, ^, _
                self.state = ParserState::SosString;
                AnsiAction::None
            }
            _ => {
                self.state = ParserState::Ground;
                AnsiAction::None
            }
        }
    }

    /// Escape intermediate state
    fn escape_intermediate(&mut self, byte: u8) -> AnsiAction {
        match byte {
            0x20..=0x2F => {
                self.push_intermediate(byte);
                AnsiAction::None
            }
            0x30..=0x7E => {
                self.state = ParserState::Ground;
                AnsiAction::EscDispatch(self.build_esc(byte))
            }
            _ => {
                self.state = ParserState::Ground;
                AnsiAction::None
            }
        }
    }

    /// CSI entry state
    fn csi_entry(&mut self, byte: u8) -> AnsiAction {
        match byte {
            0x30..=0x39 => {
                // Digit
                self.param_value = (byte - 0x30) as u32;
                self.state = ParserState::CsiParam;
                AnsiAction::None
            }
            0x3B => {
                // Semicolon - empty parameter
                self.push_param();
                self.state = ParserState::CsiParam;
                AnsiAction::None
            }
            0x3A => {
                // Colon - sub-parameter (treat as ignore for now)
                self.state = ParserState::CsiIgnore;
                AnsiAction::None
            }
            0x3C..=0x3F => {
                // Private mode indicator
                if byte == 0x3F {
                    self.private = true;
                } else if byte == 0x3E {
                    self.greater_than = true;
                }
                self.state = ParserState::CsiParam;
                AnsiAction::None
            }
            0x20..=0x2F => {
                self.push_intermediate(byte);
                self.state = ParserState::CsiIntermediate;
                AnsiAction::None
            }
            0x40..=0x7E => {
                self.state = ParserState::Ground;
                AnsiAction::CsiDispatch(self.build_csi(byte))
            }
            _ => {
                self.state = ParserState::CsiIgnore;
                AnsiAction::None
            }
        }
    }

    /// CSI parameter state
    fn csi_param(&mut self, byte: u8) -> AnsiAction {
        match byte {
            0x30..=0x39 => {
                self.param_value = self.param_value.saturating_mul(10)
                    .saturating_add((byte - 0x30) as u32);
                AnsiAction::None
            }
            0x3B => {
                self.push_param();
                AnsiAction::None
            }
            0x3A | 0x3C..=0x3F => {
                self.state = ParserState::CsiIgnore;
                AnsiAction::None
            }
            0x20..=0x2F => {
                self.push_param();
                self.push_intermediate(byte);
                self.state = ParserState::CsiIntermediate;
                AnsiAction::None
            }
            0x40..=0x7E => {
                self.push_param();
                self.state = ParserState::Ground;
                AnsiAction::CsiDispatch(self.build_csi(byte))
            }
            _ => {
                self.state = ParserState::CsiIgnore;
                AnsiAction::None
            }
        }
    }

    /// CSI intermediate state
    fn csi_intermediate(&mut self, byte: u8) -> AnsiAction {
        match byte {
            0x20..=0x2F => {
                self.push_intermediate(byte);
                AnsiAction::None
            }
            0x40..=0x7E => {
                self.state = ParserState::Ground;
                AnsiAction::CsiDispatch(self.build_csi(byte))
            }
            _ => {
                self.state = ParserState::CsiIgnore;
                AnsiAction::None
            }
        }
    }

    /// CSI ignore state
    fn csi_ignore(&mut self, byte: u8) -> AnsiAction {
        if byte >= 0x40 && byte <= 0x7E {
            self.state = ParserState::Ground;
        }
        AnsiAction::None
    }

    /// DCS entry state
    fn dcs_entry(&mut self, byte: u8) -> AnsiAction {
        match byte {
            0x30..=0x39 => {
                self.param_value = (byte - 0x30) as u32;
                self.state = ParserState::DcsParam;
                AnsiAction::None
            }
            0x3B => {
                self.push_param();
                self.state = ParserState::DcsParam;
                AnsiAction::None
            }
            0x3C..=0x3F => {
                self.state = ParserState::DcsParam;
                AnsiAction::None
            }
            0x20..=0x2F => {
                self.push_intermediate(byte);
                self.state = ParserState::DcsIntermediate;
                AnsiAction::None
            }
            0x40..=0x7E => {
                self.state = ParserState::DcsPassthrough;
                AnsiAction::DcsHook(self.build_dcs(byte))
            }
            _ => {
                self.state = ParserState::DcsIgnore;
                AnsiAction::None
            }
        }
    }

    /// DCS parameter state
    fn dcs_param(&mut self, byte: u8) -> AnsiAction {
        match byte {
            0x30..=0x39 => {
                self.param_value = self.param_value.saturating_mul(10)
                    .saturating_add((byte - 0x30) as u32);
                AnsiAction::None
            }
            0x3B => {
                self.push_param();
                AnsiAction::None
            }
            0x20..=0x2F => {
                self.push_param();
                self.push_intermediate(byte);
                self.state = ParserState::DcsIntermediate;
                AnsiAction::None
            }
            0x40..=0x7E => {
                self.push_param();
                self.state = ParserState::DcsPassthrough;
                AnsiAction::DcsHook(self.build_dcs(byte))
            }
            _ => {
                self.state = ParserState::DcsIgnore;
                AnsiAction::None
            }
        }
    }

    /// DCS intermediate state
    fn dcs_intermediate(&mut self, byte: u8) -> AnsiAction {
        match byte {
            0x20..=0x2F => {
                self.push_intermediate(byte);
                AnsiAction::None
            }
            0x40..=0x7E => {
                self.state = ParserState::DcsPassthrough;
                AnsiAction::DcsHook(self.build_dcs(byte))
            }
            _ => {
                self.state = ParserState::DcsIgnore;
                AnsiAction::None
            }
        }
    }

    /// DCS passthrough state
    fn dcs_passthrough(&mut self, byte: u8) -> AnsiAction {
        if byte == 0x9C { // ST
            self.state = ParserState::Ground;
            return AnsiAction::DcsUnhook;
        }
        AnsiAction::DcsPut(byte)
    }

    /// DCS ignore state
    fn dcs_ignore(&mut self, byte: u8) -> AnsiAction {
        if byte == 0x9C { // ST
            self.state = ParserState::Ground;
        }
        AnsiAction::None
    }

    /// OSC string state
    fn osc_string(&mut self, byte: u8) -> AnsiAction {
        match byte {
            0x07 | 0x9C => { // BEL or ST
                self.state = ParserState::Ground;
                AnsiAction::OscDispatch(self.build_osc())
            }
            0x1B => {
                // Check for ESC \ (ST)
                self.state = ParserState::Escape;
                AnsiAction::None
            }
            0x3B if self.osc_collecting_cmd => {
                // Semicolon separates command from data
                self.osc_collecting_cmd = false;
                AnsiAction::None
            }
            _ => {
                if self.osc_collecting_cmd {
                    if byte >= 0x30 && byte <= 0x39 {
                        self.osc_command = self.osc_command * 10 + (byte - 0x30) as u32;
                    }
                } else if self.osc_pos < MAX_OSC_LEN {
                    self.osc_buffer[self.osc_pos] = byte;
                    self.osc_pos += 1;
                }
                AnsiAction::None
            }
        }
    }

    /// SOS/PM/APC string state
    fn sos_string(&mut self, byte: u8) -> AnsiAction {
        if byte == 0x9C || byte == 0x07 { // ST or BEL
            self.state = ParserState::Ground;
        }
        AnsiAction::None
    }

    /// Get current state
    pub fn state(&self) -> ParserState {
        self.state
    }
}

/// Common CSI commands
pub mod csi {
    /// Cursor Up (CUU)
    pub const UP: u8 = b'A';
    /// Cursor Down (CUD)
    pub const DOWN: u8 = b'B';
    /// Cursor Forward (CUF)
    pub const FORWARD: u8 = b'C';
    /// Cursor Backward (CUB)
    pub const BACKWARD: u8 = b'D';
    /// Cursor Next Line (CNL)
    pub const NEXT_LINE: u8 = b'E';
    /// Cursor Previous Line (CPL)
    pub const PREV_LINE: u8 = b'F';
    /// Cursor Horizontal Absolute (CHA)
    pub const COLUMN_ABS: u8 = b'G';
    /// Cursor Position (CUP)
    pub const POSITION: u8 = b'H';
    /// Erase in Display (ED)
    pub const ERASE_DISPLAY: u8 = b'J';
    /// Erase in Line (EL)
    pub const ERASE_LINE: u8 = b'K';
    /// Insert Lines (IL)
    pub const INSERT_LINES: u8 = b'L';
    /// Delete Lines (DL)
    pub const DELETE_LINES: u8 = b'M';
    /// Insert Characters (ICH)
    pub const INSERT_CHARS: u8 = b'@';
    /// Delete Characters (DCH)
    pub const DELETE_CHARS: u8 = b'P';
    /// Erase Characters (ECH)
    pub const ERASE_CHARS: u8 = b'X';
    /// Scroll Up (SU)
    pub const SCROLL_UP: u8 = b'S';
    /// Scroll Down (SD)
    pub const SCROLL_DOWN: u8 = b'T';
    /// Cursor Position (same as H)
    pub const POSITION_F: u8 = b'f';
    /// Set Graphics Rendition (SGR)
    pub const SGR: u8 = b'm';
    /// Device Status Report (DSR)
    pub const DEVICE_STATUS: u8 = b'n';
    /// Save Cursor (DECSC via CSI)
    pub const SAVE_CURSOR: u8 = b's';
    /// Restore Cursor (DECRC via CSI)
    pub const RESTORE_CURSOR: u8 = b'u';
    /// Set Mode (SM)
    pub const SET_MODE: u8 = b'h';
    /// Reset Mode (RM)
    pub const RESET_MODE: u8 = b'l';
    /// Set Scrolling Region (DECSTBM)
    pub const SCROLL_REGION: u8 = b'r';
    /// Window Manipulation
    pub const WINDOW_MANIP: u8 = b't';
    /// Cursor Forward Tab (CHT)
    pub const FORWARD_TAB: u8 = b'I';
    /// Cursor Backward Tab (CBT)
    pub const BACKWARD_TAB: u8 = b'Z';
    /// Tab Clear (TBC)
    pub const TAB_CLEAR: u8 = b'g';
    /// Repeat Character (REP)
    pub const REPEAT: u8 = b'b';
    /// Line Position Absolute (VPA)
    pub const LINE_ABS: u8 = b'd';
    /// Send Device Attributes (Primary)
    pub const DEVICE_ATTR: u8 = b'c';
}

/// Common ESC commands
pub mod esc {
    /// Save Cursor (DECSC)
    pub const SAVE_CURSOR: u8 = b'7';
    /// Restore Cursor (DECRC)
    pub const RESTORE_CURSOR: u8 = b'8';
    /// Application Keypad (DECKPAM)
    pub const KEYPAD_APP: u8 = b'=';
    /// Numeric Keypad (DECKPNM)
    pub const KEYPAD_NUM: u8 = b'>';
    /// Index (IND)
    pub const INDEX: u8 = b'D';
    /// Next Line (NEL)
    pub const NEXT_LINE: u8 = b'E';
    /// Tab Set (HTS)
    pub const TAB_SET: u8 = b'H';
    /// Reverse Index (RI)
    pub const REVERSE_INDEX: u8 = b'M';
    /// Single Shift 2 (SS2)
    pub const SS2: u8 = b'N';
    /// Single Shift 3 (SS3)
    pub const SS3: u8 = b'O';
    /// Device Control String (DCS)
    pub const DCS: u8 = b'P';
    /// Reset to Initial State (RIS)
    pub const RESET: u8 = b'c';
}

/// Common OSC commands
pub mod osc {
    /// Set window title and icon name
    pub const SET_TITLE_AND_ICON: u32 = 0;
    /// Set icon name
    pub const SET_ICON: u32 = 1;
    /// Set window title
    pub const SET_TITLE: u32 = 2;
    /// Set X property
    pub const SET_X_PROPERTY: u32 = 3;
    /// Set/query color palette
    pub const COLOR_PALETTE: u32 = 4;
    /// Set special colors (foreground, background, cursor)
    pub const SPECIAL_COLOR: u32 = 10;
    /// Set foreground color
    pub const FOREGROUND: u32 = 10;
    /// Set background color
    pub const BACKGROUND: u32 = 11;
    /// Set cursor color
    pub const CURSOR_COLOR: u32 = 12;
    /// Reset colors
    pub const RESET_COLOR: u32 = 104;
    /// Set hyperlink
    pub const HYPERLINK: u32 = 8;
    /// Notify
    pub const NOTIFY: u32 = 9;
    /// Copy to clipboard
    pub const CLIPBOARD: u32 = 52;
}

/// DEC private modes
pub mod dec {
    /// Cursor Keys Mode (DECCKM)
    pub const CURSOR_KEYS: u32 = 1;
    /// ANSI/VT52 Mode (DECANM)
    pub const ANSI_MODE: u32 = 2;
    /// Column Mode (DECCOLM)
    pub const COLUMN_MODE: u32 = 3;
    /// Scroll Mode (DECSCLM)
    pub const SCROLL_MODE: u32 = 4;
    /// Screen Mode (DECSCNM)
    pub const SCREEN_MODE: u32 = 5;
    /// Origin Mode (DECOM)
    pub const ORIGIN_MODE: u32 = 6;
    /// Auto Wrap Mode (DECAWM)
    pub const AUTO_WRAP: u32 = 7;
    /// Auto Repeat Mode (DECARM)
    pub const AUTO_REPEAT: u32 = 8;
    /// Show Cursor (DECTCEM)
    pub const SHOW_CURSOR: u32 = 25;
    /// Allow 80/132 Column Mode
    pub const ALLOW_COLUMN_MODE: u32 = 40;
    /// Alternate Screen Buffer (xterm)
    pub const ALT_SCREEN: u32 = 47;
    /// Application Cursor Keys
    pub const APP_CURSOR: u32 = 1;
    /// Mouse Tracking: X10
    pub const MOUSE_X10: u32 = 9;
    /// Mouse Tracking: Normal
    pub const MOUSE_NORMAL: u32 = 1000;
    /// Mouse Tracking: Highlight
    pub const MOUSE_HIGHLIGHT: u32 = 1001;
    /// Mouse Tracking: Button Event
    pub const MOUSE_BUTTON: u32 = 1002;
    /// Mouse Tracking: Any Event
    pub const MOUSE_ANY: u32 = 1003;
    /// Focus Tracking
    pub const FOCUS_EVENT: u32 = 1004;
    /// Mouse UTF-8 Mode
    pub const MOUSE_UTF8: u32 = 1005;
    /// Mouse SGR Mode
    pub const MOUSE_SGR: u32 = 1006;
    /// Mouse Alternate Scroll
    pub const MOUSE_ALT_SCROLL: u32 = 1007;
    /// Alternate Screen + Clear (xterm)
    pub const ALT_SCREEN_CLEAR: u32 = 1049;
    /// Bracketed Paste Mode
    pub const BRACKETED_PASTE: u32 = 2004;
}

/// SGR (Select Graphic Rendition) attributes
pub mod sgr {
    /// Reset all attributes
    pub const RESET: u32 = 0;
    /// Bold
    pub const BOLD: u32 = 1;
    /// Dim/Faint
    pub const DIM: u32 = 2;
    /// Italic
    pub const ITALIC: u32 = 3;
    /// Underline
    pub const UNDERLINE: u32 = 4;
    /// Slow blink
    pub const BLINK: u32 = 5;
    /// Rapid blink
    pub const RAPID_BLINK: u32 = 6;
    /// Reverse video
    pub const REVERSE: u32 = 7;
    /// Hidden/Conceal
    pub const HIDDEN: u32 = 8;
    /// Strikethrough
    pub const STRIKETHROUGH: u32 = 9;
    /// Primary font
    pub const PRIMARY_FONT: u32 = 10;
    /// Double underline
    pub const DOUBLE_UNDERLINE: u32 = 21;
    /// Normal intensity (not bold/dim)
    pub const NORMAL_INTENSITY: u32 = 22;
    /// Not italic
    pub const NO_ITALIC: u32 = 23;
    /// Not underlined
    pub const NO_UNDERLINE: u32 = 24;
    /// Not blinking
    pub const NO_BLINK: u32 = 25;
    /// Not reversed
    pub const NO_REVERSE: u32 = 27;
    /// Not hidden
    pub const NO_HIDDEN: u32 = 28;
    /// Not strikethrough
    pub const NO_STRIKETHROUGH: u32 = 29;
    /// Set foreground color (30-37, 38, 90-97)
    pub const FG_BASE: u32 = 30;
    /// Set foreground color extended
    pub const FG_EXTENDED: u32 = 38;
    /// Default foreground color
    pub const FG_DEFAULT: u32 = 39;
    /// Set background color (40-47, 48, 100-107)
    pub const BG_BASE: u32 = 40;
    /// Set background color extended
    pub const BG_EXTENDED: u32 = 48;
    /// Default background color
    pub const BG_DEFAULT: u32 = 49;
    /// Overline
    pub const OVERLINE: u32 = 53;
    /// Not overlined
    pub const NO_OVERLINE: u32 = 55;
    /// Set underline color
    pub const UNDERLINE_COLOR: u32 = 58;
    /// Default underline color
    pub const UNDERLINE_DEFAULT: u32 = 59;
    /// Bright foreground base
    pub const FG_BRIGHT_BASE: u32 = 90;
    /// Bright background base
    pub const BG_BRIGHT_BASE: u32 = 100;
}
