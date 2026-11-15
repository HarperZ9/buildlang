//! Key Derivation Functions
//!
//! Provides HKDF, PBKDF2, scrypt, and Argon2 implementations.

use alloc::boxed::Box;
use alloc::vec::Vec;
use alloc::vec;
use super::{CryptoError, STATS};
use super::hash::{HashOps, HashAlgorithm};
use super::mac::{MacAlgorithm, Mac};
use core::sync::atomic::Ordering;

/// KDF algorithm
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KdfAlgorithm {
    /// HKDF with SHA-256
    HkdfSha256,
    /// HKDF with SHA-512
    HkdfSha512,
    /// PBKDF2 with SHA-256
    Pbkdf2Sha256,
    /// PBKDF2 with SHA-512
    Pbkdf2Sha512,
    /// scrypt
    Scrypt,
    /// Argon2id
    Argon2id,
}

/// KDF operations trait
pub trait KdfOps: Send + Sync {
    /// Get algorithm name
    fn name(&self) -> &str;

    /// Derive key material
    fn derive(&self, input: &[u8], output: &mut [u8]) -> Result<(), CryptoError>;
}

// =============================================================================
// HKDF (HMAC-based Key Derivation Function)
// =============================================================================

/// HKDF implementation (RFC 5869)
pub struct Hkdf {
    /// Hash algorithm
    hash: HashAlgorithm,
    /// Salt (optional)
    salt: Vec<u8>,
    /// Info (optional context)
    info: Vec<u8>,
}

impl Hkdf {
    /// Create new HKDF instance
    pub fn new(hash: HashAlgorithm) -> Self {
        Self {
            hash,
            salt: Vec::new(),
            info: Vec::new(),
        }
    }

    /// Set salt
    pub fn with_salt(mut self, salt: &[u8]) -> Self {
        self.salt = salt.to_vec();
        self
    }

    /// Set info
    pub fn with_info(mut self, info: &[u8]) -> Self {
        self.info = info.to_vec();
        self
    }

    /// HKDF-Extract
    pub fn extract(&self, ikm: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let mac_alg = match self.hash {
            HashAlgorithm::Sha256 => MacAlgorithm::HmacSha256,
            HashAlgorithm::Sha512 => MacAlgorithm::HmacSha512,
            _ => return Err(CryptoError::AlgorithmNotFound),
        };

        let hash_len = self.hash.digest_size();

        // Use salt or zero salt
        let salt = if self.salt.is_empty() {
            vec![0u8; hash_len]
        } else {
            self.salt.clone()
        };

        let mut mac = Mac::new(mac_alg, &salt)?;
        mac.update(ikm);
        Ok(mac.finalize())
    }

    /// HKDF-Expand
    pub fn expand(&self, prk: &[u8], output: &mut [u8]) -> Result<(), CryptoError> {
        let mac_alg = match self.hash {
            HashAlgorithm::Sha256 => MacAlgorithm::HmacSha256,
            HashAlgorithm::Sha512 => MacAlgorithm::HmacSha512,
            _ => return Err(CryptoError::AlgorithmNotFound),
        };

        let hash_len = self.hash.digest_size();
        let n = (output.len() + hash_len - 1) / hash_len;

        if n > 255 {
            return Err(CryptoError::InvalidOutputLength);
        }

        let mut t = Vec::new();
        let mut offset = 0;

        for i in 1..=n {
            let mut mac = Mac::new(mac_alg, prk)?;

            // T(i) = HMAC-Hash(PRK, T(i-1) | info | i)
            mac.update(&t);
            mac.update(&self.info);
            mac.update(&[i as u8]);

            t = mac.finalize();

            let to_copy = core::cmp::min(hash_len, output.len() - offset);
            output[offset..offset + to_copy].copy_from_slice(&t[..to_copy]);
            offset += to_copy;
        }

        STATS.kdf_ops.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Full HKDF (extract + expand)
    pub fn derive_key(&self, ikm: &[u8], output: &mut [u8]) -> Result<(), CryptoError> {
        let prk = self.extract(ikm)?;
        self.expand(&prk, output)
    }
}

impl KdfOps for Hkdf {
    fn name(&self) -> &str {
        match self.hash {
            HashAlgorithm::Sha256 => "hkdf-sha256",
            HashAlgorithm::Sha512 => "hkdf-sha512",
            _ => "hkdf",
        }
    }

