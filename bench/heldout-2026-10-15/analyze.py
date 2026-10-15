"""Analyse bench4 grades. Usage: python analyze.py <grades.jsonl> [<grades.jsonl> ...]

For each file: counts per outcome, pass@1 with a Wilson 95% interval,
check_pass_but_fail (build_fail + crash) and the stricter count of every
answer that passed `buildc check` and did not pass. Pass bar (preregistered):
pass@1 >= 22 of 27 and check_pass_but_fail == 0.
"""
import json
import math
import sys
from collections import Counter

Z = 1.959963984540054
N_TASKS = 27
PASS_BAR = 22


def wilson(k, n):
    if n == 0:
        return (0.0, 0.0)
    p = k / n
    d = 1 + Z * Z / n
    c = (p + Z * Z / (2 * n)) / d
    h = Z * math.sqrt(p * (1 - p) / n + Z * Z / (4 * n * n)) / d
    return (max(0.0, c - h), min(1.0, c + h))


def summarize(path):
    rows = [json.loads(l) for l in open(path, encoding="utf-8") if l.strip()]
    assert len(rows) == N_TASKS, (path, len(rows))
    c = Counter(r["outcome"] for r in rows)
    k, n = c["pass"], len(rows)
    lo, hi = wilson(k, n)
    cpbf = c["build_fail"] + c["crash"]
    strict = cpbf + c["wrong"]
    return {
        "file": path,
        "outcomes": dict(c),
        "pass": k,
        "n": n,
        "pass_rate": round(k / n, 4),
        "wilson95": [round(lo, 4), round(hi, 4)],
        "check_pass_but_fail": cpbf,
        "check_pass_not_pass": strict,
        "meets_bar": k >= PASS_BAR and cpbf == 0,
        "failed_tasks": {r["task"]: r["outcome"] for r in rows if r["outcome"] != "pass"},
    }


if __name__ == "__main__":
    # self-test of the interval against a known value: 22/27 -> [0.6330, 0.9182] (checked by hand)
    lo, hi = wilson(22, 27)
    assert abs(lo - 0.6330) < 1e-3 and abs(hi - 0.9182) < 1e-3, (lo, hi)
    for p in sys.argv[1:]:
        print(json.dumps(summarize(p), indent=1))
