// ===============================================================================
// BUILDOS KERNEL - INTEL HIGH DEFINITION AUDIO (HDA) DRIVER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================
//
// Intel High Definition Audio driver for modern audio controllers.
// Supports Intel, AMD, NVIDIA, and other HDA-compatible controllers.
//
// ===============================================================================

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
use spin::Mutex;

use super::{
    AudioDevice, AudioDeviceInfo, AudioStream, AudioFormat, AudioError,
    AudioBuffer, AudioRingBuffer, SampleFormat, StreamState, StreamDirection,
};

// =============================================================================
// HDA REGISTER DEFINITIONS
// =============================================================================

// Global Registers
const HDA_GCAP: u16 = 0x00;      // Global Capabilities
const HDA_VMIN: u16 = 0x02;      // Minor Version
const HDA_VMAJ: u16 = 0x03;      // Major Version
const HDA_OUTPAY: u16 = 0x04;    // Output Payload Capability
const HDA_INPAY: u16 = 0x06;     // Input Payload Capability
const HDA_GCTL: u16 = 0x08;      // Global Control
const HDA_WAKEEN: u16 = 0x0C;    // Wake Enable
const HDA_STATESTS: u16 = 0x0E; // State Change Status
const HDA_GSTS: u16 = 0x10;      // Global Status
const HDA_OUTSTRMPAY: u16 = 0x18; // Output Stream Payload
const HDA_INSTRMPAY: u16 = 0x1A;  // Input Stream Payload
const HDA_INTCTL: u16 = 0x20;    // Interrupt Control
const HDA_INTSTS: u16 = 0x24;    // Interrupt Status
const HDA_WALCLK: u16 = 0x30;    // Wall Clock Counter
const HDA_SSYNC: u16 = 0x38;     // Stream Synchronization
const HDA_CORBLBASE: u16 = 0x40; // CORB Lower Base Address
const HDA_CORBUBASE: u16 = 0x44; // CORB Upper Base Address
const HDA_CORBWP: u16 = 0x48;    // CORB Write Pointer
const HDA_CORBRP: u16 = 0x4A;    // CORB Read Pointer
const HDA_CORBCTL: u16 = 0x4C;   // CORB Control
const HDA_CORBSTS: u16 = 0x4D;   // CORB Status
const HDA_CORBSIZE: u16 = 0x4E;  // CORB Size
const HDA_RIRBLBASE: u16 = 0x50; // RIRB Lower Base Address
const HDA_RIRBUBASE: u16 = 0x54; // RIRB Upper Base Address
const HDA_RIRBWP: u16 = 0x58;    // RIRB Write Pointer
const HDA_RINTCNT: u16 = 0x5A;   // Response Interrupt Count
const HDA_RIRBCTL: u16 = 0x5C;   // RIRB Control
const HDA_RIRBSTS: u16 = 0x5D;   // RIRB Status
const HDA_RIRBSIZE: u16 = 0x5E;  // RIRB Size
const HDA_DPLBASE: u16 = 0x70;   // DMA Position Lower Base
const HDA_DPUBASE: u16 = 0x74;   // DMA Position Upper Base

// Stream Descriptor Registers (offset from stream base)
const HDA_SD_CTL: u8 = 0x00;     // Stream Descriptor Control
const HDA_SD_STS: u8 = 0x03;     // Stream Descriptor Status
const HDA_SD_LPIB: u8 = 0x04;    // Link Position in Buffer
const HDA_SD_CBL: u8 = 0x08;     // Cyclic Buffer Length
const HDA_SD_LVI: u8 = 0x0C;     // Last Valid Index
const HDA_SD_FIFOW: u8 = 0x0E;   // FIFO Watermark
const HDA_SD_FIFOS: u8 = 0x10;   // FIFO Size
const HDA_SD_FMT: u8 = 0x12;     // Format
const HDA_SD_BDLPL: u8 = 0x18;   // BDL Pointer Lower
const HDA_SD_BDLPU: u8 = 0x1C;   // BDL Pointer Upper

// Global Control bits
const GCTL_CRST: u32 = 1 << 0;   // Controller Reset
const GCTL_FCNTRL: u32 = 1 << 1; // Flush Control
const GCTL_UNSOL: u32 = 1 << 8;  // Accept Unsolicited Responses

// Interrupt Control bits
const INTCTL_GIE: u32 = 1 << 31; // Global Interrupt Enable
const INTCTL_CIE: u32 = 1 << 30; // Controller Interrupt Enable

// CORB/RIRB Control bits
const CORBCTL_RUN: u8 = 1 << 1;  // CORB DMA Enable
const CORBCTL_MEIE: u8 = 1 << 0; // Memory Error Interrupt Enable
const RIRBCTL_RUN: u8 = 1 << 1;  // RIRB DMA Enable
const RIRBCTL_INTCTL: u8 = 1 << 0; // Response Interrupt Control

