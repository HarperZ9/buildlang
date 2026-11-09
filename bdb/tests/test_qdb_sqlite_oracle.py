#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Differential test of qdb against SQLite on generated queries in qdb's dialect.

Each seed builds two tables (40 rows in `a`, 20 in `b`) and 200 SELECTs drawn
from the dialect qdb documents in bdb/README.md: WHERE trees with AND, OR,
NOT, IN, LIKE, BETWEEN and IS NULL; ORDER BY on a total order; LIMIT and
OFFSET; DISTINCT; COUNT, SUM, MIN, MAX and AVG; GROUP BY with HAVING; INNER
and LEFT JOIN. The same statements run through qdb (one process per
statement) and Python's sqlite3, and the rows are compared:

  text     every rendered cell equal as text (SQL NULL renders as NULL)
  numeric  as text, except that two cells that both parse as numbers are
           equal when they agree to 1e-9 relative

A query without ORDER BY is compared as a multiset of rows. Every
disagreement is put in one class: rejected (qdb printed an error),
null_vs_empty (the rows differ only where SQLite has NULL and qdb an empty
cell, or the reverse), number_text (equal under numeric, not under text),
rows (anything else).

Planted check: for the first query that agrees, the harness alters one cell
of qdb's answer and must count a disagreement.

  python test_qdb_sqlite_oracle.py QDB [--seeds 1,...,10] [--json OUT]
