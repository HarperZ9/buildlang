#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""qdb through bdb/lib against SQLite on the same machine (bdb/COMPLETE.md P19).

Builds the static library (bdb/lib/make_lib.py, `buildc build --release
--emit staticlib`), then throughput.c twice: once linked with libmain.a, once
compiled with the SQLite amalgamation (sqlite3.c, default options, -O2). Each run builds a table of N rows and times OPS
point selects, updates or both, inside one transaction (see throughput.c).
qdb's stdout goes to the null device. Reports the median of --reps runs per
engine and mode, and qdb's time as a multiple of SQLite's. --apis picks the
statement API: text (P19, SQL text per statement) and prep (P25, statements
prepared once, the key bound, rows read through the row API), both engines
alike.

  python bench_qdb_throughput.py BUILDC --sqlite-src DIR [--n 10000]
         [--ops 2000] [--reps 3] [--modes select,update,mixed]
         [--apis text,prep] [--json OUT] [--aa]

The engines run in turn, one pair per repetition. Besides the ratio of the
medians, each line gives the median of the per-pair ratios with a
distribution-free interval (order statistics, exact binomial coverage of at
least 90 percent when there are enough pairs). --aa puts SQLite in qdb's
place: the planted check that the interval brackets 1 for equal engines.

DIR holds sqlite3.c from https://www.sqlite.org (public domain).
"""
import argparse
import json
import os
import statistics
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))


def build(buildc, cc, sqlite_src, wd):
    proj = os.path.join(wd, "lib")
    subprocess.run([sys.executable, os.path.join(HERE, "..", "lib", "make_lib.py"), proj], check=True,
                   capture_output=True)
    env = dict(os.environ, BUILDLANG_CC=cc)
    p = subprocess.run([buildc, "build", "--release", "--emit", "staticlib", proj], capture_output=True, env=env)
    if p.returncode != 0:
        raise SystemExit("staticlib build failed:\n" + p.stdout.decode(errors="replace") + p.stderr.decode(errors="replace"))
    lib = None
    for root, _dirs, files in os.walk(os.path.join(proj, "target")):
        if "libmain.a" in files:
            lib = root
    if lib is None:
        raise SystemExit("libmain.a not found under %s" % proj)
    src = os.path.join(HERE, "throughput.c")
    ext = ".exe" if os.name == "nt" else ""
    flags = ["-lws2_32"] if os.name == "nt" else []
    qexe = os.path.join(wd, "tp_qdb" + ext)
    sexe = os.path.join(wd, "tp_sqlite" + ext)
    for cmd in ([cc, "-O2", "-w", "-DUSE_QDB", "-I", lib, src, os.path.join(lib, "libmain.a")] + flags + ["-lm", "-o", qexe],
                [cc, "-O2", "-w", src, os.path.join(sqlite_src, "sqlite3.c"), "-lm"]
                + ([] if os.name == "nt" else ["-lpthread", "-ldl"]) + ["-o", sexe]):
        p = subprocess.run(cmd, capture_output=True)
        if p.returncode != 0:
            raise SystemExit("compile failed: %s\n%s" % (" ".join(cmd), p.stderr.decode(errors="replace")))
    return qexe, sexe


def run(exe, wd, n, ops, mode, tag, api="text"):
    db = os.path.join(wd, "%s_%s.db" % (tag, mode))
    for s in ("", ".wal", ".tmp", ".hist", "-journal"):
        if os.path.exists(db + s):
            os.remove(db + s)
    p = subprocess.run([exe, db, str(n), str(ops), mode, api], stdout=subprocess.DEVNULL,
                       stderr=subprocess.PIPE, timeout=3600)
    line = p.stderr.decode(errors="replace").strip().split("\n")[-1]
    parts = line.split()
    if p.returncode != 0 or len(parts) != 6:
        raise SystemExit("%s %s failed (exit %d): %s" % (tag, mode, p.returncode, line))
    return float(parts[4])


def end_sum(exe, wd, tag, api="text"):
    """SUM(v) after a short mixed run, printed as the last line of stdout."""
    db = os.path.join(wd, "check_%s_%s.db" % (tag, api))
    p = subprocess.run([exe, db, "1000", "200", "mixed", api], capture_output=True, timeout=600)
    out = p.stdout.decode(errors="replace").replace("\r", "")
    lines = [l.strip() for l in out.split("\n") if l.strip()]
    return lines[-1] if lines else None


def median_interval(xs):
    """Distribution-free interval for the median of sorted xs: order statistics k and n-1-k,
    with k the largest index whose exact binomial coverage stays at or above 90 percent."""
    n = len(xs)
    if n < 3:
        return xs[0], xs[-1], 0.0
    from math import comb
    def cover(k):
        return 1.0 - 2.0 * sum(comb(n, i) for i in range(k + 1)) / 2.0 ** n
    k = 0
    while k + 1 < n // 2 and cover(k + 1) >= 0.90:
        k += 1
    return xs[k], xs[n - 1 - k], cover(k)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("buildc")
    ap.add_argument("--sqlite-src", required=True)
    ap.add_argument("--cc", default="gcc")
    ap.add_argument("--n", type=int, default=10000)
    ap.add_argument("--ops", type=int, default=2000)
    ap.add_argument("--reps", type=int, default=3)
    ap.add_argument("--modes", default="select,update,mixed")
    ap.add_argument("--apis", default="text")
    ap.add_argument("--json")
    ap.add_argument("--aa", action="store_true",
                    help="planted check: run SQLite in qdb's place, so every ratio should be 1")
    a = ap.parse_args()
    out = {}
    with tempfile.TemporaryDirectory() as wd:
        qexe, sexe = build(a.buildc, a.cc, a.sqlite_src, wd)
        if a.aa:
            qexe = sexe
        out["end_state_equal"] = True
        for api in a.apis.split(","):
            sq, ss = end_sum(qexe, wd, "qdb", api), end_sum(sexe, wd, "sqlite", api)
            print("end state check, api %s (SUM(v) after 1,000 rows and 200 mixed statements): qdb %s, sqlite %s: %s"
                  % (api, sq, ss, "equal" if sq == ss and sq else "DIFFERENT"))
            out["end_state_equal"] = out["end_state_equal"] and sq == ss and bool(sq)
        for api in a.apis.split(","):
            for mode in a.modes.split(","):
                # Alternate the engines so a slow spell on the machine falls on both.
                qs, ss = [], []
                for _ in range(a.reps):
                    qs.append(run(qexe, wd, a.n, a.ops, mode, "qdb", api))
                    ss.append(run(sexe, wd, a.n, a.ops, mode, "sqlite", api))
                tq, ts = statistics.median(qs), statistics.median(ss)
                ratios = sorted(q / s for q, s in zip(qs, ss))
                lo, hi, cover = median_interval(ratios)
                key = mode if api == "text" else api + "_" + mode
                out[key] = {"qdb_s": tq, "sqlite_s": ts, "qdb_ops_per_s": a.ops / tq,
                            "sqlite_ops_per_s": a.ops / ts, "ratio": tq / ts,
                            "pair_ratio_median": statistics.median(ratios), "pair_ratio_lo": lo,
                            "pair_ratio_hi": hi, "pair_ratio_coverage": cover,
                            "qdb_runs_s": qs, "sqlite_runs_s": ss}
                print("%-4s %-7s N=%d ops=%d  qdb %10.1f ops/s  sqlite %10.1f ops/s  qdb takes %.2fx SQLite's time"
                      "  (paired median %.2f, %.0f%% interval %.2f to %.2f, %d pairs)"
                      % (api, mode, a.n, a.ops, a.ops / tq, a.ops / ts, tq / ts,
                         statistics.median(ratios), 100 * cover, lo, hi, len(ratios)))
    if a.json:
        with open(a.json, "w") as f:
            json.dump(out, f, indent=1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