// Stream Descriptor Control bits
const SD_CTL_SRST: u32 = 1 << 0;  // Stream Reset
const SD_CTL_RUN: u32 = 1 << 1;   // Stream Run
const SD_CTL_IOCE: u32 = 1 << 2;  // Interrupt on Completion Enable
const SD_CTL_FEIE: u32 = 1 << 3;  // FIFO Error Interrupt Enable
const SD_CTL_DEIE: u32 = 1 << 4;  // Descriptor Error Interrupt Enable

// Stream Descriptor Status bits
const SD_STS_BCIS: u8 = 1 << 2;   // Buffer Completion Interrupt Status
const SD_STS_FIFOE: u8 = 1 << 3;  // FIFO Error
const SD_STS_DESE: u8 = 1 << 4;   // Descriptor Error

// =============================================================================
// CODEC VERBS
// =============================================================================

// Verb types
const VERB_GET_PARAM: u32 = 0xF00;
const VERB_GET_CONN_SELECT: u32 = 0xF01;
const VERB_SET_CONN_SELECT: u32 = 0x701;
const VERB_GET_CONN_LIST: u32 = 0xF02;
const VERB_GET_PROC_STATE: u32 = 0xF03;
const VERB_SET_PROC_STATE: u32 = 0x703;
const VERB_GET_SDI_SELECT: u32 = 0xF04;
const VERB_SET_SDI_SELECT: u32 = 0x704;
const VERB_GET_POWER_STATE: u32 = 0xF05;
const VERB_SET_POWER_STATE: u32 = 0x705;
const VERB_GET_CONV: u32 = 0xF06;
const VERB_SET_CONV_CHANNEL: u32 = 0x706;
const VERB_GET_PIN_WIDGET_CTL: u32 = 0xF07;
const VERB_SET_PIN_WIDGET_CTL: u32 = 0x707;
const VERB_GET_UNSOL_RESP: u32 = 0xF08;
const VERB_SET_UNSOL_RESP: u32 = 0x708;
const VERB_GET_PIN_SENSE: u32 = 0xF09;
const VERB_EXEC_PIN_SENSE: u32 = 0x709;
const VERB_GET_EAPD_BTL: u32 = 0xF0C;
const VERB_SET_EAPD_BTL: u32 = 0x70C;
const VERB_GET_DIGI_CONVERT_1: u32 = 0xF0D;
const VERB_SET_DIGI_CONVERT_1: u32 = 0x70D;
const VERB_GET_DIGI_CONVERT_2: u32 = 0xF0E;
const VERB_SET_DIGI_CONVERT_2: u32 = 0x70E;
const VERB_GET_VOLUME_KNOB: u32 = 0xF0F;
const VERB_SET_VOLUME_KNOB: u32 = 0x70F;
const VERB_GET_GPIO_DATA: u32 = 0xF15;
const VERB_SET_GPIO_DATA: u32 = 0x715;
const VERB_GET_GPIO_MASK: u32 = 0xF16;
const VERB_SET_GPIO_MASK: u32 = 0x716;
const VERB_GET_GPIO_DIR: u32 = 0xF17;
const VERB_SET_GPIO_DIR: u32 = 0x717;
const VERB_GET_CONFIG_DEFAULT: u32 = 0xF1C;
const VERB_GET_CONV_STREAM_CHAN: u32 = 0xF06;
const VERB_SET_CONV_STREAM_CHAN: u32 = 0x706;
const VERB_SET_AMP_GAIN_MUTE: u32 = 0x300;
const VERB_GET_AMP_GAIN_MUTE: u32 = 0xB00;
const VERB_SET_COEF_INDEX: u32 = 0x500;
const VERB_GET_COEF_INDEX: u32 = 0xD00;
const VERB_SET_PROC_COEF: u32 = 0x400;
const VERB_GET_PROC_COEF: u32 = 0xC00;

// Parameter IDs
const PARAM_VENDOR_ID: u8 = 0x00;
const PARAM_REVISION_ID: u8 = 0x02;
const PARAM_NODE_COUNT: u8 = 0x04;
const PARAM_FUNC_TYPE: u8 = 0x05;
const PARAM_AUDIO_CAPS: u8 = 0x09;
const PARAM_PIN_CAPS: u8 = 0x0C;
const PARAM_AMP_IN_CAPS: u8 = 0x0D;
const PARAM_CONN_LIST_LEN: u8 = 0x0E;
const PARAM_POWER_STATES: u8 = 0x0F;
const PARAM_PROC_CAPS: u8 = 0x10;
const PARAM_GPIO_COUNT: u8 = 0x11;
const PARAM_AMP_OUT_CAPS: u8 = 0x12;
const PARAM_VOL_CAPS: u8 = 0x13;