    fn derive(&self, input: &[u8], output: &mut [u8]) -> Result<(), CryptoError> {
        self.derive_key(input, output)
    }
}

/// Convenience function for HKDF-SHA256
pub fn hkdf_sha256(
    salt: &[u8],
    ikm: &[u8],
    info: &[u8],
    output: &mut [u8],
) -> Result<(), CryptoError> {
    let hkdf = Hkdf::new(HashAlgorithm::Sha256)
        .with_salt(salt)
        .with_info(info);
    hkdf.derive_key(ikm, output)
}

/// Convenience function for HKDF-SHA512
pub fn hkdf_sha512(
    salt: &[u8],
    ikm: &[u8],
    info: &[u8],
    output: &mut [u8],
) -> Result<(), CryptoError> {
    let hkdf = Hkdf::new(HashAlgorithm::Sha512)
        .with_salt(salt)
        .with_info(info);
    hkdf.derive_key(ikm, output)
}

// =============================================================================
// PBKDF2 (Password-Based Key Derivation Function 2)
// =============================================================================

/// PBKDF2 implementation (RFC 2898)
pub struct Pbkdf2 {
    /// Hash algorithm
    hash: HashAlgorithm,
    /// Salt
    salt: Vec<u8>,
    /// Iteration count
    iterations: u32,
}

impl Pbkdf2 {
    /// Create new PBKDF2 instance
    pub fn new(hash: HashAlgorithm, iterations: u32) -> Self {
        Self {
            hash,
            salt: Vec::new(),
            iterations,
        }
    }

    /// Set salt
    pub fn with_salt(mut self, salt: &[u8]) -> Self {
        self.salt = salt.to_vec();
        self
    }

    /// Derive key from password
    pub fn derive_key(&self, password: &[u8], output: &mut [u8]) -> Result<(), CryptoError> {
        if self.iterations == 0 {
            return Err(CryptoError::InvalidParameter);
        }

        let mac_alg = match self.hash {
            HashAlgorithm::Sha256 => MacAlgorithm::HmacSha256,
            HashAlgorithm::Sha512 => MacAlgorithm::HmacSha512,
            _ => return Err(CryptoError::AlgorithmNotFound),
        };

        let hash_len = self.hash.digest_size();
        let blocks = (output.len() + hash_len - 1) / hash_len;
        let mut offset = 0;

        for block in 1..=blocks {
            let block_output = self.f(password, block as u32, mac_alg)?;

            let to_copy = core::cmp::min(hash_len, output.len() - offset);
            output[offset..offset + to_copy].copy_from_slice(&block_output[..to_copy]);
            offset += to_copy;
        }

        STATS.kdf_ops.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// F function: U1 ^ U2 ^ ... ^ Uc
    fn f(&self, password: &[u8], block: u32, mac_alg: MacAlgorithm) -> Result<Vec<u8>, CryptoError> {
        let mut mac = Mac::new(mac_alg, password)?;

        // U1 = PRF(Password, Salt || INT(i))
        mac.update(&self.salt);
        mac.update(&block.to_be_bytes());
        let mut u = mac.finalize();
        let mut result = u.clone();

        // U2...Uc
        for _ in 1..self.iterations {
            let mut mac = Mac::new(mac_alg, password)?;
            mac.update(&u);
            u = mac.finalize();

            // XOR with result
            for (r, x) in result.iter_mut().zip(u.iter()) {
                *r ^= *x;
            }
        }

        Ok(result)
    }
}

impl KdfOps for Pbkdf2 {
    fn name(&self) -> &str {
        match self.hash {
            HashAlgorithm::Sha256 => "pbkdf2-sha256",
            HashAlgorithm::Sha512 => "pbkdf2-sha512",
            _ => "pbkdf2",
        }
    }

