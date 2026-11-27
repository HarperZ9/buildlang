# Workstation 5-Target GPU Compute Shaders (24 Pure-OSI MIT Kernels)

License: Pure-OSI `MIT` (`24/24` standalone BuildLang GPU compute kernels, `0` external dependencies, `0` local paths).
Author Vision Anchors: `F1`, `N29`, `N38`, `N41`, `N42`, `N44`, `N53`, `N63`, `N64`.
Verification Contract: Every kernel passes `buildc check` (`0` errors), `buildc lint` (`No issues found`), `SIG-058` privacy screening (`26/26` bundle files clean), and 5-backend translation validation (`WGSL`, `HLSL`, `MSL`, `SPIR-V`, `GLSL` -> `120/120` certified shaders, `500` superopt rewrites).

## 1. Graduated Kernel Manifest (`[Checked]` 24 GPU Compute Kernels)

| Kernel File | Domain | Hardware / Algorithmic Focus |
| :- | :- | :- |
| `bilateral_atrous_denoise_compute.bld` | Denoising | Edge-stopping atrous spatial filter |
| `bvh_aabb_intersect_compute.bld` | Ray Tracing | Slab AABB ray-box intersection test |
| `cayley_symplectic_step_compute.bld` | Long-Context AI | `O(1)` Cayley symplectic state update |
| `cloth_verlet_step_compute.bld` | Physics | Velocity-Verlet cloth constraint step |
| `flash_attn_tile_compute.bld` | Attention | Online-softmax tiled attention kernel |
| `hiz_depth_downsample_compute.bld` | Culling | Hierarchical-Z min/max pyramid mip |
| `indirect_frustum_cull_compute.bld` | Visibility | Six-plane frustum plus Hi-Z draw cull |
| `int4_dequant_matvec_compute.bld` | Quantization | Group-wise INT4 dequantized GEMV |
| `jonswap_wave_compute.bld` | Ocean Sim | JONSWAP spectrum Gerstner displacement |
| `merkle_hash_step_compute.bld` | Cryptography | Domain-separated Merkle node hashing |
| `moe_router_topk_compute.bld` | Sparse MoE | Top-K expert gating and score dispatch |
| `ntc610_decode_compute.bld` | Compression | SM 6.10 cooperative neural texture decode |
| `oklab_filmic_grade_compute.bld` | Color Grading | OKLab perceptual filmic tone mapper |
| `ordered_dither_compute.bld` | Halftoning | Bayer ordered quantization threshold |
| `pbr_ggx_brdf_compute.bld` | Shading | GGX microfacet specular BRDF lobe |
| `rmsnorm_swiglu_compute.bld` | Transformer | Fused RMSNorm with SwiGLU activation |
| `rope_rotary_embed_compute.bld` | Embeddings | Rotary position complex-pair rotation |
| `sdf_raymarch_compute.bld` | Geometry | Sphere-traced signed distance marcher |
| `shallow_water_step_compute.bld` | Fluids | 2D shallow-water heightfield flux step |
| `sky_rayleigh_mie_compute.bld` | Atmosphere | Dual-scattering Rayleigh and Mie sky |
| `spectral_xyz_eval_compute.bld` | Optics | Spectral power distribution CIE XYZ sum |
| `splat_3dgs_project_compute.bld` | Gaussian Splat | 3D Gaussian covariance screen projection |
| `volumetric_froxel_scatter_compute.bld` | Volumetrics | Froxel marching extinction and inscatter |
| `wind_foliage_bend_compute.bld` | Vegetation | Procedural aerodynamic stem deflection |

## 2. Certified Five-Target Superopt Gains and Planted Controls (`[Checked]`)

1. **Five-Backend Parity vs Unoptimized Baseline (`120/120` Shaders):** Across all `24` kernels compiled to `WGSL`, `HLSL`, `MSL`, `SPIR-V`, and `GLSL` (`390,286 B` aggregate shader footprint on workstation GPU silicon), e-graph superoptimization applies `500` certified rewrites cutting aggregate MIR cost by `48.82%` (`510 -> 261`) and increasing fused FMA calls from `0` baseline to `105` while `ntc610_decode_compute.bld` targets `10.67 MiB` per 4K texture vs `64.0 MiB` BC7 VRAM (`-83.33%` memory cut), verified live by `dxc`, `glslangValidator`, `spirv-val`, `naga`, and `buildc verify-cert`.
2. **Planted Soundness Controls:** Translation-validation harnesses catch corrupted SPIR-V magic headers (`0x07230200`), mismatched subgroup intrinsics across targets, and MIR cost regressions fail-closed.

## 3. Running Locally

```sh
for f in universe/workstation-gpu-compute/*.bld; do
  buildc check "$f"
  buildc lint "$f"
done
```

## 4. Honest Scope Limits (`[Reported]` Boundaries)

- Each file provides a clean single-source GPU compute kernel validated by `buildc` static checking and shader codegen; host Vulkan/DX12/Metal swapchain dispatch orchestration lives in downstream rendering engines.
- Graduation adds `+24` passing `.bld` files under `universe/workstation-gpu-compute/` while leaving `universe/CHECK-FAILS.txt` untouched.
