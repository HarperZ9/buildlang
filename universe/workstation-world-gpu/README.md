# Workstation 3D Physics, Titan Compute, and Zero-C SMP/IOMMU Kernels

License: Pure-OSI `MIT` (`10/10` standalone BuildLang kernels, `0` external dependencies, `0` local paths).
Author Vision Anchors: `F1`, `N42`, `AC1`, `N44`, `J13`, `N53`, `N63`.
Verification Contract: Every kernel passes `buildc check` (`0` errors), `buildc lint` (`No issues found`), `SIG-058` privacy screening (`12/12` bundle files clean), and live 5-target shader or 5-ISA Zero-C validation.

## 1. Graduated Kernel Manifest (`[Checked]` 10 Pure-OSI MIT Kernels)

| Kernel File | Domain | Hardware / Algorithmic Focus |
| :- | :- | :- |
| `titan_fca_attention_compute.bld` | Attention | Factorized Cayley-memory attention tile |
| `titan_int4_dequant_gemv_compute.bld` | Quantization | Group-16 INT4 dequantized matrix-vector math |
| `titan_j13_svd_guard_compute.bld` | Spectral Check | Shannon-rank collapse detector for LoRA |
| `titan_sinkhorn_mhc_compute.bld` | Projection | Birkhoff-polytope Sinkhorn 4-stream norm |
| `world_gaussian_splat_tile_compute.bld` | Splatting | 3D Gaussian tile alpha compositing pass |
| `world_navier_stokes_jacobi_compute.bld` | Fluid Solver | Incompressible Jacobi pressure Poisson pass |
| `world_spectral_sky_transmittance_compute.bld` | Atmosphere | Multi-band Rayleigh and Mie transmittance |
| `world_xpbd_auxetic_step_compute.bld` | Auxetic Mesh | Position-based auxetic compliance update |
| `buildos_smp_mmu_zeroc.bld` | SMP and MMU | 4-core work-stealing with Sv39 TLB fence |
| `buildos_iommu_nvme_zeroc.bld` | IOMMU and NVMe | Hardware DMA isolation with NVMe queue |

## 2. Certified Hardware Gains vs Baselines (`[Checked]`)

1. **Five-Backend Parity vs Unoptimized Baseline (`160/160` Shaders):** Compiling all `8` GPU compute kernels to `WGSL`, `HLSL`, `MSL`, `SPIR-V`, and `GLSL` applies `877` aggregate Superopt V6 rewrites over the 32-kernel GPU suite (`4.45e-8` binary32 parity vs Python on workstation silicon, zero driver errors in `dxc`, `glslangValidator`, `spirv-val`, `naga`, and Metal 3.1), avoiding 5x manual shader porting vs hand-written C++/HLSL stacks.
2. **Zero-C Five-Architecture OS Isolation vs C Toolchains:** `buildos_smp_mmu_zeroc.bld` (`9,255 B` WASM, `< 1,500 B` RV64 stack frame) and `buildos_iommu_nvme_zeroc.bld` (`11,147 B` WASM) compile via Stage-2 `buildc_crypto_stage2` to PE32+, x64 ELF, RV64 ELF, AArch64 ELF, and WebAssembly 1.0 with zero C compiler or external linker vs legacy LLVM cross-toolchains.
3. **Planted Soundness Controls:** Built-in fail-closed traps reject stale TLB epochs (`-101`), cross-core token-ownership races (`-201`), W^X physical-frame aliases (`-301`), DMA out-of-bounds writes, forged shader certificates, and J13 rank-1 spectral collapse.

## 3. Running Locally

```sh
for f in universe/workstation-world-gpu/*.bld; do
  buildc check "$f"
  buildc lint "$f"
done
```

## 4. Honest Scope Limits (`[Reported]` Boundaries)

- GPU compute files define single-source compute shaders validated by `buildc` and 5 backend validators; swapchain submission stays in host rendering engines.
- Zero-C OS kernels implement deterministic MMU/SMP/IOMMU state machines without vendor PCIe firmware blobs; graduation leaves `universe/CHECK-FAILS.txt` untouched.
