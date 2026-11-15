//! RFCOMM (Radio Frequency Communication)
//!
//! Serial port emulation over Bluetooth.

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use spin::{Mutex, RwLock};

use super::l2cap::{L2capChannel, L2capManager, L2CAP_PSM_RFCOMM};
use super::BluetoothError;

// =============================================================================
// RFCOMM CONSTANTS
// =============================================================================

/// Default RFCOMM MTU
pub const RFCOMM_DEFAULT_MTU: u16 = 127;

/// Maximum RFCOMM MTU
pub const RFCOMM_MAX_MTU: u16 = 32767;

/// Number of RFCOMM channels (1-30)
pub const RFCOMM_MAX_CHANNELS: u8 = 30;

/// Control channel DLCI
pub const RFCOMM_CTRL_DLCI: u8 = 0;

// =============================================================================
// RFCOMM FRAME TYPES
// =============================================================================

/// RFCOMM frame types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RfcommFrameType {
    /// Set Asynchronous Balanced Mode (connect)
    Sabm = 0x2F,
    /// Unnumbered Acknowledgement
    Ua = 0x63,
    /// Disconnected Mode
    Dm = 0x0F,
    /// Disconnect
    Disc = 0x43,
    /// Unnumbered Information with Header check
    Uih = 0xEF,
    /// Unnumbered Information with Header check (with credits)
    UihCredits = 0xFF,
}

impl RfcommFrameType {
    fn from_byte(byte: u8) -> Option<Self> {
        // Mask out P/F bit
        match byte & 0xEF {
            0x2F => Some(Self::Sabm),
            0x63 => Some(Self::Ua),
            0x0F => Some(Self::Dm),
            0x43 => Some(Self::Disc),
            0xEF => Some(Self::Uih),
            _ => None,
        }
    }
}

// =============================================================================
// RFCOMM MCC (Multiplexer Control Commands)
// =============================================================================

/// Multiplexer control command types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MccType {
    /// Parameter Negotiation
    Pn = 0x20,
    /// Power Saving Control
    Psc = 0x10,
    /// Close Down Multiplexer
    Cld = 0x30,
    /// Test
    Test = 0x08,
    /// Flow Control On All
    FcOn = 0x28,
    /// Flow Control Off All
    FcOff = 0x18,
    /// Modem Status Command
    Msc = 0x38,
    /// Non-Supported Command Response
    Nsc = 0x04,
    /// Remote Port Negotiation
    Rpn = 0x24,
    /// Remote Line Status
    Rls = 0x14,
    /// Service Negotiation
    Sn = 0x34,
}

impl MccType {
    fn from_byte(byte: u8) -> Option<Self> {
        // Mask out C/R and EA bits
        match byte & 0xFC {
            0x20 => Some(Self::Pn),
            0x10 => Some(Self::Psc),
            0x30 => Some(Self::Cld),
            0x08 => Some(Self::Test),
            0x28 => Some(Self::FcOn),
            0x18 => Some(Self::FcOff),
            0x38 => Some(Self::Msc),
            0x04 => Some(Self::Nsc),
            0x24 => Some(Self::Rpn),
            0x14 => Some(Self::Rls),
            0x34 => Some(Self::Sn),
            _ => None,
        }
    }
}

// =============================================================================
// MODEM SIGNALS
// =============================================================================

/// Modem status signals
#[derive(Clone, Copy, Debug, Default)]
pub struct ModemSignals {
    /// Flow control (FC)
    pub fc: bool,
    /// Ready to communicate (RTC)
    pub rtc: bool,
    /// Ready to receive (RTR)
    pub rtr: bool,
    /// Incoming call (IC)
    pub ic: bool,
    /// Data valid (DV)
    pub dv: bool,
}

impl ModemSignals {
    /// Parse from byte
    pub fn from_byte(byte: u8) -> Self {
        Self {
            fc: (byte & 0x02) != 0,
            rtc: (byte & 0x04) != 0,
            rtr: (byte & 0x08) != 0,
            ic: (byte & 0x40) != 0,
            dv: (byte & 0x80) != 0,
        }
    }

    /// Convert to byte
    pub fn to_byte(&self) -> u8 {
        let mut byte = 0x01; // EA bit always set
        if self.fc {
            byte |= 0x02;
        }
        if self.rtc {
            byte |= 0x04;
        }
        if self.rtr {
            byte |= 0x08;
        }
        if self.ic {
            byte |= 0x40;
        }
        if self.dv {
            byte |= 0x80;
        }
        byte
    }

    /// Default ready state
    pub fn ready() -> Self {
        Self {
            fc: false,
            rtc: true,
            rtr: true,
            ic: false,
            dv: true,
        }
    }
}

// =============================================================================
// RFCOMM FRAME
// =============================================================================

