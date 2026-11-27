# QEC and Graph ML Five-Backend GPU Compute Kernels

This bundle graduates eight pure-MIT BuildLang `#[compute]` GPU kernels compiled across WGSL, HLSL, MSL, SPIR-V, and GLSL using `buildc` under `strict-bit-exact` subgroup validation. Every source file passes `buildc check` plus `buildc lint` inside `buildc check-dir` cleanly while emitting verified `buildlang.gpu-tv-cert/v1` certificates across five shader targets.

| Kernel File | Domain | Compute Scope |
| :- | :- | :- |
| `surface_code_syndrome_gpu.bld` | Quantum QEC | Four-qubit `u32` stabilizer parity syndrome with defect mask plus Pauli correction |
| `graphsage_neighbor_agg_gpu.bld` | Graph ML | Attention-weighted neighbor pooling fused with ReLU self projection plus L2 norm |
| `black_scholes_greeks_gpu.bld` | Option Pricing | European call option price plus analytical Delta, Gamma, Vega, Theta Greeks |
| `conjugate_gradient_spmv_gpu.bld` | Sparse Linear Algebra | Four-tap CSR SpMV `A*p` fused with Conjugate Gradient `x`, `r`, `p` update |
| `kalman_rts_smoother_gpu.bld` | State Estimation | Backward Rauch-Tung-Striebel smoother gain with covariance update |
| `bloom_filter_xxhash64_gpu.bld` | Hashing | Multi-probe `u32` Bloom membership test with xxHash32 double-hashing |
| `catmull_clark_subdiv_gpu.bld` | Mesh Subdivision | Valence-n Catmull-Clark face-edge-vertex 3D limit smoothing stencil |
| `bilateral_guided_denoise_gpu.bld` | GI Denoising | Edge-stopping A-Trous joint bilateral filter weighted by luma, normal, depth |

All eight kernels originate from the verified 480-shader GPU compute pack (`wave161a`). Each file carries pure-OSI MIT licensing (`LICENSE`) with self-contained compute math and zero external dependencies.
