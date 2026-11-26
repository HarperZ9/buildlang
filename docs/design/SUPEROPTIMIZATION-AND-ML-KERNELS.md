# Certified Superoptimization (`V5`), Hardware-Optimal Scheduling, and Differentiable ML Kernels (`V6`)

Status: design specification with live runnable `V5` prototype (`examples/v5_egraph_superopt_cert.bld`).
Written: 2026-11-26.
Companions: `docs/STRATEGY-2026-10-SURPASS.md` (`P2`, `P3`, `P4`, `V1`, `V4`, `V5`, `V6`, `S4`), `docs/design/BUILDDB-BUILDQL.md`, and `universe/workstation/wave15_online_rkl_noh4_oracle.bld`.

---

## 1. Thesis: How BuildLang Matches and Surpasses CUDA, Triton, and PyTorch

Modern ML and physics stacks force an unnecessary split between three broken worlds:

1. **PyTorch / Python eager + torch.compile:** Fast to write, opaque in memory layout, fragile under dynamic shapes, and impossible to certify bit-exact across backends.
2. **Triton / tile DSLs:** Productive for NVIDIA and AMD GPU block kernels, yet lacking static effect/aliasing proofs, CPU/mobile/WebGPU portability, and machine-checkable rewrite certificates.
3. **Hand-tuned CUDA / C++23 / intrinsics:** Maximum hardware control, locked to a single vendor ABI, vulnerable to undefined behavior (`__restrict__` aliasing lies, silent `-ffast-math` drift, out-of-bounds shared-memory indexing), and unverified after compiler passes.

BuildLang closes this split with a single effect-typed, ownership-checked systems language compiled through three interlocking stages:

- **Stage A (`V5` Certified E-Graph Superoptimization):** Algebraic, strength-reduction, FMA, fused-reduction, and loop-tiling rewrites performed in a bounded equality-saturation graph where **every rewrite emits an independent equivalence certificate** re-checked without trusting the optimizer.
- **Stage B (Hardware-Optimal Instruction & Memory Scheduling):** Effect-row-proven purity (`pure`) and linear/unique buffer ownership unlock aggressive auto-vectorization (`AVX-512`, `NEON/SVE`, `RVV 1.0`), register/shared-memory tiling, warp/subgroup reductions, and tensor/matrix-core lowering across `CUDA/PTX`, `SPIR-V`, `HLSL SM 6.9/6.10`, `Metal (msl)`, and `WebGPU (wgsl)`.
- **Stage C (`V6` Differentiable Effects + Fused ML Kernels):** Reverse-mode automatic differentiation over effect-typed pure tensor functions, lowering directly into fused Online-Softmax, FlashAttention, and quantized (`Q4_K_M` / 1.58-bit ternary) kernels with bit-exact or ULP-bounded behavioral equivalence receipts against PyTorch/CUDA references.

---

## 2. Stage A: `V5` Certified E-Graph Superoptimization

### 2.1 Representation Over BuildLang MIR
Equality saturation operates on pure SSA expression DAGs and structured loop nests inside BuildLang MIR (`buildc mir`):

- **E-Nodes & E-Classes:** Expressions are hash-consed into a compact arena (`ops`, operand IDs, literal payloads, union-find e-classes). Congruence closure guarantees that whenever `a == b`, `f(a) == f(b)` merges automatically.
- **Bounded Deterministic Saturation:** Saturation runs with explicit caps (`max_iters`, `max_nodes`, `max_class_size`) and deterministic tie-breaking by canonical node ID so `buildc build-hash` remains byte-identical across repeated runs and machines.
- **Effect & Floating-Point Discipline:** Rewrites NEVER cross impure effect boundaries (`IO`, `Console`, `Network`, mutable shared state). Floating-point rewrites carry an explicit IEEE-754 soundness tag:
  - `strict_ieee754`: bit-exact for all finite inputs (e.g., `x * 1.0 -> x`, `x * 2.0 -> x + x`, `x - x -> 0.0` under non-NaN proof).
  - `certified_exact_domain`: bit-exact on proven finite/dyadic/integer intervals discharged by `S4` abstract interpretation (e.g., `(x * 3.0) + (x * 5.0) -> x * 8.0` and exact FMA fusion).
  - `bounded_ulp(k)`: explicitly declared ULP envelope (e.g., reciprocal approximation or FMA rounding shift), permitted ONLY when the function's contract declares the tolerance. Silent `-ffast-math` reassociation is forbidden.

