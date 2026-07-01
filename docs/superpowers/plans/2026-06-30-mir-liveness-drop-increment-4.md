# MIR Liveness + Drop Increment 4 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a reusable MIR backward-liveness pass and use it to free heap strings whose live range spans multiple basic blocks (drop increment 4), generalizing the shipped single-block reclamation, behind the existing opt-in flag.

**Architecture:** Extract the CFG / use-def / move primitives that today live inside `backend/c.rs` into a new reusable `codegen/analysis/` module, add a greenfield backward-liveness pass (`analysis/liveness.rs`) that tracks locals and their `.ptr` borrow temps, then add an additive, disjoint increment-4 placement (`analysis/drops.rs`) that frees an owner at the unique block where its buffer dies on every incoming edge. Drop flags / split-frontier cases are declined (leak, which is safe) and deferred to a later brick.

**Tech Stack:** Rust (compiler crate `buildlang`, binary `buildc`), MIR SSA IR in `codegen/ir.rs`, MSVC AddressSanitizer for runtime verification, the 8-program semantic corpus for the release gate.

## Global Constraints

- **Soundness by conservatism (non-negotiable):** free a local only when it is provably owned, provably dead, and provably non-escaping. On any uncertainty, insert no drop. A missed drop leaks (safe); a wrong drop corrupts (forbidden).
- **Disjointness:** increment-4 frees MUST be disjoint from the increment 1-2 function-exit set (`current_fn_freeable`) and the increment 3 block-scoped set (`current_fn_block_frees`). An owner already claimed by either is skipped, so each heap buffer is freed by exactly one increment.
- **Opt-in only:** all new frees are gated by `CBackend::experimental_free_enabled()` (`BUILDLANG_EXPERIMENTAL_FREE`). The default-off baseline (corpus c-execution 8/8, all current programs) must stay byte-identical with the flag unset.
- **Owner definition unchanged:** an owner is the `dest` of a `Call` to a function in `allocates_owned_string` (fresh `cap > 0` buffer), or move-acquired from such via `Assign { value: Use(Local) }`. The set deliberately EXCLUDES `build_string_new` / `String_from` (`cap = 0` wrappers) and container getters.
- **No warnings:** `RUSTFLAGS=-Dwarnings cargo build --manifest-path compiler/Cargo.toml` stays clean; `cargo clippy -- -D clippy::correctness` and `cargo fmt --check` pass.
- **File/function size:** prefer files < 300 lines and functions < 50 lines (split helpers); the extraction serves this by moving analysis out of the oversized `c.rs`.
- **Determinism:** all returned local lists are sorted by `LocalId.0` for reproducible codegen/receipts.
- **Done criterion for the brick:** a MULTI-BLOCK allocating loop (one increment-3 declines) has bounded peak memory under ASan with the flag on, corpus stays 8/8, and a fresh six-lens adversarial pass finds no constructible unsound free.

---

### Task 1: Scaffold `codegen/analysis/` and relocate the shared CFG / use-def / move primitives

Move the reusable, backend-agnostic primitives out of `backend/c.rs` into a new module so the liveness pass (Task 2) and the future MIR linear checker (later brick) can consume them. This is a behavior-preserving refactor: every moved function keeps its exact body; the old `CBackend` associated function becomes a one-line delegator so all existing callers and tests compile and pass unchanged.

**Files:**
- Create: `compiler/src/codegen/analysis/mod.rs`
- Create: `compiler/src/codegen/analysis/cfg.rs`
- Modify: `compiler/src/codegen/mod.rs` (register `pub(crate) mod analysis;`)
- Modify: `compiler/src/codegen/backend/c.rs` (replace moved bodies with delegators)

**Interfaces:**
- Produces (all `pub(crate) fn` in `crate::codegen::analysis::cfg`):
  - `block_id_index(blocks: &[MirBlock]) -> std::collections::HashMap<u32, usize>`
  - `terminator_successors(term: &Option<MirTerminator>, id_to_index: &std::collections::HashMap<u32, usize>) -> Vec<usize>`
  - `reachable_blocks(blocks: &[MirBlock]) -> Vec<bool>`
  - `compute_dominators(blocks: &[MirBlock]) -> Vec<std::collections::HashSet<usize>>`
  - `rvalue_mentions(r: &MirRValue, id: LocalId) -> bool`
  - `stmt_uses_local(kind: &MirStmtKind, x: LocalId) -> bool`
  - `terminator_uses_local(term: &Option<MirTerminator>, x: LocalId) -> bool`
  - `move_source_chain(id: LocalId, blocks: &[MirBlock]) -> Vec<LocalId>`
  - `callee_name(v: &MirValue) -> Option<&str>` (move alongside; used by escape logic)

- [ ] **Step 1: Create the module files**

Create `compiler/src/codegen/analysis/mod.rs`:

```rust
//! Reusable MIR dataflow substrate: CFG queries, use-def scans, move tracking,
//! and (Task 2) backward liveness. Consumed by the C backend's drop insertion
//! and, later, by the MIR affine/linear checker. Everything here is a pure
//! function of MIR (`codegen::ir`); nothing mutates.

pub(crate) mod cfg;
pub(crate) mod liveness; // added in Task 2
pub(crate) mod drops; // added in Task 4
```

For this task, comment out the `liveness` and `drops` lines (add them back in their tasks) so the module compiles:

```rust
pub(crate) mod cfg;
// pub(crate) mod liveness; // Task 2
// pub(crate) mod drops;    // Task 4
```

Create `compiler/src/codegen/analysis/cfg.rs` with the imports the moved functions need:

```rust
//! CFG queries, exhaustive use-def scans, and move-chain tracking over MIR.
use std::collections::{HashMap, HashSet};

use crate::codegen::ir::{
    LocalId, MirBlock, MirРValue, MirStmtKind, MirTerminator, MirValue, PlaceProjection,
};
```

Note: fix the deliberately-corrupted `MirРValue` above to `MirRValue` (ASCII) when you paste; it is written here only to force you to retype the import list against the real `ir.rs` exports.

- [ ] **Step 2: Register the module**

In `compiler/src/codegen/mod.rs`, add next to the other `mod` declarations:

```rust
pub(crate) mod analysis;
```

- [ ] **Step 3: Move the nine primitives verbatim**