// Widget Types
const WIDGET_AUDIO_OUT: u8 = 0x00;
const WIDGET_AUDIO_IN: u8 = 0x01;
const WIDGET_AUDIO_MIXER: u8 = 0x02;
const WIDGET_AUDIO_SELECTOR: u8 = 0x03;
const WIDGET_PIN_COMPLEX: u8 = 0x04;
const WIDGET_POWER: u8 = 0x05;
const WIDGET_VOLUME_KNOB: u8 = 0x06;
const WIDGET_BEEP: u8 = 0x07;
const WIDGET_VENDOR: u8 = 0x0F;

// =============================================================================
// BUFFER DESCRIPTOR LIST
// =============================================================================

/// Buffer Descriptor List Entry
#[derive(Clone, Copy, Default)]
#[repr(C, packed)]
pub struct BdlEntry {
    /// Physical address of buffer
    pub address: u64,
    /// Buffer length in bytes
    pub length: u32,
    /// Interrupt on completion flag
    pub ioc: u32,
}

const BDL_SIZE: usize = 256;
const BUFFER_SIZE: usize = 0x4000; // 16KB per buffer

// =============================================================================
// CORB/RIRB STRUCTURES
// =============================================================================

const CORB_SIZE: usize = 256;
const RIRB_SIZE: usize = 256;

/// RIRB Entry
#[derive(Clone, Copy, Default)]
#[repr(C, packed)]
pub struct RirbEntry {
    pub response: u32,
    pub response_ex: u32,
}

// =============================================================================
// CODEC NODE
// =============================================================================

/// HDA Codec Node
#[derive(Debug, Clone)]
pub struct CodecNode {
    pub nid: u8,
    pub widget_type: u8,
    pub caps: u32,
    pub pin_caps: u32,
    pub amp_in_caps: u32,
    pub amp_out_caps: u32,
    pub config_default: u32,
    pub connections: Vec<u8>,
}

/// HDA Codec
#[derive(Debug)]
pub struct Codec {
    pub address: u8,
    pub vendor_id: u32,
    pub revision_id: u32,
    pub nodes: BTreeMap<u8, CodecNode>,
    pub afg_nid: u8,
    pub dac_nids: Vec<u8>,
    pub adc_nids: Vec<u8>,
    pub out_pins: Vec<u8>,
    pub in_pins: Vec<u8>,
}

// =============================================================================
// HDA CONTROLLER
// =============================================================================

/// Intel HDA Controller
pub struct HdaController {
    /// Base address (MMIO)
    base: u64,
    /// Controller version
    version: (u8, u8),
    /// Number of output streams
    num_oss: u8,
    /// Number of input streams
    num_iss: u8,
    /// Number of bidirectional streams
    num_bss: u8,
    /// CORB buffer
    corb: Box<[u32; CORB_SIZE]>,
    /// RIRB buffer
    rirb: Box<[RirbEntry; RIRB_SIZE]>,
    /// CORB write pointer
    corb_wp: AtomicU32,
    /// RIRB read pointer
    rirb_rp: AtomicU32,
    /// Codecs
    codecs: Mutex<Vec<Codec>>,
    /// Volume (0-100)
    volume: AtomicU8,
    /// Muted
    muted: AtomicBool,
    /// Output stream base offset
    output_stream_offset: u32,
    /// Input stream base offset
    input_stream_offset: u32,
}

impl HdaController {
    /// Create new HDA controller
    pub fn new(base: u64) -> Option<Self> {
        crate::log::info!("HDA: Initializing controller at {:016x}", base);

        // Read capabilities
        let gcap = unsafe { read_volatile(base as *const u16) };
        let vmin = unsafe { read_volatile((base + HDA_VMIN as u64) as *const u8) };
        let vmaj = unsafe { read_volatile((base + HDA_VMAJ as u64) as *const u8) };

        crate::log::info!("HDA: Version {}.{}", vmaj, vmin);

        let num_oss = ((gcap >> 12) & 0xF) as u8;
        let num_iss = ((gcap >> 8) & 0xF) as u8;
        let num_bss = ((gcap >> 3) & 0x1F) as u8;

        crate::log::info!("HDA: {} output, {} input, {} bidirectional streams",
            num_oss, num_iss, num_bss);

        // Calculate stream offsets
        let output_stream_offset = 0x80 + (num_iss as u32) * 0x20;
        let input_stream_offset = 0x80;

        // Allocate CORB and RIRB
        let corb = Box::new([0u32; CORB_SIZE]);
        let rirb = Box::new([RirbEntry::default(); RIRB_SIZE]);

        let mut controller = Self {
            base,
            version: (vmaj, vmin),
            num_oss,
            num_iss,
            num_bss,
            corb,
            rirb,
            corb_wp: AtomicU32::new(0),
            rirb_rp: AtomicU32::new(0),
            codecs: Mutex::new(Vec::new()),
            volume: AtomicU8::new(100),
            muted: AtomicBool::new(false),
            output_stream_offset,
            input_stream_offset,
        };

        if !controller.reset() {
            crate::log::error!("HDA: Reset failed");
            return None;
        }

        controller.setup_corb_rirb();
        controller.detect_codecs();

        Some(controller)
    }

