# Memory pillar: design and plan (2026-06-30)

The "memory" pillar of the buildc/buildlang foundation is the one substantial
gap remaining after transpiler, effects, and receipts reached runtime-verified
state (the semantic corpus c-execution passes 8/8 under MSVC). This document
records the verified current state and the implementation plan. It exists
because deterministic memory management is correctness-critical: a wrong free is
a use-after-free or double-free, which is strictly worse than a leak, so this
pillar must be designed before it is rushed.

## Verified current state

Compiled programs allocate heap memory and never free it.

- The embedded C runtime (`compiler/src/codegen/runtime.rs`) defines
  `build_string_free`, `build_vec_free`, and `build_hvec_free`, but they are
  dead code: nothing calls them.
- MIR has a `Drop` terminator (`compiler/src/codegen/ir.rs`), and the C and
  LLVM backends both *handle* it, but the MIR builder never *inserts* one. The C
  backend's Drop arm is literally `// No explicit drop in C` followed by a
  `goto` to the target block; the dropped place is ignored.
- Empirical check (2026-06-30): a program that creates three `String`s lowers to
  C with 9 `build_string_new` allocation sites and 0 `build_string_free` calls.
- The GC at `compiler/src/runtime/gc.rs` (refcounting + cycle detection) is a
  Rust model used by the compiler's own analysis. It is not C, so it is not
  what runs inside compiled programs. It is a design reference, not a drop-in
  runtime.

Consequence: short programs run correctly (the OS reclaims everything at exit,
which is why the corpus passes), but any long-running program grows without
bound. A program-exit "free everything" arena would be cosmetic, since the OS
already frees on exit; it does not solve in-flight growth. The real fix is
early reclamation during execution.

## Two design paths

1. Ownership-based drop insertion (RAII, Rust-style). The MIR builder inserts
   `Drop` terminators for owned heap locals at the end of their live range,
   accounting for moves, returns, and by-value passing. The C backend lowers
   each `Drop` to the matching `build_*_free`. Deterministic, zero runtime
   overhead for non-heap code, and it composes with the existing interprocedural
   lifetime analysis. It does not reclaim reference cycles on its own.
2. Tracing or refcounting GC in C. Port the `gc.rs` model into the emitted C
   runtime: an `RcHeader` per heap object, `inc/dec_strong` on assignment, and a
   periodic cycle collector. Handles cycles, but adds per-object headers and
   runtime cost, and threads refcount operations through every assignment in
   codegen.

Recommendation: path 1 first (it matches the stated "pay for what you use,
reference counting for most objects" philosophy in `gc.rs` and the existing
lifetime analysis), then add cycle collection (a bounded path-2 subset) only for
the types that can actually form cycles.

## Soundness rule (non-negotiable)

Drop insertion must be sound by conservatism: free a local only when it is
provably owned and provably dead and provably non-escaping (not moved into
another value, not returned, not passed by value to a callee, not aliased
through a pointer that outlives it). When any of these is uncertain, do not
insert a drop. An uncertain case then leaks, which is acceptable; it must never
corrupt. Correctness dominates completeness here.

## Bounded first sub-step

Insert drops for the clearest sound case and grow coverage from there:

1. Single-owner heap locals (`BuildString`, `BuildVec`) created in a function,
   whose only uses are by-reference field reads (e.g. `.ptr`), that are not
   returned, not moved, and not stored into an escaping aggregate. Free them at
   the end of the function (before each `return` and at fallthrough).
2. Extend to block-scoped locals (free at end of the owning block), then to
   conditional ownership (drop flags) only once the simple cases are proven.

## Verification plan

- Unit: golden MIR/C tests asserting a `Drop` is inserted for a sound case and
  is NOT inserted for an escaping/returned/moved value (the regression that
  prevents corruption).
- Runtime: compile each case with MSVC AddressSanitizer (`cl /fsanitize=address`)
  and run, asserting no use-after-free and no double-free, and that the targeted
  allocations are freed. The semantic corpus c-execution must stay 8/8.
- The pillar is only "done" when a long-running allocation loop has bounded peak
  memory under ASan, not merely when the corpus passes.

## Why this is documented rather than already implemented

The transpiler/effects/receipts pillars were bounded, TDD-verifiable bricks and
were shipped. Drop insertion is a move/liveness analysis whose failure mode is
silent memory corruption. The honest sequence is: register the gap with verified
evidence (this document), pick the sound conservative approach, and implement it
behind ASan verification, rather than ship an unsound analysis under time
pressure. This is the same register-before-claim discipline the rest of the
project runs on.
