//! AC'97 Audio Codec Driver
//!
//! Implements support for AC'97 compatible audio controllers.

#![allow(dead_code)]

use alloc::string::String;
use alloc::vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use super::{
    CardType, SoundError, StreamDirection,
    sound_core::{SoundCard, SoundDevice, DeviceType, CardCapabilities, SampleRateCapability, FormatCapability, DmaBuffer},
    mixer::{Mixer, MixerElement, MixerControl, ControlType},
};

// =============================================================================
// AC'97 REGISTER DEFINITIONS
// =============================================================================

// Mixer registers (offsets into NAMBAR)
/// Reset Register
pub const AC97_RESET: u16 = 0x00;
/// Master Volume
pub const AC97_MASTER: u16 = 0x02;
/// Aux Out Volume (Headphone)
pub const AC97_AUXOUT: u16 = 0x04;
/// Mono Volume
pub const AC97_MONO: u16 = 0x06;
/// Master Tone Control
pub const AC97_MASTER_TONE: u16 = 0x08;
/// PC Beep Volume
pub const AC97_PC_BEEP: u16 = 0x0A;
/// Phone Volume
pub const AC97_PHONE: u16 = 0x0C;
/// Mic Volume
pub const AC97_MIC: u16 = 0x0E;
/// Line In Volume
pub const AC97_LINE_IN: u16 = 0x10;
/// CD Volume
pub const AC97_CD: u16 = 0x12;
/// Video Volume
pub const AC97_VIDEO: u16 = 0x14;
/// Aux In Volume
pub const AC97_AUX_IN: u16 = 0x16;
/// PCM Out Volume
pub const AC97_PCM_OUT: u16 = 0x18;
/// Record Select
pub const AC97_RECORD_SEL: u16 = 0x1A;
/// Record Gain
pub const AC97_RECORD_GAIN: u16 = 0x1C;
/// Record Gain Mic
pub const AC97_RECORD_GAIN_MIC: u16 = 0x1E;
/// General Purpose
pub const AC97_GENERAL_PURPOSE: u16 = 0x20;
/// 3D Control
pub const AC97_3D_CONTROL: u16 = 0x22;
/// Audio Interrupt/Paging
pub const AC97_INT_PAGING: u16 = 0x24;
/// Powerdown Control/Status
pub const AC97_POWERDOWN: u16 = 0x26;
/// Extended Audio ID
pub const AC97_EXTENDED_ID: u16 = 0x28;
/// Extended Audio Status/Control
pub const AC97_EXTENDED_CTRL: u16 = 0x2A;
/// PCM Front DAC Rate
pub const AC97_PCM_FRONT_DAC_RATE: u16 = 0x2C;
/// PCM Surround DAC Rate
pub const AC97_PCM_SURR_DAC_RATE: u16 = 0x2E;
/// PCM LFE DAC Rate
pub const AC97_PCM_LFE_DAC_RATE: u16 = 0x30;
/// PCM ADC Rate
pub const AC97_PCM_ADC_RATE: u16 = 0x32;
/// MIC ADC Rate
pub const AC97_MIC_ADC_RATE: u16 = 0x34;
/// Center/LFE Volume
pub const AC97_CENTER_LFE: u16 = 0x36;
/// Surround Volume
pub const AC97_SURROUND: u16 = 0x38;
/// S/PDIF Control
pub const AC97_SPDIF_CTRL: u16 = 0x3A;
/// Vendor ID 1
pub const AC97_VENDOR_ID1: u16 = 0x7C;
/// Vendor ID 2
pub const AC97_VENDOR_ID2: u16 = 0x7E;

// Bus master registers (offsets into NABMBAR)
/// PCM In Buffer Descriptor Base Address
pub const AC97_PI_BDBAR: u16 = 0x00;
/// PCM In Current Index Value
pub const AC97_PI_CIV: u16 = 0x04;
/// PCM In Last Valid Index
pub const AC97_PI_LVI: u16 = 0x05;
/// PCM In Status
pub const AC97_PI_SR: u16 = 0x06;
/// PCM In Position in Current Buffer
pub const AC97_PI_PICB: u16 = 0x08;
/// PCM In Prefetch Index Value
pub const AC97_PI_PIV: u16 = 0x0A;
/// PCM In Control
pub const AC97_PI_CR: u16 = 0x0B;

