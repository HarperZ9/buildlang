// ===============================================================================
// BUILDOS KERNEL - CHACHA20 CSPRNG
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! ChaCha20-based Cryptographically Secure PRNG
//!
//! Implements the ChaCha20 stream cipher as a CSPRNG following RFC 7539.
//! Used as the primary random number generator after seeding from entropy pool.

/// ChaCha20 state (16 x 32-bit words)
const STATE_WORDS: usize = 16;

/// ChaCha20 constants "expand 32-byte k"
const CONSTANTS: [u32; 4] = [0x61707865, 0x3320646e, 0x79622d32, 0x6b206574];

/// ChaCha20 CSPRNG
#[derive(Clone)]
pub struct ChaCha20Rng {
    /// ChaCha20 state
    state: [u32; STATE_WORDS],
    /// Output buffer
    buffer: [u8; 64],
    /// Position in buffer
    position: usize,
    /// Block counter
    counter: u64,
}

impl ChaCha20Rng {
    /// Create new ChaCha20 RNG from 32-byte seed
    pub fn from_seed(seed: [u8; 32]) -> Self {
        let mut state = [0u32; STATE_WORDS];

        // Constants
        state[0] = CONSTANTS[0];
        state[1] = CONSTANTS[1];
        state[2] = CONSTANTS[2];
        state[3] = CONSTANTS[3];

        // Key (8 words from 32-byte seed)
        for i in 0..8 {
            state[4 + i] = u32::from_le_bytes([
                seed[i * 4],
                seed[i * 4 + 1],
                seed[i * 4 + 2],
                seed[i * 4 + 3],
            ]);
        }

        // Counter (start at 0)
        state[12] = 0;
        state[13] = 0;

        // Nonce (all zeros for CSPRNG use)
        state[14] = 0;
        state[15] = 0;

        let mut rng = Self {
            state,
            buffer: [0u8; 64],
            position: 64, // Force regeneration on first use
            counter: 0,
        };

        // Generate first block
        rng.refill();

        rng
    }

    /// Reseed the RNG with new key material
    pub fn reseed(&mut self, seed: &[u8; 32]) {
        // Update key
        for i in 0..8 {
            self.state[4 + i] = u32::from_le_bytes([
                seed[i * 4],
                seed[i * 4 + 1],
                seed[i * 4 + 2],
                seed[i * 4 + 3],
            ]);
        }

        // Reset counter
        self.counter = 0;
        self.state[12] = 0;
        self.state[13] = 0;

        // Regenerate buffer
        self.refill();
    }

    /// Fill buffer with random bytes
    pub fn fill(&mut self, dest: &mut [u8]) {
        let mut offset = 0;

        while offset < dest.len() {
            if self.position >= 64 {
                self.refill();
            }

            let available = 64 - self.position;
            let to_copy = core::cmp::min(available, dest.len() - offset);
            dest[offset..offset + to_copy]
                .copy_from_slice(&self.buffer[self.position..self.position + to_copy]);
            self.position += to_copy;
            offset += to_copy;
        }
    }

    /// Get next u32
    pub fn next_u32(&mut self) -> u32 {
        let mut buf = [0u8; 4];
        self.fill(&mut buf);
        u32::from_le_bytes(buf)
    }

    /// Get next u64
    pub fn next_u64(&mut self) -> u64 {
        let mut buf = [0u8; 8];
        self.fill(&mut buf);
        u64::from_le_bytes(buf)
    }

    /// Refill the output buffer
    fn refill(&mut self) {
        // Update counter in state
        self.state[12] = (self.counter & 0xFFFFFFFF) as u32;
        self.state[13] = ((self.counter >> 32) & 0xFFFFFFFF) as u32;

        // Run ChaCha20 block function
        let output = self.chacha20_block();

        // Convert output to bytes
        for (i, word) in output.iter().enumerate() {
            let bytes = word.to_le_bytes();
            self.buffer[i * 4..i * 4 + 4].copy_from_slice(&bytes);
        }

        self.position = 0;
        self.counter = self.counter.wrapping_add(1);
    }

