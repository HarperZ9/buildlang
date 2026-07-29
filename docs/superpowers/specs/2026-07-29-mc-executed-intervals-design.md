# w1: Monte Carlo EXECUTED interval discipline (design)

> Design document, not a plan. Internal register. Grounded 2026-07-29 against
> branch `feat/drop-flags` (HEAD b57abb1), the slice 2 commit 9430439, the
> slice 2 plan (`docs/superpowers/plans/2026-07-28-mc-estimator-receipts.md`),
> `compiler/src/scientific_runtime.rs`, `docs/SCIENTIFIC-RECEIPT.md` sections
> 1-3 and 6, and the five-modes brief
> (`git show docs/epistemic-os-vision:docs/superpowers/specs/2026-07-28-five-modes-one-environment.md`).
> All file:line references below verified today (high confidence).

## 0. The answer up front

EXECUTED intervals are achievable without the verifier trusting an
unverifiable kernel claim, but only for the ARITHMETIC layer of the claim.
The verifier can honestly compute the interval itself if the kernel prints
the estimator's raw sufficient statistic as extra columns of the captured
series: cumulative `successes` and `trials` counters beside the existing
invariant scalar. Those are pre-arithmetic facts the verifier can check for
structural coherence and re-derive exactly under the sealed seed, and the
interval over them is a fixed computation the verifier owns end to end.

What can NEVER be executed is the SEMANTIC layer: that the indicator counts
what the author says it counts, that the draws behave as independent samples,
and that the named confidence level covers the true value. Those stay
declared, and the design seals them as does-not-prove facts (new `not_claimed`
entries) rather than pretending the upgrade removed them. This is the
honest-refusal clause resolved as a partial refusal: EXECUTED hardens the
interval arithmetic and the denominator; it does not, and cannot, harden the
indicator's meaning. The design says so in the receipt itself.

## 1. What ships today (the gap, precisely)

- The `monte_carlo` block seals `{estimator, samples, interval_method,
  status: "DECLARED"}` (scientific_runtime.rs:281-293). Verify re-checks only
  SHAPE: seeded Random pairing, non-zero samples, non-empty names, status
  exactly `DECLARED` (scientific_runtime.rs:2318-2349). Nothing checks that
  `samples: 2000` has anything to do with what the program did, and nothing
  computes any interval anywhere. v0 claims reproducibility and declaration
  discipline, never interval correctness, and its docs say so.
- The shipped kernel `examples/mc_pi_rejection.bld` prints ONE column: the
  slack `band - |estimate_k - pi|` post burn-in, checked by `non_negative`.
  The estimate series is destroyed at the print site: from slack alone the
  verifier can recover `|estimate - pi|` but not the estimate, the successes
  count, or the denominator. The obstacle in the tasking is real.
- The machinery that already solves the shape of this problem:
  - Multi-column capture: the series is row-major with a sealed
    `column_count`; the relation invariant de-interleaves and computes its
    check verifier-side from raw columns, on the stated argument that a
    kernel printing raw columns cannot conceal a disagreement by computing
    the check itself (scientific_runtime.rs:113-119, 982-1027).
  - The EXECUTED precedent: `cross_backend.status == "EXECUTED"` means the
    block witnesses a run that actually happened and verify re-executes it
    rather than trusting the declaration (scientific_runtime.rs:337-361,
    docs/SCIENTIFIC-RECEIPT.md section 2). Status vocabulary per block, with
    verify pinning the one honest value, is the established idiom.
  - The column-count contract is symmetric across emit and verify
    (scientific_runtime.rs:904-923), and `evaluate_measurement` is the single
    dispatch both go through (1041-1059).
- The five-modes brief names the flywheel statistics module as the reference
  design: declared MDE, "no effect" vs "no power" made distinguishable. The
  language-level translation: a receipt that seals a witnessed denominator
  and a computed width lets a consumer price a wide interval as absence of
  power rather than absence of signal. That distinguishability, not interval
  truth, is what this upgrade buys.

