// ===============================================================================
// BUILDOS KERNEL - AC'97 AUDIO DRIVER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================
//
// Audio Codec '97 (AC'97) driver for legacy audio chipsets.
// Supports Intel ICH series and compatible AC97 controllers.
//
// ===============================================================================

#![allow(dead_code)]

use alloc::boxed::Box;
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
// AC97 REGISTER DEFINITIONS
// =============================================================================

// Native Audio Mixer (NAM) Registers - accessed via NAMBAR
const AC97_RESET: u16 = 0x00;
const AC97_MASTER_VOL: u16 = 0x02;
const AC97_AUX_OUT_VOL: u16 = 0x04;
const AC97_MONO_VOL: u16 = 0x06;
const AC97_MASTER_TONE: u16 = 0x08;
const AC97_PC_BEEP_VOL: u16 = 0x0A;
const AC97_PHONE_VOL: u16 = 0x0C;
const AC97_MIC_VOL: u16 = 0x0E;
const AC97_LINE_IN_VOL: u16 = 0x10;
const AC97_CD_VOL: u16 = 0x12;
const AC97_VIDEO_VOL: u16 = 0x14;
const AC97_AUX_IN_VOL: u16 = 0x16;
const AC97_PCM_OUT_VOL: u16 = 0x18;
const AC97_RECORD_SELECT: u16 = 0x1A;
const AC97_RECORD_GAIN: u16 = 0x1C;
const AC97_RECORD_GAIN_MIC: u16 = 0x1E;
const AC97_GENERAL_PURPOSE: u16 = 0x20;
const AC97_3D_CONTROL: u16 = 0x22;
const AC97_AUDIO_INT_PAGING: u16 = 0x24;
const AC97_POWERDOWN: u16 = 0x26;
const AC97_EXT_AUDIO_ID: u16 = 0x28;
const AC97_EXT_AUDIO_CTRL: u16 = 0x2A;
const AC97_PCM_FRONT_DAC: u16 = 0x2C;
const AC97_PCM_SURROUND_DAC: u16 = 0x2E;
const AC97_PCM_LFE_DAC: u16 = 0x30;
const AC97_PCM_LR_ADC: u16 = 0x32;
const AC97_MIC_ADC: u16 = 0x34;
const AC97_VENDOR_ID1: u16 = 0x7C;
const AC97_VENDOR_ID2: u16 = 0x7E;

// Native Audio Bus Master (NABM) Registers - accessed via NABMBAR
const AC97_PI_BDBAR: u8 = 0x00;   // PCM In Buffer Descriptor Base Address
const AC97_PI_CIV: u8 = 0x04;     // PCM In Current Index Value
const AC97_PI_LVI: u8 = 0x05;     // PCM In Last Valid Index
const AC97_PI_SR: u8 = 0x06;      // PCM In Status
const AC97_PI_PICB: u8 = 0x08;    // PCM In Position in Current Buffer
const AC97_PI_PIV: u8 = 0x0A;     // PCM In Prefetched Index Value
const AC97_PI_CR: u8 = 0x0B;      // PCM In Control

const AC97_PO_BDBAR: u8 = 0x10;   // PCM Out Buffer Descriptor Base Address
const AC97_PO_CIV: u8 = 0x14;     // PCM Out Current Index Value
const AC97_PO_LVI: u8 = 0x15;     // PCM Out Last Valid Index
const AC97_PO_SR: u8 = 0x16;      // PCM Out Status
const AC97_PO_PICB: u8 = 0x18;    // PCM Out Position in Current Buffer
const AC97_PO_PIV: u8 = 0x1A;     // PCM Out Prefetched Index Value
const AC97_PO_CR: u8 = 0x1B;      // PCM Out Control

const AC97_MC_BDBAR: u8 = 0x20;   // Mic In Buffer Descriptor Base Address
const AC97_MC_CIV: u8 = 0x24;     // Mic In Current Index Value
const AC97_MC_LVI: u8 = 0x25;     // Mic In Last Valid Index
const AC97_MC_SR: u8 = 0x26;      // Mic In Status
const AC97_MC_PICB: u8 = 0x28;    // Mic In Position in Current Buffer
const AC97_MC_PIV: u8 = 0x2A;     // Mic In Prefetched Index Value
const AC97_MC_CR: u8 = 0x2B;      // Mic In Control

