#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Crash-recovery test for qdb's append-only WAL.

A REPL session that ends in a statement qdb rejects exits through
process_exit(1), skipping the clean-exit checkpoint. That leaves the WAL as a
crash would: committed records not yet folded into the .db, plus an open
transaction. The test then truncates that WAL at every byte offset (a torn
write), restores the .db, reopens qdb, and checks the recovered rows against
a reference model of the recovery rule:

  a transaction's OP lines are applied if and only if its whole COMMIT
  record (COMMIT and its 8-digit hash, bdb/COMPLETE.md P15b) is present in
  the surviving prefix; nothing else is applied.

Cases:
  small     a few autocommit INSERTs, one committed BEGIN block, one open
            BEGIN block; every byte offset of the WAL.
  large     enough rows that a mid-load checkpoint rewrites the .db, so the
            recovered state is .db rows plus WAL rows; 200 random offsets.

With --old OLD_QDB the small case's truncated WALs also go through the old
build, whose recovery output must match the new build's exactly, except at
offsets that cut inside a COMMIT record: the old build took any line starting
with COMMIT as a commit, which P15b removed.

  python test_qdb_wal_recovery.py NEW_QDB [--old OLD_QDB]
"""
import argparse
import os
import random
import re
import shutil
import subprocess
import sys
import tempfile

ROW_RE = re.compile(r"INSERT INTO t VALUES \((\d+),")
COMMIT_RE = re.compile(r"COMMIT [0-9a-f]{8}")


def whole_commit(line):
    return COMMIT_RE.fullmatch(line) is not None


def torn_commit(prefix):
    """The prefix ends inside a COMMIT record."""
    last = prefix.rsplit(b"\n", 1)[-1].decode(errors="replace")
    return last.startswith("COMMIT") and not whole_commit(last)


def session(qdb, db, stmts):
    """Run stmts in one REPL, then a rejected statement so qdb exits with 1."""
    script = "\n".join(stmts) + "\nNOT_A_STATEMENT\n"
    p = subprocess.run([qdb, db], input=script.encode(), capture_output=True, timeout=600)
    if p.returncode != 1:
        raise SystemExit("setup: expected exit 1 from the rejected statement, got %d" % p.returncode)


def expected_ids(db_bytes, wal_bytes):
    """Reference model: rows in the .db plus rows from committed WAL records."""
    ids, table = [], False
    for line in db_bytes.decode().replace("\r", "").split("\n"):
        if line == "TABLE t":
            table = True
        if line.startswith("ROW "):
            ids.append(int(line[4:].split("|")[0]))
    pending, open_txn = [], False
    for line in wal_bytes.decode(errors="replace").replace("\r", "").split("\n"):
        if line.startswith("TXID "):
            pending, open_txn = [], True
        elif line.startswith("COMMIT"):
            if open_txn and whole_commit(line):
                for op in pending:
                    if op.startswith("CREATE TABLE t"):
                        table = True
                    m = ROW_RE.match(op)
                    if m:
                        ids.append(int(m.group(1)))
            pending, open_txn = [], False
        elif line.startswith("OP ") and open_txn:
            pending.append(line[3:])
    return table, sorted(ids)


def recover(qdb, db, db_bytes, wal_bytes):
    for p in (db, db + ".wal"):
        if os.path.exists(p):
            os.remove(p)
    if db_bytes is not None:
        with open(db, "wb") as f:
            f.write(db_bytes)
    with open(db + ".wal", "wb") as f:
        f.write(wal_bytes)
    p = subprocess.run([qdb, db, "SELECT id FROM t ORDER BY id"], capture_output=True, timeout=120)
    out = p.stdout.decode(errors="replace").replace("\r", "")
    return p.returncode, out


def parse_ids(out):
    if "not found" in out:
        return False, []
    rows = [l.strip() for l in out.split("\n")[2:] if l.strip()]
    return True, sorted(int(r) for r in rows)


def run_case(name, qdb, old, stmts, offsets_fn, wd):
    src = os.path.join(wd, name + "_src.db")
    session(qdb, src, stmts)
    wal = open(src + ".wal", "rb").read()
    db_bytes = open(src, "rb").read() if os.path.exists(src) else None
    offsets = offsets_fn(len(wal))
    work = os.path.join(wd, name + ".db")
    fails = 0
    for cut in offsets:
        prefix = wal[:cut]
        want = expected_ids(db_bytes or b"", prefix)
        rc, out = recover(qdb, work, db_bytes, prefix)
        got = parse_ids(out)
        if rc != 0 or got != want:
            fails += 1
            if fails <= 5:
                extra = sorted(set(got[1]) - set(want[1]))[:5]
                missing = sorted(set(want[1]) - set(got[1]))[:5]
                print("FAIL %s cut=%d/%d rc=%d table want/got=%s/%s rows want/got=%d/%d extra=%s missing=%s"
                      % (name, cut, len(wal), rc, want[0], got[0], len(want[1]), len(got[1]), extra, missing))
        if old and not torn_commit(prefix):
            orc, oout = recover(old, work, db_bytes, prefix)
            if (orc, oout) != (rc, out):
                fails += 1
                if fails <= 5:
                    print("FAIL %s cut=%d old/new recovery output differs" % (name, cut))
    print("%s %s: %d truncation points, WAL %d bytes, .db %s, %d failures"
          % ("PASS" if fails == 0 else "FAIL", name, len(offsets), len(wal),
             "%d bytes" % len(db_bytes) if db_bytes is not None else "absent", fails))
    return fails, wal, db_bytes


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("new")
    ap.add_argument("--old")
    a = ap.parse_args()
    if not os.path.isfile(a.new):
        print("SKIP: %s not found" % a.new)
        return 0
    total = 0
    with tempfile.TemporaryDirectory() as wd:
        small = ["CREATE TABLE t (id INTEGER, v TEXT)",
                 "INSERT INTO t VALUES (1, 'a')",
                 "INSERT INTO t VALUES (2, 'b')",
                 "BEGIN", "INSERT INTO t VALUES (3, 'c')", "INSERT INTO t VALUES (4, 'd')", "COMMIT",
                 "INSERT INTO t VALUES (5, 'e')",
                 "BEGIN", "INSERT INTO t VALUES (6, 'f')"]
        f, wal, _ = run_case("small", a.new, a.old, small, lambda n: range(n + 1), wd)
        total += f
        # Full WAL: rows 1-5 recover, the open transaction (row 6) does not.
        rc, out = recover(a.new, os.path.join(wd, "full.db"), None, wal)
        if parse_ids(out) != (True, [1, 2, 3, 4, 5]):
            print("FAIL full-WAL recovery: %r" % out)
            total += 1
        else:
            print("PASS full WAL recovers rows 1-5 and drops the open transaction")

        rnd = random.Random(7)
        large = ["CREATE TABLE t (id INTEGER, v TEXT)", "CREATE INDEX t_v ON t (v)"]
        large += ["INSERT INTO t VALUES (%d, 'value_%05d')" % (i, (i * 37) % 4000) for i in range(1, 4001)]
        large += ["BEGIN"] + ["INSERT INTO t VALUES (%d, 'open')" % i for i in range(4001, 4011)]
        # No old-build comparison here: the old replay is quadratic, and 200
        # replays of a 4,000-record WAL through it would take hours.
        f, _, db_bytes = run_case("large", a.new, None, large,
                                  lambda n: sorted(set([0, n] + [rnd.randint(0, n) for _ in range(200)])), wd)
        total += f
        if db_bytes is None:
            print("FAIL large: no mid-load checkpoint happened, so the .db+WAL path went untested")
            total += 1
    print("WAL RECOVERY: %d failures" % total)
    return 1 if total else 0


if __name__ == "__main__":
    sys.exit(main())