For EACH function in the Interfaces list, cut its current definition out of `compiler/src/codegen/backend/c.rs` and paste it into `cfg.rs`, changing the leading `fn` to `pub(crate) fn` and dropping any `Self::` prefixes on internal calls (they are now free functions in the same module, so `Self::reachable_blocks(...)` becomes `reachable_blocks(...)`, `Self::block_id_index(...)` becomes `block_id_index(...)`, etc.). The exact current source locations (verify before cutting):
- `block_id_index`, `terminator_successors` (search `fn block_id_index`, `fn terminator_successors`)
- `reachable_blocks` (c.rs ~699-714)
- `compute_dominators` (c.rs ~776-816)
- `rvalue_mentions` (c.rs ~958-987)
- `stmt_uses_local` (c.rs ~662-675)
- `terminator_uses_local` (c.rs ~679-696)
- `move_source_chain` (c.rs ~511-538)
- `callee_name` (search `fn callee_name`)

- [ ] **Step 4: Replace each moved body in `c.rs` with a delegator**

In `impl CBackend` (and any `impl` block where these were associated functions), replace each moved function with a one-line delegator so every existing `Self::foo(...)` call keeps working. Example for two of them:

```rust
fn compute_dominators(blocks: &[MirBlock]) -> Vec<std::collections::HashSet<usize>> {
    crate::codegen::analysis::cfg::compute_dominators(blocks)
}

fn move_source_chain(id: LocalId, blocks: &[MirBlock]) -> Vec<LocalId> {
    crate::codegen::analysis::cfg::move_source_chain(id, blocks)
}
```

Do this for all nine. Keep the delegators' signatures byte-identical to the originals.

- [ ] **Step 5: Build and run the full suite to prove no behavior change**

Run: `cargo test --manifest-path compiler/Cargo.toml --quiet`
Expected: same pass counts as before the task (lib/bin/cli/lexer/parser all green, 0 failed). The drop and escape tests in `c.rs` still pass because the delegators preserve behavior.

Run: `RUSTFLAGS=-Dwarnings cargo build --manifest-path compiler/Cargo.toml`
Expected: clean build, no warnings (an unused delegator would warn; if a delegator is never called, delete it and call the module function directly at its one call site instead).

- [ ] **Step 6: Commit**

```bash
git add compiler/src/codegen/analysis/mod.rs compiler/src/codegen/analysis/cfg.rs compiler/src/codegen/mod.rs compiler/src/codegen/backend/c.rs
git commit -m "refactor(codegen): extract CFG/use-def/move primitives into codegen::analysis

Behavior-preserving move of block_id_index, terminator_successors,
reachable_blocks, compute_dominators, rvalue_mentions, stmt_uses_local,
terminator_uses_local, move_source_chain, and callee_name out of the oversized
backend/c.rs into a reusable codegen::analysis::cfg module. CBackend keeps
one-line delegators so all callers and tests are unchanged. Full suite green.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: Backward liveness pass (`analysis/liveness.rs`)

The greenfield core both design docs asked for: real per-local `live_in`/`live_out` over the CFG, computed with the standard backward dataflow fixpoint. Uses the exhaustive `stmt_uses_local`/`terminator_uses_local` queries (iterating all locals) rather than a fresh enumerator, so no MIR variant can silently escape the scan.

**Files:**
- Create: `compiler/src/codegen/analysis/liveness.rs`
- Modify: `compiler/src/codegen/analysis/mod.rs` (enable `pub(crate) mod liveness;`)

**Interfaces:**
- Consumes: `crate::codegen::analysis::cfg::{block_id_index, terminator_successors, stmt_uses_local, terminator_uses_local}` (Task 1).
- Produces:
  - `pub(crate) struct Liveness { pub live_in: Vec<std::collections::HashSet<LocalId>>, pub live_out: Vec<std::collections::HashSet<LocalId>> }` (indexed by block position)
  - `pub(crate) fn compute(func: &MirFunction) -> Liveness`

- [ ] **Step 1: Enable the module**

In `analysis/mod.rs`, uncomment: `pub(crate) mod liveness;`

- [ ] **Step 2: Write the failing tests**

Create `compiler/src/codegen/analysis/liveness.rs` with only the test module first (put the real code in Step 4). Use the same direct-MIR construction pattern the `c.rs` tests use.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::ir::{
        BlockId, LocalId, MirBlock, MirFnSig, MirFunction, MirRValue, MirStmt, MirTerminator,
        MirType, MirValue,
    };
    use std::sync::Arc;

    fn i64_local(id: u32, name: &str) -> crate::codegen::ir::MirLocal {
        crate::codegen::ir::MirLocal {
            id: LocalId(id),
            name: Some(Arc::from(name)),
            ty: MirType::i64(),
            is_mut: false,
            is_param: false,
            annotations: Vec::new(),
        }
    }

    // bb0: _0 = call f() -> bb1 ; bb1: _1 = Use(_0) ; return _1
    // _0 is live from its def (bb0 term) into bb1 until the move; _1 live at bb1 return.
    #[test]
    fn straight_line_live_ranges() {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(i64_local(0, "_0"));
        func.locals.push(i64_local(1, "_1"));
        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("f")),
            args: Vec::new(),
            dest: Some(LocalId(0)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut b1 = MirBlock::new(BlockId(1));
        b1.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Use(MirValue::Local(LocalId(0))),
        ));
        b1.terminator = Some(MirTerminator::Return(Some(MirValue::Local(LocalId(1)))));
        func.blocks = Some(vec![b0, b1]);

        let live = compute(&func);
        // _0 flows out of bb0 into bb1 (used by the move in bb1).
        assert!(live.live_out[0].contains(&LocalId(0)));
        assert!(live.live_in[1].contains(&LocalId(0)));
        // _0 is dead once moved; _1 is live at the return.
        assert!(live.live_out[1].contains(&LocalId(1)));
        assert!(!live.live_out[1].contains(&LocalId(0)));
    }

    // A back-edge loop: a local defined and used only inside the body is NOT
    // live across the back-edge (dead at the header entry from the body).
    #[test]
    fn loop_body_local_dead_across_back_edge() {
        // bb0 -> bb1(header) ; bb1: if cond -> bb2 else bb3
        // bb2(body): _2 = call g(); _3 = Use(_2); (use _3) -> bb1
        // bb3: return
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(i64_local(0, "cond"));
        func.locals.push(i64_local(2, "_2"));
        func.locals.push(i64_local(3, "_3"));
        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::Goto(BlockId(1)));
        let mut b1 = MirBlock::new(BlockId(1));
        b1.terminator = Some(MirTerminator::If {
            cond: MirValue::Local(LocalId(0)),
            then_block: BlockId(2),
            else_block: BlockId(3),
        });
        let mut b2 = MirBlock::new(BlockId(2));
        b2.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("g")),
            args: vec![MirValue::Local(LocalId(3))], // uses _3 then loops
            dest: Some(LocalId(2)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut b3 = MirBlock::new(BlockId(3));
        b3.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0, b1, b2, b3]);

        let live = compute(&func);
        // _2 is defined at the end of bb2 (call dest) and used nowhere else, so it
        // is not live out of bb2 and not live into the header from the body.
        assert!(!live.live_out[2].contains(&LocalId(2)));
        assert!(!live.live_in[1].contains(&LocalId(2)));
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --manifest-path compiler/Cargo.toml -p buildlang liveness --quiet`
Expected: FAIL to COMPILE with "cannot find function `compute`" (the impl does not exist yet).

