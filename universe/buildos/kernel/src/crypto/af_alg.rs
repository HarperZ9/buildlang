//! AF_ALG Socket Interface
//!
//! Provides Linux-compatible AF_ALG socket interface for userspace crypto.
//! Supports hash, skcipher, aead, and rng algorithm types.

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::vec;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::{Mutex, RwLock};

use super::{CryptoError, STATS};
use super::hash::HashOps;
use super::cipher::CipherOps;
use super::aead::AeadOps;
use super::rng::{RngOps, RngAlgorithm};
use super::mac::{MacOps, MacAlgorithm};

/// AF_ALG address family number
pub const AF_ALG: u16 = 38;

/// Algorithm socket option level
pub const SOL_ALG: i32 = 279;

/// Socket options
pub const ALG_SET_KEY: u32 = 1;
pub const ALG_SET_IV: u32 = 2;
pub const ALG_SET_OP: u32 = 3;
pub const ALG_SET_AEAD_ASSOCLEN: u32 = 4;
pub const ALG_SET_AEAD_AUTHSIZE: u32 = 5;
pub const ALG_SET_DRBG_ENTROPY: u32 = 6;

/// Operation types
pub const ALG_OP_DECRYPT: u32 = 0;
pub const ALG_OP_ENCRYPT: u32 = 1;

/// Control message types
pub const ALG_SET_IV_CMN: u32 = 1;
pub const ALG_SET_OP_CMN: u32 = 2;
pub const ALG_SET_AEAD_ASSOCLEN_CMN: u32 = 3;

/// Maximum algorithm name length
pub const CRYPTO_MAX_ALG_NAME: usize = 128;

/// Next socket ID
static NEXT_SOCKET_ID: AtomicU64 = AtomicU64::new(1);

/// Socket registry
static SOCKETS: RwLock<BTreeMap<u64, Arc<Mutex<AlgSocket>>>> = RwLock::new(BTreeMap::new());

/// Algorithm type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlgType {
    /// Hash algorithm
    Hash,
    /// Symmetric cipher
    Skcipher,
    /// AEAD cipher
    Aead,
    /// Random number generator
    Rng,
    /// Message authentication code
    Mac,
}

impl AlgType {
    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "hash" => Some(Self::Hash),
            "skcipher" => Some(Self::Skcipher),
            "aead" => Some(Self::Aead),
            "rng" => Some(Self::Rng),
            "mac" => Some(Self::Mac),
            _ => None,
        }
    }
}

/// AF_ALG socket address
#[repr(C)]
#[derive(Clone, Debug)]
pub struct SockaddrAlg {
    /// Address family (AF_ALG)
    pub family: u16,
    /// Algorithm type
    pub type_name: [u8; 14],
    /// Feature mask
    pub feat: u32,
    /// Required feature mask
    pub mask: u32,
    /// Algorithm name
    pub name: [u8; CRYPTO_MAX_ALG_NAME],
}

impl SockaddrAlg {
    /// Create new sockaddr_alg
    pub fn new(alg_type: &str, alg_name: &str) -> Self {
        let mut addr = Self {
            family: AF_ALG,
            type_name: [0; 14],
            feat: 0,
            mask: 0,
            name: [0; CRYPTO_MAX_ALG_NAME],
        };

        let type_bytes = alg_type.as_bytes();
        let len = core::cmp::min(type_bytes.len(), 13);
        addr.type_name[..len].copy_from_slice(&type_bytes[..len]);

        let name_bytes = alg_name.as_bytes();
        let len = core::cmp::min(name_bytes.len(), CRYPTO_MAX_ALG_NAME - 1);
        addr.name[..len].copy_from_slice(&name_bytes[..len]);

        addr
    }

    /// Get algorithm type as string
    pub fn get_type(&self) -> Option<String> {
        let end = self.type_name.iter().position(|&c| c == 0).unwrap_or(14);
        core::str::from_utf8(&self.type_name[..end])
            .ok()
            .map(String::from)
    }

