# Five modes, one chain

Scientific computation is not one shape. A simulation is deterministic, a
quantum amplitude is exact-probabilistic, a random walk is stochastic, a Monte
Carlo estimate declares its sampling discipline, and a heuristic search
reports an incumbent under a budget. `buildc run --emit-receipt` seals a
PASS/FAIL_EXPECTED verdict for any one of them, and `buildc receipt chain`
binds any number of receipts into one ordered, tamper-evident bundle with no
change to the receipt schema itself. This tour emits one receipt per mode from
the shipped example kernels, chains them, and verifies the chain. Every
command below is copy-paste from the repo root. Full schema and
failure-class reference: [SCIENTIFIC-RECEIPT.md](SCIENTIFIC-RECEIPT.md).

## The five receipts

### 1. Deterministic

```
buildc run examples/heat_equation_energy.bld --emit-receipt det.json \
  --invariant energy-monotone --metric energy --problem 1d-heat-equation-energy
```

The 1-D heat equation's discrete energy is monotone non-increasing under a
stable finite-difference step. This receipt witnesses that the compiled
program's captured energy series never rose across the run; it does not prove
the discretization is convergent or that the parameters are physical.

### 2. Probabilistic, exact

```
buildc run examples/born_rule_normalization.bld --emit-receipt prob.json \
  --invariant conservation --problem born-rule-normalization
```

A qubit evolved by a unitary gate keeps its total Born probability at 1 to
roundoff. This receipt witnesses that the observed probability stayed
constant across the run; it does not claim anything about the physics of
measurement beyond that single conserved-quantity check.

### 3. Stochastic

```
buildc run examples/random_walk_bound.bld --emit-receipt stoch.json \
  --invariant non-negative --metric slack --problem seeded-random-walk-envelope --seed 42
```

A seeded random walk can never leave its worst-case envelope, so the slack
against that bound stays non-negative for every seed. The seed is sealed, so
`receipt verify` re-runs the exact stream; the receipt witnesses this seed's
run, not the distribution over all seeds.

### 4. Monte Carlo

```
buildc run examples/mc_pi_rejection.bld --emit-receipt mc.json \
  --invariant non-negative --metric slack --problem mc-pi-rejection --seed 42 \
  --mc-estimator mean --mc-samples 2000 --mc-interval normal-approx-95
```

Pi by rejection sampling, checked against a calibrated error band from a
burn-in. The receipt seals the estimator's declared denominator, seed, and
interval method, and witnesses that this seed's estimate landed in the band;
it never claims the interval method itself is statistically correct, only
that the facts needed to price the estimate were declared and reproduced.

### 5. Heuristic

```
buildc run examples/greedy_change_budget.bld --emit-receipt heur.json \
  --invariant non-negative --metric slack --problem greedy-change-budget \
  --budget-steps 60000 --budget-consumed 495
```

Greedy coin change is not optimal for every denomination set, run under a
calibrated step budget. The receipt witnesses that the search stayed inside
its declared budget and carries `NOT_PROVES_OPTIMALITY` in its labels; a
budgeted heuristic reports its incumbent, never a proof no better answer
exists.

### Bonus: cross-backend

Needs a local `rustc`:

```
buildc run examples/decay_cross_backend.bld --emit-receipt cross.json \
  --cross-backend rust --invariant cross-backend --problem decay-cross-backend
```

The same decay recurrence run through the C backend and the Rust backend,
sealed as a two-column relation. The receipt witnesses that the two backends
agreed on this kernel; it does not witness correctness of either backend on
kernels it was not run against.

## Chaining them

```
buildc receipt chain build det.json prob.json stoch.json mc.json heur.json cross.json -o chain.json
buildc receipt chain verify chain.json
```

`chain build` records each member's seal in order and computes one seal over
that ordered list. `chain verify` recomputes the chain seal, checks each
member's current seal against the pinned one, and re-verifies every member
through the real `receipt verify` path, so it needs the same C toolchain (and,
for the cross-backend link, `rustc`) the individual receipts did.

Edit a member's stored `violation_count` without re-sealing it and re-run
`chain verify`: the pinned seal still matches, but the member no longer
re-verifies, so the chain fails closed with `CHAIN_LINK_UNVERIFIED`. Reorder,
drop, or substitute a member instead, and the chain fails with
`CHAIN_SEAL_MISMATCH`, `CHAIN_LINK_MISSING`, or `CHAIN_LINK_TAMPERED`
respectively. `compiler/tests/cli.rs::five_modes_bind_into_one_chain`
exercises this end to end.

## What the chain proves, and what it does not

A verified chain proves that five (or six) receipts existed in a stated
order, each one unaltered since it was chained, and each one still
independently re-derivable from its own source. It does not prove any of the
five computations is correct in the physical or mathematical sense: each
member's honest-scope sentence above still applies on its own. The chain adds
ordering and bundling; it does not upgrade any member's claim.
