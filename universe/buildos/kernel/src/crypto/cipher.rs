//! Symmetric Cipher Algorithms
//!
//! Provides AES, ChaCha20, and other symmetric ciphers.

#![allow(dead_code)]

use alloc::boxed::Box;
use super::{CryptoError, STATS};
use core::sync::atomic::Ordering;

/// Cipher algorithm identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CipherAlgorithm {
    /// AES-128
    Aes128,
    /// AES-192
    Aes192,
    /// AES-256
    Aes256,
    /// ChaCha20
    ChaCha20,
    /// Triple DES
    TripleDes,
    /// Blowfish
    Blowfish,
    /// Camellia
    Camellia,
    /// SM4
    Sm4,
}

impl CipherAlgorithm {
    /// Get block size
    pub fn block_size(&self) -> usize {
        match self {
            Self::Aes128 | Self::Aes192 | Self::Aes256 => 16,
            Self::ChaCha20 => 1, // Stream cipher
            Self::TripleDes => 8,
            Self::Blowfish => 8,
            Self::Camellia => 16,
            Self::Sm4 => 16,
        }
    }

    /// Get key size
    pub fn key_size(&self) -> usize {
        match self {
            Self::Aes128 => 16,
            Self::Aes192 => 24,
            Self::Aes256 => 32,
            Self::ChaCha20 => 32,
            Self::TripleDes => 24,
            Self::Blowfish => 56, // Variable, up to 56
            Self::Camellia => 32,
            Self::Sm4 => 16,
        }
    }
}

/// Cipher mode of operation
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CipherMode {
    /// Electronic Codebook
    Ecb,
    /// Cipher Block Chaining
    Cbc,
    /// Counter Mode
    Ctr,
    /// Output Feedback
    Ofb,
    /// Cipher Feedback
    Cfb,
    /// XEX-based Tweaked-codebook mode with ciphertext Stealing
    Xts,
}

impl CipherMode {
    /// Does this mode require padding?
    pub fn needs_padding(&self) -> bool {
        matches!(self, Self::Ecb | Self::Cbc)
    }

    /// Does this mode require an IV?
    pub fn needs_iv(&self) -> bool {
        !matches!(self, Self::Ecb)
    }
}

/// Cipher operations trait
pub trait CipherOps: Send + Sync {
    /// Get algorithm name
    fn name(&self) -> &str;

    /// Get block size
    fn block_size(&self) -> usize;

    /// Encrypt a single block (ECB mode)
    fn encrypt_block(&self, block: &mut [u8]) -> Result<(), CryptoError>;

    /// Decrypt a single block (ECB mode)
    fn decrypt_block(&self, block: &mut [u8]) -> Result<(), CryptoError>;

    /// Encrypt data with IV (for CBC, CTR, etc.)
    fn encrypt(&self, iv: &[u8], data: &mut [u8]) -> Result<(), CryptoError>;

    /// Decrypt data with IV
    fn decrypt(&self, iv: &[u8], data: &mut [u8]) -> Result<(), CryptoError>;

    /// Set the key
    fn set_key(&mut self, key: &[u8]) -> Result<(), CryptoError>;
}

/// Cipher context
pub struct Cipher {
    /// Algorithm
    algorithm: CipherAlgorithm,
    /// Mode
    mode: CipherMode,
    /// Implementation
    inner: Box<dyn CipherOps>,
}

impl Cipher {
    /// Create new cipher
    pub fn new(algorithm: CipherAlgorithm, mode: CipherMode, key: &[u8]) -> Result<Self, CryptoError> {
        let inner = match algorithm {
            CipherAlgorithm::Aes128 | CipherAlgorithm::Aes192 | CipherAlgorithm::Aes256 => {
                create_aes_cipher(key, mode)?
            }
            CipherAlgorithm::ChaCha20 => {
                create_chacha20_cipher(key)?
            }
            _ => return Err(CryptoError::AlgorithmNotFound),
        };

        Ok(Self {
            algorithm,
            mode,
            inner,
        })
    }

    /// Encrypt data
    pub fn encrypt(&self, iv: &[u8], data: &mut [u8]) -> Result<(), CryptoError> {
        STATS.encryptions.fetch_add(1, Ordering::Relaxed);
        self.inner.encrypt(iv, data)
    }

