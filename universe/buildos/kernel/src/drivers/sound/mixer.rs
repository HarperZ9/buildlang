// ===============================================================================
// BUILDOS KERNEL - AUDIO MIXER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================
//
// Software audio mixer for combining multiple audio streams.
// Supports volume control, panning, and channel mapping.
//
// ===============================================================================

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
use spin::Mutex;

use super::{AudioFormat, SampleFormat};

// =============================================================================
// MIXER CHANNEL
// =============================================================================

/// Mixer channel for a single audio source
pub struct MixerChannel {
    /// Channel name
    name: String,
    /// Volume (0-100)
    volume: AtomicU8,
    /// Balance/Pan (-100 to +100, 0 = center)
    balance: i8,
    /// Muted
    muted: AtomicBool,
    /// Solo (only this channel plays)
    solo: AtomicBool,
    /// Channel format
    format: AudioFormat,
    /// Ring buffer for audio data
    buffer: Mutex<MixerBuffer>,
    /// Active (receiving data)
    active: AtomicBool,
}

impl MixerChannel {
    pub fn new(name: String, format: AudioFormat, buffer_size: usize) -> Self {
        Self {
            name,
            volume: AtomicU8::new(100),
            balance: 0,
            muted: AtomicBool::new(false),
            solo: AtomicBool::new(false),
            format,
            buffer: Mutex::new(MixerBuffer::new(buffer_size)),
            active: AtomicBool::new(true),
        }
    }

    /// Write audio data to channel
    pub fn write(&self, data: &[u8]) -> usize {
        if !self.active.load(Ordering::Acquire) {
            return 0;
        }

        self.buffer.lock().write(data)
    }

    /// Read audio data from channel
    pub fn read(&self, data: &mut [u8]) -> usize {
        if self.muted.load(Ordering::Acquire) {
            data.fill(0);
            return data.len();
        }

        self.buffer.lock().read(data)
    }

    /// Get available data in channel
    pub fn available(&self) -> usize {
        self.buffer.lock().available()
    }

    /// Get free space in channel buffer
    pub fn free(&self) -> usize {
        self.buffer.lock().free()
    }

    /// Get volume
    pub fn volume(&self) -> u8 {
        self.volume.load(Ordering::Acquire)
    }

    /// Set volume
    pub fn set_volume(&self, volume: u8) {
        self.volume.store(volume.min(100), Ordering::Release);
    }

    /// Check if muted
    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Acquire)
    }

    /// Set mute state
    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Release);
    }

    /// Check if solo
    pub fn is_solo(&self) -> bool {
        self.solo.load(Ordering::Acquire)
    }

    /// Set solo state
    pub fn set_solo(&self, solo: bool) {
        self.solo.store(solo, Ordering::Release);
    }

    /// Get channel name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get channel format
    pub fn format(&self) -> AudioFormat {
        self.format
    }
}

// =============================================================================
// MIXER BUFFER
// =============================================================================

/// Ring buffer for mixer channel
struct MixerBuffer {
    data: Box<[u8]>,
    size: usize,
    read_pos: usize,
    write_pos: usize,
}

impl MixerBuffer {
    fn new(size: usize) -> Self {
        Self {
            data: alloc::vec![0u8; size].into_boxed_slice(),
            size,
            read_pos: 0,
            write_pos: 0,
        }
    }

    fn available(&self) -> usize {
        if self.write_pos >= self.read_pos {
            self.write_pos - self.read_pos
        } else {
            self.size - self.read_pos + self.write_pos
        }
    }

    fn free(&self) -> usize {
        self.size - self.available() - 1
    }

    fn write(&mut self, data: &[u8]) -> usize {
        let free = self.free();
        let to_write = data.len().min(free);

        if to_write == 0 {
            return 0;
        }

        let first_chunk = (self.size - self.write_pos).min(to_write);
        self.data[self.write_pos..self.write_pos + first_chunk]
            .copy_from_slice(&data[..first_chunk]);

        let second_chunk = to_write - first_chunk;
        if second_chunk > 0 {
            self.data[..second_chunk].copy_from_slice(&data[first_chunk..to_write]);
        }

        self.write_pos = (self.write_pos + to_write) % self.size;
        to_write
    }