/// RFCOMM frame
#[derive(Clone, Debug)]
pub struct RfcommFrame {
    /// Address field (DLCI + EA + C/R)
    pub address: u8,
    /// Control field (frame type + P/F)
    pub control: u8,
    /// Length
    pub length: u16,
    /// Credits (for credit-based flow control)
    pub credits: Option<u8>,
    /// Information field
    pub data: Vec<u8>,
    /// Frame Check Sequence
    pub fcs: u8,
}

impl RfcommFrame {
    /// Get DLCI from address
    pub fn dlci(&self) -> u8 {
        self.address >> 2
    }

    /// Get command/response bit
    pub fn cr(&self) -> bool {
        (self.address & 0x02) != 0
    }

    /// Get P/F bit
    pub fn pf(&self) -> bool {
        (self.control & 0x10) != 0
    }

    /// Get frame type
    pub fn frame_type(&self) -> Option<RfcommFrameType> {
        RfcommFrameType::from_byte(self.control)
    }

    /// Create SABM frame
    pub fn sabm(dlci: u8, initiator: bool) -> Self {
        let cr = if initiator { 1 } else { 0 };
        let address = (dlci << 2) | (cr << 1) | 0x01;
        let control = RfcommFrameType::Sabm as u8 | 0x10; // P bit set

        Self {
            address,
            control,
            length: 0,
            credits: None,
            data: Vec::new(),
            fcs: Self::calc_fcs(&[address, control]),
        }
    }

    /// Create UA frame
    pub fn ua(dlci: u8, initiator: bool) -> Self {
        let cr = if initiator { 0 } else { 1 };
        let address = (dlci << 2) | (cr << 1) | 0x01;
        let control = RfcommFrameType::Ua as u8 | 0x10; // F bit set

        Self {
            address,
            control,
            length: 0,
            credits: None,
            data: Vec::new(),
            fcs: Self::calc_fcs(&[address, control]),
        }
    }

    /// Create DM frame
    pub fn dm(dlci: u8, initiator: bool) -> Self {
        let cr = if initiator { 0 } else { 1 };
        let address = (dlci << 2) | (cr << 1) | 0x01;
        let control = RfcommFrameType::Dm as u8 | 0x10; // F bit set

        Self {
            address,
            control,
            length: 0,
            credits: None,
            data: Vec::new(),
            fcs: Self::calc_fcs(&[address, control]),
        }
    }

    /// Create DISC frame
    pub fn disc(dlci: u8, initiator: bool) -> Self {
        let cr = if initiator { 1 } else { 0 };
        let address = (dlci << 2) | (cr << 1) | 0x01;
        let control = RfcommFrameType::Disc as u8 | 0x10; // P bit set

        Self {
            address,
            control,
            length: 0,
            credits: None,
            data: Vec::new(),
            fcs: Self::calc_fcs(&[address, control]),
        }
    }

    /// Create UIH frame
    pub fn uih(dlci: u8, initiator: bool, data: Vec<u8>, credits: Option<u8>) -> Self {
        let cr = if initiator { 1 } else { 0 };
        let address = (dlci << 2) | (cr << 1) | 0x01;
        let control = if credits.is_some() {
            RfcommFrameType::UihCredits as u8
        } else {
            RfcommFrameType::Uih as u8
        };

        let length = data.len() as u16;

        Self {
            address,
            control,
            length,
            credits,
            data,
            fcs: Self::calc_fcs(&[address, control]),
        }
    }

    /// Calculate FCS
    fn calc_fcs(data: &[u8]) -> u8 {
        let mut fcs: u8 = 0xFF;
        for &byte in data {
            fcs = FCS_TABLE[(fcs ^ byte) as usize];
        }
        0xFF - fcs
    }

    /// Verify FCS
    pub fn verify_fcs(&self) -> bool {
        let expected = Self::calc_fcs(&[self.address, self.control]);
        expected == self.fcs
    }

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 4 {
            return None;
        }

        let address = data[0];
        let control = data[1];

        // Parse length (1 or 2 bytes)
        let (length, len_size) = if (data[2] & 0x01) != 0 {
            ((data[2] >> 1) as u16, 1)
        } else {
            if data.len() < 5 {
                return None;
            }
            (((data[3] as u16) << 7) | ((data[2] >> 1) as u16), 2)
        };

        let header_size = 2 + len_size;

        // Check for credits
        let (credits, credit_size) = if (control & 0xEF) == RfcommFrameType::UihCredits as u8 {
            if data.len() < header_size + 1 {
                return None;
            }
            (Some(data[header_size]), 1)
        } else {
            (None, 0)
        };

        let data_start = header_size + credit_size;
        let data_end = data_start + length as usize;

        if data.len() < data_end + 1 {
            return None;
        }

        let frame_data = data[data_start..data_end].to_vec();
        let fcs = data[data_end];

        Some(Self {
            address,
            control,
            length,
            credits,
            data: frame_data,
            fcs,
        })
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        bytes.push(self.address);
        bytes.push(self.control);

        // Length field
        if self.length < 128 {
            bytes.push(((self.length as u8) << 1) | 0x01);
        } else {
            bytes.push((self.length as u8) << 1);
            bytes.push((self.length >> 7) as u8);
        }

