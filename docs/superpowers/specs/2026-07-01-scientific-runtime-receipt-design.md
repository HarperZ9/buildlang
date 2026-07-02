# Design: Scientific-Runtime Receipt (buildc accountable compute)

Status: draft for execution (2026-07-01), branch `feat/sci-runtime-receipt`. Governed by
`docs/UNIVERSAL_SUBSTRATE_DIRECTIVE_2026-06-30.md`. Applies the Telos dogfood research
(pass 0009/0010 `BuildScientificRuntimeReceipt/v1`, the heat-equation energy proof) into buildc.
The accountable-compute layer on top of the math syntax (Pillar B).

## Summary

Add `buildc run --emit-receipt <path>`: buildc compiles and runs a `.bld` program, captures its
numeric stdout as a measurement series, checks a stated **invariant** over that series, and
emits a sealed, re-checkable JSON receipt (`buildlang-scientific-runtime-receipt/v0`). Add
`buildc receipt verify` support for that schema: re-run the program, re-derive the source
digest, and re-check the invariant verdict. The flagship invariant is **energy-monotone
non-increasing** and the flagship program is a 1-D heat-equation discrete-energy kernel written
in buildlang using the shipped `Vec<f64>` `linalg` library and `**`.

This is the reconcile applied to numerical compute: perceive (run the program) -> check against
an unauthored criterion (the invariant) -> carry a re-checkable proof (the receipt).

## Honest scope (read first)

The receipt witnesses that **the compiled program's observed output series satisfies (or, for a
negative fixture, expectedly violates) the stated invariant**. It does NOT prove the underlying
PDE is solved correctly, and it does NOT claim a new physical law. The receipt carries the label
`NOT_A_NEW_PHYSICAL_LAW`. buildc checks a mathematical monotonicity property of a numeric series;
the physics lives in the `.bld` program, not in the compiler. v0 is deliberately one invariant
(monotone non-increasing) over one series, sealed and re-derivable; the full 7-layer richness and
Crucible-at-emit-time are deferred.

## Why program-emitted (not a compiler builtin probe)

Two models were considered:
- **(A) builtin probe:** buildc embeds a fixed heat-equation FD solver + energy check in Rust
  (like `corpus verify`). Rejected for v0: more compiler code, more brittle, and it cannot
  dogfood the shipped math syntax (the point of the demo).
- **(B) program-emitted (chosen):** the kernel is a `.bld` program; buildc runs it, captures the
  numeric series, and checks the invariant. `cmd_run` today uses `.status()` (inherits stdout,
  `main.rs:5609`); `cmd_test` already uses `.output()` to capture (`main.rs:5768`), so capture is a
  contained change. Integrity comes from `receipt verify` RE-RUNNING the program and re-checking
  the invariant (as `corpus verify` re-runs C stdout), not from trusting the stored series.

## Determinism decision

buildc checks and seals the **invariant verdict** (monotone non-increasing within tolerance
`1e-12`; `violation_count`), NOT exact float values. The stable run yields `violation_count == 0`;
the unstable run blows up (`~1.6e28`), so the verdict is robust to platform float differences and
codegen reassociation. buildc computes its OWN deterministic SHA-256 seal over its canonical
receipt form via the existing `source_digest_hex` (`main.rs:3798`); the research's Python-probe
`source_hash` (`b3021c14...`) is recorded only as a provenance reference, never matched byte-wise.

## Architecture

### The receipt (`compiler/src/scientific_runtime.rs`, new module)

Model on `compiler/src/mir_representation.rs` (self-contained: own schema const, serde
`Serialize`/`Deserialize` structs, a `build_*` fn, a `verify_*` fn, importing `source_digest_hex`
and the digest structs from `super`). Register `mod scientific_runtime;` in `main.rs` and
re-export `verify_scientific_runtime_receipt` like the other receipt modules.

Schema id: `buildlang-scientific-runtime-receipt/v0`. Fields (honest subset of the research's 16,
using the layers buildc can actually fill):
- `schema`, `compiler` = "buildc", `compiler_version`, `language_version`.
- `source` (path), `source_digest` (`{algorithm:"sha256", hex}` via `source_text_digest_hex`),
  `input_graph_digest` (over the module graph, reusing `input_graph_digest`).