    fn read(&mut self, data: &mut [u8]) -> usize {
        let available = self.available();
        let to_read = data.len().min(available);

        if to_read == 0 {
            data.fill(0);
            return 0;
        }

        let first_chunk = (self.size - self.read_pos).min(to_read);
        data[..first_chunk]
            .copy_from_slice(&self.data[self.read_pos..self.read_pos + first_chunk]);

        let second_chunk = to_read - first_chunk;
        if second_chunk > 0 {
            data[first_chunk..to_read].copy_from_slice(&self.data[..second_chunk]);
        }

        self.read_pos = (self.read_pos + to_read) % self.size;

        // Fill rest with silence
        data[to_read..].fill(0);

        to_read
    }

    fn clear(&mut self) {
        self.read_pos = 0;
        self.write_pos = 0;
        self.data.fill(0);
    }
}

// =============================================================================
// SOFTWARE MIXER
// =============================================================================

/// Software audio mixer
pub struct SoftwareMixer {
    /// Output format
    output_format: AudioFormat,
    /// Master volume
    master_volume: AtomicU8,
    /// Master mute
    master_muted: AtomicBool,
    /// Input channels
    channels: Mutex<Vec<Arc<MixerChannel>>>,
    /// Mixing buffer
    mix_buffer: Mutex<Vec<i32>>,
    /// Output buffer size
    output_buffer_size: usize,
}

impl SoftwareMixer {
    pub fn new(output_format: AudioFormat, buffer_size: usize) -> Self {
        let samples_per_buffer = buffer_size / output_format.bytes_per_frame();

        Self {
            output_format,
            master_volume: AtomicU8::new(100),
            master_muted: AtomicBool::new(false),
            channels: Mutex::new(Vec::new()),
            mix_buffer: Mutex::new(vec![0i32; samples_per_buffer * output_format.channels as usize]),
            output_buffer_size: buffer_size,
        }
    }

    /// Add a channel to the mixer
    pub fn add_channel(&self, name: String, format: AudioFormat) -> Arc<MixerChannel> {
        let channel = Arc::new(MixerChannel::new(
            name,
            format,
            self.output_buffer_size * 4,
        ));

        self.channels.lock().push(channel.clone());
        channel
    }

    /// Remove a channel from the mixer
    pub fn remove_channel(&self, name: &str) {
        self.channels.lock().retain(|c| c.name() != name);
    }

    /// Get master volume
    pub fn master_volume(&self) -> u8 {
        self.master_volume.load(Ordering::Acquire)
    }

    /// Set master volume
    pub fn set_master_volume(&self, volume: u8) {
        self.master_volume.store(volume.min(100), Ordering::Release);
    }

    /// Check if master is muted
    pub fn is_master_muted(&self) -> bool {
        self.master_muted.load(Ordering::Acquire)
    }

    /// Set master mute
    pub fn set_master_muted(&self, muted: bool) {
        self.master_muted.store(muted, Ordering::Release);
    }

    /// Mix all channels and produce output
    pub fn mix(&self, output: &mut [u8]) -> usize {
        if self.master_muted.load(Ordering::Acquire) {
            output.fill(0);
            return output.len();
        }

        let channels = self.channels.lock();
        let num_output_channels = self.output_format.channels as usize;
        let bytes_per_sample = self.output_format.sample_format.bytes_per_sample();
        let frames = output.len() / self.output_format.bytes_per_frame();

        // Check for solo channels
        let any_solo = channels.iter().any(|c| c.is_solo());

        // Clear mix buffer
        let mut mix_buffer = self.mix_buffer.lock();
        mix_buffer.fill(0);

        // Temporary buffer for reading from channels
        let mut channel_buffer = vec![0u8; frames * num_output_channels * bytes_per_sample];

        // Mix each channel
        for channel in channels.iter() {
            // Skip if solo mode and this channel is not solo
            if any_solo && !channel.is_solo() {
                continue;
            }

            // Skip if muted
            if channel.is_muted() {
                continue;
            }

            // Read from channel
            let read = channel.read(&mut channel_buffer);
            if read == 0 {
                continue;
            }

            let channel_volume = channel.volume() as i32;

            // Mix samples
            match channel.format().sample_format {
                SampleFormat::S16Le => {
                    self.mix_s16le(&channel_buffer, &mut mix_buffer, channel_volume, num_output_channels);
                }
                SampleFormat::U8 => {
                    self.mix_u8(&channel_buffer, &mut mix_buffer, channel_volume, num_output_channels);
                }
                _ => {
                    // Unsupported format
                }
            }
        }

        // Apply master volume and convert to output format
        let master_vol = self.master_volume.load(Ordering::Acquire) as i32;

        match self.output_format.sample_format {
            SampleFormat::S16Le => {
                self.output_s16le(&mix_buffer, output, master_vol);
            }
            SampleFormat::U8 => {
                self.output_u8(&mix_buffer, output, master_vol);
            }
            _ => {
                output.fill(0);
            }
        }

        output.len()
    }