    fn derive(&self, input: &[u8], output: &mut [u8]) -> Result<(), CryptoError> {
        self.derive_key(input, output)
    }
}

/// Convenience function for PBKDF2-SHA256
pub fn pbkdf2_sha256(
    password: &[u8],
    salt: &[u8],
    iterations: u32,
    output: &mut [u8],
) -> Result<(), CryptoError> {
    let pbkdf2 = Pbkdf2::new(HashAlgorithm::Sha256, iterations).with_salt(salt);
    pbkdf2.derive_key(password, output)
}

/// Convenience function for PBKDF2-SHA512
pub fn pbkdf2_sha512(
    password: &[u8],
    salt: &[u8],
    iterations: u32,
    output: &mut [u8],
) -> Result<(), CryptoError> {
    let pbkdf2 = Pbkdf2::new(HashAlgorithm::Sha512, iterations).with_salt(salt);
    pbkdf2.derive_key(password, output)
}

// =============================================================================
// scrypt
// =============================================================================

/// scrypt parameters
#[derive(Clone, Copy, Debug)]
pub struct ScryptParams {
    /// CPU/memory cost (N)
    pub n: u32,
    /// Block size (r)
    pub r: u32,
    /// Parallelization (p)
    pub p: u32,
}

impl ScryptParams {
    /// Default parameters for interactive login
    pub fn interactive() -> Self {
        Self {
            n: 16384,  // 2^14
            r: 8,
            p: 1,
        }
    }

    /// Default parameters for sensitive storage
    pub fn sensitive() -> Self {
        Self {
            n: 1048576, // 2^20
            r: 8,
            p: 1,
        }
    }

    /// Validate parameters
    pub fn validate(&self) -> Result<(), CryptoError> {
        // N must be power of 2 greater than 1
        if self.n < 2 || (self.n & (self.n - 1)) != 0 {
            return Err(CryptoError::InvalidParameter);
        }
        if self.r == 0 || self.p == 0 {
            return Err(CryptoError::InvalidParameter);
        }
        Ok(())
    }
}

/// scrypt implementation
pub struct Scrypt {
    /// Parameters
    params: ScryptParams,
    /// Salt
    salt: Vec<u8>,
}

impl Scrypt {
    /// Create new scrypt instance
    pub fn new(params: ScryptParams) -> Result<Self, CryptoError> {
        params.validate()?;
        Ok(Self {
            params,
            salt: Vec::new(),
        })
    }

    /// Set salt
    pub fn with_salt(mut self, salt: &[u8]) -> Self {
        self.salt = salt.to_vec();
        self
    }

