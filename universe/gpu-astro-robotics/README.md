# Astrodynamics and Robotics Five-Backend GPU Compute Kernels

This bundle graduates eight pure-MIT BuildLang `#[compute]` GPU kernels compiled across WGSL, HLSL, MSL, SPIR-V, and GLSL using `buildc` under `strict-bit-exact` subgroup validation. Every source file passes `buildc check` plus `buildc lint` inside `buildc check-dir` cleanly while emitting verified `buildlang.gpu-tv-cert/v1` certificates across five shader targets.

| Kernel File | Domain | Compute Scope |
| :- | :- | :- |
| `kepler_lambert_orbit_gpu.bld` | Astrodynamics | Universal-variable Stumpff Newton update emitting Lagrange `f` and `g` |
| `mhd_hll_plasma_flux_gpu.bld` | Plasma Physics | Ideal MHD fast magnetosonic speeds with HLL Riemann interface fluxes |
| `inverse_kinematics_ccd_gpu.bld` | Robotics | Cyclic-coordinate-descent joint rotation step with clamped angle limits |
| `icp_pointcloud_svd_gpu.bld` | LiDAR SLAM | Huber-weighted 3D centered correspondence covariance plus residual |
| `rayleigh_mie_phase_gpu.bld` | Optics | Cornette-Shanks Mie aerosol phase blended with Rayleigh spectral scatter |
| `svd_jacobi_3x3_gpu.bld` | Linear Algebra | Symmetric 3x3 Givens Jacobi eigensolver sweep preserving matrix trace |
| `hilbert_curve_morton3d_gpu.bld` | Spatial Hashing | 30-bit `u32` Morton Z-order dilation with Gray-Hilbert key plus x decode |
| `reed_solomon_gf256_gpu.bld` | Error Coding | GF(`2^8`) Russian-peasant Horner evaluation emitting parity `S0` and `S1` |

All eight kernels originate from the verified 440-shader GPU compute pack (`wave156b`). Each file carries pure-OSI MIT licensing (`LICENSE`) with self-contained compute math and zero external dependencies.
