# Parallel map and for

`par_for` and `par_map` run a function over many indices at once on the C backend. The
checker proves, from the source, that the function can run on any thread, in any order, with
no lock. The result is the vector a serial loop would build, byte for byte, at every thread
count.

```text
let next = par_for(n, &world, advance);   // next[i] == advance(&world, i)
let ys   = par_map(&xs, f);               // ys[i]   == f(xs[i])
```

## What the checker proves

A parallel body is a named function or a closure that captures nothing and writes its
parameter and return types. Three rules, each with its own code:

| Code | Rule | Why |
|---|---|---|
| BL0406 | The body's effect row is empty: no `Console`, `FileSystem`, `Network`, `Clock`, `Random`, `Foreign` or user effect, and no call to a function that declares one. | An effect is an observable order. Two threads printing interleave; two threads reading a clock see different values. |
| BL0515 | The body cannot write shared state: no `&mut` or `*mut` parameter, no write to a `static mut` (directly or through any function it calls), and in a closure no write to a variable from outside it. | Without a write to shared state there is no data race, so no lock is needed. |
| BL0407 | The checker can see the body. A function value or a capturing closure is refused. | A function value hides its effect row; pass data a closure would capture through `par_for`'s context argument. |

The ordinary effect pass already proves a function with no `~` row performs no effect, so the
parallel check only has to read the body's declared row and its parameters.

### The BL0406 fix

The fix is exact edits in the diagnostics JSON (`buildc check --error-format=json`, the
`edits` of the first suggestion). It moves the effectful top-level statements of the body
into a serial `for` loop placed after the statement that holds the call. The loop binds the
body's parameters for each index, so a statement that reads only those parameters prints the
same bytes in the same order as the serial program. The body loses its effect row; the
enclosing function gains the effect if it lacks it.

```text
fn shade(s: &Scene, i: i64) -> f64 ~ Console {     fn shade(s: &Scene, i: i64) -> f64 {
    println!("pixel {}", i);                            (i as f64) * s.scale
    (i as f64) * s.scale                            }
}                                              =>
let img = par_for(4, &scene, shade);               let img = par_for(4, &scene, shade);
                                                    for i in 0..4 {
                                                        println!("pixel {}", i);
                                                    }
```

The fix is `MaybeIncorrect`, never `MachineApplicable`: adding an effect to the enclosing
function widens its authority, and a moved statement that reads one of the body's own locals
needs a hand edit (a note names those locals). When an effect sits anywhere other than a
top-level statement (a `let` initializer, a nested block, the tail), the diagnostic carries no
edits and says so.

## How it runs

Lowering turns each call into one call of a per-body C wrapper. The wrapper allocates the
result vector and hands the index range to a fixed thread pool (Win32 threads and condition
variables on Windows, pthreads elsewhere). Threads take contiguous chunks of
`n / (threads * 8)` indices under one mutex; the body's result for index `i` goes to slot `i`.
No two indices share a slot, and the body is the same C function in every mode, so the
schedule cannot change the result.

- `BUILD_PAR_THREADS` sets the pool size. The default is the logical processor count (at most
  256). `BUILD_PAR_THREADS=1` runs every index on the calling thread, in order.
- A `par_for` inside a parallel body runs serially on the thread that made it.
- The runtime's number formatters keep their rotating buffers per thread, so a body may call
  `to_string()` or format numbers.
- A panic inside a body ends the program. When several indices would panic, which message
  prints first can depend on the schedule.
- Only the C backend lowers `par_for` and `par_map`. The experimental backends refuse them.

## Measured speed-ups

Ten rounds per benchmark. Each round runs the serial loop, `par_for` at 1 thread and
`par_for` at 8 threads once, in a shuffled order, at below-normal priority. The speed-up is
the serial loop's time over the 8-thread time, paired within a round; the table gives the
median of the 10 rounds and a percentile bootstrap 95 % interval of that median (10,000
resamples). Kernel time is measured inside the program around the compute section; wall time
is the whole process, including setup and the serial output loop.

Machine: Intel i7-13700KF (8 performance cores with 2 threads each, 8 efficiency cores; 24
logical processors), Windows 11, gcc 13.2 `-O2`. The machine was not quiet: a GPU training
job held the GPU at 96 %, and the CPU was 26-32 % busy (median over runs, sampled for 0.5 s
before each run) with other work. Raw rows, load samples and output hashes are in
`bench/parallel-2026-11/results/bench-20261107-201256.json`.

| Benchmark | Size | Serial kernel | 8-thread kernel | Kernel speed-up | Wall speed-up | 1-thread / serial | Byte-identical |
|---|---|---|---|---|---|---|---|
| n-body step | 4,096 bodies, 40 steps | 1,676 ms | 314 ms | **5.34x** [5.07, 5.63] | 3.20x [3.07, 3.31] | 0.99 | 30 of 30 runs |
| per-pixel shading | 3,200 x 2,400, 25 spheres, 2 lights, shadows | 1,275 ms | 217 ms | **5.87x** [5.57, 6.05] | 4.58x [4.39, 4.77] | 1.00 | 30 of 30 runs, plus the full 70 MB image |
| Low Water 40-fight corpus | 40 fights, one per index | 2,466 ms | 486 ms | **5.09x** [4.94, 5.37] | 3.87x [3.71, 4.08] | 1.00 | 30 of 30 runs; 40 of 40 traces equal the JS sim's |

The M5 bar is at least 4x on 8 cores for the n-body and pixel kernels, judged on the lower end
of the interval, with byte-identical output on every benchmark. Both kernels pass: 5.07 and
5.57 are above 4. The whole-process speed-up of the n-body program does not reach 4x
(3.20x). The 0.3 s between wall and kernel time is serial work: setup and printing 12,288
shortest-round-trip floats at the end.
Two earlier runs on the same day agree (n-body kernel 5.43x and 5.49x, shading 6.38x and
6.50x, the corpus 5.39x; consoles in `results/`).

Eight threads reach 5-6x, not 8x. Windows places the eight threads on a mix of performance
and efficiency cores, and the machine was shared; neither was controlled. The 1-thread column
shows the pool costs nothing measurable when it does not split the work.

The corpus is the Low Water combat step ported to BuildLang, kept in the game's private
repository; its source and traces are not published here. Its `main` was changed only to run
the 40 fights through `par_for(40, &fights, fight_text)`, and the checker accepted the whole
call tree under the fight (empty effect rows, no `static mut` writes).

Planted controls, all caught:

- Nine unsafe bodies in `tests/parallel/reject` (a print, a file write, a user effect, a print
  in a closure, a `&mut` parameter, a `static mut` written through a helper, a closure writing
  an outer variable, a function value, a capturing closure) are refused with their codes.
- `bench/parallel-2026-11/control/plant_reduction.py` patches the n-body C so each chunk adds
  its kinetic energy to one shared sum in the order chunks finish. The byte check flagged 10
  of 10 runs at 8 threads against the 1-thread run, with 2 distinct outputs among the 10.
- With the formatters' buffers shared again, a body that calls `to_string()` corrupted 2
  strings in 1 of 5 runs at 3 threads; with per-thread buffers, 0 of 5.

## What this is not

Bend parallelizes recursion with no annotation: any balanced call tree runs in parallel.
BuildLang does not claim that, and does not claim parity with it. The claim BuildLang tests is
narrower: an explicit `par_for` or `par_map` whose safety the checker proves from the effect
row, with output identical to the serial run.
