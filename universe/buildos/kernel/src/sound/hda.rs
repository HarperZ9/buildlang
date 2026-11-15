//! Intel High Definition Audio (HDA) Controller Driver
//!
//! Implements support for Intel HD Audio compatible controllers.

#![allow(dead_code)]

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use super::{
    CardType, SoundError, StreamDirection, SampleFormat,
    sound_core::{SoundCard, SoundDevice, DeviceType, CardCapabilities, SampleRateCapability, FormatCapability, DmaBuffer},
    mixer::{Mixer, MixerElement, MixerControl, ControlType},
};

// =============================================================================
// HDA REGISTER DEFINITIONS
// =============================================================================

/// HDA Global Capabilities Register
pub const REG_GCAP: u16 = 0x00;
/// HDA Version Minor
pub const REG_VMIN: u16 = 0x02;
/// HDA Version Major
pub const REG_VMAJ: u16 = 0x03;
/// Output Payload Capability
pub const REG_OUTPAY: u16 = 0x04;
/// Input Payload Capability
pub const REG_INPAY: u16 = 0x06;
/// Global Control
pub const REG_GCTL: u16 = 0x08;
/// Wake Enable
pub const REG_WAKEEN: u16 = 0x0C;
/// State Change Status
pub const REG_STATESTS: u16 = 0x0E;
/// Global Status
pub const REG_GSTS: u16 = 0x10;
/// Output Stream Payload Capability
pub const REG_OUTSTRMPAY: u16 = 0x18;
/// Input Stream Payload Capability
pub const REG_INSTRMPAY: u16 = 0x1A;
/// Interrupt Control
pub const REG_INTCTL: u16 = 0x20;
/// Interrupt Status
pub const REG_INTSTS: u16 = 0x24;
/// Wall Clock Counter
pub const REG_WALLCLK: u16 = 0x30;
/// Stream Synchronization
pub const REG_SSYNC: u16 = 0x38;
/// CORB Lower Base Address
pub const REG_CORBLBASE: u16 = 0x40;
/// CORB Upper Base Address
pub const REG_CORBUBASE: u16 = 0x44;
/// CORB Write Pointer
pub const REG_CORBWP: u16 = 0x48;
/// CORB Read Pointer
pub const REG_CORBRP: u16 = 0x4A;
/// CORB Control
pub const REG_CORBCTL: u16 = 0x4C;
/// CORB Status
pub const REG_CORBSTS: u16 = 0x4D;
/// CORB Size
pub const REG_CORBSIZE: u16 = 0x4E;
/// RIRB Lower Base Address
pub const REG_RIRBLBASE: u16 = 0x50;
/// RIRB Upper Base Address
pub const REG_RIRBUBASE: u16 = 0x54;
/// RIRB Write Pointer
pub const REG_RIRBWP: u16 = 0x58;
/// Response Interrupt Count
pub const REG_RINTCNT: u16 = 0x5A;
/// RIRB Control
pub const REG_RIRBCTL: u16 = 0x5C;
/// RIRB Status
pub const REG_RIRBSTS: u16 = 0x5D;
/// RIRB Size
pub const REG_RIRBSIZE: u16 = 0x5E;
/// DMA Position Buffer Lower Base
pub const REG_DPLBASE: u16 = 0x70;
/// DMA Position Buffer Upper Base
pub const REG_DPUBASE: u16 = 0x74;

/// Stream descriptor offset
pub const STREAM_DESC_OFFSET: u16 = 0x80;
/// Stream descriptor size
pub const STREAM_DESC_SIZE: u16 = 0x20;

/// Stream Control
pub const REG_SD_CTL: u16 = 0x00;
/// Stream Status
pub const REG_SD_STS: u16 = 0x03;
/// Stream Link Position
pub const REG_SD_LPIB: u16 = 0x04;
/// Stream Cyclic Buffer Length
pub const REG_SD_CBL: u16 = 0x08;
/// Stream Last Valid Index
pub const REG_SD_LVI: u16 = 0x0C;
/// Stream FIFO Size
pub const REG_SD_FIFOS: u16 = 0x10;
/// Stream Format
pub const REG_SD_FMT: u16 = 0x12;
/// Stream BDL Lower Base
pub const REG_SD_BDLPL: u16 = 0x18;
/// Stream BDL Upper Base
pub const REG_SD_BDLPU: u16 = 0x1C;

// Global Control bits
pub const GCTL_RESET: u32 = 1 << 0;
pub const GCTL_FCNTRL: u32 = 1 << 1;
pub const GCTL_UNSOL: u32 = 1 << 8;

// Stream Control bits
pub const SD_CTL_RESET: u32 = 1 << 0;
pub const SD_CTL_RUN: u32 = 1 << 1;
pub const SD_CTL_IOCE: u32 = 1 << 2;
pub const SD_CTL_FEIE: u32 = 1 << 3;
pub const SD_CTL_DEIE: u32 = 1 << 4;

// Stream Status bits
pub const SD_STS_BCIS: u8 = 1 << 2;
pub const SD_STS_FIFOE: u8 = 1 << 3;
pub const SD_STS_DESE: u8 = 1 << 4;
pub const SD_STS_FIFORDY: u8 = 1 << 5;

// =============================================================================
// HDA CODEC VERBS
// =============================================================================

