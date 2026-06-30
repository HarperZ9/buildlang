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
  C with 9 `build_string_new` calls and 0 `build_string_free` calls. CORRECTION
  (2026-06-30, verified by reading `runtime.rs`): `build_string_new` does NOT
  allocate. It wraps the input pointer with `cap = 0` ("literal, not owned"), and
  `build_string_free` only frees when `cap > 0`. So string literals and
  `String::from(<literal>)` (which lowers to `build_string_new(arg.ptr)`) are
  zero-cap wrappers: they do not leak and are no-ops to free. The earlier framing
  that counted `build_string_new` sites as leaks was wrong. The genuine heap
  allocations (cap > 0, returned by `malloc`) come from DERIVED strings:
  `build_string_concat`, `build_format_str`/`build_format_i32`/`build_format_f64`
  (and `build_i32_to_string`/`build_f64_to_string`), `build_read_file`/`_bytes`,
  the `build_string_to_upper`/`_lower`/`_trim`/`_substring`/`_replace`/`_repeat`/
  `_char_at`/`_from_cstr` transforms, and `build_args_get`/`build_read_line`/
  `build_read_all`/`build_tcp_recv`/`build_getenv`. Two BuildString-returning
  runtime functions return an ALIAS into a container, not a fresh buffer
  (`build_hvec_get_str` returns `*(BuildString*)build_vec_get(...)`, and
  `build_hmap_get_str_str`); these must never be in the freeable set or they
  double-free the container's buffer.
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
  CONFIRMED 2026-06-30: this MSVC has working ASan; a deliberate use-after-free
  compiled with `/fsanitize=address` reports `AddressSanitizer: heap-use-after-free`
  at runtime (run the exe with the MSVC bin on PATH so the asan runtime DLL
  resolves). The safety net for drop insertion is therefore ready to use.
- The pillar is only "done" when a long-running allocation loop has bounded peak
  memory under ASan, not merely when the corpus passes.

## Concrete implementation findings (2026-06-30)

Investigated the MIR surface to scope the first increment precisely:

- There is no liveness or scope infrastructure to lean on. `MirTerminator::Drop`,
  `MirStmtKind::StorageLive`, and `MirStmtKind::StorageDead` are all defined and
  have builder helpers, but the lowering never emits them. So drop placement
  must be computed fresh, not read off the MIR.
- Heap allocation is a `Call` terminator: `L = build_string_new(...)` is
  `MirTerminator::Call { dest: Some(L), .. }`. The runtime `build_string_free`
  is self-guarding (`if (cap > 0) free(...)`), so freeing a literal-backed or
  non-heap BuildString is a safe no-op. This narrows the real hazard to two
  cases: freeing a moved-from local (double-free) or an uninitialized local.
- A function-exit free (free at each `Return`) avoids per-scope liveness: it
  needs only a whole-function escape scan, not a CFG dataflow.

### Status: first increment SHIPPED (2026-06-30, opt-in)

The drop-insertion framework is implemented in the C backend behind the
`BUILDLANG_EXPERIMENTAL_FREE` flag (default off): `freeable_owned_string_locals`
(the conservative analysis), `local_is_referenced` (the complete use scan, with
the rvalue/statement matches compiler-verified exhaustive and the `Assert`
terminator covered), and emission of `build_string_free` before each `Return`.
Verified: 3 analysis unit tests; full Rust suite green; and the semantic corpus
c-execution stays 8/8 with the flag ENABLED (so the drops it does emit are sound
on real programs). Coverage is intentionally narrow for now (see below) and
reclaims little in practice yet; the value is a sound framework + verification
loop to broaden incrementally.

Two follow-ups surfaced: (a) broaden coverage - the entry-block-only definite-init
rule frees at most the first heap local (each allocating `Call` splits the
block), so the next step is dominance-based definite-init plus the
known-non-retaining-call whitelist; (b) owned-`String` programs did not compile -
RESOLVED 2026-06-30 (see below).