        // Credits
        if let Some(credits) = self.credits {
            bytes.push(credits);
        }

        // Data
        bytes.extend_from_slice(&self.data);

        // FCS
        bytes.push(self.fcs);

        bytes
    }
}

// FCS lookup table
static FCS_TABLE: [u8; 256] = [
    0x00, 0x91, 0xE3, 0x72, 0x07, 0x96, 0xE4, 0x75, 0x0E, 0x9F, 0xED, 0x7C, 0x09, 0x98, 0xEA, 0x7B,
    0x1C, 0x8D, 0xFF, 0x6E, 0x1B, 0x8A, 0xF8, 0x69, 0x12, 0x83, 0xF1, 0x60, 0x15, 0x84, 0xF6, 0x67,
    0x38, 0xA9, 0xDB, 0x4A, 0x3F, 0xAE, 0xDC, 0x4D, 0x36, 0xA7, 0xD5, 0x44, 0x31, 0xA0, 0xD2, 0x43,
    0x24, 0xB5, 0xC7, 0x56, 0x23, 0xB2, 0xC0, 0x51, 0x2A, 0xBB, 0xC9, 0x58, 0x2D, 0xBC, 0xCE, 0x5F,
    0x70, 0xE1, 0x93, 0x02, 0x77, 0xE6, 0x94, 0x05, 0x7E, 0xEF, 0x9D, 0x0C, 0x79, 0xE8, 0x9A, 0x0B,
    0x6C, 0xFD, 0x8F, 0x1E, 0x6B, 0xFA, 0x88, 0x19, 0x62, 0xF3, 0x81, 0x10, 0x65, 0xF4, 0x86, 0x17,
    0x48, 0xD9, 0xAB, 0x3A, 0x4F, 0xDE, 0xAC, 0x3D, 0x46, 0xD7, 0xA5, 0x34, 0x41, 0xD0, 0xA2, 0x33,
    0x54, 0xC5, 0xB7, 0x26, 0x53, 0xC2, 0xB0, 0x21, 0x5A, 0xCB, 0xB9, 0x28, 0x5D, 0xCC, 0xBE, 0x2F,
    0xE0, 0x71, 0x03, 0x92, 0xE7, 0x76, 0x04, 0x95, 0xEE, 0x7F, 0x0D, 0x9C, 0xE9, 0x78, 0x0A, 0x9B,
    0xFC, 0x6D, 0x1F, 0x8E, 0xFB, 0x6A, 0x18, 0x89, 0xF2, 0x63, 0x11, 0x80, 0xF5, 0x64, 0x16, 0x87,
    0xD8, 0x49, 0x3B, 0xAA, 0xDF, 0x4E, 0x3C, 0xAD, 0xD6, 0x47, 0x35, 0xA4, 0xD1, 0x40, 0x32, 0xA3,
    0xC4, 0x55, 0x27, 0xB6, 0xC3, 0x52, 0x20, 0xB1, 0xCA, 0x5B, 0x29, 0xB8, 0xCD, 0x5C, 0x2E, 0xBF,
    0x90, 0x01, 0x73, 0xE2, 0x97, 0x06, 0x74, 0xE5, 0x9E, 0x0F, 0x7D, 0xEC, 0x99, 0x08, 0x7A, 0xEB,
    0x8C, 0x1D, 0x6F, 0xFE, 0x8B, 0x1A, 0x68, 0xF9, 0x82, 0x13, 0x61, 0xF0, 0x85, 0x14, 0x66, 0xF7,
    0xA8, 0x39, 0x4B, 0xDA, 0xAF, 0x3E, 0x4C, 0xDD, 0xA6, 0x37, 0x45, 0xD4, 0xA1, 0x30, 0x42, 0xD3,
    0xB4, 0x25, 0x57, 0xC6, 0xB3, 0x22, 0x50, 0xC1, 0xBA, 0x2B, 0x59, 0xC8, 0xBD, 0x2C, 0x5E, 0xCF,
];

// =============================================================================
// RFCOMM CHANNEL
// =============================================================================

/// RFCOMM channel state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelState {
    /// Closed
    Closed,
    /// Wait for SABM acknowledgement
    WaitSabm,
    /// Wait for UA
    WaitUa,
    /// Open
    Open,
    /// Wait for DISC acknowledgement
    WaitDisc,
}

/// RFCOMM channel (DLC)
pub struct RfcommChannel {
    /// DLCI (Data Link Connection Identifier)
    pub dlci: u8,
    /// Server channel (1-30)
    pub channel: u8,
    /// Are we initiator
    pub initiator: bool,
    /// State
    state: AtomicU8,
    /// MTU
    pub mtu: u16,
    /// TX credits
    tx_credits: AtomicU8,
    /// RX credits
    rx_credits: AtomicU8,
    /// Use credit-based flow control
    pub credit_flow: bool,
    /// Local modem signals
    pub local_signals: RwLock<ModemSignals>,
    /// Remote modem signals
    pub remote_signals: RwLock<ModemSignals>,
    /// TX queue
    tx_queue: Mutex<VecDeque<Vec<u8>>>,
    /// RX queue
    rx_queue: Mutex<VecDeque<Vec<u8>>>,
    /// Receive callback
    rx_callback: RwLock<Option<Box<dyn Fn(&[u8]) + Send + Sync>>>,
    /// State change callback
    state_callback: RwLock<Option<Box<dyn Fn(ChannelState) + Send + Sync>>>,
}

