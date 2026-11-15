// ===============================================================================
// BUILDOS KERNEL - ENTROPY POOL
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Entropy Pool
//!
//! Collects and mixes entropy from various sources using a cryptographic
//! hash function. The pool is designed to:
//! - Accumulate entropy from multiple sources
//! - Mix entropy thoroughly using SHA-256-like compression
//! - Provide extracted randomness without depleting the pool
//! - Track entropy estimation conservatively

use alloc::vec::Vec;

/// Pool size in bytes
pub const POOL_SIZE: usize = 512;

/// Maximum entropy bits the pool can hold
pub const MAX_ENTROPY_BITS: u64 = POOL_SIZE as u64 * 8;

/// SHA-256 initial hash values
const H: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
    0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// SHA-256 round constants
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
    0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
    0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
    0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
    0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
    0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// Entropy pool for collecting and mixing randomness
pub struct EntropyPool {
    /// Pool buffer
    pool: [u8; POOL_SIZE],
    /// Current mix position
    mix_pos: usize,
    /// Estimated entropy bits
    entropy_bits: u64,
    /// Input buffer for compression
    input_buffer: Vec<u8>,
    /// Extraction counter
    extract_count: u64,
}

impl EntropyPool {
    /// Create new empty entropy pool
    pub fn new() -> Self {
        Self {
            pool: [0u8; POOL_SIZE],
            mix_pos: 0,
            entropy_bits: 0,
            input_buffer: Vec::new(),
            extract_count: 0,
        }
    }

    /// Add entropy to the pool
    pub fn add_entropy(&mut self, data: &[u8], bits: u64) {
        // Add data to input buffer
        self.input_buffer.extend_from_slice(data);

        // Mix when we have enough data
        while self.input_buffer.len() >= 64 {
            let mut block = [0u8; 64];
            block.copy_from_slice(&self.input_buffer[..64]);
            self.input_buffer.drain(..64);
            self.mix_block(&block);
        }

        // Also do a quick mix of remaining data
        if !data.is_empty() {
            self.quick_mix(data);
        }

        // Update entropy estimate (conservative)
        self.entropy_bits = core::cmp::min(
            self.entropy_bits.saturating_add(bits),
            MAX_ENTROPY_BITS,
        );
    }

    /// Quick mix for small amounts of data
    fn quick_mix(&mut self, data: &[u8]) {
        for &byte in data {
            // XOR with current position
            self.pool[self.mix_pos] ^= byte;

            // Rotate pool position with feedback
            let feedback = self.pool[self.mix_pos];
            self.mix_pos = (self.mix_pos + 1 + (feedback as usize & 0x1F)) % POOL_SIZE;

            // Additional mixing
            let prev = (self.mix_pos + POOL_SIZE - 1) % POOL_SIZE;
            let next = (self.mix_pos + 1) % POOL_SIZE;
            self.pool[self.mix_pos] ^= self.pool[prev].rotate_left(3);
            self.pool[next] ^= byte.rotate_right(5);
        }
    }

