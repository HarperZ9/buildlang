# Increment 5: split-frontier drop flags (memory pillar, W5) - implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans, task-by-task.

**Scope verdict (read first).** Increment 5 as scoped below IS one safe commit:
one additive analysis module, one bounded backend wiring, fixtures, tests, docs.
It is comparable in size to increment 4 (fc982e8: +195 lines wiring; bf3fe5e:
+148 lines analysis). No prerequisite is missing: the liveness and
buffer-liveness overlay in `compiler/src/codegen/analysis/liveness.rs` and the
cfg helpers in `compiler/src/codegen/analysis/cfg.rs` were extracted for exactly
this. What this increment deliberately DEFERS is named in "Decline conditions
that remain" below; the largest deferral is flag-guarded frees at RE-ENTRANT
blocks (loop headers), which the flag mechanism would make sound but which stays
declined here to keep the adversarial surface bounded.

**Goal.** Reclaim heap `BuildString` buffers whose death frontier is SPLIT
across conditional edges (used on one arm of an `if`, not the other) and
buffers whose ALLOCATION is conditional (def block does not dominate the frees),
the two shapes every prior increment declines. Mechanism: a per-buffer runtime
drop flag in the emitted C. Everything stays behind `BUILDLANG_EXPERIMENTAL_FREE`
(default off, flag-off output byte-identical), additive and disjoint from
increments 1-4 (each buffer freed by exactly one mechanism), soundness rule
absolute: never a double free, never a use after free; leaking is the safe
fallback.

**Motivating fixture that leaks today** (verified against the shipped gates by
reading `freeable_owned_string_locals`, `block_scoped_freeable`,
`live_range_confined_to_block`, and `multi_block_freeable`; the split-frontier
decline is the `declines_split_death_frontier` unit test in
`compiler/src/codegen/analysis/drops.rs`):

```
fn main() ~ Console {
    let mut i = 0;
    while i < 1000000 {
        let s = i.to_string() + "!";
        if i % 2 == 0 {
            println!("{}", s);
        }
        i = i + 1;
    }
}
```

`s`'s buffer: allocated every iteration, used only on even iterations. The
function-exit pass declines (def does not dominate the return), increment 3
declines (live range spans blocks), increment 4 declines (the join after the
`if` has one TERMINAL predecessor and one CLEAN predecessor descending from a
live block, the split-frontier signature its `clean_chain_ok` walk rejects).
One million buffers leak.

## The design

### Flag representation in emitted C

Per enrolled owner `L` (MIR `LocalId` value N, C name from
`self.local_name(L, &func.locals)`):

- Declaration, with the other locals at function top:
  `uint8_t __bl_live_N = 0;`
  (`<stdint.h>` is already included by the emitted preamble, c.rs line ~898.
  The `__bl_` prefix matches existing emitted-runtime naming such as
  `__build_init_io`; keyed by the numeric LocalId so it cannot collide with
  another flag. Residual collision with a user identifier literally named
  `__bl_live_N` is accepted as negligible and noted in the design doc.)
- Set, immediately after `L`'s unique definition site (exactly one exists,
  `def_count == 1` is a candidate gate):
  `__bl_live_N = 1;`
- Guarded free, at each free site (block start or before `return`):
  `if (__bl_live_N) { build_string_free(sname); __bl_live_N = 0; }`

### The soundness invariant (state it verbatim in the code comments)

At every point in the emitted C, `__bl_live_N == 1` implies `L` currently holds
a fresh allocated buffer that no free site has released. Established by: init 0
at declaration; set 1 ONLY immediately after `L`'s unique def (an allocating
call or a move-acquire of a fresh buffer whose source is moved-from and
therefore excluded from every free set); cleared to 0 immediately after every
guarded free; `build_string_free` reached only under the flag test. The flag
guards ALLOCATED (double free, uninitialized free). It does not guard LIVENESS:
a site must additionally satisfy buffer-dead-at-entry (no future use of the
owner or any borrow temp on any path), which is what forbids use-after-free.
Both halves are load-bearing.

### Placement rule

