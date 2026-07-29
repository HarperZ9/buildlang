# Compile-time dimensional analysis (typed physical units)

Status: **first slice shipped; checker slice one shipped (experimental);
remaining passes specced.** This document describes BuildLang's
dimensional-analysis feature. The parts marked SHIPPED are implemented and
tested on `main`. The parts marked SPEC are the staged plan for the remaining
passes, precise enough to build against.

Honest maturity: `f64<m/s>` is now a first-class, EXPERIMENTAL, opt-in type in
the Hindley-Milner checker (checker slice one, below), but it makes no claim
that a compiled program's runtime numbers carry units. The annotation is
checked and canonicalized at compile time and erased before MIR; the C backend
is unchanged and guarantees nothing about units at runtime. Dimension
variables (an unannotated literal inferring its dimension from context) are
still SPEC, so a unit bug flowing through an unannotated intermediate binding
is not caught unless a later boundary is annotated.

## Why

A dimensional bug (adding a length to a time, treating an energy as a power)
is a whole class of scientific-software defect that a compiler CAN catch, and a
kind of accountability the scientific-runtime receipt should carry: a measured
series labelled `m/s` should be labelled with a CHECKED unit, not an arbitrary
free-text string an emitter could typo. This feature makes the unit a
first-class, algebra-backed object.

## The model

A physical **dimension** is a vector of integer exponents over the seven SI
base dimensions, in a fixed canonical order:

| index | base dimension        | SI base unit | symbol |
|-------|-----------------------|--------------|--------|
| 0     | length                | metre        | `m`    |
| 1     | mass                  | kilogram     | `kg`   |
| 2     | time                  | second       | `s`    |
| 3     | electric current      | ampere       | `A`    |
| 4     | temperature           | kelvin       | `K`    |
| 5     | amount of substance   | mole         | `mol`  |
| 6     | luminous intensity    | candela      | `cd`   |

Velocity is `[1, 0, -1, 0, 0, 0, 0]` (`m/s`), force is `[1, 1, -2, 0, 0, 0, 0]`
(`m*kg/s^2`, a newton), and a pure number is the all-zero vector (`1`).

The algebra a checker needs:

- **multiply** (`a * b`): add exponents component-wise.
- **divide** (`a / b`): subtract exponents component-wise.
- **power** (`a^n`): scale every exponent by `n`.
- **add / subtract / compare**: require EQUAL dimensions. A mismatch is an
  error. This is the rule a dimensional bug trips.

Exponents are integers. Fractional exponents (`sqrt(Hz)`) are out of scope for
this core and are documented as a non-goal below.

## Unit annotation grammar

```text
unit    := factor ( ('*' | '/') factor )*
factor  := token ( '^' signed-int )?
token   := a base-unit or named-derived-unit symbol
```

`*` keeps the next factor in the numerator; `/` places it in the denominator
(binding one factor, matching the canonical formatter). `1` is the
dimensionless literal. Examples: `m`, `s`, `m/s`, `kg*m/s^2`, `1/s`, `J`.

Named derived units recognised by the core (a small, documented subset, not a
full SI/CODATA table): `Hz` (`1/s`), `N` (`m*kg/s^2`), `Pa` (`kg/(m*s^2)`),
`J` (energy), `W` (power), `C` (charge), `V` (potential).

## Canonical form

The canonical string lists positive-exponent factors first, in fixed SI base
order, joined by `*`; then, if any negative exponents exist, a `/` followed by
the negative-exponent factors with their absolute exponents. Exponent `1` omits
the `^1`. Dimensionless is the literal `1`.

Because the order is fixed, `kg*m/s^2` and `m*kg/s^2` both canonicalize to
`m*kg/s^2` (length before mass), and `m*s^-1` and `m/s` both canonicalize to
`m/s`. Two spellings of the same unit therefore seal to identical bytes.

## SHIPPED: the core module and receipt integration

`compiler/src/units.rs` (public as `buildlang::units`) implements:

- `Dimension`, `BaseDimension`, the algebra (`multiply`, `divide`, `powi`,
  `reciprocal`), and the checked operations (`checked_add`, `checked_sub`,
  `checked_compare`) that return `UnitError::Mismatch` on unequal dimensions.
- `parse_unit`, `lookup_unit`, `canonicalize_unit`, and canonical formatting.
- 18 unit tests covering the algebra, the parser (including malformed and
  unknown-unit rejection), the canonical order, and named-derived-unit
  equivalences (e.g. `J == N*m`, `W == J/s`).

