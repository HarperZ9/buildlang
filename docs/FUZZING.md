# Fuzzing and linting

BuildLang checks itself with generated programs and with its own lint. Every
target here uses fixed seeds that are recorded in the test file, so a failure
names a seed and a case index and reproduces on any machine. Nothing here
needs a tool outside the repository: the generators are a few lines of Rust on
a SplitMix64 stream.

## Rules for a fuzz target

1. **A reference.** Each target compares the compiler with something that was
   written separately: Rust checked arithmetic, the interpreter, a second
   backend, or a published test vector.
2. **Recorded seeds.** Seeds are constants in the test. A new seed that finds a
   defect is added to the list.
3. **Minimize, then keep.** A mismatch is shrunk to a small case and that case
   becomes a named regression row.
4. **A planted defect.** Each target carries a deliberate fault that it must
   catch. A target that passes with the fault in place is broken.
5. **Short runs.** A target finishes in seconds inside `cargo test`.
6. **A skipped run is visible.** `BUILDLANG_FUZZ_REQUIRE_CC=1` turns a run
   skipped for want of a C compiler into a failure. CI sets it on Linux.

## Targets

| Stage or kernel | Reference | State |
|---|---|---|
| Integer expressions, source to native code | Rust checked `i64` arithmetic | Built: `compiler/tests/fuzz_int_expr.rs` |
| Lexer and parser | Print the syntax tree, parse the text again, compare | Planned |
| Type checker | A generated program that is well typed by construction must check; one planted type fault must be refused | Planned |
| Lowering to the middle representation | The interpreter on the same program | Planned |
| C backend against the wasm and x86 backends | The same program through two backends | Planned, after the wasm backend handles calls and loops |
| Float expressions | Rust `f64` with the same operation order, compared bit for bit | Planned |
| Vectors: push, index, clear, alias | A Rust `Vec` model | Planned |
| `buildc tv` | A planted wrong translation must be refused | Built: `compiler/tests/tv_c.rs` |
| `buildc build-hash` | A planted run counter must be reported | Built: `compiler/tests/build_hash.rs` |
| Kernel `sum-det` | Exact rational sum for the goldens; `sd_i8_ref` for the slot, 20000 seeded inputs of length 0 to 4999 | Built: `registry/packages/sum-det/0.1.0`, `sum-det fuzz SEED COUNT` |
| Kernel `sha256` | Published test vectors and a second implementation | Planned with the kernel |
| qdb | SQLite on generated queries | Built: `bdb/tests/test_qdb_differential.py` |

## The integer expression target

- 3 seeds, 150 expressions each, depth 4, over `+ - * / % & | ^ << >>`, unary
  minus and `if` with a comparison. Arguments come from a pool weighted toward
  the edges of `i32` and `i64`.
- Each seed is one program: 150 functions and 150 printed values. The 450
  values match the reference.
- 8 expressions that the reference refuses (overflow or division by zero) each
  stop with exit code 101 and print nothing.
- Planted defect 1: a reference whose remainder takes the wrong sign disagrees
  with the true reference on the first seed, and the shrinker reduces the case
  to at most 5 nodes that still contain `%`.
- Planted defect 2: one corrupted output line is reported at its index.

What this target does not cover: unsigned and narrow integer types, mixed
widths, loops, and values that reach the expression through memory.

### Found while building it

- A left shift is emitted as a C shift on a signed value. gcc and clang give
  the two's complement result that the reference expects, and the C standard
  before C23 leaves it undefined for a negative operand. The emitted code
  should shift the unsigned value and convert back.
- `buildc lint` reported trailing whitespace only on indented lines. Fixed,
  with `compiler/tests/lint.rs`.

## Lint in CI

CI runs `buildc lint` over every tracked source under `examples/` and `bench/`
and fails on any error. Two held-out control programs carry planted type
faults; the step fails unless lint refuses both.

`buildc lint` reports type errors, parse errors, trailing whitespace, lines
over 120 characters and TODO, FIXME and HACK markers. It has no check for
unused bindings, shadowing or unreachable code yet.