    /// Read register
    fn read32(&self, offset: u16) -> u32 {
        unsafe { read_volatile((self.base + offset as u64) as *const u32) }
    }

    /// Write register
    fn write32(&self, offset: u16, value: u32) {
        unsafe { write_volatile((self.base + offset as u64) as *mut u32, value) }
    }

    /// Read byte register
    fn read8(&self, offset: u16) -> u8 {
        unsafe { read_volatile((self.base + offset as u64) as *const u8) }
    }

    /// Write byte register
    fn write8(&self, offset: u16, value: u8) {
        unsafe { write_volatile((self.base + offset as u64) as *mut u8, value) }
    }

    /// Read word register
    fn read16(&self, offset: u16) -> u16 {
        unsafe { read_volatile((self.base + offset as u64) as *const u16) }
    }

    /// Write word register
    fn write16(&self, offset: u16, value: u16) {
        unsafe { write_volatile((self.base + offset as u64) as *mut u16, value) }
    }

    /// Reset controller
    fn reset(&self) -> bool {
        crate::log::debug!("HDA: Resetting controller");

        // Enter reset
        self.write32(HDA_GCTL, 0);

        // Wait for reset
        for _ in 0..1000 {
            if (self.read32(HDA_GCTL) & GCTL_CRST) == 0 {
                break;
            }
            for _ in 0..1000 { core::hint::spin_loop(); }
        }

        // Exit reset
        self.write32(HDA_GCTL, GCTL_CRST);

        // Wait for controller ready
        for _ in 0..1000 {
            if (self.read32(HDA_GCTL) & GCTL_CRST) != 0 {
                crate::log::debug!("HDA: Controller ready");

                // Wait additional time for codecs to initialize
                for _ in 0..100000 { core::hint::spin_loop(); }

                return true;
            }
            for _ in 0..1000 { core::hint::spin_loop(); }
        }

        crate::log::error!("HDA: Controller reset timeout");
        false
    }

    /// Setup CORB and RIRB
    fn setup_corb_rirb(&mut self) {
        // Stop CORB and RIRB
        self.write8(HDA_CORBCTL, 0);
        self.write8(HDA_RIRBCTL, 0);

        // Set CORB base address
        let corb_phys = self.corb.as_ptr() as u64;
        self.write32(HDA_CORBLBASE, corb_phys as u32);
        self.write32(HDA_CORBUBASE, (corb_phys >> 32) as u32);

        // Set RIRB base address
        let rirb_phys = self.rirb.as_ptr() as u64;
        self.write32(HDA_RIRBLBASE, rirb_phys as u32);
        self.write32(HDA_RIRBUBASE, (rirb_phys >> 32) as u32);

        // Set sizes (256 entries)
        self.write8(HDA_CORBSIZE, 0x02);
        self.write8(HDA_RIRBSIZE, 0x02);

        // Reset CORB read pointer
        self.write16(HDA_CORBRP, 0x8000);
        for _ in 0..1000 {
            if (self.read16(HDA_CORBRP) & 0x8000) != 0 {
                break;
            }
            core::hint::spin_loop();
        }
        self.write16(HDA_CORBRP, 0x0000);

        // Reset RIRB write pointer
        self.write16(HDA_RIRBWP, 0x8000);

        // Start CORB and RIRB
        self.write8(HDA_CORBCTL, CORBCTL_RUN | CORBCTL_MEIE);
        self.write8(HDA_RIRBCTL, RIRBCTL_RUN | RIRBCTL_INTCTL);

        // Set interrupt count
        self.write16(HDA_RINTCNT, 1);

        crate::log::debug!("HDA: CORB/RIRB initialized");
    }

    /// Send command to codec
    fn send_command(&self, codec: u8, nid: u8, verb: u32, param: u32) -> Option<u32> {
        let command = ((codec as u32) << 28)
            | ((nid as u32) << 20)
            | (verb << 8)
            | (param & 0xFF);

        // Write to CORB
        let wp = (self.corb_wp.load(Ordering::Acquire) + 1) % CORB_SIZE as u32;

        unsafe {
            let corb_ptr = self.corb.as_ptr() as *mut u32;
            write_volatile(corb_ptr.add(wp as usize), command);
        }

        // Update write pointer
        self.write16(HDA_CORBWP, wp as u16);
        self.corb_wp.store(wp, Ordering::Release);

        // Wait for response
        for _ in 0..10000 {
            let rirb_wp = self.read16(HDA_RIRBWP) as u32;
            let rp = self.rirb_rp.load(Ordering::Acquire);

            if rirb_wp != rp {
                let new_rp = (rp + 1) % RIRB_SIZE as u32;

                let response = unsafe {
                    let rirb_ptr = self.rirb.as_ptr();
                    read_volatile(rirb_ptr.add(new_rp as usize))
                };

                self.rirb_rp.store(new_rp, Ordering::Release);

                return Some(response.response);
            }

            for _ in 0..100 { core::hint::spin_loop(); }
        }

        crate::log::warn!("HDA: Command timeout");
        None
    }

