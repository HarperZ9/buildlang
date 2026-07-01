# Linear Types (`#[linear]`) — Status & Honest Scope

> Status: **experimental — a best-effort no-cloning LINT, not a proven-sound checker** (2026-07-01).
> Opt-in. Off by default. No effect unless you write `#[linear]`. It catches the
> common cloning mistakes and every class listed under "What is enforced" below,
> but it is NOT a soundness guarantee: known residual holes remain (see "Known
> residual"). Do not rely on it as a safety guarantee for no-cloning-critical use
> until it is proven sound (a tracked, multi-iteration effort).

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

## Design: two layers (conservative AST gate + additive MIR lint)

The checker is two cooperating layers:

1. **AST conservative gate** (`types/infer.rs`, `types/check.rs`). Tracks
   consumption of **directly named** linear locals/parameters (a live/consumed
   slot per binding, with `if`/`match` branch merges and a loop guard), plus the
   declaration-time **containment rule**. It is *sound-over-complete*: a linear
   value is rejected anywhere it could escape the name-based tracker (aggregates,
   generics, closures, deref, ...), even when a particular use would be safe. This
   layer is imprecise (it over-rejects some safe compositional code) but that is a
   usability cost, not a soundness cost.

2. **MIR affine/borrow checker** (`codegen/analysis/linear.rs`, brick 2, 2026-07-01),
   built on the reusable `codegen::analysis` dataflow substrate (liveness,
   move-chains, dominators, CFG — the same substrate that powers drop insertion).
   It runs post-lowering as an **additive** gate and closes classes the name-based
   AST tracker structurally cannot follow, using place-based move/init dataflow,
   borrow-provenance tracking, an interprocedural borrow-escape fixpoint (extended
   through aggregates and indirect/fn-pointer calls), and match-idiom recognition.
   Its errors surface as compile errors with spans.

## What is enforced (verified empirically via `buildc check`)

Every item is a confirmed exploit that is now rejected, with a regression test.
The AST gate and the MIR checker together cover:

- Direct double-use of a local or **parameter**; move via `let` then reuse;
  branch-consume-then-use (conservative union); loop-body consume.
- Storing a linear in a **tuple / array / repeat**, a **generic / non-`#[linear]`
  field**, a **generic constructor / `Some`/`Ok`**, a **builtin** collection; the
  declaration-time **containment rule**.
- **Move out of a shared borrow** (`let a = *r`), including when the borrow is
  **laundered** through a tuple/array/struct/enum aggregate or a function return
  (the interprocedural escape fixpoint follows aggregates), and through
  **higher-order fn-pointer** forwarding (`apply(deref_any, &coin)`).
- **Field read/move out of a `&`-borrowed aggregate** (`match &w { W{coin:c} => .. }`
  for both struct and enum variants, and `w.coin` through `&Wrap`).
- **Extracting a linear field twice** from an owned `#[linear]` struct
  (`let a = w.coin; let b = w.coin`).
- **Generic deref-and-return** through any path (`f<T>(r:&T)->T` laundering `*r`
  through a struct/tuple/enum, incl. multi-level generic wrapper chains).
- **Closure** capturing+consuming an outer linear; inner-block **shadow** revival;
  **`while`/`while let` condition** consume; **method on a borrowed linear receiver**.

## Known residual (NOT yet closed — this is why it is a lint, not a guarantee)

After brick 2 the residual is small but real. Do not rely on no-cloning for these:

1. **`&mut`-match payload move** — `match b { Box::A(c) => c }` where `b: &mut` of a
   linear enum, then the container `b` is used again: the payload is moved out of
   the `&mut` and the container is reused, double-consuming the one resource. The
   MIR checker deliberately excludes `&mut` from the move-out-of-borrow rule (a
   sound fix needs container-reused-after-`&mut`-extract dataflow with real
   over-rejection risk); left for the next hardening pass.
2. **Un-enumerated advanced corners.** Adversarial testing keeps surfacing new
   narrow root causes (this is expected: a complete affine checker is a
   research-grade, multi-iteration effort — cf. Rust's borrow checker, Linear
   Haskell). Treat any un-listed advanced composition (exotic higher-order,
   trait-object dispatch, deep alias/reborrow chains) as **not yet guaranteed**.

## Path to true soundness

The MIR affine/borrow checker is the right foundation and has closed the bulk of
the documented open classes, but *completeness* is a deliberate, multi-brick
effort. Remaining work: `&mut` move-out-with-reuse tracking, the
**drop-without-consume** ("must use") half of linearity, and continued adversarial
hardening until an adversarial sweep finds no constructible clone. Only then
should the "sound no-cloning" claim be made.

## Methodology note

This status is the product of repeated **adversarial verification** — fan-outs of
attack agents by lens, each constructing `.bld` programs and running the real
`buildc check` binary, with confirmed clones executed (`buildc run`) to prove they
are true double-consumes, not artifacts. Every "enforced" item is a
confirmed-and-fixed exploit; every "known residual" item is a confirmed live
exploit. The honest takeaway, unchanged: ship the verifier with the feature, and
**do not claim soundness the verifier has not earned.**