const AC97_GLOB_CNT: u8 = 0x2C;   // Global Control
const AC97_GLOB_STA: u8 = 0x30;   // Global Status

// Status Register bits
const SR_DCH: u16 = 1 << 0;       // DMA Controller Halted
const SR_CELV: u16 = 1 << 1;      // Current Equals Last Valid
const SR_LVBCI: u16 = 1 << 2;     // Last Valid Buffer Completion Interrupt
const SR_BCIS: u16 = 1 << 3;      // Buffer Completion Interrupt Status
const SR_FIFOE: u16 = 1 << 4;     // FIFO Error

// Control Register bits
const CR_RPBM: u8 = 1 << 0;       // Run/Pause Bus Master
const CR_RR: u8 = 1 << 1;         // Reset Registers
const CR_LVBIE: u8 = 1 << 2;      // Last Valid Buffer Interrupt Enable
const CR_FEIE: u8 = 1 << 3;       // FIFO Error Interrupt Enable
const CR_IOCE: u8 = 1 << 4;       // Interrupt on Completion Enable

// Global Control bits
const GC_GIE: u32 = 1 << 0;       // GPI Interrupt Enable
const GC_COLD: u32 = 1 << 1;      // Cold Reset
const GC_WARM: u32 = 1 << 2;      // Warm Reset
const GC_SHUT: u32 = 1 << 3;      // Shut Off
const GC_2CH: u32 = 0 << 20;      // 2 Channels
const GC_4CH: u32 = 1 << 20;      // 4 Channels
const GC_6CH: u32 = 2 << 20;      // 6 Channels

// Global Status bits
const GS_MD3: u32 = 1 << 0;       // Modem Power Down
const GS_AD3: u32 = 1 << 1;       // Audio Power Down
const GS_RCS: u32 = 1 << 2;       // Read Completion Status
const GS_B3S12: u32 = 1 << 3;     // Bit 3 Slot 12
const GS_B4S12: u32 = 1 << 4;     // Bit 4 Slot 12
const GS_B5S12: u32 = 1 << 5;     // Bit 5 Slot 12
const GS_S1RI: u32 = 1 << 6;      // Secondary Resume Interrupt
const GS_S0RI: u32 = 1 << 7;      // Primary Resume Interrupt
const GS_S1CR: u32 = 1 << 8;      // Secondary Codec Ready
const GS_S0CR: u32 = 1 << 9;      // Primary Codec Ready
const GS_MINT: u32 = 1 << 10;     // Mic In Interrupt
const GS_POINT: u32 = 1 << 11;    // PCM Out Interrupt
const GS_PIINT: u32 = 1 << 12;    // PCM In Interrupt

// =============================================================================
// BUFFER DESCRIPTOR
// =============================================================================

/// Buffer Descriptor List Entry
#[derive(Clone, Copy, Default)]
#[repr(C, packed)]
pub struct BufferDescriptor {
    /// Physical address of buffer
    pub address: u32,
    /// Buffer length in samples (16-bit samples)
    pub length: u16,
    /// Control flags
    pub flags: u16,
}

const BD_IOC: u16 = 1 << 15;  // Interrupt on Completion
const BD_BUP: u16 = 1 << 14;  // Buffer Underrun Policy (0=last valid, 1=zero)

const NUM_DESCRIPTORS: usize = 32;
const BUFFER_SIZE: usize = 0x10000; // 64KB per buffer

// =============================================================================
// AC97 CONTROLLER
// =============================================================================

/// AC97 Controller
pub struct Ac97Controller {
    /// Native Audio Mixer BAR (I/O or MMIO)
    nambar: u32,
    /// Native Audio Bus Master BAR (I/O)
    nabmbar: u32,
    /// Is MMIO (vs I/O ports)
    is_mmio: bool,
    /// Vendor ID
    vendor_id: u32,
    /// Volume (0-100)
    volume: AtomicU8,
    /// Muted
    muted: AtomicBool,
    /// Extended audio capabilities
    ext_caps: u16,
    /// Supported sample rates
    sample_rates: Vec<u32>,
    /// Playback buffer descriptors
    playback_bd: Box<[BufferDescriptor; NUM_DESCRIPTORS]>,
    /// Playback buffers
    playback_buffers: [Option<AudioBuffer>; NUM_DESCRIPTORS],
    /// Capture buffer descriptors
    capture_bd: Box<[BufferDescriptor; NUM_DESCRIPTORS]>,
    /// Capture buffers
    capture_buffers: [Option<AudioBuffer>; NUM_DESCRIPTORS],
    /// Playback state
    playback_running: AtomicBool,
    /// Capture state
    capture_running: AtomicBool,
    /// Current playback sample rate
    playback_rate: AtomicU32,
    /// Current capture sample rate
    capture_rate: AtomicU32,
}