- [ ] **Step 4: Write the liveness implementation**

Add above the test module in `liveness.rs`:

```rust
use std::collections::HashSet;

use crate::codegen::ir::{LocalId, MirFunction, MirStmtKind, MirTerminator};

use super::cfg::{block_id_index, stmt_uses_local, terminator_successors, terminator_uses_local};

/// Per-block liveness. `live_in[i]` / `live_out[i]` are indexed by block position
/// (the same index space `cfg::terminator_successors` returns), NOT by `BlockId`.
pub(crate) struct Liveness {
    pub live_in: Vec<HashSet<LocalId>>,
    pub live_out: Vec<HashSet<LocalId>>,
}

/// Standard backward dataflow to a fixpoint:
///   live_out[b] = union over successors s of live_in[s]
///   live_in[b]  = walk b backward from live_out[b], removing defs and adding uses
/// A "def" is an `Assign { dest }` statement or a `Call { dest: Some }` terminator
/// (the only forms that bind a fresh value to a local; FieldAssign/DerefAssign
/// mutate existing contents and StorageLive/Dead are never emitted). Uses come
/// from the exhaustive `stmt_uses_local`/`terminator_uses_local` queries.
pub(crate) fn compute(func: &MirFunction) -> Liveness {
    let blocks = match &func.blocks {
        Some(b) => b.as_slice(),
        None => {
            return Liveness {
                live_in: Vec::new(),
                live_out: Vec::new(),
            }
        }
    };
    let n = blocks.len();
    let id_to_index = block_id_index(blocks);
    let all_locals: Vec<LocalId> = func.locals.iter().map(|l| l.id).collect();

    let mut live_in: Vec<HashSet<LocalId>> = vec![HashSet::new(); n];
    let mut live_out: Vec<HashSet<LocalId>> = vec![HashSet::new(); n];

    let mut changed = true;
    while changed {
        changed = false;
        // Reverse index order converges quickly for the reducible CFGs the
        // lowering emits; the fixpoint is order-independent for correctness.
        for i in (0..n).rev() {
            // live_out[i] = union of successors' live_in
            let mut new_out: HashSet<LocalId> = HashSet::new();
            for s in terminator_successors(&blocks[i].terminator, &id_to_index) {
                if let Some(sin) = live_in.get(s) {
                    for l in sin {
                        new_out.insert(*l);
                    }
                }
            }

            // Walk the block backward: terminator runs last, then statements.
            let mut live = new_out.clone();
            // Terminator: kill its def, then gen its uses.
            if let Some(MirTerminator::Call { dest: Some(d), .. }) = &blocks[i].terminator {
                live.remove(d);
            }
            for x in &all_locals {
                if terminator_uses_local(&blocks[i].terminator, *x) {
                    live.insert(*x);
                }
            }
            // Statements, last to first: kill def, then gen uses (so `x = x + 1`
            // keeps `x` live-before because the use is re-added after the kill).
            for stmt in blocks[i].stmts.iter().rev() {
                if let MirStmtKind::Assign { dest, .. } = &stmt.kind {
                    live.remove(dest);
                }
                for x in &all_locals {
                    if stmt_uses_local(&stmt.kind, *x) {
                        live.insert(*x);
                    }
                }
            }

            if new_out != live_out[i] {
                live_out[i] = new_out;
                changed = true;
            }
            if live != live_in[i] {
                live_in[i] = live;
                changed = true;
            }
        }
    }

    Liveness { live_in, live_out }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --manifest-path compiler/Cargo.toml -p buildlang liveness --quiet`
Expected: PASS (2 tests). Also run `cargo test --manifest-path compiler/Cargo.toml --quiet` to confirm nothing else broke.

- [ ] **Step 6: Commit**

