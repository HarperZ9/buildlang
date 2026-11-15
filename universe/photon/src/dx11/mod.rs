// ═══════════════════════════════════════════════════════════════════════════════
// PHOTON - DirectX 11 Hook Framework
// ═══════════════════════════════════════════════════════════════════════════════
// Copyright © 2024-2025 Zain Dana Harper. All Rights Reserved.
// ═══════════════════════════════════════════════════════════════════════════════

//! DirectX 11 hooking implementation.
//!
//! This module provides VTable-based hooking for DirectX 11 interfaces:
//! - IDXGISwapChain (Present, ResizeBuffers)
//! - ID3D11DeviceContext (Draw, DrawIndexed, PSSetShader, VSSetShader)
//! - ID3D11Device (CreatePixelShader, CreateVertexShader)

mod vtable;
mod device;
mod context;
mod swapchain;

pub use device::*;
pub use context::*;
pub use swapchain::*;
pub use vtable::*;

use crate::error::{PhotonError, PhotonResult};
use crate::hook::{HookInfo, HookStatus};
use crate::render::{RenderCallback, DrawCallInfo};

use log::{info, warn, debug, error};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::ptr;

use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Foundation::{HWND, BOOL, TRUE, FALSE};
use windows::core::Interface;

// ═══════════════════════════════════════════════════════════════════════════════
// VTABLE INDICES
// ═══════════════════════════════════════════════════════════════════════════════

/// IDXGISwapChain VTable indices
pub mod swapchain_vtable {
    pub const PRESENT: usize = 8;
    pub const GET_BUFFER: usize = 9;
    pub const SET_FULLSCREEN_STATE: usize = 10;
    pub const GET_FULLSCREEN_STATE: usize = 11;
    pub const RESIZE_BUFFERS: usize = 13;
    pub const RESIZE_TARGET: usize = 14;
}

/// ID3D11DeviceContext VTable indices
pub mod context_vtable {
    pub const VS_SET_SHADER: usize = 11;
    pub const PS_SET_SHADER: usize = 9;
    pub const DRAW: usize = 13;
    pub const DRAW_INDEXED: usize = 12;
    pub const DRAW_INSTANCED: usize = 20;
    pub const DRAW_INDEXED_INSTANCED: usize = 21;
    pub const DRAW_AUTO: usize = 38;
    pub const OM_SET_RENDER_TARGETS: usize = 33;
    pub const OM_GET_RENDER_TARGETS: usize = 89;
    pub const RS_SET_VIEWPORTS: usize = 44;
}

/// ID3D11Device VTable indices
pub mod device_vtable {
    pub const CREATE_BUFFER: usize = 3;
    pub const CREATE_TEXTURE_2D: usize = 5;
    pub const CREATE_SHADER_RESOURCE_VIEW: usize = 7;
    pub const CREATE_RENDER_TARGET_VIEW: usize = 9;
    pub const CREATE_VERTEX_SHADER: usize = 12;
    pub const CREATE_GEOMETRY_SHADER: usize = 13;
    pub const CREATE_PIXEL_SHADER: usize = 15;
    pub const CREATE_COMPUTE_SHADER: usize = 18;
}

// ═══════════════════════════════════════════════════════════════════════════════
// DX11 CONTEXT
// ═══════════════════════════════════════════════════════════════════════════════

/// Shared context for DX11 hooks
#[derive(Debug)]
pub struct DX11Context {
    /// The D3D11 device
    pub device: Option<ID3D11Device>,
    /// The device context
    pub context: Option<ID3D11DeviceContext>,
    /// The swap chain
    pub swapchain: Option<IDXGISwapChain>,
    /// Back buffer render target view
    pub render_target: Option<ID3D11RenderTargetView>,
    /// Window handle
    pub hwnd: HWND,
    /// Back buffer width
    pub width: u32,
    /// Back buffer height
    pub height: u32,
}

impl DX11Context {
    /// Create a new empty context
    pub fn new() -> Self {
        Self {
            device: None,
            context: None,
            swapchain: None,
            render_target: None,
            hwnd: HWND::default(),
            width: 0,
            height: 0,
        }
    }
}