impl RfcommChannel {
    /// Create new channel
    pub fn new(dlci: u8, initiator: bool) -> Self {
        let channel = dlci >> 1;
        Self {
            dlci,
            channel,
            initiator,
            state: AtomicU8::new(ChannelState::Closed as u8),
            mtu: RFCOMM_DEFAULT_MTU,
            tx_credits: AtomicU8::new(0),
            rx_credits: AtomicU8::new(7), // Initial credits
            credit_flow: true,
            local_signals: RwLock::new(ModemSignals::ready()),
            remote_signals: RwLock::new(ModemSignals::default()),
            tx_queue: Mutex::new(VecDeque::new()),
            rx_queue: Mutex::new(VecDeque::new()),
            rx_callback: RwLock::new(None),
            state_callback: RwLock::new(None),
        }
    }

    /// Get state
    pub fn state(&self) -> ChannelState {
        match self.state.load(Ordering::Acquire) {
            0 => ChannelState::Closed,
            1 => ChannelState::WaitSabm,
            2 => ChannelState::WaitUa,
            3 => ChannelState::Open,
            _ => ChannelState::WaitDisc,
        }
    }

    /// Set state
    pub fn set_state(&self, state: ChannelState) {
        self.state.store(state as u8, Ordering::Release);
        if let Some(callback) = self.state_callback.read().as_ref() {
            callback(state);
        }
    }

    /// Is channel open
    pub fn is_open(&self) -> bool {
        self.state() == ChannelState::Open
    }

    /// Add TX credits
    pub fn add_tx_credits(&self, credits: u8) {
        self.tx_credits.fetch_add(credits, Ordering::SeqCst);
    }

    /// Consume TX credit
    pub fn consume_tx_credit(&self) -> bool {
        loop {
            let current = self.tx_credits.load(Ordering::Acquire);
            if current == 0 {
                return false;
            }
            if self
                .tx_credits
                .compare_exchange(current, current - 1, Ordering::SeqCst, Ordering::Relaxed)
                .is_ok()
            {
                return true;
            }
        }
    }

    /// Get TX credits
    pub fn tx_credits(&self) -> u8 {
        self.tx_credits.load(Ordering::Acquire)
    }

    /// Grant RX credits
    pub fn grant_rx_credits(&self, credits: u8) -> u8 {
        self.rx_credits.fetch_add(credits, Ordering::SeqCst);
        credits
    }

    /// Consume RX credit
    pub fn consume_rx_credit(&self) {
        self.rx_credits.fetch_sub(1, Ordering::SeqCst);
    }

    /// Get RX credits
    pub fn rx_credits(&self) -> u8 {
        self.rx_credits.load(Ordering::Acquire)
    }

    /// Set receive callback
    pub fn set_rx_callback<F>(&self, callback: F)
    where
        F: Fn(&[u8]) + Send + Sync + 'static,
    {
        *self.rx_callback.write() = Some(Box::new(callback));
    }

    /// Set state callback
    pub fn set_state_callback<F>(&self, callback: F)
    where
        F: Fn(ChannelState) + Send + Sync + 'static,
    {
        *self.state_callback.write() = Some(Box::new(callback));
    }

    /// Queue data for sending
    pub fn send(&self, data: &[u8]) -> Result<(), BluetoothError> {
        if self.state() != ChannelState::Open {
            return Err(BluetoothError::NotConnected);
        }

        // Split into MTU-sized chunks
        for chunk in data.chunks(self.mtu as usize) {
            self.tx_queue.lock().push_back(chunk.to_vec());
        }

        Ok(())
    }

    /// Receive data
    pub fn receive(&self, data: &[u8]) {
        if self.credit_flow {
            self.consume_rx_credit();
        }

        if let Some(callback) = self.rx_callback.read().as_ref() {
            callback(data);
        } else {
            self.rx_queue.lock().push_back(data.to_vec());
        }
    }

    /// Get pending TX data
    pub fn pending_tx(&self) -> Option<Vec<u8>> {
        self.tx_queue.lock().pop_front()
    }

    /// Get pending RX data (if no callback set)
    pub fn pending_rx(&self) -> Option<Vec<u8>> {
        self.rx_queue.lock().pop_front()
    }

    /// Read data (blocking-style)
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, BluetoothError> {
        if let Some(data) = self.pending_rx() {
            let len = data.len().min(buf.len());
            buf[..len].copy_from_slice(&data[..len]);
            Ok(len)
        } else {
            Err(BluetoothError::WouldBlock)
        }
    }

