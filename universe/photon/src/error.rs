// ═══════════════════════════════════════════════════════════════════════════════
// PHOTON - Error Types
// ═══════════════════════════════════════════════════════════════════════════════
// Copyright © 2024-2025 Zain Dana Harper. All Rights Reserved.
// ═══════════════════════════════════════════════════════════════════════════════

//! Error types for the Photon hook framework.

use thiserror::Error;

/// Result type for Photon operations
pub type PhotonResult<T> = Result<T, PhotonError>;

/// Errors that can occur in the Photon hook framework
#[derive(Debug, Error)]
pub enum PhotonError {
    /// No graphics API was detected
    #[error("No graphics API detected in process")]
    NoApiDetected,

    /// The specified API is not supported
    #[error("Graphics API not supported: {api}")]
    UnsupportedApi { api: String },

    /// Failed to locate a required module/DLL
    #[error("Module not found: {name}")]
    ModuleNotFound { name: String },

    /// Failed to locate a function in a module
    #[error("Function not found: {func} in {module}")]
    FunctionNotFound { module: String, func: String },

    /// Hook installation failed
    #[error("Failed to install hook at {address:#x}: {reason}")]
    HookInstallFailed { address: u64, reason: String },

    /// Hook is already installed
    #[error("Hook already installed: {name}")]
    HookAlreadyInstalled { name: String },

    /// Hook removal failed
    #[error("Failed to remove hook: {name}")]
    HookRemoveFailed { name: String },

    /// VTable not found
    #[error("VTable not found for interface: {interface}")]
    VTableNotFound { interface: String },

    /// Invalid VTable index
    #[error("Invalid VTable index {index} for {interface}")]
    InvalidVTableIndex { interface: String, index: usize },

    /// Memory protection change failed
    #[error("Failed to change memory protection at {address:#x}")]
    MemoryProtectFailed { address: u64 },

    /// Failed to create dummy device for VTable retrieval
    #[error("Failed to create dummy device: {reason}")]
    DummyDeviceCreationFailed { reason: String },

    /// Shader compilation failed
    #[error("Shader compilation failed: {error}")]
    ShaderCompilationFailed { error: String },

    /// Shader injection failed
    #[error("Shader injection failed: {reason}")]
    ShaderInjectionFailed { reason: String },

    /// Invalid shader bytecode
    #[error("Invalid shader bytecode")]
    InvalidShaderBytecode,

    /// Resource not found
    #[error("Resource not found: {name}")]
    ResourceNotFound { name: String },

    /// Render state error
    #[error("Render state error: {reason}")]
    RenderStateError { reason: String },

    /// Thread synchronization error
    #[error("Synchronization error: {reason}")]
    SyncError { reason: String },

    /// Windows API error
    #[error("Windows API error: {0}")]
    WindowsError(#[from] windows::core::Error),

    /// Generic I/O error
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Null pointer encountered
    #[error("Null pointer: {context}")]
    NullPointer { context: String },

    /// Already initialized
    #[error("Photon already initialized")]
    AlreadyInitialized,

    /// Not initialized
    #[error("Photon not initialized")]
    NotInitialized,

    /// Internal error
    #[error("Internal error: {reason}")]
    Internal { reason: String },
}

impl PhotonError {
    /// Create a hook installation error
    pub fn hook_install(address: u64, reason: impl Into<String>) -> Self {
        PhotonError::HookInstallFailed {
            address,
            reason: reason.into(),
        }
    }

    /// Create a module not found error
    pub fn module_not_found(name: impl Into<String>) -> Self {
        PhotonError::ModuleNotFound { name: name.into() }
    }

    /// Create a function not found error
    pub fn function_not_found(module: impl Into<String>, func: impl Into<String>) -> Self {
        PhotonError::FunctionNotFound {
            module: module.into(),
            func: func.into(),
        }
    }

    /// Create an internal error
    pub fn internal(reason: impl Into<String>) -> Self {
        PhotonError::Internal {
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = PhotonError::NoApiDetected;
        assert_eq!(err.to_string(), "No graphics API detected in process");

        let err = PhotonError::hook_install(0x12345678, "permission denied");
        assert!(err.to_string().contains("0x12345678"));
    }

    #[test]
    fn test_error_creation() {
        let err = PhotonError::module_not_found("d3d11.dll");
        match err {
            PhotonError::ModuleNotFound { name } => assert_eq!(name, "d3d11.dll"),
            _ => panic!("Wrong error type"),
        }
    }
}
