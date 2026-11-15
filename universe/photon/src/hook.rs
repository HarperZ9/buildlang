// ═══════════════════════════════════════════════════════════════════════════════
// PHOTON - Universal Hook System
// ═══════════════════════════════════════════════════════════════════════════════
// Copyright © 2024-2025 Zain Dana Harper. All Rights Reserved.
// ═══════════════════════════════════════════════════════════════════════════════

//! Universal hook system supporting multiple graphics APIs.

use crate::error::{PhotonError, PhotonResult};
use crate::dx11::DX11Hook;

use log::{info, warn, debug, error};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::GetModuleHandleA;
use windows::core::PCSTR;

// ═══════════════════════════════════════════════════════════════════════════════
// TYPES
// ═══════════════════════════════════════════════════════════════════════════════

/// Supported graphics APIs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum GraphicsAPI {
    DirectX9 = 1,
    DirectX10 = 2,
    DirectX11 = 3,
    DirectX12 = 4,
    Vulkan = 5,
    OpenGL = 6,
}

impl GraphicsAPI {
    /// Get the module name for this API
    pub fn module_name(&self) -> &'static str {
        match self {
            GraphicsAPI::DirectX9 => "d3d9.dll",
            GraphicsAPI::DirectX10 => "d3d10.dll",
            GraphicsAPI::DirectX11 => "d3d11.dll",
            GraphicsAPI::DirectX12 => "d3d12.dll",
            GraphicsAPI::Vulkan => "vulkan-1.dll",
            GraphicsAPI::OpenGL => "opengl32.dll",
        }
    }

    /// Get all known APIs
    pub fn all() -> &'static [GraphicsAPI] {
        &[
            GraphicsAPI::DirectX12,
            GraphicsAPI::DirectX11,
            GraphicsAPI::DirectX10,
            GraphicsAPI::DirectX9,
            GraphicsAPI::Vulkan,
            GraphicsAPI::OpenGL,
        ]
    }
}

/// Status of a hook point
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookStatus {
    /// Hook is not installed
    NotInstalled,
    /// Hook installation is pending
    Pending,
    /// Hook is installed and active
    Installed,
    /// Hook installation failed
    Failed,
    /// Hook is temporarily disabled
    Disabled,
}

/// Information about a hook point
#[derive(Debug, Clone)]
pub struct HookInfo {
    /// Name of the hooked function
    pub name: String,
    /// Address of the original function
    pub original_address: u64,
    /// Address of the hook trampoline
    pub trampoline_address: u64,
    /// Current status
    pub status: HookStatus,
    /// Number of times this hook has been called
    pub call_count: u64,
}

/// Configuration for the Photon hook system
#[derive(Debug, Clone)]
pub struct HookConfig {
    /// Enable DX9 hooks
    pub hook_dx9: bool,
    /// Enable DX10 hooks
    pub hook_dx10: bool,
    /// Enable DX11 hooks
    pub hook_dx11: bool,
    /// Enable DX12 hooks
    pub hook_dx12: bool,
    /// Enable Vulkan hooks
    pub hook_vulkan: bool,
    /// Enable OpenGL hooks
    pub hook_opengl: bool,
    /// Hook Present/SwapBuffers
    pub hook_present: bool,
    /// Hook Draw calls
    pub hook_draw: bool,
    /// Hook shader creation
    pub hook_shaders: bool,
    /// Hook resource creation
    pub hook_resources: bool,
    /// Enable debug overlay
    pub debug_overlay: bool,
    /// Log all hooked calls
    pub log_calls: bool,
}

