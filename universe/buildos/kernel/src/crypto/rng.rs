//! Cryptographic Random Number Generation
//!
//! Provides secure random number generation using hardware RNG and entropy pools.

use alloc::boxed::Box;
use alloc::vec::Vec;
use alloc::vec;
use super::CryptoError;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

/// RNG algorithm
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RngAlgorithm {
    /// Hardware RNG (RDRAND/RDSEED)
    Hardware,
    /// ChaCha20-based CSPRNG
    ChaCha20,
    /// Combined hardware + software
    Combined,
    /// Fortuna CSPRNG
    Fortuna,
}

/// Random number generator trait
pub trait RngOps: Send + Sync {
    /// Get algorithm name
    fn name(&self) -> &str;

    /// Fill buffer with random bytes
    fn fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), CryptoError>;

    /// Generate random bytes
    fn random_bytes(&mut self, len: usize) -> Result<Vec<u8>, CryptoError> {
        let mut buf = vec![0u8; len];
        self.fill_bytes(&mut buf)?;
        Ok(buf)
    }

    /// Reseed with additional entropy
    fn reseed(&mut self, seed: &[u8]) -> Result<(), CryptoError>;

    /// Check if RNG is seeded
    fn is_seeded(&self) -> bool;
}

/// Hardware capabilities
static HAS_RDRAND: AtomicBool = AtomicBool::new(false);
static HAS_RDSEED: AtomicBool = AtomicBool::new(false);

/// Global entropy pool
static ENTROPY_POOL: Mutex<EntropyPool> = Mutex::new(EntropyPool::new());

/// System CSPRNG
static SYSTEM_RNG: Mutex<Option<ChaCha20Rng>> = Mutex::new(None);

/// Check for hardware RNG support
pub fn init() {
    // Check CPUID for RDRAND/RDSEED
    let has_rdrand: bool;
    let has_rdseed: bool;

    unsafe {
        let mut ecx: u32;
        let mut ebx: u32;

        // CPUID leaf 1, check ECX bit 30 for RDRAND
        core::arch::asm!(
            "push rbx",
            "cpuid",
            "pop rbx",
            inout("eax") 1u32 => _,
            out("ecx") ecx,
            out("edx") _,
        );
        has_rdrand = (ecx & (1 << 30)) != 0;

        // CPUID leaf 7, check EBX bit 18 for RDSEED
        core::arch::asm!(
            "push rbx",
            "cpuid",
            "mov {ebx_out:e}, ebx",
            "pop rbx",
            inout("eax") 7u32 => _,
            inout("ecx") 0u32 => _,
            ebx_out = out(reg) ebx,
            out("edx") _,
        );
        has_rdseed = (ebx & (1 << 18)) != 0;
    }

    HAS_RDRAND.store(has_rdrand, Ordering::SeqCst);
    HAS_RDSEED.store(has_rdseed, Ordering::SeqCst);

    // Initialize system RNG
    let mut rng = ChaCha20Rng::new();

    // Seed from hardware if available
    if has_rdseed || has_rdrand {
        let mut seed = [0u8; 32];
        if hardware_random(&mut seed) {
            let _ = rng.reseed(&seed);
        }
    }

    // Seed from TSC as fallback
    let tsc = read_tsc();
    let tsc_bytes = tsc.to_le_bytes();
    let _ = rng.reseed(&tsc_bytes);

    *SYSTEM_RNG.lock() = Some(rng);
}

/// Read TSC (Time Stamp Counter)
fn read_tsc() -> u64 {
    unsafe {
        let mut lo: u32;
        let mut hi: u32;
        core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi);
        ((hi as u64) << 32) | (lo as u64)
    }
}

/// Check if hardware RNG is available
pub fn has_hardware_rng() -> bool {
    HAS_RDRAND.load(Ordering::Relaxed) || HAS_RDSEED.load(Ordering::Relaxed)
}

/// Get random bytes from hardware RNG
pub fn hardware_random(dest: &mut [u8]) -> bool {
    if HAS_RDSEED.load(Ordering::Relaxed) {
        return rdseed_bytes(dest);
    }
    if HAS_RDRAND.load(Ordering::Relaxed) {
        return rdrand_bytes(dest);
    }
    false
}

