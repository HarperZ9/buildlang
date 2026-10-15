# Grader controls (run before sealing)

Hand-written programs in `controls/gen/`, graded with `grade.py` against
buildc 1.6.0 from crates.io (binary SHA-256
69faaecc98e9a44631768e922806120f766e0a96203e8c690f15075d5a6e3a56).

| file | built to be | expected outcome | observed |
|---|---|---|---|
| leap.bld | correct | pass | pass |
| exit_status.bld | correct, exits 3 | pass | pass |
| collatz.bld | prints 0 0 | wrong | wrong |
| squares_file.bld | right stdout, short output file | wrong | wrong |
| bits.bld | `let s: str = 4` | check_fail | check_fail (BL0301) |
| rle.bld | `loop {}` after the print | crash (timeout) | crash (timeout) |
| bst.bld | unbounded recursion | crash | crash (rc 3221225725, stack overflow) |
| the other 20 | absent | missing | missing |

No build_fail control: no small program was found that passes the 1.6.0
check and fails in the C compiler, which is what M1 aimed for. The class is
detected by the exit status of `buildc build` and a missing executable.

Found while validating: `buildc FILE -o prog.exe` writes C source under an
.exe name, and `buildc run` reduces a crashed child's exit code to one byte
(0xC0000005 reads as 5). The grader therefore builds with `buildc build .`
and runs the executable itself. Both become issues after the probe.
