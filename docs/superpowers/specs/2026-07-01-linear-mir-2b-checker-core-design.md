# Design: Linear-on-MIR 2b — Checker core (move/init dataflow + double-consume + move-out-of-borrow)

Status: draft for execution (2026-07-01), branch `feat/linear-mir-checker`. Second of four
specs. Depends on 2a (linearity annotations + span side-table on MIR).

## Summary

Build the MIR affine/borrow checker as a new pass in `codegen/analysis/linear.rs`, consuming
the `codegen::analysis` substrate (CFG, dominators, liveness, move-chains) and the 2a
groundwork (linearity annotations + spans). This sub-brick ships the CORE: a forward
move/init dataflow over linear places that flags **double-consume / use-after-move** and
**move-out-of-a-shared-borrow**, emitting `TypeErrorWithSpan`. It runs ADDITIVELY (the AST
tracker is NOT yet removed; that is 2d), so this sub-brick's job is to catch the direct cases
AND begin closing the open classes without regressing anything. The `match`-idiom and full
generic/precision handling are 2c.

## Motivation

Places, not names. The AST tracker keys a `HashMap<String, LinearSlot>` by binding name and
cannot follow projections, borrows, or generics (`infer.rs:93-125`). MIR makes moves
(`MirRValue::Use(MirValue::Local)`), borrows (`Ref`/`AddressOf { place }`), and move-out-of-
borrow (`Deref { ptr }` / `PlaceProjection::Deref`) explicit and place-addressable
(`ir.rs:649-703`). A dataflow checker over these is both sound (catches the open classes) and
precise (accepts safe compositional code the AST tracker refuses).

## Architecture

`compiler/src/codegen/analysis/linear.rs`, `pub(crate) fn check(func: &MirFunction) ->
Vec<TypeErrorWithSpan>`. Pure function of MIR; no `TypeContext` (linearity comes from 2a
annotations). Consumes `super::cfg::*` and `super::liveness::*`.

**Linear locals.** `is_linear_local(func, id)` = the local's `annotations` contains
`"linear"` (from 2a).

**Move/init dataflow (forward, per linear local).** State per program point: each linear
local is `Init` (owns its value) or `Moved` (its value was consumed). Transfer:
- A **move** of linear local `L` (an rvalue `Use(Local(L))`, or `L` passed by value as a
  `Call` arg, or returned) transitions `L: Init -> Moved` and, if `L` was already `Moved` at
  that point, emits **use-after-move** at that site's span.
- A **borrow** of `L` (`Ref`/`AddressOf` whose `place.local == L`, or a `Deref`/field read of
  `L`) does NOT move `L`, but READS it: if `L` is `Moved` at that point, emit **borrow-after-
  move** (closes class 5). Borrows otherwise leave state unchanged.
- A **(re)definition** of `L` (an `Assign { dest: L }` or `Call { dest: Some(L) }`) resets
  `L -> Init` (a fresh value; e.g. a loop that re-binds each iteration).
Join at CFG merges is conservative: `Moved` if `Moved` on ANY predecessor path (maybe-moved),
so a value moved on one branch is treated as moved after the join. Fixpoint over the CFG
using the substrate's dominators/reachability and the same worklist shape as `liveness.rs`.

**Move-out-of-borrow (closes classes 1 and 3, the direct forms).** A move whose SOURCE place
is a `Deref` of a shared (`is_mut == false`) reference to a linear value is categorically
illegal: emit **move-out-of-shared-borrow**. Concretely, an `Assign { dest, value:
MirRValue::Deref { ptr, pointee_ty } }` (or `Use` of a place with a leading
`PlaceProjection::Deref`) where the referent is linear. "Referent is linear" is determined
from the annotation on the pointee local, or `pointee_ty` being a linear struct name resolved
via 2a's annotation of the borrowed local. This is the `let a = *r` shape
(`lower_deref`, `expr.rs:6019-6044`).

Note: class 3 (generic deref) is closed HERE for the MONOMORPHIZED case, because by the time
a concrete linear local exists in MIR the generic `T` is substituted; the generic body's
`*r` becomes a concrete `Deref` of a linear referent. (Match-through-`&`, class 1's match
form, needs the `VariantField`/idiom handling in 2c; the plain `let a = *r` form is closed
here.)

