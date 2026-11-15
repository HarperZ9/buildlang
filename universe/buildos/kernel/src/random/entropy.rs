// ===============================================================================
// BUILDOS KERNEL - ENTROPY SOURCES
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Entropy Source Abstractions
//!
//! Defines traits and implementations for various entropy sources:
//! - Hardware RNG (RDRAND/RDSEED)
//! - Timing jitter
//! - Interrupt timing
//! - Disk I/O timing
//! - Network packet timing

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

/// Entropy source trait
pub trait EntropySource: Send + Sync {
    /// Name of the entropy source
    fn name(&self) -> &'static str;

    /// Get entropy bytes
    /// Returns (bytes, estimated_entropy_bits)
    fn get_entropy(&mut self, buf: &mut [u8]) -> (usize, u64);

    /// Quality rating (0-100, higher is better)
    fn quality(&self) -> u8;

    /// Whether source is available
    fn is_available(&self) -> bool;

    /// Estimated bits of entropy per byte
    fn entropy_rate(&self) -> f32;
}

/// Hardware RNG using RDRAND/RDSEED
pub struct HardwareRng {
    has_rdrand: bool,
    has_rdseed: bool,
    failure_count: AtomicU64,
}

impl HardwareRng {
    pub fn new() -> Self {
        Self {
            has_rdrand: check_rdrand(),
            has_rdseed: check_rdseed(),
            failure_count: AtomicU64::new(0),
        }
    }

    /// Get raw RDRAND value with retry
    pub fn rdrand_retry(&self, retries: u32) -> Option<u64> {
        for _ in 0..retries {
            if let Some(val) = rdrand_u64() {
                return Some(val);
            }
        }
        self.failure_count.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Get raw RDSEED value with retry
    pub fn rdseed_retry(&self, retries: u32) -> Option<u64> {
        for _ in 0..retries {
            if let Some(val) = rdseed_u64() {
                return Some(val);
            }
            // RDSEED may need more time between attempts
            for _ in 0..100 {
                core::hint::spin_loop();
            }
        }
        self.failure_count.fetch_add(1, Ordering::Relaxed);
        None
    }
}

impl EntropySource for HardwareRng {
    fn name(&self) -> &'static str {
        if self.has_rdseed {
            "Intel RDSEED"
        } else if self.has_rdrand {
            "Intel RDRAND"
        } else {
            "Hardware RNG (unavailable)"
        }
    }

    fn get_entropy(&mut self, buf: &mut [u8]) -> (usize, u64) {
        if !self.is_available() {
            return (0, 0);
        }

        let mut filled = 0;
        let mut entropy_bits = 0u64;

        // Prefer RDSEED for true entropy
        if self.has_rdseed {
            while filled + 8 <= buf.len() {
                if let Some(val) = self.rdseed_retry(10) {
                    buf[filled..filled + 8].copy_from_slice(&val.to_le_bytes());
                    filled += 8;
                    entropy_bits += 64; // RDSEED provides full entropy
                } else {
                    break;
                }
            }
        }

        // Fall back to RDRAND
        if self.has_rdrand && filled < buf.len() {
            while filled + 8 <= buf.len() {
                if let Some(val) = self.rdrand_retry(10) {
                    buf[filled..filled + 8].copy_from_slice(&val.to_le_bytes());
                    filled += 8;
                    entropy_bits += 8; // RDRAND entropy estimate is lower
                } else {
                    break;
                }
            }

            // Handle remaining bytes
            if filled < buf.len() {
                if let Some(val) = self.rdrand_retry(10) {
                    let bytes = val.to_le_bytes();
                    let remaining = buf.len() - filled;
                    buf[filled..].copy_from_slice(&bytes[..remaining]);
                    filled += remaining;
                    entropy_bits += remaining as u64;
                }
            }
        }

        (filled, entropy_bits)
    }

    fn quality(&self) -> u8 {
        if self.has_rdseed {
            95
        } else if self.has_rdrand {
            80
        } else {
            0
        }
    }

    fn is_available(&self) -> bool {
        self.has_rdrand || self.has_rdseed
    }

    fn entropy_rate(&self) -> f32 {
        if self.has_rdseed {
            8.0 // Full entropy per byte
        } else if self.has_rdrand {
            1.0 // Conservative estimate
        } else {
            0.0
        }
    }
}

impl Default for HardwareRng {
    fn default() -> Self {
        Self::new()
    }
}

/// Timing jitter entropy source
pub struct TimingJitter {
    samples: Vec<u64>,
    last_tsc: u64,
    collection_active: bool,
}

impl TimingJitter {
    pub fn new() -> Self {
        Self {
            samples: Vec::with_capacity(256),
            last_tsc: read_tsc(),
            collection_active: true,
        }
    }

