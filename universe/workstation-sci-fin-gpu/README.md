# Workstation Scientific-Bio-Audio and Finance-Graph-Optics-FFT GPU Kernels

This bundle graduates eight pure-MIT BuildLang `#[compute]` GPU kernels compiled and certified across WGSL, HLSL, MSL, SPIR-V, and GLSL with `buildc` (`strict-bit-exact` subgroup validation). Every source passes `buildc check`, `buildc lint`, and `buildc check-dir` with zero warnings and emits verified `buildlang.gpu-tv-cert/v1` certificates across five shader backends.

| Kernel File | Domain | Physical Scope |
| :- | :- | :- |
| `sci_fdtd_maxwell_yee_compute.bld` | Electromagnetics | 2D TMz Maxwell Yee FDTD curl step with conductive damping and field energy |
| `sci_reaction_diffusion_gs_compute.bld` | Morphogenesis | Gray-Scott 5-point Laplacian reaction-diffusion step clamped to `[0, 1]` |
| `sci_fdn_reverb_dsp_compute.bld` | Audio DSP | 4-channel Householder feedback delay network reverb with lowpass damping |
| `sci_molecular_lj_verlet_compute.bld` | Molecular Dynamics | Lennard-Jones 12-6 pairwise force with symplectic velocity-Verlet update |
| `fin_heston_montecarlo_compute.bld` | Quantitative Finance | Heston full-truncation Milstein path step and discounted call payoff |
| `graph_pagerank_spmv_compute.bld` | Graph Analytics | Damped PageRank 4-neighbor gather power iteration with L1 residual |
| `optics_thinfilm_airy_compute.bld` | Physical Optics | RGB Airy thin-film interference reflectance bounded in `[0, 1]` |
| `dsp_fft_radix2_butterfly_compute.bld` | Spectral DSP | Cooley-Tukey radix-2 complex FFT butterfly with Parseval energy check |

Four scientific, bio, and audio kernels originate from the verified 180-shader pack (`wave124d`), and four finance, graph, optics, and FFT kernels complete the bicentennial 200-shader pack (`wave127a`). All eight files share pure-OSI MIT licensing (`LICENSE`) with zero external dependencies, zero secrets, and zero local paths.
