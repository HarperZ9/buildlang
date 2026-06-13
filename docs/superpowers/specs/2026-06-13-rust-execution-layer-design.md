# Rust Execution Layer Design

Date: 2026-06-13
Status: Implemented for first smoke slice

## Purpose

Move the Rust backend from compile-only validation to a first executable
behavior layer. The backend already lowers selected QuantaLang programs to Rust
source and checks them with `rustc --emit=metadata`; this slice adds a narrower
gate that compiles generated Rust to an executable, runs it, and verifies stdout.

## First Slice

The executable layer is intentionally small:

- Add a test helper that writes generated Rust to a temp file, invokes `rustc`,
  runs the produced executable, and compares stdout to an expected string.
- Keep existing `rustc --emit=metadata` tests as the broader syntax/type gate.
- Add executable smoke tests only for deterministic subset programs:
  scalar branching, references and mutation, structs/arrays, and tuple
  ownership reuse.
- Store those programs under `semantic-corpus/programs/` and include them from
  the Rust backend tests so backend receipts and tests share one source vector.
- Report rustc failures and runtime failures with stdout, stderr, and generated
  source to preserve diagnosability.

## Architecture

The helper lives in the Rust backend test module beside
`assert_rustc_metadata_ok`. It reuses `compile_quanta_to_rust` so the tested path
stays parser -> type checker -> MIR -> Rust backend -> rustc -> executable.

The generated executable is not installed or reused. Each test creates a
process-local temp directory under `std::env::temp_dir()` and writes one
`generated.rs` plus one executable artifact. Windows uses an `.exe` suffix;
other platforms use the unsuffixed output path.

## Non-Goals

- No CLI integration for executable Rust artifacts in this slice.
- No Cargo project generation.
- No full Rust backend runtime conformance suite.
- No expansion into LLVM, WASM, SPIR-V, HLSL, or GLSL validators.

## Acceptance

- New executable Rust smoke tests pass through Cargo.
- Existing `generated_rust_compiles` metadata tests remain green.
- `rust_target` CLI/alias test remains green.
- Full compiler test suite remains green.
- Documentation and backend capability descriptor distinguish metadata coverage
  from the narrower executable smoke coverage.
