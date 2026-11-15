// ═══════════════════════════════════════════════════════════════════════════════
// PHOTON - VTable Manipulation Utilities
// ═══════════════════════════════════════════════════════════════════════════════
// Copyright © 2024-2025 Zain Dana Harper. All Rights Reserved.
// ═══════════════════════════════════════════════════════════════════════════════

//! VTable manipulation and hook installation utilities.

use crate::error::{PhotonError, PhotonResult};

use log::{debug, warn};
use std::ptr;

use windows::Win32::System::Memory::{
    VirtualProtect, PAGE_PROTECTION_FLAGS, PAGE_EXECUTE_READWRITE, PAGE_READONLY,
};

// ═══════════════════════════════════════════════════════════════════════════════
// VTABLE HOOK
// ═══════════════════════════════════════════════════════════════════════════════

/// A VTable hook that replaces a function pointer in a COM interface's VTable
pub struct VTableHook {
    /// Address of the VTable entry
    vtable_entry: *mut u64,
    /// Original function pointer
    original: u64,
    /// Hook function pointer
    hook: u64,
    /// Whether the hook is currently installed
    installed: bool,
}

impl VTableHook {
    /// Create a new VTable hook
    ///
    /// # Safety
    ///
    /// The vtable_addr must be a valid VTable pointer and index must be valid.
    pub unsafe fn new(vtable_addr: u64, index: usize, hook_fn: u64) -> PhotonResult<Self> {
        let vtable = vtable_addr as *mut u64;
        let vtable_entry = vtable.add(index);
        let original = *vtable_entry;

        Ok(Self {
            vtable_entry,
            original,
            hook: hook_fn,
            installed: false,
        })
    }

    /// Install the hook by replacing the VTable entry
    pub fn install(&mut self) -> PhotonResult<()> {
        if self.installed {
            return Ok(());
        }

        unsafe {
            // Change memory protection to allow writing
            let mut old_protect = PAGE_PROTECTION_FLAGS::default();
            let result = VirtualProtect(
                self.vtable_entry as *mut _,
                std::mem::size_of::<u64>(),
                PAGE_EXECUTE_READWRITE,
                &mut old_protect,
            );

            if result.is_err() {
                return Err(PhotonError::MemoryProtectFailed {
                    address: self.vtable_entry as u64,
                });
            }

            // Replace the function pointer
            *self.vtable_entry = self.hook;

            // Restore original protection
            let _ = VirtualProtect(
                self.vtable_entry as *mut _,
                std::mem::size_of::<u64>(),
                old_protect,
                &mut old_protect,
            );
        }

        self.installed = true;
        debug!("VTable hook installed at {:p}", self.vtable_entry);

        Ok(())
    }

    /// Uninstall the hook by restoring the original VTable entry
    pub fn uninstall(&mut self) -> PhotonResult<()> {
        if !self.installed {
            return Ok(());
        }

        unsafe {
            // Change memory protection to allow writing
            let mut old_protect = PAGE_PROTECTION_FLAGS::default();
            let result = VirtualProtect(
                self.vtable_entry as *mut _,
                std::mem::size_of::<u64>(),
                PAGE_EXECUTE_READWRITE,
                &mut old_protect,
            );

            if result.is_err() {
                return Err(PhotonError::MemoryProtectFailed {
                    address: self.vtable_entry as u64,
                });
            }

            // Restore the original function pointer
            *self.vtable_entry = self.original;

            // Restore original protection
            let _ = VirtualProtect(
                self.vtable_entry as *mut _,
                std::mem::size_of::<u64>(),
                old_protect,
                &mut old_protect,
            );
        }

        self.installed = false;
        debug!("VTable hook uninstalled at {:p}", self.vtable_entry);

        Ok(())
    }

    /// Get the original function pointer
    pub fn original(&self) -> u64 {
        self.original
    }

    /// Check if the hook is installed
    pub fn is_installed(&self) -> bool {
        self.installed
    }
}