`buildc run --emit-receipt <path> --units <UNIT>` canonicalizes the declared
unit through this core BEFORE any compilation:

- A malformed or unknown unit is a hard error reported immediately (no receipt
  is written).
- A valid unit is recorded in the receipt as its CHECKED canonical form
  (`measurement.units`), covered by the existing receipt seal, so it re-verifies
  through `buildc receipt verify` unchanged. The unit rides on the accountability
  layer; it does not bypass it.

Two CLI integration tests cover the positive path (canonicalized unit sealed
and re-verified) and the negative path (unknown unit rejected before compile,
no receipt written).

## SHIPPED: checker slice one (experimental, 2026-07-29)

`f64<UNIT>` / `f32<UNIT>` unit annotations now parse through the shipped
`units::parse_unit` grammar and are enforced by the Hindley-Milner checker.
EXPERIMENTAL and opt-in: a program that never writes `<...>` after a float
head is completely unaffected (byte-identical behavior, byte-identical
emitted C, byte-identical receipts).

**Syntax and shared grammar.** A unit annotation is angle brackets directly
after `f64`/`f32`: `f64<m/s>`, `f32<J>`, `f64<kg*m/s^2>`, `f64<1>`
(dimensionless). The grammar inside `< >` is EXACTLY `units::parse_unit`'s,
shared errors included: `f64<zebra>` reports `unknown unit \`zebra\``,
`f64<>` reports `empty unit annotation`, both as parse errors. Everything
routed through the type parser gets the syntax: let annotations, function
parameters, return types, struct fields, and casts (`x as f64<m>` is the
deliberate re-annotation escape hatch: casts resolve their target through
the same `lower_type` path as a let annotation).

**Enforcement matrix.** All dimension math is the shipped `units` algebra;
no new algebra was added anywhere.

- `+`, `-`: operands must share a dimension (`checked_add`/`checked_sub`);
  mismatch reports `` unit mismatch: cannot add/subtract `X` and `Y` ``.
- `%`, comparisons (`<`,`<=`,`>`,`>=`,`==`,`!=`): same-dimension required via
  the unifier; comparisons additionally get the operation-worded
  `checked_compare` pre-check message.
- `*`, `/`: derived dimensions via `Dimension::multiply`/`divide`. A `None`
  (unannotated) operand counts as dimensionless in the product, so
  `2.0 * d` keeps `d`'s unit; `None * None` stays `None` (a unit is never
  invented where neither operand wrote one).
- Unary minus preserves the unit.
- Unification is the backstop covering every boundary: let annotation,
  assignment, call argument, return value, and generic instantiation (a
  unit riding on a bound type variable survives substitution).
- `**` (Pow) on a unit-carrying operand is a LOUD `UnsupportedConstruct`
  error, never a silently wrong dimension: dimensional exponentiation
  (`powi` lifting) is specced below, not shipped.

**Weak mode (the one deliberate simplification).** This slice uses a
weaker, safe rule instead of full dimension variables: an unannotated
numeric type is UNCONSTRAINED and compatible with any unit (same rule the
pre-existing `Ty.annotations` `with` mechanism already used). Fewer catches,
zero false positives, no new inference machinery. `let p: f64 = 1.0; d + p`
where `d: f64<m>` is therefore accepted; a unit bug flowing through an
unannotated intermediate binding is caught only if a LATER boundary is
annotated. Full dimension variables are the specced follow-up.

**Erasure before MIR (zero codegen impact, mechanically verified).**
`MirType::Float` has no unit slot and codegen lowers types from the AST, not
from the checked `Ty`, so there is no channel for a dimension to reach
emitted C. A `WithUnit` AST node delegates to its base type in
`codegen/lower/types.rs`; every other `ast::TypeKind` matcher in the codegen
and parser layers was audited for a path a `WithUnit` node could reach a
wrong-type fallback (none found: each site either delegates through the
fixed function, cannot structurally receive the node, or already refuses
loudly rather than guessing, e.g. the GPU kernel path, which requires a bare
`f32`/`f64` path type and rejects anything else). Mechanically pinned by a
fixture pair (`compiler/tests/units/units_velocity.bld` and
`units_velocity_plain.bld`, identical minus annotations): `buildc build`
emits byte-identical C for both, and both run with identical stdout. A
mutation check (removing the delegation arm) confirmed the byte-identity
test goes red -- an annotated local would silently lower to `MirType::i32()`
without it.

