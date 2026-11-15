# BUILD-UNIVERSE — Engineering Heatmap & Action Plan

> Ground-truth concreteness assessment of the foundational repo.
> Method: 5 parallel source audits (compiler, kernel, rendering, finance/AI/tools,
> native+APPS) plus direct fact verification. Scores are "raw engineering
> concreteness" 0-10: real, correct, extractable logic vs scaffolding/showcase.
> Verified: 2026-06-05.

## 1. The one-paragraph truth

BUILD-UNIVERSE has a real, load-bearing core -- the BuildLang compiler -- and a
set of genuinely-correct algorithm kernels sitting inside a large outer shell of
hollow showcase modules. The compiler C backend works end-to-end: .bld programs
transpile to C and compile MSVC-native, producing 56 working executables (SQL
engine, parser, grep, calc). Around that, a handful of modules contain verifiably-
correct math (color science, options pricing, SHA-256, forward-mode autodiff,
SARIMA, LSTM forward pass). The rest -- most of the rendering and tooling clusters
-- are struct definitions plus stubbed methods. The shippable product layer is not
here; it lives in the separate APPS workspace as tested Python. The raw engineering backbone
is: the compiler + its native-program pipeline + ~6 verifiable kernels.

## 2. The heatmap

### TIER 1 -- Load-bearing, real, extractable (the actual engine)

| Component | Score | What is real |
|---|---|---|
| buildlang compiler (C backend) | 6.5 | Lexer->parser->typecheck->C-codegen end-to-end; 755 unit tests; monomorphization, traits/vtables, one-shot effects |
| programs/ (65 src, 56 MSVC exes) | 9.0 | Pipeline ships native binaries: qdb (SQL, since moved to buildlang `bdb/`), qparse, qsed, grep, base64, color_test 12/12 pass |
| spectrum (color science) | 9.0 | Correct sRGB-XYZ matrices, PQ/HLG EOTFs, 13 tonemappers, verified OKLab constants |
| chromatic (perceptual color) | 8.0 | Real LAB-RGB with matrix inversion, gamut mapping via binary search |
| delta (options pricing) | 8.0 | Black-Scholes d1/d2 exact, Abramowitz-Stegun erf, full Greeks, Newton-Raphson + Brent IV |
| foundation math + crypto | 8.0 / 7.0 | SHA-256 FIPS 180-4 correct; trig/pow/log via intrinsics |

### TIER 2 -- Real kernel inside scaffolding (extract the core, drop the wrapper)

| Component | Score | Real core | Scaffolding |
|---|---|---|---|
| buildos kernel | 6.5 | Memory, scheduler (CFS+RT), ext2/4, IPC, 15-phase boot, TCP/IP stack 16K LOC, USB/NVMe/AHCI/PCI/ACPI drivers | AI syscalls return -1; self-healing is Z-score, not ML |
| entropy (ML) | 7.0 | LSTM forward pass correct; GBDT variance-split trees | No backprop |
| oracle (forecasting) | 7.0 | SARIMA differencing + AR/MA fitting | Forecast integration thin |
| field-tensor | 6.0 | Cholesky, power-iteration eigenvalues, SMA/EMA/RSI | 4D market application sparse |
| quantum-finance | 6.0 | OHLCV, TWAP/VWAP routing, risk calcs | Broker APIs return empty data |
| photon (97K LOC) | 6.0 | ~15% real: meshlet culling, light attenuation, ray differentials, BVH | ~85% boilerplate; hooks return false |
| prism (shaders) | 7.0 | Correct HLSL tonemapper math | No GPU compile/injection |
| axiom (autodiff/NAS) | 5.0 | Forward-mode dual numbers fully correct | Mutation operators stubbed; MAML/CMA-ES sketched |
| foundation collections | 6.0 | Vec growth + unsafe ptr ops correct | HashMap/BTreeMap sparse; regex executor unimplemented |

### TIER 3 -- Hollow showcase (design docs, not engineering)