For each owner `(L, def_bi)` from `CBackend::sound_owned_candidates(func)`
(the existing ownership/move/taint/one-def/escape gates; carry over the
soundness precondition comment from `drops.rs` lines 19-27: candidates MUST be
escape-filtered, the buffer-liveness overlay is blind to multi-hop `.ptr`
copies) that is NOT in the claimed set
(`fn_exit` from `freeable_owned_string_locals` union `block_scoped` values
union increment-4 `multi_block_freeable` values):

1. Enroll `L` as a flag owner.
2. Compute `live = liveness::compute(func)`,
   `buf_in = buffer_live_in(func, &live, L)`,
   `buf_out = buffer_live_out(func, &live, L)`,
   `terminal[b] = buf_in[b] && !buf_out[b]`,
   preds over `reachable_blocks` via `block_id_index` +
   `terminator_successors`, and `dom = compute_dominators(blocks)`
   (all real, existing functions in `analysis::liveness` / `analysis::cfg`).
3. Frontier free sites: every block `S` with
   - `reachable[S]`, `S != entry`, `preds[S]` nonempty;
   - `!buf_in[S]` (buffer dead at entry: the UAF guard; the overlay already
     includes the owner's move-source chain and one-hop `.ptr` borrow temps);
   - some `p in preds[S]` with `terminal[p]` (a real death happened just
     upstream; this is also what keeps the site set small: blocks after a death
     are clean, so no downstream block re-qualifies);
   - NOT re-entrant: no `p in preds[S]` with `dom[p]` containing `S` (the
     increment-4 loop-header/self-loop exclusion, kept verbatim; the flag would
     make re-entrant sites sound but relaxing that is a named follow-up, not
     this brick).
   Emit a guarded free at the START of each such `S`. Unlike increment 4 there
   is NO dominance requirement (`def_bi` need not dominate `S`: the flag covers
   conditional allocation) and NO uniqueness requirement (multiple sites on
   different paths are fine; the clear makes a later site on the same path a
   no-op).
4. Return backstop: at every `Return`, alongside the existing unguarded
   function-exit frees, emit the guarded free for every flag owner. This
   reclaims paths that bypass every frontier site (early return, break out of
   the loop mid-iteration) and is sound unconditionally: nothing executes after
   the block's statements at the Return terminator, and non-escape (already
   gated) means no pointer derived from `L` survives the function.

Note what this rule does to the motivating fixture: flag set after the concat
call in the loop body, one guarded free at the in-body join (non-re-entrant,
one terminal pred from the used arm, one clean pred from the skip arm), guarded
free before `return`. Per-iteration reclamation, last buffer caught at exit.

## Global constraints

- ONE commit on branch `feat/drop-flags`, stacked on `feat/public-refresh`
  (create from current HEAD ea251d4). Do not push. This plan doc rides in the
  commit (move it to `docs/superpowers/plans/2026-07-29-drop-flags.md`, drop
  the -DRAFT suffix).
- `BUILDLANG_EXPERIMENTAL_FREE` unset: emitted C byte-identical to baseline.
  Verified mechanically (Task 5), not assumed.
- Additive and disjoint: increments 1-4 code paths untouched except the
  read-only wiring in `generate_function`; no owner appears in two mechanisms.
- Every new guard mutation-checked with red/green evidence (Task 6); exit codes
  captured before any pipe (the pipes-swallow-exit-codes trap); no em-dashes in
  any prose this commit adds; `cargo fmt --check` clean.
- `buildc corpus verify` 8/8 with the flag ON and OFF; full `cargo test` from
  `compiler/` 0 failed (current baseline 1605 passed, 11 ignored).
- Adversarial pass (Task 7) runs in an ISOLATED worktree (increment 3's
  working-tree-reverted lesson, recorded in the design doc).
- Test idiom note: no existing test sets `BUILDLANG_EXPERIMENTAL_FREE` (env is
  process-global and cargo tests run in parallel; verified by grep, the env var
  appears only in c.rs source). Keep that discipline: unit tests call the
  analysis functions and emission helper directly; end-to-end flag-on evidence
  is the recorded C-diff + ASan protocol + corpus runs, exactly the increment-4
  idiom.

### Task 1: analysis module `compiler/src/codegen/analysis/flags.rs`

