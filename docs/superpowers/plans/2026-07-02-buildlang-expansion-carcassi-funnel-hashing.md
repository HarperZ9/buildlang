# buildlang/buildc Expansion: Carcassi Born-Rule + Funnel-Hashing Kernels Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **This is a fresh-session handoff.** It was authored at the close of the Phase D checkpoint (main `90bb775`, buildlang 1.1.0 on crates.io). A prior session wrote it as the next work packet; it is not being executed by its author. Read the Roadmap section first to understand what already shipped, then execute the two features below in order.

**Goal:** Add two new showcase kernels to the scientific-runtime-receipt family, each reusing an existing invariant (no new registry entries): (1) a Born-rule normalization kernel that witnesses quantum-state probability conservation under unitary evolution via `conservation`, and (2) a funnel-hashing (arXiv 2501.02305) probe-complexity kernel that witnesses a sub-linear worst-case probe bound via `non-negative`.

**Architecture:** Both features follow the family's established shape: a positive `.bld` kernel that PASSes its invariant, a paired negative fixture that FAILs it for the right reason (the can-it-FAIL law), a CLI round-trip test in `compiler/tests/cli.rs` (emit PASS then verify exit 0; emit negative fixture then verify exit 0), and a docs entry in `docs/SCIENTIFIC-RECEIPT.md`. Neither feature touches `compiler/src/scientific_runtime.rs` or the invariant registry: they are pure kernel + example + test + docs work on top of invariants that already ship.

**Tech Stack:** Rust compiler (`buildlang` crate, `buildc` binary), buildlang `.bld` source kernels compiled through the C backend to native executables, `serde_json` for receipt inspection in tests.

## Global Constraints

Copy these verbatim into every task's working context. They bind all tasks.

- **No new invariants.** Carcassi reuses `conservation`; funnel-hashing reuses `non-negative`. Do NOT add arms to `scientific_runtime.rs`, `KNOWN_INVARIANTS`, `invariant_tolerance`, `invariant_expectation`, `evaluate_invariant`, or `evaluate_measurement`. If, after reading AoP Brief 003, you conclude the deeper Born-rule entropy equivalence genuinely needs a new invariant, that is a scope escalation to the operator, not part of this plan.
- **Kernel idiom.** Pure `println!` output, one `f64` per line; effect signature `fn main() ~ Console`; heap `Vec` builtins (`vec_new_f64`/`vec_push_f64`/`vec_get_f64`/`vec_new_i64`/`vec_push_i64`/`vec_get_i64`/`vec_len`); the `**` power operator and `cos`/`sin` builtins are available; hardcode numeric literals (there is no `pi()` without module declarations). NO em-dashes anywhere in kernel comments, docs, or commit messages (rewrite the sentence, do not swap the character).
- **In-place `Vec` writes.** Indexed assignment `table[slot] = value` compiles and lowers to `build_hvec_set_i64` / `build_hvec_set_f64` (see `compiler/src/codegen/lower/expr.rs:1236` and the codegen test `vec_indexed_assignment_stores_through_a_setter`). The receipt docs say `vec_set_f64` "is not yet exposed"; that refers to the standalone builtin *name*, not indexed assignment. Reads from a heap `Vec` use the builtin form `vec_get_i64(table, slot)`, matching `examples/search_bound_binary.bld`.
- **Always emit and verify from the repo root** (`c:/dev/public/pubscan/quantalang`). The receipt seals the source path relative to the working directory; emitting from `compiler/` then verifying from the root breaks path resolution (a lesson already paid for during Phase D).
- **Cadence per feature.** build -> gate (full suite + corpus 8/8 + live matrix, all from repo root) -> Workflow adversarial review (N lenses -> structured find -> independent adversarial verify -> confirmed findings) -> fix every confirmed finding -> merge on a proper branch with `--no-ff` -> push. Every kernel ships a negative fixture that fails for the right reason.
- **Baseline is measured, not assumed.** At main `90bb775` the gate was roughly 940 lib / 309 cli / 52 / 88 passing with corpus 8/8, but re-run `cargo test -p buildlang` yourself to get the true current baseline before you add tests. Each new CLI round-trip test raises the cli count.
- **Publishing is operator-gated.** Do NOT publish to crates.io as part of this work. When both features land, the next release would be `1.2.0` (minor: backward-compatible new kernels). Publishing waits for an explicit operator "yes" / "re-publish".

---

## Roadmap: what shipped, what remains

### Shipped through the Phase D checkpoint (main `90bb775`, buildlang 1.1.0)

| Layer | Delivered |
| --- | --- |
| Phase A | Receipt export bridge: `buildc receipt export` emits witnessed Crucible measurements |
| Phase B | Effect system fills `input_dataset`/`seed`/`determinism` as fail-closed, capability-derived witnessed absences plus a declared method and effect-policy chain |
| Phase C | The invariant family (four adversarially-reviewed slices): `energy-monotone`, `conservation`, `bounded`, `energy-identity`, `relation` |
| Phase D1 | `conserved-band` (approximate conservation, symplectic leapfrog demo) |
| Phase D2 | `non-negative` (absolute lower floor; the algorithmic result-bearing member; binary-search probe-slack demo) |
| Phase D3 | Reaction-invariant checker (chemistry demo, reuses `conservation`) |
| Release | buildlang 1.1.0 published to crates.io; README/CHANGELOG/STATUS/repo-description synced |

The invariant family is SEVEN members. Both master-plan named demos ship (a Hamiltonian runtime branch via `conserved-band`/symplectic, and a reaction invariant checker). Coverage spans physics (heat, rotation, oscillator, symplectic, reaction), one algorithmic member (binary-search probe-slack), and cross-column (double-angle relation).

### Remaining (this plan plus the backlog)