impl Ac97Controller {
    /// Create new AC97 controller
    pub fn new(nambar: u32, nabmbar: u32, is_mmio: bool) -> Option<Self> {
        crate::log::info!("AC97: Initializing controller NAM={:08x} NABM={:08x}", nambar, nabmbar);

        let mut controller = Self {
            nambar,
            nabmbar,
            is_mmio,
            vendor_id: 0,
            volume: AtomicU8::new(100),
            muted: AtomicBool::new(false),
            ext_caps: 0,
            sample_rates: vec![48000], // Default to 48kHz
            playback_bd: Box::new([BufferDescriptor::default(); NUM_DESCRIPTORS]),
            playback_buffers: Default::default(),
            capture_bd: Box::new([BufferDescriptor::default(); NUM_DESCRIPTORS]),
            capture_buffers: Default::default(),
            playback_running: AtomicBool::new(false),
            capture_running: AtomicBool::new(false),
            playback_rate: AtomicU32::new(48000),
            capture_rate: AtomicU32::new(48000),
        };

        if !controller.reset() {
            crate::log::error!("AC97: Reset failed");
            return None;
        }

        // Read vendor ID
        let id1 = controller.read_mixer(AC97_VENDOR_ID1);
        let id2 = controller.read_mixer(AC97_VENDOR_ID2);
        controller.vendor_id = ((id1 as u32) << 16) | (id2 as u32);

        crate::log::info!("AC97: Codec vendor ID: {:08x}", controller.vendor_id);

        // Read extended audio capabilities
        controller.ext_caps = controller.read_mixer(AC97_EXT_AUDIO_ID);
        crate::log::debug!("AC97: Extended capabilities: {:04x}", controller.ext_caps);

        // Check for variable rate audio support
        if (controller.ext_caps & 0x0001) != 0 {
            crate::log::info!("AC97: Variable rate audio supported");
            controller.sample_rates = vec![8000, 11025, 16000, 22050, 32000, 44100, 48000];

            // Enable VRA
            let ctrl = controller.read_mixer(AC97_EXT_AUDIO_CTRL);
            controller.write_mixer(AC97_EXT_AUDIO_CTRL, ctrl | 0x0001);
        }

        // Initialize volumes
        controller.write_mixer(AC97_MASTER_VOL, 0x0000);  // Max volume
        controller.write_mixer(AC97_PCM_OUT_VOL, 0x0808);  // Mid volume

        // Setup buffer descriptors
        controller.setup_buffers();

        Some(controller)
    }

    /// Reset the controller
    fn reset(&self) -> bool {
        // Cold reset
        self.write_nabm32(AC97_GLOB_CNT, GC_COLD);

        // Wait for codec ready
        for _ in 0..1000 {
            let status = self.read_nabm32(AC97_GLOB_STA);
            if (status & GS_S0CR) != 0 {
                crate::log::debug!("AC97: Primary codec ready");

                // Reset codec
                self.write_mixer(AC97_RESET, 0);

                // Wait a bit
                for _ in 0..10000 { core::hint::spin_loop(); }

                return true;
            }
            for _ in 0..10000 { core::hint::spin_loop(); }
        }

        crate::log::error!("AC97: Timeout waiting for codec ready");
        false
    }

    /// Setup DMA buffers
    fn setup_buffers(&mut self) {
        // Setup playback buffer descriptors
        let bd_phys = self.playback_bd.as_ptr() as u32;
        self.write_nabm32(AC97_PO_BDBAR as u8, bd_phys);

        // Setup capture buffer descriptors
        let bd_phys = self.capture_bd.as_ptr() as u32;
        self.write_nabm32(AC97_PI_BDBAR as u8, bd_phys);

        crate::log::debug!("AC97: Buffer descriptors initialized");
    }

