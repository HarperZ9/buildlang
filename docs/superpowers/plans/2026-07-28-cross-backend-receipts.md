# Cross-Backend Relation Receipts Implementation Plan (slice 5 of the five-modes brief)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The same kernel executed through two backends becomes one receipt: the C anchor's output and the Rust validation lane's output are the two columns of a relation checked under a fixed cross-backend tolerance, so backend agreement stops being a bespoke cross-check and becomes a sealed, re-derivable receipt family member.

**Architecture:** A new invariant family member (`cross-backend`, evaluator reused from `relation_columns_agree` at 2 columns) plus a sealed `cross_backend` block recording the secondary lane's facts (target, toolchain, executable digest, raw stdout digest, exit code). Emit runs BOTH backends and interleaves their parsed series as a 2-column row-major measurement; verify re-runs both and re-derives the verdict. The block and the invariant are a strict biconditional.

**Deviation from the five-modes brief, named:** the brief sketched the GPU lane as the second column. v0 uses the RUST backend instead: it is the repo's designated validation lane, it needs no device, and the receipt mechanism is backend-agnostic, so the GPU lane can slot in later as another `--cross-backend` value without reshaping anything. This deviation keeps the slice device-free and CI-runnable.

**Measured feasibility facts this plan is built on (2026-07-28 probe):** `buildc build <projdir> --target rust` emits `target/debug/main.rs` for a scalar f64 while-loop kernel; `rustc -O -o <exe> main.rs` compiles it; both backends compute IDENTICAL doubles for the probe recurrence `x = x*0.9 + 0.01`; but the C runtime prints `%g` (6 significant digits) while Rust prints shortest-roundtrip, so printed values differ by up to about 5e-7 absolute on O(1) values (`0.829` vs `0.8290000000000001`). Therefore the cross-backend tolerance CANNOT be the relation family's 1e-9.

## Global constraints

