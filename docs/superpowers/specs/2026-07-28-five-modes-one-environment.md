# Five modes, one environment: an honest assessment of the epistemics-OS question

**Question posed by the operator, 2026-07-28:** can buildlang/buildc be
upgraded into a complete AI-native, rendering, science, and epistemics OS,
built to marry deterministic, probabilistic, stochastic, heuristic, and Monte
Carlo computation in one environment, natively?

**Short answer:** yes, and buildc is unusually positioned for it, for one
precise reason. Its epistemics core already treats nondeterminism as a TYPED,
WITNESSED ABSENCE: the effect system derives the receipt's determinism fields
from capabilities, fail-closed. Most platforms marry the five modes by erasing
their differences (everything is just a number when it returns). buildc can
marry them by TYPING the differences: each mode is admitted as a capability
with a mode-matched receipt discipline, and a claim without its discipline
does not compile into the environment's evidence layer at all.

**What "OS" honestly means here:** an operating ENVIRONMENT, not a kernel.
Language + runtime + receipts + corpus + registry + editors + the flywheel
bridge. Do not use the word OS on a public surface until it operates
something; this document is internal register.

**Posture constraints inherited, not renegotiated:** the 2026-06-15 wind-down
holds. C stays the verified execution anchor; Rust is the validation lane;
SPIR-V and friends stay experimental with loud maturity labels. Every slice
below lands the way the invariant family landed: one reviewed commit, a
paired positive/negative fixture, a verdict the verifier re-derives.

---

## 1. The unifying design rule

One rule generates the whole architecture:

> A computation mode enters the environment as a CAPABILITY EFFECT, and its
> results enter the evidence layer only through a RECEIPT DISCIPLINE matched
> to what that mode can actually promise.

The five modes, under that rule:

| mode | admitted as | receipt discipline | state today |
|---|---|---|---|
| deterministic | pure functions, `Console` | byte-identity on re-run (MATCH) | SHIPS |
| probabilistic, exact | closed-form distributions | same as deterministic: the math is analytic | SHIPS by example (the Born-rule/entropy relation pair checks a probability identity to roundoff) |
| stochastic | a `Random(seed)` capability | the seed is SEALED; replay is exact; the determinism field flips from witnessed-absence to witnessed-seed | planned (the Wave 4 seeded-RNG builtin), not built |
| Monte Carlo | an estimator over `Random` | n, seed, estimator id, interval by a DECLARED method; the claim is the interval, never the point | not built; the flywheel harness's statistics module is the reference design (declared MDE, "no effect" vs "no power" made distinguishable) |
| heuristic | a search under a BUDGET | incumbent + budget ceiling + `NOT_PROVES_OPTIMALITY` on every result | half-ships: the family's not-proven idiom and the binary-search probe kernel exist; budget fields do not exist in buildc receipts yet |

The deep point: the weaker the mode's promise, the more the receipt must
carry. Deterministic needs only a hash. Monte Carlo needs its denominator,
its seed, and its interval method, or the number is unpriceable. That is the
same law the flywheel receipt learned this cycle (a hit count without
attempts is unpriceable; a result without its budget ceiling hides whether it
stopped at the limit), now stated as a language-level admission rule.

## 2. What each pillar already has, verified in-tree

**Epistemics (the strongest pillar, already the identity):** sealed
scientific receipts re-derived by re-running; the seven-member invariant
family plus the Born-rule/entropy relation pair; receipt and corpus
self-tests (every tamper rejected with a typed failure class); capability
effects filling determinism fields fail-closed; the `receipt export` bridge
emitting WITNESSED Crucible measurements; `buildc_receipt_bridge.py` in the
flywheel harness importing all of it fail-closed.

**Science:** the invariant family spans physics (heat, rotation, oscillator,
symplectic, reaction network), one quantum identity, and algorithmic bounds;
units canonicalization exists at the CLI; the exact-rational symplectic
oracle is fenced and waiting on the research side.

