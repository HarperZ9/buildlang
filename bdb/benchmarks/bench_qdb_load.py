#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Measure how qdb's write cost scales with the number of rows loaded.

Each scenario pipes one SQL script through a single qdb REPL process, so the
time is the engine's own write path (parse, insert, index upkeep, WAL, save)
plus one process start. The scaling exponent k in t ~ N^k is the least-squares
slope of log(t) against log(N) over the sizes that ran.

Scenarios:
  autocommit   N single-row INSERTs, each its own implicit transaction
  txn          BEGIN, N single-row INSERTs, COMMIT
  txn_indexed  as txn, with two indexes created first and one indexed
               SELECT after COMMIT, so deferred index work is inside the timing
  interleaved  autocommit INSERT, each followed by an indexed SELECT: the
               worst case for index upkeep, every read sees one new row
  txn_keyed    as txn, into (id INTEGER PRIMARY KEY, name TEXT UNIQUE, age
               INTEGER), so every row is checked against both keys (P17)
  txn_keyed_update  as txn_keyed, then one UPDATE of the UNIQUE column

Usage:
  python bench_qdb_load.py QDB_EXE [--sizes 1000,2000,4000] [--budget 120]
                           [--scenarios autocommit,txn,txn_indexed] [--json OUT]

A size is skipped once the previous size of the same scenario took longer than
the budget in seconds, so the run stays bounded on the slow engine.
"""
import argparse
import json
import math
import os
import subprocess
import sys
import tempfile
import time


def script(scenario, n):
    if scenario.startswith("txn_keyed"):
        lines = ["CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT UNIQUE, age INTEGER)"]
    else:
        lines = ["CREATE TABLE t (id INTEGER, name TEXT, age INTEGER)"]
    if scenario in ("txn_indexed", "interleaved"):
        lines.append("CREATE INDEX t_age ON t (age)")
        lines.append("CREATE INDEX t_name ON t (name)")
    if scenario in ("txn", "txn_indexed", "txn_keyed", "txn_keyed_update"):
        lines.append("BEGIN")
    # A fixed permutation of ids so index inserts land in the middle, not
    # only at the end (the sorted-append case would flatter an index).
    for i in range(n):
        k = (i * 7919) % n
        lines.append("INSERT INTO t VALUES (%d, 'u%d', %d)" % (k, k, 20 + k % 50))
        if scenario == "interleaved":
            lines.append("SELECT id FROM t WHERE age = %d" % (20 + k % 50))
    if scenario in ("txn", "txn_indexed", "txn_keyed", "txn_keyed_update"):
        lines.append("COMMIT")
    if scenario == "txn_keyed_update":
        lines.append("UPDATE t SET name = 'renamed' WHERE id = 0")
    if scenario == "txn_indexed":
        lines.append("SELECT id FROM t WHERE age = 33")
    lines.append(".quit")
    return "\n".join(lines) + "\n"


def run_once(qdb, scenario, n, workdir):
    db = os.path.join(workdir, "bench_%s_%d.db" % (scenario, n))
    for p in (db, db + ".wal"):
        if os.path.exists(p):
            os.remove(p)
    sql = script(scenario, n)
    t0 = time.perf_counter()
    subprocess.run([qdb, db], input=sql.encode(), stdout=subprocess.DEVNULL,
                   stderr=subprocess.DEVNULL, check=True)
    dt = time.perf_counter() - t0
    out = subprocess.run([qdb, db, "SELECT COUNT(*) FROM t"], capture_output=True,
                         check=True).stdout.decode()
    count = int(out.replace("\r", "").strip().split("\n")[-1].strip())
    if count != n:
        raise SystemExit("%s N=%d: expected %d rows, found %d" % (scenario, n, n, count))
    for p in (db, db + ".wal"):
        if os.path.exists(p):
            os.remove(p)
    return dt


def slope(points):
    xs = [math.log(n) for n, _ in points]
    ys = [math.log(t) for _, t in points]
    mx, my = sum(xs) / len(xs), sum(ys) / len(ys)
    sxx = sum((x - mx) ** 2 for x in xs)
    return sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / sxx if sxx else float("nan")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("qdb")
    ap.add_argument("--sizes", default="1000,2000,4000,8000")
    ap.add_argument("--scenarios", default="autocommit,txn,txn_indexed")
    ap.add_argument("--budget", type=float, default=120.0)
    ap.add_argument("--json")
    a = ap.parse_args()
    sizes = [int(s) for s in a.sizes.split(",")]
    result = {}
    with tempfile.TemporaryDirectory() as wd:
        for sc in a.scenarios.split(","):
            pts = []
            for n in sizes:
                if pts and pts[-1][1] > a.budget:
                    print("%-12s N=%7d  skipped (previous size over %.0fs budget)" % (sc, n, a.budget))
                    continue
                dt = run_once(a.qdb, sc, n, wd)
                pts.append((n, dt))
                print("%-12s N=%7d  %9.3f s  %8.1f us/row" % (sc, n, dt, dt / n * 1e6), flush=True)
            k = slope(pts) if len(pts) > 1 else float("nan")
            print("%-12s exponent k = %.2f over N=%d..%d" % (sc, k, pts[0][0], pts[-1][0]))
            result[sc] = {"points": pts, "exponent": k}
    if a.json:
        with open(a.json, "w") as f:
            json.dump(result, f, indent=2)
    return 0


if __name__ == "__main__":
    sys.exit(main())
