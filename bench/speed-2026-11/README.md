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