    /// Get algorithm name as string
    pub fn get_name(&self) -> Option<String> {
        let end = self.name.iter().position(|&c| c == 0).unwrap_or(CRYPTO_MAX_ALG_NAME);
        core::str::from_utf8(&self.name[..end])
            .ok()
            .map(String::from)
    }
}

/// Algorithm instance state
pub enum AlgInstance {
    /// Hash instance
    Hash(Box<dyn HashOps>),
    /// Cipher instance
    Skcipher {
        cipher: Box<dyn CipherOps>,
        iv: Vec<u8>,
        operation: u32,
    },
    /// AEAD instance
    Aead {
        aead: Box<dyn AeadOps>,
        iv: Vec<u8>,
        aad_len: usize,
        tag_size: usize,
        operation: u32,
    },
    /// RNG instance
    Rng(Box<dyn RngOps>),
    /// MAC instance
    Mac(Box<dyn MacOps>),
}

/// AF_ALG socket
pub struct AlgSocket {
    /// Socket ID
    id: u64,
    /// Algorithm type
    alg_type: AlgType,
    /// Algorithm name
    alg_name: String,
    /// Key (if set)
    key: Vec<u8>,
    /// Algorithm instance (for child sockets)
    instance: Option<AlgInstance>,
    /// Parent socket ID (for child sockets)
    parent: Option<u64>,
    /// Is bound
    bound: bool,
    /// Accept queue
    accept_queue: Vec<u64>,
}

impl AlgSocket {
    /// Create new algorithm socket
    pub fn new() -> Self {
        let id = NEXT_SOCKET_ID.fetch_add(1, Ordering::SeqCst);

        Self {
            id,
            alg_type: AlgType::Hash,
            alg_name: String::new(),
            key: Vec::new(),
            instance: None,
            parent: None,
            bound: false,
            accept_queue: Vec::new(),
        }
    }

    /// Get socket ID
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Bind to algorithm
    pub fn bind(&mut self, addr: &SockaddrAlg) -> Result<(), CryptoError> {
        if self.bound {
            return Err(CryptoError::SocketError);
        }

        let type_str = addr.get_type().ok_or(CryptoError::InvalidParameter)?;
        let name_str = addr.get_name().ok_or(CryptoError::InvalidParameter)?;

        self.alg_type = AlgType::from_str(&type_str).ok_or(CryptoError::AlgorithmNotFound)?;
        self.alg_name = name_str;
        self.bound = true;

        Ok(())
    }

    /// Set socket option
    pub fn setsockopt(&mut self, optname: u32, optval: &[u8]) -> Result<(), CryptoError> {
        match optname {
            ALG_SET_KEY => {
                self.key = optval.to_vec();
                Ok(())
            }
            ALG_SET_AEAD_AUTHSIZE => {
                // Set authentication tag size for AEAD
                if optval.len() >= 4 {
                    let size = u32::from_ne_bytes([optval[0], optval[1], optval[2], optval[3]]);
                    if let Some(AlgInstance::Aead { ref mut tag_size, .. }) = self.instance {
                        *tag_size = size as usize;
                    }
                }
                Ok(())
            }
            _ => Err(CryptoError::InvalidParameter),
        }
    }

    /// Accept new operation socket
    pub fn accept(&mut self) -> Result<u64, CryptoError> {
        if !self.bound {
            return Err(CryptoError::SocketError);
        }

        // Create child socket with algorithm instance
        let mut child = AlgSocket::new();
        child.alg_type = self.alg_type;
        child.alg_name = self.alg_name.clone();
        child.parent = Some(self.id);

        // Create algorithm instance
        child.instance = Some(self.create_instance()?);

        let child_id = child.id;

        // Register child socket
        SOCKETS.write().insert(child_id, Arc::new(Mutex::new(child)));

        Ok(child_id)
    }

