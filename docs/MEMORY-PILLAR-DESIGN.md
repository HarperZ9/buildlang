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

### Third increment: block-scoped drops (SHIPPED 2026-06-30, opt-in)

STATUS: shipped in commit b0b9f35 (behind `BUILDLANG_EXPERIMENTAL_FREE`, default
off). The bounded-first-sub-step described below is implemented
(`block_scoped_freeable`, `live_range_confined_to_block`, `sound_owned_candidates`,
`move_source_chain` in the C backend) and verified: the free lands after the use
inside the loop body; a 1,000,000-iteration allocating loop is ASan-clean
(`cl /fsanitize=address`); peak working set drops from 983 MB (leaking) to 3.3 MB
(block-scoped), a 265x reduction at identical output; full lib suite green; corpus
c-execution 8/8 with the flag on and off; the function-exit path is unchanged. A
six-lens adversarial pass found one latent gap (a `.ptr` borrow off a MOVE SOURCE,
not source-reachable today) which is now closed by also scanning the move-source
chain in `live_range_confined_to_block`. The default stays OFF: coverage is the
narrow single-block-confined case (multi-block live ranges still leak, awaiting the
real-liveness sub-step), and a fresh adversarial pass should run in an ISOLATED
worktree (the first run reverted the working tree mid-implementation).

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

### Increment 4 (multi-block reclamation) — ASan verification

Increment 4 (`multi_block_freeable` in `compiler/src/codegen/analysis/drops.rs`,
merged into `current_fn_block_frees` in `compiler/src/codegen/backend/c.rs`,
commit fc982e8) is wired behind `BUILDLANG_EXPERIMENTAL_FREE` and claims owners
whose live range SPANS blocks — exactly the case increment 3
(`live_range_confined_to_block`) declines, because that rule requires every
`.ptr`/field borrow of the owner to occur in the SAME block as the owner's
definition. This section is the empirical proof that increment 4's frees are
sound and reclaim heap on such a program.

**Fixture** (`compiler/tests/mem/multi_block_loop.bld`):

```
fn main() ~ Console {
    let mut i = 0;
    while i < 1000000 {
        let s = i.to_string() + "!";
        if i % 2 == 0 {
            println!("even");
        }
        println!("{}", s);
        i = i + 1;
    }
}
```

The `if i % 2 == 0 { ... }` splits the loop body into distinct basic blocks
between the allocation (`i.to_string() + "!"`, a `build_string_concat` call)
and the use (`println!("{}", s)`), so `s`'s definition and its `.ptr` borrow
land in different blocks by construction.

**Confirming increment 4, not increment 3, is the reclaimer.** Generating C
with the flag off and on and diffing:

```
buildc compiler/tests/mem/multi_block_loop.bld --target c -o off.c   # BUILDLANG_EXPERIMENTAL_FREE unset
BUILDLANG_EXPERIMENTAL_FREE=1 buildc compiler/tests/mem/multi_block_loop.bld --target c -o on.c
diff off.c on.c
```

```
1691a1692
>     build_string_free(s);
```

The generated `main()` (identical between the two files apart from that one
line) lowers to:

```
bb4:
    _4 = build_string_new(__str0);
    _5 = build_string_concat(_3, _4);   // s's buffer allocated HERE (bb4)
    s = _5;
    _7 = (i % 2);
    _8 = (_7 == 0);
    if (_8) goto bb7; else goto bb8;
bb7:
    _9 = __str1; printf(_9); goto bb10;
bb8:
bb9:
    _11 = s.ptr;                         // s's ONLY borrow, in bb9 - a DIFFERENT
    _12 = __str2; printf(_12, _11);      // block than bb4 where s was defined
    goto bb11;
bb10:
    goto bb9;
bb11:
    build_string_free(s);                // the extra free, after the printf use
    _13 = (i + 1); i = _13;
    goto bb1;
```

