# Budgeted-Search Receipts Implementation Plan (slice 3 of the five-modes brief)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A heuristic (budgeted-search) run's receipt carries its budget ceiling, its consumption, and an exhausted flag as a sealed `budget` block, and mechanically refuses to claim optimality, so a search result can never enter the evidence layer pretending it proved more than a budgeted search can prove.

**Architecture:** The block mirrors the flywheel receipt v3 lesson (a result without its budget ceiling hides whether it stopped at the limit) in the shipped author-DECLARED idiom of `monte_carlo`: CLI-declared facts with hard shape contracts verify re-checks. Two mechanical honesty rules ride with it: a budgeted receipt's `labels` gains `NOT_PROVES_OPTIMALITY` and its `not_claimed` gains `optimality` (both re-derived at verify), and the author's free-text fields may not contradict the label (the claim-language rule: `--method` / `--problem` text containing "optimal" is refused on a budgeted run).

**Tech stack:** Rust (buildc compiler), the shipped scientific-receipt slice pattern. The two reference implementations to imitate are in this branch's own history: `git show` the commits titled "feat: Random capability with witnessed-seed scientific receipts" and "feat: Monte Carlo estimator receipts (five-modes slice 2)". Follow their structure file by file; this plan states what differs.

## Global constraints

- ONE commit on branch `feat/budget-receipts` (already created and checked out). Do not push. Do not touch main.
- Positive and negative fixtures; every new gate mutation-tested: break it, watch the covering test fail, restore it, watch it pass. Record each mutation and its red/green result in your report.
- Backward compatible: receipts without the block keep their exact bytes and seals (`Option` + `serde(default, skip_serializing_if = "Option::is_none")`).
- No em-dash characters anywhere. No local drive paths in docs. ASCII only in .bld files and JSON.
- Full suite green with the exit code captured BEFORE any pipe (`cargo test ... > log 2>&1; echo $?` then grep the log; never `cargo test | tail`).
- `cargo fmt --manifest-path compiler/Cargo.toml` before the commit; `cargo fmt --check` must pass.
- These kernels are deterministic (no Random): a budget block does NOT require a seed. Do not couple it to `Random`.

### Task 1: the sealed block and its contracts (compiler/src/scientific_runtime.rs)

- [ ] Add after `ScientificMonteCarlo`:

```rust
/// The budgeted-search admission block: a heuristic result without its
/// budget ceiling hides whether it stopped at the limit, so the ceiling,
/// the consumption, and the exhausted flag seal together. Author-DECLARED
/// like [`ScientificMonteCarlo`], with shape contracts verify re-checks:
/// the ceiling is non-zero, consumption never exceeds it (a consumption
/// above its ceiling is incoherent), and `exhausted` is DERIVED
/// (`steps_consumed == steps_limit`), never hand-set. A budgeted receipt
/// additionally carries `NOT_PROVES_OPTIMALITY` in `labels` and
/// `optimality` in `not_claimed`, both re-derived at verify: a budgeted
/// search may report its incumbent, never a proof of optimality.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScientificBudget {
    /// The declared step ceiling. Non-zero.
    pub steps_limit: u64,
    /// The declared steps consumed. At most `steps_limit`.
    pub steps_consumed: u64,
    /// Whether the search ran to its ceiling: EXACTLY
    /// `steps_consumed == steps_limit`, re-derived at verify.
    pub exhausted: bool,
    /// `DECLARED` (v0): the facts were stated, not independently metered.
    pub status: String,
}
```

- [ ] `ScientificRuntimeReceipt` gains, directly after `monte_carlo`:

```rust
    /// The budgeted-search admission block, present IFF the run declared a
    /// budget (`--budget-steps` + `--budget-consumed`). Optional-with-default
    /// so receipts sealed before this block existed keep their exact bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<ScientificBudget>,
```

- [ ] `ScientificReceiptInputs` gains `pub budget: Option<ScientificBudget>,` after `monte_carlo`; the builder destructures it and stores it in the receipt literal after `monte_carlo`.
- [ ] In the builder, the labels/boundary rule (place beside the existing `labels` construction): when `budget.is_some()`, push `"NOT_PROVES_OPTIMALITY".to_string()` onto `labels` and `"optimality".to_string()` onto the `not_claimed` vector (append after the `NOT_CLAIMED_BOUNDARY` copy).
- [ ] Verify contracts, in `evaluate_scientific_runtime_receipt` directly after the `monte_carlo` contract block, all `FIELD_CONTRACT_VIOLATION` with a one-line `eprintln!` naming the violation in the style of the mc block's messages:
  - `budget.steps_limit == 0` refused (a zero ceiling is not a budget).
  - `budget.steps_consumed > budget.steps_limit` refused (incoherent).
  - `budget.exhausted != (budget.steps_consumed == budget.steps_limit)` refused (the flag is derived, never hand-set).
  - `budget.status != "DECLARED"` refused.
  - The label pairing, checked for EVERY receipt (not only budgeted ones): `receipt.labels.contains("NOT_PROVES_OPTIMALITY")` must equal `receipt.budget.is_some()`, and `receipt.not_claimed.contains("optimality")` must equal `receipt.budget.is_some()`. A budgetless receipt claiming the label, or a budgeted one missing it, is refused.
  - The claim-language rule on the sealed strings: if `budget.is_some()` and either `receipt.problem.label` or `receipt.numerical_method.description` contains the substring `optimal` case-insensitively, refuse (message: a budgeted search reports its incumbent, never optimality; the free text may not contradict NOT_PROVES_OPTIMALITY).