## 2. Decision 1: the data path

Chosen: **(d), a refinement of (a): cumulative sufficient-statistic columns.**
The kernel prints, per post-burn-in step, one row of exactly three columns:

```
<invariant_scalar> <successes> <trials>
```

Column 0 feeds the declared invariant unchanged (the slack under
`non_negative` for the pi kernel). Columns 1 and 2 are the estimator's
running sufficient statistic as raw cumulative counters, integer-valued.
The verifier, on an EXECUTED receipt:

1. De-interleaves by the sealed `column_count == 3`.
2. Checks structural coherence of the aggregate columns: every value
   integer-valued (`fract() == 0`) and below 2^53; `trials` increments by
   exactly 1 across consecutive rows (the first row's absolute value is
   free: it is the burn-in edge); `successes` non-decreasing with increments
   in {0, 1}; `successes <= trials` on every row.
3. Takes the final row as the full-sample statistic and requires
   `trials_final == monte_carlo.samples`. The declared denominator becomes a
   WITNESSED one: this is the single biggest honesty gain of the slice, and
   it costs one equality check.
4. Computes `p_hat = successes_final / trials_final` and the interval by the
   named method, entirely in verifier-owned code, and compares against the
   sealed executed fields (tolerances in Decision 3).

The recompute runs TWICE, in two stages:

- **Stage A, no re-run:** over the SEALED `measurement.observed_values`. The
  sealed executed fields must be the named method applied to the sealed
  series. This makes a tampered-and-resealed interval a pure data
  contradiction, rejectable before any program re-run, which keeps the
  self-test's no-compiler property intact (docs/SCIENTIFIC-RECEIPT.md,
  self-test section: every case rejected before any re-run).
- **Stage B, after the re-run:** the same computation over the re-parsed
  re-run series. Verify never compares raw floats series-to-series today
  (only count and verdict), so without stage B a receipt could stay
  internally coherent while no longer describing the run it names.

### The candidates killed, with reasons

**(a) as stated (a running-estimate column beside the slack):** the running
estimate `4 * inside / k` is a DERIVED float. The verifier either trusts the
kernel's arithmetic (the exact trust EXECUTED is supposed to remove) or
reverse-engineers the transform (kernel semantics buildc cannot parse). Raw
counters are the pre-arithmetic facts; the relation invariant's argument,
moved one level down: print what you aggregated, not what you computed from
it. The refinement keeps (a)'s capture mechanism and replaces its payload.

**(b) a second `--mc-metric` series convention:** the receipt's honesty rests
on ONE stdout stream, ONE measurement block, ONE raw-stdout digest, ONE
sealed extraction policy. A second series needs a stream-splitting protocol
(prefix tags or a second channel), a second digest, and a second count rule:
a parallel copy of machinery that column de-interleaving already provides,
plus a new seam for the two streams to disagree across. Interleaved columns
also keep the aggregates row-aligned with the invariant series for free.
Killed as duplication with new failure modes and no added honesty.

**(c) verifier re-runs a REFERENCE estimator from the sealed seed:** buildc
cannot parse kernel semantics, so a reference estimator is a SECOND program
whose equivalence to the kernel is precisely the unverifiable claim. The PRNG
consumption pattern (draw order, draws per iteration, burn-in) is
kernel-specific, so a mismatch is a false alarm and a match is unearned
confidence, and the mechanism blesses exactly one kernel shape forever. This
is the declared trust re-introduced at one remove, wearing an EXECUTED
badge, which is worse than DECLARED because it lies about its category.
Killed on principle, not on cost.

### Machinery deltas the chosen path requires

- `column_count_matches_invariant` (scientific_runtime.rs:911-923) gains an
  arm: an EXECUTED mc receipt requires `column_count == 3` paired with a
  single-scalar invariant. `relation` and `cross-backend` are refused with an
  EXECUTED mc block (`cross_backend` is already transitively excluded, since
  it refuses Random and mc requires it, scientific_runtime.rs:2442-2448).