    /// Write data
    pub fn write(&self, data: &[u8]) -> Result<usize, BluetoothError> {
        self.send(data)?;
        Ok(data.len())
    }
}

// =============================================================================
// RFCOMM SESSION
// =============================================================================

/// RFCOMM session (multiplexer)
pub struct RfcommSession {
    /// L2CAP channel
    l2cap_channel: Arc<RwLock<L2capChannel>>,
    /// Are we initiator
    pub initiator: bool,
    /// Is multiplexer open
    mux_open: AtomicBool,
    /// Data Link Connections
    channels: RwLock<BTreeMap<u8, Arc<RwLock<RfcommChannel>>>>,
    /// Pending parameter negotiation
    pending_pn: Mutex<Option<(u8, u16)>>,
    /// Session MTU
    pub mtu: u16,
    /// Credit-based flow control
    pub credit_flow: bool,
}

impl RfcommSession {
    /// Create new session
    pub fn new(l2cap_channel: Arc<RwLock<L2capChannel>>, initiator: bool) -> Self {
        Self {
            l2cap_channel,
            initiator,
            mux_open: AtomicBool::new(false),
            channels: RwLock::new(BTreeMap::new()),
            pending_pn: Mutex::new(None),
            mtu: RFCOMM_DEFAULT_MTU,
            credit_flow: true,
        }
    }

    /// Is multiplexer open
    pub fn is_mux_open(&self) -> bool {
        self.mux_open.load(Ordering::Acquire)
    }

    /// Open multiplexer
    pub fn open_mux(&self) -> Result<(), BluetoothError> {
        // Send SABM on DLCI 0
        let frame = RfcommFrame::sabm(0, self.initiator);
        self.send_frame(&frame)
    }

    /// Close multiplexer
    pub fn close_mux(&self) -> Result<(), BluetoothError> {
        // Send DISC on DLCI 0
        let frame = RfcommFrame::disc(0, self.initiator);
        self.send_frame(&frame)?;

        self.mux_open.store(false, Ordering::Release);

        // Close all channels
        for channel in self.channels.read().values() {
            channel.write().set_state(ChannelState::Closed);
        }

        Ok(())
    }

    /// Get or create channel
    fn get_or_create_channel(&self, dlci: u8) -> Arc<RwLock<RfcommChannel>> {
        let mut channels = self.channels.write();
        if let Some(channel) = channels.get(&dlci) {
            return channel.clone();
        }

        let channel = Arc::new(RwLock::new(RfcommChannel::new(dlci, self.initiator)));
        channels.insert(dlci, channel.clone());
        channel
    }

    /// Open channel to server channel
    pub fn open_channel(&self, server_channel: u8) -> Result<Arc<RwLock<RfcommChannel>>, BluetoothError> {
        if !self.is_mux_open() {
            return Err(BluetoothError::NotConnected);
        }

        // Calculate DLCI: direction bit + server channel
        let dlci = if self.initiator {
            (server_channel << 1) | 0x00
        } else {
            (server_channel << 1) | 0x01
        };

        let channel = self.get_or_create_channel(dlci);

        // Send PN (Parameter Negotiation) first
        self.send_pn(dlci)?;
        *self.pending_pn.lock() = Some((dlci, RFCOMM_DEFAULT_MTU));

        // Channel will be opened after PN response
        channel.write().set_state(ChannelState::WaitSabm);

        Ok(channel)
    }

    /// Close channel
    pub fn close_channel(&self, dlci: u8) -> Result<(), BluetoothError> {
        if let Some(channel) = self.channels.read().get(&dlci) {
            channel.write().set_state(ChannelState::WaitDisc);
            let frame = RfcommFrame::disc(dlci, self.initiator);
            self.send_frame(&frame)?;
        }
        Ok(())
    }

    /// Send frame
    fn send_frame(&self, frame: &RfcommFrame) -> Result<(), BluetoothError> {
        let data = frame.to_bytes();
        self.l2cap_channel.read().send(data)
    }

    /// Send PN (Parameter Negotiation)
    fn send_pn(&self, dlci: u8) -> Result<(), BluetoothError> {
        let mut pn_data = Vec::with_capacity(8);
        pn_data.push(dlci); // D1-D6: DLCI
        pn_data.push(0xE0); // I1-I4: 0xE = Credit-based flow control, CL: 0
        pn_data.push(0x00); // P1-P6: Priority
        pn_data.push(0x00); // T1-T8: Timer (unused)
        pn_data.extend_from_slice(&self.mtu.to_le_bytes()); // N1: Max frame size
        pn_data.push(0x00); // NA1: Max retransmissions (unused)
        pn_data.push(7); // K: Initial credits

        self.send_mcc(MccType::Pn, true, &pn_data)
    }