    /// ChaCha20 block function
    fn chacha20_block(&self) -> [u32; STATE_WORDS] {
        let mut working = self.state;

        // 20 rounds (10 column rounds + 10 diagonal rounds)
        for _ in 0..10 {
            // Column rounds
            quarter_round(&mut working, 0, 4, 8, 12);
            quarter_round(&mut working, 1, 5, 9, 13);
            quarter_round(&mut working, 2, 6, 10, 14);
            quarter_round(&mut working, 3, 7, 11, 15);

            // Diagonal rounds
            quarter_round(&mut working, 0, 5, 10, 15);
            quarter_round(&mut working, 1, 6, 11, 12);
            quarter_round(&mut working, 2, 7, 8, 13);
            quarter_round(&mut working, 3, 4, 9, 14);
        }

        // Add original state
        for i in 0..STATE_WORDS {
            working[i] = working[i].wrapping_add(self.state[i]);
        }

        working
    }
}

/// ChaCha20 quarter round
#[inline(always)]
fn quarter_round(state: &mut [u32; STATE_WORDS], a: usize, b: usize, c: usize, d: usize) {
    state[a] = state[a].wrapping_add(state[b]);
    state[d] ^= state[a];
    state[d] = state[d].rotate_left(16);

    state[c] = state[c].wrapping_add(state[d]);
    state[b] ^= state[c];
    state[b] = state[b].rotate_left(12);

    state[a] = state[a].wrapping_add(state[b]);
    state[d] ^= state[a];
    state[d] = state[d].rotate_left(8);

    state[c] = state[c].wrapping_add(state[d]);
    state[b] ^= state[c];
    state[b] = state[b].rotate_left(7);
}

/// ChaCha20 stream cipher (for encryption, not just PRNG)
pub struct ChaCha20 {
    state: [u32; STATE_WORDS],
    counter: u64,
}

impl ChaCha20 {
    /// Create new ChaCha20 cipher
    pub fn new(key: &[u8; 32], nonce: &[u8; 12]) -> Self {
        let mut state = [0u32; STATE_WORDS];

        // Constants
        state[0] = CONSTANTS[0];
        state[1] = CONSTANTS[1];
        state[2] = CONSTANTS[2];
        state[3] = CONSTANTS[3];

        // Key
        for i in 0..8 {
            state[4 + i] = u32::from_le_bytes([
                key[i * 4],
                key[i * 4 + 1],
                key[i * 4 + 2],
                key[i * 4 + 3],
            ]);
        }

        // Counter
        state[12] = 0;

        // Nonce
        state[13] = u32::from_le_bytes([nonce[0], nonce[1], nonce[2], nonce[3]]);
        state[14] = u32::from_le_bytes([nonce[4], nonce[5], nonce[6], nonce[7]]);
        state[15] = u32::from_le_bytes([nonce[8], nonce[9], nonce[10], nonce[11]]);

        Self { state, counter: 0 }
    }

    /// Encrypt/decrypt data in place (XOR with keystream)
    pub fn apply_keystream(&mut self, data: &mut [u8]) {
        let mut offset = 0;

        while offset < data.len() {
            // Update counter
            self.state[12] = (self.counter & 0xFFFFFFFF) as u32;

            // Generate block
            let mut working = self.state;
            for _ in 0..10 {
                quarter_round(&mut working, 0, 4, 8, 12);
                quarter_round(&mut working, 1, 5, 9, 13);
                quarter_round(&mut working, 2, 6, 10, 14);
                quarter_round(&mut working, 3, 7, 11, 15);
                quarter_round(&mut working, 0, 5, 10, 15);
                quarter_round(&mut working, 1, 6, 11, 12);
                quarter_round(&mut working, 2, 7, 8, 13);
                quarter_round(&mut working, 3, 4, 9, 14);
            }

            for i in 0..STATE_WORDS {
                working[i] = working[i].wrapping_add(self.state[i]);
            }

            // XOR with data
            for (i, word) in working.iter().enumerate() {
                let bytes = word.to_le_bytes();
                for (j, &byte) in bytes.iter().enumerate() {
                    let idx = offset + i * 4 + j;
                    if idx >= data.len() {
                        break;
                    }
                    data[idx] ^= byte;
                }
            }

            self.counter += 1;
            offset += 64;
        }
    }