/// PCM Out Buffer Descriptor Base Address
pub const AC97_PO_BDBAR: u16 = 0x10;
/// PCM Out Current Index Value
pub const AC97_PO_CIV: u16 = 0x14;
/// PCM Out Last Valid Index
pub const AC97_PO_LVI: u16 = 0x15;
/// PCM Out Status
pub const AC97_PO_SR: u16 = 0x16;
/// PCM Out Position in Current Buffer
pub const AC97_PO_PICB: u16 = 0x18;
/// PCM Out Prefetch Index Value
pub const AC97_PO_PIV: u16 = 0x1A;
/// PCM Out Control
pub const AC97_PO_CR: u16 = 0x1B;

/// Mic In Buffer Descriptor Base Address
pub const AC97_MC_BDBAR: u16 = 0x20;
/// Mic In Current Index Value
pub const AC97_MC_CIV: u16 = 0x24;
/// Mic In Last Valid Index
pub const AC97_MC_LVI: u16 = 0x25;
/// Mic In Status
pub const AC97_MC_SR: u16 = 0x26;
/// Mic In Position in Current Buffer
pub const AC97_MC_PICB: u16 = 0x28;
/// Mic In Prefetch Index Value
pub const AC97_MC_PIV: u16 = 0x2A;
/// Mic In Control
pub const AC97_MC_CR: u16 = 0x2B;

/// Global Control
pub const AC97_GLOB_CNT: u16 = 0x2C;
/// Global Status
pub const AC97_GLOB_STA: u16 = 0x30;
/// Codec Access Semaphore
pub const AC97_CAS: u16 = 0x34;

// Status register bits
pub const SR_DCH: u16 = 1 << 0;   // DMA Controller Halted
pub const SR_CELV: u16 = 1 << 1;  // Current Equals Last Valid
pub const SR_LVBCI: u16 = 1 << 2; // Last Valid Buffer Completion Interrupt
pub const SR_BCIS: u16 = 1 << 3;  // Buffer Completion Interrupt Status
pub const SR_FIFOE: u16 = 1 << 4; // FIFO Error

// Control register bits
pub const CR_RPBM: u8 = 1 << 0;  // Run/Pause Bus Master
pub const CR_RR: u8 = 1 << 1;    // Reset Registers
pub const CR_LVBIE: u8 = 1 << 2; // Last Valid Buffer Interrupt Enable
pub const CR_FEIE: u8 = 1 << 3;  // FIFO Error Interrupt Enable
pub const CR_IOCE: u8 = 1 << 4;  // Interrupt On Completion Enable

// Global control bits
pub const GC_GIE: u32 = 1 << 0;     // Global Interrupt Enable
pub const GC_COLD_RESET: u32 = 1 << 1; // Cold Reset
pub const GC_WARM_RESET: u32 = 1 << 2; // Warm Reset
pub const GC_LINK_OFF: u32 = 1 << 3;   // AC-link Shut Off
pub const GC_SAMPLE_CAP: u32 = 3 << 20; // Sample Capabilities
pub const GC_20BIT: u32 = 1 << 20;      // 20-bit samples
pub const GC_MULTICHAN: u32 = 3 << 21;  // Multi-channel capability

// Global status bits
pub const GS_MINT: u32 = 1 << 0;   // Modem Interrupt
pub const GS_POINT: u32 = 1 << 1;  // PCM Out Interrupt
pub const GS_PIINT: u32 = 1 << 2;  // PCM In Interrupt
pub const GS_MOINT: u32 = 1 << 5;  // Modem Out Interrupt
pub const GS_MIINT: u32 = 1 << 6;  // Modem In Interrupt
pub const GS_GSCI: u32 = 1 << 7;   // GPI Status Change Interrupt
pub const GS_MD3: u32 = 1 << 16;   // Modem Power Down (audio codec ready)
pub const GS_AD3: u32 = 1 << 17;   // Audio Power Down (audio codec ready)
pub const GS_RCS: u32 = 1 << 18;   // Read Completion Status
pub const GS_S0CR: u32 = 1 << 20;  // Slot 0 Codec Ready
pub const GS_S1CR: u32 = 1 << 21;  // Slot 1 Codec Ready
pub const GS_S2CR: u32 = 1 << 22;  // Slot 2 Codec Ready

