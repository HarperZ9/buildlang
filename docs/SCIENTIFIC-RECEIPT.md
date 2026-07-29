# Scientific-Runtime Receipt (`buildlang`)

> Status: **shipped 2026-07-01** (accountable compute, on top of Pillar B math syntax).
> Additive: `buildc run` without `--emit-receipt` is byte-identical to before, and the
> existing `buildlang-check-receipt/v1` verify path is unchanged.

`buildc run --emit-receipt` compiles and runs a `.bld` program, captures its numeric
stdout as a measurement series, checks a stated **invariant** over that series, and emits a
sealed, re-checkable JSON receipt (`buildlang-scientific-runtime-receipt/v0`). `buildc
receipt verify` re-runs the program, re-derives the source digest, and re-checks the
invariant verdict.

This is the reconcile applied to numerical compute: perceive (run the program), check
against an unauthored criterion (the invariant), carry a re-checkable proof (the receipt).

## Honest scope (read this first)

The receipt witnesses one thing and states it plainly: **the compiled program's observed
output series satisfies the stated invariant** (or, for a negative fixture, expectedly
violates it). That is the whole claim.

It does **not** prove the underlying PDE is solved correctly, and it does **not** claim a
new physical law. buildc checks a mathematical monotonicity property of a numeric series;
the physics lives in the `.bld` program, not in the compiler. Every receipt carries the
label `NOT_A_NEW_PHYSICAL_LAW` so a reader cannot mistake the artifact for more than it is.

Concretely, a PASS says: "buildc ran this exact source, captured this series, and the series
was monotone non-increasing within tolerance." It does not say the discretization is
convergent, the scheme is consistent, the parameters are physical, or the result matches any
reference solution. Those are the program author's responsibility, and the receipt makes no
statement about them.

