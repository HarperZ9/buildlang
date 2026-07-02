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
  value: approximate conservation, e.g. a symplectic integrator's energy). Any
  other value is an error reported **before** compiling.
- `--columns <N>` sets how many columns each row of the captured series holds
  (default `1`). `>= 2` is required by `--invariant relation` and rejected by
  the single-scalar invariants.
- `--metric <NAME>` labels the captured series (default `series`).
- `--problem <LABEL>` records a free-text problem label (optional).
- `--negative-fixture` marks that the invariant is *expected* to fail (see
  [Negative fixtures](#4-negative-fixtures)).

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
- `runtime_state`: `{ os, exit_code }`.
- `args`: the trailing program arguments the run was invoked with; `receipt verify` re-runs
  with exactly these.
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
  hazard, so a capability added later cannot silently widen the claim. `seed` is
  `NOT_APPLICABLE` in v0 (the language has no RNG builtin). These are the master plan's
  "input dataset" and "seed" receipt fields, filled by the typed-effect system rather than
  by assertion.
- `determinism`: `{ deterministic_modulo_args, grounds }`, derived the same fail-closed
  way from every nondeterminism source the language exposes (Clock, Environment,
  FileSystem, Network, Foreign, Gpu, and stdin reads); `Process` (exit) alone is safe, and
  the wall `Clock` breaks determinism without counting as a dataset. Verify re-derives all
  three capability-derived fields; edits that do not re-derive fail as
  `EFFECT_POLICY_DRIFT`.
- `numerical_method`: `{ description?, status }`, author-DECLARED via `--method` (buildc
  cannot derive scheme semantics from source and does not pretend to); an inconsistent
  status/description pair is rejected (`FIELD_CONTRACT_VIOLATION`).
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
it FAILs); `bounded` has `examples/bounded_oscillation.bld` (an undamped oscillator's `x^2`
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
of the band, so it FAILs). Run any negative kernel with `--negative-fixture` for a
`FAIL_EXPECTED` receipt.

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
| `FIELD_CONTRACT_VIOLATION` | a sealed field claims something the language version cannot express (a seed with no RNG builtin), is internally inconsistent (DECLARED method, no description), or resealed a non-canonical `invariant.tolerance` | 1 |
| `EFFECT_POLICY_DRIFT` | the sealed effect/capability facts, or the witnessed fields derived from them, do not re-derive from the source | 1 |
| `TOOL_UNAVAILABLE` | no C compiler available for the re-run | 4 |
| `REDERIVATION_FAILED` | the source could not be re-checked (missing file, check failure) | inner code |
| `RERUN_FAILED` | the program could not be re-compiled or re-run | inner code |
| `RERUN_EXIT_MISMATCH` | the re-run's process exit code differs from the sealed one (covers a crashing re-run) | 1 |
| `SOURCE_DIGEST_MISMATCH`, `INPUT_GRAPH_DIGEST_MISMATCH` | the source changed since sealing | 1 |
| `MEASUREMENT_COUNT_DRIFT`, `INVARIANT_STATUS_DRIFT`, `VIOLATION_COUNT_DRIFT`, `RECEIPT_STATUS_DRIFT` | the re-run disagrees with a stored verdict fact | 1 |
| `SEAL_MISMATCH` | the stored receipt body does not re-seal | 1 |
| `INVARIANT_NOT_HELD` | faithful receipt, but the recorded verdict is `FAIL_UNEXPECTED` or `UNVERIFIABLE` | 3 |

Receipts are loaded through a strict parser that rejects duplicate object keys at any
depth: with a permissive last-duplicate-wins reader, a document carrying two
`receipt_status` keys can show one value to a hasher and another to a reader, which is a
seal-forgery vector. Non-finite JSON literals (`NaN`, `Infinity`) are likewise rejected at
parse time.

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
  max principle), `energy-identity` (a quantitative energy-balance residual), and `relation`
  (cross-column agreement over `--columns N`) ship. Relations beyond per-row agreement (named
  physical identities across columns, header-named columns, more than pairwise comparison) are
  follow-ons.
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