/// Fill bytes using RDSEED
fn rdseed_bytes(dest: &mut [u8]) -> bool {
    let mut pos = 0;

    while pos + 8 <= dest.len() {
        let mut value: u64;
        let mut success: u8;

        unsafe {
            core::arch::asm!(
                "rdseed {0}",
                "setc {1}",
                out(reg) value,
                out(reg_byte) success,
            );
        }

        if success == 0 {
            // Retry with backoff
            for _retry in 0..10 {
                core::hint::spin_loop();
                unsafe {
                    core::arch::asm!(
                        "rdseed {0}",
                        "setc {1}",
                        out(reg) value,
                        out(reg_byte) success,
                    );
                }
                if success != 0 {
                    break;
                }
            }
            if success == 0 {
                return false;
            }
        }

        dest[pos..pos + 8].copy_from_slice(&value.to_le_bytes());
        pos += 8;
    }

    // Handle remaining bytes
    if pos < dest.len() {
        let mut value: u64;
        let mut success: u8;

        unsafe {
            core::arch::asm!(
                "rdseed {0}",
                "setc {1}",
                out(reg) value,
                out(reg_byte) success,
            );
        }

        if success == 0 {
            return false;
        }

        let bytes = value.to_le_bytes();
        let remaining = dest.len() - pos;
        dest[pos..].copy_from_slice(&bytes[..remaining]);
    }

    true
}

/// Fill bytes using RDRAND
fn rdrand_bytes(dest: &mut [u8]) -> bool {
    let mut pos = 0;

    while pos + 8 <= dest.len() {
        let mut value: u64;
        let mut success: u8;

        unsafe {
            core::arch::asm!(
                "rdrand {0}",
                "setc {1}",
                out(reg) value,
                out(reg_byte) success,
            );
        }

        if success == 0 {
            // Retry
            for _ in 0..10 {
                core::hint::spin_loop();
                unsafe {
                    core::arch::asm!(
                        "rdrand {0}",
                        "setc {1}",
                        out(reg) value,
                        out(reg_byte) success,
                    );
                }
                if success != 0 {
                    break;
                }
            }
            if success == 0 {
                return false;
            }
        }

        dest[pos..pos + 8].copy_from_slice(&value.to_le_bytes());
        pos += 8;
    }

    // Handle remaining bytes
    if pos < dest.len() {
        let mut value: u64;
        let mut success: u8;

        unsafe {
            core::arch::asm!(
                "rdrand {0}",
                "setc {1}",
                out(reg) value,
                out(reg_byte) success,
            );
        }

        if success == 0 {
            return false;
        }

        let bytes = value.to_le_bytes();
        let remaining = dest.len() - pos;
        dest[pos..].copy_from_slice(&bytes[..remaining]);
    }

    true
}

/// Get random u64
pub fn random_u64() -> u64 {
    let mut buf = [0u8; 8];

    // Try hardware first
    if hardware_random(&mut buf) {
        return u64::from_le_bytes(buf);
    }

    // Fall back to system RNG
    if let Some(ref mut rng) = *SYSTEM_RNG.lock() {
        let _ = rng.fill_bytes(&mut buf);
    }

    u64::from_le_bytes(buf)
}

/// Get random bytes
pub fn random_bytes(len: usize) -> Result<Vec<u8>, CryptoError> {
    let mut buf = vec![0u8; len];

    // Try hardware first
    if hardware_random(&mut buf) {
        return Ok(buf);
    }

    // Fall back to system RNG
    if let Some(ref mut rng) = *SYSTEM_RNG.lock() {
        rng.fill_bytes(&mut buf)?;
        return Ok(buf);
    }

    Err(CryptoError::RngFailed)
}

/// Add entropy to the pool
pub fn add_entropy(data: &[u8], entropy_bits: u32) {
    ENTROPY_POOL.lock().add_entropy(data, entropy_bits);
}

// =============================================================================
// Entropy Pool
// =============================================================================

