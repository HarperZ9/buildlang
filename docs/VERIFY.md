# Laws and contracts

BuildLang states what code must do beside the code, and `buildc` checks it. A `law` is a claim over
all inputs; `requires` and `ensures` are a function's contract. `buildc verify` gives each claim one of
three verdicts: proved, refuted with a concrete counterexample, or unknown with the reason. `buildc
check` fails on a refuted claim (BL0801), so a code change that breaks a spec breaks the build.

```
law add_comm(a: u64, b: u64) { a + b == b + a }

fn damp(e: i64) -> i64 requires e >= 0 ensures result <= e && result >= 0 {
    e - e / 4
}

law damp_twice(e: i64) requires e >= 0 { damp(damp(e)) <= e }
```

```
buildc verify laws.bld          # one line per claim
buildc verify --json laws.bld   # the buildlang-verify/v1 report
buildc check laws.bld           # fails with BL0801 on any counterexample
```

## Syntax

- `law name(params) [requires P] { body }` declares a claim. The body is a `bool` expression and may
  use `let`, `if`, `match` and calls. A law is an ordinary function returning `bool`, so a program can
  also call it.
- `fn f(params) -> T requires P ensures Q { .. }` adds a contract. `Q` names the return value
  `result`. Several clauses of each kind are joined with `&&`. Contracts apply to free, non-generic
  functions.
- `law`, `requires`, `ensures` and `result` are contextual: they keep working as ordinary names
  everywhere else.

The compiler lowers each clause to a hidden function returning `bool`, so the type checker checks
specs with its usual diagnostics.

## What a verdict means

Plain integer arithmetic traps on overflow, so every run that finishes computed the exact value. A
claim is **proved** when, for every input inside the declared parameter types and the `requires`,
every evaluation that finishes makes it true. Inputs on which the claim traps are listed separately
under `traps`; they are never folded into "proved". For an `ensures`, the claim is "if `requires`
holds and the body finishes with `result`, then `ensures` holds". Each call to a function with a
`requires` adds the obligation that the clause holds at that call.

**Refuted** carries an input on which the claim is false. The concrete evaluator ran that input
before it is reported, so a refutation never rests on the prover. **Unknown** carries the reason.

## How a claim is checked

Every claim gets two independent looks.

1. **The concrete side** runs the claim with the language's trapping arithmetic. When the domain
   (narrowed by literal bounds in `requires`) has at most 2^20 points, it runs every point, and a claim
   true on all of them is proved by exhaustion. Otherwise it runs boundary values and a fixed
   pseudo-random sample, which can refute but never prove.
2. **The symbolic side** executes loop-free code path by path. Each finishing path gives facts (each
   arithmetic step's result lies in its type's range, division identities) and a goal. Integer terms
   are put in polynomial normal form over Z; bit operators that are associative and commutative are
   flattened and sorted. The goal is checked by case splitting on atoms with Fourier-Motzkin
   elimination and integer tightening for the linear part, treating each nonlinear monomial as a free
   unknown. Every step only weakens what the prover knows, so a proof is sound; the procedure is
   incomplete. Recursive calls use the callee's `ensures` (induction over finishing runs) or stay
   uninterpreted.

If the prover claims a goal that the concrete side refutes, the verdict is `inconsistent` (BL0802),
a defect in the verifier, and the claim is not proved.

Scope today: integers up to 64 bits and `bool`; `let`, `if`, `match`, `return`, calls, casts and the
integer methods `abs`, `min`, `max`, `wrapping_add`, `wrapping_sub`, `wrapping_mul`. Loops run on the
concrete side only. Floats, heap values, effects, user-defined methods and generics are outside the subset and
leave a claim unknown. Shift amounts are taken modulo the type's width, as the C backend emits them.

No SMT solver is required. An external solver may be added as an optional executable: Z3 (MIT) and
cvc5 (BSD-3-Clause) fit AGPL-3.0 distribution because nothing is linked or bundled. No Lean or TeX
toolchain is used.

## V2: a bit-precise solver inside `buildc` (preregistered 2026-11-09, before the run)

