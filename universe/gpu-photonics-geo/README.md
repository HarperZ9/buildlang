# Photonics and Scientific Five-Backend GPU Compute Kernels

This bundle graduates eight pure-MIT BuildLang `#[compute]` GPU kernels compiled across WGSL, HLSL, MSL, SPIR-V, and GLSL using `buildc` under `strict-bit-exact` subgroup validation. Every source file passes `buildc check` plus `buildc lint` inside `buildc check-dir` cleanly while emitting verified `buildlang.gpu-tv-cert/v1` certificates across five shader targets.

| Kernel File | Domain | Compute Scope |
| :- | :- | :- |
| `spectral_path_mis_bsdf_gpu.bld` | Spectral Photonics | Cauchy-dispersed Schlick Fresnel with GGX masking plus Veach power-heuristic MIS |
| `atmosphere_multiscatter_lut_gpu.bld` | Atmospheric Optics | Hillaire Rayleigh/Mie/Ozone extinction with Henyey-Greenstein phase plus `Psi_ms` LUT |
| `nbody_barnes_hut_quadrupole_gpu.bld` | Astrophysics | Softened 3D monopole attraction plus traceless quadrupole tensor gravity acceleration |
| `smith_waterman_banded_gpu.bld` | Genomics | Gotoh affine-gap banded Smith-Waterman local alignment cell with running max score |
| `seismic_rtm_acoustic_3d_gpu.bld` | Geophysics | Fourth-order 3D acoustic RTM stencil with Cerjan sponge damping plus imaging cross-correlation |
| `lattice_boltzmann_d3q19_gpu.bld` | Fluid Dynamics | D3Q19 Lattice-Boltzmann velocity reconstruction with Maxwell equilibrium plus BGK relaxation |
| `optimal_transport_sinkhorn_gpu.bld` | Optimal Transport | Four-column Sinkhorn-Knopp entropic Gibbs row scaling update with transported cost |
| `cholesky_batched_spd_gpu.bld` | Linear Algebra | Batched 3x3 SPD Cholesky `L * L^T` factorization with forward/backward solve |

All eight kernels originate from the verified 320-shader GPU compute pack (`wave138a`). Each file carries pure-OSI MIT licensing (`LICENSE`) with self-contained compute math and zero external dependencies.