| Component | Score | Reality |
|---|---|---|
| lumina | 5.0 | Preset configs; claimed FFT bloom has zero FFT code |
| refract | 5.0 | ENB metadata; no D3D11 hook installation |
| forge | 5.0 | Logger real; formatter/linter/debugger/profiler are shells |
| neutrino | 4.0 | Neural rendering is type enums; zero tensor ops |
| nexus | 4.0 | Mod-framework skeleton; no loader/solver |
| wavelength | 4.0 | Media containers; no DSP/codec |

## 3. Verified fact ledger (conflicts found in canonical docs)

- VERSION file = 1.0.0; README = 1.0.0; CATALOG = 1.0.0; UNIVERSE.toml = 2.0.0 (outlier).
- LICENSE file = MIT (Copyright Zain Dana Harper 2024-2026); README = MIT; UNIVERSE.toml = Proprietary (outlier).
- Module count: CATALOG names 22; UNIVERSE.toml says 32; 35 dirs contain lib.bld.
- Compile status: README says "does not compile as a whole, Alpha"; ENGINEERING says "16/16 modules compile". Reality: each module transpiles to C individually; cross-module resolution incomplete; self-hosting 0% compilable.
- Compiler tests: README "599", ENGINEERING "604"; actual #[test] count = 755.
- The "8 backends" claim: only C is end-to-end. HLSL/GLSL emit text. x86-64/ARM64/WASM/LLVM/SPIR-V have no linker/assembler integration -- none produce a runnable artifact.
- CHANGELOG lists features absent or stubbed in source: borrow checker, SVD, Blake3, Ed25519, HTTP/2, WebSocket, Transformer, SPIR-V compilation, GARCH, Prophet, Isolation Forest.
- ~6 GB is Cargo target/ build cache (buildos 3.1 GB, buildlang 2.9 GB). Real source < ~5 MB.

## 4. Plan of action (heatmap-driven)

### Phase 0 -- Hygiene and honest status (highest ROI)
- Purge build waste: target/, root .obj/.pdb, stray .exe, release zips -> .gitignore. Reclaims ~6 GB.
- Collapse README + ENGINEERING + CHANGELOG into one honest STATUS with the Tier table. Label each module real / kernel-only / sketch.
- Resolve version + license outliers in UNIVERSE.toml.

### Phase 1 -- Harden the load-bearing core (highest strategic leverage)
- Fix the 3 generic-system gaps blocking self-hosting AND cross-module resolution: and-self / and-mut-self receivers, generic methods returning Self, heap types in enum variants.
- Wire LLVM backend to clang for a second real backend + cross-platform native output.

### Phase 2 -- Consolidate the verifiable kernels
- Promote Tier-1/Tier-2 correct kernels into a verified set, each with a self-checking program in the color_test mold.
- Demote Tier-3 modules to a labeled sketches area, or delete.

### Phase 3 -- Bridge spec to product (APPS)
- Formalize the UNIVERSE-APPS mapping; make .bld (spec) and Python (product) validate against shared golden-vector tests. Ship calibrate-pro, build-color.

## 5. The decision

Invest in the compiler core (Phase 1), not the breadth. The ecosystem value rests on
the compiler being real -- and it mostly is. The three generic-system fixes convert
"modules that each transpile in isolation" into "an ecosystem that compiles together
and can host its own compiler." Tier 3 is a distraction until the core can carry the
stdlib. The verifiable kernels (Tier 1) are the proof-of-value to harden; the showcase
modules are honesty-debt to label or cut.

## Phase 1 outcome (2026-06-05)

Phase 1 (compiler generic-system fixes) was started and largely re-scoped by
empirical testing. Of the three target gaps, most documented limitations were
already resolved on the current compiler:
- &self / &mut self receivers, &str params, trait self-dispatch, occurs-check,
  generic methods returning Self, and heap types in enum variants all compile.
- One real defect found and fixed: bare Self / unit-struct value in value position
  (fn new() -> Self { Self }). 3-layer fix + regression test; 612 tests pass.
  See buildlang commit 8d83d74 (branch feat/phase1-generics) and STATUS.md.

Revised guidance: the highest-leverage compiler work is NOT the documented
generic-system list (mostly already done) but (a) finding the ACTUAL remaining
self-hosting blockers by compiling the real buildlang/src/ tree, and (b) a local
C toolchain so generated C can be execution-verified, not just emitted. STATUS.md
is the canonical capability record.
