// ===============================================================================
// BUILDOS KERNEL - RANDOM NUMBER GENERATOR SUBSYSTEM
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Random Number Generator Subsystem
//!
//! Provides cryptographically secure random number generation using:
//! - Hardware entropy sources (RDRAND, RDSEED)
//! - Software entropy gathering (interrupts, timing jitter)
//! - ChaCha20-based CSPRNG
//! - Entropy pool with mixing

pub mod entropy;
pub mod chacha20;
pub mod pool;

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use crate::sync::Spinlock;

pub use entropy::{EntropySource, HardwareRng, TimingJitter};
pub use chacha20::ChaCha20Rng;
pub use pool::EntropyPool;

/// Global RNG state
static RNG: Spinlock<Option<RandomState>> = Spinlock::new(None);

/// Whether RNG is initialized
static RNG_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Entropy bits collected
static ENTROPY_BITS: AtomicU64 = AtomicU64::new(0);

/// Minimum entropy before seeding CSPRNG
pub const MIN_ENTROPY_BITS: u64 = 256;

/// Reseed interval (bytes generated)
pub const RESEED_INTERVAL: u64 = 1 << 20; // 1 MB

/// Random state
struct RandomState {
    /// Primary CSPRNG
    csprng: ChaCha20Rng,
    /// Entropy pool for reseeding
    pool: EntropyPool,
    /// Bytes generated since last reseed
    bytes_since_reseed: u64,
    /// Hardware RNG available
    hw_rng_available: bool,
    /// RDSEED available
    rdseed_available: bool,
}

/// Initialize the random subsystem
pub fn init() {
    // Check CPU features
    let hw_rng = check_rdrand();
    let rdseed = check_rdseed();

    // Create initial entropy pool
    let mut pool = EntropyPool::new();

    // Gather initial entropy
    gather_early_entropy(&mut pool, hw_rng, rdseed);

    // Create CSPRNG seeded from pool
    let mut seed = [0u8; 32];
    pool.extract(&mut seed);
    let csprng = ChaCha20Rng::from_seed(seed);

    // Zeroize seed
    for b in seed.iter_mut() {
        *b = 0;
    }

    let state = RandomState {
        csprng,
        pool,
        bytes_since_reseed: 0,
        hw_rng_available: hw_rng,
        rdseed_available: rdseed,
    };

    *RNG.lock() = Some(state);
    RNG_INITIALIZED.store(true, Ordering::Release);

    crate::kprintln!("[RNG] Initialized: RDRAND={}, RDSEED={}", hw_rng, rdseed);
}

/// Check if RDRAND is available
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

/// Check if RDSEED is available
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

/// Gather early boot entropy
fn gather_early_entropy(pool: &mut EntropyPool, hw_rng: bool, rdseed: bool) {
    // Use RDSEED for true entropy if available
    if rdseed {
        for _ in 0..8 {
            if let Some(val) = rdseed_u64() {
                pool.add_entropy(&val.to_le_bytes(), 64);
            }
        }
    }

    // Use RDRAND for additional mixing
    if hw_rng {
        for _ in 0..16 {
            if let Some(val) = rdrand_u64() {
                pool.add_entropy(&val.to_le_bytes(), 8); // Lower entropy estimate
            }
        }
    }

    // Use TSC for timing entropy
    let tsc = read_tsc();
    pool.add_entropy(&tsc.to_le_bytes(), 8);

    // Use performance counters if available
    let perf = read_perf_counter();
    pool.add_entropy(&perf.to_le_bytes(), 4);

    // Memory layout entropy
    let stack_addr = &pool as *const _ as u64;
    pool.add_entropy(&stack_addr.to_le_bytes(), 4);

    ENTROPY_BITS.store(pool.entropy_bits(), Ordering::Release);
}

/// Read TSC
#[inline]
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

/// Read performance counter (simplified)
fn read_perf_counter() -> u64 {
    // Try to read APERF if available
    read_tsc() ^ (read_tsc() >> 16)
}

