# Build Universe: architecture

Build Universe is a compiler ecosystem, not a single program. It is deliberately
mixed-language: a language and its standard library written in BuildLang (`.bld`),
a compiler written in Rust that turns a `.bld` module into C, and a hobby OS kernel
written in Rust. This document maps the layers, shows how a `.bld` module becomes a
native binary, describes the module-dependency shape, and states the maturity tier
of each part honestly. Where any statement here disagrees with
[STATUS.md](STATUS.md), STATUS.md is canonical.

This is an alpha research compiler. Read this as a map of what exists and what does
not, not as a claim that the ecosystem builds as a whole. It does not.

## The layers

```
   Rust                                     BuildLang (.bld)                Rust
   ----                                     ----------------                ----
+------------------+   transpiles   +---------------------------+     +----------------+
|  buildlang/      | -------------> |  foundation/  (stdlib)    |     |  buildos/      |
|  the compiler    |   one module   |  spectrum, delta, oracle, |     |  the OS kernel |
|  (~231K LOC)     |   at a time    |  chromatic, ... (.bld)    |     |  (~196K LOC)   |
+------------------+                +-------------+-------------+     +----------------+
                                                  | emits C
                                                  v
                                            C source (.c/.h)
                                                  | MSVC (cl)
                                                  v
                                          native .exe / .obj
```

Four layers, three languages:

1. **BuildLang, the language.** A multi-paradigm systems language with algebraic
   effects, ownership, traits, and generics. Its syntax and semantics are defined
   in practice by the `.bld` source in this repo, which serves as both working code
   and language specification.

2. **The standard library and domain modules (`.bld`).** `foundation/` is the
   standard library (math, string, collections, io, crypto). The domain modules
   (spectrum, chromatic, delta, oracle, entropy, field-tensor, photon, prism,
   axiom, and the rest) are separate `.bld` packages that demonstrate the language
   across color science, options pricing, forecasting, ML, and rendering.

3. **The compiler (`buildlang/`, Rust, ~231K LOC).** The load-bearing engine. It
   lexes, parses, type-checks, monomorphizes, resolves traits and vtables, lowers
   one-shot algebraic effects, and emits C. `buildlang/` is a separate repo,
   git-ignored inside this one; it produces `buildc` (the compiler binary). On
   `cargo test` it reports 612 pass / 0 fail (755 `#[test]` annotations in tree,
   counting ignored and multi-binary tests).

4. **The OS kernel (`buildos/`, Rust, ~196K LOC).** A separate hobby kernel:
   memory management, a scheduler, ext2/ext4, IPC, PCI/ACPI/AHCI/NVMe/USB drivers,
   and a TCP/IP stack. It is written in Rust, **not** in BuildLang, and is an
   educational kernel, not a production OS. See [buildos/STATUS.md](buildos/STATUS.md).

## How a `.bld` module becomes C

A single module compiles end to end. The whole ecosystem does not (see the
limitations below). For one module the pipeline is:

```
  module.bld
     |  1. lex        (tokens; UTF-8-aware, box-drawing chars rejected in comments)
     |  2. parse      (AST; recursive descent)
     |  3. resolve    (mod/use, DefId assignment, type_module_map)
     |  4. type-check (Hindley-Milner + trait bounds; occurs-check; unification)
     |  5. monomorphize (generic instances expanded to concrete types)
     |  6. lower       (traits -> vtables; one-shot effects; MIR-style lowering)
     |  7. codegen     (emit C: structs -> typedef struct, enums forward-declared,
     |                   Vec -> typed runtime calls, intrinsics -> C stdlib)
     v
  module.c / module.h
     |  8. cl (MSVC)   (compile + link)
     v
  native .exe
```

Steps 1 through 7 are `buildc`; step 8 is a local C toolchain (MSVC, verified with
Visual Studio 2022 BuildTools via `vcvars64.bat`). The C backend is the only backend
that runs end to end. HLSL/GLSL emit shader **text** only. The x86-64, ARM64, WASM,
LLVM, and SPIR-V backends generate output but have no linker/assembler integration,
so none produces a runnable artifact yet.

Two honesty notes that STATUS.md records and this map must not soften:

- **"Transpiles cleanly" does not imply "emits compilable C."** The 16 `.bld` domain
  modules that pass the organism check pass a `buildc` transpile, which is a real but
  shallow bar. A separate deep check found that some library modules (spectrum, delta)
  transpile but their emitted C fails `cl` on a name-prefixing codegen bug (a
  `hdr_ColorPrimaries` typedef against a bare `ColorPrimaries` field reference). That
  is open compiler work, tracked as a limitation, not a finished capability.

