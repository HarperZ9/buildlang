#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""qdb through bdb/lib against SQLite on the same machine (bdb/COMPLETE.md P19).

Builds the static library (bdb/lib/make_lib.py, `buildc build --release
--emit staticlib`), then throughput.c twice: once linked with libmain.a, once
compiled with the SQLite amalgamation (sqlite3.c, default options, -O2). Each run builds a table of N rows and times OPS
point selects, updates or both, inside one transaction (see throughput.c).
qdb's stdout goes to the null device. Reports the median of --reps runs per
engine and mode, and qdb's time as a multiple of SQLite's.

  python bench_qdb_throughput.py BUILDC --sqlite-src DIR [--n 10000]
         [--ops 2000] [--reps 3] [--modes select,update,mixed] [--json OUT]

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


def run(exe, wd, n, ops, mode, tag):
    db = os.path.join(wd, "%s_%s.db" % (tag, mode))
    for s in ("", ".wal", ".tmp", ".hist", "-journal"):
        if os.path.exists(db + s):
            os.remove(db + s)
    p = subprocess.run([exe, db, str(n), str(ops), mode], stdout=subprocess.DEVNULL,
                       stderr=subprocess.PIPE, timeout=3600)
    line = p.stderr.decode(errors="replace").strip().split("\n")[-1]
    parts = line.split()
    if p.returncode != 0 or len(parts) != 6:
        raise SystemExit("%s %s failed (exit %d): %s" % (tag, mode, p.returncode, line))
    return float(parts[4])


def end_sum(exe, wd, tag):
    """SUM(v) after a short mixed run, printed as the last line of stdout."""
    db = os.path.join(wd, "check_%s.db" % tag)
    p = subprocess.run([exe, db, "1000", "200", "mixed"], capture_output=True, timeout=600)
    out = p.stdout.decode(errors="replace").replace("\r", "")
    lines = [l.strip() for l in out.split("\n") if l.strip()]
    return lines[-1] if lines else None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("buildc")
    ap.add_argument("--sqlite-src", required=True)
    ap.add_argument("--cc", default="gcc")
    ap.add_argument("--n", type=int, default=10000)
    ap.add_argument("--ops", type=int, default=2000)
    ap.add_argument("--reps", type=int, default=3)
    ap.add_argument("--modes", default="select,update,mixed")
    ap.add_argument("--json")
    a = ap.parse_args()
    out = {}
    with tempfile.TemporaryDirectory() as wd:
        qexe, sexe = build(a.buildc, a.cc, a.sqlite_src, wd)
        sq, ss = end_sum(qexe, wd, "qdb"), end_sum(sexe, wd, "sqlite")
        print("end state check (SUM(v) after 1,000 rows and 200 mixed statements): qdb %s, sqlite %s: %s"
              % (sq, ss, "equal" if sq == ss and sq else "DIFFERENT"))
        out["end_state_equal"] = sq == ss and bool(sq)
        for mode in a.modes.split(","):
            tq = statistics.median(run(qexe, wd, a.n, a.ops, mode, "qdb") for _ in range(a.reps))
            ts = statistics.median(run(sexe, wd, a.n, a.ops, mode, "sqlite") for _ in range(a.reps))
            out[mode] = {"qdb_s": tq, "sqlite_s": ts, "qdb_ops_per_s": a.ops / tq,
                         "sqlite_ops_per_s": a.ops / ts, "ratio": tq / ts}
            print("%-7s N=%d ops=%d  qdb %9.1f ops/s  sqlite %10.1f ops/s  qdb takes %.1fx SQLite's time"
                  % (mode, a.n, a.ops, a.ops / tq, a.ops / ts, tq / ts))
    if a.json:
        with open(a.json, "w") as f:
            json.dump(out, f, indent=1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
