# Frontier Domain Five-Backend GPU Compute Kernels

This bundle graduates eight pure-MIT BuildLang `#[compute]` GPU kernels compiled across WGSL, HLSL, MSL, SPIR-V, and GLSL using `buildc` under `strict-bit-exact` subgroup validation. Every source file passes `buildc check` plus `buildc lint` inside `buildc check-dir` cleanly while emitting verified `buildlang.gpu-tv-cert/v1` certificates across five shader targets.

| Kernel File | Domain | Compute Scope |
| :- | :- | :- |
| `rigid_body_lcp_pgs_gpu.bld` | Rigid-Body Physics | Baumgarte Projected Gauss-Seidel normal impulse solve with Coulomb friction clamping |
| `mpm_snow_elastoplastic_gpu.bld` | Continuum Mechanics | Singular-value snow yield clamping with plastic hardening Lame moduli plus Cauchy stress |
| `fdtd_acoustic_room_ir_gpu.bld` | Wave Acoustics | Staggered 3D velocity-pressure FDTD update with wall impedance absorption plus energy density |
| `qft_statevector_gate_gpu.bld` | Quantum Simulation | Controlled phase gate `R_k` followed by unitary Hadamard butterfly with norm tracking |
| `cryoem_ctf_wiener_filter_gpu.bld` | Structural Biology | Aberration phase-contrast CTF with B-factor envelope plus SNR-regularized Wiener filter |
| `kalman_ekf_slam_update_gpu.bld` | Robotics SLAM | Newton-Raphson range Jacobian with Kalman gain plus positive-definite Joseph covariance |
| `signed_distance_jfa_3d_gpu.bld` | Geometry | Three-dimensional Jump Flooding nearest-seed propagation emitting signed Euclidean SDF |
| `neural_radiance_hashgrid_gpu.bld` | Neural Rendering | Bilinear hash-grid feature blend with Beer-Lambert transmittance volume marching |

All eight kernels originate from the verified 280-shader GPU compute pack (`wave135c`). Each file carries pure-OSI MIT licensing (`LICENSE`) with self-contained compute math and zero external dependencies.
