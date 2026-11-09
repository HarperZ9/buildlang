# Translation validation for the C backend

`buildc tv FILE` compiles FILE through the C backend, writes a certificate for every function, and
hands the source, the C and the certificates to an independent checker. A function is **certified**
when the checker proves that its C does what its source says, for every input: the same return
value, the same traps, the same non-termination. The guarantee holds for the C that the user's C
compiler builds, so a miscompilation in BuildLang's own lowering or backend is caught function by
function.

```
buildc tv prog.bld                      # one line per function, then coverage
buildc tv --json --out certs prog.bld   # keep prog.c and prog.tv-cert.json
buildc tv-check prog.bld certs/prog.c certs/prog.tv-cert.json   # the checker alone
```

`tv` exits 1 when a function inside the subset is not certified.

## The certificate

`buildlang-tv-cert/v1` holds, for each function: its name, the SHA-256 of its source text and of its
emitted C text, and the relation the checker must prove: which C label heads each source loop (loops
numbered in source pre-order) and which C variable holds each source variable. The relation is a
claim. A wrong one only makes the checker refuse the function.

## The checker

The checker in `compiler/src/tv/check` re-derives each certificate from the three texts:

1. It hashes the function on both sides and compares with the certificate.
2. It parses the emitted C itself: declarations, labels, assignments, conditional and plain `goto`,
   `return` and call statements. It reads no data from the code generator and imports nothing from
   the code generator or the MIR; `compiler/tests/tv_c.rs` holds that boundary. It shares the
   lexer, parser and AST with the compiler, and the term and proof modules with `buildc verify`.
3. It runs both sides symbolically, segment by segment between cut points (the entry and each loop
   head). Source semantics come from the AST, with the language's trapping arithmetic. C semantics
   come from a table of the runtime helpers (`bl_iadd_T` is `+` that traps outside `T`,
   `bl_idiv_T` traps on a zero divisor, and so on) and the C operators.
4. At each cut both sides start from one shared state. It proves that the two sides finish the
   segment on the same inputs, so they trap alike, and that every source path and C path that can
   both run reach the same exit with equal variables or equal return values. Induction over the
   cuts gives equal behaviour for every input.

Calls are uninterpreted on both sides: the same callee with the same arguments. Each callee is
certified on its own.

## Scope

Functions whose parameters, return and locals are integers up to 64 bits, `f32`, `f64` or `bool`,
with `let`, assignment, `if`, `match`, `while`, `loop`, `break`, `continue`, `return` and calls.
Named constants with a literal value, and the built-in `PI`, `E` and `TAU`, are their values; the C
side's constants are read from the C file's top-level definitions. Everything else is reported as
outside the subset with the reason: heap values, strings, effects, `for`, loops nested inside
expressions, and C the parser does not read.

Floats are opaque IEEE values. Each float operation is an uninterpreted function named by operator
and width, each comparison an uninterpreted predicate (never rewritten: with NaN, `!(a < b)` is not
`a >= b`), each literal its exact bit pattern, each conversion an uninterpreted function of the
value. Maths methods and builtins go through one table in `compiler/src/verify/float.rs` that names
the C function each one lowers to. The two sides agree only when they apply the same operations to
the same operands in the same order, so no algebraic identity of real numbers is ever assumed.

Trusted base, in plain words: the runtime helpers' C text matches the checker's table; the C
compiler compiles the certified C faithfully; signed left shift wraps as two's complement (the
backend emits `x << (n & 31)` on signed types); a call whose result is dropped is assumed to be
dropped on both sides alike; the C compiler evaluates `double` and `float` arithmetic in IEEE 754
binary64 and binary32 with no contraction (the prelude sets `FP_CONTRACT OFF`) and no excess
precision; each C maths function named in the float table is the meaning of the source builtin that
lowers to it.

## Coverage

Functions certified out of functions compiled, `buildc tv` over every program in `tests/programs`
that compiles and every module in `stdlib`:

| Corpus | Compiled | Certified | Not certified | Outside the subset |
|---|---|---|---|---|
| `tests/programs` (147 of 154 programs compile) | 435 | 159 (37 %) | 0 | 276 |
| `stdlib` (14 of 16 modules compile) | 139 | 58 (42 %) | 0 | 81 |