### Owned-String compile gap: RESOLVED 2026-06-30

`let s = String::from(x)` had two distinct defects, both now fixed:

1. Codegen emitted an undefined `String_from` symbol. Fixed in 915752f by
   special-casing `String_from` in the C backend to `build_string_new(<arg>.ptr)`,
   exactly like `String_new`.
2. The dest local was still typed `int32_t`, because `resolve_call_return_type`
   (the lowering name->MIR-type map in `codegen/lower/expr.rs`) had a `String_new`
   arm but no `String_from` arm, so it fell through to the `i32` fallback. The
   emitted C was therefore `int32_t s; s = build_string_new(...)` - a real C2440
   (`cannot convert from 'BuildString' to 'int32_t'`) under a C compiler. Fixed by
   adding `String_from` to that arm so the dest is typed `BuildString`.

Correction to the 915752f commit note: that note attributed the still-failing
`cl` compile to a "sandbox overlay-FS view mismatch (stale binary)". That was a
misdiagnosis. The C2440 was defect (2) above - a genuine remaining
lowering-type-inference gap, not a stale binary. 915752f was a correct but partial
fix; the type-inference arm completes it.

Verified end-to-end: a `String::from` + `println!` program now emits
`BuildString s;`, compiles under `cl` with exit 0 (only benign C4090 const-qualifier
warnings on the printf-arg copy), and prints `hello`. The semantic corpus
c-execution stays 8/8 both with and without `BUILDLANG_EXPERIMENTAL_FREE`. A
golden test (`string_from_dest_is_typed_buildstring_not_int`) asserts every
`build_string_new` dest is declared `BuildString`, never `int32_t`, so the
regression cannot silently return. Owned strings can now be the subject of future
drop-insertion coverage.

### First increment (narrow, sound, opt-in)

Free a `BuildString` local at every `Return` iff: it is non-parameter; it is the
`dest` of an allocating `Call` in the entry block (block 0, so definitely
initialized); and it is never referenced anywhere else in the function (so it is
not moved, aliased, returned, or read). Such a local uniquely owns a buffer
nothing else touches.

The soundness of this rests entirely on the local-use scan being COMPLETE: it
must report a reference if the local appears in ANY `MirValue::Local`,
`MirPlace.local`, or projection across every statement and terminator. A single
missed variant frees a live value. Because that scan is miss-intolerant, the
first increment ships behind an opt-in flag (default off) so the verified
baseline (corpus c-execution 8/8, all current programs) stays on the existing
no-free path while the opt-in path is proven with `cl /fsanitize=address` on a
growing test set. Coverage then broadens (allow uses that are only field reads
flowing to known non-retaining functions like `printf`/`build_print_*`; then
block-scoped drops with definite-init flags) one ASan-verified step at a time.

### Second increment: move-aware ownership (MIR-grounded, 2026-06-30)

Inspecting the actual MIR for `fn main() ~ Console { let s = String::from("hello"); println!("{}", s); }`
corrected the planned "non-retaining-call whitelist" next step: the real blocker
is not borrowing, it is MOVE-ALIASING. The lowered MIR is a three-local chain:

- `_1 = build_string_new("hello")`  (Call dest, block 0) - buffer A, intermediate
- `_2 = String_from(_1)`            (Call dest, block 1) - buffer B, a fresh copy
- `s = Use(_2)`                     (Assign,    block 2) - STRUCT COPY: `_2` and `s`
  now hold the same `.ptr`, i.e. they ALIAS buffer B
- `_4 = s.ptr; printf(fmt, _4)`     (field read feeding a non-retaining call)

So a naive "free every owned BuildString" frees buffer B twice (via `_2` and via
`s`): a double-free. The `let` binding is a move at the language level (BuildString
is move-only, so the checker forbids use-after-move), but at MIR/C level it is a
struct copy that creates a transient alias. Sound reclamation therefore needs MOVE
TRACKING, not just a borrow whitelist.