### 2.2 Certified Rewrite Rule Families

| Family | Representative Rules | Hardware / ML Payoff | Soundness Condition |
| :--- | :--- | :--- | :--- |
| **Algebraic & Identity** | `x * 1 -> x`, `x + 0 -> x`, `x * 0 -> 0`, `x - x -> 0`, `exp(log(x)) -> x` | Eliminates dead affine/normalization ops emitted by generic tensor or shader code. | Finite non-NaN (`S4` interval/finite domain) + `x > 0` for `log`. |
| **Strength Reduction** | `x * 2.0 -> x + x`, `x * 2^k -> x << k` (exact ints), `x / 2^k -> x * 2^{-k}` (dyadic float) | Replaces high-latency multiply/divide pipelines with single-cycle add/shift/exponent ops. | Exact IEEE-754 power-of-two scaling or bitwidth non-overflow (`V1`). |
| **Distributive Factoring & Constant Folding** | `(x * c1) + (x * c2) -> x * (c1 + c2)`, hoist loop-invariant scale factors | Collapses multi-term linear combinations, polynomial evaluations, and affine layer biases. | Exact dyadic/integer domain or declared ULP contract. |
| **FMA & Fused Reductions** | `(a * b) + c -> fma(a, b, c)`, 2-pass softmax+KL -> 1-pass associative 5-scalar monoid `(mq, lq, mp, lp, s_acc)` | Cuts memory bandwidth by 2x to 3x and maps directly to single-cycle FMA / warp reductions. | Declared FMA contract; associative monoid proof for online-softmax merge. |
| **Loop Tiling & Fusion** | `map(g, map(f, xs)) -> map(g . f, xs)`, 2D/3D strip-mine + interchange (`BM x BN x BK`) | Eliminates intermediate HBM/RAM allocations and keeps working tiles inside L1 / SRAM. | Pure effect row + disjoint buffer ownership proof (`&mut` non-aliasing). |

### 2.3 Equivalence Certificate Schema (`buildlang.v5-egraph-cert/v1`)
The e-graph search engine is **untrusted**. Extraction emits a compact, ordered proof chain stored in the build receipt (and queryable via `buildc bql`):

```json
{
  "schema": "buildlang.v5-egraph-cert/v1",
  "function": "affine_inner_loop",
  "ieee754_mode": "certified_exact_domain",
  "orig_cost": 17,
  "opt_cost": 3,
  "orig_mir_sha256": "sha256:...",
  "opt_mir_sha256": "sha256:...",
  "steps": [
    { "step": 1, "rule_id": "distrib_fold", "before": "(x * 3.0) + (x * 5.0)", "after": "x * 8.0" },
    { "step": 2, "rule_id": "mul_one", "before": "((x * 3.0) + (x * 5.0)) * 1.0", "after": "(x * 3.0) + (x * 5.0)" },
    { "step": 3, "rule_id": "sub_self", "before": "z - z", "after": "0.0" },
    { "step": 4, "rule_id": "mul_two_add", "before": "y * 2.0", "after": "y + y" },
    { "step": 5, "rule_id": "add_zero", "before": "(y + y) + 0.0", "after": "y + y" },
    { "step": 6, "rule_id": "add_zero", "before": "rhs_inner + 0.0", "after": "rhs_inner" },
    { "step": 7, "rule_id": "fma_fuse", "before": "(x * 8.0) + (y + y)", "after": "fma(x, 8.0, y + y)" }
  ]
}
```

