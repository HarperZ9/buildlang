#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Text round trip: values with |, backslash, line breaks and quotes survive.

1,000 generated text values, each holding at least one of: a pipe, a
backslash, a line feed, a carriage return, a single quote, a leading or
trailing space. Every value is inserted with its id, then read back two ways:

  checkpoint  the inserting process exits cleanly, so the rows reach the
              .db file; a fresh process reads them from the .db
  wal         one INSERT of every row, stopped (QDB_CRASH_AT=2) at the
              checkpoint, after the WAL append, so the rows survive only in
              the WAL; a fresh process replays them

The read-back is qdb's own output for `SELECT v FROM t WHERE id = N`, which
renders the cell between a header and a separator line, so each value is
checked by id with a query of its own. A value counts as kept when the
rendered cell equals the inserted text.

Planted check: the comparison is run once against a value with one character
changed and must report it as changed.

  python test_qdb_text.py QDB [--n 1000]
"""
import argparse
import os
import random
import subprocess
import sys
import tempfile

SPECIALS = ["|", "\\", "\n", "\r", "'", " "]


def gen(n, seed=11):
    rnd = random.Random(seed)
    alphabet = "abcxyz019 ,.;-_" + "".join(SPECIALS)
    vals = []
    for i in range(n):
        s = "".join(rnd.choice(alphabet) for _ in range(rnd.randint(1, 12)))
        s = s.replace(chr(13) + chr(10), chr(10))
        if not any(c in s for c in SPECIALS[:4]):
            s += rnd.choice(SPECIALS[:4])
        if rnd.random() < 0.1:
            s = " " + s + " "
        vals.append(s)
    return vals


def sql_quote(s):
    return "'" + s.replace("'", "''") + "'"


def read_cell(qdb, db, i):
    p = subprocess.run([qdb, db, "SELECT v FROM t WHERE id = %d" % i], capture_output=True, timeout=60)
    out = p.stdout.decode("utf-8", errors="replace")
    # The Windows C runtime writes each line feed as CR LF. The generated
    # values hold no CR LF pair, so this undoes exactly that translation. On
    # other systems nothing is translated, and a value ending in CR must keep it.
    if os.name == "nt":
        out = out.replace(chr(13) + chr(10), chr(10))
    # Header "v...", separator "-...", then the padded cell; print adds "\n".
    first = out.find("\n")
    second = out.find("\n", first + 1) if first >= 0 else -1
    if second < 0:
        return None  # no table printed: the row or the table is gone
    body = out[second + 1:]
    if body.endswith("\n"):
        body = body[:-1]
    return body


def same(got, want):
    # A cell is padded to the header width, 1 for "v", so only an empty
    # value is padded (to one space). Trailing spaces are compared exactly.
    if got is None:
        return False
    return got == want or (want == "" and got == " ")


def run(qdb, vals, mode, wd):
    db = os.path.join(wd, "text_%s.db" % mode)
    for suf in ("", ".wal", ".ckpt"):
        if os.path.exists(db + suf):
            os.remove(db + suf)
    subprocess.run([qdb, db, "CREATE TABLE t (id INTEGER, v TEXT)"], capture_output=True, timeout=60)
    # The WAL case sends every row in one statement, so its one WAL record
    # is the only copy: the process stops at write 2, the checkpoint.
    chunk = 50 if mode == "checkpoint" else len(vals)
    for start in range(0, len(vals), chunk):
        rows = ", ".join("(%d, %s)" % (start + j, sql_quote(v)) for j, v in enumerate(vals[start:start + chunk]))
        env = dict(os.environ)
        if mode == "wal":
            env["QDB_CRASH_AT"] = "2"  # write 1 is the WAL append; stop at the checkpoint
        subprocess.run([qdb, db, "INSERT INTO t VALUES " + rows], capture_output=True, env=env, timeout=60)
    kept = 0
    bad = []
    for i, v in enumerate(vals):
        got = read_cell(qdb, db, i)
        if same(got, v):
            kept += 1
        elif len(bad) < 5:
            bad.append((i, v, got))
    return kept, bad


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("qdb")
    ap.add_argument("--n", type=int, default=1000)
    a = ap.parse_args()
    qdb = os.path.abspath(a.qdb)
    if not os.path.isfile(qdb):
        print("SKIP: %s not found" % qdb)
        return 0
    vals = gen(a.n)
    fails = 0
    with tempfile.TemporaryDirectory() as wd:
        for mode in ("checkpoint", "wal"):
            kept, bad = run(qdb, vals, mode, wd)
            print("%s %s: %d of %d values kept" % ("PASS" if kept == len(vals) else "FAIL", mode, kept, len(vals)))
            for i, v, got in bad:
                print("   id %d want %r got %r" % (i, v, got))
            fails += len(vals) - kept
    planted = not same(vals[0][:-1] + ("x" if vals[0][-1:] != "x" else "y"), vals[0])
    print("planted check (one changed character is caught): %s" % ("caught" if planted else "MISSED"))
    print("TEXT ROUND TRIP: %d values changed" % fails)
    if not planted:
        return 2
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())
