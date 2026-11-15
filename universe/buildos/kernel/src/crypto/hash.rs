//! Hash Algorithms
//!
//! Provides SHA-256, SHA-512, BLAKE2, and other hash functions.

use alloc::boxed::Box;
use alloc::vec::Vec;
use super::{CryptoError, STATS};
use core::sync::atomic::Ordering;

/// Hash algorithm identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HashAlgorithm {
    /// SHA-1 (deprecated, for legacy use)
    Sha1,
    /// SHA-224
    Sha224,
    /// SHA-256
    Sha256,
    /// SHA-384
    Sha384,
    /// SHA-512
    Sha512,
    /// SHA-512/256
    Sha512_256,
    /// SHA3-256
    Sha3_256,
    /// SHA3-512
    Sha3_512,
    /// BLAKE2b-256
    Blake2b256,
    /// BLAKE2b-512
    Blake2b512,
    /// BLAKE2s-256
    Blake2s256,
    /// MD5 (insecure, for legacy use)
    Md5,
    /// SM3
    Sm3,
}

impl HashAlgorithm {
    /// Get digest size in bytes
    pub fn digest_size(&self) -> usize {
        match self {
            Self::Sha1 => 20,
            Self::Sha224 => 28,
            Self::Sha256 => 32,
            Self::Sha384 => 48,
            Self::Sha512 => 64,
            Self::Sha512_256 => 32,
            Self::Sha3_256 => 32,
            Self::Sha3_512 => 64,
            Self::Blake2b256 => 32,
            Self::Blake2b512 => 64,
            Self::Blake2s256 => 32,
            Self::Md5 => 16,
            Self::Sm3 => 32,
        }
    }

    /// Get block size in bytes
    pub fn block_size(&self) -> usize {
        match self {
            Self::Sha1 | Self::Sha224 | Self::Sha256 => 64,
            Self::Sha384 | Self::Sha512 | Self::Sha512_256 => 128,
            Self::Sha3_256 => 136,
            Self::Sha3_512 => 72,
            Self::Blake2b256 | Self::Blake2b512 => 128,
            Self::Blake2s256 => 64,
            Self::Md5 => 64,
            Self::Sm3 => 64,
        }
    }
}

/// Fixed-size digest
#[derive(Clone, Debug)]
pub struct Digest {
    /// Digest bytes
    pub bytes: Vec<u8>,
    /// Algorithm used
    pub algorithm: HashAlgorithm,
}

impl Digest {
    /// Create new digest
    pub fn new(algorithm: HashAlgorithm, bytes: Vec<u8>) -> Self {
        Self { bytes, algorithm }
    }

    /// Get digest as hex string
    pub fn to_hex(&self) -> alloc::string::String {
        use alloc::format;
        self.bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Compare digests in constant time
    pub fn ct_eq(&self, other: &Digest) -> bool {
        if self.bytes.len() != other.bytes.len() {
            return false;
        }

        let mut result = 0u8;
        for (a, b) in self.bytes.iter().zip(other.bytes.iter()) {
            result |= a ^ b;
        }
        result == 0
    }
}

/// Hash operations trait
pub trait HashOps: Send + Sync {
    /// Get algorithm name
    fn name(&self) -> &str;

    /// Get digest size
    fn digest_size(&self) -> usize;

    /// Get block size
    fn block_size(&self) -> usize;

    /// Update hash with data
    fn update(&mut self, data: &[u8]);

    /// Finalize and get digest
    fn finalize(&mut self) -> Vec<u8>;

    /// Reset to initial state
    fn reset(&mut self);

    /// One-shot hash
    fn digest(&mut self, data: &[u8]) -> Vec<u8> {
        self.reset();
        self.update(data);
        self.finalize()
    }
}

/// Hash context
pub struct Hash {
    /// Algorithm
    algorithm: HashAlgorithm,
    /// Implementation
    inner: Box<dyn HashOps>,
}

impl Hash {
    /// Create new hash
    pub fn new(algorithm: HashAlgorithm) -> Result<Self, CryptoError> {
        let inner = match algorithm {
            HashAlgorithm::Sha256 => create_sha256()?,
            HashAlgorithm::Sha512 => create_sha512()?,
            HashAlgorithm::Blake2b256 => create_blake2b(32)?,
            HashAlgorithm::Blake2s256 => create_blake2s(32)?,
            _ => return Err(CryptoError::AlgorithmNotFound),
        };

        Ok(Self { algorithm, inner })
    }