impl Drop for VTableHook {
    fn drop(&mut self) {
        if self.installed {
            if let Err(e) = self.uninstall() {
                warn!("Failed to uninstall VTable hook on drop: {:?}", e);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// DETOUR HOOK (using inline patching)
// ═══════════════════════════════════════════════════════════════════════════════

/// Size of a JMP instruction on x64
const JMP_SIZE: usize = 14; // JMP [RIP+0] + 8-byte address

/// A detour hook that patches the target function to jump to the hook
pub struct DetourHook {
    /// Target function address
    target: u64,
    /// Hook function address
    hook: u64,
    /// Original bytes that were overwritten
    original_bytes: [u8; JMP_SIZE],
    /// Trampoline for calling the original function
    trampoline: Vec<u8>,
    /// Whether the hook is installed
    installed: bool,
}

impl DetourHook {
    /// Create a new detour hook
    ///
    /// # Safety
    ///
    /// target must be a valid function address.
    pub unsafe fn new(target: u64, hook: u64) -> PhotonResult<Self> {
        let mut original_bytes = [0u8; JMP_SIZE];

        // Read the original bytes
        ptr::copy_nonoverlapping(
            target as *const u8,
            original_bytes.as_mut_ptr(),
            JMP_SIZE,
        );

        // Create trampoline (simplified - real implementation would use proper disassembly)
        let trampoline = create_trampoline(target, &original_bytes)?;

        Ok(Self {
            target,
            hook,
            original_bytes,
            trampoline,
            installed: false,
        })
    }

    /// Install the detour
    pub fn install(&mut self) -> PhotonResult<()> {
        if self.installed {
            return Ok(());
        }

        unsafe {
            // Change memory protection
            let mut old_protect = PAGE_PROTECTION_FLAGS::default();
            let result = VirtualProtect(
                self.target as *mut _,
                JMP_SIZE,
                PAGE_EXECUTE_READWRITE,
                &mut old_protect,
            );

            if result.is_err() {
                return Err(PhotonError::MemoryProtectFailed {
                    address: self.target,
                });
            }

            // Write the JMP instruction
            write_jmp(self.target as *mut u8, self.hook);

            // Restore protection
            let _ = VirtualProtect(
                self.target as *mut _,
                JMP_SIZE,
                old_protect,
                &mut old_protect,
            );
        }

        self.installed = true;
        debug!("Detour installed at {:#x} -> {:#x}", self.target, self.hook);

        Ok(())
    }

    /// Uninstall the detour
    pub fn uninstall(&mut self) -> PhotonResult<()> {
        if !self.installed {
            return Ok(());
        }

        unsafe {
            // Change memory protection
            let mut old_protect = PAGE_PROTECTION_FLAGS::default();
            let result = VirtualProtect(
                self.target as *mut _,
                JMP_SIZE,
                PAGE_EXECUTE_READWRITE,
                &mut old_protect,
            );

            if result.is_err() {
                return Err(PhotonError::MemoryProtectFailed {
                    address: self.target,
                });
            }

            // Restore original bytes
            ptr::copy_nonoverlapping(
                self.original_bytes.as_ptr(),
                self.target as *mut u8,
                JMP_SIZE,
            );

            // Restore protection
            let _ = VirtualProtect(
                self.target as *mut _,
                JMP_SIZE,
                old_protect,
                &mut old_protect,
            );
        }

        self.installed = false;
        debug!("Detour uninstalled at {:#x}", self.target);

        Ok(())
    }

    /// Get the trampoline address for calling the original function
    pub fn trampoline(&self) -> u64 {
        self.trampoline.as_ptr() as u64
    }
}

impl Drop for DetourHook {
    fn drop(&mut self) {
        if self.installed {
            if let Err(e) = self.uninstall() {
                warn!("Failed to uninstall detour on drop: {:?}", e);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// HELPER FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════════════

/// Write a 64-bit JMP instruction at the target address
unsafe fn write_jmp(target: *mut u8, destination: u64) {
    // JMP [RIP+0] ; FF 25 00 00 00 00
    // DQ destination ; 8-byte address
    *target.add(0) = 0xFF;
    *target.add(1) = 0x25;
    *target.add(2) = 0x00;
    *target.add(3) = 0x00;
    *target.add(4) = 0x00;
    *target.add(5) = 0x00;

    // Write the 64-bit address
    ptr::copy_nonoverlapping(
        &destination as *const u64 as *const u8,
        target.add(6),
        8,
    );
}

/// Create a trampoline for the original function
fn create_trampoline(target: u64, original_bytes: &[u8; JMP_SIZE]) -> PhotonResult<Vec<u8>> {
    use windows::Win32::System::Memory::{
        VirtualAlloc, MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READWRITE,
    };

    // Allocate executable memory for the trampoline
    let size = JMP_SIZE + JMP_SIZE; // Original bytes + JMP back

    let trampoline_mem = unsafe {
        VirtualAlloc(
            None,
            size,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_EXECUTE_READWRITE,
        )
    };

    if trampoline_mem.is_null() {
        return Err(PhotonError::Internal {
            reason: "Failed to allocate trampoline memory".into(),
        });
    }

    unsafe {
        let trampoline = trampoline_mem as *mut u8;

        // Copy original bytes
        ptr::copy_nonoverlapping(
            original_bytes.as_ptr(),
            trampoline,
            JMP_SIZE,
        );

        // Write JMP to continue execution after the hook
        write_jmp(trampoline.add(JMP_SIZE), target + JMP_SIZE as u64);
    }

    // Return the trampoline as a Vec (for ownership tracking)
    // Note: In a real implementation, we'd need to properly free this memory
    let mut vec = Vec::with_capacity(size);
    unsafe {
        ptr::copy_nonoverlapping(
            trampoline_mem as *const u8,
            vec.as_mut_ptr(),
            size,
        );
        vec.set_len(size);
    }

    Ok(vec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jmp_size() {
        assert_eq!(JMP_SIZE, 14);
    }
}