- `evaluate_measurement` (scientific_runtime.rs:1041-1059) gains the mirror
  of the relation arm: for an EXECUTED mc receipt a single-scalar invariant
  evaluates over de-interleaved column 0, and `effective_len` is the ROW
  count. `measurement.count` stays the total token count, exactly as
  `relation` handles it, so token drift stays independently caught.
- New emit flag `--mc-executed` (boolean): requires all three `--mc-*`
  declaration flags; forces `--columns` to 3 in the cross-backend idiom
  (unset default silently upgraded, any other value refused); refused with
  `--gpu` and `--cross-backend` (both already refuse the mc flags).

## 3. Decision 2: what EXECUTED claims, and refuses to claim

An EXECUTED `monte_carlo` block claims, and verify re-derives, exactly:

1. The sealed interval is the named method applied to the captured aggregate
   columns (stage A), and the same computation over a fresh re-run under the
   sealed seed re-derives it (stage B).
2. The denominator is witnessed: `samples == n_effective == trials_final`,
   re-derived from the seeded stream.
3. The aggregate stream is structurally coherent as a cumulative Bernoulli
   count (the checks in Decision 1).

It refuses to claim, sealed machine-readably as `not_claimed` additions
(present IFF the block is EXECUTED, the `NOT_PROVES_OPTIMALITY` pairing
idiom) plus does-not-prove prose in the docs:

- `sample_independence`: the PRNG is a deterministic recurrence; independence
  is a modeling assumption about it, not a checkable fact.
- `interval_coverage`: the 95% is a property of the method under assumptions
  the receipt cannot check. EXECUTED does not claim the true value lies in
  the interval, for ANY value of "true".
- `estimator_semantics`: the verifier witnesses that a coherent counter was
  aggregated and the arithmetic over it; it cannot witness that the indicator
  measures quarter-circle membership rather than anything else, nor that any
  author-side transform (pi = 4p) is the right one. The truth-band slack in
  column 0 keeps carrying that check for known-answer kernels, as a separate,
  honestly-labeled claim.

Estimator unbiasedness needs no new entry: it is subsumed by
`estimator_semantics` and the existing physical-law boundary. The oracle
block does NOT change: the interval is an admission-block fact whose
"verdict" is re-derivation, not pass/fail; promoting it to an oracle kind
(`executed_interval`) was considered and killed because it would conflate the
receipt's verdict criterion (the invariant) with a computation that has no
pass/fail semantics of its own.

## 4. Decision 3: v1 method set and numeric discipline

Executed vocabulary, pinned (an EXECUTED block must name one of these; the
verifier owns the arithmetic):

- `normal-approx-95`: `p_hat +/- z * sqrt(p_hat * (1 - p_hat) / n)`, with z
  pinned as a named const `1.959963984540054` (the double nearest the 0.975
  normal quantile) beside the family tolerances. Degenerate guard: refused at
  emit when `successes == 0` or `successes == trials` (a zero-width interval
  at the boundary overclaims; the error message points at `wilson-95`).
- `wilson-95`: the Wilson score interval, same pinned z. Well-defined at the
  boundary proportions, asymmetric by construction.
- `clopper-pearson-95`: NOT executable in v1. Exact binomial bounds need an
  inverse incomplete beta, which is its own numerics-correctness burden with
  no in-tree oracle. A method the verifier cannot execute cannot ride on an
  EXECUTED block: refused at emit and at verify. The raw `successes` count IS
  sealed, so v2 can add it with no schema or kernel change. DECLARED blocks
  keep free-text method names forever (their claim never included execution).

Executed estimator vocabulary v1: `proportion` only (the mean of Bernoulli
indicators). DECLARED blocks keep free text (the shipped corpus uses `mean`,
untouched).

