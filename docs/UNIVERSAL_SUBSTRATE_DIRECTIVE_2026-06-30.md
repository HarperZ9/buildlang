# Universal Substrate Directive - 2026-06-30

Status: in force. This is the named source of truth for the project's posture and
direction. Where any in-tree document disagrees with this directive, this directive
governs until a later dated successor replaces it.

## Supersession Notice

This directive supersedes `docs/UNIVERSAL_SUBSTRATE_DIRECTIVE_2026-06-29.md`, which is
kept in the tree with a banner marking it superseded so the provenance trail stays
honest. The 06-29 directive stratified the native/GPU backends (x86-64, ARM64, WASM,
SPIR-V, LLVM) as deferred, reopening only against a named consuming product. This
directive lifts that deferral as an explicit, funded operator decision: the project
returns to full-speed development across the whole language, sequenced (not parallel)
and gated by the same register-before-claim discipline the rest of the project runs on.

The lift is a decision about what is *funded and sequenced*, not a claim about what is
*done*. The honest baseline below is unchanged: only the C path is verified end to end.
Reopening a lane means it is sanctioned to work on and to graduate through published
criteria, not that it is production-ready today.

## Identity (2026-06-30)

buildlang is **the honest scientific language**: Julia-class ergonomics for scientific,
numerical, and research computing, *fused with* a property Julia structurally cannot
add without a type-system rewrite. That property is buildlang's existing and
differentiating asset:

- first-class typed **algebraic effects** and **capability accountability** (ambient IO
  is a typed, receipt-backed, policy-gated effect), and
- **linear / no-cloning types** (`#[linear]`), the shared foundation for quantum qubit
  no-cloning, on-chain no-double-spend, and fin-sec resource-handle safety.

The strategic thesis is not "out-Julia Julia on Julia's home turf." Julia's real moat is
a fifteen-year package ecosystem, not five language features, and each of those features
is independently a multi-quarter-to-multi-year effort. The winning position is: be the
language a scientist *chooses* for work that wants both numerical ergonomics **and**
provable accountability or no-cloning guarantees. Effect-typed multiple dispatch and
linear resource handles are combinations Julia does not have.

## Target Pillars (both, sequenced)

The program pursues Julia's five pillars **and** deepens the effects/accountability/linear
wedge. Sequenced by leverage times tractability, all built on the verified C path so no
pillar is blocked on the deferred backend slog:

- **Foundation (first, active now):** memory reclamation (sound Drop insertion) and
  linear-type soundness, built on one shared MIR dataflow substrate. See
  `docs/superpowers/specs/2026-06-30-mir-affine-foundation-design.md`. Rationale: both
  `docs/MEMORY-PILLAR-DESIGN.md` and `docs/LINEAR-TYPES.md` independently identify the
  same missing machine (a real MIR liveness / affine analysis), and scientific programs
  allocate heavily, so the heap-leak and the no-cloning soundness gaps must close before
  feature pillars pile on top.
- **Pillar A - Multiple dispatch.** Method table (multiple methods per name), dispatch
  resolution over full argument-type tuples, static resolution on the C path plus a
  dynamic shim. Composes with effects into effect-typed multiple dispatch.
- **Pillar B - Mathematical syntax.** A real `Array{T,N}` and matrix algebra in the
  standard library, broadcasting desugar (`.+`), and Unicode / LaTeX operators in the
  lexer.
- **Pillar C - Runtime execution via a real LLVM JIT.** Operator decision (2026-06-30):
  pursue an in-process LLVM JIT (ORC, per-signature type specialization, runtime
  machine-code emission), the literal Julia execution model. This is chosen with eyes
  open: it is a multi-person-year effort with heavy MSVC / LLVM-FFI integration risk on
  Windows, and it does not retire the C path. The C-AOT path remains the production
  anchor until the JIT graduates through published criteria. This pillar is sequenced as
  its own program, not a near-term deliverable, and must not be described as a quick win.
- **Pillar D - Built-in parallelism.** Threads / atomics / SIMD in emitted C, then GPU
  (the SPIR-V backend is the seed), then distributed. Near-term honest win: a threads /
  OpenMP primitive that actually reaches a compiled program.
- **Pillar E - Interoperability.** The existing native C-ABI FFI is the strongest pillar
  today; the near-term honest win is to move it from "text-asserted" to "end-to-end
  executed and gated" by adding an executing FFI program (for example `puts` / `sqrt`) to
  the semantic-corpus release gate. Python / R / Fortran interop is a separate, larger
  program.

## Honest Baseline (what is actually verified, 2026-06-30)

- Front-end is strong and real: lexer, parser, Hindley-Milner inference with first-class
  typed algebraic effects, traits, const generics, higher-kinded types, interprocedural
  lifetimes. ~1,313 cargo tests pass (lib 872 / bin 44 / cli 263 / lexer 51 / parser 83,
  0 failed), warning-clean under `RUSTFLAGS=-Dwarnings`.
- **Only the C backend is verified end to end.** Rust-subset, x86-64, ARM64, WASM, LLVM,
  SPIR-V, HLSL, GLSL emit a representation but are not a production path.
- MIR is a type-complete SSA IR, in-memory only. It has `Drop`, `StorageLive`, and
  `StorageDead` forms but the builder never emits them, and there is no liveness pass.
- Memory reclamation is opt-in and narrow: three increments behind
  `BUILDLANG_EXPERIMENTAL_FREE` (default off) reclaim single-block-confined owned strings
  (a 1M-iteration loop went 983 MB to 3.3 MB, ASan-clean), but multi-block live ranges
  still leak. Closing this is the foundation brick.
- `#[linear]` no-cloning is experimental and **not yet sound**: a long list of escape
  classes is enforced, but five remain open (pattern-match-through-a-borrow,
  enum-variant shorthand init, generic deref / result, match-guard fall-through,
  borrow-after-move). Soundness needs the MIR affine checker.
- No JIT of any kind exists today. No multiple dispatch (same-name functions overwrite).
  No general array/matrix algebra or broadcasting. Parallelism does not reach compiled
  programs.

## Source Of Truth

The dated directive is the posture. STATUS.md states current verified capability. Design
specs under `docs/superpowers/specs/` describe the sequenced bricks. Roadmap documents
describe ambition. None of them overrides this directive. The next dated directive
supersedes this one.