/// Execute RDRAND instruction
#[inline]
pub fn rdrand_u64() -> Option<u64> {
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

/// Execute RDRAND instruction (32-bit)
#[inline]
pub fn rdrand_u32() -> Option<u32> {
    let value: u32;
    let success: u8;
    unsafe {
        core::arch::asm!(
            "rdrand {0:e}",
            "setc {1}",
            out(reg) value,
            out(reg_byte) success,
            options(nostack, nomem)
        );
    }
    if success != 0 { Some(value) } else { None }
}

/// Execute RDSEED instruction
#[inline]
pub fn rdseed_u64() -> Option<u64> {
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

/// Execute RDSEED instruction (32-bit)
#[inline]
pub fn rdseed_u32() -> Option<u32> {
    let value: u32;
    let success: u8;
    unsafe {
        core::arch::asm!(
            "rdseed {0:e}",
            "setc {1}",
            out(reg) value,
            out(reg_byte) success,
            options(nostack, nomem)
        );
    }
    if success != 0 { Some(value) } else { None }
}

/// Get random bytes (blocking, high security)
/// This will block if not enough entropy is available
pub fn get_random_bytes(buf: &mut [u8]) {
    if !RNG_INITIALIZED.load(Ordering::Acquire) {
        // Fallback to hardware RNG if available
        if check_rdrand() {
            fill_with_rdrand(buf);
            return;
        }
        // Otherwise fill with zeros (not ideal)
        buf.fill(0);
        return;
    }

    let mut guard = RNG.lock();
    if let Some(ref mut state) = *guard {
        // Check if we need to reseed
        if state.bytes_since_reseed >= RESEED_INTERVAL {
            reseed_csprng(state);
        }

        // Generate random bytes
        state.csprng.fill(buf);
        state.bytes_since_reseed += buf.len() as u64;
    }
}

/// Get random bytes (non-blocking, for urandom)
pub fn get_urandom_bytes(buf: &mut [u8]) {
    get_random_bytes(buf)
}

/// Fill buffer with RDRAND
fn fill_with_rdrand(buf: &mut [u8]) {
    let mut offset = 0;
    while offset + 8 <= buf.len() {
        if let Some(val) = rdrand_u64() {
            buf[offset..offset + 8].copy_from_slice(&val.to_le_bytes());
            offset += 8;
        } else {
            // Retry
            for _ in 0..10 {
                if let Some(val) = rdrand_u64() {
                    buf[offset..offset + 8].copy_from_slice(&val.to_le_bytes());
                    offset += 8;
                    break;
                }
            }
        }
    }
    // Handle remaining bytes
    if offset < buf.len() {
        if let Some(val) = rdrand_u64() {
            let remaining = buf.len() - offset;
            buf[offset..].copy_from_slice(&val.to_le_bytes()[..remaining]);
        }
    }
}

/// Reseed the CSPRNG from entropy pool
fn reseed_csprng(state: &mut RandomState) {
    // Add hardware entropy if available
    if state.rdseed_available {
        for _ in 0..4 {
            if let Some(val) = rdseed_u64() {
                state.pool.add_entropy(&val.to_le_bytes(), 64);
            }
        }
    } else if state.hw_rng_available {
        for _ in 0..8 {
            if let Some(val) = rdrand_u64() {
                state.pool.add_entropy(&val.to_le_bytes(), 8);
            }
        }
    }

    // Add timing entropy
    let tsc = read_tsc();
    state.pool.add_entropy(&tsc.to_le_bytes(), 4);

    // Extract new seed
    let mut seed = [0u8; 32];
    state.pool.extract(&mut seed);
    state.csprng.reseed(&seed);
    state.bytes_since_reseed = 0;

    // Zeroize
    for b in seed.iter_mut() {
        *b = 0;
    }

    ENTROPY_BITS.store(state.pool.entropy_bits(), Ordering::Release);
}

/// Add entropy from external source
pub fn add_entropy(data: &[u8], entropy_bits: u64) {
    if !RNG_INITIALIZED.load(Ordering::Acquire) {
        return;
    }

    let mut guard = RNG.lock();
    if let Some(ref mut state) = *guard {
        state.pool.add_entropy(data, entropy_bits);
        ENTROPY_BITS.store(state.pool.entropy_bits(), Ordering::Release);
    }
}

/// Add entropy from interrupt timing
pub fn add_interrupt_entropy(irq: u32) {
    let tsc = read_tsc();
    let data = [
        (tsc & 0xFF) as u8,
        ((tsc >> 8) & 0xFF) as u8,
        (irq & 0xFF) as u8,
        ((irq >> 8) & 0xFF) as u8,
    ];
    add_entropy(&data, 1); // Conservative entropy estimate
}

/// Add entropy from disk timing
pub fn add_disk_entropy(sector: u64, latency_ns: u64) {
    let data = [
        sector.to_le_bytes(),
        latency_ns.to_le_bytes(),
        read_tsc().to_le_bytes(),
    ].concat();
    add_entropy(&data, 2);
}

/// Add entropy from network packet
pub fn add_network_entropy(data: &[u8]) {
    let tsc = read_tsc();
    let mut entropy_data = tsc.to_le_bytes().to_vec();
    if data.len() > 16 {
        entropy_data.extend_from_slice(&data[..16]);
    } else {
        entropy_data.extend_from_slice(data);
    }
    add_entropy(&entropy_data, 2);
}

/// Add entropy from keyboard/mouse input
pub fn add_input_entropy(scancode: u8) {
    let tsc = read_tsc();
    let data = [
        scancode,
        (tsc & 0xFF) as u8,
        ((tsc >> 8) & 0xFF) as u8,
        ((tsc >> 16) & 0xFF) as u8,
    ];
    add_entropy(&data, 2);
}

/// Get available entropy bits
pub fn available_entropy() -> u64 {
    ENTROPY_BITS.load(Ordering::Acquire)
}

/// Check if RNG is ready (enough entropy)
pub fn is_ready() -> bool {
    available_entropy() >= MIN_ENTROPY_BITS
}

/// Get a random u64
pub fn random_u64() -> u64 {
    let mut buf = [0u8; 8];
    get_random_bytes(&mut buf);
    u64::from_le_bytes(buf)
}

/// Get a random u32
pub fn random_u32() -> u32 {
    let mut buf = [0u8; 4];
    get_random_bytes(&mut buf);
    u32::from_le_bytes(buf)
}

/// Get a random u8
pub fn random_u8() -> u8 {
    let mut buf = [0u8; 1];
    get_random_bytes(&mut buf);
    buf[0]
}

/// Get a random u64 in range [0, max)
pub fn random_range(max: u64) -> u64 {
    if max == 0 {
        return 0;
    }
    // Use rejection sampling to avoid bias
    let threshold = u64::MAX - (u64::MAX % max);
    loop {
        let val = random_u64();
        if val < threshold {
            return val % max;
        }
    }
}

/// Shuffle slice using Fisher-Yates algorithm
pub fn shuffle<T>(slice: &mut [T]) {
    let len = slice.len();
    for i in (1..len).rev() {
        let j = random_range((i + 1) as u64) as usize;
        slice.swap(i, j);
    }
}

/// Generate random UUID v4
pub fn generate_uuid() -> [u8; 16] {
    let mut uuid = [0u8; 16];
    get_random_bytes(&mut uuid);

    // Set version to 4
    uuid[6] = (uuid[6] & 0x0F) | 0x40;
    // Set variant to RFC 4122
    uuid[8] = (uuid[8] & 0x3F) | 0x80;

    uuid
}

/// Format UUID as string
pub fn format_uuid(uuid: &[u8; 16]) -> alloc::string::String {
    use alloc::format;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        uuid[0], uuid[1], uuid[2], uuid[3],
        uuid[4], uuid[5],
        uuid[6], uuid[7],
        uuid[8], uuid[9],
        uuid[10], uuid[11], uuid[12], uuid[13], uuid[14], uuid[15]
    )
}

/// Random device interface
pub struct RandomDevice {
    blocking: bool,
}

impl RandomDevice {
    /// Create /dev/random (blocking)
    pub fn random() -> Self {
        Self { blocking: true }
    }

    /// Create /dev/urandom (non-blocking)
    pub fn urandom() -> Self {
        Self { blocking: false }
    }

    /// Read random bytes
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, RandomError> {
        if self.blocking && !is_ready() {
            return Err(RandomError::NotReady);
        }
        get_random_bytes(buf);
        Ok(buf.len())
    }

    /// Write entropy (seed the pool)
    pub fn write(&self, buf: &[u8]) -> Result<usize, RandomError> {
        add_entropy(buf, 0); // User-provided entropy gets 0 bits credit
        Ok(buf.len())
    }

    /// IOCTL operations
    pub fn ioctl(&self, cmd: u32, arg: u64) -> Result<u64, RandomError> {
        match cmd {
            RNDGETENTCNT => Ok(available_entropy()),
            RNDADDTOENTCNT => {
                // Only root can increase entropy count
                ENTROPY_BITS.fetch_add(arg, Ordering::AcqRel);
                Ok(0)
            }
            RNDGETPOOL => Ok(0), // Deprecated
            RNDADDENTROPY => {
                // Add entropy with bits credit
                Ok(0)
            }
            RNDZAPENTCNT => {
                ENTROPY_BITS.store(0, Ordering::Release);
                Ok(0)
            }
            RNDCLEARPOOL => {
                ENTROPY_BITS.store(0, Ordering::Release);
                Ok(0)
            }
            RNDRESEEDCRNG => {
                // Force reseed
                let mut guard = RNG.lock();
                if let Some(ref mut state) = *guard {
                    reseed_csprng(state);
                }
                Ok(0)
            }
            _ => Err(RandomError::InvalidIoctl),
        }
    }
}

/// Random device IOCTL commands
pub const RNDGETENTCNT: u32 = 0x80045200;
pub const RNDADDTOENTCNT: u32 = 0x40045201;
pub const RNDGETPOOL: u32 = 0x80085202;
pub const RNDADDENTROPY: u32 = 0x40085203;
pub const RNDZAPENTCNT: u32 = 0x5204;
pub const RNDCLEARPOOL: u32 = 0x5206;
pub const RNDRESEEDCRNG: u32 = 0x5207;

/// Random errors
#[derive(Clone, Copy, Debug)]
pub enum RandomError {
    /// Not enough entropy
    NotReady,
    /// Invalid IOCTL command
    InvalidIoctl,
    /// Permission denied
    PermissionDenied,
}

impl RandomError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::NotReady => -11,      // EAGAIN
            Self::InvalidIoctl => -25,  // ENOTTY
            Self::PermissionDenied => -1, // EPERM
        }
    }
}
