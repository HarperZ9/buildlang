# Design: MIR Affine/Ownership Foundation

Status: draft for review (2026-06-30). Governed by
`docs/UNIVERSAL_SUBSTRATE_DIRECTIVE_2026-06-30.md` (foundation pillar, first brick).

## Summary

Build one shared MIR dataflow substrate (liveness, ownership, move-graph, borrow-flow)
and use it, as its first client, to generalize Drop insertion from the current
single-block-confined case to true end-of-live-range across arbitrary control flow. The
same substrate is later consumed by a MIR-level affine/linear checker to close the five
open `#[linear]` escape classes. This document specifies the substrate and the bounded
first brick (Approach A, drop flags deferred). Linear-on-MIR and drop flags are separate,
later bricks scoped here but not built here.

## Motivation (two convergent gaps, one missing machine)

Both existing design docs independently name the same missing analysis:

- `docs/MEMORY-PILLAR-DESIGN.md`: three drop increments shipped behind
  `BUILDLANG_EXPERIMENTAL_FREE` (default off) reclaim single-block-confined owned strings
  (a 1M-iteration loop went 983 MB to 3.3 MB, ASan-clean), but multi-block live ranges
  still leak because "there is no liveness or scope infrastructure to lean on." The next
  step it names is "real per-local liveness (live_out via backward dataflow, tracking the
  borrow temps too)."
- `docs/LINEAR-TYPES.md`: the AST slot-tracker enforces many escape classes but five
  remain open (pattern-match-through-a-borrow, enum-variant shorthand init, generic
  deref/result, match-guard fall-through, borrow-after-move). It states soundness "wants a
  proper affine/linear move-and-borrow checker, most naturally on MIR."

The machine both ask for is the same: a real MIR move/borrow/liveness analysis.

## Architecture

Not one pass that does both jobs. Drop insertion is a codegen concern (free `BuildString`
buffers, C-backend-specific, opt-in, leaking is safe). Linear checking is a type-level
guarantee (reject-by-default, reported as compile errors, currently AST-phase in
`types/infer.rs`). Fusing them would conflate layers. The correct shape is one shared
*substrate* with two correctly-layered clients:

```
            compiler/src/codegen/analysis/   (new, reusable, unit-tested)
            ├── liveness.rs     backward per-local live_in/live_out, incl. borrow temps
            ├── ownership.rs     owning heap locals; allocating-fn + borrow whitelists
            ├── move_graph.rs    move edges (Assign Use), transitive taint, multi-acquirer
            └── borrow_flow.rs   where .ptr/Ref temps flow and where they die
                     │ consumes codegen/ir.rs MIR (CFG, MirLocal.ty, terminators)
        ┌────────────┴─────────────┐
        ▼                          ▼
  Drop insertion (codegen)   MIR affine/linear checker (type-gate)
  backend/c.rs, later llvm   new; closes the 5 open #[linear] classes
  = FIRST BRICK              = LATER BRICK (brick 2)
```

The substrate is extracted from the analyses that today live welded inside
`backend/c.rs` (`compute_dominators`, `reachable_blocks`, the exhaustive use-def query
API `rvalue_mentions`/`stmt_uses_local`/`terminator_uses_local`, `owned_string_escapes`,
`move_source_chain`) plus the one greenfield addition: backward liveness. Extraction is
what unblocks reuse by the linear checker and by non-C backends; keeping the analyses in
`c.rs` (rejected Approach B) would re-entrench the coupling.

## Substrate: components and API

Inputs: a `&MirFunction` with its `blocks: Vec<MirBlock>` (CFG via terminators), and
`MirLocal.ty` (fully resolved, e.g. `MirType::Struct("BuildString")`, `MirType::Vec`,
`MirType::Map`). Everything is a pure function of MIR; no mutation.

- `liveness.rs` (greenfield): a standard backward worklist/fixpoint computing
  `live_out[block]` and per-statement liveness for every local. Critically it also tracks
  **borrow temps**: a `T = L.ptr` or `T = &L` keeps `L`'s buffer live for as long as `T`
  is live, so `L`'s effective live range is the union of its own and its borrows'. This
  is the fact the current single-block heuristic lacks.
