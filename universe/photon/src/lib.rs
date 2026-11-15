// ═══════════════════════════════════════════════════════════════════════════════
// PHOTON - Universal Graphics Hook Framework
// ═══════════════════════════════════════════════════════════════════════════════
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ═══════════════════════════════════════════════════════════════════════════════
//
//! # Photon Hook Framework
//!
//! Universal graphics API hooking framework supporting:
//! - DirectX 9, 10, 11, 12
//! - Vulkan
//! - OpenGL
//!
//! ## Features
//!
//! - **API Detection**: Automatically detects which graphics API is in use
//! - **VTable Hooking**: Safe hooking of COM interface methods
//! - **Shader Injection**: Runtime shader modification and injection
//! - **Render Interception**: Intercept draw calls for modification
//!
//! ## Example
//!
//! ```rust,ignore
//! use photon_hook::{PhotonHook, HookConfig};
//!
//! let mut hook = PhotonHook::new(HookConfig::default());
//! hook.detect_api()?;
//! hook.install()?;
//! ```

#![cfg(windows)]
#![allow(non_snake_case)]
#![allow(non_camel_case_types)]

pub mod error;
pub mod hook;
pub mod dx11;
pub mod shader;
pub mod render;
pub mod memory;

pub use error::{PhotonError, PhotonResult};
pub use hook::{PhotonHook, HookConfig, HookStatus, GraphicsAPI};
pub use dx11::{DX11Hook, DX11Context};
pub use shader::{ShaderInjector, ShaderType, CompiledShader};
pub use render::{RenderCallback, DrawCallInfo, RenderState};

use log::{info, warn, error, debug};
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use std::sync::Arc;

// ═══════════════════════════════════════════════════════════════════════════════
// GLOBAL STATE
// ═══════════════════════════════════════════════════════════════════════════════

/// Global Photon hook instance
static PHOTON: Lazy<RwLock<Option<Arc<PhotonHook>>>> = Lazy::new(|| RwLock::new(None));

/// Initialize the Photon hook system
///
/// # Safety
///
/// This function must be called from DllMain or equivalent initialization context.
pub fn init() -> PhotonResult<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();

    info!("Photon Hook Framework v1.0.0 initializing...");

    let config = HookConfig::default();
    let hook = PhotonHook::new(config)?;

    *PHOTON.write() = Some(Arc::new(hook));

    info!("Photon Hook Framework initialized successfully");
    Ok(())
}

/// Get a reference to the global Photon hook
pub fn get_hook() -> Option<Arc<PhotonHook>> {
    PHOTON.read().clone()
}

/// Shutdown the Photon hook system
pub fn shutdown() -> PhotonResult<()> {
    info!("Photon Hook Framework shutting down...");

    if let Some(hook) = PHOTON.write().take() {
        // Hook will be dropped and cleanup performed
        drop(hook);
    }

    info!("Photon Hook Framework shutdown complete");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// DLL ENTRY POINT (Windows)
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(windows)]
mod dll {
    use super::*;
    use windows::Win32::Foundation::{BOOL, HMODULE, TRUE, FALSE};
    use windows::Win32::System::SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH};

    /// DLL entry point
    ///
    /// # Safety
    ///
    /// Called by Windows loader. Must not perform complex initialization here.
    #[no_mangle]
    pub unsafe extern "system" fn DllMain(
        _hmodule: HMODULE,
        reason: u32,
        _reserved: *mut std::ffi::c_void,
    ) -> BOOL {
        match reason {
            DLL_PROCESS_ATTACH => {
                // Spawn initialization on a separate thread to avoid loader lock
                std::thread::spawn(|| {
                    if let Err(e) = init() {
                        error!("Failed to initialize Photon: {:?}", e);
                    }
                });
                TRUE
            }
            DLL_PROCESS_DETACH => {
                if let Err(e) = shutdown() {
                    error!("Failed to shutdown Photon: {:?}", e);
                }
                TRUE
            }
            _ => TRUE,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// EXPORTED C API
// ═══════════════════════════════════════════════════════════════════════════════

/// Initialize Photon hook system (C API)
///
/// # Safety
///
/// Must be called before any other Photon functions.
#[no_mangle]
pub unsafe extern "C" fn photon_init() -> i32 {
    match init() {
        Ok(()) => 0,
        Err(e) => {
            error!("photon_init failed: {:?}", e);
            -1
        }
    }
}

/// Detect graphics API in use (C API)
///
/// Returns: 0=None, 1=DX9, 2=DX10, 3=DX11, 4=DX12, 5=Vulkan, 6=OpenGL
#[no_mangle]
pub unsafe extern "C" fn photon_detect_api() -> i32 {
    if let Some(hook) = get_hook() {
        match hook.detected_api() {
            None => 0,
            Some(GraphicsAPI::DirectX9) => 1,
            Some(GraphicsAPI::DirectX10) => 2,
            Some(GraphicsAPI::DirectX11) => 3,
            Some(GraphicsAPI::DirectX12) => 4,
            Some(GraphicsAPI::Vulkan) => 5,
            Some(GraphicsAPI::OpenGL) => 6,
        }
    } else {
        -1
    }
}

/// Install hooks for detected API (C API)
#[no_mangle]
pub unsafe extern "C" fn photon_install_hooks() -> i32 {
    if let Some(hook) = get_hook() {
        // Note: This would need interior mutability in the real implementation
        0
    } else {
        -1
    }
}

/// Shutdown Photon hook system (C API)
#[no_mangle]
pub unsafe extern "C" fn photon_shutdown() -> i32 {
    match shutdown() {
        Ok(()) => 0,
        Err(e) => {
            error!("photon_shutdown failed: {:?}", e);
            -1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_enum() {
        assert_eq!(GraphicsAPI::DirectX11 as u32, 3);
    }
}
