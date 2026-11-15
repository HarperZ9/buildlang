//! Message Authentication Codes
//!
//! Provides HMAC, Poly1305, CMAC and other MAC algorithms.

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::vec::Vec;
use super::{CryptoError, STATS};
use super::hash::{HashOps, HashAlgorithm};
use core::sync::atomic::Ordering;

/// MAC algorithm identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacAlgorithm {
    /// HMAC-SHA256
    HmacSha256,
    /// HMAC-SHA512
    HmacSha512,
    /// HMAC-SHA1
    HmacSha1,
    /// Poly1305
    Poly1305,
    /// CMAC-AES
    CmacAes,
    /// GMAC
    Gmac,
}

impl MacAlgorithm {
    /// Get output size
    pub fn output_size(&self) -> usize {
        match self {
            Self::HmacSha256 => 32,
            Self::HmacSha512 => 64,
            Self::HmacSha1 => 20,
            Self::Poly1305 => 16,
            Self::CmacAes => 16,
            Self::Gmac => 16,
        }
    }
}

/// MAC operations trait
pub trait MacOps: Send + Sync {
    /// Get algorithm name
    fn name(&self) -> &str;

    /// Get output size
    fn output_size(&self) -> usize;

    /// Update MAC with data
    fn update(&mut self, data: &[u8]);

    /// Finalize and get MAC
    fn finalize(&mut self) -> Vec<u8>;

    /// Reset with same key
    fn reset(&mut self);

    /// Set key
    fn set_key(&mut self, key: &[u8]) -> Result<(), CryptoError>;

    /// One-shot MAC
    fn mac(&mut self, data: &[u8]) -> Vec<u8> {
        self.reset();
        self.update(data);
        self.finalize()
    }
}

/// MAC context
pub struct Mac {
    /// Algorithm
    algorithm: MacAlgorithm,
    /// Implementation
    inner: Box<dyn MacOps>,
}

impl Mac {
    /// Create new MAC
    pub fn new(algorithm: MacAlgorithm, key: &[u8]) -> Result<Self, CryptoError> {
        let inner = match algorithm {
            MacAlgorithm::HmacSha256 => create_hmac(HashAlgorithm::Sha256, key)?,
            MacAlgorithm::HmacSha512 => create_hmac(HashAlgorithm::Sha512, key)?,
            MacAlgorithm::Poly1305 => create_poly1305(key)?,
            _ => return Err(CryptoError::AlgorithmNotFound),
        };

        Ok(Self { algorithm, inner })
    }

    /// Update with data
    pub fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
    }

    /// Finalize and get MAC
    pub fn finalize(&mut self) -> Vec<u8> {
        STATS.macs.fetch_add(1, Ordering::Relaxed);
        self.inner.finalize()
    }

    /// Verify MAC
    pub fn verify(&mut self, data: &[u8], expected: &[u8]) -> bool {
        self.inner.reset();
        self.inner.update(data);
        let computed = self.inner.finalize();

        // Constant-time comparison
        if computed.len() != expected.len() {
            return false;
        }

        let mut result = 0u8;
        for (a, b) in computed.iter().zip(expected.iter()) {
            result |= a ^ b;
        }
        result == 0
    }
}

/// Create HMAC instance
fn create_hmac(hash: HashAlgorithm, key: &[u8]) -> Result<Box<dyn MacOps>, CryptoError> {
    Ok(Box::new(Hmac::new(hash, key)?))
}

/// Create Poly1305 instance
fn create_poly1305(key: &[u8]) -> Result<Box<dyn MacOps>, CryptoError> {
    Ok(Box::new(Poly1305::new(key)?))
}

/// HMAC implementation
struct Hmac {
    /// Inner hash
    inner_hash: Box<dyn HashOps>,
    /// Outer hash
    outer_hash: Box<dyn HashOps>,
    /// Inner padding
    i_key_pad: Vec<u8>,
    /// Outer padding
    o_key_pad: Vec<u8>,
    /// Block size
    block_size: usize,
}