"""
import argparse
import json
import os
import random
import sqlite3
import subprocess
import sys
import tempfile

NAMES = ["ann", "bob", "cy", "dee", "ed", "flo", "gus", "o''neil"]


def gen(seed):
    rnd = random.Random(seed)
    setup = ["CREATE TABLE a (id INTEGER, name TEXT, score REAL, grp INTEGER)",
             "CREATE TABLE b (id INTEGER, aid INTEGER, tag TEXT)"]
    for i in range(1, 41):
        r = rnd.random()
        name = "NULL" if r < 0.05 else ("''" if r < 0.10 else "'%s'" % rnd.choice(NAMES))
        score = "NULL" if rnd.random() < 0.05 else "%.1f" % rnd.uniform(-10, 90)
        setup.append("INSERT INTO a VALUES (%d, %s, %s, %d)" % (i, name, score, rnd.randint(1, 5)))
    for i in range(1, 21):
        setup.append("INSERT INTO b VALUES (%d, %d, '%s')" % (i, rnd.randint(1, 45), rnd.choice("xyz")))

    def cond():
        k = rnd.randint(0, 9)
        if k == 0:
            return "score > %d" % rnd.randint(-5, 80)
        if k == 1:
            return "grp = %d" % rnd.randint(1, 5)
        if k == 2:
            return "name = '%s'" % rnd.choice(NAMES)
        if k == 3:
            return "name LIKE '%s%%'" % rnd.choice("abcdefgo")
        if k == 4:
            return "grp IN (%d, %d)" % (rnd.randint(1, 5), rnd.randint(1, 5))
        if k == 5:
            lo = rnd.randint(-10, 60)
            return "score BETWEEN %d AND %d" % (lo, lo + rnd.randint(5, 40))
        if k == 6:
            return "name IS NULL"
        if k == 7:
            return "score IS NOT NULL"
        if k == 8:
            return "id <= %d" % rnd.randint(1, 40)
        return "id != %d" % rnd.randint(1, 40)

    def where():
        k = rnd.randint(0, 4)
        if k == 0:
            return cond()
        if k == 1:
            return "%s AND %s" % (cond(), cond())
        if k == 2:
            return "%s OR %s" % (cond(), cond())
        if k == 3:
            return "NOT %s" % cond()
        return "(%s OR %s) AND %s" % (cond(), cond(), cond())

    qs = []
    for _ in range(200):
        k = rnd.randint(0, 9)
        if k <= 2:
            q = "SELECT id, name, score FROM a WHERE %s ORDER BY id" % where()
            if rnd.random() < 0.3:
                q += " LIMIT %d" % rnd.randint(0, 8)
                if rnd.random() < 0.5:
                    q += " OFFSET %d" % rnd.randint(0, 5)
            qs.append((q, True))
        elif k == 3:
            qs.append(("SELECT DISTINCT grp FROM a WHERE %s" % where(), False))
        elif k == 4:
            f = rnd.choice(["COUNT(*)", "SUM(score)", "MIN(score)", "MAX(score)", "AVG(score)",
                            "COUNT(name)", "MIN(name)", "MAX(name)"])
            qs.append(("SELECT %s FROM a WHERE %s" % (f, where()), False))
        elif k == 5:
            f = rnd.choice(["COUNT(*)", "SUM(score)", "MAX(score)", "MIN(id)", "AVG(score)"])
            q = "SELECT grp, %s FROM a GROUP BY grp" % f
            if rnd.random() < 0.4:
                q += " HAVING COUNT(*) > %d" % rnd.randint(1, 9)
            q += " ORDER BY grp"
            qs.append((q, True))
        elif k == 6:
            qs.append(("SELECT a.id, b.tag FROM a JOIN b ON a.id = b.aid ORDER BY a.id, b.tag", False))
        elif k == 7:
            qs.append(("SELECT a.id, b.tag FROM a LEFT JOIN b ON a.id = b.aid WHERE a.grp = %d ORDER BY a.id"
                       % rnd.randint(1, 5), False))
        elif k == 8:
            qs.append(("SELECT name, score FROM a WHERE %s ORDER BY score DESC, id" % where(), False))
        else:
            qs.append(("SELECT id FROM a WHERE id IN (SELECT aid FROM b) ORDER BY id", True))
    return setup, qs


def qdb_run(exe, db, sql):
    p = subprocess.run([exe, db, sql], capture_output=True, timeout=60)
    out = p.stdout.decode(errors="replace").replace("\r", "")
    lines = out.split("\n")
    if p.returncode != 0 or any(l.startswith("Error") for l in lines) or len(lines) < 2:
        return None, out
    rows = []
    for line in lines[2:]:
        if line == "":
            continue
        rows.append(tuple(c.strip() for c in line.split(" | ")))
    return rows, out


def sqlite_render(v):
    if v is None:
        return "NULL"
    if isinstance(v, float) and v == int(v) and abs(v) < 1e15:
        return repr(v)
    return str(v)


def num(s):
    try:
        return float(s)
    except (TypeError, ValueError):
        return None


def cells_equal(x, y, numeric):
    if x == y:
        return True
    if numeric:
        a, b = num(x), num(y)
        if a is not None and b is not None:
            return abs(a - b) <= 1e-9 * max(1.0, abs(a), abs(b))
    return False


def rows_equal(q, s, numeric, ordered):
    if len(q) != len(s):
        return False
    if not ordered:
        key = lambda r: tuple(str(c) for c in r)
        q, s = sorted(q, key=key), sorted(s, key=key)
    for rq, rs in zip(q, s):
        if len(rq) != len(rs):
            return False
        if not all(cells_equal(x, y, numeric) for x, y in zip(rq, rs)):
            return False
    return True


def classify(qrows, srows, ordered):
    if qrows is None:
        return "rejected"
    if rows_equal(qrows, srows, False, ordered):
        return "agree_text"
    if rows_equal(qrows, srows, True, ordered):
        return "number_text"
    blank = [tuple("" if c == "NULL" else c for c in r) for r in srows]
    if rows_equal(qrows, blank, True, ordered):
        return "null_vs_empty"
    return "rows"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("qdb")
    ap.add_argument("--seeds", default="1,2,3,4,5,6,7,8,9,10")
    ap.add_argument("--json")
    a = ap.parse_args()
    exe = os.path.abspath(a.qdb)
    if not os.path.isfile(exe):
        print("SKIP: %s not found" % exe)
        return 0
    counts = {}
    examples = {}
    planted = None
    total = 0
    with tempfile.TemporaryDirectory() as wd:
        for seed in [int(s) for s in a.seeds.split(",")]:
            setup, qs = gen(seed)
            db = os.path.join(wd, "s%d.db" % seed)
            con = sqlite3.connect(":memory:")
            for st in setup:
                con.execute(st)
                r, out = qdb_run(exe, db, st)
                if r is None and "Error" in out:
                    print("setup failed on seed %d: %s: %s" % (seed, st, out.strip()))
            for sql, _ in qs:
                # A query ordered by a non-unique key is compared as a multiset.
                ordered = "ORDER BY id" in sql or "ORDER BY grp" in sql
                srows = [tuple(sqlite_render(v) for v in r) for r in con.execute(sql).fetchall()]
                qrows, out = qdb_run(exe, db, sql)
                cls = classify(qrows, srows, ordered)
                total += 1
                counts[cls] = counts.get(cls, 0) + 1
                if cls not in ("agree_text",) and len(examples.setdefault(cls, [])) < 4:
                    examples[cls].append({"seed": seed, "sql": sql, "qdb": (qrows or out)[:6] if qrows else out[:200],
                                          "sqlite": srows[:6]})
                if planted is None and cls == "agree_text" and qrows:
                    bad = [list(r) for r in qrows]
                    bad[0][0] = "planted-" + bad[0][0]
                    planted = classify([tuple(r) for r in bad], srows, ordered) != "agree_text"
    agree_text = counts.get("agree_text", 0)
    agree_num = agree_text + counts.get("number_text", 0)
    for cls in sorted(counts):
        print("  %-14s %4d" % (cls, counts[cls]))
    print("SQLITE ORACLE: %d queries; %d agree as text (%.1f%%), %d agree with numbers compared as numbers (%.1f%%)"
          % (total, agree_text, 100.0 * agree_text / max(1, total), agree_num, 100.0 * agree_num / max(1, total)))
    print("planted check (one altered cell is a disagreement): %s" % ("caught" if planted else "MISSED"))
    for cls, ex in examples.items():
        for e in ex[:2]:
            print("  e.g. %s seed %d: %s" % (cls, e["seed"], e["sql"]))
            print("       qdb:    %s" % (e["qdb"],))
            print("       sqlite: %s" % (e["sqlite"],))
    if a.json:
        with open(a.json, "w") as f:
            json.dump({"total": total, "counts": counts, "examples": examples, "planted": planted}, f, indent=1)
    if not planted:
        return 2
    return 0 if agree_num == total else 1


if __name__ == "__main__":
    sys.exit(main())