Sealed executed fields, present IFF `status == "EXECUTED"`:

```
estimate      f64   p_hat = successes_final / trials_final
interval_low  f64   lower bound by the named method
interval_high f64   upper bound by the named method
n_effective   u64   trials_final (must equal samples)
successes     u64   successes_final
```

Named deviation from the tasking sketch (`{estimate, half_width,
n_effective}`): Wilson's interval is not centered on `p_hat`, so a lone
half_width either loses the center or forces a dishonestly symmetric
reading. Low/high bounds are method-agnostic; half_width is derivable for
the symmetric method by anyone who wants it.

Numeric discipline:

- Integers exact: `n_effective`, `successes`, and the integrality checks on
  the columns are exact comparisons. The aggregates re-derive exactly under
  the sealed seed: the PRNG is integer arithmetic over u64 state, and for the
  reference kernel the indicator path (`x*x + y*y < 1.0`) uses only IEEE
  correctly-rounded ops with no libm calls, so the counter stream is
  bit-stable across platforms (moderate confidence for the general claim;
  high for this kernel class). Kernel-author discipline, documented not
  enforced: aggregate columns must be libm-free integer counters, or stage B
  will fail loudly on the platform where the stream differs, which is the
  correct outcome.
- Floats within a pinned absolute `1e-12` (`MC_RECOMPUTE_TOLERANCE`), applied
  to `estimate`, `interval_low`, `interval_high` in both stages. The interval
  arithmetic runs on identical integer inputs through one fixed Rust
  implementation at emit and verify, so agreement should be exact in
  practice; the tolerance is headroom against a future compiler reassociating
  verifier-side float ops, in the family's verdict-robustness style, not a
  load-bearing looseness. Values are O(1) proportions, so absolute is safe.

## 5. Decision 4: backward compatibility and the verify contracts

- **DECLARED receipts stay valid forever.** The status gate at
  scientific_runtime.rs:2342-2348 becomes a two-arm vocabulary
  (`DECLARED | EXECUTED`); every contract added by this slice fires only on
  `EXECUTED`. The five executed fields are `Option` +
  `#[serde(default, skip_serializing_if = "Option::is_none")]`, so every
  receipt sealed before this slice parses and re-serializes to its exact
  bytes and seal. `ScientificMonteCarlo` loses `Eq` (f64 fields), the
  `ScientificBudget` precedent (scientific_runtime.rs:306-309).
- Schema stays `v0`: additive optional fields within v0 is the shipped
  precedent (wall metering added `wall_seconds_limit`/`wall_exceeded` the
  same way).
- **EXECUTED is opt-in** via `--mc-executed`; the DECLARED emission path is
  byte-for-byte untouched, existing corpus members re-emit unchanged.
- Verify contracts, by status:
  - DECLARED: exactly today's four checks (seeded-Random pairing, non-zero
    samples, non-empty names, status vocabulary), PLUS: any executed field
    present on a DECLARED block is refused (the biconditional, the
    `wall_exceeded`-without-`wall_seconds_limit` idiom).
  - EXECUTED: the DECLARED shape checks, then: all five executed fields
    present; estimator in the executable vocabulary; interval_method in the
    executable vocabulary; `column_count == 3` with a single-scalar
    invariant; aggregate coherence over the sealed series; stage A recompute;
    `n_effective == samples`; then after the re-run, stage B recompute and
    coherence over the re-derived series.
- Failure classes, additive within v0 (the table has grown slice by slice):
  every pre-re-run EXECUTED violation is `FIELD_CONTRACT_VIOLATION` (a sealed
  field claims something the sealed data contradicts, the existing meaning);
  stage B disagreement is one new class, `MC_INTERVAL_DRIFT`, exit 1, sitting
  in the drift family beside `MEASUREMENT_COUNT_DRIFT`. Old receipts can
  never produce the new class.

## 6. Decision 5: kernel migration, corpus, fixtures