    /// Read mixer register
    fn read_mixer(&self, reg: u16) -> u16 {
        if self.is_mmio {
            unsafe { read_volatile((self.nambar as u64 + reg as u64) as *const u16) }
        } else {
            // I/O port access
            let port = (self.nambar + reg as u32) as u16;
            unsafe {
                let value: u16;
                core::arch::asm!(
                    "in ax, dx",
                    out("ax") value,
                    in("dx") port,
                    options(nomem, nostack)
                );
                value
            }
        }
    }

    /// Write mixer register
    fn write_mixer(&self, reg: u16, value: u16) {
        if self.is_mmio {
            unsafe { write_volatile((self.nambar as u64 + reg as u64) as *mut u16, value) }
        } else {
            let port = (self.nambar + reg as u32) as u16;
            unsafe {
                core::arch::asm!(
                    "out dx, ax",
                    in("dx") port,
                    in("ax") value,
                    options(nomem, nostack)
                );
            }
        }
    }

    /// Read NABM register (8-bit)
    fn read_nabm8(&self, reg: u8) -> u8 {
        let port = (self.nabmbar + reg as u32) as u16;
        unsafe {
            let value: u8;
            core::arch::asm!(
                "in al, dx",
                out("al") value,
                in("dx") port,
                options(nomem, nostack)
            );
            value
        }
    }

    /// Write NABM register (8-bit)
    fn write_nabm8(&self, reg: u8, value: u8) {
        let port = (self.nabmbar + reg as u32) as u16;
        unsafe {
            core::arch::asm!(
                "out dx, al",
                in("dx") port,
                in("al") value,
                options(nomem, nostack)
            );
        }
    }

    /// Read NABM register (16-bit)
    fn read_nabm16(&self, reg: u8) -> u16 {
        let port = (self.nabmbar + reg as u32) as u16;
        unsafe {
            let value: u16;
            core::arch::asm!(
                "in ax, dx",
                out("ax") value,
                in("dx") port,
                options(nomem, nostack)
            );
            value
        }
    }

    /// Write NABM register (16-bit)
    fn write_nabm16(&self, reg: u8, value: u16) {
        let port = (self.nabmbar + reg as u32) as u16;
        unsafe {
            core::arch::asm!(
                "out dx, ax",
                in("dx") port,
                in("ax") value,
                options(nomem, nostack)
            );
        }
    }

    /// Read NABM register (32-bit)
    fn read_nabm32(&self, reg: u8) -> u32 {
        let port = (self.nabmbar + reg as u32) as u16;
        unsafe {
            let value: u32;
            core::arch::asm!(
                "in eax, dx",
                out("eax") value,
                in("dx") port,
                options(nomem, nostack)
            );
            value
        }
    }

    /// Write NABM register (32-bit)
    fn write_nabm32(&self, reg: u8, value: u32) {
        let port = (self.nabmbar + reg as u32) as u16;
        unsafe {
            core::arch::asm!(
                "out dx, eax",
                in("dx") port,
                in("eax") value,
                options(nomem, nostack)
            );
        }
    }

    /// Set sample rate
    fn set_sample_rate(&self, direction: StreamDirection, rate: u32) -> bool {
        if !self.sample_rates.contains(&rate) {
            // Find closest supported rate
            let closest = self.sample_rates.iter()
                .min_by_key(|&&r| (r as i32 - rate as i32).abs())
                .copied()
                .unwrap_or(48000);

            crate::log::warn!("AC97: Rate {} not supported, using {}", rate, closest);
        }

        match direction {
            StreamDirection::Playback => {
                self.write_mixer(AC97_PCM_FRONT_DAC, rate as u16);
                self.playback_rate.store(rate, Ordering::Release);
            }
            StreamDirection::Capture => {
                self.write_mixer(AC97_PCM_LR_ADC, rate as u16);
                self.capture_rate.store(rate, Ordering::Release);
            }
        }

        true
    }

    /// Set volume (0-100)
    fn set_volume_internal(&self, volume: u8) {
        // AC97 volume is 6-bit attenuation (0x3F = mute, 0x00 = max)
        let attenuation = ((100 - volume.min(100)) as u16 * 0x3F) / 100;
        let stereo = attenuation | (attenuation << 8);

        self.write_mixer(AC97_MASTER_VOL, stereo);
        self.volume.store(volume, Ordering::Release);
    }