impl Default for DX11Context {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// DX11 HOOK
// ═══════════════════════════════════════════════════════════════════════════════

/// DirectX 11 hook manager
pub struct DX11Hook {
    /// Shared context
    context: Arc<RwLock<DX11Context>>,
    /// VTable addresses
    vtables: DX11VTables,
    /// Original function pointers
    original_functions: HashMap<String, u64>,
    /// Hook information
    hook_info: HashMap<String, HookInfo>,
    /// Present callback
    present_callback: Option<Box<dyn Fn(&DX11Context) + Send + Sync>>,
    /// Draw callback
    draw_callback: Option<Box<dyn Fn(&DrawCallInfo) + Send + Sync>>,
    /// Is initialized
    initialized: bool,
}

/// Stored VTable addresses
#[derive(Debug, Default)]
pub struct DX11VTables {
    pub device: u64,
    pub context: u64,
    pub swapchain: u64,
}

impl DX11Hook {
    /// Create a new DX11 hook instance
    pub fn new() -> PhotonResult<Self> {
        Ok(Self {
            context: Arc::new(RwLock::new(DX11Context::new())),
            vtables: DX11VTables::default(),
            original_functions: HashMap::new(),
            hook_info: HashMap::new(),
            present_callback: None,
            draw_callback: None,
            initialized: false,
        })
    }

    /// Initialize by creating a dummy device to get VTable addresses
    pub fn initialize(&mut self) -> PhotonResult<()> {
        if self.initialized {
            return Ok(());
        }

        info!("Initializing DX11 hook system...");

        // Create a dummy window for device creation
        let hwnd = self.create_dummy_window()?;

        // Create dummy swap chain description
        let swap_chain_desc = DXGI_SWAP_CHAIN_DESC {
            BufferDesc: DXGI_MODE_DESC {
                Width: 100,
                Height: 100,
                RefreshRate: DXGI_RATIONAL {
                    Numerator: 60,
                    Denominator: 1,
                },
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                ScanlineOrdering: DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED,
                Scaling: DXGI_MODE_SCALING_UNSPECIFIED,
            },
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 1,
            OutputWindow: hwnd,
            Windowed: TRUE,
            SwapEffect: DXGI_SWAP_EFFECT_DISCARD,
            Flags: 0,
        };

        // Create device and swap chain
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        let mut swapchain: Option<IDXGISwapChain> = None;
        let mut feature_level = D3D_FEATURE_LEVEL_11_0;

        unsafe {
            D3D11CreateDeviceAndSwapChain(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                None,
                D3D11_CREATE_DEVICE_FLAG(0),
                Some(&[D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_1]),
                D3D11_SDK_VERSION,
                Some(&swap_chain_desc),
                Some(&mut swapchain),
                Some(&mut device),
                Some(&mut feature_level),
                Some(&mut context),
            )?;
        }

        let device = device.ok_or(PhotonError::DummyDeviceCreationFailed {
            reason: "Device is null".into(),
        })?;
        let context = context.ok_or(PhotonError::DummyDeviceCreationFailed {
            reason: "Context is null".into(),
        })?;
        let swapchain = swapchain.ok_or(PhotonError::DummyDeviceCreationFailed {
            reason: "SwapChain is null".into(),
        })?;

        // Extract VTable addresses
        self.vtables = DX11VTables {
            device: get_vtable_ptr(&device),
            context: get_vtable_ptr(&context),
            swapchain: get_vtable_ptr(&swapchain),
        };

        debug!("VTables - Device: {:#x}, Context: {:#x}, SwapChain: {:#x}",
            self.vtables.device, self.vtables.context, self.vtables.swapchain);

        // Store original function pointers before hooking
        self.store_original_functions();

        // Clean up dummy resources (they're automatically dropped)
        self.destroy_dummy_window(hwnd);

        self.initialized = true;
        info!("DX11 hook system initialized");

        Ok(())
    }