    /// Update with data
    pub fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
    }

    /// Finalize and get digest
    pub fn finalize(&mut self) -> Digest {
        STATS.hashes.fetch_add(1, Ordering::Relaxed);
        Digest::new(self.algorithm, self.inner.finalize())
    }

    /// Reset hash
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    /// One-shot digest
    pub fn digest(&mut self, data: &[u8]) -> Digest {
        STATS.hashes.fetch_add(1, Ordering::Relaxed);
        Digest::new(self.algorithm, self.inner.digest(data))
    }
}

/// Allocate hash by name
pub fn allocate(name: &str) -> Result<Box<dyn HashOps>, CryptoError> {
    match name {
        "sha256" => create_sha256(),
        "sha512" => create_sha512(),
        "blake2b-256" => create_blake2b(32),
        "blake2s-256" => create_blake2s(32),
        _ => Err(CryptoError::AlgorithmNotFound),
    }
}

/// Convenience function for SHA-256
pub fn sha256(data: &[u8]) -> Digest {
    let mut hash = Hash::new(HashAlgorithm::Sha256).unwrap();
    hash.digest(data)
}

/// Convenience function for SHA-512
pub fn sha512(data: &[u8]) -> Digest {
    let mut hash = Hash::new(HashAlgorithm::Sha512).unwrap();
    hash.digest(data)
}

/// Create SHA-256 implementation
fn create_sha256() -> Result<Box<dyn HashOps>, CryptoError> {
    Ok(Box::new(Sha256::new()))
}

/// Create SHA-512 implementation
fn create_sha512() -> Result<Box<dyn HashOps>, CryptoError> {
    Ok(Box::new(Sha512::new()))
}

/// Create BLAKE2b implementation
fn create_blake2b(digest_size: usize) -> Result<Box<dyn HashOps>, CryptoError> {
    Ok(Box::new(Blake2b::new(digest_size)))
}

/// Create BLAKE2s implementation
fn create_blake2s(digest_size: usize) -> Result<Box<dyn HashOps>, CryptoError> {
    Ok(Box::new(Blake2s::new(digest_size)))
}

/// SHA-256 implementation
struct Sha256 {
    /// State
    h: [u32; 8],
    /// Buffer
    buffer: [u8; 64],
    /// Buffer position
    buflen: usize,
    /// Total bytes processed
    total: u64,
}

impl Sha256 {
    /// SHA-256 initial hash values
    const H0: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];

    /// SHA-256 round constants
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    pub fn new() -> Self {
        Self {
            h: Self::H0,
            buffer: [0u8; 64],
            buflen: 0,
            total: 0,
        }
    }

    fn process_block(&mut self, block: &[u8]) {
        let mut w = [0u32; 64];

        // Prepare message schedule
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }

        for i in 16..64 {
            let s0 = w[i-15].rotate_right(7) ^ w[i-15].rotate_right(18) ^ (w[i-15] >> 3);
            let s1 = w[i-2].rotate_right(17) ^ w[i-2].rotate_right(19) ^ (w[i-2] >> 10);
            w[i] = w[i-16].wrapping_add(s0).wrapping_add(w[i-7]).wrapping_add(s1);
        }

        // Initialize working variables
        let mut a = self.h[0];
        let mut b = self.h[1];
        let mut c = self.h[2];
        let mut d = self.h[3];
        let mut e = self.h[4];
        let mut f = self.h[5];
        let mut g = self.h[6];
        let mut h = self.h[7];

        // Main loop
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(Self::K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        // Update state
        self.h[0] = self.h[0].wrapping_add(a);
        self.h[1] = self.h[1].wrapping_add(b);
        self.h[2] = self.h[2].wrapping_add(c);
        self.h[3] = self.h[3].wrapping_add(d);
        self.h[4] = self.h[4].wrapping_add(e);
        self.h[5] = self.h[5].wrapping_add(f);
        self.h[6] = self.h[6].wrapping_add(g);
        self.h[7] = self.h[7].wrapping_add(h);
    }
}

