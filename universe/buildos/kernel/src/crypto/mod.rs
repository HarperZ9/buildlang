// ===============================================================================
// BUILDOS KERNEL - CRYPTOGRAPHIC SUBSYSTEM
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.

//! Kernel Cryptographic Subsystem
//!
//! This module provides cryptographic primitives for the kernel:
//! - Symmetric ciphers (AES, ChaCha20)
//! - Hash algorithms (SHA-256, SHA-512, BLAKE2)
//! - Message authentication codes (HMAC)
//! - Authenticated encryption (AES-GCM, ChaCha20-Poly1305)
//! - Random number generation
//! - Key derivation functions (HKDF, PBKDF2)

#![allow(dead_code)]

pub mod cipher;
pub mod hash;
pub mod mac;
pub mod aead;
pub mod rng;
pub mod kdf;
pub mod af_alg;

pub use cipher::{CipherAlgorithm, CipherMode, CipherOps};
pub use hash::{HashAlgorithm, HashOps, Digest};
pub use mac::{Mac, MacAlgorithm, MacOps};
pub use aead::{Aead, AeadAlgorithm, AeadOps};
pub use rng::{RngAlgorithm, RngOps};
pub use kdf::{KdfAlgorithm, KdfOps, Hkdf, Pbkdf2, Scrypt, Argon2};

use alloc::collections::BTreeMap;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use crate::sync::{Mutex, RwLock};

/// Crypto subsystem error types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CryptoError {
    /// Algorithm not available
    AlgorithmNotFound,
    /// Invalid key size
    InvalidKeySize,
    /// Invalid IV/nonce size
    InvalidIvSize,
    /// Invalid nonce size (alias)
    InvalidNonceSize,
    /// Invalid input size
    InvalidInputSize,
    /// Invalid ciphertext
    InvalidCiphertext,
    /// Invalid output length
    InvalidOutputLength,
    /// Invalid parameter
    InvalidParameter,
    /// Invalid operation
    InvalidOperation,
    /// Authentication failed
    AuthenticationFailed,
    /// Buffer too small
    BufferTooSmall,
    /// Operation failed
    OperationFailed,
    /// Out of memory
    OutOfMemory,
    /// Not initialized
    NotInitialized,
    /// Hardware error
    HardwareError,
    /// Hardware not available
    HardwareNotAvailable,
    /// Permission denied
    PermissionDenied,
    /// RNG failed
    RngFailed,
    /// RNG not seeded
    RngNotSeeded,
    /// Insufficient entropy
    InsufficientEntropy,
    /// Socket error
    SocketError,
}

impl CryptoError {
    /// Convert to errno
    pub fn to_errno(self) -> i32 {
        match self {
            Self::AlgorithmNotFound => -22,     // EINVAL
            Self::InvalidKeySize => -22,        // EINVAL
            Self::InvalidIvSize => -22,         // EINVAL
            Self::InvalidNonceSize => -22,      // EINVAL
            Self::InvalidInputSize => -22,      // EINVAL
            Self::InvalidCiphertext => -74,     // EBADMSG
            Self::InvalidOutputLength => -22,   // EINVAL
            Self::InvalidParameter => -22,      // EINVAL
            Self::InvalidOperation => -22,      // EINVAL
            Self::AuthenticationFailed => -74,  // EBADMSG
            Self::BufferTooSmall => -90,        // EMSGSIZE
            Self::OperationFailed => -5,        // EIO
            Self::OutOfMemory => -12,           // ENOMEM
            Self::NotInitialized => -22,        // EINVAL
            Self::HardwareError => -5,          // EIO
            Self::HardwareNotAvailable => -38,  // ENOSYS
            Self::PermissionDenied => -1,       // EPERM
            Self::RngFailed => -5,              // EIO
            Self::RngNotSeeded => -11,          // EAGAIN
            Self::InsufficientEntropy => -11,   // EAGAIN
            Self::SocketError => -88,           // ENOTSOCK
        }
    }
}

/// Algorithm type
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlgorithmType {
    /// Cipher (symmetric encryption)
    Cipher,
    /// Hash (digest)
    Hash,
    /// MAC (message authentication code)
    Mac,
    /// AEAD (authenticated encryption)
    Aead,
    /// RNG (random number generator)
    Rng,
    /// KDF (key derivation function)
    Kdf,
    /// Compression
    Compress,
}