- [ ] New file (keeps `drops.rs` from growing past its 513 lines; register
  `pub(crate) mod flags;` in `compiler/src/codegen/analysis/mod.rs`).
- [ ] Public item:
  ```rust
  pub(crate) struct FlagFrees {
      /// Owners enrolled for flag management, sorted by LocalId. Drives the
      /// flag declaration, the def-site set, and the Return backstop frees.
      pub owners: Vec<LocalId>,
      /// Guarded frees at block START, keyed by the C `bb<id>` (BlockId.0),
      /// values sorted by LocalId. Same key space as increment 3/4's map.
      pub block_frees: HashMap<u32, Vec<LocalId>>,
  }
  pub(crate) fn split_frontier_flag_frees(
      func: &MirFunction,
      candidates: &[(LocalId, usize)],
      claimed: &HashSet<LocalId>,
  ) -> FlagFrees
  ```
  implementing the placement rule above with the named helpers
  (`liveness::compute`, `buffer_live_in`, `buffer_live_out`,
  `cfg::reachable_blocks`, `cfg::block_id_index`, `cfg::terminator_successors`,
  `cfg::compute_dominators`). Carry the escape-filtered-candidates
  precondition comment and the soundness invariant comment.
- [ ] Unit tests in the file, `drops.rs` constructed-MIR idiom (reuse its
  builder shapes; `bs(..)`/`i64_local(..)` helpers can be duplicated locally,
  the existing test mods do the same):
  - `enrolls_split_frontier_and_places_join_site`: the
    `declines_split_death_frontier` CFG from drops.rs (bb0 alloc, bb0b if,
    bb1 use->bb3, bb2 clean->bb3, bb3 return): owner enrolled, exactly one
    block site at bb3, and `multi_block_freeable` on the same input still
    returns empty (the pair proves 4-declines/5-claims).
  - `conditional_alloc_enrolls_with_no_dominance`: alloc inside one arm of an
    if, used in the arm, join then return; def block does NOT dominate the
    join; owner enrolled, site at the join.
  - `uaf_shape_declines_live_join`: owner used again AFTER the join
    (`buf_in[join]` true): join is NOT a site; the only sites are past the
    last use.
  - `declines_reentrant_site`: mirror drops.rs
    `declines_loop_header_death_block` and `declines_self_loop_death_block`:
    no site at a block with a back-edge predecessor.
  - `claimed_owner_not_enrolled`: same CFG as the first test but `claimed`
    contains the owner: `owners` empty, `block_frees` empty (disjointness, the
    no-double-free half).
  - `no_site_without_terminal_pred`: an owner whose buffer is dead everywhere
    reachable from a clean-only region gets no frontier site there (blocks
    after death do not re-qualify).

### Task 2: backend wiring in `compiler/src/codegen/backend/c.rs`

Line anchors below are as of ea251d4; re-locate by the quoted code, not the
number.

- [ ] Two new fields on `CBackend` beside `current_fn_block_frees` (~line 56):
  `current_fn_flag_owners: Vec<LocalId>` and
  `current_fn_flag_block_frees: std::collections::HashMap<u32, Vec<LocalId>>`,
  cleared/initialized in `new()` and recomputed per function.
- [ ] In `generate_function` (~line 1919), inside the existing
  `if Self::experimental_free_enabled()` block AFTER the increment-4 merge
  (~line 1960): build `claimed` = fn_exit set union block-scoped values union
  the increment-4 extra values (the sets are already materialized there), call
  `analysis::flags::split_frontier_flag_frees(func, &candidates, &claimed)`
  (reuse the `candidates` vec already computed at ~line 1944), store both
  fields. Flag off: both stay empty, so every emission below is a no-op and
  the baseline is byte-identical.
- [ ] Flag declarations: in the local-declaration loop's tail (~line 2002,
  after the `for local in &func.locals` declaration loop), for each flag owner
  emit `uint8_t __bl_live_<id> = 0;` via `write_indent` + `writeln!`. Before
  coding, READ `collect_used_locals` (~line 4470) and confirm a `Call` dest
  counts as used (increment 1 frees never-referenced Call dests, so it must;
  verify, do not assume). If it somehow does not, add flag owners to the
  used set rather than weakening dead-local elimination.
