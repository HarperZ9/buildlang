# Design: Linear-on-MIR 2d — Migration off the AST tracker + adversarial verification

Status: draft for execution (2026-07-01), branch `feat/linear-mir-checker`. Fourth of four
specs. Depends on 2a + 2b + 2c. Soundness-critical.

## Summary

Retire the AST-phase linear FLOW tracker now that the MIR checker (2b/2c) is a proven
superset of its soundness plus the 5 open classes and is more precise. Remove the 13
flow-sensitive reject/consume sites from `types/infer.rs`, keep the declaration-level
containment rule at the type phase, update the diagnostics/tests/docs, and run a fresh
six-lens adversarial pass. After 2d there is ONE linear checker (on MIR), it is sound and
precise, and `docs/LINEAR-TYPES.md` can drop the "not yet fully sound" and "5 known-open"
sections.

## Motivation

Two overlapping checkers with different dispositions (AST = reject-position, MIR = track-
precisely) produce contradictory diagnostics and double maintenance. The MIR checker was
built to REPLACE the flow-sensitive AST tracking. The AST tracker's flow-reject sites also
CAUSE the over-rejection of safe programs; removing them is what delivers the precision the
MIR checker enables. The AST DECLARATION rule (`LinearFieldInNonLinearType`,
`check.rs:355-417`) is not flow-sensitive and stays.

## Architecture

**Precondition gate (do not start 2d until all hold):**
1. The MIR checker fails all 5 open-class repros AND all existing `linear_*` enforced cases
   (a superset-soundness matrix: every case the AST tracker rejected, the MIR checker also
   rejects).
2. The MIR checker accepts the curated safe-precision set (2c) that the AST tracker rejected.
3. `buildc corpus verify` 8/8 and the full suite green with the MIR checker active.

**Removal (surgical, from `types/infer.rs`):**
- Delete/neutralize the 13 flow-sensitive sites: `consume_linear` calls and `reject_linear_
  escape` calls at `infer.rs:2922, 3186, 3199, 3203, 3212, 3275, 3523, 3853, 3873, 3885,
  3901, 4111, 5402, 5525` (verify exact set at implementation time). Remove the now-dead
  `LinearSlot`, `linear_slots`, `linear_loop_markers`, `linear_shadow_stack`,
  `consume_linear`, `merge_linear_snapshots`, `register_linear_local`, `reject_linear_escape`
  and their bookkeeping in `infer.rs` (lines ~93-125, 547-624, 631-695, 665-677).
- KEEP: `has_linear_attr`, `mark_linear`/`is_linear_def`, `linear_def_of`, and
  `validate_linear_containment` / `check_fields_not_linear` (declaration-level, still needed;
  they also feed 2a's annotation stamping).
- Move the AST-phase `linear_*` regression tests (`check.rs:1780-2153`) to drive the MIR
  checker instead: each program that formerly failed the AST tracker must now fail the MIR
  linear check (via `buildc check` end-to-end, or a `linear::check` harness). Do not silently
  delete coverage — re-home it.

**Diagnostics reconciliation:** the MIR checker's error variants (`LinearUseAfterMove`,
`LinearMoveOutOfBorrow`, ...) become the sole linear diagnostics. Remove or repurpose
`LinearInUnsupportedPosition` (the blanket over-rejection error) if nothing else emits it.

**Pipeline:** with the AST flow-tracker gone, the MIR checker is the linear gate. Confirm the
error path: `buildc check`/`build` lower to MIR, run `linear::check`, and reject on errors
with good spans. Ensure a linear error still blocks codegen (it does: errors returned from
`codegen::generate` fail the build).

## Data flow

Unchanged from 2c at runtime. The change is DELETION of the parallel AST path, so a program's
only linear verdict comes from the MIR checker.

## Error handling

The disposition is now unambiguous: the MIR checker rejects unsound programs (sound) and
accepts safe ones (precise). The removal must not open ANY hole: the superset-soundness
matrix (precondition 1) is the guard, re-verified after removal.

## Testing

- **Superset matrix (the core gate):** a test table pairing every historical linear case
  (24 enforced + 5 open + the safe-precision set) with its expected verdict under the MIR
  checker; all must hold AFTER the AST tracker is removed.
- **Adversarial:** a fresh six-lens adversarial pass (move-through-projection, borrow-after-
  partial-move, generic monomorph edge, match-guard/arm interactions, closure capture,
  container/collection storage) trying to construct an unsound program the MIR checker
  ACCEPTS. Run each finding empirically (probe `.bld` + `buildc check`) IN THE MAIN TREE with
  revert (NOT `isolation: 'worktree'` — that infra failed in the increment-4 workspace due to
  nested git repos). Any confirmed hole -> fix in `linear.rs` + regression.
- **Regression:** full `cargo test` green; `-Dwarnings` clean; clippy correctness clean;
  `buildc corpus verify` 8/8.
- **No dead code:** `-Dwarnings` will flag the removed AST tracker's now-unused helpers;
  delete them fully.

## Docs

- `docs/LINEAR-TYPES.md`: rewrite. Remove "not yet fully sound" and the "5 known-open"
  section; describe the MIR affine/borrow checker, its soundness basis (place-based move/init
  dataflow), and its precision (accepts compositional positions). State honestly what it does
  NOT do yet (e.g. partial-move refinement if left conservative).
- `STATUS.md`: update the `#[linear]` bullet from "experimental, not yet fully sound" to the
  new state (sound MIR checker; note test counts).
- The 2a-2d design specs: mark the brick shipped.

## File touch-points

- `compiler/src/types/infer.rs`: remove the flow tracker (the bulk of the change).
- `compiler/src/types/check.rs`: re-home the `linear_*` tests to drive the MIR checker; keep
  the containment rule.
- `compiler/src/error.rs`: retire `LinearInUnsupportedPosition` if unused.
- `docs/LINEAR-TYPES.md`, `STATUS.md`.

## Risks

- **Silent soundness regression on removal.** The one real danger: an AST-caught case the MIR
  checker misses, exposed only after removal. Mitigation: the superset matrix (precondition 1)
  must be GREEN before removal and re-run after; the adversarial pass is the backstop.
- **Lowering panics on linear-invalid input.** With the AST block gone, invalid programs now
  reach lowering. 2b verified lowering is robust; re-confirm across the repro set (no panics,
  clean diagnostics).
- **Diagnostic-quality regression.** MIR spans (2a) must point at the right source. Spot-check
  that each repro's error underlines the offending expression, not the whole function.

## Dependency

Requires 2a + 2b + 2c. Terminal sub-brick: after 2d, `#[linear]` is sound + precise on MIR,
the wedge (quantum no-cloning / on-chain no-double-spend / fin-sec handles) rests on a sound
foundation, and the AST/MIR duplication is gone.