// Algorithm flags
bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, Default)]
    pub struct AlgorithmFlags: u32 {
        /// Hardware accelerated
        const HARDWARE = 1 << 0;
        /// Async capable
        const ASYNC = 1 << 1;
        /// Needs fallback
        const NEED_FALLBACK = 1 << 2;
        /// Internal use only
        const INTERNAL = 1 << 3;
        /// FIPS approved
        const FIPS = 1 << 4;
    }
}

/// Algorithm registration
#[derive(Clone, Debug)]
pub struct AlgorithmInfo {
    /// Algorithm name
    pub name: String,
    /// Driver name
    pub driver_name: String,
    /// Algorithm type
    pub algo_type: AlgorithmType,
    /// Priority (higher = preferred)
    pub priority: u32,
    /// Flags
    pub flags: AlgorithmFlags,
    /// Block size (for ciphers)
    pub block_size: usize,
    /// Key sizes supported
    pub key_sizes: Vec<usize>,
    /// IV size (for ciphers)
    pub iv_size: usize,
    /// Digest size (for hashes)
    pub digest_size: usize,
    /// Max auth tag size (for AEAD)
    pub max_auth_size: usize,
}

/// Crypto algorithm registry
pub struct CryptoRegistry {
    /// Registered algorithms by name
    algorithms: RwLock<BTreeMap<String, AlgorithmInfo>>,
    /// Algorithms by type
    by_type: RwLock<BTreeMap<AlgorithmType, Vec<String>>>,
    /// Next algorithm ID
    next_id: AtomicU64,
    /// Hardware acceleration available
    hw_available: AtomicBool,
}

impl CryptoRegistry {
    /// Create new registry
    pub const fn new() -> Self {
        Self {
            algorithms: RwLock::new(BTreeMap::new()),
            by_type: RwLock::new(BTreeMap::new()),
            next_id: AtomicU64::new(1),
            hw_available: AtomicBool::new(false),
        }
    }

    /// Register an algorithm
    pub fn register(&self, info: AlgorithmInfo) {
        let name = info.name.clone();
        let algo_type = info.algo_type;
        self.algorithms.write().insert(name.clone(), info);
        self.by_type.write()
            .entry(algo_type)
            .or_insert_with(Vec::new)
            .push(name);
    }

    /// Get algorithm info
    pub fn get(&self, name: &str) -> Option<AlgorithmInfo> {
        self.algorithms.read().get(name).cloned()
    }

    /// Get best algorithm of type
    pub fn get_best(&self, algo_type: AlgorithmType) -> Option<String> {
        let by_type = self.by_type.read();
        let algos = by_type.get(&algo_type)?;
        let algorithms = self.algorithms.read();
        algos.iter()
            .filter_map(|name| algorithms.get(name).map(|info| (name.clone(), info.priority)))
            .max_by_key(|(_, priority)| *priority)
            .map(|(name, _)| name)
    }

    /// List all algorithms
    pub fn list(&self) -> Vec<AlgorithmInfo> {
        self.algorithms.read().values().cloned().collect()
    }

    /// List algorithms by type
    pub fn list_by_type(&self, algo_type: AlgorithmType) -> Vec<String> {
        self.by_type.read()
            .get(&algo_type)
            .cloned()
            .unwrap_or_default()
    }
}

/// Global crypto registry
static REGISTRY: Mutex<Option<CryptoRegistry>> = Mutex::new(None);

/// Crypto statistics
#[derive(Debug, Default)]
pub struct CryptoStats {
    /// Encryption operations
    pub encryptions: AtomicU64,
    /// Decryption operations
    pub decryptions: AtomicU64,
    /// Hash operations
    pub hashes: AtomicU64,
    /// MAC operations
    pub macs: AtomicU64,
    /// AEAD operations
    pub aeads: AtomicU64,
    /// KDF operations
    pub kdf_ops: AtomicU64,
    /// RNG bytes generated
    pub rng_bytes: AtomicU64,
    /// Hardware accelerated operations
    pub hw_ops: AtomicU64,
    /// Software fallback operations
    pub sw_ops: AtomicU64,
    /// Errors
    pub errors: AtomicU64,
}