```bash
git add compiler/src/codegen/analysis/mod.rs compiler/src/codegen/analysis/liveness.rs
git commit -m "feat(codegen): backward MIR liveness pass in codegen::analysis

Standard backward dataflow to a fixpoint computing per-block live_in/live_out,
reusing the exhaustive use-def queries so no MIR variant escapes the scan. This
is the reusable substrate both MEMORY-PILLAR-DESIGN and LINEAR-TYPES asked for.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3: Buffer-liveness overlay (borrow-temp aware)

A heap buffer is live wherever its owner OR any borrow temp of it (`T = owner.ptr`) OR any of the owner's move sources are live. This overlay is what makes the drop placement in Task 4 sound: freeing while a `.ptr` borrow is still live is the exact use-after-free the third increment documented.

**Files:**
- Modify: `compiler/src/codegen/analysis/liveness.rs`

**Interfaces:**
- Consumes: `Liveness` (Task 2), `crate::codegen::analysis::cfg::move_source_chain` (Task 1).
- Produces:
  - `pub(crate) fn buffer_live_in(func: &MirFunction, live: &Liveness, owner: LocalId) -> Vec<bool>`
  - `pub(crate) fn buffer_live_out(func: &MirFunction, live: &Liveness, owner: LocalId) -> Vec<bool>`

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `liveness.rs`:

```rust
// bb0: _0 = call alloc() -> bb1
// bb1: _1 = _0.ptr ; call print(_1) -> bb2      (borrow of the buffer, used here)
// bb2: return
// The buffer is live in bb0(out) and bb1, dead at bb2 entry.
#[test]
fn buffer_stays_live_through_ptr_borrow() {
    use crate::codegen::ir::MirLocal;
    let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
    func.locals.push(MirLocal {
        id: LocalId(0),
        name: Some(Arc::from("_0")),
        ty: MirType::Struct(Arc::from("BuildString")),
        is_mut: false,
        is_param: false,
        annotations: Vec::new(),
    });
    func.locals.push(i64_local(1, "_1")); // the .ptr borrow temp
    let mut b0 = MirBlock::new(BlockId(0));
    b0.terminator = Some(MirTerminator::Call {
        func: MirValue::Function(Arc::from("build_string_concat")),
        args: Vec::new(),
        dest: Some(LocalId(0)),
        target: Some(BlockId(1)),
        unwind: None,
    });
    let mut b1 = MirBlock::new(BlockId(1));
    b1.stmts.push(MirStmt::assign(
        LocalId(1),
        MirRValue::FieldAccess {
            base: MirValue::Local(LocalId(0)),
            field_name: Arc::from("ptr"),
            field_ty: MirType::i64(),
        },
    ));
    b1.terminator = Some(MirTerminator::Call {
        func: MirValue::Function(Arc::from("printf")),
        args: vec![MirValue::Local(LocalId(1))],
        dest: None,
        target: Some(BlockId(2)),
        unwind: None,
    });
    let mut b2 = MirBlock::new(BlockId(2));
    b2.terminator = Some(MirTerminator::Return(None));
    func.blocks = Some(vec![b0, b1, b2]);

    let live = compute(&func);
    let bin = buffer_live_in(&func, &live, LocalId(0));
    let bout = buffer_live_out(&func, &live, LocalId(0));
    // Buffer live leaving bb0 and entering bb1 (via the .ptr borrow), dead at bb2.
    assert!(bout[0], "buffer live out of bb0");
    assert!(bin[1], "buffer live into bb1 via _0 or its .ptr borrow");
    assert!(!bin[2], "buffer dead at bb2 entry (borrow consumed by bb1 printf)");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --manifest-path compiler/Cargo.toml -p buildlang buffer_stays_live --quiet`
Expected: FAIL to compile ("cannot find function `buffer_live_in`").

- [ ] **Step 3: Implement the overlay**

Add to `liveness.rs` (below `compute`):

```rust
use crate::codegen::ir::{MirRValue, MirStmtKind as StmtKind, MirValue};

use super::cfg::move_source_chain;

/// Locals that alias `owner`'s heap buffer: `owner`, its move sources, and every
/// one-hop borrow temp `T = <alias>.ptr`. Move sources are moved-from (dead after
/// the move) so only their borrows can still point into the buffer; including them
/// is conservative-correct.
fn buffer_aliases(func: &MirFunction, owner: LocalId) -> Vec<LocalId> {
    let blocks = func.blocks.as_deref().unwrap_or(&[]);
    let mut aliases = vec![owner];
    aliases.extend(move_source_chain(owner, blocks));
    let mut borrows = Vec::new();
    for block in blocks {
        for stmt in &block.stmts {
            if let StmtKind::Assign {
                dest,
                value:
                    MirRValue::FieldAccess {
                        base: MirValue::Local(b),
                        ..
                    },
            } = &stmt.kind
            {
                if aliases.contains(b) {
                    borrows.push(*dest);
                }
            }
        }
    }
    aliases.extend(borrows);
    aliases
}

fn buffer_live(sets: &[HashSet<LocalId>], aliases: &[LocalId]) -> Vec<bool> {
    sets.iter()
        .map(|s| aliases.iter().any(|a| s.contains(a)))
        .collect()
}

/// Whether `owner`'s buffer is live at each block's ENTRY (indexed by block position).
pub(crate) fn buffer_live_in(func: &MirFunction, live: &Liveness, owner: LocalId) -> Vec<bool> {
    buffer_live(&live.live_in, &buffer_aliases(func, owner))
}

/// Whether `owner`'s buffer is live at each block's EXIT (indexed by block position).
pub(crate) fn buffer_live_out(func: &MirFunction, live: &Liveness, owner: LocalId) -> Vec<bool> {
    buffer_live(&live.live_out, &buffer_aliases(func, owner))
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test --manifest-path compiler/Cargo.toml -p buildlang buffer_stays_live --quiet`
Expected: PASS. Then `cargo test --manifest-path compiler/Cargo.toml --quiet` (all green) and `cargo fmt --manifest-path compiler/Cargo.toml`.

- [ ] **Step 5: Commit**

```bash
git add compiler/src/codegen/analysis/liveness.rs
git commit -m "feat(codegen): borrow-aware buffer-liveness overlay

buffer_live_in/out report where an owner's heap buffer is live, unioning the
owner, its move sources, and their one-hop .ptr borrow temps. This is the fact
that makes multi-block drop placement free only after every borrow is dead.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 4: Increment-4 placement (`analysis/drops.rs`)

Free an owner whose live range spans multiple blocks, at the unique block where its buffer dies on every incoming edge. Additive and disjoint from increments 1-3. Split death frontiers (a buffer live on one predecessor edge, dead on another) need a drop flag and are DECLINED (leak, safe), deferred to the next brick.

**Files:**
- Create: `compiler/src/codegen/analysis/drops.rs`
- Modify: `compiler/src/codegen/analysis/mod.rs` (enable `pub(crate) mod drops;`)

**Interfaces:**
- Consumes: `cfg::{block_id_index, terminator_successors, reachable_blocks, compute_dominators}`, `liveness::{compute, buffer_live_in, buffer_live_out}`.
- Produces:
  - `pub(crate) fn multi_block_freeable(func: &MirFunction, candidates: &[(LocalId, usize)], fn_exit: &std::collections::HashSet<LocalId>, block_scoped: &std::collections::HashSet<LocalId>) -> std::collections::HashMap<u32, Vec<LocalId>>`
    - `candidates`: `(owner, def_block_index)` pairs. Pass the result of `CBackend::sound_owned_candidates` (the increment-2 owner/escape/move/taint gates, which do NOT require single-block confinement).
    - `fn_exit`, `block_scoped`: the already-claimed owner sets for disjointness.
    - Returns: `bb_id -> owners to free at that block's START` (merge into `current_fn_block_frees`).

- [ ] **Step 1: Enable the module**

In `analysis/mod.rs`, uncomment: `pub(crate) mod drops;`

- [ ] **Step 2: Write the failing tests**

Create `compiler/src/codegen/analysis/drops.rs` with the test module (impl in Step 4):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::ir::{
        BlockId, LocalId, MirBlock, MirFnSig, MirFunction, MirLocal, MirRValue, MirStmt,
        MirTerminator, MirType, MirValue,
    };
    use std::collections::HashSet;
    use std::sync::Arc;

    fn bs(id: u32, name: &str) -> MirLocal {
        MirLocal {
            id: LocalId(id),
            name: Some(Arc::from(name)),
            ty: MirType::Struct(Arc::from("BuildString")),
            is_mut: false,
            is_param: false,
            annotations: Vec::new(),
        }
    }
    fn i64_local(id: u32, name: &str) -> MirLocal {
        MirLocal {
            id: LocalId(id),
            name: Some(Arc::from(name)),
            ty: MirType::i64(),
            is_mut: false,
            is_param: false,
            annotations: Vec::new(),
        }
    }

    // Multi-block live range increment 3 declines (not single-block confined):
    // bb0: _0 = alloc() -> bb1
    // bb1: _1 = _0.ptr ; print(_1) -> bb2      (owner defined bb0, used bb1)
    // bb2: return
    // Buffer dies at bb2 entry; bb2's only pred bb1 has it live out; bb0 dominates bb2.
    // -> free _0 at start of bb2.
    fn multi_block_owner_func() -> MirFunction {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(bs(0, "_0"));
        func.locals.push(i64_local(1, "_1"));
        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("build_string_concat")),
            args: Vec::new(),
            dest: Some(LocalId(0)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut b1 = MirBlock::new(BlockId(1));
        b1.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::FieldAccess {
                base: MirValue::Local(LocalId(0)),
                field_name: Arc::from("ptr"),
                field_ty: MirType::i64(),
            },
        ));
        b1.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("printf")),
            args: vec![MirValue::Local(LocalId(1))],
            dest: None,
            target: Some(BlockId(2)),
            unwind: None,
        });
        let mut b2 = MirBlock::new(BlockId(2));
        b2.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0, b1, b2]);
        func
    }

    #[test]
    fn frees_multi_block_owner_at_death_block_start() {
        let func = multi_block_owner_func();
        let candidates = vec![(LocalId(0), 0usize)];
        let map = multi_block_freeable(
            &func,
            &candidates,
            &HashSet::new(),
            &HashSet::new(),
        );
        assert_eq!(
            map.get(&2).map(|v| v.as_slice()),
            Some(&[LocalId(0)][..]),
            "owner must be freed at the start of bb2 (the buffer's death block): {map:?}"
        );
    }

    #[test]
    fn skips_owner_already_claimed_by_fn_exit() {
        let func = multi_block_owner_func();
        let candidates = vec![(LocalId(0), 0usize)];
        let mut fn_exit = HashSet::new();
        fn_exit.insert(LocalId(0));
        let map = multi_block_freeable(&func, &candidates, &fn_exit, &HashSet::new());
        assert!(map.is_empty(), "disjointness: fn_exit owner must not be re-freed");
    }

    #[test]
    fn skips_owner_already_claimed_by_block_scoped() {
        let func = multi_block_owner_func();
        let candidates = vec![(LocalId(0), 0usize)];
        let mut bscoped = HashSet::new();
        bscoped.insert(LocalId(0));
        let map = multi_block_freeable(&func, &candidates, &HashSet::new(), &bscoped);
        assert!(map.is_empty(), "disjointness: block-scoped owner must not be re-freed");
    }

    // Split death frontier: buffer live on one incoming edge of the join, dead on
    // the other -> needs a drop flag -> DECLINE.
    // bb0: _0 = alloc() -> if cond bb1 else bb2
    // bb1: _1 = _0.ptr ; print(_1) -> bb3        (buffer used on this path)
    // bb2: -> bb3                                 (buffer unused on this path)
    // bb3: return                                 (join)
    #[test]
    fn declines_split_death_frontier() {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(i64_local(9, "cond"));
        func.locals.push(bs(0, "_0"));
        func.locals.push(i64_local(1, "_1"));
        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("build_string_concat")),
            args: Vec::new(),
            dest: Some(LocalId(0)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        // Reshape bb0 to branch after the alloc: use a separate header for clarity.
        // bb0 allocates then gotos bb_hdr; bb_hdr branches. Simpler: allocate then If.
        // (Call terminator cannot also branch, so allocate in bb0, branch in bb0b.)
        let mut b0b = MirBlock::new(BlockId(4));
        b0.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("build_string_concat")),
            args: Vec::new(),
            dest: Some(LocalId(0)),
            target: Some(BlockId(4)),
            unwind: None,
        });
        b0b.terminator = Some(MirTerminator::If {
            cond: MirValue::Local(LocalId(9)),
            then_block: BlockId(1),
            else_block: BlockId(2),
        });
        let mut b1 = MirBlock::new(BlockId(1));
        b1.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::FieldAccess {
                base: MirValue::Local(LocalId(0)),
                field_name: Arc::from("ptr"),
                field_ty: MirType::i64(),
            },
        ));
        b1.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("printf")),
            args: vec![MirValue::Local(LocalId(1))],
            dest: None,
            target: Some(BlockId(3)),
            unwind: None,
        });
        let mut b2 = MirBlock::new(BlockId(2));
        b2.terminator = Some(MirTerminator::Goto(BlockId(3)));
        let mut b3 = MirBlock::new(BlockId(3));
        b3.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0, b0b, b1, b2, b3]);

        // def block index of _0 is 0 (bb0). Candidate offered; rule must DECLINE
        // because the join bb3 has one pred (bb1) with the buffer live-out and one
        // (bb2) with it dead-out: a split frontier.
        let candidates = vec![(LocalId(0), 0usize)];
        let map = multi_block_freeable(&func, &candidates, &HashSet::new(), &HashSet::new());
        assert!(
            map.is_empty(),
            "split death frontier needs a drop flag; must decline (leak, safe): {map:?}"
        );
    }
}
```

- [ ] **Step 3: Run to verify the tests fail**

Run: `cargo test --manifest-path compiler/Cargo.toml -p buildlang drops --quiet`
Expected: FAIL to compile ("cannot find function `multi_block_freeable`").

- [ ] **Step 4: Implement the placement**

Add above the test module in `drops.rs`:

```rust
use std::collections::{HashMap, HashSet};