// Extended audio capabilities
pub const EXT_VRA: u16 = 1 << 0;   // Variable Rate Audio
pub const EXT_DRA: u16 = 1 << 1;   // Double Rate Audio
pub const EXT_SPDIF: u16 = 1 << 2; // S/PDIF
pub const EXT_VRM: u16 = 1 << 3;   // Variable Rate Mic
pub const EXT_DSA: u16 = 3 << 4;   // DAC Slot Assignment
pub const EXT_CDAC: u16 = 1 << 6;  // Center DAC
pub const EXT_SDAC: u16 = 1 << 7;  // Surround DAC
pub const EXT_LDAC: u16 = 1 << 8;  // LFE DAC
pub const EXT_AMAP: u16 = 1 << 9;  // Slot/DAC Mappings
pub const EXT_REV: u16 = 3 << 10;  // AC'97 Revision

// =============================================================================
// AC'97 STRUCTURES
// =============================================================================

/// Buffer Descriptor List Entry
#[repr(C, packed)]
#[derive(Clone, Copy, Debug, Default)]
pub struct BdlEntry {
    /// Buffer address (physical)
    pub addr: u32,
    /// Number of samples (16-bit samples)
    pub samples: u16,
    /// Control flags
    pub control: u16,
}

/// BDL control flags
pub const BDL_IOC: u16 = 1 << 15;  // Interrupt On Completion
pub const BDL_BUP: u16 = 1 << 14;  // Buffer Underrun Policy

/// AC'97 Codec info
#[derive(Clone, Debug)]
pub struct Ac97Codec {
    /// Vendor ID
    pub vendor_id: u32,
    /// Codec ID string
    pub name: String,
    /// Extended capabilities
    pub ext_caps: u16,
    /// Supports variable rate audio
    pub vra: bool,
    /// Supports S/PDIF
    pub spdif: bool,
    /// Number of channels (2, 4, or 6)
    pub channels: u8,
    /// Sample rate
    pub sample_rate: u32,
}

impl Default for Ac97Codec {
    fn default() -> Self {
        Self {
            vendor_id: 0,
            name: String::from("Unknown AC'97 Codec"),
            ext_caps: 0,
            vra: false,
            spdif: false,
            channels: 2,
            sample_rate: 48000,
        }
    }
}

/// AC'97 Controller
pub struct Ac97Controller {
    /// Mixer I/O base (NAMBAR)
    mixer_base: u16,
    /// Bus Master I/O base (NABMBAR)
    bus_master_base: u16,
    /// MMIO base (if memory mapped)
    mmio_base: Option<usize>,
    /// Codec info
    codec: Ac97Codec,
    /// Output BDL buffer
    output_bdl: Option<DmaBuffer>,
    /// Output audio buffer
    output_buffer: Option<DmaBuffer>,
    /// Input BDL buffer
    input_bdl: Option<DmaBuffer>,
    /// Input audio buffer
    input_buffer: Option<DmaBuffer>,
    /// Is running (playback)
    playing: AtomicBool,
    /// Is running (capture)
    recording: AtomicBool,
    /// Current write position
    write_pos: AtomicU32,
    /// Current read position
    read_pos: AtomicU32,
    /// Buffer size in bytes
    buffer_size: usize,
    /// Period size in bytes
    period_size: usize,
}

impl Ac97Controller {
    /// Create new AC'97 controller
    pub fn new(mixer_base: u16, bus_master_base: u16) -> Self {
        Self {
            mixer_base,
            bus_master_base,
            mmio_base: None,
            codec: Ac97Codec::default(),
            output_bdl: None,
            output_buffer: None,
            input_bdl: None,
            input_buffer: None,
            playing: AtomicBool::new(false),
            recording: AtomicBool::new(false),
            write_pos: AtomicU32::new(0),
            read_pos: AtomicU32::new(0),
            buffer_size: 0,
            period_size: 0,
        }
    }

    /// Initialize controller
    pub fn init(&mut self) -> Result<(), SoundError> {
        // Reset codec
        self.reset()?;

        // Detect codec
        self.detect_codec()?;

        // Initialize mixer to reasonable defaults
        self.init_mixer()?;

        crate::kprintln!("[AC97] Codec: {} (VRA: {})", self.codec.name, self.codec.vra);

        Ok(())
    }

    /// Reset controller and codec
    fn reset(&mut self) -> Result<(), SoundError> {
        // Perform cold reset
        self.write_bus_master32(AC97_GLOB_CNT, GC_COLD_RESET);
        self.delay(100000);

        // Clear reset and wait for codec ready
        self.write_bus_master32(AC97_GLOB_CNT, 0);
        self.delay(100000);

        // Wait for primary codec ready
        for _ in 0..1000 {
            let status = self.read_bus_master32(AC97_GLOB_STA);
            if (status & GS_S0CR) != 0 {
                // Codec is ready, reset the codec itself
                self.write_mixer16(AC97_RESET, 0);
                self.delay(10000);
                return Ok(());
            }
            self.delay(1000);
        }

        Err(SoundError::NoDevice)
    }