Reasoning that this is increment 4's free, not increment 3's: `s` is defined
in bb4 and its only `.ptr` borrow is read in bb9. `live_range_confined_to_block`
(increment 3) scans every block other than the defining block for a use of the
owner or its borrow temps and returns `false` on the first hit outside the
def block — the borrow in bb9 (`bi != b`, since bb9's index differs from
bb4's) trips that check immediately, so increment 3 declines `s` by
construction. Increment 4's `multi_block_freeable` instead runs backward
per-local liveness (`buffer_live_in`/`buffer_live_out`) and places the free at
the start of bb11, the block where every backward path from a "buffer is
live and dies here" block (bb9, via the isolated bb9->bb11 edge through the
back-edge merge bb10) is anchored — i.e. exactly the multi-block liveness
placement the increment implements. Since the free appears ONLY with the flag
on, and increment 3's rule provably rejects `s`, this free is increment 4's.

**ASan run** (MSVC BuildTools 2022, `vcvars64.bat` loaded so the ASan runtime
DLL resolves):

```
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
cl.exe /nologo /std:c11 /fsanitize=address /Fe:on.exe on.c
on.exe
```

Result: exit code 0, zero `AddressSanitizer:` lines on stderr (checked across
two independent runs), and complete correct output — 1,500,000 lines
(1,000,000 `"{i}!"` lines + 500,000 `"even"` lines, matching the `i % 2 == 0`
predicate over 1,000,000 iterations), ending on `999999!`. The flag-off build
(`off.c` compiled the same way) is likewise ASan-clean, as expected (a leak
alone does not trip ASan's UAF/double-free detectors). No heap-use-after-free,
no double-free.

**Peak memory.** Two measurements were taken, and they disagree, which is
itself the finding:

1. *Under ASan* (`cl /fsanitize=address`), sampling `Process.PeakWorkingSet64`
   on the running child (Start-Process + a polling loop, PID-scoped, output
   redirected to a file to avoid pipe backpressure): `on.exe` (freeing) peaked
   at **99.8-101.1 MB** across two runs; `off.exe` (leaking) peaked at
   **92.3-92.6 MB** across two runs. The freeing build is NOT smaller under
   ASan — it is marginally larger. This is consistent with ASan's documented
   quarantine behavior (moderate confidence, reasoned from ASan's known design
   rather than inspected in this session): `/fsanitize=address` intercepts
   `free()` and holds freed blocks in a poisoned quarantine (to catch
   use-after-free) rather than returning them to the OS immediately, so
   process-level peak working set under ASan does not reflect the
   allocator-level effect of the extra `build_string_free` call. ASan is the
   right tool for the soundness claim (no UAF/double-free) and the wrong tool
   for the peak-memory claim.
2. *Without ASan* (`cl /nologo /std:c11 /O2`, same `on.c`/`off.c`, same
   measurement method): `on.exe` peaked at **18.79-18.80 MB** across three
   runs; `off.exe` peaked at **34.25 MB** across two runs (a third run's
   overall polling command timed out after collecting consistent samples;
   individual per-run readings were reproducible). This is a real, consistent
   ~1.8x reduction, in the expected direction. It is far more modest than
   increment 3's previously recorded 265x (983 MB -> 3.3 MB): that entry does
   not state its measurement tool, and per-iteration allocation size and
   count were not necessarily identical to this fixture's small
   `i.to_string() + "!"` buffers (a few bytes of payload each), so process
   baseline memory (loaded DLLs, stack, CRT/heap bookkeeping) is a larger
   fraction of the total here, compressing the visible ratio. The direction
   (freeing bounds better than leaking) and the soundness (ASan-clean) are
   the load-bearing claims; the exact multiplier is fixture-dependent and
   should not be read as a general bound.

**Commands used** (measurement methodology, for reproducibility): each `.exe`
was launched via a tiny `.bat` wrapper that redirects stdout/stderr to files
(pipe-based redirection from `Start-Process -RedirectStandardOutput`
deadlocked once output exceeded the pipe buffer for this 1.5M-line program —
worth knowing if this is reproduced), then polled every ~15 ms with
`Get-Process -Name <procname> | Refresh(); .PeakWorkingSet64` until the
process exited, taking the running maximum. Output completeness (exact line
counts and content) was checked after every run to rule out a killed/timed-out
process masquerading as a valid low-memory sample.

**Conclusion.** Increment 4 reclaims a genuinely multi-block-live owner
(verified by direct MIR/CFG reasoning against `live_range_confined_to_block`,
not merely by the C diff) with zero use-after-free and zero double-free under
two independent ASan runs of a 1,000,000-iteration loop. The bounded-peak-memory
claim holds without ASan (freeing peaks ~1.8x lower than leaking, reproducibly)
but does NOT hold as a peak-working-set measurement while ASan's quarantine is
active — that is a limitation of the measurement tool for this specific
metric, not evidence against the free's correctness or effect.

### Increment 5: split-frontier drop flags

Increment 5 (`split_frontier_flag_frees` in `compiler/src/codegen/analysis/flags.rs`,
wired into `compiler/src/codegen/backend/c.rs`'s `generate_function`/
`generate_terminator`/`generate_statement`) is wired behind
`BUILDLANG_EXPERIMENTAL_FREE` and reclaims owners whose death frontier is
SPLIT across conditional edges (used on one arm of an `if`, not the other)
and owners whose ALLOCATION is conditional (def block does not dominate the
frees), the two shapes increments 1-4 decline. Mechanism: a per-buffer
runtime `uint8_t` drop flag in the emitted C, tested and cleared at every
free, additive and disjoint from increments 1-4 (each buffer freed by
exactly one mechanism).

**The soundness invariant (verbatim, carried in both `analysis::flags` and
`backend::c`'s emission code):**

> At every point in the emitted C, `__bl_live_N == 1` implies `L` currently
> holds a fresh allocated buffer that no free site has released. Established
> by: init 0 at declaration; set 1 ONLY immediately after `L`'s unique def
> (an allocating call or a move-acquire of a fresh buffer whose source is
> moved-from and therefore excluded from every free set); cleared to 0
> immediately after every guarded free; `build_string_free` reached only
> under the flag test. The flag guards ALLOCATED (double free, uninitialized
> free). It does not guard LIVENESS: a site must additionally satisfy
> buffer-dead-at-entry (no future use of the owner or any borrow temp on any
> path), which is what forbids use-after-free. Both halves are load-bearing.

**Placement rule.** For each owner not already claimed by increments 1-4:
enroll it as a flag owner unconditionally; compute per-block buffer
liveness and `terminal[b] = buf_in[b] && !buf_out[b]`; a block `S` is a
frontier free site iff it is reachable, not the entry, has a predecessor,
`!buf_in[S]` (the UAF guard), some predecessor is terminal (a real death
happened upstream), and `S` is not re-entrant (no predecessor has `S` in
its dominator set, i.e. no back-edge into `S`). Unlike increment 4 there is
no dominance requirement (the flag covers conditional allocation) and no
uniqueness requirement (multiple sites are fine; the shared flag's clear
makes a later site on the same path a no-op). Every enrolled owner
additionally gets a guarded free at every `Return` (the backstop), which
reclaims paths that bypass every frontier site.

**Two hazards caught in design** (both are why the flag exists rather than
an unguarded free at the naive placement):

1. Return-backstop double free without the clear: a frontier site frees the
   buffer, then the same execution reaches a `Return` whose backstop would
   free it again if the guard did not clear the flag on every free.
2. Uninitialized free without the guard on conditional allocation: an owner
   allocated on only one arm of an `if`, with the other arm never touching
   it; an unguarded free reachable from both arms would free garbage
   (uninitialized `cap`/`ptr`) on the arm that never allocated.

**Verified deviation from the original motivating fixture.** The first
draft of the motivating fixture and of `conditional_alloc.bld` modeled the
would-be leak on `drops.rs`'s synthetic `declines_split_death_frontier`
unit-test CFG: a single owner used (or allocated) on one arm of an `if`,
merging directly at a shared join. Compiling that exact shape and diffing
flag-on against flag-off showed NO flag machinery: increment 4
(`multi_block_freeable`) already claims it, unguarded, and correctly so.
Reading the generated C explained why: every `Call` terminator (`printf`,
here) gets its own continuation block for "control after the call
returns". That continuation block is reached from exactly one predecessor
(the arm that made the call) and lies entirely inside the allocating arm's
own dominance region, so it satisfies increment 4's single-clean-death-
frontier rule (and, for the conditional-allocation case, its dominance
requirement) all by itself, without ever needing to look past the join the
synthetic CFG's clean-chain rejection is designed to catch.

The shape that genuinely defeats increment 4 in real compiled code is
MULTIPLE independently-valid candidate sites for the same owner: when both
arms of an `if` use (or allocate-and-use) the owner, each arm's call
continuation block independently satisfies increment 4's per-block site
rule, so `multi_block_freeable`'s function-wide "exactly one qualifying
block" requirement is violated by having two, and it declines the owner
entirely (leak, safe) even though each site would have been sound in
isolation. Increment 5 has no uniqueness requirement, so it is the only
mechanism that reclaims this owner. The shipped fixtures
(`compiler/tests/mem/split_frontier_loop.bld`,
`compiler/tests/mem/conditional_alloc.bld`) use this multiplicity shape;
`analysis::flags`'s own unit tests separately verify the clean-chain/
dominance-declined shape the original synthetic CFG describes, via directly
constructed MIR. Both are genuine declines of increments 1-4; the real
`.bld` fixtures exercise the one reachable from real BuildLang source
today, and each fixture's header comment records this finding in full.

**C-diff confirmation** (`buildc compiler/tests/mem/split_frontier_loop.bld
--target c -o off.c` / `set BUILDLANG_EXPERIMENTAL_FREE=1` / `-o on.c` /
`fc off.c on.c`):

```
+    uint8_t __bl_live_6 = 0;
...
bb3:
+    if (__bl_live_6) { build_string_free(s); __bl_live_6 = 0; }
     fflush(stdout);
     return 0;
bb4:
     _4 = build_string_new(__str0);
     _5 = build_string_concat(_3, _4);
     s = _5;
+    __bl_live_6 = 1;
     _7 = (i % 2);
     _8 = (_7 == 0);
     if (_8) goto bb7; else goto bb8;
...
bb10:
+    if (__bl_live_6) { build_string_free(s); __bl_live_6 = 0; }
     goto bb9;
bb11:
+    if (__bl_live_6) { build_string_free(s); __bl_live_6 = 0; }
     goto bb9;
```

One declaration, one set after the concat's move-acquire (`s = _5;`), one
guarded free per `if`/`else` arm's continuation block, one guarded Return
backstop. `conditional_alloc.bld`'s diff has the same four-part shape (decl,
set, two frontier sites, Return backstop), with the set landing inside the
conditionally-allocating arm instead of unconditionally in the loop body.

**Double-free negative regression.** Regenerating `on.c` for the pre-
existing `compiler/tests/mem/multi_block_loop.bld` and diffing against its
`off.c` is still exactly the single unguarded `build_string_free(s);` line
from increment 4: no flag machinery. Disjointness holds at the emitted-C
level; a buffer already freed by increment 4 gains no second freer.

**ASan run** (MSVC BuildTools 2022, `vcvars64.bat` loaded):

```
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
cl.exe /nologo /std:c11 /fsanitize=address /Fe:on.exe on.c
on.exe > out.txt 2> err.txt
```

Result: `split_frontier_loop.bld` on.exe, two independent runs, exit 0,
zero `AddressSanitizer:` lines, 1,000,000 lines of output ending
`odd 999999!` (matches the fixture's predicate: 1,000,000 iterations, one
line per iteration). off.exe: exit 0, same 1,000,000 lines, zero
`AddressSanitizer:` lines. `conditional_alloc.bld` on.exe, two independent
runs, exit 0, zero `AddressSanitizer:` lines, 500,000 lines ending
`b 999998!` (500,000 iterations satisfy `i % 2 == 0`; matches the
predicate). off.exe: same, clean.

**Peak memory** (without ASan, `cl /nologo /std:c11 /O2`, `PeakWorkingSet64`
polling, two runs each): `split_frontier_loop` off.exe 34.2-34.6 MB, on.exe
~19.2 MB (about 1.8x lower). `conditional_alloc` off.exe 18.7-19.2 MB,
on.exe 11.0-11.8 MB (about 1.6-1.7x lower). Direction only, as with
increment 4: the multiplier is fixture-dependent.

**Mutation checks** (break each guard, observe red, restore, observe
green; every mutation reverted and reconfirmed byte-identical to the clean
`on.c` before moving to the next):

- Remove `!buf_in[S]`: `uaf_shape_declines_live_join` and the full
  `codegen` test suite stayed GREEN. This is not a gap: `buf_out[p]` is
  defined as the union of `buf_in` over ALL of `p`'s successors, so
  `terminal[p] == true` mathematically implies `buf_in[S] == false` for
  every direct successor `S` of `p` (if some successor still needed the
  buffer, that need would flow into `buf_out[p]` via the union, making
  `p` not terminal). The `some predecessor is terminal` check already
  implies the UAF guard for any site reachable through this rule; removing
  `!buf_in[S]` cannot produce a counterexample under the current, correct
  liveness formalism. The check is KEPT (defense in depth against a future
  change to `terminal`'s definition) but this is recorded as a finding
  against the plan's assumption, not a red/green pair: it does not
  independently falsify.
- Remove the `claimed` skip: `claimed_owner_not_enrolled` RED (a claimed
  owner got enrolled, violating disjointness). Restored, GREEN.
- Remove the terminal-pred requirement: `no_site_without_terminal_pred`
  RED (clean blocks downstream of a non-terminal predecessor wrongly
  re-qualified). Restored, GREEN.
- Remove the re-entrant exclusion: `declines_reentrant_site` RED (a loop
  header wrongly became a site). Restored, GREEN.
- Remove the `__bl_live_N = 0;` clear: helper exact-text test RED.
  Compiling and running `split_frontier_loop.bld` under ASan with the
  mutation: `AddressSanitizer: attempting double-free`, exit 1 (the
  frontier site frees, the Return backstop frees the same buffer again on
  the same path). Restored; helper text test GREEN; regenerated `on.c`
  byte-identical to the clean version; re-ran ASan clean.
- Change the flag initializer `= 0` to `= 1`: helper/decl text RED (both
  the emitted declaration and, separately, the exact-text helper test).
  The natural compiled-fixture repro (a program whose allocating arm never
  runs) depended on whatever garbage happened to occupy the never-assigned
  local's stack slot; on the test machine that garbage read back as
  `cap == 0`, so `build_string_free`'s own `if (cap > 0)` guard silently
  no-oped it (still undefined behavior, but not one MSVC ASan's UAF/
  double-free/invalid-free detectors are positioned to catch without
  MemorySanitizer-style uninitialized-read detection). A second,
  deterministic harness reproducing the identical emitted guard pattern
  with a REAL stale pointer left in the reused stack slot (via ordinary
  stack-frame reuse across two calls) hit
  `AddressSanitizer: attempting free on address which was not
  malloc()-ed`, exit 1: bad-free. Restored; both text checks GREEN;
  regenerated `on.c` byte-identical to the clean version.
- Remove the def-site `__bl_live_N = 1;` emission (move-acquire arm, the
  one the shipped fixtures exercise): C-diff check RED (the set line is
  absent from the expected on-diff). This mutation fails SAFE: the flag
  never becomes 1, so every guarded free is a permanent no-op and the
  buffer leaks every iteration; no corruption. Restored; C-diff
  byte-identical to the clean version.

**Adversarial pass** (six lenses, isolated `git worktree`, never the main
tree; removed after): every lens either found no counterexample or found
something that resolves, on rigorous analysis, to a documented and
harmless margin.

1. Flag-invariant attack: no counterexample. `sound_owned_candidates`'s
   `def_count == 1` gate, combined with how `owner_def` is populated
   (only an allocating-Call dest or a move-acquire from an existing
   owner), makes it structurally impossible for the SET emission to fire
   on anything other than the owner's true unique definition.
2. Borrow-overlay blindness: no counterexample. `owned_string_escapes`'s
   exhaustive `rvalue_mentions` match rejects `Ref`/`AddressOf` and any
   non-`FieldAccess` mention before a candidate ever reaches
   `analysis::flags`; the multi-hop-`.ptr`-copy shape the escape check
   defends against is confirmed NOT reachable from surface BuildLang
   syntax today (`.ptr` is not a user-accessible field), matching the
   existing "latent: not currently source-reachable" comment in `cfg.rs`.
3. Move-chain aliasing: no counterexample. A source moved conditionally
   into two different destinations on mutually exclusive arms is tainted
   by the pre-existing (increment-2) multi-acquirer guard regardless of
   control flow, excluding both destinations from `sound_owned_candidates`
   entirely; verified on a real 1,000,000-iteration compiled program that
   emits zero free machinery for the tainted owner (leak, safe).
4. Re-entrance: the dominator-based re-entrant exclusion, as written, does
   not catch every structurally re-entrant block (a body block reached by
   a single forward edge whose own successor loops back to the header is
   not flagged by "does some predecessor's dominator set contain this
   block"). This is not a soundness gap: any such block either fails the
   `buf_in[S] == false`/terminal-predecessor checks via the same
   liveness-fixpoint argument that makes the mutation-1 finding hold (the
   buffer being genuinely needed again next iteration makes the fixpoint
   correctly mark it live), or, if it does qualify, the runtime GUARD
   (not the site-selection logic) makes a repeat execution of an
   already-cleared flag a safe no-op. Confirmed empirically on a nested-
   loop nightmare stress case (2,000 x 500 = 1,000,000 inner iterations,
   increments 4 and 5 both active) under ASan: exit 0, zero
   `AddressSanitizer:` lines, all 1,000,000 lines correct. The kept
   exclusion is therefore a scope-bounding choice matching increment 4's
   rule verbatim, not a load-bearing soundness gate for increment 5;
   relaxing it remains a named follow-up, not this brick.
5. Return-backstop interaction: no counterexample. Disjoint from the
   unguarded fn-exit frees by construction (the `claimed` set excludes any
   overlap). Found a minor, harmless REDUNDANCY: when a frontier site and
   a `Return` land in the same block (e.g. an early `return` right after
   the site), two guarded-free statements for the same owner are emitted
   back to back; the second is always a no-op (the flag is already 0).
   Verified sound under ASan on a 500,001-line early-return stress case
   (exit 0, zero `AddressSanitizer:` lines). Not fixed: it is pure
   emission verbosity with zero soundness effect.
6. Special-case Call emission paths: no counterexample. Every special-case
   string comparison in the Call arm's generic tail (`__vtable_dispatch_`,
   the `intrinsic_*` remap, `Vec_new`/`String_new`/`String_from`/`clone`/
   `None`/`Some`/`Ok`/`Err`/the unit-struct constructors/`fn_returns_void`)
   was re-enumerated exhaustively; none can ever equal one of the closed
   6-name `allocates_owned_string` list, so an allocating callee always
   reaches the generic tail where the def-site set is emitted.

**Kept re-entrant exclusion.** As lens 4 above establishes, the flag's
runtime guard would make a re-entrant site SOUND (the guard blocks the
second free); the exclusion is kept verbatim from increment 4 to bound
this brick's adversarial surface to exactly what is already audited and
tested, per the plan. Relaxing it to cover loop-header/self-loop death
frontiers (the largest remaining decline, per-iteration leaks in a loop
whose only death frontier is its header) is a named, scoped follow-up.

**Verification bar.** Unit: 12 new tests (6 in `analysis::flags`, 2 new
plus 1 extended in `backend::c`), full suite 1,613 passed / 0 failed / 11
ignored (parent baseline 1,605 / 0 / 11). C-diff: both new fixtures and the
double-free negative regression confirmed. ASan: clean on both new
fixtures, two runs each, plus three additional adversarial-pass ASan runs
(nested-loop stress, early-return stress) all clean; two mutation ASan
runs both caught the intended hazard (double-free, bad-free). Corpus:
`buildc corpus verify` 8/8 with the flag on and off. Byte-identity: flag
off, zero differences across both new fixtures, the pre-existing
`multi_block_loop.bld`, and two semantic-corpus programs, generated at the
parent commit and at this increment.

## Why this is documented rather than already implemented

The transpiler/effects/receipts pillars were bounded, TDD-verifiable bricks and
were shipped. Drop insertion is a move/liveness analysis whose failure mode is
silent memory corruption. The honest sequence is: register the gap with verified
evidence (this document), pick the sound conservative approach, and implement it
behind ASan verification, rather than ship an unsound analysis under time
pressure. This is the same register-before-claim discipline the rest of the
project runs on.