    /// Handle interrupt
    pub fn handle_interrupt(&self) {
        let status = self.read_nabm32(AC97_GLOB_STA);

        if (status & GS_POINT) != 0 {
            // PCM Out interrupt
            let po_sr = self.read_nabm16(AC97_PO_SR);

            if (po_sr & SR_BCIS) != 0 {
                // Buffer completed
                crate::log::trace!("AC97: PCM Out buffer complete");
            }

            if (po_sr & SR_LVBCI) != 0 {
                // Last valid buffer
                crate::log::trace!("AC97: PCM Out last buffer");
            }

            if (po_sr & SR_FIFOE) != 0 {
                crate::log::warn!("AC97: PCM Out FIFO error");
            }

            // Clear status
            self.write_nabm16(AC97_PO_SR, po_sr);
        }

        if (status & GS_PIINT) != 0 {
            // PCM In interrupt
            let pi_sr = self.read_nabm16(AC97_PI_SR);
            self.write_nabm16(AC97_PI_SR, pi_sr);
        }
    }
}

// =============================================================================
// AUDIO DEVICE IMPLEMENTATION
// =============================================================================

impl AudioDevice for Ac97Controller {
    fn info(&self) -> AudioDeviceInfo {
        AudioDeviceInfo {
            name: "AC97 Audio".to_string(),
            sample_rates: self.sample_rates.clone(),
            sample_formats: vec![SampleFormat::S16Le],
            min_channels: 2,
            max_channels: 2,
            playback: true,
            capture: true,
        }
    }

    fn open(
        &self,
        direction: StreamDirection,
        format: AudioFormat,
    ) -> Result<Box<dyn AudioStream>, AudioError> {
        // Validate format
        if format.sample_format != SampleFormat::S16Le {
            return Err(AudioError::FormatNotSupported);
        }

        if format.channels != 2 {
            return Err(AudioError::FormatNotSupported);
        }

        // Set sample rate
        self.set_sample_rate(direction, format.sample_rate);

        let stream = Ac97Stream::new(
            self.nabmbar,
            direction,
            format,
        );

        Ok(Box::new(stream))
    }

    fn volume(&self) -> u8 {
        self.volume.load(Ordering::Acquire)
    }

    fn set_volume(&self, volume: u8) {
        self.set_volume_internal(volume);
    }

    fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Acquire)
    }

    fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Release);

        if muted {
            self.write_mixer(AC97_MASTER_VOL, 0x8000); // Mute bit
        } else {
            self.set_volume_internal(self.volume());
        }
    }
}

// =============================================================================
// AC97 STREAM
// =============================================================================

/// AC97 Audio Stream
pub struct Ac97Stream {
    nabmbar: u32,
    direction: StreamDirection,
    format: AudioFormat,
    state: Mutex<StreamState>,
    ring_buffer: AudioRingBuffer,
    descriptors: Box<[BufferDescriptor; NUM_DESCRIPTORS]>,
    buffers: [AudioBuffer; NUM_DESCRIPTORS],
    current_descriptor: AtomicU32,
}

impl Ac97Stream {
    fn new(nabmbar: u32, direction: StreamDirection, format: AudioFormat) -> Self {
        let ring_buffer = AudioRingBuffer::new(BUFFER_SIZE * 4);

        let descriptors = Box::new([BufferDescriptor::default(); NUM_DESCRIPTORS]);
        let buffers = core::array::from_fn(|_| AudioBuffer::new(BUFFER_SIZE));

        Self {
            nabmbar,
            direction,
            format,
            state: Mutex::new(StreamState::Open),
            ring_buffer,
            descriptors,
            buffers,
            current_descriptor: AtomicU32::new(0),
        }
    }

    fn base_reg(&self) -> u8 {
        match self.direction {
            StreamDirection::Playback => AC97_PO_BDBAR,
            StreamDirection::Capture => AC97_PI_BDBAR,
        }
    }

