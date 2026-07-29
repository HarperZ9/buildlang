# W6: unit-annotated numeric types, checker slice one (experimental) - implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans, task-by-task.

**Scope verdict (read first).** The slice below IS one safe commit: one parser
interception, one AST variant, one optional field on `Ty`, bounded arms in
`unify_impl` and `infer_binary`, two error variants, one delegation arm in MIR
type lowering, fixtures, tests, docs. Everything is gated on syntax that today
either fails to parse or is silently discarded (probe evidence below), so the
existing surface cannot regress except through the named hazards, each of which
gets a mutation check.

What this slice DELIBERATELY CUTS from the roadmap sketch in
`docs/DIMENSIONAL-ANALYSIS.md` (each stays SPEC there, named in Task 8):

1. **Dimension variables** (Pass B's "unannotated literal is dimension-
   polymorphic via a fresh dimension variable"). This slice uses the weaker,
   safe rule the `Ty.annotations` field already implements for `with`
   annotations: an unannotated numeric type is UNCONSTRAINED and compatible
   with any unit. Fewer catches, zero false positives, no new inference
   machinery. Full variables are the follow-up brick.
2. **`powi` lifting** (`**` with an integer-literal exponent producing a
   scaled dimension). `**` on any unit-carrying operand is a LOUD unsupported
   error in this slice, never a silently wrong dimension.
3. **Pass D receipt flow-through** (`--units` derived from the checked type).
   Untouched: `--units` stays the checked, canonicalized source of the
   scientific-runtime receipt's `measurement.units`, exactly as shipped.
4. **Integer units** (`i64<m>`), GPU kernels, casts as a claimed surface, and
   arrays of united floats as a claimed surface (the last two flow through
   mechanically; Task 4 verifies they do not miscompile, but the slice claims
   and fixtures scalars on `f64`/`f32` only).

What the slice DOES ship: `f64<UNIT>` / `f32<UNIT>` annotations parsing through
the shipped `units::parse_unit` grammar; same-dimension enforcement for `+`,
`-`, `%`, and all comparisons; derived dimensions for `*` and `/` via the
shipped `Dimension::multiply` / `divide`; enforcement at every unification
boundary (let annotation, assignment, call argument vs parameter, return value)
because the rule lives in the unifier's Float arm; zero codegen impact,
mechanically verified.

## Grounding evidence (recorded 2026-07-29 on feat/drop-flags HEAD)

Probe runs (debug `buildc check`, probe files with `let v: f64<...> = 1.0;`):

| annotation | today's behavior | exit |
|---|---|---|
| `f64<m/s>` | parse error `expected `>`, found `/`` | 1 |
| `f64<kg*m/s^2>` | parse error `expected `>`, found `*`` | 1 |
| `f64<1>` | parse error `expected type, found integer` | 1 |
| `f64<m>` | parses; checker SILENTLY DISCARDS the argument; check OK | 0 |
| `f64<zebra>` | parses; silently discarded; check OK | 0 |

Why: the type path arm (`compiler/src/parser/ty.rs` ~line 205) calls
`parse_path` (`compiler/src/parser/mod.rs:568`), whose `parse_generic_args`
(line 612) parses each argument with `parse_type`, so `m` parses as a path type
but `/`, `*`, and `1` are rejected before the closing `>`. Then
`lower_type_path` (`compiler/src/types/infer.rs:6365`) hits
`lookup_primitive("f64")` FIRST and returns `Ty::float(F64)` without ever
looking at the path's generics. So single-token forms are accepted-and-ignored
today; composite forms cannot parse at all.

Backward-compat evidence: `grep -rn "f64<\|f32<"` over the repo finds ZERO
BuildLang sources using the syntax (only Rust `visit_f64<E>` generics in
`main.rs`/`bdf/json.rs` and the CHANGELOG/spec prose). No corpus program, no
test fixture, no example. The behavior change for previously-meaningless
`f64<ident>` forms (silently ignored, now honored or rejected) therefore
breaks nothing in-repo and is documented honestly in Task 8.

Key shipped surfaces this slice builds on (read them before coding):

- `compiler/src/units.rs`: `Dimension` (Copy, Eq, Hash), `parse_unit` (370),
  `canonicalize_unit` (502), `checked_add`/`checked_sub`/`checked_compare`
  (183-220), `UnitError` Display (290-306). 18 unit tests. Do not modify.
- `compiler/src/types/ty.rs`: `Ty { kind, annotations }` (193-200);
  `substitute` preserves `annotations` after the match (539-542) and Float
  types take the `_ => return self.clone()` early exit (537); `Display`
  appends annotations (~748). The `annotations` field is the exact precedent
  for an optional metadata field on `Ty` that participates in Eq/Hash.
- `compiler/src/types/unify.rs`: `unify_impl` (59); the annotation
  compatibility precheck (69-87) implements "both annotated must agree, mixed
  is compatible", which is this slice's unit rule; Float arms at 125-128.
- `compiler/src/types/infer.rs`: `lower_type` (6255), `lower_type_path`
  (6365), `infer_binary` (3406; arithmetic arm 3529, comparisons 3535),
  `infer_local` (5394, lowers the let annotation then unifies with the init),
  `infer_assign` (3615). `TypeError::UnsupportedConstruct` usage idiom at 5408.
- `compiler/src/types/error.rs`: `TypeError` (72), thiserror `#[error(...)]`
  voice: lowercase, backticked values, "expected X, found Y".
- `compiler/src/main.rs`: `type_error_kind` (5007) maps variants to receipt
  diagnostic kinds; `CheckReceipt` (999) and `CheckReceiptDiagnostic` (1020);
  diagnostics assembly (5597-5621). `buildc build` runs the full TypeChecker
  and aborts on type errors before codegen (~6398-6411), so the new checks
  gate builds, not only `buildc check`.
- `compiler/src/codegen/lower/types.rs`: `lower_type_from_ast_inner` (48);
  the `WithEffect` delegation arm (120-128) is the exact precedent for the
  new arm; the `_ => MirType::i32()` fallback (129) is the miscompile hazard.
  NOTE: MIR lowering types from the AST, not from checked `Ty`, so the AST
  arm is mandatory, and the wildcard means the compiler will NOT force it.
  Every file matching on `ast::TypeKind` (grep evidence): codegen/lower/
  {expr,macros,mod,types}.rs, gpu/mod.rs, parser/item.rs. Task 4 audits each.

## The design

### Surface syntax and parse path

A unit annotation is angle brackets directly after the primitive float head:
`f64<m/s>`, `f32<J>`, `f64<kg*m/s^2>`, `f64<1>` (explicitly dimensionless).
Grammar inside the brackets is EXACTLY the shipped `units::parse_unit` grammar,
shared errors included (Pass A commitment in the spec doc).

Interception point: `parse_type_primary` in `compiler/src/parser/ty.rs`, in
the path-types arm (~line 205), BEFORE `parse_path` runs. Condition: current
token is `Ident` whose source text is exactly `f64` or `f32` AND the next
token is `Lt`. Then:

1. Consume the ident and the `<`.
2. Scan tokens, recording spans, until the first `Gt` or `Shr` (the unit
   grammar has no nested `<`, so no depth tracking). On `Shr`, downgrade the
   token to `Gt` in place and treat the first half as the closer, the same
   trick `expect_closing_angle` (parser/mod.rs:640) already uses, so
   `Vec<f64<m>>` closes correctly. On EOF, parse error "expected `>`".
3. Slice the SOURCE TEXT between the `<` span end and the closer span start
   (the probe shows `m/s` lexes as three tokens; reconstructing from source
   text keeps the unit grammar owned by `units::parse_unit`, not the lexer).
4. `units::parse_unit(text)`: `Err(e)` is a parse error carrying `e`'s
   Display verbatim (so `f64<zebra>` reports ``unknown unit `zebra``` and
   `f64<>` reports "empty unit annotation"); `Ok(dim)` produces the new AST
   node with `base` = the plain `Path(f64)` type.

Everything routed through `parse_type` gets the syntax (let annotations, fn
params, return types, struct fields, casts). The CHECKED-and-claimed surface
is decided by where the checker enforces, which is the unifier, so let/assign/
param/return come from one rule. Casts: Task 3 verifies whether the cast arm
resolves its target via `lower_type`; if it does, `x as f64<m>` is the
deliberate re-annotation escape hatch and is documented as such; if not, casts
stay unclaimed. Do not add cast-specific code either way.

### AST representation

New variant in `compiler/src/ast/ty.rs` (`TypeKind`, line 68), in the
BUILDLANG EXTENSIONS block beside `WithEffect`:

```rust
/// A unit-annotated numeric type: `f64<m/s>` (experimental).
/// `dim` is the parsed, canonical dimension; `unit_text` is the source
/// spelling for diagnostics. The base is always a primitive float path
/// in this slice (the parser only produces this for f64/f32 heads).
WithUnit {
    base: Box<Type>,
    dim: crate::units::Dimension,
    unit_text: std::sync::Arc<str>,
},
```

`units` is a dependency-free sibling module in the same crate; the AST may
depend on it. `Dimension` is `Copy + PartialEq`, matching `TypeKind`'s derives.

### `Ty` representation

New field on `Ty` (`compiler/src/types/ty.rs:193`), the `annotations` shape:

```rust
/// Optional physical dimension for numeric types (`f64<m/s>`), experimental.
/// `None` means unconstrained: compatible with any unit, the common case.
pub unit_dim: Option<crate::units::Dimension>,
```

Named `unit_dim`, NOT `unit`, because `Ty::unit()` (line 257) is the unit-type
constructor and a same-named field would shadow-confuse every reader.
`Dimension` is `Copy + Eq + Hash`, so the derives hold. Mechanics:

- `Ty::new` / `with_annotations` initialize `None`. Add
  `pub fn with_unit_dim(mut self, dim: units::Dimension) -> Self`.
- Adding the field makes every struct-literal construction of `Ty` a compile
  error; fix each (grep `Ty {` under `compiler/src/types/`), which is the
  compiler enforcing coverage.
- `substitute` (490): Float hits `_ => return self.clone()`, preserving the
  field; ALSO mirror the annotations-preservation block (539-542) with
  `if self.unit_dim.is_some() && result.unit_dim.is_none() { result.unit_dim = self.unit_dim; }`
  so a unit riding on a Var-containing type survives substitution.
- `Display` (~748): when `unit_dim` is `Some(d)`, render `f64<{d}>` using the
  canonical formatter (`Dimension` already implements Display as the canonical
  string). Existing programs never have `Some`, so Display is unchanged for
  them. This is what makes the generic `TypeMismatch` voice name canonical
  forms: ``type mismatch: expected `f64<m/s>`, found `f64<m>` ``.
- Eq/Hash now include the field. This is the annotations precedent (they are
  already in Eq/Hash and unify's fast path says "equal including
  annotations"). All existing values are `None`, so existing behavior is
  bit-identical. The residual risk (Ty-keyed lookups treating `f64<m>` and
  `f64` as distinct, e.g. method dispatch on an annotated receiver) is the
  riskiest decision; see the final section and Task 5's audit + test.

### Lowering (checker side)

`lower_type` (`infer.rs:6255`) gains the arm:

```rust
ast::TypeKind::WithUnit { base, dim, .. } => {
    let base_ty = self.lower_type(base);
    match base_ty.kind {
        TyKind::Float(_) => base_ty.with_unit_dim(*dim),
        _ => base_ty, // defensive: parser only emits float heads
    }
}
```

### Checker rules (exact)

All dimension math is the shipped `units` algebra; no new algebra anywhere.

1. **Unification (the backstop that covers let, assign, argument, return).**
   In `unify_impl`, replace the two Float arms (unify.rs:125-128) with a
   single arm that first checks units:
   - `(Some(a), Some(b))` with `a != b` ->
     `Err(TypeError::UnitMismatch { expected: a.to_canonical_string(), found: b.to_canonical_string() })`
   - anything else (equal, or at least one `None`) -> `Ok(())` (float width
     coercion preserved exactly as today).
   Mixed `Some`/`None` is compatible and the `Some` side propagates only when
   a type VARIABLE binds to the full united `Ty` (which is how
   `let x: f64<m> = 1.0;` gives the literal its unit today, no new code).
2. **Add, Sub, Rem, comparisons.** `infer_binary` (3406): split `Rem` out of
   the arithmetic arm so `Add | Sub` and `Rem` keep the existing
   unify-left-right body (the Float arm from rule 1 now enforces same
   dimension through it; `m % m = m` is correct Rem semantics and comes free).
   For `Add | Sub` and the comparison arm, ADD a pre-check before the unify:
   apply both sides; if BOTH are `TyKind::Float` with `Some` dims, call
   `checked_add` / `checked_sub` / `checked_compare`; on `Err`, report
   `TypeError::UnitOperationMismatch` (below) and return `Ty::error()`
   (comparisons included; do not also run the unify, one diagnostic per site).
   The pre-check exists ONLY to produce the operation-worded message; the
   unifier remains the enforcement backstop for every shape the pre-check
   does not see (vars, mixed annotation, argument passing).
3. **Mul, Div.** New arm body: apply both sides. If neither is a Float with
   `Some` dim, keep today's body verbatim (unify, return applied left). If at
   least one is: unify STRIPPED copies (clone with `unit_dim = None`) so
   width checking still happens without a spurious dimension mismatch, then
   return the applied left with
   `unit_dim = Some(l.unwrap_or(DIMENSIONLESS).multiply(&r.unwrap_or(DIMENSIONLESS)))`
   (`divide` for `/`). `None` operands count as dimensionless in the product
   (so `2.0 * d` keeps `m`), but a `None * None` result stays `None` (never
   invent units). A computed `Some(DIMENSIONLESS)` stays `Some` (`m/s / m/s`
   is explicitly dimensionless `1` and keeps enforcing).
4. **Pow.** In the `BinOp::Pow` arm: if either applied operand is a Float
   with `Some` dim, report `TypeError::UnsupportedConstruct { construct:
   "`**` on unit-carrying operands", detail: "dimensional exponentiation
   (powi lifting) is a specced follow-up in docs/DIMENSIONAL-ANALYSIS.md;
   compute the power with `*`, or drop the annotation" }` and return
   `Ty::error()`. Loud deferral, never a silently wrong dimension.
5. **Unary minus.** Read the unary arm; it must return the operand type
   unchanged (unit preserved). Add the one-line test; no code expected.
6. **Broadcast dot ops** (`.*` etc.): element unify flows through rule 1
   mechanically. Unclaimed; Task 4 only proves no miscompile.

### Error variants and messages (repo diagnostic voice)

In `types/error.rs`, two new variants:

```rust
/// Unit dimensions disagree at a unification boundary (assignment,
/// annotation, argument, return).
#[error("unit mismatch: expected `{expected}`, found `{found}` (dimensions differ)")]
UnitMismatch { expected: String, found: String },

/// Unit dimensions disagree for a specific operation (add/subtract/compare),
/// wording identical to units::UnitError::Mismatch.
#[error("unit mismatch: cannot {operation} `{left}` and `{right}` (dimensions differ)")]
UnitOperationMismatch { operation: &'static str, left: String, right: String },
```

The strings are ALWAYS canonical forms from `to_canonical_string()`, so the
diagnostics normalize spelling (`m*s^-1` reports as `m/s`). `main.rs`'s
`type_error_kind` (5007) gains the two arms ("UnitMismatch",
"UnitOperationMismatch"); if that match has no wildcard the compiler forces
this, verify either way.

### Codegen erasure (the guarantee and its proof)

The annotation must have ZERO codegen impact. Three layers:

1. **Structural**: `MirType::Float(FloatSize)` has no unit slot, and codegen
   never reads `Ty.unit_dim` (MIR lowering types from the AST). There is no
   channel for a dimension to reach emitted C.
2. **Code**: `lower_type_from_ast_inner` (codegen/lower/types.rs:48) gains a
   `WithUnit { base, .. } => self.lower_type_from_ast(base)` arm, the
   `WithEffect` precedent verbatim. WITHOUT this arm the wildcard silently
   lowers an annotated local to `MirType::i32()`, a miscompile; this is a
   named mutation check.
3. **Mechanical**: fixture pair `units_velocity.bld` (annotated) and
   `units_velocity_plain.bld` (byte-identical minus annotations); a CLI test
   emits C for both (`--target c` / `--emit c`, follow the mem-fixture
   command shape from the increment-5 plan) and asserts the outputs are
   byte-identical. Plus the standing gates: corpus verify 8/8 and the full
   suite, which pin every EXISTING program's output.

### Receipts: no shape change (explicit)

- **Check receipts** (`buildlang-check-receipt/v1`): NO schema change. The
  new errors surface as entries in the EXISTING `diagnostics` array, whose
  `kind` is an open string field (main.rs:5610-5621). New kind values appear
  only for programs that use the new opt-in syntax and fail. A program using
  composite annotations could not previously parse; a single-token
  `f64<m>` program previously checked OK with the annotation discarded and
  still checks OK now (its receipt is unchanged); a previously-check-OK
  `f64<zebra>` program would now fail at parse, and grep evidence shows no
  such program exists anywhere in-repo. Documented in Task 8 as the one
  intended semantic change.
- **Scientific-runtime receipts**: untouched. `measurement.units` keeps its
  shipped `--units` source (Pass D deferred). No unit fact from the checker
  enters any receipt in this slice.

## Global constraints

- ONE commit on branch `feat/units-in-types`, stacked on `feat/drop-flags`
  (create from its HEAD). Do not push. This plan doc rides in the commit
  (move to `docs/superpowers/plans/2026-07-29-units-in-types.md`, drop the
  -DRAFT suffix).
- Backward compatible: zero behavior change for any program not using the
  new syntax. Existing emitted C byte-identical (corpus + fixture gates),
  existing receipts byte-identical, full `cargo test` from `compiler/`
  0 failed (baseline per STATUS: 1,613 passed; re-capture the number at the
  branch point before claiming it).
- Honest register: every doc and message says EXPERIMENTAL; no claim that
  runtime numbers carry units; the C backend guarantee is stated as "erased
  before MIR, verified byte-identical".
- Every new gate mutation-checked with red/green evidence (Task 7); exit
  codes captured before any pipe (the pipes-swallow-exit-codes trap); no
  em-dashes in any prose this commit adds; `cargo fmt --check` clean.
- No new dependency, no change to `units.rs`, no receipt schema change, no
  backend work beyond the single delegation arm (wind-down posture holds).

### Task 1: parser + AST

- [ ] `ast/ty.rs`: add `TypeKind::WithUnit` as specced. `cargo check` and fix
  every non-exhaustive match the compiler surfaces by delegating to `base`;
  then AUDIT the wildcard matchers the compiler cannot flag (grep list in
  the evidence section) and record the audit in the commit body.
- [ ] `parser/ty.rs`: the interception in `parse_type_primary` as specced
  (f64/f32 head only, source-text slice, `Shr` downgrade, EOF guard,
  `parse_unit` errors verbatim).
- [ ] Parser tests (parser test suite idiom): `f64<m/s>` and `f64<kg*m/s^2>`
  and `f64<1>` parse to `WithUnit` with the expected dimensions; `f32<J>`
  parses; `f64<>` and `f64<m s>` and `f64<zebra>` are parse errors carrying
  the `UnitError` message; `Vec<f64<m>>` parses (Shr split); `f64 < x` in
  EXPRESSION position still parses as comparison (parse_path_in_expr never
  takes generics, cite the existing guard, add the regression test anyway);
  a plain `f64` annotation is untouched.

### Task 2: `Ty` representation

- [ ] `types/ty.rs`: `unit_dim` field, `with_unit_dim`, substitute
  preservation, Display rendering, all as specced. Fix every `Ty { .. }`
  struct literal the compiler surfaces.
- [ ] Unit tests: Display renders `f64<m/s>` canonical (`m*s^-1` input);
  substitute preserves the dim through a Var chain; `Ty::float(F64)` equality
  with and without dims behaves as expected (with != without).

### Task 3: checker rules

- [ ] `infer.rs` `lower_type`: the `WithUnit` arm.
- [ ] `unify.rs`: the Float-arm unit rule.
- [ ] `infer.rs` `infer_binary`: the Add/Sub/compare pre-check, the Mul/Div
  derived-dimension arm, the Rem split, the Pow guard, exactly as specced.
- [ ] `error.rs`: the two variants; `main.rs` `type_error_kind` arms.
- [ ] Read the cast inference arm; record (commit body + doc) whether
  `x as f64<m>` re-annotates (escape hatch, document) or ignores (unclaimed).
- [ ] Checker tests, `check_source` idiom (check.rs test mod): the negative
  and positive matrix:
  - mismatched add refused: `let d: f64<m> = 3.0; let t: f64<s> = 1.0;
    let x = d + t;` -> exactly one error, message
    ``unit mismatch: cannot add `m` and `s` (dimensions differ)``.
  - mismatched subtract and mismatched compare (`d < t`) refused with the
    operation-specific wording.
  - mismatched assign refused: `let mut d: f64<m> = 3.0; d = t;` ->
    `UnitMismatch` expected `m` found `s`.
  - mismatched let-annotation refused: `let bad: f64<m> = d / t;` ->
    expected `m`, found `m/s`.
  - mismatched argument refused: `fn go(v: f64<m/s>) {}` called with
    `f64<m>` -> `UnitMismatch` (proves the unify backstop covers calls).
  - derived dimensions accepted: `let v: f64<m/s> = d / t;` and
    `let e: f64<J> = ...` built from `kg*m/s^2 * m` -> no errors (proves
    multiply/divide and named-derived equivalence end to end).
  - same-dimension positive: `d + d`, `d % d`, `d < d`, `2.0 * d` kept `m`
    (assign the product to `f64<m>`), unannotated mixing allowed
    (`let p: f64 = 1.0; d + p` no error, weak mode documented).
  - `d ** 2.0` -> the UnsupportedConstruct message.
  - unary minus preserves the unit (assign `-d` to `f64<m>`).
  - normalization: `f64<m*s^-1>` unifies with `f64<m/s>` (canonical equality).

### Task 4: MIR erasure + no-miscompile audit

- [ ] `codegen/lower/types.rs`: the `WithUnit` delegation arm.
- [ ] Audit every `ast::TypeKind` wildcard matcher (grep list) for a path a
  `WithUnit` node could reach and hit a wrong-type fallback: codegen/lower/
  {expr,macros,mod}.rs, gpu/mod.rs, parser/item.rs. Each site either
  delegates, cannot receive the node, or is recorded with why it is safe.
- [ ] Fixture pair `units_velocity.bld` / `units_velocity_plain.bld` under
  `compiler/tests/` (annotated program: let-annotations, a param, a derived
  division, printed output). CLI test: both compile via `buildc build`,
  emitted C byte-identical, program runs with identical output.
- [ ] `buildc check --receipt -` on the annotated fixture: receipt schema
  string unchanged, `diagnostics` empty; on a mismatched-add fixture:
  one diagnostic, `stage: "type"`, `kind: "UnitOperationMismatch"`.

### Task 5: Ty-equality audit (the diffuse risk)

- [ ] Grep Ty-keyed maps/sets and equality-driven lookups under
  `compiler/src/types/` (dispatch.rs, traits.rs, context.rs) for sites where
  `f64<m>` vs `f64` inequality could change resolution. For each: record
  safe, or strip `unit_dim` at the lookup boundary.
- [ ] Behavioral test: a method call on an annotated float receiver (use
  whatever inherent float method the checker resolves today; if none
  resolves for plain `f64` either, record that and drop the test) plus an
  annotated float through a generic function if the checker supports it.

### Task 6: full-surface regression

- [ ] `cargo fmt --check` clean; full `cargo test` from `compiler/` 0 failed;
  test-count delta recorded.
- [ ] `buildc corpus verify` 8/8 (no flag interaction expected; capture exit
  code directly, no pipes).
- [ ] Re-run the five probe files from the evidence table; record the new
  behaviors (composite forms now check OK; `f64<zebra>` now a parse error
  naming the unknown unit).

### Task 7: mutation checks (break, observe red, restore, observe green)

- [ ] Remove the unify Float-arm unit rule: mismatched-assign and
  mismatched-argument tests red.
- [ ] Remove the Add pre-check only: the exact-wording `cannot add` test red
  (the generic backstop message differs), proving the message contract.
- [ ] Swap `multiply` for `divide` in the Mul arm: derived-dimension positive
  test red.
- [ ] Treat `None * None` as dimensionless `Some`: the unannotated-mixing
  positive test red (units invented where none were written).
- [ ] Remove the Pow guard: the `**` test red.
- [ ] Remove the MIR `WithUnit` delegation arm: the byte-identity CLI test
  red (annotated C differs, i32 fallback), proving the miscompile hazard is
  pinned.
- [ ] Remove the substitute preservation line: the substitute unit test red.
- [ ] Make the parser skip `parse_unit` validation (store raw text): the
  `f64<zebra>` parse-error test red.

### Task 8: docs + changelog, same commit

- [ ] `docs/DIMENSIONAL-ANALYSIS.md`: status line becomes "first slice
  shipped; checker slice one shipped (experimental); remaining passes
  specced". New SHIPPED section "Checker slice one" stating exactly: the
  syntax and shared grammar; the enforcement matrix (add/sub/rem/compare
  equal-dimension, mul/div derived, unification boundaries); the weak-mode
  rule (unannotated floats unconstrained, dimension variables still SPEC);
  the Pow loud-deferral; erasure before MIR with the byte-identity evidence;
  receipts unchanged (Pass D still SPEC); the `f64<ident>`
  previously-silently-ignored honesty note with the probe table. Trim Pass
  A/B/C SPEC text to what remains (variables, powi, receipt flow-through).
- [ ] `STATUS.md` Type Checker bullet: one appended sentence in the existing
  register, experimental label included, naming what is NOT claimed (no
  runtime units, no inference variables, no receipt derivation).
- [ ] `CHANGELOG.md` Unreleased: one bullet, sibling register to the shipped
  units-core entry, honest scope, backward-compat statement, test counts.
- [ ] Commit subject: `feat(types): unit-annotated numeric types, checker
  slice one (experimental)`; body records the Task 1/4/5 audits, the cast
  finding, the probe table delta, and ends with the Co-Authored-By line per
  repo convention. ONE commit, do not push.

## Deferrals that REMAIN after this slice (name them everywhere)

1. Dimension variables / polymorphic literals (Pass B/C full): unannotated
   floats are unconstrained, so a unit bug that flows through an unannotated
   intermediate binding is NOT caught unless a later boundary is annotated.
2. `powi` lifting for `**` (loudly unsupported on united operands).
3. Pass D receipt flow-through (`--units` stays the receipt source).
4. Integer units, GPU kernels, arrays and broadcast ops as claimed surfaces,
   casts unless Task 3 finds them free.
5. Unit CONVERSIONS, fractional exponents, full SI table: spec non-goals,
   unchanged.

## Riskiest design decision and its mitigation

Adding `unit_dim` to `Ty` puts a new field into Eq/Hash for the whole type
system, so any Ty-keyed lookup could newly distinguish `f64<m>` from `f64`.
The mitigation is layered: the field defaults to `None` everywhere (existing
programs bit-identical, enforced by the full suite, corpus, and the
byte-identity fixtures); the `annotations` field is an existing in-Eq/Hash
precedent the codebase already tolerates; Task 5 audits every Ty-keyed lookup
and tests method resolution on an annotated receiver; and the compiler itself
enforces construction coverage because the new field breaks every `Ty` struct
literal. The second-ranked risk, the MIR `_ => MirType::i32()` silent
miscompile for the new AST variant, is pinned by the delegation arm plus a
mutation check that proves the byte-identity test goes red without it.