/// Entropy pool for collecting system entropy
pub struct EntropyPool {
    /// Pool data
    pool: [u8; 256],
    /// Pool position
    pos: usize,
    /// Estimated entropy bits
    entropy_bits: u32,
    /// Mix counter
    mix_count: u64,
}

impl EntropyPool {
    /// Create new entropy pool
    pub const fn new() -> Self {
        Self {
            pool: [0u8; 256],
            pos: 0,
            entropy_bits: 0,
            mix_count: 0,
        }
    }

    /// Add entropy to the pool
    pub fn add_entropy(&mut self, data: &[u8], bits: u32) {
        for &byte in data {
            self.pool[self.pos] ^= byte;
            self.pos = (self.pos + 1) % 256;
        }

        self.entropy_bits = self.entropy_bits.saturating_add(bits);
        if self.entropy_bits > 256 * 8 {
            self.entropy_bits = 256 * 8;
        }

        // Mix periodically
        if self.mix_count % 256 == 0 {
            self.mix();
        }
        self.mix_count += 1;
    }

    /// Mix the entropy pool
    fn mix(&mut self) {
        // Simple mixing using rotation and XOR
        for i in 0..256 {
            let j = (i + 1) % 256;
            let k = (i + 128) % 256;
            self.pool[i] = self.pool[i]
                .rotate_left(3)
                .wrapping_add(self.pool[j])
                ^ self.pool[k];
        }
    }

    /// Extract entropy from the pool
    pub fn extract(&mut self, dest: &mut [u8]) -> Result<(), CryptoError> {
        if (dest.len() * 8) as u32 > self.entropy_bits {
            return Err(CryptoError::InsufficientEntropy);
        }

        // Hash the pool to extract entropy
        let hash = self.hash_pool();

        let to_copy = core::cmp::min(dest.len(), 32);
        dest[..to_copy].copy_from_slice(&hash[..to_copy]);

        // Reduce entropy estimate
        self.entropy_bits = self.entropy_bits.saturating_sub((to_copy * 8) as u32);

        // Re-mix the pool
        self.mix();

        Ok(())
    }

    /// Hash the pool (simple hash for extraction)
    fn hash_pool(&self) -> [u8; 32] {
        // Simple hash based on pool content
        let mut h = [0u8; 32];

        for i in 0..8 {
            for j in 0..32 {
                h[j] ^= self.pool[i * 32 + j];
            }
        }

        // Additional mixing
        for i in 0..32 {
            h[i] = h[i].rotate_left(3).wrapping_add(h[(i + 1) % 32]);
        }

        h
    }

    /// Get estimated entropy bits
    pub fn available_entropy(&self) -> u32 {
        self.entropy_bits
    }
}

// =============================================================================
// ChaCha20-based CSPRNG
// =============================================================================

/// ChaCha20-based CSPRNG
pub struct ChaCha20Rng {
    /// ChaCha20 key
    key: [u8; 32],
    /// Counter
    counter: u64,
    /// Nonce
    nonce: [u8; 12],
    /// Buffer
    buffer: [u8; 64],
    /// Buffer position
    buf_pos: usize,
    /// Is seeded
    seeded: bool,
}

impl ChaCha20Rng {
    /// Create new ChaCha20 RNG
    pub fn new() -> Self {
        Self {
            key: [0u8; 32],
            counter: 0,
            nonce: [0u8; 12],
            buffer: [0u8; 64],
            buf_pos: 64, // Empty buffer
            seeded: false,
        }
    }