    /// Mix S16LE samples into mix buffer
    fn mix_s16le(&self, input: &[u8], mix_buffer: &mut [i32], volume: i32, _channels: usize) {
        let samples = input.len() / 2;

        for i in 0..samples.min(mix_buffer.len()) {
            if i * 2 + 1 >= input.len() {
                break;
            }

            let sample = i16::from_le_bytes([input[i * 2], input[i * 2 + 1]]) as i32;
            mix_buffer[i] += (sample * volume) / 100;
        }
    }

    /// Mix U8 samples into mix buffer
    fn mix_u8(&self, input: &[u8], mix_buffer: &mut [i32], volume: i32, _channels: usize) {
        for i in 0..input.len().min(mix_buffer.len()) {
            // Convert U8 to signed value (-128 to 127)
            let sample = (input[i] as i32 - 128) * 256; // Scale to 16-bit range
            mix_buffer[i] += (sample * volume) / 100;
        }
    }

    /// Output mixed samples as S16LE
    fn output_s16le(&self, mix_buffer: &[i32], output: &mut [u8], master_vol: i32) {
        let samples = output.len() / 2;

        for i in 0..samples.min(mix_buffer.len()) {
            // Apply master volume and clamp
            let sample = ((mix_buffer[i] * master_vol) / 100).clamp(-32768, 32767) as i16;
            let bytes = sample.to_le_bytes();
            output[i * 2] = bytes[0];
            output[i * 2 + 1] = bytes[1];
        }
    }

    /// Output mixed samples as U8
    fn output_u8(&self, mix_buffer: &[i32], output: &mut [u8], master_vol: i32) {
        for i in 0..output.len().min(mix_buffer.len()) {
            let sample = ((mix_buffer[i] * master_vol) / 100) / 256;
            output[i] = (sample.clamp(-128, 127) + 128) as u8;
        }
    }

    /// Get list of channel names
    pub fn channel_names(&self) -> Vec<String> {
        self.channels.lock().iter().map(|c| c.name().to_string()).collect()
    }

    /// Get channel by name
    pub fn channel(&self, name: &str) -> Option<Arc<MixerChannel>> {
        self.channels.lock().iter().find(|c| c.name() == name).cloned()
    }
}

// =============================================================================
// DSP UTILITIES
// =============================================================================

/// Apply simple low-pass filter
pub fn lowpass_filter(samples: &mut [i16], cutoff: f32, sample_rate: f32) {
    let rc = 1.0 / (2.0 * core::f32::consts::PI * cutoff);
    let dt = 1.0 / sample_rate;
    let alpha = dt / (rc + dt);

    let mut prev = samples[0] as f32;

    for sample in samples.iter_mut() {
        let current = *sample as f32;
        prev = prev + alpha * (current - prev);
        *sample = prev as i16;
    }
}

/// Apply simple high-pass filter
pub fn highpass_filter(samples: &mut [i16], cutoff: f32, sample_rate: f32) {
    let rc = 1.0 / (2.0 * core::f32::consts::PI * cutoff);
    let dt = 1.0 / sample_rate;
    let alpha = rc / (rc + dt);

    let mut prev_in = samples[0] as f32;
    let mut prev_out = samples[0] as f32;

    for sample in samples.iter_mut() {
        let current = *sample as f32;
        prev_out = alpha * (prev_out + current - prev_in);
        prev_in = current;
        *sample = prev_out as i16;
    }
}

/// Apply gain to samples
pub fn apply_gain(samples: &mut [i16], gain_db: f32) {
    let gain = libm::powf(10.0, gain_db / 20.0);

    for sample in samples.iter_mut() {
        let amplified = (*sample as f32) * gain;
        *sample = amplified.clamp(-32768.0, 32767.0) as i16;
    }
}

/// Mix two sample buffers
pub fn mix_samples(dst: &mut [i16], src: &[i16], src_volume: f32) {
    for (d, s) in dst.iter_mut().zip(src.iter()) {
        let mixed = (*d as f32) + (*s as f32) * src_volume;
        *d = mixed.clamp(-32768.0, 32767.0) as i16;
    }
}

/// Convert mono to stereo
pub fn mono_to_stereo(mono: &[i16], stereo: &mut [i16]) {
    for (i, sample) in mono.iter().enumerate() {
        if i * 2 + 1 < stereo.len() {
            stereo[i * 2] = *sample;
            stereo[i * 2 + 1] = *sample;
        }
    }
}

