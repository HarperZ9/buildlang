# Harness diff against bench/heldout-2026-10-15

The grader and analysis are copied; these are the only changes.

```diff
--- heldout-2026-10-15/grade.py
+++ grade.py
@@ -1,6 +1,6 @@
-"""Grade bench4. Usage: python grade.py <gen_dir> <out_jsonl>
+"""Grade bench5 (M3 probe, 2026-11-08). Usage: python grade.py <gen_dir> <out_jsonl>
 
-Environment: BENCH_BUILDC is the buildc binary under test (released 1.6.0).
+Environment: BENCH_BUILDC is the buildc binary under test (released 1.11.0).
 BUILDLANG_STDLIB is left unset so the stdlib embedded in buildc is used.
 
 Per task, in a fresh temporary directory holding the fixtures:
--- heldout-2026-10-15/analyze.py
+++ analyze.py
@@ -1,9 +1,9 @@
-"""Analyse bench4 grades. Usage: python analyze.py <grades.jsonl> [<grades.jsonl> ...]
+"""Analyse bench5 grades (M3 probe, 2026-11-08). Usage: python analyze.py <grades.jsonl> [<grades.jsonl> ...]
 
 For each file: counts per outcome, pass@1 with a Wilson 95% interval,
 check_pass_but_fail (build_fail + crash) and the stricter count of every
 answer that passed `buildc check` and did not pass. Pass bar (preregistered):
-pass@1 >= 22 of 27 and check_pass_but_fail == 0.
+pass@1 >= 29 of 34 and check_pass_but_fail == 0.
 """
 import json
 import math
@@ -11,8 +11,8 @@
 from collections import Counter
 
 Z = 1.959963984540054
-N_TASKS = 27
-PASS_BAR = 22
+N_TASKS = 34
+PASS_BAR = 29
 
 
 def wilson(k, n):
```