    /// Create seeded RNG
    pub fn from_seed(seed: &[u8]) -> Result<Self, CryptoError> {
        let mut rng = Self::new();
        rng.reseed(seed)?;
        Ok(rng)
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

    /// Generate ChaCha20 block
    fn chacha20_block(&self, counter: u64) -> [u8; 64] {
        let mut state = [0u32; 16];

        // Constants
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
        state[12] = counter as u32;
        state[13] = (counter >> 32) as u32;

        // Nonce
        state[14] = u32::from_le_bytes([self.nonce[0], self.nonce[1], self.nonce[2], self.nonce[3]]);
        state[15] = u32::from_le_bytes([self.nonce[4], self.nonce[5], self.nonce[6], self.nonce[7]]);

        let initial_state = state;

        // 20 rounds
        for _ in 0..10 {
            Self::quarter_round(&mut state, 0, 4, 8, 12);
            Self::quarter_round(&mut state, 1, 5, 9, 13);
            Self::quarter_round(&mut state, 2, 6, 10, 14);
            Self::quarter_round(&mut state, 3, 7, 11, 15);

            Self::quarter_round(&mut state, 0, 5, 10, 15);
            Self::quarter_round(&mut state, 1, 6, 11, 12);
            Self::quarter_round(&mut state, 2, 7, 8, 13);
            Self::quarter_round(&mut state, 3, 4, 9, 14);
        }

        // Add initial state
        for i in 0..16 {
            state[i] = state[i].wrapping_add(initial_state[i]);
        }

        // Serialize
        let mut output = [0u8; 64];
        for i in 0..16 {
            output[i * 4..(i + 1) * 4].copy_from_slice(&state[i].to_le_bytes());
        }

        output
    }

    /// Refill the buffer
    fn refill(&mut self) {
        self.buffer = self.chacha20_block(self.counter);
        self.counter += 1;
        self.buf_pos = 0;
    }
}

impl RngOps for ChaCha20Rng {
    fn name(&self) -> &str {
        "chacha20-rng"
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), CryptoError> {
        if !self.seeded {
            return Err(CryptoError::RngNotSeeded);
        }

        let mut written = 0;

        while written < dest.len() {
            if self.buf_pos >= 64 {
                self.refill();
            }

            let available = 64 - self.buf_pos;
            let to_copy = core::cmp::min(available, dest.len() - written);

            dest[written..written + to_copy]
                .copy_from_slice(&self.buffer[self.buf_pos..self.buf_pos + to_copy]);

            self.buf_pos += to_copy;
            written += to_copy;
        }

        Ok(())
    }

    fn reseed(&mut self, seed: &[u8]) -> Result<(), CryptoError> {
        // XOR seed into key
        for (i, &byte) in seed.iter().enumerate() {
            self.key[i % 32] ^= byte;
        }

        // Update nonce from seed if long enough
        if seed.len() > 32 {
            for (i, &byte) in seed[32..].iter().take(12).enumerate() {
                self.nonce[i] ^= byte;
            }
        }

        // Increment counter
        self.counter = self.counter.wrapping_add(1);

        // Generate new key material
        let new_key = self.chacha20_block(self.counter);
        self.key.copy_from_slice(&new_key[..32]);
        self.counter += 1;

        self.seeded = true;
        self.buf_pos = 64; // Force buffer refill

        Ok(())
    }

    fn is_seeded(&self) -> bool {
        self.seeded
    }
}

// =============================================================================
// Fortuna CSPRNG
// =============================================================================

/// Number of entropy pools in Fortuna
const FORTUNA_POOLS: usize = 32;

/// Fortuna CSPRNG
pub struct Fortuna {
    /// Key
    key: [u8; 32],
    /// Counter
    counter: u128,
    /// Entropy pools
    pools: [[u8; 32]; FORTUNA_POOLS],
    /// Pool positions
    pool_pos: [usize; FORTUNA_POOLS],
    /// Reseed counter
    reseed_count: u64,
    /// Last reseed time
    last_reseed: u64,
    /// Pool index for next entropy add
    add_pool: usize,
}

impl Fortuna {
    /// Create new Fortuna RNG
    pub fn new() -> Self {
        Self {
            key: [0u8; 32],
            counter: 0,
            pools: [[0u8; 32]; FORTUNA_POOLS],
            pool_pos: [0; FORTUNA_POOLS],
            reseed_count: 0,
            last_reseed: 0,
            add_pool: 0,
        }
    }

