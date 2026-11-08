# Measured constants

A number that came from a measurement can say where it came from:

```text
#[measured(source = "CODATA 2018", uncertainty = 1.5e-15)]
const G: f64<m^3*kg^-1*s^-2> = 6.67430e-11;
```

`buildc check` refuses a `#[measured]` attribute (BL0605) when:

- it has no `source`, or `source` is an empty string;
- it has a key other than `source` and `uncertainty`;
- `uncertainty` is not a finite, non-negative number;
- it sits on anything but a `const`.

The check receipt (`buildc check --receipt`) lists every measured constant in
the root file under `measured_constants`: the name, the value exactly as
written, the canonical unit when the type carries one, the source, and the
uncertainty when given. `buildc receipt verify` re-derives the list from the
current source, so a receipt edited to change a source or an uncertainty no
longer verifies. A program with no measured constants has no
`measured_constants` key, and its receipt is unchanged.

## Limits

- The attribute changes no emitted code. The uncertainty is a recorded label
  in the constant's own unit; no arithmetic propagates it to results.
- The source is a free-text string. The compiler checks that one is given, not
  that it is right.
- Only top-level constants of the root file are checked and listed; a
  `#[measured]` constant inside a `mod` is not looked at yet.

## Why

Physics and signal work in this ecosystem keeps numbers whose origin matters to
the result: a measured Strouhal number, an energy budget, a speed of sound.
When such a value travels without its source, a reader cannot tell a measured
input from a guess. The receipt now says which inputs rest on outside
measurements and where those came from.

Tests: `compiler/tests/measured_constants.rs` and
`tests/reject/measured_without_source.bld`.
