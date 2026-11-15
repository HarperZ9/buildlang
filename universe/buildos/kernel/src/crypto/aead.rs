//! Authenticated Encryption with Associated Data (AEAD)
//!
//! Provides AES-GCM, ChaCha20-Poly1305, and other AEAD algorithms.

use alloc::boxed::Box;
use alloc::vec::Vec;
use super::{CryptoError, STATS};
use super::cipher::CipherOps;
use core::sync::atomic::Ordering;

/// AEAD algorithm identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AeadAlgorithm {
    /// AES-128-GCM
    Aes128Gcm,
    /// AES-256-GCM
    Aes256Gcm,
    /// ChaCha20-Poly1305
    ChaCha20Poly1305,
    /// AES-128-CCM
    Aes128Ccm,
    /// AES-256-CCM
    Aes256Ccm,
}

impl AeadAlgorithm {
    /// Get key size in bytes
    pub fn key_size(&self) -> usize {
        match self {
            Self::Aes128Gcm | Self::Aes128Ccm => 16,
            Self::Aes256Gcm | Self::Aes256Ccm => 32,
            Self::ChaCha20Poly1305 => 32,
        }
    }

    /// Get nonce size in bytes
    pub fn nonce_size(&self) -> usize {
        match self {
            Self::Aes128Gcm | Self::Aes256Gcm => 12,
            Self::ChaCha20Poly1305 => 12,
            Self::Aes128Ccm | Self::Aes256Ccm => 12,
        }
    }

    /// Get tag size in bytes
    pub fn tag_size(&self) -> usize {
        match self {
            Self::Aes128Gcm | Self::Aes256Gcm => 16,
            Self::ChaCha20Poly1305 => 16,
            Self::Aes128Ccm | Self::Aes256Ccm => 16,
        }
    }
}

/// AEAD operations trait
pub trait AeadOps: Send + Sync {
    /// Get algorithm name
    fn name(&self) -> &str;

    /// Get key size
    fn key_size(&self) -> usize;

    /// Get nonce size
    fn nonce_size(&self) -> usize;

    /// Get tag size
    fn tag_size(&self) -> usize;