    /// Mix a 64-byte block using SHA-256-like compression
    fn mix_block(&mut self, block: &[u8; 64]) {
        // Extract current state from pool positions
        let pos1 = self.mix_pos;
        let pos2 = (self.mix_pos + 64) % POOL_SIZE;
        let pos3 = (self.mix_pos + 128) % POOL_SIZE;
        let pos4 = (self.mix_pos + 192) % POOL_SIZE;
        let pos5 = (self.mix_pos + 256) % POOL_SIZE;
        let pos6 = (self.mix_pos + 320) % POOL_SIZE;
        let pos7 = (self.mix_pos + 384) % POOL_SIZE;
        let pos8 = (self.mix_pos + 448) % POOL_SIZE;

        let mut h = [
            u32::from_be_bytes([
                self.pool[pos1], self.pool[pos1 + 1],
                self.pool[pos1 + 2], self.pool[pos1 + 3],
            ]) ^ H[0],
            u32::from_be_bytes([
                self.pool[pos2], self.pool[pos2 + 1],
                self.pool[pos2 + 2], self.pool[pos2 + 3],
            ]) ^ H[1],
            u32::from_be_bytes([
                self.pool[pos3], self.pool[pos3 + 1],
                self.pool[pos3 + 2], self.pool[pos3 + 3],
            ]) ^ H[2],
            u32::from_be_bytes([
                self.pool[pos4], self.pool[pos4 + 1],
                self.pool[pos4 + 2], self.pool[pos4 + 3],
            ]) ^ H[3],
            u32::from_be_bytes([
                self.pool[pos5], self.pool[pos5 + 1],
                self.pool[pos5 + 2], self.pool[pos5 + 3],
            ]) ^ H[4],
            u32::from_be_bytes([
                self.pool[pos6], self.pool[pos6 + 1],
                self.pool[pos6 + 2], self.pool[pos6 + 3],
            ]) ^ H[5],
            u32::from_be_bytes([
                self.pool[pos7], self.pool[pos7 + 1],
                self.pool[pos7 + 2], self.pool[pos7 + 3],
            ]) ^ H[6],
            u32::from_be_bytes([
                self.pool[pos8], self.pool[pos8 + 1],
                self.pool[pos8 + 2], self.pool[pos8 + 3],
            ]) ^ H[7],
        ];

        // Compress block
        self.sha256_compress(&mut h, block);

        // Mix result back into pool
        for (i, &word) in h.iter().enumerate() {
            let bytes = word.to_be_bytes();
            let pos = (self.mix_pos + i * 64) % POOL_SIZE;
            for (j, &b) in bytes.iter().enumerate() {
                self.pool[(pos + j) % POOL_SIZE] ^= b;
            }
        }

        // Update mix position
        self.mix_pos = (self.mix_pos + 32) % POOL_SIZE;
    }

    /// SHA-256 compression function
    fn sha256_compress(&self, h: &mut [u32; 8], block: &[u8; 64]) {
        // Parse block into words
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }

        // Extend words
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        // Initialize working variables
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);

        // Compression loop
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        // Add back to hash
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    /// Extract random bytes from pool
    pub fn extract(&mut self, output: &mut [u8]) {
        let mut extracted = 0;

        while extracted < output.len() {
            // Create extraction block with counter and pool state
            let mut block = [0u8; 64];
            block[0..8].copy_from_slice(&self.extract_count.to_le_bytes());

            // Fill rest with pool data from various positions
            for i in 0..7 {
                let pos = (self.mix_pos + i * 64 + (self.extract_count as usize * 8)) % POOL_SIZE;
                let end = core::cmp::min(pos + 8, POOL_SIZE);
                let len = end - pos;
                block[8 + i * 8..8 + i * 8 + len].copy_from_slice(&self.pool[pos..end]);
            }

            // Hash the block
            let mut h = H;
            self.sha256_compress(&mut h, &block);

            // Extract bytes from hash
            for (i, &word) in h.iter().enumerate() {
                let bytes = word.to_be_bytes();
                for (j, &b) in bytes.iter().enumerate() {
                    if extracted + i * 4 + j < output.len() {
                        output[extracted + i * 4 + j] = b;
                    }
                }
            }

            extracted += 32;
            self.extract_count += 1;

            // Mix extraction back into pool
            self.quick_mix(&output[extracted.saturating_sub(32)..extracted.min(output.len())]);
        }

        // Reduce entropy estimate (extraction consumes entropy)
        let bits_extracted = (output.len() * 8) as u64;
        self.entropy_bits = self.entropy_bits.saturating_sub(bits_extracted);
    }

    /// Get current entropy estimate in bits
    pub fn entropy_bits(&self) -> u64 {
        self.entropy_bits
    }

    /// Check if pool has minimum entropy
    pub fn has_min_entropy(&self) -> bool {
        self.entropy_bits >= super::MIN_ENTROPY_BITS
    }

    /// Reset pool (for testing only)
    #[cfg(test)]
    pub fn reset(&mut self) {
        self.pool = [0u8; POOL_SIZE];
        self.mix_pos = 0;
        self.entropy_bits = 0;
        self.input_buffer.clear();
        self.extract_count = 0;
    }
}

impl Default for EntropyPool {
    fn default() -> Self {
        Self::new()
    }
}

