# BuildLang against Rust and C: speed baseline

Preregistered 2026-11-09, before any timed run; the results section was added after the run.

Author target, 2026-11-09: "BuildLang should be immensely faster than Rust, and much more
dterministic and precise." Speed against Rust has never been measured. This run measures it.

## Set and variants

Benchmarks Game programs, single thread, the same algorithm and operation order in each language:
n-body (5 bodies, `{:.9}` energies) and spectral-norm (`{:.9}`). Planned next, not in this run:
fannkuch-redux, mandelbrot, binary-trees, fasta, a qdb query workload and a superframe kernel.

Variants on this machine (Windows 11, Intel Core i7-13700KF, 24 logical CPUs, gcc 13.2.0 MinGW-W64 UCRT, rustc 1.92.0 with LLVM 21.1.3):

- `bl_O2`, `bl_O3`: buildc's emitted C (`buildc build --release --keep-c`), compiled with gcc -O2 and -O3.
- `rust`: `rustc -C opt-level=3` (no target-cpu flag, matching gcc's default x86-64 target).
- `c_O2`: a hand-written C reference with gcc -O2.

Each run's stdout must be byte-identical across variants (CRLF normalised). 10 rounds, order
shuffled per round, one warm-up run per variant; median wall time with a bootstrap 95 % interval,
and the paired ratio against `c_O2`.

## Predictions (my own estimates, written before any timing)

- Rust against C -O2: ratio 0.85 to 1.15 on both.
- BuildLang -O2 against C -O2: n-body 1.2 to 3.0 (bodies live in a runtime vector handle; every
  read and write goes through the runtime and a bounds check); spectral-norm 1.0 to 1.6 (checked
  64-bit integer arithmetic in `a(i, j)` and bounds-checked vector reads).
- BuildLang is not faster than Rust on either kernel. A lead, if any, needs proved-check removal.

## Results (2026-11-09, `results/run-2026-11-09.json`)

Median wall time of 10 rounds, bootstrap 95 % interval, and the paired ratio against `c_O2`. Every
variant printed the same bytes.

| Benchmark | `bl_O2` | `bl_O3` | `rust` | `c_O2` |
|---|---|---|---|---|
| n-body, 5,000,000 steps | 1.076 s, 3.36x (3.21-3.39) | 1.060 s, 3.28x (3.15-3.36) | 0.329 s, 1.02x (0.98-1.04) | 0.323 s |
| spectral-norm, n = 3000 | 1.708 s, 2.25x (2.25-2.26) | 1.692 s, 2.23x (2.22-2.24) | 0.754 s, 0.99x (0.99-1.00) | 0.758 s |

- BuildLang is 2.2 to 3.4 times slower than Rust and C on these two kernels. Rust and C are level.
- Predictions: Rust against C held. BuildLang against C missed on the slow side for both (3.36 above
  the predicted 1.2 to 3.0; 2.25 above 1.0 to 1.6). No lead over Rust exists today.
- Where the time goes, read from the emitted C: every vector read and write is a runtime call on a
  `void*` vector (`build_hvec_get_f64`, `build_hvec_set_f64`) with a bounds check, and every integer
  step is a checked helper (`bl_iadd_i64`, `bl_imul_i64`). Rust in release mode does not check
  integer overflow at all, so part of the gap is a semantic difference: BuildLang traps where Rust
  wraps.
- The route to a lead the brief names: drop a bounds or overflow check only where `buildc verify`
  proves it cannot fire, with a planted wrong proof that must not remove a needed check; typed
  vectors lowered to plain C arrays; and checked arithmetic lowered to `__builtin_*_overflow`.

Reproduce: `sh mk.sh nbody 1000 && sh mk.sh sn 100` (builds the four variants and prints their
output hashes), then `python run.py --runs 10 --out results/run.json nbody:5000000 sn:3000`.
Toolchain versions and hashes are recorded outside the repository.

## Second run: checked arithmetic through the overflow builtins (2026-11-09, `results/run-2026-11-09-builtins.json`)

The integer helpers keep every overflow check but use `__builtin_add_overflow`, `__builtin_sub_overflow`
and `__builtin_mul_overflow` under gcc and clang. Before any run, a hand edit of the emitted C put
spectral-norm at 954 ms against 1774 ms, and at 834 ms with the checks removed (C: 828 ms), so the old
multiply check, which divides, was nearly the whole gap there.

| Benchmark | `bl_O2` | `bl_O3` | `rust` | `c_O2` |
|---|---|---|---|---|
| n-body, 5,000,000 steps | 1.090 s, 3.35x (3.19-3.55) | 1.066 s, 3.27x | 0.330 s, 1.01x | 0.326 s |
| spectral-norm, n = 3000 | 0.925 s, 1.20x (1.17-1.22) | 0.773 s, 1.01x (1.00-1.02) | 0.756 s, 0.99x | 0.768 s |

- spectral-norm: from 2.25x to 1.20x of C at -O2 and 1.01x at -O3, with the same traps. Rust stays
  at 0.99x, so BuildLang at -O3 is level with Rust while still checking every overflow Rust wraps.
- n-body did not move: its cost is the generic vector (runtime calls through a `void*` with the
  element size read at run time), not arithmetic.

## Third run: vector accesses as plain C array reads (2026-11-09, `results/run-2026-11-09-typed-vec.json`)

Preregistered before the timed run: a function that cannot resize or replace a vector loads the
vector's buffer and length once at entry and indexes them with `sizeof(T)` as the stride; every
bounds check stays, with the same message and exit code 101. The struct setter stores through a
typed pointer instead of `memcpy`. Prediction: n-body 0.95 to 1.15x of C at -O2 with the hoisting,
1.2 to 1.5x with the typed store alone. Hand edits of the emitted C, timed while a test suite ran
(noisy, directional only): 1167 ms base, about 590 ms typed store alone, about 430 ms hoisted, C
about 410 ms.

| Benchmark | `bl_O2` | `bl_O3` | `rust` | `c_O2` |
|---|---|---|---|---|
| n-body, 5,000,000 steps | 0.319 s, 0.98x (0.93-1.07) | 0.313 s, 0.98x (0.92-1.01) | 0.319 s, 1.02x (0.95-1.04) | 0.324 s |
| spectral-norm, n = 3000 | 0.790 s, 1.06x (1.04-1.09) | 0.749 s, 1.01x (0.99-1.02) | 0.740 s, 1.00x (0.97-1.02) | 0.743 s |

- n-body: from 3.35x to 0.98x of C at -O2, level with Rust (1.02x), every bounds check kept. The
  prediction held. The intervals overlap, so this is a tie with C and Rust, not a lead.
- spectral-norm: from 1.20x to 1.06x at -O2 (its `mul_av` loops now hoist; the call to the scalar
  function `a` is allowed because `a` cannot reach a vector), 1.01x at -O3.
- Which part did what, from the hand edits: a constant stride (`sizeof(T)` instead of the runtime
  element size) was worth about 110 ms of n-body; an entry check that called an abort function
  cost about 100 ms, so a vector whose element size does not match its type gets a hoisted length
  of 0 instead, and its first access stops with a layout message.
- Removing the bounds checks entirely after hoisting (hand edit) showed no difference outside the
  noise on n-body, so proved-check removal has little left to win on this kernel.