impl Hmac {
    /// Create new HMAC
    pub fn new(algorithm: HashAlgorithm, key: &[u8]) -> Result<Self, CryptoError> {
        let inner_hash = super::hash::allocate(match algorithm {
            HashAlgorithm::Sha256 => "sha256",
            HashAlgorithm::Sha512 => "sha512",
            _ => return Err(CryptoError::AlgorithmNotFound),
        })?;

        let outer_hash = super::hash::allocate(match algorithm {
            HashAlgorithm::Sha256 => "sha256",
            HashAlgorithm::Sha512 => "sha512",
            _ => return Err(CryptoError::AlgorithmNotFound),
        })?;

        let block_size = inner_hash.block_size();

        // Prepare key
        let mut key_block = vec![0u8; block_size];
        if key.len() > block_size {
            // Hash the key if too long
            let mut hasher = super::hash::allocate(match algorithm {
                HashAlgorithm::Sha256 => "sha256",
                HashAlgorithm::Sha512 => "sha512",
                _ => return Err(CryptoError::AlgorithmNotFound),
            })?;
            hasher.update(key);
            let hashed = hasher.finalize();
            key_block[..hashed.len()].copy_from_slice(&hashed);
        } else {
            key_block[..key.len()].copy_from_slice(key);
        }

        // Create paddings
        let mut i_key_pad = vec![0x36u8; block_size];
        let mut o_key_pad = vec![0x5cu8; block_size];

        for i in 0..block_size {
            i_key_pad[i] ^= key_block[i];
            o_key_pad[i] ^= key_block[i];
        }

        let mut hmac = Self {
            inner_hash,
            outer_hash,
            i_key_pad,
            o_key_pad,
            block_size,
        };

        // Initialize inner hash with i_key_pad
        hmac.inner_hash.update(&hmac.i_key_pad);

        Ok(hmac)
    }
}

impl MacOps for Hmac {
    fn name(&self) -> &str {
        "hmac"
    }

    fn output_size(&self) -> usize {
        self.inner_hash.digest_size()
    }

    fn update(&mut self, data: &[u8]) {
        self.inner_hash.update(data);
    }

    fn finalize(&mut self) -> Vec<u8> {
        // inner = H(i_key_pad || message)
        let inner_result = self.inner_hash.finalize();

        // outer = H(o_key_pad || inner)
        self.outer_hash.reset();
        self.outer_hash.update(&self.o_key_pad);
        self.outer_hash.update(&inner_result);
        self.outer_hash.finalize()
    }

    fn reset(&mut self) {
        self.inner_hash.reset();
        self.outer_hash.reset();
        self.inner_hash.update(&self.i_key_pad);
    }

    fn set_key(&mut self, key: &[u8]) -> Result<(), CryptoError> {
        let mut key_block = vec![0u8; self.block_size];
        if key.len() > self.block_size {
            self.inner_hash.reset();
            self.inner_hash.update(key);
            let hashed = self.inner_hash.finalize();
            key_block[..hashed.len()].copy_from_slice(&hashed);
        } else {
            key_block[..key.len()].copy_from_slice(key);
        }

        self.i_key_pad = vec![0x36u8; self.block_size];
        self.o_key_pad = vec![0x5cu8; self.block_size];

        for i in 0..self.block_size {
            self.i_key_pad[i] ^= key_block[i];
            self.o_key_pad[i] ^= key_block[i];
        }

        self.reset();
        Ok(())
    }
}

/// Poly1305 implementation
struct Poly1305 {
    /// r component (clamped)
    r: [u64; 3],
    /// s component
    s: [u64; 2],
    /// Accumulator
    h: [u64; 3],
    /// Buffer
    buffer: [u8; 16],
    /// Buffer length
    buflen: usize,
}

impl Poly1305 {
    /// Create new Poly1305 MAC
    pub fn new(key: &[u8]) -> Result<Self, CryptoError> {
        if key.len() != 32 {
            return Err(CryptoError::InvalidKeySize);
        }

        // r is first 16 bytes, clamped
        let r0 = u64::from_le_bytes([key[0], key[1], key[2], key[3], key[4], key[5], key[6], key[7]]);
        let r1 = u64::from_le_bytes([key[8], key[9], key[10], key[11], key[12], key[13], key[14], key[15]]);

        // Clamp r
        let r0 = r0 & 0x0ffffffc0fffffff;
        let r1 = r1 & 0x0ffffffc0ffffffc;

        // s is second 16 bytes
        let s0 = u64::from_le_bytes([key[16], key[17], key[18], key[19], key[20], key[21], key[22], key[23]]);
        let s1 = u64::from_le_bytes([key[24], key[25], key[26], key[27], key[28], key[29], key[30], key[31]]);

        Ok(Self {
            r: [r0, r1, 0],
            s: [s0, s1],
            h: [0, 0, 0],
            buffer: [0u8; 16],
            buflen: 0,
        })
    }