    /// Collect a timing sample
    pub fn collect_sample(&mut self) {
        if !self.collection_active {
            return;
        }

        let tsc = read_tsc();
        let delta = tsc.wrapping_sub(self.last_tsc);
        self.last_tsc = tsc;

        if self.samples.len() < 256 {
            self.samples.push(delta);
        } else {
            // Rotate samples
            self.samples.remove(0);
            self.samples.push(delta);
        }
    }

    /// Estimate entropy from collected samples
    fn estimate_entropy(&self) -> u64 {
        if self.samples.len() < 64 {
            return 0;
        }

        // Count unique low bits (simple entropy estimation)
        let mut seen = [false; 256];
        let mut unique = 0;

        for &sample in &self.samples {
            let byte = (sample & 0xFF) as u8;
            if !seen[byte as usize] {
                seen[byte as usize] = true;
                unique += 1;
            }
        }

        // Conservative: 1 bit per 4 unique values seen
        unique / 4
    }
}

impl EntropySource for TimingJitter {
    fn name(&self) -> &'static str {
        "CPU Timing Jitter"
    }

    fn get_entropy(&mut self, buf: &mut [u8]) -> (usize, u64) {
        // Collect more samples
        for _ in 0..buf.len() * 8 {
            self.collect_sample();
        }

        // Mix samples into output
        let mut filled = 0;
        let mut sample_idx = 0;

        while filled < buf.len() && sample_idx < self.samples.len() {
            // XOR fold multiple samples into one byte
            let mut byte = 0u8;
            for i in 0..8.min(self.samples.len() - sample_idx) {
                let sample = self.samples[sample_idx + i];
                byte ^= (sample >> (i * 8)) as u8;
            }
            buf[filled] = byte;
            filled += 1;
            sample_idx += 8;
        }

        let entropy_bits = self.estimate_entropy().min((filled * 2) as u64);
        (filled, entropy_bits)
    }

    fn quality(&self) -> u8 {
        if self.samples.len() >= 64 {
            60
        } else {
            20
        }
    }

    fn is_available(&self) -> bool {
        true // Always available, just varying quality
    }

    fn entropy_rate(&self) -> f32 {
        0.25 // Conservative: 2 bits per byte
    }
}

impl Default for TimingJitter {
    fn default() -> Self {
        Self::new()
    }
}

/// Interrupt timing entropy source
pub struct InterruptEntropy {
    samples: Vec<(u64, u32)>, // (timestamp, irq)
    max_samples: usize,
}

impl InterruptEntropy {
    pub fn new() -> Self {
        Self {
            samples: Vec::with_capacity(128),
            max_samples: 128,
        }
    }

    /// Record an interrupt
    pub fn record_interrupt(&mut self, irq: u32) {
        let tsc = read_tsc();

        if self.samples.len() >= self.max_samples {
            self.samples.remove(0);
        }
        self.samples.push((tsc, irq));
    }

    /// Extract entropy from samples
    pub fn extract(&mut self) -> Vec<u8> {
        if self.samples.len() < 16 {
            return Vec::new();
        }

        let mut output = Vec::new();

        // Compute deltas between consecutive interrupts
        let mut deltas = Vec::new();
        for i in 1..self.samples.len() {
            let delta = self.samples[i].0.wrapping_sub(self.samples[i - 1].0);
            let irq_mix = self.samples[i].1 ^ self.samples[i - 1].1;
            deltas.push(delta ^ (irq_mix as u64));
        }

        // XOR fold deltas into bytes
        for chunk in deltas.chunks(8) {
            let mut byte = 0u8;
            for (i, &delta) in chunk.iter().enumerate() {
                byte ^= (delta >> (i * 8)) as u8;
            }
            output.push(byte);
        }

        // Clear used samples
        self.samples.clear();

        output
    }
}

impl EntropySource for InterruptEntropy {
    fn name(&self) -> &'static str {
        "Interrupt Timing"
    }

    fn get_entropy(&mut self, buf: &mut [u8]) -> (usize, u64) {
        let extracted = self.extract();
        let to_copy = core::cmp::min(extracted.len(), buf.len());
        buf[..to_copy].copy_from_slice(&extracted[..to_copy]);

        // Conservative entropy estimate
        let entropy_bits = (to_copy / 2) as u64;
        (to_copy, entropy_bits)
    }

    fn quality(&self) -> u8 {
        if self.samples.len() >= 64 {
            70
        } else {
            30
        }
    }

    fn is_available(&self) -> bool {
        self.samples.len() >= 16
    }

    fn entropy_rate(&self) -> f32 {
        0.5 // 4 bits per byte
    }
}

impl Default for InterruptEntropy {
    fn default() -> Self {
        Self::new()
    }
}

/// Disk I/O timing entropy source
pub struct DiskEntropy {
    samples: Vec<(u64, u64, u64)>, // (sector, latency_ns, timestamp)
}

