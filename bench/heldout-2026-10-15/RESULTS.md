# Held-out probe (bench4): results

Preregistration: `PREREG.md`. Every frozen file is hashed in `MANIFEST.sha256`
(SHA-256 of the manifest: 6f9de2cdc162c60d2f86e029c969fbd1070c2f5189ec13cbb58bca15fc322f14),
committed as buildlang commit 3fb9704 on branch
`probe/heldout-commitment-20261015`, pushed 2026-10-15T20:27:54Z, before
either generator was launched. Compiler: buildc 1.6.0 from crates.io.
Generators: two fresh Claude Opus 5.5 subagents (samples A and B); both
report reading only the five allowed files and running nothing. Answer
hashes: `gen/A.sha256`, `gen/B.sha256`.

## Verdict

The bar (pass@1 at least 22 of 27 and zero check-pass-but-fail on sample A)
is missed on both counts.

| | pass@1 | Wilson 95% | check_pass_but_fail | check_pass_not_pass | check_fail |
|---|---|---|---|---|---|
| A (primary) | 20/27 (74%) | 55% to 87% | 4 | 5 | 2 |
| B (secondary) | 17/27 (63%) | 44% to 78% | 4 | 8 | 2 |
| Prediction | 15 to 21 | | 1 or 2 | | |

The pass@1 prediction held. The check_pass_but_fail prediction did not: 4 in
each sample, against 0 on the contaminated run 3. "`buildc check` passes, so
the program builds and runs" is false on held-out tasks for 1.6.0.

Repair round (sample A, one fresh agent given each failure's grader report):
5 of 7 repaired. Caveats: the `bst` repair passes by replaying the insertion
sequence without a recursive type, so it meets the output and not the task;
counting by the task text, 4 of 7. The repair agent read its allowed input
files with `cat` instead of the file-read tool (reported by the agent). One
repaired program (`csv`) exposed a further silent miscompile (below).

## Failures by root cause

Each class has a minimal program in `repro/` that reproduces on 1.6.0.

| # | Class | Answers hit | Checker saw it? | Repro |
|---|---|---|---|---|
| 1 | A `match` arm block ending in an `if`/`else` statement assigns an undeclared unit temporary in the generated C | ledger A, ledger B, inventory A, inventory B | no, gcc fails | `match_unit_arm.bld` |
| 2 | `"".split(sep)` dereferences null; any file that ends in a newline reaches it | config A, csv A (crash 0xC0000005) | no | `split_empty.bld` |
| 3 | Indexing a `&Vec<T>` parameter returns the vector handle, not the element: a C type error when compared, silent garbage when printed | bsearch B | no | `index_borrowed_vec.bld` |
| 4 | A generic function over `Vec<T>` lowers to C that names `T` | largest B, largest A after repair | no, gcc fails | `generic_vec.bld` |
| 5 | Elements of a `Vec<char>` print as their integer code | caesar B, rle B, config B, csv B | no, wrong output | `vec_char_display.bld` |
| 6 | `format!` leaves `\n` and `\t` escapes as literal backslash text (`println!` handles them) | squares_file A | no, wrong output | `format_escapes.bld` |
| 7 | The set of locals known to hold a char is never cleared between functions, so an integer in a later function can print as a character | csv A after repair | no, wrong output | `char_leak.bld` |
| 8 | A generic function is not instantiated per call: a second call with another type is rejected (spec 2.4 says each call site is monomorphized) | largest A | yes, wrongly (BL0301) | `generic_twice.bld` |
| 9 | No `Box` and no documented way to write a recursive type | bst A, bst B | yes (BL0901) | n/a |
| 10 | `len(x)` as a free function | pascal B | yes (BL0901) | n/a |

Classes 1 to 4 are check-pass-but-fail (M1 bugs by the preregistered rule).
Classes 5 to 7 are worse for an agent: the program builds, exits 0 and prints
the wrong text, so neither the checker nor the build tells the agent anything.
Class 8 is the checker rejecting a valid program. Classes 9 and 10 are
correct rejections; 9 is a language gap the docs do not mention.

Grader validation also found two toolchain defects before any answer was
graded: `buildc FILE -o prog.exe` writes C source under the .exe name, and
`buildc run` reduces a crashed program's exit code to one byte
(0xC0000005 becomes 5).

## Decision (as preregistered)

check_pass_but_fail is above 0, so M1 is reopened for classes 1 to 4, and the
silent wrong-output classes 5 to 7 are treated the same way. Their fixes land
before or with the next M2 PR, each with its repro as a regression test, and
each as an issue in the buildlang repository. Nothing in this folder's frozen
files changes.

## What this does not show

n=27 per sample, one attempt per task, one model family that also wrote the
tasks. Small programs only. The two samples share tasks, so they are not
pooled. No claim about other models or larger programs.