    /// Detect codec and capabilities
    fn detect_codec(&mut self) -> Result<(), SoundError> {
        // Read vendor ID
        let vendor1 = self.read_mixer16(AC97_VENDOR_ID1);
        let vendor2 = self.read_mixer16(AC97_VENDOR_ID2);
        self.codec.vendor_id = ((vendor1 as u32) << 16) | (vendor2 as u32);

        // Identify codec
        self.codec.name = match self.codec.vendor_id {
            0x41445303..=0x41445399 => String::from("Analog Devices AD1819/AD1881"),
            0x414B4D00..=0x414B4DFF => String::from("Asahi Kasei AK4540"),
            0x414C4326 => String::from("Realtek ALC650"),
            0x414C4710..=0x414C47FF => String::from("Realtek ALC200"),
            0x43525900..=0x435259FF => String::from("Cirrus Logic CS4297"),
            0x49434500..=0x494345FF => String::from("ICEnsemble ICE1232"),
            0x4E534331 => String::from("National Semiconductor LM4549"),
            0x53494C22 => String::from("Silicon Labs Si3036"),
            0x54524103 => String::from("TriTech TR28023"),
            0x54524106 => String::from("TriTech TR28026"),
            0x54524108 => String::from("TriTech TR28028"),
            0x54524123 => String::from("TriTech TR28602"),
            0x574D4C00..=0x574D4CFF => String::from("Wolfson WM9701A"),
            0x574D4D00..=0x574D4DFF => String::from("Wolfson WM9703/WM9704"),
            0x83847608 => String::from("SigmaTel STAC9708"),
            0x83847609 => String::from("SigmaTel STAC9721"),
            0x83847600..=0x838476FF => String::from("SigmaTel STAC9700"),
            _ => String::from("Unknown AC'97 Codec"),
        };

        // Read extended capabilities
        self.codec.ext_caps = self.read_mixer16(AC97_EXTENDED_ID);
        self.codec.vra = (self.codec.ext_caps & EXT_VRA) != 0;
        self.codec.spdif = (self.codec.ext_caps & EXT_SPDIF) != 0;

        // Determine channel capability
        if (self.codec.ext_caps & (EXT_SDAC | EXT_CDAC | EXT_LDAC)) != 0 {
            self.codec.channels = 6;
        } else {
            self.codec.channels = 2;
        }

        // Enable VRA if supported
        if self.codec.vra {
            let ctrl = self.read_mixer16(AC97_EXTENDED_CTRL);
            self.write_mixer16(AC97_EXTENDED_CTRL, ctrl | EXT_VRA);
        }

        Ok(())
    }

    /// Initialize mixer to defaults
    fn init_mixer(&mut self) -> Result<(), SoundError> {
        // Set master volume to 0dB (unmuted)
        self.write_mixer16(AC97_MASTER, 0x0000);

        // Set PCM out volume to 0dB
        self.write_mixer16(AC97_PCM_OUT, 0x0808);

        // Set headphone volume to 0dB
        self.write_mixer16(AC97_AUXOUT, 0x0000);

        // Set line-in volume
        self.write_mixer16(AC97_LINE_IN, 0x0808);

        // Set mic volume
        self.write_mixer16(AC97_MIC, 0x0008);

        // Set record select to line-in
        self.write_mixer16(AC97_RECORD_SEL, 0x0404);

        // Set record gain
        self.write_mixer16(AC97_RECORD_GAIN, 0x0000);

        Ok(())
    }

    /// Set sample rate
    pub fn set_sample_rate(&mut self, rate: u32) -> Result<u32, SoundError> {
        if !self.codec.vra {
            // Fixed rate codec, can only do 48kHz
            if rate != 48000 {
                return Err(SoundError::RateNotSupported);
            }
            return Ok(48000);
        }

        // Clamp rate to supported range
        let rate = rate.clamp(8000, 48000);

        // Set DAC rate
        self.write_mixer16(AC97_PCM_FRONT_DAC_RATE, rate as u16);

        // Read back actual rate
        let actual = self.read_mixer16(AC97_PCM_FRONT_DAC_RATE) as u32;

        // Set ADC rate
        self.write_mixer16(AC97_PCM_ADC_RATE, rate as u16);

        self.codec.sample_rate = actual;
        Ok(actual)
    }

