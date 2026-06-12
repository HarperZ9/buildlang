# Rust Target Backend Design

Date: 2026-06-12
Status: Accepted for first implementation slice

## Purpose

Add Rust as a first-class QuantaC code generation target so QuantaLang can use
Rust as a typed substrate and verification bridge rather than only treating
Rust as the compiler implementation language.

## First Slice

The first backend is intentionally narrow:

- Add `Target::Rust` and `OutputFormat::RustSource`.
- Support `--target rust` / `--target rs` and `.rs` output inference.
- Emit Rust source for MIR functions, locals, assignments, arithmetic, simple
  calls, returns, structs, arrays, references, and basic branches.
- Reject unsupported MIR with explicit `CodegenError::Unsupported` instead of
  silently emitting incorrect Rust.
- Verify generated Rust with the local Rust compiler by running
  `rustc --emit=metadata` on selected generated outputs.

## Ownership Bridge

The compiler already performs QuantaLang borrow-state checks during type
inference. This slice does not replace that with a Rust clone; it projects
borrow-shaped MIR into Rust syntax so `rustc` can act as an additional backend
validation layer. The QuantaLang checker remains the language authority, while
Rust gives a concrete systems-language substrate for the portable subset.

## Non-Goals

- Full self-hosting.
- Full standard-library lowering.
- Shader-to-Rust compute parity.
- Async, traits, closures, generics, and package/runtime semantics beyond the
  existing MIR subset needed by smoke tests.

## Acceptance

- Compiler unit tests pass.
- `quantac --target rust` emits `.rs` files.
- Generated Rust for at least hello, functions, variables, arithmetic, and
  simple branching passes `rustc --emit=metadata`.
- Documentation states the Rust target status as experimental and subset-based.
