//! Drive the C ABI from Rust to verify the FFI boundary logic without a C
//! toolchain. The same calls a C/C++ D3D11 hook would make.

use photon_frametrace::ffi::*;
use photon_frametrace::FrameState;

#[test]
fn c_abi_detects_the_ssr_hazard() {
    unsafe {
        let s: *mut FrameState = ft_new();
        // SSR resource 0x5232: SRV view 1, RTV view 2.
        ft_register_view(s, 1, 0x5232, 0); // Srv
        ft_register_view(s, 2, 0x5232, 1); // Rtv
        let rtvs = [2u64];
        ft_set_render_targets(s, rtvs.as_ptr(), rtvs.len(), 0);
        let srvs = [1u64];
        ft_set_shader_resources(s, 1 /*Ps*/, 27, srvs.as_ptr(), srvs.len());
        ft_draw(s);

        assert_eq!(ft_hazard_count(s), 1);
        assert_eq!(ft_hazard_kind(s, 0), 0); // ReadWrite
        assert_eq!(ft_hazard_resource(s, 0), 0x5232);

        // Unbind t27 with a null view id (0) and confirm the hazard clears.
        let nulls = [0u64];
        ft_set_shader_resources(s, 1, 27, nulls.as_ptr(), nulls.len());
        assert_eq!(ft_hazard_count(s), 0);

        ft_free(s);
    }
}

#[test]
fn c_abi_restore_verify_detects_a_leak() {
    unsafe {
        let s: *mut FrameState = ft_new();
        ft_register_view(s, 10, 100, 0); // game SRV, resource 100
        let srvs = [10u64];
        ft_set_shader_resources(s, 1, 27, srvs.as_ptr(), srvs.len());
        let saved = ft_snapshot(s);
        // effect unbinds t27 and never restores it
        let nulls = [0u64];
        ft_set_shader_resources(s, 1, 27, nulls.as_ptr(), nulls.len());
        let restored = ft_snapshot(s);
        assert_eq!(ft_restore_leak_count(saved, restored), 1);
        let mut buf = [0 as std::os::raw::c_char; 128];
        let n = ft_restore_first_leak(saved, restored, buf.as_mut_ptr(), buf.len());
        assert!(n > 0);
        ft_snapshot_free(saved);
        ft_snapshot_free(restored);
        ft_free(s);
    }
}