    /// Decrypt data
    pub fn decrypt(&self, iv: &[u8], data: &mut [u8]) -> Result<(), CryptoError> {
        STATS.decryptions.fetch_add(1, Ordering::Relaxed);
        self.inner.decrypt(iv, data)
    }

    /// Get block size
    pub fn block_size(&self) -> usize {
        self.algorithm.block_size()
    }
}

/// Allocate cipher by name
pub fn allocate(name: &str, key: &[u8]) -> Result<Box<dyn CipherOps>, CryptoError> {
    match name {
        "aes" | "aes-256" => create_aes_cipher(key, CipherMode::Ecb),
        "aes-cbc" => create_aes_cipher(key, CipherMode::Cbc),
        "aes-ctr" => create_aes_cipher(key, CipherMode::Ctr),
        "chacha20" => create_chacha20_cipher(key),
        _ => Err(CryptoError::AlgorithmNotFound),
    }
}

/// Create AES cipher implementation
fn create_aes_cipher(key: &[u8], mode: CipherMode) -> Result<Box<dyn CipherOps>, CryptoError> {
    // Check key size
    if key.len() != 16 && key.len() != 24 && key.len() != 32 {
        return Err(CryptoError::InvalidKeySize);
    }

    Ok(Box::new(AesCipher::new(key, mode)?))
}

/// Create ChaCha20 cipher
fn create_chacha20_cipher(key: &[u8]) -> Result<Box<dyn CipherOps>, CryptoError> {
    if key.len() != 32 {
        return Err(CryptoError::InvalidKeySize);
    }

    Ok(Box::new(ChaCha20Cipher::new(key)?))
}

/// AES cipher implementation
struct AesCipher {
    /// Round keys (expanded)
    round_keys: [[u32; 4]; 15],
    /// Number of rounds
    rounds: usize,
    /// Mode
    mode: CipherMode,
}

impl AesCipher {
    /// Create new AES cipher
    pub fn new(key: &[u8], mode: CipherMode) -> Result<Self, CryptoError> {
        let rounds = match key.len() {
            16 => 10,
            24 => 12,
            32 => 14,
            _ => return Err(CryptoError::InvalidKeySize),
        };

        let mut cipher = Self {
            round_keys: [[0u32; 4]; 15],
            rounds,
            mode,
        };

        cipher.expand_key(key);
        Ok(cipher)
    }

    /// Key expansion
    fn expand_key(&mut self, key: &[u8]) {
        let nk = key.len() / 4;

        // First Nk words are the key itself
        for i in 0..nk {
            self.round_keys[i / 4][i % 4] = u32::from_be_bytes([
                key[4 * i],
                key[4 * i + 1],
                key[4 * i + 2],
                key[4 * i + 3],
            ]);
        }

        // Expand remaining round keys
        for i in nk..(4 * (self.rounds + 1)) {
            let mut temp = self.round_keys[(i - 1) / 4][(i - 1) % 4];

            if i % nk == 0 {
                temp = self.sub_word(self.rot_word(temp)) ^ RCON[i / nk - 1];
            } else if nk > 6 && i % nk == 4 {
                temp = self.sub_word(temp);
            }

            self.round_keys[i / 4][i % 4] =
                self.round_keys[(i - nk) / 4][(i - nk) % 4] ^ temp;
        }
    }

    /// Rotate word
    fn rot_word(&self, word: u32) -> u32 {
        (word << 8) | (word >> 24)
    }

    /// Substitute word using S-box
    fn sub_word(&self, word: u32) -> u32 {
        let bytes = word.to_be_bytes();
        let subbed = [
            SBOX[bytes[0] as usize],
            SBOX[bytes[1] as usize],
            SBOX[bytes[2] as usize],
            SBOX[bytes[3] as usize],
        ];
        u32::from_be_bytes(subbed)
    }

    /// Encrypt a single block
    fn encrypt_block_impl(&self, block: &mut [u8; 16]) {
        let mut state = [[0u8; 4]; 4];

        // Copy to state
        for i in 0..4 {
            for j in 0..4 {
                state[j][i] = block[i * 4 + j];
            }
        }

        // Initial round
        self.add_round_key(&mut state, 0);

        // Main rounds
        for round in 1..self.rounds {
            self.sub_bytes(&mut state);
            self.shift_rows(&mut state);
            self.mix_columns(&mut state);
            self.add_round_key(&mut state, round);
        }

        // Final round
        self.sub_bytes(&mut state);
        self.shift_rows(&mut state);
        self.add_round_key(&mut state, self.rounds);

        // Copy back
        for i in 0..4 {
            for j in 0..4 {
                block[i * 4 + j] = state[j][i];
            }
        }
    }