- ONE commit on branch `feat/cross-backend-receipts` (create it from feat/model-capability's HEAD once slice 4 is complete; the controller will have done this before dispatch). Do not push. Do not touch main.
- Backward compatible: receipts without the block keep their exact bytes and seals.
- Every new gate mutation-tested with red/green evidence recorded. Never `git checkout` a working-tree file to restore; use inverse edits.
- No em-dash characters anywhere. Exit codes captured before any pipe. `cargo fmt --check` clean before the commit.
- The three sibling slices in `git log` (Random, monte_carlo, budget) are the structural references; the `relation` invariant's existing code paths are the evaluator reference.
- v0 composition rule, fail closed: `--cross-backend` refuses a `Random`-observing kernel (the Rust lane has no seeded PRNG builtin, so the streams could not agree), refuses `--seed`, and refuses `--mc-*` (which requires Random anyway). `--budget-*` is orthogonal and permitted.

### Task 1: the invariant member (compiler/src/scientific_runtime.rs)

- [ ] Constants beside the relation ones:

```rust
/// The invariant name emitted for the CROSS-BACKEND agreement check: each row
/// holds the same step's value computed by two backends (column 0 the C
/// anchor, column 1 the secondary lane), and every row must agree within the
/// cross-backend tolerance. The evaluator is the relation family's; only the
/// tolerance and the sealed provenance differ.
pub const CROSS_BACKEND_INVARIANT: &str = "cross_backend_columns_agree";

/// Tolerance for the cross-backend check. Looser than RELATION_TOLERANCE for
/// a measured reason: the C runtime prints %g (6 significant digits) while
/// the Rust lane prints shortest-roundtrip, so two IDENTICAL doubles can
/// print up to ~5e-7 apart on O(1) values (measured 2026-07-28 on the decay
/// recurrence). 1e-5 clears that display-rounding floor by ~20x, while a
/// genuine cross-backend divergence (a dropped term, a different formula, a
/// miscompiled kernel) is O(1) and caught decisively. Like every family
/// tolerance it is absolute: cross-backend kernels must emit O(1) values.
pub const CROSS_BACKEND_TOLERANCE: f64 = 1e-5;
```

- [ ] Register it exactly as `relation` is registered: `KNOWN_INVARIANTS` (append at the end), `invariant_tolerance` arm, `invariant_expectation` arm ("every row's two backend columns agree within the cross-backend tolerance"), `column_count_matches_invariant` (requires EXACTLY 2), and `evaluate_measurement` (route through the same `relation_columns_agree` arm the relation invariant uses; do NOT add it to `evaluate_invariant`, same reason as relation).
- [ ] The sealed block after `ScientificBudget`:

```rust
/// The cross-backend admission block: the secondary lane's witnessed facts.
/// Present IFF the invariant is the cross-backend member (a strict
/// biconditional verify re-checks). The secondary raw-stdout digest and
/// executable digest are sealed at emit and REPORTED at verify (exact bytes
/// are toolchain-dependent by design, exactly like the primary's); the
/// re-checked quantity is the verdict over the re-derived columns.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScientificCrossBackend {
    /// The secondary lane. v0: `rust` only (the validation lane); the GPU
    /// lane is a later value, not a different mechanism.
    pub secondary_target: String,
    /// First line of `rustc --version` at emit (human triage).
    pub secondary_toolchain_version: String,
    /// sha256 over the full version-probe output bytes.
    pub secondary_toolchain_digest: ScientificDigest,
    /// sha256 of the compiled secondary executable BEFORE it ran.
    pub secondary_executable_digest: ScientificDigest,
    /// sha256 over the secondary's exact raw stdout bytes at emit.
    pub secondary_raw_stdout_digest: ScientificDigest,
    /// The secondary's process exit code (sealed; re-checked at verify).
    pub secondary_exit_code: i32,
    /// `EXECUTED` (v0): unlike the declared blocks, this one witnesses a run
    /// that actually happened; verify re-executes it.
    pub status: String,
}
```

- [ ] `ScientificRuntimeReceipt.cross_backend: Option<ScientificCrossBackend>` after `budget`, same serde attributes; `ScientificReceiptInputs` field; builder threads it.
- [ ] Verify contracts after the budget contract block, all `FIELD_CONTRACT_VIOLATION` unless stated: the biconditional (`cross_backend.is_some()` iff `receipt.invariant.name == CROSS_BACKEND_INVARIANT`); `secondary_target` must be `rust`; `status` must be `EXECUTED`; the three digests must be well-formed sha256 (reuse `digest_is_well_formed`, class `DIGEST_MALFORMED` to match how primary digests are treated).
- [ ] Verify re-run threading: the rerun closure signature gains the cross-backend request: change `FnOnce(&Path, &[String], Option<u64>)` to `FnOnce(&Path, &[String], Option<u64>, Option<&str>)` (the fourth argument is the secondary target from the sealed block, None when absent), and `RerunObservation` gains `pub secondary: Option<SecondaryObservation>` with

```rust
pub struct SecondaryObservation {
    pub parsed: ParsedSeries,
    pub exit_code: i32,
    pub raw_stdout_digest: ScientificDigest,
    pub executable_digest: ScientificDigest,
}
```

  In the evaluator, when the receipt carries the block: require `observation.secondary` (its absence after a requested secondary re-run is `RERUN_FAILED`); check `secondary.exit_code` against the sealed one (`RERUN_EXIT_MISMATCH`, message naming the secondary lane); rebuild the interleaved series from the two re-parsed series (refuse on length mismatch with `MEASUREMENT_COUNT_DRIFT`, message naming both counts); use the rebuilt interleaved series for the verdict recomputation in place of the primary-only series; and REPORT (never require) `secondary_raw_stdout_reproduced` and `secondary_executable_reproduced` alongside the existing reproduction flags (extend `ScientificVerifyReport` and both printers with the two flags, defaulting them to true-absent semantics only where the block is absent: print them only for cross-backend receipts in the human line, and always include them in the JSON when the block is present).
- [ ] Self-test tamper case 9: swap the sealed invariant/block pairing: if the receipt has a `cross_backend` block, remove it (keeping the invariant name); if it has none, add a syntactically valid block (target `rust`, `EXECUTED`, three 64-hex digests of `a`, exit 0) while leaving the invariant name unchanged; reseal; expected `FIELD_CONTRACT_VIOLATION` (the biconditional). Update the pinned class-list test (now nine entries) and the self-test count pins (8/8 to 9/9) everywhere they appear in cli.rs and docs.
- [ ] `base_inputs` fixture gains `cross_backend: None,`.
- [ ] Unit tests, modeled on the sibling block tests: (a) a receipt built with the block, invariant `cross_backend_columns_agree`, column_count 2, and an interleaved AGREEING series (e.g. `[1.0, 1.0000005, 0.5, 0.5000001]`) verifies Ok when the rerun closure returns a matching secondary observation, and the closure ASSERTS it received `Some("rust")` as the fourth argument; (b) the same receipt with a DIVERGENT interleaved series (`[1.0, 1.1, 0.5, 0.5]`) has `receipt_status` FAIL_UNEXPECTED at build time (this is the family's can-it-fail evidence at the evaluator level: a genuine divergence FAILs) and its negative-fixture variant is FAIL_EXPECTED; (c) block without the invariant refused, invariant without the block refused, wrong target refused, wrong status refused; (d) a receipt with no block still verifies through a closure passed `None` as the fourth argument (assert it).

### Task 2: emit (compiler/src/main.rs)

- [ ] CLI flag on Run: `#[arg(long, value_name = "TARGET")] cross_backend: Option<String>,` documented: runs the kernel through the secondary lane as well and seals a 2-column cross-backend receipt; v0 supports `rust`; requires `--invariant cross-backend` (and vice versa); refused with `--gpu`, with `--seed`, with `--mc-*`, and on a `Random`-observing kernel.
- [ ] Map the CLI invariant name: add `"cross-backend" => CROSS_BACKEND_INVARIANT` to the invariant-name match and to the unknown-invariant error text's supported list.
- [ ] Emit gates in `cmd_run` (receipt path), beside the existing pairing gates: `--cross-backend` given with any value other than `rust` refused; `--cross-backend` iff `--invariant cross-backend` (both directions refused with messages naming the pairing); `--cross-backend` with `seed.is_some()` or any mc flag refused; after the effect policy is derived, `--cross-backend` on a `Random`-observing kernel refused (the Rust lane has no seeded PRNG; the streams could not agree). Force `columns` to 2 for this invariant: require the user's `--columns` to be either unset (default 1, which you override to 2 with a comment saying the invariant defines its column structure) or exactly 2; any other value refused by the existing column-count gate.
- [ ] The secondary run inside the receipt path, after the primary capture: probe `rustc` (`rustc --version`, capture full output bytes for the digest; missing rustc refuses with an install hint, exit 1, BEFORE the primary run happens so no work is wasted: place the probe beside the C toolchain probe); emit the Rust source for the SAME resolved source file through the same internal path `cmd_build` uses for `--target rust` (read `cmd_build`'s dispatch to find the codegen entry; if that path requires a project directory, create a temp dir, copy the source in as `main.bld`, and reuse the path; a lowering failure for a kernel outside the Rust subset refuses with a message naming the subset limitation); compile with `rustc -O -o <temp>/main_rust.exe <emitted main.rs>` (compile failure refuses, stderr forwarded); hash the executable before running; run it with the same trailing args and the same BUILD_RANDOM_SEED scrubbing the primary applies (no seed is ever set here since Random is refused); capture stdout bytes, digest them, parse the series with `parse_numeric_series`.
- [ ] Interleave: refuse (exit 1, message naming both counts) if the two parsed series lengths differ or either is empty or either diverged; otherwise build the row-major interleaved series `[c0, r0, c1, r1, ...]`, set `column_count = 2`, `series` = interleaved, `raw_stdout_digest` = primary's (unchanged), and build the `ScientificCrossBackend` block from the probed toolchain facts, executable digest, stdout digest, and exit code. Clean up the temp dir.
- [ ] Thread the block into `ScientificReceiptInputs`. The `--emit-receipt` echo behavior stays primary-only (the secondary's stdout is not echoed; note it in the docs).
- [ ] Verify dispatch closures (both call sites) updated for the new fourth argument: when `Some("rust")`, perform the same secondary pipeline (probe rustc: absent at verify is `TOOL_UNAVAILABLE` semantics, so return Err(4) from the closure to match how the primary toolchain absence is classed; lowering or compile failure returns Err with the `RERUN_FAILED` mapping the evaluator applies) and fill `RerunObservation.secondary`; when `None`, leave it `None`.
- [ ] Corpus member field `#[serde(default, skip_serializing_if = "Option::is_none")] pub cross_backend: Option<String>,` passed through as `--cross-backend <value>`.

### Task 3: the kernel and corpus entry

- [ ] `examples/decay_cross_backend.bld`: the probed recurrence, formalized:

```
// Cross-backend kernel: the same decay recurrence through two backends.
//
// x starts at 1.0 and steps through x = x * 0.9 + 0.01 for 40 steps,
// printing x each step. Emitted with `--cross-backend rust --invariant
// cross-backend`, buildc runs this through the C anchor AND the Rust
// validation lane and seals each step's two values as one row of a
// 2-column relation checked at the cross-backend tolerance (1e-5,
// calibrated to the display-rounding floor between C's %g and Rust's
// shortest-roundtrip formatting; the computed doubles are identical for
// this kernel, measured 2026-07-28). The receipt witnesses backend
// AGREEMENT on this kernel, not correctness of either backend.

fn main() ~ Console {
    let mut x: f64 = 1.0;
    let mut k: i32 = 0;
    while k < 40 {
        x = x * 0.9 + 0.01;
        println!("{}", x);
        k = k + 1;
    }
}
```

- [ ] Emit + verify it manually end to end before wiring the corpus; record the verify MATCH line in your report.
- [ ] Corpus: ONE member (`{"source": "examples/decay_cross_backend.bld", "invariant": "cross-backend", "cross_backend": "rust", "expected_status": "PASS"}`), corpus 26 -> 27. There is deliberately NO corpus FAIL_EXPECTED partner and the docs must say why: an honest deterministic kernel that computes DIFFERENT values on the two backends cannot exist by construction (that impossibility is what the invariant witnesses), so the can-it-fail evidence lives in the evaluator-level divergence unit test (Task 1b), the refusal gates, and self-test case 9. Update the shipped-count pin 26 -> 27 and its message ("thirteen pairs plus the cross-backend singleton").

### Task 4: end-to-end tests (compiler/tests/cli.rs)

- [ ] `cross_backend_receipt_round_trips_and_pins_the_pairing`: emit the decay kernel with `--cross-backend rust --invariant cross-backend`, assert receipt_status PASS, `column_count == 2`, `measurement.count == 80`, the block's target/status/digest shapes, and verify exit 0 (this test needs rustc; skip with an eprintln if `rustc --version` fails, mirroring how `c_backend_ready` skips); refusals: `--cross-backend rust` without `--invariant cross-backend` and vice versa; `--cross-backend rust --seed 7` on the decay kernel; `--cross-backend rust` on `examples/random_walk_bound.bld` (Random-observing) with `--invariant cross-backend`; `--cross-backend vulkan` (unsupported target named in message); missing rustc simulated by running buildc with PATH set to just the directory containing the C compiler or by `--cross-backend rust` under an env where rustc is unreachable IF that is feasible in the harness (if not feasible cleanly, drop this one assertion and note it in your report rather than shipping a flaky test).
- [ ] Self-test pin 8/8 -> 9/9; corpus pin as in Task 3.

### Task 5: docs + changelog, same commit

- [ ] docs/SCIENTIFIC-RECEIPT.md: flags bullet for `--cross-backend`; the invariant list in section 1 gains `cross-backend`; a schema bullet for the block after `budget` (state: EXECUTED not DECLARED, verify re-executes both lanes, reproduction of secondary bytes reported never required, rustc absence at verify is TOOL_UNAVAILABLE); section 3 gains the cross-backend member with the tolerance calibration story (the ~5e-7 display floor, the 1e-5 choice, the O(1) scaling doctrine) and the no-negative-pair rationale; the corpus section notes the singleton; failure-class table rows touched: TOOL_UNAVAILABLE mentions rustc for cross-backend receipts.
- [ ] docs/EFFECTS_GUIDE.md: unchanged (no new capability). STATUS.md: ONE sentence added to the Rust Backend paragraph: the receipt layer now consumes the lane via `--cross-backend rust` (cross-backend relation receipts).
- [ ] CHANGELOG.md: new bullet at the top of Unreleased, sibling register.

### Task 6: verification gate

- [ ] Full suite exit 0 captured before any pipe; corpus 27/27; self-test 9/9 on a cross-backend receipt AND a plain one; fmt clean.
- [ ] Mutation checks with red/green: (a) break the biconditional pairing gate; (b) break the length-mismatch interleave refusal (make it truncate instead), covering test = whichever unit/cli test pins count 80 or the refusal message (if none does, ADD the covering assertion first, then mutate); (c) loosen CROSS_BACKEND_TOLERANCE to 1.0 and watch the divergence unit test (Task 1b) fail.
- [ ] ONE commit, subject `feat: cross-backend relation receipts through the Rust validation lane (five-modes slice 5)`, body in the sibling register with the calibration story and verification counts, ending with: Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>. Do NOT push.