Bounded sound rule (the second increment, still opt-in, ASan-gated). Free an
owning BuildString local `L` at every `Return` iff ALL hold:

1. `L` is non-parameter and typed `BuildString`.
2. `L` is OWNING: it is the `dest` of a Call to a known ALLOCATING runtime
   function that returns a FRESH, solely-owned `cap > 0` buffer
   (`build_string_concat`, `build_format_str`/`_i32`/`_f64`,
   `build_i32_to_string`/`build_f64_to_string`, `build_read_file`/`_bytes`, the
   `build_string_*` transforms, `build_args_get`/`build_read_line`/`build_read_all`/
   `build_tcp_recv`/`build_getenv`), OR it is move-acquired by `Assign { dest: L,
   value: Use(src) }` where `src` is itself an owning BuildString. NOTE the
   allocating set deliberately EXCLUDES `build_string_new` and `String_from` (they
   return `cap = 0` wrappers, so there is nothing to free) and the container-alias
   getters `build_hvec_get_str`/`build_hmap_get_str_str` (freeing them would
   double-free the container).
3. Definite init: `L`'s defining block dominates every `Return` block (so `L` is
   initialized on every path to a free; this matters because `build_string_free`
   only self-guards on `cap`, and an uninitialized BuildString has garbage `cap`).
4. `L` is NOT moved-from: there is no `Assign { value: Use(L) }` transferring `L`'s
   buffer to another owner (if there is, that other owner is freed instead; `L` is
   excluded - this is the alias guard that prevents the double-free above).
5. `L` does not ESCAPE. Every use of `L` other than its definition is exactly one
   of: (a) a direct argument to a whitelisted BORROW call (reads, never retains or
   frees the arg: `String_from`, `printf`, `build_print_*`, `build_string_len`,
   `build_string_eq`, ...); or (b) a `FieldAccess { base: L, field: ptr|len|cap }`
   into a temp `T` where `T` is a non-aggregate scalar/pointer whose every use is
   itself a whitelisted borrow-call argument (one-hop taint: `L -> T -> borrow`).
   Any other appearance (returned, address-taken, stored into an aggregate or
   field, passed to a non-whitelisted call, or a `T` that escapes) means `L`
   escapes and is NOT freed.

Each heap buffer is then freed exactly once: an alloc-defined local that is later
moved-from is excluded by (4); its move destination (move-acquired by (2)) is the
sole freer. The borrow whitelist in (5) is the ONLY trust surface - every function
on it must be audited to read-but-never-retain-or-free its BuildString/`.ptr`
argument; when in doubt, leave it off (the local then leaks, which is safe).

SCOPE of this increment: freeing at `Return` reclaims heap strings in STRAIGHT-LINE
code (e.g. a function that builds a formatted/concatenated string and prints it).
It does NOT bound a loop that allocates per iteration, because the frees land at
function exit, not at end-of-iteration. Bounding loop memory is the THIRD increment
(block/scope-scoped drops with definite-init flags), which builds on the same
owning/move/escape machinery. This increment is the sound foundation for that, not
the finish line for the "bounded peak memory under a loop" done-criterion.

Verification bar for this increment (must all pass before the flag default flips):
golden unit tests that FREE the simple owned case and do NOT free each unsound
case (moved-out/returned, stored-into-Vec, aliased, escaping `.ptr`); an ASan
battery (`cl /fsanitize=address`) over those same programs asserting zero
use-after-free and zero double-free AND that a long allocation loop has bounded
peak memory; corpus c-execution stays 8/8 with the flag on; and an adversarial
pass that actively tries to construct a program the rule mis-frees.

### Adversarial audit of the second increment (2026-06-30)

A six-lens adversarial pass (move-aliasing, container-aliasing, borrow-whitelist
trust, field-read taint, control-flow/dominance, string-method aliasing) attacked
the implemented analysis, each lens trying to construct a program that emits an
unsound free. It found one runtime-confirmed double-free and three latent issues;
all are now fixed or guarded, each with a regression test:

1. MULTI-MOVE-ACQUIRER DOUBLE-FREE (real, ASan-confirmed; FIXED). The move alias
   guard assumed each move source has exactly one acquirer. `let p = c; let q = c;`
   (a use-after-move the front end does not reject at codegen) moves `c` into two
   destinations that alias one buffer; both were freed. Fix: a source moved into
   more than one destination taints all its acquirers (propagated along move
   edges); tainted owners are never freed. Verified: both counterexamples now emit
   zero frees and run ASan-clean, while the single-move case still frees once.

2. STATIC-MUT STASH (latent UAF; GUARDED). Storing an owner or its `.ptr` into a
   `static mut` global escapes, but that store is currently DROPPED by a separate
   lowering gap, so the per-function scan cannot see it - "sound by accident."
   Fix: if the module declares any mutable global whose type could hold a heap
   string alias (pointer, struct, Vec, Map, tuple, ...), the drop analysis is
   disabled module-wide. Sound by construction, independent of the lowering gap.

3. DOMINATOR OVER-CONSERVATISM (fail-safe; FIXED). `compute_dominators` intersected
   over unreachable predecessors, erasing real dominators and suppressing most
   sound frees (the lowering routinely emits unreachable blocks). This only ever
   caused spurious leaks, never an unsound free, but it gutted reclamation. Fix:
   intersect only reachable predecessors. Verified: an early-return program that
   previously freed nothing now frees its entry-block owner at both returns,
   ASan-clean.

4. BORROW-WHITELIST WILDCARD (hardening). `borrows_string_arg` trusted any name
   matching `build_print*` by prefix. Replaced with a closed, line-by-line-audited
   list (no wildcard); adding a runtime function no longer auto-trusts it.

The container-aliasing, borrow-whitelist-trust, and string-method lenses produced
NO constructible unsound free: container get-back aliases are excluded from the
owner set, container insert callees are non-borrows (so the owner escapes), the
whole-function escape scan is order-insensitive, and every string method mallocs a
fresh buffer (never aliases its receiver) and is absent from the allocating set.

UPDATE (2026-06-30): the `static mut` stash coupling is now closed at the source.
Assigning to a module global/static previously dropped the store silently
(`lower_assign` only resolved local targets); it now FAILS CLOSED with a clear
`CodegenError::Unsupported` (commit 79e765e). So a program that would stash an
owned string into a global no longer compiles and cannot reach the drop analysis
at all. The module-wide mutable-global guard remains as defense in depth. When full
global-store SUPPORT lands (a cross-backend MIR store form), `owned_string_escapes`
must treat a global-target store as an escape before that guard is narrowed.

Remaining HARD GATE before the flag default may flip: the function-exit scope is
lifted to block/scope-scoped drops so a loop has bounded peak memory (the third
increment). Until then the flag stays off by default; the verified baseline is
untouched.

### Third increment: block-scoped drops (MIR-grounded design, 2026-06-30)

The remaining gate. Function-exit drops do not bound a loop that allocates per
iteration, because the frees land at the `Return`, not at end-of-iteration. The
fix is to free a loop-body owner at the end of its scope so each iteration's buffer
is reclaimed.

MIR ground truth (`while i < 3 { let a = String::from("ab"); let b = String::from("cd");
let s = a + b; println!("{}", s); i = i + 1; }`):

- bb1 is the loop header (`if i < 3 -> body / exit`); the body is bb2..bb9 with a
  single back-edge bb9 -> bb1.
- The only heap allocation is `_9 = build_string_concat(_5, _8)` in bb7 (everything
  else is a `cap = 0` literal wrapper). `s` (`_10`) move-acquires it in bb8
  (`_10 = Use(_9)`), is last used in bb8 (`_11 = _10.ptr; printf(fmt, _11)`), and is
  dead across the back-edge (not used in bb9, not live-in to bb1).
