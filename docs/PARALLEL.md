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
| BL0407 | The checker can see the body. A function value or a capturing closure is refused. | A function value hides its effect row; data a closure would capture goes through `par_for`'s context argument instead. |

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

RESULTS_PLACEHOLDER

## What this is not

Bend parallelizes recursion with no annotation: any balanced call tree runs in parallel.
BuildLang does not claim that, and does not claim parity with it. The claim BuildLang tests is
narrower: an explicit `par_for` or `par_map` whose safety the checker proves from the effect
row, with output identical to the serial run.
