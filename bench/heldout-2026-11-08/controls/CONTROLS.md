# Grader controls (bench5)

Written before the grader is first run on 1.11.0. Each control answer has a predicted outcome;
the grader must give exactly these, or it is fixed before any generator output is graded.

| Control | Predicted outcome | Why |
|---|---|---|
| fizzsum | pass | correct program |
| histogram | wrong | prints two lines of the four, one with a wrong count |
| binary | check_fail | assigns a string to an `i32` |
| fib | crash | unbounded-depth recursion (stack overflow) |
| score_check | pass | correct program that exits with status 3 |
| upper_file | pass | correct program that writes the output file |
| (all others) | missing | no answer file |

Result, 2026-11-08, crates.io 1.11.0: 6 of 6 controls as predicted (fizzsum, score_check and upper_file pass; histogram wrong; binary check_fail; fib crash); the other 28 tasks missing.