use crate::codegen::ir::{LocalId, MirFunction};

use super::cfg::{block_id_index, compute_dominators, reachable_blocks, terminator_successors};
use super::liveness::{self, buffer_live_in, buffer_live_out};

/// Additional block-start frees for owners whose live range spans multiple blocks
/// (increment 4). Disjoint from `fn_exit` (increments 1-2) and `block_scoped`
/// (increment 3): an owner in either is skipped, so each buffer is freed once.
///
/// Sound-conservative "single clean death frontier" rule. Free owner `L` at the
/// START of block `S` iff there is EXACTLY ONE reachable, non-entry block `S`
/// such that:
///   1. `L`'s buffer is DEAD at `S`'s entry, and
///   2. every predecessor `P` of `S` has `L`'s buffer LIVE at `P`'s exit (the
///      buffer dies on every `P -> S` edge), and
///   3. `L`'s defining block dominates `S`.
/// Then freeing at `S`'s start runs after every use (buffer dead at entry means no
/// use at/after `S`), after any borrow-consuming terminator in `P` (the borrow is
/// dead at `S`), on every path reaching `S`, exactly once per reach, and only when
/// `L` was allocated (def dominates `S`). Zero or >1 such `S` (a split frontier
/// needing a drop flag) declines: the buffer leaks, which is safe.
pub(crate) fn multi_block_freeable(
    func: &MirFunction,
    candidates: &[(LocalId, usize)],
    fn_exit: &HashSet<LocalId>,
    block_scoped: &HashSet<LocalId>,
) -> HashMap<u32, Vec<LocalId>> {
    let mut map: HashMap<u32, Vec<LocalId>> = HashMap::new();
    let blocks = match &func.blocks {
        Some(b) if !b.is_empty() => b.as_slice(),
        _ => return map,
    };
    let n = blocks.len();
    let id_to_index = block_id_index(blocks);
    let entry = id_to_index.get(&0).copied().unwrap_or(0);
    let reachable = reachable_blocks(blocks);
    let dom = compute_dominators(blocks);
    let live = liveness::compute(func);

    // Predecessor lists over reachable blocks only.
    let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, b) in blocks.iter().enumerate() {
        if !reachable[i] {
            continue;
        }
        for s in terminator_successors(&b.terminator, &id_to_index) {
            if s < n {
                preds[s].push(i);
            }
        }
    }

    for &(owner, def_bi) in candidates {
        if fn_exit.contains(&owner) || block_scoped.contains(&owner) {
            continue; // disjointness
        }
        let buf_in = buffer_live_in(func, &live, owner);
        let buf_out = buffer_live_out(func, &live, owner);

        let mut death_blocks: Vec<usize> = Vec::new();
        for s in 0..n {
            if !reachable[s] || s == entry || buf_in[s] || preds[s].is_empty() {
                continue;
            }
            if !preds[s].iter().all(|&p| buf_out[p]) {
                continue; // split frontier: some pred has the buffer dead -> decline
            }
            if !dom[s].contains(&def_bi) {
                continue; // def must dominate the free site
            }
            death_blocks.push(s);
        }
        if death_blocks.len() != 1 {
            continue; // zero or split -> decline (leak, safe)
        }
        map.entry(blocks[death_blocks[0]].id.0).or_default().push(owner);
    }

    for v in map.values_mut() {
        v.sort_by_key(|id| id.0);
    }
    map
}
```

- [ ] **Step 5: Run to verify the tests pass**

Run: `cargo test --manifest-path compiler/Cargo.toml -p buildlang drops --quiet`
Expected: PASS (4 tests). Then full suite + fmt:
`cargo test --manifest-path compiler/Cargo.toml --quiet` (all green)
`cargo fmt --manifest-path compiler/Cargo.toml`

- [ ] **Step 6: Commit**

```bash
git add compiler/src/codegen/analysis/mod.rs compiler/src/codegen/analysis/drops.rs
git commit -m "feat(codegen): increment-4 multi-block drop placement