    /// Create algorithm instance
    fn create_instance(&self) -> Result<AlgInstance, CryptoError> {
        match self.alg_type {
            AlgType::Hash => {
                let hash = super::hash::allocate(&self.alg_name)?;
                Ok(AlgInstance::Hash(hash))
            }
            AlgType::Skcipher => {
                let cipher = super::cipher::allocate(&self.alg_name, &self.key)?;
                Ok(AlgInstance::Skcipher {
                    cipher,
                    iv: Vec::new(),
                    operation: ALG_OP_ENCRYPT,
                })
            }
            AlgType::Aead => {
                let aead = super::aead::allocate(&self.alg_name, &self.key)?;
                let tag_size = aead.tag_size();
                Ok(AlgInstance::Aead {
                    aead,
                    iv: Vec::new(),
                    aad_len: 0,
                    tag_size,
                    operation: ALG_OP_ENCRYPT,
                })
            }
            AlgType::Rng => {
                let rng = super::rng::create_rng(RngAlgorithm::ChaCha20)?;
                Ok(AlgInstance::Rng(rng))
            }
            AlgType::Mac => {
                let mac_alg = match self.alg_name.as_str() {
                    "hmac(sha256)" => MacAlgorithm::HmacSha256,
                    "hmac(sha512)" => MacAlgorithm::HmacSha512,
                    "poly1305" => MacAlgorithm::Poly1305,
                    _ => return Err(CryptoError::AlgorithmNotFound),
                };
                let _mac = super::mac::Mac::new(mac_alg, &self.key)?;
                // Need to get inner as MacOps - for now create wrapper
                Ok(AlgInstance::Mac(create_mac_ops(&self.alg_name, &self.key)?))
            }
        }
    }

    /// Set control message (IV, operation, etc.)
    pub fn set_cmsg(&mut self, cmsg_type: u32, data: &[u8]) -> Result<(), CryptoError> {
        match cmsg_type {
            ALG_SET_IV_CMN => {
                if let Some(ref mut instance) = self.instance {
                    match instance {
                        AlgInstance::Skcipher { ref mut iv, .. } => {
                            *iv = data.to_vec();
                        }
                        AlgInstance::Aead { ref mut iv, .. } => {
                            *iv = data.to_vec();
                        }
                        _ => {}
                    }
                }
                Ok(())
            }
            ALG_SET_OP_CMN => {
                if data.len() >= 4 {
                    let op = u32::from_ne_bytes([data[0], data[1], data[2], data[3]]);
                    if let Some(ref mut instance) = self.instance {
                        match instance {
                            AlgInstance::Skcipher { ref mut operation, .. } => {
                                *operation = op;
                            }
                            AlgInstance::Aead { ref mut operation, .. } => {
                                *operation = op;
                            }
                            _ => {}
                        }
                    }
                }
                Ok(())
            }
            ALG_SET_AEAD_ASSOCLEN_CMN => {
                if data.len() >= 4 {
                    let len = u32::from_ne_bytes([data[0], data[1], data[2], data[3]]);
                    if let Some(AlgInstance::Aead { ref mut aad_len, .. }) = self.instance {
                        *aad_len = len as usize;
                    }
                }
                Ok(())
            }
            _ => Err(CryptoError::InvalidParameter),
        }
    }

    /// Write data to socket
    pub fn write(&mut self, data: &[u8]) -> Result<usize, CryptoError> {
        if let Some(ref mut instance) = self.instance {
            match instance {
                AlgInstance::Hash(ref mut hash) => {
                    hash.update(data);
                    Ok(data.len())
                }
                AlgInstance::Mac(ref mut mac) => {
                    mac.update(data);
                    Ok(data.len())
                }
                _ => Err(CryptoError::InvalidOperation),
            }
        } else {
            Err(CryptoError::SocketError)
        }
    }

