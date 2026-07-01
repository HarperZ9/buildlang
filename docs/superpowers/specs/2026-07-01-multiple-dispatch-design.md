# Design: Static Multiple Dispatch (Pillar A)

Status: draft for execution (2026-07-01), branch `feat/multiple-dispatch`. Governed by
`docs/UNIVERSAL_SUBSTRATE_DIRECTIVE_2026-06-30.md` (the honest scientific language: Julia
ergonomics + the effects/linear wedge). First Julia pillar after the foundation.

## Summary

Add Julia-style **multiple dispatch**: multiple functions may share one name, each with a
different parameter-type signature, and a call selects the method whose parameter types best
match the **tuple of all argument types** (not just the first/receiver). Do it **statically**
— resolve at type-check time from the inferred argument types, then emit a direct call to the
selected method (mangled to a distinct function). Dynamic (runtime-type) dispatch is deferred
(buildlang has no runtime type descriptors; HM inference makes nearly every call statically
resolvable).

## Motivation

Multiple dispatch is Julia's defining feature and is currently **structurally impossible** in
buildlang: functions are bound one-per-name in the type checker's scope
(`types/context.rs` `bindings: HashMap<Arc<str>, TypeScheme>`), so a second `fn add(...)`
silently OVERWRITES the first (`check.rs:611 self.ctx.define_var(name, fn_ty)`). Operator
overloading dispatches on the **left operand only** (`codegen/lower/expr.rs:845-883`,
`impl_methods[(left_type, op)]`). This is single dispatch at best.

Everything needed for the *static* version already exists (verified by two code maps):
- `infer_call` (`types/infer.rs:3713`) infers **all** argument types before choosing the
  callee — the arg-type tuple is available at the dispatch point.
- The type system has `TyKind` (`types/ty.rs:770`) with `Adt(DefId, _)`, `Int`, `Float`,
  `Ref`, etc. for keying candidates.
- Codegen already **monomorphizes and name-mangles by type** (`lower/types.rs:617
  lower_generic_call`, `lower/mod.rs:1152 mangle_generic_name`, `mangle_type`) and the C
  backend emits **direct calls by name** (`backend/c.rs:2703+`). So one distinct C function
  per (name, param-type-tuple) is already how generics work.

## Architecture

### 1. Multi-method registry (type checker)

Store multiple candidates per function name. Functions are already keyed by `DefId` in
`context.functions`; the gap is the name->scheme binding that overwrites. Add a registry:
```rust
// context.rs
multi_methods: HashMap<Arc<str>, Vec<MethodCandidate>>,
struct MethodCandidate { def_id: DefId, sig: FnSig, param_tys: Vec<Ty>, ret: Ty }
```
At `collect_function`, instead of overwriting `define_var(name, fn_ty)`, APPEND a candidate to
`multi_methods[name]`. Keep a single `define_var` for the common case (used when a name is
referenced as a value, e.g. a fn-pointer) pointing at the sole candidate; if a name has >1
candidate, referencing it as a bare value (not a call) is an error ("ambiguous function
reference; multiple dispatch requires a call with arguments") unless later refined.

### 2. Resolution by argument-type tuple (type checker)

In `infer_call`, after inferring the arg-type tuple `(a1, .., aN)`:
- Look up `multi_methods[name]`.
- If 0 candidates: existing behavior (var lookup / builtin / error).
- If 1 candidate: use it (fast path; identical to today for non-overloaded names).
- If >1: **rank by specificity** and pick the unique best:
  - **Exact match** (each `ai` equal to `param_i`) — most specific.
  - **Coercion/subtype match** (each `ai` coercible to `param_i` via the existing
    `coerce_arg`/subtype rules; concrete-over-generic: a concrete `param_i` is more specific
    than a generic `Param`).
  - **Generic match** (a `param_i` that is a type parameter matches any `ai`).
  Arity must match first (filter by `param_tys.len() == arg_tys.len()`). Rank candidates by a
  specificity score (exact > coercion > generic, summed per position, Julia-style
  "most specific wins"). If a unique most-specific candidate exists, select it; if none
  matches -> `NoMatchingMethod` error; if two are equally-most-specific and incomparable ->
  `AmbiguousMethod` error (list the candidates), exactly like Julia.
- Record the SELECTED candidate's `DefId` on the call node (a side-table
  `call_dispatch: HashMap<call-span-or-id, DefId>` on the checker, or annotate the AST) so
  codegen knows which method to emit. (Reuse the mechanism the checker already uses to pass
  resolved info to codegen, e.g. how `impl_methods` / monomorphization keys flow.)

### 3. Codegen: mangle overloaded definitions; emit the selected method

- A function name with **exactly one** definition keeps its **plain name** (backward compat +
  `extern "C"` FFI untouched — a `#[no_mangle]`/extern function is never overloaded).
- A name with **multiple** definitions: each definition is emitted as a **distinct mangled
  function** `name_<param-ty-tuple>` (reuse `mangle_type` over the param types, analogous to
  `mangle_generic_name` but over the concrete param types). Build a
  `HashMap<DefId, mangled-name>` at collection.
- At a call site, codegen looks up the checker's selected `DefId` for that call and emits a
  direct `Call { func: MirValue::Function(mangled_name_of(def_id)) }`. For non-overloaded
  names, the plain name (no dispatch table lookup needed).
- Generics compose: a generic `fn f<T>(..)` participates as a candidate whose params contain
  `Param`; when selected, it monomorphizes via the existing `lower_generic_call` path (the
  concrete arg types drive the substitution), producing `f_<concrete>`.

