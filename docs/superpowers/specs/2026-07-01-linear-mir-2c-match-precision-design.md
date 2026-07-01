# Design: Linear-on-MIR 2c — Match idiom, classes 1/3 fully, and precision

Status: draft for execution (2026-07-01), branch `feat/linear-mir-checker`. Third of four
specs. Depends on 2b (checker core).

## Summary

Extend the 2b MIR checker to (1) recognize buildlang's `match` lowering idiom so moves and
borrows inside match arms/guards are seen, closing class 1 (match-through-`&borrow`) and
class 4 (guard fall-through) fully; (2) confirm class 3 (generic deref/result) is closed by
the monomorphized `Deref` handling; and (3) deliver the PRECISION payoff: accept the safe
compositional programs (tuples, `Option<Linear>`/`Ok(q)`, generic pass-through, closures)
that the name-keyed AST tracker currently over-rejects. Still additive to the AST tracker
(removal is 2d), but by end of 2c the MIR checker is a proven SUPERSET of the AST tracker's
soundness plus the 5 classes, which 2d relies on.

## Motivation

Two structural facts from the exploration:
- **`match` does not lower to `Switch`.** `lower_match` (`expr.rs:3906`) reads the tag as
  `FieldAccess { field_name: "tag" }` (`expr.rs:4300-4305`), compares with `BinOp::Eq`
  (`expr.rs:4307-4313`), and branches with a chain of `MirTerminator::If` (`expr.rs:4130`).
  `Switch`/`Discriminant` exist but AST->MIR emits them only from the effect-handler path
  (`macros.rs:701`). A MIR checker must recognize the `tag + Eq + If`-chain, or match-lowering
  must be upgraded to real `Switch`. Match-through-`&enum` inserts `Deref` then `VariantField`
  (`expr.rs:3925-3931`), so the borrow IS visible.
- **The AST tracker is imprecise.** It rejects any construct its name walk cannot follow with
  a blanket `LinearInUnsupportedPosition` (`error.rs:204-209`; reject sites `infer.rs:3186,
  3199, 3203, 3212, 3849-3858, 5547-5557`, ...). Tuples, `Option<Linear>`, generic
  pass-through, and closures are refused even when safe. The MIR checker, tracking places
  over a CFG, can accept them.

## Architecture

Extend `codegen/analysis/linear.rs`.

**Match-idiom recognition.** Teach the checker the two shapes:
- **Discriminant test:** a block whose statements compute `t = base.tag` (`FieldAccess`
  field `"tag"`) then `c = t == K` (`BinaryOp Eq`) and whose terminator is `If { cond: c }`
  is a match discriminant test on `base`. The checker treats the arm-body successor as the
  block where `base`'s variant payload is live.
- **Payload bind:** `MirRValue::VariantField { base, variant_name, field_index }`
  (`expr.rs:4370-4378`) binds a variant field out of `base`. If `base` is (or is a `Deref`
  of) a linear value, a by-value bind of the payload is a MOVE out of `base`; if `base` came
  through a shared `&` (a `Deref` on the place), it is a **move-out-of-shared-borrow** (closes
  class 1's match form). If `base` is owned, it MOVES `base`'s payload (so `base` becomes
  `Moved` / partially-moved — track the whole `base` as moved conservatively).

Alternative considered: upgrade `lower_match` to emit real `Switch`/`Discriminant` so the
checker needs no idiom recognition. REJECTED for 2c scope (touches codegen for all backends,
risk to the verified C path); recognizing the idiom is contained to the checker. Note it as a
future cleanup.

**Guard fall-through (class 4).** The AST tracker resets `linear_slots` per arm
(`infer.rs:4599`), modeling arms as exclusive; a guard that consumes a linear then falls
through to a later arm that consumes it again lands on the SAME runtime path but is missed
(`infer.rs:4588-4632`). On MIR, a guard lowers to real blocks with a false-edge into the next
candidate. The 2b move/init dataflow, applied to the guard blocks and their false-edges,
sees the guard's move DOMINATE the later arm's move on the fall-through path and reports
use-after-move automatically — provided the checker walks the guard blocks (verify the guard
blocks are in the CFG the dataflow traverses; they are, via the If-chain).

**Precision.** Because the dataflow tracks each linear LOCAL's Init/Moved state by place, a
linear value stored in a tuple/`Option`/passed through a generic is simply MOVED into that
position once and not flagged unless moved again. Removing the AST tracker's blanket rejects
is 2d; but 2c must PROVE the MIR checker accepts these safe programs (tests below), so 2d can
safely delete the AST rejects.

## Data flow

Same as 2b, with the match-idiom rewriter feeding the dataflow: before running the fixpoint,
identify `tag+Eq+If` discriminant tests and `VariantField` binds so payload moves/borrows are
attributed to the scrutinee place.

## Error handling

Soundness-critical (checker). By end of 2c, for every one of the 5 classes there is a passing
`.bld` repro that now FAILS to check, and for a curated set of SAFE compositional programs
(tuple-of-linear, `Option<Linear>`, generic-identity-over-linear, closure-moving-linear-once)
the checker ACCEPTS them. Both directions are required before 2d removes the AST tracker.

## Testing

- **Unit:** MIR shapes for match-through-`&` (Deref + VariantField) -> move-out-of-borrow;
  guard fall-through (guard block moves, later arm moves) -> use-after-move; owned match
  payload bind -> scrutinee moved.
- **End-to-end (repros):** ALL FIVE open-class repros from the exploration now fail
  `buildc check` with a linear diagnostic. The direct `linear_*` enforced cases still fail.
- **End-to-end (precision, the payoff):** a curated set of SAFE programs the AST tracker
  currently REJECTS (tuple of one linear used once; `Option<Coin>` constructed and consumed
  once; `fn id<T>(x: T) -> T { x }` applied to a linear then consumed once; a closure that
  moves a linear once) must CHECK CLEAN under the MIR checker. (While the AST tracker is still
  active they may still be rejected at the type phase — so run these against a build flag or a
  direct `linear::check` unit harness that bypasses the AST tracker, proving the MIR checker
  accepts them ahead of the 2d removal.)
- **Regression:** full `cargo test` green; `-Dwarnings` clean; `buildc corpus verify` 8/8.

## File touch-points

- `compiler/src/codegen/analysis/linear.rs`: match-idiom recognition + VariantField handling
  + the precision test harness hook.
- Test fixtures: a `compiler/tests/linear/` dir (or inline unit MIR) for the 5 repros and the
  safe-precision set.

## Risks

- **Idiom brittleness.** Recognizing `tag+Eq+If` is pattern-matching on a lowering shape that
  could change. Mitigation: a focused unit test that a representative `match` lowers to the
  recognized shape, so a lowering change breaks the test loudly. Long-term: upgrade lowering
  to real `Switch` (out of scope here, noted).
- **Partial moves.** Moving one variant payload out of an owned scrutinee is a partial move.
  2c tracks the whole scrutinee as moved (conservative-sound; may over-reject a later use of a
  DIFFERENT field). Acceptable for soundness; refine only if a real program needs it.

## Dependency

Requires 2a + 2b. Enables 2d (migration), which needs the "MIR checker is a proven superset"
property this sub-brick establishes.