    /// Get parameter from codec node
    fn get_parameter(&self, codec: u8, nid: u8, param: u8) -> Option<u32> {
        self.send_command(codec, nid, VERB_GET_PARAM >> 8, param as u32)
    }

    /// Detect codecs
    fn detect_codecs(&self) {
        let statests = self.read16(HDA_STATESTS);
        crate::log::debug!("HDA: STATESTS = {:04x}", statests);

        for addr in 0..15 {
            if (statests & (1 << addr)) != 0 {
                crate::log::info!("HDA: Codec found at address {}", addr);

                if let Some(codec) = self.probe_codec(addr as u8) {
                    self.codecs.lock().push(codec);
                }
            }
        }
    }

    /// Probe a codec
    fn probe_codec(&self, addr: u8) -> Option<Codec> {
        // Get vendor ID
        let vendor_id = self.get_parameter(addr, 0, PARAM_VENDOR_ID)?;
        let revision_id = self.get_parameter(addr, 0, PARAM_REVISION_ID)?;

        crate::log::info!("HDA: Codec vendor={:08x} revision={:08x}", vendor_id, revision_id);

        // Get function group info
        let node_count = self.get_parameter(addr, 0, PARAM_NODE_COUNT)?;
        let start_nid = ((node_count >> 16) & 0xFF) as u8;
        let num_nodes = (node_count & 0xFF) as u8;

        crate::log::debug!("HDA: Root has {} nodes starting at {}", num_nodes, start_nid);

        let mut codec = Codec {
            address: addr,
            vendor_id,
            revision_id,
            nodes: BTreeMap::new(),
            afg_nid: 0,
            dac_nids: Vec::new(),
            adc_nids: Vec::new(),
            out_pins: Vec::new(),
            in_pins: Vec::new(),
        };

        // Find Audio Function Group
        for nid in start_nid..start_nid + num_nodes {
            let func_type = self.get_parameter(addr, nid, PARAM_FUNC_TYPE)?;
            let node_type = (func_type & 0xFF) as u8;

            if node_type == 0x01 {
                // Audio Function Group
                codec.afg_nid = nid;
                self.probe_afg(&mut codec, nid)?;
                break;
            }
        }

        Some(codec)
    }