impl DiskEntropy {
    pub fn new() -> Self {
        Self {
            samples: Vec::with_capacity(64),
        }
    }

    /// Record a disk operation
    pub fn record_io(&mut self, sector: u64, latency_ns: u64) {
        let tsc = read_tsc();

        if self.samples.len() >= 64 {
            self.samples.remove(0);
        }
        self.samples.push((sector, latency_ns, tsc));
    }
}

impl EntropySource for DiskEntropy {
    fn name(&self) -> &'static str {
        "Disk I/O Timing"
    }

    fn get_entropy(&mut self, buf: &mut [u8]) -> (usize, u64) {
        if self.samples.len() < 8 {
            return (0, 0);
        }

        let mut filled = 0;

        for chunk in self.samples.chunks(4) {
            if filled >= buf.len() {
                break;
            }

            // Mix latency values (most variable part)
            let mut mixed = 0u64;
            for (_, latency, tsc) in chunk {
                mixed ^= latency ^ tsc;
            }

            let bytes = mixed.to_le_bytes();
            let to_copy = core::cmp::min(8, buf.len() - filled);
            buf[filled..filled + to_copy].copy_from_slice(&bytes[..to_copy]);
            filled += to_copy;
        }

        self.samples.clear();

        // Conservative estimate
        let entropy_bits = (filled / 4) as u64;
        (filled, entropy_bits)
    }

    fn quality(&self) -> u8 {
        if self.samples.len() >= 32 {
            75
        } else {
            40
        }
    }

    fn is_available(&self) -> bool {
        self.samples.len() >= 8
    }

    fn entropy_rate(&self) -> f32 {
        0.5
    }
}

impl Default for DiskEntropy {
    fn default() -> Self {
        Self::new()
    }
}

/// Network packet entropy source
pub struct NetworkEntropy {
    samples: Vec<(u64, u16, u8)>, // (timestamp, size, protocol)
}

impl NetworkEntropy {
    pub fn new() -> Self {
        Self {
            samples: Vec::with_capacity(128),
        }
    }

    /// Record a network packet
    pub fn record_packet(&mut self, size: u16, protocol: u8) {
        let tsc = read_tsc();

        if self.samples.len() >= 128 {
            self.samples.remove(0);
        }
        self.samples.push((tsc, size, protocol));
    }
}

impl EntropySource for NetworkEntropy {
    fn name(&self) -> &'static str {
        "Network Packet Timing"
    }

    fn get_entropy(&mut self, buf: &mut [u8]) -> (usize, u64) {
        if self.samples.len() < 8 {
            return (0, 0);
        }

        let mut filled = 0;

        // Compute inter-packet timing deltas
        let mut deltas = Vec::new();
        for i in 1..self.samples.len() {
            let delta = self.samples[i].0.wrapping_sub(self.samples[i - 1].0);
            let mix = (self.samples[i].1 as u64) ^ ((self.samples[i].2 as u64) << 16);
            deltas.push(delta ^ mix);
        }

        for delta in &deltas {
            if filled >= buf.len() {
                break;
            }
            buf[filled] = (*delta & 0xFF) as u8;
            filled += 1;
        }

        self.samples.clear();

        let entropy_bits = (filled / 2) as u64;
        (filled, entropy_bits)
    }

    fn quality(&self) -> u8 {
        if self.samples.len() >= 64 {
            70
        } else {
            35
        }
    }

    fn is_available(&self) -> bool {
        self.samples.len() >= 8
    }

    fn entropy_rate(&self) -> f32 {
        0.5
    }
}

impl Default for NetworkEntropy {
    fn default() -> Self {
        Self::new()
    }
}

/// Keyboard/mouse input entropy
pub struct InputEntropy {
    samples: Vec<(u64, u8, u8)>, // (timestamp, event_type, data)
}

impl InputEntropy {
    pub fn new() -> Self {
        Self {
            samples: Vec::with_capacity(64),
        }
    }

    /// Record a keyboard event
    pub fn record_key(&mut self, scancode: u8) {
        let tsc = read_tsc();
        if self.samples.len() >= 64 {
            self.samples.remove(0);
        }
        self.samples.push((tsc, 0, scancode));
    }

    /// Record a mouse event
    pub fn record_mouse(&mut self, buttons: u8, dx: i8, dy: i8) {
        let tsc = read_tsc();
        if self.samples.len() >= 64 {
            self.samples.remove(0);
        }
        self.samples.push((tsc, 1, buttons ^ (dx as u8) ^ (dy as u8)));
    }
}