- **`examples/mc_pi_rejection.bld` stays exactly as it is**, as the DECLARED
  exemplar. It is the backward-compat witness: rewriting it would orphan the
  DECLARED path's corpus coverage and falsify the claim that DECLARED
  receipts remain first-class. Its header gains one sentence pointing at the
  executed sibling.
- **New `examples/mc_pi_rejection_executed.bld`:** the same rejection kernel
  printing three-column rows `slack inside k` post burn-in, emitted with
  `--seed 42 --mc-executed --mc-estimator proportion --mc-samples 2000
  --mc-interval wilson-95 --invariant non-negative`. Header comment records
  the executed numbers for seed 42 (measured at implementation time, not
  invented here) the way the DECLARED kernel records its 0.2094 calibration.
- **New negative fixture `examples/mc_pi_rejection_executed_broken.bld`:**
  the wrong-area estimator (factor 3.0) with the SAME counter columns. The
  proportion columns are untouched, so the interval executes and re-derives
  cleanly while the slack column blows the truth band: `FAIL_EXPECTED`. This
  seals the slice's central lesson into the corpus: an executed interval is
  not a truth claim, and the two claims fail independently.
- Corpus 24 -> 26, both new members with full mc + executed passthrough
  fields (`mc_executed: true` added to the corpus schema).
- **Emit-refusal tests (cli.rs, not corpus):** `--mc-executed` without the
  three declaration flags; with `clopper-pearson-95` (the unexecutable
  method, one of the two tasked negatives); declared samples that do not
  equal the witnessed final trials (kernel draws 1000, declares 2000);
  incoherent aggregates (a successes column that jumps by 2); an explicit
  `--columns` other than 3; degenerate `normal-approx-95` at a boundary
  proportion. Emit computes the same checks as verify stage A, fail closed:
  an incoherent EXECUTED block never gets sealed in the first place.
- **Self-test case 10** (the other tasked negative, the sealed interval that
  does not recompute): nudge sealed `interval_high`, reseal, expect
  `FIELD_CONTRACT_VIOLATION` via stage A. Pre-re-run by construction, so the
  self-test keeps needing no C compiler. A resealed unexecutable method name
  is rejected through the same stage and is covered by a verify unit test
  rather than an eleventh case (one case per gate FAMILY, the shipped
  self-test philosophy).

## 7. The riskiest decision, named

The fixed positional column convention plus the coherence checks as the
entire provenance story. A deliberately adversarial kernel can print a
synthetic counter stream that satisfies every coherence check while being
unrelated to its actual draws: EXECUTED hardens the arithmetic and the
denominator, not the indicator's provenance. The design's defense is not
mechanical but declarative, and that is the bet: the `estimator_semantics` /
`sample_independence` / `interval_coverage` entries in `not_claimed`, and the
does-not-prove prose, must carry the boundary. If a consumer reads EXECUTED
as "the interval is true," no achievable mechanism would have saved them; the
receipt at least states, in sealed machine-readable form, exactly which
reading is licensed. Second risk, smaller: the cross-platform exactness bet
on integer counters, mitigated by the integrality checks and the loud stage B
failure on the platform where it breaks.

## 8. Deferred, tracked

- `clopper-pearson-95` execution (needs a verified inverse incomplete beta;
  the sealed `successes` already carries the input it will need).
- A declared affine transform field (`scale`, e.g. 4.0 for pi) so the
  executed fields could be stated in the headline quantity's units. Killed
  for v1 to keep the sealed-knob count minimal: the truth band already
  witnesses the pi-level claim, and a declared scale on an executed number
  blurs the status boundary this slice exists to sharpen.
- Non-Bernoulli estimators (a mean over a continuous column needs sealed
  moments and loses the integer-exactness argument; a different design, not
  a vocabulary entry).
- Multiple mc estimates per receipt (one block per receipt is the v0 shape;
  chains already compose receipts).