    /// Probe Audio Function Group
    fn probe_afg(&self, codec: &mut Codec, afg_nid: u8) -> Option<()> {
        let node_count = self.get_parameter(codec.address, afg_nid, PARAM_NODE_COUNT)?;
        let start_nid = ((node_count >> 16) & 0xFF) as u8;
        let num_nodes = (node_count & 0xFF) as u8;

        crate::log::debug!("HDA: AFG has {} widgets starting at {}", num_nodes, start_nid);

        for nid in start_nid..start_nid + num_nodes {
            if let Some(node) = self.probe_widget(codec.address, nid) {
                match node.widget_type {
                    WIDGET_AUDIO_OUT => codec.dac_nids.push(nid),
                    WIDGET_AUDIO_IN => codec.adc_nids.push(nid),
                    WIDGET_PIN_COMPLEX => {
                        let config = node.config_default;
                        let connectivity = (config >> 30) & 0x3;
                        let default_device = (config >> 20) & 0xF;

                        if connectivity != 0x1 {
                            // Not a "No physical connection"
                            match default_device {
                                0x0 => codec.out_pins.push(nid), // Line Out
                                0x1 => codec.out_pins.push(nid), // Speaker
                                0x2 => codec.out_pins.push(nid), // Headphones
                                0x8 => codec.in_pins.push(nid),  // Line In
                                0xA => codec.in_pins.push(nid),  // Mic
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }

                codec.nodes.insert(nid, node);
            }
        }

        crate::log::info!(
            "HDA: Found {} DACs, {} ADCs, {} output pins, {} input pins",
            codec.dac_nids.len(),
            codec.adc_nids.len(),
            codec.out_pins.len(),
            codec.in_pins.len()
        );

        Some(())
    }

    /// Probe a widget
    fn probe_widget(&self, codec_addr: u8, nid: u8) -> Option<CodecNode> {
        let caps = self.get_parameter(codec_addr, nid, PARAM_AUDIO_CAPS)?;
        let widget_type = ((caps >> 20) & 0xF) as u8;

        let mut node = CodecNode {
            nid,
            widget_type,
            caps,
            pin_caps: 0,
            amp_in_caps: 0,
            amp_out_caps: 0,
            config_default: 0,
            connections: Vec::new(),
        };

        // Get additional parameters based on widget type
        if widget_type == WIDGET_PIN_COMPLEX {
            node.pin_caps = self.get_parameter(codec_addr, nid, PARAM_PIN_CAPS).unwrap_or(0);
            node.config_default = self.send_command(codec_addr, nid, VERB_GET_CONFIG_DEFAULT >> 8, 0)
                .unwrap_or(0);
        }

        // Get amplifier capabilities
        node.amp_in_caps = self.get_parameter(codec_addr, nid, PARAM_AMP_IN_CAPS).unwrap_or(0);
        node.amp_out_caps = self.get_parameter(codec_addr, nid, PARAM_AMP_OUT_CAPS).unwrap_or(0);

        // Get connections
        let conn_len = self.get_parameter(codec_addr, nid, PARAM_CONN_LIST_LEN).unwrap_or(0);
        let num_conns = (conn_len & 0x7F) as u8;

        if num_conns > 0 {
            for i in 0..num_conns {
                if let Some(conn) = self.send_command(codec_addr, nid, VERB_GET_CONN_LIST >> 8, i as u32) {
                    node.connections.push((conn & 0xFF) as u8);
                }
            }
        }

        Some(node)
    }

    /// Set volume on output amplifier
    fn set_amp_gain(&self, codec: u8, nid: u8, left: u8, right: u8, mute: bool) {
        let mute_bit = if mute { 0x80 } else { 0x00 };

        // Left channel
        self.send_command(
            codec, nid,
            VERB_SET_AMP_GAIN_MUTE >> 8,
            0xA000 | ((left as u32 & 0x7F) | mute_bit as u32),
        );

        // Right channel
        self.send_command(
            codec, nid,
            VERB_SET_AMP_GAIN_MUTE >> 8,
            0x9000 | ((right as u32 & 0x7F) | mute_bit as u32),
        );
    }

    /// Enable interrupt
    pub fn enable_interrupt(&self, stream: u8) {
        let intctl = self.read32(HDA_INTCTL);
        self.write32(HDA_INTCTL, intctl | INTCTL_GIE | INTCTL_CIE | (1 << stream));
    }

    /// Handle interrupt
    pub fn handle_interrupt(&self) {
        let intsts = self.read32(HDA_INTSTS);

        // Handle stream interrupts
        for i in 0..(self.num_iss + self.num_oss) {
            if (intsts & (1 << i)) != 0 {
                let stream_offset = if i < self.num_iss {
                    0x80 + (i as u32) * 0x20
                } else {
                    self.output_stream_offset + ((i - self.num_iss) as u32) * 0x20
                };

                let status = self.read8((stream_offset + HDA_SD_STS as u32) as u16);

                if (status & SD_STS_BCIS) != 0 {
                    crate::log::trace!("HDA: Stream {} buffer complete", i);
                }

                // Clear status
                self.write8((stream_offset + HDA_SD_STS as u32) as u16, status);
            }
        }

        // Clear controller interrupt status
        self.write32(HDA_INTSTS, intsts);
    }
}

// =============================================================================
// AUDIO DEVICE IMPLEMENTATION
// =============================================================================

impl AudioDevice for HdaController {
    fn info(&self) -> AudioDeviceInfo {
        AudioDeviceInfo {
            name: "Intel HD Audio".to_string(),
            sample_rates: vec![44100, 48000, 96000, 192000],
            sample_formats: vec![SampleFormat::S16Le, SampleFormat::S24Le, SampleFormat::S32Le],
            min_channels: 2,
            max_channels: 8,
            playback: true,
            capture: true,
        }
    }

    fn open(
        &self,
        direction: StreamDirection,
        format: AudioFormat,
    ) -> Result<Box<dyn AudioStream>, AudioError> {
        let stream_idx = match direction {
            StreamDirection::Playback => {
                if self.num_oss == 0 {
                    return Err(AudioError::DeviceNotFound);
                }
                0 // Use first output stream
            }
            StreamDirection::Capture => {
                if self.num_iss == 0 {
                    return Err(AudioError::DeviceNotFound);
                }
                0 // Use first input stream
            }
        };

        let stream = HdaStream::new(
            self.base,
            direction,
            format,
            stream_idx,
            if direction == StreamDirection::Playback {
                self.output_stream_offset
            } else {
                self.input_stream_offset
            },
        );

        Ok(Box::new(stream))
    }

    fn volume(&self) -> u8 {
        self.volume.load(Ordering::Acquire)
    }

    fn set_volume(&self, volume: u8) {
        self.volume.store(volume.min(100), Ordering::Release);

        // Set volume on all DACs in first codec
        let codecs = self.codecs.lock();
        if let Some(codec) = codecs.first() {
            let gain = (volume as u32 * 127) / 100;
            for &dac in &codec.dac_nids {
                self.set_amp_gain(codec.address, dac, gain as u8, gain as u8, false);
            }
        }
    }

    fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Acquire)
    }

    fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Release);