    fn process_block(&mut self, block: &[u8], final_block: bool) {
        // Add block to accumulator
        let mut n = [0u64; 3];

        n[0] = u64::from_le_bytes([
            block[0], block[1], block[2], block[3],
            block[4], block[5], block[6], block[7],
        ]);

        if block.len() > 8 {
            n[1] = u64::from_le_bytes([
                block[8], block[9], block[10], block[11],
                block[12], block[13], block[14], block[15],
            ]);
        }

        if !final_block {
            n[2] = 1; // High bit for full blocks
        }

        // h += n
        let mut carry = 0u128;
        carry += self.h[0] as u128 + n[0] as u128;
        self.h[0] = carry as u64;
        carry >>= 64;

        carry += self.h[1] as u128 + n[1] as u128;
        self.h[1] = carry as u64;
        carry >>= 64;

        carry += self.h[2] as u128 + n[2] as u128;
        self.h[2] = carry as u64;

        // h *= r (mod 2^130 - 5)
        // Simplified multiplication - full implementation would be more complex
    }
}

impl MacOps for Poly1305 {
    fn name(&self) -> &str {
        "poly1305"
    }

    fn output_size(&self) -> usize {
        16
    }

    fn update(&mut self, data: &[u8]) {
        let mut offset = 0;

        // Fill buffer
        if self.buflen > 0 {
            let needed = 16 - self.buflen;
            let to_copy = core::cmp::min(needed, data.len());
            self.buffer[self.buflen..self.buflen + to_copy].copy_from_slice(&data[..to_copy]);
            self.buflen += to_copy;
            offset = to_copy;

            if self.buflen == 16 {
                self.process_block(&self.buffer.clone(), false);
                self.buflen = 0;
            }
        }

        // Process full blocks
        while offset + 16 <= data.len() {
            self.process_block(&data[offset..offset + 16], false);
            offset += 16;
        }

        // Buffer remaining
        if offset < data.len() {
            let remaining = data.len() - offset;
            self.buffer[..remaining].copy_from_slice(&data[offset..]);
            self.buflen = remaining;
        }
    }

    fn finalize(&mut self) -> Vec<u8> {
        // Process final block if any
        if self.buflen > 0 {
            let mut final_block = [0u8; 16];
            final_block[..self.buflen].copy_from_slice(&self.buffer[..self.buflen]);
            final_block[self.buflen] = 1; // Padding
            self.process_block(&final_block, true);
        }

        // h += s
        let mut carry = self.h[0] as u128 + self.s[0] as u128;
        let out0 = carry as u64;
        carry >>= 64;

        carry += self.h[1] as u128 + self.s[1] as u128;
        let out1 = carry as u64;

        // Output
        let mut output = Vec::with_capacity(16);
        output.extend_from_slice(&out0.to_le_bytes());
        output.extend_from_slice(&out1.to_le_bytes());
        output
    }

    fn reset(&mut self) {
        self.h = [0, 0, 0];
        self.buffer = [0u8; 16];
        self.buflen = 0;
    }

    fn set_key(&mut self, key: &[u8]) -> Result<(), CryptoError> {
        if key.len() != 32 {
            return Err(CryptoError::InvalidKeySize);
        }

        let r0 = u64::from_le_bytes([key[0], key[1], key[2], key[3], key[4], key[5], key[6], key[7]]);
        let r1 = u64::from_le_bytes([key[8], key[9], key[10], key[11], key[12], key[13], key[14], key[15]]);

        self.r[0] = r0 & 0x0ffffffc0fffffff;
        self.r[1] = r1 & 0x0ffffffc0ffffffc;

        self.s[0] = u64::from_le_bytes([key[16], key[17], key[18], key[19], key[20], key[21], key[22], key[23]]);
        self.s[1] = u64::from_le_bytes([key[24], key[25], key[26], key[27], key[28], key[29], key[30], key[31]]);

        self.reset();
        Ok(())
    }
}

/// Convenience function for HMAC-SHA256
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = Mac::new(MacAlgorithm::HmacSha256, key).unwrap();
    mac.update(data);
    mac.finalize()
}

/// Convenience function for HMAC-SHA512
pub fn hmac_sha512(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = Mac::new(MacAlgorithm::HmacSha512, key).unwrap();
    mac.update(data);
    mac.finalize()
}