    /// Encrypt and authenticate
    fn encrypt(
        &self,
        nonce: &[u8],
        aad: &[u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, CryptoError>;

    /// Decrypt and verify
    fn decrypt(
        &self,
        nonce: &[u8],
        aad: &[u8],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, CryptoError>;

    /// Encrypt in place
    fn encrypt_in_place(
        &self,
        nonce: &[u8],
        aad: &[u8],
        buffer: &mut Vec<u8>,
    ) -> Result<(), CryptoError> {
        let ciphertext = self.encrypt(nonce, aad, buffer)?;
        buffer.clear();
        buffer.extend_from_slice(&ciphertext);
        Ok(())
    }

    /// Decrypt in place
    fn decrypt_in_place(
        &self,
        nonce: &[u8],
        aad: &[u8],
        buffer: &mut Vec<u8>,
    ) -> Result<(), CryptoError> {
        let plaintext = self.decrypt(nonce, aad, buffer)?;
        buffer.clear();
        buffer.extend_from_slice(&plaintext);
        Ok(())
    }
}

/// AEAD context
pub struct Aead {
    /// Algorithm
    algorithm: AeadAlgorithm,
    /// Implementation
    inner: Box<dyn AeadOps>,
}

impl Aead {
    /// Create new AEAD context
    pub fn new(algorithm: AeadAlgorithm, key: &[u8]) -> Result<Self, CryptoError> {
        if key.len() != algorithm.key_size() {
            return Err(CryptoError::InvalidKeySize);
        }

        let inner: Box<dyn AeadOps> = match algorithm {
            AeadAlgorithm::Aes128Gcm | AeadAlgorithm::Aes256Gcm => {
                Box::new(AesGcm::new(key)?)
            }
            AeadAlgorithm::ChaCha20Poly1305 => {
                Box::new(ChaCha20Poly1305::new(key)?)
            }
            _ => return Err(CryptoError::AlgorithmNotFound),
        };

        Ok(Self { algorithm, inner })
    }

    /// Encrypt and authenticate
    pub fn encrypt(
        &self,
        nonce: &[u8],
        aad: &[u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        if nonce.len() != self.algorithm.nonce_size() {
            return Err(CryptoError::InvalidNonceSize);
        }
        STATS.encryptions.fetch_add(1, Ordering::Relaxed);
        self.inner.encrypt(nonce, aad, plaintext)
    }

    /// Decrypt and verify
    pub fn decrypt(
        &self,
        nonce: &[u8],
        aad: &[u8],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        if nonce.len() != self.algorithm.nonce_size() {
            return Err(CryptoError::InvalidNonceSize);
        }
        if ciphertext.len() < self.algorithm.tag_size() {
            return Err(CryptoError::InvalidCiphertext);
        }
        STATS.decryptions.fetch_add(1, Ordering::Relaxed);
        self.inner.decrypt(nonce, aad, ciphertext)
    }

    /// Get algorithm
    pub fn algorithm(&self) -> AeadAlgorithm {
        self.algorithm
    }
}

// =============================================================================
// AES-GCM Implementation
// =============================================================================

/// AES-GCM authenticated encryption
struct AesGcm {
    /// AES cipher for encryption
    cipher: Box<dyn CipherOps>,
    /// Precomputed H value for GHASH
    h: [u64; 2],
    /// Key length
    key_len: usize,
}

impl AesGcm {
    /// Create new AES-GCM instance
    pub fn new(key: &[u8]) -> Result<Self, CryptoError> {
        if key.len() != 16 && key.len() != 32 {
            return Err(CryptoError::InvalidKeySize);
        }

        let cipher = super::cipher::allocate("aes-ecb", key)?;

        // Compute H = E(K, 0^128)
        let mut h_block = [0u8; 16];
        cipher.encrypt(&[], &mut h_block)?;

        let h0 = u64::from_be_bytes([
            h_block[0], h_block[1], h_block[2], h_block[3],
            h_block[4], h_block[5], h_block[6], h_block[7],
        ]);
        let h1 = u64::from_be_bytes([
            h_block[8], h_block[9], h_block[10], h_block[11],
            h_block[12], h_block[13], h_block[14], h_block[15],
        ]);

        Ok(Self {
            cipher,
            h: [h0, h1],
            key_len: key.len(),
        })
    }

    /// GHASH multiplication in GF(2^128)
    fn ghash_multiply(&self, x: &[u64; 2], y: &[u64; 2]) -> [u64; 2] {
        let mut z = [0u64; 2];
        let mut v = *y;

        // Polynomial multiplication in GF(2^128)
        for i in 0..2 {
            for j in 0..64 {
                if (x[i] >> (63 - j)) & 1 == 1 {
                    z[0] ^= v[0];
                    z[1] ^= v[1];
                }

                // Multiply v by x (shift and reduce)
                let lsb = v[1] & 1;
                v[1] = (v[1] >> 1) | (v[0] << 63);
                v[0] >>= 1;

                // Reduction polynomial: x^128 + x^7 + x^2 + x + 1
                if lsb == 1 {
                    v[0] ^= 0xe100000000000000;
                }
            }
        }

        z
    }

    /// Compute GHASH
    fn ghash(&self, aad: &[u8], ciphertext: &[u8]) -> [u8; 16] {
        let mut y = [0u64; 2];

        // Process AAD
        let mut pos = 0;
        while pos + 16 <= aad.len() {
            let x = [
                u64::from_be_bytes([
                    aad[pos], aad[pos + 1], aad[pos + 2], aad[pos + 3],
                    aad[pos + 4], aad[pos + 5], aad[pos + 6], aad[pos + 7],
                ]),
                u64::from_be_bytes([
                    aad[pos + 8], aad[pos + 9], aad[pos + 10], aad[pos + 11],
                    aad[pos + 12], aad[pos + 13], aad[pos + 14], aad[pos + 15],
                ]),
            ];
            y[0] ^= x[0];
            y[1] ^= x[1];
            y = self.ghash_multiply(&y, &self.h);
            pos += 16;
        }

        // Handle partial AAD block
        if pos < aad.len() {
            let mut block = [0u8; 16];
            block[..aad.len() - pos].copy_from_slice(&aad[pos..]);
            let x = [
                u64::from_be_bytes([
                    block[0], block[1], block[2], block[3],
                    block[4], block[5], block[6], block[7],
                ]),
                u64::from_be_bytes([
                    block[8], block[9], block[10], block[11],
                    block[12], block[13], block[14], block[15],
                ]),
            ];
            y[0] ^= x[0];
            y[1] ^= x[1];
            y = self.ghash_multiply(&y, &self.h);
        }

        // Process ciphertext
        pos = 0;
        while pos + 16 <= ciphertext.len() {
            let x = [
                u64::from_be_bytes([
                    ciphertext[pos], ciphertext[pos + 1], ciphertext[pos + 2], ciphertext[pos + 3],
                    ciphertext[pos + 4], ciphertext[pos + 5], ciphertext[pos + 6], ciphertext[pos + 7],
                ]),
                u64::from_be_bytes([
                    ciphertext[pos + 8], ciphertext[pos + 9], ciphertext[pos + 10], ciphertext[pos + 11],
                    ciphertext[pos + 12], ciphertext[pos + 13], ciphertext[pos + 14], ciphertext[pos + 15],
                ]),
            ];
            y[0] ^= x[0];
            y[1] ^= x[1];
            y = self.ghash_multiply(&y, &self.h);
            pos += 16;
        }

        // Handle partial ciphertext block
        if pos < ciphertext.len() {
            let mut block = [0u8; 16];
            block[..ciphertext.len() - pos].copy_from_slice(&ciphertext[pos..]);
            let x = [
                u64::from_be_bytes([
                    block[0], block[1], block[2], block[3],
                    block[4], block[5], block[6], block[7],
                ]),
                u64::from_be_bytes([
                    block[8], block[9], block[10], block[11],
                    block[12], block[13], block[14], block[15],
                ]),
            ];
            y[0] ^= x[0];
            y[1] ^= x[1];
            y = self.ghash_multiply(&y, &self.h);
        }

        // Add lengths
        let len_block = [
            ((aad.len() * 8) as u64),
            ((ciphertext.len() * 8) as u64),
        ];
        y[0] ^= len_block[0];
        y[1] ^= len_block[1];
        y = self.ghash_multiply(&y, &self.h);

        // Convert to bytes
        let mut result = [0u8; 16];
        result[..8].copy_from_slice(&y[0].to_be_bytes());
        result[8..].copy_from_slice(&y[1].to_be_bytes());
        result
    }

    /// Generate counter block
    fn make_counter(&self, nonce: &[u8], counter: u32) -> [u8; 16] {
        let mut block = [0u8; 16];
        block[..12].copy_from_slice(nonce);
        block[12..16].copy_from_slice(&counter.to_be_bytes());
        block
    }
}

impl AeadOps for AesGcm {
    fn name(&self) -> &str {
        if self.key_len == 16 { "aes-128-gcm" } else { "aes-256-gcm" }
    }

    fn key_size(&self) -> usize {
        self.key_len
    }

    fn nonce_size(&self) -> usize {
        12
    }

    fn tag_size(&self) -> usize {
        16
    }

    fn encrypt(
        &self,
        nonce: &[u8],
        aad: &[u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        if nonce.len() != 12 {
            return Err(CryptoError::InvalidNonceSize);
        }

        let mut ciphertext = Vec::with_capacity(plaintext.len() + 16);

        // Encrypt using CTR mode starting at counter 2
        let mut counter = 2u32;
        let mut pos = 0;

        while pos < plaintext.len() {
            let mut keystream = self.make_counter(nonce, counter);
            self.cipher.encrypt(&[], &mut keystream)?;

            let block_len = core::cmp::min(16, plaintext.len() - pos);
            for i in 0..block_len {
                ciphertext.push(plaintext[pos + i] ^ keystream[i]);
            }

            counter += 1;
            pos += 16;
        }

        // Compute GHASH
        let ghash = self.ghash(aad, &ciphertext);

        // Encrypt GHASH with counter 1 to get tag
        let mut tag = self.make_counter(nonce, 1);
        self.cipher.encrypt(&[], &mut tag)?;
        for i in 0..16 {
            tag[i] ^= ghash[i];
        }

        // Append tag
        ciphertext.extend_from_slice(&tag);

        Ok(ciphertext)
    }

    fn decrypt(
        &self,
        nonce: &[u8],
        aad: &[u8],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        if nonce.len() != 12 {
            return Err(CryptoError::InvalidNonceSize);
        }

        if ciphertext.len() < 16 {
            return Err(CryptoError::InvalidCiphertext);
        }

        let tag_start = ciphertext.len() - 16;
        let ct = &ciphertext[..tag_start];
        let received_tag = &ciphertext[tag_start..];

        // Compute expected GHASH
        let ghash = self.ghash(aad, ct);

        // Encrypt GHASH with counter 1 to get expected tag
        let mut expected_tag = self.make_counter(nonce, 1);
        self.cipher.encrypt(&[], &mut expected_tag)?;
        for i in 0..16 {
            expected_tag[i] ^= ghash[i];
        }

        // Constant-time tag comparison
        let mut diff = 0u8;
        for i in 0..16 {
            diff |= expected_tag[i] ^ received_tag[i];
        }
        if diff != 0 {
            return Err(CryptoError::AuthenticationFailed);
        }

        // Decrypt using CTR mode
        let mut plaintext = Vec::with_capacity(ct.len());
        let mut counter = 2u32;
        let mut pos = 0;

        while pos < ct.len() {
            let mut keystream = self.make_counter(nonce, counter);
            self.cipher.encrypt(&[], &mut keystream)?;

            let block_len = core::cmp::min(16, ct.len() - pos);
            for i in 0..block_len {
                plaintext.push(ct[pos + i] ^ keystream[i]);
            }

            counter += 1;
            pos += 16;
        }

        Ok(plaintext)
    }
}

// =============================================================================
// ChaCha20-Poly1305 Implementation
// =============================================================================

/// ChaCha20-Poly1305 authenticated encryption
struct ChaCha20Poly1305 {
    /// Key
    key: [u8; 32],
}

impl ChaCha20Poly1305 {
    /// Create new ChaCha20-Poly1305 instance
    pub fn new(key: &[u8]) -> Result<Self, CryptoError> {
        if key.len() != 32 {
            return Err(CryptoError::InvalidKeySize);
        }

        let mut k = [0u8; 32];
        k.copy_from_slice(key);

        Ok(Self { key: k })
    }

    /// ChaCha20 quarter round
    fn quarter_round(state: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
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

    /// ChaCha20 block function
    fn chacha20_block(&self, nonce: &[u8], counter: u32) -> [u8; 64] {
        // Initialize state
        let mut state = [0u32; 16];

        // Constants "expand 32-byte k"
        state[0] = 0x61707865;
        state[1] = 0x3320646e;
        state[2] = 0x79622d32;
        state[3] = 0x6b206574;

        // Key
        for i in 0..8 {
            state[4 + i] = u32::from_le_bytes([
                self.key[i * 4],
                self.key[i * 4 + 1],
                self.key[i * 4 + 2],
                self.key[i * 4 + 3],
            ]);
        }

        // Counter
        state[12] = counter;

        // Nonce
        state[13] = u32::from_le_bytes([nonce[0], nonce[1], nonce[2], nonce[3]]);
        state[14] = u32::from_le_bytes([nonce[4], nonce[5], nonce[6], nonce[7]]);
        state[15] = u32::from_le_bytes([nonce[8], nonce[9], nonce[10], nonce[11]]);

        let initial_state = state;

        // 20 rounds (10 double rounds)
        for _ in 0..10 {
            // Column rounds
            Self::quarter_round(&mut state, 0, 4, 8, 12);
            Self::quarter_round(&mut state, 1, 5, 9, 13);
            Self::quarter_round(&mut state, 2, 6, 10, 14);
            Self::quarter_round(&mut state, 3, 7, 11, 15);

            // Diagonal rounds
            Self::quarter_round(&mut state, 0, 5, 10, 15);
            Self::quarter_round(&mut state, 1, 6, 11, 12);
            Self::quarter_round(&mut state, 2, 7, 8, 13);
            Self::quarter_round(&mut state, 3, 4, 9, 14);
        }

        // Add initial state
        for i in 0..16 {
            state[i] = state[i].wrapping_add(initial_state[i]);
        }

        // Serialize to bytes
        let mut output = [0u8; 64];
        for i in 0..16 {
            output[i * 4..(i + 1) * 4].copy_from_slice(&state[i].to_le_bytes());
        }

        output
    }

    /// Generate Poly1305 key from ChaCha20
    fn poly1305_key(&self, nonce: &[u8]) -> [u8; 32] {
        let block = self.chacha20_block(nonce, 0);
        let mut key = [0u8; 32];
        key.copy_from_slice(&block[..32]);
        key
    }

    /// Poly1305 MAC
    fn poly1305_mac(&self, key: &[u8; 32], data: &[u8]) -> [u8; 16] {
        // r is first 16 bytes, clamped
        let r0 = u64::from_le_bytes([key[0], key[1], key[2], key[3], key[4], key[5], key[6], key[7]]);
        let r1 = u64::from_le_bytes([key[8], key[9], key[10], key[11], key[12], key[13], key[14], key[15]]);

        let r0 = r0 & 0x0ffffffc0fffffff;
        let r1 = r1 & 0x0ffffffc0ffffffc;

        // s is second 16 bytes
        let s0 = u64::from_le_bytes([key[16], key[17], key[18], key[19], key[20], key[21], key[22], key[23]]);
        let s1 = u64::from_le_bytes([key[24], key[25], key[26], key[27], key[28], key[29], key[30], key[31]]);

        // Accumulator
        let mut h = [0u128; 3];

        // Process full blocks
        let mut pos = 0;
        while pos + 16 <= data.len() {
            let n0 = u64::from_le_bytes([
                data[pos], data[pos + 1], data[pos + 2], data[pos + 3],
                data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7],
            ]);
            let n1 = u64::from_le_bytes([
                data[pos + 8], data[pos + 9], data[pos + 10], data[pos + 11],
                data[pos + 12], data[pos + 13], data[pos + 14], data[pos + 15],
            ]);

            // h += n
            h[0] += n0 as u128;
            h[1] += n1 as u128;
            h[2] += 1; // High bit

            // h *= r (simplified)
            let t0 = h[0] * r0 as u128;
            let t1 = h[0] * r1 as u128 + h[1] * r0 as u128;
            let t2 = h[1] * r1 as u128 + h[2] * r0 as u128;

            // Carry propagation (simplified)
            h[0] = t0 & 0xffffffffffffffff;
            h[1] = (t1 + (t0 >> 64)) & 0xffffffffffffffff;
            h[2] = (t2 + (t1 >> 64)) & 0x3;

            pos += 16;
        }

        // Handle final partial block
        if pos < data.len() {
            let mut block = [0u8; 17];
            block[..data.len() - pos].copy_from_slice(&data[pos..]);
            block[data.len() - pos] = 1; // Padding

            let n0 = u64::from_le_bytes([
                block[0], block[1], block[2], block[3],
                block[4], block[5], block[6], block[7],
            ]);
            let n1 = u64::from_le_bytes([
                block[8], block[9], block[10], block[11],
                block[12], block[13], block[14], block[15],
            ]);

            h[0] += n0 as u128;
            h[1] += n1 as u128;
            // No high bit for partial block
        }

        // h += s
        let result0 = (h[0] as u64).wrapping_add(s0);
        let carry = if result0 < s0 { 1u64 } else { 0u64 };
        let result1 = (h[1] as u64).wrapping_add(s1).wrapping_add(carry);

        let mut output = [0u8; 16];
        output[..8].copy_from_slice(&result0.to_le_bytes());
        output[8..].copy_from_slice(&result1.to_le_bytes());
        output
    }

    /// Construct Poly1305 input for AEAD
    fn construct_poly_input(&self, aad: &[u8], ciphertext: &[u8]) -> Vec<u8> {
        let mut input = Vec::new();

        // AAD
        input.extend_from_slice(aad);
        // Pad to 16 bytes
        let aad_padding = (16 - (aad.len() % 16)) % 16;
        input.extend(core::iter::repeat(0u8).take(aad_padding));

        // Ciphertext
        input.extend_from_slice(ciphertext);
        // Pad to 16 bytes
        let ct_padding = (16 - (ciphertext.len() % 16)) % 16;
        input.extend(core::iter::repeat(0u8).take(ct_padding));

        // Lengths as little-endian 64-bit integers
        input.extend_from_slice(&(aad.len() as u64).to_le_bytes());
        input.extend_from_slice(&(ciphertext.len() as u64).to_le_bytes());

        input
    }
}

impl AeadOps for ChaCha20Poly1305 {
    fn name(&self) -> &str {
        "chacha20-poly1305"
    }

    fn key_size(&self) -> usize {
        32
    }

    fn nonce_size(&self) -> usize {
        12
    }

    fn tag_size(&self) -> usize {
        16
    }

    fn encrypt(
        &self,
        nonce: &[u8],
        aad: &[u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        if nonce.len() != 12 {
            return Err(CryptoError::InvalidNonceSize);
        }

        // Encrypt with ChaCha20 (counter starts at 1 for encryption)
        let mut ciphertext = Vec::with_capacity(plaintext.len() + 16);
        let mut counter = 1u32;
        let mut pos = 0;

        while pos < plaintext.len() {
            let keystream = self.chacha20_block(nonce, counter);
            let block_len = core::cmp::min(64, plaintext.len() - pos);

            for i in 0..block_len {
                ciphertext.push(plaintext[pos + i] ^ keystream[i]);
            }

            counter += 1;
            pos += 64;
        }

        // Generate Poly1305 key
        let poly_key = self.poly1305_key(nonce);

        // Compute tag
        let poly_input = self.construct_poly_input(aad, &ciphertext);
        let tag = self.poly1305_mac(&poly_key, &poly_input);

        // Append tag
        ciphertext.extend_from_slice(&tag);

        Ok(ciphertext)
    }

    fn decrypt(
        &self,
        nonce: &[u8],
        aad: &[u8],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        if nonce.len() != 12 {
            return Err(CryptoError::InvalidNonceSize);
        }

        if ciphertext.len() < 16 {
            return Err(CryptoError::InvalidCiphertext);
        }

        let tag_start = ciphertext.len() - 16;
        let ct = &ciphertext[..tag_start];
        let received_tag = &ciphertext[tag_start..];

        // Generate Poly1305 key
        let poly_key = self.poly1305_key(nonce);

        // Compute expected tag
        let poly_input = self.construct_poly_input(aad, ct);
        let expected_tag = self.poly1305_mac(&poly_key, &poly_input);

        // Constant-time tag comparison
        let mut diff = 0u8;
        for i in 0..16 {
            diff |= expected_tag[i] ^ received_tag[i];
        }
        if diff != 0 {
            return Err(CryptoError::AuthenticationFailed);
        }

        // Decrypt with ChaCha20
        let mut plaintext = Vec::with_capacity(ct.len());
        let mut counter = 1u32;
        let mut pos = 0;

        while pos < ct.len() {
            let keystream = self.chacha20_block(nonce, counter);
            let block_len = core::cmp::min(64, ct.len() - pos);

            for i in 0..block_len {
                plaintext.push(ct[pos + i] ^ keystream[i]);
            }

            counter += 1;
            pos += 64;
        }

        Ok(plaintext)
    }
}

// =============================================================================
// Convenience Functions
// =============================================================================

/// Create AEAD instance by name
pub fn allocate(name: &str, key: &[u8]) -> Result<Box<dyn AeadOps>, CryptoError> {
    match name {
        "aes-128-gcm" => Ok(Box::new(AesGcm::new(key)?)),
        "aes-256-gcm" => Ok(Box::new(AesGcm::new(key)?)),
        "chacha20-poly1305" => Ok(Box::new(ChaCha20Poly1305::new(key)?)),
        _ => Err(CryptoError::AlgorithmNotFound),
    }
}

/// Encrypt with AES-256-GCM
pub fn aes_256_gcm_encrypt(
    key: &[u8],
    nonce: &[u8],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let aead = Aead::new(AeadAlgorithm::Aes256Gcm, key)?;
    aead.encrypt(nonce, aad, plaintext)
}

/// Decrypt with AES-256-GCM
pub fn aes_256_gcm_decrypt(
    key: &[u8],
    nonce: &[u8],
    aad: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let aead = Aead::new(AeadAlgorithm::Aes256Gcm, key)?;
    aead.decrypt(nonce, aad, ciphertext)
}

/// Encrypt with ChaCha20-Poly1305
pub fn chacha20_poly1305_encrypt(
    key: &[u8],
    nonce: &[u8],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let aead = Aead::new(AeadAlgorithm::ChaCha20Poly1305, key)?;
    aead.encrypt(nonce, aad, plaintext)
}

/// Decrypt with ChaCha20-Poly1305
pub fn chacha20_poly1305_decrypt(
    key: &[u8],
    nonce: &[u8],
    aad: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let aead = Aead::new(AeadAlgorithm::ChaCha20Poly1305, key)?;
    aead.decrypt(nonce, aad, ciphertext)
}

/// Sealed box (anonymous encryption)
pub struct SealedBox {
    /// Algorithm
    algorithm: AeadAlgorithm,
}

impl SealedBox {
    /// Create new sealed box
    pub fn new(algorithm: AeadAlgorithm) -> Self {
        Self { algorithm }
    }

    /// Seal (encrypt) data
    pub fn seal(&self, key: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        // Generate random nonce
        let nonce = super::generate_random_bytes(12)?;

        let aead = Aead::new(self.algorithm, key)?;
        let mut output = nonce.clone();
        let ciphertext = aead.encrypt(&nonce, &[], plaintext)?;
        output.extend(ciphertext);

        Ok(output)
    }

    /// Open (decrypt) data
    pub fn open(&self, key: &[u8], sealed: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if sealed.len() < 12 + 16 {
            return Err(CryptoError::InvalidCiphertext);
        }

        let nonce = &sealed[..12];
        let ciphertext = &sealed[12..];

        let aead = Aead::new(self.algorithm, key)?;
        aead.decrypt(nonce, &[], ciphertext)
    }
}