    /// Add entropy to pools
    pub fn add_random_event(&mut self, _source_id: u8, data: &[u8]) {
        let pool = self.add_pool;

        // XOR data into pool
        for (i, &byte) in data.iter().enumerate() {
            let pos = (self.pool_pos[pool] + i) % 32;
            self.pools[pool][pos] ^= byte;
        }

        self.pool_pos[pool] = (self.pool_pos[pool] + data.len()) % 32;
        self.add_pool = (self.add_pool + 1) % FORTUNA_POOLS;
    }

    /// Check if reseed is needed
    fn should_reseed(&self, now: u64) -> bool {
        // Reseed if pool 0 has data and 100ms have passed
        self.pool_pos[0] > 0 && (now - self.last_reseed) >= 100
    }

    /// Perform reseed
    fn reseed(&mut self, now: u64) {
        self.reseed_count += 1;

        // Collect pools that should be included
        let mut seed_data = Vec::new();

        for i in 0..FORTUNA_POOLS {
            // Pool i is included if 2^i divides reseed_count
            if self.reseed_count % (1 << i) == 0 {
                seed_data.extend_from_slice(&self.pools[i]);
                self.pools[i] = [0u8; 32];
                self.pool_pos[i] = 0;
            }
        }

        // Hash seed data with current key
        let mut new_key = [0u8; 32];
        for i in 0..32 {
            new_key[i] = self.key[i];
        }
        for (i, &byte) in seed_data.iter().enumerate() {
            new_key[i % 32] ^= byte;
        }

        // Simple mixing
        for i in 0..32 {
            new_key[i] = new_key[i]
                .rotate_left(3)
                .wrapping_add(new_key[(i + 1) % 32]);
        }

        self.key = new_key;
        self.counter += 1;
        self.last_reseed = now;
    }

    /// Generate blocks using AES-CTR (simplified as XOR with counter)
    fn generate_blocks(&mut self, dest: &mut [u8]) {
        let mut pos = 0;

        while pos < dest.len() {
            // Generate pseudo-random block from key and counter
            let block = self.generate_block();

            let to_copy = core::cmp::min(16, dest.len() - pos);
            dest[pos..pos + to_copy].copy_from_slice(&block[..to_copy]);

            self.counter += 1;
            pos += 16;
        }

        // Generate new key after every request
        let mut new_key = [0u8; 32];
        new_key[..16].copy_from_slice(&self.generate_block());
        self.counter += 1;
        new_key[16..].copy_from_slice(&self.generate_block());
        self.counter += 1;
        self.key = new_key;
    }

    /// Generate single block
    fn generate_block(&self) -> [u8; 16] {
        let mut block = [0u8; 16];

        // XOR key with counter
        let counter_bytes = self.counter.to_le_bytes();
        for i in 0..16 {
            block[i] = self.key[i] ^ counter_bytes[i];
        }

        // Simple mixing
        for _ in 0..4 {
            for i in 0..16 {
                block[i] = block[i]
                    .rotate_left(3)
                    .wrapping_add(block[(i + 1) % 16])
                    ^ self.key[16 + (i % 16)];
            }
        }

        block
    }
}

impl RngOps for Fortuna {
    fn name(&self) -> &str {
        "fortuna"
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), CryptoError> {
        let now = read_tsc() / 1_000_000; // Approximate milliseconds

        if self.should_reseed(now) {
            self.reseed(now);
        }

        if self.reseed_count == 0 {
            return Err(CryptoError::RngNotSeeded);
        }

        self.generate_blocks(dest);
        Ok(())
    }

    fn reseed(&mut self, seed: &[u8]) -> Result<(), CryptoError> {
        self.add_random_event(0, seed);

        // Force reseed
        let now = read_tsc() / 1_000_000;
        self.reseed(now);

        Ok(())
    }

    fn is_seeded(&self) -> bool {
        self.reseed_count > 0
    }
}

// =============================================================================
// Hardware RNG wrapper
// =============================================================================

/// Hardware RNG wrapper
pub struct HardwareRng;

impl HardwareRng {
    /// Create new hardware RNG
    pub fn new() -> Result<Self, CryptoError> {
        if !has_hardware_rng() {
            return Err(CryptoError::HardwareNotAvailable);
        }
        Ok(Self)
    }
}

impl RngOps for HardwareRng {
    fn name(&self) -> &str {
        if HAS_RDSEED.load(Ordering::Relaxed) {
            "rdseed"
        } else {
            "rdrand"
        }
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), CryptoError> {
        if hardware_random(dest) {
            Ok(())
        } else {
            Err(CryptoError::RngFailed)
        }
    }