pub(crate) static STATS: CryptoStats = CryptoStats {
    encryptions: AtomicU64::new(0),
    decryptions: AtomicU64::new(0),
    hashes: AtomicU64::new(0),
    macs: AtomicU64::new(0),
    aeads: AtomicU64::new(0),
    kdf_ops: AtomicU64::new(0),
    rng_bytes: AtomicU64::new(0),
    hw_ops: AtomicU64::new(0),
    sw_ops: AtomicU64::new(0),
    errors: AtomicU64::new(0),
};

/// Initialize crypto subsystem
pub fn init() {
    let registry = CryptoRegistry::new();

    // Detect hardware acceleration
    let hw_caps = detect_hw_capabilities();
    registry.hw_available.store(hw_caps.aes_ni || hw_caps.sha_ni, Ordering::Release);

    // Register built-in algorithms
    register_builtin_algorithms(&registry, &hw_caps);

    *REGISTRY.lock() = Some(registry);

    // Initialize RNG
    rng::init();

    crate::kprintln!("[CRYPTO] Cryptographic subsystem initialized");
    if hw_caps.aes_ni {
        crate::kprintln!("[CRYPTO] AES-NI hardware acceleration available");
    }
    if hw_caps.sha_ni {
        crate::kprintln!("[CRYPTO] SHA-NI hardware acceleration available");
    }
}

/// Hardware capabilities
#[derive(Clone, Copy, Debug, Default)]
pub struct HwCapabilities {
    /// AES-NI available
    pub aes_ni: bool,
    /// SHA extensions available
    pub sha_ni: bool,
    /// AVX available
    pub avx: bool,
    /// AVX2 available
    pub avx2: bool,
    /// AVX-512 available
    pub avx512: bool,
    /// PCLMULQDQ (carry-less multiplication)
    pub pclmulqdq: bool,
    /// RDRAND instruction
    pub rdrand: bool,
    /// RDSEED instruction
    pub rdseed: bool,
}

/// Detect CPU hardware capabilities
fn detect_hw_capabilities() -> HwCapabilities {
    let mut caps = HwCapabilities::default();

    // CPUID check
    let cpuid = cpuid(1);

    // ECX flags
    caps.aes_ni = (cpuid.ecx & (1 << 25)) != 0;
    caps.pclmulqdq = (cpuid.ecx & (1 << 1)) != 0;
    caps.avx = (cpuid.ecx & (1 << 28)) != 0;
    caps.rdrand = (cpuid.ecx & (1 << 30)) != 0;

    // Extended features (leaf 7)
    let cpuid7 = cpuid_subleaf(7, 0);

    // EBX flags
    caps.avx2 = (cpuid7.ebx & (1 << 5)) != 0;
    caps.sha_ni = (cpuid7.ebx & (1 << 29)) != 0;
    caps.rdseed = (cpuid7.ebx & (1 << 18)) != 0;
    caps.avx512 = (cpuid7.ebx & (1 << 16)) != 0;

    caps
}