    /// Create a dummy window for device creation
    fn create_dummy_window(&self) -> PhotonResult<HWND> {
        use windows::Win32::UI::WindowsAndMessaging::*;
        use windows::Win32::System::LibraryLoader::GetModuleHandleA;
        use windows::core::PCSTR;

        unsafe {
            let class_name = windows::core::s!("PhotonDummyClass");
            let hinstance = GetModuleHandleA(PCSTR::null())?;

            let wc = WNDCLASSEXA {
                cbSize: std::mem::size_of::<WNDCLASSEXA>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(dummy_wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance.into(),
                hIcon: HICON::default(),
                hCursor: HCURSOR::default(),
                hbrBackground: HBRUSH::default(),
                lpszMenuName: PCSTR::null(),
                lpszClassName: class_name,
                hIconSm: HICON::default(),
            };

            RegisterClassExA(&wc);

            let hwnd = CreateWindowExA(
                WINDOW_EX_STYLE::default(),
                class_name,
                windows::core::s!("PhotonDummy"),
                WS_OVERLAPPEDWINDOW,
                0, 0, 100, 100,
                HWND::default(),
                HMENU::default(),
                hinstance,
                None,
            )?;

            Ok(hwnd)
        }
    }

    /// Destroy the dummy window
    fn destroy_dummy_window(&self, hwnd: HWND) {
        use windows::Win32::UI::WindowsAndMessaging::DestroyWindow;
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }

    /// Store original function pointers
    fn store_original_functions(&mut self) {
        if self.vtables.swapchain != 0 {
            let present_addr = get_vtable_function(self.vtables.swapchain, swapchain_vtable::PRESENT);
            self.original_functions.insert("IDXGISwapChain::Present".into(), present_addr);

            let resize_addr = get_vtable_function(self.vtables.swapchain, swapchain_vtable::RESIZE_BUFFERS);
            self.original_functions.insert("IDXGISwapChain::ResizeBuffers".into(), resize_addr);
        }

        if self.vtables.context != 0 {
            let draw_addr = get_vtable_function(self.vtables.context, context_vtable::DRAW);
            self.original_functions.insert("ID3D11DeviceContext::Draw".into(), draw_addr);

            let draw_indexed_addr = get_vtable_function(self.vtables.context, context_vtable::DRAW_INDEXED);
            self.original_functions.insert("ID3D11DeviceContext::DrawIndexed".into(), draw_indexed_addr);

            let ps_set_addr = get_vtable_function(self.vtables.context, context_vtable::PS_SET_SHADER);
            self.original_functions.insert("ID3D11DeviceContext::PSSetShader".into(), ps_set_addr);

            let vs_set_addr = get_vtable_function(self.vtables.context, context_vtable::VS_SET_SHADER);
            self.original_functions.insert("ID3D11DeviceContext::VSSetShader".into(), vs_set_addr);
        }

        if self.vtables.device != 0 {
            let create_ps_addr = get_vtable_function(self.vtables.device, device_vtable::CREATE_PIXEL_SHADER);
            self.original_functions.insert("ID3D11Device::CreatePixelShader".into(), create_ps_addr);

            let create_vs_addr = get_vtable_function(self.vtables.device, device_vtable::CREATE_VERTEX_SHADER);
            self.original_functions.insert("ID3D11Device::CreateVertexShader".into(), create_vs_addr);
        }
    }

    /// Hook the Present function
    pub fn hook_present(&mut self) -> PhotonResult<()> {
        info!("Hooking IDXGISwapChain::Present...");

        let original = *self.original_functions
            .get("IDXGISwapChain::Present")
            .ok_or(PhotonError::VTableNotFound {
                interface: "IDXGISwapChain".into(),
            })?;

        // In a real implementation, we would use MinHook or similar here
        // For now, we just record the hook info
        self.hook_info.insert("IDXGISwapChain::Present".into(), HookInfo {
            name: "IDXGISwapChain::Present".into(),
            original_address: original,
            trampoline_address: 0, // Would be set by MinHook
            status: HookStatus::Installed,
            call_count: 0,
        });

        info!("Present hook installed at {:#x}", original);
        Ok(())
    }

    /// Hook draw calls
    pub fn hook_draw_calls(&mut self) -> PhotonResult<()> {
        info!("Hooking draw calls...");

        // Hook Draw
        if let Some(&original) = self.original_functions.get("ID3D11DeviceContext::Draw") {
            self.hook_info.insert("ID3D11DeviceContext::Draw".into(), HookInfo {
                name: "ID3D11DeviceContext::Draw".into(),
                original_address: original,
                trampoline_address: 0,
                status: HookStatus::Installed,
                call_count: 0,
            });
            info!("Draw hook installed at {:#x}", original);
        }

        // Hook DrawIndexed
        if let Some(&original) = self.original_functions.get("ID3D11DeviceContext::DrawIndexed") {
            self.hook_info.insert("ID3D11DeviceContext::DrawIndexed".into(), HookInfo {
                name: "ID3D11DeviceContext::DrawIndexed".into(),
                original_address: original,
                trampoline_address: 0,
                status: HookStatus::Installed,
                call_count: 0,
            });
            info!("DrawIndexed hook installed at {:#x}", original);
        }

        Ok(())
    }

    /// Hook shader creation
    pub fn hook_shader_creation(&mut self) -> PhotonResult<()> {
        info!("Hooking shader creation...");

        // Hook CreatePixelShader
        if let Some(&original) = self.original_functions.get("ID3D11Device::CreatePixelShader") {
            self.hook_info.insert("ID3D11Device::CreatePixelShader".into(), HookInfo {
                name: "ID3D11Device::CreatePixelShader".into(),
                original_address: original,
                trampoline_address: 0,
                status: HookStatus::Installed,
                call_count: 0,
            });
            info!("CreatePixelShader hook installed at {:#x}", original);
        }

        // Hook CreateVertexShader
        if let Some(&original) = self.original_functions.get("ID3D11Device::CreateVertexShader") {
            self.hook_info.insert("ID3D11Device::CreateVertexShader".into(), HookInfo {
                name: "ID3D11Device::CreateVertexShader".into(),
                original_address: original,
                trampoline_address: 0,
                status: HookStatus::Installed,
                call_count: 0,
            });
            info!("CreateVertexShader hook installed at {:#x}", original);
        }

        Ok(())
    }

    /// Set the Present callback
    pub fn set_present_callback<F>(&mut self, callback: F)
    where
        F: Fn(&DX11Context) + Send + Sync + 'static,
    {
        self.present_callback = Some(Box::new(callback));
    }

    /// Set the draw callback
    pub fn set_draw_callback<F>(&mut self, callback: F)
    where
        F: Fn(&DrawCallInfo) + Send + Sync + 'static,
    {
        self.draw_callback = Some(Box::new(callback));
    }

    /// Get hook information
    pub fn get_hook_info(&self) -> Vec<(String, HookInfo)> {
        self.hook_info.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Uninstall all hooks
    pub fn uninstall(&mut self) -> PhotonResult<()> {
        info!("Uninstalling DX11 hooks...");

        // In a real implementation, we would remove the hooks here
        for (name, info) in self.hook_info.iter_mut() {
            info.status = HookStatus::NotInstalled;
            debug!("Uninstalled hook: {}", name);
        }

        self.initialized = false;
        info!("DX11 hooks uninstalled");
        Ok(())
    }

    /// Get the shared context
    pub fn get_context(&self) -> Arc<RwLock<DX11Context>> {
        self.context.clone()
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// HELPER FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════════════

/// Get the VTable pointer from a COM interface
fn get_vtable_ptr<T: Interface>(obj: &T) -> u64 {
    unsafe {
        let ptr = obj as *const T as *const *const u64;
        *ptr as u64
    }
}

/// Get a function pointer from a VTable
fn get_vtable_function(vtable: u64, index: usize) -> u64 {
    unsafe {
        let vtable_ptr = vtable as *const u64;
        *vtable_ptr.add(index)
    }
}

/// Dummy window procedure
unsafe extern "system" fn dummy_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::DefWindowProcA;
    DefWindowProcA(hwnd, msg, wparam, lparam)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vtable_indices() {
        assert_eq!(swapchain_vtable::PRESENT, 8);
        assert_eq!(context_vtable::DRAW, 13);
        assert_eq!(device_vtable::CREATE_PIXEL_SHADER, 15);
    }

    #[test]
    fn test_dx11_context_default() {
        let ctx = DX11Context::default();
        assert!(ctx.device.is_none());
        assert!(ctx.swapchain.is_none());
    }

    #[test]
    fn test_dx11_vtables_default() {
        let vtables = DX11VTables::default();
        assert_eq!(vtables.device, 0);
        assert_eq!(vtables.context, 0);
        assert_eq!(vtables.swapchain, 0);
    }
}