impl Default for HookConfig {
    fn default() -> Self {
        Self {
            hook_dx9: true,
            hook_dx10: true,
            hook_dx11: true,
            hook_dx12: true,
            hook_vulkan: true,
            hook_opengl: true,
            hook_present: true,
            hook_draw: true,
            hook_shaders: true,
            hook_resources: false,
            debug_overlay: false,
            log_calls: false,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// PHOTON HOOK
// ═══════════════════════════════════════════════════════════════════════════════

/// Main Photon hook manager
pub struct PhotonHook {
    /// Configuration
    config: HookConfig,
    /// Detected graphics API
    detected_api: RwLock<Option<GraphicsAPI>>,
    /// DX11 hook instance
    dx11_hook: RwLock<Option<DX11Hook>>,
    /// All installed hooks
    hooks: RwLock<HashMap<String, HookInfo>>,
    /// Is the hook system active
    active: RwLock<bool>,
}

impl PhotonHook {
    /// Create a new Photon hook instance
    pub fn new(config: HookConfig) -> PhotonResult<Self> {
        info!("Creating Photon hook with config: {:?}", config);

        let hook = Self {
            config,
            detected_api: RwLock::new(None),
            dx11_hook: RwLock::new(None),
            hooks: RwLock::new(HashMap::new()),
            active: RwLock::new(false),
        };

        Ok(hook)
    }

    /// Get the detected graphics API
    pub fn detected_api(&self) -> Option<GraphicsAPI> {
        *self.detected_api.read()
    }

    /// Check if a module is loaded in the current process
    fn is_module_loaded(name: &str) -> bool {
        unsafe {
            let cstr = std::ffi::CString::new(name).unwrap();
            let result = GetModuleHandleA(PCSTR::from_raw(cstr.as_ptr() as *const u8));
            result.is_ok() && result.unwrap() != HMODULE::default()
        }
    }

    /// Detect which graphics API is in use
    pub fn detect_api(&self) -> PhotonResult<GraphicsAPI> {
        info!("Detecting graphics API...");

        // Check APIs in order of preference (newest first)
        for api in GraphicsAPI::all() {
            if Self::is_module_loaded(api.module_name()) {
                info!("Detected graphics API: {:?}", api);
                *self.detected_api.write() = Some(*api);
                return Ok(*api);
            }
        }

        warn!("No graphics API detected");
        Err(PhotonError::NoApiDetected)
    }

    /// Install hooks for the detected API
    pub fn install(&self) -> PhotonResult<()> {
        let api = self.detected_api()
            .ok_or(PhotonError::NoApiDetected)?;

        info!("Installing hooks for {:?}...", api);

        match api {
            GraphicsAPI::DirectX11 => self.install_dx11_hooks()?,
            GraphicsAPI::DirectX12 => self.install_dx12_hooks()?,
            GraphicsAPI::DirectX10 => self.install_dx10_hooks()?,
            GraphicsAPI::DirectX9 => self.install_dx9_hooks()?,
            GraphicsAPI::Vulkan => self.install_vulkan_hooks()?,
            GraphicsAPI::OpenGL => self.install_opengl_hooks()?,
        }

        *self.active.write() = true;
        info!("Hooks installed successfully");

        Ok(())
    }

    /// Install DX11 hooks
    fn install_dx11_hooks(&self) -> PhotonResult<()> {
        if !self.config.hook_dx11 {
            info!("DX11 hooks disabled in config");
            return Ok(());
        }

        info!("Installing DX11 hooks...");

        let mut dx11_hook = DX11Hook::new()?;
        dx11_hook.initialize()?;

        if self.config.hook_present {
            dx11_hook.hook_present()?;
        }

        if self.config.hook_draw {
            dx11_hook.hook_draw_calls()?;
        }

        if self.config.hook_shaders {
            dx11_hook.hook_shader_creation()?;
        }

        // Store hook info
        for (name, info) in dx11_hook.get_hook_info() {
            self.hooks.write().insert(name, info);
        }

        *self.dx11_hook.write() = Some(dx11_hook);

        info!("DX11 hooks installed");
        Ok(())
    }

    /// Install DX12 hooks (stub)
    fn install_dx12_hooks(&self) -> PhotonResult<()> {
        if !self.config.hook_dx12 {
            return Ok(());
        }
        warn!("DX12 hooks not yet implemented");
        Ok(())
    }

    /// Install DX10 hooks (stub)
    fn install_dx10_hooks(&self) -> PhotonResult<()> {
        if !self.config.hook_dx10 {
            return Ok(());
        }
        warn!("DX10 hooks not yet implemented");
        Ok(())
    }

    /// Install DX9 hooks (stub)
    fn install_dx9_hooks(&self) -> PhotonResult<()> {
        if !self.config.hook_dx9 {
            return Ok(());
        }
        warn!("DX9 hooks not yet implemented");
        Ok(())
    }

    /// Install Vulkan hooks (stub)
    fn install_vulkan_hooks(&self) -> PhotonResult<()> {
        if !self.config.hook_vulkan {
            return Ok(());
        }
        warn!("Vulkan hooks not yet implemented");
        Ok(())
    }

    /// Install OpenGL hooks (stub)
    fn install_opengl_hooks(&self) -> PhotonResult<()> {
        if !self.config.hook_opengl {
            return Ok(());
        }
        warn!("OpenGL hooks not yet implemented");
        Ok(())
    }

    /// Uninstall all hooks
    pub fn uninstall(&self) -> PhotonResult<()> {
        info!("Uninstalling all hooks...");

        // Uninstall DX11 hooks
        if let Some(mut dx11_hook) = self.dx11_hook.write().take() {
            dx11_hook.uninstall()?;
        }

        self.hooks.write().clear();
        *self.active.write() = false;

        info!("All hooks uninstalled");
        Ok(())
    }

    /// Check if hooks are active
    pub fn is_active(&self) -> bool {
        *self.active.read()
    }

    /// Get information about all installed hooks
    pub fn get_all_hooks(&self) -> Vec<HookInfo> {
        self.hooks.read().values().cloned().collect()
    }

    /// Get the configuration
    pub fn config(&self) -> &HookConfig {
        &self.config
    }
}

impl Drop for PhotonHook {
    fn drop(&mut self) {
        if *self.active.read() {
            if let Err(e) = self.uninstall() {
                error!("Error uninstalling hooks during drop: {:?}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graphics_api_module_names() {
        assert_eq!(GraphicsAPI::DirectX11.module_name(), "d3d11.dll");
        assert_eq!(GraphicsAPI::Vulkan.module_name(), "vulkan-1.dll");
    }

    #[test]
    fn test_hook_config_default() {
        let config = HookConfig::default();
        assert!(config.hook_dx11);
        assert!(config.hook_present);
        assert!(config.hook_draw);
    }

    #[test]
    fn test_photon_hook_creation() {
        let config = HookConfig::default();
        let hook = PhotonHook::new(config);
        assert!(hook.is_ok());
    }
}