### 2.4 Independent Checker & Forgery Rejection (`buildc tv` / `buildc tv-check`)
An independent verifier re-checks every step directly from fixed structural axioms without reading union-find internals:
1. Confirms each `rule_id` belongs to the signed axiom table and matches `(before, after)` structurally.
2. Confirms every side condition (`finite`, non-zero divisor, exact power-of-two, `pure` effect) holds.
3. Rejects any forged rewrite (such as planting `x * 2.0 -> x + 3.0` under `rule_mul_two_add`) with a hard failure.
4. Pairs structural proof checking with `V4` translation validation on the lowered C/LLVM/PTX/SPIR-V output so verification survives code generation all the way to machine instructions.

### 2.5 Runnable Prototype Verification (`examples/v5_egraph_superopt_cert.bld`)
A pure BuildLang prototype implementing union-find e-classes, cost-based extraction, step-by-step equivalence certificate emission, an independent structural verifier, a 64-point frozen-input bit-exact oracle, a planted unsound rewrite control (`x * 2.0 -> x + 3.0`), and a 4,000,000-iteration compiled inner-loop benchmark ships in `examples/v5_egraph_superopt_cert.bld`.

**Observed verification run (`buildc 1.15.0`, 2026-11-26):**
- `buildc check examples/v5_egraph_superopt_cert.bld`: `0` errors (`4,115` tokens, `31` items).
- `buildc lint examples/v5_egraph_superopt_cert.bld`: `0` errors, `0` warnings.
- Cost reduction: `orig_cost = 17` -> `opt_cost = 3` (**5.67x** cost reduction across `7` independently verified steps).
- Frozen-input parity (64 test points) + 4M-iteration benchmark parity: `max_err = 0.0`, `bench_err = 0.0` (bit-exact).
- Planted unsound rewrite (`x * 2.0 -> x + 3.0`): **REJECTED** (`forged_rejected = true`) by both the independent axiom checker and the semantic oracle.
- Throughput on 4,000,000 iterations: `3.80x` speedup at `-O 0` (`70.06 ms -> 18.46 ms`) and `2.59x` speedup at `-O 3` (`49.54 ms -> 19.10 ms`, proving certified e-graph factoring outperforms `-O 3` alone when strict IEEE-754 prevents uncertified reassociation).

---

## 3. Stage B: Hardware-Optimal Instruction & Memory Scheduling

### 3.1 Why Effect Rows + Linear Ownership Unlock Faster Code Than C++/CUDA
Standard C++ and CUDA optimizers leave 20% to 60% of peak hardware throughput on the table because pointer aliasing and hidden side effects force conservative memory barriers and scalar fallback loops. In BuildLang:

- **`pure` Effect Row:** Guarantees a function performs zero global state mutation, zero I/O, and zero hidden locks. Any loop body with an empty effect row is provably safe to vectorize, unroll, tile, and hoist.
- **Linear / Unique Ownership (`&mut` Disjointness):** Distinct mutable slice arguments cannot alias. The scheduler emits hardware `noalias` / `restrict` proofs *backed by the type checker* rather than unchecked programmer assertions.
- **Deterministic Associative Reductions:** When a `par_for` reduction supplies an explicit associative monoid (as in `universe/workstation/wave15_online_rkl_noh4_oracle.bld`'s 5-scalar `rkl_merge`), the scheduler maps the exact same tree topology across CPU threads and GPU warps, preserving **`0.0` thread-count drift** (`2.37 - 3.74 ms` on `151,936`-vocab Qwen2.5 procedural logits vs `59.97 ms` Python, `16x - 25.3x` faster).

### 3.2 CPU Vectorization & Cache Scheduling (`x86-64`, `arm64`, `riscv64`)
1. **SIMD Lane Selection:** Lower pure inner loops into explicit packed vector MIR (`Vec4f64`, `Vec8f32`, `Vec16f32`) mapped to:
   - `x86-64`: AVX2 / AVX-512 FMA (`vfmadd231pd` / `vfmadd231ps`), masked tail loops with zero scalar peel stalls.
   - `arm64`: NEON / SVE predicated vector instructions (`fmla`).
   - `riscv64`: RVV 1.0 vector-length-agnostic (`vsetvli` + `vfmacc.vv`) aligned with `--target riscv64`.
2. **Cache-Aware Register & L1/L2 Blocking:** Tile dense reductions and stencils (`MC x KC x NC` micro-kernels) using hardware topology descriptors (planned `topology` table in BuildDB `H7`) so inner accumulators stay 100% live in vector registers.

### 3.3 GPU Memory Hierarchy & Multi-Target Lowering
One effect-checked `#[compute]` kernel lowers cleanly across all five GPU backends (`CUDA/PTX`, `SPIR-V`, `HLSL SM 6.9/6.10`, `Metal MSL`, and `WebGPU WGSL`):

| Hardware Mechanism | CUDA / PTX | Vulkan SPIR-V | DX12 HLSL SM 6.9/6.10 | Metal (`msl`) | WebGPU (`wgsl`) |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Shared / Tile SRAM** | `__shared__` + `+1` bank-pad | `Workgroup` storage class | `groupshared` | `threadgroup` | `var<workgroup>` |
| **Warp / Subgroup Reduce** | `__shfl_xor_sync` | `OpGroupNonUniformFAdd` | `WaveActiveSum` | `simd_sum` | `subgroupAdd` |
| **Async Tile Copy / Double Buffer** | `cp.async` / TMA | `KHR_cooperative_matrix` / staged loads | Root-descriptor staged tile loads | `threadgroup` staged loads | `workgroupBarrier` double-buffer |
| **Matrix / Tensor Cores** | `mma.sync` / WGMMA (`fp16`/`bf16`/`fp8`) | `OpCooperativeMatrixMulAddKHR` | `WaveMatrix` (SM 6.9+) | `simdgroup_matrix_multiply_accumulate` | Register-tiled FMA fallback |
| **Bounds & Safety Proof (`V1`)** | Elided when `S4` proves tile indices in-bounds | Same static elision | Same static elision | Same static elision | Elided or trapped per WGSL spec |

**Key Architectural Win Over Triton & CUDA:**
- Compared to **CUDA**: single `.bld` source compiles to NVIDIA, AMD, Apple Silicon, Intel, and browser WebGPU with vendor validator gates (`DXC`, `spirv-val`, `glslang`, `naga`) already wired into CI.
- Compared to **Triton**: zero Python JIT runtime dependency, static `V1` bounds/crash certificates, `V5` certified algebraic/tiling rewrites, and seamless CPU/GPU parity testing (`P4`).

---

## 4. Stage C: `V6` Differentiable Effects + Fused ML Kernels

### 4.1 Effect-Typed Reverse-Mode Automatic Differentiation
Rather than PyTorch's runtime dynamic tape (which allocates Python objects per op and silently breaks when user code performs in-place mutation or non-differentiable control flow), BuildLang implements compile-time source-to-source reverse-mode AD directly on MIR:

1. **Effect Gate:** `grad(f)` accepts ONLY functions whose effect row is pure (or restricted to `Tensor` / `Grad` effects). Attempting to differentiate through `IO`, `Random` without an explicit seed, or aliased mutation fails at `buildc check` with a clear `BL` diagnostic.
2. **Static Primal / Adjoint Fusion:** Because the entire forward pass and backward pullback live in the same MIR module, Stage A's `V5` e-graph runs **across the forward-backward boundary**, fusing recomputed activations, cancelling redundant `exp`/`log` pairs, and eliminating dead gradient buffers before code generation.
3. **Compile-Time Activation Checkpointing:** Linear tape ownership lets the compiler schedule exact rematerialization boundaries to fit a declared VRAM budget (e.g., 24 GB RTX 4090) without OOM guesswork.

### 4.2 Target Flagship Fused ML Kernels

1. **Fused 1-Pass Online-Softmax & Reverse-KL (`RLVR` / Distillation Loss):**
   - Extends `universe/workstation/wave15_online_rkl_noh4_oracle.bld` from CPU `par_for` to GPU warp/block reductions.
   - Materializes **zero** `[batch, seq, 151936]` intermediate probability tensors in HBM (saving gigabytes per step during 7B/14B RLVR post-training) while matching exact 2-pass `f64` reference KL to `< 1e-13` (`f64`) / declared `bf16` ULP tolerance.
2. **Fused FlashAttention & Paged KV-Cache Attention:**
   - Single-pass QK^T -> online-softmax -> PV block kernel keeping Q/O tiles in registers and K/V tiles in double-buffered SRAM.
   - Static `S4` interval proofs discharge page-table index bounds (`V1`), eliminating runtime bounds traps inside the innermost dot-product loop.
3. **Fused Quantized GEMV / GEMM (`Q4_K_M` and 1.58-bit Ternary):**
   - Dequantizes packed 4-bit block scales (`Q4_K_M`, matching local `4.68 GB` `JFC3P-7B-Oracle-v1` inference) or 2-bit ternary weights (`{-1, 0, +1}`) **inside registers directly ahead of FMA / matrix-core instructions**, avoiding dequantized FP16 round-trips to VRAM.
   - Ships with a `V6` equivalence receipt verifying per-step output parity against PyTorch/CUDA and `llama.cpp` reference tensors on identical weights and seeds.

---

## 5. Phased Implementation Roadmap & Falsification Gates

| Phase | Concrete Deliverable | Cheapest Falsifying Test / CI Gate |
| :--- | :--- | :--- |
| **Phase 1 (Landed Prototype)** | `examples/v5_egraph_superopt_cert.bld` + this design spec | `buildc check`, `buildc lint`, `buildc run` PASS; `0.0` frozen-input error; planted forgery `x * 2 -> x + 3` rejected; measurable `-O 3` speedup. |
| **Phase 2 (`V5` Compiler Pass)** | `compiler/src/mir/egraph.rs` + `buildlang.v5-egraph-cert/v1` integrated into `buildc -O 2` and `buildc tv-check` | Every function in `tests/programs/` passes `buildc tv-check`; mutation harness injects 10 forged certificates and `10/10` are refused. |
| **Phase 3 (Stage B GPU/SIMD Scheduler)** | Effect-proven `noalias` + subgroup/register tiling across `x86-64`, `arm64`, `riscv64`, `SPIR-V`, `HLSL`, `MSL`, `WGSL` | Vendor validators (`DXC`, `spirv-val`, `naga`) accept all emitted shaders; CPU vs GPU parity (`P4`) matches within declared ULP bound on lavapipe and native GPU. |
| **Phase 4 (`V6` AD + Fused Kernels)** | `grad()` MIR pass + fused Online-Softmax Reverse-KL, FlashAttention, and `Q4_K_M` GEMV kernels | Head-to-head dated benchmark vs PyTorch 2.x + Triton + CUDA on identical GPU hardware, weights, and data order; loss/logit delta inside declared tolerance per step. |

---

## 6. Honest Scope Boundary (`Does not prove`)

Per `docs/STRATEGY-2026-10-SURPASS.md`: writing this blueprint and shipping the pure-BuildLang `V5` e-graph prototype **does not prove** throughput superiority over CUDA, Triton, or PyTorch today. The prototype proves certified e-graph cost reduction (`17 -> 3`), independent forgery rejection, bit-exact parity (`0.0`), and a `2.59x - 3.80x` inner-loop speedup inside compiled BuildLang. Any parity or lead claim against CUDA, Triton, or PyTorch requires a dated, receipt-sealed head-to-head benchmark on shared GPU hardware after Phases 2 to 4 land.