/// Verb: Get Parameter
pub const VERB_GET_PARAM: u32 = 0xF00;
/// Verb: Get Connection Select Control
pub const VERB_GET_CONN_SEL: u32 = 0xF01;
/// Verb: Set Connection Select Control
pub const VERB_SET_CONN_SEL: u32 = 0x701;
/// Verb: Get Connection List Entry
pub const VERB_GET_CONN_LIST: u32 = 0xF02;
/// Verb: Get Processing State
pub const VERB_GET_PROC_STATE: u32 = 0xF03;
/// Verb: Set Processing State
pub const VERB_SET_PROC_STATE: u32 = 0x703;
/// Verb: Get Coefficient Index
pub const VERB_GET_COEF_INDEX: u32 = 0xD;
/// Verb: Set Coefficient Index
pub const VERB_SET_COEF_INDEX: u32 = 0x5;
/// Verb: Get Processing Coefficient
pub const VERB_GET_PROC_COEF: u32 = 0xC;
/// Verb: Set Processing Coefficient
pub const VERB_SET_PROC_COEF: u32 = 0x4;
/// Verb: Get Amplifier Gain/Mute
pub const VERB_GET_AMP_GAIN: u32 = 0xB;
/// Verb: Set Amplifier Gain/Mute
pub const VERB_SET_AMP_GAIN: u32 = 0x3;
/// Verb: Get Converter Format
pub const VERB_GET_CONV_FMT: u32 = 0xA;
/// Verb: Set Converter Format
pub const VERB_SET_CONV_FMT: u32 = 0x2;
/// Verb: Get Digital Converter Control
pub const VERB_GET_DIGI_CONV: u32 = 0xF0D;
/// Verb: Set Digital Converter Control 1
pub const VERB_SET_DIGI_CONV_1: u32 = 0x70D;
/// Verb: Set Digital Converter Control 2
pub const VERB_SET_DIGI_CONV_2: u32 = 0x70E;
/// Verb: Get Power State
pub const VERB_GET_POWER_STATE: u32 = 0xF05;
/// Verb: Set Power State
pub const VERB_SET_POWER_STATE: u32 = 0x705;
/// Verb: Get Converter Channel Count
pub const VERB_GET_CONV_CHANNEL_CNT: u32 = 0xF2D;
/// Verb: Set Converter Channel Count
pub const VERB_SET_CONV_CHANNEL_CNT: u32 = 0x72D;
/// Verb: Get HDMI DIP Size
pub const VERB_GET_HDMI_DIP_SIZE: u32 = 0xF2E;
/// Verb: Get HDMI ELD Select
pub const VERB_GET_HDMI_ELDD: u32 = 0xF2F;
/// Verb: Set HDMI DIP Index
pub const VERB_SET_HDMI_DIP_INDEX: u32 = 0x730;
/// Verb: Set HDMI DIP Data
pub const VERB_SET_HDMI_DIP_DATA: u32 = 0x731;
/// Verb: Set HDMI DIP Transmit Control
pub const VERB_SET_HDMI_DIP_XMIT: u32 = 0x732;
/// Verb: Set HDMI Channel Allocation
pub const VERB_SET_HDMI_CHANALLOC: u32 = 0x734;
/// Verb: Get Volume Knob Control
pub const VERB_GET_VOL_KNOB: u32 = 0xF0F;
/// Verb: Set Volume Knob Control
pub const VERB_SET_VOL_KNOB: u32 = 0x70F;
/// Verb: Get GPIO Data
pub const VERB_GET_GPIO_DATA: u32 = 0xF15;
/// Verb: Set GPIO Data
pub const VERB_SET_GPIO_DATA: u32 = 0x715;
/// Verb: Get GPIO Enable Mask
pub const VERB_GET_GPIO_MASK: u32 = 0xF16;
/// Verb: Set GPIO Enable Mask
pub const VERB_SET_GPIO_MASK: u32 = 0x716;
/// Verb: Get GPIO Direction
pub const VERB_GET_GPIO_DIR: u32 = 0xF17;
/// Verb: Set GPIO Direction
pub const VERB_SET_GPIO_DIR: u32 = 0x717;
/// Verb: Get GPIO Wake Enable Mask
pub const VERB_GET_GPIO_WAKE: u32 = 0xF18;
/// Verb: Set GPIO Wake Enable Mask
pub const VERB_SET_GPIO_WAKE: u32 = 0x718;
/// Verb: Get GPIO Unsolicited Response Enable Mask
pub const VERB_GET_GPIO_UNSOL: u32 = 0xF19;
/// Verb: Set GPIO Unsolicited Response Enable Mask
pub const VERB_SET_GPIO_UNSOL: u32 = 0x719;
/// Verb: Get GPIO Sticky Mask
pub const VERB_GET_GPIO_STICKY: u32 = 0xF1A;
/// Verb: Set GPIO Sticky Mask
pub const VERB_SET_GPIO_STICKY: u32 = 0x71A;
/// Verb: Get Beep Generation
pub const VERB_GET_BEEP: u32 = 0xF0A;
/// Verb: Set Beep Generation
pub const VERB_SET_BEEP: u32 = 0x70A;
/// Verb: Get EAPD/BTL Enable
pub const VERB_GET_EAPD_BTL: u32 = 0xF0C;
/// Verb: Set EAPD/BTL Enable
pub const VERB_SET_EAPD_BTL: u32 = 0x70C;
/// Verb: Get Configuration Default
pub const VERB_GET_CONFIG_DEFAULT: u32 = 0xF1C;
/// Verb: Set Configuration Default 0
pub const VERB_SET_CONFIG_D0: u32 = 0x71C;
/// Verb: Set Configuration Default 1
pub const VERB_SET_CONFIG_D1: u32 = 0x71D;
/// Verb: Set Configuration Default 2
pub const VERB_SET_CONFIG_D2: u32 = 0x71E;
/// Verb: Set Configuration Default 3
pub const VERB_SET_CONFIG_D3: u32 = 0x71F;
/// Verb: Get Pin Widget Control
pub const VERB_GET_PIN_CTL: u32 = 0xF07;
/// Verb: Set Pin Widget Control
pub const VERB_SET_PIN_CTL: u32 = 0x707;
/// Verb: Get Unsolicited Response
pub const VERB_GET_UNSOL_RESP: u32 = 0xF08;
/// Verb: Set Unsolicited Response
pub const VERB_SET_UNSOL_RESP: u32 = 0x708;
/// Verb: Get Pin Sense
pub const VERB_GET_PIN_SENSE: u32 = 0xF09;
/// Verb: Execute Pin Sense
pub const VERB_EXEC_PIN_SENSE: u32 = 0x709;
/// Verb: Get Stream/Channel
pub const VERB_GET_STREAM_CHANNEL: u32 = 0xF06;
/// Verb: Set Stream/Channel
pub const VERB_SET_STREAM_CHANNEL: u32 = 0x706;

