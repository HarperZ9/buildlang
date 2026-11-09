#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Randomized differential test: two qdb builds must agree on every output.

Generates a seeded random workload (two tables, three indexes, single and
multi-row INSERT into either table, UPDATE, DELETE, BEGIN/COMMIT,
BEGIN/ROLLBACK, indexed equality and range SELECTs, ORDER BY, aggregates,
GROUP BY) and runs it through both binaries two ways:

  repl     the whole script piped into one REPL session per binary
  process  each statement as its own `qdb DB "<sql>"` process

The repl way compares the full stdout and the final .db file byte for byte
between the two builds, after removing a format-1 header line ("QDB 1 <txid>")
from either .db: the header names the last transaction in the file, which
the reference engine does not write. The process way runs the new build
only, and compares its per-statement output and final .db with the same
build's repl run, byte for byte, header included. Since BEGIN in one process
can be finished by COMMIT or ROLLBACK in a later one (HOLD in the WAL), the
two ways must agree; the reference engine, which drops a transaction at
every process exit, is not compared there.

Typical use is the build before a change against the build after it:

  python test_qdb_differential.py OLD_QDB NEW_QDB [--seeds 1,2,3] [--ops 400]

Exits 0 when every seed agrees, 1 on the first disagreement (and prints the
seed, mode and first differing line so the case can be replayed).
"""
import argparse
import os
import random
import subprocess
import sys
import tempfile


def gen_workload(seed, nops, no_null=False, no_float_agg=False):
    rnd = random.Random(seed)
    names = ["ann", "bob", "cy", "dee", "ed", "flo", "", "gus"]
    if no_null:
        # The reference engine predates NULL apart from '' (bdb/COMPLETE.md P7):
        # it stored both as one empty cell. On data with neither, the two
        # engines must still agree byte for byte.
        names[names.index("")] = "hal"
    tags = ["x", "y", "z"]
    stmts = [
        "CREATE TABLE a (id INTEGER, name TEXT, score REAL)",
        "CREATE INDEX a_score ON a (score)",
        "CREATE TABLE b (id INTEGER, tag TEXT)",
        "CREATE INDEX a_name ON a (name)",
        "CREATE INDEX b_id ON b (id)",
    ]
    next_id = 1
    in_txn = False

    def score():
        r = rnd.random()
        if r < 0.05:
            return "0" if no_null else "NULL"
        if r < 0.35:
            return str(rnd.randint(-3, 9))          # many duplicates
        return "%.1f" % rnd.uniform(-5, 50)

    def a_row():
        nonlocal next_id
        i = next_id if rnd.random() < 0.8 else rnd.randint(1, max(1, next_id))
        next_id += 1
        return "(%d, '%s', %s)" % (i, rnd.choice(names), score())

    for _ in range(nops):
        r = rnd.random()
        if r < 0.30:
            stmts.append("INSERT INTO a VALUES " + a_row())
        elif r < 0.38:
            stmts.append("INSERT INTO a VALUES " + ", ".join(a_row() for _ in range(rnd.randint(2, 6))))
        elif r < 0.46:
            stmts.append("INSERT INTO b VALUES (%d, '%s')" % (rnd.randint(1, 40), rnd.choice(tags)))
        elif r < 0.50:
            stmts.append("UPDATE a SET score = %s WHERE id = %d" % (score(), rnd.randint(1, next_id)))
        elif r < 0.53:
            stmts.append("DELETE FROM a WHERE score < %d" % rnd.randint(-4, 3))
        elif r < 0.55:
            stmts.append("DELETE FROM b WHERE id = %d" % rnd.randint(1, 40))
        elif r < 0.60 and not in_txn:
            stmts.append("BEGIN")
            in_txn = True
        elif r < 0.64 and in_txn:
            stmts.append(rnd.choice(["COMMIT", "COMMIT", "ROLLBACK"]))
            in_txn = False
        elif r < 0.72:
            stmts.append("SELECT id, name, score FROM a WHERE score = %s" % rnd.choice([str(rnd.randint(-3, 9)), "%.1f" % rnd.uniform(-5, 50)]))
        elif r < 0.76:
            stmts.append("SELECT id, score FROM a WHERE name = '%s'" % rnd.choice(names))
        elif r < 0.80:
            op = rnd.choice([">", "<", ">=", "<=", "!="])
            stmts.append("SELECT id, score FROM a WHERE score %s %d" % (op, rnd.randint(-3, 30)))
        elif r < 0.83:
            stmts.append("SELECT id, tag FROM b WHERE id = %d" % rnd.randint(1, 40))
        elif r < 0.87:
            stmts.append("SELECT id, name, score FROM a ORDER BY score DESC, id LIMIT 7")
        elif r < 0.91:
            # SUM, MIN and MAX over no rows are NULL since P7 (0 and blank in the
            # reference engine), so the no-null workload asks only once a has rows.
            if not (no_null and next_id == 1):
                if no_float_agg:
                    stmts.append("SELECT COUNT(*), MIN(score), MAX(name) FROM a")
                else:
                    stmts.append("SELECT COUNT(*), SUM(score), MIN(score), MAX(name) FROM a")
        elif r < 0.95:
            # The reference engine divides AVG as integers and rounds SUM and AVG to
            # two decimals (P8); --no-float-agg asks MIN instead, and SUM and AVG are
            # checked against SQLite (test_qdb_sqlite_oracle.py).
            agg = "MIN(score)" if no_float_agg else "AVG(score)"
            stmts.append("SELECT name, COUNT(*), %s FROM a GROUP BY name ORDER BY name" % agg)
        else:
            stmts.append("SELECT tag, COUNT(*) FROM b GROUP BY tag ORDER BY tag")
    if in_txn:
        stmts.append("COMMIT")
    stmts += [
        "SELECT * FROM a ORDER BY id, score",
        "SELECT * FROM b ORDER BY id, tag",
        "SELECT id FROM a WHERE score = 5",
        "SELECT id FROM a WHERE score > 10",
    ]
    return stmts


def clean(path):
    for p in (path, path + ".wal"):
        if os.path.exists(p):
            os.remove(p)


def run_repl(qdb, db, stmts):
    clean(db)
    script = "\n".join(stmts) + "\n.quit\n"
    out = subprocess.run([qdb, db], input=script.encode(), capture_output=True, timeout=900).stdout
    return out.decode(errors="replace").replace("\r", ""), read_db(db)


def run_process(qdb, db, stmts):
    clean(db)
    chunks = []
    for s in stmts:
        out = subprocess.run([qdb, db, s], capture_output=True, timeout=120).stdout
        chunks.append("> " + s + "\n" + out.decode(errors="replace").replace("\r", ""))
    return "".join(chunks), read_db(db)


def read_db(db):
    if not os.path.exists(db):
        return b""
    with open(db, "rb") as f:
        return f.read()


def body(db_bytes):
    """The .db without its format header line (format 1 or 2)."""
    if db_bytes.startswith(b"QDB 1 ") or db_bytes.startswith(b"QDB 2 "):
        return db_bytes[db_bytes.index(b"\n") + 1:]
    return db_bytes


def repl_chunks(out, n):
    """Split a REPL transcript into per-statement outputs, as run_process prints them."""
    parts = out.split("qdb> ")[1:n + 1]
    return parts


def first_diff(a, b):
    la, lb = a.split("\n"), b.split("\n")
    for i in range(max(len(la), len(lb))):
        x = la[i] if i < len(la) else "<missing>"
        y = lb[i] if i < len(lb) else "<missing>"
        if x != y:
            return i + 1, x, y
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("old")
    ap.add_argument("new")
    ap.add_argument("--seeds", default="1,2,3,4,5")
    ap.add_argument("--ops", type=int, default=400)
    ap.add_argument("--process-ops", type=int, default=80)
    ap.add_argument("--no-null", action="store_true",
                    help="generate no NULL and no '' (for a reference engine older than P7)")
    ap.add_argument("--no-float-agg", action="store_true",
                    help="ask no SUM or AVG (for a reference engine older than P8)")
    a = ap.parse_args()
    for exe in (a.old, a.new):
        if not os.path.isfile(exe):
            print("SKIP: %s not found" % exe)
            return 0
    failures = 0
    checks = 0
    with tempfile.TemporaryDirectory() as wd:
        for seed in [int(s) for s in a.seeds.split(",")]:
            for mode, runner, nops in (("repl", run_repl, a.ops), ("process", run_process, a.process_ops)):
                stmts = gen_workload(seed, nops, a.no_null, a.no_float_agg)
                if mode == "repl":
                    o_out, o_db = runner(a.old, os.path.join(wd, "old.db"), stmts)
                    n_out, n_db = runner(a.new, os.path.join(wd, "new.db"), stmts)
                    o_db, n_db = body(o_db), body(n_db)
                else:
                    r_out, o_db = run_repl(a.new, os.path.join(wd, "old.db"), stmts)
                    o_out = "".join("> " + st + "\n" + chunk for st, chunk in zip(stmts, repl_chunks(r_out, len(stmts))))
                    n_out, n_db = runner(a.new, os.path.join(wd, "new.db"), stmts)
                checks += 1
                if o_out != n_out:
                    line, x, y = first_diff(o_out, n_out)
                    print("FAIL seed=%d mode=%s stdout differs at line %d\n  old: %s\n  new: %s" % (seed, mode, line, x, y))
                    failures += 1
                elif o_db != n_db:
                    print("FAIL seed=%d mode=%s final .db differs (%d vs %d bytes)" % (seed, mode, len(o_db), len(n_db)))
                    failures += 1
                else:
                    print("PASS seed=%d mode=%s (%d statements, %d output lines, %d-byte .db)"
                          % (seed, mode, len(stmts), o_out.count("\n"), len(o_db)))
    print("DIFFERENTIAL: %d checks, %d failures" % (checks, failures))
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
