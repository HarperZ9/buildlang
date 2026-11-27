# Quadricentennial Engine and Climate Five-Backend GPU Compute Kernels

This bundle graduates eight pure-MIT BuildLang `#[compute]` GPU kernels compiled across WGSL, HLSL, MSL, SPIR-V, and GLSL using `buildc` under `strict-bit-exact` subgroup validation. Every source file passes `buildc check` plus `buildc lint` inside `buildc check-dir` cleanly while emitting verified `buildlang.gpu-tv-cert/v1` certificates across five shader targets.

| Kernel File | Domain | Compute Scope |
| :- | :- | :- |
| `meshlet_frustum_hi_z_cull_gpu.bld` | Engine Culling | Frustum plane sphere culling with backface cone test plus Hi-Z pyramid depth check |
| `restir_di_reservoir_resample_gpu.bld` | Real-Time Lighting | Spatio-temporal ReSTIR DI MIS weighted reservoir resampling estimator |
| `shallow_water_swe_climate_gpu.bld` | Climate PDE | Two-dimensional shallow-water height and momentum update with Coriolis plus drag |
| `protein_dihedral_ramachandran_gpu.bld` | Structural Biology | Protein backbone dihedral Fourier torsion with soft-core Lennard-Jones 12-6 |
| `bc7_block_endpoint_fit_gpu.bld` | Texture Compression | RGB block mean with principal covariance projection plus BC7 endpoint error fit |
| `radix_sort_histogram_scan_gpu.bld` | GPU Primitives | Four-bit `u32` radix digit extraction with bucket histogram plus prefix scan |
| `delaunay_flip_edge_gpu.bld` | Mesh Geometry | Lifted paraboloid 3x3 Delaunay incircle determinant with edge-flip flag |
| `cloth_pbd_bending_gpu.bld` | Cloth Simulation | Compliance-damped XPBD isometric bending Lagrange multiplier plus vertex shift |

All eight kernels originate from the verified Quadricentennial 400-shader GPU compute pack (`wave146a`). Each file carries pure-OSI MIT licensing (`LICENSE`) with self-contained compute math and zero external dependencies.