// =============================================================================
// HDA PARAMETER IDS
// =============================================================================

/// Parameter: Vendor ID
pub const PARAM_VENDOR_ID: u8 = 0x00;
/// Parameter: Revision ID
pub const PARAM_REVISION_ID: u8 = 0x02;
/// Parameter: Subordinate Node Count
pub const PARAM_SUB_NODE_COUNT: u8 = 0x04;
/// Parameter: Function Group Type
pub const PARAM_FN_GROUP_TYPE: u8 = 0x05;
/// Parameter: Audio Function Group Capabilities
pub const PARAM_AFG_CAP: u8 = 0x08;
/// Parameter: Audio Widget Capabilities
pub const PARAM_AW_CAP: u8 = 0x09;
/// Parameter: Supported PCM Size/Rates
pub const PARAM_PCM_CAP: u8 = 0x0A;
/// Parameter: Supported Stream Formats
pub const PARAM_STREAM_FMT: u8 = 0x0B;
/// Parameter: Pin Capabilities
pub const PARAM_PIN_CAP: u8 = 0x0C;
/// Parameter: Input Amplifier Capabilities
pub const PARAM_IN_AMP_CAP: u8 = 0x0D;
/// Parameter: Output Amplifier Capabilities
pub const PARAM_OUT_AMP_CAP: u8 = 0x12;
/// Parameter: Connection List Length
pub const PARAM_CONN_LIST_LEN: u8 = 0x0E;
/// Parameter: Supported Power States
pub const PARAM_POWER_STATES: u8 = 0x0F;
/// Parameter: Processing Capabilities
pub const PARAM_PROC_CAP: u8 = 0x10;
/// Parameter: GPIO Count
pub const PARAM_GPIO_CNT: u8 = 0x11;
/// Parameter: Volume Knob Capabilities
pub const PARAM_VOL_KNOB_CAP: u8 = 0x13;

// =============================================================================
// HDA WIDGET TYPES
// =============================================================================

/// Widget: Audio Output
pub const WIDGET_AUDIO_OUT: u8 = 0x0;
/// Widget: Audio Input
pub const WIDGET_AUDIO_IN: u8 = 0x1;
/// Widget: Audio Mixer
pub const WIDGET_AUDIO_MIX: u8 = 0x2;
/// Widget: Audio Selector
pub const WIDGET_AUDIO_SEL: u8 = 0x3;
/// Widget: Pin Complex
pub const WIDGET_PIN: u8 = 0x4;
/// Widget: Power Widget
pub const WIDGET_POWER: u8 = 0x5;
/// Widget: Volume Knob
pub const WIDGET_VOL_KNOB: u8 = 0x6;
/// Widget: Beep Generator
pub const WIDGET_BEEP: u8 = 0x7;
/// Widget: Vendor Defined
pub const WIDGET_VENDOR: u8 = 0xF;

// =============================================================================
// HDA STRUCTURES
// =============================================================================

/// Buffer Descriptor List Entry
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct BdlEntry {
    /// Lower 32 bits of buffer address
    pub addr_low: u32,
    /// Upper 32 bits of buffer address
    pub addr_high: u32,
    /// Length in bytes
    pub length: u32,
    /// Interrupt on completion flag
    pub ioc: u32,
}

/// CORB/RIRB Entry
#[repr(C, packed)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CorbEntry {
    /// Verb
    pub verb: u32,
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug, Default)]
pub struct RirbEntry {
    /// Response
    pub response: u32,
    /// Response extended
    pub response_ex: u32,
}

/// HDA Codec info
#[derive(Clone, Debug)]
pub struct HdaCodec {
    /// Codec address (0-14)
    pub address: u8,
    /// Vendor ID
    pub vendor_id: u16,
    /// Device ID
    pub device_id: u16,
    /// Revision ID
    pub revision_id: u32,
    /// Function groups
    pub function_groups: Vec<HdaFunctionGroup>,
    /// Widgets
    pub widgets: Vec<HdaWidget>,
}

impl HdaCodec {
    pub fn new(address: u8) -> Self {
        Self {
            address,
            vendor_id: 0,
            device_id: 0,
            revision_id: 0,
            function_groups: Vec::new(),
            widgets: Vec::new(),
        }
    }
}

/// HDA Function Group
#[derive(Clone, Debug)]
pub struct HdaFunctionGroup {
    /// Node ID
    pub nid: u8,
    /// Function group type
    pub fg_type: FunctionGroupType,
    /// Start node
    pub start_nid: u8,
    /// Number of nodes
    pub num_nodes: u8,
    /// Capabilities
    pub caps: u32,
}

/// Function group type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionGroupType {
    /// Audio function group
    Audio,
    /// Vendor-specific modem function group
    Modem,
    /// Vendor-defined
    VendorDefined,
    /// Unknown
    Unknown(u8),
}

impl From<u8> for FunctionGroupType {
    fn from(val: u8) -> Self {
        match val & 0x7F {
            0x01 => Self::Audio,
            0x02 => Self::Modem,
            0x80..=0xFF => Self::VendorDefined,
            other => Self::Unknown(other),
        }
    }
}

