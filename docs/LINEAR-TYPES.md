# Linear Types (`#[linear]`) — Status & Honest Scope

> Status: **experimental, not yet production-sound** (2026-06-30).
> Opt-in. Off by default. No effect unless you write `#[linear]`.

`#[linear]` marks a struct or enum whose values are a tracked **resource**: a
value of that type may be **moved/consumed at most once** (no-cloning). This one
type-system property is the shared foundation for three directions — quantum
qubit no-cloning, on-chain no-double-spend, and fin-sec resource-handle safety.

```
#[linear]
struct Coin { value: i64 }
fn spend(c: Coin) -> i64 { c.value }
fn main() ~ Console {
    let coin = Coin { value: 100 };
    let a = spend(coin);
    let b = spend(coin);   // rejected: "use of linear value `coin` after it was consumed"
}
```

Ordinary (non-`#[linear]`) types are entirely unaffected: they keep copy-like
reuse. Borrows (`&q`) never consume.

## Design: sound-over-complete by conservative rejection

The analysis tracks consumption of **directly named** linear locals and
parameters (a live/consumed slot per binding, with `if`/`match` branch merges and
a loop guard). Because a value can appear in many compositional positions the
slot-tracker cannot follow (aggregates, generics, closures, deref, ...), the
checker takes the conservative stance: **a linear value is rejected anywhere it
could escape tracking**, even if a particular use would have been safe. It favors
never permitting a clone over accepting every safe program.

## What is enforced (verified across adversarial passes)

Each item below was a confirmed exploit that is now rejected, with a regression
test in `compiler/src/types/check.rs` (`linear_*`):

- Direct double-use of a local or **parameter** (`spend(coin); spend(coin)`).
- Move via `let` then reuse; use in a branch then after (conservative union).
- Storing a linear in a **tuple / array / array-repeat** (`(coin, _)`, `[coin; n]`).
- Passing a linear to a **generic / mismatched parameter** (`id<T>(coin)`).
- Storing a linear in a **generic or non-`#[linear]` struct field** (`Holder<T>{item:coin}`),
  and the declaration-time **containment rule** (a non-linear aggregate may not
  declare a linear field).
- A **generic constructor / `Some`/`Ok`** payload, and a **builtin** arg (`vec_push`).
- **Deref** out of a reference (`let a = *r`).
- A **closure** that captures and consumes an outer linear.
- An inner-block **shadow** reviving a consumed outer binding.
- A **field read through a reference** (`w.coin` where `w: &Wrap`).
- A **struct shorthand** field init (`Wallet { coin }`) consumes the moved local.
- A **`while`/`while let` condition** that consumes an outer linear (re-evaluated).
- A **method call on a borrowed linear receiver** (`(&coin).burn()`).

## Known-open (NOT yet sound — do not rely on no-cloning here)

A third adversarial pass (2026-06-30) found these still-open classes. They are
the reason this feature is labeled experimental:

1. **Pattern-match binding through a borrow** — `match &w { Wallet { coin: c } => spend(c) }`
   mints a fresh owned linear `c` from a `&Wallet` without consuming `w`.
   (`bind_pattern` ignores that the scrutinee is a reference.)
2. **Enum-variant shorthand field init** — `Wrap::Has { item }` neither consumes
   nor rejects the moved linear (the enum-literal path lacks the guard the
   struct-literal path has).
3. **Generic deref / generic-call result** — `fn deref_any<T>(r: &T) -> T { *r }`
   launders a linear out of a borrow inside a generic body; more generally, a
   generic call whose *result* unifies to a linear type is not tracked at the
   bind site (the bind-time type is an unresolved var).
4. **Match-guard fall-through** — a guard that consumes a linear, then a later arm
   body consumes it again on the same runtime path (guards run even when the arm
   is not taken).
5. **Borrow-after-move** — reading `&q` after `q` was moved (use-after-move of
   borrowed data; severity is contract-dependent — borrows are defined as
   non-consuming, so this is arguably a separate borrow-lifetime concern).

## Path to true soundness

The slot-tracker on the AST type checker has gone about as far as conservative
rejection can cheaply reach. Full no-cloning soundness wants a proper
**affine/linear move-and-borrow checker**, most naturally on **MIR** (where
projections, moves, and borrows are explicit) rather than on the AST. That is a
deliberate, multi-iteration piece of work (cf. Rust's borrow checker, Linear
Haskell) and is the correct home for: move-out-of-borrow (covers cases 1, 3, 5),
per-path guard/arm tracking (case 4), enum-literal moves (case 2), and the
deferred **drop-without-consume** ("must use") half of linearity.

## Methodology note

This status is the product of three **adversarial verification passes** (a
fan-out of attack agents by lens + independent per-finding verifiers, each
running `buildc check` against the built compiler). Every "enforced" item above
is a confirmed-and-fixed exploit; every "known-open" item is a confirmed,
independently-verified live exploit. The honest takeaway: ship the verifier with
the feature, and do not claim soundness the verifier has not earned.
