# sum-det 0.1.0

Add a list of binary64 floats and get the same bits on every machine. A float sum depends on the
order of its additions, so each function here fixes one order and the tests hold it to that order
bit for bit. Two more functions return the exact sum, rounded once.

```
buildc pkg build --release
target/pkg/sum-det sum i8 tests/hand.txt        # one result per input line, as 16 hex digits
target/pkg/sum-det fuzz 12345 20000             # slot against its reference, seeded
target/pkg/sum-det bench i8 65536 12000 1       # prints milliseconds and a checksum
```

## Contract

Input: `n >= 0` finite binary64 values. No partial sum in the chosen order may overflow. NaN and
infinities are outside the contract, and no test covers them.

Output: one binary64 value, class `exact-ieee`. Every step is one IEEE 754 addition, round to
nearest with ties to even. No fused multiply-add, no reassociation, subnormals exact. The C
backend builds with `-ffp-contract=off -fno-fast-math`, which keeps the compiler from changing the
order.

| Order | Function | Additions, in evaluation order |
|---|---|---|
| `serial` | `sd_serial` | `s = +0.0`, then `s = s + x[i]` for `i = 0 .. n-1` |
| `tree2` | `sd_tree` | `n = 0` gives `+0.0`; `n = 1` gives `x[0]`; else split at `p`, the largest power of two strictly below `n`, and return `tree(x[0..p]) + tree(x[p..n])` |
| `interleave8` | `sd_i8`, `sd_i8_ref` | eight lanes start at `+0.0`; `lane[i mod 8] = lane[i mod 8] + x[i]` for `i = 0 .. n-1`; result `((l0 + l1) + (l2 + l3)) + ((l4 + l5) + (l6 + l7))` |
| exact | `sd_fsum` | Shewchuk non-overlapping partials, then one rounding of the exact sum |
| `binned` | `sd_binned` | every input as an integer times 2^-1074, added exactly in 32-bit limbs, then one rounding |

`sd_fsum` and `sd_binned` return the binary64 value nearest the exact sum, and `+0.0` when the
exact sum is zero. They share no code, and neither depends on input order.

An empty input gives `+0.0` in every order. `[-0.0]` gives `-0.0` under `tree2` and `+0.0` under
the others, because their running total starts at `+0.0`.

## Reference

`ref/reference.py` writes every input and golden file from Python floats, one addition per step.
The exact sum has two references that share no code: `math.fsum`, and a sum of
`fractions.Fraction` rounded once. `python ref/reference.py check` exits 1 if any stored file
differs from a fresh write. `tests/SOURCES.txt` gives the digest and origin of each file.

## Tests

- `tests/cases.txt`: 25 hand-written inputs (cancellation, half-way cases, subnormals, values near
  overflow) and 42 seeded inputs of six kinds, for all six order names.
- `tests/props.txt`: seven named properties, 3000 rounds each.
- `sum-det fuzz SEED COUNT`: `sd_i8` against `sd_i8_ref` on seeded inputs of length 0 to 4999.
- `tests/controls.txt`: six planted-wrong slots. A check that passes any of them is broken.
- `holdout/`: cases and benchmark sizes the improvement loop uses only after a variant has
  already cleared the open steps.

## The slot and the improvement loop

`src/i8.bld` is a slot: one pure function, `sd_i8`, that must return the bits of `sd_i8_ref` for
every input. A faster version may replace it only through the improvement loop, which lives under `improve/`.

```
python improve/spawn.py --buildc buildc lane-counter unroll8 unroll16
```

Each variant is a folder holding `src/i8.bld` and `NOTE.txt`. The loop takes the controls first,
then the named proposals, and sends each through these steps. The first failed step refuses it.

1. **Admit.** The folder holds only the slot. The slot has no effect row, no file, clock,
   environment or exit call, and every function it adds is named `sdv_*`.
2. **Build, lint, translation validation.** Lint is clean, and `buildc tv` certifies every function
   it certified in the baseline, with a count that does not fall.
3. **Cases and properties.** All of `tests/cases.txt` and `tests/props.txt`.
4. **Fuzz.** 20000 rounds with a seed drawn for the run and written to the ledger.
5. **Paired benchmark.** 20 rounds of baseline then variant, one process per measurement, at the
   open size. The gain per round is `(a - b) / a`. The variant goes on only if the lower end of the
   95 percent interval of the mean gain is above 5 percent, its checksums equal the baseline's,
   and its peak memory is within 5 percent of the baseline's.
6. **Hold-out.** The hold-out cases, and the paired benchmark at each hold-out size, where the
   checksums must agree and the whole interval of the gain may not lie below zero. A variant that
   fails here was fitted to the open set and is refused. Only pass or fail reaches the ledger.
7. **Decision.** A variant that clears every step is `eligible`. At the end of the run the
   eligible variant with the highest lower bound replaces the slot, provided every control was
   refused and the frozen files are unchanged. The bytes written are the ones that were built
   and measured.

Controls go through steps 1 to 4 and the hold-out cases in every run. A control that no step
refuses voids the run, and a void run promotes nothing.

### What the loop cannot change

`improve/FROZEN.sha256` lists the SHA-256 of the cases, goldens, inputs, properties, controls,
hold-out files, the benchmark rule, the reference, every source file except the slot, the loop
kernel and the spawner. The kernel checks every digest before the first step and again before a
promotion, refuses to start if a required file is missing from the list, and writes the digest of
the list into each ledger line. The spawner refuses to start unless the listed files match the
last commit. A change to any of them is a change to `FROZEN.sha256` in a reviewed pull request.
The kernel itself writes to three places: its working folder under `target/`, the end of the
ledger, and the slot.

### Ledger

`improve/LEDGER.jsonl` has one line per variant and one per run, refusals included. Each line
carries the digest of the line before it and its own index, so `improve-kernel verify` detects a
removed, edited or reordered line.

### Parts

`improve/kernel/` is the loop, written in BuildLang: the frozen check, the admission rule, the
judges, the interval and the ledger. BuildLang has no builtin that starts a process, so
`improve/spawn.py` runs the commands the kernel prints. It judges nothing.

### Limits

- The hold-out files sit in the same repository as the variants. Whoever writes a variant can
  read them. The loop keeps hold-out numbers out of the ledger; it cannot keep a person out of a
  folder.
- The fuzz run stops at length 4999. A fault that needs a longer input is left to the hold-out
  cases, which reach 100003.
- `buildc tv` does not certify a function that takes a vector, so it does not prove the slot
  equal to the reference. Its step guards the certified helpers only.
- The admission rule reads text. It is a filter for the listed words, with the effect checker of
  the compiler behind it.
- Timings come from one machine. The benchmark result file names it.