/// HDA Widget
#[derive(Clone, Debug)]
pub struct HdaWidget {
    /// Node ID
    pub nid: u8,
    /// Widget type
    pub widget_type: WidgetType,
    /// Capabilities
    pub caps: u32,
    /// Pin capabilities (for pin widgets)
    pub pin_caps: u32,
    /// Amplifier capabilities (input)
    pub amp_in_caps: u32,
    /// Amplifier capabilities (output)
    pub amp_out_caps: u32,
    /// Connection list
    pub connections: Vec<u8>,
    /// Configuration default (for pins)
    pub config_default: u32,
    /// Current pin control
    pub pin_ctl: u8,
    /// Current stream/channel
    pub stream_channel: u8,
}

impl HdaWidget {
    pub fn new(nid: u8, widget_type: WidgetType, caps: u32) -> Self {
        Self {
            nid,
            widget_type,
            caps,
            pin_caps: 0,
            amp_in_caps: 0,
            amp_out_caps: 0,
            connections: Vec::new(),
            config_default: 0,
            pin_ctl: 0,
            stream_channel: 0,
        }
    }

    /// Check if widget has output amplifier
    pub fn has_out_amp(&self) -> bool {
        (self.caps & (1 << 2)) != 0
    }

    /// Check if widget has input amplifier
    pub fn has_in_amp(&self) -> bool {
        (self.caps & (1 << 1)) != 0
    }

    /// Get number of steps for output amplifier
    pub fn out_amp_num_steps(&self) -> u8 {
        ((self.amp_out_caps >> 8) & 0x7F) as u8
    }

    /// Get step size for output amplifier (in 0.25 dB units)
    pub fn out_amp_step_size(&self) -> u8 {
        ((self.amp_out_caps >> 16) & 0x7F) as u8
    }

    /// Get offset for output amplifier
    pub fn out_amp_offset(&self) -> u8 {
        (self.amp_out_caps & 0x7F) as u8
    }
}

/// Widget type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetType {
    AudioOutput,
    AudioInput,
    AudioMixer,
    AudioSelector,
    PinComplex,
    Power,
    VolumeKnob,
    BeepGenerator,
    VendorDefined,
    Unknown(u8),
}

impl From<u8> for WidgetType {
    fn from(val: u8) -> Self {
        match val {
            WIDGET_AUDIO_OUT => Self::AudioOutput,
            WIDGET_AUDIO_IN => Self::AudioInput,
            WIDGET_AUDIO_MIX => Self::AudioMixer,
            WIDGET_AUDIO_SEL => Self::AudioSelector,
            WIDGET_PIN => Self::PinComplex,
            WIDGET_POWER => Self::Power,
            WIDGET_VOL_KNOB => Self::VolumeKnob,
            WIDGET_BEEP => Self::BeepGenerator,
            WIDGET_VENDOR => Self::VendorDefined,
            other => Self::Unknown(other),
        }
    }
}

/// HDA Controller
pub struct HdaController {
    /// Base address
    base_addr: u64,
    /// Mapped MMIO address
    mmio_base: usize,
    /// Controller version
    version: (u8, u8),
    /// Number of input streams
    input_streams: u8,
    /// Number of output streams
    output_streams: u8,
    /// Number of bidirectional streams
    bidi_streams: u8,
    /// Serial data out signals
    sdo_signals: u8,
    /// 64-bit addressing supported
    addr64: bool,
    /// Codecs
    codecs: Vec<HdaCodec>,
    /// CORB buffer
    corb: Option<DmaBuffer>,
    /// RIRB buffer
    rirb: Option<DmaBuffer>,
    /// CORB write pointer
    corb_wp: AtomicU32,
    /// RIRB read pointer
    rirb_rp: AtomicU32,
    /// Active output stream
    active_output: AtomicU32,
    /// Active input stream
    active_input: AtomicU32,
    /// Is running
    running: AtomicBool,
    /// Stream buffers
    stream_buffers: Vec<Option<DmaBuffer>>,
    /// BDL buffers
    bdl_buffers: Vec<Option<DmaBuffer>>,
}

impl HdaController {
    /// Create new HDA controller
    pub fn new(base_addr: u64) -> Self {
        Self {
            base_addr,
            mmio_base: 0,
            version: (0, 0),
            input_streams: 0,
            output_streams: 0,
            bidi_streams: 0,
            sdo_signals: 0,
            addr64: false,
            codecs: Vec::new(),
            corb: None,
            rirb: None,
            corb_wp: AtomicU32::new(0),
            rirb_rp: AtomicU32::new(0),
            active_output: AtomicU32::new(0),
            active_input: AtomicU32::new(0),
            running: AtomicBool::new(false),
            stream_buffers: Vec::new(),
            bdl_buffers: Vec::new(),
        }
    }

    /// Initialize controller
    pub fn init(&mut self) -> Result<(), SoundError> {
        // Map MMIO region
        self.mmio_base = crate::memory::map_mmio(self.base_addr, 0x4000)
            .ok_or(SoundError::NoMemory)? as usize;

        // Read capabilities
        let gcap = self.read32(REG_GCAP);
        self.version = (self.read8(REG_VMAJ), self.read8(REG_VMIN));
        self.output_streams = ((gcap >> 12) & 0x0F) as u8;
        self.input_streams = ((gcap >> 8) & 0x0F) as u8;
        self.bidi_streams = ((gcap >> 3) & 0x1F) as u8;
        self.sdo_signals = ((gcap >> 1) & 0x03) as u8;
        self.addr64 = (gcap & 1) != 0;

        crate::kprintln!("[HDA] Version {}.{}, {} out, {} in, {} bidi streams",
            self.version.0, self.version.1,
            self.output_streams, self.input_streams, self.bidi_streams);

        // Reset controller
        self.reset()?;

        // Setup CORB/RIRB
        self.setup_corb_rirb()?;

        // Probe codecs
        self.probe_codecs()?;

        // Initialize stream buffers
        let total_streams = self.input_streams + self.output_streams + self.bidi_streams;
        for _ in 0..total_streams {
            self.stream_buffers.push(None);
            self.bdl_buffers.push(None);
        }

        Ok(())
    }