- [ ] Self-test tamper case 8, after case 7: if the receipt has a budget block, set its `steps_consumed` to `steps_limit + 1`; if not, add `{"steps_limit": 10, "steps_consumed": 11, "exhausted": false, "status": "DECLARED"}`; reseal; expected class `FIELD_CONTRACT_VIOLATION`. Update the pinned class-list test (`self_test_cases_cover_distinct_failure_classes_and_seal_states`) to expect the extra `FIELD_CONTRACT_VIOLATION` entry.
- [ ] `base_inputs` test fixture gains `budget: None,`.
- [ ] Unit test `verify_enforces_the_budget_admission_contracts`, modeled line for line on `verify_enforces_the_monte_carlo_admission_contracts` (same nested-fn structure, default Console policy since no Random is needed): a receipt with `ScientificBudget { steps_limit: 500, steps_consumed: 437, exhausted: false, status: "DECLARED" }` verifies Ok and its JSON carries the block plus the `NOT_PROVES_OPTIMALITY` label and `optimality` boundary entry; zero limit refused; consumed > limit refused; hand-set `exhausted: true` with consumed < limit refused; status `METERED` refused; a receipt built with a budget and `problem_label: Some("greedy-change-optimal".to_string())` refused (claim language); a budgeted receipt whose `NOT_PROVES_OPTIMALITY` label is stripped after building (`receipt.labels.retain(|l| l != "NOT_PROVES_OPTIMALITY")` then `seal_receipt(&mut receipt)`) refused, and symmetrically a budgetless receipt with the label pushed and re-sealed refused (these two are the covering tests for the label-pairing rule, so its mutation check can go red); a budgetless receipt's JSON has no `budget` key and no `NOT_PROVES_OPTIMALITY` label.

### Task 2: CLI admission (compiler/src/main.rs)

- [ ] Import `ScientificBudget` beside `ScientificMonteCarlo`.
- [ ] Run flags after the `mc_interval` flag, doc comments in the same voice:

```rust
        /// Declare the run a budgeted search: the step ceiling. Both
        /// --budget-* flags declare together or not at all. A budgeted
        /// receipt carries NOT_PROVES_OPTIMALITY and refuses free text
        /// claiming optimality.
        #[arg(long, value_name = "LIMIT")]
        budget_steps: Option<u64>,

        /// The declared steps consumed (at most the ceiling).
        #[arg(long, value_name = "N")]
        budget_consumed: Option<u64>,
```

- [ ] Thread `budget_steps` / `budget_consumed` through the `Commands::Run` match arm into `cmd_run` (and refuse them with `--gpu` exactly as the mc flags are refused there). In `cmd_run`, directly after the monte_carlo validation block, build `let budget: Option<ScientificBudget>` with the same all-or-nothing shape: neither flag -> `None`; both -> validate `steps_limit > 0` (message: a zero ceiling is not a budget) and `steps_consumed <= steps_limit` (message: consumed exceeds the declared ceiling; a consumption above its ceiling is incoherent), then `Some(ScientificBudget { steps_limit, steps_consumed, exhausted: steps_consumed == steps_limit, status: "DECLARED".to_string() })`; exactly one flag -> refuse (message: a budget declares its ceiling AND its consumption together (--budget-steps, --budget-consumed); a result without its budget ceiling hides whether it stopped at the limit).
- [ ] The claim-language emit gate, beside it: if `budget.is_some()` and `method` or `problem` contains `optimal` case-insensitively, refuse with the same sentence as the verify-side message.
- [ ] `inputs.budget = budget` in the `ScientificReceiptInputs` literal.
- [ ] Corpus member passthrough: `ScientificCorpusMember` gains `#[serde(default, skip_serializing_if = "Option::is_none")] pub budget_steps: Option<u64>,` and the same for `budget_consumed`; `cmd_receipt_corpus` passes each through as `--budget-steps` / `--budget-consumed` exactly as the mc fields are passed.

### Task 3: the heuristic kernel pair + calibration

