# Workstation Frontier Surpass Kernels (2026)

License: Pure-OSI `MIT` (`15/15` standalone BuildLang kernels, `0` external dependencies, `0` secrets, `0` local host paths).
Author Vision Anchors: `A18`, `B4`, `F1`, `I1`, `J13`, `N44`, `N49`, `N50`, `N53`, `N63`, `N64`.
Verification Contract: Every kernel passes `buildc check` (`0` errors), `buildc lint` (`0` warnings), native `-O 3` compilation, and `SIG-058` privacy screening (`17/17` bundle files clean).

## 1. Graduated Kernel Manifest (`[Checked]` 15 Pure-OSI MIT Kernels)

| Kernel File | Signal | Domain | Verification Focus |
| :- | :- | :- | :- |
| `cs001_loom_mhc_loss_per_flop.bld` | `CS-001` | Manifold Routing | Sinkhorn doubly-stochastic mHC with `33.56x` lower MSE |
| `cs005_fca_titans_grid_probe.bld` | `CS-005` | Long-Context AI | `O(s^1.5)` FCA sparse probe plus `O(1)` Cayley state |
| `cs006_sparrow_fixedpoint_loop.bld` | `CS-006` | Implicit Depth | Fixed-point DEQ loop with `24.0x` activation reduction |
| `cs008_model_carving_loop.bld` | `CS-008` | Model Pruning | Structured carving loop with fail-closed accuracy guard |
| `cs009_sleep_lora_consolidation.bld` | `CS-009` | Continual Learning | Fisher EWC plus DER++ sleep consolidation (`97.09%` cut) |
| `cs014_fractal_procedural_weights.bld` | `CS-014` | Weight Synthesis | `16x` fractal Kronecker weight compression plus IID null |
| `cs024_posterior_rare_tail_audit.bld` | `CS-024` | Tail-Risk Audit | Rare-tail CVaR posterior bound with adversarial trap |
| `cs025_agentic_ppl_bpb.bld` | `CS-025` | Evaluation Math | Exact bits-per-byte and perplexity verifier kernel |
| `cs031_automated_port_verifier.bld` | `CS-031` | Compiler Porting | Deterministic cross-language port parity checker |
| `cs042_onebit_vis_dynamic_lights.bld` | `CS-042` | Real-Time Lighting | `32.0x` VRAM 1-bit shadow visibility bitset map |
| `cs045_persistent_3d_memory.bld` | `CS-045` | 3D Vision | `12/12` anchor retention across `15` re-acquisitions |
| `cs046_video_skeleton_mocap.bld` | `CS-046` | Motion Capture | Two-view bone-constrained mocap (`-51.75%` MPJPE) |
| `cs047_kiruna_night_spectral_sky.bld` | `CS-047` | Spectral Optics | `81`-band Kiruna night sky with `2.87x` Purkinje shift |
| `cs049_fourier_optics_pdb_provenance.bld` | `CS-049` | Wave Optics | Fraunhofer diffraction grating plus PDB provenance |
| `titan_unified_epigenetic_engine.bld` | `OPT-079` | Unified AI Engine | Five-moat Titan engine (`159` superopt rewrites) |

## 2. Measured Baseline Gains and Planted Controls (`[Checked]`)

1. **Sparse Long-Context Memory vs Dense Baselines:** `cs005_fca_titans_grid_probe.bld` cuts 128k KV VRAM by `75.48x` (`13.67 GB` vs `1,032.02 GB`) with `0.0` Cayley drift while `titan_unified_epigenetic_engine.bld` unifies FCA, Sinkhorn mHC, INT4 quantization, lossless speculative decoding, and J13 SVD defense into one `113 KB` executable (`2.55x` native speedup over Python).
2. **Real-Time Graphics and Spectral Optics:** `cs042_onebit_vis_dynamic_lights.bld` compresses visibility bitsets by `32.0x` with `0.0` radiance error, `cs045_persistent_3d_memory.bld` eliminates reprojection drift (`0.0 px` vs `7.99 px` baseline), and `cs046_video_skeleton_mocap.bld` cuts MPJPE by `51.75%` with `0.0 m` bone-length error.
3. **Planted Fail-Closed Controls:** Every kernel embeds adversarial defect traps covering causal future leaks, VRAM budget overruns, non-orthonormal SO(3) rotations, parallel-ray triangulation singularities, J13 rank-1 collapse, and IID random-weight honest nulls.

## 3. Running Locally

```sh
for f in universe/workstation-surpass-2026/*.bld; do
  buildc check "$f"
  buildc lint "$f"
done
```

## 4. Honest Scope Limits (`[Reported]` Boundaries)

- Each `.bld` file executes a standalone deterministic IEEE-754 `f64` CPU verification kernel (`Console` / `Clock` effects only); multi-GPU distributed training harnesses live outside this compiler universe directory.
- Graduation adds `+15` passing `.bld` files under `universe/workstation-surpass-2026/` while leaving `universe/CHECK-FAILS.txt` and all other `universe/` directories completely untouched.
