#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""INSERT, UPDATE and DELETE on indexed tables, qdb against SQLite (bdb/COMPLETE.md P20).

Each seed builds three tables in the order a, b, c, so a DELETE in `a` moves
the cells of `b` and `c`, with indexes on a(k), a(s), a(id), b(k) and c(id).
It then runs a script of INSERT, UPDATE and DELETE statements, some inside
BEGIN ... COMMIT or BEGIN ... ROLLBACK, with reads in between that go through
the indexes (equality and range on an indexed column) and reads that scan.
The script runs through one qdb process (its REPL on stdin) and through
Python's sqlite3. Every read is compared as a multiset of rendered rows.

Planted check: the harness alters one cell of the first qdb answer that agrees
and must count a disagreement.

  python test_qdb_mutations.py QDB [--seeds 1,...,20] [--ops 300]
"""
import argparse
import os
import random
import sqlite3
import subprocess
import sys
import tempfile

WORDS = ["ash", "elm", "fir", "oak", "yew", "o''hara"]


def gen(seed, ops):
    rnd = random.Random(seed)
    setup = ["CREATE TABLE a (id INTEGER, k INTEGER, s TEXT)",
             "CREATE TABLE b (id INTEGER, k INTEGER)",
             "CREATE TABLE c (id INTEGER, w TEXT)",
             "CREATE INDEX a_k ON a (k)", "CREATE INDEX a_s ON a (s)",
             "CREATE INDEX a_id ON a (id)", "CREATE INDEX b_k ON b (k)",
             "CREATE INDEX c_id ON c (id)"]
    nid = {"a": 0, "b": 0, "c": 0}

    def word():
        r = rnd.random()
        if r < 0.08:
            return "NULL"
        return "'%s'" % rnd.choice(WORDS)

    def ins(t):
        rows = []
        for _ in range(rnd.randint(1, 4)):
            nid[t] += 1
            if t == "a":
                rows.append("(%d, %d, %s)" % (nid[t], rnd.randint(0, 30), word()))
            elif t == "b":
                rows.append("(%d, %d)" % (nid[t], rnd.randint(0, 20)))
            else:
                rows.append("(%d, %s)" % (nid[t], word()))
        return "INSERT INTO %s VALUES %s" % (t, ", ".join(rows))

    for t in ("a", "b", "c"):
        for _ in range(8):
            setup.append(ins(t))

    script = []
    in_txn = False
    for _ in range(ops):
        x = rnd.random()
        if x < 0.16:
            script.append(("w", ins(rnd.choice("abc"))))
        elif x < 0.24:
            script.append(("w", "UPDATE a SET k = %d WHERE id = %d" % (rnd.randint(0, 30), rnd.randint(1, nid["a"]))))
        elif x < 0.30:
            script.append(("w", "UPDATE a SET k = k %s %d WHERE k = %d" % (rnd.choice("+-"), rnd.randint(0, 5), rnd.randint(0, 30))))
        elif x < 0.34:
            script.append(("w", "UPDATE a SET k = k - 1 WHERE k > %d" % rnd.randint(0, 30)))
        elif x < 0.40:
            script.append(("w", "UPDATE a SET s = %s WHERE k = %d" % (word(), rnd.randint(0, 30))))
        elif x < 0.44:
            script.append(("w", "UPDATE b SET k = %d WHERE id = %d" % (rnd.randint(0, 20), rnd.randint(1, nid["b"]))))
        elif x < 0.47:
            script.append(("w", "UPDATE c SET id = %d WHERE id = %d" % (rnd.randint(1, nid["c"] + 5), rnd.randint(1, nid["c"]))))
        elif x < 0.52:
            script.append(("w", "DELETE FROM a WHERE k = %d" % rnd.randint(0, 30)))
        elif x < 0.55:
            script.append(("w", "DELETE FROM a WHERE id = %d" % rnd.randint(1, nid["a"])))
        elif x < 0.58:
            script.append(("w", "DELETE FROM b WHERE k > %d" % rnd.randint(10, 20)))
        elif x < 0.61:
            script.append(("w", "DELETE FROM c WHERE id = %d" % rnd.randint(1, nid["c"])))
        elif x < 0.65:
            if in_txn:
                script.append(("w", rnd.choice(["COMMIT", "ROLLBACK"])))
            else:
                script.append(("w", "BEGIN"))
            in_txn = not in_txn
        elif x < 0.73:
            script.append(("r", "SELECT id, k, s FROM a WHERE k = %d" % rnd.randint(0, 30)))
        elif x < 0.77:
            script.append(("r", "SELECT id, k, s FROM a WHERE k > %d" % rnd.randint(0, 30)))
        elif x < 0.81:
            script.append(("r", "SELECT id, k FROM a WHERE s = %s" % ("'%s'" % rnd.choice(WORDS))))
        elif x < 0.85:
            script.append(("r", "SELECT id, k FROM b WHERE k = %d" % rnd.randint(0, 20)))
        elif x < 0.88:
            script.append(("r", "SELECT id, k FROM b WHERE k < %d" % rnd.randint(0, 20)))
        elif x < 0.92:
            script.append(("r", "SELECT id, w FROM c WHERE id = %d" % rnd.randint(1, nid["c"] + 5)))
        elif x < 0.96:
            script.append(("r", "SELECT id, k, s FROM a WHERE id = %d" % rnd.randint(1, nid["a"])))
        else:
            script.append(("r", "SELECT * FROM %s" % rnd.choice("abc")))
    if in_txn:
        script.append(("w", "COMMIT"))
    for t in ("a", "b", "c"):
        script.append(("r", "SELECT * FROM %s" % t))
    return setup, script


def qdb_answers(exe, setup, script, wd):
    db = os.path.join(wd, "m.db")
    for s in ("", ".wal", ".tmp", ".hist"):
        if os.path.exists(db + s):
            os.remove(db + s)
    stmts = setup + [s for _, s in script]
    p = subprocess.run([exe, db], input=("\n".join(stmts) + "\n.quit\n").encode(),
                       capture_output=True, timeout=600)
    out = p.stdout.decode(errors="replace").replace("\r", "")
    chunks = out.split("qdb> ")[1:]
    if len(chunks) < len(stmts):
        raise SystemExit("qdb printed %d prompts for %d statements (exit %d):\n%s"
                         % (len(chunks), len(stmts), p.returncode, out[-2000:]))
    chunks = chunks[len(setup):]
    answers = []
    for (kind, sql), chunk in zip(script, chunks):
        if kind != "r":
            if "Error" in chunk:
                answers.append(("error", chunk.strip()))
            continue
        lines = chunk.split("\n")
        if any(l.startswith("Error") for l in lines):
            answers.append(("error", chunk.strip()))
            continue
        rows = []
        for line in lines[2:]:
            if line.strip() == "":
                continue
            rows.append(tuple(c.strip() for c in line.split(" | ")))
        answers.append(("rows", sorted(rows)))
    return answers


def render(v):
    if v is None:
        return "NULL"
    return str(v)


def sqlite_answers(setup, script):
    con = sqlite3.connect(":memory:", isolation_level=None)
    for s in setup:
        con.execute(s)
    answers = []
    for kind, sql in script:
        if kind != "r":
            try:
                con.execute(sql)
            except sqlite3.Error as e:
                answers.append(("error", str(e)))
            continue
        rows = [tuple(render(v) for v in r) for r in con.execute(sql).fetchall()]
        answers.append(("rows", sorted(rows)))
    return answers


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("qdb")
    ap.add_argument("--seeds", default=",".join(str(i) for i in range(1, 21)))
    ap.add_argument("--ops", type=int, default=300)
    a = ap.parse_args()
    exe = os.path.abspath(a.qdb)
    reads = bad = 0
    planted = None
    with tempfile.TemporaryDirectory() as wd:
        for seed in [int(s) for s in a.seeds.split(",")]:
            setup, script = gen(seed, a.ops)
            q = qdb_answers(exe, setup, script, wd)
            s = sqlite_answers(setup, script)
            reads_here = [sql for kind, sql in script if kind == "r"]
            if len(q) != len(s):
                print("seed %d: %d qdb answers, %d sqlite answers (an error on one side only)"
                      % (seed, len(q), len(s)))
                print("  qdb errors: %s" % [x for x in q if x[0] == "error"][:3])
                print("  sqlite errors: %s" % [x for x in s if x[0] == "error"][:3])
                bad += 1
                continue
            for i, (x, y) in enumerate(zip(q, s)):
                reads += 1
                if x != y:
                    bad += 1
                    if bad <= 5:
                        print("seed %d, answer %d (%s):\n  qdb    %s\n  sqlite %s"
                              % (seed, i, reads_here[i] if i < len(reads_here) else "?", x[1][:8], y[1][:8]))
                elif planted is None and x[0] == "rows" and x[1]:
                    altered = [list(r) for r in x[1]]
                    altered[0][0] = altered[0][0] + "9"
                    planted = [tuple(r) for r in altered] != y[1]
    print("answers compared: %d, disagreements: %d" % (reads, bad))
    print("planted check (one altered cell counted as a disagreement): %s"
          % ("caught" if planted else "MISSED"))
    return 0 if bad == 0 and planted else 1


if __name__ == "__main__":
    sys.exit(main())
