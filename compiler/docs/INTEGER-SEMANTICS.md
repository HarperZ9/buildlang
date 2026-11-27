# BuildLang Cross-Backend Integer Semantics

## Unified Language Rule

BuildLang enforces one consistent integer overflow specification across all CPU and GPU targets (`C`, `HLSL`, `GLSL`, `WGSL`, `MSL`, and `SPIR-V`):

1. **Plain integer `+`, `-`, and `*` are checked (non-overflowing) on every target.**
   - **CPU (`Target::C`):** Plain integer arithmetic lowers to checked runtime helpers (`bl_iadd_*`, `bl_uadd_*`, `bl_isub_*`, `bl_usub_*`, `bl_imul_*`, `bl_umul_*`) that trap fail-closed with exit code `101` and an explicit overflow message. Compile-time literal folds also emit type-check diagnostics.
   - **GPU (`HLSL`, `GLSL`, `WGSL`, `MSL`, `SPIR-V`):** GPU compute lanes cannot raise a host panic or process exit code. Before GPU backend emission, `codegen::analysis::int_overflow::check_gpu_int_overflow` audits MIR and rejects fail-closed at compile time (`CodegenError::Rejected`) any plain integer `+`, `-`, or `*` that either overflows on statically known constants or operates on unguarded integer data (values loaded from integer storage buffers, bitwise-mixed data, or integer data outputs). Guarded loop-counter increments (`while step < max_steps { step = step + 1; }`), identity no-ops (`x + 0`, `x * 1`), and dispatch-coordinate buffer address calculations (`i * k + kk`) remain accepted.

2. **Explicit `.wrapping_add`, `.wrapping_sub`, and `.wrapping_mul` lower to native modular `mod 2^W` arithmetic on every target.**
   - Every target lowers `BinOp::AddWrapping`, `BinOp::SubWrapping`, and `BinOp::MulWrapping` to two's-complement wrap-around without undefined behavior or traps:
     - `C`: `bl_uadd_wrapping_*` / `bl_iadd_wrapping_*` (unsigned cast wrap, zero C99 signed overflow UB).
     - `HLSL`: native `+`, `-`, `*` on `uint`; `asint(asuint(a) op asuint(b))` on `int`.
     - `GLSL`: native `+`, `-`, `*` on `uint` and `int` (GLSL 450 section 5.9 defines wrapping two's-complement arithmetic).
     - `WGSL`: native `+`, `-`, `*` on `u32` and `i32` (WGSL section 8.6 defines wrapping two's-complement arithmetic).
     - `MSL`: native `+`, `-`, `*` on `uint`; `static_cast<int>(static_cast<uint>(a) op static_cast<uint>(b))` on `int` (zero Metal C++14 signed overflow UB).
     - `SPIR-V`: `OpIAdd`, `OpISub`, `OpIMul` (SPIR-V section 3.42.13 defines exact low-order wrapping arithmetic).

## Migration Guidance for Cryptographic and Bitwise GPU Kernels

Algorithms that intentionally rely on 32-bit modular wrap-around (such as FIPS 180-4 SHA-256, ChaCha20, or FNV-1a) must write explicit `.wrapping_add(...)`, `.wrapping_sub(...)`, or `.wrapping_mul(...)` calls instead of plain `+`, `-`, `*`. The exact same source compiles cleanly and produces bit-identical results across CPU `C` and all five GPU targets.

## Scope Limits

- Compile-time GPU overflow analysis uses sound local constant propagation, loop-bound guard recognition, and data-origin taint tracking rather than full SMT range proofs over arbitrary buffer dimensions; address expressions derived solely from dispatch coordinates and uniform bounds (such as `i * k + kk` inside `if i < m && j < n`) are treated as valid index arithmetic.
- Checked integer division and remainder (`/` and `%`) trap on zero divisors on CPU; GPU division by zero remains guarded by shader-level `if denom == 0` checks.