    /// Reset controller
    fn reset(&mut self) -> Result<(), SoundError> {
        // Enter reset
        self.write32(REG_GCTL, 0);

        // Wait for reset to take effect
        for _ in 0..100 {
            if (self.read32(REG_GCTL) & GCTL_RESET) == 0 {
                break;
            }
            self.delay(1000);
        }

        // Exit reset
        self.write32(REG_GCTL, GCTL_RESET);

        // Wait for controller to come out of reset
        for _ in 0..100 {
            if (self.read32(REG_GCTL) & GCTL_RESET) != 0 {
                break;
            }
            self.delay(1000);
        }

        if (self.read32(REG_GCTL) & GCTL_RESET) == 0 {
            return Err(SoundError::HardwareError);
        }

        // Wait for codecs to initialize
        self.delay(10000);

        Ok(())
    }

    /// Setup CORB/RIRB
    fn setup_corb_rirb(&mut self) -> Result<(), SoundError> {
        // Allocate CORB buffer (256 entries * 4 bytes = 1KB)
        let corb_size = 256 * 4;
        let corb_phys = crate::memory::alloc_dma_buffer(corb_size)
            .ok_or(SoundError::NoMemory)?;
        let corb_virt = crate::memory::map_mmio(corb_phys, corb_size)
            .ok_or(SoundError::NoMemory)?;

        // Clear CORB
        unsafe {
            core::ptr::write_bytes(corb_virt as *mut u8, 0, corb_size);
        }

        self.corb = Some(DmaBuffer::new(corb_phys, corb_virt as usize, corb_size, corb_size));

        // Allocate RIRB buffer (256 entries * 8 bytes = 2KB)
        let rirb_size = 256 * 8;
        let rirb_phys = crate::memory::alloc_dma_buffer(rirb_size)
            .ok_or(SoundError::NoMemory)?;
        let rirb_virt = crate::memory::map_mmio(rirb_phys, rirb_size)
            .ok_or(SoundError::NoMemory)?;

        // Clear RIRB
        unsafe {
            core::ptr::write_bytes(rirb_virt as *mut u8, 0, rirb_size);
        }

        self.rirb = Some(DmaBuffer::new(rirb_phys, rirb_virt as usize, rirb_size, rirb_size));

        // Stop CORB/RIRB
        self.write8(REG_CORBCTL, 0);
        self.write8(REG_RIRBCTL, 0);

        // Set CORB base address
        if let Some(ref corb) = self.corb {
            self.write32(REG_CORBLBASE, corb.phys_addr as u32);
            self.write32(REG_CORBUBASE, (corb.phys_addr >> 32) as u32);
        }

        // Set RIRB base address
        if let Some(ref rirb) = self.rirb {
            self.write32(REG_RIRBLBASE, rirb.phys_addr as u32);
            self.write32(REG_RIRBUBASE, (rirb.phys_addr >> 32) as u32);
        }

        // Set CORB size to 256 entries
        self.write8(REG_CORBSIZE, 0x02);

        // Set RIRB size to 256 entries
        self.write8(REG_RIRBSIZE, 0x02);

        // Reset CORB read pointer
        self.write16(REG_CORBRP, 0x8000);
        for _ in 0..100 {
            if (self.read16(REG_CORBRP) & 0x8000) != 0 {
                break;
            }
            self.delay(100);
        }
        self.write16(REG_CORBRP, 0);
        for _ in 0..100 {
            if (self.read16(REG_CORBRP) & 0x8000) == 0 {
                break;
            }
            self.delay(100);
        }

        // Reset RIRB write pointer
        self.write16(REG_RIRBWP, 0x8000);

        // Set response interrupt count
        self.write16(REG_RINTCNT, 1);

        // Start CORB
        self.write8(REG_CORBCTL, 0x02);

        // Start RIRB with interrupt enable
        self.write8(REG_RIRBCTL, 0x03);

        Ok(())
    }

    /// Probe for codecs
    fn probe_codecs(&mut self) -> Result<(), SoundError> {
        // Wait for codec detection
        self.delay(10000);

        let statests = self.read16(REG_STATESTS);

        for addr in 0..15u8 {
            if (statests & (1 << addr)) != 0 {
                if let Ok(codec) = self.probe_codec(addr) {
                    crate::kprintln!("[HDA] Codec {}: Vendor {:04X}:{:04X}",
                        addr, codec.vendor_id, codec.device_id);
                    self.codecs.push(codec);
                }
            }
        }

        // Clear STATESTS
        self.write16(REG_STATESTS, statests);

        if self.codecs.is_empty() {
            return Err(SoundError::NoDevice);
        }

        Ok(())
    }