impl EntropySource for InputEntropy {
    fn name(&self) -> &'static str {
        "User Input Timing"
    }

    fn get_entropy(&mut self, buf: &mut [u8]) -> (usize, u64) {
        if self.samples.len() < 4 {
            return (0, 0);
        }

        let mut filled = 0;

        for i in 1..self.samples.len() {
            if filled >= buf.len() {
                break;
            }

            let delta = self.samples[i].0.wrapping_sub(self.samples[i - 1].0);
            let mix = self.samples[i].2 ^ self.samples[i - 1].2;
            buf[filled] = ((delta ^ (mix as u64)) & 0xFF) as u8;
            filled += 1;
        }

        self.samples.clear();

        // User input has high entropy from timing
        let entropy_bits = (filled * 2) as u64;
        (filled, entropy_bits)
    }

    fn quality(&self) -> u8 {
        if self.samples.len() >= 16 {
            85
        } else {
            50
        }
    }

    fn is_available(&self) -> bool {
        self.samples.len() >= 4
    }

    fn entropy_rate(&self) -> f32 {
        2.0 // High entropy from unpredictable user behavior
    }
}

impl Default for InputEntropy {
    fn default() -> Self {
        Self::new()
    }
}

/// Combined entropy source manager
pub struct EntropyManager {
    sources: Vec<Box<dyn EntropySource>>,
}

impl EntropyManager {
    pub fn new() -> Self {
        let mut sources: Vec<Box<dyn EntropySource>> = Vec::new();

        // Add hardware RNG if available
        let hw_rng = HardwareRng::new();
        if hw_rng.is_available() {
            sources.push(Box::new(hw_rng));
        }

        // Add timing jitter
        sources.push(Box::new(TimingJitter::new()));

        // Add interrupt entropy
        sources.push(Box::new(InterruptEntropy::new()));

        Self { sources }
    }

    /// Get entropy from all available sources
    pub fn get_entropy(&mut self, buf: &mut [u8]) -> (usize, u64) {
        let mut total_filled = 0;
        let mut total_entropy = 0u64;
        let mut temp = vec![0u8; buf.len()];

        for source in &mut self.sources {
            if source.is_available() {
                let (filled, entropy) = source.get_entropy(&mut temp);
                if filled > 0 {
                    // XOR into output buffer
                    for i in 0..filled.min(buf.len()) {
                        buf[i] ^= temp[i];
                    }
                    total_filled = total_filled.max(filled);
                    total_entropy += entropy;
                }
            }
        }

        (total_filled, total_entropy)
    }

    /// Add a custom entropy source
    pub fn add_source(&mut self, source: Box<dyn EntropySource>) {
        self.sources.push(source);
    }

    /// Get quality rating (weighted average)
    pub fn quality(&self) -> u8 {
        if self.sources.is_empty() {
            return 0;
        }

        let total: u32 = self.sources.iter()
            .filter(|s| s.is_available())
            .map(|s| s.quality() as u32)
            .sum();

        let count = self.sources.iter()
            .filter(|s| s.is_available())
            .count() as u32;

        if count > 0 {
            (total / count) as u8
        } else {
            0
        }
    }
}

impl Default for EntropyManager {
    fn default() -> Self {
        Self::new()
    }
}

// Helper functions

fn check_rdrand() -> bool {
    let ecx: u32;
    unsafe {
        core::arch::asm!(
            "push rbx",
            "mov eax, 1",
            "cpuid",
            "pop rbx",
            out("ecx") ecx,
            out("eax") _,
            out("edx") _,
            options(nomem)
        );
    }
    ecx & (1 << 30) != 0
}

fn check_rdseed() -> bool {
    let ebx: u32;
    unsafe {
        core::arch::asm!(
            "push rbx",
            "mov eax, 7",
            "xor ecx, ecx",
            "cpuid",
            "mov {ebx_out:e}, ebx",
            "pop rbx",
            ebx_out = out(reg) ebx,
            out("eax") _,
            out("ecx") _,
            out("edx") _,
            options(nomem)
        );
    }
    ebx & (1 << 18) != 0
}

fn rdrand_u64() -> Option<u64> {
    let value: u64;
    let success: u8;
    unsafe {
        core::arch::asm!(
            "rdrand {0}",
            "setc {1}",
            out(reg) value,
            out(reg_byte) success,
            options(nostack, nomem)
        );
    }
    if success != 0 { Some(value) } else { None }
}

fn rdseed_u64() -> Option<u64> {
    let value: u64;
    let success: u8;
    unsafe {
        core::arch::asm!(
            "rdseed {0}",
            "setc {1}",
            out(reg) value,
            out(reg_byte) success,
            options(nostack, nomem)
        );
    }
    if success != 0 { Some(value) } else { None }
}

fn read_tsc() -> u64 {
    unsafe {
        let lo: u32;
        let hi: u32;
        core::arch::asm!(
            "rdtsc",
            out("eax") lo,
            out("edx") hi,
            options(nostack, nomem)
        );
        ((hi as u64) << 32) | (lo as u64)
    }
}