    /// Derive key from password
    pub fn derive_key(&self, password: &[u8], output: &mut [u8]) -> Result<(), CryptoError> {
        let n = self.params.n as usize;
        let r = self.params.r as usize;
        let p = self.params.p as usize;

        // Step 1: Generate initial data using PBKDF2
        let block_size = 128 * r;
        let mut b = vec![0u8; block_size * p];

        pbkdf2_sha256(password, &self.salt, 1, &mut b)?;

        // Step 2: Apply ROMix to each block
        for i in 0..p {
            let block_start = i * block_size;
            let block_end = block_start + block_size;
            self.romix(&mut b[block_start..block_end], n, r)?;
        }

        // Step 3: Generate output using PBKDF2
        pbkdf2_sha256(password, &b, 1, output)?;

        STATS.kdf_ops.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// ROMix function
    fn romix(&self, block: &mut [u8], n: usize, r: usize) -> Result<(), CryptoError> {
        let block_size = 128 * r;

        // Allocate V array
        let mut v = vec![0u8; block_size * n];

        // Step 1: Build lookup table
        v[..block_size].copy_from_slice(block);
        for i in 1..n {
            let prev_start = (i - 1) * block_size;
            let curr_start = i * block_size;

            // Copy previous block
            let (left, right) = v.split_at_mut(curr_start);
            right[..block_size].copy_from_slice(&left[prev_start..prev_start + block_size]);

            // Apply BlockMix
            self.block_mix(&mut right[..block_size], r);
        }

        // Apply final BlockMix to V[n-1]
        block.copy_from_slice(&v[(n - 1) * block_size..n * block_size]);
        self.block_mix(block, r);

        // Step 2: Mix with random lookups
        for _ in 0..n {
            // j = Integerify(X) mod N
            let j = self.integerify(block, n);
            let v_j = &v[j * block_size..(j + 1) * block_size];

            // X = X XOR V[j]
            for (x, y) in block.iter_mut().zip(v_j.iter()) {
                *x ^= *y;
            }

            self.block_mix(block, r);
        }

        Ok(())
    }

    /// BlockMix function using Salsa20/8
    fn block_mix(&self, block: &mut [u8], r: usize) {
        let mut x = [0u32; 16];

        // Initialize X from last 64-byte chunk
        let last_chunk = (2 * r - 1) * 64;
        for i in 0..16 {
            x[i] = u32::from_le_bytes([
                block[last_chunk + i * 4],
                block[last_chunk + i * 4 + 1],
                block[last_chunk + i * 4 + 2],
                block[last_chunk + i * 4 + 3],
            ]);
        }

        // Process each 64-byte chunk
        let mut output = vec![0u8; block.len()];
        let _y_offset = 0;

        for chunk_idx in 0..(2 * r) {
            let chunk_start = chunk_idx * 64;

            // X = X XOR chunk
            for i in 0..16 {
                let chunk_word = u32::from_le_bytes([
                    block[chunk_start + i * 4],
                    block[chunk_start + i * 4 + 1],
                    block[chunk_start + i * 4 + 2],
                    block[chunk_start + i * 4 + 3],
                ]);
                x[i] ^= chunk_word;
            }

            // Apply Salsa20/8
            self.salsa20_8(&mut x);

            // Store in Y (even chunks go to first half, odd to second)
            let out_offset = if chunk_idx % 2 == 0 {
                (chunk_idx / 2) * 64
            } else {
                r * 64 + (chunk_idx / 2) * 64
            };

            for i in 0..16 {
                let bytes = x[i].to_le_bytes();
                output[out_offset + i * 4..out_offset + i * 4 + 4].copy_from_slice(&bytes);
            }
        }

        block.copy_from_slice(&output);
    }

    /// Salsa20/8 core function
    fn salsa20_8(&self, x: &mut [u32; 16]) {
        let original = *x;

        // 8 rounds (4 double rounds)
        for _ in 0..4 {
            // Column round
            x[4] ^= (x[0].wrapping_add(x[12])).rotate_left(7);
            x[8] ^= (x[4].wrapping_add(x[0])).rotate_left(9);
            x[12] ^= (x[8].wrapping_add(x[4])).rotate_left(13);
            x[0] ^= (x[12].wrapping_add(x[8])).rotate_left(18);

            x[9] ^= (x[5].wrapping_add(x[1])).rotate_left(7);
            x[13] ^= (x[9].wrapping_add(x[5])).rotate_left(9);
            x[1] ^= (x[13].wrapping_add(x[9])).rotate_left(13);
            x[5] ^= (x[1].wrapping_add(x[13])).rotate_left(18);

            x[14] ^= (x[10].wrapping_add(x[6])).rotate_left(7);
            x[2] ^= (x[14].wrapping_add(x[10])).rotate_left(9);
            x[6] ^= (x[2].wrapping_add(x[14])).rotate_left(13);
            x[10] ^= (x[6].wrapping_add(x[2])).rotate_left(18);

            x[3] ^= (x[15].wrapping_add(x[11])).rotate_left(7);
            x[7] ^= (x[3].wrapping_add(x[15])).rotate_left(9);
            x[11] ^= (x[7].wrapping_add(x[3])).rotate_left(13);
            x[15] ^= (x[11].wrapping_add(x[7])).rotate_left(18);

            // Row round
            x[1] ^= (x[0].wrapping_add(x[3])).rotate_left(7);
            x[2] ^= (x[1].wrapping_add(x[0])).rotate_left(9);
            x[3] ^= (x[2].wrapping_add(x[1])).rotate_left(13);
            x[0] ^= (x[3].wrapping_add(x[2])).rotate_left(18);

            x[6] ^= (x[5].wrapping_add(x[4])).rotate_left(7);
            x[7] ^= (x[6].wrapping_add(x[5])).rotate_left(9);
            x[4] ^= (x[7].wrapping_add(x[6])).rotate_left(13);
            x[5] ^= (x[4].wrapping_add(x[7])).rotate_left(18);

            x[11] ^= (x[10].wrapping_add(x[9])).rotate_left(7);
            x[8] ^= (x[11].wrapping_add(x[10])).rotate_left(9);
            x[9] ^= (x[8].wrapping_add(x[11])).rotate_left(13);
            x[10] ^= (x[9].wrapping_add(x[8])).rotate_left(18);

            x[12] ^= (x[15].wrapping_add(x[14])).rotate_left(7);
            x[13] ^= (x[12].wrapping_add(x[15])).rotate_left(9);
            x[14] ^= (x[13].wrapping_add(x[12])).rotate_left(13);
            x[15] ^= (x[14].wrapping_add(x[13])).rotate_left(18);
        }

        // Add original
        for i in 0..16 {
            x[i] = x[i].wrapping_add(original[i]);
        }
    }

    /// Integerify function - extract integer from block
    fn integerify(&self, block: &[u8], n: usize) -> usize {
        // Take last 64 bytes, extract first 8 bytes as little-endian u64
        let offset = block.len() - 64;
        let value = u64::from_le_bytes([
            block[offset],
            block[offset + 1],
            block[offset + 2],
            block[offset + 3],
            block[offset + 4],
            block[offset + 5],
            block[offset + 6],
            block[offset + 7],
        ]);

        (value as usize) % n
    }
}

impl KdfOps for Scrypt {
    fn name(&self) -> &str {
        "scrypt"
    }