    /// Send MSC (Modem Status Command)
    fn send_msc(&self, dlci: u8, signals: &ModemSignals) -> Result<(), BluetoothError> {
        let mut msc_data = Vec::with_capacity(2);
        msc_data.push((dlci << 2) | 0x03); // DLCI + EA + CR
        msc_data.push(signals.to_byte());

        self.send_mcc(MccType::Msc, true, &msc_data)
    }

    /// Send MCC (Multiplexer Control Command)
    fn send_mcc(&self, mcc_type: MccType, command: bool, data: &[u8]) -> Result<(), BluetoothError> {
        let mut mcc_data = Vec::with_capacity(2 + data.len());

        // Type field
        let type_byte = (mcc_type as u8) | (if command { 0x02 } else { 0x00 }) | 0x01;
        mcc_data.push(type_byte);

        // Length field
        mcc_data.push(((data.len() as u8) << 1) | 0x01);

        // Value
        mcc_data.extend_from_slice(data);

        // Send as UIH on DLCI 0
        let frame = RfcommFrame::uih(0, self.initiator, mcc_data, None);
        self.send_frame(&frame)
    }

    /// Handle incoming L2CAP data
    pub fn handle_data(&self, data: &[u8]) {
        if let Some(frame) = RfcommFrame::from_bytes(data) {
            self.handle_frame(&frame);
        }
    }

    /// Handle frame
    fn handle_frame(&self, frame: &RfcommFrame) {
        let _dlci = frame.dlci();

        match frame.frame_type() {
            Some(RfcommFrameType::Sabm) => self.handle_sabm(frame),
            Some(RfcommFrameType::Ua) => self.handle_ua(frame),
            Some(RfcommFrameType::Dm) => self.handle_dm(frame),
            Some(RfcommFrameType::Disc) => self.handle_disc(frame),
            Some(RfcommFrameType::Uih) | Some(RfcommFrameType::UihCredits) => {
                self.handle_uih(frame)
            }
            None => {}
        }
    }

    fn handle_sabm(&self, frame: &RfcommFrame) {
        let dlci = frame.dlci();

        if dlci == 0 {
            // Multiplexer control channel
            self.mux_open.store(true, Ordering::Release);

            // Send UA
            let ua = RfcommFrame::ua(0, self.initiator);
            let _ = self.send_frame(&ua);
        } else {
            // Data channel - accept connection
            let channel = self.get_or_create_channel(dlci);
            channel.write().set_state(ChannelState::Open);

            // Send UA
            let ua = RfcommFrame::ua(dlci, self.initiator);
            let _ = self.send_frame(&ua);

            // Send MSC
            let _ = self.send_msc(dlci, &ModemSignals::ready());
        }
    }

    fn handle_ua(&self, frame: &RfcommFrame) {
        let dlci = frame.dlci();

        if dlci == 0 {
            // Multiplexer opened
            self.mux_open.store(true, Ordering::Release);
        } else {
            // Channel opened
            if let Some(channel) = self.channels.read().get(&dlci) {
                let ch = channel.read();
                if ch.state() == ChannelState::WaitUa {
                    drop(ch);
                    channel.write().set_state(ChannelState::Open);

                    // Send MSC
                    let _ = self.send_msc(dlci, &ModemSignals::ready());
                }
            }
        }
    }

    fn handle_dm(&self, frame: &RfcommFrame) {
        let dlci = frame.dlci();

        if dlci == 0 {
            // Multiplexer rejected
            self.mux_open.store(false, Ordering::Release);
        } else {
            // Channel rejected
            if let Some(channel) = self.channels.read().get(&dlci) {
                channel.write().set_state(ChannelState::Closed);
            }
        }
    }

    fn handle_disc(&self, frame: &RfcommFrame) {
        let dlci = frame.dlci();

        // Send UA
        let ua = RfcommFrame::ua(dlci, self.initiator);
        let _ = self.send_frame(&ua);

        if dlci == 0 {
            // Multiplexer closed
            self.mux_open.store(false, Ordering::Release);
            for channel in self.channels.read().values() {
                channel.write().set_state(ChannelState::Closed);
            }
        } else {
            // Channel closed
            if let Some(channel) = self.channels.read().get(&dlci) {
                channel.write().set_state(ChannelState::Closed);
            }
        }
    }

    fn handle_uih(&self, frame: &RfcommFrame) {
        let dlci = frame.dlci();

        // Handle credits
        if let Some(credits) = frame.credits {
            if let Some(channel) = self.channels.read().get(&dlci) {
                channel.read().add_tx_credits(credits);
            }
        }

        if dlci == 0 {
            // Control channel - MCC
            self.handle_mcc(&frame.data);
        } else {
            // Data channel
            if let Some(channel) = self.channels.read().get(&dlci) {
                channel.read().receive(&frame.data);
            }
        }
    }