**Receipts unchanged.** Check receipts (`buildlang-check-receipt/v1`): no
schema change; the new errors are ordinary entries in the existing
`diagnostics` array (`kind: "UnitMismatch"` / `"UnitOperationMismatch"`).
Scientific-runtime receipts: untouched; `measurement.units` keeps its
shipped `--units` source (Pass D below, still SPEC).

**Honesty note: the one intended semantic change.** Before this slice,
`f64<ident>` parsed as an ordinary path type and the generic argument was
SILENTLY DISCARDED at lowering (`lower_type_path` matches the primitive name
before ever looking at generics). A single-token form like `f64<m>` checked
OK with the unit ignored; a composite form like `f64<m/s>` could not parse
at all (`/` is not a type). Probe table (recorded 2026-07-29 on
`feat/drop-flags` HEAD, re-run after shipping):

| annotation | before this slice | after this slice |
|---|---|---|
| `f64<m/s>` | parse error `expected `>`, found `/`` | parses, checks OK |
| `f64<kg*m/s^2>` | parse error `expected `>`, found `*`` | parses, checks OK |
| `f64<1>` | parse error `expected type, found integer` | parses, checks OK |
| `f64<m>` | parses; argument silently discarded; check OK | parses; unit honored; check OK |
| `f64<zebra>` | parses; silently discarded; check OK | parse error `unknown unit \`zebra\`` |

A repo-wide grep found zero BuildLang sources using `f64<`/`f32<` syntax
before this slice, so no existing corpus program, test fixture, or example
is affected by the one behavior change (`f64<zebra>`, previously
meaningless and accepted, now correctly rejected).

## SPEC: remaining passes (staged, not yet built)

Checker slice one (above) ships enforcement; these are the follow-ups it
deliberately deferred.

### Dimension variables (weak-mode follow-up)

Full inference: an unannotated numeric literal is dimension-POLYMORPHIC (a
fresh dimension variable) rather than unconstrained, so `1.0` unifies with
whatever dimension context ultimately requires, and an unannotated
intermediate binding no longer hides a unit bug. This is the generalization
of checker slice one's weak-mode rule.

### `powi` lifting for `**`

An integer-literal exponent in `d ** 2` should scale `d`'s dimension
(`Dimension::powi`) rather than being rejected. Requires recognizing the
literal-exponent shape at the type-checking site; a non-literal exponent on
a unit-carrying base stays rejected (fractional/runtime exponents are a
non-goal, below).

### Pass D: receipt flow-through

When a `run` kernel's measured value has an inferred, non-polymorphic
dimension (once dimension variables land), `--units` becomes optional: the
receipt's `measurement.units` is derived from the checked type instead of a
hand-declared flag, and a `--units` that disagrees with the inferred
dimension is a hard error. Until then, `--units` stays the (checked,
canonicalized) source of the receipt unit, which is what both the original
shipped slice and checker slice one do.

## Non-goals (explicit)

- Fractional exponents (`m^(1/2)`). Integer exponents only.
- Unit CONVERSIONS with scale factors (`km` -> `m`, `eV` -> `J`). This core is
  about DIMENSIONS, not magnitudes; a `km`-vs-`m` scale layer is a separate
  feature.
- Runtime unit tracking. Units are a compile-time and receipt-label concern;
  the C backend emits ordinary `double` arithmetic and carries no unit metadata.
- A complete SI/CODATA derived-unit table. The named-derived set is a curated,
  documented subset.
- Integer units (`i64<m>`), GPU kernels, and arrays/broadcast ops of united
  floats as a CLAIMED surface (checker slice one's fixtures and tests are
  `f64`/`f32` scalars only; a unit-annotated GPU kernel parameter is
  correctly rejected today by the existing "must be a float scalar or
  slice" gate, and the derivation/multiply-divide arms flow mechanically
  through arrays and broadcast ops without a miscompile, but neither is a
  tested, claimed surface).
- Dimensional correctness of inherent numeric methods (`.sqrt()`, `.cbrt()`,
  ...). These currently return the receiver's type unchanged
  (`infer.rs`'s FLOAT METHODS arm), which is correct for identity-shaped
  methods (`.abs()`, `.floor()`, `.ceil()`, `.round()`, `.trunc()`) but NOT
  for `.sqrt()`/`.cbrt()` (which should scale the exponents). Unclaimed and
  unchanged by checker slice one; a future pass would need a `powi`-style
  per-method exponent rule, not a unification change.

## Provenance

The shipped slice rides on the existing `buildlang-scientific-runtime-receipt/v0`
seal without changing the schema: `measurement.units` was already an optional
field; the change is that it is now a CHECKED canonical unit rather than an
unvalidated free-text string when `--units` is supplied.
