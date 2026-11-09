#!/usr/bin/env python3
"""ORDER BY cost: time a sort of N rows, net of loading the same file.

For each N, builds a table t (id INTEGER, v INTEGER) of N rows with v drawn
from a seeded generator (many ties), then times, three times each:

  sort   qdb DB "SELECT id, v FROM t ORDER BY v"
  base   qdb DB "SELECT COUNT(*) FROM t"

and reports the median sort time minus the median base time, the ratio
between successive sizes, and the log-log slope. n log n gives a slope near
1.1 over these sizes; selection sort gives 2.

  python bench_qdb_sort.py QDB [--sizes 1000,10000,30000]
"""
import argparse
import math
import os
import random
import statistics
import subprocess
import sys
import tempfile
import time


def build(qdb, db, n):
    for suf in ("", ".wal", ".ckpt"):
        if os.path.exists(db + suf):
            os.remove(db + suf)
    rnd = random.Random(n)
    lines = ["CREATE TABLE t (id INTEGER, v INTEGER)", "BEGIN"]
    for start in range(0, n, 500):
        rows = ", ".join("(%d, %d)" % (i, rnd.randint(0, n // 4)) for i in range(start, min(n, start + 500)))
        lines.append("INSERT INTO t VALUES " + rows)
    lines += ["COMMIT", ".quit"]
    subprocess.run([qdb, db], input=("\n".join(lines) + "\n").encode(), capture_output=True, timeout=3600)


def timed(qdb, db, sql):
    t0 = time.perf_counter()
    p = subprocess.run([qdb, db, sql], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=7200)
    if p.returncode != 0:
        raise SystemExit("query failed: %s" % sql)
    return time.perf_counter() - t0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("qdb")
    ap.add_argument("--sizes", default="1000,10000,30000")
    ap.add_argument("--reps", type=int, default=3)
    a = ap.parse_args()
    qdb = os.path.abspath(a.qdb)
    sizes = [int(x) for x in a.sizes.split(",")]
    res = []
    with tempfile.TemporaryDirectory() as wd:
        for n in sizes:
            db = os.path.join(wd, "sort%d.db" % n)
            build(qdb, db, n)
            sort = statistics.median(timed(qdb, db, "SELECT id, v FROM t ORDER BY v") for _ in range(a.reps))
            base = statistics.median(timed(qdb, db, "SELECT COUNT(*) FROM t") for _ in range(a.reps))
            net = max(sort - base, 1e-6)
            res.append((n, sort, base, net))
            print("N=%6d  sort %.3f s  base %.3f s  net %.3f s" % (n, sort, base, net))
            sys.stdout.flush()
    for (n0, _, _, t0), (n1, _, _, t1) in zip(res, res[1:]):
        print("ratio N=%d/%d: %.2f  (slope %.2f)" % (n1, n0, t1 / t0, math.log(t1 / t0) / math.log(n1 / n0)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