    fn handle_mcc(&self, data: &[u8]) {
        if data.len() < 2 {
            return;
        }

        let type_byte = data[0];
        let is_command = (type_byte & 0x02) != 0;
        let mcc_type = MccType::from_byte(type_byte);

        let len = (data[1] >> 1) as usize;
        let value = if data.len() >= 2 + len {
            &data[2..2 + len]
        } else {
            return;
        };

        match mcc_type {
            Some(MccType::Pn) => self.handle_pn(is_command, value),
            Some(MccType::Msc) => self.handle_msc(is_command, value),
            Some(MccType::Rpn) => self.handle_rpn(is_command, value),
            Some(MccType::Rls) => self.handle_rls(is_command, value),
            Some(MccType::Test) => self.handle_test(is_command, value),
            Some(MccType::FcOn) => self.handle_fc_on(is_command),
            Some(MccType::FcOff) => self.handle_fc_off(is_command),
            _ => {
                if is_command {
                    // Send NSC response
                    let _ = self.send_mcc(MccType::Nsc, false, &[type_byte]);
                }
            }
        }
    }

    fn handle_pn(&self, is_command: bool, data: &[u8]) {
        if data.len() < 8 {
            return;
        }

        let dlci = data[0] & 0x3F;
        let _cl = data[1] & 0x0F;
        let mtu = u16::from_le_bytes([data[4], data[5]]);
        let credits = data[7];

        if is_command {
            // Send PN response
            let response = data.to_vec();
            // Adjust parameters as needed
            let _ = self.send_mcc(MccType::Pn, false, &response);
        } else {
            // PN response received - now send SABM
            if let Some((pending_dlci, _)) = *self.pending_pn.lock() {
                if pending_dlci == dlci {
                    let channel = self.get_or_create_channel(dlci);
                    {
                        let mut ch = channel.write();
                        ch.mtu = mtu;
                        ch.add_tx_credits(credits);
                        ch.set_state(ChannelState::WaitUa);
                    }

                    let sabm = RfcommFrame::sabm(dlci, self.initiator);
                    let _ = self.send_frame(&sabm);
                }
            }
        }
    }

    fn handle_msc(&self, is_command: bool, data: &[u8]) {
        if data.len() < 2 {
            return;
        }

        let dlci = (data[0] >> 2) & 0x3F;
        let signals = ModemSignals::from_byte(data[1]);

        if let Some(channel) = self.channels.read().get(&dlci) {
            *channel.read().remote_signals.write() = signals;
        }

        if is_command {
            // Send MSC response
            let _ = self.send_mcc(MccType::Msc, false, data);
        }
    }

    fn handle_rpn(&self, is_command: bool, data: &[u8]) {
        // Remote Port Negotiation - baud rate, data bits, etc.
        if is_command {
            let _ = self.send_mcc(MccType::Rpn, false, data);
        }
    }

    fn handle_rls(&self, is_command: bool, data: &[u8]) {
        // Remote Line Status
        if is_command {
            let _ = self.send_mcc(MccType::Rls, false, data);
        }
    }

    fn handle_test(&self, is_command: bool, data: &[u8]) {
        if is_command {
            // Echo back the test data
            let _ = self.send_mcc(MccType::Test, false, data);
        }
    }

    fn handle_fc_on(&self, is_command: bool) {
        // Flow control on all channels enabled
        if is_command {
            let _ = self.send_mcc(MccType::FcOn, false, &[]);
        }
    }

    fn handle_fc_off(&self, is_command: bool) {
        // Flow control on all channels disabled
        if is_command {
            let _ = self.send_mcc(MccType::FcOff, false, &[]);
        }
    }

    /// Send data on channel
    pub fn send_data(&self, dlci: u8, data: &[u8]) -> Result<(), BluetoothError> {
        let channel = self
            .channels
            .read()
            .get(&dlci)
            .cloned()
            .ok_or(BluetoothError::NotFound)?;

        let ch = channel.read();
        if ch.state() != ChannelState::Open {
            return Err(BluetoothError::NotConnected);
        }

        // Check credits
        let credits = if ch.credit_flow && ch.tx_credits() < 1 {
            return Err(BluetoothError::WouldBlock);
        } else if ch.credit_flow {
            ch.consume_tx_credit();
            // Grant more RX credits if needed
            let grant = if ch.rx_credits() < 3 {
                Some(ch.grant_rx_credits(7))
            } else {
                None
            };
            grant
        } else {
            None
        };

        drop(ch);

        // Split into MTU-sized chunks
        let mtu = channel.read().mtu as usize;
        for chunk in data.chunks(mtu) {
            let frame = RfcommFrame::uih(dlci, self.initiator, chunk.to_vec(), credits);
            self.send_frame(&frame)?;
        }

        Ok(())
    }

    /// Get channel
    pub fn get_channel(&self, dlci: u8) -> Option<Arc<RwLock<RfcommChannel>>> {
        self.channels.read().get(&dlci).cloned()
    }
}

// =============================================================================
// RFCOMM SERVER
// =============================================================================