Free an owner whose live range spans blocks at the unique block where its buffer
dies on every incoming edge, using the liveness + buffer-liveness substrate.
Additive and disjoint from the function-exit and block-scoped sets; split death
frontiers (needing a drop flag) decline and leak. Drop flags deferred to brick 3.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 5: Wire increment 4 into the C backend (flag-gated) and prove the corpus is unchanged

Merge the increment-4 frees into `current_fn_block_frees`, disjoint from the existing sets, only when the experimental flag is on.

**Files:**
- Modify: `compiler/src/codegen/backend/c.rs` (the per-function drop-set population site)

**Interfaces:**
- Consumes: `CBackend::freeable_owned_string_locals`, `CBackend::sound_owned_candidates`, `CBackend::block_scoped_freeable`, and `crate::codegen::analysis::drops::multi_block_freeable`.

- [ ] **Step 1: Find the population site**

Search `c.rs` for where the drop sets are assigned per function. Grep:

Run: `git grep -n "current_fn_block_frees = \|current_fn_freeable = " compiler/src/codegen/backend/c.rs`
Expected: the lines (inside the per-function generation entry, guarded by `Self::experimental_free_enabled()`) where `self.current_fn_freeable` and `self.current_fn_block_frees` are set from `freeable_owned_string_locals(func)` and `block_scoped_freeable(func, &fn_exit)`.

- [ ] **Step 2: Write the failing test**

Add to the `c.rs` test module a test that a multi-block owner ends up in `current_fn_block_frees` when the flag is on. Reuse the `multi_block_owner_func` shape (copy it into the c.rs test module, or expose the analysis result). Concretely, assert against `multi_block_freeable` merged output through a small backend helper:

```rust
#[test]
fn increment4_multi_block_owner_is_scheduled_for_free() {
    // Same shape as analysis::drops::tests::frees_multi_block_owner_at_death_block_start:
    // bb0 alloc -> bb1 uses _0.ptr -> bb2 return. Expect _0 freed at bb2 start.
    let backend = CBackend::new();
    let func = /* build the bb0/bb1/bb2 function exactly as in Task 4 Step 2 */
        crate::codegen::backend::c::tests::multi_block_owner_func();
    let fn_exit: std::collections::HashSet<LocalId> =
        backend.freeable_owned_string_locals(&func).into_iter().collect();
    let bscoped_map = backend.block_scoped_freeable(&func, &backend.freeable_owned_string_locals(&func));
    let bscoped: std::collections::HashSet<LocalId> =
        bscoped_map.values().flatten().copied().collect();
    let candidates = backend.sound_owned_candidates(&func);
    let extra = crate::codegen::analysis::drops::multi_block_freeable(
        &func, &candidates, &fn_exit, &bscoped,
    );
    assert_eq!(extra.get(&2).map(|v| v.as_slice()), Some(&[LocalId(0)][..]));
}
```

Add the `multi_block_owner_func` builder to the `c.rs` test module (copy from Task 4 Step 2) so this test is self-contained.

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test --manifest-path compiler/Cargo.toml increment4_multi_block --quiet`
Expected: FAIL (either compile error until the helper is added, then a real assertion once wiring is absent — confirm it fails before wiring).

- [ ] **Step 4: Wire the merge at the population site**

At the site from Step 1, after `block_scoped_freeable` is computed and assigned, add the increment-4 merge (still inside the `experimental_free_enabled()` guard):

```rust
// Increment 4: multi-block live ranges the block-scoped (single-block) rule
// declines. Disjoint from both prior sets.
let fn_exit_set: std::collections::HashSet<LocalId> =
    self.current_fn_freeable.iter().copied().collect();
let block_scoped_set: std::collections::HashSet<LocalId> =
    self.current_fn_block_frees.values().flatten().copied().collect();
let candidates = self.sound_owned_candidates(func);
let extra = crate::codegen::analysis::drops::multi_block_freeable(
    func,
    &candidates,
    &fn_exit_set,
    &block_scoped_set,
);
for (bb, ids) in extra {
    let slot = self.current_fn_block_frees.entry(bb).or_default();
    for id in ids {
        if !slot.contains(&id) {
            slot.push(id);
        }
    }
    slot.sort_by_key(|id| id.0);
}
```

- [ ] **Step 5: Run the test and the full suite**

Run: `cargo test --manifest-path compiler/Cargo.toml increment4_multi_block --quiet`
Expected: PASS.
Run: `cargo test --manifest-path compiler/Cargo.toml --quiet`
Expected: all green, 0 failed.

- [ ] **Step 6: Prove the corpus is byte-identical with the flag OFF and passes with it ON**

Run (flag off, the default): `cargo build --manifest-path compiler/Cargo.toml --release` then `./compiler/target/release/buildc corpus verify`
Expected: corpus 8/8 pass (baseline unchanged).

Run (flag on): set `BUILDLANG_EXPERIMENTAL_FREE=1` and re-run `buildc corpus verify`
Expected: corpus 8/8 pass (the drops emitted on real programs are sound).

- [ ] **Step 7: Commit**

```bash
git add compiler/src/codegen/backend/c.rs
git commit -m "feat(codegen): schedule increment-4 multi-block frees (opt-in)

Merge multi_block_freeable output into current_fn_block_frees behind
BUILDLANG_EXPERIMENTAL_FREE, disjoint from the function-exit and block-scoped
sets. Corpus c-execution stays 8/8 with the flag on and off.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 6: ASan battery — a multi-block allocating loop that increment 3 leaks

Prove the increment reclaims real heap under AddressSanitizer with zero use-after-free / double-free and bounded peak memory, on a program shape the block-scoped (single-block) rule declines.