    fn reseed(&mut self, _seed: &[u8]) -> Result<(), CryptoError> {
        // Hardware RNG doesn't need reseeding
        Ok(())
    }

    fn is_seeded(&self) -> bool {
        true
    }
}

// =============================================================================
// Combined RNG
// =============================================================================

/// Combined hardware + software RNG
pub struct CombinedRng {
    /// ChaCha20 RNG
    chacha: ChaCha20Rng,
    /// Has hardware
    has_hardware: bool,
}

impl CombinedRng {
    /// Create new combined RNG
    pub fn new() -> Result<Self, CryptoError> {
        let mut chacha = ChaCha20Rng::new();
        let has_hardware = has_hardware_rng();

        // Seed from hardware if available
        if has_hardware {
            let mut seed = [0u8; 32];
            if hardware_random(&mut seed) {
                chacha.reseed(&seed)?;
            }
        }

        // Also seed from TSC
        let tsc = read_tsc();
        chacha.reseed(&tsc.to_le_bytes())?;

        Ok(Self {
            chacha,
            has_hardware,
        })
    }
}

impl RngOps for CombinedRng {
    fn name(&self) -> &str {
        "combined-rng"
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), CryptoError> {
        // Generate from ChaCha20
        self.chacha.fill_bytes(dest)?;

        // XOR with hardware random if available
        if self.has_hardware {
            let mut hw_bytes = vec![0u8; dest.len()];
            if hardware_random(&mut hw_bytes) {
                for (d, h) in dest.iter_mut().zip(hw_bytes.iter()) {
                    *d ^= *h;
                }
            }
        }

        Ok(())
    }

    fn reseed(&mut self, seed: &[u8]) -> Result<(), CryptoError> {
        self.chacha.reseed(seed)?;

        // Also mix in hardware random
        if self.has_hardware {
            let mut hw_seed = [0u8; 32];
            if hardware_random(&mut hw_seed) {
                self.chacha.reseed(&hw_seed)?;
            }
        }

        Ok(())
    }

    fn is_seeded(&self) -> bool {
        self.chacha.is_seeded()
    }
}

// =============================================================================
// Factory Functions
// =============================================================================

/// Create RNG by algorithm
pub fn create_rng(algorithm: RngAlgorithm) -> Result<Box<dyn RngOps>, CryptoError> {
    match algorithm {
        RngAlgorithm::Hardware => Ok(Box::new(HardwareRng::new()?)),
        RngAlgorithm::ChaCha20 => {
            let mut rng = ChaCha20Rng::new();
            // Auto-seed from hardware or TSC
            let tsc = read_tsc();
            rng.reseed(&tsc.to_le_bytes())?;
            if has_hardware_rng() {
                let mut seed = [0u8; 32];
                if hardware_random(&mut seed) {
                    rng.reseed(&seed)?;
                }
            }
            Ok(Box::new(rng))
        }
        RngAlgorithm::Combined => Ok(Box::new(CombinedRng::new()?)),
        RngAlgorithm::Fortuna => {
            let mut rng = Fortuna::new();
            // Seed from available sources
            let tsc = read_tsc();
            rng.add_random_event(0, &tsc.to_le_bytes());
            if has_hardware_rng() {
                let mut seed = [0u8; 32];
                if hardware_random(&mut seed) {
                    rng.add_random_event(1, &seed);
                }
            }
            Ok(Box::new(rng))
        }
    }
}

/// Securely generate random bytes
pub fn secure_random(dest: &mut [u8]) -> Result<(), CryptoError> {
    if let Some(ref mut rng) = *SYSTEM_RNG.lock() {
        rng.fill_bytes(dest)
    } else {
        Err(CryptoError::RngNotSeeded)
    }
}
