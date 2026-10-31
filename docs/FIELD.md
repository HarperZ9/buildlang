# The field type: a spec for BuildLang

Status: **SPEC.** Nothing here is implemented in the BuildLang compiler yet. The reference implementation is the JavaScript package `packages/field` in HarperZ9/superframe (`src/field.js`, `units.js`, `ledger.js`, `grade.js`, `transducer.js`, `ops.js`). This page states the contract BuildLang should meet. It extends the compile-time units in `DIMENSIONAL-ANALYSIS.md`, which are erased before MIR, with values that keep their meaning at run time.

## What a field is

A field is values over a domain, carried with what is needed to trust them.

| Part | Contract |
|---|---|
| domain | one of `point`, `sequence`, `grid`, `mesh`, `graph`, `spacetime`. A mesh or graph names its nodes and its edges or faces; the others name a shape |
| values | a flat buffer, with `components` values per domain element |
| unit | a dimension vector over the seven SI base quantities plus `bit`, and a scale to SI. Adding volts to bits is refused. Angles, counts and ratios are dimensionless |
| uncertainty | a standard deviation per value, or one for all |
| provenance | a status, a source and a method. The status is one of `defined`, `measured`, `derived`, `fitted`, `from_memory`, `missing` |
| representation | `analog` or `digital`, with sample rate, grain or bit depth, and for floats the rounding class |
| ledger | a hash chain of every conversion that produced this field |

A computed result is never stronger than `derived`, and never stronger than its weakest input. One value held from memory marks everything computed from it.

## Converters

A converter is a typed transducer. It declares an id, a claimed grade, a tolerance and how a round trip is compared, plus its source and target domain kind and unit, which are checked on entry. It offers as many of these as it can: an inverse, a target sample for the reverse test, a reference that a lowering must equal, a structure it keeps, or a named loss estimate.

Grades, strongest first:

| Grade | Meaning, measured on the data in hand |
|---|---|
| `isomorphism` | `g(f(x)) = x` and `f(g(y)) = y`, each within the tolerance |
| `embedding` | `g(f(x)) = x` within the tolerance; the reverse direction fails or was not tested |
| `homomorphism` | information is lost, and a declared structure survives its check |
| `none` | information is lost and no declared structure survived |

Applying a converter measures the grade, appends a ledger entry, and refuses by default when the measurement does not support the claim. The measured loss is the round-trip entropy `H(x | g(f(x)))`. For numbers it is the entropy of the residual coded at the tolerance grain, which bounds the loss from above at that grain, on that sample. For symbols it comes from joint counts.

## The ledger

Each entry names the converter, the claimed grade, the measured grade, the bits lost, the tolerance, and hashes of the input and output values. Each entry also holds the hash of the entry before it, so editing or dropping a step breaks the chain. Value hashes cover a header (encoding, length, unit) and then the bytes, so the same numbers in another encoding hash differently. Canonical JSON follows `canonical-record/1` from HarperZ9/flywheel, tested against its CC0 vectors.

## Rounding class

A float lowering records how it rounds. This is where the type earns its place in a compiler. `saxpy`, `y' = a x + y` in float32, has two lowerings:

- `fused` rounds once.
- `unfused` rounds twice: `round(round(a x) + y)`.

Both are valid programs and they give different answers on some inputs, so a field that claims isomorphism with the fused spec at 0 ULP must be refused when lowered unfused. `tests/differential/test_saxpy_rounding.py` holds the seed of the differential test: inputs, both answers computed exactly, and a check that the two classes differ. Compiling `examples/gpu/saxpy.bld` on each backend and comparing to these answers is the next step, and it has not been done. Which class each backend produces today is unknown.

## What BuildLang needs to add

1. Run-time unit and dimension values on a `Field` type, sharing the checker's canonical form from `DIMENSIONAL-ANALYSIS.md`.
2. A `rounding` attribute in the lowering, reported per kernel by each backend.
3. `Converter` as a language-level trait with the grade check above.
4. A ledger in the standard library using `canonical-record/1`.
5. Differential tests that compare each backend with the exact answers.

## Licence

The superframe package is FSL-1.1-MIT and this repo is AGPL-3.0-only. BuildLang takes this contract and writes its own code, with no code copied across that line.