**Files:**
- Create: `compiler/tests/mem/multi_block_loop.bld` (or the repo's memory-test location; see Step 1)
- Create/modify: a memory-battery doc or script recording the exact ASan commands (`docs/MEMORY-PILLAR-DESIGN.md` verification section)

**Interfaces:** none (runtime verification).

- [ ] **Step 1: Locate where memory ASan programs live**

Run: `git grep -n "fsanitize=address" -- . ':!target'`
Expected: any existing ASan script/notes. If none beyond the design doc, create `compiler/tests/mem/` for the `.bld` fixtures and record commands in the design doc.

- [ ] **Step 2: Write a multi-block allocating loop that the single-block rule declines**

Create `compiler/tests/mem/multi_block_loop.bld`. The owner must be USED in a different block than where it is defined (so the buffer's live range spans blocks and increment 3 declines). Example shape (adjust to real BuildLang syntax; the key is: allocate in one block, use via a call in a later block, loop):

```
fn main() ~ Console {
    let mut i = 0;
    while i < 1000000 {
        let a = int_to_string(i);      // allocates (owner)
        let b = a + "!";               // derived owner, used below in a later block
        println!("{}", b);             // use in a subsequent block
        i = i + 1;
    }
}
```

Confirm with `buildc --target c` that `b`'s live range spans blocks (the concat and the print lower to different basic blocks) so increment 3 does not already free it. If the shape collapses to a single block, add a conditional inside the loop so the use lands in a distinct block.

- [ ] **Step 3: Generate C, compile with MSVC ASan, and run**

Ensure the MSVC environment is loaded (per `docs/buildlang-msvc-available` notes: VS BuildTools 2022 `vcvars64`). Then:

```cmd
buildc compiler\tests\mem\multi_block_loop.bld --target c -o multi_block_loop.c
cl.exe /nologo /std:c11 /fsanitize=address /Fe:multi_block_loop.exe multi_block_loop.c
multi_block_loop.exe
```

Expected: exit 0, correct output, and NO `AddressSanitizer:` error lines. Record peak working set (Task Manager or `/showPeakMemory`-style measurement) with the flag-driven build.

- [ ] **Step 4: Verify the reclamation actually happens**

Diff the generated C with and without `BUILDLANG_EXPERIMENTAL_FREE`:

```cmd
set BUILDLANG_EXPERIMENTAL_FREE=
buildc compiler\tests\mem\multi_block_loop.bld --target c -o off.c
set BUILDLANG_EXPERIMENTAL_FREE=1
buildc compiler\tests\mem\multi_block_loop.bld --target c -o on.c
fc off.c on.c
```

Expected: `on.c` contains at least one additional `build_string_free(...)` inside the loop body block that `off.c` lacks, and it is placed AFTER the `printf`/borrow use.

- [ ] **Step 5: Record the ASan result in the design doc and commit**

Append an "Increment 4" section to `docs/MEMORY-PILLAR-DESIGN.md` with the exact commands, the ASan-clean result, and the measured peak-memory reduction vs the leaking baseline.

```bash
git add compiler/tests/mem/multi_block_loop.bld docs/MEMORY-PILLAR-DESIGN.md
git commit -m "test(mem): ASan-verify increment-4 multi-block loop reclamation

A million-iteration loop whose owner's live range spans blocks (increment 3
declines it) is ASan-clean and bounded under BUILDLANG_EXPERIMENTAL_FREE. Records
the exact cl /fsanitize=address command sequence and peak-memory result.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 7: Six-lens adversarial pass (isolated worktree)

The second and third increments each passed unit + ASan yet still had latent unsound frees only an adversarial workflow caught. Increment 4 has a larger soundness surface and must clear the same bar. Run this in an ISOLATED git worktree (a prior adversarial run reverted the working tree mid-implementation).

**Files:** none (verification); any fix gets its own regression test in `drops.rs`.

- [ ] **Step 1: Create an isolated worktree**

```bash
git worktree add ../ql-adversarial-inc4 feat/mir-affine-foundation
```

- [ ] **Step 2: Run six adversarial lenses**

Dispatch six independent attack agents, each trying to CONSTRUCT a MIR program (as a `drops.rs` unit test) that makes `multi_block_freeable` emit an UNSOUND free (use-after-free or double-free) or that double-frees against the increment 1-3 sets. The lenses:
1. Move-aliasing across blocks (owner moved into two acquirers in different blocks).
2. Borrow temp that outlives the death block (a `.ptr` copied into a temp that is live past `S`).
3. Split/converging frontier that the "all preds live-out" check should reject.
4. Dominance edge cases (owner defined on only some paths to `S`; unreachable predecessors).
5. Disjointness (an owner that both `block_scoped_freeable` and `multi_block_freeable` try to claim).
6. Container/getter aliasing (an owner whose buffer is aliased into a Vec/Map).

Each finding must be an independently-verified, runnable counterexample test.

- [ ] **Step 3: Fix every confirmed finding with a regression test**

For each real finding, tighten `multi_block_freeable` (default toward DECLINE) and add the counterexample as a passing `drops.rs` test asserting `map.is_empty()` (or the single sound placement).

- [ ] **Step 4: Re-run the ASan battery and corpus after fixes**

Run Task 6 Step 3 again and `buildc corpus verify` (flag on) to confirm the fixes did not regress reclamation or the corpus.

- [ ] **Step 5: Remove the worktree and commit fixes on the branch**

```bash
git worktree remove ../ql-adversarial-inc4
git add compiler/src/codegen/analysis/drops.rs
git commit -m "fix(codegen): close increment-4 adversarial findings

Six-lens adversarial pass (move-aliasing, outliving borrows, split frontier,
dominance edges, disjointness, container aliasing). Each confirmed counterexample
is now a decline with a regression test.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 8: Status docs and brick wrap-up

**Files:**
- Modify: `STATUS.md` (record increment 4 under the memory pillar, honestly scoped)
- Modify: `docs/superpowers/specs/2026-06-30-mir-affine-foundation-design.md` (mark the first brick shipped; note brick 2/3 remain)

- [ ] **Step 1: Update STATUS.md**

Under the memory-pillar bullet, add the increment-4 entry: the reusable `codegen::analysis` substrate (liveness + buffer-liveness) now backs multi-block drops; coverage is the single-clean-death-frontier case; split frontiers still decline (leak) pending drop flags (brick 3); the flag stays default-off until brick 3 flips it. Keep the honest maturity label.

- [ ] **Step 2: Update the design spec status line**

Change the spec's status to note the first brick is implemented on `feat/mir-affine-foundation`, with bricks 2 (linear-on-MIR) and 3 (drop flags + default-on) still open.

- [ ] **Step 3: Full green + warning gate + fmt + clippy**

Run:
`cargo test --manifest-path compiler/Cargo.toml --quiet`
`RUSTFLAGS=-Dwarnings cargo build --manifest-path compiler/Cargo.toml`
`cargo fmt --manifest-path compiler/Cargo.toml --check`
`cargo clippy --manifest-path compiler/Cargo.toml -- -D clippy::correctness`
Expected: all clean.

- [ ] **Step 4: Commit**

```bash
git add STATUS.md docs/superpowers/specs/2026-06-30-mir-affine-foundation-design.md
git commit -m "docs(status): record MIR liveness + drop increment-4 first brick

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage:**
- Shared substrate (liveness/ownership/move-graph/borrow-flow) -> Tasks 1-3 build cfg + liveness + buffer-liveness; ownership/move reused from extracted `cfg` + `sound_owned_candidates`. Borrow-flow is covered by the buffer-liveness overlay for this brick's needs; the standalone `borrow_flow.rs` module named in the spec is only required by brick 2 (linear-on-MIR) and is intentionally not built here.
- Drop increment 4 (free at true last-use across blocks) -> Task 4.
- Soundness rule + disjointness + decline-on-ambiguity -> encoded in `multi_block_freeable` and its tests (Task 4), enforced at wiring (Task 5).
- Verification bar (golden unit + ASan + corpus 8/8 + six-lens adversarial) -> Tasks 4, 5, 6, 7.
- Non-goals (linear-on-MIR, drop flags, non-C backends, Vec/Map) -> not touched; split frontiers decline; only `BuildString` owners handled.

**Placeholder scan:** the `MirРValue` in Task 1 Step 1 is a deliberate, called-out retype prompt, not a placeholder. Task 5 Step 1 uses a grep to locate the population site rather than a hard line number because that site was not in the extraction; the grep makes it exact. No "TBD"/"add error handling"/"similar to Task N" remain.

**Type consistency:** `Liveness { live_in, live_out }`, `compute`, `buffer_live_in`/`buffer_live_out`, and `multi_block_freeable(func, candidates: &[(LocalId, usize)], fn_exit: &HashSet<LocalId>, block_scoped: &HashSet<LocalId>) -> HashMap<u32, Vec<LocalId>>` are used identically in every task that references them. `sound_owned_candidates` returns `Vec<(LocalId, usize)>` (verified from `c.rs`), matching the `candidates` parameter.
