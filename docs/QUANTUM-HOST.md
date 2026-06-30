# BuildLang as a Quantum Host — Direction & Spike

> Status: **exploratory spike** (2026-06-30). This documents a direction, an
> honest assessment of fit, a working first demonstration, and the concrete
> path. Nothing here is production. The framing is deliberate: BuildLang is not
> trying to *be* Qiskit; it is exploring whether it can be the **unifying
> classical-quantum host** — one effect-typed language where classical control
> and quantum kernels live together, with the quantum parts lowering to QIR /
> OpenQASM and the classical parts to C/LLVM.

## Why the fit is real (and where it is not)

**Architectural advantages that map onto quantum:**

1. **Effects.** Measurement is literally a side effect (it collapses state);
   decoherence and classical feedback are effects too. An effect-typed language
   is one of the few designs that can model quantum operations with discipline -
   the same lineage as Q#, Silq, Quingo. A `~ Quantum` / `~ Measure` effect row
   is a natural extension of the existing `~ Console` / `~ Clock` rows.
2. **QIR is the on-ramp, and we have an LLVM path.** QIR (Quantum Intermediate
   Representation) is *LLVM IR plus quantum intrinsics* - the emerging
   vendor-neutral substrate. BuildLang already has an LLVM backend and an
   MIR→backend pipeline; a QIR backend is "LLVM backend + quantum intrinsic
   emission." An OpenQASM 3 text backend is the same shape as the existing C/Rust
   text backends.
3. **The transpiler thesis is the right frame.** Hosting classical control +
   quantum kernels in one language, lowering each part to its substrate, *is* the
   universal-transpiler thesis applied to a new target.

**The one load-bearing gap — linear types.** Quantum programming requires the
**no-cloning** rule enforced in the type system: a qubit cannot be silently
copied or dropped. That needs **linear (or strict affine) types**. BuildLang's
move/borrow analysis is affine-*ish* and a real head start, **but a probe this
session showed it does not yet reject reusing a by-value value** (passing a
struct to a function twice type-checks clean). So no-cloning is *not* enforced
today. This is the single most important thing to build first.

**Honest limits.** The hard part of quantum compilation is not the frontend - it
is circuit optimization, qubit routing to hardware topology, gate synthesis, and
pulse-level control. The plan is to **lower to QIR and hand off** to vendor
toolchains (tket, Catalyst/MLIR-quantum), not to reimplement them. And BuildLang
is research-grade; hardware vendors demand extreme correctness, which is why the
ongoing codegen-hardening loop matters.

## The spike — a Bell state, simulated in BuildLang today

`examples/quantum/bell.bld` builds the canonical entangled pair
`(|00> + |11>) / sqrt(2)` with an `H` gate and a `CNOT`, as an explicit 2-qubit
state-vector simulation, then measures it.

```
q0 --[H]--*--      prepares (|00> + |11>)/sqrt(2)
q1 -------(X)--    (control q0, target q1)
```

Verified end-to-end (BuildLang → C → MSVC `cl` → run):

```
P(00) permille 500     P(01) permille 0
P(10) permille 0       P(11) permille 500
shots 00 1470   shots 01 0   shots 10 0   shots 11 530
```

The probabilities are *exactly* the Bell state (50/50 over {00,11}, zero for
{01,10}), and 2000 measured shots land **only** on the correlated outcomes
{00,11} - never the anti-correlated {01,10}. That correlation is the signature of
entanglement, expressed and executed in BuildLang. (The 1470/530 shot split is
just the cheap single-seed LCG RNG; the deterministic probabilities are the
rigorous proof. A real RNG / repeated state-prep would balance it.)

This proves the **expression + execution** half: BuildLang can host quantum
algorithms as simulation right now, using only existing primitives (`Vec<f64>`,
indexed assignment, arithmetic, the `~ Clock` effect).

## The path (in dependency order)

1. **Linear types (no-cloning)** - the decisive first brick. **SHIPPED
   (2026-06-30).** An opt-in `#[linear]` attribute marks a struct/enum whose
   values may be moved/consumed **at most once**; the type checker rejects
   use-after-consume (the no-cloning rule). Borrows (`&q`) do not consume;
   ordinary types keep copy-like reuse (backward compatible, full suite green).
   Coverage: let-bound locals, **function parameters**, branch joins
   (`if`/`if let`/`match`, conservative union-of-consumed), and a loop guard
   (consuming an outer linear value inside a loop is rejected). A **containment
   rule** rejects a non-`#[linear]` aggregate holding a linear field, so the
   resource cannot be laundered out of an untracked wrapper. Built on the
   existing move/borrow analysis; verified by three adversarial passes that
   closed 14 compositional escape classes (each a regression test). **Status:
   experimental, not yet fully sound** - a third pass still found a few open
   classes (pattern-match-through-a-borrow, enum-variant shorthand init, generic
   deref/result, match-guard fall-through, borrow-after-move). Full no-cloning
   soundness needs an affine/borrow checker on MIR. Honest scope is in
   `docs/LINEAR-TYPES.md`. *Also deferred to brick 1b: drop-without-consume
   ("must use") enforcement and per-path branch tracking.*
2. **`~ Quantum` effect + gate intrinsics** - `qubit() -> Qubit`, `h(Qubit)`,
   `cnot(Qubit, Qubit)`, `measure(Qubit) -> i32`, expressed in MIR/stdlib, with
   measurement carrying the effect.
3. **A simulator backend** - generalize the spike's hand-written 2-qubit
   state-vector into an n-qubit runtime so gate intrinsics actually execute.
4. **A QIR backend** - reuse the LLVM path, emit quantum intrinsics. This is the
   real-hardware on-ramp.
5. **An OpenQASM 3 text backend** - same shape as the C/Rust text backends, for
   interop with the Python-dominated ecosystem.
6. **Defer optimization / routing** to existing QIR/OpenQASM consumers.

Steps 1–3 are language + runtime work we can do here; 4–5 reuse machinery that
already exists in the compiler.

## Bottom line

Today: there are still no `qubit`, gate, or measurement *intrinsics* in the
compiler - but the decisive prerequisite, **no-cloning via linear types, now
exists and is enforced** (brick 1, shipped 2026-06-30), the spike shows the
language can already *express and run* a quantum circuit, and the design
(effects + MIR + LLVM/multi-target transpiler) is a well-suited classical
foundation to grow a hybrid quantum host. The same linear-type machinery is the
shared substrate for the fin-sec (settlement-obligation safety) and blockchain
(no-double-spend assets) directions. The next brick is the `~ Quantum` effect
with `qubit()` / `h` / `cnot` / `measure` gate intrinsics over an n-qubit
state-vector runtime (step 2).
