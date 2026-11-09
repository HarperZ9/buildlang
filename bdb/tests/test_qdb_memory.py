#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Memory growth of qdb per row and per statement (bdb/COMPLETE.md P30).

    test_qdb_memory.py QDB [--planted QDB_PLANTED] [--json OUT]
    test_qdb_memory.py --write-planted DB_BLD OUT_BLD

Each workload runs twice, at a small and a large size, in a fresh
directory. The peak memory of the qdb process is read from the operating
system after it exits, and the slope is

    (peak(large) - peak(small)) / (large - small)     bytes per unit

  load     bytes per row: N single-row INSERTs in one transaction.
  select   bytes per statement: M point SELECTs on a 5,000-row table.
  update   bytes per statement: M UPDATEs of an existing row, 1,000 to a
           transaction, on a 5,000-row table. The table does not grow.

A slope above its bound fails the test. The bounds are in BOUNDS below and
are preregistered in bdb/COMPLETE.md.

Planted check: --write-planted writes a copy of db.bld in which the parser
makes a new list for every field at every statement (the state before P29).
Built and passed as --planted, that engine must exceed the select bound, or
the test fails with MISSED. A leak smaller than the bound is not caught.

Peak memory is the peak resident set (ru_maxrss) on POSIX and the peak
pagefile commit on Windows. Stdlib only.
"""
import argparse, json, os, subprocess, sys, tempfile

BOUNDS = {"load": 1500.0, "select": 1900.0, "update": 1600.0}
SIZES = {"load": (20000, 80000), "select": (20000, 80000), "update": (20000, 80000)}
TABLE_ROWS = 5000
PLANT_OLD = "    d.reset_fresh = false;"
PLANT_NEW = "    d.reset_fresh = true;"


def workload(kind, n):
    out = ["CREATE TABLE t (id INTEGER PRIMARY KEY, v INTEGER);", "BEGIN;"]
    rows = n if kind == "load" else TABLE_ROWS
    for i in range(1, rows + 1):
        out.append("INSERT INTO t VALUES (%d, %d);" % (i, (i * 7919) % 100003))
    out.append("COMMIT;")
    if kind == "select":
        for j in range(n):
            out.append("SELECT v FROM t WHERE id = %d;" % ((j * 7919) % TABLE_ROWS + 1))
    elif kind == "update":
        for j in range(n):
            if j % 1000 == 0:
                out.append("BEGIN;")
            out.append("UPDATE t SET v = %d WHERE id = %d;" % ((j * 104729) % 100003, (j * 7919) % TABLE_ROWS + 1))
            if j % 1000 == 999 or j == n - 1:
                out.append("COMMIT;")
    out.append(".quit")
    return ("\n".join(out) + "\n").encode()


def peak_bytes(qdb, sql):
    """Run qdb on sql in a fresh directory; return (peak bytes, exit code)."""
    with tempfile.TemporaryDirectory() as d:
        src = os.path.join(d, "in.sql")
        with open(src, "wb") as f:
            f.write(sql)
        with open(src, "rb") as fin:
            p = subprocess.Popen([qdb, os.path.join(d, "t.db")], stdin=fin,
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            if os.name == "nt":
                p.wait()
                import ctypes
                from ctypes import wintypes
                class PMC(ctypes.Structure):
                    _fields_ = [("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD)] + [
                        (n, ctypes.c_size_t) for n in (
                            "PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage",
                            "QuotaPagedPoolUsage", "QuotaPeakNonPagedPoolUsage",
                            "QuotaNonPagedPoolUsage", "PagefileUsage", "PeakPagefileUsage")]
                m = PMC()
                m.cb = ctypes.sizeof(PMC)
                ok = ctypes.windll.psapi.GetProcessMemoryInfo(
                    wintypes.HANDLE(int(p._handle)), ctypes.byref(m), m.cb)
                if not ok:
                    raise OSError("GetProcessMemoryInfo failed")
                return m.PeakPagefileUsage, p.returncode
            _, status, ru = os.wait4(p.pid, 0)
            p.returncode = os.waitstatus_to_exitcode(status)
            scale = 1 if sys.platform == "darwin" else 1024
            return ru.ru_maxrss * scale, p.returncode


def slopes(qdb, kinds):
    res = {}
    for kind in kinds:
        small, large = SIZES[kind]
        a, rca = peak_bytes(qdb, workload(kind, small))
        b, rcb = peak_bytes(qdb, workload(kind, large))
        if rca != 0 or rcb != 0:
            raise SystemExit("qdb exited %s and %s on the %s workload" % (rca, rcb, kind))
        res[kind] = {"small": small, "large": large, "peak_small": a, "peak_large": b,
                     "slope": (b - a) / float(large - small), "bound": BOUNDS[kind]}
    return res


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("qdb", nargs="?")
    ap.add_argument("--planted")
    ap.add_argument("--json")
    ap.add_argument("--write-planted", nargs=2, metavar=("DB_BLD", "OUT_BLD"))
    a = ap.parse_args()
    if a.write_planted:
        src = open(a.write_planted[0], encoding="utf-8", newline="").read()
        if src.count(PLANT_OLD) != 1:
            raise SystemExit("plant site not found exactly once in %s" % a.write_planted[0])
        with open(a.write_planted[1], "w", encoding="utf-8", newline="") as f:
            f.write(src.replace(PLANT_OLD, PLANT_NEW))
        print("planted source written: %s" % a.write_planted[1])
        return 0
    if not a.qdb:
        ap.error("QDB is required")
    res = slopes(os.path.abspath(a.qdb), ["load", "select", "update"])
    fail = 0
    for kind, r in res.items():
        over = r["slope"] > r["bound"]
        fail += over
        print("%-6s %8.1f bytes per %s (bound %.0f; peaks %d and %d bytes at %d and %d) %s" % (
            kind, r["slope"], "row" if kind == "load" else "statement", r["bound"],
            r["peak_small"], r["peak_large"], r["small"], r["large"], "OVER" if over else "ok"))
    out = {"engine": res}
    if a.planted:
        pr = slopes(os.path.abspath(a.planted), ["select"])["select"]
        caught = pr["slope"] > pr["bound"]
        out["planted"] = dict(pr, caught=caught)
        print("planted (a new list per parser field per statement): %.1f bytes per statement, %s" % (
            pr["slope"], "caught" if caught else "MISSED"))
        fail += not caught
    if a.json:
        with open(a.json, "w") as f:
            json.dump(out, f, indent=1)
    print("MEMORY %s" % ("FAIL" if fail else "PASS"))
    return 1 if fail else 0


if __name__ == "__main__":
    sys.exit(main())
