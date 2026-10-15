# Held-out agent-writability probe (bench4): preregistration

Written 2026-10-15 before any generator sees the tasks. Not edited after the
hash is committed. Results go in `RESULTS.md`; failures become issues and
fixes in later PRs, never edits to this folder's frozen files.

## Why

Run 3 (`../bench3/`) scored 10 of 10 but is contaminated: SPECIFICATION.md 7.4
and the agent skill were written after run 2's failures, and 7.4 mirrors one
task almost verbatim. This probe asks the question run 3 could not: does
BuildLang 1.6.0, with its public docs, let a fresh agent write correct
programs for tasks nobody tuned the docs to?

## Decision it informs

- Meets the bar (below): the next work is M2 as planned (exact replacements in
  suggestions, `vec_new()` element inference, retiring the language server's
  older pattern checks).
- check_pass_but_fail above 0: M1 ("the checker is the truth") is reopened
  for that class, and the fix lands before or with the first M2 PR.
- pass@1 below the bar with check_pass_but_fail 0: each failure class becomes
  an issue; classes the checker or docs can close are scheduled into M2.
A miss is reported as a miss.

## Tasks

`TASKS.md`, 27 tasks, written for this probe on 2026-10-15. Expected outputs
come from Python reference solutions in `reference.py`, materialized in
`expected.json` (`python reference.py --check` confirms they agree).

Held-out controls applied while writing:
- Written before reading the frozen docs, then checked mechanically for theme
  overlap against the four doc files (keyword counts; see below).
- None of the 10 tasks from runs 1 to 3 is reused. Two drafts were changed
  because they resembled earlier tasks or doc examples: an exit-status task
  that summed integers from a file (too close to run 1's `file_sum` and the
  spec 7.4 example) became a sortedness check, and `squares_file` prints the
  last line instead of a sum.
- Remaining overlap, recorded: the docs teach hand-written digit conversion
  (spec 7.4, SKILL.md), which `rpn`, `config`, `csv` and `parse_fields` need.
  That is a technique, not a task. The effects guide's `load_config` examples
  share the word "config" and the FileSystem effect with task `config`; the
  task (key=value parsing with comments and trimming) is not in the docs.

## Frozen docs

BuildLang tag v1.6.0, commit be65834dfa21918c8737e4695d2043dceccd4b5c:
`SPECIFICATION.md`, `docs/EFFECTS_GUIDE.md`, `skills/buildlang/SKILL.md`,
`llms.txt`, copied into `context/` with hashes in `context/SHA256SUMS`.
These are the public, agent-facing docs shipped with 1.6.0.

## Compiler under test

`buildlang` 1.6.0 installed from crates.io with
`cargo install buildlang --version 1.6.0 --locked` into a scratch root;
`BUILDLANG_STDLIB` unset, so the stdlib embedded in buildc is used.

## Generator (same harness as runs 1 to 3)

Fresh Claude Opus 5.5 general-purpose subagents launched from this session,
prompt in `PROMPT.md`, sent verbatim. They may read only the task file and
the four doc files and write only answer files; they run nothing. Compliance
rests on the agent's report, as before. Two independent samples, A and B,
each one attempt per task. Temperature is not controllable here and unknown.

## Grading and analysis

`grade.py` (outcomes in the order checked: missing, check_fail, build_fail,
crash, wrong, pass; details in its docstring) and `analyze.py`.

- Primary: sample A pass@1 out of 27 with a Wilson 95% interval.
- Primary: sample A check_pass_but_fail = build_fail + crash (passed
  `buildc check`, then failed to build, timed out or terminated abnormally).
- Reported with it: check_pass_not_pass (adds `wrong`), per-task outcomes.
- Secondary: sample B with the same statistics; tasks failed in both samples;
  repair@1 for sample A's failures (one fresh subagent, prompt in `PROMPT.md`,
  given the grader report for each failed task).
- No pooling of A and B into one interval: the samples share tasks, so a
  pooled interval would be too narrow.

## Pass bar

pass@1 at least 22 of 27 (81%) and check_pass_but_fail equal to 0, on
sample A. Sample B is reported beside it and does not change the verdict.

## Predictions (before the run)

- Sample A pass@1: 15 to 21 of 27, so below the bar. Likeliest failures:
  recursive types (`bst`), generics over strings (`largest`), float
  formatting (`stats`), process exit codes (`exit_status`), function
  parameters (`higher_order`), and string slicing or character arithmetic
  (`caesar`, `palindromes`, `parse_fields`).
- check_pass_but_fail: 1 or 2, most likely in recursive types or generics,
  which the earlier probes never exercised.

## Grader validation before sealing

A control set of hand-written programs (outside `gen/`) was graded before
the hash: one correct answer, one correct answer that must exit 3, one wrong
output, one check failure, one wrong output file, one timeout, one stack
overflow, and missing
files for the rest. Expected and observed outcomes are in
`controls/CONTROLS.md`.

## Limits

n=27 per sample, one attempt per task, one model family, and the same model
family wrote the tasks and the answers, so difficulty may be tuned to it
without intent. Small programs only; no claim about larger programs, other
models, or security properties.

## Harness change from runs 1 to 3

Runs 1 to 3 graded with `buildc run`. This probe builds with `buildc build .`
(the task file copied to `main.bld`) and runs the executable directly, so a
build failure and a crash are told apart and a crash keeps its exit code.
Generation is unchanged.

## Binary under test

buildc 1.6.0, `cargo install buildlang --version 1.6.0 --locked`, Windows
x86-64, SHA-256 69faaecc98e9a44631768e922806120f766e0a96203e8c690f15075d5a6e3a56.
C compiler used by buildc: gcc (as reported by `buildc build`).