    /// Setup playback buffer
    pub fn setup_playback(&mut self, buffer_size: usize) -> Result<(), SoundError> {
        // Stop any existing playback
        self.stop_playback()?;

        self.buffer_size = buffer_size;
        self.period_size = buffer_size / 32; // 32 BDL entries

        // Allocate audio buffer
        let buffer_phys = crate::memory::alloc_dma_buffer(buffer_size)
            .ok_or(SoundError::NoMemory)?;
        let buffer_virt = crate::memory::map_mmio(buffer_phys, buffer_size)
            .ok_or(SoundError::NoMemory)?;

        // Clear buffer
        unsafe {
            core::ptr::write_bytes(buffer_virt as *mut u8, 0, buffer_size);
        }

        self.output_buffer = Some(DmaBuffer::new(
            buffer_phys, buffer_virt as usize, buffer_size, self.period_size
        ));

        // Allocate BDL (32 entries)
        let bdl_size = 32 * core::mem::size_of::<BdlEntry>();
        let bdl_phys = crate::memory::alloc_dma_buffer(bdl_size)
            .ok_or(SoundError::NoMemory)?;
        let bdl_virt = crate::memory::map_mmio(bdl_phys, bdl_size)
            .ok_or(SoundError::NoMemory)?;

        // Setup BDL entries
        for i in 0..32 {
            let entry = BdlEntry {
                addr: (buffer_phys + (i * self.period_size) as u64) as u32,
                samples: (self.period_size / 2) as u16, // 16-bit samples
                control: if i == 31 { BDL_IOC } else { 0 },
            };
            unsafe {
                let ptr = (bdl_virt as *mut BdlEntry).add(i);
                core::ptr::write_volatile(ptr, entry);
            }
        }

        self.output_bdl = Some(DmaBuffer::new(bdl_phys, bdl_virt as usize, bdl_size, bdl_size));

        // Set BDL base address
        self.write_bus_master32(AC97_PO_BDBAR, bdl_phys as u32);

        // Set last valid index
        self.write_bus_master8(AC97_PO_LVI, 31);

        Ok(())
    }

    /// Setup capture buffer
    pub fn setup_capture(&mut self, buffer_size: usize) -> Result<(), SoundError> {
        // Stop any existing capture
        self.stop_capture()?;

        let period_size = buffer_size / 32;

        // Allocate audio buffer
        let buffer_phys = crate::memory::alloc_dma_buffer(buffer_size)
            .ok_or(SoundError::NoMemory)?;
        let buffer_virt = crate::memory::map_mmio(buffer_phys, buffer_size)
            .ok_or(SoundError::NoMemory)?;

        // Clear buffer
        unsafe {
            core::ptr::write_bytes(buffer_virt as *mut u8, 0, buffer_size);
        }

        self.input_buffer = Some(DmaBuffer::new(
            buffer_phys, buffer_virt as usize, buffer_size, period_size
        ));

        // Allocate BDL
        let bdl_size = 32 * core::mem::size_of::<BdlEntry>();
        let bdl_phys = crate::memory::alloc_dma_buffer(bdl_size)
            .ok_or(SoundError::NoMemory)?;
        let bdl_virt = crate::memory::map_mmio(bdl_phys, bdl_size)
            .ok_or(SoundError::NoMemory)?;

        // Setup BDL entries
        for i in 0..32 {
            let entry = BdlEntry {
                addr: (buffer_phys + (i * period_size) as u64) as u32,
                samples: (period_size / 2) as u16,
                control: if i == 31 { BDL_IOC } else { 0 },
            };
            unsafe {
                let ptr = (bdl_virt as *mut BdlEntry).add(i);
                core::ptr::write_volatile(ptr, entry);
            }
        }

        self.input_bdl = Some(DmaBuffer::new(bdl_phys, bdl_virt as usize, bdl_size, bdl_size));

        // Set BDL base address
        self.write_bus_master32(AC97_PI_BDBAR, bdl_phys as u32);

        // Set last valid index
        self.write_bus_master8(AC97_PI_LVI, 31);

        Ok(())
    }

    /// Start playback
    pub fn start_playback(&mut self) -> Result<(), SoundError> {
        if self.output_buffer.is_none() {
            return Err(SoundError::NotReady);
        }

        // Clear status
        self.write_bus_master16(AC97_PO_SR, SR_LVBCI | SR_CELV | SR_BCIS | SR_FIFOE);

        // Enable DMA with interrupts
        self.write_bus_master8(AC97_PO_CR, CR_RPBM | CR_IOCE | CR_LVBIE);

        self.playing.store(true, Ordering::Release);
        Ok(())
    }

