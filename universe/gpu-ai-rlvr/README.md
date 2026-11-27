# Titan AI and RLVR Five-Backend GPU Compute Kernels

This bundle graduates eight pure-MIT BuildLang `#[compute]` GPU kernels compiled across WGSL, HLSL, MSL, SPIR-V, and GLSL using `buildc` under `strict-bit-exact` subgroup validation. Every source file passes `buildc check` plus `buildc lint` inside `buildc check-dir` cleanly while emitting verified `buildlang.gpu-tv-cert/v1` certificates across five shader targets.

| Kernel File | Workload | Compute Scope |
| :- | :- | :- |
| `flash_attention_v3_tiled.bld` | Attention | Online-softmax causal tiled FlashAttention-3 update step with running max rescaling |
| `rmsnorm_swiglu_fused.bld` | MLP | Fused four-feature Newton-Raphson RMSNorm plus rational SiLU-gated SwiGLU residual block |
| `int4_awq_dequant_gemv.bld` | Quantization | Group-128 activation-aware INT4 dequantization with salient scaling plus four-lane GEMV |
| `moe_topk2_capacity_router.bld` | Sparse MoE | Four-expert top-2 load-balanced capacity router with deterministic overflow throttling |
| `mamba2_ssd_chunk_scan.bld` | State Space | Two-step Mamba-2 State-Space Dual selective chunk scan with contractive decay |
| `grpo_advantage_clip_loss.bld` | RLVR Policy | Four-rollout GRPO advantage normalization with PPO ratio clipping plus Schulman KL penalty |
| `speculative_tree_verify_gpu.bld` | Speculative Decoding | Parallel Leviathan-Chen tree token verifier with parent-gated acceptance flags |
| `h2o_heavy_hitter_evict_gpu.bld` | KV Eviction | Heavy-Hitter KV-cache eviction scorer combining EMA attention mass with recency protection |

All eight kernels originate from the verified 240-shader GPU compute pack (`wave134a`). Each file carries pure-OSI MIT licensing (`LICENSE`) with self-contained compute math and zero external dependencies.