impl HashOps for Sha256 {
    fn name(&self) -> &str { "sha256" }
    fn digest_size(&self) -> usize { 32 }
    fn block_size(&self) -> usize { 64 }

    fn update(&mut self, data: &[u8]) {
        let mut offset = 0;

        // Fill buffer
        if self.buflen > 0 {
            let needed = 64 - self.buflen;
            let to_copy = core::cmp::min(needed, data.len());
            self.buffer[self.buflen..self.buflen + to_copy].copy_from_slice(&data[..to_copy]);
            self.buflen += to_copy;
            offset = to_copy;

            if self.buflen == 64 {
                self.process_block(&self.buffer.clone());
                self.buflen = 0;
            }
        }

        // Process full blocks
        while offset + 64 <= data.len() {
            self.process_block(&data[offset..offset + 64]);
            offset += 64;
        }

        // Buffer remaining
        if offset < data.len() {
            let remaining = data.len() - offset;
            self.buffer[..remaining].copy_from_slice(&data[offset..]);
            self.buflen = remaining;
        }

        self.total += data.len() as u64;
    }

    fn finalize(&mut self) -> Vec<u8> {
        // Padding
        let total_bits = self.total * 8;

        // Add 0x80
        self.buffer[self.buflen] = 0x80;
        self.buflen += 1;

        // Pad with zeros
        if self.buflen > 56 {
            while self.buflen < 64 {
                self.buffer[self.buflen] = 0;
                self.buflen += 1;
            }
            self.process_block(&self.buffer.clone());
            self.buflen = 0;
        }

        while self.buflen < 56 {
            self.buffer[self.buflen] = 0;
            self.buflen += 1;
        }

        // Append length
        self.buffer[56..64].copy_from_slice(&total_bits.to_be_bytes());
        self.process_block(&self.buffer.clone());

        // Output
        let mut output = Vec::with_capacity(32);
        for word in &self.h {
            output.extend_from_slice(&word.to_be_bytes());
        }
        output
    }

    fn reset(&mut self) {
        self.h = Self::H0;
        self.buffer = [0u8; 64];
        self.buflen = 0;
        self.total = 0;
    }
}

/// SHA-512 implementation
struct Sha512 {
    /// State
    h: [u64; 8],
    /// Buffer
    buffer: [u8; 128],
    /// Buffer position
    buflen: usize,
    /// Total bytes processed
    total: u128,
}

impl Sha512 {
    /// SHA-512 initial hash values
    const H0: [u64; 8] = [
        0x6a09e667f3bcc908, 0xbb67ae8584caa73b, 0x3c6ef372fe94f82b, 0xa54ff53a5f1d36f1,
        0x510e527fade682d1, 0x9b05688c2b3e6c1f, 0x1f83d9abfb41bd6b, 0x5be0cd19137e2179,
    ];