    /// Probe single codec
    fn probe_codec(&mut self, address: u8) -> Result<HdaCodec, SoundError> {
        let mut codec = HdaCodec::new(address);

        // Get vendor ID
        let vendor_id = self.send_verb(address, 0, VERB_GET_PARAM, PARAM_VENDOR_ID as u32)?;
        codec.vendor_id = (vendor_id >> 16) as u16;
        codec.device_id = vendor_id as u16;

        // Get revision ID
        codec.revision_id = self.send_verb(address, 0, VERB_GET_PARAM, PARAM_REVISION_ID as u32)?;

        // Get subordinate node count
        let sub_nodes = self.send_verb(address, 0, VERB_GET_PARAM, PARAM_SUB_NODE_COUNT as u32)?;
        let start_nid = ((sub_nodes >> 16) & 0xFF) as u8;
        let num_nodes = (sub_nodes & 0xFF) as u8;

        // Enumerate function groups
        for nid in start_nid..(start_nid + num_nodes) {
            let fg_type_raw = self.send_verb(address, nid, VERB_GET_PARAM, PARAM_FN_GROUP_TYPE as u32)?;
            let fg_type = FunctionGroupType::from((fg_type_raw & 0xFF) as u8);
            let is_audio = matches!(fg_type, FunctionGroupType::Audio);
            let fg = HdaFunctionGroup {
                nid,
                fg_type,
                start_nid: 0,
                num_nodes: 0,
                caps: 0,
            };
            codec.function_groups.push(fg);

            // If audio function group, enumerate widgets
            if is_audio {
                self.enumerate_widgets(&mut codec, address, nid)?;
            }
        }

        Ok(codec)
    }

    /// Enumerate widgets for audio function group
    fn enumerate_widgets(&mut self, codec: &mut HdaCodec, caddr: u8, fg_nid: u8) -> Result<(), SoundError> {
        // Get widget count
        let sub_nodes = self.send_verb(caddr, fg_nid, VERB_GET_PARAM, PARAM_SUB_NODE_COUNT as u32)?;
        let start_nid = ((sub_nodes >> 16) & 0xFF) as u8;
        let num_nodes = (sub_nodes & 0xFF) as u8;

        for nid in start_nid..(start_nid.saturating_add(num_nodes)) {
            // Get widget capabilities
            let caps = self.send_verb(caddr, nid, VERB_GET_PARAM, PARAM_AW_CAP as u32)?;
            let widget_type = WidgetType::from(((caps >> 20) & 0xF) as u8);

            let mut widget = HdaWidget::new(nid, widget_type, caps);

            // Get additional capabilities based on widget type
            match widget_type {
                WidgetType::PinComplex => {
                    widget.pin_caps = self.send_verb(caddr, nid, VERB_GET_PARAM, PARAM_PIN_CAP as u32)?;
                    widget.config_default = self.send_verb(caddr, nid, VERB_GET_CONFIG_DEFAULT, 0)?;
                }
                _ => {}
            }

            // Get amplifier capabilities
            if widget.has_out_amp() {
                widget.amp_out_caps = self.send_verb(caddr, nid, VERB_GET_PARAM, PARAM_OUT_AMP_CAP as u32)?;
            }
            if widget.has_in_amp() {
                widget.amp_in_caps = self.send_verb(caddr, nid, VERB_GET_PARAM, PARAM_IN_AMP_CAP as u32)?;
            }

            // Get connection list
            let conn_len = self.send_verb(caddr, nid, VERB_GET_PARAM, PARAM_CONN_LIST_LEN as u32)?;
            let num_conns = (conn_len & 0x7F) as usize;
            if num_conns > 0 && num_conns < 32 {
                for i in 0..num_conns {
                    let entry = self.send_verb(caddr, nid, VERB_GET_CONN_LIST, i as u32)?;
                    widget.connections.push((entry & 0xFF) as u8);
                }
            }

            codec.widgets.push(widget);
        }

        Ok(())
    }

    /// Send verb to codec
    pub fn send_verb(&mut self, caddr: u8, nid: u8, verb: u32, payload: u32) -> Result<u32, SoundError> {
        let cmd = ((caddr as u32) << 28) | ((nid as u32) << 20) | (verb << 8) | (payload & 0xFF);
        self.send_command(cmd)
    }

    /// Send command via CORB
    fn send_command(&mut self, cmd: u32) -> Result<u32, SoundError> {
        let corb = self.corb.as_ref().ok_or(SoundError::NotReady)?;

        // Get current write pointer
        let wp = self.corb_wp.fetch_add(1, Ordering::SeqCst) % 256;

        // Write command to CORB
        unsafe {
            let entry = (corb.virt_addr as *mut u32).add(wp as usize);
            core::ptr::write_volatile(entry, cmd);
        }

        // Update hardware write pointer
        self.write16(REG_CORBWP, wp as u16);

        // Wait for response in RIRB
        self.read_response()
    }

    /// Read response from RIRB
    fn read_response(&mut self) -> Result<u32, SoundError> {
        let rirb = self.rirb.as_ref().ok_or(SoundError::NotReady)?;

        // Wait for response
        for _ in 0..1000 {
            let wp = self.read16(REG_RIRBWP) as u32;
            let rp = self.rirb_rp.load(Ordering::Acquire);

            if wp != rp {
                // Read response
                let next_rp = (rp + 1) % 256;
                let response = unsafe {
                    let entry = (rirb.virt_addr as *const RirbEntry).add(next_rp as usize);
                    core::ptr::read_volatile(entry)
                };

                self.rirb_rp.store(next_rp, Ordering::Release);

                // Clear RIRB status
                self.write8(REG_RIRBSTS, 0x05);

                return Ok(response.response);
            }

            self.delay(100);
        }

        Err(SoundError::Timeout)
    }