- `build_state`: `{ target: "c", compiler_status: "compiled_and_executed", flags: [...] }`.
- `runtime_state`: `{ os, exit_code }`.
- `problem`: `{ label }` (from `--problem`, e.g. "1d-heat-equation-energy"; free-text, optional).
- `measurement`: `{ metric (from --metric, default "series"), observed_values: [f64], count,
  units (optional) }` (the parsed stdout series).
- `invariant`: `{ name: "energy_monotone_nonincreasing", expectation: "no step increases energy
  beyond tolerance", tolerance: 1e-12, observed: { violation_count, first_violation_step (opt),
  initial_value, final_value }, status: "PASS"|"FAIL" }`.
- `negative_fixture`: bool (from `--negative-fixture`).
- `labels`: `["NOT_A_NEW_PHYSICAL_LAW"]` (+ `"NEGATIVE_FIXTURE"` when applicable).
- `receipt_status`: `PASS` | `FAIL_EXPECTED` | `FAIL_UNEXPECTED` | `UNVERIFIABLE`.
- `seal`: `{algorithm:"sha256", hex}` over the canonical receipt (all fields except `seal`).
- `provenance`: `{ research_source_hash: "b3021c14..." }` (reference only).

Status rule: invariant PASS -> `receipt_status = PASS`. Invariant FAIL with `--negative-fixture`
-> `FAIL_EXPECTED`. Invariant FAIL without it -> `FAIL_UNEXPECTED`. Empty/unparseable series ->
`UNVERIFIABLE`.

### The invariant checker

`energy_monotone_nonincreasing(series: &[f64], tol) -> { violation_count, first_violation_step,
initial, final }`: count steps where `series[k+1] > series[k] + tol`. PASS iff `violation_count ==
0` and `series.len() >= 2`.

### CLI + capture (`compiler/src/main.rs`)

Add to the `Run` subcommand (`main.rs:178-186`): `--emit-receipt <PATH>` (`-` = stdout),
`--invariant <NAME>` (default `energy-monotone`), `--metric <NAME>`, `--problem <LABEL>`,
`--negative-fixture`. Thread through the dispatch (`main.rs:473`) into `cmd_run`. When
`--emit-receipt` is set, `cmd_run` uses `.output()` (capture) instead of `.status()`
(`main.rs:5609-5616`), echoes the captured stdout (so `run` still shows output), parses the
whitespace/newline-separated f64 series, runs the invariant checker, builds + seals the receipt
via the new module, and writes it (reuse `write_check_receipt`, `main.rs:4556`, which handles
`-`). When `--emit-receipt` is NOT set, behavior is byte-identical to today (`.status()` path
untouched).

### Verify (`compiler/src/main.rs`)

In `cmd_receipt_verify` (`main.rs:1701`) and `cmd_receipt_verify_json` (`main.rs:1813`): after
reading `/schema`, branch `buildlang-scientific-runtime-receipt/v0` to a verifier that (1)
re-derives the source digest from the embedded `/source` via `source_text_digest_hex` and compares
with `verify_receipt_digest` (`main.rs:1385`); (2) re-runs the program (compile + `.output()`),
re-parses the series, re-runs the invariant checker, and compares the recomputed
`invariant.status` + `violation_count` + `receipt_status` against the stored ones. A drift is a
verification failure. (Re-run, not stored-value trust, matches `corpus verify`.)

### The kernel program (`examples/heat_equation_energy.bld`)

A buildlang 1-D heat-equation discrete-energy kernel. `mod core; mod math; mod linalg;`. Build
`u: Vec<f64>` of 129 points from `u_i = sin(pi x_i) + 0.25 sin(3 pi x_i)`, endpoints 0. Loop 400
steps; each step DOUBLE-BUFFER a fresh `u_next` (via `vec_new_f64`/`vec_push_f64`, since
`vec_set_f64` is unexposed) with the explicit update `u_next[i] = u[i] + r*(u[i-1] - 2*u[i] +
u[i+1])` for interior `i`, boundaries 0; compute `e = linalg::vec_dot(u_next, u_next) * dx` and
`println!("{}", e)`; set `u = u_next`. `r = alpha*dt/dx**2` chosen stable (`r = 0.45`, using the
`**` operator for `dx**2`). Use `Vec<f64>` + `while`/`for`-over-Vec loops, NOT `.+` broadcasting
(broadcasting compile-time-unrolls over a fixed-N Array and is the wrong vehicle for a runtime
129-point stencil). An unstable variant (`r = 0.55`) is the negative fixture.

