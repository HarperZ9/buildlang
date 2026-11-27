# Post-Quantum Crypto and Medical Imaging Five-Backend GPU Compute Kernels

This bundle graduates eight pure-MIT BuildLang `#[compute]` GPU kernels compiled across WGSL, HLSL, MSL, SPIR-V, and GLSL using `buildc` under `strict-bit-exact` subgroup validation. Every source file passes `buildc check` plus `buildc lint` inside `buildc check-dir` cleanly while emitting verified `buildlang.gpu-tv-cert/v1` certificates across five shader targets.

| Kernel File | Domain | Compute Scope |
| :- | :- | :- |
| `mlkem_ntt_q3329_gpu.bld` | Post-Quantum Crypto | FIPS 203 `q=3329` ML-KEM Cooley-Tukey butterfly using exact `i32` Barrett reduction |
| `blake3_compress_gpu.bld` | Cryptographic Hash | Seven-round BLAKE3 `u32` ARX quarter-round compression with XOR chaining fold |
| `chacha20_block_gpu.bld` | Stream Cipher | RFC 8439 twenty-round ChaCha20 `u32` ARX double-round keystream generator |
| `radon_fbp_ct_recon_gpu.bld` | Medical CT | Ram-Lak filtered back-projection Radon sinogram interpolation into voxel space |
| `mri_nufft_gridding_gpu.bld` | Medical MRI | Two-dimensional Kaiser-Bessel NUFFT k-space gridding with density compensation |
| `biquad_iir_bank_dsp_gpu.bld` | Studio Audio DSP | Cascaded two-band Transposed Direct Form II parametric biquad EQ with soft clip |
| `kalman_ukf_sigma_pts_gpu.bld` | Nonlinear Filtering | Unscented Kalman Filter three-point sigma propagation with posterior variance |
| `voronoi_lloyd_relax_gpu.bld` | Geometry | Density-weighted Centroidal Voronoi Tessellation Lloyd relaxation step |

All eight kernels originate from the verified 360-shader GPU compute pack (`wave142a`). Each file carries pure-OSI MIT licensing (`LICENSE`) with self-contained compute math and zero external dependencies.