Before floats (main 7038d3d): 31 (7 %) and 22 (16 %); after floats 152 and 58; after constants 159
and 58. Most functions still outside the subset print, or take or return strings, structs, arrays,
tuples, `Result` or `Option` (266 of 276 in `tests/programs`; about 140 of them are `main` with
`~ Console`). The other 10 use constructs the checker does not model yet: match patterns (4), C
lines the parser does not read (3), a loop inside an expression, a callee that is not a name, and
one implicit float to integer conversion (below).

## Planted checks

- `tests/tv/subset.bld`: nine functions inside the subset (branches, `match`, a `while` sum, Euclid's
  loop, `loop` with `break`, early `return`, calls); all nine are certified.
- With `BUILDLANG_PLANT_MISCOMPILE=add-to-sub`, a test-only switch that makes the backend emit the
  checked subtract helper for a checked `+`, the three functions that add are not certified and the
  other six stay certified.
- A forged certificate (a wrong variable map, a wrong loop head) and tampered C (an add turned into a
  subtract, with and without a rewritten hash) are refused.
- `tests/tv/floats.bld`: seventeen float functions, all certified; with
  `BUILDLANG_PLANT_MISCOMPILE=fadd-to-fsub` the six that add floats are not certified; eight C
  rewrites that are wrong under IEEE 754 are refused (`compiler/tests/tv_float.rs`).

## A mismatch the checker found

`fn lp(n: i64) -> i64 { let mut i = 0; loop { if i >= n { break; } i += 2; } i }` passes `buildc
check`, but the backend declares `i` as `int32_t`, so the C traps once `i` passes 2^31 while the
source, where `i` takes `n`'s type, does not. The checker reports `lp` not certified ("C traps where
the source does not"). Running it with a step of 10^9 up to 5 * 10^9 aborts with "attempt to add with
overflow". Filed as issue #159 and fixed: the checker now records the integer type it infers for an
unannotated `let`, lowering emits that type when it is a wider signed integer, and `lp` is certified.
The regression tests are in `compiler/tests/int_let_inference.rs`.

## Widening 1: floats (preregistered 2026-11-09, before the run)