/// Convert stereo to mono
pub fn stereo_to_mono(stereo: &[i16], mono: &mut [i16]) {
    for (i, mono_sample) in mono.iter_mut().enumerate() {
        if i * 2 + 1 < stereo.len() {
            let left = stereo[i * 2] as i32;
            let right = stereo[i * 2 + 1] as i32;
            *mono_sample = ((left + right) / 2) as i16;
        }
    }
}

/// Resample audio (simple linear interpolation)
pub fn resample(input: &[i16], output: &mut [i16], input_rate: u32, output_rate: u32) {
    if input_rate == output_rate {
        let len = input.len().min(output.len());
        output[..len].copy_from_slice(&input[..len]);
        return;
    }

    let ratio = input_rate as f32 / output_rate as f32;

    for (i, out_sample) in output.iter_mut().enumerate() {
        let pos = (i as f32) * ratio;
        let idx = pos as usize;
        let frac = pos - idx as f32;

        if idx + 1 < input.len() {
            let a = input[idx] as f32;
            let b = input[idx + 1] as f32;
            *out_sample = (a + frac * (b - a)) as i16;
        } else if idx < input.len() {
            *out_sample = input[idx];
        } else {
            *out_sample = 0;
        }
    }
}

// =============================================================================
// AUDIO LEVEL METER
// =============================================================================

/// Audio level meter
pub struct LevelMeter {
    /// Peak level (0.0 to 1.0)
    peak: AtomicU32,
    /// RMS level (0.0 to 1.0)
    rms: AtomicU32,
    /// Peak hold time in samples
    peak_hold_samples: usize,
    /// Current peak hold counter
    peak_hold_counter: AtomicU32,
}

impl LevelMeter {
    pub fn new(peak_hold_samples: usize) -> Self {
        Self {
            peak: AtomicU32::new(0),
            rms: AtomicU32::new(0),
            peak_hold_samples,
            peak_hold_counter: AtomicU32::new(0),
        }
    }

    /// Update meter with new samples
    pub fn update(&self, samples: &[i16]) {
        if samples.is_empty() {
            return;
        }

        let mut max_sample: i32 = 0;
        let mut sum_squares: i64 = 0;

        for &sample in samples {
            let abs = (sample as i32).abs();
            if abs > max_sample {
                max_sample = abs;
            }
            sum_squares += (sample as i64) * (sample as i64);
        }

        // Update peak with hold
        let current_peak = self.peak.load(Ordering::Acquire);
        let new_peak_f = max_sample as f32 / 32768.0;
        let new_peak = (new_peak_f * 1000000.0) as u32;

        if new_peak >= current_peak {
            self.peak.store(new_peak, Ordering::Release);
            self.peak_hold_counter.store(self.peak_hold_samples as u32, Ordering::Release);
        } else {
            let counter = self.peak_hold_counter.load(Ordering::Acquire);
            if counter > 0 {
                self.peak_hold_counter.store(counter.saturating_sub(samples.len() as u32), Ordering::Release);
            } else {
                // Decay peak
                let decayed = ((current_peak as f32) * 0.99) as u32;
                self.peak.store(decayed, Ordering::Release);
            }
        }

        // Update RMS
        let rms_f = libm::sqrtf((sum_squares as f32) / (samples.len() as f32)) / 32768.0;
        self.rms.store((rms_f * 1000000.0) as u32, Ordering::Release);
    }

    /// Get peak level (0.0 to 1.0)
    pub fn peak(&self) -> f32 {
        (self.peak.load(Ordering::Acquire) as f32) / 1000000.0
    }

    /// Get RMS level (0.0 to 1.0)
    pub fn rms(&self) -> f32 {
        (self.rms.load(Ordering::Acquire) as f32) / 1000000.0
    }

    /// Get peak in dB
    pub fn peak_db(&self) -> f32 {
        let peak = self.peak();
        if peak > 0.0 {
            20.0 * libm::log10f(peak)
        } else {
            -96.0
        }
    }

    /// Get RMS in dB
    pub fn rms_db(&self) -> f32 {
        let rms = self.rms();
        if rms > 0.0 {
            20.0 * libm::log10f(rms)
        } else {
            -96.0
        }
    }

    /// Reset the meter
    pub fn reset(&self) {
        self.peak.store(0, Ordering::Release);
        self.rms.store(0, Ordering::Release);
        self.peak_hold_counter.store(0, Ordering::Release);
    }
}

use alloc::string::ToString;