    /// Stop playback
    pub fn stop_playback(&mut self) -> Result<(), SoundError> {
        // Stop DMA
        self.write_bus_master8(AC97_PO_CR, 0);

        // Reset registers
        self.write_bus_master8(AC97_PO_CR, CR_RR);
        self.delay(1000);
        self.write_bus_master8(AC97_PO_CR, 0);

        self.playing.store(false, Ordering::Release);
        Ok(())
    }

    /// Start capture
    pub fn start_capture(&mut self) -> Result<(), SoundError> {
        if self.input_buffer.is_none() {
            return Err(SoundError::NotReady);
        }

        // Clear status
        self.write_bus_master16(AC97_PI_SR, SR_LVBCI | SR_CELV | SR_BCIS | SR_FIFOE);

        // Enable DMA with interrupts
        self.write_bus_master8(AC97_PI_CR, CR_RPBM | CR_IOCE | CR_LVBIE);

        self.recording.store(true, Ordering::Release);
        Ok(())
    }

    /// Stop capture
    pub fn stop_capture(&mut self) -> Result<(), SoundError> {
        // Stop DMA
        self.write_bus_master8(AC97_PI_CR, 0);

        // Reset registers
        self.write_bus_master8(AC97_PI_CR, CR_RR);
        self.delay(1000);
        self.write_bus_master8(AC97_PI_CR, 0);

        self.recording.store(false, Ordering::Release);
        Ok(())
    }

    /// Write audio data
    pub fn write(&self, data: &[u8]) -> Result<usize, SoundError> {
        let buffer = self.output_buffer.as_ref().ok_or(SoundError::NotReady)?;

        let write_pos = self.write_pos.load(Ordering::Acquire) as usize;
        let available = buffer.available_write() as usize;
        let write_len = data.len().min(available);

        if write_len == 0 {
            return Ok(0);
        }

        // Handle wrap-around
        let first_part = (buffer.size - write_pos).min(write_len);
        unsafe {
            core::ptr::copy_nonoverlapping(
                data.as_ptr(),
                (buffer.virt_addr + write_pos) as *mut u8,
                first_part
            );
            if write_len > first_part {
                core::ptr::copy_nonoverlapping(
                    data.as_ptr().add(first_part),
                    buffer.virt_addr as *mut u8,
                    write_len - first_part
                );
            }
        }

        let new_pos = (write_pos + write_len) % buffer.size;
        self.write_pos.store(new_pos as u32, Ordering::Release);

        Ok(write_len)
    }

    /// Read audio data
    pub fn read(&self, data: &mut [u8]) -> Result<usize, SoundError> {
        let buffer = self.input_buffer.as_ref().ok_or(SoundError::NotReady)?;

        let read_pos = self.read_pos.load(Ordering::Acquire) as usize;
        let available = buffer.available_read() as usize;
        let read_len = data.len().min(available);

        if read_len == 0 {
            return Ok(0);
        }

        // Handle wrap-around
        let first_part = (buffer.size - read_pos).min(read_len);
        unsafe {
            core::ptr::copy_nonoverlapping(
                (buffer.virt_addr + read_pos) as *const u8,
                data.as_mut_ptr(),
                first_part
            );
            if read_len > first_part {
                core::ptr::copy_nonoverlapping(
                    buffer.virt_addr as *const u8,
                    data.as_mut_ptr().add(first_part),
                    read_len - first_part
                );
            }
        }

        let new_pos = (read_pos + read_len) % buffer.size;
        self.read_pos.store(new_pos as u32, Ordering::Release);

        Ok(read_len)
    }

    /// Get playback position
    pub fn get_playback_position(&self) -> u32 {
        // Current index value (which BDL entry we're at)
        let civ = self.read_bus_master8(AC97_PO_CIV) as u32;
        // Position in current buffer (samples remaining)
        let picb = self.read_bus_master16(AC97_PO_PICB) as u32;

        // Calculate byte position
        let period_samples = self.period_size / 2; // 16-bit samples
        let played_samples = civ * period_samples as u32 + (period_samples as u32 - picb);
        played_samples * 2 // Convert samples to bytes
    }

    /// Get capture position
    pub fn get_capture_position(&self) -> u32 {
        let civ = self.read_bus_master8(AC97_PI_CIV) as u32;
        let picb = self.read_bus_master16(AC97_PI_PICB) as u32;
        let period_samples = self.period_size / 2;
        let captured_samples = civ * period_samples as u32 + (period_samples as u32 - picb);
        captured_samples * 2
    }