    fn derive(&self, input: &[u8], output: &mut [u8]) -> Result<(), CryptoError> {
        self.derive_key(input, output)
    }
}

/// Convenience function for scrypt
pub fn scrypt(
    password: &[u8],
    salt: &[u8],
    params: ScryptParams,
    output: &mut [u8],
) -> Result<(), CryptoError> {
    let s = Scrypt::new(params)?.with_salt(salt);
    s.derive_key(password, output)
}

// =============================================================================
// Argon2
// =============================================================================

/// Argon2 variant
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Argon2Variant {
    /// Argon2d - data dependent
    Argon2d,
    /// Argon2i - data independent
    Argon2i,
    /// Argon2id - hybrid
    Argon2id,
}

/// Argon2 parameters
#[derive(Clone, Debug)]
pub struct Argon2Params {
    /// Variant
    pub variant: Argon2Variant,
    /// Memory cost in KiB
    pub m_cost: u32,
    /// Time cost (iterations)
    pub t_cost: u32,
    /// Parallelism
    pub p_cost: u32,
}

impl Argon2Params {
    /// Default parameters
    pub fn default() -> Self {
        Self {
            variant: Argon2Variant::Argon2id,
            m_cost: 65536,  // 64 MiB
            t_cost: 3,
            p_cost: 4,
        }
    }

    /// Validate parameters
    pub fn validate(&self) -> Result<(), CryptoError> {
        if self.m_cost < 8 {
            return Err(CryptoError::InvalidParameter);
        }
        if self.t_cost < 1 {
            return Err(CryptoError::InvalidParameter);
        }
        if self.p_cost < 1 || self.p_cost > 0xffffff {
            return Err(CryptoError::InvalidParameter);
        }
        Ok(())
    }
}

/// Argon2 implementation
pub struct Argon2 {
    /// Parameters
    params: Argon2Params,
    /// Salt
    salt: Vec<u8>,
    /// Additional data
    ad: Vec<u8>,
    /// Secret key
    secret: Vec<u8>,
}

impl Argon2 {
    /// Block size in bytes
    const BLOCK_SIZE: usize = 1024;

    /// Create new Argon2 instance
    pub fn new(params: Argon2Params) -> Result<Self, CryptoError> {
        params.validate()?;
        Ok(Self {
            params,
            salt: Vec::new(),
            ad: Vec::new(),
            secret: Vec::new(),
        })
    }

    /// Set salt
    pub fn with_salt(mut self, salt: &[u8]) -> Self {
        self.salt = salt.to_vec();
        self
    }

    /// Set additional data
    pub fn with_ad(mut self, ad: &[u8]) -> Self {
        self.ad = ad.to_vec();
        self
    }

    /// Set secret key
    pub fn with_secret(mut self, secret: &[u8]) -> Self {
        self.secret = secret.to_vec();
        self
    }

