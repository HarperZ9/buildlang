# Compile-time dimensional analysis (typed physical units)

Status: **first slice shipped; full type-checker integration specced.** This
document describes BuildLang's dimensional-analysis feature. The parts marked
SHIPPED are implemented and tested on `main`. The parts marked SPEC are the
staged plan for the remaining passes, precise enough to build against.

Honest maturity: the shipped slice is a pure, tested dimensional-algebra core
plus a receipt integration. It does NOT yet make `f64<m/s>` a first-class type
in the Hindley-Milner checker, and it makes no claim that a compiled program's
runtime numbers carry units. It checks and canonicalizes unit ANNOTATIONS and
the scientific-runtime receipt's measurement label. The C backend is unchanged
and guarantees nothing about units at runtime.

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

## SPEC: full type-checker integration (staged, not yet built)

This is the remaining multi-pass work. It is deliberately NOT part of the first
slice, because it touches the parser, the AST, the type representation, and the
Hindley-Milner unification engine (`compiler/src/types/infer.rs`), and doing it
correctly is a larger build than one increment should attempt.

### Pass A: unit-annotated types in the parser and AST

Surface syntax: a numeric type may carry a unit annotation in angle brackets,
`f64<m/s>`, `f32<J>`, `f64<1>` (explicitly dimensionless). Add a `TypeKind`
variant (or a side-table keyed by `NodeId`) carrying the parsed `Dimension`.
The parser reuses `units::parse_unit` on the annotation text, so the grammar
and the error messages are shared with the shipped core. An annotation that
fails to parse is a parse error with the `UnitError` message.

### Pass B: dimensions in the type representation

Extend the internal `Ty` for a numeric type with an optional `Dimension`. A
numeric literal with no annotation is dimension-POLYMORPHIC (a fresh dimension
variable), so `1.0` unifies with any unit; an annotated binding fixes it.

### Pass C: unification and the checked rules

- Unifying two numeric types unifies their dimensions: equal dimensions unify,
  a dimension variable binds to a concrete dimension, and two DIFFERENT
  concrete dimensions are a unification failure that surfaces as a unit-mismatch
  diagnostic (reusing `UnitError::Mismatch`'s wording).
- `+`, `-`, and the comparison operators require their operands' dimensions to
  unify (this is `Dimension::checked_add` / `checked_sub` / `checked_compare`
  lifted into inference). `*` produces `multiply`, `/` produces `divide`, and an
  integer-literal exponent in a `powi`-like intrinsic produces `powi`.
- `let v: f64<m> = a + b;` where `a: f64<m>` and `b: f64<s>` is then a COMPILE
  ERROR. That negative test (a unit mismatch failing to compile) is the
  acceptance criterion for this pass.

### Pass D: receipt flow-through

When a `run` kernel's measured value has an inferred, non-polymorphic dimension,
`--units` becomes optional: the receipt's `measurement.units` is derived from
the checked type instead of a hand-declared flag, and a `--units` that
disagrees with the inferred dimension is a hard error. Until Pass C lands,
`--units` stays the (checked, canonicalized) source of the receipt unit, which
is what the shipped slice does.

## Non-goals (explicit)

- Fractional exponents (`m^(1/2)`). Integer exponents only.
- Unit CONVERSIONS with scale factors (`km` -> `m`, `eV` -> `J`). This core is
  about DIMENSIONS, not magnitudes; a `km`-vs-`m` scale layer is a separate
  feature.
- Runtime unit tracking. Units are a compile-time and receipt-label concern;
  the C backend emits ordinary `double` arithmetic and carries no unit metadata.
- A complete SI/CODATA derived-unit table. The named-derived set is a curated,
  documented subset.

## Provenance

The shipped slice rides on the existing `buildlang-scientific-runtime-receipt/v0`
seal without changing the schema: `measurement.units` was already an optional
field; the change is that it is now a CHECKED canonical unit rather than an
unvalidated free-text string when `--units` is supplied.
