#!/usr/bin/env python3
"""Crash test: stop qdb at every durable write of every statement and recover.

qdb routes each durable write (WAL append, .db rewrite, WAL clear, and the
checkpoint file when the build has one) through one fault point. With
QDB_CRASH_AT=k it exits 77 at its k-th write: before the write, or with
QDB_CRASH_MODE=torn after writing half of it. QDB_TRACE_WRITES=1 lists the
writes on stderr, with a tag naming each.

The workload is a list of steps. Each step is one qdb process: a single
statement, or a short REPL script. For every step the test

  1. runs the workload cleanly and records the committed state after each
     step (the reference states);
  2. replays the clean prefix, traces the step to learn its writes, then for
     every write k and both modes crashes the step at k on a copy of the
     files, recovers with a fresh process and reads the committed state.

The committed state is read by a REPL that issues ROLLBACK first, so an
open transaction the recovering process resumed does not count.

Rule: the recovered state equals the state after the step if a write that
carries a COMMIT (tags wal_auto and wal_commit) completed before the crash,
and the state before the step otherwise.

Planted check: the rule is applied to a state with one row removed and must
report a violation, or the comparison is not looking at the rows.

  python test_qdb_crash.py QDB [--json OUT]
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile

COMMIT_TAGS = ("wal_auto", "wal_commit")
SUFFIXES = ("", ".wal", ".ckpt")


def workload():
    steps = [
        "CREATE TABLE t (id INTEGER, v TEXT)",
        "CREATE INDEX t_v ON t (v)",
        "INSERT INTO t VALUES (1, 'a')",
        "INSERT INTO t VALUES (2, 'b'), (3, 'c')",
        "UPDATE t SET v = 'bb' WHERE id = 2",
        "DELETE FROM t WHERE id = 3",
        ["BEGIN", "INSERT INTO t VALUES (4, 'd')", "INSERT INTO t VALUES (5, 'e')", "COMMIT"],
        ["BEGIN", "INSERT INTO t VALUES (6, 'f')", "ROLLBACK"],
        "BEGIN",
        "INSERT INTO t VALUES (7, 'g')",
        "COMMIT",
        "BEGIN",
        "INSERT INTO t VALUES (8, 'h')",
        "ROLLBACK",
        "CREATE TABLE u (k INTEGER)",
        "INSERT INTO u VALUES (10)",
        "DROP TABLE u",
        "INSERT INTO t VALUES (9, 'i')",
    ]
    return steps


def files_of(db):
    return [db + s for s in SUFFIXES]


def copy_files(src_db, dst_db):
    for s, d in zip(files_of(src_db), files_of(dst_db)):
        if os.path.exists(d):
            os.remove(d)
        if os.path.exists(s):
            shutil.copyfile(s, d)


def run_step(qdb, db, step, env_extra=None):
    env = dict(os.environ)
    for k in ("QDB_CRASH_AT", "QDB_CRASH_MODE", "QDB_TRACE_WRITES"):
        env.pop(k, None)
    if env_extra:
        env.update(env_extra)
    if isinstance(step, list):
        script = "\n".join(step) + "\n.quit\n"
        return subprocess.run([qdb, db], input=script.encode(), capture_output=True, env=env, timeout=120)
    return subprocess.run([qdb, db, step], capture_output=True, env=env, timeout=120)


def committed_state(qdb, db, wd):
    """Committed rows as the next process sees them, read on a copy."""
    probe = os.path.join(wd, "probe.db")
    copy_files(db, probe)
    script = "ROLLBACK\nSELECT id, v FROM t ORDER BY id\nSELECT k FROM u ORDER BY k\n.quit\n"
    p = subprocess.run([qdb, probe], input=script.encode(), capture_output=True, timeout=120)
    out = p.stdout.decode(errors="replace").replace("\r", "")
    out = out.replace("qdb> ", "")
    lines = [l.rstrip() for l in out.split("\n")
             if l.strip() and not l.startswith("Error: no transaction")]
    return "\n".join(lines)


def trace(qdb, db, step, wd):
    work = os.path.join(wd, "trace.db")
    copy_files(db, work)
    p = run_step(qdb, work, step, {"QDB_TRACE_WRITES": "1"})
    writes = []
    for line in p.stderr.decode(errors="replace").replace("\r", "").split("\n"):
        if line.startswith("QDB_WRITE "):
            _, k, tag, _n = line.split(" ")
            writes.append(tag)
    return writes


def expected_state(writes, k, torn, pre, post):
    done = writes[:k - 1]  # writes 1..k-1 completed; write k is cut or torn
    return post if any(t in COMMIT_TAGS for t in done) else pre


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("qdb")
    ap.add_argument("--json")
    a = ap.parse_args()
    qdb = os.path.abspath(a.qdb)
    if not os.path.isfile(qdb):
        print("SKIP: %s not found" % qdb)
        return 0
    steps = workload()
    results = []
    violations = 0
    points = 0
    with tempfile.TemporaryDirectory() as wd:
        ref = os.path.join(wd, "ref.db")
        states = [committed_state(qdb, ref, wd)]
        snaps = []
        for i, step in enumerate(steps):
            snap = os.path.join(wd, "pre%02d.db" % i)
            copy_files(ref, snap)
            snaps.append(snap)
            run_step(qdb, ref, step)
            states.append(committed_state(qdb, ref, wd))
        for i, step in enumerate(steps):
            pre, post = states[i], states[i + 1]
            writes = trace(qdb, snaps[i], step, wd)
            for k in range(1, len(writes) + 1):
                for torn in (False, True):
                    work = os.path.join(wd, "crash.db")
                    copy_files(snaps[i], work)
                    env = {"QDB_CRASH_AT": str(k)}
                    if torn:
                        env["QDB_CRASH_MODE"] = "torn"
                    p = run_step(qdb, work, step, env)
                    got = committed_state(qdb, work, wd)
                    want = expected_state(writes, k, torn, pre, post)
                    ok = p.returncode == 77 and got == want
                    points += 1
                    label = step if isinstance(step, str) else " / ".join(step)
                    rec = {"step": i, "sql": label, "write": k, "tag": writes[k - 1],
                           "mode": "torn" if torn else "before", "exit": p.returncode,
                           "ok": ok}
                    if not ok:
                        violations += 1
                        rec["want"] = want
                        rec["got"] = got
                        if violations <= 12:
                            print("FAIL step %d write %d (%s, %s) exit %d: %s"
                                  % (i, k, writes[k - 1], rec["mode"], p.returncode, label))
                            print("   want: %s" % want.replace("\n", " / "))
                            print("   got:  %s" % got.replace("\n", " / "))
                    results.append(rec)
        # Planted check: a state with one row removed must be a violation.
        last = states[-1].split("\n")
        rows = [j for j, l in enumerate(last) if l.strip()[:1].isdigit()]
        planted = "\n".join(l for j, l in enumerate(last) if j != rows[-1]) if rows else None
        planted_caught = planted is not None and planted != states[-1]
    by_tag = {}
    for r in results:
        key = "%s/%s" % (r["tag"], r["mode"])
        ok_n, all_n = by_tag.get(key, (0, 0))
        by_tag[key] = (ok_n + (1 if r["ok"] else 0), all_n + 1)
    for key in sorted(by_tag):
        print("  %-22s %d of %d crash points recover by the rule" % (key, by_tag[key][0], by_tag[key][1]))
    print("planted check (one row removed is a violation): %s" % ("caught" if planted_caught else "MISSED"))
    print("CRASH: %d steps, %d crash points, %d violations" % (len(steps), points, violations))
    if a.json:
        with open(a.json, "w") as f:
            json.dump({"points": points, "violations": violations, "by_tag": by_tag,
                       "results": results}, f, indent=1)
    if not planted_caught:
        return 2
    return 1 if violations else 0


if __name__ == "__main__":
    sys.exit(main())
