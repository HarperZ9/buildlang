> **Superseded 2026-06-30** by `docs/UNIVERSAL_SUBSTRATE_DIRECTIVE_2026-06-30.md`, which
> lifts the native/GPU backend deferral and returns the project to full-speed, sequenced
> development. This document is preserved unedited below (except this banner and the
> status line) for the provenance trail; it no longer governs posture.

# Universal Substrate Directive - 2026-06-29

Status: superseded 2026-06-30. Preserved for provenance. See the successor directive for
the governing posture.

## Supersession Notice

This directive supersedes `docs/COMPILER_WIND_DOWN_ASSESSMENT_2026-06-15.md`. The
wind-down document is kept in the tree, unedited in body, with a banner marking it
superseded, so the provenance trail stays honest. The reopen it triggers is
stratified, not a blanket reversal:

- **Lifted** (active work is now sanctioned): MIR as a serializable interlingua;
  the effects and data substrate; C-path hardening across operating systems; and
  self-hosting (an explicit operator decision, see below).
- **Kept deferred** (the wind-down's caution still holds): full productionization of
  the native and GPU backends (x86-64 and ARM64 linker integration, WASM execution
  harness, SPIR-V Vulkan validation, LLVM). Each of these reopens only against a
  named consuming product and published graduation criteria, not in parallel.

## Identity And Rename

The substrate is named **buildlang** (the effects language) and **buildc** (the
compiler binary). This activates the previously deferred quanta to build rebrand.

The rename is **milestone 1** of this program and is executed from a written
checklist, never a blind search and replace. Scope, confirmed by the operator:

- The whole `quanta-*` family becomes `build-*` (quantalang to buildlang,
  quanta-universe to build-universe, quanta-color/finance/oracle/engine/ui/ecosystem,
  and the two quantalang-* tooling repos).
- The `.quanta` source extension becomes `.bld`.
- The outward GitHub repositories and the landing page path are renamed in the same
  program (not deferred), after each repository verifies green locally.

Measured surface in this repository alone: 837 `.quanta` files, roughly 45,000
`quanta*` token occurrences across 2,132 files, and roughly 7,100 `quanta_*` runtime
symbols. This is a migration, executed in verified stages, not an edit.

## Operator Decisions (2026-06-29)

1. **Meaning of "universal cross-language transpiler": both, sequenced.** Harden the
   output and data spine first (a serializable MIR interlingua, `.bld` to many
   targets, and a native data interface). Then add input front-ends that ingest other
   source languages (Rust, Python, Go) into MIR. The interlingua is proven before the
   project bets on multi-language ingestion.

2. **Self-hosting is an active milestone**, not a deferred or abandoned goal. The
   path is to unblock module imports, user-defined `macro_rules!`, and a self-hosted
   core library (Option, Result, Vec) so that buildc can eventually compile itself.
   The existing self-hosted `.bld` source is retained as the target to make compile.
   It is labeled clearly as not-yet-functional so it never implies false readiness.

## Honest Baseline (what is actually verified)

- The compiler front-end is strong and real: lexer, parser, Hindley-Milner inference
  with first-class typed algebraic effects, traits, const generics, higher-kinded
  types, and interprocedural lifetimes. 1002 tests pass (verified via `cargo test`).
- **Only the C backend is verified end-to-end.** The other backends (Rust subset,
  x86-64, ARM64, WASM, LLVM, SPIR-V, HLSL, GLSL) are experimental and are not a
  production path today.
- MIR is a well-designed, type-complete SSA IR that is currently in-memory only. It
  has never been serialized and never been used as an input channel. Making it a
  durable interlingua is the keystone of this program.
- Tool interop today is ad-hoc JSON over stdin and stdout. There is no Quanta-native
  data format and no effect-typed boundary. The native data suite (qdb, qsql, qkv,
  qjq, qcsv) is real but siloed.
- "Universal substrate, all hardware, full OS" is a multi-quarter program, not a
  near-term deliverable. The most demonstrable near-term win is the data interface,
  reachable in weeks on the verified C path without the backend slog.

## Sequenced Program

- **Phase 0 (now): posture and rename.** This directive; the supersession banner;
  reclassify backends by strategic importance; execute the quanta to build rename
  migration (milestone 1) from its checklist.
- **Phase 1: make the verified spine load-bearing.** Serialize and version MIR with
  round-trip golden tests; define a transpile-preservation criterion and a harness
  proving the same MIR lowered to C and to Rust produces byte-identical output; add a
  three-OS CI matrix for the C path.
- **Phase 1b (parallel, operator-elevated): self-hosting unblock.** Module imports,
  `macro_rules!` with hygiene, and a self-hosted Option/Result/Vec, with method and
  closure receiver fixes, toward buildc compiling its own front-end.
- **Phase 2: native data substrate.** A Quanta Data Format with effect-typed read and
  write boundaries; an effect-rowed message envelope replacing ad-hoc JSON; port the
  data suite to it in dual JSON-and-native mode during transition.
- **Phase 3: cross-language ingestion (the input half).** A Rust-subset to MIR
  front-end run through the Phase 1 preservation harness, with a written MIR
  coverage-gap report. Gated on operator go/no-go.
- **Phase 4 (deferred, conditional): native and GPU backend productionization.**
  Reopened only against a named consuming product and published graduation criteria.

## Source Of Truth

The dated directive is the posture. STATUS.md states current verified capability.
Roadmap documents describe ambition. None of them overrides this directive's
stratification of what is funded now versus deferred.
