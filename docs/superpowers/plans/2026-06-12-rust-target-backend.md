# Rust Target Backend Implementation Plan

> **For Zain:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan step-by-step.

**Goal:** Add an experimental Rust source target to QuantaC and verify emitted
Rust with `rustc`.

**Architecture:** Extend the existing MIR backend interface with a new
`RustBackend` that emits Rust source. Keep the first slice conservative and
return explicit unsupported-feature errors when MIR constructs are not safely
lowered yet.

**Tech Stack:** Rust compiler crate, existing QuantaLang MIR, existing CLI.

### Task 1: Add Red Tests

**Files:**
- Modify: `compiler/src/codegen/mod.rs`
- Modify: `compiler/src/main.rs`

**Steps:**
1. Add tests for `OutputFormat::RustSource` text behavior.
2. Add tests for `Target::Rust` availability and default `.rs` extension.
3. Add CLI target parser tests for `rust`, `rs`, and `.rs` inference.
4. Run targeted tests and confirm the expected failure.

### Task 2: Wire Target Plumbing

**Files:**
- Modify: `compiler/src/codegen/backend/mod.rs`
- Modify: `compiler/src/codegen/mod.rs`
- Modify: `compiler/src/main.rs`

**Steps:**
1. Add `Target::Rust`.
2. Add `OutputFormat::RustSource`.
3. Wire `CodeGenerator::generate` to a Rust backend.
4. Update CLI target parsing, help text, default extension inference, and watch
   target inference.

### Task 3: Implement RustBackend

**Files:**
- Add: `compiler/src/codegen/backend/rust.rs`

**Steps:**
1. Emit a header and deterministic string table comments.
2. Emit structs and functions from MIR.
3. Map scalar MIR types to Rust scalar types.
4. Emit locals, assignments, arithmetic, calls, returns, references, arrays,
   and simple branch/goto blocks.
5. Convert `printf`-style calls through a small Rust formatting helper so
   lowered dynamic format strings do not violate Rust macro literal rules.
6. Return `CodegenError::Unsupported` for uncovered MIR constructs.

### Task 4: Documentation

**Files:**
- Modify: `README.md`

**Steps:**
1. Add Rust to the target list and target selection table.
2. Mark it experimental and subset-based.
3. Mention `rustc --emit=metadata` as the current validation gate.

### Task 5: Verify

**Commands:**
- `C:\Users\Zain\.cargo\bin\cargo.exe fmt --manifest-path compiler\Cargo.toml`
- `C:\Users\Zain\.cargo\bin\cargo.exe test --manifest-path compiler\Cargo.toml --quiet`
- Generate Rust for selected programs with `quantac --target rust`.
- `C:\Users\Zain\.cargo\bin\rustc.exe --emit=metadata <generated>.rs`

**Expected:** Rust target tests pass, full compiler tests remain green, and
generated Rust passes metadata compilation for the selected subset.
