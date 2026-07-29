# Monte Carlo Estimator Receipts Implementation Plan (slice 2 of the five-modes brief)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Monte Carlo run's receipt carries the estimator's admission facts (sample count n, estimator id, interval by a declared method) as a sealed `monte_carlo` block, fail closed, so an MC number can never enter the evidence layer without its denominator and its interval discipline.

**Architecture:** The block is author-DECLARED metadata (buildc cannot derive n or the interval method from source and does not pretend to), following the `numerical_method` idiom, with two hard contracts verify re-checks: completeness (estimator, samples, and interval method declare together or not at all; a partial declaration is refused at emit) and pairing (an MC block rides only on a program that observes `Random` with a sealed seed). v0 claims REPRODUCIBILITY and declaration discipline, not correctness of the interval; for the known-answer kernel the slack idiom additionally checks the running estimate against truth for the sealed seed.

**Tech stack:** Rust (buildc compiler), the shipped scientific-receipt slice pattern (invariant family, corpus, self-test, cli tests).

**Deviation from the brief, named:** the brief sketched the `bounded` invariant for the truth check; `bounded` fences a series against its own initial value, which is wrong for an MC error trajectory (early error is the largest). The truth check uses the family's established SLACK idiom under `non-negative` instead: the kernel prints `band - |estimate_k - pi|` after a burn-in, with the band calibrated against the sealed seed the way the funnel kernel's probe bound was calibrated.

## Global constraints

- One reviewed commit; positive and negative fixtures; every guard mutation-tested (break it, watch the covering test fail, restore).
- Backward compatible: receipts sealed before this slice must parse and re-serialize to their original bytes (`monte_carlo` is `Option` + `skip_serializing_if`).
- No em-dashes in any file; public-register docs carry no local paths.
- Branch `feat/mc-estimator-receipts` stacked on `feat/random-capability`; push branch only, never main.

### Task 1: the sealed block and its contracts (scientific_runtime.rs)

- [ ] `ScientificMonteCarlo { estimator: String, samples: u64, interval_method: String, status: String }` (status always `DECLARED` in v0; verify rejects anything else).
- [ ] `ScientificRuntimeReceipt.monte_carlo: Option<ScientificMonteCarlo>` placed after `numerical_method` (its sibling author-declared block), `#[serde(default, skip_serializing_if = "Option::is_none")]`.
- [ ] `ScientificReceiptInputs.monte_carlo` threaded through `build_scientific_runtime_receipt`.
- [ ] Verify contracts, both `FIELD_CONTRACT_VIOLATION`: (a) block present requires `Random` in the RE-DERIVED capabilities AND a sealed `seed_value` (checked beside the existing seed pairing gate in step 2a); (b) `samples == 0`, an empty `estimator`, an empty `interval_method`, or a status other than `DECLARED` is refused (a zero-denominator or nameless MC claim is unpriceable).
- [ ] Self-test tamper case 7: if the receipt has an mc block, set `samples` to 0; if it has none, add a block with `samples: 0`; reseal; expected class `FIELD_CONTRACT_VIOLATION` either way. Update the pinned class list test.
- [ ] Unit tests: builder threads the block; verify accepts a well-formed mc receipt (rederive closure returns Random policy); rejects zero samples, empty method, missing Random, status not DECLARED; old-receipt bytes unaffected (serialize a no-mc receipt, assert no `monte_carlo` key).

### Task 2: CLI admission (main.rs)

- [ ] Run flags: `--mc-estimator <ID>`, `--mc-samples <N>` (u64), `--mc-interval <METHOD>`; documented as ignored without `--emit-receipt` like the other receipt flags.
- [ ] Emit gates, before compiling: any mc flag without the other two is refused with the brief's sentence (an estimator whose interval method is undeclared is refused; same for every partial combination); `--mc-samples 0` refused; mc flags on a program with no `Random` capability refused (rides the existing capability gate; the seed requirement then follows from slice 1's pairing).
- [ ] Corpus member fields `mc_estimator` / `mc_samples` / `mc_interval` (all optional; runner passes them through; the same all-or-nothing contract applies and a partial corpus declaration fails the corpus loudly at emit).

### Task 3: the known-answer kernel pair + calibration

- [ ] `examples/mc_pi_rejection.bld`: pi by rejection sampling, n = 2000 draws of (x, y), running estimate `4 * inside / k`, printing `band - |estimate - pi|` for k >= burn under `~ Console + Random`. Calibrate burn and band against seed 42 empirically (run, take the measured worst error after burn, set the band with clear margin, record both numbers in the kernel's header comment the way funnel_probe records 14-vs-20).
- [ ] `examples/mc_pi_rejection_broken.bld`: the same kernel with the estimator factor 3.0 instead of 4.0 (a wrong-area estimator converges to 3*pi/4, error ~0.785, decisively outside any honest band), declared `--negative-fixture`, FAIL_EXPECTED.
- [ ] Corpus +2 members with seed 42 and full mc declarations (corpus 22 -> 24).

### Task 4: end-to-end tests (tests/cli.rs)

- [ ] Round-trip test: emit the pi kernel with full mc flags + seed; assert receipt_status PASS, `monte_carlo` block sealed with declared values, verify exits 0; emit the broken kernel as negative fixture; assert FAIL_EXPECTED and verify 0.
- [ ] Refusal tests: mc flags partial (each missing-one combination) refused with the interval-discipline message; `--mc-samples 0` refused; mc flags on `search_bound_binary.bld` (no Random) refused.
- [ ] Corpus count pin 22 -> 24; self-test count pin 6/6 -> 7/7.

### Task 5: docs + changelog, same commit

- [ ] SCIENTIFIC-RECEIPT.md: flags list; schema layer bullet for `monte_carlo` (the admission rule sentence: the weaker the mode's promise, the more the receipt must carry; v0 claims reproducibility and declaration discipline, not interval correctness); family section gains the MC pair; self-test text 7 cases; corpus text twelve pairs.
- [ ] CHANGELOG Unreleased entry.

### Task 6: verification gate

- [ ] Full suite green with exit code captured before any pipe; `cargo fmt --check` clean; corpus 24/24; self-test 7/7 on an mc receipt and a plain one; mutation checks: break the pairing gate, the completeness gate, and the zero-samples gate one at a time and watch the covering test fail; same-seed emit reproduces the raw stdout digest.
- [ ] Commit (one commit) and push `feat/mc-estimator-receipts`.