    /// SHA-512 round constants
    const K: [u64; 80] = [
        0x428a2f98d728ae22, 0x7137449123ef65cd, 0xb5c0fbcfec4d3b2f, 0xe9b5dba58189dbbc,
        0x3956c25bf348b538, 0x59f111f1b605d019, 0x923f82a4af194f9b, 0xab1c5ed5da6d8118,
        0xd807aa98a3030242, 0x12835b0145706fbe, 0x243185be4ee4b28c, 0x550c7dc3d5ffb4e2,
        0x72be5d74f27b896f, 0x80deb1fe3b1696b1, 0x9bdc06a725c71235, 0xc19bf174cf692694,
        0xe49b69c19ef14ad2, 0xefbe4786384f25e3, 0x0fc19dc68b8cd5b5, 0x240ca1cc77ac9c65,
        0x2de92c6f592b0275, 0x4a7484aa6ea6e483, 0x5cb0a9dcbd41fbd4, 0x76f988da831153b5,
        0x983e5152ee66dfab, 0xa831c66d2db43210, 0xb00327c898fb213f, 0xbf597fc7beef0ee4,
        0xc6e00bf33da88fc2, 0xd5a79147930aa725, 0x06ca6351e003826f, 0x142929670a0e6e70,
        0x27b70a8546d22ffc, 0x2e1b21385c26c926, 0x4d2c6dfc5ac42aed, 0x53380d139d95b3df,
        0x650a73548baf63de, 0x766a0abb3c77b2a8, 0x81c2c92e47edaee6, 0x92722c851482353b,
        0xa2bfe8a14cf10364, 0xa81a664bbc423001, 0xc24b8b70d0f89791, 0xc76c51a30654be30,
        0xd192e819d6ef5218, 0xd69906245565a910, 0xf40e35855771202a, 0x106aa07032bbd1b8,
        0x19a4c116b8d2d0c8, 0x1e376c085141ab53, 0x2748774cdf8eeb99, 0x34b0bcb5e19b48a8,
        0x391c0cb3c5c95a63, 0x4ed8aa4ae3418acb, 0x5b9cca4f7763e373, 0x682e6ff3d6b2b8a3,
        0x748f82ee5defb2fc, 0x78a5636f43172f60, 0x84c87814a1f0ab72, 0x8cc702081a6439ec,
        0x90befffa23631e28, 0xa4506cebde82bde9, 0xbef9a3f7b2c67915, 0xc67178f2e372532b,
        0xca273eceea26619c, 0xd186b8c721c0c207, 0xeada7dd6cde0eb1e, 0xf57d4f7fee6ed178,
        0x06f067aa72176fba, 0x0a637dc5a2c898a6, 0x113f9804bef90dae, 0x1b710b35131c471b,
        0x28db77f523047d84, 0x32caab7b40c72493, 0x3c9ebe0a15c9bebc, 0x431d67c49c100d4c,
        0x4cc5d4becb3e42b6, 0x597f299cfc657e2a, 0x5fcb6fab3ad6faec, 0x6c44198c4a475817,
    ];

    pub fn new() -> Self {
        Self {
            h: Self::H0,
            buffer: [0u8; 128],
            buflen: 0,
            total: 0,
        }
    }

