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

v0 is deliberately **one invariant over one series**. See [Deferred](#deferred-tracked-follow-ons)
for what is out of scope.

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
- `--invariant <NAME>` selects the invariant. v0 supports only `energy-monotone` (the
  default). Any other value is an error reported **before** compiling.
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
- `build_state`: `{ target: "c", compiler_status: "compiled_and_executed", flags: [...] }`.
- `runtime_state`: `{ os, exit_code }`.
- `args`: the trailing program arguments the run was invoked with; `receipt verify` re-runs
  with exactly these.
- `problem`: `{ label }` from `--problem` (optional).
- `measurement`: `{ metric, observed_values: [f64], count, units? }`, the parsed stdout
  series (always finite values; see the non-finite rule in section 1).
- `invariant`: the checked criterion and its verdict (see below).
- `negative_fixture`: whether `--negative-fixture` was set.
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

## 3. The energy-monotone invariant

v0 ships one invariant, `energy_monotone_nonincreasing`: over the observed series
`s[0], s[1], ...`, count every step `k` where `s[k+1] > s[k] + tol` (tolerance `1e-12`). The
verdict is **PASS** iff `increase_count == 0` **and** the series has at least two points; a
single point or an empty series does not pass (there is no step to check). The `observed`
block records `increase_count`, `first_increase_step` (when any), `initial_value`, and
`final_value`.

The tolerance absorbs floating-point jitter (a step that rises by less than `1e-12` is not
counted as an increase) without absorbing real growth. Checking a *monotonicity verdict*
rather than exact float values is deliberate: the verdict is robust to platform float
differences and codegen reassociation, so a receipt emitted on one machine re-verifies on
another even though the exact printed floats may differ in the last bits.

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

1. **Re-derive the source digest** from the source referenced by the receipt (the same
   pipeline that produced the stored digests) and compare both the source and input-graph
   digests. A change to the source file since sealing shows up here as a mismatch.
2. **Re-run the program with the receipt's recorded `args`**, re-parse the series, and
   **re-check the measurement count**: the re-run must produce exactly
   `measurement.count` values (the count is deterministic, unlike the exact floats), so an
   edited `observed_values` array of the wrong length is caught here.
3. **Recompute the verdict** with the exact same status rule. The recomputed
   `invariant.status`, `increase_count`, and `receipt_status` must match the stored values;
   any drift is a verification failure with a clear `... drift: receipt X, re-run Y`
   diagnostic. This checks the *verdict*, not exact floats, so it is robust to platform
   float non-reproducibility (the same principle `buildc corpus verify` uses when it
   re-runs C stdout).
4. **Recompute the seal** over the stored receipt and compare to `seal.hex`. This catches
   accidental corruption of any field that does not change the re-run verdict (for example
   `runtime_state.os`), giving layered integrity.

### Exit-code semantics (safe as a CI gate)

A receipt that passes all four checks is **faithful**: it reproduces. But a faithful
receipt that *records* a failure is not a pass, so the exit code reflects the verdict too:

| outcome | exit code |
|---|---|
| faithful, `PASS` or `FAIL_EXPECTED` | `0` (human output: `MATCH: ...`) |
| faithful, `FAIL_UNEXPECTED` or `UNVERIFIABLE` | `3` (human output: `FAIL: ... invariant did not hold`) |
| did not reproduce (digest, count, verdict, or seal drift) | `1` |

This makes `buildc receipt verify r.json && deploy` safe: it will not deploy on a receipt
that records an unexpected invariant violation or a diverged/unverifiable run. A negative
fixture reproducing its *expected* failure is a legitimate pass (the checker demonstrably
catches violations), so `FAIL_EXPECTED` exits `0`. With `--json`, the report carries
`"faithful"` and `"invariant_held"` fields alongside the verdict.

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

## Provenance

The receipt's `provenance.research_source_hash` references the Telos dogfood research (pass
0009/0010, `BuildScientificRuntimeReceipt/v1`, the heat-equation energy proof) that the
buildc feature is derived from. It is recorded for lineage only. buildc computes its **own**
deterministic seal over its **own** canonical receipt form; the referenced research hash is
never matched byte-wise and no claim is made that buildc reproduced the research artifact
byte-for-byte. The provenance link records where the idea came from, nothing stronger.

## Deferred (tracked follow-ons)

v0 is one invariant over one series, sealed and re-derivable. Explicitly out of scope for v0:

- **Other invariants.** Only `energy-monotone` ships. Additional invariants (conservation,
  boundedness, convergence-rate checks) are separate follow-ons.
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