    /// Setup output stream
    pub fn setup_output_stream(
        &mut self,
        stream_idx: u8,
        format: SampleFormat,
        rate: u32,
        channels: u32,
        buffer_size: usize,
    ) -> Result<(), SoundError> {
        if stream_idx >= self.output_streams {
            return Err(SoundError::InvalidParameter);
        }

        let stream_offset = STREAM_DESC_OFFSET + (stream_idx as u16) * STREAM_DESC_SIZE;

        // Stop stream
        self.write_stream(stream_offset, REG_SD_CTL, 0u32);
        self.delay(1000);

        // Reset stream
        self.write_stream(stream_offset, REG_SD_CTL, SD_CTL_RESET);
        for _ in 0..100 {
            if (self.read_stream::<u32>(stream_offset, REG_SD_CTL) & SD_CTL_RESET) != 0 {
                break;
            }
            self.delay(100);
        }

        // Clear reset
        self.write_stream(stream_offset, REG_SD_CTL, 0u32);
        for _ in 0..100 {
            if (self.read_stream::<u32>(stream_offset, REG_SD_CTL) & SD_CTL_RESET) == 0 {
                break;
            }
            self.delay(100);
        }

        // Allocate DMA buffer
        let buffer_phys = crate::memory::alloc_dma_buffer(buffer_size)
            .ok_or(SoundError::NoMemory)?;
        let buffer_virt = crate::memory::map_mmio(buffer_phys, buffer_size)
            .ok_or(SoundError::NoMemory)?;

        // Clear buffer
        unsafe {
            core::ptr::write_bytes(buffer_virt as *mut u8, 0, buffer_size);
        }

        self.stream_buffers[stream_idx as usize] = Some(DmaBuffer::new(
            buffer_phys, buffer_virt as usize, buffer_size, buffer_size / 4
        ));

        // Allocate BDL (Buffer Descriptor List) - 4 entries
        let bdl_size = 4 * core::mem::size_of::<BdlEntry>();
        let bdl_phys = crate::memory::alloc_dma_buffer(bdl_size)
            .ok_or(SoundError::NoMemory)?;
        let bdl_virt = crate::memory::map_mmio(bdl_phys, bdl_size)
            .ok_or(SoundError::NoMemory)?;

        // Setup BDL entries (4 periods)
        let period_size = buffer_size / 4;
        for i in 0..4 {
            let entry = BdlEntry {
                addr_low: (buffer_phys + (i * period_size) as u64) as u32,
                addr_high: ((buffer_phys + (i * period_size) as u64) >> 32) as u32,
                length: period_size as u32,
                ioc: 1, // Interrupt on completion
            };
            unsafe {
                let ptr = (bdl_virt as *mut BdlEntry).add(i);
                core::ptr::write_volatile(ptr, entry);
            }
        }

        self.bdl_buffers[stream_idx as usize] = Some(DmaBuffer::new(
            bdl_phys, bdl_virt as usize, bdl_size, bdl_size
        ));

        // Set BDL address
        self.write_stream(stream_offset, REG_SD_BDLPL, bdl_phys as u32);
        self.write_stream(stream_offset, REG_SD_BDLPU, (bdl_phys >> 32) as u32);

        // Set cyclic buffer length
        self.write_stream(stream_offset, REG_SD_CBL, buffer_size as u32);

        // Set last valid index (4 entries - 1)
        self.write_stream(stream_offset, REG_SD_LVI, 3u16);

        // Set format
        let fmt = self.encode_format(format, rate, channels)?;
        self.write_stream(stream_offset, REG_SD_FMT, fmt);

        // Set stream ID in control register (use stream_idx + 1 as stream tag)
        let stream_tag = (stream_idx + 1) as u32;
        self.write_stream(stream_offset, REG_SD_CTL, stream_tag << 20);

        Ok(())
    }

    /// Encode format register
    fn encode_format(&self, format: SampleFormat, rate: u32, channels: u32) -> Result<u16, SoundError> {
        // Base rate (48kHz = 0, 44.1kHz = 1)
        let base = if rate % 44100 == 0 { 1u16 } else { 0u16 };

        // Rate multiplier
        let mult = match rate {
            8000 | 11025 => 0,
            16000 | 22050 => 0,
            32000 | 44100 | 48000 => 0,
            88200 | 96000 => 1,
            176400 | 192000 => 3,
            _ => return Err(SoundError::RateNotSupported),
        };

        // Rate divisor
        let div = match rate {
            8000 => 5,
            11025 => 3,
            16000 => 2,
            22050 => 1,
            32000 | 44100 | 48000 => 0,
            _ => 0,
        };

        // Bits per sample
        let bits = match format {
            SampleFormat::S8 | SampleFormat::U8 => 0,
            SampleFormat::S16Le | SampleFormat::S16Be => 1,
            SampleFormat::S24Le32 | SampleFormat::S24Be32 => 3,
            SampleFormat::S32Le | SampleFormat::S32Be => 4,
            _ => return Err(SoundError::FormatNotSupported),
        };

        // Channels (0 = mono, 1 = stereo, etc.)
        let chan = (channels - 1) as u16;

        Ok((base << 14) | (mult << 11) | (div << 8) | (bits << 4) | chan)
    }

    /// Start stream
    pub fn start_stream(&mut self, stream_idx: u8, is_output: bool) -> Result<(), SoundError> {
        let stream_offset = if is_output {
            STREAM_DESC_OFFSET + (stream_idx as u16) * STREAM_DESC_SIZE
        } else {
            STREAM_DESC_OFFSET + ((self.output_streams + stream_idx) as u16) * STREAM_DESC_SIZE
        };

        // Enable interrupts and run
        let ctl = self.read_stream::<u32>(stream_offset, REG_SD_CTL);
        self.write_stream(stream_offset, REG_SD_CTL, ctl | SD_CTL_RUN | SD_CTL_IOCE);

        self.running.store(true, Ordering::Release);

        Ok(())
    }

    /// Stop stream
    pub fn stop_stream(&mut self, stream_idx: u8, is_output: bool) -> Result<(), SoundError> {
        let stream_offset = if is_output {
            STREAM_DESC_OFFSET + (stream_idx as u16) * STREAM_DESC_SIZE
        } else {
            STREAM_DESC_OFFSET + ((self.output_streams + stream_idx) as u16) * STREAM_DESC_SIZE
        };

        // Stop stream
        let ctl = self.read_stream::<u32>(stream_offset, REG_SD_CTL);
        self.write_stream(stream_offset, REG_SD_CTL, ctl & !SD_CTL_RUN);

        Ok(())
    }