**Diagnostics.** Emit `TypeError::LinearUseAfterMove { name }` (reuse the existing variant,
`error.rs:190`) or a new `TypeError::LinearMoveOutOfBorrow { name }` variant, wrapped in
`TypeErrorWithSpan` (`error.rs:20-29`) using the 2a span table for the offending site. `name`
comes from `MirLocal.name` (best-effort; MIR keeps debug names). NEVER emit `CodegenError`
(no span field).

## Pipeline wiring (additive, non-removing)

Insert the check right after lowering, before backend codegen, at `codegen/mod.rs:113`
(where `&self.ctx` and the lowered `&mir` are both in scope). Collect the errors. Surface
them as COMPILE ERRORS: the driver (`main.rs`, the `check`/`build` paths) must fail the
compile (non-zero exit, print the diagnostics) when the MIR linear check returns errors. This
establishes the first MIR-phase user diagnostic; gate it behind a run so `buildc check` also
runs it (the check path currently stops at typecheck, `main.rs:4417-4478` — it must now also
lower + run the MIR linear check to report these errors).

**Additive safety.** The AST tracker stays in place this sub-brick. To avoid DOUBLE
diagnostics on cases both catch, this sub-brick runs the MIR checker but the migration off
the AST tracker is 2d. For 2b, accept that a direct double-use may be reported by BOTH (the
AST tracker at the type phase blocks lowering, so the MIR checker never runs on it) — in
practice the AST tracker's pre-lowering block means the MIR checker only sees programs the
AST tracker PASSED, so the MIR checker's job in 2b is exactly to catch what the AST tracker
missed (the open classes) with no double-reporting. Verify this interaction explicitly.

## Data flow

`MirFunction` (with 2a annotations + spans) -> `linear::check` -> `Vec<TypeErrorWithSpan>` ->
`codegen::generate` returns them as errors -> driver prints + non-zero exit. On no errors,
codegen proceeds unchanged.

## Error handling

- Conservative on uncertainty is NOT the disposition here (unlike drop insertion): this is a
  CHECKER, so a missed error is unsound. But 2b is additive (AST tracker still active), so a
  2b gap only leaves an open class open, never regresses. 2c/2d tighten and take over.
- Robustness: lowering must not panic on a linear-invalid program (linearity is a semantic
  overlay; lowering mechanics do not need it). Verify: a program with a deliberate
  use-after-move lowers to MIR and the checker reports it (rather than a lowering panic).

## Testing

- **Unit (dataflow):** construct MIR functions directly (the `bs`/`i64_local` +
  `MirFunction`/`MirBlock` pattern from `codegen/analysis/drops.rs` tests) with a `"linear"`-
  annotated local: (a) moved twice -> use-after-move; (b) moved then borrowed -> borrow-after-
  move; (c) moved out of `*r` (shared) -> move-out-of-shared-borrow; (d) moved once, or moved
  then re-bound in a loop -> NO error; (e) branch where one arm moves, join then use -> use-
  after-move (maybe-moved join).
- **End-to-end (`.bld` via buildc check):** the class-2/4/5 minimal repros from the
  exploration should now FAIL to check (exit non-zero) with a linear diagnostic; the direct
  `linear_*` enforced cases still fail; and every currently-green program + the 8-program
  corpus still checks clean (no false positives).
- **Regression:** full `cargo test` green; `-Dwarnings` clean; `buildc corpus verify` 8/8.

## File touch-points

- New: `compiler/src/codegen/analysis/linear.rs`; register `pub(crate) mod linear;` in
  `analysis/mod.rs`.
- `compiler/src/error.rs`: add `LinearMoveOutOfBorrow { name }` (and possibly
  `LinearBorrowAfterMove { name }`) `TypeError` variants with `#[error(...)]` messages.
- `compiler/src/codegen/mod.rs`: run `linear::check` after `lower_module`, return errors.
- `compiler/src/main.rs`: the `check` and `build` paths surface MIR linear errors (lower +
  check even for `buildc check`).

## Risks

- **`buildc check` now lowers.** Making `check` run the MIR pass means `check` lowers (more
  work). Acceptable; note it. Confirm `check`'s error output includes the new diagnostics.
- **Double-reporting.** Verify the AST-tracker-still-active interaction does not double-report
  (the pre-lowering block should prevent it). If it does, defer the overlapping AST reject to
  2d rather than patch here.
- **maybe-moved join precision.** A too-conservative join could false-positive on safe code.
  2b targets soundness of the direct + borrow/move-out cases; 2c handles the precision cases.

## Dependency

Requires 2a. Enables 2c (match idiom + precision) and 2d (migration).