    /// Get keystream bytes
    pub fn keystream(&mut self, output: &mut [u8]) {
        output.fill(0);
        self.apply_keystream(output);
    }
}

/// HChaCha20 - extended nonce variant
pub fn hchacha20(key: &[u8; 32], nonce: &[u8; 16]) -> [u8; 32] {
    let mut state = [0u32; STATE_WORDS];

    // Constants
    state[0] = CONSTANTS[0];
    state[1] = CONSTANTS[1];
    state[2] = CONSTANTS[2];
    state[3] = CONSTANTS[3];

    // Key
    for i in 0..8 {
        state[4 + i] = u32::from_le_bytes([
            key[i * 4],
            key[i * 4 + 1],
            key[i * 4 + 2],
            key[i * 4 + 3],
        ]);
    }

    // Nonce
    for i in 0..4 {
        state[12 + i] = u32::from_le_bytes([
            nonce[i * 4],
            nonce[i * 4 + 1],
            nonce[i * 4 + 2],
            nonce[i * 4 + 3],
        ]);
    }

    // 20 rounds
    for _ in 0..10 {
        quarter_round(&mut state, 0, 4, 8, 12);
        quarter_round(&mut state, 1, 5, 9, 13);
        quarter_round(&mut state, 2, 6, 10, 14);
        quarter_round(&mut state, 3, 7, 11, 15);
        quarter_round(&mut state, 0, 5, 10, 15);
        quarter_round(&mut state, 1, 6, 11, 12);
        quarter_round(&mut state, 2, 7, 8, 13);
        quarter_round(&mut state, 3, 4, 9, 14);
    }

    // Extract subkey from words 0-3 and 12-15
    let mut subkey = [0u8; 32];
    for i in 0..4 {
        let bytes = state[i].to_le_bytes();
        subkey[i * 4..i * 4 + 4].copy_from_slice(&bytes);
    }
    for i in 0..4 {
        let bytes = state[12 + i].to_le_bytes();
        subkey[16 + i * 4..16 + i * 4 + 4].copy_from_slice(&bytes);
    }

    subkey
}

/// XChaCha20 - ChaCha20 with extended 24-byte nonce
pub struct XChaCha20 {
    inner: ChaCha20,
}

impl XChaCha20 {
    /// Create new XChaCha20 cipher
    pub fn new(key: &[u8; 32], nonce: &[u8; 24]) -> Self {
        // Derive subkey using HChaCha20 with first 16 bytes of nonce
        let mut hnonce = [0u8; 16];
        hnonce.copy_from_slice(&nonce[..16]);
        let subkey = hchacha20(key, &hnonce);

        // Create ChaCha20 with subkey and last 8 bytes of nonce (prefixed with zeros)
        let mut chacha_nonce = [0u8; 12];
        chacha_nonce[4..].copy_from_slice(&nonce[16..]);

        Self {
            inner: ChaCha20::new(&subkey, &chacha_nonce),
        }
    }

    /// Encrypt/decrypt data
    pub fn apply_keystream(&mut self, data: &mut [u8]) {
        self.inner.apply_keystream(data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chacha20_rng() {
        let seed = [0u8; 32];
        let mut rng = ChaCha20Rng::from_seed(seed);

        let mut buf = [0u8; 64];
        rng.fill(&mut buf);

        // Should produce non-zero output
        assert!(buf.iter().any(|&b| b != 0));
    }

    #[test]
    fn test_quarter_round() {
        let mut state = [
            0x879531e0, 0xc5ecf37d, 0x516461b1, 0xc9a62f8a,
            0x44c20ef3, 0x3390af7f, 0xd9fc690b, 0x2a5f714c,
            0x53372767, 0xb00a5631, 0x974c541a, 0x359e9963,
            0x5c971061, 0x3d631689, 0x2098d9d6, 0x91dbd320,
        ];

        quarter_round(&mut state, 2, 7, 8, 13);

        assert_eq!(state[2], 0xbdb886dc);
        assert_eq!(state[7], 0xcfacafd2);
        assert_eq!(state[8], 0xe46bea80);
        assert_eq!(state[13], 0xccc07c79);
    }
}