**Rendering:** HLSL/GLSL source output ships as a supported adjunct; a GPU
receipt module and a GPU cross-check (max-abs-deviation against the CPU
anchor) exist; SPIR-V is a real but experimental backend. The old graphics
vision ("write the math once, run it identically on CPU and GPU") survives
INSIDE this frame as a receipt: the same kernel's CPU and GPU outputs are two
columns of a relation, checked under a fixed tolerance. Rendering does not
need its own epistemics; it needs the existing one applied across backends.

**AI-native (the thinnest pillar today):** nothing in the language knows
about models. What exists is the seam: the flywheel bridge, and the harness's
propose/dispose thesis. The upgrade is to make a model call a FOREIGN
CAPABILITY (`~ Model`), receipted like any other boundary crossing (model
digest, prompt hash, parameters, seed), with one type rule that IS the
flywheel thesis: a value originating under `~ Model` cannot reach an accept
path without passing a receipt boundary. Models propose; oracles dispose;
the compiler enforces the split.

## 3. The staged path, each slice falsifiable

Ordered so every slice is one reviewed commit with its own negative fixture,
and no slice depends on an unbuilt one.

1. **`Random(seed)` capability + witnessed-seed receipts.** The Wave 4 item,
   promoted: it unlocks stochastic and Monte Carlo at once. Positive kernel: a
   seeded random walk whose step-count invariant holds; negative: the same
   kernel demanding `Random` without a seal, refused fail-closed. The
   determinism field gains a third honest state: `seeded(<seed>)`.
2. **Monte Carlo estimator receipts.** A known-answer kernel (pi by
   rejection sampling) under a new receipt block: n, seed, estimator id,
   interval by a declared method. v0 claims REPRODUCIBILITY and interval
   discipline, not correctness of the interval; for known-answer kernels the
   `bounded` invariant additionally checks the estimate against truth.
   Negative: an estimator whose interval method is undeclared, refused.
3. **Budgeted-search receipts.** Budget ceiling fields (mirroring the
   flywheel receipt v3: consumed vs allowed, exhausted flag), a heuristic
   kernel (greedy on a fixed instance) carrying `NOT_PROVES_OPTIMALITY` and
   its budget; negative: a claimed-optimal greedy result, refused by the
   claim-language rule already in the family's idiom.
4. **The `Model` capability.** Foreign-capability model calls through the
   local endpoint, receipted; the propose/dispose type rule; first demo is
   the flywheel loop calling a buildc-verified kernel as its oracle, which
   closes the loop with the harness pilot already planned on the other side.
5. **Cross-backend relation receipts.** The same kernel emitted through C
   and through the GPU lane, outputs as relation columns under a fixed
   tolerance, formalizing the existing GPU cross-check as a receipt family.

Slices 1 through 3 are pure buildc work in the shipped pattern. Slice 4
touches the harness seam and should be co-designed with the flywheel side.
Slice 5 promotes the rendering pillar without granting any experimental
backend a maturity it has not earned.

## 4. Risks, stated plainly

- **Scale.** This is a single-maintainer ecosystem and the compiler is large.
  The mitigation is the one already proven: slice discipline, adversarial
  review per slice, and refusing to let any slice grow past one commit.
- **Scope gravity.** "OS" invites building everything. The admission rule is
  the defense: nothing enters without its receipt discipline, including
  features.
- **The word "natively."** Slices 1 to 3 are genuinely native (language and
  runtime). The AI pillar starts as a foreign capability, which is the honest
  shape: models are boundary crossings, and pretending otherwise is how
  provenance dies.
- **Claim hygiene.** Public surfaces keep maturity labels exact, per
  AGENTS.md. The phrase "epistemics OS" stays internal until the environment
  demonstrably operates the five modes end to end.

## 5. What this document is not

Not a plan (each slice gets its own, in the writing-plans idiom, when picked
up). Not a commitment of sequence beyond the dependency order stated. Not a
public claim. It is the assessment the question deserved: the marriage is
achievable, the foundation is real, and the route is the same discipline
that built the invariant family, applied five times.