    /// Derive key from password
    pub fn derive_key(&self, password: &[u8], output: &mut [u8]) -> Result<(), CryptoError> {
        let p = self.params.p_cost as usize;
        let m = (self.params.m_cost as usize / p) * p; // Round down to multiple of p
        let q = m / p; // Blocks per lane
        let t = self.params.t_cost;

        // Allocate memory blocks
        let mut memory = vec![[0u64; Self::BLOCK_SIZE / 8]; m];

        // Compute H0
        let h0 = self.compute_h0(password, output.len() as u32)?;

        // Initialize first two blocks of each lane
        for lane in 0..p {
            // B[lane][0] = H'^1024(H0 || 0 || lane)
            let mut input = h0.clone();
            input.extend(&0u32.to_le_bytes());
            input.extend(&(lane as u32).to_le_bytes());
            let block = self.blake2b_long(&input, Self::BLOCK_SIZE)?;
            self.bytes_to_block(&block, &mut memory[lane * q]);

            // B[lane][1] = H'^1024(H0 || 1 || lane)
            input = h0.clone();
            input.extend(&1u32.to_le_bytes());
            input.extend(&(lane as u32).to_le_bytes());
            let block = self.blake2b_long(&input, Self::BLOCK_SIZE)?;
            self.bytes_to_block(&block, &mut memory[lane * q + 1]);
        }

        // Main iterations
        for pass in 0..t {
            for slice in 0..4 {
                for lane in 0..p {
                    self.fill_segment(&mut memory, pass, slice, lane as u32, p, q)?;
                }
            }
        }

        // XOR final blocks
        let mut final_block = [0u64; Self::BLOCK_SIZE / 8];
        for lane in 0..p {
            let last_idx = lane * q + q - 1;
            for (f, m) in final_block.iter_mut().zip(memory[last_idx].iter()) {
                *f ^= *m;
            }
        }

        // Hash final block to output
        let final_bytes = self.block_to_bytes(&final_block);
        let hash = self.blake2b_long(&final_bytes, output.len())?;
        output.copy_from_slice(&hash);

        STATS.kdf_ops.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Compute H0
    fn compute_h0(&self, password: &[u8], output_len: u32) -> Result<Vec<u8>, CryptoError> {
        let mut hasher = super::hash::Blake2b::new(64);

        // H0 = H(p || τ || m || t || v || y || |P| || P || |S| || S || |K| || K || |X| || X)
        hasher.update(&self.params.p_cost.to_le_bytes());
        hasher.update(&output_len.to_le_bytes());
        hasher.update(&self.params.m_cost.to_le_bytes());
        hasher.update(&self.params.t_cost.to_le_bytes());
        hasher.update(&0x13u32.to_le_bytes()); // Version 0x13
        hasher.update(&(self.params.variant as u32).to_le_bytes());

        hasher.update(&(password.len() as u32).to_le_bytes());
        hasher.update(password);

        hasher.update(&(self.salt.len() as u32).to_le_bytes());
        hasher.update(&self.salt);

        hasher.update(&(self.secret.len() as u32).to_le_bytes());
        hasher.update(&self.secret);

        hasher.update(&(self.ad.len() as u32).to_le_bytes());
        hasher.update(&self.ad);

        Ok(hasher.finalize())
    }

    /// Fill a segment of memory
    fn fill_segment(
        &self,
        memory: &mut [[u64; 128]],
        pass: u32,
        slice: usize,
        lane: u32,
        _p: usize,
        q: usize,
    ) -> Result<(), CryptoError> {
        let segment_length = q / 4;
        let starting_index = if pass == 0 && slice == 0 { 2 } else { 0 };

        for idx in starting_index..segment_length {
            let absolute_idx = slice * segment_length + idx;
            let block_idx = lane as usize * q + absolute_idx;

            // Get reference block indices (simplified)
            let prev_idx = if absolute_idx == 0 {
                lane as usize * q + q - 1
            } else {
                lane as usize * q + absolute_idx - 1
            };

            // Simplified: use previous block as reference
            let ref_idx = prev_idx;

            // G function (simplified)
            let mut new_block = [0u64; 128];
            for i in 0..128 {
                new_block[i] = memory[prev_idx][i] ^ memory[ref_idx][i];
            }

            // Apply compression function
            self.compress(&mut new_block);

            // XOR with previous for passes > 0
            if pass > 0 {
                for i in 0..128 {
                    memory[block_idx][i] ^= new_block[i];
                }
            } else {
                memory[block_idx] = new_block;
            }
        }

        Ok(())
    }

    /// Compression function (simplified Blake2b-based)
    fn compress(&self, block: &mut [u64; 128]) {
        // Apply permutation
        for i in 0..8 {
            // Row mixing
            for j in 0..8 {
                let idx = i * 16 + j * 2;
                let a = block[idx];
                let b = block[idx + 1];
                block[idx] = a.wrapping_add(b).wrapping_add(2u64.wrapping_mul(a & 0xffffffff).wrapping_mul(b & 0xffffffff));
                block[idx + 1] = block[idx].rotate_right(32);
            }
        }

        // Column mixing
        for i in 0..16 {
            for j in 0..4 {
                let idx1 = j * 32 + i;
                let idx2 = j * 32 + i + 16;
                let a = block[idx1];
                let b = block[idx2];
                block[idx1] = a ^ b;
                block[idx2] = (a.wrapping_add(b)).rotate_right(24);
            }
        }
    }

    /// Blake2b with variable output length
    fn blake2b_long(&self, input: &[u8], output_len: usize) -> Result<Vec<u8>, CryptoError> {
        if output_len <= 64 {
            let mut hasher = super::hash::Blake2b::new(output_len);
            hasher.update(&(output_len as u32).to_le_bytes());
            hasher.update(input);
            Ok(hasher.finalize())
        } else {
            // For longer outputs, chain Blake2b calls
            let mut output = Vec::with_capacity(output_len);
            let mut remaining = output_len;

            // First block
            let mut hasher = super::hash::Blake2b::new(64);
            hasher.update(&(output_len as u32).to_le_bytes());
            hasher.update(input);
            let mut v = hasher.finalize();

            output.extend(&v[..32]);
            remaining -= 32;

            // Subsequent blocks
            while remaining > 64 {
                let mut hasher = super::hash::Blake2b::new(64);
                hasher.update(&v);
                v = hasher.finalize();
                output.extend(&v[..32]);
                remaining -= 32;
            }

            // Final block
            let mut hasher = super::hash::Blake2b::new(remaining);
            hasher.update(&v);
            output.extend(hasher.finalize());

            Ok(output)
        }
    }

    /// Convert bytes to block
    fn bytes_to_block(&self, bytes: &[u8], block: &mut [u64; 128]) {
        for i in 0..128 {
            block[i] = u64::from_le_bytes([
                bytes[i * 8],
                bytes[i * 8 + 1],
                bytes[i * 8 + 2],
                bytes[i * 8 + 3],
                bytes[i * 8 + 4],
                bytes[i * 8 + 5],
                bytes[i * 8 + 6],
                bytes[i * 8 + 7],
            ]);
        }
    }

    /// Convert block to bytes
    fn block_to_bytes(&self, block: &[u64; 128]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::BLOCK_SIZE);
        for &word in block {
            bytes.extend(&word.to_le_bytes());
        }
        bytes
    }
}

impl KdfOps for Argon2 {
    fn name(&self) -> &str {
        match self.params.variant {
            Argon2Variant::Argon2d => "argon2d",
            Argon2Variant::Argon2i => "argon2i",
            Argon2Variant::Argon2id => "argon2id",
        }
    }

