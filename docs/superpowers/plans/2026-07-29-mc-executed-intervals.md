# w1: Monte Carlo EXECUTED interval discipline (implementation plan, DRAFT)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans, task-by-task.

**Scope verdict (read first).** This is ONE commit, comparable in shape to the prior
scientific-runtime slices (slice 2, `docs/superpowers/plans/2026-07-28-mc-estimator-receipts.md`,
commit `9430439`, which added the DECLARED `monte_carlo` block this slice extends): one
schema extension (five `Option` fields plus a two-arm status), one shared pure recompute
function called from three sites (emit, verify stage A, verify stage B), one new CLI flag,
one new kernel pair, one new corpus pair, one new self-test case, and the matching docs.
No prerequisite is missing: the DECLARED `monte_carlo` block, the multi-column capture
machinery (`column_count`, `relation_columns_agree`, `evaluate_measurement`), and the
EXECUTED-status precedent (`cross_backend`) all already ship.

**Goal.** Upgrade `monte_carlo` from a DECLARED-only admission block to a two-arm
`DECLARED | EXECUTED` block where an EXECUTED receipt seals a Wilson or normal-approx-95
interval the verifier RE-DERIVES from raw sufficient-statistic columns the kernel prints
(cumulative `successes`/`trials` counters beside the existing invariant scalar), never
from the kernel's own arithmetic. Mechanism: the kernel prints exactly three columns per
post-burn-in row, `<invariant_scalar> <successes> <trials>`; a new shared function
`compute_mc_executed` de-interleaves them, checks structural coherence as a cumulative
Bernoulli count, and recomputes the interval, entirely in verifier-owned code. Emit calls
it once (fail closed: an incoherent EXECUTED block is never sealed). Verify calls it
TWICE: Stage A over the sealed `measurement.observed_values` (no re-run, so a
tampered-and-resealed interval is a pure data contradiction, rejectable before any program
re-run, keeping the self-test's no-compiler property intact), and Stage B over the
re-parsed re-run series (a new failure class, `MC_INTERVAL_DRIFT`, since a Stage-A-clean
receipt that no longer describes the run it names is a different kind of dishonesty than a
tampered field). What EXECUTED can never claim — sample independence, interval coverage,
estimator semantics — is sealed as three new `not_claimed` entries, present iff the block
is EXECUTED, mirroring the `NOT_PROVES_OPTIMALITY`/`optimality` pairing idiom the budget
block already uses.

**Source of truth.** `.superpowers/sdd/w1-mc-executed-design-DRAFT.md`. This plan
implements it exactly; every deviation below is either a design line-anchor correction
(verified by reading the current file) or a decision the design explicitly left open,
marked inline with one sentence of rationale. No other deviation is authorized.

**Controller-pinned counts (override the design doc's numbers).** Corpus 27 -> 29
(thirteen pairs plus the cross-backend singleton, plus this slice's new pair). Self-test
9 -> 10. Every count in this plan uses these.

## Global constraints

- ONE commit on branch `feat/mc-executed-intervals`. The stack base (which commit or
  branch it is created from) is decided by the controller at dispatch time, not by this
  plan: do not assume the HEAD current when this plan was drafted (`0fc3406`, branch
  `feat/units-in-types`, being actively edited by a concurrent agent on
  `compiler/src/types/infer.rs`) is the base merely because it was HEAD at draft time. Do
  not push.
- Backward compatibility is absolute: every scientific-runtime receipt sealed before this
  slice parses and re-serializes to its EXACT bytes and seal. The five new
  `ScientificMonteCarlo` fields are `Option<T>` with
  `#[serde(default, skip_serializing_if = "Option::is_none")]`, the `ScientificBudget`
  precedent (`compiler/src/scientific_runtime.rs:306-309`). `ScientificMonteCarlo` loses
  `Eq` (gains `f64` fields), same precedent. Schema stays `v0`.
- `--mc-executed` is opt-in; the DECLARED emission path (all 27 existing corpus members)
  is byte-for-byte untouched.
- Every new gate is mutation-tested: break it, observe the specific test go red, restore
  it, observe green. Task 9 enumerates every gate this slice adds.
- No em-dashes in any prose or code comment this commit adds (repo-wide voice rule).
- Exit codes are captured before any pipe in every verification command run for this
  slice (the pipes-swallow-exit-codes trap); never `| tail` or `| grep` a gate command
  without capturing `$?`/`%ERRORLEVEL%` first.
- `cargo fmt --check` clean.
- `buildc corpus verify examples/scientific-corpus.json` reports `29/29`.
- `buildc receipt verify <fixture>.json --self-test` reports `10/10`.
- Full `cargo test` from `compiler/` reports 0 failed (baseline before this slice: confirm
  the count by running it once at the start of Task 11; do not assume a stale number).

## Task 1: schema extension (`compiler/src/scientific_runtime.rs`)

- [ ] Extend `ScientificMonteCarlo` (currently lines 280-293: `estimator`, `samples`,
  `interval_method`, `status`) with five `Option` fields, dropping `Eq` from the derive:

  ```rust
  #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
  pub struct ScientificMonteCarlo {
      pub estimator: String,
      pub samples: u64,
      pub interval_method: String,
      /// `DECLARED` | `EXECUTED`.
      pub status: String,
      /// p_hat = successes_final / trials_final. Present IFF `status == "EXECUTED"`.
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub estimate: Option<f64>,
      /// Lower bound by the named method. Present IFF `status == "EXECUTED"`.
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub interval_low: Option<f64>,
      /// Upper bound by the named method. Present IFF `status == "EXECUTED"`.
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub interval_high: Option<f64>,
      /// trials_final; MUST equal `samples`, the witnessed denominator.
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub n_effective: Option<u64>,
      /// successes_final.
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub successes: Option<u64>,
  }
  ```

- [ ] Add the pinned vocabulary and tolerance constants, beside the other family
  constants near the top of the file (after `CROSS_BACKEND_TOLERANCE`, line ~141):

  ```rust
  /// Executed Monte Carlo estimator vocabulary, v1: the mean of Bernoulli
  /// indicators. DECLARED blocks keep free text forever (the shipped corpus
  /// uses `mean`); this vocabulary gates EXECUTED blocks only.
  pub const MC_EXECUTED_ESTIMATOR_PROPORTION: &str = "proportion";

  /// Executed interval method: normal approximation. Degenerate at a boundary
  /// proportion (successes == 0 or successes == trials): refused at emit and
  /// at verify, pointing at `wilson-95`.
  pub const MC_INTERVAL_NORMAL_APPROX_95: &str = "normal-approx-95";

  /// Executed interval method: Wilson score. Well-defined at the boundary
  /// proportions, asymmetric by construction.
  pub const MC_INTERVAL_WILSON_95: &str = "wilson-95";

  /// NOT executable in v1 (needs an inverse incomplete beta with no in-tree
  /// oracle). Named here only so refusal messages can point at it by
  /// constant rather than a bare string; never accepted by
  /// `compute_mc_executed`.
  pub const MC_INTERVAL_CLOPPER_PEARSON_95: &str = "clopper-pearson-95";

  /// z for a two-sided 95% normal/Wilson interval: the double nearest the
  /// 0.975 standard-normal quantile. Shared by both executable methods.
  pub const MC_INTERVAL_Z_95: f64 = 1.959963984540054;

  /// Absolute float tolerance for the emit/verify interval recompute
  /// (`estimate`, `interval_low`, `interval_high`, both stages). The
  /// arithmetic runs on identical integer inputs through one fixed Rust
  /// implementation at emit and verify, so agreement should be exact in
  /// practice; this is headroom against a future compiler reassociating
  /// verifier-side float ops, not load-bearing looseness. Values are O(1)
  /// proportions, so absolute is safe.
  pub const MC_RECOMPUTE_TOLERANCE: f64 = 1e-12;

  /// The three `not_claimed` entries an EXECUTED monte_carlo block adds,
  /// present iff `status == "EXECUTED"` (the `NOT_PROVES_OPTIMALITY`/
  /// `optimality` pairing idiom). EXECUTED hardens the interval arithmetic
  /// and the denominator; it cannot and does not claim these.
  pub const MC_EXECUTED_NOT_CLAIMED: &[&str] =
      &["sample_independence", "interval_coverage", "estimator_semantics"];
  ```

- [ ] Fix the existing test helper `mc()` inside
  `verify_enforces_the_monte_carlo_admission_contracts` (currently lines 5715-5722): add
  the five new fields as `None` so the struct literal compiles.
- [ ] Update the same test's `bad.status = "EXECUTED".to_string()` case (currently lines
  5780-5782, comment "A status v0 cannot honestly say is refused"): EXECUTED is now
  expressible, so this specific literal (via `mc()`, which sets none of the five executed
  fields) must still fail, but for a DIFFERENT reason: EXECUTED with no executed fields
  present is `FIELD_CONTRACT_VIOLATION` (Task 4's field-presence gate), not an unknown
  status. Reword the comment; the assertion (`Err(1)`) is unchanged. Add a sibling case
  in the same test for `status` set to a third string (e.g. `"SIMULATED"`), which must
  still be refused as an unknown status, so the two-arm vocabulary itself stays gated.

## Task 2: `compute_mc_executed` (shared pure recompute, `scientific_runtime.rs`)

This is the single function emit and both verify stages call, so all three can never
disagree on the math. Insert after `evaluate_measurement` (current lines 1041-1065).

- [ ] Result type:

  ```rust
  /// The recomputed EXECUTED monte_carlo fields, owned by the caller to
  /// compare against the sealed ones (emit: seal them; verify: compare).
  #[derive(Clone, Debug, PartialEq)]
  pub struct McExecutedComputed {
      pub estimate: f64,
      pub interval_low: f64,
      pub interval_high: f64,
      pub n_effective: u64,
      pub successes: u64,
  }
  ```

- [ ] The function:

  ```rust
  /// Recompute the EXECUTED monte_carlo fields from a captured three-column
  /// series (`<invariant_scalar> <successes> <trials>` per row), the
  /// declared denominator, and the named interval method. PURE and
  /// unit-tested; called from emit (fail closed before sealing), verify
  /// Stage A (over the sealed series, before any re-run), and verify Stage B
  /// (over the re-run series). Never trusts anything but the raw columns.
  ///
  /// Coherence checks, in order (Decision 1, design doc): every successes/
  /// trials value is integer-valued (`fract() == 0`) and below 2^53; trials
  /// increments by exactly 1 across consecutive rows (the first row's
  /// absolute value is free, the burn-in edge); successes is non-decreasing
  /// with increments in {0, 1}; successes <= trials on every row; the final
  /// row's trials equals `samples` (the witnessed-denominator equality).
  /// `interval_method` must be one of the two executable methods
  /// (`MC_INTERVAL_NORMAL_APPROX_95`, `MC_INTERVAL_WILSON_95`); any other
  /// name (including `clopper-pearson-95`) is refused here, the single
  /// source of truth for the executable vocabulary. `normal-approx-95` is
  /// additionally refused at a boundary proportion (successes_final == 0 or
  /// == trials_final): a zero-width interval there overclaims precision;
  /// the message points at `wilson-95`.
  pub fn compute_mc_executed(
      series: &[f64],
      column_count: usize,
      samples: u64,
      interval_method: &str,
  ) -> Result<McExecutedComputed, String> {
      const MAX_EXACT_INTEGER: f64 = 9007199254740992.0; // 2^53

      if column_count != 3 {
          return Err(format!(
              "column_count {column_count} is not 3: an EXECUTED monte_carlo receipt requires exactly three columns per row"
          ));
      }
      if series.is_empty() || series.len() % 3 != 0 {
          return Err(format!(
              "series length {} is not a positive multiple of 3: ragged rows cannot witness a Bernoulli count",
              series.len()
          ));
      }
      let rows = series.len() / 3;

      let mut prev_trials: Option<f64> = None;
      let mut prev_successes: Option<f64> = None;
      for k in 0..rows {
          let successes_k = series[k * 3 + 1];
          let trials_k = series[k * 3 + 2];
          for (label, value) in [("successes", successes_k), ("trials", trials_k)] {
              if value.fract() != 0.0 || !value.is_finite() || value.abs() >= MAX_EXACT_INTEGER {
                  return Err(format!(
                      "row {k}: {label} = {value} is not an exact non-negative integer below 2^53"
                  ));
              }
          }
          if let Some(prev) = prev_trials {
              if trials_k != prev + 1.0 {
                  return Err(format!(
                      "row {k}: trials {trials_k} does not follow row {}'s trials {prev} by exactly 1",
                      k - 1
                  ));
              }
          }
          if let Some(prev) = prev_successes {
              let delta = successes_k - prev;
              if delta != 0.0 && delta != 1.0 {
                  return Err(format!(
                      "row {k}: successes {successes_k} does not follow row {}'s successes {prev} by 0 or 1",
                      k - 1
                  ));
              }
          }
          if successes_k > trials_k {
              return Err(format!(
                  "row {k}: successes {successes_k} exceeds trials {trials_k}"
              ));
          }
          prev_trials = Some(trials_k);
          prev_successes = Some(successes_k);
      }

      let trials_final = prev_trials.expect("rows > 0, checked above") as u64;
      let successes_final = prev_successes.expect("rows > 0, checked above") as u64;
      if trials_final != samples {
          return Err(format!(
              "witnessed final trials {trials_final} does not equal the declared samples {samples}: the denominator is not witnessed"
          ));
      }

      let n = trials_final as f64;
      let p_hat = successes_final as f64 / n;
      let z = MC_INTERVAL_Z_95;

      let (interval_low, interval_high) = match interval_method {
          MC_INTERVAL_NORMAL_APPROX_95 => {
              if successes_final == 0 || successes_final == trials_final {
                  return Err(format!(
                      "normal-approx-95 is degenerate at the boundary proportion (successes = {successes_final} of {trials_final}); use wilson-95"
                  ));
              }
              let half_width = z * (p_hat * (1.0 - p_hat) / n).sqrt();
              (p_hat - half_width, p_hat + half_width)
          }
          MC_INTERVAL_WILSON_95 => {
              let z2 = z * z;
              let denom = 1.0 + z2 / n;
              let center = (p_hat + z2 / (2.0 * n)) / denom;
              let margin =
                  (z / denom) * (p_hat * (1.0 - p_hat) / n + z2 / (4.0 * n * n)).sqrt();
              (center - margin, center + margin)
          }
          other => {
              return Err(format!(
                  "interval_method `{other}` is not in the EXECUTED executable vocabulary (v1: normal-approx-95, wilson-95); clopper-pearson-95 needs a verified inverse incomplete beta and is not executable"
              ));
          }
      };

      Ok(McExecutedComputed {
          estimate: p_hat,
          interval_low,
          interval_high,
          n_effective: trials_final,
          successes: successes_final,
      })
  }
  ```

- [ ] Unit tests, `#[cfg(test)] mod tests` in the same file:
  - `compute_mc_executed_recovers_a_coherent_wilson_series`: a hand-built 4-row series
    (e.g. successes `1,1,2,2`, trials `10,11,12,13`), `samples = 13`, `wilson-95`; assert
    `Ok` with `n_effective == 13`, `successes == 2`, and `estimate == 2.0/13.0`.
  - `compute_mc_executed_rejects_before_any_rerun_on_bad_trials_step`: trials jumping by 2
    between two rows; assert `Err` whose message names the offending row and field
    (**this is the "Stage-A-rejects-before-rerun" property test**: it proves the
    coherence check fires on pure data, with no re-run machinery involved at all, since
    the function takes no compile/run inputs).
  - `compute_mc_executed_rejects_successes_decrease`.
  - `compute_mc_executed_rejects_successes_exceeding_trials`.
  - `compute_mc_executed_rejects_non_integer_column`.
  - `compute_mc_executed_witnessed_denominator_must_equal_samples`: coherent series whose
    final trials is 2000 but `samples = 1999`; assert `Err` naming both numbers (**the
    witnessed-denominator equality test** named in the task).
  - `compute_mc_executed_rejects_ragged_series`: `series.len()` not a multiple of 3.
  - `compute_mc_executed_normal_approx_degenerate_at_zero_successes` and
    `..._at_all_successes`: both boundary refusals, message points at `wilson-95`.
  - `compute_mc_executed_rejects_clopper_pearson`: message names the inverse-incomplete-
    beta reason.
  - `compute_mc_executed_wilson_matches_hand_computed_value`: one series where
    `interval_low`/`interval_high` are checked against a value computed independently
    (by hand or a second formula transcription) to `1e-9`, catching a transcription bug
    in the Wilson formula itself (the unit test that would fail if the code above has a
    sign or parenthesization error).

## Task 3: `column_count_matches_invariant` and `evaluate_measurement`

**Decision (design left the exact signature open):** thread a `mc_executed: bool`
parameter through `column_count_matches_invariant`. Without it, the function cannot tell
"3 columns, single-scalar invariant" apart for an EXECUTED mc receipt versus a receipt
that just set `--columns 3` on an ordinary single-scalar invariant, and the two must be
treated differently (the former valid, the latter refused, exactly as `column_count == 2`
already is for every invariant except `cross-backend`). Rationale: this is the minimal
change that keeps the contract PRECISE (an EXECUTED mc receipt requires exactly 3 columns
paired with a single-scalar invariant; `relation`/`cross-backend` explicitly refuse
`mc_executed == true`, since their columns mean something else) rather than loosening
`column_count == 1` to `column_count in {1, 3}` for every single-scalar invariant
regardless of whether an MC block backs it.

- [ ] `column_count_matches_invariant` (currently lines 904-923), new signature and body:

  ```rust
  pub fn column_count_matches_invariant(name: &str, column_count: usize, mc_executed: bool) -> bool {
      if name == RELATION_INVARIANT {
          !mc_executed && column_count >= 2
      } else if name == CROSS_BACKEND_INVARIANT {
          !mc_executed && column_count == 2
      } else if mc_executed {
          column_count == 3
      } else {
          column_count == 1
      }
  }
  ```

  Update the doc comment: "the `relation` invariant reads across a row's columns...; an
  EXECUTED monte_carlo receipt requires exactly 3 columns (the invariant scalar plus the
  witnessed successes/trials counters) paired with a single-scalar invariant name, never
  with `relation`/`cross-backend` (their columns already mean something else); every other
  single-scalar invariant requires exactly 1."
- [ ] Update both call sites for the new parameter:
  - `main.rs` emit gate (currently line 7658): pass the emit-time `mc_executed` bool
    (from the new `--mc-executed` flag, Task 5).
  - `scientific_runtime.rs` verify gate (currently line 2167): pass
    `receipt.monte_carlo.as_ref().is_some_and(|mc| mc.status == "EXECUTED")`.
- [ ] `evaluate_measurement` (currently lines 1041-1065): add the mirror arm the design
  names, between the existing `RELATION_INVARIANT | CROSS_BACKEND_INVARIANT` arm and the
  final catch-all:

  ```rust
  pub fn evaluate_measurement(
      name: &str,
      series: &[f64],
      tol: f64,
      column_count: usize,
  ) -> MeasurementVerdict {
      match name {
          RELATION_INVARIANT | CROSS_BACKEND_INVARIANT => {
              // unchanged
              let (observed, rows) = relation_columns_agree(series, tol, column_count);
              MeasurementVerdict { observed, effective_len: rows }
          }
          _ if column_count == 3 => {
              // An EXECUTED monte_carlo receipt: column 0 is the declared
              // single-scalar invariant, columns 1-2 are the witnessed
              // successes/trials counters `compute_mc_executed` checks
              // separately. De-interleave and evaluate the invariant over
              // column 0 only; rows are the effective observation count,
              // mirroring the relation arm above. Ragged (not a multiple of
              // 3) yields zero rows, same "cannot witness" treatment
              // `relation_columns_agree` gives a ragged relation series.
              let ragged = series.is_empty() || series.len() % 3 != 0;
              let col0: Vec<f64> = if ragged {
                  Vec::new()
              } else {
                  series.iter().step_by(3).copied().collect()
              };
              let rows = col0.len();
              MeasurementVerdict {
                  observed: evaluate_invariant(name, &col0, tol),
                  effective_len: rows,
              }
          }
          _ => MeasurementVerdict {
              observed: evaluate_invariant(name, series, tol),
              effective_len: series.len(),
          },
      }
  }
  ```

  This arm is reached only for names other than `relation`/`cross-backend` (the first arm
  already claimed those unconditionally), so a genuine 3-column `relation` request is
  unaffected. `column_count_matches_invariant` (above) is the ONLY gate that ties
  `column_count == 3` to `mc_executed`; `evaluate_measurement` trusts that gate already
  ran, consistent with how it already trusts `column_count_matches_invariant` for the
  relation arm.
- [ ] Unit tests: `evaluate_measurement_deinterleaves_column_zero_for_three_columns`
  (a 3-column series where columns 1-2 would fail `non_negative` but column 0 passes;
  assert the verdict reads column 0 only) and
  `evaluate_measurement_three_column_ragged_series_cannot_witness` (mirrors the existing
  ragged-relation test).

## Task 4: verify wiring (`scientific_runtime.rs`, `evaluate_scientific_runtime_receipt`)

**Stage A** extends the existing MC admission block (currently lines 2323-2349, all
before the re-run at line 2586, so this stays pre-re-run and needs no C compiler):

- [ ] Replace the block with a two-arm version:

  ```rust
  if let Some(mc) = &receipt.monte_carlo {
      if !rederived_uses_random || receipt.seed_value.is_none() {
          eprintln!(
              "Error: the receipt carries a monte_carlo block but the program is not a seeded Random run (an MC estimate needs the Random capability and a sealed seed to re-derive)"
          );
          return Err(verify_failure_class(json, "FIELD_CONTRACT_VIOLATION", 1));
      }
      if mc.samples == 0 {
          eprintln!("Error: monte_carlo.samples is 0: an MC claim without its denominator is unpriceable");
          return Err(verify_failure_class(json, "FIELD_CONTRACT_VIOLATION", 1));
      }
      if mc.estimator.trim().is_empty() || mc.interval_method.trim().is_empty() {
          eprintln!("Error: monte_carlo declares an empty estimator or interval_method: the claim is the interval, never the point, so both must be named");
          return Err(verify_failure_class(json, "FIELD_CONTRACT_VIOLATION", 1));
      }
      let executed_fields_present = mc.estimate.is_some()
          || mc.interval_low.is_some()
          || mc.interval_high.is_some()
          || mc.n_effective.is_some()
          || mc.successes.is_some();
      match mc.status.as_str() {
          "DECLARED" => {
              if executed_fields_present {
                  eprintln!("Error: monte_carlo.status is DECLARED but an executed field is present: a DECLARED block never carries estimate/interval_low/interval_high/n_effective/successes");
                  return Err(verify_failure_class(json, "FIELD_CONTRACT_VIOLATION", 1));
              }
          }
          "EXECUTED" => {
              let (Some(estimate), Some(interval_low), Some(interval_high), Some(n_effective), Some(successes)) =
                  (mc.estimate, mc.interval_low, mc.interval_high, mc.n_effective, mc.successes)
              else {
                  eprintln!("Error: monte_carlo.status is EXECUTED but not all five executed fields (estimate, interval_low, interval_high, n_effective, successes) are present");
                  return Err(verify_failure_class(json, "FIELD_CONTRACT_VIOLATION", 1));
              };
              if mc.estimator != MC_EXECUTED_ESTIMATOR_PROPORTION {
                  eprintln!("Error: EXECUTED monte_carlo estimator `{}` is not in the executable vocabulary (v1: proportion)", mc.estimator);
                  return Err(verify_failure_class(json, "FIELD_CONTRACT_VIOLATION", 1));
              }
              let computed = compute_mc_executed(
                  &receipt.measurement.observed_values,
                  receipt.measurement.column_count,
                  mc.samples,
                  &mc.interval_method,
              )
              .map_err(|reason| {
                  eprintln!("Error: EXECUTED monte_carlo Stage A (sealed series, no re-run) recompute failed: {reason}");
                  verify_failure_class(json, "FIELD_CONTRACT_VIOLATION", 1)
              })?;
              if computed.n_effective != n_effective || n_effective != mc.samples {
                  eprintln!("Error: sealed n_effective {n_effective} does not match the Stage A witnessed denominator {} (declared samples {})", computed.n_effective, mc.samples);
                  return Err(verify_failure_class(json, "FIELD_CONTRACT_VIOLATION", 1));
              }
              if computed.successes != successes {
                  eprintln!("Error: sealed successes {successes} does not match the Stage A recompute {}", computed.successes);
                  return Err(verify_failure_class(json, "FIELD_CONTRACT_VIOLATION", 1));
              }
              if (computed.estimate - estimate).abs() > MC_RECOMPUTE_TOLERANCE
                  || (computed.interval_low - interval_low).abs() > MC_RECOMPUTE_TOLERANCE
                  || (computed.interval_high - interval_high).abs() > MC_RECOMPUTE_TOLERANCE
              {
                  eprintln!("Error: sealed EXECUTED interval fields do not match the Stage A recompute over the sealed series (estimate {estimate} vs {}, low {interval_low} vs {}, high {interval_high} vs {})", computed.estimate, computed.interval_low, computed.interval_high);
                  return Err(verify_failure_class(json, "FIELD_CONTRACT_VIOLATION", 1));
              }
          }
          other => {
              eprintln!("Error: monte_carlo.status `{other}` is not expressible (only DECLARED or EXECUTED)");
              return Err(verify_failure_class(json, "FIELD_CONTRACT_VIOLATION", 1));
          }
      }
  }
  ```

- [ ] `not_claimed` biconditional, placed beside the existing `NOT_PROVES_OPTIMALITY`/
  `optimality` checks (currently lines 2491-2508):

  ```rust
  let mc_executed = receipt.monte_carlo.as_ref().is_some_and(|mc| mc.status == "EXECUTED");
  let has_all_mc_not_claimed = MC_EXECUTED_NOT_CLAIMED
      .iter()
      .all(|c| receipt.not_claimed.iter().any(|x| x == c));
  let has_any_mc_not_claimed = MC_EXECUTED_NOT_CLAIMED
      .iter()
      .any(|c| receipt.not_claimed.iter().any(|x| x == c));
  if mc_executed != has_all_mc_not_claimed || (has_any_mc_not_claimed && !has_all_mc_not_claimed) {
      eprintln!(
          "Error: not_claimed `sample_independence`/`interval_coverage`/`estimator_semantics` present={} but monte_carlo EXECUTED={}: the boundary entries must pair exactly (all three or none) with an EXECUTED block",
          has_any_mc_not_claimed, mc_executed
      );
      return Err(verify_failure_class(json, "FIELD_CONTRACT_VIOLATION", 1));
  }
  ```

**Stage B**, placed immediately after the existing `MEASUREMENT_COUNT_DRIFT` check
(currently lines 2732-2739) and before the `recompute_verdict` call, using the same
`verdict_series` the rest of post-re-run verify already uses:

- [ ] Insert:

  ```rust
  if let Some(mc) = &receipt.monte_carlo {
      if mc.status == "EXECUTED" {
          // Stage A already required all five fields present; unwrap is safe.
          let estimate = mc.estimate.expect("Stage A validated presence");
          let interval_low = mc.interval_low.expect("Stage A validated presence");
          let interval_high = mc.interval_high.expect("Stage A validated presence");
          let computed = compute_mc_executed(
              &verdict_series,
              receipt.measurement.column_count,
              mc.samples,
              &mc.interval_method,
          )
          .map_err(|reason| {
              eprintln!("Error: EXECUTED monte_carlo Stage B (re-run series) recompute failed: {reason}");
              verify_failure_class(json, "MC_INTERVAL_DRIFT", 1)
          })?;
          if computed.n_effective != mc.samples
              || computed.successes != mc.successes.expect("Stage A validated presence")
              || (computed.estimate - estimate).abs() > MC_RECOMPUTE_TOLERANCE
              || (computed.interval_low - interval_low).abs() > MC_RECOMPUTE_TOLERANCE
              || (computed.interval_high - interval_high).abs() > MC_RECOMPUTE_TOLERANCE
          {
              eprintln!("Error: EXECUTED monte_carlo interval drift: sealed fields do not match the Stage B recompute over the re-run series");
              return Err(verify_failure_class(json, "MC_INTERVAL_DRIFT", 1));
          }
      }
  }
  ```

- [ ] Add `MC_INTERVAL_DRIFT` to the `verify_failure_class` doc comment (currently around
  lines 3175-3177, beside `MEASUREMENT_COUNT_DRIFT`): "`MC_INTERVAL_DRIFT`: an EXECUTED
  monte_carlo receipt's sealed interval fields do not match the Stage B recompute over the
  re-run series (Stage A already passed over the sealed series; this catches a receipt
  that stopped describing the run it names)."
- [ ] Unit tests in `scientific_runtime.rs`'s `#[cfg(test)] mod tests`, extending
  `verify_enforces_the_monte_carlo_admission_contracts` or a new sibling test:
  - `verify_stage_a_rejects_tampered_executed_interval_before_any_rerun`: build a receipt
    with a coherent EXECUTED block, tamper `interval_high` directly on the
    `ScientificRuntimeReceipt` (bypassing `seal_receipt` so the seal is stale is wrong;
    instead mutate then re-seal via the same path the self-test's `reseal_json` uses),
    pass a `rerun_series` closure that `panic!`s if called (the existing idiom at line
    5697), assert `Err(1)` with the panic never firing (the direct proof that Stage A
    rejects before Task 8's self-test case exercises it through the CLI).
  - `verify_stage_b_reports_mc_interval_drift_on_a_changed_rerun`: a coherent EXECUTED
    receipt whose `rerun_series` closure returns a DIFFERENT coherent series (e.g. one
    more successful draw), assert `Err(1)` and that the printed `failure_class` is
    `MC_INTERVAL_DRIFT` (use the `--json` path, matching the existing test idiom that
    reads `failure_class` from the JSON report).
  - `verify_refuses_declared_block_carrying_an_executed_field`.
  - `verify_refuses_executed_block_missing_an_executed_field`.
  - `verify_refuses_executed_estimator_outside_vocabulary`.
  - `verify_not_claimed_triad_biconditional`: EXECUTED block with the triad ABSENT is
    refused; DECLARED block with the triad PRESENT is refused; EXECUTED with only two of
    three present is refused.

## Task 5: emit wiring (`compiler/src/main.rs`)

- [ ] New CLI flag, `Commands::Run`, immediately after `mc_interval` (currently line 272)
  and before `budget_steps` (currently line 274):

  ```rust
  /// Declare the Monte Carlo run EXECUTED: the verifier re-derives the
  /// interval from raw sufficient-statistic columns the kernel prints
  /// (successes/trials counters beside the invariant scalar) instead of
  /// trusting the declaration. Requires all three --mc-* flags together;
  /// forces --columns to 3 (an unset default is silently upgraded, any
  /// other explicit value is refused, the --cross-backend idiom).
  #[arg(long)]
  mc_executed: bool,
  ```

- [ ] Thread `mc_executed` through the `main()` dispatch match arm (currently lines
  662-730): add to the `Commands::Run { .. }` destructure, add to the `--gpu` refusal
  condition (currently lines 689-693: `else if mc_estimator.is_some() || mc_samples.is_some() || mc_interval.is_some()`, add `|| mc_executed` since a bare `--gpu --mc-executed` with no other mc flags
  bypasses `cmd_run` entirely via `cmd_run_gpu` and would otherwise never hit the
  all-or-nothing gate inside `cmd_run`), and pass it as a new positional argument to
  `cmd_run(...)` after `mc_interval.as_deref()`.
- [ ] `cmd_run` signature (currently lines 7471-7490): add `mc_executed: bool` after
  `mc_interval: Option<&str>`.
- [ ] Extend the MC declaration gate (currently lines 7504-7532) with the mc_executed
  pairing and vocabulary checks, still CLI-shape-only (before compiling):

  ```rust
  if mc_executed && mc_flag_count < 3 {
      eprintln!("Error: --mc-executed requires the full Monte Carlo declaration (--mc-estimator, --mc-samples, --mc-interval)");
      return Err(1);
  }
  if let Some(estimator) = mc_estimator {
      if mc_executed && estimator != MC_EXECUTED_ESTIMATOR_PROPORTION {
          eprintln!("Error: --mc-executed requires --mc-estimator proportion (v1 executable vocabulary); DECLARED blocks may still use free text");
          return Err(1);
      }
  }
  ```

  (The interval-method vocabulary check is NOT duplicated here: it is fail-closed inside
  `compute_mc_executed`, called once real data exists, below.)
- [ ] Columns auto-upgrade (currently lines 7643-7651): add the mc_executed branch beside
  the existing cross-backend one:

  ```rust
  let columns = if invariant_name == CROSS_BACKEND_INVARIANT && columns == 1 {
      2
  } else if mc_executed && columns == 1 {
      3
  } else {
      columns
  };
  ```

- [ ] Column-structure gate call (currently line 7658): pass `mc_executed` as the new
  third argument to `column_count_matches_invariant`; add a branch to the error-message
  `if`/`else if` chain (currently lines 7659-7671) for `mc_executed`: `"--mc-executed
  needs --columns 3 (the invariant scalar plus the witnessed successes/trials counters);
  the invariant defines its own column structure"`.
- [ ] `--cross-backend` composition (currently lines 7743-7748, `if mc_flag_count > 0`):
  this already transitively refuses `--cross-backend --mc-executed` whenever the three
  declaration flags are present (which `--mc-executed` now requires); no code change
  needed here, but add a one-line comment noting the transitive coverage so a future
  reader does not think it is missing.
- [ ] **Defer EXECUTED finalization until after capture.** The early `monte_carlo`
  construction (currently lines 7504-7532) can only build the DECLARED shape (status is
  unknown to be EXECUTED-and-coherent until the real series exists). Change: the early
  block always builds a `status: "DECLARED".to_string()` block as today (all five new
  fields `None`), stored in `monte_carlo`. After the run is captured and `series`/
  `column_count` are finalized (currently lines 7889-7930, the same place
  `cross_backend_block` is finalized), if `mc_executed` is true, recompute the final
  block:

  ```rust
  let monte_carlo = if mc_executed {
      let mc = monte_carlo.expect("mc_executed implies mc_flag_count == 3, checked above");
      let computed = compute_mc_executed(&series, column_count, mc.samples, &mc.interval_method)
          .map_err(|reason| {
              eprintln!("Error: --mc-executed refuses to seal an incoherent EXECUTED block: {reason}");
              1i32
          })?;
      Some(ScientificMonteCarlo {
          status: "EXECUTED".to_string(),
          estimate: Some(computed.estimate),
          interval_low: Some(computed.interval_low),
          interval_high: Some(computed.interval_high),
          n_effective: Some(computed.n_effective),
          successes: Some(computed.successes),
          ..mc
      })
  } else {
      monte_carlo
  };
  ```

  Place this right after the `series`/`column_count`/`cross_backend_block` tuple
  (currently ending line 7930) and before `ScientificReceiptInputs` is built (currently
  line 7938), so it fires BEFORE sealing (fail closed: an incoherent EXECUTED block never
  reaches `build_scientific_runtime_receipt`).
- [ ] `not_claimed` additions at emit, `scientific_runtime.rs`,
  `build_scientific_runtime_receipt` (currently lines 1214-1218, after the existing
  budget/`optimality` push, before `witnessed_fields_from_capabilities`):

  ```rust
  if let Some(mc) = &monte_carlo {
      if mc.status == "EXECUTED" {
          not_claimed.extend(MC_EXECUTED_NOT_CLAIMED.iter().map(|s| s.to_string()));
      }
  }
  ```

## Task 6: kernel, negative fixture, corpus (27 -> 29)

- [ ] `examples/mc_pi_rejection_executed.bld`: copy `examples/mc_pi_rejection.bld`'s
  algorithm exactly (same `n = 2000`, `burn = 200`, `band = 0.3`, same seed-42 stream,
  same `4.0 * inside / k` estimate and slack computation, since a re-derivation on the
  SAME PRNG stream needs no new calibration), and print three columns per post-burn-in
  row instead of one:

  ```
  println!("{} {} {}", band - err, inside, k);
  ```

  Header comment: reuse the existing kernel's calibration prose verbatim (band 0.3 vs
  measured worst error 0.2094 under seed 42), add one sentence pointing at the DECLARED
  sibling, and add the EXECUTED-specific facts that can only be known by running the
  emitted kernel: `successes_final` (= `inside` at k = 2000), `trials_final` (= 2000, by
  construction), and the `wilson-95` `interval_low`/`interval_high` for seed 42. **These
  three numbers are not invented here** (this plan is read-only, no build/run performed
  under it): the implementer runs `buildc run examples/mc_pi_rejection_executed.bld
  --emit-receipt - --seed 42 --mc-executed --mc-estimator proportion --mc-samples 2000
  --mc-interval wilson-95 --invariant non-negative --metric slack --problem
  mc-pi-rejection-executed`, reads `monte_carlo.successes`/`interval_low`/`interval_high`
  from the printed receipt, and transcribes them into the header comment, exactly the
  discipline the shipped kernel's own header used for its `0.2094` figure.
- [ ] `examples/mc_pi_rejection_executed_broken.bld`: copy
  `examples/mc_pi_rejection_broken.bld` (the wrong-area 3.0-factor estimator) and add the
  same two extra columns (`inside`, `k`), UNCHANGED by the wrong-area factor (which only
  scales the printed `est`/slack, not the `inside` counter). Header comment: the existing
  broken kernel's prose plus one sentence stating the central lesson: the proportion
  columns are untouched, so the interval executes and re-derives cleanly (a coherent,
  witnessed EXECUTED block) while the slack column still blows the truth band
  (`FAIL_EXPECTED`), proving the two claims (interval arithmetic, estimator truth) fail
  independently.
- [ ] `ScientificCorpusMember` (`main.rs`, currently lines 1663-1705): add
  `#[serde(default)] pub mc_executed: bool` after `mc_interval` (the `negative_fixture`
  precedent: no `skip_serializing_if`, since this manifest is hand-authored input, never
  re-serialized from the struct, so byte-stability does not apply here the way it does to
  a sealed receipt).
- [ ] `cmd_receipt_corpus` (currently around lines 2073-2081, beside the `mc_interval`
  passthrough): `if member.mc_executed { emit.arg("--mc-executed"); }`.
- [ ] `examples/scientific-corpus.json`: insert two new members immediately after the
  existing `mc_pi_rejection`/`mc_pi_rejection_broken` pair (currently lines 26-27),
  keeping the file's pairing-adjacency convention. **Decision (design left the exact
  manifest shape open):** do not set an explicit `"columns"` key; rely on the same
  auto-upgrade-from-default-1 convention the `cross-backend` singleton member already
  relies on (it carries no `"columns"` key either), rather than introducing a new,
  inconsistent style for this pair.

  ```json
  {"source": "examples/mc_pi_rejection_executed.bld", "invariant": "non-negative", "seed": 42, "mc_estimator": "proportion", "mc_samples": 2000, "mc_interval": "wilson-95", "mc_executed": true, "expected_status": "PASS"},
  {"source": "examples/mc_pi_rejection_executed_broken.bld", "invariant": "non-negative", "negative_fixture": true, "seed": 42, "mc_estimator": "proportion", "mc_samples": 2000, "mc_interval": "wilson-95", "mc_executed": true, "expected_status": "FAIL_EXPECTED"},
  ```

  Corpus count: 27 -> 29.

## Task 7: self-test case 10 (9 -> 10)

**Decision (design left the exact tamper shape open):** do NOT synthesize a fully
self-contained coherent EXECUTED receipt state (rewriting `invariant.name`,
`measurement.observed_values`, `column_count`, `observed`, `receipt_status`, etc. to keep
everything cross-referentially consistent). Reuse cases 7-9's existing "mutate the
existing block if present, else add a syntactically valid one" idiom instead. Rationale,
verified by reading the verify code path (Task 4): EVERY pre-re-run EXECUTED violation
this slice adds reports `FIELD_CONTRACT_VIOLATION`, the SAME class case 7 already relies
on when its zero-denominator MC tamper is checked against a receipt whose underlying
kernel (e.g. `funnel_probe.bld`, the fixture the shipped self-test CLI test already uses)
does not even observe `Random` — case 7 passes today regardless of WHICH specific MC gate
fires first, because `--self-test` only asserts the `failure_class` string, never the
specific message. Case 10 gets the same robustness for free: whichever stage-A gate fires
first (seed pairing, field presence, or the interval-mismatch check this case targets) on
an arbitrary pristine input, the reported class is `FIELD_CONTRACT_VIOLATION` either way.

- [ ] Add to `build_self_test_cases` (`scientific_runtime.rs`, after case 9, currently
  ending line 1574):

  ```rust
  // 10. FIELD_CONTRACT_VIOLATION (MC executed interval does not recompute):
  //    nudge the sealed interval_high on an EXECUTED monte_carlo block (the
  //    receipt's own block if it already carries one -- e.g. a receipt
  //    emitted from mc_pi_rejection_executed.bld -- else a syntactically
  //    valid one is added, mirroring case 9's cross_backend fallback).
  //    Either way the tamper reaches Stage A's FIELD_CONTRACT_VIOLATION arm:
  //    see the design note above for why this is robust to whichever
  //    receipt self-test is run against.
  {
      let mut v = receipt_json.clone();
      match v.get_mut("monte_carlo") {
          Some(mc) if !mc.is_null() && mc.get("status").and_then(|s| s.as_str()) == Some("EXECUTED") => {
              let bumped = mc["interval_high"].as_f64().unwrap_or(0.5) + 0.25;
              mc["interval_high"] = serde_json::Value::from(bumped);
          }
          _ => {
              v["monte_carlo"] = serde_json::json!({
                  "estimator": "proportion",
                  "samples": 4u64,
                  "interval_method": "wilson-95",
                  "status": "EXECUTED",
                  "estimate": 0.5,
                  "interval_low": 0.15,
                  "interval_high": 0.85,
                  "n_effective": 4u64,
                  "successes": 2u64,
              });
          }
      }
      let v = reseal_json(&v)?;
      cases.push(SelfTestCase {
          label: "EXECUTED monte_carlo interval_high nudged against its Stage A recompute".to_string(),
          tampered: v,
          expected_class: "FIELD_CONTRACT_VIOLATION".to_string(),
          resealed: true,
      });
  }
  ```

- [ ] Update `self_test_cases_cover_distinct_failure_classes_and_seal_states` (currently
  line ~3439-3444 area) to expect 10 cases.
- [ ] Update `compiler/tests/cli.rs`'s
  `receipt_verify_self_test_proves_the_verifier_can_fail` (currently lines 15833-15897):
  the `stdout.contains("9/9 tampers rejected...")` assertion (line 15878) becomes
  `"10/10 tampers rejected with the expected failure_class"`.

## Task 8: CLI tests (`compiler/tests/cli.rs`)

Model on the existing `mc_pi_rejection` round-trip block (currently lines 14940-15080+,
the `mc_flags` helper it builds from).

- [ ] Round-trip test `mc_pi_rejection_executed_round_trip_and_negative_fixture`: emit and
  verify the PASS receipt from `mc_pi_rejection_executed.bld` under
  `--mc-executed --mc-estimator proportion --mc-samples 2000 --mc-interval wilson-95`,
  assert `receipt_status == "PASS"`, `monte_carlo.status == "EXECUTED"`,
  `monte_carlo.n_effective == 2000`, `monte_carlo.successes` present and
  `<= 2000`, `monte_carlo.interval_low < monte_carlo.estimate < monte_carlo.interval_high`;
  emit and verify the FAIL_EXPECTED receipt from `mc_pi_rejection_executed_broken.bld`
  with `--negative-fixture`, assert `receipt_status == "FAIL_EXPECTED"` AND
  `monte_carlo.status == "EXECUTED"` with a re-deriving interval (the central lesson: the
  interval claim and the truth-band claim fail independently).
- [ ] Six emit-refusal tests, each asserting non-zero exit and a specific stderr
  substring:
  1. `mc_executed_without_full_declaration_is_refused`: `--mc-executed` alone (no
     `--mc-estimator`/`--mc-samples`/`--mc-interval`); expect "requires the full Monte
     Carlo declaration".
  2. `mc_executed_clopper_pearson_is_refused`: `--mc-interval clopper-pearson-95`; expect
     "not in the EXECUTED executable vocabulary" / "inverse incomplete beta".
  3. `mc_executed_declared_samples_mismatching_witnessed_trials_is_refused`: use a
     modified copy of the executed kernel (or a `--` trailing arg that changes its loop
     bound, if the kernel supports one; otherwise add a tiny new fixture kernel that
     prints exactly 999 rows) declared with `--mc-samples 2000`; expect "does not equal the
     declared samples".
  4. `mc_executed_incoherent_successes_jump_is_refused`: a tiny purpose-built fixture
     kernel that prints a successes column jumping by 2 in one step; expect "does not
     follow" / "by 0 or 1".
  5. `mc_executed_explicit_columns_other_than_three_is_refused`: `--mc-executed --columns
     2`; expect "needs --columns 3".
  6. `mc_executed_normal_approx_degenerate_boundary_is_refused`: a tiny purpose-built
     fixture kernel whose counter never fails a single draw (successes == trials always,
     e.g. printing `x*x+y*y < 2.0` which is always true in the unit square) with
     `--mc-interval normal-approx-95`; expect "degenerate" / "wilson-95".
  Fixture kernels for cases 3, 4, 6 are new, minimal `.bld` files under
  `examples/` or `compiler/tests/programs/` (implementer's choice of directory,
  matching whichever the existing tiny CLI-test-only fixtures already use in this file);
  they are NOT corpus members (the design explicitly scopes these as "cli.rs, not
  corpus").
- [ ] `--gpu`/`--mc-executed` and `--cross-backend`/`--mc-executed` composition refusal
  tests (mirroring the existing `--gpu`/`--mc-*` and `--cross-backend`/`--mc-*` tests
  already in this file; grep for them and add the `--mc-executed` sibling case beside
  each).

## Task 9: mutation checks (every new gate; break, observe red, restore, observe green)

- [ ] `compute_mc_executed`: remove the `trials_k != prev + 1.0` check ->
  `compute_mc_executed_rejects_before_any_rerun_on_bad_trials_step` red; restore, green.
- [ ] `compute_mc_executed`: remove the successes-delta-in-{0,1} check ->
  `compute_mc_executed_rejects_successes_decrease` red; restore, green.
- [ ] `compute_mc_executed`: remove the `successes_k > trials_k` check ->
  `compute_mc_executed_rejects_successes_exceeding_trials` red; restore, green.
- [ ] `compute_mc_executed`: remove the `trials_final != samples` check ->
  `compute_mc_executed_witnessed_denominator_must_equal_samples` red; restore, green.
- [ ] `compute_mc_executed`: remove the normal-approx boundary guard ->
  `compute_mc_executed_normal_approx_degenerate_at_zero_successes` red; restore, green.
- [ ] `compute_mc_executed`: flip a sign in the Wilson `margin` computation ->
  `compute_mc_executed_wilson_matches_hand_computed_value` red; restore, green (this is
  the mutation that would NOT be caught by any coherence test, only the hand-computed
  value test, so it is worth calling out explicitly in the commit body).
- [ ] `column_count_matches_invariant`: remove the `!mc_executed &&` guard on the
  `RELATION_INVARIANT` arm -> a new test asserting `column_count_matches_invariant(RELATION_INVARIANT, 3, true) == false` goes red; restore, green.
- [ ] `evaluate_measurement`: remove the ragged-series guard in the new arm ->
  `evaluate_measurement_three_column_ragged_series_cannot_witness` red; restore, green.
- [ ] Verify Stage A: remove the `executed_fields_present` DECLARED-biconditional check ->
  `verify_refuses_declared_block_carrying_an_executed_field` red; restore, green.
- [ ] Verify Stage A: remove the all-five-fields-present destructure guard (replace with
  an unconditional unwrap-or-default) -> `verify_refuses_executed_block_missing_an_executed_field` red; restore, green.
- [ ] Verify Stage A: remove the estimator vocabulary check ->
  `verify_refuses_executed_estimator_outside_vocabulary` red; restore, green.
- [ ] Verify Stage A: loosen the `MC_RECOMPUTE_TOLERANCE` comparison to always pass ->
  `verify_stage_a_rejects_tampered_executed_interval_before_any_rerun` red; restore,
  green.
- [ ] Verify Stage B: skip the Stage B block entirely when `mc.status == "EXECUTED"` ->
  `verify_stage_b_reports_mc_interval_drift_on_a_changed_rerun` red; restore, green (this
  is the mutation that proves Stage B is load-bearing and not redundant with Stage A: a
  receipt whose sealed series was never tampered, only its RE-RUN diverges, passes Stage A
  cleanly and must be caught here).
- [ ] Verify: remove the `not_claimed` triad biconditional -> `verify_not_claimed_triad_biconditional` red; restore, green.
- [ ] Emit: remove the fail-closed `compute_mc_executed` call before sealing (seal
  unconditionally) -> the CLI refusal tests (Task 8, cases 3, 4, 6) go red (an incoherent
  block gets sealed instead of refused); restore, green.
- [ ] Self-test case 10: temporarily delete the case from `build_self_test_cases` ->
  `self_test_cases_cover_distinct_failure_classes_and_seal_states` (expects 10) red;
  restore, green.

## Task 10: docs deltas (same commit)

- [ ] `docs/SCIENTIFIC-RECEIPT.md`, section 1 (flags, currently lines 98-103 for the
  existing `--mc-*` bullets): add a `--mc-executed` bullet describing the boolean flag,
  the all-or-nothing requirement, the columns auto-upgrade, and the fail-closed emit
  refusal.
- [ ] Section 2 (schema, currently lines 218-226, the `monte_carlo` block description):
  rewrite to describe the two-arm `status`, the five new `Option` fields present iff
  EXECUTED, the estimator/interval-method executable vocabulary (`proportion`;
  `normal-approx-95`, `wilson-95`; `clopper-pearson-95` sealed-successes-only, not
  executable), the witnessed-denominator equality, and the DECLARED-vs-EXECUTED
  biconditional on the five fields (mirroring the existing `wall_exceeded`-without-
  `wall_seconds_limit` idiom prose already in this section for `budget`).
- [ ] Section 3 (invariant family, currently the `column_count` discussion around line
  316+): add one paragraph on the EXECUTED 3-column shape and the `evaluate_measurement`
  de-interleave-to-column-0 behavior.
- [ ] Failure classes table (currently lines 587-614): add `MC_INTERVAL_DRIFT` as its own
  row, exit 1, "an EXECUTED monte_carlo receipt's sealed interval fields do not match the
  Stage B recompute over the re-run series." Extend the `FIELD_CONTRACT_VIOLATION` row's
  parenthetical (currently one long sentence) with the new EXECUTED sub-cases (missing
  executed fields, executed fields on a DECLARED block, estimator/interval_method outside
  the executable vocabulary, incoherent aggregate columns, a witnessed denominator that
  disagrees with the declared `samples`, the Stage A interval mismatch, the `not_claimed`
  triad biconditional).
- [ ] Self-test section (currently lines 622-653): "nine cases" -> "ten cases"; add case
  10's description in the enumerated list; update the "There is no tenth case" closing
  paragraph (currently lines 650-653, about wall-metering) since there NOW IS a tenth
  case for a different reason. Rewrite that paragraph to describe case 10 instead (it no
  longer needs to argue why a tenth case is unnecessary).
- [ ] Corpus section (currently lines 679-711): "thirteen pairs plus the cross-backend
  singleton" -> "fourteen pairs plus the cross-backend singleton"; document the new
  `mc_executed` manifest field and its `--mc-executed` passthrough beside the existing
  `mc_estimator`/`mc_samples`/`mc_interval` prose (currently lines 684-686).
- [ ] `CHANGELOG.md` `## Unreleased` (currently starting line 11): one new bullet at the
  TOP of the list (the file's newest-first convention, confirmed by the split-frontier
  entry currently occupying that position), same register as the existing entries
  (feature name in bold, honest scope, concrete numbers once Task 11 produces them: corpus
  29/29, self-test 10/10, full-suite pass count). Content: the two-stage recompute, the
  witnessed denominator, the `not_claimed` additions, and the explicit statement that
  EXECUTED hardens interval arithmetic and the denominator, never the estimator's
  semantics or independence.

## Task 11: final verification gate

- [ ] `cargo fmt --check --manifest-path compiler/Cargo.toml`: clean.
- [ ] `buildc corpus verify examples/scientific-corpus.json`: `29/29`. Capture the exit
  code directly (no pipe) before printing/inspecting output.
- [ ] `buildc receipt verify <a freshly emitted mc_pi_rejection_executed.bld receipt>.json
  --self-test`: `10/10`. Capture the exit code directly before inspecting output.
- [ ] Full `cargo test --manifest-path compiler/Cargo.toml`: 0 failed. Record the exact
  passed/ignored counts (do not assume the pre-slice baseline is unchanged; report the
  new numbers).
- [ ] Re-run `buildc corpus verify` a second time to confirm determinism (two consecutive
  clean runs), matching the discipline the split-frontier increment applied to its own
  gates.
- [ ] Only after all of the above are green: fill in CHANGELOG's numbers (Task 10) and
  commit. One commit, on `feat/mc-executed-intervals`, not pushed, per Global Constraints.