    /// Get/Set master volume (0-100)
    pub fn get_master_volume(&self) -> u8 {
        let reg = self.read_mixer16(AC97_MASTER);
        if (reg & 0x8000) != 0 {
            return 0; // Muted
        }
        // AC'97 uses attenuation, convert to percentage
        let atten = reg & 0x3F;
        100 - ((atten * 100) / 63) as u8
    }

    pub fn set_master_volume(&mut self, percent: u8) {
        let percent = percent.min(100);
        if percent == 0 {
            // Mute
            self.write_mixer16(AC97_MASTER, 0x8000);
        } else {
            // Calculate attenuation
            let atten = ((100 - percent as u16) * 63) / 100;
            self.write_mixer16(AC97_MASTER, atten | (atten << 8));
        }
    }

    /// Get/Set PCM volume
    pub fn get_pcm_volume(&self) -> u8 {
        let reg = self.read_mixer16(AC97_PCM_OUT);
        if (reg & 0x8000) != 0 {
            return 0;
        }
        let atten = reg & 0x1F;
        100 - ((atten * 100) / 31) as u8
    }

    pub fn set_pcm_volume(&mut self, percent: u8) {
        let percent = percent.min(100);
        if percent == 0 {
            self.write_mixer16(AC97_PCM_OUT, 0x8000);
        } else {
            let atten = ((100 - percent as u16) * 31) / 100;
            self.write_mixer16(AC97_PCM_OUT, atten | (atten << 8));
        }
    }

    /// Handle interrupt
    pub fn handle_interrupt(&mut self) -> bool {
        let status = self.read_bus_master32(AC97_GLOB_STA);
        let mut handled = false;

        // PCM Out interrupt
        if (status & GS_POINT) != 0 {
            let po_sr = self.read_bus_master16(AC97_PO_SR);
            // Clear status
            self.write_bus_master16(AC97_PO_SR, po_sr);

            if (po_sr & SR_LVBCI) != 0 {
                // Wrap around - update LVI to keep playing
                self.write_bus_master8(AC97_PO_LVI, 31);
            }

            handled = true;
        }

        // PCM In interrupt
        if (status & GS_PIINT) != 0 {
            let pi_sr = self.read_bus_master16(AC97_PI_SR);
            self.write_bus_master16(AC97_PI_SR, pi_sr);

            if (pi_sr & SR_LVBCI) != 0 {
                self.write_bus_master8(AC97_PI_LVI, 31);
            }

            handled = true;
        }

        handled
    }

    // I/O helpers
    fn read_mixer16(&self, reg: u16) -> u16 {
        unsafe {
            let port = self.mixer_base + reg;
            let mut val: u16;
            core::arch::asm!("in ax, dx", out("ax") val, in("dx") port);
            val
        }
    }

    fn write_mixer16(&self, reg: u16, val: u16) {
        unsafe {
            let port = self.mixer_base + reg;
            core::arch::asm!("out dx, ax", in("dx") port, in("ax") val);
        }
    }

    fn read_bus_master8(&self, reg: u16) -> u8 {
        unsafe {
            let port = self.bus_master_base + reg;
            let mut val: u8;
            core::arch::asm!("in al, dx", out("al") val, in("dx") port);
            val
        }
    }

    fn write_bus_master8(&self, reg: u16, val: u8) {
        unsafe {
            let port = self.bus_master_base + reg;
            core::arch::asm!("out dx, al", in("dx") port, in("al") val);
        }
    }

    fn read_bus_master16(&self, reg: u16) -> u16 {
        unsafe {
            let port = self.bus_master_base + reg;
            let mut val: u16;
            core::arch::asm!("in ax, dx", out("ax") val, in("dx") port);
            val
        }
    }

    fn write_bus_master16(&self, reg: u16, val: u16) {
        unsafe {
            let port = self.bus_master_base + reg;
            core::arch::asm!("out dx, ax", in("dx") port, in("ax") val);
        }
    }

    fn read_bus_master32(&self, reg: u16) -> u32 {
        unsafe {
            let port = self.bus_master_base + reg;
            let mut val: u32;
            core::arch::asm!("in eax, dx", out("eax") val, in("dx") port);
            val
        }
    }