    /// Decrypt a single block
    fn decrypt_block_impl(&self, block: &mut [u8; 16]) {
        let mut state = [[0u8; 4]; 4];

        // Copy to state
        for i in 0..4 {
            for j in 0..4 {
                state[j][i] = block[i * 4 + j];
            }
        }

        // Initial round
        self.add_round_key(&mut state, self.rounds);

        // Main rounds
        for round in (1..self.rounds).rev() {
            self.inv_shift_rows(&mut state);
            self.inv_sub_bytes(&mut state);
            self.add_round_key(&mut state, round);
            self.inv_mix_columns(&mut state);
        }

        // Final round
        self.inv_shift_rows(&mut state);
        self.inv_sub_bytes(&mut state);
        self.add_round_key(&mut state, 0);

        // Copy back
        for i in 0..4 {
            for j in 0..4 {
                block[i * 4 + j] = state[j][i];
            }
        }
    }

    fn add_round_key(&self, state: &mut [[u8; 4]; 4], round: usize) {
        for i in 0..4 {
            let key_word = self.round_keys[round][i].to_be_bytes();
            for j in 0..4 {
                state[j][i] ^= key_word[j];
            }
        }
    }

    fn sub_bytes(&self, state: &mut [[u8; 4]; 4]) {
        for i in 0..4 {
            for j in 0..4 {
                state[i][j] = SBOX[state[i][j] as usize];
            }
        }
    }

    fn inv_sub_bytes(&self, state: &mut [[u8; 4]; 4]) {
        for i in 0..4 {
            for j in 0..4 {
                state[i][j] = INV_SBOX[state[i][j] as usize];
            }
        }
    }

    fn shift_rows(&self, state: &mut [[u8; 4]; 4]) {
        // Row 1: shift left by 1
        let temp = state[1][0];
        state[1][0] = state[1][1];
        state[1][1] = state[1][2];
        state[1][2] = state[1][3];
        state[1][3] = temp;

        // Row 2: shift left by 2
        let temp0 = state[2][0];
        let temp1 = state[2][1];
        state[2][0] = state[2][2];
        state[2][1] = state[2][3];
        state[2][2] = temp0;
        state[2][3] = temp1;

        // Row 3: shift left by 3
        let temp = state[3][3];
        state[3][3] = state[3][2];
        state[3][2] = state[3][1];
        state[3][1] = state[3][0];
        state[3][0] = temp;
    }

    fn inv_shift_rows(&self, state: &mut [[u8; 4]; 4]) {
        // Row 1: shift right by 1
        let temp = state[1][3];
        state[1][3] = state[1][2];
        state[1][2] = state[1][1];
        state[1][1] = state[1][0];
        state[1][0] = temp;

        // Row 2: shift right by 2
        let temp0 = state[2][0];
        let temp1 = state[2][1];
        state[2][0] = state[2][2];
        state[2][1] = state[2][3];
        state[2][2] = temp0;
        state[2][3] = temp1;

        // Row 3: shift right by 3
        let temp = state[3][0];
        state[3][0] = state[3][1];
        state[3][1] = state[3][2];
        state[3][2] = state[3][3];
        state[3][3] = temp;
    }

    fn mix_columns(&self, state: &mut [[u8; 4]; 4]) {
        for i in 0..4 {
            let a = state[0][i];
            let b = state[1][i];
            let c = state[2][i];
            let d = state[3][i];

            state[0][i] = gmul(a, 2) ^ gmul(b, 3) ^ c ^ d;
            state[1][i] = a ^ gmul(b, 2) ^ gmul(c, 3) ^ d;
            state[2][i] = a ^ b ^ gmul(c, 2) ^ gmul(d, 3);
            state[3][i] = gmul(a, 3) ^ b ^ c ^ gmul(d, 2);
        }
    }

