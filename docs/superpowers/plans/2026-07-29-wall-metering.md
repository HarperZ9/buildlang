# Wall-Clock Metering Implementation Plan (endgame queue W2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** The receipt gains its first EXECUTED time fact: the witnessed run's wall-clock duration, measured by buildc and sealed, with an optional DECLARED wall ceiling on the budget block whose exceeded flag is derived mechanically. This moves the budget discipline one step from declaration toward metering without pretending buildc can meter what it cannot (step counts stay declared; wall time is the one thing the harness genuinely observes).

**Architecture:** `runtime_state` (the existing executed-facts block) gains `wall_seconds: Option<f64>`, measured around the primary program run at emit and sealed; verify re-measures its own re-run and REPORTS the fresh number beside the sealed one, never requiring agreement (timing is environmental, exactly like raw stdout bytes). The `budget` block gains `wall_seconds_limit: Option<f64>` (declared via `--budget-wall-seconds`, valid only alongside the existing steps pair) and `wall_exceeded: Option<bool>`, DERIVED as `measured > limit` and re-derivable at verify from the two sealed numbers (FIELD_CONTRACT_VIOLATION on inconsistency).

## Global constraints

- ONE commit on branch `feat/wall-metering` (checked out, stacked on feat/five-modes-chain); do not push; the plan doc rides in the commit.
- Backward compatible: old receipts (no `wall_seconds`, no wall fields in budget) parse and re-seal byte-identically (`Option` + `serde(default, skip_serializing_if = "Option::is_none")` on every new field).
- Every new gate mutation-tested with red/green evidence; inverse-edit restores only; exit codes captured before any pipe; no em-dashes; `cargo fmt --check` clean; corpus stays 27/27; self-test stays 9/9 (no new tamper case: the wall fields are covered by the seal and the derived-flag contract, and a tenth case would tamper the same FIELD_CONTRACT_VIOLATION arm the budget case already exercises; the docs say so).
- Sibling reference commits in `git log`: the budget slice for block-field contracts, the cross-backend slice for report-only executed facts.

### Task 1: measurement and sealing (main.rs + scientific_runtime.rs)

- [ ] In `compile_and_capture_run`, measure the child's wall time with `std::time::Instant` around the `run_cmd.output()` call and add `wall_seconds: f64` (rounded to 3 decimal places: `(secs * 1000.0).round() / 1000.0`, millisecond precision keeps the JSON tidy and the fact honest) to `CapturedRun`.
- [ ] `ScientificRuntimeState` gains `#[serde(default, skip_serializing_if = "Option::is_none")] pub wall_seconds: Option<f64>,`; the emit path seals `Some(captured.wall_seconds)`; `ScientificReceiptInputs` carries it.
- [ ] `RerunObservation` gains `pub wall_seconds: f64` filled by both verify closures (same measurement in the shared rerun helper); the evaluator REPORTS it: `ScientificVerifyReport` gains `wall_seconds_sealed: Option<f64>` and `wall_seconds_remeasured: f64`; the human MATCH line appends `wall_seconds=<sealed>~<remeasured>` only when the sealed value exists; the JSON report includes both under a `wall_seconds` object when sealed exists. NO pass/fail on any timing value, ever; the doc comment says why (environmental, like raw bytes).

### Task 2: the declared ceiling on the budget block

- [ ] `ScientificBudget` gains `#[serde(default, skip_serializing_if = "Option::is_none")] pub wall_seconds_limit: Option<f64>,` and `#[serde(default, skip_serializing_if = "Option::is_none")] pub wall_exceeded: Option<bool>,`.
- [ ] CLI: `--budget-wall-seconds <LIMIT>` (f64). Gates at emit: refused without the steps pair (the wall ceiling is a member of the budget declaration, not a freestanding one); refused when `<= 0.0` or non-finite; when present, emit sets `wall_exceeded: Some(measured > limit)` from the sealed measurement; when absent both new fields stay None.
- [ ] Verify contracts beside the existing budget contracts, FIELD_CONTRACT_VIOLATION each: `wall_seconds_limit` present requires a positive finite value AND requires `runtime_state.wall_seconds` present AND `wall_exceeded` present and exactly equal to `sealed wall_seconds > sealed limit`; `wall_exceeded` present without `wall_seconds_limit` refused; `wall_seconds_limit` on a receipt with no budget block cannot exist structurally (field lives inside the block) but a block with ONLY wall fields and zeroed steps is already refused by the existing steps contracts (assert this stays true in a test rather than adding a gate).
- [ ] IMPORTANT subtlety: `wall_exceeded` is derived from the SEALED measurement at emit, never from verify's re-measured time (a slower verify machine must not flip a receipt's coherence); state this in the field's doc comment and check it in the contract using only sealed values.

### Task 3: tests

- [ ] Unit tests (scientific_runtime tests module, sibling idiom): a receipt with sealed wall_seconds and a budget carrying limit+correct exceeded verifies Ok; exceeded flag inconsistent with the sealed numbers refused; wall_exceeded without limit refused; limit without sealed wall_seconds refused; a receipt with NO new fields round-trips with no new keys in its JSON (backward-compat pin).
- [ ] CLI test `wall_metering_seals_and_reports`: emit greedy_change_budget.bld with the budget flags plus `--budget-wall-seconds 300` (generously above any real run), assert receipt has `runtime_state.wall_seconds > 0`, `budget.wall_seconds_limit == 300.0`, `budget.wall_exceeded == false`, verify exits 0 and its stdout mentions `wall_seconds`; refusals: `--budget-wall-seconds 300` without the steps pair; `--budget-wall-seconds 0`.
- [ ] Mutation checks: (a) break the exceeded-consistency verify gate, unit test red, restore green; (b) break the CLI without-steps refusal, cli test red, restore green.

### Task 4: docs + changelog, same commit

- [ ] SCIENTIFIC-RECEIPT.md: `runtime_state` bullet gains wall_seconds (executed, sealed, reported-not-required at verify); the budget flags bullet gains `--budget-wall-seconds` with the derived-from-sealed rule; the budget schema bullet gains the two fields; one sentence on why there is no tenth self-test case.
- [ ] CHANGELOG Unreleased: new top bullet, sibling register.

### Task 5: verification gate

- [ ] Full suite 0 failed (exit captured before any pipe); corpus 27/27; self-test 9/9; fmt clean; ONE commit, subject "feat: wall-clock metering as the first executed budget fact", body in the sibling register, ending with: Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>. Do NOT push.