- `ownership.rs`: which locals own a fresh, solely-owned heap buffer. Reuses the audited
  allocating-function set (`build_string_concat`, `build_format_*`, `build_read_*`, the
  string transforms, ...) and the closed, line-audited borrow whitelist. Deliberately
  excludes `cap = 0` wrappers (`build_string_new`, `String_from`) and container-alias
  getters (`build_hvec_get_str`, `build_hmap_get_str_str`).
- `move_graph.rs`: move edges from `Assign { dest, value: Use(src) }` where `src` owns;
  transitive move-source chains; and the multi-acquirer taint rule (a source moved into
  more than one dest taints all acquirers so none is freed). This is the exact guard that
  caught the real ASan double-free in the second increment; it moves verbatim, with tests.
- `borrow_flow.rs`: one-hop-and-beyond taint of where a `.ptr`/`Ref` of an owner flows
  and where those temps die. Feeds both drop placement (free after the last borrow dies)
  and, later, the linear checker's move-out-of-borrow detection.

Dominators and reachability move into the module unchanged (the second increment already
depends on intersecting only *reachable* predecessors, a fix that must be preserved).

## First brick: liveness-driven Drop increment 4 (Approach A, drop flags deferred)

Generalize the shipped block-scoped drops to multi-block live ranges using real liveness.

**What it adds.** Today a local is freed only if its whole live range (including borrow
temps) is confined to one block on an isolated edge. Increment 4 frees an owned heap local
`L` at the true end of its live range even when that range spans blocks, by placing the
free at the start of the unique block `S` where `L` and all its borrow temps are provably
dead and which is reached after the last use on every path (once per iteration for a loop
body, reclaiming each iteration's buffer).

**Soundness rule (non-negotiable, unchanged from the project standard).** Free `L` only
when it is provably owned, provably dead, and provably non-escaping. Concretely, all must
hold: `L` passes the ownership/move/escape/taint/one-def gates of the second increment;
`L`'s effective live range (self plus borrow temps, from `liveness.rs`) ends before `S`;
`S` post-dominates every use of `L` and is dominated by `L`'s def (so the free runs on
every path, after every use, exactly once); and placing the free at `S`'s start lands
*after* any terminator that consumes a `.ptr` borrow (the use-after-free hazard the third
increment documented: a `.ptr` borrow flowing into a `printf` terminator must be freed
after the terminator, never before). When placement is ambiguous or would require a drop
flag (conditional ownership, non-isolated merge edges), **decline and leak** (safe).

**Disjointness.** Increment-4 frees are disjoint from the increment 1-3 free sets: a local
is claimed by exactly one increment's logic. This is the same discipline that made
increment 3 safe against function-exit frees, and it makes double-free impossible by
construction rather than by argument.

**Non-goals of this brick** (kept out to stay bounded and verifiable): drop flags /
edge-splitting for conditional ownership; any `#[linear]` change; any non-C backend
emission; freeing `Vec`/`Map` (strings first, the audited allocating set).

## Data flow

`MirFunction` → `liveness.rs` (live ranges incl. borrows) + `ownership.rs` (owning set) +
`move_graph.rs` (move/taint) + `borrow_flow.rs` (borrow deaths) → a placement pass that,
for each owning non-tainted non-escaping `L`, computes the sound free site `S` or declines
→ the C backend emits `build_string_free(L)` at `S`'s start (reusing the existing emission
path that already frees at block starts for increment 3) behind
`BUILDLANG_EXPERIMENTAL_FREE`.

## Error handling

- Analysis is conservative: any uncertainty yields no drop (a leak, which is safe; the
  soundness rule forbids trading a leak for a possible corruption).
- The module-wide mutable-global guard is preserved: if the module declares any mutable
  global whose type could hold a heap-string alias, drop analysis is disabled module-wide.
- Unsupported MIR forms fail closed (`CodegenError::Unsupported`), never silently drop a
  store that could be an escape (the `static mut` stash fix, commit `79e765e`).