    fn inv_mix_columns(&self, state: &mut [[u8; 4]; 4]) {
        for i in 0..4 {
            let a = state[0][i];
            let b = state[1][i];
            let c = state[2][i];
            let d = state[3][i];

            state[0][i] = gmul(a, 0x0e) ^ gmul(b, 0x0b) ^ gmul(c, 0x0d) ^ gmul(d, 0x09);
            state[1][i] = gmul(a, 0x09) ^ gmul(b, 0x0e) ^ gmul(c, 0x0b) ^ gmul(d, 0x0d);
            state[2][i] = gmul(a, 0x0d) ^ gmul(b, 0x09) ^ gmul(c, 0x0e) ^ gmul(d, 0x0b);
            state[3][i] = gmul(a, 0x0b) ^ gmul(b, 0x0d) ^ gmul(c, 0x09) ^ gmul(d, 0x0e);
        }
    }
}

impl CipherOps for AesCipher {
    fn name(&self) -> &str {
        "aes"
    }

    fn block_size(&self) -> usize {
        16
    }

    fn encrypt_block(&self, block: &mut [u8]) -> Result<(), CryptoError> {
        if block.len() != 16 {
            return Err(CryptoError::InvalidInputSize);
        }

        let mut block_arr = [0u8; 16];
        block_arr.copy_from_slice(block);
        self.encrypt_block_impl(&mut block_arr);
        block.copy_from_slice(&block_arr);

        Ok(())
    }

    fn decrypt_block(&self, block: &mut [u8]) -> Result<(), CryptoError> {
        if block.len() != 16 {
            return Err(CryptoError::InvalidInputSize);
        }

        let mut block_arr = [0u8; 16];
        block_arr.copy_from_slice(block);
        self.decrypt_block_impl(&mut block_arr);
        block.copy_from_slice(&block_arr);

        Ok(())
    }

    fn encrypt(&self, iv: &[u8], data: &mut [u8]) -> Result<(), CryptoError> {
        match self.mode {
            CipherMode::Ecb => {
                for chunk in data.chunks_mut(16) {
                    if chunk.len() == 16 {
                        self.encrypt_block(chunk)?;
                    }
                }
                Ok(())
            }
            CipherMode::Cbc => {
                if iv.len() != 16 {
                    return Err(CryptoError::InvalidIvSize);
                }

                let mut prev = [0u8; 16];
                prev.copy_from_slice(iv);

                for chunk in data.chunks_mut(16) {
                    if chunk.len() == 16 {
                        for i in 0..16 {
                            chunk[i] ^= prev[i];
                        }
                        self.encrypt_block(chunk)?;
                        prev.copy_from_slice(chunk);
                    }
                }
                Ok(())
            }
            CipherMode::Ctr => {
                if iv.len() != 16 {
                    return Err(CryptoError::InvalidIvSize);
                }

                let mut counter = [0u8; 16];
                counter.copy_from_slice(iv);

                for chunk in data.chunks_mut(16) {
                    let keystream = counter;
                    let mut ks = [0u8; 16];
                    ks.copy_from_slice(&keystream);
                    self.encrypt_block_impl(&mut ks);

                    for (i, byte) in chunk.iter_mut().enumerate() {
                        *byte ^= ks[i];
                    }

                    // Increment counter
                    for i in (0..16).rev() {
                        counter[i] = counter[i].wrapping_add(1);
                        if counter[i] != 0 {
                            break;
                        }
                    }
                }
                Ok(())
            }
            _ => Err(CryptoError::AlgorithmNotFound),
        }
    }

    fn decrypt(&self, iv: &[u8], data: &mut [u8]) -> Result<(), CryptoError> {
        match self.mode {
            CipherMode::Ecb => {
                for chunk in data.chunks_mut(16) {
                    if chunk.len() == 16 {
                        self.decrypt_block(chunk)?;
                    }
                }
                Ok(())
            }
            CipherMode::Cbc => {
                if iv.len() != 16 {
                    return Err(CryptoError::InvalidIvSize);
                }

                let mut prev = [0u8; 16];
                prev.copy_from_slice(iv);

                for chunk in data.chunks_mut(16) {
                    if chunk.len() == 16 {
                        let mut temp = [0u8; 16];
                        temp.copy_from_slice(chunk);

                        self.decrypt_block(chunk)?;

                        for i in 0..16 {
                            chunk[i] ^= prev[i];
                        }
                        prev.copy_from_slice(&temp);
                    }
                }
                Ok(())
            }
            CipherMode::Ctr => {
                // CTR mode decryption is same as encryption
                self.encrypt(iv, data)
            }
            _ => Err(CryptoError::AlgorithmNotFound),
        }
    }