### 4. Operator overloading -> both operands

Extend the operator dispatch (`lower/expr.rs:845-883`) from left-operand-only to a two-operand
method lookup: an operator `a + b` desugars to an `add` multi-method call with args
`(a, b)` and resolves by BOTH operand types through the same multi-method resolver. (Keep the
existing left-operand fast path as the 1-candidate case.) This is a follow-on increment.

## Data flow

`collect_function` -> append candidate to `multi_methods` + assign a mangled name per
overloaded def. `infer_call` -> infer arg tuple -> `resolve_overload(name, arg_tys)` ->
selected `DefId` recorded. Codegen -> emit direct call to the selected def's (mangled) name.

## Error handling

- **NoMatchingMethod**: no candidate's params match the arg tuple (after arity filter). Error
  lists the arg types and the candidate signatures.
- **AmbiguousMethod**: two or more equally-most-specific candidates. Error lists them
  (Julia-style "ambiguous" with the tied signatures) so the user adds a more specific method.
- **Bare overloaded reference**: referencing an overloaded name as a value (not a call) is an
  error until first-class multimethod values are designed (out of scope).
- Backward compat: a non-overloaded name behaves EXACTLY as today (no new errors).

## Testing

- **Unit (resolution):** given candidates `add(i32,i32)`, `add(f64,f64)`, `add(&str,&str)`,
  assert `add(3,5)` selects the i32 one, `add(3.0,5.0)` the f64 one; `add(3, 5.0)` (no exact,
  test coercion or NoMatchingMethod per the rules); a generic `add<T>(T,T)` coexisting with
  `add(i32,i32)` -> `add(3,5)` picks the concrete (more specific); an ambiguous pair ->
  AmbiguousMethod.
- **End-to-end (`.bld` via buildc run):** the three-`add` program compiles and each call
  dispatches to the right method (assert stdout). A dispatch-on-second-arg case
  (`f(i32,i32)` vs `f(i32,f64)`) proves it's not receiver-only. Operator overloading on both
  operands (once increment 4 lands).
- **Backward compat / corpus:** every existing program + the 8-program corpus still
  `buildc check`/`corpus verify` cleanly (non-overloaded names unaffected; plain mangling
  preserved; FFI extern names unchanged). Full `cargo test` green; `-Dwarnings`,
  `fmt --check`, clippy correctness green.

## Backward compatibility (load-bearing)

Only names with 2+ definitions are mangled; single-definition names (the overwhelming
majority, incl. all `extern "C"`/FFI and stdlib entry points) keep their plain C name. This
guarantees existing programs and the C-ABI surface are byte-identical. Verify with a
differential C-output sweep on the corpus + examples (like the linear brick's 237-file sweep).

## Decomposition (sub-tasks; each impl -> review -> gate)

- **A1 — registry + resolution (type checker):** `multi_methods` registry, append at
  collect, `resolve_overload` with specificity ranking + Ambiguous/NoMatch errors, arg-tuple
  extraction in `infer_call`, record selected DefId. Unit tests for resolution. (Overloaded
  names still error at codegen until A2 — or A1 gates behind check-only tests.)
- **A2 — codegen mangling + call emission:** per-def mangled names for overloaded defs;
  emit the selected def's function at call sites; keep plain names for single defs.
  End-to-end (`buildc run`) tests; corpus byte-identical sweep.
- **A3 — operator overloading on both operands:** desugar operators through the resolver.
- **A4 — polish + docs:** ambiguity diagnostics quality; update STATUS.md + a
  `docs/MULTIPLE-DISPATCH.md`; note dynamic dispatch as deferred.

Dynamic (runtime-type) multiple dispatch is a separate, later effort (needs runtime type
descriptors + a dispatch table; the C backend's `__vtable_dispatch_` infra is the seed).

## File touch-points (verify at implementation time)

- `compiler/src/types/context.rs`: `multi_methods` registry; append vs overwrite.
- `compiler/src/types/check.rs:597 collect_function`: append candidate; assign mangled name.
- `compiler/src/types/infer.rs:3713 infer_call`: arg-tuple extraction + `resolve_overload` +
  record selected DefId.
- `compiler/src/types/error.rs`: `NoMatchingMethod`, `AmbiguousMethod` variants.
- `compiler/src/codegen/lower/`: per-def mangling for overloaded defs; call emission to the
  selected def; the existing `mangle_type`/`lower_generic_call` reuse.
- `compiler/src/codegen/lower/expr.rs:845 operator dispatch`: both-operand (A3).

## Risks

- **Selecting the resolved method into codegen.** The checker resolves the DefId; codegen must
  reliably map call-site -> selected DefId -> mangled name. Confirm the checker->codegen info
  channel (how monomorphization keys / impl_methods already flow) and reuse it; do not invent
  a fragile parallel path.
- **Specificity/ambiguity rules.** Julia's rules are subtle (partial order over signatures).
  Start with a simple, sound ordering (exact > coercion > generic, position-summed; ties ->
  Ambiguous) and grow; over-report ambiguity rather than silently pick wrong.
- **Backward compat.** The single-def-stays-plain rule is the guardrail; the corpus/examples
  byte-identical sweep is the check.
- **Coercion interactions.** Integer-literal defaulting and numeric coercions can make
  "exact match" fuzzy; pin the resolution tests to buildlang's actual coercion behavior.