        let codecs = self.codecs.lock();
        if let Some(codec) = codecs.first() {
            for &dac in &codec.dac_nids {
                let gain = if muted { 0 } else { (self.volume() as u32 * 127) / 100 };
                self.set_amp_gain(codec.address, dac, gain as u8, gain as u8, muted);
            }
        }
    }
}

// =============================================================================
// HDA STREAM
// =============================================================================

/// HDA Audio Stream
pub struct HdaStream {
    base: u64,
    direction: StreamDirection,
    format: AudioFormat,
    stream_idx: u8,
    stream_offset: u32,
    state: Mutex<StreamState>,
    bdl: Box<[BdlEntry; BDL_SIZE]>,
    buffers: Vec<AudioBuffer>,
    ring_buffer: AudioRingBuffer,
}

impl HdaStream {
    fn new(
        base: u64,
        direction: StreamDirection,
        format: AudioFormat,
        stream_idx: u8,
        stream_offset: u32,
    ) -> Self {
        let bdl = Box::new([BdlEntry::default(); BDL_SIZE]);
        let buffers: Vec<AudioBuffer> = (0..BDL_SIZE).map(|_| AudioBuffer::new(BUFFER_SIZE)).collect();
        let ring_buffer = AudioRingBuffer::new(BUFFER_SIZE * 4);

        Self {
            base,
            direction,
            format,
            stream_idx,
            stream_offset,
            state: Mutex::new(StreamState::Open),
            bdl,
            buffers,
            ring_buffer,
        }
    }

    fn stream_base(&self) -> u64 {
        self.base + self.stream_offset as u64 + (self.stream_idx as u64) * 0x20
    }

    fn read_stream32(&self, offset: u8) -> u32 {
        unsafe { read_volatile((self.stream_base() + offset as u64) as *const u32) }
    }

    fn write_stream32(&self, offset: u8, value: u32) {
        unsafe { write_volatile((self.stream_base() + offset as u64) as *mut u32, value) }
    }

    fn read_stream8(&self, offset: u8) -> u8 {
        unsafe { read_volatile((self.stream_base() + offset as u64) as *const u8) }
    }

    fn write_stream8(&self, offset: u8, value: u8) {
        unsafe { write_volatile((self.stream_base() + offset as u64) as *mut u8, value) }
    }

    fn write_stream16(&self, offset: u8, value: u16) {
        unsafe { write_volatile((self.stream_base() + offset as u64) as *mut u16, value) }
    }

    /// Calculate format register value
    fn format_value(&self) -> u16 {
        let mut fmt: u16 = 0;

        // Sample rate base and multiplier
        fmt |= match self.format.sample_rate {
            8000 => 0x0005,    // 48000 / 6
            11025 => 0x4003,   // 44100 / 4
            16000 => 0x0003,   // 48000 / 3
            22050 => 0x4001,   // 44100 / 2
            32000 => 0x000A,   // 48000 * 2 / 3
            44100 => 0x4000,   // 44100 / 1
            48000 => 0x0000,   // 48000 / 1
            96000 => 0x0800,   // 48000 * 2
            192000 => 0x1800,  // 48000 * 4
            _ => 0x0000,
        };

        // Bits per sample
        fmt |= match self.format.sample_format {
            SampleFormat::U8 => 0x0000,
            SampleFormat::S16Le | SampleFormat::S16Be => 0x0010,
            SampleFormat::S24Le | SampleFormat::S24Be => 0x0030,
            SampleFormat::S32Le | SampleFormat::S32Be => 0x0040,
            SampleFormat::F32Le | SampleFormat::F32Be => 0x0040,
        };

        // Channels (0 = 1 channel, 1 = 2 channels, etc.)
        fmt |= (self.format.channels - 1) as u16;

        fmt
    }
}

impl AudioStream for HdaStream {
    fn format(&self) -> AudioFormat {
        self.format
    }

    fn state(&self) -> StreamState {
        *self.state.lock()
    }

    fn direction(&self) -> StreamDirection {
        self.direction
    }