/// RFCOMM server for listening on channels
pub struct RfcommServer {
    /// L2CAP manager reference
    l2cap: Arc<RwLock<L2capManager>>,
    /// Registered channels
    channels: RwLock<BTreeMap<u8, Box<dyn Fn(Arc<RwLock<RfcommChannel>>) + Send + Sync>>>,
    /// Active sessions
    sessions: RwLock<BTreeMap<u16, Arc<RwLock<RfcommSession>>>>,
}

impl RfcommServer {
    /// Create new RFCOMM server
    pub fn new(l2cap: Arc<RwLock<L2capManager>>) -> Arc<Self> {
        let server = Arc::new(Self {
            l2cap: l2cap.clone(),
            channels: RwLock::new(BTreeMap::new()),
            sessions: RwLock::new(BTreeMap::new()),
        });

        // Register L2CAP PSM handler
        let server_clone = server.clone();
        l2cap.read().register_psm(L2CAP_PSM_RFCOMM, move |l2cap_channel| {
            server_clone.handle_connection(l2cap_channel);
        });

        server
    }

    /// Register channel handler
    pub fn register<F>(&self, channel: u8, handler: F)
    where
        F: Fn(Arc<RwLock<RfcommChannel>>) + Send + Sync + 'static,
    {
        if channel >= 1 && channel <= RFCOMM_MAX_CHANNELS {
            self.channels.write().insert(channel, Box::new(handler));
        }
    }

    /// Unregister channel
    pub fn unregister(&self, channel: u8) {
        self.channels.write().remove(&channel);
    }

    /// Handle incoming L2CAP connection
    fn handle_connection(&self, l2cap_channel: Arc<RwLock<L2capChannel>>) {
        let handle = l2cap_channel.read().handle;

        let session = Arc::new(RwLock::new(RfcommSession::new(l2cap_channel.clone(), false)));
        self.sessions.write().insert(handle, session.clone());

        // Set up data handler
        let session_clone = session.clone();
        let server = self.clone_weak();
        l2cap_channel.read().set_rx_callback(move |data| {
            session_clone.read().handle_data(data);
            // Check for new channels
            if let Some(server) = server.upgrade() {
                server.check_new_channels(&session_clone);
            }
        });
    }

    /// Check for new channels to notify handlers
    fn check_new_channels(&self, session: &Arc<RwLock<RfcommSession>>) {
        let session = session.read();
        for (_dlci, channel) in session.channels.read().iter() {
            let ch = channel.read();
            if ch.state() == ChannelState::Open {
                let server_channel = ch.channel;
                drop(ch);

                if let Some(handler) = self.channels.read().get(&server_channel) {
                    handler(channel.clone());
                }
            }
        }
    }

    /// Get weak reference for callbacks
    fn clone_weak(&self) -> alloc::sync::Weak<Self> {
        // In a real implementation, we'd use Arc::downgrade
        // For now, return a placeholder
        alloc::sync::Weak::new()
    }
}

// =============================================================================
// RFCOMM CLIENT
// =============================================================================

/// RFCOMM client for connecting to remote services
pub struct RfcommClient {
    /// L2CAP manager reference
    l2cap: Arc<RwLock<L2capManager>>,
    /// Active sessions
    sessions: RwLock<BTreeMap<u16, Arc<RwLock<RfcommSession>>>>,
}

impl RfcommClient {
    /// Create new RFCOMM client
    pub fn new(l2cap: Arc<RwLock<L2capManager>>) -> Self {
        Self {
            l2cap,
            sessions: RwLock::new(BTreeMap::new()),
        }
    }

    /// Connect to remote RFCOMM channel
    pub fn connect(
        &self,
        handle: u16,
        server_channel: u8,
    ) -> Result<Arc<RwLock<RfcommChannel>>, BluetoothError> {
        // Get or create session
        let session = self.get_or_create_session(handle)?;

        // Open multiplexer if needed
        if !session.read().is_mux_open() {
            session.read().open_mux()?;
            // Wait for UA (simplified - real implementation would be async)
        }

        // Open channel
        let result = session.read().open_channel(server_channel);
        result
    }

    /// Get or create session
    fn get_or_create_session(&self, handle: u16) -> Result<Arc<RwLock<RfcommSession>>, BluetoothError> {
        let mut sessions = self.sessions.write();

        if let Some(session) = sessions.get(&handle) {
            return Ok(session.clone());
        }

        // Create L2CAP connection
        let l2cap_channel = self.l2cap.read().connect(handle, L2CAP_PSM_RFCOMM)?;

        let session = Arc::new(RwLock::new(RfcommSession::new(l2cap_channel.clone(), true)));
        sessions.insert(handle, session.clone());

        // Set up data handler
        let session_clone = session.clone();
        l2cap_channel.read().set_rx_callback(move |data| {
            session_clone.read().handle_data(data);
        });

        Ok(session)
    }

    /// Disconnect session
    pub fn disconnect(&self, handle: u16) -> Result<(), BluetoothError> {
        if let Some(session) = self.sessions.write().remove(&handle) {
            session.read().close_mux()?;
        }
        Ok(())
    }
}