- **Self-contained programs are the strongest evidence.** `color_test`, `wc`, and
  `base64` transpile and compile `cl`-clean and run; `color_test` passes 12/12 CIE
  self-checks. `programs/calc` is contradicted at transpile (a mutability mismatch).
  The 56 native executables in `programs/` are the concrete proof-of-pipeline.

## Module-dependency shape

- Every `.bld` module depends on the **compiler** (`buildlang/`). There is no `.bld`
  module that builds without `buildc`.
- Domain modules depend on `foundation/` (the standard library) for math, strings,
  and collections, and otherwise stand alone. They do **not** currently resolve each
  other across module boundaries in a single whole-ecosystem build.
- `buildos/` (the kernel) is independent of the `.bld` modules; it is a Rust project
  built with its own Rust toolchain, not through `buildc`.
- The Python product layer (`calibrate-pro`, `build-color`, and the other APPS
  submodules, in a separate workspace) is a **parallel hand-written re-implementation**
  of the same algorithms, not a binding: Python does not import the `.bld` modules.
  See [LINEAGE.md](LINEAGE.md).

## Maturity tiers (pulled from STATUS.md, not inflated)

STATUS.md scores engineering concreteness 0-10 from direct source audit: real,
correct, extractable logic vs scaffolding/showcase. Summarized here; STATUS.md is the
canonical detail.

### Tier 1 — load-bearing, real, extractable (the engine)

| Component | Score | What is real |
|---|---|---|
| buildlang compiler (C backend) | 6.5 | Full front-to-C pipeline; monomorphization, traits/vtables, one-shot effects; 612 pass / 0 fail |
| programs/ (56 MSVC exes) | 9.0 | qdb (SQL, moved to buildlang `bdb/` on 2026-11-09), qparse, qsed, grep, base64, calc, color_test 12/12 |
| spectrum (color science) | 9.0 | sRGB/XYZ matrices, PQ/HLG EOTFs, 13 tonemappers, verified OKLab constants |
| chromatic (perceptual color) | 8.0 | LAB/RGB with matrix inversion, gamut mapping via binary search |
| delta (options pricing) | 8.0 | Black-Scholes exact, full Greeks, Newton-Raphson + Brent IV |
| foundation math / crypto | 8.0 / 7.0 | SHA-256 FIPS 180-4 correct; trig/pow/log via intrinsics |

### Tier 2 — real kernel inside scaffolding (extract the core)

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

### Tier 3 — sketch / showcase (design intent, not engineering)

These remain in-tree, interlaced into the build wiring, but must **not** be
represented as implemented engineering: lumina (5.0, preset configs, claimed FFT
bloom has no FFT code), refract (5.0, ENB metadata, no D3D11 hook), forge (5.0,
logger real, linter/debugger/profiler are shells), neutrino (4.0, type enums, zero
tensor ops), nexus (4.0, skeleton, no loader), wavelength (4.0, containers, no
DSP/codec).

Infrastructure dirs (cli, config, runtime, repl, lsp, fmt, debug, test, profiler,
bench, benchmarks, pkg, docs, universe, examples) are scaffolding/glue for the
ecosystem manifest, of mixed completeness. Treat as supporting tooling, not headline
modules.

## What is not achieved (the honest boundary)

- **The ecosystem does not compile as a whole.** Each `.bld` module transpiles to C
  individually; whole-ecosystem cross-module resolution is incomplete.
- **Self-hosting is a goal, not a fact.** The ~231K-line compiler in `buildlang/src/`
  does not compile itself from `.bld`; per-feature compilation success does not imply
  whole-program self-hosting.
- **No borrow checker on emitted output.** Compiled output is currently unchecked for
  the borrow rules.
- **Only the C backend is end to end.** The other backends are experimental.

## How the ecosystem is verified

`tools/verify_organism.py` (registry `tools/components.toml`) builds and tests every
real component and reports ground truth; its exit code is the failure count, so it
doubles as a CI gate (last run: 20 passed / 0 failed, covering frametrace, buildlang,
and 16 `.bld` domain modules verified by `buildc` transpile). CI runs it on
`windows-latest` via `.github/workflows/organism.yml`; the file-validation and lint
gate runs via `.github/workflows/ci.yml`. See [CONTRIBUTING.md](CONTRIBUTING.md) for
the build-from-source and gate details.

## Where it fits Project Telos

Build Universe is the compiler-and-language substrate of the Build family. Its
verifiable kernels (color science, options pricing, SARIMA, SHA-256) are the
proof-of-value; the Python product layer (calibrate-pro, build-color, and the other
APPS submodules) re-expresses those same algorithms as shipped tools. See
[docs/ENTERPRISE-READINESS.md](docs/ENTERPRISE-READINESS.md) for the
production-capable vs alpha split and [LINEAGE.md](LINEAGE.md) for the family tree.