1. **Feature 1 (this plan): Carcassi Born-rule normalization kernel.** Reuses `conservation`. Adds a quantum-domain instance to the family.
2. **Feature 2 (this plan): full funnel-hashing probe-complexity kernel.** Reuses `non-negative`. A more ambitious instance of the algorithmic result-bearing member than binary search.
3. **Wave 4 (backlog, not this plan): self-tests, receipt chaining, backend-admission policy, a rust re-run lane, and seeded stochastic receipts.** See the Wave 4 Roadmap section below and `docs/superpowers/plans/2026-07-01-research-uplift-backlog.md`.
4. **Lyapunov-decrease certificate: NEIGHBOR-OWNED, do NOT build here.** It is proof-surface wedge #10 (see the operator handoff `HANDOFF-robotics-cybernetics-wedge10`). Left in the roadmap only so nobody rebuilds it.

### Did we complete the entire roadmap?

**No.** Waves 1 and 2, Phases A/B/C, and Phase D slices D1 through D3 are complete, and 1.1.0 is published with all outward docs synced. Outstanding: the Phase D tail (this plan's two kernels), all of Wave 4, and the neighbor-owned lyapunov wedge. This plan closes the Phase D tail; Wave 4 remains a separate work packet after it.

---

## Feature 1: Carcassi Born-Rule Normalization Kernel

### Design constraint (read before writing any code)

The Carcassi and Aidala result (Assumptions of Physics, Brief 003) frames the Born rule `P_i = |psi_i|^2` as an information-theoretic equivalence: it is the unique probability assignment consistent with the brief's entropy and independence assumptions. That equivalence is a statement about *which rule is correct*, not a per-step numerical identity you can re-check to a fixed tolerance by re-running a program. It is NOT roundoff-crisp, so it does not fit the v0 family directly.

What IS roundoff-crisp, and is the checkable shadow of the Born rule, is **normalization conservation**: for a genuine quantum state evolved by a unitary gate, the total Born probability `sum_i |psi_i|^2` equals 1 for all time. A valid probability distribution stays normalized precisely because the evolution is unitary. That is exactly a `conservation` invariant over the total-probability series.

**First task before coding: read AoP Brief 003** (the operator's research corpus, or fetch the "Assumptions of Physics" Brief 003 by Carcassi and Aidala). Confirm the framing above and extract the exact identity the brief states. Do not prescribe an identity from memory. Then implement the recommended first cut (normalization conservation). The deeper entropy/frequency-convergence version would need either a calibrated tolerance or the Wave 4 seeded-RNG builtin for a frequency-convergence demo; it is deferred, not part of this feature.

**Honest scope.** This kernel does not add a new invariant mechanism (`conservation` already ships, and `conservation_rotation.bld` already conserves a squared radius under a real 2D rotation). Its value is domain coverage: it is the family's first genuinely *quantum* instance, using complex amplitudes and a complex unitary gate, so the imaginary parts carry real weight (which a real 2D rotation never exercises). That is the differentiation from `conservation_rotation.bld`, and the kernel comment must state it.

### Files

- Create: `examples/born_rule_normalization.bld` (positive kernel)
- Create: `examples/born_rule_leaky.bld` (negative fixture)
- Modify: `compiler/tests/cli.rs` (add one round-trip test near the other scientific-runtime CLI tests, after `non_negative_invariant_round_trips_a_result_bearing_bound`)
- Modify: `docs/SCIENTIFIC-RECEIPT.md` (add the kernel pair under the `conservation` member and update the Deferred section)

### Interfaces

- Consumes: the `conservation` invariant (tolerance 1e-9, violation when `|s[k] - s[0]| > tol`), the CLI surface `buildc run <file> --emit-receipt <out> --invariant conservation [--negative-fixture] [--problem <label>]` and `buildc receipt verify <out>`, and the receipt JSON fields `invariant.name` (`conserved_quantity_constant`), `receipt_status` (`PASS` / `FAIL_EXPECTED`), `invariant.observed.violation_count`.
- Produces: two example kernels and one CLI test that the docs and any future release notes reference by name.

### The physics (so the kernel is correct, not cargo-culted)

Represent a single qubit `psi = (alpha, beta)` with complex amplitudes `alpha = a0 + i*b0`, `beta = a1 + i*b1`, stored as four `f64`: `a0, b0, a1, b1`. Start in `|0>`: `(a0,b0,a1,b1) = (1,0,0,0)`.

Evolve by the genuinely-complex single-qubit unitary `U = [[cos t, -i sin t], [-i sin t, cos t]]` (this is `exp(-i t X)`, an X-rotation; it is unitary and mixes real and imaginary parts). Applying `U` to `(alpha, beta)` and separating real and imaginary parts gives the update:

```
a0' = cos t * a0 + sin t * b1
b0' = cos t * b0 - sin t * a1
a1' = cos t * a1 + sin t * b0
b1' = cos t * b1 - sin t * a0
```

The total Born probability is `a0^2 + b0^2 + a1^2 + b1^2`. Under this unitary it stays 1 to roundoff (after one step from `|0>` the state is `(cos t, 0, 0, -sin t)`, probability `cos^2 t + sin^2 t = 1`, and `b1 = -sin t` is nonzero, so the imaginary channel is genuinely exercised).

The negative fixture applies the same rotation followed by a small gain `g = 1.001` on all four components (a non-unitary, physically forbidden gate, `det = g^2 != 1`). The total probability then grows by `g^2` each step and drifts away from 1 far beyond the 1e-9 tolerance, so `conservation` FAILs.

### Task C1: Positive kernel and its CLI round-trip test

**Files:**
- Create: `examples/born_rule_normalization.bld`
- Modify: `compiler/tests/cli.rs`

**Interfaces:**
- Consumes: `conservation` invariant, `buildc run`/`receipt verify` CLI, receipt JSON fields above.
- Produces: `examples/born_rule_normalization.bld` (a kernel printing the total Born probability once per step).

- [ ] **Step 1: Write the failing test**

Add to `compiler/tests/cli.rs`, immediately after the `non_negative_invariant_round_trips_a_result_bearing_bound` test (around line 14339). This mirrors the existing scientific-runtime CLI tests exactly.

```rust
#[test]
fn born_rule_normalization_round_trips_conservation() {
    if !c_backend_ready() {
        eprintln!("skipping born_rule_normalization_round_trips_conservation: C backend not ready");
        return;
    }
    let dir = std::env::temp_dir().join(format!("buildlang_sci_born_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create born-rule fixture dir");

    // POSITIVE: a single qubit evolved by a unitary X-rotation keeps its total
    // Born probability at 1, so `--invariant conservation` PASSes.
    let pass_receipt = dir.join("unitary.json");
    let emit_pass = buildc()
        .arg("run")
        .arg(repo_example("born_rule_normalization.bld"))
        .args(["--emit-receipt"])
        .arg(&pass_receipt)
        .args(["--invariant", "conservation", "--problem", "born-rule-normalization"])
        .output()
        .expect("emit born-rule PASS receipt");
    assert!(
        emit_pass.status.success(),
        "emitting the born-rule PASS receipt should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit_pass.stderr)
    );
    let pass: serde_json::Value =
        serde_json::from_slice(&fs::read(&pass_receipt).expect("read PASS receipt")).unwrap();
    assert_eq!(pass["invariant"]["name"], "conserved_quantity_constant");
    assert_eq!(pass["receipt_status"], "PASS");
    assert_eq!(pass["invariant"]["observed"]["violation_count"], 0);

    let verify_pass = buildc()
        .args(["receipt", "verify"])
        .arg(&pass_receipt)
        .output()
        .expect("verify born-rule PASS receipt");
    assert!(
        verify_pass.status.success(),
        "the born-rule PASS receipt must verify\nstderr:\n{}",
        String::from_utf8_lossy(&verify_pass.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p buildlang --test cli born_rule_normalization_round_trips_conservation -- --nocapture`
Expected: FAIL (the example file does not exist yet, so `buildc run` errors and `emit_pass.status.success()` is false).

- [ ] **Step 3: Write the positive kernel**

Create `examples/born_rule_normalization.bld`:

```rust
// Born-rule normalization kernel (CONSERVING): quantum probability under a
// unitary gate.
//
// A single qubit psi = (alpha, beta) with complex amplitudes alpha = a0 + i*b0
// and beta = a1 + i*b1 is stored as four f64. Evolving it by the unitary
// X-rotation U = [[cos t, -i sin t], [-i sin t, cos t]] preserves the total
// Born probability sum_i |psi_i|^2 = a0^2 + b0^2 + a1^2 + b1^2 = 1 for all time:
// a valid probability distribution stays normalized precisely because the gate
// is unitary. This is the roundoff-crisp, re-checkable shadow of the Born rule
// (the deeper Carcassi and Aidala entropy equivalence in AoP Brief 003 is an
// information-theoretic statement, not a per-step numerical identity).
//
// Unlike conservation_rotation.bld (a real 2D rotation preserving a squared
// radius), this kernel is genuinely complex: after the first step the imaginary
// parts b0/b1 are nonzero and carry the state, so the quantum structure is real.
// The kernel prints the total probability once per step; it stays at 1.0 to the
// ~1e-15 roundoff scale, well inside the conserved-quantity tolerance (1e-9), so
// `--invariant conservation` PASSes. Companion negative fixture:
// born_rule_leaky.bld (a non-unitary gain that inflates the probability).

fn main() ~ Console {
    let t: f64 = 0.05;
    let c: f64 = cos(t);
    let s: f64 = sin(t);
    let mut a0: f64 = 1.0;
    let mut b0: f64 = 0.0;
    let mut a1: f64 = 0.0;
    let mut b1: f64 = 0.0;
    let mut step: i32 = 0;
    while step < 200 {
        // Total Born probability: sum over basis states of |amplitude|^2.
        let prob: f64 = a0 ** 2 + b0 ** 2 + a1 ** 2 + b1 ** 2;
        println!("{}", prob);
        // Apply the unitary X-rotation to (alpha, beta).
        let a0n: f64 = c * a0 + s * b1;
        let b0n: f64 = c * b0 - s * a1;
        let a1n: f64 = c * a1 + s * b0;
        let b1n: f64 = c * b1 - s * a0;
        a0 = a0n;
        b0 = b0n;
        a1 = a1n;
        b1 = b1n;
        step = step + 1;
    }
}
```

- [ ] **Step 4: Sanity-run the kernel directly**

Build once, then run the kernel and eyeball the output (every line must read `1` or `1.0000...` to roughly 1e-15):

Run:
```
cargo build -p buildlang --bin buildc
./target/debug/buildc run examples/born_rule_normalization.bld
```
(On Windows the binary is `target/debug/buildc.exe`.)
Expected: 200 lines, each `1` or `0.99999999999999...`/`1.00000000000000...`. If any line drifts visibly (third decimal or sooner), the update is wrong; re-derive the four update lines from the physics section before proceeding.

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p buildlang --test cli born_rule_normalization_round_trips_conservation -- --nocapture`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add examples/born_rule_normalization.bld compiler/tests/cli.rs
git commit -m "feat(receipt): Born-rule normalization kernel (quantum conservation demo)"
```

### Task C2: Negative fixture (non-unitary gain)

**Files:**
- Create: `examples/born_rule_leaky.bld`
- Modify: `compiler/tests/cli.rs` (extend the C1 test with the negative-fixture half)

**Interfaces:**
- Consumes: the `--negative-fixture` flag and `FAIL_EXPECTED` receipt status.
- Produces: a paired fixture whose total probability drifts, proving the invariant can FAIL.

- [ ] **Step 1: Extend the test with the failing negative half**

In `compiler/tests/cli.rs`, inside `born_rule_normalization_round_trips_conservation`, replace the final `let _ = fs::remove_dir_all(&dir);` with the negative-fixture block followed by the cleanup:

```rust
    // NEGATIVE fixture: a non-unitary gain (g = 1.001 on every amplitude each
    // step) inflates the total probability past 1, so with `--negative-fixture`
    // it is a FAIL_EXPECTED receipt that STILL verifies.
    let fail_receipt = dir.join("leaky.json");
    let emit_fail = buildc()
        .arg("run")
        .arg(repo_example("born_rule_leaky.bld"))
        .args(["--emit-receipt"])
        .arg(&fail_receipt)
        .args(["--invariant", "conservation", "--negative-fixture", "--problem", "born-rule-leaky"])
        .output()
        .expect("emit born-rule negative fixture");
    assert!(emit_fail.status.success(), "emitting the negative fixture should succeed");
    let fail: serde_json::Value =
        serde_json::from_slice(&fs::read(&fail_receipt).expect("read FAIL receipt")).unwrap();
    assert_eq!(fail["receipt_status"], "FAIL_EXPECTED");
    assert!(fail["invariant"]["observed"]["violation_count"].as_u64().unwrap() > 0);

    let verify_fail = buildc()
        .args(["receipt", "verify"])
        .arg(&fail_receipt)
        .output()
        .expect("verify born-rule negative fixture");
    assert!(
        verify_fail.status.success(),
        "a faithfully reproduced FAIL_EXPECTED must verify (exit 0)\nstderr:\n{}",
        String::from_utf8_lossy(&verify_fail.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p buildlang --test cli born_rule_normalization_round_trips_conservation -- --nocapture`
Expected: FAIL (the fixture file does not exist yet).

- [ ] **Step 3: Write the negative fixture**

Create `examples/born_rule_leaky.bld`:

```rust
// Born-rule normalization NEGATIVE fixture (NON-UNITARY GATE).
//
// Identical to born_rule_normalization.bld EXCEPT a small gain g = 1.001 is
// applied to every amplitude each step. That gate is not unitary (its
// determinant is g^2 != 1), so it is a physically forbidden evolution: the
// total Born probability grows by g^2 each step and drifts away from 1 (reaching
// roughly 1.5 over the run), far beyond the 1e-9 tolerance. So `--invariant
// conservation` FAILs: the receipt catches a gate that violates the
// probability-conservation law unitarity guarantees.
//
// Paired negative fixture for born_rule_normalization.bld; run with
// `--negative-fixture` for FAIL_EXPECTED.

fn main() ~ Console {
    let t: f64 = 0.05;
    let c: f64 = cos(t);
    let s: f64 = sin(t);
    let g: f64 = 1.001;
    let mut a0: f64 = 1.0;
    let mut b0: f64 = 0.0;
    let mut a1: f64 = 0.0;
    let mut b1: f64 = 0.0;
    let mut step: i32 = 0;
    while step < 200 {
        let prob: f64 = a0 ** 2 + b0 ** 2 + a1 ** 2 + b1 ** 2;
        println!("{}", prob);
        // Unitary rotation, then a NON-UNITARY gain that breaks conservation.
        let a0n: f64 = g * (c * a0 + s * b1);
        let b0n: f64 = g * (c * b0 - s * a1);
        let a1n: f64 = g * (c * a1 + s * b0);
        let b1n: f64 = g * (c * b1 - s * a0);
        a0 = a0n;
        b0 = b0n;
        a1 = a1n;
        b1 = b1n;
        step = step + 1;
    }
}
```

- [ ] **Step 4: Sanity-run the fixture directly**

Run: `./target/debug/buildc run examples/born_rule_leaky.bld`
Expected: 200 lines rising from `1` toward roughly `1.49` (each step multiplies the probability by `g^2 = 1.002001`). The rise must be visible and monotone.

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p buildlang --test cli born_rule_normalization_round_trips_conservation -- --nocapture`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add examples/born_rule_leaky.bld compiler/tests/cli.rs
git commit -m "test(receipt): Born-rule negative fixture (non-unitary gain FAILs conservation)"
```

### Task C3: Document the Born-rule kernel

**Files:**
- Modify: `docs/SCIENTIFIC-RECEIPT.md`

- [ ] **Step 1: Add the kernel pair to the docs**

In `docs/SCIENTIFIC-RECEIPT.md`, find the section that lists the `conservation` member's kernels (the rotation and reaction examples). Add a short paragraph naming the new pair. Match the surrounding prose style (no em-dashes):

```markdown
The `conservation` member also carries a quantum instance. `born_rule_normalization.bld`
evolves a single qubit by a unitary X-rotation and prints the total Born probability
`sum_i |psi_i|^2` each step; unitarity keeps it at 1 to roundoff, so a `--invariant conservation`
receipt PASSes. The kernel is genuinely complex (its imaginary amplitudes carry the state after
the first step), which distinguishes it from the real 2D rotation in `conservation_rotation.bld`.
Its negative fixture `born_rule_leaky.bld` applies a non-unitary gain that inflates the
probability, so `conservation` FAILs and the receipt catches a gate that violates the
probability-conservation law. This is the checkable shadow of the Born rule; the deeper Carcassi
and Aidala entropy equivalence (AoP Brief 003) is information-theoretic, not a per-step numerical
identity, and stays out of scope for v0.
```

- [ ] **Step 2: Update the Deferred section**

In the Deferred section (around line 522), the current text mentions "named physical identities across columns". Leave the funnel-hashing sentence for Feature 2. Confirm nothing in the Deferred list now contradicts the shipped Born-rule kernel (the Born-rule normalization is shipped; only the entropy-equivalence version remains deferred). If needed, append one clause noting the entropy-equivalence Born-rule version is deferred pending a calibrated tolerance or the Wave 4 seeded-RNG builtin.

- [ ] **Step 3: Commit**

```bash
git add docs/SCIENTIFIC-RECEIPT.md
git commit -m "docs(receipt): document the Born-rule normalization kernel pair"
```

### Task C4: Optional research spike (gated, may be skipped)

**This task is a spike, not a deliverable.** Do it only if time allows and the operator wants the deeper version scoped.

- [ ] Read AoP Brief 003 in full and write a two-paragraph note (in `docs/superpowers/specs/` or the ledger) stating: the exact identity the brief proves, whether a roundoff-crisp checkable form exists beyond normalization conservation, and if not, precisely what a calibrated-tolerance or seeded-RNG frequency-convergence demo would require. Do NOT implement it. This note becomes the spec seed if the operator later greenlights it.

### Task C5: Gate, review, merge (Feature 1)

- [ ] **Step 1: Gate.** From the repo root run the full suite and the corpus, and confirm the live matrix is green: `cargo test -p buildlang` (record the new counts), then the corpus check (expected 8/8), then the live invariant matrix. Everything green before proceeding.
- [ ] **Step 2: Adversarial review.** Run a Workflow review over the Feature 1 diff (branch `merge-base` to `HEAD`): several lenses (physics correctness of the unitary update, does-the-negative-fixture-fail-for-the-right-reason, test hygiene, docs accuracy), each finding independently adversarially verified, confirmed findings only.
- [ ] **Step 3: Fix** every confirmed finding, re-running the covering CLI test after each fix.
- [ ] **Step 4: Merge** the feature branch into `main` with `--no-ff`, then push.

---

## Feature 2: Funnel-Hashing Probe-Complexity Kernel

### Design constraint (read before writing any code)

Funnel hashing (Farach-Colton, Krapivin, Kuszmaul, arXiv 2501.02305, "Optimal Bounds for Open Addressing Without Reordering") divides the table into geometrically decreasing levels and inserts greedily without reordering, achieving `O(log^2(1/delta))` worst-case expected probe complexity at load factor `1 - delta`, beating the classical `O(1/delta)` of uniform open addressing.

**Honest scope.** This kernel demonstrates the funnel's KEY PROPERTY (a sub-linear worst-case probe bound versus single-level linear probing at high load). It is a faithful-in-spirit funnel: contiguous, geometrically decreasing levels, a bounded probe budget per level, greedy descent, and a final catch-all region. It is NOT a bit-exact reproduction of the paper's optimal constant, and the kernel comment and docs must say so plainly. The result-bearing-bound concept already ships via `examples/search_bound_binary.bld`; this feature is a more ambitious instance of the same `non-negative` member, on a real data structure.

**The number-one risk is calibration and the can-it-FAIL law.** A funnel kernel proves nothing unless (a) its measured max probe count stays under the bound with margin, AND (b) the SAME key sequence under naive linear probing exceeds that bound. If linear probing does not exceed the bound at your chosen load, the demo is theatrical (a verifier that cannot fail). Tasks F1 through F3 are an empirical loop that TUNES the load factor, budget, and bound until the separation holds with margin, documenting the measured numbers. Do not skip the measurement and hardcode a guessed bound.

### Language capabilities (verified at plan-authoring time, main `90bb775`)

All confirmed present:
- Integer modulo `%` (`BinOp::Rem`) and integer division `/`.
- `Vec<i64>` via `vec_new_i64` / `vec_push_i64` / `vec_get_i64` / `vec_len` (no type annotation needed: `let mut t = vec_new_i64();` infers `Vec<i64>`).
- Indexed assignment `table[slot] = value` (lowers to `build_hvec_set_i64`).
- Casts `x as i64` / `x as i32` / `x as f64`.

The hash uses odd multiplier constants that fit in `i32` (so integer literals are unambiguous), with the key cast to `i64` before multiplying to avoid overflow, then reduced modulo a PRIME level size (prime moduli avoid the low-bit degeneracy that a power-of-two modulus suffers with a multiplicative hash).

### Files

- Create: `examples/funnel_probe.bld` (positive kernel)
- Create: `examples/funnel_probe_linear.bld` (negative fixture, single-level linear probing)
- Modify: `compiler/tests/cli.rs` (add one round-trip test after the Feature 1 test)
- Modify: `docs/SCIENTIFIC-RECEIPT.md` (funnel entry under the `non-negative` member; update Deferred)

### Interfaces

- Consumes: the `non-negative` invariant (tolerance 1e-9, violation when `s[k] < -tol`), the CLI surface `buildc run <file> --emit-receipt <out> --invariant non-negative [--negative-fixture] [--metric slack] [--problem <label>]`, `buildc receipt verify <out>`, and the receipt JSON fields `invariant.name` (`non_negative`), `receipt_status`, `invariant.observed.violation_count`.
- Produces: two example kernels (with a calibrated `bound` documented in their comments) and one CLI test.

### Task F1: Funnel kernel and empirical probe measurement

**Files:**
- Create: `examples/funnel_probe.bld`

**Interfaces:**
- Produces: a funnel-hashing kernel that inserts a fixed key sequence and prints `bound - probes` (the slack) per insertion.

- [ ] **Step 1: Write the funnel kernel**

Create `examples/funnel_probe.bld`. `bound` and `load` are placeholders to be calibrated in Task F3; start with the values shown.

```rust
// Funnel-hashing probe-bound kernel (RESULT-BEARING): a leveled open-addressing
// scheme keeps its worst-case probe count under a proven-in-spirit bound.
//
// Funnel hashing (arXiv 2501.02305, Farach-Colton, Krapivin, Kuszmaul) divides
// the table into geometrically decreasing levels and inserts greedily without
// reordering, achieving O(log^2(1/delta)) worst-case expected probes at load
// factor 1 - delta, beating the O(1/delta) of uniform probing. This kernel is a
// faithful-in-spirit funnel (contiguous decreasing levels, a bounded probe
// budget per level, greedy descent, a final catch-all), not a bit-exact
// reproduction of the paper's optimal constant. It inserts a fixed key sequence
// and prints the SLACK = bound - probes each insertion. A funnel that holds its
// bound never goes negative, so `--invariant non-negative` PASSes.
//
// This is the family's ALGORITHMIC member applied to a real data structure (a
// richer instance than search_bound_binary.bld). The bound below is calibrated
// empirically so the funnel stays under it while the naive linear-probing
// fixture (funnel_probe_linear.bld) blows past it at the same load.

fn main() ~ Console {
    let n: i32 = 1024;
    let empty: i64 = -1;
    let mut table = vec_new_i64();
    let mut i: i32 = 0;
    while i < n {
        vec_push_i64(table, empty);
        i = i + 1;
    }

    // Four bounded levels of geometrically decreasing size occupy [0, 960):
    // L0=[0,512) L1=[512,768) L2=[768,896) L3=[896,960). A key probes at most
    // `budget` slots per level, descending when its budget is exhausted. Any key
    // still unplaced falls back to a full-table linear probe (guaranteed to
    // succeed while load < n), which is counted honestly and stays rare at a
    // calibrated load.
    let budget: i32 = 4;
    let a: i64 = 1000003;   // odd multiplier (prime), fits i32
    let b: i64 = 999983;    // odd per-level decorrelator (prime), fits i32

    let bound: i32 = 24;    // CALIBRATE in Task F3.
    let load: i32 = 768;    // ~75% load; CALIBRATE in Task F3.

    let mut k: i32 = 0;
    while k < load {
        let mut probes: i32 = 0;
        let mut placed: i32 = 0;

        let mut off: i32 = 0;
        let mut size: i32 = 512;
        let mut level: i32 = 0;
        while level < 4 {
            if placed == 0 {
                let start: i32 = (((k as i64) * a + (level as i64) * b) % (size as i64)) as i32;
                let mut j: i32 = 0;
                while j < budget {
                    if placed == 0 {
                        let slot: i32 = off + (start + j) % size;
                        probes = probes + 1;
                        let occ: i64 = vec_get_i64(table, slot);
                        if occ == empty {
                            table[slot] = k as i64;
                            placed = 1;
                        }
                    }
                    j = j + 1;
                }
            }
            off = off + size;
            size = size / 2;
            level = level + 1;
        }

        // Final catch-all: full-table linear probe, guaranteed to terminate
        // while load < n. Rare at a calibrated load.
        if placed == 0 {
            let fstart: i32 = (((k as i64) * a) % (n as i64)) as i32;
            let mut j: i32 = 0;
            while placed == 0 {
                let slot: i32 = (fstart + j) % n;
                probes = probes + 1;
                let occ: i64 = vec_get_i64(table, slot);
                if occ == empty {
                    table[slot] = k as i64;
                    placed = 1;
                }
                j = j + 1;
            }
        }

        // Slack against the calibrated funnel bound; non-negative iff the funnel held.
        println!("{}", (bound - probes) as f64);
        k = k + 1;
    }
}
```

- [ ] **Step 2: Build and run, then measure the probe distribution**

Run:
```
cargo build -p buildlang --bin buildc
./target/debug/buildc run examples/funnel_probe.bld > /tmp/funnel_out.txt
```
Then inspect the output. Each line is `bound - probes = 24 - probes`. Compute the observed MAX probes as `24 - min(line)` and the fallback rate (how many lines are small, indicating the catch-all triggered). Record the observed max probes for Task F3.

Acceptance for this step: the kernel compiles, runs to completion (no hang: the catch-all guarantees termination), and prints exactly `load` (768) lines. If it hangs, the catch-all guard is wrong; do not proceed until it terminates. Note the observed max probes.

- [ ] **Step 3: Commit the kernel (bound not yet calibrated)**

```bash
git add examples/funnel_probe.bld
git commit -m "feat(receipt): funnel-hashing probe-bound kernel (uncalibrated bound)"
```

### Task F2: Linear-probing negative fixture and its measurement

**Files:**
- Create: `examples/funnel_probe_linear.bld`

- [ ] **Step 1: Write the linear-probing fixture**

Create `examples/funnel_probe_linear.bld`. Keep `n`, `load`, `a`, and `bound` identical to the funnel kernel (they will be calibrated together in F3).

```rust
// Funnel-hashing NEGATIVE fixture (SINGLE-LEVEL LINEAR PROBING).
//
// Identical setup to funnel_probe.bld (same table size, same key sequence, same
// bound) EXCEPT insertion is naive single-level linear probing over the whole
// table: hash the key, then scan forward one slot at a time until an empty slot
// is found. At high load linear probing forms long clusters, so the worst-case
// probe count for some insertion exceeds the funnel bound and the printed slack
// goes negative. So `--invariant non-negative` FAILs: the receipt catches an
// open-addressing scheme that does not hold the funnel's sub-linear bound.
//
// Paired negative fixture for funnel_probe.bld; run with `--negative-fixture`
// for FAIL_EXPECTED.

fn main() ~ Console {
    let n: i32 = 1024;
    let empty: i64 = -1;
    let mut table = vec_new_i64();
    let mut i: i32 = 0;
    while i < n {
        vec_push_i64(table, empty);
        i = i + 1;
    }

    let a: i64 = 1000003;
    let bound: i32 = 24;    // SAME bound as funnel_probe.bld; CALIBRATE in F3.
    let load: i32 = 768;    // SAME load as funnel_probe.bld.

    let mut k: i32 = 0;
    while k < load {
        let mut probes: i32 = 0;
        let start: i32 = (((k as i64) * a) % (n as i64)) as i32;
        let mut j: i32 = 0;
        let mut placed: i32 = 0;
        while placed == 0 {
            let slot: i32 = (start + j) % n;
            probes = probes + 1;
            let occ: i64 = vec_get_i64(table, slot);
            if occ == empty {
                table[slot] = k as i64;
                placed = 1;
            }
            j = j + 1;
        }
        println!("{}", (bound - probes) as f64);
        k = k + 1;
    }
}
```

- [ ] **Step 2: Build and run, then measure**

Run: `./target/debug/buildc run examples/funnel_probe_linear.bld > /tmp/linear_out.txt`
Compute the observed MAX probes as `24 - min(line)`. Record it. This is the linear-probing worst case at the current load.

- [ ] **Step 3: Commit the fixture (bound not yet calibrated)**

```bash
git add examples/funnel_probe_linear.bld
git commit -m "feat(receipt): linear-probing negative fixture for funnel bound (uncalibrated)"
```

### Task F3: Calibrate the bound (the can-it-FAIL gate)

**Files:**
- Modify: `examples/funnel_probe.bld`
- Modify: `examples/funnel_probe_linear.bld`

This task has no code template because its output is a MEASURED number. Follow the procedure; the acceptance criterion is a real separation with margin.

- [ ] **Step 1: Compare the two measured maxes.** From F1 and F2 you have `funnel_max` and `linear_max` at load 768. The required relationship is:

```
funnel_max  <  bound  <=  linear_max
```

with margin (aim for `bound >= funnel_max + 4` and `bound <= linear_max - 4`).

- [ ] **Step 2: If no separating bound exists at load 768, tune.** Adjust in this order and re-run both kernels after each change:
  1. Raise `load` toward higher occupancy (try 832, then 896). Higher load widens linear probing's clusters faster than it hurts the funnel, so the separation grows. Keep `load < n` (1024) with comfortable headroom (do not exceed ~950; the catch-all needs empty slots).
  2. If the funnel's own max is spiking because the catch-all triggers too often, raise `budget` (try 6, then 8) so more keys place in bounded levels.
  3. Only after a clean separation exists, set `bound` to the midpoint (rounded) between `funnel_max` and `linear_max`.

- [ ] **Step 3: Lock the calibrated values.** Set the SAME `bound` and `load` (and `budget` if you changed it) in BOTH `examples/funnel_probe.bld` and `examples/funnel_probe_linear.bld`. Update each kernel's comment to state the measured numbers, for example: "At load L (X%) the funnel's max probe count is F and linear probing's is G; bound = B sits between them with margin."

- [ ] **Step 4: Re-run both and confirm the separation holds.**

Run:
```
./target/debug/buildc run examples/funnel_probe.bld | sort -n | head -1
./target/debug/buildc run examples/funnel_probe_linear.bld | sort -n | head -1
```
Expected: the funnel's minimum slack line is `>= 0` (with margin), and the linear fixture's minimum slack line is `< 0`. If either fails, return to Step 2.

- [ ] **Step 5: Commit the calibration.**

```bash
git add examples/funnel_probe.bld examples/funnel_probe_linear.bld
git commit -m "feat(receipt): calibrate funnel bound (funnel holds, linear probing exceeds)"
```

### Task F4: CLI round-trip test

**Files:**
- Modify: `compiler/tests/cli.rs`

- [ ] **Step 1: Write the failing test**

Add to `compiler/tests/cli.rs` after the Feature 1 test. This mirrors `non_negative_invariant_round_trips_a_result_bearing_bound` exactly.

```rust
#[test]
fn funnel_hashing_round_trips_a_probe_bound() {
    if !c_backend_ready() {
        eprintln!("skipping funnel_hashing_round_trips_a_probe_bound: C backend not ready");
        return;
    }
    let dir = std::env::temp_dir().join(format!("buildlang_sci_funnel_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create funnel fixture dir");

    // POSITIVE: the funnel's measured probe count stays under its calibrated
    // bound, so the printed slack (bound - probes) stays non-negative and
    // `--invariant non-negative` PASSes.
    let pass_receipt = dir.join("funnel.json");
    let emit_pass = buildc()
        .arg("run")
        .arg(repo_example("funnel_probe.bld"))
        .args(["--emit-receipt"])
        .arg(&pass_receipt)
        .args([
            "--invariant",
            "non-negative",
            "--metric",
            "slack",
            "--problem",
            "funnel-hashing-probe-bound",
        ])
        .output()
        .expect("emit funnel PASS receipt");
    assert!(
        emit_pass.status.success(),
        "emitting the funnel PASS receipt should succeed\nstderr:\n{}",
        String::from_utf8_lossy(&emit_pass.stderr)
    );
    let pass: serde_json::Value =
        serde_json::from_slice(&fs::read(&pass_receipt).expect("read PASS receipt")).unwrap();
    assert_eq!(pass["invariant"]["name"], "non_negative");
    assert_eq!(pass["receipt_status"], "PASS");
    assert_eq!(pass["invariant"]["observed"]["violation_count"], 0);

    let verify_pass = buildc()
        .args(["receipt", "verify"])
        .arg(&pass_receipt)
        .output()
        .expect("verify funnel PASS receipt");
    assert!(
        verify_pass.status.success(),
        "the funnel PASS receipt must verify\nstderr:\n{}",
        String::from_utf8_lossy(&verify_pass.stderr)
    );

    // NEGATIVE fixture: single-level linear probing exceeds the same bound, so
    // the slack goes negative. With `--negative-fixture` it is a FAIL_EXPECTED
    // receipt that STILL verifies.
    let fail_receipt = dir.join("linear.json");
    let emit_fail = buildc()
        .arg("run")
        .arg(repo_example("funnel_probe_linear.bld"))
        .args(["--emit-receipt"])
        .arg(&fail_receipt)
        .args([
            "--invariant",
            "non-negative",
            "--negative-fixture",
            "--metric",
            "slack",
            "--problem",
            "linear-probing-probe-bound",
        ])
        .output()
        .expect("emit funnel negative fixture");
    assert!(emit_fail.status.success(), "emitting the negative fixture should succeed");
    let fail: serde_json::Value =
        serde_json::from_slice(&fs::read(&fail_receipt).expect("read FAIL receipt")).unwrap();
    assert_eq!(fail["receipt_status"], "FAIL_EXPECTED");
    assert!(fail["invariant"]["observed"]["violation_count"].as_u64().unwrap() > 0);

    let verify_fail = buildc()
        .args(["receipt", "verify"])
        .arg(&fail_receipt)
        .output()
        .expect("verify funnel negative fixture");
    assert!(
        verify_fail.status.success(),
        "a faithfully reproduced FAIL_EXPECTED must verify (exit 0)\nstderr:\n{}",
        String::from_utf8_lossy(&verify_fail.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cargo test -p buildlang --test cli funnel_hashing_round_trips_a_probe_bound -- --nocapture`
Expected: PASS. (The kernels already exist and are calibrated, so this should pass on the first run. If the PASS receipt does not PASS, or the negative fixture does not FAIL_EXPECTED, the calibration in F3 is wrong; return to F3.)

- [ ] **Step 3: Commit**

```bash
git add compiler/tests/cli.rs
git commit -m "test(receipt): funnel-hashing probe-bound CLI round-trip"
```

### Task F5: Document the funnel kernel

**Files:**
- Modify: `docs/SCIENTIFIC-RECEIPT.md`

- [ ] **Step 1: Add the funnel entry under the `non-negative` member**

Add a paragraph next to the binary-search description. Match the style, no em-dashes:

```markdown
The `non-negative` member also carries a data-structure instance. `funnel_probe.bld` implements
funnel hashing (arXiv 2501.02305): a leveled open-addressing scheme with geometrically decreasing
levels and a bounded probe budget per level. It inserts a fixed key sequence and prints the slack
`bound - probes` each insertion; the funnel holds its calibrated bound, so `--invariant
non-negative` PASSes. Its negative fixture `funnel_probe_linear.bld` runs the same key sequence
under naive single-level linear probing, whose clusters exceed the bound at the same load, so
`non-negative` FAILs. This is a faithful-in-spirit funnel that exhibits the sub-linear worst-case
probe bound, not a bit-exact reproduction of the paper's optimal O(log^2(1/delta)) constant.
```

- [ ] **Step 2: Update the Deferred section**

In the Deferred section (around line 532), the sentence currently says "a full funnel-hashing (arXiv 2501.02305) probe-complexity kernel beyond the binary-search slack demo" is a follow-on. That follow-on is now shipped: remove or rewrite that clause so the Deferred list no longer claims funnel hashing is outstanding. Keep any remaining deferred items (richer relations, seeded stochastic receipts) intact.

- [ ] **Step 3: Commit**

```bash
git add docs/SCIENTIFIC-RECEIPT.md
git commit -m "docs(receipt): document funnel-hashing kernel, clear it from Deferred"
```

### Task F6: Gate, review, merge (Feature 2)

- [ ] **Step 1: Gate.** From the repo root: `cargo test -p buildlang` (record counts), corpus check (expected 8/8), live invariant matrix green.
- [ ] **Step 2: Adversarial review.** Run a Workflow review over the Feature 2 diff with lenses tuned to this feature: (a) is the funnel genuinely sub-linear or does the catch-all dominate (re-derive from the recorded F1/F2 measurements), (b) does the negative fixture fail for the RIGHT reason (linear clustering, not a coincidence of the hash constant), (c) is the bound calibrated with real margin or fitted to pass, (d) test hygiene, (e) docs accuracy and no overclaiming of the paper's constant. Independently verify each finding.
- [ ] **Step 3: Fix** every confirmed finding, re-running the funnel CLI test after each fix.
- [ ] **Step 4: Merge** the feature branch into `main` with `--no-ff`, then push.

---

## Wave 4 Roadmap (backlog, NOT part of this plan)

After the two kernels land, Wave 4 is the next work packet. It is about making the verifier harder to fool, not adding more invariants. Source of truth: `docs/superpowers/plans/2026-07-01-research-uplift-backlog.md`. Items, roughly in dependency order:

1. **`receipt verify --self-test`.** Auto-tamper one sealed field per receipt (source hash, each invariant field, args, column_count) and assert each tamper yields its distinct failure class (SEAL_MISMATCH, FIELD_CONTRACT_VIOLATION, REDERIVATION_FAILED, and so on). This proves the verifier's failure taxonomy is real, extending the can-it-FAIL law from kernels to the verifier itself.
2. **`corpus verify --self-test`** plus corpus expected-classification manifests: each corpus entry declares its expected verdict, and the corpus check asserts the classification, so a silent verdict regression fails CI.
3. **Receipt chaining (`receipt chain-verify`).** Verify an ordered sequence of receipts as a chain (each seals the prior's digest), so a multi-stage computation carries one re-checkable provenance thread.
4. **Backend-admission policy.** An experimental backend (LLVM JIT, WASM, and so on) is admitted to "production-verified" only after it reproduces the C backend's receipt verdicts across the corpus. This turns backend maturity into an earned, witnessed status rather than a hand-set label.
5. **Rust re-run lane / maturity demotion.** A second re-run path (the Rust backend) for receipts, with automatic maturity demotion if a backend stops reproducing.
6. **Seeded stochastic receipts.** Requires a seeded-RNG builtin (not yet in the language). Unlocks Monte Carlo kernels and the frequency-convergence version of the Born-rule demo (Feature 1's deferred deeper form).

## Lyapunov: NEIGHBOR-OWNED, do NOT build

A lyapunov-decrease certificate (a control-theory stability witness) belongs to the operator's proof-surface wedge #10 (`HANDOFF-robotics-cybernetics-wedge10`). It is listed here only so it is not accidentally rebuilt inside buildlang. If a stability-certificate need arises in buildlang, coordinate with that surface rather than adding a new invariant here.

---

## Self-Review (run against this plan before executing)

- **Spec coverage.** Both features requested (Carcassi Born-rule identity kernel; full funnel-hashing kernel) have complete task sequences with runnable code and CLI round-trip tests. The remaining-roadmap question is answered in the Roadmap section. Wave 4 and lyapunov are scoped as out of this plan.
- **Placeholder scan.** The only intentionally-uncalibrated values are the funnel `bound`/`load`/`budget`, which are measured quantities by design; Task F3 is the explicit procedure that fixes them, with a concrete acceptance criterion (`funnel_max < bound <= linear_max` with margin). No "TBD", no "add error handling", no undefined symbols. Every code step shows complete code.
- **Type/name consistency.** Invariant names used in tests match the registry: `conservation` maps to `conserved_quantity_constant`, `non-negative` maps to `non_negative`. Receipt fields (`invariant.name`, `receipt_status`, `invariant.observed.violation_count`) and CLI flags (`--emit-receipt <out>`, `--invariant`, `--negative-fixture`, `--metric`, `--problem`) match the verified `non_negative_invariant_round_trips_a_result_bearing_bound` test. Kernel builtins (`cos`, `sin`, `**`, `vec_new_i64`/`vec_push_i64`/`vec_get_i64`, indexed assignment) are all verified present at main `90bb775`.
- **Can-it-FAIL law.** Both features ship a negative fixture that FAILs its invariant for a stated, correct reason (a non-unitary gate for Born-rule; single-level linear-probe clustering for funnel), and the CLI test asserts `FAIL_EXPECTED` with `violation_count > 0`.

## Execution Handoff

This plan is a fresh-session handoff authored at the Phase D checkpoint. It is NOT to be executed by its author in the authoring session. When a fresh session picks it up:

- Start by reading the Roadmap section and re-running the gate to establish the true current baseline.
- Execute Feature 1 (Tasks C1 through C5), then Feature 2 (Tasks F1 through F6), each with the standard cadence (build -> gate -> adversarial review -> fix -> merge -> push).
- Recommended sub-skill: superpowers:subagent-driven-development (fresh implementer per task, task review between tasks, whole-branch review at the end). Or superpowers:executing-plans for inline batch execution with checkpoints.
- Do NOT publish to crates.io; the next release (1.2.0) is operator-gated.