- [ ] Def-site flag set, allocating-call case: in `generate_terminator`'s
  `MirTerminator::Call` arm, the GENERIC tail that emits `dest = callee(...)`
  and then `goto` (~lines 3320-3338). After the dest assignment and before the
  goto, if `dest` is a flag owner, emit `__bl_live_<id> = 1;`. The arm has
  early-exit special cases (vtable dispatch ~line 2880, and others); READ the
  full arm and confirm none of them can carry a callee from
  `allocates_owned_string` (closed 6-name list, plain named runtime calls;
  the special cases key on `__vtable_dispatch_`/`intrinsic_` prefixes). Record
  that reading in the commit body. A missed set fails SAFE (flag stays 0,
  buffer leaks); a spurious set is impossible because the emission is keyed on
  `dest == flag owner` and `def_count == 1`.
- [ ] Def-site flag set, move-acquire case: in `generate_statement`'s
  `MirStmtKind::Assign` arm, after the emitted assignment, if the dest is a
  flag owner emit the same set line (`def_count == 1` makes any Assign to a
  flag owner its unique def; the candidate gates only admit allocating-Call
  and `Use(Local)` defs).
- [ ] Emission helper + guarded sites: add
  `fn emit_flag_guarded_free(&mut self, owner: LocalId, locals: &[MirLocal])`
  producing exactly
  `if (__bl_live_<id>) { build_string_free(<name>); __bl_live_<id> = 0; }`.
  Call it (a) at block start, immediately after the existing increment-3/4
  unguarded loop (~line 2055), for `current_fn_flag_block_frees[block.id.0]`;
  (b) in the `MirTerminator::Return` arm, immediately after the existing
  `current_fn_freeable` unguarded frees (~line 3353), for every flag owner.
  Do NOT merge flag owners into the existing maps: those emit unguarded frees.
- [ ] Unit tests in the c.rs test mod: exact-text test for
  `emit_flag_guarded_free` (guard, free, clear, all on the owner's real local
  name); a test that a fn constructed like the split-frontier CFG yields
  disjoint sets (`freeable_owned_string_locals`, `block_scoped_freeable`,
  `multi_block_freeable`, `split_frontier_flag_frees` pairwise disjoint
  owners); the multi-hop-alias idiom test (~line 5621) extended to assert the
  escaping owner is also absent from `split_frontier_flag_frees` output.
- [ ] Trivial-goto interaction: after generating the fixture's on.c, eyeball
  that `eliminate_trivial_gotos` did not detach a guarded free from its block
  (increments 3/4 already emit at block starts and survive it; confirm, note
  in commit body).

### Task 3: fixtures and C-diff evidence

- [ ] `compiler/tests/mem/split_frontier_loop.bld`: the motivating fixture
  above, 1,000,000 iterations (increment-4 file header comment idiom:
  state which increments decline it and why).
- [ ] `compiler/tests/mem/conditional_alloc.bld`: allocation inside a taken/
  not-taken `if` at function scope with a tail after the join, e.g.
  ```
  fn main() ~ Console {
      let mut i = 0;
      if i < 1 {
          let s = i.to_string() + "!";
          println!("{}", s);
      }
      while i < 3 { i = i + 1; }
      println!("done");
  }
  ```
  This is the would-be UNINITIALIZED-FREE shape: an unguarded join free would
  free garbage `cap` on the skip path. The emitted guard is what makes it
  sound; the ASan mutation run in Task 6 proves the guard is load-bearing.
- [ ] C-diff confirmation, increment-4 idiom (record the actual diff hunks in
  the design doc section):
  ```
  buildc compiler/tests/mem/split_frontier_loop.bld --target c -o off.c
  set BUILDLANG_EXPERIMENTAL_FREE=1
  buildc compiler/tests/mem/split_frontier_loop.bld --target c -o on.c
  fc off.c on.c
  ```
  Expected on-diff, and nothing else: one `uint8_t __bl_live_N = 0;`, one
  `__bl_live_N = 1;` after the `build_string_concat` call, one guarded free at
  the in-body join, one guarded free before `return`. Reason through the MIR
  (as the increment-4 section did) that the join is the split frontier
  increment 4 declines, so the free is increment 5's.