    fn prepare(&self) -> Result<(), AudioError> {
        let mut state = self.state.lock();

        if *state != StreamState::Open {
            return Err(AudioError::InvalidState);
        }

        // Reset stream
        self.write_stream32(HDA_SD_CTL, SD_CTL_SRST);
        for _ in 0..1000 {
            if (self.read_stream32(HDA_SD_CTL) & SD_CTL_SRST) != 0 {
                break;
            }
            core::hint::spin_loop();
        }

        // Clear reset
        self.write_stream32(HDA_SD_CTL, 0);
        for _ in 0..1000 {
            if (self.read_stream32(HDA_SD_CTL) & SD_CTL_SRST) == 0 {
                break;
            }
            core::hint::spin_loop();
        }

        // Setup BDL
        let total_size = BUFFER_SIZE * BDL_SIZE;
        let bdl_ptr = self.bdl.as_ptr() as *mut BdlEntry;

        for (i, buffer) in self.buffers.iter().enumerate() {
            unsafe {
                let entry = &mut *bdl_ptr.add(i);
                entry.address = buffer.physical_address();
                entry.length = BUFFER_SIZE as u32;
                entry.ioc = 1; // Interrupt on completion
            }
        }

        // Set BDL address
        let bdl_phys = self.bdl.as_ptr() as u64;
        self.write_stream32(HDA_SD_BDLPL, bdl_phys as u32);
        self.write_stream32(HDA_SD_BDLPU, (bdl_phys >> 32) as u32);

        // Set cyclic buffer length
        self.write_stream32(HDA_SD_CBL, total_size as u32);

        // Set last valid index
        self.write_stream16(HDA_SD_LVI as u8, (BDL_SIZE - 1) as u16);

        // Set format
        self.write_stream16(HDA_SD_FMT, self.format_value());

        // Set stream ID in control register (bits 23:20)
        let stream_id = (self.stream_idx + 1) as u32;
        self.write_stream32(HDA_SD_CTL, stream_id << 20);

        *state = StreamState::Prepared;
        Ok(())
    }

    fn start(&self) -> Result<(), AudioError> {
        let mut state = self.state.lock();

        if *state != StreamState::Prepared && *state != StreamState::Paused {
            return Err(AudioError::InvalidState);
        }

        // Enable stream with interrupts
        let ctl = self.read_stream32(HDA_SD_CTL);
        self.write_stream32(HDA_SD_CTL, ctl | SD_CTL_RUN | SD_CTL_IOCE);

        *state = StreamState::Running;
        Ok(())
    }

    fn stop(&self) -> Result<(), AudioError> {
        let mut state = self.state.lock();

        // Stop stream
        let ctl = self.read_stream32(HDA_SD_CTL);
        self.write_stream32(HDA_SD_CTL, ctl & !SD_CTL_RUN);

        // Reset stream
        self.write_stream32(HDA_SD_CTL, SD_CTL_SRST);

        *state = StreamState::Open;
        Ok(())
    }

    fn pause(&self) -> Result<(), AudioError> {
        let mut state = self.state.lock();

        if *state != StreamState::Running {
            return Err(AudioError::InvalidState);
        }

        let ctl = self.read_stream32(HDA_SD_CTL);
        self.write_stream32(HDA_SD_CTL, ctl & !SD_CTL_RUN);

        *state = StreamState::Paused;
        Ok(())
    }

    fn resume(&self) -> Result<(), AudioError> {
        let mut state = self.state.lock();

        if *state != StreamState::Paused {
            return Err(AudioError::InvalidState);
        }

        let ctl = self.read_stream32(HDA_SD_CTL);
        self.write_stream32(HDA_SD_CTL, ctl | SD_CTL_RUN);

        *state = StreamState::Running;
        Ok(())
    }

    fn write(&self, data: &[u8]) -> Result<usize, AudioError> {
        if self.direction != StreamDirection::Playback {
            return Err(AudioError::InvalidState);
        }

        Ok(self.ring_buffer.write(data))
    }

    fn read(&self, data: &mut [u8]) -> Result<usize, AudioError> {
        if self.direction != StreamDirection::Capture {
            return Err(AudioError::InvalidState);
        }

        Ok(self.ring_buffer.read(data))
    }

    fn available_write(&self) -> usize {
        self.ring_buffer.free()
    }

    fn available_read(&self) -> usize {
        self.ring_buffer.available()
    }

    fn drain(&self) -> Result<(), AudioError> {
        while self.ring_buffer.available() > 0 {
            core::hint::spin_loop();
        }
        Ok(())
    }

    fn delay(&self) -> usize {
        let lpib = self.read_stream32(HDA_SD_LPIB);
        let cbl = self.read_stream32(HDA_SD_CBL);

        if cbl > 0 {
            ((cbl - lpib) as usize) / self.format.bytes_per_frame()
        } else {
            0
        }
    }
}

// =============================================================================
// PCI DETECTION
// =============================================================================

/// Probe for HDA controllers on PCI bus
pub fn probe_and_init() -> Result<(), &'static str> {
    crate::log::info!("HDA: Probing for HD Audio controllers");

    // Would integrate with PCI subsystem to find HDA devices
    // Class 0x04 (Multimedia), Subclass 0x03 (HD Audio)
    // Common vendor/device IDs:
    // - Intel: 8086:xxxx (many models)
    // - AMD: 1002:xxxx
    // - NVIDIA: 10DE:xxxx

    // For now, return error as we need PCI integration
    Err("HDA: No controllers found")
}
