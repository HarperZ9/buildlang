# Held-out agent-writability probe (bench5, M3): preregistration

Written 2026-11-08 before any generator sees the tasks. Not edited after the hash is
committed and pushed. Results go in `RESULTS.md`; failures become issues and fixes in later
PRs, never edits to this folder's frozen files.

## Decision it informs

M3 in `docs/STRATEGY-2026-10.md`: "pass@1 of at least 25/30 for each of two independent
generators, and zero check-pass-but-fail programs", graded against the current crates.io
release. M3 is the first stage of `docs/LANGUAGE-COMPLETE.md`.

- Both samples meet the bar: M3 PASS on 1.11.0, and the row in LANGUAGE-COMPLETE says so.
- Either sample below the bar, or any check-pass-but-fail: M3 FAIL on 1.11.0. Each failure
  class becomes an issue; check-pass-but-fail classes come first (the checker is the truth).
A miss is reported as a miss.

## Bar, scaled to 34 tasks (fixed here, before the run)

The strategy's bar is 25 of 30 (83.3%) with 0 check-pass-but-fail. This probe has 34 tasks, so
the bar is 29 of 34 (the same rate, rounded up) per sample, and check_pass_but_fail 0 per
sample. Also reported, not a second bar: the same counts on tasks 1 to 30 alone against 25.

## Tasks

`TASKS.md`, 34 tasks written for this probe on 2026-11-08: 26 general tasks and 8
game-shaped tasks (fixed-step simulation: projectile, particles, duel_trace, life; data table
load: units_table, inventory_log, grid_path; trace writing: duel_trace; seeded generator:
dice). Expected outputs come from `reference.py`, materialized in `expected.json`
(`python reference.py --check` confirms they agree).

Held-out controls applied while writing: none of bench4's 27 task ids or task statements is
reused; tasks were written without reading the frozen docs. Known overlap, recorded: the docs
teach hand-written digit conversion (spec 7.4 and SKILL.md), which `hex_parse`, `parse_sum`,
`score_check`, `units_table` and `inventory_log` need; `stock` and `units_table` share shape
(a struct per row) with bench4's `ledger` and `csv`. The author of the tasks (this agent) wrote
the 1.11.0 compiler changes the probe runs against, and knows the language; the tasks were not
tuned to those changes (no task needs an opaque handle; `Vec::insert`/`remove` and `push(char)`
are usable in `josephus`, `brackets`, `hex_parse` and `reverse_words` but each has other forms).

## Frozen docs

BuildLang tag v1.11.0 (commit 38e63eb): `SPECIFICATION.md`, `docs/EFFECTS_GUIDE.md`,
`skills/buildlang/SKILL.md`, `llms.txt`, copied into `context/` with hashes in
`context/SHA256SUMS`.

## Compiler under test

`buildlang` 1.11.0 installed from crates.io with `cargo install buildlang --version 1.11.0
--locked` into a scratch root; `BUILDLANG_STDLIB` unset.

## Generators (the route bench4 used)

Fresh Claude Opus 5.5 general-purpose subagents launched from the probe session, prompt in
`PROMPT.md`, sent verbatim. They may read only the task file and the four doc files and write
only answer files; they run nothing. Compliance rests on the agent's report. Two samples, A and
B, one attempt per task each. Independence is limited and stated: both samples are the same
model with the same prompt, so they are independent draws, not independent models.
Temperature is not controllable here and unknown.

## Grading and analysis

`grade.py` and `analyze.py` are the bench4 harness (`bench/heldout-2026-10-15`) with only the
task count, the bar and the probe name changed (diff recorded in `HARNESS-DIFF.md`). Outcomes
in the order checked: missing, check_fail, build_fail, crash, wrong, pass.
check_pass_but_fail = build_fail + crash; check_pass_not_pass adds wrong.

- Primary, per sample: pass@1 out of 34 with a Wilson 95% interval, and check_pass_but_fail.
- Secondary: tasks failed in both samples; per-task outcomes; the game-shaped subset.
- No pooling of A and B into one interval.

## Predictions (before the run)

- pass@1 between 24 and 31 per sample (bench4 was 20 and 17 of 27 on 1.6.0; many of its
  failure classes are fixed). Low confidence.
- check_pass_but_fail 0 or 1 per sample. Low confidence.
- The game-shaped tasks fail more often than the rest (file reading and JSON-shaped output
  text). Moderate confidence.