- [ ] Double-free negative regression: regenerate on.c for the EXISTING
  `compiler/tests/mem/multi_block_loop.bld` and assert its diff against off.c
  is STILL exactly the single unguarded `build_string_free(s);` line from
  increment 4 (no flag machinery: disjointness at the emitted-C level; this is
  the would-be double-free shape, a buffer already freed by increment 4 must
  not gain a second freer).

### Task 4: ASan verification protocol (exact commands)

MSVC ASan is the protocol; it is native Windows, WSL is NOT required (confirmed
working 2026-06-30 in the design doc and used by increment 4). Run from a
plain cmd shell:

```
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
cl.exe /nologo /std:c11 /fsanitize=address /Fe:on.exe on.c
cl.exe /nologo /std:c11 /fsanitize=address /Fe:off.exe off.c
```

- [ ] Run each exe via a `.bat` wrapper redirecting stdout/stderr to files
  (pipe redirection deadlocks past the pipe buffer on the 1.5M-line output;
  recorded increment-4 methodology), with the MSVC bin on PATH so the ASan
  runtime DLL resolves. PASS = exit 0, zero `AddressSanitizer:` lines on
  stderr, and COMPLETE output verified by line count and final line
  (`split_frontier_loop`: 500,000 lines ending `999998!`; check the count
  against the fixture's actual predicate before asserting). Two independent
  runs of on.exe; one of off.exe.
- [ ] Same ASan compile+run for `conditional_alloc.bld` on.exe (both branches
  of its predicate if feasible by editing the constant, else the taken path
  plus the Task 6 guard mutation covers the skip path).
- [ ] Peak memory WITHOUT ASan (`cl /nologo /std:c11 /O2`), because ASan's
  quarantine masks the reclaim: poll `PeakWorkingSet64` per the recorded
  increment-4 method. Claim DIRECTION only (on.exe peaks below off.exe),
  record the numbers, do not claim a general multiplier.
- [ ] Fallback if MSVC ASan is unavailable on the executing machine: there is
  no substitute lane; the verification bar stays OPEN, the increment does NOT
  merge, and the design doc entry records exactly which evidence exists
  (unit + C-diff + corpus) and which is missing (ASan). Honest block, not a
  workaround.
- [ ] `buildc corpus verify` 8/8 with the flag ON and with it OFF (capture
  exit codes directly, no pipes).

### Task 5: flag-off byte-identity gate

- [ ] With `BUILDLANG_EXPERIMENTAL_FREE` unset, generate C for both mem
  fixtures and at least two corpus programs at the parent commit and at the
  increment commit; `fc` each pair: zero differences. (The wiring computes
  nothing when the env var is unset; this proves it.)

### Task 6: mutation checks (every new guard; break, observe red, restore, observe green)

- [ ] Remove `!buf_in[S]` from the site rule: `uaf_shape_declines_live_join`
  red.
- [ ] Remove the `claimed` skip: `claimed_owner_not_enrolled` red.
- [ ] Remove the terminal-pred requirement: `no_site_without_terminal_pred`
  red.
- [ ] Remove the re-entrant exclusion: `declines_reentrant_site` red.
- [ ] Remove the `__bl_live_N = 0;` clear from the emission helper: helper
  text test red; ADDITIONALLY compile+run `split_frontier_loop` under ASan
  with the mutation: double free (frontier site then Return backstop on the
  same path) reported. Record the ASan line as evidence, restore, re-run
  clean.
- [ ] Change the flag initializer `= 0` to `= 1`: helper/decl text test red;
  ADDITIONALLY `conditional_alloc` under ASan on the skip path frees garbage
  (crash or ASan report). Record, restore, re-run clean.
- [ ] Remove the def-site `__bl_live_N = 1;` emission: C-diff check red (the
  set line is part of the expected on-diff); note this mutation fails SAFE
  (leak) and say so in the evidence.

### Task 7: adversarial pass (isolated worktree, before merge)

- [ ] Six-lens pass in a `git worktree` (never the working tree), lenses
  matching the increment-3/4 register: (1) flag-invariant attack (find any
  emission path that sets the flag without a fresh allocation, or frees
  outside the guard); (2) borrow-overlay blindness (multi-hop `.ptr`, `Ref`/
  `AddressOf`: must already be rejected by `owned_string_escapes` before the
  overlay is consulted); (3) move-chain aliasing (conditional move, multi-move
  taint); (4) re-entrance (construct any path executing a frontier site twice
  for one allocation); (5) Return-backstop interaction with the unguarded
  fn-exit frees (same Return, disjoint owners); (6) special-case Call
  emission paths (vtable/intrinsic) smuggling an allocating callee past the
  def-site set. Each lens either constructs a runnable counterexample
  (fix before merge) or records why it cannot.

### Task 8: docs + changelog, same commit

- [ ] `docs/MEMORY-PILLAR-DESIGN.md`: new section "### Increment 5:
  split-frontier drop flags" after the increment-4 section, same register:
  the rule, the flag invariant verbatim, the two hazards caught in design
  (Return-backstop double free without the clear; uninitialized free without
  the guard on conditional allocation), the C-diff hunks, the ASan + peak
  numbers, the verification bar, and the kept re-entrant exclusion with the
  note that flags make relaxing it a candidate follow-up.
- [ ] `STATUS.md` Runtime: GC paragraph (line ~106): replace the trailing
  honest-scope clause with: increment 5 ships flag-guarded frees for split
  and conditional death frontiers behind the same opt-in flag; per-buffer
  `uint8_t` drop flags, set at the unique allocation/move-acquire, tested and
  cleared at every free; frees land at non-re-entrant death-frontier blocks
  plus a guarded Return backstop; ASan-clean on a 1,000,000-iteration
  conditional-use loop. Then the remaining declines (list below) and the
  unchanged closer: the memory pillar is NOT done and the flag is NOT
  default-on.
- [ ] `CHANGELOG.md` Unreleased: one bullet, sibling register, honest scope
  included.
- [ ] Commit subject: `feat(codegen): split-frontier drop flags (increment 5,
  opt-in)`; body records the reading-confirmations from Task 2, the evidence
  summary, and ends with the Co-Authored-By line per repo convention. ONE
  commit, do not push.

## Decline conditions that REMAIN after increment 5 (name them everywhere)

1. Everything `sound_owned_candidates` rejects: escaping owners (returned,
   stored into aggregates/globals/containers/slices, passed to any callee off
   the closed `borrows_string_arg` list, `Ref`/`AddressOf`, multi-hop `.ptr`
   copies), moved-from sources, multi-move-tainted owners, reassigned owners
   (`def_count != 1`; flags for reassignment need clear-on-overwrite, a later
   brick), and whole modules tripping the mutable-global alias guard.
2. Allocations outside the closed 6-name `allocates_owned_string` list
   (`build_read_file`, the `build_string_*` transforms, `build_args_get`,
   etc. still leak; broadening the audited list is its own increment).
3. Re-entrant frontier sites (loop headers, self-loops): declined here even
   though the flag would guard them; only the Return backstop reclaims those
   owners, so a loop whose ONLY death frontier is its header still leaks
   per-iteration (safe).
4. Conditional moves: the not-moved path's buffer leaks (the source is
   excluded wholesale).
5. `BuildVec`/`BuildMap`/hvec buffers: `build_vec_free`/`build_hvec_free`
   remain dead code; strings only, all five increments.
6. C backend only; the MIR builder still inserts no `Drop` terminators; other
   backends reclaim nothing.

## Riskiest design decision and its mitigation

Replacing the static once-per-allocation proof (dominance + unique death block,
increment 4) with a RUNTIME invariant carried by emitted C across every backend
emission path. The failure asymmetry is the mitigation's core: a missed
flag-SET leaks (safe); only a spurious set or a guard/clear omission can
corrupt, and those are pinned by exact-text emission tests, the C-diff
expected-hunk check, two targeted ASan mutation runs (clear removed; init
flipped), and adversarial lens 6 on the Call arm's special-case exits. The
UAF half never depends on the flag at all: it rests on `!buf_in[S]` over the
same buffer-liveness overlay increment 4 already trusts, with the same
escape-filtered-candidates precondition.