The change: when the linear prover cannot close an obligation, it goes to a bit-precise decision
procedure that ships in `buildc` (`compiler/src/verify/smt`): the obligation is bit-blasted to CNF
with a width at which no arithmetic wraps, and solved by a CDCL SAT solver. An UNSAT answer counts
as a proof only after an independent checker replays the solver's learned clauses (reverse unit
propagation). A SAT answer is only a candidate input: the concrete evaluator runs it, and only a
run that makes the claim false is reported as a counterexample. No outside solver is downloaded,
linked or run.

Baseline on main (88ede1e), measured before the change:

- `tests/verify/smt_false.bld`, 24 false claims: 6 refuted, 18 unknown, 0 proved.
- `tests/verify/smt_true.bld`, 14 true claims: 5 proved, 9 unknown, 0 refuted.

Bounds and predictions, each a merge condition unless marked as a prediction:

- S1 (condition). The false set: 0 proved, 0 inconsistent. Prediction: all 24 refuted.
- S2 (condition). The true set: 0 refuted, 0 inconsistent. Prediction: at least 12 of 14 proved;
  `inverse_round_trip` (two chained 64-bit multiplications) may stay unknown within the conflict
  budget.
- S3 (condition). The existing verdicts in `compiler/tests/verify_laws.rs` and the false-accept set
  in `src/verify/tests.rs` are unchanged.
- S4 (condition). A planted encoder fault (`drop-carry`: every adder drops its carry) is caught by the
  differential check that compares the encoder with the concrete bit semantics on random terms. The
  same check passes without the fault.
- S5 (condition). The replay checker refuses a forged learned clause and a proof that stops short.
- S6 (condition). The SAT solver agrees with brute force on 300 random 3-SAT formulas, and every
  UNSAT answer among them replays.

## Laws in the repository

`tests/verify` holds three files whose expected verdicts are fixed in `compiler/tests/verify_laws.rs`.

**Bend 2's published laws, ported** (`bend_laws.bld`). Source: `github.com/bendlang/bend` at
9a4223d, Apache-2.0. Bend states these over unbounded `Nat`; here each runs over a fixed-width type.

| Bend law | Form here | Verdict |
|---|---|---|
| `add_comm`, `add_assoc`, `mul_comm`, `mul_dist` (proof_numerics) | recursive `add`/`mul` as Bend writes them, inputs below 64, 16, 32 and 8 | proved, every input run |
| `divmod_ok` (proof_numerics) | recursive `divmod.go`, the pair packed in one `u32`, `n, m < 64` | proved, every input run |
| `add_comm` | recursive `add` over all of `u32` | unknown: needs induction the prover does not do |
| `add_comm`, `add_assoc`, `mul_comm`, `mul_dist`, `divmod_ok` | the language's own `u64` arithmetic | proved for every input (symbolic) |
| `tree_is_seq` (pure_par_sum) | recursive fork/join sum against the sequential sum, `d <= 6`, `i < 64` | proved, every input run |
| `last_press` (app_pong_game_2d) | `Pong.press` on four boolean keys, any `u32` key code | proved for every input (symbolic) |

Not ported: the laws over trees, lists, typed expressions, events and strings (`pure_par_sort`,
`proof_insertion_sort`, `proof_typed_eval`, `pure_hvm5_mini`, the io and app demos, `esc_quits`),
because heap data is outside the subset.

**BuildLang's own laws** (`own_laws.bld`): the DETMATH integer rules (`wrapping_mul` commutative and
associative, xor laws, the int8 conversion round trip), the trace codec's integer pieces (zigzag and
byte packing round trips, proved by exhaustion), and conservation laws from the invariant family
(atom balance of `A + B <=> C`, a bounded averaging step, a damping step that never raises energy).
DETMATH's float rules are outside the subset. Four planted false laws in the file are refuted.

**Planted checks** (`planted.bld`): a false law refuted, a true law proved, a true recursive law
reported unknown, a false `ensures` refuted and a true one proved.

## Verified

- `cargo test` in `compiler/`: the unit tests in `src/verify/tests.rs`, including a false-accept set of
  ten false laws whose counterexamples are rare; the prover accepts none. A mutant prover (a
  Fourier-Motzkin step that treats `0 >= 0` as a contradiction) fails that set.
- `compiler/tests/verify_laws.rs`: every verdict in the three files above, and `buildc check` failing
  with BL0801 on the planted file and passing on the Bend file.

Nothing here compares BuildLang with Bend or with any other verifier on a shared benchmark.