- [ ] `examples/greedy_change_budget.bld`: greedy coin change with denominations {4, 3, 1} (greedy is a genuine heuristic here: for amount 6 it picks 4+1+1 where 3+3 is better, so it is demonstrably non-optimal while always terminating). For every amount `a` from 1 to 60 run the greedy loop (repeatedly subtract the largest denomination <= remaining), count the coins used as `steps`, and print `(step_budget - steps) as f64` where `step_budget` is an in-kernel constant you CALIBRATE: first write the kernel printing raw `steps`, run it (`buildc run`), take the measured worst over the 60 amounts, then set `step_budget` to a value with roughly 1.4x margin above the measured worst, and record BOTH numbers in the kernel's header comment (imitate how examples/mc_pi_rejection.bld's header records 0.2094 measured vs 0.3 band). Header comment also states the greedy-is-not-optimal fact (amount 6: greedy 3 coins, optimal 2) and that the receipt therefore carries NOT_PROVES_OPTIMALITY. Invariant: `non-negative`, PASS.
- [ ] `examples/greedy_change_budget_broken.bld`: the same kernel with `step_budget` set BELOW the measured worst (use the measured worst minus 2), so the slack goes negative for the worst amounts and the `non-negative` invariant FAILs as declared (FAIL_EXPECTED negative fixture).
- [ ] Emit both manually to verify verdicts before wiring the corpus, with a full budget declaration: `--budget-steps 60000 --budget-consumed <total coins actually used across all 60 amounts, measured>` for the positive (any coherent pair is fine; use the real measured total so the declaration is honest and record it in the corpus), and the same flags for the negative fixture. Confirm the emitted receipt carries the block, the label, and verifies MATCH.

### Task 4: end-to-end tests (compiler/tests/cli.rs)

- [ ] `budget_declaration_round_trips_and_pins_the_admission_contract`, modeled on `monte_carlo_declaration_round_trips_and_pins_the_admission_contract`: positive emit of `greedy_change_budget.bld` with the budget flags asserts receipt_status PASS, the sealed block fields, `labels` containing `NOT_PROVES_OPTIMALITY`, `not_claimed` containing `optimality`, and verify exit 0; negative-fixture emit of the broken kernel asserts FAIL_EXPECTED and verify exit 0; refusals: each partial flag combination (message contains `together`), `--budget-steps 0` (message contains `ceiling`), consumed > limit (message contains `incoherent`), and the claim-language refusal (emit the positive kernel with the budget flags plus `--problem greedy-optimal-change`, assert failure and stderr containing `optimality`).
- [ ] Corpus members: add to `examples/scientific-corpus.json` the pair with `budget_steps` / `budget_consumed` set to the calibrated honest values from Task 3; update the shipped-count pin 24 -> 26 ("all thirteen kernel pairs").
- [ ] Self-test count pin 7/7 -> 8/8 in `receipt_verify_self_test_proves_the_verifier_can_fail`.

### Task 5: docs + changelog, same commit

- [ ] docs/SCIENTIFIC-RECEIPT.md: flags list gains the two `--budget-*` bullets (voice of the `--mc-*` bullet); schema layers gain a `budget` bullet after the `monte_carlo` bullet (state the derived-exhausted rule, the label pairing, and the claim-language rule); the family section (section 3) gains the greedy pair after the MC pair (state the calibrated numbers and the amount-6 non-optimality witness); the `FIELD_CONTRACT_VIOLATION` row gains the budget shapes; the self-test section says four FIELD_CONTRACT_VIOLATION gates and eight cases; the corpus section says thirteen pairs and names the budget passthrough fields.
- [ ] CHANGELOG.md: a new bullet at the TOP of the existing `## Unreleased` section (above the Monte Carlo bullet), same register: what the block seals, the fail-closed contracts, the label/boundary pairing, the claim-language rule, corpus to 26, backward compatible.

### Task 6: verification gate (record all of it in your report)

- [ ] Full suite: `cargo test --manifest-path compiler/Cargo.toml > /tmp/suite.log 2>&1; echo exit=$?` then grep `test result` from the log. Must be exit 0, 0 failed.
- [ ] `compiler/target/debug/buildc.exe receipt corpus examples/scientific-corpus.json` reports 26/26.
- [ ] `receipt verify --self-test` on a budgeted receipt AND on a plain one: both 8/8.
- [ ] Mutation checks (use a python inline replace with an exact unique anchor string, restore with the inverse replace, NEVER `git checkout` on the working tree): (a) break the consumed<=limit verify gate, watch the unit test fail, restore; (b) break the CLI all-or-nothing gate, watch the cli refusal test fail, restore; (c) break the label-pairing verify rule, watch the unit test fail, restore. After each restore, re-run the covering test and confirm green.
- [ ] `cargo fmt --manifest-path compiler/Cargo.toml` then `cargo fmt --check` clean.
- [ ] ONE commit on `feat/budget-receipts` in the style of the slice 2 commit message (subject `feat: budgeted-search receipts (five-modes slice 3)`, body stating the admission rule, the contracts, the kernel pair with calibrated numbers, and the verification results with exact counts). Do NOT push.