    /// Read from socket
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, CryptoError> {
        if let Some(ref mut instance) = self.instance {
            match instance {
                AlgInstance::Hash(ref mut hash) => {
                    let digest = hash.finalize();
                    let len = core::cmp::min(buf.len(), digest.len());
                    buf[..len].copy_from_slice(&digest[..len]);
                    hash.reset();
                    Ok(len)
                }
                AlgInstance::Mac(ref mut mac) => {
                    let tag = mac.finalize();
                    let len = core::cmp::min(buf.len(), tag.len());
                    buf[..len].copy_from_slice(&tag[..len]);
                    mac.reset();
                    Ok(len)
                }
                AlgInstance::Rng(ref mut rng) => {
                    rng.fill_bytes(buf)?;
                    Ok(buf.len())
                }
                _ => Err(CryptoError::InvalidOperation),
            }
        } else {
            Err(CryptoError::SocketError)
        }
    }

    /// Send message with encryption/decryption
    pub fn sendmsg(&mut self, data: &[u8], output: &mut [u8]) -> Result<usize, CryptoError> {
        if let Some(ref mut instance) = self.instance {
            match instance {
                AlgInstance::Skcipher { ref cipher, ref iv, ref operation } => {
                    let mut out_data = data.to_vec();

                    if *operation == ALG_OP_ENCRYPT {
                        cipher.encrypt(iv, &mut out_data)?;
                    } else {
                        cipher.decrypt(iv, &mut out_data)?;
                    }

                    let len = core::cmp::min(output.len(), out_data.len());
                    output[..len].copy_from_slice(&out_data[..len]);

                    STATS.encryptions.fetch_add(1, Ordering::Relaxed);
                    Ok(len)
                }
                AlgInstance::Aead { ref aead, ref iv, ref aad_len, ref operation, .. } => {
                    let aad = if *aad_len > 0 && *aad_len <= data.len() {
                        &data[..*aad_len]
                    } else {
                        &[]
                    };
                    let payload = if *aad_len <= data.len() {
                        &data[*aad_len..]
                    } else {
                        data
                    };

                    let result = if *operation == ALG_OP_ENCRYPT {
                        aead.encrypt(iv, aad, payload)?
                    } else {
                        aead.decrypt(iv, aad, payload)?
                    };

                    let len = core::cmp::min(output.len(), result.len());
                    output[..len].copy_from_slice(&result[..len]);

                    Ok(len)
                }
                _ => Err(CryptoError::InvalidOperation),
            }
        } else {
            Err(CryptoError::SocketError)
        }
    }
}

/// Create MAC ops wrapper
fn create_mac_ops(name: &str, key: &[u8]) -> Result<Box<dyn MacOps>, CryptoError> {
    // This would ideally use the existing Mac type but we need a trait object
    match name {
        "hmac(sha256)" => {
            let mac = super::mac::Mac::new(MacAlgorithm::HmacSha256, key)?;
            Ok(Box::new(MacWrapper { inner: Arc::new(Mutex::new(mac)) }))
        }
        "hmac(sha512)" => {
            let mac = super::mac::Mac::new(MacAlgorithm::HmacSha512, key)?;
            Ok(Box::new(MacWrapper { inner: Arc::new(Mutex::new(mac)) }))
        }
        _ => Err(CryptoError::AlgorithmNotFound),
    }
}

/// MAC wrapper for trait object
struct MacWrapper {
    inner: Arc<Mutex<super::mac::Mac>>,
}

impl MacOps for MacWrapper {
    fn name(&self) -> &str {
        "mac-wrapper"
    }

    fn output_size(&self) -> usize {
        32 // Default to SHA256 size
    }

    fn update(&mut self, data: &[u8]) {
        self.inner.lock().update(data);
    }

    fn finalize(&mut self) -> Vec<u8> {
        self.inner.lock().finalize()
    }

    fn reset(&mut self) {
        // MAC doesn't have public reset
    }

    fn set_key(&mut self, _key: &[u8]) -> Result<(), CryptoError> {
        // Would need to recreate
        Ok(())
    }
}

// =============================================================================
// System Call Interface
// =============================================================================