    fn write_reg8(&self, offset: u8, value: u8) {
        let port = (self.nabmbar + self.base_reg() as u32 + offset as u32) as u16;
        unsafe {
            core::arch::asm!(
                "out dx, al",
                in("dx") port,
                in("al") value,
                options(nomem, nostack)
            );
        }
    }

    fn read_reg8(&self, offset: u8) -> u8 {
        let port = (self.nabmbar + self.base_reg() as u32 + offset as u32) as u16;
        unsafe {
            let value: u8;
            core::arch::asm!(
                "in al, dx",
                out("al") value,
                in("dx") port,
                options(nomem, nostack)
            );
            value
        }
    }

    fn write_reg32(&self, offset: u8, value: u32) {
        let port = (self.nabmbar + self.base_reg() as u32 + offset as u32) as u16;
        unsafe {
            core::arch::asm!(
                "out dx, eax",
                in("dx") port,
                in("eax") value,
                options(nomem, nostack)
            );
        }
    }
}

impl AudioStream for Ac97Stream {
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

        // Reset DMA
        self.write_reg8(0x0B, CR_RR);

        // Wait for reset
        for _ in 0..1000 {
            if (self.read_reg8(0x0B) & CR_RR) == 0 {
                break;
            }
            core::hint::spin_loop();
        }

        // Setup buffer descriptors
        let bd_phys = self.descriptors.as_ptr() as u32;
        self.write_reg32(0x00, bd_phys);

        // Initialize descriptors
        for (i, buffer) in self.buffers.iter().enumerate() {
            let desc = &mut unsafe { &mut *(self.descriptors.as_ptr() as *mut [BufferDescriptor; NUM_DESCRIPTORS]) }[i];
            desc.address = buffer.physical_address() as u32;
            desc.length = (BUFFER_SIZE / 2) as u16; // Length in samples
            desc.flags = BD_IOC;
        }

        *state = StreamState::Prepared;
        Ok(())
    }

    fn start(&self) -> Result<(), AudioError> {
        let mut state = self.state.lock();

        if *state != StreamState::Prepared && *state != StreamState::Paused {
            return Err(AudioError::InvalidState);
        }

        // Set last valid index
        self.write_reg8(0x05, (NUM_DESCRIPTORS - 1) as u8);

        // Enable DMA with interrupts
        self.write_reg8(0x0B, CR_RPBM | CR_IOCE | CR_LVBIE);

        *state = StreamState::Running;
        Ok(())
    }

    fn stop(&self) -> Result<(), AudioError> {
        let mut state = self.state.lock();

        // Stop DMA
        self.write_reg8(0x0B, 0);

        // Reset
        self.write_reg8(0x0B, CR_RR);

        *state = StreamState::Open;
        Ok(())
    }

    fn pause(&self) -> Result<(), AudioError> {
        let mut state = self.state.lock();

        if *state != StreamState::Running {
            return Err(AudioError::InvalidState);
        }

        // Stop DMA but don't reset
        let cr = self.read_reg8(0x0B);
        self.write_reg8(0x0B, cr & !CR_RPBM);

        *state = StreamState::Paused;
        Ok(())
    }

    fn resume(&self) -> Result<(), AudioError> {
        let mut state = self.state.lock();

        if *state != StreamState::Paused {
            return Err(AudioError::InvalidState);
        }

        let cr = self.read_reg8(0x0B);
        self.write_reg8(0x0B, cr | CR_RPBM);

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
        // Wait for ring buffer to empty
        while self.ring_buffer.available() > 0 {
            core::hint::spin_loop();
        }
        Ok(())
    }

    fn delay(&self) -> usize {
        self.ring_buffer.available() / self.format.bytes_per_frame()
    }
}

// =============================================================================
// PCI DETECTION
// =============================================================================

/// Probe for AC97 controllers on PCI bus
pub fn probe_and_init() -> Result<(), &'static str> {
    crate::log::info!("AC97: Probing for AC97 controllers");

    // Would integrate with PCI subsystem to find AC97 devices
    // Class 0x04 (Multimedia), Subclass 0x01 (Audio)
    // Common vendor/device IDs:
    // - Intel ICH: 8086:2415, 8086:2425, 8086:2445, 8086:24C5, etc.
    // - VIA: 1106:3058, 1106:3059
    // - SiS: 1039:7012

    // For now, return error as we need PCI integration
    Err("AC97: No controllers found")
}
