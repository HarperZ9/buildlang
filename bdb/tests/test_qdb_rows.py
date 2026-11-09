#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Prepared statements and rows through bdb/lib (bdb/COMPLETE.md P25).

Builds the library and bdb/lib/rows_driver.c, then runs each workload twice
through qdb_prepare / qdb_step:

  literal  every statement as text with its values written in (no `?`, so
           every statement runs as text through exec_sql, rows captured)
  bound    the same statements with each integer literal of a SELECT or an
           UPDATE turned into a `?` bound with qdb_bind_int64, so point selects
           and updates take the planned paths

The two outputs (rows and column names) and the two final .db and .wal files
must be byte-identical. The literal run's rows must also equal the CLI's
output for the same statements, read cell by cell.

Workloads: the differential test's seeded workloads, and a keyed workload
(an indexed INTEGER key holding integers, reals, text and NULL; updates by
+, -, * and =, inside and outside transactions; an index added on the SET
column halfway, which re-plans the prepared statements).

Planted check: a library whose numeric index search also accepts the next
larger key must make the comparison fail.

  python test_qdb_rows.py BUILDC QDB [--cc gcc] [--seeds 1,2,3] [--ops 150]
"""
import argparse
import os
import random
import re
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from test_qdb_differential import gen_workload, clean  # noqa: E402

PLANT_FROM = "if vec_get(nums, j) == x {\n            vset_i32(out, n,"
PLANT_TO = "if vec_get(nums, j) <= x + 1.0 {\n            vset_i32(out, n,"


def build(buildc, cc, wd, name, plant=False):
    proj = os.path.join(wd, name)
    subprocess.run([sys.executable, os.path.join(HERE, "..", "lib", "make_lib.py"), proj], check=True,
                   capture_output=True)
    if plant:
        src = os.path.join(proj, "main.bld")
        text = open(src, encoding="utf-8").read()
        if text.count(PLANT_FROM) != 1:
            raise SystemExit("planted check: anchor not found once in the library source")
        open(src, "w", encoding="utf-8", newline="\n").write(text.replace(PLANT_FROM, PLANT_TO))
    env = dict(os.environ, BUILDLANG_CC=cc)
    p = subprocess.run([buildc, "build", "--emit", "staticlib", proj], capture_output=True, env=env)
    if p.returncode != 0:
        raise SystemExit("staticlib build failed:\n" + p.stdout.decode(errors="replace") + p.stderr.decode(errors="replace"))
    lib = os.path.join(proj, "target", "debug")
    flags = [l.strip() for l in open(os.path.join(lib, "main.link")).read().split("\n") if l.strip()]
    exe = os.path.join(wd, name + "_rows.exe")
    p = subprocess.run([cc, "-O2", "-w", "-I", lib, os.path.join(HERE, "..", "lib", "rows_driver.c"),
                        os.path.join(lib, "libmain.a")] + flags + ["-lm", "-o", exe], capture_output=True)
    if p.returncode != 0:
        raise SystemExit("rows_driver link failed:\n" + p.stderr.decode(errors="replace"))
    return exe


INT_LIT = re.compile(r"(?<![\w.'])-?\d+(?![\w.])")


def parametrize(stmt):
    """The statement with its integer literals outside quotes as `?`, and the
    parameter list; unchanged with no parameters for other statements."""
    up = stmt.upper()
    if not (up.startswith("SELECT ") or up.startswith("UPDATE ")):
        return stmt, []
    pieces = re.split(r"('(?:[^']|'')*')", stmt)
    out, params = [], []
    for i, piece in enumerate(pieces):
        if i % 2 == 1:
            out.append(piece)
            continue

        def sub(m):
            params.append("i:" + m.group(0))
            return "?"
        out.append(INT_LIT.sub(sub, piece))
    return "".join(out), params


def keyed_workload(seed, nops):
    rnd = random.Random(seed)
    stmts = ["CREATE TABLE t (id INTEGER, v INTEGER, w TEXT)", "CREATE INDEX t_id ON t (id)"]
    keys = ["1", "2", "3", "4", "5", "007", "-3", "2.0", "2.5", "'abc'", "NULL", "'5'", "40"]
    for i in range(30):
        v = rnd.choice(["%d" % rnd.randint(-5, 50), "1.5", "'x'", "NULL"]) if i % 5 == 0 else "%d" % rnd.randint(-5, 50)
        stmts.append("INSERT INTO t VALUES (%s, %s, 'r%d')" % (rnd.choice(keys), v, i))
    in_txn = False
    for j in range(nops):
        if j == nops // 2:
            stmts.append("CREATE INDEX t_v ON t (v)")
        r = rnd.random()
        k = rnd.choice([0, 1, 2, 3, 4, 5, 7, -3, 40, 99])
        if r < 0.30:
            stmts.append("SELECT v, w FROM t WHERE id = %d" % k)
        elif r < 0.38:
            stmts.append("SELECT * FROM t WHERE id = %d" % k)
        elif r < 0.60:
            stmts.append("UPDATE t SET v = v %s %d WHERE id = %d" % (rnd.choice("+-*"), rnd.randint(0, 4), k))
        elif r < 0.68:
            nv = rnd.randint(-9, 99)
            stmts.append("UPDATE t SET v = %d WHERE id = %d" % (nv, k))
            if j >= nops // 2:
                # Search the moved key at once (P27): the index on v must
                # find it at its new place, with no rebuild in between.
                stmts.append("SELECT id, v FROM t WHERE v = %d" % nv)
        elif r < 0.74:
            stmts.append("SELECT id, v FROM t WHERE v = %d" % rnd.randint(-5, 60))
        elif r < 0.80:
            stmts.append("INSERT INTO t VALUES (%d, %d, 'n%d')" % (k, rnd.randint(0, 9), j))
        elif r < 0.84 and not in_txn:
            stmts.append("BEGIN")
            in_txn = True
        elif r < 0.90 and in_txn:
            stmts.append(rnd.choice(["COMMIT", "ROLLBACK"]))
            in_txn = False
        else:
            stmts.append("SELECT COUNT(*), SUM(v) FROM t")
    if in_txn:
        stmts.append("COMMIT")
    stmts.append("SELECT id, v, w FROM t ORDER BY w")
    return stmts


def run_driver(exe, db, lines, wd):
    clean(db)
    path = os.path.join(wd, "rows_stmts.txt")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
    p = subprocess.run([exe, db, path], capture_output=True, timeout=900)
    files = []
    for s in ("", ".wal"):
        files.append(open(db + s, "rb").read() if os.path.exists(db + s) else b"")
    return p.stdout.decode(errors="replace").replace("\r", ""), files


def blocks(out):
    """Output split per statement, without the "> " lines (the two runs frame
    their statements differently)."""
    res, cur = [], None
    for line in out.split("\n"):
        if line.startswith("> "):
            if cur is not None:
                res.append(cur)
            cur = []
        elif cur is not None and line:
            cur.append(line)
    if cur is not None:
        res.append(cur)
    return res


def cli_rows(qdb, db, stmts):
    """The CLI's rows for each statement, cells split on " | " and trimmed; for a
    statement without a result table, the lines it printed (error messages)."""
    clean(db)
    res = []
    for s in stmts:
        out = subprocess.run([qdb, db, s], capture_output=True, timeout=120).stdout.decode(errors="replace")
        lines = [l for l in out.replace("\r", "").split("\n") if l]
        if len(lines) >= 2 and set(lines[1]) <= set("-+"):
            rows = ["\t".join("\\N" if c.strip() == "NULL" else c.strip() for c in l.split(" | ")) for l in lines[2:]]
            res.append(rows)
        else:
            res.append(lines)  # no result table: messages, compared as printed
    return res


def compare(exe, wd, stmts):
    lit_out, lit_files = run_driver(exe, os.path.join(wd, "lit.db"), stmts, wd)
    bound = []
    for s in stmts:
        q, params = parametrize(s)
        bound.append("\t".join([q] + params))
    nparams = sum(len(parametrize(s)[1]) for s in stmts)
    b_out, b_files = run_driver(exe, os.path.join(wd, "bound.db"), bound, wd)
    lb, bb = blocks(lit_out), blocks(b_out)
    diff = sum(1 for x, y in zip(lb, bb) if x != y) + abs(len(lb) - len(bb))
    files_same = lit_files == b_files
    return diff, files_same, nparams, lb


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("buildc")
    ap.add_argument("qdb")
    ap.add_argument("--cc", default="gcc")
    ap.add_argument("--seeds", default="1,2,3")
    ap.add_argument("--ops", type=int, default=150)
    a = ap.parse_args()
    buildc, qdb = os.path.abspath(a.buildc), os.path.abspath(a.qdb)
    if shutil.which(a.cc) is None:
        print("SKIP: no %s on PATH" % a.cc)
        return 0
    failures = 0
    with tempfile.TemporaryDirectory() as wd:
        exe = build(buildc, a.cc, wd, "qdbrows")
        plant = build(buildc, a.cc, wd, "qdbplant", plant=True)
        plant_diffs = 0
        for seed in [int(s) for s in a.seeds.split(",")]:
            for kind, stmts in (("differential", gen_workload(seed, a.ops)), ("keyed", keyed_workload(seed, a.ops))):
                diff, same, npar, lit_blocks = compare(exe, wd, stmts)
                cli = cli_rows(qdb, os.path.join(wd, "cli.db"), stmts)
                lit_rows = [[l for l in b if not l.startswith("# ")] for b in lit_blocks]
                cli_diff = sum(1 for x, y in zip(lit_rows, cli) if x != y) + abs(len(lit_rows) - len(cli))
                ok = diff == 0 and same and cli_diff == 0
                failures += 0 if ok else 1
                print("seed %d %-12s %4d statements, %4d bound parameters: bound vs literal %d differ, "
                      ".db/.wal %s, literal vs CLI %d differ  %s"
                      % (seed, kind, len(stmts), npar, diff, "same" if same else "DIFFERENT", cli_diff,
                         "ok" if ok else "FAIL"))
                pdiff, psame, _n, _b = compare(plant, wd, stmts)
                plant_diffs += pdiff + (0 if psame else 1)
        caught = plant_diffs > 0
        print("planted (index search accepts the next key): %d differences: %s"
              % (plant_diffs, "caught" if caught else "MISSED"))
        if not caught:
            failures += 1
    print("ROWS %s" % ("PASS" if failures == 0 else "FAIL (%d)" % failures))
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