/// Create AF_ALG socket
pub fn socket() -> Result<u64, CryptoError> {
    let socket = AlgSocket::new();
    let id = socket.id;

    SOCKETS.write().insert(id, Arc::new(Mutex::new(socket)));

    Ok(id)
}

/// Bind socket to algorithm
pub fn bind(socket_id: u64, addr: &SockaddrAlg) -> Result<(), CryptoError> {
    let sockets = SOCKETS.read();
    let socket = sockets.get(&socket_id).ok_or(CryptoError::SocketError)?;
    let result = socket.lock().bind(addr);
    drop(sockets);
    result
}

/// Set socket option
pub fn setsockopt(socket_id: u64, optname: u32, optval: &[u8]) -> Result<(), CryptoError> {
    let sockets = SOCKETS.read();
    let socket = sockets.get(&socket_id).ok_or(CryptoError::SocketError)?;
    let result = socket.lock().setsockopt(optname, optval);
    drop(sockets);
    result
}

/// Accept new operation socket
pub fn accept(socket_id: u64) -> Result<u64, CryptoError> {
    let sockets = SOCKETS.read();
    let socket = sockets.get(&socket_id).ok_or(CryptoError::SocketError)?;
    let result = socket.lock().accept();
    drop(sockets);
    result
}

/// Write to socket
pub fn write(socket_id: u64, data: &[u8]) -> Result<usize, CryptoError> {
    let sockets = SOCKETS.read();
    let socket = sockets.get(&socket_id).ok_or(CryptoError::SocketError)?;
    let result = socket.lock().write(data);
    drop(sockets);
    result
}

/// Read from socket
pub fn read(socket_id: u64, buf: &mut [u8]) -> Result<usize, CryptoError> {
    let sockets = SOCKETS.read();
    let socket = sockets.get(&socket_id).ok_or(CryptoError::SocketError)?;
    let result = socket.lock().read(buf);
    drop(sockets);
    result
}

/// Send message
pub fn sendmsg(socket_id: u64, cmsg: &[(u32, Vec<u8>)], data: &[u8], output: &mut [u8]) -> Result<usize, CryptoError> {
    let sockets = SOCKETS.read();
    let socket = sockets.get(&socket_id).ok_or(CryptoError::SocketError)?;

    let mut sock = socket.lock();

    // Apply control messages
    for (cmsg_type, cmsg_data) in cmsg {
        sock.set_cmsg(*cmsg_type, cmsg_data)?;
    }

    sock.sendmsg(data, output)
}

/// Close socket
pub fn close(socket_id: u64) -> Result<(), CryptoError> {
    SOCKETS.write().remove(&socket_id);
    Ok(())
}

// =============================================================================
// Algorithm Templates
// =============================================================================

/// Algorithm template for CBC mode
pub struct CbcTemplate;

impl CbcTemplate {
    /// Wrap cipher name
    pub fn wrap_name(cipher: &str) -> String {
        alloc::format!("cbc({})", cipher)
    }
}

/// Algorithm template for CTR mode
pub struct CtrTemplate;

impl CtrTemplate {
    /// Wrap cipher name
    pub fn wrap_name(cipher: &str) -> String {
        alloc::format!("ctr({})", cipher)
    }
}

/// Algorithm template for GCM mode
pub struct GcmTemplate;

impl GcmTemplate {
    /// Wrap cipher name
    pub fn wrap_name(cipher: &str) -> String {
        alloc::format!("gcm({})", cipher)
    }
}

/// Algorithm template for HMAC
pub struct HmacTemplate;

impl HmacTemplate {
    /// Wrap hash name
    pub fn wrap_name(hash: &str) -> String {
        alloc::format!("hmac({})", hash)
    }
}

// =============================================================================
// Algorithm Registration
// =============================================================================

/// Registered algorithm info
#[derive(Clone, Debug)]
pub struct AlgInfo {
    /// Algorithm type
    pub alg_type: AlgType,
    /// Algorithm name
    pub name: String,
    /// Driver name
    pub driver_name: String,
    /// Priority
    pub priority: u32,
    /// Block size
    pub block_size: usize,
    /// Key size (min)
    pub min_key_size: usize,
    /// Key size (max)
    pub max_key_size: usize,
    /// IV size
    pub iv_size: usize,
    /// Digest size (for hash/mac)
    pub digest_size: usize,
}