/// Register built-in algorithms
fn register_builtin_algorithms(registry: &CryptoRegistry, hw: &HwCapabilities) {
    // Ciphers
    registry.register(AlgorithmInfo {
        name: String::from("aes"),
        driver_name: if hw.aes_ni { String::from("aes-aesni") } else { String::from("aes-generic") },
        algo_type: AlgorithmType::Cipher,
        priority: if hw.aes_ni { 300 } else { 100 },
        flags: if hw.aes_ni { AlgorithmFlags::HARDWARE | AlgorithmFlags::FIPS } else { AlgorithmFlags::FIPS },
        block_size: 16,
        key_sizes: vec![16, 24, 32],
        iv_size: 16,
        digest_size: 0,
        max_auth_size: 0,
    });

    registry.register(AlgorithmInfo {
        name: String::from("chacha20"),
        driver_name: String::from("chacha20-generic"),
        algo_type: AlgorithmType::Cipher,
        priority: 100,
        flags: AlgorithmFlags::empty(),
        block_size: 1,
        key_sizes: vec![32],
        iv_size: 16,
        digest_size: 0,
        max_auth_size: 0,
    });

    // Hashes
    registry.register(AlgorithmInfo {
        name: String::from("sha256"),
        driver_name: if hw.sha_ni { String::from("sha256-ni") } else { String::from("sha256-generic") },
        algo_type: AlgorithmType::Hash,
        priority: if hw.sha_ni { 300 } else { 100 },
        flags: if hw.sha_ni { AlgorithmFlags::HARDWARE | AlgorithmFlags::FIPS } else { AlgorithmFlags::FIPS },
        block_size: 64,
        key_sizes: vec![],
        iv_size: 0,
        digest_size: 32,
        max_auth_size: 0,
    });

    registry.register(AlgorithmInfo {
        name: String::from("sha512"),
        driver_name: String::from("sha512-generic"),
        algo_type: AlgorithmType::Hash,
        priority: 100,
        flags: AlgorithmFlags::FIPS,
        block_size: 128,
        key_sizes: vec![],
        iv_size: 0,
        digest_size: 64,
        max_auth_size: 0,
    });

    registry.register(AlgorithmInfo {
        name: String::from("blake2b-256"),
        driver_name: String::from("blake2b-256-generic"),
        algo_type: AlgorithmType::Hash,
        priority: 100,
        flags: AlgorithmFlags::empty(),
        block_size: 128,
        key_sizes: vec![],
        iv_size: 0,
        digest_size: 32,
        max_auth_size: 0,
    });

    registry.register(AlgorithmInfo {
        name: String::from("blake2s-256"),
        driver_name: String::from("blake2s-256-generic"),
        algo_type: AlgorithmType::Hash,
        priority: 100,
        flags: AlgorithmFlags::empty(),
        block_size: 64,
        key_sizes: vec![],
        iv_size: 0,
        digest_size: 32,
        max_auth_size: 0,
    });

    // MACs
    registry.register(AlgorithmInfo {
        name: String::from("hmac(sha256)"),
        driver_name: String::from("hmac-sha256"),
        algo_type: AlgorithmType::Mac,
        priority: 100,
        flags: AlgorithmFlags::FIPS,
        block_size: 64,
        key_sizes: vec![],
        iv_size: 0,
        digest_size: 32,
        max_auth_size: 32,
    });

    registry.register(AlgorithmInfo {
        name: String::from("hmac(sha512)"),
        driver_name: String::from("hmac-sha512"),
        algo_type: AlgorithmType::Mac,
        priority: 100,
        flags: AlgorithmFlags::FIPS,
        block_size: 128,
        key_sizes: vec![],
        iv_size: 0,
        digest_size: 64,
        max_auth_size: 64,
    });

    registry.register(AlgorithmInfo {
        name: String::from("poly1305"),
        driver_name: String::from("poly1305-generic"),
        algo_type: AlgorithmType::Mac,
        priority: 100,
        flags: AlgorithmFlags::empty(),
        block_size: 16,
        key_sizes: vec![32],
        iv_size: 0,
        digest_size: 16,
        max_auth_size: 16,
    });

    // AEADs
    registry.register(AlgorithmInfo {
        name: String::from("gcm(aes)"),
        driver_name: if hw.aes_ni && hw.pclmulqdq {
            String::from("gcm-aes-aesni")
        } else {
            String::from("gcm-aes-generic")
        },
        algo_type: AlgorithmType::Aead,
        priority: if hw.aes_ni && hw.pclmulqdq { 400 } else { 100 },
        flags: if hw.aes_ni && hw.pclmulqdq { AlgorithmFlags::HARDWARE | AlgorithmFlags::FIPS } else { AlgorithmFlags::FIPS },
        block_size: 16,
        key_sizes: vec![16, 24, 32],
        iv_size: 12,
        digest_size: 0,
        max_auth_size: 16,
    });

    registry.register(AlgorithmInfo {
        name: String::from("chacha20poly1305"),
        driver_name: String::from("chacha20poly1305-generic"),
        algo_type: AlgorithmType::Aead,
        priority: 100,
        flags: AlgorithmFlags::empty(),
        block_size: 1,
        key_sizes: vec![32],
        iv_size: 12,
        digest_size: 0,
        max_auth_size: 16,
    });

    // KDFs
    registry.register(AlgorithmInfo {
        name: String::from("hkdf(sha256)"),
        driver_name: String::from("hkdf-sha256"),
        algo_type: AlgorithmType::Kdf,
        priority: 100,
        flags: AlgorithmFlags::FIPS,
        block_size: 0,
        key_sizes: vec![],
        iv_size: 0,
        digest_size: 32,
        max_auth_size: 0,
    });

    // RNG
    registry.register(AlgorithmInfo {
        name: String::from("drbg_nopr_ctr_aes256"),
        driver_name: String::from("drbg-ctr-aes256"),
        algo_type: AlgorithmType::Rng,
        priority: 200,
        flags: AlgorithmFlags::FIPS,
        block_size: 0,
        key_sizes: vec![],
        iv_size: 0,
        digest_size: 0,
        max_auth_size: 0,
    });

    if hw.rdrand {
        registry.register(AlgorithmInfo {
            name: String::from("rdrand"),
            driver_name: String::from("rdrand-hw"),
            algo_type: AlgorithmType::Rng,
            priority: 300,
            flags: AlgorithmFlags::HARDWARE,
            block_size: 0,
            key_sizes: vec![],
            iv_size: 0,
            digest_size: 0,
            max_auth_size: 0,
        });
    }
}