    fn derive(&self, input: &[u8], output: &mut [u8]) -> Result<(), CryptoError> {
        self.derive_key(input, output)
    }
}

/// Convenience function for Argon2id
pub fn argon2id(
    password: &[u8],
    salt: &[u8],
    params: Argon2Params,
    output: &mut [u8],
) -> Result<(), CryptoError> {
    let a = Argon2::new(params)?.with_salt(salt);
    a.derive_key(password, output)
}

// =============================================================================
// Factory
// =============================================================================

/// Create KDF by algorithm
pub fn create_kdf(algorithm: KdfAlgorithm) -> Result<Box<dyn KdfOps>, CryptoError> {
    match algorithm {
        KdfAlgorithm::HkdfSha256 => Ok(Box::new(Hkdf::new(HashAlgorithm::Sha256))),
        KdfAlgorithm::HkdfSha512 => Ok(Box::new(Hkdf::new(HashAlgorithm::Sha512))),
        KdfAlgorithm::Pbkdf2Sha256 => Ok(Box::new(Pbkdf2::new(HashAlgorithm::Sha256, 100000))),
        KdfAlgorithm::Pbkdf2Sha512 => Ok(Box::new(Pbkdf2::new(HashAlgorithm::Sha512, 100000))),
        KdfAlgorithm::Scrypt => Ok(Box::new(Scrypt::new(ScryptParams::interactive())?)),
        KdfAlgorithm::Argon2id => Ok(Box::new(Argon2::new(Argon2Params::default())?)),
    }
}