/// Algorithm registry
static ALGORITHMS: RwLock<Vec<AlgInfo>> = RwLock::new(Vec::new());

/// Register algorithm
pub fn register_algorithm(info: AlgInfo) {
    ALGORITHMS.write().push(info);
}

/// Get algorithm info
pub fn get_algorithm_info(alg_type: AlgType, name: &str) -> Option<AlgInfo> {
    ALGORITHMS.read().iter()
        .find(|a| a.alg_type == alg_type && a.name == name)
        .cloned()
}

/// List all algorithms
pub fn list_algorithms() -> Vec<AlgInfo> {
    ALGORITHMS.read().clone()
}

/// Initialize AF_ALG subsystem with standard algorithms
pub fn init() {
    // Register hash algorithms
    register_algorithm(AlgInfo {
        alg_type: AlgType::Hash,
        name: String::from("sha256"),
        driver_name: String::from("sha256-generic"),
        priority: 100,
        block_size: 64,
        min_key_size: 0,
        max_key_size: 0,
        iv_size: 0,
        digest_size: 32,
    });

    register_algorithm(AlgInfo {
        alg_type: AlgType::Hash,
        name: String::from("sha512"),
        driver_name: String::from("sha512-generic"),
        priority: 100,
        block_size: 128,
        min_key_size: 0,
        max_key_size: 0,
        iv_size: 0,
        digest_size: 64,
    });

    register_algorithm(AlgInfo {
        alg_type: AlgType::Hash,
        name: String::from("blake2b-256"),
        driver_name: String::from("blake2b-generic"),
        priority: 100,
        block_size: 128,
        min_key_size: 0,
        max_key_size: 64,
        iv_size: 0,
        digest_size: 32,
    });

    // Register cipher algorithms
    register_algorithm(AlgInfo {
        alg_type: AlgType::Skcipher,
        name: String::from("aes-cbc"),
        driver_name: String::from("aes-cbc-generic"),
        priority: 100,
        block_size: 16,
        min_key_size: 16,
        max_key_size: 32,
        iv_size: 16,
        digest_size: 0,
    });

    register_algorithm(AlgInfo {
        alg_type: AlgType::Skcipher,
        name: String::from("aes-ctr"),
        driver_name: String::from("aes-ctr-generic"),
        priority: 100,
        block_size: 1,
        min_key_size: 16,
        max_key_size: 32,
        iv_size: 16,
        digest_size: 0,
    });

    register_algorithm(AlgInfo {
        alg_type: AlgType::Skcipher,
        name: String::from("chacha20"),
        driver_name: String::from("chacha20-generic"),
        priority: 100,
        block_size: 1,
        min_key_size: 32,
        max_key_size: 32,
        iv_size: 16,
        digest_size: 0,
    });

    // Register AEAD algorithms
    register_algorithm(AlgInfo {
        alg_type: AlgType::Aead,
        name: String::from("aes-128-gcm"),
        driver_name: String::from("aes-gcm-generic"),
        priority: 100,
        block_size: 1,
        min_key_size: 16,
        max_key_size: 16,
        iv_size: 12,
        digest_size: 16,
    });

    register_algorithm(AlgInfo {
        alg_type: AlgType::Aead,
        name: String::from("aes-256-gcm"),
        driver_name: String::from("aes-gcm-generic"),
        priority: 100,
        block_size: 1,
        min_key_size: 32,
        max_key_size: 32,
        iv_size: 12,
        digest_size: 16,
    });

    register_algorithm(AlgInfo {
        alg_type: AlgType::Aead,
        name: String::from("chacha20-poly1305"),
        driver_name: String::from("chacha20poly1305-generic"),
        priority: 100,
        block_size: 1,
        min_key_size: 32,
        max_key_size: 32,
        iv_size: 12,
        digest_size: 16,
    });

    // Register MAC algorithms
    register_algorithm(AlgInfo {
        alg_type: AlgType::Mac,
        name: String::from("hmac(sha256)"),
        driver_name: String::from("hmac-sha256-generic"),
        priority: 100,
        block_size: 64,
        min_key_size: 0,
        max_key_size: 64,
        iv_size: 0,
        digest_size: 32,
    });

    register_algorithm(AlgInfo {
        alg_type: AlgType::Mac,
        name: String::from("hmac(sha512)"),
        driver_name: String::from("hmac-sha512-generic"),
        priority: 100,
        block_size: 128,
        min_key_size: 0,
        max_key_size: 128,
        iv_size: 0,
        digest_size: 64,
    });

    // Register RNG algorithms
    register_algorithm(AlgInfo {
        alg_type: AlgType::Rng,
        name: String::from("drbg_nopr_ctr_aes256"),
        driver_name: String::from("drbg-generic"),
        priority: 100,
        block_size: 0,
        min_key_size: 0,
        max_key_size: 0,
        iv_size: 0,
        digest_size: 0,
    });
}