v0 checks **one invariant over one captured series** per receipt (from a small invariant
family; see [The invariant family](#3-the-invariant-family)). Most invariants read one scalar
per step; the `relation` invariant reads `--columns N` values per row and checks a relation
across them. Richer multi-column analytics beyond row agreement are out of scope; see
[Deferred](#deferred-tracked-follow-ons).

## 1. Emitting a receipt (`buildc run --emit-receipt`)

```
buildc run examples/heat_equation_energy.bld --emit-receipt receipt.json \
    --problem 1d-heat-equation-energy
```

buildc compiles the program to C, runs it, and captures stdout. Every whitespace- or
newline-separated token that parses as a finite `f64` becomes one entry in the measurement
series (both plain-decimal `0.530827` and scientific `1.59908e+28` are accepted). buildc then
runs the invariant checker, builds the receipt, seals it, and writes it to the given path
(`-` writes the receipt to stdout).

**Non-finite values mean divergence.** If the program prints an `inf` or NaN value (in any
C-runtime spelling, including Windows forms like `-nan(ind)` and `1.#INF`), the run is
treated as numerically diverged: parsing stops at that token, only the finite prefix is
stored (so the receipt always serializes and re-verifies cleanly), the receipt is
`UNVERIFIABLE`, and it is labelled `NONFINITE_OBSERVED`. A diverged run is never a PASS,
even when its finite prefix happens to look monotone: the invariant could not be honestly
evaluated over a blown-up computation.

Trailing program arguments (`buildc run prog.bld --emit-receipt r.json -- <args>`) are
recorded in the receipt's `args` field, and `receipt verify` re-runs the program with
exactly those arguments, so an argv-parameterized kernel is re-derived under the same
conditions it was emitted under.

Flags on the `run` subcommand (all additive; absent `--emit-receipt`, none of them run):

- `--emit-receipt <PATH>` writes the receipt to `PATH` (`-` = stdout).
- `--invariant <NAME>` selects the invariant to check over the series:
  `energy-monotone` (the default; the observed scalar never increases beyond
  tolerance), `conservation` (the observed scalar stays within tolerance of its
  initial value), `bounded` (the observed scalar never rises above its initial
  value: the discrete maximum principle), `energy-identity` (each value is a
  per-step energy-balance residual that stays within tolerance of zero), or
  `relation` (the columns of each row must agree; requires `--columns >= 2`), or
  `conserved-band` (the scalar stays within a fixed error budget of its initial
  value: approximate conservation, e.g. a symplectic integrator's energy), or
  `non-negative` (the scalar never drops below zero: an absolute lower floor,
  e.g. a result-bearing slack that stays non-negative), or `cross-backend` (the
  same kernel's C anchor and secondary-lane columns must agree; requires
  `--cross-backend <TARGET>`, forces `--columns` to 2). Any other value is an
  error reported **before** compiling.
- `--columns <N>` sets how many columns each row of the captured series holds
  (default `1`). `>= 2` is required by `--invariant relation` and rejected by
  the single-scalar invariants.
- `--metric <NAME>` labels the captured series (default `series`).
- `--problem <LABEL>` records a free-text problem label (optional).
- `--negative-fixture` marks that the invariant is *expected* to fail (see
  [Negative fixtures](#4-negative-fixtures)).
- `--seed <N>` seeds the program's `random_f64()` stream (the `Random`
  capability). The pairing is enforced both ways, fail closed: a kernel that
  observes `Random` refuses to emit a receipt without a seed (an unseeded
  stream cannot be re-derived), and a kernel with no `Random` capability
  refuses a seed (nothing consumes it, so sealing it would fabricate a
  witnessed knob). The seed is sealed as `seed_value` and `receipt verify`
  re-runs the exact stream. This flag also works without `--emit-receipt`
  (plain `run` of a seeded kernel); a `Random`-using program run with no seed
  anywhere aborts at its first draw rather than inventing a default stream.
- `--mc-estimator <ID>`, `--mc-samples <N>`, `--mc-interval <METHOD>` declare the run a
  Monte Carlo estimate and seal the estimator's admission facts (id, sample-count
  denominator, interval method) as the receipt's `monte_carlo` block. All three declare
  together or not at all: an estimator whose interval method is undeclared is refused, and
  so is every other partial declaration (the claim is the interval, never the point). A
  zero sample count is refused as an unpriceable denominator, and the declaration requires
  a `Random`-observing program with a seed (an MC claim over a stream that cannot
  re-derive is worthless as evidence).
- `--mc-executed` upgrades the declaration to EXECUTED: the verifier RE-DERIVES the interval
  from raw sufficient-statistic columns the kernel prints (successes/trials counters beside
  the invariant scalar) instead of trusting it. Requires all three `--mc-*` flags together
  (`--mc-executed` alone is refused), and `--mc-estimator` must be `proportion` (v1's only
  executable estimator; DECLARED blocks may still use free text). `--columns` is forced to 3
  (an unset default of 1 is silently upgraded, any other explicit value is refused, the
  `--cross-backend` idiom). Fail-closed at emit: an incoherent EXECUTED block (a bad
  successes/trials stream, a witnessed denominator that disagrees with `--mc-samples`, an
  unexecutable `--mc-interval`, or a degenerate boundary proportion under
  `normal-approx-95`) is refused before the receipt is sealed, never sealed as if it were
  coherent.
- `--budget-steps <LIMIT>`, `--budget-consumed <N>` declare the run a budgeted search and
  seal the step ceiling, the consumption, and a DERIVED `exhausted` flag as the receipt's
  `budget` block. Both declare together or not at all (a result without its budget ceiling
  hides whether it stopped at the limit). A zero ceiling is refused, and consumption above
  the ceiling is refused as incoherent. Unlike `--mc-*`, this declaration is deterministic
  and does not require `Random` or a seed. A budgeted receipt additionally carries
  `NOT_PROVES_OPTIMALITY` in `labels` and `optimality` in `not_claimed`, and `--method` /
  `--problem` text containing `optimal` (case-insensitively) is refused: a budgeted search
  may report its incumbent, never a proof of optimality.
- `--budget-wall-seconds <LIMIT>` declares an OPTIONAL wall-clock ceiling on top of the step
  budget: a member of the budget declaration, not a freestanding knob, so it requires the
  `--budget-steps`/`--budget-consumed` pair and refuses without it. `LIMIT` must be a
  positive, finite number of seconds. When present, `budget.wall_exceeded` is set at emit
  from the SEALED measurement (`runtime_state.wall_seconds > wall_seconds_limit`), never
  from a later re-run: a slower verify machine must not flip a receipt's coherence.
- `--cross-backend <TARGET>` runs the kernel through a SECOND backend as well and seals a
  2-column cross-backend receipt: column 0 is the C anchor, column 1 the secondary lane. v0
  supports `rust` (the repo's designated validation lane) only. Requires `--invariant
  cross-backend` (and vice versa, a strict biconditional); refused with `--gpu`, with
  `--seed`, with any `--mc-*` flag, and on a `Random`-observing kernel (the Rust lane has no
  seeded PRNG builtin, so the streams could not agree). `--columns` is forced to 2 (an unset
  default of 1 is silently upgraded; any other value is refused). The secondary lane's stdout
  is captured and parsed but never echoed (only the primary's output is echoed, as always).

The program's own stdout is preserved: when the receipt is written to a file, the program's
output is echoed to real stdout byte-for-byte (identical to plain `run`); when the receipt is
written to stdout (`-`), the program echo is routed to stderr so stdout stays pure JSON.

Emitting the receipt is the success signal. `buildc run --emit-receipt` returns success once
the receipt is written, even if the invariant failed or the program exited nonzero; the
observed exit code and the PASS/FAIL verdict are recorded **in** the receipt, not in the
process exit code.

## 2. The schema (`buildlang-scientific-runtime-receipt/v0`) and its layers

The receipt is a single JSON object. Its layers, outermost meaning first:

- `schema`, `compiler` (= `"buildc"`), `compiler_version`, `language_version`.
- `source` (the path), `source_digest` (`{algorithm: "sha256", hex}` over the source bytes),
  `input_graph_digest` (sha256 over the resolved module graph).
- `build_state`: `{ target: "c", compiler_status: "compiled_and_executed", flags: [...],
  toolchain }`. The `toolchain` block is the pass-0122 `compiler_branch` contract: the
  resolved C compiler command, the first line of its version banner, a sha256 over the full
  version-probe output, the host `os/arch` target, a sha256 of the buildc binary that
  emitted the receipt, and a sha256 of the compiled program executable (hashed before it
  ran).
- `runtime_state`: `{ os, exit_code, wall_seconds? }`. `wall_seconds` is the receipt's first
  EXECUTED time fact: the witnessed run's wall-clock duration, measured with
  `std::time::Instant` around the primary program's run and sealed at emit. `receipt verify`
  re-measures its own re-run and REPORTS the fresh number beside the sealed one (in the human
  MATCH line and, when the receipt sealed one, under a `wall_seconds` object in `--json`
  output); there is no pass/fail on either number, since timing is environmental, exactly
  like raw stdout bytes. Optional-with-default so receipts sealed before this field existed
  parse and re-seal byte-identically.
- `args`: the trailing program arguments the run was invoked with; `receipt verify` re-runs
  with exactly these.
- `seed_value`: the RNG seed the run was invoked with (`--seed N`), present IFF the program
  observes the `Random` capability; `receipt verify` re-runs under exactly this seed, so a
  seeded stochastic run is as re-derivable as a deterministic one. Verify enforces the
  pairing against the RE-DERIVED capabilities (`FIELD_CONTRACT_VIOLATION` when a
  Random-using program's receipt seals no seed, or a seed rides on a program nothing in
  which draws), and a re-sealed seed swap is caught because the `seed` field's grounds no
  longer re-derive (`EFFECT_POLICY_DRIFT`).
- `problem`: `{ label }` from `--problem` (optional).
- `oracle`: `{ kind, name, status }`, the criterion the verdict is measured against. v0
  emits `kind: "declared_invariant"` with `status: "DECLARED"`: the named invariant IS the
  criterion, stated rather than derived from an executed reference. Verify rejects an
  oracle whose kind it cannot re-check or whose name does not bind to the invariant.
- `effect_policy`: `{ facts_digest, observed_capabilities, reads_stdin }`, the type/effect
  policy as WITNESSED facts: a sha256 over the canonical rendering of every function's
  declared effects and observed capabilities, plus the capability union and a `reads_stdin`
  flag. `Console` covers both stdout writes (safe) and stdin reads (an external input), so
  the capability NAME alone cannot decide the fields below; `reads_stdin` disambiguates.
  Verify re-derives all of these through the check pipeline and fails with
  `EFFECT_POLICY_DRIFT` on any disagreement.
- `input_dataset`, `seed`: `{ status, grounds }` fields whose values are honest evidence
  statements derived from the capability facts, FAIL CLOSED. A capability is a dataset
  hazard unless it provably cannot feed external data: FileSystem, Network, Environment,
  Foreign (extern C), Gpu, and `Console`-reading-stdin all fence the field
  (`POSSIBLE_UNWITNESSED`); a program with none of them PROVABLY consumed no external
  dataset (`NONE_WITNESSED`). Any capability this build does not recognise is treated as a
  hazard, so a capability added later cannot silently widen the claim. `seed` is a
  trichotomy: `NOT_APPLICABLE` when the program observes no `Random` capability (nothing
  draws, so there is no seed to record), `SEALED` when it does and the seed was supplied
  (the grounds name the value; `seed_value` carries it machine-readably), and `UNSEEDED`
  only as the re-derivation's honest answer for a receipt whose claims disagree with its
  capabilities (emit refuses to produce that state). These are the master plan's
  "input dataset" and "seed" receipt fields, filled by the typed-effect system rather than
  by assertion.
- `determinism`: `{ deterministic_modulo_args, grounds }`, derived the same fail-closed
  way from every nondeterminism source the language exposes (Clock, Environment,
  FileSystem, Network, Foreign, Gpu, stdin reads, and UNSEEDED Random); `Process` (exit)
  alone is safe, and the wall `Clock` breaks determinism without counting as a dataset.
  A SEEDED `Random` is the third honest state: it does not break the claim, and the
  grounds carry the qualification (deterministic given the sealed seed). Verify
  re-derives all three capability-derived fields; edits that do not re-derive fail as
  `EFFECT_POLICY_DRIFT`.
- **The `Model` capability and the propose/dispose admission rule.** `model_complete`
  (a dumb line-protocol call over TCP to `BUILD_MODEL_ENDPOINT`) carries its own `Model`
  capability, and the receipt layer refuses it outright: `buildc run --emit-receipt`
  aborts up front on a Model-observing program, and `receipt verify` refuses any receipt
  whose RE-DERIVED capabilities include `Model` (`CAPABILITY_INADMISSIBLE`), so a
  hand-built or tampered receipt cannot slip one through either. Models propose; oracles
  dispose. `Model` carries no explicit arm in the capability-derived fields above, so the
  fail-closed default treats it as a hazard for both `input_dataset` and `determinism`
  the same as any other capability this build does not specifically recognise -- moot in
  practice, since no receipt over a Model-observing program can exist to carry those
  fields. There is no corpus member and no `--self-test` case for `Model`: the refusal
  happens before a receipt can exist, so there is nothing for either to exercise.
- `numerical_method`: `{ description?, status }`, author-DECLARED via `--method` (buildc
  cannot derive scheme semantics from source and does not pretend to); an inconsistent
  status/description pair is rejected (`FIELD_CONTRACT_VIOLATION`).
- `monte_carlo` (optional): `{ estimator, samples, interval_method, status, estimate?,
  interval_low?, interval_high?, n_effective?, successes? }`, the MC admission block from
  the `--mc-*` flags. Author-declared, with hard shape contracts verify re-checks
  (`FIELD_CONTRACT_VIOLATION`): it rides only on a seeded `Random` run, `samples` is
  non-zero, `estimator` and `interval_method` are non-empty, and `status` is one of two
  values.
  - `DECLARED` (v0's original shape): the facts were stated, not independently executed;
    the five `estimate`/`interval_low`/`interval_high`/`n_effective`/`successes` fields are
    absent, and their presence on a DECLARED block is refused. `estimator`/`interval_method`
    stay free text forever (the shipped corpus uses `mean`/`normal-approx-95`).
  - `EXECUTED` (`--mc-executed`): the verifier RE-DERIVES the interval from raw
    sufficient-statistic columns the kernel prints (a three-column series,
    `<invariant_scalar> <successes> <trials>` per row), never from the kernel's own
    arithmetic, at TWO stages: Stage A over the SEALED `measurement.observed_values` before
    any re-run (so a tampered-and-resealed interval is a pure data contradiction, rejectable
    with no C compiler), and Stage B over the re-parsed re-run series (`MC_INTERVAL_DRIFT`
    on disagreement, since a Stage-A-clean receipt that no longer describes the run it names
    is a different kind of dishonesty than a tampered field). All five executed fields must
    be present, and are refused when absent. The executable estimator vocabulary (v1) is
    `proportion` only (the mean of Bernoulli indicators); the executable interval-method
    vocabulary is `normal-approx-95` and `wilson-95` (`normal-approx-95` additionally
    refused at the boundary proportion, successes = 0 or successes = trials, since a
    zero-width interval there overclaims precision; the message points at `wilson-95`).
    `clopper-pearson-95` is sealed-successes-only, not executable in v1 (it needs a verified
    inverse incomplete beta with no in-tree oracle), and is refused at both emit and verify.
    The aggregate columns must be structurally coherent as a cumulative Bernoulli count
    (every value an exact integer below 2^53, `trials` incrementing by exactly 1 across
    consecutive rows after the free first-row value, `successes` non-decreasing with
    increments in `{0, 1}`, `successes <= trials` on every row), and the final row's
    `trials` MUST equal the declared `samples`: the witnessed-denominator equality, the
    single biggest honesty gain of the EXECUTED shape. Float fields are compared within a
    pinned absolute tolerance (`1e-12`) in both stages. An EXECUTED block additionally adds
    three `not_claimed` entries -- `sample_independence`, `interval_coverage`,
    `estimator_semantics` -- present if and only if `status == "EXECUTED"` (the
    `NOT_PROVES_OPTIMALITY`/`optimality` pairing idiom): EXECUTED hardens the interval
    arithmetic and the denominator, but it cannot and does not harden that the draws are
    independent, that the named confidence level covers the true value, or that the
    indicator counts what the author says it counts.

  This is the admission rule for the weakest-promise mode: deterministic work needs only a
  hash, but an MC number without its denominator, seed, and interval method is
  unpriceable, so the receipt refuses to exist without them. DECLARED claims
  REPRODUCIBILITY and declaration discipline, never correctness of the interval; EXECUTED
  hardens the interval arithmetic and the witnessed denominator, never the estimator's
  semantics or independence.
- `budget` (optional): `{ steps_limit, steps_consumed, exhausted, status, wall_seconds_limit?,
  wall_exceeded? }`, the budgeted-search admission block from the `--budget-*` flags.
  Author-declared like `monte_carlo`, but with hard shape contracts verify re-checks
  (`FIELD_CONTRACT_VIOLATION`): `steps_limit` is non-zero, `steps_consumed` never exceeds it,
  `exhausted` is DERIVED (`steps_consumed == steps_limit`, never hand-set), and `status` is
  `DECLARED`. The label pairing is checked for EVERY receipt: `labels` must contain
  `NOT_PROVES_OPTIMALITY` and `not_claimed` must contain `optimality` if and only if the
  receipt carries a `budget` block. The claim-language rule keeps the free text honest: on a
  budgeted receipt, `problem.label` or `numerical_method.description` containing `optimal`
  (case-insensitively) is refused, because a budgeted search reports its incumbent, never a
  proof of optimality. A result without its budget ceiling hides whether it stopped at the
  limit, so the receipt refuses to exist without it. `wall_seconds_limit` (from
  `--budget-wall-seconds`) is the OPTIONAL declared wall-clock ceiling in seconds, present
  IFF one was declared; when present it must be positive and finite, and the receipt must
  carry a sealed `runtime_state.wall_seconds` to derive against. `wall_exceeded` is present
  IFF `wall_seconds_limit` is, and is DERIVED from the two SEALED numbers only
  (`runtime_state.wall_seconds > wall_seconds_limit`), both at emit and at re-check: verify
  never substitutes its own re-measured time into this comparison, because a slower verify
  machine must not flip a receipt's coherence.
- `cross_backend` (optional): `{ secondary_target, secondary_toolchain_version,
  secondary_toolchain_digest, secondary_executable_digest, secondary_raw_stdout_digest,
  secondary_exit_code, status }`, the cross-backend admission block from `--cross-backend
  <TARGET>`. Present IFF the invariant is `cross_backend_columns_agree` (a strict
  biconditional verify re-checks, `FIELD_CONTRACT_VIOLATION` on either side alone).
  `secondary_target` must be `rust` (v0). Unlike `monte_carlo`/`budget`, `status` is
  `EXECUTED`, not `DECLARED`: this block witnesses a run that ACTUALLY HAPPENED, and verify
  RE-EXECUTES both lanes rather than trusting the declaration. The three secondary digests
  must be well-formed sha256 (`DIGEST_MALFORMED` otherwise) and, like the primary's, are
  REPORTED as reproduced at verify (`secondary_raw_stdout_reproduced`,
  `secondary_executable_reproduced`), never required. Verify also re-probes the local rustc
  and compares it against the sealed `secondary_toolchain_digest`, mirroring the primary C
  lane's `toolchain_matched`: `secondary_toolchain_matched` is `false` on drift, WARNS
  (never fails; cross-toolchain re-verification stays legitimate by design) so a `RUSTC`
  override at verify time is visible rather than silently discarded. rustc absent at verify
  time exits 4 (`RERUN_FAILED`, TOOL_UNAVAILABLE semantics for the secondary lane; see the
  failure-class table below).
- `measurement`: `{ metric, observed_values: [f64], count, raw_stdout_digest,
  series_extraction_policy, units? }`. `raw_stdout_digest` seals the EXACT captured stdout
  bytes (the parse into `observed_values` is a lossy transform, so byte drift stays
  distinguishable from semantic drift); `series_extraction_policy` is the versioned parse
  discipline, hard-checked at verify.
- `invariant`: the checked criterion and its verdict (see below).
- `telemetry_branch`, `lineage_branch`: `{ status: "UNAVAILABLE_FENCED" }`. The pass-0122
  contract names these branches; buildc does not produce them and says so in-band rather
  than omitting the fields (absence of evidence is witnessed, never implied).
- `negative_fixture`: whether `--negative-fixture` was set.
- `not_claimed`: the machine-readable claims boundary (the honest-scope section as sealed
  data); must include `"physical_law"` or verify rejects the receipt outright.
- `diverged`: whether the run produced a non-finite value (sealed in-band, not just as a
  label, because verify's re-check rules branch on it; see section 6).
- `labels`: always includes `"NOT_A_NEW_PHYSICAL_LAW"`; adds `"NEGATIVE_FIXTURE"` when the
  fixture flag is set and `"NONFINITE_OBSERVED"` when the run diverged.
- `receipt_status`: `PASS` | `FAIL_EXPECTED` | `FAIL_UNEXPECTED` | `UNVERIFIABLE`.
- `seal`: `{algorithm: "sha256", hex}` over the canonical receipt with the seal hex blanked.
- `provenance`: `{ research_source_hash }`, a reference to the Telos research (see
  [Provenance](#provenance)); recorded for lineage only, never matched byte-wise.

### The `receipt_status` rule

The `invariant.status` (`PASS`/`FAIL`) is the raw verdict over the observed series. The
`receipt_status` layers the negative-fixture and unverifiable interpretation on top:

| condition | `receipt_status` |
|---|---|
| invariant PASS | `PASS` |
| invariant FAIL, `--negative-fixture` set | `FAIL_EXPECTED` |
| invariant FAIL, no `--negative-fixture` | `FAIL_UNEXPECTED` |
| empty or unparseable series | `UNVERIFIABLE` |
| non-finite value observed (diverged) | `UNVERIFIABLE` + `NONFINITE_OBSERVED` label |

### The seal

The seal is a SHA-256 over the canonical JSON of the whole receipt with `seal.hex` blanked
to the empty string. It is deterministic (serde preserves field order) and tamper-evident:
changing any field changes the seal. `receipt verify` re-derives it and compares. The seal
is buildc's own deterministic hash of its canonical form; integrity for the *numeric* verdict
comes from the re-run (below), not from trusting the stored series byte-for-byte.

## 3. The invariant family

Each invariant reduces the observed series to a **violation count**, and the verdict rule is
uniform across the family: **PASS** iff `violation_count == 0` **and** the series has at least
two points (a single point or an empty series cannot witness anything). The `observed` block
records `violation_count`, `first_violation_step` (when any), `initial_value`, and
`final_value`. The tolerance is a **fixed property of the invariant**, not an author knob:
verify re-checks the sealed `invariant.tolerance` against the canonical value for the named
invariant and rejects a receipt that resealed a different one (`FIELD_CONTRACT_VIOLATION`), so
a receipt cannot weaken its own check.

| `--invariant` | sealed name | a step is a violation when | tolerance |
|---|---|---|---|
| `energy-monotone` | `energy_monotone_nonincreasing` | `s[k+1] > s[k] + tol` (energy rose) | `1e-12` |
| `conservation` | `conserved_quantity_constant` | `abs(s[k] - s[0]) > tol` (drifted from the initial value) | `1e-9` |
| `bounded` | `bounded_by_initial_maximum` | `s[k] > s[0] + tol` (rose above the initial value) | `1e-9` |
| `energy-identity` | `energy_identity_residual` | `abs(s[k]) > tol` (energy-balance residual is not zero) | `1e-9` |
| `relation` (`--columns N>=2`) | `relation_columns_agree` | a row's columns differ by more than `tol` (the verifier compares them) | `1e-9` |
| `conserved-band` | `conserved_within_band` | `abs(s[k] - s[0]) > tol` (left a fixed error budget of the initial value) | `5e-3` |
| `non-negative` | `non_negative` | `s[k] < -tol` (dropped below zero) | `1e-9` |
| `cross-backend` (`--cross-backend <TARGET>`, `--columns` forced to 2) | `cross_backend_columns_agree` | the C-anchor and secondary-lane columns of a row differ by more than `tol` (the same evaluator as `relation`) | `1e-5` |

An EXECUTED `monte_carlo` receipt (`--mc-executed`, `--columns` forced to 3) takes any
single-scalar invariant name above (never `relation`/`cross-backend`: their columns already
mean something else, and `column_count_matches_invariant` refuses the pairing). The row is
`<invariant_scalar> <successes> <trials>`; `evaluate_measurement` DE-INTERLEAVES it,
evaluating the named invariant over column 0 only, with columns 1-2 left for
`compute_mc_executed`'s separate coherence and interval re-derivation. The effective
observation count for the "at least two points" verdict rule is the ROW count, mirroring
how `relation` counts rows rather than raw tokens. A ragged three-column series (length not
a multiple of 3) yields zero rows, the same "cannot witness" treatment a ragged `relation`
series gets.

The `conservation` and `bounded` references are both `s[0]` (the initial value), not the mean,
so a re-run that reproduces a different-length prefix cannot shift the reference. The checks
are genuinely distinct: `conservation` fences BOTH sides of `s[0]`, `bounded` fences only the
UPPER side (the discrete maximum principle: the quantity may decay freely but never overshoot
its start), and `energy-monotone` forbids any step-wise rise. A series that dips and returns to
its initial value PASSes `bounded` while FAILing both of the others. `energy-identity` is the
odd one out: its reference is **zero** (an absolute bound), so **every** step is checked
including step 0, and its series is not a physical trajectory but a per-step energy-balance
*residual* that a faithful scheme keeps at roundoff. The same series `[0.1, 0, 0]` gives three
different verdicts across the family, which is exactly why they are separate invariants. The
looser `1e-9` tolerance (vs the `1e-12` monotone bound) reflects that a genuinely conserved,
bounded, or balanced discrete quantity still accumulates roundoff over many steps, while a real
leak, overshoot, or dropped balance term drifts by an amount the bound still catches decisively.

`conserved-band` is APPROXIMATE conservation: the quantity must stay within a fixed error
BUDGET (`5e-3`) of its initial value, forever. It reuses conservation's two-sided evaluator with
a looser, calibrated tolerance, so it accepts a quantity that is only approximately conserved
while still rejecting one that drifts away. Its motivating case is symplectic integration: the
reference kernel (`examples/symplectic_oscillator.bld`) is a leapfrog / velocity-Verlet harmonic
oscillator whose energy `H = 0.5*(p^2 + q^2)` oscillates in a measured ~1.25e-3 band around
`H_0` forever, with no secular drift, so `5e-3` clears it ~4x; the negative fixture
(`examples/euler_oscillator.bld`) is explicit Euler, whose energy grows by `(1 + dt^2)` per step
and leaves the band within two steps. Starting mid-oscillation (`q = p = 1`), the symplectic
energy rises slightly ABOVE `H_0` and dips below, so the same series FAILs both `conservation`
(it deviates beyond roundoff) and `bounded` (it rises above the start); only `conserved-band`
accepts it, which is exactly why it is a separate invariant. The tolerance is an ABSOLUTE budget
(like the whole family): a kernel must be resolved, and scaled, to fit it.

`non-negative` is the lower-side companion to `bounded` and the family's first **algorithmic**
member. Where `bounded` fences the upper side against `s[0]`, `non-negative` fences the lower
side against an absolute floor of zero, allowing arbitrarily large positive values (a series
`[1, 5, 100, 0]` PASSes `non-negative` while FAILing `bounded`, `conservation`, and
`energy-monotone`). Its motivating use is a RESULT-BEARING kernel: a program runs an algorithm,
measures a cost, and prints the SLACK (a proven bound minus the measured cost); the receipt
then witnesses that the slack never goes negative, i.e. the algorithm never exceeds its bound.
The reference kernel (`examples/search_bound_binary.bld`) runs a binary search on 1024 elements
(worst case `floor(log2 n) + 1 = 11` probes) and prints `11 - probes` per lookup, which stays
non-negative, so it PASSes; the negative fixture (`examples/search_bound_linear.bld`) uses a
linear scan (up to 1024 probes), so the slack drops to about `-1013` and it FAILs. This carries
the receipt beyond physical simulation to witnessing a computation's measured complexity.

`relation` is the family's first **cross-column** invariant, and the first whose check the
VERIFIER computes rather than trusting a residual the kernel printed. With `--columns N` the
captured token stream is read row-major as `N` columns per row; the relation holds when every
row's columns agree within tolerance. Because the kernel only prints the raw columns (for
example two independent computations of the same quantity), it cannot conceal a divergence by
computing the agreement itself, the way a single-column residual invariant lets it. The
reference kernel (`examples/relation_double_angle.bld`) prints `sin(2t)` two ways, directly and
via the double-angle identity `2*sin(t)*cos(t)`, which agree to roundoff (PASS); the negative
fixture (`examples/relation_double_angle_broken.bld`) drops the factor of 2, so the columns
differ by `abs(col0)/2` and it FAILs. `count` stays the total token count (`N * rows`), so a
re-run's token drift is caught independently of the column structure, while the "at least two
observations" verdict rule counts ROWS.

`cross-backend` is `relation` applied across BACKENDS instead of across formulas: it reuses
`relation_columns_agree` unchanged (only the tolerance and the sealed provenance differ), with
column 0 the C anchor and column 1 the secondary lane (`--cross-backend rust`, v0's only
supported value, the repo's designated validation lane). Unlike every other member, the block
that carries its provenance (`cross_backend`) is EXECUTED, not DECLARED: verify re-runs BOTH
lanes rather than trusting a declaration. **Tolerance calibration.** A probe on this machine
(2026-07-28) built the same scalar f64 recurrence through both backends and found the computed
doubles IDENTICAL, but the printed text differs: the C runtime prints `%g` (6 significant
digits) while the Rust lane prints shortest-roundtrip, so two bit-identical doubles can print up
to ~5e-7 apart on O(1) values (`0.829` vs `0.8290000000000001`). `relation`'s `1e-9` would
therefore reject faithful agreement on formatting alone, so `cross-backend` uses a dedicated
`1e-5`, clearing that display floor by ~20x while still catching a genuine divergence (a dropped
term, a different formula, a miscompiled kernel), which is O(1) and caught decisively. The
reference kernel (`examples/decay_cross_backend.bld`) steps `x = x*0.9 + 0.01` for 40 iterations
under `--cross-backend rust --invariant cross-backend`, PASSing because the two backends compute
(and, up to the print-format floor, report) the same trajectory. **There is deliberately no
negative-fixture partner.** Every other family member ships a paired kernel that PASSes and one
that FAILs, but an honest deterministic kernel that computes DIFFERENT values on two backends
cannot exist by construction here: that impossibility (two faithful compilations of the same
source agree) is exactly what the invariant witnesses. The can-it-fail evidence instead lives at
the evaluator level (a unit test asserts a genuine divergence FAILs), in the refusal gates (an
unsupported target, a Random-observing kernel, a length mismatch between the two re-parsed
series), and in self-test case 9 (the invariant/block biconditional).

`energy-identity` is the family's first **quantitative** invariant. The 1-D heat equation's
continuous energy law `d/dt integral(u^2) = -2*alpha*integral(u_x^2)` has an exact discrete
analogue for the FTCS scheme: `E_next - E = -2*r*Du2 + r**2 * Lu2`, where `E = sum_i u_i^2`,
`Du2 = sum_i (u_{i+1}-u_i)^2`, and `Lu2 = sum_i (Lu_i)^2`. The reference kernel
(`examples/energy_identity.bld`) prints the per-step residual `(E_next - E) + 2*r*Du2 -
r**2 * Lu2`, which is zero to roundoff (measured max ~2e-14), so it PASSes; the negative fixture
(`examples/energy_identity_broken.bld`) drops the `r**2 * Lu2` correction, leaving an `O(r^2)`
residual (~1e-5) that FAILs from step 0. The tolerance sits ~5 orders above the faithful
roundoff and ~4 orders below the broken residual.

Checking a *violation-count verdict* rather than exact float values is deliberate: the verdict
is robust to platform float differences and codegen reassociation, so a receipt emitted on one
machine re-verifies on another even though the exact printed floats may differ in the last
bits.

## 4. Negative fixtures

A negative fixture is a program whose invariant is *expected* to fail; it proves the checker
actually catches violations. Run it with `--negative-fixture`:

```
buildc run examples/heat_equation_energy_unstable.bld --emit-receipt - --negative-fixture
```

The unstable kernel's energy grows, so `invariant.status` is `FAIL`. With
`--negative-fixture` the `receipt_status` is `FAIL_EXPECTED` and the receipt is additionally
labelled `NEGATIVE_FIXTURE`. **Without** the flag the same failing run is `FAIL_UNEXPECTED`,
because an unexpected invariant violation is a genuine red flag, not a demo of the checker.

Every invariant ships with a paired positive/negative kernel: `energy-monotone` has the stable
and unstable heat kernels above; `conservation` has `examples/conservation_rotation.bld` (a
rotation preserves the squared radius `r^2 = x^2 + y^2` to roundoff, so it PASSes) and
`examples/conservation_decay.bld` (a lossy scheme leaks 0.5% per step, so `r^2`/`q` drifts and
it FAILs), and the same invariant applied to a REACTION NETWORK in
`examples/reaction_atom_balance.bld` (the reversible reaction `A + B <=> C` conserves the atom
count `[A] + [C]` to roundoff as it proceeds, so it PASSes) with
`examples/reaction_atom_balance_broken.bld` (a stoichiometry bug that produces two `C` per event
drifts the balance and FAILs), and the same invariant applied to a QUANTUM STATE in
`examples/born_rule_normalization.bld` (a single qubit starting from the equal superposition
`(|0> + |1>)/sqrt(2)` and evolved by a unitary X-rotation keeps its total Born probability
`sum |psi_i|^2 = 1` to roundoff, so it PASSes; the gate drives both amplitudes' imaginary parts
nonzero, so they are genuinely complex, whereas conservation_rotation.bld is a purely real
geometric rotation) with
`examples/born_rule_leaky.bld` (a non-unitary gain inflates the probability, so `conservation`
FAILs and the receipt catches a gate that breaks the probability-conservation law unitarity
guarantees); this is the roundoff-crisp shadow of the Born rule, while the deeper Carcassi and
Aidala entropy equivalence (AoP Brief 003) is information-theoretic and out of scope for v0;
`bounded` has `examples/bounded_oscillation.bld` (an undamped oscillator's `x^2`
dips to 0 and returns to its initial `1.0` without ever exceeding it, so it PASSes) and
`examples/bounded_overshoot.bld` (an explicit-Euler oscillator injects energy, so `E = x^2 +
v^2` grows past its initial value and it FAILs); `energy-identity` has
`examples/energy_identity.bld` (the FTCS kernel computes the exact discrete energy balance, so
its residual is roundoff and it PASSes) and `examples/energy_identity_broken.bld` (the same
kernel with the `r**2 * Lu2` correction dropped, so its residual is O(r^2) and it FAILs);
`relation` has `examples/relation_double_angle.bld` (`sin(2t)` computed two ways, which agree,
so it PASSes) and `examples/relation_double_angle_broken.bld` (column 1 drops the factor of 2,
so the two columns disagree and it FAILs); `conserved-band` has
`examples/symplectic_oscillator.bld` (a leapfrog oscillator whose energy stays in an O(dt^2)
band, so it PASSes) and `examples/euler_oscillator.bld` (explicit Euler, whose energy drifts out
of the band, so it FAILs); `non-negative` has `examples/search_bound_binary.bld` (binary
search's probe slack stays non-negative, so it PASSes) and `examples/search_bound_linear.bld`
(a linear scan exceeds the bound, so the slack goes negative and it FAILs), and a DATA-STRUCTURE
instance in `examples/funnel_probe.bld` (funnel hashing, arXiv 2501.02305: a leveled
open-addressing scheme whose worst-case probe count stays under a calibrated bound of 20 at 75%
load, its measured worst being 14 probes, so it PASSes) with `examples/funnel_probe_linear.bld`
(naive single-level linear probing on the same keys clusters to 85 probes and exceeds the bound,
so it FAILs; this is a faithful-in-spirit funnel that exhibits the sub-linear worst-case probe
bound, not a bit-exact reproduction of the paper's optimal constant), and a SEEDED-STOCHASTIC
instance in `examples/random_walk_bound.bld` (a 200-step random walk driven by `random_f64()`
under `--seed 42` can never be farther than its step count from the origin, so the slack
against that worst-case envelope stays non-negative for every seed and it PASSes) with
`examples/random_walk_bound_broken.bld` (the same walk claiming the tighter envelope
`|position| <= 7`, which a 200-step walk leaves for essentially any seed, so it FAILs; the
sealed seed makes both stochastic verdicts exactly re-derivable), and a MONTE CARLO
instance in `examples/mc_pi_rejection.bld` (pi by rejection sampling under the full
`--mc-*` declaration: from a burn-in of 200 samples the running estimate stays within a
band of 0.3 of pi, calibrated against the corpus seed 42 whose measured worst error is
0.2094, so the slack stays non-negative and it PASSes) with
`examples/mc_pi_rejection_broken.bld` (a wrong-area estimator multiplies by 3.0 instead of
4.0 and converges to 3*pi/4, a systematic bias of ~0.785 no sampling repairs, so it blows
through the band and FAILs: the harness catches a biased estimator, not just an unlucky
one), and a BUDGETED-SEARCH instance in `examples/greedy_change_budget.bld` (greedy coin
change over denominations {4, 3, 1}, a genuine heuristic: amount 6 takes greedy's 3 coins
(4+1+1) where 3+3 is the optimal 2, so the receipt carries `NOT_PROVES_OPTIMALITY`. Under
the full `--budget-*` declaration, the measured worst coin count over amounts 1..60 is 16
(at amount 58); the per-amount step_budget is calibrated to 23, a margin of ~1.4x, so the
slack stays non-negative and it PASSes; the run declares `--budget-steps 60000
--budget-consumed 495`, 495 being the real measured total coins used across all 60
amounts) with `examples/greedy_change_budget_broken.bld` (the same loop under
step_budget 14, the measured worst minus 2, so the worst amounts blow through it and it
FAILs: the harness catches a search that overran its own declared budget). Run any
negative kernel with `--negative-fixture` for a `FAIL_EXPECTED` receipt.

## 5. The heat-equation kernel example

`examples/heat_equation_energy.bld` is the flagship program. It simulates the 1-D heat
equation `u_t = alpha * u_xx` on `[0, 1]` with fixed zero endpoints, using the explicit
forward-time centered-space (FTCS) finite-difference scheme:

```
u_next[i] = u[i] + r * (u[i-1] - 2*u[i] + u[i+1]),   r = alpha*dt/dx**2
```

on a 129-point grid over 400 timesteps. Each step it prints the discrete energy
`E_k = dx * sum_i u_i^2` (computed as `dx * linalg::vec_dot(u, u)`), one value per line.

FTCS is stable when `r <= 0.5`. The kernel uses `r = 0.45` (stable), and under a stable `r`
the discrete energy is monotone non-increasing: a discrete analogue of the continuous energy
dissipation `d/dt integral(u^2) = -2*alpha*integral(u_x^2)`. The companion
`examples/heat_equation_energy_unstable.bld` uses `r = 0.55` (unstable) and the energy grows
instead, which is the negative fixture.

The kernel dogfoods the shipped math syntax (see `docs/MATH-SYNTAX.md`): the dynamic
`Vec<f64>` builtins, the `linalg::vec_dot` reduction, and the `**` power operator (`dx ** 2`).
It uses runtime `Vec` loops rather than the `.+ .- .* ./` broadcasting operators, because the
129-point stencil is a runtime-sized `Vec`, not a fixed-N compile-time `Array` (broadcasting
would compile-time-unroll over a fixed length, the wrong vehicle here).

## 6. Verifying a receipt (`buildc receipt verify`)

```
buildc receipt verify receipt.json          # human output
buildc receipt verify receipt.json --json    # machine-readable report
```

`receipt verify` dispatches on the receipt's `schema`. For a scientific-runtime receipt it
**re-runs and re-checks** rather than trusting the stored numbers:

1. **Recompute the seal (integrity gate)** over the stored receipt body, right after the
   schema/compiler applicability check and BEFORE any sealed field is interpreted. An
   unsealed hand-edit to any field is therefore reported as tampering (`SEAL_MISMATCH`)
   rather than misreported as whichever field-level contradiction it happens to trip first;
   every field-level rejection below is thus known to concern a genuinely author-sealed
   value. (Genuine non-reproduction of a VALIDLY-sealed receipt is a separate matter, caught
   by the re-run checks below.)
2. **Re-derive the source digest** from the source referenced by the receipt (the same
   pipeline that produced the stored digests) and compare both the source and input-graph
   digests, plus the effect/capability policy. A change to the source file since sealing
   shows up here as a mismatch.
3. **Re-run the program with the receipt's recorded `args`**, re-parse the series, and
   **re-check the measurement count**: the re-run must produce exactly
   `measurement.count` values (for a non-diverged run the count is deterministic, unlike
   the exact floats), so an edited `observed_values` array of the wrong length is caught
   here. **Diverged runs are the exception**: there the finite-prefix length is the index
   of the first non-finite value, a platform-dependent quantity (a 1-ULP libm difference
   can shift the divergence step), so when the receipt records divergence AND the re-run
   also diverges, the count and increase-count checks are skipped and the reproduced
   divergence itself is the faithfulness signal. A recorded divergence that does NOT
   reproduce (or a divergence the receipt never recorded) fails as non-reproduction.
4. **Recompute the verdict** with the exact same status rule. The recomputed
   `invariant.status`, `violation_count`, and `receipt_status` must match the stored values;
   any drift is a verification failure with a clear `... drift: receipt X, re-run Y`
   diagnostic. This checks the *verdict*, not exact floats, so it is robust to platform
   float non-reproducibility (the same principle `buildc corpus verify` uses when it
   re-runs C stdout).

### Exit-code semantics (safe as a CI gate)

A receipt that passes all four checks is **faithful**: it reproduces. But a faithful
receipt that *records* a failure is not a pass, so the exit code reflects the verdict too:

| outcome | exit code |
|---|---|
| faithful, `PASS` or `FAIL_EXPECTED` | `0` (human output: `MATCH: ...`) |
| faithful, `FAIL_UNEXPECTED` or `UNVERIFIABLE` | `3` (human output: `FAIL: ... invariant did not hold`) |
| did not reproduce (digest, count, verdict, or seal drift) | `1` |
| no C compiler available for the re-run | `4` (`TOOL_UNAVAILABLE`, checked before any re-run attempt) |

Verify additionally REPORTS (never requires) three reproduction signals, in the human MATCH
line and as `--json` fields: `toolchain_matched` (the local C toolchain equals the sealed
one; a mismatch warns and marks any drift below as possibly environmental),
`raw_stdout_reproduced` (the re-run's exact stdout bytes match the sealed digest), and
`executable_reproduced` (the re-compiled binary matches the sealed digest; commonly false
even on the same machine, since C compilers embed timestamps, which is exactly why it is
reported rather than required). The verdict, not these bytes, is the re-checked quantity.

This makes `buildc receipt verify r.json && deploy` safe: it will not deploy on a receipt
that records an unexpected invariant violation or a diverged/unverifiable run. A negative
fixture reproducing its *expected* failure is a legitimate pass (the checker demonstrably
catches violations), so `FAIL_EXPECTED` exits `0`. With `--json`, the report carries
`"faithful"` and `"invariant_held"` fields alongside the verdict.

### Failure classes (stable within schema v0)

Every verification failure prints `failure_class: <CODE>` on stderr, and `--json` emits a
`{"status": "failed", "failure_class": ...}` report (schema-agnostic for load-stage
failures, where the document's schema could not be established). This lets negative
fixtures and CI pin the *specific* failure instead of accepting "anything failed":

| class | meaning | exit |
|---|---|---|
| `MALFORMED` | unreadable file, invalid JSON, duplicate object key, or fields that do not deserialize | 1 |
| `SCHEMA_UNSUPPORTED` | missing or unrecognized `schema` | 1 |
| `COMPILER_MISMATCH` | `compiler` is not `buildc` | 1 |
| `OVERCLAIM_BOUNDARY_MISSING` | `not_claimed` omits `physical_law` | 1 |
| `EXTRACTION_POLICY_MISMATCH` | the sealed series-extraction policy's version tag is not the one this verifier implements (prose after the tag is display text) | 1 |
| `DIGEST_MALFORMED` | a sealed digest field is not a real sha256 (64 hex chars); an absent hash cannot masquerade as witnessed provenance | 1 |
| `ORACLE_KIND_UNSUPPORTED`, `ORACLE_STATUS_UNSUPPORTED`, `ORACLE_BINDING_MISMATCH`, `INVARIANT_UNSUPPORTED` | the oracle/invariant block names a kind, status, or criterion this verifier does not implement; binding is pinned to the implementation, never to another sealed field | 1 |
| `FENCE_STATUS_UNEXPECTED` | a telemetry/lineage fence was edited to claim availability v0 does not produce | 1 |
| `FIELD_CONTRACT_VIOLATION` | a sealed field claims something the program cannot express (a `seed_value` when nothing observes `Random`, a Random-using program with no sealed seed, a `monte_carlo` block without a seeded Random run or with a zero/nameless denominator, estimator, or interval method, a status outside `DECLARED`/`EXECUTED`, a `budget` block with a zero ceiling, consumption above its ceiling, a hand-set `exhausted`, or a non-`DECLARED` status, a `budget.wall_seconds_limit` that is non-positive or non-finite, present without a sealed `runtime_state.wall_seconds`, or paired with a `wall_exceeded` that disagrees with the SEALED `wall_seconds > wall_seconds_limit` comparison, a `wall_exceeded` present without `wall_seconds_limit`, a `cross_backend` block present without the `cross_backend_columns_agree` invariant or that invariant without the block (the biconditional), a non-`rust` `cross_backend.secondary_target`, a non-`EXECUTED` `cross_backend.status`, a `cross_backend` block whose RE-DERIVED capabilities include `Random` (the Rust lane has no seeded PRNG, so the streams could not agree; this transitively excludes a `monte_carlo` block riding along too, since MC requires `Random`), or a `NOT_PROVES_OPTIMALITY`/`optimality` pairing or claim-language mismatch), is internally inconsistent (DECLARED method, no description), or resealed a non-canonical `invariant.tolerance`. The EXECUTED `monte_carlo` sub-cases: an executed field (`estimate`/`interval_low`/`interval_high`/`n_effective`/`successes`) present on a DECLARED block, or one of the five ABSENT on an EXECUTED block; an EXECUTED `estimator`/`interval_method` outside the v1 executable vocabulary (`proportion`; `normal-approx-95`, `wilson-95`); a `column_count` other than 3 on an EXECUTED block; an incoherent aggregate successes/trials stream (non-integer, a trials step other than +1, a successes step outside `{0, 1}`, `successes > trials`); a witnessed final `trials` that disagrees with the declared `samples` (the denominator is not witnessed); the Stage A recompute (over the sealed series) disagreeing with the sealed `estimate`/`interval_low`/`interval_high`/`n_effective`/`successes`; and the `sample_independence`/`interval_coverage`/`estimator_semantics` `not_claimed` triad biconditional | 1 |
| `EFFECT_POLICY_DRIFT` | the sealed effect/capability facts, or the witnessed fields derived from them, do not re-derive from the source | 1 |
| `CAPABILITY_INADMISSIBLE` | the RE-DERIVED capabilities include `Model`: a scientific receipt cannot witness a model-mediated run (models propose, oracles dispose) | 1 |
| `TOOL_UNAVAILABLE` | no C compiler available for the re-run | 4 |
| `REDERIVATION_FAILED` | the source could not be re-checked (missing file, check failure) | inner code |
| `RERUN_FAILED` | the program could not be re-compiled or re-run; for a cross-backend receipt, a missing `rustc` at verify time is TOOL_UNAVAILABLE semantics for the secondary lane, reported here with exit code 4 (matching how the primary C toolchain's absence is classed) | inner code (4 for a missing rustc) |
| `RERUN_EXIT_MISMATCH` | the re-run's process exit code differs from the sealed one (covers a crashing re-run) | 1 |
| `SOURCE_DIGEST_MISMATCH`, `INPUT_GRAPH_DIGEST_MISMATCH` | the source changed since sealing | 1 |
| `MEASUREMENT_COUNT_DRIFT`, `INVARIANT_STATUS_DRIFT`, `VIOLATION_COUNT_DRIFT`, `RECEIPT_STATUS_DRIFT` | the re-run disagrees with a stored verdict fact | 1 |
| `MC_INTERVAL_DRIFT` | an EXECUTED `monte_carlo` receipt's sealed interval fields do not match the Stage B recompute over the re-run series (Stage A already passed over the sealed series; this catches a receipt that stopped describing the run it names) | 1 |
| `SEAL_MISMATCH` | the stored receipt body does not re-seal | 1 |
| `INVARIANT_NOT_HELD` | faithful receipt, but the recorded verdict is `FAIL_UNEXPECTED` or `UNVERIFIABLE` | 3 |

Receipts are loaded through a strict parser that rejects duplicate object keys at any
depth: with a permissive last-duplicate-wins reader, a document carrying two
`receipt_status` keys can show one value to a hasher and another to a reader, which is a
seal-forgery vector. Non-finite JSON literals (`NaN`, `Infinity`) are likewise rejected at
parse time.

### Proving the verifier can FAIL (`--self-test`)

A failure taxonomy is only worth trusting if the verifier can actually reach each class. The
negative-fixture kernels close the can-it-FAIL gap on the invariants; `--self-test` closes the
same gap on the verifier:

```
buildc receipt verify receipt.json --self-test
```

Given a valid scientific-runtime receipt, it tampers several distinct sealed fields and asserts
that each tamper is rejected by the real verify path with its expected `failure_class`. Cases
that keep the body well-formed are re-sealed (so the tamper passes the integrity gate and reaches
the specific contract check under test); the seal-mismatch case is deliberately left unsealed.
The current ten cases exercise five separate arms of the taxonomy: `COMPILER_MISMATCH`
(foreign compiler tag), `SEAL_MISMATCH` (a witnessed value edited without re-sealing),
`MALFORMED` (a required field removed), `FIELD_CONTRACT_VIOLATION` (six times, through
different gates: a sealed tolerance loosened then re-sealed, the sealed `seed_value` flipped
against the program's capabilities then re-sealed, a `monte_carlo` block given a zero sample
denominator then re-sealed, a `budget` block given a `steps_consumed` above
`steps_limit` then re-sealed, the sealed `cross_backend` block swapped against the
invariant name -- removed if present, added with a syntactically valid shape if absent --
then re-sealed, and case 10: an EXECUTED `monte_carlo` block's sealed `interval_high`
nudged against its Stage A recompute -- the receipt's own block if it already carries one
(e.g. a receipt emitted from `mc_pi_rejection_executed.bld`), else a syntactically valid
one is added, mirroring case 9's `cross_backend` fallback -- then re-sealed), and
`INVARIANT_UNSUPPORTED` (an unknown invariant name, re-sealed). Every case is rejected
before any program re-run, so `--self-test` needs no C compiler (and no rustc): the
seed-pairing, MC, budget, cross-backend, and EXECUTED-interval cases are rejected at the
source re-derivation stage or the field-contract gate (Stage A, in the EXECUTED case),
so they (alone) need the receipt's source file readable, exactly as `buildc check` would.
It exits 0 only if every tamper produced its expected class, and prints `self-test: N/N
tampers rejected with the expected failure_class`. Case 10 is robust to WHICHEVER Stage A
gate fires first (seed pairing, field presence, or the interval-mismatch check it targets)
on an arbitrary pristine input: every pre-re-run EXECUTED violation this slice adds
reports `FIELD_CONTRACT_VIOLATION`, the same class case 7's zero-denominator tamper
already relies on regardless of which specific MC gate fires first.

### Chaining receipts (`receipt chain`)

A multi-stage computation produces several receipts. A receipt chain binds them into one
ordered, tamper-evident bundle without changing the receipt schema:

```
buildc receipt chain build stage1.json stage2.json stage3.json -o chain.json
buildc receipt chain verify chain.json
```

`chain build` records each member's `seal.hex` in order and computes a chain seal over the
ordered list of member seals. `chain verify` then (1) recomputes the chain seal and compares it,
(2) checks each member receipt's current seal against the seal pinned in the chain, and (3)
re-verifies each member receipt through the real `receipt verify` path. Each break has a stable
`failure_class`: `CHAIN_SEAL_MISMATCH` (a member was reordered, added, or dropped),
`CHAIN_LINK_TAMPERED` (a member file was substituted or its seal edited), `CHAIN_LINK_MISSING` (a
member file is gone), and `CHAIN_LINK_UNVERIFIED` (a member no longer re-verifies). Because step 3
re-runs each member, `chain verify` needs the C toolchain and the member sources, exactly like
`receipt verify`.

For a worked walkthrough that chains one receipt per computation mode (deterministic,
probabilistic-exact, stochastic, Monte Carlo, heuristic, plus the cross-backend bonus), see
[FIVE-MODES-TOUR.md](FIVE-MODES-TOUR.md).

### The example corpus (`receipt corpus`)

The example kernels come in positive/negative pairs, each declared to PASS or to FAIL_EXPECTED
under a named invariant. `examples/scientific-corpus.json` records that ground truth for all
fourteen pairs plus the cross-backend singleton (a member whose kernel draws from
`random_f64()` also declares its `seed`, which the runner passes as `--seed`, an MC member
declares `mc_estimator` / `mc_samples` / `mc_interval`, passed through the same way, an
EXECUTED MC member additionally declares `mc_executed: true`, passed through as
`--mc-executed`, a budgeted member declares `budget_steps` / `budget_consumed`, passed
through as `--budget-steps` / `--budget-consumed`, and the cross-backend member declares
`cross_backend`, passed through as `--cross-backend`; a Random member with no declared seed,
or a partial MC or budget declaration, fails the corpus loudly, because emit refuses it),
and one command checks reality against it:

```
buildc receipt corpus examples/scientific-corpus.json
```

For every member it emits the receipt under the declared invariant and flags, asserts the emitted
`receipt_status` equals the declared one, and re-verifies the receipt through the real `receipt
verify` path. It exits 0 only when every member classifies and re-verifies exactly as declared,
printing `corpus: N/N members classified and re-verified as declared`. A kernel whose verdict
silently changes (a PASS that starts failing, or a negative fixture that stops failing) breaks the
corpus with a `declared X, emitted Y` line and a non-zero exit, so the corpus is a gate that can
fail, not a rubber stamp. The manifest is author-written input and is not sealed; its declared
statuses are the ground truth the command checks against.

The `cross-backend` member (`examples/decay_cross_backend.bld`) is deliberately a SINGLETON,
with no FAIL_EXPECTED partner: every other invariant ships a paired kernel that PASSes and one
that FAILs, but an honest deterministic kernel that computes DIFFERENT values on two backends
cannot exist by construction (that impossibility is what the invariant witnesses), so there is
no negative fixture to pair it with. The can-it-fail evidence for this member instead lives in
the evaluator-level divergence unit test, the CLI refusal gates, and self-test case 9 (see
above).

### What the seal does and does not witness

The re-run re-derives the source digests, the measurement count, and the verdict triple.
The remaining descriptive fields (`observed_values` element bytes, `os`, `exit_code`,
`flags`, `labels`) are covered by the seal, which is an **unkeyed** SHA-256: it detects
accidental corruption, but anyone can recompute it after editing those fields. Integrity
for the claim that matters (the verdict over this exact source) comes from the re-run, not
from trusting stored bytes. Do not read the seal as cryptographic tamper-proofing of the
descriptive metadata.

**Version drift is a warning, not a failure.** If `compiler_version` or `language_version`
differs from the current build, verify prints a `Warning:` and continues. A scientific
receipt records a *numerical* verdict a later compiler build can still legitimately
reproduce, so a version bump alone is not tampering. (This differs from
`buildlang-check-receipt/v1`, which hard-pins versions because it replays version-sensitive
effect and capability facts. This receipt does not.)

### Receipt seals are not byte-reproducible (by design)

Two identical runs of the same kernel produce receipts with **different seals**. The receipt
seals `build_state.toolchain.program_executable_digest`, the SHA-256 of the compiled program
binary, and that binary is not reproducible across builds: the C compiler and linker embed
non-deterministic content (image timestamps, temp paths, link order), so the digest, and
therefore the seal, changes each build. This is the only field that varies between two otherwise
identical emits.

This is deliberate and consistent with the seal model above, not a defect. The executable digest
witnesses and tamper-seals the *exact* binary that produced the run, and it is required to be
present and well-formed (an absent or malformed digest fails `DIGEST_MALFORMED`, so "hash
unavailable" cannot masquerade as witnessed provenance). But verify never re-checks it for
*equality*: it re-runs the program and re-checks the *verdict*, and executable reproduction is
reported, never required (a fresh build with a different binary digest still verifies). So a
non-reproducible seal never breaks verification, exactly like the exact-float re-derivation
deferred below. Byte-reproducibility of the receipt artifact is a non-goal here; re-checkability
of the verdict is the guarantee.

One consequence is worth stating: because `receipt chain` pins each member's seal, a chain
identifies *specific* receipt artifacts. Re-emitting a member is a new artifact with a new seal,
so it requires rebuilding the chain. That is correct provenance semantics (the chain fixes the
exact receipts it was built over), not a limitation to work around.

If byte-reproducible receipts ever become a requirement, two paths (not taken here) exist: make
the build reproducible with deterministic compile and link flags so the executable digest is
stable, or stop sealing the executable digest and seal a reproducible proxy instead. Both
deliberately change the seal-everything, fail-closed model chosen here, so they are gated on an
explicit decision rather than adopted by default.

## 7. Exporting into Crucible/Telos (`buildc receipt export`)

```
buildc receipt export receipt.json -o measurement.json \
    --claim-id heat-energy-monotone --claim-sha256 <hex>
```

The bridge into the proof-packet system: exports the receipt as ONE Crucible
measurement row (`claim_id, claim_sha256, deviation, tolerance, method,
measured_at, evidence, recheck`) inside a versioned envelope
(`buildlang-crucible-measurement-export/v0`). The honesty discipline:

- **The receipt is re-verified first**, through the exact evaluation path
  `receipt verify` uses. A receipt that does not reproduce exports nothing
  (the exit codes propagate). Only faithfulness earns a measurement.
- **The deviation is derived from the fresh re-run**, never copied from stored
  values: the recomputed increase count for measurable verdicts, JSON `null`
  for UNVERIFIABLE (Crucible reads an unmeasurable deviation as UNVERIFIABLE,
  fail-closed). Failing receipts export their real count; the receipt_status
  travels in `evidence` so a thesis can frame an expected failure.
- **The `recheck` descriptor makes the row witnessed, not asserted**: it seals
  the replay oracle (`buildc.receipt.verify`), the hash of the exact receipt
  file, the source digest, the recorded args, the full replay command, and the
  expected verdict triple. An independent replayer can re-run buildc and
  rebuild the row; a measurement without such a descriptor is exactly the
  author-supplied pattern Crucible's MATCH-provenance gate exists to catch.

Claim binding (`--claim-id` / `--claim-sha256`) belongs to the thesis side;
when omitted the envelope carries a binding note, and Crucible fails closed
(UNVERIFIABLE) on an unbound measurement.

Three refinements the mapping enforces:

- **Expected failure is bound explicitly, never assumed.** Crucible's verdict
  is pure margin arithmetic; there is no thesis-side reframe for an expected
  failure. `--claim-expects-failure` (valid only for a negative-fixture
  receipt) makes the deviation claim-relative: a fixture that failed as
  predicted measures 0 (MATCH), one that unexpectedly passed measures 1
  (DRIFT against the failure-predicting claim).
- **Diverged receipts never seal a platform-dependent replay expectation.**
  A diverged run's increase count is prefix-derived and legitimately differs
  across toolchains (the verifier's own rule), so `recheck.expected.
  violation_count` is null and `recheck.diverged` is true: a replayer matches
  on receipt_status, not on a number that cannot reproduce. The expected exit
  code of the sealed replay command is also carried (0 faithful-held, 3
  faithful-not-held).
- **Reproduction signals ride outside `evidence`.** The witnessing re-run's
  `toolchain_matched` / `raw_stdout_reproduced` / `executable_reproduced`
  flags are a top-level `reproduction` object (auditable, but excluded from
  Crucible's evidence-stability comparison, since they legitimately differ
  per replay environment); the sealed-time stdout digest in evidence is
  labeled `sealed_raw_stdout` so it cannot be mistaken for the re-run's
  bytes.

Exports write atomically (temp file + rename), so a failed export never
destroys a previous good measurement. Exporting the check-receipt and corpus
surfaces are documented follow-ons of this bridge.

## Provenance

The receipt's `provenance.research_source_hash` references the Telos dogfood research (pass
0009/0010, `BuildScientificRuntimeReceipt/v1`, the heat-equation energy proof) that the
buildc feature is derived from. It is recorded for lineage only. buildc computes its **own**
deterministic seal over its **own** canonical receipt form; the referenced research hash is
never matched byte-wise and no claim is made that buildc reproduced the research artifact
byte-for-byte. The provenance link records where the idea came from, nothing stronger.

## Deferred (tracked follow-ons)

v0 checks one invariant over one scalar series, sealed and re-derivable. Explicitly out of
scope for v0:

- **Richer relations and analytics.** `energy-monotone`, `conservation`, `bounded` (a discrete
  max principle), `energy-identity` (a quantitative energy-balance residual), `relation`
  (cross-column agreement over `--columns N`), `conserved-band` (approximate conservation),
  `non-negative` (an absolute lower floor, used for a result-bearing complexity slack, shipped
  with both a binary-search bound and a funnel-hashing (arXiv 2501.02305) probe bound), and
  `cross-backend` (`relation`'s evaluator applied across the C anchor and a secondary lane,
  `--cross-backend rust` in v0) ship. Relations beyond per-row agreement (named physical
  identities across columns, header-named columns) are follow-ons. The Born-rule kernel ships
  the roundoff-crisp normalization-conservation form; the deeper Carcassi and Aidala entropy
  equivalence (AoP Brief 003) would need a calibrated tolerance or a seeded-RNG
  frequency-convergence demo and stays a follow-on. The GPU lane for `--cross-backend` is a
  later value, not a mechanism change.
- **The full 7-layer receipt richness.** The research schema carries more layers than buildc
  can honestly fill today; v0 fills the subset buildc actually derives.
- **Crucible-at-emit-time.** v0 checks the invariant and seals; it does not run a Crucible
  judgment pass at emit time.
- **In-place `Vec` update (`vec_set_f64`).** The kernel double-buffers a fresh `u_next` each
  step because `vec_set_f64` is not yet exposed; wiring it is a deferred optimization, not a
  correctness change.
- **Exact-float sealing.** The seal is a byte-exact hash of buildc's canonical JSON, and the
  on-disk receipt re-seals exactly after a read-back (buildc enables serde_json's
  `float_roundtrip` so parsing reproduces the serialized f64 bits). But the *verdict* is what
  re-verification checks, deliberately, so platform float differences in a fresh re-run do
  not break verify. An exact-float re-derivation guarantee for the re-run series itself is a
  separate, harder effort.