    /// Get stream position
    pub fn get_stream_position(&self, stream_idx: u8, is_output: bool) -> u32 {
        let stream_offset = if is_output {
            STREAM_DESC_OFFSET + (stream_idx as u16) * STREAM_DESC_SIZE
        } else {
            STREAM_DESC_OFFSET + ((self.output_streams + stream_idx) as u16) * STREAM_DESC_SIZE
        };

        self.read_stream::<u32>(stream_offset, REG_SD_LPIB)
    }

    /// Write to stream buffer
    pub fn write_buffer(&self, stream_idx: u8, data: &[u8], offset: usize) -> Result<usize, SoundError> {
        let buffer = self.stream_buffers.get(stream_idx as usize)
            .and_then(|b| b.as_ref())
            .ok_or(SoundError::NotReady)?;

        let write_len = data.len().min(buffer.size - offset);
        unsafe {
            core::ptr::copy_nonoverlapping(
                data.as_ptr(),
                (buffer.virt_addr + offset) as *mut u8,
                write_len
            );
        }

        Ok(write_len)
    }

    // Register access helpers
    fn read8(&self, reg: u16) -> u8 {
        unsafe { core::ptr::read_volatile((self.mmio_base + reg as usize) as *const u8) }
    }

    fn read16(&self, reg: u16) -> u16 {
        unsafe { core::ptr::read_volatile((self.mmio_base + reg as usize) as *const u16) }
    }

    fn read32(&self, reg: u16) -> u32 {
        unsafe { core::ptr::read_volatile((self.mmio_base + reg as usize) as *const u32) }
    }

    fn write8(&self, reg: u16, val: u8) {
        unsafe { core::ptr::write_volatile((self.mmio_base + reg as usize) as *mut u8, val) }
    }

    fn write16(&self, reg: u16, val: u16) {
        unsafe { core::ptr::write_volatile((self.mmio_base + reg as usize) as *mut u16, val) }
    }

    fn write32(&self, reg: u16, val: u32) {
        unsafe { core::ptr::write_volatile((self.mmio_base + reg as usize) as *mut u32, val) }
    }

    fn read_stream<T: Copy>(&self, stream_base: u16, reg: u16) -> T {
        unsafe {
            core::ptr::read_volatile((self.mmio_base + stream_base as usize + reg as usize) as *const T)
        }
    }

    fn write_stream<T>(&self, stream_base: u16, reg: u16, val: T) {
        unsafe {
            core::ptr::write_volatile((self.mmio_base + stream_base as usize + reg as usize) as *mut T, val)
        }
    }

    fn delay(&self, us: u64) {
        crate::time::delay_us(us);
    }
}

/// Initialize HDA controller
pub fn init_controller(bus: u8, slot: u8, func: u8) -> Option<SoundCard> {
    // Get BAR0
    let bar0 = crate::drivers::pci::read_bar(bus, slot, func, 0)?;

    // Create controller
    let mut controller = HdaController::new(bar0);

    // Initialize
    if controller.init().is_err() {
        return None;
    }

    // Create sound card
    let mut card = SoundCard::new(
        CardType::IntelHda,
        "snd_hda_intel",
        "HDA Intel Controller"
    );

    // Add playback device
    if controller.output_streams > 0 {
        let mut device = SoundDevice::new(
            0,
            String::from("HDA Digital"),
            DeviceType::Pcm,
            StreamDirection::Playback,
            controller.output_streams as u32
        );
        device.capabilities = CardCapabilities {
            playback: true,
            capture: false,
            full_duplex: true,
            hardware_mixing: false,
            hardware_volume: true,
            max_playback_channels: 8,
            max_capture_channels: 0,
            sample_rates: SampleRateCapability::standard(),
            formats: FormatCapability::new(FormatCapability::COMMON),
        };
        card.add_device(device);
    }

    // Add capture device
    if controller.input_streams > 0 {
        let mut device = SoundDevice::new(
            1,
            String::from("HDA Digital Capture"),
            DeviceType::Pcm,
            StreamDirection::Capture,
            controller.input_streams as u32
        );
        device.capabilities = CardCapabilities {
            playback: false,
            capture: true,
            full_duplex: true,
            hardware_mixing: false,
            hardware_volume: true,
            max_playback_channels: 0,
            max_capture_channels: 8,
            sample_rates: SampleRateCapability::standard(),
            formats: FormatCapability::new(FormatCapability::COMMON),
        };
        card.add_device(device);
    }

    // Create mixer
    let mixer = Mixer::new("HDA Mixer", card.id);

    // Add master volume control
    let master_vol = MixerControl::new("Master", 0, ControlType::Integer, 2);
    let mut master_elem = MixerElement::new("Master", 0);
    master_elem.add_control(master_vol);
    mixer.add_element(master_elem);

    // Add PCM volume control
    let pcm_vol = MixerControl::new("PCM", 1, ControlType::Integer, 2);
    let mut pcm_elem = MixerElement::new("PCM", 1);
    pcm_elem.add_control(pcm_vol);
    mixer.add_element(pcm_elem);

    // Add headphone control
    let hp_switch = MixerControl::new("Headphone", 2, ControlType::Boolean, 1);
    let mut hp_elem = MixerElement::new("Headphone", 2);
    hp_elem.add_control(hp_switch);
    mixer.add_element(hp_elem);

    card.set_mixer(mixer);

    // Store controller
    card.set_private_data(controller);

    Some(card)
}