// =============================================================================
// Userspace Helper Macros
// =============================================================================

/// Helper to compute hash of data
pub fn hash_data(alg_name: &str, data: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let sock = socket()?;

    let addr = SockaddrAlg::new("hash", alg_name);
    bind(sock, &addr)?;

    let op_sock = accept(sock)?;
    write(op_sock, data)?;

    let mut digest = vec![0u8; 64]; // Max digest size
    let len = read(op_sock, &mut digest)?;
    digest.truncate(len);

    close(op_sock)?;
    close(sock)?;

    Ok(digest)
}

/// Helper to encrypt data
pub fn encrypt_data(
    alg_name: &str,
    key: &[u8],
    iv: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let sock = socket()?;

    let addr = SockaddrAlg::new("skcipher", alg_name);
    bind(sock, &addr)?;
    setsockopt(sock, ALG_SET_KEY, key)?;

    let op_sock = accept(sock)?;

    let cmsg = vec![
        (ALG_SET_IV_CMN, iv.to_vec()),
        (ALG_SET_OP_CMN, ALG_OP_ENCRYPT.to_ne_bytes().to_vec()),
    ];

    let mut ciphertext = vec![0u8; plaintext.len() + 16];
    let len = sendmsg(op_sock, &cmsg, plaintext, &mut ciphertext)?;
    ciphertext.truncate(len);

    close(op_sock)?;
    close(sock)?;

    Ok(ciphertext)
}

/// Helper to decrypt data
pub fn decrypt_data(
    alg_name: &str,
    key: &[u8],
    iv: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let sock = socket()?;

    let addr = SockaddrAlg::new("skcipher", alg_name);
    bind(sock, &addr)?;
    setsockopt(sock, ALG_SET_KEY, key)?;

    let op_sock = accept(sock)?;

    let cmsg = vec![
        (ALG_SET_IV_CMN, iv.to_vec()),
        (ALG_SET_OP_CMN, ALG_OP_DECRYPT.to_ne_bytes().to_vec()),
    ];

    let mut plaintext = vec![0u8; ciphertext.len()];
    let len = sendmsg(op_sock, &cmsg, ciphertext, &mut plaintext)?;
    plaintext.truncate(len);

    close(op_sock)?;
    close(sock)?;

    Ok(plaintext)
}

/// Helper to generate random bytes
pub fn random_data(len: usize) -> Result<Vec<u8>, CryptoError> {
    let sock = socket()?;

    let addr = SockaddrAlg::new("rng", "drbg_nopr_ctr_aes256");
    bind(sock, &addr)?;

    let op_sock = accept(sock)?;

    let mut data = vec![0u8; len];
    read(op_sock, &mut data)?;

    close(op_sock)?;
    close(sock)?;

    Ok(data)
}