    fn set_key(&mut self, key: &[u8]) -> Result<(), CryptoError> {
        self.rounds = match key.len() {
            16 => 10,
            24 => 12,
            32 => 14,
            _ => return Err(CryptoError::InvalidKeySize),
        };
        self.expand_key(key);
        Ok(())
    }
}

/// ChaCha20 cipher implementation
struct ChaCha20Cipher {
    /// Key
    key: [u32; 8],
}

impl ChaCha20Cipher {
    /// Create new ChaCha20 cipher
    pub fn new(key: &[u8]) -> Result<Self, CryptoError> {
        if key.len() != 32 {
            return Err(CryptoError::InvalidKeySize);
        }

        let mut key_words = [0u32; 8];
        for i in 0..8 {
            key_words[i] = u32::from_le_bytes([
                key[i * 4],
                key[i * 4 + 1],
                key[i * 4 + 2],
                key[i * 4 + 3],
            ]);
        }

        Ok(Self { key: key_words })
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

    /// Generate keystream block
    fn keystream_block(&self, nonce: &[u8], counter: u32) -> [u8; 64] {
        // ChaCha20 state initialization
        let mut state = [0u32; 16];

        // Constants "expand 32-byte k"
        state[0] = 0x61707865;
        state[1] = 0x3320646e;
        state[2] = 0x79622d32;
        state[3] = 0x6b206574;

        // Key
        for i in 0..8 {
            state[4 + i] = self.key[i];
        }

        // Counter
        state[12] = counter;

        // Nonce
        state[13] = u32::from_le_bytes([nonce[0], nonce[1], nonce[2], nonce[3]]);
        state[14] = u32::from_le_bytes([nonce[4], nonce[5], nonce[6], nonce[7]]);
        state[15] = u32::from_le_bytes([nonce[8], nonce[9], nonce[10], nonce[11]]);

        let mut working = state;

        // 20 rounds (10 double rounds)
        for _ in 0..10 {
            // Column rounds
            Self::quarter_round(&mut working, 0, 4, 8, 12);
            Self::quarter_round(&mut working, 1, 5, 9, 13);
            Self::quarter_round(&mut working, 2, 6, 10, 14);
            Self::quarter_round(&mut working, 3, 7, 11, 15);

            // Diagonal rounds
            Self::quarter_round(&mut working, 0, 5, 10, 15);
            Self::quarter_round(&mut working, 1, 6, 11, 12);
            Self::quarter_round(&mut working, 2, 7, 8, 13);
            Self::quarter_round(&mut working, 3, 4, 9, 14);
        }

        // Add original state
        for i in 0..16 {
            working[i] = working[i].wrapping_add(state[i]);
        }

        // Serialize
        let mut output = [0u8; 64];
        for i in 0..16 {
            output[i * 4..i * 4 + 4].copy_from_slice(&working[i].to_le_bytes());
        }

        output
    }
}

impl CipherOps for ChaCha20Cipher {
    fn name(&self) -> &str {
        "chacha20"
    }

    fn block_size(&self) -> usize {
        1 // Stream cipher
    }

    fn encrypt_block(&self, _block: &mut [u8]) -> Result<(), CryptoError> {
        Err(CryptoError::AlgorithmNotFound) // Stream cipher, no block encrypt
    }

    fn decrypt_block(&self, _block: &mut [u8]) -> Result<(), CryptoError> {
        Err(CryptoError::AlgorithmNotFound) // Stream cipher, no block decrypt
    }

    fn encrypt(&self, iv: &[u8], data: &mut [u8]) -> Result<(), CryptoError> {
        if iv.len() != 16 {
            return Err(CryptoError::InvalidIvSize);
        }

        // First 4 bytes of IV are initial counter, rest is nonce
        let initial_counter = u32::from_le_bytes([iv[0], iv[1], iv[2], iv[3]]);
        let nonce = &iv[4..16];

        let mut counter = initial_counter;

        for chunk in data.chunks_mut(64) {
            let keystream = self.keystream_block(nonce, counter);

            for (i, byte) in chunk.iter_mut().enumerate() {
                *byte ^= keystream[i];
            }

            counter = counter.wrapping_add(1);
        }

        Ok(())
    }

