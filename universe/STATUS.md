# BUILD-UNIVERSE — Module Maturity Ledger (Canonical)

Last verified: 2026-06-05. This file is the single source of truth for module
reality. Where README, ENGINEERING, CHANGELOG, UNIVERSE.toml, or CATALOG.json
disagree, this file wins. Scores are engineering concreteness (0-10) from direct
source audit: real, correct, extractable logic vs scaffolding/showcase.

## Canonical facts

- Version: 1.0.0  |  License: MIT (see LICENSE)
- Compiler: 612 tests pass / 0 fail on cargo test (755 #[test] annotations incl. ignored/multi-bin); only the C backend is end-to-end.
  HLSL/GLSL emit text; x86-64/ARM64/WASM/LLVM/SPIR-V emit output but have no
  linker/assembler integration (no runnable artifacts yet).
- Each .bld module transpiles to C individually. Whole-ecosystem cross-module
  compilation and compiler self-hosting are not yet achieved.
- ~6 GB of local disk is Cargo target/ build cache (already git-ignored; the
  compiler dir buildlang/ is itself git-ignored, a separate repo).

## Tier 1 — Load-bearing, real, extractable (the engine)

| Component | Score | What is real |
|---|---|---|
| buildlang compiler (C backend) | 6.5 | Full front-to-C pipeline; monomorphization, traits/vtables, one-shot effects; 612 pass / 0 fail |
| programs/ (56 MSVC exes) | 9.0 | qdb (SQL, moved to buildlang `bdb/` on 2026-11-09), qparse, qsed, grep, base64, calc, color_test 12/12 |
| spectrum (color science) | 9.0 | sRGB/XYZ matrices, PQ/HLG EOTFs, 13 tonemappers, verified OKLab constants |
| chromatic (perceptual color) | 8.0 | LAB/RGB with matrix inversion, gamut mapping via binary search |
| delta (options pricing) | 8.0 | Black-Scholes exact, full Greeks, Newton-Raphson + Brent IV |
| foundation math / crypto | 8.0 / 7.0 | SHA-256 FIPS 180-4 correct; trig/pow/log via intrinsics |

## Tier 2 — Real kernel inside scaffolding (extract the core)

| Component | Score | Real core | Scaffolding |
|---|---|---|---|
| buildos kernel | 6.5 | Memory, scheduler, ext2/4, IPC, drivers, TCP/IP stack | AI syscalls return -1; self-healing is Z-score, not ML |
| entropy | 7.0 | LSTM forward pass; GBDT variance splits | No backprop |
| oracle | 7.0 | SARIMA differencing + AR/MA fitting | Forecast integration thin |
| field-tensor | 6.0 | Cholesky, power-iteration eigenvalues, indicators | 4D market application sparse |
| quantum-finance | 6.0 | OHLCV, TWAP/VWAP, risk calcs | Broker APIs return empty data |
| photon | 6.0 | ~15% real: meshlet culling, attenuation, ray differentials | ~85% boilerplate; hooks return false |
| prism | 7.0 | Correct HLSL tonemapper math | No GPU compile/injection |
| axiom | 5.0 | Forward-mode dual-number autodiff correct | Mutation ops stubbed; MAML/CMA-ES sketched |
| foundation collections | 6.0 | Vec correct | HashMap/BTreeMap sparse; regex executor missing |

## Tier 3 — Sketch / showcase (design intent, not engineering)

These remain in-tree (interlaced into build/interconnect wiring) but are not to
be represented as implemented engineering.

| Component | Score | Reality |
|---|---|---|
| lumina | 5.0 | Preset configs; claimed FFT bloom has no FFT code |
| refract | 5.0 | ENB metadata; no D3D11 hook installation |
| forge | 5.0 | Logger real; linter/debugger/profiler are shells |
| neutrino | 4.0 | Neural rendering is type enums; zero tensor ops |
| nexus | 4.0 | Mod-framework skeleton; no loader/solver |
| wavelength | 4.0 | Media containers; no DSP/codec |

## Infrastructure dirs (not domain modules)

cli, config, runtime, repl, lsp, fmt, debug, test, profiler, bench, benchmarks,
pkg, docs, universe, examples are scaffolding/glue for the ecosystem manifest,
of mixed completeness. Treat as supporting tooling, not headline modules.

## Compiler capabilities verified 2026-06-05 (Phase 1)

A local C toolchain is now available (MSVC -- Visual Studio 18 Community / 2022
BuildTools via vcvars64.bat), so generated C is now NATIVELY compiled and run,
not just emitted. This immediately corrected several emission-only conclusions.

- Works (was documented as a stale limitation): &self / &mut self receivers; &str
  params; trait default methods calling self.method(); occurs-check on &Struct
  returning the same struct literal; Self{...}, Self::method().
- FIXED + native-verified: the non-generic constructor pattern
  fn new() -> Self { ... } with let x = T::new() called from a free function.
  Emission alone hid a real bug: the call result was typed as the literal Self
  (invalid C; method dispatch fell back to a bare name). Root cause was an
  impl-collect forward-declaration that did not resolve Self -> concrete type.
  Fixed in buildlang 8d83d74 + 2e9296f. e2e regressions tests/programs/132
  (prints 42) and 133 (prints 5) now compile with MSVC cl and run. Rust suite
  612 pass / 0 fail.
- KNOWN OPEN (native LINK failure): GENERIC impl methods returning Self, e.g.
  Wrap<T>::new() -> Self -- the monomorphized constructor/method are not emitted
  (unresolved external symbol). This is the next real blocker (generic
  monomorphization). Correction: an earlier note claimed generic methods
  returning Self "compile"; that was emission-only and is FALSE at native link.
- Heap types (String) in enum variants and generic enum methods: type-check +
  emit only; not yet individually native-checked.

Caveats: (1) The 231K-line compiler in buildlang/src/ (self-hosting is the goal, not
achieved) still does not compile as a whole -- per-feature success does not imply
whole-program self-hosting.
(2) Dev-env: cargo incremental builds are unreliable here (source mtimes are
preserved by the IO layer); use `cargo clean -p buildlang` before each rebuild.
(3) Native build helper: a local build.bat outside this tree (vcvars -> cl -> run).

## Verifiable tier (machine-checked)

The load-bearing components are now machine-verifiable, not merely asserted in
this file. Run:

    python tools/verify_organism.py     (registry: tools/components.toml)

It builds and tests every real component across languages and reports ground
truth; the exit code is the failure count, so it is also a CI gate. Last run:
20 passed / 0 failed -- frametrace (core, C ABI, hook, adapter), buildlang, and 16 .bld domain modules (verified by buildc transpile). CI runs it on windows-latest (.github/workflows/organism.yml).
New real modules join the organism by registering in components.toml.

## Compiler organ deep-codegen finding (2026-06-05)

The Compiler organ (tools/coherence/compiler_oracle.py) adjudicates the deeper
claim "the emitted C compiles", beyond the shallow "buildc exits 0":
- Self-contained programs color_test, wc, base64: CONFIRMED (transpile + cl /c clean).
- programs/calc: CONTRADICTED at transpile (mutability mismatch, calc.bld:120).
- Library modules spectrum, delta: transpile but the emitted C FAILS cl -- a real
  name-prefixing codegen bug (typedef `hdr_ColorPrimaries` but a bare
  `ColorPrimaries` field reference; same class for `BTreeMap` in delta).
So "transpiles cleanly" (the 16 green .bld modules in the organism check) does
NOT imply "emits compilable C". The membrane surfaced this; the codegen prefixing
fix is open buildlang work. The organism gate intentionally keeps "transpiles"
as the module bar (a real if shallow check); the deep check is a separate witness.