/// Fast mixing function based on SipHash
pub fn fast_mix(data: &[u8], seed: u64) -> u64 {
    let mut v0 = 0x736f6d6570736575u64 ^ seed;
    let mut v1 = 0x646f72616e646f6du64 ^ seed;
    let mut v2 = 0x6c7967656e657261u64 ^ seed;
    let mut v3 = 0x7465646279746573u64 ^ seed;

    let mut offset = 0;
    while offset + 8 <= data.len() {
        let m = u64::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ]);

        v3 ^= m;
        sipround(&mut v0, &mut v1, &mut v2, &mut v3);
        sipround(&mut v0, &mut v1, &mut v2, &mut v3);
        v0 ^= m;

        offset += 8;
    }

    // Handle remaining bytes
    let mut last = ((data.len() & 0xFF) as u64) << 56;
    let remaining = data.len() - offset;
    for i in 0..remaining {
        last |= (data[offset + i] as u64) << (i * 8);
    }

    v3 ^= last;
    sipround(&mut v0, &mut v1, &mut v2, &mut v3);
    sipround(&mut v0, &mut v1, &mut v2, &mut v3);
    v0 ^= last;

    v2 ^= 0xFF;
    sipround(&mut v0, &mut v1, &mut v2, &mut v3);
    sipround(&mut v0, &mut v1, &mut v2, &mut v3);
    sipround(&mut v0, &mut v1, &mut v2, &mut v3);
    sipround(&mut v0, &mut v1, &mut v2, &mut v3);

    v0 ^ v1 ^ v2 ^ v3
}

#[inline(always)]
fn sipround(v0: &mut u64, v1: &mut u64, v2: &mut u64, v3: &mut u64) {
    *v0 = v0.wrapping_add(*v1);
    *v1 = v1.rotate_left(13);
    *v1 ^= *v0;
    *v0 = v0.rotate_left(32);

    *v2 = v2.wrapping_add(*v3);
    *v3 = v3.rotate_left(16);
    *v3 ^= *v2;

    *v0 = v0.wrapping_add(*v3);
    *v3 = v3.rotate_left(21);
    *v3 ^= *v0;

    *v2 = v2.wrapping_add(*v1);
    *v1 = v1.rotate_left(17);
    *v1 ^= *v2;
    *v2 = v2.rotate_left(32);
}

/// Timing jitter entropy source
pub struct TimingJitterSource {
    last_sample: u64,
    samples: [u8; 64],
    sample_count: usize,
}

impl TimingJitterSource {
    pub fn new() -> Self {
        Self {
            last_sample: 0,
            samples: [0u8; 64],
            sample_count: 0,
        }
    }

    /// Collect a timing sample
    pub fn sample(&mut self) -> Option<u8> {
        let tsc = read_tsc();
        let delta = tsc.wrapping_sub(self.last_sample);
        self.last_sample = tsc;

        // XOR fold delta into a byte
        let byte = ((delta >> 0) ^ (delta >> 8) ^ (delta >> 16) ^ (delta >> 24)) as u8;

        self.samples[self.sample_count % 64] = byte;
        self.sample_count += 1;

        if self.sample_count >= 64 {
            Some(byte)
        } else {
            None
        }
    }

    /// Get collected samples
    pub fn get_samples(&self) -> &[u8] {
        let count = core::cmp::min(self.sample_count, 64);
        &self.samples[..count]
    }

    /// Estimate entropy bits
    pub fn entropy_estimate(&self) -> u64 {
        if self.sample_count < 64 {
            0
        } else {
            // Conservative estimate: 1 bit per 8 samples
            (self.sample_count / 8) as u64
        }
    }
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

impl Default for TimingJitterSource {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entropy_pool() {
        let mut pool = EntropyPool::new();

        // Add some entropy
        let data = b"test entropy data";
        pool.add_entropy(data, 64);

        assert!(pool.entropy_bits() >= 64);

        // Extract some bytes
        let mut output = [0u8; 32];
        pool.extract(&mut output);

        // Should produce non-zero output
        assert!(output.iter().any(|&b| b != 0));
    }

    #[test]
    fn test_fast_mix() {
        let data = b"test data for mixing";
        let result1 = fast_mix(data, 12345);
        let result2 = fast_mix(data, 12345);
        let result3 = fast_mix(data, 54321);

        assert_eq!(result1, result2); // Same seed = same result
        assert_ne!(result1, result3); // Different seed = different result
    }
}