    fn decrypt(&self, iv: &[u8], data: &mut [u8]) -> Result<(), CryptoError> {
        // ChaCha20 decryption is same as encryption
        self.encrypt(iv, data)
    }

    fn set_key(&mut self, key: &[u8]) -> Result<(), CryptoError> {
        if key.len() != 32 {
            return Err(CryptoError::InvalidKeySize);
        }

        for i in 0..8 {
            self.key[i] = u32::from_le_bytes([
                key[i * 4],
                key[i * 4 + 1],
                key[i * 4 + 2],
                key[i * 4 + 3],
            ]);
        }

        Ok(())
    }
}

/// GF(2^8) multiplication
fn gmul(a: u8, b: u8) -> u8 {
    let mut result = 0u8;
    let mut a = a;
    let mut b = b;

    for _ in 0..8 {
        if b & 1 != 0 {
            result ^= a;
        }
        let hi_bit = a & 0x80;
        a <<= 1;
        if hi_bit != 0 {
            a ^= 0x1b; // x^8 + x^4 + x^3 + x + 1
        }
        b >>= 1;
    }

    result
}

/// AES S-Box
static SBOX: [u8; 256] = [
    0x63, 0x7c, 0x77, 0x7b, 0xf2, 0x6b, 0x6f, 0xc5, 0x30, 0x01, 0x67, 0x2b, 0xfe, 0xd7, 0xab, 0x76,
    0xca, 0x82, 0xc9, 0x7d, 0xfa, 0x59, 0x47, 0xf0, 0xad, 0xd4, 0xa2, 0xaf, 0x9c, 0xa4, 0x72, 0xc0,
    0xb7, 0xfd, 0x93, 0x26, 0x36, 0x3f, 0xf7, 0xcc, 0x34, 0xa5, 0xe5, 0xf1, 0x71, 0xd8, 0x31, 0x15,
    0x04, 0xc7, 0x23, 0xc3, 0x18, 0x96, 0x05, 0x9a, 0x07, 0x12, 0x80, 0xe2, 0xeb, 0x27, 0xb2, 0x75,
    0x09, 0x83, 0x2c, 0x1a, 0x1b, 0x6e, 0x5a, 0xa0, 0x52, 0x3b, 0xd6, 0xb3, 0x29, 0xe3, 0x2f, 0x84,
    0x53, 0xd1, 0x00, 0xed, 0x20, 0xfc, 0xb1, 0x5b, 0x6a, 0xcb, 0xbe, 0x39, 0x4a, 0x4c, 0x58, 0xcf,
    0xd0, 0xef, 0xaa, 0xfb, 0x43, 0x4d, 0x33, 0x85, 0x45, 0xf9, 0x02, 0x7f, 0x50, 0x3c, 0x9f, 0xa8,
    0x51, 0xa3, 0x40, 0x8f, 0x92, 0x9d, 0x38, 0xf5, 0xbc, 0xb6, 0xda, 0x21, 0x10, 0xff, 0xf3, 0xd2,
    0xcd, 0x0c, 0x13, 0xec, 0x5f, 0x97, 0x44, 0x17, 0xc4, 0xa7, 0x7e, 0x3d, 0x64, 0x5d, 0x19, 0x73,
    0x60, 0x81, 0x4f, 0xdc, 0x22, 0x2a, 0x90, 0x88, 0x46, 0xee, 0xb8, 0x14, 0xde, 0x5e, 0x0b, 0xdb,
    0xe0, 0x32, 0x3a, 0x0a, 0x49, 0x06, 0x24, 0x5c, 0xc2, 0xd3, 0xac, 0x62, 0x91, 0x95, 0xe4, 0x79,
    0xe7, 0xc8, 0x37, 0x6d, 0x8d, 0xd5, 0x4e, 0xa9, 0x6c, 0x56, 0xf4, 0xea, 0x65, 0x7a, 0xae, 0x08,
    0xba, 0x78, 0x25, 0x2e, 0x1c, 0xa6, 0xb4, 0xc6, 0xe8, 0xdd, 0x74, 0x1f, 0x4b, 0xbd, 0x8b, 0x8a,
    0x70, 0x3e, 0xb5, 0x66, 0x48, 0x03, 0xf6, 0x0e, 0x61, 0x35, 0x57, 0xb9, 0x86, 0xc1, 0x1d, 0x9e,
    0xe1, 0xf8, 0x98, 0x11, 0x69, 0xd9, 0x8e, 0x94, 0x9b, 0x1e, 0x87, 0xe9, 0xce, 0x55, 0x28, 0xdf,
    0x8c, 0xa1, 0x89, 0x0d, 0xbf, 0xe6, 0x42, 0x68, 0x41, 0x99, 0x2d, 0x0f, 0xb0, 0x54, 0xbb, 0x16,
];

/// AES Inverse S-Box
static INV_SBOX: [u8; 256] = [
    0x52, 0x09, 0x6a, 0xd5, 0x30, 0x36, 0xa5, 0x38, 0xbf, 0x40, 0xa3, 0x9e, 0x81, 0xf3, 0xd7, 0xfb,
    0x7c, 0xe3, 0x39, 0x82, 0x9b, 0x2f, 0xff, 0x87, 0x34, 0x8e, 0x43, 0x44, 0xc4, 0xde, 0xe9, 0xcb,
    0x54, 0x7b, 0x94, 0x32, 0xa6, 0xc2, 0x23, 0x3d, 0xee, 0x4c, 0x95, 0x0b, 0x42, 0xfa, 0xc3, 0x4e,
    0x08, 0x2e, 0xa1, 0x66, 0x28, 0xd9, 0x24, 0xb2, 0x76, 0x5b, 0xa2, 0x49, 0x6d, 0x8b, 0xd1, 0x25,
    0x72, 0xf8, 0xf6, 0x64, 0x86, 0x68, 0x98, 0x16, 0xd4, 0xa4, 0x5c, 0xcc, 0x5d, 0x65, 0xb6, 0x92,
    0x6c, 0x70, 0x48, 0x50, 0xfd, 0xed, 0xb9, 0xda, 0x5e, 0x15, 0x46, 0x57, 0xa7, 0x8d, 0x9d, 0x84,
    0x90, 0xd8, 0xab, 0x00, 0x8c, 0xbc, 0xd3, 0x0a, 0xf7, 0xe4, 0x58, 0x05, 0xb8, 0xb3, 0x45, 0x06,
    0xd0, 0x2c, 0x1e, 0x8f, 0xca, 0x3f, 0x0f, 0x02, 0xc1, 0xaf, 0xbd, 0x03, 0x01, 0x13, 0x8a, 0x6b,
    0x3a, 0x91, 0x11, 0x41, 0x4f, 0x67, 0xdc, 0xea, 0x97, 0xf2, 0xcf, 0xce, 0xf0, 0xb4, 0xe6, 0x73,
    0x96, 0xac, 0x74, 0x22, 0xe7, 0xad, 0x35, 0x85, 0xe2, 0xf9, 0x37, 0xe8, 0x1c, 0x75, 0xdf, 0x6e,
    0x47, 0xf1, 0x1a, 0x71, 0x1d, 0x29, 0xc5, 0x89, 0x6f, 0xb7, 0x62, 0x0e, 0xaa, 0x18, 0xbe, 0x1b,
    0xfc, 0x56, 0x3e, 0x4b, 0xc6, 0xd2, 0x79, 0x20, 0x9a, 0xdb, 0xc0, 0xfe, 0x78, 0xcd, 0x5a, 0xf4,
    0x1f, 0xdd, 0xa8, 0x33, 0x88, 0x07, 0xc7, 0x31, 0xb1, 0x12, 0x10, 0x59, 0x27, 0x80, 0xec, 0x5f,
    0x60, 0x51, 0x7f, 0xa9, 0x19, 0xb5, 0x4a, 0x0d, 0x2d, 0xe5, 0x7a, 0x9f, 0x93, 0xc9, 0x9c, 0xef,
    0xa0, 0xe0, 0x3b, 0x4d, 0xae, 0x2a, 0xf5, 0xb0, 0xc8, 0xeb, 0xbb, 0x3c, 0x83, 0x53, 0x99, 0x61,
    0x17, 0x2b, 0x04, 0x7e, 0xba, 0x77, 0xd6, 0x26, 0xe1, 0x69, 0x14, 0x63, 0x55, 0x21, 0x0c, 0x7d,
];

/// AES round constants
static RCON: [u32; 10] = [
    0x01000000, 0x02000000, 0x04000000, 0x08000000, 0x10000000,
    0x20000000, 0x40000000, 0x80000000, 0x1b000000, 0x36000000,
];