/// Get crypto statistics
pub fn stats() -> (u64, u64, u64, u64, u64, u64) {
    (
        STATS.encryptions.load(Ordering::Relaxed),
        STATS.decryptions.load(Ordering::Relaxed),
        STATS.hashes.load(Ordering::Relaxed),
        STATS.aeads.load(Ordering::Relaxed),
        STATS.hw_ops.load(Ordering::Relaxed),
        STATS.errors.load(Ordering::Relaxed),
    )
}

/// Allocate a cipher by name
pub fn alloc_cipher(name: &str, key: &[u8]) -> Result<Box<dyn cipher::CipherOps>, CryptoError> {
    cipher::allocate(name, key)
}

/// Allocate a hash by name
pub fn alloc_hash(name: &str) -> Result<Box<dyn hash::HashOps>, CryptoError> {
    hash::allocate(name)
}

/// Allocate an AEAD by name
pub fn alloc_aead(name: &str, key: &[u8]) -> Result<Box<dyn aead::AeadOps>, CryptoError> {
    aead::allocate(name, key)
}

/// Get random bytes
pub fn get_random_bytes(buf: &mut [u8]) -> Result<(), CryptoError> {
    rng::secure_random(buf)
}

/// Generate random bytes (convenience function)
pub fn generate_random_bytes(len: usize) -> Result<Vec<u8>, CryptoError> {
    rng::random_bytes(len)
}

/// Initialize AF_ALG subsystem
pub fn init_af_alg() {
    af_alg::init();
}

// CPUID helpers
#[derive(Clone, Copy)]
struct CpuidResult {
    eax: u32,
    ebx: u32,
    ecx: u32,
    edx: u32,
}

fn cpuid(leaf: u32) -> CpuidResult {
    let (eax, ebx, ecx, edx): (u32, u32, u32, u32);
    unsafe {
        core::arch::asm!(
            "push rbx",
            "cpuid",
            "mov {ebx_out:e}, ebx",
            "pop rbx",
            inout("eax") leaf => eax,
            ebx_out = out(reg) ebx,
            inout("ecx") 0u32 => ecx,
            out("edx") edx,
        );
    }
    CpuidResult { eax, ebx, ecx, edx }
}

fn cpuid_subleaf(leaf: u32, subleaf: u32) -> CpuidResult {
    let (eax, ebx, ecx, edx): (u32, u32, u32, u32);
    unsafe {
        core::arch::asm!(
            "push rbx",
            "cpuid",
            "mov {ebx_out:e}, ebx",
            "pop rbx",
            inout("eax") leaf => eax,
            ebx_out = out(reg) ebx,
            inout("ecx") subleaf => ecx,
            out("edx") edx,
        );
    }
    CpuidResult { eax, ebx, ecx, edx }
}
