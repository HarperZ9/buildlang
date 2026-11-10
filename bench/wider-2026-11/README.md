# Wider benchmark set: BuildLang against Rust and C

Four more Benchmarks Game kernels, single thread, the same algorithm and operation order in each
language. The predictions in `PREDICTIONS.md` were written before any timed run.

- `fannkuch`: fannkuch-redux, n = 11.
- `mandel`: mandelbrot, 6000 x 6000, 50 iterations, a hash of the bitmap in place of the image.
- `bintree`: binary-trees, depth 20, with the nodes in an index arena. `Option<Box<T>>` does not
  compile through the C backend yet, so the pointer form is not measured.
- `fasta`: the random-select kernel of fasta, 50,000,000 picks, counts and a hash in place of text.

## Variants

Windows 11, Intel Core i7-13700KF, gcc 13.2.0, rustc 1.98.0. The compiler is buildc 1.13.0 plus the
`Vec::clear` fix.

- `bl_O2`, `bl_O3`, `bl_O3n`: buildc's emitted C with gcc -O2, -O3, and -O3 -march=native.
- `rust`: `rustc -C opt-level=3`. `rust_n` adds `target-cpu=native`, one codegen unit and `panic=abort`.
- `c_O2`, `c_O3`, `c_O3n`: a hand-written C reference with the same three gcc settings.

One warm-up run per variant, then 10 rounds with the order shuffled in each round (seed 7). Median
wall time, the paired ratio against `c_O2`, and a bootstrap 95 % interval (10,000 resamples, seed 1).
Every variant printed the same bytes on every benchmark (line endings normalised).

## Results (2026-11-09, `results/run-2026-11-09.json`)

Median seconds, then the ratio against `c_O2` with its interval where one is given.

| Variant | fannkuch 11 | mandel 6000 | bintree 20 | fasta 50M |
|---|---|---|---|---|
| `bl_O2` | 1.604, 0.98 (0.97-0.99) | 1.436, 0.99 (0.98-0.99) | 2.356, 2.41 (2.31-2.44) | 0.477, 1.26 (1.25-1.27) |
| `bl_O3` | 1.644, 1.00 | 1.432, 0.98 | 1.832, 1.87 (1.79-1.90) | 0.472, 1.26 |
| `bl_O3n` | 1.619, 0.99 | 1.457, 1.00 | 1.831, 1.88 | 0.476, 1.26 |
| `rust` | 1.708, 1.04 (1.03-1.05) | 1.427, 0.98 | 1.363, 1.38 | 0.450, 1.19 |
| `rust_n` | 1.644, 1.00 | 1.393, 0.96 | 1.345, 1.37 | 0.437, 1.16 |
| `c_O2` | 1.637 | 1.456 | 0.979 | 0.379 |
| `c_O3` | 1.419, 0.86 | 1.452, 1.00 | 0.927, 0.94 | 0.378, 1.00 |
| `c_O3n` | 1.413, 0.86 | 1.361, 0.94 | 0.886, 0.90 | 0.383, 1.01 |

Fastest BuildLang variant against Rust and against the fastest C, paired per round (`paired.py`).
The BuildLang variant is picked after the run, so these ratios are a best case for BuildLang.

| Benchmark | against `rust` | against `rust_n` | against `c_O3n` |
|---|---|---|---|
| fannkuch 11 | 0.94 (0.93-0.96) | 0.98 | 1.14 |
| fannkuch 11, second run | 0.97 (0.96-0.98) | 1.00 (0.99-1.03) | 1.14 |
| mandel 6000 | 1.00 (1.00-1.01) | 1.03 | 1.05 |
| bintree 20 | 1.36 (1.33-1.39) | not computed | 2.08 |
| fasta 50M | 1.05 (1.04-1.06) | 1.09 | 1.24 |

## Reading

- fannkuch-redux: BuildLang is 3 to 6 % faster than Rust at default flags in two runs, level with
  Rust at `target-cpu=native`, and 14 % slower than gcc -O3 -march=native on the C reference.
- mandelbrot: BuildLang, Rust and C -O2 are level within 2 %.
- binary-trees: BuildLang is 36 % slower than Rust and 1.9 to 2.4 times slower than C. Pushes and
  recursive calls go through the runtime vector handle.
- fasta: BuildLang is 5 % slower than Rust and 26 % slower than C. The checked multiply and
  remainder run on every step.
- BuildLang leads Rust on one of the four kernels at default flags and on none at native flags.

## Predictions against results

| Benchmark | Predicted `bl` / `c_O2` | Measured | Predicted `rust` / `c_O2` | Measured |
|---|---|---|---|---|
| fannkuch | 1.3 to 2.2 | 0.98: missed, faster than predicted | 0.9 to 1.2 | 1.04 |
| mandel | 0.95 to 1.15 | 0.99 | 0.95 to 1.10 | 0.98 |
| bintree | 1.2 to 2.5 | 2.41 at -O2, 1.87 at -O3 | 0.9 to 1.3 | 1.38: missed |
| fasta | 1.0 to 1.4 | 1.26 | 0.95 to 1.15 | 1.19: missed |

The prediction that BuildLang beats Rust on none of the four missed on fannkuch. Per the
preregistered rule it got a second run before being reported; the lead over default-flag Rust
held (0.97, interval 0.96 to 0.98) and the lead over native Rust did not.

## Limits

- The machine was in use: CPU load before the rounds was between 10 % and 41 %. Shuffled order and
  paired ratios reduce the effect and do not remove it.
- The first sizes (fannkuch 10, mandelbrot 2000, binary-trees 16) ran under 0.2 s on three
  kernels, so the sizes were raised by the rule in `PREDICTIONS.md`. The small-size run is kept in
  `results/run-2026-11-09-small-sizes.json`.
- One machine, one operating system, gcc only. clang was not available.
- BuildLang traps on integer overflow and on an index out of range. Rust in release mode wraps on
  overflow, and the C reference checks neither.

## Reproduce

```
BUILDC=path/to/buildc RUSTC="rustup run 1.98.0 rustc" sh mk.sh fannkuch 7
python run.py --runs 10 --out results/run.json fannkuch:11 mandel:6000 bintree:20 fasta:50000000
python paired.py results/run.json
```