    fn write_bus_master32(&self, reg: u16, val: u32) {
        unsafe {
            let port = self.bus_master_base + reg;
            core::arch::asm!("out dx, eax", in("dx") port, in("eax") val);
        }
    }

    fn delay(&self, us: u64) {
        crate::time::delay_us(us);
    }
}

/// Initialize AC'97 controller
pub fn init_controller(bus: u8, slot: u8, func: u8) -> Option<SoundCard> {
    // Get BARs
    let mixer_bar = crate::drivers::pci::read_bar(bus, slot, func, 0)? as u16;
    let bus_master_bar = crate::drivers::pci::read_bar(bus, slot, func, 1)? as u16;

    // Enable bus mastering and I/O space
    crate::drivers::pci::enable_bus_master(bus, slot, func);
    crate::drivers::pci::enable_io_space(bus, slot, func);

    // Create controller
    let mut controller = Ac97Controller::new(mixer_bar, bus_master_bar);

    // Initialize
    if controller.init().is_err() {
        return None;
    }

    // Create sound card
    let mut card = SoundCard::new(
        CardType::Ac97,
        "snd_intel8x0",
        &controller.codec.name
    );

    // Add playback device
    let mut playback_device = SoundDevice::new(
        0,
        String::from("AC'97 Playback"),
        DeviceType::Pcm,
        StreamDirection::Playback,
        1
    );

    let mut rates = SampleRateCapability::default();
    if controller.codec.vra {
        rates = SampleRateCapability::continuous(8000, 48000);
    } else {
        rates.min = 48000;
        rates.max = 48000;
        rates.discrete_rates[0] = 48000;
        rates.discrete_count = 1;
    }

    playback_device.capabilities = CardCapabilities {
        playback: true,
        capture: false,
        full_duplex: true,
        hardware_mixing: false,
        hardware_volume: true,
        max_playback_channels: controller.codec.channels as u32,
        max_capture_channels: 0,
        sample_rates: rates.clone(),
        formats: FormatCapability::new(FormatCapability::S16_LE),
    };
    card.add_device(playback_device);

    // Add capture device
    let mut capture_device = SoundDevice::new(
        1,
        String::from("AC'97 Capture"),
        DeviceType::Pcm,
        StreamDirection::Capture,
        1
    );
    capture_device.capabilities = CardCapabilities {
        playback: false,
        capture: true,
        full_duplex: true,
        hardware_mixing: false,
        hardware_volume: true,
        max_playback_channels: 0,
        max_capture_channels: 2,
        sample_rates: rates,
        formats: FormatCapability::new(FormatCapability::S16_LE),
    };
    card.add_device(capture_device);

    // Create mixer
    let mixer = Mixer::new("AC'97 Mixer", card.id);

    // Add master volume control
    let mut master_vol = MixerControl::new("Master", 0, ControlType::Integer, 2);
    master_vol.set_range(0, 63);
    let mut master_elem = MixerElement::new("Master", 0);
    master_elem.add_control(master_vol);
    mixer.add_element(master_elem);

    // Add PCM volume control
    let mut pcm_vol = MixerControl::new("PCM", 1, ControlType::Integer, 2);
    pcm_vol.set_range(0, 31);
    let mut pcm_elem = MixerElement::new("PCM", 1);
    pcm_elem.add_control(pcm_vol);
    mixer.add_element(pcm_elem);

    // Add line-in volume
    let mut line_vol = MixerControl::new("Line", 2, ControlType::Integer, 2);
    line_vol.set_range(0, 31);
    let mut line_elem = MixerElement::new("Line", 2);
    line_elem.add_control(line_vol);
    mixer.add_element(line_elem);

    // Add mic volume
    let mut mic_vol = MixerControl::new("Mic", 3, ControlType::Integer, 2);
    mic_vol.set_range(0, 31);
    let mut mic_elem = MixerElement::new("Mic", 3);
    mic_elem.add_control(mic_vol);
    mixer.add_element(mic_elem);

    // Add capture source selector
    let capture_src = MixerControl::new_enumerated(
        "Capture Source", 4,
        vec![
            String::from("Mic"),
            String::from("CD"),
            String::from("Video"),
            String::from("Aux"),
            String::from("Line"),
            String::from("Mix"),
            String::from("Mix Mono"),
            String::from("Phone"),
        ],
    );
    let mut capture_elem = MixerElement::new("Capture Source", 4);
    capture_elem.add_control(capture_src);
    mixer.add_element(capture_elem);

    card.set_mixer(mixer);

    // Store controller
    card.set_private_data(controller);

    Some(card)
}