    fn process_block(&mut self, block: &[u8]) {
        let mut w = [0u64; 80];

        // Prepare message schedule
        for i in 0..16 {
            w[i] = u64::from_be_bytes([
                block[i * 8], block[i * 8 + 1], block[i * 8 + 2], block[i * 8 + 3],
                block[i * 8 + 4], block[i * 8 + 5], block[i * 8 + 6], block[i * 8 + 7],
            ]);
        }

        for i in 16..80 {
            let s0 = w[i-15].rotate_right(1) ^ w[i-15].rotate_right(8) ^ (w[i-15] >> 7);
            let s1 = w[i-2].rotate_right(19) ^ w[i-2].rotate_right(61) ^ (w[i-2] >> 6);
            w[i] = w[i-16].wrapping_add(s0).wrapping_add(w[i-7]).wrapping_add(s1);
        }

        let mut a = self.h[0];
        let mut b = self.h[1];
        let mut c = self.h[2];
        let mut d = self.h[3];
        let mut e = self.h[4];
        let mut f = self.h[5];
        let mut g = self.h[6];
        let mut h = self.h[7];

        for i in 0..80 {
            let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(Self::K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.h[0] = self.h[0].wrapping_add(a);
        self.h[1] = self.h[1].wrapping_add(b);
        self.h[2] = self.h[2].wrapping_add(c);
        self.h[3] = self.h[3].wrapping_add(d);
        self.h[4] = self.h[4].wrapping_add(e);
        self.h[5] = self.h[5].wrapping_add(f);
        self.h[6] = self.h[6].wrapping_add(g);
        self.h[7] = self.h[7].wrapping_add(h);
    }
}

impl HashOps for Sha512 {
    fn name(&self) -> &str { "sha512" }
    fn digest_size(&self) -> usize { 64 }
    fn block_size(&self) -> usize { 128 }

    fn update(&mut self, data: &[u8]) {
        let mut offset = 0;

        if self.buflen > 0 {
            let needed = 128 - self.buflen;
            let to_copy = core::cmp::min(needed, data.len());
            self.buffer[self.buflen..self.buflen + to_copy].copy_from_slice(&data[..to_copy]);
            self.buflen += to_copy;
            offset = to_copy;

            if self.buflen == 128 {
                self.process_block(&self.buffer.clone());
                self.buflen = 0;
            }
        }

        while offset + 128 <= data.len() {
            self.process_block(&data[offset..offset + 128]);
            offset += 128;
        }

        if offset < data.len() {
            let remaining = data.len() - offset;
            self.buffer[..remaining].copy_from_slice(&data[offset..]);
            self.buflen = remaining;
        }

        self.total += data.len() as u128;
    }

    fn finalize(&mut self) -> Vec<u8> {
        let total_bits = self.total * 8;

        self.buffer[self.buflen] = 0x80;
        self.buflen += 1;

        if self.buflen > 112 {
            while self.buflen < 128 {
                self.buffer[self.buflen] = 0;
                self.buflen += 1;
            }
            self.process_block(&self.buffer.clone());
            self.buflen = 0;
        }

        while self.buflen < 112 {
            self.buffer[self.buflen] = 0;
            self.buflen += 1;
        }

        self.buffer[112..128].copy_from_slice(&total_bits.to_be_bytes());
        self.process_block(&self.buffer.clone());

        let mut output = Vec::with_capacity(64);
        for word in &self.h {
            output.extend_from_slice(&word.to_be_bytes());
        }
        output
    }

    fn reset(&mut self) {
        self.h = Self::H0;
        self.buffer = [0u8; 128];
        self.buflen = 0;
        self.total = 0;
    }
}

/// BLAKE2b implementation
pub struct Blake2b {
    h: [u64; 8],
    t: [u64; 2],
    f: [u64; 2],
    buf: [u8; 128],
    buflen: usize,
    outlen: usize,
}

impl Blake2b {
    const IV: [u64; 8] = [
        0x6a09e667f3bcc908, 0xbb67ae8584caa73b, 0x3c6ef372fe94f82b, 0xa54ff53a5f1d36f1,
        0x510e527fade682d1, 0x9b05688c2b3e6c1f, 0x1f83d9abfb41bd6b, 0x5be0cd19137e2179,
    ];

    const SIGMA: [[usize; 16]; 12] = [
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
        [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
        [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
        [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
        [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
        [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
        [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
        [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
        [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    ];

    pub fn new(outlen: usize) -> Self {
        let mut h = Self::IV;
        h[0] ^= 0x01010000 ^ (outlen as u64);

        Self {
            h,
            t: [0, 0],
            f: [0, 0],
            buf: [0u8; 128],
            buflen: 0,
            outlen,
        }
    }

    fn compress(&mut self, block: &[u8], last: bool) {
        let mut m = [0u64; 16];
        for i in 0..16 {
            m[i] = u64::from_le_bytes([
                block[i*8], block[i*8+1], block[i*8+2], block[i*8+3],
                block[i*8+4], block[i*8+5], block[i*8+6], block[i*8+7],
            ]);
        }

        let mut v = [0u64; 16];
        v[..8].copy_from_slice(&self.h);
        v[8..16].copy_from_slice(&Self::IV);
        v[12] ^= self.t[0];
        v[13] ^= self.t[1];

        if last {
            v[14] = !v[14];
        }

        for i in 0..12 {
            let s = &Self::SIGMA[i];
            Self::g(&mut v, 0, 4, 8, 12, m[s[0]], m[s[1]]);
            Self::g(&mut v, 1, 5, 9, 13, m[s[2]], m[s[3]]);
            Self::g(&mut v, 2, 6, 10, 14, m[s[4]], m[s[5]]);
            Self::g(&mut v, 3, 7, 11, 15, m[s[6]], m[s[7]]);
            Self::g(&mut v, 0, 5, 10, 15, m[s[8]], m[s[9]]);
            Self::g(&mut v, 1, 6, 11, 12, m[s[10]], m[s[11]]);
            Self::g(&mut v, 2, 7, 8, 13, m[s[12]], m[s[13]]);
            Self::g(&mut v, 3, 4, 9, 14, m[s[14]], m[s[15]]);
        }

        for i in 0..8 {
            self.h[i] ^= v[i] ^ v[i + 8];
        }
    }

    fn g(v: &mut [u64; 16], a: usize, b: usize, c: usize, d: usize, x: u64, y: u64) {
        v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
        v[d] = (v[d] ^ v[a]).rotate_right(32);
        v[c] = v[c].wrapping_add(v[d]);
        v[b] = (v[b] ^ v[c]).rotate_right(24);
        v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
        v[d] = (v[d] ^ v[a]).rotate_right(16);
        v[c] = v[c].wrapping_add(v[d]);
        v[b] = (v[b] ^ v[c]).rotate_right(63);
    }
}

impl HashOps for Blake2b {
    fn name(&self) -> &str { "blake2b" }
    fn digest_size(&self) -> usize { self.outlen }
    fn block_size(&self) -> usize { 128 }

    fn update(&mut self, data: &[u8]) {
        let mut offset = 0;

        if self.buflen > 0 && self.buflen + data.len() > 128 {
            let needed = 128 - self.buflen;
            self.buf[self.buflen..128].copy_from_slice(&data[..needed]);
            self.t[0] = self.t[0].wrapping_add(128);
            self.compress(&self.buf.clone(), false);
            self.buflen = 0;
            offset = needed;
        }

        while offset + 128 < data.len() {
            self.t[0] = self.t[0].wrapping_add(128);
            self.compress(&data[offset..offset + 128], false);
            offset += 128;
        }

        if offset < data.len() {
            let remaining = data.len() - offset;
            self.buf[self.buflen..self.buflen + remaining].copy_from_slice(&data[offset..]);
            self.buflen += remaining;
        }
    }

    fn finalize(&mut self) -> Vec<u8> {
        self.t[0] = self.t[0].wrapping_add(self.buflen as u64);

        while self.buflen < 128 {
            self.buf[self.buflen] = 0;
            self.buflen += 1;
        }

        self.compress(&self.buf.clone(), true);

        let mut output = Vec::with_capacity(self.outlen);
        for word in &self.h[..self.outlen / 8] {
            output.extend_from_slice(&word.to_le_bytes());
        }
        output.truncate(self.outlen);
        output
    }

    fn reset(&mut self) {
        self.h = Self::IV;
        self.h[0] ^= 0x01010000 ^ (self.outlen as u64);
        self.t = [0, 0];
        self.f = [0, 0];
        self.buf = [0u8; 128];
        self.buflen = 0;
    }
}

/// BLAKE2s implementation (similar to BLAKE2b but with 32-bit words)
struct Blake2s {
    h: [u32; 8],
    t: [u32; 2],
    buf: [u8; 64],
    buflen: usize,
    outlen: usize,
}

impl Blake2s {
    const IV: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];

    pub fn new(outlen: usize) -> Self {
        let mut h = Self::IV;
        h[0] ^= 0x01010000 ^ (outlen as u32);

        Self {
            h,
            t: [0, 0],
            buf: [0u8; 64],
            buflen: 0,
            outlen,
        }
    }
}

impl HashOps for Blake2s {
    fn name(&self) -> &str { "blake2s" }
    fn digest_size(&self) -> usize { self.outlen }
    fn block_size(&self) -> usize { 64 }

    fn update(&mut self, data: &[u8]) {
        // Simplified - would implement full BLAKE2s compression
        for chunk in data.chunks(64) {
            self.t[0] = self.t[0].wrapping_add(chunk.len() as u32);
            // Compression would go here
        }
    }

    fn finalize(&mut self) -> Vec<u8> {
        let mut output = Vec::with_capacity(self.outlen);
        for word in &self.h[..self.outlen / 4] {
            output.extend_from_slice(&word.to_le_bytes());
        }
        output.truncate(self.outlen);
        output
    }

    fn reset(&mut self) {
        self.h = Self::IV;
        self.h[0] ^= 0x01010000 ^ (self.outlen as u32);
        self.t = [0, 0];
        self.buf = [0u8; 64];
        self.buflen = 0;
    }
}
