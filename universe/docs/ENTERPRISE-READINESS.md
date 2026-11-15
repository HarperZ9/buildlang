# Build Universe Enterprise Readiness

Read this first: Build Universe is an **alpha research compiler ecosystem, not a
shipped toolchain**. It has a real, load-bearing core (the BuildLang-to-C compiler and
a set of verifiable algorithm kernels) sitting inside a larger shell of scaffolding and
showcase modules. This document states plainly what is production-capable, what is
alpha, and what is not achieved, so that no one plans against a capability that does not
exist. Where anything here disagrees with [STATUS.md](../STATUS.md), STATUS.md is
canonical.

There is no registry install. This is a from-source build (`buildc` from `buildlang/`,
plus a local C toolchain). Do not assume `cargo install`, crates.io, or PyPI.

## What it is

A mixed-language compiler ecosystem:

- **BuildLang** and its `.bld` standard library and domain modules (the language and
  its programs).
- **The compiler** (`buildlang/`, Rust, ~231K LOC, 612 passing cargo tests) that
  transpiles one `.bld` module to C, which MSVC then builds to a native binary.
- **A hobby OS kernel** (`buildos/`, Rust, ~196K LOC), separate from the language.

See [ARCHITECTURE.md](../ARCHITECTURE.md) for the layers and the `.bld`-to-C pipeline.

## What is production-capable

Narrow and specific. These are the parts an outside reviewer can build, run, and check:

- **The C backend for self-contained programs.** `buildc` lexes, type-checks,
  monomorphizes, resolves traits and vtables, lowers one-shot effects, and emits C that
  a C toolchain compiles and runs. `programs/` holds 56 MSVC-clean native executables
  from 65 `.bld` sources, including qdb (a SQL engine, since moved to the buildlang repository as `bdb/`), qparse, qsed, grep, base64, and
  calc. `color_test` transpiles, compiles `cl`-clean, runs, and passes 12/12 CIE 1976
  self-checks. This end-to-end path is the concrete proof-of-pipeline.
- **The color-science modules.** spectrum (sRGB/XYZ matrices, PQ/HLG EOTFs, 13
  tonemappers, verified OKLab constants) and chromatic (LAB/RGB with matrix inversion,
  gamut mapping via binary search) contain correct, audited math. These same algorithms
  are re-expressed and shipped as tested Python in the product layer (build-color,
  calibrate-pro).
- **Other verifiable kernels.** delta (Black-Scholes exact, full Greeks, Newton-Raphson
  + Brent IV), foundation crypto (SHA-256, FIPS 180-4 correct), oracle (SARIMA
  differencing + AR/MA fitting), entropy (LSTM forward pass), axiom (forward-mode
  dual-number autodiff), field-tensor (Cholesky, power-iteration eigenvalues). Correct
  math, audited; see the Tier tables in STATUS.md for the exact real-vs-scaffolding
  split of each.

## What is alpha

- **Cross-module compilation.** Each `.bld` module transpiles to C individually.
  Whole-ecosystem cross-module resolution is incomplete; the ecosystem does not compile
  as a whole.
- **Self-hosting.** The ~231K-line compiler does not compile itself from `.bld`.
  Per-feature success does not imply whole-program self-hosting. This is a goal.
- **Emitted-C compilability beyond self-contained programs.** "Transpiles cleanly" (the
  organism-check bar) does not imply "emits compilable C." Some library modules
  (spectrum, delta) transpile but their emitted C fails `cl` on an open name-prefixing
  codegen bug. Self-contained programs are the strongest evidence; library modules are
  not yet uniformly `cl`-clean.
- **The OS kernel.** `buildos/` is a substantial hobby kernel (memory, scheduler,
  ext2/4, IPC, drivers, TCP/IP stack), but it is educational, not a production OS; boot
  in QEMU is plausible but unverified here. Its AI syscalls return -1 and "self-healing"
  is Z-score thresholding, not ML.

## What is not achieved

- **No borrow checker on compiled output.** The emitted output is not checked for the
  borrow rules.
- **Only the C backend runs end to end.** HLSL/GLSL emit shader text only; x86-64,
  ARM64, WASM, LLVM, and SPIR-V generate output but have no linker/assembler
  integration and produce no runnable artifact yet.
- **Tier 3 modules are sketches, not engineering.** lumina, refract, forge's
  linter/debugger/profiler, neutrino, nexus, and wavelength are design intent and
  stubbed methods. Do not plan against them. (CHANGELOG.md lists the features that were
  previously over-claimed and are not implemented: borrow checker, SVD, Blake3/Ed25519,
  HTTP/2, Transformers, GARCH/Prophet, and more.)

## Honest limitations for a reviewer

- Treat STATUS.md, not this document or the README, as the capability source of truth.
- The verified evidence is the 56 native programs and the audited kernels, reproducible
  from source with `buildc` plus a C toolchain and `python tools/verify_organism.py`.
- Dev-environment caveats are real: Cargo incremental builds are unreliable here (run
  `cargo clean -p buildlang` before a rebuild); 3 SPIR-V tests are skipped without
  `spirv-val`.
- The Python product layer (calibrate-pro, build-color, and the other APPS submodules)
  is a **parallel hand-written re-implementation** of the same algorithms, not a binding
  to the `.bld` modules. See [LINEAGE.md](../LINEAGE.md).

## How it fits Project Telos

Build Universe is the compiler-and-language substrate beneath the Build family. Its
verifiable kernels are the proof-of-value; the shipped Python products re-express those
same algorithms as tools a user can run today. Within the Project Telos
accountability frame, the load-bearing claim is checkable: `tools/verify_organism.py`
(registry `tools/components.toml`) builds and tests every real component and returns its
failure count as an exit code, so the health of the ecosystem is a machine-checked
witness, not an assertion. CI runs it on `windows-latest` via
`.github/workflows/organism.yml`. The honest framing is the deliverable here: an alpha
compiler whose real parts are verifiable and whose incomplete parts are named, not an
enterprise toolchain.

See [ARCHITECTURE.md](../ARCHITECTURE.md) for the design map and [CONTRIBUTING.md](../CONTRIBUTING.md)
for build-from-source and gates.
