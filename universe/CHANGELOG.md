# Changelog

All notable changes to BUILD-UNIVERSE are documented here.
Format based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/);
this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> This file was rewritten on 2026-06-05 to reflect ground truth. The prior
> entry listed many features as shipped that are not implemented (borrow
> checker, SVD, Blake3/Ed25519, HTTP/2, Transformers, GARCH/Prophet, etc.).
> Those are now listed under "Not yet implemented." STATUS.md is canonical.

## [Unreleased]

### Moved
- qdb (BuildDB, `programs/db.bld`), its tests, benchmarks, architecture notes and
  workflow moved to HarperZ9/buildlang `bdb/` on 2026-11-09 with their history
  (author decision: "Move into BuildLang"). `programs/QDB.md` points there; 4a296c1
  is the last commit here that holds the source.

## [1.0.0] - 2026

### Verified - actually ships
- **BuildLang compiler**: lexer, parser, type checker, monomorphization,
  traits + vtables, one-shot algebraic effects; **C backend end-to-end**;
  755 test functions in tree; produces buildc.exe.
- **Native programs**: 56 MSVC-clean executables from 65 .bld sources in
  programs/ - including qdb (SQL engine), qparse, qsed, grep, base64, calc,
  and color_test (12/12 self-checks pass).
- **foundation stdlib**: SHA-256 (FIPS 180-4 correct), math via intrinsics,
  Vec with correct growth. Collections HashMap/BTreeMap are partial; the
  regex executor is not implemented.
- **Verifiable domain kernels** (correct math, audited): spectrum color
  science (sRGB/XYZ, EOTFs, 13 tonemappers, OKLab), chromatic perceptual
  color, delta (Black-Scholes + full Greeks + IV solvers), oracle (SARIMA),
  entropy (LSTM forward pass), axiom (forward-mode dual-number autodiff),
  field-tensor (Cholesky, power-iteration eigenvalues, indicators).
- **BuildOS**: substantial hobby kernel - memory management, scheduler,
  ext2/ext4, IPC, drivers (PCI/ACPI/AHCI/NVMe/USB), a TCP/IP stack. Boots in
  QEMU is plausible but unverified here. AI syscalls and "self-healing" are
  stubs (return -1; Z-score thresholding, not ML).

### Backends
- **C**: end-to-end, verified. HLSL/GLSL: emit shader text only.
- x86-64, ARM64, WASM, LLVM, SPIR-V: generate output but have no
  linker/assembler integration - none yet produce a runnable artifact.

### Not yet implemented (previously listed as done - corrected)
- Compiler borrow checker (compiled output is unchecked), whole-ecosystem
  cross-module compilation, self-hosting.
- Crypto beyond SHA-256: AES-256, ChaCha20, Blake3, Ed25519, X25519.
- Networking: TLS, HTTP/1.1+HTTP/2, WebSocket; the async runtime.
- Serialization: MessagePack, Protocol Buffers, YAML.
- Linear algebra: SVD, full eigendecomposition beyond power iteration.
- ML: backprop/optimizers (SGD/Adam/...), Conv/GRU/Transformer training,
  GARCH, Prophet, Isolation Forest, One-Class SVM.
- Graphics: SPIR-V compilation pipeline, ray tracing, scene graph runtime.
- Tooling: a unified package/CLI ("quark"); forge's linter/debugger/profiler
  are shells (its logger is real).

### Corrections
- License unified to **fair-source** (FSL-1.1-MIT; source-available, competing commercial use reserved), matching
  the LICENSE file; UNIVERSE.toml corrected from "Proprietary".
- Version unified to **1.0.0**; UNIVERSE.toml corrected from 2.0.0.
- ENGINEERING.md APPS test counts corrected to measured values
  (calibrate-pro 228, build-color 281, build-engine 173); aurora has no
  source present locally.

---

*Copyright (c) 2022-2026 Zain Dana Harper. FSL-1.1-MIT (see LICENSE).*