- Exhaustive matches on `MirRValue`/`MirStmtKind` stay compiler-enforced exhaustive so a
  new MIR variant forces a handler update rather than silently escaping the use scan.

## Testing / verification bar (all required before the flag default may flip)

- **Unit (substrate):** liveness golden tests (live_out correctness incl. borrow temps on
  branch/loop/merge CFGs); ownership/move/borrow tests migrated from the current `c.rs`
  suite so no coverage is lost in extraction.
- **Unit (drop increment 4):** golden MIR/C tests that FREE the multi-block owned case and
  DECLINE every unsound case (moved-out, returned, stored-into-aggregate, aliased,
  escaping `.ptr`, conditional-ownership-needing-a-flag).
- **ASan battery** (`cl /fsanitize=address`): a multi-block allocating loop shows zero
  use-after-free, zero double-free, and bounded peak working set. A deliberate
  use-after-free must still be caught (proves the harness is live).
- **Corpus:** semantic-corpus c-execution stays 8/8 with the flag on and off.
- **Adversarial:** a fresh six-lens adversarial pass in an ISOLATED worktree (the second
  increment passed unit + ASan yet still had a real double-free only the adversarial
  workflow caught; a larger live-range surface must clear the same bar, not a smaller one).

## Sequenced follow-on bricks (scoped here, built later, each its own spec/plan/verify)

- **Brick 2 - linear-on-MIR.** Thread linearity to MIR (a `is_linear` flag on `MirLocal`
  or a `DefId -> bool` map passed at codegen), build the affine move/borrow check on the
  shared substrate, and close the five open classes (move-out-of-borrow covers cases 1/3/5,
  per-path guard/arm tracking covers case 4, enum-literal moves cover case 2), plus the
  deferred must-use ("drop-without-consume") half. Reported as type-gate errors with spans.
- **Brick 3 - drop flags and default-on.** Add drop flags / edge-splitting for conditional
  ownership and non-isolated merges, then flip `BUILDLANG_EXPERIMENTAL_FREE` to default-on
  once the ASan battery is green and the corpus stays 8/8. This is the "memory pillar done"
  criterion: bounded peak memory under a loop by default, not merely corpus-passing.

## File-level touch points (current locations, verify at implementation time)

- New: `compiler/src/codegen/analysis/{mod,liveness,ownership,move_graph,borrow_flow}.rs`.
- Refactor out of `compiler/src/codegen/backend/c.rs`: `compute_dominators` (~776-816),
  `reachable_blocks`, `rvalue_mentions`/`stmt_uses_local`/`terminator_uses_local`,
  `owned_string_escapes`/`ptr_temp_escapes`, `sound_owned_candidates`,
  `block_scoped_freeable`, `move_source_chain`. c.rs keeps only the emission call sites
  (~2071, 2076) now fed by the module.
- MIR types consumed, not changed in this brick: `compiler/src/codegen/ir.rs`
  (`MirTerminator::Drop` ~900, `MirLocal.ty` ~489, CFG ~415-472).
- Runtime frees unchanged: `compiler/src/codegen/runtime.rs` (`build_string_free` ~130).
- Tests: extend `compiler/src/codegen/backend/c.rs` drop tests (~4930+) and add a
  `codegen/analysis` unit suite; add a multi-block ASan program to the memory battery.

## Risks and open questions

- **Extraction churn.** `c.rs` is large and the analyses are interwoven; extraction must be
  behavior-preserving (migrate tests first, keep the flag-off baseline untouched). Global
  quality gate (no file > 300 lines, no fn > 50 lines) is served by the extraction.
- **Liveness of borrow temps** is the crux: missing one `.ptr`/`Ref` flow frees a live
  buffer. The exhaustive-match discipline plus borrow_flow tests are the mitigation.
- **Open question for brick 2 (not this brick):** carry `is_linear` on `MirLocal` versus a
  side map. Prefer the side map first (no backward-incompatible MIR change), revisit if the
  checker needs it pervasively.
