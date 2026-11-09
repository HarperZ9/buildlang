#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""qdb as a C library: one open database, many statements, same output as the CLI.

Builds the library (bdb/lib/make_lib.py, then `buildc build --emit staticlib`),
links bdb/lib/driver.c against it, and runs the differential test's seeded
workloads two ways:

  library  every statement through qdb_exec on one handle, in one process
  cli      every statement as its own `qdb DB "<sql>"` process

The stdout of both (each statement framed as "> <sql>") and the final .db
must be byte-identical.

Planted check: the library run is repeated with one statement left out, and
the comparison must report a difference.

  python test_qdb_lib.py BUILDC QDB [--cc gcc] [--seeds 1,2,3] [--ops 120]
"""
import argparse
import os
import shutil
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from test_qdb_differential import gen_workload, run_process, read_db, clean  # noqa: E402


def build(buildc, cc, wd):
    proj = os.path.join(wd, "qdblib")
    subprocess.run([sys.executable, os.path.join(HERE, "..", "lib", "make_lib.py"), proj], check=True,
                   capture_output=True)
    env = dict(os.environ, BUILDLANG_CC=cc)
    p = subprocess.run([buildc, "build", "--emit", "staticlib", proj], capture_output=True, env=env)
    if p.returncode != 0:
        raise SystemExit("staticlib build failed:\n" + p.stdout.decode(errors="replace") + p.stderr.decode(errors="replace"))
    lib = os.path.join(proj, "target", "debug")
    flags = [l.strip() for l in open(os.path.join(lib, "main.link")).read().split("\n") if l.strip()]
    exe = os.path.join(wd, "driver.exe")
    p = subprocess.run([cc, "-O2", "-w", "-I", lib, os.path.join(HERE, "..", "lib", "driver.c"),
                        os.path.join(lib, "libmain.a")] + flags + ["-lm", "-o", exe], capture_output=True)
    if p.returncode != 0:
        raise SystemExit("driver link failed:\n" + p.stderr.decode(errors="replace"))
    return exe


def run_lib(driver, db, stmts, wd):
    clean(db)
    path = os.path.join(wd, "stmts.txt")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(stmts) + "\n")
    out = subprocess.run([driver, db, path], capture_output=True, timeout=900).stdout
    return out.decode(errors="replace").replace("\r", ""), read_db(db)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("buildc")
    ap.add_argument("qdb")
    ap.add_argument("--cc", default="gcc")
    ap.add_argument("--seeds", default="1,2,3")
    ap.add_argument("--ops", type=int, default=120)
    a = ap.parse_args()
    buildc, qdb = os.path.abspath(a.buildc), os.path.abspath(a.qdb)
    if shutil.which(a.cc) is None:
        print("SKIP: no %s on PATH" % a.cc)
        return 0
    failures = 0
    planted = None
    with tempfile.TemporaryDirectory() as wd:
        driver = build(buildc, a.cc, wd)
        for seed in [int(s) for s in a.seeds.split(",")]:
            stmts = gen_workload(seed, a.ops)
            t0 = time.perf_counter()
            l_out, l_db = run_lib(driver, os.path.join(wd, "lib.db"), stmts, wd)
            t1 = time.perf_counter()
            c_out, c_db = run_process(qdb, os.path.join(wd, "cli.db"), stmts)
            t2 = time.perf_counter()
            same = l_out == c_out and l_db == c_db
            print("%s seed=%d: %d statements in one process, %d output lines, %d-byte .db; "
                  "library %.2f s, one process per statement %.2f s"
                  % ("PASS" if same else "FAIL", seed, len(stmts), l_out.count("\n"), len(l_db), t1 - t0, t2 - t1))
            if not same:
                failures += 1
            if planted is None:
                p_out, p_db = run_lib(driver, os.path.join(wd, "lib.db"), stmts[:5] + stmts[6:], wd)
                planted = (p_out, p_db) != (c_out, c_db)
    print("planted check (one statement left out is a difference): %s" % ("caught" if planted else "MISSED"))
    print("LIBRARY: %d failures" % failures)
    if not planted:
        return 2
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
