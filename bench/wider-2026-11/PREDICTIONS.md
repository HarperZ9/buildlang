# Wider benchmark set: predictions

Written 2026-11-09 before any timed run. Only smoke runs at small sizes had been done, to check that every
variant prints the same bytes (fannkuch 7, mandelbrot 200, binary-trees 10, fasta 100000). No wall time was
read from those runs.

Compiler: buildc from fix/vec-clear merged with main (1a59a28). gcc 13.2.0, rustc 1.98.0. Sizes: fannkuch 10,
mandelbrot 2000, binary-trees 16, fasta 50,000,000. A size changes only if a run falls under 0.2 s or over
10 s, and the change is recorded.

Ratios are median wall time against `c_O2`.

| Benchmark | bl_O2 | bl_O3 | rust | Reason |
|---|---|---|---|---|
| fannkuch-redux | 1.3 to 2.2 | 1.3 to 2.2 | 0.9 to 1.2 | Stores into the permutation vectors keep bounds checks; hoisting covers reads and the store rule after #225 is conservative. |
| mandelbrot | 0.95 to 1.15 | 0.95 to 1.15 | 0.95 to 1.10 | Float-only inner loop with no vector access. |
| binary-trees (arena) | 1.2 to 2.5 | 1.2 to 2.5 | 0.9 to 1.3 | Index arithmetic is checked and pushes go through the runtime handle. |
| fasta select | 1.0 to 1.4 | 1.0 to 1.4 | 0.95 to 1.15 | Checked multiply and remainder per step; four-entry table read. |

Native builds (`-march=native`, `target-cpu=native`): within 0.85 to 1.05 of the same variant without the
flag on every benchmark. No kernel here has a wide vectorizable loop.

BuildLang is predicted not to beat Rust on any of the four. A win on any of them would be a surprise and
gets a second run before it is reported.
