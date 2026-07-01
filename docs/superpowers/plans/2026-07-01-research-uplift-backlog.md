# Research-Uplift Backlog (from all previous runs, full project shape)

> Synthesized 2026-07-01 from a 14-reader harvest over the complete Telos dogfood
> research corpus (113 pass ledgers, schemas/, adversarial/, fixtures/, crucible/,
> tools/, rl-scaling-receipt-spine.md), a buildc gap-map self-audit, four grounded
> research videos, and this session's own verified findings. 123 raw items were
> deduplicated into the waves below. Ranking = honesty uplift x architectural fit
> x effort. The governing lesson (verified against the sibling Crucible registry,
> where 872/875 deviations were author-supplied and refutations never executed):
> **a verifier that cannot fail proves nothing.**

## Wave 1: verifier falsification (branch `feat/verifier-falsification`)

The anti-self-confirming sweep. Every item makes an existing green light falsifiable.

- **W1.1 Corpus execution tamper tests.** The C and Rust execution receipts are the
  ONLY corpus receipt family with zero negative fixtures (30+ `corpus_verify_rejects_*`
  tests cover the other six families). Add tamper tests via the existing
  `assert_corpus_verify_rejects` helper: manifest `expected_stdout` edited (proves
  `verify_c_corpus_stdout` can fail), receipt `result.passed` edited, receipt program
  list truncated. [harvest items 116, 109, 31]
- **W1.2 Re-derive the corpus capability gate.** `apply_capability_receipt_metadata`
  stamps `capability_gate: "passed"` unconditionally and `verify_receipt` merely
  string-compares the stamp: the exact Crucible anti-pattern inside buildc. Fix:
  corpus verify re-derives each program's observed capabilities through the REAL
  type checker (`run_check` -> `FunctionEffectSummary.observed_capabilities`),
  unions them, and fails on drift against the stored `declared_effects` /
  `observed_capabilities`. Tamper test: a corpus-copy program gains a capability
  (stdout unchanged) and corpus verify must REJECT. [items 114, 115-lite]
- **W1.3 Typed negative fixtures.** ALREADY SUBSTANTIALLY ADDRESSED: the divergence
  fix routes non-finite failures to UNVERIFIABLE (never FAIL_EXPECTED), which was
  the wrong-reason case item 79 named. The residual (distinguishing finite failure
  KINDS) is meaningless while v0 has one invariant; revisit in Wave 3 when the
  invariant family lands. [item 79: adjudicated, deferred residual]
- **W1.4 Typed failure codes.** `receipt verify` failures currently collapse into
  exit 1 + prose. Add a stable `failure_class` enum (SOURCE_DIGEST_MISMATCH,
  MEASUREMENT_COUNT_DRIFT, INVARIANT_STATUS_DRIFT, INCREASE_COUNT_DRIFT,
  RECEIPT_STATUS_DRIFT, SEAL_MISMATCH, DIVERGENCE_NOT_REPRODUCED, MALFORMED, ...)
  emitted on stderr and in the `--json` failure report, so negative fixtures can
  pin (code, verdict) pairs instead of "anything failed". [items 43, 21, 88, 108]
- **W1.5 Strict receipt loading.** serde_json is last-duplicate-wins: a receipt
  with two `receipt_status` keys is a seal-forgery vector (hasher sees one, reader
  the other). Load scientific receipts through a duplicate-key-rejecting parser
  (mirror the bdf/json.rs custom-visitor pattern); prove non-finite JSON literals
  already fail; negative fixtures for both. [items 12, 97, 103, 80]

## Wave 2: receipt v0.1 schema honesty

- Raw stdout sha256 + series-extraction-policy version alongside the parsed series,
  so RAW drift is distinguishable from parse-policy tolerance. [81, 90, 11, 6]
- Toolchain block (cc identity + version output hash, target triple, host OS) and
  verify preflight, splitting NOT_REPRODUCED into TOOLCHAIN_MISMATCH vs genuine
  output divergence; TOOL_UNAVAILABLE as its own verdict, never a silent skip.
  [51, 64, 25, 15, 20, 89, 107]
- `not_claimed` claims-boundary array sealed in-band (the honest-scope section of
  the docs, machine-readable). [28, 75, 41, 83]
- Split UNVERIFIABLE reason in-band: the sealed `diverged` field landed this
  session; add the `unverifiable_reason` enum on the status when a second reason
  class appears. [118: partially landed]
- `buildc doctor --json` capability/availability matrix receipt. [122, 86, 52, 104]

## Wave 3: the invariant family (needs multi-series capture)

- Multi-column series capture (v0 = one Vec<f64>): the enabler for everything below.
- `--invariant conservation` (constant sum within tol; positive = periodic-BC heat
  kernel, negative = Dirichlet leak) and `--invariant bounded` (discrete max
  principle; negative = CFL overshoot). [119, 3, 68, 77, 93, 99]
- `--invariant energy-identity` (quantitative dissipation residual
  d/dt||u||^2 = -2k||u_x||^2 as a per-step residual bound, not just monotonicity).
  [39, 98]
- Relation-invariants (|f(series_a) - g(series_b)| <= tol): grounded by the
  Carcassi Born-rule/entropy equivalence kernel (checkable numeric identity;
  AoP Brief 003) and the Lyapunov/detailed-balance certificate fixtures worked in
  passes 0091-0104. Every invariant ships with its paired wrong-system AND
  wrong-certificate negative fixtures. [72, 78, 94, 101 + video 1]
- Result-bearing kernel: funnel hashing (arXiv 2501.02305) with a measured
  probe-complexity bound receipt (witnesses the measurement, not the theorem).
  [video 3]

## Wave 4: verifier self-test + chains (larger arcs)

- `receipt verify --self-test`: auto-materialize one tampered variant per receipt
  field, assert each is rejected with its distinct failure code. [87, 117, 42]
- `corpus verify --self-test`: inject one known tamper per receipt family in a
  temp corpus copy; a user-run green becomes distinguishable from a decorative one.
  [117, 32]
- Corpus expected-classification manifests (MATCH / DRIFT_EXPECTED /
  BOUNDARY_EXPECTED / FAIL_EXPECTED per fixture). [70, 100, 84]
- Receipt chaining (check-receipt seal embedded in the scientific receipt;
  `receipt chain-verify`). [45, 23, 34]
- Backend-admission policy: a second backend (LLVM JIT) is admitted only after
  reproducing the C backend's receipt verdicts on the corpus. [55, 113]
- Rust-execution re-run lane or explicit maturity demotion in corpus output. [115, 46]
- Seeded stochastic receipts (needs a seeded RNG builtin). [49, 71, 102]

## Explicitly not adopted (with reasons)

- SLSA/in-toto renderer [9]: interop polish, not honesty; revisit at productization.
- OTel attachment layer [35], hash-chained global run ledger [18]: platform-scale
  machinery Telos owns; buildc adopts only the per-receipt forms.
- Looped-LLM items (video 4): thesis-relevant (latent loops remove the auditable
  trace, strengthening the receipt argument) but no compiler artifact.
- Synthetic infinity-categories (video 2): design-philosophy reinforcement for
  native linear types/effects; no shovel-ready compiler item.