- The function-exit pass cannot free `_10`: its defining block bb8 does not dominate
  the return (bb3). So `_10` leaks one buffer per iteration today.

CRITICAL PLACEMENT HAZARD (caught in design, 2026-06-30 - records why the naive
placement is a use-after-free). The obvious idea "free `_10` at the END of bb8,
after its statements, before the terminator" is UNSOUND. bb8 is
`_10 = _9; _11 = _10.ptr; <terminator: printf(fmt, _11)>`. The statement
`_11 = _10.ptr` copies the BUFFER POINTER (a borrow), and the terminator `printf`
READS through `_11`. Freeing `_10` before the terminator frees the buffer `_11`
points into, so the very next instruction (the print) is a heap-use-after-free.
The borrow flows INTO the terminator, so the free must land AFTER the terminator
runs, not before it. Any block-scoped scheme that frees at end-of-statements while
a `.ptr` borrow of the owner is consumed by the terminator is wrong - and the
print case (the whole point) is exactly that shape.

Corrected sound placement: free `L` at the START of `B`'s successor `S`, only on an
ISOLATED edge `B -> S` (B has exactly one successor S, and S has exactly one
predecessor B). Then the print terminator has already run (the borrow is consumed),
`S` is reached exactly once per execution of `B`, and `L` plus all its `.ptr`
borrows are dead at `S` (verified, not assumed). For the loop, bb8's only successor
is bb9 and bb9's only predecessor is bb8, so the free lands at the start of bb9
(inside the loop body, after the print) - reclaiming the buffer each iteration with
no UAF and no double free.

Tightest provably-sound first sub-step (ADDITIVE; does NOT touch the verified
function-exit path, so no buffer can be freed twice). Free an owned heap local `L`
at the START of block `S` iff:

1. `L` passes ALL the second-increment ownership/escape/move/taint/one-def gates
   (the same `sound owned` predicate), and `L` is NOT in the function-exit free set
   (disjointness -> no double free).
2. `L` is DEFINED in a block `B`, and every USE of `L` AND of every `.ptr`/field
   borrow temp derived from `L` is within `B` (statements or terminator) - never in
   another block. (Confining the live range of `L` and its borrows to `B` makes "L
   and its borrows are dead after B" hold without a full liveness pass: a path back
   to a use re-executes the def at `B` first.)
3. `B` has exactly one successor `S` and `S` has exactly one predecessor `B` (an
   isolated edge), so freeing at the start of `S` runs once per `B` and after `B`'s
   terminator has consumed any borrow.

The free is emitted at the START of `S`'s statements. Coverage is intentionally
narrow (single-block live range + isolated successor edge); it already captures the
dominant loop pattern (allocate, read once via `.ptr` into a print, discard). Later
sub-steps add real per-local liveness (live_out via backward dataflow, tracking the
borrow temps too) to free locals whose live range spans blocks or whose successor
edge is not isolated (needs edge-splitting / drop flags) - each ASan-verified.

Verification bar (all required before the flag default may flip): golden unit tests
that place the free inside the loop-body block for the print-temp and place NONE for
a local that is used after the block / moved out / carried across the back-edge; an
ASan run of a million-iteration allocating loop showing zero use-after-free, zero
double-free, AND bounded peak working set (the free is structurally inside the loop
body); corpus c-execution stays 8/8 with the flag on; and a fresh adversarial pass
(the second increment passed unit + self-battery + ASan yet still had a real
double-free that ONLY the six-lens adversarial workflow caught - block-scoped drops
has a larger soundness surface and must clear the same bar, not a smaller one).

## Why this is documented rather than already implemented

The transpiler/effects/receipts pillars were bounded, TDD-verifiable bricks and
were shipped. Drop insertion is a move/liveness analysis whose failure mode is
silent memory corruption. The honest sequence is: register the gap with verified
evidence (this document), pick the sound conservative approach, and implement it
behind ASan verification, rather than ship an unsound analysis under time
pressure. This is the same register-before-claim discipline the rest of the
project runs on.