Re-measured on main 7038d3d (after #162, #167 and #172) before any change: `tests/programs` 31 of 435
certified, 0 not certified; `stdlib` 22 of 139, 0 not certified. The counts did not move. The top
refusal by far is the signature: 397 of 404 in `tests/programs`. A signature survey puts about 146 of
them at `main` with effects (printing), about 160 at signatures with `f32` or `f64`, and the rest at
structs, arrays, references, tuples, `Result` and `Option`.

The change: `f32` and `f64` parameters, locals and returns. Each float operation (`+ - * / %`,
negation) is an uninterpreted IEEE operation on both sides, named by operator and width; each float
comparison is an uninterpreted predicate, never folded or rewritten into another comparison (NaN makes
`!(a < b)` differ from `a >= b`); a literal is its exact bit pattern; a conversion (int to float,
float to float, `bl_f2i_T`) is an uninterpreted function of the value. Maths methods and builtins map
through a fixed table to the C function each lowers to (`x.sqrt()` to `sqrt`, `sin(x)` to
`build_det_sin`, `x.min(y)` to `build_min_f64`, and so on). Two sides agree only when they apply the
same operations to the same operands in the same order. The trusted base grows by one line: the C
compiler evaluates `double` and `float` arithmetic in IEEE 754 binary64 and binary32 with no
contraction (the prelude sets `FP_CONTRACT OFF`) and no excess precision.

Predictions: `tests/programs` from 31 to between 110 and 180 of 435; `stdlib` up by fewer than 20.
Any not-certified function is investigated as a possible real miscompile before the feature merges.

Planted checks, each must hold or the change does not merge:

- F1. `BUILDLANG_PLANT_MISCOMPILE=fadd-to-fsub` (new, test only) makes the backend emit a float `-`
  for a float `+`. Every certified function in the float fixture that adds floats becomes not
  certified; the others stay certified.
- F2. The integer plant `add-to-sub` is still caught exactly as before.
- F3. A refusal set of C rewrites that look harmless but change IEEE results, each fed to `tv-check`
  with a rewritten hash, must all be refused: `x == x` to `true`, `x - x` to `0.0`, `(a + b) + c` to
  `a + (b + c)`, `x * 0.0` to `0.0`, `!(a < b)` to `a >= b`, `-(a - b)` to `b - a`, a literal off by
  one unit in the last place, and a round trip through `float` dropped.

### Results (2026-11-09)

- Coverage: `tests/programs` 31 to 152 of 435, inside the predicted 110 to 180. `stdlib` 22 to 58
  of 139: the prediction of fewer than 20 more missed, because `stdlib/math.bld` and
  `stdlib/graphics.bld` are mostly scalar float functions.
- F1 held: with `fadd-to-fsub`, the six functions of `tests/tv/floats.bld` that add floats are not
  certified and the other eleven stay certified (`compiler/tests/tv_float.rs`).
- F2 held: `compiler/tests/tv_c.rs` passes unchanged.
- F3 held: all eight IEEE-unsafe rewrites are refused as not certified, each with its hash rewritten.
- `buildc verify` keeps floats outside its subset: a float law is reported unknown, never proved.
- One function was not certified on the first run, `lab_f` in `tests/programs/86_color_spaces_port.bld`.
  The cause was the checker's own inference: `let delta = 6.0 / 29.0` took the integer fallback type.
  Arithmetic over float literals now types as `f64`, and `lab_f` is certified.
- Refused, and correctly so: `to_fixed_16_16` in `tests/programs/80_icc_profile.bld` returns
  `floor(fixed)`, an `f64`, from a function declared `-> i32`. `buildc check` accepts it and the C
  converts with a plain assignment (truncation, undefined out of range), not the saturating
  `bl_f2i_i32` that `as i32` uses. The type checker should reject the implicit conversion.
- Found while probing the backend, not by the checker (constants are outside the subset): a user
  `const PI: f64 = 3.0;` is ignored, and every use of `PI` compiles to the built-in
  3.141592653589793 (`lower_ident` checks `PI`, `E` and `TAU` before user constants). Modelling
  constants in the checker is the next widening; it turns this into a refusal.

## Widening 2: named constants (preregistered 2026-11-09, before the run)

After floats, 7 functions in `tests/programs` are refused only for an unknown name: five read the
built-in `PI`, two read a user `const`. The change: a `const` with a literal value (integer, float or
bool, possibly negated) is that literal on the source side; the built-ins `PI`, `E` and `TAU` are the
`f64` constants from the language's table when the program defines no constant of that name. On the C
side the checker reads each top-level `const` definition from the C text itself.

Predictions: `tests/programs` gains 5 to 7 certified functions; 0 new not-certified on corpus
programs.

Planted checks:

- K1. `tests/tv/consts.bld` uses a user integer constant, a user float constant, a negative constant
  and the built-in `PI`; every such function is certified.
- K2. A program that defines `const PI: f64 = 3.0;` and reads it: the function is not certified on
  main today, because the backend uses the built-in value (issue #177). Once #177 is fixed it must be
  certified.
- K3. A C rewrite that changes one constant's initializer is refused. (The function hashes do not
  cover top-level C definitions, so no hash needs rewriting: the checker reads the initializer.)

### Results (2026-11-09)

- Coverage: `tests/programs` 152 to 159 of 435 (37 %), at the top of the predicted 5 to 7; the five
  `PI` readers and both user constants are certified. `stdlib` unchanged at 58 of 139. 0 not
  certified in either corpus.
- K1 held: the four constant readers in `tests/tv/consts.bld` are certified.
- K2 held as predicted: `area` in `tests/tv/shadow_pi.bld` is not certified ("a source path and a C
  path disagree"): the source multiplies by 3.0, the C by 3.141592653589793. The checker now
  catches issue #177.
- K3 held: changing `SCALE`, `HALF` or `ENABLED` in the C makes its reader not certified
  (`compiler/tests/tv_consts.rs`).

## Verified

- `compiler/tests/tv_c.rs`, `compiler/tests/tv_float.rs` and `compiler/tests/tv_consts.rs`: the
  planted checks above and the import boundary.
- The coverage table: `buildc tv --json` over `tests/programs/*.bld` and `stdlib/*.bld`, counts summed
  from the reports.

Nothing here compares this with Anthropic and Theorem's proofs over compiled binaries on a shared
benchmark; that comparison has not been run.