## Data flow

`buildc run kernel.bld --emit-receipt out.json` -> compile to C -> execute (capture stdout) ->
parse f64 series -> invariant check -> build receipt (source/build/runtime/problem/measurement/
invariant/labels/status) -> seal (sha256 over canonical) -> write. `buildc receipt verify out.json`
-> re-derive source digest + re-run + re-check invariant -> MATCH/mismatch.

## Backward compatibility

`run` without `--emit-receipt` is byte-identical (the `.status()` path is untouched; capture only
on the receipt path). New CLI flags are additive. New module + new `receipt verify` schema branch;
the existing `buildlang-check-receipt/v1` path is unchanged. serde/serde_json/sha2 already deps
(no new deps). Prove with the differential sweep + `corpus verify` 8/8 + full `cargo test`.

## Testing

- **Kernel:** `buildc run examples/heat_equation_energy.bld` prints a strictly non-increasing
  energy series (stable). The unstable variant prints an increasing/blowing-up series.
- **Emit (e2e, cli.rs):** `run --emit-receipt -` on the stable kernel yields a receipt with
  `receipt_status == "PASS"`, `invariant.status == "PASS"`, `invariant.observed.violation_count ==
  0`, a valid `seal`, and `labels` containing `NOT_A_NEW_PHYSICAL_LAW`. The unstable kernel with
  `--negative-fixture` yields `receipt_status == "FAIL_EXPECTED"`; without it, `FAIL_UNEXPECTED`.
- **Verify (e2e):** `receipt verify` on the emitted stable receipt -> success; a receipt whose
  stored `invariant.status` is tampered to disagree with the re-run -> failure; a source-digest
  tamper -> failure.
- **Regression:** full `cargo test` green; `cargo fmt --check` + `cargo clippy -- -D
  clippy::correctness` clean (run `cargo fmt`, commit; do not revert the drift files); `buildc
  corpus verify` 8/8; the differential sweep 0 regressions (existing programs' emitted C
  unchanged; `run` default path unchanged).

## Decomposition (each: impl -> task-review gate; buildc-run verifiable)

- **T1 — the kernel** (`examples/heat_equation_energy.bld`): the heat-equation energy kernel over
  `Vec<f64>` + `linalg::vec_dot` + `**`; prints the energy series. Verify stable = monotone,
  unstable variant = increases.
- **T2 — emit** (`scientific_runtime.rs` + `run --emit-receipt` + the invariant checker + capture
  + seal): the receipt module, CLI flags, stdout capture, invariant check, sealed emission.
- **T3 — verify** (`receipt verify` schema branch): re-derive digest + re-run + re-check invariant.
- **T4 — tests + docs**: cli.rs emit+verify round-trip (stable PASS, unstable FAIL_EXPECTED);
  STATUS.md bullet + `docs/SCIENTIFIC-RECEIPT.md` (honest scope).

## Risks

- **Float parse/print agreement.** The receipt parser and the verify re-check must parse the C
  backend's `println!("{}", f64)` format identically. Mitigation: check the verdict (monotonicity),
  not exact values; confirm the f64 print format at T1.
- **`.output()` swallows live output.** Mitigation: echo the captured stdout in receipt mode.
- **Double-buffering allocation.** ~2x alloc/step at 129pts x 400 steps is fine; `vec_set_f64`
  wiring is a deferred optimization (name exists at `runtime.rs:240`, unwired).
- **Overclaim.** Mitigation: the `NOT_A_NEW_PHYSICAL_LAW` label + the honest-scope doc; the receipt
  states it witnesses an observed-series invariant, not PDE correctness.
