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

Functions whose parameters, return and locals are integers up to 64 bits or `bool`, with `let`,
assignment, `if`, `match`, `while`, `loop`, `break`, `continue`, `return` and calls. Everything else
is reported as outside the subset with the reason: heap values, strings, floats, effects, `for`,
loops nested inside expressions, and C the parser does not read.

Trusted base, in plain words: the runtime helpers' C text matches the checker's table; the C
compiler compiles the certified C faithfully; signed left shift wraps as two's complement (the
backend emits `x << (n & 31)` on signed types); a call whose result is dropped is assumed to be
dropped on both sides alike.

## Coverage

Functions certified out of functions compiled, `buildc tv` over every program in `tests/programs`
that compiles and every module in `stdlib`:

| Corpus | Compiled | Certified | Not certified | Outside the subset |
|---|---|---|---|---|
| `tests/programs` (147 of 154 programs compile) | 435 | 31 (7 %) | 0 | 404 |
| `stdlib` (14 of 16 modules compile) | 139 | 22 (16 %) | 0 | 117 |

Most functions fall outside the subset because they print, or take or return strings, floats or
structs (397 of 404 in `tests/programs`). Of the functions whose signature is inside the subset, 31
of 38 are certified in `tests/programs` and 22 of 26 in `stdlib`; the rest use constructs the
checker does not model yet (match patterns, enum constants, field access, a loop inside an `if` expression).

## Planted checks

- `tests/tv/subset.bld`: nine functions inside the subset (branches, `match`, a `while` sum, Euclid's
  loop, `loop` with `break`, early `return`, calls); all nine are certified.
- With `BUILDLANG_PLANT_MISCOMPILE=add-to-sub`, a test-only switch that makes the backend emit the
  checked subtract helper for a checked `+`, the three functions that add are not certified and the
  other six stay certified.
- A forged certificate (a wrong variable map, a wrong loop head) and tampered C (an add turned into a
  subtract, with and without a rewritten hash) are refused.

## A mismatch the checker found

`fn lp(n: i64) -> i64 { let mut i = 0; loop { if i >= n { break; } i += 2; } i }` passes `buildc
check`, but the backend declares `i` as `int32_t`, so the C traps once `i` passes 2^31 while the
source, where `i` takes `n`'s type, does not. The checker reports `lp` not certified ("C traps where
the source does not"). Running it with a step of 10^9 up to 5 * 10^9 aborts with "attempt to add with
overflow". Filed as issue #159.

## Verified

- `compiler/tests/tv_c.rs`: the planted checks above and the import boundary.
- The coverage table: `buildc tv --json` over `tests/programs/*.bld` and `stdlib/*.bld`, counts summed
  from the reports.

Nothing here compares this with Anthropic and Theorem's proofs over compiled binaries on a shared
benchmark; that comparison has not been run.
