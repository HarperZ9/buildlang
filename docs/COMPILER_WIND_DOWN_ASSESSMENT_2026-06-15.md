# Compiler Wind-Down Assessment - 2026-06-15

This note captures the current posture for winding down full-scale QuantaLang
compiler/language development and shifting toward preservation, packaging, and
selective maintenance.

## Recommendation

Freeze broad language expansion after the current accountability/security
slice. Keep QuantaLang positioned as a working compiler core with a verified C
path, a practical accountability/effects model, and clearly labeled
experimental backend research. Future work should be limited to:

- regression fixes for verified surfaces;
- receipt, policy, and documentation alignment;
- packaging/release hygiene;
- narrow backend preservation tests when they prevent existing evidence from
rotting.

Do not start new full-scale language features, self-hosting pushes, or backend
productionization unless a later product direction explicitly reopens that
lane.

## Product Anchor

The production claim remains the C path:

- parser -> type checker -> MIR -> C backend -> native executable;
- semantic corpus receipts for the current 8-program corpus;
- `quantac corpus verify` coverage for manifest, C/Rust receipts, and real
  C-backend stdout;
- `quantac doctor` for local readiness diagnostics.

The broader compiler is valuable as a public research/product surface, but the
release-candidate boundary should stay honest: C is the practical target;
Rust and x86/x64 are experimental.

## Rust Backend Assessment

Current status: experimental subset, useful as a compiler-adjacent validation
route.

Evidence gathered on 2026-06-15:

- `cargo test --manifest-path compiler\Cargo.toml generated_rust_compiles --quiet`
  passed: 14 metadata tests.
- `cargo test --manifest-path compiler\Cargo.toml generated_rust_runs --quiet`
  passed: 8 executable stdout tests.
- `cargo test --manifest-path compiler\Cargo.toml semantic_corpus_manifest --quiet`
  passed: 2 manifest/receipt tests.
- `semantic-corpus/receipts/rust-execution-2026-06-13.json` records 8
  generated-artifact execution programs, `rustc` executable builds, and stdout
  assertions.

What it proves:

- generated Rust compiles for selected ownership, borrowing, aggregate, field,
  dereference, and lifetime smoke surfaces;
- selected generated Rust executables run and match stdout;
- semantic corpus paths, expected stdout, generated Rust, `rustc`, and receipt
  metadata are coupled by tests.

What it does not prove:

- full MIR coverage;
- production-grade Rust backend semantics;
- parity with the C backend;
- broad standard-library, effects-runtime, or self-hosting viability.

Wind-down action:

- keep the Rust backend as an experimental validation lane;
- preserve the existing metadata/execution corpus and receipts;
- avoid expanding the Rust backend beyond bug fixes and receipt preservation.

## x86/x64 Backend Assessment

Current status: experimental assembly/object research lane. `x64` maps to the
same `Target::X86_64` as `x86-64`.

Evidence gathered on 2026-06-15:

- `cargo test --manifest-path compiler\Cargo.toml x86_64 --quiet` passed:
  61 backend and encoder tests.
- `compiler/src/main.rs` accepts `x86-64`, `x86_64`, and `x64`.
- `compiler/src/codegen/mod.rs` routes `Target::X86_64` to
  `backend::x86_64::X86_64Backend`.
- `compiler/src/main.rs` writes x86-64 assembly and attempts assembler/linker
  guidance or tool invocation when available.

What it proves:

- the backend and instruction encoder are actively test-covered in isolation;
- basic assembly and raw machine-code generation paths exist;
- CLI target selection reaches the backend.

What it does not prove:

- stable native executable production across Windows/Linux/macOS;
- a verified x86/x64 semantic corpus;
- production linker/runtime integration;
- full MIR coverage for structs, enums, arrays, effects runtime, and richer
  control flow.

Wind-down action:

- label x86/x64 as preserved experimental backend research;
- keep tests as regression guards;
- do not invest in assembler/linker/runtime completion during wind-down.

## Recent Update Digest

The last couple of days moved QuantaLang from "compiler with effects" toward a
typed accountability language:

- direct ambient capability gate for file, network, process, environment,
  clock, GPU, console, and FFI surfaces;
- check receipts with compiler/version/source/input graph digests;
- receipt verification with source, policy, profile, and accountability replay;
- strict built-in policy profiles and policy scaffolding from receipts;
- direct and propagated capability-source allowlists;
- callback/effect provenance through higher-order functions, methods, trait
  objects, closures, async awaits, aggregates, selected branches, loops, casts,
  refs/derefs, pipes, and assignments;
- compile-time macro gates for include/environment macros;
- macro-argument scanning for ambient helpers, external modules, unknown extern
  calls, and foreign statics;
- semantic corpus execution receipts for C and Rust validation lanes;
- public README/STATUS/effects-guide updates tied to current test counts.

## Current Local Baseline

As of this assessment, docs and status claim the CI-shaped baseline as:

- 868 passing tests;
- 0 failing tests;
- 10 ignored tests;
- 4 filtered tests;
- 192 CLI tests;
- 99,214 Rust compiler source lines;
- 12,273 Rust integration-test lines.

The final release-shaped verification for the commit that includes this note
should rerun the full CI-shaped cargo baseline, warning gate, clippy
correctness, rustfmt, diff whitespace, `.env` ignore, stale-count scan, and
credential-shaped diff scan.
